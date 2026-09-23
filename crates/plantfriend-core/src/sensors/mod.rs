
/// XKC-Y25-NPN non-contact liquid presense sensor.
#[cfg(feature = "sensor-xkc-y25")]
pub mod xkc_y25;
#[cfg(feature = "sensor-xkc-y25")]
pub use xkc_y25::*;

/// Simple GPIO float switch.
#[cfg(feature = "sensor-floatswitch")]
pub mod basic_float;
#[cfg(feature = "sensor-floatswitch")]
pub use basic_float::*;


pub trait HomeAssistantState {
    fn as_ha_state(self) -> &'static str;
}

pub trait MqttState {
    fn as_mqtt_state(self) -> &'static str;
}

pub trait DigitalSignalState: Copy + Eq {
    fn active_state() -> Self;
    fn inactive_state() -> Self;

    fn from_active(active: bool) -> Self {
        if active {
            Self::active_state()
        } else {
            Self::inactive_state()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingTransition<S>
where
    S: Copy + Eq,
{
    state: S,
    since_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigitalInputDebouncer<S>
where
    S: Copy + Eq,
{
    stable: S,
    debounce_ms: u64,
    pending: Option<PendingTransition<S>>,
}

impl<S> DigitalInputDebouncer<S>
where
    S: Copy + Eq,
{
    pub fn new(initial: S, debounce_ms: u64) -> Self {
        Self {
            stable: initial,
            debounce_ms,
            pending: None,
        }
    }

    pub fn stable(&self) -> S {
        self.stable
    }

    pub fn update(&mut self, observed: S, now_ms: u64) -> Option<S> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigitalInputSensorConfig {
    pub active_high: bool,
    pub debounce_ms: u64,
}

pub fn decode_digital_input_state<S>(is_high: bool, active_high: bool) -> S
where
    S: DigitalSignalState,
{
    S::from_active(is_high == active_high)
}


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

impl HomeAssistantState for LiquidState {
    fn as_ha_state(self) -> &'static str {
        Self::as_ha_state(self)
    }
}

impl MqttState for LiquidState {
    fn as_mqtt_state(self) -> &'static str {
        self.as_ha_state()
    }
}

impl DigitalSignalState for LiquidState {
    fn active_state() -> Self {
        Self::Present
    }

    fn inactive_state() -> Self {
        Self::Absent
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightLux(pub f32);

pub trait SensorHubSnapshot {
    fn liquid_level(&self) -> LiquidState;
    fn light_lux(&self) -> Option<LightLux>;
    fn water_level_switch_closed(&self) -> Option<bool>;
}
