use anyhow::Result;

use crate::{transaction::Transaction, utxo_set::UTXOSet};

#[derive(Debug, Clone, Default)]
pub struct MemPool {
    pub pending_transactions: Vec<Transaction>,
}

impl MemPool {
    pub fn add(&mut self, utxo_set: &UTXOSet, transaction: Transaction) -> Result<()> {
        let mut pending_utxo_set = utxo_set.clone();
        for tx in self.pending_transactions.iter() {
            pending_utxo_set.update(tx)?;
        }

        pending_utxo_set.validate_transaction(&transaction)?;
        self.pending_transactions.push(transaction);

        Ok(())
    }

    pub fn drain(&mut self) -> Vec<Transaction> {
        self.pending_transactions.drain(..).collect()
    }
}
