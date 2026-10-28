//! **RFC-0002 Phase F, step F7: the generic history dissection, played to the bottom.**
//!
//! On the corpus programs that keep a history (the dense GQA model and the sliding + global one),
//! every committed tile whose cone reduces over `H` is a dissected leaf: its cone's reductions (the
//! softmax's maximum and exponent sum, the value contraction) are found from the program alone.
//!
//! * An honest responder's root claim is admitted (it finalizes to the committed tile), every
//!   honest round folds, and the bottom acquits, whichever child the challenger names.
//! * A responder that committed a tile finalized from a lie in ANY one reduction — raised or lowered
//!   by one, in any demanded element — and keeps its rounds folding by pushing the lie into a child
//!   is followed by the challenger to the lie's tile and convicted there.
//! * The root claim is refused when it is about another leaf, when a value is outside its
//!   reduction's proven interval, when it claims an element the tile does not read, and when it does
//!   not finalize to the committed tile.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    build_tir_dissect_bottom_v1, build_tir_dissect_round_v1, build_tir_root_claim_v1, check_tir_dissect_bottom_v1,
    check_tir_root_claim_v1, tir_root_claim_finalizes_to_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1,
    PalwTirFoldV1, PalwTirRangeClaimV1, PalwTirRootClaimV1, palw_tir_dissect_site_v1,
};

const SESSION: Hash64 = Hash64::from_bytes([0x5E; 64]);
const W: u64 = 10;

/// Every dissected leaf of an admissible fixture: `(leaf index, site)`.
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

#[test]
fn an_honest_dissection_is_admitted_folds_and_acquits() {
    let mut played = 0;
    let mut reductions_seen = std::collections::BTreeSet::new();
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        for (leaf, site) in dissected(&f) {
            let root =
                build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).unwrap_or_else(|e| panic!("{} leaf {leaf}: {e}", f.name));
            let admitted = check_tir_root_claim_v1(&root, leaf, &RULES).unwrap_or_else(|e| panic!("{} leaf {leaf}: {e}", f.name));
            assert_eq!(admitted, site);
            for fold in &site.folds {
                reductions_seen.insert(*fold as u8);
            }
            // The challenger names each child in turn across leaves: the first, then the last.
            let mut phase = PalwTirDissectPhaseV1::open(SESSION, leaf, &site, &root, 2, 0, W).expect("opens");
            let mut t = 1;
            while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("a round");
                phase.apply_round(&round, t, W).unwrap_or_else(|e| panic!("{} leaf {leaf}: {e}", f.name));
                let children = phase.child_ranges().len() as u8;
                let pick = if played % 2 == 0 { 0 } else { children - 1 };
                phase.apply_choice(&choice(phase.round(), pick), t + 1, W).expect("a legal choice");
                t += 2;
            }
            assert_eq!(phase.turn(), PalwBisectTurnV1::Terminal);
            let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("the bottom");
            assert_eq!(
                check_tir_dissect_bottom_v1(&phase, &bottom, leaf, &RULES),
                Err(PalwStepRefuteError::NoFaultFound),
                "{} leaf {leaf}",
                f.name
            );
            played += 1;
        }
    }
    assert!(played >= 4, "{played} dissections played");
    assert_eq!(reductions_seen.len(), 2, "sums and maxima both dissected");
}

/// The honest children of the disputed range against the phase's root (what a truthful challenger
/// computes for itself), and the same children pushed to fold to the phase's (possibly false) claim.
fn children_of(
    x: &Execution,
    f: &Fixture,
    phase: &PalwTirDissectPhaseV1,
    site: &PalwTirDissectSiteV1,
) -> (PalwTirDissectRoundV1, PalwTirDissectRoundV1) {
    let store = Store { f, x };
    let honest = build_tir_dissect_round_v1(&x.binding, phase, site.tile_positions, &store, &RULES).expect("a round");
    let mut pushed = honest.clone();
    for (i, fold) in site.folds.iter().enumerate() {
        for e in 0..phase.elements()[i].len() {
            let claim = phase.claim().partials[i][e];
            match fold {
                PalwTirFoldV1::Sum => {
                    let sum: i128 = pushed.children.iter().map(|c| c.partials[i][e]).sum();
                    pushed.children[0].partials[i][e] += claim - sum;
                }
                PalwTirFoldV1::Max => {
                    let max = pushed.children.iter().map(|c| c.partials[i][e]).max().unwrap();
                    if claim > max {
                        pushed.children[0].partials[i][e] = claim;
                    } else {
                        for c in pushed.children.iter_mut() {
                            c.partials[i][e] = c.partials[i][e].min(claim);
                        }
                    }
                }
            }
        }
    }
    (honest, pushed)
}

/// Plays a lying root claim to the bottom against an execution whose committed tile it finalizes to:
/// the responder keeps every round folding by pushing the lie into a child, the challenger names the
/// first child whose claim is not what it computes. `None` when a round cannot fold at all (the lie
/// then loses by the responder's silence, before any bottom).
fn play_lie(
    f: &Fixture,
    x: &Execution,
    leaf: u64,
    site: &PalwTirDissectSiteV1,
    lie: &PalwTirRootClaimV1,
) -> Option<Result<kaspa_consensus_core::palw_step_leg::PalwStepRefutationVerdictV1, PalwStepRefuteError>> {
    check_tir_root_claim_v1(lie, leaf, &RULES)
        .unwrap_or_else(|e| panic!("{} leaf {leaf}: the lie finalizes, so it is admitted: {e}", f.name));
    let mut phase = PalwTirDissectPhaseV1::open(SESSION, leaf, site, lie, 2, 0, W).expect("opens");
    let mut t = 1;
    while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
        let (truth, pushed) = children_of(x, f, &phase, site);
        if phase.apply_round(&pushed, t, W).is_err() {
            return None;
        }
        let named = pushed.children.iter().zip(&truth.children).position(|(p, h)| p != h).expect("the lie is somewhere") as u8;
        phase.apply_choice(&choice(phase.round(), named), t + 1, W).expect("a legal choice");
        t += 2;
    }
    let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &Store { f, x }, &RULES).expect("the bottom");
    Some(check_tir_dissect_bottom_v1(&phase, &bottom, leaf, &RULES))
}

#[test]
fn a_lie_in_any_reduction_is_followed_to_its_tile_and_convicted() {
    // A lie of one in a softmax sum or a contraction is usually absorbed by the requantisation that
    // follows it; the steps below find, per reduction, the smallest power-of-two lie that moves the
    // committed tile, and also play the absorbed lie of one against the honest tile (a false claim is
    // convicted whether or not it moved anything).
    const STEPS: [i128; 9] = [1, 1 << 4, 1 << 8, 1 << 12, 1 << 16, 1 << 20, 1 << 24, 1 << 28, 1 << 32];
    let (mut moved, mut absorbed, mut silent, mut by_interval) = (0, 0, 0, 0);
    let (mut moved_by_fold, mut absorbed_by_fold) = (std::collections::BTreeSet::new(), std::collections::BTreeSet::new());
    for f in fixtures() {
        let honest = f.honest();
        let honest_store = Store { f: &f, x: &honest };
        // Every third leaf: each play is a full dissection of range evaluations.
        for (leaf, site) in dissected(&f).into_iter().filter(|(leaf, _)| leaf % 3 == 0) {
            let li = leaf as usize;
            let iv = f.interval(&f.leaves[li]);
            let root = build_tir_root_claim_v1(&honest.binding, leaf, &honest_store, &RULES).expect("honest root");
            for k in 0..site.reductions.len() {
                let n = root.totals.partials[k].len();
                let mut found = false;
                'search: for e in [0, n / 2, n - 1] {
                    for step in STEPS {
                        for delta in [step, -step] {
                            let mut totals = root.totals.clone();
                            totals.partials[k][e] += delta;
                            if totals.partials[k][e] < site.bounds[k].lo || totals.partials[k][e] > site.bounds[k].hi {
                                continue;
                            }
                            let Ok(tile) =
                                tir_root_claim_finalizes_to_v1(&honest.binding, leaf, &root.elements, &totals, &honest_store, &RULES)
                            else {
                                continue;
                            };
                            let absorbed_one = tile == f.values[li];
                            if absorbed_one && !(step == 1 && e == 0) {
                                continue;
                            }
                            if tile.iter().any(|v| !iv.contains(*v)) {
                                by_interval += 1; // PALW-TIR-33 convicts it by a cone close, before any dissection
                                continue;
                            }
                            // The executor committed the tile the lie finalizes to (the honest one, if absorbed).
                            let mut values = f.values.clone();
                            values[li] = tile;
                            let x = f.commit(&values, &f.rows, &f.generated);
                            let carriage =
                                build_tir_root_claim_v1(&x.binding, leaf, &Store { f: &f, x: &x }, &RULES).expect("a carriage");
                            let lie = PalwTirRootClaimV1 {
                                version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                                elements: root.elements.clone(),
                                totals,
                                finalize: carriage.finalize,
                            };
                            match play_lie(&f, &x, leaf, &site, &lie) {
                                None => silent += 1,
                                Some(verdict) => {
                                    let verdict = verdict.unwrap_or_else(|e| {
                                        panic!("{} leaf {leaf} r{k} e{e} {delta}: the lie was acquitted: {e:?}", f.name)
                                    });
                                    assert!(
                                        matches!(verdict.fault, PalwStepFaultV1::ComputationMismatch { .. }),
                                        "{:?}",
                                        verdict.fault
                                    );
                                    if absorbed_one {
                                        absorbed += 1;
                                        absorbed_by_fold.insert(site.folds[k] as u8);
                                    } else {
                                        moved += 1;
                                        moved_by_fold.insert(site.folds[k] as u8);
                                    }
                                }
                            }
                            if !absorbed_one {
                                found = true;
                                break 'search;
                            }
                        }
                    }
                }
                let _ = found; // a reduction no lie can move (the softmax maximum) is played absorbed only
            }
        }
    }
    eprintln!(
        "lies convicted at the bottom: {moved} that moved the tile, {absorbed} absorbed; {silent} could not fold; {by_interval} left the interval"
    );
    assert!(moved >= 8, "{moved} tile-moving lies convicted at the bottom");
    assert!(absorbed >= 4, "{absorbed} absorbed lies convicted at the bottom");
    // A softmax is invariant under its shift, so a lie in the maximum is absorbed (or underflows the
    // sum to nothing and does not finalize); a false maximum is still a false claim, followed down.
    assert!(moved_by_fold.contains(&(PalwTirFoldV1::Sum as u8)), "a tile-moving lie in a sum followed down");
    assert_eq!(absorbed_by_fold.len(), 2, "a lie in a sum and a lie in a maximum both followed down");
}

#[test]
fn a_root_claim_is_refused_unless_it_is_this_leafs_and_finalizes() {
    let mut checked = 0;
    for f in fixtures() {
        let x = f.honest();
        let store = Store { f: &f, x: &x };
        let Some((leaf, site)) = dissected(&f).into_iter().next() else { continue };
        let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("honest root");
        assert!(check_tir_root_claim_v1(&root, leaf, &RULES).is_ok());
        assert!(check_tir_root_claim_v1(&root, leaf + 1, &RULES).is_err(), "{}: another leaf", f.name);
        // Outside the proven interval.
        let mut out = root.clone();
        out.totals.partials[0][0] = site.bounds[0].hi + 1;
        assert!(check_tir_root_claim_v1(&out, leaf, &RULES).is_err(), "{}: outside the interval", f.name);
        // An element the tile does not read (appended past the demanded ones, if the reduction has one).
        let mut extra = root.clone();
        let last = *extra.elements[0].last().unwrap();
        if (last as u64) + 1 < site.counts[0] {
            extra.elements[0].push(last + 1);
            extra.totals.partials[0].push(0);
            assert!(check_tir_root_claim_v1(&extra, leaf, &RULES).is_err(), "{}: a value nobody reads", f.name);
        }
        // Honest totals against a forged committed tile: does not finalize.
        let mut values = f.values.clone();
        let li = leaf as usize;
        let iv = f.interval(&f.leaves[li]);
        values[li][0] = if iv.contains(values[li][0] + 1) { values[li][0] + 1 } else { values[li][0] - 1 };
        let forged = f.commit(&values, &f.rows, &f.generated);
        let carriage = build_tir_root_claim_v1(&forged.binding, leaf, &Store { f: &f, x: &forged }, &RULES).expect("a carriage");
        assert!(
            check_tir_root_claim_v1(&carriage, leaf, &RULES).is_err(),
            "{}: the honest totals do not finalize to a forged tile",
            f.name
        );
        // A round that does not fold is refused; a choice out of turn, round or range too.
        let mut phase = PalwTirDissectPhaseV1::open(SESSION, leaf, &site, &root, 2, 0, W).expect("opens");
        if phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
            let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("a round");
            let mut bent = round.clone();
            bent.children[0].partials[0][0] += 1;
            assert!(phase.apply_round(&bent, 1, W).is_err(), "{}: a round that does not fold", f.name);
            assert!(phase.apply_choice(&choice(0, 0), 1, W).is_err(), "{}: a choice before the round", f.name);
            let fewer = PalwTirDissectRoundV1 { version: round.version, children: round.children[..1].to_vec() };
            assert!(phase.apply_round(&fewer, 1, W).is_err(), "{}: the wrong number of children", f.name);
            phase.apply_round(&round, 1, W).expect("the honest round");
            assert!(phase.apply_choice(&choice(1, 0), 2, W).is_err(), "{}: another round's choice", f.name);
            assert!(phase.apply_choice(&choice(0, 99), 2, W).is_err(), "{}: a child out of range", f.name);
            // Silence: the challenger owes the choice now.
            let mut silent = phase.clone();
            let no_show = silent.declare_no_show(phase.last_deadline_daa() + 1).expect("past the deadline");
            assert_eq!(no_show.silent_party, kaspa_consensus_core::palw_bisect::PalwBisectPartyV1::Challenger);
        }
        let _ = PalwTirRangeClaimV1 { partials: Vec::new() };
        checked += 1;
    }
    assert!(checked >= 2, "{checked}");
}
