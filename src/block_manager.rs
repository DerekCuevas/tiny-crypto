use anyhow::Result;
use std::{collections::HashMap, sync::Arc};

use crate::{block::Block, chain::BlockchainNode, crypto::Hash};

#[derive(Debug, Clone, Default)]
pub struct BlockManager {
    pub block_index: HashMap<Hash, Block>,
    pub node_index: HashMap<Hash, Arc<BlockchainNode>>,
    pub orphan_blocks: HashMap<Hash, Block>,
}

impl BlockManager {
    pub fn get_block(&self, hash: &Hash) -> Option<&Block> {
        self.block_index.get(hash)
    }

    pub fn contains_block(&self, hash: &Hash) -> bool {
        self.block_index.contains_key(hash)
    }

    pub fn add_block(&mut self, block: Block) -> Result<Option<Arc<BlockchainNode>>> {
        let hash = block.header.hash()?;

        let previous_node = self.node_index.get(&block.header.previous_block_hash);

        if previous_node.is_none() && block.height > 1 {
            self.orphan_blocks.insert(hash, block);
            return Ok(None);
        }

        let mut node = BlockchainNode::new(&block);
        node.set_previous(previous_node.cloned())?;

        let node_ref = Arc::new(node);

        self.node_index.insert(hash, node_ref.clone());
        self.block_index.insert(hash, block);

        Ok(Some(node_ref))
    }
}
