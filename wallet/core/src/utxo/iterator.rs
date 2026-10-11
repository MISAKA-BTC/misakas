//!
//! Associative iterator over the UTXO set.
//!

use crate::utxo::{UtxoContext, UtxoEntryReference};

pub struct UtxoIterator {
    entries: Vec<UtxoEntryReference>,
    cursor: usize,
    context: UtxoContext,
}

impl UtxoIterator {
    pub fn new(utxo_context: &UtxoContext) -> Self {
        utxo_context.refresh_mature_coinbases();
        Self { entries: utxo_context.context().mature.clone(), cursor: 0, context: utxo_context.clone() }
    }
}

impl Iterator for UtxoIterator {
    type Item = UtxoEntryReference;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.entries.get(self.cursor)?.clone();
            self.cursor += 1;
            if self.context.is_entry_spendable(&entry) {
                return Some(entry);
            }
        }
    }
}

impl std::fmt::Debug for UtxoIterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UtxoIterator").field("entries", &self.entries).field("cursor", &self.cursor).finish()
    }
}
