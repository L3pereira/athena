use crate::domain::entities::WithdrawalStatusEvent;
use serde::{Deserialize, Serialize};

// Re-export event types from trading-core
pub use trading_core::events::{
    DepthSnapshotEvent, DepthUpdateEvent, OrderAcceptedEvent, OrderCanceledEvent,
    OrderExpiredEvent, OrderFilledEvent, OrderRejectedEvent, TradeExecutedEvent,
};

// Re-export event types from use cases for convenience
pub use crate::application::use_cases::{
    DepositCreditedEvent, LiquidityAddedEvent, LiquidityRemovedEvent, SwapExecutedEvent,
};

/// Domain events emitted by the exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "camelCase")]
pub enum ExchangeEvent {
    /// Order was accepted and added to the book
    OrderAccepted(OrderAcceptedEvent),
    /// Order was rejected
    OrderRejected(OrderRejectedEvent),
    /// Order was partially filled
    OrderPartiallyFilled(OrderFilledEvent),
    /// Order was completely filled
    OrderFilled(OrderFilledEvent),
    /// Order was canceled
    OrderCanceled(OrderCanceledEvent),
    /// Order expired due to time in force
    OrderExpired(OrderExpiredEvent),
    /// Trade occurred
    TradeExecuted(TradeExecutedEvent),
    /// Order book depth update (delta)
    DepthUpdate(DepthUpdateEvent),
    /// Full order book snapshot
    DepthSnapshot(DepthSnapshotEvent),
    /// Withdrawal status changed
    WithdrawalStatus(WithdrawalStatusEvent),
    /// DEX swap executed
    SwapExecuted(SwapExecutedEvent),
    /// Liquidity added to pool
    LiquidityAdded(LiquidityAddedEvent),
    /// Liquidity removed from pool
    LiquidityRemoved(LiquidityRemovedEvent),
    /// Deposit credited to account
    DepositCredited(DepositCreditedEvent),
}
