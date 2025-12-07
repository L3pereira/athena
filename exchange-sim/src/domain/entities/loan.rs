//! Loan entity for borrowed assets (margin/short selling).

use crate::domain::value_objects::Timestamp;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

    /// Check if loan is fully repaid
    pub fn is_repaid(&self) -> bool {
        self.principal.is_zero() && self.accrued_interest.is_zero()
    }
}
