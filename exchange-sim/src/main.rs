use exchange_sim::infrastructure::SimulatorConfig;
use exchange_sim::{Exchange, ExchangeConfig, RateLimitConfig};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn print_help() {
    eprintln!(
        r#"Exchange Simulator - Binance-compatible trading simulator

USAGE:
    exchange-sim [OPTIONS]

OPTIONS:
    --config <PATH>     Load configuration from JSON file
    --help              Print this help message

ENVIRONMENT VARIABLES:
    HOST                Server host (default: 0.0.0.0)
    PORT                Server port (default: 8080)
    RUST_LOG            Log level filter

EXAMPLES:
    # Run with defaults
    exchange-sim

    # Run with config file
    exchange-sim --config config.json

    # Run with custom port
    PORT=9000 exchange-sim
"#
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "exchange_sim=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mut config_path: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--config" | "-c" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --config requires a path argument");
                    std::process::exit(1);
                }
                config_path = Some(args[i].clone());
            }
            arg => {
                eprintln!("Unknown argument: {}", arg);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    let exchange = if let Some(path) = config_path {
        // Load from config file
        tracing::info!("Loading configuration from: {}", path);
        let sim_config = SimulatorConfig::from_file(&path)?;
        tracing::info!("Exchange: {}", sim_config.name);
        tracing::info!("Markets: {}", sim_config.markets.len());
        tracing::info!("Accounts: {}", sim_config.accounts.len());
        tracing::info!("Seed orders: {}", sim_config.seed_orders.len());

        Exchange::from_config(sim_config).await?
    } else {
        // Use default configuration with env var overrides
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

        tracing::info!("Using default configuration");
        Exchange::new(config)
    };

    tracing::info!("Starting Exchange Simulator");
    tracing::info!(
        "REST API: http://{}:{}/api/v3/",
        exchange.config.host,
        exchange.config.rest_port
    );
    tracing::info!(
        "WebSocket: ws://{}:{}/ws",
        exchange.config.host,
        exchange.config.ws_port
    );
    tracing::info!(
        "Admin API: http://{}:{}/admin/",
        exchange.config.host,
        exchange.config.rest_port
    );
    tracing::info!("Available endpoints:");
    tracing::info!("  GET  /api/v3/ping");
    tracing::info!("  GET  /api/v3/time");
    tracing::info!("  GET  /api/v3/exchangeInfo");
    tracing::info!("  GET  /api/v3/depth?symbol=BTCUSDT&limit=100");
    tracing::info!("  POST /api/v3/order");
    tracing::info!("  DELETE /api/v3/order");

    exchange.run().await
}
