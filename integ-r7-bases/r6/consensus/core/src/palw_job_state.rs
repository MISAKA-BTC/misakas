//! ADR-0037: the asynchronous PALW job state machine — pure transition core.
//!
//! Decision 2 of ADR-0037 replaces the per-block challenge-horizon walk with one
//! pruning-surviving state object per job. This module is that object's *transition
//! algebra*: a closed, monotone status machine and the verdict semantics that make
//! invariants I8/I9/I10 structural instead of reviewable:
//!
//! * **I8** — a well-formed refutation LOCKS a job (`ChallengeWindow → Disputed`); no
//!   event sequence containing only a refutation reaches a reward-destroying terminal.
//! * **I9** — the only reward/bond-destroying terminals are [`PalwJobStatusV3::Convicted`]
//!   (exact primitive conviction, [`PalwJobEventV3::ExactConviction`]) and the objective
//!   no-show ([`PalwJobEventV3::DeadlineMissed`] before commitment/panel duty was met).
//! * **I10** — [`PalwJobStatusV3::Unadjudicable`] slashes no one
//!   ([`PalwJobStatusV3::slashes_executor`] is `false`), zeroes the job's inflation
//!   credit ([`PalwJobStatusV3::creditable`] is `false`), and demands a class freeze
//!   ([`PalwJobStatusV3::demands_class_freeze`]), because reaching it proves the class's
//!   catalog-completeness claim was false.
//!
//! Everything here is arithmetic over the caller's facts — no store handle, no clock, no
//! registry. Deadlines, signatures, panel membership and budget are checked by the
//! consumer *before* it emits an event; this machine only answers "is that event legal in
//! this status, and what status follows". Same shape as [`crate::palw_credit`]: pure,
//! deterministic, byte-identical between construction and validation.
//!
//! Consensus-inert: nothing constructs [`PalwJobStateV3`] on any shipped network. The
//! Track-C change set (ADR-0037 §Implementation order) wires it; per that ADR, partial
//! *activation* is forbidden — landing the algebra early is how the change set is built,
//! activating a subset of it is how fail-opens are preserved.

use crate::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;

/// The closed status set of ADR-0037 Decision 2, with the provisional polarity carried
/// where the diagram needs it (a `ChallengeWindow` that forgot whether the panel
/// provisionally accepted could not finalize to the right side without re-reading
/// history — the exact pattern this machine exists to end).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwJobStatusV3 {
    /// Escrow posted, no commitment yet.
    Open,
    /// First commitment accepted (first-accepted-wins; a second commitment is not an
    /// event, it is an admission error upstream).
    Committed,
    /// The future anchor landed and the panel is bound.
    PanelSelected,
    /// q-of-n sampled verification agreed (`true`) or refused agreement (`false`).
    Provisional { accepted: bool },
    /// The challenge window is open over a provisional verdict.
    ChallengeWindow { provisionally_accepted: bool },
    /// A well-formed refutation locked the job (I8). Reward is frozen, not destroyed.
    Disputed { provisionally_accepted: bool },
    /// The court is bisecting toward one primitive.
    Adjudicating { provisionally_accepted: bool },
    /// Terminal: window closed with no dispute over an accepting provisional verdict.
    FinalizedAccepted,
    /// Terminal: window closed over a rejecting provisional verdict, or the executor
    /// objectively failed to show (I9's second arm).
    FinalizedRejected,
    /// Terminal: the court reproduced a divergent primitive — exact-bit guilt (I9).
    Convicted,
    /// Terminal: the court reproduced the executor's bits; the challenger was wrong.
    NoFaultFound,
    /// Terminal: the court could not reduce the dispute to a catalogued primitive.
    /// No slash (I10), no credit, class freeze demanded.
    Unadjudicable,
}

/// Events the consumer may feed the machine. Each names the *chain fact* that licenses
/// it; the machine trusts the caller established that fact (deadline math, signature
/// validity, panel membership are consumer-entry checks per ADR-0037 Decision 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwJobEventV3 {
    /// The job's first commitment was accepted on-chain.
    CommitmentAccepted,
    /// The commitment's future anchor block finalized and the panel derives from it.
    PanelAnchored,
    /// The panel reached quorum: `accepted` per the q-of-n root comparison.
    QuorumReached { accepted: bool },
    /// The provisional verdict's challenge window opened.
    ChallengeWindowOpened,
    /// The dual deadline (DAA ∧ MTP, ADR-0037 Decision 4) passed with no dispute filed.
    ChallengeWindowClosed,
    /// A well-formed, bonded refutation was accepted inside the window.
    RefutationFiled,
    /// The dispute escalated into the bisection court.
    AdjudicationOpened,
    /// The court's one-primitive CPU ruling: the executor's claimed bits are wrong.
    ExactConviction,
    /// The court's one-primitive CPU ruling: the executor's claimed bits reproduce.
    ExecutorVindicated,
    /// The court could not reduce the dispute to a catalogued primitive.
    AdjudicationImpossible,
    /// An objective deadline the *executor or panel* owed was missed (no commitment in
    /// time, no quorum in time). Only legal before a provisional verdict exists — after
    /// that, silence is what `ChallengeWindowClosed` is for.
    DeadlineMissed,
}

/// A transition refusal: the event is not legal in the current status. The machine is
/// closed — an illegal event is a caller bug or an attack, never a silent no-op (I7's
/// spirit: absence of a legal transition is an error, not an empty answer).
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
#[error("PALW job event {event:?} is not legal in status {status:?}")]
pub struct PalwJobTransitionError {
    pub status: PalwJobStatusV3,
    pub event: PalwJobEventV3,
}

impl PalwJobStatusV3 {
    /// The one transition function. Total over (status, event); returns the successor or
    /// a refusal. No event is ever interpreted two ways, and terminals accept nothing.
    pub fn apply(self, event: PalwJobEventV3) -> Result<PalwJobStatusV3, PalwJobTransitionError> {
        use PalwJobEventV3 as E;
        use PalwJobStatusV3 as S;
        let refused = PalwJobTransitionError { status: self, event };
        Ok(match (self, event) {
            (S::Open, E::CommitmentAccepted) => S::Committed,
            (S::Open, E::DeadlineMissed) => S::FinalizedRejected,
            (S::Committed, E::PanelAnchored) => S::PanelSelected,
            (S::Committed, E::DeadlineMissed) => S::FinalizedRejected,
            (S::PanelSelected, E::QuorumReached { accepted }) => S::Provisional { accepted },
            (S::PanelSelected, E::DeadlineMissed) => S::FinalizedRejected,
            (S::Provisional { accepted }, E::ChallengeWindowOpened) => S::ChallengeWindow { provisionally_accepted: accepted },
            (S::ChallengeWindow { provisionally_accepted: true }, E::ChallengeWindowClosed) => S::FinalizedAccepted,
            (S::ChallengeWindow { provisionally_accepted: false }, E::ChallengeWindowClosed) => S::FinalizedRejected,
            // I8: the refutation locks — it does not finalize, reject, or erase.
            (S::ChallengeWindow { provisionally_accepted }, E::RefutationFiled) => S::Disputed { provisionally_accepted },
            (S::Disputed { provisionally_accepted }, E::AdjudicationOpened) => S::Adjudicating { provisionally_accepted },
            // I9: exact conviction is the only guilty verdict the court can return.
            (S::Adjudicating { .. }, E::ExactConviction) => S::Convicted,
            (S::Adjudicating { .. }, E::ExecutorVindicated) => S::NoFaultFound,
            // I10: the court may also refuse to rule — and that refusal is terminal.
            (S::Adjudicating { .. }, E::AdjudicationImpossible) => S::Unadjudicable,
            _ => return Err(refused),
        })
    }

    /// Strictly increases along every legal transition — the machine's monotonicity
    /// witness (no cycles, no reopening). Polarity does not affect rank.
    pub fn rank(self) -> u8 {
        use PalwJobStatusV3 as S;
        match self {
            S::Open => 0,
            S::Committed => 1,
            S::PanelSelected => 2,
            S::Provisional { .. } => 3,
            S::ChallengeWindow { .. } => 4,
            S::Disputed { .. } => 5,
            S::Adjudicating { .. } => 6,
            S::FinalizedAccepted | S::FinalizedRejected | S::Convicted | S::NoFaultFound | S::Unadjudicable => 7,
        }
    }

    /// Terminal statuses accept no events, ever.
    pub fn is_terminal(self) -> bool {
        self.rank() == 7
    }

    /// May this job's inflation credit be minted? Only two terminals say yes: a clean
    /// finalize on the accepting side, or a court that vindicated the executor.
    /// `Unadjudicable` is `false` — credit zero without slash (I10).
    pub fn creditable(self) -> bool {
        matches!(self, PalwJobStatusV3::FinalizedAccepted | PalwJobStatusV3::NoFaultFound)
    }

    /// May the executor's bond be slashed? Exactly one status: `Convicted` (I9).
    /// `FinalizedRejected` refuses credit and forfeits the *job*, but a rejection or a
    /// no-show destroys the reward, not the bond, unless a separate objective-no-show
    /// bond rule (a consumer policy, not this machine) says otherwise.
    pub fn slashes_executor(self) -> bool {
        matches!(self, PalwJobStatusV3::Convicted)
    }

    /// Does this terminal demand the execution class be auto-frozen? Only
    /// `Unadjudicable`: reaching it proves the class's catalog-completeness claim false
    /// (ADR-0037 Decision 2), so new jobs on the class must stop until a new class
    /// version passes re-audit.
    pub fn demands_class_freeze(self) -> bool {
        matches!(self, PalwJobStatusV3::Unadjudicable)
    }
}

/// Deadlines are dual on purpose (ADR-0037 Decision 4): one saturated mergeset can jump
/// DAA without wall time passing, and a timestamp alone can be nudged. An action is due
/// only when BOTH have passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDualDeadlineV3 {
    /// `anchor_daa + min_daa_delta`.
    pub due_daa: u64,
    /// `anchor_mtp + min_seconds` (past-median-time, seconds).
    pub due_mtp: u64,
}

impl PalwDualDeadlineV3 {
    /// True iff the deadline has passed under BOTH clocks.
    pub fn expired(&self, current_daa: u64, past_median_time: u64) -> bool {
        current_daa >= self.due_daa && past_median_time >= self.due_mtp
    }
}

/// The per-job deadline set the consumer reads before emitting `DeadlineMissed` /
/// `ChallengeWindowClosed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDeadlinesV3 {
    /// A commitment must be accepted by here or the job no-shows.
    pub commit_by: PalwDualDeadlineV3,
    /// The panel must reach quorum by here.
    pub quorum_by: PalwDualDeadlineV3,
    /// The challenge window closes here; only after it may a provisional verdict finalize.
    pub challenge_close: PalwDualDeadlineV3,
}

/// ADR-0037 Decision 2's job object — every fact a consumer needs, bound once at each
/// transition, so no consumer ever re-derives a payee, a panel, or a root from history.
/// Payees resolve ONLY through the recorded bond outpoints (I4); `validator_id`s follow
/// the [`crate::palw_carriage`] idiom (the bond registry resolves keys, state carries
/// their identity hash).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwJobStateV3 {
    /// `H("MISAKA/PALW/JOB/V3" ‖ network_id ‖ request_txid ‖ request_output_index ‖
    /// requester_nonce ‖ model_band_id)` — the identity everything else binds to.
    pub job_id: Hash64,
    /// The full request context digest carried into every signature (Decision 3).
    pub job_context_hash: Hash64,
    /// What quality the requester bought (ADR-0034 band).
    pub model_band_id: Hash64,
    /// What environment consensus can adjudicate (ADR-0034 class; exact, never a family).
    pub execution_class_id: Hash64,
    /// The escrow-funding output; refunds return here.
    pub requester_outpoint: TransactionOutpoint,
    /// The executor's bond — the ONLY source of the executor payout script (I4).
    pub executor_bond_outpoint: TransactionOutpoint,
    /// The executor's registry identity (key resolution is the registry's job).
    pub executor_validator_id: Hash64,
    pub commitment_root: Hash64,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    /// The future anchor the panel derives from (Decision 4).
    pub commitment_anchor_hash: Hash64,
    /// The eligible-set snapshot at that anchor.
    pub eligible_set_snapshot_id: Hash64,
    /// The bound panel — the ONLY source of attester payout scripts (I4).
    pub selected_verifier_bond_outpoints: Vec<TransactionOutpoint>,
    pub status: PalwJobStatusV3,
    pub deadlines: PalwDeadlinesV3,
    /// User escrow (sompi), paid from ordinary UTXOs; winner+panel+refund+fee ≤ this.
    pub user_escrow_amount: u64,
    /// The inflation-credit ceiling this job may draw from the epoch budget (sompi).
    pub max_inflation_credit: u64,
    /// One bit per selected verifier plus the executor's bit 0; a set bit is a paid
    /// claim, and a job credits at most once per claimant (I3).
    pub reward_claimed_bitmap: u32,
}

impl PalwJobStateV3 {
    /// Advance the job. The status algebra decides legality; the state object stays
    /// otherwise untouched — fields are bound at their own admission points, never
    /// rewritten by a transition.
    pub fn apply(&mut self, event: PalwJobEventV3) -> Result<(), PalwJobTransitionError> {
        self.status = self.status.apply(event)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PalwJobEventV3 as E;
    use PalwJobStatusV3 as S;

    /// Every status the machine can inhabit, both polarities where one exists.
    fn all_statuses() -> Vec<S> {
        let mut v = vec![
            S::Open,
            S::Committed,
            S::PanelSelected,
            S::FinalizedAccepted,
            S::FinalizedRejected,
            S::Convicted,
            S::NoFaultFound,
            S::Unadjudicable,
        ];
        for accepted in [false, true] {
            v.push(S::Provisional { accepted });
            v.push(S::ChallengeWindow { provisionally_accepted: accepted });
            v.push(S::Disputed { provisionally_accepted: accepted });
            v.push(S::Adjudicating { provisionally_accepted: accepted });
        }
        v
    }

    fn all_events() -> Vec<E> {
        vec![
            E::CommitmentAccepted,
            E::PanelAnchored,
            E::QuorumReached { accepted: true },
            E::QuorumReached { accepted: false },
            E::ChallengeWindowOpened,
            E::ChallengeWindowClosed,
            E::RefutationFiled,
            E::AdjudicationOpened,
            E::ExactConviction,
            E::ExecutorVindicated,
            E::AdjudicationImpossible,
            E::DeadlineMissed,
        ]
    }

    /// The machine is monotone: every legal transition strictly increases rank, so no
    /// cycle and no reopening is expressible at all.
    #[test]
    fn every_legal_transition_strictly_increases_rank() {
        for s in all_statuses() {
            for e in all_events() {
                if let Ok(next) = s.apply(e) {
                    assert!(next.rank() > s.rank(), "{s:?} --{e:?}--> {next:?} does not increase rank");
                }
            }
        }
    }

    /// Terminals are absorbing: no event whatsoever is legal in a terminal status.
    #[test]
    fn terminals_accept_no_events() {
        for s in all_statuses().into_iter().filter(|s| s.is_terminal()) {
            for e in all_events() {
                assert_eq!(s.apply(e), Err(PalwJobTransitionError { status: s, event: e }), "terminal {s:?} accepted {e:?}");
            }
        }
    }

    /// The happy path, accepting side: Open → … → FinalizedAccepted, creditable, no slash.
    #[test]
    fn clean_accept_path_credits_and_slashes_nobody() {
        let mut s = S::Open;
        for e in [
            E::CommitmentAccepted,
            E::PanelAnchored,
            E::QuorumReached { accepted: true },
            E::ChallengeWindowOpened,
            E::ChallengeWindowClosed,
        ] {
            s = s.apply(e).unwrap();
        }
        assert_eq!(s, S::FinalizedAccepted);
        assert!(s.creditable());
        assert!(!s.slashes_executor());
        assert!(!s.demands_class_freeze());
    }

    /// The rejecting provisional verdict finalizes to the rejecting side — the window
    /// remembers its polarity without re-reading history.
    #[test]
    fn rejecting_window_finalizes_rejected() {
        let s = S::ChallengeWindow { provisionally_accepted: false }.apply(E::ChallengeWindowClosed).unwrap();
        assert_eq!(s, S::FinalizedRejected);
        assert!(!s.creditable());
    }

    /// I8: a refutation locks. From `ChallengeWindow`, `RefutationFiled` reaches
    /// `Disputed` — never a terminal — and no event sequence that contains a refutation
    /// but no court ruling reaches ANY terminal.
    #[test]
    fn i8_refutation_locks_never_destroys() {
        for accepted in [false, true] {
            let s = S::ChallengeWindow { provisionally_accepted: accepted }.apply(E::RefutationFiled).unwrap();
            assert_eq!(s, S::Disputed { provisionally_accepted: accepted });
            assert!(!s.is_terminal());
            // From Disputed, the only exits are into the court; closing the window or a
            // second refutation are illegal, so the locked reward cannot silently die.
            assert!(s.apply(E::ChallengeWindowClosed).is_err());
            assert!(s.apply(E::RefutationFiled).is_err());
            assert!(s.apply(E::DeadlineMissed).is_err());
        }
    }

    /// I9: the only route into `Convicted` is `ExactConviction` from `Adjudicating`, and
    /// the only route into a slash is `Convicted`.
    #[test]
    fn i9_only_exact_conviction_slashes() {
        for s in all_statuses() {
            for e in all_events() {
                if let Ok(next) = s.apply(e)
                    && next.slashes_executor()
                {
                    assert!(matches!(s, S::Adjudicating { .. }), "slash reached from {s:?}");
                    assert_eq!(e, E::ExactConviction, "slash reached via {e:?}");
                }
            }
        }
        // And the no-show arm destroys the reward, not the bond.
        for s in [S::Open, S::Committed, S::PanelSelected] {
            let t = s.apply(E::DeadlineMissed).unwrap();
            assert_eq!(t, S::FinalizedRejected);
            assert!(!t.slashes_executor());
        }
    }

    /// `DeadlineMissed` is illegal once a provisional verdict exists — silence after
    /// that point is `ChallengeWindowClosed`'s meaning, and conflating the two would
    /// let a stalled court masquerade as a no-show.
    #[test]
    fn deadline_missed_is_pre_provisional_only() {
        for accepted in [false, true] {
            for s in [
                S::Provisional { accepted },
                S::ChallengeWindow { provisionally_accepted: accepted },
                S::Disputed { provisionally_accepted: accepted },
                S::Adjudicating { provisionally_accepted: accepted },
            ] {
                assert!(s.apply(E::DeadlineMissed).is_err(), "{s:?} accepted DeadlineMissed");
            }
        }
    }

    /// I10: `Unadjudicable` credits nothing, slashes nobody, and demands the class
    /// freeze. `NoFaultFound` credits (the executor was right all along).
    #[test]
    fn i10_unadjudicable_no_slash_no_credit_freezes_class() {
        let u = S::Adjudicating { provisionally_accepted: true }.apply(E::AdjudicationImpossible).unwrap();
        assert_eq!(u, S::Unadjudicable);
        assert!(!u.slashes_executor());
        assert!(!u.creditable());
        assert!(u.demands_class_freeze());

        let v = S::Adjudicating { provisionally_accepted: false }.apply(E::ExecutorVindicated).unwrap();
        assert_eq!(v, S::NoFaultFound);
        assert!(v.creditable());
        assert!(!v.slashes_executor());
        assert!(!v.demands_class_freeze());
    }

    /// The dual deadline requires BOTH clocks: DAA alone (one saturated mergeset) or
    /// MTP alone (a nudged timestamp) is not expiry.
    #[test]
    fn dual_deadline_requires_both_clocks() {
        let d = PalwDualDeadlineV3 { due_daa: 100, due_mtp: 1_000 };
        assert!(!d.expired(100, 999), "DAA alone must not expire");
        assert!(!d.expired(99, 1_000), "MTP alone must not expire");
        assert!(d.expired(100, 1_000));
    }

    /// Exhaustive reachability: from `Open`, exactly the intended terminal set is
    /// reachable, and every non-terminal has at least one legal continuation (no dead
    /// ends short of a terminal).
    #[test]
    fn reachable_terminals_are_exactly_the_intended_five() {
        let mut seen = std::collections::BTreeSet::new();
        let mut frontier = vec![S::Open];
        let mut terminals = std::collections::BTreeSet::new();
        while let Some(s) = frontier.pop() {
            if !seen.insert(format!("{s:?}")) {
                continue;
            }
            let nexts: Vec<S> = all_events().into_iter().filter_map(|e| s.apply(e).ok()).collect();
            if s.is_terminal() {
                assert!(nexts.is_empty());
                terminals.insert(format!("{s:?}"));
            } else {
                assert!(!nexts.is_empty(), "non-terminal {s:?} is a dead end");
                frontier.extend(nexts);
            }
        }
        let expected: std::collections::BTreeSet<String> =
            ["FinalizedAccepted", "FinalizedRejected", "Convicted", "NoFaultFound", "Unadjudicable"]
                .into_iter()
                .map(String::from)
                .collect();
        assert_eq!(terminals, expected);
    }

    /// The state object delegates legality to the algebra and touches nothing else.
    #[test]
    fn state_apply_moves_status_only() {
        let mut job = PalwJobStateV3 {
            job_id: Hash64::from_u64_word(1),
            job_context_hash: Hash64::from_u64_word(2),
            model_band_id: Hash64::from_u64_word(3),
            execution_class_id: Hash64::from_u64_word(4),
            requester_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(5), 0),
            executor_bond_outpoint: TransactionOutpoint::new(Hash64::from_u64_word(6), 0),
            executor_validator_id: Hash64::from_u64_word(7),
            commitment_root: Hash64::from_u64_word(8),
            trace_root: Hash64::from_u64_word(9),
            output_root: Hash64::from_u64_word(10),
            commitment_anchor_hash: Hash64::from_u64_word(11),
            eligible_set_snapshot_id: Hash64::from_u64_word(12),
            selected_verifier_bond_outpoints: vec![TransactionOutpoint::new(Hash64::from_u64_word(13), 1)],
            status: S::Open,
            deadlines: PalwDeadlinesV3 {
                commit_by: PalwDualDeadlineV3 { due_daa: 10, due_mtp: 10 },
                quorum_by: PalwDualDeadlineV3 { due_daa: 20, due_mtp: 20 },
                challenge_close: PalwDualDeadlineV3 { due_daa: 30, due_mtp: 30 },
            },
            user_escrow_amount: 1_000,
            max_inflation_credit: 500,
            reward_claimed_bitmap: 0,
        };
        let before = job.clone();
        job.apply(E::CommitmentAccepted).unwrap();
        assert_eq!(job.status, S::Committed);
        assert_eq!(job.reward_claimed_bitmap, before.reward_claimed_bitmap);
        assert_eq!(job.selected_verifier_bond_outpoints, before.selected_verifier_bond_outpoints);
        // An illegal event refuses and leaves the status alone.
        assert!(job.apply(E::ExactConviction).is_err());
        assert_eq!(job.status, S::Committed);
    }
}
