use anyhow::{bail, Result};
use log::info;
use plantfriend_core::outputs::neopixel::NeopixelStatusProfile;
use plantfriend_core::outputs::{
    DeviceRuntimeStatus, LedFrame, PlantHealthSignal, RgbColor, StatusLedProfile,
};

use crate::config::{GeneratedLedKind, GeneratedLedRuntimeSpec};

pub struct StatusIndicatorHub {
    leds: Vec<ActiveLedRuntime>,
}

impl StatusIndicatorHub {
    pub fn from_specs(specs: &'static [GeneratedLedRuntimeSpec]) -> Result<Self> {
        let mut leds = Vec::with_capacity(specs.len());
        for spec in specs {
            match spec.kind {
                GeneratedLedKind::Neopixel => {
                    leds.push(ActiveLedRuntime::Neopixel(NeopixelRuntime::new(spec)))
                }
            }
        }

        Ok(Self { leds })
    }

    pub fn set_status(
        &mut self,
        runtime: DeviceRuntimeStatus,
        health: PlantHealthSignal,
        now_ms: u64,
    ) -> Result<()> {
        for led in &mut self.leds {
            led.apply(runtime, health, now_ms)?;
        }
        Ok(())
    }
}

enum ActiveLedRuntime {
    Neopixel(NeopixelRuntime),
}

impl ActiveLedRuntime {
    fn apply(
        &mut self,
        runtime: DeviceRuntimeStatus,
        health: PlantHealthSignal,
        now_ms: u64,
    ) -> Result<()> {
        match self {
            Self::Neopixel(led) => led.apply(runtime, health, now_ms),
        }
    }
}

struct NeopixelRuntime {
    spec: &'static GeneratedLedRuntimeSpec,
    profile: NeopixelStatusProfile,
    last_frame: Option<LedFrame>,
}

impl NeopixelRuntime {
    fn new(spec: &'static GeneratedLedRuntimeSpec) -> Self {
        Self {
            spec,
            profile: NeopixelStatusProfile::new(spec.neopixel_config()),
            last_frame: None,
        }
    }

    fn apply(
        &mut self,
        runtime: DeviceRuntimeStatus,
        health: PlantHealthSignal,
        now_ms: u64,
    ) -> Result<()> {
        if self.spec.led_count == 0 {
            bail!("LED runtime '{}' has led_count=0", self.spec.id);
        }

        let frame = self.profile.frame_for(runtime, health, now_ms);
        if self.last_frame == Some(frame) {
            return Ok(());
        }

        self.write_neopixel_frame(frame)?;
        self.last_frame = Some(frame);
        Ok(())
    }

    fn write_neopixel_frame(&self, frame: LedFrame) -> Result<()> {
        let color = effective_color_for_order(frame.color, self.spec);
        info!(
            "neopixel '{}' (GPIO {}, leds: {}) <- rgb({}, {}, {})",
            self.spec.id, self.spec.pin, self.spec.led_count, color.r, color.g, color.b
        );
        Ok(())
    }
}

fn effective_color_for_order(color: RgbColor, spec: &GeneratedLedRuntimeSpec) -> RgbColor {
    match spec.color_order {
        crate::config::GeneratedNeopixelColorOrder::Rgb => RgbColor::new(color.g, color.r, color.b),
        crate::config::GeneratedNeopixelColorOrder::Grb => color,
    }
}
