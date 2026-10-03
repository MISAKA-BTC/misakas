//! **testnet-12's int-11 flag day: RFC-0003, RFC-0004 and the capacity ramp to ρ = 25 / ρ = 100** (the user's decision of
//! 2026-10-01, "100倍にして"; the coordinator's of 2026-10-02: the ceilings, the list, the +95 offset).
//!
//! * **the list** — [`PALW_T12_INT11_FENCES_V1`] arms, at ONE height H ([`PALW_T12_INT11_FLAG_DAY_DAA`], 5,300), decode rules, the
//!   generative fence, FP Job V5, the held leaf challenge, the improvement fence, F-N's verification term (`L_ver`) and ρ = 25
//!   (F-L's second step); ρ = 100 (its third) is its own entry at H + 95 ([`PALW_T12_INT11_RHO100_DAA`]). Heights no other fence
//!   uses: the fork id names heights, not fences. `palw_model_court_window` and `palw_model_virtual_v1` stay dormant;
//! * **the baseline** — [`palw_t12_release_v5_params`] (the shipped ruleset with the list dormant) IS the DAA-3,600 release
//!   (int-10, the fleet's): its params and schedule ids are pinned here by value, so this release's re-pin cannot move them;
//! * **the release** — testnet-12 as shipped IS that baseline with the list at H and ρ = 100 at H + 95: params and schedule ids move,
//!   the identity does not, and no other fence moves;
//! * **the ceilings** — testnet-12's provisional ones ([`PALW_T12_GEN_CEILINGS_V1`], [`PALW_T12_IMPROVE_CEILINGS_V1`]): the
//!   generative fence no looser than the IR where the IR bounds the same dimension, the improvement fence's the drill's by value;
//! * **the fork id** — an int-10 node is kept below H in both directions and refused from H; a build with the list but not
//!   ρ = 100 is refused from H + 95;
//! * **the prerequisites** — each entry's, refused by name where it is out of order;
//! * **the drill** — `--palw-drill-int11-at` ([`palw_drill_int11_at_v1`]) moves exactly the list (ρ = 100 to H' + 95) on a salted drill
//!   chain, on a copy, after the flag days it needs below it.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_t12_flag_day_int11`

use kaspa_consensus_core::config::drill::{
    PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_int11_at_v1, palw_drill_model_court_window_at_v1, palw_drill_post_launch_fences_at_v1,
    palw_drill_post_launch_fences_v2_at_v1, palw_drill_post_launch_fences_v3_at_v1, palw_drill_tir_fence2_at_v1, palw_drill_tir_fence_at_v1,
};
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_DECODE_RULES_DAA, PALW_T12_INT11_FENCES_V1, PALW_T12_INT11_FLAG_DAY_DAA, PALW_T12_INT11_RHO100_DAA,
    PALW_T12_INT11_RHO100_FENCES_V1, PALW_T12_INT11_RHO100_OFFSET_DAA, PALW_T12_MODEL_COURT_WINDOW_DAA, PALW_T12_POST_LAUNCH_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_DAA, PALW_T12_TIR_FLAG_DAY_DAA, Params,
    palw_t12_arm_int11_flag_day_at_v1, palw_t12_drill_params_v1, palw_t12_launch_params_v1, palw_t12_release_v1_params,
    palw_t12_release_v2_params, palw_t12_release_v3_params, palw_t12_release_v4_params, palw_t12_release_v5_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_gen_v1::{PALW_T12_GEN_CEILINGS_V1, PalwGenFenceV1};
use kaspa_consensus_core::palw_improve_v1::{PALW_DRILL_IMPROVE_CEILINGS_V1, PALW_T12_IMPROVE_CEILINGS_V1, PalwImprovementFenceV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// The flag day's height H.
const H: u64 = 5_300;
/// ρ = 100's height.
const RHO100: u64 = H + 95;

/// The names of the list, in the order the entries' prerequisites need.
const LIST: [&str; 20] = [
    "palw_fp_decode_rules",
    "palw_gen_v1",
    "palw_fp_job_v5",
    "palw_held_close_chunks_v1",
    "palw_improvement_v1",
    // int-12: RFC-0006, RFC-0007, RFC-0001 and RFC-0002's rest ride the same height.
    "palw_tir_shard_v1",
    "palw_verification_vertex_v1",
    "palw_witness_manifest_v1",
    "palw_audit_mesh_v1",
    "palw_capped_onboarding_v1",
    "palw_fp_decode_constraint",
    "palw_fp_constraint_v2",
    "palw_fp_prefix_state",
    "palw_fp_prefix_inherit",
    "palw_fp_tokenizer_match",
    "palw_adapter_class_v1",
    "palw_class_seating",
    "palw_gdn_key_heads",
    "palw_capacity_network_verify",
    "palw_capacity_aggregate_liability_step_2",
];

/// int-10's params id (the DAA-3,600 release, the fleet's), split so that the re-pin tool does not read it as a pin of the shipped
/// ruleset: it must NOT move when THIS release is re-pinned — that is what the baseline test below is for.
fn int10_params_id() -> String {
    concat!("254509533bb693ced0fed823a4c25e16", "6ba2542d576e4021b0e4b4d6fe4079e1").to_owned()
}

/// int-10's schedule id, split for the same reason.
fn int10_schedule_id() -> String {
    concat!("1e39c738b97a695c8a2c2d4129660eda", "8fa7ac5f1e8b529b916314c01750c593").to_owned()
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x5b; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// `p` with the flag day at `at` (`None`: dormant), through each entry's own `set`.
fn with_list(mut p: Params, at: Option<u64>) -> Params {
    palw_t12_arm_int11_flag_day_at_v1(&mut p, at);
    p
}

#[test]
fn the_flag_day_is_the_list_at_h_and_rho100_at_h_plus_95_heights_no_other_fence_uses() {
    assert_eq!(PALW_T12_INT11_FLAG_DAY_DAA, Some(H), "H (tentative: the user names the final height)");
    assert_eq!(PALW_T12_INT11_RHO100_OFFSET_DAA, 95, "ρ = 100 arms 95 DAA after H (the user's decision of 2026-10-01)");
    assert_eq!(PALW_T12_INT11_RHO100_DAA, Some(RHO100));
    let names: Vec<&str> = PALW_T12_INT11_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, LIST, "the int-11 list, in the order its prerequisites need");
    assert_eq!(PALW_T12_INT11_RHO100_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>(), ["palw_capacity_aggregate_liability_step_3"]);
    // On no earlier list (the DAA-750, the second and the capacity flag days).
    for earlier in PALW_T12_POST_LAUNCH_FENCES_V1.iter().chain(PALW_T12_POST_LAUNCH_FENCES_V2).chain(PALW_T12_POST_LAUNCH_FENCES_V3) {
        assert!(!LIST.contains(&earlier.name) && earlier.name != "palw_capacity_aggregate_liability_step_3", "{} is on an earlier list", earlier.name);
    }
    // The dormant decode-rules list keeps its own (dormant) height; the court window and ADR-0162 stay dormant by decision.
    assert_eq!(PALW_T12_DECODE_RULES_DAA, None, "the decode rules arm with the int-11 list, not with their own dormant one");
    assert_eq!(PALW_T12_MODEL_COURT_WINDOW_DAA, None);
    let shipped = palw_t12_shipped_params();
    assert_eq!(fence_at(&shipped, "palw_model_court_window"), None, "the court window is armed nowhere");
    assert_eq!(fence_at(&shipped, "palw_model_virtual_v1"), None, "ADR-0162's fence is not scheduled: the user has not decided");
    for name in LIST {
        assert_eq!(fence_at(&shipped, name), Some(H), "{name} arms at H");
    }
    assert_eq!(fence_at(&shipped, "palw_capacity_aggregate_liability_step_3"), Some(RHO100));
    // A height no other fence uses.
    for (name, fence) in shipped.palw_fences_v1() {
        let at = fence.map(|f| f.daa_score());
        if !LIST.contains(&name) {
            assert_ne!(at, Some(H), "{name}: 5,300 is the int-11 list's alone");
        }
        if name != "palw_capacity_aggregate_liability_step_3" {
            assert_ne!(at, Some(RHO100), "{name}: 5,395 is ρ = 100's alone");
        }
    }
    let schedule = shipped.fence_schedule_v1();
    for earlier in [750, 1_000, 1_300, 1_700, PALW_T12_TIR_FLAG_DAY_DAA.expect("the IR flag day"), PALW_T12_TIR_FENCE2_DAA.expect("the 3,600 flag day")] {
        assert!(schedule.contains(&earlier), "{earlier}: an earlier flag day, still scheduled ({schedule:?})");
    }
    assert_eq!(&schedule[schedule.len() - 2..], [H, RHO100], "the int-11 flag day's two heights are the schedule's last ({schedule:?})");
}

#[test]
fn the_release_v5_baseline_is_int10s_ruleset_to_the_id() {
    let baseline = palw_t12_release_v5_params();
    let (params, identity, schedule) = ids(&baseline);
    assert_eq!(params, int10_params_id(), "the baseline is the fleet's int-10 ruleset");
    assert_eq!(schedule, int10_schedule_id(), "…and its schedule");
    for name in LIST.iter().chain(["palw_capacity_aggregate_liability_step_3"].iter()) {
        assert_eq!(fence_at(&baseline, name), None, "{name} is dormant on the int-10 baseline");
    }
    assert_eq!(fence_at(&baseline, "palw_tir_fence2"), PALW_T12_TIR_FENCE2_DAA, "the DAA-3,600 flag day is the baseline's");
    baseline.validate_palw_v2().expect("the int-10 ruleset validates on this build");
    // …and it is int-8's with the DAA-3,600 flag day armed (the chain of baselines is unbroken): v4 with fence2 set.
    let mut armed = palw_t12_release_v4_params();
    for f in kaspa_consensus_core::config::params::PALW_T12_TIR_FENCE2_FENCES_V1 {
        (f.set)(&mut armed, Some(ForkActivation::new(PALW_T12_TIR_FENCE2_DAA.expect("the 3,600 flag day"))));
    }
    assert_eq!(ids(&armed), (params, identity, schedule), "int-10 = int-8 + the DAA-3,600 flag day");
    // Every older baseline carries the list dormant too.
    for (name, p) in [
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_release_v4_params", palw_t12_release_v4_params()),
    ] {
        for entry in LIST {
            assert_eq!(fence_at(&p, entry), None, "{name}: {entry} is dormant");
        }
        assert_eq!(fence_at(&p, "palw_capacity_aggregate_liability_step_3"), None, "{name}: ρ = 100");
        p.validate_palw_v2().unwrap_or_else(|e| panic!("{name} validates: {e}"));
    }
}

#[test]
fn testnet12_ships_the_list_armed_at_h_over_the_int10_release() {
    let baseline = palw_t12_release_v5_params();
    let shipped = palw_t12_shipped_params();
    shipped.validate_palw_v2().expect("testnet-12 as shipped validates");
    assert_eq!(ids(&shipped), ids(&with_list(baseline.clone(), Some(H))), "testnet-12 as shipped arms the list at 5,300 over int-10");
    assert_eq!(ids(&Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))), ids(&shipped), "the node's door");
    let (bp, bi, bs) = ids(&baseline);
    let (sp, si, ss) = ids(&shipped);
    assert_ne!(sp, bp, "the ruleset names the fences");
    assert_ne!(ss, bs, "the schedule names 5,300 and 5,395");
    assert_eq!(si, bi, "the identity does not move: a future height is not an identity");
    // No other fence moved.
    for ((name, now), (was_name, was)) in shipped.palw_fences_v1().into_iter().zip(baseline.palw_fences_v1()) {
        assert_eq!(name, was_name);
        let moved_here = LIST.contains(&name) || name == "palw_capacity_aggregate_liability_step_3";
        if moved_here {
            assert_eq!(was, None, "{name} is dormant on the int-10 baseline");
            assert!(now.is_some(), "{name} is armed");
        } else {
            assert_eq!(now, was, "{name}: moved by the int-11 flag day");
        }
    }
    // `set(None)` gives int-10 back, to the id; `set(Some(never()))` keeps the identity and is dormant.
    assert_eq!(ids(&with_list(shipped.clone(), None)), ids(&baseline), "set(None): the int-10 ruleset");
    let mut never = shipped.clone();
    for f in PALW_T12_INT11_RHO100_FENCES_V1.iter().chain(PALW_T12_INT11_FENCES_V1.iter().rev()) {
        (f.set)(&mut never, Some(ForkActivation::never()));
    }
    never.validate_palw_v2().expect("never() is dormant");
    assert_eq!(ids(&never).1, bi, "never(): the identity");
    // The params id is NOT asked to collapse: a scheduled `never()` is named in it (u64::MAX), as every other t12 fence's own test
    // has it (palw_capacity_weight_cap_is_t12_only, palw_capacity_aggregate_liability_is_t12_only) — absence is the identity's.
    assert_ne!(ids(&never).0, ids(&shipped).0, "never(): not the armed release");
}

#[test]
fn the_rules_are_live_from_h_and_rho_steps_up_at_h_and_at_h_plus_95() {
    let shipped = palw_t12_shipped_params();
    assert!(!shipped.palw_gen_v1_active_at(H - 1) && shipped.palw_gen_v1_active_at(H));
    assert!(!shipped.palw_fp_decode_rules_active_at(H - 1) && shipped.palw_fp_decode_rules_active_at(H));
    assert!(!shipped.palw_fp_job_v5_active_at(H - 1) && shipped.palw_fp_job_v5_active_at(H));
    assert!(!shipped.palw_held_close_chunks_active_at(H - 1) && shipped.palw_held_close_chunks_active_at(H));
    assert!(!shipped.palw_improvement_v1_active_at(H - 1) && shipped.palw_improvement_v1_active_at(H));
    let PalwConsensusMode::ConsensusV2(bundle) = &shipped.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!(bundle.state.capacity_network_verify_level_at(H - 1), None, "L_ver is not in force below H");
    assert_eq!(bundle.state.capacity_network_verify_level_at(H), Some(435), "L_ver = 435 at the shipped 600 / 20, from H");
    let rho = |daa: u64| shipped.palw_capacity_step_at_v1(daa).map(|s| s.rho);
    for (daa, want) in [(1_700, 10), (H - 1, 10), (H, 25), (RHO100 - 1, 25), (RHO100, 100), (RHO100 + 10_000, 100)] {
        assert_eq!(rho(daa), Some(want), "ρ at {daa}");
    }
    // The values the fences carry: testnet-12's ceilings, provisional.
    assert_eq!(shipped.palw_gen_v1, Some(PalwGenFenceV1::testnet12_v1(ForkActivation::new(H))));
    assert_eq!(shipped.palw_improvement_v1, Some(PalwImprovementFenceV1::testnet12_v1(ForkActivation::new(H))));
    assert_eq!(shipped.palw_gen_v1.unwrap().ceilings, PALW_T12_GEN_CEILINGS_V1);
    assert_eq!(shipped.palw_improvement_v1.unwrap().ceilings, PALW_T12_IMPROVE_CEILINGS_V1);
}

#[test]
fn the_ceilings_are_no_looser_than_the_ir_and_the_improvement_ones_are_the_drills_by_value() {
    let gen_ceilings = PALW_T12_GEN_CEILINGS_V1;
    let ir = kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_CEILINGS_V1;
    for profile in kaspa_consensus_core::palw_gen_v1::PalwGenProfileV1::ALL {
        let c = gen_ceilings.of(profile);
        assert_eq!(c.max_position_macs, ir.max_macs_per_position, "{profile:?}: MACs a position, the IR's 2^37");
        assert_eq!(c.max_job_cone_work, ir.max_cone_work, "{profile:?}: cone work, the IR's 2^16");
        assert!(c.max_state_bytes <= ir.max_state_bytes, "{profile:?}: state bytes no looser than the IR's");
        assert_eq!(
            (c.max_position_step_leaves, c.max_job_macs, c.max_job_transcendentals, c.max_job_step_leaves, c.max_stages, c.max_class_bytes, c.max_inflight_claims),
            (1 << 22, 1 << 46, 1 << 40, 1 << 30, 16, 1 << 21, 16),
            "{profile:?}: the drills' values everywhere else"
        );
    }
    gen_ceilings.within_format_caps().expect("within the format caps");
    assert_eq!(PALW_T12_IMPROVE_CEILINGS_V1, PALW_DRILL_IMPROVE_CEILINGS_V1, "the improvement ceilings are the drill's, by value");
    PALW_T12_IMPROVE_CEILINGS_V1.within_format_caps().expect("within the format caps");
    // A drill drills what ships.
    assert_eq!(PalwGenFenceV1::drill_v1(ForkActivation::new(7)), PalwGenFenceV1::testnet12_v1(ForkActivation::new(7)));
    assert_eq!(PalwImprovementFenceV1::drill_v1(ForkActivation::new(7)), PalwImprovementFenceV1::testnet12_v1(ForkActivation::new(7)));
}

#[test]
fn the_fork_id_keeps_an_int10_node_below_h_and_refuses_it_from_h() {
    let old = palw_t12_release_v5_params();
    let new = palw_t12_shipped_params();
    for daa in [3_700, 4_500, H - 1] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert_eq!(n.next, H, "the new build announces 5,300 next");
        assert!(!evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new keeps old at {daa}");
        assert!(!evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old keeps new at {daa}");
    }
    for daa in [H, H + 1, RHO100 + 1] {
        let o = fork_id_v1(&old, daa);
        let n = fork_id_v1(&new, daa);
        assert!(evaluate_fork_id_v1(&new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "new refuses old at {daa}");
        assert!(evaluate_fork_id_v1(&old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "old refuses new at {daa}");
    }
    // Between the two heights the new build announces ρ = 100 next; a build that has the list but not ρ = 100 is refused from H + 95.
    let mut partial = old.clone();
    for f in PALW_T12_INT11_FENCES_V1 {
        (f.set)(&mut partial, Some(ForkActivation::new(H)));
    }
    for daa in [H, H + 50, RHO100 - 1] {
        let n = fork_id_v1(&new, daa);
        let p = fork_id_v1(&partial, daa);
        assert_eq!(n.next, RHO100, "ρ = 100 is announced next at {daa}");
        assert!(!evaluate_fork_id_v1(&new, daa, p.fired.as_bytes().as_slice(), p.next).refuses(), "new keeps the list-only build at {daa}");
        assert!(!evaluate_fork_id_v1(&partial, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "…and it keeps new at {daa}");
    }
    let (n, p) = (fork_id_v1(&new, RHO100), fork_id_v1(&partial, RHO100));
    assert!(evaluate_fork_id_v1(&new, RHO100, p.fired.as_bytes().as_slice(), p.next).refuses(), "a build without ρ = 100 is refused from H + 95");
    assert!(evaluate_fork_id_v1(&partial, RHO100, n.fired.as_bytes().as_slice(), n.next).refuses(), "…and refuses the build that has it");
}

fn entry(name: &str) -> &'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1 {
    PALW_T12_INT11_FENCES_V1.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} is on the list"))
}

fn refused_opt(p: &Params) -> Option<String> {
    p.validate_palw_v2().err().map(|e| format!("{e:?}"))
}

#[test]
fn the_list_holds_its_prerequisites_by_name() {
    let baseline = palw_t12_release_v5_params();
    let refused = |p: &Params, why: &str| -> String {
        let e = p.validate_palw_v2().expect_err(why);
        format!("{e:?}")
    };
    let at = |h: u64| Some(ForkActivation::new(h));
    // The generative fence needs the IR fence at or below it (2,000).
    let ir = PALW_T12_TIR_FLAG_DAY_DAA.expect("the IR flag day");
    let mut low = baseline.clone();
    (PALW_T12_INT11_FENCES_V1[1].set)(&mut low, at(ir - 1));
    assert!(refused(&low, "gen below the IR fence").contains("palw_tir"), "gen below palw_tir_v1");
    // FP Job V5 and the improvement fence need the generative fence AND the decode rules at or below them.
    let mut no_decode = baseline.clone();
    for f in &PALW_T12_INT11_FENCES_V1[1..] {
        (f.set)(&mut no_decode, at(H));
    }
    refused(&no_decode, "V5 and the improvement fence without the decode rules");
    let mut no_gen = baseline.clone();
    for f in PALW_T12_INT11_FENCES_V1 {
        (f.set)(&mut no_gen, if f.name == "palw_gen_v1" { None } else { at(H) });
    }
    refused(&no_gen, "V5 and the improvement fence without the generative fence");
    let mut late_gen = baseline.clone();
    for f in PALW_T12_INT11_FENCES_V1 {
        (f.set)(&mut late_gen, at(if f.name == "palw_gen_v1" { H + 1 } else { H }));
    }
    refused(&late_gen, "a generative fence after the fences that need it");
    // L_ver needs F-N (the network room, DAA 1,700) at or below it.
    let mut early_verify = baseline.clone();
    (entry("palw_capacity_network_verify").set)(&mut early_verify, at(1_699));
    refused(&early_verify, "L_ver below F-N");
    // ρ = 25 needs F-L's ρ = 10 step below it, ρ = 100 a ρ = 25 step below it and heights that strictly increase.
    let panics = |mut p: Params, entry: &'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1, h: u64| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || (entry.set)(&mut p, at(h)))).is_err()
    };
    assert!(panics(baseline.clone(), &PALW_T12_INT11_RHO100_FENCES_V1[0], RHO100), "ρ = 100 without ρ = 25");
    let mut steps = with_list(baseline.clone(), Some(H));
    (PALW_T12_INT11_RHO100_FENCES_V1[0].set)(&mut steps, at(H));
    refused(&steps, "ρ = 100 at ρ = 25's own height");
    let mut under = with_list(baseline.clone(), Some(H));
    (entry("palw_capacity_aggregate_liability_step_2").set)(&mut under, at(1_700));
    refused(&under, "ρ = 25 at ρ = 10's own height");
    // int-12's entries: each one's prerequisite inside the list, refused where it is missing or later than its dependant.
    for (dependant, prerequisite) in [
        ("palw_witness_manifest_v1", "palw_verification_vertex_v1"),
        ("palw_audit_mesh_v1", "palw_verification_vertex_v1"),
        ("palw_capped_onboarding_v1", "palw_audit_mesh_v1"),
        ("palw_fp_constraint_v2", "palw_fp_decode_constraint"),
        ("palw_fp_prefix_inherit", "palw_fp_prefix_state"),
        ("palw_adapter_class_v1", "palw_improvement_v1"),
        ("palw_tir_shard_v1", "palw_verification_vertex_v1"),
    ] {
        let _ = dependant;
        let mut missing = with_list(baseline.clone(), Some(H));
        (entry(prerequisite).set)(&mut missing, None);
        let mut later = with_list(baseline.clone(), Some(H));
        (entry(prerequisite).set)(&mut later, at(H + 1));
        let failures = [refused_opt(&missing), refused_opt(&later)];
        assert!(failures.iter().all(Option::is_some), "{dependant} without {prerequisite} at or below it is refused ({failures:?})");
    }
    // Everything in order validates, and so does ρ = 100 a DAA after ρ = 25.
    let mut ok = with_list(baseline, Some(H));
    (PALW_T12_INT11_RHO100_FENCES_V1[0].set)(&mut ok, at(H + 1));
    ok.validate_palw_v2().expect("ρ = 100 one DAA after ρ = 25 is in order");
}

/// A drill ruleset with the flag days the int-11 list needs below it, as the drill's frozen order moves them.
fn drill_with_the_earlier_flag_days_low() -> Params {
    let mut p = palw_t12_drill_params_v1(&salt());
    palw_drill_post_launch_fences_at_v1(&mut p, 6).expect("--palw-drill-fence-at=6");
    palw_drill_post_launch_fences_v2_at_v1(&mut p, 10).expect("--palw-drill-fence2-at=10");
    palw_drill_post_launch_fences_v3_at_v1(&mut p, 14).expect("--palw-drill-fence3-at=14");
    palw_drill_tir_fence_at_v1(&mut p, 20).expect("--palw-drill-tir-at=20");
    palw_drill_tir_fence2_at_v1(&mut p, 24).expect("--palw-drill-tir2-at=24");
    p
}

#[test]
fn the_drill_crosses_the_whole_list_at_a_low_height_with_rho100_95_later_and_moves_nothing_else() {
    let before = drill_with_the_earlier_flag_days_low();
    // The drill ruleset arms the release's list at H like every ruleset: the move is a MOVE.
    assert_eq!(fence_at(&before, "palw_gen_v1"), Some(H));
    let mut moved = before.clone();
    let moves = palw_drill_int11_at_v1(&mut moved, 28).expect("a salted drill crosses the int-11 flag day low");
    assert_eq!(moves.len(), LIST.len() + 1, "the list and ρ = 100");
    for m in &moves {
        let want = if m.name == "palw_capacity_aggregate_liability_step_3" { 28 + 95 } else { 28 };
        assert_eq!(m.at, want, "{}", m.name);
        assert!(m.was.is_some(), "{} was armed at the release's height on the drill ruleset", m.name);
    }
    moved.validate_palw_v2().expect("the result validates");
    for ((name, was), (_, now)) in before.palw_fences_v1().iter().zip(moved.palw_fences_v1().iter()) {
        if LIST.contains(name) {
            assert_eq!(now.map(|f| f.daa_score()), Some(28), "{name}");
        } else if *name == "palw_capacity_aggregate_liability_step_3" {
            assert_eq!(now.map(|f| f.daa_score()), Some(28 + 95), "{name}");
        } else {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    let rho = |p: &Params, daa: u64| p.palw_capacity_step_at_v1(daa).map(|s| s.rho);
    for (daa, want) in [(27, 10), (28, 25), (122, 25), (123, 100)] {
        assert_eq!(rho(&moved, daa), Some(want), "ρ at {daa}");
    }
    assert_eq!(ids(&moved).1, ids(&before).1, "a future height: the identity is the drill's");
    // Refusals leave the ruleset as it came.
    for (at, why) in [(0, "genesis"), (u64::MAX, "never"), (u64::MAX - 10, "ρ = 100 past the last DAA")] {
        let mut p = before.clone();
        assert!(palw_drill_int11_at_v1(&mut p, at).is_err(), "{why}");
        assert_eq!(ids(&p), ids(&before), "{why}: untouched");
    }
    let mut same = before.clone();
    assert!(palw_drill_int11_at_v1(&mut same, 24).is_err(), "another fence's height (the second IR fence's)");
    assert_eq!(ids(&same), ids(&before), "refused untouched");
    let mut rho100_collides = before.clone();
    palw_drill_model_court_window_at_v1(&mut rho100_collides, 28 + 95).expect("the (dormant-on-testnet-12) court window at ρ = 100's future height");
    let was = ids(&rho100_collides);
    let why = palw_drill_int11_at_v1(&mut rho100_collides, 28).expect_err("ρ = 100's height is another fence's");
    assert!(why.contains("palw_model_court_window") && why.contains("fork id"), "{why}");
    assert_eq!(ids(&rho100_collides), was, "refused on the second height: the first move was on a copy, the ruleset is as it came");
    assert_eq!(fence_at(&rho100_collides, "palw_gen_v1"), Some(H), "…the list is where it was");
    // The flag days it needs below it, or it is refused by name and the ruleset is untouched.
    let bare = palw_t12_drill_params_v1(&salt());
    let mut p = bare.clone();
    assert!(palw_drill_int11_at_v1(&mut p, 28).is_err(), "the IR flag days and the capacity flag day are above 28 on the bare drill ruleset");
    assert_eq!(ids(&p), ids(&bare));
    // Public testnet-12's genesis and another network: never.
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_int11_at_v1(&mut public, 28).is_err(), "public testnet-12's genesis");
    let mut mainnet = kaspa_consensus_core::config::params::mainnet_shipped_params();
    assert!(palw_drill_int11_at_v1(&mut mainnet, 28).is_err(), "another network");
}

#[test]
fn a_per_fence_drill_flag_moves_its_entry_and_keeps_the_rest_of_the_release_armed() {
    use kaspa_consensus_core::config::drill::{palw_drill_capacity_step2_at_v1, palw_drill_capacity_step3_at_v1};
    // ρ = 25 alone, low: the release's ρ = 100 (H + 95) stays after it — the move keeps the later steps that stay in order.
    let mut p = drill_with_the_earlier_flag_days_low();
    palw_drill_capacity_step2_at_v1(&mut p, 560).expect("--palw-drill-capacity-step2-at=560");
    assert_eq!(fence_at(&p, "palw_capacity_aggregate_liability_step_2"), Some(560));
    assert_eq!(fence_at(&p, "palw_capacity_aggregate_liability_step_3"), Some(RHO100), "ρ = 100 stays at the release's height");
    // ρ = 100 below ρ = 25's release height needs ρ = 25 moved first: the ramp's heights strictly increase.
    let mut q = drill_with_the_earlier_flag_days_low();
    assert!(palw_drill_capacity_step3_at_v1(&mut q, 655).is_err(), "ρ = 100 at 655 under the release's ρ = 25 at 5,300");
    palw_drill_capacity_step2_at_v1(&mut q, 560).expect("ρ = 25 low");
    palw_drill_capacity_step3_at_v1(&mut q, 655).expect("and ρ = 100 above it");
    assert_eq!((fence_at(&q, "palw_capacity_aggregate_liability_step_2"), fence_at(&q, "palw_capacity_aggregate_liability_step_3")), (Some(560), Some(655)));
    q.validate_palw_v2().expect("validates");
}

/// **The release's fence table, printed for the review and the report** (`--nocapture`): every testnet-12 fence with a height past genesis,
/// by height, the three ids of the shipped ruleset and of the int-10 baseline, and the fork id's schedule. Asserts nothing the tests above
/// do not: it is the table the release is announced with.
#[test]
fn print_the_release_fence_table() {
    let shipped = palw_t12_shipped_params();
    let baseline = palw_t12_release_v5_params();
    let mut rows: Vec<(u64, &str)> = shipped
        .palw_fences_v1()
        .into_iter()
        .filter_map(|(name, f)| f.map(|f| (f.daa_score(), name)))
        .filter(|(at, _)| *at != 0 && *at != u64::MAX)
        .collect();
    rows.sort();
    println!("FENCE TABLE (testnet-12 as shipped; heights past genesis):");
    for (at, name) in &rows {
        let note = if baseline.palw_fences_v1().iter().any(|(n, f)| n == name && f.map(|f| f.daa_score()) == Some(*at)) { "" } else { "  <- int-11" };
        println!("  {at:>6}  {name}{note}");
    }
    let (sp, si, ss) = ids(&shipped);
    let (bp, bi, bs) = ids(&baseline);
    println!("SHIPPED  params {sp}\n         identity {si}\n         schedule {ss}");
    println!("INT-10   params {bp}\n         identity {bi}\n         schedule {bs}");
    println!("SCHEDULE {:?}", shipped.fence_schedule_v1());
    println!(
        "CEILINGS gen: macs/position 2^{} cone work 2^{}; improve: {} candidates, {} items",
        PALW_T12_GEN_CEILINGS_V1.image.max_position_macs.trailing_zeros(),
        PALW_T12_GEN_CEILINGS_V1.image.max_job_cone_work.trailing_zeros(),
        PALW_T12_IMPROVE_CEILINGS_V1.max_candidates_per_epoch,
        PALW_T12_IMPROVE_CEILINGS_V1.max_items_per_epoch
    );
    assert!(rows.iter().any(|(at, name)| *at == H && *name == "palw_gen_v1"));
}
