use std::collections::{HashMap, HashSet};

use anyhow::Result;
use bincode::Encode;
use secp256k1::{PublicKey, ecdsa::Signature};

use crate::{
    constants::{BLOCKS_PER_REWARD_HALVING, GENESIS_BLOCK_REWARD},
    crypto::{Address, Hash, KeyPair, MerkleTree, SignatureExt, sha256d},
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
    pub address: Address,
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

#[derive(Debug, Clone)]
pub struct SigningInfo {
    pub signature: Signature,
    pub public_key: PublicKey,
}

impl SigningInfo {
    pub fn sign(keypair: &KeyPair, bytes: &[u8]) -> Self {
        Self {
            signature: keypair.sign(&bytes),
            public_key: keypair.public_key,
        }
    }

    pub fn verify_bytes(&self, bytes: &[u8]) -> Result<bool> {
        Ok(self.signature.verify(bytes, &self.public_key))
    }

    pub fn address(&self) -> Address {
        Address::from_public_key(&self.public_key)
    }
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub body: TransactionBody,
    pub signing_info: SigningInfo,
}

impl Transaction {
    pub fn id(&self) -> Result<TxId> {
        self.body.id()
    }

    pub fn verify(&self) -> Result<bool> {
        self.signing_info.verify_bytes(&self.body.as_bytes()?)
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

    fn block_reward(height: u32) -> u64 {
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
}

#[derive(Debug, Clone, Default)]
pub struct UTXOSet {
    pub outputs: HashSet<TransactionOutputReference>,
}

impl UTXOSet {
    pub fn update(&mut self, transaction: &Transaction) -> Result<()> {
        let TransactionBody { input, outputs } = &transaction.body;

        if let TransactionInput::Reference(reference) = input {
            let removed = self.outputs.remove(&reference);
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
            self.outputs.insert(output);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransactionState {
    pub transactions: HashMap<TxId, Transaction>,
    pub uxto_set: UTXOSet,
}

impl TransactionState {
    pub fn add_transaction(&mut self, transaction: Transaction) -> Result<()> {
        let id = transaction.id()?;

        self.validate_transaction(&transaction)?;

        self.uxto_set.update(&transaction)?;
        self.transactions.insert(id, transaction);

        Ok(())
    }

    pub fn validate_transaction(&self, transaction: &Transaction) -> Result<()> {
        transaction.verify()?;

        let TransactionBody { input, outputs } = &transaction.body;

        if let TransactionInput::Reference(reference) = input {
            let Some(tx) = self.transactions.get(&reference.id) else {
                return Err(anyhow::anyhow!("Transaction output reference not found"));
            };

            let Some(output_ref) = self.uxto_set.outputs.get(&reference) else {
                return Err(anyhow::anyhow!("Transaction output already spent"));
            };

            let Some(output) = tx.body.outputs.get(output_ref.index) else {
                return Err(anyhow::anyhow!("Transaction output index not found"));
            };

            if output.address != tx.signing_info.address() {
                return Err(anyhow::anyhow!(
                    "Transaction output address does not match input address"
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::*;

    #[test]
    fn test_transaction() {
        let mut state = TransactionState::default();

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

        state.add_transaction(tx_a.clone()).unwrap();

        assert!(
            state
                .uxto_set
                .outputs
                .contains(&tx_a.output_reference(0).unwrap())
        );

        let keypair_alice = KeyPair::generate();
        let address_alice = Address::from_public_key(&keypair_alice.public_key);

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

        let tx_b = tx_b_body.into_tx(&keypair_bob).unwrap();

        state.add_transaction(tx_b.clone()).unwrap();

        assert!(
            !state
                .uxto_set
                .outputs
                .contains(&tx_a.output_reference(0).unwrap())
        );

        assert!(
            state
                .uxto_set
                .outputs
                .contains(&tx_b.output_reference(0).unwrap())
        );

        assert!(
            state
                .uxto_set
                .outputs
                .contains(&tx_b.output_reference(1).unwrap())
        );
    }
}
