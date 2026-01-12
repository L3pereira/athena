//! Profit and Loss Tracking
//!
//! For tracking realized and unrealized P&L of trading positions.

use serde::{Deserialize, Serialize};
use trading_core::Price;

/// P&L tracking for a trading position
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PnL {
    /// Realized P&L from closed trades (in quote currency raw units)
    realized: i64,
    /// Average entry price for current position (raw units)
    avg_entry_price: i64,
    /// Current position size (positive = long, negative = short)
    position: i64,
    /// Total fees paid (raw units)
    fees_paid: i64,
}

impl PnL {
    /// Create new P&L tracker
    pub fn new() -> Self {
        Self {
            realized: 0,
            avg_entry_price: 0,
            position: 0,
            fees_paid: 0,
        }
    }

    /// Record a trade
    ///
    /// - `signed_qty`: positive for buy, negative for sell
    /// - `price`: execution price
    /// - `fee`: fee paid (always positive)
    pub fn record_trade(&mut self, signed_qty: i64, price: Price, fee: i64) {
        let price_raw = price.raw();
        self.fees_paid += fee;

        if signed_qty == 0 {
            return;
        }

        let old_position = self.position;
        let new_position = old_position + signed_qty;

        // Determine if this trade is opening, closing, or both
        if old_position == 0 {
            // Opening new position
            self.avg_entry_price = price_raw;
            self.position = new_position;
        } else if (old_position > 0 && signed_qty > 0) || (old_position < 0 && signed_qty < 0) {
            // Adding to existing position - update average entry
            // Use i128 to avoid overflow with large price * quantity
            let old_cost = (self.avg_entry_price as i128) * (old_position.abs() as i128);
            let new_cost = (price_raw as i128) * (signed_qty.abs() as i128);
            let total_position = old_position.abs() + signed_qty.abs();
            self.avg_entry_price = ((old_cost + new_cost) / (total_position as i128)) as i64;
            self.position = new_position;
        } else {
            // Reducing or reversing position
            let closing_qty = signed_qty.abs().min(old_position.abs());

            // Calculate realized P&L for closed portion
            // Use i128 to avoid overflow
            let pnl_per_unit = if old_position > 0 {
                // Was long, selling to close
                price_raw - self.avg_entry_price
            } else {
                // Was short, buying to close
                self.avg_entry_price - price_raw
            };
            self.realized += ((pnl_per_unit as i128) * (closing_qty as i128)) as i64;

            // Handle any reversal
            let reversal_qty = signed_qty.abs() - closing_qty;
            if reversal_qty > 0 {
                // Position reversed - new entry at current price
                self.avg_entry_price = price_raw;
            }

            self.position = new_position;

            // Reset entry price if flat
            if self.position == 0 {
                self.avg_entry_price = 0;
            }
        }
    }

    /// Calculate unrealized P&L at current mark price
    pub fn unrealized(&self, mark_price: Price) -> i64 {
        if self.position == 0 {
            return 0;
        }

        let mark_raw = mark_price.raw();
        let pnl_per_unit = if self.position > 0 {
            mark_raw - self.avg_entry_price
        } else {
            self.avg_entry_price - mark_raw
        };

        // Use i128 to avoid overflow
        ((pnl_per_unit as i128) * (self.position.abs() as i128)) as i64
    }

    /// Get realized P&L
    pub fn realized(&self) -> i64 {
        self.realized
    }

    /// Get total P&L (realized + unrealized) at mark price
    pub fn total(&self, mark_price: Price) -> i64 {
        self.realized + self.unrealized(mark_price)
    }

    /// Get total P&L minus fees
    pub fn net_total(&self, mark_price: Price) -> i64 {
        self.total(mark_price) - self.fees_paid
    }

    /// Get current position
    pub fn position(&self) -> i64 {
        self.position
    }

    /// Get average entry price
    pub fn avg_entry_price(&self) -> Price {
        Price::from_raw(self.avg_entry_price)
    }

    /// Get total fees paid
    pub fn fees_paid(&self) -> i64 {
        self.fees_paid
    }

    /// Reset P&L tracking (keeps position info)
    pub fn reset_realized(&mut self) {
        self.realized = 0;
        self.fees_paid = 0;
    }
}

impl Default for PnL {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_long_position() {
        let mut pnl = PnL::new();
        pnl.record_trade(100, Price::from_int(50000), 10);

        assert_eq!(pnl.position(), 100);
        assert_eq!(pnl.avg_entry_price(), Price::from_int(50000));
        assert_eq!(pnl.realized(), 0);
        assert_eq!(pnl.fees_paid(), 10);
    }

    #[test]
    fn test_close_long_with_profit() {
        let mut pnl = PnL::new();
        // Buy 100 @ 50000
        pnl.record_trade(100, Price::from_int(50000), 10);
        // Sell 100 @ 51000 (profit of 1000 per unit)
        pnl.record_trade(-100, Price::from_int(51000), 10);

        assert_eq!(pnl.position(), 0);
        // Profit = (51000 - 50000) * 100 = 100_000 in raw units
        // Price::from_int(1000) = 1000_00000000 raw
        assert_eq!(pnl.realized(), 1000_00000000 * 100);
    }

    #[test]
    fn test_close_long_with_loss() {
        let mut pnl = PnL::new();
        // Buy 100 @ 50000
        pnl.record_trade(100, Price::from_int(50000), 0);
        // Sell 100 @ 49000 (loss of 1000 per unit)
        pnl.record_trade(-100, Price::from_int(49000), 0);

        assert_eq!(pnl.position(), 0);
        assert_eq!(pnl.realized(), -1000_00000000 * 100);
    }

    #[test]
    fn test_short_position() {
        let mut pnl = PnL::new();
        // Sell short 100 @ 50000
        pnl.record_trade(-100, Price::from_int(50000), 0);
        assert_eq!(pnl.position(), -100);

        // Buy to cover @ 49000 (profit of 1000 per unit)
        pnl.record_trade(100, Price::from_int(49000), 0);
        assert_eq!(pnl.position(), 0);
        assert_eq!(pnl.realized(), 1000_00000000 * 100);
    }

    #[test]
    fn test_add_to_position() {
        let mut pnl = PnL::new();
        // Buy 100 @ 50000
        pnl.record_trade(100, Price::from_int(50000), 0);
        // Buy another 100 @ 52000
        pnl.record_trade(100, Price::from_int(52000), 0);

        assert_eq!(pnl.position(), 200);
        // Average = (50000 * 100 + 52000 * 100) / 200 = 51000
        assert_eq!(pnl.avg_entry_price(), Price::from_int(51000));
    }

    #[test]
    fn test_partial_close() {
        let mut pnl = PnL::new();
        // Buy 100 @ 50000
        pnl.record_trade(100, Price::from_int(50000), 0);
        // Sell 50 @ 52000
        pnl.record_trade(-50, Price::from_int(52000), 0);

        assert_eq!(pnl.position(), 50);
        // Realized = (52000 - 50000) * 50
        assert_eq!(pnl.realized(), 2000_00000000 * 50);
        // Entry price unchanged for remaining position
        assert_eq!(pnl.avg_entry_price(), Price::from_int(50000));
    }

    #[test]
    fn test_unrealized_pnl() {
        let mut pnl = PnL::new();
        pnl.record_trade(100, Price::from_int(50000), 0);

        // Mark price higher - profit
        let unrealized = pnl.unrealized(Price::from_int(51000));
        assert_eq!(unrealized, 1000_00000000 * 100);

        // Mark price lower - loss
        let unrealized = pnl.unrealized(Price::from_int(49000));
        assert_eq!(unrealized, -1000_00000000 * 100);
    }

    #[test]
    fn test_position_reversal() {
        let mut pnl = PnL::new();
        // Long 100 @ 50000
        pnl.record_trade(100, Price::from_int(50000), 0);
        // Sell 150 @ 51000 (close long + open short)
        pnl.record_trade(-150, Price::from_int(51000), 0);

        // Should be short 50
        assert_eq!(pnl.position(), -50);
        // Realized from closing long = (51000 - 50000) * 100
        assert_eq!(pnl.realized(), 1000_00000000 * 100);
        // New entry for short position
        assert_eq!(pnl.avg_entry_price(), Price::from_int(51000));
    }

    #[test]
    fn test_net_total() {
        let mut pnl = PnL::new();
        pnl.record_trade(100, Price::from_int(50000), 100);
        pnl.record_trade(-100, Price::from_int(51000), 100);

        let total = pnl.total(Price::from_int(51000));
        let net = pnl.net_total(Price::from_int(51000));

        assert_eq!(net, total - 200);
    }
}
