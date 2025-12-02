//! Transport configuration

/// Subjects for logical message routing
///
/// Even with tokio channels, we use logical subject names for:
/// - Clear message categorization
/// - Easy migration to distributed transports later
/// - Debugging and logging
pub struct Subjects;

impl Subjects {
    // Market Data (Gateway → Internal)

    /// Order book updates for a specific instrument: `md.BTC-USD`
    pub fn market_data(instrument: &str) -> String {
        format!("md.{}", instrument)
    }

    /// Subscribe to all market data: `md.*`
    pub fn market_data_all() -> &'static str {
        "md.*"
    }

    /// Snapshot request for an instrument: `md.BTC-USD.snapshot`
    pub fn snapshot_request(instrument: &str) -> String {
        format!("md.{}.snapshot", instrument)
    }

    /// Trade notifications for a specific instrument: `trades.BTC-USD`
    pub fn trades(instrument: &str) -> String {
        format!("trades.{}", instrument)
    }

    /// Subscribe to all trades: `trades.*`
    pub fn trades_all() -> &'static str {
        "trades.*"
    }

    // Orders (Internal → Gateway → Exchange)

    /// Order submission requests
    pub const ORDER_SUBMIT: &'static str = "orders.submit";

    /// Cancel requests
    pub const ORDER_CANCEL: &'static str = "orders.cancel";

    /// Order response for a specific client: `orders.response.{client_id}`
    pub fn order_response(client_id: &str) -> String {
        format!("orders.response.{}", client_id)
    }

    /// Subscribe to all order responses: `orders.response.*`
    pub fn order_response_all() -> &'static str {
        "orders.response.*"
    }

    // Control

    /// System heartbeats
    pub const HEARTBEAT: &'static str = "control.heartbeat";

    /// Component status updates
    pub const STATUS: &'static str = "control.status";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subjects() {
        assert_eq!(Subjects::market_data("BTC-USD"), "md.BTC-USD");
        assert_eq!(Subjects::trades("ETH-USD"), "trades.ETH-USD");
        assert_eq!(
            Subjects::order_response("client-123"),
            "orders.response.client-123"
        );
    }
}
