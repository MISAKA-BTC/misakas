//! **ADR-0128 — DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate.**
//!
//! The pure rules behind [`DnsBftGateV1`]: no store, no fence, no signature check. The virtual
//! processor walks the selected chain once from the sink, verifies every attestation and precommit
//! it reads, and hands the verified inputs here. Whether an epoch's canonical anchor is DNS-final
//! is a function of those inputs and nothing else.
//!
//! * **Voting power is bonded stake** (Decision 1). A vote weighs the `amount` of the bond it is
//!   cast under, once per `(validator_id, bond_outpoint)`, for a bond `Active` at the epoch's
//!   anchor. No compute, no decay, no frozen snapshot.
//! * **Two rounds, one denominator** (Decision 2). Round one is the attestations naming the epoch's
//!   anchor; round two is the lock-consistent precommits naming it under the epoch's snapshot
//!   commitment, counted only once round one has reached quorum. Both are fractions of one `W(E)`,
//!   the stake of the epoch's counted set, and each must be strictly above two thirds of it
//!   ([`dns_bft_quorum_v1`]). The anchor is DNS-final when both are ([`DnsBftEpochVerdictV1`]).
//! * **The counted set leaks the silent, and the evidence covers the silence** (Decision 3,
//!   [`dns_bft_counted_set_v1`]). The evidence is read from the chain prefix that ends at the
//!   epoch's anchor, inside a window at least `t_leak_daa` long, so an epoch's counted set is one
//!   answer on every sink that shares that prefix.
//! * **The denominator is signed** (Decision 4, [`dns_bft_snapshot_commitment_v1`]).
//! * **The lock is the chain's** ([`lock_consistent_precommits`], [`held_precommit_lock`]): the
//!   first precommit a bond has in view declares no lock the chain could show, each later one
//!   declares its predecessor, and a misdeclaration stops the bond's count there.
//! * **The duty is read from the chain** (Decision 6, [`dns_bft_precommit_duty_v1`]).

use std::collections::{BTreeSet, HashMap, HashSet};

use blake2b_simd::Params as Blake2bParams;
use kaspa_hashes::Hash64;

use crate::{
    TransactionId,
    config::params::DnsBftGateV1,
    dns_finality::{
        DnsParams, DnsTxKind, PrecommitDuty, PrecommitLock, StakeBondRecord, StakePrecommitPayload, anchor_cutoff_blue_score,
        canonical_lagged_epoch_anchor, dns_tx_kind, is_bond_active_at, ready_epoch_from_tip_blue_score,
    },
    tx::{Transaction, TransactionOutpoint},
};

/// BLAKE2b-512 key of the snapshot commitment a precommit signs (Decision 4).
pub const DNS_BFT_SNAPSHOT_DOMAIN_V1: &[u8] = b"misaka/dns-bft-snapshot/v1";

/// BLAKE2b-512 key of `root(S(E))`, the counted set inside the snapshot commitment. Its own key, so
/// a root can never be read as a commitment or the reverse.
pub const DNS_BFT_SNAPSHOT_ROOT_DOMAIN_V1: &[u8] = b"misaka/dns-bft-snapshot/v1/root";

/// The smallest `min_retained_validators` a network may configure: the smallest set in which one
/// fault is tolerated (ADR-0128 Decision 8).
pub const DNS_BFT_MIN_RETAINED_VALIDATORS_FLOOR_V1: u32 = 4;

/// `L`, the leak's evidence window, in blue score: `t_leak_daa + reentry_final_depth_daa +
/// attestation_epoch_length_blue_score + attestation_lag_blue_score` (Decision 3). `None` when the
/// sum does not fit — [`crate::config::params::Params::validate_palw_v2`] refuses such a network.
pub fn dns_bft_evidence_window_blue_score_v1(gate: &DnsBftGateV1, dns: &DnsParams) -> Option<u64> {
    gate.t_leak_daa
        .checked_add(gate.reentry_final_depth_daa)?
        .checked_add(dns.attestation_epoch_length_blue_score)?
        .checked_add(dns.attestation_lag_blue_score)
}

/// **What a sink's evaluation reads**: its StakeScore window, whose epochs it evaluates, plus the
/// evidence window below them. The pruning depth must be at least this, or the pruning point passes
/// the evidence a synced node's leak reads (Decision 3, SA-3). testnet-11's planned numbers give
/// `1,500 + 5,440 = 6,940`.
pub fn dns_bft_walk_blue_score_v1(gate: &DnsBftGateV1, dns: &DnsParams) -> Option<u64> {
    dns.stake_score_window_blue_score.checked_add(dns_bft_evidence_window_blue_score_v1(gate, dns)?)
}

/// **The strict BFT quorum: `3·signed > 2·total`, with `total > 0`.**
///
/// Exactly two thirds is not a quorum. `signed` is clamped to `total`, so an over-count cannot
/// manufacture one, and the comparison is written as `signed − unsigned > unsigned` so no product
/// can overflow.
pub fn dns_bft_quorum_v1(signed: u128, total: u128) -> bool {
    if total == 0 {
        return false;
    }
    let signed = signed.min(total);
    let unsigned = total - signed;
    signed > unsigned && signed - unsigned > unsigned
}

/// The numbers the rules read, gathered once per evaluation from the fence and the overlay params.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DnsBftRulesV1 {
    pub t_leak_daa: u64,
    pub reentry_final_depth_daa: u64,
    pub min_retained_validators: u32,
    /// `L` — see [`dns_bft_evidence_window_blue_score_v1`].
    pub evidence_window_blue_score: u64,
    pub epoch_length_blue_score: u64,
    pub anchor_backoff_blue_score: u64,
}

impl DnsBftRulesV1 {
    /// `None` when the evidence window does not fit in a `u64`.
    pub fn new(gate: &DnsBftGateV1, dns: &DnsParams) -> Option<Self> {
        Some(Self {
            t_leak_daa: gate.t_leak_daa,
            reentry_final_depth_daa: gate.reentry_final_depth_daa,
            min_retained_validators: gate.min_retained_validators,
            evidence_window_blue_score: dns_bft_evidence_window_blue_score_v1(gate, dns)?,
            epoch_length_blue_score: dns.attestation_epoch_length_blue_score.max(1),
            anchor_backoff_blue_score: dns.attestation_anchor_backoff_blue_score,
        })
    }

    /// The blue score `epoch`'s evidence window starts at: `L` below its anchor, or genesis.
    pub fn evidence_floor_blue_score(&self, epoch: &DnsBftEpochV1) -> u64 {
        epoch.anchor_blue_score.saturating_sub(self.evidence_window_blue_score)
    }

    fn cutoff_blue_score(&self, epoch: u64) -> u64 {
        anchor_cutoff_blue_score(epoch, self.epoch_length_blue_score, self.anchor_backoff_blue_score)
    }

    /// **Can every walk that covers `epoch`'s evidence window derive `attested_epoch`'s anchor?**
    ///
    /// The canonical anchor of `E'` and its duplicate flag are decided by the chain blocks down to
    /// `anchor_cutoff(E' − 1)`. When that cutoff is inside `epoch`'s window, every node that covers
    /// the window derives the same anchor; when it is below, a node whose walk happens to reach
    /// further (an earlier sink) could credit evidence a later sink cannot see, and the counted set
    /// would stop being a function of the epoch. So such evidence is not evidence here — and it
    /// cannot matter: an anchor that old is more than `t_leak_daa` below the epoch's own.
    pub fn anchor_decidable_within(&self, attested_epoch: u64, epoch: &DnsBftEpochV1) -> bool {
        self.cutoff_blue_score(attested_epoch.saturating_sub(1)) >= self.evidence_floor_blue_score(epoch)
    }
}

/// One selected-chain block the walk passed.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DnsBftChainBlockV1 {
    pub hash: Hash64,
    pub blue_score: u64,
    pub daa_score: u64,
}

/// One evaluated epoch: its canonical lagged anchor on the sink's chain.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DnsBftEpochV1 {
    pub epoch: u64,
    pub anchor_hash: Hash64,
    pub anchor_blue_score: u64,
    pub anchor_daa_score: u64,
}

/// **An attestation the walk read and verified**, exactly as the credit walk reads one: accepted on
/// the sink's chain, naming the canonical non-duplicate anchor of its epoch on that chain, the bond
/// `Active` at that anchor and bound to `validator_id`, the validator-set commitment zero, the
/// ML-DSA-87 signature valid. The rules below decide which epochs it speaks for.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DnsBftAttestationV1 {
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    pub epoch: u64,
    pub anchor_hash: Hash64,
    pub anchor_daa_score: u64,
    /// The chain block that accepted it.
    pub accepted_blue_score: u64,
    pub accepted_daa_score: u64,
}

/// **A precommit the walk read and verified**: accepted on the sink's chain, its declared lock
/// self-consistent, bound to its bond's validator, its ML-DSA-87 signature valid under the precommit
/// context. Everything else — the epoch it names, the anchor, the snapshot commitment, whether its
/// lock is the one the chain shows — is judged by the rules below.
///
/// Every signed precommit is part of its bond's lock chain, counted or not: the chain shows that
/// the validator locked on it, whatever it locked on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrecommitRecord {
    pub validator_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    pub epoch: u64,
    pub target_hash: Hash64,
    pub target_daa_score: u64,
    pub declared_lock: PrecommitLock,
    pub snapshot_commitment: Hash64,
    /// The chain block that accepted it. The DAA score orders a bond's lock chain; the blue score
    /// places it against a horizon.
    pub accepted_blue_score: u64,
    pub accepted_daa_score: u64,
}

/// Every decodable [`StakePrecommitPayload`] among `txs`.
pub fn precommits_from_accepted_txs(txs: &[Transaction]) -> Vec<StakePrecommitPayload> {
    txs.iter()
        .filter(|tx| dns_tx_kind(&tx.subnetwork_id) == Some(DnsTxKind::StakePrecommit))
        .filter_map(|tx| borsh::from_slice::<StakePrecommitPayload>(&tx.payload).ok())
        .collect()
}

/// **The epochs a sink evaluates: its StakeScore window's**, derived exactly as
/// `canonical_anchors_in_window` derives the credit walk's — the latest epoch buried by the lag,
/// downward while the previous epoch's cutoff is still inside the window, duplicates skipped.
///
/// `chain` is the selected chain from the sink downward (tip first); blocks past the window are
/// ignored. Ascending by epoch.
pub fn dns_bft_window_epochs_v1(
    chain: &[DnsBftChainBlockV1],
    sink_blue_score: u64,
    window_blue_score: u64,
    epoch_length_blue_score: u64,
    lag_blue_score: u64,
    backoff_blue_score: u64,
) -> Vec<DnsBftEpochV1> {
    let epoch_len = epoch_length_blue_score.max(1);
    let Some(latest_ready) = ready_epoch_from_tip_blue_score(sink_blue_score, epoch_len, lag_blue_score) else {
        return Vec::new();
    };
    let ancestors: Vec<(Hash64, u64, u64)> = chain
        .iter()
        .take_while(|b| sink_blue_score.saturating_sub(b.blue_score) <= window_blue_score)
        .map(|b| (b.hash, b.blue_score, b.daa_score))
        .collect();
    let oldest_blue = ancestors.last().map_or(sink_blue_score, |a| a.1);
    let mut epochs = Vec::new();
    let mut epoch = latest_ready;
    loop {
        if anchor_cutoff_blue_score(epoch.saturating_sub(1), epoch_len, backoff_blue_score) < oldest_blue {
            break;
        }
        if let Some(anchor) = canonical_lagged_epoch_anchor(epoch, epoch_len, backoff_blue_score, &ancestors)
            && !anchor.duplicate_of_previous_anchor
        {
            epochs.push(DnsBftEpochV1 {
                epoch,
                anchor_hash: anchor.anchor_hash,
                anchor_blue_score: anchor.anchor_blue_score,
                anchor_daa_score: anchor.anchor_daa_score,
            });
        }
        if epoch == 0 {
            break;
        }
        epoch -= 1;
    }
    epochs.reverse();
    epochs
}

/// **The lower edge of `epoch`'s evidence window, as a DAA score**: the oldest chain block at or
/// below the anchor whose blue score is inside the window. On a chain younger than the window that
/// is genesis, which makes an absent bond's fallback its own activation.
pub fn dns_bft_window_lower_edge_daa_v1(chain: &[DnsBftChainBlockV1], epoch: &DnsBftEpochV1, rules: &DnsBftRulesV1) -> u64 {
    let floor = rules.evidence_floor_blue_score(epoch);
    chain
        .iter()
        .filter(|b| b.daa_score <= epoch.anchor_daa_score && b.blue_score >= floor)
        .map(|b| b.daa_score)
        .min()
        .unwrap_or(epoch.anchor_daa_score)
}

/// **Is `attestation` final evidence for `epoch` (Decision 3)?**
///
/// Final means all of: accepted by a chain block at or below the epoch's anchor (so the evidence is
/// in the prefix that ends there, whichever sink reads it); that block inside the evidence window;
/// the attested anchor a strict ancestor of the epoch's and at least `reentry_final_depth_daa` below
/// it (ADR-0066 SA-2 — re-entry waits for an attestation that is itself buried); and the attested
/// anchor decidable inside the window ([`DnsBftRulesV1::anchor_decidable_within`]).
///
/// One spelling for both readers: the fold below, and the walk that decides which candidates are
/// worth a signature check.
pub fn dns_bft_is_final_evidence_v1(attestation: &DnsBftAttestationV1, epoch: &DnsBftEpochV1, rules: &DnsBftRulesV1) -> bool {
    let a = attestation;
    let accepted_in_prefix =
        a.accepted_daa_score <= epoch.anchor_daa_score && a.accepted_blue_score >= rules.evidence_floor_blue_score(epoch);
    let buried = a.anchor_daa_score < epoch.anchor_daa_score
        && a.anchor_daa_score.saturating_add(rules.reentry_final_depth_daa) <= epoch.anchor_daa_score;
    accepted_in_prefix && buried && rules.anchor_decidable_within(a.epoch, epoch)
}

/// **`last_E(bond)` where the bond has evidence**: the anchor DAA of the youngest epoch it attested
/// in an attestation that is final evidence for `epoch` ([`dns_bft_is_final_evidence_v1`]).
///
/// A bond absent from the map has no such evidence and is measured from
/// `max(activation, window lower edge)` instead.
pub fn dns_bft_last_final_attestation_daa_v1(
    epoch: &DnsBftEpochV1,
    attestations: &[DnsBftAttestationV1],
    rules: &DnsBftRulesV1,
) -> HashMap<TransactionOutpoint, u64> {
    let mut last: HashMap<TransactionOutpoint, u64> = HashMap::new();
    for a in attestations.iter().filter(|a| dns_bft_is_final_evidence_v1(a, epoch, rules)) {
        let entry = last.entry(a.bond_outpoint).or_insert(a.anchor_daa_score);
        *entry = (*entry).max(a.anchor_daa_score);
    }
    last
}

/// One bond in a counted set.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct DnsBftCountedBondV1 {
    pub bond_outpoint: TransactionOutpoint,
    pub validator_id: Hash64,
    pub amount: u64,
}

/// **`S(E)`: the bonds an epoch's votes are counted against.**
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DnsBftCountedSetV1 {
    /// Sorted by outpoint `(transaction_id, index)` — the order the snapshot root commits.
    pub bonds: Vec<DnsBftCountedBondV1>,
    /// `W(E)`, the stake of `bonds`.
    pub total_stake: u128,
    /// The bonds `Active` at the anchor that the leak excluded, in outpoint order. Empty when the
    /// floor held.
    pub leaked: Vec<TransactionOutpoint>,
    /// Leaking would have left fewer than `min_retained_validators` distinct validators, so nothing
    /// was leaked: finality waits rather than a few validators finalizing for the network.
    pub floor_held: bool,
}

fn outpoint_key(outpoint: &TransactionOutpoint) -> (TransactionId, u32) {
    (outpoint.transaction_id, outpoint.index)
}

impl DnsBftCountedSetV1 {
    /// The counted bond at `outpoint`, if it is in the set.
    pub fn get(&self, outpoint: &TransactionOutpoint) -> Option<&DnsBftCountedBondV1> {
        let key = outpoint_key(outpoint);
        self.bonds.binary_search_by(|b| outpoint_key(&b.bond_outpoint).cmp(&key)).ok().map(|i| &self.bonds[i])
    }

    /// The stake of a vote cast by `validator_id` under `outpoint`: the bond's amount when the bond
    /// is counted and bound to that validator, `None` otherwise.
    pub fn vote_weight(&self, validator_id: &Hash64, outpoint: &TransactionOutpoint) -> Option<u128> {
        self.get(outpoint).filter(|b| b.validator_id == *validator_id).map(|b| b.amount as u128)
    }

    /// Distinct validators in the set.
    pub fn validator_count(&self) -> usize {
        self.bonds.iter().map(|b| b.validator_id).collect::<BTreeSet<_>>().len()
    }
}

/// **`S(E)` = the bonds `Active` at the anchor, less those leaked (Decision 3).**
///
/// A bond is leaked when `anchor_daa − last ≥ t_leak_daa`, `last` being its youngest final
/// attestation's anchor DAA ([`dns_bft_last_final_attestation_daa_v1`]) or, without one, the later
/// of its activation and the window's lower edge — so a validator that attests is measured from its
/// attestation, an old bond absent from a window at least `t_leak_daa` long is silent for at least
/// that long, and a bond younger than the leak period is never leaked for a silence it could not
/// have broken (SA-4). If leaking would leave fewer than `min_retained_validators` distinct
/// validators, nothing is leaked.
pub fn dns_bft_counted_set_v1(
    bonds: &[StakeBondRecord],
    anchor_daa_score: u64,
    window_lower_edge_daa: u64,
    last_final_attestation_daa: &HashMap<TransactionOutpoint, u64>,
    rules: &DnsBftRulesV1,
) -> DnsBftCountedSetV1 {
    let mut active: Vec<&StakeBondRecord> = bonds.iter().filter(|b| is_bond_active_at(b, anchor_daa_score)).collect();
    active.sort_by_key(|b| outpoint_key(&b.bond_outpoint));
    let leaked_at = |b: &StakeBondRecord| {
        let last = last_final_attestation_daa
            .get(&b.bond_outpoint)
            .copied()
            .unwrap_or_else(|| b.activation_daa_score.max(window_lower_edge_daa));
        anchor_daa_score.saturating_sub(last) >= rules.t_leak_daa
    };
    let leaked: Vec<TransactionOutpoint> = active.iter().filter(|b| leaked_at(b)).map(|b| b.bond_outpoint).collect();
    let retained_validators = active.iter().filter(|b| !leaked_at(b)).map(|b| b.validator_pubkey_hash).collect::<BTreeSet<_>>().len();
    let floor_held = !leaked.is_empty() && retained_validators < rules.min_retained_validators as usize;
    let (counted, leaked): (Vec<&StakeBondRecord>, Vec<TransactionOutpoint>) =
        if floor_held { (active, Vec::new()) } else { (active.into_iter().filter(|b| !leaked_at(b)).collect(), leaked) };
    let bonds: Vec<DnsBftCountedBondV1> = counted
        .into_iter()
        .map(|b| DnsBftCountedBondV1 { bond_outpoint: b.bond_outpoint, validator_id: b.validator_pubkey_hash, amount: b.amount })
        .collect();
    let total_stake = bonds.iter().fold(0u128, |acc, b| acc.saturating_add(b.amount as u128));
    DnsBftCountedSetV1 { bonds, total_stake, leaked, floor_held }
}

/// `root(S(E))`: BLAKE2b-512 keyed with [`DNS_BFT_SNAPSHOT_ROOT_DOMAIN_V1`] over
///
/// ```text
/// count (u64 LE) ‖ for each bond, in outpoint order:
///     transaction_id (64 B) ‖ index (u32 LE) ‖ validator_id (64 B) ‖ amount (u64 LE)
/// ```
pub fn dns_bft_snapshot_root_v1(counted: &DnsBftCountedSetV1) -> Hash64 {
    let mut sorted: Vec<&DnsBftCountedBondV1> = counted.bonds.iter().collect();
    sorted.sort_by_key(|b| outpoint_key(&b.bond_outpoint));
    let mut hasher = Blake2bParams::new().hash_length(64).key(DNS_BFT_SNAPSHOT_ROOT_DOMAIN_V1).to_state();
    hasher.update(&(sorted.len() as u64).to_le_bytes());
    for b in sorted {
        hasher.update(b.bond_outpoint.transaction_id.as_byte_slice());
        hasher.update(&b.bond_outpoint.index.to_le_bytes());
        hasher.update(b.validator_id.as_byte_slice());
        hasher.update(&b.amount.to_le_bytes());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The snapshot commitment a precommit for `epoch` must carry (Decision 4)**: BLAKE2b-512 keyed
/// with [`DNS_BFT_SNAPSHOT_DOMAIN_V1`] over
///
/// ```text
/// epoch (u64 LE) ‖ anchor_hash (64 B) ‖ anchor_daa (u64 LE) ‖ W(E) (u128 LE) ‖ root(S(E)) (64 B)
/// ```
///
/// A precommit binds the set it was counted against, so a lock cannot be restated under a different
/// denominator, and two branches that count different sets for one epoch cannot share a precommit.
pub fn dns_bft_snapshot_commitment_v1(epoch: &DnsBftEpochV1, counted: &DnsBftCountedSetV1) -> Hash64 {
    let mut hasher = Blake2bParams::new().hash_length(64).key(DNS_BFT_SNAPSHOT_DOMAIN_V1).to_state();
    hasher.update(&epoch.epoch.to_le_bytes());
    hasher.update(epoch.anchor_hash.as_byte_slice());
    hasher.update(&epoch.anchor_daa_score.to_le_bytes());
    hasher.update(&counted.total_stake.to_le_bytes());
    hasher.update(dns_bft_snapshot_root_v1(counted).as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(hasher.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **`P(E)`: the counted stake that attested the epoch's anchor.** One vote per
/// `(validator_id, bond_outpoint)`, however many shards carried it.
pub fn dns_bft_attested_stake_v1(epoch: &DnsBftEpochV1, counted: &DnsBftCountedSetV1, attestations: &[DnsBftAttestationV1]) -> u128 {
    let mut seen: HashSet<(Hash64, TransactionOutpoint)> = HashSet::new();
    attestations
        .iter()
        .filter(|a| a.epoch == epoch.epoch && a.anchor_hash == epoch.anchor_hash && a.anchor_daa_score == epoch.anchor_daa_score)
        .filter_map(|a| {
            counted.vote_weight(&a.validator_id, &a.bond_outpoint).filter(|_| seen.insert((a.validator_id, a.bond_outpoint)))
        })
        .fold(0u128, |acc, w| acc.saturating_add(w))
}

/// **Where a bond's lock chain comes into view.**
///
/// The chain a node can read is bounded — by the walk, and on a pruned node by the pruning point —
/// so the rule "the first precommit declares no lock" is applied to the first precommit IN VIEW,
/// and it has to admit the one thing the view cannot show: a lock whose precommit could only have
/// been accepted below the horizon. A precommit for epoch `E_l` is accepted after `E_l`'s anchor is
/// buried, above `anchor_cutoff(E_l)`; so a declared lock whose cutoff is below the horizon may be
/// real and unseen, and one whose cutoff is at or above it would be in view if it existed.
///
/// An evaluated epoch's horizon is the floor of its own evidence window
/// ([`Self::for_epoch`]), which makes whether a precommit for it is lock-consistent a function of
/// the chain prefix that accepted it and of the epoch — not of how far a particular sink's walk
/// reached — and ties the lock's memory to the leak's: a bond silent for longer than the window has
/// leaked, and comes back with a lock chain that starts again.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PrecommitLockHorizonV1 {
    /// Precommits accepted at a lower blue score are out of view.
    pub visible_from_blue_score: u64,
    pub epoch_length_blue_score: u64,
    pub anchor_backoff_blue_score: u64,
}

impl PrecommitLockHorizonV1 {
    /// The whole chain in view: the first precommit must declare no lock at all.
    pub fn from_genesis(rules: &DnsBftRulesV1) -> Self {
        Self {
            visible_from_blue_score: 0,
            epoch_length_blue_score: rules.epoch_length_blue_score,
            anchor_backoff_blue_score: rules.anchor_backoff_blue_score,
        }
    }

    /// The horizon `epoch`'s precommits are judged against: its evidence window's floor.
    pub fn for_epoch(epoch: &DnsBftEpochV1, rules: &DnsBftRulesV1) -> Self {
        Self { visible_from_blue_score: rules.evidence_floor_blue_score(epoch), ..Self::from_genesis(rules) }
    }

    fn sees(&self, record: &PrecommitRecord) -> bool {
        record.accepted_blue_score >= self.visible_from_blue_score
    }

    /// Could a precommit that took `lock` only have been accepted below the horizon?
    fn hides(&self, lock: &PrecommitLock) -> bool {
        *lock != PrecommitLock::default()
            && anchor_cutoff_blue_score(lock.epoch, self.epoch_length_blue_score, self.anchor_backoff_blue_score)
                < self.visible_from_blue_score
    }
}

/// A precommit's vote, as a key: `(epoch, target, target DAA, declared lock epoch, declared lock
/// anchor)`. Two records with one key are a rebroadcast, which `precommit_fault` does not convict and
/// the lock chain neither counts twice nor reads as a misdeclaration.
type PrecommitVoteKey = (u64, Hash64, u64, u64, Hash64);

fn vote_key(r: &PrecommitRecord) -> PrecommitVoteKey {
    (r.epoch, r.target_hash, r.target_daa_score, r.declared_lock.epoch, r.declared_lock.anchor)
}

/// Chain order: the accepting block's DAA score, ties broken by `(epoch, bond txid, index, target)`
/// so every node orders identically. The accepting block's blue score never decreases in it.
fn chain_order(r: &PrecommitRecord) -> (u64, u64, TransactionId, u32, Hash64) {
    (r.accepted_daa_score, r.epoch, r.bond_outpoint.transaction_id, r.bond_outpoint.index, r.target_hash)
}

/// **One bond's lock chain under `horizon`**, `chain` in chain order: the records in view, cut at the
/// first misdeclared lock. The first record in view must declare no lock or a lock the horizon
/// hides; each later one must declare its predecessor's `(epoch, target)`. `visit` sees every
/// lock-consistent record in order — a rebroadcast of a vote already in the chain included, which
/// moves nothing — and stops the walk by returning `false`. Returns the lock the walked records
/// leave the bond holding.
fn walk_lock_chain<'a>(
    chain: &[&'a PrecommitRecord],
    horizon: &PrecommitLockHorizonV1,
    mut visit: impl FnMut(&'a PrecommitRecord) -> bool,
) -> PrecommitLock {
    let mut held = PrecommitLock::default();
    let mut votes: HashSet<PrecommitVoteKey> = HashSet::new();
    for r in chain.iter().copied().filter(|r| horizon.sees(r)) {
        let key = vote_key(r);
        if !votes.contains(&key) {
            let consistent = if votes.is_empty() {
                r.declared_lock == PrecommitLock::default() || horizon.hides(&r.declared_lock)
            } else {
                r.declared_lock == held
            };
            if !consistent {
                break; // misdeclared: this one and everything after it are uncountable
            }
            held = PrecommitLock { epoch: r.epoch, anchor: r.target_hash };
            votes.insert(key);
        }
        if !visit(r) {
            break;
        }
    }
    held
}

/// **Every bond's precommits, grouped once and put in chain order** — what the lock rule walks for
/// each epoch without regrouping the whole set.
pub struct DnsBftPrecommitChainsV1<'a> {
    by_bond: HashMap<(Hash64, TransactionOutpoint), Vec<&'a PrecommitRecord>>,
}

impl<'a> DnsBftPrecommitChainsV1<'a> {
    pub fn new(records: &'a [PrecommitRecord]) -> Self {
        let mut by_bond: HashMap<(Hash64, TransactionOutpoint), Vec<&'a PrecommitRecord>> = HashMap::new();
        for r in records {
            by_bond.entry((r.validator_id, r.bond_outpoint)).or_default().push(r);
        }
        for chain in by_bond.values_mut() {
            chain.sort_by_key(|r| chain_order(r));
        }
        Self { by_bond }
    }

    fn chain(&self, validator_id: Hash64, bond_outpoint: TransactionOutpoint) -> &[&'a PrecommitRecord] {
        self.by_bond.get(&(validator_id, bond_outpoint)).map_or(&[], |chain| chain.as_slice())
    }

    /// Whether `(validator_id, bond_outpoint)`'s lock-consistent chain under `horizon` holds a
    /// precommit for which `wanted` is true. The walk stops at the first one.
    pub fn has_consistent(
        &self,
        validator_id: Hash64,
        bond_outpoint: TransactionOutpoint,
        horizon: &PrecommitLockHorizonV1,
        wanted: impl Fn(&PrecommitRecord) -> bool,
    ) -> bool {
        let chain = self.chain(validator_id, bond_outpoint);
        if !chain.iter().any(|r| wanted(r)) {
            return false;
        }
        let mut found = false;
        walk_lock_chain(chain, horizon, |r| {
            found = wanted(r);
            !found
        });
        found
    }

    /// The lock `(validator_id, bond_outpoint)` carries under `horizon`.
    pub fn held_lock(
        &self,
        validator_id: Hash64,
        bond_outpoint: TransactionOutpoint,
        horizon: &PrecommitLockHorizonV1,
    ) -> PrecommitLock {
        walk_lock_chain(self.chain(validator_id, bond_outpoint), horizon, |_| true)
    }

    /// Whether the chain shows `(validator_id, bond_outpoint)` signing any precommit for `epoch`.
    pub fn signed_epoch(&self, validator_id: Hash64, bond_outpoint: TransactionOutpoint, epoch: u64) -> bool {
        self.chain(validator_id, bond_outpoint).iter().any(|r| r.epoch == epoch)
    }
}

/// **The precommits whose declared lock is the one the chain shows**: every bond's lock chain under
/// `horizon`, bonds in `(validator, outpoint)` order, each in chain order.
///
/// That severity is the point: the declaration is what a validator can later be held to, so it
/// has to be a faithful running record. A validator that misdeclares stops counting in round two
/// until the misdeclaration leaves the horizon, and if it misdeclared because it carries a different
/// lock on another branch, the two signed payloads are the proof (`precommit_fault`).
pub fn lock_consistent_precommits<'a>(records: &'a [PrecommitRecord], horizon: &PrecommitLockHorizonV1) -> Vec<&'a PrecommitRecord> {
    let chains = DnsBftPrecommitChainsV1::new(records);
    // `TransactionOutpoint` is not `Ord`, so the order is imposed on the way out.
    let mut bonds: Vec<(Hash64, TransactionOutpoint)> = chains.by_bond.keys().copied().collect();
    bonds.sort_by_key(|(validator, outpoint)| (*validator, outpoint_key(outpoint)));
    let mut kept = Vec::new();
    for (validator, outpoint) in bonds {
        walk_lock_chain(chains.chain(validator, outpoint), horizon, |r| {
            kept.push(r);
            true
        });
    }
    kept
}

/// **The lock `(validator_id, bond_outpoint)` carries on this chain** — the target of the last
/// precommit in its lock-consistent chain under `horizon`, or no lock when it has none. What its
/// next precommit must declare.
pub fn held_precommit_lock(
    records: &[PrecommitRecord],
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    horizon: &PrecommitLockHorizonV1,
) -> PrecommitLock {
    DnsBftPrecommitChainsV1::new(records).held_lock(validator_id, bond_outpoint, horizon)
}

/// **`C(E)`: the counted stake that precommitted the epoch's anchor under its snapshot commitment,
/// with the lock the chain shows** — under the epoch's own horizon
/// ([`PrecommitLockHorizonV1::for_epoch`]). One vote per counted `(validator_id, bond_outpoint)`.
pub fn dns_bft_precommitted_stake_v1(
    epoch: &DnsBftEpochV1,
    counted: &DnsBftCountedSetV1,
    snapshot_commitment: Hash64,
    chains: &DnsBftPrecommitChainsV1<'_>,
    rules: &DnsBftRulesV1,
) -> u128 {
    let horizon = PrecommitLockHorizonV1::for_epoch(epoch, rules);
    counted
        .bonds
        .iter()
        .filter(|b| {
            chains.has_consistent(b.validator_id, b.bond_outpoint, &horizon, |p| {
                p.epoch == epoch.epoch
                    && p.target_hash == epoch.anchor_hash
                    && p.target_daa_score == epoch.anchor_daa_score
                    && p.snapshot_commitment == snapshot_commitment
            })
        })
        .fold(0u128, |acc, b| acc.saturating_add(b.amount as u128))
}

/// **One epoch, decided (Decision 2).**
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DnsBftEpochVerdictV1 {
    pub epoch: DnsBftEpochV1,
    pub counted: DnsBftCountedSetV1,
    pub snapshot_commitment: Hash64,
    /// `P(E)`.
    pub attested_stake: u128,
    /// `C(E)` — zero unless round one reached quorum, because a precommit counts only for an epoch
    /// whose round one did.
    pub precommitted_stake: u128,
}

impl DnsBftEpochVerdictV1 {
    /// Round one: `3·P(E) > 2·W(E)`.
    pub fn round_one(&self) -> bool {
        dns_bft_quorum_v1(self.attested_stake, self.counted.total_stake)
    }

    /// **DNS-final**: round one, and `3·C(E) > 2·W(E)` over the same `W(E)`. Round one alone is never
    /// final.
    pub fn dns_final(&self) -> bool {
        self.round_one() && dns_bft_quorum_v1(self.precommitted_stake, self.counted.total_stake)
    }
}

/// **Decide one epoch** from the walk's verified inputs.
pub fn dns_bft_evaluate_epoch_v1(
    epoch: &DnsBftEpochV1,
    chain: &[DnsBftChainBlockV1],
    bonds: &[StakeBondRecord],
    attestations: &[DnsBftAttestationV1],
    precommits: &[PrecommitRecord],
    rules: &DnsBftRulesV1,
) -> DnsBftEpochVerdictV1 {
    decide_epoch(epoch, chain, bonds, attestations, &DnsBftPrecommitChainsV1::new(precommits), rules)
}

fn decide_epoch(
    epoch: &DnsBftEpochV1,
    chain: &[DnsBftChainBlockV1],
    bonds: &[StakeBondRecord],
    attestations: &[DnsBftAttestationV1],
    chains: &DnsBftPrecommitChainsV1<'_>,
    rules: &DnsBftRulesV1,
) -> DnsBftEpochVerdictV1 {
    let edge = dns_bft_window_lower_edge_daa_v1(chain, epoch, rules);
    let last = dns_bft_last_final_attestation_daa_v1(epoch, attestations, rules);
    let counted = dns_bft_counted_set_v1(bonds, epoch.anchor_daa_score, edge, &last, rules);
    let snapshot_commitment = dns_bft_snapshot_commitment_v1(epoch, &counted);
    let attested_stake = dns_bft_attested_stake_v1(epoch, &counted, attestations);
    let precommitted_stake = if dns_bft_quorum_v1(attested_stake, counted.total_stake) {
        dns_bft_precommitted_stake_v1(epoch, &counted, snapshot_commitment, chains, rules)
    } else {
        0
    };
    DnsBftEpochVerdictV1 { epoch: *epoch, counted, snapshot_commitment, attested_stake, precommitted_stake }
}

/// Decide every evaluated epoch, in the order given, grouping the precommits once.
pub fn dns_bft_evaluate_epochs_v1(
    epochs: &[DnsBftEpochV1],
    chain: &[DnsBftChainBlockV1],
    bonds: &[StakeBondRecord],
    attestations: &[DnsBftAttestationV1],
    precommits: &[PrecommitRecord],
    rules: &DnsBftRulesV1,
) -> Vec<DnsBftEpochVerdictV1> {
    let chains = DnsBftPrecommitChainsV1::new(precommits);
    epochs.iter().map(|e| decide_epoch(e, chain, bonds, attestations, &chains, rules)).collect()
}

/// The DNS-final epochs among `verdicts`.
pub fn dns_final_epochs_v1(verdicts: &[DnsBftEpochVerdictV1]) -> BTreeSet<u64> {
    verdicts.iter().filter(|v| v.dns_final()).map(|v| v.epoch.epoch).collect()
}

/// The newest DNS-final epoch among `verdicts`.
pub fn newest_dns_final_v1(verdicts: &[DnsBftEpochVerdictV1]) -> Option<DnsBftEpochV1> {
    verdicts.iter().filter(|v| v.dns_final()).max_by_key(|v| v.epoch.epoch).map(|v| v.epoch)
}

/// **The confirmed anchor a sink's evaluation leaves (Decision 5)**, as `(anchor, anchor DAA)`.
///
/// `carried` is the previous confirmation, already known by the caller to be one this rule made and
/// still a chain ancestor of the sink. The newest DNS-final anchor replaces it when it is newer;
/// otherwise it is carried forward — both lie on the sink's chain, so the newer one descends from the
/// older and confirmation never moves back.
pub fn dns_bft_confirmed_anchor_v1(carried: Option<(Hash64, u64)>, newest_final: Option<(Hash64, u64)>) -> Option<(Hash64, u64)> {
    match (carried, newest_final) {
        (Some(carried), Some(newest)) if newest.1 > carried.1 => Some(newest),
        (Some(carried), _) => Some(carried),
        (None, newest) => newest,
    }
}

/// **What round two asks of one bond at the sink (Decision 6)**, read from the chain.
///
/// * `held` — the lock the chain shows the bond carrying, under the newest evaluated epoch's horizon
///   (the one its next precommit will most likely be judged by; where the chain shows the same
///   lock under every horizon, which is the case for any bond that has not misdeclared, the choice
///   does not matter).
/// * `due` — every evaluated epoch whose round one reached quorum, newer than the held lock (a lock
///   must name a strictly earlier epoch), in whose counted set the bond is, and for which the chain
///   shows no precommit from the bond at all (a second precommit for one epoch that differs from
///   the first is equivocation). Ascending, each with its anchor, anchor DAA and snapshot commitment.
pub fn dns_bft_precommit_duty_v1(
    round_active: bool,
    sink_daa_score: u64,
    verdicts: &[DnsBftEpochVerdictV1],
    precommits: &[PrecommitRecord],
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    rules: &DnsBftRulesV1,
) -> PrecommitDuty {
    let mut duty = PrecommitDuty { round_active, sink_daa_score, ..Default::default() };
    if !round_active {
        return duty;
    }
    let horizon = verdicts
        .last()
        .map_or_else(|| PrecommitLockHorizonV1::from_genesis(rules), |v| PrecommitLockHorizonV1::for_epoch(&v.epoch, rules));
    let chains = DnsBftPrecommitChainsV1::new(precommits);
    duty.held = chains.held_lock(validator_id, bond_outpoint, &horizon);
    let mut due: Vec<&DnsBftEpochVerdictV1> = verdicts
        .iter()
        .filter(|v| v.round_one())
        .filter(|v| v.epoch.epoch > duty.held.epoch)
        .filter(|v| v.counted.vote_weight(&validator_id, &bond_outpoint).is_some())
        .filter(|v| !chains.signed_epoch(validator_id, bond_outpoint, v.epoch.epoch))
        .collect();
    due.sort_by_key(|v| v.epoch.epoch);
    duty.due =
        due.into_iter().map(|v| (v.epoch.epoch, v.epoch.anchor_hash, v.epoch.anchor_daa_score, v.snapshot_commitment)).collect();
    duty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dns_finality::BondStatus;

    // testnet-11's planned numbers (ADR-0128 §5) on a dense chain where blue score and DAA score
    // advance together, one per block: epochs of 100, a lag of 100, a backoff of 1, so epoch E's
    // anchor is block `100·E + 98` and the evidence window `L` is 5,040 + 200 + 100 + 100 = 5,440.
    const T_LEAK: u64 = 5_040;
    const REENTRY: u64 = 200;
    const EPOCH_LEN: u64 = 100;
    const LAG: u64 = 100;
    const BACKOFF: u64 = 1;
    const L: u64 = T_LEAK + REENTRY + EPOCH_LEN + LAG;

    fn block_hash(n: u64) -> Hash64 {
        Hash64::from_u64_word(n.wrapping_add(1))
    }

    fn rules_with_floor(min_retained_validators: u32) -> DnsBftRulesV1 {
        DnsBftRulesV1 {
            t_leak_daa: T_LEAK,
            reentry_final_depth_daa: REENTRY,
            min_retained_validators,
            evidence_window_blue_score: L,
            epoch_length_blue_score: EPOCH_LEN,
            anchor_backoff_blue_score: BACKOFF,
        }
    }

    fn rules() -> DnsBftRulesV1 {
        rules_with_floor(4)
    }

    /// The dense chain from `tip` down to genesis, tip first.
    fn dense_chain(tip: u64) -> Vec<DnsBftChainBlockV1> {
        (0..=tip).rev().map(|n| DnsBftChainBlockV1 { hash: block_hash(n), blue_score: n, daa_score: n }).collect()
    }

    /// Epoch `e`'s canonical anchor on the dense chain.
    fn epoch_at(e: u64) -> DnsBftEpochV1 {
        let n = anchor_cutoff_blue_score(e, EPOCH_LEN, BACKOFF);
        DnsBftEpochV1 { epoch: e, anchor_hash: block_hash(n), anchor_blue_score: n, anchor_daa_score: n }
    }

    fn outpoint(tag: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([tag; 64]), u32::from(tag))
    }

    fn validator(tag: u8) -> Hash64 {
        Hash64::from_bytes([tag.wrapping_add(0x80); 64])
    }

    /// Bond `tag`, of validator `tag` unless said otherwise, active from `activation`.
    fn bond_of(tag: u8, validator_tag: u8, amount: u64, activation: u64) -> StakeBondRecord {
        StakeBondRecord {
            version: 1,
            bond_outpoint: outpoint(tag),
            owner_pubkey_hash: validator(validator_tag),
            validator_pubkey_hash: validator(validator_tag),
            validator_pubkey: Vec::new(),
            amount,
            activation_daa_score: activation,
            created_daa_score: activation,
            unbonding_period_blocks: 10_000,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: BondStatus::Active,
        }
    }

    fn bond(tag: u8, amount: u64, activation: u64) -> StakeBondRecord {
        bond_of(tag, tag, amount, activation)
    }

    /// An attestation by `b` for epoch `e`, accepted `delay` blocks after the anchor.
    fn attest(b: &StakeBondRecord, e: u64, delay: u64) -> DnsBftAttestationV1 {
        let anchor = epoch_at(e);
        DnsBftAttestationV1 {
            validator_id: b.validator_pubkey_hash,
            bond_outpoint: b.bond_outpoint,
            epoch: e,
            anchor_hash: anchor.anchor_hash,
            anchor_daa_score: anchor.anchor_daa_score,
            accepted_blue_score: anchor.anchor_blue_score + delay,
            accepted_daa_score: anchor.anchor_daa_score + delay,
        }
    }

    /// A precommit by `b` for `epoch`, declaring `lock`, accepted at block `accepted`.
    fn precommit(
        b: &StakeBondRecord,
        epoch: &DnsBftEpochV1,
        lock: PrecommitLock,
        commitment: Hash64,
        accepted: u64,
    ) -> PrecommitRecord {
        PrecommitRecord {
            validator_id: b.validator_pubkey_hash,
            bond_outpoint: b.bond_outpoint,
            epoch: epoch.epoch,
            target_hash: epoch.anchor_hash,
            target_daa_score: epoch.anchor_daa_score,
            declared_lock: lock,
            snapshot_commitment: commitment,
            accepted_blue_score: accepted,
            accepted_daa_score: accepted,
        }
    }

    fn lock_of(epoch: &DnsBftEpochV1) -> PrecommitLock {
        PrecommitLock { epoch: epoch.epoch, anchor: epoch.anchor_hash }
    }

    #[test]
    fn the_walk_is_the_stake_score_window_plus_the_evidence_window() {
        let mut dns = crate::config::params::DEVNET_PARAMS.dns_params.clone().expect("devnet runs an overlay");
        dns.stake_score_window_blue_score = 1_500;
        dns.attestation_epoch_length_blue_score = EPOCH_LEN;
        dns.attestation_lag_blue_score = LAG;
        let gate = DnsBftGateV1 {
            activation: crate::config::params::ForkActivation::new(7_001),
            t_leak_daa: T_LEAK,
            reentry_final_depth_daa: REENTRY,
            min_retained_validators: 4,
        };
        assert_eq!(dns_bft_evidence_window_blue_score_v1(&gate, &dns), Some(5_440), "ADR-0128 §5: 5,040 + 200 + 100 + 100");
        assert_eq!(dns_bft_walk_blue_score_v1(&gate, &dns), Some(6_940), "Decision 3: 1,500 + 5,440");
        let overflowing = DnsBftGateV1 { t_leak_daa: u64::MAX, ..gate };
        assert_eq!(dns_bft_walk_blue_score_v1(&overflowing, &dns), None, "a walk that does not fit is not a number");
        assert!(DnsBftRulesV1::new(&overflowing, &dns).is_none());
    }

    #[test]
    fn the_quorum_is_strict_and_an_empty_denominator_is_never_a_quorum() {
        assert!(!dns_bft_quorum_v1(2, 3), "exactly two thirds is not above two thirds");
        assert!(!dns_bft_quorum_v1(6, 9));
        assert!(!dns_bft_quorum_v1(66, 99));
        assert!(dns_bft_quorum_v1(67, 100));
        assert!(dns_bft_quorum_v1(3, 4));
        assert!(dns_bft_quorum_v1(1, 1));
        assert!(!dns_bft_quorum_v1(0, 1));
        assert!(!dns_bft_quorum_v1(0, 0), "no stake is no quorum");
        assert!(!dns_bft_quorum_v1(5, 0), "not even with a signed count");
        assert!(dns_bft_quorum_v1(10, 4), "an over-count is clamped to the total, not wrapped");
        // Near the top of the range the comparison still does not overflow.
        let total = u128::MAX;
        assert!(!dns_bft_quorum_v1(total / 3 * 2, total));
        assert!(dns_bft_quorum_v1(total / 3 * 2 + 2, total));
    }

    /// Four validators of 100 at epoch 120, all attesting their epochs: the set every round test
    /// below counts against.
    fn four_attesting() -> (Vec<StakeBondRecord>, Vec<DnsBftChainBlockV1>, DnsBftEpochV1) {
        let bonds: Vec<StakeBondRecord> = (1..=4).map(|t| bond(t, 100, 1_000)).collect();
        let e = epoch_at(120);
        (bonds, dense_chain(e.anchor_blue_score + 300), e)
    }

    #[test]
    fn one_denominator_serves_both_rounds_and_round_one_alone_is_never_final() {
        let (bonds, chain, e) = four_attesting();
        // Every bond attested recently, so nobody leaks and W(E) = 400.
        let mut atts: Vec<DnsBftAttestationV1> = bonds.iter().map(|b| attest(b, e.epoch - 5, 10)).collect();
        // Round one: three of four (300 > 266.7).
        atts.extend(bonds[..3].iter().map(|b| attest(b, e.epoch, 5)));
        let without_precommits = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &[], &rules());
        assert_eq!(without_precommits.counted.total_stake, 400);
        assert_eq!(without_precommits.attested_stake, 300);
        assert!(without_precommits.round_one());
        assert!(!without_precommits.dns_final(), "round one alone is never final");

        let commitment = without_precommits.snapshot_commitment;
        let two: Vec<PrecommitRecord> =
            bonds[..2].iter().map(|b| precommit(b, &e, PrecommitLock::default(), commitment, e.anchor_blue_score + 20)).collect();
        let short = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &two, &rules());
        assert_eq!(short.precommitted_stake, 200);
        assert!(!short.dns_final(), "200 of 400 precommitted is not a quorum of the same W(E)");

        let three: Vec<PrecommitRecord> =
            bonds[..3].iter().map(|b| precommit(b, &e, PrecommitLock::default(), commitment, e.anchor_blue_score + 20)).collect();
        let final_ = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &three, &rules());
        assert_eq!(final_.precommitted_stake, 300);
        assert!(final_.dns_final(), "both rounds above two thirds of one W(E)");
        assert_eq!(dns_final_epochs_v1(std::slice::from_ref(&final_)), BTreeSet::from([e.epoch]));

        // A precommit counts only for an epoch whose round one reached quorum: with two attesters,
        // three precommits count for nothing.
        let two_attest: Vec<DnsBftAttestationV1> =
            atts.iter().filter(|a| a.epoch != e.epoch || a.bond_outpoint != bonds[2].bond_outpoint).copied().collect();
        let no_round_one = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &two_attest, &three, &rules());
        assert!(!no_round_one.round_one());
        assert_eq!(no_round_one.precommitted_stake, 0, "round two waits for round one");
        assert!(!no_round_one.dns_final());

        // One vote per (validator, bond): the same attestation carried twice is still 100.
        let mut doubled = atts.clone();
        doubled.push(attest(&bonds[0], e.epoch, 7));
        assert_eq!(dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &doubled, &[], &rules()).attested_stake, 300);
    }

    #[test]
    fn a_vote_weighs_its_bond_once_per_validator_and_bond() {
        // One validator holding two bonds votes with both amounts; a third bond it does not own does
        // not lend it weight.
        let e = epoch_at(120);
        let chain = dense_chain(e.anchor_blue_score + 300);
        let bonds =
            vec![bond_of(1, 1, 100, 1_000), bond_of(2, 1, 50, 1_000), bond(3, 100, 1_000), bond(4, 100, 1_000), bond(5, 100, 1_000)];
        let mut atts: Vec<DnsBftAttestationV1> = bonds.iter().map(|b| attest(b, e.epoch - 5, 10)).collect();
        atts.push(attest(&bonds[0], e.epoch, 5));
        atts.push(attest(&bonds[1], e.epoch, 5));
        // A vote under bond 3 that names validator 1 is not bond 3's vote.
        let mut forged = attest(&bonds[2], e.epoch, 5);
        forged.validator_id = bonds[0].validator_pubkey_hash;
        atts.push(forged);
        let verdict = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &[], &rules());
        assert_eq!(verdict.counted.total_stake, 450);
        assert_eq!(verdict.attested_stake, 150);
    }

    #[test]
    fn a_precommit_bound_to_another_snapshot_does_not_count() {
        let (bonds, chain, e) = four_attesting();
        let mut atts: Vec<DnsBftAttestationV1> = bonds.iter().map(|b| attest(b, e.epoch - 5, 10)).collect();
        atts.extend(bonds.iter().map(|b| attest(b, e.epoch, 5)));
        let verdict = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &[], &rules());
        let right = verdict.snapshot_commitment;

        // The commitment another set would carry: one bond fewer, one amount different, one epoch
        // later, one anchor different.
        let mut fewer = verdict.counted.clone();
        fewer.bonds.pop();
        fewer.total_stake = 300;
        let mut richer = verdict.counted.clone();
        richer.bonds[0].amount = 101;
        richer.total_stake = 401;
        let later = DnsBftEpochV1 { epoch: e.epoch + 1, ..e };
        let elsewhere = DnsBftEpochV1 { anchor_hash: block_hash(1), ..e };
        let wrong = [
            dns_bft_snapshot_commitment_v1(&e, &fewer),
            dns_bft_snapshot_commitment_v1(&e, &richer),
            dns_bft_snapshot_commitment_v1(&later, &verdict.counted),
            dns_bft_snapshot_commitment_v1(&elsewhere, &verdict.counted),
        ];
        for w in wrong {
            assert_ne!(w, right, "the commitment binds the epoch, the anchor and the counted set");
        }
        // The root is over the set, not the order a caller happened to list it in.
        let mut reversed = verdict.counted.clone();
        reversed.bonds.reverse();
        assert_eq!(dns_bft_snapshot_root_v1(&reversed), dns_bft_snapshot_root_v1(&verdict.counted));

        let at = e.anchor_blue_score + 20;
        let mut pcs: Vec<PrecommitRecord> = bonds[..2].iter().map(|b| precommit(b, &e, PrecommitLock::default(), right, at)).collect();
        pcs.push(precommit(&bonds[2], &e, PrecommitLock::default(), wrong[0], at));
        let one_mismatched = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &pcs, &rules());
        assert_eq!(one_mismatched.precommitted_stake, 200, "the precommit bound to a smaller set is not counted against this one");
        assert!(!one_mismatched.dns_final());
        pcs[2].snapshot_commitment = right;
        assert!(dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &pcs, &rules()).dns_final());
    }

    #[test]
    fn lock_consistency_truncates_at_a_misdeclaration() {
        let b = bond(1, 100, 1_000);
        let (e1, e2, e3, e4) = (epoch_at(101), epoch_at(102), epoch_at(103), epoch_at(104));
        let c = Hash64::from_bytes([9; 64]);
        let p1 = precommit(&b, &e1, PrecommitLock::default(), c, e1.anchor_blue_score + 150);
        let p2 = precommit(&b, &e2, lock_of(&e1), c, e2.anchor_blue_score + 150);
        // p3 forgets the lock p2 took and restates p1's instead.
        let p3 = precommit(&b, &e3, lock_of(&e1), c, e3.anchor_blue_score + 150);
        // p4 declares p3 correctly — but p3 never counted, so neither does p4.
        let p4 = precommit(&b, &e4, lock_of(&e3), c, e4.anchor_blue_score + 150);
        let records = vec![p4.clone(), p2.clone(), p3.clone(), p1.clone()];
        let whole = PrecommitLockHorizonV1::from_genesis(&rules());
        let kept = lock_consistent_precommits(&records, &whole);
        assert_eq!(kept, vec![&p1, &p2], "chain order, cut at the first misdeclared lock");
        assert_eq!(held_precommit_lock(&records, b.validator_pubkey_hash, b.bond_outpoint, &whole), lock_of(&e2));

        // A rebroadcast of a vote already counted is the same vote: kept, and it moves nothing.
        let mut rebroadcast = p1.clone();
        rebroadcast.accepted_blue_score = p2.accepted_blue_score + 10;
        rebroadcast.accepted_daa_score = p2.accepted_daa_score + 10;
        let p3_right = precommit(&b, &e3, lock_of(&e2), c, e3.anchor_blue_score + 150);
        let with_rebroadcast = vec![p1.clone(), p2.clone(), rebroadcast.clone(), p3_right.clone()];
        assert_eq!(lock_consistent_precommits(&with_rebroadcast, &whole), vec![&p1, &p2, &rebroadcast, &p3_right]);
        assert_eq!(held_precommit_lock(&with_rebroadcast, b.validator_pubkey_hash, b.bond_outpoint, &whole), lock_of(&e3));

        // The whole chain in view: the first precommit must declare no lock at all.
        let invented = precommit(&b, &e2, lock_of(&e1), c, e2.anchor_blue_score + 150);
        assert!(lock_consistent_precommits(std::slice::from_ref(&invented), &whole).is_empty());

        // A horizon above p1: p2 is the first in view, and the lock it declares (p1's) could only
        // have been accepted below the horizon, so it stands.
        let above_p1 = PrecommitLockHorizonV1 { visible_from_blue_score: p1.accepted_blue_score + 1, ..whole };
        assert_eq!(lock_consistent_precommits(&[p1.clone(), p2.clone()], &above_p1), vec![&p2]);
        // But a first precommit in view may not declare a lock the horizon would show: e3's
        // precommit could only be accepted above e3's cutoff, which is in view.
        let claims_unseen = precommit(&b, &e4, lock_of(&e3), c, e4.anchor_blue_score + 150);
        assert!(
            lock_consistent_precommits(std::slice::from_ref(&claims_unseen), &above_p1).is_empty(),
            "a lock at an epoch whose precommit would be in view, and is not, is a misdeclaration"
        );

        // The count follows the truncation: a precommit after the cut does not vote.
        let bonds: Vec<StakeBondRecord> = (1..=4).map(|t| bond(t, 100, 1_000)).collect();
        let chain = dense_chain(e4.anchor_blue_score + 300);
        let mut atts: Vec<DnsBftAttestationV1> = bonds.iter().map(|x| attest(x, 95, 10)).collect();
        atts.extend(bonds.iter().map(|x| attest(x, e4.epoch, 5)));
        let commitment = dns_bft_evaluate_epoch_v1(&e4, &chain, &bonds, &atts, &[], &rules()).snapshot_commitment;
        let mut pcs = vec![p1, p2, p3];
        pcs.push(precommit(&bonds[0], &e4, lock_of(&e3), commitment, e4.anchor_blue_score + 150));
        pcs.extend(bonds[1..3].iter().map(|x| precommit(x, &e4, PrecommitLock::default(), commitment, e4.anchor_blue_score + 150)));
        let verdict = dns_bft_evaluate_epoch_v1(&e4, &chain, &bonds, &atts, &pcs, &rules());
        assert_eq!(verdict.precommitted_stake, 200, "bond 1's precommit follows its misdeclaration and does not count");
        assert!(!verdict.dns_final());
    }

    /// Everything the leak tests share: an epoch deep in a chain with a window longer than
    /// `t_leak_daa`, and four validators who attest every so often and never leak — so the floor
    /// never holds and each bond under test is judged on its own evidence.
    fn leak_fixture() -> (DnsBftEpochV1, Vec<DnsBftChainBlockV1>, Vec<StakeBondRecord>, Vec<DnsBftAttestationV1>) {
        let e = epoch_at(200); // anchor at 20,098
        assert!(L > T_LEAK, "the window is longer than the silence it measures");
        let chain = dense_chain(e.anchor_blue_score + 1_000);
        let steady: Vec<StakeBondRecord> = (1..=4).map(|t| bond(t, 100, 100)).collect();
        let atts = steady.iter().flat_map(|b| [attest(b, 170, 50), attest(b, 190, 50)]).collect();
        (e, chain, steady, atts)
    }

    fn counted(
        e: &DnsBftEpochV1,
        chain: &[DnsBftChainBlockV1],
        bonds: &[StakeBondRecord],
        atts: &[DnsBftAttestationV1],
        rules: &DnsBftRulesV1,
    ) -> DnsBftCountedSetV1 {
        let edge = dns_bft_window_lower_edge_daa_v1(chain, e, rules);
        dns_bft_counted_set_v1(bonds, e.anchor_daa_score, edge, &dns_bft_last_final_attestation_daa_v1(e, atts, rules), rules)
    }

    #[test]
    fn the_leak_measures_an_attester_from_its_youngest_final_attestation() {
        let (e, chain, mut bonds, mut atts) = leak_fixture();
        // Bond 10 attested epoch 148 (anchor 14,898: 5,200 below) and epoch 150 (anchor 15,098: 5,000
        // below). Both are inside the window; the youngest decides, and 5,000 < 5,040.
        let recent = bond(10, 100, 100);
        atts.push(attest(&recent, 148, 30));
        atts.push(attest(&recent, 150, 30));
        // Bond 11 attested only epoch 148: 5,200 of silence.
        let silent = bond(11, 100, 100);
        atts.push(attest(&silent, 148, 30));
        bonds.extend([recent.clone(), silent.clone()]);

        let last = dns_bft_last_final_attestation_daa_v1(&e, &atts, &rules());
        assert_eq!(last.get(&recent.bond_outpoint), Some(&epoch_at(150).anchor_daa_score));
        assert_eq!(last.get(&silent.bond_outpoint), Some(&epoch_at(148).anchor_daa_score));
        let set = counted(&e, &chain, &bonds, &atts, &rules());
        assert_eq!(set.leaked, vec![silent.bond_outpoint], "5,200 of silence leaks, 5,000 does not");
        assert!(set.get(&recent.bond_outpoint).is_some());
        assert!(!set.floor_held);
        assert_eq!(set.total_stake, 500);

        // The boundary is `≥ t_leak`: an attestation exactly 5,040 below leaks, 5,039 does not.
        let exact = DnsBftAttestationV1 { anchor_daa_score: e.anchor_daa_score - T_LEAK, ..attest(&silent, 149, 0) };
        let mut at_the_boundary = atts.clone();
        at_the_boundary.push(exact);
        // Its "epoch 149" anchor is off the dense grid; the rules read only the numbers.
        assert!(counted(&e, &chain, &bonds, &at_the_boundary, &rules()).leaked.contains(&silent.bond_outpoint));
        let inside = DnsBftAttestationV1 { anchor_daa_score: e.anchor_daa_score - T_LEAK + 1, ..exact };
        at_the_boundary.push(inside);
        assert!(!counted(&e, &chain, &bonds, &at_the_boundary, &rules()).leaked.contains(&silent.bond_outpoint));
    }

    #[test]
    fn an_absent_old_bond_is_measured_from_the_window_edge() {
        let (e, chain, mut bonds, atts) = leak_fixture();
        let edge = dns_bft_window_lower_edge_daa_v1(&chain, &e, &rules());
        assert_eq!(edge, e.anchor_daa_score - L, "the oldest block inside the window, as a DAA score");
        // Bond 12 has been bonded since block 100 and never attested inside the window: it is measured
        // from the edge, 5,440 below the anchor, and leaks.
        let absent = bond(12, 100, 100);
        bonds.push(absent.clone());
        let set = counted(&e, &chain, &bonds, &atts, &rules());
        assert_eq!(set.leaked, vec![absent.bond_outpoint]);

        // An attestation older than the window is not evidence, even when the walk holds it: the
        // bond is measured from the edge all the same.
        let mut with_ancient = atts.clone();
        with_ancient.push(attest(&absent, 100, 10));
        assert!(!dns_bft_last_final_attestation_daa_v1(&e, &with_ancient, &rules()).contains_key(&absent.bond_outpoint));
        assert_eq!(counted(&e, &chain, &bonds, &with_ancient, &rules()).leaked, vec![absent.bond_outpoint]);

        // On a chain younger than the window the edge is genesis, and an absent bond falls back to
        // its own activation.
        let young_epoch = epoch_at(30); // anchor at 3,098
        let young_chain = dense_chain(young_epoch.anchor_blue_score + 300);
        assert_eq!(dns_bft_window_lower_edge_daa_v1(&young_chain, &young_epoch, &rules()), 0);
    }

    #[test]
    fn an_absent_young_bond_is_measured_from_its_activation() {
        let (e, chain, mut bonds, atts) = leak_fixture();
        // Bonded 3,000 before the anchor, after the window's edge: 3,000 < 5,040, never leaked for a
        // silence it could not have broken.
        let newcomer = bond(13, 100, e.anchor_daa_score - 3_000);
        // Bonded 5,100 before the anchor — still after the edge (5,440) — and silent since: leaked,
        // measured from its activation rather than from the edge.
        let lapsed = bond(14, 100, e.anchor_daa_score - 5_100);
        bonds.extend([newcomer.clone(), lapsed.clone()]);
        let set = counted(&e, &chain, &bonds, &atts, &rules());
        assert!(set.get(&newcomer.bond_outpoint).is_some(), "SA-4: a young bond is not silence");
        assert_eq!(set.leaked, vec![lapsed.bond_outpoint]);
    }

    #[test]
    fn re_entry_waits_for_an_attestation_that_is_itself_buried() {
        let (e, chain, mut bonds, mut atts) = leak_fixture();
        let returning = bond(15, 100, 100);
        bonds.push(returning.clone());
        // Silent for the whole window, then it attests epoch 199 — anchor 19,998, 100 below the
        // anchor, which is less than the 200 re-entry depth: not yet evidence, still leaked.
        atts.push(attest(&returning, 199, 50));
        assert_eq!(counted(&e, &chain, &bonds, &atts, &rules()).leaked, vec![returning.bond_outpoint]);
        // Epoch 197 — anchor 19,798, 300 below — is buried: it re-enters.
        atts.push(attest(&returning, 197, 50));
        let set = counted(&e, &chain, &bonds, &atts, &rules());
        assert!(set.leaked.is_empty());
        assert!(set.get(&returning.bond_outpoint).is_some());

        // Evidence accepted by a block ABOVE the anchor is not in the prefix that ends there, however
        // old the epoch it names.
        let mut late = atts.clone();
        late.retain(|a| a.bond_outpoint != returning.bond_outpoint);
        late.push(attest(&returning, 197, 400)); // accepted at 20,198 > 20,098
        assert_eq!(counted(&e, &chain, &bonds, &late, &rules()).leaked, vec![returning.bond_outpoint]);
    }

    #[test]
    fn the_floor_halts_rather_than_leaks() {
        let e = epoch_at(200);
        let chain = dense_chain(e.anchor_blue_score + 1_000);
        let bonds: Vec<StakeBondRecord> = (1..=5).map(|t| bond(t, 100, 100)).collect();
        // Three attest; two are silent for the whole window.
        let mut atts: Vec<DnsBftAttestationV1> = bonds[..3].iter().map(|b| attest(b, 190, 50)).collect();
        atts.extend(bonds[..3].iter().map(|b| attest(b, e.epoch, 20)));

        // Floor of four: leaking two would leave three validators, so nothing leaks, W(E) stays 500,
        // and three of five is not a quorum — finality waits.
        let held = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &[], &rules_with_floor(4));
        assert!(held.counted.floor_held);
        assert!(held.counted.leaked.is_empty());
        assert_eq!(held.counted.total_stake, 500);
        assert!(!held.round_one(), "the floor is a halt, not a hole");

        // A floor of three is not breached by the same leak: W(E) is 300 and the three have quorum.
        let leaked = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &[], &rules_with_floor(3));
        assert!(!leaked.counted.floor_held);
        assert_eq!(leaked.counted.leaked.len(), 2);
        assert_eq!(leaked.counted.total_stake, 300);
        assert!(leaked.round_one());

        // A set that is small without any leak is not a held floor: there was nothing to leak.
        let small: Vec<StakeBondRecord> = bonds[..3].to_vec();
        let set = counted(&e, &chain, &small, &atts, &rules_with_floor(4));
        assert!(!set.floor_held && set.leaked.is_empty());
    }

    #[test]
    fn the_evidence_an_epoch_reads_does_not_depend_on_how_far_a_walk_reached() {
        let r = rules();
        let e = epoch_at(200);
        let floor = r.evidence_floor_blue_score(&e);
        assert_eq!(floor, e.anchor_blue_score - L);
        // Epoch 146's previous cutoff (14,498) is below the floor (14,658): an attestation for it is
        // not evidence for epoch 200, even accepted inside the window by a walk that could derive
        // its anchor.
        assert!(!r.anchor_decidable_within(146, &e));
        assert!(r.anchor_decidable_within(148, &e));
        let b = bond(1, 100, 100);
        let inside_window_old_epoch =
            DnsBftAttestationV1 { accepted_blue_score: floor + 5, accepted_daa_score: floor + 5, ..attest(&b, 146, 0) };
        assert!(dns_bft_last_final_attestation_daa_v1(&e, &[inside_window_old_epoch], &r).is_empty());
        // A block below the floor does not accept evidence either.
        let below = DnsBftAttestationV1 { accepted_blue_score: floor - 1, accepted_daa_score: floor - 1, ..attest(&b, 148, 0) };
        assert!(dns_bft_last_final_attestation_daa_v1(&e, &[below], &r).is_empty());
    }

    #[test]
    fn the_window_epochs_are_the_ones_the_credit_walk_evaluates() {
        // Dense chain to 1,000: ready through epoch 8 (1,000 ≥ 899 + 100), and down while the
        // previous epoch's cutoff (100·(E−1) + 98) is inside the window. A window of 300 holds
        // blocks down to 700, so epoch 7 (previous cutoff 698) is out; 302 holds 698, so it is in.
        let chain = dense_chain(1_000);
        let epochs = dns_bft_window_epochs_v1(&chain, 1_000, 300, EPOCH_LEN, LAG, BACKOFF);
        assert_eq!(epochs, vec![epoch_at(8)]);
        let epochs = dns_bft_window_epochs_v1(&chain, 1_000, 302, EPOCH_LEN, LAG, BACKOFF);
        assert_eq!(epochs, vec![epoch_at(7), epoch_at(8)]);

        // A sparse chain whose blue score jumps over a whole epoch: the jumped epoch reuses the
        // previous anchor and is skipped as a duplicate.
        let sparse: Vec<DnsBftChainBlockV1> = [1_000u64, 980, 950, 890, 880, 650, 640, 600]
            .iter()
            .map(|&n| DnsBftChainBlockV1 { hash: block_hash(n), blue_score: n, daa_score: n })
            .collect();
        let epochs = dns_bft_window_epochs_v1(&sparse, 1_000, 400, EPOCH_LEN, LAG, BACKOFF);
        // Epoch 7 (cutoff 798) anchors at 650; epoch 6 (cutoff 698) also at 650 — so 7 is a duplicate.
        assert!(epochs.iter().all(|e| e.epoch != 7), "a duplicate anchor earns no evaluation: {epochs:?}");
        assert!(epochs.iter().any(|e| e.epoch == 8 && e.anchor_blue_score == 890));
        assert!(dns_bft_window_epochs_v1(&chain[..50], 1_000, 300, EPOCH_LEN, 2_000, BACKOFF).is_empty(), "nothing is ready");
    }

    #[test]
    fn confirmation_advances_to_the_newest_final_anchor_and_never_moves_back() {
        let (a, b) = ((block_hash(1), 100u64), (block_hash(2), 200u64));
        assert_eq!(dns_bft_confirmed_anchor_v1(None, None), None);
        assert_eq!(dns_bft_confirmed_anchor_v1(None, Some(a)), Some(a));
        assert_eq!(dns_bft_confirmed_anchor_v1(Some(a), None), Some(a), "carried while nothing newer is final");
        assert_eq!(dns_bft_confirmed_anchor_v1(Some(a), Some(b)), Some(b));
        assert_eq!(dns_bft_confirmed_anchor_v1(Some(b), Some(a)), Some(b), "an older final anchor does not replace a newer one");
        assert_eq!(dns_bft_confirmed_anchor_v1(Some(a), Some(a)), Some(a));

        // The newest final epoch, not the newest epoch.
        let (bonds, chain, e) = four_attesting();
        let atts: Vec<DnsBftAttestationV1> = bonds.iter().map(|x| attest(x, e.epoch, 5)).collect();
        let verdict = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &[], &rules());
        let pcs: Vec<PrecommitRecord> = bonds
            .iter()
            .map(|x| precommit(x, &e, PrecommitLock::default(), verdict.snapshot_commitment, e.anchor_blue_score + 30))
            .collect();
        let final_ = dns_bft_evaluate_epoch_v1(&e, &chain, &bonds, &atts, &pcs, &rules());
        let newer_not_final = DnsBftEpochVerdictV1 { epoch: epoch_at(e.epoch + 1), ..verdict };
        assert_eq!(newest_dns_final_v1(&[final_, newer_not_final]), Some(e));
    }

    #[test]
    fn the_duty_answers_what_the_chain_shows() {
        let bonds: Vec<StakeBondRecord> = (1..=4).map(|t| bond(t, 100, 1_000)).collect();
        let (e1, e2, e3) = (epoch_at(120), epoch_at(121), epoch_at(122));
        let chain = dense_chain(e3.anchor_blue_score + 300);
        let mut atts: Vec<DnsBftAttestationV1> = Vec::new();
        for e in [e1, e2] {
            atts.extend(bonds.iter().map(|b| attest(b, e.epoch, 5)));
        }
        // Epoch 122 reached only two attesters: no round one, nothing due there.
        atts.extend(bonds[..2].iter().map(|b| attest(b, e3.epoch, 5)));
        let bare = dns_bft_evaluate_epochs_v1(&[e1, e2, e3], &chain, &bonds, &atts, &[], &rules());
        let me = &bonds[0];
        // Bond 1 precommitted epoch 120; it holds that lock.
        let pcs = vec![precommit(me, &e1, PrecommitLock::default(), bare[0].snapshot_commitment, e1.anchor_blue_score + 150)];
        let verdicts = dns_bft_evaluate_epochs_v1(&[e1, e2, e3], &chain, &bonds, &atts, &pcs, &rules());
        let duty = dns_bft_precommit_duty_v1(true, 99, &verdicts, &pcs, me.validator_pubkey_hash, me.bond_outpoint, &rules());
        assert!(duty.round_active);
        assert_eq!(duty.sink_daa_score, 99);
        assert_eq!(duty.held, lock_of(&e1));
        assert_eq!(duty.due, vec![(e2.epoch, e2.anchor_hash, e2.anchor_daa_score, verdicts[1].snapshot_commitment)]);

        // Bond 2 has precommitted nothing: both round-one epochs are due, ascending, each with its
        // own commitment, and it holds no lock.
        let other = &bonds[1];
        let duty = dns_bft_precommit_duty_v1(true, 99, &verdicts, &pcs, other.validator_pubkey_hash, other.bond_outpoint, &rules());
        assert_eq!(duty.held, PrecommitLock::default());
        assert_eq!(
            duty.due,
            vec![
                (e1.epoch, e1.anchor_hash, e1.anchor_daa_score, verdicts[0].snapshot_commitment),
                (e2.epoch, e2.anchor_hash, e2.anchor_daa_score, verdicts[1].snapshot_commitment),
            ]
        );
        assert_ne!(verdicts[0].snapshot_commitment, verdicts[1].snapshot_commitment);

        // A precommit for epoch 121 the chain shows — even one that does not count (another anchor) —
        // takes 121 off the list: a second, different precommit for one epoch is equivocation.
        let mut elsewhere = precommit(other, &e2, PrecommitLock::default(), Hash64::default(), e2.anchor_blue_score + 150);
        elsewhere.target_hash = block_hash(7);
        let duty =
            dns_bft_precommit_duty_v1(true, 99, &verdicts, &[elsewhere], other.validator_pubkey_hash, other.bond_outpoint, &rules());
        assert_eq!(duty.due.iter().map(|d| d.0).collect::<Vec<_>>(), Vec::<u64>::new(), "121 is signed, and 120 is below its lock");

        // A bond outside every counted set owes nothing; below the fence nothing is answered.
        let stranger = bond(9, 100, 1_000);
        assert!(
            dns_bft_precommit_duty_v1(true, 99, &verdicts, &pcs, stranger.validator_pubkey_hash, stranger.bond_outpoint, &rules())
                .due
                .is_empty()
        );
        let dormant = dns_bft_precommit_duty_v1(false, 99, &verdicts, &pcs, me.validator_pubkey_hash, me.bond_outpoint, &rules());
        assert_eq!(dormant, PrecommitDuty { round_active: false, sink_daa_score: 99, ..Default::default() });
    }

    #[test]
    fn precommits_are_read_from_their_own_subnetwork_only() {
        let p = StakePrecommitPayload {
            version: crate::dns_finality::DNS_PAYLOAD_VERSION_V1,
            validator_id: validator(1),
            bond_outpoint: outpoint(1),
            epoch: 5,
            target_hash: block_hash(5),
            target_daa_score: 5,
            locked_epoch: 0,
            locked_hash: Hash64::default(),
            snapshot_commitment: Hash64::from_bytes([3; 64]),
            signature: vec![0; 4],
        };
        let tx = crate::dns_finality::stake_precommit_tx(&p);
        let mut garbage = tx.clone();
        garbage.payload = vec![0xff];
        let mut elsewhere = tx.clone();
        elsewhere.subnetwork_id = crate::subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD;
        assert_eq!(precommits_from_accepted_txs(&[tx, garbage, elsewhere]), vec![p]);
    }
}
