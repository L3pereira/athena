use crate::domain::{Order, OrderId, PriceLevel, Symbol, Timestamp, Trade};
use tokio::sync::oneshot;

/// Commands that can be sent to an order book shard
#[derive(Debug)]
pub enum OrderBookCommand {
    /// Submit an order for matching
    SubmitOrder {
        order: Order,
        timestamp: Timestamp,
        response: oneshot::Sender<SubmitOrderResponse>,
    },

    /// Cancel an existing order
    CancelOrder {
        symbol: Symbol,
        order_id: OrderId,
        timestamp: Timestamp,
        response: oneshot::Sender<CancelOrderResponse>,
    },

    /// Get order book depth
    GetDepth {
        symbol: Symbol,
        limit: usize,
        response: oneshot::Sender<GetDepthResponse>,
    },

    /// Get a specific order
    GetOrder {
        order_id: OrderId,
        response: oneshot::Sender<Option<Order>>,
    },

    /// Get or create an order book (for initialization)
    GetOrCreateBook {
        symbol: Symbol,
        response: oneshot::Sender<()>,
    },

    /// Get current sequence number
    GetSequence {
        symbol: Symbol,
        response: oneshot::Sender<Option<u64>>,
    },

    /// Shutdown the shard
    Shutdown,
}

/// Response from submitting an order
#[derive(Debug, Clone)]
pub struct SubmitOrderResponse {
    pub order: Order,
    pub trades: Vec<Trade>,
    pub remaining: Option<Order>,
}

/// Response from cancelling an order
#[derive(Debug, Clone)]
pub enum CancelOrderResponse {
    Cancelled(Order),
    NotFound,
    AlreadyFilled,
}

/// Response from getting depth
#[derive(Debug, Clone)]
pub struct GetDepthResponse {
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
    pub sequence: u64,
}

/// Statistics for a shard
#[derive(Debug, Clone, Default)]
pub struct ShardStats {
    pub shard_id: usize,
    pub num_symbols: usize,
    pub total_orders_processed: u64,
    pub total_trades_executed: u64,
    pub commands_in_queue: usize,
}
