use std::cmp::Ordering;

use crate::{
    crypto::{Hash, sha256d},
    transaction::Transaction,
};
use anyhow::Result;
use bincode::{Encode, encode_into_slice};
use hex;

#[derive(Debug, Clone, Encode)]
pub struct BlockHeader {
    previous_block_hash: Hash,
    merkle_root: Hash,
    timestamp: u32,
    difficulty: u8,
    nonce: u64,
}

impl BlockHeader {
    pub fn as_bytes(&self) -> Result<[u8; 77]> {
        let encode_config = bincode::config::standard()
            .with_little_endian()
            .with_fixed_int_encoding()
            .with_limit::<77>();

        let mut bytes = [0u8; 77];
        encode_into_slice(self, &mut bytes, encode_config)?;
        Ok(bytes)
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

    pub fn compute_nonce(&self) -> Result<u64> {
        let target = self.difficulty_target()?;
        let mut bytes = self.as_bytes()?;
        let mut nonce = 0u64;

        loop {
            bytes[69..77].copy_from_slice(&nonce.to_le_bytes());

            let hash = sha256d(&bytes);

            if nonce % 1_000_000 == 0 {
                println!("Nonce: {}, Hash: 0x{}", nonce, hex::encode(hash));
            }

            if self.target_met(&hash, &target) {
                break;
            }

            nonce += 1;
        }

        Ok(nonce)
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
    pub fn new(previous: &Block, transactions: Vec<Transaction>) -> Result<Self> {
        // TODO: Add block reward coinbase transaction (must be first)
        // compute merkle root
        // mine block

        let header = BlockHeader {
            previous_block_hash: previous.header.hash()?,
            merkle_root: [0; 32],
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty: previous.header.difficulty,
            nonce: 0,
        };

        Ok(Self {
            height: previous.height + 1,
            header,
            transactions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let nonce = header.compute_nonce().unwrap();
        println!("Found Nonce: {}", nonce);
        header.nonce = nonce;

        let hash = header.hash().unwrap();
        println!("Block Hash: 0x{}", hex::encode(hash));

        let is_valid = header.validate_hash().unwrap();
        assert!(is_valid);
    }
}
