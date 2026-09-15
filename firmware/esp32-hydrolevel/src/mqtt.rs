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
use plantminder_core::sensors::LiquidState;
use serde::Serialize;

use crate::config::{Config, TlsConfig};

// ── Home Assistant discovery payload ─────────────────────────────────────────

/// HA MQTT discovery payload for a `binary_sensor` entity.
///
/// Only the fields used by the liquid-level sensor are present.  Additional
/// fields can be added for future sensor types without touching the MQTT
/// infrastructure.
#[derive(Serialize)]
struct HaDiscoveryPayload<'a> {
    name: &'a str,
    unique_id: &'a str,
    /// HA device_class – "moisture" represents a liquid presence sensor.
    device_class: &'a str,
    state_topic: &'a str,
    availability_topic: &'a str,
    payload_on: &'static str,
    payload_off: &'static str,
    payload_available: &'static str,
    payload_not_available: &'static str,
    /// Device block groups multiple entities under one HA device entry.
    device: HaDevice<'a>,
}

#[derive(Serialize)]
struct HaDevice<'a> {
    identifiers: [&'a str; 1],
    name: &'a str,
    model: &'static str,
    manufacturer: &'static str,
}

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
                payload: b"offline",
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

        // HA discovery topic:
        //   homeassistant/binary_sensor/<device_id>/liquid_level/config
        let discovery_topic = format!(
            "{}/binary_sensor/{}/liquid_level/config",
            ha.discovery_prefix, ha.device_id
        );

        // ── Build HA discovery payload ────────────────────────────────────────
        let unique_id = format!("{}_liquid_level", ha.device_id);
        let payload_struct = HaDiscoveryPayload {
            name: "Liquid Level",
            unique_id: &unique_id,
            device_class: "moisture",
            state_topic: ha.state_topic,
            availability_topic: ha.availability_topic,
            payload_on: "ON",
            payload_off: "OFF",
            payload_available: "online",
            payload_not_available: "offline",
            device: HaDevice {
                identifiers: [ha.device_id],
                name: ha.device_name,
                model: "XKC-Y25-NPN",
                manufacturer: "Hydrolevel / ESP32-C3",
            },
        };
        let discovery_payload = serde_json::to_string(&payload_struct)?;

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
            .enqueue(&topic, QoS::AtLeastOnce, true, b"online")?;
        Ok(())
    }

    /// Publish the current liquid level state.
    pub fn publish_state(&mut self, state: LiquidState) -> Result<()> {
        let topic = self.state_topic.clone();
        let payload = state.as_ha_state().as_bytes();
        info!("Publishing state '{}' → {}", state.as_ha_state(), topic);
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
        cfg.server_certificate = Some(
            esp_idf_svc::tls::X509::pem_until_nul(ca)
                .map_err(|e| anyhow::anyhow!("Invalid CA certificate: {:?}", e))?,
        );
    }

    match (tls.client_cert, tls.client_key) {
        (Some(cert), Some(key)) => {
            cfg.client_certificate = Some(
                esp_idf_svc::tls::X509::pem_until_nul(cert)
                    .map_err(|e| anyhow::anyhow!("Invalid client certificate: {:?}", e))?,
            );
            cfg.private_key = Some(
                esp_idf_svc::tls::X509::pem_until_nul(key)
                    .map_err(|e| anyhow::anyhow!("Invalid client key: {:?}", e))?,
            );
        }
        (None, None) => {}
        _ => bail!(
            "Both HYDROLEVEL_MQTT_CLIENT_CERT_PATH and HYDROLEVEL_MQTT_CLIENT_KEY_PATH \
             must be set together for mTLS"
        ),
    }

    Ok(())
}
