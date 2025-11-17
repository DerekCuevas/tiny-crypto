use std::{collections::BTreeMap, sync::Arc};

use anyhow::Result;

use crate::block::{Block, BlockHeader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockchainNode {
    pub height: u32,
    pub header: BlockHeader,
    pub previous: Option<Arc<BlockchainNode>>,
}

#[derive(Debug, Clone, Default)]
pub struct Blockchain {
    pub nodes: BTreeMap<u32, Arc<BlockchainNode>>,
}

impl Blockchain {
    pub fn build(blocks: Vec<Block>) -> Self {
        let mut nodes = BTreeMap::new();

        let mut sorted_nodes: Vec<(u32, BlockHeader)> =
            blocks.into_iter().map(|b| (b.height, b.header)).collect();
        sorted_nodes.sort_by_key(|(height, _)| *height);

        for (height, header) in sorted_nodes {
            let previous = (height > 0)
                .then(|| nodes.get(&(height - 1)).map(Arc::clone))
                .flatten();

            nodes.insert(
                height,
                Arc::new(BlockchainNode {
                    height,
                    header,
                    previous,
                }),
            );
        }

        Self { nodes }
    }

    pub fn add_block(&mut self, block: &Block) -> Result<()> {
        if self.contains_block(&block) {
            return Ok(());
        }

        let tail = self.tail();

        let is_next = tail
            .as_ref()
            .map(|t| t.height + 1 == block.height)
            .unwrap_or(true);

        // TODO: handle fork
        if !is_next {
            return Err(anyhow::anyhow!("Block is not the next in the chain"));
        }

        let node = BlockchainNode {
            height: block.height,
            header: block.header.clone(),
            previous: tail.clone(),
        };

        self.nodes.insert(block.height, Arc::new(node));

        Ok(())
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

    pub fn get_node(&self, height: u32) -> Option<Arc<BlockchainNode>> {
        self.nodes.get(&height).map(|node| node.clone())
    }

    pub fn contains_node(&self, index: &BlockchainNode) -> bool {
        self.nodes
            .get(&index.height)
            .is_some_and(|node| node.as_ref() == index)
    }

    pub fn contains_block(&self, block: &Block) -> bool {
        self.nodes
            .get(&block.height)
            .is_some_and(|node| node.header.hash().ok() == block.header.hash().ok())
    }

    pub fn find_fork(&self, other_chain_node: &BlockchainNode) -> Option<Arc<BlockchainNode>> {
        let mut current_node = Some(Arc::new(other_chain_node.clone()));

        while let Some(node) = current_node {
            if self.contains_node(&node) {
                return Some(node);
            }

            current_node = node.previous.clone();
        }

        None
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

    #[test]
    fn test_build_blockchain() {
        let block_a = test_block(1, None, vec![]);
        let block_b = test_block(2, Some(&block_a), vec![]);
        let block_c = test_block(3, Some(&block_b), vec![]);
        let chain_a = Blockchain::build(vec![block_a.clone(), block_b.clone(), block_c.clone()]);

        assert_eq!(chain_a.height(), 3);
        assert!(chain_a.contains_block(&block_a));
        assert!(chain_a.contains_block(&block_b));
        assert!(chain_a.contains_block(&block_c));

        let block_d = test_block(4, Some(&block_c), vec![]);
        let block_e = test_block(5, Some(&block_d), vec![]);
        let chain_b = Blockchain::build(vec![
            block_a.clone(),
            block_b.clone(),
            block_c.clone(),
            block_d.clone(),
            block_e.clone(),
        ]);

        assert_eq!(chain_b.height(), 5);
        assert!(chain_b.contains_block(&block_d));
        assert!(chain_b.contains_block(&block_e));

        let tail_b_node = chain_b.tail().unwrap();
        let fork = chain_a.find_fork(&tail_b_node).unwrap();

        assert_eq!(fork.height, 3);

        let mut chain_c = chain_a.clone();
        chain_c.add_block(&block_d).unwrap();
        assert_eq!(chain_c.height(), 4);
    }
}
