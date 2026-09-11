//! **PALW court-close carriage v1 — the cut, in the crate that assembles it** (ADR-0104).
//!
//! A court close that fits one carrier is a `CourtClosed` on an ordinary lifecycle transaction. A
//! close that does not is a signed `CourtCloseDeclared` pinning every byte that will follow, then
//! those bytes as `CourtCloseChunk`s — one per carrier — and the chunk that COMPLETES the group
//! assembles them, checks the declaration's `close_digest`, decodes and adjudicates through the
//! arm a one-carrier close takes (ADR-0080 design A, W5–W7).
//!
//! That assembler lives in [`crate::palw_state_v2`]. Until ADR-0104 the CUT lived in
//! `misaka-cli`, which meant the only program that could file a split close was a command-line
//! tool, and the party that actually prosecutes disputes — a node's own panel loop — built one
//! `CourtClosed`, handed it to one carrier, and logged a warning when it did not fit. **A close
//! denied through its assembly window is not a delay but a conviction of the declaring side**, so
//! that warning was a prosecution abandoned. This module is that cutter, moved here so the bytes a
//! filer sends and the bytes the chain assembles are decided by one function.
//!
//! # What is here and what is deliberately not
//!
//! Pure. A `PalwConsensusObjectV2` and a `PalwCourtParamsV2` in; objects, indices and refusals
//! out. **No key, no wallet, no RPC client, no I/O.** Signing the declaration stays with the
//! caller, because the key is the caller's and the acceptance layer verifies it against the
//! DECLARING side's registered bond — a fact about chain state that neither this module nor its
//! callers can check offline, and the reason a side is refused rather than defaulted.
//!
//! The chain's own answer about a group in flight arrives as [`PalwCourtCloseGroupSeenV1`], a
//! plain view of three numbers. A node fills it from `PalwChainStateV2::court_close_group`, which
//! it already holds; a CLI fills it from `GetPalwPendingChunkGroup`. Taking the RPC type here
//! would put `kaspa-rpc-core` under `kaspa-consensus-core` and would invite a node to ask itself
//! over the wire for state in its own hand.
//!
//! # The two ceilings, and which one is which
//!
//! * **the COST ceiling** — `PalwCourtParamsV2::max_close_bytes`, checked over the PROOF's own
//!   payload. It is inside `palw_ruleset_id_v2`, so a close over it is refused on the merits by
//!   every node and no carriage helps.
//! * **the CARRIER ceiling** — [`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`] = 100,000, measured over the
//!   SERIALIZED OBJECT, which carries the proof payload PLUS a `PalwStepBindingV2` the cost rule
//!   counts none of.
//!
//! W5 stopped choosing the first and started deriving it from the second:
//! `max_close_bytes = palw_close_bytes_for_chunks_v1(max_close_chunks)`, where the framing
//! fraction is the allowance for exactly those untolled bytes. So the same carrier reads 100,000
//! serialized and `palw_close_bytes_for_chunks_v1(1)` = 83,333 counted, and those are one number
//! in two units rather than two numbers. **A chunk count is decided by serializing the finished
//! close — never by predicting it from a counted-byte estimate**, which is why
//! [`palw_plan_court_close_carriage_v1`] takes the object and not a size.
//!
//! The count a ruleset pays for is its own: `palw_rc_shipped_params()` carries 27,
//! `devnet_shipped_params()` frames to **1**, and on that network the split path must REFUSE
//! rather than engage. One cutter, two answers, each read off the network's court.

use crate::Hash64;
use crate::palw_mode_v2::PalwCourtParamsV2;
use crate::palw_state_v2::{
    PALW_COURT_CLOSE_CHUNK_MAX_BYTES, PALW_COURT_CLOSE_INCLUSION_MARGIN, PALW_COURT_CLOSE_MAX_CHUNKS, PalwConsensusObjectV2,
    PalwCourtSideV1, palw_close_assembly_daa_v1, palw_court_close_chunk_digest_v1,
};

// =================================================================================================
// Refusals
// =================================================================================================

/// **Why a carriage was refused, in facts rather than in prose.**
///
/// Every variant carries the numbers a caller needs to say it in its own words: a node logs one
/// way and an operator's tool reads another, and a shared message would end up written for
/// whichever reader was imagined first. What is shared is the RULE — which refusal applies, and
/// on which numbers.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCloseCarriageError {
    #[error("this object is not a court close")]
    NotACourtClose,

    #[error("this close does not serialize: {why}")]
    DoesNotSerialize { why: String },

    /// The close is wider than any carriage this network pays for. `ruleset_binds` says WHICH
    /// ceiling stopped it, because the two need different actions: the ruleset's is a fact about
    /// the network the dispute is on and the only remedy is a smaller close, while the row's is a
    /// fact about the state layout that no ruleset can raise.
    #[error("this close serializes to {whole_bytes} bytes — {count} carriers, and at most {max} are admitted")]
    TooManyCarriers { whole_bytes: usize, count: usize, max: u8, ruleset_binds: bool },

    /// A `CourtClosed` names no side and a declaration cannot be built without one. Refused rather
    /// than defaulted: a default here files one party's move under the other party's bond, and the
    /// mover pays for it.
    #[error("this close needs {count} carriers, so it rides as a declaration — and a declaration must name its side")]
    SideNotNamed { whole_bytes: usize, count: usize },

    /// The cutter produced a chunk the transition would refuse by name (`CourtCloseChunkTooLarge`).
    /// Belt to the count's braces: a cutter that could emit one is a cutter whose output has to be
    /// trusted rather than checked.
    #[error("chunk {index} came out {bytes} bytes, outside what one carrier holds")]
    CutOutOfRange { index: usize, bytes: usize },

    /// **One declaration per `(session, side)`, ever.** The chain's group is declared in a
    /// different number of chunks, so it is not this close's group and never will be.
    #[error("the chain's group on this side is declared in {seen} chunks and this close needs {planned}")]
    GroupCountDiffers { planned: u8, seen: u32 },

    /// The chain's group pins a different close. Filing into it spends carriers to complete
    /// somebody else's assembly, which at completion convicts the DECLARING side — this side.
    #[error("the chain's group on this side pins another close")]
    GroupDigestDiffers { planned: Hash64, seen: Option<Hash64> },

    /// The declaration gate refuses a group that cannot finish inside the session's backstop
    /// (`CourtCloseCannotAssemble`), and the backstop never moves. Asked here before the
    /// declaration is funded, with the same expression.
    #[error("this group needs {needed} DAA to assemble and the session ends in {left}")]
    AssemblyWindowTooShort { chunk_count: u8, needed: u64, now: u64, backstop: u64, left: u64, fits: u64 },
}

// =================================================================================================
// The plan
// =================================================================================================

/// One close's carriage, decided before a fee is spent.
#[derive(Debug, Clone)]
pub struct PalwCourtCloseCarriageV1 {
    pub session_id: Hash64,
    /// Which of the two bonds the session id binds is moving. `None` when the close rides whole: a
    /// `CourtClosed` attributes itself to nobody, because nothing rides behind it to attribute.
    pub side: Option<PalwCourtSideV1>,
    /// The declaration, when the close is split — also `parts[0]`. Kept separately because every
    /// caller has to treat it differently: it is the only part that is signed, the only one that
    /// must land first, and the only one whose refusal strands the rest.
    pub declaration: Option<PalwConsensusObjectV2>,
    /// The objects to carry, in the order the chain must see them: the declaration, then the
    /// chunks in index order. One entry when the close rides whole.
    pub parts: Vec<PalwConsensusObjectV2>,
    /// The serialized close, before cutting.
    pub whole_bytes: usize,
    /// The chunks alone, without the declaration. Zero when the close rides whole. This is the
    /// number every consensus rule is denominated in — the ruleset's count, the row's bitmap and
    /// the assembly window all count CHUNKS — so it is spelled once and never re-derived from
    /// `parts.len()`, which is one larger.
    pub chunk_count: u8,
    /// `palw_court_close_chunk_digest_v1` of the concatenation — what the declaration pins as
    /// `close_digest`, and what the completing chunk checks the assembly against.
    pub close_digest: Hash64,
}

impl PalwCourtCloseCarriageV1 {
    /// Whether this close opens a group at all.
    pub fn is_split(&self) -> bool {
        self.chunk_count > 0
    }

    /// The blocks this close spends of the mover's own turn — the `close_blocks` term
    /// `palw_court_move_cost_daa_v1` takes, and the reason it takes it. The declaration is one of
    /// them: it is a court move on a carrier of its own.
    pub fn close_blocks(&self) -> u64 {
        self.parts.len() as u64
    }

    /// The DAA the chain gives this group to finish arriving, from the block that carries the
    /// declaration — [`PALW_COURT_CLOSE_INCLUSION_MARGIN`] per chunk, read from the same function
    /// the transition reads it from rather than multiplied here.
    pub fn assembly_daa(&self) -> u64 {
        palw_close_assembly_daa_v1(self.chunk_count)
    }

    /// **The chain's own key for this group** — `(session_id, side)`, and not a digest of the
    /// bytes. That is the whole reason the court has its own table rather than riding ADR-0075
    /// D14's `pending_chunks`: a free digest can be squatted in unbounded quantity and that
    /// table's refusal is a delay for a certification and a DISPUTE LOST for a court. A side of a
    /// session cannot be squatted, and one declaration per side is all a session will accept.
    ///
    /// Empty for a close that rides whole, which has no group to resume into.
    pub fn group_key(&self) -> String {
        match self.side {
            None => String::new(),
            Some(side) => format!("{}/{}", self.session_id, side.name()),
        }
    }
}

/// **What the chain says about a group already in flight**, in the three numbers a resume needs.
///
/// A plain view rather than an RPC type, so both filers can fill it from what they have: a node
/// from `PalwChainStateV2::court_close_group`, a tool from `GetPalwPendingChunkGroup`. It is only
/// ever consulted for a group the chain says it FOUND — an absent group is `None`, not a zeroed
/// row, because "no declaration yet" and "a declaration of nothing" are different answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PalwCourtCloseGroupSeenV1 {
    /// The chunks the declaration pinned. Taken as `u32` because that is what the wire carries; a
    /// legal one is `1..=PALW_COURT_CLOSE_MAX_CHUNKS`, and anything else can only disagree with a
    /// plan, which is the answer this view is consulted for.
    pub count: u32,
    /// One bit per index, exactly as `PalwCourtCloseGroupV2::present` holds it. **A set, not a
    /// prefix** — chunks arrive in any order and a carrier can be orphaned.
    pub present: u64,
    /// The close the group pins. `None` when the answer carries no digest this build can read,
    /// which is not this plan's digest either — and is reported as the mismatch it is rather than
    /// as a parse failure a filer might retry.
    pub close_digest: Option<Hash64>,
}

// =================================================================================================
// The cut
// =================================================================================================

/// **How many carriers one court close may spend — from the ruleset that will judge it.**
///
/// Two ceilings, and the smaller is the answer. `PalwCourtParamsV2::max_close_chunks` is the
/// network's: it is inside `palw_ruleset_id_v2`, it is what class admission prices a class
/// against, and it is 27 on the RC and 1 on devnet. [`PALW_COURT_CLOSE_MAX_CHUNKS`] is the ROW's:
/// `PalwCourtCloseGroupV2::present` is a `u64` bitmap, so a count above it could not be
/// represented whatever a ruleset said.
pub fn palw_court_close_max_parts_v1(court: &PalwCourtParamsV2) -> u8 {
    court.max_close_chunks().min(PALW_COURT_CLOSE_MAX_CHUNKS as u64) as u8
}

/// **Cut the close the way the court's own table cuts it, or say it rides whole.**
///
/// Not `palw_object_chunks_v1`: that is the certification lane's cutter, it keys a group by a free
/// digest and caps at `PALW_OBJECT_CHUNK_MAX_COUNT`, and a court close does not ride there. The
/// court's cut is by [`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`], pinned by
/// [`palw_court_close_chunk_digest_v1`] per index, and bounded by
/// [`palw_court_close_max_parts_v1`].
///
/// **Every refusal it can make, it makes here** — before a plan exists to price, let alone fund.
/// The failure this exists to prevent is a mover paying carrier after carrier into a group that
/// was never going to assemble, and a limit discovered at the last carrier is that failure.
///
/// The declaration comes back UNSIGNED, and that is not an oversight: `palw_lifecycle_object_may_
/// ride_v2` refuses a declaration with no signature, so a plan cannot be filed by accident, and
/// the key that must sign it is the declaring side's registered bond key — which is the caller's,
/// not this module's.
pub fn palw_plan_court_close_carriage_v1(
    object: &PalwConsensusObjectV2,
    court: &PalwCourtParamsV2,
    side: Option<PalwCourtSideV1>,
) -> Result<PalwCourtCloseCarriageV1, PalwCloseCarriageError> {
    let PalwConsensusObjectV2::CourtClosed { session_id, verdict, .. } = object else {
        return Err(PalwCloseCarriageError::NotACourtClose);
    };
    let whole = borsh::to_vec(object).map_err(|e| PalwCloseCarriageError::DoesNotSerialize { why: e.to_string() })?;
    let whole_bytes = whole.len();
    let close_digest = palw_court_close_chunk_digest_v1(&whole);
    if whole_bytes <= PALW_COURT_CLOSE_CHUNK_MAX_BYTES {
        return Ok(PalwCourtCloseCarriageV1 {
            session_id: *session_id,
            side: None,
            declaration: None,
            parts: vec![object.clone()],
            whole_bytes,
            chunk_count: 0,
            close_digest,
        });
    }
    let count = whole_bytes.div_ceil(PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
    let max = palw_court_close_max_parts_v1(court);
    if count > max as usize {
        // Which ceiling stopped it. The ruleset's binds when it is at or under the row's; above
        // that the row is what the state can address and no ruleset raises it.
        let ruleset_binds = court.max_close_chunks() <= PALW_COURT_CLOSE_MAX_CHUNKS as u64;
        return Err(PalwCloseCarriageError::TooManyCarriers { whole_bytes, count, max, ruleset_binds });
    }
    let Some(side) = side else {
        return Err(PalwCloseCarriageError::SideNotNamed { whole_bytes, count });
    };
    let chunks: Vec<Vec<u8>> = whole.chunks(PALW_COURT_CLOSE_CHUNK_MAX_BYTES).map(|part| part.to_vec()).collect();
    debug_assert_eq!(chunks.len(), count);
    if let Some(index) = chunks.iter().position(|part| part.is_empty() || part.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES) {
        return Err(PalwCloseCarriageError::CutOutOfRange { index, bytes: chunks[index].len() });
    }
    let chunk_digests: Vec<Hash64> = chunks.iter().map(|part| palw_court_close_chunk_digest_v1(part)).collect();
    let declaration = PalwConsensusObjectV2::CourtCloseDeclared {
        session_id: *session_id,
        side,
        count: count as u8,
        chunk_digests,
        close_digest,
        verdict: *verdict,
        signature: Vec::new(),
    };
    let mut parts = Vec::with_capacity(count + 1);
    parts.push(declaration.clone());
    parts.extend(chunks.into_iter().enumerate().map(|(index, bytes)| PalwConsensusObjectV2::CourtCloseChunk {
        session_id: *session_id,
        side,
        index: index as u8,
        bytes,
    }));
    Ok(PalwCourtCloseCarriageV1 {
        session_id: *session_id,
        side: Some(side),
        declaration: Some(declaration),
        parts,
        whole_bytes,
        chunk_count: count as u8,
        close_digest,
    })
}

/// **Which of this plan's parts the chain has NOT got yet** (ADR-0080 design A, W6/W7).
///
/// A pure function of the plan and the chain's own answer, so the half of a resume that is
/// otherwise only exercised by an outage can be tested without a node.
///
/// The bitmap is why this exists. `present` is one bit per index and chunks arrive in ANY order,
/// so "four of seven have landed" does not say which three are missing — a filer that walked its
/// own journal could only ever answer with a PREFIX, which is wrong the moment one carrier in the
/// middle is orphaned.
///
/// Two refusals rather than a resume, and both are the same fact: **one declaration per
/// `(session, side)`, ever.** A group pinning a different count or a different close is not this
/// close's group and never will be; filing into it spends carriers to complete somebody else's
/// assembly, which at completion convicts the declaring side. Better to lose the move than the
/// dispute.
///
/// Part 0 is the declaration and the group's existence is proof it landed; part `i + 1` carries
/// chunk `i`, and a set bit is a chunk the chain already has.
pub fn palw_court_close_parts_to_send_v1(
    plan: &PalwCourtCloseCarriageV1,
    group: Option<&PalwCourtCloseGroupSeenV1>,
) -> Result<Vec<usize>, PalwCloseCarriageError> {
    palw_court_close_parts_owed_v1(plan.chunk_count, plan.close_digest, group)
}

/// [`palw_court_close_parts_to_send_v1`] without the plan — **the resume is a function of the
/// group's IDENTITY and the chain's bitmap, never of the bytes.**
///
/// A filer that has already cut its close and is holding the parts across ticks does not need to
/// hand them back to be counted, and a caller that had to assemble a plan-shaped value just to ask
/// this question would be filling fields nothing reads. Both callers reach the same arithmetic:
/// this is where it is written, and the plan form above is one line over it.
///
/// The indices are into the parts as the chain must see them — 0 is the declaration, `i + 1`
/// carries chunk `i` — and a whole close is the `chunk_count = 0` case, which owes part 0 alone.
pub fn palw_court_close_parts_owed_v1(
    chunk_count: u8,
    close_digest: Hash64,
    group: Option<&PalwCourtCloseGroupSeenV1>,
) -> Result<Vec<usize>, PalwCloseCarriageError> {
    let Some(group) = group else {
        return Ok((0..=usize::from(chunk_count)).collect());
    };
    if u64::from(chunk_count) != u64::from(group.count) {
        return Err(PalwCloseCarriageError::GroupCountDiffers { planned: chunk_count, seen: group.count });
    }
    if group.close_digest != Some(close_digest) {
        return Err(PalwCloseCarriageError::GroupDigestDiffers { planned: close_digest, seen: group.close_digest });
    }
    Ok((0..chunk_count).filter(|index| group.present & (1u64 << index) == 0).map(|index| index as usize + 1).collect())
}

/// **Will the chain give this group time to finish arriving?**
///
/// The declaration gate refuses a group that cannot assemble inside the session's BACKSTOP —
/// `declared_daa + palw_close_assembly_daa_v1(count) > session.deadline_daa` is
/// `CourtCloseCannotAssemble` — and the backstop never moves, because extending it would sell
/// either party a free window for the price of a declaration. Checked here against the same
/// expression, with the same consensus function, before the declaration is paid for.
///
/// It needs two numbers a filer must supply rather than invent: where the chain is, and where the
/// session ends. With neither there is no honest answer and this is not called — a window checked
/// against a turn deadline would be a second clock, and a mover would find out which one the chain
/// kept at the last carrier.
pub fn palw_court_close_assembly_fits_v1(chunk_count: u8, now: u64, backstop: u64) -> Result<(), PalwCloseCarriageError> {
    let needed = palw_close_assembly_daa_v1(chunk_count);
    if now.saturating_add(needed) <= backstop {
        return Ok(());
    }
    let left = backstop.saturating_sub(now);
    Err(PalwCloseCarriageError::AssemblyWindowTooShort {
        chunk_count,
        needed,
        now,
        backstop,
        left,
        fits: left / PALW_COURT_CLOSE_INCLUSION_MARGIN,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{Params, devnet_shipped_params, palw_rc_shipped_params};
    use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1};
    use crate::palw_court_v2::PalwCourtVerdictProofV2;
    use crate::palw_mode_v2::{PalwConsensusMode, palw_close_bytes_for_chunks_v1};
    use crate::palw_state_v2::PalwCourtVerdictV2;

    /// The court a shipped preset ships, read off the bundle rather than rebuilt: these tests are
    /// about the numbers the NETWORK will judge a close by.
    fn court_of_v1(params: &Params) -> PalwCourtParamsV2 {
        match &params.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.court,
            _ => panic!("this preset ships no V2 bundle, so it has no court"),
        }
    }

    /// A close whose serialized size is driven by one padded operand opening, so a test can ask
    /// for "a close that needs three carriers" and get one.
    fn close_of_bytes(pad: usize) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::CourtClosed {
            session_id: Hash64::from(7u64),
            verdict: PalwCourtVerdictV2::ExecutorGuilty,
            proof: PalwCourtVerdictProofV2::Arithmetic {
                refutation: crate::palw_step_refute::tests::skeleton_refutation(),
                operand_openings: vec![PalwArtifactOpeningV1 {
                    operand: PalwArtifactOperandV1 {
                        tensor_name: "blk.0.attn_q.weight".to_string(),
                        layer: Some(0),
                        row_start: 0,
                        bytes: vec![0x5Au8; pad],
                    },
                    leaf_index: 0,
                    leaf_count: 1,
                    path: Vec::new(),
                }],
            },
        }
    }

    /// A close cut to `want` carriers, found by measurement rather than by predicting the
    /// serialized size of a structure — which is the mistake this module's doc names.
    fn close_needing(want: usize) -> PalwConsensusObjectV2 {
        let empty = borsh::to_vec(&close_of_bytes(0)).expect("serializes").len();
        // One byte of pad is one byte of object, so the pad that lands in the middle of carrier
        // `want` is the whole minus what `want - 1` carriers already hold, minus the fixed part.
        let target = (want - 1) * PALW_COURT_CLOSE_CHUNK_MAX_BYTES + PALW_COURT_CLOSE_CHUNK_MAX_BYTES / 2;
        let object = close_of_bytes(target.saturating_sub(empty));
        let cut = borsh::to_vec(&object).expect("serializes").len().div_ceil(PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
        assert_eq!(cut, want, "the fixture did not land on {want} carriers");
        object
    }

    fn rc_court() -> PalwCourtParamsV2 {
        court_of_v1(&palw_rc_shipped_params())
    }

    /// **Invariant 1: the cut is the court's own.** Every part's digest is the transition's own
    /// digest of its bytes, the declaration pins them in index order, and the concatenation is the
    /// serialized close — which is the exact check the completing chunk performs, so a cutter that
    /// passed this and failed there would be a cutter the chain convicts its user for.
    #[test]
    fn the_cut_is_the_one_the_completing_chunk_checks() {
        let object = close_needing(3);
        let whole = borsh::to_vec(&object).expect("serializes");
        let plan = palw_plan_court_close_carriage_v1(&object, &rc_court(), Some(PalwCourtSideV1::Executor)).expect("plans");
        assert_eq!(plan.chunk_count, 3);
        assert_eq!(plan.parts.len(), 4, "a split close rides as a declaration and its chunks");
        assert_eq!(plan.whole_bytes, whole.len());
        assert_eq!(plan.close_digest, palw_court_close_chunk_digest_v1(&whole));

        let PalwConsensusObjectV2::CourtCloseDeclared { count, chunk_digests, close_digest, side, session_id, signature, .. } =
            plan.declaration.as_ref().expect("declares")
        else {
            panic!("part 0 is not a declaration");
        };
        assert_eq!(*count, plan.chunk_count);
        assert_eq!(*close_digest, plan.close_digest);
        assert_eq!(*side, PalwCourtSideV1::Executor);
        assert_eq!(*session_id, plan.session_id);
        assert!(signature.is_empty(), "this module must not pretend to sign; the bond key is the caller's");

        let mut assembled = Vec::new();
        for (index, part) in plan.parts[1..].iter().enumerate() {
            let PalwConsensusObjectV2::CourtCloseChunk { index: at, bytes, .. } = part else { panic!("part {index} is not a chunk") };
            assert_eq!(*at as usize, index, "the chunks are not in index order");
            assert_eq!(
                chunk_digests[index],
                palw_court_close_chunk_digest_v1(bytes),
                "chunk {index} is not what the declaration pins"
            );
            assert!(!bytes.is_empty() && bytes.len() <= PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
            assembled.extend_from_slice(bytes);
        }
        assert_eq!(assembled, whole, "the assembly is not the close");
        assert_eq!(palw_court_close_chunk_digest_v1(&assembled), *close_digest);
    }

    /// A close that fits one carrier opens no group: no declaration, no side, no assembly window
    /// and nothing to resume into.
    #[test]
    fn a_close_that_fits_one_carrier_opens_nothing() {
        let plan = palw_plan_court_close_carriage_v1(&close_of_bytes(0), &rc_court(), None).expect("plans");
        assert!(!plan.is_split());
        assert_eq!(plan.chunk_count, 0);
        assert_eq!(plan.parts.len(), 1);
        assert!(plan.declaration.is_none());
        assert_eq!(plan.assembly_daa(), 0);
        assert!(plan.group_key().is_empty());
        assert_eq!(plan.close_blocks(), 1);
    }

    /// **Invariant 5: `max_close_chunks` binds, and it is the network's number.** Devnet pays for
    /// one carrier, so the split path is refused there rather than engaged; the RC pays for 27 and
    /// admits the same close.
    #[test]
    fn the_ruleset_decides_whether_a_split_close_may_be_filed_at_all() {
        let devnet = court_of_v1(&devnet_shipped_params());
        assert_eq!(palw_court_close_max_parts_v1(&devnet), 1, "devnet frames to one carrier");
        assert_eq!(palw_court_close_max_parts_v1(&rc_court()), 27, "the RC pays for 27");

        let object = close_needing(3);
        let why = palw_plan_court_close_carriage_v1(&object, &devnet, Some(PalwCourtSideV1::Executor))
            .expect_err("a one-carrier ruleset must refuse a three-carrier close");
        let PalwCloseCarriageError::TooManyCarriers { count, max, ruleset_binds, .. } = why else { panic!("{why}") };
        assert_eq!((count, max), (3, 1));
        assert!(ruleset_binds, "on devnet it is the RULESET that refuses, and no build change helps");
        palw_plan_court_close_carriage_v1(&object, &rc_court(), Some(PalwCourtSideV1::Executor)).expect("the RC admits it");
    }

    /// The row's cap is the one that binds when a ruleset asks for more than the bitmap can
    /// address, and it is named separately because no ruleset raises it.
    #[test]
    fn a_ruleset_above_the_rows_bitmap_is_bound_by_the_row() {
        let wide = PalwCourtParamsV2::with_cost_ceilings(
            1 << 22,
            45,
            1,
            palw_close_bytes_for_chunks_v1(PALW_COURT_CLOSE_MAX_CHUNKS as u64 + 8),
            1 << 30,
            64,
        )
        .expect("a court above the row's cap");
        assert!(wide.max_close_chunks() > PALW_COURT_CLOSE_MAX_CHUNKS as u64);
        assert_eq!(palw_court_close_max_parts_v1(&wide), PALW_COURT_CLOSE_MAX_CHUNKS);
    }

    /// A split close names the side rather than defaulting it: the transition reads the declarer
    /// from the session (challenger) or the claim (executor), so a guess files one party's move
    /// under the other party's bond.
    #[test]
    fn a_split_close_refuses_to_guess_its_side() {
        let why = palw_plan_court_close_carriage_v1(&close_needing(2), &rc_court(), None).expect_err("a split close needs a side");
        assert!(matches!(why, PalwCloseCarriageError::SideNotNamed { count: 2, .. }), "{why}");
        // And the two sides are two groups: one side's resume must not walk into the other's.
        let executor = palw_plan_court_close_carriage_v1(&close_needing(2), &rc_court(), Some(PalwCourtSideV1::Executor)).unwrap();
        let challenger = palw_plan_court_close_carriage_v1(&close_needing(2), &rc_court(), Some(PalwCourtSideV1::Challenger)).unwrap();
        assert_ne!(executor.group_key(), challenger.group_key());
        assert!(executor.group_key().ends_with("executor"));
    }

    /// **Invariant 3: a resume sends the parts the bitmap LACKS — a set, never a prefix.**
    #[test]
    fn a_resume_is_a_set_and_not_a_prefix() {
        let object = close_needing(4);
        let plan = palw_plan_court_close_carriage_v1(&object, &rc_court(), Some(PalwCourtSideV1::Executor)).expect("plans");
        let seen = |present: u64| PalwCourtCloseGroupSeenV1 {
            count: u32::from(plan.chunk_count),
            present,
            close_digest: Some(plan.close_digest),
        };

        // No group at all: everything, declaration first.
        assert_eq!(palw_court_close_parts_to_send_v1(&plan, None).unwrap(), vec![0, 1, 2, 3, 4]);
        // A gap in the middle with the top index set — the shape a prefix gets wrong.
        assert_eq!(palw_court_close_parts_to_send_v1(&plan, Some(&seen(0b1001))).unwrap(), vec![2, 3]);
        // Complete: nothing, and in particular not the declaration again.
        assert!(palw_court_close_parts_to_send_v1(&plan, Some(&seen(0b1111))).unwrap().is_empty());
        // Declared and empty: every chunk, but never part 0 — the group's existence proves it
        // landed, and a second declaration on a side is refused.
        assert_eq!(palw_court_close_parts_to_send_v1(&plan, Some(&seen(0))).unwrap(), vec![1, 2, 3, 4]);
    }

    /// **Invariant 4: a group that is not this plan's is refused before a carrier is spent** —
    /// both by count and by the close it pins, and an unreadable digest is a mismatch rather than
    /// a retry.
    #[test]
    fn another_close_on_this_side_is_refused_rather_than_completed() {
        let plan = palw_plan_court_close_carriage_v1(&close_needing(3), &rc_court(), Some(PalwCourtSideV1::Executor)).expect("plans");
        let wider =
            PalwCourtCloseGroupSeenV1 { count: u32::from(plan.chunk_count) + 1, present: 0, close_digest: Some(plan.close_digest) };
        assert!(
            matches!(
                palw_court_close_parts_to_send_v1(&plan, Some(&wider)),
                Err(PalwCloseCarriageError::GroupCountDiffers { planned: 3, seen: 4 })
            ),
            "a group of another size is another close"
        );
        for other in [Some(Hash64::from(0xABCDu64)), None] {
            let group = PalwCourtCloseGroupSeenV1 { count: u32::from(plan.chunk_count), present: 0, close_digest: other };
            assert!(
                matches!(
                    palw_court_close_parts_to_send_v1(&plan, Some(&group)),
                    Err(PalwCloseCarriageError::GroupDigestDiffers { .. })
                ),
                "completing a group pinning another close convicts the declaring side"
            );
        }
    }

    /// The assembly window is the chain's own arithmetic, asked before the declaration is funded,
    /// and its refusal says how many chunks WOULD have fitted.
    #[test]
    fn the_assembly_window_is_asked_before_the_declaration_is_paid_for() {
        assert_eq!(palw_court_close_assembly_fits_v1(3, 100, 112), Ok(()), "3 chunks need 12 DAA and there are 12");
        let why = palw_court_close_assembly_fits_v1(3, 100, 111).expect_err("one DAA short is short");
        let PalwCloseCarriageError::AssemblyWindowTooShort { needed, left, fits, .. } = why else { panic!("{why}") };
        assert_eq!((needed, left, fits), (12, 11, 2));
        // The margin is consensus's, not this module's.
        assert_eq!(needed, palw_close_assembly_daa_v1(3));
        assert_eq!(PALW_COURT_CLOSE_INCLUSION_MARGIN * 3, needed);
    }
}
