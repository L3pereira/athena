use crate::entities::PriceLevel;
use crate::value_objects::Symbol;
use serde::{Deserialize, Serialize};

/// Binance-style depth update (delta)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthUpdateEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(rename = "b")]
    pub bids: Vec<[String; 2]>, // [price, quantity]
    #[serde(rename = "a")]
    pub asks: Vec<[String; 2]>,
}

/// Binance-style depth snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthSnapshotEvent {
    #[serde(rename = "lastUpdateId")]
    pub last_update_id: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

impl DepthUpdateEvent {
    pub fn new(
        symbol: &Symbol,
        first_update_id: u64,
        final_update_id: u64,
        bids: Vec<PriceLevel>,
        asks: Vec<PriceLevel>,
        event_time: i64,
    ) -> Self {
        DepthUpdateEvent {
            event_type: "depthUpdate".to_string(),
            event_time,
            symbol: symbol.to_string(),
            first_update_id,
            final_update_id,
            bids: bids
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
            asks: asks
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
        }
    }
}

impl DepthSnapshotEvent {
    pub fn new(last_update_id: u64, bids: Vec<PriceLevel>, asks: Vec<PriceLevel>) -> Self {
        DepthSnapshotEvent {
            last_update_id,
            bids: bids
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
            asks: asks
                .iter()
                .map(|l| [l.price.to_string(), l.quantity.to_string()])
                .collect(),
        }
    }
}
