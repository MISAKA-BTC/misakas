//! **RFC-0024 §4 / §5 (G14 item 7, P2; lane r5-p2-terminal): the versioned position terminal and the two-level lookahead.**
//! DORMANT behind `palw_tir_bounded_door_v1`. Run: `cargo test -p kaspa-consensus-core --test palw_tir_position_terminal_v2`.
//!
//! * Middle position: with tiles of 3+ positions (the site's `tile_positions` raised to 4), the challenger names a position
//!   strictly between the first and the last; an honest middle acquits and a lie moved into the middle is convicted there, and
//!   the signed-object path (`PositionDissectionV2` + choice) names exactly the same narrowing.
//! * The position move's bindings (session, round, tile range, root/tile claim digests, choice digest) refuse every tamper.
//! * Lookahead (B-01): a depth-2 disclosure + one `ChoicePathV2` reach exactly the phase the old two rounds and two choices
//!   reach; B-03: a missing, duplicated or altered candidate is refused; the frontier bound is typed-missing (refused at `None`).
//! * The position witness budget is typed-missing: `None` refuses as `PositionWitnessBudgetIncomplete`, never a verdict.

#![allow(warnings)]
#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    build_tir_dissect_position_bottom_v1, build_tir_dissect_position_round_v1, build_tir_dissect_round_v1, build_tir_root_claim_v1,
    check_tir_dissect_position_bottom_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, PalwTirChoicePathV2, PalwTirDissectChoiceV1,
    PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1, PalwTirFoldV1, PalwTirLookaheadDisclosureV2,
    palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_position_terminal_v2::{
    PALW_TIR_POSITION_OBJECT_VERSION_V2, PALW_TIR_POSITION_WITNESS_BUDGET_V2, PalwTirPositionChoiceV2, PalwTirPositionCloseV2,
    PalwTirPositionDissectionV2, PalwTirPositionWitnessErrorV2, check_tir_position_dissection_v2,
    check_tir_position_witness_budget_v2, palw_tir_position_dissection_digest_v2, palw_tir_position_dissection_for_v2,
    palw_tir_position_named_v2,
};

const SESSION: Hash64 = Hash64::from_bytes([0x6E; 64]);
const W: u64 = 10;

fn dissected(f: &Fixture, tile_positions: Option<u32>) -> Vec<(u64, PalwTirDissectSiteV1)> {
    let intervals = f.intervals.as_ref().expect("admissible");
    f.leaves
        .iter()
        .enumerate()
        .filter_map(|(i, leaf)| palw_tir_dissect_site_v1(&f.space, intervals, leaf).map(|s| (i as u64, s)))
        .map(|(i, mut s)| {
            if let Some(t) = tile_positions {
                s.tile_positions = t;
            }
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

/// Terminal tile phases reached by naming the first / the last child at every round.
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
fn a_middle_position_of_a_wide_tile_is_named_acquitted_honestly_and_convicted_when_it_lies() {
    let (mut honest_middles, mut convicted_middles) = (0, 0);
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, Some(4)) {
            let k_sum = site.folds.iter().position(|fold| *fold == PalwTirFoldV1::Sum);
            for phase in terminal_phases(&x, &store, leaf, &site) {
                let narrowing = phase.position_narrowing_v1().expect("terminal");
                let (from, to) = narrowing.tile_range();
                let width = to - from;
                if width < 3 {
                    continue; // only tiles with a real middle (≥ 3 positions)
                }
                let middle = width / 2;
                assert!(middle > 0 && middle < width - 1, "a strict middle of {width}");
                let honest = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).expect("position round");

                // The signed-object path: the V2 position round binds to the phase, and the challenger's choice names the same
                // narrowing as the pure path.
                let d = palw_tir_position_dissection_for_v2(&phase, honest.clone()).expect("terminal");
                check_tir_position_dissection_v2(&phase, &d).expect("binds");
                let c = PalwTirPositionChoiceV2 {
                    version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    tile_round: phase.round(),
                    dissection_digest: palw_tir_position_dissection_digest_v2(&d),
                    position_child: middle as u8,
                };
                let named = palw_tir_position_named_v2(&phase, &d, &c).expect("names the middle");
                assert_eq!(named, narrowing.choose_position_v1(&honest, middle as u8).unwrap());
                assert_eq!(named.terminal_position_range(), Some((from + middle, from + middle + 1)));
                let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).expect("position bottom");
                assert_eq!(
                    check_tir_dissect_position_bottom_v1(&phase, &named, &bottom, leaf, &RULES),
                    Err(PalwStepRefuteError::NoFaultFound),
                    "{} leaf {leaf}: honest middle position {}",
                    f.name,
                    from + middle
                );
                honest_middles += 1;

                // A lie moved INTO the middle (and out of the first position) keeps the Σ fold; named there, it is convicted.
                let Some(k) = k_sum else { continue };
                if phase.elements()[k].is_empty() {
                    continue;
                }
                let mut lie = honest.clone();
                lie.children[middle].partials[k][0] += 1;
                lie.children[0].partials[k][0] -= 1;
                let Some(d_lie) = palw_tir_position_dissection_for_v2(&phase, lie.clone()) else { continue };
                if check_tir_position_dissection_v2(&phase, &d_lie).is_err() {
                    continue; // outside a proven interval: refused before any bottom
                }
                let c_lie = PalwTirPositionChoiceV2 { dissection_digest: palw_tir_position_dissection_digest_v2(&d_lie), ..c.clone() };
                let named = palw_tir_position_named_v2(&phase, &d_lie, &c_lie).expect("names the lying middle");
                let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).expect("position bottom");
                let verdict = check_tir_dissect_position_bottom_v1(&phase, &named, &bottom, leaf, &RULES);
                assert!(verdict.is_ok(), "{} leaf {leaf}: the middle lie convicts: {verdict:?}", f.name);
                // The old choice (digest of the honest round) cannot be replayed against the lying round.
                assert!(palw_tir_position_named_v2(&phase, &d_lie, &c).is_err());
                convicted_middles += 1;
            }
        }
    }
    assert!(honest_middles >= 1, "{honest_middles} honest middle positions played");
    assert!(convicted_middles >= 1, "{convicted_middles} middle lies convicted");
}

#[test]
fn a_position_move_bound_to_another_session_round_tile_or_claim_is_refused() {
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, Some(4)) {
            for phase in terminal_phases(&x, &store, leaf, &site) {
                let narrowing = phase.position_narrowing_v1().unwrap();
                if !narrowing.awaits_position_round() {
                    // One position: no round exists, and a V2 round is refused.
                    let d = palw_tir_position_dissection_for_v2(
                        &phase,
                        PalwTirDissectRoundV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, children: vec![] },
                    );
                    assert!(d.is_none_or(|d| check_tir_position_dissection_v2(&phase, &d).is_err()));
                    continue;
                }
                let round = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).unwrap();
                let d = palw_tir_position_dissection_for_v2(&phase, round).unwrap();
                check_tir_position_dissection_v2(&phase, &d).unwrap();
                let tampers: Vec<Box<dyn Fn(&mut PalwTirPositionDissectionV2)>> = vec![
                    Box::new(|d| d.version = 1),
                    Box::new(|d| d.session_id = Hash64::from_bytes([1; 64])),
                    Box::new(|d| d.tile_round += 1),
                    Box::new(|d| d.tile_from += 1),
                    Box::new(|d| d.tile_to += 1),
                    Box::new(|d| d.root_claim_digest = Hash64::from_bytes([2; 64])),
                    Box::new(|d| d.tile_claim_digest = Hash64::from_bytes([3; 64])),
                    Box::new(|d| {
                        d.round.children.pop();
                    }),
                    Box::new(|d| {
                        let first = d.round.children[0].clone();
                        d.round.children.push(first);
                    }),
                ];
                for tamper in &tampers {
                    let mut bad = d.clone();
                    tamper(&mut bad);
                    assert!(check_tir_position_dissection_v2(&phase, &bad).is_err());
                }
                let c = PalwTirPositionChoiceV2 {
                    version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    tile_round: phase.round(),
                    dissection_digest: palw_tir_position_dissection_digest_v2(&d),
                    position_child: 0,
                };
                palw_tir_position_named_v2(&phase, &d, &c).unwrap();
                let width = d.round.children.len() as u8;
                assert!(
                    palw_tir_position_named_v2(&phase, &d, &PalwTirPositionChoiceV2 { position_child: width, ..c.clone() }).is_err()
                );
                assert!(
                    palw_tir_position_named_v2(&phase, &d, &PalwTirPositionChoiceV2 { tile_round: c.tile_round + 1, ..c.clone() })
                        .is_err()
                );
                assert!(palw_tir_position_named_v2(&phase, &d, &PalwTirPositionChoiceV2 { version: 1, ..c.clone() }).is_err());
                checked += 1;
            }
        }
    }
    assert!(checked >= 1);
}

/// The honest depth-2 frontier at a phase awaiting disclosure: per level-1 child, its own children's claims (or itself when
/// it is one tile), plus the two V1 rounds that frontier stands for.
fn honest_lookahead(
    x: &Execution,
    store: &Store<'_>,
    phase: &PalwTirDissectPhaseV1,
    tile_positions: u32,
) -> (PalwTirLookaheadDisclosureV2, PalwTirDissectRoundV1) {
    let level1 = build_tir_dissect_round_v1(&x.binding, phase, tile_positions, store, &RULES).expect("level 1");
    let mut frontier = Vec::new();
    for (i, &(_, count)) in phase.child_ranges().iter().enumerate() {
        if count <= 1 {
            frontier.push(vec![level1.children[i].clone()]);
        } else {
            let child = phase.project_unrooted_child_v1(&level1, i as u8).expect("projects");
            let level2 = build_tir_dissect_round_v1(&x.binding, &child, tile_positions, store, &RULES).expect("level 2");
            frontier.push(level2.children);
        }
    }
    (PalwTirLookaheadDisclosureV2 { version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, frontier }, level1)
}

#[test]
fn a_two_level_lookahead_and_one_choice_path_equal_the_old_two_rounds() {
    const CAP: Option<usize> = Some(1 << 20);
    let mut played = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, None) {
            let phase = opened(&x, &store, leaf, &site);
            if phase.turn() != PalwBisectTurnV1::AwaitDisclosure {
                continue;
            }
            let (msg, level1) = honest_lookahead(&x, &store, &phase, site.tile_positions);
            // No bound passed: the bundle is refused by name.
            assert!(phase.expand_lookahead_v2(&msg, None).is_err());
            // A bound below the body refuses too (RFC-0024 §5.5).
            assert!(phase.expand_lookahead_v2(&msg, Some(0)).is_err());
            // V8 (decided 2026-10-22, lane P3): the adopted bound admits the fixture's honest frontier; one below its size refuses.
            let values = kaspa_consensus_core::palw_tir_dissect_v1::PalwTirLookaheadExpansionV2::frontier_values(&msg);
            let adopted = kaspa_consensus_core::palw_tir_position_terminal_v2::PALW_TIR_LOOKAHEAD_MAX_FRONTIER_VALUES_V2;
            assert_eq!(adopted, Some(3_072));
            assert!(values <= 3_072, "the fixture frontier ({values} values) is inside the adopted bound");
            assert!(phase.expand_lookahead_v2(&msg, adopted).is_ok());
            assert!(phase.expand_lookahead_v2(&msg, Some(values - 1)).is_err());
            let expansion = phase.expand_lookahead_v2(&msg, CAP).expect("the honest frontier is checked");
            assert_eq!(expansion.level1, level1, "the omitted level is reconstructed exactly by Σ/max");

            for (a, &(_, count)) in phase.child_ranges().iter().enumerate() {
                let picks: Vec<u8> = if count <= 1 {
                    vec![]
                } else {
                    let child = phase.project_unrooted_child_v1(&level1, a as u8).unwrap();
                    let n = child.child_ranges().len() as u8;
                    let mut v = vec![0, n - 1];
                    v.dedup();
                    v
                };
                // The old transition: round, choice, round, choice.
                let mut old = phase.clone();
                old.apply_round(&level1, 5, W).unwrap();
                old.apply_choice(&choice(old.round(), a as u8), 5, W).unwrap();
                let olds: Vec<(Vec<u8>, PalwTirDissectPhaseV1)> = if picks.is_empty() {
                    vec![(vec![a as u8], old)]
                } else {
                    let round2 = build_tir_dissect_round_v1(&x.binding, &old, site.tile_positions, &store, &RULES).unwrap();
                    picks
                        .iter()
                        .map(|&b| {
                            let mut o = old.clone();
                            o.apply_round(&round2, 5, W).unwrap();
                            o.apply_choice(&choice(o.round(), b), 5, W).unwrap();
                            (vec![a as u8, b], o)
                        })
                        .collect()
                };
                for (path, expected) in olds {
                    let mut new = phase.clone();
                    new.apply_lookahead_disclosure_v2(&msg, CAP, 5, W).unwrap();
                    let p = PalwTirChoicePathV2 {
                        version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2,
                        session_id: SESSION,
                        round: 0,
                        path: path.clone(),
                    };
                    new.apply_choice_path_v2(&msg, &p, CAP, 5, W).expect("a legal path");
                    assert_eq!(new, expected, "{} leaf {leaf} path {path:?}: lookahead ≡ two V1 rounds", f.name);
                    // A path of the wrong length is refused (one child per disclosed level).
                    let mut wrong = phase.clone();
                    wrong.apply_lookahead_disclosure_v2(&msg, CAP, 5, W).unwrap();
                    let short = PalwTirChoicePathV2 { path: path[..path.len() - 1].to_vec(), ..p.clone() };
                    let long = PalwTirChoicePathV2 { path: [path.clone(), vec![0]].concat(), ..p.clone() };
                    assert!(wrong.clone().apply_choice_path_v2(&msg, &short, CAP, 5, W).is_err());
                    assert!(wrong.apply_choice_path_v2(&msg, &long, CAP, 5, W).is_err());
                    played += 1;
                }
            }

            // B-03: a missing candidate, a duplicated one, an altered value, an extra group — refused.
            if let Some(g) = msg.frontier.iter().position(|group| group.len() >= 2) {
                let mut missing = msg.clone();
                missing.frontier[g].pop();
                assert!(phase.expand_lookahead_v2(&missing, CAP).is_err());
                let mut dup = msg.clone();
                let first = dup.frontier[g][0].clone();
                dup.frontier[g].push(first);
                assert!(phase.expand_lookahead_v2(&dup, CAP).is_err());
            }
            let mut extra = msg.clone();
            extra.frontier.push(extra.frontier[0].clone());
            assert!(phase.expand_lookahead_v2(&extra, CAP).is_err());
            if let Some(r) = msg.frontier[0][0].partials.iter().position(|v| !v.is_empty()) {
                // Raise the row throughout the frontier so every reduction level
                // changes, including maxima in another group.
                let mut altered = msg.clone();
                for group in &mut altered.frontier {
                    for child in group {
                        for value in &mut child.partials[r] {
                            *value += 1;
                        }
                    }
                }
                assert!(phase.expand_lookahead_v2(&altered, CAP).is_err());
            }
            // An unsupported disclosure version is refused while a valid one is pending.
            // Version 3 is now the supported position-lookahead form.
            let mut pending = phase.clone();
            pending.apply_lookahead_disclosure_v2(&msg, CAP, 5, W).unwrap();
            let mut other = msg.clone();
            other.version = u16::MAX;
            let p =
                PalwTirChoicePathV2 { version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, session_id: SESSION, round: 0, path: vec![0, 0] };
            assert!(pending.apply_choice_path_v2(&other, &p, CAP, 5, W).is_err());
        }
    }
    assert!(played >= 2, "{played} lookahead paths compared");
}

#[test]
fn the_position_witness_budget_refuses_without_an_adopted_bound_and_never_convicts() {
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f, Some(4)) {
            for phase in terminal_phases(&x, &store, leaf, &site) {
                let narrowing = phase.position_narrowing_v1().unwrap();
                if !narrowing.awaits_position_round() {
                    continue;
                }
                let round = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).unwrap();
                let d = palw_tir_position_dissection_for_v2(&phase, round).unwrap();
                let c = PalwTirPositionChoiceV2 {
                    version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    tile_round: phase.round(),
                    dissection_digest: palw_tir_position_dissection_digest_v2(&d),
                    position_child: 0,
                };
                let named = palw_tir_position_named_v2(&phase, &d, &c).unwrap();
                let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).unwrap();
                let close = PalwTirPositionCloseV2 {
                    version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    verdict: PalwCourtVerdictV2::ChallengerDefeated,
                    dissection: d,
                    responder_signature: vec![0; 4627],
                    choice: c,
                    challenger_signature: vec![0; 4627],
                    proof: PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom) },
                    step_multiproof: None,
                };
                // No budget passed: refused by name (RFC-0024 §4.4), never a conviction.
                assert_eq!(
                    check_tir_position_witness_budget_v2(&close, None),
                    Err(PalwTirPositionWitnessErrorV2::PositionWitnessBudgetIncomplete)
                );
                let bytes = borsh::to_vec(&close).unwrap().len() as u64;
                // V8 (decided 2026-10-22, lane P3): the fixture's real honest position close fits the adopted one-part budget.
                assert_eq!(PALW_TIR_POSITION_WITNESS_BUDGET_V2, Some(83_333));
                assert_eq!(check_tir_position_witness_budget_v2(&close, PALW_TIR_POSITION_WITNESS_BUDGET_V2), Ok(bytes));
                eprintln!("[p3-v8] fixture position close: {bytes} B against the adopted budget 83,333 B");
                assert!(matches!(
                    check_tir_position_witness_budget_v2(&close, Some(bytes - 1)),
                    Err(PalwTirPositionWitnessErrorV2::PositionWitnessOverBudget { .. })
                ));
                assert_eq!(check_tir_position_witness_budget_v2(&close, Some(bytes)), Ok(bytes));
                checked += 1;
            }
        }
    }
    assert!(checked >= 1);
}
