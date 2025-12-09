//! Exchange Simulator
//!
//! A Binance-compatible exchange simulator for testing trading infrastructure.
//!
//! # Architecture
//!
//! This crate follows Clean Architecture with clear separation of concerns:
//!
//! - **Domain**: Core business entities and rules (OrderBook, Order, Trade, etc.)
//! - **Application**: Use cases and port interfaces (SubmitOrder, CancelOrder, etc.)
//! - **Infrastructure**: Implementations of ports (InMemoryRepository, SimulationClock, etc.)
//! - **Presentation**: REST API and WebSocket handlers
//!
//! # Features
//!
//! - Full order book with price-time priority matching
//! - Binance-compatible REST API (`/api/v3/...`)
//! - WebSocket streams for depth updates and trades
//! - Rate limiting (simulating Binance limits)
//! - Universal clock for time synchronization
//! - Multiple instrument support
//!
//! # Example
//!
//! ```ignore
//! use exchange_sim::{Exchange, ExchangeConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ExchangeConfig::default();
//!     let exchange = Exchange::new(config);
//!     exchange.run("0.0.0.0:8080").await;
//! }
//! ```

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

// Re-export commonly used types
pub use domain::{
    // DEX / AMM types
    AddLiquidityResult,
    AmmType,
    // Blockchain simulator types
    BlockchainError,
    BlockchainSimulator,
    BlockchainTx,
    // Clearing method for crypto vs equities
    ClearingMethod,
    Clock,
    ControllableClock,
    // Custodian and withdrawal types
    Custodian,
    CustodianId,
    CustodianType,
    DepositAddress,
    ExchangeEvent,
    FeeSchedule,
    Instrument,
    InstrumentStatus,
    LiquidityPool,
    LpPosition,
    MarginCalculator,
    Network,
    NetworkConfig,
    Order,
    OrderBook,
    OrderId,
    OrderStatus,
    OrderType,
    PoolError,
    PoolId,
    Price,
    Quantity,
    RemoveLiquidityResult,
    SettlementCycle,
    Side,
    StandardMarginCalculator,
    SwapOutput,
    SwapResult,
    Symbol,
    TimeInForce,
    TimeScale,
    Timestamp,
    Trade,
    TradeId,
    TradingPairConfig,
    TxId,
    TxStatus,
    WithdrawalConfig,
    WithdrawalError,
    WithdrawalId,
    WithdrawalRequest,
    WithdrawalStatus,
};

pub use infrastructure::{
    BlockchainAdapter, BlockchainAdapterError, BroadcastEventPublisher, InMemoryAccountRepository,
    InMemoryCustodianRepository, InMemoryDepositAddressRegistry, InMemoryInstrumentRepository,
    InMemoryOrderBookRepository, InMemoryPoolRepository, InMemoryProcessedDepositTracker,
    InMemoryWithdrawalRepository, SimulationClock, TokenBucketRateLimiter,
};

pub use application::{
    // Withdrawal use cases
    AddConfirmationCommand,
    // DEX use cases
    AddLiquidityCommand,
    AddLiquidityExecutionResult,
    // Order management
    CancelOrderCommand,
    CancelOrderResult,
    ConfirmWithdrawalCommand,
    // Deposit use cases
    Deposit,
    DepositId,
    DepositStatus,
    DepthResult,
    FailWithdrawalCommand,
    GetDepthQuery,
    LiquidityUseCase,
    LiquidityUseCaseError,
    ProcessDepositError,
    ProcessDepositUseCase,
    ProcessDepositsResult,
    ProcessWithdrawalCommand,
    ProcessWithdrawalError,
    ProcessWithdrawalResult,
    ProcessWithdrawalUseCase,
    RateLimitConfig,
    RegisterDepositAddressCommand,
    RemoveLiquidityCommand,
    RemoveLiquidityExecutionResult,
    RequestWithdrawalCommand,
    RequestWithdrawalResult,
    RequestWithdrawalUseCase,
    SubmitOrderCommand,
    SubmitOrderResult,
    SwapCommand,
    SwapExecutionResult,
    SwapQuote,
    SwapUseCase,
    SwapUseCaseError,
    WithdrawalUseCaseError,
};

// Re-export port traits for integration tests
pub use application::ports::{
    AccountRepository,
    // Custodian and withdrawal ports
    CustodianReader,
    CustodianWriter,
    // Event publishing
    EventPublisher,
    // DEX ports
    LpPositionReader,
    LpPositionWriter,
    MarketDataReader,
    OrderBookReader,
    OrderBookRepository,
    OrderBookWriter,
    OrderLookup,
    PoolReader,
    PoolWriter,
    WithdrawalReader,
    WithdrawalWriter,
};

pub use presentation::{AppState, StreamManager, WsState, create_router};

use axum::Router;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Exchange configuration
#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    /// REST API host
    pub host: String,
    /// REST API port
    pub rest_port: u16,
    /// WebSocket port (can be same as REST)
    pub ws_port: u16,
    /// Rate limit configuration
    pub rate_limits: RateLimitConfig,
    /// Event channel capacity
    pub event_capacity: usize,
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        ExchangeConfig {
            host: "0.0.0.0".to_string(),
            rest_port: 8080,
            ws_port: 8080,
            rate_limits: RateLimitConfig::default(),
            event_capacity: 10000,
        }
    }
}

/// The main exchange server
pub struct Exchange<C: Clock + 'static> {
    pub config: ExchangeConfig,
    pub clock: Arc<C>,
    pub account_repo: Arc<InMemoryAccountRepository>,
    pub order_book_repo: Arc<InMemoryOrderBookRepository>,
    pub instrument_repo: Arc<InMemoryInstrumentRepository>,
    pub event_publisher: Arc<BroadcastEventPublisher>,
    pub rate_limiter: Arc<TokenBucketRateLimiter>,
}

impl<C: Clock + 'static> Exchange<C> {
    /// Create a new exchange with the given clock
    pub fn with_clock(config: ExchangeConfig, clock: Arc<C>) -> Self {
        let rate_limiter = Arc::new(TokenBucketRateLimiter::new(config.rate_limits.clone()));
        let event_publisher = Arc::new(BroadcastEventPublisher::new(config.event_capacity));
        let account_repo = Arc::new(InMemoryAccountRepository::new());
        let order_book_repo = Arc::new(InMemoryOrderBookRepository::new());
        let instrument_repo = Arc::new(InMemoryInstrumentRepository::with_defaults());

        Exchange {
            config,
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        }
    }

    /// Create the REST API router
    pub fn rest_router(&self) -> Router {
        let state = Arc::new(AppState::new(
            Arc::clone(&self.clock),
            Arc::clone(&self.account_repo),
            Arc::clone(&self.order_book_repo),
            Arc::clone(&self.instrument_repo),
            Arc::clone(&self.event_publisher),
            Arc::clone(&self.rate_limiter),
        ));

        create_router(state)
    }

    /// Create WebSocket state
    pub fn ws_state(&self) -> Arc<WsState<C>> {
        Arc::new(WsState {
            clock: Arc::clone(&self.clock),
            stream_manager: Arc::new(StreamManager::new(Arc::clone(&self.event_publisher))),
            rate_limiter: Arc::clone(&self.rate_limiter),
        })
    }

    /// Run the exchange server
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", self.config.host, self.config.rest_port);

        // Create combined router with REST and WebSocket
        let ws_state = self.ws_state();
        let router = self.rest_router().route(
            "/ws",
            axum::routing::get({
                let ws_state = Arc::clone(&ws_state);
                move |ws| presentation::ws_handler(ws, axum::extract::State(ws_state))
            }),
        );

        tracing::info!("Exchange simulator listening on {}", addr);

        let listener = TcpListener::bind(&addr).await?;
        axum::serve(listener, router).await?;

        Ok(())
    }

    /// Add a trading pair configuration to the exchange
    pub async fn add_trading_pair(&self, config: TradingPairConfig) {
        self.instrument_repo.add(config);
    }

    /// Get the event publisher for subscribing to events
    pub fn event_publisher(&self) -> &Arc<BroadcastEventPublisher> {
        &self.event_publisher
    }
}

impl Exchange<SimulationClock> {
    /// Create a new exchange with default simulation clock
    pub fn new(config: ExchangeConfig) -> Self {
        let clock = Arc::new(SimulationClock::new());
        Self::with_clock(config, clock)
    }

    /// Create a new exchange with fixed time (for testing)
    pub fn fixed_time(config: ExchangeConfig) -> Self {
        let clock = Arc::new(SimulationClock::fixed());
        Self::with_clock(config, clock)
    }

    /// Create exchange from SimulatorConfig (JSON config)
    pub async fn from_config(
        sim_config: infrastructure::SimulatorConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Convert to ExchangeConfig
        let exchange_config = ExchangeConfig {
            host: sim_config.server.host,
            rest_port: sim_config.server.port,
            ws_port: sim_config.server.port,
            rate_limits: RateLimitConfig {
                requests_per_minute: sim_config.rate_limits.requests_per_minute,
                orders_per_second: sim_config.rate_limits.orders_per_second,
                orders_per_day: sim_config.rate_limits.orders_per_day,
                request_weight_per_minute: sim_config.rate_limits.request_weight_per_minute,
                ws_connections_per_ip: sim_config.rate_limits.ws_connections_per_ip,
                ws_messages_per_second: sim_config.rate_limits.ws_messages_per_second,
            },
            event_capacity: sim_config.server.event_capacity,
        };

        // Create exchange with empty instrument repo
        let clock = Arc::new(SimulationClock::new());
        let rate_limiter = Arc::new(TokenBucketRateLimiter::new(
            exchange_config.rate_limits.clone(),
        ));
        let event_publisher =
            Arc::new(BroadcastEventPublisher::new(exchange_config.event_capacity));
        let account_repo = Arc::new(InMemoryAccountRepository::new());
        let order_book_repo = Arc::new(InMemoryOrderBookRepository::new());
        let instrument_repo = Arc::new(InMemoryInstrumentRepository::new()); // Empty, not with_defaults

        let exchange = Exchange {
            config: exchange_config,
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        };

        // Add configured markets
        for market in &sim_config.markets {
            let trading_pair = market.to_trading_pair_config()?;
            tracing::info!(
                "Adding market: {} ({:?})",
                trading_pair.symbol,
                trading_pair.instrument_type
            );
            exchange.instrument_repo.add(trading_pair);
        }

        // Create configured accounts
        for account_config in &sim_config.accounts {
            let mut account = exchange
                .account_repo
                .get_or_create(&account_config.owner_id)
                .await;
            for deposit in &account_config.deposits {
                account.deposit(&deposit.asset, deposit.amount);
            }
            if let Some(tier) = account_config.fee_tier {
                account.fee_schedule = FeeSchedule::from_tier(tier);
            }
            exchange.account_repo.save(account).await;
            tracing::info!("Created account: {}", account_config.owner_id);
        }

        // Create order books and seed orders
        for seed_order in &sim_config.seed_orders {
            let symbol = Symbol::new(&seed_order.symbol)?;

            // Ensure order book exists
            let mut book = exchange.order_book_repo.get_or_create(&symbol).await;

            // Create the seed order
            let order = Order::new_limit(
                symbol.clone(),
                seed_order.side,
                Quantity::from(seed_order.quantity),
                Price::from(seed_order.price),
                seed_order.time_in_force.unwrap_or(TimeInForce::Gtc),
            );

            book.add_order(order);
            exchange.order_book_repo.save(book).await;
            tracing::info!(
                "Added seed order: {} {} {} @ {}",
                seed_order.side,
                seed_order.quantity,
                seed_order.symbol,
                seed_order.price
            );
        }

        Ok(exchange)
    }
}
