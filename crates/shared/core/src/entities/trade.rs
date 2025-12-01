use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::OrderId;
use super::fee::TradeFees;
use crate::instruments::InstrumentId;

/// Unique identifier for a trade
pub type TradeId = Uuid;

/// Trade resulting from matching orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: TradeId,
    /// The instrument that was traded
    pub instrument_id: InstrumentId,
    pub buy_order_id: OrderId,
    pub sell_order_id: OrderId,
    pub price: Decimal,
    pub quantity: Decimal,
    pub timestamp: DateTime<Utc>,
    /// Fees charged for this trade (optional for backward compatibility)
    pub fees: Option<TradeFees>,
}

impl Trade {
    /// Create a new trade with explicit timestamp
    pub fn new_with_time(
        instrument_id: impl Into<InstrumentId>,
        buy_order_id: OrderId,
        sell_order_id: OrderId,
        price: Decimal,
        quantity: Decimal,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            instrument_id: instrument_id.into(),
            buy_order_id,
            sell_order_id,
            price,
            quantity,
            timestamp,
            fees: None,
        }
    }

    /// Create a new trade using current system time
    /// Note: For simulation, prefer `new_with_time` with clock-provided time
    pub fn new(
        instrument_id: impl Into<InstrumentId>,
        buy_order_id: OrderId,
        sell_order_id: OrderId,
        price: Decimal,
        quantity: Decimal,
    ) -> Self {
        Self::new_with_time(
            instrument_id,
            buy_order_id,
            sell_order_id,
            price,
            quantity,
            Utc::now(),
        )
    }

    /// Create a new trade with fees
    pub fn new_with_fees(
        instrument_id: impl Into<InstrumentId>,
        buy_order_id: OrderId,
        sell_order_id: OrderId,
        price: Decimal,
        quantity: Decimal,
        timestamp: DateTime<Utc>,
        fees: TradeFees,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            instrument_id: instrument_id.into(),
            buy_order_id,
            sell_order_id,
            price,
            quantity,
            timestamp,
            fees: Some(fees),
        }
    }

    /// Set fees on an existing trade
    pub fn with_fees(mut self, fees: TradeFees) -> Self {
        self.fees = Some(fees);
        self
    }

    /// Get the symbol/instrument identifier as a string slice
    /// Convenience method for backward compatibility
    pub fn symbol(&self) -> &str {
        self.instrument_id.as_str()
    }

    /// Returns the notional value of the trade (price * quantity)
    pub fn notional(&self) -> Decimal {
        self.price * self.quantity
    }

    /// Get buyer's fee (if fees are set)
    pub fn buyer_fee(&self) -> Decimal {
        self.fees
            .as_ref()
            .map(|f| f.buyer_fee)
            .unwrap_or(Decimal::ZERO)
    }

    /// Get seller's fee (if fees are set)
    pub fn seller_fee(&self) -> Decimal {
        self.fees
            .as_ref()
            .map(|f| f.seller_fee)
            .unwrap_or(Decimal::ZERO)
    }

    /// Get total fees collected
    pub fn total_fees(&self) -> Decimal {
        self.fees
            .as_ref()
            .map(|f| f.total())
            .unwrap_or(Decimal::ZERO)
    }
}
