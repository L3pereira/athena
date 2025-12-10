use crate::entities::{Order, OrderStatus};
use crate::value_objects::{OrderId, Price, Quantity, Side, Symbol, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAcceptedEvent {
    pub order_id: OrderId,
    pub client_order_id: Option<String>,
    pub symbol: Symbol,
    pub side: Side,
    pub price: Option<Price>,
    pub quantity: Quantity,
    pub timestamp: Timestamp,
}

impl From<&Order> for OrderAcceptedEvent {
    fn from(order: &Order) -> Self {
        OrderAcceptedEvent {
            order_id: order.id,
            client_order_id: order.client_order_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side,
            price: order.price,
            quantity: order.quantity,
            timestamp: order.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRejectedEvent {
    pub order_id: OrderId,
    pub client_order_id: Option<String>,
    pub symbol: Symbol,
    pub reason: String,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderFilledEvent {
    pub order_id: OrderId,
    pub client_order_id: Option<String>,
    pub symbol: Symbol,
    pub side: Side,
    pub status: OrderStatus,
    pub price: Price,
    pub quantity: Quantity,
    pub cumulative_quantity: Quantity,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCanceledEvent {
    pub order_id: OrderId,
    pub client_order_id: Option<String>,
    pub symbol: Symbol,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExpiredEvent {
    pub order_id: OrderId,
    pub client_order_id: Option<String>,
    pub symbol: Symbol,
    pub timestamp: Timestamp,
}
