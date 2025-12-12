use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use trading_core::{DepthSnapshotEvent, OrderId, Price, Quantity, Side, TimeInForce};

use crate::domain::{DepthFetcher, FetchError};

#[derive(Error, Debug)]
pub enum RestError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {code} - {msg}")]
    Api { code: i32, msg: String },
    #[error("Parse error: {0}")]
    Parse(String),
}

/// Convert infrastructure RestError to domain FetchError
impl From<RestError> for FetchError {
    fn from(err: RestError) -> Self {
        match err {
            RestError::Http(e) => FetchError::Network(e.to_string()),
            RestError::Api { code, msg } => FetchError::Api { code, message: msg },
            RestError::Parse(msg) => FetchError::Parse(msg),
        }
    }
}

/// REST API client for the exchange simulator
/// Infrastructure component - handles HTTP communication
#[derive(Clone)]
pub struct RestClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl RestClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        RestClient {
            client: Client::new(),
            base_url,
            api_key,
        }
    }

    /// Get server time
    pub async fn get_server_time(&self) -> Result<i64, RestError> {
        #[derive(Deserialize)]
        struct TimeResponse {
            #[serde(rename = "serverTime")]
            server_time: i64,
        }

        let resp: TimeResponse = self.get("/api/v3/time").await?;
        Ok(resp.server_time)
    }

    /// Get order book depth snapshot
    pub async fn get_depth(
        &self,
        symbol: &str,
        limit: Option<u32>,
    ) -> Result<DepthSnapshotEvent, RestError> {
        let limit = limit.unwrap_or(100);
        let path = format!("/api/v3/depth?symbol={}&limit={}", symbol, limit);
        self.get(&path).await
    }

    /// Place a new order
    pub async fn place_order(&self, request: NewOrderRequest) -> Result<OrderResponse, RestError> {
        self.post("/api/v3/order", &request).await
    }

    /// Cancel an order
    pub async fn cancel_order(
        &self,
        symbol: &str,
        order_id: OrderId,
    ) -> Result<OrderResponse, RestError> {
        let path = format!("/api/v3/order?symbol={}&orderId={}", symbol, order_id);
        self.delete(&path).await
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, RestError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, RestError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, RestError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .header("X-MBX-APIKEY", &self.api_key)
            .send()
            .await?;

        self.handle_response(resp).await
    }

    async fn handle_response<T: DeserializeOwned>(
        &self,
        resp: reqwest::Response,
    ) -> Result<T, RestError> {
        let status = resp.status();
        let text = resp.text().await?;

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ApiError>(&text) {
                return Err(RestError::Api {
                    code: err.code,
                    msg: err.msg,
                });
            }
            return Err(RestError::Parse(format!("HTTP {}: {}", status, text)));
        }

        serde_json::from_str(&text).map_err(|e| RestError::Parse(e.to_string()))
    }
}

#[derive(Deserialize)]
struct ApiError {
    code: i32,
    msg: String,
}

/// Request to place a new order
#[derive(Debug, Clone, Serialize)]
pub struct NewOrderRequest {
    pub symbol: String,
    pub side: Side,
    #[serde(rename = "type")]
    pub order_type: String,
    #[serde(rename = "timeInForce", skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    pub quantity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
    #[serde(rename = "newClientOrderId", skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
}

impl NewOrderRequest {
    pub fn limit(symbol: &str, side: Side, quantity: Quantity, price: Price) -> Self {
        NewOrderRequest {
            symbol: symbol.to_string(),
            side,
            order_type: "LIMIT".to_string(),
            time_in_force: Some(TimeInForce::Gtc),
            quantity: quantity.to_string(),
            price: Some(price.to_string()),
            client_order_id: None,
        }
    }

    pub fn market(symbol: &str, side: Side, quantity: Quantity) -> Self {
        NewOrderRequest {
            symbol: symbol.to_string(),
            side,
            order_type: "MARKET".to_string(),
            time_in_force: None,
            quantity: quantity.to_string(),
            price: None,
            client_order_id: None,
        }
    }

    pub fn with_client_order_id(mut self, id: impl Into<String>) -> Self {
        self.client_order_id = Some(id.into());
        self
    }
}

/// Response from order operations
#[derive(Debug, Clone, Deserialize)]
pub struct OrderResponse {
    pub symbol: String,
    #[serde(rename = "orderId")]
    pub order_id: u64,
    #[serde(rename = "clientOrderId")]
    pub client_order_id: Option<String>,
    #[serde(rename = "transactTime")]
    pub transact_time: Option<i64>,
    pub price: String,
    #[serde(rename = "origQty")]
    pub orig_qty: String,
    #[serde(rename = "executedQty")]
    pub executed_qty: String,
    pub status: String,
    #[serde(rename = "timeInForce")]
    pub time_in_force: Option<String>,
    #[serde(rename = "type")]
    pub order_type: String,
    pub side: String,
}

/// Implement DepthFetcher trait for RestClient (Dependency Inversion)
///
/// Converts infrastructure RestError to domain FetchError to maintain
/// proper dependency direction (infrastructure -> domain).
#[async_trait]
impl DepthFetcher for RestClient {
    async fn get_depth(
        &self,
        symbol: &str,
        limit: Option<u32>,
    ) -> Result<DepthSnapshotEvent, FetchError> {
        RestClient::get_depth(self, symbol, limit)
            .await
            .map_err(FetchError::from)
    }
}
