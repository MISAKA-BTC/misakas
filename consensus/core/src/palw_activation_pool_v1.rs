//! **The Activation Pool and the listing rules it rides on** (ADR-0152-adjacent: Activation Pool,
//! user decision 2026-09-25; the adversarial review's §4 "Minimal spec", its findings cited by id).
//!
//! A registration is a long-lived, asynchronous LISTING: nothing between a class's registration and
//! its first panel may be a deadline, and whoever objectively proves they prepared the model is paid
//! for it — never for the yes or no they then vote. `Params::palw_activation_pool` (`Some(0)` on
//! testnet-12 alone, genesis-only) arms three rules at once:
//!
//! * **R1 (the P1 fix):** silence reclamation (`apply_class_reclamation`) never reclaims a class
//!   whose registry row exists and does not admit claims — a `Candidate`, `Registered`,
//!   `Prefetching` or `Held` class cannot produce, so its silence is the network's absence, not the
//!   class's — nor a genesis row (no registrant bond), like the floor;
//! * **R2 (the review's M2):** the registry's span step skips the rows of `Dormant` and `Frozen`
//!   classes, and a `Candidate` row reads its ready seats and its jury only at its OWN staggered
//!   audit span, so the per-span cost of a listing does not grow with the listings nobody audits;
//! * **the pool itself**, in the later sections of this module.
//!
//! Below the fence — every network but testnet-12 — nothing here is reached, and every fold is
//! byte-identical to a build without this module.

use crate::Hash64;

/// **The pool's terms** — every number the rules below read, carried beside the fence in
/// `Params::palw_activation_pool` so a network states them once and every node folds one answer.
/// The defaults ([`PALW_ACTIVATION_POOL_TERMS_V1`]) are the user's illustrative scale of 2026-09-25;
/// they are the values to tune, not derivations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwActivationPoolTermsV1 {
    /// `A0`: the preparation reward's base per payee, in sompi. `A_MAX` ramps from `A0` at the
    /// pool's opening to `3·A0` after [`Self::ramp_daa`] (the waiting bonus).
    pub prep_base_sompi: u64,
    /// `α`: the preparation budget's share of every inflow, in permille; the rest is the bonus's.
    pub prep_share_permille: u16,
    /// `β`: the share of the bonus budget one activation event pays, in permille; the rest stays
    /// for a later re-formation and later sponsors.
    pub bonus_share_permille: u16,
    /// `W`: the DAA over which `A_MAX` ramps from `A0` to `3·A0` (5,040 DAA = 7 days at 120 s).
    pub ramp_daa: u64,
    /// The most operators one class's preparation reward is ever paid to (once each).
    pub prep_payee_cap: u16,
    /// The most operators one class's activation bonus is ever paid to (once each).
    pub bonus_payee_cap: u16,
    /// The least a top-up may add, in sompi.
    pub min_topup_sompi: u64,
}

/// **The user's illustrative terms (2026-09-25)**: `A0 = 20 MSK`, `α = 400‰`, `β = 500‰`,
/// `W = 5,040 DAA`, caps 64 and 32, a 1 MSK least top-up.
pub const PALW_ACTIVATION_POOL_TERMS_V1: PalwActivationPoolTermsV1 = PalwActivationPoolTermsV1 {
    prep_base_sompi: 20 * crate::constants::SOMPI_PER_KASPA,
    prep_share_permille: 400,
    bonus_share_permille: 500,
    ramp_daa: 5_040,
    prep_payee_cap: 64,
    bonus_payee_cap: 32,
    min_topup_sompi: crate::constants::SOMPI_PER_KASPA,
};

impl Default for PalwActivationPoolTermsV1 {
    fn default() -> Self {
        PALW_ACTIVATION_POOL_TERMS_V1
    }
}

impl PalwActivationPoolTermsV1 {
    /// Why these terms cannot run, or `None`. A permille past 1,000 would pay out more than a
    /// budget holds; a zero cap is a pool that can never pay; a zero ramp divides by nothing; a
    /// zero least top-up admits a row per dust output.
    pub fn refusal(&self) -> Option<&'static str> {
        if self.prep_share_permille > 1_000 {
            return Some("palw_activation_pool's prep_share_permille (α) is past 1,000 ‰");
        }
        if self.bonus_share_permille == 0 || self.bonus_share_permille > 1_000 {
            return Some("palw_activation_pool's bonus_share_permille (β) is not in 1..=1,000 ‰");
        }
        if self.ramp_daa == 0 {
            return Some("palw_activation_pool's ramp_daa (W) is zero");
        }
        if self.prep_payee_cap == 0 || self.bonus_payee_cap == 0 {
            return Some("palw_activation_pool's payee caps must both be positive");
        }
        if self.min_topup_sompi == 0 {
            return Some("palw_activation_pool's min_topup_sompi is zero");
        }
        if self.prep_base_sompi == 0 {
            return Some("palw_activation_pool's prep_base_sompi (A0) is zero");
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// R2: every Candidate meets its jury at its own span (the review's M2 and the design's burst fix)
// ---------------------------------------------------------------------------------------------

/// The key of a class's audit offset (`H(domain ‖ class_id)`).
pub const PALW_ADMISSION_AUDIT_STAGGER_DOMAIN_V1: &[u8] = b"misaka-palw/admission-audit/stagger/v1";

/// **R2: where in the audit period a class's audit falls** — `H(domain ‖ class_id) mod period`,
/// the first eight bytes of the keyed BLAKE2b-512 read little-endian. Stateless: nothing records the
/// offset and nothing can move it, so a registrant cannot choose its audit span except by choosing
/// its class id — which it already chooses, and which buys it one fixed span in the period, the
/// same one draw every period as before.
pub fn palw_admission_audit_offset_v1(class_id: &Hash64, period_spans: u64) -> u64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_ADMISSION_AUDIT_STAGGER_DOMAIN_V1).to_state();
    state.update(class_id.as_byte_slice());
    let digest = state.finalize();
    let mut word = [0u8; 8];
    word.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(word) % period_spans.max(1)
}

/// **R2: is `span_now` this class's audit span** — `(span + H(class_id)) mod period == 0`, span zero
/// never (the jury's seed is the anchor of the span before, and there is none). The period is the
/// unstaggered one ([`crate::palw_model_registry_v1::palw_admission_audit_period_spans_v2`]), so a
/// class still meets exactly one jury per period; only WHICH span of the period it is moves, and
/// the network's readiness proofs for different classes stop landing in the same few blocks.
pub fn palw_admission_audit_due_staggered_v1(class_id: &Hash64, span_now: u64, period_spans: u64) -> bool {
    let period = period_spans.max(1);
    span_now > 0 && (span_now % period + palw_admission_audit_offset_v1(class_id, period)) % period == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_terms_are_the_users_scale_and_run() {
        let t = PALW_ACTIVATION_POOL_TERMS_V1;
        assert_eq!(t.prep_base_sompi, 2_000_000_000, "A0 = 20 MSK");
        assert_eq!((t.prep_share_permille, t.bonus_share_permille), (400, 500), "α = 400 ‰, β = 500 ‰");
        assert_eq!(t.ramp_daa, 5_040, "W = 7 days of 120 s DAA");
        assert_eq!((t.prep_payee_cap, t.bonus_payee_cap), (64, 32));
        assert_eq!(t.min_topup_sompi, 100_000_000, "1 MSK");
        assert_eq!(t.refusal(), None);
        assert!(PalwActivationPoolTermsV1 { bonus_share_permille: 0, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { prep_share_permille: 1_001, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { ramp_daa: 0, ..t }.refusal().is_some());
        assert!(PalwActivationPoolTermsV1 { prep_payee_cap: 0, ..t }.refusal().is_some());
    }

    /// R2: the stagger is one span per period per class, spread over the period, and stateless.
    #[test]
    fn a_class_meets_one_jury_per_period_at_its_own_span() {
        let period = 100u64;
        let mut offsets = std::collections::BTreeSet::new();
        for n in 0..64u64 {
            let class = Hash64::from_u64_word(0xC1A5_5000 + n);
            let due: Vec<u64> = (1..=3 * period).filter(|span| palw_admission_audit_due_staggered_v1(&class, *span, period)).collect();
            assert_eq!(due.len(), 3, "one audit per period: {due:?}");
            assert!(due.windows(2).all(|w| w[1] - w[0] == period), "exactly a period apart: {due:?}");
            assert_eq!((due[0] + palw_admission_audit_offset_v1(&class, period)) % period, 0);
            offsets.insert(palw_admission_audit_offset_v1(&class, period));
        }
        assert!(offsets.len() > 40, "64 classes spread over the period, not stacked on one span: {} offsets", offsets.len());
        assert!(!palw_admission_audit_due_staggered_v1(&Hash64::from_u64_word(1), 0, period), "span zero is never an audit");
        assert!((1..10).all(|span| palw_admission_audit_due_staggered_v1(&Hash64::from_u64_word(9), span, 1)), "a one-span period audits every span");
    }
}
