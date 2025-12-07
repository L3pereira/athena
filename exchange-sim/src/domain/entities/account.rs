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
            maker_discount: Decimal::ONE, // No discount
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
    } // 10% maker, 5% taker discount
    pub fn tier_2() -> Self {
        Self::vip(2, dec!(0.80), dec!(0.90))
    } // 20% maker, 10% taker
    pub fn tier_3() -> Self {
        Self::vip(3, dec!(0.70), dec!(0.85))
    } // 30% maker, 15% taker
    pub fn tier_4() -> Self {
        Self::vip(4, dec!(0.60), dec!(0.80))
    } // 40% maker, 20% taker
    pub fn tier_5() -> Self {
        Self::vip(5, dec!(0.50), dec!(0.75))
    } // 50% maker, 25% taker
    pub fn market_maker() -> Self {
        Self::vip(9, dec!(-0.50), dec!(0.50))
    } // Negative = rebate

    /// Apply discount to base fee rates
    pub fn apply(&self, maker_rate: Decimal, taker_rate: Decimal) -> (Decimal, Decimal) {
        (
            maker_rate * self.maker_discount,
            taker_rate * self.taker_discount,
        )
    }
}

/// A trading account with balances, positions, and margin tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub owner_id: String,
    pub status: AccountStatus,
    pub margin_mode: MarginMode,

    /// Asset balances: asset -> (available, locked)
    /// Available = can be used for new orders
    /// Locked = reserved for open orders/positions
    balances: HashMap<String, AssetBalance>,

    /// Open positions by symbol
    positions: HashMap<Symbol, Position>,

    /// Active loans (borrowed assets for short selling)
    loans: HashMap<String, Loan>,

    /// Margin configuration
    pub initial_margin_rate: Decimal, // e.g., 0.10 = 10x leverage
    pub maintenance_margin_rate: Decimal, // e.g., 0.05 = liquidation threshold

    /// Fee schedule (VIP tier discounts)
    pub fee_schedule: FeeSchedule,

    /// Timestamps
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
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

/// A position in a trading pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: Symbol,
    pub side: PositionSide,
    /// Position size (always positive)
    pub quantity: Quantity,
    /// Average entry price
    pub entry_price: Price,
    /// Current mark price for P&L calculation
    pub mark_price: Price,
    /// Realized P&L from closed portions
    pub realized_pnl: Decimal,
    /// Margin allocated to this position
    pub margin: Decimal,
    /// Timestamp when position was opened
    pub opened_at: Timestamp,
    /// Last update timestamp
    pub updated_at: Timestamp,
}

/// Position side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
}

impl Position {
    /// Create a new position
    pub fn new(
        symbol: Symbol,
        side: PositionSide,
        quantity: Quantity,
        entry_price: Price,
        margin: Decimal,
        now: Timestamp,
    ) -> Self {
        Self {
            symbol,
            side,
            quantity,
            entry_price,
            mark_price: entry_price,
            realized_pnl: Decimal::ZERO,
            margin,
            opened_at: now,
            updated_at: now,
        }
    }

    /// Calculate unrealized P&L
    pub fn unrealized_pnl(&self) -> Decimal {
        let qty = self.quantity.inner();
        let entry = self.entry_price.inner();
        let mark = self.mark_price.inner();

        match self.side {
            PositionSide::Long => qty * (mark - entry),
            PositionSide::Short => qty * (entry - mark),
        }
    }

    /// Total P&L (realized + unrealized)
    pub fn total_pnl(&self) -> Decimal {
        self.realized_pnl + self.unrealized_pnl()
    }

    /// Notional value at current mark price
    pub fn notional_value(&self) -> Decimal {
        self.quantity.inner() * self.mark_price.inner()
    }

    /// Calculate liquidation price
    pub fn liquidation_price(&self, maintenance_margin_rate: Decimal) -> Price {
        let entry = self.entry_price.inner();
        let margin_ratio = self.margin / self.notional_value();

        let liq_price = match self.side {
            // Long: liquidate when price drops enough that margin is depleted
            PositionSide::Long => entry * (Decimal::ONE - margin_ratio + maintenance_margin_rate),
            // Short: liquidate when price rises enough
            PositionSide::Short => entry * (Decimal::ONE + margin_ratio - maintenance_margin_rate),
        };

        Price::from(liq_price.max(Decimal::ZERO))
    }

    /// Check if position should be liquidated
    pub fn should_liquidate(&self, maintenance_margin_rate: Decimal) -> bool {
        let liq_price = self.liquidation_price(maintenance_margin_rate);
        match self.side {
            PositionSide::Long => self.mark_price <= liq_price,
            PositionSide::Short => self.mark_price >= liq_price,
        }
    }

    /// Update mark price
    pub fn update_mark_price(&mut self, price: Price, now: Timestamp) {
        self.mark_price = price;
        self.updated_at = now;
    }

    /// Increase position size
    pub fn increase(
        &mut self,
        quantity: Quantity,
        price: Price,
        additional_margin: Decimal,
        now: Timestamp,
    ) {
        let old_notional = self.quantity.inner() * self.entry_price.inner();
        let new_notional = quantity.inner() * price.inner();
        let total_qty = self.quantity.inner() + quantity.inner();

        // Weighted average entry price
        self.entry_price = Price::from((old_notional + new_notional) / total_qty);
        self.quantity = Quantity::from(total_qty);
        self.margin += additional_margin;
        self.updated_at = now;
    }

    /// Decrease position size, returns realized P&L
    pub fn decrease(&mut self, quantity: Quantity, exit_price: Price, now: Timestamp) -> Decimal {
        let close_qty = quantity.inner().min(self.quantity.inner());
        let entry = self.entry_price.inner();
        let exit = exit_price.inner();

        // Calculate realized P&L for closed portion
        let pnl = match self.side {
            PositionSide::Long => close_qty * (exit - entry),
            PositionSide::Short => close_qty * (entry - exit),
        };

        // Release proportional margin
        let close_ratio = close_qty / self.quantity.inner();
        let released_margin = self.margin * close_ratio;
        self.margin -= released_margin;

        // Update quantity
        self.quantity = Quantity::from(self.quantity.inner() - close_qty);
        self.realized_pnl += pnl;
        self.updated_at = now;

        pnl
    }

    /// Check if position is closed (zero quantity)
    pub fn is_closed(&self) -> bool {
        self.quantity.is_zero()
    }
}

/// A loan for borrowed assets (for short selling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loan {
    pub id: Uuid,
    /// Asset being borrowed (e.g., "BTC")
    pub asset: String,
    /// Amount borrowed
    pub principal: Decimal,
    /// Interest rate (annual, e.g., 0.05 = 5%)
    pub interest_rate: Decimal,
    /// Accrued interest
    pub accrued_interest: Decimal,
    /// Collateral locked (in quote currency, e.g., USDT)
    pub collateral: Decimal,
    /// When the loan was created
    pub created_at: Timestamp,
    /// Last interest accrual
    pub last_accrual: Timestamp,
}

impl Loan {
    pub fn new(
        asset: impl Into<String>,
        principal: Decimal,
        interest_rate: Decimal,
        collateral: Decimal,
        now: Timestamp,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            asset: asset.into(),
            principal,
            interest_rate,
            accrued_interest: Decimal::ZERO,
            collateral,
            created_at: now,
            last_accrual: now,
        }
    }

    /// Total amount owed (principal + interest)
    pub fn total_owed(&self) -> Decimal {
        self.principal + self.accrued_interest
    }

    /// Accrue interest based on time elapsed
    pub fn accrue_interest(&mut self, now: Timestamp) {
        let elapsed_ms = (now - self.last_accrual).num_milliseconds();
        if elapsed_ms <= 0 {
            return;
        }

        // Convert annual rate to per-millisecond rate
        let ms_per_year = dec!(31_536_000_000); // 365 * 24 * 60 * 60 * 1000
        let rate_per_ms = self.interest_rate / ms_per_year;

        let interest = self.principal * rate_per_ms * Decimal::from(elapsed_ms);
        self.accrued_interest += interest;
        self.last_accrual = now;
    }

    /// Repay part of the loan, returns remaining principal
    pub fn repay(&mut self, amount: Decimal) -> Decimal {
        // First pay off interest
        if amount <= self.accrued_interest {
            self.accrued_interest -= amount;
            return self.principal;
        }

        let after_interest = amount - self.accrued_interest;
        self.accrued_interest = Decimal::ZERO;

        // Then pay off principal
        self.principal = (self.principal - after_interest).max(Decimal::ZERO);
        self.principal
    }
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
            initial_margin_rate: dec!(0.10),     // 10x leverage
            maintenance_margin_rate: dec!(0.05), // 5% maintenance
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

    /// Withdraw funds (fails if insufficient available balance)
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
        // Check collateral
        let collateral_balance = self
            .balances
            .get_mut(collateral_asset)
            .ok_or(AccountError::InsufficientCollateral)?;

        if collateral_balance.available < collateral_amount {
            return Err(AccountError::InsufficientCollateral);
        }

        // Lock collateral
        collateral_balance.available -= collateral_amount;
        collateral_balance.locked += collateral_amount;

        // Create loan
        let loan = Loan::new(asset, amount, interest_rate, collateral_amount, now);

        // Add borrowed amount to available balance
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

        // Accrue interest first
        loan.accrue_interest(now);

        let asset_balance = self
            .balances
            .get_mut(asset)
            .ok_or(AccountError::InsufficientBalance)?;

        // Check we have enough to repay
        let repay_amount = amount.min(loan.total_owed());
        if asset_balance.available < repay_amount {
            return Err(AccountError::InsufficientBalance);
        }

        // Deduct from balance
        asset_balance.available -= repay_amount;
        asset_balance.borrowed = (asset_balance.borrowed - repay_amount).max(Decimal::ZERO);

        // Repay loan
        let remaining = loan.repay(repay_amount);

        // If fully repaid, release collateral
        if remaining.is_zero() && loan.accrued_interest.is_zero() {
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
                // Increase existing position
                pos.increase(quantity, price, margin, now);
            } else {
                // Opposite side - close/reduce first
                let close_qty = quantity.inner().min(pos.quantity.inner());
                pos.decrease(Quantity::from(close_qty), price, now);

                if pos.is_closed() {
                    self.positions.remove(&symbol);
                }

                // If there's remaining quantity, open new position
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
            // New position
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

    // ========== Margin Calculations ==========

    /// Total equity (all assets at current prices)
    pub fn equity(&self) -> Decimal {
        let mut total = Decimal::ZERO;

        // Sum all balances (net of borrows)
        for balance in self.balances.values() {
            total += balance.net();
        }

        // Add unrealized P&L from positions
        for pos in self.positions.values() {
            total += pos.unrealized_pnl();
        }

        total
    }

    /// Total margin used
    pub fn used_margin(&self) -> Decimal {
        self.positions.values().map(|p| p.margin).sum()
    }

    /// Available margin for new positions
    pub fn available_margin(&self) -> Decimal {
        (self.equity() - self.used_margin()).max(Decimal::ZERO)
    }

    /// Margin ratio (equity / maintenance margin required)
    pub fn margin_ratio(&self) -> Decimal {
        let maintenance_required: Decimal = self
            .positions
            .values()
            .map(|p| p.notional_value() * self.maintenance_margin_rate)
            .sum();

        if maintenance_required.is_zero() {
            return Decimal::MAX;
        }

        self.equity() / maintenance_required
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
        if self.positions.is_empty() {
            self.status = AccountStatus::Active;
            return;
        }

        let margin_ratio = self.margin_ratio();

        self.status = if margin_ratio < Decimal::ONE {
            AccountStatus::Liquidating
        } else if margin_ratio < dec!(1.2) {
            AccountStatus::MarginCall
        } else {
            AccountStatus::Active
        };
    }

    /// Get positions that should be liquidated
    pub fn liquidatable_positions(&self) -> Vec<&Position> {
        self.positions
            .values()
            .filter(|p| p.should_liquidate(self.maintenance_margin_rate))
            .collect()
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
    use rust_decimal_macros::dec;

    #[test]
    fn test_deposit_withdraw() {
        let mut account = Account::new("user1");

        account.deposit("USDT", dec!(10000));
        assert_eq!(account.balance("USDT").available, dec!(10000));

        account.withdraw("USDT", dec!(3000)).unwrap();
        assert_eq!(account.balance("USDT").available, dec!(7000));

        // Can't withdraw more than available
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

        // Deposit collateral
        account.deposit("USDT", dec!(60000));

        // Borrow 1 BTC with USDT as collateral (150% collateral ratio)
        // At $50k BTC, need $75k collateral
        account
            .borrow(
                "BTC",
                dec!(1),    // borrow 1 BTC
                dec!(0.05), // 5% annual interest
                "USDT",
                dec!(55000), // collateral
                now,
            )
            .unwrap();

        // Now have 1 BTC available (borrowed)
        let btc_balance = account.balance("BTC");
        assert_eq!(btc_balance.available, dec!(1));
        assert_eq!(btc_balance.borrowed, dec!(1));

        // Collateral is locked
        let usdt_balance = account.balance("USDT");
        assert_eq!(usdt_balance.available, dec!(5000));
        assert_eq!(usdt_balance.locked, dec!(55000));
    }

    #[test]
    fn test_short_position_pnl() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = chrono::Utc::now();

        // Open short at $50k
        let mut pos = Position::new(
            symbol,
            PositionSide::Short,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            dec!(5000), // margin
            now,
        );

        // Price drops to $45k - profit!
        pos.update_mark_price(Price::from(dec!(45000)), now);
        assert_eq!(pos.unrealized_pnl(), dec!(5000));

        // Price rises to $55k - loss
        pos.update_mark_price(Price::from(dec!(55000)), now);
        assert_eq!(pos.unrealized_pnl(), dec!(-5000));
    }

    #[test]
    fn test_short_liquidation_price() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = chrono::Utc::now();

        // Short 1 BTC at $50k with 10% margin ($5k)
        let pos = Position::new(
            symbol,
            PositionSide::Short,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            dec!(5000), // 10% margin
            now,
        );

        // Liquidation price for short = entry * (1 + margin_ratio - maintenance)
        // = 50000 * (1 + 0.10 - 0.05) = 50000 * 1.05 = 52500
        let liq_price = pos.liquidation_price(dec!(0.05));
        assert_eq!(liq_price, Price::from(dec!(52500)));
    }

    #[test]
    fn test_full_short_flow() {
        let mut account = Account::new("trader1").with_margin_rates(dec!(0.10), dec!(0.05));
        let now = chrono::Utc::now();
        let symbol = Symbol::new("BTCUSDT").unwrap();

        // 1. Deposit USDT collateral
        account.deposit("USDT", dec!(100000));

        // 2. Borrow 1 BTC to sell short
        account
            .borrow(
                "BTC",
                dec!(1),
                dec!(0.05),
                "USDT",
                dec!(60000), // collateral
                now,
            )
            .unwrap();

        // 3. Sell BTC (open short position)
        // Margin required = 1 * 50000 * 0.10 = 5000
        account.lock("USDT", dec!(5000)).unwrap();
        account.open_position(
            symbol.clone(),
            PositionSide::Short,
            Quantity::from(dec!(1)),
            Price::from(dec!(50000)),
            dec!(5000),
            now,
        );

        // The borrowed BTC is now "sold"
        account.withdraw("BTC", dec!(1)).unwrap();

        // 4. Price drops to $40k - close for profit
        let mut prices = HashMap::new();
        prices.insert(symbol.clone(), Price::from(dec!(40000)));
        account.update_mark_prices(&prices, now);

        // Check unrealized P&L = +$10k
        let pos = account.position(&symbol).unwrap();
        assert_eq!(pos.unrealized_pnl(), dec!(10000));

        // 5. Close position
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
        // Default uses multiplier of 1.0 (no discount)
        assert_eq!(schedule.maker_discount, Decimal::ONE);
        assert_eq!(schedule.taker_discount, Decimal::ONE);
    }

    #[test]
    fn test_fee_schedule_vip_tiers() {
        // Tier 1: 10% maker discount (0.90 multiplier), 5% taker discount (0.95 multiplier)
        let tier1 = FeeSchedule::tier_1();
        assert_eq!(tier1.tier, 1);
        assert_eq!(tier1.maker_discount, dec!(0.90));
        assert_eq!(tier1.taker_discount, dec!(0.95));

        // Tier 5: 50% maker discount (0.50 multiplier), 25% taker discount (0.75 multiplier)
        let tier5 = FeeSchedule::tier_5();
        assert_eq!(tier5.tier, 5);
        assert_eq!(tier5.maker_discount, dec!(0.50));
        assert_eq!(tier5.taker_discount, dec!(0.75));
    }

    #[test]
    fn test_fee_schedule_market_maker_rebate() {
        // Market makers get negative maker fee (rebate)
        let mm = FeeSchedule::market_maker();
        assert_eq!(mm.tier, 9);
        // Negative multiplier means negative fee = rebate credited to account
        assert_eq!(mm.maker_discount, dec!(-0.50));
        assert_eq!(mm.taker_discount, dec!(0.50));
    }

    #[test]
    fn test_account_effective_fees() {
        // Base rates: 1 bps maker (0.01%), 2 bps taker (0.02%)
        let base_maker = dec!(0.0001);
        let base_taker = dec!(0.0002);

        // Default account - no discount (multiplier = 1.0)
        let account = Account::new("user1");
        let (effective_maker, effective_taker) = account.effective_fees(base_maker, base_taker);
        assert_eq!(effective_maker, dec!(0.0001)); // No change
        assert_eq!(effective_taker, dec!(0.0002)); // No change

        // VIP tier 1 - 10% maker discount (0.90 multiplier), 5% taker discount (0.95)
        let vip1 = Account::new("vip1").with_fee_schedule(FeeSchedule::tier_1());
        let (eff_maker, eff_taker) = vip1.effective_fees(base_maker, base_taker);
        assert_eq!(eff_maker, dec!(0.00009)); // 0.0001 * 0.90
        assert_eq!(eff_taker, dec!(0.00019)); // 0.0002 * 0.95

        // VIP tier 5 - 50% maker discount (0.50), 25% taker discount (0.75)
        let vip5 = Account::new("vip5").with_fee_schedule(FeeSchedule::tier_5());
        let (eff_maker, eff_taker) = vip5.effective_fees(base_maker, base_taker);
        assert_eq!(eff_maker, dec!(0.00005)); // 0.0001 * 0.50
        assert_eq!(eff_taker, dec!(0.00015)); // 0.0002 * 0.75
    }

    #[test]
    fn test_market_maker_gets_rebate() {
        // Base rates: 1 bps maker, 2 bps taker
        let base_maker = dec!(0.0001);
        let base_taker = dec!(0.0002);

        // Market maker with -0.50 maker multiplier = negative fee = rebate
        let mm = Account::new("mm1").with_fee_schedule(FeeSchedule::market_maker());
        let (eff_maker, eff_taker) = mm.effective_fees(base_maker, base_taker);

        // Maker fee: 0.0001 * (-0.50) = -0.00005 (REBATE!)
        // Negative rate means exchange pays the market maker for providing liquidity
        assert_eq!(eff_maker, dec!(-0.00005));
        assert!(eff_maker < Decimal::ZERO); // Confirm it's a rebate

        // Taker fee: 0.0002 * 0.50 = 0.0001 (50% discount)
        assert_eq!(eff_taker, dec!(0.0001));
    }

    #[test]
    fn test_fee_calculation_on_trade() {
        // Simulate fee calculation for a $10,000 trade
        let trade_value = dec!(10000);

        // Base rates: 1 bps maker (0.01%), 2 bps taker (0.02%)
        let base_maker = dec!(0.0001);
        let base_taker = dec!(0.0002);

        // Regular user: full fees
        let regular = Account::new("regular");
        let (maker_rate, taker_rate) = regular.effective_fees(base_maker, base_taker);
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        assert_eq!(maker_fee, dec!(1.00)); // $1.00 maker fee
        assert_eq!(taker_fee, dec!(2.00)); // $2.00 taker fee

        // VIP tier 5: discounted fees
        let vip5 = Account::new("vip5").with_fee_schedule(FeeSchedule::tier_5());
        let (maker_rate, taker_rate) = vip5.effective_fees(base_maker, base_taker);
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        assert_eq!(maker_fee, dec!(0.50)); // $0.50 maker fee (50% discount)
        assert_eq!(taker_fee, dec!(1.50)); // $1.50 taker fee (25% discount)

        // Market maker: receives rebate on maker trades
        let mm = Account::new("mm").with_fee_schedule(FeeSchedule::market_maker());
        let (maker_rate, taker_rate) = mm.effective_fees(base_maker, base_taker);
        let maker_fee = trade_value * maker_rate;
        let taker_fee = trade_value * taker_rate;
        assert_eq!(maker_fee, dec!(-0.50)); // -$0.50 = REBATE paid to market maker
        assert_eq!(taker_fee, dec!(1.00)); // $1.00 taker fee (50% discount)
    }
}
