use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::instruments::InstrumentId;

/// Position side - long (bought) or short (sold)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    /// Long position - bought the asset, profit when price rises
    Long,
    /// Short position - sold borrowed asset, profit when price falls
    Short,
}

impl PositionSide {
    /// Returns the opposite side
    pub fn opposite(&self) -> Self {
        match self {
            PositionSide::Long => PositionSide::Short,
            PositionSide::Short => PositionSide::Long,
        }
    }
}

/// Represents a trading position in an instrument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// Unique position identifier
    pub id: Uuid,

    /// Account that owns this position
    pub account_id: Uuid,

    /// Instrument being traded
    pub instrument_id: InstrumentId,

    /// Position side (long/short)
    pub side: PositionSide,

    /// Current position quantity (always positive)
    pub quantity: Decimal,

    /// Average entry price
    pub entry_price: Decimal,

    /// Current market price (for P&L calculation)
    pub mark_price: Decimal,

    /// Realized profit/loss (from closed portions)
    pub realized_pnl: Decimal,

    /// Initial margin used for this position
    pub initial_margin: Decimal,

    /// Maintenance margin required
    pub maintenance_margin: Decimal,

    /// Liquidation price
    pub liquidation_price: Decimal,

    /// When the position was opened
    pub opened_at: DateTime<Utc>,

    /// Last update time
    pub updated_at: DateTime<Utc>,
}

impl Position {
    /// Create a new position
    pub fn new(
        account_id: Uuid,
        instrument_id: InstrumentId,
        side: PositionSide,
        quantity: Decimal,
        entry_price: Decimal,
        initial_margin_rate: Decimal,
        maintenance_margin_rate: Decimal,
    ) -> Self {
        let now = Utc::now();
        let notional = quantity * entry_price;
        let initial_margin = notional * initial_margin_rate;
        let maintenance_margin = notional * maintenance_margin_rate;

        // Calculate liquidation price based on side
        let liquidation_price = match side {
            PositionSide::Long => {
                // Long: liquidate when price drops to where margin is depleted
                // liquidation_price = entry_price * (1 - initial_margin_rate + maintenance_margin_rate)
                entry_price * (Decimal::ONE - initial_margin_rate + maintenance_margin_rate)
            }
            PositionSide::Short => {
                // Short: liquidate when price rises to where margin is depleted
                // liquidation_price = entry_price * (1 + initial_margin_rate - maintenance_margin_rate)
                entry_price * (Decimal::ONE + initial_margin_rate - maintenance_margin_rate)
            }
        };

        Self {
            id: Uuid::new_v4(),
            account_id,
            instrument_id,
            side,
            quantity,
            entry_price,
            mark_price: entry_price,
            realized_pnl: Decimal::ZERO,
            initial_margin,
            maintenance_margin,
            liquidation_price,
            opened_at: now,
            updated_at: now,
        }
    }

    /// Calculate unrealized P&L based on current mark price
    pub fn unrealized_pnl(&self) -> Decimal {
        let price_diff = self.mark_price - self.entry_price;
        match self.side {
            PositionSide::Long => self.quantity * price_diff,
            PositionSide::Short => self.quantity * -price_diff,
        }
    }

    /// Calculate total P&L (realized + unrealized)
    pub fn total_pnl(&self) -> Decimal {
        self.realized_pnl + self.unrealized_pnl()
    }

    /// Calculate current notional value
    pub fn notional_value(&self) -> Decimal {
        self.quantity * self.mark_price
    }

    /// Calculate current margin ratio
    pub fn margin_ratio(&self) -> Decimal {
        if self.notional_value() == Decimal::ZERO {
            return Decimal::ZERO;
        }
        (self.initial_margin + self.unrealized_pnl()) / self.notional_value()
    }

    /// Check if position should be liquidated
    pub fn should_liquidate(&self) -> bool {
        match self.side {
            PositionSide::Long => self.mark_price <= self.liquidation_price,
            PositionSide::Short => self.mark_price >= self.liquidation_price,
        }
    }

    /// Update mark price and recalculate margins
    pub fn update_mark_price(&mut self, new_price: Decimal) {
        self.mark_price = new_price;
        self.updated_at = Utc::now();
    }

    /// Increase position size (add to existing position)
    pub fn increase(
        &mut self,
        quantity: Decimal,
        price: Decimal,
        initial_margin_rate: Decimal,
        maintenance_margin_rate: Decimal,
    ) {
        // Calculate new average entry price
        let old_notional = self.quantity * self.entry_price;
        let new_notional = quantity * price;
        let total_quantity = self.quantity + quantity;

        if total_quantity > Decimal::ZERO {
            self.entry_price = (old_notional + new_notional) / total_quantity;
        }

        self.quantity = total_quantity;

        // Recalculate margins
        let notional = self.quantity * self.entry_price;
        self.initial_margin = notional * initial_margin_rate;
        self.maintenance_margin = notional * maintenance_margin_rate;

        // Recalculate liquidation price
        self.liquidation_price = match self.side {
            PositionSide::Long => {
                self.entry_price * (Decimal::ONE - initial_margin_rate + maintenance_margin_rate)
            }
            PositionSide::Short => {
                self.entry_price * (Decimal::ONE + initial_margin_rate - maintenance_margin_rate)
            }
        };

        self.updated_at = Utc::now();
    }

    /// Decrease position size (reduce existing position)
    /// Returns the realized P&L from the reduction
    pub fn decrease(
        &mut self,
        quantity: Decimal,
        price: Decimal,
        initial_margin_rate: Decimal,
        maintenance_margin_rate: Decimal,
    ) -> Decimal {
        let reduce_qty = quantity.min(self.quantity);

        // Calculate realized P&L for the reduced portion
        let price_diff = price - self.entry_price;
        let pnl = match self.side {
            PositionSide::Long => reduce_qty * price_diff,
            PositionSide::Short => reduce_qty * -price_diff,
        };

        self.realized_pnl += pnl;
        self.quantity -= reduce_qty;

        // Recalculate margins for remaining position
        if self.quantity > Decimal::ZERO {
            let notional = self.quantity * self.entry_price;
            self.initial_margin = notional * initial_margin_rate;
            self.maintenance_margin = notional * maintenance_margin_rate;
        } else {
            self.initial_margin = Decimal::ZERO;
            self.maintenance_margin = Decimal::ZERO;
        }

        self.updated_at = Utc::now();
        pnl
    }

    /// Check if position is closed (quantity is zero)
    pub fn is_closed(&self) -> bool {
        self.quantity == Decimal::ZERO
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn create_test_position(side: PositionSide) -> Position {
        Position::new(
            Uuid::new_v4(),
            InstrumentId::new("BTC/USD"),
            side,
            dec!(1.0),
            dec!(50000.0),
            dec!(0.10), // 10% initial margin
            dec!(0.05), // 5% maintenance margin
        )
    }

    #[test]
    fn test_long_position_creation() {
        let pos = create_test_position(PositionSide::Long);

        assert_eq!(pos.quantity, dec!(1.0));
        assert_eq!(pos.entry_price, dec!(50000.0));
        assert_eq!(pos.initial_margin, dec!(5000.0)); // 10% of 50000
        assert_eq!(pos.maintenance_margin, dec!(2500.0)); // 5% of 50000
        // Liquidation: 50000 * (1 - 0.10 + 0.05) = 50000 * 0.95 = 47500
        assert_eq!(pos.liquidation_price, dec!(47500.0));
    }

    #[test]
    fn test_short_position_creation() {
        let pos = create_test_position(PositionSide::Short);

        assert_eq!(pos.quantity, dec!(1.0));
        assert_eq!(pos.entry_price, dec!(50000.0));
        // Liquidation: 50000 * (1 + 0.10 - 0.05) = 50000 * 1.05 = 52500
        assert_eq!(pos.liquidation_price, dec!(52500.0));
    }

    #[test]
    fn test_long_unrealized_pnl() {
        let mut pos = create_test_position(PositionSide::Long);

        // Price goes up - profit
        pos.update_mark_price(dec!(55000.0));
        assert_eq!(pos.unrealized_pnl(), dec!(5000.0));

        // Price goes down - loss
        pos.update_mark_price(dec!(48000.0));
        assert_eq!(pos.unrealized_pnl(), dec!(-2000.0));
    }

    #[test]
    fn test_short_unrealized_pnl() {
        let mut pos = create_test_position(PositionSide::Short);

        // Price goes down - profit for short
        pos.update_mark_price(dec!(45000.0));
        assert_eq!(pos.unrealized_pnl(), dec!(5000.0));

        // Price goes up - loss for short
        pos.update_mark_price(dec!(52000.0));
        assert_eq!(pos.unrealized_pnl(), dec!(-2000.0));
    }

    #[test]
    fn test_long_liquidation_check() {
        let mut pos = create_test_position(PositionSide::Long);

        // Above liquidation price - safe
        pos.update_mark_price(dec!(48000.0));
        assert!(!pos.should_liquidate());

        // At liquidation price - liquidate
        pos.update_mark_price(dec!(47500.0));
        assert!(pos.should_liquidate());

        // Below liquidation price - definitely liquidate
        pos.update_mark_price(dec!(45000.0));
        assert!(pos.should_liquidate());
    }

    #[test]
    fn test_short_liquidation_check() {
        let mut pos = create_test_position(PositionSide::Short);

        // Below liquidation price - safe
        pos.update_mark_price(dec!(51000.0));
        assert!(!pos.should_liquidate());

        // At liquidation price - liquidate
        pos.update_mark_price(dec!(52500.0));
        assert!(pos.should_liquidate());

        // Above liquidation price - definitely liquidate
        pos.update_mark_price(dec!(55000.0));
        assert!(pos.should_liquidate());
    }

    #[test]
    fn test_position_increase() {
        let mut pos = create_test_position(PositionSide::Long);

        // Add more at a different price
        pos.increase(dec!(1.0), dec!(52000.0), dec!(0.10), dec!(0.05));

        assert_eq!(pos.quantity, dec!(2.0));
        // Average price: (50000 + 52000) / 2 = 51000
        assert_eq!(pos.entry_price, dec!(51000.0));
        // New margin: 2 * 51000 * 0.10 = 10200
        assert_eq!(pos.initial_margin, dec!(10200.0));
    }

    #[test]
    fn test_position_decrease() {
        let mut pos = create_test_position(PositionSide::Long);

        // Close half at a profit
        let pnl = pos.decrease(dec!(0.5), dec!(55000.0), dec!(0.10), dec!(0.05));

        // Realized P&L: 0.5 * (55000 - 50000) = 2500
        assert_eq!(pnl, dec!(2500.0));
        assert_eq!(pos.realized_pnl, dec!(2500.0));
        assert_eq!(pos.quantity, dec!(0.5));
    }

    #[test]
    fn test_position_close() {
        let mut pos = create_test_position(PositionSide::Long);

        // Close entire position
        let pnl = pos.decrease(dec!(1.0), dec!(55000.0), dec!(0.10), dec!(0.05));

        assert_eq!(pnl, dec!(5000.0));
        assert!(pos.is_closed());
        assert_eq!(pos.quantity, Decimal::ZERO);
    }
}
