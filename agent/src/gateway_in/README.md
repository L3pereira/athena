# Gateway In Module

A **Clean Architecture** gateway module for connecting to cryptocurrency exchanges. Handles WebSocket streaming, REST API calls, and order book synchronization with full SOLID compliance.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Module Structure](#module-structure)
- [Layer Details](#layer-details)
- [Order Book Synchronization](#order-book-synchronization)
- [Multi-Exchange Support](#multi-exchange-support)
- [SOLID Principles](#solid-principles)
- [Configuration](#configuration)
- [Usage Examples](#usage-examples)

---

## Architecture Overview

The module follows **Clean Architecture** with clear separation of concerns:

```mermaid
graph TB
    subgraph External["External Systems"]
        EX1["Binance API"]
        EX2["Kraken API"]
        EX3["Simulator"]
    end

    subgraph Config["Config Layer"]
        JSON["gateway_config.json"]
        Loader["Config Loader"]
        Types["Config Types"]
    end

    subgraph Application["Application Layer"]
        EM["ExchangeManager"]
        MDH["MarketDataHandler"]
        AppCfg["MarketDataConfig"]
    end

    subgraph Domain["Domain Layer"]
        Traits["Traits<br/>(DepthFetcher, OrderBookWriter)"]
        Events["Events<br/>(StreamData, WsEvent)"]
        Exchange["Exchange Types<br/>(ExchangeId, QualifiedSymbol)"]
        Sync["SyncStatus"]
    end

    subgraph Infrastructure["Infrastructure Layer"]
        REST["RestClient"]
        WS["WsClient"]
        Parsers["Parsers<br/>(Depth, Trade)"]
    end

    subgraph OrderBooks["Order Book Manager"]
        OBM["OrderBookManager"]
        Books["Per-Exchange Books"]
    end

    JSON --> Loader --> Types
    Types --> EM

    EM --> MDH
    MDH --> Traits

    REST -.->|implements| Traits
    WS --> Events
    Parsers --> Events

    REST --> EX1
    REST --> EX2
    REST --> EX3
    WS --> EX1
    WS --> EX2
    WS --> EX3

    MDH --> OBM
    OBM --> Books
```

### Dependency Flow

```mermaid
graph LR
    C[Config] --> A[Application]
    A --> D[Domain]
    I[Infrastructure] -.->|implements| D
    I --> A

    style D fill:#e1f5fe,stroke:#01579b
    style A fill:#fff3e0,stroke:#e65100
    style I fill:#f3e5f5,stroke:#7b1fa2
    style C fill:#e8f5e9,stroke:#2e7d32
```

**Key Principle**: Application layer depends on domain abstractions. Infrastructure implements those abstractions.

---

## Module Structure

```
agent/src/gateway_in/
├── mod.rs                          # Module root with re-exports
│
├── config/                         # Configuration Layer
│   ├── mod.rs                      # Config exports
│   ├── types.rs                    # JSON config structures
│   ├── loader.rs                   # Config loading & validation
│   └── gateway_config.json         # Default configuration
│
├── domain/                         # Domain Layer (Core)
│   ├── mod.rs                      # Domain exports
│   ├── traits.rs                   # Abstraction boundaries
│   ├── events.rs                   # Domain events & WS protocol
│   ├── exchange.rs                 # ExchangeId, QualifiedSymbol
│   └── sync_status.rs              # Sync state machine
│
├── application/                    # Application Layer
│   ├── mod.rs                      # Application exports
│   ├── config.rs                   # App-level config objects
│   ├── market_data_handler.rs      # Per-exchange orchestration
│   └── exchange_manager.rs         # Multi-exchange orchestration
│
└── infrastructure/                 # Infrastructure Layer
    ├── mod.rs                      # Infrastructure exports
    ├── rest_client.rs              # HTTP REST client
    ├── ws_client.rs                # WebSocket client
    └── parsers.rs                  # Exchange format parsers
```

---

## Layer Details

### Config Layer

Handles JSON-based configuration with serde deserialization.

```mermaid
classDiagram
    class GatewayConfigFile {
        +Vec~ExchangeConfig~ exchanges
        +GlobalConfig global
        +enabled_exchanges() Vec
        +validate() Result
    }

    class ExchangeConfig {
        +String id
        +String name
        +bool enabled
        +String rest_url
        +String ws_url
        +String api_key
        +Vec~String~ symbols
        +MarketDataConfigJson market_data
    }

    class GlobalConfig {
        +u64 reconnect_delay_ms
        +u32 max_reconnect_attempts
        +u64 heartbeat_interval_ms
    }

    class MarketDataConfigJson {
        +u64 snapshot_interval_ms
        +usize max_buffer_size
        +to_market_data_config()
    }

    GatewayConfigFile *-- ExchangeConfig
    GatewayConfigFile *-- GlobalConfig
    ExchangeConfig *-- MarketDataConfigJson
```

### Domain Layer

Contains business abstractions with **zero external dependencies**.

```mermaid
classDiagram
    class DepthFetcher {
        <<trait>>
        +get_depth(symbol, limit) Result~DepthSnapshotEvent~
    }

    class OrderBookWriter {
        <<trait>>
        +apply_snapshot(key, snapshot)
        +apply_update(exchange_id, update) bool
    }

    class StreamParser {
        <<trait>>
        +can_parse(stream) bool
        +parse(stream, data) Option~StreamData~
    }

    class ExchangeId {
        -String id
        +new(id) ExchangeId
        +binance() ExchangeId
        +kraken() ExchangeId
        +simulator() ExchangeId
    }

    class QualifiedSymbol {
        -ExchangeId exchange
        -String symbol
        +new(exchange, symbol) QualifiedSymbol
    }

    class SyncStatus {
        <<enum>>
        Uninitialized
        Syncing
        Synced
        OutOfSync
        +is_ready() bool
        +needs_snapshot() bool
    }

    QualifiedSymbol --> ExchangeId
```

### Application Layer

Orchestrates domain logic using infrastructure components.

```mermaid
classDiagram
    class MarketDataHandler~F,B~ {
        -MarketDataConfig config
        -Arc~F~ fetcher
        -Arc~B~ order_books
        -Arc~Mutex~ state
        +new(config, fetcher, order_books)
        +start(ws_sender) Sender~WsEvent~
        +status(symbol) SyncStatus
        +all_synced() bool
    }

    class ExchangeManager~B~ {
        -GatewayConfigFile config
        -Arc~B~ order_books
        -HashMap handlers
        +new(config, order_books)
        +initialize()
        +start_all() HashMap~Sender~
        +rest_client(exchange_id) Option
    }

    class MarketDataConfig {
        +ExchangeId exchange_id
        +Duration snapshot_interval
        +usize max_buffer_size
        +Vec~String~ symbols
    }

    MarketDataHandler --> MarketDataConfig
    ExchangeManager --> MarketDataHandler
```

### Infrastructure Layer

Implements domain abstractions with concrete technology.

```mermaid
classDiagram
    class RestClient {
        -Client client
        -String base_url
        -String api_key
        +new(base_url, api_key)
        +get_depth(symbol, limit)
        +place_order(...)
        +cancel_order(...)
    }

    class WsClient {
        -String url
        +new(url)
        +connect() Result~WsRequestSender, Receiver~
    }

    class WsRequestSender {
        -Sender~WsRequest~ tx
        -Arc~AtomicU64~ request_id
        +subscribe(streams)
        +unsubscribe(streams)
        +list_subscriptions()
    }

    class DepthParser {
        +can_parse(stream) bool
        +parse(stream, data) Option~StreamData~
    }

    class TradeParser {
        +can_parse(stream) bool
        +parse(stream, data) Option~StreamData~
    }

    RestClient ..|> DepthFetcher : implements
    DepthParser ..|> StreamParser : implements
    TradeParser ..|> StreamParser : implements
    WsClient --> WsRequestSender
```

---

## Order Book Synchronization

### Sync State Machine

```mermaid
stateDiagram-v2
    [*] --> Uninitialized

    Uninitialized --> Syncing : First update received

    Syncing --> Synced : Snapshot + buffer applied successfully
    Syncing --> Syncing : Buffer updates (waiting for snapshot)
    Syncing --> OutOfSync : Gap in updates detected

    Synced --> Synced : Updates applied successfully
    Synced --> OutOfSync : Update rejected (sequence gap)
    Synced --> OutOfSync : WebSocket disconnected

    OutOfSync --> Syncing : Snapshot queued
```

### Synchronization Flow

```mermaid
sequenceDiagram
    participant WS as WebSocket
    participant Handler as MarketDataHandler
    participant Buffer as Update Buffer
    participant REST as RestClient
    participant OB as OrderBooks

    Note over Handler: State: Uninitialized

    WS->>Handler: DepthUpdate (U=100, u=102)
    Handler->>Buffer: Buffer update
    Handler->>Handler: Queue snapshot
    Note over Handler: State: Syncing

    WS->>Handler: DepthUpdate (U=103, u=105)
    Handler->>Buffer: Buffer update

    REST->>Handler: Snapshot (lastUpdateId=101)
    Handler->>OB: Apply snapshot

    Note over Handler: Find first update where<br/>firstUpdateId <= 102 <= finalUpdateId

    Handler->>OB: Apply buffered update (100-102)
    Handler->>OB: Apply buffered update (103-105)
    Note over Handler: State: Synced

    WS->>Handler: DepthUpdate (U=106, u=108)
    Handler->>OB: Apply update directly
    OB-->>Handler: Success
    Note over Handler: State: Synced

    WS->>Handler: DepthUpdate (U=115, u=117)
    Handler->>OB: Apply update
    OB-->>Handler: Rejected (gap: expected 109)
    Handler->>Handler: Queue snapshot
    Note over Handler: State: OutOfSync
```

### Buffer Replay Algorithm

```mermaid
flowchart TD
    Start([Snapshot Received]) --> Apply[Apply Snapshot to OrderBook]
    Apply --> GetBuffer[Get Buffered Updates]
    GetBuffer --> FindFirst{Find first update where<br/>firstUpdateId <= lastUpdateId+1 <= finalUpdateId}

    FindFirst -->|Found| ReplayLoop[Replay buffered updates in order]
    FindFirst -->|Gap detected| MarkOutOfSync[Mark OutOfSync]

    ReplayLoop --> ApplyUpdate{Apply update}
    ApplyUpdate -->|Success| NextUpdate{More updates?}
    ApplyUpdate -->|Rejected| MarkOutOfSync

    NextUpdate -->|Yes| ReplayLoop
    NextUpdate -->|No| MarkSynced[Mark Synced]

    MarkOutOfSync --> Requeue[Requeue snapshot]
    MarkSynced --> Done([Done])
    Requeue --> Done
```

---

## Multi-Exchange Support

### Architecture

```mermaid
graph TB
    subgraph Config["Configuration"]
        JSON["gateway_config.json"]
    end

    subgraph Manager["ExchangeManager"]
        EM[ExchangeManager]
    end

    subgraph Exchanges["Per-Exchange Handlers"]
        subgraph Binance["Binance"]
            B_REST["RestClient"]
            B_WS["WsClient"]
            B_MDH["MarketDataHandler"]
        end

        subgraph Kraken["Kraken"]
            K_REST["RestClient"]
            K_WS["WsClient"]
            K_MDH["MarketDataHandler"]
        end

        subgraph Simulator["Simulator"]
            S_REST["RestClient"]
            S_WS["WsClient"]
            S_MDH["MarketDataHandler"]
        end
    end

    subgraph OrderBooks["Shared Order Books"]
        OBM["OrderBookManager"]

        subgraph Books["QualifiedSymbol Keys"]
            B1["binance:BTCUSDT"]
            B2["binance:ETHUSDT"]
            K1["kraken:BTCUSDT"]
            S1["simulator:BTCUSDT"]
        end
    end

    JSON --> EM
    EM --> Binance
    EM --> Kraken
    EM --> Simulator

    B_MDH --> OBM
    K_MDH --> OBM
    S_MDH --> OBM

    OBM --> Books
```

### Qualified Symbol Keys

```mermaid
graph LR
    subgraph Keys["Order Book Keys"]
        K1["binance:BTCUSDT"]
        K2["kraken:BTCUSDT"]
        K3["simulator:BTCUSDT"]
    end

    subgraph Same["Same Symbol, Different Prices"]
        B["Binance BTC<br/>$45,000"]
        K["Kraken BTC<br/>$45,010"]
        S["Simulator BTC<br/>$44,990"]
    end

    K1 --> B
    K2 --> K
    K3 --> S

    Arb["Arbitrage Detection:<br/>Buy Simulator → Sell Kraken"]
    B & K & S --> Arb
```

---

## SOLID Principles

### Single Responsibility (SRP)

```mermaid
graph TB
    subgraph SRP["Each Component Has One Job"]
        T["traits.rs<br/>Only abstractions"]
        E["events.rs<br/>Only events & protocol"]
        EX["exchange.rs<br/>Only identifiers"]
        S["sync_status.rs<br/>Only sync state"]

        RC["rest_client.rs<br/>Only HTTP"]
        WC["ws_client.rs<br/>Only WebSocket"]
        P["parsers.rs<br/>Only parsing"]

        MDH["market_data_handler.rs<br/>Only sync orchestration"]
        EM["exchange_manager.rs<br/>Only multi-exchange coordination"]
    end
```

### Open/Closed Principle (OCP)

```mermaid
graph TB
    subgraph OCP["Open for Extension, Closed for Modification"]
        Trait["StreamParser Trait"]

        Existing["Existing Parsers"]
        DP["DepthParser"]
        TP["TradeParser"]

        New["New Parsers (no code changes)"]
        CP["CoinbaseParser"]
        KP["KrakenParser"]
    end

    Trait --> Existing
    Existing --> DP
    Existing --> TP

    Trait --> New
    New --> CP
    New --> KP

    style New fill:#e8f5e9
```

### Liskov Substitution (LSP)

```rust
// All DepthFetcher implementations are interchangeable
pub struct MarketDataHandler<F: DepthFetcher, B: OrderBookWriter> {
    fetcher: Arc<F>,       // RestClient, MockFetcher, etc.
    order_books: Arc<B>,   // OrderBookManager, TestBooks, etc.
}

// Works with ANY implementation
let handler = MarketDataHandler::new(config, RestClient::new(...), books);
let handler = MarketDataHandler::new(config, MockFetcher::new(...), books);
```

### Interface Segregation (ISP)

```mermaid
graph TB
    subgraph ISP["Focused, Single-Purpose Traits"]
        DF["DepthFetcher<br/>1 method: get_depth()"]
        OBW["OrderBookWriter<br/>2 methods: apply_snapshot(), apply_update()"]
        SP["StreamParser<br/>2 methods: can_parse(), parse()"]
    end

    subgraph Not["NOT a Monolithic Interface"]
        Bad["ExchangeClient<br/>get_depth()<br/>place_order()<br/>cancel_order()<br/>get_account()<br/>subscribe()<br/>...20 more methods"]
    end

    ISP --> Good["Clients depend only on what they need"]
    Not --> Bad2["Clients forced to depend on unused methods"]

    style Not fill:#ffebee
    style ISP fill:#e8f5e9
```

### Dependency Inversion (DIP)

```mermaid
graph TB
    subgraph HighLevel["High-Level (Application)"]
        MDH["MarketDataHandler"]
    end

    subgraph Abstractions["Abstractions (Domain)"]
        DF["DepthFetcher trait"]
        OBW["OrderBookWriter trait"]
    end

    subgraph LowLevel["Low-Level (Infrastructure)"]
        RC["RestClient"]
        OBM["OrderBookManager"]
    end

    MDH -->|depends on| DF
    MDH -->|depends on| OBW

    RC -.->|implements| DF
    OBM -.->|implements| OBW

    style Abstractions fill:#fff3e0
```

---

## Configuration

### Default Configuration

```json
{
  "exchanges": [
    {
      "id": "binance",
      "name": "Binance",
      "enabled": true,
      "rest_url": "https://api.binance.com",
      "ws_url": "wss://stream.binance.com:9443/ws",
      "symbols": ["BTCUSDT", "ETHUSDT"],
      "market_data": {
        "snapshot_interval_ms": 100,
        "max_buffer_size": 1000
      }
    },
    {
      "id": "simulator",
      "name": "Local Simulator",
      "enabled": true,
      "rest_url": "http://localhost:8080",
      "ws_url": "ws://localhost:8080/ws",
      "symbols": ["BTCUSDT", "ETHUSDT"]
    }
  ],
  "global": {
    "reconnect_delay_ms": 5000,
    "max_reconnect_attempts": 10,
    "heartbeat_interval_ms": 30000
  }
}
```

### Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `snapshot_interval_ms` | 100 | Minimum time between REST snapshot requests |
| `max_buffer_size` | 1000 | Maximum updates to buffer while syncing |
| `reconnect_delay_ms` | 5000 | Delay before reconnection attempts |
| `max_reconnect_attempts` | 10 | Maximum reconnection attempts |
| `heartbeat_interval_ms` | 30000 | WebSocket heartbeat interval |

---

## Usage Examples

### Basic Setup

```rust
use agent::gateway_in::{
    load_default_config, ExchangeManager, ExchangeId
};
use agent::order_book::OrderBookManager;

#[tokio::main]
async fn main() {
    // Load configuration
    let config = load_default_config().unwrap();

    // Create shared order book manager
    let order_books = OrderBookManager::new();

    // Create and initialize exchange manager
    let mut manager = ExchangeManager::new(config, order_books.clone());
    manager.initialize();

    // Start all exchanges
    let event_senders = manager.start_all().await;

    // Access order books
    let btc_book = order_books.book("binance", "BTCUSDT");

    if btc_book.is_initialized() {
        println!("Best bid: {:?}", btc_book.best_bid());
        println!("Best ask: {:?}", btc_book.best_ask());
    }
}
```

### Single Exchange with Gateway Facade

```rust
use agent::gateway_in::{Gateway, GatewayConfig};

#[tokio::main]
async fn main() {
    let config = GatewayConfig::new(
        "http://localhost:8080".to_string(),
        "ws://localhost:8080/ws".to_string(),
        "api-key".to_string(),
    );

    let gateway = Gateway::new(config);

    // Use REST client
    let depth = gateway.rest()
        .get_depth("BTCUSDT", Some(20))
        .await
        .unwrap();

    println!("Last update ID: {}", depth.last_update_id);
}
```

### Custom Parser Implementation

```rust
use agent::gateway_in::{StreamParser, StreamData};
use serde_json::Value;

struct CoinbaseParser;

impl StreamParser for CoinbaseParser {
    fn can_parse(&self, stream: &str) -> bool {
        stream.contains("coinbase")
    }

    fn parse(&self, stream: &str, data: &Value) -> Option<StreamData> {
        // Parse Coinbase-specific format
        let symbol = data.get("product_id")?.as_str()?;
        let bids = parse_coinbase_levels(data.get("bids")?);
        let asks = parse_coinbase_levels(data.get("asks")?);

        Some(StreamData::DepthUpdate {
            symbol: symbol.to_string(),
            event_time: 0,
            first_update_id: 0,
            final_update_id: 0,
            bids,
            asks,
        })
    }
}
```

### Accessing Multi-Exchange Data

```rust
use agent::gateway_in::ExchangeId;

// Get books for same symbol on different exchanges
let binance_btc = order_books.book("binance", "BTCUSDT");
let kraken_btc = order_books.book("kraken", "BTCUSDT");

// Arbitrage detection
if let (Some(binance_ask), Some(kraken_bid)) =
    (binance_btc.best_ask(), kraken_btc.best_bid())
{
    if kraken_bid.price > binance_ask.price {
        let spread = kraken_bid.price.inner() - binance_ask.price.inner();
        println!("Arbitrage opportunity: {} spread", spread);
    }
}

// List all symbols for an exchange
let binance_symbols = order_books.symbols_for_exchange(&ExchangeId::binance());
println!("Binance symbols: {:?}", binance_symbols);
```

---

## Error Handling

### Error Types

```rust
pub enum RestError {
    Http(reqwest::Error),           // Network/HTTP errors
    Api { code: i32, msg: String }, // Exchange API errors
    Parse(String),                  // JSON parsing errors
}

pub enum WsError {
    Connection(tungstenite::Error), // WebSocket connection errors
    Serialization(serde_json::Error), // Message serialization errors
    ChannelClosed,                  // Internal channel closed
    NotConnected,                   // Not connected to exchange
}

pub enum ConfigError {
    IoError(std::io::Error),        // File I/O errors
    ParseError(serde_json::Error),  // JSON parsing errors
    NoEnabledExchanges,             // No exchanges enabled
    ExchangeNotFound(String),       // Exchange not in config
}
```

### Recovery Strategies

| Error | Recovery |
|-------|----------|
| Snapshot fetch failure | Requeued with interval backoff |
| Update apply failure | Mark OutOfSync, requeue snapshot |
| WebSocket disconnect | Mark all symbols OutOfSync, reconnect |
| Buffer overflow | Drop oldest updates, continue syncing |

---

## Thread Safety

```mermaid
graph TB
    subgraph Concurrency["Concurrency Primitives"]
        Arc["Arc<T><br/>Shared ownership"]
        Mutex["parking_lot::Mutex<br/>Synchronous state updates"]
        Atomic["AtomicU64<br/>Request ID generation"]
        Channel["mpsc::channel<br/>Event passing"]
    end

    subgraph Tasks["Concurrent Tasks"]
        EventLoop["Event Loop<br/>(processes WS events)"]
        SnapshotLoop["Snapshot Loop<br/>(fetches REST snapshots)"]
        WsIncoming["WS Incoming<br/>(receives messages)"]
        WsOutgoing["WS Outgoing<br/>(sends requests)"]
    end

    Arc --> EventLoop
    Arc --> SnapshotLoop
    Mutex --> EventLoop
    Mutex --> SnapshotLoop
    Channel --> EventLoop
    Channel --> WsIncoming
```

---

## Testing

```rust
// Mock implementations for testing
struct MockFetcher {
    responses: HashMap<String, DepthSnapshotEvent>,
}

impl DepthFetcher for MockFetcher {
    async fn get_depth(&self, symbol: &str, _limit: Option<u32>)
        -> Result<DepthSnapshotEvent, RestError>
    {
        self.responses.get(symbol)
            .cloned()
            .ok_or(RestError::Api { code: -1, msg: "Not found".into() })
    }
}

#[test]
fn test_sync_flow() {
    let fetcher = MockFetcher::with_response("BTCUSDT", snapshot);
    let books = TestOrderBooks::new();

    let handler = MarketDataHandler::new(config, fetcher, books);
    // Test synchronization logic...
}
```

---

## License

See [LICENSE](../../../LICENSE) for details.
