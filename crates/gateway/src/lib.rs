//! Athena Gateway
//!
//! Gateway layer for the Athena trading system. Provides:
//! - Transport abstraction (tokio channels, with traits for future transports)
//! - Wire message types for market data and orders
//! - Exchange adapters (simulator, Binance, etc.)
//!
//! ## Architecture
//!
//! ```text
//! External World (Binance, Simulator)
//!         │
//!    ┌────▼────┐
//!    │ Gateway │
//!    │ In/Out  │
//!    └────┬────┘
//!         │ Channels:
//!         │ md.{instrument}, trades.{instrument}, orders.submit
//!    ┌────▼────┐
//!    │Internal │
//!    │Systems  │
//!    └─────────┘
//! ```
//!
//! ## Transport
//!
//! Currently uses tokio channels for single-process operation (training/backtesting).
//! The `Publisher`/`Subscriber`/`Requester` traits allow plugging in other
//! transports (NATS, Aeron, etc.) when needed.

pub mod adapters;
pub mod error;
pub mod messages;
pub mod transport;

// Re-export commonly used types
pub use error::{GatewayError, TransportError};
pub use messages::{
    market_data::{BookLevel, OrderBookUpdate, TradeMessage},
    order::{CancelRequest, OrderRequest, OrderResponse},
};
pub use transport::{
    Publisher, Requester, Subjects, Subscriber,
    channel::{ChannelPublisher, ChannelRequester, ChannelResponder, ChannelSubscriber},
};
