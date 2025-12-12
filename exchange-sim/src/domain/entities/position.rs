//! Trading position entity for derivatives and margin trading.

use crate::domain::value_objects::{PRICE_SCALE, Price, Quantity, Symbol, Timestamp, Value};
use serde::{Deserialize, Serialize};

/// Position side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    Long,
    Short,
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
    pub realized_pnl: Value,
    /// Margin allocated to this position
    pub margin: Value,
    /// Timestamp when position was opened
    pub opened_at: Timestamp,
    /// Last update timestamp
    pub updated_at: Timestamp,
}

impl Position {
    /// Create a new position
    pub fn new(
        symbol: Symbol,
        side: PositionSide,
        quantity: Quantity,
        entry_price: Price,
        margin: Value,
        now: Timestamp,
    ) -> Self {
        Self {
            symbol,
            side,
            quantity,
            entry_price,
            mark_price: entry_price,
            realized_pnl: Value::ZERO,
            margin,
            opened_at: now,
            updated_at: now,
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
        additional_margin: Value,
        now: Timestamp,
    ) {
        // Weighted average: (p1*q1 + p2*q2) / (q1+q2)
        // Using i128 for precision
        let p1q1 = self.entry_price.raw() as i128 * self.quantity.raw() as i128;
        let p2q2 = price.raw() as i128 * quantity.raw() as i128;
        let q_total = self.quantity.raw() as i128 + quantity.raw() as i128;

        // Result has PRICE_SCALE (p*q / q = p)
        let avg_price_raw = (p1q1 + p2q2) / q_total;
        self.entry_price = Price::from_raw(avg_price_raw as i64);
        self.quantity = Quantity::from_raw(self.quantity.raw() + quantity.raw());
        self.margin = Value::from_raw(self.margin.raw() + additional_margin.raw());
        self.updated_at = now;
    }

    /// Decrease position size, returns realized P&L
    pub fn decrease(&mut self, quantity: Quantity, exit_price: Price, now: Timestamp) -> Value {
        let close_qty_raw = quantity.raw().min(self.quantity.raw());

        // Calculate realized P&L: qty * (exit - entry) or qty * (entry - exit) for short
        let price_diff = match self.side {
            PositionSide::Long => exit_price.raw() - self.entry_price.raw(),
            PositionSide::Short => self.entry_price.raw() - exit_price.raw(),
        };

        // pnl = qty * price_diff, adjust for scale
        let pnl_raw = close_qty_raw as i128 * price_diff as i128 / PRICE_SCALE as i128;
        let pnl = Value::from_raw(pnl_raw);

        // Release proportional margin
        if self.quantity.raw() > 0 {
            let close_ratio_num = close_qty_raw as i128;
            let close_ratio_denom = self.quantity.raw() as i128;
            let released_margin_raw = self.margin.raw() * close_ratio_num / close_ratio_denom;
            self.margin = Value::from_raw(self.margin.raw() - released_margin_raw);
        }

        // Update quantity
        self.quantity = Quantity::from_raw(self.quantity.raw() - close_qty_raw);
        self.realized_pnl = Value::from_raw(self.realized_pnl.raw() + pnl.raw());
        self.updated_at = now;

        pnl
    }

    /// Check if position is closed (zero quantity)
    pub fn is_closed(&self) -> bool {
        self.quantity.is_zero()
    }

    /// Notional value at current mark price
    pub fn notional_value(&self) -> Value {
        self.mark_price.mul_qty(self.quantity)
    }
}
