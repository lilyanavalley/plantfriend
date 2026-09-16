#![cfg_attr(all(not(test), target_os = "none"), no_std)]
#![cfg_attr(all(not(test), target_os = "none"), no_main)]

use plantfriend_core::protocol::RadioProtocol;
use plantfriend_core::sensors::LiquidState;

fn selected_protocol() -> RadioProtocol {
    if cfg!(feature = "custom-ble") {
        RadioProtocol::BleCustom
    } else {
        RadioProtocol::BleBTHome
    }
}

#[cfg(all(not(test), target_os = "none"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    let protocol = selected_protocol();
    let sample = LiquidState::Absent;

    let _payload = if matches!(protocol, RadioProtocol::BleBTHome) {
        Some(plantfriend_core::bthome::liquid_level_advertisement(
            sample, false,
        ))
    } else {
        None
    };

    loop {}
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {

    let protocol = selected_protocol();
    let sample = LiquidState::Absent;

    let _payload = if matches!(protocol, RadioProtocol::BleBTHome) {
        Some(plantfriend_core::bthome::liquid_level_advertisement(
            sample, false,
        ))
    } else {
        None
    };

    println!("nRF52 scaffold active with protocol: {:?}", protocol);
}
