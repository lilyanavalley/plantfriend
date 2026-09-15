#![forbid(unsafe_code)]

pub mod protocol {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RadioProtocol {
        WifiMqtt,
        BleBTHome,
        BleCustom,
        LoRa,
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
}

pub mod bthome {
    use crate::sensors::LiquidState;

    pub const BT_HOME_SERVICE_UUID: u16 = 0xFCD2;
    const BT_HOME_INFO_OBJECT_ID: u8 = 0x40;
    const BT_HOME_BINARY_SENSOR_OBJECT_ID: u8 = 0x2D;

    pub fn liquid_level_advertisement(state: LiquidState, encrypted: bool) -> [u8; 4] {
        [
            BT_HOME_INFO_OBJECT_ID,
            if encrypted { 0b0100_0001 } else { 0b0100_0000 },
            BT_HOME_BINARY_SENSOR_OBJECT_ID,
            if state.as_bool() { 1 } else { 0 },
        ]
    }
}
