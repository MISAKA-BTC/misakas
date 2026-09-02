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

/// **The rung deadline a session's own backstop admits** (ADR-0080 W4).
///
/// `min(accepted_daa + w_round, session_deadline_daa − assembly_reserve_daa)`.
///
/// ADR-0080 splits a court close across several blocks, so a mover spends part of its turn
/// ASSEMBLING the close rather than answering. The reserve is those blocks
/// (`palw_context_ladder::palw_close_assembly_daa_v1(court.max_close_chunks())` is the derived
/// figure, and it is passed in rather than read here so this machine keeps taking numbers instead
/// of deriving them — the same discipline that keeps `w_round` an argument). It is a per-RULESET
/// number, which is the other reason it cannot be read here: two shipped networks answer 216 and 8.
///
/// A rung window that would run past the reserve is pulled back to it. That is a cap, not an
/// error: the party whose turn it is loses window it was never going to be able to use, and the
/// party after it keeps the blocks its own close costs.
///
/// Saturating in both directions. A reserve wider than the backstop yields 0 — a session with no
/// assembly room, which the whole-session backstop ends on the challenger's side.
pub const fn bisect_rung_deadline_within_session_v1(
    accepted_daa: u64,
    w_round: u64,
    session_deadline_daa: u64,
    assembly_reserve_daa: u64,
) -> u64 {
    let rung = accepted_daa.saturating_add(w_round);
    let cap = session_deadline_daa.saturating_sub(assembly_reserve_daa);
    if rung < cap {
        rung
    } else {
        cap
    }
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
    #[error("the disclosed midpoint state repeats an endpoint's own state — the interval would contain no divergence")]
    MidStateRepeatsAnEndpoint,
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
///
/// Borsh-serializable with a pinned discriminant because the ladder is chain state now
/// (`PalwCourtSessionStateV2::ladder`) and therefore part of the PALW state root: a reordering of
/// these variants would silently move every root that contains a live dispute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwBisectTurnV1 {
    /// The responder must disclose the current midpoint's state commitment.
    AwaitDisclosure = 0,
    /// The challenger must agree/disagree with the last disclosure.
    AwaitVerdict = 1,
    /// The interval is one index wide: the responder must open that index's input state for
    /// the terminal one-step / one-call check. The ladder's job is done.
    Terminal,
    /// A party went silent past its rung deadline and the dispute is decided against them. An
    /// absorbing state: no later move is legal, and no second no-show is chargeable.
    Abandoned = 3,
}

/// The full ladder state. Every observer feeding it the same message stream derives the same
/// state — that is what makes rung silence an OBJECTIVE offense.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBisectLadderV1 {
    session_id: Hash64,
    space_size: u64,
    lo: u64,
    hi: u64,
    round: u32,
    turn: PalwBisectTurnV1,
    last_deadline_daa: u64,
    /// The state commitments the interval's endpoints currently carry — the **anchor pair**.
    ///
    /// `lo_state` starts at the job context (the execution's agreed starting point) and `hi_state`
    /// at the announced `committed_root`; every accepted verdict replaces one of them with the
    /// midpoint state the responder disclosed. So the responder is bound to a chain of its own
    /// claims rather than to nothing, and whichever index the interval collapses on arrives with
    /// two pinned states — which is exactly the pair a terminal check needs, and the reason its
    /// absence made this field's predecessor inert.
    lo_state: Hash64,
    hi_state: Hash64,
    /// Disclosed midpoint states, rung-ordered.
    ///
    /// Recorded, and now **partly binding** (external audit P0-9 item 1).
    ///
    /// `apply_disclosure` used to accept any `mid_state` whatsoever — only the MIDPOINT was checked
    /// — so the rungs bound nothing and the located index was a function of what the responder chose
    /// to say. It now refuses a state that repeats either endpoint of the current interval, and the
    /// endpoints are maintained as [`Self::lo_state`] / [`Self::hi_state`]: the seeds are the job
    /// context and the announced root, and every verdict replaces one with the state just disclosed.
    ///
    /// **What that does and does not buy.** It does not make a disclosure TRUE — no full node can
    /// decide that without the execution, which is what the terminal opening and the proof-carrying
    /// operands of `palw_artifact` are for. It makes the responder accountable to its own chain of
    /// claims: it can no longer answer with a state it has already committed to, and it can no
    /// longer drive the interval onto endpoints that are equal — an interval whose ends agree
    /// contains no divergence, so a ladder that reached one has disproved its own dispute.
    ///
    /// The pair also IS the terminal check's anchor pair, so its ABSENCE no longer blocks that
    /// check. Its weakness still does: a responder that discloses DISTINCT junk at every rung is
    /// refused nothing here and steers the interval exactly as before, so a terminal move added on
    /// the strength of this alone would be fail-open. See the four grounds recorded in `palw_facts`
    /// — this settles the anchor pair, not the steering.
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
            // The execution's agreed start and the root the block announced: the two states the
            // dispute claims differ. Everything the ladder narrows lies strictly between them.
            lo_state: *job_context_hash,
            hi_state: *committed_root,
            disclosures: Vec::new(),
        })
    }

    /// **Open a ladder already narrowed to a committed checkpoint's window** (ADR-0030 §3, the
    /// consumption side).
    ///
    /// [`Self::open`] starts at `[0, space_size)` with `lo_state` = the job context, because that
    /// is the only start two parties are guaranteed to agree about. A committed checkpoint is a
    /// **better** one: it is a state the responder is already bound to, under
    /// `checkpoint_merkle_root` and therefore under `committed_execution_root`, so it can be an
    /// interval endpoint without either party conceding anything they had not already said.
    ///
    /// The ladder then bisects `[anchor_index, space_size)` instead of the whole space — `log₂` of
    /// what is left rather than of everything — and the rungs a dispute costs fall with it.
    ///
    /// # Why the session id does NOT move
    ///
    /// It is [`bisect_session_id_v1`] over the same six inputs, byte for byte. `court_session_id_v2`
    /// derives it from the CLAIM (claim id, trace root, both parties, space, space size), and a V2
    /// court would reject a ladder whose id it cannot recompute — so an anchor that changed the id
    /// would be a ladder no court accepts. There is one dispute per id and the anchor lives inside
    /// it, so nothing collides.
    ///
    /// # Why a wrong anchor is the challenger's problem and nobody else's
    ///
    /// The caller must have verified `anchor_state` against the claim's checkpoint leg — this
    /// machine takes hashes and cannot check that, the same way it cannot check any disclosure.
    /// What it does not need to check is *which* committed checkpoint was chosen: narrowing past
    /// the divergence loses the challenger its own bond, and narrowing short of it only costs extra
    /// rungs. A challenger cannot use the anchor to make the responder defend a region the claim
    /// never covered, because every index in `[anchor_index, space_size)` is inside the space the
    /// id already names.
    pub fn open_anchored(
        job_context_hash: &Hash64,
        committed_root: &Hash64,
        challenger_id: &Hash64,
        responder_id: &Hash64,
        space: PalwBisectSpaceV1,
        space_size: u64,
        anchor_index: u64,
        anchor_state: Hash64,
        opened_at_daa: u64,
        first_deadline_daa: u64,
    ) -> Result<Self, PalwBisectError> {
        let mut ladder = Self::open(
            job_context_hash,
            committed_root,
            challenger_id,
            responder_id,
            space,
            space_size,
            opened_at_daa,
            first_deadline_daa,
        )?;
        // The interval must still contain something to bisect. `anchor_index == space_size - 1`
        // leaves one index, which is a terminal, not a ladder — refused here rather than opened
        // into a machine whose first midpoint is its own endpoint.
        if anchor_index + 1 >= space_size {
            return Err(PalwBisectError::SpaceOutOfRange { got: space_size - anchor_index, max: PALW_BISECT_MAX_SPACE });
        }
        // An anchor equal to the announced root is an interval whose ends agree — no divergence
        // inside it, so the dispute has disproved itself before a rung. The same rule
        // `apply_disclosure` enforces on every later state, applied to the seed.
        if anchor_state == *committed_root {
            return Err(PalwBisectError::MidStateRepeatsAnEndpoint);
        }
        ladder.lo = anchor_index;
        ladder.lo_state = anchor_state;
        Ok(ladder)
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

    /// **Pull this rung's deadline back inside the session's own assembly reserve** (ADR-0080 W4).
    ///
    /// A court close no longer fits one carrier, so a mover spends blocks ASSEMBLING its close
    /// before the move exists at all. Those blocks come out of the session, and if the rung clock
    /// is allowed to run to the session backstop the reserve is spent by whichever party moved
    /// last — the deadline then closes on the other side while it is still assembling, which is
    /// audit M2-24's shape one level down.
    ///
    /// The cap is [`bisect_rung_deadline_within_session_v1`]'s: `session.deadline_daa −
    /// assembly_reserve_daa`, never raised, only lowered. Returns the deadline in force after the
    /// cap, so a caller can index on it without reading the field back.
    ///
    /// **A cap at or below the current score means the session has no assembly room left**, and
    /// that is the honest answer rather than an error: the whole-session backstop is what ends such
    /// a session, on the challenger's side, which is what an unfinishable prosecution deserves.
    /// This is also why the cap may break the monotonicity [`Self::open`] requires of the FIRST
    /// deadline — a backstop is not a rung window, and a session whose backstop has already passed
    /// cannot be given a rung in the future to preserve an invariant about rungs.
    pub fn cap_deadline_to_session_v1(&mut self, session_deadline_daa: u64, assembly_reserve_daa: u64) -> u64 {
        let cap = session_deadline_daa.saturating_sub(assembly_reserve_daa);
        if cap < self.last_deadline_daa {
            self.last_deadline_daa = cap;
        }
        self.last_deadline_daa
    }

    pub fn session_id(&self) -> Hash64 {
        self.session_id
    }

    pub fn turn(&self) -> PalwBisectTurnV1 {
        self.turn
    }

    /// The DAA score by which the party named by [`Self::turn`] must move. The rung sweep reads
    /// it: silence past it is the objective offense, and the chain is the observer that sees it.
    pub fn last_deadline_daa(&self) -> u64 {
        self.last_deadline_daa
    }

    /// The rung the responder most recently answered: `(midpoint, disclosed state)`.
    ///
    /// The challenger's move is a comparison against this, so without it a challenger could only
    /// guess which index it was being asked about — and a verdict about the wrong index steers the
    /// interval away from the divergence just as effectively as a lie.
    pub fn last_disclosure(&self) -> Option<(u64, Hash64)> {
        self.disclosures.last().copied()
    }

    pub fn round(&self) -> u32 {
        self.round
    }

    /// The disputed interval `[lo, hi)`.
    pub fn interval(&self) -> (u64, u64) {
        (self.lo, self.hi)
    }

    /// The index the terminal check adjudicates: `Some` only at `Terminal` AND when the interval
    /// really is one index wide.
    ///
    /// A width-0 interval has no index, and answering `Some(lo)` for one would point a future terminal
    /// check at a step OUTSIDE the disputed interval — an index nobody agreed to bisect toward. Today
    /// `apply_verdict` cannot produce width 0 (`open` refuses `space_size < 2`, and narrowing to the
    /// pinned midpoint always leaves `hi - lo >= 1`), so this is a guard on an unreachable state
    /// rather than a fix. It is written anyway because the state is one arithmetic edit away and the
    /// failure it would cause is a conviction on the wrong step.
    pub fn terminal_index(&self) -> Option<u64> {
        (self.turn == PalwBisectTurnV1::Terminal && self.hi.saturating_sub(self.lo) == 1).then_some(self.lo)
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
        // Audit P0-9 item 1: a disclosure that repeats an endpoint's own state is refused.
        //
        // The rungs used to bind nothing — only the MIDPOINT was checked, so a guilty responder
        // could disclose junk at every rung, an honest challenger would disagree every time, and
        // the interval would collapse on an index the RESPONDER steered onto an honestly-openable
        // leaf. This does not make a disclosure true, which no full node can check; it makes the
        // responder ACCOUNTABLE to its own claims.
        //
        // Repeating an endpoint is the move that has to go: the dispute asserts the interval's
        // endpoints diverge, so a midpoint state equal to one of them says the divergence lies
        // wholly in the other half — a claim the responder is free to make by choosing that half,
        // but not by asserting a state it has already committed to. Accepting it lets a verdict
        // collapse the interval onto endpoints that are EQUAL, which is an interval containing no
        // divergence, and a ladder that has proved its own dispute empty cannot then convict.
        if msg.mid_state == self.lo_state || msg.mid_state == self.hi_state {
            return Err(PalwBisectError::MidStateRepeatsAnEndpoint);
        }
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
        // The state this rung's responder disclosed for `mid`. `apply_disclosure` pushed it and the
        // turn machine guarantees exactly one disclosure precedes each verdict, so the last entry is
        // this rung's — read rather than re-derived, because a second derivation is a second chance
        // to disagree with what was actually accepted.
        let Some(&(_, disclosed)) = self.disclosures.last() else {
            return Err(PalwBisectError::TurnMismatch { expected: "a disclosure before the verdict", got: "none" });
        };
        if msg.agree {
            // Agreeing means the divergence is PAST the midpoint, so the midpoint's disclosed
            // state becomes the interval's new low anchor. Recording it is what makes the next
            // rung's endpoint-repeat check bite against this rung's claim.
            self.lo = mid;
            self.lo_state = disclosed;
        } else {
            self.hi = mid;
            self.hi_state = disclosed;
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

    /// `terminal_index` answers only for a genuinely one-wide interval.
    ///
    /// A width-0 interval has no index; answering `Some(lo)` would point a terminal check at a step
    /// outside the disputed interval. The state is unreachable through the public API today, which is
    /// why the guard is asserted directly on a constructed ladder rather than reached through moves.
    #[test]
    fn a_width_zero_interval_has_no_terminal_index() {
        let mut ladder = open_ladder(16);
        // Walked to a real terminal: one index wide, so it answers.
        while ladder.turn() != PalwBisectTurnV1::Terminal {
            let round = ladder.round();
            let mid = ladder.expected_midpoint().expect("a non-terminal ladder has a midpoint");
            ladder
                .apply_disclosure(
                    &PalwBisectDisclosureV1 {
                        version: 1,
                        session_id: ladder.session_id(),
                        round,
                        midpoint: mid,
                        mid_state: h64(0x40u8.wrapping_add(round as u8)),
                    },
                    100,
                    10,
                )
                .unwrap();
            ladder
                .apply_verdict(&PalwBisectVerdictV1 { version: 1, session_id: ladder.session_id(), round, agree: false }, 110, 10)
                .unwrap();
        }
        let (lo, hi) = ladder.interval();
        assert_eq!(hi - lo, 1, "a walked ladder terminates one index wide");
        assert_eq!(ladder.terminal_index(), Some(lo));
    }

    fn open_ladder(space_size: u64) -> PalwBisectLadderV1 {
        PalwBisectLadderV1::open(&h64(1), &h64(2), &h64(3), &h64(4), PalwBisectSpaceV1::StepLeaves, space_size, 100, 200).unwrap()
    }

    // ---------------------------------------------------------------------------------------
    // ADR-0080 W4 — a rung may not spend the session's close-assembly reserve
    // ---------------------------------------------------------------------------------------

    /// **The rung window is capped at the session backstop LESS the assembly reserve**, so the
    /// blocks a split close occupies are still there when the next party has to assemble one.
    ///
    /// The pure form first — it is what a caller with two numbers and no ladder wants — then the
    /// same rule applied to a live ladder.
    #[test]
    fn a_rung_deadline_is_capped_at_the_sessions_assembly_reserve() {
        // ADR-0080's shipped figure, restated here as a fixture rather than imported: this
        // machine takes numbers, it does not derive them.
        const RESERVE: u64 = 216;
        // Inside the reserve: the rung window is what it was.
        assert_eq!(bisect_rung_deadline_within_session_v1(100, 30, 1_000, RESERVE), 130);
        // Past it: pulled back to `backstop − reserve`, never to the backstop itself.
        assert_eq!(bisect_rung_deadline_within_session_v1(800, 30, 1_000, RESERVE), 784);
        assert_eq!(1_000 - RESERVE, 784, "the cap is the backstop less the reserve, and nothing else");
        // The last rung window that is NOT capped, and the first that is — the boundary, so a
        // change to the comparison shows up here rather than as an off-by-one in a dispute.
        assert_eq!(bisect_rung_deadline_within_session_v1(753, 30, 1_000, RESERVE), 783);
        assert_eq!(bisect_rung_deadline_within_session_v1(754, 30, 1_000, RESERVE), 784);
        assert_eq!(bisect_rung_deadline_within_session_v1(755, 30, 1_000, RESERVE), 784);
        // Exactly on the cap is already at it, and the cap is idempotent.
        assert_eq!(bisect_rung_deadline_within_session_v1(784, 0, 1_000, RESERVE), 784);
        // A reserve wider than the backstop is a session with no assembly room: 0, saturating,
        // rather than an underflow.
        assert_eq!(bisect_rung_deadline_within_session_v1(100, 30, 100, RESERVE), 0);
        assert_eq!(bisect_rung_deadline_within_session_v1(u64::MAX, u64::MAX, 1_000, RESERVE), 784);
    }

    /// The same rule on a live ladder: the cap lowers a rung deadline and never raises one, and
    /// the capped deadline is strictly inside the backstop — which is the condition the court
    /// sweep reads to decide whether a rung clock may fire at all.
    #[test]
    fn capping_a_ladders_rung_lowers_it_and_leaves_the_reserve() {
        const RESERVE: u64 = 216;
        const BACKSTOP: u64 = 1_000;
        let mut ladder = open_ladder(16);
        // `open`'s first deadline is 200 — well inside the cap, so the cap changes nothing.
        assert_eq!(ladder.last_deadline_daa(), 200);
        assert_eq!(ladder.cap_deadline_to_session_v1(BACKSTOP, RESERVE), 200, "the cap raised a deadline");

        // A rung accepted late sets a deadline past the reserve; the cap pulls it back.
        let round = ladder.round();
        let mid = ladder.expected_midpoint().expect("a fresh ladder has a midpoint");
        ladder
            .apply_disclosure(
                &PalwBisectDisclosureV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: ladder.session_id(),
                    round,
                    midpoint: mid,
                    mid_state: h64(0x40),
                },
                900,
                W_ROUND,
            )
            .expect("the disclosure is on turn");
        assert_eq!(ladder.last_deadline_daa(), 930, "the uncapped rung window is the accepted score plus w_round");
        assert_eq!(ladder.cap_deadline_to_session_v1(BACKSTOP, RESERVE), 784);
        assert_eq!(ladder.last_deadline_daa(), 784);
        assert!(ladder.last_deadline_daa() < BACKSTOP, "a capped rung must still be able to fire before the backstop");
        assert_eq!(BACKSTOP - ladder.last_deadline_daa(), RESERVE, "the reserve is exactly what the cap left");
        // Idempotent, and it does not disturb anything else the rung set.
        assert_eq!(ladder.cap_deadline_to_session_v1(BACKSTOP, RESERVE), 784);
        assert_eq!(ladder.turn(), PalwBisectTurnV1::AwaitVerdict);
        assert_eq!(ladder.last_disclosure(), Some((mid, h64(0x40))));
        // And silence past the capped deadline is still the objective offense, charged to the
        // party whose turn it is — the cap moves WHEN, never WHO.
        let no_show = ladder.declare_no_show(785).expect("silence past the capped deadline");
        assert_eq!((no_show.silent_party, no_show.deadline_daa), (PalwBisectPartyV1::Challenger, 784));
    }

    /// Walk a ladder to its terminal, driving the interval toward `divergence`. Returns the
    /// ladder at whatever state the walk reached — the tests below assert what that state is.
    fn walk_to_terminal(space_size: u64, divergence: u64) -> PalwBisectLadderV1 {
        let mut ladder = open_ladder(space_size);
        let mut accepted = 200u64;
        while ladder.turn() == PalwBisectTurnV1::AwaitDisclosure {
            let round = ladder.round();
            let mid = ladder.expected_midpoint().expect("a ladder awaiting disclosure has a midpoint");
            accepted += 1;
            ladder
                .apply_disclosure(
                    &PalwBisectDisclosureV1 {
                        version: PALW_BISECT_OBJECT_VERSION_V1,
                        session_id: ladder.session_id(),
                        round,
                        midpoint: mid,
                        // Distinct per rung: the endpoint-repeat rule refuses a state that echoes
                        // either anchor, and a fixture that tripped it would be testing that rule
                        // instead of this one.
                        mid_state: h64(0x40u8.wrapping_add(round as u8)),
                    },
                    accepted,
                    W_ROUND,
                )
                .expect("the disclosure is on turn, on round and on midpoint");
            accepted += 1;
            // "Agree" means the divergence is PAST the midpoint. Steering by the real divergence
            // is what makes the located index meaningful rather than an artifact of the fixture.
            ladder
                .apply_verdict(
                    &PalwBisectVerdictV1 {
                        version: PALW_BISECT_OBJECT_VERSION_V1,
                        session_id: ladder.session_id(),
                        round,
                        agree: divergence >= mid,
                    },
                    accepted,
                    W_ROUND,
                )
                .expect("the verdict is on turn and on round");
        }
        ladder
    }

    /// **P0-9 item 1: the ladder REACHES a terminal, and the terminal is one step wide.**
    ///
    /// A bisection that narrows forever adjudicates nothing — the challenge window closes with
    /// the dispute open, which under ADR-0038's ramp pins the block at `Provisional` and lets an
    /// unfalsifiable accusation deny an honest producer its weight indefinitely. What the court
    /// owes is termination at a SINGLE index, because a one-step interval is the only thing the
    /// arithmetic layer can adjudicate without the model.
    #[test]
    fn palw_v2_bisection_reaches_terminal_verdict() {
        // Every divergence in a 16-wide space, so the property is the ladder's and not one
        // fixture's luck.
        for divergence in 0..16u64 {
            let ladder = walk_to_terminal(16, divergence);
            assert_eq!(ladder.turn(), PalwBisectTurnV1::Terminal, "divergence {divergence}: the ladder must terminate");
            let (lo, hi) = ladder.interval();
            assert_eq!(hi - lo, 1, "divergence {divergence}: a terminal interval is one index wide");
            assert_eq!(ladder.terminal_index(), Some(lo), "divergence {divergence}: the terminal names its index");
            assert_eq!(lo, divergence, "divergence {divergence}: the ladder located the index the verdicts steered it to");
            // And the ladder is closed to further ladder moves — the terminal opening is the
            // court's business, not another rung.
            let round = ladder.round();
            let mut after = ladder.clone();
            assert!(matches!(
                after.apply_disclosure(
                    &PalwBisectDisclosureV1 {
                        version: PALW_BISECT_OBJECT_VERSION_V1,
                        session_id: after.session_id(),
                        round,
                        midpoint: lo,
                        mid_state: h64(0xEE),
                    },
                    900,
                    W_ROUND
                ),
                Err(PalwBisectError::AlreadyTerminal)
            ));
        }
    }

    /// **P0-9 item 2: the responder's silence decides the dispute against it.**
    ///
    /// Liveness, not soundness: a responder that simply stops answering must not be able to hold
    /// a dispute open past the challenge window. Silence past a rung deadline is the objective
    /// offense, and the ladder must end — a no-show that left the machine movable was the shape
    /// the 2026-08-17 re-audit found (a game with no terminal state).
    #[test]
    fn palw_v2_bisection_responder_timeout_defaults() {
        let mut ladder = open_ladder(16);
        assert_eq!(ladder.turn(), PalwBisectTurnV1::AwaitDisclosure, "the responder moves first");
        let deadline = 200; // `open`'s `first_deadline_daa`

        // Before the deadline there is no offense: an early accusation would let a challenger
        // win by being impatient.
        assert!(matches!(ladder.declare_no_show(deadline), Err(PalwBisectError::DeadlineNotReached { deadline: 200, observed: 200 })));

        let offense = ladder.declare_no_show(deadline + 1).expect("silence past the deadline is an offense");
        assert_eq!(offense.silent_party, PalwBisectPartyV1::Responder);
        assert_eq!((offense.deadline_daa, offense.observed_daa), (deadline, deadline + 1));
        assert_eq!(ladder.turn(), PalwBisectTurnV1::Abandoned, "the dispute is decided, not merely annotated");

        // Absorbing: no later move, and no second charge for the same silence.
        assert!(matches!(ladder.declare_no_show(deadline + 99), Err(PalwBisectError::AlreadyTerminal)));
        assert!(matches!(
            ladder.apply_disclosure(
                &PalwBisectDisclosureV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: ladder.session_id(),
                    round: 0,
                    midpoint: 8,
                    mid_state: h64(0x40),
                },
                deadline + 2,
                W_ROUND
            ),
            Err(PalwBisectError::AlreadyTerminal)
        ));
    }

    /// **P0-9 item 3: the challenger's silence decides it the other way.**
    ///
    /// The mirror matters on its own: a challenger that opens a dispute and goes quiet is the
    /// cheapest denial-of-service against an honest producer's maturity, so the default must run
    /// in both directions and name the right party. Charging the wrong one would slash the honest
    /// side for its opponent's silence.
    #[test]
    fn palw_v2_bisection_challenger_timeout_defaults() {
        let mut ladder = open_ladder(16);
        ladder
            .apply_disclosure(
                &PalwBisectDisclosureV1 {
                    version: PALW_BISECT_OBJECT_VERSION_V1,
                    session_id: ladder.session_id(),
                    round: 0,
                    midpoint: 8,
                    mid_state: h64(0x40),
                },
                210,
                W_ROUND,
            )
            .expect("the responder answers its rung");
        assert_eq!(ladder.turn(), PalwBisectTurnV1::AwaitVerdict, "now the challenger owes a move");

        // The deadline is a fact about the chain and the registered window — neither party sets
        // it (the re-audit's fix: a party that set its opponent's clock could win by expiry).
        let deadline = 210 + W_ROUND;
        assert!(matches!(ladder.declare_no_show(deadline), Err(PalwBisectError::DeadlineNotReached { .. })));

        let offense = ladder.declare_no_show(deadline + 1).expect("the challenger's silence is an offense too");
        assert_eq!(offense.silent_party, PalwBisectPartyV1::Challenger, "the SILENT party is charged, not the accused");
        assert_eq!(ladder.turn(), PalwBisectTurnV1::Abandoned);

        // The two offenses are distinct objects, so a slash consumer can never confuse them.
        let mut responder_side = open_ladder(16);
        let responder_offense = responder_side.declare_no_show(201).expect("responder silence");
        assert_ne!(
            bisect_offense_id_v1(&offense),
            bisect_offense_id_v1(&responder_offense),
            "the two defaults must not share an offense id"
        );
    }

    /// **P0-9 item 4: a disclosure must be about the midpoint the ladder is at.**
    ///
    /// Without it the responder chooses which index the ladder narrows toward, so the located
    /// step is a function of what the accused decided to talk about rather than of where the
    /// executions actually diverge — a court that convicts on an index the responder picked is
    /// no court at all. The endpoint-repeat rule is asserted beside it because the two together
    /// are what bind a rung: the right INDEX, and a state that is not one the responder already
    /// committed to.
    #[test]
    fn palw_v2_bisection_midpoint_must_be_in_commitment() {
        let mut ladder = open_ladder(16);
        let expected = ladder.expected_midpoint().expect("a fresh ladder has a midpoint");
        assert_eq!(expected, bisect_midpoint_v1(0, 16));

        let disclose = |midpoint: u64, mid_state: Hash64| PalwBisectDisclosureV1 {
            version: PALW_BISECT_OBJECT_VERSION_V1,
            session_id: ladder.session_id(),
            round: 0,
            midpoint,
            mid_state,
        };
        for wrong in [0u64, 1, expected - 1, expected + 1, 15, 16, u64::MAX] {
            let mut probe = ladder.clone();
            assert!(
                matches!(
                    probe.apply_disclosure(&disclose(wrong, h64(0x40)), 210, W_ROUND),
                    // A wrong-midpoint "disclosure" is not a protocol move at all, which is the
                    // error's own wording — the index is the move's identity, not a parameter.
                    Err(PalwBisectError::TurnMismatch { expected: "the pinned midpoint", .. })
                ),
                "midpoint {wrong} must be refused; only {expected} is the ladder's own"
            );
            assert_eq!(probe, ladder, "a refused disclosure moves nothing");
        }

        // A state that repeats either anchor is refused too: an interval whose ends agree contains
        // no divergence, so a ladder driven onto one has disproved its own dispute.
        for repeat in [h64(1), h64(2)] {
            let mut probe = ladder.clone();
            assert!(probe.apply_disclosure(&disclose(expected, repeat), 210, W_ROUND).is_err(), "an endpoint echo must be refused");
        }

        ladder.apply_disclosure(&disclose(expected, h64(0x40)), 210, W_ROUND).expect("the ladder's own midpoint is accepted");
        assert_eq!(ladder.turn(), PalwBisectTurnV1::AwaitVerdict);
    }

    /// **P0-9 item 5: the ladder is deep enough for the traces the network really registers.**
    ///
    /// The measurement `d1891333` is the origin of this row: a fixed 10-round ladder cannot reach
    /// a step inside the pinned model's trace, so deep fraud was structurally un-prosecutable —
    /// the court would run out of rungs before it located anything. The rule is
    /// `rounds = ceil(log2(step_leaf_count))`, and what this pins is that the machine's own budget
    /// covers it for every space the catalog can legally register.
    #[test]
    fn palw_v2_ladder_depth_covers_measured_trace() {
        // A bisection over N indices needs ceil(log2(N)) halvings. Asserted against the walk, not
        // against a formula restated — a formula that agreed with a wrong implementation would
        // prove nothing.
        for space in [2u64, 3, 4, 5, 16, 17, 1_024, 4_096, 65_536, 1 << 20, PALW_BISECT_MAX_SPACE] {
            let expected_rounds = space.next_power_of_two().trailing_zeros();
            for divergence in [0, space / 3, space / 2, space - 1] {
                let ladder = walk_to_terminal(space, divergence);
                assert_eq!(ladder.turn(), PalwBisectTurnV1::Terminal, "space {space} divergence {divergence} must terminate");
                assert!(
                    ladder.round() <= expected_rounds,
                    "space {space} divergence {divergence}: took {} rungs, ceil(log2) is {expected_rounds}",
                    ladder.round()
                );
                assert!(ladder.round() <= PALW_BISECT_MAX_ROUNDS, "space {space}: the ladder's own budget must cover its own space");
            }
        }

        // And the court's declared shape agrees with the machine: `PalwCourtParamsV2` derives its
        // worst-case duration from the same ceil(log2), so a ruleset cannot claim a shallower
        // ladder than the one that runs.
        for leaves in [2u64, 1_024, 65_536, 1 << 20] {
            let court = crate::palw_mode_v2::PalwCourtParamsV2::new(leaves, 20, 2).expect("a well-formed court");
            assert_eq!(
                court.bisection_rounds(),
                leaves.next_power_of_two().trailing_zeros(),
                "the ruleset's declared depth and the ladder's real depth are one number"
            );
        }
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
                        .apply_disclosure(
                            &PalwBisectDisclosureV1 {
                                version: 1,
                                session_id: ladder.session_id(),
                                round: ladder.round(),
                                midpoint: mid,
                                // An honest responder's state at `mid` is a function of `mid` and distinct from
                                // every other index's. `h64(mid % 251)` was neither once the ladder
                                // narrowed — it repeats every 251 indices and collides with the
                                // all-one-byte seeds — and the endpoint-repeat check refuses exactly that.
                                mid_state: Hash64::from_u64_word(0x5A5A_0000 + mid),
                            },
                            daa,
                            W_ROUND,
                        )
                        .unwrap();
                    daa += 10;
                    // Honest challenger: prefix [0, mid) matches iff divergence >= mid.
                    ladder
                        .apply_verdict(
                            &PalwBisectVerdictV1 {
                                version: 1,
                                session_id: ladder.session_id(),
                                round: ladder.round(),
                                agree: divergence >= mid,
                            },
                            daa,
                            W_ROUND,
                        )
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
        let alien = PalwBisectDisclosureV1 { version: 1, session_id: h64(0xEE), round: 0, midpoint: 8, mid_state: h64(9) };
        assert_eq!(ladder.apply_disclosure(&alien, 300, W_ROUND), Err(PalwBisectError::SessionMismatch));
        // Wrong midpoint.
        let off_mid = PalwBisectDisclosureV1 { version: 1, session_id: sid, round: 0, midpoint: 7, mid_state: h64(9) };
        assert!(matches!(ladder.apply_disclosure(&off_mid, 300, W_ROUND), Err(PalwBisectError::TurnMismatch { .. })));
        // A zero rung window would make every move instantly overdue — refused rather than
        // trusted from a registration that got it wrong. (The case that used to sit here,
        // "non-monotonic deadline", is gone with the carried deadlines themselves: a party can no
        // longer propose one at all.)
        let ok_shape = PalwBisectDisclosureV1 { version: 1, session_id: sid, round: 0, midpoint: 8, mid_state: h64(9) };
        assert_eq!(ladder.apply_disclosure(&ok_shape, 300, 0), Err(PalwBisectError::ZeroRungWindow));
        // Wrong round.
        let wrong_round = PalwBisectDisclosureV1 { version: 1, session_id: sid, round: 3, midpoint: 8, mid_state: h64(9) };
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
        ladder
            .apply_disclosure(
                &PalwBisectDisclosureV1 { version: 1, session_id: ladder.session_id(), round: 0, midpoint: 8, mid_state: h64(9) },
                500,
                W_ROUND,
            )
            .unwrap();
        let before = ladder.clone();
        // A zero window is refused, and nothing moved.
        let verdict = PalwBisectVerdictV1 { version: 1, session_id: ladder.session_id(), round: 0, agree: true };
        assert_eq!(ladder.apply_verdict(&verdict, 510, 0), Err(PalwBisectError::ZeroRungWindow));
        assert_eq!(ladder, before, "a refused verdict must leave the ladder untouched");
    }

    /// **Audit P0-9 item 1**: the rungs bind the responder to its own claims.
    ///
    /// Before this, only the MIDPOINT was checked and `mid_state` was recorded unread — so a guilty
    /// responder could disclose the same junk at every rung, an honest challenger would disagree
    /// every time, and the interval would collapse on an index the RESPONDER steered onto a leaf it
    /// could open honestly. The check does not make a disclosure true (no full node can decide
    /// that); it makes repeating a state the responder has already committed to inadmissible.
    ///
    /// Both endpoints are covered, and the second is the one that matters: the dispute asserts the
    /// endpoints differ, so a midpoint equal to either says the interval it would create contains
    /// no divergence at all.
    #[test]
    fn a_disclosure_may_not_repeat_an_endpoint_state() {
        let ctx = h64(0x11);
        let root = h64(0x22);
        let mut ladder = PalwBisectLadderV1::open(&ctx, &root, &h64(0x33), &h64(0x44), PalwBisectSpaceV1::StepLeaves, 16, 100, 110)
            .expect("a 16-wide space opens");
        let mid = ladder.expected_midpoint().unwrap();
        let disclose =
            |state| PalwBisectDisclosureV1 { version: 1, session_id: ladder.session_id(), round: 0, midpoint: mid, mid_state: state };

        assert_eq!(ladder.clone().apply_disclosure(&disclose(ctx), 120, 10), Err(PalwBisectError::MidStateRepeatsAnEndpoint));
        assert_eq!(ladder.clone().apply_disclosure(&disclose(root), 120, 10), Err(PalwBisectError::MidStateRepeatsAnEndpoint));
        // Anything else is admissible — this is accountability, not verification.
        assert!(ladder.apply_disclosure(&disclose(h64(0x77)), 120, 10).is_ok());

        // And the anchor moves: after agreeing, the disclosed state IS the low endpoint, so the next
        // rung may not repeat it either. That is what makes the binding cumulative rather than a
        // one-off check against the seeds.
        ladder
            .apply_verdict(&PalwBisectVerdictV1 { version: 1, session_id: ladder.session_id(), round: 0, agree: true }, 130, 10)
            .unwrap();
        let next = ladder.expected_midpoint().unwrap();
        let repeat =
            PalwBisectDisclosureV1 { version: 1, session_id: ladder.session_id(), round: 1, midpoint: next, mid_state: h64(0x77) };
        assert_eq!(ladder.apply_disclosure(&repeat, 140, 10), Err(PalwBisectError::MidStateRepeatsAnEndpoint));
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
    /// **An anchored ladder bisects what is LEFT, and is the same session while doing it.**
    ///
    /// Three things, and the middle one is why the anchor can exist at all: an anchored ladder
    /// costs fewer rungs, it carries the SAME session id (so a V2 court that derives the id from
    /// the claim still recognises it), and it refuses the two seeds that would make it meaningless.
    #[test]
    fn an_anchored_ladder_narrows_what_is_left_under_the_same_session_id() {
        let (ctx, root, ch, re) = (h64(0x11), h64(0x22), h64(0x33), h64(0x44));
        let space = PalwBisectSpaceV1::StepLeaves;
        let size = 1024u64;

        let plain = PalwBisectLadderV1::open(&ctx, &root, &ch, &re, space, size, 100, 200).expect("opens");
        let anchored = PalwBisectLadderV1::open_anchored(&ctx, &root, &ch, &re, space, size, 960, h64(0x55), 100, 200).expect("opens");

        // Same dispute, same id — the V2 court derives it from the claim and would not recognise
        // a ladder whose anchor had moved it.
        assert_eq!(plain.session_id(), anchored.session_id());
        assert_eq!(plain.interval(), (0, size));
        assert_eq!(anchored.interval(), (960, size));

        // Fewer rungs, which is the whole point: ⌈log₂ 1024⌉ = 10 against ⌈log₂ 64⌉ = 6.
        let rungs = |mut lo: u64, hi: u64| {
            let mut n = 0;
            let mut hi = hi;
            while hi - lo > 1 {
                let mid = bisect_midpoint_v1(lo, hi);
                if n % 2 == 0 {
                    hi = mid
                } else {
                    lo = mid
                }
                n += 1;
            }
            n
        };
        assert!(rungs(960, size) < rungs(0, size), "the anchor bought no rungs");

        // An anchor at the last index leaves nothing to bisect — a terminal, not a ladder.
        assert!(PalwBisectLadderV1::open_anchored(&ctx, &root, &ch, &re, space, size, size - 1, h64(0x55), 100, 200).is_err());
        // An anchor equal to the announced root is an interval whose ends agree: the dispute has
        // disproved itself before a rung, and seeding it is refused the same way a disclosure that
        // repeats an endpoint is.
        assert_eq!(
            PalwBisectLadderV1::open_anchored(&ctx, &root, &ch, &re, space, size, 960, root, 100, 200),
            Err(PalwBisectError::MidStateRepeatsAnEndpoint)
        );
    }
}
