use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::position::{Position, PositionSide};
use crate::instruments::InstrumentId;

/// Type of margin account
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarginMode {
    /// Cross margin - all positions share the same margin pool
    Cross,
    /// Isolated margin - each position has its own margin
    Isolated,
}

/// Margin account status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    /// Normal operating status
    Active,
    /// Margin call - close to liquidation
    MarginCall,
    /// Being liquidated
    Liquidating,
    /// Account frozen
    Frozen,
}

/// Margin account for tracking balances and positions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarginAccount {
    /// Unique account identifier
    pub id: Uuid,

    /// Account owner identifier
    pub owner_id: String,

    /// Account status
    pub status: AccountStatus,

    /// Margin mode (cross or isolated)
    pub margin_mode: MarginMode,

    /// Available balance (can be used for new positions)
    pub available_balance: Decimal,

    /// Balance locked as margin for open positions
    pub margin_balance: Decimal,

    /// Total account equity (available + margin + unrealized P&L)
    pub equity: Decimal,

    /// Open positions indexed by instrument
    pub positions: HashMap<InstrumentId, Position>,

    /// Initial margin rate (e.g., 0.10 for 10x leverage)
    pub initial_margin_rate: Decimal,

    /// Maintenance margin rate
    pub maintenance_margin_rate: Decimal,

    /// Margin call threshold (e.g., 0.80 = 80% of maintenance)
    pub margin_call_threshold: Decimal,

    /// When the account was created
    pub created_at: DateTime<Utc>,

    /// Last update time
    pub updated_at: DateTime<Utc>,
}

impl MarginAccount {
    /// Create a new margin account
    pub fn new(
        owner_id: String,
        initial_balance: Decimal,
        margin_mode: MarginMode,
        initial_margin_rate: Decimal,
        maintenance_margin_rate: Decimal,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            owner_id,
            status: AccountStatus::Active,
            margin_mode,
            available_balance: initial_balance,
            margin_balance: Decimal::ZERO,
            equity: initial_balance,
            positions: HashMap::new(),
            initial_margin_rate,
            maintenance_margin_rate,
            margin_call_threshold: Decimal::new(80, 2), // 80%
            created_at: now,
            updated_at: now,
        }
    }

    /// Deposit funds into the account
    pub fn deposit(&mut self, amount: Decimal) {
        self.available_balance += amount;
        self.recalculate_equity();
        self.updated_at = Utc::now();
    }

    /// Withdraw funds from the account
    pub fn withdraw(&mut self, amount: Decimal) -> Result<(), &'static str> {
        if amount > self.available_balance {
            return Err("Insufficient available balance");
        }
        self.available_balance -= amount;
        self.recalculate_equity();
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Calculate required margin for a new position
    pub fn calculate_required_margin(&self, quantity: Decimal, price: Decimal) -> Decimal {
        quantity * price * self.initial_margin_rate
    }

    /// Check if account has sufficient margin for a new position
    pub fn has_sufficient_margin(&self, quantity: Decimal, price: Decimal) -> bool {
        let required = self.calculate_required_margin(quantity, price);
        self.available_balance >= required
    }

    /// Open or increase a position
    pub fn open_position(
        &mut self,
        instrument_id: InstrumentId,
        side: PositionSide,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<(), &'static str> {
        let required_margin = self.calculate_required_margin(quantity, price);

        if self.available_balance < required_margin {
            return Err("Insufficient margin");
        }

        // Lock the margin
        self.available_balance -= required_margin;
        self.margin_balance += required_margin;

        // Check if we have an existing position in this instrument
        if let Some(existing) = self.positions.get_mut(&instrument_id) {
            if existing.side == side {
                // Same side - increase position
                existing.increase(
                    quantity,
                    price,
                    self.initial_margin_rate,
                    self.maintenance_margin_rate,
                );
            } else {
                // Opposite side - reduce or flip position
                let existing_qty = existing.quantity;
                if quantity >= existing_qty {
                    // Close existing and potentially open opposite
                    let realized_pnl = existing.decrease(
                        existing_qty,
                        price,
                        self.initial_margin_rate,
                        self.maintenance_margin_rate,
                    );
                    self.available_balance += realized_pnl;

                    // Release margin from closed position
                    let released_margin =
                        existing_qty * existing.entry_price * self.initial_margin_rate;
                    self.margin_balance -= released_margin;
                    self.available_balance += released_margin;

                    if existing.is_closed() {
                        self.positions.remove(&instrument_id);
                    }

                    // Open remaining as new position in opposite direction
                    let remaining = quantity - existing_qty;
                    if remaining > Decimal::ZERO {
                        let new_position = Position::new(
                            self.id,
                            instrument_id.clone(),
                            side,
                            remaining,
                            price,
                            self.initial_margin_rate,
                            self.maintenance_margin_rate,
                        );
                        self.positions.insert(instrument_id, new_position);
                    }
                } else {
                    // Partial close
                    let realized_pnl = existing.decrease(
                        quantity,
                        price,
                        self.initial_margin_rate,
                        self.maintenance_margin_rate,
                    );
                    self.available_balance += realized_pnl;

                    // Release proportional margin
                    let released_margin =
                        quantity * existing.entry_price * self.initial_margin_rate;
                    self.margin_balance -= released_margin;
                    self.available_balance += released_margin;
                }
            }
        } else {
            // New position
            let position = Position::new(
                self.id,
                instrument_id.clone(),
                side,
                quantity,
                price,
                self.initial_margin_rate,
                self.maintenance_margin_rate,
            );
            self.positions.insert(instrument_id, position);
        }

        self.recalculate_equity();
        self.update_status();
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Close a position partially or fully
    pub fn close_position(
        &mut self,
        instrument_id: &InstrumentId,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<Decimal, &'static str> {
        let position = self
            .positions
            .get_mut(instrument_id)
            .ok_or("Position not found")?;

        let close_qty = quantity.min(position.quantity);
        let realized_pnl = position.decrease(
            close_qty,
            price,
            self.initial_margin_rate,
            self.maintenance_margin_rate,
        );

        // Release margin
        let released_margin = close_qty * position.entry_price * self.initial_margin_rate;
        self.margin_balance -= released_margin;
        self.available_balance += released_margin + realized_pnl;

        // Remove position if fully closed
        if position.is_closed() {
            self.positions.remove(instrument_id);
        }

        self.recalculate_equity();
        self.update_status();
        self.updated_at = Utc::now();
        Ok(realized_pnl)
    }

    /// Update mark prices for all positions
    pub fn update_mark_prices(&mut self, prices: &HashMap<InstrumentId, Decimal>) {
        for (instrument_id, position) in self.positions.iter_mut() {
            if let Some(&price) = prices.get(instrument_id) {
                position.update_mark_price(price);
            }
        }
        self.recalculate_equity();
        self.update_status();
        self.updated_at = Utc::now();
    }

    /// Calculate total unrealized P&L across all positions
    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.positions.values().map(|p| p.unrealized_pnl()).sum()
    }

    /// Calculate total realized P&L across all positions
    pub fn total_realized_pnl(&self) -> Decimal {
        self.positions.values().map(|p| p.realized_pnl).sum()
    }

    /// Calculate total initial margin required
    pub fn total_initial_margin(&self) -> Decimal {
        self.positions.values().map(|p| p.initial_margin).sum()
    }

    /// Calculate total maintenance margin required
    pub fn total_maintenance_margin(&self) -> Decimal {
        self.positions.values().map(|p| p.maintenance_margin).sum()
    }

    /// Calculate margin ratio (equity / maintenance margin)
    pub fn margin_ratio(&self) -> Decimal {
        let maintenance = self.total_maintenance_margin();
        if maintenance == Decimal::ZERO {
            return Decimal::MAX;
        }
        self.equity / maintenance
    }

    /// Get positions that should be liquidated
    pub fn positions_to_liquidate(&self) -> Vec<&Position> {
        self.positions
            .values()
            .filter(|p| p.should_liquidate())
            .collect()
    }

    /// Recalculate total equity
    fn recalculate_equity(&mut self) {
        self.equity = self.available_balance + self.margin_balance + self.total_unrealized_pnl();
    }

    /// Update account status based on margin ratio
    fn update_status(&mut self) {
        let margin_ratio = self.margin_ratio();
        let maintenance = self.total_maintenance_margin();

        if maintenance == Decimal::ZERO {
            self.status = AccountStatus::Active;
            return;
        }

        // Below 100% maintenance margin - liquidation
        if margin_ratio < Decimal::ONE {
            self.status = AccountStatus::Liquidating;
        }
        // Below margin call threshold
        else if margin_ratio < Decimal::ONE + self.margin_call_threshold {
            self.status = AccountStatus::MarginCall;
        }
        // Healthy
        else {
            self.status = AccountStatus::Active;
        }
    }

    /// Get maximum leverage based on initial margin rate
    pub fn max_leverage(&self) -> Decimal {
        if self.initial_margin_rate == Decimal::ZERO {
            return Decimal::ZERO;
        }
        Decimal::ONE / self.initial_margin_rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn create_test_account() -> MarginAccount {
        MarginAccount::new(
            "test_user".to_string(),
            dec!(10000.0),
            MarginMode::Cross,
            dec!(0.10), // 10% initial = 10x leverage
            dec!(0.05), // 5% maintenance
        )
    }

    #[test]
    fn test_account_creation() {
        let account = create_test_account();

        assert_eq!(account.available_balance, dec!(10000.0));
        assert_eq!(account.margin_balance, Decimal::ZERO);
        assert_eq!(account.equity, dec!(10000.0));
        assert_eq!(account.max_leverage(), dec!(10));
    }

    #[test]
    fn test_deposit_withdraw() {
        let mut account = create_test_account();

        account.deposit(dec!(5000.0));
        assert_eq!(account.available_balance, dec!(15000.0));
        assert_eq!(account.equity, dec!(15000.0));

        account.withdraw(dec!(3000.0)).unwrap();
        assert_eq!(account.available_balance, dec!(12000.0));

        // Try to withdraw more than available
        let result = account.withdraw(dec!(20000.0));
        assert!(result.is_err());
    }

    #[test]
    fn test_open_long_position() {
        let mut account = create_test_account();

        // Open 1 BTC long at $50,000 (requires $5,000 margin at 10x)
        let result = account.open_position(
            InstrumentId::new("BTC/USD"),
            PositionSide::Long,
            dec!(1.0),
            dec!(50000.0),
        );

        assert!(result.is_ok());
        assert_eq!(account.available_balance, dec!(5000.0)); // 10000 - 5000
        assert_eq!(account.margin_balance, dec!(5000.0));
        assert!(
            account
                .positions
                .contains_key(&InstrumentId::new("BTC/USD"))
        );
    }

    #[test]
    fn test_insufficient_margin() {
        let mut account = create_test_account();

        // Try to open position requiring more margin than available
        // 3 BTC at $50,000 = $150,000 notional, needs $15,000 margin
        let result = account.open_position(
            InstrumentId::new("BTC/USD"),
            PositionSide::Long,
            dec!(3.0),
            dec!(50000.0),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_close_position_with_profit() {
        let mut account = create_test_account();

        // Open position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(1.0),
                dec!(50000.0),
            )
            .unwrap();

        // Close at profit ($55,000)
        let pnl = account
            .close_position(&InstrumentId::new("BTC/USD"), dec!(1.0), dec!(55000.0))
            .unwrap();

        assert_eq!(pnl, dec!(5000.0));
        // Original: 10000, margin released: 5000, pnl: 5000
        assert_eq!(account.available_balance, dec!(15000.0));
        assert_eq!(account.margin_balance, Decimal::ZERO);
        assert!(account.positions.is_empty());
    }

    #[test]
    fn test_close_position_with_loss() {
        let mut account = create_test_account();

        // Open position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(1.0),
                dec!(50000.0),
            )
            .unwrap();

        // Close at loss ($48,000)
        let pnl = account
            .close_position(&InstrumentId::new("BTC/USD"), dec!(1.0), dec!(48000.0))
            .unwrap();

        assert_eq!(pnl, dec!(-2000.0));
        // Original: 10000, margin released: 5000, pnl: -2000
        assert_eq!(account.available_balance, dec!(8000.0));
    }

    #[test]
    fn test_short_position() {
        let mut account = create_test_account();

        // Open short position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Short,
                dec!(1.0),
                dec!(50000.0),
            )
            .unwrap();

        // Close at profit (price went down to $45,000)
        let pnl = account
            .close_position(&InstrumentId::new("BTC/USD"), dec!(1.0), dec!(45000.0))
            .unwrap();

        assert_eq!(pnl, dec!(5000.0));
    }

    #[test]
    fn test_update_mark_prices() {
        let mut account = create_test_account();

        // Open position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(1.0),
                dec!(50000.0),
            )
            .unwrap();

        // Update mark price
        let mut prices = HashMap::new();
        prices.insert(InstrumentId::new("BTC/USD"), dec!(55000.0));
        account.update_mark_prices(&prices);

        // Check unrealized P&L
        assert_eq!(account.total_unrealized_pnl(), dec!(5000.0));
        // Equity should include unrealized P&L
        assert_eq!(account.equity, dec!(15000.0)); // 5000 available + 5000 margin + 5000 pnl
    }

    #[test]
    fn test_margin_call_status() {
        let mut account = create_test_account();

        // Open max leverage position
        account
            .open_position(
                InstrumentId::new("BTC/USD"),
                PositionSide::Long,
                dec!(2.0),
                dec!(50000.0),
            )
            .unwrap();

        // All margin used, 0 available
        assert_eq!(account.available_balance, Decimal::ZERO);

        // Update price to cause significant loss
        let mut prices = HashMap::new();
        prices.insert(InstrumentId::new("BTC/USD"), dec!(47000.0));
        account.update_mark_prices(&prices);

        // Should be in margin call or liquidating status
        assert!(matches!(
            account.status,
            AccountStatus::MarginCall | AccountStatus::Liquidating
        ));
    }
}
