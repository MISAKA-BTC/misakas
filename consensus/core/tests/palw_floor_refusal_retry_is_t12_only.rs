//! **Lane F2: the floor-refusal retry (`Params::palw_floor_refusal_retry`) ships DORMANT, as the first
//! entry of testnet-12's second post-launch flag day (`PALW_T12_POST_LAUNCH_FENCES_V2`), and arming it
//! is a flag day that moves exactly what a flag day moves.**
//!
//! Past its height a claim whose stake draw its anchor block refuses for eligibility at every seed —
//! SW-10's floor, fewer eligible operators than seats after the load filters, no eligible outsider — is
//! re-anchored at its next slot instead of voiding (the user's decision of 2026-09-27; public testnet-12
//! voided 172 such claims between DAA ~624 and 749). The list's height
//! (`PALW_T12_POST_LAUNCH_FENCE_V2_DAA`) is `None` on this build, so the fence is `None` on every preset,
//! testnet-12 included, and hashed Some-only in every writer: every shipped id — testnet-12's DAA-750
//! release pins among them — is byte-identical to the build before the field existed. These tests arm
//! it on a copy of the DAA-750 release (`palw_t12_release_v1_params()`, the ruleset the fleet runs) and
//! pin what that does:
//!
//! * it moves `consensus_params_id` and `consensus_schedule_id`, never `consensus_identity_id`
//!   (the identity normalises a future height — and `Some(never())` — to absence);
//! * it enters the fork-id schedule, so an armed build keeps a released peer until the height and
//!   refuses it from there, and a released build (whose gate names no such fence) keeps the armed one;
//! * `validate_palw_v2` refuses it unsynced, below R-core+, and off ConsensusV2;
//! * the second flag day's list is the ONE place its fences are armed and moved: every entry a fence
//!   `palw_fences_v1` names, disjoint from the DAA-750 list, set through its own `set`, cleared by
//!   `palw_t12_launch_params_v1` (which still hashes to the launch release's ids), moved on a salted
//!   drill by `palw_drill_post_launch_fences_v2_at_v1` and on no other chain.

use kaspa_consensus_core::config::drill::{
    PalwDrillSaltV1, palw_drill_post_launch_fences_at_v1, palw_drill_post_launch_fences_v2_at_v1,
};
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_POST_LAUNCH_FENCE_V2_DAA, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, Params,
    SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_drill_params_v1,
    palw_t12_launch_params_v1, palw_t12_release_v1_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as THIS build ships it — the DAA-750 post-launch release (the same pins
/// `palw_registry_resilience_is_t12_only`'s `T12_RELEASE` holds): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "dbbc9104a2ee754f0f053a6e1614118979fd2c3dc87cbe6bffcf6dcaf4bd59c9",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "7c652212ab5337bda9508deeee2d2e119331856fce0bd27897f19dd66e552397",
);

/// testnet-12 as it LAUNCHED (`0e8ec984e`): both post-launch lists dormant.
const T12_LAUNCH: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Every other preset at the release, as `palw_registry_resilience_is_t12_only` pins them.
const OTHERS_AT_THE_RELEASE: &[(&str, &str, &str, &str)] = &[
    (
        "testnet-11",
        "bd633ce933974d4134676efbdaf46b269dc2fb78f007e0907479aabd4d743f29",
        "44cb8fd729e9575a6e3b1e72c466b8abce4b9ecd81bb556685c9ba225487117f",
        "5a1d8d5679e0e8d7e9022255668fd5d4b3e4c8a6c367acf3882c6a3d480d8b64",
    ),
    (
        "devnet",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "7a27f341e49902ebb5e15ea79a45806fbd37b65daaddf8f0a5a10a15f9bfd4a8",
        "edd80c01c791d225d602b9136f539f4dfeb506ba1b3071b177b0d873a661142f",
    ),
    (
        "mainnet",
        "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
        "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
    (
        "testnet-10",
        "0d9cf361e02dea6d9e873014ff5e414c2e8e6879e8d705cde6c924a3a3f8dd88",
        "2c3067c01e76ac32f0bd3f78ba49cc25f2ea771af4eb848e5aaad17f825a0a64",
        "7ea3296f36fc827898aa6560a1f71159652a12a7dd69105178564b9f7723d5d0",
    ),
    (
        "simnet",
        "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        "63238ba10766c824ff6915484829b01eb4fc3c105665a7db2cf6b175bf870dfd",
        "f981edc9bff1b71ae46abf030c0c56c40beafabeeae78d8435dd502ad6191f69",
    ),
];

/// A height for the tests to arm at — one no released testnet-12 fence uses (they sit at genesis, the
/// DAA-750 release's 750 and the bond-maturity schedule's 1,000), as the next flag day's must be.
const H: u64 = 1_300;

const NAME: &str = "palw_floor_refusal_retry";

fn shipped(name: &str) -> Params {
    match name {
        "testnet-12" => palw_t12_release_v1_params(),
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        "testnet-10" => Params::from(TESTNET_PARAMS.net),
        "simnet" => Params::from(SIMNET_PARAMS.net),
        other => panic!("no such preset {other}"),
    }
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn mirror(p: &Params) -> Option<u64> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.floor_refusal_retry_from_daa(),
        _ => None,
    }
}

/// The DAA-750 release with the fence armed at `at` through the second list's own entry, as the
/// operator's build assembles it.
fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_release_v1_params();
    let entry = PALW_T12_POST_LAUNCH_FENCES_V2.iter().find(|f| f.name == NAME).expect("an entry of the second list");
    (entry.set)(&mut p, Some(at));
    p
}

/// **Dormant everywhere as shipped** — testnet-12 included — with an empty mirror, and the second list's
/// height unset, so the shipped ruleset IS the DAA-750 release.
#[test]
fn the_fence_ships_dormant_on_every_preset() {
    assert_eq!(PALW_T12_POST_LAUNCH_FENCE_V2_DAA, None, "the second flag day is dormant on this build");
    for name in ["testnet-12", "testnet-11", "devnet", "mainnet", "testnet-10", "simnet"] {
        let p = shipped(name);
        assert_eq!(p.palw_floor_refusal_retry, None, "{name}: the fence ships dormant");
        assert_eq!(mirror(&p), None, "{name}: and so does the bundle's mirror");
    }
    let t12 = palw_t12_shipped_params();
    assert_eq!(t12.palw_floor_refusal_retry, None, "testnet-12 as shipped");
    assert_eq!(ids(&t12), ids(&palw_t12_release_v1_params()), "the shipped ruleset is the DAA-750 release");
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
}

/// **testnet-12's release ids and every other preset's are byte-identical to the build before the
/// field** — hashed Some-only in every writer; and testnet-12 as launched still hashes to the launch.
#[test]
fn every_shipped_id_is_the_release_ones() {
    let t12 = ids(&palw_t12_shipped_params());
    println!("testnet-12: {t12:?}");
    assert_eq!((t12.0.as_str(), t12.1.as_str(), t12.2.as_str()), T12_RELEASE, "testnet-12 announces the DAA-750 release's ids");
    let launch = ids(&palw_t12_launch_params_v1());
    assert_eq!((launch.0.as_str(), launch.1.as_str(), launch.2.as_str()), T12_LAUNCH, "testnet-12 as launched is the launch");
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in OTHERS_AT_THE_RELEASE {
        let now = ids(&shipped(name));
        println!("{name}: {now:?}");
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the fence moved: {moved:?}");
}

/// **Arming it at a height moves the ruleset and the schedule, never the identity** — so the
/// operator's build and a released build still pass the identity handshake — and `Some(never())`
/// collapses to absence in the identity too.
#[test]
fn arming_moves_the_params_and_schedule_ids_and_never_the_identity() {
    let release = palw_t12_release_v1_params();
    let at_h = armed(ForkActivation::new(H));
    at_h.validate_palw_v2().expect("armed at a height, testnet-12 validates");
    assert_eq!(mirror(&at_h), Some(H), "the mirror the fold reads");
    let (p0, i0, s0) = ids(&release);
    let (p1, i1, s1) = ids(&at_h);
    println!("testnet-12 armed at {H}: {:?}", (&p1, &i1, &s1));
    assert_ne!(p1, p0, "the ruleset a node announces names the fence");
    assert_ne!(s1, s0, "and the schedule the operator log names");
    assert_eq!(i1, i0, "a height not yet reached is absence in the identity");
    let released = release.fence_schedule_v1();
    assert!(!released.contains(&H), "the premise: {H} is a height no released fence uses ({released:?})");
    let mut expected = released.clone();
    expected.push(H);
    expected.sort_unstable();
    assert_eq!(at_h.fence_schedule_v1(), expected, "the fence joins the fork-id schedule at its own height");
    let at_h2 = armed(ForkActivation::new(H + 1));
    assert_ne!(at_h2.consensus_params_id(), at_h.consensus_params_id());
    assert_ne!(at_h2.consensus_schedule_id(), at_h.consensus_schedule_id());
    assert_eq!(at_h2.consensus_identity_id(), release.consensus_identity_id());
    let never = armed(ForkActivation::never());
    assert_eq!(mirror(&never), None, "a never-armed fence mirrors nothing");
    never.validate_palw_v2().expect("a never-armed fence is dormant");
    assert_eq!(never.consensus_identity_id(), release.consensus_identity_id(), "never() collapses to absence in the identity");
    let at_genesis = armed(ForkActivation::always());
    at_genesis.validate_palw_v2().expect("genesis is an admissible height");
    assert_ne!(at_genesis.consensus_identity_id(), release.consensus_identity_id(), "in force at genesis: two identities");
}

/// **The handshake across the rollout.** An armed build keeps a released peer at every height below
/// the fence and refuses it from the height; the released build, whose gate names no such fence, keeps
/// the armed one.
#[test]
fn an_armed_build_keeps_the_released_build_until_the_height() {
    let release = palw_t12_release_v1_params();
    let upgraded = armed(ForkActivation::new(H));
    assert_eq!(release.consensus_identity_id(), upgraded.consensus_identity_id(), "the identity gate passes both ways");
    for daa in [0, 1, 750, 1_000, H - 1] {
        let old = fork_id_v1(&release, daa);
        let new = fork_id_v1(&upgraded, daa);
        assert_eq!(old.fired, new.fired, "DAA {daa}: one history below the fence");
        let upgraded_judges_release = evaluate_fork_id_v1(&upgraded, daa, old.fired.as_bytes().as_slice(), old.next);
        assert!(
            !upgraded_judges_release.refuses(),
            "DAA {daa}: the armed build keeps the released build: {upgraded_judges_release:?}"
        );
        let release_judges_upgraded = evaluate_fork_id_v1(&release, daa, new.fired.as_bytes().as_slice(), new.next);
        assert!(
            !release_judges_upgraded.refuses(),
            "DAA {daa}: the released build keeps the armed build: {release_judges_upgraded:?}"
        );
    }
    for daa in [H, H + 1, H + 10_000] {
        let old = fork_id_v1(&release, daa);
        let verdict = evaluate_fork_id_v1(&upgraded, daa, old.fired.as_bytes().as_slice(), old.next);
        assert!(verdict.refuses(), "DAA {daa}: past the fence the armed build refuses the released build: {verdict:?}");
        let peer = fork_id_v1(&upgraded, daa);
        assert!(!evaluate_fork_id_v1(&upgraded, daa, peer.fired.as_bytes().as_slice(), peer.next).refuses());
    }
}

/// **What the fence refuses**: an unsynced mirror either way, a height below R-core+, and testnet-11 —
/// which carries no R-core+ and so no stake draw to retry — so the fence is testnet-12's by construction.
#[test]
fn the_fence_is_refused_unsynced_below_rcore_and_off_testnet12() {
    let mut unsynced = palw_t12_release_v1_params();
    unsynced.palw_floor_refusal_retry = Some(ForkActivation::new(H));
    let why = unsynced.validate_palw_v2().expect_err("an unsynced mirror is refused");
    assert!(why.to_string().contains("palw_floor_refusal_retry disagrees with the V2 bundle's mirror"), "{why}");

    let mut stray = armed(ForkActivation::new(H));
    stray.palw_floor_refusal_retry = None;
    let why = stray.validate_palw_v2().expect_err("a mirror without the fence is refused");
    assert!(why.to_string().contains("without palw_floor_refusal_retry armed"), "{why}");

    let mut no_rcore = armed(ForkActivation::new(H));
    no_rcore.palw_rcore_plus = None;
    let why = no_rcore.validate_palw_floor_refusal_retry_v1().expect_err("the fence's own rule refuses it").to_string();
    assert!(why.contains("without palw_rcore_plus armed at or below it"), "{why}");
    let mut above = armed(ForkActivation::new(H));
    above.palw_rcore_plus = Some(ForkActivation::new(H + 1));
    let why = above.validate_palw_floor_refusal_retry_v1().expect_err("R-core+ above the fence").to_string();
    assert!(why.contains("without palw_rcore_plus armed at or below it"), "{why}");

    let mut t11 = palw_rc_shipped_params();
    t11.palw_floor_refusal_retry = Some(ForkActivation::new(1_000_000));
    t11.sync_palw_floor_refusal_retry();
    let why = t11.validate_palw_floor_refusal_retry_v1().expect_err("testnet-11 cannot arm it").to_string();
    assert!(why.contains("without palw_rcore_plus"), "testnet-11 cannot arm it: {why}");
    let mut t10 = Params::from(TESTNET_PARAMS.net);
    t10.palw_floor_refusal_retry = Some(ForkActivation::new(1_000_000));
    let why = t10.validate_palw_floor_refusal_retry_v1().expect_err("off ConsensusV2").to_string();
    assert!(why.contains("not ConsensusV2"), "{why}");
}

/// **The second flag day's list** (`PALW_T12_POST_LAUNCH_FENCES_V2`): each entry a fence
/// `palw_fences_v1` names, listed once, in neither the DAA-750 list nor armed by it; set through its own
/// `set` and cleared by it (the release's ids back, byte for byte); cleared with the DAA-750 list by
/// `palw_t12_launch_params_v1`. All entries at one height validate together over the release; and a
/// salted drill arms them at a height of their own, after the first flag day's move, while public
/// testnet-12 refuses the move.
#[test]
fn the_second_flag_day_list_is_the_one_place_its_fences_are_set() {
    let release = palw_t12_release_v1_params();
    let names: Vec<&str> = release.palw_fences_v1().into_iter().map(|(name, _)| name).collect();
    assert!(!PALW_T12_POST_LAUNCH_FENCES_V2.is_empty());
    for (i, fence) in PALW_T12_POST_LAUNCH_FENCES_V2.iter().enumerate() {
        assert!(names.contains(&fence.name), "{} is a fence palw_fences_v1 names", fence.name);
        assert!(PALW_T12_POST_LAUNCH_FENCES_V2[..i].iter().all(|other| other.name != fence.name), "{} listed once", fence.name);
        assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|v1| v1.name != fence.name), "{} is not the DAA-750 list's", fence.name);
        let height = |p: &Params| p.palw_fences_v1().into_iter().find(|(n, _)| *n == fence.name).unwrap().1;
        assert_eq!(height(&release), None, "{}: dormant on the release", fence.name);
        let mut set = release.clone();
        (fence.set)(&mut set, Some(ForkActivation::new(H)));
        assert_eq!(height(&set), Some(ForkActivation::new(H)), "{}: its entry sets it", fence.name);
        set.validate_palw_v2().unwrap_or_else(|e| panic!("{} at {H} over the release: {e}", fence.name));
        (fence.set)(&mut set, None);
        assert_eq!(ids(&set), ids(&release), "{}: cleared through its entry, the release's ids", fence.name);
    }
    let mut all = release.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V2 {
        (fence.set)(&mut all, Some(ForkActivation::new(H)));
    }
    all.validate_palw_v2().expect("every entry at one height validates over the release");
    let launch = palw_t12_launch_params_v1();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1.iter().chain(PALW_T12_POST_LAUNCH_FENCES_V2) {
        let at = launch.palw_fences_v1().into_iter().find(|(n, _)| *n == fence.name).unwrap().1;
        assert_eq!(at, None, "as launched, {} is dormant", fence.name);
    }

    // The drill: a salted testnet-12 chain crosses the first flag day at 40 and this one at 60.
    let salt = PalwDrillSaltV1::from_bytes([0x5F; 32]).unwrap();
    let mut drill = palw_t12_drill_params_v1(&salt);
    palw_drill_post_launch_fences_at_v1(&mut drill, 40).expect("the first flag day moves to 40");
    let moves = palw_drill_post_launch_fences_v2_at_v1(&mut drill, 60).expect("the second arms at 60");
    assert_eq!(moves.iter().map(|m| (m.name, m.was, m.at)).collect::<Vec<_>>(), vec![(NAME, None, 60)], "ARMED, from dormant");
    assert_eq!(drill.palw_floor_refusal_retry, Some(ForkActivation::new(60)));
    assert_eq!(mirror(&drill), Some(60), "the mirror follows");
    let why = palw_drill_post_launch_fences_v2_at_v1(&mut drill.clone(), 40).unwrap_err();
    assert!(why.contains("--palw-drill-fence2-at=40") && why.contains("fork id"), "the first flag day's height: {why}");
    let mut public = palw_t12_shipped_params();
    let why = palw_drill_post_launch_fences_v2_at_v1(&mut public, 60).unwrap_err();
    assert!(why.contains("PUBLIC testnet-12"), "{why}");
    let mut t11 = palw_rc_shipped_params();
    assert!(palw_drill_post_launch_fences_v2_at_v1(&mut t11, 60).is_err(), "a drill is testnet-12's");
}
