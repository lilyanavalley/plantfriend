use crate::sensors::{LightLux, SensorHubSnapshot};

#[cfg(feature = "led-neopixel")]
pub mod neopixel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRuntimeStatus {
    Booting,
    WifiConnecting,
    BrokerConnecting,
    Online,
    Degraded,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlantHealthSignal {
    Unknown,
    Healthy,
    NeedsWater,
    Alert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedEffect {
    Solid,
    BlinkSlow,
    BlinkFast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const OFF: Self = Self::new(0, 0, 0);
    pub const RED: Self = Self::new(255, 0, 0);
    pub const GREEN: Self = Self::new(0, 255, 0);
    pub const BLUE: Self = Self::new(0, 0, 255);
    pub const YELLOW: Self = Self::new(255, 180, 0);
    pub const CYAN: Self = Self::new(0, 255, 255);
    pub const MAGENTA: Self = Self::new(255, 0, 255);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn scale(self, brightness: u8) -> Self {
        let scale_channel =
            |channel: u8| -> u8 { ((channel as u16 * brightness as u16) / 255) as u8 };

        Self {
            r: scale_channel(self.r),
            g: scale_channel(self.g),
            b: scale_channel(self.b),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedFrame {
    pub color: RgbColor,
    pub effect: LedEffect,
}

impl LedFrame {
    pub const fn solid(color: RgbColor) -> Self {
        Self {
            color,
            effect: LedEffect::Solid,
        }
    }
}

pub trait StatusLedProfile {
    fn frame_for(
        &self,
        runtime: DeviceRuntimeStatus,
        health: PlantHealthSignal,
        now_ms: u64,
    ) -> LedFrame;
}

pub trait LedOutputDriver {
    type Error;

    fn apply_frame(&mut self, frame: LedFrame) -> Result<(), Self::Error>;
}

pub trait MetricsDisplay {
    type Error;

    fn render_status(
        &mut self,
        runtime: DeviceRuntimeStatus,
        summary: &PlantStatusSummary,
    ) -> Result<(), Self::Error>;
}

pub trait EpaperDisplay {
    type Error;

    fn refresh_status(
        &mut self,
        runtime: DeviceRuntimeStatus,
        summary: &PlantStatusSummary,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlantStatusSummary {
    pub liquid_state_present: bool,
    pub water_level_switch_closed: Option<bool>,
    pub light_lux: Option<LightLux>,
}

impl PlantStatusSummary {
    pub fn from_snapshot(snapshot: &impl SensorHubSnapshot) -> Self {
        Self {
            liquid_state_present: snapshot.liquid_level().as_bool(),
            water_level_switch_closed: snapshot.water_level_switch_closed(),
            light_lux: snapshot.light_lux(),
        }
    }
}
