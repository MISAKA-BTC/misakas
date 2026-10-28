//! **RFC-0024 A / ADR-0191 (2): the TIR dissection's terminal tile narrowed to ONE history position**
//! (G14 lane court-archive-terminal; DORMANT behind `palw_tir_bounded_door_v1`).
//!
//! * Honest: on every dissected leaf of the corpus, every terminal tile's position round folds to the
//!   tile claim, and the one-position bottom acquits at the first, middle (when the tile has one) and
//!   last position; its carriage reads no more than the whole-tile bottom's.
//! * Forged: a responder that keeps the position round folding by moving a lie between two
//!   positions of a `Σ` reduction is named at that position (first / middle / last) and convicted
//!   there by the one-position evaluation.
//! * Below the fence the selector keeps the whole-tile terminal, which is unchanged.

#![allow(warnings)]
#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    build_tir_dissect_bottom_v1, build_tir_dissect_position_bottom_v1, build_tir_dissect_position_round_v1,
    build_tir_dissect_round_v1, build_tir_root_claim_v1, check_tir_dissect_bottom_v1, check_tir_dissect_position_bottom_v1,
    check_tir_root_claim_v1, tir_dissect_terminal_narrows_to_position_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectSiteV1, PalwTirFoldV1,
    palw_tir_dissect_site_v1,
};

const SESSION: Hash64 = Hash64::from_bytes([0x5F; 64]);
const W: u64 = 10;

fn dissected(f: &Fixture) -> Vec<(u64, PalwTirDissectSiteV1)> {
    let intervals = f.intervals.as_ref().expect("admissible");
    f.leaves
        .iter()
        .enumerate()
        .filter_map(|(i, leaf)| palw_tir_dissect_site_v1(&f.space, intervals, leaf).map(|s| (i as u64, s)))
        .collect()
}

fn choice(round: u32, child: u8) -> PalwTirDissectChoiceV1 {
    PalwTirDissectChoiceV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, session_id: SESSION, round, child }
}

/// Every terminal tile phase of an honest dissection of `leaf` (one per tile the challenger can reach
/// by naming the first or the last child at each round).
fn terminal_phases(f: &Fixture, x: &Execution, leaf: u64, site: &PalwTirDissectSiteV1) -> Vec<PalwTirDissectPhaseV1> {
    let store = Store { f, x };
    let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("honest root");
    check_tir_root_claim_v1(&root, leaf, &RULES).expect("admitted");
    let mut out = Vec::new();
    for last in [false, true] {
        let mut phase = PalwTirDissectPhaseV1::open(SESSION, leaf, site, &root, 2, 0, W).expect("opens");
        let mut t = 1;
        while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
            let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("a round");
            phase.apply_round(&round, t, W).expect("honest round folds");
            let pick = if last { phase.child_ranges().len() as u8 - 1 } else { 0 };
            phase.apply_choice(&choice(phase.round(), pick), t + 1, W).expect("a legal choice");
            t += 2;
        }
        assert_eq!(phase.turn(), PalwBisectTurnV1::Terminal);
        if !out.contains(&phase) {
            out.push(phase);
        }
    }
    out
}

/// First, middle (if distinct) and last position index inside a tile of `width` positions.
fn picks(width: usize) -> Vec<usize> {
    let mut v = vec![0, width / 2, width - 1];
    v.dedup();
    v
}

#[test]
fn an_honest_terminal_narrows_to_each_position_and_acquits() {
    let mut played = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f) {
            for phase in terminal_phases(&f, &x, leaf, &site) {
                // The whole-tile terminal (below the fence) is unchanged.
                let whole = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("the tile bottom");
                assert_eq!(check_tir_dissect_bottom_v1(&phase, &whole, leaf, &RULES), Err(PalwStepRefuteError::NoFaultFound));
                let narrowing = phase.position_narrowing_v1().expect("terminal");
                let (from, to) = narrowing.tile_range();
                assert_eq!(Some((from, to)), phase.terminal_range());
                let width = to - from;
                let round = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).expect("position round");
                assert_eq!(round.children.len(), width);
                if width == 1 {
                    assert!(!narrowing.awaits_position_round(), "a one-position tile needs no extra round");
                    assert_eq!(narrowing.terminal_position_range(), Some((from, from + 1)));
                    assert!(narrowing.check_position_round_v1(&round).is_err());
                }
                for k in if width == 1 { vec![] } else { picks(width) } {
                    narrowing.check_position_round_v1(&round).expect("honest positions fold to the tile claim");
                    let named = narrowing.choose_position_v1(&round, k as u8).expect("a legal position");
                    assert_eq!(named.terminal_position_range(), Some((from + k, from + k + 1)));
                    assert_eq!(named.claim(), &round.children[k]);
                    let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).expect("position bottom");
                    assert_eq!(
                        check_tir_dissect_position_bottom_v1(&phase, &named, &bottom, leaf, &RULES),
                        Err(PalwStepRefuteError::NoFaultFound),
                        "{} leaf {leaf} position {}",
                        f.name,
                        from + k
                    );
                    // The single-position terminal reads only that position's set: never more bytes
                    // than the whole tile's carriage.
                    assert!(borsh::to_vec(&bottom).unwrap().len() <= borsh::to_vec(&whole).unwrap().len());
                    // An un-narrowed tile is not a position terminal.
                    assert!(check_tir_dissect_position_bottom_v1(&phase, &narrowing, &bottom, leaf, &RULES).is_err());
                    played += 1;
                }
            }
        }
    }
    assert!(played >= 4, "{played} position terminals played");
}

#[test]
fn a_lie_moved_between_positions_is_named_and_convicted_at_first_middle_and_last() {
    let mut convicted = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f).into_iter().filter(|(leaf, _)| leaf % 2 == 0) {
            let Some(k_sum) = site.folds.iter().position(|fold| *fold == PalwTirFoldV1::Sum) else { continue };
            for phase in terminal_phases(&f, &x, leaf, &site) {
                let narrowing = phase.position_narrowing_v1().expect("terminal");
                let (from, to) = narrowing.tile_range();
                let width = to - from;
                if width < 2 || phase.elements()[k_sum].is_empty() {
                    continue;
                }
                let honest = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).expect("position round");
                for k in picks(width) {
                    // Raise position k by one, lower another position by one: the Σ fold to the tile's
                    // (honest) claim still holds, so the round is admitted.
                    let other = if k == 0 { width - 1 } else { 0 };
                    let mut lie = honest.clone();
                    lie.children[k].partials[k_sum][0] += 1;
                    lie.children[other].partials[k_sum][0] -= 1;
                    if narrowing.check_position_round_v1(&lie).is_err() {
                        continue; // outside a proven interval: refused before any bottom
                    }
                    // A tampered fold is refused by name.
                    let mut broken = honest.clone();
                    broken.children[k].partials[k_sum][0] += 1;
                    assert!(narrowing.check_position_round_v1(&broken).is_err());
                    let named = narrowing.choose_position_v1(&lie, k as u8).expect("the challenger names the lie");
                    let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).expect("position bottom");
                    let verdict = check_tir_dissect_position_bottom_v1(&phase, &named, &bottom, leaf, &RULES);
                    assert!(verdict.is_ok(), "{} leaf {leaf} position {}: {verdict:?}", f.name, from + k);
                    // The honest neighbour the challenger did not name still acquits.
                    let fair = narrowing.choose_position_v1(&honest, k as u8).unwrap();
                    let fair_bottom = build_tir_dissect_position_bottom_v1(&x.binding, &fair, &store, &RULES).unwrap();
                    assert_eq!(
                        check_tir_dissect_position_bottom_v1(&phase, &fair, &fair_bottom, leaf, &RULES),
                        Err(PalwStepRefuteError::NoFaultFound)
                    );
                    convicted += 1;
                }
            }
        }
    }
    assert!(convicted >= 2, "{convicted} position lies convicted");
}

#[test]
fn below_the_door_fence_the_terminal_stays_the_whole_tile() {
    assert!(!tir_dissect_terminal_narrows_to_position_v1(None, 0));
    assert!(!tir_dissect_terminal_narrows_to_position_v1(None, u64::MAX - 1));
    assert!(!tir_dissect_terminal_narrows_to_position_v1(Some(ForkActivation::never()), u64::MAX - 1));
    let fence = Some(ForkActivation::new(16_000));
    assert!(!tir_dissect_terminal_narrows_to_position_v1(fence, 15_999));
    assert!(tir_dissect_terminal_narrows_to_position_v1(fence, 16_000));
    // A phase that is not yet terminal has no position narrowing.
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        if let Some((leaf, site)) = dissected(&f).into_iter().next() {
            let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("root");
            let phase = PalwTirDissectPhaseV1::open(SESSION, leaf, &site, &root, 2, 0, W).expect("opens");
            assert_eq!(phase.position_narrowing_v1().is_some(), phase.turn() == PalwBisectTurnV1::Terminal);
        }
    }
}
