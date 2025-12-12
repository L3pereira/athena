//! Market Data Publisher
//!
//! Publishes order book deltas and snapshots to strategy via transport layer.
//! Gateway does NOT build order books - it only forwards deltas from exchanges.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use trading_core::{CompactLevel, DepthSnapshotEvent, DepthUpdate, OrderBookSnapshot};
use transport::{
    MSG_DEPTH_UPDATE, MSG_ORDER_BOOK_SNAPSHOT, Publisher, TransportError, WireMessage,
};

/// Publisher for market data updates to strategy processes
pub struct MarketDataPublisher {
    transport: Box<dyn Publisher>,
    sequence: AtomicU64,
    source: String,
}

impl MarketDataPublisher {
    /// Create a new market data publisher
    pub fn new(transport: Box<dyn Publisher>, source: impl Into<String>) -> Self {
        MarketDataPublisher {
            transport,
            sequence: AtomicU64::new(0),
            source: source.into(),
        }
    }

    fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst)
    }

    fn timestamp_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    /// Forward a depth update delta to strategy
    pub fn publish_delta(&self, update: &DepthUpdate) -> Result<(), TransportError> {
        let payload =
            bincode::serialize(update).map_err(|e| TransportError::Serialization(e.to_string()))?;

        let msg = WireMessage {
            msg_type: MSG_DEPTH_UPDATE,
            sequence: self.next_sequence(),
            timestamp_ns: Self::timestamp_ns(),
            source: self.source.clone(),
            payload,
        };

        let data =
            bincode::serialize(&msg).map_err(|e| TransportError::Serialization(e.to_string()))?;

        self.transport.publish(&data)
    }

    /// Forward a snapshot to strategy (when strategy requests it)
    pub fn publish_snapshot(
        &self,
        exchange: &str,
        symbol: &str,
        snapshot: &DepthSnapshotEvent,
    ) -> Result<(), TransportError> {
        // Convert DepthSnapshotEvent to our IPC format
        let ipc_snapshot = OrderBookSnapshot {
            exchange: exchange.to_string(),
            symbol: symbol.to_string(),
            last_update_id: snapshot.last_update_id,
            timestamp_ns: Self::timestamp_ns(),
            bids: snapshot
                .bids
                .iter()
                .map(|arr| {
                    let price = arr[0].parse::<f64>().unwrap_or(0.0);
                    let qty = arr[1].parse::<f64>().unwrap_or(0.0);
                    CompactLevel {
                        price_raw: (price * 100_000_000.0) as i64,
                        quantity_raw: (qty * 100_000_000.0) as i64,
                    }
                })
                .collect(),
            asks: snapshot
                .asks
                .iter()
                .map(|arr| {
                    let price = arr[0].parse::<f64>().unwrap_or(0.0);
                    let qty = arr[1].parse::<f64>().unwrap_or(0.0);
                    CompactLevel {
                        price_raw: (price * 100_000_000.0) as i64,
                        quantity_raw: (qty * 100_000_000.0) as i64,
                    }
                })
                .collect(),
        };

        let payload = bincode::serialize(&ipc_snapshot)
            .map_err(|e| TransportError::Serialization(e.to_string()))?;

        let msg = WireMessage {
            msg_type: MSG_ORDER_BOOK_SNAPSHOT,
            sequence: self.next_sequence(),
            timestamp_ns: Self::timestamp_ns(),
            source: self.source.clone(),
            payload,
        };

        let data =
            bincode::serialize(&msg).map_err(|e| TransportError::Serialization(e.to_string()))?;

        self.transport.publish(&data)
    }

    /// Get current sequence number
    pub fn sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Flush any buffered data
    pub fn flush(&self) -> Result<(), TransportError> {
        self.transport.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use transport::{Subscriber, channel_pair};

    #[test]
    fn test_publish_delta() {
        let (publisher, subscriber) = channel_pair(100);
        let md_pub = MarketDataPublisher::new(Box::new(publisher), "gateway");

        let update = DepthUpdate::new("binance", "BTCUSDT", 100, 105)
            .with_bids(vec![CompactLevel::new(50000_00000000, 1_00000000)])
            .with_asks(vec![CompactLevel::new(50001_00000000, 2_00000000)]);

        md_pub.publish_delta(&update).unwrap();

        assert_eq!(md_pub.sequence(), 1);

        // Verify we can receive and decode the message
        let mut received = false;
        subscriber
            .poll(&mut |data| {
                let msg: WireMessage = bincode::deserialize(data).unwrap();
                assert_eq!(msg.msg_type, MSG_DEPTH_UPDATE);
                assert_eq!(msg.sequence, 0);
                assert_eq!(msg.source, "gateway");

                let decoded: DepthUpdate = bincode::deserialize(&msg.payload).unwrap();
                assert_eq!(decoded.exchange, "binance");
                assert_eq!(decoded.symbol, "BTCUSDT");
                received = true;
            })
            .unwrap();

        assert!(received);
    }

    #[test]
    fn test_publish_snapshot() {
        let (publisher, subscriber) = channel_pair(100);
        let md_pub = MarketDataPublisher::new(Box::new(publisher), "gateway");

        let snapshot = DepthSnapshotEvent {
            last_update_id: 12345,
            bids: vec![["50000.0".to_string(), "1.5".to_string()]],
            asks: vec![["50001.0".to_string(), "2.0".to_string()]],
        };

        md_pub
            .publish_snapshot("binance", "BTCUSDT", &snapshot)
            .unwrap();

        let mut received = false;
        subscriber
            .poll(&mut |data| {
                let msg: WireMessage = bincode::deserialize(data).unwrap();
                assert_eq!(msg.msg_type, MSG_ORDER_BOOK_SNAPSHOT);

                let decoded: OrderBookSnapshot = bincode::deserialize(&msg.payload).unwrap();
                assert_eq!(decoded.exchange, "binance");
                assert_eq!(decoded.symbol, "BTCUSDT");
                assert_eq!(decoded.last_update_id, 12345);
                received = true;
            })
            .unwrap();

        assert!(received);
    }
}
