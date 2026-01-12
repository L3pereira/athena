//! Exchange Adapter
//!
//! Wraps exchange-sim for use in ABM simulations.
//! Provides a simplified interface for agents to submit orders and receive fills.

use crate::application::agents::{AgentAction, Fill, OrderType};
use chrono::Duration;
use exchange_sim::{
    AccountRepository, BroadcastEventPublisher, Clock, ControllableClock, Exchange, ExchangeConfig,
    ExchangeEvent, InMemoryAccountRepository, InMemoryOrderBookRepository, OrderBookReader,
    OrderType as ExOrderType, SimulationClock, SubmitOrderCommand, Symbol, TimeInForce,
    TradingPairConfig, Value, application::use_cases::SubmitOrderUseCase,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use trading_core::{Price, Quantity, Side};

// Re-export Side for seed_orderbook

/// Wrapper around exchange-sim for ABM integration
pub struct ExchangeAdapter {
    exchange: Exchange<SimulationClock>,
    symbol: Symbol,
}

impl ExchangeAdapter {
    /// Create a new exchange adapter with default configuration
    pub fn new(symbol_str: &str) -> Result<Self, String> {
        let config = ExchangeConfig::default();
        let exchange = Exchange::new(config);

        let symbol = Symbol::new(symbol_str).map_err(|e| e.to_string())?;

        // Add trading pair
        let pair_config = TradingPairConfig::new(symbol.clone(), "BTC", "USDT").with_fees_bps(1, 2); // 1 bps maker, 2 bps taker
        exchange.instrument_repo.add(pair_config);

        Ok(Self { exchange, symbol })
    }

    /// Create an account for an agent with initial balances
    pub async fn create_account(&self, agent_id: &str, btc_balance: i64, usdt_balance: i64) {
        let mut account = self.exchange.account_repo.get_or_create(agent_id).await;
        account.deposit("BTC", Value::from_raw(btc_balance as i128));
        account.deposit("USDT", Value::from_raw(usdt_balance as i128));
        self.exchange.account_repo.save(account).await;
    }

    /// Subscribe to exchange events for the trading symbol
    pub fn subscribe(&self) -> broadcast::Receiver<ExchangeEvent> {
        self.exchange
            .event_publisher
            .subscribe_symbol(self.symbol.as_str())
    }

    /// Submit an order from an agent action
    pub async fn submit_order(&self, agent_id: &str, action: &AgentAction) -> Result<Fill, String> {
        let (client_order_id, side, price, quantity, order_type) = match action {
            AgentAction::SubmitOrder {
                client_order_id,
                side,
                price,
                quantity,
                order_type,
                ..
            } => (
                *client_order_id,
                *side,
                *price,
                *quantity,
                order_type.clone(),
            ),
            _ => return Err("Not an order submission".to_string()),
        };

        let use_case = SubmitOrderUseCase::new(
            Arc::clone(&self.exchange.clock),
            Arc::clone(&self.exchange.account_repo),
            Arc::clone(&self.exchange.order_book_repo),
            Arc::clone(&self.exchange.instrument_repo),
            Arc::clone(&self.exchange.event_publisher),
            Arc::clone(&self.exchange.rate_limiter),
        );

        let ex_order_type = match order_type {
            OrderType::Market => ExOrderType::Market,
            OrderType::Limit => ExOrderType::Limit,
            OrderType::PostOnly => ExOrderType::LimitMaker,
        };

        let command = SubmitOrderCommand {
            symbol: self.symbol.to_string(),
            side,
            order_type: ex_order_type,
            quantity,
            price: Some(price),
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            client_order_id: Some(format!("{}_{}", agent_id, client_order_id)),
        };

        match use_case.execute(agent_id, command).await {
            Ok(result) => {
                // Aggregate fills
                let total_qty: i64 = result.fills.iter().map(|f| f.quantity.raw()).sum();
                let total_fee: i64 = result.fills.iter().map(|f| f.commission.raw() as i64).sum();
                let avg_price = if !result.fills.is_empty() && total_qty > 0 {
                    // Use i128 to avoid overflow when multiplying price * quantity
                    let weighted_sum: i128 = result
                        .fills
                        .iter()
                        .map(|f| (f.price.raw() as i128) * (f.quantity.raw() as i128))
                        .sum();
                    Price::from_raw((weighted_sum / total_qty as i128) as i64)
                } else {
                    price
                };

                let signed_qty = match side {
                    Side::Buy => total_qty,
                    Side::Sell => -total_qty,
                };

                Ok(Fill {
                    order_id: client_order_id,
                    price: avg_price,
                    signed_qty,
                    fee: total_fee,
                    timestamp_ms: self.exchange.clock.now_millis() as u64,
                })
            }
            Err(e) => Err(format!("{:?}", e)),
        }
    }

    /// Get the current best bid price
    pub async fn best_bid(&self) -> Option<Price> {
        let book = self
            .exchange
            .order_book_repo
            .get_or_create(&self.symbol)
            .await;
        book.best_bid()
    }

    /// Get the current best ask price
    pub async fn best_ask(&self) -> Option<Price> {
        let book = self
            .exchange
            .order_book_repo
            .get_or_create(&self.symbol)
            .await;
        book.best_ask()
    }

    /// Get order book depth (top N levels)
    pub async fn get_depth(
        &self,
        levels: usize,
    ) -> (Vec<(Price, Quantity)>, Vec<(Price, Quantity)>) {
        let book = self
            .exchange
            .order_book_repo
            .get_or_create(&self.symbol)
            .await;
        let bids: Vec<_> = book
            .get_bids(levels)
            .into_iter()
            .map(|l| (l.price, l.quantity))
            .collect();
        let asks: Vec<_> = book
            .get_asks(levels)
            .into_iter()
            .map(|l| (l.price, l.quantity))
            .collect();
        (bids, asks)
    }

    /// Advance the simulation clock
    pub fn advance_time(&self, millis: i64) {
        self.exchange.clock.advance(Duration::milliseconds(millis));
    }

    /// Get current timestamp
    pub fn now_ms(&self) -> u64 {
        self.exchange.clock.now_millis() as u64
    }

    /// Seed the orderbook with initial liquidity around a mid price
    ///
    /// This solves the cold-start problem by providing initial quotes
    /// for agents to trade against.
    pub async fn seed_orderbook(
        &self,
        liquidity_provider: &str,
        mid_price: Price,
        spread_bps: i64,
        num_levels: usize,
        size_per_level: Quantity,
    ) -> Result<(), String> {
        use exchange_sim::application::use_cases::SubmitOrderUseCase;

        let half_spread = (mid_price.raw() * spread_bps) / 20_000; // half of spread_bps

        let use_case = SubmitOrderUseCase::new(
            Arc::clone(&self.exchange.clock),
            Arc::clone(&self.exchange.account_repo),
            Arc::clone(&self.exchange.order_book_repo),
            Arc::clone(&self.exchange.instrument_repo),
            Arc::clone(&self.exchange.event_publisher),
            Arc::clone(&self.exchange.rate_limiter),
        );

        for level in 0..num_levels {
            let offset = half_spread + (level as i64 * half_spread / 2);

            // Bid level
            let bid_price = Price::from_raw(mid_price.raw() - offset);
            let bid_cmd = SubmitOrderCommand {
                symbol: self.symbol.to_string(),
                side: Side::Buy,
                order_type: ExOrderType::Limit,
                quantity: size_per_level,
                price: Some(bid_price),
                stop_price: None,
                time_in_force: TimeInForce::Gtc,
                client_order_id: Some(format!("{}_seed_bid_{}", liquidity_provider, level)),
            };
            use_case
                .execute(liquidity_provider, bid_cmd)
                .await
                .map_err(|e| format!("{:?}", e))?;

            // Ask level
            let ask_price = Price::from_raw(mid_price.raw() + offset);
            let ask_cmd = SubmitOrderCommand {
                symbol: self.symbol.to_string(),
                side: Side::Sell,
                order_type: ExOrderType::Limit,
                quantity: size_per_level,
                price: Some(ask_price),
                stop_price: None,
                time_in_force: TimeInForce::Gtc,
                client_order_id: Some(format!("{}_seed_ask_{}", liquidity_provider, level)),
            };
            use_case
                .execute(liquidity_provider, ask_cmd)
                .await
                .map_err(|e| format!("{:?}", e))?;
        }

        Ok(())
    }

    /// Get the exchange event publisher
    pub fn event_publisher(&self) -> &Arc<BroadcastEventPublisher> {
        &self.exchange.event_publisher
    }

    /// Get account repository (for checking balances)
    pub fn account_repo(&self) -> &Arc<InMemoryAccountRepository> {
        &self.exchange.account_repo
    }

    /// Get order book repository
    pub fn order_book_repo(&self) -> &Arc<InMemoryOrderBookRepository> {
        &self.exchange.order_book_repo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_adapter() {
        let adapter = ExchangeAdapter::new("BTCUSDT").unwrap();
        assert!(adapter.best_bid().await.is_none()); // Empty book
        assert!(adapter.best_ask().await.is_none());
    }

    #[tokio::test]
    async fn test_create_account() {
        let adapter = ExchangeAdapter::new("BTCUSDT").unwrap();

        // Create account with 10 BTC and 1M USDT
        adapter
            .create_account("agent_1", 10_00000000, 1_000_000_00000000)
            .await;

        // Verify account exists
        let account = adapter.account_repo().get_or_create("agent_1").await;
        assert!(account.balance("BTC").available.raw() > 0);
        assert!(account.balance("USDT").available.raw() > 0);
    }
}
