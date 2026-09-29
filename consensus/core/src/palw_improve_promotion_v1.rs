//! **RFC-0004 §7.5 / spec 17 §17.9: promotion — the pinned sign table.**
//!
//! `k(m, a)` is the smallest `k ∈ 0..=m+1` with `P[Bin(m, ½) ≥ k] ≤ a`. At the level `a = α/(1000·K)`
//! that is `1000·K·Σ_{i=k}^{m} C(m, i) ≤ α_permille·2^m`, decided on exact integers — no float
//! anywhere. `k = m + 1` is the sentinel: no count attains the level, and the test cannot reject
//! [P2].
//!
//! The table is a network constant over `m ∈ 0..=2048`, `K ∈ 1..=64` and `α ∈ {10, 50}` permille
//! [P1]. Its byte form is hashed into `sign_table_id`, and the digest is pinned
//! ([`PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1`]); the test that recomputes the whole table holds the pin. A
//! lookup computes its one entry from the definition — one binomial row, `O(m²/64)` limb operations —
//! and the digest is what binds every entry to every other implementation.

use crate::Hash64;

/// The largest `m` the table covers — and so the largest `n` a policy may draw (spec 17 §17.4.3).
pub const PALW_IMPROVE_SIGN_N_MAX_V1: u32 = 2048;
/// The largest `K` the table covers — and so the largest `k_max` a policy may name.
pub const PALW_IMPROVE_SIGN_K_MAX_V1: u32 = 64;
/// The table's levels, in permille, in their byte-form order. A policy's `α` is one of them.
pub const PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1: [u16; 2] = [10, 50];
/// The byte form's tag.
pub const PALW_IMPROVE_SIGN_TABLE_TAG_V1: &[u8] = b"PALW-IMPROVE-SIGN-TABLE-V1";
/// **The pinned table's digest**, `H("misaka-palw/improve/sign-table/v1", byte form)`, hex. Recomputed
/// by `the_pinned_sign_table_is_the_definition` (a table that moves moves this, and the fence's
/// `sign_table_id` with it).
pub const PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1: &str =
    "ab9f6b108a470d0122c6a5ee3dea479913c966033425ef29cef03b2008b983cfd529060e622fa9fb2115d2957daa6176cb8cf7b78c40777c2c7b556ec3a82981";
/// The fewest anchor pairs a judge needs to be kept for an epoch (spec 17 §17.9.3) [S4].
pub const PALW_IMPROVE_MIN_ANCHOR_PAIRS_V1: u32 = 8;

/// Is `alpha_permille` one of the table's levels?
pub fn palw_improve_sign_alpha_is_tabled_v1(alpha_permille: u16) -> bool {
    PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1.contains(&alpha_permille)
}

// ---- exact unsigned integers, just enough for binomial tails ----

/// Little-endian 64-bit limbs, no trailing zero limb (zero is the empty vector).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Nat(Vec<u64>);

impl Nat {
    fn from_u64(v: u64) -> Self {
        let mut n = Nat(vec![v]);
        n.trim();
        n
    }

    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    /// `2^bits`.
    fn pow2(bits: u32) -> Self {
        let mut limbs = vec![0u64; (bits / 64) as usize + 1];
        limbs[(bits / 64) as usize] = 1u64 << (bits % 64);
        Nat(limbs)
    }

    fn add_assign(&mut self, other: &Nat) {
        if self.0.len() < other.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        let mut carry = 0u64;
        for i in 0..self.0.len() {
            let b = other.0.get(i).copied().unwrap_or(0);
            let (s1, c1) = self.0[i].overflowing_add(b);
            let (s2, c2) = s1.overflowing_add(carry);
            self.0[i] = s2;
            carry = (c1 as u64) + (c2 as u64);
        }
        if carry != 0 {
            self.0.push(carry);
        }
    }

    fn mul_small(&self, m: u64) -> Nat {
        let mut out = Vec::with_capacity(self.0.len() + 1);
        let mut carry = 0u128;
        for &limb in &self.0 {
            let prod = limb as u128 * m as u128 + carry;
            out.push(prod as u64);
            carry = prod >> 64;
        }
        if carry != 0 {
            out.push(carry as u64);
        }
        let mut n = Nat(out);
        n.trim();
        n
    }

    /// Exact division by a small divisor (the caller knows it divides).
    fn div_small_exact(&self, d: u64) -> Nat {
        let mut out = vec![0u64; self.0.len()];
        let mut rem = 0u128;
        for i in (0..self.0.len()).rev() {
            let cur = (rem << 64) | self.0[i] as u128;
            out[i] = (cur / d as u128) as u64;
            rem = cur % d as u128;
        }
        debug_assert_eq!(rem, 0, "an exact division");
        let mut n = Nat(out);
        n.trim();
        n
    }

    fn cmp(&self, other: &Nat) -> std::cmp::Ordering {
        if self.0.len() != other.0.len() {
            return self.0.len().cmp(&other.0.len());
        }
        for i in (0..self.0.len()).rev() {
            if self.0[i] != other.0[i] {
                return self.0[i].cmp(&other.0[i]);
            }
        }
        std::cmp::Ordering::Equal
    }
}

/// The upper tails of row `m`: `tails[k] = Σ_{i=k}^{m} C(m, i)` for `k ∈ 0..=m+1` (`tails[m+1] = 0`).
fn binomial_tails(m: u32) -> Vec<Nat> {
    let m = m as u64;
    let mut row = Vec::with_capacity(m as usize + 1);
    let mut c = Nat::from_u64(1);
    row.push(c.clone());
    for i in 0..m {
        c = c.mul_small(m - i).div_small_exact(i + 1);
        row.push(c.clone());
    }
    let mut tails = vec![Nat(Vec::new()); m as usize + 2];
    for k in (0..=m as usize).rev() {
        let mut t = tails[k + 1].clone();
        t.add_assign(&row[k]);
        tails[k] = t;
    }
    tails
}

/// The smallest `k` with `1000·K·tails[k] ≤ α·2^m`, by bisection over the decreasing tails.
fn critical_from_tails(m: u32, tails: &[Nat], alpha_permille: u16, k_count: u32) -> u32 {
    let rhs = Nat::pow2(m).mul_small(alpha_permille as u64);
    let scale = 1000u64 * k_count as u64;
    let passes = |k: usize| tails[k].mul_small(scale).cmp(&rhs) != std::cmp::Ordering::Greater;
    // `passes(m + 1)` holds (the empty tail); find the first k that passes.
    let (mut lo, mut hi) = (0usize, m as usize + 1);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if passes(mid) { hi = mid } else { lo = mid + 1 }
    }
    lo as u32
}

/// **`k(m, α/K)`** — the pinned table's entry, computed from the definition. `None` outside the
/// table's domain (`m > 2048`, `K ∉ 1..=64`, `α` not a tabled level).
pub fn palw_improve_sign_critical_v1(m: u32, alpha_permille: u16, k_count: u32) -> Option<u32> {
    if m > PALW_IMPROVE_SIGN_N_MAX_V1
        || k_count == 0
        || k_count > PALW_IMPROVE_SIGN_K_MAX_V1
        || !palw_improve_sign_alpha_is_tabled_v1(alpha_permille)
    {
        return None;
    }
    Some(critical_from_tails(m, &binomial_tails(m), alpha_permille, k_count))
}

/// **Does the one-sided sign test reject "no better" for `wins` against `losses`?** `wins ≥ k(wins +
/// losses, α/K)` (spec 17 §17.9.4 rule 3). `None` outside the table's domain.
pub fn palw_improve_sign_test_rejects_v1(wins: u32, losses: u32, alpha_permille: u16, k_count: u32) -> Option<bool> {
    let m = wins.checked_add(losses)?;
    palw_improve_sign_critical_v1(m, alpha_permille, k_count).map(|k| wins >= k)
}

/// **The table's byte form** (spec 17 §17.9.2): the tag, the dimensions, the levels, then every
/// `LE u32 k(m, α/K)` for each `α` in order, each `K` ascending, each `m` ascending.
pub fn palw_improve_sign_table_bytes_v1() -> Vec<u8> {
    let n_max = PALW_IMPROVE_SIGN_N_MAX_V1;
    let k_max = PALW_IMPROVE_SIGN_K_MAX_V1;
    let alphas = PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1;
    let mut out = Vec::with_capacity(64 + 4 * alphas.len() * k_max as usize * (n_max as usize + 1));
    out.extend_from_slice(PALW_IMPROVE_SIGN_TABLE_TAG_V1);
    out.extend_from_slice(&(n_max as u16).to_le_bytes());
    out.push(k_max as u8);
    out.push(alphas.len() as u8);
    for alpha in alphas {
        out.extend_from_slice(&alpha.to_le_bytes());
    }
    // Entries are grouped α-major, then K, then m; compute each row's tails once.
    let mut table = vec![0u32; alphas.len() * k_max as usize * (n_max as usize + 1)];
    for m in 0..=n_max {
        let tails = binomial_tails(m);
        for (a, alpha) in alphas.iter().enumerate() {
            for k_count in 1..=k_max {
                let index = (a * k_max as usize + (k_count as usize - 1)) * (n_max as usize + 1) + m as usize;
                table[index] = critical_from_tails(m, &tails, *alpha, k_count);
            }
        }
    }
    for entry in table {
        out.extend_from_slice(&entry.to_le_bytes());
    }
    out
}

/// The digest of [`palw_improve_sign_table_bytes_v1`] under `misaka-palw/improve/sign-table/v1`.
/// Computes the whole table: tests and vectors only (the fence reads the pin).
pub fn palw_improve_sign_table_digest_v1() -> Hash64 {
    let bytes = palw_improve_sign_table_bytes_v1();
    let mut state =
        blake2b_simd::Params::new().hash_length(64).key(crate::palw_improve_v1::PALW_IMPROVE_SIGN_TABLE_DOMAIN_V1).to_state();
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---- counts and the decision (spec 17 §17.9) ----

use crate::palw_improve_state_v1::{
    PalwEvalItemV1, PalwEvalSubjectV1, PalwItemOutcomeV1, PalwItemSourceV1, PalwNoChangeReasonV1, PalwPairedCountsV1,
    PalwPromotionCountsV1, PalwPromotionOutcomeV1, PalwScoringKindV1,
};

/// **One item's outcome** for a subject against the parent (spec 17 §17.9.1): a missing score counts for
/// the incumbent — the parent's missing is a Loss, the subject's missing is a Loss. ExactMatch,
/// RefLogLik and Judge compare the two scores; Pairwise reads the subject's recorded outcome (+1 Win,
/// −1 Loss, 0 Tie; the stage applied the order and the margin) and ignores `parent`.
pub fn palw_improve_item_outcome_v1(kind: PalwScoringKindV1, parent: Option<i64>, subject: Option<i64>) -> PalwItemOutcomeV1 {
    use std::cmp::Ordering::*;
    if kind == PalwScoringKindV1::Pairwise {
        return match subject {
            Some(1) => PalwItemOutcomeV1::Win,
            Some(0) => PalwItemOutcomeV1::Tie,
            _ => PalwItemOutcomeV1::Loss,
        };
    }
    let (Some(p), Some(c)) = (parent, subject) else { return PalwItemOutcomeV1::Loss };
    match c.cmp(&p) {
        Greater => PalwItemOutcomeV1::Win,
        Less => PalwItemOutcomeV1::Loss,
        Equal => PalwItemOutcomeV1::Tie,
    }
}

/// The recorded scores an item's count reads, one lookup per `(item, subject, kind)`.
pub trait PalwImproveScoresV1 {
    fn score(&self, item: u32, subject: &PalwEvalSubjectV1, kind: PalwScoringKindV1) -> Option<i64>;
    /// Does any subject have a score of `kind` on `item`?
    fn any_score(&self, item: u32, kind: PalwScoringKindV1) -> bool;
}

/// **An item's primary kind** (spec 17 §17.9.1): the kind of a primary score any subject recorded on
/// it (ExactMatch first); for an item with none, `Some(ExactMatch)` unless a guard score was recorded
/// (a judged-only item, `None`) — so an item nobody evaluated counts, and counts for the parent.
pub fn palw_improve_item_primary_v1<S: PalwImproveScoresV1>(scores: &S, item: u32) -> Option<PalwScoringKindV1> {
    if scores.any_score(item, PalwScoringKindV1::ExactMatch) {
        Some(PalwScoringKindV1::ExactMatch)
    } else if scores.any_score(item, PalwScoringKindV1::RefLogLik) {
        Some(PalwScoringKindV1::RefLogLik)
    } else if scores.any_score(item, PalwScoringKindV1::Judge) || scores.any_score(item, PalwScoringKindV1::Pairwise) {
        None
    } else {
        Some(PalwScoringKindV1::ExactMatch)
    }
}

/// **Is a judge excluded for the epoch** (spec 17 §17.9.3) [S4]? Over the drawn exact-match items it
/// judged, every ordered pair `(p, f)` of subjects where `p` passed and `f` failed is an anchor pair; it
/// is correct when the judge scored `p` above `f`. Excluded with fewer than 8 pairs, or when
/// `1000·correct < floor·pairs`.
pub fn palw_improve_judge_excluded_v1<S: PalwImproveScoresV1>(
    scores: &S,
    items: &[PalwEvalItemV1],
    subjects: &[PalwEvalSubjectV1],
    judge: &crate::Hash64,
    anchor_floor_permille: u16,
) -> bool {
    let (mut pairs, mut correct) = (0u64, 0u64);
    for item in items.iter().filter(|i| !i.dropped && i.judge.as_ref() == Some(judge)) {
        if palw_improve_item_primary_v1(scores, item.item) != Some(PalwScoringKindV1::ExactMatch) {
            continue;
        }
        let rows: Vec<(bool, i64)> = subjects
            .iter()
            .filter_map(|s| {
                let pass = scores.score(item.item, s, PalwScoringKindV1::ExactMatch)?;
                let judged = scores.score(item.item, s, PalwScoringKindV1::Judge)?;
                Some((pass == 1, judged))
            })
            .collect();
        for (p_pass, p_score) in &rows {
            for (f_pass, f_score) in &rows {
                if *p_pass && !*f_pass {
                    pairs += 1;
                    correct += (p_score > f_score) as u64;
                }
            }
        }
    }
    pairs < PALW_IMPROVE_MIN_ANCHOR_PAIRS_V1 as u64 || 1000 * correct < anchor_floor_permille as u64 * pairs
}

/// What the rule reads of the policy.
#[derive(Clone, Copy, Debug)]
pub struct PalwImproveRuleV1 {
    pub n_min: u32,
    pub delta_permille: u16,
    pub epsilon_permille: u16,
    pub epsilon_safety_permille: u16,
    pub alpha_permille: u16,
    pub has_judge: bool,
    pub has_pairwise: bool,
}

/// **A subject's counts** (spec 17 §17.9.3) against the parent, over the epoch's items; `excluded`
/// answers whether a judge failed its anchors.
pub fn palw_improve_counts_v1<S: PalwImproveScoresV1>(
    scores: &S,
    items: &[PalwEvalItemV1],
    subject: &PalwEvalSubjectV1,
    rule: &PalwImproveRuleV1,
    excluded: &dyn Fn(&crate::Hash64) -> bool,
) -> PalwPromotionCountsV1 {
    let parent = PalwEvalSubjectV1::Parent;
    let mut counts = PalwPromotionCountsV1::default();
    for item in items.iter().filter(|i| !i.dropped) {
        let i = item.item;
        let primary = palw_improve_item_primary_v1(scores, i);
        let outcome = |kind: PalwScoringKindV1| {
            palw_improve_item_outcome_v1(kind, scores.score(i, &parent, kind), scores.score(i, subject, kind))
        };
        match item.source {
            PalwItemSourceV1::HoldOut | PalwItemSourceV1::Setter { .. } => {
                if let Some(kind) = primary {
                    counts.primary.add(outcome(kind));
                }
                let judge_kept = item.judge.as_ref().is_some_and(|j| !excluded(j));
                if rule.has_judge && judge_kept {
                    counts.judge.add(outcome(PalwScoringKindV1::Judge));
                }
                if rule.has_pairwise && judge_kept {
                    counts.pairwise.add(outcome(PalwScoringKindV1::Pairwise));
                }
            }
            PalwItemSourceV1::Regression { .. } => counts.regression.add(outcome(primary.unwrap_or(PalwScoringKindV1::ExactMatch))),
            PalwItemSourceV1::Safety { .. } => counts.safety.add(outcome(primary.unwrap_or(PalwScoringKindV1::ExactMatch))),
        }
    }
    counts
}

fn suite_ok(c: &PalwPairedCountsV1, epsilon_permille: u16) -> bool {
    let net_loss = c.losses as i64 - c.wins as i64;
    1000 * net_loss <= epsilon_permille as i64 * c.total() as i64
}

/// A guard fails when the parent's wins are significant: `c ≥ k(b + c, α/K)`.
fn guard_fails(c: &PalwPairedCountsV1, alpha_permille: u16, k_count: u32) -> bool {
    palw_improve_sign_test_rejects_v1(c.losses, c.wins, alpha_permille, k_count).unwrap_or(true)
}

/// **Eligibility** (spec 17 §17.9.4), rules 1–7, at `K = k_count`. Outside the table's domain (`n`
/// past 2048, which the policy check forbids) the subject is not eligible.
pub fn palw_improve_eligible_v1(c: &PalwPromotionCountsV1, rule: &PalwImproveRuleV1, k_count: u32) -> bool {
    let p = &c.primary;
    let n = p.total();
    n >= rule.n_min
        && 1000 * (p.wins as i64 - p.losses as i64) >= rule.delta_permille as i64 * n as i64
        && palw_improve_sign_test_rejects_v1(p.wins, p.losses, rule.alpha_permille, k_count).unwrap_or(false)
        && suite_ok(&c.regression, rule.epsilon_permille)
        && suite_ok(&c.safety, rule.epsilon_safety_permille)
        && !(rule.has_judge && guard_fails(&c.judge, rule.alpha_permille, k_count))
        && !(rule.has_pairwise && guard_fails(&c.pairwise, rule.alpha_permille, k_count))
}

/// **The decision** (spec 17 §17.9.4–5) over the frozen candidate set in acceptance order, each with
/// its counts (eligibility filled in): the eligible candidate with the largest `b − c`, ties to the
/// earliest; `TooFewItems` when the primary count is under `n_min` (the same `n` for every candidate);
/// `NoneEligible` otherwise.
pub fn palw_improve_decide_v1(
    candidates: &[(crate::Hash64, PalwPromotionCountsV1)],
    rule: &PalwImproveRuleV1,
) -> PalwPromotionOutcomeV1 {
    let Some((_, first)) = candidates.first() else {
        return PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::NoCandidate };
    };
    if first.primary.total() < rule.n_min {
        return PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::TooFewItems };
    }
    let mut best: Option<(crate::Hash64, i64, &PalwPromotionCountsV1)> = None;
    for (class_id, counts) in candidates.iter().filter(|(_, c)| c.eligible) {
        let margin = counts.primary.wins as i64 - counts.primary.losses as i64;
        if best.is_none_or(|(_, m, _)| margin > m) {
            best = Some((*class_id, margin, counts));
        }
    }
    match best {
        Some((class_id, _, counts)) => {
            PalwPromotionOutcomeV1::Promoted { class_id, wins: counts.primary.wins, losses: counts.primary.losses }
        }
        None => PalwPromotionOutcomeV1::NoChange { reason: PalwNoChangeReasonV1::NoneEligible },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A brute force over `u128` for small `m`: `P[Bin(m,½) ≥ k] ≤ α/(1000K)`.
    fn brute(m: u32, alpha: u16, k_count: u32) -> u32 {
        let mut c = vec![1u128; 1];
        for i in 0..m as u128 {
            let next = c[i as usize] * (m as u128 - i) / (i + 1);
            c.push(next);
        }
        for k in 0..=m + 1 {
            let tail: u128 = (k..=m).map(|i| c[i as usize]).sum();
            if 1000 * k_count as u128 * tail <= alpha as u128 * (1u128 << m) {
                return k;
            }
        }
        unreachable!("k = m + 1 always passes")
    }

    #[test]
    fn the_exact_critical_values_match_a_brute_force() {
        for m in 0..=100u32 {
            for alpha in PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1 {
                for k_count in [1u32, 2, 3, 4, 8, 64] {
                    assert_eq!(
                        palw_improve_sign_critical_v1(m, alpha, k_count),
                        Some(brute(m, alpha, k_count)),
                        "m {m} α {alpha} K {k_count}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_sentinel_and_the_domain() {
        // m = 0..4 at 1 %: no count attains 1 % (even 4 of 4 is 1/16), so k = m + 1.
        for m in 0..=4 {
            assert_eq!(palw_improve_sign_critical_v1(m, 10, 1), Some(m + 1));
        }
        // At 5 % and K = 1: 5 of 5 is 1/32 < 5 %.
        assert_eq!(palw_improve_sign_critical_v1(5, 50, 1), Some(5));
        assert_eq!(palw_improve_sign_critical_v1(2049, 50, 1), None);
        assert_eq!(palw_improve_sign_critical_v1(10, 50, 0), None);
        assert_eq!(palw_improve_sign_critical_v1(10, 50, 65), None);
        assert_eq!(palw_improve_sign_critical_v1(10, 25, 1), None, "an untabled level");
        assert_eq!(palw_improve_sign_test_rejects_v1(5, 0, 50, 1), Some(true));
        assert_eq!(palw_improve_sign_test_rejects_v1(4, 0, 50, 1), Some(false));
    }

    #[test]
    fn k_never_decreases_in_m_and_moves_by_at_most_one() {
        for alpha in PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1 {
            for k_count in [1u32, 4, 64] {
                let mut last = palw_improve_sign_critical_v1(0, alpha, k_count).unwrap();
                for m in 1..=300u32 {
                    let k = palw_improve_sign_critical_v1(m, alpha, k_count).unwrap();
                    assert!(k >= last && k <= last + 1, "m {m} α {alpha} K {k_count}: {last} → {k}");
                    last = k;
                }
            }
        }
    }

    #[test]
    fn a_larger_k_never_lowers_the_bar() {
        for m in [10u32, 50, 200, 2048] {
            for alpha in PALW_IMPROVE_SIGN_ALPHAS_PERMILLE_V1 {
                let mut last = 0;
                for k_count in 1..=64 {
                    let k = palw_improve_sign_critical_v1(m, alpha, k_count).unwrap();
                    assert!(k >= last, "m {m} α {alpha}: K {k_count}");
                    last = k;
                }
            }
        }
    }
}

#[cfg(test)]
mod pin {
    use super::*;

    /// **The pin is the table** (spec 17 §17.9.2): recompute every entry from the definition, hash the
    /// byte form, and compare with [`PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1`]. The byte form is 262,272
    /// entries behind a 36-byte header.
    #[test]
    fn the_pinned_sign_table_is_the_definition() {
        let bytes = palw_improve_sign_table_bytes_v1();
        assert_eq!(bytes.len(), PALW_IMPROVE_SIGN_TABLE_TAG_V1.len() + 2 + 1 + 1 + 2 * 2 + 4 * 2 * 64 * 2049);
        let digest = palw_improve_sign_table_digest_v1();
        println!("REPIN improve.sign_table_id {digest}");
        assert_eq!(digest.to_string(), PALW_IMPROVE_SIGN_TABLE_ID_HEX_V1, "the sign table moved: re-pin it and say why");
    }
}
