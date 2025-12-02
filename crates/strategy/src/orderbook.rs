//! Local Order Book Replica
//!
//! Each strategy maintains its own copy of the order book, updated from
//! deltas received via channels. This eliminates contention - no locks
//! needed when reading the book.

use athena_gateway::messages::market_data::OrderBookUpdate;
use rust_decimal::Decimal;
use std::collections::BTreeMap;

/// Local order book replica for a single instrument
///
/// Uses BTreeMap for price levels to maintain sorted order.
/// Bids are stored in descending order (highest first).
/// Asks are stored in ascending order (lowest first).
#[derive(Debug, Clone)]
pub struct LocalOrderBook {
    instrument_id: String,
    /// Bid levels: price -> quantity (sorted descending)
    bids: BTreeMap<Decimal, Decimal>,
    /// Ask levels: price -> quantity (sorted ascending)
    asks: BTreeMap<Decimal, Decimal>,
    /// Last sequence number processed
    sequence: u64,
    /// Timestamp of last update (nanoseconds)
    last_update_ns: i64,
}

impl LocalOrderBook {
    /// Create a new empty order book
    pub fn new(instrument_id: impl Into<String>) -> Self {
        Self {
            instrument_id: instrument_id.into(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            sequence: 0,
            last_update_ns: 0,
        }
    }

    /// Apply an update (snapshot or delta) to the order book
    pub fn apply_update(&mut self, update: &OrderBookUpdate) {
        match update {
            OrderBookUpdate::Snapshot {
                instrument_id,
                bids,
                asks,
                sequence,
                timestamp_ns,
            } => {
                if instrument_id != &self.instrument_id {
                    return;
                }
                // Clear and rebuild from snapshot
                self.bids.clear();
                self.asks.clear();
                for level in bids {
                    if !level.quantity.is_zero() {
                        self.bids.insert(level.price, level.quantity);
                    }
                }
                for level in asks {
                    if !level.quantity.is_zero() {
                        self.asks.insert(level.price, level.quantity);
                    }
                }
                self.sequence = *sequence;
                self.last_update_ns = *timestamp_ns;
            }
            OrderBookUpdate::Delta {
                instrument_id,
                bids,
                asks,
                sequence,
                timestamp_ns,
            } => {
                if instrument_id != &self.instrument_id {
                    return;
                }
                // Apply delta updates
                for level in bids {
                    if level.quantity.is_zero() {
                        self.bids.remove(&level.price);
                    } else {
                        self.bids.insert(level.price, level.quantity);
                    }
                }
                for level in asks {
                    if level.quantity.is_zero() {
                        self.asks.remove(&level.price);
                    } else {
                        self.asks.insert(level.price, level.quantity);
                    }
                }
                self.sequence = *sequence;
                self.last_update_ns = *timestamp_ns;
            }
        }
    }

    /// Get the instrument ID
    pub fn instrument_id(&self) -> &str {
        &self.instrument_id
    }

    /// Get current sequence number
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Get timestamp of last update
    pub fn last_update_ns(&self) -> i64 {
        self.last_update_ns
    }

    // === Price Queries ===

    /// Get best bid price and quantity
    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next_back().map(|(p, q)| (*p, *q))
    }

    /// Get best ask price and quantity
    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(p, q)| (*p, *q))
    }

    /// Get mid price (average of best bid and ask)
    pub fn mid_price(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some((bid + ask) / Decimal::TWO),
            _ => None,
        }
    }

    /// Get spread (ask - bid)
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some(ask - bid),
            _ => None,
        }
    }

    /// Get spread as percentage of mid price
    pub fn spread_bps(&self) -> Option<Decimal> {
        match (self.spread(), self.mid_price()) {
            (Some(spread), Some(mid)) if !mid.is_zero() => {
                Some(spread / mid * Decimal::from(10000))
            }
            _ => None,
        }
    }

    // === Level Queries ===

    /// Get top N bid levels (highest prices first)
    pub fn top_bids(&self, n: usize) -> Vec<(Decimal, Decimal)> {
        self.bids
            .iter()
            .rev()
            .take(n)
            .map(|(p, q)| (*p, *q))
            .collect()
    }

    /// Get top N ask levels (lowest prices first)
    pub fn top_asks(&self, n: usize) -> Vec<(Decimal, Decimal)> {
        self.asks.iter().take(n).map(|(p, q)| (*p, *q)).collect()
    }

    /// Get quantity at a specific bid price
    pub fn bid_qty_at(&self, price: Decimal) -> Decimal {
        self.bids.get(&price).copied().unwrap_or(Decimal::ZERO)
    }

    /// Get quantity at a specific ask price
    pub fn ask_qty_at(&self, price: Decimal) -> Decimal {
        self.asks.get(&price).copied().unwrap_or(Decimal::ZERO)
    }

    /// Get total bid quantity up to N levels
    pub fn total_bid_qty(&self, levels: usize) -> Decimal {
        self.bids.iter().rev().take(levels).map(|(_, q)| q).sum()
    }

    /// Get total ask quantity up to N levels
    pub fn total_ask_qty(&self, levels: usize) -> Decimal {
        self.asks.iter().take(levels).map(|(_, q)| q).sum()
    }

    /// Check if book is empty
    pub fn is_empty(&self) -> bool {
        self.bids.is_empty() && self.asks.is_empty()
    }

    /// Check if book has both sides (can quote)
    pub fn is_two_sided(&self) -> bool {
        !self.bids.is_empty() && !self.asks.is_empty()
    }

    // === Market Making Helpers ===

    /// Calculate VWAP for buying `qty` (sweeping asks)
    pub fn vwap_buy(&self, qty: Decimal) -> Option<Decimal> {
        let mut remaining = qty;
        let mut total_cost = Decimal::ZERO;

        for (price, level_qty) in self.asks.iter() {
            if remaining.is_zero() {
                break;
            }
            let fill_qty = remaining.min(*level_qty);
            total_cost += fill_qty * price;
            remaining -= fill_qty;
        }

        if remaining.is_zero() {
            Some(total_cost / qty)
        } else {
            None // Not enough liquidity
        }
    }

    /// Calculate VWAP for selling `qty` (sweeping bids)
    pub fn vwap_sell(&self, qty: Decimal) -> Option<Decimal> {
        let mut remaining = qty;
        let mut total_proceeds = Decimal::ZERO;

        for (price, level_qty) in self.bids.iter().rev() {
            if remaining.is_zero() {
                break;
            }
            let fill_qty = remaining.min(*level_qty);
            total_proceeds += fill_qty * price;
            remaining -= fill_qty;
        }

        if remaining.is_zero() {
            Some(total_proceeds / qty)
        } else {
            None // Not enough liquidity
        }
    }

    /// Calculate imbalance ratio: (bid_qty - ask_qty) / (bid_qty + ask_qty)
    /// Returns value between -1 (all asks) and +1 (all bids)
    pub fn imbalance(&self, levels: usize) -> Option<Decimal> {
        let bid_qty = self.total_bid_qty(levels);
        let ask_qty = self.total_ask_qty(levels);
        let total = bid_qty + ask_qty;

        if total.is_zero() {
            None
        } else {
            Some((bid_qty - ask_qty) / total)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use athena_gateway::messages::market_data::BookLevel;
    use rust_decimal_macros::dec;

    fn sample_snapshot() -> OrderBookUpdate {
        OrderBookUpdate::Snapshot {
            instrument_id: "BTC-USD".to_string(),
            bids: vec![
                BookLevel::new(dec!(50000), dec!(1.0)),
                BookLevel::new(dec!(49900), dec!(2.0)),
                BookLevel::new(dec!(49800), dec!(3.0)),
            ],
            asks: vec![
                BookLevel::new(dec!(50100), dec!(1.5)),
                BookLevel::new(dec!(50200), dec!(2.5)),
                BookLevel::new(dec!(50300), dec!(3.5)),
            ],
            sequence: 1,
            timestamp_ns: 1000000,
        }
    }

    #[test]
    fn test_apply_snapshot() {
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&sample_snapshot());

        assert_eq!(book.best_bid(), Some((dec!(50000), dec!(1.0))));
        assert_eq!(book.best_ask(), Some((dec!(50100), dec!(1.5))));
        assert_eq!(book.sequence(), 1);
    }

    #[test]
    fn test_mid_price_and_spread() {
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&sample_snapshot());

        assert_eq!(book.mid_price(), Some(dec!(50050)));
        assert_eq!(book.spread(), Some(dec!(100)));
    }

    #[test]
    fn test_apply_delta() {
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&sample_snapshot());

        // Apply delta: update bid, remove ask level
        let delta = OrderBookUpdate::Delta {
            instrument_id: "BTC-USD".to_string(),
            bids: vec![BookLevel::new(dec!(50000), dec!(5.0))], // Update quantity
            asks: vec![BookLevel::new(dec!(50100), dec!(0))],   // Remove level
            sequence: 2,
            timestamp_ns: 2000000,
        };
        book.apply_update(&delta);

        assert_eq!(book.best_bid(), Some((dec!(50000), dec!(5.0))));
        assert_eq!(book.best_ask(), Some((dec!(50200), dec!(2.5)))); // Next level
        assert_eq!(book.sequence(), 2);
    }

    #[test]
    fn test_top_levels() {
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&sample_snapshot());

        let top_bids = book.top_bids(2);
        assert_eq!(top_bids.len(), 2);
        assert_eq!(top_bids[0], (dec!(50000), dec!(1.0)));
        assert_eq!(top_bids[1], (dec!(49900), dec!(2.0)));

        let top_asks = book.top_asks(2);
        assert_eq!(top_asks.len(), 2);
        assert_eq!(top_asks[0], (dec!(50100), dec!(1.5)));
        assert_eq!(top_asks[1], (dec!(50200), dec!(2.5)));
    }

    #[test]
    fn test_vwap() {
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&sample_snapshot());

        // VWAP buy 1.0: should get all at 50100
        let vwap = book.vwap_buy(dec!(1.0)).unwrap();
        assert_eq!(vwap, dec!(50100));

        // VWAP buy 2.0: 1.5 @ 50100 + 0.5 @ 50200
        let vwap = book.vwap_buy(dec!(2.0)).unwrap();
        let expected = (dec!(1.5) * dec!(50100) + dec!(0.5) * dec!(50200)) / dec!(2.0);
        assert_eq!(vwap, expected);
    }

    #[test]
    fn test_imbalance() {
        let mut book = LocalOrderBook::new("BTC-USD");
        book.apply_update(&sample_snapshot());

        // Bids: 1 + 2 + 3 = 6
        // Asks: 1.5 + 2.5 + 3.5 = 7.5
        // Imbalance = (6 - 7.5) / (6 + 7.5) = -1.5 / 13.5
        let imb = book.imbalance(3).unwrap();
        let expected = (dec!(6) - dec!(7.5)) / (dec!(6) + dec!(7.5));
        assert_eq!(imb, expected);
    }
}
