use std::collections::{HashMap, HashSet};

use anyhow::Result;
use bincode::Encode;
use rs_merkle::MerkleTree;
use secp256k1::{PublicKey, SecretKey, ecdsa::Signature};

use crate::crypto::{
    Hash, KeyPair, Sha256dHasher, address, merkle_tree, sha256d, sign_message, verify_signature,
};

#[derive(Debug, Clone, Hash, Eq, PartialEq, Encode)]
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

#[derive(Debug, Clone, Hash, Eq, PartialEq, Encode)]
pub struct TransactionOutputReference {
    pub id: TxId,
    pub index: usize,
}

#[derive(Debug, Clone, Encode)]
pub enum TransactionInput {
    Coinbase { block_height: u32 },
    Reference(TransactionOutputReference),
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

    pub fn output_reference(&self, index: usize) -> Result<TransactionOutputReference> {
        if index >= self.body.outputs.len() {
            return Err(anyhow::anyhow!("Transaction output index out of bounds"));
        }

        Ok(TransactionOutputReference {
            id: self.id.clone(),
            index,
        })
    }

    pub fn new_coinbase(keypair: &KeyPair, block_height: u32, reward: u64) -> Result<Self> {
        let body = TransactionBody {
            input: TransactionInput::Coinbase { block_height },
            outputs: vec![TransactionOutput {
                value: reward,
                address: address(&keypair.public_key),
            }],
        };

        body.into_tx(&keypair.secret_key)
    }
}

#[derive(Debug, Clone, Default)]
pub struct UnspentTransactionOutput {
    pub output: HashSet<TransactionOutputReference>,
}

impl UnspentTransactionOutput {
    pub fn update(&mut self, transaction: &Transaction) -> Result<()> {
        let TransactionBody { input, outputs } = &transaction.body;

        if let TransactionInput::Reference(reference) = input {
            let removed = self.output.remove(&reference);
            if !removed {
                return Err(anyhow::anyhow!("Transaction output reference not found"));
            }
        }

        let new_unspent_outputs = outputs
            .iter()
            .enumerate()
            .map(|(index, _o)| transaction.output_reference(index))
            .collect::<Result<Vec<_>>>()?;

        for output in new_unspent_outputs {
            self.output.insert(output);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransactionState {
    pub transactions: HashMap<TxId, Transaction>,
    pub unspent_output_set: UnspentTransactionOutput,
}

impl TransactionState {
    pub fn add_transaction(
        &mut self,
        public_key: &PublicKey,
        transaction: Transaction,
    ) -> Result<()> {
        self.validate_transaction(public_key, &transaction)?;

        self.unspent_output_set.update(&transaction)?;

        self.transactions
            .insert(transaction.id.clone(), transaction);

        Ok(())
    }

    pub fn validate_transaction(
        &self,
        public_key: &PublicKey,
        transaction: &Transaction,
    ) -> Result<()> {
        transaction.verify(public_key)?;

        let TransactionBody { input, outputs } = &transaction.body;

        if let TransactionInput::Reference(reference) = input {
            let Some(tx) = self.transactions.get(&reference.id) else {
                return Err(anyhow::anyhow!("Transaction output reference not found"));
            };

            let Some(output_ref) = self.unspent_output_set.output.get(&reference) else {
                return Err(anyhow::anyhow!("Transaction output already spent"));
            };

            let Some(output) = tx.body.outputs.get(output_ref.index) else {
                return Err(anyhow::anyhow!("Transaction output index not found"));
            };

            if output.address != address(&public_key) {
                return Err(anyhow::anyhow!(
                    "Transaction output address does not match public key"
                ));
            }

            let tx_output_value = outputs.iter().map(|o| o.value).sum::<u64>();
            if tx_output_value != output.value {
                return Err(anyhow::anyhow!("Transaction output value does not match"));
            }
        }

        Ok(())
    }
}

pub fn build_merkle_tree(transactions: &Vec<Transaction>) -> Result<MerkleTree<Sha256dHasher>> {
    let leaves = transactions.iter().map(|tx| tx.id.0.as_slice()).collect();
    Ok(merkle_tree(leaves))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{KeyPair, address};

    #[test]
    fn test_transaction() {
        let mut state = TransactionState::default();

        let keypair_bob = KeyPair::generate();
        let address_bob = address(&keypair_bob.public_key);

        let tx_a_body = TransactionBody {
            input: TransactionInput::Coinbase { block_height: 0 },
            outputs: vec![TransactionOutput {
                value: 100,
                address: address_bob.clone(),
            }],
        };

        let tx_a = tx_a_body.into_tx(&keypair_bob.secret_key).unwrap();

        state
            .add_transaction(&keypair_bob.public_key, tx_a.clone())
            .unwrap();

        assert!(
            state
                .unspent_output_set
                .output
                .contains(&tx_a.output_reference(0).unwrap())
        );

        let keypair_alice = KeyPair::generate();
        let address_alice = address(&keypair_alice.public_key);

        let tx_b_body = TransactionBody {
            input: TransactionInput::Reference(tx_a.output_reference(0).unwrap()),
            outputs: vec![
                TransactionOutput {
                    value: 50,
                    address: address_alice,
                },
                TransactionOutput {
                    value: 50,
                    address: address_bob,
                },
            ],
        };

        let tx_b = tx_b_body.into_tx(&keypair_bob.secret_key).unwrap();

        state
            .add_transaction(&keypair_bob.public_key, tx_b.clone())
            .unwrap();

        assert!(
            !state
                .unspent_output_set
                .output
                .contains(&tx_a.output_reference(0).unwrap())
        );

        assert!(
            state
                .unspent_output_set
                .output
                .contains(&tx_b.output_reference(0).unwrap())
        );

        assert!(
            state
                .unspent_output_set
                .output
                .contains(&tx_b.output_reference(1).unwrap())
        );
    }
}
