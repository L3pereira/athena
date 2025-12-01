use athena_core::{
    entities::{MarginAccount, Order, Position, PositionSide, Side, Trade},
    instruments::InstrumentId,
};
use athena_ports::{
    LiquidationOrder, RiskCheckResult, RiskConfig, RiskError, RiskManager, RiskResult,
};
use log::{debug, info, warn};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Basic risk manager implementation
pub struct BasicRiskManager {
    config: RiskConfig,
}

impl BasicRiskManager {
    /// Create a new basic risk manager with default config
    pub fn new() -> Self {
        Self {
            config: RiskConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: RiskConfig) -> Self {
        Self { config }
    }

    /// Create with specific leverage
    pub fn with_leverage(leverage: u32) -> Self {
        Self {
            config: RiskConfig::with_leverage(leverage),
        }
    }

    /// Get the risk configuration
    pub fn config(&self) -> &RiskConfig {
        &self.config
    }

    /// Convert order side to position side
    fn order_side_to_position_side(side: Side) -> PositionSide {
        match side {
            Side::Buy => PositionSide::Long,
            Side::Sell => PositionSide::Short,
        }
    }
}

impl Default for BasicRiskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RiskManager for BasicRiskManager {
    fn validate_order(
        &self,
        account: &MarginAccount,
        order: &Order,
    ) -> RiskResult<RiskCheckResult> {
        // Check account status
        match account.status {
            athena_core::entities::AccountStatus::Frozen => {
                return Err(RiskError::AccountFrozen(account.id));
            }
            athena_core::entities::AccountStatus::Liquidating => {
                return Ok(RiskCheckResult::rejected(
                    "Account is being liquidated".to_string(),
                ));
            }
            _ => {}
        }

        // Get price for margin calculation
        // For market orders, use a conservative estimate (ZERO triggers rejection below)
        // In production, you'd use current market price
        let price = order.price.unwrap_or(Decimal::ZERO);

        if price == Decimal::ZERO {
            return Ok(RiskCheckResult::rejected(
                "Cannot validate order without price".to_string(),
            ));
        }

        // Get current position for this instrument
        let current_position = account.positions.get(&order.instrument_id);

        // Calculate required margin
        let required_margin = self.calculate_required_margin(order, current_position);

        // Check if sufficient margin is available
        if required_margin > account.available_balance {
            return Ok(RiskCheckResult::rejected(format!(
                "Insufficient margin: required {}, available {}",
                required_margin, account.available_balance
            )));
        }

        // Check max position size if configured
        if let Some(max_size) = self.config.max_position_size {
            let new_size = current_position
                .map(|p| {
                    if Self::order_side_to_position_side(order.side) == p.side {
                        p.quantity + order.quantity
                    } else {
                        (p.quantity - order.quantity).abs()
                    }
                })
                .unwrap_or(order.quantity);

            if new_size > max_size {
                return Ok(RiskCheckResult::rejected(format!(
                    "Position size {} exceeds maximum {}",
                    new_size, max_size
                )));
            }
        }

        // Check max total exposure if configured
        if let Some(max_exposure) = self.config.max_total_exposure {
            let current_exposure: Decimal =
                account.positions.values().map(|p| p.notional_value()).sum();
            let new_exposure = current_exposure + (order.quantity * price);

            if new_exposure > max_exposure {
                return Ok(RiskCheckResult::rejected(format!(
                    "Total exposure {} exceeds maximum {}",
                    new_exposure, max_exposure
                )));
            }
        }

        // Calculate margin state after order
        let available_after = account.available_balance - required_margin;
        let margin_after = account.margin_balance + required_margin;
        let total_margin_required = account.total_maintenance_margin()
            + (order.quantity * price * self.config.maintenance_margin_rate);

        let ratio_after = if total_margin_required > Decimal::ZERO {
            (available_after + margin_after) / total_margin_required
        } else {
            Decimal::MAX
        };

        let mut result = RiskCheckResult::approved(required_margin, available_after, ratio_after);

        // Add warnings if approaching margin limits
        if ratio_after < Decimal::new(150, 2) {
            result = result.with_warning("Low margin ratio after order".to_string());
        }

        debug!(
            "Order validated: instrument={}, side={:?}, qty={}, required_margin={}, ratio_after={}",
            order.symbol(),
            order.side,
            order.quantity,
            required_margin,
            ratio_after
        );

        Ok(result)
    }

    fn calculate_required_margin(
        &self,
        order: &Order,
        current_position: Option<&Position>,
    ) -> Decimal {
        let price = order.price.unwrap_or(Decimal::ZERO);
        let order_side = Self::order_side_to_position_side(order.side);

        match current_position {
            Some(position) => {
                if position.side == order_side {
                    // Adding to position - need margin for new quantity
                    order.quantity * price * self.config.initial_margin_rate
                } else {
                    // Reducing position - no additional margin needed
                    // In fact, margin may be released
                    if order.quantity >= position.quantity {
                        // Closing and potentially reversing
                        let excess = order.quantity - position.quantity;
                        excess * price * self.config.initial_margin_rate
                    } else {
                        Decimal::ZERO
                    }
                }
            }
            None => {
                // New position
                order.quantity * price * self.config.initial_margin_rate
            }
        }
    }

    fn process_trade(&self, account: &mut MarginAccount, trade: &Trade) -> RiskResult<()> {
        info!(
            "Processing trade: account={}, instrument={}, qty={}, price={}",
            account.id,
            trade.symbol(),
            trade.quantity,
            trade.price
        );

        // This is a simplified implementation
        // In a real system, you'd determine the side from the order that initiated the trade
        // and update the account positions accordingly

        Ok(())
    }

    fn update_mark_prices(
        &self,
        account: &mut MarginAccount,
        prices: &HashMap<InstrumentId, Decimal>,
    ) -> Vec<LiquidationOrder> {
        account.update_mark_prices(prices);

        // Check for positions that need liquidation
        self.generate_liquidation_orders(account)
    }

    fn check_liquidation(&self, position: &Position) -> bool {
        position.should_liquidate()
    }

    fn generate_liquidation_orders(&self, account: &MarginAccount) -> Vec<LiquidationOrder> {
        let mut orders = Vec::new();

        for position in account.positions.values() {
            if self.check_liquidation(position) {
                warn!(
                    "Liquidation triggered: account={}, instrument={}, side={:?}, qty={}, mark_price={}, liq_price={}",
                    account.id,
                    position.instrument_id,
                    position.side,
                    position.quantity,
                    position.mark_price,
                    position.liquidation_price
                );

                let close_side = position.side.opposite();

                orders.push(LiquidationOrder {
                    account_id: account.id,
                    position_id: position.id,
                    instrument_id: position.instrument_id.clone(),
                    close_side,
                    quantity: position.quantity,
                    mark_price: position.mark_price,
                    liquidation_price: position.liquidation_price,
                    reason: format!(
                        "Mark price {} {} liquidation price {}",
                        position.mark_price,
                        if position.side == PositionSide::Long {
                            "<="
                        } else {
                            ">="
                        },
                        position.liquidation_price
                    ),
                });
            }
        }

        orders
    }

    fn calculate_leverage(&self, account: &MarginAccount) -> Decimal {
        let total_notional: Decimal = account.positions.values().map(|p| p.notional_value()).sum();

        if account.equity == Decimal::ZERO {
            return Decimal::ZERO;
        }

        total_notional / account.equity
    }

    fn max_position_size(
        &self,
        account: &MarginAccount,
        instrument_id: &InstrumentId,
        side: PositionSide,
        price: Decimal,
    ) -> Decimal {
        if price == Decimal::ZERO {
            return Decimal::ZERO;
        }

        // Calculate maximum based on available margin
        let max_from_margin = account.available_balance / (price * self.config.initial_margin_rate);

        // Apply position size limit if configured
        let max_size = match self.config.max_position_size {
            Some(limit) => {
                // Account for existing position
                let existing = account
                    .positions
                    .get(instrument_id)
                    .map(|p| {
                        if p.side == side {
                            p.quantity
                        } else {
                            Decimal::ZERO
                        }
                    })
                    .unwrap_or(Decimal::ZERO);

                (limit - existing).max(Decimal::ZERO).min(max_from_margin)
            }
            None => max_from_margin,
        };

        max_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use athena_core::entities::{MarginMode, OrderType, TimeInForce};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn create_test_account() -> MarginAccount {
        MarginAccount::new(
            "test_user".to_string(),
            dec!(10000.0),
            MarginMode::Cross,
            dec!(0.10),
            dec!(0.05),
        )
    }

    fn create_test_order(side: Side, quantity: Decimal, price: Decimal) -> Order {
        Order::new(
            "BTC/USD".to_string(),
            side,
            OrderType::Limit,
            quantity,
            Some(price),
            None,
            TimeInForce::GTC,
        )
    }

    #[test]
    fn test_risk_manager_creation() {
        let rm = BasicRiskManager::new();
        assert_eq!(rm.config().initial_margin_rate, dec!(0.10));
        assert_eq!(rm.config().max_leverage, dec!(10));
    }

    #[test]
    fn test_validate_order_approved() {
        let rm = BasicRiskManager::new();
        let account = create_test_account();
        let order = create_test_order(Side::Buy, dec!(1.0), dec!(50000.0));

        let result = rm.validate_order(&account, &order).unwrap();

        assert!(result.approved);
        assert_eq!(result.required_margin, dec!(5000.0)); // 10% of 50000
        assert_eq!(result.available_margin_after, dec!(5000.0)); // 10000 - 5000
    }

    #[test]
    fn test_validate_order_insufficient_margin() {
        let rm = BasicRiskManager::new();
        let account = create_test_account();
        // Try to buy 3 BTC at $50,000 = $150,000 notional, needs $15,000 margin
        let order = create_test_order(Side::Buy, dec!(3.0), dec!(50000.0));

        let result = rm.validate_order(&account, &order).unwrap();

        assert!(!result.approved);
        assert!(result.rejection_reason.is_some());
    }

    #[test]
    fn test_calculate_required_margin_new_position() {
        let rm = BasicRiskManager::new();
        let order = create_test_order(Side::Buy, dec!(1.0), dec!(50000.0));

        let margin = rm.calculate_required_margin(&order, None);

        assert_eq!(margin, dec!(5000.0)); // 10% of 50000
    }

    #[test]
    fn test_calculate_required_margin_adding_to_position() {
        let rm = BasicRiskManager::new();
        let order = create_test_order(Side::Buy, dec!(1.0), dec!(50000.0));

        // Existing long position
        let position = Position::new(
            Uuid::new_v4(),
            InstrumentId::new("BTC/USD"),
            PositionSide::Long,
            dec!(1.0),
            dec!(50000.0),
            dec!(0.10),
            dec!(0.05),
        );

        let margin = rm.calculate_required_margin(&order, Some(&position));

        // Adding to same side - need full margin for new qty
        assert_eq!(margin, dec!(5000.0));
    }

    #[test]
    fn test_calculate_required_margin_reducing_position() {
        let rm = BasicRiskManager::new();
        let order = create_test_order(Side::Sell, dec!(0.5), dec!(50000.0));

        // Existing long position
        let position = Position::new(
            Uuid::new_v4(),
            InstrumentId::new("BTC/USD"),
            PositionSide::Long,
            dec!(1.0),
            dec!(50000.0),
            dec!(0.10),
            dec!(0.05),
        );

        let margin = rm.calculate_required_margin(&order, Some(&position));

        // Reducing position - no additional margin needed
        assert_eq!(margin, Decimal::ZERO);
    }

    #[test]
    fn test_max_position_size() {
        let rm = BasicRiskManager::new();
        let account = create_test_account();

        let max = rm.max_position_size(
            &account,
            &InstrumentId::new("BTC/USD"),
            PositionSide::Long,
            dec!(50000.0),
        );

        // With 10000 available and 10% margin, max = 10000 / (50000 * 0.10) = 2
        assert_eq!(max, dec!(2.0));
    }

    #[test]
    fn test_calculate_leverage() {
        let rm = BasicRiskManager::new();
        let mut account = create_test_account();

        // No positions - zero leverage
        assert_eq!(rm.calculate_leverage(&account), Decimal::ZERO);

        // Open a position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(1.0),
                dec!(50000.0),
            )
            .unwrap();

        // Leverage = notional / equity = 50000 / 10000 = 5
        assert_eq!(rm.calculate_leverage(&account), dec!(5));
    }

    #[test]
    fn test_liquidation_detection() {
        let rm = BasicRiskManager::new();
        let mut account = create_test_account();

        // Open a leveraged position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(2.0), // Max leverage
                dec!(50000.0),
            )
            .unwrap();

        // Update price to trigger liquidation
        let mut prices = HashMap::new();
        prices.insert(InstrumentId::new("BTC/USD"), dec!(45000.0)); // Below liquidation price

        let liquidations = rm.update_mark_prices(&mut account, &prices);

        assert!(!liquidations.is_empty());
        assert_eq!(liquidations[0].close_side, PositionSide::Short); // Close long by selling
    }

    #[test]
    fn test_no_liquidation_when_price_safe() {
        let rm = BasicRiskManager::new();
        let mut account = create_test_account();

        // Open a position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(1.0),
                dec!(50000.0),
            )
            .unwrap();

        // Update price (still safe)
        let mut prices = HashMap::new();
        prices.insert(InstrumentId::new("BTC/USD"), dec!(49000.0));

        let liquidations = rm.update_mark_prices(&mut account, &prices);

        assert!(liquidations.is_empty());
    }
}
