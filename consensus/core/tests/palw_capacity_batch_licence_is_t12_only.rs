//! **ADR-0160 F-B — the batch licence (`Params::palw_capacity_batch_licence`, lane verify, post-launch
//! and later than the DAA-500 release) is dormant on every shipped preset, and arming it is a
//! scheduled fence like any other.**
//!
//! Past it a lifecycle carrier may carry `ReceiptLicensedBatchV1` (tag 59): one window-root signature
//! per seat, a Merkle path per claim, the coverage funnel unchanged. As shipped the field is `None`
//! everywhere, testnet-12 included, and testnet-12's three ids are the release's to the byte; armed at
//! a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//! `consensus_identity_id`; `Some(never())` collapses to absence; the fork id gates on the height;
//! `validate_palw_v2` refuses it off ConsensusV2, without `palw_rcore_plus` and `palw_verification_v2`
//! at or below it, and with a bundle mirror that disagrees.

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_POST_LAUNCH_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as the release ships it (`0e8ec984e`; pinned by the shipping re-pin `9c717c16d`):
/// params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick for the capacity release's `H_cap`: a low one a drill crosses, and
/// later ones — never 500 (the DAA-500 release's) nor 1,000 (`palw_bond_maturity`'s).
const HEIGHTS: [u64; 3] = [60, 1_500, 5_000];

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

fn mirrors(p: &Params) -> (Option<u64>, Option<u64>) {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => (bundle.state.capacity_batch_from_daa(), bundle.state.capacity_room_from_daa()),
        _ => (None, None),
    }
}

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_capacity_batch_licence = Some(ForkActivation::new(height));
    p.sync_palw_capacity_verify();
    p
}

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its
/// Some-only writers, its collapse and its bundle mirror cost the live chain nothing.
#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_capacity_batch_licence, None, "{name}: ADR-0160's fence ships dormant");
        assert_eq!(p.palw_capacity_batch_licence_fence(), None, "{name}");
        assert!(!p.palw_capacity_batch_licence_active_at(0) && !p.palw_capacity_batch_licence_active_at(u64::MAX), "{name}");
        assert_eq!(mirrors(&p), (None, None), "{name}: the fold's mirrors are empty");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_capacity_batch_licence" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
    // The capacity entries are on the post-launch list, so `--palw-drill-fence-at` crosses them.
    let entry = PALW_T12_POST_LAUNCH_FENCES_V1.iter().find(|f| f.name == "palw_capacity_batch_licence").expect("F-B is on the post-launch list");
    let mut set = palw_t12_shipped_params();
    (entry.set)(&mut set, Some(ForkActivation::new(1_500)));
    assert_eq!(set.palw_capacity_batch_licence, Some(ForkActivation::new(1_500)));
    set.validate_palw_v2().expect("the entry sets the field AND the fold's mirror");
    (entry.set)(&mut set, None);
    assert_eq!(set.palw_capacity_batch_licence, None);
    assert_eq!(mirrors(&set), (None, None), "and unsets both");
    assert_eq!(ids(&set), now, "set and unset is the release again");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — an
/// armed node and a shipped node stay peers until the height. Armed at genesis it is a rule in force
/// from block one. A `Some(never())` is absence (`a-some-only-fence-needs-its-never-collapse`).
#[test]
fn arming_the_fence_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 with palw_capacity_batch_licence at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert_eq!(armed.palw_capacity_batch_licence_fence(), Some(ForkActivation::new(height)));
        assert!(armed.palw_capacity_batch_licence_active_at(height) && !armed.palw_capacity_batch_licence_active_at(height - 1));
    }
    let mut never = shipped.clone();
    never.palw_capacity_batch_licence = Some(ForkActivation::never());
    never.sync_palw_capacity_verify();
    never.validate_palw_v2().expect("a never-armed fence is no fence");
    assert_eq!(never.palw_capacity_batch_licence_fence(), None, "a never-armed fence arms nothing");
    assert_eq!(mirrors(&never), (None, None), "and mirrors nothing");
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
    // The two capacity fences are independent heights of one id: both at one height is a third id.
    let mut both = armed_at(1_500);
    both.palw_capacity_verify_room = Some(ForkActivation::new(1_500));
    both.sync_palw_capacity_verify();
    both.validate_palw_v2().expect("F-B and F-R at one common height, H_cap");
    assert!(!seen.contains(&both.consensus_params_id().to_string()));
}

/// **The fork id sees the height**: below it an armed build and the shipped build keep each other in
/// BOTH directions; from it the armed build refuses the shipped one — a named partition at the flag
/// day, never a silent fork.
#[test]
fn an_armed_build_below_the_fence_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height, not one the release schedules");
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

/// **What `validate_palw_v2` refuses** — its prerequisites at or below it, ConsensusV2, and a bundle
/// mirror that disagrees with the fence (a missed `sync_palw_capacity_verify` is a startup refusal).
#[test]
fn validate_names_the_prerequisites_and_the_mirror() {
    let refused = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = armed_at(1_500);
        edit(&mut p);
        let why = p.validate_palw_capacity_verify_v1().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
        assert!(p.validate_palw_v2().is_err(), "{needle}: validate_palw_v2 asks it");
    };
    refused(&|p| p.palw_rcore_plus = Some(ForkActivation::new(1_501)), "palw_capacity_batch_licence is armed without");
    refused(&|p| p.palw_verification_v2 = None, "palw_capacity_batch_licence is armed without");
    refused(&|p| p.palw_verification_v2 = Some(ForkActivation::new(1_501)), "palw_capacity_batch_licence is armed without");

    // The mirror: an armed fence whose bundle copy was never synced, and a bundle copy with no fence.
    refused(&|p| {
        p.palw_capacity_batch_licence = Some(ForkActivation::new(1_600));
    }, "disagrees with the V2 bundle's mirror");
    let mut stale = palw_t12_shipped_params();
    stale.palw_capacity_batch_licence = Some(ForkActivation::new(1_500));
    stale.sync_palw_capacity_verify();
    stale.palw_capacity_batch_licence = None;
    assert!(
        stale.validate_palw_capacity_verify_v1().is_err_and(|e| format!("{e:?}").contains("without palw_capacity_batch_licence or")),
        "a mirror with no fence is refused"
    );
    // Off ConsensusV2 (mainnet's bundle-free const), refused by name.
    let mut v1 = MAINNET_PARAMS;
    v1.palw_capacity_batch_licence = Some(ForkActivation::new(1_500));
    assert!(v1.validate_palw_capacity_verify_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")));
    // Dormant (None or never) is legal anywhere.
    for (name, mut p) in presets() {
        p.palw_capacity_batch_licence = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_verify_v1(), Ok(()), "{name}: never() is dormant");
    }
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis fences, any height is legal");
    }
}
