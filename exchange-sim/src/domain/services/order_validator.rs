use crate::domain::entities::{Order, OrderBook, TradingPairConfig};
use crate::domain::value_objects::OrderType;

/// Validates orders against instrument rules and book state
pub struct OrderValidator;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub code: i32,
    pub message: String,
}

impl ValidationError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        ValidationError {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for ValidationError {}

impl OrderValidator {
    /// Validate an order against trading pair configuration
    pub fn validate_order(
        order: &Order,
        config: &TradingPairConfig,
        book: &OrderBook,
    ) -> Result<(), ValidationError> {
        // Basic order validation
        order
            .validate()
            .map_err(|e| ValidationError::new(-1100, e))?;

        // Check instrument is trading
        if !config.is_trading() {
            return Err(ValidationError::new(-1013, "Market is closed"));
        }

        // Validate quantity against lot size
        if !config.validate_quantity(order.quantity) {
            return Err(ValidationError::new(
                -1013,
                format!(
                    "Quantity {} is not aligned to lot size {}",
                    order.quantity, config.lot_size
                ),
            ));
        }

        // Validate price for limit orders
        if let Some(price) = order.price {
            if !config.validate_price(price) {
                return Err(ValidationError::new(
                    -1013,
                    format!(
                        "Price {} is not aligned to tick size {}",
                        price, config.tick_size
                    ),
                ));
            }

            // Validate notional value
            if !config.validate_notional(price, order.quantity) {
                return Err(ValidationError::new(
                    -1013,
                    format!("Notional value below minimum {}", config.min_notional),
                ));
            }
        }

        // LIMIT_MAKER orders must not be immediately matchable
        if order.order_type == OrderType::LimitMaker {
            let best_bid = book.best_bid();
            let best_ask = book.best_ask();

            if order.is_marketable(best_bid, best_ask) {
                return Err(ValidationError::new(
                    -2010,
                    "LIMIT_MAKER order would immediately match",
                ));
            }
        }

        Ok(())
    }

    /// Validate that an order can be canceled
    pub fn validate_cancel(order: &Order) -> Result<(), ValidationError> {
        if order.status.is_final() {
            return Err(ValidationError::new(
                -2011,
                format!(
                    "Order is already {}",
                    format!("{:?}", order.status).to_uppercase()
                ),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::value_objects::*;
    use rust_decimal_macros::dec;

    fn create_config() -> TradingPairConfig {
        TradingPairConfig::new(Symbol::new("BTCUSDT").unwrap(), "BTC", "USDT")
            .with_tick_size(Price::from(dec!(0.01)))
            .with_lot_size(Quantity::from(dec!(0.001)))
    }

    #[test]
    fn test_valid_order() {
        let config = create_config();
        let book = OrderBook::new(config.symbol.clone());

        let order = Order::new_limit(
            config.symbol.clone(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100.00)),
            TimeInForce::Gtc,
        );

        assert!(OrderValidator::validate_order(&order, &config, &book).is_ok());
    }

    #[test]
    fn test_invalid_tick_size() {
        let config = create_config();
        let book = OrderBook::new(config.symbol.clone());

        let order = Order::new_limit(
            config.symbol.clone(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100.001)), // Not aligned to 0.01 tick size
            TimeInForce::Gtc,
        );

        let result = OrderValidator::validate_order(&order, &config, &book);
        assert!(result.is_err());
    }
}
