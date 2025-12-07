use crate::domain::value_objects::{
    OrderId, OrderType, Price, Quantity, Side, Symbol, TimeInForce, Timestamp,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
    PendingCancel,
}

impl OrderStatus {
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled
                | OrderStatus::Canceled
                | OrderStatus::Rejected
                | OrderStatus::Expired
        )
    }

    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::New | OrderStatus::PartiallyFilled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub client_order_id: Option<String>,
    pub symbol: Symbol,
    pub side: Side,
    pub order_type: OrderType,
    pub quantity: Quantity,
    pub filled_quantity: Quantity,
    pub price: Option<Price>,
    pub stop_price: Option<Price>,
    pub time_in_force: TimeInForce,
    pub status: OrderStatus,
    pub expire_time: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Order {
    pub fn new_limit(
        symbol: Symbol,
        side: Side,
        quantity: Quantity,
        price: Price,
        time_in_force: TimeInForce,
    ) -> Self {
        let now = Utc::now();
        Order {
            id: OrderId::new_v4(),
            client_order_id: None,
            symbol,
            side,
            order_type: OrderType::Limit,
            quantity,
            filled_quantity: Quantity::ZERO,
            price: Some(price),
            stop_price: None,
            time_in_force,
            status: OrderStatus::New,
            expire_time: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_market(symbol: Symbol, side: Side, quantity: Quantity) -> Self {
        let now = Utc::now();
        Order {
            id: OrderId::new_v4(),
            client_order_id: None,
            symbol,
            side,
            order_type: OrderType::Market,
            quantity,
            filled_quantity: Quantity::ZERO,
            price: None,
            stop_price: None,
            time_in_force: TimeInForce::Ioc,
            status: OrderStatus::New,
            expire_time: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_client_order_id(mut self, client_order_id: impl Into<String>) -> Self {
        self.client_order_id = Some(client_order_id.into());
        self
    }

    pub fn with_expire_time(mut self, expire_time: Timestamp) -> Self {
        self.expire_time = Some(expire_time);
        self
    }

    pub fn remaining_quantity(&self) -> Quantity {
        self.quantity.saturating_sub(self.filled_quantity)
    }

    pub fn is_filled(&self) -> bool {
        self.filled_quantity >= self.quantity
    }

    pub fn fill(&mut self, quantity: Quantity, now: Timestamp) {
        self.filled_quantity = self.filled_quantity + quantity;
        self.updated_at = now;

        if self.is_filled() {
            self.status = OrderStatus::Filled;
        } else if self.filled_quantity > Quantity::ZERO {
            self.status = OrderStatus::PartiallyFilled;
        }
    }

    pub fn cancel(&mut self, now: Timestamp) {
        if self.status.is_active() {
            self.status = OrderStatus::Canceled;
            self.updated_at = now;
        }
    }

    pub fn expire(&mut self, now: Timestamp) {
        if self.status.is_active() {
            self.status = OrderStatus::Expired;
            self.updated_at = now;
        }
    }

    pub fn reject(&mut self, now: Timestamp) {
        self.status = OrderStatus::Rejected;
        self.updated_at = now;
    }

    pub fn is_marketable(&self, best_bid: Option<Price>, best_ask: Option<Price>) -> bool {
        match self.order_type {
            OrderType::Market => true,
            OrderType::Limit | OrderType::LimitMaker => {
                let Some(order_price) = self.price else {
                    return false;
                };
                match self.side {
                    Side::Buy => best_ask.is_some_and(|ask| order_price >= ask),
                    Side::Sell => best_bid.is_some_and(|bid| order_price <= bid),
                }
            }
            _ => false, // Conditional orders need to be triggered first
        }
    }

    pub fn is_expired(&self, now: Timestamp) -> bool {
        self.time_in_force.is_expired(self.expire_time, now)
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.quantity.is_zero() {
            return Err("Quantity must be greater than zero");
        }

        if self.order_type.requires_price() && self.price.is_none() {
            return Err("Price required for this order type");
        }

        if self.order_type.requires_stop_price() && self.stop_price.is_none() {
            return Err("Stop price required for this order type");
        }

        if self.time_in_force == TimeInForce::Gtd && self.expire_time.is_none() {
            return Err("Expire time required for GTD orders");
        }

        Ok(())
    }
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Order {}

impl std::hash::Hash for Order {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
