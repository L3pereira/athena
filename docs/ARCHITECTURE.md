# Athena Architecture

This document describes the system architecture, component interactions, and data flow through Athena's trading simulation platform.

## System Overview

Athena is a multi-agent trading simulation platform where independent trading agents compete in a realistic market environment. The architecture follows Clean Architecture principles with clear separation between:

- **Domain Layer**: Core trading types (Order, Trade, Position)
- **Application Layer**: Strategy execution, order management, risk controls
- **Infrastructure Layer**: Exchange simulation, channel-based communication

```mermaid
graph TB
    subgraph Simulation["Simulation Runner"]
        BOOT[Bootstrap]
        SIM[TradingSimulation]
        EF[EventFeed]
    end

    subgraph Agents["Trading Agents"]
        A1[Agent 1<br/>MarketMaker]
        A2[Agent 2<br/>MeanReversion]
        AN[Agent N<br/>...]
    end

    subgraph Infrastructure["Infrastructure"]
        GW[Gateway]
        EX[Exchange-Sim]
    end

    subgraph Management["Management Layer"]
        OM[Order Manager]
        RM[Risk Manager]
    end

    BOOT --> SIM
    SIM --> A1 & A2 & AN
    EF -.->|fair value| A2
    A1 & A2 & AN -->|orders| GW
    GW <-->|submit/response| EX
    GW -->|market data| A1 & A2 & AN
    OM -.->|signals| A1 & A2
    RM -.->|parameters| OM
```

## Component Details

### 1. Gateway (`athena-gateway`)

The Gateway provides a transport-agnostic interface between strategies and exchanges. It normalizes wire formats and handles message routing.

**Key Components:**
- `GatewayIn`: Receives exchange events, publishes to internal components
- `GatewayOut`: Receives order requests, submits to exchange
- `Transport`: Pluggable transport layer (channels, future: NATS, ZeroMQ)

```mermaid
graph LR
    subgraph GatewayIn["Gateway In"]
        MD[Market Data<br/>Publisher]
        TR[Trade<br/>Publisher]
        OR[Order Response<br/>Publisher]
    end

    subgraph GatewayOut["Gateway Out"]
        OQ[Order<br/>Requester]
        CQ[Cancel<br/>Requester]
    end

    EX[Exchange] -->|trades, updates| GatewayIn
    GatewayIn -->|broadcast| STRAT[Strategies]
    STRAT -->|request/reply| GatewayOut
    GatewayOut -->|submit| EX
```

### 2. Strategy (`athena-strategy`)

Strategies implement trading logic and react to market events. Each strategy maintains its own local state (order book replica, positions) to avoid contention.

**Key Components:**
- `Strategy` trait: Interface for all trading strategies
- `LocalOrderBook`: Lock-free order book replica per strategy
- `StrategyContext`: Provides market state to strategy callbacks

**Built-in Strategies:**
- `BasicMarketMaker`: Inventory-based market making with quote skewing
- `MeanReversionTaker`: Informed trading based on fair value signals

```mermaid
classDiagram
    class Strategy {
        <<trait>>
        +name() str
        +on_book_update(update, ctx) Vec~Action~
        +on_trade(trade, ctx) Vec~Action~
        +on_order_update(update, ctx) Vec~Action~
        +on_event(event, ctx) Vec~Action~
        +on_tick(ctx) Vec~Action~
        +on_shutdown() Vec~Action~
    }

    class BasicMarketMaker {
        -config: MarketMakerConfig
        -state: MMState
        +calculate_quotes(mid, position)
        +generate_quotes(book, position)
    }

    class MeanReversionTaker {
        -config: MeanReversionConfig
        -fair_value: Option~Decimal~
        +calculate_deviation()
        +generate_signal()
    }

    Strategy <|.. BasicMarketMaker
    Strategy <|.. MeanReversionTaker
```

### 3. Order Manager (`athena-order-manager`)

Aggregates signals from multiple strategies into portfolio targets, plans execution, and validates against risk parameters.

**Key Components:**
- `SignalAggregator`: Combines signals by confidence weighting
- `ExecutionPlanner`: Converts portfolio delta to order slices
- `RiskValidator`: Validates targets against risk parameters
- `PositionTracker`: Tracks positions with strategy attribution

```mermaid
flowchart LR
    subgraph Input
        S1[Strategy 1<br/>Signal]
        S2[Strategy 2<br/>Signal]
    end

    subgraph OrderManager["Order Manager"]
        AGG[Signal<br/>Aggregator]
        RISK[Risk<br/>Validator]
        EXEC[Execution<br/>Planner]
        POS[Position<br/>Tracker]
    end

    subgraph Output
        ORD[Orders]
    end

    S1 & S2 --> AGG
    AGG -->|portfolio target| RISK
    RISK -->|validated target| EXEC
    EXEC --> ORD
    ORD -.->|fills| POS
    POS -.->|positions| AGG
```

### 4. Risk Manager (`athena-risk-manager`)

Monitors trading activity and publishes risk parameters that control order execution.

**Key Components:**
- `TradingRiskManager`: Central risk state management
- `TradingRiskParameters`: Published limits and controls
- `BasicSurveillance`: Market manipulation detection

```mermaid
flowchart TB
    subgraph Inputs
        TRADES[Trade Feed]
        BOOKS[Order Books]
        PNL[PnL Updates]
    end

    subgraph RiskManager["Risk Manager"]
        TRM[Trading Risk<br/>Manager]
        SURV[Surveillance]
        PARAMS[Risk<br/>Parameters]
    end

    subgraph Controls
        DD[Drawdown<br/>Limits]
        PL[Position<br/>Limits]
        MQ[Market<br/>Quality]
    end

    TRADES --> TRM
    BOOKS --> SURV
    PNL --> TRM
    SURV -->|alerts| TRM
    TRM --> PARAMS
    PARAMS --> DD & PL & MQ
```

### 5. Runner (`athena-runner`)

Orchestrates the full simulation: bootstraps agents, connects channels, and manages the simulation lifecycle.

**Key Components:**
- `SimulationBootstrap`: Creates exchange, registers agent accounts
- `AgentRunner`: Wraps strategy with event loop and state
- `EventFeedSimulator`: Generates fair value and sentiment events
- `TradingSimulation`: Main orchestration loop

## Message Flow Sequences

### Order Submission Flow

```mermaid
sequenceDiagram
    participant S as Strategy
    participant A as AgentRunner
    participant B as ExchangeBridge
    participant E as Exchange
    participant OB as OrderBook

    S->>A: Action::SubmitOrder(request)
    A->>A: Track open order
    A->>B: AgentOrder via mpsc
    B->>B: Convert to core::Order
    B->>E: submit_order(order)
    E->>OB: route order
    OB->>OB: match/add to book
    OB-->>E: OrderUpdate
    E-->>B: order_id or error
    B-->>A: OrderResponse via mpsc
    A->>S: on_order_update(response)
```

### Market Data Flow

```mermaid
sequenceDiagram
    participant E as Exchange
    participant P as MessageProcessor
    participant BC as broadcast::Sender
    participant A1 as Agent 1
    participant A2 as Agent 2

    E->>P: ExchangeMessage::Trade
    P->>P: Update order book state
    E->>P: ExchangeMessage::Heartbeat
    P->>BC: OrderBookUpdate::Snapshot
    BC-->>A1: recv() OrderBookUpdate
    BC-->>A2: recv() OrderBookUpdate
    A1->>A1: Apply to LocalOrderBook
    A2->>A2: Apply to LocalOrderBook
    A1->>A1: strategy.on_book_update()
    A2->>A2: strategy.on_book_update()
```

### Event Feed Flow (Informed Trading)

```mermaid
sequenceDiagram
    participant EF as EventFeed
    participant BC as broadcast::Sender
    participant MM as MarketMaker
    participant MR as MeanReversion

    loop Every interval
        EF->>EF: Generate MarketEvent
        EF->>BC: FairValue{price}
        BC-->>MM: recv() - ignored
        BC-->>MR: recv() FairValue
        MR->>MR: Update fair_value
        MR->>MR: calculate_deviation()
        alt deviation > threshold
            MR->>MR: generate_signal()
            MR-->>MR: Action::SubmitOrder
        end
    end
```

### Risk Validation Flow

```mermaid
sequenceDiagram
    participant S as Strategy
    participant AGG as SignalAggregator
    participant RV as RiskValidator
    participant RM as RiskManager
    participant EP as ExecutionPlanner

    S->>AGG: Signal{target, alpha, confidence}
    AGG->>AGG: Aggregate by instrument
    AGG->>RV: PortfolioTarget
    RV->>RM: Get TradingRiskParameters
    RM-->>RV: Parameters

    alt Trading Disabled
        RV-->>AGG: RiskResult::reject()
    else Position Limit Exceeded
        RV->>RV: Adjust target to limit
        RV-->>AGG: RiskResult::pass_with_adjustment()
    else Drawdown Warning
        RV->>RV: Apply size multiplier
        RV-->>AGG: RiskResult::pass_with_adjustment()
    else All Clear
        RV-->>AGG: RiskResult::pass()
    end

    AGG->>EP: Validated target
    EP->>EP: Calculate delta from position
    EP->>EP: Slice into orders
    EP-->>S: Vec<OrderPlan>
```

## Channel Architecture

All inter-component communication uses tokio channels - no serialization overhead for in-process messaging.

```mermaid
graph TB
    subgraph Broadcast["broadcast::channel (1-to-many)"]
        MD_TX[Market Data TX]
        EF_TX[Event Feed TX]
    end

    subgraph MPSC["mpsc::channel (many-to-1)"]
        ORD_TX[Order TX]
        RESP_TX[Response TX per agent]
    end

    MD_TX -->|subscribe| A1[Agent 1]
    MD_TX -->|subscribe| A2[Agent 2]
    EF_TX -->|subscribe| A2

    A1 -->|send| ORD_TX
    A2 -->|send| ORD_TX
    ORD_TX --> BRIDGE[Exchange Bridge]

    BRIDGE -->|send| RESP_TX
    RESP_TX --> A1
```

**Channel Types:**
| Channel | Type | Purpose |
|---------|------|---------|
| Market Data | `broadcast<OrderBookUpdate>` | OB snapshots/deltas to all agents |
| Event Feed | `broadcast<MarketEvent>` | Fair value, sentiment to takers |
| Orders | `mpsc<AgentOrder>` | Order requests from agents |
| Responses | `mpsc<OrderResponse>` | Per-agent order responses |

## Agent Isolation

Each agent runs in complete isolation with private state:

```mermaid
graph TB
    subgraph Agent1["Agent 1 (tokio::spawn)"]
        S1[Strategy]
        OB1[LocalOrderBook]
        POS1[Positions]
        ORD1[OpenOrders]
    end

    subgraph Agent2["Agent 2 (tokio::spawn)"]
        S2[Strategy]
        OB2[LocalOrderBook]
        POS2[Positions]
        ORD2[OpenOrders]
    end

    BC[Broadcast Channels] -->|clone| Agent1
    BC -->|clone| Agent2

    Note1[No shared mutable state<br/>No locks required]
```

**Benefits:**
- No contention between agents
- Each agent processes at its own pace
- Lagged agents don't block others
- Easy to add/remove agents dynamically

## See Also

- [Simulation Guide](SIMULATION.md) - Running multi-agent simulations
- [README](../README.md) - Project overview and quick start
