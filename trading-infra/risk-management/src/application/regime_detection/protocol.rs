//! Regime Detection Protocol
//!
//! Core trait for regime detection (SOLID: OCP)

use crate::domain::{MarketRegime, OrderbookMoments, RegimeShift};

/// Regime detection interface (Open for extension, closed for modification)
///
/// Implementations detect the current market regime and identify regime shifts.
/// All implementations must be thread-safe (Send + Sync).
pub trait RegimeDetector: Send + Sync {
    /// Detect the current market regime from orderbook moments
    fn detect(&self, moments: &OrderbookMoments) -> MarketRegime;

    /// Check for a regime shift between two moment snapshots
    ///
    /// Returns Some(RegimeShift) if a shift is detected, None otherwise.
    fn detect_shift(
        &self,
        before: &OrderbookMoments,
        after: &OrderbookMoments,
    ) -> Option<RegimeShift>;

    /// Get the model name for logging/debugging
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDetector;

    impl RegimeDetector for TestDetector {
        fn detect(&self, _moments: &OrderbookMoments) -> MarketRegime {
            MarketRegime::Normal
        }

        fn detect_shift(
            &self,
            _before: &OrderbookMoments,
            _after: &OrderbookMoments,
        ) -> Option<RegimeShift> {
            None
        }

        fn name(&self) -> &str {
            "test"
        }
    }

    #[test]
    fn test_trait_object() {
        let detector: Box<dyn RegimeDetector> = Box::new(TestDetector);
        let moments = OrderbookMoments::default();
        assert_eq!(detector.detect(&moments), MarketRegime::Normal);
    }
}
