use plantminder_core::protocol::RadioProtocol;
use plantminder_core::sensors::LiquidState;

fn selected_protocol() -> RadioProtocol {
    if cfg!(feature = "custom-ble") {
        RadioProtocol::BleCustom
    } else {
        RadioProtocol::BleBTHome
    }
}

fn main() {
    let protocol = selected_protocol();
    let sample = LiquidState::Absent;

    let _payload = if matches!(protocol, RadioProtocol::BleBTHome) {
        Some(plantminder_core::bthome::liquid_level_advertisement(
            sample, false,
        ))
    } else {
        None
    };

    println!("nRF52 scaffold active with protocol: {:?}", protocol);
}
