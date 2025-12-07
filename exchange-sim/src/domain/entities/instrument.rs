use crate::domain::value_objects::{Price, Quantity, Symbol};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Trading pair configuration (exchange-level settings)
///
/// This represents the exchange's configuration for a trading pair,
/// including tick sizes, lot sizes, trading status, etc.
///
/// For financial instrument type definitions (Spot, Perpetual, Option),
/// see the `instruments` module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPairConfig {
    pub symbol: Symbol,
    pub base_asset: String,
    pub quote_asset: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Minimum notional value (price * quantity)
    pub min_notional: Decimal,
    /// Minimum quantity allowed
    pub min_quantity: Quantity,
    /// Maximum quantity allowed
    pub max_quantity: Quantity,
    /// Status: TRADING, HALT, BREAK, etc.
    pub status: InstrumentStatus,
    /// Allowed order types for this instrument
    pub order_types: Vec<String>,
    /// Maker fee rate (negative = rebate, e.g., -0.0001 = -0.01% rebate)
    pub maker_fee_rate: Decimal,
    /// Taker fee rate (e.g., 0.001 = 0.1%)
    pub taker_fee_rate: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstrumentStatus {
    #[default]
    Trading,
    Halt,
    Break,
    PreTrading,
    PostTrading,
}

impl TradingPairConfig {
    pub fn new(
        symbol: Symbol,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
    ) -> Self {
        TradingPairConfig {
            symbol,
            base_asset: base_asset.into(),
            quote_asset: quote_asset.into(),
            tick_size: Price::from(Decimal::new(1, 2)), // 0.01
            lot_size: Quantity::from(Decimal::new(1, 3)), // 0.001
            min_notional: Decimal::new(10, 0),          // 10
            min_quantity: Quantity::from(Decimal::new(1, 6)), // 0.000001
            max_quantity: Quantity::from(Decimal::new(9000, 0)), // 9000
            status: InstrumentStatus::Trading,
            order_types: vec![
                "LIMIT".to_string(),
                "MARKET".to_string(),
                "LIMIT_MAKER".to_string(),
                "STOP_LOSS".to_string(),
                "STOP_LOSS_LIMIT".to_string(),
                "TAKE_PROFIT".to_string(),
                "TAKE_PROFIT_LIMIT".to_string(),
            ],
            maker_fee_rate: Decimal::new(1, 4), // 0.0001 = 0.01% (1 bps)
            taker_fee_rate: Decimal::new(2, 4), // 0.0002 = 0.02% (2 bps)
        }
    }

    pub fn with_tick_size(mut self, tick_size: Price) -> Self {
        self.tick_size = tick_size;
        self
    }

    pub fn with_lot_size(mut self, lot_size: Quantity) -> Self {
        self.lot_size = lot_size;
        self
    }

    pub fn with_min_notional(mut self, min_notional: Decimal) -> Self {
        self.min_notional = min_notional;
        self
    }

    /// Set maker fee rate (can be negative for rebates)
    pub fn with_maker_fee(mut self, rate: Decimal) -> Self {
        self.maker_fee_rate = rate;
        self
    }

    /// Set taker fee rate
    pub fn with_taker_fee(mut self, rate: Decimal) -> Self {
        self.taker_fee_rate = rate;
        self
    }

    /// Set both maker and taker fees at once
    pub fn with_fees(mut self, maker: Decimal, taker: Decimal) -> Self {
        self.maker_fee_rate = maker;
        self.taker_fee_rate = taker;
        self
    }

    /// Calculate fee for a trade
    /// Returns (fee_amount, is_rebate)
    pub fn calculate_fee(&self, notional: Decimal, is_maker: bool) -> (Decimal, bool) {
        let rate = if is_maker {
            self.maker_fee_rate
        } else {
            self.taker_fee_rate
        };
        let fee = notional * rate;
        (fee.abs(), rate < Decimal::ZERO)
    }

    pub fn is_trading(&self) -> bool {
        self.status == InstrumentStatus::Trading
    }

    pub fn validate_price(&self, price: Price) -> bool {
        if price.is_zero() {
            return false;
        }
        // Check if price is aligned to tick size
        let rounded = price.round_to_tick(self.tick_size);
        rounded == price
    }

    pub fn validate_quantity(&self, quantity: Quantity) -> bool {
        if quantity < self.min_quantity || quantity > self.max_quantity {
            return false;
        }
        // Check if quantity is aligned to lot size
        let rounded = quantity.round_to_lot(self.lot_size);
        rounded == quantity
    }

    pub fn validate_notional(&self, price: Price, quantity: Quantity) -> bool {
        let notional = price.inner() * quantity.inner();
        notional >= self.min_notional
    }

    pub fn round_price(&self, price: Price) -> Price {
        price.round_to_tick(self.tick_size)
    }

    pub fn round_quantity(&self, quantity: Quantity) -> Quantity {
        quantity.round_to_lot(self.lot_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_trading_pair_config_default_fees() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT");

        // Default fees: 1 bps maker, 2 bps taker
        assert_eq!(config.maker_fee_rate, Decimal::new(1, 4)); // 0.0001
        assert_eq!(config.taker_fee_rate, Decimal::new(2, 4)); // 0.0002
    }

    #[test]
    fn test_trading_pair_config_with_fees() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT")
            .with_maker_fee(dec!(0.0005)) // 5 bps
            .with_taker_fee(dec!(0.001)); // 10 bps

        assert_eq!(config.maker_fee_rate, dec!(0.0005));
        assert_eq!(config.taker_fee_rate, dec!(0.001));
    }

    #[test]
    fn test_trading_pair_config_with_fees_combined() {
        let symbol = Symbol::new("ETHUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "ETH", "USDT").with_fees(dec!(-0.0001), dec!(0.0003)); // Maker rebate, taker fee

        assert_eq!(config.maker_fee_rate, dec!(-0.0001)); // Rebate
        assert_eq!(config.taker_fee_rate, dec!(0.0003));
    }

    #[test]
    fn test_calculate_fee_taker() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "BTC", "USDT").with_fees(dec!(0.0001), dec!(0.0002));

        // $10,000 trade as taker
        let notional = dec!(10000);
        let (fee_amount, is_rebate) = config.calculate_fee(notional, false);

        assert_eq!(fee_amount, dec!(2.00)); // $2.00 taker fee
        assert!(!is_rebate);
    }

    #[test]
    fn test_calculate_fee_maker() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "BTC", "USDT").with_fees(dec!(0.0001), dec!(0.0002));

        // $10,000 trade as maker
        let notional = dec!(10000);
        let (fee_amount, is_rebate) = config.calculate_fee(notional, true);

        assert_eq!(fee_amount, dec!(1.00)); // $1.00 maker fee
        assert!(!is_rebate);
    }

    #[test]
    fn test_calculate_fee_maker_rebate() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "BTC", "USDT").with_fees(dec!(-0.0001), dec!(0.0002)); // Negative maker = rebate

        // $10,000 trade as maker
        let notional = dec!(10000);
        let (fee_amount, is_rebate) = config.calculate_fee(notional, true);

        assert_eq!(fee_amount, dec!(1.00)); // $1.00 rebate amount (absolute value)
        assert!(is_rebate); // This is a rebate!
    }

    #[test]
    fn test_validate_price_tick_size() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "BTC", "USDT").with_tick_size(Price::from(dec!(0.01)));

        // Valid prices (aligned to 0.01 tick)
        assert!(config.validate_price(Price::from(dec!(100.00))));
        assert!(config.validate_price(Price::from(dec!(100.01))));
        assert!(config.validate_price(Price::from(dec!(99.99))));

        // Invalid price (not aligned to tick)
        assert!(!config.validate_price(Price::from(dec!(100.001))));

        // Zero price is invalid
        assert!(!config.validate_price(Price::from(dec!(0))));
    }

    #[test]
    fn test_validate_quantity_lot_size() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT")
            .with_lot_size(Quantity::from(dec!(0.001)));

        // Valid quantities (aligned to 0.001 lot)
        assert!(config.validate_quantity(Quantity::from(dec!(1.000))));
        assert!(config.validate_quantity(Quantity::from(dec!(0.001))));
        assert!(config.validate_quantity(Quantity::from(dec!(1.234))));

        // Invalid quantity (not aligned to lot)
        assert!(!config.validate_quantity(Quantity::from(dec!(1.0001))));
    }
}
