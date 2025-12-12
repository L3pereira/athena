use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::traits::StreamParser;

/// Parsed stream data from WebSocket - core domain event
#[derive(Debug, Clone)]
pub enum StreamData {
    DepthUpdate {
        symbol: String,
        event_time: i64,
        first_update_id: u64,
        final_update_id: u64,
        bids: Vec<[String; 2]>,
        asks: Vec<[String; 2]>,
    },
    Trade {
        symbol: String,
        trade_id: u64,
        price: String,
        quantity: String,
        buyer_order_id: u64,
        seller_order_id: u64,
        trade_time: i64,
        is_buyer_maker: bool,
    },
}

impl StreamData {
    /// Parse stream data using injected parsers (Dependency Inversion compliant)
    ///
    /// Parsers are injected from the caller (typically infrastructure layer),
    /// keeping the domain layer free of infrastructure dependencies.
    ///
    /// # Example
    /// ```ignore
    /// let parsers: &[&dyn StreamParser] = &[&DepthParser, &TradeParser];
    /// let data = StreamData::parse_with(stream, json, parsers);
    /// ```
    pub fn parse_with(stream: &str, data: &Value, parsers: &[&dyn StreamParser]) -> Option<Self> {
        for parser in parsers {
            if parser.can_parse(stream)
                && let Some(result) = parser.parse(stream, data)
            {
                return Some(result);
            }
        }
        None
    }
}

/// Events received from the WebSocket connection
#[derive(Debug, Clone)]
pub enum WsEvent {
    /// Successful response to a request
    Response {
        id: u64,
        result: Option<serde_json::Value>,
    },
    /// Parsed stream data
    StreamData(StreamData),
    /// Raw message (couldn't parse)
    RawMessage(String),
    /// API error
    ApiError {
        id: Option<u64>,
        code: i32,
        msg: String,
    },
    /// Connection error
    Error(String),
    /// Disconnected
    Disconnected,
}

/// WebSocket request messages (Binance-compatible)
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "method", rename_all = "UPPERCASE")]
pub enum WsRequest {
    Subscribe {
        id: u64,
        params: Vec<String>,
    },
    Unsubscribe {
        id: u64,
        params: Vec<String>,
    },
    #[serde(rename = "LIST_SUBSCRIPTIONS")]
    ListSubscriptions {
        id: u64,
    },
}

impl WsRequest {
    pub fn subscribe(id: u64, streams: Vec<String>) -> Self {
        WsRequest::Subscribe {
            id,
            params: streams,
        }
    }

    pub fn unsubscribe(id: u64, streams: Vec<String>) -> Self {
        WsRequest::Unsubscribe {
            id,
            params: streams,
        }
    }

    pub fn list_subscriptions(id: u64) -> Self {
        WsRequest::ListSubscriptions { id }
    }
}

/// WebSocket response messages
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WsResponse {
    /// Successful response to a request
    Result { id: u64, result: Option<Value> },
    /// Stream data (market events)
    Stream { stream: String, data: Value },
    /// Error response
    Error {
        id: Option<u64>,
        code: i32,
        msg: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_serialization() {
        let req = WsRequest::subscribe(1, vec!["btcusdt@depth".to_string()]);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("SUBSCRIBE"));
        assert!(json.contains("btcusdt@depth"));
    }
}
