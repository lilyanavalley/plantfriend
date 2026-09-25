//! XKC-Y25-NPN capacitive liquid level sensor.
//! 
//! Detects liquid capacitance to determine presense of liquid in a non-contact application.
//! This type of sensor is particularly useful in sensing the level of water in a container so as to affirm
//! there is enough water to completely submerge a pump (or not overfill a reservoir...) The XKC-Y25 in
//! particular can be affixed to the outside of a liquid container to keep sensor dry and the liquid itself
//! undesturbed by the sensor.
//! 
//! This sensor comes from: https://www.xkc-sensor.com/detail/1432.html
//! 
//! Datasheet: https://jqrorwxhkmjqll5p-static.micyjz.com/XKC-Y25-PUB+INFO-EN-V16-aidlkBplKkmlrSRolprriiiko.pdf?dp=GvUApKfKKUAU


use super::{DigitalInputDebouncer, DigitalInputSensorConfig, LiquidState};


pub type DigitalInputLiquidSensorConfig = DigitalInputSensorConfig;
pub type LiquidLevelDebouncer = DigitalInputDebouncer<LiquidState>;

