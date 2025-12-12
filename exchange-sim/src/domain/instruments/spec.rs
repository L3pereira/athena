use crate::domain::{Price, Quantity, Rate};

/// Common specification trait for all tradeable instruments
pub trait InstrumentSpec: Send + Sync {
    /// Unique symbol identifier
    fn symbol(&self) -> &str;

    /// Minimum price increment
    fn tick_size(&self) -> Price;

    /// Minimum quantity increment
    fn lot_size(&self) -> Quantity;

    /// Base asset (e.g., BTC in BTCUSDT)
    fn base_asset(&self) -> &str;

    /// Quote/settlement asset (e.g., USDT in BTCUSDT)
    fn quote_asset(&self) -> &str;

    /// Whether this is a derivative instrument
    fn is_derivative(&self) -> bool {
        false
    }

    /// Margin requirement (in basis points, e.g., 10000 = 100%)
    fn margin_requirement(&self) -> Rate {
        Rate::ONE // 100% margin (no leverage) by default
    }

    /// Can this instrument be shorted?
    fn is_shortable(&self) -> bool {
        false
    }

    /// Maximum leverage (10000 / margin_bps)
    fn max_leverage(&self) -> i64 {
        let margin_bps = self.margin_requirement().bps();
        if margin_bps == 0 {
            return 1;
        }
        10000 / margin_bps
    }

    /// Validate price against tick size
    fn validate_price(&self, price: Price) -> bool {
        if self.tick_size().raw() == 0 {
            return true;
        }
        (price.raw() % self.tick_size().raw()) == 0
    }

    /// Validate quantity against lot size
    fn validate_quantity(&self, quantity: Quantity) -> bool {
        if self.lot_size().raw() == 0 {
            return true;
        }
        (quantity.raw() % self.lot_size().raw()) == 0
    }

    /// Round price to nearest valid tick
    fn round_price(&self, price: Price) -> Price {
        if self.tick_size().raw() == 0 {
            return price;
        }
        let tick = self.tick_size().raw();
        let ticks = price.raw() / tick;
        Price::from_raw(ticks * tick)
    }

    /// Round quantity to nearest valid lot
    fn round_quantity(&self, quantity: Quantity) -> Quantity {
        if self.lot_size().raw() == 0 {
            return quantity;
        }
        let lot = self.lot_size().raw();
        let lots = quantity.raw() / lot;
        Quantity::from_raw(lots * lot)
    }
}
