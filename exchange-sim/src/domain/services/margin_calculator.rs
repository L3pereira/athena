//! Margin and liquidation calculation services.

use crate::domain::entities::{Position, PositionSide};
use crate::domain::value_objects::Price;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Trait for margin calculations - allows different margin models
pub trait MarginCalculator: Send + Sync {
    /// Calculate unrealized P&L for a position
    fn unrealized_pnl(&self, position: &Position) -> Decimal;

    /// Calculate liquidation price for a position
    fn liquidation_price(&self, position: &Position, maintenance_margin_rate: Decimal) -> Price;

    /// Check if position should be liquidated
    fn should_liquidate(&self, position: &Position, maintenance_margin_rate: Decimal) -> bool;

    /// Calculate required initial margin for a new position
    fn required_margin(
        &self,
        quantity: Decimal,
        price: Decimal,
        initial_margin_rate: Decimal,
    ) -> Decimal;
}

/// Standard margin calculator used by most exchanges
#[derive(Debug, Clone, Default)]
pub struct StandardMarginCalculator;

impl MarginCalculator for StandardMarginCalculator {
    fn unrealized_pnl(&self, position: &Position) -> Decimal {
        let qty = position.quantity.inner();
        let entry = position.entry_price.inner();
        let mark = position.mark_price.inner();

        match position.side {
            PositionSide::Long => qty * (mark - entry),
            PositionSide::Short => qty * (entry - mark),
        }
    }

    fn liquidation_price(&self, position: &Position, maintenance_margin_rate: Decimal) -> Price {
        let entry = position.entry_price.inner();
        let notional = position.quantity.inner() * position.mark_price.inner();

        if notional.is_zero() {
            return Price::from(Decimal::ZERO);
        }

        let margin_ratio = position.margin / notional;

        let liq_price = match position.side {
            // Long: liquidate when price drops enough that margin is depleted
            PositionSide::Long => entry * (Decimal::ONE - margin_ratio + maintenance_margin_rate),
            // Short: liquidate when price rises enough
            PositionSide::Short => entry * (Decimal::ONE + margin_ratio - maintenance_margin_rate),
        };

        Price::from(liq_price.max(Decimal::ZERO))
    }

    fn should_liquidate(&self, position: &Position, maintenance_margin_rate: Decimal) -> bool {
        let liq_price = self.liquidation_price(position, maintenance_margin_rate);
        match position.side {
            PositionSide::Long => position.mark_price <= liq_price,
            PositionSide::Short => position.mark_price >= liq_price,
        }
    }

    fn required_margin(
        &self,
        quantity: Decimal,
        price: Decimal,
        initial_margin_rate: Decimal,
    ) -> Decimal {
        quantity * price * initial_margin_rate
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
    pub fn total_unrealized_pnl<'a>(
        &self,
        positions: impl Iterator<Item = &'a Position>,
    ) -> Decimal {
        positions.map(|p| self.calculator.unrealized_pnl(p)).sum()
    }

    /// Calculate total used margin
    pub fn total_used_margin<'a>(&self, positions: impl Iterator<Item = &'a Position>) -> Decimal {
        positions.map(|p| p.margin).sum()
    }

    /// Calculate margin ratio (equity / maintenance margin required)
    pub fn margin_ratio<'a>(
        &self,
        equity: Decimal,
        positions: impl Iterator<Item = &'a Position>,
        maintenance_margin_rate: Decimal,
    ) -> Decimal {
        let maintenance_required: Decimal = positions
            .map(|p| p.notional_value() * maintenance_margin_rate)
            .sum();

        if maintenance_required.is_zero() {
            return Decimal::MAX;
        }

        equity / maintenance_required
    }

    /// Get positions that should be liquidated
    pub fn liquidatable_positions<'a>(
        &self,
        positions: impl Iterator<Item = &'a Position>,
        maintenance_margin_rate: Decimal,
    ) -> Vec<&'a Position> {
        positions
            .filter(|p| self.calculator.should_liquidate(p, maintenance_margin_rate))
            .collect()
    }

    /// Determine account status based on margin ratio
    pub fn determine_status(&self, margin_ratio: Decimal, has_positions: bool) -> MarginStatus {
        if !has_positions {
            return MarginStatus::Healthy;
        }

        if margin_ratio < Decimal::ONE {
            MarginStatus::Liquidating
        } else if margin_ratio < dec!(1.2) {
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
    use crate::domain::value_objects::{Quantity, Symbol};
    use rust_decimal_macros::dec;

    fn make_position(
        side: PositionSide,
        entry: Decimal,
        mark: Decimal,
        margin: Decimal,
    ) -> Position {
        let mut pos = Position::new(
            Symbol::new("BTCUSDT").unwrap(),
            side,
            Quantity::from(dec!(1)),
            Price::from(entry),
            margin,
            chrono::Utc::now(),
        );
        pos.mark_price = Price::from(mark);
        pos
    }

    #[test]
    fn test_long_unrealized_pnl() {
        let calc = StandardMarginCalculator;

        // Long at 50k, mark at 55k = +5k profit
        let pos = make_position(PositionSide::Long, dec!(50000), dec!(55000), dec!(5000));
        assert_eq!(calc.unrealized_pnl(&pos), dec!(5000));

        // Long at 50k, mark at 45k = -5k loss
        let pos = make_position(PositionSide::Long, dec!(50000), dec!(45000), dec!(5000));
        assert_eq!(calc.unrealized_pnl(&pos), dec!(-5000));
    }

    #[test]
    fn test_short_unrealized_pnl() {
        let calc = StandardMarginCalculator;

        // Short at 50k, mark at 45k = +5k profit
        let pos = make_position(PositionSide::Short, dec!(50000), dec!(45000), dec!(5000));
        assert_eq!(calc.unrealized_pnl(&pos), dec!(5000));

        // Short at 50k, mark at 55k = -5k loss
        let pos = make_position(PositionSide::Short, dec!(50000), dec!(55000), dec!(5000));
        assert_eq!(calc.unrealized_pnl(&pos), dec!(-5000));
    }

    #[test]
    fn test_liquidation_price_short() {
        let calc = StandardMarginCalculator;

        // Short 1 BTC at $50k with 10% margin ($5k)
        let pos = make_position(PositionSide::Short, dec!(50000), dec!(50000), dec!(5000));

        // Liquidation price for short = entry * (1 + margin_ratio - maintenance)
        // = 50000 * (1 + 0.10 - 0.05) = 50000 * 1.05 = 52500
        let liq_price = calc.liquidation_price(&pos, dec!(0.05));
        assert_eq!(liq_price, Price::from(dec!(52500)));
    }

    #[test]
    fn test_margin_status() {
        let calc = AccountMarginCalculator::default();

        assert_eq!(
            calc.determine_status(dec!(2.0), true),
            MarginStatus::Healthy
        );
        assert_eq!(
            calc.determine_status(dec!(1.1), true),
            MarginStatus::MarginCall
        );
        assert_eq!(
            calc.determine_status(dec!(0.9), true),
            MarginStatus::Liquidating
        );
        assert_eq!(
            calc.determine_status(dec!(0.5), false),
            MarginStatus::Healthy
        );
    }
}
