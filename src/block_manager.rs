use anyhow::Result;
use std::{collections::HashMap, sync::Arc};

use crate::{block::Block, chain::BlockchainNode, crypto::Hash};

#[derive(Debug, Clone, Default)]
pub struct BlockManager {
    pub blocks: HashMap<Hash, Arc<Block>>,
    pub nodes: HashMap<Hash, Arc<BlockchainNode>>,
    pub orphan_blocks: HashMap<Hash, Arc<Block>>,
}

#[derive(Debug, Clone)]
pub enum AddBlockResult {
    Added(Arc<BlockchainNode>),
    Orphaned,
}

impl BlockManager {
    pub fn get_block(&self, hash: &Hash) -> Option<&Block> {
        self.blocks.get(hash).map(Arc::as_ref)
    }

    pub fn contains_block(&self, hash: &Hash) -> bool {
        self.blocks.contains_key(hash)
    }

    pub fn add_block(&mut self, block: Arc<Block>) -> Result<AddBlockResult> {
        let hash = block.header.hash()?;

        let previous_node = self.nodes.get(&block.header.previous_block_hash);

        if previous_node.is_none() && block.height > 1 {
            self.orphan_blocks.insert(hash, block);
            return Ok(AddBlockResult::Orphaned);
        }

        let mut node = BlockchainNode::new(&block);
        node.set_previous(previous_node.cloned())?;

        let node_ref = Arc::new(node);

        self.nodes.insert(hash, node_ref.clone());
        self.blocks.insert(hash, block);

        Ok(AddBlockResult::Added(node_ref))
    }

    pub fn remove_block(&mut self, hash: &Hash) {
        self.blocks.remove(hash);
        self.nodes.remove(hash);
        self.orphan_blocks.remove(hash);
    }
}
