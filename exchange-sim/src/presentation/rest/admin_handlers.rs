//! Admin/Bootstrap handlers for exchange simulation setup
//!
//! These endpoints are NOT part of the Binance API - they're used to
//! set up the exchange for testing (create accounts, markets, etc.)

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::application::ports::AccountRepository;
use crate::domain::{Clock, FeeSchedule, Symbol, TradingPairConfig};
use crate::presentation::rest::router::AppState;

// ============================================================================
// DTOs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateAccountRequest {
    pub owner_id: String,
    #[serde(default)]
    pub deposits: Vec<DepositRequest>,
    #[serde(default)]
    pub fee_tier: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    pub asset: String,
    pub amount: Decimal,
}

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub id: String,
    pub owner_id: String,
    pub balances: Vec<BalanceResponse>,
    pub fee_tier: u8,
}

#[derive(Debug, Serialize)]
pub struct BalanceResponse {
    pub asset: String,
    pub available: Decimal,
    pub locked: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct CreateMarketRequest {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    #[serde(default)]
    pub maker_fee_bps: Option<i32>,
    #[serde(default)]
    pub taker_fee_bps: Option<i32>,
    #[serde(default)]
    pub tick_size: Option<Decimal>,
    #[serde(default)]
    pub lot_size: Option<Decimal>,
}

#[derive(Debug, Serialize)]
pub struct MarketResponse {
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub maker_fee_bps: Decimal,
    pub taker_fee_bps: Decimal,
    pub tick_size: Decimal,
    pub lot_size: Decimal,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Account Handlers
// ============================================================================

/// POST /admin/accounts - Create a new trading account
pub async fn create_account<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
    Json(req): Json<CreateAccountRequest>,
) -> Result<(StatusCode, Json<AccountResponse>), (StatusCode, Json<ErrorResponse>)> {
    let mut account = state.account_repo.get_or_create(&req.owner_id).await;

    // Apply deposits
    for deposit in &req.deposits {
        account.deposit(&deposit.asset, deposit.amount);
    }

    // Set fee tier
    if let Some(tier) = req.fee_tier {
        account.fee_schedule = match tier {
            0 => FeeSchedule::default(),
            1 => FeeSchedule::tier_1(),
            2 => FeeSchedule::tier_2(),
            3 => FeeSchedule::tier_3(),
            4 => FeeSchedule::tier_4(),
            5 => FeeSchedule::tier_5(),
            9 => FeeSchedule::market_maker(),
            _ => FeeSchedule::default(),
        };
    }

    // Build balance response
    let balances: Vec<BalanceResponse> = req
        .deposits
        .iter()
        .map(|d| {
            let bal = account.balance(&d.asset);
            BalanceResponse {
                asset: d.asset.clone(),
                available: bal.available,
                locked: bal.locked,
            }
        })
        .collect();

    let response = AccountResponse {
        id: account.id.to_string(),
        owner_id: account.owner_id.clone(),
        balances,
        fee_tier: account.fee_schedule.tier,
    };

    state.account_repo.save(account).await;

    Ok((StatusCode::CREATED, Json(response)))
}

/// POST /admin/accounts/{owner_id}/deposit - Deposit funds to account
pub async fn deposit<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
    Path(owner_id): Path<String>,
    Json(req): Json<DepositRequest>,
) -> Result<(StatusCode, Json<BalanceResponse>), (StatusCode, Json<ErrorResponse>)> {
    let mut account = state.account_repo.get_or_create(&owner_id).await;
    account.deposit(&req.asset, req.amount);

    let bal = account.balance(&req.asset);
    let response = BalanceResponse {
        asset: req.asset,
        available: bal.available,
        locked: bal.locked,
    };

    state.account_repo.save(account).await;

    Ok((StatusCode::OK, Json(response)))
}

/// GET /admin/accounts/{owner_id} - Get account details
pub async fn get_account<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
    Path(owner_id): Path<String>,
) -> Result<Json<AccountResponse>, (StatusCode, Json<ErrorResponse>)> {
    let account = state
        .account_repo
        .get_by_owner(&owner_id)
        .await
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Account not found".to_string(),
                }),
            )
        })?;

    let balances = account
        .all_balances()
        .map(|(asset, bal)| BalanceResponse {
            asset: asset.clone(),
            available: bal.available,
            locked: bal.locked,
        })
        .collect();

    Ok(Json(AccountResponse {
        id: account.id.to_string(),
        owner_id: account.owner_id.clone(),
        balances,
        fee_tier: account.fee_schedule.tier,
    }))
}

/// PUT /admin/accounts/{owner_id}/fee-tier - Set account fee tier
pub async fn set_fee_tier<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
    Path(owner_id): Path<String>,
    Json(tier): Json<u8>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let mut account = state.account_repo.get_or_create(&owner_id).await;

    account.fee_schedule = match tier {
        0 => FeeSchedule::default(),
        1 => FeeSchedule::tier_1(),
        2 => FeeSchedule::tier_2(),
        3 => FeeSchedule::tier_3(),
        4 => FeeSchedule::tier_4(),
        5 => FeeSchedule::tier_5(),
        9 => FeeSchedule::market_maker(),
        _ => FeeSchedule::default(),
    };

    state.account_repo.save(account).await;

    Ok(StatusCode::OK)
}

// ============================================================================
// Market Handlers
// ============================================================================

/// POST /admin/markets - Create a new trading pair/market
pub async fn create_market<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
    Json(req): Json<CreateMarketRequest>,
) -> Result<(StatusCode, Json<MarketResponse>), (StatusCode, Json<ErrorResponse>)> {
    let symbol = Symbol::new(&req.symbol).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let mut config = TradingPairConfig::new(symbol, &req.base_asset, &req.quote_asset);

    // Apply custom fees (bps = basis points, 1 bps = 0.0001)
    if let Some(maker_bps) = req.maker_fee_bps {
        let rate = Decimal::new(maker_bps.into(), 4);
        config = config.with_maker_fee(rate);
    }
    if let Some(taker_bps) = req.taker_fee_bps {
        let rate = Decimal::new(taker_bps.into(), 4);
        config = config.with_taker_fee(rate);
    }
    if let Some(tick) = req.tick_size {
        config = config.with_tick_size(crate::domain::Price::from(tick));
    }
    if let Some(lot) = req.lot_size {
        config = config.with_lot_size(crate::domain::Quantity::from(lot));
    }

    let response = MarketResponse {
        symbol: config.symbol.to_string(),
        base_asset: config.base_asset.clone(),
        quote_asset: config.quote_asset.clone(),
        maker_fee_bps: config.maker_fee_rate * Decimal::new(10000, 0),
        taker_fee_bps: config.taker_fee_rate * Decimal::new(10000, 0),
        tick_size: config.tick_size.inner(),
        lot_size: config.lot_size.inner(),
    };

    state.instrument_repo.add(config);

    Ok((StatusCode::CREATED, Json(response)))
}

/// GET /admin/markets - List all markets
pub async fn list_markets<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
) -> Json<Vec<MarketResponse>> {
    let markets: Vec<MarketResponse> = state
        .instrument_repo
        .all()
        .iter()
        .map(|config| MarketResponse {
            symbol: config.symbol.to_string(),
            base_asset: config.base_asset.clone(),
            quote_asset: config.quote_asset.clone(),
            maker_fee_bps: config.maker_fee_rate * Decimal::new(10000, 0),
            taker_fee_bps: config.taker_fee_rate * Decimal::new(10000, 0),
            tick_size: config.tick_size.inner(),
            lot_size: config.lot_size.inner(),
        })
        .collect();

    Json(markets)
}

/// GET /admin/markets/{symbol} - Get market details
pub async fn get_market<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
    Path(symbol): Path<String>,
) -> Result<Json<MarketResponse>, (StatusCode, Json<ErrorResponse>)> {
    let sym = Symbol::new(&symbol).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )
    })?;

    let config = state.instrument_repo.get(&sym).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Market not found".to_string(),
            }),
        )
    })?;

    Ok(Json(MarketResponse {
        symbol: config.symbol.to_string(),
        base_asset: config.base_asset.clone(),
        quote_asset: config.quote_asset.clone(),
        maker_fee_bps: config.maker_fee_rate * Decimal::new(10000, 0),
        taker_fee_bps: config.taker_fee_rate * Decimal::new(10000, 0),
        tick_size: config.tick_size.inner(),
        lot_size: config.lot_size.inner(),
    }))
}
