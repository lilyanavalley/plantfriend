use core::cell::Cell;

use plantfriend_core::publish::MonotonicClock;

/// Simple controllable monotonic clock for host-side integration tests.
#[derive(Debug)]
pub struct MockClock {
    now_ms: Cell<u64>,
}

impl MockClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: Cell::new(now_ms),
        }
    }

    pub fn set_ms(&self, now_ms: u64) {
        self.now_ms.set(now_ms);
    }

    pub fn advance_ms(&self, delta_ms: u64) {
        self.now_ms
            .set(self.now_ms.get().saturating_add(delta_ms));
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new(0)
    }
}

impl MonotonicClock for MockClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.get()
    }
}
