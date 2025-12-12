pub mod entities;
pub mod events;
pub mod instruments;
pub mod matching;
pub mod services;
pub mod value_objects;

// Re-export entity types
pub use entities::{
    Account, AccountError, AccountId, AccountStatus, AddLiquidityOutput, AddLiquidityResult,
    AmmType, AssetBalance, ClearingMethod, Custodian, CustodianId, CustodianType, FeeSchedule,
    FuturesConfig, InstrumentStatus, InstrumentType, LiquidityPool, Loan, LpPosition, MarginMode,
    Network, OptionConfig, Order, OrderBook, OrderBookSnapshot, OrderStatus, PoolError, PoolId,
    Position, PositionSide, PriceLevel, RemoveLiquidityOutput, RemoveLiquidityResult,
    SettlementCycle, SwapOutput, SwapResult, Trade, TradingPairConfig, WithdrawalConfig,
    WithdrawalError, WithdrawalId, WithdrawalRequest, WithdrawalStatus, WithdrawalStatusEvent,
};

// Re-export events
pub use events::{
    DepthSnapshotEvent, DepthUpdateEvent, ExchangeEvent, OrderAcceptedEvent, OrderCanceledEvent,
    OrderExpiredEvent, OrderFilledEvent, OrderRejectedEvent, TradeExecutedEvent,
};

// Re-export services
pub use services::{
    AccountMarginCalculator, AgentTimeView, BlockchainError, BlockchainSimulator, BlockchainState,
    BlockchainTx, Clock, ClockSource, ControllableClock, DepositAddress, DriftingClock,
    ExchangeClock, ExternalClockAdapter, MarginCalculator, MarginStatus, NetworkConfig, NetworkSim,
    NtpSyncEvent, OrderValidator, StandardMarginCalculator, TimeScale, TimeUpdate, TxId, TxStatus,
    WorldClock,
};

// Re-export value objects
pub use value_objects::{
    BPS_SCALE, OrderId, OrderType, PRICE_SCALE, Price, QUANTITY_SCALE, Quantity, Rate, Side,
    Symbol, TimeInForce, Timestamp, TradeId, Value,
};

// Re-export matching algorithms
pub use matching::{MatchResult, MatchingAlgorithm, PriceTimeMatcher, ProRataMatcher};

// Re-export instruments
pub use instruments::{
    ExerciseStyle, FutureContract, Instrument, InstrumentSpec, OptionContract, OptionType,
    PerpetualContract, SettlementType, SpotPair,
};
