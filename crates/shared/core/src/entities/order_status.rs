use serde::{Deserialize, Serialize};

/// Order lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Order has been created but not yet processed
    New,
    /// Order has been partially filled
    PartiallyFilled,
    /// Order has been completely filled
    Filled,
    /// Order has been canceled by the user
    Canceled,
    /// Order was rejected by the exchange
    Rejected,
    /// Order has expired (GTD/DAY)
    Expired,
}

impl OrderStatus {
    /// Returns true if the order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled
                | OrderStatus::Canceled
                | OrderStatus::Rejected
                | OrderStatus::Expired
        )
    }

    /// Returns true if the order is still active
    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::New | OrderStatus::PartiallyFilled)
    }
}
