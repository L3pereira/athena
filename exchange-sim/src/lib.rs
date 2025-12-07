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
    Clock, ControllableClock, ExchangeEvent, FeeSchedule, Instrument, InstrumentStatus,
    MarginCalculator, Order, OrderBook, OrderId, OrderStatus, OrderType, Price, Quantity, Side,
    StandardMarginCalculator, Symbol, TimeInForce, TimeScale, Timestamp, Trade, TradeId,
    TradingPairConfig,
};

pub use infrastructure::{
    BroadcastEventPublisher, InMemoryAccountRepository, InMemoryInstrumentRepository,
    InMemoryOrderBookRepository, SimulationClock, TokenBucketRateLimiter,
};

pub use application::{
    CancelOrderCommand, CancelOrderResult, DepthResult, GetDepthQuery, RateLimitConfig,
    SubmitOrderCommand, SubmitOrderResult,
};

// Re-export port traits for integration tests
pub use application::ports::{AccountRepository, OrderBookRepository};

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
}
