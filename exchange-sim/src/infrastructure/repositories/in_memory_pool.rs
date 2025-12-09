//! In-memory liquidity pool repository implementation

use crate::application::ports::{LpPositionReader, LpPositionWriter, PoolReader, PoolWriter};
use crate::domain::{AccountId, LiquidityPool, LpPosition, PoolId};
use async_trait::async_trait;
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::sync::Arc;

/// In-memory pool repository
///
/// Thread-safe storage for liquidity pools and LP positions using DashMap.
pub struct InMemoryPoolRepository {
    pools: Arc<DashMap<PoolId, LiquidityPool>>,
    /// Index: "token_a-token_b" -> PoolId (normalized, alphabetically sorted)
    token_index: Arc<DashMap<String, PoolId>>,
    /// LP positions: (PoolId, AccountId) -> LpPosition
    positions: Arc<DashMap<(PoolId, AccountId), LpPosition>>,
}

impl InMemoryPoolRepository {
    pub fn new() -> Self {
        Self {
            pools: Arc::new(DashMap::new()),
            token_index: Arc::new(DashMap::new()),
            positions: Arc::new(DashMap::new()),
        }
    }

    /// Normalize token pair key (alphabetically sorted)
    fn token_key(token_a: &str, token_b: &str) -> String {
        if token_a < token_b {
            format!("{}-{}", token_a, token_b)
        } else {
            format!("{}-{}", token_b, token_a)
        }
    }
}

impl Default for InMemoryPoolRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for InMemoryPoolRepository {
    fn clone(&self) -> Self {
        Self {
            pools: Arc::clone(&self.pools),
            token_index: Arc::clone(&self.token_index),
            positions: Arc::clone(&self.positions),
        }
    }
}

#[async_trait]
impl PoolReader for InMemoryPoolRepository {
    async fn get(&self, id: &PoolId) -> Option<LiquidityPool> {
        self.pools.get(id).map(|p| p.value().clone())
    }

    async fn get_by_tokens(&self, token_a: &str, token_b: &str) -> Option<LiquidityPool> {
        let key = Self::token_key(token_a, token_b);
        let pool_id = self.token_index.get(&key)?;
        self.pools.get(pool_id.value()).map(|p| p.value().clone())
    }

    async fn get_active(&self) -> Vec<LiquidityPool> {
        self.pools
            .iter()
            .filter(|p| p.active)
            .map(|p| p.value().clone())
            .collect()
    }

    async fn get_by_token(&self, token: &str) -> Vec<LiquidityPool> {
        self.pools
            .iter()
            .filter(|p| p.token_a == token || p.token_b == token)
            .map(|p| p.value().clone())
            .collect()
    }
}

#[async_trait]
impl PoolWriter for InMemoryPoolRepository {
    async fn save(&self, pool: LiquidityPool) {
        let key = Self::token_key(&pool.token_a, &pool.token_b);
        self.token_index.insert(key, pool.id);
        self.pools.insert(pool.id, pool);
    }

    async fn upsert(&self, pool: LiquidityPool) {
        self.save(pool).await;
    }
}

#[async_trait]
impl LpPositionReader for InMemoryPoolRepository {
    async fn get_position(&self, pool_id: &PoolId, account_id: &AccountId) -> Option<LpPosition> {
        self.positions
            .get(&(*pool_id, *account_id))
            .map(|p| p.value().clone())
    }

    async fn get_positions_by_account(&self, account_id: &AccountId) -> Vec<LpPosition> {
        self.positions
            .iter()
            .filter(|p| &p.key().1 == account_id)
            .map(|p| p.value().clone())
            .collect()
    }

    async fn get_positions_by_pool(&self, pool_id: &PoolId) -> Vec<LpPosition> {
        self.positions
            .iter()
            .filter(|p| &p.key().0 == pool_id)
            .map(|p| p.value().clone())
            .collect()
    }
}

#[async_trait]
impl LpPositionWriter for InMemoryPoolRepository {
    async fn save_position(&self, position: LpPosition) {
        self.positions
            .insert((position.pool_id, position.account_id), position);
    }

    async fn update_position_tokens(
        &self,
        pool_id: &PoolId,
        account_id: &AccountId,
        lp_tokens: Decimal,
    ) {
        if let Some(mut pos) = self.positions.get_mut(&(*pool_id, *account_id)) {
            pos.lp_tokens = lp_tokens;
        }
    }

    async fn delete_position(&self, pool_id: &PoolId, account_id: &AccountId) -> bool {
        self.positions.remove(&(*pool_id, *account_id)).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_create_and_get_pool() {
        let repo = InMemoryPoolRepository::new();

        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.reserve_a = dec!(10000);
        pool.reserve_b = dec!(1);
        let id = pool.id;

        repo.save(pool).await;

        let retrieved = repo.get(&id).await.unwrap();
        assert_eq!(retrieved.token_a, "USDT");
        assert_eq!(retrieved.reserve_a, dec!(10000));
    }

    #[tokio::test]
    async fn test_get_by_tokens() {
        let repo = InMemoryPoolRepository::new();

        let pool = LiquidityPool::new("USDT", "BTC");
        let id = pool.id;

        repo.save(pool).await;

        // Should work regardless of order
        let by_tokens1 = repo.get_by_tokens("USDT", "BTC").await.unwrap();
        assert_eq!(by_tokens1.id, id);

        let by_tokens2 = repo.get_by_tokens("BTC", "USDT").await.unwrap();
        assert_eq!(by_tokens2.id, id);
    }

    #[tokio::test]
    async fn test_lp_positions() {
        let repo = InMemoryPoolRepository::new();

        let pool = LiquidityPool::new("USDT", "ETH");
        let pool_id = pool.id;
        repo.save(pool).await;

        let account_id = Uuid::new_v4();
        let position = LpPosition::new(pool_id, account_id, dec!(100), dec!(0.001));

        repo.save_position(position).await;

        let retrieved = repo.get_position(&pool_id, &account_id).await.unwrap();
        assert_eq!(retrieved.lp_tokens, dec!(100));

        // Update tokens
        repo.update_position_tokens(&pool_id, &account_id, dec!(150))
            .await;
        let updated = repo.get_position(&pool_id, &account_id).await.unwrap();
        assert_eq!(updated.lp_tokens, dec!(150));
    }

    #[tokio::test]
    async fn test_get_by_token() {
        let repo = InMemoryPoolRepository::new();

        let pool1 = LiquidityPool::new("USDT", "BTC");
        let pool2 = LiquidityPool::new("USDT", "ETH");
        let pool3 = LiquidityPool::new("BTC", "ETH");

        repo.save(pool1).await;
        repo.save(pool2).await;
        repo.save(pool3).await;

        let usdt_pools = repo.get_by_token("USDT").await;
        assert_eq!(usdt_pools.len(), 2);

        let btc_pools = repo.get_by_token("BTC").await;
        assert_eq!(btc_pools.len(), 2);

        let eth_pools = repo.get_by_token("ETH").await;
        assert_eq!(eth_pools.len(), 2);
    }
}
