//! Agent Actions
//!
//! Actions that agents can take in the market.

use trading_core::{Price, Quantity, Side};

/// Order type for agent submissions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    /// Limit order (rests on book)
    Limit,
    /// Market order (immediate execution)
    Market,
    /// Post-only (rejected if would cross)
    PostOnly,
}

/// Time in force for orders
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeInForce {
    /// Good til canceled
    Gtc,
    /// Immediate or cancel
    Ioc,
    /// Fill or kill
    Fok,
}

impl Default for TimeInForce {
    fn default() -> Self {
        Self::Gtc
    }
}

/// Actions an agent can take
#[derive(Debug, Clone)]
pub enum AgentAction {
    /// Submit a new order
    SubmitOrder {
        /// Client order ID (agent-assigned)
        client_order_id: u64,
        /// Side (buy/sell)
        side: Side,
        /// Order type
        order_type: OrderType,
        /// Price (ignored for market orders)
        price: Price,
        /// Quantity
        quantity: Quantity,
        /// Time in force
        time_in_force: TimeInForce,
    },

    /// Cancel an existing order
    CancelOrder {
        /// Order ID to cancel
        order_id: u64,
    },

    /// Cancel all orders
    CancelAll,

    /// Modify an existing order (cancel + replace)
    ModifyOrder {
        /// Order ID to modify
        order_id: u64,
        /// New price
        new_price: Price,
        /// New quantity
        new_quantity: Quantity,
    },

    /// No action this tick
    NoOp,
}

impl AgentAction {
    /// Create a limit buy order
    pub fn limit_buy(client_order_id: u64, price: Price, quantity: Quantity) -> Self {
        Self::SubmitOrder {
            client_order_id,
            side: Side::Buy,
            order_type: OrderType::Limit,
            price,
            quantity,
            time_in_force: TimeInForce::Gtc,
        }
    }

    /// Create a limit sell order
    pub fn limit_sell(client_order_id: u64, price: Price, quantity: Quantity) -> Self {
        Self::SubmitOrder {
            client_order_id,
            side: Side::Sell,
            order_type: OrderType::Limit,
            price,
            quantity,
            time_in_force: TimeInForce::Gtc,
        }
    }

    /// Create a post-only buy order
    pub fn post_only_buy(client_order_id: u64, price: Price, quantity: Quantity) -> Self {
        Self::SubmitOrder {
            client_order_id,
            side: Side::Buy,
            order_type: OrderType::PostOnly,
            price,
            quantity,
            time_in_force: TimeInForce::Gtc,
        }
    }

    /// Create a post-only sell order
    pub fn post_only_sell(client_order_id: u64, price: Price, quantity: Quantity) -> Self {
        Self::SubmitOrder {
            client_order_id,
            side: Side::Sell,
            order_type: OrderType::PostOnly,
            price,
            quantity,
            time_in_force: TimeInForce::Gtc,
        }
    }

    /// Create a market buy order
    pub fn market_buy(client_order_id: u64, quantity: Quantity) -> Self {
        Self::SubmitOrder {
            client_order_id,
            side: Side::Buy,
            order_type: OrderType::Market,
            price: Price::from_raw(0), // Ignored for market orders
            quantity,
            time_in_force: TimeInForce::Ioc,
        }
    }

    /// Create a market sell order
    pub fn market_sell(client_order_id: u64, quantity: Quantity) -> Self {
        Self::SubmitOrder {
            client_order_id,
            side: Side::Sell,
            order_type: OrderType::Market,
            price: Price::from_raw(0), // Ignored for market orders
            quantity,
            time_in_force: TimeInForce::Ioc,
        }
    }
}
