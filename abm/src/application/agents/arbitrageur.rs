//! Arbitrageur Agent
//!
//! Profits from price discrepancies between local market and reference (Binance).
//! Role: Keeps local price anchored to reference, provides price discovery.
//!
//! Strategy:
//! - If local_ask < reference_bid - threshold: Buy local (cheap)
//! - If local_bid > reference_ask + threshold: Sell local (expensive)

use super::{Agent, AgentAction, AgentId, Fill, MarketEvent, MarketState};
use risk_management::{PnL, ReferenceFeed};
use trading_core::Quantity;

/// Configuration for arbitrageur
#[derive(Debug, Clone)]
pub struct ArbitrageConfig {
    /// Minimum profit threshold in bps to trigger arb
    pub min_profit_bps: f64,
    /// Order size (raw units)
    pub order_size: i64,
    /// Maximum position before reducing exposure
    pub max_position: i64,
    /// Use aggressive (market) orders vs passive (limit)
    pub aggressive: bool,
}

impl Default for ArbitrageConfig {
    fn default() -> Self {
        Self {
            min_profit_bps: 5.0,       // 5 bps minimum profit
            order_size: 100_000_000,   // 1 unit
            max_position: 500_000_000, // 5 units max exposure
            aggressive: true,          // Market orders for speed
        }
    }
}

/// Arbitrageur agent
pub struct Arbitrageur<F: ReferenceFeed> {
    id: AgentId,
    config: ArbitrageConfig,
    reference_feed: F,
    pnl: PnL,
    position: i64, // Current position for risk management
    next_order_id: u64,
}

impl<F: ReferenceFeed> Arbitrageur<F> {
    pub fn new(id: impl Into<String>, config: ArbitrageConfig, reference_feed: F) -> Self {
        Self {
            id: AgentId::new(id),
            config,
            reference_feed,
            pnl: PnL::new(),
            position: 0,
            next_order_id: 1,
        }
    }

    fn next_client_order_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Check if buying local is profitable
    /// Buy local if we can buy cheap and (theoretically) sell at reference
    fn check_buy_opportunity(&self, state: &MarketState) -> Option<i64> {
        let local_ask = state.bbo.ask_price.raw();
        let ref_price = self.reference_feed.mid_price().raw();

        // Profit = ref_price - local_ask
        let profit_raw = ref_price - local_ask;
        let profit_bps = (profit_raw as f64 / ref_price as f64) * 10_000.0;

        if profit_bps >= self.config.min_profit_bps {
            Some(profit_raw)
        } else {
            None
        }
    }

    /// Check if selling local is profitable
    /// Sell local if we can sell expensive relative to reference
    fn check_sell_opportunity(&self, state: &MarketState) -> Option<i64> {
        let local_bid = state.bbo.bid_price.raw();
        let ref_price = self.reference_feed.mid_price().raw();

        // Profit = local_bid - ref_price
        let profit_raw = local_bid - ref_price;
        let profit_bps = (profit_raw as f64 / ref_price as f64) * 10_000.0;

        if profit_bps >= self.config.min_profit_bps {
            Some(profit_raw)
        } else {
            None
        }
    }
}

impl<F: ReferenceFeed + Send + Sync> Agent for Arbitrageur<F> {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn on_tick(&mut self, state: &MarketState) -> Vec<AgentAction> {
        if !state.has_quotes() {
            return vec![AgentAction::NoOp];
        }

        // Check position limits
        let can_buy = self.position < self.config.max_position;
        let can_sell = self.position > -self.config.max_position;

        let quantity = Quantity::from_raw(self.config.order_size);
        let client_order_id = self.next_client_order_id();

        // Check for buy opportunity (local is cheap)
        if can_buy {
            if let Some(_profit) = self.check_buy_opportunity(state) {
                return if self.config.aggressive {
                    vec![AgentAction::market_buy(client_order_id, quantity)]
                } else {
                    vec![AgentAction::limit_buy(
                        client_order_id,
                        state.bbo.ask_price, // Hit the ask
                        quantity,
                    )]
                };
            }
        }

        // Check for sell opportunity (local is expensive)
        if can_sell {
            if let Some(_profit) = self.check_sell_opportunity(state) {
                return if self.config.aggressive {
                    vec![AgentAction::market_sell(client_order_id, quantity)]
                } else {
                    vec![AgentAction::limit_sell(
                        client_order_id,
                        state.bbo.bid_price, // Hit the bid
                        quantity,
                    )]
                };
            }
        }

        vec![AgentAction::NoOp]
    }

    fn on_fill(&mut self, fill: &Fill) {
        self.pnl.record_trade(fill.signed_qty, fill.price, fill.fee);
        self.position += fill.signed_qty;
    }

    fn on_event(&mut self, _event: &MarketEvent) {
        // Arb tracks position through fills, ignores other events
    }

    fn pnl(&self) -> &PnL {
        &self.pnl
    }

    fn agent_type(&self) -> &'static str {
        "Arbitrageur"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::BBO;
    use risk_management::OrderbookMoments;
    use trading_core::Price;

    struct MockFeed {
        mid_price: Price,
    }

    impl ReferenceFeed for MockFeed {
        fn moments(&self) -> OrderbookMoments {
            OrderbookMoments::default()
        }

        fn mid_price(&self) -> Price {
            self.mid_price
        }
    }

    fn create_arb() -> Arbitrageur<MockFeed> {
        let feed = MockFeed {
            mid_price: Price::from_int(50000),
        };
        Arbitrageur::new("arb-1", ArbitrageConfig::default(), feed)
    }

    #[test]
    fn test_no_arb_when_prices_aligned() {
        let mut arb = create_arb();

        // Local price close to reference
        let mut state = MarketState::empty("BTC-USDT");
        state.bbo = BBO {
            bid_price: Price::from_int(49999),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(50001),
            ask_size: Quantity::from_raw(100),
        };

        let actions = arb.on_tick(&state);
        assert!(matches!(actions[0], AgentAction::NoOp));
    }

    #[test]
    fn test_buy_when_local_cheap() {
        // Reference at 50000, local ask at 49950 (10 bps cheap)
        let feed = MockFeed {
            mid_price: Price::from_int(50000),
        };
        let mut arb = Arbitrageur::new("arb-1", ArbitrageConfig::default(), feed);

        let mut state = MarketState::empty("BTC-USDT");
        state.bbo = BBO {
            bid_price: Price::from_int(49940),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(49950), // Cheap!
            ask_size: Quantity::from_raw(100),
        };

        let actions = arb.on_tick(&state);
        assert!(matches!(
            actions[0],
            AgentAction::SubmitOrder {
                side: trading_core::Side::Buy,
                ..
            }
        ));
    }

    #[test]
    fn test_sell_when_local_expensive() {
        // Reference at 50000, local bid at 50050 (10 bps expensive)
        let feed = MockFeed {
            mid_price: Price::from_int(50000),
        };
        let mut arb = Arbitrageur::new("arb-1", ArbitrageConfig::default(), feed);

        let mut state = MarketState::empty("BTC-USDT");
        state.bbo = BBO {
            bid_price: Price::from_int(50050), // Expensive!
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(50060),
            ask_size: Quantity::from_raw(100),
        };

        let actions = arb.on_tick(&state);
        assert!(matches!(
            actions[0],
            AgentAction::SubmitOrder {
                side: trading_core::Side::Sell,
                ..
            }
        ));
    }

    #[test]
    fn test_position_limits() {
        let feed = MockFeed {
            mid_price: Price::from_int(50000),
        };
        let config = ArbitrageConfig {
            max_position: 100, // Very small limit
            ..Default::default()
        };
        let mut arb = Arbitrageur::new("arb-1", config, feed);

        // Set position at limit
        arb.position = 100;

        // Create buy opportunity
        let mut state = MarketState::empty("BTC-USDT");
        state.bbo = BBO {
            bid_price: Price::from_int(49940),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(49950),
            ask_size: Quantity::from_raw(100),
        };

        // Should not buy because at position limit
        let actions = arb.on_tick(&state);
        assert!(matches!(actions[0], AgentAction::NoOp));
    }
}
