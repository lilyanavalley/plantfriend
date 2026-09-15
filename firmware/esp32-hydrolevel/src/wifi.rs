// src/wifi.rs – Wi-Fi station connection using esp-idf-svc

use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::peripheral::Peripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::info;

use crate::config::WifiConfig;

/// Connect to Wi-Fi as a station and block until an IP address is obtained.
///
/// Returns a `BlockingWifi` handle that must be kept alive for the lifetime of
/// the program.  Dropping it disconnects the interface.
///
/// # Type parameters
/// * `'d` – lifetime tied to the modem peripheral, typically the lifetime of
///           `Peripherals` which lives in `main()`.
pub fn connect<'d>(
    modem: impl Peripheral<P = Modem> + 'd,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    config: &WifiConfig,
) -> Result<BlockingWifi<EspWifi<'d>>> {
    info!("Connecting to Wi-Fi SSID: {}", config.ssid);

    let esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;
    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

    let wifi_config = Configuration::Client(ClientConfiguration {
        ssid: config
            .ssid
            .try_into()
            .map_err(|_| anyhow::anyhow!("SSID too long (max 32 bytes)"))?,
        password: config
            .password
            .try_into()
            .map_err(|_| anyhow::anyhow!("Wi-Fi password too long (max 64 bytes)"))?,
        auth_method: if config.password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    });

    wifi.set_configuration(&wifi_config)?;
    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().sta_netif().get_ip_info()?;
    info!("Wi-Fi connected. IP: {}", ip.ip);

    Ok(wifi)
}
