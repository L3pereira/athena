//! Trading position entity for derivatives and margin trading.

use crate::domain::value_objects::{Price, Quantity, Symbol, Timestamp};
use rust_decimal::Decimal;
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
    pub realized_pnl: Decimal,
    /// Margin allocated to this position
    pub margin: Decimal,
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

    /// Notional value at current mark price
    pub fn notional_value(&self) -> Decimal {
        self.quantity.inner() * self.mark_price.inner()
    }
}
