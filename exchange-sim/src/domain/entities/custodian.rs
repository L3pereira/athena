//! Custodian entity for asset custody and withdrawal management
//!
//! Custodians represent where assets are actually held:
//! - Hot wallets (fast access, lower limits)
//! - Cold wallets (slow access, higher security)
//! - Smart contracts (DEX, on-chain custody)

use crate::domain::value_objects::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::Timestamp;

/// Unique identifier for a custodian
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CustodianId(Uuid);

impl CustodianId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for CustodianId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CustodianId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Type of custodian/wallet
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CustodianType {
    /// Hot wallet - online, fast access, lower security
    #[default]
    HotWallet,
    /// Cold wallet - offline, slow access, higher security
    ColdWallet,
    /// Smart contract - on-chain custody (DEX)
    SmartContract,
    /// Exchange internal - for internal transfers
    ExchangeInternal,
}

impl std::fmt::Display for CustodianType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CustodianType::HotWallet => write!(f, "HOT_WALLET"),
            CustodianType::ColdWallet => write!(f, "COLD_WALLET"),
            CustodianType::SmartContract => write!(f, "SMART_CONTRACT"),
            CustodianType::ExchangeInternal => write!(f, "EXCHANGE_INTERNAL"),
        }
    }
}

/// Network/blockchain for the custodian
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Network {
    /// Ethereum mainnet
    #[default]
    Ethereum,
    /// Bitcoin mainnet
    Bitcoin,
    /// Binance Smart Chain
    Bsc,
    /// Polygon
    Polygon,
    /// Arbitrum
    Arbitrum,
    /// Solana
    Solana,
    /// Internal (off-chain)
    Internal,
    /// Custom network
    Custom(String),
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Network::Ethereum => write!(f, "ETH"),
            Network::Bitcoin => write!(f, "BTC"),
            Network::Bsc => write!(f, "BSC"),
            Network::Polygon => write!(f, "POLYGON"),
            Network::Arbitrum => write!(f, "ARBITRUM"),
            Network::Solana => write!(f, "SOL"),
            Network::Internal => write!(f, "INTERNAL"),
            Network::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Configuration for withdrawal fees and limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithdrawalConfig {
    /// Asset this config applies to
    pub asset: String,
    /// Network for withdrawal
    pub network: Network,
    /// Fixed withdrawal fee
    pub fee: Value,
    /// Minimum withdrawal amount
    pub min_amount: Value,
    /// Maximum withdrawal amount per transaction
    pub max_amount: Value,
    /// Maximum daily withdrawal amount
    pub daily_limit: Value,
    /// Number of confirmations required
    pub confirmations_required: u32,
    /// Estimated processing time in seconds
    pub processing_time_secs: u64,
    /// Whether withdrawals are enabled
    pub enabled: bool,
}

impl WithdrawalConfig {
    pub fn new(asset: impl Into<String>, network: Network) -> Self {
        Self {
            asset: asset.into(),
            network,
            fee: Value::ZERO,
            min_amount: Value::ZERO,
            max_amount: Value::from_int(1_000_000),
            daily_limit: Value::from_int(10_000_000),
            confirmations_required: 1,
            processing_time_secs: 60,
            enabled: true,
        }
    }

    pub fn with_fee(mut self, fee: Value) -> Self {
        self.fee = fee;
        self
    }

    pub fn with_min_amount(mut self, min: Value) -> Self {
        self.min_amount = min;
        self
    }

    pub fn with_max_amount(mut self, max: Value) -> Self {
        self.max_amount = max;
        self
    }

    pub fn with_daily_limit(mut self, limit: Value) -> Self {
        self.daily_limit = limit;
        self
    }

    pub fn with_confirmations(mut self, confirms: u32) -> Self {
        self.confirmations_required = confirms;
        self
    }

    pub fn with_processing_time(mut self, secs: u64) -> Self {
        self.processing_time_secs = secs;
        self
    }

    /// Validate a withdrawal amount
    pub fn validate_amount(&self, amount: Value) -> Result<(), WithdrawalError> {
        if !self.enabled {
            return Err(WithdrawalError::Disabled);
        }
        if amount.raw() < self.min_amount.raw() {
            return Err(WithdrawalError::BelowMinimum {
                amount,
                minimum: self.min_amount,
            });
        }
        if amount.raw() > self.max_amount.raw() {
            return Err(WithdrawalError::ExceedsMaximum {
                amount,
                maximum: self.max_amount,
            });
        }
        Ok(())
    }

    /// Calculate total amount needed (amount + fee)
    pub fn total_required(&self, amount: Value) -> Value {
        Value::from_raw(amount.raw() + self.fee.raw())
    }
}

/// A custodian that holds assets
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Custodian {
    pub id: CustodianId,
    pub name: String,
    pub custodian_type: CustodianType,
    pub network: Network,
    /// Address/identifier (wallet address, contract address, etc.)
    pub address: Option<String>,
    /// Asset balances held by this custodian
    pub balances: HashMap<String, Value>,
    /// Withdrawal configurations per asset
    pub withdrawal_configs: HashMap<String, WithdrawalConfig>,
    /// Whether this custodian is active
    pub active: bool,
    /// Created timestamp
    pub created_at: Timestamp,
}

impl Custodian {
    pub fn new(name: impl Into<String>, custodian_type: CustodianType, network: Network) -> Self {
        Self {
            id: CustodianId::new(),
            name: name.into(),
            custodian_type,
            network,
            address: None,
            balances: HashMap::new(),
            withdrawal_configs: HashMap::new(),
            active: true,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_address(mut self, address: impl Into<String>) -> Self {
        self.address = Some(address.into());
        self
    }

    pub fn with_withdrawal_config(mut self, config: WithdrawalConfig) -> Self {
        self.withdrawal_configs.insert(config.asset.clone(), config);
        self
    }

    /// Get balance for an asset
    pub fn balance(&self, asset: &str) -> Value {
        self.balances.get(asset).copied().unwrap_or(Value::ZERO)
    }

    /// Deposit funds into this custodian
    pub fn deposit(&mut self, asset: &str, amount: Value) {
        let balance = self
            .balances
            .entry(asset.to_string())
            .or_insert(Value::ZERO);
        *balance = Value::from_raw(balance.raw() + amount.raw());
    }

    /// Withdraw funds from this custodian
    pub fn withdraw(&mut self, asset: &str, amount: Value) -> Result<(), WithdrawalError> {
        let balance = self.balance(asset);
        if balance.raw() < amount.raw() {
            return Err(WithdrawalError::InsufficientCustodianBalance {
                available: balance,
                requested: amount,
            });
        }

        let balance = self.balances.get_mut(asset).unwrap();
        *balance = Value::from_raw(balance.raw() - amount.raw());
        Ok(())
    }

    /// Get withdrawal config for an asset
    pub fn withdrawal_config(&self, asset: &str) -> Option<&WithdrawalConfig> {
        self.withdrawal_configs.get(asset)
    }

    /// Check if withdrawals are supported for an asset
    pub fn supports_withdrawal(&self, asset: &str) -> bool {
        self.withdrawal_configs
            .get(asset)
            .map(|c| c.enabled)
            .unwrap_or(false)
    }

    /// Estimated processing time for a withdrawal
    pub fn processing_time(&self, asset: &str) -> u64 {
        self.withdrawal_configs
            .get(asset)
            .map(|c| c.processing_time_secs)
            .unwrap_or(0)
    }
}

/// Errors that can occur during withdrawal
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WithdrawalError {
    /// Withdrawals are disabled for this asset
    Disabled,
    /// Amount below minimum
    BelowMinimum { amount: Value, minimum: Value },
    /// Amount exceeds maximum
    ExceedsMaximum { amount: Value, maximum: Value },
    /// Daily limit exceeded
    DailyLimitExceeded { used: Value, limit: Value },
    /// Insufficient balance in user account
    InsufficientBalance { available: Value, requested: Value },
    /// Insufficient balance in custodian
    InsufficientCustodianBalance { available: Value, requested: Value },
    /// Invalid destination address
    InvalidAddress(String),
    /// Network error
    NetworkError(String),
    /// Custodian not found
    CustodianNotFound,
    /// Asset not supported
    AssetNotSupported(String),
}

impl std::fmt::Display for WithdrawalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WithdrawalError::Disabled => write!(f, "Withdrawals are disabled"),
            WithdrawalError::BelowMinimum { amount, minimum } => {
                write!(
                    f,
                    "Amount {} below minimum {}",
                    amount.to_f64(),
                    minimum.to_f64()
                )
            }
            WithdrawalError::ExceedsMaximum { amount, maximum } => {
                write!(
                    f,
                    "Amount {} exceeds maximum {}",
                    amount.to_f64(),
                    maximum.to_f64()
                )
            }
            WithdrawalError::DailyLimitExceeded { used, limit } => {
                write!(
                    f,
                    "Daily limit exceeded: {} / {}",
                    used.to_f64(),
                    limit.to_f64()
                )
            }
            WithdrawalError::InsufficientBalance {
                available,
                requested,
            } => {
                write!(
                    f,
                    "Insufficient balance: {} available, {} requested",
                    available.to_f64(),
                    requested.to_f64()
                )
            }
            WithdrawalError::InsufficientCustodianBalance {
                available,
                requested,
            } => {
                write!(
                    f,
                    "Insufficient custodian balance: {} available, {} requested",
                    available.to_f64(),
                    requested.to_f64()
                )
            }
            WithdrawalError::InvalidAddress(addr) => write!(f, "Invalid address: {}", addr),
            WithdrawalError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            WithdrawalError::CustodianNotFound => write!(f, "Custodian not found"),
            WithdrawalError::AssetNotSupported(asset) => {
                write!(f, "Asset not supported: {}", asset)
            }
        }
    }
}

impl std::error::Error for WithdrawalError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custodian_deposit_withdraw() {
        let mut custodian =
            Custodian::new("Hot Wallet", CustodianType::HotWallet, Network::Ethereum);

        custodian.deposit("USDT", Value::from_int(10000));
        assert_eq!(custodian.balance("USDT"), Value::from_int(10000));

        custodian.withdraw("USDT", Value::from_int(3000)).unwrap();
        assert_eq!(custodian.balance("USDT"), Value::from_int(7000));
    }

    #[test]
    fn test_custodian_insufficient_balance() {
        let mut custodian =
            Custodian::new("Hot Wallet", CustodianType::HotWallet, Network::Ethereum);

        custodian.deposit("USDT", Value::from_int(1000));
        let result = custodian.withdraw("USDT", Value::from_int(2000));

        assert!(matches!(
            result,
            Err(WithdrawalError::InsufficientCustodianBalance { .. })
        ));
    }

    #[test]
    fn test_withdrawal_config_validation() {
        let config = WithdrawalConfig::new("USDT", Network::Ethereum)
            .with_fee(Value::from_int(5))
            .with_min_amount(Value::from_int(10))
            .with_max_amount(Value::from_int(10000));

        assert!(config.validate_amount(Value::from_int(100)).is_ok());
        assert!(config.validate_amount(Value::from_int(5)).is_err()); // Below minimum
        assert!(config.validate_amount(Value::from_int(20000)).is_err()); // Above maximum

        assert_eq!(
            config.total_required(Value::from_int(100)),
            Value::from_int(105)
        ); // Amount + fee
    }
}
