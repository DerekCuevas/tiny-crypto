use anyhow::Result;
use std::{collections::HashMap, sync::Arc};

use crate::transaction::{
    Transaction, TransactionBody, TransactionInput, TransactionOutputReference,
};

#[derive(Debug, Clone, Default)]
pub struct UTXOSet {
    pub outputs: HashMap<TransactionOutputReference, Arc<Transaction>>,
}

impl UTXOSet {
    pub fn update(&mut self, transaction: &Transaction) -> Result<()> {
        let transaction = Arc::new(transaction.clone());

        let TransactionBody { input, outputs } = &transaction.body;

        if let TransactionInput::Reference(reference) = input {
            let removed = self.outputs.remove(&reference);
            if removed.is_none() {
                return Err(anyhow::anyhow!("Transaction output reference not found"));
            }
        }

        let new_unspent_outputs = outputs
            .iter()
            .enumerate()
            .map(|(index, _o)| transaction.output_reference(index))
            .collect::<Result<Vec<_>>>()?;

        for output in new_unspent_outputs {
            self.outputs.insert(output, transaction.clone());
        }

        Ok(())
    }

    pub fn validate_transaction(&self, transaction: &Transaction) -> Result<bool> {
        transaction.verify_signature()?;

        let TransactionBody { input, outputs } = &transaction.body;

        if let TransactionInput::Reference(reference) = input {
            let Some(spent_tx) = self.outputs.get(reference) else {
                return Err(anyhow::anyhow!("Transaction output already spent"));
            };

            let Some(output) = spent_tx.body.outputs.get(reference.index) else {
                return Err(anyhow::anyhow!("Transaction output index not found"));
            };

            if output.address != transaction.signing_info.address() {
                return Err(anyhow::anyhow!(
                    "Transaction not signed by owner of output address"
                ));
            }

            let tx_output_value = outputs.iter().map(|o| o.value).sum::<u64>();
            if tx_output_value != output.value {
                return Err(anyhow::anyhow!("Transaction output value does not match"));
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::*;
    use crate::transaction::*;

    #[test]
    fn test_utxo_set() {
        let mut utxo_set = UTXOSet::default();

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

        utxo_set.update(&tx_a).unwrap();

        assert!(
            utxo_set
                .outputs
                .contains_key(&tx_a.output_reference(0).unwrap())
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

        utxo_set.update(&tx_b).unwrap();

        assert!(
            !utxo_set
                .outputs
                .contains_key(&tx_a.output_reference(0).unwrap())
        );

        assert!(
            utxo_set
                .outputs
                .contains_key(&tx_b.output_reference(0).unwrap())
        );

        assert!(
            utxo_set
                .outputs
                .contains_key(&tx_b.output_reference(1).unwrap())
        );
    }
}
