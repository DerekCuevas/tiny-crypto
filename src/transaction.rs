use anyhow::Result;
use bincode::Encode;
use secp256k1::{PublicKey, SecretKey, ecdsa::Signature};

use crate::crypto::{Hash, sha256d, sign_message, verify_signature};

#[derive(Debug, Clone, Encode)]
pub struct TxId(pub Hash);

impl TxId {
    pub fn empty() -> Self {
        Self([0; 32])
    }
}

#[derive(Debug, Clone, Encode)]
pub struct TransactionOutput {
    pub value: u64,
    pub address: String,
}

#[derive(Debug, Clone, Encode)]
pub enum TransactionInput {
    Coinbase,
    TransactionOutputReference { id: TxId, index: u32 },
}

#[derive(Debug, Clone, Encode)]
pub struct TransactionBody {
    pub input: TransactionInput,
    pub outputs: Vec<TransactionOutput>,
}

impl TransactionBody {
    pub fn as_bytes(&self) -> Result<Vec<u8>> {
        let bytes = bincode::encode_to_vec(self, bincode::config::standard())?;
        Ok(bytes)
    }

    pub fn id(&self) -> Result<TxId> {
        let hash = sha256d(&self.as_bytes()?);
        Ok(TxId(hash))
    }

    pub fn sign(&self, secret_key: &SecretKey) -> Result<Signature> {
        Ok(sign_message(&self.as_bytes()?, secret_key))
    }

    pub fn into_tx(self, secret_key: &SecretKey) -> Result<Transaction> {
        Ok(Transaction {
            id: self.id()?,
            signature: self.sign(secret_key)?,
            body: self,
        })
    }

    pub fn validate(&self) -> Result<bool> {
        // UTXO validation (no double spends)
        // Input validation (creator is the owner of the previous output - address matches public key)
        // Output (amount) validation
        todo!()
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: TxId,
    pub body: TransactionBody,
    pub signature: Signature,
}

impl Transaction {
    pub fn verify(&self, public_key: &PublicKey) -> Result<bool> {
        Ok(verify_signature(
            &self.body.as_bytes()?,
            &self.signature,
            public_key,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyPair, address};

    #[test]
    fn test_transaction() {
        let keypair_bob = KeyPair::generate();

        let tx_a_body = TransactionBody {
            input: TransactionInput::Coinbase,
            outputs: vec![TransactionOutput {
                value: 100,
                address: address(&keypair_bob.public_key),
            }],
        };

        let tx_a = tx_a_body.into_tx(&keypair_bob.secret_key).unwrap();

        let is_valid = tx_a.verify(&keypair_bob.public_key).unwrap();
        assert!(is_valid);

        let keypair_alice = KeyPair::generate();

        let tx_b_body = TransactionBody {
            input: TransactionInput::TransactionOutputReference {
                id: tx_a.id,
                index: 0,
            },
            outputs: vec![
                TransactionOutput {
                    value: 50,
                    address: address(&keypair_alice.public_key),
                },
                TransactionOutput {
                    value: 50,
                    address: address(&keypair_bob.public_key),
                },
            ],
        };

        let tx_b = tx_b_body.into_tx(&keypair_bob.secret_key).unwrap();

        dbg!(&tx_b);
    }
}
