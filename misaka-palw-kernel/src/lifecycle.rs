//! **RFC-0011 §15.5: a constraint-verified claim's lifecycle, its disputes, and the network's error budget (§15.4).**
//!
//! ```text
//! Committed → ChallengeBound → Checking → ProbabilisticPass → WindowClosed → Final
//!                                  └─ counted failure / filed challenge → Disputed → Convicted | (dismissed: back)
//!                                  └─ missing evidence / deadline        → Unavailable | TimedOut
//! ```
//!
//! * `ProbabilisticPass` needs the tally's **complete** per-segment coverage ([`crate::receipt::TallyStateV1::Covered`]): silence,
//!   missing rounds, absent constraints or unavailable witnesses never count as a pass. A deadline reached without it is `TimedOut`.
//! * `Final` needs the pass, the challenge window elapsed, the DA/retention obligation met and **no open dispute**.
//! * A dispute may be filed by **any** public bond (ADR-0173 D1) up to the window's end, also against a passed claim; it blocks Final
//!   until its exact court verdict. A conviction voids the claim (the producer's fraud); a dismissal returns the claim to where it was.
//! * Missing material is `Unavailable` — an availability outcome, never an arithmetic conviction; whether the producer defaulted on
//!   a demand is recorded separately and is the only ground the DA path gives for blaming it.
//!
//! [`DisputeBudgetV1`] bounds what disputes can force: per-claim and per-accuser open disputes and the total court work in flight
//! (§15.5: rate-limit bonded disputes and bound simultaneous court exposure).

use std::collections::BTreeMap;

use crate::hash::Digest;
use crate::receipt::TallyStateV1;

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum ClaimStateV1 {
    Committed = 0,
    ChallengeBound {
        anchor_daa: u64,
    } = 1,
    Checking {
        anchor_daa: u64,
        deadline_daa: u64,
    } = 2,
    ProbabilisticPass {
        passed_daa: u64,
        window_end_daa: u64,
    } = 3,
    WindowClosed {
        window_end_daa: u64,
    } = 4,
    Final {
        final_daa: u64,
    } = 5,
    /// Open disputes; `resume` is the state a dismissal of the last one returns to.
    Disputed {
        open: u32,
        resume: Box<ClaimStateV1>,
    } = 6,
    Convicted {
        daa: u64,
    } = 7,
    Unavailable {
        daa: u64,
        producer_defaulted: bool,
    } = 8,
    TimedOut {
        daa: u64,
    } = 9,
}

impl ClaimStateV1 {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Final { .. } | Self::Convicted { .. } | Self::Unavailable { .. } | Self::TimedOut { .. })
    }
}

/// The versioned timing a claim is bound to at its binding (a fence mid-window never reinterprets it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct LifecyclePolicyV1 {
    /// From the anchor to the receipts' deadline.
    pub check_window_daa: u64,
    /// From the pass to the end of the challenge window.
    pub challenge_window_daa: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimEventV1 {
    /// The challenge anchor is fixed (after the claim's commitments are bound).
    BindChallenge { anchor_daa: u64 },
    /// The beacon exists: duties start.
    StartChecking { daa: u64 },
    /// The tally as it stands at `daa`.
    Tally { daa: u64, state: TallyStateV1 },
    /// A public bond filed a fault proof or a challenge.
    DisputeFiled { daa: u64 },
    /// The exact court's verdict on one open dispute.
    CourtVerdict { daa: u64, convicted: bool },
    /// The DA path concluded: material the claim needs is not available (and whether the producer defaulted on a demand).
    MaterialUnavailable { daa: u64, producer_defaulted: bool },
    /// Time passes (deadlines and windows are checked against it).
    Tick { daa: u64 },
    /// The retention/DA obligation for Final is met.
    RetentionMet,
    /// Material was served for a demand at some `daa`: the claim may not reach Final before `until_daa` (`daa + proof grace`), so
    /// the prosecution the served values enable can still be filed before Final. Holds only ever extend to the latest service, and a
    /// service can only happen inside the bounded demand window, so the hold never moves Final past
    /// `window end + court deadline + proof grace`.
    ProofGrace { until_daa: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleErrorV1 {
    #[error("{event} is not valid in state {state}")]
    Invalid { state: String, event: String },
}

/// One claim's lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ClaimLifecycleV1 {
    pub policy: LifecyclePolicyV1,
    pub state: ClaimStateV1,
    pub retention_met: bool,
    /// The earliest DAA at which Final may be reached (the latest service + proof grace; `0`: no hold).
    pub final_hold_until: u64,
}

impl ClaimLifecycleV1 {
    pub fn new(policy: LifecyclePolicyV1) -> Self {
        Self { policy, state: ClaimStateV1::Committed, retention_met: false, final_hold_until: 0 }
    }

    fn invalid(&self, e: &ClaimEventV1) -> LifecycleErrorV1 {
        LifecycleErrorV1::Invalid { state: format!("{:?}", self.state), event: format!("{e:?}") }
    }

    /// Apply one event. Terminal states accept nothing.
    pub fn apply(&mut self, e: ClaimEventV1) -> Result<&ClaimStateV1, LifecycleErrorV1> {
        use ClaimEventV1 as E;
        use ClaimStateV1 as S;
        if self.state.is_terminal() {
            return Err(self.invalid(&e));
        }
        let next = match (&self.state, &e) {
            (_, E::RetentionMet) => {
                self.retention_met = true;
                self.finalize_if_due(None)
            }
            (_, E::ProofGrace { until_daa }) => {
                self.final_hold_until = self.final_hold_until.max(*until_daa);
                self.state.clone()
            }
            (S::Committed, E::BindChallenge { anchor_daa }) => S::ChallengeBound { anchor_daa: *anchor_daa },
            (S::ChallengeBound { anchor_daa }, E::StartChecking { daa }) if daa >= anchor_daa => {
                S::Checking { anchor_daa: *anchor_daa, deadline_daa: anchor_daa + self.policy.check_window_daa }
            }
            (S::Checking { deadline_daa, .. }, E::Tally { daa, state }) => match state {
                TallyStateV1::Covered if daa <= deadline_daa => {
                    S::ProbabilisticPass { passed_daa: *daa, window_end_daa: daa + self.policy.challenge_window_daa }
                }
                TallyStateV1::Disputed { .. } => S::Disputed { open: 1, resume: Box::new(self.state.clone()) },
                TallyStateV1::Unavailable { .. } if daa > deadline_daa => S::Unavailable { daa: *daa, producer_defaulted: false },
                _ if daa > deadline_daa => S::TimedOut { daa: *daa },
                _ => self.state.clone(),
            },
            (S::Checking { deadline_daa, .. }, E::Tick { daa }) if daa > deadline_daa => S::TimedOut { daa: *daa },
            (S::ProbabilisticPass { window_end_daa, .. }, E::Tick { daa }) if daa >= window_end_daa => {
                S::WindowClosed { window_end_daa: *window_end_daa }
            }
            // A challenge may be filed until the window closes, also against a passed claim.
            (S::Checking { .. } | S::ProbabilisticPass { .. }, E::DisputeFiled { daa }) if self.window_open(*daa) => {
                S::Disputed { open: 1, resume: Box::new(self.state.clone()) }
            }
            (S::Disputed { open, resume }, E::DisputeFiled { .. }) => S::Disputed { open: open + 1, resume: resume.clone() },
            // The Panel's coverage may land while a dispute is open: it moves the state the dispute resumes to (the pass and its
            // window start now), and Final stays blocked until the dispute's verdict.
            (S::Disputed { open, resume }, E::Tally { daa, state: TallyStateV1::Covered }) => match &**resume {
                S::Checking { deadline_daa, .. } if daa <= deadline_daa => S::Disputed {
                    open: *open,
                    resume: Box::new(S::ProbabilisticPass {
                        passed_daa: *daa,
                        window_end_daa: daa + self.policy.challenge_window_daa,
                    }),
                },
                _ => self.state.clone(),
            },
            (S::Disputed { .. }, E::CourtVerdict { daa, convicted: true }) => S::Convicted { daa: *daa },
            (S::Disputed { open, resume }, E::CourtVerdict { convicted: false, .. }) => {
                if *open > 1 {
                    S::Disputed { open: open - 1, resume: resume.clone() }
                } else {
                    (**resume).clone()
                }
            }
            (
                S::Checking { .. } | S::ProbabilisticPass { .. } | S::Disputed { .. },
                E::MaterialUnavailable { daa, producer_defaulted },
            ) => S::Unavailable { daa: *daa, producer_defaulted: *producer_defaulted },
            (_, E::Tick { .. }) => self.state.clone(),
            _ => return Err(self.invalid(&e)),
        };
        self.state = next;
        let tick = match e {
            E::Tick { daa } => Some(daa),
            _ => None,
        };
        self.state = self.finalize_if_due(tick);
        Ok(&self.state)
    }

    fn window_open(&self, daa: u64) -> bool {
        match &self.state {
            ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => daa < *window_end_daa,
            _ => true,
        }
    }

    /// Final needs the closed window, the retention obligation, no open dispute (a `Disputed` state is never finalized here) and
    /// the proof grace after the latest service to have elapsed (without a clock reading, `window_end` is the clock).
    fn finalize_if_due(&self, daa: Option<u64>) -> ClaimStateV1 {
        match (&self.state, daa) {
            (ClaimStateV1::WindowClosed { window_end_daa }, d) if self.retention_met => {
                let at = d.unwrap_or(*window_end_daa).max(*window_end_daa);
                if at >= self.final_hold_until { ClaimStateV1::Final { final_daa: at } } else { self.state.clone() }
            }
            _ => self.state.clone(),
        }
    }
}

/// The bounds on what disputes can force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisputeLimitsV1 {
    pub max_open_per_claim: u32,
    pub max_open_per_accuser: u32,
    /// The court work (the plan's worst court per dispute) all open disputes may hold at once.
    pub max_court_work_in_flight: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DisputeRefusalV1 {
    #[error("the claim already has {0} open disputes")]
    ClaimFull(u32),
    #[error("the accuser already has {0} open disputes")]
    AccuserFull(u32),
    #[error("{need} court work would exceed the {limit} in flight")]
    CourtFull { need: u128, limit: u128 },
    #[error("the accuser holds no Active public bond")]
    NoBond,
}

/// Open disputes across claims. Any Active public bond may file (no Panel seat required — ADR-0173 D1); the bounds are counts and
/// work, never a permission.
#[derive(Clone, Debug)]
pub struct DisputeBudgetV1 {
    pub limits: DisputeLimitsV1,
    per_claim: BTreeMap<Digest, u32>,
    per_accuser: BTreeMap<Digest, u32>,
    in_flight: u128,
}

impl DisputeBudgetV1 {
    pub fn new(limits: DisputeLimitsV1) -> Self {
        Self { limits, per_claim: BTreeMap::new(), per_accuser: BTreeMap::new(), in_flight: 0 }
    }

    pub fn file(
        &mut self,
        claim: Digest,
        accuser: Digest,
        accuser_bond_active: bool,
        court_work: u128,
    ) -> Result<(), DisputeRefusalV1> {
        if !accuser_bond_active {
            return Err(DisputeRefusalV1::NoBond);
        }
        let c = self.per_claim.get(&claim).copied().unwrap_or(0);
        if c >= self.limits.max_open_per_claim {
            return Err(DisputeRefusalV1::ClaimFull(c));
        }
        let a = self.per_accuser.get(&accuser).copied().unwrap_or(0);
        if a >= self.limits.max_open_per_accuser {
            return Err(DisputeRefusalV1::AccuserFull(a));
        }
        let need = self.in_flight.saturating_add(court_work);
        if need > self.limits.max_court_work_in_flight {
            return Err(DisputeRefusalV1::CourtFull { need, limit: self.limits.max_court_work_in_flight });
        }
        *self.per_claim.entry(claim).or_default() += 1;
        *self.per_accuser.entry(accuser).or_default() += 1;
        self.in_flight = need;
        Ok(())
    }

    /// A verdict releases the dispute's counts and work.
    pub fn resolve(&mut self, claim: Digest, accuser: Digest, court_work: u128) {
        if let Some(c) = self.per_claim.get_mut(&claim) {
            *c = c.saturating_sub(1);
        }
        if let Some(a) = self.per_accuser.get_mut(&accuser) {
            *a = a.saturating_sub(1);
        }
        self.in_flight = self.in_flight.saturating_sub(court_work);
    }

    pub fn in_flight(&self) -> u128 {
        self.in_flight
    }
}

/// **RFC-0011 §15.4's network bound**, as bits: `P(any false acceptance) ≤ min(1, Q·ε_check + ε_env)` for `Q` adversarial attempts
/// (claims, reorg retries, grinding trials) at a per-claim `ε_check = 2^-check_bits` and a separately stated bad-assumption event
/// `ε_env = 2^-env_bits` (compromised accepting Panel, biased beacon, selective DA, censorship). Conservative: `min(a, b) − 1` bits
/// for the sum of `2^-a` and `2^-b`. The algebraic target alone is never the network's security level.
pub fn network_false_acceptance_bits_v1(check_bits: u16, attempts: u128, env_bits: u16) -> u16 {
    let log2_q = if attempts <= 1 { 0 } else { 128 - (attempts - 1).leading_zeros() as i64 };
    let a = check_bits as i64 - log2_q;
    let b = env_bits as i64;
    (a.min(b) - 1).clamp(0, u16::MAX as i64) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> LifecyclePolicyV1 {
        LifecyclePolicyV1 { check_window_daa: 100, challenge_window_daa: 50 }
    }

    fn checking() -> ClaimLifecycleV1 {
        let mut l = ClaimLifecycleV1::new(policy());
        l.apply(ClaimEventV1::BindChallenge { anchor_daa: 10 }).unwrap();
        l.apply(ClaimEventV1::StartChecking { daa: 12 }).unwrap();
        l
    }

    #[test]
    fn a_covered_claim_finalizes_only_after_its_window_and_retention() {
        let mut l = checking();
        l.apply(ClaimEventV1::Tally { daa: 20, state: TallyStateV1::Covered }).unwrap();
        assert_eq!(l.state, ClaimStateV1::ProbabilisticPass { passed_daa: 20, window_end_daa: 70 });
        l.apply(ClaimEventV1::Tick { daa: 69 }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::ProbabilisticPass { .. }), "the window is open");
        l.apply(ClaimEventV1::Tick { daa: 70 }).unwrap();
        assert_eq!(l.state, ClaimStateV1::WindowClosed { window_end_daa: 70 }, "no Final without retention");
        l.apply(ClaimEventV1::RetentionMet).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Final { .. }));
        assert!(l.apply(ClaimEventV1::DisputeFiled { daa: 80 }).is_err(), "terminal");
    }

    #[test]
    fn a_service_holds_final_until_its_proof_grace_has_elapsed() {
        let mut l = checking();
        l.apply(ClaimEventV1::Tally { daa: 20, state: TallyStateV1::Covered }).unwrap(); // window end 70
        l.apply(ClaimEventV1::ProofGrace { until_daa: 75 }).unwrap();
        l.apply(ClaimEventV1::ProofGrace { until_daa: 72 }).unwrap(); // an earlier service never shortens the hold
        l.apply(ClaimEventV1::Tick { daa: 70 }).unwrap();
        assert_eq!(l.state, ClaimStateV1::WindowClosed { window_end_daa: 70 }, "the window closed but a service holds Final");
        l.apply(ClaimEventV1::RetentionMet).unwrap();
        l.apply(ClaimEventV1::Tick { daa: 74 }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::WindowClosed { .. }));
        l.apply(ClaimEventV1::Tick { daa: 75 }).unwrap();
        assert_eq!(l.state, ClaimStateV1::Final { final_daa: 75 });
    }

    #[test]
    fn silence_and_partial_coverage_never_pass() {
        let mut l = checking();
        l.apply(ClaimEventV1::Tally { daa: 50, state: TallyStateV1::Incomplete { uncovered: vec![2] } }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Checking { .. }));
        l.apply(ClaimEventV1::Tick { daa: 111 }).unwrap();
        assert_eq!(l.state, ClaimStateV1::TimedOut { daa: 111 }, "a deadline without coverage is a timeout, not a pass");
        let mut l = checking();
        l.apply(ClaimEventV1::Tally { daa: 111, state: TallyStateV1::Covered }).unwrap();
        assert_eq!(l.state, ClaimStateV1::TimedOut { daa: 111 }, "coverage after the deadline is late");
    }

    #[test]
    fn a_dispute_blocks_final_a_dismissal_resumes_and_a_conviction_voids() {
        let mut l = checking();
        l.apply(ClaimEventV1::Tally { daa: 20, state: TallyStateV1::Covered }).unwrap();
        l.apply(ClaimEventV1::DisputeFiled { daa: 30 }).unwrap();
        l.apply(ClaimEventV1::DisputeFiled { daa: 31 }).unwrap();
        l.apply(ClaimEventV1::RetentionMet).unwrap();
        l.apply(ClaimEventV1::Tick { daa: 500 }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Disputed { open: 2, .. }), "open disputes block Final whatever the clock");
        l.apply(ClaimEventV1::CourtVerdict { daa: 501, convicted: false }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Disputed { open: 1, .. }));
        l.apply(ClaimEventV1::CourtVerdict { daa: 502, convicted: false }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::ProbabilisticPass { .. }), "dismissed: back to the pass");
        l.apply(ClaimEventV1::Tick { daa: 503 }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Final { .. }));

        let mut l = checking();
        l.apply(ClaimEventV1::Tally { daa: 20, state: TallyStateV1::Covered }).unwrap();
        assert!(l.apply(ClaimEventV1::DisputeFiled { daa: 70 }).is_err(), "a challenge after the window is refused");
        let mut l = checking();
        l.apply(ClaimEventV1::Tally {
            daa: 20,
            state: TallyStateV1::Disputed {
                first: ([1; 64], crate::receipt::ReceiptVerdictV1::Fail { position: 0, occurrence: 0, node: 0 }),
            },
        })
        .unwrap();
        l.apply(ClaimEventV1::CourtVerdict { daa: 21, convicted: true }).unwrap();
        assert_eq!(l.state, ClaimStateV1::Convicted { daa: 21 });
    }

    #[test]
    fn coverage_during_a_dispute_starts_the_window_and_final_waits_for_the_verdict() {
        let mut l = checking();
        l.apply(ClaimEventV1::DisputeFiled { daa: 15 }).unwrap();
        l.apply(ClaimEventV1::Tally { daa: 20, state: TallyStateV1::Covered }).unwrap();
        l.apply(ClaimEventV1::RetentionMet).unwrap();
        l.apply(ClaimEventV1::Tick { daa: 300 }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Disputed { open: 1, .. }), "no Final under an open dispute");
        l.apply(ClaimEventV1::CourtVerdict { daa: 301, convicted: false }).unwrap();
        assert_eq!(l.state, ClaimStateV1::ProbabilisticPass { passed_daa: 20, window_end_daa: 70 });
        l.apply(ClaimEventV1::Tick { daa: 302 }).unwrap();
        assert!(matches!(l.state, ClaimStateV1::Final { .. }));
        // Late coverage under a dispute changes nothing: the dismissal returns to Checking, which then times out.
        let mut l = checking();
        l.apply(ClaimEventV1::DisputeFiled { daa: 15 }).unwrap();
        l.apply(ClaimEventV1::Tally { daa: 111, state: TallyStateV1::Covered }).unwrap();
        l.apply(ClaimEventV1::CourtVerdict { daa: 112, convicted: false }).unwrap();
        l.apply(ClaimEventV1::Tick { daa: 113 }).unwrap();
        assert_eq!(l.state, ClaimStateV1::TimedOut { daa: 113 });
    }

    #[test]
    fn missing_material_is_the_da_path_not_a_conviction() {
        let mut l = checking();
        l.apply(ClaimEventV1::MaterialUnavailable { daa: 40, producer_defaulted: true }).unwrap();
        assert_eq!(l.state, ClaimStateV1::Unavailable { daa: 40, producer_defaulted: true });
    }

    #[test]
    fn disputes_are_open_to_any_bond_and_bounded_by_count_and_court_work() {
        let mut b =
            DisputeBudgetV1::new(DisputeLimitsV1 { max_open_per_claim: 2, max_open_per_accuser: 1, max_court_work_in_flight: 100 });
        assert_eq!(b.file([1; 64], [9; 64], false, 10), Err(DisputeRefusalV1::NoBond));
        b.file([1; 64], [9; 64], true, 60).unwrap();
        assert_eq!(b.file([2; 64], [9; 64], true, 10), Err(DisputeRefusalV1::AccuserFull(1)));
        assert!(matches!(b.file([1; 64], [8; 64], true, 50), Err(DisputeRefusalV1::CourtFull { .. })));
        b.file([1; 64], [8; 64], true, 40).unwrap();
        assert_eq!(b.file([1; 64], [7; 64], true, 0), Err(DisputeRefusalV1::ClaimFull(2)));
        b.resolve([1; 64], [9; 64], 60);
        assert_eq!(b.in_flight(), 40);
        b.file([2; 64], [9; 64], true, 10).unwrap();
    }

    #[test]
    fn the_network_bound_charges_attempts_and_the_environment() {
        assert_eq!(network_false_acceptance_bits_v1(200, 1, 300), 199);
        assert_eq!(network_false_acceptance_bits_v1(200, 1 << 40, 300), 159);
        assert_eq!(network_false_acceptance_bits_v1(200, 1 << 40, 30), 29, "a weak environment dominates the algebra");
    }
}
