use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{OrderStatus, OrderType, Side, TimeInForce};
use crate::instruments::InstrumentId;

/// Unique identifier for an order
pub type OrderId = Uuid;

/// Full order details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    /// The instrument being traded
    pub instrument_id: InstrumentId,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    /// Required for Limit and StopLimit orders
    pub price: Option<Decimal>,
    /// Required for StopLoss and StopLimit orders
    pub stop_price: Option<Decimal>,
    pub time_in_force: TimeInForce,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Order {
    /// Create a new order with explicit timestamp
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_time(
        instrument_id: impl Into<InstrumentId>,
        side: Side,
        order_type: OrderType,
        quantity: Decimal,
        price: Option<Decimal>,
        stop_price: Option<Decimal>,
        time_in_force: TimeInForce,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            instrument_id: instrument_id.into(),
            side,
            order_type,
            quantity,
            filled_quantity: Decimal::ZERO,
            price,
            stop_price,
            time_in_force,
            status: OrderStatus::New,
            created_at: timestamp,
            updated_at: timestamp,
        }
    }

    /// Create a new order using current system time
    /// Note: For simulation, prefer `new_with_time` with clock-provided time
    pub fn new(
        instrument_id: impl Into<InstrumentId>,
        side: Side,
        order_type: OrderType,
        quantity: Decimal,
        price: Option<Decimal>,
        stop_price: Option<Decimal>,
        time_in_force: TimeInForce,
    ) -> Self {
        Self::new_with_time(
            instrument_id,
            side,
            order_type,
            quantity,
            price,
            stop_price,
            time_in_force,
            Utc::now(),
        )
    }

    /// Get the symbol/instrument identifier as a string slice
    /// Convenience method for backward compatibility
    pub fn symbol(&self) -> &str {
        self.instrument_id.as_str()
    }

    /// Validate the order based on order type requirements
    pub fn validate(&self) -> bool {
        match self.order_type {
            OrderType::Market => true,
            OrderType::Limit => self.price.is_some(),
            OrderType::StopLoss => self.stop_price.is_some(),
            OrderType::StopLimit => self.price.is_some() && self.stop_price.is_some(),
        }
    }

    /// Determine if an order is marketable against the best price
    pub fn is_marketable(&self, best_price: Option<Decimal>) -> bool {
        match (self.side, self.order_type, self.price, best_price) {
            // Market orders are always marketable if there's liquidity
            (_, OrderType::Market, _, Some(_)) => true,

            // Buy limit order is marketable if limit price >= best ask
            (Side::Buy, OrderType::Limit, Some(limit), Some(ask)) if limit >= ask => true,

            // Sell limit order is marketable if limit price <= best bid
            (Side::Sell, OrderType::Limit, Some(limit), Some(bid)) if limit <= bid => true,

            _ => false,
        }
    }

    /// Returns remaining quantity to be filled
    pub fn remaining_quantity(&self) -> Decimal {
        self.quantity - self.filled_quantity
    }

    /// Returns true if the order is completely filled
    pub fn is_filled(&self) -> bool {
        self.filled_quantity >= self.quantity
    }
}
