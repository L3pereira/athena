use athena_core::{Order, Price, Quantity, Trade};

use crate::error::MatchingResult;

/// Port for order matching algorithms
///
/// Different implementations support various matching strategies:
/// - Price-Time Priority (FIFO)
/// - Pro-Rata allocation
/// - etc.
pub trait MatchingAlgorithm: Send {
    /// Check if two orders can match
    fn can_match(&self, buy_order: &Order, sell_order: &Order) -> bool;

    /// Match two orders and return the resulting trade and remaining quantities
    ///
    /// Returns: (trade, buy_remaining_qty, sell_remaining_qty)
    fn match_orders(
        &mut self,
        buy_order: &Order,
        sell_order: &Order,
    ) -> MatchingResult<(Trade, Quantity, Quantity)>;

    /// Get the last traded price for a symbol/instrument
    fn last_price(&self, symbol: &str) -> Option<Price>;

    /// Update the last traded price for a symbol/instrument
    fn update_last_price(&mut self, symbol: &str, price: Price);

    /// Validate if an order can be filled according to time-in-force rules
    fn validate_time_in_force(&self, order: &Order, fill_qty: Quantity) -> MatchingResult<bool>;

    /// Get the name of the algorithm
    fn name(&self) -> &str;
}
