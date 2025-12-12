//! Loan entity for borrowed assets (margin/short selling).

use crate::domain::value_objects::{Rate, Timestamp, Value};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Milliseconds per year (365 * 24 * 60 * 60 * 1000)
const MS_PER_YEAR: i128 = 31_536_000_000;

/// A loan for borrowed assets (for short selling)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Loan {
    pub id: Uuid,
    /// Asset being borrowed (e.g., "BTC")
    pub asset: String,
    /// Amount borrowed
    pub principal: Value,
    /// Interest rate (annual, in basis points - e.g., 500 = 5%)
    pub interest_rate: Rate,
    /// Accrued interest
    pub accrued_interest: Value,
    /// Collateral locked (in quote currency, e.g., USDT)
    pub collateral: Value,
    /// When the loan was created
    pub created_at: Timestamp,
    /// Last interest accrual
    pub last_accrual: Timestamp,
}

impl Loan {
    pub fn new(
        asset: impl Into<String>,
        principal: Value,
        interest_rate: Rate,
        collateral: Value,
        now: Timestamp,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            asset: asset.into(),
            principal,
            interest_rate,
            accrued_interest: Value::ZERO,
            collateral,
            created_at: now,
            last_accrual: now,
        }
    }

    /// Total amount owed (principal + interest)
    pub fn total_owed(&self) -> Value {
        self.principal + self.accrued_interest
    }

    /// Accrue interest based on time elapsed
    pub fn accrue_interest(&mut self, now: Timestamp) {
        let elapsed_ms = (now - self.last_accrual).num_milliseconds();
        if elapsed_ms <= 0 {
            return;
        }

        // Interest = principal * rate * elapsed_time / year
        // rate is in bps (10000 = 100%), so: principal * rate / 10000 * elapsed_ms / ms_per_year
        // = principal * rate * elapsed_ms / (10000 * ms_per_year)
        let interest_raw =
            (self.principal.raw() * self.interest_rate.bps() as i128 * elapsed_ms as i128)
                / (10_000 * MS_PER_YEAR);

        self.accrued_interest = Value::from_raw(self.accrued_interest.raw() + interest_raw);
        self.last_accrual = now;
    }

    /// Repay part of the loan, returns remaining principal
    pub fn repay(&mut self, amount: Value) -> Value {
        // First pay off interest
        if amount.raw() <= self.accrued_interest.raw() {
            self.accrued_interest = Value::from_raw(self.accrued_interest.raw() - amount.raw());
            return self.principal;
        }

        let after_interest = amount.raw() - self.accrued_interest.raw();
        self.accrued_interest = Value::ZERO;

        // Then pay off principal
        self.principal = Value::from_raw((self.principal.raw() - after_interest).max(0));
        self.principal
    }

    /// Check if loan is fully repaid
    pub fn is_repaid(&self) -> bool {
        self.principal.raw() == 0 && self.accrued_interest.raw() == 0
    }
}
