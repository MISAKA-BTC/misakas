#![allow(dead_code, unused_imports)]
//! **ADR-0152 §4-ter (A-held) on testnet-12's own ruleset: the held court's endings and prices.**
//!
//! Everything is the network's own `Params`, its bundle's `PalwStateParamsV2` (the answerability
//! mirror included), its genesis fold and `apply_palw_transition_v7`, with the extras
//! `dos_l5_common::extras` resolves plus what that file holds dormant on purpose: the two court
//! ladders the processor resolves, and F2's `palw_offence_attribution` (testnet-12 arms it at DAA 0;
//! `dos_l5_common` pins the V1 route below it). The claims are licensed junk attempts on the genesis
//! 8k held row — the answerable one.
//!
//! **How a session is opened here, and why.** A held dissection opens only on a one-move accusation
//! the bound verdict (`palw_audit_2026_09_23`) defers — which needs the claim's real binding, i.e. a
//! real 8k execution (≈ 105M leaves), not a fixture. So the ONE block that opens a session is folded
//! with the 2026-09-23 fence forced off, where the unbound verdict defers the grief object the
//! `t12_shard_court_unbound_fused_grief` probe builds; every other block, the session's whole life
//! included, is folded under testnet-12's own fences. Nothing measured below depends on how the
//! session came to be: the endings are the sweep's and the charges the ledger's.
//!
//! * **C1 and the F2 residual.** The producer, colluding with the bystander that opened the court,
//!   never files its root claim: past `palw_offence_attribution` the answerable class's silence is a
//!   default — voided `CourtDefault`, charged as a fraud — and kind 3's `CourtFraud` naming it is
//!   refused by name against the full-mask `Valid` signer, whose lock stands. The red twin (the void
//!   recorded as `CourtFraud`) convicts that signer — the reason is the whole fix. Below the fence
//!   the same silence ends in the mercy.
//! * **C4's reservation.** A bystander at the 13,000 MSK producer floor opens one held dissection;
//!   its second, on another claim, must fit `max(committed, 500‰·C) + accuser + charge ≤ C` with
//!   BOTH sessions counted at their charge `max(reserved, G)` — refused. Below the fence the old
//!   reservation (`reserved` alone) admits it.

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
    chain_on_params(t12(), rule, bystander_collateral, which)
}

/// [`chain_on`] over `p` — testnet-12 as shipped, or past a flag day (`t12_2m_open`).
fn chain_on_params(
    p: Params,
    rule: Rule,
    bystander_collateral: u64,
    which: fn(&Params) -> HeldRow,
) -> (Chain, HeldRow, Vec<(PalwBondKeyV2, Hash64)>) {
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
    let claim = c.s.claim(&claim_id).expect("claim").clone();
    let profile = row.profile.clone();
    let mut job = row.job.clone();
    job.job_id = h(0x0A1E_0001);
    let prefill = job.declared_prefill_tokens;
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).expect("a slot");
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: prefill - 1, tile_index: 0 };
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
            accuser_bond: bond_key(BYSTANDER),
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

/// **C1 and the F2 residual on testnet-12's 8k row: a colluding producer's silence in a
/// bystander's held dissection is a DEFAULT, and a default convicts no signer.**
#[test]
fn a_colluding_silence_in_a_bystanders_held_dissection_convicts_no_signer() {
    let (mut c, row, seats) = chain(Rule::T12, 1_300_000_000_000);
    let claim_id = licensed(&mut c, &row, &seats, 1);
    open(&mut c, &row, claim_id).expect("the bystander's held dissection opens");
    assert_eq!(c.s.court_sessions_for_claim(&claim_id), 1);
    let before = c.s.clone();
    let executor_before = before.bond(&bond_key(EXECUTOR)).unwrap().collateral;
    for _ in 0..200 {
        c.empty();
        if c.s.court_sessions_for_claim(&claim_id) == 0 {
            break;
        }
    }
    let PalwClaimPhaseV2::Voided { voided_daa, reason } = c.s.claim(&claim_id).unwrap().phase else {
        panic!("past the fence the withheld root claim ends the court against the producer: {:?}", c.s.claim(&claim_id).unwrap().phase)
    };
    assert_eq!(reason, PalwVoidReasonV2::CourtDefault, "a default, never a proof");
    assert!(c.s.bond(&bond_key(EXECUTOR)).unwrap().collateral < executor_before, "charged as a fraud");
    assert_eq!(
        c.s.bond(&bond_key(BYSTANDER)).unwrap().collateral,
        before.bond(&bond_key(BYSTANDER)).unwrap().collateral,
        "the bystander pays nothing"
    );

    // Kind 3 naming the default, against the full-mask Valid signer: refused by name.
    let rules = PalwIdentityRulesV1 {
        prompt_ids_form: c.p.palw_prompt_ids_form_at(c.daa),
        base_class_id: c.sp.base_class_id(),
        da_signer_liability: false,
    };
    let seat = seats[0].0;
    let receipt = valid_receipts(claim_id, &[seat]).pop().expect("the licence's receipt");
    let evidence = |state: &PalwChainStateV2| {
        let payload = PalwPanelFalseValidEvidenceV2 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V2,
            claim_id,
            accused_seat: seat.0,
            receipt: PalwFalseValidReceiptV1::Full(receipt.clone()),
            contradiction: PalwPanelContradictionV1::CourtFraud { voided_daa },
            prompt_ids_opening: None,
            reporter_reveal: vec![],
        };
        palw_check_panel_false_valid_v2(state, &seat, &borsh::to_vec(&payload).unwrap(), false, false, rules, None)
    };
    let refused = evidence(&c.s).expect_err("a default convicts no seat");
    assert!(matches!(&refused, PalwOffenceVerifyError::ContradictionNotAdmitted(why) if why.contains("DEFAULT")), "{refused:?}");
    // The red twin: the same chain with the void recorded as the fraud the fold used to write.
    let mut carriage = PalwStateCarriageV2::from_state(&c.s);
    carriage.claims.get_mut(&claim_id).unwrap().phase = PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud };
    if let Some(row) = carriage.panel_liabilities.get_mut(&claim_id) {
        row.void_reason = Some(PalwVoidReasonV2::CourtFraud);
    }
    let red = carriage
        .into_state_v3(&c.sp, None, flags(&c.p, 0).uncertified_weightless, c.p.palw_canonical_work_daa())
        .expect("the red twin rebuilds");
    assert!(
        evidence(&red).is_ok(),
        "recorded as CourtFraud, the colluding silence would convict the honest signer: {:?}",
        evidence(&red)
    );

    // Below the fence the same silence ends in the mercy.
    let (mut c, row, seats) = chain(Rule::Below, 1_300_000_000_000);
    let claim_id = licensed(&mut c, &row, &seats, 1);
    open(&mut c, &row, claim_id).expect("opens");
    for _ in 0..200 {
        c.empty();
        if c.s.court_sessions_for_claim(&claim_id) == 0 {
            break;
        }
    }
    assert!(
        !matches!(c.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { .. }),
        "below the fence the held mercy stands: {:?}",
        c.s.claim(&claim_id).unwrap().phase
    );
}

/// **C4's reservation on testnet-12's one ledger: a bystander at the producer floor holds ONE held
/// dissection's charge in its free half, not two.**
#[test]
fn a_floor_bystander_holds_one_held_dissection_charge_not_two() {
    let floor = 1_300_000_000_000; // 13,000 MSK
    let (mut c, row, seats) = chain(Rule::T12, floor);
    let (a, b) = (licensed(&mut c, &row, &seats, 1), licensed(&mut c, &row, &seats, 2));
    let claim = c.s.claim(&a).unwrap().clone();
    open(&mut c, &row, a).expect("the first held dissection fits the free half");
    let held = palw_accuser_exposure_v1(&c.s, &bond_key(BYSTANDER));
    assert_eq!(held, claim.reserved, "the court index counts a session at the claim's reserved");
    let refused = open(&mut c, &row, b).expect_err("the second does not fit once both are counted at their charge");
    assert!(matches!(&refused, PalwStateV2Error::AccusationExposureCeiling { edge, .. } if *edge == "held dissection"), "{refused:?}");
    println!("claim reserved {:.2} MSK, bystander floor {:.2} MSK: {refused}", msk(claim.reserved), msk(u128::from(floor)));

    // Below the fence the reservation is the claim's reserved alone: the second opens.
    let (mut c, row, seats) = chain(Rule::Below, floor);
    let (a, b) = (licensed(&mut c, &row, &seats, 1), licensed(&mut c, &row, &seats, 2));
    open(&mut c, &row, a).expect("opens");
    open(&mut c, &row, b).expect("below the fence the second held dissection opens on reserved alone");
    assert_eq!((c.s.court_sessions_for_claim(&a), c.s.court_sessions_for_claim(&b)), (1, 1));
}

/// **The 2M row opens no held dissection on testnet-12** (the shard-court addendum on the network's
/// own ruleset).
///
/// * **At launch the 2M row takes no claim at all** (re-pinned at the pre-merge of
///   feat/t12-class-verify-deadline: ADR-0152 §4-quater V2 / U-D1 close every 2M claim on every lane,
///   `ClassDeadlineUnmeasured`, until a flag day installs a measured row) — so no dissection can open
///   on it, a stronger closure than the addendum's.
/// * **Past that flag day** (`t12_2m_open`: a measured row, the only configuration in which a 2M
///   claim exists) the addendum still holds: the one-move accusation the unbound verdict defers to a
///   dissection is refused `ShardCourtHeldSiteUnanswerable` past `palw_offence_attribution` — no
///   session, no reservation, the accuser's collateral untouched. Below the fence the same
///   accusation opens the dissection it always did.
#[test]
fn the_2m_row_opens_no_held_dissection_on_testnet_12() {
    // A bystander that can back the 2M claim's reservation (≈ 59,743 MSK), so the twin below the
    // fence opens on it and the refusal above is the addendum's, not the ceiling's.
    let collateral = 100_000_000_000_000; // 1,000,000 MSK
    // At launch: the 2M attempt itself is refused, by name.
    let (mut c, row, _) = chain_on(Rule::T12, collateral, two_m);
    let pwu = palw_pwu_v1(row.target, row.leaves);
    let (env, key, _) =
        junk_attempt(row.id, bond_key(EXECUTOR), pubkey_of(EXECUTOR), &operator_pubkey_of(EXECUTOR), pwu, 1, 0x0A1E_0001);
    let rule = c.rule;
    let closed = c.step(&[], PalwBlockWorkV3::Attempt(&env), key, rule).expect_err("the 2M row takes no claim at launch");
    assert!(
        matches!(&closed, PalwStateV2Error::ClassDeadlineUnmeasured { class, .. } if *class == row.id),
        "closed by name at launch: {closed:?}"
    );
    // Past the flag day that opens it.
    let (mut c, row, seats) = chain_on_params(t12_2m_open(), Rule::T12, collateral, two_m);
    assert!(c.sp.held_class_is_unanswerable_v1(&row.id), "the bundle mirrors the 2M row as unanswerable");
    let claim_id = licensed(&mut c, &row, &seats, 1);
    let bystander_before = c.s.bond(&bond_key(BYSTANDER)).unwrap().clone();
    let refused = open(&mut c, &row, claim_id).expect_err("past the fence the 2M row's dissection never opens");
    assert!(
        matches!(&refused, PalwStateV2Error::ShardCourtHeldSiteUnanswerable { class, .. } if *class == row.id),
        "refused by name: {refused:?}"
    );
    assert_eq!(c.s.court_sessions_for_claim(&claim_id), 0, "no session");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &bond_key(BYSTANDER)), 0, "no reservation");
    assert_eq!(c.s.bond(&bond_key(BYSTANDER)).unwrap(), &bystander_before, "the accuser untouched");
    assert!(matches!(c.s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the claim stands");

    // Below the fence: the dissection opens, as it did before the addendum.
    let (mut c, row, seats) = chain_on_params(t12_2m_open(), Rule::Below, collateral, two_m);
    let claim_id = licensed(&mut c, &row, &seats, 1);
    open(&mut c, &row, claim_id).expect("below the fence the 2M row's dissection opens");
    assert_eq!(c.s.court_sessions_for_claim(&claim_id), 1);
}

/// **The review's F7: every accuser gate counts an open held session at its charge.** A bystander at
/// the producer floor (13,000 MSK: a free half of 6,500) opens one held dissection — its charge,
/// `min(max(reserved, G), floor)` ≈ 3,695 MSK, where the court index counts `reserved` ≈ 494 — and
/// then accuses other claims through the data-availability court (eight `Final` 8k claims whose
/// vesting rows are unmatured: DA-8 keeps them accusable, and the class's inflight cap of five is
/// not held by them). Past the fence the DA gate reads the accuser ledger with the held session at
/// its charge: the accusations stop where `charge + Σ exposure` would pass the free half, where the
/// court index's count alone would still fit them. Below the fence (`palw_offence_attribution` off,
/// R-core+ on) the held session counts at `reserved` and the accusation refused past the fence opens
/// — the ≈ 3,200 MSK over-commit the review measured.
#[test]
fn every_accuser_gate_counts_a_held_session_at_its_charge() {
    let floor = 1_300_000_000_000u64; // 13,000 MSK
    let free_half = u128::from(floor) / 2;
    let accuse = |id: Hash64| PalwConsensusObjectV2::DefaultAccused {
        claim: id,
        missing_event_index: kaspa_consensus_core::palw_state_v2::palw_da_event_index_v1(0, 0),
        accuser: bond_key(BYSTANDER),
        signature: vec![],
    };
    let mut refused_at = None;
    for rule in [Rule::T12, Rule::Below] {
        let (mut c, row, seats) = chain(rule, floor);
        // Eight claims licensed four at a time and run to `Final` (the inflight cap is five).
        let mut finals = Vec::new();
        for batch in 0..2u64 {
            let ids: Vec<Hash64> = (0..4).map(|n| licensed(&mut c, &row, &seats, 10 * (batch + 1) + n)).collect();
            for _ in 0..400 {
                c.empty();
                if ids.iter().all(|id| matches!(c.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { .. })) {
                    break;
                }
            }
            assert!(
                ids.iter().all(|id| matches!(c.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { .. })),
                "batch {batch} Final"
            );
            finals.extend(ids);
        }
        let held_claim = licensed(&mut c, &row, &seats, 1);
        open(&mut c, &row, held_claim).expect("the held dissection opens");
        let held = palw_accuser_exposure_v1(&c.s, &bond_key(BYSTANDER));
        let mut exposures = Vec::new();
        let mut refusal = None;
        for id in &finals {
            match c.step(&[accuse(*id)], PalwBlockWorkV3::None, Hash64::default(), rule) {
                Ok(()) => exposures.push(c.s.da_session(id, &bond_key(BYSTANDER)).expect("the DA session").exposure),
                Err(e) => {
                    refusal = Some(e);
                    break;
                }
            }
        }
        assert_eq!(c.s.court_sessions_for_claim(&held_claim), 1, "the held session stayed open throughout");
        let sum: u128 = exposures.iter().sum();
        match rule {
            Rule::T12 => {
                let refusal = refusal.expect("past the fence the DA gate stops the bystander");
                let PalwStateV2Error::AccusationExposureCeiling { edge, accusation, backed, .. } = &refusal else {
                    panic!("refused by the accuser gate: {refusal:?}")
                };
                assert_eq!(*edge, "data-availability session");
                // The ledger the gate read (backed = C − room = C/2 + ledger, the bystander committing
                // no work): above the court index's count by the held session's surplus, and past the
                // free half with the next accusation — where the index's count alone still fits it.
                let ledger = *backed - free_half;
                assert!(ledger > held + sum, "the held session is counted at its charge, not at {held} reserved");
                assert!(ledger + *accusation > free_half, "refused because it does not fit the free half");
                assert!(held + sum + *accusation <= free_half, "counted at reserved, it would have fit: the over-commit closed");
                println!(
                    "past the fence: {} DA sessions ({:.2} MSK) beside a held session reserved {:.2} MSK and charged {:.2} MSK; the next ({:.2} MSK) refused",
                    exposures.len(),
                    msk(sum),
                    msk(held),
                    msk(ledger - sum),
                    msk(*accusation)
                );
                refused_at = Some(exposures.len());
            }
            _ => {
                let n = refused_at.expect("the fence first");
                assert!(
                    exposures.len() > n,
                    "below the fence the held session counts at reserved and the accusation refused past it opens ({} > {n})",
                    exposures.len()
                );
                assert!(held + sum <= free_half, "the old ledger: reserved + Σ exposure within the free half");
            }
        }
    }
}

// ---- The review's F3 and the user's decision (B): a dissection's verdict is the producer's ------
//
// The reviewer's probe and the split-δ probe (review_aheld_honest_execution_self_conviction.rs,
// review_aheld_split_disclosure_self_conviction.rs), adopted: the court's own kernels, an HONEST
// execution, and a bottom that reads `ExecutorGuilty` anyway — the verdict proves the responder's
// disclosure false, not the execution. The fold then records it `CourtHeldVerdict` on testnet-12,
// and kind 3 refuses it against the signers who replayed that execution.

mod probe {
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1,
    };
    use kaspa_consensus_core::palw_attn_dissect::{
        PALW_ATTN_DISSECT_OBJECT_VERSION_V1, PalwAttnDissectRoundV1, PalwAttnRangeClaimV1, PalwAttnRootClaimV1, palw_attn_fold_v1,
    };
    use kaspa_consensus_core::palw_base0_a16::{
        A16AttnFusedParamsV1, A16QuantParams, a16_attn_finalize_v1, a16_attn_root_claim_v1, a16_attn_tile_triple_v1,
    };
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;

    fn params() -> A16AttnFusedParamsV1 {
        A16AttnFusedParamsV1 {
            scores: A16QuantParams { multiplier: 1 << 10, shift: 30, zero: 3 },
            probs: A16QuantParams { multiplier: 1 << 15, shift: 24, zero: 0 },
            values: A16QuantParams { multiplier: 1, shift: 22, zero: -5 },
            up_bits: 2,
        }
    }

    const D: usize = 4; // d_head = kv_dim (one KV head)
    const H: usize = 12; // history positions
    const TILE: usize = 4; // positions per history tile: three tiles

    fn series() -> (Vec<i32>, Vec<i32>, Vec<i32>) {
        let q = vec![900, -1_200, 3_000, 150];
        let k: Vec<i32> = (0..H * D).map(|i| ((i as i32 * 7_919) % 20_000) - 10_000).collect();
        let v: Vec<i32> = (0..H * D).map(|i| ((i as i32 * 104_729) % 30_000) - 15_000).collect();
        (q, k, v)
    }

    /// The honest claim over tiles `[first, first + count)`, against the root's `(m*, S*)`.
    fn honest_range(first: u64, count: u64, m: i32, s: i64) -> PalwAttnRangeClaimV1 {
        let (q, k, v) = series();
        let tiles: Vec<PalwAttnRangeClaimV1> = (first..first + count)
            .map(|t| {
                let (a, b) = (t as usize * TILE, ((t as usize + 1) * TILE).min(H));
                a16_attn_tile_triple_v1(&q, &k[a * D..b * D], &v[a * D..b * D], D, 0, (0, D), params(), m, s).expect("a tile")
            })
            .collect();
        palw_attn_fold_v1(&tiles).expect("folds")
    }

    /// **The court's own kernels on an HONEST execution.** `root_lie` moves the filed root's `V*` (it
    /// still finalizes to the honest committed tile); `split` leaves the root honest and splits `+lie
    /// / −lie` across the first round's two children. Either way the lie then rides child 0, the
    /// challenger names it at every round, and the result is the narrowed claim beside the bottom's
    /// recompute of that tile.
    pub(super) fn play(lie: i64, split: bool) -> (PalwAttnRangeClaimV1, PalwAttnRangeClaimV1) {
        let (q, k, v) = series();
        let honest = a16_attn_root_claim_v1(&q, &k, &v, D, 0, (0, D), params(), TILE).expect("the honest root");
        let committed = a16_attn_finalize_v1(&honest.v_acc, params().values);
        let mut claimed = honest.clone();
        if !split {
            claimed.v_acc[0] += lie;
        }
        assert_eq!(a16_attn_finalize_v1(&claimed.v_acc, params().values), committed, "the filed root finalizes to the honest tile");
        let root = PalwAttnRootClaimV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: 0,
            lane_first: 0,
            lane_count: D as u16,
            history_positions: H as u32,
            claim: claimed,
        };
        let session = Hash64::from_u64_word(0x5E1F);
        let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
            session,
            &root,
            (0, 0, D as u16),
            H as u32,
            &committed,
            params().values,
            2,
            TILE as u32,
            0,
            10,
            true,
        )
        .expect("the phase opens");
        let (m, s) = phase.root_scale();
        let (mut daa, mut first) = (1, true);
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let mut children: Vec<PalwAttnRangeClaimV1> =
                phase.child_ranges().iter().map(|&(f, c)| honest_range(f, c, m, s)).collect();
            children[0].v_acc[0] += lie;
            if split && first {
                children[1].v_acc[0] -= lie;
            }
            first = false;
            phase
                .apply_round(
                    &PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children: children.clone() },
                    daa,
                    10,
                )
                .expect("the disclosure folds to the filed root");
            let named = phase
                .child_ranges()
                .iter()
                .zip(&children)
                .position(|(&(f, c), claim)| honest_range(f, c, m, s) != *claim)
                .map(|i| i as u8)
                .unwrap_or(0);
            let choice = PalwAttnDissectChoiceV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id: session,
                round: phase.round(),
                child: named,
            };
            phase.apply_choice(&choice, daa + 1, 10).expect("a legal choice");
            daa += 2;
        }
        let tile = phase.terminal_tile().expect("narrowed");
        (phase.claim().clone(), honest_range(tile, 1, m, s))
    }
}

/// **The review's F3 on testnet-12's own ruleset, decision (B): a held dissection's verdict convicts
/// the producer and no signer.**
///
/// The premise, on the court's kernels (the reviewer's probe and the split-δ probe): an HONEST
/// execution's dissection bottoms `ExecutorGuilty` when its responder lies in the root (`V* + 1`) or
/// splits a lie across one round's children over an honest root — the verdict proves the disclosure
/// false, never the execution. On testnet-12's fold, a held dissection's `ExecutorGuilty` close (an
/// `AttnDissection` proof over the 8k row) therefore voids `CourtHeldVerdict`: the producer is charged
/// (S-4's tier), S-4's `CourtConviction` record is written and the challenger's reward opened; the
/// full-mask `Valid` signer's lock is untouched, and kind 3's `CourtFraud` naming the void is refused
/// by name — the red twin (the same void as `CourtFraud`) is admitted. Below the fence the same close
/// voids `CourtFraud`, as it always did.
#[test]
fn a_held_dissections_verdict_convicts_the_producer_and_no_signer_on_testnet_12() {
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnDissectBottomV1, PalwAttnRowOpeningV1, PalwAttnTileEvidenceV1,
    };
    use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
    use kaspa_consensus_core::palw_state_v2::{PalwCourtVerdictV2, palw_court_conviction_offence_id_v1};
    for (name, split) in [("the reviewer's V*+1", false), ("the split-δ", true)] {
        let (narrowed, recomputed) = probe::play(1, split);
        assert_ne!(narrowed, recomputed, "{name}: an honest execution's bottom reads ExecutorGuilty — the premise");
    }

    for rule in [Rule::T12, Rule::Below] {
        let (mut c, row, seats) = chain(rule, 1_300_000_000_000);
        let claim_id = licensed(&mut c, &row, &seats, 1);
        open(&mut c, &row, claim_id).expect("the bystander's held dissection opens");
        let sid = c.s.court_sessions_iter().find(|(_, x)| x.claim == claim_id).map(|(k, _)| *k).expect("the session");
        let before = c.s.clone();
        // The close of a held dissection: its bottom's proof (the fold records the acceptance layer's
        // adjudicated verdict; it reads the proof's form, never re-derives it).
        let PalwConsensusObjectV2::ShardCourtAccused { accusation } = accusation(&c, &row, claim_id) else { unreachable!() };
        let tile = |index: u64| PalwAttnRowOpeningV1 {
            leaf: accusation.refutation.output_preimage.clone(),
            opening: PalwStepOpeningV1 { leaf_index: index, leaf_hash: h(0xBAD5), siblings: vec![] },
        };
        let proof = PalwCourtVerdictProofV2::AttnDissection {
            binding: Box::new(accusation.refutation.binding.clone()),
            bottom: Box::new(PalwAttnDissectBottomV1 {
                version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id: sid,
                tile: 0,
                query: tile(0),
                anchor: None,
                k: PalwAttnTileEvidenceV1::CacheWrites { rows: vec![] },
                v: PalwAttnTileEvidenceV1::CacheWrites { rows: vec![] },
                out_tile: tile(accusation.leaf_index),
            }),
            operand_openings: vec![],
        };
        c.step(
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict: PalwCourtVerdictV2::ExecutorGuilty, proof }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            rule,
        )
        .expect("the close folds");
        let PalwClaimPhaseV2::Voided { voided_daa, reason } = c.s.claim(&claim_id).unwrap().phase else {
            panic!("{rule:?}: the close voids the claim: {:?}", c.s.claim(&claim_id).unwrap().phase)
        };
        if rule == Rule::Below {
            assert_eq!(reason, PalwVoidReasonV2::CourtFraud, "below the fence the verdict is the fraud it always was");
            continue;
        }
        assert_eq!(reason, PalwVoidReasonV2::CourtHeldVerdict, "a held dissection's verdict (decision (B))");
        assert!(
            c.s.bond(&bond_key(EXECUTOR)).unwrap().collateral < before.bond(&bond_key(EXECUTOR)).unwrap().collateral,
            "the producer is charged"
        );
        let key = palw_court_conviction_offence_id_v1(&bond_key(EXECUTOR).0, &claim_id);
        assert!(c.s.consumed_offence(&key).is_some(), "S-4's CourtConviction record");
        assert!(
            c.s.reward_pending(&key)
                .and_then(|reward| reward.best.as_ref())
                .is_some_and(|winner| winner.reporter == bond_key(BYSTANDER)),
            "the challenger's reporter reward"
        );
        let seat = seats[0].0;
        assert_eq!(c.s.bond(&seat).unwrap().collateral, before.bond(&seat).unwrap().collateral, "the Valid signer is not charged");

        // Kind 3 naming the void, against the full-mask Valid signer: refused by name.
        let rules = PalwIdentityRulesV1 {
            prompt_ids_form: c.p.palw_prompt_ids_form_at(c.daa),
            base_class_id: c.sp.base_class_id(),
            da_signer_liability: false,
        };
        let receipt = valid_receipts(claim_id, &[seat]).pop().expect("the licence's receipt");
        let kind3 = |state: &PalwChainStateV2| {
            let payload = PalwPanelFalseValidEvidenceV2 {
                version: PALW_PANEL_FALSE_VALID_VERSION_V2,
                claim_id,
                accused_seat: seat.0,
                receipt: PalwFalseValidReceiptV1::Full(receipt.clone()),
                contradiction: PalwPanelContradictionV1::CourtFraud { voided_daa },
                prompt_ids_opening: None,
                reporter_reveal: vec![],
            };
            palw_check_panel_false_valid_v2(state, &seat, &borsh::to_vec(&payload).unwrap(), false, false, rules, None)
        };
        let refused = kind3(&c.s).expect_err("a held verdict convicts no signer");
        assert!(
            matches!(&refused, PalwOffenceVerifyError::ContradictionNotAdmitted(why) if why.contains("HELD DISSECTION")),
            "{refused:?}"
        );
        let mut carriage = PalwStateCarriageV2::from_state(&c.s);
        carriage.claims.get_mut(&claim_id).unwrap().phase =
            PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud };
        if let Some(row) = carriage.panel_liabilities.get_mut(&claim_id) {
            row.void_reason = Some(PalwVoidReasonV2::CourtFraud);
        }
        let red = carriage
            .into_state_v3(&c.sp, None, flags(&c.p, 0).uncertified_weightless, c.p.palw_canonical_work_daa())
            .expect("the red twin rebuilds");
        assert!(kind3(&red).is_ok(), "recorded as CourtFraud, the same void would convict the honest signer: {:?}", kind3(&red));
    }
}
