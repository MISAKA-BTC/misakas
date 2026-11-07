#![allow(dead_code, unused_imports)]
//! **The review's F2 on testnet-12's own ruleset: a decoy at the lie's own leaf holds off no seat.**
//!
//! The reviewer's probe (A-held review, F2), adopted: a decoy held dissection opened by the
//! producer's Sybil (a non-seat) at the LIE's own leaf used to lock every seat of the claim's panel
//! out — C3's further-session rule refused a seat's dissection "at a leaf a session already
//! disputes". C3 now admits a seat at any leaf (one per seat, at most `1 + seat_count` on the claim;
//! the session id binds the challenger, so two sessions at one leaf do not collide). Harness copied
//! from `t12_aheld_held_court.rs` (testnet-12's own ruleset; the opening block folds with the
//! 2026-09-23 fence off so the unbound verdict defers the grief object, as that file does).

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1, PalwIdentityRulesV1, PalwPanelFalseValidEvidenceV2,
    palw_check_panel_false_valid_v2,
};
use kaspa_consensus_core::palw_offence_v1::{PalwOffenceVerifyError, PalwPanelContradictionV1};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_shard_court_v1::{PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateCarriageV2, PalwStateParamsV2,
    PalwStateV2Error, PalwTransitionExtrasV1, PalwVoidReasonV2, apply_palw_transition_v7, palw_accuser_exposure_v1,
};
use kaspa_consensus_core::palw_step::{
    PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index, step_leaf_count_capped_v1,
};
use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;

const EXECUTOR: u64 = 0x00A1_E0E0;
const BYSTANDER: u64 = 0x00A1_B5B5;

/// How a block is folded: testnet-12's own fences, the opening block's (2026-09-23 off), or the
/// fence-off twin (`palw_offence_attribution` off, everything else testnet-12's).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    T12,
    Below,
    /// The block that opens a session under `T12` / `Below`: the same, with the 2026-09-23 fence off.
    Opening,
    OpeningBelow,
}

fn court_extras(p: &Params, daa: u64, rule: Rule) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    let ladder =
        kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(&bundle(p).court, p.palw_court_ladder_active_at(daa));
    e.shard_court_ladder = p.palw_shard_court_active_at(daa).then_some(ladder);
    e.held_context_ladder = p.palw_held_context_active_at(daa).then_some(ladder);
    e.offence_attribution_active = p.palw_offence_attribution_active_at(daa);
    if matches!(rule, Rule::Below | Rule::OpeningBelow) {
        e.offence_attribution_active = false;
    }
    if matches!(rule, Rule::Opening | Rule::OpeningBelow) {
        e.audit_2026_09_23_active = false;
        e.settled_anchor_depth = None;
    }
    e
}

struct Chain {
    p: Params,
    sp: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
    /// The rule every block but an opening folds under.
    rule: Rule,
}

impl Chain {
    fn fold(
        &self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
        rule: Rule,
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let f = flags(&self.p, daa);
        apply_palw_transition_v7(
            &self.s,
            &self.sp,
            None,
            &ctx(0x0A1E_0000 + daa, daa, daa, if matches!(work, PalwBlockWorkV3::Attempt(_)) { T12_BLOCK_SUBSIDY_SOMPI } else { 0 }),
            objects,
            work,
            &[],
            key,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &court_extras(&self.p, daa, rule),
        )
        .map(|(s, _, skips)| {
            assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
            s
        })
    }

    fn step(
        &mut self,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
        rule: Rule,
    ) -> Result<(), PalwStateV2Error> {
        let daa = self.daa + 1;
        let next = self.fold(daa, objects, work, key, rule)?;
        let reloaded = PalwStateCarriageV2::from_state(&next).into_state(&self.sp, Some(next.state_root())).expect("reloads");
        assert_eq!(reloaded, next, "DAA {daa}: the carriage reloads to the state");
        self.s = next;
        self.daa = daa;
        Ok(())
    }

    fn empty(&mut self) {
        let rule = self.rule;
        self.step(&[], PalwBlockWorkV3::None, Hash64::default(), rule).unwrap_or_else(|e| panic!("DAA {}: {e:?}", self.daa + 1));
    }
}

/// testnet-12's genesis 8k held row: its id, published profile and canonical job, target, leaves.
struct HeldRow {
    id: Hash64,
    profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    target: u128,
    leaves: u64,
}

fn held_row(p: &Params, n_ctx: fn(u32) -> bool) -> Option<HeldRow> {
    let rows = genesis_classes(p);
    bundle(p).genesis_objects.iter().find_map(|o| match o {
        PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
            if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) && n_ctx(c.profile.n_ctx) =>
        {
            let (_, leaves, target, _) = *rows.iter().find(|r| r.0 == *class_id).expect("a genesis row");
            Some(HeldRow { id: *class_id, profile: c.profile.clone(), job: c.canonical.clone(), target, leaves })
        }
        _ => None,
    })
}

fn eight_k(p: &Params) -> HeldRow {
    held_row(p, |n| n == 8_192).expect("testnet-12 registers the 8k held row")
}

/// testnet-12's genesis 2M held row — the one class its bundle mirrors as unanswerable.
fn two_m(p: &Params) -> HeldRow {
    held_row(p, |n| n > kaspa_consensus_core::palw_state_v2::PALW_HELD_ANSWERABLE_N_CTX_V1)
        .expect("testnet-12 registers the 2M held row")
}

/// testnet-12 at DAA 1,000: the 8k row made `Active` through the carriage (its registry row opens
/// `Prefetching` until seven seats prove the artifact — the one step here that is not the fold), the
/// executor and the bystander bonded — the bystander at exactly `bystander_collateral`.
fn chain(rule: Rule, bystander_collateral: u64) -> (Chain, HeldRow, Vec<(PalwBondKeyV2, Hash64)>) {
    let (c, row, seats) = chain_on(rule, bystander_collateral, eight_k);
    assert!(!c.sp.held_class_is_unanswerable_v1(&row.id), "the 8k row is answerable");
    (c, row, seats)
}

/// [`chain`] on the held row `which` picks.
fn chain_on(rule: Rule, bystander_collateral: u64, which: fn(&Params) -> HeldRow) -> (Chain, HeldRow, Vec<(PalwBondKeyV2, Hash64)>) {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    assert!(!sp.held_unanswerable_classes().is_empty(), "the bundle carries the answerability mirror");
    let row = which(&p);
    let seats: Vec<(PalwBondKeyV2, Hash64)> =
        genesis_bonds(&p)[..b.panel.seat_count() as usize].iter().map(|(k, o, _)| (*k, *o)).collect();
    let mut s = genesis_state(&p);
    let mut c = PalwStateCarriageV2::from_state(&s);
    c.model_lifecycles.get_mut(&row.id).expect("the row").state =
        kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active;
    s = c.into_state_v3(&sp, None, flags(&p, 0).uncertified_weightless, p.palw_canonical_work_daa()).expect("a consistent carriage");
    let mut chain = Chain { p, sp, s, daa: 999, rule };
    chain
        .step(
            &[bond_obj(EXECUTOR, 1_000_000_000_000_000), bond_obj(BYSTANDER, bystander_collateral)],
            PalwBlockWorkV3::None,
            Hash64::default(),
            rule,
        )
        .expect("the executor and the bystander bond");
    (chain, row, seats)
}

/// A licensed junk attempt of the executor's on the 8k row: attempt, panel, all-`Valid` licence.
fn licensed(c: &mut Chain, row: &HeldRow, seats: &[(PalwBondKeyV2, Hash64)], nonce: u64) -> Hash64 {
    let rule = c.rule;
    let pwu = match c.s.palw_canonical_per_draw_v1(&row.id, c.daa + 1, c.p.palw_canonical_work_daa()) {
        Some(work) => kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(row.target, work),
        None => palw_pwu_v1(row.target, row.leaves),
    };
    let (env, key, claim_id) =
        junk_attempt(row.id, bond_key(EXECUTOR), pubkey_of(EXECUTOR), &operator_pubkey_of(EXECUTOR), pwu, nonce, 0x0A1E_0000 + nonce);
    c.step(&[], PalwBlockWorkV3::Attempt(&env), key, rule).expect("the attempt folds");
    assert!(c.s.claim(&claim_id).is_some(), "the claim is recorded");
    c.step(
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x0A1E_0A00 + nonce), seats: seats_of(seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        rule,
    )
    .expect("the panel binds");
    let keys: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    c.step(
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &keys) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        rule,
    )
    .expect("the panel licenses");
    assert!(matches!(c.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    claim_id
}

/// The bystander's accusation at layer 0's fused site on the last prefill position of a job context
/// it made up — the object the unbound verdict defers (see the module note).
fn accusation(c: &Chain, row: &HeldRow, claim_id: Hash64) -> PalwConsensusObjectV2 {
    accusation_by(c, row, claim_id, bond_key(BYSTANDER), 1)
}

fn accusation_by(c: &Chain, row: &HeldRow, claim_id: Hash64, accuser: PalwBondKeyV2, back: u32) -> PalwConsensusObjectV2 {
    let claim = c.s.claim(&claim_id).expect("claim").clone();
    let profile = row.profile.clone();
    let mut job = row.job.clone();
    job.job_id = h(0x0A1E_0001);
    let prefill = job.declared_prefill_tokens;
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).expect("a slot");
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: prefill - back, tile_index: 0 };
    let leaf = canonical_step_leaf_index(&profile, &job, &coord).expect("a leaf");
    let binding = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        step_leaf_count: step_leaf_count_capped_v1(&profile, &job, u64::MAX).expect("leaves"),
        job_context: job,
        checkpoint_profile: kaspa_consensus_core::palw_state_chunk_map::integer_kv_checkpoint_profile_v1(
            kaspa_consensus_core::palw_state_chunk_map::PALW_INTEGER_KV_CHECKPOINT_INTERVAL_V1,
        ),
        state_chunk_map_id: profile.state_chunk_map_id,
        shape_profile: profile,
        full_logits_trace_root: h(0xBAD1),
        activation_leg_root: h(0xBAD2),
        step_merkle_root: h(0xBAD3),
        checkpoint_count: prefill,
        checkpoint_merkle_root: h(0xBAD4),
        committed_execution_root: claim.execution_root,
    };
    PalwConsensusObjectV2::ShardCourtAccused {
        accusation: Box::new(PalwShardCourtAccusationV1 {
            version: PALW_SHARD_COURT_VERSION_V1,
            claim: claim_id,
            execution_root: claim.execution_root,
            trace_root: claim.trace_root,
            executor_bond: claim.bond,
            accuser_bond: accuser,
            leaf_index: leaf,
            refutation: PalwExecutionStepRefutationV1 {
                binding,
                output_opening: PalwStepOpeningV1 { leaf_index: leaf, leaf_hash: h(0xBAD5), siblings: vec![] },
                output_preimage: PalwStepTileLeafV1 { version: 1, coord, value_count: 0, values_le: vec![] },
                inputs: vec![],
                prompt_token_ids: vec![],
                decode_tokens: None,
                kv_checkpoint: None,
            },
            artifact_openings: vec![],
            prompt_ids_opening: None,
            signature: vec![0; MLDSA87_SIGNATURE_LEN],
        }),
    }
}

fn open(c: &mut Chain, row: &HeldRow, claim_id: Hash64) -> Result<(), PalwStateV2Error> {
    let object = accusation(c, row, claim_id);
    let rule = if c.rule == Rule::Below { Rule::OpeningBelow } else { Rule::Opening };
    c.step(&[object], PalwBlockWorkV3::None, Hash64::default(), rule)
}

fn open_by(c: &mut Chain, row: &HeldRow, claim_id: Hash64, accuser: PalwBondKeyV2, back: u32) -> Result<(), PalwStateV2Error> {
    let object = accusation_by(c, row, claim_id, accuser, back);
    c.step(&[object], PalwBlockWorkV3::None, Hash64::default(), Rule::Opening)
}

#[test]
fn a_same_leaf_decoy_holds_off_no_seat() {
    let (mut c, row, seats) = chain(Rule::T12, 1_300_000_000_000);
    let claim_id = licensed(&mut c, &row, &seats, 1);
    // The producer's Sybil (a non-seat) opens first, at the leaf the lie sits on.
    open(&mut c, &row, claim_id).expect("the decoy opens at the lie's leaf");
    assert_eq!(c.s.court_sessions_for_claim(&claim_id), 1);
    // Every seat of the panel opens its own dissection at that same leaf.
    for (i, (seat, _)) in seats.iter().enumerate() {
        open_by(&mut c, &row, claim_id, *seat, 1).unwrap_or_else(|e| panic!("seat {i} at the decoy's leaf: {e}"));
        assert_eq!(c.s.court_sessions_for_claim(&claim_id), 2 + i, "seat {i}'s session beside the others");
    }
    assert_eq!(c.s.court_sessions_for_claim(&claim_id), 1 + seats.len(), "the decoy and one per seat");
    // One per seat, at any leaf; and a non-seat still waits.
    let again = open_by(&mut c, &row, claim_id, seats[0].0, 2).expect_err("a seat's second session");
    assert!(
        matches!(&again, PalwStateV2Error::HeldDissectionFurtherSessionRefused { why, .. } if why.contains("already holds")),
        "{again:?}"
    );
}
