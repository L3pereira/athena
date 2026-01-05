# Reflexive Market Architecture

## Overview

This document describes the architecture for a reflexive market simulation where:
- Markets are bootstrapped with regime-specific orderbook moments
- Trades impact entire L2 structure (not just price)
- Large trades can shift the regime itself (Soros's reflexivity)
- Agents detect market structure and adapt in real-time

---

## Clean Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│                     DOMAIN LAYER                            │
│  Pure business logic, no external dependencies              │
│  - OrderbookMoments, FullImpact, MarketStructureState       │
│  - Regime definitions, Signal types                         │
└─────────────────────────────────────────────────────────────┘
                            ↑
                    Depends on
                            │
┌─────────────────────────────────────────────────────────────┐
│                   APPLICATION LAYER                          │
│  Use cases, orchestration                                    │
│  - FullImpactModel, RegimeShiftDetector                     │
│  - SyntheticOrderbookGenerator, ReflexiveDMM                │
│  - ReflexiveAgent, Simulator                                │
└─────────────────────────────────────────────────────────────┘
                            ↑
                    Depends on
                            │
┌─────────────────────────────────────────────────────────────┐
│                 INFRASTRUCTURE LAYER                         │
│  External concerns (I/O, matching engine)                   │
│  - MatchingEngine, BinanceClient (data ingestion)           │
│  - Persistence, Logging                                      │
└─────────────────────────────────────────────────────────────┘
```

---

## SOLID Principles Applied

### Single Responsibility Principle (SRP)

Each component has one reason to change:

| Component | Responsibility |
|-----------|----------------|
| `OrderbookMoments` | Represent statistical moments of orderbook |
| `FullImpactModel` | Estimate structural impact of trades |
| `RegimeShiftDetector` | Detect when trades shift the regime |
| `SyntheticOrderbookGenerator` | Generate orderbooks from moments |
| `ReflexiveDMM` | Maintain orderbook + adapt to regime shifts |
| `ReflexiveAgent` | Make trading decisions with impact awareness |

### Open/Closed Principle (OCP)

**Extensible without modification:**

```python
# Impact model is abstract - add new models without changing existing code
class ImpactModel(Protocol):
    def estimate(self, order_size: float, side: Side, orderbook: OrderBook) -> Impact:
        ...

# Implementations
class SquareRootImpact(ImpactModel): ...
class FullImpactModel(ImpactModel): ...  # Our new L2-aware model
class PropagatorImpact(ImpactModel): ...  # Future: full path dependence
```

```python
# Orderbook generator is pluggable
class OrderbookGenerator(Protocol):
    def generate(self, mid_price: float) -> OrderBook:
        ...

# Implementations
class SyntheticOrderbookGenerator(OrderbookGenerator): ...  # Copula-based
class EmpiricalBootstrapGenerator(OrderbookGenerator): ...  # From historical
class FactorModelGenerator(OrderbookGenerator): ...         # Latent factor
```

### Liskov Substitution Principle (LSP)

All agents are substitutable:

```python
class TradingAgent(Protocol):
    def decide(self, state: MarketState, orderbook: OrderBook, ...) -> List[MarketEvent]:
        ...

# All agent types can be used interchangeably
agents: List[TradingAgent] = [
    ReflexiveAgent(...),      # Impact-aware
    InformedTraderAgent(...), # FV-based
    NoiseTraderAgent(...),    # Random
    MarketMakerAgent(...),    # Quoting
]
```

### Interface Segregation Principle (ISP)

Small, focused interfaces:

```python
# Instead of one large interface, split by capability
class CanEstimateImpact(Protocol):
    def estimate_impact(self, order: Order) -> Impact: ...

class CanDetectRegime(Protocol):
    def detect_regime(self, state: MarketState) -> int: ...

class CanAdaptToRegime(Protocol):
    def on_regime_change(self, old: int, new: int) -> None: ...

# Agent only implements what it needs
class ReflexiveAgent(TradingAgent, CanEstimateImpact, CanDetectRegime, CanAdaptToRegime):
    ...
```

### Dependency Inversion Principle (DIP)

High-level modules don't depend on low-level modules:

```python
# Simulator depends on abstractions, not concretions
class ABMSimulator:
    def __init__(
        self,
        config: SimulationConfig,
        impact_model: ImpactModel,              # Abstract
        orderbook_generator: OrderbookGenerator, # Abstract
        regime_detector: RegimeDetector,         # Abstract
    ):
        ...
```

---

## Core Domain Types

### OrderbookMoments

Complete statistical description of orderbook state:

```python
@dataclass(frozen=True)
class OrderbookMoments:
    """Immutable value object representing L2 distribution."""

    # Spread distribution (log-normal)
    spread_mean_bps: float
    spread_var_bps: float

    # Depth per level (log-normal with exponential decay)
    depth_mean: Tuple[float, ...]  # Mean depth at levels 1..K
    depth_var: Tuple[float, ...]   # Variance at each level

    # Bid/ask imbalance [-1, 1]
    imbalance_mean: float
    imbalance_var: float

    # Shape parameters
    decay_rate: float              # How fast depth decreases away from mid
    level_correlation: float       # Correlation between adjacent levels
    n_levels: int

    # Resilience (recovery dynamics)
    recovery_half_life: float      # Steps until 50% recovery after shock
```

**Why these moments?**
- **Spread**: Tightness of market, cost to cross
- **Depth**: Capacity to absorb orders without moving price
- **Imbalance**: Directional pressure, predicts short-term returns
- **Decay rate**: Shape of orderbook (steep vs flat)
- **Level correlation**: Captures "liquidity waves" across levels
- **Recovery half-life**: How fast market heals after trades

### FullImpact

Impact on entire L2 structure, not just price:

```python
@dataclass(frozen=True)
class FullImpact:
    """Impact extends beyond price to entire orderbook structure."""

    price_impact_bps: float       # Standard: how much price moves
    spread_impact_pct: float      # How much spread widens
    depth_impact_pct: float       # How much depth consumed
    volatility_impact_pct: float  # How much short-term vol increases
    recovery_half_life: float     # Steps until 50% recovery
    regime_shift_prob: float      # P(this trade shifts the regime)
```

**Why full impact?**
- Traditional models only predict price change
- But trades also widen spreads, consume depth, increase volatility
- Large trades can permanently shift market structure (regime change)

---

## The Reflexivity Loop

```
┌─────────────────────────────────────────────────────────────────┐
│                     REFLEXIVE MARKET LOOP                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. REGIME MOMENTS define target orderbook structure            │
│         ↓                                                       │
│  2. GENERATOR creates orderbook matching those moments          │
│         ↓                                                       │
│  3. AGENTS observe market state, make trading decisions         │
│         ↓                                                       │
│  4. IMPACT MODEL estimates structural impact of trades          │
│         ↓                                                       │
│  5. REGIME DETECTOR checks: did trades shift the regime?        │
│         ↓                                                       │
│     ┌─────────┐                                                 │
│     │ SHIFTED │──YES──→ Update regime, change target moments    │
│     └─────────┘         Generator now uses NEW moments          │
│         │                                                       │
│        NO                                                       │
│         ↓                                                       │
│  6. DMM refills orderbook toward target moments                 │
│         ↓                                                       │
│     LOOP BACK TO STEP 3                                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Impact Model Design

### Why Not Just Price Impact?

Traditional impact models:
```
ΔPrice = λ × Q        (Kyle)
ΔPrice = σ × √(Q/V)   (Square-root)
```

These only tell you how much price moves. But when you trade:
- **Spread widens**: DMMs increase quotes to protect against adverse selection
- **Depth thins**: You consumed the liquidity, takes time to refill
- **Volatility spikes**: Large trades cause short-term vol increase
- **Regime may shift**: If big enough, market structure permanently changes

### Full Impact Model

```python
class FullImpactModel:
    """
    Estimates impact on entire L2 structure.

    Components:
    1. Price impact: Square-root model (empirically validated)
    2. Spread impact: Proportional to depth consumed
    3. Depth impact: Direct consumption + cascade effects
    4. Volatility impact: Large trades increase short-term vol
    5. Regime shift probability: Sigmoid function of depth ratio
    """

    def estimate(self, order_size, side, orderbook, moments) -> FullImpact:
        depth_ratio = order_size / orderbook.depth_at_touch

        return FullImpact(
            price_impact_bps=self._sqrt_impact(depth_ratio),
            spread_impact_pct=depth_ratio * self.spread_sensitivity,
            depth_impact_pct=min(depth_ratio, 1.0),
            volatility_impact_pct=depth_ratio * self.vol_sensitivity,
            recovery_half_life=moments.recovery_half_life * (1 + depth_ratio),
            regime_shift_prob=self._sigmoid(depth_ratio, threshold=0.2),
        )
```

---

## Regime Shift Detection

### What is a Regime Shift?

A regime is defined by statistical moments of market state. You shift the regime when:
1. Your trade causes moments to change significantly (> 2 sigma)
2. The change persists (doesn't recover within N steps)

### Detection Algorithm

```python
class RegimeShiftDetector:
    """
    Implements Soros's reflexivity: trades can change structure.

    Algorithm:
    1. Maintain history of moments (rolling window)
    2. Compute baseline mean and std for each moment
    3. After trade, compare new moments to baseline
    4. If deviation > threshold AND persists, regime shifted
    """

    def detect_shift(self, before: OrderbookMoments, after: OrderbookMoments) -> bool:
        for moment in ['spread_mean_bps', 'imbalance_mean', 'total_depth']:
            z_score = (after[moment] - before[moment]) / baseline_std[moment]
            if abs(z_score) > 2.0:
                return True
        return False
```

---

## Synthetic Orderbook Generation

### Generative Model: Copula-Based

Why Copula?
- Depths at adjacent levels are correlated
- If level 1 is thick, level 2 is likely thick too
- Copula captures this correlation structure

```python
class SyntheticOrderbookGenerator:
    """
    Generates orderbooks matching target regime moments.

    Algorithm:
    1. Sample spread from LogNormal(μ_spread, σ_spread)
    2. Sample imbalance from TruncatedNormal(μ_imb, σ_imb, -1, 1)
    3. For each side (bid, ask):
       a. Generate correlated uniforms via Gaussian Copula
       b. Transform to LogNormal marginals
       c. Apply exponential decay across levels
       d. Scale by side fraction (from imbalance)
    4. Build orderbook from generated depths
    """
```

### Why Log-Normal for Depths?

- Depths must be positive
- Real depth distributions are right-skewed (few large orders)
- Log-normal captures both properties
- Variance increases with level (more uncertainty far from mid)

---

## Reflexive DMM

### Role

The DMM serves two purposes:
1. **Provides liquidity**: Maintains orderbook toward target moments
2. **Detects regime changes**: Monitors if structure has shifted

### Algorithm

```python
class ReflexiveDMM:
    """
    DMM that maintains orderbook AND adapts to regime shifts.

    Each step:
    1. Extract current moments from orderbook
    2. If recent trades were large:
       a. Estimate impact
       b. If regime_shift_prob > 0.5, check for actual shift
       c. If shifted, find closest regime and switch generator
    3. If moments deviate from target, refresh orders
    """
```

---

## Reflexive Agent

### Key Capability: Impact Awareness

```python
class ReflexiveAgent:
    """
    Agent that:
    1. Detects current market regime
    2. Estimates own impact before trading
    3. Reduces size if about to shift regime
    4. Compares edge vs impact cost
    """

    def decide(self, state, orderbook, inventory) -> List[Order]:
        # Detect regime
        regime = self.regime_detector.detect(state)

        # Generate signal
        signal = self._compute_signal(state, inventory)

        # Estimate my impact
        impact = self.impact_model.estimate(signal.quantity, signal.side, orderbook)

        # Am I about to break the market?
        if impact.regime_shift_prob > 0.3:
            signal = self._reduce_size(signal)

        # Is edge > cost?
        if self._expected_edge(state) < impact.total_cost_bps:
            return []  # Not worth it

        return self._generate_orders(signal)
```

---

## Calibration from Real Data

### Moment Extraction

Given historical L2 snapshots from Binance:

```python
class OrderbookMomentCalibrator:
    """
    Extracts moments per regime from historical data.

    Algorithm:
    1. Detect regime for each snapshot (DBSCAN on price moments)
    2. Group snapshots by regime
    3. For each regime, compute:
       - Spread mean/var
       - Depth mean/var at each level
       - Imbalance mean/var
       - Decay rate (fit exponential)
       - Level correlation (empirical)
       - Recovery half-life (from trade impact analysis)
    """
```

---

## File Structure

```
abm/
├── domain/
│   ├── orderbook_moments.py       # OrderbookMoments, FullImpact
│   ├── market_structure.py        # MarketStructureState
│   └── signals.py                 # Signal types
│
├── application/
│   ├── impact/
│   │   ├── __init__.py
│   │   ├── protocols.py           # ImpactModel protocol
│   │   ├── full_impact_model.py   # L2-aware impact
│   │   └── regime_shift_detector.py
│   │
│   ├── generators/
│   │   ├── __init__.py
│   │   ├── protocols.py           # OrderbookGenerator protocol
│   │   ├── synthetic_orderbook.py # Copula-based generation
│   │   └── reflexive_dmm.py       # DMM + regime maintenance
│   │
│   ├── agents/
│   │   ├── reflexive_agent.py     # Impact-aware agent
│   │   └── ...
│   │
│   ├── calibration/
│   │   └── orderbook_calibrator.py
│   │
│   └── simulator.py               # Integration
│
└── infra/
    ├── matching_engine.py
    └── binance_client.py          # Data ingestion
```

---

## Summary

| Concept | Implementation |
|---------|----------------|
| L2 moments | `OrderbookMoments` - spread, depth, imbalance, decay, correlation |
| Full impact | `FullImpactModel` - price + spread + depth + vol + regime shift |
| Reflexivity | `RegimeShiftDetector` - detects when trades shift structure |
| Generation | `SyntheticOrderbookGenerator` - Copula-based L2 creation |
| Maintenance | `ReflexiveDMM` - maintains book + detects regime changes |
| Adaptation | `ReflexiveAgent` - estimates impact, reduces size if needed |

**Key insight**: Markets are not exogenous. When you're big enough, your actions change the structure you're trading against. The architecture must account for this reflexivity.
