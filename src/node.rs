use crate::{
    block::Block, block_manager::BlockManager, chain::Blockchain, crypto::KeyPair,
    mem_pool::MemPool, transaction::Transaction, utxo_set::UTXOSet,
};
use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct NodeState {
    pub block_manager: BlockManager,
    pub chain: Blockchain,
    pub utxo_set: UTXOSet,
    pub mem_pool: MemPool,
}

impl NodeState {
    pub fn build_utxo_set(&mut self) -> Result<()> {
        self.utxo_set = UTXOSet::default();

        for node in self.chain.nodes.values() {
            if let Some(block) = self.block_manager.get_block(&node.header.hash()?) {
                for tx in &block.transactions {
                    self.utxo_set.update(tx)?;
                }
            }
        }

        Ok(())
    }

    pub fn add_block(&mut self, block: Block) -> Result<()> {
        let hash = block.header.hash()?;

        if self.block_manager.contains_block(&hash) {
            return Ok(());
        }

        block.validate()?;

        let Some(node) = self.block_manager.add_block(block)? else {
            return Ok(());
        };

        if node.work >= self.chain.chain_work().unwrap_or_default() {
            self.chain.set_tail(node)?;
            self.build_utxo_set()?;
        }

        Ok(())
    }

    pub fn add_transaction(&mut self, transaction: Transaction) -> Result<()> {
        self.mem_pool.add(&self.utxo_set, transaction)?;
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

    fn test_block(
        keypair: &KeyPair,
        difficulty: u8,
        previous: Option<&Block>,
        transactions: Vec<Transaction>,
    ) -> Result<Block> {
        let height = previous.map(|p| p.height + 1).unwrap_or(1);

        let coinbase_tx = Transaction::new_coinbase(keypair, height)?;
        let mut block_transactions = vec![coinbase_tx];
        block_transactions.extend(transactions);

        let merkle_tree = Transaction::build_merkle_tree(&block_transactions)?;
        let merkle_root = merkle_tree.root().unwrap_or_default();

        let header = BlockHeader {
            previous_block_hash: previous
                .and_then(|p| p.header.hash().ok())
                .unwrap_or_default(),
            merkle_root,
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty,
            nonce: 0,
        };

        let mut block = Block {
            header,
            height,
            transactions: block_transactions,
        };

        block.mine()?;
        Ok(block)
    }

    #[test]
    fn test_append_transactions() {
        let keypair_bob = KeyPair::generate();
        let address_bob = Address::from_public_key(&keypair_bob.public_key);

        let mut node = Node::new(NodeConfig {
            keypair: keypair_bob.clone(),
        });

        let genesis_block = test_block(&keypair_bob, 2, None, vec![]).unwrap();
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

        let tail_node = node.state.chain.tail().unwrap();
        assert_eq!(tail_node.height, 2);

        let latest_block_transaction_ids = node
            .state
            .block_manager
            .get_block(&tail_node.header.hash().unwrap())
            .unwrap()
            .transactions
            .iter()
            .map(|tx| tx.id())
            .collect::<Result<HashSet<_>>>()
            .unwrap();

        let expected_transaction_ids = HashSet::from([tx_a.id().unwrap(), tx_b.id().unwrap()]);

        assert!(latest_block_transaction_ids.is_superset(&expected_transaction_ids));
    }

    #[test]
    fn test_append_block() {
        let keypair = KeyPair::generate();

        let mut node = Node::new(NodeConfig {
            keypair: keypair.clone(),
        });

        let block_a = test_block(&keypair, 2, None, vec![]).unwrap();
        node.handle_message(Message::NewBlock(block_a.clone()))
            .unwrap();

        assert_eq!(node.state.chain.height(), 1);

        let block_b = test_block(&keypair, 2, Some(&block_a), vec![]).unwrap();
        node.handle_message(Message::NewBlock(block_b.clone()))
            .unwrap();

        assert_eq!(node.state.chain.height(), 2);

        // add a block that is not the next in the chain, should be orphaned
        let block_c = test_block(&keypair, 2, Some(&block_b), vec![]).unwrap();
        let block_d = test_block(&keypair, 2, Some(&block_c), vec![]).unwrap();

        assert_eq!(node.state.block_manager.orphan_blocks.len(), 0);

        node.handle_message(Message::NewBlock(block_d.clone()))
            .unwrap();

        assert_eq!(node.state.block_manager.orphan_blocks.len(), 1);
        assert_eq!(node.state.chain.height(), 2);
    }
}
