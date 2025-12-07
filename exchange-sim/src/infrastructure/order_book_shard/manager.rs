use crate::application::ports::SyncEventSink;
use crate::domain::{ExchangeEvent, Order, OrderId, Symbol, Timestamp};
use crate::infrastructure::BroadcastEventPublisher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio::sync::{broadcast, oneshot};

use super::command::{
    CancelOrderResponse, GetDepthResponse, OrderBookCommand, ShardStats, SubmitOrderResponse,
};
use super::shard::{OrderBookShard, ShardConfig, ShardError, ShardHandle};

// ============================================================================
// Sharding Strategy (DIP-compliant)
// ============================================================================

/// Strategy for distributing symbols across shards
pub trait ShardingStrategy: Send + Sync {
    /// Get the shard index for a symbol
    fn get_shard_index(&self, symbol: &str, num_shards: usize) -> usize;
}

/// Default sharding strategy using consistent hashing
pub struct ConsistentHashStrategy;

impl ShardingStrategy for ConsistentHashStrategy {
    fn get_shard_index(&self, symbol: &str, num_shards: usize) -> usize {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        symbol.hash(&mut hasher);
        (hasher.finish() as usize) % num_shards
    }
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the sharded order book manager
#[derive(Debug, Clone)]
pub struct ShardManagerConfig {
    /// Number of regular shards (for non-hot symbols)
    pub num_shards: usize,
    /// Symbols that get their own dedicated shard
    pub hot_symbols: HashSet<String>,
    /// Buffer size for command channels
    pub command_buffer_size: usize,
    /// Whether to pin shards to CPU cores
    pub pin_to_cores: bool,
}

impl Default for ShardManagerConfig {
    fn default() -> Self {
        Self {
            num_shards: num_cpus::get().max(4),
            hot_symbols: HashSet::new(),
            command_buffer_size: 10_000,
            pin_to_cores: false,
        }
    }
}

impl ShardManagerConfig {
    pub fn with_hot_symbols(
        mut self,
        symbols: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.hot_symbols = symbols.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_num_shards(mut self, n: usize) -> Self {
        self.num_shards = n;
        self
    }

    pub fn with_core_pinning(mut self, enabled: bool) -> Self {
        self.pin_to_cores = enabled;
        self
    }
}

// ============================================================================
// Sharded Order Book Manager
// ============================================================================

/// Manages multiple order book shards with consistent hashing
pub struct ShardedOrderBookManager {
    /// Regular shards for non-hot symbols
    regular_shards: Vec<ShardHandle>,
    /// Dedicated shards for hot symbols (symbol -> shard)
    hot_symbol_shards: HashMap<String, ShardHandle>,
    /// Thread handles for cleanup
    thread_handles: Vec<JoinHandle<()>>,
    /// Event publisher (for subscriptions)
    event_publisher: Arc<BroadcastEventPublisher>,
    /// Sharding strategy
    sharding_strategy: Arc<dyn ShardingStrategy>,
    /// Configuration (kept for introspection)
    #[allow(dead_code)]
    config: ShardManagerConfig,
}

impl ShardedOrderBookManager {
    /// Create and start all shards with default sharding strategy
    pub fn new(config: ShardManagerConfig) -> Self {
        Self::with_strategy(config, Arc::new(ConsistentHashStrategy))
    }

    /// Create with custom sharding strategy
    pub fn with_strategy(
        config: ShardManagerConfig,
        sharding_strategy: Arc<dyn ShardingStrategy>,
    ) -> Self {
        let event_publisher = Arc::new(BroadcastEventPublisher::new(100_000));
        // Cast to dyn SyncEventSink for passing to shards
        let event_sink: Arc<dyn SyncEventSink> =
            Arc::clone(&event_publisher) as Arc<dyn SyncEventSink>;

        let mut regular_shards = Vec::with_capacity(config.num_shards);
        let mut hot_symbol_shards = HashMap::new();
        let mut thread_handles = Vec::new();

        // Spawn regular shards
        for shard_id in 0..config.num_shards {
            let shard_config = ShardConfig {
                shard_id,
                command_buffer_size: config.command_buffer_size,
                pin_to_core: if config.pin_to_cores {
                    Some(shard_id % num_cpus::get())
                } else {
                    None
                },
            };

            let (handle, thread) = OrderBookShard::spawn(shard_config, Arc::clone(&event_sink));
            regular_shards.push(handle);
            thread_handles.push(thread);
        }

        // Spawn dedicated shards for hot symbols
        let mut hot_shard_id = config.num_shards;
        for symbol in &config.hot_symbols {
            let shard_config = ShardConfig {
                shard_id: hot_shard_id,
                command_buffer_size: config.command_buffer_size * 2, // Larger buffer for hot symbols
                pin_to_core: if config.pin_to_cores {
                    Some(hot_shard_id % num_cpus::get())
                } else {
                    None
                },
            };

            let (handle, thread) = OrderBookShard::spawn(shard_config, Arc::clone(&event_sink));
            hot_symbol_shards.insert(symbol.clone(), handle);
            thread_handles.push(thread);
            hot_shard_id += 1;
        }

        tracing::info!(
            regular_shards = config.num_shards,
            hot_symbols = config.hot_symbols.len(),
            "ShardedOrderBookManager started"
        );

        Self {
            regular_shards,
            hot_symbol_shards,
            thread_handles,
            event_publisher,
            sharding_strategy,
            config,
        }
    }

    /// Get the shard handle for a symbol
    fn get_shard(&self, symbol: &str) -> &ShardHandle {
        // Check if this is a hot symbol with dedicated shard
        if let Some(shard) = self.hot_symbol_shards.get(symbol) {
            return shard;
        }

        // Otherwise, use sharding strategy to pick a regular shard
        let shard_idx = self
            .sharding_strategy
            .get_shard_index(symbol, self.regular_shards.len());
        &self.regular_shards[shard_idx]
    }

    /// Submit an order
    pub async fn submit_order(
        &self,
        order: Order,
        timestamp: Timestamp,
    ) -> Result<SubmitOrderResponse, ShardError> {
        let shard = self.get_shard(order.symbol.as_str());
        let (tx, rx) = oneshot::channel();

        shard.send(OrderBookCommand::SubmitOrder {
            order,
            timestamp,
            response: tx,
        })?;

        rx.await.map_err(|_| ShardError::ShardShutdown)
    }

    /// Cancel an order
    pub async fn cancel_order(
        &self,
        symbol: &Symbol,
        order_id: OrderId,
        timestamp: Timestamp,
    ) -> Result<CancelOrderResponse, ShardError> {
        let shard = self.get_shard(symbol.as_str());
        let (tx, rx) = oneshot::channel();

        shard.send(OrderBookCommand::CancelOrder {
            symbol: symbol.clone(),
            order_id,
            timestamp,
            response: tx,
        })?;

        rx.await.map_err(|_| ShardError::ShardShutdown)
    }

    /// Get order book depth
    pub async fn get_depth(
        &self,
        symbol: &Symbol,
        limit: usize,
    ) -> Result<GetDepthResponse, ShardError> {
        let shard = self.get_shard(symbol.as_str());
        let (tx, rx) = oneshot::channel();

        shard.send(OrderBookCommand::GetDepth {
            symbol: symbol.clone(),
            limit,
            response: tx,
        })?;

        rx.await.map_err(|_| ShardError::ShardShutdown)
    }

    /// Get a specific order
    pub async fn get_order(
        &self,
        symbol: &Symbol,
        order_id: OrderId,
    ) -> Result<Option<Order>, ShardError> {
        let shard = self.get_shard(symbol.as_str());
        let (tx, rx) = oneshot::channel();

        shard.send(OrderBookCommand::GetOrder {
            order_id,
            response: tx,
        })?;

        rx.await.map_err(|_| ShardError::ShardShutdown)
    }

    /// Get or create an order book
    pub async fn get_or_create_book(&self, symbol: &Symbol) -> Result<(), ShardError> {
        let shard = self.get_shard(symbol.as_str());
        let (tx, rx) = oneshot::channel();

        shard.send(OrderBookCommand::GetOrCreateBook {
            symbol: symbol.clone(),
            response: tx,
        })?;

        rx.await.map_err(|_| ShardError::ShardShutdown)
    }

    /// Get sequence number for a symbol
    pub async fn get_sequence(&self, symbol: &Symbol) -> Result<Option<u64>, ShardError> {
        let shard = self.get_shard(symbol.as_str());
        let (tx, rx) = oneshot::channel();

        shard.send(OrderBookCommand::GetSequence {
            symbol: symbol.clone(),
            response: tx,
        })?;

        rx.await.map_err(|_| ShardError::ShardShutdown)
    }

    /// Subscribe to exchange events
    pub fn subscribe(&self) -> broadcast::Receiver<ExchangeEvent> {
        self.event_publisher.subscribe()
    }

    /// Subscribe to events for a specific symbol
    pub fn subscribe_symbol(&self, symbol: &str) -> broadcast::Receiver<ExchangeEvent> {
        self.event_publisher.subscribe_symbol(symbol)
    }

    /// Get statistics for all shards
    pub fn stats(&self) -> Vec<ShardStats> {
        let mut stats = Vec::new();

        for shard in &self.regular_shards {
            stats.push(shard.stats());
        }

        for shard in self.hot_symbol_shards.values() {
            let mut s = shard.stats();
            s.num_symbols = 1; // Dedicated shard
            stats.push(s);
        }

        stats
    }

    /// Check if all shards are healthy
    pub fn is_healthy(&self) -> bool {
        self.regular_shards.iter().all(|s| s.is_alive())
            && self.hot_symbol_shards.values().all(|s| s.is_alive())
    }

    /// Send shutdown command to all shards (helper to avoid duplication)
    fn send_shutdown_to_all_shards(&self) {
        for shard in &self.regular_shards {
            let _ = shard.send(OrderBookCommand::Shutdown);
        }
        for shard in self.hot_symbol_shards.values() {
            let _ = shard.send(OrderBookCommand::Shutdown);
        }
    }

    /// Shutdown all shards gracefully
    pub fn shutdown(mut self) {
        self.send_shutdown_to_all_shards();

        // Wait for threads to finish - take ownership of handles
        let handles = std::mem::take(&mut self.thread_handles);
        for handle in handles {
            let _ = handle.join();
        }

        tracing::info!("ShardedOrderBookManager shutdown complete");
    }
}

impl Drop for ShardedOrderBookManager {
    fn drop(&mut self) {
        // Send shutdown to all shards (threads will exit when channel closes)
        self.send_shutdown_to_all_shards();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Price, Quantity, Side, TimeInForce};
    use chrono::Utc;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn test_submit_order_to_shard() {
        let config = ShardManagerConfig::default().with_num_shards(2);
        let manager = ShardedOrderBookManager::new(config);

        let symbol = Symbol::new("BTCUSDT").unwrap();
        let order = Order::new_limit(
            symbol,
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            TimeInForce::Gtc,
        );

        let result = manager.submit_order(order, Utc::now()).await;
        assert!(result.is_ok());

        manager.shutdown();
    }

    #[tokio::test]
    async fn test_hot_symbol_dedicated_shard() {
        let config = ShardManagerConfig::default()
            .with_num_shards(2)
            .with_hot_symbols(vec!["BTCUSDT"]);

        let manager = ShardedOrderBookManager::new(config);

        // BTC goes to dedicated shard
        let btc_symbol = Symbol::new("BTCUSDT").unwrap();
        let btc_order = Order::new_limit(
            btc_symbol.clone(),
            Side::Buy,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            TimeInForce::Gtc,
        );

        // ETH goes to regular shard
        let eth_symbol = Symbol::new("ETHUSDT").unwrap();
        let eth_order = Order::new_limit(
            eth_symbol.clone(),
            Side::Buy,
            Quantity::from(dec!(10)),
            Price::from(dec!(3000)),
            TimeInForce::Gtc,
        );

        let btc_result = manager.submit_order(btc_order, Utc::now()).await;
        let eth_result = manager.submit_order(eth_order, Utc::now()).await;

        assert!(btc_result.is_ok());
        assert!(eth_result.is_ok());

        manager.shutdown();
    }

    #[tokio::test]
    async fn test_get_depth() {
        let config = ShardManagerConfig::default().with_num_shards(2);
        let manager = ShardedOrderBookManager::new(config);

        let symbol = Symbol::new("BTCUSDT").unwrap();

        // Submit some orders
        for i in 1..=5 {
            let price = Price::from(rust_decimal::Decimal::from(50000 - i * 100));
            let order = Order::new_limit(
                symbol.clone(),
                Side::Buy,
                Quantity::from(dec!(1)),
                price,
                TimeInForce::Gtc,
            );
            manager.submit_order(order, Utc::now()).await.unwrap();
        }

        let depth = manager.get_depth(&symbol, 10).await.unwrap();
        assert_eq!(depth.bids.len(), 5);
        assert_eq!(depth.asks.len(), 0);

        manager.shutdown();
    }

    #[tokio::test]
    async fn test_custom_sharding_strategy() {
        // Test with custom strategy
        struct RoundRobinStrategy {
            counter: std::sync::atomic::AtomicUsize,
        }

        impl ShardingStrategy for RoundRobinStrategy {
            fn get_shard_index(&self, _symbol: &str, num_shards: usize) -> usize {
                self.counter
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    % num_shards
            }
        }

        let strategy = Arc::new(RoundRobinStrategy {
            counter: std::sync::atomic::AtomicUsize::new(0),
        });

        let config = ShardManagerConfig::default().with_num_shards(4);
        let manager = ShardedOrderBookManager::with_strategy(config, strategy);

        assert!(manager.is_healthy());
        manager.shutdown();
    }
}
