use crate::domain::{Order, Price, Quantity, Side, Timestamp, Trade};
use std::collections::VecDeque;

/// Result of a matching operation
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Trades generated from the match
    pub trades: Vec<Trade>,
    /// Remaining quantity of the aggressor order (if any)
    pub remaining_qty: Quantity,
    /// Orders that were fully filled and should be removed
    pub filled_order_ids: Vec<uuid::Uuid>,
}

/// Trait for order matching algorithms
///
/// Different markets use different matching algorithms:
/// - Price-Time Priority (FIFO): Most common in equity markets
/// - Pro-Rata: Common in options/futures markets (CME)
/// - Price-Size-Time: Gives priority to larger orders
pub trait MatchingAlgorithm: Send + Sync {
    /// Algorithm name
    fn name(&self) -> &str;

    /// Match an incoming order against resting orders at a price level
    ///
    /// # Arguments
    /// * `aggressor` - The incoming order
    /// * `resting_orders` - Orders at the price level (in arrival order)
    /// * `match_price` - The price at which matching occurs
    /// * `timestamp` - Current time
    ///
    /// # Returns
    /// MatchResult containing trades and updated state
    fn match_at_level(
        &self,
        aggressor: &mut Order,
        resting_orders: &mut VecDeque<Order>,
        match_price: Price,
        timestamp: Timestamp,
    ) -> MatchResult;

    /// Determine match price when aggressor meets resting order
    ///
    /// Default: Price of the resting (passive) order
    fn determine_price(&self, _aggressor: &Order, resting: &Order) -> Price {
        resting.price.expect("Resting order must have price")
    }
}

/// Price-Time Priority (FIFO) Matching
///
/// Orders are filled in the order they arrived. The first order
/// at a price level gets filled first.
///
/// Used by: NYSE, NASDAQ, most equity exchanges
#[derive(Debug, Default)]
pub struct PriceTimeMatcher;

impl PriceTimeMatcher {
    pub fn new() -> Self {
        Self
    }
}

impl MatchingAlgorithm for PriceTimeMatcher {
    fn name(&self) -> &str {
        "Price-Time (FIFO)"
    }

    fn match_at_level(
        &self,
        aggressor: &mut Order,
        resting_orders: &mut VecDeque<Order>,
        match_price: Price,
        timestamp: Timestamp,
    ) -> MatchResult {
        let mut trades = Vec::new();
        let mut filled_ids = Vec::new();

        while aggressor.remaining_quantity() > Quantity::ZERO {
            let Some(resting) = resting_orders.front_mut() else {
                break;
            };

            let fill_qty = aggressor
                .remaining_quantity()
                .min(resting.remaining_quantity());

            if fill_qty.is_zero() {
                break;
            }

            // Create trade
            let (buyer_id, seller_id, buyer_is_maker) = match aggressor.side {
                Side::Buy => (aggressor.id, resting.id, false),
                Side::Sell => (resting.id, aggressor.id, true),
            };

            let trade = Trade::new(
                aggressor.symbol.clone(),
                match_price,
                fill_qty,
                buyer_id,
                seller_id,
                aggressor.side,
            )
            .with_timestamp(timestamp)
            .with_buyer_is_maker(buyer_is_maker);

            trades.push(trade);

            // Update orders
            aggressor.fill(fill_qty, timestamp);
            resting.fill(fill_qty, timestamp);

            // Remove filled resting order
            if resting.is_filled() {
                filled_ids.push(resting.id);
                resting_orders.pop_front();
            }
        }

        MatchResult {
            trades,
            remaining_qty: aggressor.remaining_quantity(),
            filled_order_ids: filled_ids,
        }
    }
}

/// Pro-Rata Matching
///
/// Orders at a price level are filled proportionally based on their size.
/// Larger orders get a larger share of the fill.
///
/// Used by: CME (for certain products), options markets
///
/// Formula: fill_qty = (order_qty / total_qty) * available_qty
#[derive(Debug, Default)]
pub struct ProRataMatcher {
    /// Minimum allocation (prevents dust fills)
    min_allocation: Quantity,
}

impl ProRataMatcher {
    pub fn new() -> Self {
        Self {
            min_allocation: Quantity::ZERO,
        }
    }

    pub fn with_min_allocation(mut self, min: Quantity) -> Self {
        self.min_allocation = min;
        self
    }
}

impl MatchingAlgorithm for ProRataMatcher {
    fn name(&self) -> &str {
        "Pro-Rata"
    }

    fn match_at_level(
        &self,
        aggressor: &mut Order,
        resting_orders: &mut VecDeque<Order>,
        match_price: Price,
        timestamp: Timestamp,
    ) -> MatchResult {
        let mut trades = Vec::new();
        let mut filled_ids = Vec::new();

        if resting_orders.is_empty() {
            return MatchResult {
                trades,
                remaining_qty: aggressor.remaining_quantity(),
                filled_order_ids: filled_ids,
            };
        }

        // Calculate total resting quantity
        let total_resting: Quantity = resting_orders
            .iter()
            .map(|o| o.remaining_quantity())
            .fold(Quantity::ZERO, |a, b| a + b);

        if total_resting.is_zero() {
            return MatchResult {
                trades,
                remaining_qty: aggressor.remaining_quantity(),
                filled_order_ids: filled_ids,
            };
        }

        let aggressor_qty = aggressor.remaining_quantity();
        let available_to_fill = aggressor_qty.min(total_resting);

        // Calculate proportional allocations
        let mut allocations: Vec<(usize, Quantity)> = Vec::new();
        let mut allocated_total = Quantity::ZERO;

        for (idx, order) in resting_orders.iter().enumerate() {
            let order_qty = order.remaining_quantity();
            // Pro-rata formula: (order_qty / total_resting) * available_to_fill
            let ratio = order_qty.inner() / total_resting.inner();
            let allocation = Quantity::from(ratio * available_to_fill.inner());

            // Apply minimum allocation filter
            if allocation >= self.min_allocation {
                allocations.push((idx, allocation));
                allocated_total = allocated_total + allocation;
            }
        }

        // Distribute any rounding remainder to largest orders (FIFO among equals)
        let remainder = available_to_fill.saturating_sub(allocated_total);
        if remainder > Quantity::ZERO && !allocations.is_empty() {
            // Give remainder to first order
            allocations[0].1 = allocations[0].1 + remainder;
        }

        // Execute fills
        for (idx, fill_qty) in allocations {
            if fill_qty.is_zero() {
                continue;
            }

            let resting = &mut resting_orders[idx];

            let (buyer_id, seller_id, buyer_is_maker) = match aggressor.side {
                Side::Buy => (aggressor.id, resting.id, false),
                Side::Sell => (resting.id, aggressor.id, true),
            };

            let trade = Trade::new(
                aggressor.symbol.clone(),
                match_price,
                fill_qty,
                buyer_id,
                seller_id,
                aggressor.side,
            )
            .with_timestamp(timestamp)
            .with_buyer_is_maker(buyer_is_maker);

            trades.push(trade);

            aggressor.fill(fill_qty, timestamp);
            resting.fill(fill_qty, timestamp);

            if resting.is_filled() {
                filled_ids.push(resting.id);
            }
        }

        // Remove filled orders (in reverse to preserve indices)
        resting_orders.retain(|o| !o.is_filled());

        MatchResult {
            trades,
            remaining_qty: aggressor.remaining_quantity(),
            filled_order_ids: filled_ids,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Symbol, TimeInForce};
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn make_order(side: Side, qty: Quantity, price: Price) -> Order {
        Order::new_limit(
            Symbol::new("BTCUSDT").unwrap(),
            side,
            qty,
            price,
            TimeInForce::Gtc,
        )
    }

    #[test]
    fn test_price_time_fifo() {
        let matcher = PriceTimeMatcher::new();
        let now = Utc::now();
        let price = Price::from(dec!(100));

        // Two resting sell orders
        let mut resting = VecDeque::new();
        resting.push_back(make_order(Side::Sell, Quantity::from(dec!(5)), price));
        resting.push_back(make_order(Side::Sell, Quantity::from(dec!(10)), price));

        // Aggressor buy for 8
        let mut aggressor = make_order(Side::Buy, Quantity::from(dec!(8)), price);

        let result = matcher.match_at_level(&mut aggressor, &mut resting, price, now);

        // Should fill first order (5) completely, then 3 from second
        assert_eq!(result.trades.len(), 2);
        assert_eq!(result.trades[0].quantity, Quantity::from(dec!(5)));
        assert_eq!(result.trades[1].quantity, Quantity::from(dec!(3)));
        assert_eq!(result.remaining_qty, Quantity::ZERO);
        assert_eq!(result.filled_order_ids.len(), 1); // First order fully filled
    }

    #[test]
    fn test_pro_rata_allocation() {
        let matcher = ProRataMatcher::new();
        let now = Utc::now();
        let price = Price::from(dec!(100));

        // Two resting orders: 30 and 70 (30% and 70% of total)
        let mut resting = VecDeque::new();
        resting.push_back(make_order(Side::Sell, Quantity::from(dec!(30)), price));
        resting.push_back(make_order(Side::Sell, Quantity::from(dec!(70)), price));

        // Aggressor buy for 10
        let mut aggressor = make_order(Side::Buy, Quantity::from(dec!(10)), price);

        let result = matcher.match_at_level(&mut aggressor, &mut resting, price, now);

        // Pro-rata: 30% of 10 = 3, 70% of 10 = 7
        assert_eq!(result.trades.len(), 2);
        // First trade should be ~3 (30% of 10)
        assert_eq!(result.trades[0].quantity, Quantity::from(dec!(3)));
        // Second trade should be ~7 (70% of 10)
        assert_eq!(result.trades[1].quantity, Quantity::from(dec!(7)));
        assert_eq!(result.remaining_qty, Quantity::ZERO);
    }
}
