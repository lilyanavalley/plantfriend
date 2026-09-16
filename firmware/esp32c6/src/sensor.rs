// src/sensor.rs – XKC-Y25-NPN capacitive liquid level sensor driver
//
// The XKC-Y25-NPN sensor has an NPN open-collector digital output:
//   • Output LOW (pulled to GND) → liquid detected (sensor activated)
//   • Output HIGH (pulled up)   → no liquid (sensor not activated)
//
// The active polarity can be inverted via `SensorConfig::active_high` to
// support other sensor variants or inverted wiring.
//
// Reading the sensor in a loop with a software debounce is intentionally simple
// and compatible with the single-core ESP32-C3.  Future sensors (e.g., DS18B20
// temperature, DHT22 humidity, analog pH probe) can be added as additional
// modules without modifying this file.

use std::time::Instant;

use esp_idf_svc::hal::gpio::{Input, PinDriver, Pull};
use log::debug;
use plantfriend_core::sensors::{LiquidLevelDebouncer, LiquidState};

use crate::config::SensorConfig;

/// Driver for the digital capacitive liquid level sensor.
pub struct LiquidLevelSensor<'d> {
    pin: PinDriver<'d, esp_idf_svc::hal::gpio::AnyInputPin, Input>,
    active_high: bool,
    started_at: Instant,
    debouncer: LiquidLevelDebouncer,
}

impl<'d> LiquidLevelSensor<'d> {
    /// Initialise the sensor driver.
    ///
    /// # Arguments
    /// * `pin`    – GPIO pin configured as floating input (pull-up applied here).
    /// * `config` – Sensor section from the firmware configuration.
    pub fn new(
        pin: esp_idf_svc::hal::gpio::AnyInputPin,
        config: &SensorConfig,
    ) -> anyhow::Result<Self> {
        let mut driver = PinDriver::input(pin)?;

        // The XKC-Y25-NPN has an NPN open-collector output.  A pull-up keeps
        // the line HIGH when the sensor is not activated.
        driver.set_pull(Pull::Up)?;

        let initial = Self::read_raw(&driver, config.logic.active_high);
        debug!("Sensor initial state: {:?}", initial);

        Ok(Self {
            pin: driver,
            active_high: config.logic.active_high,
            started_at: Instant::now(),
            debouncer: LiquidLevelDebouncer::new(initial, config.logic.debounce_ms),
        })
    }

    /// Poll the sensor and return the new stable state if it has changed.
    ///
    /// Returns `Some(state)` on a debounced state transition, `None` otherwise.
    pub fn poll(&mut self) -> Option<LiquidState> {
        let raw = Self::read_raw(&self.pin, self.active_high);
        let now_ms = self.started_at.elapsed().as_millis() as u64;

        if let Some(stable) = self.debouncer.update(raw, now_ms) {
            debug!("Sensor state changed: {:?}", stable);
            return Some(stable);
        }

        None
    }

    /// Return the current debounced (stable) state without triggering a change
    /// event.  Useful for the initial publish on startup.
    pub fn state(&self) -> LiquidState {
        self.debouncer.stable()
    }

    // ── private ──────────────────────────────────────────────────────────────

    fn read_raw(
        pin: &PinDriver<'_, esp_idf_svc::hal::gpio::AnyInputPin, Input>,
        active_high: bool,
    ) -> LiquidState {
        let level = pin.is_high();
        if level == active_high {
            LiquidState::Present
        } else {
            LiquidState::Absent
        }
    }
}
