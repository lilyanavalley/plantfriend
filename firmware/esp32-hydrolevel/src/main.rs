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

use config::Config;
use mqtt::MqttManager;
use sensor::LiquidLevelSensor;

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

    // ── Sensor GPIO ───────────────────────────────────────────────────────────
    // Obtain a type-erased input pin for the configured GPIO number.
    // SAFETY: We own `peripherals` exclusively (taken above); this pin will
    // not be aliased elsewhere in this single-binary firmware.
    let sensor_pin: AnyInputPin = unsafe { AnyInputPin::new(cfg.sensor.gpio_pin as i32) };
    let mut sensor = LiquidLevelSensor::new(sensor_pin, &cfg.sensor)?;

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
            warn!("HYDROLEVEL_OTA_AUTO_APPLY is true but HYDROLEVEL_OTA_URL is empty");
        }
    } else if cfg.ota.firmware_url.is_some() {
        info!("HYDROLEVEL_OTA_URL is configured but auto-apply is disabled");
    }

    // ── MQTT ──────────────────────────────────────────────────────────────────
    let mut mqtt = MqttManager::connect(&cfg)?;

    // Give the broker a moment to process the connection before publishing.
    std::thread::sleep(Duration::from_millis(500));

    // Publish HA auto-discovery so the entity appears in Home Assistant.
    mqtt.publish_discovery()?;
    // Announce the device as online.
    mqtt.publish_online()?;
    // Publish the initial sensor state so HA doesn't show "unavailable".
    mqtt.publish_state(sensor.state())?;

    // ── Main event loop ───────────────────────────────────────────────────────
    let heartbeat_interval = if cfg.publish.interval_ms > 0 {
        Some(Duration::from_millis(cfg.publish.interval_ms))
    } else {
        None
    };
    let mut last_heartbeat = Instant::now();

    info!("Entering main loop");
    loop {
        // Poll sensor for debounced state change.
        if let Some(new_state) = sensor.poll() {
            if let Err(e) = mqtt.publish_state(new_state) {
                error!("Failed to publish state: {e}");
            }
        }

        // Periodic heartbeat publish keeps HA state fresh after broker restart.
        if let Some(interval) = heartbeat_interval {
            if last_heartbeat.elapsed() >= interval {
                if let Err(e) = mqtt.publish_state(sensor.state()) {
                    warn!("Heartbeat publish failed: {e}");
                }
                last_heartbeat = Instant::now();
            }
        }

        // Yield to the ESP-IDF scheduler; prevents starving the Wi-Fi/MQTT stack.
        std::thread::sleep(Duration::from_millis(10));
    }
}
