use crate::domain::{Order, OrderBook, OrderId, PriceLevel, Symbol};
use async_trait::async_trait;

/// Repository for managing order books
///
/// This port abstracts the storage and retrieval of order books,
/// allowing different implementations (in-memory, persistent, distributed).
#[async_trait]
pub trait OrderBookRepository: Send + Sync {
    /// Get or create an order book for a symbol
    async fn get_or_create(&self, symbol: &Symbol) -> OrderBook;

    /// Get an order book by symbol (if exists)
    async fn get(&self, symbol: &Symbol) -> Option<OrderBook>;

    /// Save an order book
    async fn save(&self, book: OrderBook);

    /// Get a specific order from any book
    async fn get_order(&self, order_id: OrderId) -> Option<Order>;

    /// Get all active symbols
    async fn get_symbols(&self) -> Vec<Symbol>;

    /// Get depth snapshot for a symbol
    async fn get_depth(
        &self,
        symbol: &Symbol,
        limit: usize,
    ) -> Option<(Vec<PriceLevel>, Vec<PriceLevel>, u64)>;

    /// Get current sequence number for a symbol
    async fn get_sequence(&self, symbol: &Symbol) -> Option<u64>;
}
