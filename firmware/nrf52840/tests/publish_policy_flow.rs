mod support {
    pub mod mock_time;
}

use plantfriend_core::publish::{PublishReason, StatePublishPolicy};
use plantfriend_core::sensors::LiquidState;

use support::mock_time::MockClock;

#[test]
fn publish_policy_flow_with_mock_clock() {
    let clock = MockClock::new(0);
    clock.set_ms(0);
    let mut policy = StatePublishPolicy::with_clock(LiquidState::Absent, Some(1000), &clock);

    let initial = policy.initial_event().expect("expected initial event");
    assert_eq!(initial.reason, PublishReason::Initial);
    assert_eq!(initial.state, LiquidState::Absent);

    clock.advance_ms(500);
    assert_eq!(policy.on_tick_with_clock(&clock), None);

    let changed = policy
        .on_state_change(LiquidState::Present)
        .expect("expected state-change event");
    assert_eq!(changed.reason, PublishReason::StateChanged);
    assert_eq!(changed.state, LiquidState::Present);

    clock.advance_ms(500);
    let heartbeat = policy
        .on_tick_with_clock(&clock)
        .expect("expected heartbeat event");
    assert_eq!(heartbeat.reason, PublishReason::Heartbeat);
    assert_eq!(heartbeat.state, LiquidState::Present);
}

#[test]
fn publish_policy_no_heartbeat_when_interval_is_disabled() {
    let clock = MockClock::new(0);
    let mut policy = StatePublishPolicy::with_clock(LiquidState::Absent, None, &clock);

    let initial = policy.initial_event().expect("expected initial event");
    assert_eq!(initial.reason, PublishReason::Initial);
    assert_eq!(initial.state, LiquidState::Absent);

    clock.advance_ms(10_000);
    assert_eq!(policy.on_tick_with_clock(&clock), None);

    clock.advance_ms(10_000);
    assert_eq!(policy.on_tick_with_clock(&clock), None);
}
