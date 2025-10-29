use std::cmp::Ordering;

use anyhow::Result;
use bincode::Encode;
use hex;

use crate::{
    crypto::{Hash, KeyPair, sha256d},
    transaction::Transaction,
};

#[derive(Debug, Clone, Encode, Default)]
pub struct BlockHeader {
    pub previous_block_hash: Hash,
    pub merkle_root: Hash,
    pub timestamp: u32,
    pub difficulty: u8,
    pub nonce: u64,
}

impl BlockHeader {
    pub fn as_bytes(&self) -> Result<Vec<u8>> {
        Ok(bincode::encode_to_vec(self, bincode::config::standard())?)
    }

    pub fn hash(&self) -> Result<Hash> {
        Ok(sha256d(&self.as_bytes()?))
    }

    fn difficulty_target(&self) -> Result<Hash> {
        if self.difficulty >= 32 {
            return Err(anyhow::anyhow!("Difficultly target is too high"));
        }

        let mut target = Hash::default();

        target[0] = u8::MAX;
        target.rotate_right(self.difficulty as usize);

        Ok(target)
    }

    fn target_met(&self, hash: &Hash, target: &Hash) -> bool {
        matches!(hash.cmp(target), Ordering::Less | Ordering::Equal)
    }

    pub fn compute_nonce_naive(&self) -> Result<u64> {
        let target = self.difficulty_target()?;

        let mut header = self.clone();
        header.nonce = 0;

        loop {
            let hash = header.hash()?;

            if self.target_met(&hash, &target) {
                break;
            }

            header.nonce += 1;

            if header.nonce % 1_000_000 == 0 {
                println!("Nonce: {}, Hash: 0x{}", header.nonce, hex::encode(hash));
            }
        }

        Ok(header.nonce)
    }

    pub fn validate_hash(&self) -> Result<bool> {
        let hash = self.hash()?;
        let target = self.difficulty_target()?;

        Ok(self.target_met(&hash, &target))
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub height: u32,
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn new(
        keypair: &KeyPair,
        previous: &Block,
        input_transactions: Vec<Transaction>,
    ) -> Result<Self> {
        let height = previous.height + 1;
        let coinbase_tx = Transaction::new_coinbase(keypair, height)?;

        let mut transactions = vec![coinbase_tx];
        transactions.extend(input_transactions);

        let merkle_tree = Transaction::build_merkle_tree(&transactions)?;
        let merkle_root = merkle_tree
            .root()
            .ok_or(anyhow::anyhow!("Failed to compute merkle root"))?;

        let header = BlockHeader {
            previous_block_hash: previous.header.hash()?,
            merkle_root,
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty: previous.header.difficulty,
            nonce: 0,
        };

        Ok(Self {
            height,
            header,
            transactions,
        })
    }

    pub fn mine(&mut self) -> Result<()> {
        self.header.nonce = self.header.compute_nonce_naive()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::*;
    use crate::transaction::*;

    #[test]
    fn test_block_header() {
        let header = BlockHeader {
            previous_block_hash: [2; 32],
            merkle_root: [3; 32],
            timestamp: 4,
            difficulty: 1,
            nonce: 0,
        };

        let bytes = header.as_bytes().unwrap();
        let hash = header.hash().unwrap();

        println!("Block Header Bytes: 0x{}", hex::encode(bytes));
        println!("Block Hash: 0x{}", hex::encode(hash));
    }

    #[test]
    fn test_difficulty_target() {
        let header = BlockHeader {
            previous_block_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 0,
            difficulty: 2,
            nonce: 0,
        };

        let target = header.difficulty_target().unwrap();

        let expected = [
            0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(target, expected);
        println!("Difficulty target: 0x{}", hex::encode(target));
    }

    #[test]
    fn test_compute_nonce() {
        let mut header = BlockHeader {
            previous_block_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 1760850297,
            difficulty: 1,
            nonce: 0,
        };

        let nonce = header.compute_nonce_naive().unwrap();
        println!("Found Nonce: {}", nonce);
        header.nonce = nonce;

        let hash = header.hash().unwrap();
        println!("Block Hash: 0x{}", hex::encode(hash));

        let is_valid = header.validate_hash().unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_build_block() {
        let keypair_bob = KeyPair::generate();

        let genesis_tx = Transaction::new_coinbase(&keypair_bob, 0).unwrap();

        let mut genesis_block = Block {
            height: 0,
            transactions: vec![genesis_tx.clone()],
            header: BlockHeader::default(),
        };

        genesis_block.header.difficulty = 1;
        genesis_block.mine().unwrap();

        println!(
            "Genesis block hash: 0x{}",
            hex::encode(genesis_block.header.hash().unwrap())
        );

        let keypair_alice = KeyPair::generate();
        let address_alice = Address::from_public_key(&keypair_alice.public_key);

        let tx_a_body = TransactionBody {
            input: TransactionInput::Reference(genesis_tx.output_reference(0).unwrap()),
            outputs: vec![TransactionOutput {
                value: 50,
                address: address_alice.clone(),
            }],
        };

        let tx_a = tx_a_body.into_tx(&keypair_bob).unwrap();

        let mut block = Block::new(&keypair_bob, &genesis_block, vec![tx_a.clone()]).unwrap();

        block.mine().unwrap();
        println!(
            "Block hash: 0x{}",
            hex::encode(block.header.hash().unwrap())
        );
    }
}
