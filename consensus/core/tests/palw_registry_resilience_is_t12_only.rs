//! **Lane F1: the registry-resilience fence (the 2026-09-25 pre-launch sweep's V03 and V05) ships
//! dormant, and arming it is a post-launch flag day that moves exactly what a flag day moves.**
//!
//! `Params::palw_registry_resilience` changes three fold rules past its height — a no-capable-panel
//! at the anchor slot re-anchors instead of voiding (V03(1)), a readiness hold keeps its probation
//! progress (V03(2)), a probation resets only on two distinct bonds' failed probes (V05). testnet-12
//! launched from `0e8ec984e` without it, so it is `None` on every preset, testnet-12 included, and
//! hashed Some-only in every writer: every shipped id — testnet-12's release pins among them — is
//! byte-identical to the build before the field existed. The operator arms it later at one common
//! post-launch height; these tests arm it on a copy of testnet-12 and pin what that does:
//!
//! * it moves `consensus_params_id` and `consensus_schedule_id`, never `consensus_identity_id`
//!   (the identity normalises a future height — and `Some(never())` — to absence);
//! * it enters the fork-id schedule, so an armed build keeps a shipped peer until the height and
//!   refuses it from there, and a shipped build (whose gate names no fence) keeps the armed one;
//! * `validate_palw_v2` refuses it unsynced, below R-core+ or the registry, and off ConsensusV2.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as released (`0e8ec984e`: params `b8564b88…`, identity `5de80e64…`, schedule
/// `93da24cc…`) — the ids the launch's nodes announce, which a build carrying this field must
/// announce too while the fence is unset.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Every other preset at the release, as `palw_clock_lead_cap_is_t12_only` pins them there.
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

/// A height for the tests to arm at — one no released testnet-12 fence uses (they sit at genesis, and
/// the bond-maturity schedule's 1,000), as the operator's common post-launch height must be.
const H: u64 = 2_000;

fn shipped(name: &str) -> Params {
    match name {
        "testnet-12" => palw_t12_shipped_params(),
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
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.registry_resilience_from_daa(),
        _ => None,
    }
}

/// testnet-12 with the fence armed at `at`, mirrored as the operator's build assembles it.
fn armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_registry_resilience = Some(at);
    p.sync_palw_registry_resilience();
    p
}

/// **Dormant everywhere as shipped** — testnet-12 included — with an empty mirror, and testnet-12's
/// release ruleset still validates.
#[test]
fn the_fence_ships_dormant_on_every_preset() {
    for name in ["testnet-12", "testnet-11", "devnet", "mainnet", "testnet-10", "simnet"] {
        let p = shipped(name);
        assert_eq!(p.palw_registry_resilience, None, "{name}: the fence ships dormant");
        assert_eq!(mirror(&p), None, "{name}: and so does the bundle's mirror");
    }
    palw_t12_shipped_params().validate_palw_v2().expect("testnet-12 as released validates");
}

/// **testnet-12's release ids and every other preset's are byte-identical to the build before the
/// field** — the field is hashed Some-only in every writer.
#[test]
fn every_shipped_id_is_the_release_ones() {
    let t12 = ids(&palw_t12_shipped_params());
    println!("testnet-12: {t12:?}");
    assert_eq!((t12.0.as_str(), t12.1.as_str(), t12.2.as_str()), T12_RELEASE, "testnet-12 announces the release's ids");
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
/// operator's build and a launch build still pass the identity handshake — and `Some(never())`
/// collapses to absence in the identity too (the fourth place a Some-only fence needs).
#[test]
fn arming_moves_the_params_and_schedule_ids_and_never_the_identity() {
    let t12 = palw_t12_shipped_params();
    let at_h = armed(ForkActivation::new(H));
    at_h.validate_palw_v2().expect("armed at a height, testnet-12 validates");
    assert_eq!(mirror(&at_h), Some(H), "the mirror the fold reads");
    let (p0, i0, s0) = ids(&t12);
    let (p1, i1, s1) = ids(&at_h);
    println!("testnet-12 armed at {H}: {:?}", (&p1, &i1, &s1));
    assert_ne!(p1, p0, "the ruleset a node announces names the fence");
    assert_ne!(s1, s0, "and the schedule the operator log names");
    assert_eq!(i1, i0, "a height not yet reached is absence in the identity");
    // testnet-12's schedule as released: every fence at genesis but the ones named here (printed, so
    // the operator's common post-launch height can be chosen clear of them).
    let scheduled: Vec<(&str, u64)> = t12
        .palw_fences_v1()
        .into_iter()
        .filter_map(|(name, fence)| fence.map(|f| (name, f.daa_score())))
        .filter(|(_, score)| *score != 0 && *score != u64::MAX)
        .collect();
    println!("testnet-12's scheduled fences: {scheduled:?}; schedule {:?}", t12.fence_schedule_v1());
    let released = t12.fence_schedule_v1();
    assert!(!released.contains(&H), "the premise: {H} is a height no released fence uses");
    let mut expected = released.clone();
    expected.push(H);
    expected.sort_unstable();
    assert_eq!(at_h.fence_schedule_v1(), expected, "the fence joins the fork-id schedule at its own height");
    // Two heights are two schedules and two rulesets, one identity.
    let at_h2 = armed(ForkActivation::new(H + 1));
    assert_ne!(at_h2.consensus_params_id(), at_h.consensus_params_id());
    assert_ne!(at_h2.consensus_schedule_id(), at_h.consensus_schedule_id());
    assert_eq!(at_h2.consensus_identity_id(), t12.consensus_identity_id());
    // `Some(never())`: the identity is the shipped one.
    let never = armed(ForkActivation::never());
    assert_eq!(mirror(&never), None, "a never-armed fence mirrors nothing");
    never.validate_palw_v2().expect("a never-armed fence is dormant");
    assert_eq!(never.consensus_identity_id(), t12.consensus_identity_id(), "never() collapses to absence in the identity");
    // At genesis it is a rule in force from block one: a different identity.
    let at_genesis = armed(ForkActivation::always());
    at_genesis.validate_palw_v2().expect("genesis is an admissible height");
    assert_ne!(at_genesis.consensus_identity_id(), t12.consensus_identity_id(), "in force at genesis: two identities");
}

/// **The handshake across the rollout.** An armed build keeps a launch-build peer at every height
/// below the fence — they agree about every block either can produce — and refuses it from the
/// height; the launch build, whose gate names no fence, keeps the armed one (it learns of the split
/// only when blocks differ, which is why the armed side's refusal is the one that matters).
#[test]
fn an_armed_build_keeps_the_launch_build_until_the_height() {
    let launch = palw_t12_shipped_params();
    let upgraded = armed(ForkActivation::new(H));
    assert_eq!(launch.consensus_identity_id(), upgraded.consensus_identity_id(), "the identity gate passes both ways");
    for daa in [0, 1, H / 2, H - 1] {
        let old = fork_id_v1(&launch, daa);
        let new = fork_id_v1(&upgraded, daa);
        assert_eq!(old.fired, new.fired, "DAA {daa}: one history below the fence");
        let upgraded_judges_launch = evaluate_fork_id_v1(&upgraded, daa, old.fired.as_bytes().as_slice(), old.next);
        assert!(!upgraded_judges_launch.refuses(), "DAA {daa}: the armed build keeps the launch build: {upgraded_judges_launch:?}");
        let launch_judges_upgraded = evaluate_fork_id_v1(&launch, daa, new.fired.as_bytes().as_slice(), new.next);
        assert!(!launch_judges_upgraded.refuses(), "DAA {daa}: the launch build keeps the armed build: {launch_judges_upgraded:?}");
    }
    for daa in [H, H + 1, H + 10_000] {
        let old = fork_id_v1(&launch, daa);
        let verdict = evaluate_fork_id_v1(&upgraded, daa, old.fired.as_bytes().as_slice(), old.next);
        assert!(verdict.refuses(), "DAA {daa}: past the fence the armed build refuses the launch build: {verdict:?}");
        // Two armed builds agree at every height.
        let peer = fork_id_v1(&upgraded, daa);
        assert!(!evaluate_fork_id_v1(&upgraded, daa, peer.fired.as_bytes().as_slice(), peer.next).refuses());
    }
}

/// **What the fence refuses**: an unsynced mirror either way, a height below R-core+ or the
/// registry (the rules it changes are theirs), and testnet-11 — which carries neither R-core+ nor
/// SW-8's anchor-block void — so the fence is testnet-12's by construction.
#[test]
fn the_fence_is_refused_unsynced_below_its_prerequisites_and_off_testnet12() {
    let mut unsynced = palw_t12_shipped_params();
    unsynced.palw_registry_resilience = Some(ForkActivation::new(H));
    let why = unsynced.validate_palw_v2().expect_err("an unsynced mirror is refused");
    assert!(why.to_string().contains("palw_registry_resilience disagrees with the V2 bundle's mirror"), "{why}");

    let mut stray = armed(ForkActivation::new(H));
    stray.palw_registry_resilience = None;
    let why = stray.validate_palw_v2().expect_err("a mirror without the fence is refused");
    assert!(why.to_string().contains("without palw_registry_resilience armed"), "{why}");

    let mut no_rcore = armed(ForkActivation::new(H));
    no_rcore.palw_rcore_plus = None;
    no_rcore.sync_palw_rcore_plus();
    let why = no_rcore.validate_registry_resilience_only();
    assert!(why.contains("without palw_rcore_plus and palw_model_registry"), "{why}");

    let mut t11 = palw_rc_shipped_params();
    t11.palw_registry_resilience = Some(ForkActivation::new(1_000_000));
    t11.sync_palw_registry_resilience();
    let why = t11.validate_registry_resilience_only();
    assert!(why.contains("without palw_rcore_plus and palw_model_registry"), "testnet-11 cannot arm it: {why}");
}

/// The fence's own refusal, asked alone (a ruleset missing a prerequisite may be refused first by
/// that prerequisite's own rule inside `validate_palw_v2`).
trait RegistryResilienceOnly {
    fn validate_registry_resilience_only(&self) -> String;
}

impl RegistryResilienceOnly for Params {
    fn validate_registry_resilience_only(&self) -> String {
        self.validate_palw_registry_resilience_v1().expect_err("the fence's own rule refuses it").to_string()
    }
}
