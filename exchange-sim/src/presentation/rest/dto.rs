use crate::domain::Order;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Request to create a new order (Binance-compatible)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderRequest {
    pub symbol: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    #[serde(default)]
    pub time_in_force: Option<String>,
    pub quantity: String,
    #[serde(default)]
    pub price: Option<String>,
    #[serde(default)]
    pub stop_price: Option<String>,
    #[serde(default)]
    pub new_client_order_id: Option<String>,
}

/// Order response (Binance-compatible)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderResponse {
    pub symbol: String,
    pub order_id: i64,
    pub order_list_id: i64,
    pub client_order_id: String,
    pub transact_time: i64,
    pub price: String,
    pub orig_qty: String,
    pub executed_qty: String,
    pub cummulative_quote_qty: String,
    pub status: String,
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<FillResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FillResponse {
    pub price: String,
    pub qty: String,
    pub commission: String,
    pub commission_asset: String,
    pub trade_id: i64,
}

impl OrderResponse {
    pub fn from_order(order: &Order, fills: Vec<FillResponse>) -> Self {
        let executed_qty = order.filled_quantity.inner();
        let avg_price = if !fills.is_empty() {
            fills
                .iter()
                .map(|f| {
                    f.price.parse::<Decimal>().unwrap_or(Decimal::ZERO)
                        * f.qty.parse::<Decimal>().unwrap_or(Decimal::ZERO)
                })
                .sum::<Decimal>()
                / executed_qty
        } else {
            Decimal::ZERO
        };
        let cummulative_quote_qty = executed_qty * avg_price;

        OrderResponse {
            symbol: order.symbol.to_string(),
            order_id: order.id.as_u128() as i64,
            order_list_id: -1,
            client_order_id: order
                .client_order_id
                .clone()
                .unwrap_or_else(|| order.id.to_string()),
            transact_time: order.updated_at.timestamp_millis(),
            price: order
                .price
                .map(|p| p.to_string())
                .unwrap_or("0".to_string()),
            orig_qty: order.quantity.to_string(),
            executed_qty: order.filled_quantity.to_string(),
            cummulative_quote_qty: cummulative_quote_qty.to_string(),
            status: format!("{:?}", order.status).to_uppercase(),
            time_in_force: order.time_in_force.to_string(),
            order_type: order.order_type.to_string(),
            side: order.side.to_string(),
            fills,
        }
    }
}

/// Cancel order request
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderRequest {
    pub symbol: String,
    #[serde(default)]
    pub order_id: Option<i64>,
    #[serde(default)]
    pub orig_client_order_id: Option<String>,
}

/// Cancel order response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelOrderResponse {
    pub symbol: String,
    pub orig_client_order_id: String,
    pub order_id: i64,
    pub order_list_id: i64,
    pub client_order_id: String,
    pub price: String,
    pub orig_qty: String,
    pub executed_qty: String,
    pub cummulative_quote_qty: String,
    pub status: String,
    pub time_in_force: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
}

/// Depth request query params
#[derive(Debug, Clone, Deserialize)]
pub struct DepthQuery {
    pub symbol: String,
    #[serde(default = "default_depth_limit")]
    pub limit: usize,
}

fn default_depth_limit() -> usize {
    100
}

/// Depth response (Binance-compatible)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthResponse {
    pub last_update_id: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

/// Server time response
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTimeResponse {
    pub server_time: i64,
}

/// Ping response (empty)
#[derive(Debug, Clone, Serialize)]
pub struct PingResponse {}

/// Error response (Binance-compatible)
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub code: i32,
    pub msg: String,
}

impl ErrorResponse {
    pub fn new(code: i32, msg: impl Into<String>) -> Self {
        ErrorResponse {
            code,
            msg: msg.into(),
        }
    }
}

// Note: TryFrom implementations are in domain::value_objects
