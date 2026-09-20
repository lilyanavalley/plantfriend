// src/mqtt.rs – MQTT client with Home Assistant auto-discovery
//
// Supports:
//  • MQTT 3.1.1 and MQTT 5 (selected via the broker URI scheme)
//  • Plain and TLS-encrypted connections (mqtt:// vs mqtts://)
//  • Username/password authentication
//  • Certificate-based mutual TLS (mTLS) when client_cert + client_key are set
//  • Home Assistant MQTT discovery protocol for binary_sensor entities
//
// The HA discovery payload published to
//   <discovery_prefix>/binary_sensor/<device_id>/liquid_level/config
// makes the sensor appear automatically in HA without manual configuration.

use anyhow::{bail, Result};
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EventPayload, LwtConfiguration, MqttClientConfiguration, QoS,
};
use log::{error, info, warn};
use plantfriend_core::homeassistant::{
    liquid_level_discovery_payload, liquid_level_state_payload, DeviceMetadata,
};
use plantfriend_core::mqtt::{
    liquid_level_discovery_topic, AVAILABILITY_OFFLINE, AVAILABILITY_ONLINE,
};
use plantfriend_core::sensors::LiquidState;

use crate::config::{Config, TlsConfig};

// ── MQTT client wrapper ───────────────────────────────────────────────────────

/// Manages the MQTT connection and publishes sensor state + HA discovery.
pub struct MqttManager {
    client: EspMqttClient<'static>,
    state_topic: String,
    availability_topic: String,
    discovery_topic: String,
    discovery_payload: String,
}

impl MqttManager {
    /// Construct and connect the MQTT client.
    pub fn connect(config: &Config) -> Result<Self> {
        let mc = &config.mqtt;
        let ha = &config.ha;

        // ── Build MQTT client configuration ──────────────────────────────────
        let mut mqtt_cfg = MqttClientConfiguration {
            client_id: Some(mc.client_id),
            username: if mc.username.is_empty() {
                None
            } else {
                Some(mc.username)
            },
            password: if mc.password.is_empty() {
                None
            } else {
                Some(mc.password)
            },
            // Last-will testament: publish "offline" to availability topic on
            // ungraceful disconnect so HA marks the device unavailable.
            lwt: Some(LwtConfiguration {
                topic: ha.availability_topic,
                payload: AVAILABILITY_OFFLINE.as_bytes(),
                qos: QoS::AtLeastOnce,
                retain: true,
            }),
            keep_alive_interval: Some(std::time::Duration::from_secs(30)),
            ..Default::default()
        };

        // ── TLS configuration (optional) ──────────────────────────────────────
        if let Some(tls) = &config.mqtt.tls {
            apply_tls_config(&mut mqtt_cfg, tls)?;
        }

        // ── Connect ───────────────────────────────────────────────────────────
        info!("Connecting to MQTT broker: {}", mc.broker_uri);

        let client = EspMqttClient::new_cb(mc.broker_uri, &mqtt_cfg, move |event| {
            match event.payload() {
                EventPayload::Connected(_) => info!("MQTT connected"),
                EventPayload::Disconnected => warn!("MQTT disconnected"),
                EventPayload::Error(e) => error!("MQTT error: {:?}", e),
                _ => {}
            }
        })?;

        // ── Build topic strings ───────────────────────────────────────────────
        let state_topic = ha.state_topic.to_string();
        let availability_topic = ha.availability_topic.to_string();

        let discovery_topic = liquid_level_discovery_topic(ha.discovery_prefix, ha.device_id);

        // ── Build HA discovery payload ────────────────────────────────────────
        let discovery_payload = liquid_level_discovery_payload(
            DeviceMetadata {
                device_id: ha.device_id,
                device_name: ha.device_name,
                model: "XKC-Y25-NPN",
                manufacturer: "Hydrolevel / ESP32",
            },
            ha.state_topic,
            ha.availability_topic,
        )?;

        Ok(Self {
            client,
            state_topic,
            availability_topic,
            discovery_topic,
            discovery_payload,
        })
    }

    /// Publish the Home Assistant MQTT discovery payload (retained).
    pub fn publish_discovery(&mut self) -> Result<()> {
        info!("Publishing HA discovery to: {}", self.discovery_topic);
        self.client.enqueue(
            &self.discovery_topic,
            QoS::AtLeastOnce,
            true, // retained so HA picks it up after restart
            self.discovery_payload.as_bytes(),
        )?;
        Ok(())
    }

    /// Announce that the device is online (retained availability message).
    pub fn publish_online(&mut self) -> Result<()> {
        let topic = self.availability_topic.clone();
        self.client
            .enqueue(&topic, QoS::AtLeastOnce, true, AVAILABILITY_ONLINE.as_bytes())?;
        Ok(())
    }

    /// Publish the current liquid level state.
    pub fn publish_state(&mut self, state: LiquidState) -> Result<()> {
        let topic = self.state_topic.clone();
        let payload = liquid_level_state_payload(state).as_bytes();
        info!("Publishing state '{}' → {}", liquid_level_state_payload(state), topic);
        self.client
            .enqueue(&topic, QoS::AtLeastOnce, false, payload)?;
        Ok(())
    }
}

// ── TLS helper ────────────────────────────────────────────────────────────────

/// Apply optional TLS settings to the MQTT client configuration.
///
/// `MqttClientConfiguration` carries raw `*const u8` pointer fields
/// (server_certificate, client_certificate, private_key) that must live
/// at least as long as the configuration struct.  Since the certificate bytes
/// come from `&'static [u8]` slices embedded in the binary, this is safe.
fn apply_tls_config(cfg: &mut MqttClientConfiguration<'_>, tls: &TlsConfig) -> Result<()> {
    if let Some(ca) = tls.ca_cert {
        // esp-idf-svc expects a null-terminated PEM or a DER blob.
        // We store the raw bytes embedded by build.rs.
        cfg.server_certificate = Some(esp_idf_svc::tls::X509::pem_until_nul(ca));
    }

    match (tls.client_cert, tls.client_key) {
        (Some(cert), Some(key)) => {
            cfg.client_certificate = Some(esp_idf_svc::tls::X509::pem_until_nul(cert));
            cfg.private_key = Some(esp_idf_svc::tls::X509::pem_until_nul(key));
        }
        (None, None) => {}
        _ => bail!(
            "Both PLANTFRIEND_MQTT_CLIENT_CERT_PATH and PLANTFRIEND_MQTT_CLIENT_KEY_PATH \
             must be set together for mTLS"
        ),
    }

    Ok(())
}
