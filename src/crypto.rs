use sha2::{Digest, Sha256};

pub type Hash = [u8; 32];

pub fn sha256d(bytes: &[u8]) -> Hash {
    Sha256::digest(Sha256::digest(bytes)).into()
}
