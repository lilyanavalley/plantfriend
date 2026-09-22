# plantfriend / plantfriend firmware workspace

Modular embedded firmware workspace for a plantfriend sensor hub that targets multiple chip families while sharing a common core.

## Workspace layout

- `crates/plantfriend-core` — chip-agnostic domain models and protocol abstractions
- `firmware/esp32c6` — ESP32 firmware (Wi-Fi + MQTT + Home Assistant discovery)
- `firmware/nrf52840` — nRF52 scaffold wired to the same core, with BLE mode selection

## Core goals

- Reuse one core library across supported chips
- Keep chip/radio specifics in package-local firmware crates
- Support Home Assistant integrations via chip-specific transports
- BLE path supports either:
  - BTHome-oriented payloads
  - custom BLE payloads

## Current implementation status

### `plantfriend-core`

Includes:
- shared liquid-level state model
- shared sensor-hub traits for future multi-sensor snapshots
- shared radio protocol enum (`WiFi+MQTT`, `BLE BTHome`, `BLE custom`, `LoRa`)
- BTHome-style binary sensor payload helper for BLE advertisement data

### ESP32 package

The previous single-crate firmware now lives in:
`firmware/esp32c6`

Features:
- digital capacitive liquid-level sensing
- optional OTA on boot
- MQTT 3.1.1 / MQTT 5 publishing
- TLS / mTLS MQTT support
- Home Assistant MQTT auto-discovery

### nRF52 package

Scaffold package:
`firmware/nrf52840`

Currently provides:
- feature-gated protocol selection (`bthome` default, `custom-ble` optional)
- wiring to `plantfriend-core` state + BTHome helper

## Build

From workspace root:

```sh
cargo build --workspace --release
```

Build a specific package:

```sh
cargo build -p esp32c6 --target riscv32imac-esp-espidf --release
cargo build -p nrf52840 --release
cargo build -p plantfriend-core --release
```

## Flashing ESP32-C6

```sh
cargo run -p esp32c6 --target riscv32imac-esp-espidf --release
# or
espflash flash --monitor target/riscv32imac-esp-espidf/release/esp32c6
```

## ESP32 configuration

Copy and edit:

```sh
cp firmware/esp32c6/.env.example firmware/esp32c6/.env
$EDITOR firmware/esp32c6/.env
```

All `PLANTFRIEND_*` values are loaded at build time for the ESP32 package.

## Flashing nRF52840

```sh
cargo run -p nrf52840 --release
# or 
espflash flash --monitor target/thumbv7em-none-eabihf
```

## Notes

- LoRa is represented in core protocol abstractions as a future transport.
- The nRF52 package is intentionally scaffold-first to allow selecting final HAL/stack choices without changing shared domain logic.
