use athena_core::Timestamp;
use athena_ports::Clock;
use chrono::Duration;
use std::sync::Arc;

use crate::WorldClock;

/// Per-exchange clock with configurable drift from the world clock
///
/// In real markets, different exchanges have slightly different times due to:
/// - Clock synchronization errors
/// - Network propagation delays
/// - Different time sources
///
/// This clock simulates that drift for realistic multi-exchange scenarios.
pub struct ExchangeClock {
    /// Reference to the universal world clock
    world: Arc<WorldClock>,
    /// Drift offset (can be positive or negative)
    drift: Duration,
    /// Name/identifier for this exchange clock
    name: String,
}

impl ExchangeClock {
    /// Create a new exchange clock with specified drift
    ///
    /// # Arguments
    /// * `world` - Reference to the world clock
    /// * `drift` - Time offset from world clock (positive = ahead, negative = behind)
    /// * `name` - Identifier for this exchange (e.g., "NYSE", "Binance")
    pub fn new(world: Arc<WorldClock>, drift: Duration, name: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            world,
            drift,
            name: name.into(),
        })
    }

    /// Create an exchange clock with zero drift
    pub fn new_synchronized(world: Arc<WorldClock>, name: impl Into<String>) -> Arc<Self> {
        Self::new(world, Duration::zero(), name)
    }

    /// Create an exchange clock with random drift within a range
    ///
    /// # Arguments
    /// * `world` - Reference to the world clock
    /// * `max_drift_ms` - Maximum drift in milliseconds (will be ± this value)
    /// * `name` - Identifier for this exchange
    pub fn new_with_random_drift(
        world: Arc<WorldClock>,
        max_drift_ms: i64,
        name: impl Into<String>,
    ) -> Arc<Self> {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Simple pseudo-random based on system time
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        let drift_ms = (seed % (2 * max_drift_ms + 1)) - max_drift_ms;
        Self::new(world, Duration::milliseconds(drift_ms), name)
    }

    /// Get the current drift offset
    pub fn drift(&self) -> Duration {
        self.drift
    }

    /// Get current time (async version)
    pub async fn now_async(&self) -> Timestamp {
        self.world.now_async().await + self.drift
    }

    /// Get reference to the underlying world clock
    pub fn world_clock(&self) -> &Arc<WorldClock> {
        &self.world
    }
}

impl Clock for ExchangeClock {
    fn now(&self) -> Timestamp {
        self.world.now() + self.drift
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_exchange_clock_drift() {
        let world = WorldClock::new(None);

        let exchange_ahead =
            ExchangeClock::new(world.clone(), Duration::milliseconds(100), "Exchange-Ahead");

        let exchange_behind = ExchangeClock::new(
            world.clone(),
            Duration::milliseconds(-50),
            "Exchange-Behind",
        );

        let world_time = world.now_async().await;
        let ahead_time = exchange_ahead.now_async().await;
        let behind_time = exchange_behind.now_async().await;

        // Verify drift relationships
        assert!(ahead_time > world_time);
        assert!(behind_time < world_time);

        let ahead_diff = ahead_time - world_time;
        let behind_diff = world_time - behind_time;

        assert!(
            ahead_diff >= Duration::milliseconds(99) && ahead_diff <= Duration::milliseconds(101)
        );
        assert!(
            behind_diff >= Duration::milliseconds(49) && behind_diff <= Duration::milliseconds(51)
        );
    }

    #[tokio::test]
    async fn test_synchronized_exchange() {
        let world = WorldClock::new(None);
        let exchange = ExchangeClock::new_synchronized(world.clone(), "Sync-Exchange");

        let world_time = world.now_async().await;
        let exchange_time = exchange.now_async().await;

        // Should be very close (within a few microseconds)
        let diff = (exchange_time - world_time)
            .num_microseconds()
            .unwrap_or(0)
            .abs();
        assert!(diff < 1000); // Less than 1ms difference
    }
}
