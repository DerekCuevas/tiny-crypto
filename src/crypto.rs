use bincode::Encode;
use ripemd::Ripemd160;
use secp256k1::ecdsa::Signature;
use secp256k1::{Message, Secp256k1};
use secp256k1::{PublicKey, SecretKey, rand};
use sha2::{Digest, Sha256};

pub type Hash = [u8; 32];

pub fn sha256d(bytes: &[u8]) -> Hash {
    Sha256::digest(Sha256::digest(bytes)).into()
}

#[derive(Clone)]
pub struct KeyPair {
    pub secret_key: SecretKey,
    pub public_key: PublicKey,
}

impl KeyPair {
    pub fn generate() -> Self {
        let secp = Secp256k1::new();
        let (secret_key, public_key) = secp.generate_keypair(&mut rand::rng());
        Self {
            secret_key,
            public_key,
        }
    }

    pub fn sign(&self, bytes: &[u8]) -> Signature {
        let secp = Secp256k1::new();
        let digest = sha256d(bytes);
        let message = Message::from_digest(digest);
        secp.sign_ecdsa(message, &self.secret_key)
    }
}

pub trait SignatureExt {
    fn verify(&self, bytes: &[u8], public_key: &PublicKey) -> bool;
}

impl SignatureExt for Signature {
    fn verify(&self, bytes: &[u8], public_key: &PublicKey) -> bool {
        let secp = Secp256k1::verification_only();
        let digest = sha256d(bytes);
        let message = Message::from_digest(digest);
        secp.verify_ecdsa(message, &self, &public_key).is_ok()
    }
}

#[derive(Debug, Clone, Encode, Eq, PartialEq)]
pub struct Address(String);

impl Address {
    pub fn from_public_key(public_key: &PublicKey) -> Self {
        let hash_1 = Sha256::digest(public_key.serialize_uncompressed());

        let mut ripemd_hasher = Ripemd160::new();
        ripemd_hasher.update(hash_1);
        let hash_2 = ripemd_hasher.finalize();

        let version_byte = 0u8.to_le_bytes();
        let version_and_hash = [version_byte.to_vec(), hash_2.to_vec()].concat();

        let checksum = &sha256d(&version_and_hash)[..4];
        let address_bytes = [version_byte.to_vec(), hash_2.to_vec(), checksum.to_vec()].concat();

        Address(bs58::encode(address_bytes).into_string())
    }
}

#[derive(Clone)]
struct Sha256dHasher {}

impl rs_merkle::Hasher for Sha256dHasher {
    type Hash = [u8; 32];
    fn hash(data: &[u8]) -> Self::Hash {
        sha256d(data).into()
    }
}

#[derive(Clone)]
pub struct MerkleTree {
    tree: rs_merkle::MerkleTree<Sha256dHasher>,
}

impl MerkleTree {
    pub fn from_leaves(leaf_bytes: Vec<&[u8]>) -> Self {
        let leaves: Vec<[u8; 32]> = leaf_bytes.iter().map(|x| sha256d(x)).collect();
        Self {
            tree: rs_merkle::MerkleTree::<Sha256dHasher>::from_leaves(&leaves),
        }
    }

    pub fn root(&self) -> Option<Hash> {
        self.tree.root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_sign_and_verify_message() {
        let key_pair_a = KeyPair::generate();
        let key_pair_b = KeyPair::generate();

        let bytes = b"Hello, world!";

        let signature = key_pair_a.sign(bytes);
        let is_valid = signature.verify(bytes, &key_pair_a.public_key);

        assert!(is_valid);

        let expected_invalid = signature.verify(bytes, &key_pair_b.public_key);
        assert!(!expected_invalid);
    }

    #[test]
    fn test_address() {
        let pk_str = "035fe61fefdd77e3f8065c57ce7750d4b4aa7bc881ebb8875d1a211c28d08ca111";
        let pk = PublicKey::from_str(pk_str).unwrap();

        let pk_address = Address::from_public_key(&pk);
        assert_eq!(pk_address.0, "1KYYpnPHa2fpyfrGmug6pprexoJU74ihwW");
    }

    #[test]
    fn test_merkle_tree() {
        let leaves = vec![b"Hello, world!".as_slice(), b"Hello, world!".as_slice()];
        let tree = MerkleTree::from_leaves(leaves);

        let root = tree.root().unwrap();
        println!("Root: 0x{}", hex::encode(root));

        assert_eq!(
            root.to_vec(),
            hex::decode("9d6bf165d3b3552fcf9c4bd1fee36db5aca38d992a6aff5178c7aac79c6d715d")
                .unwrap()
        );
    }
}
