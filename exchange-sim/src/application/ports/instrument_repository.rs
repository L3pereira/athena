use crate::domain::Symbol;
use crate::domain::entities::TradingPairConfig;
use async_trait::async_trait;

/// Repository for managing trading pair configurations
#[async_trait]
pub trait InstrumentRepository: Send + Sync {
    /// Get a trading pair config by symbol
    async fn get(&self, symbol: &Symbol) -> Option<TradingPairConfig>;

    /// Get all trading pair configs
    async fn get_all(&self) -> Vec<TradingPairConfig>;

    /// Save or update a trading pair config
    async fn save(&self, config: TradingPairConfig);

    /// Check if a symbol exists
    async fn exists(&self, symbol: &Symbol) -> bool;

    /// Get trading pair configs filtered by status (trading, halted, etc.)
    async fn get_trading(&self) -> Vec<TradingPairConfig>;
}
