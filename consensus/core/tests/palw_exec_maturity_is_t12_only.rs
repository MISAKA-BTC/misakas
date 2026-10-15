//! **testnet-12's execution-quantum maturity is the challenge window it applies — 120 DAA — and no
//! other network moves** (user decision 2026-09-25: 「短縮前の 1,200 DAA のまま → 120 に変更して」).
//!
//! ADR-0151 delays a Final's execution quanta by a maturity before a round permit may spend one, and
//! prices the rights still convictable after it into `max_fraud_gain`. The maturity was
//! `palw_exec_quantum_maturity_daa_v1(window_challenge(), window_court)` — the UNSHORTENED lattice
//! window, 1,200 DAA — while testnet-12 licenses every claim under ADR-0132 §7.6's 120-DAA short
//! window from genesis; so its first round permit came ~1,342 DAA (~45 h) after launch.
//!
//! `Params::palw_exec_quantum_maturity_daa` states it: `Some(120)` on testnet-12, `None` (the lattice
//! rule, byte for byte) everywhere else; read through `Params::palw_exec_quantum_maturity_v1` into
//! `PalwEconomicSafetyFoldV1::maturity_daa`, the one value the snapshot delay and the lock both read.
//! Hashed Some-only in `consensus_params_id` and `consensus_schedule_id`, and dropped from
//! `consensus_identity_id` with a scheduled `palw_economic_safety`, so testnet-11, devnet and mainnet
//! fingerprint byte-identically to the build before the field.
//!
//! **What the shorter maturity costs, re-derived here** (the economic-safety table): nothing on
//! testnet-12. The residual is `min(quanta, rounds_in(window_court − maturity)) × permit`, and past
//! the 2026-09-23 audit a Final prices at most `2^16 + 1` quanta — fewer than either gap holds
//! (216,000 rounds at 1,200, 345,600 at 120) — so `max_fraud_gain`, every seat lock read from it
//! and the genesis collateral a seat posts are the same figures at both maturities.

use kaspa_consensus_core::config::params::{
    ForkActivation, OverrideParams, Params, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_exec_quantum_maturity_daa_v1, palw_permit_value_sompi_v1,
    palw_realizable_before_maturity_v1, palw_rounds_per_daa_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{
    PALW_EXEC_MAX_QUANTA_PER_SPAN_V1, PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_offence_v1::{PALW_PANEL_COLLUDING_QUORUM_V1, palw_colluding_quorum_covers_v1};
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_state_v2::{PALW_RCORE_FINAL_BASIS_K_V1, PALW_SHORT_CHALLENGE_WINDOW_DAA_V1, palw_rcore_lock_v1};
use kaspa_hashes::Hash64;

/// `(network, consensus_params_id, consensus_identity_id, consensus_schedule_id)` — the values the other
/// testnet-12-only fences pin at `rcore/int-3` `2004c588` (`palw_rcore_plus_is_t12_only`'s `AT_V22`, with
/// mainnet's 2026-09-25 DNS-set re-pin), unmoved by the maturity.
const UNMOVED: &[(&str, &str, &str, &str)] = &[
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was bd633ce9…, 44cb8fd7…, 5a1d8d56…)
        "testnet-11",
        "0c2c022a11572d73779fc0610cf65d4147feb499a9b58472901d3d73ecfc1f71",
        "bf4cc0b3e2fa208a18cce48d0c33db74e48adf275a069687b534856411f1c83e",
        "f023f74befcdaf732f213616c79fd94782d7728f835151bbfd986498f962bd64",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 7a27f341…, 7a27f341…, edd80c01…)
        "devnet",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "f17c6555a617736bd13e90eaab80974223149c211d08af516008bd6500010b1f",
        "d82068a0080c47f45f376c81c621ec002aa8541a22c49e9d0a373e263c39c680",
    ),
    (
        // re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was eb866c61…, 7819e5ed…, a1ed7ff0…)
        "mainnet",
        "0befd2fa1da052fb88ef8a04cc1bd6412ea560371aaec606790194dee41ac3fa",
        "db7f8f9869a612c6e90ff8a01806569a0fe7391cfa8226c3c3a89680ae83d277",
        "7b8f4090c31d524312f1b6d8a91d0e47544b031a62560d90b9a1bc65b709e7c6",
    ),
];

/// testnet-12 with the maturity taken away: testnet-12's own ruleset at `rcore/int-3` `2004c588`, the
/// tree the maturity landed on (the field is hashed Some-only, and the pinned twins of
/// `palw_offence_attribution_is_t12_only`, `evm_bridge_ledger_is_t12_only` and
/// `t12_mainnet_values_moved_only_these` hold unchanged with it taken away — so nothing else moved).
/// The re-pin (`scripts/t12_repin.py`, `maturity.*`) moves it with every other twin.
// re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 199d11f3…, e4add02e…, 47f346f0…)
const T12_WITHOUT_THE_MATURITY: (&str, &str, &str) = (
    "2916df76f8f1351672b8fc27ea2e13fe2b1329e91254400eaf5f239da8bac1ff",
    "5ebd92ed4a3b18c7fb554c1966934115c046b148b9050e14e5c62cba92f6306b",
    "09de753e9ee329c7245d31db970f1dfc1b14b25c4561bd528718283b50dce834",
);

/// testnet-12 with the 120-DAA maturity: the shipped preset's full ids (= `palw_readiness_horizon_is_t12_only`'s
/// `T12_WITH_THE_HORIZON`). Taken from this test's own output.
// re-pin 2026-10-08 @e35409e2a: RFC-0012 removes DNS BFT; new ruleset only (was 5ee7fd8e…, 5de80e64…, 1678e073…)
const T12_WITH_THE_MATURITY: (&str, &str, &str) = (
    "59a6d1b00434b42129201098fafb2c889a27d6392a87e531df6e1409beeb6ce7",
    "92042711856b239713d0376b7466422e561618fc51e115e1557be34b078e5acd",
    "bc0600cc47b9433b99b715e902e3892e86295460b98cc1893c4fccc26f2b780c",
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

fn pinned((a, b, c): (&str, &str, &str)) -> (String, String, String) {
    (a.to_string(), b.to_string(), c.to_string())
}

fn without_the_maturity(t12: &Params) -> Params {
    let mut twin = t12.clone();
    twin.palw_exec_quantum_maturity_daa = None;
    twin
}

/// Stated on testnet-12 alone, at the challenge window it applies to every licence from genesis; every
/// other preset reads the lattice rule (or nothing, where no V2 bundle exists).
#[test]
fn the_maturity_is_stated_on_testnet12_only() {
    let t12 = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    assert_eq!(t12.palw_exec_quantum_maturity_daa, Some(PALW_SHORT_CHALLENGE_WINDOW_DAA_V1));
    assert_eq!(PALW_SHORT_CHALLENGE_WINDOW_DAA_V1, 120);
    assert_eq!(t12.palw_exec_quantum_maturity_v1(), 120, "the maturity the fold serves and prices");
    assert_eq!(t12.palw_exec_quantum_maturity_v1(), bundle.state.window_challenge_at(0), "= the challenge window testnet-12 applies");
    assert_eq!(bundle.state.window_challenge(), 1_200, "not the unshortened lattice window");
    assert!(t12.palw_economic_safety.is_some_and(|f| f.is_active(0)), "the bundle it rides is armed from genesis");
    t12.validate_palw_v2().expect("testnet-12 validates with the maturity");
    for (name, _, _, _) in UNMOVED {
        let p = shipped(name);
        assert_eq!(p.palw_exec_quantum_maturity_daa, None, "{name}: not stated");
        assert!(p.palw_economic_safety.is_none(), "{name}: ADR-0151's bundle is dormant, so no maturity is served at all");
        let lattice = match &p.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(b) => palw_exec_quantum_maturity_daa_v1(b.state.window_challenge(), b.state.window_court()),
            _ => 0,
        };
        assert_eq!(p.palw_exec_quantum_maturity_v1(), lattice, "{name}: the None rule");
    }
    assert_eq!(palw_rc_shipped_params().palw_exec_quantum_maturity_v1(), 1_200, "testnet-11 would read the unshortened window");
}

/// **testnet-11, devnet and mainnet are byte-identical to the build before the maturity.**
#[test]
fn testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_maturity() {
    let mut moved = Vec::new();
    for (name, params_id, identity_id, schedule_id) in UNMOVED {
        let now = ids(&shipped(name));
        if now != pinned((params_id, identity_id, schedule_id)) {
            moved.push(format!("{name}: {now:?}"));
        }
    }
    assert!(moved.is_empty(), "a preset that does not state the maturity moved: {moved:?}");
}

/// **The maturity moves testnet-12's fingerprint — and so does its value — and nothing else did.**
/// Taken away, testnet-12 is exactly the ruleset before it; a node that states another maturity
/// announces another ruleset; and a scheduled bundle takes the maturity out of the identity with it.
#[test]
fn the_maturity_moves_testnet12s_fingerprint_and_nothing_else_did() {
    let t12 = palw_t12_shipped_params();
    let twin = without_the_maturity(&t12);
    twin.validate_palw_v2().expect("the maturity is optional: testnet-12 without it is a legal ruleset");
    // Printed before the asserts, for `scripts/t12_repin.py`'s harvest.
    println!("testnet-12 with the maturity {:?} / without the maturity {:?}", ids(&t12), ids(&twin));
    assert_eq!(ids(&twin), pinned(T12_WITHOUT_THE_MATURITY), "taken away, testnet-12 is the ruleset before it");
    assert_eq!(ids(&t12), pinned(T12_WITH_THE_MATURITY), "testnet-12's ids with the maturity");
    assert_ne!(t12.consensus_params_id(), twin.consensus_params_id(), "the ruleset a node announces names the maturity");
    assert_ne!(t12.consensus_identity_id(), twin.consensus_identity_id(), "in force from block one: two identities");
    assert_ne!(t12.consensus_schedule_id(), twin.consensus_schedule_id(), "and the schedule reports it");
    let mut other = t12.clone();
    other.palw_exec_quantum_maturity_daa = Some(600);
    other.validate_palw_v2().expect("600 DAA is a maturity inside the horizon");
    assert_ne!(t12.consensus_params_id(), other.consensus_params_id(), "the value is in the params id");
    assert_ne!(t12.consensus_schedule_id(), other.consensus_schedule_id(), "and in the schedule id");
    // A scheduled — not yet active — bundle drags its maturity out of the identity (audit3 H1): two
    // builds that differ only in the maturity of a FUTURE bundle stay peers until it fires.
    let scheduled = |maturity: Option<u64>| {
        let mut p = t12.clone();
        p.palw_economic_safety = Some(ForkActivation::new(5_000));
        p.palw_exec_quantum_maturity_daa = maturity;
        p
    };
    assert_eq!(
        scheduled(Some(120)).consensus_identity_id(),
        scheduled(None).consensus_identity_id(),
        "a scheduled bundle's maturity is not an identity difference"
    );
    assert_ne!(scheduled(Some(120)).consensus_schedule_id(), scheduled(None).consensus_schedule_id(), "the schedule still reports it");
}

/// **`validate_palw_v2` refuses the maturity without ADR-0151's bundle, past the liability horizon,
/// and at the lattice window (the `None` rule spelled twice) — and admits any other value.**
#[test]
fn the_maturity_rides_its_bundle_inside_the_horizon() {
    let t12 = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let court = bundle.state.window_court();
    assert_eq!(court, 3_000);
    let with = |maturity: u64| {
        let mut p = t12.clone();
        p.palw_exec_quantum_maturity_daa = Some(maturity);
        p.validate_palw_v2()
    };
    for (maturity, legal, needle) in [
        (0, true, ""),
        (120, true, ""),
        (600, true, ""),
        (2_453, true, ""),
        (court, true, ""),
        (court + 1, false, "past window_court"),
        (1_200, false, "states the lattice challenge window"),
    ] {
        match with(maturity) {
            Ok(()) => assert!(legal, "{maturity} DAA validated"),
            Err(why) => {
                assert!(!legal, "{maturity} DAA refused: {why:?}");
                assert!(format!("{why:?}").contains(needle), "{maturity}: {why:?}");
            }
        }
    }
    // testnet-11 cannot state it: ADR-0151's bundle is dormant there.
    let mut t11 = palw_rc_shipped_params();
    t11.palw_exec_quantum_maturity_daa = Some(120);
    let why = t11.validate_palw_v2().expect_err("no bundle, no maturity");
    assert!(format!("{why:?}").contains("stated without palw_economic_safety"), "{why:?}");
    // And an override drops it with the bundle it rides.
    let overrides: OverrideParams = serde_json::from_str("{}").expect("an empty override");
    let overridden = t12.clone().override_params(overrides);
    assert_eq!(overridden.palw_economic_safety, None);
    assert_eq!(overridden.palw_exec_quantum_maturity_daa, None, "rides palw_economic_safety through an override");
}

/// **The economic-safety table, re-derived at 1,200 and at 120 DAA: every figure that reads the
/// residual is the same, and every one is covered.**
///
/// Per genesis class, in the unit the runtime reserves in (`t12_collateral_terms`' exposure pwu, a
/// genesis-era escrow of 3,200.85 MSK): the quanta the fold prices (`min(count, 2^16) + 1`), the
/// residual, `max_fraud_gain`, the three-colluder and one-colluder seat locks, R-core+'s Final-basis
/// lock, and the genesis seat collateral. The invariant — the smallest colluding set's slashable locks
/// out-value the most the lie definitely extracts — holds at both maturities with the same margin.
#[test]
fn the_economic_safety_table_is_the_same_at_1200_and_at_120() {
    let t12 = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let court = bundle.state.window_court();
    let cadence = t12.target_time_per_block();
    assert_eq!(palw_rounds_per_daa_v1(cadence), 120);
    let base = kaspa_consensus_core::config::params::palw_t12_base_params();
    let subsidy = kaspa_consensus_core::config::params::palw_genesis_block_subsidy_sompi(&base);
    let carve = u64::from(base.palw_overlay_carve.map(|c| c.worker_carve_permille).unwrap_or(0));
    let escrow = subsidy / 1_000 * carve;
    assert_eq!(escrow, 320_084_650_080, "3,200.85 MSK: the escrow a genesis-era claim carries");
    let permit = palw_permit_value_sompi_v1(PALW_T12_PERMIT_FEE_CEILING_SOMPI);
    let msk = |s: u128| s as f64 / 1e8;
    let seat = u128::from(PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI);
    assert_eq!(seat, 93_906_321_001_040, "939,063.21 MSK a genesis seat — not re-derived here: it prices no rights");

    // `t12_collateral_terms`: the exposure pwu each genesis class reserves in, and its price.
    const SLASH: u64 = 5;
    let classes: [(&str, u64); 3] =
        [("BASE-0 floor", 7_708), ("held Qwen2.5 @8,192", 494_320_046), ("held Qwen2.5 @2M", 1_194_858_841_364)];
    println!("\n  maturity 1,200 -> 120 DAA; gap {} -> {} DAA; permit {} sompi", court - 1_200, court - 120, permit);
    for (name, exposure) in classes {
        let counted = palw_execution_quantum_count_v1(
            u128::from(exposure),
            u128::from(PALW_EXECUTION_QUANTUM_V1),
            Hash64::default(),
            Hash64::default(),
        )
        .min(PALW_EXEC_MAX_QUANTA_PER_SPAN_V1 as u32);
        let quanta = counted.saturating_add(1);
        let row = |maturity: u64| {
            let residual = palw_realizable_before_maturity_v1(quanta, maturity, court, cadence, permit);
            let facts = PalwClaimFraudFactsV1 {
                reserved: u128::from(exposure) * u128::from(SLASH),
                escrowed_reward: escrow,
                exposure_pwu: exposure,
                slash_value_per_pwu: SLASH,
                extra_economic_rights_sompi: residual,
            };
            let gain = palw_max_fraud_gain_v1(&facts);
            let lock_3 = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
            let lock_1 = palw_seat_lock_required_v2(gain, 1);
            let lock_final = palw_rcore_lock_v1(gain - u128::from(escrow), escrow, 0, PALW_RCORE_FINAL_BASIS_K_V1);
            (residual, gain, lock_3, lock_1, lock_final)
        };
        let (r_long, g_long, l3_long, l1_long, lf_long) = row(1_200);
        let (r, g, l3, l1, lf) = row(PALW_SHORT_CHALLENGE_WINDOW_DAA_V1);
        println!(
            "  {name:<22} quanta {quanta:>6}  residual {:>8.2} -> {:>8.2} MSK  G {:>10.2} -> {:>10.2}  lock(3) {:>10.2}  lock(1) {:>10.2}  \
             lock(k'=2) {:>10.2}  seat {:.2} MSK",
            msk(r_long),
            msk(r),
            msk(g_long),
            msk(g),
            msk(l3),
            msk(l1),
            msk(lf),
            msk(seat),
        );
        assert_eq!(r, u128::from(quanta) * u128::from(permit), "{name}: at 120 every priced quantum is in the residual");
        assert_eq!((r, g, l3, l1, lf), (r_long, g_long, l3_long, l1_long, lf_long), "{name}: 1,200 -> 120 moves no figure");
        assert!(palw_colluding_quorum_covers_v1(l3, PALW_PANEL_COLLUDING_QUORUM_V1, g), "{name}: three colluding locks cover G");
        assert!(l1 > g, "{name}: the one S2 full seat covers G alone");
        // R-core+ vests the escrow (its rows burn `E` on a conviction), so its Final-basis locks price
        // the residual `G_res = G − E` — which carries the rights term — with the tenth of margin.
        assert!(lf * u128::from(PALW_RCORE_FINAL_BASIS_K_V1) > g - u128::from(escrow), "{name}: k' = 2 Final-basis locks cover G_res");
        assert!(l1 < seat, "{name}: the dearest lock fits one genesis seat");
    }
    // The 2M row, pinned: 65,537 quanta, 655.37 MSK of residual, G = 63,599.16 MSK at either maturity.
    let two_m = 1_194_858_841_364u64;
    let r = palw_realizable_before_maturity_v1(65_537, PALW_SHORT_CHALLENGE_WINDOW_DAA_V1, court, cadence, permit);
    assert_eq!(r, 65_537_000_000);
    let g = palw_max_fraud_gain_v1(&PalwClaimFraudFactsV1 {
        reserved: 0,
        escrowed_reward: escrow,
        exposure_pwu: two_m,
        slash_value_per_pwu: SLASH,
        extra_economic_rights_sompi: r,
    });
    assert_eq!(g, 6_359_915_856_900, "63,599.15856900 MSK");
}
