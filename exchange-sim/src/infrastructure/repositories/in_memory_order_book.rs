use crate::application::ports::OrderBookRepository;
use crate::domain::{Order, OrderBook, OrderId, PriceLevel, Symbol};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

/// In-memory order book repository
///
/// Thread-safe storage for order books using DashMap.
/// Suitable for simulation and testing.
pub struct InMemoryOrderBookRepository {
    books: Arc<DashMap<String, OrderBook>>,
}

impl InMemoryOrderBookRepository {
    pub fn new() -> Self {
        InMemoryOrderBookRepository {
            books: Arc::new(DashMap::new()),
        }
    }
}

impl Default for InMemoryOrderBookRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryOrderBookRepository {
    fn clone(&self) -> Self {
        InMemoryOrderBookRepository {
            books: Arc::clone(&self.books),
        }
    }
}

#[async_trait]
impl OrderBookRepository for InMemoryOrderBookRepository {
    async fn get_or_create(&self, symbol: &Symbol) -> OrderBook {
        let key = symbol.to_string();

        // Try to get existing
        if let Some(book) = self.books.get(&key) {
            return book.value().clone();
        }

        // Create new
        let book = OrderBook::new(symbol.clone());
        self.books.insert(key.clone(), book.clone());
        book
    }

    async fn get(&self, symbol: &Symbol) -> Option<OrderBook> {
        self.books
            .get(&symbol.to_string())
            .map(|b| b.value().clone())
    }

    async fn save(&self, book: OrderBook) {
        self.books.insert(book.symbol().to_string(), book);
    }

    async fn get_order(&self, order_id: OrderId) -> Option<Order> {
        for entry in self.books.iter() {
            if let Some(order) = entry.value().get_order(order_id) {
                return Some(order.clone());
            }
        }
        None
    }

    async fn get_symbols(&self) -> Vec<Symbol> {
        self.books
            .iter()
            .map(|entry| entry.value().symbol().clone())
            .collect()
    }

    async fn get_depth(
        &self,
        symbol: &Symbol,
        limit: usize,
    ) -> Option<(Vec<PriceLevel>, Vec<PriceLevel>, u64)> {
        self.books.get(&symbol.to_string()).map(|book| {
            let bids = book.get_bids(limit);
            let asks = book.get_asks(limit);
            let sequence = book.sequence();
            (bids, asks, sequence)
        })
    }

    async fn get_sequence(&self, symbol: &Symbol) -> Option<u64> {
        self.books.get(&symbol.to_string()).map(|b| b.sequence())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Price, Quantity, Side, TimeInForce};
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_get_or_create() {
        let repo = InMemoryOrderBookRepository::new();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        let book1 = repo.get_or_create(&symbol).await;
        let book2 = repo.get_or_create(&symbol).await;

        assert_eq!(book1.symbol(), book2.symbol());
    }

    #[tokio::test]
    async fn test_save_and_get() {
        let repo = InMemoryOrderBookRepository::new();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        let mut book = OrderBook::new(symbol.clone());
        let order = Order::new_limit(
            symbol.clone(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(100)),
            TimeInForce::Gtc,
        );
        book.add_order(order);

        repo.save(book).await;

        let retrieved = repo.get(&symbol).await.unwrap();
        assert_eq!(retrieved.order_count(), 1);
    }
}
