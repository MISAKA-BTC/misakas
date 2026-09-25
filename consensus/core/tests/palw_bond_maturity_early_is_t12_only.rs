//! **Lane maturity — ADR-0065 D1 brought forward to a post-launch fence
//! (`Params::palw_bond_maturity_early`, 2026-09-26) is dormant on every shipped preset, arming it is
//! a scheduled fence like any other, and past it only the genesis bonds are drawable until a newer
//! bond's own window has run.**
//!
//! testnet-12 launched from `0e8ec984e` with `palw_bond_maturity` (D1, window 1,000 DAA) scheduled at
//! DAA 1,000 — its own window, the earliest `validate_palw_v2` lets it sit — so for the first 1,000
//! DAA no window applies and a bond registered after launch is on a floor panel within minutes (a
//! drill: registered at DAA 13, seated at bind DAA 21). The fix ships behind this fence, which the
//! integrator arms at the one post-launch height (500). So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are
//!   the release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`);
//! * armed below 1,000 it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id`, and a `Some(never())` collapses to absence;
//! * the fork id names the height (and 1,000 itself is refused: a second fence there would be
//!   invisible to the fork id);
//! * `validate_palw_v2` refuses it without `palw_bond_maturity` scheduled above it, off ConsensusV2,
//!   and inside the window on a genesis above DAA 0 (the only case in which it starves the draw);
//! * the window in force is ONE window over two fences: `None` below the fence (the released rule),
//!   `palw_bond_maturity`'s from the fence on, identical to the released rule from 1,000 on;
//! * on testnet-12's own fold, a bond registered at DAA 13 is drawable below the fence, leaves the
//!   draw at the fence, and returns at anchor 1,013 (its own registration + window), while every
//!   genesis bond but the executor's stays eligible and a full jury is drawn at every anchor.
//!
//! The processor half — the acceptance layer and the assembler resolving one window on a real
//! testnet-12 chain that crosses the fence — is `consensus`'s `t12_seat_maturity_fence`.

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_BOND_MATURITY_WINDOW_DAA, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_bond_maturity_window_in_force_v1, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_panel_v2::{
    derive_panel_v2_with_maturity, palw_bond_maturity_window_v2, palw_panel_eligible_bonds_v2, palw_seat_maturity_floor_v1,
};

/// testnet-12 as the release ships it (`0e8ec984e`; pinned by the shipping re-pin `9c717c16d` as
/// `palw_clock_lead_cap_is_t12_only::T12_WITH_THE_CAP`): params, identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights the fence may take on testnet-12: the processor tests' low height, the release's 500, and
/// one just below `palw_bond_maturity`'s 1,000.
const HEIGHTS: [u64; 3] = [60, 500, 999];

/// The release height the integrator arms every post-launch fence at.
const RELEASE_FENCE: u64 = 500;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_bond_maturity_early = Some(ForkActivation::new(height));
    p
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

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's: the field, its
/// Some-only writers and its collapse cost the live chain nothing until an operator arms it.
#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_bond_maturity_early, None, "{name}: lane maturity's fence ships dormant");
        assert_eq!(p.palw_bond_maturity_early_fence(), None, "{name}");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_bond_maturity_early" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
    // And as shipped D1 is still the release's: no window below 1,000, 1,000 from it.
    for daa in [0, 13, 21, RELEASE_FENCE, 999] {
        assert_eq!(t12.palw_bond_maturity_window_at(daa), None, "shipped: no window at anchor {daa}");
    }
    assert_eq!(t12.palw_bond_maturity_window_at(1_000), Some(PALW_T12_BOND_MATURITY_WINDOW_DAA));
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — so an
/// armed node and a shipped node stay peers until the height (M1-6). A `Some(never())` is absence
/// (the fourth of the four places a Some-only fence needs).
#[test]
fn arming_moves_the_params_and_schedule_ids_but_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let (params_id, identity_id, schedule_id) = ids(&shipped);
    let mut seen = std::collections::BTreeSet::new();
    for height in HEIGHTS {
        let armed = armed_at(height);
        armed.validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: a runnable ruleset: {e:?}"));
        let (p, i, s) = ids(&armed);
        println!("testnet-12 armed at {height}: params {p} identity {i} schedule {s}");
        assert_ne!(p, params_id, "armed at {height}: the ruleset a node announces names the fence");
        assert_ne!(s, schedule_id, "armed at {height}: the schedule the operator log names it");
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert!(armed.palw_bond_maturity_window_at(height).is_some() && armed.palw_bond_maturity_window_at(height - 1).is_none());
    }
    let mut never = shipped.clone();
    never.palw_bond_maturity_early = Some(ForkActivation::never());
    never.validate_palw_v2().expect("never() is dormant");
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    assert_eq!(never.palw_bond_maturity_early_fence(), None, "and absence to the resolver");
}

/// **The fork id sees the height** (a-fence-at-a-scheduled-height-is-invisible-to-the-fork-id): the
/// fence is on the gate, at its own height, and not at testnet-12's scheduled 1,000. Below it an
/// armed build and the shipped build keep each other in BOTH directions; from it the armed build
/// refuses the shipped one — the flag day is a named refusal, not a silent fork.
#[test]
fn an_armed_build_below_its_fence_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    println!("testnet-12 as shipped gates on {shipped_gate:?}");
    for height in HEIGHTS {
        let armed = armed_at(height);
        let gate = fork_id_gate_fences_v1(&armed);
        assert!(gate.contains(&height), "armed at {height}: the height is on the fork-id gate ({gate:?})");
        assert!(!shipped_gate.contains(&height), "armed at {height}: an INDEPENDENT height, not one the release schedules");
        for daa in [0, 1, height / 2, height - 1] {
            let (a, s) = (fork_id_v1(&armed, daa), fork_id_v1(&shipped, daa));
            let armed_sees = evaluate_fork_id_v1(&armed, daa, s.fired.as_bytes().as_slice(), s.next);
            let shipped_sees = evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next);
            assert!(
                !armed_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the armed node keeps the shipped one ({armed_sees:?})"
            );
            assert!(
                !shipped_sees.refuses(),
                "armed at {height}, both at DAA {daa}: the shipped node keeps the armed one ({shipped_sees:?})"
            );
        }
        let s = fork_id_v1(&shipped, height);
        let past = evaluate_fork_id_v1(&armed, height, s.fired.as_bytes().as_slice(), s.next);
        println!("armed at {height}: at the height the armed node says {past:?} to a shipped peer");
        assert!(past.refuses(), "armed at {height}: from the height the armed node refuses a node that did not upgrade");
    }
}

/// **What `validate_palw_v2` refuses**, each by name: no `palw_bond_maturity` to take the window from;
/// a height at or past `palw_bond_maturity`'s own (1,000 is invisible to the fork id, and past it the
/// fence brings nothing forward); a network that is not ConsensusV2; and the one case in which D1
/// armed inside its window starves the draw — a genesis above DAA 0.
#[test]
fn what_the_fence_refuses() {
    let named = |p: &Params| p.validate_palw_v2().is_err_and(|e| format!("{e:?}").contains("palw_bond_maturity_early"));

    // No `palw_bond_maturity`: testnet-11 and devnet schedule no D1.
    for (name, mut p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        assert!(p.palw_bond_maturity.is_none(), "{name}: no D1 of its own");
        p.palw_bond_maturity_early = Some(ForkActivation::new(RELEASE_FENCE));
        println!("{name} armed: {:?}", p.validate_palw_v2());
        assert!(named(&p), "{name}: no palw_bond_maturity, no window to bring forward — refused by name");
    }
    // testnet-12 with its D1 unscheduled: the same refusal.
    let mut unscheduled = armed_at(RELEASE_FENCE);
    unscheduled.palw_bond_maturity = None;
    assert!(named(&unscheduled), "no palw_bond_maturity on testnet-12: refused by name");

    // At or past palw_bond_maturity's own height.
    for height in [1_000, 1_001, 5_000] {
        assert!(named(&armed_at(height)), "armed at {height}: at or past D1's own height is refused by name");
    }

    // Not ConsensusV2 (testnet-10's params).
    let mut v1 = Params::from(TESTNET_PARAMS.net);
    v1.palw_bond_maturity_early = Some(ForkActivation::new(RELEASE_FENCE));
    assert!(
        v1.validate_palw_bond_maturity_early_v1().is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")),
        "off ConsensusV2 there is no V2 panel to draw"
    );

    // The genesis case, alone (the rest of `validate_palw_v2` has its own view of a moved genesis): a
    // genesis at DAA 5 registers its bonds at 5, and `anchor - 1,000` saturating at 0 stays below them
    // until 1,005 — so the early fence is refused below that and admitted from it (D1's own height
    // moved past it so the height rule does not answer first).
    let mut shifted = armed_at(RELEASE_FENCE);
    shifted.genesis.daa_score = 5;
    shifted.palw_bond_maturity.as_mut().unwrap().activation = ForkActivation::new(5_000);
    assert!(
        shifted.validate_palw_bond_maturity_early_v1().is_err_and(|e| format!("{e:?}").contains("genesis above DAA 0")),
        "inside the window on a genesis above DAA 0: refused"
    );
    shifted.palw_bond_maturity_early = Some(ForkActivation::new(1_005));
    shifted.validate_palw_bond_maturity_early_v1().expect("at genesis + window the genesis bonds are mature");

    // Dormant is legal anywhere; armed below 1,000 over testnet-12's genesis D1 is legal at any height.
    for (name, mut p) in presets() {
        p.palw_bond_maturity_early = Some(ForkActivation::never());
        p.validate_palw_bond_maturity_early_v1().unwrap_or_else(|e| panic!("{name}: never() is dormant: {e:?}"));
    }
    for height in [0, 1, 60, RELEASE_FENCE, 999] {
        armed_at(height).validate_palw_v2().unwrap_or_else(|e| panic!("armed at {height}: legal on testnet-12: {e:?}"));
    }
}

/// **One window over two fences — no double counting, no gap at 1,000.** Below the fence nothing
/// (the released rule, byte for byte); from the fence `palw_bond_maturity`'s 1,000; from 1,000 on the
/// armed and the shipped ruleset answer the same window for every anchor, so past D1's own height the
/// draw is exactly the release's.
#[test]
fn one_window_on_both_fences() {
    let shipped = palw_t12_shipped_params();
    for fence in HEIGHTS {
        let armed = armed_at(fence);
        for daa in (0..=2_100u64).chain([5_000, 1_000_000, u64::MAX - 1]) {
            let (a, s) = (armed.palw_bond_maturity_window_at(daa), shipped.palw_bond_maturity_window_at(daa));
            match daa {
                d if d < fence => assert_eq!(a, None, "fence {fence}, anchor {d}: below the fence no window, as released"),
                d if d < 1_000 => {
                    assert_eq!(a, Some(PALW_T12_BOND_MATURITY_WINDOW_DAA), "fence {fence}, anchor {d}: D1's window, early");
                    assert_eq!(s, None, "anchor {d}: the release had none");
                }
                d => assert_eq!(a, s, "fence {fence}, anchor {d}: from 1,000 the armed and released rule are one"),
            }
            // The free function the processor calls is the same answer.
            assert_eq!(
                a,
                palw_bond_maturity_window_in_force_v1(armed.palw_bond_maturity, armed.palw_bond_maturity_early_fence(), daa)
            );
        }
    }
    // Without palw_bond_maturity the early fence answers nothing (and `validate_palw_v2` refuses it).
    assert_eq!(palw_bond_maturity_window_in_force_v1(None, Some(ForkActivation::new(RELEASE_FENCE)), 700), None);

    // **The second clock composes and cannot starve the genesis bonds.** Past the fence the processor
    // widens the window by the settled-anchor floor exactly as it does past 1,000
    // (`palw_bond_maturity_window_v2`). Between the fence and 1,000 the DAA window alone already puts
    // the floor at 0 (`anchor - 1,000` saturates), and the widening only ever LOWERS it — so whatever
    // the second clock answers (the bootstrap waiver `None`, the pruned `Some(0)`, or any settled
    // anchor), the floor is exactly 0: every genesis bond (registered at 0) and nothing registered
    // after launch. From 1,000 the two clocks answer as they do on the release.
    let armed = armed_at(RELEASE_FENCE);
    for anchor in [RELEASE_FENCE, 501, 750, 999] {
        let window = armed.palw_bond_maturity_window_at(anchor).expect("in force past the fence");
        for settled in [None, Some(0), Some(1), Some(anchor / 2), Some(anchor - 1)] {
            let floor = palw_seat_maturity_floor_v1(anchor, Some(palw_bond_maturity_window_v2(anchor, window, settled)));
            assert_eq!(floor, Some(0), "anchor {anchor}, settled floor {settled:?}: genesis bonds only, never an outsider");
        }
    }
}

/// **On testnet-12's own fold, the draw.** A newcomer bond — genesis collateral, the floor declared,
/// registered by the real fold at DAA 13 — and a floor claim by genesis card 0. For each anchor the
/// window is resolved as the processor resolves it (`palw_bond_maturity_window_at`, widened by the
/// second clock, which waives itself here: no anchor has settled), and the draw is asked for its
/// eligible population and its jury:
///
/// * below the fence (and at every anchor on the shipped ruleset before 1,000) the newcomer is
///   eligible — the released behaviour, the drill's "seated at bind DAA 21";
/// * from the fence to 1,012 it is not — its window counts from its own registration, no grace for
///   having registered before the fence;
/// * from 1,013 (13 + 1,000) it is eligible again, on the armed ruleset and the shipped one alike;
/// * every genesis bond but the executor's is eligible at every anchor, and a full jury is drawn at
///   every anchor — no claim voids for want of seats.
#[test]
fn a_bond_registered_at_daa_13_leaves_the_draw_at_the_fence_and_returns_at_its_own_window() {
    const REGISTERED: u64 = 13;
    let shipped = t12();
    let b = bundle(&shipped);
    let base = b.base_class_id;
    let mut chain = Chain::new(shipped.clone());
    let newcomer = bond_key(0x5EA7);
    let registration = PalwConsensusObjectV2::BondRegistered {
        bond: newcomer,
        pubkey: pubkey_of(0x5EA7),
        operator_pubkey: operator_pubkey_of(0x5EA7),
        collateral: PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI,
        payout_payload: h(0x9A00_5EA7),
        capable_classes: std::collections::BTreeSet::from([base]),
        signature: Vec::new(),
    };
    chain.step_at(REGISTERED, &[registration], PalwBlockWorkV3::None, Hash64::default(), 0);
    let record = chain.s.bond(&newcomer).expect("the fold registered the newcomer").clone();
    assert_eq!(record.registered_daa, REGISTERED, "registered_daa is the accepting block's DAA");
    let claim_id = chain.floor_claim(0x5EA7);
    let executor = chain.claim(&claim_id).bond;
    let genesis: Vec<PalwBondKeyV2> = genesis_bonds(&shipped).into_iter().map(|(k, _, _)| k).collect();
    assert_eq!(genesis.len(), 8, "testnet-12's eight genesis bonds");
    assert!(genesis.iter().all(|k| chain.s.bond(k).is_some_and(|b| b.registered_daa == 0)), "genesis bonds register at DAA 0");
    let min_collateral = b.state.min_collateral_sompi();

    // What the processor's `palw_bond_maturity_window_at` computes: D1's window at the anchor, widened
    // by the second clock (waived here: fewer than `depth` anchors have settled), then the one floor.
    let registered_by = |p: &Params, anchor: u64| -> Option<u64> {
        let depth = p.palw_settled_anchor_depth.expect("testnet-12 runs the second clock");
        let floor = kaspa_consensus_core::palw_panel_v2::palw_settled_anchor_floor_daa_v1(&chain.s, anchor, depth);
        assert_eq!(floor, None, "no anchor has settled on this chain: the bootstrap waiver");
        palw_seat_maturity_floor_v1(
            anchor,
            p.palw_bond_maturity_window_at(anchor).map(|w| palw_bond_maturity_window_v2(anchor, w, floor)),
        )
    };
    let eligible = |p: &Params, anchor: u64| -> Vec<PalwBondKeyV2> {
        palw_panel_eligible_bonds_v2(
            &chain.s,
            &claim_id,
            min_collateral,
            registered_by(p, anchor),
            p.palw_capability_bound_at(anchor),
            None,
            None,
            b.panel.seat_count(),
        )
        .expect("the claim is live")
        .into_iter()
        .map(|(k, _)| *k)
        .collect()
    };
    let jury = |p: &Params, anchor: u64| -> Vec<PalwBondKeyV2> {
        derive_panel_v2_with_maturity(
            &chain.s,
            &b.panel,
            &claim_id,
            BlockHash::from_u64_word(0xA2C4_0000 + anchor),
            min_collateral,
            registered_by(p, anchor),
        )
        .unwrap_or_else(|e| panic!("anchor {anchor}: a full jury is drawn: {e:?}"))
        .into_iter()
        .map(|seat| seat.bond)
        .collect()
    };

    let mut newcomer_seated_below = 0usize;
    for fence in [60u64, RELEASE_FENCE] {
        let armed = armed_at(fence);
        armed.validate_palw_v2().expect("armed on a copy");
        println!("=== fence at {fence} ===");
        for anchor in [15, 21, 59, 60, 61, 499, 500, 501, 999, 1_000, 1_012, 1_013, 1_014, 2_000] {
            let (on, off) = (eligible(&armed, anchor), eligible(&shipped, anchor));
            let panel = jury(&armed, anchor);
            let drawable = on.contains(&newcomer);
            println!(
                "anchor {anchor:>5}: registered-by {:?}, newcomer drawable {drawable} (released rule: {}), {} eligible, jury {} seats{}",
                registered_by(&armed, anchor),
                off.contains(&newcomer),
                on.len(),
                panel.len(),
                if panel.contains(&newcomer) { " (newcomer seated)" } else { "" }
            );
            let expect = anchor < fence || anchor >= REGISTERED + PALW_T12_BOND_MATURITY_WINDOW_DAA;
            assert_eq!(drawable, expect, "fence {fence}, anchor {anchor}: newcomer drawable");
            assert_eq!(off.contains(&newcomer), anchor < 1_000 || anchor >= 1_013, "the released rule at anchor {anchor}");
            if anchor >= 1_000 {
                assert_eq!(on, off, "fence {fence}, anchor {anchor}: from D1's own height the populations are the release's");
            }
            for g in genesis.iter().filter(|g| **g != executor) {
                assert!(on.contains(g), "fence {fence}, anchor {anchor}: genesis bond {g:?} stays eligible");
            }
            assert!(!on.contains(&executor), "the executor never sits on its own panel");
            assert_eq!(panel.len(), b.panel.seat_count() as usize, "fence {fence}, anchor {anchor}: a full jury");
            if !expect {
                assert!(!panel.contains(&newcomer), "fence {fence}, anchor {anchor}: an immature bond is never seated");
                assert!(panel.iter().all(|s| genesis.contains(s)), "fence {fence}, anchor {anchor}: a jury of genesis bonds");
            } else if anchor < fence && panel.contains(&newcomer) {
                newcomer_seated_below += 1;
            }
        }
    }
    println!("newcomer seated on {newcomer_seated_below} of the below-fence draws");
}
