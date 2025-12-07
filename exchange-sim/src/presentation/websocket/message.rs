use serde::{Deserialize, Serialize};

/// WebSocket incoming message
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "method", rename_all = "UPPERCASE")]
pub enum WsRequest {
    /// Subscribe to streams
    Subscribe { id: u64, params: Vec<String> },
    /// Unsubscribe from streams
    Unsubscribe { id: u64, params: Vec<String> },
    /// List current subscriptions
    #[serde(rename = "LIST_SUBSCRIPTIONS")]
    ListSubscriptions { id: u64 },
}

/// WebSocket response message
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum WsResponse {
    /// Response to subscribe/unsubscribe
    Result {
        id: u64,
        result: Option<serde_json::Value>,
    },
    /// Stream data
    Stream {
        stream: String,
        data: serde_json::Value,
    },
    /// Error response
    Error {
        id: Option<u64>,
        code: i32,
        msg: String,
    },
}

impl WsResponse {
    pub fn ok(id: u64) -> Self {
        WsResponse::Result { id, result: None }
    }

    pub fn subscriptions(id: u64, subs: Vec<String>) -> Self {
        WsResponse::Result {
            id,
            result: Some(serde_json::json!(subs)),
        }
    }

    pub fn error(id: Option<u64>, code: i32, msg: impl Into<String>) -> Self {
        WsResponse::Error {
            id,
            code,
            msg: msg.into(),
        }
    }
}

/// Binance-compatible stream message types
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthUpdateMessage {
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
    pub bids: Vec<[String; 2]>,
    #[serde(rename = "a")]
    pub asks: Vec<[String; 2]>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeMessage {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "t")]
    pub trade_id: i64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub quantity: String,
    #[serde(rename = "b")]
    pub buyer_order_id: i64,
    #[serde(rename = "a")]
    pub seller_order_id: i64,
    #[serde(rename = "T")]
    pub trade_time: i64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

/// Generic wrapper for all stream messages
#[derive(Debug, Clone, Serialize)]
pub struct WsMessage {
    pub stream: String,
    pub data: serde_json::Value,
}
