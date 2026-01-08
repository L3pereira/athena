//! Quoting Model Protocol
//!
//! Core trait for market making quote generation (SOLID: OCP)

use crate::domain::{Inventory, Quote};
use chrono::Duration;
use trading_core::Price;

/// Quoting model interface (Open for extension, closed for modification)
///
/// Implementations compute optimal two-sided quotes.
/// All implementations must be thread-safe (Send + Sync).
pub trait QuotingModel: Send + Sync {
    /// Compute optimal quotes given current state
    ///
    /// # Arguments
    /// * `mid_price` - Current mid price
    /// * `inventory` - Current inventory position
    /// * `volatility` - Current volatility estimate (annualized)
    /// * `time_remaining` - Time remaining in trading session
    ///
    /// # Returns
    /// Two-sided quote (bid_price, ask_price, bid_size, ask_size)
    fn compute_quotes(
        &self,
        mid_price: Price,
        inventory: &Inventory,
        volatility: f64,
        time_remaining: Duration,
    ) -> Quote;

    /// Get the model name for logging/debugging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::Quantity;

    struct TestQuoter;

    impl QuotingModel for TestQuoter {
        fn compute_quotes(
            &self,
            mid_price: Price,
            _inventory: &Inventory,
            _volatility: f64,
            _time_remaining: Duration,
        ) -> Quote {
            Quote::symmetric(
                mid_price,
                Price::from_raw(5_000_000),
                Quantity::from_int(10),
            )
        }

        fn name(&self) -> &str {
            "test"
        }
    }

    #[test]
    fn test_trait_object() {
        let quoter: Box<dyn QuotingModel> = Box::new(TestQuoter);
        let quote = quoter.compute_quotes(
            Price::from_int(100),
            &Inventory::default(),
            0.2,
            Duration::hours(1),
        );

        assert!(!quote.is_crossed());
        assert!((quote.mid_price().raw() - 100_00000000).abs() < 1000);
    }
}
