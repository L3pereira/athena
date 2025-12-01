use athena_core::Timestamp;
use athena_ports::Clock;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Time scale modes for simulation
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TimeScale {
    /// Real-time (1:1 ratio with wall clock)
    #[default]
    Normal,
    /// Accelerated time (multiplier applied to elapsed time)
    Fast(u32),
    /// Decelerated time (divisor applied to elapsed time)
    Slow(u32),
    /// Fixed time (only advances when explicitly moved)
    Fixed,
}

/// Universal simulation clock - the source of truth for all time in the simulation
///
/// All other clocks (ExchangeClock, AgentTimeView) derive their time from this clock.
/// This enables consistent time across the entire simulation while allowing
/// time scaling for testing (fast-forward, slow-motion, or fixed time).
pub struct WorldClock {
    /// When the simulation started in real (wall) time
    start_real: Timestamp,
    /// The initial simulated time
    start_sim: Timestamp,
    /// Current time scale
    scale: RwLock<TimeScale>,
    /// Current simulation time (used in Fixed mode or as cache)
    current_time: RwLock<Timestamp>,
}

impl WorldClock {
    /// Create a new world clock
    ///
    /// # Arguments
    /// * `initial_time` - Optional starting time. If None, uses current wall time.
    pub fn new(initial_time: Option<Timestamp>) -> Arc<Self> {
        let start_real = Utc::now();
        let start_sim = initial_time.unwrap_or(start_real);

        Arc::new(Self {
            start_real,
            start_sim,
            scale: RwLock::new(TimeScale::Normal),
            current_time: RwLock::new(start_sim),
        })
    }

    /// Set the time scale
    pub async fn set_time_scale(&self, scale: TimeScale) {
        // Update current time before changing scale to preserve continuity
        let _ = self.update_time().await;
        let mut scale_guard = self.scale.write().await;
        *scale_guard = scale;
    }

    /// Get the current time scale
    pub async fn time_scale(&self) -> TimeScale {
        *self.scale.read().await
    }

    /// Update and return the current simulation time based on the time scale
    async fn update_time(&self) -> Timestamp {
        let scale = *self.scale.read().await;
        let real_now = Utc::now();
        let real_elapsed = real_now - self.start_real;

        let new_time = match scale {
            TimeScale::Normal => self.start_sim + real_elapsed,
            TimeScale::Fast(multiplier) => {
                let sim_elapsed = real_elapsed * multiplier as i32;
                self.start_sim + sim_elapsed
            }
            TimeScale::Slow(divisor) => {
                if divisor == 0 {
                    self.start_sim
                } else {
                    let sim_elapsed = real_elapsed / divisor as i32;
                    self.start_sim + sim_elapsed
                }
            }
            TimeScale::Fixed => {
                // In fixed mode, return cached time without updating
                return *self.current_time.read().await;
            }
        };

        // Update cached time
        let mut current = self.current_time.write().await;
        *current = new_time;
        new_time
    }

    /// Advance the simulated time by a specified duration
    ///
    /// This is primarily useful in Fixed mode for deterministic testing.
    /// In other modes, it adjusts the base time.
    pub async fn advance(&self, duration: Duration) {
        let mut current = self.current_time.write().await;
        *current += duration;
    }

    /// Explicitly set the simulation time
    ///
    /// Warning: This can cause time discontinuities. Use with caution.
    pub async fn set_time(&self, time: Timestamp) {
        let mut current = self.current_time.write().await;
        *current = time;
    }

    /// Get current time (async version with internal update)
    pub async fn now_async(&self) -> Timestamp {
        self.update_time().await
    }
}

impl Clock for WorldClock {
    fn now(&self) -> Timestamp {
        // For sync interface, we need to use blocking
        // In practice, prefer now_async() when in async context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.update_time())
        })
    }

    fn name(&self) -> &str {
        "WorldClock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_world_clock_creation() {
        let clock = WorldClock::new(None);
        let time1 = clock.now_async().await;
        let time2 = clock.now_async().await;
        assert!(time2 >= time1);
    }

    #[tokio::test]
    async fn test_fixed_mode() {
        let clock = WorldClock::new(None);
        clock.set_time_scale(TimeScale::Fixed).await;

        let time1 = clock.now_async().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        let time2 = clock.now_async().await;

        // In fixed mode, time should not advance automatically
        assert_eq!(time1, time2);

        // Advance manually
        clock.advance(Duration::seconds(5)).await;
        let time3 = clock.now_async().await;
        assert_eq!(time3 - time1, Duration::seconds(5));
    }
}
