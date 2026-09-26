// src/wifi.rs – Wi-Fi station connection with AP-based pairing portal

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use embedded_svc::http::{Headers, Method};
use embedded_svc::io::{Read, Write};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::http::server::{Configuration as HttpServerConfiguration, EspHttpServer};
use esp_idf_svc::nvs::{EspDefaultNvs, EspDefaultNvsPartition, EspNvs};
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi,
};
use log::{info, warn};

use crate::config::Config;

const WIFI_NAMESPACE: &str = "wifi_pairing";
const WIFI_SSID_KEY: &str = "ssid";
const WIFI_PASSWORD_KEY: &str = "password";
const MAX_FORM_BODY_BYTES: usize = 512;

#[derive(Clone, Debug)]
struct WifiCredentials {
    ssid: String,
    password: String,
}

impl WifiCredentials {
    fn new(ssid: String, password: String) -> Result<Self> {
        if ssid.is_empty() {
            bail!("Wi-Fi SSID cannot be empty");
        }
        if ssid.contains('\0') || password.contains('\0') {
            bail!("Wi-Fi credentials cannot contain NUL bytes");
        }
        if ssid.len() > 32 {
            bail!("Wi-Fi SSID is too long (max 32 bytes)");
        }
        if password.len() > 64 {
            bail!("Wi-Fi password is too long (max 64 bytes)");
        }
        Ok(Self { ssid, password })
    }
}

pub fn connect<'d>(
    modem: impl WifiModemPeripheral + 'd,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    config: &Config,
) -> Result<BlockingWifi<EspWifi<'d>>> {
    let esp_wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs.clone()))?;
    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

    if let Some(saved) = load_paired_credentials(&nvs)? {
        info!("Found paired Wi-Fi credentials in NVS, connecting as station");
        match connect_station(&mut wifi, &saved) {
            Ok(()) => return Ok(wifi),
            Err(err) => warn!("Stored Wi-Fi credentials failed: {err}"),
        }
    } else if let Some(bootstrap) = bootstrap_credentials(config)? {
        info!("No paired Wi-Fi credentials in NVS, trying bootstrap credentials");
        match connect_station(&mut wifi, &bootstrap) {
            Ok(()) => {
                persist_paired_credentials(&nvs, &bootstrap)?;
                return Ok(wifi);
            }
            Err(err) => warn!("Bootstrap Wi-Fi credentials failed: {err}"),
        }
    }

    info!("Entering Wi-Fi pairing mode");
    let paired = run_pairing_portal(&mut wifi, &nvs, config)?;
    connect_station(&mut wifi, &paired)?;

    Ok(wifi)
}

fn bootstrap_credentials(config: &Config) -> Result<Option<WifiCredentials>> {
    match (config.wifi.bootstrap_ssid, config.wifi.bootstrap_password) {
        (Some(ssid), Some(password)) => {
            WifiCredentials::new(ssid.to_owned(), password.to_owned()).map(Some)
        }
        (Some(ssid), None) => WifiCredentials::new(ssid.to_owned(), String::new()).map(Some),
        (None, Some(_)) => bail!(
            "PLANTFRIEND_WIFI_BOOTSTRAP_PASSWORD is set but PLANTFRIEND_WIFI_BOOTSTRAP_SSID is missing"
        ),
        (None, None) => Ok(None),
    }
}

fn connect_station(wifi: &mut BlockingWifi<EspWifi<'_>>, creds: &WifiCredentials) -> Result<()> {
    if wifi.is_started()? {
        wifi.stop()?;
    }

    info!("Connecting to Wi-Fi SSID: {}", creds.ssid);

    let wifi_config = Configuration::Client(ClientConfiguration {
        ssid: creds
            .ssid
            .as_str()
            .try_into()
            .map_err(|_| anyhow!("SSID too long (max 32 bytes)"))?,
        password: creds
            .password
            .as_str()
            .try_into()
            .map_err(|_| anyhow!("Wi-Fi password too long (max 64 bytes)"))?,
        auth_method: if creds.password.is_empty() {
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

    Ok(())
}

fn run_pairing_portal(
    wifi: &mut BlockingWifi<EspWifi<'_>>,
    nvs_partition: &EspDefaultNvsPartition,
    config: &Config,
) -> Result<WifiCredentials> {
    if !(1..=13).contains(&config.wifi.pairing_ap_channel) {
        bail!("PLANTFRIEND_PAIRING_AP_CHANNEL must be in range 1..=13");
    }
    if config.wifi.pairing_ap_max_connections == 0 {
        bail!("PLANTFRIEND_PAIRING_AP_MAX_CONNECTIONS must be greater than 0");
    }

    if wifi.is_started()? {
        wifi.stop()?;
    }

    let pairing_ssid = resolve_pairing_ssid(config)?;
    let ap_config = Configuration::AccessPoint(AccessPointConfiguration {
        ssid: pairing_ssid
            .as_str()
            .try_into()
            .map_err(|_| anyhow!("Pairing AP SSID too long (max 32 bytes)"))?,
        channel: config.wifi.pairing_ap_channel,
        auth_method: AuthMethod::None,
        password: "".try_into().expect("empty password is valid"),
        max_connections: config.wifi.pairing_ap_max_connections,
        ..Default::default()
    });

    wifi.set_configuration(&ap_config)?;
    wifi.start()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().ap_netif().get_ip_info()?.ip;
    info!("Pairing AP started: '{pairing_ssid}' at http://{ip}");

    let pending_credentials: Arc<Mutex<Option<WifiCredentials>>> = Arc::new(Mutex::new(None));

    let mut server = EspHttpServer::new(&HttpServerConfiguration {
        stack_size: 10_240,
        ..Default::default()
    })?;

    let form_html = pairing_form_html(&pairing_ssid);
    server.fn_handler("/", Method::Get, move |req| {
        req.into_ok_response()?.write_all(form_html.as_bytes())?;
        Ok(())
    })?;

    let pending_for_post = pending_credentials.clone();
    server.fn_handler::<anyhow::Error, _>("/configure", Method::Post, move |mut req| {
        let len = req.content_len().unwrap_or(0) as usize;
        if len == 0 {
            req.into_status_response(400)?
                .write_all(b"Missing request body")?;
            return Ok(());
        }

        if len > MAX_FORM_BODY_BYTES {
            req.into_status_response(413)?
                .write_all(b"Request body too large")?;
            return Ok(());
        }

        let mut body = vec![0u8; len];
        req.read_exact(&mut body)?;
        let body = std::str::from_utf8(&body).context("Invalid UTF-8 in form submission")?;
        let creds = parse_form_credentials(body)?;

        {
            let mut slot = pending_for_post
                .lock()
                .map_err(|_| anyhow!("Pairing state mutex was poisoned"))?;
            *slot = Some(creds);
        }

        req.into_ok_response()?.write_all(
            b"<html><body><h2>Credentials saved.</h2><p>Connecting to Wi-Fi...</p></body></html>",
        )?;
        Ok(())
    })?;

    loop {
        let maybe = {
            let mut guard = pending_credentials
                .lock()
                .map_err(|_| anyhow!("Pairing state mutex was poisoned"))?;
            guard.take()
        };

        if let Some(creds) = maybe {
            persist_paired_credentials(nvs_partition, &creds)?;
            info!("Paired Wi-Fi credentials stored in NVS");
            drop(server);
            return Ok(creds);
        }

        thread::sleep(Duration::from_millis(100));
    }
}

fn resolve_pairing_ssid(config: &Config) -> Result<String> {
    let raw = config
        .wifi
        .pairing_ap_ssid
        .unwrap_or(config.ha.device_id)
        .trim();
    if raw.is_empty() {
        bail!("Pairing AP SSID cannot be empty");
    }

    let bytes = raw.as_bytes();
    if bytes.len() > 32 {
        bail!("Pairing AP SSID is too long (max 32 bytes)");
    }
    Ok(raw.to_owned())
}

fn load_paired_credentials(
    nvs_partition: &EspDefaultNvsPartition,
) -> Result<Option<WifiCredentials>> {
    let nvs: EspDefaultNvs = EspNvs::new(nvs_partition.clone(), WIFI_NAMESPACE, true)?;
    let Some(ssid) = read_nvs_string(&nvs, WIFI_SSID_KEY)? else {
        return Ok(None);
    };

    let password = read_nvs_string(&nvs, WIFI_PASSWORD_KEY)?.unwrap_or_default();
    Ok(Some(WifiCredentials::new(ssid, password)?))
}

fn persist_paired_credentials(
    nvs_partition: &EspDefaultNvsPartition,
    creds: &WifiCredentials,
) -> Result<()> {
    let nvs: EspDefaultNvs = EspNvs::new(nvs_partition.clone(), WIFI_NAMESPACE, true)?;
    nvs.set_str(WIFI_SSID_KEY, &creds.ssid)?;
    nvs.set_str(WIFI_PASSWORD_KEY, &creds.password)?;
    Ok(())
}

fn read_nvs_string(nvs: &EspDefaultNvs, key: &str) -> Result<Option<String>> {
    let Some(len) = nvs.str_len(key)? else {
        return Ok(None);
    };

    let mut buf = vec![0u8; len];
    let value = nvs
        .get_str(key, &mut buf)?
        .ok_or_else(|| anyhow!("NVS key '{key}' disappeared while reading"))?;
    Ok(Some(value.to_owned()))
}

fn parse_form_credentials(body: &str) -> Result<WifiCredentials> {
    let mut ssid = None;
    let mut password = String::new();

    for pair in body.split('&') {
        let Some((raw_key, raw_value)) = pair.split_once('=') else {
            continue;
        };
        let key = url_decode(raw_key)?;
        let value = url_decode(raw_value)?;

        if key == "ssid" {
            ssid = Some(value);
        } else if key == "password" {
            password = value;
        }
    }

    let ssid = ssid.ok_or_else(|| anyhow!("Missing ssid field"))?;
    WifiCredentials::new(ssid, password)
}

fn url_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(value.len());
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                if i + 2 >= bytes.len() {
                    bail!("Malformed percent escape");
                }
                let hi = hex_value(bytes[i + 1])?;
                let lo = hex_value(bytes[i + 2])?;
                out.push((hi << 4) | lo);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }

    String::from_utf8(out).context("Form field is not valid UTF-8")
}

fn hex_value(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("Invalid hex digit"),
    }
}

fn pairing_form_html(ssid: &str) -> String {
    format!(
        r#"<!doctype html>
<html>
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1" />
  <title>Plantfriend Wi-Fi Pairing</title>
  <style>
    body {{ font-family: sans-serif; margin: 2rem; max-width: 480px; }}
    label {{ display: block; margin-top: 1rem; font-weight: 600; }}
    input {{ width: 100%; padding: 0.5rem; margin-top: 0.25rem; }}
    button {{ margin-top: 1.25rem; padding: 0.6rem 1rem; }}
  </style>
</head>
<body>
  <h1>Plantfriend Wi-Fi Pairing</h1>
  <p>Connected to setup network: <strong>{ssid}</strong></p>
  <form method="POST" action="/configure">
    <label for="ssid">Wi-Fi SSID</label>
    <input id="ssid" name="ssid" maxlength="32" required />
    <label for="password">Wi-Fi Password</label>
    <input id="password" name="password" type="password" maxlength="64" />
    <button type="submit">Save and connect</button>
  </form>
</body>
</html>"#
    )
}
