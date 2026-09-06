use kaspa_consensus_core::BlockHash; // PR-9.5e: tips are block hashes (Hash64)
use kaspa_consensus_core::{
    BlockHashSet, HashMapCustomHasher,
    tx::{TransactionOutpoint, UtxoEntry},
    utxo::utxo_diff::UtxoDiff,
};
use kaspa_utils::hashmap::NestedHashMapExtensions;

use crate::model::{CirculatingSupplyDiff, CompactUtxoEntry, UtxoChanges, UtxoSetByScriptPublicKey};

/// A struct holding all changes to the utxoindex with on-the-fly conversions and processing.
pub struct UtxoIndexChanges {
    pub utxo_changes: UtxoChanges,
    pub supply_change: CirculatingSupplyDiff,
    pub tips: BlockHashSet,
}

impl UtxoIndexChanges {
    /// Create a new [`UtxoIndexChanges`] struct
    pub fn new() -> Self {
        Self {
            utxo_changes: UtxoChanges::new(UtxoSetByScriptPublicKey::new(), UtxoSetByScriptPublicKey::new()),
            supply_change: 0,
            tips: BlockHashSet::new(),
        }
    }

    /// **ADR-0087 Decision 3 / ADR-0089 Decision 8 (mainnet audit 2026-09-06, M-12): a model
    /// market's sink is in the UTXO set and can never be spent.**
    ///
    /// `OP_RETURN "MSKMDL01" <line id>` is dead by script — ADR-0089 Decision 8 says "the sink
    /// outputs on the UTXO side are dead by script" — and its value is already counted by the fold
    /// as `msk_reserve`, from which the market pays out through the coinbase. Counting it as
    /// circulating supply as well double-counts it and makes `getCoinSupply` over-report by the
    /// total ever sunk, which grows monotonically with every buy and every seed and never falls.
    ///
    /// Recognised by its EXACT script, never by its shape — the same reader consensus and the
    /// mempool use — so no other `OP_RETURN` form and no unspendable-looking script rides along.
    /// Returns `false` on every network today: no chain has a market (the fence is `None` on every
    /// shipped preset), so this is a no-op until one does.
    fn is_dead_by_script(entry: &UtxoEntry) -> bool {
        kaspa_consensus_core::palw_model_market_v1::palw_model_sink_class_v1(&entry.script_public_key).is_some()
    }

    /// Add a [`UtxoDiff`] the [`UtxoIndexChanges`] struct.
    pub fn update_utxo_diff(&mut self, utxo_diff: UtxoDiff) {
        let (to_add, to_remove) = (utxo_diff.add, utxo_diff.remove);

        for (transaction_outpoint, utxo_entry) in to_add.into_iter() {
            if !Self::is_dead_by_script(&utxo_entry) {
                self.supply_change += utxo_entry.amount as CirculatingSupplyDiff;
            }

            self.utxo_changes.added.insert_into_nested(
                utxo_entry.script_public_key,
                transaction_outpoint,
                CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
            );
        }

        for (transaction_outpoint, utxo_entry) in to_remove.into_iter() {
            // The same guard as the add loop, and not optional: a sink is never spent, so it should
            // never reach here — but if the two loops disagreed, the incremental total and the
            // resync total would diverge, which is one rule spelled two ways.
            if !Self::is_dead_by_script(&utxo_entry) {
                self.supply_change -= utxo_entry.amount as CirculatingSupplyDiff;
            }

            self.utxo_changes.removed.insert_into_nested(
                utxo_entry.script_public_key,
                transaction_outpoint,
                CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
            );
        }
    }

    /// Add a [`Vec<(TransactionOutpoint, UtxoEntry)>`] the [`UtxoIndexChanges`] struct
    ///
    /// Note: This is meant to be used when resyncing.
    pub fn add_utxos_from_vector(&mut self, utxo_vector: Vec<(TransactionOutpoint, UtxoEntry)>) {
        for (transaction_outpoint, utxo_entry) in utxo_vector.into_iter() {
            // The resync path counts what the incremental path counts, or a resynced node reports a
            // different supply from the node beside it (audit M-12).
            if !Self::is_dead_by_script(&utxo_entry) {
                self.supply_change += utxo_entry.amount as CirculatingSupplyDiff;
            }

            self.utxo_changes.added.insert_into_nested(
                utxo_entry.script_public_key,
                transaction_outpoint,
                CompactUtxoEntry::new(utxo_entry.amount, utxo_entry.block_daa_score, utxo_entry.is_coinbase),
            );
        }
    }

    pub fn set_tips(&mut self, tips: Vec<BlockHash>) {
        self.tips = BlockHashSet::from_iter(tips);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        Hash64,
        palw_model_market_v1::palw_model_sink_spk_v1,
        tx::{ScriptPublicKey, TransactionOutpoint},
        utxo::utxo_collection::UtxoCollection,
    };

    fn spendable_spk() -> ScriptPublicKey {
        // Any ordinary output; its exact class does not matter, only that it is not the sink.
        ScriptPublicKey::new(0, vec![0x51u8; 8].into())
    }

    fn outpoint(word: u64) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_u64_word(word), 0)
    }

    fn entry(amount: u64, spk: ScriptPublicKey) -> UtxoEntry {
        UtxoEntry::new(amount, spk, 1, false)
    }

    /// **Circulating supply counts what can be spent** (mainnet audit 2026-09-06, M-12).
    ///
    /// A model market's sink (`OP_RETURN "MSKMDL01" <line id>`) is in the UTXO set and is dead by
    /// script — ADR-0089 Decision 8 — and the fold already counts its value as the market's
    /// `msk_reserve`, out of which the coinbase pays. Counting it here as well double-counts it and
    /// makes `getCoinSupply` over-report by the total ever sunk, a figure that only ever grows.
    ///
    /// The sink is still INDEXED: an output nobody can spend is still an output a wallet or an
    /// explorer may want to see. Only the supply accumulator skips it.
    #[test]
    fn a_dead_sink_is_indexed_but_not_counted_as_supply() {
        let sink_spk = palw_model_sink_spk_v1(&Hash64::from_u64_word(7));
        let live = (outpoint(1), entry(1_000, spendable_spk()));
        let sunk = (outpoint(2), entry(5_000, sink_spk.clone()));

        let mut add = UtxoCollection::new();
        add.insert(live.0, live.1.clone());
        add.insert(sunk.0, sunk.1.clone());
        let mut incremental = UtxoIndexChanges::new();
        incremental.update_utxo_diff(UtxoDiff { add, remove: UtxoCollection::new() });

        assert_eq!(incremental.supply_change, 1_000, "the sunk 5,000 is not circulating supply");
        assert!(
            incremental.utxo_changes.added.get(&live.1.script_public_key).is_some_and(|m| m.contains_key(&live.0)),
            "the spendable output is indexed"
        );
        assert!(
            incremental.utxo_changes.added.get(&sink_spk).is_some_and(|m| m.contains_key(&sunk.0)),
            "and so is the sink — it is excluded from the SUPPLY, not from the index"
        );

        // The resync path must reach the same number, or a resynced node reports a different
        // supply from the node beside it.
        let mut resync = UtxoIndexChanges::new();
        resync.add_utxos_from_vector(vec![(live.0, live.1.clone()), (sunk.0, sunk.1.clone())]);
        assert_eq!(resync.supply_change, incremental.supply_change, "one rule, one spelling, both paths");

        // ..and spending the live output removes exactly what it added, so the two loops agree.
        let mut remove = UtxoCollection::new();
        remove.insert(live.0, live.1.clone());
        remove.insert(sunk.0, sunk.1.clone());
        let mut spent = UtxoIndexChanges::new();
        spent.update_utxo_diff(UtxoDiff { add: UtxoCollection::new(), remove });
        assert_eq!(spent.supply_change, -1_000, "what was never counted in is never counted out");
    }
}
