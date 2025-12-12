use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::interval;

use crate::domain::{
    DepthFetcher, OrderBookWriter, QualifiedSymbol, StreamData, SyncStatus, WsEvent,
};
use crate::infrastructure::WsRequestSender;

use super::config::MarketDataConfig;

/// Per-symbol state
struct SymbolState {
    status: SyncStatus,
    /// Buffered updates while waiting for snapshot
    buffer: VecDeque<BufferedUpdate>,
}

struct BufferedUpdate {
    first_update_id: u64,
    final_update_id: u64,
    bids: Vec<[String; 2]>,
    asks: Vec<[String; 2]>,
}

/// Internal state shared between tasks
struct HandlerState {
    symbols: HashMap<String, SymbolState>,
    /// Queue of symbols needing snapshots (FIFO)
    snapshot_queue: VecDeque<String>,
    /// Symbols currently in the queue (for dedup)
    in_queue: HashSet<String>,
}

/// Market data handler that manages sync state and rate-limited snapshots.
///
/// Application layer - orchestrates domain logic using infrastructure.
///
/// Generic over:
/// - `F`: DepthFetcher - for fetching order book snapshots
/// - `B`: OrderBookWriter - for writing to order books
pub struct MarketDataHandler<F, B>
where
    F: DepthFetcher + 'static,
    B: OrderBookWriter + 'static,
{
    config: MarketDataConfig,
    fetcher: Arc<F>,
    order_books: Arc<B>,
    state: Arc<Mutex<HandlerState>>,
}

impl<F, B> MarketDataHandler<F, B>
where
    F: DepthFetcher + 'static,
    B: OrderBookWriter + 'static,
{
    pub fn new(config: MarketDataConfig, fetcher: F, order_books: B) -> Self {
        let mut symbols = HashMap::new();
        for sym in &config.symbols {
            symbols.insert(
                sym.to_uppercase(),
                SymbolState {
                    status: SyncStatus::Uninitialized,
                    buffer: VecDeque::new(),
                },
            );
        }

        MarketDataHandler {
            config,
            fetcher: Arc::new(fetcher),
            order_books: Arc::new(order_books),
            state: Arc::new(Mutex::new(HandlerState {
                symbols,
                snapshot_queue: VecDeque::new(),
                in_queue: HashSet::new(),
            })),
        }
    }

    /// Create handler with pre-wrapped Arc dependencies
    pub fn with_arcs(config: MarketDataConfig, fetcher: Arc<F>, order_books: Arc<B>) -> Self {
        let mut symbols = HashMap::new();
        for sym in &config.symbols {
            symbols.insert(
                sym.to_uppercase(),
                SymbolState {
                    status: SyncStatus::Uninitialized,
                    buffer: VecDeque::new(),
                },
            );
        }

        MarketDataHandler {
            config,
            fetcher,
            order_books,
            state: Arc::new(Mutex::new(HandlerState {
                symbols,
                snapshot_queue: VecDeque::new(),
                in_queue: HashSet::new(),
            })),
        }
    }

    /// Get sync status for a symbol
    pub fn status(&self, symbol: &str) -> SyncStatus {
        let symbol_upper = symbol.to_uppercase();
        self.state
            .lock()
            .symbols
            .get(&symbol_upper)
            .map(|s| s.status)
            .unwrap_or(SyncStatus::Uninitialized)
    }

    /// Check if all symbols are synced
    pub fn all_synced(&self) -> bool {
        self.state
            .lock()
            .symbols
            .values()
            .all(|s| s.status == SyncStatus::Synced)
    }

    /// Get list of symbols that are not synced
    pub fn unsynced_symbols(&self) -> Vec<String> {
        self.state
            .lock()
            .symbols
            .iter()
            .filter(|(_, s)| s.status != SyncStatus::Synced)
            .map(|(sym, _)| sym.clone())
            .collect()
    }

    /// Start the handler - returns a channel to send WS events to
    pub async fn start(self: Arc<Self>, ws_sender: WsRequestSender) -> mpsc::Sender<WsEvent> {
        let (tx, rx) = mpsc::channel(1024);

        // Spawn event processing task
        let handler = Arc::clone(&self);
        tokio::spawn(async move {
            handler.run_event_loop(rx).await;
        });

        // Spawn snapshot fetching task
        let handler = Arc::clone(&self);
        tokio::spawn(async move {
            handler.run_snapshot_loop().await;
        });

        // Subscribe to depth streams for all symbols
        let streams: Vec<String> = self
            .config
            .symbols
            .iter()
            .map(|s| format!("{}@depth", s.to_lowercase()))
            .collect();

        if !streams.is_empty()
            && let Err(e) = ws_sender.subscribe(streams).await
        {
            tracing::error!("Failed to subscribe to streams: {:?}", e);
        }

        // Queue initial snapshots for all symbols
        {
            let mut state = self.state.lock();
            for symbol in &self.config.symbols {
                let symbol_upper = symbol.to_uppercase();
                if let Some(sym_state) = state.symbols.get_mut(&symbol_upper) {
                    sym_state.status = SyncStatus::Syncing;
                }
                if !state.in_queue.contains(&symbol_upper) {
                    state.snapshot_queue.push_back(symbol_upper.clone());
                    state.in_queue.insert(symbol_upper);
                }
            }
        }

        tx
    }

    async fn run_event_loop(self: Arc<Self>, mut rx: mpsc::Receiver<WsEvent>) {
        while let Some(event) = rx.recv().await {
            match event {
                WsEvent::StreamData(StreamData::DepthUpdate {
                    symbol,
                    first_update_id,
                    final_update_id,
                    bids,
                    asks,
                    ..
                }) => {
                    self.handle_depth_update(&symbol, first_update_id, final_update_id, bids, asks);
                }
                WsEvent::Disconnected => {
                    tracing::warn!("WebSocket disconnected, marking all symbols out of sync");
                    self.mark_all_out_of_sync();
                }
                WsEvent::Error(e) => {
                    tracing::error!("WebSocket error: {}", e);
                }
                _ => {}
            }
        }
    }

    fn handle_depth_update(
        &self,
        symbol: &str,
        first_update_id: u64,
        final_update_id: u64,
        bids: Vec<[String; 2]>,
        asks: Vec<[String; 2]>,
    ) {
        let symbol_upper = symbol.to_uppercase();
        let mut state = self.state.lock();

        let Some(sym_state) = state.symbols.get_mut(&symbol_upper) else {
            return;
        };

        match sym_state.status {
            SyncStatus::Synced => {
                let update = StreamData::DepthUpdate {
                    symbol: symbol_upper.clone(),
                    event_time: 0,
                    first_update_id,
                    final_update_id,
                    bids,
                    asks,
                };

                if !self
                    .order_books
                    .apply_update(&self.config.exchange_id, &update)
                {
                    tracing::warn!("{} out of sync, queueing for resync", symbol_upper);
                    sym_state.status = SyncStatus::OutOfSync;
                    if !state.in_queue.contains(&symbol_upper) {
                        state.snapshot_queue.push_back(symbol_upper.clone());
                        state.in_queue.insert(symbol_upper);
                    }
                }
            }
            SyncStatus::Syncing | SyncStatus::OutOfSync => {
                if sym_state.buffer.len() < self.config.max_buffer_size {
                    sym_state.buffer.push_back(BufferedUpdate {
                        first_update_id,
                        final_update_id,
                        bids,
                        asks,
                    });
                } else {
                    tracing::warn!("{} buffer full, dropping update", symbol_upper);
                }
            }
            SyncStatus::Uninitialized => {
                sym_state.status = SyncStatus::Syncing;
                if !state.in_queue.contains(&symbol_upper) {
                    state.snapshot_queue.push_back(symbol_upper.clone());
                    state.in_queue.insert(symbol_upper);
                }
            }
        }
    }

    async fn run_snapshot_loop(self: Arc<Self>) {
        let mut ticker = interval(self.config.snapshot_interval);

        loop {
            ticker.tick().await;

            let symbol = {
                let mut state = self.state.lock();
                if let Some(sym) = state.snapshot_queue.pop_front() {
                    state.in_queue.remove(&sym);
                    Some(sym)
                } else {
                    None
                }
            };

            let Some(symbol) = symbol else {
                continue;
            };

            tracing::debug!("Fetching snapshot for {}", symbol);

            match self.fetcher.get_depth(&symbol, Some(100)).await {
                Ok(snapshot) => {
                    self.apply_snapshot_with_buffer(&symbol, snapshot);
                }
                Err(e) => {
                    tracing::error!("Failed to fetch snapshot for {}: {:?}", symbol, e);
                    let mut state = self.state.lock();
                    if !state.in_queue.contains(&symbol) {
                        state.snapshot_queue.push_back(symbol.clone());
                        state.in_queue.insert(symbol);
                    }
                }
            }
        }
    }

    fn apply_snapshot_with_buffer(&self, symbol: &str, snapshot: trading_core::DepthSnapshotEvent) {
        let symbol_upper = symbol.to_uppercase();
        let last_update_id = snapshot.last_update_id;

        let key = QualifiedSymbol::new(self.config.exchange_id.clone(), &symbol_upper);
        self.order_books.apply_snapshot(&key, &snapshot);

        let mut state = self.state.lock();
        let Some(sym_state) = state.symbols.get_mut(&symbol_upper) else {
            return;
        };

        let mut found_first = false;
        while let Some(update) = sym_state.buffer.pop_front() {
            if !found_first {
                let expected = last_update_id + 1;
                if update.first_update_id <= expected && expected <= update.final_update_id {
                    found_first = true;
                } else if update.final_update_id < expected {
                    continue;
                } else {
                    tracing::warn!(
                        "{} has gap in updates after snapshot (expected {}, got {}..{})",
                        symbol_upper,
                        expected,
                        update.first_update_id,
                        update.final_update_id
                    );
                    sym_state.status = SyncStatus::OutOfSync;
                    sym_state.buffer.clear();
                    if !state.in_queue.contains(&symbol_upper) {
                        state.snapshot_queue.push_back(symbol_upper.clone());
                        state.in_queue.insert(symbol_upper);
                    }
                    return;
                }
            }

            let stream_data = StreamData::DepthUpdate {
                symbol: symbol_upper.clone(),
                event_time: 0,
                first_update_id: update.first_update_id,
                final_update_id: update.final_update_id,
                bids: update.bids,
                asks: update.asks,
            };

            if !self
                .order_books
                .apply_update(&self.config.exchange_id, &stream_data)
            {
                tracing::warn!("{} failed to apply buffered update", symbol_upper);
                sym_state.status = SyncStatus::OutOfSync;
                sym_state.buffer.clear();
                if !state.in_queue.contains(&symbol_upper) {
                    state.snapshot_queue.push_back(symbol_upper.clone());
                    state.in_queue.insert(symbol_upper);
                }
                return;
            }
        }

        sym_state.status = SyncStatus::Synced;
        tracing::info!("{} synced successfully", symbol_upper);
    }

    fn mark_all_out_of_sync(&self) {
        let mut state = self.state.lock();

        let symbols_to_queue: Vec<String> = state
            .symbols
            .iter_mut()
            .filter_map(|(symbol, sym_state)| {
                if sym_state.status == SyncStatus::Synced {
                    sym_state.status = SyncStatus::OutOfSync;
                    sym_state.buffer.clear();
                    Some(symbol.clone())
                } else {
                    None
                }
            })
            .collect();

        for symbol in symbols_to_queue {
            if !state.in_queue.contains(&symbol) {
                state.snapshot_queue.push_back(symbol.clone());
                state.in_queue.insert(symbol);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_status_in_handler() {
        let _status = SyncStatus::Uninitialized;
    }
}
