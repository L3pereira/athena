//! Agent Runner - Runs trading strategies and manages their lifecycle
//!
//! Each agent wraps a strategy and handles:
//! - Receiving market data updates
//! - Receiving event feed updates (for informed traders)
//! - Publishing actions/signals to order manager
//! - Managing local state (order book, positions)

use athena_gateway::messages::{
    market_data::OrderBookUpdate,
    order::{OrderRequest, OrderResponse},
};
use athena_strategy::{
    LocalOrderBook, MarketEvent, OpenOrder, Position, Strategy, StrategyContext,
};
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

/// Agent configuration
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Unique agent identifier
    pub agent_id: String,
    /// Exchange account ID for this agent
    pub account_id: Uuid,
    /// Instruments to subscribe to
    pub instruments: Vec<String>,
    /// Whether this agent receives event feed
    pub uses_event_feed: bool,
}

/// Agent runner - wraps a strategy and manages its lifecycle
pub struct AgentRunner<S: Strategy> {
    /// Agent configuration
    config: AgentConfig,
    /// The trading strategy
    strategy: S,
    /// Local order books (per instrument)
    books: HashMap<String, LocalOrderBook>,
    /// Current positions
    positions: HashMap<String, Position>,
    /// Open orders
    open_orders: HashMap<String, OpenOrder>,
    /// Market data receiver
    md_rx: broadcast::Receiver<OrderBookUpdate>,
    /// Event feed receiver (optional)
    event_rx: Option<broadcast::Receiver<MarketEvent>>,
    /// Order request sender
    order_tx: mpsc::Sender<AgentOrder>,
    /// Order response receiver
    order_response_rx: mpsc::Receiver<OrderResponse>,
}

/// Order from an agent (includes agent identity)
#[derive(Debug, Clone)]
pub struct AgentOrder {
    pub agent_id: String,
    pub account_id: Uuid,
    pub request: OrderRequest,
}

impl<S: Strategy> AgentRunner<S> {
    /// Create a new agent runner
    pub fn new(
        config: AgentConfig,
        strategy: S,
        md_rx: broadcast::Receiver<OrderBookUpdate>,
        event_rx: Option<broadcast::Receiver<MarketEvent>>,
        order_tx: mpsc::Sender<AgentOrder>,
        order_response_rx: mpsc::Receiver<OrderResponse>,
    ) -> Self {
        let mut books = HashMap::new();
        for instrument in &config.instruments {
            books.insert(instrument.clone(), LocalOrderBook::new(instrument));
        }

        Self {
            config,
            strategy,
            books,
            positions: HashMap::new(),
            open_orders: HashMap::new(),
            md_rx,
            event_rx,
            order_tx,
            order_response_rx,
        }
    }

    /// Build context with cloned data (needed for async to avoid borrow conflicts)
    fn build_context(
        &self,
    ) -> (
        HashMap<String, LocalOrderBook>,
        HashMap<String, Position>,
        HashMap<String, OpenOrder>,
    ) {
        (
            self.books.clone(),
            self.positions.clone(),
            self.open_orders.clone(),
        )
    }

    /// Process market data update
    async fn handle_book_update(&mut self, update: OrderBookUpdate) {
        // Update local book
        let instrument_id = update.instrument_id().to_string();
        if let Some(book) = self.books.get_mut(&instrument_id) {
            book.apply_update(&update);
        }

        // Clone context data for async call (avoids borrow conflict with strategy)
        let (books, positions, open_orders) = self.build_context();
        let ctx = StrategyContext {
            books: &books,
            positions: &positions,
            open_orders: &open_orders,
        };

        // Let strategy react
        let actions = self.strategy.on_book_update(&update, &ctx).await;

        // Process actions
        self.process_actions(actions).await;
    }

    /// Process event feed update
    async fn handle_event(&mut self, event: MarketEvent) {
        log::debug!(
            "[{}] Received event for {}: {:?}",
            self.config.agent_id,
            event.instrument_id(),
            event
        );

        // Clone context data for async call
        let (books, positions, open_orders) = self.build_context();
        let ctx = StrategyContext {
            books: &books,
            positions: &positions,
            open_orders: &open_orders,
        };

        // Let strategy react to event
        let actions = self.strategy.on_event(&event, &ctx).await;

        // Process any actions from the strategy
        self.process_actions(actions).await;
    }

    /// Process order response
    async fn handle_order_response(&mut self, response: OrderResponse) {
        use athena_gateway::messages::order::OrderStatusWire;

        // Update open orders based on status
        match response.status {
            OrderStatusWire::Accepted => {
                log::debug!(
                    "[{}] Order accepted: {}",
                    self.config.agent_id,
                    response.client_order_id
                );
            }
            OrderStatusWire::Filled | OrderStatusWire::PartiallyFilled => {
                log::debug!(
                    "[{}] Order filled: {} @ {:?}",
                    self.config.agent_id,
                    response.filled_qty,
                    response.avg_price
                );
                // Remove from open orders if fully filled
                if let Some(order) = self.open_orders.get_mut(&response.client_order_id) {
                    order.filled_qty = response.filled_qty;
                    if order.filled_qty >= order.quantity {
                        self.open_orders.remove(&response.client_order_id);
                    }
                }
            }
            OrderStatusWire::Cancelled | OrderStatusWire::Expired => {
                self.open_orders.remove(&response.client_order_id);
            }
            OrderStatusWire::Rejected => {
                log::warn!(
                    "[{}] Order rejected: {} - {:?}",
                    self.config.agent_id,
                    response.client_order_id,
                    response.reject_reason
                );
                self.open_orders.remove(&response.client_order_id);
            }
        }

        // Clone context data for async call
        let (books, positions, open_orders) = self.build_context();
        let ctx = StrategyContext {
            books: &books,
            positions: &positions,
            open_orders: &open_orders,
        };

        // Let strategy react
        let actions = self.strategy.on_order_update(&response, &ctx).await;
        self.process_actions(actions).await;
    }

    /// Process actions from strategy
    async fn process_actions(&mut self, actions: Vec<athena_strategy::Action>) {
        for action in actions {
            match action {
                athena_strategy::Action::SubmitOrder(request) => {
                    // Track open order
                    let open_order = OpenOrder {
                        client_order_id: request.client_order_id.clone(),
                        instrument_id: request.instrument_id.clone(),
                        side: request.side,
                        price: request.price,
                        quantity: request.quantity,
                        filled_qty: rust_decimal::Decimal::ZERO,
                    };
                    self.open_orders
                        .insert(request.client_order_id.clone(), open_order);

                    // Send to exchange
                    let agent_order = AgentOrder {
                        agent_id: self.config.agent_id.clone(),
                        account_id: self.config.account_id,
                        request,
                    };

                    if let Err(e) = self.order_tx.send(agent_order).await {
                        log::error!("[{}] Failed to send order: {}", self.config.agent_id, e);
                    }
                }
                athena_strategy::Action::CancelOrder { client_order_id } => {
                    log::debug!(
                        "[{}] Cancelling order: {}",
                        self.config.agent_id,
                        client_order_id
                    );
                    // Would send cancel request
                }
                athena_strategy::Action::CancelAll { instrument_id } => {
                    log::debug!(
                        "[{}] Cancelling all orders for {:?}",
                        self.config.agent_id,
                        instrument_id
                    );
                }
            }
        }
    }

    /// Run the agent
    pub async fn run(mut self) {
        log::info!("[{}] Agent started", self.config.agent_id);

        loop {
            tokio::select! {
                // Market data update
                result = self.md_rx.recv() => {
                    match result {
                        Ok(update) => self.handle_book_update(update).await,
                        Err(broadcast::error::RecvError::Closed) => {
                            log::info!("[{}] Market data channel closed", self.config.agent_id);
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("[{}] Lagged {} market data messages", self.config.agent_id, n);
                        }
                    }
                }

                // Event feed update (if subscribed)
                result = async {
                    match &mut self.event_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match result {
                        Ok(event) => self.handle_event(event).await,
                        Err(broadcast::error::RecvError::Closed) => {
                            log::info!("[{}] Event feed closed", self.config.agent_id);
                            self.event_rx = None;
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            log::warn!("[{}] Lagged {} event messages", self.config.agent_id, n);
                        }
                    }
                }

                // Order responses
                Some(response) = self.order_response_rx.recv() => {
                    self.handle_order_response(response).await;
                }
            }
        }

        // Shutdown
        let actions = self.strategy.on_shutdown().await;
        self.process_actions(actions).await;

        log::info!("[{}] Agent stopped", self.config.agent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full tests are in the integration test file
    // These are just basic unit tests

    #[test]
    fn test_agent_config() {
        let config = AgentConfig {
            agent_id: "test-agent".to_string(),
            account_id: Uuid::new_v4(),
            instruments: vec!["BTC-USD".to_string()],
            uses_event_feed: true,
        };

        assert_eq!(config.agent_id, "test-agent");
        assert!(config.uses_event_feed);
    }
}
