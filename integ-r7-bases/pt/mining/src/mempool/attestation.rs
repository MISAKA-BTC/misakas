//! kaspa-pq DNS-finality mempool attestation index & policy.
//!
//! Live-testnet incident: `StakeAttestationShard` transactions (subnetwork `0x11`) submitted by
//! the in-process validator over RPC are admitted at [`Priority::High`]. High-priority mempool txs
//! never expire, so stale attestation shards accumulated indefinitely; the block-template selector
//! kept picking the stale shards while fresh attestations were never mined, and DNS finality
//! (`dns_confirmed`) could not recover.
//!
//! This module provides the **local mempool/mining policy** that lets attestation shards expire,
//! dedup by `(bond, validator, epoch)`, and have current/recent-epoch shards mined first. It is
//! consensus-neutral: nothing here changes transaction validity, wire formats, signatures or block
//! validation. Every code path is gated on [`AttestationMempoolPolicy::enabled`], which is `false`
//! by default and only `true` when the chain carries `dns_params`.

use crate::mempool::tx::Priority;
use kaspa_consensus_core::{
    Hash64,
    dns_finality::StakeAttestationShardPayload,
    subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
    tx::{MutableTransaction, TransactionId, TransactionOutpoint},
};
use smallvec::SmallVec;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
};

/// Identity of a single attestation within a shard: the bond it spends "depth" from, the attesting
/// validator, and the epoch it attests. Two shards sharing any key attest the same `(bond,
/// validator, epoch)` triple and are therefore mempool duplicates / replacements of each other.
///
/// `TransactionOutpoint` does not derive `Ord`, so `Ord`/`PartialOrd` are implemented by hand over
/// the outpoint's `(transaction_id, index)` followed by the remaining fields. The total order is
/// only used for deterministic tie-breaking, not for any consensus decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AttestationKey {
    pub bond_outpoint: TransactionOutpoint,
    pub validator_id: Hash64,
    pub epoch: u64,
}

impl PartialOrd for AttestationKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AttestationKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // TransactionOutpoint is not Ord; order it lexicographically by (transaction_id, index).
        self.bond_outpoint
            .transaction_id
            .cmp(&other.bond_outpoint.transaction_id)
            .then(self.bond_outpoint.index.cmp(&other.bond_outpoint.index))
            .then(self.validator_id.cmp(&other.validator_id))
            .then(self.epoch.cmp(&other.epoch))
    }
}

/// Per-shard metadata extracted from an attestation-shard mempool transaction. Holds exactly the
/// fields the policy needs (epoch, anchor tuple, the per-attestation keys, fee/mass/feerate) so the
/// policy never has to re-decode the borsh payload after ingestion.
///
/// Some fields (`added_at_daa_score`, `mass`, `priority`) are carried for completeness/diagnostics
/// and future policy refinements even though the current P0/P1 paths read mass from the frontier
/// key; `#[allow(dead_code)]` keeps them part of the struct without warnings.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct AttestationMempoolMeta {
    pub tx_id: TransactionId,
    /// The shard-level epoch (`StakeAttestationShardPayload::epoch`). Used for TTL/selection.
    pub shard_epoch: u64,
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    pub validator_set_commitment: Hash64,
    /// One key per contained attestation. Usually a single attestation per shard (the in-process
    /// validator case), hence the inline-1 `SmallVec`.
    pub keys: SmallVec<[AttestationKey; 1]>,
    pub added_at_daa_score: u64,
    pub fee: u64,
    pub mass: u64,
    pub feerate: f64,
    pub priority: Priority,
}

impl AttestationMempoolMeta {
    /// The `(epoch, target_hash, target_daa_score, validator_set_commitment)` anchor tuple two
    /// shards must share to be considered "the same attestation" (vs. potential equivocation).
    pub fn anchor_tuple(&self) -> (u64, Hash64, u64, Hash64) {
        (self.shard_epoch, self.target_hash, self.target_daa_score, self.validator_set_commitment)
    }
}

/// Decode an attestation-shard mempool transaction into [`AttestationMempoolMeta`].
///
/// - Returns `None` when `mtx` is not a `StakeAttestationShard` tx (the common, hot case).
/// - Returns `Some(Err(msg))` when the subnetwork matches but the borsh payload fails to decode.
/// - Returns `Some(Ok(meta))` otherwise.
///
/// The transaction is expected to be fully populated (fee + masses) — it is decoded only after
/// mempool pre-validation. If the fee/masses are unexpectedly absent we fall back to `0`/`0.0`
/// rather than panicking, since the index is best-effort policy state, not a consensus invariant.
pub(crate) fn extract_attestation_meta(
    mtx: &MutableTransaction,
    added_at_daa_score: u64,
    priority: Priority,
) -> Option<Result<AttestationMempoolMeta, String>> {
    if mtx.tx.subnetwork_id != SUBNETWORK_ID_STAKE_ATTESTATION_SHARD {
        return None;
    }

    let payload: StakeAttestationShardPayload = match borsh::from_slice(&mtx.tx.payload) {
        Ok(payload) => payload,
        Err(err) => {
            return Some(Err(format!("failed to borsh-decode StakeAttestationShardPayload: {err}")));
        }
    };

    let keys: SmallVec<[AttestationKey; 1]> = payload
        .attestations
        .iter()
        .map(|att| AttestationKey { bond_outpoint: att.bond_outpoint, validator_id: att.validator_id, epoch: att.epoch })
        .collect();

    let fee = mtx.calculated_fee.unwrap_or(0);
    let mass = mtx.calculated_non_contextual_masses.map(|m| m.max()).unwrap_or(0);
    let feerate = mtx.calculated_feerate().unwrap_or(0.0);

    Some(Ok(AttestationMempoolMeta {
        tx_id: mtx.id(),
        shard_epoch: payload.epoch,
        target_hash: payload.target_hash,
        target_daa_score: payload.target_daa_score,
        validator_set_commitment: payload.validator_set_commitment,
        keys,
        added_at_daa_score,
        fee,
        mass,
        feerate,
        priority,
    }))
}

/// In-memory index of attestation-shard mempool transactions, kept consistent with the transaction
/// pool. Three views over the same set of metas:
///
/// - `by_txid`: the authoritative store (one entry per attestation-shard tx in the pool).
/// - `by_key`: maps each `(bond, validator, epoch)` key to the owning tx (for dedup / replacement).
/// - `by_epoch`: groups tx ids by shard epoch (for TTL sweeps and recent-window selection).
#[derive(Default)]
pub(crate) struct AttestationIndex {
    pub by_txid: HashMap<TransactionId, AttestationMempoolMeta>,
    pub by_key: HashMap<AttestationKey, TransactionId>,
    pub by_epoch: BTreeMap<u64, HashSet<TransactionId>>,
}

impl AttestationIndex {
    pub fn len(&self) -> usize {
        self.by_txid.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_txid.is_empty()
    }

    #[allow(dead_code)]
    pub fn contains(&self, tx_id: &TransactionId) -> bool {
        self.by_txid.contains_key(tx_id)
    }

    pub fn get(&self, tx_id: &TransactionId) -> Option<&AttestationMempoolMeta> {
        self.by_txid.get(tx_id)
    }

    /// Returns the tx currently owning `key`, if any.
    pub fn owner_of_key(&self, key: &AttestationKey) -> Option<TransactionId> {
        self.by_key.get(key).copied()
    }

    /// Insert a meta, keeping all three maps consistent. Existing `by_key` ownership for any of the
    /// meta's keys is overwritten (callers resolve replacement/dedup before inserting), so the index
    /// never points a key at a tx no longer in `by_txid`.
    pub fn insert(&mut self, meta: AttestationMempoolMeta) {
        let tx_id = meta.tx_id;
        for key in meta.keys.iter() {
            self.by_key.insert(*key, tx_id);
        }
        self.by_epoch.entry(meta.shard_epoch).or_default().insert(tx_id);
        self.by_txid.insert(tx_id, meta);
    }

    /// Remove a tx from all three maps. A `by_key` entry is cleared only if it still points at this
    /// tx (a newer replacement may already own the key). No-op if the tx is not indexed.
    pub fn remove(&mut self, tx_id: &TransactionId) -> Option<AttestationMempoolMeta> {
        let meta = self.by_txid.remove(tx_id)?;
        for key in meta.keys.iter() {
            if self.by_key.get(key) == Some(tx_id) {
                self.by_key.remove(key);
            }
        }
        if let Some(set) = self.by_epoch.get_mut(&meta.shard_epoch) {
            set.remove(tx_id);
            if set.is_empty() {
                self.by_epoch.remove(&meta.shard_epoch);
            }
        }
        Some(meta)
    }

    /// Collect tx ids of attestation shards whose `shard_epoch` is older than a hard-retention
    /// cutoff: `latest_ready_epoch - shard_epoch > hard_retention_epochs`. These expire **even if
    /// high priority** — the whole point of the fix.
    pub fn collect_hard_expired(&self, latest_ready_epoch: u64, hard_retention_epochs: u64) -> Vec<TransactionId> {
        let mut expired = Vec::new();
        for (&epoch, tx_ids) in self.by_epoch.iter() {
            // by_epoch is sorted ascending; once we reach an epoch within retention we can stop.
            if latest_ready_epoch.saturating_sub(epoch) <= hard_retention_epochs {
                break;
            }
            expired.extend(tx_ids.iter().copied());
        }
        expired
    }
}

/// kaspa-pq audit v26 (H-4): why an attestation shard was quarantined. Today the only
/// producer is the template classifier's transient (`Quarantine`) drop, but the reason is
/// recorded so diagnostics / future policy can distinguish quarantine causes without
/// re-deriving them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuarantineReason {
    /// The shard was structurally fine but not eligible against a template's selected-parent
    /// bond view (a reorg / a few more blocks could make it eligible). Recorded so the shard
    /// is held out of the priority/inner selector lanes until `until_epoch`, instead of being
    /// re-selected into every subsequent template (the live-testnet stall), without hard-evicting
    /// a recoverable bond.
    TemplateTransient,
}

/// kaspa-pq audit v26 (H-4): a single quarantine record — why a shard is held out and the
/// (exclusive) epoch at which the hold lapses.
#[derive(Clone, Copy, Debug)]
pub(crate) struct QuarantineEntry {
    #[allow(dead_code)]
    pub reason: QuarantineReason,
    /// The shard is quarantined while `current_epoch < until_epoch`; once
    /// `current_epoch >= until_epoch` the entry is lazily evicted and the shard becomes
    /// re-selectable.
    pub until_epoch: u64,
}

/// kaspa-pq audit v26 (H-4): a short-term quarantine map for attestation shards the template
/// classifier dropped transiently. Holding a shard out of selection for a small number of
/// epochs (`AttestationMempoolPolicy::quarantine_epochs`) breaks the "drop -> re-select ->
/// drop" loop without hard-evicting a bond that a reorg / more blocks could make eligible.
///
/// Empty / inert whenever the attestation overlay is off (nothing is ever inserted), so
/// non-overlay nets are byte-identical.
#[derive(Default)]
pub(crate) struct AttestationQuarantine {
    entries: HashMap<TransactionId, QuarantineEntry>,
}

impl AttestationQuarantine {
    /// Quarantine `tx_id` for `reason` until `until_epoch` (exclusive). Overwrites any prior
    /// entry for the same tx (the latest template view wins).
    pub fn insert(&mut self, tx_id: TransactionId, reason: QuarantineReason, until_epoch: u64) {
        self.entries.insert(tx_id, QuarantineEntry { reason, until_epoch });
    }

    /// Whether `tx_id` is currently quarantined at `current_epoch`. Lazily evicts (and reports
    /// `false` for) an entry whose hold has lapsed (`current_epoch >= until_epoch`), so a
    /// recovered bond becomes re-selectable without waiting for an explicit sweep. Takes `&mut`
    /// for the lazy eviction.
    ///
    /// The hot read path (the `&self` selector builders) uses the non-mutating [`Self::is_active`]
    /// instead, with eviction handled by [`Self::retain_active`]; this `&mut` lazy-evict variant is
    /// kept as part of the H-4 API surface (and exercised by tests).
    #[allow(dead_code)]
    pub fn is_quarantined(&mut self, tx_id: &TransactionId, current_epoch: u64) -> bool {
        match self.entries.get(tx_id) {
            Some(entry) if current_epoch < entry.until_epoch => true,
            Some(_) => {
                // Hold lapsed — evict lazily and report not-quarantined.
                self.entries.remove(tx_id);
                false
            }
            None => false,
        }
    }

    /// Immutable variant of [`Self::is_quarantined`] for read-only (`&self`) call sites such as
    /// the template selector builders: reports whether the hold is still active at
    /// `current_epoch` WITHOUT lazily evicting a lapsed entry. Lapsed entries are reaped by
    /// [`Self::retain_active`] (and by [`Self::is_quarantined`] on the `&mut` paths), so this
    /// only ever returns `true` while the hold is genuinely active.
    pub fn is_active(&self, tx_id: &TransactionId, current_epoch: u64) -> bool {
        matches!(self.entries.get(tx_id), Some(entry) if current_epoch < entry.until_epoch)
    }

    /// Drop every entry whose hold has lapsed at `current_epoch`. Called on new block / TTL
    /// sweep so the map does not accumulate stale entries.
    pub fn retain_active(&mut self, current_epoch: u64) {
        self.entries.retain(|_, entry| current_epoch < entry.until_epoch);
    }

    /// Remove a specific entry (keeps the map consistent with `AttestationIndex::remove`).
    pub fn remove(&mut self, tx_id: &TransactionId) {
        self.entries.remove(tx_id);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Local mempool/mining policy for DNS-finality attestation shards. Sourced from the chain's
/// `DnsParams`; `enabled` mirrors `dns_params.is_some()`. When `enabled` is `false` every
/// attestation code path is a no-op and behavior is byte-identical to upstream.
#[derive(Clone, Debug)]
pub struct AttestationMempoolPolicy {
    pub enabled: bool,
    pub epoch_len_blue_score: u64,
    pub attestation_lag_blue_score: u64,
    pub stake_score_window_blue_score: u64,
    pub reward_uniqueness_window_blocks: u64,
    pub required_stake_depth_epochs: u64,
    pub hard_retention_grace_epochs: u64,
    pub replacement_bump_pct: u64,
    pub max_attestation_mempool_txs: usize,
    pub max_attestation_txs_per_key: usize,
    /// kaspa-pq audit v24 (M-1): the local per-block cap on the number of
    /// `StakeAttestationShard` *transactions* (NOT the number of attestations). This is
    /// distinct from `DnsParams::max_attestations_per_block`, which caps reward outputs for
    /// individual attestations. `0` means unlimited at this selector layer; block mass and the
    /// active validator set still bound the template. Optional hard mandatory deployments must not
    /// use a low static default here, because too small a shard cap can make the quality floor
    /// unreachable in one block.
    pub max_attestation_shard_txs_per_block: u64,
    pub max_attestation_shard_mass_per_block: u64,
    /// kaspa-pq audit v26 (H-4): how many epochs (relative to the latest ready epoch) a
    /// template-transient-dropped shard is held in quarantine before it becomes re-selectable.
    /// Default `1`; clamped to `1..=3` in [`Self::from_dns_params`]. A short hold breaks the
    /// "drop -> re-select -> drop" loop without hard-evicting a recoverable bond.
    pub quarantine_epochs: u64,
}

impl AttestationMempoolPolicy {
    /// Disabled policy — the default. Every attestation code path becomes a no-op.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            epoch_len_blue_score: 0,
            attestation_lag_blue_score: 0,
            stake_score_window_blue_score: 0,
            reward_uniqueness_window_blocks: 0,
            required_stake_depth_epochs: 0,
            hard_retention_grace_epochs: 2,
            replacement_bump_pct: 0,
            max_attestation_mempool_txs: 0,
            max_attestation_txs_per_key: 0,
            max_attestation_shard_txs_per_block: 0,
            max_attestation_shard_mass_per_block: 0,
            quarantine_epochs: 1,
        }
    }

    /// Build an enabled policy from a chain's [`DnsParams`].
    ///
    /// - `required_stake_depth_epochs = ceil(required_stake_depth.0 / STAKE_SCORE_SCALE)`.
    /// - `hard_retention_grace_epochs` defaults to `2` (folded into `hard_retention_epochs()`).
    /// - Per-block shard budgets default to `0` (unlimited in the selector, still bounded by block
    ///   mass). Optional hard mandatory inclusion must not inherit the reward-side
    ///   `DnsParams::max_attestations_per_block` as a shard-tx cap: with many active validators a
    ///   local cap of 16 can make a hard-inclusion template fail the consensus quality floor.
    ///
    /// `replacement_bump_pct` defaults to `10` (a 10% feerate bump to replace a same-key shard);
    /// `max_attestation_txs_per_key` defaults to `1` — the index keeps exactly one shard per
    /// `(bond, validator, epoch)` key (audit v24 H-4 rejects same-key/different-anchor shards and
    /// replaces same-key duplicates), so this is a hard per-key cap, not merely advisory.
    /// `max_attestation_mempool_txs` is an advisory ceiling left at a generous default.
    pub fn from_dns_params(params: &kaspa_consensus_core::dns_finality::DnsParams) -> Self {
        use kaspa_consensus_core::dns_finality::STAKE_SCORE_SCALE;
        let required_stake_depth_epochs = (params.required_stake_depth.0.div_ceil(STAKE_SCORE_SCALE)) as u64;
        Self {
            enabled: true,
            epoch_len_blue_score: params.attestation_epoch_length_blue_score,
            attestation_lag_blue_score: params.attestation_lag_blue_score,
            stake_score_window_blue_score: params.stake_score_window_blue_score,
            reward_uniqueness_window_blocks: params.reward_uniqueness_window_blocks,
            required_stake_depth_epochs,
            hard_retention_grace_epochs: 2,
            replacement_bump_pct: 10,
            max_attestation_mempool_txs: 100_000,
            max_attestation_txs_per_key: 1,
            max_attestation_shard_txs_per_block: 0,
            max_attestation_shard_mass_per_block: 0,
            // kaspa-pq audit v26 (H-4): default to a 1-epoch quarantine, clamped to 1..=3.
            quarantine_epochs: 1,
        }
    }

    /// Hard-retention horizon in epochs: shards older than this (relative to the latest ready
    /// epoch) are force-expired regardless of priority.
    ///
    /// `ceil(stake_score_window_blue_score / epoch_len_blue_score) + hard_retention_grace_epochs`.
    pub fn hard_retention_epochs(&self) -> u64 {
        let epoch_len = self.epoch_len_blue_score.max(1);
        let window_epochs = self.stake_score_window_blue_score.div_ceil(epoch_len);
        window_epochs.saturating_add(self.hard_retention_grace_epochs)
    }
}

impl Default for AttestationMempoolPolicy {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        config::params::TESTNET_DNS_PARAMS,
        constants::TX_VERSION,
        dns_finality::{StakeAttestation, StakeAttestationShardPayload},
        mass::NonContextualMasses,
        subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD},
        tx::{MutableTransaction, Transaction, TransactionOutpoint},
    };

    fn dummy_hash(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    fn shard_payload(epoch: u64, num_attestations: usize) -> StakeAttestationShardPayload {
        let attestations = (0..num_attestations)
            .map(|i| StakeAttestation {
                version: 1,
                validator_id: dummy_hash(i as u8 + 1),
                bond_outpoint: TransactionOutpoint::new(dummy_hash(0xaa), i as u32),
                epoch,
                target_hash: dummy_hash(0xbb),
                target_daa_score: 1234,
                validator_set_commitment: dummy_hash(0xcc),
                signature: vec![],
            })
            .collect();
        StakeAttestationShardPayload {
            version: 1,
            epoch,
            target_hash: dummy_hash(0xbb),
            target_daa_score: 1234,
            validator_set_commitment: dummy_hash(0xcc),
            attestations,
        }
    }

    /// Build a populated mempool MutableTransaction carrying a borsh-encoded payload on the given
    /// subnetwork (no inputs/outputs — fine for unit-testing the extractor / index).
    fn mtx_with_payload(subnetwork: kaspa_consensus_core::subnets::SubnetworkId, payload: Vec<u8>) -> MutableTransaction {
        let tx = Transaction::new(TX_VERSION, vec![], vec![], 0, subnetwork, 0, payload);
        let mut mtx = MutableTransaction::from_tx(tx);
        mtx.calculated_fee = Some(10_000);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(2000, 2000));
        mtx
    }

    #[test]
    fn non_shard_tx_returns_none() {
        let mtx = mtx_with_payload(SUBNETWORK_ID_NATIVE, vec![]);
        assert!(extract_attestation_meta(&mtx, 0, Priority::High).is_none());
    }

    #[test]
    fn valid_single_attestation_shard_yields_one_key() {
        let payload = borsh::to_vec(&shard_payload(7, 1)).unwrap();
        let mtx = mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, payload);
        let meta = extract_attestation_meta(&mtx, 42, Priority::High).expect("is a shard tx").expect("decodes");
        assert_eq!(meta.shard_epoch, 7);
        assert_eq!(meta.keys.len(), 1);
        assert_eq!(meta.keys[0].epoch, 7);
        assert_eq!(meta.added_at_daa_score, 42);
        assert_eq!(meta.fee, 10_000);
        assert_eq!(meta.mass, 2000);
    }

    #[test]
    fn multi_attestation_shard_yields_multiple_keys() {
        let payload = borsh::to_vec(&shard_payload(3, 4)).unwrap();
        let mtx = mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, payload);
        let meta = extract_attestation_meta(&mtx, 0, Priority::Low).expect("is a shard tx").expect("decodes");
        assert_eq!(meta.keys.len(), 4);
    }

    #[test]
    fn dns_reward_cap_does_not_become_local_shard_cap() {
        let policy = AttestationMempoolPolicy::from_dns_params(&TESTNET_DNS_PARAMS);
        assert!(TESTNET_DNS_PARAMS.max_attestations_per_block > 0, "test fixture must carry a reward-side cap");
        assert_eq!(policy.max_attestation_shard_txs_per_block, 0, "attestation selection needs no static shard-tx cap");
        assert_eq!(policy.max_attestation_shard_mass_per_block, 0, "attestation selection needs no static shard-mass cap");
    }

    #[test]
    fn invalid_payload_returns_err() {
        // Subnetwork matches but the payload is not a valid borsh StakeAttestationShardPayload.
        let mtx = mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, vec![0xff, 0x00, 0x13]);
        let result = extract_attestation_meta(&mtx, 0, Priority::High);
        assert!(matches!(result, Some(Err(_))));
    }

    #[test]
    fn index_insert_remove_keeps_maps_consistent() {
        let mut index = AttestationIndex::default();
        let payload = borsh::to_vec(&shard_payload(5, 2)).unwrap();
        let mtx = mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, payload);
        let meta = extract_attestation_meta(&mtx, 0, Priority::High).unwrap().unwrap();
        let tx_id = meta.tx_id;
        let keys = meta.keys.clone();

        index.insert(meta);
        assert_eq!(index.len(), 1);
        assert!(index.contains(&tx_id));
        for key in keys.iter() {
            assert_eq!(index.owner_of_key(key), Some(tx_id));
        }
        assert!(index.by_epoch.get(&5).unwrap().contains(&tx_id));

        index.remove(&tx_id);
        assert!(index.is_empty());
        assert!(index.by_key.is_empty());
        assert!(index.by_epoch.is_empty());
    }

    /// Build a shard payload whose single attestation has an explicit anchor, so two shards can
    /// share a `(bond, validator, epoch)` key while differing in their anchor tuple.
    fn shard_payload_with_anchor(
        epoch: u64,
        validator: u8,
        target_hash: u8,
        target_daa_score: u64,
        vsc: u8,
    ) -> StakeAttestationShardPayload {
        let att = StakeAttestation {
            version: 1,
            validator_id: dummy_hash(validator),
            bond_outpoint: TransactionOutpoint::new(dummy_hash(0xaa), 0),
            epoch,
            target_hash: dummy_hash(target_hash),
            target_daa_score,
            validator_set_commitment: dummy_hash(vsc),
            signature: vec![],
        };
        StakeAttestationShardPayload {
            version: 1,
            epoch,
            target_hash: dummy_hash(target_hash),
            target_daa_score,
            validator_set_commitment: dummy_hash(vsc),
            attestations: vec![att],
        }
    }

    /// kaspa-pq audit v24 (H-4): two shards sharing a `(bond, validator, epoch)` key but with
    /// DIFFERENT anchor tuples are detectable as conflicting (the dedup path rejects the new one).
    /// This pins the `anchor_tuple()` discriminator the rejection relies on.
    #[test]
    fn same_key_different_anchor_has_distinct_anchor_tuple() {
        let p1 = borsh::to_vec(&shard_payload_with_anchor(7, 1, 0xb1, 100, 0xc1)).unwrap();
        let p2 = borsh::to_vec(&shard_payload_with_anchor(7, 1, 0xb2, 200, 0xc2)).unwrap();
        let m1 = extract_attestation_meta(&mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, p1), 0, Priority::High)
            .unwrap()
            .unwrap();
        let m2 = extract_attestation_meta(&mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, p2), 0, Priority::High)
            .unwrap()
            .unwrap();
        // Same key (bond, validator, epoch) ...
        assert_eq!(m1.keys[0], m2.keys[0], "the two shards must share the (bond, validator, epoch) key");
        // ... but different anchor tuples ⇒ the H-4 conflict path fires.
        assert_ne!(m1.anchor_tuple(), m2.anchor_tuple(), "different anchors must yield distinct anchor tuples");
    }

    /// kaspa-pq audit v24 (H-4): the index keeps exactly one shard per key (per-key cap = 1). After
    /// the dedup contract resolves a same-key collision by removing the superseded shard, the index
    /// never holds two txs for one key.
    #[test]
    fn index_keeps_single_shard_per_key() {
        let mut index = AttestationIndex::default();
        let p1 = borsh::to_vec(&shard_payload_with_anchor(7, 1, 0xbb, 100, 0xcc)).unwrap();
        let m1 = extract_attestation_meta(&mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, p1), 0, Priority::High)
            .unwrap()
            .unwrap();
        let key = m1.keys[0];
        let old_id = m1.tx_id;
        index.insert(m1);
        assert_eq!(index.owner_of_key(&key), Some(old_id));

        // A same-key, same-anchor replacement with a DIFFERENT tx body (an extra output, which is
        // hashed into the id but does not change the attestation key/anchor) ⇒ distinct tx id.
        let payload2 = borsh::to_vec(&shard_payload_with_anchor(7, 1, 0xbb, 100, 0xcc)).unwrap();
        let tx2 = Transaction::new(
            TX_VERSION,
            vec![],
            vec![kaspa_consensus_core::tx::TransactionOutput::new(
                123,
                kaspa_consensus_core::tx::ScriptPublicKey::from_vec(0, vec![0x51]),
            )],
            0,
            SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
            0,
            payload2,
        );
        let mut mtx2 = MutableTransaction::from_tx(tx2);
        mtx2.calculated_fee = Some(10_000);
        mtx2.calculated_non_contextual_masses = Some(NonContextualMasses::new(2000, 2000));
        let m2 = extract_attestation_meta(&mtx2, 0, Priority::High).unwrap().unwrap();
        let new_id = m2.tx_id;
        assert_ne!(old_id, new_id);
        // The dedup contract: remove the old, insert the new (one shard per key throughout).
        index.remove(&old_id);
        index.insert(m2);
        assert_eq!(index.owner_of_key(&key), Some(new_id), "exactly one shard owns the key after replacement");
        assert_eq!(index.len(), 1, "the per-key cap (1) holds — no stale tx lingers");
        assert!(!index.contains(&old_id), "the superseded shard must not linger in by_txid (H-4 accumulation guard)");
    }

    #[test]
    fn ttl_hard_expires_old_even_high_priority_keeps_recent() {
        // hard_retention_epochs = ceil(window/epoch_len) + grace. Pick simple values.
        let policy = AttestationMempoolPolicy {
            enabled: true,
            epoch_len_blue_score: 100,
            stake_score_window_blue_score: 300, // ceil(300/100) = 3
            hard_retention_grace_epochs: 2,     // => hard_retention_epochs = 5
            ..AttestationMempoolPolicy::disabled()
        };
        assert_eq!(policy.hard_retention_epochs(), 5);

        let mut index = AttestationIndex::default();
        // A stale high-priority shard at epoch 1 and a recent one at epoch 20.
        for epoch in [1u64, 20u64] {
            let payload = borsh::to_vec(&shard_payload(epoch, 1)).unwrap();
            let mut mtx = mtx_with_payload(SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, payload);
            // Distinguish the two txs' ids by setting a distinct lock_time-equivalent via gas (ids
            // already differ because the payload epoch differs).
            mtx.calculated_fee = Some(10_000);
            let meta = extract_attestation_meta(&mtx, 0, Priority::High).unwrap().unwrap();
            index.insert(meta);
        }

        let latest_ready_epoch = 22;
        let expired = index.collect_hard_expired(latest_ready_epoch, policy.hard_retention_epochs());
        // epoch 1: 22 - 1 = 21 > 5 -> expired. epoch 20: 22 - 20 = 2 <= 5 -> kept.
        assert_eq!(expired.len(), 1, "only the stale epoch-1 shard should hard-expire");
        let stale_meta = index.by_epoch.get(&1).unwrap().iter().next().copied().unwrap();
        assert!(expired.contains(&stale_meta));
        assert!(index.by_epoch.contains_key(&20), "the recent shard must remain");
    }

    fn tx_id(b: u8) -> TransactionId {
        TransactionId::from_bytes([b; 64])
    }

    /// kaspa-pq audit v26 (H-4): a quarantined shard is excluded while `current_epoch <
    /// until_epoch` and becomes re-selectable once the hold lapses. The `&mut` `is_quarantined`
    /// also LAZILY EVICTS the lapsed entry (so a later `len()` drops it without a sweep).
    #[test]
    fn quarantine_excludes_until_then_releases_with_lazy_evict() {
        let mut q = AttestationQuarantine::default();
        let id = tx_id(1);
        q.insert(id, QuarantineReason::TemplateTransient, 5); // held while epoch < 5

        assert!(q.is_quarantined(&id, 3), "still held at epoch 3");
        assert!(q.is_quarantined(&id, 4), "still held at epoch 4 (until is exclusive)");
        assert_eq!(q.len(), 1, "entry present while held");

        // At epoch 5 the hold has lapsed -> not quarantined AND lazily evicted.
        assert!(!q.is_quarantined(&id, 5), "released at epoch == until_epoch");
        assert_eq!(q.len(), 0, "lapsed entry is lazily evicted by is_quarantined");
    }

    /// kaspa-pq audit v26 (H-4): the immutable `is_active` peek mirrors the hold window without
    /// mutating, and `retain_active` reaps lapsed entries.
    #[test]
    fn quarantine_is_active_peek_and_retain_active() {
        let mut q = AttestationQuarantine::default();
        let a = tx_id(1);
        let b = tx_id(2);
        q.insert(a, QuarantineReason::TemplateTransient, 5);
        q.insert(b, QuarantineReason::TemplateTransient, 10);

        // Immutable peek does not evict.
        assert!(q.is_active(&a, 4));
        assert!(!q.is_active(&a, 5));
        assert_eq!(q.len(), 2, "is_active must not mutate the map");

        // Reaping at epoch 5 drops `a` (until 5) but keeps `b` (until 10).
        q.retain_active(5);
        assert!(!q.is_active(&a, 5));
        assert!(q.is_active(&b, 5));
        assert_eq!(q.len(), 1);

        q.remove(&b);
        assert_eq!(q.len(), 0);
    }
}
