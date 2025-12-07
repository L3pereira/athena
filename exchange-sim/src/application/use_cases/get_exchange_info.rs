use crate::application::ports::{InstrumentRepository, RateLimitConfig, RateLimiter};
use crate::domain::entities::TradingPairConfig;
use serde::Serialize;
use std::sync::Arc;

/// Binance-compatible exchange information
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeInfo {
    pub timezone: String,
    pub server_time: i64,
    pub rate_limits: Vec<RateLimitInfo>,
    pub symbols: Vec<SymbolInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitInfo {
    pub rate_limit_type: String,
    pub interval: String,
    pub interval_num: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo {
    pub symbol: String,
    pub status: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub base_asset_precision: u8,
    pub quote_asset_precision: u8,
    pub order_types: Vec<String>,
    pub filters: Vec<SymbolFilter>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "filterType")]
pub enum SymbolFilter {
    #[serde(rename = "PRICE_FILTER")]
    PriceFilter {
        min_price: String,
        max_price: String,
        tick_size: String,
    },
    #[serde(rename = "LOT_SIZE")]
    LotSize {
        min_qty: String,
        max_qty: String,
        step_size: String,
    },
    #[serde(rename = "MIN_NOTIONAL")]
    MinNotional { min_notional: String },
}

pub struct GetExchangeInfoUseCase<I, R>
where
    I: InstrumentRepository,
    R: RateLimiter,
{
    instrument_repo: Arc<I>,
    rate_limiter: Arc<R>,
}

impl<I, R> GetExchangeInfoUseCase<I, R>
where
    I: InstrumentRepository,
    R: RateLimiter,
{
    pub fn new(instrument_repo: Arc<I>, rate_limiter: Arc<R>) -> Self {
        Self {
            instrument_repo,
            rate_limiter,
        }
    }

    pub async fn execute(&self, client_id: &str) -> Result<ExchangeInfo, ExchangeInfoError> {
        // Weight: 10
        let rate_result = self.rate_limiter.check_request(client_id, 10).await;
        if !rate_result.allowed {
            return Err(ExchangeInfoError::RateLimited {
                retry_after_ms: rate_result.retry_after.map(|d| d.as_millis() as u64),
            });
        }

        let instruments = self.instrument_repo.get_all().await;
        let config = self.rate_limiter.config();

        Ok(ExchangeInfo {
            timezone: "UTC".to_string(),
            server_time: chrono::Utc::now().timestamp_millis(),
            rate_limits: Self::build_rate_limits(config),
            symbols: instruments
                .into_iter()
                .map(Self::build_symbol_info)
                .collect(),
        })
    }

    fn build_rate_limits(config: &RateLimitConfig) -> Vec<RateLimitInfo> {
        vec![
            RateLimitInfo {
                rate_limit_type: "REQUEST_WEIGHT".to_string(),
                interval: "MINUTE".to_string(),
                interval_num: 1,
                limit: config.request_weight_per_minute,
            },
            RateLimitInfo {
                rate_limit_type: "ORDERS".to_string(),
                interval: "SECOND".to_string(),
                interval_num: 1,
                limit: config.orders_per_second,
            },
            RateLimitInfo {
                rate_limit_type: "ORDERS".to_string(),
                interval: "DAY".to_string(),
                interval_num: 1,
                limit: config.orders_per_day,
            },
        ]
    }

    fn build_symbol_info(config: TradingPairConfig) -> SymbolInfo {
        SymbolInfo {
            symbol: config.symbol.to_string(),
            status: format!("{:?}", config.status).to_uppercase(),
            base_asset: config.base_asset.clone(),
            quote_asset: config.quote_asset.clone(),
            base_asset_precision: 8,
            quote_asset_precision: 8,
            order_types: config.order_types.clone(),
            filters: vec![
                SymbolFilter::PriceFilter {
                    min_price: "0.01".to_string(),
                    max_price: "1000000.00".to_string(),
                    tick_size: config.tick_size.to_string(),
                },
                SymbolFilter::LotSize {
                    min_qty: config.min_quantity.to_string(),
                    max_qty: config.max_quantity.to_string(),
                    step_size: config.lot_size.to_string(),
                },
                SymbolFilter::MinNotional {
                    min_notional: config.min_notional.to_string(),
                },
            ],
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExchangeInfoError {
    RateLimited { retry_after_ms: Option<u64> },
}

impl std::fmt::Display for ExchangeInfoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExchangeInfoError::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, ", retry after {}ms", ms)?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ExchangeInfoError {}
