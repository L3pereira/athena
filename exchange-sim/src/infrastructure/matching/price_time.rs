/// Price-time priority matching (FIFO)
///
/// This is the default matching algorithm used by most exchanges.
/// The OrderBook entity already implements this natively.
/// This struct is a placeholder for the strategy pattern if other
/// matching algorithms are needed (e.g., pro-rata for CME).
pub struct PriceTimeMatcher;

impl PriceTimeMatcher {
    pub fn new() -> Self {
        PriceTimeMatcher
    }
}

impl Default for PriceTimeMatcher {
    fn default() -> Self {
        Self::new()
    }
}
