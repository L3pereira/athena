use crate::domain::{Clock, ControllableClock, TimeScale, Timestamp};
use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use std::sync::Arc;

/// Simulation clock implementation for testing and simulation
///
/// This clock can be:
/// - Run in real-time
/// - Accelerated (faster than real-time)
/// - Slowed down
/// - Fixed (manual time advancement only)
///
/// Thread-safe and can be shared across the trading infrastructure.
#[derive(Debug)]
pub struct SimulationClock {
    inner: Arc<RwLock<ClockState>>,
}

#[derive(Debug)]
struct ClockState {
    /// The reference point in simulated time
    simulated_time: DateTime<Utc>,
    /// The wall clock time when simulation started/was last reset
    wall_clock_reference: DateTime<Utc>,
    /// Current time scale
    time_scale: TimeScale,
}

impl SimulationClock {
    pub fn new() -> Self {
        let now = Utc::now();
        SimulationClock {
            inner: Arc::new(RwLock::new(ClockState {
                simulated_time: now,
                wall_clock_reference: now,
                time_scale: TimeScale::RealTime,
            })),
        }
    }

    /// Create a clock starting at a specific time (in Fixed mode)
    pub fn at(time: DateTime<Utc>) -> Self {
        SimulationClock {
            inner: Arc::new(RwLock::new(ClockState {
                simulated_time: time,
                wall_clock_reference: Utc::now(),
                time_scale: TimeScale::Fixed,
            })),
        }
    }

    /// Create a clock in fixed mode at current time
    pub fn fixed() -> Self {
        let clock = Self::new();
        clock.set_time_scale(TimeScale::Fixed);
        clock
    }

    /// Create a fast clock (for simulation)
    pub fn fast(multiplier: f64) -> Self {
        let clock = Self::new();
        clock.set_time_scale(TimeScale::Fast(multiplier));
        clock
    }
}

impl Default for SimulationClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SimulationClock {
    fn clone(&self) -> Self {
        SimulationClock {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Clock for SimulationClock {
    fn now(&self) -> Timestamp {
        let state = self.inner.read();

        match state.time_scale {
            TimeScale::Fixed => state.simulated_time,
            TimeScale::RealTime => {
                let elapsed = Utc::now() - state.wall_clock_reference;
                state.simulated_time + elapsed
            }
            TimeScale::Fast(multiplier) => {
                let elapsed = Utc::now() - state.wall_clock_reference;
                let scaled_nanos =
                    (elapsed.num_nanoseconds().unwrap_or(0) as f64 * multiplier) as i64;
                let scaled_elapsed = Duration::nanoseconds(scaled_nanos);
                state.simulated_time + scaled_elapsed
            }
            TimeScale::Slow(multiplier) => {
                let elapsed = Utc::now() - state.wall_clock_reference;
                let scaled_nanos =
                    (elapsed.num_nanoseconds().unwrap_or(0) as f64 * multiplier) as i64;
                let scaled_elapsed = Duration::nanoseconds(scaled_nanos);
                state.simulated_time + scaled_elapsed
            }
        }
    }
}

impl ControllableClock for SimulationClock {
    fn set_time_scale(&self, scale: TimeScale) {
        let mut state = self.inner.write();

        // Capture current simulated time before changing scale
        let current_time = match state.time_scale {
            TimeScale::Fixed => state.simulated_time,
            TimeScale::RealTime => {
                let elapsed = Utc::now() - state.wall_clock_reference;
                state.simulated_time + elapsed
            }
            TimeScale::Fast(multiplier) | TimeScale::Slow(multiplier) => {
                let elapsed = Utc::now() - state.wall_clock_reference;
                let scaled_nanos =
                    (elapsed.num_nanoseconds().unwrap_or(0) as f64 * multiplier) as i64;
                let scaled_elapsed = Duration::nanoseconds(scaled_nanos);
                state.simulated_time + scaled_elapsed
            }
        };

        state.simulated_time = current_time;
        state.wall_clock_reference = Utc::now();
        state.time_scale = scale;
    }

    fn time_scale(&self) -> TimeScale {
        self.inner.read().time_scale
    }

    fn advance(&self, duration: Duration) {
        let mut state = self.inner.write();
        state.simulated_time += duration;
        state.wall_clock_reference = Utc::now();
    }

    fn set_time(&self, time: Timestamp) {
        let mut state = self.inner.write();
        state.simulated_time = time;
        state.wall_clock_reference = Utc::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_time_does_not_advance() {
        let clock = SimulationClock::fixed();
        let t1 = clock.now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = clock.now();
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_advance_time() {
        let clock = SimulationClock::fixed();
        let t1 = clock.now();
        clock.advance(Duration::seconds(60));
        let t2 = clock.now();
        assert_eq!((t2 - t1).num_seconds(), 60);
    }

    #[test]
    fn test_set_time() {
        let clock = SimulationClock::fixed(); // Start in fixed mode
        let target = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        clock.set_time(target);

        assert_eq!(clock.now(), target);
    }

    #[test]
    fn test_clone_shares_state() {
        let clock1 = SimulationClock::fixed();
        let clock2 = clock1.clone();

        clock1.advance(Duration::seconds(100));

        assert_eq!(clock1.now(), clock2.now());
    }
}
