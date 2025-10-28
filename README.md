# OpenLiquid

An open-source, high-performance decentralized exchange (DEX) with on-chain order books, inspired by Hyperliquid.

## What is OpenLiquid?

OpenLiquid is a custom Layer 1 blockchain that runs two execution engines under a single HotStuff-BFT consensus:

- **OpenCore**: High-performance order book DEX for perpetuals and spot trading
- **OpenEVM**: EVM-compatible smart contract environment with direct DEX integration via precompiles

This unified architecture eliminates traditional bridge risks while achieving >100k orders/sec with sub-second finality.

## Project Status

🚧 **Early Development** - Currently in specification and initial implementation phase.

## Getting Started

### For Contributors

1. **Choose your area**: Consensus, Core DEX, EVM Integration, Applications, or QA
2. **Read the docs**: Check out `/docs/GETTING_STARTED.md` for detailed setup instructions
3. **Pick a task**: See `/docs/IMPLEMENTATION_CHECKLIST.md` for active work items
4. **Write tests first**: We follow TDD - see test specifications in `/docs/TEST_SPECIFICATION.md`

### Documentation

- **Start here**: [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md) - New contributor guide
- **Architecture**: [`docs/implementation_spec.md`](docs/implementation_spec.md) - High-level system design
- **Implementation**: [`docs/IMPLEMENTATION_CHECKLIST.md`](docs/IMPLEMENTATION_CHECKLIST.md) - Task tracking
- **Full docs**: [`docs/README.md`](docs/README.md) - Complete documentation index

### Building the Project

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Run specific crate tests
cargo test -p consensus
cargo test -p core
cargo test -p evm
```

## Contributing

We welcome contributions! Areas of focus:

- **Consensus Layer**: HotStuff-BFT implementation, networking, cryptography
- **Core DEX**: Order book, matching engine, clearing, liquidations
- **EVM Integration**: Precompiles, state synchronization, CoreWriter
- **Applications**: Market making vaults, trading interfaces, tooling
- **Testing & QA**: Test infrastructure, fuzzing, security analysis

Before contributing, please:

1. Review [`docs/GETTING_STARTED.md`](docs/GETTING_STARTED.md)
2. Check existing issues and PRs
3. Follow the TDD workflow
4. Ensure all tests pass before submitting

## Architecture Overview

```
┌─────────────────────────────────────────┐
│         OpenLiquid L1 Blockchain        │
│     (HotStuff-BFT Consensus)           │
├─────────────────────────────────────────┤
│                                         │
│  ┌──────────┐       ┌──────────┐      │
│  │ OpenCore │◄─────►│ OpenEVM  │      │
│  │  (DEX)   │       │ (Smart   │      │
│  │          │       │ Contracts)│      │
│  └──────────┘       └──────────┘      │
│         └──────┬──────┘                │
│                ↓                        │
│         Unified State                   │
└─────────────────────────────────────────┘
```

## License

MIT License - See [LICENSE](LICENSE) for details.

## Community

- GitHub Issues: Bug reports and feature requests
- Discussions: Architecture discussions and Q&A

---

**Built with Rust** 🦀 | **Powered by HotStuff-BFT** ⚡

