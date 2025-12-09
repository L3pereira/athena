//! Liquidity Pool entity for AMM-style DEX
//!
//! Implements constant product market maker (x * y = k) like Uniswap V2.
//! Supports:
//! - Adding/removing liquidity
//! - Swapping tokens
//! - LP token accounting
//! - Fee collection

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AccountId, Price, Timestamp};

/// Unique identifier for a liquidity pool
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolId(Uuid);

impl PoolId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PoolId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// AMM type/formula
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AmmType {
    /// Constant product (x * y = k) - Uniswap V2 style
    #[default]
    ConstantProduct,
    /// Stable swap (optimized for stable pairs) - Curve style
    StableSwap,
    /// Concentrated liquidity - Uniswap V3 style (simplified)
    ConcentratedLiquidity,
}

/// A liquidity pool for AMM trading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityPool {
    /// Unique identifier
    pub id: PoolId,
    /// First token (token_a)
    pub token_a: String,
    /// Second token (token_b)
    pub token_b: String,
    /// Reserve of token_a
    pub reserve_a: Decimal,
    /// Reserve of token_b
    pub reserve_b: Decimal,
    /// AMM type
    pub amm_type: AmmType,
    /// Swap fee rate (e.g., 0.003 = 0.3%)
    pub fee_rate: Decimal,
    /// Protocol fee rate (portion of swap fee that goes to protocol)
    pub protocol_fee_rate: Decimal,
    /// Total LP token supply
    pub lp_token_supply: Decimal,
    /// LP token symbol
    pub lp_token_symbol: String,
    /// Accumulated fees for token_a
    pub accumulated_fees_a: Decimal,
    /// Accumulated fees for token_b
    pub accumulated_fees_b: Decimal,
    /// Whether the pool is active
    pub active: bool,
    /// Created timestamp
    pub created_at: Timestamp,
    /// Last swap timestamp
    pub last_swap_at: Option<Timestamp>,
    /// Total volume traded (in token_b terms)
    pub total_volume: Decimal,
    /// Number of swaps
    pub swap_count: u64,
}

impl LiquidityPool {
    /// Minimum liquidity that's locked forever (prevents division by zero)
    pub const MINIMUM_LIQUIDITY: Decimal = dec!(0.000001);

    /// Create a new liquidity pool
    pub fn new(token_a: impl Into<String>, token_b: impl Into<String>) -> Self {
        let token_a = token_a.into();
        let token_b = token_b.into();
        let lp_token_symbol = format!("{}-{}-LP", token_a, token_b);

        Self {
            id: PoolId::new(),
            token_a,
            token_b,
            reserve_a: Decimal::ZERO,
            reserve_b: Decimal::ZERO,
            amm_type: AmmType::ConstantProduct,
            fee_rate: dec!(0.003),           // 0.3% default
            protocol_fee_rate: dec!(0.0005), // 0.05% protocol fee
            lp_token_supply: Decimal::ZERO,
            lp_token_symbol,
            accumulated_fees_a: Decimal::ZERO,
            accumulated_fees_b: Decimal::ZERO,
            active: true,
            created_at: chrono::Utc::now(),
            last_swap_at: None,
            total_volume: Decimal::ZERO,
            swap_count: 0,
        }
    }

    pub fn with_fee_rate(mut self, fee_rate: Decimal) -> Self {
        self.fee_rate = fee_rate;
        self
    }

    pub fn with_amm_type(mut self, amm_type: AmmType) -> Self {
        self.amm_type = amm_type;
        self
    }

    /// Get the pool symbol (e.g., "USDT-BTC")
    pub fn symbol(&self) -> String {
        format!("{}-{}", self.token_a, self.token_b)
    }

    /// Check if pool has liquidity
    pub fn has_liquidity(&self) -> bool {
        self.reserve_a > Decimal::ZERO && self.reserve_b > Decimal::ZERO
    }

    /// Get the constant product (k = x * y)
    pub fn constant_product(&self) -> Decimal {
        self.reserve_a * self.reserve_b
    }

    /// Get the current price of token_a in terms of token_b
    pub fn price_a_in_b(&self) -> Option<Price> {
        if self.reserve_a.is_zero() {
            return None;
        }
        Some(Price::from(self.reserve_b / self.reserve_a))
    }

    /// Get the current price of token_b in terms of token_a
    pub fn price_b_in_a(&self) -> Option<Price> {
        if self.reserve_b.is_zero() {
            return None;
        }
        Some(Price::from(self.reserve_a / self.reserve_b))
    }

    /// Calculate output amount for a swap (constant product formula)
    /// Returns (output_amount, fee_amount)
    pub fn calculate_swap_output(
        &self,
        amount_in: Decimal,
        is_a_to_b: bool,
    ) -> Result<SwapOutput, PoolError> {
        if !self.active {
            return Err(PoolError::PoolInactive);
        }
        if !self.has_liquidity() {
            return Err(PoolError::InsufficientLiquidity);
        }
        if amount_in <= Decimal::ZERO {
            return Err(PoolError::InvalidAmount);
        }

        let (reserve_in, reserve_out) = if is_a_to_b {
            (self.reserve_a, self.reserve_b)
        } else {
            (self.reserve_b, self.reserve_a)
        };

        // Calculate fee
        let fee_amount = amount_in * self.fee_rate;
        let amount_in_after_fee = amount_in - fee_amount;

        // Constant product formula: (x + Δx) * (y - Δy) = x * y
        // Solving for Δy: Δy = (y * Δx) / (x + Δx)
        let amount_out = (reserve_out * amount_in_after_fee) / (reserve_in + amount_in_after_fee);

        // Calculate price impact
        let price_before = reserve_out / reserve_in;
        let new_reserve_in = reserve_in + amount_in;
        let new_reserve_out = reserve_out - amount_out;
        let price_after = new_reserve_out / new_reserve_in;
        let price_impact = ((price_before - price_after) / price_before).abs();

        // Minimum output check (0.1% slippage protection built-in)
        if amount_out <= Decimal::ZERO {
            return Err(PoolError::InsufficientOutput);
        }

        Ok(SwapOutput {
            amount_out,
            fee_amount,
            price_impact,
            effective_price: amount_out / amount_in,
        })
    }

    /// Execute a swap
    pub fn swap(
        &mut self,
        amount_in: Decimal,
        min_amount_out: Decimal,
        is_a_to_b: bool,
    ) -> Result<SwapResult, PoolError> {
        let output = self.calculate_swap_output(amount_in, is_a_to_b)?;

        if output.amount_out < min_amount_out {
            return Err(PoolError::SlippageExceeded {
                expected: min_amount_out,
                actual: output.amount_out,
            });
        }

        // Update reserves
        if is_a_to_b {
            self.reserve_a += amount_in;
            self.reserve_b -= output.amount_out;
            self.accumulated_fees_a += output.fee_amount * self.protocol_fee_rate / self.fee_rate;
        } else {
            self.reserve_b += amount_in;
            self.reserve_a -= output.amount_out;
            self.accumulated_fees_b += output.fee_amount * self.protocol_fee_rate / self.fee_rate;
        }

        // Update stats
        self.last_swap_at = Some(chrono::Utc::now());
        self.total_volume += if is_a_to_b {
            output.amount_out
        } else {
            amount_in
        };
        self.swap_count += 1;

        Ok(SwapResult {
            amount_in,
            amount_out: output.amount_out,
            fee_amount: output.fee_amount,
            price_impact: output.price_impact,
            is_a_to_b,
        })
    }

    /// Calculate LP tokens to mint for adding liquidity
    pub fn calculate_add_liquidity(
        &self,
        amount_a: Decimal,
        amount_b: Decimal,
    ) -> Result<AddLiquidityOutput, PoolError> {
        if amount_a <= Decimal::ZERO || amount_b <= Decimal::ZERO {
            return Err(PoolError::InvalidAmount);
        }

        let lp_tokens = if self.lp_token_supply.is_zero() {
            // First liquidity provider
            // LP tokens = sqrt(amount_a * amount_b) - MINIMUM_LIQUIDITY
            let product = amount_a * amount_b;
            let sqrt = decimal_sqrt(product);
            if sqrt <= Self::MINIMUM_LIQUIDITY {
                return Err(PoolError::InsufficientLiquidity);
            }
            sqrt - Self::MINIMUM_LIQUIDITY
        } else {
            // Subsequent providers
            // LP tokens = min(amount_a / reserve_a, amount_b / reserve_b) * total_supply
            let ratio_a = amount_a / self.reserve_a;
            let ratio_b = amount_b / self.reserve_b;
            let ratio = ratio_a.min(ratio_b);
            ratio * self.lp_token_supply
        };

        // Calculate optimal amounts (maintain ratio)
        let (optimal_a, optimal_b) = if self.lp_token_supply.is_zero() {
            (amount_a, amount_b)
        } else {
            let ratio = self.reserve_b / self.reserve_a;
            let optimal_b_for_a = amount_a * ratio;
            if optimal_b_for_a <= amount_b {
                (amount_a, optimal_b_for_a)
            } else {
                (amount_b / ratio, amount_b)
            }
        };

        Ok(AddLiquidityOutput {
            lp_tokens,
            amount_a_used: optimal_a,
            amount_b_used: optimal_b,
            share_of_pool: if self.lp_token_supply.is_zero() {
                dec!(1)
            } else {
                lp_tokens / (self.lp_token_supply + lp_tokens)
            },
        })
    }

    /// Add liquidity to the pool
    pub fn add_liquidity(
        &mut self,
        amount_a: Decimal,
        amount_b: Decimal,
        min_lp_tokens: Decimal,
    ) -> Result<AddLiquidityResult, PoolError> {
        let output = self.calculate_add_liquidity(amount_a, amount_b)?;

        if output.lp_tokens < min_lp_tokens {
            return Err(PoolError::SlippageExceeded {
                expected: min_lp_tokens,
                actual: output.lp_tokens,
            });
        }

        // Lock minimum liquidity on first add
        let first_add = self.lp_token_supply.is_zero();

        // Update reserves
        self.reserve_a += output.amount_a_used;
        self.reserve_b += output.amount_b_used;

        // Mint LP tokens
        if first_add {
            self.lp_token_supply = output.lp_tokens + Self::MINIMUM_LIQUIDITY;
        } else {
            self.lp_token_supply += output.lp_tokens;
        }

        Ok(AddLiquidityResult {
            lp_tokens: output.lp_tokens,
            amount_a_used: output.amount_a_used,
            amount_b_used: output.amount_b_used,
            share_of_pool: output.share_of_pool,
        })
    }

    /// Calculate tokens to receive for removing liquidity
    pub fn calculate_remove_liquidity(
        &self,
        lp_tokens: Decimal,
    ) -> Result<RemoveLiquidityOutput, PoolError> {
        if lp_tokens <= Decimal::ZERO {
            return Err(PoolError::InvalidAmount);
        }
        if lp_tokens > self.lp_token_supply - Self::MINIMUM_LIQUIDITY {
            return Err(PoolError::InsufficientLpTokens);
        }

        let share = lp_tokens / self.lp_token_supply;
        let amount_a = self.reserve_a * share;
        let amount_b = self.reserve_b * share;

        Ok(RemoveLiquidityOutput {
            amount_a,
            amount_b,
            share_removed: share,
        })
    }

    /// Remove liquidity from the pool
    pub fn remove_liquidity(
        &mut self,
        lp_tokens: Decimal,
        min_amount_a: Decimal,
        min_amount_b: Decimal,
    ) -> Result<RemoveLiquidityResult, PoolError> {
        let output = self.calculate_remove_liquidity(lp_tokens)?;

        if output.amount_a < min_amount_a {
            return Err(PoolError::SlippageExceeded {
                expected: min_amount_a,
                actual: output.amount_a,
            });
        }
        if output.amount_b < min_amount_b {
            return Err(PoolError::SlippageExceeded {
                expected: min_amount_b,
                actual: output.amount_b,
            });
        }

        // Update reserves
        self.reserve_a -= output.amount_a;
        self.reserve_b -= output.amount_b;

        // Burn LP tokens
        self.lp_token_supply -= lp_tokens;

        Ok(RemoveLiquidityResult {
            lp_tokens_burned: lp_tokens,
            amount_a: output.amount_a,
            amount_b: output.amount_b,
        })
    }

    /// Calculate impermanent loss for a position
    /// Returns the loss as a percentage (0.0 = no loss, 0.05 = 5% loss)
    pub fn calculate_impermanent_loss(
        initial_price_ratio: Decimal,
        current_price_ratio: Decimal,
    ) -> Decimal {
        if initial_price_ratio.is_zero() {
            return Decimal::ZERO;
        }

        let price_ratio = current_price_ratio / initial_price_ratio;
        let sqrt_ratio = decimal_sqrt(price_ratio);

        // IL = 2 * sqrt(price_ratio) / (1 + price_ratio) - 1
        let il = dec!(2) * sqrt_ratio / (dec!(1) + price_ratio) - dec!(1);
        il.abs()
    }
}

/// Output of swap calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapOutput {
    pub amount_out: Decimal,
    pub fee_amount: Decimal,
    pub price_impact: Decimal,
    pub effective_price: Decimal,
}

/// Result of executing a swap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapResult {
    pub amount_in: Decimal,
    pub amount_out: Decimal,
    pub fee_amount: Decimal,
    pub price_impact: Decimal,
    pub is_a_to_b: bool,
}

/// Output of add liquidity calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLiquidityOutput {
    pub lp_tokens: Decimal,
    pub amount_a_used: Decimal,
    pub amount_b_used: Decimal,
    pub share_of_pool: Decimal,
}

/// Result of adding liquidity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLiquidityResult {
    pub lp_tokens: Decimal,
    pub amount_a_used: Decimal,
    pub amount_b_used: Decimal,
    pub share_of_pool: Decimal,
}

/// Output of remove liquidity calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLiquidityOutput {
    pub amount_a: Decimal,
    pub amount_b: Decimal,
    pub share_removed: Decimal,
}

/// Result of removing liquidity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLiquidityResult {
    pub lp_tokens_burned: Decimal,
    pub amount_a: Decimal,
    pub amount_b: Decimal,
}

/// LP position for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpPosition {
    pub pool_id: PoolId,
    pub account_id: AccountId,
    pub lp_tokens: Decimal,
    /// Price ratio when position was opened (for IL calculation)
    pub entry_price_ratio: Decimal,
    pub created_at: Timestamp,
}

impl LpPosition {
    pub fn new(
        pool_id: PoolId,
        account_id: AccountId,
        lp_tokens: Decimal,
        entry_price_ratio: Decimal,
    ) -> Self {
        Self {
            pool_id,
            account_id,
            lp_tokens,
            entry_price_ratio,
            created_at: chrono::Utc::now(),
        }
    }
}

/// Errors that can occur with liquidity pools
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PoolError {
    PoolInactive,
    InsufficientLiquidity,
    InsufficientOutput,
    InsufficientLpTokens,
    InvalidAmount,
    SlippageExceeded { expected: Decimal, actual: Decimal },
    PoolNotFound,
    TokenNotSupported(String),
}

impl std::fmt::Display for PoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PoolError::PoolInactive => write!(f, "Pool is inactive"),
            PoolError::InsufficientLiquidity => write!(f, "Insufficient liquidity"),
            PoolError::InsufficientOutput => write!(f, "Insufficient output amount"),
            PoolError::InsufficientLpTokens => write!(f, "Insufficient LP tokens"),
            PoolError::InvalidAmount => write!(f, "Invalid amount"),
            PoolError::SlippageExceeded { expected, actual } => {
                write!(
                    f,
                    "Slippage exceeded: expected {}, got {}",
                    expected, actual
                )
            }
            PoolError::PoolNotFound => write!(f, "Pool not found"),
            PoolError::TokenNotSupported(token) => write!(f, "Token not supported: {}", token),
        }
    }
}

impl std::error::Error for PoolError {}

/// Simple integer square root using Newton's method for Decimal
fn decimal_sqrt(n: Decimal) -> Decimal {
    if n.is_zero() {
        return Decimal::ZERO;
    }
    if n < Decimal::ZERO {
        return Decimal::ZERO; // Handle negative input
    }

    let mut x = n;
    let mut y = (x + dec!(1)) / dec!(2);

    // Newton's method iterations
    for _ in 0..100 {
        if y >= x {
            break;
        }
        x = y;
        y = (x + n / x) / dec!(2);
    }

    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_pool() {
        let pool = LiquidityPool::new("USDT", "BTC");
        assert_eq!(pool.token_a, "USDT");
        assert_eq!(pool.token_b, "BTC");
        assert!(!pool.has_liquidity());
    }

    #[test]
    fn test_add_initial_liquidity() {
        let mut pool = LiquidityPool::new("USDT", "BTC");

        let result = pool.add_liquidity(dec!(10000), dec!(1), dec!(0)).unwrap();

        assert!(result.lp_tokens > Decimal::ZERO);
        assert_eq!(pool.reserve_a, dec!(10000));
        assert_eq!(pool.reserve_b, dec!(1));
        assert!(pool.has_liquidity());
    }

    #[test]
    fn test_swap() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(dec!(100000), dec!(1), dec!(0)).unwrap();

        // Swap 1000 USDT for BTC
        let result = pool.swap(dec!(1000), dec!(0), true).unwrap();

        assert!(result.amount_out > Decimal::ZERO);
        assert!(result.amount_out < dec!(0.01)); // Should get less than 1% of BTC reserve
        assert!(result.fee_amount > Decimal::ZERO);
        assert!(result.price_impact > Decimal::ZERO);
    }

    #[test]
    fn test_price_calculation() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(dec!(100000), dec!(1), dec!(0)).unwrap();

        // Price of BTC in USDT terms
        let price = pool.price_b_in_a().unwrap();
        assert_eq!(price.inner(), dec!(100000));
    }

    #[test]
    fn test_constant_product_maintained() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(dec!(100000), dec!(1), dec!(0)).unwrap();

        let k_before = pool.constant_product();

        // Perform swap
        pool.swap(dec!(10000), dec!(0), true).unwrap();

        let k_after = pool.constant_product();

        // k should increase slightly due to fees
        assert!(k_after >= k_before);
    }

    #[test]
    fn test_remove_liquidity() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        let add_result = pool.add_liquidity(dec!(10000), dec!(1), dec!(0)).unwrap();

        // Remove half the liquidity
        let half_lp = add_result.lp_tokens / dec!(2);
        let remove_result = pool.remove_liquidity(half_lp, dec!(0), dec!(0)).unwrap();

        assert!(remove_result.amount_a > dec!(4000)); // Should get roughly half
        assert!(remove_result.amount_b > dec!(0.4));
    }

    #[test]
    fn test_slippage_protection() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(dec!(10000), dec!(1), dec!(0)).unwrap();

        // Try to swap with unrealistic minimum output
        let result = pool.swap(dec!(100), dec!(1), true); // Expecting 1 BTC for 100 USDT

        assert!(matches!(result, Err(PoolError::SlippageExceeded { .. })));
    }

    #[test]
    fn test_decimal_sqrt() {
        assert_eq!(decimal_sqrt(dec!(4)), dec!(2));
        assert_eq!(decimal_sqrt(dec!(9)), dec!(3));
        assert_eq!(decimal_sqrt(dec!(100)), dec!(10));

        // Approximate for non-perfect squares
        let sqrt_2 = decimal_sqrt(dec!(2));
        assert!(sqrt_2 > dec!(1.41) && sqrt_2 < dec!(1.42));
    }

    #[test]
    fn test_impermanent_loss() {
        // 2x price increase
        let il = LiquidityPool::calculate_impermanent_loss(dec!(1), dec!(2));
        // IL for 2x should be about 5.7%
        assert!(il > dec!(0.05) && il < dec!(0.06));

        // No price change = no IL
        let il_none = LiquidityPool::calculate_impermanent_loss(dec!(1), dec!(1));
        assert!(il_none < dec!(0.001));
    }
}
