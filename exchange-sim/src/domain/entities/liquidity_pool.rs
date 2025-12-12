//! Liquidity Pool entity for AMM-style DEX
//!
//! Implements constant product market maker (x * y = k) like Uniswap V2.
//! Supports:
//! - Adding/removing liquidity
//! - Swapping tokens
//! - LP token accounting
//! - Fee collection

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AccountId, PRICE_SCALE, Price, Rate, Timestamp, Value};

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
    pub reserve_a: Value,
    /// Reserve of token_b
    pub reserve_b: Value,
    /// AMM type
    pub amm_type: AmmType,
    /// Swap fee rate (in basis points, e.g., 30 = 0.3%)
    pub fee_rate: Rate,
    /// Protocol fee rate (portion of swap fee that goes to protocol)
    pub protocol_fee_rate: Rate,
    /// Total LP token supply
    pub lp_token_supply: Value,
    /// LP token symbol
    pub lp_token_symbol: String,
    /// Accumulated fees for token_a
    pub accumulated_fees_a: Value,
    /// Accumulated fees for token_b
    pub accumulated_fees_b: Value,
    /// Whether the pool is active
    pub active: bool,
    /// Created timestamp
    pub created_at: Timestamp,
    /// Last swap timestamp
    pub last_swap_at: Option<Timestamp>,
    /// Total volume traded (in token_b terms)
    pub total_volume: Value,
    /// Number of swaps
    pub swap_count: u64,
}

/// Minimum liquidity constant (prevents division by zero)
const MINIMUM_LIQUIDITY_RAW: i128 = 1000; // 0.00001 with 8 decimals

impl LiquidityPool {
    /// Minimum liquidity that's locked forever
    pub const MINIMUM_LIQUIDITY: Value = Value::from_raw(MINIMUM_LIQUIDITY_RAW);

    /// Create a new liquidity pool
    pub fn new(token_a: impl Into<String>, token_b: impl Into<String>) -> Self {
        let token_a = token_a.into();
        let token_b = token_b.into();
        let lp_token_symbol = format!("{}-{}-LP", token_a, token_b);

        Self {
            id: PoolId::new(),
            token_a,
            token_b,
            reserve_a: Value::ZERO,
            reserve_b: Value::ZERO,
            amm_type: AmmType::ConstantProduct,
            fee_rate: Rate::from_bps(30),         // 0.3% default
            protocol_fee_rate: Rate::from_bps(5), // 0.05% protocol fee
            lp_token_supply: Value::ZERO,
            lp_token_symbol,
            accumulated_fees_a: Value::ZERO,
            accumulated_fees_b: Value::ZERO,
            active: true,
            created_at: chrono::Utc::now(),
            last_swap_at: None,
            total_volume: Value::ZERO,
            swap_count: 0,
        }
    }

    pub fn with_fee_rate(mut self, fee_rate: Rate) -> Self {
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
        self.reserve_a.raw() > 0 && self.reserve_b.raw() > 0
    }

    /// Get the constant product (k = x * y), scaled down to avoid overflow
    pub fn constant_product(&self) -> i128 {
        // Use sqrt(PRICE_SCALE) as divisor to preserve more precision
        // sqrt(100_000_000) = 10_000
        const SQRT_SCALE: i128 = 10_000;
        (self.reserve_a.raw() / SQRT_SCALE) * (self.reserve_b.raw() / SQRT_SCALE)
    }

    /// Get the current price of token_a in terms of token_b
    pub fn price_a_in_b(&self) -> Option<Price> {
        if self.reserve_a.raw() == 0 {
            return None;
        }
        // price = reserve_b / reserve_a, scaled to PRICE_SCALE
        let price_raw = (self.reserve_b.raw() * PRICE_SCALE as i128) / self.reserve_a.raw();
        Some(Price::from_raw(price_raw as i64))
    }

    /// Get the current price of token_b in terms of token_a
    pub fn price_b_in_a(&self) -> Option<Price> {
        if self.reserve_b.raw() == 0 {
            return None;
        }
        let price_raw = (self.reserve_a.raw() * PRICE_SCALE as i128) / self.reserve_b.raw();
        Some(Price::from_raw(price_raw as i64))
    }

    /// Calculate output amount for a swap (constant product formula)
    pub fn calculate_swap_output(
        &self,
        amount_in: Value,
        is_a_to_b: bool,
    ) -> Result<SwapOutput, PoolError> {
        if !self.active {
            return Err(PoolError::PoolInactive);
        }
        if !self.has_liquidity() {
            return Err(PoolError::InsufficientLiquidity);
        }
        if amount_in.raw() <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        let (reserve_in, reserve_out) = if is_a_to_b {
            (self.reserve_a.raw(), self.reserve_b.raw())
        } else {
            (self.reserve_b.raw(), self.reserve_a.raw())
        };

        // Calculate fee: fee = amount_in * fee_rate / 10000
        let fee_amount_raw = (amount_in.raw() * self.fee_rate.bps() as i128) / 10_000;
        let amount_in_after_fee = amount_in.raw() - fee_amount_raw;

        // Constant product formula: (x + Δx) * (y - Δy) = x * y
        // Δy = (y * Δx) / (x + Δx)
        let amount_out_raw =
            (reserve_out * amount_in_after_fee) / (reserve_in + amount_in_after_fee);

        // Calculate price impact
        // price_before = reserve_out / reserve_in
        // price_after = (reserve_out - amount_out) / (reserve_in + amount_in)
        // price_impact = |price_before - price_after| / price_before
        let price_impact_raw = if reserve_in > 0 {
            let new_reserve_in = reserve_in + amount_in.raw();
            let new_reserve_out = reserve_out - amount_out_raw;
            // Use basis points for impact: (before - after) * 10000 / before
            let before = (reserve_out * PRICE_SCALE as i128) / reserve_in;
            let after = (new_reserve_out * PRICE_SCALE as i128) / new_reserve_in;
            ((before - after).abs() * 10_000) / before
        } else {
            0
        };

        if amount_out_raw <= 0 {
            return Err(PoolError::InsufficientOutput);
        }

        // effective_price = amount_out / amount_in
        let effective_price_raw = (amount_out_raw * PRICE_SCALE as i128) / amount_in.raw();

        Ok(SwapOutput {
            amount_out: Value::from_raw(amount_out_raw),
            fee_amount: Value::from_raw(fee_amount_raw),
            price_impact_bps: price_impact_raw as i64,
            effective_price: Price::from_raw(effective_price_raw as i64),
        })
    }

    /// Execute a swap
    pub fn swap(
        &mut self,
        amount_in: Value,
        min_amount_out: Value,
        is_a_to_b: bool,
    ) -> Result<SwapResult, PoolError> {
        let output = self.calculate_swap_output(amount_in, is_a_to_b)?;

        if output.amount_out.raw() < min_amount_out.raw() {
            return Err(PoolError::SlippageExceeded {
                expected: min_amount_out,
                actual: output.amount_out,
            });
        }

        // Calculate protocol fee (portion of fee that goes to protocol)
        let protocol_fee_raw = if self.fee_rate.bps() > 0 {
            (output.fee_amount.raw() * self.protocol_fee_rate.bps() as i128)
                / self.fee_rate.bps() as i128
        } else {
            0
        };

        // Update reserves
        if is_a_to_b {
            self.reserve_a = Value::from_raw(self.reserve_a.raw() + amount_in.raw());
            self.reserve_b = Value::from_raw(self.reserve_b.raw() - output.amount_out.raw());
            self.accumulated_fees_a =
                Value::from_raw(self.accumulated_fees_a.raw() + protocol_fee_raw);
        } else {
            self.reserve_b = Value::from_raw(self.reserve_b.raw() + amount_in.raw());
            self.reserve_a = Value::from_raw(self.reserve_a.raw() - output.amount_out.raw());
            self.accumulated_fees_b =
                Value::from_raw(self.accumulated_fees_b.raw() + protocol_fee_raw);
        }

        // Update stats
        self.last_swap_at = Some(chrono::Utc::now());
        let volume_add = if is_a_to_b {
            output.amount_out
        } else {
            amount_in
        };
        self.total_volume = self.total_volume + volume_add;
        self.swap_count += 1;

        Ok(SwapResult {
            amount_in,
            amount_out: output.amount_out,
            fee_amount: output.fee_amount,
            price_impact_bps: output.price_impact_bps,
            is_a_to_b,
        })
    }

    /// Calculate LP tokens to mint for adding liquidity
    pub fn calculate_add_liquidity(
        &self,
        amount_a: Value,
        amount_b: Value,
    ) -> Result<AddLiquidityOutput, PoolError> {
        if amount_a.raw() <= 0 || amount_b.raw() <= 0 {
            return Err(PoolError::InvalidAmount);
        }

        let lp_tokens_raw = if self.lp_token_supply.raw() == 0 {
            // First liquidity provider: LP = sqrt(amount_a * amount_b) - MINIMUM_LIQUIDITY
            // Use sqrt(PRICE_SCALE) = 10_000 to preserve precision for fractional amounts
            const SQRT_SCALE: i128 = 10_000;
            let product = (amount_a.raw() / SQRT_SCALE) * (amount_b.raw() / SQRT_SCALE);
            let sqrt = integer_sqrt(product) * SQRT_SCALE;
            if sqrt <= MINIMUM_LIQUIDITY_RAW {
                return Err(PoolError::InsufficientLiquidity);
            }
            sqrt - MINIMUM_LIQUIDITY_RAW
        } else {
            // Subsequent providers: LP = min(a/ra, b/rb) * supply
            // ratio_a = amount_a * PRICE_SCALE / reserve_a
            let ratio_a = (amount_a.raw() * PRICE_SCALE as i128) / self.reserve_a.raw();
            let ratio_b = (amount_b.raw() * PRICE_SCALE as i128) / self.reserve_b.raw();
            let ratio = ratio_a.min(ratio_b);
            (ratio * self.lp_token_supply.raw()) / PRICE_SCALE as i128
        };

        // Calculate optimal amounts
        let (optimal_a, optimal_b) = if self.lp_token_supply.raw() == 0 {
            (amount_a.raw(), amount_b.raw())
        } else {
            // optimal_b = amount_a * reserve_b / reserve_a
            let optimal_b_for_a = (amount_a.raw() * self.reserve_b.raw()) / self.reserve_a.raw();
            if optimal_b_for_a <= amount_b.raw() {
                (amount_a.raw(), optimal_b_for_a)
            } else {
                let optimal_a_for_b =
                    (amount_b.raw() * self.reserve_a.raw()) / self.reserve_b.raw();
                (optimal_a_for_b, amount_b.raw())
            }
        };

        // share_of_pool = lp_tokens / (total + lp_tokens)
        let share_bps = if self.lp_token_supply.raw() == 0 {
            10_000 // 100%
        } else {
            let new_total = self.lp_token_supply.raw() + lp_tokens_raw;
            ((lp_tokens_raw * 10_000) / new_total) as i64
        };

        Ok(AddLiquidityOutput {
            lp_tokens: Value::from_raw(lp_tokens_raw),
            amount_a_used: Value::from_raw(optimal_a),
            amount_b_used: Value::from_raw(optimal_b),
            share_of_pool_bps: share_bps,
        })
    }

    /// Add liquidity to the pool
    pub fn add_liquidity(
        &mut self,
        amount_a: Value,
        amount_b: Value,
        min_lp_tokens: Value,
    ) -> Result<AddLiquidityResult, PoolError> {
        let output = self.calculate_add_liquidity(amount_a, amount_b)?;

        if output.lp_tokens.raw() < min_lp_tokens.raw() {
            return Err(PoolError::SlippageExceeded {
                expected: min_lp_tokens,
                actual: output.lp_tokens,
            });
        }

        let first_add = self.lp_token_supply.raw() == 0;

        // Update reserves
        self.reserve_a = self.reserve_a + output.amount_a_used;
        self.reserve_b = self.reserve_b + output.amount_b_used;

        // Mint LP tokens
        if first_add {
            self.lp_token_supply = Value::from_raw(output.lp_tokens.raw() + MINIMUM_LIQUIDITY_RAW);
        } else {
            self.lp_token_supply = self.lp_token_supply + output.lp_tokens;
        }

        Ok(AddLiquidityResult {
            lp_tokens: output.lp_tokens,
            amount_a_used: output.amount_a_used,
            amount_b_used: output.amount_b_used,
            share_of_pool_bps: output.share_of_pool_bps,
        })
    }

    /// Calculate tokens to receive for removing liquidity
    pub fn calculate_remove_liquidity(
        &self,
        lp_tokens: Value,
    ) -> Result<RemoveLiquidityOutput, PoolError> {
        if lp_tokens.raw() <= 0 {
            return Err(PoolError::InvalidAmount);
        }
        if lp_tokens.raw() > self.lp_token_supply.raw() - MINIMUM_LIQUIDITY_RAW {
            return Err(PoolError::InsufficientLpTokens);
        }

        // share = lp_tokens / lp_supply
        let amount_a_raw = (self.reserve_a.raw() * lp_tokens.raw()) / self.lp_token_supply.raw();
        let amount_b_raw = (self.reserve_b.raw() * lp_tokens.raw()) / self.lp_token_supply.raw();

        let share_removed_bps = ((lp_tokens.raw() * 10_000) / self.lp_token_supply.raw()) as i64;

        Ok(RemoveLiquidityOutput {
            amount_a: Value::from_raw(amount_a_raw),
            amount_b: Value::from_raw(amount_b_raw),
            share_removed_bps,
        })
    }

    /// Remove liquidity from the pool
    pub fn remove_liquidity(
        &mut self,
        lp_tokens: Value,
        min_amount_a: Value,
        min_amount_b: Value,
    ) -> Result<RemoveLiquidityResult, PoolError> {
        let output = self.calculate_remove_liquidity(lp_tokens)?;

        if output.amount_a.raw() < min_amount_a.raw() {
            return Err(PoolError::SlippageExceeded {
                expected: min_amount_a,
                actual: output.amount_a,
            });
        }
        if output.amount_b.raw() < min_amount_b.raw() {
            return Err(PoolError::SlippageExceeded {
                expected: min_amount_b,
                actual: output.amount_b,
            });
        }

        // Update reserves
        self.reserve_a = Value::from_raw(self.reserve_a.raw() - output.amount_a.raw());
        self.reserve_b = Value::from_raw(self.reserve_b.raw() - output.amount_b.raw());

        // Burn LP tokens
        self.lp_token_supply = Value::from_raw(self.lp_token_supply.raw() - lp_tokens.raw());

        Ok(RemoveLiquidityResult {
            lp_tokens_burned: lp_tokens,
            amount_a: output.amount_a,
            amount_b: output.amount_b,
        })
    }

    /// Calculate impermanent loss for a position
    /// Returns the loss in basis points (e.g., 570 = 5.7%)
    pub fn calculate_impermanent_loss(
        initial_price_ratio_raw: i128,
        current_price_ratio_raw: i128,
    ) -> i64 {
        if initial_price_ratio_raw == 0 {
            return 0;
        }

        // price_ratio = current / initial (scaled by PRICE_SCALE)
        let price_ratio = (current_price_ratio_raw * PRICE_SCALE as i128) / initial_price_ratio_raw;
        let sqrt_ratio = integer_sqrt(price_ratio * PRICE_SCALE as i128);

        // IL = 2 * sqrt(ratio) / (1 + ratio) - 1
        // In fixed point: il_bps = (2 * sqrt_ratio * 10000 / (PRICE_SCALE + price_ratio)) - 10000
        let numerator = 2 * sqrt_ratio * 10_000;
        let denominator = PRICE_SCALE as i128 + price_ratio;
        let il_raw = (numerator / denominator) - 10_000;

        il_raw.abs() as i64
    }
}

/// Output of swap calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapOutput {
    pub amount_out: Value,
    pub fee_amount: Value,
    pub price_impact_bps: i64,
    pub effective_price: Price,
}

/// Result of executing a swap
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapResult {
    pub amount_in: Value,
    pub amount_out: Value,
    pub fee_amount: Value,
    pub price_impact_bps: i64,
    pub is_a_to_b: bool,
}

/// Output of add liquidity calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLiquidityOutput {
    pub lp_tokens: Value,
    pub amount_a_used: Value,
    pub amount_b_used: Value,
    pub share_of_pool_bps: i64,
}

/// Result of adding liquidity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLiquidityResult {
    pub lp_tokens: Value,
    pub amount_a_used: Value,
    pub amount_b_used: Value,
    pub share_of_pool_bps: i64,
}

/// Output of remove liquidity calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLiquidityOutput {
    pub amount_a: Value,
    pub amount_b: Value,
    pub share_removed_bps: i64,
}

/// Result of removing liquidity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveLiquidityResult {
    pub lp_tokens_burned: Value,
    pub amount_a: Value,
    pub amount_b: Value,
}

/// LP position for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpPosition {
    pub pool_id: PoolId,
    pub account_id: AccountId,
    pub lp_tokens: Value,
    /// Price ratio when position was opened (raw i128 for IL calculation)
    pub entry_price_ratio_raw: i128,
    pub created_at: Timestamp,
}

impl LpPosition {
    pub fn new(
        pool_id: PoolId,
        account_id: AccountId,
        lp_tokens: Value,
        entry_price_ratio_raw: i128,
    ) -> Self {
        Self {
            pool_id,
            account_id,
            lp_tokens,
            entry_price_ratio_raw,
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
    SlippageExceeded { expected: Value, actual: Value },
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
                    "Slippage exceeded: expected {:?}, got {:?}",
                    expected, actual
                )
            }
            PoolError::PoolNotFound => write!(f, "Pool not found"),
            PoolError::TokenNotSupported(token) => write!(f, "Token not supported: {}", token),
        }
    }
}

impl std::error::Error for PoolError {}

/// Integer square root using Newton's method
fn integer_sqrt(n: i128) -> i128 {
    if n <= 0 {
        return 0;
    }

    let mut x = n;
    let mut y = (x + 1) / 2;

    for _ in 0..100 {
        if y >= x {
            break;
        }
        x = y;
        y = (x + n / x) / 2;
    }

    x
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create Value from integer (scales by PRICE_SCALE)
    fn val(n: i64) -> Value {
        Value::from_raw(n as i128 * PRICE_SCALE as i128)
    }

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

        let result = pool.add_liquidity(val(10000), val(1), Value::ZERO).unwrap();

        assert!(result.lp_tokens.raw() > 0);
        assert_eq!(pool.reserve_a.raw(), val(10000).raw());
        assert_eq!(pool.reserve_b.raw(), val(1).raw());
        assert!(pool.has_liquidity());
    }

    #[test]
    fn test_swap() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(val(100000), val(1), Value::ZERO)
            .unwrap();

        // Swap 1000 USDT for BTC
        let result = pool.swap(val(1000), Value::ZERO, true).unwrap();

        assert!(result.amount_out.raw() > 0);
        assert!(result.amount_out.raw() < val(1).raw() / 100); // Should get less than 1% of BTC reserve
        assert!(result.fee_amount.raw() > 0);
        assert!(result.price_impact_bps > 0);
    }

    #[test]
    fn test_price_calculation() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(val(100000), val(1), Value::ZERO)
            .unwrap();

        // Price of BTC in USDT terms
        let price = pool.price_b_in_a().unwrap();
        assert_eq!(price.raw(), 100000 * PRICE_SCALE);
    }

    #[test]
    fn test_constant_product_maintained() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(val(100000), val(1), Value::ZERO)
            .unwrap();

        let k_before = pool.constant_product();

        // Perform swap
        pool.swap(val(10000), Value::ZERO, true).unwrap();

        let k_after = pool.constant_product();

        // k should increase slightly due to fees
        assert!(k_after >= k_before);
    }

    #[test]
    fn test_remove_liquidity() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        let add_result = pool.add_liquidity(val(10000), val(1), Value::ZERO).unwrap();

        // Remove half the liquidity
        let half_lp = Value::from_raw(add_result.lp_tokens.raw() / 2);
        let remove_result = pool
            .remove_liquidity(half_lp, Value::ZERO, Value::ZERO)
            .unwrap();

        // Should get roughly half of each
        assert!(remove_result.amount_a.raw() > val(4000).raw());
        assert!(remove_result.amount_b.raw() > val(1).raw() * 4 / 10);
    }

    #[test]
    fn test_slippage_protection() {
        let mut pool = LiquidityPool::new("USDT", "BTC");
        pool.add_liquidity(val(10000), val(1), Value::ZERO).unwrap();

        // Try to swap with unrealistic minimum output
        let result = pool.swap(val(100), val(1), true); // Expecting 1 BTC for 100 USDT

        assert!(matches!(result, Err(PoolError::SlippageExceeded { .. })));
    }

    #[test]
    fn test_integer_sqrt() {
        assert_eq!(integer_sqrt(4), 2);
        assert_eq!(integer_sqrt(9), 3);
        assert_eq!(integer_sqrt(100), 10);
        assert_eq!(integer_sqrt(2), 1); // Floor
        assert_eq!(integer_sqrt(3), 1);
        assert_eq!(integer_sqrt(5), 2);
    }

    #[test]
    fn test_impermanent_loss() {
        // 2x price increase
        let il =
            LiquidityPool::calculate_impermanent_loss(PRICE_SCALE as i128, 2 * PRICE_SCALE as i128);
        // IL for 2x should be about 5.7% = 570 bps
        assert!(il > 500 && il < 600);

        // No price change = no IL
        let il_none =
            LiquidityPool::calculate_impermanent_loss(PRICE_SCALE as i128, PRICE_SCALE as i128);
        assert!(il_none < 10);
    }
}
