//! **W-T10 — ADR-0160 F-W (`Params::palw_capacity_weight_cap`, lane cap-weight) is dormant on every
//! shipped preset, and arming it is a scheduled fence like any other.**
//!
//! The per-bond weight cap (J-1: staged claim weight, `Σ_b min(X_b, ⌊C/6,500 MSK⌋ FCW)`, the capped
//! reservation) is a LATER change — the capacity programme's own flag day, never the DAA-750 release
//! (rcore/cap-s1). So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, the V2 bundle's mirror is `None`,
//!   and testnet-12's three ids are the DAA-750 release's to the byte (`dbbc9104…` / `5de80e64…` /
//!   `7c652212…`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, and a `Some(never())` collapses to absence (the fourth of the four places a
//!   Some-only fence needs);
//! * the fork id names the height: below it an armed build and the shipped build keep each other, from
//!   it the armed build refuses the shipped one;
//! * `validate_palw_v2` refuses it off ConsensusV2, without `palw_rcore_plus`,
//!   `palw_reorg_strict_economic_win` or `palw_operator_anchor` (lane A — ADR-0160 §8 A1: the anchor is
//!   what keeps a private branch from staging claims the public branch cannot match) at or below it,
//!   and with the bundle's mirror apart from the field;
//! * it is an entry of `PALW_T12_CAPACITY_FENCES_V1` — NOT of `PALW_T12_POST_LAUNCH_FENCES_V1`, which the
//!   release arms at DAA 750 — whose `set` moves the field and the mirror together, and the capacity list
//!   armed over the release validates (`palw_drill_capacity_fences_at_v1` crosses it on a drill).
//!
//! The rule itself is `palw_state_v2::tests::capacity_weight_cap_v1` (W-T1 … W-T9).

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_POST_LAUNCH_FENCE_DAA, PALW_T12_POST_LAUNCH_FENCES_V1, Params,
    SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_arm_capacity_fences_v1,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as THIS build ships it — the DAA-750 post-launch release (`c3dbaee3c`, every fence of
/// `PALW_T12_POST_LAUNCH_FENCES_V1` at DAA 750), with F-W dormant: params, identity, schedule — the same
/// pins `palw_operator_anchor_is_t12_only::T12_RELEASE` holds (rcore/cap-s1: the capacity fences do not
/// move them).
const T12_RELEASE: (&str, &str, &str) = (
    "dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397",
);

/// Heights an operator might pick: a low one a drill crosses, the capacity release's own later one.
/// Never 750 (the post-launch release's) nor 1,000 (`palw_bond_maturity`'s) — a fence at a scheduled
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

/// F-W's prerequisites from the post-launch list, each set the way the list sets it: the
/// strict-economic-win reorg rule, lane A's operator anchor, and lane F1's execution seed (lane A's own
/// prerequisite).
const PREREQUISITES: [&str; 3] = ["palw_reorg_strict_economic_win", "palw_panel_seed_execution", "palw_operator_anchor"];

fn set(p: &mut Params, name: &str, at: Option<ForkActivation>) {
    (PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} is listed")).set)(p, at);
}

/// testnet-12 with F-W's prerequisites at or below `height` — the baseline F-W's own id moves are
/// measured against. The release arms them at DAA 750 (with bind-deadlock, which needs lane A at or below
/// it), so a height past 750 keeps the release's own ruleset and a lower one moves the three down to it.
fn prerequisites_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    if height < PALW_T12_POST_LAUNCH_FENCE_DAA {
        for name in PREREQUISITES {
            set(&mut p, name, Some(ForkActivation::new(height)));
        }
    }
    p
}

/// …and F-W at the same height, set the way the post-launch list sets it (the field and the mirror).
fn armed_at(height: u64) -> Params {
    let mut p = prerequisites_at(height);
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
/// against the same ruleset with only its prerequisites (strict-win, lane A and its seed) at that height,
/// so the move is F-W's own. A `Some(never())` is absence; at genesis the identity separates.
#[test]
fn arming_the_weight_cap_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (_, identity_id, _) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let base = prerequisites_at(height);
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let ((bp, bi, bs), (p, i, s)) = (ids(&base), ids(&armed));
        println!("testnet-12 with F-W's prerequisites and the weight cap at {height}: params {p} identity {i} schedule {s}");
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
            assert!(
                !evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next).refuses(),
                "armed at {height}, DAA {daa}"
            );
            assert!(
                !evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next).refuses(),
                "armed at {height}, DAA {daa}"
            );
        }
        let s = fork_id_v1(&shipped, height);
        assert!(
            evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next).refuses(),
            "armed at {height}: from the height the armed node refuses a node that did not upgrade"
        );
    }
}

/// **What `validate_palw_v2` refuses**, each by name: off ConsensusV2; without R-core+, strict-win or
/// lane A's operator anchor at or below it; a field the mirror disagrees with; a mirror with no field.
/// Same height is at or below.
#[test]
fn the_weight_cap_needs_rcore_plus_strict_win_and_the_operator_anchor_at_or_below_it_and_its_mirror() {
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
    // ADR-0160 §8 A1: without the anchor a private branch anchors (and grinds) its own panels, stages its
    // claims to its bonds' caps, and wins strict-win against a public branch whose own post-fork claims
    // weigh 0 until bound (`capacity_weight_cap_v1::w_t6b_…`).
    let mut no_anchor = armed_at(2_345);
    set(&mut no_anchor, "palw_operator_anchor", None);
    refused(no_anchor, "without palw_operator_anchor at or below it");
    let mut late_anchor = armed_at(2_345);
    set(&mut late_anchor, "palw_operator_anchor", Some(ForkActivation::new(2_346)));
    refused(late_anchor, "without palw_operator_anchor at or below it");
    let mut late_rcore = armed_at(2_345);
    late_rcore.palw_rcore_plus = Some(ForkActivation::new(2_346));
    let why = late_rcore.validate_palw_capacity_weight_cap_v1().expect_err("R-core+ above it");
    assert!(format!("{why:?}").contains("without palw_rcore_plus at or below it"), "{why:?}");
    let mut unmirrored = prerequisites_at(2_345);
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
    armed_at(2_345).validate_palw_v2().expect("the prerequisites at the same height are at or below it");
}

/// **The capacity list carries F-W, and the DAA-750 list does not** (rcore/cap-s1): the capacity entry's
/// `set` moves the field and the fold's mirror together; the capacity list armed over the release at one
/// height at or past the release's (where its prerequisites strict-win and lane A are) validates, and
/// setting it back is the release byte for byte. Below the release's height it is refused by the
/// prerequisites' names.
#[test]
fn the_capacity_list_sets_the_weight_cap_and_its_mirror_together() {
    assert!(
        PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != "palw_capacity_weight_cap"),
        "F-W is never an entry of the DAA-750 release's list (that list is armed on the live chain)"
    );
    let entry = PALW_T12_CAPACITY_FENCES_V1.iter().find(|f| f.name == "palw_capacity_weight_cap").expect("F-W is listed");
    for name in PREREQUISITES {
        assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().any(|f| f.name == name), "its prerequisite {name} is the release's");
    }
    let release = palw_t12_shipped_params();
    for at in [PALW_T12_POST_LAUNCH_FENCE_DAA + 1, 2_345] {
        let mut armed = release.clone();
        palw_t12_arm_capacity_fences_v1(&mut armed, Some(ForkActivation::new(at))).expect("the capacity list over the release");
        assert_eq!((armed.palw_capacity_weight_cap, mirror(&armed)), (Some(ForkActivation::new(at)), Some(at)));
        (entry.set)(&mut armed, None);
        assert_eq!((armed.palw_capacity_weight_cap, mirror(&armed)), (None, None), "set to None clears both");
        assert_eq!(format!("{armed:?}"), format!("{release:?}"), "the capacity list is the whole difference");
    }
    let mut early = release.clone();
    let why = palw_t12_arm_capacity_fences_v1(&mut early, Some(ForkActivation::new(PALW_T12_POST_LAUNCH_FENCE_DAA - 1)))
        .expect_err("below the release's strict-win and lane A");
    assert!(why.contains("at or below it"), "refused by a prerequisite's name: {why}");
}
