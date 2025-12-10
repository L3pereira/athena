use std::fmt;

/// Unique identifier for an exchange
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExchangeId(String);

impl ExchangeId {
    pub fn new(id: impl Into<String>) -> Self {
        ExchangeId(id.into().to_lowercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
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

/// Well-known exchange identifiers
impl ExchangeId {
    pub fn binance() -> Self {
        ExchangeId::new("binance")
    }

    pub fn kraken() -> Self {
        ExchangeId::new("kraken")
    }

    pub fn simulator() -> Self {
        ExchangeId::new("simulator")
    }
}

/// A symbol qualified with its exchange
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedSymbol {
    pub exchange: ExchangeId,
    pub symbol: String,
}

impl QualifiedSymbol {
    pub fn new(exchange: impl Into<ExchangeId>, symbol: impl Into<String>) -> Self {
        QualifiedSymbol {
            exchange: exchange.into(),
            symbol: symbol.into().to_uppercase(),
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
    fn test_exchange_id() {
        let id = ExchangeId::new("Binance");
        assert_eq!(id.as_str(), "binance");
        assert_eq!(id, ExchangeId::binance());
    }

    #[test]
    fn test_qualified_symbol() {
        let sym = QualifiedSymbol::new("binance", "btcusdt");
        assert_eq!(sym.exchange, ExchangeId::binance());
        assert_eq!(sym.symbol, "BTCUSDT");
        assert_eq!(sym.to_string(), "binance:BTCUSDT");
    }
}
