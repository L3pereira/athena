//! Exchange Identifiers
//!
//! Types for identifying exchanges and symbols across the trading infrastructure.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Unique identifier for an exchange
///
/// Exchange IDs are normalized to lowercase (e.g., "Binance" becomes "binance").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExchangeId(String);

impl ExchangeId {
    /// Create a new exchange ID (normalized to lowercase)
    pub fn new(id: impl Into<String>) -> Self {
        ExchangeId(id.into().to_lowercase())
    }

    /// Get the exchange ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Binance exchange
    pub fn binance() -> Self {
        ExchangeId::new("binance")
    }

    /// Kraken exchange
    pub fn kraken() -> Self {
        ExchangeId::new("kraken")
    }

    /// Coinbase exchange
    pub fn coinbase() -> Self {
        ExchangeId::new("coinbase")
    }

    /// OKX exchange
    pub fn okx() -> Self {
        ExchangeId::new("okx")
    }

    /// Bybit exchange
    pub fn bybit() -> Self {
        ExchangeId::new("bybit")
    }

    /// Simulator (exchange-sim)
    pub fn simulator() -> Self {
        ExchangeId::new("simulator")
    }
}

impl fmt::Display for ExchangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for ExchangeId {
    fn from(s: &str) -> Self {
        ExchangeId::new(s)
    }
}

impl From<String> for ExchangeId {
    fn from(s: String) -> Self {
        ExchangeId::new(s)
    }
}

/// A symbol qualified with its exchange
///
/// Uniquely identifies a trading pair across all exchanges.
/// Symbols are normalized to uppercase (e.g., "btcusdt" becomes "BTCUSDT").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QualifiedSymbol {
    /// Exchange identifier
    pub exchange: ExchangeId,
    /// Symbol (trading pair), normalized to uppercase
    pub symbol: String,
}

impl QualifiedSymbol {
    /// Create a new qualified symbol
    pub fn new(exchange: impl Into<ExchangeId>, symbol: impl Into<String>) -> Self {
        QualifiedSymbol {
            exchange: exchange.into(),
            symbol: symbol.into().to_uppercase(),
        }
    }

    /// Parse from "exchange:symbol" format
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 2 {
            Some(QualifiedSymbol::new(parts[0], parts[1]))
        } else {
            None
        }
    }
}

impl fmt::Display for QualifiedSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.exchange, self.symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exchange_id_normalization() {
        let id = ExchangeId::new("Binance");
        assert_eq!(id.as_str(), "binance");
        assert_eq!(id, ExchangeId::binance());
    }

    #[test]
    fn test_exchange_id_from_str() {
        let id: ExchangeId = "KRAKEN".into();
        assert_eq!(id, ExchangeId::kraken());
    }

    #[test]
    fn test_qualified_symbol() {
        let sym = QualifiedSymbol::new("binance", "btcusdt");
        assert_eq!(sym.exchange, ExchangeId::binance());
        assert_eq!(sym.symbol, "BTCUSDT");
        assert_eq!(sym.to_string(), "binance:BTCUSDT");
    }

    #[test]
    fn test_qualified_symbol_parse() {
        let sym = QualifiedSymbol::parse("kraken:ETHUSDT").unwrap();
        assert_eq!(sym.exchange, ExchangeId::kraken());
        assert_eq!(sym.symbol, "ETHUSDT");

        assert!(QualifiedSymbol::parse("invalid").is_none());
    }

    #[test]
    fn test_exchange_id_equality() {
        assert_eq!(ExchangeId::new("binance"), ExchangeId::new("BINANCE"));
        assert_ne!(ExchangeId::binance(), ExchangeId::kraken());
    }
}
