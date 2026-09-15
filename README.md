# plantfriend / plantminder firmware workspace

Modular embedded firmware workspace for a Plantminder sensor hub that targets multiple chip families while sharing a common core.

## Workspace layout

- `crates/plantminder-core` — chip-agnostic domain models and protocol abstractions
- `firmware/esp32-hydrolevel` — ESP32-C3 firmware (Wi-Fi + MQTT + Home Assistant discovery)
- `firmware/nrf52-plantminder` — nRF52 scaffold wired to the same core, with BLE mode selection

## Core goals

- Reuse one core library across supported chips
- Keep chip/radio specifics in package-local firmware crates
- Support Home Assistant integrations via chip-specific transports
- BLE path supports either:
  - BTHome-oriented payloads
  - custom BLE payloads

## Current implementation status

### `plantminder-core`

Includes:
- shared liquid-level state model
- shared sensor-hub traits for future multi-sensor snapshots
- shared radio protocol enum (`WiFi+MQTT`, `BLE BTHome`, `BLE custom`, `LoRa`)
- BTHome-style binary sensor payload helper for BLE advertisement data

### ESP32 package

The previous single-crate firmware now lives in:
`/home/runner/work/plantfriend/plantfriend/firmware/esp32-hydrolevel`

Features:
- digital capacitive liquid-level sensing
- optional OTA on boot
- MQTT 3.1.1 / MQTT 5 publishing
- TLS / mTLS MQTT support
- Home Assistant MQTT auto-discovery

### nRF52 package

Scaffold package:
`/home/runner/work/plantfriend/plantfriend/firmware/nrf52-plantminder`

Currently provides:
- feature-gated protocol selection (`bthome` default, `custom-ble` optional)
- wiring to `plantminder-core` state + BTHome helper

## Build

From workspace root:

```sh
cargo build --release
```

Build a specific package:

```sh
cargo build -p hydrolevel --release
cargo build -p nrf52-plantminder --release
cargo build -p plantminder-core --release
```

## Flashing ESP32-C3

```sh
cargo run -p hydrolevel --release
# or
espflash flash --monitor target/riscv32imc-esp-espidf/release/hydrolevel
```

## ESP32 configuration

Copy and edit:

```sh
cp firmware/esp32-hydrolevel/.env.example firmware/esp32-hydrolevel/.env
$EDITOR firmware/esp32-hydrolevel/.env
```

All `HYDROLEVEL_*` values are loaded at build time for the ESP32 package.

## Notes

- LoRa is represented in core protocol abstractions as a future transport.
- The nRF52 package is intentionally scaffold-first to allow selecting final HAL/stack choices without changing shared domain logic.
