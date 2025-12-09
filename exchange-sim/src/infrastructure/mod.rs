mod blockchain_adapter;
pub mod clock;
pub mod config;
pub mod event_publisher;
pub mod matching;
pub mod order_book_shard;
pub mod rate_limiter;
pub mod repositories;

pub use blockchain_adapter::{
    BlockchainAdapter, BlockchainAdapterError, InMemoryDepositAddressRegistry,
    InMemoryProcessedDepositTracker,
};
pub use clock::SimulationClock;
pub use config::{
    AccountConfig, ConfigError, CustodianConfig, DepositConfig, FuturesConfigDto, MarketConfig,
    OptionConfigDto, PoolConfig, RateLimitConfigDto, SeedOrderConfig, ServerConfig,
    SimulatorConfig, WithdrawalConfigDto,
};
pub use event_publisher::BroadcastEventPublisher;
pub use matching::PriceTimeMatcher;
pub use order_book_shard::{
    CancelOrderResponse, ConsistentHashStrategy, GetDepthResponse, OrderBookCommand, ShardConfig,
    ShardError, ShardHandle, ShardManagerConfig, ShardStats, ShardedOrderBookManager,
    ShardingStrategy, SubmitOrderResponse,
};
pub use rate_limiter::TokenBucketRateLimiter;
pub use repositories::{
    InMemoryAccountRepository, InMemoryCustodianRepository, InMemoryInstrumentRepository,
    InMemoryOrderBookRepository, InMemoryPoolRepository, InMemoryWithdrawalRepository,
};
