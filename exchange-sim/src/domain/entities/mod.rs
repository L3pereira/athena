mod account;
mod custodian;
mod instrument;
mod liquidity_pool;
mod loan;
mod order_book;
mod position;
mod withdrawal;

pub use account::{
    Account, AccountError, AccountId, AccountStatus, AssetBalance, FeeSchedule, MarginMode,
};
pub use custodian::{
    Custodian, CustodianId, CustodianType, Network, WithdrawalConfig, WithdrawalError,
};
pub use instrument::{
    ClearingMethod, FuturesConfig, InstrumentStatus, InstrumentType, OptionConfig, SettlementCycle,
    TradingPairConfig,
};
pub use liquidity_pool::{
    AddLiquidityOutput, AddLiquidityResult, AmmType, LiquidityPool, LpPosition, PoolError, PoolId,
    RemoveLiquidityOutput, RemoveLiquidityResult, SwapOutput, SwapResult,
};
// Note: ExerciseStyle and OptionType are re-exported from domain::instruments to avoid duplication
pub use loan::Loan;
pub use order_book::{OrderBook, OrderBookSnapshot};
pub use position::{Position, PositionSide};
pub use withdrawal::{WithdrawalId, WithdrawalRequest, WithdrawalStatus, WithdrawalStatusEvent};

// Re-export from trading-core
pub use trading_core::entities::{Order, OrderStatus, PriceLevel, Trade};
