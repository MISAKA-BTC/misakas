//! **ADR-0096 Decisions 7 and 8: the committed decode token is the argmax over the ADMITTED
//! lanes, and the court still needs two disclosures.**
//!
//! ADR-0082 Decision 11 made the committed token a seeded argmax over the whole logits row
//! ([`crate::palw_decode_select_v2`]). This module is the one narrowing an exact court can carry:
//!
//! ```text
//!   committed = argmax over { j : admitted(state_p, j) } of decode_lane_key_v2(values[j], …)
//!                                                            ties to the LOWEST index
//! ```
//!
//! `state_p` is a constraint automaton's state after the committed prefix — a value both sides
//! derive from material the claim already commits — and `admitted` reads that state and the LANE
//! INDEX and nothing else. **That is the whole design constraint, and it is why this module takes
//! a predicate over lanes rather than a scoring function over rows.**
//!
//! **What it buys, stated as the property the court depends on.** ADR-0049 Decision E's refutation
//! is two disclosures: open the committed lane's tile and a lane that should have beaten it,
//! recompute both keys, compare. A constraint that re-scored, renormalised, or read the row's mass
//! would need the WHOLE row at every dispute — at a Qwen-class vocabulary the 993 KB row
//! `palw_close_budget` already refuses. A per-lane predicate needs neither: a forbidden lane's
//! value cannot change the answer at all, and `a_forbidden_lanes_value_never_changes_the_selection`
//! is that as a sweep.
//!
//! **What it adds to the court.** One arm, and it convicts on ONE disclosure:
//! [`check_constrained_decode_token_v1`] refutes a claim whose committed lane is not admitted,
//! whatever its key. A class that ignores the constraint is caught by opening the token it
//! committed, without a competing lane to point at.
//!
//! **The empty constraint is the shipped rule, byte for byte.** [`admit_all_v1`] admits every lane,
//! the argmax runs over the whole row, and the answer is `decode_token_select_v2` — the same
//! property ADR-0082 Decision 11 holds at `T_q = 0`, swept here by
//! `the_empty_constraint_is_the_shipped_rule_on_every_row`. So a job that declares no constraint
//! selects identically on both sides of the fence, which is what lets the fence be dormant without
//! the rule being a second code path.
//!
//! **Nothing in this module is armed by its existence.** The rule that selects between the
//! unconstrained and the constrained form is `Params::palw_fp_decode_constraint`, `None` on every
//! shipped preset and REFUSED at assembly by this build (`validate_palw_v2`) — there is no
//! automaton in the state transition and no engine decodes under one. What is here is the
//! selection and the refutation, as pure functions, so that the automaton lands against a rule the
//! court already agrees with.

use crate::palw_decode_select_v2::{PalwDecodeSamplingV2, decode_lane_beats_v2};

/// **The empty constraint** — every lane admitted, which is the rule below the fence and the rule a
/// job that declares no constraint gets above it.
pub fn admit_all_v1(_lane: usize) -> bool {
    true
}

/// **Why a constrained row has no answer.** Both are refusals rather than a fallback lane, and the
/// distinction is the one an operator needs: an empty row is a broken engine, an empty admitted set
/// is a constraint that has painted itself into a corner at this position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwConstrainedSelectErrorV1 {
    /// The logits row is empty. Not reachable from a class that ran.
    EmptyRow,
    /// The row is non-empty and the constraint admits none of it.
    ///
    /// **It is an error and never a silent widening.** A rule that fell back to the unconstrained
    /// argmax here would make "the committed token is admitted" false exactly when it matters, and
    /// the court's one-disclosure arm would convict an engine that followed the rule.
    NoAdmittedLane,
}

impl core::fmt::Display for PalwConstrainedSelectErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::EmptyRow => f.write_str("the logits row is empty"),
            Self::NoAdmittedLane => f.write_str("the constraint admits no lane at this position"),
        }
    }
}

/// **The constrained selection rule.**
///
/// The argmax of [`crate::palw_decode_select_v2::decode_lane_key_v2`] over the admitted lanes,
/// ties to the lowest index — the same tie rule as every selection in this tree, so a constrained
/// row and an unconstrained one break a tie the same way.
///
/// With [`admit_all_v1`] this is `decode_token_select_v2` exactly, on every row.
pub fn decode_token_select_constrained_v1(
    values: &[i32],
    sampling: &PalwDecodeSamplingV2,
    position: u32,
    admitted: impl Fn(usize) -> bool,
) -> Result<usize, PalwConstrainedSelectErrorV1> {
    if values.is_empty() {
        return Err(PalwConstrainedSelectErrorV1::EmptyRow);
    }
    let mut best: Option<(usize, i64)> = None;
    for (lane, value) in values.iter().enumerate() {
        if !admitted(lane) {
            continue;
        }
        let key = sampling.lane_key(*value, position, lane);
        match best {
            None => best = Some((lane, key)),
            Some((best_lane, best_key)) if decode_lane_beats_v2(key, lane, best_key, best_lane) => best = Some((lane, key)),
            Some(_) => {}
        }
    }
    best.map(|(lane, _)| lane).ok_or(PalwConstrainedSelectErrorV1::NoAdmittedLane)
}

/// **What the court decides about one committed token, from at most two disclosures.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwConstrainedVerdictV1 {
    /// The committed lane is admitted and the disclosed competitor does not beat it. **Not a
    /// finding that the claim is correct** — only that these disclosures do not refute it, which is
    /// the same thing ADR-0049 Decision E's arm answers.
    Upheld,
    /// **The committed lane is not admitted.** One disclosure, and the value is irrelevant: a class
    /// that ignored the constraint committed a token the rule forbids.
    RefutedNotAdmitted,
    /// The competitor is admitted and beats the committed lane's key.
    RefutedBeaten,
}

impl PalwConstrainedVerdictV1 {
    pub const fn is_refuted(&self) -> bool {
        !matches!(self, Self::Upheld)
    }
}

/// **The refutation, as the court runs it.**
///
/// Takes the committed lane and ONE competitor — the two tiles a challenger opened — plus the
/// sampling pair and the admission predicate, and answers in the order the arms are cheap:
///
/// 1. the committed lane is not admitted → refuted, without looking at any key;
/// 2. the competitor is not admitted → it proves nothing, and the answer is `Upheld` for these
///    disclosures (a forbidden lane with a huge logit is exactly what a constraint is FOR);
/// 3. otherwise, the ordinary key comparison.
///
/// **The sampling pair must come from the CLAIM**, never from the challenger — a challenger who
/// could state the temperature could state `0` and convict an honestly sampled token. That is
/// [`PalwDecodeSamplingV2`]'s own warning and it is unchanged here; the admission predicate is
/// under the same rule, because a challenger who could choose the automaton could forbid the lane
/// the class correctly committed.
pub fn check_constrained_decode_token_v1(
    committed_lane: usize,
    committed_value: i32,
    beat_lane: usize,
    beat_value: i32,
    sampling: &PalwDecodeSamplingV2,
    position: u32,
    admitted: impl Fn(usize) -> bool,
) -> PalwConstrainedVerdictV1 {
    if !admitted(committed_lane) {
        return PalwConstrainedVerdictV1::RefutedNotAdmitted;
    }
    if beat_lane == committed_lane || !admitted(beat_lane) {
        return PalwConstrainedVerdictV1::Upheld;
    }
    let committed_key = sampling.lane_key(committed_value, position, committed_lane);
    let beat_key = sampling.lane_key(beat_value, position, beat_lane);
    if decode_lane_beats_v2(beat_key, beat_lane, committed_key, committed_lane) {
        PalwConstrainedVerdictV1::RefutedBeaten
    } else {
        PalwConstrainedVerdictV1::Upheld
    }
}

/// **An explicit admitted set over a bounded vocabulary**, as a bitmap.
///
/// The automaton is what a network derives at each position; this is the materialised form the
/// court and the tests hold, and the one an engine may cache for a position. One bit per lane, so a
/// 152,064-lane vocabulary costs 19 KB — which is why the admitted set may be materialised while
/// the logits row it selects over may not.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PalwAdmittedSetV1 {
    bits: Vec<u64>,
    lanes: usize,
}

impl PalwAdmittedSetV1 {
    /// An empty set over `lanes` lanes — admits nothing, which
    /// [`decode_token_select_constrained_v1`] answers `NoAdmittedLane` for.
    pub fn none(lanes: usize) -> Self {
        Self { bits: vec![0u64; lanes.div_ceil(64)], lanes }
    }

    /// Every lane admitted — the materialised [`admit_all_v1`].
    pub fn all(lanes: usize) -> Self {
        let mut set = Self::none(lanes);
        for lane in 0..lanes {
            set.admit(lane);
        }
        set
    }

    /// The set of exactly these lanes. Out-of-range lanes are ignored rather than panicking: a
    /// caller building one from an automaton's output is not the place to discover a vocabulary
    /// mismatch, and `lanes()` states the width the set actually has.
    pub fn from_lanes(lanes: usize, admitted: impl IntoIterator<Item = usize>) -> Self {
        let mut set = Self::none(lanes);
        for lane in admitted {
            set.admit(lane);
        }
        set
    }

    pub fn admit(&mut self, lane: usize) {
        if lane < self.lanes {
            self.bits[lane / 64] |= 1u64 << (lane % 64);
        }
    }

    /// Is this lane admitted? Out of range is `false` — a lane the vocabulary does not have is not
    /// a lane a class may commit.
    pub fn admits(&self, lane: usize) -> bool {
        lane < self.lanes && (self.bits[lane / 64] >> (lane % 64)) & 1 == 1
    }

    pub fn lanes(&self) -> usize {
        self.lanes
    }

    /// How many lanes are admitted. Zero is the corner the selection rule refuses.
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// The predicate form, for the two functions above.
    pub fn predicate(&self) -> impl Fn(usize) -> bool + '_ {
        move |lane| self.admits(lane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_decode_select_v2::{PALW_DECODE_SEED_GREEDY, PALW_DECODE_T_ONE, decode_token_select_v2};

    /// The generator `palw_decode_select_v2`'s own sweeps use, kept identical so the two modules
    /// are measured over the same rows.
    fn rows(seed: u64, count: usize, width: usize, spread: i32) -> Vec<Vec<i32>> {
        let mut x = seed | 1;
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        (0..count).map(|_| (0..width).map(|_| ((next() % (2 * spread as u64 + 1)) as i64 - spread as i64) as i32).collect()).collect()
    }

    fn sampling(temperature_q: u32, seed: [u8; 32]) -> PalwDecodeSamplingV2 {
        PalwDecodeSamplingV2 { seed, temperature_q }
    }

    /// **ADR-0096 invariant 1: the empty constraint IS the shipped rule.** Not "agrees on typical
    /// rows" — the same index on every row, at every temperature, at every position. This is what
    /// lets the fence be dormant without the constrained form being a second code path, and it is
    /// the exact shape of `the_greedy_temperature_is_the_shipped_rule_on_every_row` one decision on.
    #[test]
    fn the_empty_constraint_is_the_shipped_rule_on_every_row() {
        let hot = [0x5Au8; 32];
        for width in [1usize, 2, 3, 17, 64, 129, 1024] {
            for spread in [1i32, 3, 1_000, i32::MAX / 2] {
                for row in rows(0xD1B5_4A32_D192_ED03 ^ width as u64, 12, width, spread) {
                    for temperature_q in [0u32, 1, PALW_DECODE_T_ONE as u32, u32::MAX] {
                        for (seed, position) in [(PALW_DECODE_SEED_GREEDY, 0u32), (hot, 0), (hot, 1), (hot, 4_095)] {
                            let s = sampling(temperature_q, seed);
                            let unconstrained = decode_token_select_v2(&row, &seed, position, temperature_q);
                            assert_eq!(
                                decode_token_select_constrained_v1(&row, &s, position, admit_all_v1),
                                Ok(unconstrained),
                                "the empty constraint diverged at width {width}, spread {spread}, T_q {temperature_q}"
                            );
                            // …and the materialised form of the same set answers the same thing.
                            let all = PalwAdmittedSetV1::all(row.len());
                            assert_eq!(
                                decode_token_select_constrained_v1(&row, &s, position, all.predicate()),
                                Ok(unconstrained),
                                "the materialised admit-all must be the predicate admit-all"
                            );
                        }
                    }
                }
            }
        }
        // The degenerate rows the generator cannot produce.
        let s = sampling(0, PALW_DECODE_SEED_GREEDY);
        for row in [vec![i32::MIN; 5], vec![i32::MAX; 5], vec![0; 5], vec![i32::MIN, i32::MAX, i32::MIN]] {
            assert_eq!(
                decode_token_select_constrained_v1(&row, &s, 0, admit_all_v1),
                Ok(decode_token_select_v2(&row, &PALW_DECODE_SEED_GREEDY, 0, 0))
            );
        }
    }

    /// **ADR-0096 invariant 2: the committed token is admitted, and an empty admitted set is a
    /// REFUSAL rather than a widening.**
    ///
    /// The second half is the load-bearing one. A rule that fell back to the unconstrained argmax
    /// when nothing was admitted would make the court's one-disclosure arm convict an engine that
    /// had followed the rule — the two halves have to agree about the corner.
    #[test]
    fn the_selected_lane_is_admitted_and_an_empty_set_refuses() {
        let s = sampling(PALW_DECODE_T_ONE as u32, [0x11u8; 32]);
        for width in [1usize, 2, 5, 64, 257] {
            for row in rows(0xA076_1D64_78BD_642F ^ width as u64, 8, width, 5_000) {
                // Every non-empty subset shape this width can show: singletons, the top half, an
                // odd/even stripe — each including at least one lane that is NOT the free argmax
                // whenever the width allows it.
                let free = decode_token_select_v2(&row, &s.seed, 3, s.temperature_q);
                let shapes: Vec<Vec<usize>> = vec![
                    vec![0],
                    vec![width - 1],
                    (0..width).filter(|l| l % 2 == 0).collect(),
                    (0..width).filter(|l| *l != free).collect(),
                    (width / 2..width).collect(),
                ];
                for shape in shapes.into_iter().filter(|s| !s.is_empty()) {
                    let set = PalwAdmittedSetV1::from_lanes(width, shape.iter().copied());
                    let picked = decode_token_select_constrained_v1(&row, &s, 3, set.predicate()).expect("a non-empty set answers");
                    assert!(set.admits(picked), "the rule selected a lane its own constraint forbids");
                    assert!(shape.contains(&picked));
                    // And it is the best of the admitted lanes, checked directly.
                    for lane in shape.iter().copied() {
                        let beats =
                            decode_lane_beats_v2(s.lane_key(row[lane], 3, lane), lane, s.lane_key(row[picked], 3, picked), picked);
                        assert!(!beats, "lane {lane} beats the selected lane {picked} and is admitted");
                    }
                }
                // The corner: nothing admitted is a named refusal, on a row that plainly has an argmax.
                let empty = PalwAdmittedSetV1::none(width);
                assert_eq!(empty.count(), 0);
                assert_eq!(
                    decode_token_select_constrained_v1(&row, &s, 3, empty.predicate()),
                    Err(PalwConstrainedSelectErrorV1::NoAdmittedLane)
                );
            }
        }
        // An empty ROW is the other refusal, and a different one.
        assert_eq!(decode_token_select_constrained_v1(&[], &s, 0, admit_all_v1), Err(PalwConstrainedSelectErrorV1::EmptyRow));
    }

    /// **ADR-0096 invariant 4: a forbidden lane's value never changes the selection.**
    ///
    /// This is Decision 8 as a sweep, and it is the property the two-disclosure refutation rests
    /// on: if a lane the constraint forbids could move the answer, a court would need the whole row
    /// to try one token, and the design would be the 993 KB disclosure `palw_close_budget` refuses.
    #[test]
    fn a_forbidden_lanes_value_never_changes_the_selection() {
        let s = sampling(PALW_DECODE_T_ONE as u32 / 2, [0xC3u8; 32]);
        for width in [2usize, 3, 33, 128] {
            for row in rows(0x2545_F491_4F6C_DD1D ^ width as u64, 8, width, 9_000) {
                // Admit the even lanes; every odd lane is forbidden and therefore free to move.
                let set = PalwAdmittedSetV1::from_lanes(width, (0..width).filter(|l| l % 2 == 0));
                let before = decode_token_select_constrained_v1(&row, &s, 11, set.predicate()).expect("even lanes exist");
                for forbidden in (0..width).filter(|l| l % 2 == 1) {
                    for replacement in [i32::MIN, -1, 0, 1, i32::MAX] {
                        let mut perturbed = row.clone();
                        perturbed[forbidden] = replacement;
                        assert_eq!(
                            decode_token_select_constrained_v1(&perturbed, &s, 11, set.predicate()),
                            Ok(before),
                            "moving forbidden lane {forbidden} to {replacement} moved the answer — the court would need the row"
                        );
                    }
                }
            }
        }
    }

    /// **The court's two arms, and the one that convicts on a single disclosure.**
    ///
    /// A committed lane the constraint forbids is refuted whatever its value — including when it is
    /// the row's free argmax by a wide margin, which is exactly the case a class that ignored the
    /// constraint produces. A forbidden COMPETITOR proves nothing, which is the other half: a
    /// challenger cannot convict by pointing at a lane the rule already excluded.
    #[test]
    fn the_refutation_convicts_a_forbidden_commit_and_ignores_a_forbidden_competitor() {
        let s = sampling(0, PALW_DECODE_SEED_GREEDY);
        // Lane 1 is the row's argmax by a mile, and the constraint forbids it.
        let row = [10i32, 1_000_000, 20, 30];
        let set = PalwAdmittedSetV1::from_lanes(row.len(), [0usize, 2, 3]);
        assert_eq!(decode_token_select_constrained_v1(&row, &s, 0, set.predicate()), Ok(3), "the best ADMITTED lane");

        // One disclosure: the committed lane is forbidden. The competitor's identity is irrelevant,
        // and so is the committed value — a class cannot buy admission with a large logit.
        for (beat_lane, beat_value) in [(0usize, row[0]), (3, row[3]), (1, row[1])] {
            assert_eq!(
                check_constrained_decode_token_v1(1, row[1], beat_lane, beat_value, &s, 0, set.predicate()),
                PalwConstrainedVerdictV1::RefutedNotAdmitted
            );
        }

        // The ordinary arm, over admitted lanes: 3 beats 2, so a claim on 2 falls to a disclosure of 3.
        assert_eq!(
            check_constrained_decode_token_v1(2, row[2], 3, row[3], &s, 0, set.predicate()),
            PalwConstrainedVerdictV1::RefutedBeaten
        );
        // …and the correct commitment survives the best competitor there is.
        assert_eq!(check_constrained_decode_token_v1(3, row[3], 2, row[2], &s, 0, set.predicate()), PalwConstrainedVerdictV1::Upheld);
        // A FORBIDDEN competitor proves nothing, even though its key dwarfs the committed lane's.
        assert_eq!(
            check_constrained_decode_token_v1(3, row[3], 1, row[1], &s, 0, set.predicate()),
            PalwConstrainedVerdictV1::Upheld,
            "a challenger must not convict by pointing at a lane the constraint excludes"
        );
        // Pointing at the committed lane itself is not a disclosure of anything.
        assert_eq!(check_constrained_decode_token_v1(3, row[3], 3, row[3], &s, 0, set.predicate()), PalwConstrainedVerdictV1::Upheld);
        assert!(PalwConstrainedVerdictV1::RefutedNotAdmitted.is_refuted());
        assert!(PalwConstrainedVerdictV1::RefutedBeaten.is_refuted());
        assert!(!PalwConstrainedVerdictV1::Upheld.is_refuted());
    }

    /// **The court's verdict agrees with the rule, on every row and every subset it is given.**
    ///
    /// The selection rule and the refutation are two functions and could drift; this is the pin
    /// that says they do not. For each row and admitted set: the lane the rule picks is upheld
    /// against every competitor, and every OTHER admitted lane is refuted by a disclosure of the
    /// lane the rule picked.
    #[test]
    fn the_refutation_and_the_selection_rule_never_disagree() {
        for temperature_q in [0u32, 1, PALW_DECODE_T_ONE as u32] {
            let s = sampling(temperature_q, [0x7Eu8; 32]);
            for width in [2usize, 7, 40] {
                for row in rows(0x1442_9C68_1A0B_9F3D ^ width as u64, 6, width, 4_000) {
                    for stride in [1usize, 2, 3] {
                        let set = PalwAdmittedSetV1::from_lanes(width, (0..width).filter(|l| l % stride == 0));
                        let picked = decode_token_select_constrained_v1(&row, &s, 2, set.predicate()).expect("non-empty");
                        for lane in 0..width {
                            assert_eq!(
                                check_constrained_decode_token_v1(picked, row[picked], lane, row[lane], &s, 2, set.predicate()),
                                PalwConstrainedVerdictV1::Upheld,
                                "the rule's own answer was refuted by lane {lane}"
                            );
                            if lane != picked && set.admits(lane) {
                                assert_eq!(
                                    check_constrained_decode_token_v1(lane, row[lane], picked, row[picked], &s, 2, set.predicate()),
                                    PalwConstrainedVerdictV1::RefutedBeaten,
                                    "admitted lane {lane} is not the rule's answer and survived the rule's answer"
                                );
                            }
                            if !set.admits(lane) {
                                assert_eq!(
                                    check_constrained_decode_token_v1(lane, row[lane], picked, row[picked], &s, 2, set.predicate()),
                                    PalwConstrainedVerdictV1::RefutedNotAdmitted
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// The bitmap's own edges: width that is not a multiple of 64, out-of-range lanes, and the
    /// `all`/`none` extremes. A set that silently admitted a lane past the vocabulary would let a
    /// class commit a token no tokenizer has.
    #[test]
    fn the_admitted_set_is_exact_at_its_edges() {
        for lanes in [0usize, 1, 63, 64, 65, 129] {
            let none = PalwAdmittedSetV1::none(lanes);
            assert_eq!(none.lanes(), lanes);
            assert_eq!(none.count(), 0);
            let all = PalwAdmittedSetV1::all(lanes);
            assert_eq!(all.count(), lanes, "all() must set exactly the lanes that exist, not the whole last word");
            for lane in 0..lanes {
                assert!(all.admits(lane));
                assert!(!none.admits(lane));
            }
            // Out of range on both, and a set built naming out-of-range lanes stays empty.
            assert!(!all.admits(lanes));
            assert!(!all.admits(usize::MAX));
            assert_eq!(PalwAdmittedSetV1::from_lanes(lanes, [lanes, lanes + 1, usize::MAX]).count(), 0);
        }
        let odd = PalwAdmittedSetV1::from_lanes(70, [0usize, 63, 64, 69]);
        assert_eq!(odd.count(), 4);
        for lane in [0usize, 63, 64, 69] {
            assert!(odd.admits(lane));
        }
        for lane in [1usize, 62, 65, 68, 70, 71] {
            assert!(!odd.admits(lane));
        }
    }
}
