When I first wrote about the Hyperliquid Precompiles, the tech stack was still in its infancy — limited in functionality and only available on testnet for feedback. A lot has changed in what feels like a short time since that original piece. Today we have new terminology, more detailed offical documentation, a mainnet deployment, expanded functionality, a growing community of builders, and, most importantly, protocols now running entirely on top of it.

This article aims to provide a more in-depth guide to how the HyperEVM and HyperCore interact — both to help builders understand how to write their code effectively, and just as importantly, to give auditors a clear framework for identifying potential risks.

This article will intentionally duplicate some content from the original piece so that readers don’t need to refer back to what has now become an outdated resource. As with any fast-moving stack, the details here may not be 100% correct, and if errors are identified, we’ll update them accordingly.

Block Sequencing
Conceptually, the Hyperliquid stack is built around two distinct block types: Core Blocks and EVM Blocks. Within the EVM category, there are in fact two different forms — small blocks and big blocks.

Core Blocks are produced at very fast intervals — at the time of writing, there are roughly 12 blocks per second. These blocks execute a fixed set of well-defined transactions such as creating a limit order, depositing into a vault, or delegating stake to a validator, etc.

EVM Blocks, by contrast, are general-purpose. What can be achieved within an EVM block is limited only by the block’s gas capacity, making them the natural home for complex smart contract logic and more flexible applications.

It’s important to build the right mental model of how these blocks fit together. The key point is that Core and EVM blocks run sequentially in separate execution environments, yet operate under the same consensus mechanism and share the same global state.

Press enter or click to view image in full size

In practice, they aren’t even truly separate blocks — though we describe them that way for clarity. As we dive deeper, this distinction will become clearer and the reasoning behind the architecture more intuitive.

The Dual EVM block architecture
This brings us to one of the most unique design choices in Hyperliquid: the dual EVM block architecture.

The primary motivation behind the dual-block architecture is to decouple block speed and block size when allocating throughput improvements. Users want faster blocks for lower time to confirmation. Builders want larger blocks to include larger transactions such as more complex contract deployments. Instead of a forced tradeoff, the dual-block system will allow simultaneous improvement along both axes.

Small EVM blocks are produced every 1 second, each with a gas limit of 2 million and Big EVM blocks are produced every 60 seconds, each with a gas limit of 30 million. This means the chain maintains its fast cadence of small blocks while periodically providing the much larger capacity of a big block.

Another imporant characteristic to note is that given the 60-second interval also falls on a 1-second boundary, whenever a big block is produced, a small block is produced just prior to it.

Press enter or click to view image in full size

The big block and small block will have the same block.timestamp but an increasing block number.

Blocks inside Blocks
Conceptually, the blocks are separate and produced in sequence. In reality, however, EVM blocks are produced within the scope of a Core block and are only executed once the Core transactions have completed.

This subtle detail is critical: it explains how Precompiles and the CoreWriter fit into the picture and how state is read and written during block production.

If we could look inside a Core block it would look something like this.

Press enter or click to view image in full size

And as expected, when a small block and big block are produced together, they will be produced inside the same Core block. Note here that the small block is always produced prior to the big block.

Press enter or click to view image in full size

Precompiles
Precompiled contracts, often just refered to as Precompiles, offer a way for an Ethereum Virtual Machine (EVM) implementation to provide access to native functions. They behave like smart contracts and have a well known address, however, that’s where the similarites end. What happens when the Precompiles are called is untirely up to the EVM implementation.

Lets take a look at an example of this would work to read a users perp positions on the L1.

// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

contract PositionReader {
  address constant PRECOMPILE_ADDRESS = 0x0000000000000000000000000000000000000800;

  struct Position {
    int64 szi;
    uint32 leverage;
    uint64 entryNtl;
  }

  function readPosition(address user, uint16 perp) external view returns (Position memory) {
    (bool success, bytes memory result) = PRECOMPILE_ADDRESS.staticcall(abi.encode(user, perp));
    require(success, "readPosition call failed");

    return abi.decode(result, (Position));
  }
}
Now it becomes trivial for any smart contract to read a perps position and given the sequential nature in which the blocks are executed, there is a guarantee that the value read will be up to date.

Lets take a look at this conceptually.

Press enter or click to view image in full size

As highlighted in the diagram, whenever an EVM block is produced, the Precompiles always reads from the latest Core block state — in other words, the state of the Core block they are called within. This rule applies to both small blocks and big blocks, ensuring that all Precompile reads are grounded in the most recent Core state.

The special case here is that, while the big block reads from the Core block it is produced within, the state it accesses may already have been mutated by the execution of the preceding small block. This will be discussed in more detail later.

An important distinction is that Precompiles always read from the current state, and the EVM block is not strictly pinned to the Core block in which it was produced. Instead, whenever an EVM call touches a Precompile, it will always read from the current Core state.

This is another critical point to understand, and will become much clearer when illustrated with a concrete example.

Here we have a basic contract which returns the current L1/Core block number along side the EVM block number and timestamp.

Press enter or click to view image in full size

Now we can call this repeatably which should allow for multiple calls between blocks.

Press enter or click to view image in full size

And from the output below we can see that the L1 block number increases on every call, while the EVM block number and timestamp only advance when a new block is mined. This demonstrates that the EVM consistently accesses the latest Core block, even though the Core block is produced after the EVM block.

Press enter or click to view image in full size

CoreWriter
The CoreWriter enables smart contracts on the HyperEVM to create transactions directly on HyperCore. This mechanism is both elegant in its simplicity and powerful in its capability.

The CoreWriter is deployed at a well-known, fixed address (0x333…333) and exposes the following interface for interaction.

Press enter or click to view image in full size

This gets even more interesting when we look at the CoreWriter implementation.

Press enter or click to view image in full size

The implementation of the CoreWriter itself is intentionally minimal. It does little more than emit the RawAction event, yet this behavior provides a critical insight into its internal design.

What is the underlying of a CoreWriter?
To begin, it is useful to adapt our mental model by considering the sequence of blocks. When an EVM block is produced, any CoreWriter actions included in that block become visible in the subsequent Core block, subject to a few notable exceptions.

Press enter or click to view image in full size

Digging a little deeper, lets see how that looks in conceptually within an EVM block.

Press enter or click to view image in full size

When a transaction in an EVM block invokes the CoreWriter, the resulting actions are not executed immediately. Instead, these actions are queued and processed only after the EVM block has been finalized. The node observes the emitted log events and appends them to a queue for subsequent execution.

We can now extrapolate that conceptually as the following.

Press enter or click to view image in full size

Once the EVM block has finalized, the queued CoreWriter actions are executed, all within the scope of the original Core block. This ensures that the relationship between EVM execution and Core state remains consistent and deterministic.

In cases where a big block and small block are produced together, the CoreWriter actions raised in the small block are executed first, followed by the big block and any CoreWriter actions it includes. As noted earlier, this means Precompile calls inside the big block do not necessarily read from the initial Core block state, since that state may already have been mutated by the CoreWriter actions executed in the small block.

Press enter or click to view image in full size

There are notable exceptions to the general execution model, specifically with order actions and vault transfers. Unlike standard CoreWriter actions, these are not executed within the same Core block in which they are raised. Instead, they are intentionally delayed on-chain for a short period before being processed. This design choice prevents potential latency advantages that could arise from the HyperEVM bypassing the L1 mempool.

In cases where actions are delayed, it is only the action itself that is deferred — not the transaction that would result. This distinction can be illustrated through the example of a vault transfer.

Suppose an account holds a balance of 100 USD in its perp balance. A CoreWriter action is raised to transfer that 100 USD into a vault. During the next EVM block, the account’s perp balance will still report as 100 USD as the action has yet to be converted into a transaction. However, at some later point, the action is converted into a Core transaction and the funds are deposited into the vault.

This delay introduces the possibility of conflicting actions. For instance, in a subsequent EVM block, another CoreWriter action might be executed to transfer the same 100 USD from the perp balance into the spot balance. By the time the delayed vault transfer is processed, the perp balance would no longer contain sufficient funds, causing the vault transfer to fail.

To summarize the understanding here is that actions are intents, not immediate state changes.

Atomicity and the lack of it
Atomicity in the context of a blockchain transaction is the ability to retain the integrity of the blockchain state. If something fails in the transaction, the entire transaction is rolled back as if it was never executed. Atomicity is also a very powerful tool for a developer as it removes the complexity that exists when dealing with operations that may only partially succeed.

The atomicity you gain with the CoreWriter is limited to the guarantee that once you raise an action, it will be enqueued for processing. What you do not get, however, is a guarantee that the action itself will succeed. For example, if you submit a spot order through the CoreWriter and that order ultimately fails to fill, the failure will not revert the originating EVM transaction.

This distinction is important for smart contract developers to understand, since it means there are cases where the on-chain state you expect may not match the eventual Core outcome. That does not make the system unreliable, but it does mean you need to account for these nuances in your design. Fortunately, there are well-established design patterns that allow you to safely build within this model — you simply need to think more carefully about how to structure your solution so it remains robust even when Core actions do not resolve exactly as expected.

Asset Transfers
Transferring assets between HyperCore and the HyperEVM is a distinct interaction type. While in crypto this might commonly be referred to as “bridging,” in reality it isn’t a bridge at all. Instead, it is simply a series of state updates that synchronize balances from the EVM state to the Core state, all executed within the scope of a single Core block.

With the new found knowledge of how the blocks exists, the assets transfers are alot easier to understand as its a similar process.

A standard ERC-20 transfer from a user’s HyperEVM account to that token’s associated System Address (as described in detail in the official documentation) will decrement the balance on the HyperEVM and then credit the user’s Spot Balance on HyperCore.

Just as the node observes RawAction events from the CoreWriter, it also monitors ERC-20 Transfer events emitted by the System Addresses of all linked tokens. These Transfer events are queued in the same way CoreWriter actions are queued.

When the EVM block finishes execution, the processing order is deterministic:

First, all queued Transfer events are applied, updating balances from the EVM into Core.
Then, any pending CoreWriter actions are executed.
Press enter or click to view image in full size

The special case for HYPE
When sending HYPE, the native gas token of the HyperEVM, the transaction does not generate any standard event logs as it is a basic transaction. To address this, HyperEVM uses a dedicated System Address: 0x2222222222222222222222222222222222222222.

This contract exposes a payable function which, when invoked, emits a Receive event. That event acts as a substitute for an ERC-20 Transfer log, ensuring the node has a consistent signal to track value moving from the EVM into Core.

Apart from this event-handling workaround, the flow is identical to any other asset transfer.

An important property of this design is that transfers from the HyperEVM into HyperCore are guaranteed to finalize by the very next Core block. In other words, once the EVM block completes, the queued transfer events are always processed in the same Core block.

Going the other way
Most interactions we’ve described so far flow from the HyperEVM into HyperCore. However, asset transfers can also originate from HyperCore back to the EVM.

When an asset is sent via a Spot Send action on HyperCore, the transfer is first queued within Core and held until the next EVM block is produced. At that point the queued Spot Send transfers are executed before the EVM block itself is processed. Internally this is simply a call to the token’s native transfer method, which means that a transfer originating from HyperCore appears exactly the same as any other token transfer on the EVM. This design keeps balances in sync across both systems and ensures that contracts and indexers can track Core-originated transfers just as reliably as transfers that originate on the EVM.

Press enter or click to view image in full size

Thats great, but why have my assets dissapeared?
So as we’ve seen, transferring assets between the HyperEVM and HyperCore might look like a bridge, but in reality it is a deterministic and reliable state transfer. This raises an important question: why can it sometimes seem like your assets have disappeared? The answer is that they never actually disappear. What happens instead is that during the brief moment when a transfer has been initiated but not yet finalized on the other side, the assets cannot be fully accounted for within the system.

Lets simplify this with an example.

Consider the case where a smart contract sends 1 BTC from its HyperEVM balance to the token’s associated system address (0x200…00N). At this point, the smart contract’s balance no longer reflects the 1 BTC, and instead the system address shows the increased balance. Since this is a shared address, there is no way to determine how much of that balance specifically originated from the contract. This is expected behavior and functions just like any other ERC-20 transfer.

However, as we have already seen, token transfers are not processed into the Core state until after the EVM block has completed. This means that if, within the same EVM block — whether in the same transaction or a subsequent transaction — you query the spot balance via a precompile, the 1 BTC update will not yet appear. The assets are not lost, but during this brief window they cannot be directly accounted for because they are in the queue awaiting settlement into Core.

The simple solution to this is to track those assets internally and assign them against the L1 block number.

Press enter or click to view image in full size

Press enter or click to view image in full size

Wrapping Up
Though the use of Precompiles and CoreWriter, the Hyperliquid team have provided a way in which smart contracts running on the HyperEVM can read state and interact with HyperCore, effectively giving smart contracts access to the perps and spot liquidity on the L1. Imagine if Binance allowed smart contract developers to deploy on the Binance order books… this is what Hyperliquid enables. This is going to enable a new era of DeFi primitives and strategies to be built and live completely on-chain.