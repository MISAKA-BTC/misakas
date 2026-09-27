//! **ADR-0160 stage 2 — F-Q (`Params::palw_capacity_audit_door`, the audit door) and F-S
//! (`Params::palw_capacity_issuance_slots`, the issuance slots) are dormant on every shipped preset, and
//! arming either is a scheduled fence like any other** (rcore/cap-s1).
//!
//! As shipped both fields are `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//! DAA-1,300 release's to the byte; armed at a future height each moves `consensus_params_id` and
//! `consensus_schedule_id` but NOT `consensus_identity_id`; `Some(never())` collapses to absence; the
//! fork id gates on the height; `validate_palw_v2` refuses each off ConsensusV2, without its
//! prerequisites at or below it (F-Q: F-L, F-B, lane A; F-S: F-W, F-L, F-E), with a bundle mirror that
//! disagrees, and a CREDITED step of F-L that starts below F-Q (D-23). Both are entries of
//! `PALW_T12_CAPACITY_FENCES_V1`, never of the DAA-750 list.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_arm_capacity_fences_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as THIS build ships it — the DAA-1,300 release (rcore/int-5), every capacity
/// fence dormant (rcore/cap-s1): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a",
);

/// Heights past the release's 750 (where the capacity fences' prerequisites are).
const HEIGHTS: [u64; 3] = [1_001, 1_500, 5_000];

const NAMES: [&str; 2] = ["palw_capacity_audit_door", "palw_capacity_issuance_slots"];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12", palw_t12_shipped_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

/// `(F-Q's height, the pool's operators, F-S's height)` as the fold's bundle holds them.
fn mirrors(p: &Params) -> (Option<u64>, usize, Option<u64>) {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => (
            bundle.state.capacity_audit_from_daa(),
            bundle.state.capacity_audit_operators().len(),
            bundle.state.capacity_slots_from_daa(),
        ),
        _ => (None, 0, None),
    }
}

fn entry(name: &str) -> &'static kaspa_consensus_core::config::params::PalwPostLaunchFenceV1 {
    PALW_T12_CAPACITY_FENCES_V1.iter().find(|f| f.name == name).expect("listed")
}

/// The release with every stage-1 capacity fence armed at `h` (F-W, F-E, F-L, F-B, F-R) and stage 2's
/// two left dormant (and every later stage's: F-N needs F-S).
fn stage1_at(h: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    for f in PALW_T12_CAPACITY_FENCES_V1.iter().filter(|f| !NAMES.contains(&f.name) && f.name != "palw_capacity_network_room") {
        (f.set)(&mut p, Some(ForkActivation::new(h)));
    }
    p.validate_palw_v2().expect("stage 1's list validates over the release");
    p
}

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's.
#[test]
fn the_fences_are_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!((p.palw_capacity_audit_door, p.palw_capacity_issuance_slots), (None, None), "{name}: stage 2 ships dormant");
        assert_eq!((p.palw_capacity_audit_door_fence(), p.palw_capacity_issuance_slots_fence()), (None, None), "{name}");
        assert_eq!(mirrors(&p), (None, 0, None), "{name}: the fold's mirrors are empty");
        for fence in NAMES {
            assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == fence && f.is_none()), "{name}: {fence} is on the fork-id list");
        }
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not**; the
/// mirrors follow (F-Q's carries lane A's eight operator bonds); `Some(never())` is absence; the fork id
/// gates on the height.
#[test]
fn arming_either_fence_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (_, identity, _) = ids(&shipped);
    for h in HEIGHTS {
        let base = stage1_at(h);
        let (params_id, _, schedule_id) = ids(&base);
        for name in NAMES {
            let mut armed = base.clone();
            (entry(name).set)(&mut armed, Some(ForkActivation::new(h)));
            armed.validate_palw_v2().unwrap_or_else(|e| panic!("{name} at {h}: a runnable ruleset: {e:?}"));
            let (p, i, s) = ids(&armed);
            assert_ne!(p, params_id, "{name} at {h}: the ruleset names it");
            assert_ne!(s, schedule_id, "{name} at {h}: the schedule names it");
            assert_eq!(i, identity, "{name} at {h}: the identity does not move");
            let fence = match name {
                "palw_capacity_audit_door" => armed.palw_capacity_audit_door_fence(),
                _ => armed.palw_capacity_issuance_slots_fence(),
            };
            assert_eq!(fence, Some(ForkActivation::new(h)));
            let m = mirrors(&armed);
            match name {
                "palw_capacity_audit_door" => assert_eq!(m, (Some(h), 8, None), "F-Q's mirror carries lane A's eight operators"),
                _ => assert_eq!(m, (None, 0, Some(h)), "F-S's mirror"),
            }
            let mut never = base.clone();
            (entry(name).set)(&mut never, Some(ForkActivation::never()));
            never.validate_palw_v2().expect("a never-armed fence is no fence");
            assert_eq!(mirrors(&never), mirrors(&base), "{name}: a never-armed fence mirrors nothing");
            assert_eq!(never.consensus_identity_id().to_string(), identity, "{name}: Some(never()) is absence in the identity");
            // The gate reads heights: against the release (no capacity fence), the armed node refuses from
            // `h`. (Against a node that already arms stage 1 at the same `h` it cannot — a fence at an
            // already-scheduled height is invisible to the fork id — which is why the capacity list arms
            // at ONE height and a later stage is its own flag day.)
            let s = fork_id_v1(&shipped, h);
            assert!(
                evaluate_fork_id_v1(&armed, h, s.fired.as_bytes().as_slice(), s.next).refuses(),
                "{name} at {h}: from the height the armed node refuses a node that did not upgrade"
            );
            (entry(name).set)(&mut armed, None);
            assert_eq!(ids(&armed), ids(&base), "{name}: set back, the base");
        }
    }
}

/// **What `validate_palw_v2` refuses**, each by name.
#[test]
fn the_fences_need_their_prerequisites_their_mirrors_and_a_credit_needs_the_door() {
    let h = 1_500;
    let refused = |p: &Params, needle: &str| {
        let why = p.validate_palw_capacity_stage2_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
    };
    // F-Q without F-L / F-B / lane A at or below it.
    let mut q = palw_t12_shipped_params();
    (entry("palw_capacity_audit_door").set)(&mut q, Some(ForkActivation::new(h)));
    refused(&q, "palw_capacity_audit_door is armed without");
    // F-S without F-W / F-L / F-E at or below it.
    let mut s = palw_t12_shipped_params();
    (entry("palw_capacity_issuance_slots").set)(&mut s, Some(ForkActivation::new(h)));
    refused(&s, "palw_capacity_issuance_slots is armed without");
    // A mirror that disagrees.
    let mut m = stage1_at(h);
    m.palw_capacity_issuance_slots = Some(ForkActivation::new(h));
    refused(&m, "disagrees with the V2 bundle's mirror");
    // A credited step (q ≥ q_seat) with no audit door at or below it.
    let mut c = stage1_at(h);
    c.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(h),
        steps: vec![PalwCapacityStepV1 { from_daa: h, rho: 10, q_credit_permille: 250 }],
    });
    c.sync_palw_capacity_liability();
    refused(&c, "credits a step without palw_capacity_audit_door");
    // With the door at the step's height it validates.
    (entry("palw_capacity_audit_door").set)(&mut c, Some(ForkActivation::new(h)));
    c.validate_palw_v2().expect("a credited step at or past the audit door validates");
    // Off ConsensusV2.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_capacity_audit_door = Some(ForkActivation::new(h));
    assert!(v1.validate_palw_capacity_stage2_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (never) is legal anywhere.
    for (name, mut p) in presets() {
        p.palw_capacity_audit_door = Some(ForkActivation::never());
        p.palw_capacity_issuance_slots = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_stage2_v1(), Ok(()), "{name}: never() is dormant");
    }
}

/// **The capacity list carries both, the DAA-750 list neither**; the whole list armed at one height over
/// the release validates.
#[test]
fn the_capacity_list_carries_both_and_arms_whole() {
    for name in NAMES {
        assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != name), "{name} is never on the DAA-750 list");
        assert!(PALW_T12_CAPACITY_FENCES_V1.iter().any(|f| f.name == name), "{name} is on the capacity list");
    }
    let mut p = palw_t12_shipped_params();
    palw_t12_arm_capacity_fences_v1(&mut p, Some(ForkActivation::new(1_001))).expect("the whole capacity list at one height");
    assert_eq!(mirrors(&p), (Some(1_001), 8, Some(1_001)));
}
