#[allow(dead_code)]
#[path = "../../../firmware/esp32c6/build_config.rs"]
mod build_config;

pub use build_config::SensorRuntimeDriverKind;

#[cfg(test)]
mod tests {
    use super::SensorRuntimeDriverKind;

    #[test]
    fn maps_xkc_y25_kind() {
        let mapped = SensorRuntimeDriverKind::from_device_kind("xkc_y25")
            .expect("xkc_y25 should map successfully");
        assert_eq!(mapped, SensorRuntimeDriverKind::XkcY25);
        assert_eq!(
            mapped.generated_sensor_kind_variant(),
            "GeneratedSensorKind::XkcY25"
        );
    }

    #[test]
    fn maps_basic_float_kind() {
        let mapped = SensorRuntimeDriverKind::from_device_kind("basic_float")
            .expect("basic_float should map successfully");
        assert_eq!(mapped, SensorRuntimeDriverKind::BasicFloat);
        assert_eq!(
            mapped.generated_sensor_kind_variant(),
            "GeneratedSensorKind::BasicFloat"
        );
    }

    #[test]
    fn rejects_unknown_kind() {
        let err = SensorRuntimeDriverKind::from_device_kind("ultrasonic")
            .expect_err("unknown kind should fail");
        assert!(
            err.contains("Unsupported sensor kind"),
            "unexpected err: {err}"
        );
    }
}
