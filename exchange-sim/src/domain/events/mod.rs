use crate::domain::entities::{Order, OrderStatus, PriceLevel, Trade};
use crate::domain::value_objects::{OrderId, Price, Quantity, Side, Symbol, Timestamp};
use serde::{Deserialize, Serialize};

/// Domain events emitted by the exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "camelCase")]
pub enum ExchangeEvent {
    /// Order was accepted and added to the book
    OrderAccepted(OrderAcceptedEvent),
    /// Order was rejected
    OrderRejected(OrderRejectedEvent),
    /// Order was partially filled
    OrderPartiallyFilled(OrderFilledEvent),
    /// Order was completely filled
    OrderFilled(OrderFilledEvent),
    /// Order was canceled
    OrderCanceled(OrderCanceledEvent),
    /// Order expired due to time in force
    OrderExpired(OrderExpiredEvent),
    /// Trade occurred
    TradeExecuted(TradeExecutedEvent),
    /// Order book depth update (delta)
    DepthUpdate(DepthUpdateEvent),
    /// Full order book snapshot
    DepthSnapshot(DepthSnapshotEvent),
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecutedEvent {
    pub trade_id: uuid::Uuid,
    pub symbol: Symbol,
    pub price: Price,
    pub quantity: Quantity,
    pub buyer_order_id: OrderId,
    pub seller_order_id: OrderId,
    pub buyer_is_maker: bool,
    pub timestamp: Timestamp,
}

/// Binance-style depth update (delta)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthUpdateEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<[String; 2]>, // [price, quantity]
    #[serde(rename = "a")]
    pub asks: Vec<[String; 2]>,
}

/// Binance-style depth snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthSnapshotEvent {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
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

impl From<&Trade> for TradeExecutedEvent {
    fn from(trade: &Trade) -> Self {
        TradeExecutedEvent {
            trade_id: trade.id,
            symbol: trade.symbol.clone(),
            price: trade.price,
            quantity: trade.quantity,
            buyer_order_id: trade.buyer_order_id,
            seller_order_id: trade.seller_order_id,
            buyer_is_maker: trade.buyer_is_maker,
            timestamp: trade.timestamp,
        }
    }
}

impl DepthUpdateEvent {
    pub fn new(
        symbol: &Symbol,
        first_update_id: u64,
        final_update_id: u64,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        event_time: i64,
    ) -> Self {
        DepthUpdateEvent {
            event_type: "depthUpdate".to_string(),
            event_time,
            symbol: symbol.to_string(),
            first_update_id,
            final_update_id,
            bids: bids
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
            asks: asks
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
        }
    }
}

impl DepthSnapshotEvent {
    pub fn new(last_update_id: u64, bids: Vec<PriceLevel>, asks: Vec<PriceLevel>) -> Self {
        DepthSnapshotEvent {
            last_update_id,
            bids: bids
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
            asks: asks
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
        }
    }
}
