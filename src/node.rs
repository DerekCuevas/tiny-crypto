use std::collections::HashMap;

use crate::{
    block::Block,
    chain::Blockchain,
    crypto::{Hash, KeyPair},
    mem_pool::MemPool,
    transaction::Transaction,
    utxo_set::UTXOSet,
};
use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct NodeState {
    pub store: HashMap<Hash, Block>,
    pub chain: Blockchain,
    pub uxto_set: UTXOSet,
    pub mem_pool: MemPool,
    pub orphan_blocks: HashMap<Hash, Block>,
}

impl NodeState {
    pub fn tail_block(&self) -> Option<&Block> {
        self.chain
            .tail()
            .and_then(|node| self.store.get(&node.header.hash().ok()?))
    }

    pub fn build_uxto_set(&mut self) -> Result<()> {
        self.uxto_set = UTXOSet::default();

        for node in self.chain.nodes.values() {
            if let Some(block) = self.store.get(&node.header.hash()?) {
                for tx in &block.transactions {
                    self.uxto_set.update(tx)?;
                }
            }
        }

        Ok(())
    }

    pub fn add_block(&mut self, block: Block) -> Result<()> {
        let hash = block.header.hash()?;

        if self.store.contains_key(&hash) {
            return Ok(());
        }

        block.validate()?;

        let previous_block = self.store.get(&block.header.previous_block_hash);

        if previous_block.is_none() && !self.chain.is_empty() {
            self.orphan_blocks.insert(hash, block);
            return Ok(());
        }

        if let Some(previous_block) = previous_block {
            if !self.chain.is_tail_block(&previous_block)? {
                todo!("reorg chain");
            }
        }

        self.store.insert(hash, block.clone());
        self.chain.append_block(&block)?;
        self.build_uxto_set()?;

        Ok(())
    }

    pub fn add_transaction(&mut self, transaction: Transaction) -> Result<()> {
        self.mem_pool.add(&self.uxto_set, transaction)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    NewBlock(Block),
    NewTransaction(Transaction),
}

#[derive(Clone)]
pub struct NodeConfig {
    pub keypair: KeyPair,
}

#[derive(Clone)]
pub struct Node {
    pub config: NodeConfig,
    pub state: NodeState,
}

impl Node {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            state: NodeState::default(),
            config,
        }
    }

    pub fn into_block(&mut self, previous_block: &Block) -> Result<Block> {
        let transactions = self.state.mem_pool.drain();
        let mut block = Block::new(&self.config.keypair, previous_block, transactions)?;
        block.mine()?;
        Ok(block)
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        match message {
            Message::NewBlock(block) => self.state.add_block(block),
            Message::NewTransaction(transaction) => self.state.add_transaction(transaction),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::*;
    use crate::constants::*;
    use crate::crypto::*;
    use crate::transaction::*;
    use std::collections::HashSet;

    fn genesis_block(keypair: &KeyPair, difficulty: u8) -> Result<Block> {
        let height = 1;
        let timestamp = chrono::Utc::now().timestamp() as u32;
        let coinbase_tx = Transaction::new_coinbase(keypair, height).unwrap();
        let transactions = vec![coinbase_tx];
        let merkle_tree = Transaction::build_merkle_tree(&transactions).unwrap();
        let merkle_root = merkle_tree.root().unwrap();

        let header = BlockHeader {
            previous_block_hash: [0; 32],
            merkle_root,
            timestamp,
            difficulty,
            nonce: 0,
        };

        let mut block = Block {
            height,
            header,
            transactions,
        };

        block.mine().unwrap();

        Ok(block)
    }

    #[test]
    fn test_node() {
        let keypair_bob = KeyPair::generate();
        let address_bob = Address::from_public_key(&keypair_bob.public_key);

        let mut node = Node::new(NodeConfig {
            keypair: keypair_bob.clone(),
        });

        let genesis_block = genesis_block(&keypair_bob, 2).unwrap();
        node.handle_message(Message::NewBlock(genesis_block.clone()))
            .unwrap();

        assert_eq!(node.state.chain.height(), 1);

        // first transaction from genesis block to alice
        let keypair_alice = KeyPair::generate();
        let address_alice = Address::from_public_key(&keypair_alice.public_key);

        let coinbase_tx = genesis_block.transactions.get(0).unwrap();

        let tx_a_body = TransactionBody {
            input: TransactionInput::Reference(coinbase_tx.output_reference(0).unwrap()),
            outputs: vec![
                TransactionOutput {
                    value: (GENESIS_BLOCK_REWARD / 2) as u64,
                    address: address_alice.clone(),
                },
                TransactionOutput {
                    value: (GENESIS_BLOCK_REWARD / 2) as u64,
                    address: address_bob.clone(),
                },
            ],
        };

        let tx_a = tx_a_body.into_tx(&keypair_bob).unwrap();

        node.handle_message(Message::NewTransaction(tx_a.clone()))
            .unwrap();

        assert_eq!(node.state.mem_pool.pending_transactions.len(), 1);

        // second transaction from alice to charlie
        let keypair_charlie = KeyPair::generate();
        let address_charlie = Address::from_public_key(&keypair_charlie.public_key);

        let tx_b_body = TransactionBody {
            input: TransactionInput::Reference(tx_a.output_reference(0).unwrap()),
            outputs: vec![TransactionOutput {
                value: (GENESIS_BLOCK_REWARD / 2) as u64,
                address: address_charlie.clone(),
            }],
        };

        let tx_b = tx_b_body.into_tx(&keypair_alice).unwrap();

        node.handle_message(Message::NewTransaction(tx_b.clone()))
            .unwrap();

        // create a new block with the pending transactions and add it to the chain
        let block = node.into_block(&genesis_block).unwrap();
        node.handle_message(Message::NewBlock(block)).unwrap();

        // verify pending transactions are flushed and added to a new latest block
        assert_eq!(node.state.mem_pool.pending_transactions.len(), 0);

        let latest_block = node.state.chain.tail().unwrap();
        assert_eq!(latest_block.height, 2);

        let latest_block_transaction_ids = node
            .state
            .store
            .get(&latest_block.header.hash().unwrap())
            .unwrap()
            .transactions
            .iter()
            .map(|tx| tx.id())
            .collect::<Result<HashSet<_>>>()
            .unwrap();

        let expected_transaction_ids = HashSet::from([tx_a.id().unwrap(), tx_b.id().unwrap()]);

        assert!(latest_block_transaction_ids.is_superset(&expected_transaction_ids));
    }
}
