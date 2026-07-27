//! Deterministic per-epoch scoring (ADR-0027 §3, §11-A). Pure functions of
//! (collected facts × [`Rules`]) → milli-points. Integer-only for reproducibility.

use crate::rules::{Category, MilliPoints, POINT, Rules, Severity, Stage};

/// `floor(base_mpts × Π num_i / Π den_i)` in u128, saturating to u64. The single
/// combined product (not sequential rounding) keeps the result order-independent.
/// A zero denominator (e.g. an uptime window with 0 samples) yields 0 points.
pub fn scale(base_mpts: MilliPoints, factors: &[(u64, u64)]) -> MilliPoints {
    let mut num: u128 = base_mpts as u128;
    let mut den: u128 = 1;
    for &(n, d) in factors {
        num = num.saturating_mul(n as u128);
        den = den.saturating_mul(d as u128);
    }
    if den == 0 {
        return 0;
    }
    (num / den).min(u64::MAX as u128) as u64
}

/// C1 full-node uptime: `100 · u · m_geo · m_ver · d_n · m_stage` (§3.1).
/// `u = ok/total` (an at-sync-required success rate); `node_rank` is the per-ID
/// decrement rank (0 = first node ×1.0, 1 = second ×0.5, …). Rank ≥ 4 scores 0.
#[allow(clippy::too_many_arguments)]
pub fn pts_node(
    rules: &Rules,
    uptime_ok: u64,
    uptime_total: u64,
    geo_diverse: bool,
    fast_follow: bool,
    node_rank: usize,
    stage: Stage,
) -> MilliPoints {
    let base = rules.node_uptime_base * POINT;
    let d_n = *rules.d_n.get(node_rank).unwrap_or(&(0, 1));
    let m_geo = if geo_diverse { (rules.m_geo_num, rules.m_geo_den) } else { (1, 1) };
    let m_ver = if fast_follow { (rules.m_ver_num, rules.m_ver_den) } else { (1, 1) };
    scale(base, &[(uptime_ok, uptime_total), m_geo, m_ver, d_n, stage.factor()])
}

/// C1 validator/attestor: `200 · a · m_stage`, where `a = attested/total` epoch
/// participation. A slashing/evidence event in the window forfeits the week (§3.1).
pub fn pts_validator(rules: &Rules, attested_epochs: u64, total_epochs: u64, slashed: bool, stage: Stage) -> MilliPoints {
    if slashed {
        return 0;
    }
    scale(rules.validator_base * POINT, &[(attested_epochs, total_epochs), stage.factor()])
}

/// C2 bug report: `S(severity) · (first ? 1 : dup) · m_stage`, plus the same
/// severity again if the fix PR was accepted (§3.2).
pub fn pts_bug(rules: &Rules, severity: Severity, first_report: bool, fix_pr_accepted: bool, stage: Stage) -> MilliPoints {
    let dup = if first_report { (1, 1) } else { (rules.bug_dup_num, rules.bug_dup_den) };
    let report = scale(severity.base_points() * POINT, &[dup, stage.factor()]);
    // The accepted-fix bonus is the full severity (not duplicate-scaled), stage-weighted.
    let fix = if fix_pr_accepted { scale(severity.base_points() * POINT, &[stage.factor()]) } else { 0 };
    report + fix
}

/// A fixed-value activity (C3 campaigns/feedback, C4 infra) at `base_points`,
/// stage-weighted. Load-window tx points (§3.3) and tiered infra (§3.4) resolve to
/// a `base_points` upstream (the collector applies the per-event cap/tier) and are
/// scored here.
pub fn pts_fixed(base_points: u64, stage: Stage) -> MilliPoints {
    scale(base_points * POINT, &[stage.factor()])
}

/// A fact-carrying entry that resolves to points in exactly one category.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum Contribution {
    Node {
        uptime_ok: u64,
        uptime_total: u64,
        geo_diverse: bool,
        fast_follow: bool,
        node_rank: usize,
    },
    Validator {
        attested_epochs: u64,
        total_epochs: u64,
        slashed: bool,
    },
    IbdBench,
    Drill,
    Bug {
        severity: Severity,
        first_report: bool,
        fix_pr_accepted: bool,
    },
    /// C3 / C4 fixed-value entry (base points already tier/cap-resolved by the collector).
    Fixed {
        category: Category,
        base_points: u64,
    },
}

impl Contribution {
    /// The category this contribution scores into.
    pub fn category(&self) -> Category {
        match self {
            Contribution::Node { .. } | Contribution::Validator { .. } | Contribution::IbdBench | Contribution::Drill => {
                Category::Node
            }
            Contribution::Bug { .. } => Category::Bug,
            Contribution::Fixed { category, .. } => *category,
        }
    }

    /// The milli-points this contribution earns at `stage` under `rules`.
    pub fn points(&self, rules: &Rules, stage: Stage) -> MilliPoints {
        match *self {
            Contribution::Node { uptime_ok, uptime_total, geo_diverse, fast_follow, node_rank } => {
                pts_node(rules, uptime_ok, uptime_total, geo_diverse, fast_follow, node_rank, stage)
            }
            Contribution::Validator { attested_epochs, total_epochs, slashed } => {
                pts_validator(rules, attested_epochs, total_epochs, slashed, stage)
            }
            Contribution::IbdBench => pts_fixed(rules.ibd_bench_points, stage),
            Contribution::Drill => pts_fixed(rules.drill_points, stage),
            Contribution::Bug { severity, first_report, fix_pr_accepted } => {
                pts_bug(rules, severity, first_report, fix_pr_accepted, stage)
            }
            Contribution::Fixed { base_points, .. } => pts_fixed(base_points, stage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_scoring_applies_every_multiplier() {
        let r = Rules::default();
        // 100 base · u=97/100 · m_geo=1.5 · m_ver=1.2 · d_n=1 · stage_A=1
        // = 100_000 mpts · 97/100 · 3/2 · 6/5 = 174_600 mpts (floor).
        let p = pts_node(&r, 97, 100, true, true, 0, Stage::A);
        assert_eq!(p, 174_600);
        // second node halves; no geo/ver; stage A ×1:
        // 100_000 · 100/100 · 1 · 1 · 1/2 · 1 = 50_000.
        assert_eq!(pts_node(&r, 100, 100, false, false, 1, Stage::A), 50_000);
        // fourth node (rank 3) → 0.
        assert_eq!(pts_node(&r, 100, 100, true, true, 3, Stage::A), 0);
        // zero-sample window → 0, never panics.
        assert_eq!(pts_node(&r, 0, 0, true, true, 0, Stage::A), 0);
    }

    #[test]
    fn validator_slashing_forfeits_the_week() {
        let r = Rules::default();
        assert_eq!(pts_validator(&r, 1, 1, false, Stage::A), 200_000);
        assert_eq!(pts_validator(&r, 1, 1, true, Stage::A), 0, "slashed week = 0");
        // half participation, stage A ×1: 200_000 · 1/2 · 1 = 100_000.
        assert_eq!(pts_validator(&r, 1, 2, false, Stage::A), 100_000);
    }

    #[test]
    fn bug_first_vs_duplicate_and_fix_bonus() {
        let r = Rules::default();
        // S1 first report, stage A: 2000 pts.
        assert_eq!(pts_bug(&r, Severity::S1, true, false, Stage::A), 2_000_000);
        // duplicate S1: 10% = 200 pts.
        assert_eq!(pts_bug(&r, Severity::S1, false, false, Stage::A), 200_000);
        // first + accepted fix PR: 2000 + 2000 = 4000 pts.
        assert_eq!(pts_bug(&r, Severity::S1, true, true, Stage::A), 4_000_000);
    }

    #[test]
    fn scoring_is_deterministic_and_order_independent() {
        let r = Rules::default();
        let a = pts_node(&r, 7, 11, true, true, 0, Stage::A);
        let b = pts_node(&r, 7, 11, true, true, 0, Stage::A);
        assert_eq!(a, b);
        // combined product == manual single computation (no sequential rounding drift).
        // 100_000 · 7 · 3 · 6 · 1 · 1 / (11 · 2 · 5 · 1 · 1) = 12_600_000 / 110 = 114_545 (floor).
        assert_eq!(a, 114_545);
    }
}
