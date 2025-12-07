use crate::application::ports::InstrumentRepository;
use crate::domain::Symbol;
use crate::domain::entities::{InstrumentStatus, TradingPairConfig};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

/// In-memory instrument repository for trading pair configurations
pub struct InMemoryInstrumentRepository {
    configs: Arc<DashMap<String, TradingPairConfig>>,
}

impl InMemoryInstrumentRepository {
    pub fn new() -> Self {
        InMemoryInstrumentRepository {
            configs: Arc::new(DashMap::new()),
        }
    }

    /// Create with default trading pair configs (for testing)
    pub fn with_defaults() -> Self {
        let repo = Self::new();

        // Add some default trading pairs
        let configs = vec![
            TradingPairConfig::new(Symbol::new("BTCUSDT").unwrap(), "BTC", "USDT"),
            TradingPairConfig::new(Symbol::new("ETHUSDT").unwrap(), "ETH", "USDT"),
            TradingPairConfig::new(Symbol::new("BNBUSDT").unwrap(), "BNB", "USDT"),
            TradingPairConfig::new(Symbol::new("SOLUSDT").unwrap(), "SOL", "USDT"),
            TradingPairConfig::new(Symbol::new("XRPUSDT").unwrap(), "XRP", "USDT"),
        ];

        for config in configs {
            repo.configs.insert(config.symbol.to_string(), config);
        }

        repo
    }

    /// Add a trading pair config
    pub fn add(&self, config: TradingPairConfig) {
        self.configs.insert(config.symbol.to_string(), config);
    }

    /// Get a trading pair config (sync)
    pub fn get(&self, symbol: &Symbol) -> Option<TradingPairConfig> {
        self.configs.get(&symbol.to_string()).map(|i| i.clone())
    }

    /// Get all trading pair configs (sync)
    pub fn all(&self) -> Vec<TradingPairConfig> {
        self.configs
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }
}

impl Default for InMemoryInstrumentRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryInstrumentRepository {
    fn clone(&self) -> Self {
        InMemoryInstrumentRepository {
            configs: Arc::clone(&self.configs),
        }
    }
}

#[async_trait]
impl InstrumentRepository for InMemoryInstrumentRepository {
    async fn get(&self, symbol: &Symbol) -> Option<TradingPairConfig> {
        self.configs.get(&symbol.to_string()).map(|i| i.clone())
    }

    async fn get_all(&self) -> Vec<TradingPairConfig> {
        self.configs
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    async fn save(&self, config: TradingPairConfig) {
        self.configs.insert(config.symbol.to_string(), config);
    }

    async fn exists(&self, symbol: &Symbol) -> bool {
        self.configs.contains_key(&symbol.to_string())
    }

    async fn get_trading(&self) -> Vec<TradingPairConfig> {
        self.configs
            .iter()
            .filter(|entry| entry.value().status == InstrumentStatus::Trading)
            .map(|entry| entry.value().clone())
            .collect()
    }
}
