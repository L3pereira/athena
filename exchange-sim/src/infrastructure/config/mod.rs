//! Configuration loading for exchange simulator
//!
//! Supports JSON configuration files for:
//! - Markets (spot, futures, options)
//! - Trader accounts with initial deposits
//! - Seed orders for initial liquidity
//! - Exchange settings (rate limits, etc.)

use crate::domain::{
    ExerciseStyle, FuturesConfig, InstrumentType, OptionConfig, OptionType, Price, Quantity, Side,
    Symbol, TimeInForce, TradingPairConfig,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Root configuration for the exchange simulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatorConfig {
    /// Exchange name/identifier
    #[serde(default = "default_exchange_name")]
    pub name: String,

    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limits: RateLimitConfigDto,

    /// Markets/instruments to create
    #[serde(default)]
    pub markets: Vec<MarketConfig>,

    /// Trader accounts to create
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,

    /// Initial seed orders for liquidity
    #[serde(default)]
    pub seed_orders: Vec<SeedOrderConfig>,
}

fn default_exchange_name() -> String {
    "Athena Exchange Simulator".to_string()
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            name: default_exchange_name(),
            server: ServerConfig::default(),
            rate_limits: RateLimitConfigDto::default(),
            markets: Vec::new(),
            accounts: Vec::new(),
            seed_orders: Vec::new(),
        }
    }
}

impl SimulatorConfig {
    /// Load configuration from a JSON file
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path.as_ref()).map_err(|e| ConfigError::Io {
            path: path.as_ref().display().to_string(),
            error: e.to_string(),
        })?;

        Self::from_json(&content)
    }

    /// Parse configuration from JSON string
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        serde_json::from_str(json).map_err(|e| ConfigError::Parse(e.to_string()))
    }

    /// Create with default markets (spot pairs)
    pub fn with_default_spot_markets() -> Self {
        let markets = vec![
            MarketConfig::spot("BTCUSDT", "BTC", "USDT"),
            MarketConfig::spot("ETHUSDT", "ETH", "USDT"),
            MarketConfig::spot("BNBUSDT", "BNB", "USDT"),
            MarketConfig::spot("SOLUSDT", "SOL", "USDT"),
            MarketConfig::spot("XRPUSDT", "XRP", "USDT"),
        ];

        Self {
            markets,
            ..Default::default()
        }
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_event_capacity")]
    pub event_capacity: usize,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_event_capacity() -> usize {
    10000
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            event_capacity: default_event_capacity(),
        }
    }
}

/// Rate limit configuration (DTO for JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfigDto {
    #[serde(default = "default_requests_per_minute")]
    pub requests_per_minute: u32,
    #[serde(default = "default_orders_per_second")]
    pub orders_per_second: u32,
    #[serde(default = "default_orders_per_day")]
    pub orders_per_day: u32,
    #[serde(default = "default_request_weight_per_minute")]
    pub request_weight_per_minute: u32,
    #[serde(default = "default_ws_connections_per_ip")]
    pub ws_connections_per_ip: u32,
    #[serde(default = "default_ws_messages_per_second")]
    pub ws_messages_per_second: u32,
}

fn default_requests_per_minute() -> u32 {
    1200
}
fn default_orders_per_second() -> u32 {
    10
}
fn default_orders_per_day() -> u32 {
    200_000
}
fn default_request_weight_per_minute() -> u32 {
    1200
}
fn default_ws_connections_per_ip() -> u32 {
    5
}
fn default_ws_messages_per_second() -> u32 {
    5
}

impl Default for RateLimitConfigDto {
    fn default() -> Self {
        Self {
            requests_per_minute: default_requests_per_minute(),
            orders_per_second: default_orders_per_second(),
            orders_per_day: default_orders_per_day(),
            request_weight_per_minute: default_request_weight_per_minute(),
            ws_connections_per_ip: default_ws_connections_per_ip(),
            ws_messages_per_second: default_ws_messages_per_second(),
        }
    }
}

/// Market/Instrument configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketConfig {
    /// Symbol (e.g., "BTCUSDT", "BTC-PERP", "BTC-27DEC24-50000-C")
    pub symbol: String,
    /// Base asset (e.g., "BTC")
    pub base_asset: String,
    /// Quote asset (e.g., "USDT")
    pub quote_asset: String,
    /// Instrument type
    #[serde(default)]
    pub instrument_type: InstrumentType,
    /// Tick size (price increment)
    #[serde(default)]
    pub tick_size: Option<Decimal>,
    /// Lot size (quantity increment)
    #[serde(default)]
    pub lot_size: Option<Decimal>,
    /// Maker fee in basis points (e.g., 10 = 0.10%)
    #[serde(default)]
    pub maker_fee_bps: Option<i32>,
    /// Taker fee in basis points
    #[serde(default)]
    pub taker_fee_bps: Option<i32>,
    /// Futures-specific configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub futures: Option<FuturesConfigDto>,
    /// Options-specific configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<OptionConfigDto>,
}

impl MarketConfig {
    /// Create a spot market config
    pub fn spot(symbol: &str, base: &str, quote: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            instrument_type: InstrumentType::Spot,
            tick_size: None,
            lot_size: None,
            maker_fee_bps: None,
            taker_fee_bps: None,
            futures: None,
            option: None,
        }
    }

    /// Create a perpetual futures config
    pub fn perpetual(symbol: &str, base: &str, quote: &str) -> Self {
        Self {
            symbol: symbol.to_string(),
            base_asset: base.to_string(),
            quote_asset: quote.to_string(),
            instrument_type: InstrumentType::PerpetualFutures,
            tick_size: None,
            lot_size: None,
            maker_fee_bps: None,
            taker_fee_bps: None,
            futures: Some(FuturesConfigDto::default()),
            option: None,
        }
    }

    /// Convert to domain TradingPairConfig
    pub fn to_trading_pair_config(&self) -> Result<TradingPairConfig, ConfigError> {
        let symbol = Symbol::new(&self.symbol)
            .map_err(|e| ConfigError::InvalidMarket(format!("Invalid symbol: {}", e)))?;

        let mut config = TradingPairConfig::with_type(
            self.instrument_type,
            symbol,
            &self.base_asset,
            &self.quote_asset,
        );

        // Apply optional overrides
        if let Some(tick) = self.tick_size {
            config = config.with_tick_size(Price::from(tick));
        }
        if let Some(lot) = self.lot_size {
            config = config.with_lot_size(Quantity::from(lot));
        }
        if let Some(maker_bps) = self.maker_fee_bps {
            config = config.with_maker_fee(Decimal::new(maker_bps.into(), 4));
        }
        if let Some(taker_bps) = self.taker_fee_bps {
            config = config.with_taker_fee(Decimal::new(taker_bps.into(), 4));
        }

        // Apply futures config
        if let Some(futures_dto) = &self.futures {
            config = config.with_futures_config(futures_dto.to_domain());
        }

        // Apply option config
        if let Some(option_dto) = &self.option {
            config = config.with_option_config(option_dto.to_domain());
        }

        Ok(config)
    }
}

/// Futures configuration DTO for JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesConfigDto {
    /// Expiration timestamp (Unix millis) - None for perpetuals
    #[serde(default)]
    pub expiration_ms: Option<i64>,
    /// Contract multiplier
    #[serde(default = "default_contract_multiplier")]
    pub contract_multiplier: Decimal,
    /// Settlement asset
    #[serde(default = "default_settlement_asset")]
    pub settlement_asset: String,
    /// Maximum leverage
    #[serde(default = "default_max_leverage")]
    pub max_leverage: u32,
    /// Maintenance margin rate
    #[serde(default = "default_maintenance_margin")]
    pub maintenance_margin_rate: Decimal,
    /// Initial margin rate
    #[serde(default = "default_initial_margin")]
    pub initial_margin_rate: Decimal,
    /// Funding interval in hours (for perpetuals)
    #[serde(default)]
    pub funding_interval_hours: Option<u32>,
}

fn default_contract_multiplier() -> Decimal {
    Decimal::ONE
}
fn default_settlement_asset() -> String {
    "USDT".to_string()
}
fn default_max_leverage() -> u32 {
    125
}
fn default_maintenance_margin() -> Decimal {
    Decimal::new(4, 3) // 0.4%
}
fn default_initial_margin() -> Decimal {
    Decimal::new(1, 2) // 1%
}

impl Default for FuturesConfigDto {
    fn default() -> Self {
        Self {
            expiration_ms: None,
            contract_multiplier: default_contract_multiplier(),
            settlement_asset: default_settlement_asset(),
            max_leverage: default_max_leverage(),
            maintenance_margin_rate: default_maintenance_margin(),
            initial_margin_rate: default_initial_margin(),
            funding_interval_hours: Some(8),
        }
    }
}

impl FuturesConfigDto {
    pub fn to_domain(&self) -> FuturesConfig {
        FuturesConfig {
            expiration_ms: self.expiration_ms,
            contract_multiplier: self.contract_multiplier,
            settlement_asset: self.settlement_asset.clone(),
            max_leverage: self.max_leverage,
            maintenance_margin_rate: self.maintenance_margin_rate,
            initial_margin_rate: self.initial_margin_rate,
            funding_interval_hours: self.funding_interval_hours,
        }
    }
}

/// Option configuration DTO for JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionConfigDto {
    /// Strike price
    pub strike: Decimal,
    /// Option type (CALL or PUT)
    pub option_type: OptionType,
    /// Expiration timestamp (Unix millis)
    pub expiration_ms: i64,
    /// Exercise style (EUROPEAN or AMERICAN)
    #[serde(default)]
    pub exercise_style: ExerciseStyle,
}

impl OptionConfigDto {
    pub fn to_domain(&self) -> OptionConfig {
        OptionConfig {
            strike: Price::from(self.strike),
            option_type: self.option_type,
            expiration_ms: self.expiration_ms,
            exercise_style: self.exercise_style,
        }
    }
}

/// Trader account configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    /// Owner identifier (API key)
    pub owner_id: String,
    /// Initial deposits
    #[serde(default)]
    pub deposits: Vec<DepositConfig>,
    /// Fee tier (0-9)
    #[serde(default)]
    pub fee_tier: Option<u8>,
}

/// Deposit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositConfig {
    /// Asset to deposit
    pub asset: String,
    /// Amount to deposit
    pub amount: Decimal,
}

/// Seed order configuration for initial liquidity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedOrderConfig {
    /// Symbol to place order on
    pub symbol: String,
    /// Owner of the order
    pub owner_id: String,
    /// Order side
    pub side: Side,
    /// Limit price
    pub price: Decimal,
    /// Quantity
    pub quantity: Decimal,
    /// Time in force (defaults to GTC)
    #[serde(default)]
    pub time_in_force: Option<TimeInForce>,
}

/// Configuration errors
#[derive(Debug, Clone)]
pub enum ConfigError {
    Io { path: String, error: String },
    Parse(String),
    InvalidMarket(String),
    InvalidAccount(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io { path, error } => {
                write!(f, "Failed to read config file '{}': {}", path, error)
            }
            ConfigError::Parse(e) => write!(f, "Failed to parse config: {}", e),
            ConfigError::InvalidMarket(e) => write!(f, "Invalid market config: {}", e),
            ConfigError::InvalidAccount(e) => write!(f, "Invalid account config: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config() {
        let json = r#"{}"#;
        let config = SimulatorConfig::from_json(json).unwrap();
        assert_eq!(config.name, "Athena Exchange Simulator");
        assert!(config.markets.is_empty());
    }

    #[test]
    fn test_parse_spot_market() {
        let json = r#"{
            "markets": [
                {
                    "symbol": "BTCUSDT",
                    "base_asset": "BTC",
                    "quote_asset": "USDT"
                }
            ]
        }"#;

        let config = SimulatorConfig::from_json(json).unwrap();
        assert_eq!(config.markets.len(), 1);
        assert_eq!(config.markets[0].instrument_type, InstrumentType::Spot);

        let trading_pair = config.markets[0].to_trading_pair_config().unwrap();
        assert_eq!(trading_pair.symbol.as_str(), "BTCUSDT");
    }

    #[test]
    fn test_parse_perpetual_futures() {
        let json = r#"{
            "markets": [
                {
                    "symbol": "BTCUSDT-PERP",
                    "base_asset": "BTC",
                    "quote_asset": "USDT",
                    "instrument_type": "PERPETUAL_FUTURES",
                    "futures": {
                        "max_leverage": 100,
                        "funding_interval_hours": 8
                    }
                }
            ]
        }"#;

        let config = SimulatorConfig::from_json(json).unwrap();
        let trading_pair = config.markets[0].to_trading_pair_config().unwrap();

        assert_eq!(
            trading_pair.instrument_type,
            InstrumentType::PerpetualFutures
        );
        assert!(trading_pair.futures_config.is_some());
        assert_eq!(
            trading_pair.futures_config.as_ref().unwrap().max_leverage,
            100
        );
    }

    #[test]
    fn test_parse_option() {
        let json = r#"{
            "markets": [
                {
                    "symbol": "BTC-27DEC24-50000-C",
                    "base_asset": "BTC",
                    "quote_asset": "USDT",
                    "instrument_type": "OPTION",
                    "option": {
                        "strike": "50000",
                        "option_type": "CALL",
                        "expiration_ms": 1735257600000,
                        "exercise_style": "EUROPEAN"
                    }
                }
            ]
        }"#;

        let config = SimulatorConfig::from_json(json).unwrap();
        let trading_pair = config.markets[0].to_trading_pair_config().unwrap();

        assert_eq!(trading_pair.instrument_type, InstrumentType::Option);
        assert!(trading_pair.option_config.is_some());
    }

    #[test]
    fn test_parse_full_config() {
        let json = r#"{
            "name": "Test Exchange",
            "server": {
                "host": "127.0.0.1",
                "port": 9000
            },
            "rate_limits": {
                "requests_per_minute": 600
            },
            "markets": [
                {
                    "symbol": "BTCUSDT",
                    "base_asset": "BTC",
                    "quote_asset": "USDT",
                    "maker_fee_bps": 5,
                    "taker_fee_bps": 10
                }
            ],
            "accounts": [
                {
                    "owner_id": "market-maker-1",
                    "deposits": [
                        { "asset": "USDT", "amount": "1000000" },
                        { "asset": "BTC", "amount": "100" }
                    ],
                    "fee_tier": 0
                }
            ],
            "seed_orders": [
                {
                    "symbol": "BTCUSDT",
                    "owner_id": "market-maker-1",
                    "side": "BUY",
                    "price": "99000",
                    "quantity": "10"
                }
            ]
        }"#;

        let config = SimulatorConfig::from_json(json).unwrap();
        assert_eq!(config.name, "Test Exchange");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.rate_limits.requests_per_minute, 600);
        assert_eq!(config.markets.len(), 1);
        assert_eq!(config.accounts.len(), 1);
        assert_eq!(config.seed_orders.len(), 1);
    }
}
