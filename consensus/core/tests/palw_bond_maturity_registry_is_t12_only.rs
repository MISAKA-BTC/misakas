//! **Lane maturity-ext (post-launch, 2026-09-26): ADR-0065 D1 on the model registry, under lane
//! maturity's own fence** (`Params::palw_bond_maturity_early`; user decision: "under the same
//! DAA-500 fence"). Lane maturity kept a bond registered after launch off the PANEL DRAW until its own
//! window had run; the registry still counted it as a ready seat toward a class's `ready ≥ k`
//! thresholds (and ADR-0147's jury still drew it — the fold's half of that is
//! `palw_state_v2`'s `bond_maturity_ext_v1`). So, on testnet-12's own fold:
//!
//! * as shipped the rule resolves to nothing at every DAA — below 1,000 and past it (the release
//!   never applied D1 to the registry) — on every preset; armed, from the fence on, with
//!   `palw_bond_maturity`'s window and the second clock's depth, across 1,000;
//! * a newcomer registered at DAA 13 that proves possession of both genesis model classes is a ready
//!   seat below the fence, is not one from the fence to DAA 1,012 (named `immature`), and is one again
//!   from 1,013 — the lifecycle row's `ready_seats` as the fold writes it, and the RPC's count beside it;
//!   the release counts it throughout;
//! * the genesis seats keep every class's readiness: with only their rows, the armed and the released
//!   chain fold the SAME state root at every step across the fence, and each class's lifecycle state
//!   is the release's with the newcomer's rows too (8k does not drop to `Held`).

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_BOND_MATURITY_WINDOW_DAA, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_SEAT_NOT_READY_IMMATURE_V1, PalwBondMaturityFoldV1, PalwSeatReadinessRowV1, palw_model_registry_ready_seats_v1,
    palw_seat_not_ready_reason_v1,
};

/// The fence, low: past the registration (13), well before `palw_bond_maturity`'s 1,000.
const FENCE: u64 = 60;
const REGISTERED: u64 = 13;
const NEWCOMER: u64 = 0x5EA7;
/// The DAAs the chains step to (every block is a span boundary on testnet-12's one-DAA spans), all
/// past the registry's activation grace (DAA 30: rows are stepped from it).
const STEPS: [u64; 14] = [31, 40, 59, 60, 61, 200, 500, 999, 1_000, 1_012, 1_013, 1_014, 1_100, 2_000];

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_bond_maturity_early = Some(ForkActivation::new(height));
    p
}

#[test]
fn the_registry_rule_is_dormant_as_shipped_and_the_early_fences_alone_when_armed() {
    for (name, p) in [
        ("testnet-12", palw_t12_shipped_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ] {
        for daa in [0, REGISTERED, FENCE, 500, 999, 1_000, 1_013, 5_000, u64::MAX - 1] {
            assert_eq!(p.palw_bond_maturity_registry_fold_at(daa), None, "{name} at {daa}: the released registry");
        }
    }
    // testnet-12 as shipped applies D1 to the DRAW from 1,000 and never to the registry.
    let shipped = palw_t12_shipped_params();
    assert_eq!(shipped.palw_bond_maturity_window_at(1_000), Some(PALW_T12_BOND_MATURITY_WINDOW_DAA));
    for height in [FENCE, 500, 999] {
        let armed = armed_at(height);
        armed.validate_palw_v2().expect("armed on a copy");
        let depth = armed.palw_settled_anchor_depth.expect("testnet-12 runs the second clock");
        for daa in [0, height - 1] {
            assert_eq!(armed.palw_bond_maturity_registry_fold_at(daa), None, "armed at {height}: nothing at {daa}");
        }
        for daa in [height, height + 1, 999, 1_000, 1_013, 5_000] {
            assert_eq!(
                armed.palw_bond_maturity_registry_fold_at(daa),
                Some(PalwBondMaturityFoldV1 { window_daa: PALW_T12_BOND_MATURITY_WINDOW_DAA, settled_anchor_depth: Some(depth) }),
                "armed at {height}: D1's window and the second clock's depth at {daa}"
            );
        }
    }
}

/// One chain on `p`: the newcomer registered at DAA 13 by the real fold (genesis collateral, the floor
/// declared); then at each of [`STEPS`] fresh V2 possession rows for every genesis bond — and the
/// newcomer when `newcomer_rows` — on both genesis model classes, and one block folded there. Returns
/// per step: the DAA, the state root, and each class's `(lifecycle state, ready_seats, RPC ready count,
/// newcomer's not-ready reason)`.
#[allow(clippy::type_complexity)]
fn walk(
    p: Params,
    newcomer_rows: bool,
) -> Vec<(u64, Hash64, Vec<(kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1, u32, u32, Option<&'static str>)>)> {
    let (c8k, c2m) = model_classes(&p);
    let base = bundle(&p).base_class_id;
    let mut chain = Chain::new(p.clone());
    chain.room = true;
    let registration = PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(NEWCOMER),
        pubkey: pubkey_of(NEWCOMER),
        operator_pubkey: operator_pubkey_of(NEWCOMER),
        collateral: PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI,
        payout_payload: h(0x9A00_5EA7),
        // The floor alone, as lane maturity's newcomer: readiness does not read a declaration.
        capable_classes: std::collections::BTreeSet::from([base]),
        signature: Vec::new(),
    };
    chain.step_at(REGISTERED, &[registration], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(chain.s.bond(&bond_key(NEWCOMER)).expect("registered").registered_daa, REGISTERED);
    let mut seats: Vec<PalwBondKeyV2> = genesis_bonds(&p).into_iter().map(|(k, _, _)| k).collect();
    assert_eq!(seats.len(), 8, "testnet-12's eight genesis bonds");
    assert!(seats.iter().all(|k| chain.s.bond(k).is_some_and(|b| b.registered_daa == 0)), "registered at DAA 0");
    if newcomer_rows {
        seats.push(bond_key(NEWCOMER));
    }
    let mut out = Vec::new();
    for daa in STEPS {
        let fold = registry_fold(&p, daa).expect("testnet-12's registry is in force");
        assert_eq!(fold.span_daa, 1, "testnet-12's one-DAA spans");
        assert!(fold.governs_at(daa), "DAA {daa}: past the grace, the rows are stepped");
        let mut carriage = PalwStateCarriageV2::from_state(&chain.s);
        for class in [c8k, c2m] {
            for seat in &seats {
                carriage.seat_readiness.insert(
                    (*seat, class),
                    PalwSeatReadinessRowV1 { proved_daa: daa, proved_span: daa, leaf_index: 0, proof_version: 2, chunks: 16 },
                );
            }
        }
        chain.s = carriage.into_state(&chain.sp, None).expect("the rows are a consistent state");
        chain.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        let fold = registry_fold(&p, daa).unwrap();
        let rows = [c8k, c2m]
            .into_iter()
            .map(|class| {
                let row = chain.s.model_lifecycle(&class).expect("a genesis class holds a row");
                let rpc = palw_model_registry_ready_seats_v1(&chain.s, &chain.sp, &class, daa, &fold);
                let reason = chain
                    .s
                    .seat_readiness(&bond_key(NEWCOMER), &class)
                    .and_then(|row| palw_seat_not_ready_reason_v1(&chain.s, &chain.sp, &bond_key(NEWCOMER), row, daa, &fold));
                (row.state, row.ready_seats, rpc, reason)
            })
            .collect();
        out.push((daa, chain.s.state_root(), rows));
    }
    out
}

#[test]
fn a_newcomer_is_a_ready_seat_below_the_fence_and_not_until_its_own_window_past_it() {
    let armed = walk(armed_at(FENCE), true);
    let shipped = walk(t12(), true);
    let window = PALW_T12_BOND_MATURITY_WINDOW_DAA;
    for ((daa, _, rows), (_, _, released)) in armed.iter().zip(&shipped) {
        let counted = *daa < FENCE || *daa >= REGISTERED + window;
        println!("DAA {daa:>5}: armed {rows:?}, released {released:?} — newcomer counted {counted}");
        for ((state, ready, rpc, reason), (released_state, released_ready, released_rpc, released_reason)) in rows.iter().zip(released) {
            assert_eq!(*ready, 8 + counted as u32, "DAA {daa}: the eight genesis seats, and the newcomer iff mature");
            assert_eq!(*rpc, *ready, "DAA {daa}: the RPC counts what the fold wrote");
            assert_eq!(*reason, (!counted).then_some(PALW_SEAT_NOT_READY_IMMATURE_V1), "DAA {daa}: the newcomer's reason");
            assert_eq!((*released_ready, *released_rpc, *released_reason), (9, 9, None), "DAA {daa}: the release counts it");
            assert_eq!(state, released_state, "DAA {daa}: the class's lifecycle is the release's (8 ≥ 7 genesis seats)");
            assert!(
                !matches!(state, kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::Held),
                "DAA {daa}: no class drops to Held"
            );
        }
    }
}

#[test]
fn the_genesis_seats_fold_the_released_root_across_the_fence() {
    let armed = walk(armed_at(FENCE), false);
    let shipped = walk(t12(), false);
    for ((daa, root, rows), (_, released_root, released)) in armed.iter().zip(&shipped) {
        assert_eq!(root, released_root, "DAA {daa}: genesis readiness alone — the armed fold is the release, byte for byte");
        assert_eq!(rows, released, "DAA {daa}: every class row and count");
        for (_, ready, _, _) in rows {
            assert_eq!(*ready, 8, "DAA {daa}: all eight genesis seats ready");
        }
    }
    // And with the newcomer's rows the roots part exactly where the rule does.
    let armed = walk(armed_at(FENCE), true);
    let shipped = walk(t12(), true);
    for ((daa, root, _), (_, released_root, _)) in armed.iter().zip(&shipped) {
        let parts = *daa >= FENCE && *daa < REGISTERED + PALW_T12_BOND_MATURITY_WINDOW_DAA;
        assert_eq!(root != released_root, parts, "DAA {daa}: the roots differ iff the newcomer is immature");
    }
}
