//! Inventory Skew Model
//!
//! Position-based quote adjustment for inventory management.
//!
//! From docs Section 8:
//! Long inventory  → Lower reservation → Quotes shift DOWN → Encourage selling
//! Short inventory → Higher reservation → Quotes shift UP → Encourage buying

use super::protocol::QuotingModel;
use crate::domain::{Inventory, Quote};
use chrono::Duration;
use trading_core::{Price, Quantity};

/// Inventory skew configuration
#[derive(Debug, Clone, Copy)]
pub struct SkewConfig {
    /// Base half-spread in basis points
    pub base_half_spread_bps: f64,
    /// Skew per unit of inventory ratio (bps)
    /// skew = skew_per_unit × inventory_ratio
    pub skew_per_unit: f64,
    /// Base quote size
    pub base_size: i64,
    /// Size ratio reduction when at inventory limit
    pub size_ratio_at_limit: f64,
}

impl Default for SkewConfig {
    fn default() -> Self {
        Self {
            base_half_spread_bps: 5.0,
            skew_per_unit: 10.0, // 10 bps skew at 100% inventory
            base_size: 100_00000000,
            size_ratio_at_limit: 0.2,
        }
    }
}

/// Simple inventory skew model
pub struct InventorySkew {
    config: SkewConfig,
}

impl InventorySkew {
    pub fn new(config: SkewConfig) -> Self {
        Self { config }
    }

    /// Create with custom base spread
    pub fn with_spread(base_half_spread_bps: f64) -> Self {
        Self::new(SkewConfig {
            base_half_spread_bps,
            ..Default::default()
        })
    }

    /// Calculate skew in basis points
    fn calculate_skew_bps(&self, inventory: &Inventory) -> f64 {
        // Positive inventory ratio → negative skew (shift down)
        // Negative inventory ratio → positive skew (shift up)
        -self.config.skew_per_unit * inventory.ratio()
    }

    /// Calculate size ratio based on inventory
    fn calculate_size_ratio(&self, inventory: &Inventory, is_bid: bool) -> f64 {
        let abs_ratio = inventory.abs_ratio();

        if abs_ratio < 0.5 {
            1.0 // Full size below 50% inventory
        } else if inventory.is_long() && is_bid {
            // Long and quoting bid → reduce size
            self.interpolate_size_ratio(abs_ratio)
        } else if inventory.is_short() && !is_bid {
            // Short and quoting ask → reduce size
            self.interpolate_size_ratio(abs_ratio)
        } else {
            // Encourage trades that reduce inventory
            1.0
        }
    }

    /// Interpolate size ratio from 50% to 100% inventory
    fn interpolate_size_ratio(&self, abs_ratio: f64) -> f64 {
        // Linear interpolation from 1.0 at 50% to size_ratio_at_limit at 100%
        let t = (abs_ratio - 0.5) / 0.5; // 0 at 50%, 1 at 100%
        1.0 - t * (1.0 - self.config.size_ratio_at_limit)
    }
}

impl QuotingModel for InventorySkew {
    fn compute_quotes(
        &self,
        mid_price: Price,
        inventory: &Inventory,
        _volatility: f64,
        _time_remaining: Duration,
    ) -> Quote {
        // Calculate half-spread
        let half_spread_raw =
            (mid_price.raw() as f64 * self.config.base_half_spread_bps / 10_000.0) as i64;

        // Calculate skew
        let skew_bps = self.calculate_skew_bps(inventory);
        let skew_raw = (mid_price.raw() as f64 * skew_bps / 10_000.0) as i64;

        // Apply skew to mid (skewed mid = mid + skew)
        let skewed_mid_raw = mid_price.raw() + skew_raw;

        // Calculate prices
        let bid_price = Price::from_raw(skewed_mid_raw - half_spread_raw);
        let ask_price = Price::from_raw(skewed_mid_raw + half_spread_raw);

        // Calculate sizes
        let bid_ratio = self.calculate_size_ratio(inventory, true);
        let ask_ratio = self.calculate_size_ratio(inventory, false);

        let bid_size = Quantity::from_raw((self.config.base_size as f64 * bid_ratio) as i64);
        let ask_size = Quantity::from_raw((self.config.base_size as f64 * ask_ratio) as i64);

        Quote::new(bid_price, ask_price, bid_size, ask_size)
    }

    fn name(&self) -> &str {
        "inventory_skew"
    }
}

impl Default for InventorySkew {
    fn default() -> Self {
        Self::new(SkewConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_skew_when_flat() {
        let model = InventorySkew::default();
        let inventory = Inventory::new(1000);

        let quote = model.compute_quotes(Price::from_int(100), &inventory, 0.2, Duration::hours(1));

        // Mid should be exactly at input mid
        assert_eq!(quote.mid_price().raw(), 100_00000000);
        // Sizes should be equal
        assert_eq!(quote.bid_size, quote.ask_size);
    }

    #[test]
    fn test_skew_down_when_long() {
        let model = InventorySkew::default();
        let long = Inventory::with_position(500, 1000); // 50% long

        let quote = model.compute_quotes(Price::from_int(100), &long, 0.2, Duration::hours(1));

        // Quotes should be skewed down (mid < 100)
        assert!(quote.mid_price().raw() < 100_00000000);
    }

    #[test]
    fn test_skew_up_when_short() {
        let model = InventorySkew::default();
        let short = Inventory::with_position(-500, 1000);

        let quote = model.compute_quotes(Price::from_int(100), &short, 0.2, Duration::hours(1));

        // Quotes should be skewed up (mid > 100)
        assert!(quote.mid_price().raw() > 100_00000000);
    }

    #[test]
    fn test_size_reduction_at_limit() {
        let model = InventorySkew::default();
        let at_limit = Inventory::with_position(1000, 1000); // 100% long

        let quote = model.compute_quotes(Price::from_int(100), &at_limit, 0.2, Duration::hours(1));

        // Bid size should be reduced (don't want more longs)
        assert!(quote.bid_size.raw() < quote.ask_size.raw());
    }

    #[test]
    fn test_spread_constant() {
        let model = InventorySkew::default();
        let flat = Inventory::new(1000);
        let long = Inventory::with_position(500, 1000);

        let flat_quote = model.compute_quotes(Price::from_int(100), &flat, 0.2, Duration::hours(1));

        let long_quote = model.compute_quotes(Price::from_int(100), &long, 0.2, Duration::hours(1));

        // Spread should be the same regardless of inventory
        assert_eq!(flat_quote.spread().raw(), long_quote.spread().raw());
    }
}
