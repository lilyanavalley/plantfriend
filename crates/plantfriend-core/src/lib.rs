#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod protocol {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RadioProtocol {
        WifiMqtt,
        BleBTHome,
        BleCustom,
        LoRa,
    }
}

pub mod capabilities {
    pub const SUPPORTS_BLE: bool = cfg!(feature = "framework-ble");
    pub const SUPPORTS_WIFI_MQTT: bool = cfg!(feature = "framework-wifi-mqtt");
    pub const SUPPORTS_DIGITAL_INPUT_SENSORS: bool = cfg!(feature = "sensor-digital-input");

    pub const CHIP_ESP32_PROFILE: bool = cfg!(feature = "chip-esp32");
    pub const CHIP_NRF52_PROFILE: bool = cfg!(feature = "chip-nrf52");
}

pub mod publish {
    pub trait MonotonicClock {
        fn now_ms(&self) -> u64;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PublishReason {
        Initial,
        StateChanged,
        Heartbeat,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PublishEvent<S> {
        pub state: S,
        pub reason: PublishReason,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct StatePublishPolicy<S>
    where
        S: Copy + Eq,
    {
        current_state: S,
        heartbeat_interval_ms: Option<u64>,
        last_heartbeat_ms: u64,
        needs_initial_publish: bool,
    }

    impl<S> StatePublishPolicy<S>
    where
        S: Copy + Eq,
    {
        pub fn with_clock(
            initial_state: S,
            heartbeat_interval_ms: Option<u64>,
            clock: &impl MonotonicClock,
        ) -> Self {
            Self::new(initial_state, heartbeat_interval_ms, clock.now_ms())
        }

        pub fn new(initial_state: S, heartbeat_interval_ms: Option<u64>, now_ms: u64) -> Self {
            Self {
                current_state: initial_state,
                heartbeat_interval_ms,
                last_heartbeat_ms: now_ms,
                needs_initial_publish: true,
            }
        }

        pub fn current_state(&self) -> S {
            self.current_state
        }

        pub fn initial_event(&mut self) -> Option<PublishEvent<S>> {
            if !self.needs_initial_publish {
                return None;
            }

            self.needs_initial_publish = false;
            Some(PublishEvent {
                state: self.current_state,
                reason: PublishReason::Initial,
            })
        }

        pub fn on_state_change(&mut self, new_state: S) -> Option<PublishEvent<S>> {
            if new_state == self.current_state {
                return None;
            }

            self.current_state = new_state;
            Some(PublishEvent {
                state: self.current_state,
                reason: PublishReason::StateChanged,
            })
        }

        pub fn on_tick(&mut self, now_ms: u64) -> Option<PublishEvent<S>> {
            if let Some(interval) = self.heartbeat_interval_ms {
                if now_ms.saturating_sub(self.last_heartbeat_ms) >= interval {
                    self.last_heartbeat_ms = now_ms;
                    return Some(PublishEvent {
                        state: self.current_state,
                        reason: PublishReason::Heartbeat,
                    });
                }
            }

            None
        }

        pub fn on_tick_with_clock(
            &mut self,
            clock: &impl MonotonicClock,
        ) -> Option<PublishEvent<S>> {
            self.on_tick(clock.now_ms())
        }
    }
}

#[cfg(feature = "framework-wifi-mqtt-base")]
pub mod mqtt {
    use alloc::format;
    use alloc::string::String;

    pub const AVAILABILITY_ONLINE: &str = "online";
    pub const AVAILABILITY_OFFLINE: &str = "offline";

    pub fn binary_sensor_discovery_topic(
        discovery_prefix: &str,
        device_id: &str,
        object_id: &str,
    ) -> String {
        format!(
            "{}/binary_sensor/{}/{}/config",
            discovery_prefix, device_id, object_id
        )
    }

    pub fn liquid_level_discovery_topic(discovery_prefix: &str, device_id: &str) -> String {
        binary_sensor_discovery_topic(discovery_prefix, device_id, "liquid_level")
    }
}

pub mod sensors {

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LiquidState {
        Present,
        Absent,
    }

    impl LiquidState {
        pub fn as_ha_state(self) -> &'static str {
            match self {
                LiquidState::Present => "ON",
                LiquidState::Absent => "OFF",
            }
        }

        pub fn as_bool(self) -> bool {
            matches!(self, LiquidState::Present)
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct LightLux(pub f32);

    pub trait SensorHubSnapshot {
        fn liquid_level(&self) -> LiquidState;
        fn light_lux(&self) -> Option<LightLux>;
        fn water_level_switch_closed(&self) -> Option<bool>;
    }

    #[cfg(feature = "sensor-digital-input")]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DigitalInputLiquidSensorConfig {
        pub active_high: bool,
        pub debounce_ms: u64,
    }

    #[cfg(feature = "sensor-digital-input")]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct PendingTransition {
        state: LiquidState,
        since_ms: u64,
    }

    #[cfg(feature = "sensor-digital-input")]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LiquidLevelDebouncer {
        stable: LiquidState,
        debounce_ms: u64,
        pending: Option<PendingTransition>,
    }

    #[cfg(feature = "sensor-digital-input")]
    impl LiquidLevelDebouncer {
        pub fn new(initial: LiquidState, debounce_ms: u64) -> Self {
            Self {
                stable: initial,
                debounce_ms,
                pending: None,
            }
        }

        pub fn stable(&self) -> LiquidState {
            self.stable
        }

        pub fn update(&mut self, observed: LiquidState, now_ms: u64) -> Option<LiquidState> {
            if observed == self.stable {
                self.pending = None;
                return None;
            }

            match self.pending {
                Some(pending) if pending.state == observed => {
                    if now_ms.saturating_sub(pending.since_ms) >= self.debounce_ms {
                        self.stable = observed;
                        self.pending = None;
                        Some(self.stable)
                    } else {
                        None
                    }
                }
                _ => {
                    self.pending = Some(PendingTransition {
                        state: observed,
                        since_ms: now_ms,
                    });
                    None
                }
            }
        }
    }
}

#[cfg(feature = "homeassistant-mqtt")]
pub mod homeassistant {
    use alloc::string::String;
    use serde::Serialize;

    use crate::mqtt;
    use crate::sensors::LiquidState;

    pub const PAYLOAD_ON: &str = "ON";
    pub const PAYLOAD_OFF: &str = "OFF";
    pub const PAYLOAD_AVAILABLE: &str = mqtt::AVAILABILITY_ONLINE;
    pub const PAYLOAD_NOT_AVAILABLE: &str = mqtt::AVAILABILITY_OFFLINE;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct DeviceMetadata<'a> {
        pub device_id: &'a str,
        pub device_name: &'a str,
        pub manufacturer: &'a str,
        pub model: &'a str,
    }

    #[derive(Serialize)]
    struct BinarySensorDiscoveryPayload<'a> {
        name: &'a str,
        unique_id: String,
        device_class: &'a str,
        state_topic: &'a str,
        availability_topic: &'a str,
        payload_on: &'static str,
        payload_off: &'static str,
        payload_available: &'static str,
        payload_not_available: &'static str,
        device: DiscoveryDevice<'a>,
    }

    #[derive(Serialize)]
    struct DiscoveryDevice<'a> {
        identifiers: [&'a str; 1],
        name: &'a str,
        model: &'a str,
        manufacturer: &'a str,
    }

    pub fn liquid_level_discovery_topic(discovery_prefix: &str, device_id: &str) -> String {
        mqtt::liquid_level_discovery_topic(discovery_prefix, device_id)
    }

    pub fn liquid_level_discovery_payload(
        metadata: DeviceMetadata<'_>,
        state_topic: &str,
        availability_topic: &str,
    ) -> Result<String, serde_json::Error> {
        let payload = BinarySensorDiscoveryPayload {
            name: "Liquid Level",
            unique_id: alloc::format!("{}_liquid_level", metadata.device_id),
            device_class: "moisture",
            state_topic,
            availability_topic,
            payload_on: PAYLOAD_ON,
            payload_off: PAYLOAD_OFF,
            payload_available: PAYLOAD_AVAILABLE,
            payload_not_available: PAYLOAD_NOT_AVAILABLE,
            device: DiscoveryDevice {
                identifiers: [metadata.device_id],
                name: metadata.device_name,
                model: metadata.model,
                manufacturer: metadata.manufacturer,
            },
        };

        serde_json::to_string(&payload)
    }

    pub fn liquid_level_state_payload(state: LiquidState) -> &'static str {
        state.as_ha_state()
    }
}

#[cfg(feature = "bthome")]
pub mod bthome {
    use crate::sensors::LiquidState;

    pub const BT_HOME_SERVICE_UUID: u16 = 0xFCD2;
    const BT_HOME_BINARY_SENSOR_OBJECT_ID: u8 = 0x2D;

    pub fn liquid_level_advertisement(state: LiquidState, encrypted: bool) -> [u8; 3] {
        [
            if encrypted { 0b0100_0001 } else { 0b0100_0000 },
            BT_HOME_BINARY_SENSOR_OBJECT_ID,
            if state.as_bool() { 1 } else { 0 },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::sensors::LiquidState;

    #[cfg(feature = "sensor-digital-input")]
    #[test]
    fn debouncer_emits_only_after_stable_window() {
        use super::sensors::LiquidLevelDebouncer;

        let mut d = LiquidLevelDebouncer::new(LiquidState::Absent, 100);

        assert_eq!(d.update(LiquidState::Present, 0), None);
        assert_eq!(d.update(LiquidState::Present, 50), None);
        assert_eq!(d.update(LiquidState::Present, 99), None);
        assert_eq!(
            d.update(LiquidState::Present, 100),
            Some(LiquidState::Present)
        );
        assert_eq!(d.stable(), LiquidState::Present);
    }

    #[cfg(feature = "sensor-digital-input")]
    #[test]
    fn debouncer_cancels_pending_transition_on_bounce_back() {
        use super::sensors::LiquidLevelDebouncer;

        let mut d = LiquidLevelDebouncer::new(LiquidState::Absent, 100);

        assert_eq!(d.update(LiquidState::Present, 0), None);
        assert_eq!(d.update(LiquidState::Absent, 10), None);
        assert_eq!(d.stable(), LiquidState::Absent);

        assert_eq!(d.update(LiquidState::Present, 200), None);
        assert_eq!(
            d.update(LiquidState::Present, 301),
            Some(LiquidState::Present)
        );
    }

    #[cfg(feature = "homeassistant-mqtt")]
    #[test]
    fn homeassistant_discovery_payload_contains_expected_fields() {
        use super::homeassistant::{
            liquid_level_discovery_payload, DeviceMetadata, PAYLOAD_AVAILABLE,
        };

        let json = liquid_level_discovery_payload(
            DeviceMetadata {
                device_id: "plant-1",
                device_name: "Plant 1",
                manufacturer: "plantfriend",
                model: "XKC",
            },
            "pm/plant-1/state",
            "pm/plant-1/availability",
        )
        .expect("payload generation must succeed");

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");

        assert_eq!(parsed["name"], "Liquid Level");
        assert_eq!(parsed["unique_id"], "plant-1_liquid_level");
        assert_eq!(parsed["state_topic"], "pm/plant-1/state");
        assert_eq!(parsed["availability_topic"], "pm/plant-1/availability");
        assert_eq!(parsed["payload_available"], PAYLOAD_AVAILABLE);
        assert_eq!(parsed["device"]["identifiers"][0], "plant-1");
        assert_eq!(parsed["device"]["name"], "Plant 1");
    }

    #[cfg(feature = "homeassistant-mqtt")]
    #[test]
    fn homeassistant_state_payload_maps_liquid_state() {
        use super::homeassistant::liquid_level_state_payload;

        assert_eq!(liquid_level_state_payload(LiquidState::Present), "ON");
        assert_eq!(liquid_level_state_payload(LiquidState::Absent), "OFF");
    }

    #[test]
    fn state_publish_policy_emits_initial_once() {
        use super::publish::{PublishReason, StatePublishPolicy};

        let mut policy = StatePublishPolicy::new(LiquidState::Absent, Some(1000), 0);

        let first = policy.initial_event().expect("initial event expected");
        assert_eq!(first.state, LiquidState::Absent);
        assert_eq!(first.reason, PublishReason::Initial);
        assert_eq!(policy.initial_event(), None);
    }

    #[test]
    fn state_publish_policy_emits_state_change_only_on_transition() {
        use super::publish::{PublishReason, StatePublishPolicy};

        let mut policy = StatePublishPolicy::new(LiquidState::Absent, Some(1000), 0);
        let _ = policy.initial_event();

        assert_eq!(policy.on_state_change(LiquidState::Absent), None);

        let evt = policy
            .on_state_change(LiquidState::Present)
            .expect("state change event expected");
        assert_eq!(evt.state, LiquidState::Present);
        assert_eq!(evt.reason, PublishReason::StateChanged);
        assert_eq!(policy.current_state(), LiquidState::Present);
    }

    #[test]
    fn state_publish_policy_emits_heartbeat_on_interval() {
        use super::publish::{PublishReason, StatePublishPolicy};

        let mut policy = StatePublishPolicy::new(LiquidState::Present, Some(100), 0);
        let _ = policy.initial_event();

        assert_eq!(policy.on_tick(99), None);

        let evt = policy.on_tick(100).expect("heartbeat event expected");
        assert_eq!(evt.state, LiquidState::Present);
        assert_eq!(evt.reason, PublishReason::Heartbeat);

        assert_eq!(policy.on_tick(150), None);
        assert!(policy.on_tick(200).is_some());
    }

    #[test]
    fn state_publish_policy_disables_heartbeat_when_none() {
        use super::publish::StatePublishPolicy;

        let mut policy = StatePublishPolicy::new(LiquidState::Present, None, 0);
        let _ = policy.initial_event();

        assert_eq!(policy.on_tick(10_000), None);
        assert_eq!(policy.on_tick(20_000), None);
    }

    #[test]
    fn state_publish_policy_supports_injected_clock_trait() {
        use core::cell::Cell;

        use super::publish::{MonotonicClock, PublishReason, StatePublishPolicy};

        struct FakeClock {
            now_ms: Cell<u64>,
        }

        impl FakeClock {
            fn new(now_ms: u64) -> Self {
                Self {
                    now_ms: Cell::new(now_ms),
                }
            }

            fn set(&self, now_ms: u64) {
                self.now_ms.set(now_ms);
            }
        }

        impl MonotonicClock for FakeClock {
            fn now_ms(&self) -> u64 {
                self.now_ms.get()
            }
        }

        let clock = FakeClock::new(500);
        let mut policy = StatePublishPolicy::with_clock(LiquidState::Absent, Some(100), &clock);
        let _ = policy.initial_event();

        clock.set(590);
        assert_eq!(policy.on_tick_with_clock(&clock), None);

        clock.set(600);
        let heartbeat = policy
            .on_tick_with_clock(&clock)
            .expect("heartbeat event expected");
        assert_eq!(heartbeat.reason, PublishReason::Heartbeat);
        assert_eq!(heartbeat.state, LiquidState::Absent);
    }
}
