//! Simulator Gateway In - normalizes exchange-sim data to wire format
//!
//! Receives messages from exchange-sim and publishes them via channels
//! to internal components (OB reconstructors, strategies, etc.)

use crate::error::GatewayError;
use crate::messages::market_data::{AggressorSide, BookLevel, OrderBookUpdate, TradeMessage};
use crate::messages::order::{OrderResponse, OrderStatusWire};
use crate::transport::Publisher;
use crate::transport::config::Subjects;
use athena_core::{OrderStatus, Trade};
use log::debug;
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU64, Ordering};

/// Parameters for publishing an order response
pub struct OrderResponseParams<'a> {
    pub client_order_id: &'a str,
    pub exchange_order_id: Option<&'a str>,
    pub status: OrderStatus,
    pub filled_qty: Decimal,
    pub avg_price: Option<Decimal>,
    pub reject_reason: Option<&'a str>,
    pub timestamp_ns: i64,
}

/// Gateway In for the simulator
///
/// Receives messages from exchange-sim and publishes normalized versions
/// to internal components via channels.
pub struct SimulatorGatewayIn {
    /// Publisher for market data (order book updates)
    md_publisher: Box<dyn Publisher<OrderBookUpdate> + Send + Sync>,
    /// Publisher for trades
    trade_publisher: Box<dyn Publisher<TradeMessage> + Send + Sync>,
    /// Publisher for order responses
    order_publisher: Box<dyn Publisher<OrderResponse> + Send + Sync>,
    /// Sequence number for order book updates
    sequence: AtomicU64,
}

impl SimulatorGatewayIn {
    /// Create with custom publishers
    pub fn new(
        md_publisher: Box<dyn Publisher<OrderBookUpdate> + Send + Sync>,
        trade_publisher: Box<dyn Publisher<TradeMessage> + Send + Sync>,
        order_publisher: Box<dyn Publisher<OrderResponse> + Send + Sync>,
    ) -> Self {
        Self {
            md_publisher,
            trade_publisher,
            order_publisher,
            sequence: AtomicU64::new(0),
        }
    }

    /// Publish a trade from the exchange
    pub async fn publish_trade(&self, trade: &Trade) -> Result<(), GatewayError> {
        let trade_msg = self.normalize_trade(trade);
        let subject = Subjects::trades(&trade.instrument_id.to_string());

        debug!(
            "Publishing trade {} on subject {}",
            trade_msg.trade_id, subject
        );

        self.trade_publisher
            .publish_to(&subject, &trade_msg)
            .await
            .map_err(GatewayError::Transport)?;

        Ok(())
    }

    /// Publish an order book snapshot
    pub async fn publish_snapshot(
        &self,
        instrument_id: &str,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
        timestamp_ns: i64,
    ) -> Result<(), GatewayError> {
        let seq = self.next_sequence();

        let update = OrderBookUpdate::snapshot(
            instrument_id,
            bids.into_iter()
                .map(|(p, q)| BookLevel::new(p, q))
                .collect(),
            asks.into_iter()
                .map(|(p, q)| BookLevel::new(p, q))
                .collect(),
            seq,
            timestamp_ns,
        );

        let subject = Subjects::market_data(instrument_id);
        debug!(
            "Publishing OB snapshot for {} on subject {}",
            instrument_id, subject
        );

        self.md_publisher
            .publish_to(&subject, &update)
            .await
            .map_err(GatewayError::Transport)?;

        Ok(())
    }

    /// Publish an order book delta (incremental update)
    pub async fn publish_delta(
        &self,
        instrument_id: &str,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
        timestamp_ns: i64,
    ) -> Result<(), GatewayError> {
        let seq = self.next_sequence();

        let update = OrderBookUpdate::delta(
            instrument_id,
            bids.into_iter()
                .map(|(p, q)| BookLevel::new(p, q))
                .collect(),
            asks.into_iter()
                .map(|(p, q)| BookLevel::new(p, q))
                .collect(),
            seq,
            timestamp_ns,
        );

        let subject = Subjects::market_data(instrument_id);

        self.md_publisher
            .publish_to(&subject, &update)
            .await
            .map_err(GatewayError::Transport)?;

        Ok(())
    }

    /// Publish an order response
    pub async fn publish_order_response(
        &self,
        params: OrderResponseParams<'_>,
    ) -> Result<(), GatewayError> {
        let response = OrderResponse {
            client_order_id: params.client_order_id.to_string(),
            exchange_order_id: params.exchange_order_id.map(|s| s.to_string()),
            status: self.convert_order_status(params.status),
            filled_qty: params.filled_qty,
            avg_price: params.avg_price,
            reject_reason: params.reject_reason.map(|s| s.to_string()),
            timestamp_ns: params.timestamp_ns,
        };

        let subject = Subjects::order_response(params.client_order_id);
        debug!(
            "Publishing order response for {} on subject {}",
            params.client_order_id, subject
        );

        self.order_publisher
            .publish_to(&subject, &response)
            .await
            .map_err(GatewayError::Transport)?;

        Ok(())
    }

    /// Normalize a Trade from athena-core to wire format
    fn normalize_trade(&self, trade: &Trade) -> TradeMessage {
        TradeMessage::new(
            trade.instrument_id.to_string(),
            trade.price,
            trade.quantity,
            AggressorSide::Buy, // TODO: Determine from order context
            trade.timestamp.timestamp_nanos_opt().unwrap_or(0),
            trade.id.to_string(),
        )
    }

    /// Convert internal OrderStatus to wire format
    fn convert_order_status(&self, status: OrderStatus) -> OrderStatusWire {
        match status {
            OrderStatus::New => OrderStatusWire::Accepted,
            OrderStatus::PartiallyFilled => OrderStatusWire::PartiallyFilled,
            OrderStatus::Filled => OrderStatusWire::Filled,
            OrderStatus::Canceled => OrderStatusWire::Cancelled,
            OrderStatus::Rejected => OrderStatusWire::Rejected,
            OrderStatus::Expired => OrderStatusWire::Expired,
        }
    }

    /// Get next sequence number
    fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::Subscriber;
    use crate::transport::channel::ChannelPublisher;
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_publish_trade() {
        let (md_pub, _md_sub) = ChannelPublisher::<OrderBookUpdate>::pair(10);
        let (trade_pub, mut trade_sub) = ChannelPublisher::<TradeMessage>::pair(10);
        let (order_pub, _order_sub) = ChannelPublisher::<OrderResponse>::pair(10);

        let gateway =
            SimulatorGatewayIn::new(Box::new(md_pub), Box::new(trade_pub), Box::new(order_pub));

        let trade = Trade::new_with_time(
            "BTC-USD",
            Uuid::new_v4(),
            Uuid::new_v4(),
            dec!(50000),
            dec!(1.5),
            Utc::now(),
        );

        gateway.publish_trade(&trade).await.unwrap();

        let msg = trade_sub.next().await.unwrap();
        assert_eq!(msg.instrument_id, "BTC-USD");
        assert_eq!(msg.price, dec!(50000));
        assert_eq!(msg.quantity, dec!(1.5));
    }
}
