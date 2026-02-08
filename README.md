# Tiny Crypto

A minimal blockchain-based cryptocurrency built entirely for learning purposes in Rust. It is roughly modeled after a extremely simplified Bitcoin, focusing on foundational blockchain elements to create a small and functional digital currency. 

## Development

[Install Rust](https://rust-lang.org/tools/install/)

**Tech stack**: Standard crypto crates (`secp256k1`, `sha2`, `ripemd`, `bincode`, `rs_merkle`).

**Testing** 

```
cargo test
```

## Architecture

The codebase follows a bottom-up layered design:

**Primitives**
- `crypto.rs` — Core cryptographic building blocks: SHA-256d hashing, secp256k1 keypair generation/signing/verification, Bitcoin-style Base58Check addresses (SHA-256 + RIPEMD-160), and Merkle trees.
- `constants.rs` — Protocol parameters: 50-coin genesis reward, 210k-block halving interval, 1000 block size limit.

**Transactions**
- `transaction.rs` — UTXO-based transaction model. Each transaction has a single input (either a coinbase for mining rewards, or a reference to a previous output) and multiple outputs. Transactions are signed with ECDSA and identified by their double-SHA-256 hash.

**Blocks**
- `block.rs` — Block structure with header (prev hash, merkle root, timestamp, difficulty, nonce). Implements naive proof-of-work mining and validation (hash meets difficulty target, merkle root matches, coinbase reward is correct, no duplicate txs, signatures valid).

**Chain Management**
- `utxo_set.rs` — Tracks unspent transaction outputs. Validates that inputs reference real UTXOs, are signed by the output owner, and output values match.
- `chain.rs` — Linked list of BlockchainNodes with cumulative work calculation (for heaviest-chain selection). Can rebuild the UTXO set from the full chain.
- `block_manager.rs` — Stores blocks/nodes by hash. Handles orphan blocks (blocks whose parent hasn't arrived yet).
- `mem_pool.rs` — Holds pending transactions validated against a projected UTXO set.

**Node State**
- `node.rs` — Ties everything together. NodeState handles incoming blocks (validate, store, reorg if heavier chain) and transactions (validate, add to mempool). Node wraps state with a keypair and can create/mine new blocks from the mempool.

**Network**
- **Planned** Peer to peer networking (with [libp2p](https://libp2p.io))

**CLI** 
- (`main.rs`) 
- **Planned** Subcommands for interaction with the node/chain (viewing chain state, submitting transactions)

## Key Design Decisions / Simplifications

- **Single input per transaction** — simplified vs. Bitcoin's multi-input model
- **UTXO rebuild on reorg** — the entire UTXO set is rebuilt from scratch when the active chain tip changes
- **Signing/Scripting** P2PKH only, no dynamic scripting functionality supported.
