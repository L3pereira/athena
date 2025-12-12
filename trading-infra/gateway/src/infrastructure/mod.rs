//! Infrastructure Layer - Inbound adapters from upstream systems
//!
//! This layer contains adapters for systems we consume from:
//! - RestClient: HTTP client for exchange REST APIs
//! - WsClient: WebSocket client for exchange streams
//! - Parsers: Stream data parsing from exchange formats
//!
//! Follows Hexagonal Architecture:
//! - Infrastructure = inbound (exchanges → gateway)
//! - Presentation = outbound (gateway → consumers)

pub mod parsers;
pub mod rest_client;
pub mod ws_client;

pub use parsers::{DepthParser, StreamDataParser, TradeParser};
pub use rest_client::{NewOrderRequest, OrderResponse, RestClient, RestError};
pub use ws_client::{WsClient, WsError, WsRequestSender};
