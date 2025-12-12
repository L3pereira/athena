//! Trading account entity with balances, positions, and margin management.

use crate::domain::entities::{Loan, Position, PositionSide};
use crate::domain::services::{AccountMarginCalculator, MarginStatus};
use crate::domain::value_objects::{Price, Quantity, Rate, Symbol, Timestamp, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type AccountId = Uuid;

/// Account status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AccountStatus {
    /// Normal trading
    #[default]
    Active,
    /// Margin warning - approaching liquidation
    MarginCall,
    /// Being liquidated
    Liquidating,
    /// Frozen - no trading allowed
    Frozen,
}

/// Margin mode for the account
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarginMode {
    /// Cross margin - all positions share margin
    #[default]
    Cross,
    /// Isolated margin - each position has separate margin
    Isolated,
}

/// Fee schedule for an account (VIP tier-based discounts)
/// Discounts are stored as basis points (10000 = 100% = 1.0)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FeeSchedule {
    /// VIP tier level (0 = regular, 1-9 = VIP levels)
    pub tier: u8,
    /// Maker fee discount multiplier in bps (10000 = no discount, 8000 = 20% off)
    pub maker_discount_bps: i64,
    /// Taker fee discount multiplier in bps
    pub taker_discount_bps: i64,
}

impl Default for FeeSchedule {
    fn default() -> Self {
        Self {
            tier: 0,
            maker_discount_bps: 10_000, // 100% = no discount
            taker_discount_bps: 10_000,
        }
    }
}

impl FeeSchedule {
    /// Create a VIP tier with specified discount multipliers (in basis points)
    pub fn vip(tier: u8, maker_discount_bps: i64, taker_discount_bps: i64) -> Self {
        Self {
            tier,
            maker_discount_bps,
            taker_discount_bps,
        }
    }

    /// Standard VIP tiers (similar to major exchanges)
    pub fn tier_1() -> Self {
        Self::vip(1, 9000, 9500) // 90%, 95%
    }
    pub fn tier_2() -> Self {
        Self::vip(2, 8000, 9000) // 80%, 90%
    }
    pub fn tier_3() -> Self {
        Self::vip(3, 7000, 8500) // 70%, 85%
    }
    pub fn tier_4() -> Self {
        Self::vip(4, 6000, 8000) // 60%, 80%
    }
    pub fn tier_5() -> Self {
        Self::vip(5, 5000, 7500) // 50%, 75%
    }
    pub fn market_maker() -> Self {
        Self::vip(9, -5000, 5000) // -50% (rebate), 50%
    }

    /// Create fee schedule from tier number
    pub fn from_tier(tier: u8) -> Self {
        match tier {
            1 => Self::tier_1(),
            2 => Self::tier_2(),
            3 => Self::tier_3(),
            4 => Self::tier_4(),
            5 => Self::tier_5(),
            9 => Self::market_maker(),
            _ => Self::default(),
        }
    }

    /// Apply discount to base fee rates (in bps)
    /// Returns (effective_maker_rate, effective_taker_rate) in bps
    pub fn apply(&self, maker_rate_bps: i64, taker_rate_bps: i64) -> (i64, i64) {
        (
            (maker_rate_bps * self.maker_discount_bps) / 10_000,
            (taker_rate_bps * self.taker_discount_bps) / 10_000,
        )
    }

    /// Apply discount to Rate types
    pub fn apply_rates(&self, maker_rate: Rate, taker_rate: Rate) -> (Rate, Rate) {
        (
            Rate::from_bps((maker_rate.bps() * self.maker_discount_bps) / 10_000),
            Rate::from_bps((taker_rate.bps() * self.taker_discount_bps) / 10_000),
        )
    }
}

/// Balance for a single asset
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AssetBalance {
    /// Available for trading
    pub available: Value,
    /// Locked in orders or as margin
    pub locked: Value,
    /// Borrowed amount (for short selling)
    pub borrowed: Value,
    /// Interest owed on borrowed amount
    pub interest: Value,
}

impl Default for AssetBalance {
    fn default() -> Self {
        Self {
            available: Value::ZERO,
            locked: Value::ZERO,
            borrowed: Value::ZERO,
            interest: Value::ZERO,
        }
    }
}

impl AssetBalance {
    /// Total balance (available + locked)
    pub fn total(&self) -> Value {
        self.available + self.locked
    }

    /// Net balance (total - borrowed - interest)
    pub fn net(&self) -> Value {
        Value::from_raw(self.total().raw() - self.borrowed.raw() - self.interest.raw())
    }
}

/// A trading account with balances, positions, and margin tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub owner_id: String,
    pub status: AccountStatus,
    pub margin_mode: MarginMode,

    /// Asset balances
    balances: HashMap<String, AssetBalance>,

    /// Open positions by symbol
    positions: HashMap<Symbol, Position>,

    /// Active loans (borrowed assets for short selling)
    loans: HashMap<String, Loan>,

    /// Margin configuration (in basis points)
    pub initial_margin_rate: Rate,
    pub maintenance_margin_rate: Rate,

    /// Fee schedule (VIP tier discounts)
    pub fee_schedule: FeeSchedule,

    /// Timestamps
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Account {
    /// Create a new account
    pub fn new(owner_id: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            owner_id: owner_id.into(),
            status: AccountStatus::Active,
            margin_mode: MarginMode::Cross,
            balances: HashMap::new(),
            positions: HashMap::new(),
            loans: HashMap::new(),
            initial_margin_rate: Rate::from_bps(1000), // 10%
            maintenance_margin_rate: Rate::from_bps(500), // 5%
            fee_schedule: FeeSchedule::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create with specific margin rates
    pub fn with_margin_rates(mut self, initial: Rate, maintenance: Rate) -> Self {
        self.initial_margin_rate = initial;
        self.maintenance_margin_rate = maintenance;
        self
    }

    /// Set fee schedule (VIP tier)
    pub fn with_fee_schedule(mut self, schedule: FeeSchedule) -> Self {
        self.fee_schedule = schedule;
        self
    }

    /// Calculate effective fee rates for this account given base rates (in bps)
    pub fn effective_fees(&self, base_maker_bps: i64, base_taker_bps: i64) -> (i64, i64) {
        self.fee_schedule.apply(base_maker_bps, base_taker_bps)
    }

    /// Calculate effective fee rates using Rate types
    pub fn effective_fee_rates(&self, base_maker: Rate, base_taker: Rate) -> (Rate, Rate) {
        self.fee_schedule.apply_rates(base_maker, base_taker)
    }

    // ========== Balance Operations ==========

    /// Deposit funds
    pub fn deposit(&mut self, asset: &str, amount: Value) {
        let balance = self.balances.entry(asset.to_string()).or_default();
        balance.available = balance.available + amount;
        self.updated_at = chrono::Utc::now();
    }

    /// Withdraw funds
    pub fn withdraw(&mut self, asset: &str, amount: Value) -> Result<(), AccountError> {
        let balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        if balance.available.raw() < amount.raw() {
            return Err(AccountError::InsufficientBalance);
        }

        balance.available = Value::from_raw(balance.available.raw() - amount.raw());
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Get balance for an asset
    pub fn balance(&self, asset: &str) -> AssetBalance {
        self.balances.get(asset).copied().unwrap_or_default()
    }

    /// Get all balances as an iterator
    pub fn all_balances(&self) -> impl Iterator<Item = (&String, &AssetBalance)> {
        self.balances.iter()
    }

    /// Lock funds for an order
    pub fn lock(&mut self, asset: &str, amount: Value) -> Result<(), AccountError> {
        let balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        if balance.available.raw() < amount.raw() {
            return Err(AccountError::InsufficientBalance);
        }

        balance.available = Value::from_raw(balance.available.raw() - amount.raw());
        balance.locked = balance.locked + amount;
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Unlock funds (order cancelled)
    pub fn unlock(&mut self, asset: &str, amount: Value) {
        if let Some(balance) = self.balances.get_mut(asset) {
            let unlock_raw = amount.raw().min(balance.locked.raw());
            balance.locked = Value::from_raw(balance.locked.raw() - unlock_raw);
            balance.available = Value::from_raw(balance.available.raw() + unlock_raw);
            self.updated_at = chrono::Utc::now();
        }
    }

    // ========== Borrowing Operations ==========

    /// Borrow an asset for short selling
    pub fn borrow(
        &mut self,
        asset: &str,
        amount: Value,
        interest_rate: Rate,
        collateral_asset: &str,
        collateral_amount: Value,
        now: Timestamp,
    ) -> Result<(), AccountError> {
        let collateral_balance = self
            .balances
            .get_mut(collateral_asset)
            .ok_or(AccountError::InsufficientCollateral)?;

        if collateral_balance.available.raw() < collateral_amount.raw() {
            return Err(AccountError::InsufficientCollateral);
        }

        collateral_balance.available =
            Value::from_raw(collateral_balance.available.raw() - collateral_amount.raw());
        collateral_balance.locked = collateral_balance.locked + collateral_amount;

        let loan = Loan::new(asset, amount, interest_rate, collateral_amount, now);

        let asset_balance = self.balances.entry(asset.to_string()).or_default();
        asset_balance.available = asset_balance.available + amount;
        asset_balance.borrowed = asset_balance.borrowed + amount;

        self.loans.insert(asset.to_string(), loan);
        self.updated_at = now;

        Ok(())
    }

    /// Repay a loan
    pub fn repay_loan(
        &mut self,
        asset: &str,
        amount: Value,
        collateral_asset: &str,
        now: Timestamp,
    ) -> Result<Value, AccountError> {
        let loan = self
            .loans
            .get_mut(asset)
            .ok_or(AccountError::NoActiveLoan)?;

        loan.accrue_interest(now);

        let asset_balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        let repay_raw = amount.raw().min(loan.total_owed().raw());
        let repay_amount = Value::from_raw(repay_raw);
        if asset_balance.available.raw() < repay_raw {
            return Err(AccountError::InsufficientBalance);
        }

        asset_balance.available = Value::from_raw(asset_balance.available.raw() - repay_raw);
        asset_balance.borrowed = Value::from_raw((asset_balance.borrowed.raw() - repay_raw).max(0));

        let remaining = loan.repay(repay_amount);

        if loan.is_repaid() {
            let collateral = loan.collateral;
            if let Some(coll_balance) = self.balances.get_mut(collateral_asset) {
                coll_balance.locked = Value::from_raw(coll_balance.locked.raw() - collateral.raw());
                coll_balance.available = coll_balance.available + collateral;
            }
            self.loans.remove(asset);
        }

        self.updated_at = now;
        Ok(remaining)
    }

    /// Get active loan for an asset
    pub fn loan(&self, asset: &str) -> Option<&Loan> {
        self.loans.get(asset)
    }

    /// Check if account has borrowed an asset
    pub fn has_borrowed(&self, asset: &str) -> bool {
        self.loans.contains_key(asset)
    }

    // ========== Position Operations ==========

    /// Open or increase a position
    pub fn open_position(
        &mut self,
        symbol: Symbol,
        side: PositionSide,
        quantity: Quantity,
        price: Price,
        margin: Value,
        now: Timestamp,
    ) {
        if let Some(pos) = self.positions.get_mut(&symbol) {
            if pos.side == side {
                pos.increase(quantity, price, margin, now);
            } else {
                let close_qty_raw = quantity.raw().min(pos.quantity.raw());
                pos.decrease(Quantity::from_raw(close_qty_raw), price, now);

                if pos.is_closed() {
                    self.positions.remove(&symbol);
                }

                let remaining = quantity.raw() - close_qty_raw;
                if remaining > 0 {
                    let new_pos = Position::new(
                        symbol.clone(),
                        side,
                        Quantity::from_raw(remaining),
                        price,
                        margin,
                        now,
                    );
                    self.positions.insert(symbol, new_pos);
                }
            }
        } else {
            let pos = Position::new(symbol.clone(), side, quantity, price, margin, now);
            self.positions.insert(symbol, pos);
        }
        self.updated_at = now;
    }

    /// Close or reduce a position
    pub fn close_position(
        &mut self,
        symbol: &Symbol,
        quantity: Quantity,
        price: Price,
        now: Timestamp,
    ) -> Result<Value, AccountError> {
        let pos = self
            .positions
            .get_mut(symbol)
            .ok_or(AccountError::NoPosition)?;

        let pnl = pos.decrease(quantity, price, now);

        if pos.is_closed() {
            self.positions.remove(symbol);
        }

        self.updated_at = now;
        Ok(pnl)
    }

    /// Get position for a symbol
    pub fn position(&self, symbol: &Symbol) -> Option<&Position> {
        self.positions.get(symbol)
    }

    /// Get all positions
    pub fn positions(&self) -> impl Iterator<Item = &Position> {
        self.positions.values()
    }

    /// Update mark prices for all positions
    pub fn update_mark_prices(&mut self, prices: &HashMap<Symbol, Price>, now: Timestamp) {
        for (symbol, price) in prices {
            if let Some(pos) = self.positions.get_mut(symbol) {
                pos.update_mark_price(*price, now);
            }
        }
        self.update_status();
        self.updated_at = now;
    }

    // ========== Margin Calculations (delegated to service) ==========

    /// Total equity (all assets at current prices)
    pub fn equity(&self) -> Value {
        let balance_equity: i128 = self.balances.values().map(|b| b.net().raw()).sum();
        let unrealized_pnl = self.unrealized_pnl();
        Value::from_raw(balance_equity + unrealized_pnl.raw())
    }

    /// Total unrealized P&L across all positions
    pub fn unrealized_pnl(&self) -> Value {
        let calc = AccountMarginCalculator::default();
        calc.total_unrealized_pnl(self.positions.values())
    }

    /// Total margin used
    pub fn used_margin(&self) -> Value {
        let calc = AccountMarginCalculator::default();
        calc.total_used_margin(self.positions.values())
    }

    /// Available margin for new positions
    pub fn available_margin(&self) -> Value {
        Value::from_raw((self.equity().raw() - self.used_margin().raw()).max(0))
    }

    /// Margin ratio (equity / maintenance margin required)
    /// Returns ratio scaled by PRICE_SCALE (e.g., 1.0 = PRICE_SCALE)
    pub fn margin_ratio(&self) -> i64 {
        let calc = AccountMarginCalculator::default();
        calc.margin_ratio(
            self.equity(),
            self.positions.values(),
            self.maintenance_margin_rate,
        )
    }

    /// Check if account has sufficient margin for a new order
    pub fn has_sufficient_margin(&self, required: Value) -> bool {
        self.available_margin().raw() >= required.raw()
    }

    /// Calculate required margin for a new position
    pub fn calculate_required_margin(&self, quantity: Quantity, price: Price) -> Value {
        let notional = price.mul_qty(quantity);
        self.initial_margin_rate.apply_to_value(notional)
    }

    /// Update account status based on margin levels
    fn update_status(&mut self) {
        let calc = AccountMarginCalculator::default();
        let status = calc.determine_status(self.margin_ratio(), !self.positions.is_empty());
        self.status = match status {
            MarginStatus::Healthy => AccountStatus::Active,
            MarginStatus::MarginCall => AccountStatus::MarginCall,
            MarginStatus::Liquidating => AccountStatus::Liquidating,
        };
    }

    /// Get positions that should be liquidated
    pub fn liquidatable_positions(&self) -> Vec<&Position> {
        let calc = AccountMarginCalculator::default();
        calc.liquidatable_positions(self.positions.values(), self.maintenance_margin_rate)
    }
}

/// Account operation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountError {
    InsufficientBalance,
    InsufficientCollateral,
    InsufficientMargin,
    NoActiveLoan,
    NoPosition,
    AccountFrozen,
    AccountLiquidating,
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientBalance => write!(f, "Insufficient balance"),
            Self::InsufficientCollateral => write!(f, "Insufficient collateral"),
            Self::InsufficientMargin => write!(f, "Insufficient margin"),
            Self::NoActiveLoan => write!(f, "No active loan for this asset"),
            Self::NoPosition => write!(f, "No position for this symbol"),
            Self::AccountFrozen => write!(f, "Account is frozen"),
            Self::AccountLiquidating => write!(f, "Account is being liquidated"),
        }
    }
}

impl std::error::Error for AccountError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PRICE_SCALE;
    use crate::domain::services::{AccountMarginCalculator, MarginCalculator};

    /// Helper to create Value from integer (scales by PRICE_SCALE)
    fn val(n: i64) -> Value {
        Value::from_raw(n as i128 * PRICE_SCALE as i128)
    }

    #[test]
    fn test_deposit_withdraw() {
        let mut account = Account::new("user1");

        account.deposit("USDT", val(10000));
        assert_eq!(account.balance("USDT").available.raw(), val(10000).raw());

        account.withdraw("USDT", val(3000)).unwrap();
        assert_eq!(account.balance("USDT").available.raw(), val(7000).raw());

        assert!(account.withdraw("USDT", val(8000)).is_err());
    }

    #[test]
    fn test_lock_unlock() {
        let mut account = Account::new("user1");
        account.deposit("BTC", val(10));

        account.lock("BTC", val(3)).unwrap();
        assert_eq!(account.balance("BTC").available.raw(), val(7).raw());
        assert_eq!(account.balance("BTC").locked.raw(), val(3).raw());

        account.unlock("BTC", val(2));
        assert_eq!(account.balance("BTC").available.raw(), val(9).raw());
        assert_eq!(account.balance("BTC").locked.raw(), val(1).raw());
    }

    #[test]
    fn test_borrow_for_short() {
        let mut account = Account::new("user1");
        let now = chrono::Utc::now();

        account.deposit("USDT", val(60000));

        account
            .borrow("BTC", val(1), Rate::from_bps(500), "USDT", val(55000), now)
            .unwrap();

        let btc_balance = account.balance("BTC");
        assert_eq!(btc_balance.available.raw(), val(1).raw());
        assert_eq!(btc_balance.borrowed.raw(), val(1).raw());

        let usdt_balance = account.balance("USDT");
        assert_eq!(usdt_balance.available.raw(), val(5000).raw());
        assert_eq!(usdt_balance.locked.raw(), val(55000).raw());
    }

    #[test]
    fn test_short_position_pnl() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = chrono::Utc::now();

        let mut pos = Position::new(
            symbol,
            PositionSide::Short,
            Quantity::from_int(1),
            Price::from_int(50000),
            val(5000),
            now,
        );

        let calc = AccountMarginCalculator::default();

        pos.update_mark_price(Price::from_int(45000), now);
        assert_eq!(
            calc.position_calculator().unrealized_pnl(&pos).raw(),
            val(5000).raw()
        );

        pos.update_mark_price(Price::from_int(55000), now);
        assert_eq!(
            calc.position_calculator().unrealized_pnl(&pos).raw(),
            val(-5000).raw()
        );
    }

    #[test]
    fn test_short_liquidation_price() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = chrono::Utc::now();

        let pos = Position::new(
            symbol,
            PositionSide::Short,
            Quantity::from_int(1),
            Price::from_int(50000),
            val(5000),
            now,
        );

        let calc = AccountMarginCalculator::default();
        let liq_price = calc
            .position_calculator()
            .liquidation_price(&pos, Rate::from_bps(500)); // 5%
        assert_eq!(liq_price.raw(), 52500 * PRICE_SCALE);
    }

    #[test]
    fn test_full_short_flow() {
        let mut account =
            Account::new("trader1").with_margin_rates(Rate::from_bps(1000), Rate::from_bps(500)); // 10%, 5%
        let now = chrono::Utc::now();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        account.deposit("USDT", val(100000));

        account
            .borrow("BTC", val(1), Rate::from_bps(500), "USDT", val(60000), now)
            .unwrap();

        account.lock("USDT", val(5000)).unwrap();
        account.open_position(
            symbol.clone(),
            PositionSide::Short,
            Quantity::from_int(1),
            Price::from_int(50000),
            val(5000),
            now,
        );

        account.withdraw("BTC", val(1)).unwrap();

        let mut prices = HashMap::new();
        prices.insert(symbol.clone(), Price::from_int(40000));
        account.update_mark_prices(&prices, now);

        assert_eq!(account.unrealized_pnl().raw(), val(10000).raw());

        let pnl = account
            .close_position(&symbol, Quantity::from_int(1), Price::from_int(40000), now)
            .unwrap();
        assert_eq!(pnl.raw(), val(10000).raw());
    }

    #[test]
    fn test_fee_schedule_default() {
        let schedule = FeeSchedule::default();
        assert_eq!(schedule.tier, 0);
        assert_eq!(schedule.maker_discount_bps, 10_000); // 100%
        assert_eq!(schedule.taker_discount_bps, 10_000);
    }

    #[test]
    fn test_fee_schedule_vip_tiers() {
        let tier1 = FeeSchedule::tier_1();
        assert_eq!(tier1.tier, 1);
        assert_eq!(tier1.maker_discount_bps, 9000); // 90%
        assert_eq!(tier1.taker_discount_bps, 9500); // 95%

        let tier5 = FeeSchedule::tier_5();
        assert_eq!(tier5.tier, 5);
        assert_eq!(tier5.maker_discount_bps, 5000); // 50%
        assert_eq!(tier5.taker_discount_bps, 7500); // 75%
    }

    #[test]
    fn test_fee_schedule_market_maker_rebate() {
        let mm = FeeSchedule::market_maker();
        assert_eq!(mm.tier, 9);
        assert_eq!(mm.maker_discount_bps, -5000); // -50% (rebate)
        assert_eq!(mm.taker_discount_bps, 5000); // 50%
    }

    #[test]
    fn test_account_effective_fees() {
        // Base rates in bps: maker 1 bps, taker 2 bps
        let base_maker_bps = 1;
        let base_taker_bps = 2;

        let account = Account::new("user1");
        let (effective_maker, effective_taker) =
            account.effective_fees(base_maker_bps, base_taker_bps);
        assert_eq!(effective_maker, 1); // 1 bps
        assert_eq!(effective_taker, 2); // 2 bps

        let vip1 = Account::new("vip1").with_fee_schedule(FeeSchedule::tier_1());
        let (eff_maker, eff_taker) = vip1.effective_fees(base_maker_bps, base_taker_bps);
        // maker: 1 * 9000 / 10000 = 0 (integer division)
        // For proper fee testing, use larger base rates
        assert_eq!(eff_maker, 0);
        assert_eq!(eff_taker, 1);
    }

    #[test]
    fn test_fee_calculation_with_rates() {
        // Use Rate types for more accurate fee calculations
        let base_maker = Rate::from_bps(10); // 0.1%
        let base_taker = Rate::from_bps(20); // 0.2%

        let regular = Account::new("regular");
        let (maker_rate, taker_rate) = regular.effective_fee_rates(base_maker, base_taker);
        assert_eq!(maker_rate.bps(), 10);
        assert_eq!(taker_rate.bps(), 20);

        let vip5 = Account::new("vip5").with_fee_schedule(FeeSchedule::tier_5());
        let (maker_rate, taker_rate) = vip5.effective_fee_rates(base_maker, base_taker);
        assert_eq!(maker_rate.bps(), 5); // 50% of 10
        assert_eq!(taker_rate.bps(), 15); // 75% of 20

        let mm = Account::new("mm").with_fee_schedule(FeeSchedule::market_maker());
        let (maker_rate, taker_rate) = mm.effective_fee_rates(base_maker, base_taker);
        assert_eq!(maker_rate.bps(), -5); // -50% of 10 = rebate
        assert_eq!(taker_rate.bps(), 10); // 50% of 20
    }
}
