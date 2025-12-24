use anyhow::Result;
use bincode::Encode;
use secp256k1::{PublicKey, ecdsa::Signature};
use serde::{Deserialize, Serialize};

use crate::{
    constants::{BLOCKS_PER_REWARD_HALVING, GENESIS_BLOCK_REWARD},
    crypto::{Address, Hash, KeyPair, MerkleTree, SignatureExt, sha256d},
};

#[derive(Clone, Hash, Eq, PartialEq, Encode, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TxId(pub Hash);

impl TxId {
    pub fn empty() -> Self {
        Self(Hash([0; 32]))
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl std::fmt::Debug for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TxId({})", self.to_string())
    }
}

#[derive(Debug, Clone, Encode, Serialize, Deserialize)]
pub struct TransactionOutput {
    pub value: u64,
    pub address: Address,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq, Encode, Serialize, Deserialize)]
pub struct TransactionOutputReference {
    pub id: TxId,
    pub index: usize,
}

#[derive(Debug, Clone, Encode, Serialize, Deserialize)]
pub enum TransactionInput {
    Coinbase { block_height: u32 },
    Reference(TransactionOutputReference),
}

impl TransactionInput {
    pub fn is_coinbase(&self) -> bool {
        matches!(self, TransactionInput::Coinbase { .. })
    }
}

#[derive(Debug, Clone, Encode, Serialize, Deserialize)]
pub struct TransactionBody {
    pub input: TransactionInput,
    pub outputs: Vec<TransactionOutput>,
}

impl TransactionBody {
    pub fn as_bytes(&self) -> Result<Vec<u8>> {
        Ok(bincode::encode_to_vec(self, bincode::config::standard())?)
    }

    pub fn id(&self) -> Result<TxId> {
        Ok(TxId(sha256d(&self.as_bytes()?)))
    }

    pub fn into_tx(self, keypair: &KeyPair) -> Result<Transaction> {
        Ok(Transaction {
            signing_info: SigningInfo::sign(keypair, &self.as_bytes()?),
            body: self,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningInfo {
    #[serde(
        serialize_with = "serialize_signature",
        deserialize_with = "deserialize_signature"
    )]
    pub signature: Signature,
    #[serde(
        serialize_with = "serialize_public_key",
        deserialize_with = "deserialize_public_key"
    )]
    pub public_key: PublicKey,
}

fn serialize_signature<S>(sig: &Signature, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bytes = sig.serialize_compact();
    serializer.serialize_str(&hex::encode(bytes))
}

fn deserialize_signature<'de, D>(deserializer: D) -> Result<Signature, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s = String::deserialize(deserializer)?;
    let bytes = hex::decode(&s)
        .map_err(|e| Error::custom(format!("Invalid hex string for signature: {}", e)))?;
    Signature::from_compact(&bytes).map_err(|e| Error::custom(format!("Invalid signature: {}", e)))
}

fn serialize_public_key<S>(pk: &PublicKey, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let bytes = pk.serialize();
    serializer.serialize_str(&hex::encode(bytes))
}

fn deserialize_public_key<'de, D>(deserializer: D) -> Result<PublicKey, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s = String::deserialize(deserializer)?;
    let bytes = hex::decode(&s)
        .map_err(|e| Error::custom(format!("Invalid hex string for public key: {}", e)))?;
    PublicKey::from_slice(&bytes).map_err(|e| Error::custom(format!("Invalid public key: {}", e)))
}

impl SigningInfo {
    pub fn sign(keypair: &KeyPair, bytes: &[u8]) -> Self {
        Self {
            signature: keypair.sign(&bytes),
            public_key: keypair.public_key,
        }
    }

    pub fn verify_signature_bytes(&self, bytes: &[u8]) -> Result<bool> {
        Ok(self.signature.verify(bytes, &self.public_key))
    }

    pub fn address(&self) -> Address {
        Address::from_public_key(&self.public_key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub body: TransactionBody,
    pub signing_info: SigningInfo,
}

impl Transaction {
    pub fn id(&self) -> Result<TxId> {
        self.body.id()
    }

    pub fn verify_signature(&self) -> Result<bool> {
        self.signing_info
            .verify_signature_bytes(&self.body.as_bytes()?)
    }

    pub fn output_reference(&self, index: usize) -> Result<TransactionOutputReference> {
        if index >= self.body.outputs.len() {
            return Err(anyhow::anyhow!("Transaction output index out of bounds"));
        }

        Ok(TransactionOutputReference {
            id: self.id()?,
            index,
        })
    }

    pub fn block_reward(height: u32) -> u64 {
        GENESIS_BLOCK_REWARD as u64 / 2u32.pow(height / BLOCKS_PER_REWARD_HALVING) as u64
    }

    pub fn new_coinbase(keypair: &KeyPair, block_height: u32) -> Result<Self> {
        let value = Self::block_reward(block_height);

        let body = TransactionBody {
            input: TransactionInput::Coinbase { block_height },
            outputs: vec![TransactionOutput {
                value,
                address: Address::from_public_key(&keypair.public_key),
            }],
        };

        body.into_tx(&keypair)
    }

    pub fn build_merkle_tree(transactions: &Vec<Self>) -> Result<MerkleTree> {
        let tx_ids = transactions
            .iter()
            .map(|tx| tx.id())
            .collect::<Result<Vec<_>>>()?;

        let leaves = tx_ids.iter().map(|id| id.0.as_slice()).collect();

        Ok(MerkleTree::from_leaves(leaves))
    }

    pub fn validate(&self) -> Result<()> {
        self.verify_signature()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::*;

    #[test]
    fn test_transaction() {
        let keypair_bob = KeyPair::generate();
        let address_bob = Address::from_public_key(&keypair_bob.public_key);

        let tx_a_body = TransactionBody {
            input: TransactionInput::Coinbase { block_height: 0 },
            outputs: vec![TransactionOutput {
                value: 100,
                address: address_bob.clone(),
            }],
        };

        let tx_a = tx_a_body.into_tx(&keypair_bob).unwrap();

        assert!(tx_a.verify_signature().unwrap());
    }
}
