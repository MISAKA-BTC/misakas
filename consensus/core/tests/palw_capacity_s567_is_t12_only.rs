//! **ADR-0164 — F-EM (`palw_capacity_emission_budget`), F-M1 (`palw_capacity_multi_claim`) and F-K (`palw_capacity_rho_breaker`) are
//! dormant on every shipped preset until a testnet-12 flag day names their height, and arming them is a scheduled fence like any
//! other** (lane CAP, rcore/cap-1000, 2026-10-03).
//!
//! On the DAA-1,300 release every one is `None`, testnet-12 included, and its three ids are that release's to the byte (the pin
//! below: the int-11 flag day arms them over the int-10 release, so the pin is the DAA-1,300 release's, not the shipped ruleset's);
//! armed at a height each moves `consensus_params_id` and `consensus_schedule_id` but NOT `consensus_identity_id`; `Some(never())`
//! is absence; `validate_palw_v2` refuses each off ConsensusV2, without its prerequisites at or below it, and with a bundle mirror
//! that disagrees. They are in no earlier flag-day list and not in the capacity lists (the int-11 flag day names their height).

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, PALW_T12_CAPACITY_EMISSION_BUDGET_V1, PALW_T12_CAPACITY_FENCES_V1, PALW_T12_CAPACITY_MULTI_CLAIM_V1,
    PALW_T12_CAPACITY_RHO_BREAKER_V1, PALW_T12_CAPACITY_RHO10_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, Params, PalwPostLaunchFenceV1,
    SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v2_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// testnet-12 as THIS build ships its DAA-1,300 release — params, identity, schedule (a dormant fence must not move it).
// re-pin 2026-09-27 @b2bf20a78b0d: third post-launch flag day: capacity tests judge their fence against the DAA-1,300 release (release_v2) (was cbe9152f…, f78b02ad…)
const T12_RELEASE: (&str, &str, &str) = (
    "24e1aec3e9a102fa40d559cd28005ad5944c32caa485d685bed65c52e4c056ff",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "d263d7f2971f4e20b57b26d7b7428bd8f9346c3728bbb6927341d8b36b0c1c3a",
);

const NAMES: [&str; 3] = ["palw_capacity_emission_budget", "palw_capacity_multi_claim", "palw_capacity_rho_breaker"];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12", palw_t12_release_v2_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

fn entries() -> [&'static PalwPostLaunchFenceV1; 3] {
    [&PALW_T12_CAPACITY_EMISSION_BUDGET_V1, &PALW_T12_CAPACITY_MULTI_CLAIM_V1, &PALW_T12_CAPACITY_RHO_BREAKER_V1]
}

fn mirror(p: &Params) -> (Option<u64>, Option<u64>, Option<u64>) {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => {
            (b.state.capacity_emission_from_daa(), b.state.capacity_riders_from_daa(), b.state.capacity_breaker_from_daa())
        }
        _ => (None, None, None),
    }
}

/// The release with the whole ρ = 10 capacity list armed at `h` (what the three need below them), the three still dormant.
fn base_at(h: u64) -> Params {
    let mut p = palw_t12_release_v2_params();
    for f in PALW_T12_CAPACITY_RHO10_FENCES_V1 {
        (f.set)(&mut p, Some(ForkActivation::new(h)));
    }
    p.validate_palw_v2().expect("the capacity list over the release validates");
    p
}

fn all_at(h: u64) -> Params {
    let mut p = base_at(h);
    for f in entries() {
        (f.set)(&mut p, Some(ForkActivation::new(h)));
    }
    p.validate_palw_v2().unwrap_or_else(|e| panic!("the three at {h}: {e:?}"));
    p
}

#[test]
fn the_fences_are_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(
            (p.palw_capacity_emission_budget, p.palw_capacity_multi_claim, p.palw_capacity_rho_breaker),
            (None, None, None),
            "{name}: the three ship dormant"
        );
        assert_eq!(mirror(&p), (None, None, None), "{name}: the fold's mirror is empty");
        for fence in NAMES {
            assert!(p.palw_fences_v1().iter().any(|(n, f)| *n == fence && f.is_none()), "{name}: {fence} is on the fork-id list");
        }
        assert_eq!(p.validate_palw_capacity_s567_v1(), Ok(()), "{name}: nothing to refuse");
    }
    let t12 = palw_t12_release_v2_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

#[test]
fn arming_each_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let (_, identity, _) = ids(&palw_t12_release_v2_params());
    for h0 in [1_001u64, 1_500, 5_000] {
        // The capacity list stands at `h0`; the fences under test arm 10 DAA above it, a height no other fence uses (the fork id names
        // heights, not fences).
        let base = base_at(h0);
        let h = h0 + 10;
        let (bp, _, bs) = ids(&base);
        let mut seen = vec![ids(&base)];
        for entry in entries() {
            let mut armed = base.clone();
            (entry.set)(&mut armed, Some(ForkActivation::new(h)));
            // F-M1 needs F-EM below it: arm that first for it alone.
            if entry.name == "palw_capacity_multi_claim" {
                (PALW_T12_CAPACITY_EMISSION_BUDGET_V1.set)(&mut armed, Some(ForkActivation::new(h)));
            }
            armed.validate_palw_v2().unwrap_or_else(|e| panic!("{} at {h}: {e:?}", entry.name));
            let (p, i, s) = ids(&armed);
            assert_ne!(p, bp, "{} at {h}: the ruleset names it", entry.name);
            assert_ne!(s, bs, "{} at {h}: the schedule names it", entry.name);
            assert_eq!(i, identity, "{} at {h}: the identity does not move", entry.name);
            assert!(!seen.contains(&ids(&armed)), "{}: its own ruleset", entry.name);
            seen.push(ids(&armed));
            let f = armed.palw_fences_v1().into_iter().find(|(n, _)| *n == entry.name).and_then(|(_, f)| f);
            assert_eq!(f, Some(ForkActivation::new(h)));
            let s = fork_id_v1(&base, h);
            assert!(evaluate_fork_id_v1(&armed, h, s.fired.as_bytes().as_slice(), s.next).refuses(), "{} is gated from {h}", entry.name);
            let mut never = base.clone();
            (entry.set)(&mut never, Some(ForkActivation::never()));
            never.validate_palw_v2().expect("a never-armed fence is no fence");
            assert_eq!(mirror(&never), (None, None, None));
            assert_eq!(never.consensus_identity_id().to_string(), identity, "Some(never()) is absence in the identity");
            let mut back = armed.clone();
            (entry.set)(&mut back, None);
            if entry.name == "palw_capacity_multi_claim" {
                (PALW_T12_CAPACITY_EMISSION_BUDGET_V1.set)(&mut back, None);
            }
            assert_eq!(ids(&back), ids(&base), "{}: set(None) gives the base back, to the id", entry.name);
        }
        let all = all_at(h0);
        let h = h0;
        assert_eq!(mirror(&all), (Some(h), Some(h), Some(h)));
        for (daa, want) in [(h - 1, false), (h, true)] {
            let PalwConsensusMode::ConsensusV2(b) = &all.palw_consensus_mode else { panic!("ConsensusV2") };
            assert_eq!((b.state.capacity_emission_active_at(daa), b.state.capacity_riders_active_at(daa), b.state.capacity_breaker_active_at(daa)), (want, want, want));
        }
    }
}

#[test]
fn each_needs_its_prerequisites_at_or_below_it_and_its_mirror() {
    let h = 1_500;
    // F-EM needs F-L and F-E: on the bare release it is refused by name.
    let mut bare = palw_t12_release_v2_params();
    (PALW_T12_CAPACITY_EMISSION_BUDGET_V1.set)(&mut bare, Some(ForkActivation::new(h)));
    let why = bare.validate_palw_capacity_s567_v1().expect_err("F-EM without F-L and F-E");
    assert!(format!("{why:?}").contains("palw_capacity_emission_budget is armed without"), "{why:?}");
    // F-EM one DAA below F-E is refused; at F-E's height it is fine.
    let mut below = base_at(h);
    (PALW_T12_CAPACITY_EMISSION_BUDGET_V1.set)(&mut below, Some(ForkActivation::new(h - 1)));
    assert!(below.validate_palw_capacity_s567_v1().is_err(), "F-EM below F-E");
    // F-M1 without F-EM, and with F-EM above it.
    let mut no_emission = base_at(h);
    (PALW_T12_CAPACITY_MULTI_CLAIM_V1.set)(&mut no_emission, Some(ForkActivation::new(h)));
    let why = no_emission.validate_palw_capacity_s567_v1().expect_err("F-M1 without F-EM");
    assert!(format!("{why:?}").contains("palw_capacity_multi_claim is armed without"), "{why:?}");
    let mut late_emission = base_at(h);
    (PALW_T12_CAPACITY_EMISSION_BUDGET_V1.set)(&mut late_emission, Some(ForkActivation::new(h + 1)));
    (PALW_T12_CAPACITY_MULTI_CLAIM_V1.set)(&mut late_emission, Some(ForkActivation::new(h)));
    assert!(late_emission.validate_palw_capacity_s567_v1().is_err(), "F-EM above F-M1");
    // F-K needs F-L, F-S and F-Q.
    let mut breaker_bare = palw_t12_release_v2_params();
    (PALW_T12_CAPACITY_RHO_BREAKER_V1.set)(&mut breaker_bare, Some(ForkActivation::new(h)));
    let why = breaker_bare.validate_palw_capacity_s567_v1().expect_err("F-K without F-L, F-S and F-Q");
    assert!(format!("{why:?}").contains("palw_capacity_rho_breaker is armed without"), "{why:?}");
    // A field armed without its mirror disagrees with the bundle.
    let mut unsynced = all_at(h);
    unsynced.palw_capacity_rho_breaker = Some(ForkActivation::new(h + 1));
    let why = unsynced.validate_palw_capacity_s567_v1().expect_err("unsynced");
    assert!(format!("{why:?}").contains("disagrees with the V2 bundle's mirror"), "{why:?}");
    // A mirror without the field: the bundle says armed, the ruleset does not.
    let mut orphan = base_at(h);
    if let PalwConsensusMode::ConsensusV2(bundle) = &mut orphan.palw_consensus_mode {
        bundle.state = bundle.state.clone().with_capacity_s567_mirror(Some(h), None, None);
    }
    let why = orphan.validate_palw_capacity_s567_v1().expect_err("a mirror with the fences unarmed");
    assert!(format!("{why:?}").contains("without the fence armed"), "{why:?}");
    // Off ConsensusV2 the fold that reads them is not running.
    for set in [
        (|p: &mut Params| p.palw_capacity_emission_budget = Some(ForkActivation::new(1_500))) as fn(&mut Params),
        |p| p.palw_capacity_multi_claim = Some(ForkActivation::new(1_500)),
        |p| p.palw_capacity_rho_breaker = Some(ForkActivation::new(1_500)),
    ] {
        let mut v1 = MAINNET_PARAMS;
        set(&mut v1);
        assert!(v1.validate_palw_capacity_s567_v1().is_err_and(|e| format!("{e:?}").contains("off ConsensusV2")));
    }
    for (name, mut p) in presets() {
        p.palw_capacity_emission_budget = Some(ForkActivation::never());
        p.palw_capacity_multi_claim = Some(ForkActivation::never());
        p.palw_capacity_rho_breaker = Some(ForkActivation::never());
        assert_eq!(p.validate_palw_capacity_s567_v1(), Ok(()), "{name}: never() is dormant");
    }
}

#[test]
fn they_are_in_no_list_but_the_int11_one() {
    for name in NAMES {
        assert!(PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != name), "{name}: not in the DAA-750 list");
        assert!(PALW_T12_CAPACITY_FENCES_V1.iter().all(|f| f.name != name), "{name}: not in the capacity list");
        assert!(PALW_T12_CAPACITY_RHO10_FENCES_V1.iter().all(|f| f.name != name), "{name}: not in the ρ = 10 list");
    }
    let released = base_at(1_001);
    let mut p = released.clone();
    for f in entries() {
        (f.set)(&mut p, Some(ForkActivation::new(1_001)));
    }
    assert_eq!(mirror(&p), (Some(1_001), Some(1_001), Some(1_001)));
    for f in entries().into_iter().rev() {
        (f.set)(&mut p, None);
    }
    assert_eq!(mirror(&p), (None, None, None));
    assert_eq!(ids(&p), ids(&released));
}
