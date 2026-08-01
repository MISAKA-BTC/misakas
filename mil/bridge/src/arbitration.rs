//! **Seam 4 — mismatch arbitration.**
//!
//! When two replicas disagree, the tree has no on-chain rule: there is no k=3, no majority, no
//! tie-break. What exists is a fully-specified but INERT decision layer —
//! `PalwMismatchRecordV1::{escalation_draw, is_escalated, attribute, slash_targets}` with
//! `PalwMismatchParams::INERT` (rate 0) and zero production callers — plus ADR-0040's SLASH-01
//! row marking mismatch attribution 未着手 pending cross-device measurement, and ADR-0045 D2
//! assigning "auditor scheduling / model re-run / opening delivery" explicitly OUT of the
//! consensus crate. In other words: the missing half is the coordinator's job, and its shape is
//! already fixed.
//!
//! This module implements exactly that shape, with the node's own functions:
//!
//! 1. **Escalate?** [`PalwMismatchRecordV1::is_escalated`] — the real beacon-seeded ppm draw,
//!    plus the repeat-offender rule. A mismatch that is not drawn is recorded and left as an
//!    unresolved dispute; nothing is slashed on a coin flip nobody audited.
//! 2. **Who audits?** [`select_weighted_auditor_committee`] — the real stake-weighted,
//!    non-replacement sampler over the bonded set, seeded by the beacon and the dispute id,
//!    with BOTH disputants' credentials and operator groups excluded (the same exclusion rule
//!    `derive_palw_audit_selection` applies for batch audits, so a party can never audit its own
//!    dispute, nor can a sibling in its operator group).
//! 3. **Reference re-run.** The selected auditor replays the job and reports its match key. This
//!    is the "off-protocol input" ADR-0040 names — consensus only ever checks the verdict.
//! 4. **Attribute.** [`PalwMismatchRecordV1::attribute`] against the reference output →
//!    [`PalwMismatchRecordV1::slash_targets`]. Whoever deviates from the reference is the
//!    slash target; if neither matches, both are (SlashBoth). The honest partner is never hit,
//!    because `a != b` is what got us here.
//!
//! **What this does not do.** It does not submit a slashing transaction: `PalwMismatchVerdict`
//! has no on-chain carrier today (0x39 is reserved and undecoded; the only wired slash paths are
//! DA timeout 0x3c and search timeout 0x3f), and `econ-parameters-frozen.md` E8 records that a
//! slash is all-or-nothing on the bond's output-0 with no amount parameter. So arbitration ends
//! at signed, journaled EVIDENCE with the exact fields a future carrier needs. Claiming more
//! than that would be claiming a chain effect that cannot happen.

use std::collections::HashSet;

use kaspa_consensus_core::palw::{
    PalwMismatchParams, PalwMismatchRecordV1, PalwMismatchVerdict, ProviderBondView, select_weighted_auditor_committee,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::{Hash64, blake2b_512_keyed};
use serde::{Deserialize, Serialize};

use crate::chain::{format_outpoint, parse_hash64};
use crate::match_key::hash64_hex;
use crate::provider::RegisteredProvider;

/// Escalation policy for this bridge. Deliberately NOT `INERT` — an off-chain coordinator that
/// escalates nothing is the state we already had. 100 % (1e6 ppm) is the honest setting while
/// chat volumes are small and an audit costs one replay: every dispute gets adjudicated. The
/// rate exists so it can be turned down when volume makes that expensive, using the same draw
/// consensus would use.
pub const BRIDGE_MISMATCH_PARAMS: PalwMismatchParams =
    PalwMismatchParams { escalation_rate_ppm: 1_000_000, repeat_offender_threshold: 1 };

/// Dispute id — stands in for `(batch_id, leaf_index)` off-chain, and is what seeds the auditor
/// draw. Bound to the job and to both disputants so it cannot be steered.
pub fn dispute_id(job_id: &str, provider_a: &TransactionOutpoint, provider_b: &TransactionOutpoint) -> Hash64 {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(&(job_id.len() as u64).to_le_bytes());
    preimage.extend_from_slice(job_id.as_bytes());
    for outpoint in [provider_a, provider_b] {
        preimage.extend_from_slice(outpoint.transaction_id.as_byte_slice());
        preimage.extend_from_slice(&outpoint.index.to_le_bytes());
    }
    blake2b_512_keyed(b"misaka-palw-bridge-v1/dispute-id", &preimage)
}

/// A dispute awaiting (or holding) an adjudication.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisputeRecord {
    pub dispute_id_hex: String,
    pub job_id: String,
    pub provider_a: String,
    pub provider_b: String,
    /// The two disagreeing `output_commitment`s (the k=2 key field that differs).
    pub output_a_hex: String,
    pub output_b_hex: String,
    pub beacon_epoch: u64,
    pub escalated: bool,
    pub auditor: Option<String>,
    pub verdict: Option<String>,
    pub slash_targets: Vec<String>,
}

/// The node's record type, built from a bridge dispute.
pub fn to_consensus_record(dispute: &DisputeRecord) -> Result<PalwMismatchRecordV1, String> {
    Ok(PalwMismatchRecordV1 {
        batch_id: parse_hash64(&dispute.dispute_id_hex)?,
        // Off-chain there is one job per dispute; the leaf index is the fixed 0 slot.
        leaf_index: 0,
        provider_a: crate::chain::parse_outpoint(&dispute.provider_a)?,
        provider_b: crate::chain::parse_outpoint(&dispute.provider_b)?,
        output_a: parse_hash64(&dispute.output_a_hex)?,
        output_b: parse_hash64(&dispute.output_b_hex)?,
    })
}

/// Step 1 — the real escalation draw.
pub fn is_escalated(
    dispute: &DisputeRecord,
    beacon_seed: &Hash64,
    prior_mismatches_a: u32,
    prior_mismatches_b: u32,
) -> Result<bool, String> {
    let record = to_consensus_record(dispute)?;
    Ok(record.is_escalated(beacon_seed, &BRIDGE_MISMATCH_PARAMS, prior_mismatches_a, prior_mismatches_b))
}

/// Step 2 — the real stake-weighted auditor draw over the bonded set, with both disputants (and
/// their operator groups) excluded. Returns the chosen auditor's bond outpoint, or None when no
/// eligible third party exists — in which case the dispute stays unresolved rather than being
/// adjudicated by an interested party.
pub fn select_auditor(
    dispute: &DisputeRecord,
    beacon_seed: &Hash64,
    pov_daa_score: u64,
    candidates: &[&RegisteredProvider],
) -> Result<Option<String>, String> {
    let mut records = Vec::with_capacity(candidates.len());
    let mut excluded_credentials = HashSet::new();
    let mut excluded_groups = HashSet::new();
    for provider in candidates {
        let record = provider.record()?;
        if provider.bond_outpoint == dispute.provider_a || provider.bond_outpoint == dispute.provider_b {
            excluded_credentials.insert(record.owner_pubkey_hash);
            excluded_groups.insert(record.operator_group_id);
        }
        records.push((record.bond_outpoint, record));
    }
    let view = ProviderBondView::from_records(records);
    let (slate, _commitment) = select_weighted_auditor_committee(
        beacon_seed,
        &parse_hash64(&dispute.dispute_id_hex)?,
        &view,
        pov_daa_score,
        &excluded_credentials,
        &excluded_groups,
        1,
    );
    Ok(slate.first().map(|stake| format_outpoint(&stake.representative)))
}

/// **BRIDGE-SEL-01 — who replicates a job.** [`select_auditor`] applied to the k=2 replica draw.
///
/// # What was wrong
///
/// Assignment was CLAIM-ON-FETCH: `GET /palw/v1/assignments` handed every unassigned job to the
/// first provider that polled and was not the submitter. Whoever polled fastest took the work, so
/// a provider running a tight loop could hold an arbitrary share of all replication — and, paired
/// with an unauthenticated route (BRIDGE-AUTH-01), could hold it under someone else's name. The
/// submitter had only to out-poll the field with a second identity to replicate its own job.
///
/// # What replaces it
///
/// The replica is DERIVED, not granted: the same stake-weighted, non-replacement sampler the
/// dispute path already uses, seeded by a beacon the submitter could not know when it committed,
/// over a bond view frozen at that beacon, with the submitter's own credential and operator group
/// excluded. `round` re-rolls a lapsed assignment, so a silent selectee cannot strand a job.
/// The bridge distributes this result; it does not choose it.
///
/// # What this is NOT
///
/// Consensus does not verify it. A dishonest bridge can still hand a job to whoever it likes,
/// because nothing on chain binds an assignment to a beacon — `PalwPublicLeafV1` carries no
/// `A_commit`, no assignment proof, no reroll round (PCPB-01, still unimplemented). This makes
/// the HONEST bridge's choice unpredictable and reproducible; it does not make a dishonest one
/// detectable. That gap closes only when the leaf carries the assignment proof.
pub fn select_replica(
    job_id: &str,
    submitter_bond: &str,
    beacon_seed: &Hash64,
    pov_daa_score: u64,
    round: u32,
    candidates: &[&RegisteredProvider],
) -> Result<Option<String>, String> {
    let mut records = Vec::with_capacity(candidates.len());
    let mut excluded_credentials = HashSet::new();
    let mut excluded_groups = HashSet::new();
    for provider in candidates {
        let record = provider.record()?;
        // The independence rule, unchanged: a job is never replicated by its own submitter — now
        // extended to the submitter's operator-group siblings, exactly as disputes exclude them.
        if provider.bond_outpoint == submitter_bond {
            excluded_credentials.insert(record.owner_pubkey_hash);
            excluded_groups.insert(record.operator_group_id);
        }
        records.push((record.bond_outpoint, record));
    }
    let view = ProviderBondView::from_records(records);
    // Bind the draw to the job AND the reroll round, so a lapse produces a different selectee
    // rather than the same silent one forever.
    let draw_id = blake2b_512_keyed(BRIDGE_REPLICA_DRAW_DOMAIN, &{
        let mut preimage = Vec::with_capacity(job_id.len() + submitter_bond.len() + 4);
        preimage.extend_from_slice(job_id.as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(submitter_bond.as_bytes());
        preimage.extend_from_slice(&round.to_le_bytes());
        preimage
    });
    let (slate, _commitment) =
        select_weighted_auditor_committee(beacon_seed, &draw_id, &view, pov_daa_score, &excluded_credentials, &excluded_groups, 1);
    Ok(slate.first().map(|stake| format_outpoint(&stake.representative)))
}

/// Keyed domain separating the replica draw from the dispute-auditor draw, so one job's replica
/// selection can never alias the auditor selection for a dispute over that same job.
pub const BRIDGE_REPLICA_DRAW_DOMAIN: &[u8] = b"misaka-palw-bridge-v1/replica-draw";

/// Step 4 — attribute against the auditor's reference output and name the slash targets.
pub fn adjudicate(dispute: &DisputeRecord, reference_output: &Hash64) -> Result<(PalwMismatchVerdict, Vec<String>), String> {
    let record = to_consensus_record(dispute)?;
    let verdict = record.attribute(reference_output);
    let targets = record.slash_targets(verdict).iter().map(format_outpoint).collect();
    Ok((verdict, targets))
}

pub fn verdict_str(verdict: PalwMismatchVerdict) -> &'static str {
    match verdict {
        PalwMismatchVerdict::SlashA => "slash_a",
        PalwMismatchVerdict::SlashB => "slash_b",
        PalwMismatchVerdict::SlashBoth => "slash_both",
        PalwMismatchVerdict::NotAMismatch => "not_a_mismatch",
    }
}

/// The artifact arbitration produces: everything a future on-chain carrier (or a node operator
/// acting manually) needs, bound to the beacon epoch that seeded the draw.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlashEvidenceV1 {
    pub dispute_id_hex: String,
    pub job_id: String,
    pub provider_a: String,
    pub provider_b: String,
    pub output_a_hex: String,
    pub output_b_hex: String,
    pub auditor: String,
    pub reference_output_hex: String,
    pub verdict: String,
    pub slash_targets: Vec<String>,
    pub beacon_epoch: u64,
    pub beacon_seed_hex: String,
    /// The journal root at the moment the evidence was produced — ties it to an immutable
    /// position in the bridge's history.
    pub journal_root_hex: String,
}

impl SlashEvidenceV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dispute: &DisputeRecord,
        auditor: &str,
        reference_output: &Hash64,
        verdict: PalwMismatchVerdict,
        slash_targets: Vec<String>,
        beacon_seed_hex: &str,
        journal_root_hex: &str,
    ) -> Self {
        Self {
            dispute_id_hex: dispute.dispute_id_hex.clone(),
            job_id: dispute.job_id.clone(),
            provider_a: dispute.provider_a.clone(),
            provider_b: dispute.provider_b.clone(),
            output_a_hex: dispute.output_a_hex.clone(),
            output_b_hex: dispute.output_b_hex.clone(),
            auditor: auditor.to_string(),
            reference_output_hex: hash64_hex(reference_output),
            verdict: verdict_str(verdict).to_string(),
            slash_targets,
            beacon_epoch: dispute.beacon_epoch,
            beacon_seed_hex: beacon_seed_hex.to_string(),
            journal_root_hex: journal_root_hex.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::{BondFacts, parse_outpoint};

    fn outpoint_text(byte: u8, index: u32) -> String {
        format!("{}:{index}", format!("{byte:02x}").repeat(64))
    }

    fn dispute() -> DisputeRecord {
        let a = outpoint_text(0x11, 0);
        let b = outpoint_text(0x22, 0);
        DisputeRecord {
            dispute_id_hex: hash64_hex(&dispute_id("job-1", &parse_outpoint(&a).unwrap(), &parse_outpoint(&b).unwrap())),
            job_id: "job-1".into(),
            provider_a: a,
            provider_b: b,
            output_a_hex: "aa".repeat(64),
            output_b_hex: "bb".repeat(64),
            beacon_epoch: 12,
            escalated: false,
            auditor: None,
            verdict: None,
            slash_targets: Vec::new(),
        }
    }

    fn provider(byte: u8, group: u8, amount: u64) -> RegisteredProvider {
        let outpoint = outpoint_text(byte, 0);
        RegisteredProvider {
            bond_outpoint: outpoint.clone(),
            owner_public_key_hex: format!("{byte:02x}").repeat(2592),
            credential_hex: format!("{byte:02x}").repeat(64),
            session_public_key_hex: format!("{byte:02x}").repeat(2592),
            session_valid_from_epoch: 0,
            session_valid_until_epoch: 1_000,
            bond: BondFacts {
                bond_outpoint: outpoint,
                owner_pubkey_hash_hex: format!("{byte:02x}").repeat(64),
                operator_group_id_hex: format!("{group:02x}").repeat(64),
                amount_sompi: amount,
                activation_daa_score: 0,
                effective_status: "active".into(),
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                unbond_delay_epochs: 6,
                reward_key_root_hex: "00".repeat(64),
                runtime_classes_hex: vec![],
                capacity_by_shape: vec![],
            },
        }
    }

    fn seed(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    #[test]
    fn every_dispute_escalates_under_the_bridge_policy() {
        // rate = 1e6 ppm ⇒ the draw (always < 1e6) always passes. The point of the test is that
        // we go through the REAL predicate, not that we skip it.
        assert!(is_escalated(&dispute(), &seed(1), 0, 0).unwrap());
        // Equal outputs are not a mismatch at all, whatever the rate.
        let mut equal = dispute();
        equal.output_b_hex = equal.output_a_hex.clone();
        assert!(!is_escalated(&equal, &seed(1), 0, 0).unwrap());
    }

    #[test]
    fn attribution_follows_the_reference_run() {
        let d = dispute();
        let a_output = parse_hash64(&d.output_a_hex).unwrap();
        let b_output = parse_hash64(&d.output_b_hex).unwrap();

        // Reference agrees with A ⇒ B is slashed, and only B.
        let (verdict, targets) = adjudicate(&d, &a_output).unwrap();
        assert_eq!(verdict, PalwMismatchVerdict::SlashB);
        assert_eq!(targets, vec![d.provider_b.clone()]);

        // Reference agrees with B ⇒ A.
        let (verdict, targets) = adjudicate(&d, &b_output).unwrap();
        assert_eq!(verdict, PalwMismatchVerdict::SlashA);
        assert_eq!(targets, vec![d.provider_a.clone()]);

        // Reference agrees with neither ⇒ both.
        let (verdict, targets) = adjudicate(&d, &seed(0x99)).unwrap();
        assert_eq!(verdict, PalwMismatchVerdict::SlashBoth);
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&d.provider_a) && targets.contains(&d.provider_b));
    }

    #[test]
    fn auditor_is_never_a_disputant_or_its_operator_sibling() {
        let d = dispute();
        // a and b are the disputants (0x11, 0x22, both in operator group 1);
        // 0x33 shares group 1 with them — a sibling, also excluded; 0x44 is independent.
        let a = provider(0x11, 1, 1_000_000_000);
        let b = provider(0x22, 1, 1_000_000_000);
        let sibling = provider(0x33, 1, 9_000_000_000);
        let independent = provider(0x44, 2, 1_000_000_000);
        let candidates = vec![&a, &b, &sibling, &independent];

        let chosen = select_auditor(&d, &seed(7), 1_000, &candidates).unwrap().expect("an eligible auditor exists");
        assert_eq!(chosen, independent.bond_outpoint, "the only unconflicted candidate must be drawn");
        // …even though the sibling has 9x the stake — exclusion beats weight.
    }

    #[test]
    fn no_eligible_auditor_leaves_the_dispute_unresolved() {
        let d = dispute();
        let a = provider(0x11, 1, 1_000_000_000);
        let b = provider(0x22, 1, 1_000_000_000);
        // Only the two disputants are registered.
        assert!(select_auditor(&d, &seed(7), 1_000, &[&a, &b]).unwrap().is_none());
    }

    #[test]
    fn auditor_draw_is_deterministic_and_seed_bound() {
        let d = dispute();
        let a = provider(0x11, 1, 1_000_000_000);
        let b = provider(0x22, 1, 1_000_000_000);
        let c = provider(0x44, 2, 1_000_000_000);
        let e = provider(0x55, 3, 1_000_000_000);
        let candidates = vec![&a, &b, &c, &e];

        let first = select_auditor(&d, &seed(7), 1_000, &candidates).unwrap();
        assert_eq!(first, select_auditor(&d, &seed(7), 1_000, &candidates).unwrap(), "deterministic");
        // A different beacon seed can move the draw; both outcomes must be eligible candidates.
        let other = select_auditor(&d, &seed(200), 1_000, &candidates).unwrap().unwrap();
        assert!(other == c.bond_outpoint || other == e.bond_outpoint);
    }

    #[test]
    fn inactive_candidates_carry_no_weight() {
        let d = dispute();
        let a = provider(0x11, 1, 1_000_000_000);
        let b = provider(0x22, 1, 1_000_000_000);
        let mut slashed = provider(0x44, 2, 9_000_000_000);
        slashed.bond.effective_status = "slashed".into();
        slashed.bond.slashed_at_daa_score = Some(10);
        let healthy = provider(0x55, 3, 1_000_000_000);
        let chosen = select_auditor(&d, &seed(7), 1_000, &[&a, &b, &slashed, &healthy]).unwrap().unwrap();
        assert_eq!(chosen, healthy.bond_outpoint, "a slashed bond must never be drawn as auditor");
    }

    #[test]
    fn evidence_carries_what_a_carrier_would_need() {
        let d = dispute();
        let reference = parse_hash64(&d.output_a_hex).unwrap();
        let (verdict, targets) = adjudicate(&d, &reference).unwrap();
        let evidence = SlashEvidenceV1::new(&d, "auditor:0", &reference, verdict, targets, &"07".repeat(64), &"ab".repeat(64));
        assert_eq!(evidence.verdict, "slash_b");
        assert_eq!(evidence.slash_targets, vec![d.provider_b]);
        assert_eq!(evidence.beacon_epoch, 12);
        assert!(!evidence.journal_root_hex.is_empty());
        // Round-trips as JSON (it is written to the journal and served over HTTP).
        let json = serde_json::to_string(&evidence).unwrap();
        assert_eq!(serde_json::from_str::<SlashEvidenceV1>(&json).unwrap(), evidence);
    }
}
