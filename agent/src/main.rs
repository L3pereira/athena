use agent::gateway::{Gateway, GatewayConfig};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("agent=info".parse()?))
        .init();

    tracing::info!("Starting trading agent...");

    let config = GatewayConfig {
        rest_url: "http://localhost:8080".to_string(),
        ws_url: "ws://localhost:8080/ws".to_string(),
        api_key: "agent-001".to_string(),
    };

    let gateway = Gateway::new(config);

    // TODO: Start gateway and connect to exchange-sim
    tracing::info!("Gateway configured: {:?}", gateway.config());

    Ok(())
}
