use crate::value_objects::{Price, Quantity};
use serde::{Deserialize, Serialize};

/// Represents a single price level in the order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price: Price,
    pub quantity: Quantity,
}

impl PriceLevel {
    pub fn new(price: Price, quantity: Quantity) -> Self {
        PriceLevel { price, quantity }
    }

    pub fn is_empty(&self) -> bool {
        self.quantity.is_zero()
    }
}

impl From<(Price, Quantity)> for PriceLevel {
    fn from((price, quantity): (Price, Quantity)) -> Self {
        PriceLevel { price, quantity }
    }
}

impl PartialEq for PriceLevel {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl Eq for PriceLevel {}

impl std::hash::Hash for PriceLevel {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.price.raw().hash(state);
    }
}
