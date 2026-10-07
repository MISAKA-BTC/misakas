//! RFC-0012: deterministic, DNS-free native settlement. No production policy is assigned here.
//! Inputs must come from one canonical, executed PALW snapshot; missing evidence never buys safety.

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

/// Extract newly consumed certified FP slices from a root-verified canonical delta. A grant is
/// not work credit by itself; each slice's canonical execution identity is counted once.
pub fn mature_fp_slices_in_delta_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    delta: &crate::palw_state_v2::PalwStateDeltaV2,
    accepting_daa: u64,
    accepting_blue: u64,
    snapshot_daa: u64,
    maturity_daa: u64,
    pricing: &crate::palw_state_v2::PalwFpPricingV1,
) -> Vec<MatureUsefulWorkV1> {
    use crate::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2, PalwDeltaEntryV2, palw_fp_spend_weight_v1};
    let mut facts = Vec::new();
    for entry in &delta.entries {
        let PalwDeltaEntryV2::Claim { key, old: Some(old), new: Some(new) } = entry else {
            continue;
        };
        let (PalwClaimSourceV2::FreePrompt { spent: before, .. }, PalwClaimSourceV2::FreePrompt { spent: after, .. }) =
            (&old.source, &new.source)
        else {
            continue;
        };
        let Some(claim) = state.claim(key) else {
            continue;
        };
        let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
            continue;
        };
        let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else {
            continue;
        };
        if claim.class_id == pricing.base_class_id
            || !pricing.prices_in_compute(claim)
            || claim.trace_retention_daa > snapshot_daa
            || state.da_sessions_iter().any(|((id, _), _)| id == key)
        {
            continue;
        }
        let Some(work_id) = claim.work_id else {
            continue;
        };
        let Some(bond) = state.bond(&claim.bond) else {
            continue;
        };
        let matured_daa = final_daa.max(accepting_daa.saturating_add(maturity_daa));
        if matured_daa > snapshot_daa {
            continue;
        }
        for index in after.difference(before).filter(|index| spent.contains(index)) {
            let mut h = blake2b_simd::Params::new().hash_length(64).to_state();
            h.update(b"MISAKA/native-settlement-slice/v1");
            h.update(&work_id.as_bytes());
            h.update(&index.to_le_bytes());
            facts.push(MatureUsefulWorkV1 {
                identity: Hash64::from_bytes(*h.finalize().as_array()),
                anchor: claim.accepted_block,
                operator: bond.operator_id,
                class: claim.class_id,
                anchor_blue: claim.accepted_blue_score,
                accepted_blue: accepting_blue,
                anchor_daa: claim.accepted_daa,
                accepted_daa: accepting_daa,
                matured_daa,
                work: palw_fp_spend_weight_v1(state, claim, *quanta, pricing),
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

/// One effect's certificate in executed-result order. The caller proves canonical membership,
/// root verification, frontier coverage and the absence of unresolved lifecycle obligations.
/// Heartbeat, floor and DNS weights are deliberately not accepted as evidence by this interface.
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
    let mut ids = BTreeSet::new();
    let mut anchors = BTreeSet::new();
    let mut operators = BTreeMap::<Hash64, u128>::new();
    let mut classes = BTreeMap::<Hash64, u128>::new();
    let mut work = 0u128;
    for f in facts
        .iter()
        .filter(|f| f.accepted_daa >= effect_daa && f.accepted_blue >= effect_blue && f.matured_daa <= snapshot_daa && f.work > 0)
    {
        if !ids.insert(f.identity) {
            return Err(DuplicateWork);
        }
        if f.anchor_daa >= effect_daa && f.anchor_blue >= effect_blue {
            anchors.insert(f.anchor);
        }
        work = work.checked_add(f.work).ok_or(ArithmeticOverflow)?;
        for (map, key) in [(&mut operators, f.operator), (&mut classes, f.class)] {
            let entry = map.entry(key).or_default();
            *entry = entry.checked_add(f.work).ok_or(ArithmeticOverflow)?;
        }
    }
    let depth = anchors.len() as u64;
    if depth < policy.settled_anchor_depth {
        return Err(InsufficientDepth);
    }
    if work < policy.unique_mature_work {
        return Err(InsufficientWork);
    }
    for (map, limit) in [(&operators, policy.max_operator_permille), (&classes, policy.max_class_permille)] {
        // Division before multiplication is deliberately avoided; overflow fails closed.
        let bound = work.checked_mul(limit as u128).ok_or(ArithmeticOverflow)?;
        if map.values().any(|v| v.checked_mul(1000).is_none_or(|scaled| scaled > bound)) {
            return Err(ConcentratedWork);
        }
    }
    Ok(SettlementEvidenceV1 { depth, work })
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
