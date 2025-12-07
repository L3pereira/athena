use axum::{
    Json,
    extract::{Query, State},
    http::HeaderMap,
};
use rust_decimal::Decimal;
use std::sync::Arc;

use crate::application::{
    CancelError, CancelOrderCommand, CancelOrderUseCase, DepthError, ExchangeInfoError,
    GetDepthQuery, GetDepthUseCase, GetExchangeInfoUseCase, OrderError, SubmitOrderCommand,
    SubmitOrderUseCase,
};
use crate::domain::{Clock, OrderType, Price, Quantity, Side, TimeInForce};
use crate::presentation::rest::{ApiError, dto::*};

use super::AppState;

/// GET /api/v3/ping
pub async fn ping() -> Json<PingResponse> {
    Json(PingResponse {})
}

/// GET /api/v3/time
pub async fn server_time<C: Clock>(
    State(state): State<Arc<AppState<C>>>,
) -> Json<ServerTimeResponse> {
    Json(ServerTimeResponse {
        server_time: state.clock.now_millis(),
    })
}

/// GET /api/v3/exchangeInfo
pub async fn exchange_info<C: Clock>(
    headers: HeaderMap,
    State(state): State<Arc<AppState<C>>>,
) -> Result<Json<crate::application::ExchangeInfo>, ApiError> {
    let client_id = extract_client_id(&headers);

    let use_case = GetExchangeInfoUseCase::new(
        Arc::clone(&state.instrument_repo),
        Arc::clone(&state.rate_limiter),
    );

    use_case.execute(&client_id).await.map(Json).map_err(|e| {
        ApiError::rate_limited(match e {
            ExchangeInfoError::RateLimited { retry_after_ms } => retry_after_ms,
        })
    })
}

/// GET /api/v3/depth
pub async fn depth<C: Clock>(
    headers: HeaderMap,
    Query(query): Query<DepthQuery>,
    State(state): State<Arc<AppState<C>>>,
) -> Result<Json<DepthResponse>, ApiError> {
    let client_id = extract_client_id(&headers);

    let use_case = GetDepthUseCase::new(
        Arc::clone(&state.order_book_repo),
        Arc::clone(&state.rate_limiter),
    );

    let result = use_case
        .execute(
            &client_id,
            GetDepthQuery {
                symbol: query.symbol,
                limit: Some(query.limit),
            },
        )
        .await
        .map_err(|e| match e {
            DepthError::RateLimited { retry_after_ms } => ApiError::rate_limited(retry_after_ms),
            DepthError::InvalidSymbol(s) => ApiError::invalid_symbol(&s),
            DepthError::SymbolNotFound(s) => ApiError::invalid_symbol(&s),
        })?;

    Ok(Json(DepthResponse {
        last_update_id: result.last_update_id,
        bids: result
            .bids
            .iter()
            .map(|l| [l.price.to_string(), l.quantity.to_string()])
            .collect(),
        asks: result
            .asks
            .iter()
            .map(|l| [l.price.to_string(), l.quantity.to_string()])
            .collect(),
    }))
}

/// POST /api/v3/order
pub async fn create_order<C: Clock>(
    headers: HeaderMap,
    State(state): State<Arc<AppState<C>>>,
    Json(req): Json<CreateOrderRequest>,
) -> Result<Json<OrderResponse>, ApiError> {
    let client_id = extract_client_id(&headers);

    // Parse request
    let side: Side = req
        .side
        .as_str()
        .try_into()
        .map_err(|_| ApiError::invalid_parameter("side", "must be BUY or SELL"))?;

    let order_type: OrderType = req
        .order_type
        .as_str()
        .try_into()
        .map_err(|_| ApiError::invalid_parameter("type", "invalid order type"))?;

    let quantity = req
        .quantity
        .parse::<Decimal>()
        .map_err(|_| ApiError::invalid_parameter("quantity", "invalid decimal"))?;

    let price = if let Some(p) = &req.price {
        Some(Price::from(p.parse::<Decimal>().map_err(|_| {
            ApiError::invalid_parameter("price", "invalid decimal")
        })?))
    } else {
        None
    };

    let stop_price = if let Some(p) = &req.stop_price {
        Some(Price::from(p.parse::<Decimal>().map_err(|_| {
            ApiError::invalid_parameter("stopPrice", "invalid decimal")
        })?))
    } else {
        None
    };

    let time_in_force = req
        .time_in_force
        .as_deref()
        .map(TimeInForce::try_from)
        .transpose()
        .map_err(|_| ApiError::invalid_parameter("timeInForce", "invalid value"))?
        .unwrap_or_default();

    let command = SubmitOrderCommand {
        symbol: req.symbol,
        side,
        order_type,
        quantity: Quantity::from(quantity),
        price,
        stop_price,
        time_in_force,
        client_order_id: req.new_client_order_id,
    };

    let use_case = SubmitOrderUseCase::new(
        Arc::clone(&state.clock),
        Arc::clone(&state.account_repo),
        Arc::clone(&state.order_book_repo),
        Arc::clone(&state.instrument_repo),
        Arc::clone(&state.event_publisher),
        Arc::clone(&state.rate_limiter),
    );

    let result = use_case
        .execute(&client_id, command)
        .await
        .map_err(|e| match e {
            OrderError::RateLimited { retry_after_ms } => ApiError::rate_limited(retry_after_ms),
            OrderError::InvalidSymbol(s) => ApiError::invalid_symbol(&s),
            OrderError::SymbolNotFound(s) => ApiError::invalid_symbol(&s),
            OrderError::MissingPrice => ApiError::missing_parameter("price"),
            OrderError::MissingStopPrice => ApiError::missing_parameter("stopPrice"),
            OrderError::ValidationFailed(msg) => ApiError::bad_request(-1013, msg),
            OrderError::AccountError(e) => ApiError::bad_request(-2010, e.to_string()),
            OrderError::InternalError(msg) => ApiError::internal(msg),
        })?;

    let fills: Vec<FillResponse> = result
        .fills
        .iter()
        .enumerate()
        .map(|(i, f)| FillResponse {
            price: f.price.to_string(),
            qty: f.quantity.to_string(),
            commission: f.commission.to_string(),
            commission_asset: "USDT".to_string(), // Simplified
            trade_id: i as i64,
        })
        .collect();

    Ok(Json(OrderResponse::from_order(&result.order, fills)))
}

/// DELETE /api/v3/order
pub async fn cancel_order<C: Clock>(
    headers: HeaderMap,
    State(state): State<Arc<AppState<C>>>,
    Query(req): Query<CancelOrderRequest>,
) -> Result<Json<CancelOrderResponse>, ApiError> {
    let client_id = extract_client_id(&headers);

    let order_id = req.order_id.map(|id| uuid::Uuid::from_u128(id as u128));

    let command = CancelOrderCommand {
        symbol: req.symbol.clone(),
        order_id,
        client_order_id: req.orig_client_order_id.clone(),
    };

    let use_case = CancelOrderUseCase::new(
        Arc::clone(&state.clock),
        Arc::clone(&state.order_book_repo),
        Arc::clone(&state.event_publisher),
        Arc::clone(&state.rate_limiter),
    );

    let result = use_case
        .execute(&client_id, command)
        .await
        .map_err(|e| match e {
            CancelError::RateLimited { retry_after_ms } => ApiError::rate_limited(retry_after_ms),
            CancelError::InvalidSymbol(s) => ApiError::invalid_symbol(&s),
            CancelError::SymbolNotFound(s) => ApiError::invalid_symbol(&s),
            CancelError::OrderNotFound => ApiError::unknown_order(),
            CancelError::MissingOrderId => {
                ApiError::missing_parameter("orderId or origClientOrderId")
            }
            CancelError::ValidationFailed(msg) => ApiError::bad_request(-2011, msg),
        })?;

    let order = &result.order;
    Ok(Json(CancelOrderResponse {
        symbol: order.symbol.to_string(),
        orig_client_order_id: order.client_order_id.clone().unwrap_or_default(),
        order_id: order.id.as_u128() as i64,
        order_list_id: -1,
        client_order_id: order
            .client_order_id
            .clone()
            .unwrap_or_else(|| order.id.to_string()),
        price: order
            .price
            .map(|p| p.to_string())
            .unwrap_or("0".to_string()),
        orig_qty: order.quantity.to_string(),
        executed_qty: order.filled_quantity.to_string(),
        cummulative_quote_qty: "0".to_string(), // Simplified
        status: format!("{:?}", order.status).to_uppercase(),
        time_in_force: order.time_in_force.to_string(),
        order_type: order.order_type.to_string(),
        side: order.side.to_string(),
    }))
}

/// Extract client ID from headers (IP address or API key)
fn extract_client_id(headers: &HeaderMap) -> String {
    headers
        .get("X-MBX-APIKEY")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("X-Forwarded-For")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        })
        .unwrap_or_else(|| "anonymous".to_string())
}
