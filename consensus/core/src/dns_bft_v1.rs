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

/// **`last_E(bond)` where the bond has evidence**: the anchor DAA of the youngest epoch it attested
/// in an attestation that is final for `epoch` (Decision 3).
///
/// Final means all of: accepted by a chain block at or below the epoch's anchor (so the evidence is
/// in the prefix that ends there, whichever sink reads it); that block inside the evidence window;
/// the attested anchor a strict ancestor of the epoch's and at least `reentry_final_depth_daa` below
/// it (ADR-0066 SA-2 — re-entry waits for an attestation that is itself buried); and the attested
/// anchor decidable inside the window ([`DnsBftRulesV1::anchor_decidable_within`]).
///
/// A bond absent from the map has no such evidence and is measured from
/// `max(activation, window lower edge)` instead.
pub fn dns_bft_last_final_attestation_daa_v1(
    epoch: &DnsBftEpochV1,
    attestations: &[DnsBftAttestationV1],
    rules: &DnsBftRulesV1,
) -> HashMap<TransactionOutpoint, u64> {
    let floor = rules.evidence_floor_blue_score(epoch);
    let mut last: HashMap<TransactionOutpoint, u64> = HashMap::new();
    for a in attestations {
        let accepted_in_prefix = a.accepted_daa_score <= epoch.anchor_daa_score && a.accepted_blue_score >= floor;
        let buried = a.anchor_daa_score < epoch.anchor_daa_score
            && a.anchor_daa_score.saturating_add(rules.reentry_final_depth_daa) <= epoch.anchor_daa_score;
        if !accepted_in_prefix || !buried || !rules.anchor_decidable_within(a.epoch, epoch) {
            continue;
        }
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
        .filter_map(|a| counted.vote_weight(&a.validator_id, &a.bond_outpoint).filter(|_| seen.insert((a.validator_id, a.bond_outpoint))))
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

/// Two precommits that cast the same vote — a rebroadcast, which `precommit_fault` does not convict
/// and the lock chain does not count twice or read as a misdeclaration.
fn same_vote(a: &PrecommitRecord, b: &PrecommitRecord) -> bool {
    a.epoch == b.epoch && a.target_hash == b.target_hash && a.target_daa_score == b.target_daa_score && a.declared_lock == b.declared_lock
}

/// One bond's precommits in view, in chain order, truncated at the first misdeclared lock; plus the
/// lock the surviving prefix leaves it holding.
///
/// Chain order is the accepting block's DAA score, ties broken by `(epoch, bond txid, index,
/// target)` so every node orders identically. The first precommit must declare no lock or a lock
/// the horizon hides; each later one must declare its predecessor's `(epoch, target)`. A rebroadcast
/// of a vote already in the prefix is kept (it is the same vote) and moves nothing.
fn lock_consistent_prefix<'a>(
    mut chain: Vec<&'a PrecommitRecord>,
    horizon: &PrecommitLockHorizonV1,
) -> (Vec<&'a PrecommitRecord>, PrecommitLock) {
    chain.retain(|r| horizon.sees(r));
    chain.sort_by_key(|r| (r.accepted_daa_score, r.epoch, r.bond_outpoint.transaction_id, r.bond_outpoint.index, r.target_hash));
    let mut held = PrecommitLock::default();
    let mut kept: Vec<&PrecommitRecord> = Vec::new();
    for r in chain {
        if kept.iter().any(|k| same_vote(k, r)) {
            kept.push(r);
            continue;
        }
        let consistent =
            if kept.is_empty() { r.declared_lock == PrecommitLock::default() || horizon.hides(&r.declared_lock) } else { r.declared_lock == held };
        if !consistent {
            break; // misdeclared: this one and everything after it are uncountable
        }
        held = PrecommitLock { epoch: r.epoch, anchor: r.target_hash };
        kept.push(r);
    }
    (kept, held)
}

/// **The precommits whose declared lock is the one the chain shows**, every bond's
/// [`lock_consistent_prefix`] under `horizon`, grouped in a deterministic order.
///
/// That severity is the point: the declaration is what a validator can later be held to, so it
/// has to be a faithful running record. A validator that misdeclares stops counting in round two
/// until the misdeclaration leaves the horizon, and if it misdeclared because it carries a different
/// lock on another branch, the two signed payloads are the proof (`precommit_fault`).
pub fn lock_consistent_precommits<'a>(records: &'a [PrecommitRecord], horizon: &PrecommitLockHorizonV1) -> Vec<&'a PrecommitRecord> {
    // `TransactionOutpoint` is not `Ord`, so group in a hash map and impose the order on the way out.
    let mut by_bond: HashMap<(Hash64, TransactionOutpoint), Vec<&PrecommitRecord>> = HashMap::new();
    for r in records {
        by_bond.entry((r.validator_id, r.bond_outpoint)).or_default().push(r);
    }
    let mut groups: Vec<_> = by_bond.into_iter().collect();
    groups.sort_by_key(|((validator, outpoint), _)| (*validator, outpoint_key(outpoint)));
    groups.into_iter().flat_map(|(_, chain)| lock_consistent_prefix(chain, horizon).0).collect()
}

/// **The lock `(validator_id, bond_outpoint)` carries on this chain** — the target of the last
/// precommit in its lock-consistent prefix under `horizon`, or no lock when it has none. What its
/// next precommit must declare.
pub fn held_precommit_lock(
    records: &[PrecommitRecord],
    validator_id: Hash64,
    bond_outpoint: TransactionOutpoint,
    horizon: &PrecommitLockHorizonV1,
) -> PrecommitLock {
    let mine: Vec<&PrecommitRecord> =
        records.iter().filter(|r| r.validator_id == validator_id && r.bond_outpoint == bond_outpoint).collect();
    lock_consistent_prefix(mine, horizon).1
}

/// **`C(E)`: the counted stake that precommitted the epoch's anchor under its snapshot commitment,
/// with the lock the chain shows.** One vote per `(validator_id, bond_outpoint)`.
pub fn dns_bft_precommitted_stake_v1(
    epoch: &DnsBftEpochV1,
    counted: &DnsBftCountedSetV1,
    snapshot_commitment: Hash64,
    precommits: &[PrecommitRecord],
    rules: &DnsBftRulesV1,
) -> u128 {
    let horizon = PrecommitLockHorizonV1::for_epoch(epoch, rules);
    let mut seen: HashSet<(Hash64, TransactionOutpoint)> = HashSet::new();
    lock_consistent_precommits(precommits, &horizon)
        .into_iter()
        .filter(|p| {
            p.epoch == epoch.epoch
                && p.target_hash == epoch.anchor_hash
                && p.target_daa_score == epoch.anchor_daa_score
                && p.snapshot_commitment == snapshot_commitment
        })
        .filter_map(|p| counted.vote_weight(&p.validator_id, &p.bond_outpoint).filter(|_| seen.insert((p.validator_id, p.bond_outpoint))))
        .fold(0u128, |acc, w| acc.saturating_add(w))
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
    let edge = dns_bft_window_lower_edge_daa_v1(chain, epoch, rules);
    let last = dns_bft_last_final_attestation_daa_v1(epoch, attestations, rules);
    let counted = dns_bft_counted_set_v1(bonds, epoch.anchor_daa_score, edge, &last, rules);
    let snapshot_commitment = dns_bft_snapshot_commitment_v1(epoch, &counted);
    let attested_stake = dns_bft_attested_stake_v1(epoch, &counted, attestations);
    let precommitted_stake = if dns_bft_quorum_v1(attested_stake, counted.total_stake) {
        dns_bft_precommitted_stake_v1(epoch, &counted, snapshot_commitment, precommits, rules)
    } else {
        0
    };
    DnsBftEpochVerdictV1 { epoch: *epoch, counted, snapshot_commitment, attested_stake, precommitted_stake }
}

/// Decide every evaluated epoch, in the order given.
pub fn dns_bft_evaluate_epochs_v1(
    epochs: &[DnsBftEpochV1],
    chain: &[DnsBftChainBlockV1],
    bonds: &[StakeBondRecord],
    attestations: &[DnsBftAttestationV1],
    precommits: &[PrecommitRecord],
    rules: &DnsBftRulesV1,
) -> Vec<DnsBftEpochVerdictV1> {
    epochs.iter().map(|e| dns_bft_evaluate_epoch_v1(e, chain, bonds, attestations, precommits, rules)).collect()
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
    let horizon =
        verdicts.last().map_or_else(|| PrecommitLockHorizonV1::from_genesis(rules), |v| PrecommitLockHorizonV1::for_epoch(&v.epoch, rules));
    duty.held = held_precommit_lock(precommits, validator_id, bond_outpoint, &horizon);
    let signed: BTreeSet<u64> =
        precommits.iter().filter(|p| p.validator_id == validator_id && p.bond_outpoint == bond_outpoint).map(|p| p.epoch).collect();
    let mut due: Vec<&DnsBftEpochVerdictV1> = verdicts
        .iter()
        .filter(|v| v.round_one())
        .filter(|v| v.epoch.epoch > duty.held.epoch)
        .filter(|v| v.counted.vote_weight(&validator_id, &bond_outpoint).is_some())
        .filter(|v| !signed.contains(&v.epoch.epoch))
        .collect();
    due.sort_by_key(|v| v.epoch.epoch);
    duty.due = due.into_iter().map(|v| (v.epoch.epoch, v.epoch.anchor_hash, v.epoch.anchor_daa_score, v.snapshot_commitment)).collect();
    duty
}
