# Trading Philosophy

Conceptual foundations for understanding trading strategies and market dynamics.

See [Glossary](trading_glossary.md) for symbol definitions.

---

## Table of Contents

1. [Reality vs Simulation](#reality-vs-simulation)
2. [Information Hierarchy](#information-hierarchy)
3. [Edge vs Risk Premium](#edge-vs-risk-premium)
4. [Key Principles](#key-principles)
5. [Decision Framework](#decision-framework)

---

## Reality vs Simulation

### Important Distinction

**In Reality:**
- **Nobody has "true" Fair Value** - that would be insider trading
- All participants **estimate** FV using models, data, analysis
- The edge is **estimation quality**: better models, faster data, more resources
- A-S/GLFT work with ANY reference price (mid, model estimate, etc.)
- Information is a **spectrum**, not binary

**In the ABM Simulation:**
- FV is an **artificial construct** for studying information asymmetry
- Some agents "see" the simulation's FV (for modeling purposes)
- This creates price discovery dynamics we want to study
- "Informed" vs "Uninformed" is a **modeling simplification**

### The Real Information Edge

| Participant | Their "Edge" (Reality) |
|-------------|----------------------|
| **Hedge Funds** | Better models, alternative data, faster execution |
| **Market Makers** | Order flow visibility, speed, inventory optimization |
| **Prop Traders** | Low latency, pattern recognition, market microstructure |
| **Retail** | None (information disadvantage) |

**Key Insight**: The strategies (A-S, GLFT, Almgren-Chriss) are used by EVERYONE. The difference is the **quality of inputs** (reference price estimate, volatility forecast, etc.), not access to ground truth.

### Reconciling the Spectrum with Binary Modeling

```
REALITY: Continuous spectrum of estimation quality
─────────────────────────────────────────────────────────────►
│                                                              │
Noise     Retail    Technical   Quant      DMM      Insider
Traders   Traders   Traders     Funds      (flow)   (illegal)
│         │         │           │          │        │
Random    Basic     Regime      Models+    Order    Perfect
trading   charts    detection   alt data   flow     info

ABM SIMULATION: Binary approximation for tractability
─────────────────────────────────────────────────────────────►
│                           │                                  │
UNINFORMED                  │               INFORMED           │
(don't see FV)              │               (see FV)           │
                            │                                  │
Momentum, MR,               │               Funds, DMMs        │
Retail, Noise               │                                  │

The binary split is WHERE you draw the line on the spectrum.
It's a modeling choice, not reality.
```

---

## Information Hierarchy

### Reality: Everyone Estimates

```
                 BETTER ESTIMATES                    WORSE ESTIMATES
                       │                                  │
        ┌──────────────┼──────────────┐    ┌─────────────┼─────────────┐
        │              │              │    │             │             │
    Quant Funds      DMMs         Prop    Retail     Day         Noise
        │              │          Traders  MMs      Traders     Traders
        │              │              │    │             │             │
   Fundamental    Order flow     Speed   Public     Technical   Random
   + Alt data     + inventory    edge    data only  analysis
        │              │              │    │             │             │
    Better FV     Better FV     Faster   Lagged      Proxy       No
    estimates     estimates     reaction estimates   signals     model
```

### ABM Simulation: Binary Information

```
              INFORMED (sees FV)              UNINFORMED (no FV access)
                       │                                  │
        ┌──────────────┴──────────────┐    ┌─────────────┴─────────────┐
        │                             │    │                           │
      Funds                         DMMs  Other MMs              Directional
        │                             │    │                           │
   Push price                    Quote   Adaptive              Momentum /
   toward FV                   around FV quoting              Mean Reversion
```

### What Each Agent Observes

| Agent | Reference Price | Order Flow | Price History |
|-------|-----------------|------------|---------------|
| All MMs | Own estimate | Full tape | Full |
| Funds | Model estimate | Own trades | Full |
| Retail | Mid price | Own trades | Delayed |

---

## Edge vs Risk Premium

### The Fundamental Distinction

Most trading profits come from **Risk Premiums**, not **Edge**.

| Concept | Definition | Example |
|---------|------------|---------|
| **Edge** | True information advantage others don't have | Insider info (illegal), proprietary data feed |
| **Risk Premium** | Compensation for bearing risk others avoid | Inventory risk, infrastructure investment, drawdown risk |

**Key Insight**: Almost all known strategies provide **risk premiums**, not edge. The strategy is public knowledge - the profit comes from being willing to take the associated risks.

### Why Known Strategies Still Work

If everyone knows about momentum, mean reversion, and stat arb, why do they still generate returns?

**Because returns are compensation for RISK, not information asymmetry.**

```
Strategy Return = Risk Premium + (Edge, if any)
                       ↑              ↑
               Most profits      Rare, often
               come from here    illegal or
                                 temporary
```

### Risk Premiums by Strategy

#### Market Making (A-S / GLFT)

| What Everyone Knows | The Risk Premium Compensates For |
|---------------------|----------------------------------|
| Quote around mid, skew for inventory | **Adverse selection**: Getting picked off by informed traders |
| Use A-S/GLFT formulas | **Inventory risk**: Holding positions overnight |
| Widen spreads in volatility | **Gap risk**: Sudden price jumps |

**Who earns it**: Those willing to provide liquidity and hold inventory risk.

#### HFT / Colocation

| What Everyone Knows | The Risk Premium Compensates For |
|---------------------|----------------------------------|
| Colocation gives speed advantage | **Infrastructure risk**: $10M+ in servers, networking |
| Microstructure patterns exist | **Operational risk**: Hiring PhDs, engineers |
| Cross-venue arbitrage exists | **Technology risk**: Systems must work 99.999% |
| | **Regulatory risk**: Rules can change |

**Who earns it**: Those willing to invest massive capital in infrastructure with uncertain payoff.

```
HFT "Edge" Analysis:
- The strategy is PUBLIC (everyone knows about colocation)
- The code is PROPRIETARY (but concepts are known)
- The returns are RISK PREMIUM for:
  * Capital investment ($10-100M infrastructure)
  * Human capital (PhD quants, engineers)
  * Operational excellence (systems must not fail)
  * Regulatory navigation
```

#### Momentum

| What Everyone Knows | The Risk Premium Compensates For |
|---------------------|----------------------------------|
| Trends tend to persist | **Crash risk**: Momentum crashes hard in reversals |
| Buy winners, sell losers | **Crowding risk**: Everyone in same trade |
| Use lookback windows | **Drawdown risk**: Extended losing periods |

**Academic Evidence**: Momentum premium is ~0.5-1% monthly, but with fat-tailed crash risk.

```
Momentum Risk Premium:
- Strategy: Buy past winners, sell past losers
- Known since: Jegadeesh & Titman (1993) - 30+ years public!
- Still works: Yes, because it's RISK COMPENSATION
- The risk: Momentum crashes (2009, 2020) can wipe out years of gains
- Who earns it: Those who can survive the crashes
```

#### Mean Reversion

| What Everyone Knows | The Risk Premium Compensates For |
|---------------------|----------------------------------|
| Prices revert to mean | **Regime change risk**: Mean can shift permanently |
| Buy low, sell high | **Timing risk**: "The market can stay irrational..." |
| Use percentile ranks, quantile bands | **Leverage risk**: Often requires leverage to be profitable |

**The Classic Blowup**: LTCM knew mean reversion. They blew up because they underestimated tail risk.

```
Mean Reversion Risk Premium:
- Strategy: Fade extreme moves
- Known since: Forever (contrarian trading)
- Still works: Yes, when regime is stable
- The risk: Regime changes, trends that don't revert
- Who earns it: Those with capital to survive drawdowns
```

#### Statistical Arbitrage

| What Everyone Knows | The Risk Premium Compensates For |
|---------------------|----------------------------------|
| Correlated assets should move together | **Model risk**: Correlations break down in stress |
| Trade the spread when it diverges | **Liquidity risk**: Can't exit when spreads blow out |
| Pairs trading, factor models | **Crowding risk**: Same trades as everyone else |

```
Stat Arb Risk Premium:
- Strategy: Trade mean-reverting spreads between related assets
- Known since: 1980s (pairs trading)
- Still works: When correlations hold
- The risk: Correlation breakdown (2007-2008 quant meltdown)
- Who earns it: Those with risk management to survive blowups
```

### True Edge is Rare and Often Temporary

| Type of Edge | Duration | Legality | Example |
|--------------|----------|----------|---------|
| **Insider information** | Until trade | Illegal | Knowing earnings before release |
| **Proprietary data** | Until competitors get it | Legal | Alternative data (satellite, credit cards) |
| **Speed advantage** | Until competitors catch up | Legal | First to colocate |
| **Model innovation** | Until published/copied | Legal | New factor discovery |
| **Execution innovation** | Until adopted | Legal | Better TWAP algorithm |

**The Decay of Edge:**
```
New edge discovered
       ↓
Early adopters profit
       ↓
Strategy becomes known
       ↓
Competition increases
       ↓
Returns compress to risk premium
       ↓
Only risk-bearers profit
```

### Risk Premium Framework

```
┌─────────────────────────────────────────────────────────────────┐
│                     RETURN DECOMPOSITION                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Total Return = Risk-Free + Market Beta + Risk Premium + Edge   │
│                     ↑           ↑            ↑           ↑      │
│                  Treasury    Equity      Strategy    True       │
│                   rate      exposure     specific    alpha      │
│                                          risk       (rare)      │
│                                                                 │
├─────────────────────────────────────────────────────────────────┤
│  STRATEGY           │ PRIMARY RISK PREMIUM                      │
├─────────────────────────────────────────────────────────────────┤
│  Market Making      │ Adverse selection + Inventory risk        │
│  Momentum           │ Crash risk + Crowding risk                │
│  Mean Reversion     │ Regime change + Timing risk               │
│  Stat Arb           │ Correlation breakdown + Model risk        │
│  HFT                │ Infrastructure + Operational risk         │
│  Carry Trade        │ Currency crash risk                       │
│  Value Investing    │ Distress risk + Duration risk             │
└─────────────────────────────────────────────────────────────────┘
```

### The Implication for You

If you're building an ABM and trading strategies:

1. **Don't expect edge from known strategies** - Momentum, mean reversion, stat arb are risk premiums
2. **Ask: What risk am I being paid to bear?**
   - Inventory risk → Market making
   - Drawdown risk → Momentum/Mean reversion
   - Infrastructure risk → HFT
   - Model risk → Stat arb
3. **Your advantage in small markets**:
   - Not edge (you don't know FV better)
   - Risk premium: You're willing to provide liquidity where others won't
   - Effort premium: You've studied A-S/GLFT when others haven't

**The honest assessment:**
```
If you know A-S/GLFT and regime detection:
- You DON'T have edge (these are public)
- You CAN earn risk premium by:
  * Providing liquidity (inventory risk)
  * Trading momentum/MR (drawdown risk)
  * Operating in small markets (effort + illiquidity risk)
```

---

## Key Principles

### 1. Everyone Uses the Same Algorithms

- A-S/GLFT used by ALL market makers
- Almgren-Chriss used by ALL institutional traders
- The difference is **input quality**, not algorithm choice

### 2. Returns = Risk Premium (Mostly)

- Known strategies work because they compensate for RISK
- True edge is rare and temporary
- Size positions based on risk you're willing to bear

### 3. Markets Have Fat Tails

- Don't use z-scores (assumes normality)
- Use percentile ranks and quantile bands
- A "3-sigma" event is much more common than normal distribution suggests

### 4. Information is a Spectrum

- Reality: Everyone estimates FV with varying quality
- ABM Simulation: Binary "sees FV" vs "doesn't" is a simplification
- The binary model is useful but not literal truth

---

## Decision Framework

### Complete Strategy Stack (All Participants)

```
┌─────────────────────────────────────────────────────────────────┐
│                     ALL MARKET MAKERS                           │
├─────────────────────────────────────────────────────────────────┤
│  QUOTING: A-S / GLFT (everyone uses the same framework)         │
│  ────────────────────────────────────────────────────────       │
│  Reference price:  │ HFT: microstructure signals                │
│                    │ DMM: order flow + model                    │
│                    │ Retail MM: mid price (no edge)             │
│  Spread:           │ f(γ_inv, σ, τ, k) - same formula for all   │
│  Inventory mgmt:   │ γ_inv·q·σ²·τ skew - same for all           │
│  Order flow:       │ VPIN, imbalance - all should use           │
├─────────────────────────────────────────────────────────────────┤
│                   ALL DIRECTIONAL TRADERS                       │
├─────────────────────────────────────────────────────────────────┤
│  SIGNAL: Various sources (the differentiator)                   │
│  ──────────────────────────────────────                         │
│  Quant Fund:  │ Fundamental models + alternative data           │
│  Prop Trader: │ Order flow + microstructure                     │
│  Technical:   │ Momentum / Mean Reversion + regime detection    │
│  Retail:      │ Basic charts (information disadvantage)         │
│                                                                 │
│  EXECUTION: Almgren-Chriss (everyone should use)                │
│  ──────────────────────────────────────────────                 │
│  Impact estimate quality differs, but framework is the same     │
└─────────────────────────────────────────────────────────────────┘
```

### Decision Tree for Strategy Selection

```
You arrive at a market
         │
         ▼
  Are you providing liquidity (Maker)?
    /              \
  YES               NO
   │                 │
   ▼                 ▼
Use A-S/GLFT    What's your signal?
   │                 │
   ▼            ┌────┴────┐
What's your     │         │
reference?   Technical  Fundamental/
   │          signals    Alt data
   │             │          │
   ▼             ▼          ▼
┌──────┐    Regime      Model-based
│Better│    Detection   signal
│model?│        │          │
└──┬───┘        ▼          ▼
   │      Momentum or   Execute with
   ▼      Mean Rev      Almgren-Chriss
Tighter      │
spreads      ▼
          Execute with
          Almgren-Chriss
```

### What Each Algorithm Needs

| Algorithm | Required Inputs | Outputs |
|-----------|-----------------|---------|
| **A-S / GLFT** | reference_price, σ, q, τ, γ_inv, k | bid, ask, sizes |
| **Almgren-Chriss** | X, T, σ, η, γ_perm, λ_risk | x(t) trajectory |
| **Momentum** | prices, lookback, threshold | direction, strength |
| **Mean Reversion** | prices, lookback, percentile_thresh | direction, strength |
| **VPIN** | trades, bucket_size | toxicity score |
| **Regime Detection** | prices, vol, autocorr | regime classification |

### Key Relationships

| If you can estimate... | You can derive... |
|------------------------|-------------------|
| Reference price (FV estimate) | Optimal spread center (A-S/GLFT) |
| Volatility σ | Spread width, execution urgency |
| Market impact η | Optimal execution trajectory (Almgren-Chriss) |
| Order flow imbalance | Adjust reference price, detect informed flow |
| Price regime (θ) | Which signal strategy to weight |

### Practical Implications

**For your ABM simulation:**
- Informed agents don't have "edge" - they have better ESTIMATES
- The FV in simulation represents "better estimate quality"
- Returns should decompose into risk premium + estimation quality

**For real trading:**
- Accept that known strategies provide risk premium, not edge
- Size positions based on risk you're willing to bear
- Understand what risk you're being compensated for
- Have capital to survive the bad scenarios

**For smaller markets without DMMs:**
- Less competition for spread capture
- Higher adverse selection risk (fewer informed participants to learn from)
- Your regime detection + A-S/GLFT knowledge gives you an advantage
- Order flow analysis is still valuable but with less data

---

## See Also

- [Glossary](trading_glossary.md) - Symbol definitions
- [Strategies](trading_strategies.md) - A-S, GLFT, Almgren-Chriss
- [Microstructure](trading_microstructure.md) - Vol clustering, price clustering
- [Risk](trading_risk.md) - VaR, CVaR, risk metrics
