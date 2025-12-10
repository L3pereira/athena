use serde_json::Value;

use crate::gateway_in::domain::{StreamData, StreamParser};

/// Parser for depth updates
/// Infrastructure component - handles Binance-format depth parsing
pub struct DepthParser;

impl StreamParser for DepthParser {
    fn can_parse(&self, stream: &str) -> bool {
        stream.to_lowercase().contains("@depth")
    }

    fn parse(&self, _stream: &str, data: &Value) -> Option<StreamData> {
        Some(StreamData::DepthUpdate {
            symbol: data.get("s")?.as_str()?.to_string(),
            event_time: data.get("E")?.as_i64()?,
            first_update_id: data.get("U")?.as_u64()?,
            final_update_id: data.get("u")?.as_u64()?,
            bids: parse_price_levels(data.get("b")?)?,
            asks: parse_price_levels(data.get("a")?)?,
        })
    }
}

/// Parser for trade updates
/// Infrastructure component - handles Binance-format trade parsing
pub struct TradeParser;

impl StreamParser for TradeParser {
    fn can_parse(&self, stream: &str) -> bool {
        stream.to_lowercase().contains("@trade")
    }

    fn parse(&self, _stream: &str, data: &Value) -> Option<StreamData> {
        Some(StreamData::Trade {
            symbol: data.get("s")?.as_str()?.to_string(),
            trade_id: data.get("t")?.as_u64()?,
            price: data.get("p")?.as_str()?.to_string(),
            quantity: data.get("q")?.as_str()?.to_string(),
            buyer_order_id: data.get("b")?.as_u64()?,
            seller_order_id: data.get("a")?.as_u64()?,
            trade_time: data.get("T")?.as_i64()?,
            is_buyer_maker: data.get("m")?.as_bool()?,
        })
    }
}

fn parse_price_levels(value: &Value) -> Option<Vec<[String; 2]>> {
    let arr = value.as_array()?;
    let mut levels = Vec::with_capacity(arr.len());

    for item in arr {
        let inner = item.as_array()?;
        if inner.len() >= 2 {
            let price = inner[0].as_str()?.to_string();
            let qty = inner[1].as_str()?.to_string();
            levels.push([price, qty]);
        }
    }

    Some(levels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_parser() {
        let parser = DepthParser;
        assert!(parser.can_parse("btcusdt@depth"));
        assert!(parser.can_parse("ETHUSDT@depth@100ms"));
        assert!(!parser.can_parse("btcusdt@trade"));

        let data = serde_json::json!({
            "e": "depthUpdate",
            "E": 1234567890,
            "s": "BTCUSDT",
            "U": 100,
            "u": 105,
            "b": [["50000.00", "1.5"], ["49999.00", "2.0"]],
            "a": [["50001.00", "1.0"]]
        });

        let result = parser.parse("btcusdt@depth", &data);
        assert!(result.is_some());

        if let Some(StreamData::DepthUpdate {
            symbol,
            first_update_id,
            final_update_id,
            bids,
            asks,
            ..
        }) = result
        {
            assert_eq!(symbol, "BTCUSDT");
            assert_eq!(first_update_id, 100);
            assert_eq!(final_update_id, 105);
            assert_eq!(bids.len(), 2);
            assert_eq!(asks.len(), 1);
        } else {
            panic!("Expected DepthUpdate");
        }
    }

    #[test]
    fn test_trade_parser() {
        let parser = TradeParser;
        assert!(parser.can_parse("btcusdt@trade"));
        assert!(!parser.can_parse("btcusdt@depth"));

        let data = serde_json::json!({
            "e": "trade",
            "E": 1234567890,
            "s": "BTCUSDT",
            "t": 12345,
            "p": "50000.00",
            "q": "1.5",
            "b": 100,
            "a": 101,
            "T": 1234567890,
            "m": true
        });

        let result = parser.parse("btcusdt@trade", &data);
        assert!(result.is_some());

        if let Some(StreamData::Trade {
            symbol,
            trade_id,
            price,
            ..
        }) = result
        {
            assert_eq!(symbol, "BTCUSDT");
            assert_eq!(trade_id, 12345);
            assert_eq!(price, "50000.00");
        } else {
            panic!("Expected Trade");
        }
    }
}
