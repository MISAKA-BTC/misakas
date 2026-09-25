//! **ADR-0152 v3.1 R-3 (Phase 2, P2-8): the reporter's commit–reveal filer — the one path every
//! conviction this node files on its OWN evidence takes — and what feeds it: `ExecutorRefuted` from
//! the capture arm (SR-8, J-4, C-5), J1 auto (§3.9's borrowed strategy, J-6), P2-8c's
//! `PanelFalseValidV2` against every liable `Valid` signer (N10, `palw_filer_false_valid`) and P2-8b's
//! `ExecutorRefuted` from a replay bisection (`palw_filer_replay`).**
//!
//! A child of `palw_panel` (declared there with `#[path]`), so its thin call sites stay one line
//! each and the panel's own signer, ledger and backend seam are reused rather than restated. The
//! other two lanes reach it through ONE door, [`PalwConvictionDoorV1`] (the panel's is
//! [`PalwPanelConvictionDoorV1`]): whether the book holds an offence, and [`PalwReporterFilerV1::file`]
//! through the panel's [`PalwPanelService::reporter_filer_file_v1`] — so every filing is committed
//! over this node's own bond, keyed by its offence (one filing an offence at a time, whichever lane
//! found it: a capture-arm fault and a replay bisection of one claim are one kind-4 key), and never
//! made against this node's own bond.
//!
//! ## Why a filer, and its order
//!
//! A commit–reveal conviction (`PalwConvictionBasisV1::CheckedEvidence`: kinds 0, 3 and 4) pays its
//! reporter reward to the earliest `(committed_daa, commitment)` revealed under the consumed key, and
//! only to a commitment rooted STRICTLY below the DAA of the block that consumed the offence
//! (`apply_reporter_revealed`). An honest reporter therefore commits, waits until the commitment is a
//! chain fact, and only then lets the evidence become public (R-3's "Honest reporters commit, wait
//! for the commitment to be accepted, then broadcast the evidence"). Per filing, driven by what the
//! chain holds at each tick (`palw_reporter_filing_read_v1`, never by what was sent):
//!
//! 1. **Pre-flight**: the processor's own object gate on the evidence at the next block's DAA — the
//!    adjudicator the fold runs — so neither a commitment slot nor a carrier is spent on evidence the
//!    chain refuses. Asked again before every send.
//! 2. **`ReporterCommitted`** (tag 53): a fresh 32-byte salt, `palw_reporter_commitment_v1(offence_key,
//!    evidence_id, own bond, salt)`, signed over `palw_reporter_commit_message_v1` by this node's bond
//!    key (`palw_reporter_commit_object_v1`), through the court queue's priority lane.
//! 3. **The evidence**, once the commitment's row is [`PALW_FILER_COMMIT_DEPTH_DAA_V1`] deep.
//! 4. **`ReporterRevealed`** (tag 54) once the conviction is consumed and its reward pends on THIS
//!    evidence, while `now ≤ reveal_until`.
//! 5. Done when the sweep closes the window (the award, or the forgone amount, is the fold's).
//!
//! A filing never waits on R: past its `file_by_daa` (the landing margin before the claim's receipt
//! deadline — the claim must still be live for S2), or when the bond cannot root a commitment (not
//! `Active` at the floor, its 64 slots full, a commitment the gate refuses, or one sent
//! [`PALW_FILER_MAX_SENDS_V1`] times that never rooted), or below `Params::palw_rcore_plus` (objects
//! 53/54 are refused by name there), the evidence goes out unprotected and only R is at stake.
//!
//! ## Every send is bounded, and a stalled conviction goes back to the court
//!
//! Each object is sent at most [`PALW_FILER_MAX_SENDS_V1`] times, each copy given
//! [`PALW_FILER_RESEND_DAA_V1`] to land: the gate predicts most of what the fold refuses but not all
//! of it (a heavy-budget `PromptNotAnchored Whole`, say), and the acceptance walk mines a carrier
//! whose object it drops, so an uncapped re-send would pay a fee every ten DAA for a court window.
//! Evidence sent that many times without a conviction ends the filing `Stalled`; one the chain never
//! convicted inside the court window ends it `Expired`. Either way the filing is handed back
//! ([`PalwReporterFilerV1::tick`]): the capture arm's filing queues the one-move court's accusation it
//! replaced (its `fallback`, signed when the fault was found) if the gate admits it, and otherwise
//! lets go of the claim in the seat's `accused` set, so the named-leaf pursuit (ADR-0111 Decision 6)
//! may still file; a J1 filing that expired releases its claim's probes. A proven fault is never
//! left unfiled because kind 4 stalled.
//!
//! ## The court queue's order is not the filer's
//!
//! The filer is a step machine ([`palw_filer_step_v1`]): each tick reads the chain's rows and queues
//! at most the ONE object the next step needs, so nothing it does rests on the queue carrying its
//! items in the order they were queued — the evidence is queued only once the commitment is a row,
//! the reveal only once the conviction is. Nor does it rest on WHEN the queue carries them: a copy
//! still waiting in the queue (`in_queue`) is never sent again, never counted as having had its
//! chance to land, and never the reason an object is given up — the send cap's last copy included,
//! so a deadline-ordered queue that holds the evidence's third copy back does not end the filing
//! `Stalled`, and one that holds the commitment's third copy back does not send the evidence beside
//! it — and a copy's resend interval runs from when the carrier lane CARRIED it (`court_moved`), so
//! an item held back is not re-queued the moment it goes out. (A copy the carrier lane DROPS — the
//! mempool refused it — left the queue uncarried: its interval runs from its queuing, and it counts
//! against the cap, which bounds attempts.) Each item it queues states the DAA it is due by
//! ([`palw_filer_step_due_v1`]: the evidence its `file_by_daa` or court window, the commitment the
//! commit depth before that, the reveal R-3's reveal window's end `reveal_until`; a fallback its
//! filing's `file_by_daa`; never before the filing was made), readable by queue key through
//! [`PalwReporterFilerV1::queued_due_v1`] — where an earliest-deadline-first priority lane's due-time
//! function plugs in.
//!
//! ## What goes through it — and what does not
//!
//! Only filings whose conviction opens a commit–reveal reward — the capture arm's and J1's kind 4,
//! P2-8b's kind 4 and P2-8c's kind 3 (an execution-proving step refutation):
//! `palw_filed_offence_commit_key_v1` answers `Some(key)` for kind 4, kind 3 on an
//! execution- or claim-proving contradiction, and a standalone kind 0 — and `None` for everything
//! else. **P2-6's automatic filings earn R by NAME, never by commitment**, and stay on their own
//! lane: a `DefaultAccused` whose session defaults opens the `DaDefault` reward for the earliest
//! defaulted session's accuser (this seat), and the one-move court's `ShardCourtAccused` opens the
//! `CourtConviction` reward for its challenger (this seat) — both keys are public at admission, the
//! fold refuses every reveal on them (V3S-03, `PalwPendingRewardV1::accepts_reveals`), and a
//! commitment to one would only take one of the bond's 64 slots for `window_court`. "Never self" on
//! those lanes is the fold's (`DaAccuserIsTheProducer`, `ShardCourtAccuserIsTheProducer`) and P2-6's
//! pre-check; here it is refused before anything is built (the fold refuses `reporter == accused`).
//!
//! ## Reorgs and restarts
//!
//! Each tick recomputes the step from the chain's rows, so a commitment a reorg took back reads as
//! absent and is re-sent (the same object: the same salt and commitment) while the evidence is not
//! yet convicted; once the conviction stands, a commitment that re-rooted at or after it can never
//! win (the fold's strict order): nothing more is sent, the filing waits out the window (a later
//! reorg may yet put the row back below the conviction, and then it reveals) and lets R go at the
//! sweep — the conviction is unaffected. The book — each entry's salt, commitment, signed
//! commitment object, evidence, court fallback, send marks and counts — is written to
//! `<state_dir>/palw-reporter-filer.v1` after every change (write-then-rename), so a restart between
//! the conviction and the reveal still reveals.
//!
//! ## Front-running: what an observer can and cannot take
//!
//! * **Cannot:** a `ReporterCommitted` hides its key, evidence and reporter behind the salt, so the
//!   mempool learns nothing from it. The evidence is broadcast only after the commitment is a row, so
//!   a copier that learns it from the mempool commits in a later block — a larger `committed_daa` —
//!   and a copy of THESE bytes that folds loses R-3's order `(committed_daa, commitment)` to this
//!   node. Copying the commitment hash under another bond lands in that bond's own slot
//!   (`palw_reporter_commit_slot_v1`), blocking nothing. The reveal names this node's bond and the
//!   fold pays the bond's registered payload, so carrying or copying a reveal redirects nothing.
//! * **Can — and nothing in this filer prevents it:** (a) **a copier who re-encodes the evidence and
//!   gets its copy folded first wins R.** Kind-3/4 evidence has more than one encoding of the same
//!   contradiction: the adjudicator verifies each operand opening and takes the first match
//!   (`PalwProvenOperandsV1::from_openings_v1`, `find_operand_v1`), so an appended duplicate opening
//!   still convicts, under the same offence key and a different `evidence_id`. A copier who reads
//!   this node's evidence in the mempool commits to its re-encoding, and if the copy folds before
//!   this node's evidence — fee competition in a full mempool, or a miner that leaves the honest
//!   carrier out for one block — the conviction consumes the copy: the reward pends on the copy's
//!   `evidence_id`, this node's commitment (bound to its own) can never be revealed, and the
//!   copier's can. The two-DAA depth and the commitment's hiding do not help: the copier's
//!   commitment only has to precede the copy's conviction, not this node's. The fix is consensus
//!   (R-3/N12, the ADR owner's): canonical evidence (refuse openings the adjudicator did not resolve,
//!   duplicates and non-canonical order), or a commitment over a canonical digest of the
//!   contradiction; `p2_8_known_gap_a_re_encoded_copy_folded_first_takes_r` (the T46 suite) pins
//!   the gap until then. (b) Anyone who holds the same evidence BEFORE this node commits — another
//!   seat that sampled the same served capture, a holder of the same gossiped binding (J1 auto), or
//!   the offender itself on the garbage path, whose Sybil can pre-commit to the contradiction it will
//!   disclose (V3S-03) — can commit earlier and win R; that race is fair by commitment order. (c) A
//!   `ReporterCommitted` names its reporter bond in the clear: an offender watching its panel's
//!   seats learns that one is about to file and can use the two-DAA wait to land its own
//!   pre-committed or re-encoded contradiction first (within the conceded V3S-03). (d) The
//!   named-reward lanes (the one-move court's `ShardCourtAccused`) can be front-run outright: its
//!   challenger is whoever signs the accusation, and every byte of it is public in the mempool —
//!   one reason the capture arm files kind 4 through this filer past the fence and keeps the court
//!   as its fallback. In every case the conviction itself stands; only R moves.
//!
//! ## Resources
//!
//! A tick reads the tip once per live filing (cached rows, map lookups) and asks the gate only
//! before a send — evidence it refused, at most once a [`PALW_FILER_GATE_RETRY_DAA_V1`] — and once
//! on a hand-back. J1 auto's probe — two verifications of a held capture
//! and one out-of-range event opening for its binding — runs at most [`PALW_J1_PROBES_PER_TICK_V1`]
//! a tick, under the ledger's full-seat reservation and in a blocking task; each claim's captures
//! are digested at most [`PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1`] times a [`PALW_J1_PROBE_WINDOW_DAA_V1`]
//! window, so neither a pool of already-probed captures nor one claim's churned garbage is hashed
//! every tick or takes every tick's probe. The book is capped at [`PALW_FILER_MAX_ENTRIES_V1`] live
//! filings.
use super::*;
use kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_offence_attribution_v1::{palw_executor_refuted_object_v1, palw_filed_offence_commit_key_v1};
use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
use kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use kaspa_consensus_core::palw_state_v2::PalwReporterFilingReadV1;
use kaspa_consensus_core::palw_step_refute::PalwExecutionStepRefutationV1;
use std::path::Path;

/// **A queued object is assumed in flight this long** before the filer sends it again — the court
/// moves' own figure (`COURT_MOVE_REPLAN_DAA`): a carrier lost to the mempool or a reorg is re-sent,
/// and one that landed is never re-sent because the chain's row already answers the next step.
pub(crate) const PALW_FILER_RESEND_DAA_V1: u64 = COURT_MOVE_REPLAN_DAA;

/// **How many copies of one object the filer sends** (the review of P2-8, F4). A copy is counted when
/// it is queued, and the next goes out only once the last has LEFT the queue and had
/// [`PALW_FILER_RESEND_DAA_V1`] to land since it was carried — so three is two losses forgiven. Past
/// it: a commitment that never rooted is given up (the evidence goes out without one), evidence that
/// never convicted ends the filing `Stalled` (and hands it back), a reveal that never led stops. A
/// last copy still waiting in the queue is none of these: the cap is judged only once it went out
/// (the integration's review, F1). Without the cap a commitment the chain never roots, or evidence
/// the fold drops though the gate admits it, is paid for every ten DAA for a whole court window: up
/// to 300 carriers a filing.
pub(crate) const PALW_FILER_MAX_SENDS_V1: u8 = 3;

/// **How soon evidence the gate refused is asked about again** (the integration's review, F6): a
/// re-plan. The filer ticks every two seconds, and the gate is the processor's whole object gate on
/// the evidence — so a filing the gate keeps refusing (a court open on the claim, say) costs one gate
/// run a re-plan, never one a tick.
pub(crate) const PALW_FILER_GATE_RETRY_DAA_V1: u64 = COURT_MOVE_REPLAN_DAA;

/// **How deep a commitment's row must be before its evidence is sent.** The commitment must be
/// rooted STRICTLY below the conviction's DAA; one that a reorg puts back into the evidence's merge
/// set shares its DAA and can never be revealed. The most recent chain block is the one most often
/// reorged, so the evidence waits for one more: two DAA, about four minutes at t12's 120 s cadence,
/// against a receipt window of 600 DAA.
pub(crate) const PALW_FILER_COMMIT_DEPTH_DAA_V1: u64 = 2;

/// **The filings one lane hands this filer for one offence** (the integration of P2-8b/8c with P2-8):
/// the first, and one more once the first left the book unconvicted (`Stalled` or `Expired`, or a
/// conviction a reorg took back after the book let it go) — the same "two carriers, one lost
/// forgiven" P2-8b's lane had, now counted in filings, each of which sends each object at most
/// [`PALW_FILER_MAX_SENDS_V1`] times. The lanes count it (the book forgets a finished filing); the
/// book's own key is the "never twice at once" half.
pub(crate) const PALW_FILER_HAND_OFFS_PER_OFFENCE_V1: u8 = 2;

/// **The live filings one book holds.** Four times a bond's open commitments
/// (`PALW_REPORTER_OPEN_COMMITMENTS_PER_BOND_V1`): a node with more convictions in flight than that
/// is being flooded, and a filing refused here is logged, never silently dropped.
pub(crate) const PALW_FILER_MAX_ENTRIES_V1: usize =
    4 * kaspa_consensus_core::palw_state_v2::PALW_REPORTER_OPEN_COMMITMENTS_PER_BOND_V1 as usize;

/// **J1 auto's probes a tick** (each two capture verifications and one event opening, reserved and
/// off the tick) — one, so a pool of stranger material cannot turn the probe into a replay storm.
pub(crate) const PALW_J1_PROBES_PER_TICK_V1: usize = 1;

/// **J1 auto's accounting window**, in DAA: the court moves' re-plan interval.
pub(crate) const PALW_J1_PROBE_WINDOW_DAA_V1: u64 = COURT_MOVE_REPLAN_DAA;

/// **The captures of one claim J1 auto digests in one window** (the review of P2-8, F6): twice
/// everything a tick can hold for a claim at once — the pool's `MATERIALS_PER_CLAIM` and the two
/// retention files. A capture's digest is what tells a probed capture from a new one, and it is
/// whole-capture work, so without a bound a claim whose held captures were all probed long ago was
/// hashed again on every tick, and one claim's churned garbage could take every tick's probe. Twice
/// the reach, because a tick stops digesting at its first new capture (the probe it takes) and the
/// next tick digests the same prefix again before it reaches what lies behind it: a borrowed capture
/// at any position a claim's captures can hold is reached inside a window, and garbage ahead of it
/// delays it by at most one window — it can never exhaust the claim for good, since the count
/// starts again with every window.
pub(crate) const PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1: u8 = (2 * (MATERIALS_PER_CLAIM + 2)) as u8;

/// The court queue's rounds of this filer's three objects. The queue key's first element is the
/// OFFENCE key — a namespace no claim, session or unit key shares — and each object has its own
/// round, so a queued commitment and a queued reveal of one filing are two entries.
pub(crate) const PALW_FILER_ROUND_COMMIT_V1: u32 = u32::MAX - 3;
pub(crate) const PALW_FILER_ROUND_EVIDENCE_V1: u32 = u32::MAX - 2;
pub(crate) const PALW_FILER_ROUND_REVEAL_V1: u32 = u32::MAX - 1;

/// **Whether a court-queue entry is one of this filer's**: one of its three rounds on the accusing
/// side, carrying one of its three objects. The round alone is not enough: P2-7's answers key their
/// round by a 32-bit fold of the unit (`palw_disclosure_queue_key_v1`), which may land on any value.
pub(crate) fn palw_filer_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    matches!(round, PALW_FILER_ROUND_COMMIT_V1 | PALW_FILER_ROUND_EVIDENCE_V1 | PALW_FILER_ROUND_REVEAL_V1)
        && !responder
        && matches!(
            object,
            PalwConsensusObjectV2::ReporterCommitted { .. }
                | PalwConsensusObjectV2::ReporterRevealed { .. }
                | PalwConsensusObjectV2::ObjectiveOffence { .. }
        )
}

/// Where a filing came from — for the log, the hand-back of a stalled filing, and the tests that pin
/// each source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub(crate) enum PalwFilingOriginV1 {
    /// The capture sampler's `FaultAt` (SR-8): `ExecutorRefuted` over the refutation it proved. Handed
    /// back stalled: its court fallback is queued, else the claim leaves the seat's `accused` set.
    CaptureArm = 0,
    /// J1 auto: a held capture reproduces the claim's committed root under another job. Handed back
    /// expired: the claim's probes are released.
    BorrowedRoot = 1,
    /// P2-8c's `PanelFalseValidV2` against one `Valid` signer (`palw_filer_false_valid`). Nothing is
    /// handed back: that lane reads the key leaving this book as "no longer in flight" and asks the
    /// chain again at its next walk (a filing that ended unconvicted is handed to the filer once more,
    /// [`PALW_FILER_HAND_OFFS_PER_OFFENCE_V1`]).
    FalseValid = 2,
    /// P2-8b's `ExecutorRefuted` from a replay bisection (`palw_filer_replay`). Handed back as
    /// [`Self::FalseValid`]: that lane hands it once more while the claim's duty stands.
    Replay = 3,
}

/// **One conviction this node files — the filer's whole input.** Built by
/// [`Self::of_offence`], which reads the key from the evidence the way the fold keys it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PalwConvictionFilingV1 {
    /// The key the conviction is consumed under — what the commitment commits to and the reveal
    /// names (`palw_filed_offence_commit_key_v1`).
    pub offence_key: Hash64,
    /// `ObjectiveOffence.evidence_id`: the digest the consumed reward records and the commitment
    /// binds (N12).
    pub evidence_id: Hash64,
    /// The bond the evidence convicts. Never this node's own.
    pub accused: PalwBondKeyV2,
    /// The claim, for the log, the hand-back and J1 auto's probes.
    pub claim_id: Hash64,
    /// The `ObjectiveOffence` itself.
    pub object: PalwConsensusObjectV2,
    /// The last DAA the evidence waits for its commitment; past it the evidence goes out unprotected
    /// (a conviction never waits on R). `None`: wait the whole court window.
    pub file_by_daa: Option<u64>,
    pub origin: PalwFilingOriginV1,
    /// **What the seat files instead if this filing stalls** — keyed as the court queue keys it (the
    /// capture arm's one-move `ShardCourtAccused`, signed when the fault was found). Queued at the
    /// hand-back only if the gate admits it then. Set with [`Self::with_fallback`].
    pub fallback: Option<(Hash64, PalwConsensusObjectV2)>,
}

impl PalwConvictionFilingV1 {
    /// A filing of `object` — `None` unless it is an `ObjectiveOffence` whose conviction takes a
    /// commitment (`palw_filed_offence_commit_key_v1`; the named rewards never come here).
    pub(crate) fn of_offence(
        object: PalwConsensusObjectV2,
        claim_id: Hash64,
        file_by_daa: Option<u64>,
        origin: PalwFilingOriginV1,
    ) -> Option<Self> {
        let PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence_id, evidence } = &object else { return None };
        let offence_key = palw_filed_offence_commit_key_v1(*kind, &accused.0, evidence_id, evidence)?;
        let (evidence_id, accused) = (*evidence_id, *accused);
        Some(Self { offence_key, evidence_id, accused, claim_id, object, file_by_daa, origin, fallback: None })
    }

    /// The same filing, with `fallback` queued in its place should it stall.
    pub(crate) fn with_fallback(self, fallback: Option<(Hash64, PalwConsensusObjectV2)>) -> Self {
        Self { fallback, ..self }
    }
}

/// **One live filing, as the book keeps it** — everything a restart needs to go on: the salt and the
/// signed commitment (so the same commitment is re-sent, and revealed), the evidence, the fallback,
/// and the DAA each object was last queued at (the debounce) and how many copies of it went out
/// (the cap).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct PalwFilerEntryV1 {
    pub offence_key: Hash64,
    pub evidence_id: Hash64,
    pub accused: PalwBondKeyV2,
    pub claim_id: Hash64,
    pub object: PalwConsensusObjectV2,
    pub file_by_daa: Option<u64>,
    pub origin: PalwFilingOriginV1,
    /// This node's bond when the filing was made — the reporter the commitment binds.
    pub reporter: PalwBondKeyV2,
    pub salt: [u8; 32],
    pub commitment: Hash64,
    /// The signed `ReporterCommitted`; `None` for a filing made without one (below the fence, or a
    /// bond that could not root one then) or that gave its commitment up (refused by the gate, or
    /// never rooted), which ends when its conviction lands.
    pub commit_object: Option<PalwConsensusObjectV2>,
    /// [`PalwConvictionFilingV1::fallback`].
    pub fallback: Option<(Hash64, PalwConsensusObjectV2)>,
    pub registered_daa: u64,
    pub commit_sent: Option<u64>,
    pub evidence_sent: Option<u64>,
    pub reveal_sent: Option<u64>,
    /// Copies queued of each object ([`PALW_FILER_MAX_SENDS_V1`]).
    pub commit_sends: u8,
    pub evidence_sends: u8,
    pub reveal_sends: u8,
    /// The chain has shown this filing's reveal as the reward's best at least once.
    pub revealed_seen: bool,
    // ---- Node memory, never written to the book: each mirrors something that is node memory
    // ---- itself (the court queue, the seat's `accused` set), which a restart empties too.
    /// Whether each object's last copy still WAITS in the court queue — set by the tick from
    /// `court_pending` before every step, so the rule never re-sends a copy, counts it landed, or
    /// gives its object up while it has not gone out ([`palw_filer_step_v1`]).
    #[borsh(skip)]
    pub in_queue: PalwFilerInQueueV1,
    /// **The capture arm backs this filing** ([`PalwReporterFilerV1::back_by_capture_arm_v1`]): its
    /// own, or another lane's it found already filed — so the claim sits in the seat's `accused` set
    /// on this filing's account and leaves it at a hand-back that queues no fallback, fallback or
    /// none (the integration's review, F2).
    #[borsh(skip)]
    pub capture_arm_backed: bool,
    /// The DAA the gate last refused this filing's evidence at: asked again
    /// [`PALW_FILER_GATE_RETRY_DAA_V1`] later, not every tick.
    #[borsh(skip)]
    pub gate_refused_at: Option<u64>,
}

/// **Which of a filing's objects still waits in the court queue** ([`PalwFilerEntryV1::in_queue`]):
/// its last copy queued and neither carried nor dropped by the carrier lane yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PalwFilerInQueueV1 {
    pub commit: bool,
    pub evidence: bool,
    pub reveal: bool,
}

/// How a filing ended — logged, and asserted by the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwFilerEndV1 {
    /// Convicted, and this filing's reveal led the reward when its window closed: the award is this
    /// bond's (step 3d moves it).
    Revealed,
    /// Convicted on evidence filed without a commitment: the conviction stands, R is not asked.
    FiledDirect,
    /// Convicted, and R is not this node's — the reason.
    Forgone(&'static str),
    /// Not convicted inside the court window: the evidence never folded (the gate refused it at every
    /// send, or the claim left every door). Handed back.
    Expired,
    /// The evidence went out [`PALW_FILER_MAX_SENDS_V1`] times, each admitted by the gate and given
    /// its interval to land, and never convicted: the fold drops what the gate admits, or no carrier
    /// lands. Handed back.
    Stalled,
}

/// **What the filer does next for one filing**, from the chain's rows ([`palw_filer_step_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwFilerStepV1 {
    /// Nothing to send this tick.
    Wait,
    /// Queue the commitment (tag 53) — after the gate admits the evidence and the commitment.
    Commit,
    /// Queue the evidence — after the gate admits it.
    File,
    /// Queue the reveal (tag 54).
    Reveal,
    /// The filing is over.
    Done(PalwFilerEndV1),
}

/// **The filer's rule, one filing at a time** — pure over the entry (the queue's view of it,
/// `in_queue`, included) and the chain's read of it, so every branch (a reorg on either side of the
/// conviction, a restart, the window's close, the send cap, a copy the queue holds back) is a table
/// the tests can walk without a node. See the module doc for the order.
pub(crate) fn palw_filer_step_v1(entry: &PalwFilerEntryV1, read: &PalwReporterFilingReadV1) -> PalwFilerStepV1 {
    use PalwFilerEndV1 as End;
    use PalwFilerStepV1 as Step;
    let now = read.now_daa;
    // The last copy LEFT the queue and has had its interval to land since (`sent` is moved to the
    // DAA the carrier lane carried it at). A copy still waiting in the queue is never sent again,
    // never counted landed and never given up: the queue's order is not the filer's (F1).
    let landed = |sent: Option<u64>, queued: bool| !queued && sent.is_none_or(|at| now >= at.saturating_add(PALW_FILER_RESEND_DAA_V1));
    // The cap: every copy went out, and the last one has had its interval to land.
    let spent = |sends: u8, sent: Option<u64>, queued: bool| sends >= PALW_FILER_MAX_SENDS_V1 && landed(sent, queued);
    let queue = entry.in_queue;
    if let Some(record) = &read.consumed {
        // Convicted: the conviction stands whatever happens next; only R is at stake.
        if entry.commit_object.is_none() {
            return Step::Done(End::FiledDirect);
        }
        let Some(pending) = read.pending else {
            // The sweep closed the window (or the conviction collected nothing and opened no reward).
            return Step::Done(if entry.revealed_seen {
                End::Revealed
            } else {
                End::Forgone("no reward pends under the key: the window closed unrevealed, or the conviction opened none")
            });
        };
        // From here every "not now" WAITS for the sweep rather than ending the filing: each of them —
        // the key convicted on other evidence, this commitment's row missing or not strictly before
        // the conviction, an earlier commitment revealed — is a fact a reorg inside the window can
        // take back, and waiting costs one read a tick and sends nothing. The sweep ends it (above).
        let ours_leads = pending.best.is_some_and(|best| best.reporter == entry.reporter && best.commitment == entry.commitment);
        let revealable = pending.accepts_reveals()
            && pending.evidence_id == entry.evidence_id
            && !ours_leads
            && read.committed_daa.is_some_and(|committed_daa| {
                committed_daa < record.accepted_daa
                    && pending.best.is_none_or(|best| (committed_daa, entry.commitment) < (best.committed_daa, best.commitment))
            });
        let sendable = landed(entry.reveal_sent, queue.reveal) && entry.reveal_sends < PALW_FILER_MAX_SENDS_V1;
        return if revealable && now <= pending.reveal_until && sendable { Step::Reveal } else { Step::Wait };
    }
    // Not convicted yet. A commitment is pruned `window_court` after it was rooted when nothing
    // guards it, so a filing that has not convicted in a court window has nothing left to wait for.
    if now > entry.registered_daa.saturating_add(read.window_court) {
        return Step::Done(End::Expired);
    }
    if spent(entry.evidence_sends, entry.evidence_sent, queue.evidence) {
        return Step::Done(End::Stalled);
    }
    let file = if landed(entry.evidence_sent, queue.evidence) { Step::File } else { Step::Wait };
    if entry.commit_object.is_none() || !read.rcore_plus {
        return file;
    }
    match read.committed_daa {
        Some(committed_daa) if now >= committed_daa.saturating_add(PALW_FILER_COMMIT_DEPTH_DAA_V1) => file,
        Some(_) => Step::Wait,
        // The conviction never waits on R: past the landing margin, with no room to commit, or with
        // every copy of the commitment sent — the last one carried and given its interval — and
        // none rooted.
        None if entry.file_by_daa.is_some_and(|by| now >= by)
            || !read.reporter_may_commit
            || spent(entry.commit_sends, entry.commit_sent, queue.commit) =>
        {
            file
        }
        None if landed(entry.commit_sent, queue.commit) => Step::Commit,
        None => Step::Wait,
    }
}

/// **The DAA the object a sending step queues is due by** — what an earliest-deadline-first court
/// queue orders this filer's objects by (the priority lane's pluggable due-time function reads it
/// through [`PalwReporterFilerV1::queued_due_v1`]; the filer itself never relies on the queue's
/// order: every step is read off the chain's rows, never off what went out before it).
///
/// * **Evidence**: its `file_by_daa` — for a kind 4 the landing margin before the claim's receipt
///   deadline (the claim must still be live for S2; P2-6's accusation is due by the same margin) —
///   else, for a filing with no shorter deadline (P2-8c's kind 3, a kind 4 made past its claim's
///   receipt deadline: [`palw_kind4_file_by_v1`]), the end of its court window, after which the
///   filing expires unconvicted. Past it the evidence is overdue, and still sent.
/// * **Commitment**: [`PALW_FILER_COMMIT_DEPTH_DAA_V1`] before the evidence's date — the last DAA at
///   which a rooted commitment still lets the evidence wait for it.
/// * **Reveal**: R-3's reveal window's end, the pending reward's `reveal_until` (R-4: the
///   conviction's DAA plus `window_receipt`), after which the sweep pays whoever leads.
///
/// **Never dated before the filing was made** (the integration's review, F4): a filing made inside
/// its landing margin (its `file_by_daa` already past, so it goes out bare at once) is due at its
/// `registered_daa` — overdue for what it races, not ahead of every item queued since its margin
/// opened.
///
/// **An approximation, where the filing has no `file_by_daa`** (the review's F7): the court window's
/// end is the FILING's horizon, not its target's. Past the claim's Final a kind 3's or a late kind
/// 4's target is the claim's liability row, which expires at `final_daa + window_court`
/// (`palw_panel_liability_expiry_v1`); a filing registered after Final is dated `registered_daa −
/// final_daa` later than that. The chain's read of a filing (`PalwReporterFilingReadV1`) does not
/// name the target's expiry, and this node-only stage adds no consensus read, so the due date of a
/// late proof (a replay-sourced one, typically) may run past its target's: an earliest-deadline-first
/// lane reading it carries such a filing no sooner than its court window says. The conviction is
/// still filed at once (the date orders the queue; it never holds a filing back).
///
/// `None` for a step that queues nothing, and for a reveal the chain shows no pending reward for.
pub(crate) fn palw_filer_step_due_v1(entry: &PalwFilerEntryV1, step: PalwFilerStepV1, read: &PalwReporterFilingReadV1) -> Option<u64> {
    let born = entry.registered_daa;
    let evidence_due = entry.file_by_daa.unwrap_or_else(|| born.saturating_add(read.window_court));
    match step {
        PalwFilerStepV1::Commit => Some(evidence_due.saturating_sub(PALW_FILER_COMMIT_DEPTH_DAA_V1).max(born)),
        PalwFilerStepV1::File => Some(evidence_due.max(born)),
        PalwFilerStepV1::Reveal => read.pending.map(|pending| pending.reveal_until),
        PalwFilerStepV1::Wait | PalwFilerStepV1::Done(_) => None,
    }
}

/// What [`PalwReporterFilerV1::file`] made of a filing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwFileOutcomeV1 {
    /// In the book; `committed` says whether it will commit first.
    Queued { committed: bool },
    /// The book already holds a filing under this key (one offence, one filing).
    AlreadyFiled,
    /// The chain already convicted under this key.
    AlreadyConvicted,
    /// The accused is this node's own bond: never filed (the fold refuses `reporter == accused`).
    OwnBond,
    /// The gate refuses the evidence at the tip: nothing is spent on it.
    NotAdmitted(String),
    /// The book is full ([`PALW_FILER_MAX_ENTRIES_V1`]).
    Full,
    /// No tip state to read (off `ConsensusV2`), or no key to sign the commitment with.
    Unreadable,
}

/// The gate's verdict at the tip on `object` for `entry`'s filing (`None`: no tip state to read).
fn palw_filer_gate_v1(
    read: &mut impl FnMut(&PalwFilerEntryV1, Option<&PalwConsensusObjectV2>) -> Option<PalwReporterFilingReadV1>,
    entry: &PalwFilerEntryV1,
    object: &PalwConsensusObjectV2,
) -> Option<Result<(), String>> {
    read(entry, Some(object)).and_then(|gated| gated.object_gate)
}

/// **The book: every live filing, by offence key** — persisted in the node's state dir.
#[derive(Debug, Default)]
pub(crate) struct PalwReporterFilerV1 {
    entries: BTreeMap<Hash64, PalwFilerEntryV1>,
    path: Option<PathBuf>,
    dirty: bool,
    /// The (claim, capture) pairs J1 auto has probed — once each, keyed by the capture's digest so a
    /// stranger's garbage served first cannot spend the claim's probe before the borrowed capture
    /// arrives. Node memory only: a restart probes again, once.
    probed: HashSet<(Hash64, [u8; 32])>,
    /// J1 auto's probes left this tick.
    probes_left: usize,
    /// The window [`Self::digests`] counts in (`now_daa / PALW_J1_PROBE_WINDOW_DAA_V1`).
    probe_window: u64,
    /// Captures of each claim digested this window ([`PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1`]).
    digests: HashMap<Hash64, u8>,
    /// **The due DAA of every court-queue item this book queued** ([`palw_filer_step_due_v1`]; a
    /// hand-back's fallback is due by its filing's `file_by_daa`), by queue key — what the priority
    /// lane's due-time function reads ([`Self::queued_due_v1`]). Node memory, like the queue it
    /// dates; each tick keeps only the keys still queued.
    queued_due: HashMap<(Hash64, u32, bool), u64>,
}

/// The book's file magic: the format and its version, so a future book never misreads this one.
const PALW_FILER_FILE_MAGIC_V1: &[u8] = b"misaka-node/reporter-filer/v1\n";

impl PalwReporterFilerV1 {
    /// The book in `state_dir`, or an empty one — an unreadable file is logged and set aside
    /// (renamed `.unreadable`), never a reason not to start.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let path = state_dir.join("palw-reporter-filer.v1");
        let mut book = Self { path: Some(path.clone()), probes_left: PALW_J1_PROBES_PER_TICK_V1, ..Self::default() };
        let Ok(bytes) = std::fs::read(&path) else { return book };
        let parsed =
            bytes.strip_prefix(PALW_FILER_FILE_MAGIC_V1).and_then(|body| borsh::from_slice::<Vec<PalwFilerEntryV1>>(body).ok());
        match parsed {
            Some(entries) => {
                book.entries = entries.into_iter().map(|entry| (entry.offence_key, entry)).collect();
                if !book.entries.is_empty() {
                    info!("[{PALW_PANEL}] the reporter filer resumes {} filing(s) from {} (P2-8)", book.entries.len(), path.display());
                }
            }
            None => {
                let aside = path.with_extension("v1.unreadable");
                warn!(
                    "[{PALW_PANEL}] the reporter filer's book {} does not decode; set aside as {} (P2-8)",
                    path.display(),
                    aside.display()
                );
                let _ = std::fs::rename(&path, aside);
            }
        }
        book
    }

    /// Write the book if it changed: to a temporary file, then renamed over the old one, so a crash
    /// never leaves half a book.
    pub(crate) fn persist(&mut self) {
        if !self.dirty {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.dirty = false;
            return;
        };
        let mut bytes = PALW_FILER_FILE_MAGIC_V1.to_vec();
        let entries: Vec<&PalwFilerEntryV1> = self.entries.values().collect();
        bytes.extend(borsh::to_vec(&entries).expect("the book is borsh-serializable"));
        let temporary = path.with_extension("v1.tmp");
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&temporary, &bytes));
        match written.and_then(|()| std::fs::rename(&temporary, &path)) {
            Ok(()) => self.dirty = false,
            Err(e) => warn!(
                "[{PALW_PANEL}] cannot persist the reporter filer's book to {}: {e} — a restart would lose its salts",
                path.display()
            ),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn entry(&self, offence_key: &Hash64) -> Option<&PalwFilerEntryV1> {
        self.entries.get(offence_key)
    }

    /// **Whether a filing under `offence_key` is live in the book** — in flight, whichever lane filed
    /// it: the other lanes neither hand it again nor replay its claim while it is.
    pub(crate) fn holds(&self, offence_key: &Hash64) -> bool {
        self.entries.contains_key(offence_key)
    }

    /// **The DAA a court-queue item this book queued is due by** — the commitment, the evidence or
    /// the reveal of a live filing under its round ([`palw_filer_step_due_v1`]), or a stalled
    /// filing's fallback — `None` for every other item (another lane's states its own) and for a
    /// fallback whose filing had no deadline. The hook an earliest-deadline-first priority lane plugs
    /// this filer into: `(key, round, responder)` is the queue entry's own key. A filing with no
    /// `file_by_daa` (P2-8c's kind 3, a kind 4 made past its receipt deadline) is dated by its court
    /// window's end, which past the claim's Final can be later than its target's own expiry — the
    /// approximation [`palw_filer_step_due_v1`] states. The panel's earliest-deadline-first lane reads
    /// the dates in bulk ([`Self::queued_dues_v1`]) after each of the filer's passes.
    #[allow(dead_code)]
    pub(crate) fn queued_due_v1(&self, queue_key: &(Hash64, u32, bool)) -> Option<u64> {
        self.queued_due.get(queue_key).copied()
    }

    /// Every dated item this book has queued, `(queue key, due DAA)` — [`Self::queued_due_v1`] in
    /// bulk, for a lane that keeps its due dates in one map (`court_due.extend(..)`).
    pub(crate) fn queued_dues_v1(&self) -> impl Iterator<Item = ((Hash64, u32, bool), u64)> + '_ {
        self.queued_due.iter().map(|(key, due)| (*key, *due))
    }

    /// **The capture arm backs the filing in the book under `offence_key`** (the integration of P2-8
    /// with P2-8b): its own, or one P2-8b's replay filer made first under the claim's one kind-4 key.
    /// The filing is marked [`PalwFilerEntryV1::capture_arm_backed`] — the claim now sits in the
    /// seat's `accused` set on its account, and a hand-back that queues no fallback lets it go, even
    /// when the capture arm had no accusation to give (the integration's review, F2) — and the
    /// capture arm's one-move court accusation becomes its `fallback` if it has none, so a stall of
    /// the replay's filing still hands the proven fault to the court (F3's guarantee whichever lane
    /// filed first). Returns whether the fallback was adopted.
    pub(crate) fn back_by_capture_arm_v1(&mut self, offence_key: &Hash64, fallback: Option<(Hash64, PalwConsensusObjectV2)>) -> bool {
        let Some(entry) = self.entries.get_mut(offence_key) else { return false };
        entry.capture_arm_backed = true;
        if entry.fallback.is_some() || fallback.is_none() {
            return false;
        }
        entry.fallback = fallback;
        self.dirty = true;
        true
    }

    /// **The entry point: take one filing into the book.** `read` is the chain's read of it WITH the
    /// gate's verdict on its evidence (the caller asked for it with the commitment `salt` makes);
    /// `sign` signs the commitment with this node's bond key. Commits only where R-3 is live and the
    /// bond may root one; otherwise the filing goes out unprotected.
    pub(crate) fn file(
        &mut self,
        filing: PalwConvictionFilingV1,
        reporter: PalwBondKeyV2,
        read: &PalwReporterFilingReadV1,
        network_domain: &Hash64,
        salt: [u8; 32],
        sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> PalwFileOutcomeV1 {
        if filing.accused == reporter {
            return PalwFileOutcomeV1::OwnBond;
        }
        if self.entries.contains_key(&filing.offence_key) {
            return PalwFileOutcomeV1::AlreadyFiled;
        }
        if read.consumed.is_some() {
            return PalwFileOutcomeV1::AlreadyConvicted;
        }
        match &read.object_gate {
            Some(Ok(())) => {}
            Some(Err(why)) => return PalwFileOutcomeV1::NotAdmitted(why.clone()),
            None => return PalwFileOutcomeV1::Unreadable,
        }
        if self.entries.len() >= PALW_FILER_MAX_ENTRIES_V1 {
            return PalwFileOutcomeV1::Full;
        }
        let built = (read.rcore_plus && read.reporter_may_commit)
            .then(|| {
                kaspa_consensus_core::palw_state_v2::palw_reporter_commit_object_v1(
                    network_domain,
                    &filing.offence_key,
                    &filing.evidence_id,
                    reporter,
                    &salt,
                    sign,
                )
            })
            .flatten();
        let commitment = kaspa_consensus_core::palw_state_v2::palw_reporter_commitment_v1(
            &filing.offence_key,
            &filing.evidence_id,
            &reporter,
            &salt,
        );
        debug_assert!(built.as_ref().is_none_or(|(built, _)| *built == commitment), "one commitment function");
        let committed = built.is_some();
        self.entries.insert(
            filing.offence_key,
            PalwFilerEntryV1 {
                offence_key: filing.offence_key,
                evidence_id: filing.evidence_id,
                accused: filing.accused,
                claim_id: filing.claim_id,
                object: filing.object,
                file_by_daa: filing.file_by_daa,
                origin: filing.origin,
                reporter,
                salt,
                commitment,
                commit_object: built.map(|(_, object)| object),
                fallback: filing.fallback,
                registered_daa: read.now_daa,
                commit_sent: None,
                evidence_sent: None,
                reveal_sent: None,
                commit_sends: 0,
                evidence_sends: 0,
                reveal_sends: 0,
                revealed_seen: false,
                in_queue: PalwFilerInQueueV1::default(),
                capture_arm_backed: false,
                gate_refused_at: None,
            },
        );
        self.dirty = true;
        PalwFileOutcomeV1::Queued { committed }
    }

    /// **One tick of the book.** For each filing: note which of its objects still wait in the court
    /// queue, read the chain (`read(entry, None)`), take the step, and — for a send — ask the gate
    /// (`read(entry, Some(object))`) on the evidence (at most once a
    /// [`PALW_FILER_GATE_RETRY_DAA_V1`] after it refused), and before a commitment on the commitment
    /// too (one it refuses is given up: the filing goes on without it), then queue the object on the
    /// court queue's priority lane and count the copy. A finished filing leaves the book, its queued
    /// objects leave `court_pending`, its debounce keys leave `court_moved` (nothing else prunes
    /// them), and a stalled or expired one is handed back ([`Self::hand_back_v1`]). Returns how each
    /// finished filing ended.
    pub(crate) fn tick(
        &mut self,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
        accused: &mut HashSet<Hash64>,
        mut read: impl FnMut(&PalwFilerEntryV1, Option<&PalwConsensusObjectV2>) -> Option<PalwReporterFilingReadV1>,
    ) -> Vec<(PalwFilerEntryV1, PalwFilerEndV1)> {
        use PalwFilerStepV1 as Step;
        self.probes_left = PALW_J1_PROBES_PER_TICK_V1;
        let mut ended = Vec::new();
        for key in self.entries.keys().copied().collect::<Vec<_>>() {
            // FIFO-free (the queue's order is not the filer's): a copy still waiting in the queue is
            // neither re-sent nor counted landed (`in_queue`, read by the rule), and a copy's interval
            // runs from when it was CARRIED (`court_moved`, stamped by the carrier lane at
            // submission), not from when it was queued — an item a deadline-ordered queue held
            // behind earlier-due ones is not re-queued, stalled or given up before it goes out.
            let queued = |court_pending: &Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>, round: u32| {
                court_pending.iter().any(|(k, r, responder, _)| (*k, *r, *responder) == (key, round, false))
            };
            let live = self.entries.get_mut(&key).expect("a key just listed");
            live.in_queue = PalwFilerInQueueV1 {
                commit: queued(court_pending, PALW_FILER_ROUND_COMMIT_V1),
                evidence: queued(court_pending, PALW_FILER_ROUND_EVIDENCE_V1),
                reveal: queued(court_pending, PALW_FILER_ROUND_REVEAL_V1),
            };
            for (round, sent) in [
                (PALW_FILER_ROUND_COMMIT_V1, &mut live.commit_sent),
                (PALW_FILER_ROUND_EVIDENCE_V1, &mut live.evidence_sent),
                (PALW_FILER_ROUND_REVEAL_V1, &mut live.reveal_sent),
            ] {
                if let Some(carried) = court_moved.get(&(key, round, false)).copied()
                    && sent.is_some_and(|queued| carried > queued)
                {
                    *sent = Some(carried);
                    self.dirty = true;
                }
            }
            let entry = self.entries.get(&key).expect("a key just listed").clone();
            let Some(chain) = read(&entry, None) else { continue };
            let leads = chain.pending.is_some_and(|pending| {
                pending.best.is_some_and(|best| best.reporter == entry.reporter && best.commitment == entry.commitment)
            });
            if leads && !entry.revealed_seen {
                self.entries.get_mut(&key).expect("live").revealed_seen = true;
                self.dirty = true;
            }
            let now = chain.now_daa;
            let mut step = palw_filer_step_v1(self.entries.get(&key).expect("live"), &chain);
            match step {
                Step::Wait => continue,
                Step::Done(end) => {
                    let entry = self.entries.remove(&key).expect("live");
                    self.dirty = true;
                    self.hand_back_v1(&entry, end, court_pending, court_moved, accused, &mut read);
                    ended.push((entry, end));
                    continue;
                }
                Step::Commit | Step::File | Step::Reveal => {}
            }
            // Nothing is spent on evidence the chain refuses: the commitment and the evidence each
            // wait for the gate's word at the tip (a court opened on the claim meanwhile, say) —
            // asked again a re-plan after it refused, never on every tick (F6).
            if matches!(step, Step::Commit | Step::File) {
                if entry.gate_refused_at.is_some_and(|at| now < at.saturating_add(PALW_FILER_GATE_RETRY_DAA_V1)) {
                    continue;
                }
                match palw_filer_gate_v1(&mut read, &entry, &entry.object) {
                    Some(Ok(())) => self.entries.get_mut(&key).expect("live").gate_refused_at = None,
                    Some(Err(why)) => {
                        crate::palw_backends::note_throttled_v1("panel-reporter-filer-gate", || {
                            format!(
                                "[{PALW_PANEL}] claim {}: the filing {key} waits — the gate refuses its evidence: {why} (P2-8)",
                                entry.claim_id
                            )
                        });
                        self.entries.get_mut(&key).expect("live").gate_refused_at = Some(now);
                        continue;
                    }
                    None => continue,
                }
            }
            // Nor on a commitment it refuses (a signer that is not the bond's registered key): that
            // commitment can never root, so the filing gives it up and the evidence goes out
            // without one — R forgone, the conviction on time.
            if step == Step::Commit {
                let commit = entry.commit_object.as_ref().expect("a Commit step has a commitment to send");
                match palw_filer_gate_v1(&mut read, &entry, commit) {
                    Some(Ok(())) => {}
                    Some(Err(why)) => {
                        warn!(
                            "[{PALW_PANEL}] claim {}: the gate refuses this node's commitment to offence {key}: {why} — the evidence goes out without one (P2-8)",
                            entry.claim_id
                        );
                        let live = self.entries.get_mut(&key).expect("live");
                        live.commit_object = None;
                        self.dirty = true;
                        // The evidence's own queue view is on `live` (`in_queue`): File only if
                        // no copy of it waits in the queue.
                        step = palw_filer_step_v1(live, &chain);
                        if step != Step::File {
                            continue;
                        }
                    }
                    None => continue,
                }
            }
            let (round, object) = match step {
                Step::Commit => {
                    (PALW_FILER_ROUND_COMMIT_V1, entry.commit_object.clone().expect("a Commit step has a commitment to send"))
                }
                Step::File => (PALW_FILER_ROUND_EVIDENCE_V1, entry.object.clone()),
                Step::Reveal => (
                    PALW_FILER_ROUND_REVEAL_V1,
                    PalwConsensusObjectV2::ReporterRevealed { offence_key: key, reporter: entry.reporter, salt: entry.salt },
                ),
                Step::Wait | Step::Done(_) => unreachable!("sends only"),
            };
            let live = self.entries.get_mut(&key).expect("live");
            match palw_filer_step_due_v1(live, step, &chain) {
                Some(due) => self.queued_due.insert((key, round, false), due),
                None => self.queued_due.remove(&(key, round, false)),
            };
            let (sent, sends) = match step {
                Step::Commit => (&mut live.commit_sent, &mut live.commit_sends),
                Step::File => (&mut live.evidence_sent, &mut live.evidence_sends),
                _ => (&mut live.reveal_sent, &mut live.reveal_sends),
            };
            *sent = Some(now);
            *sends = sends.saturating_add(1);
            let copy = *sends;
            self.dirty = true;
            info!(
                "[{PALW_PANEL}] claim {}: {} for offence {key} ({:?}, R-3) queued at DAA {now}, copy {copy} of at most {PALW_FILER_MAX_SENDS_V1} (P2-8)",
                entry.claim_id,
                match step {
                    Step::Commit => "the reporter's commitment",
                    Step::File => "the evidence",
                    _ => "the reporter's reveal",
                },
                entry.origin
            );
            court_pending.push((key, round, false, object));
        }
        court_pending.retain(|(key, round, responder, object)| {
            !palw_filer_queued_v1(*round, *responder, object) || self.entries.contains_key(key)
        });
        // Dates only for what is still queued (the carrier lane drains the queue between ticks).
        self.queued_due.retain(|queue_key, _| court_pending.iter().any(|(k, r, responder, _)| (*k, *r, *responder) == *queue_key));
        ended
    }

    /// **A finished filing's leftovers, and the hand-back of one that did not convict.** Its three
    /// debounce keys leave `court_moved` (the review of P2-8, F5: `carry_priority_v1` inserts one per
    /// carrier and the filer never reads them). `Expired` or `Stalled` (F3): a proven fault must not
    /// go unfiled because kind 4 did not land, so its `fallback` — the capture arm's one-move
    /// accusation — is queued if the gate admits it now; if there is none, or the gate refuses it,
    /// a claim the capture arm holds in the seat's `accused` set on this filing's account (its own
    /// filing, or one it backed: [`PalwFilerEntryV1::capture_arm_backed`]) leaves it, so the
    /// named-leaf pursuit (ADR-0111 Decision 6) may still file. A J1 filing that expired releases its
    /// claim's probes (a stalled one keeps them: the fold drops what the gate admits, and probing
    /// again would only file it again).
    /// P2-8b's and P2-8c's filings need nothing here: their lanes read the key's leaving as the end
    /// of the flight and ask the chain again ([`PALW_FILER_HAND_OFFS_PER_OFFENCE_V1`]).
    fn hand_back_v1(
        &mut self,
        entry: &PalwFilerEntryV1,
        end: PalwFilerEndV1,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
        accused: &mut HashSet<Hash64>,
        read: &mut impl FnMut(&PalwFilerEntryV1, Option<&PalwConsensusObjectV2>) -> Option<PalwReporterFilingReadV1>,
    ) {
        for round in [PALW_FILER_ROUND_COMMIT_V1, PALW_FILER_ROUND_EVIDENCE_V1, PALW_FILER_ROUND_REVEAL_V1] {
            court_moved.remove(&(entry.offence_key, round, false));
        }
        if !matches!(end, PalwFilerEndV1::Expired | PalwFilerEndV1::Stalled) {
            return;
        }
        let fallback_queued =
            entry.fallback.as_ref().is_some_and(|(queue_key, object)| match palw_filer_gate_v1(read, entry, object) {
                Some(Ok(())) => {
                    if !court_pending.iter().any(|(k, r, responder, _)| (*k, *r, *responder) == (*queue_key, 0, false)) {
                        court_pending.push((*queue_key, 0, false, object.clone()));
                    }
                    // The accusation must land while the claim is live, as the filing it replaces.
                    if let Some(by) = entry.file_by_daa {
                        self.queued_due.insert((*queue_key, 0, false), by);
                    }
                    info!(
                        "[{PALW_PANEL}] claim {}: offence {} ({:?}) ended {end:?}; its fallback, {}, is filed instead (P2-8)",
                        entry.claim_id,
                        entry.offence_key,
                        entry.origin,
                        object_name(object)
                    );
                    true
                }
                refused => {
                    info!(
                        "[{PALW_PANEL}] claim {}: offence {} ({:?}) ended {end:?}; its fallback is not filed: {refused:?} (P2-8)",
                        entry.claim_id, entry.offence_key, entry.origin
                    );
                    false
                }
            });
        // The capture arm's own filing, or another lane's it backed ([`Self::back_by_capture_arm_v1`],
        // with an accusation to adopt or without one — F2): the claim is the capture arm's to let
        // go of.
        if (entry.origin == PalwFilingOriginV1::CaptureArm || entry.capture_arm_backed) && !fallback_queued {
            accused.remove(&entry.claim_id);
        }
        if entry.origin == PalwFilingOriginV1::BorrowedRoot && end == PalwFilerEndV1::Expired {
            self.release_claim_probes_v1(&entry.claim_id);
        }
    }

    /// **J1 auto's gate**: a probe left this tick, this claim's captures not digested
    /// [`PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1`] times this window (checked BEFORE the digest, which is
    /// whole-capture work), and this capture of `claim` not probed yet. Takes the probe and returns
    /// its key (handed back by [`Self::release_probe`] when the probe could not run).
    pub(crate) fn take_probe(&mut self, claim: Hash64, capture: &[u8], now_daa: u64) -> Option<(Hash64, [u8; 32])> {
        if self.probes_left == 0 {
            return None;
        }
        let window = now_daa / PALW_J1_PROBE_WINDOW_DAA_V1;
        if window != self.probe_window {
            self.probe_window = window;
            self.digests.clear();
        }
        let digested = self.digests.entry(claim).or_insert(0);
        if *digested >= PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1 {
            return None;
        }
        *digested += 1;
        let digest = blake2b_simd::Params::new().hash_length(32).key(b"misaka-node/j1-probe/v1").hash(capture);
        let key = (claim, <[u8; 32]>::try_from(digest.as_bytes()).expect("32 bytes"));
        if self.probed.contains(&key) {
            return None;
        }
        // Node memory only, and bounded: a node that probed this many captures forgets them at once
        // rather than one at a time — a capture probed again costs one probe, and the window's digest
        // count bounds what a forgotten claim's garbage can take.
        if self.probed.len() >= 16 * PALW_FILER_MAX_ENTRIES_V1 {
            self.probed.clear();
        }
        self.probes_left -= 1;
        self.probed.insert(key);
        Some(key)
    }

    /// A probe that did not run (the ledger refused it): asked again on a later tick.
    pub(crate) fn release_probe(&mut self, key: (Hash64, [u8; 32])) {
        self.probed.remove(&key);
    }

    /// Every probe of `claim` forgotten — a J1 filing that expired may be found and filed again.
    pub(crate) fn release_claim_probes_v1(&mut self, claim: &Hash64) {
        self.probed.retain(|(probed, _)| probed != claim);
        self.digests.remove(claim);
    }
}

/// **J1 auto's detector** (ADR-0152 §3.9 "borrowed"; J-6's borrowed path): `bytes` reproduce the
/// claim's committed roots with no job bound — the anchor, the attempt draw and the job pin blanked,
/// the checks `verify_material` skips for a caller with no block — and do NOT reproduce them under
/// the claim's own job. That is the claim's committed execution answering another job: another
/// claim's genuine roots borrowed, or an honest run relabelled. Returns the binding the capture
/// commits to — the out-of-range event opening the DA answers read a binding by (`u32::MAX`,
/// `u8::MAX`: refuted by the binding alone, so nothing is opened).
///
/// A candidate only: WHICH identity check fails, and whether any does, is the fold's
/// (`palw_binding_identity_fault_v1`, run by the gate on the `IdentityMismatch` this binding is filed
/// in); nothing here restates it. Whole-capture work: the caller reserves and offloads it.
pub(crate) fn palw_borrowed_root_binding_v1(
    backend: &dyn PalwExecutionBackendV1,
    bytes: &[u8],
    roots: PalwClaimRootsV1,
) -> Option<kaspa_consensus_core::palw_step_leg::PalwStepBindingV2> {
    let unbound = PalwClaimRootsV1 { anchor: Hash64::default(), attempt_draw: None, job_pin: None, ..roots };
    if backend.verify_material(bytes, roots) == PalwMaterialVerdictV1::Matches
        || backend.verify_material(bytes, unbound) != PalwMaterialVerdictV1::Matches
    {
        return None;
    }
    backend.disclose_trace_event(bytes, u32::MAX, u8::MAX).ok().map(|disclosure| disclosure.binding().clone())
}

/// **The landing margin of an automatic kind-4 filing**: the evidence must fold while the claim is
/// still live for S2 (a claim voided at its receipt deadline is charged only its S0′ forfeit), so
/// past `deadline − 60` it stops waiting on its commitment — P2-6's margin, for the same reason.
pub(crate) fn palw_filer_file_by_v1(receipt_deadline: u64) -> u64 {
    receipt_deadline.saturating_sub(PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1)
}

/// **An automatic kind 4's `file_by_daa`, for a filing made at `now_daa`** (the integration's review,
/// F4): the landing margin ([`palw_filer_file_by_v1`]) while the claim's receipt deadline is still
/// ahead — the S2 race is live, so past the margin the evidence goes out at once, bare — and `None`
/// once the deadline has passed. Kind 4 is still admissible there (the claim's liability or vesting
/// row is its target), but the race it had is over: landing now or a commitment's two DAA later
/// changes nothing, so the filing commits first and keeps R, as P2-8c's kind 3 does, and is dated by
/// its court window ([`palw_filer_step_due_v1`]). A bisection that finished late gave R up for
/// nothing before this.
pub(crate) fn palw_kind4_file_by_v1(receipt_deadline: u64, now_daa: u64) -> Option<u64> {
    (now_daa < receipt_deadline).then(|| palw_filer_file_by_v1(receipt_deadline))
}

/// **SR-8 / J-4 (P2-8): the capture arm's `ExecutorRefuted` over the fault it proved** — pure, so the
/// one thing the capture arm builds is tested without a node (the review of P2-8, F2): kind 4 over
/// `StepArithmetic { refutation, operand_openings }` with the sampler's prompt tile (already in the
/// network's carriage: the sampler graded exactly this), naming the claim's executor, keyed on the
/// per-claim ledger id, landing by [`palw_kind4_file_by_v1`] as of `now_daa`. `None` below `Params::palw_rcore_plus`
/// (`rcore_plus` is the node's reading of it at the tip: there the v1 court path is the whole path)
/// and against this node's own bond (the fold refuses `reporter == accused`, and a seat never
/// accuses its own claim).
#[allow(clippy::too_many_arguments)]
pub(crate) fn palw_capture_arm_filing_v1(
    rcore_plus: bool,
    own_bond: PalwBondKeyV2,
    executor: PalwBondKeyV2,
    claim_id: Hash64,
    receipt_deadline: u64,
    now_daa: u64,
    refutation: &PalwExecutionStepRefutationV1,
    openings: &[PalwArtifactOpeningV1],
    prompt_opening: &Option<PalwPromptIdsOpeningV1>,
) -> Option<PalwConvictionFilingV1> {
    if !rcore_plus || executor == own_bond {
        return None;
    }
    let object = palw_executor_refuted_object_v1(
        executor,
        claim_id,
        PalwPanelContradictionV1::StepArithmetic { refutation: refutation.clone(), operand_openings: openings.to_vec() },
        prompt_opening.clone(),
    );
    PalwConvictionFilingV1::of_offence(
        object,
        claim_id,
        palw_kind4_file_by_v1(receipt_deadline, now_daa),
        PalwFilingOriginV1::CaptureArm,
    )
}

/// What the capture arm's decision made of a proven fault ([`palw_capture_arm_files_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PalwCaptureArmFiledV1 {
    /// The seat accused the claim already (once per claim): nothing was done.
    AlreadyAccused,
    /// Kind 4 is the reporter filer's — this filing (`Queued`), one another lane filed first under
    /// the claim's one kind-4 key (`AlreadyFiled`: the capture arm backs it), or the chain's
    /// conviction (`AlreadyConvicted`) — so the court is not also filed.
    Kind4(PalwFileOutcomeV1),
    /// The one-move court's accusation was queued instead.
    Court,
    /// Nothing to file: no kind 4 taken and no accusation to queue.
    Nothing,
}

/// **SR-8 / J-4 (P2-8): what the capture arm files for a proven fault, once per claim** — pure over
/// the book, the seat's `accused` set and the court queue, with `file` the node's door into the book
/// (the panel's [`PalwPanelService::reporter_filer_file_v1`]; a test's stubbed chain), so the order
/// is tested without a node (the integration's review, F3). Kind 4 (`refuted`, past
/// `Params::palw_rcore_plus` only) through the reporter filer first, with the one-move court's
/// accusation (`court`) as its fallback should it stall; the claim is then taken care of (`accused`)
/// and the court is NOT also filed: both routes convict the claim once — the void is the marker — so
/// the second only races the first for nothing, and the court's reward, named for whoever signs the
/// accusation, is the one a mempool observer can take. Kind 4 already in the book under the claim's
/// key — P2-8b's replay filed it first — is BACKED ([`PalwReporterFilerV1::back_by_capture_arm_v1`]):
/// it takes the accusation as its fallback, and the claim leaves `accused` if it ends unconvicted with
/// no fallback queued, accusation or none (F2). Otherwise — below the fence, against this node's own
/// bond, the gate refusing kind 4 (a held class whose leaf needs a dissection, which only the court
/// can open), a full book — the accusation, exactly as the seat filed it before P2-8.
#[allow(clippy::too_many_arguments)]
pub(crate) fn palw_capture_arm_files_v1(
    filer: &mut PalwReporterFilerV1,
    accused: &mut HashSet<Hash64>,
    court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
    claim_id: Hash64,
    leaf: u64,
    refuted: Option<PalwConvictionFilingV1>,
    court: Option<(Hash64, PalwConsensusObjectV2)>,
    file: impl FnOnce(&mut PalwReporterFilerV1, PalwConvictionFilingV1) -> PalwFileOutcomeV1,
) -> PalwCaptureArmFiledV1 {
    if accused.contains(&claim_id) {
        return PalwCaptureArmFiledV1::AlreadyAccused;
    }
    if let Some(filing) = refuted {
        let key = filing.offence_key;
        match file(filer, filing.with_fallback(court.clone())) {
            outcome @ (PalwFileOutcomeV1::Queued { .. } | PalwFileOutcomeV1::AlreadyFiled | PalwFileOutcomeV1::AlreadyConvicted) => {
                // This filing, or P2-8b's replay's under the same key: backed by this accusation.
                filer.back_by_capture_arm_v1(&key, court);
                accused.insert(claim_id);
                return PalwCaptureArmFiledV1::Kind4(outcome);
            }
            PalwFileOutcomeV1::NotAdmitted(_)
            | PalwFileOutcomeV1::OwnBond
            | PalwFileOutcomeV1::Full
            | PalwFileOutcomeV1::Unreadable => {}
        }
    }
    let Some((session_id, object)) = court else { return PalwCaptureArmFiledV1::Nothing };
    info!("[{PALW_PANEL}] claim {claim_id}: accusing leaf {leaf} in the one-move court (session {session_id})");
    accused.insert(claim_id);
    court_pending.push((session_id, 0, false, object));
    PalwCaptureArmFiledV1::Court
}

/// **The door the other lanes file through** (the integration of P2-8b and P2-8c with P2-8): the
/// two questions a lane asks of the reporter filer, and nothing else — so a lane's book is tested
/// against the filer's real rule ([`PalwReporterFilerV1::file`]) without a node, and the panel's door
/// ([`PalwPanelConvictionDoorV1`]) is the one place a lane's filing meets this node's bond, salt and
/// signer.
pub(crate) trait PalwConvictionDoorV1 {
    /// Whether the book holds a filing under `offence_key` — in flight, whichever lane filed it.
    fn holds(&self, offence_key: &Hash64) -> bool;
    /// Hand `filing` to the filer: committed over this node's bond, filed, revealed — or refused
    /// ([`PalwFileOutcomeV1`]: already in the book, already convicted, this node's own bond, not
    /// admitted by the gate, a full book, no tip state).
    fn file(&mut self, filing: PalwConvictionFilingV1) -> PalwFileOutcomeV1;
}

/// **The panel's door**: the book in the panel loop, this node's bond and domain, and
/// [`PalwPanelService::reporter_filer_file_v1`] (a fresh salt, the chain's read with the gate on the
/// evidence, the commitment signed with the bond key, the book persisted).
pub(crate) struct PalwPanelConvictionDoorV1<'a> {
    service: &'a PalwPanelService,
    session: &'a kaspa_consensusmanager::ConsensusProxy,
    filer: &'a mut PalwReporterFilerV1,
    reporter: PalwBondKeyV2,
    network_domain: Hash64,
}

impl PalwConvictionDoorV1 for PalwPanelConvictionDoorV1<'_> {
    fn holds(&self, offence_key: &Hash64) -> bool {
        self.filer.holds(offence_key)
    }

    fn file(&mut self, filing: PalwConvictionFilingV1) -> PalwFileOutcomeV1 {
        self.service.reporter_filer_file_v1(self.session, self.filer, filing, self.reporter, &self.network_domain)
    }
}

/// **A door over a REAL book with a stubbed chain**, for the lanes' tests: the book's own rule
/// ([`PalwReporterFilerV1::file`]: one filing an offence at a time, never this node's bond, the gate's
/// word) at `read`, a fixed salt and signer; every hand-off logged with its outcome.
#[cfg(test)]
pub(crate) struct PalwBookDoorV1 {
    pub book: PalwReporterFilerV1,
    pub reporter: PalwBondKeyV2,
    /// The chain's read of every filing, the gate's verdict on its evidence included.
    pub read: PalwReporterFilingReadV1,
    /// Every filing handed, with the book's answer, in order.
    pub handed: Vec<(PalwConvictionFilingV1, PalwFileOutcomeV1)>,
}

#[cfg(test)]
impl PalwBookDoorV1 {
    /// An empty book for `reporter` at `now_daa`: R-3 live, room to commit, nothing rooted or
    /// convicted, the gate admitting.
    pub(crate) fn new(reporter: PalwBondKeyV2, now_daa: u64) -> Self {
        let read = PalwReporterFilingReadV1 {
            now_daa,
            rcore_plus: true,
            window_court: 3_000,
            reporter_may_commit: true,
            committed_daa: None,
            consumed: None,
            pending: None,
            awarded: None,
            object_gate: Some(Ok(())),
        };
        Self { book: PalwReporterFilerV1::default(), reporter, read, handed: Vec::new() }
    }

    /// The offence keys handed and taken (`Queued`), in order.
    pub(crate) fn queued(&self) -> Vec<Hash64> {
        self.handed
            .iter()
            .filter(|(_, outcome)| matches!(outcome, PalwFileOutcomeV1::Queued { .. }))
            .map(|(filing, _)| filing.offence_key)
            .collect()
    }

    /// **Every live filing ends unconvicted**: the book ticked past its court window, as the real
    /// tick ends one (`Expired`), its leftovers pruned. Returns how many ended.
    pub(crate) fn expire_all(&mut self) -> usize {
        let past = PalwReporterFilingReadV1 { now_daa: self.read.now_daa + self.read.window_court + 1, ..self.read.clone() };
        let ended = self.book.tick(&mut Vec::new(), &mut HashMap::new(), &mut HashSet::new(), |_, _| Some(past.clone()));
        assert!(ended.iter().all(|(_, end)| *end == PalwFilerEndV1::Expired));
        ended.len()
    }
}

#[cfg(test)]
impl PalwConvictionDoorV1 for PalwBookDoorV1 {
    fn holds(&self, offence_key: &Hash64) -> bool {
        self.book.holds(offence_key)
    }

    fn file(&mut self, filing: PalwConvictionFilingV1) -> PalwFileOutcomeV1 {
        let signer = |message: &[u8], context: &[u8]| {
            Some(blake2b_simd::Params::new().hash_length(64).key(context).hash(message).as_bytes().to_vec())
        };
        let outcome = self.book.file(filing.clone(), self.reporter, &self.read, &Hash64::from_u64_word(0xD0), [7; 32], signer);
        self.handed.push((filing, outcome.clone()));
        outcome
    }
}

/// **A real step refutation on the floor** — leaf 0 of a floor run, with its rows and the network's
/// Merkle prompt carriage: what the capture sampler holds at `FaultAt`. For the lanes' tests, so
/// the evidence they hand the book is keyed exactly as the chain keys it (an execution-proving kind
/// 3 or kind 4). Built once per test binary.
#[cfg(test)]
pub(crate) fn palw_floor_step_refutation_v1()
-> &'static (PalwExecutionStepRefutationV1, Vec<PalwArtifactOpeningV1>, Option<PalwPromptIdsOpeningV1>) {
    static REFUTATION: std::sync::OnceLock<(
        PalwExecutionStepRefutationV1,
        Vec<PalwArtifactOpeningV1>,
        Option<PalwPromptIdsOpeningV1>,
    )> = std::sync::OnceLock::new();
    REFUTATION.get_or_init(|| {
        use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("court");
        let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("root");
        let backend =
            misaka_palw_base0::backend::Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
                .with_step_ladder_cap(court.max_step_leaf_count())
                .with_prompt_ids_form(kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1);
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x5A_4D)).expect("job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
        let run = backend.execute(&job, &prompt).expect("a run");
        let refutation = backend.refutation_for_index(&run.material, 0).expect("leaf 0 opens");
        let openings = backend.operand_openings_for(&refutation).expect("its rows");
        let (refutation, prompt_opening) = kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            refutation,
        )
        .expect("the prover's list is the job's");
        (refutation, openings, prompt_opening)
    })
}

/// **The lanes' `file_by` for an `ExecutorRefuted` handed at `now_daa`** — the capture arm's own
/// ([`palw_kind4_file_by_v1`] of the duty's receipt deadline, which is never later than the chain's
/// per-claim one), so kind 4 from a replay waits on its commitment no longer than kind 4 from a
/// capture sample does, and one made past the receipt deadline commits first (F4).
pub(crate) fn palw_executor_refuted_file_by_v1(duty: &PalwSeatDutyV2, now_daa: u64) -> Option<u64> {
    palw_kind4_file_by_v1(duty.receipt_deadline, now_daa)
}

impl PalwPanelService {
    /// **The panel's [`PalwConvictionDoorV1`]** over the loop's book, for this node's `reporter` bond.
    pub(crate) fn conviction_door_v1<'a>(
        &'a self,
        session: &'a kaspa_consensusmanager::ConsensusProxy,
        filer: &'a mut PalwReporterFilerV1,
        reporter: PalwBondKeyV2,
        network_domain: Hash64,
    ) -> PalwPanelConvictionDoorV1<'a> {
        PalwPanelConvictionDoorV1 { service: self, session, filer, reporter, network_domain }
    }

    /// **The filer's entry point on the panel**: a fresh salt, the chain's read of the filing with
    /// the gate's verdict on its evidence, and the book's decision ([`PalwReporterFilerV1::file`]),
    /// the commitment signed with this node's bond key.
    pub(super) fn reporter_filer_file_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReporterFilerV1,
        filing: PalwConvictionFilingV1,
        reporter: PalwBondKeyV2,
        network_domain: &Hash64,
    ) -> PalwFileOutcomeV1 {
        if filing.accused == reporter {
            return PalwFileOutcomeV1::OwnBond;
        }
        let mut salt = [0u8; 32];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
        let commitment = kaspa_consensus_core::palw_state_v2::palw_reporter_commitment_v1(
            &filing.offence_key,
            &filing.evidence_id,
            &reporter,
            &salt,
        );
        let Some(read) = session.palw_reporter_filing_read_v1(filing.offence_key, commitment, reporter, Some(filing.object.clone()))
        else {
            return PalwFileOutcomeV1::Unreadable;
        };
        let (claim, origin, key) = (filing.claim_id, filing.origin, filing.offence_key);
        let outcome = filer.file(filing, reporter, &read, network_domain, salt, |message, context| self.sign(message, context));
        match &outcome {
            PalwFileOutcomeV1::Queued { committed } => info!(
                "[{PALW_PANEL}] claim {claim}: filing offence {key} ({origin:?}) — {} (ADR-0152 R-3, P2-8)",
                if *committed { "committing first, then the evidence, then the reveal" } else { "the evidence alone, no commitment" }
            ),
            PalwFileOutcomeV1::NotAdmitted(why) => {
                info!("[{PALW_PANEL}] claim {claim}: offence {key} ({origin:?}) is not filed — the gate refuses it: {why} (P2-8)")
            }
            other => trace!("[{PALW_PANEL}] claim {claim}: offence {key} ({origin:?}): {other:?} (P2-8)"),
        }
        filer.persist();
        outcome
    }

    /// **The filer's tick on the panel**: every live filing a step (see [`PalwReporterFilerV1::tick`]),
    /// read through `palw_reporter_filing_read_v1`, the finished ones pruned from `court_moved` and the
    /// stalled ones handed back to `accused` or the court, then the book persisted. Runs after the
    /// verdict loop (a fault found this tick is committed this tick) and before the submitter's half
    /// (which carries what it queued).
    pub(super) fn reporter_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReporterFilerV1,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
        accused: &mut HashSet<Hash64>,
    ) {
        let ended = filer.tick(court_pending, court_moved, accused, |entry, gated| {
            session.palw_reporter_filing_read_v1(entry.offence_key, entry.commitment, entry.reporter, gated.cloned())
        });
        for (entry, end) in ended {
            info!(
                "[{PALW_PANEL}] claim {}: the filing of offence {} ({:?}) ends: {end:?} (P2-8)",
                entry.claim_id, entry.offence_key, entry.origin
            );
        }
        filer.persist();
    }

    /// **The capture arm's kind-4 filing at this node** ([`palw_capture_arm_filing_v1`] at the tip's
    /// fence and this node's bond).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn capture_arm_filing_v1(
        &self,
        duty: &PalwSeatDutyV2,
        receipt_deadline: u64,
        current_daa: u64,
        bond_key: PalwBondKeyV2,
        refutation: &PalwExecutionStepRefutationV1,
        openings: &[PalwArtifactOpeningV1],
        prompt_opening: &Option<PalwPromptIdsOpeningV1>,
    ) -> Option<PalwConvictionFilingV1> {
        palw_capture_arm_filing_v1(
            self.consensus_config.params.palw_rcore_plus_active_at(current_daa),
            bond_key,
            duty.executor_bond,
            duty.claim_id,
            receipt_deadline,
            current_daa,
            refutation,
            openings,
            prompt_opening,
        )
    }

    /// **SR-8 / J-4 (P2-8): what the capture arm files for a proven fault, once per claim** — the
    /// panel's half of [`palw_capture_arm_files_v1`]: the decision itself, over this node's door into
    /// the book (a fresh salt, the gate on the evidence, the bond's signature).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn capture_arm_files_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReporterFilerV1,
        accused: &mut HashSet<Hash64>,
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        duty: &PalwSeatDutyV2,
        leaf: u64,
        bond_key: PalwBondKeyV2,
        network_domain: &Hash64,
        refuted: Option<PalwConvictionFilingV1>,
        court: Option<(Hash64, PalwConsensusObjectV2)>,
    ) {
        palw_capture_arm_files_v1(filer, accused, court_pending, duty.claim_id, leaf, refuted, court, |filer, filing| {
            self.reporter_filer_file_v1(session, filer, filing, bond_key, network_domain)
        });
    }

    /// **J1 auto (P2-8; ADR-0152 §3.9 "borrowed", J-6):** a held capture of `duty`'s claim that does not
    /// reproduce the claim under its own job is probed ([`palw_borrowed_root_binding_v1`]) — once per
    /// (claim, capture), at most [`PALW_J1_PROBES_PER_TICK_V1`] a tick and within the claim's digest
    /// budget ([`PalwReporterFilerV1::take_probe`]), under the full seat's reservation, off the tick —
    /// and a binding it finds is filed as `ExecutorRefuted { IdentityMismatch }` through the filer,
    /// whose gate is the fold's identity check (root 0, forfeiture by claim, so the lender's rights
    /// stand). Past `Params::palw_rcore_plus` only; never against this node's own bond.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn j1_auto_probe_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        filer: &mut PalwReporterFilerV1,
        duty: &PalwSeatDutyV2,
        receipt_deadline: u64,
        current_daa: u64,
        bond_key: PalwBondKeyV2,
        network_domain: &Hash64,
        bytes: &[u8],
        roots: PalwClaimRootsV1,
    ) {
        if !self.consensus_config.params.palw_rcore_plus_active_at(current_daa) || duty.executor_bond == bond_key {
            return;
        }
        let Some(probe) = filer.take_probe(duty.claim_id, bytes, current_daa) else { return };
        let Ok(backend) = self.resolve_backend(session, duty.class_id, duty.artifact_root) else { return };
        let need = self.backends().role_memory_need_for_backend_or_chain_v1(
            backend.as_ref(),
            duty.class_id,
            duty.artifact_root,
            None,
            kaspa_consensus_core::palw_resource_profile_v1::PalwResourceRoleV1::FullSeat,
            |id| self.chain_carriage_v1(session, id),
        );
        let reserved = match self.reserve_replay_v1("j1-probe", &need, duty.class_id, duty.claim_id) {
            Ok(reserved) => reserved,
            Err(why) => {
                crate::palw_backends::note_throttled_v1("panel-j1-probe-ledger", || {
                    format!("[{PALW_PANEL}] claim {}: J1 auto's probe deferred: {why}", duty.claim_id)
                });
                filer.release_probe(probe);
                return;
            }
        };
        let owned = bytes.to_vec();
        let Ok((_backend, binding)) = offload(backend, move |b| {
            let _held_for_the_probe = reserved;
            palw_borrowed_root_binding_v1(b, &owned, roots)
        })
        .await
        else {
            return;
        };
        let Some(binding) = binding else { return };
        let object = palw_executor_refuted_object_v1(
            duty.executor_bond,
            duty.claim_id,
            PalwPanelContradictionV1::IdentityMismatch { binding },
            None,
        );
        let Some(filing) = PalwConvictionFilingV1::of_offence(
            object,
            duty.claim_id,
            palw_kind4_file_by_v1(receipt_deadline, current_daa),
            PalwFilingOriginV1::BorrowedRoot,
        ) else {
            return;
        };
        info!(
            "[{PALW_PANEL}] claim {}: a held capture reproduces its committed root under another job — filing IdentityMismatch (J1 auto, P2-8)",
            duty.claim_id
        );
        self.reporter_filer_file_v1(session, filer, filing, bond_key, network_domain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1};
    use kaspa_consensus_core::palw_state_v2::{PalwPayoutV2, PalwPendingRewardV1, PalwRewardWinnerV1};
    use kaspa_consensus_core::tx::TransactionId;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }

    const ME: u64 = 0x11;
    const EXECUTOR: u64 = 0xE0;
    const DOMAIN: u64 = 0xD0;

    /// A kind-4 filing on claim `claim` against the executor, its evidence a structural refutation
    /// shape (the filer never judges it: the gate the tests stand in for does).
    fn filing(claim: u64, file_by: Option<u64>) -> PalwConvictionFilingV1 {
        let object =
            palw_executor_refuted_object_v1(bond(EXECUTOR), h(claim), PalwPanelContradictionV1::CourtFraud { voided_daa: 7 }, None);
        PalwConvictionFilingV1::of_offence(object, h(claim), file_by, PalwFilingOriginV1::CaptureArm)
            .expect("kind 4 takes a commitment")
    }

    /// The chain at `now`: R-3 live, the reporter with room, nothing rooted or convicted, the gate
    /// admitting.
    fn chain(now: u64) -> PalwReporterFilingReadV1 {
        PalwReporterFilingReadV1 {
            now_daa: now,
            rcore_plus: true,
            window_court: 3_000,
            reporter_may_commit: true,
            committed_daa: None,
            consumed: None,
            pending: None,
            awarded: None,
            object_gate: Some(Ok(())),
        }
    }

    fn signer(message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        Some(blake2b_simd::Params::new().hash_length(64).key(context).hash(message).as_bytes().to_vec())
    }

    type Queue = Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>;

    /// One tick with no `court_moved` or `accused` to watch.
    fn tick_(
        book: &mut PalwReporterFilerV1,
        queue: &mut Queue,
        read: impl FnMut(&PalwFilerEntryV1, Option<&PalwConsensusObjectV2>) -> Option<PalwReporterFilingReadV1>,
    ) -> Vec<(PalwFilerEntryV1, PalwFilerEndV1)> {
        book.tick(queue, &mut HashMap::new(), &mut HashSet::new(), read)
    }

    fn filed(book: &mut PalwReporterFilerV1, filing: PalwConvictionFilingV1, read: &PalwReporterFilingReadV1) -> PalwFilerEntryV1 {
        let key = filing.offence_key;
        assert_eq!(book.file(filing, bond(ME), read, &h(DOMAIN), [7; 32], signer), PalwFileOutcomeV1::Queued { committed: true });
        book.entry(&key).expect("in the book").clone()
    }

    fn consumed(entry: &PalwFilerEntryV1, at: u64) -> PalwConsumedOffenceV1 {
        PalwConsumedOffenceV1 {
            kind: PalwOffenceKindV1::ExecutorRefuted,
            accused: entry.accused.0,
            amount: 100,
            accepted_daa: at,
            execution_root: Hash64::default(),
            collected: 100,
            claim_id: entry.claim_id,
        }
    }

    fn pending(entry: &PalwFilerEntryV1, consumed_at: u64, best: Option<PalwRewardWinnerV1>) -> PalwPendingRewardV1 {
        PalwPendingRewardV1 { amount: 10, reveal_until: consumed_at + 600, evidence_id: entry.evidence_id, best }
    }

    fn ours(entry: &PalwFilerEntryV1, committed_daa: u64) -> PalwRewardWinnerV1 {
        PalwRewardWinnerV1 { committed_daa, commitment: entry.commitment, reporter: entry.reporter, payload: h(0xFA) }
    }

    /// **T54c's node half, the whole order on the rule:** commit → (depth) → evidence → reveal →
    /// watched to the sweep → `Revealed`; each object re-sent only after the resend interval.
    #[test]
    fn t54c_the_filer_commits_waits_files_reveals_and_ends_revealed() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(1, None), &chain(100));
        assert_eq!(
            entry.commitment,
            kaspa_consensus_core::palw_state_v2::palw_reporter_commitment_v1(
                &entry.offence_key,
                &entry.evidence_id,
                &bond(ME),
                &[7; 32]
            )
        );
        assert!(
            matches!(entry.commit_object, Some(PalwConsensusObjectV2::ReporterCommitted { commitment, reporter, .. }) if commitment == entry.commitment && reporter == bond(ME))
        );
        assert_eq!(palw_filer_step_v1(&entry, &chain(100)), PalwFilerStepV1::Commit, "nothing rooted: commit");
        let sent = PalwFilerEntryV1 { commit_sent: Some(100), ..entry.clone() };
        assert_eq!(palw_filer_step_v1(&sent, &chain(105)), PalwFilerStepV1::Wait, "in flight");
        assert_eq!(palw_filer_step_v1(&sent, &chain(110)), PalwFilerStepV1::Commit, "lost: re-sent after the interval");
        let rooted = |now| PalwReporterFilingReadV1 { committed_daa: Some(111), ..chain(now) };
        assert_eq!(palw_filer_step_v1(&sent, &rooted(112)), PalwFilerStepV1::Wait, "one DAA deep is not deep enough");
        assert_eq!(palw_filer_step_v1(&sent, &rooted(113)), PalwFilerStepV1::File, "two deep: the evidence");
        let out = PalwFilerEntryV1 { evidence_sent: Some(113), ..sent };
        assert_eq!(palw_filer_step_v1(&out, &rooted(114)), PalwFilerStepV1::Wait);
        let convicted =
            PalwReporterFilingReadV1 { consumed: Some(consumed(&out, 115)), pending: Some(pending(&out, 115, None)), ..rooted(116) };
        assert_eq!(palw_filer_step_v1(&out, &convicted), PalwFilerStepV1::Reveal, "convicted on this evidence: reveal");
        let revealing = PalwFilerEntryV1 { reveal_sent: Some(116), ..out };
        assert_eq!(palw_filer_step_v1(&revealing, &convicted), PalwFilerStepV1::Wait);
        let leads =
            PalwReporterFilingReadV1 { pending: Some(pending(&revealing, 115, Some(ours(&revealing, 111)))), ..convicted.clone() };
        assert_eq!(palw_filer_step_v1(&revealing, &leads), PalwFilerStepV1::Wait, "ours leads: watched to the sweep");
        let swept =
            PalwReporterFilingReadV1 { pending: None, awarded: Some(PalwPayoutV2 { payload: h(0xFA), amount: 10 }), ..convicted };
        let seen = PalwFilerEntryV1 { revealed_seen: true, ..revealing.clone() };
        assert_eq!(palw_filer_step_v1(&seen, &swept), PalwFilerStepV1::Done(PalwFilerEndV1::Revealed));
        assert!(
            matches!(palw_filer_step_v1(&revealing, &swept), PalwFilerStepV1::Done(PalwFilerEndV1::Forgone(_))),
            "never seen leading"
        );
    }

    /// **T39's node half on the rule: the reporter is this node's bond, never the accused.** The book
    /// refuses a filing against its own bond before anything is built; a missing reveal forfeits R
    /// only (the window closes, the filing ends `Forgone` — the conviction was never the filer's to
    /// undo); a reveal past `reveal_until` is never sent.
    #[test]
    fn t39_the_reporter_is_this_bond_never_the_accused_and_a_missed_reveal_forfeits_r_only() {
        let mut book = PalwReporterFilerV1::default();
        let mine = PalwConvictionFilingV1 { accused: bond(ME), ..filing(2, None) };
        assert_eq!(book.file(mine, bond(ME), &chain(100), &h(DOMAIN), [1; 32], signer), PalwFileOutcomeV1::OwnBond);
        assert_eq!(book.len(), 0);
        let entry = filed(&mut book, filing(2, None), &chain(100));
        assert_eq!(entry.reporter, bond(ME));
        let late = PalwReporterFilingReadV1 {
            committed_daa: Some(101),
            consumed: Some(consumed(&entry, 105)),
            pending: Some(pending(&entry, 105, None)),
            ..chain(705 + 1)
        };
        assert_eq!(palw_filer_step_v1(&entry, &late), PalwFilerStepV1::Wait, "past reveal_until: nothing is sent");
        let closed = PalwReporterFilingReadV1 { pending: None, ..late };
        assert!(matches!(palw_filer_step_v1(&entry, &closed), PalwFilerStepV1::Done(PalwFilerEndV1::Forgone(_))));
        // Already on the book / already convicted.
        assert_eq!(book.file(filing(2, None), bond(ME), &chain(101), &h(DOMAIN), [2; 32], signer), PalwFileOutcomeV1::AlreadyFiled);
        let convicted = PalwReporterFilingReadV1 { consumed: Some(consumed(&entry, 90)), ..chain(101) };
        assert_eq!(book.file(filing(3, None), bond(ME), &convicted, &h(DOMAIN), [2; 32], signer), PalwFileOutcomeV1::AlreadyConvicted);
        let refused = PalwReporterFilingReadV1 { object_gate: Some(Err("ClaimUnderSession".into())), ..chain(101) };
        assert_eq!(
            book.file(filing(4, None), bond(ME), &refused, &h(DOMAIN), [2; 32], signer),
            PalwFileOutcomeV1::NotAdmitted("ClaimUnderSession".into()),
            "nothing is spent on evidence the gate refuses"
        );
    }

    /// **The commitment reorged out.** Before the conviction: its row is gone, so the SAME commitment
    /// (same salt) is sent again and the evidence waits for it. After the conviction nothing is sent
    /// that the fold would refuse — a row that re-rooted at or after the conviction (R-3's strict
    /// order), no row, an earlier commitment revealed, the key convicted on other evidence — and the
    /// filing waits for the sweep rather than ending, since a reorg inside the window can take any of
    /// those back (a row back below the conviction is revealed); the sweep ends it `Forgone`.
    #[test]
    fn a_commitment_reorged_out_is_re_sent_before_the_conviction_and_abandoned_after_it() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(5, None), &chain(100));
        let sent = PalwFilerEntryV1 { commit_sent: Some(100), evidence_sent: None, ..entry };
        // Rooted at 101, then reorged away before the evidence was sent.
        assert_eq!(
            palw_filer_step_v1(&sent, &PalwReporterFilingReadV1 { committed_daa: Some(101), ..chain(102) }),
            PalwFilerStepV1::Wait
        );
        assert_eq!(palw_filer_step_v1(&sent, &chain(110)), PalwFilerStepV1::Commit, "gone: the same commitment again");
        // Convicted at 120, the commitment re-rooted at 120 (the same merge set) — and at 125.
        let convicted = |committed| PalwReporterFilingReadV1 {
            committed_daa: committed,
            consumed: Some(consumed(&sent, 120)),
            pending: Some(pending(&sent, 120, None)),
            ..chain(126)
        };
        for at in [120, 125] {
            assert_eq!(
                palw_filer_step_v1(&sent, &convicted(Some(at))),
                PalwFilerStepV1::Wait,
                "a commitment at {at} does not precede a conviction at 120: no reveal is sent"
            );
        }
        assert_eq!(palw_filer_step_v1(&sent, &convicted(None)), PalwFilerStepV1::Wait, "absent: it may come back before the sweep");
        assert_eq!(palw_filer_step_v1(&sent, &convicted(Some(119))), PalwFilerStepV1::Reveal, "back below the conviction: reveal");
        // An earlier commitment revealed first — or the key convicted on other evidence.
        let rival = PalwRewardWinnerV1 { committed_daa: 118, commitment: h(1), reporter: bond(0x22), payload: h(2) };
        let beaten = PalwReporterFilingReadV1 { pending: Some(pending(&sent, 120, Some(rival))), ..convicted(Some(119)) };
        assert_eq!(palw_filer_step_v1(&sent, &beaten), PalwFilerStepV1::Wait);
        let later = PalwRewardWinnerV1 { committed_daa: 119, commitment: Hash64::from_bytes([0xFF; 64]), ..rival };
        let beating = PalwReporterFilingReadV1 { pending: Some(pending(&sent, 120, Some(later))), ..convicted(Some(119)) };
        assert_eq!(palw_filer_step_v1(&sent, &beating), PalwFilerStepV1::Reveal, "ours precedes the revealed best: reveal");
        let other = PalwReporterFilingReadV1 {
            pending: Some(PalwPendingRewardV1 { evidence_id: h(0xBAD), ..pending(&sent, 120, None) }),
            ..convicted(Some(119))
        };
        assert_eq!(palw_filer_step_v1(&sent, &other), PalwFilerStepV1::Wait);
        // The sweep ends every one of them.
        let swept = PalwReporterFilingReadV1 { pending: None, ..convicted(Some(125)) };
        assert!(matches!(palw_filer_step_v1(&sent, &swept), PalwFilerStepV1::Done(PalwFilerEndV1::Forgone(_))));
    }

    /// **The conviction never waits on R.** Past `file_by_daa` the evidence goes out with no
    /// commitment rooted; a bond that cannot root one (not at the floor, or its 64 slots full) files
    /// at once; below `palw_rcore_plus` the book commits nothing (tags 53/54 are refused by name
    /// there) and the filing ends when its conviction lands; and one that never convicts expires a
    /// court window after it was filed.
    #[test]
    fn the_conviction_never_waits_on_r_and_the_fence_off_twin_commits_nothing() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(6, Some(140)), &chain(100));
        let sent = PalwFilerEntryV1 { commit_sent: Some(135), ..entry };
        assert_eq!(palw_filer_step_v1(&sent, &chain(139)), PalwFilerStepV1::Wait);
        assert_eq!(palw_filer_step_v1(&sent, &chain(140)), PalwFilerStepV1::File, "past file_by: unprotected");
        let full = PalwReporterFilingReadV1 { reporter_may_commit: false, ..chain(101) };
        assert_eq!(palw_filer_step_v1(&sent, &full), PalwFilerStepV1::File, "no room to commit: file");
        assert!(matches!(palw_filer_step_v1(&sent, &chain(100 + 3_001)), PalwFilerStepV1::Done(PalwFilerEndV1::Expired)));

        // The fence-off twin: no commitment built, the evidence straight away, done at the conviction.
        let dormant = PalwReporterFilingReadV1 { rcore_plus: false, ..chain(100) };
        let key = filing(7, None).offence_key;
        assert_eq!(
            book.file(filing(7, None), bond(ME), &dormant, &h(DOMAIN), [3; 32], signer),
            PalwFileOutcomeV1::Queued { committed: false }
        );
        let direct = book.entry(&key).unwrap().clone();
        assert!(direct.commit_object.is_none());
        assert_eq!(palw_filer_step_v1(&direct, &dormant), PalwFilerStepV1::File);
        let landed = PalwReporterFilingReadV1 { consumed: Some(consumed(&direct, 101)), ..dormant };
        assert_eq!(palw_filer_step_v1(&direct, &landed), PalwFilerStepV1::Done(PalwFilerEndV1::FiledDirect));
        // Room, but no key: filed without a commitment, never refused for want of a signature.
        let key = filing(8, None).offence_key;
        assert_eq!(
            book.file(filing(8, None), bond(ME), &chain(100), &h(DOMAIN), [4; 32], |_, _| None),
            PalwFileOutcomeV1::Queued { committed: false }
        );
        assert!(book.entry(&key).unwrap().commit_object.is_none());
    }

    /// **Only commit–reveal filings take the filer** (`palw_filed_offence_commit_key_v1`): kind 4, kind 3
    /// on an execution-proving contradiction, and kind 0 — never a kind 3 restating a court's
    /// `CourtFraud`, never a named reward's object (`DefaultAccused`, `ShardCourtAccused`: P2-6's
    /// lanes, V3S-03), and never a fold-recorded kind.
    #[test]
    fn only_commit_reveal_filings_take_the_filer() {
        assert!(PalwConvictionFilingV1::of_offence(filing(9, None).object, h(9), None, PalwFilingOriginV1::Replay).is_some());
        let restated = kaspa_consensus_core::palw_offence_attribution_v1::PalwPanelFalseValidEvidenceV2 {
            version: kaspa_consensus_core::palw_offence_attribution_v1::PALW_PANEL_FALSE_VALID_VERSION_V2,
            claim_id: h(9),
            accused_seat: bond(0x33).0,
            receipt: kaspa_consensus_core::palw_offence_attribution_v1::PalwFalseValidReceiptV1::Full(
                kaspa_consensus_core::palw_panel_v2::PalwSeatReceiptV2 {
                    claim: h(9),
                    verdict: kaspa_consensus_core::palw_panel_v2::PalwReceiptVerdictV2::Valid,
                    seat_bond: bond(0x33),
                    signed_daa: 1,
                    signature: vec![1],
                },
            ),
            contradiction: PalwPanelContradictionV1::CourtFraud { voided_daa: 7 },
            prompt_ids_opening: None,
            reporter_reveal: Vec::new(),
        };
        let offence = |kind, evidence: Vec<u8>| PalwConsensusObjectV2::ObjectiveOffence {
            kind,
            accused: bond(0x33),
            evidence_id: kaspa_consensus_core::palw_offence_v1::palw_offence_evidence_digest_v1(&evidence),
            evidence,
        };
        let kind3 = offence(PalwOffenceKindV1::PanelFalseValidV2, borsh::to_vec(&restated).unwrap());
        assert!(PalwConvictionFilingV1::of_offence(kind3, h(9), None, PalwFilingOriginV1::Replay).is_none(), "a restated CourtFraud");
        for kind in [PalwOffenceKindV1::DaDefault, PalwOffenceKindV1::CourtConviction, PalwOffenceKindV1::PanelFalseValid] {
            assert!(
                PalwConvictionFilingV1::of_offence(offence(kind, vec![1]), h(9), None, PalwFilingOriginV1::Replay).is_none(),
                "{kind:?}"
            );
        }
        let named =
            PalwConsensusObjectV2::DefaultAccused { claim: h(9), missing_event_index: 0, accuser: bond(ME), signature: vec![1] };
        assert!(PalwConvictionFilingV1::of_offence(named, h(9), None, PalwFilingOriginV1::Replay).is_none());
    }

    /// **A restart with a reveal pending reveals.** The book is written, read back into a fresh
    /// filer, and the same salt and commitment answer the chain's conviction with a reveal; the
    /// tick queues exactly that reveal on the court queue, once, and a finished filing leaves both the
    /// book and the queue.
    #[test]
    fn a_restart_with_a_reveal_pending_still_reveals() {
        let dir = tempfile::tempdir().expect("a temporary state dir");
        let mut book = PalwReporterFilerV1::load(dir.path());
        let entry = filed(&mut book, filing(10, None), &chain(100));
        book.entries.get_mut(&entry.offence_key).unwrap().evidence_sent = Some(104);
        book.dirty = true;
        book.persist();
        drop(book);

        let mut restarted = PalwReporterFilerV1::load(dir.path());
        let back = restarted.entry(&entry.offence_key).expect("the book survives the restart").clone();
        assert_eq!((back.salt, back.commitment, back.evidence_sent), (entry.salt, entry.commitment, Some(104)));
        let convicted = PalwReporterFilingReadV1 {
            committed_daa: Some(101),
            consumed: Some(consumed(&back, 105)),
            pending: Some(pending(&back, 105, None)),
            ..chain(106)
        };
        let mut queue = Vec::new();
        let ended = tick_(&mut restarted, &mut queue, |_, _| Some(convicted.clone()));
        assert!(ended.is_empty());
        let reveal = PalwConsensusObjectV2::ReporterRevealed { offence_key: back.offence_key, reporter: bond(ME), salt: back.salt };
        assert_eq!(queue, vec![(back.offence_key, PALW_FILER_ROUND_REVEAL_V1, false, reveal)]);
        tick_(&mut restarted, &mut queue, |_, _| Some(convicted.clone()));
        assert_eq!(queue.len(), 1, "queued once");
        let swept = PalwReporterFilingReadV1 { pending: None, ..convicted };
        let ended = tick_(&mut restarted, &mut queue, |_, _| Some(swept.clone()));
        assert_eq!(ended.len(), 1);
        assert!(queue.is_empty() && restarted.len() == 0, "the finished filing leaves the book and the queue");
        restarted.persist();
        assert_eq!(PalwReporterFilerV1::load(dir.path()).len(), 0, "and the book on disk");
    }

    /// **The tick asks the gate before every send**: a refused evidence is neither queued nor marked
    /// sent; an admitted one is queued under the evidence round. **And a refusal is asked about again
    /// a re-plan later, not on every tick** (the integration's review, F6): the panel ticks every two
    /// seconds, and each ask is the processor's whole gate on the evidence.
    #[test]
    fn the_tick_asks_the_gate_before_it_spends_and_asks_a_refusal_again_a_replan_later() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(11, None), &chain(100));
        let rooted = |now| PalwReporterFilingReadV1 { committed_daa: Some(101), object_gate: None, ..chain(now) };
        let mut queue = Vec::new();
        let asked = std::rc::Rc::new(std::cell::Cell::new(0));
        let gate = |now: u64, admit: bool| {
            let (asked, evidence, rooted) = (asked.clone(), entry.object.clone(), rooted(now));
            move |_: &PalwFilerEntryV1, gated: Option<&PalwConsensusObjectV2>| {
                assert!(gated.is_none_or(|object| *object == evidence), "only the evidence is asked about");
                asked.set(asked.get() + usize::from(gated.is_some()));
                let verdict = if admit { Ok(()) } else { Err("ClaimUnderSession".to_string()) };
                Some(PalwReporterFilingReadV1 { object_gate: gated.map(|_| verdict.clone()), ..rooted.clone() })
            }
        };
        tick_(&mut book, &mut queue, gate(103, false));
        assert!(queue.is_empty() && book.entry(&entry.offence_key).unwrap().evidence_sent.is_none(), "refused: nothing spent");
        assert_eq!((asked.get(), book.entry(&entry.offence_key).unwrap().gate_refused_at), (1, Some(103)));
        // Every tick of the next re-plan: not asked again (the gate would admit it by now).
        for now in [103, 104, 103 + PALW_FILER_GATE_RETRY_DAA_V1 - 1] {
            tick_(&mut book, &mut queue, gate(now, true));
        }
        assert!(queue.is_empty() && asked.get() == 1, "one gate run a re-plan, not one a tick");
        tick_(&mut book, &mut queue, gate(103 + PALW_FILER_GATE_RETRY_DAA_V1, true));
        assert_eq!(asked.get(), 2);
        assert_eq!(queue.len(), 1);
        assert_eq!((queue[0].0, queue[0].1), (entry.offence_key, PALW_FILER_ROUND_EVIDENCE_V1));
        assert_eq!(queue[0].3, entry.object);
        let live = book.entry(&entry.offence_key).unwrap();
        assert_eq!((live.evidence_sent, live.gate_refused_at), (Some(103 + PALW_FILER_GATE_RETRY_DAA_V1), None));
    }

    /// J1 auto takes one probe a tick and one per (claim, capture): a stranger's capture served first
    /// does not spend the claim's probe on the borrowed one; a released probe is taken again.
    #[test]
    fn j1_auto_probes_once_per_capture_and_once_a_tick() {
        let mut book = PalwReporterFilerV1::default();
        tick_(&mut book, &mut Vec::new(), |_, _| None);
        let garbage = book.take_probe(h(1), b"garbage", 100).expect("the tick's probe");
        assert!(book.take_probe(h(1), b"borrowed", 100).is_none(), "one a tick");
        tick_(&mut book, &mut Vec::new(), |_, _| None);
        assert!(book.take_probe(h(1), b"garbage", 100).is_none(), "once per capture");
        let borrowed = book.take_probe(h(1), b"borrowed", 100).expect("another capture of the same claim");
        tick_(&mut book, &mut Vec::new(), |_, _| None);
        book.release_probe(borrowed);
        assert_eq!(book.take_probe(h(1), b"borrowed", 100), Some(borrowed), "released: taken again");
        assert_ne!(garbage, borrowed);
    }

    /// **One offence key, whichever lane files first** (the integration of P2-8 with P2-8b/8c). A
    /// replay's kind 4 is in the book; the capture arm's kind 4 of the same claim is `AlreadyFiled`
    /// (one filing an offence), and its court accusation is ADOPTED as the replay filing's fallback —
    /// so when that filing stalls, the court still carries the proven fault if the gate admits it, and
    /// otherwise the claim leaves `accused` for the named-leaf pursuit (F3's guarantee across lanes).
    /// A fallback already set is never replaced, and a P2-8c filing is keyed per seat, apart.
    #[test]
    fn a_second_lanes_filing_of_one_offence_is_already_filed_and_its_fallback_is_adopted() {
        let court_key = h(0xC0_2B);
        let accusation =
            PalwConsensusObjectV2::DefaultAccused { claim: h(24), missing_event_index: 0, accuser: bond(ME), signature: vec![2] };
        let rooted = PalwReporterFilingReadV1 { committed_daa: Some(101), ..chain(160) };
        for admitted in [true, false] {
            let mut book = PalwReporterFilerV1::default();
            let replay = PalwConvictionFilingV1 { origin: PalwFilingOriginV1::Replay, ..filing(24, None) };
            let entry = filed(&mut book, replay, &chain(100));
            let capture_arm = filing(24, None).with_fallback(Some((court_key, accusation.clone())));
            assert_eq!(capture_arm.offence_key, entry.offence_key, "one kind-4 key a claim");
            assert_eq!(
                book.file(capture_arm.clone(), bond(ME), &chain(100), &h(DOMAIN), [8; 32], signer),
                PalwFileOutcomeV1::AlreadyFiled
            );
            assert!(book.back_by_capture_arm_v1(&entry.offence_key, capture_arm.fallback.clone()));
            assert!(!book.back_by_capture_arm_v1(&entry.offence_key, Some((h(1), accusation.clone()))), "never replaced");
            assert_eq!(book.len(), 1);
            let live = book.entries.get_mut(&entry.offence_key).unwrap();
            (live.evidence_sent, live.evidence_sends) = (Some(150), PALW_FILER_MAX_SENDS_V1);
            let (mut queue, mut accused) = (Vec::new(), HashSet::from([entry.claim_id]));
            let ended = book.tick(&mut queue, &mut HashMap::new(), &mut accused, |_, gated| {
                let object_gate = gated.map(|_| if admitted { Ok(()) } else { Err("the claim is final".to_string()) });
                Some(PalwReporterFilingReadV1 { object_gate, ..rooted.clone() })
            });
            assert_eq!(
                ended.iter().map(|(entry, end)| (entry.origin, *end)).collect::<Vec<_>>(),
                vec![(PalwFilingOriginV1::Replay, PalwFilerEndV1::Stalled)]
            );
            if admitted {
                assert_eq!(queue, vec![(court_key, 0, false, accusation.clone())], "the court carries the fault");
                assert!(accused.contains(&entry.claim_id));
            } else {
                assert!(queue.is_empty() && !accused.contains(&entry.claim_id), "refused: the pursuit may file");
            }
        }
        // A replay filing no capture arm backed hands nothing back to `accused`.
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, PalwConvictionFilingV1 { origin: PalwFilingOriginV1::Replay, ..filing(25, None) }, &chain(100));
        book.entries.get_mut(&entry.offence_key).unwrap().evidence_sends = PALW_FILER_MAX_SENDS_V1;
        let mut accused = HashSet::from([entry.claim_id]);
        book.tick(&mut Vec::new(), &mut HashMap::new(), &mut accused, |_, _| Some(rooted.clone()));
        assert!(accused.contains(&entry.claim_id) && book.len() == 0, "its lane re-hands it; the capture arm's set is untouched");
    }

    /// Only this filer's objects on its rounds are its queue entries — never a court move, a P2-6
    /// accusation, or a P2-7 answer whose folded round happens to land on one of its rounds.
    #[test]
    fn the_filers_queue_entries_are_its_own() {
        let reveal = PalwConsensusObjectV2::ReporterRevealed { offence_key: h(1), reporter: bond(ME), salt: [0; 32] };
        assert!(palw_filer_queued_v1(PALW_FILER_ROUND_REVEAL_V1, false, &reveal));
        assert!(!palw_filer_queued_v1(PALW_FILER_ROUND_REVEAL_V1, true, &reveal), "an answer's side");
        assert!(!palw_filer_queued_v1(7, false, &reveal), "a court round");
        let accused =
            PalwConsensusObjectV2::DefaultAccused { claim: h(1), missing_event_index: 0, accuser: bond(ME), signature: vec![1] };
        assert!(!palw_filer_queued_v1(PALW_FILER_ROUND_COMMIT_V1, false, &accused), "another lane's object");
        let mut queue = vec![(h(9), PALW_FILER_ROUND_EVIDENCE_V1, false, accused.clone()), (h(9), 3, false, reveal.clone())];
        tick_(&mut PalwReporterFilerV1::default(), &mut queue, |_, _| None);
        assert_eq!(queue.len(), 2, "the tick drops only its own finished filings' objects");
    }

    /// The floor, as base0's `seat0_review_regressions` resolves it (the panel tests' fixture).
    fn floor_backend() -> misaka_palw_base0::backend::Base0Backend {
        use misaka_palw_base0::classes::{canonical_class_by_model_id_v1, resolve_class_v1};
        let court =
            kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2::new(kaspa_consensus_core::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
                .expect("court");
        let entry = canonical_class_by_model_id_v1(&court, "PALW-BASE-0/rc").expect("floor");
        let root = misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("root");
        misaka_palw_base0::backend::Base0Backend::new(resolve_class_v1(&court, entry.class_id(), root, &[]).expect("resolves"))
            .with_step_ladder_cap(court.max_step_leaf_count())
            .with_prompt_ids_form(kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1)
    }

    /// **J1 auto's detector on the floor** (the borrowed strategy, ADR §3.9): the lender's capture
    /// held for a BORROWER's claim — the lender's roots, the borrower's own anchor — reproduces the
    /// committed roots with no job bound and not under the borrower's job, and yields the binding the
    /// lender's run committed (its job is the lender's anchor, which is what the fold's J1 names).
    /// The lender's own claim, and a stranger's bytes, yield nothing.
    #[test]
    fn j1_auto_finds_a_borrowed_root_and_nothing_else() {
        let backend = floor_backend();
        let lender_anchor = h(0x00C0_FFEE);
        let (job, prompt) = backend.job_for_anchor(lender_anchor).expect("job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
        let run = backend.execute(&job, &prompt).expect("the lender's run");
        let lender = PalwClaimRootsV1 {
            execution_root: run.execution_root,
            trace_root: run.trace_root,
            anchor: lender_anchor,
            attempt_draw: Some(true),
            output_root: Some(run.output_root),
            job_pin: None,
        };
        assert!(palw_borrowed_root_binding_v1(&backend, &run.material, lender).is_none(), "the lender's own claim");
        let borrower = PalwClaimRootsV1 { anchor: h(0xB0_2202), ..lender };
        let binding = palw_borrowed_root_binding_v1(&backend, &run.material, borrower).expect("the borrowed root is found");
        assert_eq!(binding.committed_execution_root, run.execution_root, "the claim's committed execution");
        assert_eq!(binding.job_context.job_id, lender_anchor, "answering the lender's job");
        let stranger = PalwClaimRootsV1 { execution_root: h(0xBAD), ..borrower };
        assert!(palw_borrowed_root_binding_v1(&backend, &run.material, stranger).is_none(), "another execution is nothing");
    }

    /// **The send cap (the review of P2-8, F4), on the rule.** Each object goes out at most
    /// `PALW_FILER_MAX_SENDS_V1` times, the last copy given its interval to land: a commitment that
    /// never rooted is given up and the evidence goes out without it; evidence that never convicted
    /// ends the filing `Stalled`; a reveal that never led is not sent again. One copy fewer, and each
    /// is sent as before.
    #[test]
    fn every_object_is_sent_a_bounded_number_of_times() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(20, None), &chain(100));
        let max = PALW_FILER_MAX_SENDS_V1;
        // The commitment: sent `max` times, never rooted.
        let committing = |sends| PalwFilerEntryV1 { commit_sent: Some(130), commit_sends: sends, ..entry.clone() };
        assert_eq!(palw_filer_step_v1(&committing(max - 1), &chain(140)), PalwFilerStepV1::Commit, "one more copy");
        assert_eq!(palw_filer_step_v1(&committing(max), &chain(135)), PalwFilerStepV1::Wait, "the last copy is still landing");
        assert_eq!(palw_filer_step_v1(&committing(max), &chain(140)), PalwFilerStepV1::File, "given up: the evidence alone");
        // The evidence: sent `max` times, never convicted.
        let rooted = |now| PalwReporterFilingReadV1 { committed_daa: Some(101), ..chain(now) };
        let filing_out = |sends| PalwFilerEntryV1 { evidence_sent: Some(150), evidence_sends: sends, ..entry.clone() };
        assert_eq!(palw_filer_step_v1(&filing_out(max - 1), &rooted(160)), PalwFilerStepV1::File, "one more copy");
        assert_eq!(palw_filer_step_v1(&filing_out(max), &rooted(155)), PalwFilerStepV1::Wait, "the last copy is still landing");
        assert_eq!(palw_filer_step_v1(&filing_out(max), &rooted(160)), PalwFilerStepV1::Done(PalwFilerEndV1::Stalled));
        // The reveal: sent `max` times, never leading.
        let convicted = PalwReporterFilingReadV1 {
            consumed: Some(consumed(&entry, 170)),
            pending: Some(pending(&entry, 170, None)),
            ..rooted(200)
        };
        let revealing = |sends| PalwFilerEntryV1 { reveal_sent: Some(180), reveal_sends: sends, ..entry.clone() };
        assert_eq!(palw_filer_step_v1(&revealing(max - 1), &convicted), PalwFilerStepV1::Reveal, "one more copy");
        assert_eq!(palw_filer_step_v1(&revealing(max), &convicted), PalwFilerStepV1::Wait, "no more: the sweep ends it");
    }

    /// **The tick counts each copy, and gives up a commitment the gate refuses** (F4): a refused
    /// commitment (a signer that is not the bond's registered key) is never queued; the filing drops
    /// it and sends the evidence in the same tick, which then ends `FiledDirect`. An admitted one is
    /// queued and counted; a copy still in the queue is neither re-queued nor counted.
    #[test]
    fn the_tick_counts_copies_and_gives_up_a_commitment_the_gate_refuses() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(21, None), &chain(100));
        let commit = entry.commit_object.clone().expect("committing");
        let gate = |now: u64| {
            let (evidence, commit) = (entry.object.clone(), commit.clone());
            move |_: &PalwFilerEntryV1, gated: Option<&PalwConsensusObjectV2>| {
                let object_gate = gated.map(|object| {
                    assert!(*object == evidence || *object == commit, "only the filing's own objects are asked about");
                    Ok(())
                });
                Some(PalwReporterFilingReadV1 { object_gate, ..chain(now) })
            }
        };
        let mut queue = Vec::new();
        tick_(&mut book, &mut queue, gate(100));
        assert_eq!(queue.iter().map(|(_, round, _, _)| *round).collect::<Vec<_>>(), vec![PALW_FILER_ROUND_COMMIT_V1]);
        assert_eq!(book.entry(&entry.offence_key).unwrap().commit_sends, 1);
        tick_(&mut book, &mut queue, gate(100 + PALW_FILER_RESEND_DAA_V1));
        assert_eq!((queue.len(), book.entry(&entry.offence_key).unwrap().commit_sends), (1, 1), "still queued: not again");
        queue.clear();
        tick_(&mut book, &mut queue, gate(100 + PALW_FILER_RESEND_DAA_V1));
        assert_eq!((queue.len(), book.entry(&entry.offence_key).unwrap().commit_sends), (1, 2), "gone from the queue: a copy");

        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(22, None), &chain(100));
        let mut queue = Vec::new();
        let commit = entry.commit_object.clone().unwrap();
        tick_(&mut book, &mut queue, |_, gated| {
            let object_gate = gated.map(|object| if *object == commit { Err("not signed".to_string()) } else { Ok(()) });
            Some(PalwReporterFilingReadV1 { object_gate, ..chain(100) })
        });
        let live = book.entry(&entry.offence_key).unwrap().clone();
        assert!(live.commit_object.is_none() && live.commit_sends == 0, "given up, never sent");
        assert_eq!(
            queue,
            vec![(entry.offence_key, PALW_FILER_ROUND_EVIDENCE_V1, false, entry.object.clone())],
            "the evidence instead"
        );
        assert_eq!(live.evidence_sends, 1);
        let landed = PalwReporterFilingReadV1 { consumed: Some(consumed(&live, 101)), ..chain(102) };
        let ended = tick_(&mut book, &mut queue, |_, _| Some(landed.clone()));
        assert_eq!(ended.iter().map(|(_, end)| *end).collect::<Vec<_>>(), vec![PalwFilerEndV1::FiledDirect]);
    }

    /// **A filing that does not convict is handed back** (the review of P2-8, F3), and every finished
    /// filing takes its debounce keys out of `court_moved` (F5). The capture arm's filing that stalls
    /// queues its court fallback when the gate admits it (the claim stays `accused`: the court now
    /// carries it); when the gate refuses it, or there is none, the claim leaves `accused` so the
    /// named-leaf pursuit may file. A J1 filing that expired releases its claim's probes; one that
    /// stalled keeps them. A conviction hands nothing back.
    #[test]
    fn a_filing_that_does_not_convict_is_handed_back_and_every_end_prunes_court_moved() {
        let court_key = h(0xC0_2A);
        // A stand-in for the one-move accusation: the hand-back queues whatever fallback it was given.
        let accusation =
            PalwConsensusObjectV2::DefaultAccused { claim: h(23), missing_event_index: 0, accuser: bond(ME), signature: vec![1] };
        let stalled = |origin, fallback: Option<(Hash64, PalwConsensusObjectV2)>| {
            let mut book = PalwReporterFilerV1::default();
            let filing = PalwConvictionFilingV1 { origin, ..filing(23, None) }.with_fallback(fallback);
            let entry = filed(&mut book, filing, &chain(100));
            let live = book.entries.get_mut(&entry.offence_key).unwrap();
            (live.evidence_sent, live.evidence_sends) = (Some(150), PALW_FILER_MAX_SENDS_V1);
            (book, entry)
        };
        let moved = |key: Hash64| {
            let mut moved: HashMap<(Hash64, u32, bool), u64> = [PALW_FILER_ROUND_COMMIT_V1, PALW_FILER_ROUND_EVIDENCE_V1]
                .into_iter()
                .map(|round| ((key, round, false), 140))
                .collect();
            moved.insert((court_key, 7, false), 140);
            moved
        };
        let rooted = PalwReporterFilingReadV1 { committed_daa: Some(101), ..chain(160) };
        for admitted in [true, false] {
            let (mut book, entry) = stalled(PalwFilingOriginV1::CaptureArm, Some((court_key, accusation.clone())));
            assert_eq!(entry.fallback, Some((court_key, accusation.clone())), "the book keeps the fallback it was given");
            let (mut queue, mut court_moved, mut accused) = (Vec::new(), moved(entry.offence_key), HashSet::from([entry.claim_id]));
            let ended = book.tick(&mut queue, &mut court_moved, &mut accused, |_, gated| {
                let object_gate = gated.map(|object| {
                    assert_eq!(*object, accusation, "only the fallback is asked about");
                    if admitted { Ok(()) } else { Err("the claim is final".to_string()) }
                });
                Some(PalwReporterFilingReadV1 { object_gate, ..rooted.clone() })
            });
            assert_eq!(ended.iter().map(|(_, end)| *end).collect::<Vec<_>>(), vec![PalwFilerEndV1::Stalled]);
            assert_eq!(court_moved.keys().copied().collect::<Vec<_>>(), vec![(court_key, 7, false)], "only its own keys pruned");
            if admitted {
                assert_eq!(queue, vec![(court_key, 0, false, accusation.clone())], "the court carries the fault now");
                assert!(accused.contains(&entry.claim_id));
            } else {
                assert!(queue.is_empty() && !accused.contains(&entry.claim_id), "refused: the pursuit may file");
            }
        }
        let (mut book, entry) = stalled(PalwFilingOriginV1::CaptureArm, None);
        let mut accused = HashSet::from([entry.claim_id]);
        book.tick(&mut Vec::new(), &mut HashMap::new(), &mut accused, |_, _| Some(rooted.clone()));
        assert!(!accused.contains(&entry.claim_id), "no fallback: the pursuit may file");

        // J1: expired releases the claim's probes, stalled keeps them.
        for (end, now) in [(PalwFilerEndV1::Expired, 100 + 3_001), (PalwFilerEndV1::Stalled, 160)] {
            let (mut book, entry) = stalled(PalwFilingOriginV1::BorrowedRoot, None);
            tick_(&mut book, &mut Vec::new(), |_, _| None);
            let probe = book.take_probe(entry.claim_id, b"borrowed", now).expect("probed once");
            let at = PalwReporterFilingReadV1 { now_daa: now, ..rooted.clone() };
            let ended = tick_(&mut book, &mut Vec::new(), |_, _| Some(at.clone()));
            assert_eq!(ended.iter().map(|(_, end)| *end).collect::<Vec<_>>(), vec![end]);
            let again = book.take_probe(entry.claim_id, b"borrowed", now);
            assert_eq!(again, (end == PalwFilerEndV1::Expired).then_some(probe), "{end:?}");
        }

        // A conviction hands nothing back, and still prunes its keys.
        let (mut book, entry) = stalled(PalwFilingOriginV1::CaptureArm, Some((court_key, accusation.clone())));
        let (mut queue, mut court_moved, mut accused) = (Vec::new(), moved(entry.offence_key), HashSet::from([entry.claim_id]));
        let swept = PalwReporterFilingReadV1 { consumed: Some(consumed(&entry, 150)), ..rooted.clone() };
        let ended = book.tick(&mut queue, &mut court_moved, &mut accused, |_, _| Some(swept.clone()));
        assert!(matches!(ended[..], [(_, PalwFilerEndV1::Forgone(_))]));
        assert!(queue.is_empty() && accused.contains(&entry.claim_id));
        assert_eq!(court_moved.len(), 1);
    }

    /// **Every object the filer queues states the DAA it is due by, and the queue's order is not the
    /// filer's** (the integration, for the priority lane's earliest-deadline-first order). The
    /// commitment is due the commit depth before the evidence's date, the evidence by its
    /// `file_by_daa` (or, with none, its court window's end), the reveal by R-3's `reveal_until`, a
    /// stalled filing's fallback by the filing's `file_by_daa`; each is readable by its queue key and
    /// forgotten once it left the queue. A copy the queue held back past the resend interval is not
    /// re-queued the tick after it is finally carried: its interval runs from the carry
    /// (`court_moved`), not from the queuing.
    #[test]
    fn every_queued_object_states_its_due_daa_and_the_queue_order_is_not_the_filers() {
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(30, Some(640)), &chain(100));
        let key = entry.offence_key;
        let (commit, evidence, reveal) = (
            (key, PALW_FILER_ROUND_COMMIT_V1, false),
            (key, PALW_FILER_ROUND_EVIDENCE_V1, false),
            (key, PALW_FILER_ROUND_REVEAL_V1, false),
        );
        let mut queue = Vec::new();
        let mut court_moved = HashMap::new();
        let tick =
            |book: &mut PalwReporterFilerV1, queue: &mut Queue, court_moved: &mut HashMap<_, _>, read: PalwReporterFilingReadV1| {
                book.tick(queue, court_moved, &mut HashSet::new(), |_, gated| {
                    Some(PalwReporterFilingReadV1 { object_gate: gated.map(|_| Ok(())), ..read.clone() })
                })
            };
        tick(&mut book, &mut queue, &mut court_moved, chain(100));
        assert_eq!(queue.len(), 1);
        assert_eq!(book.queued_due_v1(&commit), Some(640 - PALW_FILER_COMMIT_DEPTH_DAA_V1), "rooted deep enough by file_by");
        // A deadline-ordered queue holds the commitment back past its resend interval: not queued twice.
        tick(&mut book, &mut queue, &mut court_moved, chain(100 + PALW_FILER_RESEND_DAA_V1 + 5));
        assert_eq!((queue.len(), book.entry(&key).unwrap().commit_sends), (1, 1));
        // Carried at last, at 125: the next copy waits a full interval from the carry.
        queue.clear();
        court_moved.insert(commit, 125);
        tick(&mut book, &mut queue, &mut court_moved, chain(126));
        assert!(queue.is_empty(), "carried a DAA ago: still landing");
        assert_eq!(book.entry(&key).unwrap().commit_sent, Some(125));
        assert_eq!(book.queued_due_v1(&commit), None, "a date only while queued");
        tick(&mut book, &mut queue, &mut court_moved, chain(125 + PALW_FILER_RESEND_DAA_V1));
        assert_eq!((queue.len(), book.entry(&key).unwrap().commit_sends), (1, 2), "lost: the next copy, an interval after the carry");
        queue.clear();
        // Rooted: the evidence, due by file_by — and, pluggable, ahead of an undated item.
        let rooted = |now| PalwReporterFilingReadV1 { committed_daa: Some(136), ..chain(now) };
        tick(&mut book, &mut queue, &mut court_moved, rooted(138));
        assert_eq!(book.queued_due_v1(&evidence), Some(640));
        let undated = (h(0xAA), 7, false, entry.object.clone());
        queue.insert(0, undated.clone());
        queue.sort_by_key(|(k, r, responder, _)| book.queued_due_v1(&(*k, *r, *responder)).map_or((1, 0), |due| (0, due)));
        assert_eq!((queue[0].0, queue[0].1, &queue[1]), (key, PALW_FILER_ROUND_EVIDENCE_V1, &undated), "dated first");
        queue.clear();
        // Convicted: the reveal, due by R-3's reveal window's end.
        let convicted = PalwReporterFilingReadV1 {
            consumed: Some(consumed(&entry, 150)),
            pending: Some(pending(&entry, 150, None)),
            ..rooted(151)
        };
        tick(&mut book, &mut queue, &mut court_moved, convicted.clone());
        assert_eq!(book.queued_due_v1(&reveal), convicted.pending.map(|p| p.reveal_until));
        assert_eq!(book.queued_dues_v1().collect::<Vec<_>>(), vec![(reveal, 150 + 600)]);

        // No shorter deadline (P2-8c's kind 3): the court window's end, the commitment before it.
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(31, None), &chain(100));
        let mut queue = Vec::new();
        tick(&mut book, &mut queue, &mut HashMap::new(), chain(100));
        assert_eq!(
            book.queued_due_v1(&(entry.offence_key, PALW_FILER_ROUND_COMMIT_V1, false)),
            Some(100 + 3_000 - PALW_FILER_COMMIT_DEPTH_DAA_V1)
        );
        let step = |step| palw_filer_step_due_v1(&entry, step, &chain(100));
        assert_eq!(
            (step(PalwFilerStepV1::File), step(PalwFilerStepV1::Reveal), step(PalwFilerStepV1::Wait)),
            (Some(3_100), None, None)
        );

        // A stalled filing's fallback is due by the filing's own date.
        let court_key = h(0xC0_2C);
        let accusation =
            PalwConsensusObjectV2::DefaultAccused { claim: h(32), missing_event_index: 0, accuser: bond(ME), signature: vec![3] };
        let mut book = PalwReporterFilerV1::default();
        let entry = filed(&mut book, filing(32, Some(640)).with_fallback(Some((court_key, accusation))), &chain(100));
        let live = book.entries.get_mut(&entry.offence_key).unwrap();
        (live.evidence_sent, live.evidence_sends) = (Some(150), PALW_FILER_MAX_SENDS_V1);
        let mut queue = Vec::new();
        tick(&mut book, &mut queue, &mut HashMap::new(), rooted(160));
        assert_eq!(queue.len(), 1, "the fallback");
        assert_eq!(book.queued_due_v1(&(court_key, 0, false)), Some(640));
    }

    /// One tick at `read`, the gate admitting everything it is asked about.
    fn tick_admitting(
        book: &mut PalwReporterFilerV1,
        queue: &mut Queue,
        court_moved: &mut HashMap<(Hash64, u32, bool), u64>,
        accused: &mut HashSet<Hash64>,
        read: PalwReporterFilingReadV1,
    ) -> Vec<(PalwFilerEntryV1, PalwFilerEndV1)> {
        book.tick(queue, court_moved, accused, |_, gated| {
            Some(PalwReporterFilingReadV1 { object_gate: gated.map(|_| Ok(())), ..read.clone() })
        })
    }

    fn rounds(queue: &Queue) -> Vec<u32> {
        queue.iter().map(|(_, round, _, _)| *round).collect()
    }

    /// **A last copy the queue holds back is not the end of its object** (the integration's review,
    /// F1: the cap counted a copy from its QUEUING, so a deadline-ordered queue that held the third
    /// copy back one resend interval ended the filing `Stalled` and pruned the copy unsent, or gave the
    /// commitment up and queued the evidence beside the unsent commitment — one block could then carry
    /// both, and R was lost). The evidence's third copy, held back five intervals: the filing neither
    /// stalls nor re-queues it, and ends `Stalled` only a full interval after the copy was CARRIED.
    /// The commitment's third copy, held back as long: never given up, no evidence beside it, and
    /// given up — the evidence alone — only a full interval after its carry.
    #[test]
    fn a_last_copy_the_queue_holds_back_neither_stalls_the_filing_nor_gives_its_commitment_up() {
        let (max, resend) = (PALW_FILER_MAX_SENDS_V1, PALW_FILER_RESEND_DAA_V1);
        let rooted = |now| PalwReporterFilingReadV1 { committed_daa: Some(101), ..chain(now) };
        // The rule itself: a queued last copy waits, whatever the clock says.
        let entry = filed(&mut PalwReporterFilerV1::default(), filing(40, None), &chain(100));
        let held = |in_queue| PalwFilerEntryV1 { evidence_sent: Some(150), evidence_sends: max, in_queue, ..entry.clone() };
        let evidence_queued = PalwFilerInQueueV1 { evidence: true, ..Default::default() };
        assert_eq!(palw_filer_step_v1(&held(evidence_queued), &rooted(2_000)), PalwFilerStepV1::Wait);
        assert_eq!(palw_filer_step_v1(&held(Default::default()), &rooted(2_000)), PalwFilerStepV1::Done(PalwFilerEndV1::Stalled));

        // The evidence, on the tick.
        let mut book = PalwReporterFilerV1::default();
        let key = filed(&mut book, filing(40, None), &chain(100)).offence_key;
        let live = book.entries.get_mut(&key).unwrap();
        (live.evidence_sent, live.evidence_sends) = (Some(150), max - 1);
        let (mut queue, mut moved, mut accused) = (Vec::new(), HashMap::new(), HashSet::new());
        assert!(tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, rooted(200)).is_empty());
        assert_eq!((rounds(&queue), book.entry(&key).unwrap().evidence_sends), (vec![PALW_FILER_ROUND_EVIDENCE_V1], max));
        for now in [200 + resend, 200 + 5 * resend] {
            assert!(tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, rooted(now)).is_empty(), "held at {now}");
            assert_eq!((queue.len(), book.entry(&key).unwrap().evidence_sends), (1, max), "neither pruned nor queued twice");
        }
        let carried = 200 + 5 * resend + 1;
        queue.clear();
        moved.insert((key, PALW_FILER_ROUND_EVIDENCE_V1, false), carried);
        assert!(tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, rooted(carried + resend - 1)).is_empty());
        assert_eq!(book.entry(&key).unwrap().evidence_sent, Some(carried), "its interval runs from the carry");
        let ended = tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, rooted(carried + resend));
        assert_eq!(ended.iter().map(|(_, end)| *end).collect::<Vec<_>>(), vec![PalwFilerEndV1::Stalled]);

        // The commitment, on the tick: never rooted.
        let mut book = PalwReporterFilerV1::default();
        let key = filed(&mut book, filing(41, None), &chain(100)).offence_key;
        let live = book.entries.get_mut(&key).unwrap();
        (live.commit_sent, live.commit_sends) = (Some(150), max - 1);
        let (mut queue, mut moved) = (Vec::new(), HashMap::new());
        tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, chain(200));
        assert_eq!((rounds(&queue), book.entry(&key).unwrap().commit_sends), (vec![PALW_FILER_ROUND_COMMIT_V1], max));
        for now in [200 + resend, 200 + 5 * resend] {
            tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, chain(now));
            assert_eq!(rounds(&queue), vec![PALW_FILER_ROUND_COMMIT_V1], "held at {now}: no evidence beside the commitment");
            assert!(book.entry(&key).unwrap().commit_object.is_some(), "and the commitment is not given up");
        }
        let carried = 200 + 5 * resend + 1;
        queue.clear();
        moved.insert((key, PALW_FILER_ROUND_COMMIT_V1, false), carried);
        tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, chain(carried + resend - 1));
        assert!(queue.is_empty(), "the last copy is still landing: the evidence waits for it");
        tick_admitting(&mut book, &mut queue, &mut moved, &mut accused, chain(carried + resend));
        assert_eq!(
            rounds(&queue),
            vec![PALW_FILER_ROUND_EVIDENCE_V1],
            "never rooted: the evidence alone, an interval after the carry"
        );
    }

    /// **The capture arm's decision, on a book** (the integration's review, F2 and F3:
    /// [`palw_capture_arm_files_v1`] is what the panel runs, over its door). Replay first: the
    /// capture arm's kind 4 is `AlreadyFiled`; it BACKS the replay's filing — its accusation adopted as
    /// the fallback, the claim `accused`, no court beside it — so the filing's stall files the court;
    /// and with no accusation to give (no court past the fence, a shape refused, a carrier it cannot
    /// ride) the stall still lets the claim go for the named-leaf pursuit. The capture arm first: its
    /// own filing, the accusation its fallback, and the replay's later hand-off `AlreadyFiled`. Kind 4
    /// the gate refuses, or none: the accusation, as before P2-8; once per claim.
    #[test]
    fn the_capture_arm_files_kind_4_first_and_backs_a_replay_filing_it_finds() {
        let court_key = h(0xC0_2D);
        let accusation =
            PalwConsensusObjectV2::DefaultAccused { claim: h(50), missing_event_index: 0, accuser: bond(ME), signature: vec![5] };
        let door = |book: &mut PalwReporterFilerV1, filing: PalwConvictionFilingV1| {
            book.file(filing, bond(ME), &chain(100), &h(DOMAIN), [9; 32], signer)
        };
        let replay = |claim| PalwConvictionFilingV1 { origin: PalwFilingOriginV1::Replay, ..filing(claim, None) };
        let rooted = PalwReporterFilingReadV1 { committed_daa: Some(101), ..chain(160) };
        for with_court in [true, false] {
            let mut book = PalwReporterFilerV1::default();
            let key = filed(&mut book, replay(50), &chain(100)).offence_key;
            let (mut accused, mut queue) = (HashSet::new(), Vec::new());
            let court = with_court.then(|| (court_key, accusation.clone()));
            let made =
                palw_capture_arm_files_v1(&mut book, &mut accused, &mut queue, h(50), 3, Some(filing(50, None)), court.clone(), door);
            assert_eq!(made, PalwCaptureArmFiledV1::Kind4(PalwFileOutcomeV1::AlreadyFiled));
            assert!(queue.is_empty() && accused.contains(&h(50)), "the replay's filing carries it: no court beside it");
            let live = book.entry(&key).unwrap();
            assert_eq!((live.origin, live.capture_arm_backed, live.fallback.clone()), (PalwFilingOriginV1::Replay, true, court));
            let live = book.entries.get_mut(&key).unwrap();
            (live.evidence_sent, live.evidence_sends) = (Some(150), PALW_FILER_MAX_SENDS_V1);
            let ended = tick_admitting(&mut book, &mut queue, &mut HashMap::new(), &mut accused, rooted.clone());
            assert_eq!(ended.iter().map(|(_, end)| *end).collect::<Vec<_>>(), vec![PalwFilerEndV1::Stalled]);
            if with_court {
                assert_eq!(queue, vec![(court_key, 0, false, accusation.clone())], "the court carries the fault");
                assert!(accused.contains(&h(50)));
            } else {
                assert!(queue.is_empty() && !accused.contains(&h(50)), "nothing to queue: the claim is let go (F2)");
            }
        }
        // The capture arm first.
        let mut book = PalwReporterFilerV1::default();
        let (mut accused, mut queue) = (HashSet::new(), Vec::new());
        let court = Some((court_key, accusation.clone()));
        let made =
            palw_capture_arm_files_v1(&mut book, &mut accused, &mut queue, h(51), 3, Some(filing(51, None)), court.clone(), door);
        assert_eq!(made, PalwCaptureArmFiledV1::Kind4(PalwFileOutcomeV1::Queued { committed: true }));
        assert!(queue.is_empty() && accused.contains(&h(51)));
        let key = filing(51, None).offence_key;
        assert_eq!(
            book.entry(&key).map(|live| (live.origin, live.fallback.clone())),
            Some((PalwFilingOriginV1::CaptureArm, court.clone()))
        );
        assert_eq!(door(&mut book, replay(51)), PalwFileOutcomeV1::AlreadyFiled, "the replay's later hand-off");
        let again =
            palw_capture_arm_files_v1(&mut book, &mut accused, &mut queue, h(51), 3, Some(filing(51, None)), court.clone(), door);
        assert_eq!((again, book.len(), queue.len()), (PalwCaptureArmFiledV1::AlreadyAccused, 1, 0), "once per claim");
        // Kind 4 refused by the gate, or none (below the fence): the accusation.
        let refusing = |book: &mut PalwReporterFilerV1, filing: PalwConvictionFilingV1| {
            let read = PalwReporterFilingReadV1 { object_gate: Some(Err("DaUnitNeedsDissection".into())), ..chain(100) };
            book.file(filing, bond(ME), &read, &h(DOMAIN), [9; 32], signer)
        };
        let mut book = PalwReporterFilerV1::default();
        let (mut accused, mut queue) = (HashSet::new(), Vec::new());
        let made =
            palw_capture_arm_files_v1(&mut book, &mut accused, &mut queue, h(52), 3, Some(filing(52, None)), court.clone(), refusing);
        assert_eq!((made, book.len()), (PalwCaptureArmFiledV1::Court, 0));
        assert_eq!(queue, vec![(court_key, 0, false, accusation.clone())]);
        assert!(accused.contains(&h(52)));
        let made = palw_capture_arm_files_v1(&mut book, &mut accused, &mut Vec::new(), h(53), 3, None, court, door);
        assert_eq!(made, PalwCaptureArmFiledV1::Court);
        let made = palw_capture_arm_files_v1(&mut book, &mut accused, &mut Vec::new(), h(54), 3, None, None, door);
        assert!(made == PalwCaptureArmFiledV1::Nothing && !accused.contains(&h(54)));
    }

    /// **A late kind 4 commits first, and nothing is dated before it was made** (the integration's
    /// review, F4). `palw_kind4_file_by_v1`: the landing margin while the claim's receipt deadline is
    /// ahead, none past it (the S2 race is over; kind 4 still convicts the claim's liability or
    /// vesting row, and a commitment's two DAA change nothing). A filing made past its margin but
    /// before the deadline goes out bare at once, dated at its making — not at a margin that passed
    /// before it existed; one made past the deadline commits first, dated by its court window.
    #[test]
    fn a_late_kind_4_commits_first_and_nothing_is_dated_before_it_was_made() {
        let deadline = 700;
        assert_eq!(palw_kind4_file_by_v1(deadline, 600), Some(640));
        assert_eq!(palw_kind4_file_by_v1(deadline, 650), Some(640), "inside the margin: the race is still live");
        assert_eq!(palw_kind4_file_by_v1(deadline, deadline), None, "past the deadline: no margin to keep");
        let mut book = PalwReporterFilerV1::default();
        let inside = filed(&mut book, filing(60, palw_kind4_file_by_v1(deadline, 650)), &chain(650));
        assert_eq!(palw_filer_step_v1(&inside, &chain(650)), PalwFilerStepV1::File, "bare, at once");
        assert_eq!(palw_filer_step_due_v1(&inside, PalwFilerStepV1::File, &chain(650)), Some(650), "due at its making, not 640");
        let late = filed(&mut book, filing(61, palw_kind4_file_by_v1(deadline, 800)), &chain(800));
        assert_eq!(palw_filer_step_v1(&late, &chain(800)), PalwFilerStepV1::Commit, "commits first: R kept");
        assert_eq!(
            palw_filer_step_due_v1(&late, PalwFilerStepV1::Commit, &chain(800)),
            Some(800 + 3_000 - PALW_FILER_COMMIT_DEPTH_DAA_V1)
        );
        assert_eq!(palw_filer_step_due_v1(&late, PalwFilerStepV1::File, &chain(800)), Some(800 + 3_000));
    }

    /// **J1 auto's digests are bounded per claim and window** (the review of P2-8, F6): once a claim's
    /// captures were digested `PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1` times in a window, no capture of it
    /// is hashed again until the next — a pool of captures probed long ago is not re-hashed every tick,
    /// and the tick's probe is left for other claims. Garbage ahead of a borrowed capture in the pool
    /// delays it, it cannot starve it: a pool of `MATERIALS_PER_CLAIM` walked tick by tick reaches the
    /// last capture inside one window.
    #[test]
    fn j1_auto_digests_a_claim_a_bounded_number_of_times_a_window() {
        let cap = usize::from(PALW_J1_DIGESTS_PER_CLAIM_WINDOW_V1);
        let window = PALW_J1_PROBE_WINDOW_DAA_V1;
        let mut book = PalwReporterFilerV1::default();
        let pool: Vec<Vec<u8>> = (0..MATERIALS_PER_CLAIM).map(|n| format!("capture {n}").into_bytes()).collect();
        let mut probed = Vec::new();
        let mut ticks = 0;
        // Tick by tick through one window, each tick walking the pool oldest first, as the arms do.
        while probed.len() < pool.len() {
            ticks += 1;
            tick_(&mut book, &mut Vec::new(), |_, _| None);
            for capture in &pool {
                if book.take_probe(h(1), capture, 10 * window).is_some() {
                    probed.push(capture.clone());
                }
            }
            assert!(ticks <= pool.len(), "every capture of the pool is reached inside the window");
        }
        assert_eq!(probed, pool, "oldest first, the last one too");
        // The rest of the window: every capture probed, the claim's digests run out and stay out.
        for _ in 0..cap {
            tick_(&mut book, &mut Vec::new(), |_, _| None);
            for capture in &pool {
                assert!(book.take_probe(h(1), capture, 10 * window + 1).is_none());
            }
        }
        assert_eq!(book.digests.get(&h(1)).copied().map(usize::from), Some(cap), "no more digests this window");
        assert!(book.take_probe(h(1), b"fresh", 10 * window + 2).is_none(), "not even a fresh capture is hashed");
        assert_eq!(book.probes_left, PALW_J1_PROBES_PER_TICK_V1, "and the tick's probe is left for another claim");
        assert!(book.take_probe(h(2), b"another claim", 10 * window + 2).is_some());
        tick_(&mut book, &mut Vec::new(), |_, _| None);
        assert!(book.take_probe(h(1), b"fresh", 11 * window).is_some(), "the next window digests again");
    }

    /// **The capture arm's one builder** (the review of P2-8, F2): over a real leaf of the floor —
    /// the sampler's own three steps (the leaf's refutation, its rows, the prompt carriage) — the
    /// filing is exactly `palw_executor_refuted_object_v1` over `StepArithmetic { refutation, rows }`
    /// and the carried tile, naming the claim's executor, keyed on the per-claim ledger id, landing
    /// by `deadline − 60`, with an empty reporter slot; and nothing below the fence or against this
    /// node's own bond. A wrong accused, a dropped row or tile, or a missing fence turns it red.
    #[test]
    fn the_capture_arms_filing_is_kind_4_over_the_samplers_fault_and_nothing_else() {
        use kaspa_consensus_core::palw_offence_attribution_v1::{PalwExecutorRefutedEvidenceV1, palw_executor_refuted_offence_id_v1};
        let backend = floor_backend();
        let (job, prompt) = backend.job_for_anchor(h(0x5A_4D)).expect("job");
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
        let run = backend.execute(&job, &prompt).expect("a run");
        let refutation = backend.refutation_for_index(&run.material, 0).expect("leaf 0 opens");
        let openings = backend.operand_openings_for(&refutation).expect("its rows");
        let (refutation, prompt_opening) = kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
            refutation,
        )
        .expect("the prover's list is the job's");
        assert!(!openings.is_empty() && prompt_opening.is_some(), "leaf 0 reads rows and gathers the prompt: both ride");
        let (executor, claim, deadline) = (bond(EXECUTOR), h(0xC1), 1_000);
        let build_at = |rcore_plus, own, now| {
            palw_capture_arm_filing_v1(rcore_plus, own, executor, claim, deadline, now, &refutation, &openings, &prompt_opening)
        };
        let build = |rcore_plus, own| build_at(rcore_plus, own, deadline - 100);
        let filing = build(true, bond(ME)).expect("past the fence, against another bond");
        let contradiction =
            PalwPanelContradictionV1::StepArithmetic { refutation: refutation.clone(), operand_openings: openings.clone() };
        assert_eq!(filing.object, palw_executor_refuted_object_v1(executor, claim, contradiction.clone(), prompt_opening.clone()));
        assert_eq!(
            (filing.accused, filing.claim_id, filing.offence_key, filing.file_by_daa, filing.origin),
            (
                executor,
                claim,
                palw_executor_refuted_offence_id_v1(&executor.0, &claim),
                Some(deadline - 60),
                PalwFilingOriginV1::CaptureArm
            )
        );
        assert!(filing.fallback.is_none(), "the court's accusation is the caller's to add");
        let PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } = &filing.object else { panic!("an objective offence") };
        let payload: PalwExecutorRefutedEvidenceV1 = borsh::from_slice(evidence).expect("kind 4's evidence");
        assert_eq!(
            (payload.claim_id, payload.contradiction, payload.prompt_ids_opening, payload.reporter_reveal),
            (claim, contradiction, prompt_opening.clone(), Vec::new())
        );
        assert_eq!(build(false, bond(ME)), None, "below the fence: the v1 court path, whole");
        assert_eq!(build(true, executor), None, "never against this node's own bond");
        // F4: inside the landing margin it still lands by it (bare); past the receipt deadline it
        // has no margin left to keep, and commits first.
        assert_eq!(build_at(true, bond(ME), deadline - 1).map(|f| f.file_by_daa), Some(Some(deadline - 60)));
        assert_eq!(build_at(true, bond(ME), deadline).map(|f| f.file_by_daa), Some(None));
    }

    /// **The panel's wiring, pinned where it lives** (the review of P2-8, F2): no test drives the
    /// panel's loop, so each hook is read from the source. (a) The capture arm's `FaultAt` records the
    /// fault, builds kind 4 before the accusation takes the refutation, builds the accusation in a
    /// block no exit of which skips kind 4, and hands both to ONE decision that files kind 4 first
    /// and the accusation only when kind 4 is not taken; the arm itself queues nothing. (b) kind 4 is
    /// built at the tip's fence and this node's bond, and J1 checks both before any work. (c) The
    /// filer ticks once, after the verdict loop and before the submitter's half, with `court_moved`
    /// and `accused`. (d) J1 hooks the FP capture arm's non-matching capture and the attempt pool's
    /// `Nothing`, each with the roots the arm just refused.
    #[test]
    fn the_panel_wires_the_capture_arm_j1_and_the_tick() {
        let whole = include_str!("palw_panel.rs");
        let source = &whole[..whole.find("mod court_responder_coverage_pin").expect("the panel's pins")];
        let this = include_str!("palw_reporter_filer.rs");
        let body = |signature: &str| {
            let start = this.find(signature).unwrap_or_else(|| panic!("{signature}"));
            &this[start..start + this[start..].find("\n    }\n").expect("its end")]
        };
        // (a)
        let fault = source.find("CaptureSamplesV1::FaultAt { leaf, refutation, openings, prompt_opening } => {").expect("the arm");
        let arm = &source[fault..fault + source[fault..].find("self.persist_foreign_material(&duty.claim_id, &bytes);").unwrap()];
        let at = |needle: &str| arm.find(needle).unwrap_or_else(|| panic!("the fault arm no longer contains {needle:?}"));
        let order = [
            at("self.note_seat_fault_v1(duty.claim_id, leaf, 1);"),
            at("let refuted = self.capture_arm_filing_v1("),
            at("let court = 'court: {"),
            at("PalwConsensusObjectV2::ShardCourtAccused {"),
            at("Ok(()) => break 'court Some((session_id, object)),"),
            at("self.capture_arm_files_v1("),
            arm.rfind("break 'verdict None;").expect("the arm ends"),
        ];
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        assert!(!arm[order[2]..order[5]].contains("break 'verdict"), "no exit of the accusation's block skips kind 4");
        assert!(!arm.contains("court_pending.push(") && !arm.contains("accused.insert("), "the decision queues, not the arm");
        // The decision is `palw_capture_arm_files_v1` (its order is tested on a book:
        // `the_capture_arm_files_kind_4_first_and_backs_a_replay_filing_it_finds`); the panel runs it
        // over its own door, and nothing else.
        let decide = body("pub(super) fn capture_arm_files_v1(");
        assert!(decide.contains(
            "palw_capture_arm_files_v1(filer, accused, court_pending, duty.claim_id, leaf, refuted, court, |filer, filing| {\n            \
             self.reporter_filer_file_v1(session, filer, filing, bond_key, network_domain)\n        });"
        ));
        assert!(!decide.contains("court_pending.push(") && !decide.contains("accused.insert("), "the decision queues, not the panel");
        // (b)
        assert!(body("pub(super) fn capture_arm_filing_v1(").contains(
            "self.consensus_config.params.palw_rcore_plus_active_at(current_daa),\n            bond_key,\n            duty.executor_bond,"
        ));
        let j1 = body("pub(super) async fn j1_auto_probe_v1(");
        let guard = j1
            .find("if !self.consensus_config.params.palw_rcore_plus_active_at(current_daa) || duty.executor_bond == bond_key {")
            .expect("J1's guard");
        assert!(guard < j1.find("filer.take_probe(duty.claim_id, bytes, current_daa)").expect("the probe"));
        // (c)
        const TICK: &str =
            "self.reporter_filer_tick_v1(&session, &mut reporter_filer, &mut court_pending, &mut court_moved, &mut accused);";
        assert_eq!(source.matches("self.reporter_filer_tick_v1(").count(), 1);
        let tick = source.find(TICK).expect("the tick, with every seam it prunes");
        let submitter = source.find("// --- the collector + submitter's half ---").expect("the submitter");
        assert!(fault < tick && tick < submitter);
        assert!(
            body("pub(super) fn reporter_filer_tick_v1(").contains("filer.tick(court_pending, court_moved, accused, |entry, gated| {")
        );
        // (d)
        let hooks: Vec<usize> = source.match_indices("self.j1_auto_probe_v1(").map(|(at, _)| at).collect();
        assert_eq!(hooks.len(), 2, "two hooks");
        for (hook, refused) in hooks.iter().zip([
            "if backend.verify_material(&payload.capture, roots) != PalwMaterialVerdictV1::Matches {",
            "if arm == PalwMaterialArmV1::Nothing {",
        ]) {
            let arm = source.find(refused).unwrap_or_else(|| panic!("{refused}"));
            let call = &source[*hook..*hook + source[*hook..].find(".await;").expect("awaited")];
            assert!(arm < *hook && source[arm..*hook].lines().count() <= 3, "{refused} hooks J1 at once");
            assert!(call.contains("&mut reporter_filer,") && call.contains("roots,\n"), "with the roots the arm refused");
        }
    }
}
