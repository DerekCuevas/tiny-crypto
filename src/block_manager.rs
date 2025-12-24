use anyhow::Result;
use hex;
use std::fs;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::{block::Block, chain::BlockchainNode, crypto::Hash};

#[derive(Debug, Clone)]
pub struct BlockManager {
    pub blocks: HashMap<Hash, Arc<Block>>,
    pub nodes: HashMap<Hash, Arc<BlockchainNode>>,
    pub orphan_blocks: HashMap<Hash, Arc<Block>>,
    data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub enum AddBlockResult {
    Added(Arc<BlockchainNode>),
    Orphaned,
}

impl Default for BlockManager {
    fn default() -> Self {
        // Default implementation uses ./data directory
        // If directory creation fails, we'll still create the struct
        // but persistence will fail later (which is acceptable for Default)
        let data_dir = PathBuf::from("./data");
        let _ = fs::create_dir_all(&data_dir);

        Self {
            blocks: HashMap::new(),
            nodes: HashMap::new(),
            orphan_blocks: HashMap::new(),
            data_dir,
        }
    }
}

impl BlockManager {
    pub fn new(data_dir: Option<PathBuf>) -> Result<Self> {
        Self::new_with_load(data_dir, false)
    }

    pub fn new_with_load(data_dir: Option<PathBuf>, load_from_disk: bool) -> Result<Self> {
        let data_dir = data_dir.unwrap_or_else(|| PathBuf::from("./data"));

        // Create the directory if it doesn't exist
        fs::create_dir_all(&data_dir)?;

        let mut manager = Self {
            blocks: HashMap::new(),
            nodes: HashMap::new(),
            orphan_blocks: HashMap::new(),
            data_dir,
        };

        if load_from_disk {
            manager.load_from_disk()?;
        }

        Ok(manager)
    }

    pub fn load_from_disk(&mut self) -> Result<usize> {
        let mut loaded_count = 0;

        // Read all JSON files in the data directory
        let entries = fs::read_dir(&self.data_dir)?;
        let mut blocks_to_load: Vec<(Hash, Block)> = Vec::new();

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Only process .json files
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Extract hash from filename (remove .json extension)
                if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                    if let Ok(hash_bytes) = hex::decode(filename) {
                        if hash_bytes.len() == 32 {
                            let mut hash_array = [0u8; 32];
                            hash_array.copy_from_slice(&hash_bytes);
                            let hash = Hash(hash_array);

                            // Read and deserialize the block
                            let file_content = fs::read_to_string(&path)?;
                            match serde_json::from_str::<Block>(&file_content) {
                                Ok(block) => {
                                    blocks_to_load.push((hash, block));
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Warning: Failed to deserialize block from {:?}: {}",
                                        path, e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort blocks by height (genesis block first)
        blocks_to_load.sort_by_key(|(_, block)| block.height);

        // Process blocks in order, building the node chain
        for (hash, block) in blocks_to_load {
            let block_arc = Arc::new(block);
            let block_hash = block_arc.header.hash()?;

            // Verify the hash matches the filename
            if block_hash != hash {
                eprintln!("Warning: Block hash mismatch for file {:?}", hash);
                continue;
            }

            // Try to add the block (this will handle orphan blocks)
            match self.add_block_internal(block_arc.clone(), false)? {
                AddBlockResult::Added(_) => {
                    loaded_count += 1;
                }
                AddBlockResult::Orphaned => {
                    // Store as orphan block
                    self.orphan_blocks.insert(hash, block_arc);
                    loaded_count += 1;
                }
            }
        }

        Ok(loaded_count)
    }

    fn add_block_internal(&mut self, block: Arc<Block>, persist: bool) -> Result<AddBlockResult> {
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
        self.blocks.insert(hash, block.clone());

        // Optionally persist block to filesystem
        if persist {
            self.persist_block(&block, &hash)?;
        }

        Ok(AddBlockResult::Added(node_ref))
    }

    pub fn get_block(&self, hash: &Hash) -> Option<&Block> {
        self.blocks.get(hash).map(Arc::as_ref)
    }

    pub fn contains_block(&self, hash: &Hash) -> bool {
        self.blocks.contains_key(hash)
    }

    pub fn add_block(&mut self, block: Arc<Block>) -> Result<AddBlockResult> {
        self.add_block_internal(block, true)
    }

    fn persist_block(&self, block: &Block, hash: &Hash) -> Result<()> {
        // Serialize block as JSON
        let json = serde_json::to_string_pretty(block)?;

        // Create filename from hash (hex encoded)
        let filename = format!("{}.json", hex::encode(hash));
        let file_path = self.data_dir.join(filename);

        // Write to file
        fs::write(&file_path, json)?;

        Ok(())
    }

    pub fn remove_block(&mut self, hash: &Hash) {
        self.blocks.remove(hash);
        self.nodes.remove(hash);
        self.orphan_blocks.remove(hash);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::*;
    use crate::crypto::*;
    use crate::transaction::*;
    use std::fs;

    struct TestDirCleanup {
        path: PathBuf,
    }

    impl Drop for TestDirCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_block_persistence() {
        // Create a temporary test directory
        let test_dir = PathBuf::from("./test_data_persistence");

        // Clean up any existing test directory
        let _ = fs::remove_dir_all(&test_dir);

        // Ensure cleanup happens even if test fails
        let _cleanup = TestDirCleanup {
            path: test_dir.clone(),
        };

        // Create BlockManager with test directory
        let mut manager = BlockManager::new(Some(test_dir.clone())).unwrap();

        // Create a genesis block
        let keypair = KeyPair::generate();
        let genesis_tx = Transaction::new_coinbase(&keypair, 0).unwrap();

        let mut genesis_block = Block {
            height: 0,
            transactions: vec![genesis_tx],
            header: BlockHeader::default(),
        };

        genesis_block.header.difficulty = 1;
        genesis_block.mine().unwrap();

        let block_hash = genesis_block.header.hash().unwrap();
        let block_arc = Arc::new(genesis_block.clone());

        // Add block to manager (this should persist it)
        let result = manager.add_block(block_arc).unwrap();
        assert!(matches!(result, AddBlockResult::Added(_)));

        // Verify the file was created
        let filename = format!("{}.json", hex::encode(block_hash));
        let file_path = test_dir.join(&filename);
        assert!(
            file_path.exists(),
            "Block file should exist at {:?}",
            file_path
        );

        // Read and verify the JSON content
        let file_content = fs::read_to_string(&file_path).unwrap();
        assert!(!file_content.is_empty(), "Block file should not be empty");

        // Verify it's valid JSON and contains expected fields
        let json_value: serde_json::Value = serde_json::from_str(&file_content).unwrap();
        assert_eq!(json_value["height"], genesis_block.height);
        assert_eq!(
            json_value["header"]["difficulty"],
            genesis_block.header.difficulty
        );
        assert_eq!(json_value["header"]["nonce"], genesis_block.header.nonce);

        // Verify that Hash fields are serialized as hex strings, not arrays
        assert!(json_value["header"]["previous_block_hash"].is_string());
        assert!(json_value["header"]["merkle_root"].is_string());
        let previous_hash_str = json_value["header"]["previous_block_hash"]
            .as_str()
            .unwrap();
        assert_eq!(previous_hash_str.len(), 64); // 32 bytes = 64 hex characters

        assert!(json_value["transactions"].is_array());
        assert_eq!(
            json_value["transactions"].as_array().unwrap().len(),
            genesis_block.transactions.len()
        );

        // Verify the block is also in memory
        assert!(manager.contains_block(&block_hash));
        let retrieved_block = manager.get_block(&block_hash).unwrap();
        assert_eq!(retrieved_block.height, genesis_block.height);

        // Cleanup happens automatically via Drop trait
    }

    #[test]
    fn test_load_from_disk() {
        // Create a temporary test directory
        let test_dir = PathBuf::from("./test_data_load");

        // Clean up any existing test directory
        let _ = fs::remove_dir_all(&test_dir);

        // Ensure cleanup happens even if test fails
        let _cleanup = TestDirCleanup {
            path: test_dir.clone(),
        };

        // Create BlockManager and add a genesis block
        let keypair = KeyPair::generate();
        let genesis_tx = Transaction::new_coinbase(&keypair, 0).unwrap();

        let mut genesis_block = Block {
            height: 0,
            transactions: vec![genesis_tx],
            header: BlockHeader::default(),
        };

        genesis_block.header.difficulty = 1;
        genesis_block.mine().unwrap();

        let genesis_hash = genesis_block.header.hash().unwrap();

        {
            let mut manager = BlockManager::new(Some(test_dir.clone())).unwrap();
            manager.add_block(Arc::new(genesis_block.clone())).unwrap();
        }

        // Create a new BlockManager and load from disk
        let mut manager2 = BlockManager::new_with_load(Some(test_dir.clone()), true).unwrap();

        // Verify the block was loaded
        assert!(manager2.contains_block(&genesis_hash));
        let loaded_block = manager2.get_block(&genesis_hash).unwrap();
        assert_eq!(loaded_block.height, genesis_block.height);
        assert_eq!(
            loaded_block.header.difficulty,
            genesis_block.header.difficulty
        );
        assert_eq!(loaded_block.header.nonce, genesis_block.header.nonce);

        // Verify node was also reconstructed
        assert!(manager2.nodes.contains_key(&genesis_hash));

        // Add a second block and verify it can be loaded
        let mut block2 = Block::new(&keypair, &genesis_block, vec![]).unwrap();
        block2.mine().unwrap();
        let block2_hash = block2.header.hash().unwrap();

        {
            manager2.add_block(Arc::new(block2.clone())).unwrap();
        }

        // Create a third BlockManager and verify both blocks load
        let manager3 = BlockManager::new_with_load(Some(test_dir.clone()), true).unwrap();
        assert!(manager3.contains_block(&genesis_hash));
        assert!(manager3.contains_block(&block2_hash));
        assert_eq!(manager3.blocks.len(), 2);
        assert_eq!(manager3.nodes.len(), 2);

        // Cleanup happens automatically via Drop trait
    }
}
