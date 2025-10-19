use crate::crypto::sha256d;
use anyhow::Result;
use bincode::{Encode, encode_into_slice};

#[derive(Debug, Clone, Encode)]
pub struct BlockHeader {
    version: u32,
    previous_block_hash: [u8; 32],
    merkle_root: [u8; 32],
    timestamp: u32,
    bits: u32,
    nonce: u32,
}

impl BlockHeader {
    pub fn as_bytes(&self) -> Result<[u8; 80]> {
        let encode_config = bincode::config::standard()
            .with_little_endian()
            .with_fixed_int_encoding()
            .with_limit::<80>();

        let mut bytes = [0u8; 80];
        encode_into_slice(self, &mut bytes, encode_config)?;
        Ok(bytes)
    }

    pub fn hash(&self) -> Result<[u8; 32]> {
        Ok(sha256d(&self.as_bytes()?))
    }

    pub fn mine(&mut self) {
        todo!()
    }
}

pub struct Block {
    header: BlockHeader,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_header() {
        let header = BlockHeader {
            version: 1,
            previous_block_hash: [2; 32],
            merkle_root: [3; 32],
            timestamp: 4,
            bits: 5,
            nonce: 6,
        };

        let bytes = header.as_bytes();
        let hash = header.hash();

        dbg!(&bytes);
        dbg!(&hash);
    }
}
