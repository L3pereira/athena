// Re-export domain types from athena-core
// This maintains backwards compatibility during migration
pub use athena_core::{
    // Risk/margin types
    AccountStatus,
    // Fee types
    FeeConfig,
    FeeSchedule,
    FeeTier,
    // Trading entities
    FutureContract,
    Instrument,
    InstrumentId,
    InstrumentSpec,
    MarginAccount,
    MarginMode,
    OptionContract,
    OptionType,
    Order,
    OrderId,
    OrderStatus,
    OrderType,
    PerpetualContract,
    Position,
    PositionSide,
    Price,
    Quantity,
    Side,
    SpotPair,
    Symbol,
    TimeInForce,
    Timestamp,
    Trade,
    TradeFees,
    TradeId,
};

// Re-export risk management
pub use athena_ports::{
    LiquidationOrder, RiskCheckResult, RiskConfig, RiskError, RiskManager, RiskResult,
};
pub use athena_risk::BasicRiskManager;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

/// Messages passed between components of the exchange
/// Note: This stays in exchange-sim as it's application/infrastructure level
#[derive(Debug, Clone)]
pub enum ExchangeMessage {
    /// Submit a new order to the exchange
    SubmitOrder(Order),

    /// Cancel an existing order
    CancelOrder(Uuid),

    /// Notification of order status update
    OrderUpdate {
        order_id: Uuid,
        status: OrderStatus,
        filled_qty: Decimal,
        symbol: String,
    },

    /// Notification of a trade
    Trade(Trade),

    /// Periodic heartbeat for time-based operations
    Heartbeat(DateTime<Utc>),
}
