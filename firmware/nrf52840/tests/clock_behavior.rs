mod support {
    pub mod mock_time;
}

use plantfriend_core::publish::MonotonicClock;

use support::mock_time::MockClock;

#[test]
fn mock_clock_supports_set_and_advance() {
    let clock = MockClock::default();
    assert_eq!(clock.now_ms(), 0);

    clock.set_ms(250);
    assert_eq!(clock.now_ms(), 250);

    clock.advance_ms(100);
    assert_eq!(clock.now_ms(), 350);
}
