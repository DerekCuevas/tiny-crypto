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

#[derive(Debug, Clone)]
pub struct NodeState {
    pub store: HashMap<Hash, Block>,
    pub chain: Blockchain,
    pub uxto_set: UTXOSet,
    pub mem_pool: MemPool,
}

impl NodeState {
    pub fn new(mem_pool_size: usize) -> Self {
        Self {
            store: HashMap::new(),
            chain: Blockchain::default(),
            uxto_set: UTXOSet::default(),
            mem_pool: MemPool::new(mem_pool_size),
        }
    }

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
        block.validate()?;

        self.chain.add_block(&block)?;
        self.store.insert(hash, block);
        self.build_uxto_set()?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    NewBlock(Block),
    NewTransaction(Transaction),
}

#[derive(Clone)]
pub struct NodeSettings {
    pub block_size_limit: usize,
}

#[derive(Clone)]
pub struct NodeConfig {
    pub keypair: KeyPair,
    pub settings: NodeSettings,
}

#[derive(Clone)]
pub struct Node {
    pub config: NodeConfig,
    pub state: NodeState,
}

impl Node {
    pub fn new(config: NodeConfig) -> Self {
        Self {
            state: NodeState::new(config.settings.block_size_limit),
            config,
        }
    }

    fn handle_new_block(&mut self, block: Block) -> Result<()> {
        self.state.add_block(block)
    }

    fn handle_new_transaction(&mut self, transaction: Transaction) -> Result<()> {
        self.state.mem_pool.add(&self.state.uxto_set, transaction)?;

        // TODO: queue for mining
        if self.state.mem_pool.is_full() {
            let transactions = self.state.mem_pool.drain();

            if let Some(previous_block) = self.state.tail_block() {
                let mut block = Block::new(&self.config.keypair, previous_block, transactions)?;
                block.mine()?;
                self.handle_new_block(block)?;
            }
        }

        Ok(())
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        match message {
            Message::NewBlock(block) => self.handle_new_block(block),
            Message::NewTransaction(transaction) => self.handle_new_transaction(transaction),
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
            settings: NodeSettings {
                block_size_limit: 2,
            },
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

        // // second transaction from alice to charlie
        // let keypair_charlie = KeyPair::generate();
        // let address_charlie = Address::from_public_key(&keypair_charlie.public_key);

        // let tx_b_body = TransactionBody {
        //     input: TransactionInput::Reference(tx_a.output_reference(0).unwrap()),
        //     outputs: vec![TransactionOutput {
        //         value: (GENESIS_BLOCK_REWARD / 2) as u64,
        //         address: address_charlie.clone(),
        //     }],
        // };

        // let tx_b = tx_b_body.into_tx(&keypair_alice).unwrap();

        // node.handle_message(Message::NewTransaction(tx_b.clone()))
        //     .unwrap();

        // verify pending transactions are flushed and added to a new latest block
        // assert_eq!(node.tx_state.pending_transactions.len(), 0);

        // let latest_block = node.latest_block.as_ref().unwrap();
        // assert_eq!(latest_block.height, 1);

        // let latest_block_transaction_ids = latest_block
        //     .transactions
        //     .iter()
        //     .map(|tx| tx.id())
        //     .collect::<Result<HashSet<_>>>()
        //     .unwrap();

        // let expected_transaction_ids = HashSet::from([tx_a.id().unwrap(), tx_b.id().unwrap()]);

        // assert!(latest_block_transaction_ids.is_superset(&expected_transaction_ids));
    }
}
