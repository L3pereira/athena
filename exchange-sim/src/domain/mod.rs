pub mod entities;
pub mod events;
pub mod instruments;
pub mod matching;
pub mod services;
pub mod value_objects;

// Re-export entity types
pub use entities::{
    Account, AccountError, AccountId, AccountStatus, AddLiquidityOutput, AddLiquidityResult,
    AmmType, AssetBalance, Custodian, CustodianId, CustodianType, FeeSchedule, FuturesConfig,
    InstrumentStatus, InstrumentType, LiquidityPool, Loan, LpPosition, MarginMode, Network,
    OptionConfig, Order, OrderBook, OrderBookSnapshot, OrderStatus, PoolError, PoolId, Position,
    PositionSide, PriceLevel, RemoveLiquidityOutput, RemoveLiquidityResult, SwapOutput, SwapResult,
    Trade, TradingPairConfig, WithdrawalConfig, WithdrawalError, WithdrawalId, WithdrawalRequest,
    WithdrawalStatus, WithdrawalStatusEvent,
};

// Re-export events
pub use events::{
    DepthSnapshotEvent, DepthUpdateEvent, ExchangeEvent, OrderAcceptedEvent, OrderCanceledEvent,
    OrderExpiredEvent, OrderFilledEvent, OrderRejectedEvent, TradeExecutedEvent,
};

// Re-export services
pub use services::{
    AccountMarginCalculator, AgentTimeView, Clock, ClockSource, ControllableClock, DriftingClock,
    ExchangeClock, ExternalClockAdapter, MarginCalculator, MarginStatus, NetworkSim, NtpSyncEvent,
    OrderValidator, StandardMarginCalculator, TimeScale, TimeUpdate, WorldClock,
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
