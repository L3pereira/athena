use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol(String);

impl Symbol {
    pub fn new(value: impl Into<String>) -> Result<Self, &'static str> {
        let s: String = value.into();
        if s.is_empty() {
            return Err("Symbol cannot be empty");
        }
        if s.len() > 20 {
            return Err("Symbol too long (max 20 chars)");
        }
        // Binance symbols are uppercase alphanumeric
        if !s.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err("Symbol must be alphanumeric");
        }
        Ok(Symbol(s.to_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Symbol {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Symbol {
    type Error = &'static str;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Symbol::new(value)
    }
}

impl TryFrom<String> for Symbol {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Symbol::new(value)
    }
}
