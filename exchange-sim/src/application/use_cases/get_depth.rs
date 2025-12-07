use crate::application::ports::{MarketDataReader, RequestRateLimiter};
use crate::domain::{DepthSnapshotEvent, PriceLevel, Symbol};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct GetDepthQuery {
    pub symbol: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct DepthResult {
    pub last_update_id: u64,
    pub bids: Vec<PriceLevel>,
    pub asks: Vec<PriceLevel>,
}

impl DepthResult {
    pub fn to_snapshot(&self) -> DepthSnapshotEvent {
        DepthSnapshotEvent::new(self.last_update_id, self.bids.clone(), self.asks.clone())
    }
}

pub struct GetDepthUseCase<MD, R>
where
    MD: MarketDataReader,
    R: RequestRateLimiter,
{
    market_data: Arc<MD>,
    rate_limiter: Arc<R>,
}

impl<MD, R> GetDepthUseCase<MD, R>
where
    MD: MarketDataReader,
    R: RequestRateLimiter,
{
    pub fn new(market_data: Arc<MD>, rate_limiter: Arc<R>) -> Self {
        Self {
            market_data,
            rate_limiter,
        }
    }

    pub async fn execute(
        &self,
        client_id: &str,
        query: GetDepthQuery,
    ) -> Result<DepthResult, DepthError> {
        // Binance depth endpoint weights:
        // Limit 5, 10, 20, 50, 100 = weight 1
        // Limit 500 = weight 5
        // Limit 1000 = weight 10
        // Limit 5000 = weight 50
        let limit = query.limit.unwrap_or(100);
        let weight = match limit {
            0..=100 => 1,
            101..=500 => 5,
            501..=1000 => 10,
            _ => 50,
        };

        // Check rate limit
        let rate_result = self.rate_limiter.check_request(client_id, weight).await;
        if !rate_result.allowed {
            return Err(DepthError::RateLimited {
                retry_after_ms: rate_result.retry_after.map(|d| d.as_millis() as u64),
            });
        }

        // Parse symbol
        let symbol =
            Symbol::new(&query.symbol).map_err(|e| DepthError::InvalidSymbol(e.to_string()))?;

        // Get depth
        let (bids, asks, sequence) = self
            .market_data
            .get_depth(&symbol, limit)
            .await
            .ok_or_else(|| DepthError::SymbolNotFound(query.symbol.clone()))?;

        Ok(DepthResult {
            last_update_id: sequence,
            bids,
            asks,
        })
    }
}

#[derive(Debug, Clone)]
pub enum DepthError {
    RateLimited { retry_after_ms: Option<u64> },
    InvalidSymbol(String),
    SymbolNotFound(String),
}

impl std::fmt::Display for DepthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepthError::RateLimited { retry_after_ms } => {
                write!(f, "Rate limited")?;
                if let Some(ms) = retry_after_ms {
                    write!(f, ", retry after {}ms", ms)?;
                }
                Ok(())
            }
            DepthError::InvalidSymbol(s) => write!(f, "Invalid symbol: {}", s),
            DepthError::SymbolNotFound(s) => write!(f, "Symbol not found: {}", s),
        }
    }
}

impl std::error::Error for DepthError {}
