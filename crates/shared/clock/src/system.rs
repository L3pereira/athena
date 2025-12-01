use athena_core::Timestamp;
use athena_ports::Clock;
use chrono::Utc;

/// Real system clock for production use
///
/// This simply returns the current wall-clock time.
/// Use this in production where you want real-time behavior.
pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }

    fn name(&self) -> &str {
        "SystemClock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::thread;

    #[test]
    fn test_system_clock_advances() {
        let clock = SystemClock::new();
        let time1 = clock.now();
        thread::sleep(std::time::Duration::from_millis(10));
        let time2 = clock.now();

        assert!(time2 > time1);
        let diff = time2 - time1;
        assert!(diff >= Duration::milliseconds(9));
    }
}
