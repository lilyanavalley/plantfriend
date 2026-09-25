// src/main.rs – hydrolevel firmware entry point
//
// Architecture:
//   main() → initialise ESP-IDF peripherals
//          → connect Wi-Fi
//          → connect MQTT
//          → publish HA discovery + initial state
//          → event loop: poll sensor, publish on change or heartbeat

mod config;
mod mqtt;
mod ota;
mod sensor;
mod wifi;

// Future sensor modules can be added here, e.g.:
// mod temperature;   // DS18B20 / NTC
// mod humidity;      // DHT22 / SHT31
// mod ph;            // Analog pH probe via ADC

use std::time::{Duration, Instant};

use anyhow::Result;
use esp_idf_svc::hal::gpio::AnyInputPin;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition};
use log::{error, info, warn};
use plantfriend_core::publish::{MonotonicClock, PublishReason, StatePublishPolicy};
use plantfriend_core::sensors::{FloatSwitchState, LiquidState};

use config::{Config, GeneratedSensorKind, GeneratedSensorRuntimeSpec};
use mqtt::MqttManager;
use sensor::{FloatSwitchSensor, LiquidLevelSensor};

enum ActiveSensor<'d> {
    XkcY25 {
        spec: &'static GeneratedSensorRuntimeSpec,
        sensor: LiquidLevelSensor<'d>,
        publish_policy: StatePublishPolicy<LiquidState>,
    },
    BasicFloat {
        spec: &'static GeneratedSensorRuntimeSpec,
        sensor: FloatSwitchSensor<'d>,
        publish_policy: StatePublishPolicy<FloatSwitchState>,
    },
}

impl<'d> ActiveSensor<'d> {
    fn publish_initial(&mut self, mqtt: &mut MqttManager) -> Result<()> {
        match self {
            ActiveSensor::XkcY25 {
                spec,
                publish_policy,
                ..
            } => {
                if let Some(initial) = publish_policy.initial_event() {
                    mqtt.publish_state_for_sensor(spec.id, initial.state)?;
                }
            }
            ActiveSensor::BasicFloat {
                spec,
                publish_policy,
                ..
            } => {
                if let Some(initial) = publish_policy.initial_event() {
                    mqtt.publish_state_for_sensor(spec.id, initial.state)?;
                }
            }
        }

        Ok(())
    }

    fn poll_and_publish_state(&mut self, mqtt: &mut MqttManager, uptime_clock: &UptimeClock) {
        match self {
            ActiveSensor::XkcY25 {
                spec,
                sensor,
                publish_policy,
            } => {
                if let Some(new_state) = sensor.poll() {
                    if let Some(event) = publish_policy.on_state_change(new_state) {
                        if let Err(e) = mqtt.publish_state_for_sensor(spec.id, event.state) {
                            error!(
                                "Failed to publish sensor '{}' state update ({:?}): {e}",
                                spec.id, event.reason
                            );
                        }
                    }
                }

                if let Some(event) = publish_policy.on_tick_with_clock(uptime_clock) {
                    if matches!(event.reason, PublishReason::Heartbeat) {
                        if let Err(e) = mqtt.publish_state_for_sensor(spec.id, event.state) {
                            warn!("Heartbeat publish failed for sensor '{}': {e}", spec.id);
                        }
                    }
                }
            }
            ActiveSensor::BasicFloat {
                spec,
                sensor,
                publish_policy,
            } => {
                if let Some(new_state) = sensor.poll() {
                    if let Some(event) = publish_policy.on_state_change(new_state) {
                        if let Err(e) = mqtt.publish_state_for_sensor(spec.id, event.state) {
                            error!(
                                "Failed to publish sensor '{}' state update ({:?}): {e}",
                                spec.id, event.reason
                            );
                        }
                    }
                }

                if let Some(event) = publish_policy.on_tick_with_clock(uptime_clock) {
                    if matches!(event.reason, PublishReason::Heartbeat) {
                        if let Err(e) = mqtt.publish_state_for_sensor(spec.id, event.state) {
                            warn!("Heartbeat publish failed for sensor '{}': {e}", spec.id);
                        }
                    }
                }
            }
        }
    }
}

struct UptimeClock {
    started_at: Instant,
}

impl UptimeClock {
    fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl MonotonicClock for UptimeClock {
    fn now_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }
}

fn main() -> Result<()> {
    // ── ESP-IDF initialisation ────────────────────────────────────────────────
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // ── Load compile-time configuration ──────────────────────────────────────
    let cfg = Config::load();

    info!(
        "hydrolevel starting – device: {}, broker: {}",
        cfg.ha.device_id, cfg.mqtt.broker_uri
    );

    // ── Wi-Fi ─────────────────────────────────────────────────────────────────
    // `_wifi` must remain alive for the duration of the program to keep the
    // Wi-Fi interface active.
    let _wifi = wifi::connect(peripherals.modem, sysloop, nvs, &cfg.wifi)?;

    if cfg.ota.auto_apply_on_boot {
        if let Some(url) = cfg.ota.firmware_url {
            if let Err(e) = ota::try_update_and_reboot(url) {
                warn!("OTA update failed, continuing with current firmware: {e}");
            }
        } else {
            warn!("PLANTFRIEND_OTA_AUTO_APPLY is true but PLANTFRIEND_OTA_URL is empty");
        }
    } else if cfg.ota.firmware_url.is_some() {
        info!("PLANTFRIEND_OTA_URL is configured but auto-apply is disabled");
    }

    // ── MQTT ──────────────────────────────────────────────────────────────────
    let mut mqtt = MqttManager::connect(&cfg)?;

    // Give the broker a moment to process the connection before publishing.
    std::thread::sleep(Duration::from_millis(500));

    // Publish HA auto-discovery so the entity appears in Home Assistant.
    mqtt.publish_discovery()?;
    // Announce the device as online.
    mqtt.publish_online()?;

    // ── Main event loop ───────────────────────────────────────────────────────
    let heartbeat_interval_ms = if cfg.publish.interval_ms > 0 {
        Some(cfg.publish.interval_ms)
    } else {
        None
    };

    let uptime_clock = UptimeClock::new();

    let mut sensors: Vec<ActiveSensor<'static>> = Vec::with_capacity(cfg.runtime.sensors.len());
    for spec in cfg.runtime.sensors {
        // SAFETY: We own `peripherals` exclusively (taken above). The build-time
        // validation guarantees unique GPIO assignment in the sensor list.
        let pin = unsafe { AnyInputPin::steal(spec.pin as u8) };
        match spec.kind {
            GeneratedSensorKind::XkcY25 => {
                let sensor =
                    LiquidLevelSensor::new(pin, spec.logic.active_high, spec.logic.debounce_ms)?;

                info!(
                    "Configured xkc_y25 sensor '{}' on GPIO {}",
                    spec.id, spec.pin
                );

                sensors.push(ActiveSensor::XkcY25 {
                    spec,
                    publish_policy: StatePublishPolicy::with_clock(
                        sensor.state(),
                        heartbeat_interval_ms,
                        &uptime_clock,
                    ),
                    sensor,
                });
            }
            GeneratedSensorKind::BasicFloat => {
                let sensor =
                    FloatSwitchSensor::new(pin, spec.logic.active_high, spec.logic.debounce_ms)?;

                info!(
                    "Configured basic_float sensor '{}' on GPIO {}",
                    spec.id, spec.pin
                );

                sensors.push(ActiveSensor::BasicFloat {
                    spec,
                    publish_policy: StatePublishPolicy::with_clock(
                        sensor.state(),
                        heartbeat_interval_ms,
                        &uptime_clock,
                    ),
                    sensor,
                });
            }
        }
    }

    // Publish initial sensor states so HA doesn't show "unavailable".
    for runtime in &mut sensors {
        runtime.publish_initial(&mut mqtt)?;
    }

    info!("Entering main loop");
    loop {
        for runtime in &mut sensors {
            runtime.poll_and_publish_state(&mut mqtt, &uptime_clock);
        }

        // Yield to the ESP-IDF scheduler; prevents starving the Wi-Fi/MQTT stack.
        std::thread::sleep(Duration::from_millis(10));
    }
}
