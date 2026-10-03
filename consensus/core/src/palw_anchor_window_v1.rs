//! **The seed anchor is a window, not a span** (lane anchor/window, ADR-0170; ADR-0130's anchor and ADR-0147's jury, amended;
//! the DAA-5,300 flag day's fence `palw_anchor_window_v1`).
//!
//! **The hole.** The fold's seed anchor is "the latest chain block of the span before whose OWN header carried an admitted
//! attempt", reset at every span boundary, and it is read by exactly two things: ADR-0147's admission jury (an audit needs
//! the anchor of span `S − 1`, else no jury sits and the audit is skipped, not deferred) and ADR-0125/0130's schedule seeding
//! (one due snapshot a span, seeded only by the anchor of the span before). While floors flooded the chain that was true of
//! 98.7 % of testnet-12's spans. ADR-0165's reserve (`palw_floor_reserve_v1`) refuses a floor attempt while real work flows
//! (`FloorNotIdle`: skipped, never a claim, so never an anchor) and a REAL attempt is almost never a chain block (it is merged,
//! blue or red, by a heartbeat that already chose another selected parent), so under the reserve the anchored spans fall to
//! 2–3 %: a Candidate's audit seats in 2.7 % of its periods and 66 % of the exec lane's snapshots are dropped (P2's replay of
//! testnet-12's last 300 DAA, `lanes/evidence/head-admission-slip-1003/p2-anchor/`). The int-11 drill's head class missed its
//! first audit for the same reason at a 19.5 % skip rate: span 90 held two heartbeats and no attempt.
//!
//! **The rule, past the fence.** Three pieces, one height:
//!
//! * **M1 — an admitted attempt a chain block MERGES (blue or red) records the seed anchor, like the block's own does**
//!   ([`PALW_ANCHOR_WINDOW_MERGED_V1`]). Under the reserve those are the only attempts there are. The anchor is the latest
//!   admitted attempt the fold took, in consensus order, own first and then the mergeset's; its span is the span of the block
//!   that folded it, its execution key the attempt's own. Separately removable: with the flag `false` the fence is M2 + M3 only
//!   (the conservative variant, "own anchors only", whose window is then [`PALW_ANCHOR_WINDOW_SPANS_V1`] = 64, one line below).
//! * **M2 — the anchor is not cleared at a span boundary.** It keeps the span it was recorded in; every reader checks its age.
//! * **M3 — the jury and the schedule seeding read the latest anchor of the window** `S − W … S − 1`
//!   ([`palw_anchor_window_admits_v1`], `W` = [`PALW_ANCHOR_WINDOW_SPANS_V1`] = 24). The jury's population is then cut at the
//!   ANCHOR's span ([`palw_anchor_window_jury_cut_span_v1`]: bonds registered before the span the randomness was recorded in
//!   began), so a bond registered after the seed existed is never on the jury it seeds. A snapshot is seeded only by an anchor
//!   recorded at or after the span it was taken in ([`palw_anchor_window_seeds_snapshot_v1`]: ADR-0130's "participants first,
//!   randomness after", which `W ≤ maturity` already gives on testnet-12 and which is asserted rather than assumed).
//!
//! **What it costs, stated.** The audit stays one a period at the class's own staggered span (no deferral, no re-roll: the due
//! predicate is untouched, so P3's proof timing adapter is untouched). The jury's seed is known `S − anchor.span` spans before
//! the audit: p50 3, p95 15, max 20 spans on testnet-12's last 300 DAA (it was 1); the population is fixed before the seed, so
//! this lengthens only the time a registrant has to make drawn operators hold the class, which they can only do by holding it.
//! Moving the anchor costs an admitted attempt (a REAL inference while the reserve is closed; a bonded floor while it is open),
//! and the new freedom is WHICH of the attempts a merging block takes is last — a choice among attempts already public.
//!
//! No new state field, delta or carriage tail: `round_seed_anchor` is reused, and a network without the fence folds exactly as
//! before (the fence is `None` on every preset until the DAA-5,300 list arms it, hashed Some-only with its companion values).

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **W — how many spans back the jury and the schedule seeding may read the latest seed anchor.** The anchor must satisfy
/// `S − W ≤ anchor.span ≤ S − 1`. A companion value of the fence, hashed with its height ([`palw_anchor_window_value_v1`]): the
/// conservative variant (M1 off, own anchors only) needs 64 here (P2's replay: an own-anchor window of 64 spans seats 83 % of
/// audits where 24 seats 41 %), and changing it is a new network id.
pub const PALW_ANCHOR_WINDOW_SPANS_V1: u64 = 24;

/// **M1 — a merged admitted attempt records the seed anchor.** A companion value of the fence, hashed with it. `false` is the
/// conservative variant: only a block's own attempt anchors (set [`PALW_ANCHOR_WINDOW_SPANS_V1`] to 64 with it).
pub const PALW_ANCHOR_WINDOW_MERGED_V1: bool = true;

/// **The values the fingerprint hashes beside the fence's height** — `[W, M1]` — so changing either is a new network id, as
/// `palw_floor_reserve_value_v1` does for K.
pub const fn palw_anchor_window_value_v1() -> [u64; 2] {
    [PALW_ANCHOR_WINDOW_SPANS_V1, PALW_ANCHOR_WINDOW_MERGED_V1 as u64]
}

/// **The fence's entry for a flag-day list** (`PALW_T12_INT11_FENCES_V1`, after `palw_real_clock_tick_v1`): the height through its own
/// `set`, which writes the bundle's mirror.
pub const PALW_T12_ANCHOR_WINDOW_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_anchor_window_v1",
    set: |params, at| {
        params.palw_anchor_window_v1 = at;
        params.sync_palw_anchor_window_v1();
    },
};

/// The fence alone — what a drill that wants only this rule arms (`--palw-drill-int11-at` moves the whole flag-day list, this one
/// included, so there is no separate flag).
pub const PALW_T12_ANCHOR_WINDOW_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_T12_ANCHOR_WINDOW_ENTRY];

/// **Does an anchor recorded in `anchor_span` stand for a reader at `span_now` within `window` spans?**
/// `span_now − window ≤ anchor_span ≤ span_now − 1`. A zero window admits nothing; an anchor of the reader's own span or later
/// (impossible for a reader that runs at the span's opening, answered `false` rather than assumed) is not "the span before".
pub fn palw_anchor_window_admits_v1(anchor_span: u64, span_now: u64, window: u64) -> bool {
    anchor_span < span_now && span_now - anchor_span <= window
}

/// **The span the admission jury's population is cut at**, as the `span_now` argument of the one base-population function (whose
/// cutoff is `(span_now − 1) × span_daa`): the anchor's span plus one, so the cutoff is the DAA the anchor's span began at. For an
/// anchor of the span before the audit this is `span_now`, the rule as it was.
pub fn palw_anchor_window_jury_cut_span_v1(anchor_span: u64) -> u64 {
    anchor_span.saturating_add(1)
}

/// **May the anchor recorded in `anchor_span` seed the snapshot due at `target`?** Within the window of `span_now`
/// ([`palw_anchor_window_admits_v1`]) and recorded at or after the span the snapshot was taken in
/// (`target − 1 − maturity_spans`: the snapshot's target is the span after the one that took it, plus the maturity). A snapshot taken
/// in the anchor's own span is seeded by it, as the rule has always allowed (the opening block anchors the snapshot it took).
pub fn palw_anchor_window_seeds_snapshot_v1(anchor_span: u64, span_now: u64, window: u64, target: u64, maturity_spans: u64) -> bool {
    palw_anchor_window_admits_v1(anchor_span, span_now, window) && anchor_span >= target.saturating_sub(1).saturating_sub(maturity_spans)
}

/// **The Activation Pool's (a), generalised: the latest span a drawn juror's readiness proof may have LANDED in to be paid for its preparation**
/// — the span before the seed existed. The pool's rule is "landed at least two spans before the audit" (review M6: "before the seed existed";
/// the anchor of span `S − 1` is recorded in `S − 1`, so a proof of `S − 2` or earlier cannot have been sent by a juror that knew it was
/// drawn). With a carried anchor of an older span the seed exists earlier, and a proof that landed after it could be: so the bound is
/// `min(S − 2, anchor_span − 1)` — for an anchor of the span before the audit exactly the rule as it was. `None`: no span two before.
pub fn palw_anchor_window_prep_landed_by_v1(span_now: u64, anchor_span: u64) -> Option<u64> {
    span_now.checked_sub(2).map(|by| by.min(anchor_span.saturating_sub(1)))
}

impl Params {
    /// `palw_anchor_window_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_anchor_window_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_anchor_window_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Is the seed anchor a window at `daa_score`? `false` on every shipped preset.
    pub fn palw_anchor_window_active_at(&self, daa_score: u64) -> bool {
        self.palw_anchor_window_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params, which the fold reads.
    pub fn sync_palw_anchor_window_v1(&mut self) {
        let from_daa = self.palw_anchor_window_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_anchor_window_from_daa(from_daa);
        }
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`], each naming what is missing:
    ///
    /// * the mirror disagrees with the fence;
    /// * the fence off a `ConsensusV2` network;
    /// * the fence without the execution lane (`palw_execution_lane`) at or below it — it changes what the lane's seed anchor is;
    /// * without the economic-safety bundle (`palw_economic_safety`) at or below it — the schedule seeding it widens is that
    ///   bundle's branch of the rotation (a due snapshot waits for an anchored span);
    /// * without ADR-0147's admission jury (`palw_admission_independence`) at or below it — the jury it widens;
    /// * without the model registry (`palw_model_registry`) at or below it — there is no Candidate to audit.
    pub fn validate_palw_anchor_window_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.anchor_window_from_daa(),
            _ => None,
        };
        let armed = self.palw_anchor_window_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_anchor_window_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_anchor_window_v1",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_anchor_window_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let below = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !below(self.palw_execution_lane.map(|lane| lane.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_anchor_window_v1 needs palw_execution_lane at or below it: it changes what the execution lane's seed anchor is",
            ));
        }
        if !below(self.palw_economic_safety) {
            return Err(PalwModeV2Error::Invalid(
                "palw_anchor_window_v1 needs palw_economic_safety at or below it: the schedule seeding it widens is that bundle's rotation",
            ));
        }
        if !below(self.palw_admission_independence) {
            return Err(PalwModeV2Error::Invalid(
                "palw_anchor_window_v1 needs palw_admission_independence at or below it: it widens ADR-0147's admission jury",
            ));
        }
        if !below(self.palw_model_registry) {
            return Err(PalwModeV2Error::Invalid("palw_anchor_window_v1 needs palw_model_registry at or below it: a Candidate is a registry row"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_the_w_spans_before_the_reader_and_never_the_readers_own() {
        let w = PALW_ANCHOR_WINDOW_SPANS_V1;
        assert!(palw_anchor_window_admits_v1(99, 100, w), "the span before: the rule as it was");
        assert!(palw_anchor_window_admits_v1(100 - w, 100, w), "W spans back is the oldest the window reads");
        assert!(!palw_anchor_window_admits_v1(100 - w - 1, 100, w), "one span older is ignored");
        assert!(!palw_anchor_window_admits_v1(100, 100, w), "the reader's own span is not 'the span before'");
        assert!(!palw_anchor_window_admits_v1(101, 100, w), "nor a later one");
        assert!(!palw_anchor_window_admits_v1(99, 100, 0), "a zero window reads nothing");
        assert!(!palw_anchor_window_admits_v1(0, 0, w), "span zero has no span before it");
    }

    #[test]
    fn the_juries_population_is_cut_where_the_anchors_span_began() {
        // An anchor of the span before the audit: the cutoff is the one the rule always had, (S − 1) spans.
        assert_eq!(palw_anchor_window_jury_cut_span_v1(99), 100);
        // An older anchor cuts earlier: bonds registered after the seed existed are not on the jury it seeds.
        assert_eq!(palw_anchor_window_jury_cut_span_v1(80), 81);
        assert_eq!(palw_anchor_window_jury_cut_span_v1(u64::MAX), u64::MAX, "saturating, never a panic");
    }

    #[test]
    fn a_snapshot_is_seeded_only_by_an_anchor_recorded_in_or_after_the_span_it_was_taken_in() {
        // Taken in span 10, target 10 + 1 + 120 = 131: an anchor of span 10 or later seeds it, one of span 9 does not.
        assert!(palw_anchor_window_seeds_snapshot_v1(130, 131, 24, 131, 120));
        assert!(palw_anchor_window_seeds_snapshot_v1(110, 131, 24, 131, 120), "inside the window, recorded after the snapshot");
        assert!(!palw_anchor_window_seeds_snapshot_v1(106, 131, 24, 131, 120), "outside the window");
        // With no maturity (the legacy shape) the snapshot is taken the span before its target: only the span before seeds it.
        assert!(palw_anchor_window_seeds_snapshot_v1(130, 131, 24, 131, 0));
        assert!(!palw_anchor_window_seeds_snapshot_v1(129, 131, 24, 131, 0), "an anchor older than the snapshot seeds nothing");
        // A window wider than the maturity cannot reach back before the snapshot.
        assert!(!palw_anchor_window_seeds_snapshot_v1(5, 30, 64, 30, 10), "recorded before the snapshot was taken (span 19)");
    }

    #[test]
    fn the_pools_preparation_reward_is_for_a_proof_that_landed_before_the_seed_existed() {
        // The anchor of the span before the audit: the pool's rule as it was — a proof of span S − 2 or earlier.
        assert_eq!(palw_anchor_window_prep_landed_by_v1(100, 99), Some(98));
        // An older anchor moves the bound back with it: a proof that landed after the seed existed is not a preparation.
        assert_eq!(palw_anchor_window_prep_landed_by_v1(100, 80), Some(79));
        assert_eq!(palw_anchor_window_prep_landed_by_v1(100, 98), Some(97), "an anchor two spans back: one span before it");
        assert_eq!(palw_anchor_window_prep_landed_by_v1(100, 0), Some(0), "saturating, never a panic");
        assert_eq!(palw_anchor_window_prep_landed_by_v1(1, 0), None, "no span two before span 1");
    }

    #[test]
    fn the_companion_values_are_the_recommended_variant() {
        assert_eq!(palw_anchor_window_value_v1(), [24, 1], "W = 24 with M1 in; the conservative variant is [64, 0]");
    }
}
