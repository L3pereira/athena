//! Order message types

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderTypeWire {
    Limit,
    Market,
}

impl OrderTypeWire {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Limit => "limit",
            Self::Market => "market",
        }
    }
}

/// Time in force
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForceWire {
    /// Good Till Cancelled
    Gtc,
    /// Immediate Or Cancel
    Ioc,
    /// Fill Or Kill
    Fok,
}

impl TimeInForceWire {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gtc => "gtc",
            Self::Ioc => "ioc",
            Self::Fok => "fok",
        }
    }
}

/// Order submission request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    /// Client-assigned order ID for correlation
    pub client_order_id: String,
    /// Instrument to trade
    pub instrument_id: String,
    /// Buy or sell
    pub side: OrderSide,
    /// Limit or market
    pub order_type: OrderTypeWire,
    /// Quantity to trade
    pub quantity: Decimal,
    /// Price (required for limit orders)
    pub price: Option<Decimal>,
    /// Time in force
    pub time_in_force: TimeInForceWire,
}

impl OrderRequest {
    /// Create a new limit order request
    pub fn limit(
        client_order_id: impl Into<String>,
        instrument_id: impl Into<String>,
        side: OrderSide,
        quantity: Decimal,
        price: Decimal,
        time_in_force: TimeInForceWire,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            instrument_id: instrument_id.into(),
            side,
            order_type: OrderTypeWire::Limit,
            quantity,
            price: Some(price),
            time_in_force,
        }
    }

    /// Create a new market order request
    pub fn market(
        client_order_id: impl Into<String>,
        instrument_id: impl Into<String>,
        side: OrderSide,
        quantity: Decimal,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            instrument_id: instrument_id.into(),
            side,
            order_type: OrderTypeWire::Market,
            quantity,
            price: None,
            time_in_force: TimeInForceWire::Ioc, // Market orders are always IOC
        }
    }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatusWire {
    /// Order accepted by exchange
    Accepted,
    /// Order rejected
    Rejected,
    /// Order partially filled
    PartiallyFilled,
    /// Order fully filled
    Filled,
    /// Order cancelled
    Cancelled,
    /// Order expired
    Expired,
}

impl OrderStatusWire {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::PartiallyFilled => "partially_filled",
            Self::Filled => "filled",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }
}

/// Order response from exchange
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    /// Client-assigned order ID (echoed back)
    pub client_order_id: String,
    /// Exchange-assigned order ID (if accepted)
    pub exchange_order_id: Option<String>,
    /// Current status
    pub status: OrderStatusWire,
    /// Cumulative filled quantity
    pub filled_qty: Decimal,
    /// Average fill price (if any fills)
    pub avg_price: Option<Decimal>,
    /// Rejection reason (if rejected)
    pub reject_reason: Option<String>,
    /// Timestamp in nanoseconds
    pub timestamp_ns: i64,
}

impl OrderResponse {
    /// Create an accepted response
    pub fn accepted(
        client_order_id: impl Into<String>,
        exchange_order_id: impl Into<String>,
        timestamp_ns: i64,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            exchange_order_id: Some(exchange_order_id.into()),
            status: OrderStatusWire::Accepted,
            filled_qty: Decimal::ZERO,
            avg_price: None,
            reject_reason: None,
            timestamp_ns,
        }
    }

    /// Create a rejected response
    pub fn rejected(
        client_order_id: impl Into<String>,
        reason: impl Into<String>,
        timestamp_ns: i64,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            exchange_order_id: None,
            status: OrderStatusWire::Rejected,
            filled_qty: Decimal::ZERO,
            avg_price: None,
            reject_reason: Some(reason.into()),
            timestamp_ns,
        }
    }

    /// Create a filled response
    pub fn filled(
        client_order_id: impl Into<String>,
        exchange_order_id: impl Into<String>,
        filled_qty: Decimal,
        avg_price: Decimal,
        timestamp_ns: i64,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            exchange_order_id: Some(exchange_order_id.into()),
            status: OrderStatusWire::Filled,
            filled_qty,
            avg_price: Some(avg_price),
            reject_reason: None,
            timestamp_ns,
        }
    }

    /// Check if the order is terminal (no more updates expected)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            OrderStatusWire::Rejected
                | OrderStatusWire::Filled
                | OrderStatusWire::Cancelled
                | OrderStatusWire::Expired
        )
    }
}

/// Cancel order request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelRequest {
    /// Client-assigned order ID
    pub client_order_id: String,
    /// Exchange-assigned order ID (if known)
    pub exchange_order_id: Option<String>,
    /// Instrument (for routing)
    pub instrument_id: String,
}

impl CancelRequest {
    /// Create a cancel request by client order ID
    pub fn by_client_id(
        client_order_id: impl Into<String>,
        instrument_id: impl Into<String>,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            exchange_order_id: None,
            instrument_id: instrument_id.into(),
        }
    }

    /// Create a cancel request by exchange order ID
    pub fn by_exchange_id(
        client_order_id: impl Into<String>,
        exchange_order_id: impl Into<String>,
        instrument_id: impl Into<String>,
    ) -> Self {
        Self {
            client_order_id: client_order_id.into(),
            exchange_order_id: Some(exchange_order_id.into()),
            instrument_id: instrument_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_limit_order_request() {
        let order = OrderRequest::limit(
            "client-1",
            "BTC-USD",
            OrderSide::Buy,
            dec!(1.0),
            dec!(50000),
            TimeInForceWire::Gtc,
        );

        assert_eq!(order.client_order_id, "client-1");
        assert_eq!(order.instrument_id, "BTC-USD");
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.order_type, OrderTypeWire::Limit);
        assert_eq!(order.price, Some(dec!(50000)));
    }

    #[test]
    fn test_market_order_request() {
        let order = OrderRequest::market("client-2", "ETH-USD", OrderSide::Sell, dec!(10.0));

        assert_eq!(order.order_type, OrderTypeWire::Market);
        assert!(order.price.is_none());
        assert_eq!(order.time_in_force, TimeInForceWire::Ioc);
    }

    #[test]
    fn test_order_response_terminal() {
        let accepted = OrderResponse::accepted("client-1", "exch-1", 0);
        assert!(!accepted.is_terminal());

        let filled = OrderResponse::filled("client-1", "exch-1", dec!(1.0), dec!(50000), 0);
        assert!(filled.is_terminal());

        let rejected = OrderResponse::rejected("client-1", "insufficient funds", 0);
        assert!(rejected.is_terminal());
    }

    #[test]
    #[ignore = "bincode doesn't support rust_decimal's deserialize_any"]
    fn test_serialization() {
        let order = OrderRequest::limit(
            "client-1",
            "BTC-USD",
            OrderSide::Buy,
            dec!(1.0),
            dec!(50000),
            TimeInForceWire::Gtc,
        );

        let bytes = bincode::serialize(&order).unwrap();
        let decoded: OrderRequest = bincode::deserialize(&bytes).unwrap();

        assert_eq!(decoded.client_order_id, order.client_order_id);
        assert_eq!(decoded.price, order.price);
    }
}
