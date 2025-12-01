use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::instruments::InstrumentId;

/// Fee tier based on trading volume or VIP level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeeTier {
    /// Default tier for new users
    Standard,
    /// VIP tier with reduced fees
    Vip1,
    Vip2,
    Vip3,
    /// Market maker tier (negative maker fees = rebates)
    MarketMaker,
}

impl Default for FeeTier {
    fn default() -> Self {
        Self::Standard
    }
}

/// Fee structure for an instrument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeeSchedule {
    /// Instrument this schedule applies to (None = default for all)
    pub instrument_id: Option<InstrumentId>,

    /// Fee for maker orders (adds liquidity) - typically lower
    /// Can be negative for rebates
    pub maker_fee: Decimal,

    /// Fee for taker orders (removes liquidity) - typically higher
    pub taker_fee: Decimal,

    /// Minimum fee per trade (in quote currency)
    pub min_fee: Decimal,

    /// Maximum fee per trade (in quote currency, None = no max)
    pub max_fee: Option<Decimal>,
}

impl FeeSchedule {
    /// Create a new fee schedule
    pub fn new(maker_fee: Decimal, taker_fee: Decimal) -> Self {
        Self {
            instrument_id: None,
            maker_fee,
            taker_fee,
            min_fee: Decimal::ZERO,
            max_fee: None,
        }
    }

    /// Create fee schedule for a specific instrument
    pub fn for_instrument(
        instrument_id: InstrumentId,
        maker_fee: Decimal,
        taker_fee: Decimal,
    ) -> Self {
        Self {
            instrument_id: Some(instrument_id),
            maker_fee,
            taker_fee,
            min_fee: Decimal::ZERO,
            max_fee: None,
        }
    }

    /// Set minimum fee
    pub fn with_min_fee(mut self, min_fee: Decimal) -> Self {
        self.min_fee = min_fee;
        self
    }

    /// Set maximum fee
    pub fn with_max_fee(mut self, max_fee: Decimal) -> Self {
        self.max_fee = Some(max_fee);
        self
    }

    /// Calculate fee for a trade
    pub fn calculate_fee(&self, notional: Decimal, is_maker: bool) -> Decimal {
        let rate = if is_maker {
            self.maker_fee
        } else {
            self.taker_fee
        };
        let fee = notional * rate;

        // Apply min/max constraints only for positive fees (not rebates)
        // Negative fees (rebates) are allowed and bypass min/max
        if fee >= Decimal::ZERO {
            let fee = fee.max(self.min_fee);
            match self.max_fee {
                Some(max) => fee.min(max),
                None => fee,
            }
        } else {
            // Rebate - return as-is
            fee
        }
    }
}

impl Default for FeeSchedule {
    fn default() -> Self {
        Self {
            instrument_id: None,
            maker_fee: Decimal::new(1, 4), // 0.01% maker
            taker_fee: Decimal::new(5, 4), // 0.05% taker
            min_fee: Decimal::ZERO,
            max_fee: None,
        }
    }
}

/// Fee configuration manager for multiple instruments and tiers
#[derive(Debug, Clone, Default)]
pub struct FeeConfig {
    /// Default fee schedule for all instruments
    default_schedule: FeeSchedule,

    /// Per-instrument fee schedules
    instrument_schedules: std::collections::HashMap<InstrumentId, FeeSchedule>,

    /// Per-tier fee multipliers (applied to base fees)
    tier_multipliers: std::collections::HashMap<FeeTier, Decimal>,
}

impl FeeConfig {
    /// Create a new fee configuration with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom default fees
    pub fn with_default_fees(maker_fee: Decimal, taker_fee: Decimal) -> Self {
        Self {
            default_schedule: FeeSchedule::new(maker_fee, taker_fee),
            ..Default::default()
        }
    }

    /// Set the default fee schedule
    pub fn set_default_schedule(&mut self, schedule: FeeSchedule) {
        self.default_schedule = schedule;
    }

    /// Set fee schedule for a specific instrument
    pub fn set_instrument_schedule(&mut self, instrument_id: InstrumentId, schedule: FeeSchedule) {
        self.instrument_schedules.insert(instrument_id, schedule);
    }

    /// Set fee multiplier for a tier
    pub fn set_tier_multiplier(&mut self, tier: FeeTier, multiplier: Decimal) {
        self.tier_multipliers.insert(tier, multiplier);
    }

    /// Get the fee schedule for an instrument
    pub fn get_schedule(&self, instrument_id: &InstrumentId) -> &FeeSchedule {
        self.instrument_schedules
            .get(instrument_id)
            .unwrap_or(&self.default_schedule)
    }

    /// Calculate fee for a trade
    pub fn calculate_fee(
        &self,
        instrument_id: &InstrumentId,
        notional: Decimal,
        is_maker: bool,
        tier: FeeTier,
    ) -> Decimal {
        let schedule = self.get_schedule(instrument_id);
        let base_fee = schedule.calculate_fee(notional, is_maker);

        // Apply tier multiplier
        let multiplier = self
            .tier_multipliers
            .get(&tier)
            .copied()
            .unwrap_or(Decimal::ONE);
        base_fee * multiplier
    }

    /// Get maker fee rate for an instrument
    pub fn maker_fee_rate(&self, instrument_id: &InstrumentId) -> Decimal {
        self.get_schedule(instrument_id).maker_fee
    }

    /// Get taker fee rate for an instrument
    pub fn taker_fee_rate(&self, instrument_id: &InstrumentId) -> Decimal {
        self.get_schedule(instrument_id).taker_fee
    }
}

/// Fees charged on a specific trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeFees {
    /// Fee charged to the buyer
    pub buyer_fee: Decimal,

    /// Fee charged to the seller
    pub seller_fee: Decimal,

    /// Whether buyer was maker
    pub buyer_is_maker: bool,

    /// Fee currency (usually quote currency)
    pub fee_currency: String,
}

impl TradeFees {
    /// Create new trade fees
    pub fn new(
        buyer_fee: Decimal,
        seller_fee: Decimal,
        buyer_is_maker: bool,
        fee_currency: String,
    ) -> Self {
        Self {
            buyer_fee,
            seller_fee,
            buyer_is_maker,
            fee_currency,
        }
    }

    /// Total fees collected
    pub fn total(&self) -> Decimal {
        self.buyer_fee + self.seller_fee
    }
}

impl Default for TradeFees {
    fn default() -> Self {
        Self {
            buyer_fee: Decimal::ZERO,
            seller_fee: Decimal::ZERO,
            buyer_is_maker: false,
            fee_currency: "USD".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_fee_schedule_default() {
        let schedule = FeeSchedule::default();
        assert_eq!(schedule.maker_fee, dec!(0.0001));
        assert_eq!(schedule.taker_fee, dec!(0.0005));
    }

    #[test]
    fn test_fee_calculation() {
        let schedule = FeeSchedule::new(dec!(0.001), dec!(0.002)); // 0.1% maker, 0.2% taker

        // Maker fee: 10000 * 0.001 = 10
        assert_eq!(schedule.calculate_fee(dec!(10000), true), dec!(10));

        // Taker fee: 10000 * 0.002 = 20
        assert_eq!(schedule.calculate_fee(dec!(10000), false), dec!(20));
    }

    #[test]
    fn test_fee_min_max() {
        let schedule = FeeSchedule::new(dec!(0.001), dec!(0.002))
            .with_min_fee(dec!(1))
            .with_max_fee(dec!(100));

        // Below min: should be min
        assert_eq!(schedule.calculate_fee(dec!(100), true), dec!(1));

        // Above max: should be max
        assert_eq!(schedule.calculate_fee(dec!(100000), false), dec!(100));

        // In range: should be calculated
        assert_eq!(schedule.calculate_fee(dec!(10000), true), dec!(10));
    }

    #[test]
    fn test_fee_config_per_instrument() {
        let mut config = FeeConfig::with_default_fees(dec!(0.001), dec!(0.002));

        // Set custom fees for BTC
        config.set_instrument_schedule(
            InstrumentId::new("BTC/USD"),
            FeeSchedule::new(dec!(0.0005), dec!(0.001)),
        );

        // BTC should use custom fees
        let btc_fee = config.calculate_fee(
            &InstrumentId::new("BTC/USD"),
            dec!(50000),
            false,
            FeeTier::Standard,
        );
        assert_eq!(btc_fee, dec!(50)); // 50000 * 0.001

        // ETH should use default fees
        let eth_fee = config.calculate_fee(
            &InstrumentId::new("ETH/USD"),
            dec!(3000),
            false,
            FeeTier::Standard,
        );
        assert_eq!(eth_fee, dec!(6)); // 3000 * 0.002
    }

    #[test]
    fn test_fee_tier_multiplier() {
        let mut config = FeeConfig::with_default_fees(dec!(0.001), dec!(0.002));
        config.set_tier_multiplier(FeeTier::Vip1, dec!(0.8)); // 20% discount

        let standard_fee = config.calculate_fee(
            &InstrumentId::new("BTC/USD"),
            dec!(10000),
            false,
            FeeTier::Standard,
        );
        let vip_fee = config.calculate_fee(
            &InstrumentId::new("BTC/USD"),
            dec!(10000),
            false,
            FeeTier::Vip1,
        );

        assert_eq!(standard_fee, dec!(20)); // 10000 * 0.002
        assert_eq!(vip_fee, dec!(16)); // 10000 * 0.002 * 0.8
    }

    #[test]
    fn test_negative_maker_fee_rebate() {
        let schedule = FeeSchedule::new(dec!(-0.0001), dec!(0.001)); // -0.01% rebate for makers

        // Maker gets rebate (negative fee)
        assert_eq!(schedule.calculate_fee(dec!(10000), true), dec!(-1));

        // Taker pays fee
        assert_eq!(schedule.calculate_fee(dec!(10000), false), dec!(10));
    }
}
