# Table of Contents

- [Table of Contents](#table-of-contents)
- [About Hyperliquid](#about-hyperliquid)
  - [**What is Hyperliquid?**](#what-is-hyperliquid)
  - [Technical overview](#technical-overview)
- [Read Me - Builder Tools](#read-me---builder-tools)
- [Read Me - Support Guide](#read-me---support-guide)
  - [Recommendations](#recommendations)
  - [Important reminders](#important-reminders)
- [Onboarding](#onboarding)
- [HyperCore](#hypercore)
- [HyperEVM](#hyperevm)
  - [What can I do on the HyperEVM?](#what-can-i-do-on-the-hyperevm)
  - [Why build on the HyperEVM?](#why-build-on-the-hyperevm)
  - [What stage is the HyperEVM in?](#what-stage-is-the-hyperevm-in)
- [Hyperliquid Improvement Proposals (HIPs)](#hyperliquid-improvement-proposals-hips)
- [Trading](#trading)
- [Validators](#validators)
- [Risks](#risks)
  - [Smart contract risk](#smart-contract-risk)
  - [L1 risk](#l1-risk)
  - [Market liquidity risk](#market-liquidity-risk)
  - [Oracle manipulation risk](#oracle-manipulation-risk)
  - [Risk mitigation](#risk-mitigation)
- [API](#api)
- [Nodes](#nodes)

---


# About Hyperliquid

Source: https://hyperliquid.gitbook.io/hyperliquid-docs

## **What is Hyperliquid?**

Hyperliquid is a performant blockchain built with the vision of a fully onchain open financial system. Liquidity, user applications, and trading activity synergize on a unified platform that will ultimately house all of finance.

## Technical overview

Hyperliquid is a layer one blockchain (L1) written and optimized from first principles.

Hyperliquid uses a custom consensus algorithm called HyperBFT inspired by Hotstuff and its successors. Both the algorithm and networking stack are optimized from the ground up to support the unique demands of the L1.

Hyperliquid state execution is split into two broad components: HyperCore and the HyperEVM. HyperCore includes fully onchain perpetual futures and spot order books. Every order, cancel, trade, and liquidation happens transparently with one-block finality inherited from HyperBFT. HyperCore currently supports 200k orders / second, with throughput constantly improving as the node software is further optimized.

The HyperEVM brings the familiar general-purpose smart contract platform pioneered by Ethereum to the Hyperliquid blockchain. With the HyperEVM, the performant liquidity and financial primitives of HyperCore are available as permissionless building blocks for all users and builders. See the HyperEVM documentation section for more technical details.

![](https://hyperliquid.gitbook.io/hyperliquid-docs/~gitbook/image?url=https%3A%2F%2F2356094849-files.gitbook.io%2F%7E%2Ffiles%2Fv0%2Fb%2Fgitbook-x-prod.appspot.com%2Fo%2Fspaces%252FyUdp569E6w18GdfqlGvJ%252Fuploads%252FPgVwhFtylBB2kaxhQtZz%252FStack.png%3Falt%3Dmedia%26token%3Dfb5b86d0-95be-41bb-91d3-d08c8603c284&width=768&dpr=4&quality=100&sign=1bb66f75&sv=2)

[NextCore contributors](/hyperliquid-docs/about-hyperliquid/core-contributors)

Last updated 7 months ago

---


# Read Me - Builder Tools

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/builder-tools

This section lists tools that could be useful for building on Hyperliquid.

If you have developed any tools that are helpful for builders, please share by opening a ticket in [Discord](https://discord.gg/hyperliquid).

*Disclaimer: Listing of tools on this site is not an endorsement of the project. Tooling or resources listed may be developed by independent teams. It is important to DYOR before using any of them. This list is not exhaustive.*

[NextHyperEVM Tools](/hyperliquid-docs/builder-tools/hyperevm-tools)

Last updated 1 month ago

---


# Read Me - Support Guide

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/support

Read this page before you open a ticket. Tickets may take up to 48 hours for a response, with longer wait time on the weekend.

## Recommendations

1. Search this Support Guide for your issue and try the suggested solutions
2. Search the Docs to better understand concepts you have questions about
3. If you can’t find the answer to your question, seek help from the community.
4. Note that applications on the HyperEVM are from independent teams building on the Hyperliquid blockchain. Any questions related to a specific application should be directed toward the respective team’s support channels

## Important reminders

1. Never share your wallet, seed phrase, password, or private key with anyone or any website. If someone directly reaches out to you, assume they are a scammer.
2. There is no Hyperliquid app on any app store. Any app pretending to be the official Hyperliquid app is a scam. Do not download or interact with it.
3. Always check the full URL of any website you interact with. Scammers often use similar looking domains, e.g., “hyperliguid.xyz,” to prompt users to sign a malicious transaction.

[NextConnectivity issues](/hyperliquid-docs/support/faq/connectivity-issues)

Last updated 4 months ago

---


# Onboarding

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/onboarding

[How to start trading](/hyperliquid-docs/onboarding/how-to-start-trading)[How to use the HyperEVM](/hyperliquid-docs/onboarding/how-to-use-the-hyperevm)[How to stake HYPE](/hyperliquid-docs/onboarding/how-to-stake-hype)[Connect mobile via QR code](/hyperliquid-docs/onboarding/connect-mobile-via-qr-code)[Export your email wallet](/hyperliquid-docs/onboarding/export-your-email-wallet)[Testnet faucet](/hyperliquid-docs/onboarding/testnet-faucet)

[PreviousCore contributors](/hyperliquid-docs/about-hyperliquid/core-contributors)[NextHow to start trading](/hyperliquid-docs/onboarding/how-to-start-trading)

Last updated 1 year ago

---


# HyperCore

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/hypercore

[Overview](/hyperliquid-docs/hypercore/overview)[Bridge](/hyperliquid-docs/hypercore/bridge)[API servers](/hyperliquid-docs/hypercore/api-servers)[Clearinghouse](/hyperliquid-docs/hypercore/clearinghouse)[Oracle](/hyperliquid-docs/hypercore/oracle)[Order book](/hyperliquid-docs/hypercore/order-book)[Staking](/hyperliquid-docs/hypercore/staking)[Vaults](/hyperliquid-docs/hypercore/vaults)[Multi-sig](/hyperliquid-docs/hypercore/multi-sig)[Permissionless spot quote assets](/hyperliquid-docs/hypercore/permissionless-spot-quote-assets)[Aligned quote assets](/hyperliquid-docs/hypercore/aligned-quote-assets)

[PreviousTestnet faucet](/hyperliquid-docs/onboarding/testnet-faucet)[NextOverview](/hyperliquid-docs/hypercore/overview)

Last updated 1 year ago

---


# HyperEVM

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/hyperevm

The Hyperliquid blockchain features two key parts: HyperCore and HyperEVM. The HyperEVM is not a separate chain, but rather, secured by the same HyperBFT consensus as HyperCore. This lets the HyperEVM interact directly with parts of HyperCore, such as spot and perp order books.

## What can I do on the HyperEVM?

Explore directories of apps, tools, and more built by community members: [HypurrCo](https://www.hypurr.co/ecosystem-projects), [HL Eco](https://hl.eco/projects), [ASXN](https://data.asxn.xyz/dashboard/hyperliquid-ecosystem), and [Hyperliquid.wiki](https://hyperliquid.wiki/). Visit the [HyperEVM onboarding FAQ](/hyperliquid-docs/onboarding/how-to-use-the-hyperevm) for more questions.

## Why build on the HyperEVM?

Builders can plug into a mature, liquid, and performant onchain order books with HyperCore + HyperEVM on Hyperliquid. In addition, Hyperliquid has a captive audience of users who want to be at the forefront of financial change; they are excited to try new applications and conduct finance onchain. See the [HyperEVM developer section](/hyperliquid-docs/for-developers/hyperevm) for more technical details and [tools for HyperEVM builders](/hyperliquid-docs/hyperevm/tools-for-hyperevm-builders) for resources.

As one example, a project XYZ could deploy an ERC20 contract on the HyperEVM using standard EVM tooling and deploy a corresponding spot asset XYZ permissionlessly in the HyperCore spot auction. Once the HyperCore token and HyperEVM contract are linked, users can use the same XYZ token on HyperEVM applications and trade it with on the native spot order book. This has two key improvements compared to CEX listings: 1) The entire process is permissionless, with no behind-the-scenes negotiations for preferential treatment and 2) There is no bridging risk between HyperCore and HyperEVM as one unified state. Trading and building on the same chain is a 10x product improvement over CEXs.

As another example, a lending protocol could set up a pool contract that accepts token XYZ as collateral and lends out another token ABC to the borrower. To determine the liquidation threshold, the lending smart contract can read XYZ/ABC prices directly from the HyperCore order books using a read precompile. For a Solidity developer, this is as simple as calling a built-in function. Suppose the borrower's position requires liquidation. The lending smart contract can send orders directly swapping XYZ and ABC on the HyperCore order books using a write system contract. Again, this is a simple built-in function in Solidity. In a few lines of code, the lending protocol has implemented protocolized liquidations similar to how perps function on HyperCore. A theme of the HyperEVM is to abstract away the deep liquidity on HyperCore as a building block for arbitrary user applications.

## What stage is the HyperEVM in?

The HyperEVM is in the alpha stage. There are three reasons behind this gradual roll-out approach.

First, this stays true to Hyperliquid’s “no insiders” principle; everyone has equal access and starts on a level playing field. The tradeoff is that HyperEVM did not launch with the same tooling you might see on other chains, since no one is given a heads up nor paid for an integration or marketing. These short term obstacles are worth it to be a fair, credibly neutral platform in the long-run.

Second, a gradual roll-out is the safest way to upgrade a complex system doing billions of dollars of volume a day and protect against performance degradation or downtime.

Third, shipping an MVP and iterating live with user feedback allows development to adjust more nimbly. Testnets are useful for technical testing, but systems can only be hardened through real economic use.

As such, higher throughput and write system contracts are not live on mainnet yet, but will be in due time.

[PreviousAligned quote assets](/hyperliquid-docs/hypercore/aligned-quote-assets)[NextTools for HyperEVM builders](/hyperliquid-docs/hyperevm/tools-for-hyperevm-builders)

Last updated 2 months ago

---


# Hyperliquid Improvement Proposals (HIPs)

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/hyperliquid-improvement-proposals-hips

[HIP-1: Native token standard](/hyperliquid-docs/hyperliquid-improvement-proposals-hips/hip-1-native-token-standard)[HIP-2: Hyperliquidity](/hyperliquid-docs/hyperliquid-improvement-proposals-hips/hip-2-hyperliquidity)[HIP-3: Builder-deployed perpetuals](/hyperliquid-docs/hyperliquid-improvement-proposals-hips/hip-3-builder-deployed-perpetuals)[Frontend checks](/hyperliquid-docs/hyperliquid-improvement-proposals-hips/frontend-checks)

[PreviousTools for HyperEVM builders](/hyperliquid-docs/hyperevm/tools-for-hyperevm-builders)[NextHIP-1: Native token standard](/hyperliquid-docs/hyperliquid-improvement-proposals-hips/hip-1-native-token-standard)

---


# Trading

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/trading

[Perpetual assets](/hyperliquid-docs/trading/perpetual-assets)[Contract specifications](/hyperliquid-docs/trading/contract-specifications)[Margin tiers](/hyperliquid-docs/trading/margin-tiers)[Fees](/hyperliquid-docs/trading/fees)[Builder codes](/hyperliquid-docs/trading/builder-codes)[Order book](/hyperliquid-docs/trading/order-book)[Order types](/hyperliquid-docs/trading/order-types)[Take profit and stop loss orders (TP/SL)](/hyperliquid-docs/trading/take-profit-and-stop-loss-orders-tp-sl)[Margining](/hyperliquid-docs/trading/margining)[Liquidations](/hyperliquid-docs/trading/liquidations)[Entry price and pnl](/hyperliquid-docs/trading/entry-price-and-pnl)[Funding](/hyperliquid-docs/trading/funding)[Miscellaneous UI](/hyperliquid-docs/trading/miscellaneous-ui)[Auto-deleveraging](/hyperliquid-docs/trading/auto-deleveraging)[Robust price indices](/hyperliquid-docs/trading/robust-price-indices)[Self-trade prevention](/hyperliquid-docs/trading/self-trade-prevention)[Portfolio graphs](/hyperliquid-docs/trading/portfolio-graphs)[Hyperps](/hyperliquid-docs/trading/hyperps)[Market making](/hyperliquid-docs/trading/market-making)

[PreviousFrontend checks](/hyperliquid-docs/hyperliquid-improvement-proposals-hips/frontend-checks)[NextPerpetual assets](/hyperliquid-docs/trading/perpetual-assets)

Last updated 1 year ago

---


# Validators

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/validators

[Running a validator](/hyperliquid-docs/validators/running-a-validator)[Delegation program](/hyperliquid-docs/validators/delegation-program)

[PreviousMarket making](/hyperliquid-docs/trading/market-making)[NextRunning a validator](/hyperliquid-docs/validators/running-a-validator)

---


# Risks

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/risks

## Smart contract risk

The onchain perp DEX depends on the correctness and security of the Arbitrum bridge smart contracts. Bugs or vulnerabilities in the smart contracts could result in the loss of user funds.

## L1 risk

Hyperliquid runs on its own L1 which has not undergone as extensive testing and scrutiny as other established L1s like Ethereum. The network may experience downtime due to consensus or other issues.

## Market liquidity risk

As a relatively new protocol, there could be a potential risk of low liquidity, especially in the early stages. This can lead to significant price slippage for traders, negatively affecting the overall trading experience and possibly leading to substantial losses.

## Oracle manipulation risk

Hyperliquid relies on price oracles maintained by the validators to supply market data. If an oracle is compromised or manipulated for an extended period of time, the mark price could be effected and liquidations could occur before the price reverts to its fair value.

## Risk mitigation

There are additional measures in place to prevent oracle manipulation attacks on less liquid assets. One such restriction is open interest caps, which are based on a combination of liquidity, basis, and leverage in the system.

When an asset hits the open interest cap, no new positions can be opened. Furthermore, orders cannot rest further than 1% from the oracle price. HLP is exempt from these rules in order to continue quoting liquidity.

Note that this is not an exhaustive list of potential risks.

[PreviousHistorical data](/hyperliquid-docs/historical-data)[NextBug bounty program](/hyperliquid-docs/bug-bounty-program)

Last updated 7 months ago

---


# API

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/for-developers/api

Hyperliquid Python SDK: <https://github.com/hyperliquid-dex/hyperliquid-python-sdk>

Rust SDK (less maintained): <https://github.com/hyperliquid-dex/hyperliquid-rust-sdk>

Typescript SDKs written by members of the community:
<https://github.com/nktkas/hyperliquid>
<https://github.com/nomeida/hyperliquid>

CCXT also maintains integrations in multiple languages that conforms with the standard CCXT API: <https://docs.ccxt.com/#/exchanges/hyperliquid>

All example API calls use the Mainnet url (https://api.hyperliquid.xyz), but you can make the same requests against Testnet using the corresponding url (https://api.hyperliquid-testnet.xyz)

[PreviousBrand kit](/hyperliquid-docs/brand-kit)[NextNotation](/hyperliquid-docs/for-developers/api/notation)

Last updated 19 days ago

---


# Nodes

Source: https://hyperliquid.gitbook.io/hyperliquid-docs/hyperliquid-docs/for-developers/nodes

You can run a node by following the non-validator and validator nodes by following the steps in <https://github.com/hyperliquid-dex/node>.

[PreviousJSON-RPC](/hyperliquid-docs/for-developers/hyperevm/json-rpc)[NextL1 data schemas](/hyperliquid-docs/for-developers/nodes/l1-data-schemas)

Last updated 5 months ago

---
