//! **W-T10 — ADR-0160 F-W (`Params::palw_capacity_weight_cap`, lane cap-weight) is dormant on every
//! shipped preset, and arming it is a scheduled fence like any other.**
//!
//! The per-bond weight cap (J-1: staged claim weight, `Σ_b min(X_b, ⌊C/6,500 MSK⌋ FCW)`, the capped
//! reservation) is a LATER post-launch change — not the DAA-500 release. So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, the V2 bundle's mirror is `None`,
//!   and testnet-12's three ids are the launch release's to the byte (`b8564b88…` / `5de80e64…` /
//!   `93da24cc…`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, and a `Some(never())` collapses to absence (the fourth of the four places a
//!   Some-only fence needs);
//! * the fork id names the height: below it an armed build and the shipped build keep each other, from
//!   it the armed build refuses the shipped one;
//! * `validate_palw_v2` refuses it off ConsensusV2, without `palw_rcore_plus` or
//!   `palw_reorg_strict_economic_win` at or below it, and with the bundle's mirror apart from the field;
//! * it is an entry of `PALW_T12_POST_LAUNCH_FENCES_V1`, whose `set` moves the field and the mirror
//!   together, so `--palw-drill-fence-at` crosses it with every other entry.
//!
//! The rule itself is `palw_state_v2::tests::capacity_weight_cap_v1` (W-T1 … W-T9).

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as the release ships it (`0e8ec984e`; pinned by the shipping re-pin `9c717c16d` as
/// `palw_clock_lead_cap_is_t12_only::T12_WITH_THE_CAP`): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick: a low one a drill crosses, the capacity release's own later one.
/// Never 500 (the DAA-500 release's) nor 1,000 (`palw_bond_maturity`'s) — a fence at a scheduled
/// height is invisible to the fork id.
const HEIGHTS: [u64; 3] = [60, 2_345, 50_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.capacity_weight_cap_from_daa(),
        _ => None,
    }
}

/// testnet-12 with the strict-economic-win reorg rule at `height` — F-W's prerequisite, and the
/// baseline F-W's own id moves are measured against.
fn strict_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_reorg_strict_economic_win = Some(ForkActivation::new(height));
    p
}

/// …and F-W at the same height, set the way the post-launch list sets it (the field and the mirror).
fn armed_at(height: u64) -> Params {
    let mut p = strict_at(height);
    p.palw_capacity_weight_cap = Some(ForkActivation::new(height));
    p.sync_palw_capacity_weight_cap();
    p
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

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its mirror,
/// its Some-only writers and its collapse cost the live chain nothing until an operator arms it.
#[test]
fn the_weight_cap_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_capacity_weight_cap, None, "{name}: lane cap-weight's fence ships dormant");
        assert_eq!(p.palw_capacity_weight_cap_fence(), None, "{name}");
        assert_eq!(mirror(&p), None, "{name}: the fold's mirror is dormant too");
        assert!(!p.palw_capacity_weight_cap_active_at(0) && !p.palw_capacity_weight_cap_active_at(u64::MAX), "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_capacity_weight_cap" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
        assert_eq!(p.validate_palw_capacity_weight_cap_v1(), Ok(()), "{name}");
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — measured
/// against the same ruleset with only its prerequisite (strict-win) at that height, so the move is
/// F-W's own. A `Some(never())` is absence; at genesis the identity separates.
#[test]
fn arming_the_weight_cap_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (_, identity_id, _) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let base = strict_at(height);
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let ((bp, bi, bs), (p, i, s)) = (ids(&base), ids(&armed));
        println!("testnet-12 with strict-win and the weight cap at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, bp, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, bs, "armed at {height}: the schedule the operator log names it");
        assert_eq!((i.as_str(), bi.as_str()), (identity_id.as_str(), identity_id.as_str()), "armed at {height}: the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert_eq!(armed.palw_capacity_weight_cap_fence(), Some(ForkActivation::new(height)));
        assert_eq!(mirror(&armed), Some(height), "the fold's mirror follows");
        assert!(armed.palw_capacity_weight_cap_active_at(height) && !armed.palw_capacity_weight_cap_active_at(height - 1));
    }
    // Some(never()): absence in every id but the params id's own collapse — the normalised form of a
    // scheduled fence (a-some-only-fence-needs-its-never-collapse).
    let mut never = shipped.clone();
    never.palw_capacity_weight_cap = Some(ForkActivation::never());
    never.sync_palw_capacity_weight_cap();
    never.validate_palw_v2().expect("a never-armed fence is no fence");
    assert_eq!(never.palw_capacity_weight_cap_fence(), None);
    assert_eq!(mirror(&never), None, "never() mirrors as absence");
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    // At genesis the rule is in force from block one: another identity (a regenesis, not this lane).
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height**: F-W is on the gate at its own height (not one testnet-12 already
/// schedules); below it an armed build and the shipped build keep each other in both directions; from
/// it the armed build refuses the shipped one.
#[test]
fn an_armed_build_below_the_weight_cap_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an independent height");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            assert!(!evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next).refuses(), "armed at {height}, DAA {daa}");
            assert!(!evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next).refuses(), "armed at {height}, DAA {daa}");
        }
        let s = fork_id_v1(&shipped, height);
        assert!(
            evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next).refuses(),
            "armed at {height}: from the height the armed node refuses a node that did not upgrade"
        );
    }
}

/// **What `validate_palw_v2` refuses**, each by name: off ConsensusV2; without R-core+ or strict-win at
/// or below it; a field the mirror disagrees with; a mirror with no field. Same height is at or below.
#[test]
fn the_weight_cap_needs_rcore_plus_and_strict_win_at_or_below_it_and_its_mirror() {
    let refused = |mut p: Params, needle: &str| {
        let why = p.validate_palw_capacity_weight_cap_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
        p.palw_capacity_weight_cap = None;
        p.sync_palw_capacity_weight_cap();
    };
    let mut no_strict = armed_at(2_345);
    no_strict.palw_reorg_strict_economic_win = None;
    refused(no_strict, "without palw_reorg_strict_economic_win at or below it");
    let mut late_strict = armed_at(2_345);
    late_strict.palw_reorg_strict_economic_win = Some(ForkActivation::new(2_346));
    refused(late_strict, "without palw_reorg_strict_economic_win at or below it");
    let mut late_rcore = armed_at(2_345);
    late_rcore.palw_rcore_plus = Some(ForkActivation::new(2_346));
    let why = late_rcore.validate_palw_capacity_weight_cap_v1().expect_err("R-core+ above it");
    assert!(format!("{why:?}").contains("without palw_rcore_plus at or below it"), "{why:?}");
    let mut unmirrored = strict_at(2_345);
    unmirrored.palw_capacity_weight_cap = Some(ForkActivation::new(2_345));
    refused(unmirrored, "disagrees with the V2 bundle's mirror");
    let mut orphan = palw_t12_shipped_params();
    orphan.palw_capacity_weight_cap = Some(ForkActivation::new(2_345));
    orphan.sync_palw_capacity_weight_cap();
    orphan.palw_capacity_weight_cap = None;
    let why = orphan.validate_palw_capacity_weight_cap_v1().expect_err("a mirror with no field");
    assert!(format!("{why:?}").contains("without palw_capacity_weight_cap armed"), "{why:?}");
    let mut v1 = MAINNET_PARAMS;
    v1.palw_capacity_weight_cap = Some(ForkActivation::new(2_345));
    assert!(v1.validate_palw_capacity_weight_cap_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (None or never) is legal anywhere.
    for (name, mut p) in presets() {
        p.palw_capacity_weight_cap = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_weight_cap_v1(), Ok(()), "{name}: never() is dormant");
    }
    armed_at(2_345).validate_palw_v2().expect("strict-win at the same height is at or below it");
}

/// **The post-launch list carries F-W**, and its `set` moves the field and the fold's mirror together;
/// the whole list armed at one height (the drill's `--palw-drill-fence-at`; below 1,000, where lane
/// maturity's entry must sit) validates — strict-win is on the list at the same height — and setting it
/// back is the release byte for byte.
#[test]
fn the_post_launch_list_sets_the_weight_cap_and_its_mirror_together() {
    let entry = PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == "palw_capacity_weight_cap").expect("F-W is listed");
    assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().any(|f| f.name == "palw_reorg_strict_economic_win"), "and so is its prerequisite");
    let release = palw_t12_shipped_params();
    let mut armed = release.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(&mut armed, Some(ForkActivation::new(600)));
    }
    armed.validate_palw_v2().expect("the list at one height is a runnable testnet-12 ruleset");
    assert_eq!((armed.palw_capacity_weight_cap, mirror(&armed)), (Some(ForkActivation::new(600)), Some(600)));
    (entry.set)(&mut armed, None);
    assert_eq!((armed.palw_capacity_weight_cap, mirror(&armed)), (None, None), "set to None clears both");
    let mut back = armed.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(&mut back, None);
    }
    assert_eq!(format!("{back:?}"), format!("{release:?}"), "the list is the whole difference");
}
