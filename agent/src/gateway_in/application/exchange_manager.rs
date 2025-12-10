use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::gateway_in::config::GatewayConfigFile;
use crate::gateway_in::domain::{ExchangeId, OrderBookWriter, WsEvent};
use crate::gateway_in::infrastructure::{RestClient, WsClient};

use super::market_data_handler::MarketDataHandler;

/// Manages connections to multiple exchanges
pub struct ExchangeManager<B>
where
    B: OrderBookWriter + Clone + 'static,
{
    config: GatewayConfigFile,
    order_books: Arc<B>,
    handlers: HashMap<ExchangeId, ExchangeConnection>,
}

struct ExchangeConnection {
    rest_client: RestClient,
    ws_client: WsClient,
    event_sender: Option<mpsc::Sender<WsEvent>>,
}

impl<B> ExchangeManager<B>
where
    B: OrderBookWriter + Clone + 'static,
{
    /// Create a new exchange manager from configuration
    pub fn new(config: GatewayConfigFile, order_books: B) -> Self {
        ExchangeManager {
            config,
            order_books: Arc::new(order_books),
            handlers: HashMap::new(),
        }
    }

    /// Initialize all enabled exchanges
    pub fn initialize(&mut self) {
        for exchange_config in self.config.enabled_exchanges() {
            let exchange_id = ExchangeId::new(&exchange_config.id);

            let rest_client = RestClient::new(
                exchange_config.rest_url.clone(),
                exchange_config.api_key.clone(),
            );

            let ws_client = WsClient::new(exchange_config.ws_url.clone());

            self.handlers.insert(
                exchange_id,
                ExchangeConnection {
                    rest_client,
                    ws_client,
                    event_sender: None,
                },
            );
        }
    }

    /// Start market data handlers for all enabled exchanges
    pub async fn start_all(&mut self) -> HashMap<ExchangeId, mpsc::Sender<WsEvent>> {
        let mut senders = HashMap::new();

        // Collect exchange configs first to avoid borrow conflict
        let exchanges_to_start: Vec<_> = self
            .config
            .enabled_exchanges()
            .iter()
            .map(|e| {
                (
                    ExchangeId::new(&e.id),
                    e.market_data.clone(),
                    e.symbols.clone(),
                )
            })
            .collect();

        for (exchange_id, market_data_config, symbols) in exchanges_to_start {
            if let Some(sender) = self
                .start_exchange(&exchange_id, market_data_config, symbols)
                .await
            {
                senders.insert(exchange_id, sender);
            }
        }

        senders
    }

    /// Start a specific exchange
    async fn start_exchange(
        &mut self,
        exchange_id: &ExchangeId,
        market_data_config: crate::gateway_in::config::MarketDataConfigJson,
        symbols: Vec<String>,
    ) -> Option<mpsc::Sender<WsEvent>> {
        let connection = self.handlers.get_mut(exchange_id)?;

        // Connect WebSocket
        let (ws_sender, mut ws_receiver) = match connection.ws_client.connect().await {
            Ok((sender, receiver)) => (sender, receiver),
            Err(e) => {
                tracing::error!("Failed to connect to {}: {:?}", exchange_id, e);
                return None;
            }
        };

        // Create market data config
        let md_config = market_data_config.to_market_data_config(exchange_id.clone(), symbols);

        // Create market data handler
        let handler = Arc::new(MarketDataHandler::with_arcs(
            md_config,
            Arc::new(connection.rest_client.clone()),
            Arc::clone(&self.order_books),
        ));

        // Start the handler
        let event_sender = handler.start(ws_sender).await;

        // Forward WS events to the handler
        let event_sender_clone = event_sender.clone();
        let exchange_id_clone = exchange_id.clone();
        tokio::spawn(async move {
            while let Some(event) = ws_receiver.recv().await {
                if event_sender_clone.send(event).await.is_err() {
                    tracing::warn!("Handler for {} closed", exchange_id_clone);
                    break;
                }
            }
        });

        connection.event_sender = Some(event_sender.clone());
        Some(event_sender)
    }

    /// Get the REST client for a specific exchange
    pub fn rest_client(&self, exchange_id: &ExchangeId) -> Option<&RestClient> {
        self.handlers.get(exchange_id).map(|c| &c.rest_client)
    }

    /// Get list of connected exchange IDs
    pub fn connected_exchanges(&self) -> Vec<ExchangeId> {
        self.handlers
            .iter()
            .filter(|(_, c)| c.event_sender.is_some())
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Get the configuration
    pub fn config(&self) -> &GatewayConfigFile {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway_in::config::load_default_config;

    #[test]
    fn test_exchange_manager_creation() {
        use crate::order_book::OrderBookManager;

        let config = load_default_config().unwrap();
        let order_books = OrderBookManager::new();
        let mut manager = ExchangeManager::new(config, order_books);

        manager.initialize();

        // Should have simulator initialized
        assert!(manager.rest_client(&ExchangeId::simulator()).is_some());
    }
}
