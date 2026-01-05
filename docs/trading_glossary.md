# Trading Framework - Symbol Glossary

This glossary defines all mathematical symbols used across the trading framework documentation. Symbols are disambiguated with subscripts where the same letter is used for different concepts.

---

## Greek Symbols

### γ (Gamma) - Risk/Impact Parameters

| Symbol | Name | Context | Definition | Typical Range |
|--------|------|---------|------------|---------------|
| **γ_inv** | Inventory risk aversion | A-S / GLFT | Penalty on inventory variance; higher = more risk averse | 0.01 - 1.0 |
| **γ_perm** | Permanent impact | Almgren-Chriss | Price impact per unit traded that doesn't decay | 0.05 - 0.4 |

**Note**: In some literature, both use plain γ. We distinguish them here to avoid confusion.

### λ (Lambda) - Risk/Impact Parameters

| Symbol | Name | Context | Definition | Typical Range |
|--------|------|---------|------------|---------------|
| **λ_risk** | Risk aversion | Optimization | Penalty on variance in objective function | 10⁻⁵ - 10⁻² |
| **λ_kyle** | Kyle's Lambda | Market microstructure | Price impact per unit of signed order flow | 10⁻⁵ - 10⁻² |

**Relationship**: In Kyle (1985), λ = σ_v / (2·σ_u) where σ_v is informed trader volatility and σ_u is noise trader volatility.

### δ (Delta) - Half-Spread

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **δ** | Half-spread | A-S / GLFT | Distance from mid-price to bid OR ask |

**Critical Clarification**:
- δ is the HALF-spread (one side)
- Full spread = 2δ
- Bid price = mid - δ
- Ask price = mid + δ

**A-S Formula**: δ = γ_inv·σ²·τ + (2/γ_inv)·ln(1 + γ_inv/k)

### θ (Theta) - Mean Reversion Speed

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **θ** | Mean reversion speed | OU process | Rate at which Fair Value reverts to long-term mean |

**Ornstein-Uhlenbeck Process**:
```
dFV = θ(μ - FV)dt + σ_FV·dW

Where:
- FV = Fair Value (current)
- μ = Long-term mean
- θ = Mean reversion speed
- σ_FV = Volatility of FV innovations
- dW = Wiener process increment
```

**Regime Interpretation**:
| θ Value | Regime | Interpretation |
|---------|--------|----------------|
| θ < 0.1 | Trending | FV moves slowly, price follows; momentum works |
| θ ≈ 0.3 | Normal | Moderate reversion; mixed strategies |
| θ > 0.5 | Mean-reverting | FV snaps back quickly; mean reversion works |
| θ ≈ 0 | Random walk | No predictable reversion |

**Half-life**: Time for deviation to decay by 50% = ln(2)/θ ≈ 0.693/θ

### σ (Sigma) - Volatility

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **σ** | Volatility | General | Standard deviation of returns |
| **σ_FV** | FV volatility | OU process | Volatility of fair value innovations |
| **σ_v** | Informed vol | Kyle model | Volatility of informed trader's signal |
| **σ_u** | Noise vol | Kyle model | Volatility of noise trader order flow |

**Annualization**: Daily σ × √252 = Annual σ

### η (Eta) - Temporary Impact

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **η** | Temporary impact | Almgren-Chriss | Transient price impact coefficient (decays after trade) |

**Impact function**: g(v) = η·v where v is trading rate

### τ (Tau) - Time Remaining

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **τ** | Time remaining | A-S / GLFT | Normalized time to session/horizon end |

**Calculation**: τ = (T_end - t) / T_total, where τ ∈ [0, 1]

### κ (Kappa) - Almgren-Chriss Parameter

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **κ** | Urgency parameter | Almgren-Chriss | κ = √(λ_risk·σ²/η) |

**Optimal trajectory**: x(t) = X₀ · sinh(κ(T-t)) / sinh(κT)

---

## Latin Symbols

| Symbol | Name | Context | Definition |
|--------|------|---------|------------|
| **k** | Order arrival intensity | A-S / GLFT | Rate of order arrivals per unit time |
| **q** | Inventory | A-S / GLFT | Current position in units (+ = long, - = short) |
| **s** | Reference price | A-S / GLFT | Mid-price or fair value estimate |
| **r** | Reservation price | A-S | Price at which MM is indifferent; r = s - γ_inv·q·σ²·τ |
| **T** | Horizon | Almgren-Chriss | Total execution time |
| **X** | Total quantity | Almgren-Chriss | Total shares to execute |
| **x(t)** | Remaining shares | Almgren-Chriss | Shares remaining at time t |
| **v** | Trading rate | Almgren-Chriss | Shares per unit time |

---

## Key Formulas Quick Reference

### Avellaneda-Stoikov (2008)

```
Reservation price: r = s - γ_inv·q·σ²·τ
Optimal half-spread: δ = γ_inv·σ²·τ + (2/γ_inv)·ln(1 + γ_inv/k)

Components:
- γ_inv·σ²·τ = Volatility risk premium
- (2/γ_inv)·ln(1 + γ_inv/k) = Adverse selection component
```

### GLFT (2013)

```
Half-spread: δ = (1/γ_inv)·ln(1 + γ_inv/k) + fee_adjustment
Fee adjustment = (-maker_rebate + taker_fee × unwind_prob) / 2
```

### Almgren-Chriss (2000)

```
Objective: min E[Cost] + λ_risk·Var[Cost]
Optimal trajectory: x(t) = X₀ · sinh(κ(T-t)) / sinh(κT)
Where: κ = √(λ_risk·σ²/η)
```

### Kyle's Lambda (1985)

```
Price impact: ΔP = λ_kyle · (signed_order_flow)
Equilibrium: λ_kyle = σ_v / (2·σ_u)
```

### GARCH(1,1)

```
σ²_t = ω + α·ε²_{t-1} + β·σ²_{t-1}

Where:
- ω = long-term variance weight
- α = shock sensitivity (ARCH term)
- β = persistence (GARCH term)
- α + β < 1 for stationarity
```

---

## Regime Detection Variables

| Variable | Definition | Threshold Interpretation |
|----------|------------|--------------------------|
| **VR** | Variance Ratio = Var(q-period returns) / (q × Var(1-period returns)) | VR > 1 = trending, VR < 1 = mean-reverting |
| **H** | Hurst exponent | H > 0.5 = trending, H < 0.5 = mean-reverting, H = 0.5 = random walk |
| **AC** | Autocorrelation of returns | AC > 0 = trending, AC < 0 = mean-reverting |

---

## Subscript Conventions

| Subscript | Meaning |
|-----------|---------|
| _inv | Inventory-related |
| _perm | Permanent (doesn't decay) |
| _temp | Temporary (decays) |
| _risk | Risk aversion context |
| _kyle | Kyle model context |
| _FV | Fair value context |
| _t | At time t |

---

## See Also

- [Trading Strategies](trading_strategies.md) - Strategy implementations
- [Trading Microstructure](trading_microstructure.md) - Market microstructure patterns
- [Trading Risk](trading_risk.md) - Risk metrics and management
- [Trading Philosophy](trading_philosophy.md) - Edge vs risk premium
