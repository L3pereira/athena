//! Mean Reversion Trader Agent
//!
//! Profits from overreaction correction.
//! Role: Dampens volatility, provides liquidity at extremes.
//!
//! Strategy:
//! - Calculate moving average over lookback window
//! - If price > MA + threshold: Sell (bet on reversion)
//! - If price < MA - threshold: Buy (bet on reversion)

use super::{Agent, AgentAction, AgentId, Fill, MarketEvent, MarketState};
use risk_management::PnL;
use std::collections::VecDeque;
use trading_core::Quantity;

/// Configuration for mean reversion trader
#[derive(Debug, Clone)]
pub struct MeanReversionConfig {
    /// Lookback window for moving average
    pub lookback: usize,
    /// Number of standard deviations to trigger trade
    pub std_dev_threshold: f64,
    /// Order size (raw units)
    pub order_size: i64,
    /// Maximum position
    pub max_position: i64,
    /// Minimum standard deviation to avoid trading in dead markets
    pub min_std_dev_bps: f64,
}

impl Default for MeanReversionConfig {
    fn default() -> Self {
        Self {
            lookback: 50,
            std_dev_threshold: 2.0,    // 2 sigma
            order_size: 50_000_000,    // 0.5 units
            max_position: 200_000_000, // 2 units max
            min_std_dev_bps: 5.0,      // Minimum volatility to trade
        }
    }
}

/// Mean reversion trader agent
pub struct MeanReversionTrader {
    id: AgentId,
    config: MeanReversionConfig,
    pnl: PnL,
    position: i64,
    price_history: VecDeque<i64>,
    next_order_id: u64,
}

impl MeanReversionTrader {
    pub fn new(id: impl Into<String>, config: MeanReversionConfig) -> Self {
        let capacity = config.lookback * 2;
        Self {
            id: AgentId::new(id),
            config,
            pnl: PnL::new(),
            position: 0,
            price_history: VecDeque::with_capacity(capacity),
            next_order_id: 1,
        }
    }

    fn next_client_order_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Calculate mean and standard deviation of price history
    fn stats(&self) -> Option<(f64, f64)> {
        if self.price_history.len() < self.config.lookback {
            return None;
        }

        let recent: Vec<f64> = self
            .price_history
            .iter()
            .rev()
            .take(self.config.lookback)
            .map(|&p| p as f64)
            .collect();

        let mean: f64 = recent.iter().sum::<f64>() / recent.len() as f64;
        let variance: f64 =
            recent.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / recent.len() as f64;
        let std_dev = variance.sqrt();

        Some((mean, std_dev))
    }

    /// Calculate z-score of current price relative to moving average
    fn z_score(&self, current_price: i64) -> Option<f64> {
        let (mean, std_dev) = self.stats()?;

        if std_dev < 1.0 {
            return None; // Avoid division by near-zero
        }

        // Check minimum volatility
        let std_dev_bps = (std_dev / mean) * 10_000.0;
        if std_dev_bps < self.config.min_std_dev_bps {
            return None;
        }

        Some((current_price as f64 - mean) / std_dev)
    }
}

impl Agent for MeanReversionTrader {
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

        // Calculate z-score
        let Some(z) = self.z_score(mid_price) else {
            return vec![AgentAction::NoOp];
        };

        let quantity = Quantity::from_raw(self.config.order_size);
        let client_order_id = self.next_client_order_id();

        // Price too high - sell (expect reversion down)
        if z > self.config.std_dev_threshold && self.position > -self.config.max_position {
            return vec![AgentAction::market_sell(client_order_id, quantity)];
        }

        // Price too low - buy (expect reversion up)
        if z < -self.config.std_dev_threshold && self.position < self.config.max_position {
            return vec![AgentAction::market_buy(client_order_id, quantity)];
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
        "MeanReversionTrader"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::BBO;
    use trading_core::Price;

    fn create_state_at_price(price: i64) -> MarketState {
        let mut state = MarketState::empty("BTC-USDT");
        let offset = price / 200;
        state.bbo = BBO {
            bid_price: Price::from_raw(price - offset),
            bid_size: Quantity::from_raw(100),
            ask_price: Price::from_raw(price + offset),
            ask_size: Quantity::from_raw(100),
        };
        state
    }

    #[test]
    fn test_needs_history() {
        let config = MeanReversionConfig {
            lookback: 10,
            ..Default::default()
        };
        let mut trader = MeanReversionTrader::new("mr-1", config);

        for _ in 0..9 {
            let state = create_state_at_price(50000_00000000);
            let actions = trader.on_tick(&state);
            assert!(matches!(actions[0], AgentAction::NoOp));
        }
    }

    #[test]
    fn test_sells_on_high_z_score() {
        let config = MeanReversionConfig {
            lookback: 10,
            std_dev_threshold: 2.0,
            min_std_dev_bps: 1.0, // Lower for test
            ..Default::default()
        };
        let mut trader = MeanReversionTrader::new("mr-1", config);

        // Build history with some volatility
        let base_price = 50000_00000000i64;
        for i in 0..10 {
            // Oscillate slightly to create std dev
            let price = base_price
                + (if i % 2 == 0 {
                    50_00000000
                } else {
                    -50_00000000
                });
            let state = create_state_at_price(price);
            trader.on_tick(&state);
        }

        // Now spike up - should trigger sell
        let high_price = base_price + 500_00000000; // Much higher
        let state = create_state_at_price(high_price);
        let actions = trader.on_tick(&state);

        // Should sell when price is too high relative to MA
        assert!(matches!(
            actions[0],
            AgentAction::SubmitOrder {
                side: trading_core::Side::Sell,
                ..
            }
        ));
    }

    #[test]
    fn test_buys_on_low_z_score() {
        let config = MeanReversionConfig {
            lookback: 10,
            std_dev_threshold: 2.0,
            min_std_dev_bps: 1.0,
            ..Default::default()
        };
        let mut trader = MeanReversionTrader::new("mr-1", config);

        // Build history with some volatility
        let base_price = 50000_00000000i64;
        for i in 0..10 {
            let price = base_price
                + (if i % 2 == 0 {
                    50_00000000
                } else {
                    -50_00000000
                });
            let state = create_state_at_price(price);
            trader.on_tick(&state);
        }

        // Now spike down - should trigger buy
        let low_price = base_price - 500_00000000;
        let state = create_state_at_price(low_price);
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
    fn test_no_trade_when_flat() {
        let config = MeanReversionConfig {
            lookback: 10,
            std_dev_threshold: 2.0,
            min_std_dev_bps: 10.0, // High minimum volatility
            ..Default::default()
        };
        let mut trader = MeanReversionTrader::new("mr-1", config);

        // Completely flat prices
        let price = 50000_00000000i64;
        for _ in 0..20 {
            let state = create_state_at_price(price);
            let actions = trader.on_tick(&state);
            // Should never trade on flat prices
            if trader.price_history.len() >= 10 {
                assert!(matches!(actions[0], AgentAction::NoOp));
            }
        }
    }
}
