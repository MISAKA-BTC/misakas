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
        "mainnet",
        "badaa8e90f14ef0074048d6b18660864855be8ab854d0ecb01dfbb62171538e1",
        "00d98599bd45867f4a7b3bef1043431ccb8ef0d41cedbc552a87926b5e3b8af5",
        "a1ed7ff07231b84c51d9dc1013a8047ea3efb012bfc9daa36d5dd623709807e4",
    ),
];

/// testnet-12 with the horizon taken away (its mirror synced) — testnet-12 as the parent `99247983`
/// (the readiness line's merge of `rcore/int-3` 3692c7e9 into the Activation Pool line: §4-quater's
/// deadline fence and the pool both armed) fingerprints it, measured by building that tree, not
/// derived. The horizon is the only thing this change moved.
const T12_BEFORE_THE_HORIZON: (&str, &str, &str) = (
    "75437d296100e57d39c9ae6b2645e866e8dd57def8ebc5b9edac053f0055734b",
    "b1350b1565429b638d28019ba9aa6798294452118237a4bc79e6ba9b89f3062a",
    "24d71100697d5a7d34177cddabede8ae5ed62ebc8096b73c3877c79216e1bf4a",
);

/// testnet-12 with the horizon, as this change fingerprints it.
const T12_WITH_THE_HORIZON: (&str, &str, &str) = (
    "55579322a4a8b9d57982bbae6c82ee14286fb42827fae809bcb51593a403e5ca",
    "19d4c76444ecb19032f2ee48ca3ac9180ab8e5d7442a5a11894e1e79a4159e32",
    "7980d47a9a2d2c4cc139207f0af9ea2bcd1efa87d04e662b205cf5cc781d4c96",
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
