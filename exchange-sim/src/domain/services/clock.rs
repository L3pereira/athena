use crate::domain::value_objects::Timestamp;
use chrono::Duration;
use tokio::sync::broadcast;

/// Time scale for simulation control
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TimeScale {
    #[default]
    RealTime,
    /// Multiplier for fast-forward (e.g., 100.0 = 100x speed)
    Fast(f64),
    /// Multiplier for slow-motion (e.g., 0.1 = 10% speed)
    Slow(f64),
    /// Time only advances via explicit advance() calls
    Fixed,
}

/// Time update notification sent to subscribers
#[derive(Debug, Clone, Copy)]
pub struct TimeUpdate {
    pub timestamp: Timestamp,
    pub sequence: u64,
}

/// NTP sync event - simulates sudden clock corrections
#[derive(Debug, Clone, Copy)]
pub struct NtpSyncEvent {
    /// Time before sync
    pub before: Timestamp,
    /// Time after sync (jumped to)
    pub after: Timestamp,
    /// Amount of correction applied
    pub correction: Duration,
    /// Sequence number
    pub sequence: u64,
}

// ============================================================================
// TRAITS
// ============================================================================

/// Basic clock trait - provides current time
///
/// This is the fundamental trait for any time source in the system.
/// Implementations can be real clocks, simulation clocks, or drifting clocks.
pub trait Clock: Send + Sync {
    /// Get current time from this clock's perspective
    fn now(&self) -> Timestamp;

    /// Get current time as milliseconds since Unix epoch
    fn now_millis(&self) -> i64 {
        self.now().timestamp_millis()
    }

    /// Get current time as microseconds since Unix epoch
    fn now_micros(&self) -> i64 {
        self.now().timestamp_micros()
    }

    /// Get current time as nanoseconds since Unix epoch
    fn now_nanos(&self) -> i64 {
        self.now().timestamp_nanos_opt().unwrap_or(0)
    }
}

/// A clock that can broadcast time updates to subscribers
pub trait ClockSource: Clock {
    /// Subscribe to time update notifications
    fn subscribe(&self) -> broadcast::Receiver<TimeUpdate>;

    /// Get monotonic sequence number for ordering events
    fn sequence(&self) -> u64;
}

/// A clock that can be controlled (for simulation)
pub trait ControllableClock: Clock {
    /// Advance time by a duration
    fn advance(&self, duration: Duration);

    /// Set time to a specific value
    fn set_time(&self, time: Timestamp);

    /// Set time scale (speed of time passage)
    fn set_time_scale(&self, scale: TimeScale);

    /// Get current time scale
    fn time_scale(&self) -> TimeScale;
}

// ============================================================================
// EXTERNAL CLOCK ADAPTER
// ============================================================================

/// Adapter to wrap an external clock (e.g., from athena-clock)
///
/// Use this when you want to inject a clock from the shared kernel:
/// ```ignore
/// use athena_clock::WorldClock;
/// let world_clock = WorldClock::new();
/// let exchange_clock = ExternalClockAdapter::new(world_clock);
/// ```
pub struct ExternalClockAdapter<T> {
    inner: T,
}

impl<T> ExternalClockAdapter<T> {
    pub fn new(clock: T) -> Self {
        ExternalClockAdapter { inner: clock }
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }
}
