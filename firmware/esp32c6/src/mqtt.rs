// src/mqtt.rs – MQTT client with Home Assistant auto-discovery
//
// Supports:
//  • MQTT 3.1.1 and MQTT 5 (selected via the broker URI scheme)
//  • Plain and TLS-encrypted connections (mqtt:// vs mqtts://)
//  • Username/password authentication
//  • Certificate-based mutual TLS (mTLS) when client_cert + client_key are set
//  • Home Assistant MQTT discovery protocol for binary_sensor entities
//
// HA discovery payloads are published to
//   <discovery_prefix>/binary_sensor/<device_id>/<object_id>/config
// so each configured binary sensor appears automatically in HA.

use anyhow::{bail, Result};
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EventPayload, LwtConfiguration, MqttClientConfiguration, QoS,
};
use log::{error, info, warn};
use plantfriend_core::homeassistant::{binary_sensor_discovery_payload, DeviceMetadata};
use plantfriend_core::mqtt::{
    binary_sensor_discovery_topic, AVAILABILITY_OFFLINE, AVAILABILITY_ONLINE,
};
use plantfriend_core::sensors::{LiquidState, MqttState};
use std::collections::BTreeMap;

use crate::config::{Config, TlsConfig};

// ── MQTT client wrapper ───────────────────────────────────────────────────────

/// Manages the MQTT connection and publishes sensor state + HA discovery.
pub struct MqttManager {
    client: EspMqttClient<'static>,
    availability_topic: String,
    sensors: BTreeMap<String, SensorPublishBinding>,
    discovery_messages: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct SensorPublishBinding {
    mqtt_enabled: bool,
    state_topic: Option<String>,
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
        let availability_topic = ha.availability_topic.to_string();

        let mut sensors: BTreeMap<String, SensorPublishBinding> = BTreeMap::new();
        let mut discovery_messages: Vec<(String, String)> = Vec::new();

        for spec in config.runtime.sensors {
            let state_topic = spec.mqtt_state_topic.map(str::to_owned);
            if spec.outputs.mqtt && state_topic.is_none() {
                bail!(
                    "Sensor '{}' has mqtt output enabled but no mqtt_state_topic",
                    spec.id
                );
            }

            sensors.insert(
                spec.id.to_string(),
                SensorPublishBinding {
                    mqtt_enabled: spec.outputs.mqtt,
                    state_topic: state_topic.clone(),
                },
            );

            if spec.outputs.homeassistant {
                let object_id = spec.ha_object_id.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Sensor '{}' has Home Assistant output enabled but no ha_object_id",
                        spec.id
                    )
                })?;
                let name = spec.ha_name.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Sensor '{}' has Home Assistant output enabled but no ha_name",
                        spec.id
                    )
                })?;
                let device_class = spec.ha_device_class.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Sensor '{}' has Home Assistant output enabled but no ha_device_class",
                        spec.id
                    )
                })?;
                let state_topic = state_topic.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Sensor '{}' needs mqtt_state_topic for Home Assistant discovery",
                        spec.id
                    )
                })?;

                let discovery_topic =
                    binary_sensor_discovery_topic(ha.discovery_prefix, ha.device_id, object_id);

                let discovery_payload = binary_sensor_discovery_payload(
                    DeviceMetadata {
                        device_id: ha.device_id,
                        device_name: ha.device_name,
                        model: crate::config::GEN_HA_MODEL,
                        manufacturer: crate::config::GEN_HA_MANUFACTURER,
                    },
                    object_id,
                    name,
                    device_class,
                    &state_topic,
                    ha.availability_topic,
                )?;

                discovery_messages.push((discovery_topic, discovery_payload));
            }
        }

        Ok(Self {
            client,
            availability_topic,
            sensors,
            discovery_messages,
        })
    }

    /// Publish all Home Assistant MQTT discovery payloads (retained).
    pub fn publish_discovery(&mut self) -> Result<()> {
        for (topic, payload) in &self.discovery_messages {
            info!("Publishing HA discovery to: {}", topic);
            self.client.enqueue(
                topic,
                QoS::AtLeastOnce,
                true, // retained so HA picks it up after restart
                payload.as_bytes(),
            )?;
        }
        Ok(())
    }

    /// Announce that the device is online (retained availability message).
    pub fn publish_online(&mut self) -> Result<()> {
        let topic = self.availability_topic.clone();
        self.client.enqueue(
            &topic,
            QoS::AtLeastOnce,
            true,
            AVAILABILITY_ONLINE.as_bytes(),
        )?;
        Ok(())
    }

    /// Publish the current state for a specific sensor id.
    pub fn publish_state_for_sensor<S>(&mut self, sensor_id: &str, state: S) -> Result<()>
    where
        S: MqttState,
    {
        let Some(binding) = self.sensors.get(sensor_id) else {
            bail!("Unknown sensor id '{}': cannot publish state", sensor_id);
        };

        if !binding.mqtt_enabled {
            return Ok(());
        }

        let Some(topic) = binding.state_topic.as_deref() else {
            bail!(
                "Sensor '{}' has mqtt output enabled but no state topic is configured",
                sensor_id
            );
        };

        let payload_state = state.as_mqtt_state();
        let payload = payload_state.as_bytes();
        info!(
            "Publishing sensor '{}' state '{}' → {}",
            sensor_id, payload_state, topic
        );
        self.client
            .enqueue(topic, QoS::AtLeastOnce, false, payload)?;
        Ok(())
    }

    /// Legacy single-sensor publish path using the first configured sensor.
    pub fn publish_state(&mut self, state: LiquidState) -> Result<()> {
        let first_id = self
            .sensors
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("No sensor bindings configured for publish"))?;
        self.publish_state_for_sensor(&first_id, state)
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
