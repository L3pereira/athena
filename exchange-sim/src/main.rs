use exchange_sim::{Exchange, ExchangeConfig, RateLimitConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "exchange_sim=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let config = ExchangeConfig {
        host,
        rest_port: port,
        ws_port: port,
        rate_limits: RateLimitConfig {
            requests_per_minute: 1200,
            orders_per_second: 10,
            orders_per_day: 200_000,
            request_weight_per_minute: 1200,
            ws_connections_per_ip: 5,
            ws_messages_per_second: 5,
        },
        event_capacity: 10000,
    };

    tracing::info!("Starting Exchange Simulator");
    tracing::info!(
        "REST API: http://{}:{}/api/v3/",
        config.host,
        config.rest_port
    );
    tracing::info!("WebSocket: ws://{}:{}/ws", config.host, config.ws_port);
    tracing::info!("Available endpoints:");
    tracing::info!("  GET  /api/v3/ping");
    tracing::info!("  GET  /api/v3/time");
    tracing::info!("  GET  /api/v3/exchangeInfo");
    tracing::info!("  GET  /api/v3/depth?symbol=BTCUSDT&limit=100");
    tracing::info!("  POST /api/v3/order");
    tracing::info!("  DELETE /api/v3/order");
    tracing::info!("Default symbols: BTCUSDT, ETHUSDT, BNBUSDT, SOLUSDT, XRPUSDT");

    let exchange = Exchange::new(config);
    exchange.run().await
}
