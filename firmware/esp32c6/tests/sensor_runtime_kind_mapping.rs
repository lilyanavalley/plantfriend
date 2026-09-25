#[path = "../build_config.rs"]
mod build_config;

use build_config::SensorRuntimeDriverKind;

#[test]
fn maps_device_sensor_kind_to_generated_runtime_kind() {
    assert_eq!(
        SensorRuntimeDriverKind::from_device_kind("xkc_y25")
            .expect("xkc kind should map")
            .generated_sensor_kind_variant(),
        "GeneratedSensorKind::XkcY25"
    );
    assert_eq!(
        SensorRuntimeDriverKind::from_device_kind("basic_float")
            .expect("basic float kind should map")
            .generated_sensor_kind_variant(),
        "GeneratedSensorKind::BasicFloat"
    );
}

#[test]
fn rejects_unknown_sensor_kind_with_explicit_error() {
    let err = SensorRuntimeDriverKind::from_device_kind("ultrasonic")
        .expect_err("unsupported kind should return an error");
    assert!(
        err.contains("Unsupported sensor kind"),
        "unexpected error: {err}"
    );
}
