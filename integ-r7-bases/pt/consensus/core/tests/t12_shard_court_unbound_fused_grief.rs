#![allow(dead_code, unused_imports)]
//! **The unbound fused-leaf grief, replayed through the REAL fold** (adopted from the review of
//! 8be0f661, `fix/t12-shard-court-openings-first`).
//!
//! The attack (8be0f661's "understand phase"): any Active bond at the floor that is NOT on the
//! claim's panel files a `ShardCourtAccused` built from nothing but what the chain publishes — the
//! claim's id, `execution_root`, `trace_root` and executor bond, and its class's shape profile — with
//! a job context it invented, a step root it invented, and a tile and path that are nothing. It
//! names a leaf that, under ITS job context, resolves to an `AttnFused` node. Below
//! `palw_audit_2026_09_23` the verdict (`palw_one_move_verdict_v1`) answers `NeedsDissection` before
//! the binding is recomputed, and the held regime opens a dissection at that leaf, disarming the
//! licensed claim's `Final` deadline.
//!
//! Everything below is the network's own `Params` (testnet-12 and testnet-11 as a node resolves
//! them from their id), its bundle's `PalwStateParamsV2`, its own genesis fold, and
//! `apply_palw_transition_v7`. The extras are `dos_l5_common::extras` plus the two court ladders
//! the processor's `palw_transition_extras_for` resolves (processor.rs: `shard_court_ladder`,
//! `held_context_ladder` — both `palw_court_step_ladder_at` when their fence is active), which
//! `dos_l5_common::extras` leaves `None`.
//!
//! The accuser's exposure is read where the ledger in force keeps it: `reserved_exposure` below
//! `palw_rcore_plus`, the derived court-challenger index (`palw_accuser_exposure_v1`, the A-6
//! ledger) past it — S's merge puts no challenger stake in `reserved_exposure` there.
//!
//! Run: cargo test -p kaspa-consensus-core --test t12_shard_court_unbound_fused_grief -- --nocapture

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_shard_court_v1::{
    PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1, palw_shard_court_accusation_bytes_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateParamsV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use kaspa_consensus_core::palw_step::{
    PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepTableV1, canonical_step_leaf_index, step_leaf_count_capped_v1,
};
use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, PalwStepOpeningV1, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;

const GRIEFER: u64 = 0x0061_21EF;

fn t11() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))
}

/// `dos_l5_common::extras` plus the processor's two court ladders. `audit: Some(false)` forces the
/// 2026-09-23 fence off (the control on the t12 state); `None` resolves it from `p`.
fn court_extras(p: &Params, daa: u64, audit: Option<bool>) -> PalwTransitionExtrasV1 {
    let mut e = extras(p, daa);
    let ladder =
        kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(&bundle(p).court, p.palw_court_ladder_active_at(daa));
    e.shard_court_ladder = p.palw_shard_court_active_at(daa).then_some(ladder);
    e.held_context_ladder = p.palw_held_context_active_at(daa).then_some(ladder);
    if let Some(on) = audit {
        e.audit_2026_09_23_active = on;
        if !on {
            e.settled_anchor_depth = None;
        }
    }
    e
}

fn try_fold(
    p: &Params,
    sp: &PalwStateParamsV2,
    parent: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    work: PalwBlockWorkV3<'_>,
    exec_key: Hash64,
    subsidy: u64,
    audit: Option<bool>,
) -> Result<(PalwChainStateV2, Vec<(Hash64, String)>), PalwStateV2Error> {
    let f = flags(p, daa);
    apply_palw_transition_v7(
        parent,
        sp,
        None,
        &ctx(0x6121_0000 + daa, daa, daa, subsidy),
        objects,
        work,
        &[],
        exec_key,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &court_extras(p, daa, audit),
    )
    .map(|(s, _, skips)| (s, skips))
}

/// A genesis class whose published profile has an `AttnFused` site: the class id, the profile and
/// canonical job its registration carriage publishes, its initial target and declared leaves. The
/// floor (`base0_profile_v1`) has no fused site on either network, so the grief's targets are the
/// carriage-registered rows (t12: the held Qwen rows; t11: `4277d84f…`, which t11 does not hold);
/// the one with the shortest canonical job is taken, to keep the fold light.
struct FusedClass {
    id: Hash64,
    profile: kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    job: kaspa_consensus_core::palw_v2::PalwJobContextV2,
    target: u128,
    leaves: u64,
}

fn fused_genesis_class(p: &Params) -> FusedClass {
    let rows = genesis_classes(p);
    bundle(p)
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. }
                if c.profile.attn_nodes.iter().any(|n| n.op_kind == PalwStepOpKindV1::AttnFused) =>
            {
                assert_eq!(c.profile.shape_profile_id(), *class_id, "the class id IS its published profile's id");
                let (_, leaves, target, _) = *rows.iter().find(|r| r.0 == *class_id).expect("a genesis row");
                Some(FusedClass { id: *class_id, profile: c.profile.clone(), job: c.canonical.clone(), target, leaves })
            }
            _ => None,
        })
        .min_by_key(|c| c.job.declared_prefill_tokens)
        .expect("a genesis class with a fused site")
}

struct Licensed {
    p: Params,
    sp: PalwStateParamsV2,
    audit: Option<bool>,
    class: FusedClass,
    claim_id: Hash64,
    griefer: PalwBondKeyV2,
    s: PalwChainStateV2,
    licensed_daa: u64,
}

const EXECUTOR: u64 = 0x0061_21E0;

/// A junk attempt on the fused class by a fresh producer, bound to `seat_count` genesis bonds and
/// licensed all-Valid; a griefer bond registered beside it and seated nowhere. A
/// class whose registry row is not `Active` (t12's held rows open `Prefetching` and admit nothing
/// until seven seats prove the artifact — `dos_repro_3b`) is made `Active` through the carriage,
/// the one step here that is not the fold, exactly as `dos_repro_3d` does.
fn licensed_claim(p: Params, base: u64, audit: Option<bool>) -> Licensed {
    let b = bundle(&p);
    let sp = b.state.clone();
    let class = fused_genesis_class(&p);
    let seat_count = b.panel.seat_count() as usize;
    let bonds = genesis_bonds(&p);
    assert!(bonds.len() >= seat_count, "{} genesis bonds seat a {seat_count}-seat panel", bonds.len());
    let seats: Vec<(PalwBondKeyV2, Hash64)> = bonds[..seat_count].iter().map(|(k, o, _)| (*k, *o)).collect();
    let seat_keys: Vec<PalwBondKeyV2> = seats.iter().map(|s| s.0).collect();

    let mut s = genesis_state(&p);
    let lifecycle = s.model_lifecycle(&class.id).map(|r| r.state);
    if lifecycle.as_ref().is_some_and(|l| *l != kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active) {
        let mut c = kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&s);
        c.model_lifecycles.get_mut(&class.id).expect("the row").state =
            kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Active;
        s = c
            .into_state_v3(&sp, None, flags(&p, 0).uncertified_weightless, p.palw_canonical_work_daa())
            .expect("a consistent carriage");
    }
    println!("  class {} (prefill {}), registry row at genesis {lifecycle:?}", class.id, class.job.declared_prefill_tokens);

    let griefer = bond_key(GRIEFER);
    let floor = registration_floor(&p, base).max(sp.min_collateral_sompi());
    let rich = 1_000_000_000_000_000u64.max(floor); // 10,000,000 MSK: backs any genesis class's claim
    // The griefer posts 13,000 MSK (t12's producer floor) on both networks, so its bond backs the
    // accuser exposure `reserve_accuser_exposure_v2` asks of a held dissection (t11's own floor,
    // 0.004 MSK, does not). It is reserved while the court runs, never spent.
    let griefer_collateral = floor.max(1_300_000_000_000);
    let (s, _) = try_fold(
        &p,
        &sp,
        &s,
        base,
        &[bond_obj(EXECUTOR, rich), bond_obj(GRIEFER, griefer_collateral)],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        audit,
    )
    .unwrap_or_else(|e| panic!("the producer and the griefer register at {base}: {e:?}"));
    let pwu = match s.palw_canonical_per_draw_v1(&class.id, base + 1, p.palw_canonical_work_daa()) {
        Some(work) => kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(class.target, work),
        None => palw_pwu_v1(class.target, class.leaves),
    };
    let (env, key, claim_id) =
        junk_attempt(class.id, bond_key(EXECUTOR), pubkey_of(EXECUTOR), &operator_pubkey_of(EXECUTOR), pwu, 0x6121, 0x6121_0000);
    let (s, skips) = try_fold(&p, &sp, &s, base + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI, audit)
        .unwrap_or_else(|e| panic!("the attempt folds: {e:?}"));
    assert!(skips.is_empty(), "the attempt is admitted: {skips:?}");
    assert!(s.claim(&claim_id).is_some(), "the claim is recorded");
    let (s, _) = try_fold(
        &p,
        &sp,
        &s,
        base + 2,
        &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0x0006_121A), seats: seats_of(&seats) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        audit,
    )
    .unwrap_or_else(|e| panic!("the panel binds: {e:?}"));
    let (s, _) = try_fold(
        &p,
        &sp,
        &s,
        base + 3,
        &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: valid_receipts(claim_id, &seat_keys) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        audit,
    )
    .unwrap_or_else(|e| panic!("the panel licenses: {e:?}"));
    assert!(
        matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "licensed, got {:?}",
        s.claim(&claim_id).unwrap().phase
    );
    assert!(!seat_keys.contains(&griefer), "the griefer is on no panel");
    Licensed { p, sp, audit, class, claim_id, griefer, s, licensed_daa: base + 3 }
}

/// **The grief object, from public chain data only**: the claim's roots and executor, the class's
/// published profile, and a job context, step root and tile the griefer made up.
fn grief_object(l: &Licensed) -> (PalwConsensusObjectV2, u64) {
    let claim = l.s.claim(&l.claim_id).expect("claim").clone();
    let profile = l.class.profile.clone();
    assert_eq!(profile.shape_profile_id(), claim.class_id, "the class id IS the published profile's id");
    let mut job = l.class.job.clone();
    job.job_id = h(0x6121_0001);
    job.job_nullifier = h(0x6121_0002);
    job.assignment_id = h(0x6121_0003);
    job.execution_seed = [0x66; 32];
    job.prompt_token_ids_hash = h(0x6121_0004);
    let prefill = job.declared_prefill_tokens;
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 0, fused).expect("a slot");
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: prefill - 1, tile_index: 0 };
    let leaf = canonical_step_leaf_index(&profile, &job, &coord).expect("a leaf of the invented job");
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
        // The one echo: the claim's committed root, read off the chain.
        committed_execution_root: claim.execution_root,
    };
    let accusation = PalwShardCourtAccusationV1 {
        version: PALW_SHARD_COURT_VERSION_V1,
        claim: l.claim_id,
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        executor_bond: claim.bond,
        accuser_bond: l.griefer,
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
    };
    // The acceptance layer's size gate (processor.rs `max_close_bytes`), measured: the fold is the
    // gate under test, and this says whether the object would also have cleared the cheap one.
    let bytes = palw_shard_court_accusation_bytes_v1(&accusation);
    println!("  grief object: leaf {leaf}, {bytes} B against a close ceiling of {} B", bundle(&l.p).court.max_close_bytes());
    (PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) }, leaf)
}

/// Empty blocks `from..=to`.
fn advance(l: &Licensed, s: &PalwChainStateV2, from: u64, to: u64) -> PalwChainStateV2 {
    let mut s = s.clone();
    for daa in from..=to {
        s = try_fold(&l.p, &l.sp, &s, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, l.audit)
            .unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"))
            .0;
    }
    s
}

/// Empty blocks from `from` until the claim leaves `ReceiptLicensed`: `(daa, phase, state)`.
fn run_to_resolution(l: &Licensed, s: &PalwChainStateV2, from: u64, limit: u64) -> (u64, PalwClaimPhaseV2, PalwChainStateV2) {
    let mut s = s.clone();
    for daa in from..from + limit {
        s = try_fold(&l.p, &l.sp, &s, daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0, l.audit)
            .unwrap_or_else(|e| panic!("DAA {daa}: {e:?}"))
            .0;
        let phase = s.claim(&l.claim_id).expect("the claim").phase.clone();
        if !matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            return (daa, phase, s);
        }
    }
    panic!("the claim did not resolve within {limit} DAA of {from}");
}

/// What the grief, filed in the LAST block the claim's challenge deadline is still actionable in,
/// does to one network's licensed claim.
#[derive(Debug)]
struct Replay {
    /// The undisturbed claim's `Final` DAA.
    baseline_final: u64,
    /// The block the grief is filed in (`baseline_final - 1`).
    filed_at: u64,
    /// `Err(why)`: the fold refused the object. `Ok(reserved)`: a court opened and the griefer's
    /// bond carries `reserved` more exposure.
    verdict: Result<u128, String>,
    /// The claim's resolution with the grief (or, refused, with the block folded without it).
    resolved: (u64, PalwClaimPhaseV2),
    /// Collateral deltas, sompi, from the licensed state to the resolution: griefer, executor.
    griefer_delta: i128,
    executor_delta: i128,
}

fn replay(l: &Licensed) -> Replay {
    let (baseline_final, phase, _) = run_to_resolution(l, &l.s, l.licensed_daa + 1, 5_000);
    assert!(matches!(phase, PalwClaimPhaseV2::Final { .. }), "undisturbed, the licensed claim reaches Final: {phase:?}");
    let filed_at = baseline_final - 1;
    let parent = advance(l, &l.s, l.licensed_daa + 1, filed_at - 1);
    assert!(matches!(parent.claim(&l.claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let (object, leaf) = grief_object(l);
    let executor = l.s.claim(&l.claim_id).unwrap().bond;
    let collateral = |s: &PalwChainStateV2, k: &PalwBondKeyV2| s.bond(k).map_or(0, |b| b.collateral) as i128;
    // The ledger in force at the filing block (see the module note).
    let exposure = |s: &PalwChainStateV2| {
        if l.sp.rcore_plus_active_at(filed_at) {
            kaspa_consensus_core::palw_state_v2::palw_accuser_exposure_v1(s, &l.griefer)
        } else {
            s.reserved_exposure(&l.griefer)
        }
    };
    let before = exposure(&parent);
    let (verdict, after) = match try_fold(
        &l.p,
        &l.sp,
        &parent,
        filed_at,
        std::slice::from_ref(&object),
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
        l.audit,
    ) {
        Err(PalwStateV2Error::ShardCourt(why)) => {
            // The acceptance layer drops a refused object; the block folds without it.
            assert_eq!(parent.court_sessions_for_claim(&l.claim_id), 0, "no court on the claim");
            (Err(why), advance(l, &parent, filed_at, filed_at))
        }
        Err(other) => panic!("the grief object must reach the verdict, got {other:?}"),
        Ok((s, _)) => {
            assert_eq!(s.court_sessions_for_claim(&l.claim_id), 1, "a court on the claim");
            let (_, session) = s.court_sessions_iter().find(|(_, x)| x.claim == l.claim_id).expect("the session");
            assert_eq!(session.challenger_bond, l.griefer, "the griefer is the challenger");
            assert_eq!(session.ladder.terminal_index(), Some(leaf), "terminal on the invented leaf");
            (Ok(exposure(&s) - before), s)
        }
    };
    let (daa, phase, end) = run_to_resolution(l, &after, filed_at + 1, 5_000);
    Replay {
        baseline_final,
        filed_at,
        verdict,
        resolved: (daa, phase),
        griefer_delta: collateral(&end, &l.griefer) - collateral(&l.s, &l.griefer),
        executor_delta: collateral(&end, &executor) - collateral(&l.s, &executor),
    }
}

#[test]
fn the_unbound_fused_grief_is_refused_on_t12_and_unchanged_below_the_fence() {
    // ---- testnet-12, as shipped: the 2026-09-23 fence, the one-move court and the held regime from genesis.
    let p12 = t12();
    assert!(p12.palw_audit_2026_09_23_active_at(0), "t12 arms palw_audit_2026_09_23 at genesis");
    assert!(court_extras(&p12, 1_000, None).shard_court_ladder.is_some(), "t12: the one-move court is armed");
    assert!(court_extras(&p12, 1_000, None).held_context_ladder.is_some(), "t12: the held regime is armed");
    let r = replay(&licensed_claim(p12.clone(), 1_000, None));
    println!("t12 (fence on): {r:?}");
    let why = r.verdict.as_ref().expect_err("t12: the unbound grief must be refused, not open a court");
    assert!(why.contains("does not recompute its committed execution root"), "BindingDoesNotAuthenticate: {why}");
    assert!(matches!(r.resolved.1, PalwClaimPhaseV2::Final { .. }), "t12: the claim reaches Final");
    assert_eq!(r.resolved.0, r.baseline_final, "t12: Final is not delayed by one DAA");
    assert_eq!((r.griefer_delta, r.executor_delta), (0, 0), "t12: nobody is charged");

    // ---- control on the same t12 genesis with the 2026-09-23 fence forced OFF: the v1 verdict.
    let r = replay(&licensed_claim(p12, 1_000, Some(false)));
    println!("t12 genesis, fence forced off: {r:?}");
    let reserved = *r.verdict.as_ref().expect("fence off: v1 opens the dissection");
    assert!(reserved > 0, "the dissection reserves the accuser's exposure");
    assert!(r.resolved.0 > r.baseline_final, "fence off: the grief delays the claim's resolution");

    // ---- testnet-11 as a node resolves it: no 2026-09-23 fence, so the grief opens the court as before.
    let p11 = t11();
    assert!(p11.palw_audit_2026_09_23_fence().is_none(), "t11 never arms palw_audit_2026_09_23");
    let armed = [p11.palw_shard_court_fence(), p11.palw_held_context_fence()]
        .into_iter()
        .map(|f| f.expect("t11 arms the one-move court and the held regime").daa_score())
        .max()
        .unwrap();
    let r = replay(&licensed_claim(p11, armed.max(1_000) + 10, None));
    println!("t11 (court + held armed at {armed}): {r:?}");
    let reserved = *r.verdict.as_ref().expect("t11: below the fence the grief opens the dissection as before");
    assert!(reserved > 0, "the dissection reserves the accuser's exposure");
    // Below the fence the unbound grief is still a live move: t11's class is not held, so C-08 (deep
    // fence) lifts the fused-terminal mercy and the silent responder is convicted rather than excused.
    assert!(r.resolved.0 > r.baseline_final, "t11: the grief moves the claim off its undisturbed Final");
}
