use crate::application::ports::{
    AccountRepository, EventPublisher, InstrumentRepository, OrderBookReader, OrderBookWriter,
    OrderRateLimiter,
};
use crate::domain::{
    AccountError, Clock, ExchangeEvent, Order, OrderAcceptedEvent, OrderFilledEvent, OrderStatus,
    OrderType, OrderValidator, PositionSide, Price, Quantity, Side, Symbol, TimeInForce,
    TradeExecutedEvent,
};
use rust_decimal::Decimal;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SubmitOrderCommand {
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Quantity,
    pub price: Option<Price>,
    pub stop_price: Option<Price>,
    pub time_in_force: TimeInForce,
    pub client_order_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubmitOrderResult {
    pub order: Order,
    pub fills: Vec<FillInfo>,
}

#[derive(Debug, Clone)]
pub struct FillInfo {
    pub price: Price,
    pub quantity: Quantity,
    pub commission: Quantity,
}

pub struct SubmitOrderUseCase<C, A, OB, I, E, R>
where
    C: Clock,
    A: AccountRepository,
    OB: OrderBookReader + OrderBookWriter,
    I: InstrumentRepository,
    E: EventPublisher,
    R: OrderRateLimiter,
{
    clock: Arc<C>,
    account_repo: Arc<A>,
    order_book_repo: Arc<OB>,
    instrument_repo: Arc<I>,
    event_publisher: Arc<E>,
    rate_limiter: Arc<R>,
    /// Whether to enforce balance checks (can disable for testing)
    enforce_balances: bool,
}

impl<C, A, OB, I, E, R> SubmitOrderUseCase<C, A, OB, I, E, R>
where
    C: Clock,
    A: AccountRepository,
    OB: OrderBookReader + OrderBookWriter,
    I: InstrumentRepository,
    E: EventPublisher,
    R: OrderRateLimiter,
{
    pub fn new(
        clock: Arc<C>,
        account_repo: Arc<A>,
        order_book_repo: Arc<OB>,
        instrument_repo: Arc<I>,
        event_publisher: Arc<E>,
        rate_limiter: Arc<R>,
    ) -> Self {
        Self {
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
            enforce_balances: true,
        }
    }

    /// Create without balance enforcement (for backward compatibility)
    pub fn without_balance_checks(
        clock: Arc<C>,
        account_repo: Arc<A>,
        order_book_repo: Arc<OB>,
        instrument_repo: Arc<I>,
        event_publisher: Arc<E>,
        rate_limiter: Arc<R>,
    ) -> Self {
        Self {
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
            enforce_balances: false,
        }
    }

    pub async fn execute(
        &self,
        client_id: &str,
        command: SubmitOrderCommand,
    ) -> Result<SubmitOrderResult, OrderError> {
        // Check rate limit
        let rate_result = self.rate_limiter.check_order(client_id).await;
        if !rate_result.allowed {
            return Err(OrderError::RateLimited {
                retry_after_ms: rate_result.retry_after.map(|d| d.as_millis() as u64),
            });
        }

        // Parse and validate symbol
        let symbol =
            Symbol::new(&command.symbol).map_err(|e| OrderError::InvalidSymbol(e.to_string()))?;

        // Get instrument
        let instrument = self
            .instrument_repo
            .get(&symbol)
            .await
            .ok_or_else(|| OrderError::SymbolNotFound(command.symbol.clone()))?;

        let base_asset = instrument.base_asset.clone();
        let quote_asset = instrument.quote_asset.clone();

        // Create order
        let now = self.clock.now();
        let mut order = match command.order_type {
            OrderType::Market => Order::new_market(symbol.clone(), command.side, command.quantity),
            OrderType::Limit | OrderType::LimitMaker => {
                let price = command.price.ok_or(OrderError::MissingPrice)?;
                Order::new_limit(
                    symbol.clone(),
                    command.side,
                    command.quantity,
                    price,
                    command.time_in_force,
                )
            }
            _ => {
                let price = command.price;
                let mut order = Order::new_limit(
                    symbol.clone(),
                    command.side,
                    command.quantity,
                    price.unwrap_or(Price::ZERO),
                    command.time_in_force,
                );
                order.order_type = command.order_type;
                order.stop_price = command.stop_price;
                order
            }
        };

        if let Some(client_order_id) = command.client_order_id {
            order = order.with_client_order_id(client_order_id);
        }

        // Get order book
        let mut book = self.order_book_repo.get_or_create(&symbol).await;

        // Validate order
        OrderValidator::validate(&order, &instrument, &book)
            .map_err(|e| OrderError::ValidationFailed(e.message))?;

        // Get account and check/lock balances if enforcement is enabled
        let mut account = self.account_repo.get_or_create(client_id).await;

        if self.enforce_balances {
            let order_price = order.price.unwrap_or_else(|| {
                // For market orders, use best available price as estimate
                match order.side {
                    Side::Buy => book.best_ask().unwrap_or(Price::ZERO),
                    Side::Sell => book.best_bid().unwrap_or(Price::ZERO),
                }
            });

            match command.side {
                Side::Buy => {
                    // Need quote currency (e.g., USDT) to buy
                    let required = order.quantity.inner() * order_price.inner();
                    account
                        .lock(&quote_asset, required)
                        .map_err(OrderError::AccountError)?;
                }
                Side::Sell => {
                    // Need base currency (e.g., BTC) to sell
                    // Check if user has the asset or has borrowed it
                    let balance = account.balance(&base_asset);
                    if balance.available < order.quantity.inner() {
                        // Check if they have borrowed (short selling)
                        if !account.has_borrowed(&base_asset) {
                            return Err(OrderError::AccountError(
                                AccountError::InsufficientBalance,
                            ));
                        }
                    }
                    account
                        .lock(&base_asset, order.quantity.inner())
                        .map_err(OrderError::AccountError)?;
                }
            }
        }

        // Match order
        let mut fills = Vec::new();
        let (trades, remaining) = book.match_order(order.clone(), now);

        // Calculate effective fee rates for this account
        let (effective_maker_rate, effective_taker_rate) =
            account.effective_fees(instrument.maker_fee_rate, instrument.taker_fee_rate);

        // Process trades and update account
        for trade in &trades {
            let trade_value = trade.quantity.inner() * trade.price.inner();

            // The aggressor (our order) is always the taker
            // Fee is calculated as notional * rate
            let taker_fee = trade_value * effective_taker_rate;
            let is_taker_rebate = effective_taker_rate < Decimal::ZERO;

            fills.push(FillInfo {
                price: trade.price,
                quantity: trade.quantity,
                commission: Quantity::from(taker_fee.abs()),
            });

            if self.enforce_balances {
                match command.side {
                    Side::Buy => {
                        // Bought base asset, spent quote asset
                        account.unlock(&quote_asset, trade_value);
                        account.withdraw(&quote_asset, trade_value).ok();
                        account.deposit(&base_asset, trade.quantity.inner());

                        // Apply taker fee (deduct from quote asset, or credit for rebate)
                        if is_taker_rebate {
                            account.deposit(&quote_asset, taker_fee.abs());
                        } else {
                            account.withdraw(&quote_asset, taker_fee).ok();
                        }

                        // Open/increase long position
                        account.open_position(
                            symbol.clone(),
                            PositionSide::Long,
                            trade.quantity,
                            trade.price,
                            Decimal::ZERO, // Spot has no margin
                            now,
                        );
                    }
                    Side::Sell => {
                        // Sold base asset, received quote asset
                        account.unlock(&base_asset, trade.quantity.inner());
                        account.withdraw(&base_asset, trade.quantity.inner()).ok();
                        account.deposit(&quote_asset, trade_value);

                        // Apply taker fee (deduct from quote asset received, or credit for rebate)
                        if is_taker_rebate {
                            account.deposit(&quote_asset, taker_fee.abs());
                        } else {
                            account.withdraw(&quote_asset, taker_fee).ok();
                        }

                        // If we have a long position, close it; otherwise track short
                        if let Some(pos) = account.position(&symbol) {
                            if pos.side == PositionSide::Long {
                                account
                                    .close_position(&symbol, trade.quantity, trade.price, now)
                                    .ok();
                            } else {
                                // Increase short position
                                account.open_position(
                                    symbol.clone(),
                                    PositionSide::Short,
                                    trade.quantity,
                                    trade.price,
                                    Decimal::ZERO,
                                    now,
                                );
                            }
                        } else {
                            // New short position (if borrowed)
                            if account.has_borrowed(&base_asset) {
                                account.open_position(
                                    symbol.clone(),
                                    PositionSide::Short,
                                    trade.quantity,
                                    trade.price,
                                    Decimal::ZERO,
                                    now,
                                );
                            }
                        }
                    }
                }
            }

            // Create trade with fee information
            let trade_with_fees = trade.clone().with_fees(
                trade_value * effective_maker_rate, // maker fee (for the resting order)
                taker_fee,                          // taker fee (for our order)
                &quote_asset,
            );

            // Publish trade event
            self.event_publisher
                .publish_to_symbol(
                    symbol.as_str(),
                    ExchangeEvent::TradeExecuted(TradeExecutedEvent::from(&trade_with_fees)),
                )
                .await;
        }

        // Update order status based on matching result
        let final_order = if let Some(remaining_order) = remaining {
            // Order has remaining quantity
            if remaining_order.time_in_force.requires_immediate_execution() {
                // IOC/FOK - cancel remaining, unlock funds
                if self.enforce_balances {
                    let remaining_qty = remaining_order.remaining_quantity().inner();
                    match command.side {
                        Side::Buy => {
                            let order_price = remaining_order.price.unwrap_or(Price::ZERO);
                            let remaining_value = remaining_qty * order_price.inner();
                            account.unlock(&quote_asset, remaining_value);
                        }
                        Side::Sell => {
                            account.unlock(&base_asset, remaining_qty);
                        }
                    }
                }
                let mut cancelled = remaining_order;
                cancelled.cancel(now);
                cancelled
            } else {
                // Add to book
                book.add_order(remaining_order.clone());

                // Publish accepted event
                self.event_publisher
                    .publish_to_symbol(
                        symbol.as_str(),
                        ExchangeEvent::OrderAccepted(OrderAcceptedEvent::from(&remaining_order)),
                    )
                    .await;

                remaining_order
            }
        } else {
            // Order fully filled
            let mut filled = order.clone();
            filled.status = OrderStatus::Filled;
            filled.filled_quantity = filled.quantity;
            filled
        };

        // Save book and account
        self.order_book_repo.save(book).await;
        if self.enforce_balances {
            self.account_repo.save(account).await;
        }

        // Publish fill events
        if !trades.is_empty() {
            let fill_event = OrderFilledEvent {
                order_id: final_order.id,
                client_order_id: final_order.client_order_id.clone(),
                symbol: final_order.symbol.clone(),
                side: final_order.side,
                status: final_order.status,
                price: trades.last().unwrap().price,
                quantity: trades
                    .iter()
                    .map(|t| t.quantity)
                    .fold(Quantity::ZERO, |a, b| a + b),
                cumulative_quantity: final_order.filled_quantity,
                timestamp: now,
            };

            self.event_publisher
                .publish_to_symbol(symbol.as_str(), ExchangeEvent::OrderFilled(fill_event))
                .await;
        }

        Ok(SubmitOrderResult {
            order: final_order,
            fills,
        })
    }
}

#[derive(Debug, Clone)]
pub enum OrderError {
    RateLimited { retry_after_ms: Option<u64> },
    InvalidSymbol(String),
    SymbolNotFound(String),
    MissingPrice,
    MissingStopPrice,
    ValidationFailed(String),
    AccountError(AccountError),
    InternalError(String),
}

impl std::fmt::Display for OrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderError::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, ", retry after {}ms", ms)?;
                }
                Ok(())
            }
            OrderError::InvalidSymbol(s) => write!(f, "Invalid symbol: {}", s),
            OrderError::SymbolNotFound(s) => write!(f, "Symbol not found: {}", s),
            OrderError::MissingPrice => write!(f, "Price is required for this order type"),
            OrderError::MissingStopPrice => write!(f, "Stop price is required for this order type"),
            OrderError::ValidationFailed(s) => write!(f, "Validation failed: {}", s),
            OrderError::AccountError(e) => write!(f, "Account error: {}", e),
            OrderError::InternalError(s) => write!(f, "Internal error: {}", s),
        }
    }
}

impl std::error::Error for OrderError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::RateLimitConfig;
    use crate::domain::Order;
    use crate::infrastructure::{
        BroadcastEventPublisher, InMemoryAccountRepository, InMemoryInstrumentRepository,
        InMemoryOrderBookRepository, SimulationClock, TokenBucketRateLimiter,
    };
    use rust_decimal_macros::dec;

    async fn setup_test_env() -> (
        Arc<SimulationClock>,
        Arc<InMemoryAccountRepository>,
        Arc<InMemoryOrderBookRepository>,
        Arc<InMemoryInstrumentRepository>,
        Arc<BroadcastEventPublisher>,
        Arc<TokenBucketRateLimiter>,
    ) {
        let clock = Arc::new(SimulationClock::new());
        let account_repo = Arc::new(InMemoryAccountRepository::new());
        let order_book_repo = Arc::new(InMemoryOrderBookRepository::new());
        let instrument_repo = Arc::new(InMemoryInstrumentRepository::with_defaults());
        let event_publisher = Arc::new(BroadcastEventPublisher::new(1000));
        let rate_limiter = Arc::new(TokenBucketRateLimiter::new(RateLimitConfig::default()));

        (
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        )
    }

    #[tokio::test]
    async fn test_buy_order_requires_balance() {
        let (clock, account_repo, order_book_repo, instrument_repo, event_publisher, rate_limiter) =
            setup_test_env().await;

        let use_case = SubmitOrderUseCase::new(
            clock,
            Arc::clone(&account_repo),
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        );

        // Try to buy without any balance - should fail
        let command = SubmitOrderCommand {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            quantity: Quantity::from(dec!(1)),
            price: Some(Price::from(dec!(50000))),
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            client_order_id: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
        match result {
            Err(OrderError::AccountError(AccountError::InsufficientBalance)) => {}
            other => panic!("Expected InsufficientBalance, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_buy_order_with_sufficient_balance() {
        let (clock, account_repo, order_book_repo, instrument_repo, event_publisher, rate_limiter) =
            setup_test_env().await;

        // Deposit USDT for buying
        {
            let mut account = account_repo.get_or_create("trader1").await;
            account.deposit("USDT", dec!(100000));
            account_repo.save(account).await;
        }

        // Add a sell order to the book
        {
            let symbol = Symbol::new("BTCUSDT").unwrap();
            let mut book = order_book_repo.get_or_create(&symbol).await;
            let sell_order = Order::new_limit(
                symbol,
                Side::Sell,
                Quantity::from(dec!(10)),
                Price::from(dec!(50000)),
                TimeInForce::Gtc,
            );
            book.add_order(sell_order);
            order_book_repo.save(book).await;
        }

        let use_case = SubmitOrderUseCase::new(
            Arc::clone(&clock),
            Arc::clone(&account_repo),
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        );

        // Buy 1 BTC at 50000 - should succeed
        let command = SubmitOrderCommand {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            quantity: Quantity::from(dec!(1)),
            price: Some(Price::from(dec!(50000))),
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            client_order_id: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok());

        // Check the account was updated
        let account = account_repo.get_by_owner("trader1").await.unwrap();
        // Should have BTC now
        assert_eq!(account.balance("BTC").available, dec!(1));
        // Should have spent USDT (including taker fee: 50000 * 0.0002 = 10)
        assert_eq!(account.balance("USDT").available, dec!(49990)); // 100000 - 50000 - 10 fee
    }

    #[tokio::test]
    async fn test_sell_without_asset_fails() {
        let (clock, account_repo, order_book_repo, instrument_repo, event_publisher, rate_limiter) =
            setup_test_env().await;

        let use_case = SubmitOrderUseCase::new(
            clock,
            Arc::clone(&account_repo),
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        );

        // Try to sell BTC without having any - should fail
        let command = SubmitOrderCommand {
            symbol: "BTCUSDT".to_string(),
            side: Side::Sell,
            order_type: OrderType::Limit,
            quantity: Quantity::from(dec!(1)),
            price: Some(Price::from(dec!(50000))),
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            client_order_id: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_short_sell_with_borrowed_asset() {
        let (clock, account_repo, order_book_repo, instrument_repo, event_publisher, rate_limiter) =
            setup_test_env().await;
        let now = clock.now();

        // Setup: deposit collateral and borrow BTC
        {
            let mut account = account_repo.get_or_create("trader1").await;
            // Deposit USDT as collateral
            account.deposit("USDT", dec!(100000));
            // Borrow 1 BTC using USDT as collateral
            account
                .borrow(
                    "BTC",
                    dec!(1),     // borrow 1 BTC
                    dec!(0.05),  // 5% annual interest
                    "USDT",      // collateral asset
                    dec!(60000), // collateral amount (120% of position)
                    now,
                )
                .unwrap();
            account_repo.save(account).await;
        }

        // Add a buy order to the book (someone willing to buy)
        {
            let symbol = Symbol::new("BTCUSDT").unwrap();
            let mut book = order_book_repo.get_or_create(&symbol).await;
            let buy_order = Order::new_limit(
                symbol,
                Side::Buy,
                Quantity::from(dec!(10)),
                Price::from(dec!(50000)),
                TimeInForce::Gtc,
            );
            book.add_order(buy_order);
            order_book_repo.save(book).await;
        }

        let use_case = SubmitOrderUseCase::new(
            Arc::clone(&clock),
            Arc::clone(&account_repo),
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        );

        // Sell 1 BTC (short sell using borrowed asset)
        let command = SubmitOrderCommand {
            symbol: "BTCUSDT".to_string(),
            side: Side::Sell,
            order_type: OrderType::Limit,
            quantity: Quantity::from(dec!(1)),
            price: Some(Price::from(dec!(50000))),
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            client_order_id: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(
            result.is_ok(),
            "Short sell should succeed with borrowed asset: {:?}",
            result
        );

        // Check the account state
        let account = account_repo.get_by_owner("trader1").await.unwrap();

        // BTC was sold (borrowed, then sold)
        assert_eq!(account.balance("BTC").available, dec!(0));
        assert_eq!(account.balance("BTC").borrowed, dec!(1)); // Still owe 1 BTC

        // Received USDT from sale
        // Started with 100000, locked 60000 as collateral, received 50000 from sale, minus taker fee
        // Available = 100000 - 60000 + 50000 - 10 fee = 89990
        assert_eq!(account.balance("USDT").available, dec!(89990));

        // Should have a short position tracked
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let position = account.position(&symbol);
        assert!(position.is_some(), "Should have a short position");
        let pos = position.unwrap();
        assert_eq!(pos.side, PositionSide::Short);
        assert_eq!(pos.quantity, Quantity::from(dec!(1)));
    }

    #[tokio::test]
    async fn test_without_balance_enforcement() {
        let (clock, account_repo, order_book_repo, instrument_repo, event_publisher, rate_limiter) =
            setup_test_env().await;

        // Add a sell order to the book
        {
            let symbol = Symbol::new("BTCUSDT").unwrap();
            let mut book = order_book_repo.get_or_create(&symbol).await;
            let sell_order = Order::new_limit(
                symbol,
                Side::Sell,
                Quantity::from(dec!(10)),
                Price::from(dec!(50000)),
                TimeInForce::Gtc,
            );
            book.add_order(sell_order);
            order_book_repo.save(book).await;
        }

        // Use without_balance_checks for backward compatibility
        let use_case = SubmitOrderUseCase::without_balance_checks(
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        );

        // Buy without having any balance - should succeed because enforcement is off
        let command = SubmitOrderCommand {
            symbol: "BTCUSDT".to_string(),
            side: Side::Buy,
            order_type: OrderType::Limit,
            quantity: Quantity::from(dec!(1)),
            price: Some(Price::from(dec!(50000))),
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            client_order_id: None,
        };

        let result = use_case.execute("trader1", command).await;
        assert!(result.is_ok(), "Should succeed without balance enforcement");
    }
}
