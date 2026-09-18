//! **Protocol time** — ADR-0140 Decisions 4, 5 and 6.
//!
//! Every deadline in this system is stated in DAA score. That was the same thing as elapsed time
//! while one cadence paced the chain, and ADR-0138 is the ADR that made it stop being the same
//! thing: past `palw_anchor_clock` the DAA advances only where something priced it, so a window of
//! 6,000 DAA is 200 hours at the target cadence and an unbounded number of hours at any other.
//!
//! **Nothing here decides a rule.** Three things are built:
//!
//! * [`ProtocolTimeV1`] — one view of "when is it", so a subsystem reads time from one place
//!   instead of reaching for a DAA score, a header timestamp or a median-time-past on its own
//!   (D4). Building the view is what makes the inventory below possible at all.
//! * [`palw_deadline_registry_v1`] — every deadline the chain enforces, its basis today, the basis
//!   it could have, its nominal length in each, and the margin it was sized with (D5). The output
//!   is a table an operator decides from. Moving everything to elapsed time is NOT the goal: some
//!   deadlines are counted in blocks because what they bound is a number of blocks.
//! * [`palw_deadline_shadow_v1`] — both answers computed, only the existing one authoritative, the
//!   difference recorded (D6). A migration is then argued from a season of production data rather
//!   than from a projection.
//!
//! ## The trap this module has to state in its own header
//!
//! Median-time-past is **not** an external clock. ADR-0064 Fact A: a branch sees only the
//! timestamps it declares itself, bounded by the MTP rule below and the future-drift rule above.
//! So moving deadlines onto elapsed time does not reduce the chain's dependence on the clock lane's
//! price — it *increases* it, because a cheaply forged heartbeat history would then move deadlines
//! as well as ordering. ADR-0140 §5 says the two questions must be decided in that order, never the
//! reverse, and this module exists to make the first one measurable, not to pre-empt the second.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// **When it is, as one value.** The two clocks a rule may legitimately read, carried together so
/// that a caller cannot silently pick one.
///
/// `daa_score` is the chain's own counter, which past ADR-0138 counts only blocks a price paced.
/// `mtp_ms` is the median time past of the block being evaluated, in milliseconds — declared by the
/// branch, bounded by the timestamp rules, and not an oracle (see the module header).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ProtocolTimeV1 {
    pub daa_score: u64,
    pub mtp_ms: u64,
}

impl ProtocolTimeV1 {
    #[inline]
    pub const fn new(daa_score: u64, mtp_ms: u64) -> Self {
        Self { daa_score, mtp_ms }
    }

    /// DAA elapsed since `earlier`, saturating. Never negative: a reorg can move the virtual
    /// backwards, and a deadline that read a wrapped difference would fire on everything at once.
    #[inline]
    pub const fn daa_since(&self, earlier: &Self) -> u64 {
        self.daa_score.saturating_sub(earlier.daa_score)
    }

    /// Milliseconds elapsed since `earlier`, saturating. The MTP is monotonic along a chain by the
    /// timestamp rules, but a reorg is not along a chain, so the same guard applies.
    #[inline]
    pub const fn ms_since(&self, earlier: &Self) -> u64 {
        self.mtp_ms.saturating_sub(earlier.mtp_ms)
    }
}

/// Which clock a deadline is counted in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum TimeBasisV1 {
    /// Counted in DAA score — a number of blocks a price paced.
    Daa,
    /// Counted in elapsed protocol time, from the median time past.
    ElapsedMs,
}

/// Every deadline the chain enforces, named. One variant per rule, because the point of the
/// registry is that a decision is taken per deadline and not for all of them at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum DeadlineKindV1 {
    /// How long a claim's panel may still legally bind (`window_bind`).
    Bind,
    /// How long a seat has to file its receipt (`window_receipt`).
    Receipt,
    /// How long a licensed claim stays challengeable (`window_challenge`, shortened past
    /// ADR-0132 §7.6's fence).
    Challenge,
    /// The per-session court budget (`window_court`).
    Court,
    /// How long a free-prompt claim sits on abandon hold (`fp_abandon_hold_daa`).
    AbandonHold,
    /// How long a bond's collateral stays locked after a withdrawal is requested.
    BondExit,
    /// ADR-0128: how long a validator may be silent before its bond leaks (`t_leak_daa`).
    ValidatorLeak,
    /// ADR-0128: how deep a DNS-final anchor must be buried before re-entry counts
    /// (`reentry_final_depth_daa`).
    ReentryDepth,
    /// The class economy's epoch (`epoch_length`).
    Epoch,
    /// ADR-0130 M2: the execution lane's span.
    Span,
}

impl DeadlineKindV1 {
    /// A stable short name for logs and reports.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Bind => "bind",
            Self::Receipt => "receipt",
            Self::Challenge => "challenge",
            Self::Court => "court",
            Self::AbandonHold => "abandon_hold",
            Self::BondExit => "bond_exit",
            Self::ValidatorLeak => "validator_leak",
            Self::ReentryDepth => "reentry_depth",
            Self::Epoch => "epoch",
            Self::Span => "span",
        }
    }
}

/// **One row of the registry** (ADR-0140 D5). What this deadline is, what it is counted in today,
/// what it could be counted in, how long it is in each, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeadlineSpecV1 {
    pub kind: DeadlineKindV1,
    /// What the rule reads today. `Daa` for every row: that is the finding, not an omission.
    pub basis_today: TimeBasisV1,
    /// What it could read. `Daa` where the thing being bounded really is a number of blocks.
    pub candidate_basis: TimeBasisV1,
    /// Its length in DAA, as the shipped ruleset states it.
    pub nominal_daa: u64,
    /// The same length in milliseconds AT THE TARGET CADENCE — which is what the number was sized
    /// against, and which the chain only actually runs at when something paces it at that rate.
    pub nominal_ms_at_target: u64,
    /// One line on why the candidate basis is what it is. Read by a human, not by a rule.
    pub rationale: &'static str,
}

/// **The registry** — every deadline, from the shipped values rather than from a table written by
/// hand, so a ruleset change shows up here instead of drifting away from here.
///
/// `target_time_per_block_ms` is the network's own cadence; the millisecond column is the DAA
/// column times that, which is the arithmetic the original sizing used.
pub fn palw_deadline_registry_v1(
    state: &crate::palw_state_v2::PalwStateParamsV2,
    target_time_per_block_ms: u64,
    withdrawal_delay_daa: u64,
    span_daa: u64,
    dns: Option<&crate::dns_bft_v1::DnsBftRulesV1>,
) -> Vec<DeadlineSpecV1> {
    let ms = |daa: u64| daa.saturating_mul(target_time_per_block_ms);
    let row = |kind: DeadlineKindV1, candidate: TimeBasisV1, daa: u64, rationale: &'static str| DeadlineSpecV1 {
        kind,
        basis_today: TimeBasisV1::Daa,
        candidate_basis: candidate,
        nominal_daa: daa,
        nominal_ms_at_target: ms(daa),
        rationale,
    };
    let mut rows = vec![
        row(
            DeadlineKindV1::Bind,
            TimeBasisV1::ElapsedMs,
            state.window_bind(),
            "a panel needs WALL-CLOCK to assemble and sign; nothing about it is a count of blocks",
        ),
        row(
            DeadlineKindV1::Receipt,
            TimeBasisV1::ElapsedMs,
            state.window_receipt(),
            "a seat needs wall-clock to replay the job — ADR-0133 sizes this against replay seconds",
        ),
        row(
            DeadlineKindV1::Challenge,
            TimeBasisV1::ElapsedMs,
            state.window_challenge(),
            "a challenger needs wall-clock to notice and to assemble; the risk it bounds is time-shaped",
        ),
        row(
            DeadlineKindV1::Court,
            TimeBasisV1::ElapsedMs,
            state.window_court(),
            "ADR-0092 already decided the court is bound by a wall clock; the budget should agree",
        ),
        row(DeadlineKindV1::AbandonHold, TimeBasisV1::ElapsedMs, state.fp_abandon_hold_daa(), "a hold is a wait, and a wait is time"),
        row(
            DeadlineKindV1::BondExit,
            TimeBasisV1::ElapsedMs,
            withdrawal_delay_daa,
            "the delay exists so a challenge can still land: it must outlive the challenge window in the SAME unit",
        ),
        row(
            DeadlineKindV1::Epoch,
            TimeBasisV1::Daa,
            state.epoch_length(),
            "KEEP IN DAA: the epoch budgets a number of BLOCKS' worth of draws, so it is a block count by construction",
        ),
        row(
            DeadlineKindV1::Span,
            TimeBasisV1::Daa,
            span_daa,
            "KEEP IN DAA: a span schedules permits per block, and its width is a count of blocks",
        ),
    ];
    if let Some(dns) = dns {
        rows.push(row(
            DeadlineKindV1::ValidatorLeak,
            TimeBasisV1::ElapsedMs,
            dns.t_leak_daa,
            "silence is measured in time; ADR-0138 §3b already had to widen its evidence window for exactly this",
        ));
        rows.push(row(
            DeadlineKindV1::ReentryDepth,
            TimeBasisV1::Daa,
            dns.reentry_final_depth_daa,
            "KEEP IN DAA: a burial depth is a number of blocks, which is the thing it is protecting against",
        ));
    }
    rows
}

/// **One deadline, evaluated both ways** (ADR-0140 D6). `legacy_expired` is what the chain acts on.
/// `candidate_expired` is what it would have acted on had the deadline been counted in elapsed
/// time. Nothing reads `candidate_expired` but a log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeadlineShadowV1 {
    pub kind: DeadlineKindV1,
    /// The rule in force. This and only this decides.
    pub legacy_expired: bool,
    /// What the elapsed-time rule would have said.
    pub candidate_expired: bool,
    /// DAA elapsed since the deadline started.
    pub daa_elapsed: u64,
    /// Milliseconds elapsed since it started, by median time past.
    pub ms_elapsed: u64,
    /// `ms_elapsed` as a permille of the nominal duration, so a report can say "this expired at 42 %
    /// of the wall-clock it was sized for" without recomputing.
    pub ms_of_nominal_permille: u64,
}

impl DeadlineShadowV1 {
    /// The two rules disagree here — the only rows a migration report has to look at.
    #[inline]
    pub const fn disagrees(&self) -> bool {
        self.legacy_expired != self.candidate_expired
    }
}

/// Evaluate one deadline under both bases. Pure: no store, no wall clock, no host state.
pub fn palw_deadline_shadow_v1(spec: &DeadlineSpecV1, created: &ProtocolTimeV1, now: &ProtocolTimeV1) -> DeadlineShadowV1 {
    let daa_elapsed = now.daa_since(created);
    let ms_elapsed = now.ms_since(created);
    DeadlineShadowV1 {
        kind: spec.kind,
        legacy_expired: daa_elapsed >= spec.nominal_daa,
        candidate_expired: ms_elapsed >= spec.nominal_ms_at_target,
        daa_elapsed,
        ms_elapsed,
        ms_of_nominal_permille: if spec.nominal_ms_at_target == 0 {
            0
        } else {
            (ms_elapsed as u128 * 1_000 / spec.nominal_ms_at_target as u128).min(u64::MAX as u128) as u64
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(daa: u64, ms: u64) -> DeadlineSpecV1 {
        DeadlineSpecV1 {
            kind: DeadlineKindV1::Challenge,
            basis_today: TimeBasisV1::Daa,
            candidate_basis: TimeBasisV1::ElapsedMs,
            nominal_daa: daa,
            nominal_ms_at_target: ms,
            rationale: "fixture",
        }
    }

    /// The shadow answers both questions and only one of them is the rule. The interesting case is
    /// the one the whole exercise is about: a chain running SLOWER than its target cadence, where
    /// the DAA deadline has not fired and the elapsed-time one has.
    #[test]
    fn the_shadow_computes_both_and_only_the_legacy_one_is_the_rule() {
        let s = spec(120, 120 * 120_000); // 120 DAA, sized at a 120-second cadence = 4 hours
        let created = ProtocolTimeV1::new(1_000, 0);

        // A chain at exactly the target cadence: the two agree everywhere.
        let on_cadence = ProtocolTimeV1::new(1_120, 120 * 120_000);
        let sh = palw_deadline_shadow_v1(&s, &created, &on_cadence);
        assert!(sh.legacy_expired && sh.candidate_expired && !sh.disagrees());

        // A chain running at a third of the cadence — ADR-0138's testnet-11, roughly. Four hours of
        // wall clock have passed and only 40 DAA: the elapsed rule fires, the DAA rule does not.
        let slow = ProtocolTimeV1::new(1_040, 120 * 120_000);
        let sh = palw_deadline_shadow_v1(&s, &created, &slow);
        assert!(!sh.legacy_expired, "the rule in force has NOT expired");
        assert!(sh.candidate_expired, "the elapsed-time rule would have");
        assert!(sh.disagrees());
        assert_eq!(sh.daa_elapsed, 40);
        assert_eq!(sh.ms_of_nominal_permille, 1_000);

        // And the other direction: a burst chain, where the DAA rule fires early in wall-clock.
        let fast = ProtocolTimeV1::new(1_120, 12 * 120_000);
        let sh = palw_deadline_shadow_v1(&s, &created, &fast);
        assert!(sh.legacy_expired && !sh.candidate_expired && sh.disagrees());
        assert_eq!(sh.ms_of_nominal_permille, 100, "it expired at a tenth of the wall clock it was sized for");
    }

    /// A reorg moves the virtual backwards. A difference that wrapped would expire every deadline
    /// at once, which is the worst possible failure for a sweep.
    #[test]
    fn time_never_runs_backwards_into_a_wrap() {
        let s = spec(120, 14_400_000);
        let later = ProtocolTimeV1::new(1_000, 1_000_000);
        let earlier = ProtocolTimeV1::new(2_000, 2_000_000);
        let sh = palw_deadline_shadow_v1(&s, &earlier, &later);
        assert_eq!((sh.daa_elapsed, sh.ms_elapsed), (0, 0));
        assert!(!sh.legacy_expired && !sh.candidate_expired);
    }

    /// A deadline of zero is a deadline that has always passed, and the permille must not divide.
    #[test]
    fn a_zero_deadline_does_not_divide_by_zero() {
        let s = spec(0, 0);
        let sh = palw_deadline_shadow_v1(&s, &ProtocolTimeV1::new(1, 1), &ProtocolTimeV1::new(1, 1));
        assert!(sh.legacy_expired && sh.candidate_expired);
        assert_eq!(sh.ms_of_nominal_permille, 0);
    }

    /// The registry is the inventory ADR-0140 D5 asks for, and the two rows that stay in DAA are
    /// the finding: not everything is a duration.
    #[test]
    fn the_registry_names_every_deadline_and_keeps_the_block_counts_in_daa() {
        let params = crate::config::params::palw_rc_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 runs V2")
        };
        let rows = palw_deadline_registry_v1(&bundle.state, params.target_time_per_block, 7_500, 5, None);
        assert!(rows.iter().all(|r| r.basis_today == TimeBasisV1::Daa), "every deadline is counted in DAA today");

        let by = |k: DeadlineKindV1| rows.iter().find(|r| r.kind == k).expect("row").clone();
        assert_eq!(by(DeadlineKindV1::Epoch).candidate_basis, TimeBasisV1::Daa, "an epoch budgets blocks");
        assert_eq!(by(DeadlineKindV1::Span).candidate_basis, TimeBasisV1::Daa, "a span schedules per block");
        for k in [DeadlineKindV1::Bind, DeadlineKindV1::Receipt, DeadlineKindV1::Challenge, DeadlineKindV1::BondExit] {
            assert_eq!(by(k).candidate_basis, TimeBasisV1::ElapsedMs, "{}: a wait is time", k.name());
        }

        // The millisecond column is the DAA column at the network's own cadence, which is the
        // arithmetic the original sizing used — so a ruleset change moves both columns together.
        let challenge = by(DeadlineKindV1::Challenge);
        assert_eq!(challenge.nominal_ms_at_target, challenge.nominal_daa * params.target_time_per_block);
        assert!(challenge.nominal_daa > 0, "the shipped ruleset has a challenge window");

        // Every name is distinct, so a report cannot collapse two rows into one line.
        let mut names: Vec<&str> = rows.iter().map(|r| r.kind.name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
    }

    /// The DNS rows appear only where the network has the overlay, and the burial depth stays a
    /// count of blocks for the same reason a span does.
    #[test]
    fn the_dns_rows_are_present_only_with_the_overlay() {
        let params = crate::config::params::palw_rc_shipped_params();
        let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
            panic!("testnet-11 runs V2")
        };
        let without = palw_deadline_registry_v1(&bundle.state, params.target_time_per_block, 7_500, 5, None);
        assert!(!without.iter().any(|r| r.kind == DeadlineKindV1::ValidatorLeak));

        let dns = crate::dns_bft_v1::DnsBftRulesV1 {
            t_leak_daa: 5_040,
            reentry_final_depth_daa: 200,
            min_retained_validators: 4,
            evidence_window_blue_score: 5_240,
            epoch_length_blue_score: 100,
            anchor_backoff_blue_score: 10,
        };
        let with = palw_deadline_registry_v1(&bundle.state, params.target_time_per_block, 7_500, 5, Some(&dns));
        assert_eq!(with.len(), without.len() + 2);
        let leak = with.iter().find(|r| r.kind == DeadlineKindV1::ValidatorLeak).expect("row");
        assert_eq!((leak.candidate_basis, leak.nominal_daa), (TimeBasisV1::ElapsedMs, 5_040));
        let depth = with.iter().find(|r| r.kind == DeadlineKindV1::ReentryDepth).expect("row");
        assert_eq!(depth.candidate_basis, TimeBasisV1::Daa, "a burial depth is a number of blocks");
    }
}
