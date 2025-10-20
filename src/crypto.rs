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
}

pub fn sign_message(message: &[u8], secret_key: &SecretKey) -> Signature {
    let secp = Secp256k1::new();
    let digest = sha256d(message);
    let message = Message::from_digest(digest);

    secp.sign_ecdsa(message, secret_key)
}

pub fn verify_signature(message: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
    let secp = Secp256k1::verification_only();
    let digest = sha256d(message);
    let message = Message::from_digest(digest);

    secp.verify_ecdsa(message, &signature, &public_key).is_ok()
}

pub fn address(public_key: &PublicKey) -> String {
    let hash_1 = Sha256::digest(public_key.serialize_uncompressed());

    let mut ripemd_hasher = Ripemd160::new();
    ripemd_hasher.update(hash_1);
    let hash_2 = ripemd_hasher.finalize();

    let version_byte = 0u8.to_le_bytes();
    let version_and_hash = [version_byte.to_vec(), hash_2.to_vec()].concat();

    let checksum = &sha256d(&version_and_hash)[..4];
    let address_bytes = [version_byte.to_vec(), hash_2.to_vec(), checksum.to_vec()].concat();

    bs58::encode(address_bytes).into_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_sign_and_verify_message() {
        let key_pair_a = KeyPair::generate();
        let key_pair_b = KeyPair::generate();

        let message = b"Hello, world!";
        let signature = sign_message(message, &key_pair_a.secret_key);
        let is_valid = verify_signature(message, &signature, &key_pair_a.public_key);

        assert!(is_valid);

        let expected_invalid = verify_signature(message, &signature, &key_pair_b.public_key);
        assert!(!expected_invalid);
    }

    #[test]
    fn test_address() {
        let pk_str = "035fe61fefdd77e3f8065c57ce7750d4b4aa7bc881ebb8875d1a211c28d08ca111";
        let pk = PublicKey::from_str(pk_str).unwrap();

        let pk_address = address(&pk);
        assert_eq!(pk_address, "1KYYpnPHa2fpyfrGmug6pprexoJU74ihwW");
    }
}
