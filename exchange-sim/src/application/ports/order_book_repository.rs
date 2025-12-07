use crate::domain::{Order, OrderBook, OrderId, PriceLevel, Symbol};
use async_trait::async_trait;

// ============================================================================
// Focused Repository Traits (ISP-compliant)
// ============================================================================

/// Read operations for order books
#[async_trait]
pub trait OrderBookReader: Send + Sync {
    /// Get or create an order book for a symbol
    async fn get_or_create(&self, symbol: &Symbol) -> OrderBook;

    /// Get an order book by symbol (if exists)
    async fn get(&self, symbol: &Symbol) -> Option<OrderBook>;
}

/// Write operations for order books
#[async_trait]
pub trait OrderBookWriter: Send + Sync {
    /// Save an order book
    async fn save(&self, book: OrderBook);
}

/// Market data queries (depth, sequences)
#[async_trait]
pub trait MarketDataReader: Send + Sync {
    /// Get depth snapshot for a symbol
    async fn get_depth(
        &self,
        symbol: &Symbol,
        limit: usize,
    ) -> Option<(Vec<PriceLevel>, Vec<PriceLevel>, u64)>;

    /// Get current sequence number for a symbol
    async fn get_sequence(&self, symbol: &Symbol) -> Option<u64>;

    /// Get all active symbols
    async fn get_symbols(&self) -> Vec<Symbol>;
}

/// Order lookup operations
#[async_trait]
pub trait OrderLookup: Send + Sync {
    /// Get a specific order from any book
    async fn get_order(&self, order_id: OrderId) -> Option<Order>;
}

// ============================================================================
// Composite Trait (backwards compatible)
// ============================================================================

/// Full repository combining all operations
/// Use this for backwards compatibility or when you need all operations
#[async_trait]
pub trait OrderBookRepository:
    OrderBookReader + OrderBookWriter + MarketDataReader + OrderLookup
{
}

/// Blanket implementation for any type implementing all focused traits
impl<T> OrderBookRepository for T where
    T: OrderBookReader + OrderBookWriter + MarketDataReader + OrderLookup
{
}
