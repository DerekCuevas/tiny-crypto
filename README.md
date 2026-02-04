# Tiny Crypto

A minimal blockchain-based cryptocurrency built entirely for learning purposes in Rust. It is roughly modeled after a extremely simplified Bitcoin, focusing on foundational blockchain elements to create a small and functional digital currency. 

## Development

[Install Rust](https://rust-lang.org/tools/install/)

**Tech stack**: Standard crypto crates (`secp256k1`, `sha2`, `ripemd`, `bincode`, `rs_merkle`).

**Testing** 

```
cargo test
```

### Organization

- `src/block.rs`: block header, block structure, mining, and validation.
- `src/block_manager.rs`: block storage, orphan handling, and node tracking.
- `src/chain.rs`: blockchain node graph, chain work, and UTXO rebuilds.
- `src/constants.rs`: protocol constants (reward, halving, size limits).
- `src/crypto.rs`: hashing, keypairs, signatures, addresses, merkle tree.
- `src/mem_pool.rs`: pending transaction pool with validation.
- `src/node.rs`: node state, block/tx handling, and chain selection.
- `src/transaction.rs`: transaction model, signing, IDs, and coinbase.
- `src/utxo_set.rs`: Unspent transaction output tracking and transaction validation.

### Planned

- Peer to peer network (with [libp2p](https://libp2p.io))
- CLI for interaction (viewing chain state, submitting transactions)
