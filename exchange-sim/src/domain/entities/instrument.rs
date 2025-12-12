use crate::domain::entities::Network;
use crate::domain::instruments::{ExerciseStyle, OptionType};
use crate::domain::value_objects::{PRICE_SCALE, Price, Quantity, Rate, Symbol, Value};
use serde::{Deserialize, Serialize};

// ============================================================================
// SETTLEMENT TYPES
// ============================================================================

/// Settlement cycle for clearinghouse-based settlement
///
/// Defines when settlement occurs after trade execution.
/// Traditional finance uses T+N notation where N is the number of business days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementCycle {
    /// Same-day settlement (rare, used for some government securities)
    T0,
    /// Next-day settlement (common for US treasuries)
    T1,
    /// Two-day settlement (was standard for US equities before 2024)
    #[default]
    T2,
    /// Three-day settlement (legacy, used in some markets)
    T3,
}

impl SettlementCycle {
    /// Get the number of business days until settlement
    pub fn days(&self) -> u32 {
        match self {
            SettlementCycle::T0 => 0,
            SettlementCycle::T1 => 1,
            SettlementCycle::T2 => 2,
            SettlementCycle::T3 => 3,
        }
    }
}

impl std::fmt::Display for SettlementCycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettlementCycle::T0 => write!(f, "T+0"),
            SettlementCycle::T1 => write!(f, "T+1"),
            SettlementCycle::T2 => write!(f, "T+2"),
            SettlementCycle::T3 => write!(f, "T+3"),
        }
    }
}

/// Clearing method determines how positions are cleared and settled
///
/// This is distinct from `SettlementType` (Physical vs Cash) which describes
/// what is delivered. `ClearingMethod` describes the infrastructure used.
///
/// # Blockchain Clearing (Crypto)
/// - Positions are held in exchange hot custody or transferred to cold wallets
/// - Settlement requires blockchain confirmations
/// - Finality depends on network consensus (e.g., 6 confirmations for Bitcoin)
/// - Cross-exchange transfers use blockchain network
///
/// # Clearinghouse Clearing (Equities/Traditional Finance)
/// - Trades are cleared through a Central Counterparty (CCP)
/// - Settlement follows T+N cycle (business days after trade)
/// - Uses Delivery vs Payment (DVP) through Central Depository
/// - Supports multilateral netting to reduce settlement volume
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ClearingMethod {
    /// Blockchain-based clearing for cryptocurrencies
    Blockchain {
        /// The blockchain network used for settlement
        network: Network,
        /// Number of confirmations required for finality
        #[serde(default = "default_confirmations")]
        confirmations_required: u32,
    },
    /// Clearinghouse-based clearing for traditional securities
    Clearinghouse {
        /// Settlement cycle (T+0, T+1, T+2, etc.)
        #[serde(default)]
        cycle: SettlementCycle,
        /// Clearinghouse identifier (e.g., "DTCC", "LCH", "CME")
        #[serde(default = "default_clearinghouse")]
        clearinghouse_id: String,
    },
    /// Internal clearing (off-exchange, immediate)
    Internal,
}

fn default_confirmations() -> u32 {
    6 // Standard Bitcoin confirmation count
}

fn default_clearinghouse() -> String {
    "DEFAULT".to_string()
}

impl Default for ClearingMethod {
    fn default() -> Self {
        // Default to blockchain clearing with Ethereum for backwards compatibility
        ClearingMethod::Blockchain {
            network: Network::default(),
            confirmations_required: 12, // Ethereum block confirmations
        }
    }
}

impl ClearingMethod {
    /// Create a blockchain clearing method for crypto assets
    pub fn blockchain(network: Network) -> Self {
        let confirmations = match &network {
            Network::Bitcoin => 6,
            Network::Ethereum => 12,
            Network::Bsc | Network::Polygon | Network::Arbitrum => 20,
            Network::Solana => 32,
            Network::Internal => 0,
            Network::Custom(_) => 12, // Default to Ethereum-like confirmations for custom networks
        };
        ClearingMethod::Blockchain {
            network,
            confirmations_required: confirmations,
        }
    }

    /// Create a clearinghouse clearing method for equities
    pub fn clearinghouse(cycle: SettlementCycle, clearinghouse_id: impl Into<String>) -> Self {
        ClearingMethod::Clearinghouse {
            cycle,
            clearinghouse_id: clearinghouse_id.into(),
        }
    }

    /// Create internal clearing (immediate, no external verification)
    pub fn internal() -> Self {
        ClearingMethod::Internal
    }

    /// Check if this is blockchain-based clearing
    pub fn is_blockchain(&self) -> bool {
        matches!(self, ClearingMethod::Blockchain { .. })
    }

    /// Check if this is clearinghouse-based clearing
    pub fn is_clearinghouse(&self) -> bool {
        matches!(self, ClearingMethod::Clearinghouse { .. })
    }

    /// Get the network if this is blockchain clearing
    pub fn network(&self) -> Option<&Network> {
        match self {
            ClearingMethod::Blockchain { network, .. } => Some(network),
            _ => None,
        }
    }

    /// Get the settlement cycle if this is clearinghouse clearing
    pub fn settlement_cycle(&self) -> Option<SettlementCycle> {
        match self {
            ClearingMethod::Clearinghouse { cycle, .. } => Some(*cycle),
            _ => None,
        }
    }
}

impl std::fmt::Display for ClearingMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClearingMethod::Blockchain {
                network,
                confirmations_required,
            } => {
                write!(
                    f,
                    "Blockchain({:?}, {} conf)",
                    network, confirmations_required
                )
            }
            ClearingMethod::Clearinghouse {
                cycle,
                clearinghouse_id,
            } => {
                write!(f, "Clearinghouse({}, {})", clearinghouse_id, cycle)
            }
            ClearingMethod::Internal => write!(f, "Internal"),
        }
    }
}

// ============================================================================
// INSTRUMENT TYPES
// ============================================================================

/// Type of financial instrument being traded
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstrumentType {
    /// Spot trading (immediate delivery)
    #[default]
    Spot,
    /// Perpetual futures (no expiry)
    PerpetualFutures,
    /// Dated futures (with expiry)
    Futures,
    /// Options contract
    Option,
    /// Margin trading
    Margin,
}

impl std::fmt::Display for InstrumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstrumentType::Spot => write!(f, "SPOT"),
            InstrumentType::PerpetualFutures => write!(f, "PERPETUAL_FUTURES"),
            InstrumentType::Futures => write!(f, "FUTURES"),
            InstrumentType::Option => write!(f, "OPTION"),
            InstrumentType::Margin => write!(f, "MARGIN"),
        }
    }
}

/// Options-specific configuration for TradingPairConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionConfig {
    /// Strike price
    pub strike: Price,
    /// Option type (CALL or PUT)
    pub option_type: OptionType,
    /// Expiration timestamp (Unix millis)
    pub expiration_ms: i64,
    /// Exercise style
    #[serde(default)]
    pub exercise_style: ExerciseStyle,
}

/// Futures-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesConfig {
    /// Expiration timestamp (Unix millis) - None for perpetuals
    pub expiration_ms: Option<i64>,
    /// Contract multiplier scaled by PRICE_SCALE (e.g., PRICE_SCALE = 1.0)
    pub contract_multiplier: i64,
    /// Settlement asset (e.g., "USDT" for USDT-margined)
    pub settlement_asset: String,
    /// Maximum leverage allowed
    pub max_leverage: u32,
    /// Maintenance margin rate in basis points (e.g., 40 = 0.4%)
    pub maintenance_margin_bps: i64,
    /// Initial margin rate in basis points (e.g., 100 = 1%)
    pub initial_margin_bps: i64,
    /// Funding rate interval in hours (for perpetuals)
    pub funding_interval_hours: Option<u32>,
}

impl Default for FuturesConfig {
    fn default() -> Self {
        Self {
            expiration_ms: None,
            contract_multiplier: PRICE_SCALE, // 1.0 scaled
            settlement_asset: "USDT".to_string(),
            max_leverage: 125,
            maintenance_margin_bps: 40, // 0.4% = 40 bps
            initial_margin_bps: 100,    // 1% = 100 bps
            funding_interval_hours: Some(8),
        }
    }
}

impl FuturesConfig {
    /// Get maintenance margin as Rate
    pub fn maintenance_margin_rate(&self) -> Rate {
        Rate::from_bps(self.maintenance_margin_bps)
    }

    /// Get initial margin as Rate
    pub fn initial_margin_rate(&self) -> Rate {
        Rate::from_bps(self.initial_margin_bps)
    }
}

/// Trading pair configuration (exchange-level settings)
///
/// This represents the exchange's configuration for a trading pair,
/// including tick sizes, lot sizes, trading status, etc.
/// Supports multiple instrument types (Spot, Futures, Options).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingPairConfig {
    /// Type of instrument
    #[serde(default)]
    pub instrument_type: InstrumentType,
    pub symbol: Symbol,
    pub base_asset: String,
    pub quote_asset: String,
    /// Minimum price increment
    pub tick_size: Price,
    /// Minimum quantity increment
    pub lot_size: Quantity,
    /// Minimum notional value (price * quantity)
    pub min_notional: Value,
    /// Minimum quantity allowed
    pub min_quantity: Quantity,
    /// Maximum quantity allowed
    pub max_quantity: Quantity,
    /// Status: TRADING, HALT, BREAK, etc.
    pub status: InstrumentStatus,
    /// Allowed order types for this instrument
    pub order_types: Vec<String>,
    /// Maker fee rate in basis points (negative = rebate, e.g., -1 = -0.01% rebate)
    pub maker_fee_bps: i64,
    /// Taker fee rate in basis points (e.g., 10 = 0.1%)
    pub taker_fee_bps: i64,
    /// Futures-specific configuration (for PERPETUAL_FUTURES and FUTURES types)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub futures_config: Option<FuturesConfig>,
    /// Options-specific configuration (for OPTION type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_config: Option<OptionConfig>,
    /// Clearing method determines how trades are cleared and settled
    /// - Blockchain: crypto assets settled on-chain with confirmations
    /// - Clearinghouse: traditional securities settled via CCP with T+N cycle
    #[serde(default)]
    pub clearing_method: ClearingMethod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InstrumentStatus {
    #[default]
    Trading,
    Halt,
    Break,
    PreTrading,
    PostTrading,
}

impl TradingPairConfig {
    /// Create a new spot trading pair configuration
    pub fn new(
        symbol: Symbol,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
    ) -> Self {
        Self::with_type(InstrumentType::Spot, symbol, base_asset, quote_asset)
    }

    /// Create a trading pair with specific instrument type
    pub fn with_type(
        instrument_type: InstrumentType,
        symbol: Symbol,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
    ) -> Self {
        TradingPairConfig {
            instrument_type,
            symbol,
            base_asset: base_asset.into(),
            quote_asset: quote_asset.into(),
            tick_size: Price::from_f64(0.01),           // 0.01
            lot_size: Quantity::from_f64(0.001),        // 0.001
            min_notional: Value::from_int(10),          // 10
            min_quantity: Quantity::from_f64(0.000001), // 0.000001
            max_quantity: Quantity::from_f64(9000.0),   // 9000
            status: InstrumentStatus::Trading,
            order_types: vec![
                "LIMIT".to_string(),
                "MARKET".to_string(),
                "LIMIT_MAKER".to_string(),
                "STOP_LOSS".to_string(),
                "STOP_LOSS_LIMIT".to_string(),
                "TAKE_PROFIT".to_string(),
                "TAKE_PROFIT_LIMIT".to_string(),
            ],
            maker_fee_bps: 1, // 0.01% (1 bps)
            taker_fee_bps: 2, // 0.02% (2 bps)
            futures_config: None,
            option_config: None,
            clearing_method: ClearingMethod::default(),
        }
    }

    /// Create a perpetual futures configuration
    pub fn perpetual(
        symbol: Symbol,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
    ) -> Self {
        Self::with_type(
            InstrumentType::PerpetualFutures,
            symbol,
            base_asset,
            quote_asset,
        )
        .with_futures_config(FuturesConfig::default())
    }

    /// Create a dated futures configuration
    pub fn futures(
        symbol: Symbol,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
        expiration_ms: i64,
    ) -> Self {
        let futures_config = FuturesConfig {
            expiration_ms: Some(expiration_ms),
            funding_interval_hours: None, // No funding for dated futures
            ..Default::default()
        };

        Self::with_type(InstrumentType::Futures, symbol, base_asset, quote_asset)
            .with_futures_config(futures_config)
    }

    /// Create an option configuration
    pub fn option(
        symbol: Symbol,
        base_asset: impl Into<String>,
        quote_asset: impl Into<String>,
        option_config: OptionConfig,
    ) -> Self {
        Self::with_type(InstrumentType::Option, symbol, base_asset, quote_asset)
            .with_option_config(option_config)
    }

    /// Set futures-specific configuration
    pub fn with_futures_config(mut self, config: FuturesConfig) -> Self {
        self.futures_config = Some(config);
        self
    }

    /// Set option-specific configuration
    pub fn with_option_config(mut self, config: OptionConfig) -> Self {
        self.option_config = Some(config);
        self
    }

    /// Set clearing method (blockchain or clearinghouse)
    pub fn with_clearing_method(mut self, clearing_method: ClearingMethod) -> Self {
        self.clearing_method = clearing_method;
        self
    }

    /// Configure as crypto asset (blockchain clearing)
    pub fn as_crypto(self, network: Network) -> Self {
        self.with_clearing_method(ClearingMethod::blockchain(network))
    }

    /// Configure as equity (clearinghouse clearing with T+1)
    pub fn as_equity(self, clearinghouse_id: impl Into<String>) -> Self {
        self.with_clearing_method(ClearingMethod::clearinghouse(
            SettlementCycle::T1,
            clearinghouse_id,
        ))
    }

    /// Configure as equity with custom settlement cycle
    pub fn as_equity_with_cycle(
        self,
        cycle: SettlementCycle,
        clearinghouse_id: impl Into<String>,
    ) -> Self {
        self.with_clearing_method(ClearingMethod::clearinghouse(cycle, clearinghouse_id))
    }

    pub fn with_tick_size(mut self, tick_size: Price) -> Self {
        self.tick_size = tick_size;
        self
    }

    pub fn with_lot_size(mut self, lot_size: Quantity) -> Self {
        self.lot_size = lot_size;
        self
    }

    pub fn with_min_notional(mut self, min_notional: Value) -> Self {
        self.min_notional = min_notional;
        self
    }

    /// Set maker fee rate in basis points (can be negative for rebates)
    pub fn with_maker_fee_bps(mut self, bps: i64) -> Self {
        self.maker_fee_bps = bps;
        self
    }

    /// Set taker fee rate in basis points
    pub fn with_taker_fee_bps(mut self, bps: i64) -> Self {
        self.taker_fee_bps = bps;
        self
    }

    /// Set both maker and taker fees at once (in basis points)
    pub fn with_fees_bps(mut self, maker_bps: i64, taker_bps: i64) -> Self {
        self.maker_fee_bps = maker_bps;
        self.taker_fee_bps = taker_bps;
        self
    }

    /// Get maker fee as Rate
    pub fn maker_fee_rate(&self) -> Rate {
        Rate::from_bps(self.maker_fee_bps)
    }

    /// Get taker fee as Rate
    pub fn taker_fee_rate(&self) -> Rate {
        Rate::from_bps(self.taker_fee_bps)
    }

    /// Calculate fee for a trade
    /// Returns (fee_amount, is_rebate)
    pub fn calculate_fee(&self, notional: Value, is_maker: bool) -> (Value, bool) {
        let rate_bps = if is_maker {
            self.maker_fee_bps
        } else {
            self.taker_fee_bps
        };
        let rate = Rate::from_bps(rate_bps);
        let fee = rate.apply_to_value(notional);
        (Value::from_raw(fee.raw().abs()), rate_bps < 0)
    }

    pub fn is_trading(&self) -> bool {
        self.status == InstrumentStatus::Trading
    }

    pub fn validate_price(&self, price: Price) -> bool {
        if price.is_zero() {
            return false;
        }
        // Check if price is aligned to tick size
        let rounded = price.round_to_tick(self.tick_size);
        rounded == price
    }

    pub fn validate_quantity(&self, quantity: Quantity) -> bool {
        if quantity < self.min_quantity || quantity > self.max_quantity {
            return false;
        }
        // Check if quantity is aligned to lot size
        let rounded = quantity.round_to_lot(self.lot_size);
        rounded == quantity
    }

    pub fn validate_notional(&self, price: Price, quantity: Quantity) -> bool {
        let notional = price.mul_qty(quantity);
        notional >= self.min_notional
    }

    pub fn round_price(&self, price: Price) -> Price {
        price.round_to_tick(self.tick_size)
    }

    pub fn round_quantity(&self, quantity: Quantity) -> Quantity {
        quantity.round_to_lot(self.lot_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trading_pair_config_default_fees() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT");

        // Default fees: 1 bps maker, 2 bps taker
        assert_eq!(config.maker_fee_bps, 1);
        assert_eq!(config.taker_fee_bps, 2);
    }

    #[test]
    fn test_trading_pair_config_with_fees() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT")
            .with_maker_fee_bps(5) // 5 bps
            .with_taker_fee_bps(10); // 10 bps

        assert_eq!(config.maker_fee_bps, 5);
        assert_eq!(config.taker_fee_bps, 10);
    }

    #[test]
    fn test_trading_pair_config_with_fees_combined() {
        let symbol = Symbol::new("ETHUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "ETH", "USDT").with_fees_bps(-1, 3); // Maker rebate, taker fee

        assert_eq!(config.maker_fee_bps, -1); // Rebate
        assert_eq!(config.taker_fee_bps, 3);
    }

    #[test]
    fn test_calculate_fee_taker() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT").with_fees_bps(1, 2);

        // $10,000 trade as taker
        let notional = Value::from_int(10000);
        let (fee_amount, is_rebate) = config.calculate_fee(notional, false);

        // 2 bps of 10000 = 2.00
        assert_eq!(fee_amount, Value::from_int(2));
        assert!(!is_rebate);
    }

    #[test]
    fn test_calculate_fee_maker() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT").with_fees_bps(1, 2);

        // $10,000 trade as maker
        let notional = Value::from_int(10000);
        let (fee_amount, is_rebate) = config.calculate_fee(notional, true);

        // 1 bps of 10000 = 1.00
        assert_eq!(fee_amount, Value::from_int(1));
        assert!(!is_rebate);
    }

    #[test]
    fn test_calculate_fee_maker_rebate() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT").with_fees_bps(-1, 2); // Negative maker = rebate

        // $10,000 trade as maker
        let notional = Value::from_int(10000);
        let (fee_amount, is_rebate) = config.calculate_fee(notional, true);

        // 1 bps of 10000 = 1.00 (absolute value)
        assert_eq!(fee_amount, Value::from_int(1));
        assert!(is_rebate); // This is a rebate!
    }

    #[test]
    fn test_validate_price_tick_size() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "BTC", "USDT").with_tick_size(Price::from_f64(0.01));

        // Valid prices (aligned to 0.01 tick)
        assert!(config.validate_price(Price::from_f64(100.00)));
        assert!(config.validate_price(Price::from_f64(100.01)));
        assert!(config.validate_price(Price::from_f64(99.99)));

        // Invalid price (not aligned to tick)
        assert!(!config.validate_price(Price::from_f64(100.001)));

        // Zero price is invalid
        assert!(!config.validate_price(Price::ZERO));
    }

    #[test]
    fn test_validate_quantity_lot_size() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config =
            TradingPairConfig::new(symbol, "BTC", "USDT").with_lot_size(Quantity::from_f64(0.001));

        // Valid quantities (aligned to 0.001 lot)
        assert!(config.validate_quantity(Quantity::from_f64(1.000)));
        assert!(config.validate_quantity(Quantity::from_f64(0.001)));
        assert!(config.validate_quantity(Quantity::from_f64(1.234)));

        // Invalid quantity (not aligned to lot)
        assert!(!config.validate_quantity(Quantity::from_f64(1.0001)));
    }

    // ========================================================================
    // CLEARING METHOD TESTS
    // ========================================================================

    #[test]
    fn test_clearing_method_blockchain_bitcoin() {
        let method = ClearingMethod::blockchain(Network::Bitcoin);
        assert!(method.is_blockchain());
        assert!(!method.is_clearinghouse());
        assert_eq!(method.network(), Some(&Network::Bitcoin));
        assert_eq!(method.settlement_cycle(), None);

        if let ClearingMethod::Blockchain {
            confirmations_required,
            ..
        } = method
        {
            assert_eq!(confirmations_required, 6);
        }
    }

    #[test]
    fn test_clearing_method_blockchain_ethereum() {
        let method = ClearingMethod::blockchain(Network::Ethereum);
        if let ClearingMethod::Blockchain {
            confirmations_required,
            ..
        } = method
        {
            assert_eq!(confirmations_required, 12);
        }
    }

    #[test]
    fn test_clearing_method_clearinghouse() {
        let method = ClearingMethod::clearinghouse(SettlementCycle::T1, "DTCC");
        assert!(!method.is_blockchain());
        assert!(method.is_clearinghouse());
        assert_eq!(method.network(), None);
        assert_eq!(method.settlement_cycle(), Some(SettlementCycle::T1));

        if let ClearingMethod::Clearinghouse {
            cycle,
            clearinghouse_id,
        } = method
        {
            assert_eq!(cycle, SettlementCycle::T1);
            assert_eq!(clearinghouse_id, "DTCC");
        }
    }

    #[test]
    fn test_clearing_method_internal() {
        let method = ClearingMethod::internal();
        assert!(!method.is_blockchain());
        assert!(!method.is_clearinghouse());
        assert_eq!(method.network(), None);
        assert_eq!(method.settlement_cycle(), None);
    }

    #[test]
    fn test_settlement_cycle_days() {
        assert_eq!(SettlementCycle::T0.days(), 0);
        assert_eq!(SettlementCycle::T1.days(), 1);
        assert_eq!(SettlementCycle::T2.days(), 2);
        assert_eq!(SettlementCycle::T3.days(), 3);
    }

    #[test]
    fn test_trading_pair_config_as_crypto() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let config = TradingPairConfig::new(symbol, "BTC", "USDT").as_crypto(Network::Bitcoin);

        assert!(config.clearing_method.is_blockchain());
        assert_eq!(config.clearing_method.network(), Some(&Network::Bitcoin));
    }

    #[test]
    fn test_trading_pair_config_as_equity() {
        let symbol = Symbol::new("AAPL").unwrap();
        let config = TradingPairConfig::new(symbol, "AAPL", "USD").as_equity("DTCC");

        assert!(config.clearing_method.is_clearinghouse());
        assert_eq!(
            config.clearing_method.settlement_cycle(),
            Some(SettlementCycle::T1)
        );

        if let ClearingMethod::Clearinghouse {
            clearinghouse_id, ..
        } = &config.clearing_method
        {
            assert_eq!(clearinghouse_id, "DTCC");
        }
    }

    #[test]
    fn test_trading_pair_config_as_equity_with_cycle() {
        let symbol = Symbol::new("TSLA").unwrap();
        let config = TradingPairConfig::new(symbol, "TSLA", "USD")
            .as_equity_with_cycle(SettlementCycle::T2, "NSCC");

        assert!(config.clearing_method.is_clearinghouse());
        assert_eq!(
            config.clearing_method.settlement_cycle(),
            Some(SettlementCycle::T2)
        );
    }

    #[test]
    fn test_clearing_method_display() {
        let blockchain = ClearingMethod::blockchain(Network::Bitcoin);
        assert_eq!(format!("{}", blockchain), "Blockchain(Bitcoin, 6 conf)");

        let clearinghouse = ClearingMethod::clearinghouse(SettlementCycle::T1, "DTCC");
        assert_eq!(format!("{}", clearinghouse), "Clearinghouse(DTCC, T+1)");

        let internal = ClearingMethod::internal();
        assert_eq!(format!("{}", internal), "Internal");
    }
}
