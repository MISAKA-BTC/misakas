//! **Lane A — the operator-anchor fence (`Params::palw_operator_anchor`, post-launch, 2026-09-26) is
//! dormant on every shipped preset, names testnet-12's eight genesis bonds when armed, and arming it is
//! a scheduled fence like any other.**
//!
//! The user's decision (2026-09-26 01:00 JST) for the panel-seed CRITICAL: beside lane F1, past a fence
//! only an attempt produced by one of testnet-12's eight genesis bonds (premine `5e0d5f1b…:0..7`) may
//! anchor a claim's panel — operator trust in the draw until an inference-bound lottery ticket exists.
//! So:
//!
//! * as shipped the field is `None` everywhere, testnet-12 included, and testnet-12's three ids are the
//!   release's to the byte (`b8564b88…` / `5de80e64…` / `93da24cc…`);
//! * testnet-12's armed value names exactly the eight genesis cards — their premine collateral
//!   outpoints `0..7`, with the keys the cards registered;
//! * armed at a future height it moves `consensus_params_id` and `consensus_schedule_id` but NOT
//!   `consensus_identity_id` (and `Some(never())` is absence — the fourth of the four places); the
//!   params id names the operator list, so two builds trusting different operators at one height
//!   announce different rulesets;
//! * the fork id names the height: below it an armed build and the shipped build keep each other, from
//!   it the armed build refuses the shipped one — armed with lane F1 at one common height, or above it;
//! * `validate_palw_v2` refuses it without `palw_rcore_plus` and lane F1 (`palw_panel_seed_execution`)
//!   at or below it, off ConsensusV2, and with an operator list that is empty, unsorted or names a bond
//!   the genesis does not register.
//!
//! The rule is tested at the pure layer (`palw_operator_anchor_v1::tests`), on a processor chain that
//! crosses the fence (`t12_operator_anchor_fence`), and against the attacker (`void_after_panel_probe`
//! T3c, `void_after_panel_adv_verify` a5).

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_GENESIS_BONDS, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::config::premine::premine_outpoint_for;
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_operator_anchor_v1::PalwOperatorAnchorV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;

/// testnet-12 as the release ships it (`0e8ec984e`; the shipping re-pin `9c717c16d`): params,
/// identity, schedule.
const T12_RELEASE: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// Heights an operator might pick after launch. Never 1,000 — `palw_bond_maturity`'s height on
/// testnet-12, where a second fence would be invisible to the fork id.
const HEIGHTS: [u64; 3] = [900, 1_234, 5_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

/// testnet-12 as shipped with lane F1 at `height` — lane A's prerequisite.
fn f1_at(height: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_panel_seed_execution = Some(ForkActivation::new(height));
    p
}

/// testnet-12 as shipped with lane A armed at `height` over its genesis bonds, beside lane F1 at the
/// same height — the value an operator's post-launch build sets (one common height).
fn armed_at(height: u64) -> Params {
    let mut p = f1_at(height);
    p.palw_operator_anchor = p.palw_operator_anchor_of_genesis_bonds_v1(ForkActivation::new(height));
    assert!(p.palw_operator_anchor.is_some(), "testnet-12 is ConsensusV2");
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

/// **Dormant everywhere as shipped**, and testnet-12's ids are the release's.
#[test]
fn the_fence_is_dormant_on_every_shipped_preset_and_testnet12_is_the_release() {
    for (name, p) in presets() {
        assert_eq!(p.palw_operator_anchor, None, "{name}: lane A's fence ships dormant");
        assert!(!p.palw_operator_anchor_active_at(0) && !p.palw_operator_anchor_active_at(u64::MAX - 1), "{name}");
        assert!(p.palw_operator_anchor_rule_v1().is_none(), "{name}: no rule reaches the processor");
        assert!(
            p.palw_fences_v1().iter().any(|(n, fence)| *n == "palw_operator_anchor" && fence.is_none()),
            "{name}: the fence is on the list fork_id_v1 and the schedule walk read"
        );
    }
    let t12 = palw_t12_shipped_params();
    t12.validate_palw_v2().expect("testnet-12 as shipped validates");
    let now = ids(&t12);
    println!("testnet-12 on this build: {now:?}");
    assert_eq!((now.0.as_str(), now.1.as_str(), now.2.as_str()), T12_RELEASE, "testnet-12 is the release's ruleset, to the id");
}

/// **testnet-12's operators are its eight genesis cards**: the premine's collateral outputs `0..7`
/// (txid `5e0d5f1b…`), each with the key its card registered — so the rule trusts exactly the fleet
/// (b0/b1 ibm, b6 .113, b2–b5/b7 5.104) and nobody the chain registers later.
#[test]
fn testnet12_s_operators_are_its_eight_genesis_cards() {
    let armed = armed_at(1_234);
    armed.validate_palw_v2().expect("armed over the genesis bonds is a runnable ruleset");
    let value = armed.palw_operator_anchor.clone().expect("armed");
    let expected: Vec<PalwBondKeyV2> = {
        let mut v: Vec<_> =
            PALW_T12_GENESIS_BONDS.iter().map(|card| PalwBondKeyV2(premine_outpoint_for(armed.net, card.premine_index))).collect();
        v.sort();
        v
    };
    assert_eq!(value.operators, expected, "the eight premine collateral outpoints, sorted");
    assert_eq!(value.operators.len(), 8);
    let txid = premine_outpoint_for(armed.net, 0).transaction_id.to_string();
    println!("testnet-12 operator bonds: premine {txid} outputs {:?}", value.operators.iter().map(|b| b.0.index).collect::<Vec<_>>());
    assert!(txid.starts_with("5e0d5f1b"), "testnet-12's premine txid: {txid}");
    let rule = armed.palw_operator_anchor_rule_v1().expect("the processor's rule");
    let keys: Vec<(PalwBondKeyV2, Vec<u8>)> = rule.operators().map(|(b, k)| (*b, k.to_vec())).collect();
    for card in PALW_T12_GENESIS_BONDS {
        let bond = PalwBondKeyV2(premine_outpoint_for(armed.net, card.premine_index));
        assert!(keys.contains(&(bond, card.bond_pubkey.to_vec())), "card {}: its outpoint under its registered key", card.premine_index);
    }
    assert!(rule.active_at(1_234) && !rule.active_at(1_233), "keyed on the anchor's DAA");
}

/// **Armed at a future height: the ruleset and the schedule name it, the identity does not** — beside
/// lane F1 at the same height, and over F1 alone (lane A is its own entry in both ids). The params id
/// names the operator list too. A `Some(never())` is absence; armed at genesis it is a rule in force
/// from block one.
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
        let (f1_p, f1_i, f1_s) = ids(&f1_at(height));
        assert_ne!(p, f1_p, "armed at {height}: lane A is its own entry in the params id, not F1's");
        assert_ne!(s, f1_s, "armed at {height}: and in the schedule id");
        assert_eq!(f1_i, identity_id);
        assert_eq!(i, identity_id, "armed at {height}: a height not yet reached is not yet a rule — the two builds peer");
        assert!(seen.insert(p.clone()), "armed at {height}: the height is in the params id");
        assert!(armed.palw_operator_anchor_active_at(height) && !armed.palw_operator_anchor_active_at(height - 1));
    }
    // The operator list is in the params id: seven operators at the same height is another ruleset.
    let mut fewer = armed_at(1_234);
    fewer.palw_operator_anchor.as_mut().unwrap().operators.pop();
    fewer.validate_palw_v2().expect("a subset of the genesis bonds is legal");
    assert_ne!(ids(&fewer).0, ids(&armed_at(1_234)).0, "the params id names who is trusted");
    assert_eq!(ids(&fewer).2, ids(&armed_at(1_234)).2, "the schedule names only the height");
    // Some(never()) — the normalised form of a scheduled fence — is absence in the identity (and in the
    // params and schedule ids too: the list goes with the height).
    let mut never = shipped.clone();
    never.palw_operator_anchor = shipped.palw_operator_anchor_of_genesis_bonds_v1(ForkActivation::never());
    assert_eq!(never.consensus_identity_id().to_string(), identity_id, "Some(never()) is absence in the identity");
    never.validate_palw_v2().expect("never() is dormant");
    assert!(never.palw_operator_anchor_rule_v1().is_none(), "never() reaches no processor");
    // At genesis the rule is in force from block one: another identity (a regenesis, not this lane).
    let genesis = armed_at(0);
    genesis.validate_palw_v2().expect("a network may be born with the rule");
    assert_ne!(genesis.consensus_identity_id().to_string(), identity_id, "in force at genesis separates identities");
}

/// **The fork id sees the height** — lane A armed beside lane F1 at ONE common post-launch height (the
/// rollout), and lane A one height above an F1 armed earlier (both on the gate): below a height an
/// armed build and the shipped build keep each other in both directions; from it the armed build
/// refuses the shipped one.
#[test]
fn an_armed_build_below_its_fence_handshakes_with_the_shipped_build() {
    let shipped = palw_t12_shipped_params();
    let shipped_gate = fork_id_gate_fences_v1(&shipped);
    for height in HEIGHTS {
        let common = armed_at(height);
        let mut later = f1_at(height - 1);
        later.palw_operator_anchor = later.palw_operator_anchor_of_genesis_bonds_v1(ForkActivation::new(height));
        later.validate_palw_v2().expect("lane A above lane F1 is legal");
        assert!(fork_id_gate_fences_v1(&later).contains(&(height - 1)), "F1's own height is on the gate");
        // (what, the build, its first new height)
        for (what, armed, first) in [("lanes F1 + A at one height", &common, height), ("lane A above F1", &later, height - 1)] {
            let gate = fork_id_gate_fences_v1(armed);
            assert!(gate.contains(&height), "{what} at {height}: lane A's height is on the fork-id gate ({gate:?})");
            assert!(!shipped_gate.contains(&height), "{what} at {height}: an INDEPENDENT height, not one the release schedules");
            for daa in [0, 1, first / 2, first - 1] {
                let (a, s) = (fork_id_v1(armed, daa), fork_id_v1(&shipped, daa));
                assert!(!evaluate_fork_id_v1(armed, daa, s.fired.as_bytes().as_slice(), s.next).refuses(), "{what}: armed keeps shipped at {daa}");
                assert!(!evaluate_fork_id_v1(&shipped, daa, a.fired.as_bytes().as_slice(), a.next).refuses(), "{what}: shipped keeps armed at {daa}");
            }
            let s = fork_id_v1(&shipped, first);
            let past = evaluate_fork_id_v1(armed, first, s.fired.as_bytes().as_slice(), s.next);
            println!("{what} armed at {height}: at {first} the armed node says {past:?} to a shipped peer");
            assert!(past.refuses(), "{what} at {height}: from {first} the armed node refuses a node that did not upgrade");
        }
        // A build that armed F1 alone at `height - 1` and one that also armed lane A at `height` part at
        // lane A's height, by name: the second fence is its own flag day.
        let f1_only = f1_at(height - 1);
        for daa in [0, height - 1] {
            let o = fork_id_v1(&f1_only, daa);
            assert!(!evaluate_fork_id_v1(&later, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "below lane A's height the two keep each other");
        }
        let o = fork_id_v1(&f1_only, height);
        assert!(evaluate_fork_id_v1(&later, height, o.fired.as_bytes().as_slice(), o.next).refuses(), "from lane A's height the F1-only build is refused");
    }
}

/// **What `validate_palw_v2` refuses**, each by name: no R-core+, off ConsensusV2, no lane F1 at or
/// below it, and an operator list that is empty, unsorted, duplicated or names a non-genesis bond.
#[test]
fn the_fence_needs_rcore_plus_and_a_genesis_operator_list() {
    // No R-core+ (testnet-11, devnet): refused by name.
    for (name, mut p) in [("testnet-11", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())] {
        p.palw_operator_anchor = p.palw_operator_anchor_of_genesis_bonds_v1(ForkActivation::new(1_234));
        assert!(p.palw_operator_anchor.is_some(), "{name} is ConsensusV2");
        let refused = p.validate_palw_v2();
        println!("{name} armed: {refused:?}");
        assert!(
            refused.is_err_and(|e| format!("{e:?}").contains("palw_operator_anchor is armed without palw_rcore_plus")),
            "{name}: no R-core+, no fence — refused by name"
        );
    }
    // Off ConsensusV2 (testnet-10 and a hash-only mainnet): refused by name.
    for (name, mut p) in [("testnet-10", Params::from(TESTNET_PARAMS.net)), ("mainnet", mainnet_shipped_params())] {
        p.palw_operator_anchor = Some(PalwOperatorAnchorV1 { activation: ForkActivation::new(1_234), operators: vec![] });
        let refused = p.validate_palw_v2();
        assert!(refused.is_err_and(|e| format!("{e:?}").contains("not ConsensusV2")), "{name}: refused by name");
    }
    // Without lane F1 at or below it (absent, or above it): refused by name.
    for f1 in [None, Some(1_235u64)] {
        let mut p = palw_t12_shipped_params();
        p.palw_panel_seed_execution = f1.map(ForkActivation::new);
        p.palw_operator_anchor = p.palw_operator_anchor_of_genesis_bonds_v1(ForkActivation::new(1_234));
        let refused = p.validate_palw_v2();
        println!("testnet-12, lane A at 1,234 with F1 at {f1:?}: {refused:?}");
        assert!(
            refused.is_err_and(|e| format!("{e:?}").contains("without palw_panel_seed_execution (lane F1) at or below it")),
            "F1 {f1:?}: refused by name"
        );
    }
    // The value on testnet-12: empty, unsorted, a stranger.
    let base = armed_at(1_234);
    let genesis_list = base.palw_operator_anchor.clone().unwrap().operators;
    let refuse = |operators: Vec<PalwBondKeyV2>, needle: &str| {
        let mut p = f1_at(1_234);
        p.palw_operator_anchor = Some(PalwOperatorAnchorV1 { activation: ForkActivation::new(1_234), operators });
        let refused = p.validate_palw_v2();
        println!("testnet-12 with a bad operator list: {refused:?}");
        assert!(refused.is_err_and(|e| format!("{e:?}").contains(needle)), "refused naming {needle:?}");
    };
    refuse(vec![], "names no operator bond");
    refuse(genesis_list.iter().rev().copied().collect(), "sorted and distinct");
    refuse(vec![genesis_list[0], genesis_list[0]], "sorted and distinct");
    let mut stranger = genesis_list.clone();
    stranger.push(PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(u64::MAX),
        index: 0,
    }));
    stranger.sort();
    refuse(stranger, "does not register");
    // Dormant (None or never) is legal anywhere; over testnet-12's genesis R-core+ any height is.
    for (name, mut p) in presets() {
        if let Some(never) = p.palw_operator_anchor_of_genesis_bonds_v1(ForkActivation::never()) {
            p.palw_operator_anchor = Some(never);
            if name == "testnet-12" {
                p.validate_palw_v2().expect("never() is dormant");
            }
        }
    }
    for height in HEIGHTS {
        armed_at(height).validate_palw_v2().expect("over testnet-12's genesis R-core+, any height is legal");
    }
}
