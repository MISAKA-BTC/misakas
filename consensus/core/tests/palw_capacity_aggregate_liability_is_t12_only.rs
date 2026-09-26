//! **ADR-0160 lane liab — F-L (`Params::palw_capacity_aggregate_liability`, the capacity redesign's
//! aggregate bond liability) is dormant on every shipped preset, carries testnet-12's schedule when
//! armed, and arming it — or appending a step — is a scheduled fence like any other** (L-T9).
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//!   release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`), the fold's mirror `None`;
//! * testnet-12's armed value is ONE step at the height — ρ = 10, nothing credited (`q_credit = 0` until
//!   Stage 0 measures an attribution rate, ADR §9) — and the fold's mirror follows the field;
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id` (`Some(never())` is absence — the fourth of the four places); the params id
//!   names the value whole (ρ and the credit), the schedule every step's height; a step appended by a
//!   later flag day moves the params id and the schedule and not the identity, and its height is on the
//!   fork-id schedule, so a node that did not append it is refused from that height;
//! * `validate_palw_v2` refuses it without `palw_rcore_plus` and `palw_offence_attribution` at or below
//!   it, off ConsensusV2, with a value `refusal_v1` refuses, and with the bundle's mirror unsynced;
//! * it is the last entry of `PALW_T12_POST_LAUNCH_FENCES_V1`, so `--palw-drill-fence-at` arms it with the
//!   rest at one height (the crossing itself: `palw_capacity_aggregate_liability.rs`, and the processor's
//!   `t12_post_launch_fences_combined`).

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_aggregate_liability_v1::{PalwCapacityLiabilityV1, PalwCapacityStepV1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as the release ships it (`0e8ec984e`; the shipping re-pin `9c717c16d`): params,
/// identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick for `H_cap`. Never 1,000 — `palw_bond_maturity`'s height on
/// testnet-12, where a second fence would be invisible to the fork id.
const HEIGHTS: [u64; 3] = [900, 1_234, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn step(from_daa: u64, rho: u32, q: u16) -> PalwCapacityStepV1 {
    PalwCapacityStepV1 { from_daa, rho, q_credit_permille: q }
}

/// testnet-12 as shipped with `value` installed and mirrored.
fn with_value(value: Option<PalwCapacityLiabilityV1>) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_capacity_aggregate_liability = value;
    p.sync_palw_capacity_liability();
    p
}

/// testnet-12 as shipped with F-L armed at `height` over testnet-12's schedule.
fn armed_at(height: u64) -> Params {
    with_value(Some(PalwCapacityLiabilityV1::t12_at_v1(ForkActivation::new(height))))
}

fn mirror(p: &Params) -> Option<PalwCapacityLiabilityV1> {
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { return None };
    bundle.state.capacity_liability().cloned()
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

/// **Dormant everywhere as shipped**, the fold's mirror `None`, and testnet-12's ids are the release's.
#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_capacity_aggregate_liability, None, "{name}: F-L ships dormant");
        assert!(!p.palw_capacity_aggregate_liability_active_at(0) && !p.palw_capacity_aggregate_liability_active_at(u64::MAX - 1));
        assert_eq!(p.palw_capacity_step_at_v1(u64::MAX - 1), None, "{name}: no step reaches the fold");
        assert_eq!(mirror(&p), None, "{name}: the fold's mirror is empty");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_capacity_aggregate_liability" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
    let last = PALW_T12_POST_LAUNCH_FENCES_V1.last().expect("the post-launch list");
    assert_eq!(last.name, "palw_capacity_aggregate_liability", "F-L appends to the post-launch list (the capacity release's entry)");
}

/// **testnet-12's armed value: one step at the height, ρ = 10, nothing credited — and the mirror
/// follows**, through the list's own entry too.
#[test]
fn testnet12_s_value_is_rho_10_from_the_height_and_the_fold_s_mirror_follows_it() {
    let armed = armed_at(1_234);
    armed.validate_palw_v2().expect("armed over testnet-12's genesis R-core+ and attribution is a runnable ruleset");
    let value = armed.palw_capacity_aggregate_liability.clone().expect("armed");
    assert_eq!(value.steps, vec![step(1_234, 10, 0)]);
    assert_eq!(mirror(&armed), Some(value.clone()), "the fold reads the field's value");
    assert_eq!(armed.palw_capacity_step_at_v1(1_233), None);
    assert_eq!(armed.palw_capacity_step_at_v1(1_234), Some(step(1_234, 10, 0)));
    let PalwConsensusMode::ConsensusV2(bundle) = &armed.palw_consensus_mode else { panic!("V2") };
    assert_eq!(bundle.state.capacity_step_at(1_234), Some(step(1_234, 10, 0)), "the escrow and shadow lanes' accessor");
    assert!(!bundle.state.capacity_liability_active_at(1_233) && bundle.state.capacity_liability_active_at(1_234));
    // The list's entry: the same value and mirror; set back, the release byte for byte.
    let entry = PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == "palw_capacity_aggregate_liability").expect("listed");
    let mut listed = palw_t12_shipped_params();
    (entry.set)(&mut listed, Some(ForkActivation::new(1_234)));
    assert_eq!(listed.palw_capacity_aggregate_liability, Some(value));
    assert_eq!(format!("{listed:?}"), format!("{armed:?}"), "the entry sets the field and its mirror, nothing else");
    (entry.set)(&mut listed, None);
    assert_eq!(format!("{listed:?}"), format!("{:?}", palw_t12_shipped_params()), "set back: the release");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — and a
/// step appended later moves the params id and the schedule, never the identity. `Some(never())` is
/// absence; at genesis it is a rule from block one.
#[test]
fn arming_or_appending_a_step_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 with F-L at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the operator log names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p), "armed at {height}: the height is in the params id");
    }
    // The value is hashed whole: another ρ or another credit at the same height is another ruleset, and
    // the same schedule (heights only).
    let base = armed_at(1_234);
    for other in [vec![step(1_234, 25, 0)], vec![step(1_234, 10, 150)]] {
        let alt = with_value(Some(PalwCapacityLiabilityV1 { activation: ForkActivation::new(1_234), steps: other.clone() }));
        alt.validate_palw_v2().expect("another legal schedule");
        assert_ne!(ids(&alt).0, ids(&base).0, "{other:?}: the params id names ρ and the credit");
        assert_eq!(ids(&alt).2, ids(&base).2, "{other:?}: the schedule names only heights");
        assert_eq!(ids(&alt).1, identity_id);
    }
    // A later flag day appends a step (ρ = 25 from 4,000): params and schedule move, the identity not.
    let appended = with_value(Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(1_234),
        steps: vec![step(1_234, 10, 150), step(4_000, 25, 150)],
    }));
    appended.validate_palw_v2().expect("an appended step is legal");
    let (ap, ai, as_) = ids(&appended);
    assert_ne!(ap, ids(&base).0);
    assert_ne!(as_, ids(&base).2, "the appended step's height is on the schedule");
    assert_eq!(ai, identity_id, "an appended step nobody has reached is not yet a rule");
    // Some(never()) — the normalised form of a scheduled fence — is absence everywhere.
    let never = with_value(Some(PalwCapacityLiabilityV1::t12_at_v1(ForkActivation::never())));
    assert_eq!(ids(&never).1, identity_id, "Some(never()) is absence in the identity");
    never.validate_palw_v2().expect("never() is dormant");
    assert_eq!(mirror(&never), None, "never() reaches no fold");
    assert!(!never.palw_capacity_aggregate_liability_active_at(u64::MAX - 1));
    // At genesis the rule is in force from block one: another identity (a regenesis, not this lane) —
    // but a step appended at a FUTURE height on a genesis-armed value still leaves the identity alone.
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(ids(&genesis).1, identity_id, "in force at genesis separates identities");
    let genesis_later = with_value(Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(0),
        steps: vec![step(0, 10, 0), step(7_000, 25, 0)],
    }));
    genesis_later.validate_palw_v2().expect("legal");
    assert_eq!(ids(&genesis_later).1, ids(&genesis).1, "a future step on a genesis rule is not yet a rule");
    assert_ne!(ids(&genesis_later).0, ids(&genesis).0, "but the params id names it");
}

/// **The fork id sees the height, and every appended step's height**: below it an armed build and the
/// shipped build keep each other in both directions; from it the armed build refuses the shipped one —
/// and a build that appended a step refuses one that did not from the step's height.
#[test]
fn the_fork_id_gates_the_height_and_every_appended_step() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "F-L at {height} is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "an INDEPENDENT height");
        assert!(armed.fence_schedule_v1().contains(&height), "and on the schedule the fork id is derived from");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            assert!(!evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next).refuses(), "armed keeps shipped at {daa}");
            assert!(!evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next).refuses(), "shipped keeps armed at {daa}");
        }
        let s = fork_id_v1(&shipped, height);
        assert!(evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next).refuses(), "from {height} the armed node refuses");
    }
    // The appended step: the schedule (and so the fork id) names 4,000.
    let one = armed_at(1_234);
    let two = with_value(Some(PalwCapacityLiabilityV1 {
        activation: ForkActivation::new(1_234),
        steps: vec![step(1_234, 10, 0), step(4_000, 25, 0)],
    }));
    assert!(two.fence_schedule_v1().contains(&4_000) && !one.fence_schedule_v1().contains(&4_000));
    for daa in [1_234, 3_999] {
        let o = fork_id_v1(&one, daa);
        assert!(!evaluate_fork_id_v1(&two, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "below the step the two keep each other");
    }
    let o = fork_id_v1(&one, 4_000);
    assert!(evaluate_fork_id_v1(&two, 4_000, o.fired.as_bytes().as_slice(), o.next).refuses(), "from the step's height the old schedule is refused");
}

/// **What `validate_palw_v2` refuses**, each by name.
#[test]
fn the_fence_needs_rcore_plus_the_attribution_fence_a_legal_schedule_and_its_mirror() {
    // No R-core+ (testnet-11, devnet): refused by name.
    for (name, mut p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        p.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1::t12_at_v1(ForkActivation::new(1_234)));
        p.sync_palw_capacity_liability();
        let refused = p.validate_palw_v2();
        println!("{name} armed: {refused:?}");
        assert!(refused.is_err_and(|e| format!("{e:?}").contains("without palw_rcore_plus and palw_offence_attribution")), "{name}");
    }
    // Off ConsensusV2 (testnet-10, a hash-only mainnet): refused by name.
    for (name, mut p) in [("testnet-10", Params::from(TESTNET_PARAMS.net)), ("mainnet", mainnet_shipped_params())] {
        p.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1::t12_at_v1(ForkActivation::new(1_234)));
        let refused = p.validate_palw_v2();
        assert!(refused.is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")), "{name}: refused by name");
    }
    // Over testnet-12 with the attribution fence or R-core+ moved above it: refused.
    for (attribution, rcore) in [(Some(1_235u64), Some(0u64)), (Some(0), Some(1_235)), (None, Some(0))] {
        let mut p = armed_at(1_234);
        p.palw_offence_attribution = attribution.map(ForkActivation::new);
        p.palw_rcore_plus = rcore.map(ForkActivation::new);
        let refused = p.validate_palw_capacity_liability_v1();
        assert!(
            refused.is_err_and(|e| format!("{e:?}").contains("without palw_rcore_plus and palw_offence_attribution")),
            "attribution {attribution:?}, R-core+ {rcore:?}"
        );
    }
    // The value itself.
    let refuse = |steps: Vec<PalwCapacityStepV1>, needle: &str| {
        let p = with_value(Some(PalwCapacityLiabilityV1 { activation: ForkActivation::new(1_234), steps }));
        let refused = p.validate_palw_v2();
        println!("a bad schedule: {refused:?}");
        assert!(refused.is_err_and(|e| format!("{e:?}").contains(needle)), "refused naming {needle:?}");
    };
    refuse(vec![], "names no step");
    refuse(vec![step(1_300, 10, 0)], "first step");
    refuse(vec![step(1_234, 10, 0), step(1_234, 25, 0)], "strictly increasing");
    refuse(vec![step(1_234, 0, 0)], "rho = 0");
    refuse(vec![step(1_234, 10, 1_001)], "1000‰");
    // The mirror: armed and unsynced, or synced and then disarmed, is refused by name.
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_capacity_aggregate_liability = Some(PalwCapacityLiabilityV1::t12_at_v1(ForkActivation::new(1_234)));
    assert!(unsynced.validate_palw_v2().is_err_and(|e| format!("{e:?}").contains("disagrees with the V2 bundle's mirror")));
    let mut stale = armed_at(1_234);
    stale.palw_capacity_aggregate_liability = None;
    assert!(stale.validate_palw_v2().is_err_and(|e| format!("{e:?}").contains("without palw_capacity_aggregate_liability armed")));
    // Over testnet-12's genesis R-core+ and attribution, any height is legal.
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("any height");
    }
}
