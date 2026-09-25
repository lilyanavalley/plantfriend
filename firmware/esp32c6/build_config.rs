use std::collections::BTreeSet;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct DeviceToml {
    pub schema_version: u32,
    pub device: DeviceSection,
    pub wifi: WifiSection,
    pub mqtt: MqttSection,
    pub publish: PublishSection,
    pub homeassistant: HomeAssistantSection,
    pub availability: AvailabilitySection,
    pub sensors: Vec<SensorDef>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceSection {
    pub id: String,
    pub name: String,
    pub manufacturer: String,
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct WifiSection {
    pub ssid: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct MqttSection {
    pub broker_uri: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub client_id: String,
    pub tls: Option<MqttTlsSection>,
}

#[derive(Debug, Deserialize)]
pub struct MqttTlsSection {
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
    pub client_key_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PublishSection {
    pub interval_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct HomeAssistantSection {
    pub discovery_prefix: String,
}

#[derive(Debug, Deserialize)]
pub struct AvailabilitySection {
    pub topic: String,
}

#[derive(Debug, Deserialize)]
pub struct SensorDef {
    pub id: String,
    pub kind: String,
    pub pin: u32,
    pub active_high: bool,
    pub debounce_ms: u64,
    pub outputs: SensorOutputs,
    pub mqtt: Option<SensorMqtt>,
    pub homeassistant: Option<SensorHomeAssistant>,
}

#[derive(Debug, Deserialize)]
pub struct SensorOutputs {
    pub mqtt: bool,
    pub homeassistant: bool,
    pub bthome: bool,
}

#[derive(Debug, Deserialize)]
pub struct SensorMqtt {
    pub state_topic: String,
}

#[derive(Debug, Deserialize)]
pub struct SensorHomeAssistant {
    pub object_id: String,
    pub name: String,
    pub device_class: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorRuntimeDriverKind {
    XkcY25,
    BasicFloat,
}

impl SensorRuntimeDriverKind {
    pub fn from_device_kind(kind: &str) -> Result<Self, String> {
        match kind {
            "xkc_y25" => Ok(Self::XkcY25),
            "basic_float" => Ok(Self::BasicFloat),
            _ => Err(format!(
                "Unsupported sensor kind '{kind}'. Supported kinds: xkc_y25, basic_float"
            )),
        }
    }

    pub fn generated_sensor_kind_variant(self) -> &'static str {
        match self {
            Self::XkcY25 => "GeneratedSensorKind::XkcY25",
            Self::BasicFloat => "GeneratedSensorKind::BasicFloat",
        }
    }
}

pub fn parse_device_toml(content: &str) -> Result<DeviceToml, String> {
    toml::from_str(content).map_err(|e| e.to_string())
}

pub fn validate_device_toml(cfg: &DeviceToml) -> Result<(), String> {
    if cfg.schema_version != 1 {
        return Err(format!(
            "device.toml schema_version must be 1, got {}",
            cfg.schema_version
        ));
    }

    if cfg.device.id.trim().is_empty() {
        return Err("[device].id must not be empty".to_string());
    }
    if cfg.device.name.trim().is_empty() {
        return Err("[device].name must not be empty".to_string());
    }
    if cfg.device.manufacturer.trim().is_empty() {
        return Err("[device].manufacturer must not be empty".to_string());
    }
    if cfg.device.model.trim().is_empty() {
        return Err("[device].model must not be empty".to_string());
    }

    if cfg.wifi.ssid.trim().is_empty() {
        return Err("[wifi].ssid must not be empty".to_string());
    }

    if cfg.mqtt.broker_uri.trim().is_empty() {
        return Err("[mqtt].broker_uri must not be empty".to_string());
    }
    if cfg.mqtt.client_id.trim().is_empty() {
        return Err("[mqtt].client_id must not be empty".to_string());
    }

    if cfg.homeassistant.discovery_prefix.trim().is_empty() {
        return Err("[homeassistant].discovery_prefix must not be empty".to_string());
    }
    if cfg.availability.topic.trim().is_empty() {
        return Err("[availability].topic must not be empty".to_string());
    }

    if cfg.sensors.is_empty() {
        return Err("At least one [[sensors]] entry is required".to_string());
    }

    let mut sensor_ids = BTreeSet::new();
    let mut pins = BTreeSet::new();

    for sensor in &cfg.sensors {
        if !sensor_ids.insert(sensor.id.clone()) {
            return Err(format!(
                "Duplicate sensor id '{}' in [[sensors]]",
                sensor.id
            ));
        }

        if !pins.insert(sensor.pin) {
            return Err(format!(
                "GPIO pin {} is assigned to multiple sensors",
                sensor.pin
            ));
        }

        SensorRuntimeDriverKind::from_device_kind(sensor.kind.as_str())?;

        if sensor.outputs.mqtt {
            let Some(mqtt) = sensor.mqtt.as_ref() else {
                return Err(format!(
                    "Sensor '{}' enables MQTT output but [sensors.mqtt] is missing",
                    sensor.id
                ));
            };

            if mqtt.state_topic.trim().is_empty() {
                return Err(format!(
                    "Sensor '{}' mqtt.state_topic must not be empty",
                    sensor.id
                ));
            }
        }

        if sensor.outputs.homeassistant {
            let Some(ha) = sensor.homeassistant.as_ref() else {
                return Err(format!(
                    "Sensor '{}' enables Home Assistant output but [sensors.homeassistant] is missing",
                    sensor.id
                ));
            };

            if ha.object_id.trim().is_empty() {
                return Err(format!(
                    "Sensor '{}' homeassistant.object_id must not be empty",
                    sensor.id
                ));
            }
            if ha.name.trim().is_empty() {
                return Err(format!(
                    "Sensor '{}' homeassistant.name must not be empty",
                    sensor.id
                ));
            }
            if ha.device_class.trim().is_empty() {
                return Err(format!(
                    "Sensor '{}' homeassistant.device_class must not be empty",
                    sensor.id
                ));
            }
        }
    }

    if let Some(tls) = &cfg.mqtt.tls {
        let cert = normalize_optional_str(tls.client_cert_path.as_deref());
        let key = normalize_optional_str(tls.client_key_path.as_deref());
        if cert.is_some() != key.is_some() {
            return Err(
                "mqtt.tls.client_cert_path and mqtt.tls.client_key_path must be set together"
                    .to_string(),
            );
        }
    }

    Ok(())
}

pub fn normalize_optional_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{parse_device_toml, validate_device_toml, SensorRuntimeDriverKind};

    const VALID_CONFIG: &str = r#"
schema_version = 1

[device]
id = "dev-01"
name = "Hydrolevel"
manufacturer = "Plantfriend"
model = "ESP32"

[wifi]
ssid = "wifi"
password = "password"

[mqtt]
broker_uri = "mqtt://broker.local:1883"
client_id = "dev-01"

[publish]
interval_ms = 30000

[homeassistant]
discovery_prefix = "homeassistant"

[availability]
topic = "hydrolevel/dev-01/availability"

[[sensors]]
id = "tank"
kind = "xkc_y25"
pin = 4
active_high = false
debounce_ms = 200

[sensors.outputs]
mqtt = true
homeassistant = true
bthome = false

[sensors.mqtt]
state_topic = "hydrolevel/dev-01/state"

[sensors.homeassistant]
object_id = "tank"
name = "Tank"
device_class = "moisture"
"#;

    #[test]
    fn valid_config_passes_validation() {
        let cfg = parse_device_toml(VALID_CONFIG).expect("valid TOML should parse");
        let result = validate_device_toml(&cfg);
        assert!(
            result.is_ok(),
            "expected validation to pass, got {result:?}"
        );
    }

    #[test]
    fn duplicate_gpio_fails_validation() {
        let config = format!(
            "{}\n[[sensors]]\nid=\"tank2\"\nkind=\"basic_float\"\npin=4\nactive_high=true\ndebounce_ms=50\n\n[sensors.outputs]\nmqtt=true\nhomeassistant=false\nbthome=false\n\n[sensors.mqtt]\nstate_topic=\"hydrolevel/dev-01/float\"\n",
            VALID_CONFIG
        );
        let cfg = parse_device_toml(&config).expect("test TOML should parse");
        let err = validate_device_toml(&cfg).expect_err("duplicate GPIO should fail");
        assert!(err.contains("GPIO pin"), "unexpected error: {err}");
    }

    #[test]
    fn missing_mqtt_block_fails_when_mqtt_enabled() {
        let config = VALID_CONFIG.replace(
            "[sensors.mqtt]\nstate_topic = \"hydrolevel/dev-01/state\"\n",
            "",
        );
        let cfg = parse_device_toml(&config).expect("test TOML should parse");
        let err = validate_device_toml(&cfg).expect_err("missing mqtt block should fail");
        assert!(
            err.contains("enables MQTT output"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn sensor_kind_maps_to_runtime_driver_variants() {
        assert_eq!(
            SensorRuntimeDriverKind::from_device_kind("xkc_y25")
                .expect("xkc_y25 should map")
                .generated_sensor_kind_variant(),
            "GeneratedSensorKind::XkcY25"
        );
        assert_eq!(
            SensorRuntimeDriverKind::from_device_kind("basic_float")
                .expect("basic_float should map")
                .generated_sensor_kind_variant(),
            "GeneratedSensorKind::BasicFloat"
        );
    }

    #[test]
    fn unsupported_sensor_kind_returns_explicit_error() {
        let err = SensorRuntimeDriverKind::from_device_kind("unknown_kind")
            .expect_err("unknown kind should fail");
        assert!(
            err.contains("Unsupported sensor kind"),
            "unexpected error: {err}"
        );
    }
}
