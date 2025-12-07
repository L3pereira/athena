use crate::domain::{Price, Quantity};
use rust_decimal::Decimal;

/// Common specification trait for all tradeable instruments
pub trait InstrumentSpec: Send + Sync {
    /// Unique symbol identifier
    fn symbol(&self) -> &str;

    /// Minimum price increment
    fn tick_size(&self) -> Price;

    /// Minimum quantity increment
    fn lot_size(&self) -> Quantity;

    /// Margin requirement (as decimal, e.g., 0.01 = 1%)
    fn margin_requirement(&self) -> Decimal {
        Decimal::ONE // 100% margin (no leverage) by default
    }

    /// Can this instrument be shorted?
    fn is_shortable(&self) -> bool {
        false
    }

    /// Maximum leverage (1 / margin_requirement)
    fn max_leverage(&self) -> Decimal {
        if self.margin_requirement().is_zero() {
            return Decimal::ONE;
        }
        Decimal::ONE / self.margin_requirement()
    }

    /// Validate price against tick size
    fn validate_price(&self, price: Price) -> bool {
        if self.tick_size().is_zero() {
            return true;
        }
        (price.inner() % self.tick_size().inner()).is_zero()
    }

    /// Validate quantity against lot size
    fn validate_quantity(&self, quantity: Quantity) -> bool {
        if self.lot_size().is_zero() {
            return true;
        }
        (quantity.inner() % self.lot_size().inner()).is_zero()
    }

    /// Round price to nearest valid tick
    fn round_price(&self, price: Price) -> Price {
        if self.tick_size().is_zero() {
            return price;
        }
        let ticks = (price.inner() / self.tick_size().inner()).round();
        Price::from(ticks * self.tick_size().inner())
    }

    /// Round quantity to nearest valid lot
    fn round_quantity(&self, quantity: Quantity) -> Quantity {
        if self.lot_size().is_zero() {
            return quantity;
        }
        let lots = (quantity.inner() / self.lot_size().inner()).floor();
        Quantity::from(lots * self.lot_size().inner())
    }
}
