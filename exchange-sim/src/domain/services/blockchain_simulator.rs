//! Blockchain network simulator for crypto settlement.
//!
//! Simulates blockchain behavior including:
//! - Multiple networks (Bitcoin, Ethereum, etc.)
//! - Transaction submission and mempool
//! - Block production and confirmations
//! - Network fees and congestion
//! - Deposit address management

use crate::domain::entities::Network;
use crate::domain::value_objects::{Timestamp, Value};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// BLOCKCHAIN TRANSACTION
// ============================================================================

/// Unique identifier for blockchain transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxId(Uuid);

impl TxId {
    pub fn new() -> Self {
        TxId(Uuid::new_v4())
    }
}

impl Default for TxId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", &self.0.to_string().replace("-", "")[..16])
    }
}

/// Status of a blockchain transaction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxStatus {
    /// Transaction is in mempool waiting to be mined
    Pending,
    /// Transaction has been included in a block
    Confirmed { confirmations: u32 },
    /// Transaction has reached finality
    Finalized,
    /// Transaction failed (reverted, dropped, etc.)
    Failed,
}

/// A blockchain transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainTx {
    pub id: TxId,
    pub network: Network,
    pub from_address: String,
    pub to_address: String,
    pub asset: String,
    pub amount: Value,
    pub fee: Value,
    pub status: TxStatus,
    pub submitted_at: Timestamp,
    pub confirmed_at: Option<Timestamp>,
    pub block_number: Option<u64>,
}

impl BlockchainTx {
    /// Create a new pending transaction
    pub fn new(
        network: Network,
        from_address: impl Into<String>,
        to_address: impl Into<String>,
        asset: impl Into<String>,
        amount: Value,
        fee: Value,
        now: Timestamp,
    ) -> Self {
        Self {
            id: TxId::new(),
            network,
            from_address: from_address.into(),
            to_address: to_address.into(),
            asset: asset.into(),
            amount,
            fee,
            status: TxStatus::Pending,
            submitted_at: now,
            confirmed_at: None,
            block_number: None,
        }
    }

    /// Check if transaction has enough confirmations
    pub fn has_confirmations(&self, required: u32) -> bool {
        match self.status {
            TxStatus::Confirmed { confirmations } => confirmations >= required,
            TxStatus::Finalized => true,
            _ => false,
        }
    }

    /// Check if transaction is pending
    pub fn is_pending(&self) -> bool {
        matches!(self.status, TxStatus::Pending)
    }

    /// Check if transaction has failed
    pub fn is_failed(&self) -> bool {
        matches!(self.status, TxStatus::Failed)
    }
}

// ============================================================================
// DEPOSIT ADDRESS
// ============================================================================

/// A deposit address for receiving funds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepositAddress {
    pub address: String,
    pub network: Network,
    pub owner_id: String,
    pub asset: Option<String>, // None = accepts any asset on network
    pub created_at: Timestamp,
    pub label: Option<String>,
}

impl DepositAddress {
    pub fn new(
        network: Network,
        owner_id: impl Into<String>,
        asset: Option<String>,
        now: Timestamp,
    ) -> Self {
        let address = generate_address(&network);
        Self {
            address,
            network,
            owner_id: owner_id.into(),
            asset,
            created_at: now,
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Generate a simulated blockchain address for a network
fn generate_address(network: &Network) -> String {
    // UUID without dashes is 32 chars, we need to combine two for longer addresses
    let id1 = Uuid::new_v4().to_string().replace("-", "");
    let id2 = Uuid::new_v4().to_string().replace("-", "");
    let combined = format!("{}{}", id1, id2); // 64 chars

    match network {
        Network::Bitcoin => format!("bc1q{}", &combined[..32]),
        Network::Ethereum => format!("0x{}", &combined[..40]),
        Network::Bsc => format!("0x{}", &combined[..40]),
        Network::Polygon => format!("0x{}", &combined[..40]),
        Network::Arbitrum => format!("0x{}", &combined[..40]),
        Network::Solana => combined[..44].to_string(),
        Network::Internal => format!("internal:{}", &id1[..16]),
        Network::Custom(name) => format!("{}:{}", name, &id1),
    }
}

// ============================================================================
// NETWORK CONFIGURATION
// ============================================================================

/// Configuration for a blockchain network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub network: Network,
    /// Average block time in seconds
    pub block_time_secs: u64,
    /// Number of confirmations for finality
    pub confirmations_required: u32,
    /// Base fee per transaction (in native token)
    pub base_fee: Value,
    /// Current network congestion (0-100, where 100 = fully congested)
    pub congestion_pct: i32,
    /// Maximum transactions per block
    pub max_tx_per_block: u32,
}

impl NetworkConfig {
    pub fn bitcoin() -> Self {
        Self {
            network: Network::Bitcoin,
            block_time_secs: 600, // 10 minutes
            confirmations_required: 6,
            base_fee: Value::from_raw(1000), // 0.00001 BTC (1000 = 0.00001 with 8 decimal places)
            congestion_pct: 30,
            max_tx_per_block: 2500,
        }
    }

    pub fn ethereum() -> Self {
        Self {
            network: Network::Ethereum,
            block_time_secs: 12,
            confirmations_required: 12,
            base_fee: Value::from_raw(10), // 1 gwei = 0.0000000001 (very small)
            congestion_pct: 50,
            max_tx_per_block: 500,
        }
    }

    pub fn solana() -> Self {
        Self {
            network: Network::Solana,
            block_time_secs: 1, // ~400ms slots
            confirmations_required: 32,
            base_fee: Value::from_raw(500), // 0.000005 SOL
            congestion_pct: 20,
            max_tx_per_block: 10000,
        }
    }

    pub fn bsc() -> Self {
        Self {
            network: Network::Bsc,
            block_time_secs: 3,
            confirmations_required: 15,
            base_fee: Value::from_raw(50), // 5 gwei
            congestion_pct: 40,
            max_tx_per_block: 1000,
        }
    }

    pub fn polygon() -> Self {
        Self {
            network: Network::Polygon,
            block_time_secs: 2,
            confirmations_required: 20,
            base_fee: Value::from_raw(300), // 30 gwei
            congestion_pct: 30,
            max_tx_per_block: 500,
        }
    }

    pub fn arbitrum() -> Self {
        Self {
            network: Network::Arbitrum,
            block_time_secs: 1,
            confirmations_required: 20,
            base_fee: Value::from_raw(1), // 0.1 gwei
            congestion_pct: 30,
            max_tx_per_block: 2000,
        }
    }

    /// Calculate estimated fee based on congestion
    pub fn estimate_fee(&self) -> Value {
        // Fee increases with congestion: base_fee * (1 + congestion * 2)
        // congestion is 0-100, so multiply: base * (100 + congestion * 2) / 100
        let multiplier = 100 + self.congestion_pct * 2;
        Value::from_raw(self.base_fee.raw() * multiplier as i128 / 100)
    }

    /// Estimate time to confirmation in seconds
    pub fn estimate_confirmation_time(&self) -> u64 {
        self.block_time_secs * self.confirmations_required as u64
    }
}

// ============================================================================
// BLOCKCHAIN STATE
// ============================================================================

/// State of a simulated blockchain network
#[derive(Debug, Clone)]
pub struct BlockchainState {
    pub config: NetworkConfig,
    pub current_block: u64,
    pub last_block_time: Timestamp,
    /// Transactions in mempool
    pub mempool: Vec<BlockchainTx>,
    /// Confirmed transactions (by tx_id)
    pub confirmed_txs: HashMap<TxId, BlockchainTx>,
}

impl BlockchainState {
    pub fn new(config: NetworkConfig, now: Timestamp) -> Self {
        Self {
            config,
            current_block: 0,
            last_block_time: now,
            mempool: Vec::new(),
            confirmed_txs: HashMap::new(),
        }
    }

    /// Add a transaction to the mempool
    pub fn submit_tx(&mut self, tx: BlockchainTx) -> TxId {
        let id = tx.id;
        self.mempool.push(tx);
        id
    }

    /// Process a new block (advance time)
    pub fn produce_block(&mut self, now: Timestamp) {
        self.current_block += 1;
        self.last_block_time = now;

        // Move transactions from mempool to confirmed
        // In a real sim, would consider gas prices and block limits
        let max_txs = self.config.max_tx_per_block as usize;
        let to_confirm: Vec<_> = self
            .mempool
            .drain(..max_txs.min(self.mempool.len()))
            .collect();

        for mut tx in to_confirm {
            tx.status = TxStatus::Confirmed { confirmations: 1 };
            tx.confirmed_at = Some(now);
            tx.block_number = Some(self.current_block);
            self.confirmed_txs.insert(tx.id, tx);
        }

        // Increment confirmations for already-confirmed transactions
        for tx in self.confirmed_txs.values_mut() {
            if let TxStatus::Confirmed { confirmations } = &mut tx.status {
                *confirmations += 1;
                if *confirmations >= self.config.confirmations_required {
                    tx.status = TxStatus::Finalized;
                }
            }
        }
    }

    /// Get a transaction by ID
    pub fn get_tx(&self, id: &TxId) -> Option<&BlockchainTx> {
        // Check confirmed first, then mempool
        self.confirmed_txs
            .get(id)
            .or_else(|| self.mempool.iter().find(|tx| tx.id == *id))
    }

    /// Get all finalized transactions to an address
    pub fn get_finalized_deposits(&self, address: &str) -> Vec<&BlockchainTx> {
        self.confirmed_txs
            .values()
            .filter(|tx| tx.to_address == address && matches!(tx.status, TxStatus::Finalized))
            .collect()
    }
}

// ============================================================================
// BLOCKCHAIN SIMULATOR
// ============================================================================

/// Multi-network blockchain simulator
///
/// Manages multiple blockchain networks, allowing for:
/// - Cross-network deposits and withdrawals
/// - Transaction tracking with confirmations
/// - Network-specific behavior (block times, fees)
#[derive(Debug)]
pub struct BlockchainSimulator {
    networks: HashMap<Network, BlockchainState>,
    deposit_addresses: HashMap<String, DepositAddress>,
}

impl BlockchainSimulator {
    /// Create a new blockchain simulator with default networks
    pub fn new(now: Timestamp) -> Self {
        let mut networks = HashMap::new();

        // Add default networks
        networks.insert(
            Network::Bitcoin,
            BlockchainState::new(NetworkConfig::bitcoin(), now),
        );
        networks.insert(
            Network::Ethereum,
            BlockchainState::new(NetworkConfig::ethereum(), now),
        );
        networks.insert(
            Network::Solana,
            BlockchainState::new(NetworkConfig::solana(), now),
        );
        networks.insert(
            Network::Bsc,
            BlockchainState::new(NetworkConfig::bsc(), now),
        );
        networks.insert(
            Network::Polygon,
            BlockchainState::new(NetworkConfig::polygon(), now),
        );
        networks.insert(
            Network::Arbitrum,
            BlockchainState::new(NetworkConfig::arbitrum(), now),
        );

        Self {
            networks,
            deposit_addresses: HashMap::new(),
        }
    }

    /// Create an empty simulator (no networks)
    pub fn empty() -> Self {
        Self {
            networks: HashMap::new(),
            deposit_addresses: HashMap::new(),
        }
    }

    /// Add a network to the simulator
    pub fn add_network(&mut self, config: NetworkConfig, now: Timestamp) {
        let network = config.network.clone();
        self.networks
            .insert(network, BlockchainState::new(config, now));
    }

    /// Generate a new deposit address for an owner
    pub fn create_deposit_address(
        &mut self,
        network: Network,
        owner_id: impl Into<String>,
        asset: Option<String>,
        now: Timestamp,
    ) -> Result<DepositAddress, BlockchainError> {
        if !self.networks.contains_key(&network) {
            return Err(BlockchainError::UnsupportedNetwork(format!(
                "{:?}",
                network
            )));
        }

        let addr = DepositAddress::new(network, owner_id, asset, now);
        let address = addr.address.clone();
        self.deposit_addresses.insert(address, addr.clone());
        Ok(addr)
    }

    /// Submit a transaction to a network
    pub fn submit_transaction(
        &mut self,
        network: &Network,
        from_address: impl Into<String>,
        to_address: impl Into<String>,
        asset: impl Into<String>,
        amount: Value,
        now: Timestamp,
    ) -> Result<TxId, BlockchainError> {
        let state = self
            .networks
            .get_mut(network)
            .ok_or_else(|| BlockchainError::UnsupportedNetwork(format!("{:?}", network)))?;

        let fee = state.config.estimate_fee();
        let tx = BlockchainTx::new(
            network.clone(),
            from_address,
            to_address,
            asset,
            amount,
            fee,
            now,
        );

        Ok(state.submit_tx(tx))
    }

    /// Advance time and produce blocks on all networks as needed
    pub fn advance_time(&mut self, now: Timestamp) {
        for state in self.networks.values_mut() {
            // Check if enough time has passed to produce a block
            let elapsed_ms = now.timestamp_millis() - state.last_block_time.timestamp_millis();
            let elapsed_secs = elapsed_ms / 1000;
            let blocks_to_produce = elapsed_secs as u64 / state.config.block_time_secs;

            for _ in 0..blocks_to_produce {
                state.produce_block(now);
            }
        }
    }

    /// Get transaction status
    pub fn get_transaction(&self, network: &Network, tx_id: &TxId) -> Option<&BlockchainTx> {
        self.networks.get(network)?.get_tx(tx_id)
    }

    /// Get all finalized deposits for a deposit address
    pub fn get_deposits(&self, address: &str) -> Vec<&BlockchainTx> {
        let Some(deposit_addr) = self.deposit_addresses.get(address) else {
            return Vec::new();
        };
        let Some(state) = self.networks.get(&deposit_addr.network) else {
            return Vec::new();
        };
        state.get_finalized_deposits(address)
    }

    /// Get deposit address info
    pub fn get_deposit_address(&self, address: &str) -> Option<&DepositAddress> {
        self.deposit_addresses.get(address)
    }

    /// Get network state
    pub fn get_network(&self, network: &Network) -> Option<&BlockchainState> {
        self.networks.get(network)
    }

    /// Get mutable network state
    pub fn get_network_mut(&mut self, network: &Network) -> Option<&mut BlockchainState> {
        self.networks.get_mut(network)
    }

    /// Check if a network is supported
    pub fn supports_network(&self, network: &Network) -> bool {
        self.networks.contains_key(network)
    }

    /// Get all supported networks
    pub fn supported_networks(&self) -> Vec<&Network> {
        self.networks.keys().collect()
    }

    /// Estimate transaction fee for a network
    pub fn estimate_fee(&self, network: &Network) -> Option<Value> {
        self.networks.get(network).map(|s| s.config.estimate_fee())
    }

    /// Estimate confirmation time for a network (in seconds)
    pub fn estimate_confirmation_time(&self, network: &Network) -> Option<u64> {
        self.networks
            .get(network)
            .map(|s| s.config.estimate_confirmation_time())
    }
}

impl Default for BlockchainSimulator {
    fn default() -> Self {
        Self::new(Utc::now())
    }
}

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockchainError {
    UnsupportedNetwork(String),
    InsufficientBalance,
    InvalidAddress,
    TransactionFailed(String),
}

impl std::fmt::Display for BlockchainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockchainError::UnsupportedNetwork(n) => write!(f, "Unsupported network: {}", n),
            BlockchainError::InsufficientBalance => write!(f, "Insufficient balance"),
            BlockchainError::InvalidAddress => write!(f, "Invalid address"),
            BlockchainError::TransactionFailed(msg) => write!(f, "Transaction failed: {}", msg),
        }
    }
}

impl std::error::Error for BlockchainError {}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn test_timestamp() -> Timestamp {
        Utc.timestamp_millis_opt(1700000000000).unwrap() // Nov 2023
    }

    fn timestamp_plus_millis(base: Timestamp, millis: i64) -> Timestamp {
        base + chrono::Duration::milliseconds(millis)
    }

    #[test]
    fn test_create_deposit_address() {
        let mut sim = BlockchainSimulator::new(test_timestamp());

        let addr = sim
            .create_deposit_address(
                Network::Ethereum,
                "exchange1",
                Some("USDT".to_string()),
                test_timestamp(),
            )
            .unwrap();

        assert!(addr.address.starts_with("0x"));
        assert_eq!(addr.network, Network::Ethereum);
        assert_eq!(addr.owner_id, "exchange1");
        assert_eq!(addr.asset, Some("USDT".to_string()));
    }

    #[test]
    fn test_submit_transaction() {
        let mut sim = BlockchainSimulator::new(test_timestamp());

        let tx_id = sim
            .submit_transaction(
                &Network::Ethereum,
                "0xfrom",
                "0xto",
                "ETH",
                Value::from_int(1),
                test_timestamp(),
            )
            .unwrap();

        let tx = sim.get_transaction(&Network::Ethereum, &tx_id).unwrap();
        assert!(tx.is_pending());
        assert_eq!(tx.amount, Value::from_int(1));
    }

    #[test]
    fn test_block_production() {
        let now = test_timestamp();
        let mut sim = BlockchainSimulator::new(now);

        // Submit transaction
        let tx_id = sim
            .submit_transaction(
                &Network::Ethereum,
                "0xfrom",
                "0xto",
                "ETH",
                Value::from_int(1),
                now,
            )
            .unwrap();

        // Advance time by 1 block (12 seconds for Ethereum)
        let later = timestamp_plus_millis(now, 12000);
        sim.advance_time(later);

        // Transaction should now be confirmed
        let tx = sim.get_transaction(&Network::Ethereum, &tx_id).unwrap();
        assert!(!tx.is_pending());
        assert!(matches!(tx.status, TxStatus::Confirmed { .. }));
    }

    #[test]
    fn test_finalization() {
        let now = test_timestamp();
        let mut sim = BlockchainSimulator::new(now);

        let tx_id = sim
            .submit_transaction(
                &Network::Ethereum,
                "0xfrom",
                "0xto",
                "ETH",
                Value::from_int(1),
                now,
            )
            .unwrap();

        // Advance time by enough for finalization (12 blocks * 12 seconds = 144 seconds)
        let later = timestamp_plus_millis(now, 150_000);
        sim.advance_time(later);

        let tx = sim.get_transaction(&Network::Ethereum, &tx_id).unwrap();
        assert!(matches!(tx.status, TxStatus::Finalized));
    }

    #[test]
    fn test_multiple_networks() {
        let sim = BlockchainSimulator::new(test_timestamp());

        assert!(sim.supports_network(&Network::Bitcoin));
        assert!(sim.supports_network(&Network::Ethereum));
        assert!(sim.supports_network(&Network::Solana));
        assert!(!sim.supports_network(&Network::Internal));
    }

    #[test]
    fn test_fee_estimation() {
        let sim = BlockchainSimulator::new(test_timestamp());

        let eth_fee = sim.estimate_fee(&Network::Ethereum).unwrap();
        let btc_fee = sim.estimate_fee(&Network::Bitcoin).unwrap();

        // Fees should be positive
        assert!(eth_fee.raw() > 0);
        assert!(btc_fee.raw() > 0);
    }

    #[test]
    fn test_confirmation_time_estimation() {
        let sim = BlockchainSimulator::new(test_timestamp());

        // Bitcoin: 10 min * 6 confirmations = 60 min = 3600 sec
        let btc_time = sim.estimate_confirmation_time(&Network::Bitcoin).unwrap();
        assert_eq!(btc_time, 3600);

        // Ethereum: 12 sec * 12 confirmations = 144 sec
        let eth_time = sim.estimate_confirmation_time(&Network::Ethereum).unwrap();
        assert_eq!(eth_time, 144);
    }

    #[test]
    fn test_tx_id_display() {
        let tx_id = TxId::new();
        let display = format!("{}", tx_id);
        assert!(display.starts_with("0x"));
        assert_eq!(display.len(), 18); // "0x" + 16 chars
    }

    #[test]
    fn test_address_generation() {
        let btc_addr = generate_address(&Network::Bitcoin);
        assert!(btc_addr.starts_with("bc1q"));

        let eth_addr = generate_address(&Network::Ethereum);
        assert!(eth_addr.starts_with("0x"));
        assert_eq!(eth_addr.len(), 42);

        let sol_addr = generate_address(&Network::Solana);
        assert_eq!(sol_addr.len(), 44);
    }
}
