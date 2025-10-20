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

pub fn generate_key_pair() -> KeyPair {
    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::rng());
    KeyPair {
        secret_key,
        public_key,
    }
}

pub fn sign_message(message: &[u8], secret_key: &SecretKey) -> Signature {
    let secp = Secp256k1::new();
    let digest = sha256d(message);
    let message = Message::from_digest(digest);

    let signature = secp.sign_ecdsa(message, secret_key);

    signature
}

pub fn verify_signature(message: &[u8], signature: &Signature, public_key: &PublicKey) -> bool {
    let secp = Secp256k1::verification_only();
    let digest = sha256d(message);
    let message = Message::from_digest(digest);

    secp.verify_ecdsa(message, &signature, &public_key).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_and_verify_message() {
        let key_pair_a = generate_key_pair();
        let key_pair_b = generate_key_pair();

        let message = b"Hello, world!";
        let signature = sign_message(message, &key_pair_a.secret_key);
        let is_valid = verify_signature(message, &signature, &key_pair_a.public_key);

        assert!(is_valid);

        let expected_invalid = verify_signature(message, &signature, &key_pair_b.public_key);
        assert!(!expected_invalid);
    }
}
