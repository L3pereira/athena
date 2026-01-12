//! Designated Market Maker Agent
//!
//! A profit-seeking DMM that uses Avellaneda-Stoikov for optimal quoting.
//! Regime emerges from its behavior:
//! - Wide spread = stressed/volatile (protecting from adverse selection)
//! - Skewed quotes = trending (inventory management)
//! - One-sided quotes = crisis (inventory exhausted)

mod config;

pub use config::DMMConfig;

use super::{Agent, AgentAction, AgentId, Fill, MarketEvent, MarketState};
use risk_management::{AvellanedaStoikov, Inventory, PnL, QuotingModel, ReferenceFeed};
use trading_core::{Price, Quantity};

/// Designated Market Maker agent
pub struct DMMAgent<F: ReferenceFeed> {
    id: AgentId,
    config: DMMConfig,
    /// Reference feed for volatility/regime information
    reference_feed: F,
    /// Avellaneda-Stoikov quoting model
    quote_model: AvellanedaStoikov,
    /// Current inventory state
    inventory: Inventory,
    /// P&L tracking
    pnl: PnL,
    /// Active orders (client_order_id -> exchange_order_id)
    active_orders: std::collections::HashMap<u64, u64>,
    /// Next client order ID
    next_order_id: u64,
    /// Last quote prices (for comparison)
    last_bid: Price,
    last_ask: Price,
}

impl<F: ReferenceFeed> DMMAgent<F> {
    /// Create a new DMM agent
    pub fn new(id: impl Into<String>, config: DMMConfig, reference_feed: F) -> Self {
        Self {
            id: AgentId::new(id),
            quote_model: AvellanedaStoikov::with_gamma(config.gamma),
            inventory: Inventory::new(config.max_inventory),
            pnl: PnL::new(),
            reference_feed,
            config,
            active_orders: std::collections::HashMap::new(),
            next_order_id: 1,
            last_bid: Price::from_raw(0),
            last_ask: Price::from_raw(0),
        }
    }

    /// Get the reference feed
    pub fn reference_feed(&self) -> &F {
        &self.reference_feed
    }

    /// Get current inventory
    pub fn inventory(&self) -> &Inventory {
        &self.inventory
    }

    /// Compute optimal quotes based on A-S model
    fn compute_quotes(&self, state: &MarketState) -> Option<(Price, Price, Quantity)> {
        if !state.has_quotes() {
            return None;
        }

        // Get volatility from reference feed
        let ref_moments = self.reference_feed.moments();
        let volatility = ref_moments.mid_volatility.max(0.001); // Floor at 0.1%

        // Calculate time remaining (using configured horizon)
        let time_remaining = chrono::Duration::hours(self.config.time_horizon_hours as i64);

        // Get optimal quote from A-S model
        let quote = self.quote_model.compute_quotes(
            state.mid_price(),
            &self.inventory,
            volatility,
            time_remaining,
        );

        // Check minimum spread profitability
        let spread_bps = quote.spread_bps();
        if spread_bps < self.config.min_spread_bps {
            // Not profitable to quote
            return None;
        }

        // Determine quote size based on inventory
        let base_size = self.config.quote_size;
        let (bid_size, ask_size) = if self.inventory.abs_ratio() > self.config.skew_threshold {
            // Skew sizes to reduce inventory
            if self.inventory.is_long() {
                // Long - want to sell more
                (base_size / 2, base_size)
            } else {
                // Short - want to buy more
                (base_size, base_size / 2)
            }
        } else {
            (base_size, base_size)
        };

        Some((
            quote.bid_price,
            quote.ask_price,
            Quantity::from_raw(bid_size.min(ask_size)),
        ))
    }

    /// Check if we should requote (prices moved significantly)
    fn should_requote(&self, new_bid: Price, new_ask: Price) -> bool {
        if self.last_bid.raw() == 0 || self.last_ask.raw() == 0 {
            return true;
        }

        let bid_change =
            ((new_bid.raw() - self.last_bid.raw()).abs() as f64) / (self.last_bid.raw() as f64);
        let ask_change =
            ((new_ask.raw() - self.last_ask.raw()).abs() as f64) / (self.last_ask.raw() as f64);

        bid_change > self.config.requote_threshold || ask_change > self.config.requote_threshold
    }

    fn next_client_order_id(&mut self) -> u64 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }
}

impl<F: ReferenceFeed + Send + Sync> Agent for DMMAgent<F> {
    fn id(&self) -> &AgentId {
        &self.id
    }

    fn on_tick(&mut self, state: &MarketState) -> Vec<AgentAction> {
        // Check if reference feed indicates stress
        if self.reference_feed.is_stressed() && self.config.pause_on_stress {
            // Cancel all and don't quote
            return vec![AgentAction::CancelAll];
        }

        // Check inventory limits
        if self.inventory.at_limit() {
            // Only quote to reduce inventory
            let Some((bid, ask, size)) = self.compute_quotes(state) else {
                return vec![AgentAction::CancelAll];
            };

            let mut actions = vec![AgentAction::CancelAll];

            if self.inventory.is_long() && self.inventory.can_sell() {
                // Only sell
                actions.push(AgentAction::post_only_sell(
                    self.next_client_order_id(),
                    ask,
                    size,
                ));
            } else if self.inventory.is_short() && self.inventory.can_buy() {
                // Only buy
                actions.push(AgentAction::post_only_buy(
                    self.next_client_order_id(),
                    bid,
                    size,
                ));
            }

            return actions;
        }

        // Normal two-sided quoting
        let Some((bid, ask, size)) = self.compute_quotes(state) else {
            return vec![AgentAction::CancelAll];
        };

        // Check if we need to requote
        if !self.should_requote(bid, ask) {
            return vec![AgentAction::NoOp];
        }

        // Update last quotes
        self.last_bid = bid;
        self.last_ask = ask;

        // Cancel existing and submit new quotes
        vec![
            AgentAction::CancelAll,
            AgentAction::post_only_buy(self.next_client_order_id(), bid, size),
            AgentAction::post_only_sell(self.next_client_order_id(), ask, size),
        ]
    }

    fn on_fill(&mut self, fill: &Fill) {
        // Update P&L
        self.pnl.record_trade(fill.signed_qty, fill.price, fill.fee);

        // Update inventory
        self.inventory.apply_trade(fill.signed_qty);

        // Remove from active orders if fully filled
        self.active_orders.retain(|_, &mut v| v != fill.order_id);
    }

    fn on_event(&mut self, event: &MarketEvent) {
        match event {
            MarketEvent::OrderAccepted { order_id } => {
                // Track the order
                // Note: In a real implementation we'd need to map client_order_id to exchange_order_id
                let _ = order_id;
            }
            MarketEvent::OrderRejected { order_id, reason } => {
                // Log rejection
                let _ = (order_id, reason);
            }
            MarketEvent::OrderCanceled { order_id } => {
                self.active_orders.retain(|_, &mut v| v != *order_id);
            }
            _ => {}
        }
    }

    fn pnl(&self) -> &PnL {
        &self.pnl
    }

    fn agent_type(&self) -> &'static str {
        "DMM"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use risk_management::OrderbookMoments;

    /// Simple mock reference feed for testing
    struct MockFeed {
        moments: OrderbookMoments,
        mid_price: Price,
    }

    impl ReferenceFeed for MockFeed {
        fn moments(&self) -> OrderbookMoments {
            self.moments.clone()
        }

        fn mid_price(&self) -> Price {
            self.mid_price
        }
    }

    fn create_test_dmm() -> DMMAgent<MockFeed> {
        let feed = MockFeed {
            moments: OrderbookMoments {
                spread_bps: 10.0,
                depth_ratio: 0.8,
                imbalance: 0.0,
                mid_volatility: 0.02,
                ..Default::default()
            },
            mid_price: Price::from_int(50000),
        };

        DMMAgent::new("test-dmm", DMMConfig::default(), feed)
    }

    fn create_test_state() -> MarketState {
        use super::super::market_state::BBO;

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
    fn test_dmm_initial_state() {
        let dmm = create_test_dmm();

        assert_eq!(dmm.inventory().position, 0);
        assert_eq!(dmm.pnl().realized(), 0);
    }

    #[test]
    fn test_dmm_quotes_two_sided() {
        let mut dmm = create_test_dmm();
        let state = create_test_state();

        let actions = dmm.on_tick(&state);

        // Should cancel all and submit two new orders
        assert!(actions.len() >= 2);

        let has_cancel = actions.iter().any(|a| matches!(a, AgentAction::CancelAll));
        assert!(has_cancel);

        let order_count = actions
            .iter()
            .filter(|a| matches!(a, AgentAction::SubmitOrder { .. }))
            .count();
        assert_eq!(order_count, 2);
    }

    #[test]
    fn test_dmm_handles_fill() {
        let mut dmm = create_test_dmm();

        let fill = Fill {
            order_id: 1,
            signed_qty: 100, // Bought
            price: Price::from_int(50000),
            fee: 10,
            timestamp_ms: 1000,
        };

        dmm.on_fill(&fill);

        assert_eq!(dmm.inventory().position, 100);
    }
}
