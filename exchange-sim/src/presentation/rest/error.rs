use crate::presentation::rest::dto::ErrorResponse;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// API error type
#[derive(Debug)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
    pub status: StatusCode,
}

impl ApiError {
    pub fn bad_request(code: i32, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn not_found(code: i32, message: impl Into<String>) -> Self {
        ApiError {
            code,
            message: message.into(),
            status: StatusCode::NOT_FOUND,
        }
    }

    pub fn rate_limited(retry_after_ms: Option<u64>) -> Self {
        let msg = if let Some(ms) = retry_after_ms {
            format!("Too many requests; retry after {}ms", ms)
        } else {
            "Too many requests".to_string()
        };
        ApiError {
            code: -1015,
            message: msg,
            status: StatusCode::TOO_MANY_REQUESTS,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        ApiError {
            code: -1000,
            message: message.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    // Common Binance error codes
    pub fn unknown_order() -> Self {
        Self::bad_request(-2013, "Order does not exist.")
    }

    pub fn invalid_symbol(symbol: &str) -> Self {
        Self::bad_request(-1121, format!("Invalid symbol: {}", symbol))
    }

    pub fn missing_parameter(param: &str) -> Self {
        Self::bad_request(
            -1102,
            format!("Mandatory parameter '{}' was not sent", param),
        )
    }

    pub fn invalid_parameter(param: &str, reason: &str) -> Self {
        Self::bad_request(-1100, format!("Illegal parameter '{}': {}", param, reason))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(ErrorResponse::new(self.code, self.message));
        (self.status, body).into_response()
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "API Error {}: {}", self.code, self.message)
    }
}

impl std::error::Error for ApiError {}
