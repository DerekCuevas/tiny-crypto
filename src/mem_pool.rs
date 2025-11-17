use anyhow::Result;

use crate::{transaction::Transaction, utxo_set::UTXOSet};

#[derive(Debug, Clone)]
pub struct MemPool {
    pub size: usize,
    pub pending_transactions: Vec<Transaction>,
}

impl MemPool {
    pub fn new(size: usize) -> Self {
        Self {
            size,
            pending_transactions: Vec::with_capacity(size),
        }
    }

    pub fn is_full(&self) -> bool {
        self.pending_transactions.len() >= self.size
    }

    pub fn add(&mut self, utxo_set: &UTXOSet, transaction: Transaction) -> Result<()> {
        if self.is_full() {
            return Err(anyhow::anyhow!("MemPool is full"));
        }

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
