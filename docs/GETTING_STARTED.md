# Getting Started with OpenLiquid Implementation

> **You are here:** Ready to begin implementation  
> **Status:** Comprehensive specification complete, implementation not started  
> **Next Step:** Choose your starting point below

---

## **📋 Quick Start Checklist**

Before writing any code, make sure you have:

- [x] Read the [README.md](README.md) - System overview
- [x] Read [implementation_spec.md](implementation_spec.md) - Architecture details
- [ ] **Choose your role** (see below)
- [ ] **Set up your development environment**
- [ ] **Pick your first task** from [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md)
- [ ] **Write tests first** using [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md)
- [ ] **Implement feature**
- [ ] **Verify all tests pass**

---

## **🎯 Choose Your Role**

### **1. Consensus Developer**
**You're building:** HotStuff-BFT consensus engine

**Start Here:**
1. Read [hyperbft_implementation_plan.md](hyperbft_implementation_plan.md)
2. Study the HotStuff paper: `references/hotstuff.md`
3. Review [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) Phase 1 tests
4. Begin with [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) Phase 1.1

**First Task:** Implement BLS threshold signatures
- **Tests to pass:** TEST_SPEC section 1.1.1 (13 P0 tests)
- **Success criteria:** Threshold signature generation, combination, verification
- **Estimated time:** 2-4 weeks

**Critical Reading:**
- Threshold signature scheme (implementation_spec.md lines 52-66)
- Pacemaker mechanism (hyperbft_implementation_plan.md lines 54-77)
- Byzantine fault tolerance requirements

---

### **2. Core DEX Developer**
**You're building:** On-chain order book and matching engine

**Prerequisites:** Wait for Phase 1 (Consensus) to complete

**Start Here:**
1. Read [implementation_spec.md](implementation_spec.md) Milestone 2
2. Review [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) Phase 2 tests
3. Study [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) Phase 2.1

**First Task:** Implement order book data structure
- **Tests to pass:** TEST_SPEC section 2.1.1 (18 P0 tests)
- **Success criteria:** Red-Black Tree, price-time priority, >100k orders/sec
- **Estimated time:** 4-6 weeks

**Critical Reading:**
- Order book structure (implementation_spec.md lines 90-92)
- Performance targets (README.md lines 276-285)
- Margin system (implementation_spec.md lines 94-97)

---

### **3. EVM Integration Developer**
**You're building:** Precompiles, CoreWriter, dual block architecture

**Prerequisites:** Wait for Phase 1 & 2 to complete

**Start Here:**
1. Read [evm_core_interaction.md](evm_core_interaction.md) - ALL OF IT (critical!)
2. Review [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) Phase 3 tests
3. Study [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) Phase 3

**First Task:** Implement dual EVM block architecture
- **Tests to pass:** TEST_SPEC section 3.1 (10 P0 tests)
- **Success criteria:** Small (1s, 2M gas) + Big (60s, 30M gas) blocks
- **Estimated time:** 3-4 weeks

**Critical Reading:**
- Dual block timing (evm_core_interaction.md section 1)
- Precompile behavior (evm_core_interaction.md section 3)
- CoreWriter atomicity (evm_core_interaction.md section 4)
- "Disappearing assets" window (evm_core_interaction.md section 5)

**⚠️ CRITICAL WARNINGS:**
- Precompiles read CURRENT state, not block-pinned state
- CoreWriter actions are NOT atomic with EVM transactions
- Transfers execute BEFORE CoreWriter actions (deterministic ordering)
- Order actions are DELAYED to prevent latency arbitrage

---

### **4. Application Developer**
**You're building:** Market making vault, trading frontend, SDKs

**Prerequisites:** Wait for Phase 1, 2, & 3 to complete

**Start Here:**
1. Read [market_making_specification.md](market_making_specification.md)
2. Review [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) Phase 4 tests
3. Study [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) Phase 4.1

**First Task:** Implement Avellaneda-Stoikov formulas
- **Tests to pass:** TEST_SPEC section 4.1.1 (4 P0 tests)
- **Success criteria:** Correct reservation price and optimal spread
- **Estimated time:** 2-3 weeks

**Critical Reading:**
- Reservation price formula (market_making_specification.md section 2)
- Multi-asset tiering (market_making_specification.md section 3)
- Risk management (market_making_specification.md section 4)

---

### **5. Security Auditor / QA Engineer**
**You're ensuring:** System security and correctness

**Prerequisites:** Can start anytime, intensifies in Phase 5

**Start Here:**
1. Read [RISKS.md](RISKS.md) - ALL 26 identified risks
2. Review [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) - ALL test cases
3. Study [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) Phase 5

**First Task:** Set up automated testing infrastructure
- **Deliverable:** CI/CD pipeline running all tests
- **Success criteria:** Tests run on every commit, coverage tracked
- **Estimated time:** 1-2 weeks

**Critical Reading:**
- Byzantine fault injection (TEST_SPEC section 1.2.3)
- Attack vectors (TEST_SPEC section 3.5)
- Security audit checklist (TEST_SPEC section 6.2)

---

## **🛠 Development Environment Setup**

### **Recommended Stack**

**Language:** Rust (consensus + core) or Go  
**Why:** Memory safety, performance, strong type system

**Alternative:** Could use Go for easier development, but sacrifice some performance

### **Required Tools**

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update stable

# Development tools
cargo install cargo-watch    # Auto-rebuild on changes
cargo install cargo-tarpaulin # Code coverage
cargo install cargo-audit     # Security audits

# Testing tools
cargo install cargo-nextest   # Faster test runner

# Database
brew install rocksdb         # State storage (macOS)
# or
sudo apt-get install librocksdb-dev  # Linux

# Networking
brew install libp2p          # P2P networking library
```

### **Project Structure (Recommended)**

```
openliquid/
├── consensus/              # Phase 1: HotStuff-BFT
│   ├── src/
│   │   ├── crypto/        # BLS, ECDSA, hashing
│   │   ├── hotstuff/      # Consensus logic
│   │   ├── pacemaker/     # Leader election, timeouts
│   │   └── network/       # P2P networking
│   └── tests/             # Consensus tests
│
├── core/                   # Phase 2: DEX engine
│   ├── src/
│   │   ├── orderbook/     # Red-Black Tree LOB
│   │   ├── matching/      # Matching engine
│   │   ├── margin/        # Clearinghouse
│   │   ├── liquidation/   # Liquidation engine
│   │   └── oracle/        # Price feeds
│   └── tests/             # Core DEX tests
│
├── evm/                    # Phase 3: EVM integration
│   ├── src/
│   │   ├── blocks/        # Dual block architecture
│   │   ├── precompiles/   # Core state reads
│   │   ├── corewriter/    # Write bridge
│   │   └── transfers/     # Asset transfers
│   └── tests/             # EVM integration tests
│
├── app/                    # Phase 4: Applications
│   ├── contracts/         # Solidity smart contracts
│   │   └── vault/         # Market making vault
│   ├── rpc/              # JSON-RPC server
│   ├── explorer/         # Block explorer
│   └── frontend/         # Trading UI
│
├── testutil/              # Testing utilities
│   ├── generators.rs      # Test data generation
│   ├── fixtures.rs        # Test fixtures
│   └── byzantine.rs       # Byzantine fault injection
│
└── docs/                  # You are here!
```

### **Initial Setup Commands**

```bash
# Clone repository (once it exists)
git clone https://github.com/openliquid/openliquid
cd openliquid

# Create workspace
cargo init --lib consensus
cargo init --lib core
cargo init --lib evm
cargo init --lib testutil

# Set up CI/CD
cp docs/.github/workflows/tests.yml .github/workflows/

# Run initial test suite (will fail, no implementation yet)
cargo test

# Set up development loop
cargo watch -x test
```

---

## **📖 Test-Driven Development (TDD) Approach**

### **Recommended Workflow**

```
┌─────────────────────────────────────────────────┐
│  1. Pick task from IMPLEMENTATION_CHECKLIST     │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  2. Read corresponding tests in TEST_SPEC       │
│     - Understand expected behavior              │
│     - Note all edge cases                       │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  3. Write tests FIRST (will fail initially)     │
│     - Copy test code from TEST_SPEC             │
│     - Adapt to your test framework              │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  4. Run tests (should FAIL)                     │
│     - cargo test                                │
│     - Verify failures are expected              │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  5. Implement feature                           │
│     - Write minimal code to pass tests          │
│     - Follow specifications exactly             │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  6. Run tests (should PASS)                     │
│     - All tests green ✓                         │
│     - Check code coverage                       │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  7. Refactor & optimize                         │
│     - Tests still passing                       │
│     - Improve performance/readability           │
└────────────────┬────────────────────────────────┘
                 │
┌────────────────▼────────────────────────────────┐
│  8. Update IMPLEMENTATION_CHECKLIST             │
│     - Mark task complete [x]                    │
│     - Update progress bars                      │
└────────────────┬────────────────────────────────┘
                 │
                 └────► Repeat for next task
```

### **Example: Implementing BLS Signatures**

```bash
# 1. Read TEST_SPECIFICATION.md section 1.1.1
less docs/TEST_SPECIFICATION.md  # Search for "1.1.1 BLS Threshold Signatures"

# 2. Create test file
cat > consensus/tests/test_bls.rs << 'EOF'
#[test]
fn test_bls_threshold_signature_generation() {
    // Setup: n=4 validators, f=1, k=3
    let validators = setup_validators(4);
    let message = "test block hash";
    
    // Test: Generate partial signatures from k validators
    let partial_sigs: Vec<_> = validators[0..3]
        .iter()
        .map(|v| v.tsign(message))
        .collect();
    
    // Test: Combine into threshold signature
    let combined_sig = tcombine(message, &partial_sigs);
    
    // Assert: Verification succeeds
    assert!(tverify(message, &combined_sig));
    assert_eq!(combined_sig.len(), 48); // Constant size
}
EOF

# 3. Run test (will fail)
cargo test test_bls_threshold_signature_generation
# Error: tcombine not found

# 4. Implement BLS signatures
vim consensus/src/crypto/bls.rs
# ... implement tsign, tcombine, tverify ...

# 5. Run test again
cargo test test_bls_threshold_signature_generation
# ✓ Test passed!

# 6. Update checklist
vim docs/IMPLEMENTATION_CHECKLIST.md
# Change:
# - [ ] BLS threshold signature implementation
# To:
# - [x] BLS threshold signature implementation
```

---

## **📊 Progress Tracking**

### **Daily Standup Template**

```markdown
## Daily Progress - [DATE]

### Yesterday
- Completed: [List tasks]
- Tests passing: X/Y
- Blockers: [Any issues]

### Today
- Working on: [Current task from CHECKLIST]
- Target: [Number of tests to pass]

### Blockers
- [None / List blockers]

### Updated Progress
Phase 1: [██░░░░░░░░] 20%
```

### **Weekly Review Template**

```markdown
## Weekly Review - Week of [DATE]

### Accomplishments
- [x] Task 1 (from CHECKLIST)
- [x] Task 2
- [ ] Task 3 (in progress)

### Tests Status
- P0 tests: X/Y passing
- P1 tests: X/Y passing
- Code coverage: X%

### Blockers Resolved
- [List resolved issues]

### Next Week Goals
- [ ] Task 4
- [ ] Task 5

### Phase Progress Update
Phase 1: [████░░░░░░] 40% (+20% this week)
```

---

## **🎓 Learning Resources**

### **Before You Start**

**Must Read:**
1. HotStuff paper (`references/hotstuff.md`)
2. Avellaneda-Stoikov paper (`references/LimitOrderBook.md`)
3. Hyperliquid precompile guide (`references/GuideToPrecompilesArticle.md`)

**Videos (if available):**
- HotStuff presentations on YouTube
- BFT consensus explained
- EVM internals

### **Reference Implementations**

**Consensus:**
- LibraBFT (now Aptos) - Rust implementation of HotStuff
- Tendermint Core - Similar BFT consensus

**Order Books:**
- Look at existing DEX order book implementations
- Study high-frequency trading systems

**EVM:**
- revm - Rust EVM implementation
- Ethereum's go-ethereum

---

## **⚠️ Common Pitfalls**

### **Consensus (Phase 1)**
- ❌ Forgetting to update locked_qc on pre-commit
- ❌ Not implementing exponential backoff in Pacemaker
- ❌ Assuming synchronous message delivery
- ✅ Always test Byzantine fault scenarios

### **Core DEX (Phase 2)**
- ❌ Not enforcing price-time priority strictly
- ❌ Liquidations triggering on stale prices
- ❌ Oracle manipulation through single data source
- ✅ Benchmark performance continuously

### **EVM Integration (Phase 3)**
- ❌ Assuming precompiles read block-pinned state (they don't!)
- ❌ Assuming CoreWriter actions are atomic (they're not!)
- ❌ Not handling "disappearing assets" window
- ❌ Processing CoreWriter before transfers
- ✅ Read evm_core_interaction.md THREE TIMES

### **Testing**
- ❌ Writing implementation before tests
- ❌ Skipping P0 tests
- ❌ Not testing edge cases
- ✅ Aim for >90% code coverage

---

## **🚀 Launch Criteria (Don't Skip!)**

Before mainnet launch, you MUST have:

- [ ] ALL P0 tests passing (390+ tests)
- [ ] 95%+ P1 tests passing
- [ ] 3+ external security audits complete
- [ ] 6+ months stable testnet operation
- [ ] No critical bugs in last 3 months
- [ ] >90% code coverage
- [ ] Formal verification (TLA+) complete
- [ ] >50 validators committed
- [ ] Performance benchmarks met
- [ ] Documentation complete
- [ ] Community engaged

**See:** [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md) Mainnet Launch Criteria

---

## **🆘 Need Help?**

### **Documentation Questions**
1. Check the [README.md](README.md) index
2. Use search: `grep -r "keyword" docs/`
3. Review [peer_review.md](peer_review.md) for common clarifications

### **Implementation Questions**
1. Check [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md) for expected behavior
2. Review [RISKS.md](RISKS.md) for known issues
3. Look at implementation notes in specifications

### **Architecture Questions**
1. [implementation_spec.md](implementation_spec.md) - High-level overview
2. [hyperbft_implementation_plan.md](hyperbft_implementation_plan.md) - Consensus details
3. [evm_core_interaction.md](evm_core_interaction.md) - EVM/Core bridge
4. [market_making_specification.md](market_making_specification.md) - MM vault

---

## **📅 Suggested Timeline**

### **Month 1-2: Setup & Phase 1 Start**
- Set up development environment
- Implement cryptography library (BLS, ECDSA, hashing)
- Begin consensus data structures

### **Month 3-6: Phase 1 Complete**
- Finish HotStuff-BFT implementation
- P2P networking operational
- 4-node testnet running
- ALL P0 consensus tests passing

### **Month 7-12: Phase 2**
- Order book implementation
- Matching engine optimization
- Margin & liquidation systems
- Oracle module

### **Month 13-16: Phase 3**
- EVM integration
- Dual block architecture
- Precompiles & CoreWriter
- Asset transfer system

### **Month 17-19: Phase 4**
- Market making vault
- Trading frontend
- SDKs & tooling

### **Month 20-25: Phase 5**
- Security audits
- Testnet deployment
- Performance optimization
- Mainnet launch preparation

**Total: 15-25 months** (as estimated in README.md)

---

## **✅ Your First Day Checklist**

- [ ] Read this entire document
- [ ] Read [README.md](README.md)
- [ ] Choose your role (above)
- [ ] Read role-specific documentation
- [ ] Set up development environment
- [ ] Clone/create repository
- [ ] Set up test framework
- [ ] Pick first task from [IMPLEMENTATION_CHECKLIST.md](IMPLEMENTATION_CHECKLIST.md)
- [ ] Write first test from [TEST_SPECIFICATION.md](TEST_SPECIFICATION.md)
- [ ] Watch test fail
- [ ] Begin implementation!

---

## **🎉 Ready to Start?**

```bash
# Your first command
cd openliquid
git checkout -b feature/bls-signatures
cargo test  # Watch them fail (that's good!)
```

**Remember:** 
- Tests first, code second
- Read specifications carefully
- Update checklist regularly
- Ask for help when stuck
- Enjoy building the future of DeFi!

---

**Last Updated:** October 2025  
**Status:** Ready for implementation  
**Good luck!** 🚀

