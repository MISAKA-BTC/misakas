//! ADR-0040 operation rounding registry.
//!
//! # The rule
//!
//! Every consensus site that rounds, truncates, saturates, or interpolates a lookup table has one row
//! here. Exact integer operations do not require a row.
//!
//! The registry provides a single review and conformance point, analogous to
//! `crate::signature_domains`.
//!
//! # Default mode
//!
//! Round-to-nearest, ties-to-even avoids directional drift under repeated updates. Rows that use a
//! different mode record the reason.
//!
//! # Conformance vector
//!
//! Each row id is also its conformance-vector id, keeping the declared rule and its test aligned.

/// How a site resolves a value that is not exactly representable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundingMode {
    /// Round to nearest, ties to even. This is the default mode.
    Rne,
    /// Exact integer division toward zero. Declared where the quotient is definitionally a floor
    /// (indices, quantum counts) rather than an approximation.
    Floor,
    /// Bit truncation / shift. Declared where the low bits are being discarded by definition.
    Truncate,
    /// Frozen legacy behaviour retained for compatibility.
    LegacyFrozen,
}

/// What happens when a value leaves its representable range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Clamp to the range bound. Legitimate only where the clamp is part of the SPEC (activation
    /// quantisation), never as an accident-absorber.
    Clamp,
    /// Saturate at the type bound.
    Saturating,
    /// Fail loudly. Correct for oracles/reference paths, which must not silently produce a wrong
    /// answer that a fast path would then be measured against.
    Assert,
    /// Proven impossible by the overflow-budget table or a domain invariant.
    Impossible,
}

/// One declared rounding site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoundingSite {
    /// Registry id — **also the conformance-vector id**, so rule and test cannot drift apart.
    pub id: &'static str,
    /// Where it happens.
    pub site: &'static str,
    /// The arithmetic representation.
    pub representation: &'static str,
    pub mode: RoundingMode,
    /// Tie resolution, where the mode has ties.
    pub tie: &'static str,
    pub overflow: OverflowPolicy,
    /// Why this site deviates from the RNE default; empty when it does not.
    pub deviation_reason: &'static str,
}

/// Rounding registry. Add each new rounding site in the same change that introduces it.
pub const REGISTERED_ROUNDING_SITES: &[RoundingSite] = &[
    RoundingSite {
        id: "R-01",
        site: "model/requant int32→int8",
        representation: "fixed-point multiply through an i64 intermediate",
        mode: RoundingMode::Rne,
        tie: "even",
        overflow: OverflowPolicy::Clamp, // to [-127, 127] — the spec'd activation domain
        // The canonical-compute §3.3 draft wrote the half-up form `(x + (1 << (shift-1))) >> shift`,
        // which contradicts §3.1's stated RNE. This row is the permanent resolution: RNE governs, and
        // the half-up expression is wrong wherever it still appears.
        deviation_reason: "",
    },
    RoundingSite {
        id: "R-02",
        site: "model/softmax normalisation divide",
        representation: "fixed-point long division",
        mode: RoundingMode::Floor,
        tie: "n/a",
        overflow: OverflowPolicy::Impossible, // denominator > 0 by construction
        deviation_reason: "exact integer division: the quotient is defined as a floor, not approximated",
    },
    RoundingSite {
        id: "R-03",
        site: "model/isqrt (RMSNorm)",
        representation: "integer square root ⌊√n⌋",
        mode: RoundingMode::Floor,
        tie: "n/a",
        overflow: OverflowPolicy::Impossible,
        deviation_reason: "⌊√n⌋ is the definition, not a rounding choice",
    },
    RoundingSite {
        id: "R-04",
        site: "model/LUT index derivation",
        representation: "shift + clamp",
        mode: RoundingMode::Truncate,
        tie: "n/a",
        overflow: OverflowPolicy::Clamp,
        deviation_reason: "the index is a bit-field selection; the discarded low bits are the interpolation residue",
    },
    RoundingSite {
        id: "R-05",
        site: "econ/π update, σ split, EMA",
        representation: "basis-point integers",
        mode: RoundingMode::Rne,
        tie: "even",
        overflow: OverflowPolicy::Assert,
        deviation_reason: "",
    },
    RoundingSite {
        id: "R-06",
        site: "econ/esc(k) repeat-offence escalation",
        representation: "integer power μ^min(k, k_cap)",
        mode: RoundingMode::Floor, // exact; no fractional part exists
        tie: "n/a",
        overflow: OverflowPolicy::Saturating,
        deviation_reason: "exact integer exponentiation; k_cap bounds it before saturation is reachable",
    },
    RoundingSite {
        id: "R-07",
        site: "lottery/⌊CU / quantum⌋",
        representation: "integer division",
        mode: RoundingMode::Floor,
        tie: "n/a",
        overflow: OverflowPolicy::Impossible,
        deviation_reason: "a partial quantum earns no ticket; flooring is the rule, not an approximation",
    },
    RoundingSite {
        id: "R-08",
        site: "daa/difficulty retarget",
        representation: "existing Uint320 arithmetic",
        mode: RoundingMode::LegacyFrozen,
        tie: "as implemented",
        overflow: OverflowPolicy::Saturating,
        deviation_reason: "predates this registry and is consensus-frozen; changing it is a hard fork, so it is declared rather than migrated",
    },
];

/// Rounding sites that are **deliberately not yet registered** because the site does not exist yet.
/// Listed so an implementer reaches for a new row rather than silently borrowing a neighbouring one.
pub const PENDING_ROUNDING_SITES: &[&str] = &[
    // ADR-0040 §16′-1/2: the cohort sampler's r/I ratios (will be R-05-class RNE).
    "econ/window cohort sampler ratios (§16′-1)",
    // ADR-0040 P3-6: the real integer GPU kernels, once they exist per backend.
    "model/per-backend integer kernel requant (P3-6)",
];

/// `gemmlowp`-style `SQRDMULH` (round-half-away-from-zero, with a `-2^31` corner case) is a *different*
/// rounding rule from R-01. An implementation that uses it must declare its own id and **must not be
/// enabled at the same time as R-01** — see [`tests::conflicting_rounding_ids_are_never_both_active`].
///
/// Two rounding rules quietly coexisting at one site is the failure this constant exists to prevent:
/// each is individually defensible, and the pair is a consensus split.
pub const MUTUALLY_EXCLUSIVE_ROUNDING_IDS: &[(&str, &str)] = &[("R-01", "R-01-SQRDMULH")];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Ids are unique — they double as conformance-vector ids, so a collision would make two different
    /// rules share one test.
    #[test]
    fn registered_rounding_ids_are_unique() {
        let mut seen = HashSet::new();
        for s in REGISTERED_ROUNDING_SITES {
            assert!(!s.id.is_empty() && !s.site.is_empty());
            assert!(seen.insert(s.id), "duplicate rounding-site id {}", s.id);
        }
    }

    /// **RNE is the default; every deviation states why.** An unexplained deviation is how a half-up
    /// site slips in and drifts a multiplicative recurrence upward forever.
    #[test]
    fn every_non_rne_site_declares_a_reason() {
        for s in REGISTERED_ROUNDING_SITES {
            if s.mode != RoundingMode::Rne {
                assert!(!s.deviation_reason.is_empty(), "{} ({}) deviates from the RNE default without stating why", s.id, s.site);
            }
        }
    }

    /// A site whose ties matter must say how they resolve. `Rne` without `"even"` is a contradiction in
    /// terms, and catching it here is cheaper than catching it in a cross-backend mismatch.
    #[test]
    fn rne_sites_resolve_ties_to_even() {
        for s in REGISTERED_ROUNDING_SITES {
            if s.mode == RoundingMode::Rne {
                assert_eq!(s.tie, "even", "{} declares RNE but resolves ties as {:?}", s.id, s.tie);
            }
        }
    }

    /// Reference/oracle paths must fail loudly rather than clamp or saturate: an oracle that silently
    /// produces a wrong value is worse than one that stops, because the fast paths are measured
    /// against it. (ADR-0040 §3.3: the oracle is the strictest implementation, not the most tolerant.)
    #[test]
    fn econ_recurrence_sites_fail_loudly_on_overflow() {
        let econ = REGISTERED_ROUNDING_SITES.iter().find(|s| s.id == "R-05").expect("R-05 registered");
        assert_eq!(econ.overflow, OverflowPolicy::Assert, "the π recurrence must assert, never silently saturate");
    }

    /// Two rounding rules must never be simultaneously active at one site. Each may be individually
    /// correct; the PAIR is a consensus split, and it is the kind that only shows up cross-vendor.
    #[test]
    fn conflicting_rounding_ids_are_never_both_active() {
        let active: HashSet<&str> = REGISTERED_ROUNDING_SITES.iter().map(|s| s.id).collect();
        for (a, b) in MUTUALLY_EXCLUSIVE_ROUNDING_IDS {
            assert!(
                !(active.contains(a) && active.contains(b)),
                "{a} and {b} are mutually exclusive rounding rules for the same site but both are registered"
            );
        }
    }

    /// The registry must actually cover the sites this ADR introduced — otherwise it is documentation
    /// that happens to compile rather than an enforcement point.
    #[test]
    fn registry_covers_the_sites_this_adr_introduced() {
        let ids: HashSet<&str> = REGISTERED_ROUNDING_SITES.iter().map(|s| s.id).collect();
        for required in ["R-01", "R-05", "R-06", "R-07"] {
            assert!(ids.contains(required), "{required} must be registered");
        }
        for p in PENDING_ROUNDING_SITES {
            assert!(!p.is_empty());
        }
    }
}
