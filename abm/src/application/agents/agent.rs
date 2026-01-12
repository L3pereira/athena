//! Agent Trait
//!
//! Core trait that all profit-seeking agents must implement.

use super::{AgentAction, MarketState};
use risk_management::PnL;

/// Unique identifier for an agent
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Fill notification for an agent's order
#[derive(Debug, Clone)]
pub struct Fill {
    /// Order ID that was filled
    pub order_id: u64,
    /// Signed quantity (positive = bought, negative = sold)
    pub signed_qty: i64,
    /// Execution price
    pub price: trading_core::Price,
    /// Fee paid
    pub fee: i64,
    /// Timestamp
    pub timestamp_ms: u64,
}

/// Market event notification
#[derive(Debug, Clone)]
pub enum MarketEvent {
    /// Trade executed (any trade, not just ours)
    Trade {
        price: trading_core::Price,
        quantity: trading_core::Quantity,
        is_buyer_maker: bool,
        timestamp_ms: u64,
    },
    /// Orderbook update
    DepthUpdate {
        bids: Vec<(trading_core::Price, trading_core::Quantity)>,
        asks: Vec<(trading_core::Price, trading_core::Quantity)>,
        timestamp_ms: u64,
    },
    /// Our order was accepted
    OrderAccepted { order_id: u64 },
    /// Our order was rejected
    OrderRejected { order_id: u64, reason: String },
    /// Our order was canceled
    OrderCanceled { order_id: u64 },
}

/// Core trait for all agents
///
/// Agents are profit-seeking entities that observe market state and make trading decisions.
/// Regime dynamics emerge from their interactions - they don't broadcast regime, they respond
/// to profit opportunities.
pub trait Agent: Send + Sync {
    /// Get agent's unique identifier
    fn id(&self) -> &AgentId;

    /// Called each tick with current market state
    ///
    /// Returns a list of actions to execute (submit orders, cancel orders, etc.)
    fn on_tick(&mut self, state: &MarketState) -> Vec<AgentAction>;

    /// Called when one of our orders is filled
    fn on_fill(&mut self, fill: &Fill);

    /// Called on market events (trades, depth updates, etc.)
    fn on_event(&mut self, event: &MarketEvent);

    /// Get current P&L
    fn pnl(&self) -> &PnL;

    /// Get agent type name (for logging/metrics)
    fn agent_type(&self) -> &'static str;
}
