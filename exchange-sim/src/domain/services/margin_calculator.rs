//! Margin and liquidation calculation services.

use crate::domain::entities::{Position, PositionSide};
use crate::domain::value_objects::{PRICE_SCALE, Price, Quantity, Rate, Value};

/// Trait for margin calculations - allows different margin models
pub trait MarginCalculator: Send + Sync {
    /// Calculate unrealized P&L for a position
    fn unrealized_pnl(&self, position: &Position) -> Value;

    /// Calculate liquidation price for a position
    fn liquidation_price(&self, position: &Position, maintenance_margin_rate: Rate) -> Price;

    /// Check if position should be liquidated
    fn should_liquidate(&self, position: &Position, maintenance_margin_rate: Rate) -> bool;

    /// Calculate required initial margin for a new position
    fn required_margin(&self, quantity: Quantity, price: Price, initial_margin_rate: Rate)
    -> Value;
}

/// Standard margin calculator used by most exchanges
#[derive(Debug, Clone, Default)]
pub struct StandardMarginCalculator;

impl MarginCalculator for StandardMarginCalculator {
    fn unrealized_pnl(&self, position: &Position) -> Value {
        let qty = position.quantity.raw() as i128;
        let entry = position.entry_price.raw() as i128;
        let mark = position.mark_price.raw() as i128;

        let pnl_raw = match position.side {
            // qty * (mark - entry) / PRICE_SCALE (to avoid double scaling)
            PositionSide::Long => qty * (mark - entry) / PRICE_SCALE as i128,
            PositionSide::Short => qty * (entry - mark) / PRICE_SCALE as i128,
        };
        Value::from_raw(pnl_raw)
    }

    fn liquidation_price(&self, position: &Position, maintenance_margin_rate: Rate) -> Price {
        let entry = position.entry_price.raw() as i128;
        let notional = position.notional_value();

        if notional.raw() == 0 {
            return Price::from_raw(0);
        }

        // margin_ratio = margin / notional (as a fraction in PRICE_SCALE)
        let margin_ratio = (position.margin.raw() * PRICE_SCALE as i128) / notional.raw();
        let maint_rate = (maintenance_margin_rate.bps() as i128 * PRICE_SCALE as i128) / 10_000;

        let liq_price_raw = match position.side {
            // Long: entry * (1 - margin_ratio + maint_rate)
            // = entry * (PRICE_SCALE - margin_ratio + maint_rate) / PRICE_SCALE
            PositionSide::Long => {
                entry * (PRICE_SCALE as i128 - margin_ratio + maint_rate) / PRICE_SCALE as i128
            }
            // Short: entry * (1 + margin_ratio - maint_rate)
            PositionSide::Short => {
                entry * (PRICE_SCALE as i128 + margin_ratio - maint_rate) / PRICE_SCALE as i128
            }
        };

        Price::from_raw(liq_price_raw.max(0) as i64)
    }

    fn should_liquidate(&self, position: &Position, maintenance_margin_rate: Rate) -> bool {
        let liq_price = self.liquidation_price(position, maintenance_margin_rate);
        match position.side {
            PositionSide::Long => position.mark_price <= liq_price,
            PositionSide::Short => position.mark_price >= liq_price,
        }
    }

    fn required_margin(
        &self,
        quantity: Quantity,
        price: Price,
        initial_margin_rate: Rate,
    ) -> Value {
        // notional = quantity * price (as Value)
        let notional = price.mul_qty(quantity);
        // Apply margin rate
        initial_margin_rate.apply_to_value(notional)
    }
}

/// Account-level margin calculations
pub struct AccountMarginCalculator<M: MarginCalculator = StandardMarginCalculator> {
    calculator: M,
}

impl Default for AccountMarginCalculator<StandardMarginCalculator> {
    fn default() -> Self {
        Self::new(StandardMarginCalculator)
    }
}

impl<M: MarginCalculator> AccountMarginCalculator<M> {
    pub fn new(calculator: M) -> Self {
        Self { calculator }
    }

    /// Calculate total unrealized P&L across all positions
    pub fn total_unrealized_pnl<'a>(&self, positions: impl Iterator<Item = &'a Position>) -> Value {
        let total: i128 = positions
            .map(|p| self.calculator.unrealized_pnl(p).raw())
            .sum();
        Value::from_raw(total)
    }

    /// Calculate total used margin
    pub fn total_used_margin<'a>(&self, positions: impl Iterator<Item = &'a Position>) -> Value {
        let total: i128 = positions.map(|p| p.margin.raw()).sum();
        Value::from_raw(total)
    }

    /// Calculate margin ratio (equity / maintenance margin required)
    /// Returns ratio scaled by PRICE_SCALE (e.g., 1.0 = PRICE_SCALE)
    pub fn margin_ratio<'a>(
        &self,
        equity: Value,
        positions: impl Iterator<Item = &'a Position>,
        maintenance_margin_rate: Rate,
    ) -> i64 {
        let maintenance_required: i128 = positions
            .map(|p| {
                maintenance_margin_rate
                    .apply_to_value(p.notional_value())
                    .raw()
            })
            .sum();

        if maintenance_required == 0 {
            return i64::MAX;
        }

        // ratio = equity / maintenance_required, scaled by PRICE_SCALE
        ((equity.raw() * PRICE_SCALE as i128) / maintenance_required) as i64
    }

    /// Get positions that should be liquidated
    pub fn liquidatable_positions<'a>(
        &self,
        positions: impl Iterator<Item = &'a Position>,
        maintenance_margin_rate: Rate,
    ) -> Vec<&'a Position> {
        positions
            .filter(|p| self.calculator.should_liquidate(p, maintenance_margin_rate))
            .collect()
    }

    /// Determine account status based on margin ratio (scaled by PRICE_SCALE)
    /// margin_ratio of PRICE_SCALE = 1.0
    pub fn determine_status(&self, margin_ratio: i64, has_positions: bool) -> MarginStatus {
        if !has_positions {
            return MarginStatus::Healthy;
        }

        let one = PRICE_SCALE;
        let one_point_two = PRICE_SCALE * 12 / 10; // 1.2

        if margin_ratio < one {
            MarginStatus::Liquidating
        } else if margin_ratio < one_point_two {
            MarginStatus::MarginCall
        } else {
            MarginStatus::Healthy
        }
    }

    /// Get the underlying position calculator
    pub fn position_calculator(&self) -> &M {
        &self.calculator
    }
}

/// Margin status for an account
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarginStatus {
    Healthy,
    MarginCall,
    Liquidating,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::Symbol;

    fn make_position(side: PositionSide, entry: i64, mark: i64, margin: i128) -> Position {
        let mut pos = Position::new(
            Symbol::new("BTCUSDT").unwrap(),
            side,
            Quantity::from_int(1),
            Price::from_int(entry),
            Value::from_raw(margin * PRICE_SCALE as i128),
            chrono::Utc::now(),
        );
        pos.mark_price = Price::from_int(mark);
        pos
    }

    #[test]
    fn test_long_unrealized_pnl() {
        let calc = StandardMarginCalculator;

        // Long at 50k, mark at 55k = +5k profit
        let pos = make_position(PositionSide::Long, 50000, 55000, 5000);
        assert_eq!(calc.unrealized_pnl(&pos).raw(), 5000 * PRICE_SCALE as i128);

        // Long at 50k, mark at 45k = -5k loss
        let pos = make_position(PositionSide::Long, 50000, 45000, 5000);
        assert_eq!(calc.unrealized_pnl(&pos).raw(), -5000 * PRICE_SCALE as i128);
    }

    #[test]
    fn test_short_unrealized_pnl() {
        let calc = StandardMarginCalculator;

        // Short at 50k, mark at 45k = +5k profit
        let pos = make_position(PositionSide::Short, 50000, 45000, 5000);
        assert_eq!(calc.unrealized_pnl(&pos).raw(), 5000 * PRICE_SCALE as i128);

        // Short at 50k, mark at 55k = -5k loss
        let pos = make_position(PositionSide::Short, 50000, 55000, 5000);
        assert_eq!(calc.unrealized_pnl(&pos).raw(), -5000 * PRICE_SCALE as i128);
    }

    #[test]
    fn test_liquidation_price_short() {
        let calc = StandardMarginCalculator;

        // Short 1 BTC at $50k with 10% margin ($5k)
        let pos = make_position(PositionSide::Short, 50000, 50000, 5000);

        // Liquidation price for short = entry * (1 + margin_ratio - maintenance)
        // = 50000 * (1 + 0.10 - 0.05) = 50000 * 1.05 = 52500
        let liq_price = calc.liquidation_price(&pos, Rate::from_bps(500)); // 5% = 500 bps
        assert_eq!(liq_price.raw(), 52500 * PRICE_SCALE);
    }

    #[test]
    fn test_margin_status() {
        let calc = AccountMarginCalculator::default();

        // 2.0 ratio = 2 * PRICE_SCALE
        assert_eq!(
            calc.determine_status(2 * PRICE_SCALE, true),
            MarginStatus::Healthy
        );
        // 1.1 ratio
        assert_eq!(
            calc.determine_status(PRICE_SCALE * 11 / 10, true),
            MarginStatus::MarginCall
        );
        // 0.9 ratio
        assert_eq!(
            calc.determine_status(PRICE_SCALE * 9 / 10, true),
            MarginStatus::Liquidating
        );
        // No positions
        assert_eq!(
            calc.determine_status(PRICE_SCALE / 2, false),
            MarginStatus::Healthy
        );
    }
}
