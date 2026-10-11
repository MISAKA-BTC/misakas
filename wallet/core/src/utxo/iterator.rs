//!
//! Associative iterator over the UTXO set.
//!

use crate::utxo::{UtxoContext, UtxoEntryReference};
use kaspa_consensus_core::network::NetworkId;

pub struct UtxoIterator {
    entries: Vec<UtxoEntryReference>,
    cursor: usize,
    context: UtxoContext,
    network_id: Option<NetworkId>,
    generation: u64,
}

impl UtxoIterator {
    pub fn new(utxo_context: &UtxoContext) -> Self {
        let generation = utxo_context.processor().utxo_observation_generation();
        let network_id = utxo_context.processor().network_id().ok();
        utxo_context.refresh_mature_coinbases();
        Self {
            entries: utxo_context.context().mature.clone(), cursor: 0, context: utxo_context.clone(),
            network_id, generation,
        }
    }
}

impl Iterator for UtxoIterator {
    type Item = UtxoEntryReference;

    fn next(&mut self) -> Option<Self::Item> {
        if self.context.processor().network_id().ok() != self.network_id
            || self.context.processor().utxo_observation_generation() != self.generation
        {
            return None;
        }
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
