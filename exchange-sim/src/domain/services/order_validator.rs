use crate::domain::entities::{Order, OrderBook, TradingPairConfig};
use crate::domain::value_objects::OrderType;

// ============================================================================
// Validation Error
// ============================================================================

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

// ============================================================================
// Focused Validator Traits
// ============================================================================

/// Validates basic order structure (no external dependencies)
pub trait BasicOrderValidator {
    fn validate_basic(&self, order: &Order) -> Result<(), ValidationError>;
}

/// Validates order against trading pair config (doesn't need book state)
pub trait ConfigValidator {
    fn validate_against_config(
        &self,
        order: &Order,
        config: &TradingPairConfig,
    ) -> Result<(), ValidationError>;
}

/// Validates order against book state (for LIMIT_MAKER etc.)
pub trait BookStateValidator {
    fn validate_against_book(&self, order: &Order, book: &OrderBook)
    -> Result<(), ValidationError>;
}

/// Validates order cancellation
pub trait CancelValidator {
    fn validate_cancel(&self, order: &Order) -> Result<(), ValidationError>;
}

// ============================================================================
// Standard Implementations
// ============================================================================

/// Standard basic order validator
pub struct StandardBasicValidator;

impl BasicOrderValidator for StandardBasicValidator {
    fn validate_basic(&self, order: &Order) -> Result<(), ValidationError> {
        order.validate().map_err(|e| ValidationError::new(-1100, e))
    }
}

/// Standard config-based validator (price, quantity, notional)
pub struct StandardConfigValidator;

impl ConfigValidator for StandardConfigValidator {
    fn validate_against_config(
        &self,
        order: &Order,
        config: &TradingPairConfig,
    ) -> Result<(), ValidationError> {
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

        Ok(())
    }
}

/// Standard book state validator (for post-only orders)
pub struct StandardBookValidator;

impl BookStateValidator for StandardBookValidator {
    fn validate_against_book(
        &self,
        order: &Order,
        book: &OrderBook,
    ) -> Result<(), ValidationError> {
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
}

/// Standard cancel validator
pub struct StandardCancelValidator;

impl CancelValidator for StandardCancelValidator {
    fn validate_cancel(&self, order: &Order) -> Result<(), ValidationError> {
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

// ============================================================================
// Composite Validator (backwards compatible)
// ============================================================================

/// Full order validator that composes all validation phases
/// Use this for backwards compatibility or when you need all validations
pub struct OrderValidator<
    B = StandardBasicValidator,
    C = StandardConfigValidator,
    K = StandardBookValidator,
> where
    B: BasicOrderValidator,
    C: ConfigValidator,
    K: BookStateValidator,
{
    basic: B,
    config: C,
    book: K,
}

impl Default for OrderValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl OrderValidator {
    pub fn new() -> Self {
        Self {
            basic: StandardBasicValidator,
            config: StandardConfigValidator,
            book: StandardBookValidator,
        }
    }

    /// Static method for backwards compatibility
    pub fn validate(
        order: &Order,
        config: &TradingPairConfig,
        book: &OrderBook,
    ) -> Result<(), ValidationError> {
        Self::new().validate_order(order, config, book)
    }

    /// Validate that an order can be canceled
    pub fn validate_cancel(order: &Order) -> Result<(), ValidationError> {
        StandardCancelValidator.validate_cancel(order)
    }
}

impl<B, C, K> OrderValidator<B, C, K>
where
    B: BasicOrderValidator,
    C: ConfigValidator,
    K: BookStateValidator,
{
    /// Create with custom validators
    pub fn with_validators(basic: B, config: C, book: K) -> Self {
        Self {
            basic,
            config,
            book,
        }
    }

    /// Validate an order against trading pair configuration
    pub fn validate_order(
        &self,
        order: &Order,
        config: &TradingPairConfig,
        book: &OrderBook,
    ) -> Result<(), ValidationError> {
        self.basic.validate_basic(order)?;
        self.config.validate_against_config(order, config)?;
        self.book.validate_against_book(order, book)?;
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

        assert!(OrderValidator::validate(&order, &config, &book).is_ok());
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

        let result = OrderValidator::validate(&order, &config, &book);
        assert!(result.is_err());
    }

    #[test]
    fn test_focused_validators() {
        let config = create_config();
        let book = OrderBook::new(config.symbol.clone());

        let order = Order::new_limit(
            config.symbol.clone(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100.00)),
            TimeInForce::Gtc,
        );

        // Test individual validators
        assert!(StandardBasicValidator.validate_basic(&order).is_ok());
        assert!(
            StandardConfigValidator
                .validate_against_config(&order, &config)
                .is_ok()
        );
        assert!(
            StandardBookValidator
                .validate_against_book(&order, &book)
                .is_ok()
        );
    }
}
