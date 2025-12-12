//! Market Data Subscriber
//!
//! Subscribes to market data from gateway, builds local order books,
//! and requests snapshots when sequence gaps are detected.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use trading_core::{DepthUpdate, OrderBookSnapshot, SnapshotRequest};
use transport::{
    MSG_DEPTH_UPDATE, MSG_ORDER_BOOK_SNAPSHOT, Publisher, Subscriber, TransportError, WireMessage,
};

use crate::domain::order_book::OrderBookManager;

/// Subscriber for market data updates from gateway
pub struct MarketDataSubscriber {
    transport: Box<dyn Subscriber>,
    snapshot_requester: Box<dyn Publisher>,
    books: OrderBookManager,
    last_received_sequence: AtomicU64,
}

impl MarketDataSubscriber {
    /// Create a new market data subscriber
    pub fn new(
        transport: Box<dyn Subscriber>,
        snapshot_requester: Box<dyn Publisher>,
        books: OrderBookManager,
    ) -> Self {
        MarketDataSubscriber {
            transport,
            snapshot_requester,
            books,
            last_received_sequence: AtomicU64::new(0),
        }
    }

    /// Poll for incoming messages and process them
    /// Returns number of messages processed
    pub fn poll(&self) -> Result<usize, TransportError> {
        let books = &self.books;
        let snapshot_req = &self.snapshot_requester;
        let last_seq = &self.last_received_sequence;

        self.transport.poll(&mut |data| {
            if let Ok(msg) = bincode::deserialize::<WireMessage>(data) {
                // Track sequence
                last_seq.store(msg.sequence, Ordering::Relaxed);

                match msg.msg_type {
                    MSG_ORDER_BOOK_SNAPSHOT => {
                        if let Ok(snapshot) =
                            bincode::deserialize::<OrderBookSnapshot>(&msg.payload)
                        {
                            books.apply_snapshot(&snapshot);
                            tracing::debug!(
                                "Applied snapshot for {}:{} at {}",
                                snapshot.exchange,
                                snapshot.symbol,
                                snapshot.last_update_id
                            );
                        }
                    }
                    MSG_DEPTH_UPDATE => {
                        if let Ok(update) = bincode::deserialize::<DepthUpdate>(&msg.payload)
                            && !books.apply_delta(&update)
                        {
                            // Sequence gap detected - request snapshot
                            tracing::warn!(
                                "Sequence gap for {}:{}, requesting snapshot",
                                update.exchange,
                                update.symbol
                            );
                            let _ = Self::request_snapshot_internal(
                                snapshot_req.as_ref(),
                                &update.exchange,
                                &update.symbol,
                            );
                        }
                    }
                    _ => {
                        tracing::trace!("Unknown message type: {}", msg.msg_type);
                    }
                }
            }
        })
    }

    /// Request a snapshot for a specific symbol
    pub fn request_snapshot(&self, exchange: &str, symbol: &str) -> Result<(), TransportError> {
        Self::request_snapshot_internal(self.snapshot_requester.as_ref(), exchange, symbol)
    }

    fn request_snapshot_internal(
        publisher: &dyn Publisher,
        exchange: &str,
        symbol: &str,
    ) -> Result<(), TransportError> {
        let request = SnapshotRequest::new(exchange, symbol);
        let payload = bincode::serialize(&request)
            .map_err(|e| TransportError::Serialization(e.to_string()))?;

        let msg = WireMessage {
            msg_type: transport::MSG_SNAPSHOT_REQUEST,
            sequence: 0,
            timestamp_ns: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
            source: "strategy".to_string(),
            payload,
        };

        let data =
            bincode::serialize(&msg).map_err(|e| TransportError::Serialization(e.to_string()))?;

        publisher.publish(&data)
    }

    /// Get a reference to the order book manager
    pub fn books(&self) -> &OrderBookManager {
        &self.books
    }

    /// Get the last received sequence number
    pub fn last_sequence(&self) -> u64 {
        self.last_received_sequence.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trading_core::CompactLevel;
    use transport::channel_pair;

    #[test]
    fn test_receive_snapshot() {
        let (md_pub, md_sub) = channel_pair(100);
        let (snap_req_pub, _snap_req_sub) = channel_pair(100);

        let books = OrderBookManager::new();
        let subscriber =
            MarketDataSubscriber::new(Box::new(md_sub), Box::new(snap_req_pub), books.clone());

        // Publish a snapshot
        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![CompactLevel::new(50100_00000000, 1_50000000)],
        };

        let payload = bincode::serialize(&snapshot).unwrap();
        let msg = WireMessage {
            msg_type: MSG_ORDER_BOOK_SNAPSHOT,
            sequence: 1,
            timestamp_ns: 0,
            source: "gateway".to_string(),
            payload,
        };
        let data = bincode::serialize(&msg).unwrap();
        md_pub.publish(&data).unwrap();

        // Poll and process
        let processed = subscriber.poll().unwrap();
        assert_eq!(processed, 1);

        // Verify order book was updated
        let book = subscriber.books().book("binance", "BTCUSDT");
        assert!(book.is_initialized());
        assert_eq!(book.last_update_id(), 100);
    }

    #[test]
    fn test_receive_delta() {
        let (md_pub, md_sub) = channel_pair(100);
        let (snap_req_pub, _snap_req_sub) = channel_pair(100);

        let books = OrderBookManager::new();
        let subscriber =
            MarketDataSubscriber::new(Box::new(md_sub), Box::new(snap_req_pub), books.clone());

        // First send snapshot
        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![],
        };
        let payload = bincode::serialize(&snapshot).unwrap();
        let msg = WireMessage {
            msg_type: MSG_ORDER_BOOK_SNAPSHOT,
            sequence: 1,
            timestamp_ns: 0,
            source: "gateway".to_string(),
            payload,
        };
        md_pub.publish(&bincode::serialize(&msg).unwrap()).unwrap();
        subscriber.poll().unwrap();

        // Then send delta
        let update = DepthUpdate {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            first_update_id: 101,
            final_update_id: 101,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 2_00000000)],
            asks: vec![],
        };
        let payload = bincode::serialize(&update).unwrap();
        let msg = WireMessage {
            msg_type: MSG_DEPTH_UPDATE,
            sequence: 2,
            timestamp_ns: 0,
            source: "gateway".to_string(),
            payload,
        };
        md_pub.publish(&bincode::serialize(&msg).unwrap()).unwrap();
        subscriber.poll().unwrap();

        // Verify delta was applied
        let book = subscriber.books().book("binance", "BTCUSDT");
        assert_eq!(book.last_update_id(), 101);
        assert_eq!(book.best_bid().unwrap().quantity.raw(), 2_00000000);
    }

    #[test]
    fn test_sequence_gap_triggers_snapshot_request() {
        let (md_pub, md_sub) = channel_pair(100);
        let (snap_req_pub, snap_req_sub) = channel_pair(100);

        let books = OrderBookManager::new();
        let subscriber =
            MarketDataSubscriber::new(Box::new(md_sub), Box::new(snap_req_pub), books.clone());

        // First send snapshot
        let snapshot = OrderBookSnapshot {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            last_update_id: 100,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 1_00000000)],
            asks: vec![],
        };
        let payload = bincode::serialize(&snapshot).unwrap();
        let msg = WireMessage {
            msg_type: MSG_ORDER_BOOK_SNAPSHOT,
            sequence: 1,
            timestamp_ns: 0,
            source: "gateway".to_string(),
            payload,
        };
        md_pub.publish(&bincode::serialize(&msg).unwrap()).unwrap();
        subscriber.poll().unwrap();

        // Send out-of-sequence delta (gap: expected 101, got 105)
        let update = DepthUpdate {
            exchange: "binance".to_string(),
            symbol: "BTCUSDT".to_string(),
            first_update_id: 105,
            final_update_id: 105,
            timestamp_ns: 0,
            bids: vec![CompactLevel::new(50000_00000000, 2_00000000)],
            asks: vec![],
        };
        let payload = bincode::serialize(&update).unwrap();
        let msg = WireMessage {
            msg_type: MSG_DEPTH_UPDATE,
            sequence: 2,
            timestamp_ns: 0,
            source: "gateway".to_string(),
            payload,
        };
        md_pub.publish(&bincode::serialize(&msg).unwrap()).unwrap();
        subscriber.poll().unwrap();

        // Verify snapshot request was sent
        use transport::Subscriber as _;
        let mut request_received = false;
        snap_req_sub
            .poll(&mut |data| {
                let msg: WireMessage = bincode::deserialize(data).unwrap();
                assert_eq!(msg.msg_type, transport::MSG_SNAPSHOT_REQUEST);
                let req: SnapshotRequest = bincode::deserialize(&msg.payload).unwrap();
                assert_eq!(req.exchange, "binance");
                assert_eq!(req.symbol, "BTCUSDT");
                request_received = true;
            })
            .unwrap();

        assert!(request_received);

        // Original data unchanged (delta was rejected)
        let book = subscriber.books().book("binance", "BTCUSDT");
        assert_eq!(book.last_update_id(), 100);
    }
}
