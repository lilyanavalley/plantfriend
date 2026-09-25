// src/sensor.rs – Digital-input binary sensor drivers
//
// The XKC-Y25-NPN sensor has an NPN open-collector digital output:
//   • Output LOW (pulled to GND) → liquid detected (sensor activated)
//   • Output HIGH (pulled up)   → no liquid (sensor not activated)
//
// The active polarity can be inverted to support different digital sensors
// (XKC, float switch variants, inverted wiring, etc.).
//
// Reading the sensor in a loop with a software debounce is intentionally simple
// and compatible with the single-core ESP32-C6.  Future sensors (e.g., DS18B20
// temperature, DHT22 humidity, analog pH probe) can be added as additional
// modules without modifying this file.

use std::time::Instant;
use std::fmt::Debug;

use esp_idf_svc::hal::gpio::{AnyInputPin, Input, PinDriver, Pull};
use log::debug;
use plantfriend_core::sensors::{
    decode_digital_input_state, DigitalInputDebouncer, DigitalInputSensorConfig,
    DigitalSignalState, FloatSwitchState, LiquidState,
};

/// Generic driver for digital input sensors with debounced binary state.
pub struct DigitalInputSensor<'d, S>
where
    S: DigitalSignalState + Debug,
{
    pin: PinDriver<'d, Input>,
    active_high: bool,
    started_at: Instant,
    debouncer: DigitalInputDebouncer<S>,
}

impl<'d, S> DigitalInputSensor<'d, S>
where
    S: DigitalSignalState + Debug,
{
    /// Initialise the sensor driver from explicit digital input logic settings.
    pub fn new_with_logic(
        pin: AnyInputPin<'d>,
        logic: DigitalInputSensorConfig,
    ) -> anyhow::Result<Self> {
        // Many digital sensors expose an open-collector/open-drain style output.
        // Pull-up keeps the line in a defined idle state.
        let driver = PinDriver::input(pin, Pull::Up)?;

        let initial = Self::read_raw(&driver, logic.active_high);
        debug!("Sensor initial state: {:?}", initial);

        Ok(Self {
            pin: driver,
            active_high: logic.active_high,
            started_at: Instant::now(),
            debouncer: DigitalInputDebouncer::new(initial, logic.debounce_ms),
        })
    }

    /// Backward-compatible constructor for legacy single-sensor config.
    ///
    /// # Arguments
    /// * `pin`    – GPIO pin configured as floating input (pull-up applied here).
    /// * `active_high` – Whether a HIGH level represents sensor active.
    /// * `debounce_ms` – Software debounce window in milliseconds.
    pub fn new(pin: AnyInputPin<'d>, active_high: bool, debounce_ms: u64) -> anyhow::Result<Self> {
        Self::new_with_logic(
            pin,
            DigitalInputSensorConfig {
                active_high,
                debounce_ms,
            },
        )
    }

    /// Initialise the sensor driver.
    ///
    /// # Arguments
    /// * `pin`    – GPIO pin configured as floating input (pull-up applied here).
    /// * `logic` – Shared digital input behavior settings.
    pub fn from_logic_config(
        pin: AnyInputPin<'d>,
        logic: &DigitalInputSensorConfig,
    ) -> anyhow::Result<Self> {
        Self::new_with_logic(pin, *logic)
    }

    /// Poll the sensor and return the new stable state if it has changed.
    ///
    /// Returns `Some(state)` on a debounced state transition, `None` otherwise.
    pub fn poll(&mut self) -> Option<S> {
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
    pub fn state(&self) -> S {
        self.debouncer.stable()
    }

    // ── private ──────────────────────────────────────────────────────────────

    fn read_raw(pin: &PinDriver<'_, Input>, active_high: bool) -> S {
        let level = pin.is_high();
        decode_digital_input_state(level, active_high)
    }
}

pub type LiquidLevelSensor<'d> = DigitalInputSensor<'d, LiquidState>;
pub type FloatSwitchSensor<'d> = DigitalInputSensor<'d, FloatSwitchState>;
