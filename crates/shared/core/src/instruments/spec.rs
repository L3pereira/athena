use rust_decimal::Decimal;

use crate::values::{Price, Quantity};

/// Common specification trait for all instruments
///
/// Every tradeable instrument must define these properties for proper
/// order validation, matching, and risk calculations.
pub trait InstrumentSpec {
    /// Unique symbol for this instrument (e.g., "BTC/USD", "BTC-PERP")
    fn symbol(&self) -> &str;

    /// Minimum price increment (e.g., 0.01 for most USD pairs)
    fn tick_size(&self) -> Price;

    /// Minimum quantity increment (e.g., 0.001 for BTC)
    fn lot_size(&self) -> Quantity;

    /// Initial margin requirement as a decimal (e.g., 0.1 = 10%)
    /// For spot, this is typically 1.0 (100% = no leverage)
    fn margin_requirement(&self) -> Decimal;

    /// Maintenance margin requirement as a decimal
    /// Position is liquidated if margin falls below this
    fn maintenance_margin(&self) -> Decimal {
        self.margin_requirement() * Decimal::new(75, 2) // 75% of initial by default
    }

    /// Maximum leverage allowed (inverse of margin requirement)
    fn max_leverage(&self) -> Decimal {
        Decimal::ONE / self.margin_requirement()
    }

    /// Whether this instrument can be shorted
    fn is_shortable(&self) -> bool {
        true // Most instruments can be shorted
    }

    /// Validate that a price conforms to tick size
    fn validate_price(&self, price: Price) -> bool {
        let tick = self.tick_size();
        if tick == Decimal::ZERO {
            return true;
        }
        (price % tick) == Decimal::ZERO
    }

    /// Validate that a quantity conforms to lot size
    fn validate_quantity(&self, quantity: Quantity) -> bool {
        let lot = self.lot_size();
        if lot == Decimal::ZERO {
            return true;
        }
        (quantity % lot) == Decimal::ZERO
    }

    /// Round a price down to the nearest valid tick
    fn round_price_down(&self, price: Price) -> Price {
        let tick = self.tick_size();
        if tick == Decimal::ZERO {
            return price;
        }
        (price / tick).floor() * tick
    }

    /// Round a price up to the nearest valid tick
    fn round_price_up(&self, price: Price) -> Price {
        let tick = self.tick_size();
        if tick == Decimal::ZERO {
            return price;
        }
        (price / tick).ceil() * tick
    }

    /// Round a quantity down to the nearest valid lot
    fn round_quantity_down(&self, quantity: Quantity) -> Quantity {
        let lot = self.lot_size();
        if lot == Decimal::ZERO {
            return quantity;
        }
        (quantity / lot).floor() * lot
    }
}
