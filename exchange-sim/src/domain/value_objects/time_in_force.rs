use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TimeInForce {
    /// Good Till Canceled - remains active until filled or canceled
    #[default]
    #[serde(rename = "GTC")]
    Gtc,
    /// Immediate or Cancel - fills immediately, cancels unfilled portion
    #[serde(rename = "IOC")]
    Ioc,
    /// Fill or Kill - must fill completely immediately or cancel entirely
    #[serde(rename = "FOK")]
    Fok,
    /// Good Till Date - active until specified time
    #[serde(rename = "GTD")]
    Gtd,
}

impl TimeInForce {
    pub fn allows_partial_fill(&self) -> bool {
        !matches!(self, TimeInForce::Fok)
    }

    pub fn requires_immediate_execution(&self) -> bool {
        matches!(self, TimeInForce::Ioc | TimeInForce::Fok)
    }

    pub fn is_expired(&self, expiry: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        match self {
            TimeInForce::Gtd => expiry.is_some_and(|exp| now >= exp),
            _ => false,
        }
    }
}

impl fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeInForce::Gtc => write!(f, "GTC"),
            TimeInForce::Ioc => write!(f, "IOC"),
            TimeInForce::Fok => write!(f, "FOK"),
            TimeInForce::Gtd => write!(f, "GTD"),
        }
    }
}

impl TryFrom<&str> for TimeInForce {
    type Error = &'static str;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "GTC" => Ok(TimeInForce::Gtc),
            "IOC" => Ok(TimeInForce::Ioc),
            "FOK" => Ok(TimeInForce::Fok),
            "GTD" => Ok(TimeInForce::Gtd),
            _ => Err("Invalid time in force"),
        }
    }
}
