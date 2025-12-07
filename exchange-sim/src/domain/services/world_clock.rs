use super::clock::{Clock, ClockSource, ControllableClock, NtpSyncEvent, TimeScale, TimeUpdate};
use crate::domain::value_objects::Timestamp;
use chrono::{Duration, Utc};
use parking_lot::RwLock;
use rand::Rng;
use rand_distr::{Distribution, Normal};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

// ============================================================================
// WORLD CLOCK - Source of Truth
// ============================================================================

/// World Clock - acts as NTP source for the entire simulation
///
/// This is the single source of truth for time. All other clocks
/// (ExchangeClock, AgentClock) derive their time from this clock.
///
/// ```text
/// ┌─────────────────────────────────────────┐
/// │            WorldClock (NTP)             │
/// │  - Broadcasts time updates              │
/// │  - Controls simulation speed            │
/// └─────────────────────────────────────────┘
///           │ broadcast
///     ┌─────┴─────┬─────────────┐
///     ▼           ▼             ▼
/// ExchangeClock ExchangeClock AgentTimeView
/// (drift: 5ppm) (drift: -3ppm) (latency+jitter)
/// ```
pub struct WorldClock {
    inner: Arc<WorldClockInner>,
}

struct WorldClockInner {
    /// Wall clock reference point (when simulation started)
    wall_clock_start: RwLock<std::time::Instant>,
    /// Simulated time reference point
    sim_time_start: RwLock<Timestamp>,
    /// Current simulated time (for Fixed mode)
    current_time: RwLock<Timestamp>,
    /// Time scale
    scale: RwLock<TimeScale>,
    /// Monotonic sequence number for ordering
    sequence: AtomicU64,
    /// Broadcast channel for time updates
    time_tx: broadcast::Sender<TimeUpdate>,
}

impl WorldClock {
    /// Create a new world clock starting at current wall time
    pub fn new() -> Self {
        Self::at(Utc::now())
    }

    /// Create a world clock starting at a specific time
    pub fn at(start_time: Timestamp) -> Self {
        let (time_tx, _) = broadcast::channel(1024);

        Self {
            inner: Arc::new(WorldClockInner {
                wall_clock_start: RwLock::new(std::time::Instant::now()),
                sim_time_start: RwLock::new(start_time),
                current_time: RwLock::new(start_time),
                scale: RwLock::new(TimeScale::RealTime),
                sequence: AtomicU64::new(0),
                time_tx,
            }),
        }
    }

    /// Create a world clock in fixed mode (for deterministic testing)
    pub fn fixed() -> Self {
        let clock = Self::new();
        clock.set_time_scale(TimeScale::Fixed);
        clock
    }

    /// Create a fast clock for simulation
    pub fn fast(multiplier: f64) -> Self {
        let clock = Self::new();
        clock.set_time_scale(TimeScale::Fast(multiplier));
        clock
    }

    fn broadcast_update(&self, timestamp: Timestamp) {
        let seq = self.inner.sequence.fetch_add(1, Ordering::SeqCst);
        let update = TimeUpdate {
            timestamp,
            sequence: seq,
        };
        let _ = self.inner.time_tx.send(update);
    }
}

impl Default for WorldClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for WorldClock {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Clock for WorldClock {
    fn now(&self) -> Timestamp {
        let scale = *self.inner.scale.read();

        match scale {
            TimeScale::Fixed => *self.inner.current_time.read(),
            TimeScale::RealTime => {
                let elapsed = self.inner.wall_clock_start.read().elapsed();
                let elapsed_chrono = Duration::from_std(elapsed).unwrap_or(Duration::zero());
                *self.inner.sim_time_start.read() + elapsed_chrono
            }
            TimeScale::Fast(mult) => {
                let elapsed = self.inner.wall_clock_start.read().elapsed();
                let scaled_nanos = (elapsed.as_nanos() as f64 * mult) as i64;
                let scaled = Duration::nanoseconds(scaled_nanos);
                *self.inner.sim_time_start.read() + scaled
            }
            TimeScale::Slow(mult) => {
                let elapsed = self.inner.wall_clock_start.read().elapsed();
                let scaled_nanos = (elapsed.as_nanos() as f64 * mult) as i64;
                let scaled = Duration::nanoseconds(scaled_nanos);
                *self.inner.sim_time_start.read() + scaled
            }
        }
    }
}

impl ClockSource for WorldClock {
    fn subscribe(&self) -> broadcast::Receiver<TimeUpdate> {
        self.inner.time_tx.subscribe()
    }

    fn sequence(&self) -> u64 {
        self.inner.sequence.load(Ordering::SeqCst)
    }
}

impl ControllableClock for WorldClock {
    fn advance(&self, duration: Duration) {
        let current = self.now();
        let new_time = current + duration;

        let mut sim_start = self.inner.sim_time_start.write();
        let mut current_time = self.inner.current_time.write();
        let mut wall_start = self.inner.wall_clock_start.write();

        *sim_start = new_time;
        *current_time = new_time;
        *wall_start = std::time::Instant::now();

        drop(sim_start);
        drop(current_time);
        drop(wall_start);

        self.broadcast_update(new_time);
    }

    fn set_time(&self, time: Timestamp) {
        let mut sim_start = self.inner.sim_time_start.write();
        let mut current_time = self.inner.current_time.write();
        let mut wall_start = self.inner.wall_clock_start.write();

        *sim_start = time;
        *current_time = time;
        *wall_start = std::time::Instant::now();

        drop(sim_start);
        drop(current_time);
        drop(wall_start);

        self.broadcast_update(time);
    }

    fn set_time_scale(&self, scale: TimeScale) {
        let current = self.now();

        let mut sim_start = self.inner.sim_time_start.write();
        let mut current_time = self.inner.current_time.write();
        let mut wall_start = self.inner.wall_clock_start.write();
        let mut scale_guard = self.inner.scale.write();

        *sim_start = current;
        *current_time = current;
        *wall_start = std::time::Instant::now();
        *scale_guard = scale;

        drop(sim_start);
        drop(current_time);
        drop(wall_start);
        drop(scale_guard);

        self.broadcast_update(current);
    }

    fn time_scale(&self) -> TimeScale {
        *self.inner.scale.read()
    }
}

// ============================================================================
// DRIFTING CLOCK - Accumulated drift over time
// ============================================================================

/// A clock that drifts from its source at a specified rate
///
/// Drift rate is specified in PPM (parts per million):
/// - 1 ppm = 1 microsecond drift per second
/// - 10 ppm = 10 microseconds drift per second = 864ms/day
/// - Typical crystal oscillators: 10-100 ppm
/// - Good TCXO: 1-5 ppm
/// - Atomic clocks: < 0.001 ppm
pub struct DriftingClock<S: ClockSource> {
    source: S,
    inner: Arc<DriftingClockInner>,
}

struct DriftingClockInner {
    /// Initial offset from source (simulates initial sync error)
    initial_offset: RwLock<Duration>,
    /// Drift rate in PPM (positive = running fast, negative = running slow)
    drift_rate_ppm: RwLock<f64>,
    /// Time of last NTP sync (drift accumulates from this point)
    last_sync: RwLock<Timestamp>,
    /// Accumulated drift at last sync (reset to 0 on sync)
    accumulated_at_sync: RwLock<Duration>,
    /// Name/identifier
    name: String,
}

impl<S: ClockSource> DriftingClock<S> {
    /// Create a new drifting clock
    pub fn new(
        source: S,
        initial_offset: Duration,
        drift_rate_ppm: f64,
        name: impl Into<String>,
    ) -> Self {
        let now = source.now();
        Self {
            source,
            inner: Arc::new(DriftingClockInner {
                initial_offset: RwLock::new(initial_offset),
                drift_rate_ppm: RwLock::new(drift_rate_ppm),
                last_sync: RwLock::new(now),
                accumulated_at_sync: RwLock::new(Duration::zero()),
                name: name.into(),
            }),
        }
    }

    /// Create with zero drift (perfectly synced)
    pub fn synced(source: S, name: impl Into<String>) -> Self {
        Self::new(source, Duration::zero(), 0.0, name)
    }

    /// Create with typical crystal oscillator drift (~20 ppm)
    pub fn typical_crystal(source: S, name: impl Into<String>) -> Self {
        let mut rng = rand::thread_rng();
        let ppm = rng.gen_range(-20.0..20.0);
        let offset_ms = rng.gen_range(-5.0..5.0);
        Self::new(source, Duration::milliseconds(offset_ms as i64), ppm, name)
    }

    pub fn source(&self) -> &S {
        &self.source
    }

    pub fn drift_rate_ppm(&self) -> f64 {
        *self.inner.drift_rate_ppm.read()
    }

    pub fn initial_offset(&self) -> Duration {
        *self.inner.initial_offset.read()
    }

    /// Calculate accumulated drift since last sync
    pub fn accumulated_drift(&self) -> Duration {
        let source_now = self.source.now();
        let last_sync = *self.inner.last_sync.read();
        let ppm = *self.inner.drift_rate_ppm.read();
        let base_drift = *self.inner.accumulated_at_sync.read();

        let since_sync = source_now - last_sync;
        let since_sync_micros = since_sync.num_microseconds().unwrap_or(0) as f64;
        let new_drift_micros = (since_sync_micros / 1_000_000.0) * ppm;

        base_drift + Duration::microseconds(new_drift_micros as i64)
    }

    pub fn total_offset(&self) -> Duration {
        self.initial_offset() + self.accumulated_drift()
    }

    /// Simulate NTP sync - corrects accumulated drift
    pub fn ntp_sync(&self) -> NtpSyncEvent {
        let before = self.now();
        let source_now = self.source.now();

        {
            let mut last_sync = self.inner.last_sync.write();
            let mut accumulated = self.inner.accumulated_at_sync.write();
            *last_sync = source_now;
            *accumulated = Duration::zero();
        }

        let after = self.now();
        NtpSyncEvent {
            before,
            after,
            correction: after - before,
            sequence: self.source.sequence(),
        }
    }

    /// Simulate NTP sync with residual error
    pub fn ntp_sync_with_error(&self, max_error_micros: i64) -> NtpSyncEvent {
        let before = self.now();
        let source_now = self.source.now();
        let mut rng = rand::thread_rng();
        let residual = Duration::microseconds(rng.gen_range(-max_error_micros..max_error_micros));

        {
            let mut last_sync = self.inner.last_sync.write();
            let mut accumulated = self.inner.accumulated_at_sync.write();
            *last_sync = source_now;
            *accumulated = residual;
        }

        let after = self.now();
        NtpSyncEvent {
            before,
            after,
            correction: after - before,
            sequence: self.source.sequence(),
        }
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }
}

impl<S: ClockSource + Clone> Clone for DriftingClock<S> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<S: ClockSource> Clock for DriftingClock<S> {
    fn now(&self) -> Timestamp {
        self.source.now() + self.initial_offset() + self.accumulated_drift()
    }
}

/// DriftingClock implements ClockSource with intentional behavior:
/// - `now()` returns the locally-perceived time (with drift applied)
/// - `subscribe()` returns the source's time updates (for global synchronization)
/// - `sequence()` returns the source's sequence (global ordering)
///
/// This is intentional: DriftingClock represents local clock drift, but coordinates
/// via the global time source. Subscribers receive "true" time updates and must
/// apply their own drift if needed.
impl<S: ClockSource> ClockSource for DriftingClock<S> {
    /// Returns source time updates (not drifted) for global synchronization
    fn subscribe(&self) -> broadcast::Receiver<TimeUpdate> {
        self.source.subscribe()
    }

    /// Returns source sequence (global ordering, independent of local drift)
    fn sequence(&self) -> u64 {
        self.source.sequence()
    }
}

// ============================================================================
// EXCHANGE CLOCK
// ============================================================================

/// Exchange clock - a drifting clock that represents an exchange's time
pub type ExchangeClock = DriftingClock<WorldClock>;

impl ExchangeClock {
    pub fn with_offset(world: WorldClock, offset: Duration, name: impl Into<String>) -> Self {
        Self::new(world, offset, 0.0, name)
    }

    pub fn world(&self) -> &WorldClock {
        self.source()
    }
}

// ============================================================================
// NETWORK SIMULATION
// ============================================================================

/// Network latency simulator with jitter
#[derive(Debug, Clone)]
pub struct NetworkSim {
    base_latency: Duration,
    jitter_std_dev: Duration,
    packet_loss_rate: f64,
    asymmetry: f64,
}

impl NetworkSim {
    pub fn new(base_latency: Duration, jitter_std_dev: Duration) -> Self {
        Self {
            base_latency,
            jitter_std_dev,
            packet_loss_rate: 0.0,
            asymmetry: 1.0,
        }
    }

    pub fn fixed(latency: Duration) -> Self {
        Self::new(latency, Duration::zero())
    }

    pub fn colocated() -> Self {
        Self::new(Duration::microseconds(50), Duration::microseconds(10))
    }

    pub fn regional() -> Self {
        Self::new(Duration::milliseconds(2), Duration::microseconds(500))
    }

    pub fn intercontinental() -> Self {
        Self::new(Duration::milliseconds(80), Duration::milliseconds(10))
    }

    pub fn retail() -> Self {
        Self::new(Duration::milliseconds(50), Duration::milliseconds(15))
    }

    pub fn with_packet_loss(mut self, rate: f64) -> Self {
        self.packet_loss_rate = rate.clamp(0.0, 1.0);
        self
    }

    pub fn with_asymmetry(mut self, factor: f64) -> Self {
        self.asymmetry = factor;
        self
    }

    pub fn delay(&self) -> Duration {
        if self.jitter_std_dev.is_zero() {
            return self.base_latency;
        }

        let jitter_micros = self.jitter_std_dev.num_microseconds().unwrap_or(0) as f64;
        if jitter_micros > 0.0 {
            let normal = Normal::new(0.0, jitter_micros).unwrap();
            let jitter = normal.sample(&mut rand::thread_rng());
            let total_micros =
                self.base_latency.num_microseconds().unwrap_or(0) + jitter.abs() as i64;
            Duration::microseconds(total_micros)
        } else {
            self.base_latency
        }
    }

    pub fn upload_delay(&self) -> Duration {
        let base = self.delay();
        let micros = (base.num_microseconds().unwrap_or(0) as f64 * self.asymmetry) as i64;
        Duration::microseconds(micros)
    }

    pub fn download_delay(&self) -> Duration {
        self.delay()
    }

    pub fn rtt(&self) -> Duration {
        self.upload_delay() + self.download_delay()
    }

    pub fn is_packet_lost(&self) -> bool {
        self.packet_loss_rate > 0.0 && rand::thread_rng().r#gen::<f64>() < self.packet_loss_rate
    }

    pub fn base_latency(&self) -> Duration {
        self.base_latency
    }

    pub fn jitter_std_dev(&self) -> Duration {
        self.jitter_std_dev
    }
}

impl Default for NetworkSim {
    fn default() -> Self {
        Self::retail()
    }
}

// ============================================================================
// AGENT TIME VIEW
// ============================================================================

/// Agent's view of time - includes network latency and jitter
pub struct AgentTimeView<C: Clock> {
    exchange: C,
    network: NetworkSim,
    name: String,
}

impl<C: Clock> AgentTimeView<C> {
    pub fn new(exchange: C, network: NetworkSim, name: impl Into<String>) -> Self {
        Self {
            exchange,
            network,
            name: name.into(),
        }
    }

    pub fn with_latency(exchange: C, latency: Duration, name: impl Into<String>) -> Self {
        Self::new(exchange, NetworkSim::fixed(latency), name)
    }

    pub fn colocated(exchange: C, name: impl Into<String>) -> Self {
        Self::new(exchange, NetworkSim::colocated(), name)
    }

    pub fn retail(exchange: C, name: impl Into<String>) -> Self {
        Self::new(exchange, NetworkSim::retail(), name)
    }

    /// What time the agent sees (with jitter sampling)
    pub fn now_sampled(&self) -> Timestamp {
        self.exchange.now() - self.network.delay()
    }

    pub fn now_with_delay(&self, delay: Duration) -> Timestamp {
        self.exchange.now() - delay
    }

    pub fn arrival_time(&self) -> Timestamp {
        self.exchange.now() + self.network.upload_delay()
    }

    pub fn response_time(&self, exchange_send_time: Timestamp) -> Timestamp {
        exchange_send_time + self.network.download_delay()
    }

    pub fn rtt(&self) -> Duration {
        self.network.rtt()
    }

    pub fn is_packet_lost(&self) -> bool {
        self.network.is_packet_lost()
    }

    pub fn exchange(&self) -> &C {
        &self.exchange
    }

    pub fn network(&self) -> &NetworkSim {
        &self.network
    }

    pub fn network_mut(&mut self) -> &mut NetworkSim {
        &mut self.network
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl<C: Clock + Clone> Clone for AgentTimeView<C> {
    fn clone(&self) -> Self {
        Self {
            exchange: self.exchange.clone(),
            network: self.network.clone(),
            name: self.name.clone(),
        }
    }
}

impl<C: Clock> Clock for AgentTimeView<C> {
    fn now(&self) -> Timestamp {
        self.exchange.now() - self.network.base_latency()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_clock_fixed_mode() {
        let clock = WorldClock::fixed();
        let t1 = clock.now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let t2 = clock.now();
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_world_clock_advance() {
        let clock = WorldClock::fixed();
        let t1 = clock.now();
        clock.advance(Duration::seconds(60));
        let t2 = clock.now();
        assert_eq!((t2 - t1).num_seconds(), 60);
    }

    #[test]
    fn test_drifting_clock_initial_offset() {
        let world = WorldClock::fixed();
        let exchange = DriftingClock::new(world.clone(), Duration::milliseconds(5), 0.0, "NYSE");
        assert_eq!((exchange.now() - world.now()).num_milliseconds(), 5);
    }

    #[test]
    fn test_drifting_clock_accumulated_drift() {
        let world = WorldClock::fixed();
        let exchange = DriftingClock::new(world.clone(), Duration::zero(), 10.0, "EXCHANGE");
        world.advance(Duration::seconds(1));
        let drift = exchange.accumulated_drift();
        assert!(drift.num_microseconds().unwrap() >= 9);
        assert!(drift.num_microseconds().unwrap() <= 11);
    }

    #[test]
    fn test_drifting_clock_ntp_sync() {
        let world = WorldClock::fixed();
        let exchange =
            DriftingClock::new(world.clone(), Duration::milliseconds(100), 50.0, "DRIFTING");
        world.advance(Duration::seconds(10));

        let before_sync = exchange.total_offset();
        assert!(before_sync > Duration::milliseconds(100));

        let event = exchange.ntp_sync();
        assert_eq!(exchange.accumulated_drift().num_microseconds().unwrap(), 0);
        assert!(event.correction.num_microseconds().unwrap().abs() > 0);
    }

    #[test]
    fn test_network_sim_jitter() {
        let net = NetworkSim::new(Duration::milliseconds(50), Duration::milliseconds(10));
        let mut delays: Vec<i64> = (0..100)
            .map(|_| net.delay().num_microseconds().unwrap())
            .collect();
        delays.sort();
        assert!(delays[99] > delays[0], "Expected jitter variation");
        assert!(delays[0] >= 50000, "Jitter should not reduce below base");
    }

    #[test]
    fn test_network_sim_fixed() {
        let net = NetworkSim::fixed(Duration::milliseconds(10));
        assert_eq!(net.delay(), net.delay());
        assert_eq!(net.delay().num_milliseconds(), 10);
    }

    #[test]
    fn test_agent_time_view() {
        let world = WorldClock::fixed();
        let exchange = ExchangeClock::synced(world.clone(), "BINANCE");
        let agent =
            AgentTimeView::with_latency(exchange.clone(), Duration::milliseconds(50), "Trader");
        assert_eq!((exchange.now() - Clock::now(&agent)).num_milliseconds(), 50);
    }

    #[test]
    fn test_full_latency_chain() {
        let world = WorldClock::fixed();
        let exchange = ExchangeClock::new(world.clone(), Duration::milliseconds(3), 5.0, "NYSE");
        let agent =
            AgentTimeView::with_latency(exchange.clone(), Duration::milliseconds(20), "HFT");

        let world_time = world.now();
        assert_eq!((exchange.now() - world_time).num_milliseconds(), 3);
        assert_eq!((Clock::now(&agent) - world_time).num_milliseconds(), -17);
    }
}
