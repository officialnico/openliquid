# OpenLiquid Documentation

## **Overview**

OpenLiquid is an open-source, high-performance, on-chain order book decentralized exchange (DEX) inspired by Hyperliquid. This documentation suite provides comprehensive technical specifications for implementing the complete system.

**Core Innovation:** A unified Layer 1 blockchain running two distinct execution engines (OpenCore for DEX operations, OpenEVM for smart contracts) under a single HotStuff-BFT consensus mechanism.

---

## **Documentation Structure**

### **🎯 Start Here**

**[GETTING_STARTED.md](GETTING_STARTED.md)** - **NEW!** Your first day guide
- Choose your role (Consensus, Core DEX, EVM, App, or QA)
- Set up development environment
- Pick your first task
- TDD workflow and examples
- Common pitfalls to avoid

**[implementation_spec.md](implementation_spec.md)** - High-level architecture and implementation roadmap
- Core architectural principles
- Milestone-based implementation plan
- Component specifications
- Protocol economics overview
- Governance framework
- **Read this first** for system overview

---

### **🔧 Core Implementation Guides**

#### **1. Consensus Layer**
**[hyperbft_implementation_plan.md](hyperbft_implementation_plan.md)** - HotStuff-BFT consensus implementation
- Phase-by-phase development plan
- Detailed Pacemaker specification
- Complexity analysis and comparison to other BFT protocols
- Formal specification guidelines
- Testing and security hardening strategies

**Key Sections:**
- Phase 1: Foundational research & toy prototyping
- Phase 2: Production implementation (Rust/Go)
- Phase 3: Testing & security
- Phase 4: Advanced features
- **Appendix:** Complexity analysis (why HotStuff beats PBFT/Tendermint/Casper)

#### **2. EVM/Core Bridge**
**[evm_core_interaction.md](evm_core_interaction.md)** - Deep dive into state synchronization
- Dual EVM block architecture (small/big blocks)
- Sequential execution model
- Precompile behavior and guarantees
- CoreWriter atomicity model
- Asset transfer mechanisms
- Security considerations and design patterns

**Critical for:** Application developers building on OpenEVM

#### **3. Market Making**
**[market_making_specification.md](market_making_specification.md)** - Mathematical foundation for MM vault
- Complete Avellaneda-Stoikov formulas
- Reservation price and optimal spread calculations
- Multi-asset tiering system
- Risk management strategies
- Reference implementation in Solidity
- Performance metrics and backtesting

**Critical for:** Liquidity providers and vault developers

#### **4. Risk Assessment**
**[RISKS.md](RISKS.md)** - Comprehensive risk analysis and mitigation
- 26 identified risks across 10 categories
- Severity and likelihood assessments
- Detailed mitigation strategies
- Priority action items (P0, P1, P2)
- Risk monitoring dashboard
- Incident response procedures

**Critical for:** Security auditors, validators, and protocol developers

#### **5. Test Specification**
**[TEST_SPECIFICATION.md](TEST_SPECIFICATION.md)** - Complete acceptance criteria and test suite
- 390+ concrete test cases across all phases
- Priority classification (P0/P1/P2/P3)
- Success criteria for each milestone
- Byzantine fault injection tests
- Performance benchmarks and targets
- Security audit checklist
- CI/CD pipeline configuration

**Critical for:** All developers - defines "done" for each milestone

#### **6. Implementation Checklist**
**[IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md)** - Day-to-day progress tracker
- Quick-reference checklist format
- 200+ actionable implementation tasks
- Progress bars for each phase
- Links to detailed test specifications
- Mainnet launch criteria
- Weekly progress tracking

**Critical for:** Project managers, developers tracking progress

---

## **Quick Reference**

### **Architecture at a Glance**

```
┌─────────────────────────────────────────────────────┐
│                  OpenLiquid L1                      │
│  (Single Consensus: HotStuff-BFT ~12 blocks/sec)   │
├─────────────────────────────────────────────────────┤
│                                                     │
│  ┌──────────────┐         ┌──────────────┐        │
│  │  OpenCore    │◄───────►│   OpenEVM    │        │
│  │              │         │              │        │
│  │ • Order Book │         │ • Smart      │        │
│  │ • Matching   │         │   Contracts  │        │
│  │ • Clearing   │         │ • Precompiles│        │
│  │ • Oracle     │         │ • CoreWriter │        │
│  └──────────────┘         └──────────────┘        │
│         │                        │                 │
│         └────────────┬───────────┘                │
│                      ↓                             │
│              Unified State                         │
│           (RocksDB/LevelDB)                       │
└─────────────────────────────────────────────────────┘
```

### **Block Structure**

```
Core Block (produced ~every 83ms):
├─ Core Transactions (orders, cancels, liquidations)
├─ [Optional] Small EVM Block (every 1s, 2M gas)
│   └─ EVM Transactions
├─ [Optional] Big EVM Block (every 60s, 30M gas)
│   └─ Complex EVM Transactions
├─ Transfer Processing (EVM → Core)
└─ CoreWriter Action Execution
```

### **Key Formulas**

#### Consensus (HotStuff)
- **Threshold:** `k = 2f+1` where `n = 3f+1`
- **Safety Rule:** `node extends lockedQC.node OR qc.viewNumber > lockedQC.viewNumber`
- **Commit Rule:** Three-Chain (prepare → pre-commit → commit)

#### Market Making (Avellaneda-Stoikov)
- **Reservation Price:** `r = s - q·γ·σ²·(T-t)`
- **Optimal Spread:** `δᵃ + δᵇ = γ·σ²·(T-t) + (2/γ)·ln(1+γ/k)`

Where:
- `s` = mid-price
- `q` = inventory
- `γ` = risk aversion
- `σ` = volatility
- `(T-t)` = time to horizon

---

## **Implementation Roadmap**

**Detailed Tracking:** See [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) for 200+ specific tasks

### **Phase 1: Consensus Foundation** (3-6 months)
- [ ] HotStuff-BFT implementation (78+ P0 tests)
- [ ] P2P networking layer (7 P1 tests)
- [ ] Threshold signature scheme (BLS) (13 P0 tests)
- [ ] State storage (RocksDB)
- [ ] Basic block production

**Deliverable:** Functional consensus network with validator set

### **Phase 2: OpenCore Engine** (4-6 months)
- [ ] On-chain order book (Red-Black Trees) (18 P0 tests)
- [ ] Matching engine (>100k orders/sec)
- [ ] Clearinghouse & margin system (9 P0 tests)
- [ ] Oracle module (7 P0 tests)
- [ ] Liquidation engine (3 P0 tests)

**Deliverable:** Performant DEX on custom L1

### **Phase 3: OpenEVM Integration** (3-4 months)
- [ ] Embedded EVM (revm)
- [ ] Dual block architecture (10 P0 tests)
- [ ] Precompiled contracts (7 P0/P1 tests)
- [ ] CoreWriter system contract (8 P0 tests)
- [ ] Asset transfer system (13 P0 tests)
- [ ] Security mechanisms (24 P0 tests)

**Deliverable:** Unified EVM/Core execution environment

### **Phase 4: Application Layer** (2-3 months)
- [ ] Market making vault (13 P0/P1/P2 tests)
- [ ] JSON-RPC server (Ethereum-compatible)
- [ ] Block explorer
- [ ] Trading frontend
- [ ] Developer tooling & SDKs

**Deliverable:** Complete user-facing platform

### **Phase 5: Security & Launch** (3-6 months)
- [ ] E2E integration tests (5 P0 tests)
- [ ] Stress & performance tests (4 P1 tests)
- [ ] Security testing (15+ P0 tests)
- [ ] 3+ external security audits
- [ ] 6+ months testnet operation
- [ ] Mainnet launch

**Deliverable:** Production-ready L1 DEX

**Total Estimated Timeline:** 15-25 months

---

## **Key Technical Decisions**

### **Why HotStuff-BFT?**
- **O(n) view-change complexity** vs O(n³) in PBFT
- **Optimistic responsiveness** vs fixed delays in Tendermint/Casper
- **Linear communication** enables larger validator sets
- **Efficient pipelining** (Chained HotStuff)

### **Why Threshold Signatures?**
- **Constant-size QCs** vs O(n) in naive implementations
- **O(1) verification** regardless of validator set size
- **Reduces network overhead** from O(n²) to O(n)

### **Why Dual Block Architecture?**
- **Fast confirmations** (1s for simple txs)
- **High capacity** (30M gas for complex txs)
- **No forced tradeoff** between speed and size

### **Why No Traditional Bridge?**
- **Unified state** eliminates bridge risk
- **Direct precompile access** for real-time reads
- **Simple event-based writes** via CoreWriter
- **Lower latency** than cross-chain messaging

---

## **Developer Resources**

### **For Consensus Developers**
Start with: `hyperbft_implementation_plan.md`
- Implement phases sequentially
- Use prototype for validation before production
- Focus on BLS threshold signatures early
- Pacemaker is critical for liveness

### **For Core DEX Developers**
Start with: `implementation_spec.md` → Milestone 2
- Order book performance is critical
- Use benchmarks to guide data structure choices
- Liquidation engine must be robust
- Oracle needs Byzantine fault tolerance

### **For Application Developers**
Start with: `evm_core_interaction.md`
- Understand atomicity limitations
- Handle "disappearing assets" window
- Account for delayed order actions
- Test with both small and big blocks

### **For Vault Developers**
Start with: `market_making_specification.md`
- Start with base Avellaneda-Stoikov
- Tune parameters via backtesting
- Implement gradual quote size reduction
- Monitor P&L variance carefully

---

## **Testing Strategy**

**Full Specification:** See [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) for 390+ concrete test cases

### **Consensus Layer**
1. **Unit Tests:** safeNode predicate, QC formation, signature combining (P0)
2. **Integration Tests:** View changes, leader rotation, 3-chain commits (P0)
3. **Byzantine Tests:** Equivocation, conflicting proposals, network partitions (P0)
4. **Performance Tests:** Throughput >10k TPS, latency <250ms (P1)

### **DEX Layer**
1. **Unit Tests:** Order matching, margin calculations, liquidation logic (P0)
2. **Integration Tests:** Multi-user scenarios, complex order types (P0)
3. **Fuzz Tests:** Random order sequences, price movements (P1)
4. **Performance Tests:** >100k orders/sec, <1ms per order (P1)

### **EVM/Core Bridge**
1. **Unit Tests:** Precompile reads, CoreWriter event parsing (P0)
2. **Integration Tests:** Transfer finalization, delayed actions (P0)
3. **Edge Case Tests:** Conflicting actions, big/small block interaction (P0)
4. **Performance Tests:** <100μs precompile calls, gas optimization (P1)

### **End-to-End**
1. **Integration Tests:** Complete user journeys, vault live testing (P0)
2. **Stress Tests:** Sustained load >10k TPS for 24 hours (P1)
3. **Security Tests:** Byzantine attacks, smart contract exploits (P0)
4. **Formal Verification:** TLA+ safety proofs, external audits (P0)

### **Launch Criteria**
- [ ] ALL P0 tests passing (390+ tests)
- [ ] 95%+ P1 tests passing
- [ ] 3+ external security audits
- [ ] 6+ months testnet operation
- [ ] >90% code coverage
- [ ] No critical bugs in last 3 months

---

## **Performance Targets**

| Metric | Target | Rationale |
|--------|--------|-----------|
| **Consensus Finality** | 1-3 blocks (83-250ms) | Three-phase commit |
| **Core Throughput** | 100k+ orders/sec | Optimized matching |
| **EVM Small Block** | 1s confirmation | Fast UX |
| **EVM Big Block** | 60s confirmation | Complex txs |
| **Validator Set** | 100+ nodes | Decentralization |
| **View-Change** | O(n) authenticators | Linear scaling |
| **TPS (sustained)** | 10k+ | Mixed workload |

---

## **Security Model**

### **Threat Model**
- **Byzantine Replicas:** Up to `f < n/3` validators can be malicious
- **Network Adversary:** Can delay messages up to `Δ` (after GST)
- **Smart Contract Exploits:** Malicious EVM contracts
- **Oracle Manipulation:** Coordinated price feed attacks
- **Front-running:** MEV extraction attempts

### **Mitigations**
- **Consensus:** HotStuff-BFT provides safety with `f < n/3`
- **Cryptography:** Threshold signatures prevent QC forgery
- **Rate Limits:** Position and exposure caps per asset
- **Circuit Breakers:** Volatility-based spread widening
- **Delayed Actions:** Order latency arbitrage prevention
- **Oracle Diversity:** Multi-validator price sourcing

---

## **Monitoring & Observability**

### **Key Metrics to Track**

#### Consensus
- Block production rate
- View change frequency
- QC formation time
- Validator participation rate

#### Core
- Order book depth
- Matching latency
- Liquidation success rate
- Oracle price deviation

#### EVM
- Gas usage (small vs big blocks)
- Precompile call frequency
- CoreWriter action success rate
- Transfer volume

#### Vault
- Sharpe ratio
- P&L volatility
- Inventory turnover
- Fill rate

---

## **Contributing**

This is an open-source project. Contributions welcome in areas:
- Core protocol implementation
- Testing infrastructure
- Documentation improvements
- Application examples
- Performance optimization

---

## **Reference Materials**

### **Academic Papers**
- HotStuff: BFT Consensus in the Lens of Blockchain (Yin et al., 2019)
- High-frequency trading in a limit order book (Avellaneda & Stoikov, 2006)

### **Inspiration Projects**
- Hyperliquid (production DEX with similar architecture)
- Ethereum (EVM compatibility)
- Tendermint/Cosmos (BFT consensus)

### **External Documentation**
- `references/hotstuff.md` - Original HotStuff paper
- `references/LimitOrderBook.md` - Avellaneda-Stoikov paper
- `references/GuideToPrecompilesArticle.md` - Hyperliquid precompile deep-dive
- `references/MMingVaultArticle.md` - Empirical vault analysis
- `references/hyperliquid_docs.md` - Hyperliquid documentation

---

## **FAQ**

**Q: Why not use an existing L1?**  
A: Performance requirements (100k+ orders/sec) exceed general-purpose chains. Custom L1 allows optimization for DEX workload.

**Q: Why not use a traditional bridge?**  
A: Bridges introduce security risks and latency. Unified state provides better UX and simpler developer model.

**Q: Why HotStuff over Tendermint?**  
A: HotStuff provides optimistic responsiveness (fast in normal conditions) while Tendermint requires fixed delays. Both have linear view-change.

**Q: Can this handle 1000+ validators?**  
A: Theoretically yes (O(n) communication), but practical testing needed. Start with 100 validators, scale gradually.

**Q: How does this compare to Hyperliquid?**  
A: Inspired by Hyperliquid but open-source. Implementation details may differ. Goal is educational and community-driven.

---

## **Changelog**

### Version 1.0 (Current)
- Initial comprehensive documentation
- Complete consensus specification
- EVM/Core interaction details
- Market making mathematical foundation
- Implementation roadmap

---

## **License**

MIT License - See LICENSE file for details

---

## **Contact & Community**

- GitHub: [openliquid repository]
- Discord: [community server]
- Twitter: [@openliquid]

---

**Last Updated:** October 2025  
**Version:** 1.0  
**Status:** Specification Phase

