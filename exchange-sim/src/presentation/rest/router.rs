use axum::{
    Router,
    routing::{delete, get, post, put},
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use super::{admin_handlers, handlers};
use crate::domain::Clock;
use crate::infrastructure::{
    BroadcastEventPublisher, InMemoryAccountRepository, InMemoryInstrumentRepository,
    InMemoryOrderBookRepository, TokenBucketRateLimiter,
};

/// Application state shared across handlers - uses concrete infrastructure types
pub struct AppState<C: Clock> {
    pub clock: Arc<C>,
    pub account_repo: Arc<InMemoryAccountRepository>,
    pub order_book_repo: Arc<InMemoryOrderBookRepository>,
    pub instrument_repo: Arc<InMemoryInstrumentRepository>,
    pub event_publisher: Arc<BroadcastEventPublisher>,
    pub rate_limiter: Arc<TokenBucketRateLimiter>,
}

impl<C: Clock> AppState<C> {
    pub fn new(
        clock: Arc<C>,
        account_repo: Arc<InMemoryAccountRepository>,
        order_book_repo: Arc<InMemoryOrderBookRepository>,
        instrument_repo: Arc<InMemoryInstrumentRepository>,
        event_publisher: Arc<BroadcastEventPublisher>,
        rate_limiter: Arc<TokenBucketRateLimiter>,
    ) -> Self {
        AppState {
            clock,
            account_repo,
            order_book_repo,
            instrument_repo,
            event_publisher,
            rate_limiter,
        }
    }
}

/// Create the REST API router
pub fn create_router<C: Clock + 'static>(state: Arc<AppState<C>>) -> Router {
    Router::new()
        // Public endpoints (Binance API compatible)
        .route("/api/v3/ping", get(handlers::ping))
        .route("/api/v3/time", get(handlers::server_time::<C>))
        .route("/api/v3/exchangeInfo", get(handlers::exchange_info::<C>))
        .route("/api/v3/depth", get(handlers::depth::<C>))
        // Trading endpoints
        .route("/api/v3/order", post(handlers::create_order::<C>))
        .route("/api/v3/order", delete(handlers::cancel_order::<C>))
        // Admin/Bootstrap endpoints (for testing)
        .route("/admin/accounts", post(admin_handlers::create_account::<C>))
        .route(
            "/admin/accounts/{owner_id}",
            get(admin_handlers::get_account::<C>),
        )
        .route(
            "/admin/accounts/{owner_id}/deposit",
            post(admin_handlers::deposit::<C>),
        )
        .route(
            "/admin/accounts/{owner_id}/fee-tier",
            put(admin_handlers::set_fee_tier::<C>),
        )
        .route("/admin/markets", post(admin_handlers::create_market::<C>))
        .route("/admin/markets", get(admin_handlers::list_markets::<C>))
        .route(
            "/admin/markets/{symbol}",
            get(admin_handlers::get_market::<C>),
        )
        // Middleware
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
