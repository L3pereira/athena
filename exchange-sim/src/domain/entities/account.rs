//! Trading account entity with balances, positions, and margin management.

use crate::domain::entities::{Loan, Position, PositionSide};
use crate::domain::services::{AccountMarginCalculator, MarginStatus};
use crate::domain::value_objects::{Price, Quantity, Symbol, Timestamp};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type AccountId = Uuid;

/// Account status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    /// Normal trading
    Active,
    /// Margin warning - approaching liquidation
    MarginCall,
    /// Being liquidated
    Liquidating,
    /// Frozen - no trading allowed
    Frozen,
}

impl Default for AccountStatus {
    fn default() -> Self {
        Self::Active
    }
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FeeSchedule {
    /// VIP tier level (0 = regular, 1-9 = VIP levels)
    pub tier: u8,
    /// Maker fee discount multiplier (1.0 = no discount, 0.8 = 20% off)
    pub maker_discount: Decimal,
    /// Taker fee discount multiplier
    pub taker_discount: Decimal,
}

impl Default for FeeSchedule {
    fn default() -> Self {
        Self {
            tier: 0,
            maker_discount: Decimal::ONE,
            taker_discount: Decimal::ONE,
        }
    }
}

impl FeeSchedule {
    /// Create a VIP tier with specified discounts
    pub fn vip(tier: u8, maker_discount: Decimal, taker_discount: Decimal) -> Self {
        Self {
            tier,
            maker_discount,
            taker_discount,
        }
    }

    /// Standard VIP tiers (similar to major exchanges)
    pub fn tier_1() -> Self {
        Self::vip(1, dec!(0.90), dec!(0.95))
    }
    pub fn tier_2() -> Self {
        Self::vip(2, dec!(0.80), dec!(0.90))
    }
    pub fn tier_3() -> Self {
        Self::vip(3, dec!(0.70), dec!(0.85))
    }
    pub fn tier_4() -> Self {
        Self::vip(4, dec!(0.60), dec!(0.80))
    }
    pub fn tier_5() -> Self {
        Self::vip(5, dec!(0.50), dec!(0.75))
    }
    pub fn market_maker() -> Self {
        Self::vip(9, dec!(-0.50), dec!(0.50))
    }

    /// Create fee schedule from tier number
    /// This provides a single source of truth for tier-to-schedule mapping.
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

    /// Apply discount to base fee rates
    pub fn apply(&self, maker_rate: Decimal, taker_rate: Decimal) -> (Decimal, Decimal) {
        (
            maker_rate * self.maker_discount,
            taker_rate * self.taker_discount,
        )
    }
}

/// Balance for a single asset
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AssetBalance {
    /// Available for trading
    pub available: Decimal,
    /// Locked in orders or as margin
    pub locked: Decimal,
    /// Borrowed amount (for short selling)
    pub borrowed: Decimal,
    /// Interest owed on borrowed amount
    pub interest: Decimal,
}

impl Default for AssetBalance {
    fn default() -> Self {
        Self {
            available: Decimal::ZERO,
            locked: Decimal::ZERO,
            borrowed: Decimal::ZERO,
            interest: Decimal::ZERO,
        }
    }
}

impl AssetBalance {
    /// Total balance (available + locked)
    pub fn total(&self) -> Decimal {
        self.available + self.locked
    }

    /// Net balance (total - borrowed - interest)
    pub fn net(&self) -> Decimal {
        self.total() - self.borrowed - self.interest
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

    /// Margin configuration
    pub initial_margin_rate: Decimal,
    pub maintenance_margin_rate: Decimal,

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
            initial_margin_rate: dec!(0.10),
            maintenance_margin_rate: dec!(0.05),
            fee_schedule: FeeSchedule::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create with specific margin rates
    pub fn with_margin_rates(mut self, initial: Decimal, maintenance: Decimal) -> Self {
        self.initial_margin_rate = initial;
        self.maintenance_margin_rate = maintenance;
        self
    }

    /// Set fee schedule (VIP tier)
    pub fn with_fee_schedule(mut self, schedule: FeeSchedule) -> Self {
        self.fee_schedule = schedule;
        self
    }

    /// Calculate effective fee rates for this account given base rates
    pub fn effective_fees(&self, base_maker: Decimal, base_taker: Decimal) -> (Decimal, Decimal) {
        self.fee_schedule.apply(base_maker, base_taker)
    }

    // ========== Balance Operations ==========

    /// Deposit funds
    pub fn deposit(&mut self, asset: &str, amount: Decimal) {
        let balance = self.balances.entry(asset.to_string()).or_default();
        balance.available += amount;
        self.updated_at = chrono::Utc::now();
    }

    /// Withdraw funds
    pub fn withdraw(&mut self, asset: &str, amount: Decimal) -> Result<(), AccountError> {
        let balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        if balance.available < amount {
            return Err(AccountError::InsufficientBalance);
        }

        balance.available -= amount;
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
    pub fn lock(&mut self, asset: &str, amount: Decimal) -> Result<(), AccountError> {
        let balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        if balance.available < amount {
            return Err(AccountError::InsufficientBalance);
        }

        balance.available -= amount;
        balance.locked += amount;
        self.updated_at = chrono::Utc::now();
        Ok(())
    }

    /// Unlock funds (order cancelled)
    pub fn unlock(&mut self, asset: &str, amount: Decimal) {
        if let Some(balance) = self.balances.get_mut(asset) {
            let unlock_amount = amount.min(balance.locked);
            balance.locked -= unlock_amount;
            balance.available += unlock_amount;
            self.updated_at = chrono::Utc::now();
        }
    }

    // ========== Borrowing Operations ==========

    /// Borrow an asset for short selling
    pub fn borrow(
        &mut self,
        asset: &str,
        amount: Decimal,
        interest_rate: Decimal,
        collateral_asset: &str,
        collateral_amount: Decimal,
        now: Timestamp,
    ) -> Result<(), AccountError> {
        let collateral_balance = self
            .balances
            .get_mut(collateral_asset)
            .ok_or(AccountError::InsufficientCollateral)?;

        if collateral_balance.available < collateral_amount {
            return Err(AccountError::InsufficientCollateral);
        }

        collateral_balance.available -= collateral_amount;
        collateral_balance.locked += collateral_amount;

        let loan = Loan::new(asset, amount, interest_rate, collateral_amount, now);

        let asset_balance = self.balances.entry(asset.to_string()).or_default();
        asset_balance.available += amount;
        asset_balance.borrowed += amount;

        self.loans.insert(asset.to_string(), loan);
        self.updated_at = now;

        Ok(())
    }

    /// Repay a loan
    pub fn repay_loan(
        &mut self,
        asset: &str,
        amount: Decimal,
        collateral_asset: &str,
        now: Timestamp,
    ) -> Result<Decimal, AccountError> {
        let loan = self
            .loans
            .get_mut(asset)
            .ok_or(AccountError::NoActiveLoan)?;

        loan.accrue_interest(now);

        let asset_balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        let repay_amount = amount.min(loan.total_owed());
        if asset_balance.available < repay_amount {
            return Err(AccountError::InsufficientBalance);
        }

        asset_balance.available -= repay_amount;
        asset_balance.borrowed = (asset_balance.borrowed - repay_amount).max(Decimal::ZERO);

        let remaining = loan.repay(repay_amount);

        if loan.is_repaid() {
            let collateral = loan.collateral;
            if let Some(coll_balance) = self.balances.get_mut(collateral_asset) {
                coll_balance.locked -= collateral;
                coll_balance.available += collateral;
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
        margin: Decimal,
        now: Timestamp,
    ) {
        if let Some(pos) = self.positions.get_mut(&symbol) {
            if pos.side == side {
                pos.increase(quantity, price, margin, now);
            } else {
                let close_qty = quantity.inner().min(pos.quantity.inner());
                pos.decrease(Quantity::from(close_qty), price, now);

                if pos.is_closed() {
                    self.positions.remove(&symbol);
                }

                let remaining = quantity.inner() - close_qty;
                if remaining > Decimal::ZERO {
                    let new_pos = Position::new(
                        symbol.clone(),
                        side,
                        Quantity::from(remaining),
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
    ) -> Result<Decimal, AccountError> {
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
    pub fn equity(&self) -> Decimal {
        let balance_equity: Decimal = self.balances.values().map(|b| b.net()).sum();
        let unrealized_pnl = self.unrealized_pnl();
        balance_equity + unrealized_pnl
    }

    /// Total unrealized P&L across all positions
    pub fn unrealized_pnl(&self) -> Decimal {
        let calc = AccountMarginCalculator::default();
        calc.total_unrealized_pnl(self.positions.values())
    }

    /// Total margin used
    pub fn used_margin(&self) -> Decimal {
        let calc = AccountMarginCalculator::default();
        calc.total_used_margin(self.positions.values())
    }

    /// Available margin for new positions
    pub fn available_margin(&self) -> Decimal {
        (self.equity() - self.used_margin()).max(Decimal::ZERO)
    }

    /// Margin ratio (equity / maintenance margin required)
    pub fn margin_ratio(&self) -> Decimal {
        let calc = AccountMarginCalculator::default();
        calc.margin_ratio(
            self.equity(),
            self.positions.values(),
            self.maintenance_margin_rate,
        )
    }

    /// Check if account has sufficient margin for a new order
    pub fn has_sufficient_margin(&self, required: Decimal) -> bool {
        self.available_margin() >= required
    }

    /// Calculate required margin for a new position
    pub fn calculate_required_margin(&self, quantity: Quantity, price: Price) -> Decimal {
        quantity.inner() * price.inner() * self.initial_margin_rate
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
    use crate::domain::services::{AccountMarginCalculator, MarginCalculator};
    use rust_decimal_macros::dec;

    #[test]
    fn test_deposit_withdraw() {
        let mut account = Account::new("user1");

        account.deposit("USDT", dec!(10000));
        assert_eq!(account.balance("USDT").available, dec!(10000));

        account.withdraw("USDT", dec!(3000)).unwrap();
        assert_eq!(account.balance("USDT").available, dec!(7000));

        assert!(account.withdraw("USDT", dec!(8000)).is_err());
    }

    #[test]
    fn test_lock_unlock() {
        let mut account = Account::new("user1");
        account.deposit("BTC", dec!(10));

        account.lock("BTC", dec!(3)).unwrap();
        assert_eq!(account.balance("BTC").available, dec!(7));
        assert_eq!(account.balance("BTC").locked, dec!(3));

        account.unlock("BTC", dec!(2));
        assert_eq!(account.balance("BTC").available, dec!(9));
        assert_eq!(account.balance("BTC").locked, dec!(1));
    }

    #[test]
    fn test_borrow_for_short() {
        let mut account = Account::new("user1");
        let now = chrono::Utc::now();

        account.deposit("USDT", dec!(60000));

        account
            .borrow("BTC", dec!(1), dec!(0.05), "USDT", dec!(55000), now)
            .unwrap();

        let btc_balance = account.balance("BTC");
        assert_eq!(btc_balance.available, dec!(1));
        assert_eq!(btc_balance.borrowed, dec!(1));

        let usdt_balance = account.balance("USDT");
        assert_eq!(usdt_balance.available, dec!(5000));
        assert_eq!(usdt_balance.locked, dec!(55000));
    }

    #[test]
    fn test_short_position_pnl() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = chrono::Utc::now();

        let mut pos = Position::new(
            symbol,
            PositionSide::Short,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            dec!(5000),
            now,
        );

        let calc = AccountMarginCalculator::default();

        pos.update_mark_price(Price::from(dec!(45000)), now);
        assert_eq!(calc.position_calculator().unrealized_pnl(&pos), dec!(5000));

        pos.update_mark_price(Price::from(dec!(55000)), now);
        assert_eq!(calc.position_calculator().unrealized_pnl(&pos), dec!(-5000));
    }

    #[test]
    fn test_short_liquidation_price() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = chrono::Utc::now();

        let pos = Position::new(
            symbol,
            PositionSide::Short,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            dec!(5000),
            now,
        );

        let calc = AccountMarginCalculator::default();
        let liq_price = calc
            .position_calculator()
            .liquidation_price(&pos, dec!(0.05));
        assert_eq!(liq_price, Price::from(dec!(52500)));
    }

    #[test]
    fn test_full_short_flow() {
        let mut account = Account::new("trader1").with_margin_rates(dec!(0.10), dec!(0.05));
        let now = chrono::Utc::now();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        account.deposit("USDT", dec!(100000));

        account
            .borrow("BTC", dec!(1), dec!(0.05), "USDT", dec!(60000), now)
            .unwrap();

        account.lock("USDT", dec!(5000)).unwrap();
        account.open_position(
            symbol.clone(),
            PositionSide::Short,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            dec!(5000),
            now,
        );

        account.withdraw("BTC", dec!(1)).unwrap();

        let mut prices = HashMap::new();
        prices.insert(symbol.clone(), Price::from(dec!(40000)));
        account.update_mark_prices(&prices, now);

        assert_eq!(account.unrealized_pnl(), dec!(10000));

        let pnl = account
            .close_position(
                &symbol,
                Quantity::from(dec!(1)),
                Price::from(dec!(40000)),
                now,
            )
            .unwrap();
        assert_eq!(pnl, dec!(10000));
    }

    #[test]
    fn test_fee_schedule_default() {
        let schedule = FeeSchedule::default();
        assert_eq!(schedule.tier, 0);
        assert_eq!(schedule.maker_discount, Decimal::ONE);
        assert_eq!(schedule.taker_discount, Decimal::ONE);
    }

    #[test]
    fn test_fee_schedule_vip_tiers() {
        let tier1 = FeeSchedule::tier_1();
        assert_eq!(tier1.tier, 1);
        assert_eq!(tier1.maker_discount, dec!(0.90));
        assert_eq!(tier1.taker_discount, dec!(0.95));

        let tier5 = FeeSchedule::tier_5();
        assert_eq!(tier5.tier, 5);
        assert_eq!(tier5.maker_discount, dec!(0.50));
        assert_eq!(tier5.taker_discount, dec!(0.75));
    }

    #[test]
    fn test_fee_schedule_market_maker_rebate() {
        let mm = FeeSchedule::market_maker();
        assert_eq!(mm.tier, 9);
        assert_eq!(mm.maker_discount, dec!(-0.50));
        assert_eq!(mm.taker_discount, dec!(0.50));
    }

    #[test]
    fn test_account_effective_fees() {
        let base_maker = dec!(0.0001);
        let base_taker = dec!(0.0002);

        let account = Account::new("user1");
        let (effective_maker, effective_taker) = account.effective_fees(base_maker, base_taker);
        assert_eq!(effective_maker, dec!(0.0001));
        assert_eq!(effective_taker, dec!(0.0002));

        let vip1 = Account::new("vip1").with_fee_schedule(FeeSchedule::tier_1());
        let (eff_maker, eff_taker) = vip1.effective_fees(base_maker, base_taker);
        assert_eq!(eff_maker, dec!(0.00009));
        assert_eq!(eff_taker, dec!(0.00019));

        let vip5 = Account::new("vip5").with_fee_schedule(FeeSchedule::tier_5());
        let (eff_maker, eff_taker) = vip5.effective_fees(base_maker, base_taker);
        assert_eq!(eff_maker, dec!(0.00005));
        assert_eq!(eff_taker, dec!(0.00015));
    }

    #[test]
    fn test_market_maker_gets_rebate() {
        let base_maker = dec!(0.0001);
        let base_taker = dec!(0.0002);

        let mm = Account::new("mm1").with_fee_schedule(FeeSchedule::market_maker());
        let (eff_maker, eff_taker) = mm.effective_fees(base_maker, base_taker);

        assert_eq!(eff_maker, dec!(-0.00005));
        assert!(eff_maker < Decimal::ZERO);
        assert_eq!(eff_taker, dec!(0.0001));
    }

    #[test]
    fn test_fee_calculation_on_trade() {
        let trade_value = dec!(10000);
        let base_maker = dec!(0.0001);
        let base_taker = dec!(0.0002);

        let regular = Account::new("regular");
        let (maker_rate, taker_rate) = regular.effective_fees(base_maker, base_taker);
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        assert_eq!(maker_fee, dec!(1.00));
        assert_eq!(taker_fee, dec!(2.00));

        let vip5 = Account::new("vip5").with_fee_schedule(FeeSchedule::tier_5());
        let (maker_rate, taker_rate) = vip5.effective_fees(base_maker, base_taker);
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        assert_eq!(maker_fee, dec!(0.50));
        assert_eq!(taker_fee, dec!(1.50));

        let mm = Account::new("mm").with_fee_schedule(FeeSchedule::market_maker());
        let (maker_rate, taker_rate) = mm.effective_fees(base_maker, base_taker);
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        assert_eq!(maker_fee, dec!(-0.50));
        assert_eq!(taker_fee, dec!(1.00));
    }
}
