//! Noise Trader Agent
//!
//! A random trader that provides baseline volume.
//! Profit source: None (loses on average to spread and informed traders)
//! Role: Creates liquidity, reduces bid-ask bounce, provides counterparty

use super::{Agent, AgentAction, AgentId, Fill, MarketEvent, MarketState};
use rand::prelude::*;
use risk_management::PnL;
use trading_core::Quantity;

/// Configuration for noise trader
#[derive(Debug, Clone)]
pub struct NoiseTraderConfig {
    /// Probability of trading each tick (0-1)
    pub trade_probability: f64,
    /// Order size (raw units)
    pub order_size: i64,
    /// Whether to use market orders (vs limit at touch)
    pub use_market_orders: bool,
    /// Random seed (for reproducibility)
    pub seed: Option<u64>,
}

impl Default for NoiseTraderConfig {
    fn default() -> Self {
        Self {
            trade_probability: 0.1, // 10% chance per tick
            order_size: 10000000,   // 0.1 units
            use_market_orders: true,
            seed: None,
        }
    }
}

/// Noise trader agent
pub struct NoiseTrader {
    id: AgentId,
    config: NoiseTraderConfig,
    pnl: PnL,
    rng: StdRng,
    next_order_id: u64,
}

impl NoiseTrader {
    pub fn new(id: impl Into<String>, config: NoiseTraderConfig) -> Self {
        let rng = match config.seed {
            Some(seed) => StdRng::seed_from_u64(seed),
            None => StdRng::from_entropy(),
        };

        Self {
            id: AgentId::new(id),
            config,
            pnl: PnL::new(),
            rng,
            next_order_id: 1,
        }
    }

    fn next_client_order_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }
}

impl Agent for NoiseTrader {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn on_tick(&mut self, state: &MarketState) -> Vec<AgentAction> {
        // Random chance to trade
        if self.rng.r#gen::<f64>() > self.config.trade_probability {
            return vec![AgentAction::NoOp];
        }

        if !state.has_quotes() {
            return vec![AgentAction::NoOp];
        }

        // Random buy or sell
        let is_buy = self.rng.r#gen::<bool>();
        let quantity = Quantity::from_raw(self.config.order_size);
        let client_order_id = self.next_client_order_id();

        if self.config.use_market_orders {
            if is_buy {
                vec![AgentAction::market_buy(client_order_id, quantity)]
            } else {
                vec![AgentAction::market_sell(client_order_id, quantity)]
            }
        } else {
            // Limit at touch (passive)
            if is_buy {
                vec![AgentAction::limit_buy(
                    client_order_id,
                    state.bbo.bid_price,
                    quantity,
                )]
            } else {
                vec![AgentAction::limit_sell(
                    client_order_id,
                    state.bbo.ask_price,
                    quantity,
                )]
            }
        }
    }

    fn on_fill(&mut self, fill: &Fill) {
        self.pnl.record_trade(fill.signed_qty, fill.price, fill.fee);
    }

    fn on_event(&mut self, _event: &MarketEvent) {
        // Noise trader ignores events
    }

    fn pnl(&self) -> &PnL {
        &self.pnl
    }

    fn agent_type(&self) -> &'static str {
        "NoiseTrader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::BBO;
    use trading_core::Price;

    fn create_test_state() -> MarketState {
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
    fn test_noise_trader_sometimes_trades() {
        let config = NoiseTraderConfig {
            trade_probability: 1.0, // Always trade
            seed: Some(42),
            ..Default::default()
        };

        let mut trader = NoiseTrader::new("noise-1", config);
        let state = create_test_state();

        let actions = trader.on_tick(&state);
        assert!(!actions.is_empty());
        assert!(matches!(actions[0], AgentAction::SubmitOrder { .. }));
    }

    #[test]
    fn test_noise_trader_no_trade_when_probability_zero() {
        let config = NoiseTraderConfig {
            trade_probability: 0.0,
            seed: Some(42),
            ..Default::default()
        };

        let mut trader = NoiseTrader::new("noise-1", config);
        let state = create_test_state();

        for _ in 0..100 {
            let actions = trader.on_tick(&state);
            assert!(matches!(actions[0], AgentAction::NoOp));
        }
    }

    #[test]
    fn test_noise_trader_deterministic() {
        let config = NoiseTraderConfig {
            trade_probability: 0.5,
            seed: Some(12345),
            ..Default::default()
        };

        let mut trader1 = NoiseTrader::new("noise-1", config.clone());
        let mut trader2 = NoiseTrader::new("noise-2", config);
        let state = create_test_state();

        // Both should make same decisions
        for _ in 0..20 {
            let actions1 = trader1.on_tick(&state);
            let actions2 = trader2.on_tick(&state);

            // Compare action types
            assert_eq!(
                std::mem::discriminant(&actions1[0]),
                std::mem::discriminant(&actions2[0])
            );
        }
    }
}
