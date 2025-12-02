//! Market Events - External information for informed trading
//!
//! Events that strategies can consume to make informed trading decisions.
//! These represent information from external sources like index prices,
//! news sentiment, or volatility signals.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Market events that inform trading decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    /// Fair value update (e.g., from index, external source, model)
    FairValue {
        instrument_id: String,
        price: Decimal,
    },
    /// Volatility change signal
    VolatilityChange { instrument_id: String, vol: Decimal },
    /// News/sentiment signal (-1.0 bearish to 1.0 bullish)
    Sentiment {
        instrument_id: String,
        score: Decimal,
    },
}

impl MarketEvent {
    /// Get the instrument this event relates to
    pub fn instrument_id(&self) -> &str {
        match self {
            MarketEvent::FairValue { instrument_id, .. } => instrument_id,
            MarketEvent::VolatilityChange { instrument_id, .. } => instrument_id,
            MarketEvent::Sentiment { instrument_id, .. } => instrument_id,
        }
    }

    /// Create a fair value event
    pub fn fair_value(instrument_id: impl Into<String>, price: Decimal) -> Self {
        MarketEvent::FairValue {
            instrument_id: instrument_id.into(),
            price,
        }
    }

    /// Create a sentiment event
    pub fn sentiment(instrument_id: impl Into<String>, score: Decimal) -> Self {
        MarketEvent::Sentiment {
            instrument_id: instrument_id.into(),
            score: score.clamp(Decimal::NEGATIVE_ONE, Decimal::ONE),
        }
    }

    /// Create a volatility change event
    pub fn volatility(instrument_id: impl Into<String>, vol: Decimal) -> Self {
        MarketEvent::VolatilityChange {
            instrument_id: instrument_id.into(),
            vol,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_fair_value_event() {
        let event = MarketEvent::fair_value("BTC-USD", dec!(50000));
        assert_eq!(event.instrument_id(), "BTC-USD");

        if let MarketEvent::FairValue { price, .. } = event {
            assert_eq!(price, dec!(50000));
        } else {
            panic!("Wrong event type");
        }
    }

    #[test]
    fn test_sentiment_clamping() {
        let event = MarketEvent::sentiment("BTC-USD", dec!(2.5)); // Should clamp to 1.0
        if let MarketEvent::Sentiment { score, .. } = event {
            assert_eq!(score, Decimal::ONE);
        }
    }
}
