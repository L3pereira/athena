//! Momentum Trader Agent
//!
//! Profits from trend continuation.
//! Role: Amplifies trends, creates feedback loops.
//!
//! Strategy:
//! - Track price changes over a lookback window
//! - If trending up strongly, buy (bet trend continues)
//! - If trending down strongly, sell

use super::{Agent, AgentAction, AgentId, Fill, MarketEvent, MarketState};
use risk_management::PnL;
use std::collections::VecDeque;
use trading_core::Quantity;

/// Configuration for momentum trader
#[derive(Debug, Clone)]
pub struct MomentumConfig {
    /// Lookback window (number of ticks)
    pub lookback: usize,
    /// Minimum price change (in bps) to trigger trade
    pub threshold_bps: f64,
    /// Order size (raw units)
    pub order_size: i64,
    /// Maximum position
    pub max_position: i64,
}

impl Default for MomentumConfig {
    fn default() -> Self {
        Self {
            lookback: 20,
            threshold_bps: 10.0,       // 10 bps move triggers trade
            order_size: 50_000_000,    // 0.5 units
            max_position: 200_000_000, // 2 units max
        }
    }
}

/// Momentum trader agent
pub struct MomentumTrader {
    id: AgentId,
    config: MomentumConfig,
    pnl: PnL,
    position: i64,
    price_history: VecDeque<i64>,
    next_order_id: u64,
}

impl MomentumTrader {
    pub fn new(id: impl Into<String>, config: MomentumConfig) -> Self {
        Self {
            id: AgentId::new(id),
            config,
            pnl: PnL::new(),
            position: 0,
            price_history: VecDeque::with_capacity(100),
            next_order_id: 1,
        }
    }

    fn next_client_order_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Calculate momentum signal
    /// Returns price change in bps over lookback period
    fn momentum_signal(&self) -> Option<f64> {
        if self.price_history.len() < self.config.lookback {
            return None;
        }

        let current = *self.price_history.back()?;
        let old = self.price_history[self.price_history.len() - self.config.lookback];

        if old == 0 {
            return None;
        }

        let change_bps = ((current - old) as f64 / old as f64) * 10_000.0;
        Some(change_bps)
    }
}

impl Agent for MomentumTrader {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn on_tick(&mut self, state: &MarketState) -> Vec<AgentAction> {
        if !state.has_quotes() {
            return vec![AgentAction::NoOp];
        }

        // Update price history
        let mid_price = state.mid_price().raw();
        self.price_history.push_back(mid_price);
        if self.price_history.len() > self.config.lookback * 2 {
            self.price_history.pop_front();
        }

        // Calculate momentum
        let Some(momentum) = self.momentum_signal() else {
            return vec![AgentAction::NoOp];
        };

        let quantity = Quantity::from_raw(self.config.order_size);
        let client_order_id = self.next_client_order_id();

        // Strong upward momentum - buy
        if momentum > self.config.threshold_bps && self.position < self.config.max_position {
            return vec![AgentAction::market_buy(client_order_id, quantity)];
        }

        // Strong downward momentum - sell
        if momentum < -self.config.threshold_bps && self.position > -self.config.max_position {
            return vec![AgentAction::market_sell(client_order_id, quantity)];
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
        "MomentumTrader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::BBO;
    use trading_core::Price;

    fn create_state_at_price(price: i64) -> MarketState {
        let mut state = MarketState::empty("BTC-USDT");
        let offset = price / 200; // 0.5% spread
        state.bbo = BBO {
            bid_price: Price::from_raw(price - offset),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_raw(price + offset),
            ask_size: Quantity::from_raw(100),
        };
        state
    }

    #[test]
    fn test_needs_history_first() {
        let config = MomentumConfig {
            lookback: 5,
            ..Default::default()
        };
        let mut trader = MomentumTrader::new("mom-1", config);

        // Not enough history
        for _ in 0..4 {
            let state = create_state_at_price(50000_00000000);
            let actions = trader.on_tick(&state);
            assert!(matches!(actions[0], AgentAction::NoOp));
        }
    }

    #[test]
    fn test_buys_on_uptrend() {
        let config = MomentumConfig {
            lookback: 5,
            threshold_bps: 10.0,
            ..Default::default()
        };
        let mut trader = MomentumTrader::new("mom-1", config);

        // Build upward trend
        let base_price = 50000_00000000i64;
        for i in 0..5 {
            let price = base_price + (i as i64 * base_price / 1000); // 0.1% per tick = 50 bps total
            let state = create_state_at_price(price);
            trader.on_tick(&state);
        }

        // Next tick should trigger buy
        let final_price = base_price + (5 * base_price / 1000);
        let state = create_state_at_price(final_price);
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
    fn test_sells_on_downtrend() {
        let config = MomentumConfig {
            lookback: 5,
            threshold_bps: 10.0,
            ..Default::default()
        };
        let mut trader = MomentumTrader::new("mom-1", config);

        // Build downward trend
        let base_price = 50000_00000000i64;
        for i in 0..6 {
            let price = base_price - (i as i64 * base_price / 1000); // -0.1% per tick
            let state = create_state_at_price(price);
            trader.on_tick(&state);
        }

        // Should be selling on downtrend
        let final_state = create_state_at_price(base_price - (6 * base_price / 1000));
        let actions = trader.on_tick(&final_state);

        assert!(matches!(
            actions[0],
            AgentAction::SubmitOrder {
                side: trading_core::Side::Sell,
                ..
            }
        ));
    }

    #[test]
    fn test_no_trade_on_flat() {
        let config = MomentumConfig {
            lookback: 5,
            threshold_bps: 10.0,
            ..Default::default()
        };
        let mut trader = MomentumTrader::new("mom-1", config);

        // Flat prices
        let price = 50000_00000000i64;
        for _ in 0..10 {
            let state = create_state_at_price(price);
            let actions = trader.on_tick(&state);
            // After initial history, should be NoOp
            if trader.price_history.len() >= 5 {
                assert!(matches!(actions[0], AgentAction::NoOp));
            }
        }
    }
}
