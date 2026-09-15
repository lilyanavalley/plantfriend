use anyhow::{bail, Result};
use embedded_svc::http::{client::Client as HttpClient, Method};
use embedded_svc::io::Read as _;
use esp_idf_svc::hal::reset::restart;
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
use esp_idf_svc::ota::EspOta;
use log::info;

/// Downloads a firmware image from `url`, writes it to the next OTA slot, and
/// reboots into the updated image.
pub fn try_update_and_reboot(url: &str) -> Result<()> {
    info!("Starting OTA update from: {url}");

    let http_cfg = HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };

    let mut client = HttpClient::wrap(EspHttpConnection::new(&http_cfg)?);
    let request = client.request(Method::Get, url, &[])?;
    let mut response = request.submit()?;

    if response.status() != 200 {
        bail!("OTA HTTP request failed with status {}", response.status());
    }

    let mut ota = EspOta::new()?;
    let mut update = ota.initiate_update()?;
    let mut total = 0usize;
    let mut chunk = [0u8; 4096];

    loop {
        let read = response.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        update.write(&chunk[..read])?;
        total += read;
    }

    if total == 0 {
        let _ = update.abort();
        bail!("OTA payload was empty");
    }

    update.complete()?;
    info!("OTA update written successfully ({total} bytes), rebooting.");
    restart();
    unreachable!("restart() returned unexpectedly after OTA completion");
}
