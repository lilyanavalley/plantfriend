use super::{
    DeviceRuntimeStatus, LedEffect, LedFrame, PlantHealthSignal, RgbColor, StatusLedProfile,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeopixelColorOrder {
    Rgb,
    Grb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeopixelLedConfig {
    pub led_count: u16,
    pub max_brightness: u8,
    pub color_order: NeopixelColorOrder,
}

impl NeopixelLedConfig {
    pub const fn with_defaults() -> Self {
        Self {
            led_count: 1,
            max_brightness: 64,
            color_order: NeopixelColorOrder::Grb,
        }
    }
}

impl Default for NeopixelLedConfig {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeopixelStatusProfile {
    pub config: NeopixelLedConfig,
}

impl NeopixelStatusProfile {
    pub fn new(config: NeopixelLedConfig) -> Self {
        Self { config }
    }
}

impl StatusLedProfile for NeopixelStatusProfile {
    fn frame_for(
        &self,
        runtime: DeviceRuntimeStatus,
        health: PlantHealthSignal,
        now_ms: u64,
    ) -> LedFrame {
        let blink_1hz_on = ((now_ms / 500) % 2) == 0;
        let blink_2hz_on = ((now_ms / 250) % 2) == 0;

        let frame = match runtime {
            DeviceRuntimeStatus::Booting => LedFrame {
                color: if blink_1hz_on {
                    RgbColor::BLUE
                } else {
                    RgbColor::OFF
                },
                effect: LedEffect::BlinkSlow,
            },
            DeviceRuntimeStatus::WifiConnecting => LedFrame {
                color: if blink_2hz_on {
                    RgbColor::YELLOW
                } else {
                    RgbColor::OFF
                },
                effect: LedEffect::BlinkFast,
            },
            DeviceRuntimeStatus::BrokerConnecting => LedFrame {
                color: if blink_1hz_on {
                    RgbColor::MAGENTA
                } else {
                    RgbColor::OFF
                },
                effect: LedEffect::BlinkSlow,
            },
            DeviceRuntimeStatus::Online => match health {
                PlantHealthSignal::Healthy => LedFrame::solid(RgbColor::GREEN),
                PlantHealthSignal::NeedsWater => LedFrame::solid(RgbColor::RED),
                PlantHealthSignal::Alert => LedFrame::solid(RgbColor::YELLOW),
                PlantHealthSignal::Unknown => LedFrame::solid(RgbColor::CYAN),
            },
            DeviceRuntimeStatus::Degraded => LedFrame {
                color: if blink_1hz_on {
                    RgbColor::YELLOW
                } else {
                    RgbColor::OFF
                },
                effect: LedEffect::BlinkSlow,
            },
            DeviceRuntimeStatus::Error => LedFrame {
                color: if blink_2hz_on {
                    RgbColor::RED
                } else {
                    RgbColor::OFF
                },
                effect: LedEffect::BlinkFast,
            },
        };

        LedFrame {
            color: frame.color.scale(self.config.max_brightness),
            effect: frame.effect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NeopixelLedConfig, NeopixelStatusProfile};
    use crate::outputs::{DeviceRuntimeStatus, PlantHealthSignal, StatusLedProfile};

    #[test]
    fn online_needs_water_is_red() {
        let profile = NeopixelStatusProfile::new(NeopixelLedConfig::default());
        let frame = profile.frame_for(
            DeviceRuntimeStatus::Online,
            PlantHealthSignal::NeedsWater,
            1_000,
        );
        assert!(frame.color.r > frame.color.g);
        assert!(frame.color.r > frame.color.b);
    }

    #[test]
    fn booting_blinks_off_half_cycle() {
        let profile = NeopixelStatusProfile::new(NeopixelLedConfig::default());
        let on = profile.frame_for(DeviceRuntimeStatus::Booting, PlantHealthSignal::Unknown, 0);
        let off = profile.frame_for(
            DeviceRuntimeStatus::Booting,
            PlantHealthSignal::Unknown,
            600,
        );
        assert_ne!(on.color, off.color);
    }
}
