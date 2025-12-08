use crate::domain::{ExchangeEvent, PriceLevel};
use crate::infrastructure::BroadcastEventPublisher;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

use super::message::{DepthUpdateMessage, TradeMessage, WsMessage};

/// Type alias for depth snapshot state: (bids, asks, update_id)
type DepthSnapshot = (Vec<PriceLevel>, Vec<PriceLevel>, u64);

// ============================================================================
// Stream Type (OCP-compliant)
// ============================================================================

/// Supported WebSocket stream types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamType {
    Depth,
    Depth100ms,
    Depth1000ms,
    Trade,
    AggTrade,
}

impl StreamType {
    /// Parse stream type from string suffix
    pub fn from_suffix(suffix: &str) -> Option<Self> {
        match suffix {
            "depth" => Some(Self::Depth),
            "depth@100ms" => Some(Self::Depth100ms),
            "depth@1000ms" => Some(Self::Depth1000ms),
            "trade" => Some(Self::Trade),
            "aggTrade" => Some(Self::AggTrade),
            _ => None,
        }
    }

    /// Check if this stream type is depth-related
    pub fn is_depth(&self) -> bool {
        matches!(self, Self::Depth | Self::Depth100ms | Self::Depth1000ms)
    }
}

/// Parsed stream name containing symbol and type
pub struct ParsedStream {
    pub symbol: String,
    pub stream_type: StreamType,
}

impl ParsedStream {
    /// Parse a stream name like "btcusdt@depth" or "btcusdt@trade"
    pub fn parse(stream: &str) -> Option<Self> {
        let parts: Vec<&str> = stream.split('@').collect();
        if parts.len() != 2 {
            return None;
        }

        let symbol = parts[0].to_uppercase();
        let stream_type = StreamType::from_suffix(parts[1])?;

        Some(Self {
            symbol,
            stream_type,
        })
    }
}

/// Manages WebSocket stream subscriptions
pub struct StreamManager {
    publisher: Arc<BroadcastEventPublisher>,
    /// Track which symbols have depth streaming enabled
    depth_streams: Arc<DashMap<String, bool>>,
    /// Track previous depth state for delta calculation
    previous_depth: Arc<DashMap<String, DepthSnapshot>>,
}

impl StreamManager {
    pub fn new(publisher: Arc<BroadcastEventPublisher>) -> Self {
        StreamManager {
            publisher,
            depth_streams: Arc::new(DashMap::new()),
            previous_depth: Arc::new(DashMap::new()),
        }
    }

    /// Subscribe to a stream and return a receiver
    pub fn subscribe(&self, stream: &str) -> Option<broadcast::Receiver<ExchangeEvent>> {
        let parsed = ParsedStream::parse(stream)?;

        // Track depth streams for delta calculation
        if parsed.stream_type.is_depth() {
            self.depth_streams.insert(parsed.symbol.clone(), true);
        }

        Some(self.publisher.subscribe_symbol(&parsed.symbol))
    }

    /// Unsubscribe from a stream
    pub fn unsubscribe(&self, stream: &str) {
        if let Some(parsed) = ParsedStream::parse(stream) {
            if parsed.stream_type.is_depth() {
                self.depth_streams.remove(&parsed.symbol);
            }
            self.publisher.unsubscribe();
        }
    }

    /// Convert an exchange event to a WebSocket message
    pub fn event_to_message(&self, stream: &str, event: &ExchangeEvent) -> Option<WsMessage> {
        let parsed = ParsedStream::parse(stream)?;

        match (&parsed.stream_type, event) {
            (st, ExchangeEvent::DepthUpdate(update)) if st.is_depth() => Some(WsMessage {
                stream: stream.to_string(),
                data: serde_json::to_value(update).ok()?,
            }),
            (StreamType::Trade, ExchangeEvent::TradeExecuted(trade)) => {
                let msg = TradeMessage {
                    event_type: "trade".to_string(),
                    event_time: trade.timestamp.timestamp_millis(),
                    symbol: trade.symbol.to_string(),
                    trade_id: trade.trade_id.as_u128() as i64,
                    price: trade.price.to_string(),
                    quantity: trade.quantity.to_string(),
                    buyer_order_id: trade.buyer_order_id.as_u128() as i64,
                    seller_order_id: trade.seller_order_id.as_u128() as i64,
                    trade_time: trade.timestamp.timestamp_millis(),
                    is_buyer_maker: trade.buyer_is_maker,
                };
                Some(WsMessage {
                    stream: stream.to_string(),
                    data: serde_json::to_value(msg).ok()?,
                })
            }
            _ => None,
        }
    }

    /// Create a depth update message from current and previous state
    pub fn create_depth_update(
        &self,
        symbol: &str,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        first_update_id: u64,
        final_update_id: u64,
        event_time: i64,
    ) -> DepthUpdateMessage {
        // Get previous state for delta calculation
        let (prev_bids, prev_asks) = self
            .previous_depth
            .get(symbol)
            .map(|r| (r.0.clone(), r.1.clone()))
            .unwrap_or_default();

        // Calculate deltas (simplified - in production, compute actual differences)
        let bid_deltas = Self::calculate_deltas(&prev_bids, &bids);
        let ask_deltas = Self::calculate_deltas(&prev_asks, &asks);

        // Store current state
        self.previous_depth
            .insert(symbol.to_string(), (bids, asks, final_update_id));

        DepthUpdateMessage {
            event_type: "depthUpdate".to_string(),
            event_time,
            symbol: symbol.to_string(),
            first_update_id,
            final_update_id,
            bids: bid_deltas,
            asks: ask_deltas,
        }
    }

    /// Calculate price level deltas between previous and current state
    fn calculate_deltas(prev: &[PriceLevel], current: &[PriceLevel]) -> Vec<[String; 2]> {
        use std::collections::HashMap;

        let prev_map: HashMap<_, _> = prev
            .iter()
            .map(|l| (l.price.to_string(), l.quantity))
            .collect();

        let current_map: HashMap<_, _> = current
            .iter()
            .map(|l| (l.price.to_string(), l.quantity))
            .collect();

        let mut deltas = Vec::new();

        // Find changed or new levels
        for (price, qty) in &current_map {
            match prev_map.get(price) {
                Some(prev_qty) if prev_qty != qty => {
                    deltas.push([price.clone(), qty.to_string()]);
                }
                None => {
                    deltas.push([price.clone(), qty.to_string()]);
                }
                _ => {}
            }
        }

        // Find removed levels (quantity = 0)
        for price in prev_map.keys() {
            if !current_map.contains_key(price) {
                deltas.push([price.clone(), "0".to_string()]);
            }
        }

        deltas
    }
}

impl Clone for StreamManager {
    fn clone(&self) -> Self {
        StreamManager {
            publisher: Arc::clone(&self.publisher),
            depth_streams: Arc::clone(&self.depth_streams),
            previous_depth: Arc::clone(&self.previous_depth),
        }
    }
}
