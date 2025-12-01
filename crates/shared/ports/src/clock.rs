use athena_core::Timestamp;

/// Port for time abstraction
///
/// This allows the system to use different time sources:
/// - Real system time for production
/// - Simulated time with drift for testing
/// - Fixed time for deterministic tests
pub trait Clock: Send + Sync {
    /// Get the current time according to this clock
    fn now(&self) -> Timestamp;

    /// Get the clock's name/identifier for debugging
    fn name(&self) -> &str {
        "Clock"
    }
}
