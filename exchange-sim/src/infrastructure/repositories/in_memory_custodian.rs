//! In-memory custodian repository implementation

use crate::application::ports::{CustodianReader, CustodianWriter};
use crate::domain::{Custodian, CustodianId, Network};
use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;

/// In-memory custodian repository
///
/// Thread-safe storage for custodians using DashMap.
pub struct InMemoryCustodianRepository {
    custodians: Arc<DashMap<CustodianId, Custodian>>,
}

impl InMemoryCustodianRepository {
    pub fn new() -> Self {
        Self {
            custodians: Arc::new(DashMap::new()),
        }
    }

    /// Create with some default custodians
    pub fn with_defaults() -> Self {
        let repo = Self::new();

        // Add default hot wallets for common networks
        let eth_hot = Custodian::new(
            "Ethereum Hot Wallet",
            crate::domain::CustodianType::HotWallet,
            Network::Ethereum,
        )
        .with_address("0x1234...simulation");
        let btc_hot = Custodian::new(
            "Bitcoin Hot Wallet",
            crate::domain::CustodianType::HotWallet,
            Network::Bitcoin,
        )
        .with_address("bc1q...simulation");
        let internal = Custodian::new(
            "Internal Ledger",
            crate::domain::CustodianType::ExchangeInternal,
            Network::Internal,
        );

        repo.custodians.insert(eth_hot.id, eth_hot);
        repo.custodians.insert(btc_hot.id, btc_hot);
        repo.custodians.insert(internal.id, internal);

        repo
    }
}

impl Default for InMemoryCustodianRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryCustodianRepository {
    fn clone(&self) -> Self {
        Self {
            custodians: Arc::clone(&self.custodians),
        }
    }
}

#[async_trait]
impl CustodianReader for InMemoryCustodianRepository {
    async fn get(&self, id: &CustodianId) -> Option<Custodian> {
        self.custodians.get(id).map(|c| c.value().clone())
    }

    async fn get_by_network(&self, network: &Network) -> Vec<Custodian> {
        self.custodians
            .iter()
            .filter(|c| &c.network == network)
            .map(|c| c.value().clone())
            .collect()
    }

    async fn get_active(&self) -> Vec<Custodian> {
        self.custodians
            .iter()
            .filter(|c| c.active)
            .map(|c| c.value().clone())
            .collect()
    }

    async fn get_supporting_asset(&self, asset: &str) -> Vec<Custodian> {
        self.custodians
            .iter()
            .filter(|c| c.supports_withdrawal(asset))
            .map(|c| c.value().clone())
            .collect()
    }
}

#[async_trait]
impl CustodianWriter for InMemoryCustodianRepository {
    async fn save(&self, custodian: Custodian) {
        self.custodians.insert(custodian.id, custodian);
    }

    async fn delete(&self, id: &CustodianId) -> bool {
        self.custodians.remove(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{CustodianType, WithdrawalConfig};

    #[tokio::test]
    async fn test_create_and_get() {
        let repo = InMemoryCustodianRepository::new();

        let custodian = Custodian::new("Test Wallet", CustodianType::HotWallet, Network::Ethereum);
        let id = custodian.id;

        repo.save(custodian).await;

        let retrieved = repo.get(&id).await.unwrap();
        assert_eq!(retrieved.name, "Test Wallet");
    }

    #[tokio::test]
    async fn test_get_by_network() {
        let repo = InMemoryCustodianRepository::new();

        let eth = Custodian::new("ETH Wallet", CustodianType::HotWallet, Network::Ethereum);
        let btc = Custodian::new("BTC Wallet", CustodianType::HotWallet, Network::Bitcoin);

        repo.save(eth).await;
        repo.save(btc).await;

        let eth_wallets = repo.get_by_network(&Network::Ethereum).await;
        assert_eq!(eth_wallets.len(), 1);
        assert_eq!(eth_wallets[0].name, "ETH Wallet");
    }

    #[tokio::test]
    async fn test_get_supporting_asset() {
        use crate::domain::Value;
        let repo = InMemoryCustodianRepository::new();

        let config = WithdrawalConfig::new("USDT", Network::Ethereum)
            .with_fee(Value::from_int(5))
            .with_min_amount(Value::from_int(10));

        let custodian = Custodian::new("USDT Wallet", CustodianType::HotWallet, Network::Ethereum)
            .with_withdrawal_config(config);

        repo.save(custodian).await;

        let supporting = repo.get_supporting_asset("USDT").await;
        assert_eq!(supporting.len(), 1);

        let not_supporting = repo.get_supporting_asset("BTC").await;
        assert_eq!(not_supporting.len(), 0);
    }
}
