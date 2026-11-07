//! **RFC-0004 §0 / §2.2: evaluation evidence carries its assurance, and promotion adds the errors up.**
//!
//! A `V` result is "accepted under the declared assurance mode" — an exact fold/court result or a
//! probabilistic constraint-verified claim — and must say which, with the kernel and the bound. The
//! promotion's computational false-acceptance budget for `M` evaluation claims is the union bound
//! `Σ ε_i`, conditional on each suite's assumptions. It is **not** the sign test's significance (§7.5),
//! and the common Panel/randomness/DA failure events are added separately, never multiplied away.

use crate::hash::Digest;

/// RFC-0004 §2.2's trust labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrustLabelV1 {
    Verified,
    Bonded,
    Judged,
    Trusted,
}

/// How a `V` result was accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssuranceModeV1 {
    /// Integer arithmetic of the fold itself: no computational error term.
    ExactFold,
    /// A bounded exact court's verdict.
    ExactCourt,
    /// A legacy profile's own rule (seat replay quorum): its error is not this crate's to state.
    Legacy,
    /// Constraint-verified under a kernel suite with a derived conditional bound `2^-error_bits`.
    Probabilistic { descriptor: Digest, error_bits: u16 },
}

/// One evaluation result as promotion consumes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationEvidenceLabelV1 {
    pub claim_id: Digest,
    pub label: TrustLabelV1,
    pub mode: AssuranceModeV1,
}

/// The promotion's computational budget, as a bit count: `−log2 Σ ε_i`, floored, over the
/// probabilistic results; `None` when a result rests on a mode with no stated computational bound
/// (`Legacy`) or is not a `V` result at all — the caller must then report that dependency instead of
/// a number.
pub fn promotion_error_bits_v1(results: &[EvaluationEvidenceLabelV1]) -> Option<u16> {
    let mut min_bits: Option<u16> = None;
    let mut count: u128 = 0;
    for r in results {
        if r.label != TrustLabelV1::Verified {
            return None;
        }
        match r.mode {
            AssuranceModeV1::ExactFold | AssuranceModeV1::ExactCourt => {}
            AssuranceModeV1::Legacy => return None,
            AssuranceModeV1::Probabilistic { error_bits, .. } => {
                count += 1;
                min_bits = Some(min_bits.map_or(error_bits, |m| m.min(error_bits)));
            }
        }
    }
    match min_bits {
        // Σ ε_i ≤ M · max ε_i: bits ≥ min_bits − ⌈log2 M⌉.
        Some(b) => {
            let log2_up = 128 - (count - 1).leading_zeros() as i64;
            Some((b as i64 - log2_up).clamp(0, u16::MAX as i64) as u16)
        }
        None => Some(u16::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(mode: AssuranceModeV1) -> EvaluationEvidenceLabelV1 {
        EvaluationEvidenceLabelV1 { claim_id: [0; 64], label: TrustLabelV1::Verified, mode }
    }

    #[test]
    fn a_union_bound_over_probabilistic_results_and_no_number_for_a_legacy_or_judged_one() {
        let p = AssuranceModeV1::Probabilistic { descriptor: [1; 64], error_bits: 200 };
        assert_eq!(promotion_error_bits_v1(&[v(p); 4]), Some(198), "4 results: 2^-200 · 4 = 2^-198");
        assert_eq!(promotion_error_bits_v1(&[v(p), v(AssuranceModeV1::ExactFold)]), Some(200));
        assert_eq!(promotion_error_bits_v1(&[v(AssuranceModeV1::ExactCourt)]), Some(u16::MAX), "exact only: no computational term");
        assert_eq!(promotion_error_bits_v1(&[v(p), v(AssuranceModeV1::Legacy)]), None);
        let judged = EvaluationEvidenceLabelV1 { label: TrustLabelV1::Judged, ..v(p) };
        assert_eq!(promotion_error_bits_v1(&[judged]), None, "a judge's opinion is J even when its execution is valid");
    }
}
