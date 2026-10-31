//! **RFC-0024 A / §5 (G14 item 7, P2; lane r6-p2-shared): the position round ON CHAIN, its per-turn deadline, the whole-tile
//! close retired for new claims, and the lookahead whose last level ends on one history position.** DORMANT behind
//! `palw_tir_bounded_door_v1`. Run: `cargo test -p kaspa-consensus-core --test palw_tir_position_round_onchain_v2`.
//!
//! * A NEW claim's terminal tile (wider than one position) owes the responder's position round: the phase reads
//!   `AwaitDisclosure` on one tile, so the existing rung clock runs and silence is the RESPONDER's no-show (fraud cannot
//!   escape to the challenger-side backstop). No tile round, lookahead or whole-tile close is admissible there.
//! * The position round installs `Terminal` with the positions pending and the challenger's per-turn deadline
//!   (`accepted + w_round`); the position close must name a position of THAT round; the whole-tile close stays retired.
//! * A claim admitted before the fence (T15) keeps the whole-tile close: its phase is never put on the position round.
//! * A version-3 lookahead whose level-1 child is one tile carries that tile's position round: one disclosure + one path
//!   reach exactly the phase the old round + choice + position round reach (B-01), and tampers are refused (B-03).

#![allow(warnings)]
#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::{PalwBisectPartyV1, PalwBisectTurnV1};
use kaspa_consensus_core::palw_state_v2::palw_tir_position_terminal_applies_with_v2;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    build_tir_dissect_bottom_v1, build_tir_dissect_position_bottom_v1, build_tir_dissect_position_round_v1,
    build_tir_dissect_round_v1, build_tir_root_claim_v1, check_tir_dissect_bottom_v1, check_tir_dissect_position_bottom_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, PALW_TIR_LOOKAHEAD_POSITION_VERSION_V2,
    PalwTirChoicePathV2, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1,
    PalwTirLookaheadDisclosureV2, palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_position_terminal_v2::{
    PALW_TIR_POSITION_OBJECT_VERSION_V2, PalwTirPositionChoiceV2, check_tir_position_dissection_v2,
    palw_tir_position_dissection_digest_v2, palw_tir_position_dissection_for_v2, palw_tir_position_named_v2,
};

const SESSION: Hash64 = Hash64::from_bytes([0x6F; 64]);
const W: u64 = 10;
const CAP: Option<usize> = Some(usize::MAX);

fn dissected(f: &Fixture, tile_positions: u32) -> Vec<(u64, PalwTirDissectSiteV1)> {
    let intervals = f.intervals.as_ref().expect("admissible");
    f.leaves
        .iter()
        .enumerate()
        .filter_map(|(i, leaf)| palw_tir_dissect_site_v1(&f.space, intervals, leaf).map(|s| (i as u64, s)))
        .map(|(i, mut s)| {
            s.tile_positions = tile_positions;
            (i, s)
        })
        .collect()
}

fn choice(round: u32, child: u8) -> PalwTirDissectChoiceV1 {
    PalwTirDissectChoiceV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, session_id: SESSION, round, child }
}

fn opened(x: &Execution, store: &Store<'_>, leaf: u64, site: &PalwTirDissectSiteV1) -> PalwTirDissectPhaseV1 {
    let root = build_tir_root_claim_v1(&x.binding, leaf, store, &RULES).expect("honest root");
    PalwTirDissectPhaseV1::open(SESSION, leaf, site, &root, 2, 0, W).expect("opens")
}

/// Terminal tile phases reached by naming the first / the last child at every round (the move that made it terminal is at
/// DAA `t`, so its deadline is `t + W`).
fn terminal_phases(x: &Execution, store: &Store<'_>, leaf: u64, site: &PalwTirDissectSiteV1) -> Vec<PalwTirDissectPhaseV1> {
    let mut out = Vec::new();
    for last in [false, true] {
        let mut phase = opened(x, store, leaf, site);
        let mut t = 1;
        while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
            let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, store, &RULES).expect("a round");
            phase.apply_round(&round, t, W).expect("honest round folds");
            let pick = if last { phase.child_ranges().len() as u8 - 1 } else { 0 };
            phase.apply_choice(&choice(phase.round(), pick), t + 1, W).expect("a legal choice");
            t += 2;
        }
        if phase.turn() == PalwBisectTurnV1::Terminal && !out.contains(&phase) {
            out.push(phase);
        }
    }
    out
}

#[test]
fn the_position_terminal_applies_only_past_the_fence_to_claims_admitted_past_it_and_with_a_budget() {
    // (door active, door DAA, claim accepted DAA, adopted budget) -> applies
    assert!(!palw_tir_position_terminal_applies_with_v2(false, Some(10), 20, Some(1)), "below the fence");
    assert!(!palw_tir_position_terminal_applies_with_v2(true, None, 20, Some(1)), "no fence DAA");
    assert!(!palw_tir_position_terminal_applies_with_v2(true, Some(10), 9, Some(1)), "T15: admitted before the fence");
    assert!(palw_tir_position_terminal_applies_with_v2(true, Some(10), 10, Some(1)), "admitted at the fence");
    assert!(palw_tir_position_terminal_applies_with_v2(true, Some(10), 11, Some(1)));
    assert!(
        !palw_tir_position_terminal_applies_with_v2(true, Some(10), 11, None),
        "no adopted witness bound: the whole-tile close is NOT retired (no position close could end the session)"
    );
}

#[test]
fn a_new_claims_wide_terminal_owes_the_position_round_and_its_silence_is_the_responders() {
    let mut owed = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, 4) {
            for terminal in terminal_phases(&x, &store, leaf, &site) {
                let (from, to) = terminal.terminal_range().expect("terminal");
                let mut phase = terminal.clone();
                if to - from == 1 {
                    assert!(!phase.owe_position_round_v2(), "a one-position tile needs no round");
                    assert_eq!(phase, terminal);
                    continue;
                }
                let deadline = terminal.last_deadline_daa();
                assert!(phase.owe_position_round_v2());
                assert!(!phase.owe_position_round_v2(), "idempotent");
                assert!(phase.awaits_position_round_v2());
                assert_eq!(phase.turn(), PalwBisectTurnV1::AwaitDisclosure);
                assert_eq!(phase.last_deadline_daa(), deadline, "the per-turn deadline the terminal move set");
                // No tile range while owed: no whole-tile close, no tile round, no lookahead.
                assert_eq!(phase.terminal_range(), None);
                assert_eq!(phase.whole_tile_terminal_range_v2(), None);
                let whole = build_tir_dissect_bottom_v1(&x.binding, &terminal, &store, &RULES).expect("whole-tile bottom");
                assert!(!matches!(
                    check_tir_dissect_bottom_v1(&phase, &whole, leaf, &RULES),
                    Err(PalwStepRefuteError::NoFaultFound) | Ok(_)
                ));
                let any = PalwTirDissectRoundV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, children: vec![] };
                assert!(phase.clone().apply_round(&any, deadline, W).is_err(), "no tile round at one tile");
                let la = PalwTirLookaheadDisclosureV2 { version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, frontier: vec![] };
                assert!(phase.expand_lookahead_v2(&la, CAP).is_err());

                // Silence past the deadline is the RESPONDER's (the sweep's existing rung rule reads `AwaitDisclosure`).
                assert!(phase.clone().declare_no_show(deadline).is_err(), "not before the deadline");
                let no_show = phase.clone().declare_no_show(deadline + 1).expect("the responder owed a move");
                assert_eq!(no_show.silent_party, PalwBisectPartyV1::Responder);

                // The position round (tag 201's fold): `Terminal`, positions pending, the challenger's per-turn deadline.
                let narrowing = phase.position_narrowing_v2().expect("owed narrowing");
                assert_eq!(narrowing, terminal.position_narrowing_v1().unwrap());
                let honest = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).expect("position round");
                let d = palw_tir_position_dissection_for_v2(&terminal, honest.clone()).expect("terminal");
                check_tir_position_dissection_v2(&phase, &d).expect("an owed round binds (acceptance of tag 201)");
                let mut wrong = honest.clone();
                wrong.children.pop();
                assert!(phase.clone().apply_position_round_v2(&wrong, 50, W).is_err(), "a short position round");
                assert!(phase.clone().apply_position_round_v2(&honest, 50, 0).is_err(), "a zero window");
                assert!(terminal.clone().apply_position_round_v2(&honest, 50, W).is_err(), "nothing owed on an old claim's terminal");
                phase.apply_position_round_v2(&honest, 50, W).expect("the owed round");
                assert_eq!(phase.turn(), PalwBisectTurnV1::Terminal);
                assert_eq!(phase.last_deadline_daa(), 50 + W);
                assert_eq!(phase.disclosed_position_round_v2(), Some(&honest.children[..]));
                assert!(phase.clone().apply_position_round_v2(&honest, 51, W).is_err(), "once only");
                // Challenger's turn at `Terminal`: no rung charge (the backstop's rule), and the whole-tile close is retired.
                assert!(phase.clone().declare_no_show(50 + W + 1).is_err());
                assert_eq!(phase.whole_tile_terminal_range_v2(), None);
                assert_eq!(phase.terminal_range(), Some((from, to)));
                assert!(!matches!(
                    check_tir_dissect_bottom_v1(&phase, &whole, leaf, &RULES),
                    Err(PalwStepRefuteError::NoFaultFound) | Ok(_)
                ));

                // The position close names a position of the round ON CHAIN, and only of it.
                let c = PalwTirPositionChoiceV2 {
                    version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    tile_round: phase.round(),
                    dissection_digest: palw_tir_position_dissection_digest_v2(&d),
                    position_child: ((to - from) - 1) as u8,
                };
                let named = palw_tir_position_named_v2(&phase, &d, &c).expect("names the last position");
                let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).expect("position bottom");
                assert_eq!(
                    check_tir_dissect_position_bottom_v1(&phase, &named, &bottom, leaf, &RULES),
                    Err(PalwStepRefuteError::NoFaultFound)
                );
                // Another (still folding) round than the disclosed one is refused.
                let mut other = honest.clone();
                if other.children.len() >= 2 {
                    let (a, b) = (other.children[0].clone(), other.children[1].clone());
                    other.children[0] = b;
                    other.children[1] = a;
                    if other != honest {
                        let d2 = palw_tir_position_dissection_for_v2(&terminal, other).unwrap();
                        assert!(check_tir_position_dissection_v2(&phase, &d2).is_err(), "not the round on chain");
                    }
                }
                owed += 1;
            }
        }
    }
    assert!(owed >= 1, "{owed} owed position rounds exercised");
}

#[test]
fn a_claim_admitted_before_the_fence_keeps_the_whole_tile_close() {
    let mut kept = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, 4) {
            for phase in terminal_phases(&x, &store, leaf, &site) {
                // The fold never calls `owe_position_round_v2` for it: the phase is exactly as before the door.
                assert_eq!(phase.whole_tile_terminal_range_v2(), phase.terminal_range());
                assert_eq!(phase.disclosed_position_round_v2(), None);
                let whole = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("whole-tile bottom");
                assert_eq!(check_tir_dissect_bottom_v1(&phase, &whole, leaf, &RULES), Err(PalwStepRefuteError::NoFaultFound));
                // Silence at its terminal stays nobody's (the backstop ends it), as before.
                assert!(phase.clone().declare_no_show(phase.last_deadline_daa() + 1).is_err());
                kept += 1;
            }
        }
    }
    assert!(kept >= 1);
}

#[test]
fn a_lookahead_ending_on_positions_equals_the_tile_choice_plus_the_position_round() {
    let mut played = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, 4) {
            // Descend (first child) until the disputed range's children are single tiles.
            let mut phase = opened(&x, &store, leaf, &site);
            let mut t = 1;
            while phase.turn() == PalwBisectTurnV1::AwaitDisclosure && phase.child_ranges().iter().any(|&(_, n)| n > 1) {
                let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).unwrap();
                phase.apply_round(&round, t, W).unwrap();
                phase.apply_choice(&choice(phase.round(), 0), t + 1, W).unwrap();
                t += 2;
            }
            if phase.turn() != PalwBisectTurnV1::AwaitDisclosure {
                continue;
            }
            let level1 = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).unwrap();
            let mut frontier = Vec::new();
            let mut position_rounds = Vec::new();
            for (i, &(first, _)) in phase.child_ranges().iter().enumerate() {
                let child = phase.project_unrooted_child_v1(&level1, i as u8).unwrap();
                if phase.tile_width_v2(first) > 1 {
                    let narrowing = child.position_narrowing_v1().unwrap();
                    let round = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).unwrap();
                    frontier.push(round.children.clone());
                    position_rounds.push(Some(round));
                } else {
                    frontier.push(vec![level1.children[i].clone()]);
                    position_rounds.push(None);
                }
            }
            let msg = PalwTirLookaheadDisclosureV2 { version: PALW_TIR_LOOKAHEAD_POSITION_VERSION_V2, frontier };
            let expansion = phase.expand_lookahead_v2(&msg, CAP).expect("the honest position frontier");
            assert_eq!(expansion.level1, level1, "the tile claims are reconstructed exactly from the positions");
            assert_eq!(expansion.positions, position_rounds);
            assert!(phase.expand_lookahead_v2(&msg, None).is_err(), "typed-missing frontier bound");

            for (i, position_round) in position_rounds.iter().enumerate() {
                let daa = t + 10;
                // Old path: tile round, choice, then the position round (tag 201).
                let mut expected = phase.clone();
                expected.apply_round(&level1, daa, W).unwrap();
                expected.apply_choice(&choice(expected.round(), i as u8), daa, W).unwrap();
                if let Some(round) = position_round {
                    expected.owe_position_round_v2();
                    expected.apply_position_round_v2(round, daa, W).unwrap();
                }
                // New path: one disclosure, one choice path.
                let mut new = phase.clone();
                new.apply_lookahead_disclosure_v2(&msg, CAP, daa, W).unwrap();
                let p = PalwTirChoicePathV2 {
                    version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    round: phase.round(),
                    path: vec![i as u8],
                };
                new.apply_choice_path_v2(&msg, &p, CAP, daa, W).unwrap();
                assert_eq!(
                    new, expected,
                    "{} leaf {leaf} child {i}: lookahead to positions ≡ round + choice + position round",
                    f.name
                );
                if position_round.is_some() {
                    assert!(new.disclosed_position_round_v2().is_some());
                    assert_eq!(new.whole_tile_terminal_range_v2(), None, "the whole-tile close is retired");
                    assert!(!new.clone().owe_position_round_v2(), "nothing more is owed");
                    played += 1;
                }
                // A path naming a position too is refused (the position is named by the close, under its own signature).
                let mut extra = phase.clone();
                extra.apply_lookahead_disclosure_v2(&msg, CAP, daa, W).unwrap();
                let p2 = PalwTirChoicePathV2 { path: vec![i as u8, 0], ..p.clone() };
                assert!(extra.apply_choice_path_v2(&msg, &p2, CAP, daa, W).is_err());
            }

            // B-03 on the position level: a missing, duplicated or altered position is refused.
            if let Some(i) = position_rounds.iter().position(Option::is_some) {
                let mut missing = msg.clone();
                missing.frontier[i].pop();
                assert!(phase.expand_lookahead_v2(&missing, CAP).is_err());
                let mut dup = msg.clone();
                let first = dup.frontier[i][0].clone();
                dup.frontier[i].push(first);
                assert!(phase.expand_lookahead_v2(&dup, CAP).is_err());
                let mut altered = msg.clone();
                if let Some(v) = altered.frontier[i][0].partials.iter_mut().find(|r| !r.is_empty()) {
                    v[0] = i128::MAX;
                    assert!(phase.expand_lookahead_v2(&altered, CAP).is_err(), "an altered position is refused (interval / fold)");
                }
                // The same frontier under version 2 is a different bundle (one claim per single-tile child): refused.
                let as_v2 = PalwTirLookaheadDisclosureV2 { version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, ..msg.clone() };
                assert!(phase.expand_lookahead_v2(&as_v2, CAP).is_err());
            }
        }
    }
    assert!(played >= 1, "{played} lookaheads ending on positions compared");
}
