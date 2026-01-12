//! Informed Trader Agent (Toxic Flow)
//!
//! Profits from information advantage (knows reference will move).
//! Role: Creates adverse selection for DMM, forces spread widening.
//!
//! Strategy:
//! - Has latency advantage or prediction of reference price movement
//! - Buys aggressively before reference moves up
//! - Sells aggressively before reference moves down

use super::{Agent, AgentAction, AgentId, Fill, MarketEvent, MarketState};
use risk_management::{PnL, ReferenceFeed};
use std::collections::VecDeque;
use trading_core::Quantity;

/// Configuration for informed trader
#[derive(Debug, Clone)]
pub struct InformedTraderConfig {
    /// Lookback for reference momentum
    pub lookback: usize,
    /// Minimum reference movement (bps) to trigger trade
    pub signal_threshold_bps: f64,
    /// Order size (raw units)
    pub order_size: i64,
    /// Maximum position
    pub max_position: i64,
    /// Whether to trade aggressively (market orders)
    pub aggressive: bool,
}

impl Default for InformedTraderConfig {
    fn default() -> Self {
        Self {
            lookback: 5, // Short lookback - fast reaction
            signal_threshold_bps: 5.0,
            order_size: 200_000_000,     // 2 units - trades big
            max_position: 1_000_000_000, // 10 units
            aggressive: true,
        }
    }
}

/// Informed trader agent
pub struct InformedTrader<F: ReferenceFeed> {
    id: AgentId,
    config: InformedTraderConfig,
    reference_feed: F,
    pnl: PnL,
    position: i64,
    ref_price_history: VecDeque<i64>,
    next_order_id: u64,
}

impl<F: ReferenceFeed> InformedTrader<F> {
    pub fn new(id: impl Into<String>, config: InformedTraderConfig, reference_feed: F) -> Self {
        Self {
            id: AgentId::new(id),
            config,
            reference_feed,
            pnl: PnL::new(),
            position: 0,
            ref_price_history: VecDeque::with_capacity(20),
            next_order_id: 1,
        }
    }

    fn next_client_order_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Calculate reference price momentum
    /// Simulates having information advantage about reference direction
    fn reference_signal(&self) -> Option<f64> {
        if self.ref_price_history.len() < self.config.lookback {
            return None;
        }

        let current = *self.ref_price_history.back()?;
        let old = self.ref_price_history[self.ref_price_history.len() - self.config.lookback];

        if old == 0 {
            return None;
        }

        let change_bps = ((current - old) as f64 / old as f64) * 10_000.0;
        Some(change_bps)
    }
}

impl<F: ReferenceFeed + Send + Sync> Agent for InformedTrader<F> {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn on_tick(&mut self, state: &MarketState) -> Vec<AgentAction> {
        if !state.has_quotes() {
            return vec![AgentAction::NoOp];
        }

        // Update reference price history
        let ref_price = self.reference_feed.mid_price().raw();
        self.ref_price_history.push_back(ref_price);
        if self.ref_price_history.len() > self.config.lookback * 2 {
            self.ref_price_history.pop_front();
        }

        // Get signal from reference
        let Some(signal) = self.reference_signal() else {
            return vec![AgentAction::NoOp];
        };

        let quantity = Quantity::from_raw(self.config.order_size);
        let client_order_id = self.next_client_order_id();

        // Reference trending up - buy local (front-run the move)
        if signal > self.config.signal_threshold_bps && self.position < self.config.max_position {
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

        // Reference trending down - sell local (front-run the move)
        if signal < -self.config.signal_threshold_bps && self.position > -self.config.max_position {
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

        vec![AgentAction::NoOp]
    }

    fn on_fill(&mut self, fill: &Fill) {
        self.pnl.record_trade(fill.signed_qty, fill.price, fill.fee);
        self.position += fill.signed_qty;
    }

    fn on_event(&mut self, _event: &MarketEvent) {}

    fn pnl(&self) -> &PnL {
        &self.pnl
    }

    fn agent_type(&self) -> &'static str {
        "InformedTrader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::BBO;
    use parking_lot::RwLock;
    use risk_management::OrderbookMoments;
    use std::sync::Arc;
    use trading_core::Price;

    /// Mock feed with controllable price
    struct ControllableFeed {
        price: Arc<RwLock<i64>>,
    }

    impl ReferenceFeed for ControllableFeed {
        fn moments(&self) -> OrderbookMoments {
            OrderbookMoments::default()
        }

        fn mid_price(&self) -> Price {
            Price::from_raw(*self.price.read())
        }
    }

    fn create_state() -> MarketState {
        let mut state = MarketState::empty("BTC-USDT");
        state.bbo = BBO {
            bid_price: Price::from_int(49990),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_int(50010),
            ask_size: Quantity::from_raw(100),
        };
        state
    }

    #[test]
    fn test_needs_history() {
        let feed = ControllableFeed {
            price: Arc::new(RwLock::new(50000_00000000)),
        };
        let config = InformedTraderConfig {
            lookback: 5,
            ..Default::default()
        };
        let mut trader = InformedTrader::new("informed-1", config, feed);

        for _ in 0..4 {
            let actions = trader.on_tick(&create_state());
            assert!(matches!(actions[0], AgentAction::NoOp));
        }
    }

    #[test]
    fn test_buys_on_reference_uptick() {
        let price = Arc::new(RwLock::new(50000_00000000i64));
        let feed = ControllableFeed {
            price: price.clone(),
        };
        let config = InformedTraderConfig {
            lookback: 3,
            signal_threshold_bps: 5.0,
            ..Default::default()
        };
        let mut trader = InformedTrader::new("informed-1", config, feed);

        let state = create_state();

        // Build history with upward reference movement
        // Need enough movement to exceed 5 bps threshold over lookback of 3
        for i in 0..4 {
            *price.write() = 50000_00000000 + (i as i64 * 15_00000000); // +15 per tick
            trader.on_tick(&state);
        }

        // Should trigger buy on upward reference momentum
        *price.write() = 50060_00000000;
        let actions = trader.on_tick(&state);

        assert!(matches!(
            actions[0],
            AgentAction::SubmitOrder {
                side: trading_core::Side::Buy,
                ..
            }
        ));
    }

    #[test]
    fn test_sells_on_reference_downtick() {
        let price = Arc::new(RwLock::new(50000_00000000i64));
        let feed = ControllableFeed {
            price: price.clone(),
        };
        let config = InformedTraderConfig {
            lookback: 3,
            signal_threshold_bps: 5.0,
            ..Default::default()
        };
        let mut trader = InformedTrader::new("informed-1", config, feed);

        let state = create_state();

        // Build history with downward reference movement
        // Need enough movement to exceed 5 bps threshold over lookback of 3
        for i in 0..4 {
            *price.write() = 50000_00000000 - (i as i64 * 15_00000000); // -15 per tick
            trader.on_tick(&state);
        }

        // Should trigger sell on downward reference momentum
        *price.write() = 49940_00000000;
        let actions = trader.on_tick(&state);

        assert!(matches!(
            actions[0],
            AgentAction::SubmitOrder {
                side: trading_core::Side::Sell,
                ..
            }
        ));
    }
}
