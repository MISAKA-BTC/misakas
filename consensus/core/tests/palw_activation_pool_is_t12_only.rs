//! **The Activation Pool, R1 and R2 are testnet-12's alone** (ADR-0152-adjacent: Activation Pool,
//! user decision 2026-09-25).
//!
//! `Params::palw_activation_pool` arms three consensus rules — R1 (silence reclamation spares a class
//! that cannot produce and a genesis row), R2 (the registry's span step skips parked rows and audits
//! a `Candidate` at its own staggered span) and the pool itself — so it is a fence. It is `Some` at
//! genesis on testnet-12 with the user's terms and `None` everywhere else, hashed Some-only in every
//! writer (`consensus_params_id` with its terms, `consensus_schedule_id` with its terms, the
//! `for_each_fence` walk), so testnet-11, devnet and mainnet fingerprint byte-identically to the
//! build before the field existed. The pinned numbers are the ones `palw_clock_floor_is_t12_only.rs`
//! pins (v22), unchanged by this fence.

use kaspa_consensus_core::config::params::{
    ForkActivation, PalwActivationPoolParamsV1, Params, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_activation_pool_v1::{PALW_ACTIVATION_POOL_TERMS_V1, PalwActivationPoolTermsV1};

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` before the pool.
const BEFORE_THE_POOL: &[(&str, &str, &str, &str)] = &[
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

fn shipped(name: &str) -> Params {
    match name {
        "testnet-11" => palw_rc_shipped_params(),
        "devnet" => devnet_shipped_params(),
        "mainnet" => mainnet_shipped_params(),
        other => panic!("no such preset {other}"),
    }
}

/// Armed on testnet-12 at genesis with the user's terms; `None` on every other preset, and resolved
/// to nothing at any height there.
#[test]
fn the_activation_pool_is_armed_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    assert_eq!(
        t12.palw_activation_pool,
        Some(PalwActivationPoolParamsV1 { activation: ForkActivation::always(), terms: PALW_ACTIVATION_POOL_TERMS_V1 }),
        "testnet-12 arms the pool from genesis with the user's scale"
    );
    assert_eq!(t12.palw_activation_pool_at(0), Some(PALW_ACTIVATION_POOL_TERMS_V1));
    t12.validate_palw_v2().expect("testnet-12 validates with the pool");
    for (name, _, _, _) in BEFORE_THE_POOL {
        let p = shipped(name);
        assert_eq!(p.palw_activation_pool, None, "{name}: dormant");
        assert_eq!(p.palw_activation_pool_at(u64::MAX - 1), None, "{name}: never in force");
    }
}

/// **testnet-11, devnet and mainnet are byte-identical to the build before the pool.**
#[test]
fn testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_pool() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in BEFORE_THE_POOL {
        let p = shipped(name);
        let now = (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string());
        if (now.0.as_str(), now.1.as_str(), now.2.as_str()) != (*params_id, *identity_id, *schedule_id) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not arm the pool moved: {moved:?}");
}

/// **The pool moves testnet-12's fingerprint — and so do its terms.** A node armed with other terms
/// announces another ruleset and prints another schedule, so two operators who disagree on `A0` see
/// it at the handshake and in the log; the identity is blind only to heights that have not fired.
#[test]
fn the_pool_and_its_terms_move_testnet12s_fingerprint() {
    let t12 = palw_t12_shipped_params();
    let mut without = t12.clone();
    without.palw_activation_pool = None;
    assert_ne!(t12.consensus_params_id(), without.consensus_params_id(), "the ruleset a node announces names the pool");
    assert_ne!(t12.consensus_schedule_id(), without.consensus_schedule_id(), "and so does the schedule");
    assert_ne!(t12.consensus_identity_id(), without.consensus_identity_id(), "a genesis rule is in the identity");
    println!("testnet-12 params id with the pool {} / without {}", t12.consensus_params_id(), without.consensus_params_id());
    let mut other_terms = t12.clone();
    other_terms.palw_activation_pool = Some(PalwActivationPoolParamsV1 {
        activation: ForkActivation::always(),
        terms: PalwActivationPoolTermsV1 { prep_base_sompi: 30 * 100_000_000, ..PALW_ACTIVATION_POOL_TERMS_V1 },
    });
    assert_ne!(t12.consensus_params_id(), other_terms.consensus_params_id(), "the terms are in the params id");
    assert_ne!(t12.consensus_schedule_id(), other_terms.consensus_schedule_id(), "and reported beside the height");
    let mut never = t12.clone();
    never.palw_activation_pool =
        Some(PalwActivationPoolParamsV1 { activation: ForkActivation::never(), ..t12.palw_activation_pool.unwrap() });
    assert_eq!(never.palw_activation_pool_at(0), None, "a never-armed fence arms nothing");
}

/// **`validate_palw_v2` refuses the pool anywhere but at genesis, without its three prerequisites
/// at genesis, or with terms that cannot run.**
#[test]
fn the_pool_is_genesis_only_and_needs_the_registry_the_jury_and_the_audit() {
    let t12 = palw_t12_shipped_params();
    let refused = |edit: &dyn Fn(&mut Params), needle: &str| {
        let mut p = t12.clone();
        edit(&mut p);
        let why = p.validate_palw_v2().expect_err(needle);
        assert!(format!("{why:?}").contains(needle), "expected a refusal naming {needle:?}, got {why:?}");
    };
    refused(
        &|p| {
            p.palw_activation_pool =
                Some(PalwActivationPoolParamsV1 { activation: ForkActivation::new(1), ..p.palw_activation_pool.unwrap() })
        },
        "palw_activation_pool may only be armed at genesis",
    );
    refused(
        &|p| {
            p.palw_activation_pool = Some(PalwActivationPoolParamsV1 {
                activation: ForkActivation::always(),
                terms: PalwActivationPoolTermsV1 { ramp_daa: 0, ..PALW_ACTIVATION_POOL_TERMS_V1 },
            })
        },
        "ramp_daa",
    );
    refused(
        &|p| {
            p.palw_activation_pool = Some(PalwActivationPoolParamsV1 {
                activation: ForkActivation::always(),
                terms: PalwActivationPoolTermsV1 { bonus_share_permille: 1_001, ..PALW_ACTIVATION_POOL_TERMS_V1 },
            })
        },
        "bonus_share_permille",
    );
    // A prerequisite above genesis: the independence fence is what puts a bought class in
    // Candidate. ADR-0145's three economic fences arm together, so all three move to one height —
    // legal for them, and exactly the ruleset the pool refuses (R-core+, which needs them at or below
    // its own height, is asked after the pool).
    let mut late_jury = t12.clone();
    late_jury.palw_canonical_work = Some(ForkActivation::new(5));
    late_jury.palw_admission_independence = Some(ForkActivation::new(5));
    late_jury.palw_fp_derived_work = Some(ForkActivation::new(5));
    // ADR-0152 §4-quater's `palw_class_verify_deadline` (merged from rcore/int-3) needs
    // `palw_fp_derived_work` at or below it and is asked before the pool, so it is taken away here
    // (the fence cleared, its mirror synced) — the refusal under test is the pool's.
    late_jury.palw_class_verify_deadline = None;
    late_jury.sync_palw_class_verify_deadline();
    let why = late_jury.validate_palw_v2().expect_err("the pool with ADR-0147's jury past genesis");
    assert!(format!("{why:?}").contains("palw_activation_pool is armed without"), "{why:?}");
    // And the same fence off validates: the pool is optional, never required.
    let mut off = t12.clone();
    off.palw_activation_pool = None;
    off.validate_palw_v2().expect("testnet-12 without the pool is still a legal ruleset");
}

/// **The fork-id gate never lists a genesis fence, and the probe that proves every fence reaches the
/// gate can arm this one** (the name `palw_fences_v1` gives it is the one `set_fence_for_probe`
/// spells).
#[test]
fn the_pool_carries_its_fork_id_name() {
    let t12 = palw_t12_shipped_params();
    let names: Vec<&str> = t12.palw_fences_v1().into_iter().map(|(name, _)| name).collect();
    assert!(names.contains(&"palw_activation_pool"), "named in the fence list the fork-id gate and the candidate kind read");
    let at = t12.palw_fences_v1().into_iter().find(|(name, _)| *name == "palw_activation_pool").and_then(|(_, fence)| fence);
    assert_eq!(at, Some(ForkActivation::always()));
    assert!(
        !kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1(&t12).contains(&0),
        "a fence at genesis is the identity's business, never a crossed height"
    );
}
