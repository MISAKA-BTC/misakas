//! Ingestion-time fact admission (ADR-0038 D4: I-MTP-2, I-MTP-5, I-MTP-6).
//!
//! Every function here decides **whether a fact is admissible and with what
//! multiplier / base points** — never the point *value* itself (that stays in the
//! deterministic core). Two principles run through the module:
//!
//! * **I-MTP-2 — no participant-supplied booleans reach the scorer.** The C1
//!   `geo_diverse` / `fast_follow` multipliers are *derived by the service* from
//!   crawler observations ([`derive_geo_diverse`], [`derive_fast_follow`]); a
//!   `NodeRecord` field a participant could set in their own node config is treated
//!   as a claim, not a fact.
//! * **I-MTP-5 / I-MTP-6 — caps and adjudication happen at ingestion.** A C2 bug
//!   fact exists only when an allowlisted maintainer applied a terminal triage
//!   label ([`gated_bug_event`]); a C3/C4 fact's base points are table-resolved and
//!   per-event-capped before storage ([`resolve_c3_c4`]). Because caps are applied
//!   to the *fact*, the published ledger stays reproducible from published facts.

use misaka_mtp::{Category, Severity};
use misaka_mtp_collectors::GhEvent;

// --- I-MTP-2: service-derived C1 multipliers ------------------------------------------------

/// Vantage home regions (ISO-3166 alpha-2). The crawler vantages sit in DE and JP
/// (ADR-0038 D2), so a node the crawler geolocates to one of these earns **no**
/// `m_geo` bonus — the 1.5× is for infrastructure genuinely distant from the
/// vantages (design §3.1).
pub const VANTAGE_HOME_REGIONS: [&str; 2] = ["DE", "JP"];

/// The fast-follow window: a node must be observed on the current release within
/// 72 h of its publication to earn `m_ver` (design §3.1).
pub const FAST_FOLLOW_WINDOW_MS: u64 = 72 * 3600 * 1000;

/// Derive `geo_diverse` from crawler-observed IP geolocation (I-MTP-2). Never from
/// node config. `observed_country` is the ISO-3166 alpha-2 code the crawler
/// resolved for the node's source address; a node in a vantage-home region scores
/// ×1.0, everything else ×1.5.
pub fn derive_geo_diverse(observed_country: &str) -> bool {
    !VANTAGE_HOME_REGIONS.iter().any(|h| h.eq_ignore_ascii_case(observed_country))
}

/// A crawler-observed release-version crossing (I-MTP-2, the `m_ver` 1.2× input).
/// Both timestamps are service-side observations — the participant cannot set
/// either, so the multiplier cannot be self-declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionObservation {
    /// Wall-clock ms at which the crawler first observed this node advertising the
    /// current release version in its P2P user-agent.
    pub observed_at_ms: u64,
    /// Wall-clock ms at which the current release was published.
    pub release_published_ms: u64,
}

/// Derive `fast_follow` (I-MTP-2): the node was observed on the current release
/// within [`FAST_FOLLOW_WINDOW_MS`] of publication. An observation *before*
/// publication (clock skew or a pre-release build) does not qualify — fail-closed.
pub fn derive_fast_follow(obs: &VersionObservation) -> bool {
    obs.observed_at_ms >= obs.release_published_ms && obs.observed_at_ms - obs.release_published_ms <= FAST_FOLLOW_WINDOW_MS
}

// --- I-MTP-5: label-actor-gated C2 bug facts ------------------------------------------------

/// A triage label observed on an issue together with the GitHub login that applied
/// it (read from the issue timeline / events API) — the raw input to the I-MTP-5
/// gate. Label *presence* alone is spoofable by anyone with triage permission or a
/// bot, so the applying actor must be verified against the pinned allowlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelEvent {
    /// The label name, e.g. `sev/S1`, `points/accepted`, `points/duplicate-of-#42`,
    /// `points/rejected`, `points/needs-repro`.
    pub label: String,
    /// The GitHub login that applied the label (the timeline `labeled` event actor).
    pub actor: String,
}

impl LabelEvent {
    pub fn new(label: impl Into<String>, actor: impl Into<String>) -> Self {
        Self { label: label.into(), actor: actor.into() }
    }
}

fn parse_severity(label: &str) -> Option<Severity> {
    match label {
        "sev/S0" => Some(Severity::S0),
        "sev/S1" => Some(Severity::S1),
        "sev/S2" => Some(Severity::S2),
        "sev/S3" => Some(Severity::S3),
        _ => None,
    }
}

/// Gate a bug report into a scoreable C2 fact (I-MTP-5). Returns `Some(GhEvent)`
/// **only** when both of these hold, each verified against the applying actor:
///
///  * a `sev/SX` severity label applied by a maintainer on `maintainer_allowlist`;
///  * a terminal `points/accepted` (⇒ `first_report`) **or**
///    `points/duplicate-of-#N` (⇒ duplicate) label applied by an allowlisted
///    maintainer.
///
/// Everything else yields `None` (no fact, i.e. zero points): a missing severity,
/// a missing terminal label, `points/rejected` / `points/needs-repro`, or any
/// scoring label whose applying actor is not allowlisted. Public-issue disclosure
/// of a security-classified bug is handled upstream by the triage bot applying an
/// explicit `points/rejected` — which reaches here as "no accepted/duplicate
/// terminal" and correctly produces no fact (ADR-0027 precondition 4).
pub fn gated_bug_event(
    reporter_id: &str,
    labels: &[LabelEvent],
    maintainer_allowlist: &[String],
    fix_pr_accepted: bool,
    evidence: &str,
) -> Option<GhEvent> {
    let on_allowlist = |actor: &str| maintainer_allowlist.iter().any(|m| m == actor);
    // A scoring label counts only when an allowlisted actor applied it.
    let allowlisted = |pred: &dyn Fn(&str) -> bool| labels.iter().find(|l| pred(&l.label) && on_allowlist(&l.actor));

    let sev = allowlisted(&|n: &str| n.starts_with("sev/S"))?;
    let severity = parse_severity(&sev.label)?;

    let accepted = allowlisted(&|n: &str| n == "points/accepted").is_some();
    let duplicate = allowlisted(&|n: &str| n.starts_with("points/duplicate-of-")).is_some();
    // `accepted` wins if both are somehow present; neither ⇒ no fact (fail-closed).
    let first_report = if accepted {
        true
    } else if duplicate {
        false
    } else {
        return None;
    };

    Some(GhEvent { reporter_id: reporter_id.to_string(), severity, first_report, fix_pr_accepted, evidence: evidence.to_string() })
}

// --- I-MTP-6: cap-at-ingestion C3/C4 resolution ---------------------------------------------

/// Per-event cap on C3 load-window transaction points (design §3.3): at most this
/// many points from a single calendared load window.
pub const LOAD_WINDOW_PER_EVENT_CAP: u64 = 100;
/// Accepted transactions per load-window point (1 pt / 100 accepted tx, §3.3).
pub const TX_PER_LOAD_POINT: u64 = 100;
/// C4 infrastructure tier band (maintainer-assessed, §3.4).
pub const INFRA_TIER_BAND: std::ops::RangeInclusive<u64> = 100..=300;
/// C4 docs/tooling PR band, per accepted item (§3.4).
pub const DOCS_TOOLING_BAND: std::ops::RangeInclusive<u64> = 50..=500;

/// A C3/C4 activity to resolve to `(category, base_points)` at ingestion (I-MTP-6).
/// Every arm bounds its value here so no free-form point value ever reaches the
/// scorer. Only ADR-0026-calendared windows produce a [`Self::LoadWindowTx`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixedActivity {
    /// C3 load generation inside a calendared window: 1 pt / 100 accepted tx,
    /// capped at [`LOAD_WINDOW_PER_EVENT_CAP`] for the event. `accepted_tx` is
    /// counted by the service's own indexer over accepted-set transactions of the
    /// registered address (I-MTP-8).
    LoadWindowTx { accepted_tx: u64 },
    /// C4 infrastructure contribution, maintainer-assessed tier ([`INFRA_TIER_BAND`]).
    InfraTier { tier_points: u64 },
    /// C4 docs/tooling PR, per accepted item ([`DOCS_TOOLING_BAND`]).
    DocsToolingPr { item_points: u64 },
}

/// Resolve a [`FixedActivity`] to `(category, base_points)` with the per-event cap
/// applied (I-MTP-6). Returns `None` when a maintainer-assessed value is outside
/// its band — fail-closed, so a mis-entered tier is rejected rather than silently
/// clamped to a boundary that could still be gamed.
pub fn resolve_c3_c4(activity: &FixedActivity) -> Option<(Category, u64)> {
    match *activity {
        FixedActivity::LoadWindowTx { accepted_tx } => {
            let pts = (accepted_tx / TX_PER_LOAD_POINT).min(LOAD_WINDOW_PER_EVENT_CAP);
            Some((Category::Verify, pts))
        }
        FixedActivity::InfraTier { tier_points } => INFRA_TIER_BAND.contains(&tier_points).then_some((Category::Infra, tier_points)),
        FixedActivity::DocsToolingPr { item_points } => {
            DOCS_TOOLING_BAND.contains(&item_points).then_some((Category::Infra, item_points))
        }
    }
}

/// The per-identity per-epoch aggregate cap for C3 load-window points (I-MTP-6):
/// no identity may take more than `100 pt × calendared_events` from load windows in
/// one epoch. This is defense-in-depth — the primary bound is that the indexer
/// emits exactly one load-window fact per `(identity, calendared event)`, each
/// already ≤ [`LOAD_WINDOW_PER_EVENT_CAP`], so the sum is naturally bounded. The
/// clamp is applied to *facts* (not the scored ledger) so reproducibility holds.
pub fn cap_epoch_load_window(total_load_pts: u64, calendared_events: u64) -> u64 {
    total_load_pts.min(LOAD_WINDOW_PER_EVENT_CAP.saturating_mul(calendared_events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geo_diverse_is_derived_not_declared() {
        // Vantage-home regions never earn the bonus (case-insensitive).
        assert!(!derive_geo_diverse("DE"));
        assert!(!derive_geo_diverse("jp"));
        // Anything else is geo-diverse.
        assert!(derive_geo_diverse("US"));
        assert!(derive_geo_diverse("SG"));
        assert!(derive_geo_diverse(""));
    }

    #[test]
    fn fast_follow_only_within_72h_after_publication() {
        let pub_ms = 1_000_000_000;
        // exactly at publication → qualifies.
        assert!(derive_fast_follow(&VersionObservation { observed_at_ms: pub_ms, release_published_ms: pub_ms }));
        // one hour later → qualifies.
        assert!(derive_fast_follow(&VersionObservation { observed_at_ms: pub_ms + 3_600_000, release_published_ms: pub_ms }));
        // exactly at the 72h boundary → qualifies.
        assert!(derive_fast_follow(&VersionObservation {
            observed_at_ms: pub_ms + FAST_FOLLOW_WINDOW_MS,
            release_published_ms: pub_ms
        }));
        // one ms past the window → no.
        assert!(!derive_fast_follow(&VersionObservation {
            observed_at_ms: pub_ms + FAST_FOLLOW_WINDOW_MS + 1,
            release_published_ms: pub_ms
        }));
        // observed before publication (skew / pre-release build) → no (fail-closed).
        assert!(!derive_fast_follow(&VersionObservation { observed_at_ms: pub_ms - 1, release_published_ms: pub_ms }));
    }

    fn allow() -> Vec<String> {
        vec!["maintainer-a".into(), "maintainer-b".into()]
    }

    #[test]
    fn bug_fact_requires_allowlisted_severity_and_terminal() {
        // both labels applied by an allowlisted maintainer → an accepted first report.
        let labels = vec![LabelEvent::new("sev/S1", "maintainer-a"), LabelEvent::new("points/accepted", "maintainer-b")];
        let ev = gated_bug_event("gh:alice", &labels, &allow(), false, "gh#1").expect("gated fact");
        assert_eq!(ev.severity, Severity::S1);
        assert!(ev.first_report);
        assert!(!ev.fix_pr_accepted);
    }

    #[test]
    fn duplicate_terminal_yields_a_non_first_report() {
        let labels = vec![LabelEvent::new("sev/S2", "maintainer-a"), LabelEvent::new("points/duplicate-of-#7", "maintainer-a")];
        let ev = gated_bug_event("gh:bob", &labels, &allow(), false, "gh#2").unwrap();
        assert_eq!(ev.severity, Severity::S2);
        assert!(!ev.first_report, "a duplicate is not a first report");
    }

    #[test]
    fn non_allowlisted_actor_yields_no_fact() {
        // The exact same labels, but applied by a stranger with triage perms → no fact.
        let labels = vec![LabelEvent::new("sev/S0", "stranger"), LabelEvent::new("points/accepted", "stranger")];
        assert!(gated_bug_event("gh:eve", &labels, &allow(), false, "gh#3").is_none());
        // Severity allowlisted but terminal by a stranger → still no fact.
        let mixed = vec![LabelEvent::new("sev/S0", "maintainer-a"), LabelEvent::new("points/accepted", "stranger")];
        assert!(gated_bug_event("gh:eve", &mixed, &allow(), false, "gh#3").is_none());
    }

    #[test]
    fn rejected_or_needs_repro_or_missing_terminal_yields_no_fact() {
        let rejected = vec![LabelEvent::new("sev/S0", "maintainer-a"), LabelEvent::new("points/rejected", "maintainer-a")];
        assert!(gated_bug_event("gh:x", &rejected, &allow(), false, "e").is_none());
        let needs_repro = vec![LabelEvent::new("sev/S1", "maintainer-a"), LabelEvent::new("points/needs-repro", "maintainer-a")];
        assert!(gated_bug_event("gh:x", &needs_repro, &allow(), false, "e").is_none());
        // severity present but no terminal at all → no fact.
        let only_sev = vec![LabelEvent::new("sev/S1", "maintainer-a")];
        assert!(gated_bug_event("gh:x", &only_sev, &allow(), false, "e").is_none());
        // terminal present but no severity → no fact.
        let only_terminal = vec![LabelEvent::new("points/accepted", "maintainer-a")];
        assert!(gated_bug_event("gh:x", &only_terminal, &allow(), false, "e").is_none());
    }

    #[test]
    fn load_window_tx_is_capped_per_event() {
        // 5000 accepted tx → 50 pts (1/100), under the cap.
        assert_eq!(resolve_c3_c4(&FixedActivity::LoadWindowTx { accepted_tx: 5_000 }), Some((Category::Verify, 50)));
        // a wash-tx flood of 1e9 tx saturates to exactly the per-event cap.
        assert_eq!(
            resolve_c3_c4(&FixedActivity::LoadWindowTx { accepted_tx: 1_000_000_000 }),
            Some((Category::Verify, LOAD_WINDOW_PER_EVENT_CAP))
        );
        // sub-100 tx → 0 pts.
        assert_eq!(resolve_c3_c4(&FixedActivity::LoadWindowTx { accepted_tx: 99 }), Some((Category::Verify, 0)));
    }

    #[test]
    fn infra_and_docs_tiers_are_band_checked() {
        assert_eq!(resolve_c3_c4(&FixedActivity::InfraTier { tier_points: 200 }), Some((Category::Infra, 200)));
        // out of band → fail-closed None (not a silent clamp).
        assert_eq!(resolve_c3_c4(&FixedActivity::InfraTier { tier_points: 99 }), None);
        assert_eq!(resolve_c3_c4(&FixedActivity::InfraTier { tier_points: 301 }), None);
        assert_eq!(resolve_c3_c4(&FixedActivity::DocsToolingPr { item_points: 500 }), Some((Category::Infra, 500)));
        assert_eq!(resolve_c3_c4(&FixedActivity::DocsToolingPr { item_points: 49 }), None);
    }

    #[test]
    fn epoch_load_window_aggregate_cap() {
        // 3 calendared events → aggregate ceiling 300; a total of 250 passes.
        assert_eq!(cap_epoch_load_window(250, 3), 250);
        // a total above the ceiling is clamped.
        assert_eq!(cap_epoch_load_window(400, 3), 300);
        // zero calendared events → zero take (no window ran this epoch).
        assert_eq!(cap_epoch_load_window(100, 0), 0);
    }
}
