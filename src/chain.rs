use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;
use num_bigint::BigUint;
use num_traits::{One, Zero};

use crate::{
    block::{Block, BlockHeader},
    block_manager::BlockManager,
    utxo_set::UTXOSet,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockchainNode {
    pub height: u32,
    pub header: BlockHeader,
    pub work: BigUint,
    pub previous: Option<Arc<BlockchainNode>>,
}

impl BlockchainNode {
    pub fn new(block: &Block) -> Self {
        Self {
            height: block.height,
            header: block.header.clone(),
            work: BigUint::zero(),
            previous: None,
        }
    }

    fn calculate_work(&self) -> Result<BigUint> {
        let target_bytes = self.header.difficulty_target()?;
        let target = BigUint::from_bytes_be(&target_bytes);

        let max_target = (BigUint::from(2u32).pow(256u32)) - BigUint::one();
        let block_work = ((&max_target - &target) / (&target + BigUint::one())) + BigUint::one();

        let previous_work = self
            .previous
            .as_ref()
            .map(|prev| prev.work.clone())
            .unwrap_or_else(BigUint::zero);

        Ok(previous_work + block_work)
    }

    pub fn set_previous(&mut self, previous: Option<Arc<BlockchainNode>>) -> Result<()> {
        self.previous = previous;
        self.work = self.calculate_work()?;
        Ok(())
    }

    pub fn into_chain(node: Arc<BlockchainNode>) -> Result<Blockchain> {
        let mut chain = Blockchain::default();
        chain.set_tail(node)?;
        Ok(chain)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Blockchain {
    pub nodes: BTreeMap<u32, Arc<BlockchainNode>>,
}

impl Blockchain {
    pub fn is_empty(&self) -> bool {
        self.tail().is_none()
    }

    pub fn height(&self) -> u32 {
        self.nodes
            .last_key_value()
            .map(|(height, _)| *height)
            .unwrap_or(0)
    }

    pub fn tail(&self) -> Option<Arc<BlockchainNode>> {
        self.nodes.last_key_value().map(|(_, node)| node.clone())
    }

    pub fn chain_work(&self) -> Option<BigUint> {
        self.tail().map(|node| node.work.clone())
    }

    pub fn get_node(&self, height: u32) -> Option<Arc<BlockchainNode>> {
        self.nodes.get(&height).cloned()
    }

    pub fn contains_node(&self, index: &Arc<BlockchainNode>) -> bool {
        self.nodes
            .get(&index.height)
            .is_some_and(|node| node.as_ref() == index.as_ref())
    }

    pub fn contains_block(&self, block: &Block) -> bool {
        self.nodes
            .get(&block.height)
            .is_some_and(|node| node.header.hash().ok() == block.header.hash().ok())
    }

    pub fn set_tail(&mut self, node: Arc<BlockchainNode>) -> Result<()> {
        self.nodes.retain(|_, n| n.height < node.height);

        let mut current_node = node.previous.clone();

        self.nodes.insert(node.height, node);

        while let Some(node) = current_node {
            if self.contains_node(&node) {
                break;
            }

            current_node = node.previous.clone();
            self.nodes.insert(node.height, node);
        }

        Ok(())
    }

    pub fn build_utxo_set(&self, block_manager: &BlockManager) -> Result<UTXOSet> {
        let mut utxo_set = UTXOSet::default();

        for node in self.nodes.values() {
            if let Some(block) = block_manager.get_block(&node.header.hash()?) {
                for tx in &block.transactions {
                    utxo_set.update(tx)?;
                }
            }
        }

        Ok(utxo_set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::*;
    use crate::transaction::*;

    fn test_block(height: u32, previous: Option<&Block>, transactions: Vec<Transaction>) -> Block {
        let header = BlockHeader {
            previous_block_hash: previous
                .and_then(|p| p.header.hash().ok())
                .unwrap_or_default(),
            merkle_root: Hash::default(),
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty: 0,
            nonce: 0,
        };

        Block {
            header,
            height,
            transactions,
        }
    }

    fn build_from_blocks(mut blocks: Vec<Block>) -> Result<Blockchain> {
        blocks.sort_by_key(|b| b.height);

        let mut nodes = BTreeMap::new();

        for block in blocks {
            let mut node = BlockchainNode::new(&block);

            let previous = (block.height > 0)
                .then(|| nodes.get(&(block.height - 1)).map(Arc::clone))
                .flatten();

            node.set_previous(previous)?;

            nodes.insert(block.height, Arc::new(node));
        }

        Ok(Blockchain { nodes })
    }

    #[test]
    fn test_build_blockchain() {
        let block_a = test_block(1, None, vec![]);
        let block_b = test_block(2, Some(&block_a), vec![]);
        let block_c = test_block(3, Some(&block_b), vec![]);
        let chain_a =
            build_from_blocks(vec![block_a.clone(), block_b.clone(), block_c.clone()]).unwrap();

        assert_eq!(chain_a.height(), 3);
        assert!(chain_a.contains_block(&block_a));
        assert!(chain_a.contains_block(&block_b));
        assert!(chain_a.contains_block(&block_c));

        let block_d = test_block(4, Some(&block_c), vec![]);
        let block_e = test_block(5, Some(&block_d), vec![]);
        let mut chain_b = build_from_blocks(vec![
            block_a.clone(),
            block_b.clone(),
            block_c.clone(),
            block_d.clone(),
            block_e.clone(),
        ])
        .unwrap();

        assert_eq!(chain_b.height(), 5);
        assert!(chain_b.contains_block(&block_d));
        assert!(chain_b.contains_block(&block_e));
        assert!(chain_b.chain_work().unwrap() > chain_a.chain_work().unwrap());

        let tail_node_a = chain_a.tail().unwrap();
        chain_b.set_tail(tail_node_a).unwrap();

        assert_eq!(chain_b.height(), 3);
        assert_eq!(chain_b.nodes.len(), 3);
        assert!(chain_b.contains_block(&block_a));
        assert!(chain_b.contains_block(&block_b));
        assert!(chain_b.contains_block(&block_c));
        assert_eq!(chain_b.chain_work().unwrap(), chain_a.chain_work().unwrap());
    }
}
