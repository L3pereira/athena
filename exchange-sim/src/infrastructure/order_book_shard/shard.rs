use crate::application::ports::SyncEventSink;
use crate::domain::{
    ExchangeEvent, Order, OrderBook, OrderId, Symbol, Timestamp, TradeExecutedEvent,
};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::thread::{self, JoinHandle};

use super::command::{
    CancelOrderResponse, GetDepthResponse, OrderBookCommand, ShardStats, SubmitOrderResponse,
};

/// Configuration for a shard
#[derive(Debug, Clone)]
pub struct ShardConfig {
    /// Shard identifier
    pub shard_id: usize,
    /// Channel buffer size for commands
    pub command_buffer_size: usize,
    /// Whether to pin this shard to a specific CPU core
    pub pin_to_core: Option<usize>,
}

impl Default for ShardConfig {
    fn default() -> Self {
        Self {
            shard_id: 0,
            command_buffer_size: 10_000,
            pin_to_core: None,
        }
    }
}

// Shard state constants
const SHARD_STATE_ALIVE: u8 = 0;
const SHARD_STATE_SHUTTING_DOWN: u8 = 1;
const SHARD_STATE_DEAD: u8 = 2;

/// Handle to communicate with a shard
#[derive(Clone)]
pub struct ShardHandle {
    pub shard_id: usize,
    sender: Sender<OrderBookCommand>,
    orders_processed: Arc<AtomicU64>,
    trades_executed: Arc<AtomicU64>,
    state: Arc<AtomicU8>,
}

impl ShardHandle {
    /// Send a command to the shard
    pub fn send(&self, cmd: OrderBookCommand) -> Result<(), ShardError> {
        self.sender.send(cmd).map_err(|_| ShardError::ShardShutdown)
    }

    /// Get shard statistics
    pub fn stats(&self) -> ShardStats {
        ShardStats {
            shard_id: self.shard_id,
            num_symbols: 0, // Would need to query shard
            total_orders_processed: self.orders_processed.load(Ordering::Relaxed),
            total_trades_executed: self.trades_executed.load(Ordering::Relaxed),
            commands_in_queue: self.sender.len(),
        }
    }

    /// Check if shard is alive (uses proper state tracking, not heuristics)
    pub fn is_alive(&self) -> bool {
        self.state.load(Ordering::Acquire) == SHARD_STATE_ALIVE
    }
}

/// A shard that owns and processes multiple order books
pub struct OrderBookShard {
    config: ShardConfig,
    books: HashMap<String, OrderBook>,
    order_index: HashMap<OrderId, String>, // order_id -> symbol
    receiver: Receiver<OrderBookCommand>,
    event_sink: Arc<dyn SyncEventSink>,
    orders_processed: Arc<AtomicU64>,
    trades_executed: Arc<AtomicU64>,
    state: Arc<AtomicU8>,
}

impl OrderBookShard {
    /// Create a new shard and return its handle
    pub fn spawn(
        config: ShardConfig,
        event_sink: Arc<dyn SyncEventSink>,
    ) -> (ShardHandle, JoinHandle<()>) {
        let (sender, receiver) = bounded(config.command_buffer_size);
        let orders_processed = Arc::new(AtomicU64::new(0));
        let trades_executed = Arc::new(AtomicU64::new(0));
        let state = Arc::new(AtomicU8::new(SHARD_STATE_ALIVE));

        let handle = ShardHandle {
            shard_id: config.shard_id,
            sender,
            orders_processed: Arc::clone(&orders_processed),
            trades_executed: Arc::clone(&trades_executed),
            state: Arc::clone(&state),
        };

        let shard = OrderBookShard {
            config: config.clone(),
            books: HashMap::new(),
            order_index: HashMap::new(),
            receiver,
            event_sink,
            orders_processed,
            trades_executed,
            state,
        };

        let thread_handle = thread::Builder::new()
            .name(format!("orderbook-shard-{}", config.shard_id))
            .spawn(move || {
                shard.run();
            })
            .expect("Failed to spawn shard thread");

        (handle, thread_handle)
    }

    /// Main event loop - processes commands sequentially
    fn run(mut self) {
        tracing::info!(shard_id = self.config.shard_id, "Shard started");

        // Optionally pin to CPU core
        #[cfg(target_os = "linux")]
        if let Some(core) = self.config.pin_to_core
            && let Err(e) = self.pin_to_core(core)
        {
            tracing::warn!(
                shard_id = self.config.shard_id,
                core = core,
                error = %e,
                "Failed to pin shard to core"
            );
        }

        loop {
            match self.receiver.recv() {
                Ok(cmd) => {
                    if !self.process_command(cmd) {
                        // Mark as shutting down
                        self.state
                            .store(SHARD_STATE_SHUTTING_DOWN, Ordering::Release);
                        break;
                    }
                }
                Err(_) => {
                    // Channel closed, shutdown
                    tracing::info!(shard_id = self.config.shard_id, "Shard channel closed");
                    self.state
                        .store(SHARD_STATE_SHUTTING_DOWN, Ordering::Release);
                    break;
                }
            }
        }

        // Mark as dead
        self.state.store(SHARD_STATE_DEAD, Ordering::Release);
        tracing::info!(shard_id = self.config.shard_id, "Shard shutdown complete");
    }

    /// Process a single command, returns false if should shutdown
    fn process_command(&mut self, cmd: OrderBookCommand) -> bool {
        match cmd {
            OrderBookCommand::SubmitOrder {
                order,
                timestamp,
                response,
            } => {
                let result = self.handle_submit_order(order, timestamp);
                let _ = response.send(result);
            }

            OrderBookCommand::CancelOrder {
                symbol,
                order_id,
                timestamp,
                response,
            } => {
                let result = self.handle_cancel_order(&symbol, order_id, timestamp);
                let _ = response.send(result);
            }

            OrderBookCommand::GetDepth {
                symbol,
                limit,
                response,
            } => {
                let result = self.handle_get_depth(&symbol, limit);
                let _ = response.send(result);
            }

            OrderBookCommand::GetOrder { order_id, response } => {
                let result = self.handle_get_order(order_id);
                let _ = response.send(result);
            }

            OrderBookCommand::GetOrCreateBook { symbol, response } => {
                self.get_or_create_book(&symbol);
                let _ = response.send(());
            }

            OrderBookCommand::GetSequence { symbol, response } => {
                let seq = self.books.get(&symbol.to_string()).map(|b| b.sequence());
                let _ = response.send(seq);
            }

            OrderBookCommand::Shutdown => {
                return false;
            }
        }
        true
    }

    fn handle_submit_order(&mut self, order: Order, timestamp: Timestamp) -> SubmitOrderResponse {
        let symbol_str = order.symbol.to_string();
        let order_symbol = order.symbol.clone();
        let book = self.get_or_create_book(&order_symbol);

        // Match the order
        let (trades, remaining) = book.match_order(order.clone(), timestamp);

        // Update stats
        self.orders_processed.fetch_add(1, Ordering::Relaxed);
        self.trades_executed
            .fetch_add(trades.len() as u64, Ordering::Relaxed);

        // Add remaining order to book if it exists and should rest
        if let Some(ref rem) = remaining {
            // Re-get the book since we need a fresh mutable borrow
            let book = self.books.get_mut(&symbol_str).unwrap();
            book.add_order(rem.clone());
            self.order_index.insert(rem.id, symbol_str.clone());
        }

        // Publish trade events via the event sink abstraction
        for trade in &trades {
            self.event_sink
                .send(ExchangeEvent::TradeExecuted(TradeExecutedEvent::from(
                    trade,
                )));
        }

        SubmitOrderResponse {
            order,
            trades,
            remaining,
        }
    }

    fn handle_cancel_order(
        &mut self,
        symbol: &Symbol,
        order_id: OrderId,
        timestamp: Timestamp,
    ) -> CancelOrderResponse {
        let symbol_str = symbol.to_string();

        if let Some(book) = self.books.get_mut(&symbol_str)
            && let Some(mut order) = book.remove_order(order_id)
        {
            order.cancel(timestamp);
            self.order_index.remove(&order_id);
            return CancelOrderResponse::Cancelled(order);
        }

        CancelOrderResponse::NotFound
    }

    fn handle_get_depth(&self, symbol: &Symbol, limit: usize) -> GetDepthResponse {
        let symbol_str = symbol.to_string();

        if let Some(book) = self.books.get(&symbol_str) {
            GetDepthResponse {
                bids: book.get_bids(limit),
                asks: book.get_asks(limit),
                sequence: book.sequence(),
            }
        } else {
            GetDepthResponse {
                bids: Vec::new(),
                asks: Vec::new(),
                sequence: 0,
            }
        }
    }

    fn handle_get_order(&self, order_id: OrderId) -> Option<Order> {
        // Look up which symbol this order belongs to
        let symbol_str = self.order_index.get(&order_id)?;
        let book = self.books.get(symbol_str)?;
        book.get_order(order_id).cloned()
    }

    fn get_or_create_book(&mut self, symbol: &Symbol) -> &mut OrderBook {
        let symbol_str = symbol.to_string();
        self.books
            .entry(symbol_str)
            .or_insert_with(|| OrderBook::new(symbol.clone()))
    }

    #[cfg(target_os = "linux")]
    fn pin_to_core(&self, core: usize) -> Result<(), std::io::Error> {
        unsafe {
            let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
            libc::CPU_ZERO(&mut cpuset);
            libc::CPU_SET(core, &mut cpuset);

            let result = libc::pthread_setaffinity_np(
                libc::pthread_self(),
                std::mem::size_of::<libc::cpu_set_t>(),
                &cpuset,
            );

            if result != 0 {
                return Err(std::io::Error::from_raw_os_error(result));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum ShardError {
    ShardShutdown,
    Timeout,
    ChannelFull,
}

impl std::fmt::Display for ShardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardError::ShardShutdown => write!(f, "Shard has shutdown"),
            ShardError::Timeout => write!(f, "Operation timed out"),
            ShardError::ChannelFull => write!(f, "Command channel is full"),
        }
    }
}

impl std::error::Error for ShardError {}
