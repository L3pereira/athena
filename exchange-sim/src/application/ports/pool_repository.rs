//! Port for liquidity pool repository operations
//!
//! Follows Interface Segregation Principle with focused traits.

use async_trait::async_trait;

use crate::domain::{AccountId, LiquidityPool, LpPosition, PoolId};

/// Read operations for liquidity pools
#[async_trait]
pub trait PoolReader: Send + Sync {
    /// Get a pool by ID
    async fn get(&self, id: &PoolId) -> Option<LiquidityPool>;

    /// Get pool by token pair (order independent)
    async fn get_by_tokens(&self, token_a: &str, token_b: &str) -> Option<LiquidityPool>;

    /// Get all active pools
    async fn get_active(&self) -> Vec<LiquidityPool>;

    /// Get pools containing a specific token
    async fn get_by_token(&self, token: &str) -> Vec<LiquidityPool>;
}

/// Write operations for liquidity pools
#[async_trait]
pub trait PoolWriter: Send + Sync {
    /// Save a pool
    async fn save(&self, pool: LiquidityPool);

    /// Create or update a pool
    async fn upsert(&self, pool: LiquidityPool);
}

/// Read operations for LP positions
#[async_trait]
pub trait LpPositionReader: Send + Sync {
    /// Get LP position by pool and account
    async fn get_position(&self, pool_id: &PoolId, account_id: &AccountId) -> Option<LpPosition>;

    /// Get all positions for an account
    async fn get_positions_by_account(&self, account_id: &AccountId) -> Vec<LpPosition>;

    /// Get all positions for a pool
    async fn get_positions_by_pool(&self, pool_id: &PoolId) -> Vec<LpPosition>;
}

/// Write operations for LP positions
#[async_trait]
pub trait LpPositionWriter: Send + Sync {
    /// Save an LP position
    async fn save_position(&self, position: LpPosition);

    /// Update LP tokens for a position
    async fn update_position_tokens(
        &self,
        pool_id: &PoolId,
        account_id: &AccountId,
        lp_tokens: rust_decimal::Decimal,
    );

    /// Delete an LP position
    async fn delete_position(&self, pool_id: &PoolId, account_id: &AccountId) -> bool;
}

/// Combined pool repository trait
#[async_trait]
pub trait PoolRepository: PoolReader + PoolWriter + LpPositionReader + LpPositionWriter {}

// Blanket implementation
impl<T: PoolReader + PoolWriter + LpPositionReader + LpPositionWriter> PoolRepository for T {}
