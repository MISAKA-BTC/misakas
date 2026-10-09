//! **The composed false-accept bound** — the executable half of the external soundness review dossier
//! (`docs/design/palw/soundness-review-dossier/05-composition.md`).
//!
//! A pure, integer, conservative calculator. It reads no chain state, decides nothing, and **no consensus or policy code calls it**:
//! it exists so that an external reviewer can recompute every number the dossier states, and so that `tests/composition.rs` pins
//! those numbers. Every quantity is a whole-bit (or exact-integer) bound and every rounding favours the adversary: a security bit
//! count is floored, a loss is ceiled.
//!
//! ```text
//! P_FA ≤ ε_sel + Q·(R+1)·G^β · Σ_f R_f·2^(−t_f·b_f) + ε_bind
//!
//! effective = min(check − loss, binding, selection) − ⌈log2 #terms⌉           (a union of the present terms)
//!   check     = min_f (t_f·b_f) − ⌈log2 Σ_f R_f⌉       [UnionBound]   or   min_f (t_f·b_f)   [SingleFalseInstance]
//!   loss      = ⌈log2 (R+1)⌉ + β·g + ⌈log2 Q⌉          (g = ⌈log2 G⌉, the adversary's grinding bits per beacon)
//!   selection = ⌊log2 (N / (N − s))⌋                   (a verifier that checks s of N units; absent when s = N)
//! ```
//!
//! With one family, `UnionBound`, a binding term, complete coverage and no adversary budget this is exactly the kernel's
//! `plan::derived_error_bits` (`min(t·b − ⌈log2 R⌉, binding) − 1`), so the 9B-8k figures of `coverage-p1p2-record.md` (K2-TIR-v1
//! 2^-226, K2-TIR-v2 2^-150) are reproduced by the tests. What the formula assumes — a uniform seed independent of the committed
//! statement, at most `G` adversary-reachable seeds per beacon, a binding commitment, and who checks what — is the dossier's §2;
//! a number from this module is never stronger than those assumptions.

/// Why the calculator refuses its inputs (a malformed question, never a security verdict).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompositionRefusalV1 {
    #[error("a probabilistic family with no instance or no repetition")]
    EmptyFamily,
    #[error("beacons per attempt must be at least 1 (1 = non-interactive)")]
    NoBeacon,
    #[error("adaptive statements must be at least 1")]
    NoStatement,
    #[error("a sampled coverage of {checked} of {units} units")]
    Coverage { units: u128, checked: u128 },
}

/// One probabilistic relation family of a statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckFamilyV1 {
    /// `b_f`: the bits one repetition buys against one fixed false instance — `⌊log2 q⌋` of the smallest modulus the family may
    /// use (126 for `2^127 − 1`, 106 for `2^107 − 1`, 88 for `2^89 − 1`), or a sampled scope's bits per repetition.
    pub per_repetition_bits: u32,
    /// `t_f`: independent repetitions, each with its own uniform vector.
    pub repetitions: u32,
    /// `R_f`: the family's checked instances in the statement.
    pub instances: u128,
}

/// How the per-instance errors of one statement are combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstanceCompositionV1 {
    /// `Σ_f R_f · 2^(−t_f·b_f)` — what the kernel derives today (conservative; needs no independence).
    UnionBound,
    /// `max_f 2^(−t_f·b_f)`: a committed false statement has a fixed false instance, and it must pass its own check. Valid only for
    /// commit-then-challenge, non-aggregated checks (dossier reviewer question Q-04); no policy may use it before that review.
    SingleFalseInstance,
}

/// What the adversary may do around the challenge (dossier §2.4, §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdversaryBudgetV1 {
    /// `R`: counted retries after the first attempt (the policy's `retry_limit`).
    pub retry_limit: u32,
    /// `g = ⌈log2 G⌉`: distinct beacon outputs the adversary can choose among, per beacon (0 = none).
    pub grinding_bits_per_beacon: u32,
    /// `β`: beacons one attempt consumes (1 non-interactive; the round count for a staged beacon).
    pub beacons_per_attempt: u32,
    /// `Q`: statements the adversary may try over the bound's horizon (1 = this statement only).
    pub adaptive_statements: u128,
}

impl AdversaryBudgetV1 {
    /// No retry, no grinding, one beacon, one statement: the bound for a uniformly random seed.
    pub const NONE: Self = Self { retry_limit: 0, grinding_bits_per_beacon: 0, beacons_per_attempt: 1, adaptive_statements: 1 };

    /// `⌈log2 (R+1)⌉ + β·g + ⌈log2 Q⌉`.
    pub fn loss_bits(&self) -> u64 {
        ceil_log2_v1(self.retry_limit as u128 + 1) as u64
            + self.beacons_per_attempt as u64 * self.grinding_bits_per_beacon as u64
            + ceil_log2_v1(self.adaptive_statements) as u64
    }
}

/// Which part of the statement an honest verifier actually checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverageV1 {
    /// Every unit (every relation of every position): no selection term.
    Complete,
    /// `checked` distinct units of `units`, uniformly, unpredictably to the producer: a single false unit is missed with
    /// probability exactly `(units − checked) / units`.
    Sampled { units: u128, checked: u128 },
}

/// The composed bound and its parts (bits are `−log2` of the respective error terms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComposedBoundV1 {
    /// `None`: the statement has no probabilistic family (exact checks only).
    pub check_bits: Option<i64>,
    pub loss_bits: u64,
    pub binding_bits: Option<u32>,
    /// `None`: complete coverage.
    pub selection_bits: Option<u32>,
    /// `−log2` of the composed bound, floored. `≤ 0` means no security from this composition. `None`: no term at all.
    pub effective_bits: Option<i64>,
}

/// `⌈log2 x⌉` (0 for `x ≤ 1`).
pub const fn ceil_log2_v1(x: u128) -> u32 {
    if x <= 1 { 0 } else { 128 - (x - 1).leading_zeros() }
}

/// `P(c, k) = c! / (c − k)!` — ordered choices of `k` of `c` (OPV-BOOT's beacon output-selection count); `None` on overflow or
/// `k > c`.
pub fn ordered_choices_v1(c: u128, k: u32) -> Option<u128> {
    if k as u128 > c {
        return None;
    }
    (0..k as u128).try_fold(1u128, |acc, i| acc.checked_mul(c - i))
}

/// **The selection term**: `⌊log2 (N / (N − s))⌋` for a verifier checking `s` of `N` units; `None` when `s = N` (no selection loss).
/// The largest `k` with `(N − s)·2^k ≤ N`, computed exactly.
pub fn selection_bits_v1(units: u128, checked: u128) -> Result<Option<u32>, CompositionRefusalV1> {
    if units == 0 || checked > units {
        return Err(CompositionRefusalV1::Coverage { units, checked });
    }
    if checked == units {
        return Ok(None);
    }
    let missed = units - checked;
    let mut k = 0u32;
    while k + 1 < 128 && missed <= units >> (k + 1) {
        k += 1;
    }
    Ok(Some(k))
}

/// **A sampled scope's bits**, `⌊n · f_ppm · 14,426 / 10^10⌋` — the lower bound `−log2 (1 − f)^n ≥ n·f·log2 e` with
/// `log2 e > 1.4426`. The same integer formula as `kaspa-consensus-core` `ConformanceScopeV1::derived_epsilon_bits` (mirrored, not
/// imported: consensus depends on this crate).
pub fn sampled_scope_bits_v1(draws: u128, fault_ppm: u32) -> u128 {
    draws.saturating_mul(fault_ppm as u128).saturating_mul(14_426) / 10_000_000_000
}

/// The fewest draws whose [`sampled_scope_bits_v1`] reaches `target_bits` under a fault density of `fault_ppm` (`None`: `0` ppm).
pub fn draws_for_bits_v1(target_bits: u128, fault_ppm: u32) -> Option<u128> {
    if fault_ppm == 0 {
        return None;
    }
    let per = fault_ppm as u128 * 14_426;
    Some(target_bits.checked_mul(10_000_000_000)?.div_ceil(per))
}

/// **The composed false-accept bound** of one statement (see the module docs for the formula).
pub fn composed_false_accept_bits_v1(
    families: &[CheckFamilyV1],
    composition: InstanceCompositionV1,
    binding_bits: Option<u32>,
    coverage: CoverageV1,
    adversary: &AdversaryBudgetV1,
) -> Result<ComposedBoundV1, CompositionRefusalV1> {
    if families.iter().any(|f| f.instances == 0 || f.repetitions == 0) {
        return Err(CompositionRefusalV1::EmptyFamily);
    }
    if adversary.beacons_per_attempt == 0 {
        return Err(CompositionRefusalV1::NoBeacon);
    }
    if adversary.adaptive_statements == 0 {
        return Err(CompositionRefusalV1::NoStatement);
    }
    let check_bits = families.iter().map(|f| f.per_repetition_bits as i64 * f.repetitions as i64).min().map(|min_tb| {
        let total = families.iter().fold(0u128, |acc, f| acc.saturating_add(f.instances));
        match composition {
            InstanceCompositionV1::UnionBound => min_tb - ceil_log2_v1(total) as i64,
            InstanceCompositionV1::SingleFalseInstance => min_tb,
        }
    });
    let selection_bits = match coverage {
        CoverageV1::Complete => None,
        CoverageV1::Sampled { units, checked } => selection_bits_v1(units, checked)?,
    };
    let loss_bits = adversary.loss_bits();
    let terms: Vec<i64> = [check_bits.map(|c| c - loss_bits as i64), binding_bits.map(i64::from), selection_bits.map(i64::from)]
        .into_iter()
        .flatten()
        .collect();
    let effective_bits = terms.iter().min().map(|m| m - ceil_log2_v1(terms.len() as u128) as i64);
    Ok(ComposedBoundV1 { check_bits, loss_bits, binding_bits, selection_bits, effective_bits })
}

/// The fewest repetitions `t ≤ 64` of one family (`b` bits each, `instances` instances, union bound) whose composed bound reaches
/// `target_bits` against `adversary`, with the given binding term and complete coverage; `None` if no `t ≤ 64` does.
pub fn min_repetitions_v1(
    per_repetition_bits: u32,
    instances: u128,
    binding_bits: Option<u32>,
    adversary: &AdversaryBudgetV1,
    target_bits: i64,
) -> Option<u32> {
    (1..=64).find(|&t| {
        let fam = [CheckFamilyV1 { per_repetition_bits, repetitions: t, instances }];
        composed_false_accept_bits_v1(&fam, InstanceCompositionV1::UnionBound, binding_bits, CoverageV1::Complete, adversary)
            .ok()
            .and_then(|b| b.effective_bits)
            .is_some_and(|e| e >= target_bits)
    })
}

/// **The deterrent reservation** of RFC-0015 §8.1 in the form G14-R4 gives it (`OpvPolicyV1::required_reservation` on branch
/// `g14/r4-fixes`), with the detection probability as an exact fraction `detect_num / detect_den` instead of permille, so that a
/// sampled coverage (`checked / units`, often below 1 ‰) can be priced:
///
/// `⌈ max(gain + default_penalty, ⌈gain · den / num⌉) · 1000 / (1000 − accuser_permille) ⌉`
///
/// `None`: a zero detection probability (no finite reservation deters) or an accuser share of 1000 ‰.
pub fn deterrent_reservation_v1(
    gain: u128,
    default_penalty: u128,
    detect_num: u128,
    detect_den: u128,
    accuser_permille: u16,
) -> Option<u128> {
    if detect_num == 0 || detect_den == 0 || detect_num > detect_den || accuser_permille >= 1000 {
        return None;
    }
    let by_detection = gain.checked_mul(detect_den)?.div_ceil(detect_num);
    let base = gain.checked_add(default_penalty)?.max(by_detection);
    Some(base.checked_mul(1000)?.div_ceil(1000 - accuser_permille as u128))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ceil_log2_and_ordered_choices_are_exact_at_their_edges() {
        assert_eq!([0, 1, 2, 3, 4, 5].map(ceil_log2_v1), [0, 0, 1, 2, 2, 3]);
        assert_eq!(ceil_log2_v1(1 << 127), 127);
        assert_eq!(ceil_log2_v1((1 << 127) + 1), 128);
        assert_eq!(ceil_log2_v1(u128::MAX), 128);
        assert_eq!(ordered_choices_v1(32, 2), Some(992));
        assert_eq!(ordered_choices_v1(3, 4), None);
        assert_eq!(ordered_choices_v1(5, 0), Some(1));
        assert_eq!(ordered_choices_v1(u128::MAX, 2), None);
    }

    #[test]
    fn the_selection_term_is_the_exact_floor() {
        assert_eq!(selection_bits_v1(8_192, 8_192), Ok(None));
        assert_eq!(selection_bits_v1(8_192, 8), Ok(Some(0)), "miss 8,184/8,192: under one bit");
        assert_eq!(selection_bits_v1(8_192, 4_096), Ok(Some(1)));
        assert_eq!(selection_bits_v1(8_192, 8_191), Ok(Some(13)));
        assert_eq!(selection_bits_v1(1, 0), Ok(Some(0)));
        assert!(selection_bits_v1(0, 0).is_err() && selection_bits_v1(4, 5).is_err());
    }

    #[test]
    fn malformed_questions_are_refused() {
        let f = |instances, repetitions| [CheckFamilyV1 { per_repetition_bits: 88, repetitions, instances }];
        let go = |fam: &[CheckFamilyV1], a: AdversaryBudgetV1| {
            composed_false_accept_bits_v1(fam, InstanceCompositionV1::UnionBound, Some(256), CoverageV1::Complete, &a)
        };
        assert_eq!(go(&f(0, 2), AdversaryBudgetV1::NONE), Err(CompositionRefusalV1::EmptyFamily));
        assert_eq!(go(&f(1, 0), AdversaryBudgetV1::NONE), Err(CompositionRefusalV1::EmptyFamily));
        assert_eq!(
            go(&f(1, 2), AdversaryBudgetV1 { beacons_per_attempt: 0, ..AdversaryBudgetV1::NONE }),
            Err(CompositionRefusalV1::NoBeacon)
        );
        assert_eq!(
            go(&f(1, 2), AdversaryBudgetV1 { adaptive_statements: 0, ..AdversaryBudgetV1::NONE }),
            Err(CompositionRefusalV1::NoStatement)
        );
        let none = composed_false_accept_bits_v1(
            &[],
            InstanceCompositionV1::UnionBound,
            None,
            CoverageV1::Complete,
            &AdversaryBudgetV1::NONE,
        );
        assert_eq!(none.unwrap().effective_bits, None, "exact-only, no binding term modelled: nothing to bound");
    }
}
