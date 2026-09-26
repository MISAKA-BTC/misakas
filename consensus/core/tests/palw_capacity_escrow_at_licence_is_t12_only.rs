//! **ADR-0160 lane escrow — F-E (`Params::palw_capacity_escrow_at_licence`, post-launch capacity
//! release) is dormant on every shipped preset, and arming it is a scheduled fence like any other**
//! (E-T9).
//!
//! Past the fence a claim's bond holds `m_c` in its escrow slot instead of the withheld reward `E`, and
//! an unconvicted void keeps the claim's commitment for `window_receipt` (E-4). It ships after the
//! DAA-500 release, behind this fence, which the operator arms at the common capacity height `H_cap`
//! with the rest of ADR-0160's family. So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//!   release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`);
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, so an armed node and a shipped node stay peers until the height (and a
//!   `Some(never())` collapses to absence — a-some-only-fence-needs-its-never-collapse);
//! * the fork id names the height: below it the two builds keep each other; from it the armed build
//!   refuses the shipped one;
//! * `validate_palw_v2` refuses it off ConsensusV2, without option A's slot, R-core+'s ledger and F2's
//!   void at or below it, with the bundle's mirror unsynced, and where claims retire inside the hold.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};

/// testnet-12 as the launch release ships it (`0e8ec984e`): params, identity, schedule — the same
/// triple `palw_model_sink_bound_is_t12_only::T12_RELEASE` pins.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick: a low one a drill crosses, the DAA-500 release's height (the capacity
/// release takes its OWN `H_cap`, but any height is legal), and a later one. Never 1,000.
const HEIGHTS: [u64; 3] = [60, 500, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_capacity_escrow_at_licence = Some(ForkActivation::new(height));
    p.sync_palw_capacity_escrow();
    p
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.capacity_escrow_from_daa(),
        _ => None,
    }
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

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its Some-only
/// writers, its collapse and its mirror cost the live chain nothing until an operator arms it.
#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_capacity_escrow_at_licence, None, "{name}: F-E ships dormant");
        assert_eq!(p.palw_capacity_escrow_at_licence_fence(), None, "{name}");
        assert!(!p.palw_capacity_escrow_active_at(0) && !p.palw_capacity_escrow_active_at(u64::MAX), "{name}");
        assert_eq!(mirror(&p), None, "{name}: the fold's mirror is empty");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_capacity_escrow_at_licence" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not**; the
/// mirror follows; at genesis the identity separates; `Some(never())` is absence.
#[test]
fn arming_the_fence_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 with F-E at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert_eq!(mirror(&armed), Some(height), "armed at {height}: the fold's mirror follows");
        assert!(armed.palw_capacity_escrow_active_at(height) && !armed.palw_capacity_escrow_active_at(height - 1));
    }
    let mut never = shipped.clone();
    never.palw_capacity_escrow_at_licence = Some(ForkActivation::never());
    never.sync_palw_capacity_escrow();
    never.validate_palw_v2().expect("a never-armed fence is no fence");
    assert_eq!(never.palw_capacity_escrow_at_licence_fence(), None, "a never-armed fence arms nothing");
    assert_eq!(mirror(&never), None, "and mirrors nothing");
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height**: on the gate, at its own height, not one testnet-12 already
/// schedules; below it the builds keep each other both ways, from it the armed build refuses.
#[test]
fn an_armed_build_below_the_fence_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            assert!(!evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next).refuses(), "{height}@{daa}");
            assert!(!evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next).refuses(), "{height}@{daa}");
        }
        let s = fork_id_v1(&shipped, height);
        assert!(
            evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next).refuses(),
            "armed at {height}: from the height the armed node refuses a node that did not upgrade"
        );
    }
}

/// **What `validate_palw_v2` refuses**, each by name; at the same height is at or below.
#[test]
fn the_fence_needs_option_a_rcore_and_attribution_at_or_below_it_and_its_mirror() {
    let refused = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = armed_at(500);
        edit(&mut p);
        let why = p.validate_palw_capacity_escrow_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
    };
    let prerequisites = "palw_audit_2026_09_23, palw_rcore_plus and";
    refused(&|p| p.palw_audit_2026_09_23 = Some(ForkActivation::new(501)), prerequisites);
    refused(&|p| p.palw_rcore_plus = None, prerequisites);
    refused(&|p| p.palw_offence_attribution = Some(ForkActivation::never()), prerequisites);
    refused(
        &|p| {
            p.palw_capacity_escrow_at_licence = Some(ForkActivation::new(700));
        },
        "disagrees with the V2 bundle's mirror",
    );
    // A mirror left behind by a disarm is refused too.
    let mut stale = armed_at(500);
    stale.palw_capacity_escrow_at_licence = None;
    assert!(
        stale
            .validate_palw_capacity_escrow_v1()
            .is_err_and(|e| format!("{e:?}").contains("without palw_capacity_escrow_at_licence armed"))
    );
    // At the same height is at or below it.
    let mut same = armed_at(500);
    same.palw_rcore_plus = Some(ForkActivation::new(500));
    same.palw_offence_attribution = Some(ForkActivation::new(500));
    assert_eq!(same.validate_palw_capacity_escrow_v1(), Ok(()));
    // Off ConsensusV2 the fence is refused by name.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_capacity_escrow_at_licence = Some(ForkActivation::new(500));
    assert!(v1.validate_palw_capacity_escrow_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (None or never) is legal anywhere.
    for (name, mut p) in presets() {
        p.palw_capacity_escrow_at_licence = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_escrow_v1(), Ok(()), "{name}: never() is dormant");
    }
    // testnet-12 retires a claim 3,000 DAA after its void, past the 600-DAA hold.
    let t12 = armed_at(500);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("V2") };
    assert!(bundle.state.claim_retirement_daa() > bundle.state.window_receipt(), "the hold ends before the retirement");
}

/// **The post-launch list carries F-E, and its entry mirrors itself**: set on the shipped ruleset (or
/// dormant again) through the list's own setter, the fold's copy follows.
#[test]
fn the_post_launch_entry_sets_the_fence_and_its_mirror() {
    let entry = kaspa_consensus_core::config::params::PALW_T12_POST_LAUNCH_FENCES_V1
        .iter()
        .find(|f| f.name == "palw_capacity_escrow_at_licence")
        .expect("F-E is on the post-launch list (rcore/cap-int)");
    let mut p = palw_t12_shipped_params();
    (entry.set)(&mut p, Some(ForkActivation::new(500)));
    assert_eq!(p.palw_capacity_escrow_at_licence, Some(ForkActivation::new(500)));
    assert_eq!(mirror(&p), Some(500));
    p.validate_palw_v2().expect("F-E alone at 500 on the shipped ruleset validates (the cap-int prerequisites aside)");
    (entry.set)(&mut p, None);
    assert_eq!(mirror(&p), None);
    assert_eq!(ids(&p), ids(&palw_t12_shipped_params()), "set back, the release");
}
