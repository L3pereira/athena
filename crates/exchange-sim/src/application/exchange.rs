use athena_ports::RiskManager;
use athena_risk::BasicRiskManager;
use chrono::{Duration, Utc};
use log::{debug, error, info, warn};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::time::interval;
use uuid::Uuid;

use crate::error::{ExchangeError, Result};
use crate::infrastructure::{
    Router,
    time::{ExchangeClock, TimeScale},
};
use crate::model::{ExchangeMessage, FeeConfig, FeeSchedule, InstrumentId, MarginAccount, Order};

/// Main exchange that coordinates all operations
pub struct Exchange {
    /// Available trading symbols
    symbols: Vec<String>,

    /// Shared exchange clock
    clock: Arc<ExchangeClock>,

    /// Channel to the router
    router_tx: Sender<ExchangeMessage>,

    /// Channel to clients (for notifications)
    client_tx: Sender<ExchangeMessage>,

    /// Heartbeat interval in milliseconds
    #[allow(dead_code)]
    heartbeat_interval_ms: u64,

    /// Reference to the router for configuration
    router: Arc<Router>,

    /// Fee configuration for instruments
    fee_config: Arc<RwLock<FeeConfig>>,

    /// Risk manager for margin validation
    risk_manager: Arc<BasicRiskManager>,

    /// Margin accounts (keyed by account_id)
    accounts: Arc<RwLock<HashMap<Uuid, MarginAccount>>>,
}

impl Exchange {
    /// Create a new exchange instance
    pub async fn new(
        symbols: Vec<String>,
        client_tx: Sender<ExchangeMessage>,
        heartbeat_interval_ms: u64,
        channel_capacity: usize,
        default_matching_algorithm: String,
    ) -> Result<Self> {
        // Initialize the exchange clock
        let clock = Arc::new(ExchangeClock::new(None));

        // Create channels
        let (router_tx, router_rx) = channel::<ExchangeMessage>(channel_capacity);
        let (exchange_tx, exchange_rx) = channel::<ExchangeMessage>(channel_capacity);

        // Initialize router
        let router = Arc::new(Router::new(
            exchange_tx.clone(),
            clock.clone(),
            channel_capacity,
            default_matching_algorithm,
        ));

        // Create order books for each symbol
        for symbol in &symbols {
            router.create_order_book(symbol.clone())?;
        }

        // Initialize fee configuration with defaults
        let fee_config = Arc::new(RwLock::new(FeeConfig::new()));

        // Initialize risk manager with 10x leverage
        let risk_manager = Arc::new(BasicRiskManager::with_leverage(10));

        // Initialize accounts storage
        let accounts = Arc::new(RwLock::new(HashMap::new()));

        // Spawn router task
        let router_clone = router.clone();

        tokio::spawn(async move {
            router_clone.run(router_rx).await;
        });

        // Spawn exchange message handler task
        let client_tx_clone = client_tx.clone();
        tokio::spawn(async move {
            Exchange::process_exchange_messages(exchange_rx, client_tx_clone).await;
        });

        // Start heartbeat
        let router_tx_clone = router_tx.clone();
        let clock_clone = clock.clone();
        tokio::spawn(async move {
            Exchange::start_heartbeat(router_tx_clone, clock_clone, heartbeat_interval_ms).await;
        });

        Ok(Self {
            symbols,
            clock,
            router_tx,
            client_tx,
            heartbeat_interval_ms,
            router,
            fee_config,
            risk_manager,
            accounts,
        })
    }

    /// Create exchange with custom fee configuration
    pub async fn new_with_fees(
        symbols: Vec<String>,
        client_tx: Sender<ExchangeMessage>,
        heartbeat_interval_ms: u64,
        channel_capacity: usize,
        default_matching_algorithm: String,
        fee_config: FeeConfig,
    ) -> Result<Self> {
        let exchange = Self::new(
            symbols,
            client_tx,
            heartbeat_interval_ms,
            channel_capacity,
            default_matching_algorithm,
        )
        .await?;

        *exchange.fee_config.write().await = fee_config;
        Ok(exchange)
    }

    /// Start the periodic heartbeat task
    async fn start_heartbeat(
        tx: Sender<ExchangeMessage>,
        clock: Arc<ExchangeClock>,
        interval_ms: u64,
    ) {
        info!("Starting heartbeat with interval of {}ms", interval_ms);

        let mut tick_interval = interval(tokio::time::Duration::from_millis(interval_ms));

        loop {
            tick_interval.tick().await;

            // Get current time
            match clock.now().await {
                Ok(current_time) => {
                    // Send heartbeat
                    if let Err(e) = tx.send(ExchangeMessage::Heartbeat(current_time)).await {
                        error!("Error sending heartbeat: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Error getting current time: {}", e);
                }
            }
        }
    }

    /// Process messages coming back from order books via the router
    async fn process_exchange_messages(
        mut rx: Receiver<ExchangeMessage>,
        client_tx: Sender<ExchangeMessage>,
    ) {
        while let Some(message) = rx.recv().await {
            // Forward message to clients
            match &message {
                ExchangeMessage::OrderUpdate {
                    order_id,
                    status,
                    filled_qty,
                    symbol,
                } => {
                    debug!(
                        "Order update: id={}, symbol={}, status={:?}, filled={}",
                        order_id, symbol, status, filled_qty
                    );
                }
                ExchangeMessage::Trade(trade) => {
                    debug!(
                        "Trade executed: id={}, symbol={}, buy_order={}, sell_order={}, price={}, qty={}",
                        trade.id,
                        trade.symbol(),
                        trade.buy_order_id,
                        trade.sell_order_id,
                        trade.price,
                        trade.quantity
                    );
                }
                _ => {}
            }

            if let Err(e) = client_tx.send(message).await {
                error!("Failed to forward message to client: {}", e);
            }
        }
    }

    /// Submit a new order to the exchange
    pub async fn submit_order(&self, order: Order) -> Result<Uuid> {
        // Validate symbol before sending
        let symbol = order.symbol().to_string();
        if !self.symbols.contains(&symbol) {
            return Err(ExchangeError::SymbolNotFound(symbol));
        }

        let order_id = order.id;

        info!(
            "Submitting order: id={}, symbol={}, side={:?}, type={:?}, price={:?}",
            order.id,
            order.symbol(),
            order.side,
            order.order_type,
            order.price
        );

        self.router_tx
            .send(ExchangeMessage::SubmitOrder(order))
            .await
            .map_err(|e| ExchangeError::ChannelSendError(e.to_string()))?;

        Ok(order_id)
    }

    /// Cancel an existing order
    pub async fn cancel_order(&self, order_id: Uuid) -> Result<()> {
        info!("Canceling order: id={}", order_id);

        self.router_tx
            .send(ExchangeMessage::CancelOrder(order_id))
            .await
            .map_err(|e| ExchangeError::ChannelSendError(e.to_string()))?;

        Ok(())
    }

    /// Set the clock's time scale for testing
    pub async fn set_time_scale(&self, scale: TimeScale) -> Result<()> {
        info!("Setting time scale to {:?}", scale);
        self.clock.set_time_scale(scale).await
    }

    /// Advance time (for Fixed time scale)
    pub async fn advance_time(&self, duration: Duration) -> Result<()> {
        info!(
            "Advancing time by {} milliseconds",
            duration.num_milliseconds()
        );
        self.clock.advance_time(duration).await
    }

    /// Get the current exchange time
    pub async fn current_time(&self) -> Result<chrono::DateTime<Utc>> {
        self.clock.now().await
    }

    /// Get available symbols
    pub fn symbols(&self) -> &[String] {
        &self.symbols
    }

    /// Set matching algorithm for a specific symbol
    pub fn set_matching_algorithm(&self, symbol: String, algorithm: String) -> Result<()> {
        if !self.symbols.contains(&symbol) {
            return Err(ExchangeError::SymbolNotFound(symbol));
        }

        self.router.set_matching_algorithm(symbol, algorithm);
        Ok(())
    }

    // Method to adjust heartbeat interval at runtime
    pub fn set_heartbeat_interval(&mut self, _new_interval_ms: u64) -> Result<()> {
        // self.heartbeat_interval_ms = 0;
        // Implementation to restart heartbeat task...
        todo!();
    }

    // Method to send custom messages to clients
    pub fn notify_clients(&self, message: ExchangeMessage) -> Result<()> {
        self.client_tx
            .try_send(message)
            .map_err(|e| ExchangeError::ChannelSendError(e.to_string()))?;
        Ok(())
    }

    // ============ Fee Management ============

    /// Set fee schedule for a specific instrument
    pub async fn set_instrument_fees(&self, instrument_id: InstrumentId, schedule: FeeSchedule) {
        info!(
            "Setting fees for {}: maker={}, taker={}",
            instrument_id.as_str(),
            schedule.maker_fee,
            schedule.taker_fee
        );
        let mut config = self.fee_config.write().await;
        config.set_instrument_schedule(instrument_id, schedule);
    }

    /// Set default fees for all instruments
    pub async fn set_default_fees(&self, maker_fee: Decimal, taker_fee: Decimal) {
        info!(
            "Setting default fees: maker={}, taker={}",
            maker_fee, taker_fee
        );
        let mut config = self.fee_config.write().await;
        config.set_default_schedule(FeeSchedule::new(maker_fee, taker_fee));
    }

    /// Get the fee configuration (read-only)
    pub async fn fee_config(&self) -> FeeConfig {
        self.fee_config.read().await.clone()
    }

    /// Get maker fee rate for an instrument
    pub async fn maker_fee_rate(&self, instrument_id: &InstrumentId) -> Decimal {
        self.fee_config.read().await.maker_fee_rate(instrument_id)
    }

    /// Get taker fee rate for an instrument
    pub async fn taker_fee_rate(&self, instrument_id: &InstrumentId) -> Decimal {
        self.fee_config.read().await.taker_fee_rate(instrument_id)
    }

    // ============ Account Management ============

    /// Register a new margin account
    pub async fn register_account(&self, account: MarginAccount) -> Uuid {
        let account_id = account.id;
        info!(
            "Registering account: id={}, owner={}",
            account_id, account.owner_id
        );
        let mut accounts = self.accounts.write().await;
        accounts.insert(account_id, account);
        account_id
    }

    /// Get an account by ID
    pub async fn get_account(&self, account_id: &Uuid) -> Option<MarginAccount> {
        self.accounts.read().await.get(account_id).cloned()
    }

    /// Update mark prices for all accounts
    pub async fn update_mark_prices(&self, prices: &HashMap<InstrumentId, Decimal>) {
        let mut accounts = self.accounts.write().await;
        let mut liquidations = Vec::new();

        for account in accounts.values_mut() {
            let liq_orders = self.risk_manager.update_mark_prices(account, prices);
            if !liq_orders.is_empty() {
                warn!(
                    "Liquidation triggered for account {}: {} positions",
                    account.id,
                    liq_orders.len()
                );
                liquidations.extend(liq_orders);
            }
        }

        // Process liquidations (would submit liquidation orders)
        for liq in liquidations {
            warn!(
                "Liquidation order: account={}, instrument={}, qty={}, reason={}",
                liq.account_id,
                liq.instrument_id.as_str(),
                liq.quantity,
                liq.reason
            );
            // In a real system, you'd submit market orders to close positions
        }
    }

    /// Validate an order against risk limits
    pub async fn validate_order_risk(&self, account_id: &Uuid, order: &Order) -> Result<()> {
        let accounts = self.accounts.read().await;
        let account = accounts.get(account_id).ok_or_else(|| {
            ExchangeError::ValidationError(format!("Account not found: {}", account_id))
        })?;

        let result = self
            .risk_manager
            .validate_order(account, order)
            .map_err(|e| ExchangeError::ValidationError(e.to_string()))?;

        if !result.approved {
            return Err(ExchangeError::ValidationError(
                result
                    .rejection_reason
                    .unwrap_or_else(|| "Order rejected by risk check".to_string()),
            ));
        }

        for warning in &result.warnings {
            warn!("Order risk warning: {}", warning);
        }

        Ok(())
    }

    /// Get the risk manager
    pub fn risk_manager(&self) -> &BasicRiskManager {
        &self.risk_manager
    }
}
