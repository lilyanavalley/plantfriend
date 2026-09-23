//! Simple float switch sensor.
//! 
//! This type of sensor is similar to pushbuttons and hall-effect sensors that pull high/low.
//! No data stream to this type of sensor is used, and instead, the MCU interprets binary pin state
//! to reflect state to HomeAssistant/MQTT.
//! 
//! Designed around this sensor: https://a.co/d/02B6orNm


use super::{
    DigitalInputDebouncer, DigitalInputSensorConfig, DigitalSignalState, HomeAssistantState,
    MqttState,
};


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatSwitchState {
    Closed,
    Open,
}

impl FloatSwitchState {
    pub fn as_bool(self) -> bool {
        matches!(self, FloatSwitchState::Closed)
    }
}

impl HomeAssistantState for FloatSwitchState {
    fn as_ha_state(self) -> &'static str {
        match self {
            FloatSwitchState::Closed => "ON",
            FloatSwitchState::Open => "OFF",
        }
    }
}

impl MqttState for FloatSwitchState {
    fn as_mqtt_state(self) -> &'static str {
        self.as_ha_state()
    }
}

impl DigitalSignalState for FloatSwitchState {
    fn active_state() -> Self {
        Self::Closed
    }

    fn inactive_state() -> Self {
        Self::Open
    }
}

pub type FloatSwitchSensorConfig = DigitalInputSensorConfig;
pub type FloatSwitchDebouncer = DigitalInputDebouncer<FloatSwitchState>;
