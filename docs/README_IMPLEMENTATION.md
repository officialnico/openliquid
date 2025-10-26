# OpenLiquid - Rust Implementation

This is the Rust implementation of OpenLiquid, following the specification in `docs/`.

## Project Status

**Phase 1.1: Cryptography Layer** - ✅ IN PROGRESS
- [x] BLS threshold signatures (tsign, tcombine, tverify)
- [x] Hash functions (SHA-256 and BLAKE3)
- [x] ECDSA signatures (secp256k1)
- [ ] Key generation ceremony
- [ ] HSM support

**Phase 1.2-1.4** - 🔜 NEXT UP
- [ ] HotStuff-BFT consensus
- [ ] P2P networking
- [ ] State storage

See `docs/IMPLEMENTATION_CHECKLIST.md` for full roadmap.

## Quick Start

### Prerequisites

- Rust 1.70+ (`rustup update stable`)
- Build essentials for your platform

### Build

```bash
cargo build
```

### Test

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_bls_threshold_signature_generation
```

### Benchmarks

```bash
cargo bench
```

## Project Structure

```
openliquid/
├── consensus/          # Phase 1: HotStuff-BFT consensus
│   ├── src/
│   │   ├── crypto/     # ✅ BLS, ECDSA, hashing
│   │   ├── hotstuff/   # 🔜 Consensus logic
│   │   ├── pacemaker/  # 🔜 Leader election, timeouts
│   │   ├── network/    # 🔜 P2P networking
│   │   └── storage/    # 🔜 RocksDB integration
│   └── tests/          # Consensus tests
│
├── core/               # Phase 2: DEX engine (TODO)
├── evm/                # Phase 3: EVM integration (TODO)
├── testutil/           # Testing utilities
└── docs/               # 📚 Comprehensive specifications
```

## Test Results

### Phase 1.1 - Cryptography Tests

All P0 tests passing:

- ✅ `test_bls_threshold_signature_generation` - Basic k-of-n signing
- ✅ `test_bls_insufficient_signatures_fails` - Security validation
- ✅ `test_bls_adversary_cannot_forge` - Byzantine resistance
- ✅ `test_bls_verification_constant_time` - O(1) verification
- ✅ `test_bls_constant_signature_size` - 48-byte signatures
- ✅ `test_hash_collision_resistance` - No collisions in 100k samples
- ✅ `test_hash_performance` - Hash 1MB < 10ms
- ✅ `test_hash_consistency` - Deterministic output

### Next Steps

1. Complete Phase 1.1 (key management, HSM support)
2. Begin Phase 1.2 (HotStuff-BFT data structures)
3. Implement safeNode predicate
4. Build three-chain commit logic

See `docs/GETTING_STARTED.md` for detailed guidance.

## Development

### Running Tests

Follow TDD approach from `docs/GETTING_STARTED.md`:

1. Pick task from `docs/IMPLEMENTATION_CHECKLIST.md`
2. Read test specification in `docs/TEST_SPECIFICATION.md`
3. Write test first (will fail)
4. Implement feature
5. Run test (should pass)
6. Update checklist

### Code Coverage

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### Documentation

```bash
cargo doc --open
```

## Performance Targets

| Metric | Target | Status |
|--------|--------|--------|
| BLS signature size | 48 bytes | ✅ |
| BLS verification | O(1) | ✅ |
| Hash 1MB block | < 10ms | ✅ |
| Consensus TPS | > 10k | 🔜 |
| Order matching | > 100k ops/s | 🔜 |

## Contributing

1. Read specifications in `docs/`
2. Follow TDD workflow
3. Ensure all tests pass
4. Update progress in `docs/IMPLEMENTATION_CHECKLIST.md`

## License

MIT License - See LICENSE file

## Resources

- **Specifications**: See `docs/` directory
- **HotStuff Paper**: `docs/references/hotstuff.md`
- **Test Spec**: `docs/TEST_SPECIFICATION.md`
- **Getting Started**: `docs/GETTING_STARTED.md`

