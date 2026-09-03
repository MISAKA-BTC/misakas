//! **`misaka palw court-close` — filing a court close, whole or in the carriage ADR-0080 gave it.**
//!
//! A close that fits one carrier is a `CourtClosed` object on an ordinary lifecycle transaction. A
//! close that does not is a small signed `CourtCloseDeclared` that pins every byte that will
//! follow, and then those bytes as `CourtCloseChunk`s, one per carrier. Until this command the only
//! thing in the tree that filed either was a node's own panel loop (`kaspad/src/palw_panel.rs`),
//! which builds a close from a capture it happens to hold and drops it on the floor if it does not
//! fit. An operator holding an assembled close — from a drill, from another node's outbox, from a
//! court arm that stalled — had no way to put it on chain at all, and no way to find out BEFORE
//! spending a fee whether it would be refused.
//!
//! # The two ceilings, and the unit the room between them is now measured in
//!
//! * **the COST ceiling** — `PalwCourtParamsV2::max_close_bytes`, checked by
//!   [`check_close_cost_v2`] over the PROOF's own payload: openings at 64 bytes a sibling, logits
//!   rows at 4 bytes a lane, the ids. It is a consensus rule and it is inside `palw_ruleset_id_v2`.
//!   A close over it is refused by every node on the merits, and no amount of carriage helps.
//! * **the CARRIER ceiling** — [`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`], the mempool's
//!   standard-transaction mass, measured over the SERIALIZED OBJECT — which carries the proof
//!   payload PLUS a `PalwStepBindingV2`, and that carries the class's whole `PalwShapeProfileV3`
//!   ("carried in full: adjudication needs the node tables"), the job context and the checkpoint
//!   profile. The cost rule counts none of it.
//!
//! **This file used to state the room between them as 4,084 bytes, in a table, and W5 spent it.**
//! ADR-0080's W5 stopped CHOOSING `max_close_bytes` and started deriving it from a carriage count:
//!
//! ```text
//!   max_close_bytes = palw_close_bytes_for_chunks_v1(max_close_chunks)
//!                   = max_close_chunks × PALW_COURT_CLOSE_CHUNK_MAX_BYTES
//!                       × PALW_CLOSE_FRAMING_DENOMINATOR ÷ PALW_CLOSE_FRAMING_NUMERATOR
//! ```
//!
//! So the two ceilings stopped being two independent numbers with a byte gap between them: the
//! framing fraction IS that gap, promoted into the ruleset. 10/12 is the allowance W5 made for
//! exactly the untolled bytes this file measures — the binding, and borsh's own framing — and the
//! question a filing tool must answer therefore changed UNITS. It is no longer "does the largest
//! legal close fit ONE carrier", which on the RC ruleset it does not, by a factor of twenty-three.
//! It is
//!
//! ```text
//!   ⌈ (max_close_bytes + the widest shipped binding) ÷ PALW_COURT_CLOSE_CHUNK_MAX_BYTES ⌉
//!       ≤   max_close_chunks
//! ```
//!
//! and both sides are measured rather than typed:
//! [`tests::the_headroom_between_the_two_ceilings_is_measured_not_assumed`] builds the widest close
//! each shipped court admits, cuts it with this file's own cutter, and asserts that every carrier
//! fits and that there are few enough of them. The numbers are deliberately not restated in this
//! paragraph. A table in a doc comment is a claim nothing re-derives, which is exactly how the
//! previous version of it came to describe, as fact, a ruleset the build had already left.
//!
//! **The two shipped rulesets answer it differently, which is why the count is a ruleset field and
//! not a constant.** `devnet_shipped_params()` keeps the pre-ADR-0080 81,920-byte ceiling, and it
//! frames to `max_close_chunks = 1`: on devnet a legal close still has to fit one carrier, and this
//! command's split path must REFUSE rather than engage. `palw_rc_shipped_params()` carries 27. One
//! tool, two answers, each read off the network's own court rather than off a default.
//!
//! # What this command CANNOT do yet — and it is not the gap this file used to name
//!
//! The previous version of [`court_close_chunked_carriage_v1`] said a split close was blocked on
//! admitting `CourtClosed` into `apply_object`'s generic `ObjectChunk` arm. **That was a design
//! nobody built.** W5 deliberately did not widen the certification lane — `palw_chunked_object_
//! kind_admitted_v1` still admits `FamilyCertified` alone, and says why in its own doc: that
//! table's `TooManyPendingChunkGroups` is a delay for a drill and a DISPUTE LOST for a court, so
//! anyone able to rent eight slots could defeat every prosecution on the network. The close got its
//! own table instead, keyed `(session_id, side)` and therefore unsquattable.
//!
//! **That table is complete and it is not reachable.** `apply_object` has both arms, the group has
//! its bitmap and its digests, and the per-chunk digest is refused at ARRIVAL. What is missing is
//! on either side of it:
//!
//! * **W6, the authentication.** `palw_v2_validate_objects` refuses EVERY `CourtCloseDeclared`
//!   outright — "no layer yet verifies the declaring side's signature … refused rather than
//!   trusted" — and a refused lifecycle object is DROPPED with the block standing. So a declaration
//!   filed today costs its fee, produces one `info!` line on each node, and opens no group; the
//!   chunks behind it then fail `MissingCourtCloseGroup` one carrier at a time. There is also
//!   nothing to sign: `PALW_COURT_V2_ALL_DOMAINS` carries no close-declaration context, so the
//!   message a declaration binds does not yet exist to be constructed.
//! * **W7, the adjudication.** The `CourtCloseChunk` arm names what it does not do: a completing
//!   chunk "does not assemble, decode, check `close_digest`, run `check_close_cost_v2` /
//!   `adjudicate_court_close_v2` or apply the `CourtClosed` state machine". So `close_digest` is
//!   written and never read, and no shipped function says which digest it is — this tool computes
//!   the one the only shipped court-close domain admits, and W7 is what makes that answer binding
//!   rather than a guess.
//!
//! **So everything here is built and the send is gated, and the gate is the point.** The failure
//! this command exists to prevent is an operator under a court deadline paying for twenty-four
//! carriers and learning at the twenty-fourth that the group cannot assemble; filing into W6's
//! refusal is that failure with an extra step. What the command does instead is plan, price, cut,
//! digest and preview the whole carriage offline, refuse every limit it can check before the first
//! fee, and name the two consensus arms by the work item that owns them.

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_deadline::{
    PalwShippedCourtRowV1, palw_court_move_cost_daa_v1, palw_court_replay_positions_v1, palw_shipped_court_rows_v1,
};
use kaspa_consensus_core::palw_court_v2::{PALW_COURT_V2_ALL_DOMAINS, PalwCourtVerdictProofV2, check_close_cost_v2};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2, palw_close_bytes_for_chunks_v1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_COURT_CLOSE_CHUNK_MAX_BYTES, PALW_COURT_CLOSE_INCLUSION_MARGIN, PALW_COURT_CLOSE_MAX_CHUNKS, PalwConsensusObjectV2,
    PalwCourtSideV1, palw_close_assembly_daa_v1, palw_court_close_chunk_digest_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::path::{Path, PathBuf};

// =================================================================================================
// The seams: everything this tool needs from consensus and does not have
// =================================================================================================

/// **The one place that answers "may a court close ride in parts on this build".**
///
/// Today: no — and NOT for the reason this function used to give. Its previous body named the
/// generic `ObjectChunk` lane and asked for `CourtClosed` to be admitted into it. That design was
/// considered and rejected on the record (`palw_chunked_object_kind_admitted_v1`'s own doc): the
/// certification table's `TooManyPendingChunkGroups` is a delay for a drill and a dispute LOST for
/// a court, so putting the court's carriage there would let anyone renting eight slots defeat every
/// prosecution on the network. W5 built the close its OWN table instead, keyed `(session_id, side)`.
///
/// **That table is complete. What is missing is on either side of it**, and both halves are real
/// refusals rather than absences — filing into them costs the fee and carries nothing:
///
/// 1. **W6, the authentication.** `palw_v2_validate_objects`' `CourtCloseDeclared` arm returns an
///    error unconditionally, and a refused lifecycle object is DROPPED with the block standing. So
///    a declaration filed today pays its carrier, prints one `info!` line on every node, and opens
///    no group; each chunk behind it then fails `MissingCourtCloseGroup` and pays for that too.
/// 2. **W6, the message.** [`PALW_COURT_V2_ALL_DOMAINS`] carries no close-declaration signing
///    context, so there is not yet a message for the side's ML-DSA-87 key to bind. This is the half
///    this crate can READ, and [`close_declaration_context_v1`] is where it reads it: consensus
///    will register the domain when W6 lands, and that is what turns this seam green.
/// 3. **W7, the adjudication.** The `CourtCloseChunk` arm says what it does not do — a completing
///    chunk "does not assemble, decode, check `close_digest`, run `check_close_cost_v2` /
///    `adjudicate_court_close_v2` or apply the `CourtClosed` state machine". So `close_digest` is
///    written and never read, and no shipped function says which digest it IS. This tool computes
///    the one the only shipped court-close domain admits; until W7 reads it, that is a reading and
///    not an agreement, and filing on a reading is how a group assembles into a refusal.
///
/// Stated here in one place so the owning session can close it without reading this file, and so a
/// tool that cannot yet file a split close still says exactly what would let it.
pub(crate) fn court_close_chunked_carriage_v1() -> Result<(), CarriageGap> {
    if let Some(context) = close_declaration_context_v1() {
        // The domain landing is W6's own signal. It does not by itself prove the acceptance arm and
        // W7 landed with it, so this does not open the door — it says the door has a lock now, and
        // names the one file left to read. Deliberately not `Ok(())`: a tool that spends fees on
        // the strength of a constant's NAME has learned nothing from the gap list it replaced.
        return Err(CarriageGap {
            what: "a close-declaration signing context now ships, so W6 is landing and this seam is stale",
            rule: "PALW_COURT_V2_ALL_DOMAINS carries a close context — re-read palw_v2_validate_objects' CourtCloseDeclared arm",
            needs: &["misaka-cli/src/palw_court.rs: sign the declaration under the new context and drop this gate"],
            context: Some(context),
        });
    }
    Err(CarriageGap {
        what: "the acceptance layer drops every CourtCloseDeclared before a group can open",
        rule: "palw_v2_validate_objects: no layer yet verifies the declaring side's signature (ADR-0080 W6) — refused rather than trusted",
        needs: &[
            "palw_court_v2: a close-declaration signing context in PALW_COURT_V2_ALL_DOMAINS, and the message it binds (W6)",
            "virtual_processor::palw_v2_validate_objects, the CourtCloseDeclared arm: verify that signature instead of refusing (W6)",
            "palw_state_v2::apply_object, the CourtCloseChunk arm: assemble the completed group, check close_digest, decode and adjudicate (W7)",
        ],
        context: None,
    })
}

/// **Has consensus said yet what a close declaration signs?**
///
/// `misaka-cli` depends on `kaspa-consensus-core` and not on the consensus PIPELINE, so the arm
/// that actually refuses a declaration is not a symbol this crate can read. The signing domain is —
/// and the processor's own refusal names it as W6's other half ("under a signing domain registered
/// in `PALW_COURT_V2_ALL_DOMAINS`"), so the two land together. Probing the shipped domain list is
/// therefore a fact about CONSENSUS rather than about this file agreeing with itself, which is the
/// property a seam has to have to be worth keeping.
pub(crate) fn close_declaration_context_v1() -> Option<&'static [u8]> {
    PALW_COURT_V2_ALL_DOMAINS.iter().copied().find(|domain| contains_v1(domain, b"close"))
}

fn contains_v1(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|window| window == needle)
}

/// **What an operator is told when the split path is blocked.**
///
/// Its own function so a test can read it: this is the message that decides whether somebody under
/// a court deadline knows to escalate to the consensus owner or sits re-running the command, and a
/// message assembled inline in a branch nothing exercises is a message nobody has read.
pub(crate) fn blocked_message_v1(gap: &CarriageGap, carriers: usize) -> String {
    format!(
        "this close needs {carriers} carriers and {}.\n  {}\n  what is missing:\n    - {}\n  Nothing was carried and no fee was spent.",
        gap.what,
        gap.rule,
        gap.needs.join("\n    - ")
    )
}

/// What a carriage this build cannot perform is missing, in the words the owning session needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CarriageGap {
    pub what: &'static str,
    pub rule: &'static str,
    pub needs: &'static [&'static str],
    /// The close-declaration signing context, once one ships. `Some` means W6 is landing and this
    /// seam is the thing to re-read, not the thing to trust.
    pub context: Option<&'static [u8]>,
}

/// **How many carriers one court close may spend — from the ruleset that will judge it.**
///
/// Two ceilings, and the smaller one is the answer. `PalwCourtParamsV2::max_close_chunks` is the
/// network's: it is inside `palw_ruleset_id_v2`, it is what class admission prices a class against,
/// and it is 27 on the RC and **1 on devnet**, where the pre-ADR-0080 byte ceiling frames to a
/// single carrier. [`PALW_COURT_CLOSE_MAX_CHUNKS`] is the ROW's: `PalwCourtCloseGroupV2::present`
/// is a `u64` bitmap, so a count above it could not be represented whatever a ruleset said.
///
/// This used to answer `PALW_OBJECT_CHUNK_MAX_COUNT` and take its argument by `_`, because the
/// field did not exist. It exists; the argument is read.
pub(crate) fn court_close_max_parts_v1(court: &PalwCourtParamsV2) -> u8 {
    court.max_close_chunks().min(PALW_COURT_CLOSE_MAX_CHUNKS as u64) as u8
}

// =================================================================================================
// The plan
// =================================================================================================

/// One close's carriage, decided before a fee is spent.
#[derive(Debug)]
pub(crate) struct CarriagePlan {
    pub session_id: Hash64,
    /// Which of the two bonds the session id binds is moving. `None` when the close rides whole: a
    /// `CourtClosed` attributes itself to nobody, because nothing rides behind it to attribute.
    pub side: Option<PalwCourtSideV1>,
    /// The declaration, when the close is split — also `parts[0]`. Kept separately because every
    /// caller has to treat it differently: it is the only part that is signed, the only one that
    /// must land first, and the only one whose refusal strands the rest.
    pub declaration: Option<PalwConsensusObjectV2>,
    /// The objects to carry, in the order the chain must see them: the declaration, then the chunks
    /// in index order. One entry when the close rides whole.
    pub parts: Vec<PalwConsensusObjectV2>,
    /// The serialized close, before cutting.
    pub whole_bytes: usize,
    /// The chunks alone, without the declaration. Zero when the close rides whole. This is the
    /// number every consensus rule is denominated in — the ruleset's count, the row's bitmap and
    /// the assembly window all count CHUNKS — so it is spelled once and never re-derived from
    /// `parts.len()`, which is one larger.
    pub chunk_count: u8,
    /// `palw_court_close_chunk_digest_v1` of the concatenation — what the declaration pins as
    /// `close_digest`. See [`court_close_chunked_carriage_v1`]'s item 3: this is the only shipped
    /// court-close digest, and W7 is what makes it the right one.
    pub close_digest: Hash64,
}

impl CarriagePlan {
    /// The blocks this close spends of the mover's own turn — the `close_blocks` term
    /// [`palw_court_move_cost_daa_v1`] takes, and the reason it takes it. The declaration is one of
    /// them: it is a court move on a carrier of its own.
    pub fn close_blocks(&self) -> u64 {
        self.parts.len() as u64
    }

    /// The DAA the chain gives this group to finish arriving, from the block that carries the
    /// declaration — `PALW_COURT_CLOSE_INCLUSION_MARGIN` per chunk, read from consensus rather than
    /// multiplied here.
    pub fn assembly_daa(&self) -> u64 {
        palw_close_assembly_daa_v1(self.chunk_count)
    }

    /// **What a carriage journal is a journal FOR** — the chain's own key for this group, which is
    /// `(session_id, side)` and not a digest of the bytes. That is the whole reason the court has
    /// its own table (see [`court_close_chunked_carriage_v1`]): a free digest can be squatted, a
    /// side of a session cannot, and one declaration per side is all a session will ever accept.
    /// Empty for a close that rides whole, which has no group to resume into.
    pub fn journal_key(&self) -> String {
        match self.side {
            None => String::new(),
            Some(side) => format!("{}/{}", self.session_id, side_name_v1(side)),
        }
    }
}

/// The side, in the word `--side` takes, so the journal and the report cannot spell it two ways.
pub(crate) fn side_name_v1(side: PalwCourtSideV1) -> &'static str {
    match side {
        PalwCourtSideV1::Challenger => "challenger",
        PalwCourtSideV1::Executor => "executor",
    }
}

/// `--side`, refused by name rather than defaulted: filing one party's move under the other's bond
/// is not a typo the chain forgives.
pub(crate) fn parse_side_v1(text: &str) -> Result<PalwCourtSideV1, CliError> {
    match text.trim().to_ascii_lowercase().as_str() {
        "challenger" => Ok(PalwCourtSideV1::Challenger),
        "executor" => Ok(PalwCourtSideV1::Executor),
        other => Err(CliError::new(
            exit::GENERIC,
            format!("--side '{other}' is neither `challenger` nor `executor`, and a session binds exactly those two bonds"),
        )),
    }
}

/// **Cut the close the way the court's own table cuts it, or say it rides whole.**
///
/// Not [`palw_object_chunks_v1`]: that is the certification lane's cutter, it keys a group by a
/// free digest and it caps at `PALW_OBJECT_CHUNK_MAX_COUNT`, and a court close does not ride there
/// (see [`court_close_chunked_carriage_v1`]). The court's cut is by
/// [`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`], pinned by `palw_court_close_chunk_digest_v1` per index,
/// and bounded by [`court_close_max_parts_v1`].
///
/// **Every refusal it can make, it makes here** — before a plan exists to price, let alone fund.
/// The failure this file exists to prevent is a mover paying carrier after carrier into a group
/// that was never going to assemble, and a limit discovered at the last carrier is that failure.
pub(crate) fn plan_carriage_v1(
    object: &PalwConsensusObjectV2,
    court: &PalwCourtParamsV2,
    side: Option<PalwCourtSideV1>,
) -> Result<CarriagePlan, CliError> {
    let PalwConsensusObjectV2::CourtClosed { session_id, verdict, .. } = object else {
        return Err(CliError::new(exit::GENERIC, format!("{} is not a court close", object_kind_v1(object))));
    };
    let whole = borsh::to_vec(object).map_err(|e| CliError::new(exit::GENERIC, format!("this close does not serialize: {e}")))?;
    let whole_bytes = whole.len();
    let close_digest = palw_court_close_chunk_digest_v1(&whole);
    if whole_bytes <= PALW_COURT_CLOSE_CHUNK_MAX_BYTES {
        return Ok(CarriagePlan {
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
    let max = court_close_max_parts_v1(court);
    if count > max as usize {
        return Err(too_many_carriers_v1(court, whole_bytes, count, max));
    }
    // A `CourtClosed` names no side and a declaration cannot be built without one: the transition
    // reads the declarer from the session (challenger) or the claim (executor), so a tool that
    // guessed would file one party's move under the other party's bond. Refused rather than
    // defaulted — a default here is a forged move that the mover pays for.
    let Some(side) = side else {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "this close is {whole_bytes} bytes and needs {count} carriers, so it rides as a declaration and its chunks — and \
                 a declaration has to say WHICH of the session's two bonds is moving.\n  Pass --side challenger or --side \
                 executor. Nothing was carried and no fee was spent."
            ),
        ));
    };
    let chunks: Vec<Vec<u8>> = whole.chunks(PALW_COURT_CLOSE_CHUNK_MAX_BYTES).map(|part| part.to_vec()).collect();
    debug_assert_eq!(chunks.len(), count);
    // Belt to the count's braces: the transition refuses an empty or oversized chunk by name
    // (`CourtCloseChunkTooLarge`), and a cutter that could produce one is a cutter whose output has
    // to be trusted rather than checked.
    if let Some(bad) = chunks.iter().position(|part| part.is_empty() || part.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES) {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "chunk {bad} came out {} bytes, outside the 1..={PALW_COURT_CLOSE_CHUNK_MAX_BYTES} a carrier holds — this cut is \
                 wrong and nothing was carried",
                chunks[bad].len()
            ),
        ));
    }
    let chunk_digests: Vec<Hash64> = chunks.iter().map(|part| palw_court_close_chunk_digest_v1(part)).collect();
    let declaration = PalwConsensusObjectV2::CourtCloseDeclared {
        session_id: *session_id,
        side,
        count: count as u8,
        chunk_digests,
        close_digest,
        verdict: *verdict,
        // **Empty, and that is not an oversight — it is the gap, in the object.**
        // `palw_lifecycle_object_may_ride_v2` refuses a declaration with no signature, so this
        // object cannot be filed by accident. It cannot be signed either: there is no context to
        // sign under (see [`close_declaration_context_v1`]), and signing under an invented one
        // would produce a declaration that looks filed and binds nothing.
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
    Ok(CarriagePlan {
        session_id: *session_id,
        side: Some(side),
        declaration: Some(declaration),
        parts,
        whole_bytes,
        chunk_count: count as u8,
        close_digest,
    })
}

/// **Which ceiling stopped this close, and what would have fitted under it.**
///
/// Two different limits refuse the same way and an operator has to act differently on each: the
/// ruleset's is a fact about the NETWORK the dispute is on and the only remedy is a smaller close;
/// the row's is a fact about the state layout and no ruleset can raise it. Naming the wrong one
/// sends somebody to argue with the wrong file.
fn too_many_carriers_v1(court: &PalwCourtParamsV2, whole_bytes: usize, count: usize, max: u8) -> CliError {
    let ruleset_binds = court.max_close_chunks() <= PALW_COURT_CLOSE_MAX_CHUNKS as u64;
    let named = if ruleset_binds {
        "this network's court (PalwCourtParamsV2::max_close_chunks, inside the ruleset id)"
    } else {
        "the close group's own row (PALW_COURT_CLOSE_MAX_CHUNKS — `present` is a u64 bitmap)"
    };
    let carriable = max as usize * PALW_COURT_CLOSE_CHUNK_MAX_BYTES;
    // What a proof may weigh so its OBJECT still fits: the ruleset's own inverse, which is where
    // the framing allowance for the binding lives. Quoting it rather than deriving a second one is
    // the difference between an answer and an estimate.
    let counted = palw_close_bytes_for_chunks_v1(max as u64);
    CliError::new(
        exit::GENERIC,
        format!(
            "this close serializes to {whole_bytes} bytes — {count} carriers of {PALW_COURT_CLOSE_CHUNK_MAX_BYTES}, and {named} \
             admits at most {max}.\n  what would fit: {carriable} serialized bytes, which this ruleset frames from a proof of \
             {counted} counted bytes (its `max_close_bytes` is {}). You are {} bytes over.\n  Nothing was carried and no fee was \
             spent.",
            court.max_close_bytes(),
            whole_bytes.saturating_sub(carriable)
        ),
    )
}

/// **Will the chain give this group time to finish arriving?**
///
/// The declaration gate refuses a group that cannot assemble inside the session's BACKSTOP —
/// `declared_daa + palw_close_assembly_daa_v1(count) > session.deadline_daa` is
/// `CourtCloseCannotAssemble`, and the backstop never moves, because extending it would sell either
/// party a free window for the price of a declaration. Checked here against the same expression,
/// with the same consensus function, before the declaration is paid for.
///
/// It needs two numbers this tool cannot derive: where the chain is (a node's virtual DAA) and
/// where the session ends (`--deadline-at`, from the node's court log). With neither, there is no
/// honest answer and this is not called — an assembly check invented from a turn deadline would be
/// a second clock, and a mover would find out which one the chain kept at the last carrier.
pub(crate) fn check_assembly_window_v1(chunk_count: u8, now: u64, backstop: u64) -> Result<(), CliError> {
    let needed = palw_close_assembly_daa_v1(chunk_count);
    let finishes_at = now.saturating_add(needed);
    if finishes_at <= backstop {
        return Ok(());
    }
    let left = backstop.saturating_sub(now);
    let fits = left / PALW_COURT_CLOSE_INCLUSION_MARGIN;
    Err(CliError::new(
        exit::GENERIC,
        format!(
            "this group cannot assemble inside the session: {chunk_count} chunks need {needed} DAA \
             ({PALW_COURT_CLOSE_INCLUSION_MARGIN} per chunk), the chain is at {now} and the session's backstop is {backstop} — \
             {left} DAA left, so the declaration would be refused as CourtCloseCannotAssemble in the block that carried it.\n  \
             what would fit: {fits} chunks, about {} serialized bytes of close. Nothing was carried and no fee was spent.",
            fits as usize * PALW_COURT_CLOSE_CHUNK_MAX_BYTES
        ),
    ))
}

// =================================================================================================
// The deadline
// =================================================================================================

/// What a mover needs to know before it starts: the blocks the carriage costs, against the clock
/// the court runs it by.
pub(crate) struct DeadlineReport {
    /// The shipped row the floor was priced on, and whether it was chosen or assumed.
    pub row: &'static str,
    pub row_was_assumed: bool,
    pub replay_positions: u32,
    /// `palw_court_move_cost_daa_v1(row, replay_positions, close_blocks)` — one move, carriage
    /// included.
    pub move_cost_daa: u64,
    /// The same expression at `close_blocks = 1`: what the move would cost if the close fit one
    /// carrier. The difference is what splitting costs the mover.
    pub move_cost_daa_unsplit: u64,
    pub turn_deadline_daa: u64,
    pub close_blocks: u64,
}

impl DeadlineReport {
    /// The move fits inside one turn. False means the mover loses by the clock whatever it does,
    /// and it means it BEFORE the first fee is spent.
    pub fn fits_a_turn(&self) -> bool {
        self.move_cost_daa <= self.turn_deadline_daa
    }
}

/// **The mover's cost, from the one cost model this tree has.**
///
/// `palw_court_move_cost_daa_v1` takes `close_blocks` for exactly this case, and this function is
/// the caller it was written for — no second derivation, no flat "a block is a block" estimate.
/// The row is the class's own when the operator names one, and the WIDEST shipped row when they
/// do not: a floor derived from the widest row can only be too large, and a deadline report that
/// is optimistic is worse than none.
pub(crate) fn carriage_deadline_v1(
    court: &PalwCourtParamsV2,
    class_id: Option<Hash64>,
    close_blocks: u64,
) -> Result<DeadlineReport, CliError> {
    let rows = palw_shipped_court_rows_v1()
        .map_err(|e| CliError::new(exit::GENERIC, format!("the shipped court rows do not project: {e}")))?;
    let (row, assumed) = pick_row_v1(&rows, class_id)?;
    let replay_positions = palw_court_replay_positions_v1(&row.profile, row.checkpoint_interval);
    Ok(DeadlineReport {
        row: row.cost.row,
        row_was_assumed: assumed,
        replay_positions,
        move_cost_daa: palw_court_move_cost_daa_v1(&row.cost, replay_positions, close_blocks),
        move_cost_daa_unsplit: palw_court_move_cost_daa_v1(&row.cost, replay_positions, 1),
        turn_deadline_daa: court.turn_deadline_daa(),
        close_blocks,
    })
}

/// The named row, or the most expensive one. Split out because it is the only judgement in the
/// deadline path and it is the one a test should be able to pin.
pub(crate) fn pick_row_v1(
    rows: &[PalwShippedCourtRowV1],
    class_id: Option<Hash64>,
) -> Result<(&PalwShippedCourtRowV1, bool), CliError> {
    match class_id {
        Some(id) => rows.iter().find(|r| r.class_id == id).map(|r| (r, false)).ok_or_else(|| {
            CliError::new(
                exit::GENERIC,
                format!("no shipped court row prices class {id} — this build cannot say what a move on it costs"),
            )
        }),
        None => {
            // Widest = the largest replay, which is the largest floor: `palw_court_move_cost_daa_v1`
            // is monotone in `replay_positions`, so the row with the most positions is the row with
            // the highest cost and no second sort is needed.
            let widest = rows
                .iter()
                .max_by_key(|r| {
                    let positions = palw_court_replay_positions_v1(&r.profile, r.checkpoint_interval);
                    palw_court_move_cost_daa_v1(&r.cost, positions, 1)
                })
                .ok_or_else(|| CliError::new(exit::GENERIC, "this build ships no court rows".to_string()))?;
            Ok((widest, true))
        }
    }
}

// =================================================================================================
// Resume — grounded in what the chain remembers, not in what this tool wrote down
// =================================================================================================

/// **What this tool records between runs, and what it is NOT.**
///
/// It is an INDEX, not a source of truth: part index → the carrier that was submitted for it.
/// Every resume re-asks the chain whether those carriers exist, and a recorded carrier the chain
/// does not know is treated as never sent. That distinction is the whole design: a journal that
/// believed itself would skip a part whose carrier was orphaned and complete a group that can
/// never assemble.
///
/// **The chain's own answer would be better and it is not reachable.** `PalwChainStateV2` holds
/// `PalwCourtCloseGroupV2` — `present`, the arrival bitmap, beside `declared_daa` and
/// `assembly_deadline_daa` — and `court_close_group(&session, side)` reads it. No RPC exposes it:
/// `rpc/core/src/api/rpc.rs` has `get_palw_producer_facts`, `get_palw_derived_artifacts` and
/// `get_palw_free_prompt_claim`, and nothing for court sessions or their close groups. **The gap to
/// close is one RPC** (`GetPalwCourtCloseGroup { session, side } -> { count, declared_daa,
/// assembly_deadline_daa, present }`), after which this file's [`resume_point_v1`] becomes a
/// fallback rather than the mechanism — and, more to the point, after which this command could
/// check the two preflights it currently cannot: that the side's bond is the one the session binds,
/// and that no declaration for this `(session, side)` already exists.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct CarriageJournal {
    /// The chain's key for the group — `<session_id>/<side>`, from [`CarriagePlan::journal_key`].
    /// A journal whose key does not match the close on disk is a journal for another close, and is
    /// refused rather than reused.
    pub group: String,
    pub count: u8,
    pub network: String,
    /// The funding address the carriers were built from. Resume with another key cannot use them.
    pub address: String,
    pub parts: Vec<JournalPart>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct JournalPart {
    pub index: u8,
    /// The carrier this tool submitted for that part.
    pub txid: String,
    pub fee_sompi: u64,
}

/// **How far the chain says this group actually got.**
///
/// The evidence is the CHANGE each carrier pays back to the mover's own address: carrier `k`'s
/// output 0 funds carrier `k + 1`, so a carrier the chain accepted (or is holding in its mempool)
/// leaves `(txid_k, 0)` somewhere the UTXO index or the mempool can be asked about. Walking to the
/// HIGHEST recorded index that is still visible is what makes that sound when earlier changes have
/// already been spent by their successors — `(txid_0, 0)` is gone precisely BECAUSE part 1 landed.
///
/// A pure function of two chain-derived sets, so the walk is testable without a node — which is
/// the half of a resume that is usually only exercised by an outage.
pub(crate) fn resume_point_v1(parts: &[JournalPart], visible: &[TransactionOutpoint]) -> Option<(u8, TransactionOutpoint)> {
    let mut best: Option<(u8, TransactionOutpoint)> = None;
    for part in parts {
        let Ok(txid) = part.txid.parse::<TransactionId>() else { continue };
        let outpoint = TransactionOutpoint::new(txid, 0);
        if !visible.contains(&outpoint) {
            continue;
        }
        let higher = match best {
            None => true,
            Some((i, _)) => part.index >= i,
        };
        if higher {
            best = Some((part.index, outpoint));
        }
    }
    best
}

fn journal_path_v1(close: &Path, explicit: Option<&Path>) -> PathBuf {
    match explicit {
        Some(p) => p.to_path_buf(),
        None => {
            let mut p = close.as_os_str().to_os_string();
            p.push(".carriage.json");
            PathBuf::from(p)
        }
    }
}

// =================================================================================================
// The command
// =================================================================================================

/// Everything `palw court-close` takes, so the signature does not grow a seventh positional.
pub(crate) struct CourtCloseArgs<'a> {
    pub close: &'a Path,
    /// 128-hex class id of the class under dispute, so the deadline is priced on ITS row.
    pub class: Option<&'a str>,
    /// Which of the session's two bonds is declaring — `challenger` or `executor`. Only a SPLIT
    /// close needs it, and it needs it absolutely: a `CourtCloseDeclared` is attributed to one
    /// side, the transition reads that side's bond out of the session, and nothing in the
    /// `CourtClosed` file on disk says which one is moving.
    pub side: Option<&'a str>,
    /// The DAA this move's turn expires at, when the operator knows it (from the node's court
    /// log, or `session.deadline`). With it the report says whether the carriage fits the time
    /// that is LEFT rather than the time a turn is worth.
    pub deadline_at: Option<u64>,
    pub state: Option<&'a Path>,
    pub restart: bool,
    /// Plan and price the move from `--network`'s own preset, without a node. Everything up to
    /// the first byte of funding — the cost rule, the cut, the deadline — is a pure function of
    /// the close and the ruleset, and an operator deciding whether they will make their turn
    /// should not need a synced node to find out.
    pub offline: bool,
    pub yes: bool,
}

/// **File a court close, split across carriers when it does not fit one.**
pub(crate) async fn court_close(ctx: &Ctx, ks: &KeySource, args: CourtCloseArgs<'_>) -> Result<(), CliError> {
    // ---- 1. the object -------------------------------------------------------------------------
    let bytes = std::fs::read(args.close).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", args.close.display())))?;
    let object: PalwConsensusObjectV2 = borsh::from_slice(&bytes)
        .map_err(|e| CliError::new(exit::GENERIC, format!("{} is not a borsh consensus object: {e}", args.close.display())))?;
    let (session_id, verdict, proof) = match &object {
        PalwConsensusObjectV2::CourtClosed { session_id, verdict, proof } => (*session_id, format!("{verdict:?}"), proof),
        other => {
            return Err(CliError::new(
                exit::GENERIC,
                format!(
                    "{} carries a {} — this command files court closes. `palw submit-object` carries the rest.",
                    args.close.display(),
                    object_kind_v1(other)
                ),
            ));
        }
    };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
        .map_err(|why| CliError::new(exit::GENERIC, format!("{}: {why}", args.close.display())))?;

    // ---- 2. the network, and the two ceilings --------------------------------------------------
    if args.offline && args.yes {
        return Err(CliError::new(
            exit::GENERIC,
            "--offline plans a move; it cannot file one. Drop --yes, or drop --offline.".to_string(),
        ));
    }
    let nv = if args.offline { None } else { Some(connect(ctx).await?) };
    let params = match &nv {
        Some(nv) => nv.params.clone(),
        // The preset the CLI's own `--network` names. It is the same table a node boots from, so
        // the ruleset this prices against is the one that will judge the close — and when it is
        // not, the node run without `--offline` says so.
        None => {
            let id = ctx
                .network
                .parse::<kaspa_consensus_core::network::NetworkId>()
                .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
            kaspa_consensus_core::config::params::Params::from(id)
        }
    };
    let court = court_params_v1(&params)?;
    // **The cost ceiling first, because no carriage repairs it.** A close over `max_close_bytes`
    // is refused on the merits by every node; splitting it only spends more fees before the same
    // refusal. Checked here for the same reason `submit-object` grades a `FamilyCertified` before
    // spending: a refused object is a dropped carrier, the fee gone and nothing recorded.
    if let Err(why) = check_close_cost_v2(proof, &court) {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "this close is refused by the court's own cost rule, so carriage is not the problem: {why}\n  \
                 the ceiling is this network's `max_close_bytes` ({}) and it is inside the ruleset id — a close over it \n  \
                 cannot be filed at all, split or whole. Nothing was carried.",
                court.max_close_bytes()
            ),
        ));
    }
    let side = match args.side {
        Some(text) => Some(parse_side_v1(text)?),
        None => None,
    };
    let plan = plan_carriage_v1(&object, &court, side)?;
    let class_id = match args.class {
        Some(text) => Some(
            text.trim().parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("class id '{text}' is not 128-hex")))?,
        ),
        None => None,
    };
    let deadline = carriage_deadline_v1(&court, class_id, plan.close_blocks())?;

    // ---- 3. the gap ----------------------------------------------------------------------------
    // A split close is refused by the transition that assembles it on this build. Said BEFORE the
    // dry-run preview, so an operator reading the preview is not reading a plan that cannot run.
    let gap = if plan.declaration.is_some() { court_close_chunked_carriage_v1().err() } else { None };
    // **The assembly window, against the two numbers only a node holds.** The declaration gate
    // compares `now + palw_close_assembly_daa_v1(count)` with the session's backstop and refuses in
    // the block that carries it, so a mover who finds this out on chain has lost the declaration's
    // fee and the rung it suspended. Checked with the same function, before either.
    if plan.chunk_count > 0
        && let (Some(nv), Some(backstop)) = (&nv, args.deadline_at)
    {
        check_assembly_window_v1(plan.chunk_count, nv.virtual_daa, backstop)?;
    }

    // ---- 4. report -----------------------------------------------------------------------------
    let text = ctx.output != OutputFormat::Json;
    if text {
        println!("court close for session {session_id}");
        println!("  verdict {verdict}, proof {}", proof_kind_v1(proof));
        match plan.side {
            None => println!(
                "  {} bytes — one carrier as a CourtClosed (the limit is {PALW_COURT_CLOSE_CHUNK_MAX_BYTES})",
                plan.whole_bytes
            ),
            Some(side) => {
                println!(
                    "  {} bytes — a CourtCloseDeclared on the {} side plus {} chunks of at most \
                     {PALW_COURT_CLOSE_CHUNK_MAX_BYTES}, close digest {}",
                    plan.whole_bytes,
                    side_name_v1(side),
                    plan.chunk_count,
                    plan.close_digest
                );
                println!(
                    "  the group is keyed ({}, {}) and one declaration per side is all the session will ever accept; it has {} \
                     DAA from the declaring block to finish arriving",
                    plan.session_id,
                    side_name_v1(side),
                    plan.assembly_daa()
                );
                println!(
                    "  the ruleset admits {} carriers per close and the row can address {PALW_COURT_CLOSE_MAX_CHUNKS}",
                    court.max_close_chunks()
                );
            }
        }
        // The court checks each proof arm in its own units, and only the arithmetic arm has a
        // public byte count. Saying "n/a" where there is no number is better than printing one
        // this tool computed itself: a second cost model is how a close gets called admissible by
        // a tool and refused by a node.
        match kaspa_consensus_core::palw_court_v2::arithmetic_close_bytes_v2(proof) {
            Some(bytes) => {
                println!("  close cost {bytes} bytes against max_close_bytes {} — the court admits it", court.max_close_bytes())
            }
            None => println!(
                "  the {} arm's own cost check passes against max_close_bytes {} — the court admits it",
                proof_kind_v1(proof),
                court.max_close_bytes()
            ),
        }
        let assumed = if deadline.row_was_assumed { " (assumed — the widest shipped row; name --class for this one)" } else { "" };
        println!(
            "  deadline: {} block(s) of carriage; one move on {}{assumed} ({} positions of honest replay) costs {} DAA \
             ({} unsplit) against a turn deadline of {}",
            deadline.close_blocks,
            deadline.row,
            deadline.replay_positions,
            deadline.move_cost_daa,
            deadline.move_cost_daa_unsplit,
            deadline.turn_deadline_daa
        );
        if !deadline.fits_a_turn() {
            println!(
                "  THE CLOCK REFUSES THIS MOVE: {} DAA of move against a {} DAA turn. Filing it loses the turn.",
                deadline.move_cost_daa, deadline.turn_deadline_daa
            );
        }
        if args.deadline_at.is_some() && nv.is_none() {
            println!("  --deadline-at needs the chain's own DAA score to answer, and --offline has no node to ask.");
        }
        if let Some(at) = args.deadline_at.filter(|_| nv.is_some()) {
            let now = nv.as_ref().expect("filtered on Some").virtual_daa;
            let left = at.saturating_sub(now);
            println!(
                "  the session's turn expires at DAA {at}; the chain is at {now}, so {left} DAA are left and this carriage needs \
                 about {}",
                deadline.close_blocks
            );
            if left < deadline.close_blocks {
                println!("  THERE IS NOT ENOUGH TIME: the carriage alone outlasts the turn.");
            }
        }
    }

    if let Some(gap) = gap {
        let message = blocked_message_v1(&gap, plan.parts.len());
        if ctx.output == OutputFormat::Json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": false, "session": session_id.to_string(), "carriers": plan.parts.len(),
                    "whole_bytes": plan.whole_bytes, "group": plan.journal_key(),
                    "chunks": plan.chunk_count, "side": plan.side.map(side_name_v1),
                    "blocked": { "what": gap.what, "rule": gap.rule, "needs": gap.needs },
                })
            );
        }
        if let Some(nv) = &nv {
            let _ = nv.client.disconnect().await;
        }
        return Err(CliError::new(exit::GENERIC, message));
    }

    // ---- 5. resume ------------------------------------------------------------------------------
    let Some(nv) = nv else {
        // Offline: everything above is a pure function of the close and the ruleset, and nothing
        // below can be answered without asking a node what this key can spend.
        if ctx.output == OutputFormat::Json {
            println!(
                "{}",
                serde_json::json!({
                    "offline": true, "session": session_id.to_string(), "verdict": verdict,
                    "whole_bytes": plan.whole_bytes, "carriers": plan.parts.len(),
                    "chunks": plan.chunk_count, "side": plan.side.map(side_name_v1),
                    "close_digest": plan.close_digest.to_string(), "assembly_daa": plan.assembly_daa(),
                    "group": plan.journal_key(), "close_blocks": plan.close_blocks(),
                    "move_cost_daa": deadline.move_cost_daa, "move_cost_daa_unsplit": deadline.move_cost_daa_unsplit,
                    "turn_deadline_daa": deadline.turn_deadline_daa, "fits_a_turn": deadline.fits_a_turn(),
                    "row": deadline.row, "row_was_assumed": deadline.row_was_assumed,
                })
            );
        } else {
            println!("offline — planned and priced against the {} preset. Nothing was funded or sent.", ctx.network);
        }
        return Ok(());
    };
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());
    let spendable = crate::palw_fp::spendable_candidates_v1(&nv, &addr).await?;
    let journal_file = journal_path_v1(args.close, args.state);
    let mut journal = load_journal_v1(&journal_file, &plan.journal_key(), &ctx.network, &addr.to_string(), args.restart)?;
    let visible: Vec<TransactionOutpoint> = spendable.iter().map(|(o, _)| *o).collect();
    let resumed = journal.as_ref().and_then(|j| resume_point_v1(&j.parts, &visible));

    let (mut funding_outpoint, mut funding_entry, first_part) = match resumed {
        Some((index, outpoint)) => {
            let entry = spendable
                .iter()
                .find(|(o, _)| *o == outpoint)
                .map(|(_, e)| e.clone())
                .expect("resume_point_v1 only returns an outpoint it found in `visible`");
            if text {
                println!(
                    "  resuming: the chain still holds the change of part {} of {} — parts 1..={} are carried, {} to go",
                    index + 1,
                    plan.parts.len(),
                    index + 1,
                    plan.parts.len().saturating_sub(index as usize + 1)
                );
            }
            (outpoint, entry, index as usize + 1)
        }
        None => {
            if journal.is_some() && text {
                println!(
                    "  a carriage journal exists and the chain knows none of its carriers — treating every part as unsent. If \
                     parts DID land, the chain refuses the duplicates by name and nothing is lost but their fees."
                );
            }
            let (outpoint, entry) = spendable.first().cloned().ok_or_else(|| {
                CliError::new(exit::GENERIC, format!("no mature, unbonded, unspent UTXO at {addr} to fund the carrier"))
            })?;
            (outpoint, entry, 0usize)
        }
    };
    if first_part >= plan.parts.len() {
        if text {
            println!("every part of this close is already carried. Nothing to do.");
        }
        let _ = nv.client.disconnect().await;
        return Ok(());
    }

    // ---- 6. build every remaining carrier, chained, before sending one --------------------------
    let mut carriers = Vec::with_capacity(plan.parts.len() - first_part);
    for (offset, part) in plan.parts.iter().enumerate().skip(first_part) {
        let (tx, fee) = crate::palw_fp::build_carrier_v1(&key, &nv, part, funding_outpoint, &funding_entry)
            .map_err(|e| CliError::new(exit::GENERIC, format!("part {} of {}: {e}", offset + 1, plan.parts.len())))?;
        let change = tx.outputs.first().ok_or_else(|| CliError::new(exit::GENERIC, "the carrier has no change output".to_string()))?;
        funding_outpoint = TransactionOutpoint::new(tx.id(), 0);
        funding_entry = UtxoEntry::new(change.value, change.script_public_key.clone(), 0, false);
        carriers.push((offset, tx, fee));
    }

    if !args.yes {
        match ctx.output {
            OutputFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "dry_run": true, "session": session_id.to_string(), "verdict": verdict,
                    "whole_bytes": plan.whole_bytes, "group": plan.journal_key(),
                    "chunks": plan.chunk_count, "side": plan.side.map(side_name_v1),
                    "close_blocks": plan.close_blocks(), "move_cost_daa": deadline.move_cost_daa,
                    "turn_deadline_daa": deadline.turn_deadline_daa, "fits_a_turn": deadline.fits_a_turn(),
                    "carriers": carriers.iter().map(|(i, tx, fee)| serde_json::json!({
                        "part": i + 1, "of": plan.parts.len(), "txid": tx.id().to_string(),
                        "payload_bytes": tx.payload.len(), "fee_sompi": fee,
                    })).collect::<Vec<_>>(),
                })
            ),
            _ => {
                for (i, tx, fee) in &carriers {
                    println!(
                        "  part {}/{}: carrier {} ({} payload bytes, fee {fee} sompi)",
                        i + 1,
                        plan.parts.len(),
                        tx.id(),
                        tx.payload.len()
                    );
                }
                println!("dry run — nothing was sent. Re-run with --yes to file the close.");
            }
        }
        let _ = nv.client.disconnect().await;
        return Ok(());
    }

    // ---- 7. send, naming the stage on failure ---------------------------------------------------
    let total = plan.parts.len();
    let mut sent = Vec::new();
    for (offset, tx, fee) in &carriers {
        let part_no = offset + 1;
        // The journal is written BEFORE the send, not after: a carrier that reaches the node and
        // whose response is lost is exactly the case a resume must not forget. A recorded carrier
        // the chain never saw costs one re-send; an unrecorded one that landed costs a duplicate
        // the chain refuses by name and a whole group that can never complete.
        record_part_v1(&mut journal, &journal_file, &plan, &ctx.network, &addr.to_string(), *offset as u8, tx.id(), *fee)?;
        if let Err(e) = nv.client.submit_transaction(tx.as_ref().into(), false).await {
            let why = e.to_string();
            let advice = if why.contains("already spent") {
                "the funding UTXO is spent by a transaction still in the mempool — wait for a block and re-run; this command \
                 resumes from the parts that landed"
            } else if why.contains("insufficient") || why.contains("fee") {
                "re-fund the key and re-run; the parts already carried are not re-paid for"
            } else {
                "read the node's reason before re-running — a refused part leaves the group half-assembled until its TTL"
            };
            let _ = nv.client.disconnect().await;
            return Err(CliError::new(
                exit::TX_REJECTED,
                format!(
                    "part {part_no} of {total} refused: {why}\n  parts 1..{} are carried; {advice}.\n  the carriage journal is {}",
                    part_no.saturating_sub(1),
                    journal_file.display()
                ),
            ));
        }
        if text {
            println!("  part {part_no}/{total} carried by {} (fee {fee} sompi)", tx.id());
        }
        sent.push(serde_json::json!({ "part": part_no, "of": total, "txid": tx.id().to_string(), "fee_sompi": fee }));
    }

    match ctx.output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({ "ok": true, "session": session_id.to_string(), "carriers": sent, "complete": true }))
        }
        _ => {
            println!("the close is filed: {total} of {total} parts carried.");
            if plan.side.is_some() {
                println!(
                    "  the declaration opened the group and the chunks fill its `present` bitmap; the chain assembles it in the \
                     block that completes it."
                );
            } else {
                println!("  the chain adjudicates it when the carrier is accepted; a verdict the proof does not derive is refused.");
            }
        }
    }
    let _ = std::fs::remove_file(&journal_file);
    let _ = nv.client.disconnect().await;
    Ok(())
}

// =================================================================================================
// Helpers
// =================================================================================================

fn court_params_v1(params: &kaspa_consensus_core::config::params::Params) -> Result<PalwCourtParamsV2, CliError> {
    match &params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => Ok(bundle.court),
        _ => Err(CliError::new(
            exit::GENERIC,
            "this network's params carry no PALW V2 bundle, so it has no court — nothing here can be filed".to_string(),
        )),
    }
}

fn object_kind_v1(object: &PalwConsensusObjectV2) -> &'static str {
    match object {
        PalwConsensusObjectV2::CourtClosed { .. } => "CourtClosed",
        PalwConsensusObjectV2::CourtOpened { .. } => "CourtOpened",
        PalwConsensusObjectV2::CourtDisclosed { .. } => "CourtDisclosed",
        PalwConsensusObjectV2::CourtVerdictPosted { .. } => "CourtVerdictPosted",
        PalwConsensusObjectV2::FamilyCertified { .. } => "FamilyCertified",
        PalwConsensusObjectV2::ClassLaneCertified { .. } => "ClassLaneCertified",
        PalwConsensusObjectV2::ObjectChunk { .. } => "ObjectChunk",
        // The two ADR-0080 kinds this file MAKES. Naming them matters in the one message that
        // reads a file off disk: an operator who fed the tool a declaration it wrote earlier is
        // told what it is, not that it is "of another kind".
        PalwConsensusObjectV2::CourtCloseDeclared { .. } => "CourtCloseDeclared",
        PalwConsensusObjectV2::CourtCloseChunk { .. } => "CourtCloseChunk",
        _ => "lifecycle object of another kind",
    }
}

fn proof_kind_v1(proof: &PalwCourtVerdictProofV2) -> &'static str {
    match proof {
        PalwCourtVerdictProofV2::Arithmetic { .. } => "Arithmetic",
        PalwCourtVerdictProofV2::DecodeToken { .. } => "DecodeToken",
        PalwCourtVerdictProofV2::DecodeTokenTiled { .. } => "DecodeTokenTiled",
    }
}

fn load_journal_v1(
    path: &Path,
    group: &str,
    network: &str,
    address: &str,
    restart: bool,
) -> Result<Option<CarriageJournal>, CliError> {
    if restart {
        let _ = std::fs::remove_file(path);
        return Ok(None);
    }
    let Ok(text) = std::fs::read_to_string(path) else { return Ok(None) };
    let journal: CarriageJournal = serde_json::from_str(&text)
        .map_err(|e| CliError::new(exit::GENERIC, format!("{} is not a carriage journal: {e}", path.display())))?;
    // A journal for another close, another network or another key cannot be resumed from, and
    // silently ignoring it would re-send parts that already landed. Named instead.
    let want = group;
    if journal.group != want || journal.network != network || journal.address != address {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "{} is a carriage journal for group {} on {} at {} — this close is group {want} on {network} at {address}. \
                 Pass --state for a different journal, or --restart to discard it.",
                path.display(),
                journal.group,
                journal.network,
                journal.address
            ),
        ));
    }
    Ok(Some(journal))
}

fn record_part_v1(
    journal: &mut Option<CarriageJournal>,
    path: &Path,
    plan: &CarriagePlan,
    network: &str,
    address: &str,
    index: u8,
    txid: TransactionId,
    fee: u64,
) -> Result<(), CliError> {
    let j = journal.get_or_insert_with(|| CarriageJournal {
        group: plan.journal_key(),
        count: plan.parts.len() as u8,
        network: network.to_string(),
        address: address.to_string(),
        parts: Vec::new(),
    });
    j.parts.retain(|p| p.index != index);
    j.parts.push(JournalPart { index, txid: txid.to_string(), fee_sompi: fee });
    j.parts.sort_by_key(|p| p.index);
    let text =
        serde_json::to_string_pretty(j).map_err(|e| CliError::new(exit::GENERIC, format!("encode the carriage journal: {e}")))?;
    // Staged then renamed, the retention discipline this tree already uses: a reader never sees a
    // half-written journal, and a crash mid-write does not lose the parts already recorded.
    let mut staged = path.as_os_str().to_os_string();
    staged.push(".partial");
    let staged = PathBuf::from(staged);
    std::fs::write(&staged, text).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", staged.display())))?;
    std::fs::rename(&staged, path).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::{Params, devnet_shipped_params, palw_rc_shipped_params};
    use kaspa_consensus_core::palw_court_v2::arithmetic_close_bytes_v2;
    use kaspa_consensus_core::palw_legs::{PALW_LEGS_OBJECT_VERSION_V1, PalwCheckpointProfileV1};
    use kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2;
    use kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
    use kaspa_consensus_core::palw_state_v2::palw_chunked_object_kind_admitted_v1;
    use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
    use kaspa_consensus_core::palw_step_leg::{PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2};
    use kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1;
    use kaspa_consensus_core::palw_v2::PalwJobContextV2;

    fn job_context() -> PalwJobContextV2 {
        PalwJobContextV2 {
            version: 1,
            network_id: b"test".to_vec(),
            job_id: Hash64::default(),
            job_nullifier: Hash64::default(),
            assignment_id: Hash64::default(),
            execution_seed: [0u8; 32],
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: Hash64::default(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: 0,
            exact_decode_tokens: 0,
            max_context_tokens: 0,
        }
    }

    /// A close whose PROOF PAYLOAD is as small as a close can be: an empty decode-token pin.
    /// Everything it weighs is the binding.
    pub(super) fn close_with_binding(profile: &PalwShapeProfileV3, rows: usize, lanes: usize) -> PalwConsensusObjectV2 {
        let binding = PalwStepBindingV2 {
            version: PALW_STEP_LEG_OBJECT_VERSION_V1,
            job_context: job_context(),
            shape_profile: profile.clone(),
            checkpoint_profile: PalwCheckpointProfileV1 {
                version: PALW_LEGS_OBJECT_VERSION_V1,
                checkpoint_interval: 1,
                state_layout_id: Hash64::default(),
            },
            state_chunk_map_id: Hash64::default(),
            full_logits_trace_root: Hash64::default(),
            activation_leg_root: Hash64::default(),
            step_leaf_count: 0,
            step_merkle_root: Hash64::default(),
            checkpoint_count: 0,
            checkpoint_merkle_root: Hash64::default(),
            committed_execution_root: Hash64::default(),
        };
        PalwConsensusObjectV2::CourtClosed {
            session_id: Hash64::default(),
            verdict: kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty,
            proof: PalwCourtVerdictProofV2::DecodeToken {
                binding,
                pin: PalwBase0DecodeTokensV1 { logits_rows: vec![vec![0i32; lanes]; rows], generated_token_ids: Vec::new() },
                position: 0,
            },
        }
    }

    /// **The measurement this whole command rests on: what a close weighs that nobody charges it
    /// for.**
    ///
    /// The court charges a close for its PROOF payload (`max_close_bytes`). The mempool charges
    /// its carrier for the SERIALIZED OBJECT, which additionally carries `PalwStepBindingV2` — and
    /// that carries the class's whole `PalwShapeProfileV3`, node tables and all, because
    /// adjudication needs them. So the binding is the untolled part of a carrier: free to the
    /// court, paid to the mempool.
    ///
    /// Measured on a close whose proof payload is EMPTY — zero openings, zero logits rows, zero
    /// ids — so every byte counted here is the binding and nothing else. The numbers are pinned as
    /// an upper bound rather than as equalities: a profile that GROWS eats into the carriers
    /// [`the_headroom_between_the_two_ceilings_is_measured_not_assumed`] counts, and that is the
    /// event this assertion exists to catch. W5 did not retire it — it made it the ONLY term the
    /// ruleset's framing allowance does not already price.
    #[test]
    fn the_binding_is_the_untolled_part_of_a_carrier() {
        let rows = palw_shipped_court_rows_v1().expect("the shipped rows project");
        assert!(!rows.is_empty(), "this build ships no court rows, so this measures nothing");
        let mut widest = 0usize;
        let mut widest_row = "";
        for row in &rows {
            let object = close_with_binding(&row.profile, 0, 0);
            let bytes = borsh::to_vec(&object).expect("a close serializes").len();
            // The proof payload is empty by construction; the whole weight is the binding.
            println!("{}: an EMPTY close serializes to {bytes} bytes of carriage", row.cost.row);
            if bytes > widest {
                widest = bytes;
                widest_row = row.cost.row;
            }
        }
        println!("widest shipped row: {widest_row} at {widest} bytes, against a {PALW_COURT_CLOSE_CHUNK_MAX_BYTES}-byte carrier");
        assert!(widest > 0, "an empty close weighed nothing — the binding stopped riding");
        assert!(
            widest <= WIDEST_SHIPPED_BINDING_BYTES,
            "{widest_row}'s binding grew to {widest} bytes, past the {WIDEST_SHIPPED_BINDING_BYTES} this file's carriage \
             arithmetic is written against — re-measure it before shipping"
        );
    }

    /// **The two ceilings, and the room between them in the unit W5 moved it into.**
    ///
    /// The previous version of this test asserted `max_close_bytes + binding <= one carrier`, and
    /// pinned the remainder at 4,084 bytes. That property was not wrong; it was RETIRED, by a
    /// consensus change that deliberately destroyed it — ADR-0080's W5 stopped choosing
    /// `max_close_bytes` and started deriving it from `max_close_chunks`, which is 27 carriers on
    /// the RC. Re-asserting the old inequality would be asserting that W5 is not in the build.
    ///
    /// **Its successor is the same question in carriers.** A legal close is at most
    /// `max_close_bytes` of proof; its carriage is that plus the binding, which the cost rule does
    /// not count; the invariant is that the cut this tool performs produces carriers that each FIT
    /// and few enough of them that the ruleset will pay. If that ever fails, a close every node
    /// would ADMIT becomes one no operator can FILE — the same failure the old inequality guarded,
    /// stated in the unit the ruleset now budgets in.
    ///
    /// And it is asserted twice on purpose: once as arithmetic over the two consensus constants,
    /// and once over the bytes [`plan_carriage_v1`] actually emits for a close built AT the
    /// ceiling. An arithmetic that the cutter does not reproduce is the module doc's old table
    /// again, one refactor later.
    #[test]
    fn the_headroom_between_the_two_ceilings_is_measured_not_assumed() {
        let court = shipped_court();
        let largest_legal_carriage = court.max_close_bytes() as usize + WIDEST_SHIPPED_BINDING_BYTES;
        let carriers = largest_legal_carriage.div_ceil(PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
        println!(
            "max_close_bytes {} + widest binding {WIDEST_SHIPPED_BINDING_BYTES} = {largest_legal_carriage}, which is {carriers} \
             carriers of {PALW_COURT_CLOSE_CHUNK_MAX_BYTES}; this ruleset pays for {} and the row can address \
             {PALW_COURT_CLOSE_MAX_CHUNKS}",
            court.max_close_bytes(),
            court.max_close_chunks()
        );
        assert!(
            carriers > 1,
            "a legal close fits one carrier again ({largest_legal_carriage} bytes): W5's ceiling is not in this build, and the \
             split path this file drives has gone dormant — re-read the module doc before trusting it"
        );
        assert!(
            carriers as u64 <= court.max_close_chunks(),
            "a close this network ADMITS needs {carriers} carriers and the network pays for {}: the cost ceiling and the \
             carriage count have come apart, and every legal close at the top of the range is unfilable",
            court.max_close_chunks()
        );
        assert!(
            carriers <= PALW_COURT_CLOSE_MAX_CHUNKS as usize,
            "{carriers} carriers cannot be addressed by PalwCourtCloseGroupV2::present, which is a u64 bitmap capped at \
             {PALW_COURT_CLOSE_MAX_CHUNKS}"
        );

        // The arithmetic is a claim about the cutter; here is the cutter.
        let object = largest_admissible_close_v1(&court, &widest_profile());
        let whole = borsh::to_vec(&object).expect("the widest legal close serializes");
        let plan = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Executor)).expect("the widest legal close plans");
        println!("the widest legal close carries {} bytes and cuts into {} chunks", whole.len(), plan.chunk_count);
        assert_eq!(
            plan.chunk_count as usize, carriers,
            "the cut and the arithmetic disagree — one of them is describing a build the other has left"
        );
        assert_carriers_fit_and_reassemble_v1(&plan, &whole);
        println!(
            "headroom: {} carriers of the {} this ruleset pays for",
            court.max_close_chunks() - plan.chunk_count as u64,
            court.max_close_chunks()
        );
    }

    /// The widest binding any shipped row produces, measured by
    /// [`the_binding_is_the_untolled_part_of_a_carrier`] and used by the carriage arithmetic. One
    /// spelling, so the two tests cannot drift.
    const WIDEST_SHIPPED_BINDING_BYTES: usize = 13_996;

    /// **Every legal close on every shipped ruleset can be carried** — by one carrier, or by a
    /// legal number of chunks.
    ///
    /// This replaces `no_legal_close_can_need_splitting_on_a_shipped_ruleset`, which asserted that
    /// the split path was DORMANT: that the densest payload a close carries rises in carriage bytes
    /// no faster than the court charges for it, so a close grown until the court refuses it still
    /// fits one carrier. That was true and W5 ended it, by raising the ceiling twenty-three
    /// carriers past one. Deleting the test would have left the tool's central promise unasserted;
    /// keeping it would have asserted a fact about a build nobody runs.
    ///
    /// **The successor is the property the old one was a special case of.** "Fits one carrier" was
    /// only ever interesting because it meant "can be filed". So: for every court this build ships,
    /// build the widest close that court will ADMIT, cut it, and require that it can be carried —
    /// one carrier if it fits one, otherwise chunks that each fit and are few enough for both the
    /// ruleset's count and the group row's bitmap. The two shipped courts answer differently and
    /// both answers are checked: devnet's ceiling still frames to a single carrier, the RC's to 27.
    ///
    /// A close the court admits and no operator can file is the failure this whole file exists to
    /// prevent, and this is the assertion that it has not happened.
    #[test]
    fn every_legal_close_can_be_carried_on_every_shipped_ruleset() {
        let mut seen = 0usize;
        for (name, params) in shipped_rulesets_v1() {
            let Some(court) = court_of_v1(&params) else { continue };
            seen += 1;
            let object = largest_admissible_close_v1(&court, &widest_profile());
            let whole = borsh::to_vec(&object).expect("the widest legal close serializes");
            let plan = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Challenger)).unwrap_or_else(|e| {
                panic!(
                    "{name}: the widest close its OWN court admits ({} carried bytes) cannot be carried on it: {}",
                    whole.len(),
                    e.msg
                )
            });
            println!(
                "{name}: max_close_bytes {} → {} carried bytes → {} chunk(s); the ruleset pays for {}",
                court.max_close_bytes(),
                whole.len(),
                plan.chunk_count,
                court.max_close_chunks()
            );
            match plan.side {
                // One carrier. The declaration is not paid for and no group is opened, which is the
                // reason `CourtClosed` deliberately still exists.
                None => {
                    assert_eq!(plan.parts.len(), 1, "{name}: a whole close is one part");
                    assert!(plan.declaration.is_none(), "{name}: a whole close declared a group");
                    assert!(
                        whole.len() <= PALW_COURT_CLOSE_CHUNK_MAX_BYTES,
                        "{name}: a close that rides whole is {} bytes, over one carrier",
                        whole.len()
                    );
                }
                Some(_) => {
                    assert!(plan.chunk_count >= 2, "{name}: a split close cut into {} chunks", plan.chunk_count);
                    assert!(
                        plan.chunk_count as u64 <= court.max_close_chunks(),
                        "{name}: {} chunks against a ruleset that pays for {}",
                        plan.chunk_count,
                        court.max_close_chunks()
                    );
                    assert!(
                        plan.chunk_count <= PALW_COURT_CLOSE_MAX_CHUNKS,
                        "{name}: {} chunks cannot be addressed by the group's u64 bitmap",
                        plan.chunk_count
                    );
                }
            }
            assert_carriers_fit_and_reassemble_v1(&plan, &whole);
        }
        assert!(seen >= 2, "only {seen} shipped ruleset(s) carry a court — this measured almost nothing");
    }

    /// **Every carrier fits, the parts are the close, and each part is the one the declaration
    /// pinned.**
    ///
    /// The chain's own three refusals, applied to this tool's output before it is paid for:
    /// `CourtCloseChunkTooLarge` on a part outside `1..=PALW_COURT_CLOSE_CHUNK_MAX_BYTES`,
    /// `CourtCloseChunkDigestMismatch` on a part that is not the pinned preimage of its index, and
    /// `CourtCloseIndexOutOfRange` on an index past the declared count. Asserted against the same
    /// consensus functions, not against a re-implementation.
    ///
    /// The bound is on the chunk's BYTES and not on the serialized `CourtCloseChunk`: the
    /// transition measures `bytes.len()`, and the ~70 bytes of session id, side and index that ride
    /// with it are what `PALW_COURT_CLOSE_CHUNK_MAX_BYTES` already leaves room for inside the
    /// 120,000-byte standard transaction it was derived from.
    fn assert_carriers_fit_and_reassemble_v1(plan: &CarriagePlan, whole: &[u8]) {
        let mut reassembled: Vec<u8> = Vec::new();
        let mut pinned: Vec<Hash64> = Vec::new();
        for (position, part) in plan.parts.iter().enumerate() {
            match part {
                PalwConsensusObjectV2::CourtClosed { .. } => {
                    assert_eq!(position, 0, "a whole close is not the only part");
                    assert_eq!(plan.parts.len(), 1);
                    reassembled.extend_from_slice(whole);
                }
                PalwConsensusObjectV2::CourtCloseDeclared { session_id, side, count, chunk_digests, close_digest, .. } => {
                    assert_eq!(position, 0, "the declaration does not ride first, so the chunks arrive before their group");
                    assert_eq!(*session_id, plan.session_id);
                    assert_eq!(Some(*side), plan.side);
                    assert_eq!(*count, plan.chunk_count, "the declaration pins a different number of chunks than the plan cut");
                    assert_eq!(chunk_digests.len(), *count as usize, "CourtCloseDigestsIncoherent: the digests are not `count` long");
                    assert_eq!(*close_digest, palw_court_close_chunk_digest_v1(whole), "close_digest is not the digest of the whole");
                    pinned = chunk_digests.clone();
                }
                PalwConsensusObjectV2::CourtCloseChunk { session_id, side, index, bytes } => {
                    assert_eq!(*session_id, plan.session_id);
                    assert_eq!(Some(*side), plan.side);
                    assert_eq!(*index as usize, position - 1, "the chunks are not in index order behind the declaration");
                    assert!(
                        (*index) < plan.chunk_count,
                        "CourtCloseIndexOutOfRange: index {index} against count {}",
                        plan.chunk_count
                    );
                    assert!(
                        !bytes.is_empty() && bytes.len() <= PALW_COURT_CLOSE_CHUNK_MAX_BYTES,
                        "CourtCloseChunkTooLarge: chunk {index} is {} bytes",
                        bytes.len()
                    );
                    assert_eq!(
                        palw_court_close_chunk_digest_v1(bytes),
                        pinned[*index as usize],
                        "CourtCloseChunkDigestMismatch: chunk {index} is not the preimage the declaration pinned"
                    );
                    reassembled.extend_from_slice(bytes);
                }
                other => panic!("a carriage produced a {}", object_kind_v1(other)),
            }
        }
        assert_eq!(reassembled, whole, "the parts do not reassemble into the close");
    }

    /// **The widest close a court will still admit, built and then checked BOTH ways.**
    ///
    /// One decode-token row is the densest payload a close carries and the cost rule charges it at
    /// four bytes a lane, so the width that saturates `max_close_bytes` is the ceiling over four.
    /// Asserted admissible at that width and refused one lane wider, because a test that assumes
    /// where a ceiling is measures its own assumption.
    fn largest_admissible_close_v1(court: &PalwCourtParamsV2, profile: &PalwShapeProfileV3) -> PalwConsensusObjectV2 {
        let lanes = (court.max_close_bytes() / 4) as usize;
        let object = close_with_binding(profile, 1, lanes);
        let PalwConsensusObjectV2::CourtClosed { proof, .. } = &object else { unreachable!() };
        check_close_cost_v2(proof, court).expect("a close of exactly max_close_bytes is admissible");
        let over = close_with_binding(profile, 1, lanes + 1);
        let PalwConsensusObjectV2::CourtClosed { proof: wider, .. } = &over else { unreachable!() };
        check_close_cost_v2(wider, court).expect_err("one lane past max_close_bytes must be refused on the merits");
        object
    }

    /// The courts this build actually ships, by the name an operator would say. `Params::from(
    /// NetworkId)` deliberately carries no V2 bundle on any preset (a bundle is a function of
    /// genesis artifacts), so these two assembled sets are the whole shipped set.
    fn shipped_rulesets_v1() -> Vec<(&'static str, Params)> {
        vec![("palw-rc", palw_rc_shipped_params()), ("devnet", devnet_shipped_params())]
    }

    fn court_of_v1(params: &Params) -> Option<PalwCourtParamsV2> {
        match &params.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.court),
            _ => None,
        }
    }

    /// The cut is the COURT's cut, not the certification lane's: chunks of
    /// [`PALW_COURT_CLOSE_CHUNK_MAX_BYTES`] behind one declaration that pins each of them by
    /// `palw_court_close_chunk_digest_v1`, keyed `(session_id, side)` rather than by a digest of
    /// the bytes. Not asserted about a re-implementation — [`plan_carriage_v1`] calls the shipped
    /// digest and this pins that it did not wrap it in a second answer.
    #[test]
    fn the_plan_is_the_courts_own_cut() {
        let court = shipped_court();
        let object = close_with_binding(&sample_profile(), 1, 50_000);
        let whole = borsh::to_vec(&object).expect("serializes");
        assert!(whole.len() > PALW_COURT_CLOSE_CHUNK_MAX_BYTES, "the sample close fits one carrier, so it tests nothing");
        let plan = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Challenger)).expect("plans");
        assert_eq!(plan.whole_bytes, whole.len());
        assert_eq!(plan.chunk_count as usize, whole.len().div_ceil(PALW_COURT_CLOSE_CHUNK_MAX_BYTES));
        assert_eq!(plan.close_blocks(), plan.chunk_count as u64 + 1, "the declaration is a block of the mover's turn too");
        assert_eq!(plan.close_digest, palw_court_close_chunk_digest_v1(&whole));
        assert_eq!(plan.assembly_daa(), PALW_COURT_CLOSE_INCLUSION_MARGIN * plan.chunk_count as u64);
        assert_carriers_fit_and_reassemble_v1(&plan, &whole);
    }

    /// **The gap, as a test that goes red when it closes — and it is not the gap this file used to
    /// name.**
    ///
    /// The previous version asserted that a split close was refused by
    /// `PalwStateV2Error::ChunkedObjectKindNotAllowed`. That rule is real and it still refuses a
    /// `CourtClosed` in the certification lane — asserted below, because it is the reason the court
    /// has its own table — but it is NOT what stops this command: W5 built that table, and nothing
    /// this tool emits goes near `pending_chunks`.
    ///
    /// What stops it is W6, and every assertion here reads it off consensus rather than off this
    /// file's own seam, so it cannot pass by agreeing with itself.
    #[test]
    fn the_split_path_is_shut_by_a_rule_read_from_consensus() {
        // 1. The design W5 did not take, still not taken: the generic chunk lane admits one kind.
        assert!(
            !palw_chunked_object_kind_admitted_v1(&close_with_binding(&sample_profile(), 0, 0)),
            "a CourtClosed may now ride the certification lane's chunk group — the court's own table is not the only path any \
             more, and this file's cutter needs re-reading"
        );
        let refusal = kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ChunkedObjectKindNotAllowed.to_string();
        assert!(refusal.contains("only a FamilyCertified may ride in chunks"), "{refusal}");

        // 2. The rule that ACTUALLY shuts the door: no close-declaration signing context ships, so
        //    there is no message a declaration could bind, and the acceptance arm that would check
        //    one refuses everything instead.
        for domain in PALW_COURT_V2_ALL_DOMAINS {
            println!("shipped court domain: {}", String::from_utf8_lossy(domain));
        }
        assert!(
            close_declaration_context_v1().is_none(),
            "a close-declaration signing context now ships: W6 is landing. Re-read virtual_processor's CourtCloseDeclared arm, \
             sign the declaration under it, and delete this file's gate"
        );

        // 3. And the object says so in this crate, with no node: an unsigned declaration may not
        //    ride, and this build cannot sign one.
        let court = shipped_court();
        let object = close_with_binding(&sample_profile(), 1, 50_000);
        let plan = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Challenger)).expect("plans");
        let declaration = plan.declaration.expect("a split close declares");
        let why = palw_lifecycle_object_may_ride_v2(&declaration).expect_err("an unsigned declaration must not ride");
        assert!(why.contains("signature"), "the refusal stopped naming the signature: {why}");

        // 4. The seam agrees, and it names the work rather than a design nobody built.
        let gap = court_close_chunked_carriage_v1().expect_err("the split path is shut on this build");
        assert!(gap.context.is_none(), "the seam saw a context that step 2 did not");
        assert_eq!(gap.needs.len(), 3, "the gap list moved without this test being read");
        assert!(gap.rule.contains("W6"), "the seam no longer names the work item that owns the refusal: {}", gap.rule);
    }

    /// `palw_court_move_cost_daa_v1` takes `close_blocks` for exactly this, and the report must
    /// carry the term rather than a second estimate: a `k`-carrier close costs the mover `k - 1`
    /// DAA more than the same close would whole.
    #[test]
    fn splitting_costs_the_mover_exactly_the_extra_blocks() {
        let court = shipped_court();
        let one = carriage_deadline_v1(&court, None, 1).expect("prices");
        for blocks in [1u64, 2, 3, 8, 24] {
            let split = carriage_deadline_v1(&court, None, blocks).expect("prices");
            assert_eq!(split.move_cost_daa_unsplit, one.move_cost_daa, "the unsplit reference moved with the split");
            assert_eq!(
                split.move_cost_daa,
                one.move_cost_daa + (blocks - 1),
                "a {blocks}-carrier close is not priced at the unsplit move plus its extra blocks"
            );
            assert_eq!(split.close_blocks, blocks);
        }
        // The widest row is what an unnamed class is priced on, and the report says so.
        assert!(one.row_was_assumed, "a report with no class named did not say the row was assumed");
    }

    /// The resume walk: the chain's answer is the CHANGE that is still visible, and the highest
    /// recorded part whose change is visible is how far the group got — because carrier `k`'s
    /// change is gone precisely WHEN part `k + 1` landed.
    #[test]
    fn resume_walks_to_the_highest_carrier_the_chain_still_shows() {
        let txid = |b: u8| kaspa_consensus_core::tx::TransactionId::from_bytes([b; 64]);
        let parts: Vec<JournalPart> =
            (0u8..4).map(|i| JournalPart { index: i, txid: txid(i + 1).to_string(), fee_sompi: 1 }).collect();
        let out = |b: u8| TransactionOutpoint::new(txid(b), 0);

        // Nothing visible: nothing landed, or the changes were swept elsewhere. Either way the
        // tool must not claim progress it cannot see.
        assert_eq!(resume_point_v1(&parts, &[]), None);
        // Only part 2's change is visible — parts 0 and 1 were spent BY it.
        assert_eq!(resume_point_v1(&parts, &[out(3)]), Some((2u8, out(3))));
        // Several visible (a re-org, or an unrelated change): the highest wins.
        assert_eq!(resume_point_v1(&parts, &[out(1), out(3), out(2)]), Some((2u8, out(3))));
        // A stranger's outpoint is not this group's progress.
        assert_eq!(resume_point_v1(&parts, &[out(99)]), None);
        // A malformed journal entry is skipped, not fatal, and does not hide a good one.
        let mut mixed = parts.clone();
        mixed.push(JournalPart { index: 3, txid: "not-a-txid".into(), fee_sompi: 1 });
        assert_eq!(resume_point_v1(&mixed, &[out(3)]), Some((2u8, out(3))));
    }

    /// **A close too large to carry is refused BEFORE the first fee, by the ruleset's own count.**
    ///
    /// This branch used to be unreachable: `court_close_max_parts_v1` answered
    /// `PALW_OBJECT_CHUNK_MAX_COUNT`, which is what the certification lane's cutter already
    /// enforced, so the cutter always spoke first. W5 landed `max_close_chunks` and the cut is now
    /// the court's, so this is the only thing that refuses — and what it says is what an operator
    /// has to act on: which limit, and what would have fitted under it.
    #[test]
    fn a_close_too_large_to_carry_is_refused_before_anything_is_spent() {
        let court = shipped_court();
        let max = court_close_max_parts_v1(&court);
        assert_eq!(max as u64, court.max_close_chunks(), "the part cap is no longer the ruleset's own count");
        assert!(max <= PALW_COURT_CLOSE_MAX_CHUNKS, "the ruleset admits more carriers than the group row can address");
        // One carrier past what `max` carriers hold.
        let lanes = ((max as usize + 1) * PALW_COURT_CLOSE_CHUNK_MAX_BYTES) / 4;
        let object = close_with_binding(&sample_profile(), 1, lanes);
        let err = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Challenger)).expect_err("a close over the cap is refused");
        println!("{}", err.msg);
        assert!(err.msg.contains(&format!("admits at most {max}")), "the refusal does not name the ceiling it broke: {}", err.msg);
        assert!(err.msg.contains("this network's court"), "the refusal does not say WHICH limit stopped it: {}", err.msg);
        assert!(err.msg.contains("what would fit"), "the refusal does not say what number would fit: {}", err.msg);
        assert!(err.msg.contains("no fee was spent"), "{}", err.msg);
    }

    /// **A ruleset that pays for one carrier refuses a second, and this build ships one.**
    ///
    /// `devnet_shipped_params()` keeps the pre-ADR-0080 81,920-byte ceiling, which frames to
    /// `max_close_chunks = 1`. So on devnet the split path must never engage — and the refusal must
    /// say it is the NETWORK's answer, because the remedy is a smaller close and not a bigger
    /// bitmap. The one-per-network reading is the whole reason `court_close_max_parts_v1` takes the
    /// court rather than reading a constant.
    #[test]
    fn a_ruleset_that_pays_for_one_carrier_refuses_a_second() {
        let court = court_of_v1(&devnet_shipped_params()).expect("devnet ships a V2 bundle");
        assert_eq!(court.max_close_chunks(), 1, "devnet no longer carries the one-carrier close ceiling");
        assert_eq!(court_close_max_parts_v1(&court), 1);
        // Two carriers' worth: admissible nowhere near devnet's cost ceiling, but that is the
        // command's first check and this is the second.
        let object = close_with_binding(&widest_profile(), 1, 40_000);
        let err = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Challenger)).expect_err("devnet may not split a close");
        println!("{}", err.msg);
        assert!(err.msg.contains("admits at most 1"), "{}", err.msg);
        assert!(err.msg.contains("this network's court"), "{}", err.msg);
    }

    /// **A split close will not guess which side is moving.**
    ///
    /// The transition reads the declarer from the session (challenger) or the claim (executor), so
    /// a defaulted side files one party's move under the other party's bond — and the mover pays
    /// for it. Refused by name, before the first fee, with the two words `--side` takes.
    #[test]
    fn a_split_close_names_the_side_rather_than_defaulting_it() {
        let court = shipped_court();
        let object = close_with_binding(&sample_profile(), 1, 50_000);
        let err = plan_carriage_v1(&object, &court, None).expect_err("a split close with no side must be refused");
        assert!(err.msg.contains("--side challenger"), "{}", err.msg);
        assert!(err.msg.contains("no fee was spent"), "{}", err.msg);
        // A close that FITS needs no side: a `CourtClosed` attributes itself to nobody.
        let small = close_with_binding(&sample_profile(), 0, 0);
        assert!(plan_carriage_v1(&small, &court, None).expect("a whole close needs no side").side.is_none());
        // And the words are the ones the parser takes.
        assert_eq!(parse_side_v1("Challenger").expect("case-insensitive"), PalwCourtSideV1::Challenger);
        assert_eq!(parse_side_v1(" executor ").expect("trimmed"), PalwCourtSideV1::Executor);
        assert!(parse_side_v1("prosecutor").is_err(), "a third party may not declare");
    }

    /// **The assembly window is the chain's own, and the refusal says what would have fitted.**
    ///
    /// `apply_object` refuses a declaration whose group could not finish inside the session's
    /// backstop (`CourtCloseCannotAssemble`), and the backstop never moves. A mover who learns that
    /// on chain has paid for the declaration and suspended a rung for nothing, so the same
    /// expression is evaluated here — with the consensus function, not a copy of its arithmetic.
    #[test]
    fn the_assembly_window_is_the_chains_own_and_the_refusal_says_what_would_fit() {
        assert_eq!(palw_close_assembly_daa_v1(23), PALW_COURT_CLOSE_INCLUSION_MARGIN * 23);
        // Exactly enough is enough: the gate is `>`, not `>=`.
        assert!(check_assembly_window_v1(10, 1_000, 1_000 + PALW_COURT_CLOSE_INCLUSION_MARGIN * 10).is_ok());
        let err = check_assembly_window_v1(10, 1_000, 1_000 + PALW_COURT_CLOSE_INCLUSION_MARGIN * 10 - 1)
            .expect_err("one DAA short must be refused");
        println!("{}", err.msg);
        assert!(err.msg.contains("CourtCloseCannotAssemble"), "the refusal does not name the rule: {}", err.msg);
        assert!(err.msg.contains("what would fit: 9 chunks"), "the refusal does not say what would fit: {}", err.msg);
        assert!(err.msg.contains("no fee was spent"), "{}", err.msg);
    }

    /// **A carriage journal is keyed the way the chain keys the group.**
    ///
    /// `(session_id, side)`, not a digest of the bytes — that is the difference between the court's
    /// table and the certification lane's, and a journal keyed the other way would let one side's
    /// resume walk the other side's carriers.
    #[test]
    fn a_carriage_journal_is_keyed_the_way_the_chain_keys_the_group() {
        let court = shipped_court();
        let object = close_with_binding(&sample_profile(), 1, 50_000);
        let challenger = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Challenger)).expect("plans");
        let executor = plan_carriage_v1(&object, &court, Some(PalwCourtSideV1::Executor)).expect("plans");
        assert_ne!(
            challenger.journal_key(),
            executor.journal_key(),
            "the two sides of one session share a journal, so one side's resume would skip the other's parts"
        );
        assert!(challenger.journal_key().starts_with(&challenger.session_id.to_string()));
        assert!(challenger.journal_key().ends_with("challenger"));
        // A close that rides whole opens no group, so there is nothing to resume into.
        assert!(plan_carriage_v1(&close_with_binding(&sample_profile(), 0, 0), &court, None).expect("plans").journal_key().is_empty());
    }

    /// The command files closes and says so about anything else, rather than handing a stranger's
    /// object to a node that would refuse it less clearly.
    #[test]
    fn only_a_close_is_a_close() {
        assert_eq!(
            object_kind_v1(&PalwConsensusObjectV2::ObjectChunk { group: Hash64::default(), index: 0, count: 1, bytes: vec![0] }),
            "ObjectChunk"
        );
        assert_eq!(object_kind_v1(&close_with_binding(&sample_profile(), 0, 0)), "CourtClosed");
        let court = shipped_court();
        let plan = plan_carriage_v1(&close_with_binding(&sample_profile(), 1, 50_000), &court, Some(PalwCourtSideV1::Executor))
            .expect("plans");
        assert_eq!(object_kind_v1(plan.declaration.as_ref().expect("declares")), "CourtCloseDeclared");
        assert_eq!(object_kind_v1(&plan.parts[1]), "CourtCloseChunk");
    }

    /// **The message an operator gets when the split path is blocked, read rather than assumed.**
    ///
    /// This branch is reachable on the RC the moment a close outgrows one carrier, which
    /// [`the_headroom_between_the_two_ceilings_is_measured_not_assumed`] shows a legal one does —
    /// so unlike its previous life, this is not the text of an unreachable branch. It is what an
    /// operator under a court deadline actually reads, and it has to be enough to escalate on.
    #[test]
    fn the_blocked_message_names_every_missing_piece() {
        let gap = court_close_chunked_carriage_v1().expect_err("the split path is blocked on this build");
        let message = blocked_message_v1(&gap, 24);
        println!("{message}");
        assert!(message.contains("this close needs 24 carriers"), "{message}");
        assert!(message.contains("Nothing was carried and no fee was spent"), "{message}");
        for need in gap.needs {
            assert!(message.contains(need), "the message drops a missing piece: {need}\n{message}");
        }
        assert!(message.contains("CourtCloseDeclared"), "the message does not name the object that is refused: {message}");
        assert!(message.contains("W6"), "the message does not name the work item to escalate to: {message}");
    }

    fn widest_profile() -> PalwShapeProfileV3 {
        let rows = palw_shipped_court_rows_v1().expect("rows");
        rows.iter()
            .max_by_key(|r| borsh::to_vec(&close_with_binding(&r.profile, 0, 0)).expect("serializes").len())
            .expect("at least one row")
            .profile
            .clone()
    }

    /// A close is priced against a REAL court, not a hand-made one: `DEFAULT_MAX_CLOSE_BYTES` is
    /// the ceiling every shipped row is admitted under, and reading it here keeps the tests on the
    /// same number the command reads off the network.
    fn shipped_court() -> PalwCourtParamsV2 {
        court_of_v1(&palw_rc_shipped_params()).expect("the RC preset carries a V2 bundle, so it has a court")
    }

    fn sample_profile() -> PalwShapeProfileV3 {
        palw_shipped_court_rows_v1().expect("rows").first().expect("at least one row").profile.clone()
    }

    /// The cost ceiling and the carrier ceiling are read off two different rules, and this pins
    /// that the command is not conflating them — now including the derivation W5 put BETWEEN them:
    /// the byte ceiling is the carriage count framed, so the two move together by construction and
    /// a build where they do not is a build where one of them was edited by hand.
    #[test]
    fn the_two_ceilings_are_two_numbers() {
        let court = shipped_court();
        assert_eq!(court.max_close_bytes(), DEFAULT_MAX_CLOSE_BYTES, "the RC's close ceiling is not the shipped default");
        assert_ne!(
            court.max_close_bytes() as usize,
            PALW_COURT_CLOSE_CHUNK_MAX_BYTES,
            "the cost ceiling and the carrier ceiling became one number — this command's premise needs re-reading"
        );
        assert_eq!(
            court.max_close_bytes(),
            palw_close_bytes_for_chunks_v1(court.max_close_chunks()),
            "max_close_bytes is no longer the ruleset's carriage count framed — W5's derivation has been edited around"
        );
        // An arithmetic proof is the arm the cost rule measures in these units.
        let object = close_with_binding(&sample_profile(), 0, 0);
        let PalwConsensusObjectV2::CourtClosed { proof, .. } = &object else { unreachable!() };
        assert_eq!(arithmetic_close_bytes_v2(proof), None, "a decode-token close is priced by its own arm, not the arithmetic one");
    }
}
