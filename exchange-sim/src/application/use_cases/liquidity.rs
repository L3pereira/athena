//! Liquidity Management Use Cases for DEX AMM
//!
//! Handles adding and removing liquidity from pools.

use crate::application::ports::{
    AccountRepository, EventPublisher, LpPositionReader, LpPositionWriter, PoolReader, PoolWriter,
};
use crate::domain::{
    AddLiquidityResult, Clock, ExchangeEvent, LiquidityPool, LpPosition, PoolError, PoolId,
    RemoveLiquidityResult,
};
use rust_decimal::Decimal;
use std::sync::Arc;

/// Command to add liquidity to a pool
#[derive(Debug, Clone)]
pub struct AddLiquidityCommand {
    /// First token
    pub token_a: String,
    /// Second token
    pub token_b: String,
    /// Amount of token_a to add
    pub amount_a: Decimal,
    /// Amount of token_b to add
    pub amount_b: Decimal,
    /// Minimum LP tokens to receive (slippage protection)
    pub min_lp_tokens: Decimal,
}

/// Command to remove liquidity from a pool
#[derive(Debug, Clone)]
pub struct RemoveLiquidityCommand {
    /// Pool to remove from
    pub pool_id: PoolId,
    /// Amount of LP tokens to burn
    pub lp_tokens: Decimal,
    /// Minimum token_a to receive
    pub min_amount_a: Decimal,
    /// Minimum token_b to receive
    pub min_amount_b: Decimal,
}

/// Result of adding liquidity
#[derive(Debug, Clone)]
pub struct AddLiquidityExecutionResult {
    pub pool_id: PoolId,
    pub result: AddLiquidityResult,
    pub position: LpPosition,
}

/// Result of removing liquidity
#[derive(Debug, Clone)]
pub struct RemoveLiquidityExecutionResult {
    pub pool_id: PoolId,
    pub result: RemoveLiquidityResult,
    pub remaining_lp_tokens: Decimal,
}

/// Use case for managing liquidity
pub struct LiquidityUseCase<C, A, P, E>
where
    C: Clock,
    A: AccountRepository,
    P: PoolReader + PoolWriter + LpPositionReader + LpPositionWriter,
    E: EventPublisher,
{
    clock: Arc<C>,
    account_repo: Arc<A>,
    pool_repo: Arc<P>,
    event_publisher: Arc<E>,
}

impl<C, A, P, E> LiquidityUseCase<C, A, P, E>
where
    C: Clock,
    A: AccountRepository,
    P: PoolReader + PoolWriter + LpPositionReader + LpPositionWriter,
    E: EventPublisher,
{
    pub fn new(
        clock: Arc<C>,
        account_repo: Arc<A>,
        pool_repo: Arc<P>,
        event_publisher: Arc<E>,
    ) -> Self {
        Self {
            clock,
            account_repo,
            pool_repo,
            event_publisher,
        }
    }

    /// Add liquidity to a pool
    pub async fn add_liquidity(
        &self,
        client_id: &str,
        command: AddLiquidityCommand,
    ) -> Result<AddLiquidityExecutionResult, LiquidityUseCaseError> {
        // Get account
        let mut account = self
            .account_repo
            .get_by_owner(client_id)
            .await
            .ok_or(LiquidityUseCaseError::AccountNotFound)?;

        // Check balances
        let balance_a = account.balance(&command.token_a);
        let balance_b = account.balance(&command.token_b);

        if balance_a.available < command.amount_a {
            return Err(LiquidityUseCaseError::InsufficientBalance {
                asset: command.token_a.clone(),
                available: balance_a.available,
                requested: command.amount_a,
            });
        }
        if balance_b.available < command.amount_b {
            return Err(LiquidityUseCaseError::InsufficientBalance {
                asset: command.token_b.clone(),
                available: balance_b.available,
                requested: command.amount_b,
            });
        }

        // Get or create pool
        let mut pool = self
            .pool_repo
            .get_by_tokens(&command.token_a, &command.token_b)
            .await
            .unwrap_or_else(|| LiquidityPool::new(&command.token_a, &command.token_b));

        // Determine which token is which in the pool
        let (amount_a, amount_b) = if pool.token_a == command.token_a {
            (command.amount_a, command.amount_b)
        } else {
            (command.amount_b, command.amount_a)
        };

        // Calculate price ratio for IL tracking
        let entry_price_ratio = if pool.has_liquidity() {
            pool.reserve_b / pool.reserve_a
        } else {
            amount_b / amount_a
        };

        // Add liquidity
        let result = pool
            .add_liquidity(amount_a, amount_b, command.min_lp_tokens)
            .map_err(LiquidityUseCaseError::PoolError)?;

        // Deduct tokens from account
        let (token_a_used, token_b_used) = if pool.token_a == command.token_a {
            (&command.token_a, &command.token_b)
        } else {
            (&command.token_b, &command.token_a)
        };
        account
            .withdraw(token_a_used, result.amount_a_used)
            .map_err(|e| LiquidityUseCaseError::AccountError(e.to_string()))?;
        account
            .withdraw(token_b_used, result.amount_b_used)
            .map_err(|e| LiquidityUseCaseError::AccountError(e.to_string()))?;

        // Credit LP tokens (as virtual balance)
        account.deposit(&pool.lp_token_symbol, result.lp_tokens);

        // Get or create LP position
        let mut position = self
            .pool_repo
            .get_position(&pool.id, &account.id)
            .await
            .unwrap_or_else(|| {
                LpPosition::new(pool.id, account.id, Decimal::ZERO, entry_price_ratio)
            });

        position.lp_tokens += result.lp_tokens;

        // Save everything
        let pool_id = pool.id;
        self.pool_repo.save(pool).await;
        self.pool_repo.save_position(position.clone()).await;
        self.account_repo.save(account).await;

        // Publish event
        let event = LiquidityAddedEvent {
            pool_id,
            token_a: command.token_a,
            token_b: command.token_b,
            amount_a_added: result.amount_a_used,
            amount_b_added: result.amount_b_used,
            lp_tokens_minted: result.lp_tokens,
            share_of_pool: result.share_of_pool,
            timestamp: self.clock.now_millis(),
        };
        self.event_publisher
            .publish(ExchangeEvent::LiquidityAdded(event))
            .await;

        Ok(AddLiquidityExecutionResult {
            pool_id,
            result,
            position,
        })
    }

    /// Remove liquidity from a pool
    pub async fn remove_liquidity(
        &self,
        client_id: &str,
        command: RemoveLiquidityCommand,
    ) -> Result<RemoveLiquidityExecutionResult, LiquidityUseCaseError> {
        // Get account
        let mut account = self
            .account_repo
            .get_by_owner(client_id)
            .await
            .ok_or(LiquidityUseCaseError::AccountNotFound)?;

        // Get pool
        let mut pool = self
            .pool_repo
            .get(&command.pool_id)
            .await
            .ok_or(LiquidityUseCaseError::PoolNotFound)?;

        // Check LP token balance
        let lp_balance = account.balance(&pool.lp_token_symbol);
        if lp_balance.available < command.lp_tokens {
            return Err(LiquidityUseCaseError::InsufficientLpTokens {
                available: lp_balance.available,
                requested: command.lp_tokens,
            });
        }

        // Get LP position
        let mut position = self
            .pool_repo
            .get_position(&pool.id, &account.id)
            .await
            .ok_or(LiquidityUseCaseError::NoPosition)?;

        // Remove liquidity
        let result = pool
            .remove_liquidity(
                command.lp_tokens,
                command.min_amount_a,
                command.min_amount_b,
            )
            .map_err(LiquidityUseCaseError::PoolError)?;

        // Burn LP tokens
        account
            .withdraw(&pool.lp_token_symbol, command.lp_tokens)
            .map_err(|e| LiquidityUseCaseError::AccountError(e.to_string()))?;

        // Credit tokens back
        account.deposit(&pool.token_a, result.amount_a);
        account.deposit(&pool.token_b, result.amount_b);

        // Update position
        position.lp_tokens -= command.lp_tokens;
        let remaining_lp_tokens = position.lp_tokens;

        // Save everything
        let pool_id = pool.id;
        let token_a = pool.token_a.clone();
        let token_b = pool.token_b.clone();

        self.pool_repo.save(pool).await;
        if position.lp_tokens > Decimal::ZERO {
            self.pool_repo.save_position(position).await;
        } else {
            self.pool_repo.delete_position(&pool_id, &account.id).await;
        }
        self.account_repo.save(account).await;

        // Publish event
        let event = LiquidityRemovedEvent {
            pool_id,
            token_a,
            token_b,
            amount_a_received: result.amount_a,
            amount_b_received: result.amount_b,
            lp_tokens_burned: result.lp_tokens_burned,
            timestamp: self.clock.now_millis(),
        };
        self.event_publisher
            .publish(ExchangeEvent::LiquidityRemoved(event))
            .await;

        Ok(RemoveLiquidityExecutionResult {
            pool_id,
            result,
            remaining_lp_tokens,
        })
    }

    /// Get all pools
    pub async fn get_pools(&self) -> Vec<LiquidityPool> {
        self.pool_repo.get_active().await
    }

    /// Get user's LP positions
    pub async fn get_positions(
        &self,
        client_id: &str,
    ) -> Result<Vec<LpPosition>, LiquidityUseCaseError> {
        let account = self
            .account_repo
            .get_by_owner(client_id)
            .await
            .ok_or(LiquidityUseCaseError::AccountNotFound)?;

        Ok(self.pool_repo.get_positions_by_account(&account.id).await)
    }

    /// Calculate impermanent loss for a position
    pub async fn calculate_impermanent_loss(
        &self,
        client_id: &str,
        pool_id: &PoolId,
    ) -> Result<Decimal, LiquidityUseCaseError> {
        let account = self
            .account_repo
            .get_by_owner(client_id)
            .await
            .ok_or(LiquidityUseCaseError::AccountNotFound)?;

        let pool = self
            .pool_repo
            .get(pool_id)
            .await
            .ok_or(LiquidityUseCaseError::PoolNotFound)?;

        let position = self
            .pool_repo
            .get_position(pool_id, &account.id)
            .await
            .ok_or(LiquidityUseCaseError::NoPosition)?;

        let current_ratio = pool.reserve_b / pool.reserve_a;
        let il =
            LiquidityPool::calculate_impermanent_loss(position.entry_price_ratio, current_ratio);

        Ok(il)
    }
}

/// Event emitted when liquidity is added
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiquidityAddedEvent {
    pub pool_id: PoolId,
    pub token_a: String,
    pub token_b: String,
    pub amount_a_added: Decimal,
    pub amount_b_added: Decimal,
    pub lp_tokens_minted: Decimal,
    pub share_of_pool: Decimal,
    pub timestamp: i64,
}

/// Event emitted when liquidity is removed
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LiquidityRemovedEvent {
    pub pool_id: PoolId,
    pub token_a: String,
    pub token_b: String,
    pub amount_a_received: Decimal,
    pub amount_b_received: Decimal,
    pub lp_tokens_burned: Decimal,
    pub timestamp: i64,
}

/// Errors that can occur during liquidity operations
#[derive(Debug, Clone)]
pub enum LiquidityUseCaseError {
    AccountNotFound,
    PoolNotFound,
    NoPosition,
    InsufficientBalance {
        asset: String,
        available: Decimal,
        requested: Decimal,
    },
    InsufficientLpTokens {
        available: Decimal,
        requested: Decimal,
    },
    PoolError(PoolError),
    AccountError(String),
}

impl std::fmt::Display for LiquidityUseCaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiquidityUseCaseError::AccountNotFound => write!(f, "Account not found"),
            LiquidityUseCaseError::PoolNotFound => write!(f, "Pool not found"),
            LiquidityUseCaseError::NoPosition => write!(f, "No LP position found"),
            LiquidityUseCaseError::InsufficientBalance {
                asset,
                available,
                requested,
            } => {
                write!(
                    f,
                    "Insufficient {} balance: {} available, {} requested",
                    asset, available, requested
                )
            }
            LiquidityUseCaseError::InsufficientLpTokens {
                available,
                requested,
            } => {
                write!(
                    f,
                    "Insufficient LP tokens: {} available, {} requested",
                    available, requested
                )
            }
            LiquidityUseCaseError::PoolError(e) => write!(f, "Pool error: {}", e),
            LiquidityUseCaseError::AccountError(s) => write!(f, "Account error: {}", s),
        }
    }
}

impl std::error::Error for LiquidityUseCaseError {}
