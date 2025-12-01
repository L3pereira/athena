use athena_core::Timestamp;
use athena_ports::Clock;
use chrono::Duration;
use std::sync::Arc;

use crate::ExchangeClock;

/// Agent's view of time with network latency simulation
///
/// In real trading, agents (bots, strategies) don't see exchange time directly.
/// There's always network latency between the agent and the exchange.
/// This affects:
/// - When the agent sees market data (delayed)
/// - When orders arrive at the exchange (delayed)
/// - Time synchronization for strategy decisions
///
/// The latency is always positive (agent sees time "behind" the exchange).
pub struct AgentTimeView {
    /// Reference to the exchange clock this agent connects to
    exchange: Arc<ExchangeClock>,
    /// Network latency (always positive - agent sees past)
    latency: Duration,
    /// Agent identifier
    name: String,
}

impl AgentTimeView {
    /// Create a new agent time view with specified latency
    ///
    /// # Arguments
    /// * `exchange` - Reference to the exchange clock
    /// * `latency` - Network latency (should be positive)
    /// * `name` - Agent identifier
    pub fn new(exchange: Arc<ExchangeClock>, latency: Duration, name: impl Into<String>) -> Self {
        // Ensure latency is non-negative
        let latency = if latency < Duration::zero() {
            Duration::zero()
        } else {
            latency
        };

        Self {
            exchange,
            latency,
            name: name.into(),
        }
    }

    /// Create with zero latency (co-located agent)
    pub fn new_colocated(exchange: Arc<ExchangeClock>, name: impl Into<String>) -> Self {
        Self::new(exchange, Duration::zero(), name)
    }

    /// Create with typical retail latency (~50-200ms)
    pub fn new_retail(exchange: Arc<ExchangeClock>, name: impl Into<String>) -> Self {
        Self::new(exchange, Duration::milliseconds(100), name)
    }

    /// Create with typical institutional latency (~1-10ms)
    pub fn new_institutional(exchange: Arc<ExchangeClock>, name: impl Into<String>) -> Self {
        Self::new(exchange, Duration::milliseconds(5), name)
    }

    /// Get the configured latency
    pub fn latency(&self) -> Duration {
        self.latency
    }

    /// Get reference to the underlying exchange clock
    pub fn exchange_clock(&self) -> &Arc<ExchangeClock> {
        &self.exchange
    }

    /// Get current time as seen by this agent (async version)
    pub async fn now_async(&self) -> Timestamp {
        self.exchange.now_async().await - self.latency
    }

    /// Calculate when a message sent now would arrive at the exchange
    /// (agent's local time + latency = exchange receive time)
    pub async fn time_at_exchange(&self) -> Timestamp {
        self.exchange.now_async().await
    }

    /// Calculate round-trip time for this agent
    pub fn round_trip_time(&self) -> Duration {
        self.latency * 2
    }
}

impl Clock for AgentTimeView {
    fn now(&self) -> Timestamp {
        self.exchange.now() - self.latency
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WorldClock;

    #[tokio::test]
    async fn test_agent_latency() {
        let world = WorldClock::new(None);
        let exchange = ExchangeClock::new_synchronized(world.clone(), "Test-Exchange");
        let agent = AgentTimeView::new(exchange.clone(), Duration::milliseconds(50), "Test-Agent");

        let exchange_time = exchange.now_async().await;
        let agent_time = agent.now_async().await;

        // Agent should see time 50ms behind exchange
        let diff = exchange_time - agent_time;
        assert!(diff >= Duration::milliseconds(49) && diff <= Duration::milliseconds(51));
    }

    #[tokio::test]
    async fn test_colocated_agent() {
        let world = WorldClock::new(None);
        let exchange = ExchangeClock::new_synchronized(world.clone(), "Test-Exchange");
        let agent = AgentTimeView::new_colocated(exchange.clone(), "Colocated-Agent");

        let exchange_time = exchange.now_async().await;
        let agent_time = agent.now_async().await;

        // Colocated agent should have minimal time difference
        let diff = (exchange_time - agent_time)
            .num_microseconds()
            .unwrap_or(0)
            .abs();
        assert!(diff < 1000); // Less than 1ms
    }

    #[tokio::test]
    async fn test_round_trip_time() {
        let world = WorldClock::new(None);
        let exchange = ExchangeClock::new_synchronized(world, "Test-Exchange");
        let agent = AgentTimeView::new(exchange, Duration::milliseconds(25), "Test-Agent");

        assert_eq!(agent.round_trip_time(), Duration::milliseconds(50));
    }
}
