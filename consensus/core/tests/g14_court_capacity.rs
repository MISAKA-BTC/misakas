//! **Lane g14-court-capacity (2026-10-16): the court's 42 slots (audit G-07)** — the fold half of the user's direction
//! (`docs/design/palw/t12-daa14000-release-plan-2026-10-15.md`, "court の全体容量と Panel 担保" §1, §3).
//!
//! * **Arming**: `palw_court_capacity_v1` validates only with LG14-A at or below it, a sound policy, a record naming the network and
//!   height — and never with the placeholder policy (its values are measurements). The placeholder reproduces today's 42 slots.
//! * **42-slot saturation**: a coalition of reservers holds every court slot; a 43rd, honest, independent prosecution QUEUES under its
//!   worst-case guarantee instead of being refused, is granted the first slot that frees — before the coalition, which tries to
//!   re-occupy every freed slot (the continuous refill) — opens, and reaches its verdict inside its claim's hard deadline. Its
//!   occupancy is reimbursed by the convicted producer.
//! * **Bond splitting** does not avoid the occupancy fee: the fee is ρ × occupied DAA × weight per session, whoever holds it.
//! * **The block budget** holds at maximum load (work units and evidence bytes), in the fold.
//! * **Restart and reorg**: every block's delta re-applies and reverts, its carriage reloads under its root, and a node restarted
//!   mid-queue continues to the same reservations, deadlines, fees and releases.
//! * **Option (b)**: a fused-leaf class is not counted as routed until the court-capacity fence is in force.
//!
//! Policy values here are TEST FIXTURES (marked `TEST`), never a network's: the fence refuses every real height until a measured record
//! is pinned. Every fence is armed on the `Params` and mirrored by its own `sync_*`, and each ruleset passes the whole validation with a
//! test record (no mirror-only bypass, audit G-19).
//!
//! Run: cargo test -p kaspa-consensus-core --test g14_court_capacity -j 4 -- --test-threads=2

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_launch_params_v1, palw_t12_shipped_params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::palw_court_capacity_v1::*;
use kaspa_consensus_core::palw_g14_gates_v1::*;
use kaspa_consensus_core::palw_legacy_public_filer_v1::{PALW_DISPUTE_RESERVATION_VERSION_V1, PalwDisputeReservationV1};
use kaspa_consensus_core::palw_shard_court_v1::{PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwStateV2Error, PalwVoidReasonV2, palw_accuser_exposure_v1, palw_max_concurrent_court_sessions_v1,
};
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;

const RELEASE: u64 = 14_000;
const OUTSIDER: u64 = 0x0707_0001;
const EXECUTOR: u64 = 0x0707_00E0;
/// The coalition's own producer: its claims carry the decoy courts.
const COALITION_PRODUCER: u64 = 0x0707_00E1;
const SYBIL_BASE: u64 = 0x0707_1000;

// ---- policies (TEST fixtures) and arming -----------------------------------------------------------------------------------------

/// **A TEST measurement record (not a measurement of any node):** classes timed at `units × 1 µs` (`PalwCourtOpClassV1` order),
/// 20,000 B and a 20,000 B working set each, a block allotment of `budget × 1 µs` and `block_bytes`, ρ_court 1,000 sompi.
fn measurement(units: [u64; 4], budget: u64, block_bytes: u64) -> PalwCourtMeasurementV1 {
    PalwCourtMeasurementV1 {
        version: PALW_COURT_MEASUREMENT_VERSION_V1,
        class_ns: units.map(|u| u * 1_000),
        class_bytes: [20_000; 4],
        class_memory: [20_000; 4],
        block_court_ns: budget * 1_000,
        block_court_bytes: block_bytes,
        block_court_memory: u64::MAX / 4,
        rho_sompi: 1_000,
        ticket_window_daa: 0,
    }
}

/// **The TEST measurement** ([`measurement`]): a one-move accusation 1 unit, every other court operation 42, a block allotment of 42
/// units — so a block takes 42 one-move openings but one dissection move, and the derived capacity is ⌊42 / 42⌋ × the rung window:
/// testnet-12's 42 slots. ρ_court 1,000 sompi per occupied DAA per weight unit (weight 42, the heaviest operation).
fn test_measurement() -> PalwCourtMeasurementV1 {
    measurement([1, 42, 42, 42], 42, u64::MAX / 4)
}

fn test_policy() -> PalwCourtCapacityPolicyV1 {
    test_measurement().policy().expect("the TEST measurement derives a policy")
}

fn record(fence: PalwG14GatedFenceV1, genesis: Hash64, at: u64) -> PalwG14ArmingRecordV1 {
    record_with(fence, genesis, at, None)
}

/// A test record; with `measured`, its measured-values evidence is that measurement record's digest.
fn record_with(fence: PalwG14GatedFenceV1, genesis: Hash64, at: u64, measured: Option<&PalwCourtMeasurementV1>) -> PalwG14ArmingRecordV1 {
    let mut evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x70 + i as u8; 32]).collect();
    if let Some(m) = measured {
        evidence[PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1] = m.digest();
    }
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: at,
        source_commit: [0x07; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

/// `p` with LG14-A and (with `measured`) the court-capacity fence at `at` with the policy that measurement record derives, mirrors
/// synced, and its records (the court-capacity record carrying the measurement's digest); asserted to pass the whole validation with
/// them and to be refused without them.
fn arm(mut p: Params, at: u64, measured: Option<PalwCourtMeasurementV1>) -> (Params, Vec<PalwG14ArmingRecordV1>) {
    let fence = ForkActivation::new(at);
    p.palw_legacy_public_filer_v1 = Some(fence);
    p.sync_palw_legacy_public_filer_v1();
    let mut records = vec![record(PalwG14GatedFenceV1::LegacyPublicFilerV1, p.genesis.hash, at)];
    if let Some(m) = measured {
        p.palw_court_capacity_v1 = Some(PalwCourtCapacityFenceV1::measured_at(fence, m).expect("the measurement derives a policy"));
        p.sync_palw_court_capacity_v1();
        records.push(record_with(PalwG14GatedFenceV1::CourtCapacityV1, p.genesis.hash, at, Some(&m)));
    }
    p.sync_palw_g14_gates_v1();
    p.validate_palw_v2_with_arming_records_v1(&records).unwrap_or_else(|e| panic!("validates with its records: {e:?}"));
    assert!(p.validate_palw_v2().is_err(), "nothing is pinned in this binary");
    (p, records)
}

fn refused_with(e: &PalwStateV2Error, want: &str) -> bool {
    e.to_string().contains(want) || format!("{e:?}").contains(want)
}

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

fn release(claim: Hash64, reserver: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DisputeReleasedV1 { claim, reserver, signature: Vec::new() }
}

fn sybil(n: u64) -> PalwBondKeyV2 {
    bond_key(SYBIL_BASE + n)
}

// ---- the held-row fixture, with every block checked for restart and reorg ---------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    T12,
    /// The block that OPENS a held dissection (the 2026-09-23 fence off so the unbound verdict defers the junk claim's grief object —
    /// `t12_aheld_held_court`'s device).
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

#[derive(Clone)]
struct Held {
    p: Params,
    sp: PalwStateParamsV2,
    s: PalwChainStateV2,
    daa: u64,
}

impl Held {
    fn try_fold(&self, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, rule: Rule) -> Result<(PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2), PalwStateV2Error> {
        let f = flags(&self.p, daa);
        kaspa_consensus_core::palw_state_v2::apply_palw_transition_v7(
            &self.s,
            &self.sp,
            None,
            &ctx(0x0707_0000 + daa, daa, daa, if matches!(work, PalwBlockWorkV3::Attempt(_)) { T12_BLOCK_SUBSIDY_SOMPI } else { 0 }),
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
        .map(|(s, delta, skips)| {
            assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
            (s, delta)
        })
    }

    /// One block: folded; its delta re-applies to the child and reverts to the parent (a reorg across it), and its carriage reloads
    /// under its committed root (a restart / an IBD import of it); committed.
    fn step_work(&mut self, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, rule: Rule) -> Result<(), PalwStateV2Error> {
        let daa = self.daa + 1;
        let (next, delta) = self.try_fold(daa, objects, work, key, rule)?;
        assert_eq!(apply_delta_v2(&self.s, &delta, &self.sp).expect("re-applies"), next, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&next, &delta, &self.sp).expect("reverts"), self.s, "DAA {daa}: the delta reverts (reorg)");
        let reloaded = PalwStateCarriageV2::from_state(&next).into_state(&self.sp, Some(next.state_root())).expect("reloads");
        assert_eq!(reloaded, next, "DAA {daa}: the carriage reloads to the state (restart)");
        self.s = next;
        self.daa = daa;
        Ok(())
    }

    fn step(&mut self, objects: &[PalwConsensusObjectV2], rule: Rule) -> Result<(), PalwStateV2Error> {
        self.step_work(objects, PalwBlockWorkV3::None, Hash64::default(), rule)
    }

    fn beat(&mut self) {
        self.step(&[], Rule::T12).unwrap_or_else(|e| panic!("an empty block at {}: {e}", self.daa + 1));
    }

    fn collateral(&self, bond: &PalwBondKeyV2) -> u64 {
        self.s.bond(bond).expect("a bond").collateral
    }

    fn court_row(&self, claim: &Hash64, bond: &PalwBondKeyV2) -> Option<PalwReservedCourtV1> {
        self.s.legacy_dispute_v1(claim).and_then(|record| record.courts.get(bond).copied())
    }
}

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

/// testnet-12 (launch) with LG14-A and the court-capacity fence (with `policy`) armed at genesis and validated with records; the 8k row
/// `Active`; the executor, the coalition's producer, the outsider and `sybils` Sybil bonds registered after genesis.
fn held_chain(measured: Option<PalwCourtMeasurementV1>, sybils: u64) -> (Held, HeldRow, Vec<(PalwBondKeyV2, Hash64)>) {
    let policy = measured.map(|m| m.policy().expect("a policy"));
    let (p, _) = arm(palw_t12_launch_params_v1(), 0, measured);
    let b = bundle(&p);
    let sp = b.state.clone();
    assert_eq!(sp.court_capacity_v1().map(|m| m.policy), policy, "the mirror the fold reads, synced by the Params' own sync");
    let row = eight_k(&p);
    let seats: Vec<(PalwBondKeyV2, Hash64)> =
        genesis_bonds(&p)[..b.panel.seat_count() as usize].iter().map(|(k, o, _)| (*k, *o)).collect();
    let mut s = genesis_state(&p);
    let mut c = PalwStateCarriageV2::from_state(&s);
    c.model_lifecycles.get_mut(&row.id).expect("the row").state = kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active;
    s = c.into_state_v3(&sp, None, flags(&p, 0).uncertified_weightless, p.palw_canonical_work_daa()).expect("a consistent carriage");
    let mut chain = Held { p, sp, s, daa: 999 };
    let mut bonds = vec![bond_obj(EXECUTOR, RICH), bond_obj(COALITION_PRODUCER, RICH), bond_obj(OUTSIDER, RICH)];
    bonds.extend((0..sybils).map(|n| bond_obj(SYBIL_BASE + n, RICH)));
    for chunk in bonds.chunks(16) {
        chain.step(chunk, Rule::T12).expect("the bonds register");
    }
    (chain, row, seats)
}

/// A licensed junk attempt of `producer` on the 8k row: attempt, panel, every seat `Valid`.
fn licensed(c: &mut Held, row: &HeldRow, seats: &[(PalwBondKeyV2, Hash64)], producer: u64, nonce: u64) -> (Hash64, u64) {
    let pwu = match c.s.palw_canonical_per_draw_v1(&row.id, c.daa + 1, c.p.palw_canonical_work_daa()) {
        Some(work) => kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(row.target, work),
        None => palw_pwu_v1(row.target, row.leaves),
    };
    let (env, key, claim_id) =
        junk_attempt(row.id, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), pwu, nonce, 0x0707_A000 + nonce);
    c.step_work(&[], PalwBlockWorkV3::Attempt(&env), key, Rule::T12).expect("the attempt folds");
    c.step(&[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x0707_AA00 + nonce), seats: seats_of(seats) }], Rule::T12)
        .expect("the panel binds");
    let keys: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &keys) }], Rule::T12)
        .expect("every seat signs Valid");
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = c.s.claim(&claim_id).unwrap().phase else { panic!("licensed") };
    (claim_id, licensed_daa)
}

/// `accuser`'s held accusation at layer 0's fused site (the unbound verdict defers it under [`Rule::Opening`]; the session opens at the
/// named leaf, its id binding the challenger).
fn accusation_by(c: &Held, row: &HeldRow, claim_id: Hash64, accuser: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let claim = c.s.claim(&claim_id).expect("claim").clone();
    let profile = row.profile.clone();
    let mut job = row.job.clone();
    job.job_id = h(0x0707_0001);
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

// ---- arming ----------------------------------------------------------------------------------------------------------------------

/// **The fence is armable only behind evidence.** Refused without LG14-A at or below it, refused without a record for exactly this
/// network and height, refused with the placeholder policy even with a record; a measured (here TEST) policy with its record validates.
/// The placeholder reproduces today's capacity (42 on testnet-12) and prices nothing.
#[test]
fn court_capacity_arming_needs_a_measured_record() {
    let t12 = palw_t12_shipped_params();
    let turn = bundle(&t12).state.turn_deadline_daa();
    let placeholder = palw_court_capacity_placeholder_policy_v1();
    assert_eq!(placeholder.capacity_sessions(turn), palw_max_concurrent_court_sessions_v1(turn), "today's slots, derived alike");
    assert_eq!(test_policy().capacity_sessions(turn), 42);
    assert_eq!(placeholder.fee(1_000, 1), 0, "the placeholder prices nothing");
    assert!(PalwG14GatedFenceV1::CourtCapacityV1.needs_measured_value());
    assert_eq!(PalwG14GatedFenceV1::CourtCapacityV1.requirements(), PALW_COURT_CAPACITY_ARMING_REQUIREMENTS_V1);
    // Measured (TEST) policy with LG14-A and both records: validates.
    let (armed, records) = arm(t12.clone(), RELEASE, Some(test_measurement()));
    // The measurement record binds the policy: a policy it does not derive is refused; a record whose measured-values evidence is
    // not the measurement's digest is refused; no measurement record at all is refused.
    let mut drifted = armed.clone();
    let mut fence = drifted.palw_court_capacity_v1.unwrap();
    fence.policy.rho_sompi += 1;
    drifted.palw_court_capacity_v1 = Some(fence);
    drifted.sync_palw_court_capacity_v1();
    let why = drifted.validate_palw_v2_with_arming_records_v1(&records).expect_err("a policy its measurement does not derive");
    assert!(format!("{why:?}").contains("not the one its measurement record derives"), "{why:?}");
    let mut wrong_digest = records.clone();
    for r in wrong_digest.iter_mut().filter(|r| r.fence == PalwG14GatedFenceV1::CourtCapacityV1) {
        *r = record(PalwG14GatedFenceV1::CourtCapacityV1, t12.genesis.hash, RELEASE);
    }
    let why = armed.validate_palw_v2_with_arming_records_v1(&wrong_digest).expect_err("evidence is not the measurement's digest");
    assert!(format!("{why:?}").contains("measurement record's digest"), "{why:?}");
    let mut unmeasured = armed.clone();
    unmeasured.palw_court_capacity_v1 = Some(PalwCourtCapacityFenceV1 { measurement: None, ..armed.palw_court_capacity_v1.unwrap() });
    let why = unmeasured.validate_palw_v2_with_arming_records_v1(&records).expect_err("no measurement record");
    assert!(format!("{why:?}").contains("needs its measurement record"), "{why:?}");
    // The derivation: the cheapest class is the unit; an unmeasured class or an allotment that cannot take the heaviest class once is
    // refused.
    assert_eq!(test_policy().op_work, [1, 42, 42, 42]);
    assert_eq!(test_policy().block_budget.work, 42);
    assert!(measurement([1, 0, 42, 42], 42, u64::MAX / 4).policy().is_err(), "an unmeasured class");
    assert!(measurement([1, 43, 42, 42], 42, u64::MAX / 4).policy().is_err(), "the allotment cannot take the heaviest once");
    assert!(measurement([1, 42, 42, 42], 42, 10_000).policy().is_err(), "nor its bytes");
    // Without the court-capacity record: refused by name.
    let without: Vec<PalwG14ArmingRecordV1> =
        records.iter().copied().filter(|r| r.fence != PalwG14GatedFenceV1::CourtCapacityV1).collect();
    let why = armed.validate_palw_v2_with_arming_records_v1(&without).expect_err("no record");
    assert!(format!("{why:?}").contains("palw_court_capacity_v1"), "{why:?}");
    // The placeholder policy: refused even with its record.
    let mut placeholder_armed = armed.clone();
    placeholder_armed.palw_court_capacity_v1 = Some(PalwCourtCapacityFenceV1::placeholder_at(ForkActivation::new(RELEASE)));
    placeholder_armed.sync_palw_court_capacity_v1();
    placeholder_armed.sync_palw_g14_gates_v1();
    let why = placeholder_armed.validate_palw_v2_with_arming_records_v1(&records).expect_err("placeholder");
    assert!(format!("{why:?}").contains("placeholder policy"), "{why:?}");
    // Without LG14-A at or below it: refused by name.
    let mut no_filer = t12.clone();
    no_filer.palw_court_capacity_v1 = Some(PalwCourtCapacityFenceV1::measured_at(ForkActivation::new(RELEASE), test_measurement()).unwrap());
    no_filer.sync_palw_court_capacity_v1();
    let why = no_filer.validate_palw_court_capacity_in_v1(&records).expect_err("no LG14-A");
    assert!(format!("{why:?}").contains("needs palw_legacy_public_filer_v1"), "{why:?}");
    // An unsynced mirror is refused; never() is dormant.
    let mut unsynced = armed.clone();
    unsynced.palw_court_capacity_v1 =
        Some(PalwCourtCapacityFenceV1::measured_at(ForkActivation::new(RELEASE + 1), test_measurement()).unwrap());
    assert!(unsynced.validate_palw_court_capacity_in_v1(&records).is_err());
    let mut never = t12.clone();
    never.palw_court_capacity_v1 = Some(PalwCourtCapacityFenceV1::placeholder_at(ForkActivation::never()));
    never.sync_palw_court_capacity_v1();
    never.validate_palw_v2().expect("never() is dormant");
    // Arming moves the ids (Some-only, with the policy); a different policy is a different network.
    assert_ne!(armed.consensus_params_id(), t12.consensus_params_id());
    let mut other_policy = armed.clone();
    other_policy.palw_court_capacity_v1 = Some(
        PalwCourtCapacityFenceV1::measured_at(ForkActivation::new(RELEASE), PalwCourtMeasurementV1 { rho_sompi: 2_000, ..test_measurement() })
            .unwrap(),
    );
    assert_ne!(other_policy.consensus_params_id(), armed.consensus_params_id());
}

// ---- 42-slot saturation and the continuous refill ---------------------------------------------------------------------------------

/// **A coalition holds every one of the 42 slots; the 43rd prosecution — honest, independent, registered after genesis, never a seat —
/// queues instead of being refused, and is granted the first freed slot ahead of the coalition, which re-occupies every other slot as
/// it frees. It opens and reaches its verdict inside its claim's hard deadline; the convicted producer reimburses its occupancy.**
#[test]
fn g07_saturation_and_continuous_refill_an_honest_prosecution_reaches_its_verdict() {
    let policy = test_policy();
    let (mut c, row, seats) = held_chain(Some(test_measurement()), 2 * 42 + 2);
    let turn = c.sp.turn_deadline_daa();
    let capacity = policy.capacity_sessions(turn);
    assert_eq!(capacity, 42);
    // The lying claim and the outsider's reservation on it.
    let (target, _) = licensed(&mut c, &row, &seats, EXECUTOR, 1);
    let outsider = bond_key(OUTSIDER);
    c.step(&[reservation(&c.s, target, outsider)], Rule::T12).expect("the outsider reserves");
    let hard = c.s.legacy_dispute_v1(&target).unwrap().hard_deadline_daa;
    let outsider_before = c.collateral(&outsider);
    // The coalition's decoy claims, every Sybil reserving its own.
    let (d1, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 11);
    let (d2, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 12);
    c.step(&(0..42).map(|n| reservation(&c.s, d1, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations on d1");
    c.step(&(42..86).map(|n| reservation(&c.s, d2, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations on d2");
    // Saturation: 42 coalition sessions in one block (42 one-move units = the block budget).
    let fill: Vec<PalwConsensusObjectV2> = (0..42).map(|n| accusation_by(&c, &row, d1, sybil(n))).collect();
    c.step(&fill, Rule::Opening).expect("the coalition fills every slot");
    assert_eq!(c.s.court_sessions_iter().count() as u64, capacity, "every slot held");
    let saturated_at = c.daa;
    let first_backstop = c.s.court_sessions_iter().map(|(_, s)| s.deadline_daa).min().unwrap();
    // The 43rd prosecution: refused under int-12's cap, QUEUED here, its escrow on its free half.
    c.step(&[accusation_by(&c, &row, target, outsider)], Rule::Opening).expect("the honest prosecution is admitted to the queue");
    let queued = c.court_row(&target, &outsider).expect("its court row");
    assert!(matches!(queued.stage, PalwCourtStageV1::Queued { .. }), "queued, not refused: {queued:?}");
    assert!(c.s.court_sessions_iter().all(|(_, s)| s.claim != target), "nothing opened on the lying claim yet");
    let escrow = policy.fee(policy.ticket_window(turn) + c.sp.window_court(), policy.weight());
    assert_eq!(queued.escrow, escrow);
    assert!(palw_accuser_exposure_v1(&c.s, &outsider) >= escrow, "the escrow sits on the outsider's free half (A-6)");
    // The guarantee: the worst-case grant is the first coalition backstop, and opening by then leaves the hard deadline intact.
    assert!(first_backstop + policy.ticket_window(turn) <= hard, "the worst case still opens before the hard deadline");
    // The refill attack: while the honest prosecution waits, no coalition opening takes a slot ahead of it — it queues behind.
    c.step(&[accusation_by(&c, &row, d2, sybil(42))], Rule::Opening).expect("the coalition's 44th queues");
    assert!(matches!(c.court_row(&d2, &sybil(42)).unwrap().stage, PalwCourtStageV1::Queued { .. }));
    // Run the chain. Every block the coalition re-files on every slot that frees (a further 43 Sybils stand ready on d2); the
    // honest prosecution must be granted first and must open before its hard deadline.
    let mut granted_at = None;
    let mut refill = 43u64;
    while granted_at.is_none() {
        assert!(c.daa < hard, "the honest prosecution starved");
        let free_before = capacity.saturating_sub(c.s.court_sessions_iter().count() as u64);
        let mut objects = Vec::new();
        for _ in 0..free_before.min(86 - refill) {
            objects.push(accusation_by(&c, &row, d2, sybil(refill)));
            refill += 1;
        }
        if objects.is_empty() {
            c.beat();
        } else {
            // Each refill either opens (free slot, nobody queued) or queues; none takes the honest prosecution's place.
            c.step(&objects, Rule::Opening).expect("the coalition refills");
        }
        if let Some(PalwCourtStageV1::Ticket { granted_daa, .. }) = c.court_row(&target, &outsider).map(|r| r.stage) {
            granted_at = Some(granted_daa);
        }
    }
    let granted_at = granted_at.unwrap();
    eprintln!(
        "[G-07] saturated at {saturated_at}; first coalition backstop {first_backstop}; honest granted at {granted_at}; hard deadline {hard}"
    );
    assert!(granted_at <= first_backstop, "granted no later than its worst case");
    // Nothing the coalition filed after the honest prosecution queued opened ahead of it: every session it holds on its refill claim
    // opened at or after the honest grant (the refills queued behind it, or opened only once the queue was empty).
    for (_, session) in c.s.court_sessions_iter() {
        if session.claim == d2 {
            assert!(session.opened_daa >= granted_at, "a refill opened at {} ahead of the honest grant {granted_at}", session.opened_daa);
        }
    }
    // The honest prosecution opens with its ticket, whatever the coalition holds now.
    c.step(&[accusation_by(&c, &row, target, outsider)], Rule::Opening).expect("the ticket opens");
    let running = c.court_row(&target, &outsider).expect("running");
    assert!(matches!(running.stage, PalwCourtStageV1::Running { started_daa, .. } if started_daa == granted_at), "{running:?}");
    assert!(c.s.court_sessions_iter().any(|(_, s)| s.claim == target && s.challenger_bond == outsider));
    // The colluding producer cannot answer a lie: it defaults on its rung, and the court convicts — the verdict.
    let mut verdict_at = None;
    while verdict_at.is_none() {
        assert!(c.daa < hard, "the verdict must land inside the claim's hard deadline");
        c.beat();
        if let PalwClaimPhaseV2::Voided { voided_daa, reason } = c.s.claim(&target).unwrap().phase {
            assert!(matches!(reason, PalwVoidReasonV2::CourtDefault | PalwVoidReasonV2::CourtHeldVerdict | PalwVoidReasonV2::CourtFraud), "{reason:?}");
            verdict_at = Some(voided_daa);
        }
    }
    let verdict_at = verdict_at.unwrap();
    eprintln!("[G-07] verdict at {verdict_at} (hard deadline {hard})");
    assert!(verdict_at <= hard);
    // Reimbursed: the outsider paid no occupancy (the producer did), every deposit and escrow returned.
    assert!(c.s.legacy_dispute_v1(&target).is_none(), "the conviction settled and released every row");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &outsider), 0);
    assert_eq!(c.collateral(&outsider), outsider_before, "the honest prosecutor nets zero");
}

/// **The deadline guarantee under a flood of EARLIER-deadline entries.** The coalition holds every slot, and also holds a claim
/// accepted BEFORE the lying one — so its queued entries sort ahead of the honest prosecution (earliest claim deadline first). The
/// claims' trace retention is shortened in the carriage so the hard deadlines bind (the position a claim reaches ~1M DAA later on
/// testnet-12; a fixture time shift, not a params change): the honest prosecution's worst case is the coalition's backstop, plus its
/// ticket window and its session. The coalition may queue 41 entries ahead of it — each takes a slot freed at that backstop — but the
/// 42nd, which would push the honest prosecution to the second round of releases and past its deadline, is refused. When the slots
/// free, the queue is granted in deadline order: the 41 entries and the honest prosecution together; it opens and convicts in time.
#[test]
fn g07_an_earlier_deadline_flood_cannot_push_a_queued_prosecution_past_its_deadline() {
    let policy = test_policy();
    let (mut c, row, seats) = held_chain(Some(test_measurement()), 84);
    let turn = c.sp.turn_deadline_daa();
    let ticket = policy.ticket_window(turn);
    let (early, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 10);
    let (target, _) = licensed(&mut c, &row, &seats, EXECUTOR, 1);
    let (filler, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 11);
    c.step(&(0..42).map(|n| reservation(&c.s, filler, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations");
    c.step(&(0..42).map(|n| accusation_by(&c, &row, filler, sybil(n))).collect::<Vec<_>>(), Rule::Opening).expect("every slot");
    let backstop = c.s.court_sessions_iter().map(|(_, s)| s.deadline_daa).min().unwrap();
    assert!(c.s.court_sessions_iter().all(|(_, s)| s.deadline_daa == backstop), "one block: one backstop");
    let window = backstop - c.daa;
    // The time shift: the honest claim's hard deadline 5 DAA beyond its worst case; the coalition's early claim 1 DAA before it.
    let w_disclose = kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&c.sp);
    let worst = backstop + ticket + window;
    let mut carriage = PalwStateCarriageV2::from_state(&c.s);
    carriage.claims.get_mut(&target).unwrap().trace_retention_daa = worst + 5 + w_disclose;
    carriage.claims.get_mut(&early).unwrap().trace_retention_daa = worst + 4 + w_disclose;
    c.s = carriage.into_state(&c.sp, None).expect("a consistent carriage");
    let outsider = bond_key(OUTSIDER);
    c.step(&[reservation(&c.s, target, outsider)], Rule::T12).expect("the outsider reserves");
    c.step(&(42..84).map(|n| reservation(&c.s, early, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("the flood reserves");
    let hard = c.s.legacy_dispute_v1(&target).unwrap().hard_deadline_daa;
    let hard_early = c.s.legacy_dispute_v1(&early).unwrap().hard_deadline_daa;
    assert_eq!((hard, hard_early), (worst + 5, worst + 4), "the hard deadlines the fixture set");
    // The honest prosecution queues: position 0, worst-case grant at the backstop.
    c.step(&[accusation_by(&c, &row, target, outsider)], Rule::Opening).expect("queued");
    assert!(matches!(c.court_row(&target, &outsider).unwrap().stage, PalwCourtStageV1::Queued { .. }));
    // 41 earlier-deadline entries queue ahead of it (the honest prosecution moves to position 41, still a backstop slot).
    c.step(&(42..83).map(|n| accusation_by(&c, &row, early, sybil(n))).collect::<Vec<_>>(), Rule::Opening).expect("41 ahead");
    // The 42nd would push it past its deadline: refused.
    let refused = c.step(&[accusation_by(&c, &row, early, sybil(83))], Rule::Opening).expect_err("the 42nd ahead");
    assert!(refused_with(&refused, "no deadline guarantee"), "{refused:?}");
    // The slots free (the filler's producer defaults); the queue is granted in deadline order — the honest prosecution among them.
    let mut granted_at = None;
    while granted_at.is_none() {
        assert!(c.daa < backstop, "granted no later than the worst case");
        c.beat();
        if let Some(PalwCourtStageV1::Ticket { granted_daa, .. }) = c.court_row(&target, &outsider).map(|r| r.stage) {
            granted_at = Some(granted_daa);
        }
    }
    let granted = (42..83)
        .filter(|n| matches!(c.court_row(&early, &sybil(*n)).map(|r| r.stage), Some(PalwCourtStageV1::Ticket { .. })))
        .count();
    assert_eq!(granted, 41, "the 41 entries ahead were granted with it");
    c.step(&[accusation_by(&c, &row, target, outsider)], Rule::Opening).expect("the ticket opens");
    let mut verdict = None;
    while verdict.is_none() {
        assert!(c.daa < hard, "the verdict lands inside the hard deadline");
        c.beat();
        if let PalwClaimPhaseV2::Voided { voided_daa, .. } = c.s.claim(&target).unwrap().phase {
            verdict = Some(voided_daa);
        }
    }
    eprintln!(
        "[G-07 flood] backstop {backstop}; worst case {worst}; hard {hard}; honest granted at {}; verdict at {}",
        granted_at.unwrap(),
        verdict.unwrap()
    );
    // The coalition's 41 unused tickets expire and are charged for the window they held; nothing is owed by the honest prosecutor.
    for _ in 0..=ticket {
        c.beat();
    }
    assert!((42..83).all(|n| c.court_row(&early, &sybil(n)).is_none()), "every unused ticket expired and settled");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &outsider), 0);
}

/// **The scheduler, pure:** the worst-case grant of each entry; an entry whose session would end past its deadline is refused, and
/// an entry that would push an EARLIER-admitted one past its deadline is refused too.
#[test]
fn the_scheduler_admits_only_what_every_queued_deadline_survives() {
    let key = |deadline: u64, n: u64| PalwCourtQueueKeyV1 { deadline_daa: deadline, enqueued_daa: 0, claim: h(n), reserver: bond_key(n) };
    let entry = |deadline: u64, n: u64| PalwCourtQueuedV1 { key: key(deadline, n), service_daa: 100 };
    // Two slots, both held until 500; ticket window 10, service 100: the first round opens by 510 and ends by 610.
    assert_eq!(palw_court_schedule_v1(0, 2, &[500, 500], &[entry(610, 1)], 10), Ok(vec![500]));
    assert_eq!(palw_court_schedule_v1(0, 2, &[500, 500], &[entry(609, 1)], 10), Err(0), "its session would end past 609");
    // Free capacity: granted now.
    assert_eq!(palw_court_schedule_v1(7, 3, &[500, 500], &[entry(200, 1)], 10), Ok(vec![7]));
    // Two earlier-deadline entries ahead take both slots; a later one needs the second round (500 + 110 -> ends by 720).
    let ahead = [entry(610, 2), entry(610, 3)];
    assert_eq!(palw_court_schedule_v1(0, 2, &[500, 500], &[entry(720, 1), ahead[0], ahead[1]], 10), Ok(vec![610, 500, 500]));
    assert_eq!(palw_court_schedule_v1(0, 2, &[500, 500], &[entry(719, 1), ahead[0], ahead[1]], 10), Err(2));
    // Deterministic order: the key, never the slice order.
    let a = palw_court_schedule_v1(0, 2, &[500, 500], &[ahead[1], entry(720, 1), ahead[0]], 10);
    assert_eq!(a, Ok(vec![500, 610, 500]));
}

/// **Below the fence (int-12's cap) the same 43rd prosecution is refused at capacity** — the gap the fence closes.
#[test]
fn g07_unarmed_the_43rd_prosecution_is_refused_at_capacity() {
    let (mut c, row, seats) = held_chain(None, 42);
    let (target, _) = licensed(&mut c, &row, &seats, EXECUTOR, 1);
    let (d1, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 11);
    c.step(&[reservation(&c.s, target, bond_key(OUTSIDER))], Rule::T12).expect("reserved");
    c.step(&(0..42).map(|n| reservation(&c.s, d1, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations");
    c.step(&(0..42).map(|n| accusation_by(&c, &row, d1, sybil(n))).collect::<Vec<_>>(), Rule::Opening).expect("filled");
    let full = c.step(&[accusation_by(&c, &row, target, bond_key(OUTSIDER))], Rule::Opening).expect_err("at capacity");
    assert!(matches!(full, PalwStateV2Error::CourtSessionsAtCapacity { .. }), "{full:?}");
}

// ---- bond splitting ------------------------------------------------------------------------------------------------------------

/// **Splitting bonds does not avoid the occupancy cost.** Four sessions held by ONE bond (on four claims) and four sessions held by FOUR
/// bonds (one each) for the same occupancy cost the coalition the same: ρ × occupied DAA × weight per session, settled when each
/// reservation ends (an open session is charged to its backstop: the slot stays held for it).
#[test]
fn g07_bond_splitting_does_not_avoid_the_occupancy_fee() {
    let policy = test_policy();
    let charged = |split: bool| -> u128 {
        let (mut c, row, seats) = held_chain(Some(test_measurement()), 4);
        let claims: Vec<Hash64> = (0..4).map(|i| licensed(&mut c, &row, &seats, COALITION_PRODUCER, 20 + i).0).collect();
        let holder = |i: usize| if split { sybil(i as u64) } else { sybil(0) };
        let holders: Vec<PalwBondKeyV2> = (0..4).map(holder).collect();
        let before: u128 = holders.iter().collect::<std::collections::BTreeSet<_>>().iter().map(|b| u128::from(c.collateral(b))).sum();
        c.step(&claims.iter().enumerate().map(|(i, claim)| reservation(&c.s, *claim, holder(i))).collect::<Vec<_>>(), Rule::T12)
            .expect("reservations");
        c.step(&claims.iter().enumerate().map(|(i, claim)| accusation_by(&c, &row, *claim, holder(i))).collect::<Vec<_>>(), Rule::Opening)
            .expect("four sessions");
        assert_eq!(c.s.court_sessions_iter().count(), 4);
        c.beat();
        c.step(&claims.iter().enumerate().map(|(i, claim)| release(*claim, holder(i))).collect::<Vec<_>>(), Rule::T12)
            .expect("every reservation released");
        let after: u128 = holders.iter().collect::<std::collections::BTreeSet<_>>().iter().map(|b| u128::from(c.collateral(b))).sum();
        // The held deposits are burned only at retirement; only the occupancy fee has left the collateral here.
        before - after
    };
    let concentrated = charged(false);
    let split = charged(true);
    let window = palw_t12_launch_params_v1();
    let per_session = test_policy().fee(bundle(&window).state.window_court(), test_policy().weight());
    eprintln!("[G-07] occupancy fee: one bond {concentrated}, four bonds {split}, per session {per_session}");
    assert_eq!(concentrated, split, "bond splitting does not change the occupancy cost");
    assert_eq!(concentrated, 4 * per_session, "ρ × occupied DAA × weight per session");
}

// ---- the block budget at maximum load ------------------------------------------------------------------------------------------

/// **The block budget holds at maximum load.** TEST policy: 4 work units a block, a one-move accusation 1 — a block takes four court
/// openings and refuses the fifth; and an evidence-bytes budget of two accusations refuses the third. The acceptance walk holds the same
/// budget over the objects it admits (the fold is the second lock).
#[test]
fn g07_the_block_budget_holds_at_maximum_load() {
    let work_bound = measurement([1, 4, 4, 4], 4, u64::MAX / 4);
    let (mut c, row, seats) = held_chain(Some(work_bound), 5);
    let (d1, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 31);
    c.step(&(0..5).map(|n| reservation(&c.s, d1, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations");
    let five: Vec<PalwConsensusObjectV2> = (0..5).map(|n| accusation_by(&c, &row, d1, sybil(n))).collect();
    let over = c.try_fold(c.daa + 1, &five, PalwBlockWorkV3::None, Hash64::default(), Rule::Opening).map(|_| ()).expect_err("five ops");
    assert!(refused_with(&over, "court budget"), "{over:?}");
    c.step(&five[..4], Rule::Opening).expect("four ops fit the budget");
    assert_eq!(c.s.court_sessions_iter().count(), 4);
    // Bytes: a budget of two accusations' bytes.
    let one = borsh::to_vec(&five[0]).unwrap().len() as u64;
    let byte_bound = measurement([1, 1_000, 1_000, 1_000], 1_000, 2 * one + one / 2);
    let (mut c, row, seats) = held_chain(Some(byte_bound), 3);
    let (d1, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 32);
    c.step(&(0..3).map(|n| reservation(&c.s, d1, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations");
    let three: Vec<PalwConsensusObjectV2> = (0..3).map(|n| accusation_by(&c, &row, d1, sybil(n))).collect();
    let over = c.try_fold(c.daa + 1, &three, PalwBlockWorkV3::None, Hash64::default(), Rule::Opening).map(|_| ()).expect_err("bytes");
    assert!(refused_with(&over, "court budget"), "{over:?}");
    c.step(&three[..2], Rule::Opening).expect("two accusations' bytes fit");
    // Court admission needs the challenger's own reservation past the fence.
    let (d2, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 33);
    let unreserved = c.step(&[accusation_by(&c, &row, d2, sybil(0))], Rule::Opening).expect_err("no reservation on d2");
    assert!(refused_with(&unreserved, "court admission"), "{unreserved:?}");
}

// ---- restart and reorg mid-queue -------------------------------------------------------------------------------------------------

/// **Restart and reorg give identical reservations, deadlines, fees and releases.** Every block of the saturation script is already
/// checked (delta re-apply and revert, carriage reload under its root). Here a node restarted from the carriage mid-queue — the coalition
/// holding every slot, the honest prosecution queued — and a node that never stopped run the same blocks to the grant, the opening and
/// the conviction, and agree on every state root and every court row on the way; and the queue's state reverts block by block to the
/// saturated tip and re-applies to the same roots (a reorg across the grant).
#[test]
fn g07_restart_and_reorg_mid_queue_are_identical() {
    let policy = test_policy();
    let (mut c, row, seats) = held_chain(Some(test_measurement()), 42);
    let (target, _) = licensed(&mut c, &row, &seats, EXECUTOR, 1);
    let (d1, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 11);
    let outsider = bond_key(OUTSIDER);
    c.step(&[reservation(&c.s, target, outsider)], Rule::T12).expect("reserved");
    c.step(&(0..42).map(|n| reservation(&c.s, d1, sybil(n))).collect::<Vec<_>>(), Rule::T12).expect("reservations");
    c.step(&(0..42).map(|n| accusation_by(&c, &row, d1, sybil(n))).collect::<Vec<_>>(), Rule::Opening).expect("filled");
    c.step(&[accusation_by(&c, &row, target, outsider)], Rule::Opening).expect("queued");
    // Restart: a node reloaded from the carriage at this tip.
    let root = c.s.state_root();
    let mut restarted = c.clone();
    restarted.s = PalwStateCarriageV2::from_state(&c.s).into_state(&c.sp, Some(root)).expect("the carriage reloads mid-queue");
    let mut tape: Vec<(PalwChainStateV2, kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2)> = Vec::new();
    let mut done = false;
    for _ in 0..20_000 {
        let opening = matches!(c.court_row(&target, &outsider).map(|r| r.stage), Some(PalwCourtStageV1::Ticket { .. }));
        let objects = if opening { vec![accusation_by(&c, &row, target, outsider)] } else { Vec::new() };
        let rule = if opening { Rule::Opening } else { Rule::T12 };
        let daa = c.daa + 1;
        let (next, delta) = c.try_fold(daa, &objects, PalwBlockWorkV3::None, Hash64::default(), rule).expect("the running node");
        let (twin, _) = restarted.try_fold(daa, &objects, PalwBlockWorkV3::None, Hash64::default(), rule).expect("the restarted node");
        assert_eq!(twin.state_root(), next.state_root(), "DAA {daa}: the restarted node agrees");
        assert_eq!(twin.legacy_dispute_v1(&target), next.legacy_dispute_v1(&target), "DAA {daa}: the same reservation and court row");
        tape.push((c.s.clone(), delta));
        c.s = next.clone();
        c.daa = daa;
        restarted.s = twin;
        restarted.daa = daa;
        if matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::Voided { .. }) {
            done = true;
            break;
        }
    }
    assert!(done, "the conviction lands");
    // Reorg: revert every block back to the queued tip, then re-apply — the same roots, the same fees and releases.
    let tip = c.s.clone();
    let mut back = tip.clone();
    for (parent, delta) in tape.iter().rev() {
        back = revert_delta_v2(&back, delta, &c.sp).expect("reverts");
        assert_eq!(back, *parent, "a reorg back across the queue restores each parent exactly");
    }
    assert_eq!(back.state_root(), root);
    let mut forward = back;
    for (_, delta) in &tape {
        forward = apply_delta_v2(&forward, delta, &c.sp).expect("re-applies");
    }
    assert_eq!(forward, tip, "and re-applies to the same tip");
}

// ---- option (b): the fused-leaf route --------------------------------------------------------------------------------------------

/// **Option (b) does not count the fused-leaf route until the court-capacity fence is in force** (user direction 2026-10-16 §1): with
/// LG14-A + LG14-B armed, a class with fused-attention leaves (the 8k row) takes no claim; with the court-capacity fence too, it does.
/// A class with no fused leaf needs only LG14-A + LG14-B.
#[test]
fn fused_leaf_route_counts_only_with_court_capacity() {
    let p = palw_t12_launch_params_v1();
    let s = genesis_state(&p);
    let row = eight_k(&p);
    assert!(s.class(&row.id).expect("the 8k row").fused_attention, "the 8k row's attention is fused");
    for (class_id, ..) in genesis_classes(&p) {
        eprintln!("[option (b)] genesis class {class_id}: fused_attention {}", s.class(&class_id).is_some_and(|c| c.fused_attention));
    }
    let base = bundle(&p).state;
    let legacy = PalwG14RouteMirrorV1 { from_daa: 0, legacy_filer_from: Some(0), legacy_held_da_from: Some(0), ..Default::default() };
    let routed = base.clone().with_g14_route_required_v1(Some(legacy));
    assert_eq!(palw_g14_route_attempt_refusal_for_class_v1(&routed, 1_000, false), None, "no fused leaf: LG14-A + LG14-B suffice");
    let why = palw_g14_route_attempt_refusal_for_class_v1(&routed, 1_000, true).expect("a fused class is not routed yet");
    assert!(why.contains("palw_court_capacity_v1"), "{why}");
    let with_court = base.with_g14_route_required_v1(Some(PalwG14RouteMirrorV1 { court_capacity_from: Some(0), ..legacy }));
    assert_eq!(palw_g14_route_attempt_refusal_for_class_v1(&with_court, 1_000, true), None, "routed once court capacity is in force");
    let late = PalwG14RouteMirrorV1 { court_capacity_from: Some(2_000), ..legacy };
    assert!(!late.route_armed(PalwG14RouteFamilyV1::LegacyFusedLeaf, 1_000, false), "read at the claim's acceptance");
    // Params-level: option (b) + LG14-A + LG14-B + the court-capacity fence at the release, each with its record.
    let t12 = palw_t12_shipped_params();
    let (mut armed, mut records) = arm(t12.clone(), RELEASE, Some(test_measurement()));
    armed.palw_legacy_held_da_v2 = Some(ForkActivation::new(RELEASE));
    armed.palw_g14_route_required_v1 = Some(ForkActivation::new(RELEASE));
    armed.sync_palw_g14_gates_v1();
    records.push(record(PalwG14GatedFenceV1::LegacyHeldDaV2, t12.genesis.hash, RELEASE));
    records.push(record(PalwG14GatedFenceV1::G14RouteRequiredV1, t12.genesis.hash, RELEASE));
    armed.validate_palw_v2_with_arming_records_v1(&records).expect("the whole route validates with its records");
    let mirror = bundle(&armed).state.g14_route_required_v1().expect("option (b)");
    assert_eq!(mirror.court_capacity_from, Some(RELEASE));
    assert!(mirror.route_armed(PalwG14RouteFamilyV1::LegacyFusedLeaf, RELEASE, false));
}

// ---- the timing harness ----------------------------------------------------------------------------------------------------------

/// Minimum, median and maximum of `samples` (ns).
fn spread(mut samples: Vec<u128>) -> (u128, u128, u128) {
    samples.sort_unstable();
    (samples[0], samples[samples.len() / 2], samples[samples.len() - 1])
}

/// **The court-operation timing harness** (`palw_court_op_timing_harness`, named by the fence's arm-readiness requirements) — times
/// the court operations this fixture drives through the REAL fold (`apply_palw_transition_v7`) on this machine and build, and prints
/// one record line each: the operation's encoded bytes and the block's fold time with the empty block's time subtracted (min / median /
/// max over the runs, each run folding the same parent). **Not a measured record:** the policy's block budget, work units and ρ_court
/// must come from the slowest supported node, release build, at each class's maximum profile — and the one-move leaf recompute on
/// real model rows (the work unit that dominates a one-move verdict) is timed on the node by
/// `legacy_v2_attempt_full_collusion_outside_bond_convicts` (`[court-op timing]`), since this fixture's evidence is junk the fold
/// defers to a dissection before any recompute. The working set is not measured here (no allocator probe): a record needs the slowest
/// node's peak resident memory per operation class.
///
/// Run explicitly: `cargo test -p kaspa-consensus-core --test g14_court_capacity -- --ignored --nocapture palw_court_op_timing_harness`
#[test]
#[ignore]
fn palw_court_op_timing_harness() {
    use std::time::Instant;
    const RUNS: usize = 7;
    let policy = test_policy();
    let (mut c, row, seats) = held_chain(Some(test_measurement()), 42);
    let (d1, _) = licensed(&mut c, &row, &seats, COALITION_PRODUCER, 11);
    let host = format!(
        "{}-{} {} build, this development machine (NOT the slowest supported node)",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if cfg!(debug_assertions) { "debug" } else { "release" }
    );
    println!("[palw_court_op_timing_harness] {host}; NOT A MEASURED RECORD — arming stays refused until one is pinned");
    let time = |c: &Held, objects: &[PalwConsensusObjectV2], rule: Rule| -> Vec<u128> {
        (0..RUNS)
            .map(|_| {
                let t = Instant::now();
                c.try_fold(c.daa + 1, objects, PalwBlockWorkV3::None, Hash64::default(), rule).expect("the timed block folds");
                t.elapsed().as_nanos()
            })
            .collect()
    };
    let net = |samples: Vec<u128>, empty: u128, ops: u128| -> (u128, u128, u128) {
        let (lo, mid, hi) = spread(samples);
        (lo.saturating_sub(empty) / ops, mid.saturating_sub(empty) / ops, hi.saturating_sub(empty) / ops)
    };
    // The reservations (not a court operation; the admission every court opening needs past the fence).
    let reservations: Vec<PalwConsensusObjectV2> = (0..42).map(|n| reservation(&c.s, d1, sybil(n))).collect();
    let empty = spread(time(&c, &[], Rule::T12)).1;
    let bytes = borsh::to_vec(&reservations[0]).unwrap().len();
    let (lo, mid, hi) = net(time(&c, &reservations, Rule::T12), empty, 42);
    println!("[palw_court_op_timing_harness] DisputeReservedV1 (admission, not a court op): {bytes} B; fold ns/op min {lo} median {mid} max {hi}");
    c.step(&reservations, Rule::T12).expect("reservations");
    // One held opening (ShardCourtAccused deferred to a dissection at the named leaf: the OneMove class's acceptance work).
    let empty = spread(time(&c, &[], Rule::Opening)).1;
    let one = vec![accusation_by(&c, &row, d1, sybil(0))];
    let bytes = borsh::to_vec(&one[0]).unwrap().len();
    let cost = palw_court_op_cost_v1(&policy, &c.s, &one[0]).expect("a court op");
    let (lo, mid, hi) = net(time(&c, &one, Rule::Opening), empty, 1);
    println!(
        "[palw_court_op_timing_harness] OneMove (held opening, ShardCourtAccused): {bytes} B (TEST cost {cost:?}); fold ns/op min {lo} median {mid} max {hi}"
    );
    let (one_bytes, one_hi) = (bytes, hi);
    // Maximum load at the TEST budget: 42 openings in one block.
    let all: Vec<PalwConsensusObjectV2> = (0..42).map(|n| accusation_by(&c, &row, d1, sybil(n))).collect();
    let total: usize = all.iter().map(|o| borsh::to_vec(o).unwrap().len()).sum();
    let (lo, mid, hi) = net(time(&c, &all, Rule::Opening), empty, 1);
    println!(
        "[palw_court_op_timing_harness] max load (42 openings, the TEST block budget): {total} B; fold ns/block min {lo} median {mid} max {hi}"
    );
    c.step(&all, Rule::Opening).expect("every slot");
    // The rung-default sweep that closes all 42 (the Close class's work, here by default rather than by a CourtClosed object).
    let mut swept = None;
    for _ in 0..10_000 {
        let daa = c.daa + 1;
        let (next, _) = c.try_fold(daa, &[], PalwBlockWorkV3::None, Hash64::default(), Rule::T12).expect("folds");
        if next.court_sessions_iter().count() < 42 {
            swept = Some(spread(time(&c, &[], Rule::T12)));
            break;
        }
        c.s = next;
        c.daa = daa;
    }
    let (lo, mid, hi) = swept.expect("the sessions default");
    println!(
        "[palw_court_op_timing_harness] default sweep of 42 sessions (claim voided, every row settled): fold ns/block min {lo} median {mid} max {hi} (empty block ~{empty})"
    );
    // The record this run could fill (PalwCourtMeasurementV1): the classes this fixture drives, and the ones it does not — a record
    // with an unmeasured class derives no policy (`PalwCourtMeasurementV1::policy` refuses a 0), so arming stays refused until every
    // class is measured on the slowest supported node.
    println!(
        "[palw_court_op_timing_harness] measurement record from this run: OneMove (held opening) {one_bytes} B, worst {one_hi} ns; \
         Opening (CourtOpened), DissectionMove, Close: UNMEASURED here; working sets: UNMEASURED; the real one-move leaf recompute: the \
         node E2E's [court-op timing]"
    );
}

// ---- testnet-12's identity -------------------------------------------------------------------------------------------------------

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
    assert_eq!(t12.palw_court_capacity_v1, None);
    assert_eq!(bundle(&t12).state.court_capacity_v1(), None);
}
