//! Strategy Trait and Runtime
//!
//! Defines the interface for trading strategies and provides
//! the runtime to connect them to the gateway.

use crate::events::MarketEvent;
use crate::orderbook::LocalOrderBook;
use async_trait::async_trait;
use athena_gateway::messages::{
    market_data::{OrderBookUpdate, TradeMessage},
    order::{OrderRequest, OrderResponse},
};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Actions a strategy can request
#[derive(Debug, Clone)]
pub enum Action {
    /// Submit a new order
    SubmitOrder(OrderRequest),
    /// Cancel an existing order by client_order_id
    CancelOrder { client_order_id: String },
    /// Cancel all orders for an instrument (or all if None)
    CancelAll { instrument_id: Option<String> },
}

/// Position for a single instrument
#[derive(Debug, Clone, Default)]
pub struct Position {
    /// Net quantity (positive = long, negative = short)
    pub quantity: Decimal,
    /// Average entry price
    pub avg_price: Decimal,
    /// Realized PnL
    pub realized_pnl: Decimal,
}

impl Position {
    pub fn is_long(&self) -> bool {
        self.quantity > Decimal::ZERO
    }

    pub fn is_short(&self) -> bool {
        self.quantity < Decimal::ZERO
    }

    pub fn is_flat(&self) -> bool {
        self.quantity.is_zero()
    }

    /// Calculate unrealized PnL given current price
    pub fn unrealized_pnl(&self, current_price: Decimal) -> Decimal {
        self.quantity * (current_price - self.avg_price)
    }
}

/// Tracks open orders
#[derive(Debug, Clone)]
pub struct OpenOrder {
    pub client_order_id: String,
    pub instrument_id: String,
    pub side: athena_gateway::messages::order::OrderSide,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub filled_qty: Decimal,
}

/// Context provided to strategy on each event
pub struct StrategyContext<'a> {
    /// Order books for all subscribed instruments
    pub books: &'a HashMap<String, LocalOrderBook>,
    /// Current positions
    pub positions: &'a HashMap<String, Position>,
    /// Open orders
    pub open_orders: &'a HashMap<String, OpenOrder>,
}

impl StrategyContext<'_> {
    /// Get order book for an instrument
    pub fn book(&self, instrument_id: &str) -> Option<&LocalOrderBook> {
        self.books.get(instrument_id)
    }

    /// Get position for an instrument (returns default if none)
    pub fn position(&self, instrument_id: &str) -> Position {
        self.positions
            .get(instrument_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all open orders for an instrument
    pub fn orders_for(&self, instrument_id: &str) -> Vec<&OpenOrder> {
        self.open_orders
            .values()
            .filter(|o| o.instrument_id == instrument_id)
            .collect()
    }
}

/// Strategy trait - implement this for your trading strategy
#[async_trait]
pub trait Strategy: Send {
    /// Strategy name for logging
    fn name(&self) -> &str;

    /// Called when order book updates
    async fn on_book_update(
        &mut self,
        update: &OrderBookUpdate,
        ctx: &StrategyContext<'_>,
    ) -> Vec<Action>;

    /// Called when a trade occurs (optional)
    async fn on_trade(&mut self, _trade: &TradeMessage, _ctx: &StrategyContext<'_>) -> Vec<Action> {
        Vec::new()
    }

    /// Called when own order status changes (optional)
    async fn on_order_update(
        &mut self,
        _update: &OrderResponse,
        _ctx: &StrategyContext<'_>,
    ) -> Vec<Action> {
        Vec::new()
    }

    /// Called periodically for time-based logic (optional)
    async fn on_tick(&mut self, _ctx: &StrategyContext<'_>) -> Vec<Action> {
        Vec::new()
    }

    /// Called when a market event arrives (fair value, sentiment, etc.)
    /// Override this for informed trading strategies
    async fn on_event(&mut self, _event: &MarketEvent, _ctx: &StrategyContext<'_>) -> Vec<Action> {
        Vec::new()
    }

    /// Called on shutdown to cleanup (optional)
    async fn on_shutdown(&mut self) -> Vec<Action> {
        // Default: cancel all orders
        vec![Action::CancelAll {
            instrument_id: None,
        }]
    }
}
