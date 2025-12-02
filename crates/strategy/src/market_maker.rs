//! Basic Inventory-Based Market Maker
//!
//! A simple market making strategy that:
//! - Quotes bid/ask around mid price
//! - Skews quotes based on inventory (reduces position when too long/short)
//! - Respects position limits
//! - Cancels and replaces quotes when price moves

use crate::orderbook::LocalOrderBook;
use crate::strategy::{Action, Position, Strategy, StrategyContext};
use async_trait::async_trait;
use athena_gateway::messages::{
    market_data::OrderBookUpdate,
    order::{OrderRequest, OrderResponse, OrderSide, TimeInForceWire},
};
use log::{debug, info};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Configuration for the market maker
#[derive(Debug, Clone)]
pub struct MarketMakerConfig {
    /// Instrument to trade
    pub instrument_id: String,
    /// Base spread in basis points (e.g., 10 = 0.1%)
    pub spread_bps: Decimal,
    /// Quote size
    pub quote_size: Decimal,
    /// Maximum position (absolute value)
    pub max_position: Decimal,
    /// Inventory skew factor: how much to adjust spread per unit of inventory
    /// Higher = more aggressive in reducing inventory
    pub skew_factor: Decimal,
    /// Minimum tick size for price rounding
    pub tick_size: Decimal,
    /// Price change threshold to requote (in ticks)
    pub requote_threshold: Decimal,
}

impl Default for MarketMakerConfig {
    fn default() -> Self {
        Self {
            instrument_id: "BTC-USD".to_string(),
            spread_bps: dec!(10),       // 10 bps = 0.1%
            quote_size: dec!(0.1),      // 0.1 BTC per side
            max_position: dec!(1.0),    // Max 1 BTC position
            skew_factor: dec!(5),       // 5 bps per unit of inventory
            tick_size: dec!(0.01),      // $0.01 tick
            requote_threshold: dec!(5), // Requote if mid moves 5 ticks
        }
    }
}

/// State tracked by the market maker
#[derive(Debug, Default)]
struct MMState {
    /// Last mid price we quoted around
    last_mid: Option<Decimal>,
    /// Current bid order ID
    bid_order_id: Option<String>,
    /// Current ask order ID
    ask_order_id: Option<String>,
    /// Order ID counter for generating unique IDs
    order_counter: u64,
}

/// Basic inventory-based market maker
pub struct BasicMarketMaker {
    config: MarketMakerConfig,
    state: MMState,
}

impl BasicMarketMaker {
    pub fn new(config: MarketMakerConfig) -> Self {
        Self {
            config,
            state: MMState::default(),
        }
    }

    /// Generate a unique client order ID
    fn next_order_id(&mut self) -> String {
        self.state.order_counter += 1;
        format!("mm-{}", self.state.order_counter)
    }

    /// Round price to tick size
    fn round_price(&self, price: Decimal, is_bid: bool) -> Decimal {
        let ticks = price / self.config.tick_size;
        let rounded = if is_bid {
            ticks.floor() // Round down for bids
        } else {
            ticks.ceil() // Round up for asks
        };
        rounded * self.config.tick_size
    }

    /// Calculate quote prices based on mid and inventory
    fn calculate_quotes(&self, mid: Decimal, position: &Position) -> (Decimal, Decimal) {
        let half_spread_bps = self.config.spread_bps / dec!(2);

        // Inventory skew: positive inventory -> lower bid, raise ask
        let skew_bps = position.quantity * self.config.skew_factor;

        // Calculate prices in bps from mid
        let bid_offset_bps = half_spread_bps + skew_bps;
        let ask_offset_bps = half_spread_bps - skew_bps;

        // Convert bps to price
        let bid_price = mid * (dec!(1) - bid_offset_bps / dec!(10000));
        let ask_price = mid * (dec!(1) + ask_offset_bps / dec!(10000));

        // Round to tick
        (
            self.round_price(bid_price, true),
            self.round_price(ask_price, false),
        )
    }

    /// Check if we should requote based on price movement
    fn should_requote(&self, current_mid: Decimal) -> bool {
        match self.state.last_mid {
            Some(last_mid) => {
                let diff = (current_mid - last_mid).abs();
                let threshold = self.config.requote_threshold * self.config.tick_size;
                diff >= threshold
            }
            None => true, // No previous quote, should quote
        }
    }

    /// Generate quote actions based on current state
    fn generate_quotes(&mut self, book: &LocalOrderBook, position: &Position) -> Vec<Action> {
        let mut actions = Vec::new();

        // Get mid price
        let mid = match book.mid_price() {
            Some(m) => m,
            None => {
                debug!("No mid price available, skipping quotes");
                return actions;
            }
        };

        // Check if we should requote
        if !self.should_requote(mid) && self.state.bid_order_id.is_some() {
            return actions; // No need to requote
        }

        // Cancel existing orders if any
        if let Some(bid_id) = self.state.bid_order_id.take() {
            actions.push(Action::CancelOrder {
                client_order_id: bid_id,
            });
        }
        if let Some(ask_id) = self.state.ask_order_id.take() {
            actions.push(Action::CancelOrder {
                client_order_id: ask_id,
            });
        }

        // Calculate new quote prices
        let (bid_price, ask_price) = self.calculate_quotes(mid, position);

        // Check position limits for bid (buying)
        if position.quantity < self.config.max_position {
            let bid_qty = self
                .config
                .quote_size
                .min(self.config.max_position - position.quantity);
            if bid_qty > Decimal::ZERO {
                let bid_id = self.next_order_id();
                actions.push(Action::SubmitOrder(OrderRequest::limit(
                    &bid_id,
                    &self.config.instrument_id,
                    OrderSide::Buy,
                    bid_qty,
                    bid_price,
                    TimeInForceWire::Gtc,
                )));
                self.state.bid_order_id = Some(bid_id);
            }
        }

        // Check position limits for ask (selling)
        if position.quantity > -self.config.max_position {
            let ask_qty = self
                .config
                .quote_size
                .min(self.config.max_position + position.quantity);
            if ask_qty > Decimal::ZERO {
                let ask_id = self.next_order_id();
                actions.push(Action::SubmitOrder(OrderRequest::limit(
                    &ask_id,
                    &self.config.instrument_id,
                    OrderSide::Sell,
                    ask_qty,
                    ask_price,
                    TimeInForceWire::Gtc,
                )));
                self.state.ask_order_id = Some(ask_id);
            }
        }

        // Update last mid
        self.state.last_mid = Some(mid);

        if !actions.is_empty() {
            info!(
                "[{}] Quoting: bid={:.2}@{:.4} ask={:.2}@{:.4} pos={:.4} mid={:.2}",
                self.config.instrument_id,
                bid_price,
                self.config.quote_size,
                ask_price,
                self.config.quote_size,
                position.quantity,
                mid
            );
        }

        actions
    }
}

#[async_trait]
impl Strategy for BasicMarketMaker {
    fn name(&self) -> &str {
        "BasicMarketMaker"
    }

    async fn on_book_update(
        &mut self,
        update: &OrderBookUpdate,
        ctx: &StrategyContext<'_>,
    ) -> Vec<Action> {
        // Only process updates for our instrument
        let instrument_id = match update {
            OrderBookUpdate::Snapshot { instrument_id, .. } => instrument_id,
            OrderBookUpdate::Delta { instrument_id, .. } => instrument_id,
        };

        if instrument_id != &self.config.instrument_id {
            return Vec::new();
        }

        // Get order book and position
        let book = match ctx.book(instrument_id) {
            Some(b) => b,
            None => return Vec::new(),
        };
        let position = ctx.position(instrument_id);

        // Generate quotes
        self.generate_quotes(book, &position)
    }

    async fn on_order_update(
        &mut self,
        update: &OrderResponse,
        _ctx: &StrategyContext<'_>,
    ) -> Vec<Action> {
        // Track order state
        if update.is_terminal() {
            // Order is done, clear our reference
            if self.state.bid_order_id.as_ref() == Some(&update.client_order_id) {
                self.state.bid_order_id = None;
            }
            if self.state.ask_order_id.as_ref() == Some(&update.client_order_id) {
                self.state.ask_order_id = None;
            }

            debug!(
                "Order {} terminal: {:?}",
                update.client_order_id, update.status
            );
        }

        Vec::new()
    }

    async fn on_shutdown(&mut self) -> Vec<Action> {
        info!("Market maker shutting down, cancelling all orders");
        vec![Action::CancelAll {
            instrument_id: Some(self.config.instrument_id.clone()),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_quotes_no_inventory() {
        let config = MarketMakerConfig {
            spread_bps: dec!(20), // 20 bps = 0.2%
            tick_size: dec!(1),
            ..Default::default()
        };
        let mm = BasicMarketMaker::new(config);

        let mid = dec!(50000);
        let position = Position::default();

        let (bid, ask) = mm.calculate_quotes(mid, &position);

        // 10 bps = 0.1% = 50 each side
        // Bid should be ~49950, Ask should be ~50050
        assert!(bid < mid);
        assert!(ask > mid);
        assert!((bid - dec!(49950)).abs() < dec!(1));
        assert!((ask - dec!(50050)).abs() < dec!(1));
    }

    #[test]
    fn test_calculate_quotes_with_inventory() {
        let config = MarketMakerConfig {
            spread_bps: dec!(20),
            skew_factor: dec!(10), // 10 bps per unit
            tick_size: dec!(1),
            ..Default::default()
        };
        let mm = BasicMarketMaker::new(config);

        let mid = dec!(50000);

        // Long 0.5 position -> should lower bid, raise ask
        let position = Position {
            quantity: dec!(0.5),
            ..Default::default()
        };

        let (bid, ask) = mm.calculate_quotes(mid, &position);

        // Skew = 0.5 * 10 = 5 bps
        // Bid offset = 10 + 5 = 15 bps
        // Ask offset = 10 - 5 = 5 bps
        // So bid moves further from mid, ask moves closer
        let no_inv_position = Position::default();
        let (no_inv_bid, no_inv_ask) = mm.calculate_quotes(mid, &no_inv_position);

        assert!(bid < no_inv_bid); // Bid is lower when long
        assert!(ask < no_inv_ask); // Ask is also lower (closer to mid)
    }

    #[test]
    fn test_position_limits() {
        let config = MarketMakerConfig {
            max_position: dec!(1.0),
            quote_size: dec!(0.5),
            ..Default::default()
        };
        let mut mm = BasicMarketMaker::new(config);

        // Create a book with mid price
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&OrderBookUpdate::Snapshot {
            instrument_id: "BTC-USD".to_string(),
            bids: vec![athena_gateway::BookLevel::new(dec!(50000), dec!(10))],
            asks: vec![athena_gateway::BookLevel::new(dec!(50100), dec!(10))],
            sequence: 1,
            timestamp_ns: 0,
        });

        // At max long position -> should only quote ask
        let position = Position {
            quantity: dec!(1.0), // At max
            ..Default::default()
        };

        let actions = mm.generate_quotes(&book, &position);

        // Should have cancel (if any) + only ask order
        let submit_count = actions
            .iter()
            .filter(|a| matches!(a, Action::SubmitOrder(_)))
            .count();

        // Verify only one order (the ask)
        assert_eq!(submit_count, 1);

        if let Some(Action::SubmitOrder(order)) =
            actions.iter().find(|a| matches!(a, Action::SubmitOrder(_)))
        {
            assert_eq!(order.side, OrderSide::Sell);
        }
    }
}
