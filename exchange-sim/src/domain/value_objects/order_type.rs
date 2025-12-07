use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    /// Execute immediately at best available price
    Market,
    /// Execute at specified price or better
    Limit,
    /// Limit order that only adds liquidity (rejected if would match immediately)
    LimitMaker,
    /// Market order triggered when stop price is reached
    StopLoss,
    /// Limit order triggered when stop price is reached
    StopLossLimit,
    /// Market order triggered when price rises to stop price
    TakeProfit,
    /// Limit order triggered when price rises to stop price
    TakeProfitLimit,
}

impl OrderType {
    pub fn requires_price(&self) -> bool {
        matches!(
            self,
            OrderType::Limit
                | OrderType::LimitMaker
                | OrderType::StopLossLimit
                | OrderType::TakeProfitLimit
        )
    }

    pub fn requires_stop_price(&self) -> bool {
        matches!(
            self,
            OrderType::StopLoss
                | OrderType::StopLossLimit
                | OrderType::TakeProfit
                | OrderType::TakeProfitLimit
        )
    }

    pub fn is_conditional(&self) -> bool {
        self.requires_stop_price()
    }
}

impl fmt::Display for OrderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderType::Market => write!(f, "MARKET"),
            OrderType::Limit => write!(f, "LIMIT"),
            OrderType::LimitMaker => write!(f, "LIMIT_MAKER"),
            OrderType::StopLoss => write!(f, "STOP_LOSS"),
            OrderType::StopLossLimit => write!(f, "STOP_LOSS_LIMIT"),
            OrderType::TakeProfit => write!(f, "TAKE_PROFIT"),
            OrderType::TakeProfitLimit => write!(f, "TAKE_PROFIT_LIMIT"),
        }
    }
}

impl TryFrom<&str> for OrderType {
    type Error = &'static str;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_uppercase().as_str() {
            "MARKET" => Ok(OrderType::Market),
            "LIMIT" => Ok(OrderType::Limit),
            "LIMIT_MAKER" => Ok(OrderType::LimitMaker),
            "STOP_LOSS" => Ok(OrderType::StopLoss),
            "STOP_LOSS_LIMIT" => Ok(OrderType::StopLossLimit),
            "TAKE_PROFIT" => Ok(OrderType::TakeProfit),
            "TAKE_PROFIT_LIMIT" => Ok(OrderType::TakeProfitLimit),
            _ => Err("Invalid order type"),
        }
    }
}
