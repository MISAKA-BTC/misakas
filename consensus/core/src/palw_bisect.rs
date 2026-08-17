//! PALW bisection ladder v1 — ADR-0027 §1's degraded path, as a pure state machine.
//!
//! The direct route (`palw_step_refute`) needs the miner's own commitments to open. When the
//! miner has withheld intermediate state — a bare-v2 or composite-v1 commitment, or a v2 one
//! whose openings go unanswered — the challenger cannot name a step with openings. The
//! ladder forces disclosure incrementally: each rung, the RESPONDER discloses a state
//! commitment at the pinned midpoint of the disputed interval; the CHALLENGER agrees (the
//! divergence is in the upper half) or disagrees (lower half); ≈ log₂(space) rungs later the
//! interval is one index wide and the dispute terminates in the same one-step (or one-call)
//! check the direct route uses. **Non-response within `W_round` at any rung is the objective
//! offense** (v0.1 §17.1 `M-O3` / ADR-0027: withholding is a faster loss, not an escape).
//!
//! What is deliberately pinned here: the index-space contract, the midpoint function, the
//! round bound, every transition's legality, and the terminal handoff shape. What is
//! deliberately NOT here: carriage (ladder messages ride ADR-0029 rails at Stage 1),
//! deadlines' calendar values (ADR-0028 §3 owns the windows; this machine records DAA
//! deadlines it is given), and the terminal adjudication itself (`palw_step_refute` for
//! step-space ladders; the per-call replay check for event-space ones).
//!
//! Consensus-inert: nothing consumes this yet.

use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_hashes::Hash64;
use thiserror::Error;

// ---------------------------------------------------------------------------------------------
// Domains, caps
// ---------------------------------------------------------------------------------------------

pub const PALW_BISECT_OBJECT_VERSION_V1: u16 = 1;

pub const PALW_BISECT_DOMAIN_SESSION_ID: &[u8] = b"misaka-palw/bisection-session-id/v1";
pub const PALW_BISECT_DOMAIN_OFFENSE_ID: &[u8] = b"misaka-palw/bisection-offense-id/v1";

pub const PALW_BISECT_ALL_DOMAINS: &[&[u8]] = &[PALW_BISECT_DOMAIN_SESSION_ID, PALW_BISECT_DOMAIN_OFFENSE_ID];

/// Hard bound on rungs: a 2^40 space is 40 rungs; anything needing more is a malformed
/// space, not a longer dispute. (ADR-0028 sized ladders at ≈ 20 rungs for 10⁶ steps.)
pub const PALW_BISECT_MAX_ROUNDS: u32 = 48;
/// Largest index space a ladder may open (the step-space cap, with margin for event spaces).
pub const PALW_BISECT_MAX_SPACE: u64 = 1 << 40;

/// Which index space the ladder walks — the terminal check differs per space, and mixing
/// them mid-dispute is meaningless.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwBisectSpaceV1 {
    /// The `palw_step` leaf space of a v3 profile (terminal = `ExecutionStepRefutationV1`).
    StepLeaves = 0,
    /// The v2 trace event space, 0..D (terminal = one-call replay adjudication).
    TraceEvents = 1,
}

/// The pinned midpoint: `lo + (hi − lo)/2`, integer floor. One function, one witness test —
/// a responder and challenger that derive different midpoints are not in the same dispute.
pub fn bisect_midpoint_v1(lo: u64, hi: u64) -> u64 {
    debug_assert!(lo < hi);
    lo + (hi - lo) / 2
}

/// Session identity: the dispute is ABOUT one committed root, between two identities, over
/// one space. Rung messages bind this id, so a message cannot be replayed across disputes.
pub fn bisect_session_id_v1(
    job_context_hash: &Hash64,
    committed_root: &Hash64,
    challenger_id: &Hash64,
    responder_id: &Hash64,
    space: PalwBisectSpaceV1,
    space_size: u64,
) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BISECT_DOMAIN_SESSION_ID).to_state();
    h.update(job_context_hash.as_byte_slice());
    h.update(committed_root.as_byte_slice());
    h.update(challenger_id.as_byte_slice());
    h.update(responder_id.as_byte_slice());
    h.update(&[space as u8]);
    h.update(&space_size.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwBisectError {
    #[error("w_round is zero — every move would be instantly overdue, so no ladder can be played")]
    ZeroRungWindow,
    #[error("unsupported bisect object version {got} (expected {expected})")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("index space size {got} is zero, one, or over the {max} cap")]
    SpaceOutOfRange { got: u64, max: u64 },
    #[error("the message does not belong to this session")]
    SessionMismatch,
    #[error("the message is for round {got}, but the ladder is at round {expected}")]
    RoundMismatch { got: u32, expected: u32 },
    #[error("a {expected} message is required next, got {got}")]
    TurnMismatch { expected: &'static str, got: &'static str },
    #[error("the ladder is terminal; no further rungs are legal")]
    AlreadyTerminal,
    #[error("the deadline for this rung ({deadline}) is not after the previous one ({previous})")]
    DeadlineNotMonotonic { deadline: u64, previous: u64 },
    #[error("no-show can only be declared after the rung deadline ({deadline}); observed DAA is {observed}")]
    DeadlineNotReached { deadline: u64, observed: u64 },
    #[error("round budget exceeded — a legal ladder narrows to one index within the bound")]
    RoundBudgetExceeded,
}

// ---------------------------------------------------------------------------------------------
// Wire messages (Stage-1 carriage bodies; consensus-inert today)
// ---------------------------------------------------------------------------------------------

/// The responder's rung: "my execution's state commitment at the midpoint is `mid_state`".
/// What a "state commitment at index i" means is a property of the SPACE (the v2 event
/// chain's running digest; a step-space state root) — pinned at registration with the
/// commitment form; the machine treats it as an opaque, later-openable claim.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBisectDisclosureV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub round: u32,
    pub midpoint: u64,
    pub mid_state: Hash64,
}

/// The challenger's rung verdict: `agree = true` ⇒ the prefix up to the midpoint matches
/// (the divergence is in the upper half); `false` ⇒ lower half.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBisectVerdictV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub round: u32,
    pub agree: bool,
}

/// Who failed to move within their rung window. An offense record is only mintable from the
/// machine's own state — the deadline and whose turn it was are machine facts, not claims.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwBisectPartyV1 {
    Responder = 0,
    Challenger = 1,
}

/// The `M-O3` objective offense: silence past a rung deadline, attributable and deduplicable.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBisectNoShowV1 {
    pub version: u16,
    pub session_id: Hash64,
    pub round: u32,
    pub silent_party: PalwBisectPartyV1,
    pub deadline_daa: u64,
    pub observed_daa: u64,
}

pub fn bisect_offense_id_v1(offense: &PalwBisectNoShowV1) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_BISECT_DOMAIN_OFFENSE_ID).to_state();
    h.update(offense.session_id.as_byte_slice());
    h.update(&offense.round.to_le_bytes());
    h.update(&[offense.silent_party as u8]);
    h.update(&offense.deadline_daa.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------------------------

/// Whose move it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBisectTurnV1 {
    /// The responder must disclose the current midpoint's state commitment.
    AwaitDisclosure,
    /// The challenger must agree/disagree with the last disclosure.
    AwaitVerdict,
    /// The interval is one index wide: the responder must open that index's input state for
    /// the terminal one-step / one-call check. The ladder's job is done.
    Terminal,
    /// A party went silent past its rung deadline and the dispute is decided against them. An
    /// absorbing state: no later move is legal, and no second no-show is chargeable.
    Abandoned,
}

/// The full ladder state. Every observer feeding it the same message stream derives the same
/// state — that is what makes rung silence an OBJECTIVE offense.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwBisectLadderV1 {
    session_id: Hash64,
    space_size: u64,
    lo: u64,
    hi: u64,
    round: u32,
    turn: PalwBisectTurnV1,
    last_deadline_daa: u64,
    /// Disclosed midpoint states, rung-ordered — the terminal check's anchor pair comes from
    /// here (the states bracketing the final index).
    pub disclosures: Vec<(u64, Hash64)>,
}

impl PalwBisectLadderV1 {
    /// Opens a dispute over `[0, space_size)`. `first_deadline_daa` is the responder's first
    /// rung window (ADR-0028 §3 sizing; the machine only requires monotonicity).
    pub fn open(
        job_context_hash: &Hash64,
        committed_root: &Hash64,
        challenger_id: &Hash64,
        responder_id: &Hash64,
        space: PalwBisectSpaceV1,
        space_size: u64,
        opened_at_daa: u64,
        first_deadline_daa: u64,
    ) -> Result<Self, PalwBisectError> {
        if !(2..=PALW_BISECT_MAX_SPACE).contains(&space_size) {
            return Err(PalwBisectError::SpaceOutOfRange { got: space_size, max: PALW_BISECT_MAX_SPACE });
        }
        if first_deadline_daa <= opened_at_daa {
            return Err(PalwBisectError::DeadlineNotMonotonic { deadline: first_deadline_daa, previous: opened_at_daa });
        }
        Ok(Self {
            session_id: bisect_session_id_v1(job_context_hash, committed_root, challenger_id, responder_id, space, space_size),
            space_size,
            lo: 0,
            hi: space_size,
            round: 0,
            turn: PalwBisectTurnV1::AwaitDisclosure,
            last_deadline_daa: first_deadline_daa,
            disclosures: Vec::new(),
        })
    }

    /// One rung window past the block that accepted the move.
    ///
    /// A zero `w_round` would make every move instantly overdue, so it is refused here rather
    /// than trusted from a registration that got it wrong — the same shape as every other window
    /// rule in ADR-0028 §3.
    fn rung_deadline(accepted_daa: u64, w_round: u64) -> Result<u64, PalwBisectError> {
        if w_round == 0 {
            return Err(PalwBisectError::ZeroRungWindow);
        }
        Ok(accepted_daa.saturating_add(w_round))
    }

    pub fn session_id(&self) -> Hash64 {
        self.session_id
    }

    pub fn turn(&self) -> PalwBisectTurnV1 {
        self.turn
    }

    pub fn round(&self) -> u32 {
        self.round
    }

    /// The disputed interval `[lo, hi)`.
    pub fn interval(&self) -> (u64, u64) {
        (self.lo, self.hi)
    }

    /// The index the terminal check adjudicates (only meaningful at `Terminal`).
    pub fn terminal_index(&self) -> Option<u64> {
        (self.turn == PalwBisectTurnV1::Terminal).then_some(self.lo)
    }

    /// The midpoint the next disclosure must be about.
    pub fn expected_midpoint(&self) -> Option<u64> {
        (self.turn == PalwBisectTurnV1::AwaitDisclosure).then(|| bisect_midpoint_v1(self.lo, self.hi))
    }

    /// Applies the responder's disclosure.
    /// Applies the responder's disclosure. `accepted_daa` is the DAA of the block that accepted
    /// the move and `w_round` is the class's pinned rung window — the new deadline is their sum.
    ///
    /// **Neither party supplies a deadline.** They used to: the disclosure carried the
    /// challenger's verdict deadline and the verdict carried the responder's next one, with only
    /// a monotonicity check between them. That let the party moving set its opponent's clock to
    /// one DAA and win by expiry (2026-08-17 re-audit). A deadline is now a fact about the chain
    /// and the registered window, which is also what finally connects
    /// [`crate::palw_schedule::PalwScheduleParamsV1::w_round`] to the ladder it was sized for.
    pub fn apply_disclosure(&mut self, msg: &PalwBisectDisclosureV1, accepted_daa: u64, w_round: u64) -> Result<(), PalwBisectError> {
        if msg.version != PALW_BISECT_OBJECT_VERSION_V1 {
            return Err(PalwBisectError::UnsupportedVersion { got: msg.version, expected: PALW_BISECT_OBJECT_VERSION_V1 });
        }
        if msg.session_id != self.session_id {
            return Err(PalwBisectError::SessionMismatch);
        }
        match self.turn {
            PalwBisectTurnV1::AwaitDisclosure => {}
            PalwBisectTurnV1::AwaitVerdict => return Err(PalwBisectError::TurnMismatch { expected: "verdict", got: "disclosure" }),
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwBisectError::AlreadyTerminal),
        }
        if msg.round != self.round {
            return Err(PalwBisectError::RoundMismatch { got: msg.round, expected: self.round });
        }
        let mid = bisect_midpoint_v1(self.lo, self.hi);
        if msg.midpoint != mid {
            // A wrong-midpoint "disclosure" is not a protocol move at all.
            return Err(PalwBisectError::TurnMismatch { expected: "the pinned midpoint", got: "another index" });
        }
        let deadline = Self::rung_deadline(accepted_daa, w_round)?;
        self.disclosures.push((mid, msg.mid_state));
        self.last_deadline_daa = deadline;
        self.turn = PalwBisectTurnV1::AwaitVerdict;
        Ok(())
    }

    /// Applies the challenger's verdict, narrowing the interval.
    /// Applies the challenger's verdict, narrowing the interval. Deadlines are chain-derived,
    /// exactly as in [`Self::apply_disclosure`].
    pub fn apply_verdict(&mut self, msg: &PalwBisectVerdictV1, accepted_daa: u64, w_round: u64) -> Result<(), PalwBisectError> {
        if msg.version != PALW_BISECT_OBJECT_VERSION_V1 {
            return Err(PalwBisectError::UnsupportedVersion { got: msg.version, expected: PALW_BISECT_OBJECT_VERSION_V1 });
        }
        if msg.session_id != self.session_id {
            return Err(PalwBisectError::SessionMismatch);
        }
        match self.turn {
            PalwBisectTurnV1::AwaitVerdict => {}
            PalwBisectTurnV1::AwaitDisclosure => return Err(PalwBisectError::TurnMismatch { expected: "disclosure", got: "verdict" }),
            PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => return Err(PalwBisectError::AlreadyTerminal),
        }
        if msg.round != self.round {
            return Err(PalwBisectError::RoundMismatch { got: msg.round, expected: self.round });
        }
        // Everything that can refuse is decided BEFORE any field moves. The previous version
        // narrowed the interval and incremented the round, then returned `RoundBudgetExceeded` —
        // leaving a half-applied ladder in a machine whose whole point is that every observer
        // derives the identical state (2026-08-17 re-audit).
        let deadline = Self::rung_deadline(accepted_daa, w_round)?;
        let next_round = self.round + 1;
        if next_round > PALW_BISECT_MAX_ROUNDS {
            return Err(PalwBisectError::RoundBudgetExceeded);
        }
        let mid = bisect_midpoint_v1(self.lo, self.hi);
        if msg.agree {
            self.lo = mid;
        } else {
            self.hi = mid;
        }
        self.round = next_round;
        self.last_deadline_daa = deadline;
        self.turn = if self.hi - self.lo <= 1 { PalwBisectTurnV1::Terminal } else { PalwBisectTurnV1::AwaitDisclosure };
        Ok(())
    }

    /// Mints the no-show offense for the party whose move is overdue **and ends the ladder**.
    ///
    /// It used to take `&self` and leave the machine exactly where it was, so a session could be
    /// declared no-show repeatedly and still accept moves afterwards — a game with no terminal
    /// state (2026-08-17 re-audit). Silence past a rung deadline is the objective offense that
    /// DECIDES the dispute (`M-O3`), so the ladder transitions to
    /// [`PalwBisectTurnV1::Abandoned`] and refuses every later move.
    pub fn declare_no_show(&mut self, observed_daa: u64) -> Result<PalwBisectNoShowV1, PalwBisectError> {
        if observed_daa <= self.last_deadline_daa {
            return Err(PalwBisectError::DeadlineNotReached { deadline: self.last_deadline_daa, observed: observed_daa });
        }
        let silent_party = match self.turn {
            PalwBisectTurnV1::AwaitDisclosure => PalwBisectPartyV1::Responder,
            PalwBisectTurnV1::AwaitVerdict => PalwBisectPartyV1::Challenger,
            // Terminal: the responder owes the terminal opening — same window discipline.
            PalwBisectTurnV1::Terminal => PalwBisectPartyV1::Responder,
            // An abandoned ladder has already been decided; there is no second silence to charge.
            PalwBisectTurnV1::Abandoned => return Err(PalwBisectError::AlreadyTerminal),
        };
        self.turn = PalwBisectTurnV1::Abandoned;
        Ok(PalwBisectNoShowV1 {
            version: PALW_BISECT_OBJECT_VERSION_V1,
            session_id: self.session_id,
            round: self.round,
            silent_party,
            deadline_daa: self.last_deadline_daa,
            observed_daa,
        })
    }
}

// =============================================================================================
// Tests
// =============================================================================================

#[cfg(test)]
mod tests {
    /// A pinned rung window for the tests — the real one comes from the class registration.
    const W_ROUND: u64 = 30;
    use super::*;
    use crate::palw_carriage::PALW_CARRIAGE_ALL_DOMAINS;
    use crate::palw_legs::PALW_LEGS_ALL_DOMAINS;
    use crate::palw_reference::PALW_REFERENCE_ALL_DOMAINS;
    use crate::palw_schedule::PALW_SCHEDULE_ALL_DOMAINS;
    use crate::palw_slash::PALW_S_ALL_DOMAINS;
    use crate::palw_step::PALW_STEP_ALL_DOMAINS;
    use crate::palw_step_leg::PALW_STEP_LEG_ALL_DOMAINS;
    use crate::palw_v2::PALW_V2_ALL_DOMAINS;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    fn open_ladder(space_size: u64) -> PalwBisectLadderV1 {
        PalwBisectLadderV1::open(&h64(1), &h64(2), &h64(3), &h64(4), PalwBisectSpaceV1::StepLeaves, space_size, 100, 200).unwrap()
    }

    #[test]
    fn bisect_domains_are_unique_across_all_palw_modules() {
        let mut seen = std::collections::HashSet::new();
        for d in PALW_BISECT_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate bisect domain");
            assert!(d.len() <= 64);
        }
        for d in PALW_V2_ALL_DOMAINS
            .iter()
            .chain(PALW_S_ALL_DOMAINS.iter())
            .chain(PALW_LEGS_ALL_DOMAINS.iter())
            .chain(PALW_REFERENCE_ALL_DOMAINS.iter())
            .chain(PALW_SCHEDULE_ALL_DOMAINS.iter())
            .chain(PALW_CARRIAGE_ALL_DOMAINS.iter())
            .chain(PALW_STEP_ALL_DOMAINS.iter())
            .chain(PALW_STEP_LEG_ALL_DOMAINS.iter())
        {
            assert!(!seen.contains(d), "bisect reuses a foreign domain: {}", String::from_utf8_lossy(d));
        }
    }

    /// A full ladder against a simulated divergence point: the challenger's honest strategy
    /// (agree iff the divergence is strictly above the midpoint) must converge on EXACTLY
    /// the divergent index, within the log bound, from any starting size.
    #[test]
    fn ladder_converges_on_the_divergent_index() {
        for space in [2u64, 3, 5, 17, 1 << 10, (1 << 20) + 7] {
            for divergence in [0, 1, space / 2, space - 2, space - 1] {
                let mut ladder = open_ladder(space);
                let mut daa = 200u64;
                let mut rungs = 0u32;
                while ladder.turn() != PalwBisectTurnV1::Terminal {
                    let mid = ladder.expected_midpoint().unwrap();
                    daa += 10;
                    ladder
                        .apply_disclosure(&PalwBisectDisclosureV1 {
                            version: 1,
                            session_id: ladder.session_id(),
                            round: ladder.round(),
                            midpoint: mid,
                            mid_state: h64((mid % 251) as u8),
                        }, daa, W_ROUND)
                        .unwrap();
                    daa += 10;
                    // Honest challenger: prefix [0, mid) matches iff divergence >= mid.
                    ladder
                        .apply_verdict(&PalwBisectVerdictV1 {
                            version: 1,
                            session_id: ladder.session_id(),
                            round: ladder.round(),
                            agree: divergence >= mid,
                        }, daa, W_ROUND)
                        .unwrap();
                    rungs += 1;
                    assert!(rungs <= PALW_BISECT_MAX_ROUNDS, "space {space} divergence {divergence}");
                }
                assert_eq!(ladder.terminal_index(), Some(divergence), "space {space}");
                assert!(rungs <= 64 - (space.leading_zeros()) + 1, "log bound: space {space} took {rungs}");
            }
        }
    }

    #[test]
    fn protocol_violations_are_rejected() {
        let mut ladder = open_ladder(16);
        let sid = ladder.session_id();
        // Verdict before any disclosure.
        let verdict = PalwBisectVerdictV1 { version: 1, session_id: sid, round: 0, agree: true };
        assert!(matches!(ladder.apply_verdict(&verdict, 300, W_ROUND), Err(PalwBisectError::TurnMismatch { .. })));
        // Wrong session.
        let alien = PalwBisectDisclosureV1 {
            version: 1,
            session_id: h64(0xEE),
            round: 0,
            midpoint: 8,
            mid_state: h64(9),
        };
        assert_eq!(ladder.apply_disclosure(&alien, 300, W_ROUND), Err(PalwBisectError::SessionMismatch));
        // Wrong midpoint.
        let off_mid = PalwBisectDisclosureV1 {
            version: 1,
            session_id: sid,
            round: 0,
            midpoint: 7,
            mid_state: h64(9),
        };
        assert!(matches!(ladder.apply_disclosure(&off_mid, 300, W_ROUND), Err(PalwBisectError::TurnMismatch { .. })));
        // A zero rung window would make every move instantly overdue — refused rather than
        // trusted from a registration that got it wrong. (The case that used to sit here,
        // "non-monotonic deadline", is gone with the carried deadlines themselves: a party can no
        // longer propose one at all.)
        let ok_shape = PalwBisectDisclosureV1 { version: 1, session_id: sid, round: 0, midpoint: 8, mid_state: h64(9) };
        assert_eq!(ladder.apply_disclosure(&ok_shape, 300, 0), Err(PalwBisectError::ZeroRungWindow));
        // Wrong round.
        let wrong_round = PalwBisectDisclosureV1 {
            version: 1,
            session_id: sid,
            round: 3,
            midpoint: 8,
            mid_state: h64(9),
        };
        assert!(matches!(ladder.apply_disclosure(&wrong_round, 300, W_ROUND), Err(PalwBisectError::RoundMismatch { .. })));
        // Tiny/huge spaces refuse to open.
        assert!(matches!(
            PalwBisectLadderV1::open(&h64(1), &h64(2), &h64(3), &h64(4), PalwBisectSpaceV1::TraceEvents, 1, 100, 200),
            Err(PalwBisectError::SpaceOutOfRange { .. })
        ));
        assert!(matches!(
            PalwBisectLadderV1::open(
                &h64(1),
                &h64(2),
                &h64(3),
                &h64(4),
                PalwBisectSpaceV1::TraceEvents,
                PALW_BISECT_MAX_SPACE + 1,
                100,
                200
            ),
            Err(PalwBisectError::SpaceOutOfRange { .. })
        ));
    }

    #[test]
    fn no_show_is_attributable_and_deadline_gated() {
        let mut ladder = open_ladder(16);
        // Before the deadline: not declarable.
        assert!(matches!(ladder.declare_no_show(150), Err(PalwBisectError::DeadlineNotReached { .. })));
        // Past it: the responder (whose disclosure is due) is the silent party, and the ladder
        // ENDS. It used to stay open and keep accepting moves — a game with no terminal state,
        // declarable no-show after no-show (2026-08-17 re-audit).
        let offense = ladder.declare_no_show(250).unwrap();
        assert_eq!(offense.silent_party, PalwBisectPartyV1::Responder);
        assert_eq!(offense.round, 0);
        assert_eq!(ladder.turn(), PalwBisectTurnV1::Abandoned, "silence decides the dispute");
        let id1 = bisect_offense_id_v1(&offense);
        // Nothing is legal afterwards: not a move, not a second charge for the same silence.
        assert_eq!(
            ladder.apply_disclosure(
                &PalwBisectDisclosureV1 { version: 1, session_id: ladder.session_id(), round: 0, midpoint: 8, mid_state: h64(9) },
                260,
                W_ROUND
            ),
            Err(PalwBisectError::AlreadyTerminal)
        );
        assert_eq!(ladder.declare_no_show(400), Err(PalwBisectError::AlreadyTerminal));

        // On a fresh ladder, silence AFTER a disclosure is the challenger's, and the two offense
        // ids separate rung and party.
        let mut second = open_ladder(16);
        second
            .apply_disclosure(
                &PalwBisectDisclosureV1 { version: 1, session_id: second.session_id(), round: 0, midpoint: 8, mid_state: h64(9) },
                210,
                W_ROUND,
            )
            .unwrap();
        let offense2 = second.declare_no_show(210 + W_ROUND + 1).unwrap();
        assert_eq!(offense2.silent_party, PalwBisectPartyV1::Challenger);
        assert_eq!(second.turn(), PalwBisectTurnV1::Abandoned);
        assert_ne!(bisect_offense_id_v1(&offense2), id1, "offense ids must separate rungs/parties");
        // Session ids separate disputes: same parties, different root.
        let other =
            PalwBisectLadderV1::open(&h64(1), &h64(0xAB), &h64(3), &h64(4), PalwBisectSpaceV1::StepLeaves, 16, 100, 200).unwrap();
        assert_ne!(other.session_id(), ladder.session_id());
    }

    /// **The security property this hardening exists for: a party cannot set its opponent's
    /// clock.**
    ///
    /// Deadlines used to ride the messages — the disclosure carried the challenger's verdict
    /// deadline, the verdict carried the responder's next one — checked only for monotonicity.
    /// A responder could therefore write "one DAA from now" and win by expiry before the
    /// challenger could physically reply (2026-08-17 re-audit). The deadline is now
    /// `accepted_daa + w_round`, both facts the mover does not choose.
    #[test]
    fn a_party_cannot_set_its_opponents_deadline() {
        let mut ladder = open_ladder(16);
        let disclose = |l: &PalwBisectLadderV1| PalwBisectDisclosureV1 {
            version: 1,
            session_id: l.session_id(),
            round: l.round(),
            midpoint: l.expected_midpoint().unwrap(),
            mid_state: h64(9),
        };

        // The SAME message applied at the same block yields the same deadline whatever the mover
        // would have preferred — there is no field left to prefer with.
        let msg = disclose(&ladder);
        ladder.apply_disclosure(&msg, 500, W_ROUND).unwrap();
        // The challenger now has a full window: silence is not chargeable before it elapses...
        assert!(matches!(ladder.declare_no_show(500 + W_ROUND), Err(PalwBisectError::DeadlineNotReached { .. })));
        // ...and is chargeable exactly one DAA past it.
        let mut same = open_ladder(16);
        same.apply_disclosure(&disclose(&same), 500, W_ROUND).unwrap();
        assert!(same.declare_no_show(500 + W_ROUND + 1).is_ok());

        // A longer registered window moves the deadline, and only the window can.
        let mut wide = open_ladder(16);
        wide.apply_disclosure(&disclose(&wide), 500, W_ROUND * 10).unwrap();
        assert!(
            matches!(wide.declare_no_show(500 + W_ROUND + 1), Err(PalwBisectError::DeadlineNotReached { .. })),
            "the class's w_round is what sizes the window, not the mover"
        );
    }

    /// `apply_verdict` is transactional: a move that exhausts the round budget leaves the ladder
    /// exactly as it was. It used to narrow the interval and bump the round first and return the
    /// error afterwards, leaving a half-applied ladder in a machine whose entire purpose is that
    /// every observer derives the identical state.
    #[test]
    fn a_refused_verdict_changes_nothing() {
        let mut ladder = open_ladder(16);
        ladder.apply_disclosure(&PalwBisectDisclosureV1 {
            version: 1,
            session_id: ladder.session_id(),
            round: 0,
            midpoint: 8,
            mid_state: h64(9),
        }, 500, W_ROUND).unwrap();
        let before = ladder.clone();
        // A zero window is refused, and nothing moved.
        let verdict = PalwBisectVerdictV1 { version: 1, session_id: ladder.session_id(), round: 0, agree: true };
        assert_eq!(ladder.apply_verdict(&verdict, 510, 0), Err(PalwBisectError::ZeroRungWindow));
        assert_eq!(ladder, before, "a refused verdict must leave the ladder untouched");
    }

    #[test]
    fn midpoint_function_is_the_pinned_one() {
        // The floor midpoint, frozen by value witnesses (an implementation that rounds up or
        // averages differently fails here and is not in the same dispute).
        assert_eq!(bisect_midpoint_v1(0, 2), 1);
        assert_eq!(bisect_midpoint_v1(0, 3), 1);
        assert_eq!(bisect_midpoint_v1(5, 8), 6);
        assert_eq!(bisect_midpoint_v1(0, u64::MAX), u64::MAX / 2);
        assert_eq!(bisect_midpoint_v1((1 << 40) - 2, 1 << 40), (1 << 40) - 1);
    }
}
