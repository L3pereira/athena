// Re-export from athena-clock crate for new code
pub use athena_clock::{
    AgentTimeView, Clock, ExchangeClock as SimExchangeClock, SystemClock, TimeScale, WorldClock,
};

use crate::error::{ExchangeError, Result};
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::time::sleep;

/// Legacy exchange clock for backwards compatibility
///
/// This wraps the new WorldClock to maintain the existing API.
/// New code should use WorldClock directly for more features.
pub struct ExchangeClock {
    world: Arc<WorldClock>,
}

impl ExchangeClock {
    /// Create a new exchange clock
    pub fn new(initial_time: Option<DateTime<Utc>>) -> Self {
        Self {
            world: WorldClock::new(initial_time),
        }
    }

    /// Set the time scale
    pub async fn set_time_scale(&self, scale: TimeScale) -> Result<()> {
        self.world.set_time_scale(scale).await;
        Ok(())
    }

    /// Get the current simulation time
    pub async fn now(&self) -> Result<DateTime<Utc>> {
        Ok(self.world.now_async().await)
    }

    /// Advance the simulated time by a specified duration
    pub async fn advance_time(&self, duration: Duration) -> Result<()> {
        self.world.advance(duration).await;
        Ok(())
    }

    /// Explicitly set the simulation time
    pub async fn set_time(&self, time: DateTime<Utc>) -> Result<()> {
        self.world.set_time(time).await;
        Ok(())
    }

    /// Get reference to the underlying WorldClock
    pub fn world_clock(&self) -> &Arc<WorldClock> {
        &self.world
    }

    /// Get the current time scale
    pub async fn time_scale(&self) -> TimeScale {
        self.world.time_scale().await
    }
}

/// Sleep for a simulated duration based on the current time scale
pub async fn sleep_sim(clock: &Arc<ExchangeClock>, duration: Duration) -> Result<()> {
    let scale = clock.time_scale().await;

    match scale {
        TimeScale::Normal => {
            sleep(tokio::time::Duration::from_nanos(
                duration.num_nanoseconds().unwrap_or(0) as u64,
            ))
            .await;
        }
        TimeScale::Fast(multiplier) => {
            let real_duration = duration / multiplier as i32;
            sleep(tokio::time::Duration::from_nanos(
                real_duration.num_nanoseconds().unwrap_or(0) as u64,
            ))
            .await;
        }
        TimeScale::Slow(divisor) => {
            if divisor == 0 {
                return Err(ExchangeError::TimeError(
                    "Divisor cannot be zero".to_string(),
                ));
            }
            let real_duration = duration * divisor as i32;
            sleep(tokio::time::Duration::from_nanos(
                real_duration.num_nanoseconds().unwrap_or(0) as u64,
            ))
            .await;
        }
        TimeScale::Fixed => {
            // In fixed mode, we advance the time but don't actually sleep
            clock.advance_time(duration).await?;
        }
    }

    Ok(())
}
