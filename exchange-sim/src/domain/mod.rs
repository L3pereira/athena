pub mod entities;
pub mod events;
pub mod instruments;
pub mod matching;
pub mod services;
pub mod value_objects;

// Re-export entity types
pub use entities::{
    Account, AccountError, AccountId, AccountStatus, AssetBalance, FeeSchedule, InstrumentStatus,
    Loan, MarginMode, Order, OrderBook, OrderBookSnapshot, OrderStatus, Position, PositionSide,
    PriceLevel, Trade, TradingPairConfig,
};

// Re-export events
pub use events::{
    DepthSnapshotEvent, DepthUpdateEvent, ExchangeEvent, OrderAcceptedEvent, OrderCanceledEvent,
    OrderExpiredEvent, OrderFilledEvent, OrderRejectedEvent, TradeExecutedEvent,
};

// Re-export services
pub use services::{
    AgentTimeView, Clock, ClockSource, ControllableClock, DriftingClock, ExchangeClock,
    ExternalClockAdapter, NetworkSim, NtpSyncEvent, OrderValidator, TimeScale, TimeUpdate,
    WorldClock,
};

// Re-export value objects
pub use value_objects::{
    OrderId, OrderType, Price, Quantity, Side, Symbol, TimeInForce, Timestamp, TradeId,
};

// Re-export matching algorithms
pub use matching::{MatchResult, MatchingAlgorithm, PriceTimeMatcher, ProRataMatcher};

// Re-export instruments
pub use instruments::{
    ExerciseStyle, FutureContract, Instrument, InstrumentSpec, OptionContract, OptionType,
    PerpetualContract, SettlementType, SpotPair,
};
