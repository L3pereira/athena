use crate::entities::Trade;
use crate::value_objects::{OrderId, Price, Quantity, Symbol, Timestamp, TradeId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecutedEvent {
    pub trade_id: TradeId,
    pub symbol: Symbol,
    pub price: Price,
    pub quantity: Quantity,
    pub buyer_order_id: OrderId,
    pub seller_order_id: OrderId,
    pub buyer_is_maker: bool,
    pub timestamp: Timestamp,
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
