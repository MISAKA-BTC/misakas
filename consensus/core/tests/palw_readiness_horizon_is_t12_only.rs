//! **The readiness-V2 horizon is testnet-12's alone** (user decision 2026-09-25, readiness capacity
//! option (a)).
//!
//! `Params::palw_readiness_v2_max_age_spans` is how many execution spans a `SeatReadinessProvedV2`
//! row stands — the one input of `palw_readiness_max_age_daa_v1`, which every judge of a row asks:
//! the registry's ready-seat count, ADR-0147's jury, the panel draw past the audit fence, the
//! registry read the RPC serves, the panel view, the node's half-age re-prove duty and the M1
//! escalation's `max − 2`. It is `Some(always, 24)` on testnet-12 and `None` everywhere else, where
//! the default eight spans stands; hashed Some-only in every writer (`consensus_params_id` and
//! `consensus_schedule_id` with the spans, the `for_each_fence` walk), so testnet-11, devnet and
//! mainnet fingerprint byte-identically to the build before the field existed — the numbers pinned
//! beside every other testnet-12-only fence, unchanged.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, PalwReadinessV2MaxAgeParamsV1, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1, PALW_READINESS_V2_MAX_AGE_SPANS_V1, PalwModelRegistryFoldV1, PalwReadinessPolicyV1,
    PalwSeatReadinessRowV1, palw_readiness_duty_due_v2, palw_readiness_max_age_daa_v1, palw_readiness_row_is_fresh_v1,
    palw_registry_globals_of_bundle_v1,
};
use kaspa_hashes::Hash64;

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` before the horizon
/// — the values every other testnet-12-only fence pins, unmoved.
const BEFORE_THE_HORIZON: &[(&str, &str, &str, &str)] = &[
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
        // re-pin 2026-09-25 @57c1fe323c44: t12 shipping re-pin 2026-09-25 (rcore/int-3 57c1fe32): mainnet values (λ 5, 1,620 s tolerance, level 225; mainnet takes t12's DNS set, carve, Decision A), the beat lead cap (132 s past the receiver clock), the execution-quantum maturity 120 DAA, the readiness-memory / duties / mempool node fixes; T41 v22 golden moves because R1 (palw_activation_pool) excludes genesis rows from silence reclamation. Genesis a27f8f44 and premine txid 5e0d5f1b unchanged. (was badaa8e9…, 00d98599…)
        "mainnet",
        "eb866c61ca1a8ab58108be6cd1f39f951b582123472545575a5c7dbe0f1e5aa5",
        "7819e5ed2b3df50b3303df3df2f0fec7677ddcb37ed55f1d43455a37ecd9c9a8",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
];

/// testnet-12 with the horizon taken away (its mirror synced), on the integrated line (`95f6ce6d`:
/// the Activation Pool f92f34a7 with P1's hashed `b_cap`, rcore/int-3 f7350af9 with aheld-node
/// 2f92228f). The horizon is the only thing this pin and the next differ by. Taken from this test's
/// own output on that tree; the integration owner's final re-pin moves both together. When the
/// horizon landed (`3e9ae4ba`, over `99247983`) the pre-horizon value was MEASURED by building the
/// parent tree — `75437d29…` / `b1350b15…` / `24d71100…` — and matched this twin exactly.
///
/// Re-pinned 2026-09-25 (`rcore/exec-maturity-120`): testnet-12's 120-DAA execution-quantum maturity
/// (`palw_exec_maturity_is_t12_only`) is in this twin — and so are c6ffd812's mainnet values, which the
/// merge `2004c588` left un-re-pinned here. Previous: `fd7353c0…` / `a7fe561c…` / `28d866f1…`.
// re-pin 2026-09-25 @57c1fe323c44: t12 shipping re-pin 2026-09-25 (rcore/int-3 57c1fe32): mainnet values (λ 5, 1,620 s tolerance, level 225; mainnet takes t12's DNS set, carve, Decision A), the beat lead cap (132 s past the receiver clock), the execution-quantum maturity 120 DAA, the readiness-memory / duties / mempool node fixes; T41 v22 golden moves because R1 (palw_activation_pool) excludes genesis rows from silence reclamation. Genesis a27f8f44 and premine txid 5e0d5f1b unchanged. (was 3d8670a6…, af69b742…, 6cb1aaaa…)
const T12_BEFORE_THE_HORIZON: (&str, &str, &str) = (
    "71777608d6ca83717313202c59989604d615049b53365a69f79f3cb0a48ce85c",
    "9f267fb1794c366a70a9b9008773bc470f8d0ab03ebb0ca26e9a28eaacb70ab4",
    "c9964f908e0a16e5c0664c9b962506e96966f7bd53db4fc2bde6892055d65989",
);

/// testnet-12 with the horizon, on the same tree (at `3e9ae4ba`, before the pool and int-3 merges:
/// `55579322…` / `19d4c764…` / `7980d47a…`).
///
/// Re-pinned 2026-09-25 (`rcore/exec-maturity-120`): testnet-12's full ids with the 120-DAA maturity
/// (= `palw_exec_maturity_is_t12_only`'s `T12_WITH_THE_MATURITY`; at `2004c588` they were `2790d7ce…` /
/// `1fd06c99…` / `e0af0218…`, which this pin never caught up with). Previous: `4d38b3f1…` / `2477a803…` /
/// `06dd5566…`.
// re-pin 2026-09-25 @57c1fe323c44: t12 shipping re-pin 2026-09-25 (rcore/int-3 57c1fe32): mainnet values (λ 5, 1,620 s tolerance, level 225; mainnet takes t12's DNS set, carve, Decision A), the beat lead cap (132 s past the receiver clock), the execution-quantum maturity 120 DAA, the readiness-memory / duties / mempool node fixes; T41 v22 golden moves because R1 (palw_activation_pool) excludes genesis rows from silence reclamation. Genesis a27f8f44 and premine txid 5e0d5f1b unchanged. (was 730d7f10…, 50990e0f…, 01854a38…)
const T12_WITH_THE_HORIZON: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        other => panic!("no such preset {other}"),
    }
}

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn mirror(p: &Params) -> Option<u32> {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.readiness_v2_max_age_spans(),
        _ => None,
    }
}

/// testnet-12 without the horizon: the fence taken away and the bundle's mirror synced, as a build
/// without it assembles the bundle.
fn without_the_horizon(t12: &Params) -> Params {
    let mut twin = t12.clone();
    twin.palw_readiness_v2_max_age_spans = None;
    twin.sync_palw_readiness_v2_max_age_spans();
    twin
}

/// Armed on testnet-12 at genesis with 24 spans, mirrored into its bundle; `None` on every other
/// preset, where the default eight stands and the mirror is empty.
#[test]
fn the_horizon_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(
        t12.palw_readiness_v2_max_age_spans,
        Some(PalwReadinessV2MaxAgeParamsV1 { activation: ForkActivation::always(), max_age_spans: 24 }),
        "testnet-12 arms the 24-span horizon from genesis"
    );
    assert_eq!(PALW_READINESS_V2_MAX_AGE_SPANS_T12_V1, 24);
    assert_eq!(t12.palw_readiness_v2_max_age_spans_v1(), 24);
    assert_eq!(mirror(&t12), Some(24), "the bundle mirrors it");
    t12.validate_palw_v2().expect("testnet-12 validates with the horizon");
    for (name, _, _, _) in BEFORE_THE_HORIZON {
        let p = shipped(name);
        assert_eq!(p.palw_readiness_v2_max_age_spans, None, "{name}: dormant");
        assert_eq!(p.palw_readiness_v2_max_age_spans_v1(), PALW_READINESS_V2_MAX_AGE_SPANS_V1, "{name}: eight spans, pinned");
        assert_eq!(PALW_READINESS_V2_MAX_AGE_SPANS_V1, 8);
        assert_eq!(mirror(&p), None, "{name}: no mirror");
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            let g = palw_registry_globals_of_bundle_v1(bundle);
            assert_eq!(g.readiness_v2_max_age_spans, 8, "{name}: the fold's globals carry eight");
        }
    }
}

/// **testnet-11, devnet and mainnet are byte-identical to the build before the horizon.**
#[test]
fn testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_horizon() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in BEFORE_THE_HORIZON {
        let now = ids(&shipped(name));
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the horizon moved: {moved:?}");
}

/// **The horizon moves testnet-12's fingerprint — and so does its value — and nothing else did.**
/// Taken away, testnet-12 is exactly the ruleset before it; a node armed with another horizon
/// announces another ruleset and prints another schedule; a never-armed fence is absence.
#[test]
fn the_horizon_moves_testnet12s_fingerprint_and_nothing_else_did() {
    let t12 = palw_t12_shipped_params();
    let twin = without_the_horizon(&t12);
    twin.validate_palw_v2().expect("the horizon is optional: testnet-12 without it is a legal ruleset");
    println!("testnet-12 with the horizon {:?} / without {:?}", ids(&t12), ids(&twin));
    let pinned = |(a, b, c): (&str, &str, &str)| (a.to_string(), b.to_string(), c.to_string());
    assert_eq!(ids(&twin), pinned(T12_BEFORE_THE_HORIZON), "taken away, testnet-12 is the ruleset before it");
    assert_eq!(ids(&t12), pinned(T12_WITH_THE_HORIZON), "testnet-12's ids with the horizon");
    assert_ne!(t12.consensus_params_id(), twin.consensus_params_id(), "the ruleset a node announces names the horizon");
    assert_ne!(t12.consensus_identity_id(), twin.consensus_identity_id(), "in force from block one: two identities");
    assert_ne!(t12.consensus_schedule_id(), twin.consensus_schedule_id(), "and the schedule names it");
    let mut other = t12.clone();
    other.palw_readiness_v2_max_age_spans =
        Some(PalwReadinessV2MaxAgeParamsV1 { activation: ForkActivation::always(), max_age_spans: 16 });
    other.sync_palw_readiness_v2_max_age_spans();
    other.validate_palw_v2().expect("sixteen spans is inside the bounds");
    assert_ne!(t12.consensus_params_id(), other.consensus_params_id(), "the spans are in the params id");
    assert_ne!(t12.consensus_schedule_id(), other.consensus_schedule_id(), "and reported beside the height");
    let mut never = twin.clone();
    never.palw_readiness_v2_max_age_spans = Some(PalwReadinessV2MaxAgeParamsV1 { activation: ForkActivation::never(), max_age_spans: 24 });
    assert_eq!(never.palw_readiness_v2_max_age_spans_v1(), 8, "a never-armed fence arms nothing");
    assert_eq!(never.consensus_identity_id(), twin.consensus_identity_id(), "Some(never()) is absence");
    never.validate_palw_v2().expect("a never-armed fence is no fence");
}

/// **`validate_palw_v2` refuses the horizon above genesis, without the registry and readiness V2
/// at genesis, outside `[8, 30]` spans, off its bundle's mirror — and a mirror without the fence.**
#[test]
fn the_horizon_is_genesis_only_bounded_and_mirrored() {
    let t12 = palw_t12_shipped_params();
    let refused = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = t12.clone();
        edit(&mut p);
        let why = p.validate_palw_v2().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
    };
    let horizon = |activation: ForkActivation, max_age_spans: u32| Some(PalwReadinessV2MaxAgeParamsV1 { activation, max_age_spans });
    refused(
        &|p| {
            p.palw_readiness_v2_max_age_spans = horizon(ForkActivation::new(1), 24);
            p.sync_palw_readiness_v2_max_age_spans();
        },
        "palw_readiness_v2_max_age_spans may only be armed at genesis",
    );
    for (spans, legal) in [(7, false), (8, true), (24, true), (30, true), (31, false), (0, false)] {
        let mut p = t12.clone();
        p.palw_readiness_v2_max_age_spans = horizon(ForkActivation::always(), spans);
        p.sync_palw_readiness_v2_max_age_spans();
        match p.validate_palw_v2() {
            Ok(()) => assert!(legal, "{spans} spans validated"),
            Err(why) => {
                assert!(!legal, "{spans} spans refused: {why:?}");
                assert!(format!("{why:?}").contains("outside [8, 30] spans"), "{why:?}");
            }
        }
    }
    refused(
        &|p| p.palw_readiness_v2_max_age_spans = horizon(ForkActivation::always(), 16),
        "disagrees with the V2 bundle's mirror",
    );
    refused(&|p| p.palw_readiness_v2_max_age_spans = None, "the V2 bundle carries a readiness-V2 horizon without");
    // Its prerequisites: readiness V2 at a height instead of genesis. The pool (asked first) needs it
    // at genesis too, so it is taken away here — the refusal under test is the horizon's.
    refused(
        &|p| {
            p.palw_activation_pool = None;
            p.palw_readiness_v2 = Some(ForkActivation::new(5));
        },
        "palw_readiness_v2_max_age_spans is armed without palw_model_registry and palw_readiness_v2",
    );
    // testnet-11 cannot take it: its registry and readiness V2 are armed at heights.
    let mut t11 = palw_rc_shipped_params();
    t11.palw_readiness_v2_max_age_spans = horizon(ForkActivation::always(), 24);
    t11.sync_palw_readiness_v2_max_age_spans();
    assert!(t11.validate_palw_v2().is_err_and(|e| format!("{e:?}").contains("palw_readiness_v2_max_age_spans")));
}

/// **The fork-id gate never lists a genesis fence, and the probe that proves every fence reaches the
/// gate can arm this one** (the name `palw_fences_v1` gives it is the one `set_fence_for_probe` and
/// the ruleset-candidate kind spell).
#[test]
fn the_horizon_carries_its_fork_id_name() {
    let t12 = palw_t12_shipped_params();
    let at =
        t12.palw_fences_v1().into_iter().find(|(name, _)| *name == "palw_readiness_v2_max_age_spans").and_then(|(_, fence)| fence);
    assert_eq!(at, Some(ForkActivation::always()), "named in the fence list the fork-id gate and the candidate kind read");
    assert!(
        !kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1(&t12).contains(&0),
        "a fence at genesis is the identity's business, never a crossed height"
    );
}

/// **On testnet-12's own fold a V2 row stands 24 DAA and not 25, and every judge agrees** — the
/// globals testnet-12's processor hands its fold (`palw_registry_globals_of_bundle_v1`), on its
/// one-DAA spans: the one freshness rule, the fold's own question, the draw's policy past the audit
/// fence, and the node's duty (due past 12). testnet-11's globals keep eight, and its duty four.
#[test]
fn on_testnet12_a_row_at_age_24_is_fresh_and_at_25_it_is_not() {
    let t12 = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let span_daa = t12.palw_execution_lane.as_ref().expect("testnet-12 schedules the lane").schedule_span_daa;
    assert_eq!(span_daa, 1, "one-DAA spans: the horizon in DAA is the horizon in spans");
    let g = palw_registry_globals_of_bundle_v1(bundle);
    assert!(t12.palw_readiness_v2_at(0) && t12.palw_audit_2026_09_23_active_at(0), "the draw judges by the V2 rule from genesis");
    let fold = PalwModelRegistryFoldV1 {
        globals: g,
        span_daa,
        genesis_works: Default::default(),
        grace_until_daa: 0,
        admission_audit_period_daa: t12.palw_admission_audit_period_daa,
        readiness_v2_active: true,
    };
    let now = 10_000u64;
    let row = |age: u64| PalwSeatReadinessRowV1 { proved_daa: now - age, proved_span: now - age, leaf_index: 0, proof_version: 2, chunks: 16 };
    let policy = PalwReadinessPolicyV1::at(&fold, now, Hash64::from_u64_word(1), true);
    assert_eq!((palw_readiness_max_age_daa_v1(span_daa, &g, true), policy.max_age_daa), (24, 24));
    for (age, fresh) in [(0, true), (12, true), (24, true), (25, false), (30, false)] {
        assert_eq!(palw_readiness_row_is_fresh_v1(&row(age), now, span_daa, &g, true), fresh, "age {age}");
        assert_eq!(fold.readiness_row_is_fresh(&row(age), now), fresh, "the fold, age {age}");
        assert_eq!(policy.admits(&row(age)), fresh, "the draw, age {age}");
    }
    let due = |g: &kaspa_consensus_core::palw_model_registry_v1::PalwRegistryGlobalsV1, age: u64| {
        palw_readiness_duty_due_v2(Some(&row(age)), now, now, None, span_daa, g, true)
    };
    assert!(!due(&g, 12) && due(&g, 13), "testnet-12's duty: re-prove past 12");
    let t11 = palw_rc_shipped_params();
    let PalwConsensusMode::ConsensusV2(t11_bundle) = &t11.palw_consensus_mode else { panic!("testnet-11 is ConsensusV2") };
    let g11 = palw_registry_globals_of_bundle_v1(t11_bundle);
    assert_eq!(palw_readiness_max_age_daa_v1(1, &g11, true), 8, "testnet-11: eight spans");
    assert!(!due(&g11, 4) && due(&g11, 5), "testnet-11's duty: re-prove past 4, as before");
}
