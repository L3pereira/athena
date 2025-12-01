//! Athena Core Domain
//!
//! Pure domain types for the Athena trading system.
//! This crate contains no async, no I/O, and is 100% unit testable.

pub mod entities;
pub mod instruments;
pub mod values;

// Re-export commonly used types at crate root
pub use entities::{
    // Risk/margin types
    AccountStatus,
    // Fee types
    FeeConfig,
    FeeSchedule,
    FeeTier,
    MarginAccount,
    MarginMode,
    // Core trading entities
    Order,
    OrderId,
    OrderStatus,
    OrderType,
    Position,
    PositionSide,
    Side,
    TimeInForce,
    Trade,
    TradeFees,
    TradeId,
};
pub use instruments::{
    FutureContract, Instrument, InstrumentId, InstrumentSpec, OptionContract, OptionType,
    PerpetualContract, SpotPair,
};
pub use values::{Price, Quantity, Symbol, Timestamp};
