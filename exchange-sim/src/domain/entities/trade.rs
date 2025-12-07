use crate::domain::value_objects::{OrderId, Price, Quantity, Side, Symbol, Timestamp, TradeId};
use chrono::Utc;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: TradeId,
    pub symbol: Symbol,
    pub price: Price,
    pub quantity: Quantity,
    pub buyer_order_id: OrderId,
    pub seller_order_id: OrderId,
    /// The side of the order that was the aggressor (taker)
    pub taker_side: Side,
    pub timestamp: Timestamp,
    /// Buyer was the maker (their order was resting in the book)
    pub buyer_is_maker: bool,
    /// Fee paid by maker (negative = rebate received)
    pub maker_fee: Decimal,
    /// Fee paid by taker
    pub taker_fee: Decimal,
    /// Asset in which fees are denominated (typically quote asset)
    pub fee_asset: String,
}

impl Trade {
    pub fn new(
        symbol: Symbol,
        price: Price,
        quantity: Quantity,
        buyer_order_id: OrderId,
        seller_order_id: OrderId,
        taker_side: Side,
    ) -> Self {
        Trade {
            id: TradeId::new_v4(),
            symbol,
            price,
            quantity,
            buyer_order_id,
            seller_order_id,
            taker_side,
            timestamp: Utc::now(),
            buyer_is_maker: taker_side == Side::Sell,
            maker_fee: Decimal::ZERO,
            taker_fee: Decimal::ZERO,
            fee_asset: String::new(),
        }
    }

    pub fn with_timestamp(mut self, timestamp: Timestamp) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn with_buyer_is_maker(mut self, buyer_is_maker: bool) -> Self {
        self.buyer_is_maker = buyer_is_maker;
        self
    }

    /// Set fees for this trade
    pub fn with_fees(
        mut self,
        maker_fee: Decimal,
        taker_fee: Decimal,
        fee_asset: impl Into<String>,
    ) -> Self {
        self.maker_fee = maker_fee;
        self.taker_fee = taker_fee;
        self.fee_asset = fee_asset.into();
        self
    }

    /// Get the fee for a specific side (buyer or seller)
    pub fn fee_for_buyer(&self) -> Decimal {
        if self.buyer_is_maker {
            self.maker_fee
        } else {
            self.taker_fee
        }
    }

    /// Get the fee for the seller
    pub fn fee_for_seller(&self) -> Decimal {
        if self.buyer_is_maker {
            self.taker_fee
        } else {
            self.maker_fee
        }
    }

    /// Notional value of the trade (price * quantity)
    pub fn notional(&self) -> rust_decimal::Decimal {
        self.price.inner() * self.quantity.inner()
    }

    /// Returns the maker order ID
    pub fn maker_order_id(&self) -> OrderId {
        if self.buyer_is_maker {
            self.buyer_order_id
        } else {
            self.seller_order_id
        }
    }

    /// Returns the taker order ID
    pub fn taker_order_id(&self) -> OrderId {
        if self.buyer_is_maker {
            self.seller_order_id
        } else {
            self.buyer_order_id
        }
    }
}

impl PartialEq for Trade {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Trade {}

impl std::hash::Hash for Trade {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
