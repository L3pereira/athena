use std::collections::HashMap;

use athena_core::{Order, Price, Quantity, Side, TimeInForce, Trade};
use athena_ports::{MatchingAlgorithm, MatchingError, MatchingResult};
use rust_decimal::Decimal;

/// Standard price-time priority matching engine (FIFO)
///
/// Orders are matched based on:
/// 1. Best price (highest bid, lowest ask)
/// 2. Time priority (first in, first out at same price)
pub struct PriceTimeMatchingEngine {
    last_prices: HashMap<String, Decimal>,
}

impl PriceTimeMatchingEngine {
    pub fn new() -> Self {
        Self {
            last_prices: HashMap::new(),
        }
    }
}

impl Default for PriceTimeMatchingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingAlgorithm for PriceTimeMatchingEngine {
    fn name(&self) -> &str {
        "Price-Time Priority"
    }

    fn last_price(&self, symbol: &str) -> Option<Price> {
        self.last_prices.get(symbol).copied()
    }

    fn update_last_price(&mut self, symbol: &str, price: Price) {
        self.last_prices.insert(symbol.to_string(), price);
    }

    fn can_match(&self, buy_order: &Order, sell_order: &Order) -> bool {
        // Must be same instrument
        if buy_order.instrument_id != sell_order.instrument_id {
            return false;
        }

        // Must be opposite sides
        if buy_order.side != Side::Buy || sell_order.side != Side::Sell {
            return false;
        }

        // Check price conditions
        match (buy_order.price, sell_order.price) {
            // Both are limit orders - price must cross
            (Some(buy_price), Some(sell_price)) => buy_price >= sell_price,
            // At least one is a market order - can match
            _ => true,
        }
    }

    fn match_orders(
        &mut self,
        buy_order: &Order,
        sell_order: &Order,
    ) -> MatchingResult<(Trade, Quantity, Quantity)> {
        if !self.can_match(buy_order, sell_order) {
            return Err(MatchingError::CannotMatch(
                "Price or symbol mismatch".to_string(),
            ));
        }

        // Calculate match quantity
        let buy_remaining = buy_order.quantity - buy_order.filled_quantity;
        let sell_remaining = sell_order.quantity - sell_order.filled_quantity;
        let match_qty = buy_remaining.min(sell_remaining);

        if match_qty == Decimal::ZERO {
            return Err(MatchingError::NoQuantity);
        }

        // Determine match price according to price-time priority
        // Resting order (older) sets the price
        let match_price = match (buy_order.price, sell_order.price) {
            (Some(buy_price), Some(sell_price)) => {
                if buy_order.created_at < sell_order.created_at {
                    buy_price
                } else {
                    sell_price
                }
            }
            (None, Some(sell_price)) => sell_price,
            (Some(buy_price), None) => buy_price,
            (None, None) => self
                .last_price(buy_order.symbol())
                .ok_or(MatchingError::NoPriceAvailable)?,
        };

        // Create trade record
        let trade = Trade::new(
            buy_order.instrument_id.clone(),
            buy_order.id,
            sell_order.id,
            match_price,
            match_qty,
        );

        // Update last price
        self.update_last_price(trade.symbol(), match_price);

        // Calculate remaining quantities
        let buy_remaining_after = buy_remaining - match_qty;
        let sell_remaining_after = sell_remaining - match_qty;

        Ok((trade, buy_remaining_after, sell_remaining_after))
    }

    fn validate_time_in_force(&self, order: &Order, fill_qty: Quantity) -> MatchingResult<bool> {
        match order.time_in_force {
            TimeInForce::FOK => {
                if fill_qty < order.quantity {
                    return Err(MatchingError::TimeInForceViolation(
                        "FOK order cannot be partially filled".to_string(),
                    ));
                }
                Ok(true)
            }
            TimeInForce::IOC => Ok(true),
            _ => Ok(true),
        }
    }
}
