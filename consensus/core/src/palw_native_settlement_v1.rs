//! RFC-0012: deterministic, DNS-free native settlement. No production policy is assigned here.
//! Inputs must come from one canonical, executed PALW snapshot; missing evidence never buys safety.

use crate::palw_state_v2::{
    PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwDeltaEntryV2, PalwFpPricingV1, PalwStateDeltaV2,
    PalwStateParamsV2, palw_fp_spend_weight_v1,
};
use crate::{Hash64, config::params::ForkActivation, subnets::*};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const NATIVE_ESCROW_PERMILLE_V1: u16 = 920;
pub const NATIVE_INCLUSION_BPS_V1: u16 = 800;
pub const NATIVE_SETTLEMENT_SNAPSHOT_VERSION_V1: u16 = 1;

/// The exact subsidy base (including deterministic integer rounding), withheld rather than minted
/// immediately. The escrow itself is floored at 92%; any excess is unallocated and never minted.
pub fn native_worker_base_v1(subsidy: u64) -> u64 {
    subsidy - (subsidy as u128 * NATIVE_INCLUSION_BPS_V1 as u128 / 10_000) as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwDnsRetirementV1 {
    pub activation: ForkActivation,
    pub settlement: PalwSettlementPolicyV1,
    /// Maximum accepting-DAA distance for historical DNS evidence after retirement.
    pub legacy_evidence_horizon_daa: u64,
}

impl PalwDnsRetirementV1 {
    pub fn commitment_bytes(self) -> Vec<u8> {
        let mut b = self.activation.daa_score().to_le_bytes().to_vec();
        b.extend(self.settlement.commitment_bytes());
        b.extend(self.legacy_evidence_horizon_daa.to_le_bytes());
        b.extend(NATIVE_ESCROW_PERMILLE_V1.to_le_bytes());
        b.extend(NATIVE_INCLUSION_BPS_V1.to_le_bytes());
        b.extend(0u16.to_le_bytes()); // DNS subsidy / normal fee / finality fee shares
        b.extend(10_000u16.to_le_bytes()); // normal and native-settlement fees go to their carrier
        b.extend(NATIVE_SETTLEMENT_SNAPSHOT_VERSION_V1.to_le_bytes());
        b.extend([0x10, 0x11, 0x19]); // new DNS bonds, attestation shards and precommits are retired
        b
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwSettlementPolicyV1 {
    pub settled_anchor_depth: u64,
    pub unique_mature_work: u128,
    pub max_operator_permille: u16,
    pub max_class_permille: u16,
}

impl PalwSettlementPolicyV1 {
    pub const VERSION: u16 = 1;
    pub fn valid(self) -> bool {
        self.settled_anchor_depth > 0
            && self.unique_mature_work > 0
            && (1..=1000).contains(&self.max_operator_permille)
            && (1..=1000).contains(&self.max_class_permille)
    }
    /// Stable bytes committed beside the activation height in both network identities.
    pub fn commitment_bytes(self) -> Vec<u8> {
        let mut b = Self::VERSION.to_le_bytes().to_vec();
        b.extend(self.settled_anchor_depth.to_le_bytes());
        b.extend(self.unique_mature_work.to_le_bytes());
        b.extend(self.max_operator_permille.to_le_bytes());
        b.extend(self.max_class_permille.to_le_bytes());
        b
    }
}

/// DNS history, exits and PALW objects retain their own validators. Peer-discovery DNS is unrelated.
pub fn creates_dns_participation(id: &crate::subnets::SubnetworkId) -> bool {
    matches!(*id, SUBNETWORK_ID_STAKE_BOND | SUBNETWORK_ID_STAKE_ATTESTATION_SHARD | SUBNETWORK_ID_STAKE_PRECOMMIT)
}

/// Historical DNS equivocation is admitted for a finite accepting-DAA horizon only.
/// PALW evidence and exits are not routed through this retired-role check.
pub fn legacy_dns_evidence_allowed_v1(r: PalwDnsRetirementV1, tx: &crate::tx::Transaction, daa: u64) -> bool {
    if !r.activation.is_active(daa) {
        return true;
    }
    let targets = if tx.subnetwork_id == SUBNETWORK_ID_SLASHING_EVIDENCE {
        borsh::from_slice::<crate::dns_finality::SlashingEvidencePayload>(&tx.payload)
            .ok()
            .map(|e| (e.attestation_a.target_daa_score, e.attestation_b.target_daa_score))
    } else if tx.subnetwork_id == SUBNETWORK_ID_PRECOMMIT_EVIDENCE {
        borsh::from_slice::<crate::dns_finality::PrecommitEvidencePayload>(&tx.payload)
            .ok()
            .map(|e| (e.precommit_a.target_daa_score, e.precommit_b.target_daa_score))
    } else {
        return true;
    };
    daa.saturating_sub(r.activation.daa_score()) <= r.legacy_evidence_horizon_daa
        && targets.is_some_and(|(a, b)| a < r.activation.daa_score() && b < r.activation.daa_score())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatureUsefulWorkV1 {
    /// Canonical execution / slice identity, never the carrying block's hash.
    pub identity: Hash64,
    pub anchor: Hash64,
    pub operator: Hash64,
    pub class: Hash64,
    pub anchor_blue: u64,
    pub accepted_blue: u64,
    pub anchor_daa: u64,
    pub accepted_daa: u64,
    pub matured_daa: u64,
    pub work: u128,
}

/// What one selected-chain block's PALW transition contributes to native-settlement evidence — a pure
/// function of that block's delta, so it can be computed once per block hash and cached.
///
/// **Why the evidence is read from deltas and not from the sink's claims.** A terminal claim leaves
/// PALW state at `terminal + claim_retirement` (3,000 DAA on testnet-12) while the producer's trace
/// retention outlives it (`bind + receipt + challenge + court`, 5,400); a reader that only sees claims
/// still held at the sink would see none of an ordinary claim's work at the DAA it matures. The delta of
/// the block that finalized the claim (or spent a free-prompt slice) carries the full claim record, and
/// deltas are kept for the whole selected chain above the pruning point.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NativeDeltaEvidenceV1 {
    /// REAL (attempt) claims that entered `Final` in this block, with their record at `Final`.
    pub finalized_attempts: Vec<(Hash64, PalwClaimStateV2)>,
    /// Free-prompt claims whose spent set grew in this block: the record after the spend and the newly spent quanta.
    pub fp_spends: Vec<(Hash64, PalwClaimStateV2, Vec<u32>)>,
    /// Claims that entered `Voided` in this block. Evidence recorded for them anywhere on the chain is retracted
    /// (a conviction after `Final` reverses it, `reverse_convicted_final`).
    pub voided: Vec<Hash64>,
}

impl NativeDeltaEvidenceV1 {
    pub fn is_empty(&self) -> bool {
        self.finalized_attempts.is_empty() && self.fp_spends.is_empty() && self.voided.is_empty()
    }
}

/// Read a block's native-settlement evidence out of its PALW delta. Entries are taken in application order;
/// a claim touched twice in one block contributes each transition once.
pub fn native_delta_evidence_v1(delta: &PalwStateDeltaV2) -> NativeDeltaEvidenceV1 {
    let mut out = NativeDeltaEvidenceV1::default();
    for entry in &delta.entries {
        let PalwDeltaEntryV2::Claim { key, old, new: Some(new) } = entry else {
            continue;
        };
        let was_final = old.as_ref().is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. }));
        let was_void = old.as_ref().is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Voided { .. }));
        match (&new.phase, &new.source) {
            (PalwClaimPhaseV2::Voided { .. }, _) if !was_void => out.voided.push(*key),
            (PalwClaimPhaseV2::Final { .. }, PalwClaimSourceV2::Attempt) if !was_final => {
                out.finalized_attempts.push((*key, new.clone()))
            }
            (PalwClaimPhaseV2::Final { .. }, PalwClaimSourceV2::FreePrompt { spent: after, .. }) => {
                let before = match old.as_ref().map(|c| &c.source) {
                    Some(PalwClaimSourceV2::FreePrompt { spent, .. }) => spent.clone(),
                    _ => Default::default(),
                };
                let fresh: Vec<u32> = after.difference(&before).copied().collect();
                if !fresh.is_empty() {
                    out.fp_spends.push((*key, new.clone(), fresh));
                }
            }
            _ => {}
        }
    }
    out
}

/// The sink-state facts the conversion from claim records to evidence needs.
pub struct NativeFactRulesV1<'a> {
    pub state: &'a PalwChainStateV2,
    pub params: &'a PalwStateParamsV2,
    /// `Params::palw_canonical_work_daa()`: a REAL attempt below it has no canonical weight and supplies none.
    pub canonical_work_daa: Option<u64>,
    /// The committed execution-quantum maturity window (DAA).
    pub quantum_maturity_daa: u64,
    /// Claims with a DA session open at the sink are not mature.
    pub claims_with_open_da: &'a BTreeSet<Hash64>,
}

fn fp_slice_identity(work_id: &Hash64, index: u32) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).to_state();
    h.update(b"MISAKA/native-settlement-slice/v1");
    h.update(&work_id.as_bytes());
    h.update(&index.to_le_bytes());
    Hash64::from_bytes(*h.finalize().as_array())
}

/// Turn one block's extracted evidence into facts. `accepting` is that block's `(daa, blue)`. Missing information
/// is never guessed: a bond the sink no longer holds, a claim without a canonical work id or weight, a floor
/// claim, a claim in `voided` and a claim with an open DA session contribute nothing.
///
/// **Maturity** is the latest of the claim's trace retention (the producer's own obligation to keep the trace
/// available), `final + claim_retirement` (after which no conviction can reverse the `Final`), and for a
/// free-prompt slice its execution-quantum maturity after the spend. A fact is therefore never counted while
/// the claim can still be convicted or its trace still be demanded.
pub fn native_facts_of_block_v1(
    rules: &NativeFactRulesV1<'_>,
    evidence: &NativeDeltaEvidenceV1,
    voided: &BTreeSet<Hash64>,
    accepting: (u64, u64),
) -> Vec<MatureUsefulWorkV1> {
    let (accepting_daa, accepting_blue) = accepting;
    let base = rules.params.base_class_id();
    let retirement = rules.params.claim_retirement_daa();
    let mut facts = Vec::new();
    for (id, claim) in &evidence.finalized_attempts {
        let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
            continue;
        };
        if voided.contains(id) || claim.class_id == base || rules.claims_with_open_da.contains(id) {
            continue;
        }
        let (Some(identity), Some(work), Some(bond)) =
            (claim.work_id, rules.state.palw_claim_canonical_weight_v1(claim, rules.canonical_work_daa), rules.state.bond(&claim.bond))
        else {
            continue;
        };
        facts.push(MatureUsefulWorkV1 {
            identity,
            anchor: claim.accepted_block,
            operator: bond.operator_id,
            class: claim.class_id,
            anchor_blue: claim.accepted_blue_score,
            accepted_blue: claim.accepted_blue_score,
            anchor_daa: claim.accepted_daa,
            accepted_daa: claim.accepted_daa,
            matured_daa: claim.trace_retention_daa.max(final_daa.saturating_add(retirement)),
            work,
        });
    }
    let pricing = PalwFpPricingV1::of(rules.params, rules.canonical_work_daa);
    for (id, claim, fresh) in &evidence.fp_spends {
        let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
            continue;
        };
        let PalwClaimSourceV2::FreePrompt { quanta, .. } = &claim.source else {
            continue;
        };
        if voided.contains(id) || claim.class_id == pricing.base_class_id || !pricing.prices_in_compute(claim) || rules.claims_with_open_da.contains(id)
        {
            continue;
        }
        let (Some(work_id), Some(bond)) = (claim.work_id, rules.state.bond(&claim.bond)) else {
            continue;
        };
        let matured_daa = final_daa
            .max(accepting_daa.saturating_add(rules.quantum_maturity_daa))
            .max(claim.trace_retention_daa)
            .max(final_daa.saturating_add(retirement));
        let work = palw_fp_spend_weight_v1(rules.state, claim, *quanta, &pricing);
        for index in fresh {
            facts.push(MatureUsefulWorkV1 {
                identity: fp_slice_identity(&work_id, *index),
                anchor: claim.accepted_block,
                operator: bond.operator_id,
                class: claim.class_id,
                anchor_blue: claim.accepted_blue_score,
                accepted_blue: accepting_blue,
                anchor_daa: claim.accepted_daa,
                accepted_daa: accepting_daa,
                matured_daa,
                work,
            });
        }
    }
    facts
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum SettlementStopV1 {
    InvalidPolicy,
    MissingHistory,
    Unexecuted,
    FrontierNotCovered,
    OpenLifecycle,
    DuplicateWork,
    InsufficientDepth,
    InsufficientWork,
    ConcentratedWork,
    ArithmeticOverflow,
    FinalizedConflict,
}

/// Versioned snapshot, committed in the same batch as the virtual UTXO set and canonical indexes.
/// The generation is the canonical L1 sink, not a node-local counter. An absent head is explicit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSettlementSnapshotV1 {
    pub version: u16,
    pub ruleset_id: crate::Hash,
    pub policy_id: Hash64,
    pub generation: Hash64,
    pub retirement_daa: u64,
    pub frontier: Option<Hash64>,
    pub latest: Option<Hash64>,
    pub safe: Option<Hash64>,
    pub finalized: Option<Hash64>,
    pub depth: u64,
    /// Decimal string: RPC consumers must not round consensus u128 quantities.
    pub unique_work: String,
    pub stop: Option<SettlementStopV1>,
}

impl PalwSettlementPolicyV1 {
    pub fn id(self) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).to_state();
        h.update(b"MISAKA/native-settlement-policy/v1");
        h.update(&self.commitment_bytes());
        Hash64::from_bytes(*h.finalize().as_array())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettlementEvidenceV1 {
    pub depth: u64,
    pub work: u128,
}

/// Add-only accumulation of the facts that qualify for one effect. The single implementation of the
/// evidence arithmetic: [`certify_native_effect_v1`] (the per-effect reference) and
/// [`certify_native_prefix_v1`] (the O(N + F) sweep) both read their answer from it.
#[derive(Clone, Debug, Default)]
struct EvidenceAccumulatorV1 {
    ids: BTreeSet<Hash64>,
    anchors: u64,
    operators: BTreeMap<Hash64, u128>,
    classes: BTreeMap<Hash64, u128>,
    max_operator: u128,
    max_class: u128,
    work: u128,
    duplicate: bool,
    overflow: bool,
}

impl EvidenceAccumulatorV1 {
    fn add_fact(&mut self, f: &MatureUsefulWorkV1) {
        if !self.ids.insert(f.identity) {
            self.duplicate = true;
            return;
        }
        if self.overflow {
            return;
        }
        let Some(work) = self.work.checked_add(f.work) else {
            self.overflow = true;
            return;
        };
        let (Some(op), Some(class)) = (
            self.operators.get(&f.operator).copied().unwrap_or(0).checked_add(f.work),
            self.classes.get(&f.class).copied().unwrap_or(0).checked_add(f.work),
        ) else {
            self.overflow = true;
            return;
        };
        self.work = work;
        self.operators.insert(f.operator, op);
        self.classes.insert(f.class, class);
        self.max_operator = self.max_operator.max(op);
        self.max_class = self.max_class.max(class);
    }

    /// A distinct anchor that is itself at or after the effect (counted once however many facts share it).
    fn add_anchor(&mut self) {
        self.anchors += 1;
    }

    /// The fact-dependent half of a certificate, in the reference order: a duplicated identity, then an
    /// overflow, then depth, work and concentration.
    fn evidence(&self, policy: PalwSettlementPolicyV1) -> Result<SettlementEvidenceV1, SettlementStopV1> {
        use SettlementStopV1::*;
        if self.duplicate {
            return Err(DuplicateWork);
        }
        if self.overflow {
            return Err(ArithmeticOverflow);
        }
        if self.anchors < policy.settled_anchor_depth {
            return Err(InsufficientDepth);
        }
        if self.work < policy.unique_mature_work {
            return Err(InsufficientWork);
        }
        for (max, limit) in [(self.max_operator, policy.max_operator_permille), (self.max_class, policy.max_class_permille)] {
            // Division before multiplication is deliberately avoided; overflow fails closed.
            let bound = self.work.checked_mul(limit as u128).ok_or(ArithmeticOverflow)?;
            if max.checked_mul(1000).is_none_or(|scaled| scaled > bound) {
                return Err(ConcentratedWork);
            }
        }
        Ok(SettlementEvidenceV1 { depth: self.anchors, work: self.work })
    }
}

/// One effect's certificate in executed-result order. The caller proves canonical membership,
/// root verification, frontier coverage and the absence of unresolved lifecycle obligations.
/// Heartbeat, floor and DNS weights are deliberately not accepted as evidence by this interface.
///
/// This is the per-effect REFERENCE: O(facts). The processor calls [`certify_native_prefix_v1`], which is tested
/// equal to calling this once per effect.
pub fn certify_native_effect_v1(
    policy: PalwSettlementPolicyV1,
    effect_order: (u64, u64),
    snapshot_daa: u64,
    executed: bool,
    frontier_covers: bool,
    lifecycle_closed: bool,
    history_complete: bool,
    facts: &[MatureUsefulWorkV1],
) -> Result<SettlementEvidenceV1, SettlementStopV1> {
    use SettlementStopV1::*;
    let (effect_daa, effect_blue) = effect_order;
    if !policy.valid() {
        return Err(InvalidPolicy);
    }
    if !history_complete {
        return Err(MissingHistory);
    }
    if !executed {
        return Err(Unexecuted);
    }
    if !frontier_covers {
        return Err(FrontierNotCovered);
    }
    if !lifecycle_closed {
        return Err(OpenLifecycle);
    }
    let mut acc = EvidenceAccumulatorV1::default();
    let mut anchors = BTreeSet::new();
    for f in facts
        .iter()
        .filter(|f| f.accepted_daa >= effect_daa && f.accepted_blue >= effect_blue && f.matured_daa <= snapshot_daa && f.work > 0)
    {
        acc.add_fact(f);
        if f.anchor_daa >= effect_daa && f.anchor_blue >= effect_blue {
            anchors.insert(f.anchor);
        }
    }
    acc.anchors = anchors.len() as u64;
    acc.evidence(policy)
}

/// One executed effect on the selected chain, for [`certify_native_prefix_v1`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeEffectV1 {
    pub daa: u64,
    pub blue: u64,
    pub frontier_covers: bool,
    pub lifecycle_closed: bool,
}

/// The certified prefix of a chain of executed effects, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePrefixV1 {
    /// Index (in `effects`) of the newest effect of the contiguous certified prefix.
    pub safe: Option<usize>,
    /// The certificate of that effect (`depth 0 / work 0` when there is none).
    pub evidence: SettlementEvidenceV1,
    /// Why the prefix ends, `None` when every effect certified.
    pub stop: Option<SettlementStopV1>,
}

/// **Certify the longest contiguous prefix of `effects` (oldest first) in O(N log N + F log F)**, not one
/// O(F) pass per effect: the facts that qualify for an effect only grow as the effect gets older, so one
/// sweep from the newest effect to the oldest adds each fact once. A prefix never jumps an uncertified
/// effect: the answer is the newest effect for which it and every older one certify, and the stop reason is
/// that of the first uncertified effect. Equal to calling [`certify_native_effect_v1`] per effect
/// (`rfc0012_prefix_sweep_equals_the_per_effect_reference`).
pub fn certify_native_prefix_v1(
    policy: PalwSettlementPolicyV1,
    effects: &[NativeEffectV1],
    snapshot_daa: u64,
    facts: &[MatureUsefulWorkV1],
) -> NativePrefixV1 {
    use SettlementStopV1::*;
    let n = effects.len();
    let mut results: Vec<Result<SettlementEvidenceV1, SettlementStopV1>> = Vec::with_capacity(n);
    let monotone = effects.windows(2).all(|w| w[0].daa <= w[1].daa && w[0].blue <= w[1].blue);
    if !monotone || !policy.valid() {
        // Not an ordered chain (or an unusable policy): the reference, one effect at a time.
        for e in effects {
            results.push(certify_native_effect_v1(policy, (e.daa, e.blue), snapshot_daa, true, e.frontier_covers, e.lifecycle_closed, true, facts));
        }
    } else {
        // Effect `i` is qualified by a position `(daa, blue)` iff `i <= below(daa, blue)`: the thresholds fall with `i`.
        let below = |daa: u64, blue: u64| -> Option<usize> {
            let by_daa = effects.partition_point(|e| e.daa <= daa);
            let by_blue = effects.partition_point(|e| e.blue <= blue);
            by_daa.min(by_blue).checked_sub(1)
        };
        let mut fact_at: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut anchor_first: BTreeMap<Hash64, usize> = BTreeMap::new();
        let live: Vec<&MatureUsefulWorkV1> = facts.iter().filter(|f| f.matured_daa <= snapshot_daa && f.work > 0).collect();
        for (k, f) in live.iter().enumerate() {
            let Some(q) = below(f.accepted_daa, f.accepted_blue) else {
                continue;
            };
            fact_at[q].push(k);
            if let Some(qa) = below(f.anchor_daa, f.anchor_blue).map(|a| a.min(q)) {
                let first = anchor_first.entry(f.anchor).or_insert(qa);
                *first = (*first).max(qa);
            }
        }
        let mut anchor_at: Vec<u64> = vec![0; n];
        for first in anchor_first.values() {
            anchor_at[*first] += 1;
        }
        let mut acc = EvidenceAccumulatorV1::default();
        results.resize(n, Err(InvalidPolicy));
        for i in (0..n).rev() {
            for k in &fact_at[i] {
                acc.add_fact(live[*k]);
            }
            for _ in 0..anchor_at[i] {
                acc.add_anchor();
            }
            let e = effects[i];
            results[i] = if !e.frontier_covers {
                Err(FrontierNotCovered)
            } else if !e.lifecycle_closed {
                Err(OpenLifecycle)
            } else {
                acc.evidence(policy)
            };
        }
    }
    let mut out = NativePrefixV1 { safe: None, evidence: SettlementEvidenceV1::default(), stop: None };
    for (i, r) in results.into_iter().enumerate() {
        match r {
            Ok(e) => {
                out.safe = Some(i);
                out.evidence = e;
            }
            Err(stop) => {
                out.stop = Some(stop);
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }
    fn policy() -> PalwSettlementPolicyV1 {
        PalwSettlementPolicyV1 { settled_anchor_depth: 2, unique_mature_work: 20, max_operator_permille: 600, max_class_permille: 600 }
    }
    fn facts() -> Vec<MatureUsefulWorkV1> {
        (1..=2)
            .map(|v| MatureUsefulWorkV1 {
                identity: h(v),
                anchor: h(v),
                operator: h(v),
                class: h(v),
                anchor_blue: 10 + v,
                accepted_blue: 10 + v,
                anchor_daa: 10 + v,
                accepted_daa: 10 + v,
                matured_daa: 20,
                work: 10,
            })
            .collect()
    }
    #[test]
    fn rfc0012_no_dns_is_required_and_unique_work_is_counted_once() {
        let f = facts();
        assert_eq!(
            certify_native_effect_v1(policy(), (10, 10), 20, true, true, true, true, &f),
            Ok(SettlementEvidenceV1 { depth: 2, work: 20 })
        );
        let duplicate = vec![f[0], f[0], f[1]];
        assert_eq!(
            certify_native_effect_v1(policy(), (10, 10), 20, true, true, true, true, &duplicate),
            Err(SettlementStopV1::DuplicateWork)
        );
    }
    #[test]
    fn rfc0012_missing_data_open_court_and_unexecuted_effect_fail_closed() {
        let f = facts();
        for (executed, frontier, closed, history, stop) in [
            (false, true, true, true, SettlementStopV1::Unexecuted),
            (true, false, true, true, SettlementStopV1::FrontierNotCovered),
            (true, true, false, true, SettlementStopV1::OpenLifecycle),
            (true, true, true, false, SettlementStopV1::MissingHistory),
        ] {
            assert_eq!(certify_native_effect_v1(policy(), (10, 10), 20, executed, frontier, closed, history, &f), Err(stop));
        }
        assert_eq!(
            certify_native_effect_v1(policy(), (10, 10), 19, true, true, true, true, &f),
            Err(SettlementStopV1::InsufficientDepth)
        );
        assert_eq!(
            certify_native_effect_v1(policy(), (13, 13), 20, true, true, true, true, &f),
            Err(SettlementStopV1::InsufficientDepth)
        );
    }
    #[test]
    fn rfc0012_concentration_and_shared_anchor_do_not_buy_depth() {
        let mut f = facts();
        f[1].operator = f[0].operator;
        assert_eq!(
            certify_native_effect_v1(policy(), (10, 10), 20, true, true, true, true, &f),
            Err(SettlementStopV1::ConcentratedWork)
        );
        f = facts();
        f[1].anchor = f[0].anchor;
        assert_eq!(
            certify_native_effect_v1(policy(), (10, 10), 20, true, true, true, true, &f),
            Err(SettlementStopV1::InsufficientDepth)
        );
    }
    #[test]
    fn rfc0012_retirement_keeps_exits_evidence_and_palw_roles() {
        for id in [
            SUBNETWORK_ID_STAKE_UNBOND,
            SUBNETWORK_ID_SLASHING_EVIDENCE,
            SUBNETWORK_ID_PRECOMMIT_EVIDENCE,
            SUBNETWORK_ID_PALW_COMMITMENT,
            SUBNETWORK_ID_PALW_RECEIPT,
            SUBNETWORK_ID_PALW_LIFECYCLE,
        ] {
            assert!(!creates_dns_participation(&id));
        }
        assert!(creates_dns_participation(&SUBNETWORK_ID_STAKE_BOND));
    }
    #[test]
    fn rfc0012_equal_daa_does_not_count_work_before_the_executed_effect() {
        let mut f = facts();
        for x in &mut f {
            x.accepted_daa = 50;
            x.anchor_daa = 50;
        }
        assert_eq!(
            certify_native_effect_v1(policy(), (50, 13), 60, true, true, true, true, &f),
            Err(SettlementStopV1::InsufficientDepth)
        );
        assert!(certify_native_effect_v1(policy(), (50, 10), 60, true, true, true, true, &f).is_ok());
    }

    #[test]
    fn rfc0012_reward_conservation_old_claims_recovery_and_integer_edges() {
        for subsidy in [0u64, 1, 12, 99, 1001, 50_000_000_000, u64::MAX] {
            let base = native_worker_base_v1(subsidy);
            let inclusion = subsidy - base;
            for earning_carve in [620u16, 720, 920] {
                let original_escrow = (subsidy as u128 * earning_carve as u128 / 1000) as u64;
                let unallocated_burn = base.checked_sub(original_escrow).expect("the original claim was actually funded");
                assert_eq!(original_escrow as u128 + unallocated_burn as u128 + inclusion as u128, subsidy as u128);
            }
        }
    }

    #[test]
    fn rfc0012_presets_are_unassigned_and_every_policy_value_is_committed() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS};
        for p in [MAINNET_PARAMS, TESTNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS] {
            assert!(p.palw_dns_retirement.is_none());
        }
        let mut base = MAINNET_PARAMS;
        let r = PalwDnsRetirementV1 { activation: ForkActivation::new(9000), settlement: policy(), legacy_evidence_horizon_daa: 300 };
        let absent = (base.consensus_params_id(), base.consensus_schedule_id());
        base.palw_dns_retirement = Some(r);
        let assigned = (base.consensus_params_id(), base.consensus_schedule_id());
        assert_ne!(absent, assigned);
        for changed in [
            PalwDnsRetirementV1 { activation: ForkActivation::new(9001), ..r },
            PalwDnsRetirementV1 { settlement: PalwSettlementPolicyV1 { settled_anchor_depth: 3, ..r.settlement }, ..r },
            PalwDnsRetirementV1 { settlement: PalwSettlementPolicyV1 { unique_mature_work: 21, ..r.settlement }, ..r },
            PalwDnsRetirementV1 { settlement: PalwSettlementPolicyV1 { max_operator_permille: 601, ..r.settlement }, ..r },
            PalwDnsRetirementV1 { settlement: PalwSettlementPolicyV1 { max_class_permille: 601, ..r.settlement }, ..r },
            PalwDnsRetirementV1 { legacy_evidence_horizon_daa: 301, ..r },
        ] {
            base.palw_dns_retirement = Some(changed);
            assert_ne!(assigned.0, base.consensus_params_id());
            assert_ne!(assigned.1, base.consensus_schedule_id());
        }
    }

    /// The sweep is the per-effect reference, effect for effect, over random chains, facts, duplicates, overflows,
    /// equal-DAA groups, anchors older than their work and flags: 20,000 instances, deterministic.
    #[test]
    fn rfc0012_prefix_sweep_equals_the_per_effect_reference() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move |m: u64| -> u64 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % m
        };
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        for case in 0..20_000u32 {
            let n = 1 + next(8) as usize;
            let (mut daa, mut blue) = (next(5), next(3));
            let effects: Vec<NativeEffectV1> = (0..n)
                .map(|_| {
                    daa += next(3); // equal-DAA groups
                    blue += 1 + next(2);
                    NativeEffectV1 { daa, blue, frontier_covers: next(8) != 0, lifecycle_closed: next(8) != 0 }
                })
                .collect();
            let policy = PalwSettlementPolicyV1 {
                settled_anchor_depth: 1 + next(2),
                unique_mature_work: 1 + next(12) as u128,
                max_operator_permille: [300, 500, 1000][next(3) as usize],
                max_class_permille: [400, 700, 1000][next(3) as usize],
            };
            let snapshot_daa = daa + next(4);
            let facts: Vec<MatureUsefulWorkV1> = (0..4 + next(20))
                .map(|_| {
                    let accepted_daa = next(daa + 3);
                    let accepted_blue = next(blue + 3);
                    MatureUsefulWorkV1 {
                        identity: h(1 + if next(7) == 0 { 0 } else { next(40) }),
                        anchor: h(100 + next(6)),
                        operator: h(200 + next(3)),
                        class: h(300 + next(2)),
                        anchor_blue: next(accepted_blue + 1),
                        accepted_blue,
                        anchor_daa: next(accepted_daa + 1),
                        accepted_daa,
                        matured_daa: next(snapshot_daa + 3),
                        work: match next(20) {
                            0 => 0,
                            1 => u128::MAX / 3,
                            _ => 1 + next(15) as u128,
                        },
                    }
                })
                .collect();
            let swept = certify_native_prefix_v1(policy, &effects, snapshot_daa, &facts);
            let mut expected = NativePrefixV1 { safe: None, evidence: SettlementEvidenceV1::default(), stop: None };
            for (i, e) in effects.iter().enumerate() {
                match certify_native_effect_v1(
                    policy,
                    (e.daa, e.blue),
                    snapshot_daa,
                    true,
                    e.frontier_covers,
                    e.lifecycle_closed,
                    true,
                    &facts,
                ) {
                    Ok(ev) => {
                        expected.safe = Some(i);
                        expected.evidence = ev;
                    }
                    Err(stop) => {
                        expected.stop = Some(stop);
                        break;
                    }
                }
            }
            assert_eq!(swept, expected, "case {case}: effects {effects:?} facts {facts:?} policy {policy:?} at {snapshot_daa}");
            *seen.entry(format!("{:?}/{}", swept.stop, swept.safe.is_some())).or_default() += 1;
        }
        // The instances are not all one shape: every stop reason and a certified prefix both occur.
        for stop in ["DuplicateWork", "ArithmeticOverflow", "InsufficientDepth", "InsufficientWork", "ConcentratedWork", "FrontierNotCovered", "OpenLifecycle"] {
            assert!(seen.keys().any(|k| k.contains(stop)), "no instance stopped at {stop}: {seen:?}");
        }
        let certifying: u32 = seen.iter().filter(|(k, _)| k.ends_with("true")).map(|(_, v)| *v).sum();
        assert!(certifying > 500 && seen.get("None/true").copied().unwrap_or(0) > 20, "certifying instances: {seen:?}");
    }

    #[test]
    fn rfc0012_certify_precedence_is_duplicate_then_overflow_then_depth_work_concentration() {
        let policy = PalwSettlementPolicyV1 { settled_anchor_depth: 1, unique_mature_work: 1, max_operator_permille: 1000, max_class_permille: 1000 };
        let fact = |id: u64, work: u128| MatureUsefulWorkV1 {
            identity: h(id),
            anchor: h(id),
            operator: h(id),
            class: h(id),
            anchor_blue: 10,
            accepted_blue: 10,
            anchor_daa: 10,
            accepted_daa: 10,
            matured_daa: 0,
            work,
        };
        let big = u128::MAX / 2 + 1;
        let order = |facts: &[MatureUsefulWorkV1]| certify_native_effect_v1(policy, (10, 10), 20, true, true, true, true, facts);
        // Both orders of the same three facts report the duplicate before the overflow.
        let (a, b, dup) = (fact(1, big), fact(2, big), fact(1, 1));
        assert_eq!(order(&[a, b, dup]), Err(SettlementStopV1::DuplicateWork));
        assert_eq!(order(&[dup, a, b]), Err(SettlementStopV1::DuplicateWork));
        assert_eq!(order(&[a, b]), Err(SettlementStopV1::ArithmeticOverflow));
    }

    #[test]
    fn rfc0012_historical_evidence_has_a_finite_horizon_and_no_post_retirement_target() {
        use crate::{
            dns_finality::{PrecommitEvidencePayload, StakePrecommitPayload},
            tx::{Transaction, TransactionOutpoint},
        };
        let vote = |target| StakePrecommitPayload {
            version: 1,
            validator_id: h(1),
            bond_outpoint: TransactionOutpoint::new(h(2), 0),
            epoch: 1,
            target_hash: h(3),
            target_daa_score: target,
            locked_epoch: 0,
            locked_hash: h(0),
            snapshot_commitment: h(4),
            signature: vec![],
        };
        let tx = |target| {
            Transaction::new(
                0,
                vec![],
                vec![],
                0,
                SUBNETWORK_ID_PRECOMMIT_EVIDENCE,
                0,
                borsh::to_vec(&PrecommitEvidencePayload {
                    version: 1,
                    bond_outpoint: TransactionOutpoint::new(h(2), 0),
                    precommit_a: vote(99),
                    precommit_b: vote(target),
                    reporter_reward_spk_payload: [0; 64],
                })
                .unwrap(),
            )
        };
        let r = PalwDnsRetirementV1 { activation: ForkActivation::new(100), settlement: policy(), legacy_evidence_horizon_daa: 10 };
        assert!(legacy_dns_evidence_allowed_v1(r, &tx(99), 110));
        assert!(!legacy_dns_evidence_allowed_v1(r, &tx(99), 111));
        assert!(!legacy_dns_evidence_allowed_v1(r, &tx(100), 100));
        assert!(legacy_dns_evidence_allowed_v1(r, &tx(100), 99), "history below the fence uses the old validator");
    }
}
