//! **Lane g14-close-legacy (2026-10-16): the legacy V2 Panel route made G14-complete behind LG14-A / LG14-B** — the fold half
//! (audit `docs/audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md` §3.2, §4.1 G-06 / G-07 / G-13, appendix C).
//!
//! * **Arming** — LG14-A (`palw_legacy_public_filer_v1`) and LG14-B (`palw_legacy_held_da_v2`) ride the arm-readiness record gate
//!   (`kaspa_consensus_core::palw_g14_gates_v1`); every ruleset here that arms a fence PASSES the whole validation
//!   (`Params::validate_palw_v2_with_arming_records_v1`) with a record naming its genesis and height, and is refused without it.
//! * **G-06** — a bond holding a live reservation opens its held dissection beside a decoy court the producer's Sybil opened first,
//!   and the reservation holds the claim's `Final` past its floor.
//! * **G-07** — the reserver's DA sessions ride its own budget (no shared DA-8 or reservation count excludes it); the NETWORK court
//!   cap is the residual this lane does not close, pinned here.
//! * **G-13** — a `Valid` lock on a claim accepted past LG14-A lives to the claim's liability horizon (`F + window_court`), not lane
//!   V02's `F + 1,000`; a conviction that would find a relied-on lock gone is refused (`palw_g13_relied_lock_lost_v1`).
//! * **G14 from genesis** — every fence of the route at DAA 0 on an isolated genesis: V2 Attempt admitted from block 0, held, locked.
//! * **testnet-12's identity** does not move.
//!
//! Every block goes through the real fold (`apply_palw_transition_v7`) on testnet-12's launch ruleset; each fence is armed on the
//! `Params` and mirrored by its own `sync_*`, exactly as a network would arm it, and the ruleset is asserted to validate with a test
//! record — no fence is set on a mirror alone (audit G-19).
//!
//! Run: cargo test -p kaspa-consensus-core --test g14_close_legacy -j 4 -- --test-threads=2

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_launch_params_v1, palw_t12_shipped_params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_g14_gates_v1::*;
use kaspa_consensus_core::palw_legacy_public_filer_v1::{PALW_DISPUTE_RESERVATION_VERSION_V1, PalwDisputeReservationV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_shard_court_v1::{PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwStateV2Error, palw_false_valid_lock_slashable_v1, palw_g13_relied_lock_lost_v1, palw_lock_is_committed_v1,
    palw_max_concurrent_court_sessions_v1,
};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;

/// The release height the Params-level tests use (DAA 14,000).
const RELEASE: u64 = 14_000;
/// Bonds registered after genesis (none is a genesis card, an operator or a seat).
const OUTSIDER: u64 = 0x0614_0001;
const BYSTANDER: u64 = 0x0614_0002;
const EXECUTOR: u64 = 0x0614_00E0;
const SYBIL_BASE: u64 = 0x0614_1000;

// ---- records and arming -----------------------------------------------------------------------------------------------------------

/// A test arm-readiness record for `fence` on `genesis` at `at` — the format a reviewed change would pin (one distinct non-zero
/// digest per requirement, a review, a source commit). Never pinned in the binary.
fn record(fence: PalwG14GatedFenceV1, genesis: Hash64, at: u64) -> PalwG14ArmingRecordV1 {
    let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x40 + i as u8; 32]).collect();
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: at,
        source_commit: [0x14; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

/// `p` with LG14-A (and, with `held`, LG14-B; with `route`, option (b)) armed at `at`, every mirror synced, and the records the
/// arming needs. The ruleset is asserted to pass the whole validation with them and to be refused without them.
fn arm_legacy(mut p: Params, at: u64, held: bool, route: bool) -> (Params, Vec<PalwG14ArmingRecordV1>) {
    let fence = Some(ForkActivation::new(at));
    let mut records = vec![record(PalwG14GatedFenceV1::LegacyPublicFilerV1, p.genesis.hash, at)];
    p.palw_legacy_public_filer_v1 = fence;
    p.sync_palw_legacy_public_filer_v1();
    if held {
        p.palw_legacy_held_da_v2 = fence;
        records.push(record(PalwG14GatedFenceV1::LegacyHeldDaV2, p.genesis.hash, at));
    }
    if route {
        p.palw_g14_route_required_v1 = fence;
        records.push(record(PalwG14GatedFenceV1::G14RouteRequiredV1, p.genesis.hash, at));
    }
    p.sync_palw_g14_gates_v1();
    p.validate_palw_v2_with_arming_records_v1(&records).unwrap_or_else(|e| panic!("the armed ruleset validates with its records: {e:?}"));
    let unpinned = p.validate_palw_v2().expect_err("no record is pinned in this binary");
    assert!(format!("{unpinned:?}").contains("no arm-readiness record"), "{unpinned:?}");
    (p, records)
}

fn refused_with(e: &PalwStateV2Error, want: &str) -> bool {
    e.to_string().contains(want) || format!("{e:?}").contains(want)
}

/// A reservation of `claim` by `reserver` (the fold never reads the signature: the acceptance layer's).
fn reservation(s: &PalwChainStateV2, claim: Hash64, reserver: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let record = s.claim(&claim).expect("the claim").clone();
    PalwConsensusObjectV2::DisputeReservedV1 {
        reservation: Box::new(PalwDisputeReservationV1 {
            version: PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim,
            execution_root: record.execution_root,
            trace_root: record.trace_root,
            reserver,
        }),
        signature: Vec::new(),
    }
}

// ---- Task 1: LG14-A / LG14-B armable behind evidence ------------------------------------------------------------------------------

/// **LG14-A and LG14-B are armable behind a record naming the network and height, and only so.** The binary pins none: every real
/// height stays refused. LG14-B needs LG14-A at or below it. With both armed (and option (b)), the V2 Attempt family has an armed G14
/// route at the release height — the only change option (b)'s gate reads.
#[test]
fn lg14_arming_needs_a_record_naming_the_network_and_height() {
    assert!(PALW_G14_ARMING_RECORDS_V1.is_empty(), "no record is pinned in this binary");
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    // Both new gated fences: listed, measured, refused without a record.
    for fence in [PalwG14GatedFenceV1::LegacyPublicFilerV1, PalwG14GatedFenceV1::LegacyHeldDaV2] {
        assert!(PalwG14GatedFenceV1::ALL.contains(&fence), "{fence:?} is a gated fence");
        assert!(fence.needs_measured_value(), "{fence:?}: the hard-deadline envelope / descent budget are measurements");
        assert_eq!(fence.armed(&t12), None, "{fence:?}: dormant on testnet-12");
    }
    assert_eq!(PalwG14GatedFenceV1::LegacyPublicFilerV1.requirements(), PALW_LEGACY_FILER_ARMING_REQUIREMENTS_V1);
    assert_eq!(PalwG14GatedFenceV1::LegacyHeldDaV2.requirements(), PALW_LEGACY_HELD_DA_ARMING_REQUIREMENTS_V1);
    assert!(PALW_LEGACY_FILER_ARMING_REQUIREMENTS_V1.iter().any(|r| r.contains("legacy_v2_attempt_full_collusion_outside_bond_convicts")));
    assert!(PALW_LEGACY_HELD_DA_ARMING_REQUIREMENTS_V1.iter().any(|r| r.contains("legacy_v2_attempt_held_da_withholding_ends_in_da_default")));

    // LG14-A + LG14-B + option (b) at DAA 14,000 on testnet-12: refused unpinned, validates with exactly its records.
    let (p, records) = arm_legacy(t12.clone(), RELEASE, true, true);
    assert_eq!(records.len(), 3);
    for i in 0..records.len() {
        let mut fewer = records.clone();
        let dropped = fewer.remove(i);
        let why = p.validate_palw_v2_with_arming_records_v1(&fewer).expect_err("every record is load-bearing");
        assert!(format!("{why:?}").contains(dropped.fence.name()), "{:?}: {why:?}", dropped.fence);
    }
    let elsewhere: Vec<PalwG14ArmingRecordV1> =
        records.iter().map(|r| PalwG14ArmingRecordV1 { activation_daa: RELEASE + 1, ..*r }).collect();
    assert!(p.validate_palw_v2_with_arming_records_v1(&elsewhere).is_err(), "a record for another height authorizes nothing");
    let other_net: Vec<PalwG14ArmingRecordV1> =
        records.iter().map(|r| PalwG14ArmingRecordV1 { genesis: h(0xBAD), ..*r }).collect();
    assert!(p.validate_palw_v2_with_arming_records_v1(&other_net).is_err(), "a record for another network authorizes nothing");
    let malformed: Vec<PalwG14ArmingRecordV1> = records.iter().map(|r| PalwG14ArmingRecordV1 { review: [0; 32], ..*r }).collect();
    assert!(p.validate_palw_v2_with_arming_records_v1(&malformed).is_err(), "a record without a review is refused");

    // LG14-B needs LG14-A at or below it — refused by name even with its own record.
    let mut b_only = t12.clone();
    b_only.palw_legacy_held_da_v2 = Some(ForkActivation::new(RELEASE));
    b_only.sync_palw_g14_gates_v1();
    let b_record = [record(PalwG14GatedFenceV1::LegacyHeldDaV2, t12.genesis.hash, RELEASE)];
    let why = b_only.validate_palw_v2_with_arming_records_v1(&b_record).expect_err("LG14-B alone");
    assert!(format!("{why:?}").contains("needs palw_legacy_public_filer_v1"), "{why:?}");
    let (mut late_a, mut late_records) = arm_legacy(t12.clone(), RELEASE + 10, false, false);
    late_a.palw_legacy_held_da_v2 = Some(ForkActivation::new(RELEASE));
    late_records.push(record(PalwG14GatedFenceV1::LegacyHeldDaV2, t12.genesis.hash, RELEASE));
    assert!(late_a.validate_palw_v2_with_arming_records_v1(&late_records).is_err(), "LG14-B below LG14-A");
    // `never()` is dormant and needs no record.
    let mut never = t12.clone();
    never.palw_legacy_public_filer_v1 = Some(ForkActivation::never());
    never.palw_legacy_held_da_v2 = Some(ForkActivation::never());
    never.sync_palw_legacy_public_filer_v1();
    never.sync_palw_g14_gates_v1();
    never.validate_palw_v2().expect("never() is dormant");

    // Option (b)'s gate now counts V2 Attempt as routed from the release height (LG14-A + LG14-B in force), and not before.
    let sp = bundle(&p).state;
    let mirror = sp.g14_route_required_v1().expect("option (b) mirrored");
    assert_eq!((mirror.legacy_filer_from, mirror.legacy_held_da_from), (Some(RELEASE), Some(RELEASE)));
    assert_eq!(palw_g14_route_attempt_refusal_v1(&sp, RELEASE), None, "V2 Attempt has an armed G14 route at 14,000");
    assert!(mirror.route_armed(PalwG14RouteFamilyV1::LegacyPanelAttempt, RELEASE, false));
    // ...while option (b) WITHOUT LG14-B still refuses it (the gates lane's T-N8, unchanged).
    let (half, _) = arm_legacy(t12.clone(), RELEASE, false, true);
    assert!(palw_g14_route_attempt_refusal_v1(&bundle(&half).state, RELEASE).is_some());
    // Arming moves the ids (Some-only hashing); the shipped preset does not move (the identity test below).
    assert_ne!(p.consensus_params_id(), t12.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), t12.consensus_schedule_id());
}

/// **Free Prompt: the families that can recover through the TIR frame have no remaining dependency gap on LG14-A.** G-18 made the
/// staged court (constraint, prefix, tokenizer) need LG14-A, which no network could arm; LG14-A is now armable behind its record, so
/// testnet-12 at DAA 14,000 with LG14-A, the staged court, the staged TIR route, the TIR public conviction route and option (b) passes
/// the whole validation with their records — and option (b)'s gate then admits every public FP family on a FRAMED commitment. The
/// G-05 job versions stay refused unframed, and non-public evidence (V5, PanelDa) never has a route (option (b), unchanged).
#[test]
fn fp_recoverable_families_have_no_remaining_lg14a_dependency() {
    let t12 = palw_t12_shipped_params();
    let mut p = t12.clone();
    let at = Some(ForkActivation::new(RELEASE));
    p.palw_legacy_public_filer_v1 = at;
    p.sync_palw_legacy_public_filer_v1();
    p.palw_staged_public_court_v1 = at;
    p.palw_staged_tir_route_v1 = at;
    p.palw_tir_public_conviction_v1 = at;
    p.palw_g14_route_required_v1 = at;
    p.sync_palw_g14_gates_v1();
    let records: Vec<PalwG14ArmingRecordV1> = [
        PalwG14GatedFenceV1::LegacyPublicFilerV1,
        PalwG14GatedFenceV1::StagedPublicCourtV1,
        PalwG14GatedFenceV1::StagedTirRouteV1,
        PalwG14GatedFenceV1::TirPublicConvictionV1,
        PalwG14GatedFenceV1::G14RouteRequiredV1,
    ]
    .into_iter()
    .map(|fence| record(fence, t12.genesis.hash, RELEASE))
    .collect();
    p.validate_palw_v2_with_arming_records_v1(&records).unwrap_or_else(|e| panic!("the FP route ruleset validates with its records: {e:?}"));
    // Without LG14-A's record the staged court's prerequisite cannot be armed (G-18 stands; only the record unlocks it).
    let without_filer: Vec<PalwG14ArmingRecordV1> =
        records.iter().copied().filter(|r| r.fence != PalwG14GatedFenceV1::LegacyPublicFilerV1).collect();
    let why = p.validate_palw_v2_with_arming_records_v1(&without_filer).expect_err("LG14-A unrecorded");
    assert!(format!("{why:?}").contains("palw_legacy_public_filer_v1"), "{why:?}");
    let mirror = bundle(&p).state.g14_route_required_v1().expect("option (b)");
    eprintln!("[FP] route mirror at {RELEASE}: {mirror:?}");
    use PalwG14RouteFamilyV1 as F;
    for family in [F::FpSelection, F::FpConstraint, F::FpPrefix, F::FpTokenizer] {
        assert!(mirror.route_armed(family, RELEASE, true), "{family:?}: routed on a framed commitment at {RELEASE}");
    }
    assert!(!mirror.route_armed(F::FpSelection, RELEASE, false), "unframed: no selection route (G-05 stays refused)");
    assert!(!mirror.route_armed(F::NonPublicEvidence, RELEASE, true), "non-public evidence never");
    // The same ruleset without LG14-A (the gates lane's state): constraint / prefix / tokenizer have no route.
    let mut no_a = p.clone();
    no_a.palw_legacy_public_filer_v1 = None;
    no_a.sync_palw_legacy_public_filer_v1();
    no_a.sync_palw_g14_gates_v1();
    let half = bundle(&no_a).state.g14_route_required_v1().expect("option (b)");
    for family in [F::FpConstraint, F::FpPrefix, F::FpTokenizer] {
        assert!(!half.route_armed(family, RELEASE, true), "{family:?} without LG14-A");
    }
}

// ---- the held-row fixture (G-06, G-07): `t12_aheld_same_leaf_decoy`'s, with the outsider and the LG14-A fence ------------------

/// How a block folds: testnet-12's fences, or the block that OPENS a held dissection (the 2026-09-23 fence off so the unbound verdict
/// defers the junk claim's grief object — `t12_aheld_held_court`'s device; the fold then opens the session at the named leaf).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    T12,
    Opening,
}

fn court_extras(p: &Params, daa: u64, rule: Rule) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    let ladder =
        kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(&bundle(p).court, p.palw_court_ladder_active_at(daa));
    e.shard_court_ladder = p.palw_shard_court_active_at(daa).then_some(ladder);
    e.held_context_ladder = p.palw_held_context_active_at(daa).then_some(ladder);
    e.offence_attribution_active = p.palw_offence_attribution_active_at(daa);
    if rule == Rule::Opening {
        e.audit_2026_09_23_active = false;
        e.settled_anchor_depth = None;
    }
    e
}

struct Held {
    p: Params,
    sp: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
}

impl Held {
    fn fold(&self, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, rule: Rule) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let f = flags(&self.p, daa);
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            &self.s,
            &self.sp,
            None,
            &ctx(0x0614_0000 + daa, daa, daa, if matches!(work, PalwBlockWorkV3::Attempt(_)) { T12_BLOCK_SUBSIDY_SOMPI } else { 0 }),
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

    /// One block: folded, its carriage reloaded under its committed root, committed.
    fn step(&mut self, objects: &[PalwConsensusObjectV2], rule: Rule) -> Result<(), PalwStateV2Error> {
        self.step_work(objects, PalwBlockWorkV3::None, Hash64::default(), rule)
    }

    fn step_work(&mut self, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, rule: Rule) -> Result<(), PalwStateV2Error> {
        let daa = self.daa + 1;
        let next = self.fold(daa, objects, work, key, rule)?;
        let reloaded = PalwStateCarriageV2::from_state(&next).into_state(&self.sp, Some(next.state_root())).expect("reloads");
        assert_eq!(reloaded, next, "DAA {daa}: the carriage reloads to the state");
        self.s = next;
        self.daa = daa;
        Ok(())
    }

    fn skip_to(&mut self, daa: u64) {
        assert!(daa > self.daa);
        self.daa = daa - 1;
        self.step(&[], Rule::T12).unwrap_or_else(|e| panic!("an empty block at {daa}: {e}"));
    }
}

/// testnet-12's genesis 8k held row.
struct HeldRow {
    id: Hash64,
    profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    target: u128,
    leaves: u64,
}

fn eight_k(p: &Params) -> HeldRow {
    let rows = genesis_classes(p);
    bundle(p)
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
                if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) && c.profile.n_ctx == 8_192 =>
            {
                let (_, leaves, target, _) = *rows.iter().find(|r| r.0 == *class_id).expect("a genesis row");
                Some(HeldRow { id: *class_id, profile: c.profile.clone(), job: c.canonical.clone(), target, leaves })
            }
            _ => None,
        })
        .expect("testnet-12 registers the 8k held row")
}

/// testnet-12 (launch) at DAA 1,000 with the 8k row `Active`, the executor, the bystander, the outsider and `sybils` Sybil bonds
/// registered — every one after genesis; LG14-A armed at genesis when `armed` (validated with its record).
fn held_chain(armed: bool, sybils: u64) -> (Held, HeldRow, Vec<(PalwBondKeyV2, Hash64)>) {
    let p = if armed { arm_legacy(palw_t12_launch_params_v1(), 0, false, false).0 } else { palw_t12_launch_params_v1() };
    let b = bundle(&p);
    let sp = b.state.clone();
    assert_eq!(sp.legacy_public_filer_from_daa(), armed.then_some(0), "the mirror the fold reads, synced by the Params' own sync");
    let row = eight_k(&p);
    assert!(!sp.held_class_is_unanswerable_v1(&row.id), "the 8k row is answerable");
    let seats: Vec<(PalwBondKeyV2, Hash64)> =
        genesis_bonds(&p)[..b.panel.seat_count() as usize].iter().map(|(k, o, _)| (*k, *o)).collect();
    let mut s = genesis_state(&p);
    let mut c = PalwStateCarriageV2::from_state(&s);
    c.model_lifecycles.get_mut(&row.id).expect("the row").state = kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active;
    s = c.into_state_v3(&sp, None, flags(&p, 0).uncertified_weightless, p.palw_canonical_work_daa()).expect("a consistent carriage");
    let mut chain = Held { p, sp, s, daa: 999 };
    let mut bonds = vec![bond_obj(EXECUTOR, RICH), bond_obj(BYSTANDER, RICH), bond_obj(OUTSIDER, RICH)];
    bonds.extend((0..sybils).map(|n| bond_obj(SYBIL_BASE + n, RICH)));
    for chunk in bonds.chunks(16) {
        chain.step(chunk, Rule::T12).expect("the bonds register");
    }
    (chain, row, seats)
}

/// A licensed junk attempt of the executor on the 8k row: attempt, panel, every seat `Valid`.
fn licensed(c: &mut Held, row: &HeldRow, seats: &[(PalwBondKeyV2, Hash64)], nonce: u64) -> (Hash64, u64) {
    let pwu = match c.s.palw_canonical_per_draw_v1(&row.id, c.daa + 1, c.p.palw_canonical_work_daa()) {
        Some(work) => kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(row.target, work),
        None => palw_pwu_v1(row.target, row.leaves),
    };
    let (env, key, claim_id) =
        junk_attempt(row.id, bond_key(EXECUTOR), pubkey_of(EXECUTOR), &operator_pubkey_of(EXECUTOR), pwu, nonce, 0x0614_A000 + nonce);
    c.step_work(&[], PalwBlockWorkV3::Attempt(&env), key, Rule::T12).expect("the attempt folds (option (b) is not armed here)");
    c.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x0614_AA00 + nonce), seats: seats_of(seats) }], Rule::T12)
        .expect("the panel binds");
    let keys: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &keys) }], Rule::T12)
        .expect("every seat signs Valid");
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = c.s.claim(&claim_id).unwrap().phase else { panic!("licensed") };
    (claim_id, licensed_daa)
}

/// `accuser`'s held accusation at layer 0's fused site, `back` positions before the last prefill position (the unbound verdict
/// defers it under [`Rule::Opening`]; the session opens at the named leaf, its id binding the challenger).
fn accusation_by(c: &Held, row: &HeldRow, claim_id: Hash64, accuser: PalwBondKeyV2, back: u32) -> PalwConsensusObjectV2 {
    let claim = c.s.claim(&claim_id).expect("claim").clone();
    let profile = row.profile.clone();
    let mut job = row.job.clone();
    job.job_id = h(0x0614_0001);
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

fn open_by(c: &mut Held, row: &HeldRow, claim_id: Hash64, accuser: PalwBondKeyV2, back: u32) -> Result<(), PalwStateV2Error> {
    let object = accusation_by(c, row, claim_id, accuser, back);
    c.step(&[object], Rule::Opening)
}

// ---- G-06 ----------------------------------------------------------------------------------------------------------------------

/// **G-06: the outside reserver is neither outrun nor pre-empted.** The producer and every seat collude (all `Valid`); the producer's
/// Sybil (a non-seat bystander) opens a decoy held dissection first. Past LG14-A the outsider — registered after genesis, never a seat
/// — reserves the claim and opens its own dissection beside the decoy (one per reserver), the seats keep their full room, and the
/// claim does not reach `Final` past its floor while the reservation holds it. The unarmed twin is int-12's rule: the reservation is
/// refused (not in force) and the outsider's dissection waits behind the decoy ("only a seat").
#[test]
fn g06_a_reserver_opens_its_dissection_beside_a_decoy_court_and_holds_final() {
    for armed in [true, false] {
        let (mut c, row, seats) = held_chain(armed, 1);
        let (claim, licensed_daa) = licensed(&mut c, &row, &seats, 1);
        let floor = licensed_daa + c.sp.window_challenge_at(licensed_daa);
        let reserve = reservation(&c.s, claim, bond_key(OUTSIDER));
        if !armed {
            // int-12's rule: the decoy opens, the outsider's dissection waits behind it, and the reservation does not exist.
            open_by(&mut c, &row, claim, bond_key(BYSTANDER), 1).expect("the decoy opens");
            let waits = open_by(&mut c, &row, claim, bond_key(OUTSIDER), 1).expect_err("a non-seat waits behind the decoy");
            assert!(refused_with(&waits, "only a seat"), "{waits:?}");
            let refused = c.step(&[reserve], Rule::T12).expect_err("below the fence the reservation is refused by name");
            assert!(refused_with(&refused, "not in force"), "{refused:?}");
            continue;
        }
        // 1. The outsider reserves while the claim is licensed: the claim owes no deadline, and is not Final past its floor.
        c.step(&[reserve], Rule::T12).expect("the outsider reserves the claim");
        assert!(c.s.legacy_dispute_reservation_v1(&claim, &bond_key(OUTSIDER)).is_some());
        assert_eq!(c.s.deadline_of(&claim), None, "held: the claim owes no deadline");
        c.skip_to(floor + 5);
        assert!(matches!(c.s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not Final past its floor");
        // 2. The decoy: the producer's Sybil opens first, at the lie's own leaf.
        open_by(&mut c, &row, claim, bond_key(BYSTANDER), 1).expect("the decoy opens");
        assert_eq!(c.s.court_sessions_for_claim(&claim), 1);
        // A non-seat WITHOUT a reservation still waits behind it (int-12's C3 rule, unchanged).
        let waits = open_by(&mut c, &row, claim, bond_key(SYBIL_BASE), 1).expect_err("no reservation, no further dissection");
        assert!(refused_with(&waits, "only a seat"), "{waits:?}");
        // 3. G-06: past LG14-A the reserver opens its own dissection beside the decoy — one per reserver.
        open_by(&mut c, &row, claim, bond_key(OUTSIDER), 1).expect("the reserver is not pre-empted by the decoy court");
        assert_eq!(c.s.court_sessions_for_claim(&claim), 2, "the decoy and the reserver's own");
        let again = open_by(&mut c, &row, claim, bond_key(OUTSIDER), 2).expect_err("one per reserver");
        assert!(refused_with(&again, "already holds a session"), "{again:?}");
        // 4. The seats keep their full room: one each beside the decoy AND the reserver.
        for (i, (seat, _)) in seats.iter().enumerate() {
            open_by(&mut c, &row, claim, *seat, 1).unwrap_or_else(|e| panic!("seat {i} beside the reserver: {e}"));
        }
        assert_eq!(c.s.court_sessions_for_claim(&claim), 2 + seats.len(), "decoy + reserver + one per seat");
        assert!(matches!(c.s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    }
}

// ---- G-07 ----------------------------------------------------------------------------------------------------------------------

/// **G-07: the coalition's budgets are not the outsider's.** A coalition exhausts DA-8's shared non-seat budget on the claim and
/// takes 64 live reservations on it; the outsider, registered after every coalition bond, still reserves and opens a reserved DA
/// session on its own budget (34 per reservation), counted on its reservation and on neither shared counter.
#[test]
fn g07_a_coalition_cannot_spend_the_outsiders_budget() {
    let (p, _) = arm_legacy(palw_t12_launch_params_v1(), 0, true, true);
    let mut c = Chain::new(p);
    let first = 0x0614_2000;
    let coalition = 64u64;
    for chunk in (first..first + coalition).collect::<Vec<_>>().chunks(16) {
        c.step(&chunk.iter().map(|n| bond_obj(*n, RICH)).collect::<Vec<_>>());
    }
    c.step(&[bond_obj(OUTSIDER, RICH)]);
    // Option (b) is armed at genesis and the route is armed: the V2 Attempt is admitted.
    assert_eq!(palw_g14_route_attempt_refusal_v1(&c.sp, c.daa + 1), None);
    let id = c.floor_claim(0x0614_07);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    // The coalition: DA-8's shared budget spent, and 64 live reservations on the claim.
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_claims.insert(
            id,
            kaspa_consensus_core::palw_da_rcore_v1::PalwDaClaimV1 { opened_non_seat_total: 16, ..Default::default() },
        );
    });
    for n in first..first + coalition {
        c.step(&[reservation(&c.s, id, bond_key(n))]);
    }
    assert_eq!(c.s.legacy_dispute_v1(&id).unwrap().live.len(), coalition as usize);
    // The outsider: its reservation and its reserved session are admitted on its own budget.
    c.step(&[reservation(&c.s, id, bond_key(OUTSIDER))]);
    c.step(&[da_accuse(id, bond_key(OUTSIDER), 0)]);
    let session = c.s.da_session(&id, &bond_key(OUTSIDER)).expect("the reserved session is open").clone();
    assert!(session.accuser_is_seat, "seat-like: it pauses the claim (DA-5)");
    assert_eq!(c.s.legacy_dispute_reservation_v1(&id, &bond_key(OUTSIDER)).unwrap().sessions_opened, 1, "on its reservation");
    assert_eq!(c.s.da_claim(&id).unwrap().opened_non_seat_total, 16, "the shared budget is not touched");
    // The unanswered reserved session ends in the objective DA default (never a computation fault), refunding every deposit.
    let deadline = session.deadline_daa;
    c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(
        c.claim(&id).phase,
        PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::ProducerWithholding, .. }
    ));
    assert!(c.s.legacy_dispute_v1(&id).is_none());
    assert_eq!(kaspa_consensus_core::palw_state_v2::palw_accuser_exposure_v1(&c.s, &bond_key(OUTSIDER)), 0);
}

/// **G-07's residual, pinned (NOT closed):** the network's court-session cap (`palw_max_concurrent_court_sessions_v1`: the rung window
/// times the one court move a block admits) is shared. A coalition of reservers fills it with decoy dissections on a claim of its own;
/// the outsider's dissection on the lying claim is then refused for capacity — while its reservation still holds that claim's `Final`.
/// A per-bond budget or a partition does not stop a multi-bond coalition, and lifting the cap breaks the slot guarantee that keeps an
/// honest mover from defaulting; closing it needs a measured per-block court budget or a priced slot (audit appendix C).
#[test]
fn g07_residual_the_network_court_cap_is_shared() {
    let probe = bundle(&palw_t12_launch_params_v1()).state;
    let cap = palw_max_concurrent_court_sessions_v1(probe.turn_deadline_daa());
    eprintln!("[G-07] testnet-12 court-session cap: {cap} (turn_deadline_daa {})", probe.turn_deadline_daa());
    let (mut c, row, seats) = held_chain(true, cap);
    // The lying claim: reserved by the outsider and held past its floor (no court is open, so no rung clock runs).
    let (target, licensed_daa) = licensed(&mut c, &row, &seats, 3);
    c.step(&[reservation(&c.s, target, bond_key(OUTSIDER))], Rule::T12).expect("the outsider reserves the lying claim");
    c.skip_to(licensed_daa + c.sp.window_challenge_at(licensed_daa) + 5);
    assert!(matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "held: no Final, no reward");
    // The coalition's own claim; one decoy opener, then reservers stacking dissections on it up to the network cap (inside one rung
    // window: the passive responder's sessions would otherwise default — a live coalition plays both sides and keeps them open).
    let (decoy, _) = licensed(&mut c, &row, &seats, 2);
    let sybils: Vec<PalwBondKeyV2> = (0..cap - 1).map(|n| bond_key(SYBIL_BASE + n)).collect();
    let reservations: Vec<PalwConsensusObjectV2> = sybils.iter().map(|sybil| reservation(&c.s, decoy, *sybil)).collect();
    c.step(&reservations, Rule::T12).expect("the Sybils reserve the decoy claim");
    open_by(&mut c, &row, decoy, bond_key(BYSTANDER), 1).expect("the first decoy");
    let openings: Vec<PalwConsensusObjectV2> = sybils.iter().map(|sybil| accusation_by(&c, &row, decoy, *sybil, 1)).collect();
    c.step(&openings, Rule::Opening).expect("every Sybil reserver's decoy dissection");
    assert_eq!(c.s.court_sessions_iter().count() as u64, cap, "the network's court sessions are at the cap");
    let full = open_by(&mut c, &row, target, bond_key(OUTSIDER), 1).expect_err("the cap is full");
    assert!(matches!(full, PalwStateV2Error::CourtSessionsAtCapacity { .. }), "{full:?}");
    // The reservation still holds the lying claim — the residual is a delay bounded by its hard deadline, not a payout.
    assert!(matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert!(c.s.legacy_dispute_reservation_v1(&target, &bond_key(OUTSIDER)).is_some());
}

// ---- G-13 -----------------------------------------------------------------------------------------------------------------------

/// testnet-12 (launch) with lane V02's short lock life in force from genesis (as on testnet-12 since its post-launch release) and,
/// when `armed`, LG14-A (validated with its record).
fn lock_chain(armed: bool) -> Chain {
    let mut p = palw_t12_launch_params_v1();
    p.palw_final_lock_life = Some(ForkActivation::new(0));
    p.sync_palw_final_lock_life();
    let p = if armed { arm_legacy(p, 0, false, false).0 } else { p };
    Chain::new(p)
}

fn licensed_floor_claim(c: &mut Chain, seed: u64) -> (Hash64, Vec<(PalwBondKeyV2, Hash64)>) {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    (id, seats)
}

/// **G-13: a relied-on `Valid` lock lives to the claim's liability horizon.** On a claim accepted past LG14-A every seat's lock is
/// dated `F + window_court` at `Final` — the liability row's own expiry, the window in which a conviction may still reverse the
/// `Final` and the public filer may still reserve — so between `F + 1,000` and `F + window_court` it is committed (the seat cannot
/// exit) and slashable. Below the fence (the twin) lane V02's `F + 1,000` stands, byte for byte. A kind-3 conviction that would
/// find a relied-on lock gone is refused (`palw_g13_relied_lock_lost_v1`, read by the fold's conviction funnel).
#[test]
fn g13_a_valid_lock_lives_to_the_liability_horizon() {
    let mut results = Vec::new();
    for armed in [true, false] {
        let mut c = lock_chain(armed);
        let (id, seats) = licensed_floor_claim(&mut c, 0x0614_13 + u64::from(armed));
        c.finalize(id);
        let PalwClaimPhaseV2::Final { final_daa } = c.claim(&id).phase else { panic!("Final") };
        let wc = c.sp.window_court();
        let row = c.s.panel_liability(&id).expect("the liability row").clone();
        assert_eq!(row.expiry_daa, final_daa + wc, "the liability horizon");
        assert!(row.locked_sompi > 0, "the row was built from the licence's locks");
        let short = c.sp.final_lock_life_at(final_daa);
        assert!(short < wc, "lane V02's life ({short}) is shorter than window_court ({wc}) on testnet-12");
        let mid = final_daa + short + (wc - short) / 2;
        for (seat, _) in &seats {
            let lock = *c.s.slashable_lock(*seat, id).expect("each Valid seat locked");
            if armed {
                assert_eq!(lock.expiry_daa, final_daa + wc, "G-13: the lock lives to the liability horizon");
            } else {
                assert_eq!(lock.expiry_daa, final_daa + short, "below the fence: lane V02's life, byte for byte");
            }
            assert_eq!(palw_lock_is_committed_v1(&c.s, &id, &lock, mid, None, wc), armed, "committed at F + {}", mid - final_daa);
            assert_eq!(palw_false_valid_lock_slashable_v1(&c.s, &c.sp, &id, &lock, mid, None), armed, "slashable at the midpoint");
            assert!(!palw_g13_relied_lock_lost_v1(&c.s, &c.sp, &id, seat, mid, None), "nothing lost inside the horizon");
        }
        results.push((armed, final_daa));
        if !armed {
            continue;
        }
        // The predicate the conviction funnel reads: past the horizon the lock is no longer slashable, so a conviction there would
        // collect nothing from a relied-on seat — refused, never recorded as a conviction that took nothing.
        let (seat, _) = seats[0];
        assert!(palw_g13_relied_lock_lost_v1(&c.s, &c.sp, &id, &seat, final_daa + wc + 1, None));
        // A relied-on seat whose lock is gone while the row (built from locks) lists it: refused too.
        let mut carriage = PalwStateCarriageV2::from_state(&c.s);
        carriage.slashable_locks.remove(&(seat, id));
        let lost = carriage.into_state(&c.sp, None).expect("a state without one lock");
        assert!(palw_g13_relied_lock_lost_v1(&lost, &c.sp, &id, &seat, mid, None));
        // A bond the chain never relied on (no lock, not listed) is not this rule's.
        assert!(!palw_g13_relied_lock_lost_v1(&c.s, &c.sp, &id, &bond_key(OUTSIDER), mid, None));
        // The fold's conviction funnel reads this predicate (the funnel is reached only with an admissible contradiction, which the
        // node E2E `legacy_v2_attempt_full_collusion_outside_bond_convicts` files for every Valid seat).
        let funnel = include_str!("../src/palw_state_v2/transition_builder.rs");
        assert!(funnel.contains("if palw_g13_relied_lock_lost_v1(&self.state, self.params, &claim_id, &accused, now, self.second_clock_depth(now))"));
    }
    eprintln!("[G-13] (armed, F) = {results:?}");
}

// ---- G14 from genesis -------------------------------------------------------------------------------------------------------------

/// testnet-12's launch ruleset on an isolated G14 genesis with LG14-A, LG14-B, option (b), the reward liability horizon and lane
/// V02's lock life all at DAA 0 — G14 from genesis (user decision 2026-10-16).
fn genesis_ruleset(held: bool) -> Params {
    let mut p = palw_t12_launch_params_v1();
    p.genesis = palw_isolated_g14_genesis_v1(&p.genesis, Hash64::from_u64_word(0x0614_6E45_0000_0001)).expect("isolated");
    let at = Some(ForkActivation::new(0));
    p.palw_final_lock_life = at;
    p.sync_palw_final_lock_life();
    p.palw_legacy_public_filer_v1 = at;
    p.sync_palw_legacy_public_filer_v1();
    if held {
        p.palw_legacy_held_da_v2 = at;
    }
    p.palw_g14_route_required_v1 = at;
    p.palw_reward_liability_horizon_v1 = at;
    p.sync_palw_g14_gates_v1();
    assert!(p.has_isolated_g14_genesis_v1());
    p.validate_palw_v2().unwrap_or_else(|e| panic!("G14-from-genesis validates (isolated genesis, no record needed): {e:?}"));
    p
}

/// **Every fence of the legacy route at DAA 0**: a V2 Attempt is admitted from the first block (its G14 route is armed), an outside
/// bond registered after genesis reserves it and holds its `Final` past the floor, and when the hold ends the claim's `Valid` locks
/// are dated to the liability horizon. The same genesis without LG14-B (the gates lane's g14-genesis form) refuses the attempt.
#[test]
fn g14_close_legacy_from_genesis() {
    let mut c = Chain::new(genesis_ruleset(true));
    assert_eq!(palw_g14_route_attempt_refusal_v1(&c.sp, 0), None, "routed at block 0");
    c.step(&[bond_obj(OUTSIDER, RICH)]);
    let (id, seats) = licensed_floor_claim(&mut c, 0x0614_6E);
    let reserver = bond_key(OUTSIDER);
    let floor = c.s.deadline_of(&id).expect("the licensed floor");
    c.step(&[reservation(&c.s, id, reserver)]);
    assert_eq!(c.s.deadline_of(&id), None, "held");
    c.step_at(floor + 5, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "Final held past the floor");
    c.step(&[PalwConsensusObjectV2::DisputeReleasedV1 { claim: id, reserver, signature: Vec::new() }]);
    let resumed = c.s.deadline_of(&id).expect("the hold ended: DL-1 re-arms");
    c.step_at(resumed.max(c.daa) + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let PalwClaimPhaseV2::Final { final_daa } = c.claim(&id).phase else { panic!("Final once the hold ends: {:?}", c.claim(&id).phase) };
    // The hold re-dated every lock to the reservation's `hard + window_challenge` (DA-5 / L-3, only ever raised), and Final keeps
    // it: never below the liability horizon `F + window_court` (G-13), never lane V02's `F + 1,000`.
    let horizon = final_daa + c.sp.window_court();
    assert_eq!(c.s.panel_liability(&id).expect("the row").expiry_daa, horizon);
    for (seat, _) in &seats {
        let expiry = c.s.slashable_lock(*seat, id).unwrap().expiry_daa;
        assert!(expiry >= horizon, "G-13 from genesis: lock {expiry} reaches the horizon {horizon}");
        assert!(expiry > final_daa + c.sp.final_lock_life_at(final_daa));
    }
    // Without LG14-B the same genesis refuses the attempt (option (b): no armed route for the legacy Panel family).
    let half = Chain::new(genesis_ruleset(false));
    assert!(palw_g14_route_attempt_refusal_v1(&half.sp, 0).is_some());
    let (floor_class, _, _, _) = genesis_classes(&half.p)[0];
    let (producer, pubkey, operator) = floor_producer(&half.p);
    let pwu = half.floor_pwu(half.daa + 1);
    let (env, key, _) = junk_attempt(floor_class, producer, pubkey, &operator, pwu, 0x0614_6F, 0x10C0 + 0x0614_6F);
    let x = ctx(0xCA_0000 + half.daa + 1, half.daa + 1, half.daa + 1, T12_BLOCK_SUBSIDY_SOMPI);
    let refused = half.try_fold(&half.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key).map(|_| ()).expect_err("no route");
    assert!(refused_with(&refused, PALW_G14_ROUTE_ABSENT_CODE_V1), "{refused:?}");
}

// ---- testnet-12's identity -------------------------------------------------------------------------------------------------------

/// **testnet-12's identity does not move**: its launch ids are the pinned ones, its shipped ids keep their prefixes (appendix B), every
/// shipped preset leaves both legacy fences dormant, and testnet-12 as shipped validates.
#[test]
fn testnet12_identity_did_not_move() {
    let launch = palw_t12_launch_params_v1();
    assert_eq!(
        (launch.consensus_params_id().to_string(), launch.consensus_identity_id().to_string(), launch.consensus_schedule_id().to_string()),
        (
            "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f".to_string(),
            "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5".to_string(),
            "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd".to_string(),
        )
    );
    let t12 = palw_t12_shipped_params();
    let (params, identity, schedule) =
        (t12.consensus_params_id().to_string(), t12.consensus_identity_id().to_string(), t12.consensus_schedule_id().to_string());
    eprintln!("[t12 ids] params {params} identity {identity} schedule {schedule}");
    assert!(params.starts_with("5ee7fd8e") && identity.starts_with("5de80e64") && schedule.starts_with("1678e073"));
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    for p in [launch, t12] {
        assert_eq!((p.palw_legacy_public_filer_v1, p.palw_legacy_held_da_v2), (None, None));
        let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!("V2") };
        assert_eq!(b.state.legacy_public_filer_from_daa(), None);
        assert_eq!(b.state.g14_route_required_v1(), None);
    }
}
