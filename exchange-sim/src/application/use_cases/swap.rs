//! Swap Use Case for DEX AMM
//!
//! Handles token swaps through liquidity pools.

use crate::application::ports::{AccountRepository, EventPublisher, PoolReader, PoolWriter};
use crate::domain::{Clock, ExchangeEvent, PoolError, Price, SwapResult, Value};
use std::sync::Arc;

/// Command to execute a swap
#[derive(Debug, Clone)]
pub struct SwapCommand {
    /// Token being sold
    pub token_in: String,
    /// Token being bought
    pub token_out: String,
    /// Amount of token_in to swap
    pub amount_in: Value,
    /// Minimum amount of token_out to receive (slippage protection)
    pub min_amount_out: Value,
}

/// Result of a swap execution
#[derive(Debug, Clone)]
pub struct SwapExecutionResult {
    pub swap_result: SwapResult,
    pub new_reserve_in: Value,
    pub new_reserve_out: Value,
}

/// Use case for executing swaps
pub struct SwapUseCase<C, A, P, E>
where
    C: Clock,
    A: AccountRepository,
    P: PoolReader + PoolWriter,
    E: EventPublisher,
{
    clock: Arc<C>,
    account_repo: Arc<A>,
    pool_repo: Arc<P>,
    event_publisher: Arc<E>,
}

impl<C, A, P, E> SwapUseCase<C, A, P, E>
where
    C: Clock,
    A: AccountRepository,
    P: PoolReader + PoolWriter,
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

    pub async fn execute(
        &self,
        client_id: &str,
        command: SwapCommand,
    ) -> Result<SwapExecutionResult, SwapUseCaseError> {
        // Get account
        let mut account = self
            .account_repo
            .get_by_owner(client_id)
            .await
            .ok_or(SwapUseCaseError::AccountNotFound)?;

        // Check user has enough of input token
        let balance = account.balance(&command.token_in);
        if balance.available.raw() < command.amount_in.raw() {
            return Err(SwapUseCaseError::InsufficientBalance {
                available: balance.available,
                requested: command.amount_in,
            });
        }

        // Find the pool
        let mut pool = self
            .pool_repo
            .get_by_tokens(&command.token_in, &command.token_out)
            .await
            .ok_or(SwapUseCaseError::PoolNotFound {
                token_a: command.token_in.clone(),
                token_b: command.token_out.clone(),
            })?;

        // Determine swap direction
        let is_a_to_b = pool.token_a == command.token_in;

        // Execute swap
        let swap_result = pool
            .swap(command.amount_in, command.min_amount_out, is_a_to_b)
            .map_err(SwapUseCaseError::PoolError)?;

        // Update account balances
        account
            .withdraw(&command.token_in, command.amount_in)
            .map_err(|e| SwapUseCaseError::AccountError(e.to_string()))?;
        account.deposit(&command.token_out, swap_result.amount_out);

        // Get new reserves for result
        let (new_reserve_in, new_reserve_out) = if is_a_to_b {
            (pool.reserve_a, pool.reserve_b)
        } else {
            (pool.reserve_b, pool.reserve_a)
        };

        // Save pool and account
        self.pool_repo.save(pool.clone()).await;
        self.account_repo.save(account).await;

        // Publish swap event
        let swap_event = SwapExecutedEvent {
            pool_id: pool.id,
            token_in: command.token_in,
            token_out: command.token_out,
            amount_in: command.amount_in,
            amount_out: swap_result.amount_out,
            fee_amount: swap_result.fee_amount,
            price_impact_bps: swap_result.price_impact_bps,
            timestamp: self.clock.now_millis(),
        };
        self.event_publisher
            .publish(ExchangeEvent::SwapExecuted(swap_event))
            .await;

        Ok(SwapExecutionResult {
            swap_result,
            new_reserve_in,
            new_reserve_out,
        })
    }

    /// Get a quote for a swap without executing it
    pub async fn quote(&self, command: &SwapCommand) -> Result<SwapQuote, SwapUseCaseError> {
        let pool = self
            .pool_repo
            .get_by_tokens(&command.token_in, &command.token_out)
            .await
            .ok_or(SwapUseCaseError::PoolNotFound {
                token_a: command.token_in.clone(),
                token_b: command.token_out.clone(),
            })?;

        let is_a_to_b = pool.token_a == command.token_in;
        let output = pool
            .calculate_swap_output(command.amount_in, is_a_to_b)
            .map_err(SwapUseCaseError::PoolError)?;

        Ok(SwapQuote {
            amount_out: output.amount_out,
            fee_amount: output.fee_amount,
            price_impact_bps: output.price_impact_bps,
            effective_price: output.effective_price,
        })
    }
}

/// Quote for a potential swap
#[derive(Debug, Clone)]
pub struct SwapQuote {
    pub amount_out: Value,
    pub fee_amount: Value,
    pub price_impact_bps: i64,
    pub effective_price: Price,
}

/// Event emitted when a swap is executed
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SwapExecutedEvent {
    pub pool_id: crate::domain::PoolId,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: Value,
    pub amount_out: Value,
    pub fee_amount: Value,
    pub price_impact_bps: i64,
    pub timestamp: i64,
}

/// Errors that can occur during swap
#[derive(Debug, Clone)]
pub enum SwapUseCaseError {
    AccountNotFound,
    PoolNotFound { token_a: String, token_b: String },
    InsufficientBalance { available: Value, requested: Value },
    PoolError(PoolError),
    AccountError(String),
}

impl std::fmt::Display for SwapUseCaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwapUseCaseError::AccountNotFound => write!(f, "Account not found"),
            SwapUseCaseError::PoolNotFound { token_a, token_b } => {
                write!(f, "Pool not found for {}/{}", token_a, token_b)
            }
            SwapUseCaseError::InsufficientBalance {
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
            SwapUseCaseError::PoolError(e) => write!(f, "Pool error: {}", e),
            SwapUseCaseError::AccountError(s) => write!(f, "Account error: {}", s),
        }
    }
}

impl std::error::Error for SwapUseCaseError {}
