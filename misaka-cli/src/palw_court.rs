//! **`misaka palw court-close` — filing a court close whose carriage does not fit one carrier.**
//!
//! A court move is a `CourtClosed` object on an ordinary lifecycle transaction, and until this
//! command the only thing in the tree that filed one was a node's own panel loop
//! (`kaspad/src/palw_panel.rs`), which builds a close from a capture it happens to hold and drops
//! it on the floor if it does not fit. An operator holding an assembled close — from a drill, from
//! another node's outbox, from a court arm that stalled — had no way to put it on chain at all,
//! and no way to find out BEFORE spending a fee whether it would be refused.
//!
//! # The two ceilings, which are not the same ceiling — and how little room is between them
//!
//! This is the distinction the command exists to make legible, and getting it wrong is what a
//! flat "is the file big" check would do:
//!
//! * **the COST ceiling** — `PalwCourtParamsV2::max_close_bytes` (81,920 on every shipped row),
//!   checked by [`check_close_cost_v2`] over the PROOF's own payload: openings at 64 bytes a
//!   sibling, logits rows at 4 bytes a lane, the ids. It is a consensus rule and it is inside
//!   `palw_ruleset_id_v2`. A close over it is refused by every node on the merits, and no amount
//!   of carriage helps.
//! * **the CARRIER ceiling** — [`PALW_OBJECT_CHUNK_MAX_BYTES`] (100,000), the mempool's
//!   standard-transaction mass, measured over the SERIALIZED OBJECT — which carries the proof
//!   payload PLUS a `PalwStepBindingV2`, and that carries the class's whole `PalwShapeProfileV3`
//!   ("carried in full: adjudication needs the node tables"), the job context and the checkpoint
//!   profile. The cost rule counts none of it.
//!
//! **Measured, not assumed** ([`tests::the_binding_is_the_untolled_part_of_a_carrier`]): a close
//! whose proof payload is EMPTY still serializes to 5,631 bytes on `PALW-QWEN25-A16`, 6,346 on
//! `PALW-BASE-0` and **13,996 on the widest shipped row**, `Qwen3.6-35B-A3B/graph-v3`. So the
//! binding does not by itself outgrow a carrier — the first thing this file claimed, and it was
//! wrong — but it is not free either, and the sum is what matters:
//!
//! ```text
//!   81,920  what the court will admit as a close
//! + 13,996  the widest shipped binding, which the court does not charge for
//! = 95,916  the largest carrier a legal close can need today
//!  100,000  what one carrier holds
//!   ------
//!    4,084  bytes of headroom, on the whole shipped set
//! ```
//!
//! **That is the state of the world this tool is built for.** Today no acceptable close needs
//! splitting, by four kilobytes — so this command's split path is dormant and its single-carrier
//! path is the whole of what runs. The moment `max_close_bytes` rises by more than 4,084 (which
//! is what any of ADR-0080's consensus streams raising it would do), every close on the hybrid row
//! needs carriage this command is the only thing in the tree that can perform.
//! [`tests::the_headroom_between_the_two_ceilings_is_measured_not_assumed`] pins the four
//! thousand bytes, so the stream that spends them finds out here rather than in a dispute.
//!
//! # What this command CANNOT do yet, and where the gap is
//!
//! [`court_close_chunked_carriage_v1`] is the one function that answers "may a close ride in
//! parts on this build", and today it answers no: `apply_object` refuses any assembled chunk group
//! that is not a `FamilyCertified` (`PalwStateV2Error::ChunkedObjectKindNotAllowed`). The gap is
//! named there, in one place, so the day the consensus arm lands the tool is one function body
//! away from working — and until then it refuses to spend a fee on carriers the chain would drop.
//! That refusal is the point: the failure mode this command exists to prevent is an operator under
//! a court deadline paying for eight carriers and learning at the eighth that the group cannot
//! assemble.

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_court_deadline::{
    PalwShippedCourtRowV1, palw_court_move_cost_daa_v1, palw_court_replay_positions_v1, palw_shipped_court_rows_v1,
};
use kaspa_consensus_core::palw_court_v2::{PalwCourtVerdictProofV2, check_close_cost_v2};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PALW_OBJECT_CHUNK_MAX_BYTES, PALW_OBJECT_CHUNK_MAX_COUNT, PalwConsensusObjectV2, palw_object_chunk_group_id_v1,
    palw_object_chunks_v1,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::path::{Path, PathBuf};

// =================================================================================================
// The seams: everything this tool needs from consensus and does not have
// =================================================================================================

/// **The one place that answers "may a court close ride in chunks on this build".**
///
/// Today: no. `apply_object`'s chunk arm decodes the assembled group and refuses anything that is
/// not a `FamilyCertified`, so a `CourtClosed` split across carriers would take every fee, occupy
/// one of the eight `pending_chunks` slots for its whole TTL, and then fail the transition in the
/// block that completed it. There is no partial credit and no refund.
///
/// **The consensus changes this function is waiting on**, stated so the owning session can close
/// them without reading this file:
///
/// 1. `consensus/core/src/palw_state_v2.rs`, the `ObjectChunk` arm of `apply_object`: admit
///    `PalwConsensusObjectV2::CourtClosed` alongside `FamilyCertified`. The rest of the arm — the
///    group digest, the TTL eviction, the duplicate-part refusal — needs nothing.
/// 2. `consensus/core/src/palw_state_v2.rs`, `palw_object_rent_ceiling_v1`: a chunked close's
///    rent. `palw_object_chunk_group_rent_v1` already prices the SLOT and is kind-blind, so a
///    close pays it exactly as a drill does; what is missing is the analogue of
///    `palw_certification_min_fee_v1` for the COMPLETING chunk, because completing a close costs
///    every validator one `adjudicate_court_close_v2`. Without it, a close's grading is free.
/// 3. `consensus/core/src/palw_mode_v2.rs`, `PalwCourtParamsV2`: `max_close_chunks`, so the
///    ruleset — not [`PALW_OBJECT_CHUNK_MAX_COUNT`] — says how many carriers one move may spend.
///    [`court_close_max_parts_v1`] reads it the moment it exists and falls back until then.
/// 4. Whichever of W2/W3/W5 raises `max_close_bytes` above one carrier. Until it does, no
///    ACCEPTABLE close can need splitting on its proof payload alone — but see the module doc:
///    the binding already can, and that is why this command is not dead code today.
pub(crate) fn court_close_chunked_carriage_v1() -> Result<(), CarriageGap> {
    Err(CarriageGap {
        what: "a chunked CourtClosed is refused by the transition that assembles it",
        rule: "PalwStateV2Error::ChunkedObjectKindNotAllowed — only a FamilyCertified may ride in chunks",
        needs: &[
            "palw_state_v2::apply_object's ObjectChunk arm: admit CourtClosed",
            "palw_state_v2::palw_object_rent_ceiling_v1: price the completing chunk's adjudication",
            "palw_mode_v2::PalwCourtParamsV2: max_close_chunks",
        ],
    })
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
}

/// **How many carriers one court move may spend.**
///
/// The ruleset should answer this — a move's carriage is part of what a turn deadline has to
/// admit, so it belongs beside `turn_deadline_daa` in `PalwCourtParamsV2`. It does not, so the
/// answer today is the object-chunk rule's own count and the argument is taken and ignored. Named
/// as a function anyway: when `max_close_chunks` lands, one body changes and every caller —
/// the plan, the deadline and the refusal — reads the new number at once.
pub(crate) fn court_close_max_parts_v1(_court: &PalwCourtParamsV2) -> u8 {
    PALW_OBJECT_CHUNK_MAX_COUNT
}

// =================================================================================================
// The plan
// =================================================================================================

/// One close's carriage, decided before a fee is spent.
#[derive(Debug)]
pub(crate) struct CarriagePlan {
    /// The chunk group, when the close needs more than one carrier. `None` = it rides whole.
    pub group: Option<Hash64>,
    /// The objects to carry, in the order the chain must see them. One entry when `group` is None.
    pub parts: Vec<PalwConsensusObjectV2>,
    /// The serialized close, before cutting.
    pub whole_bytes: usize,
}

impl CarriagePlan {
    /// The blocks this close spends of the mover's own turn — the `close_blocks` term
    /// [`palw_court_move_cost_daa_v1`] takes, and the reason it takes it.
    pub fn close_blocks(&self) -> u64 {
        self.parts.len() as u64
    }
}

/// **Cut the close the way the chain cuts it, or say it rides whole.**
///
/// [`palw_object_chunks_v1`] is the house cutter and the group digest it keys by is the chain's
/// own; nothing here re-derives either. What this adds is the refusal the cutter cannot make: a
/// close that needs more carriers than the ruleset allows is named here rather than discovered by
/// a `TooManyPendingChunkGroups` on the fourth carrier.
pub(crate) fn plan_carriage_v1(object: &PalwConsensusObjectV2, court: &PalwCourtParamsV2) -> Result<CarriagePlan, CliError> {
    let whole = borsh::to_vec(object).map_err(|e| CliError::new(exit::GENERIC, format!("this close does not serialize: {e}")))?;
    let whole_bytes = whole.len();
    match palw_object_chunks_v1(object)
        .map_err(|e| CliError::new(exit::GENERIC, format!("this close cannot be cut into carriers: {e}. Nothing was carried.")))?
    {
        None => Ok(CarriagePlan { group: None, parts: vec![object.clone()], whole_bytes }),
        Some(parts) => {
            let max = court_close_max_parts_v1(court);
            if parts.len() > max as usize {
                return Err(CliError::new(
                    exit::GENERIC,
                    format!(
                        "this close is {whole_bytes} bytes — {} carriers of {PALW_OBJECT_CHUNK_MAX_BYTES}, and a court move may \
                         spend at most {max}. Nothing was carried.",
                        parts.len()
                    ),
                ));
            }
            Ok(CarriagePlan { group: Some(palw_object_chunk_group_id_v1(&whole)), parts, whole_bytes })
        }
    }
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
/// `pending_chunks` — the parts of the group that actually arrived, with the DAA the group was
/// opened at and therefore its TTL — and `pending_chunk_group()` reads it. No RPC exposes it:
/// `rpc/core/src/api/rpc.rs` has `get_palw_producer_facts`, `get_palw_derived_artifacts` and
/// `get_palw_free_prompt_claim`, and nothing for chunk groups. **The gap to close is one RPC**
/// (`GetPalwPendingChunkGroup { group } -> { count, opened_daa, parts_held: Vec<u8> }`), after
/// which this file's [`resume_point_v1`] becomes a fallback rather than the mechanism.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct CarriageJournal {
    /// 128-hex `palw_object_chunk_group_id_v1` of the whole close. A journal whose group does not
    /// match the close on disk is a journal for another close, and is refused rather than reused.
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
    let plan = plan_carriage_v1(&object, &court)?;
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
    let gap = if plan.group.is_some() { court_close_chunked_carriage_v1().err() } else { None };

    // ---- 4. report -----------------------------------------------------------------------------
    let text = ctx.output != OutputFormat::Json;
    if text {
        println!("court close for session {session_id}");
        println!("  verdict {verdict}, proof {}", proof_kind_v1(proof));
        match plan.group {
            None => println!("  {} bytes — one carrier (the limit is {PALW_OBJECT_CHUNK_MAX_BYTES})", plan.whole_bytes),
            Some(group) => println!(
                "  {} bytes — {} carriers of at most {PALW_OBJECT_CHUNK_MAX_BYTES}, chunk group {group}",
                plan.whole_bytes,
                plan.parts.len()
            ),
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
                    "whole_bytes": plan.whole_bytes, "group": plan.group.map(|g| g.to_string()),
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
                    "group": plan.group.map(|g| g.to_string()), "close_blocks": plan.close_blocks(),
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
    let mut journal = load_journal_v1(&journal_file, plan.group, &ctx.network, &addr.to_string(), args.restart)?;
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
                    "whole_bytes": plan.whole_bytes, "group": plan.group.map(|g| g.to_string()),
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
            if plan.group.is_some() {
                println!("  the chain applies it in the block that completes the group — until then it is parts in `pending_chunks`.");
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
    group: Option<Hash64>,
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
    let want = group.map(|g| g.to_string()).unwrap_or_default();
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
        group: plan.group.map(|g| g.to_string()).unwrap_or_default(),
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
    use kaspa_consensus_core::palw_court_v2::arithmetic_close_bytes_v2;
    use kaspa_consensus_core::palw_legs::{PALW_LEGS_OBJECT_VERSION_V1, PalwCheckpointProfileV1};
    use kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
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
    /// an upper bound rather than as equalities: a profile that GROWS eats the headroom
    /// [`the_headroom_between_the_two_ceilings_is_measured_not_assumed`] depends on, and that is
    /// the event this assertion exists to catch.
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
        println!("widest shipped row: {widest_row} at {widest} bytes, against a {PALW_OBJECT_CHUNK_MAX_BYTES}-byte carrier");
        assert!(widest > 0, "an empty close weighed nothing — the binding stopped riding");
        assert!(
            widest <= WIDEST_SHIPPED_BINDING_BYTES,
            "{widest_row}'s binding grew to {widest} bytes, past the {WIDEST_SHIPPED_BINDING_BYTES} this file's headroom \
             arithmetic is written against — re-measure the headroom before shipping it"
        );
    }

    /// **The four kilobytes between the two ceilings.**
    ///
    /// A legal close is at most `max_close_bytes` of proof; its carrier is that plus the binding,
    /// which the cost rule does not count. If the sum ever exceeds one carrier, a close that every
    /// node would ADMIT becomes one no operator can FILE — the failure this command exists for,
    /// and one that would otherwise be discovered by a mover losing a turn.
    ///
    /// The assertion is deliberately on the sum and not on `max_close_bytes` alone: the ceiling is
    /// allowed to move, and what is not allowed is for it to move past the carrier without the
    /// split path being armed. When this goes red, `court_close_chunked_carriage_v1`'s gap list is
    /// the work.
    #[test]
    fn the_headroom_between_the_two_ceilings_is_measured_not_assumed() {
        let court = shipped_court();
        let largest_legal_carrier = court.max_close_bytes() as usize + WIDEST_SHIPPED_BINDING_BYTES;
        println!(
            "max_close_bytes {} + widest binding {WIDEST_SHIPPED_BINDING_BYTES} = {largest_legal_carrier} against a \
             {PALW_OBJECT_CHUNK_MAX_BYTES}-byte carrier",
            court.max_close_bytes()
        );
        assert!(
            largest_legal_carrier <= PALW_OBJECT_CHUNK_MAX_BYTES,
            "a close this network ADMITS no longer fits one carrier ({largest_legal_carrier} > \
             {PALW_OBJECT_CHUNK_MAX_BYTES}): the split path in this file is now load-bearing and it is still blocked on \
             court_close_chunked_carriage_v1's gap list"
        );
        assert_eq!(
            PALW_OBJECT_CHUNK_MAX_BYTES - largest_legal_carrier,
            4_084,
            "the headroom moved; this file's module doc states 4,084 bytes and would now be lying"
        );
    }

    /// The widest binding any shipped row produces, measured by
    /// [`the_binding_is_the_untolled_part_of_a_carrier`] and used by the headroom arithmetic. One
    /// spelling, so the two tests cannot drift.
    const WIDEST_SHIPPED_BINDING_BYTES: usize = 13_996;

    /// The cut is the chain's cut: the group digest is `palw_object_chunk_group_id_v1` over the
    /// whole object's bytes, the parts are in index order, and reassembling them reproduces the
    /// object. Not asserted about a re-implementation — [`plan_carriage_v1`] calls the house
    /// cutter and this pins that it did not wrap it in a second answer.
    #[test]
    fn the_plan_is_the_chains_own_cut() {
        let court = shipped_court();
        // A close big enough to need carriers: the pin is what makes it big, and it is priced by
        // the court, so this one would ALSO fail the cost rule — which is why the command checks
        // that first. Here only the carriage is under test.
        let object = close_with_binding(&sample_profile(), 40, 2_000);
        let whole = borsh::to_vec(&object).expect("serializes");
        assert!(whole.len() > PALW_OBJECT_CHUNK_MAX_BYTES, "the sample close fits one carrier, so it tests nothing");
        let plan = plan_carriage_v1(&object, &court).expect("plans");
        assert_eq!(plan.whole_bytes, whole.len());
        assert_eq!(
            plan.group,
            Some(palw_object_chunk_group_id_v1(&whole)),
            "the plan keys the group by something other than the chain's digest"
        );
        assert_eq!(plan.parts.len(), whole.len().div_ceil(PALW_OBJECT_CHUNK_MAX_BYTES));
        assert_eq!(plan.close_blocks(), plan.parts.len() as u64);
        let mut reassembled = Vec::new();
        for (i, part) in plan.parts.iter().enumerate() {
            match part {
                PalwConsensusObjectV2::ObjectChunk { group, index, count, bytes } => {
                    assert_eq!(*group, plan.group.unwrap());
                    assert_eq!(*index as usize, i, "the parts are not in index order");
                    assert_eq!(*count as usize, plan.parts.len());
                    reassembled.extend_from_slice(bytes);
                }
                other => panic!("a split close produced a {}", object_kind_v1(other)),
            }
        }
        assert_eq!(reassembled, whole, "the parts do not reassemble into the close");
    }

    /// **The gap, as a test that goes red when it closes.**
    ///
    /// This is deliberately an assertion that the tool is BLOCKED. The day the consensus arm
    /// admits a chunked `CourtClosed`, this test fails and names the file to change — which is
    /// the only way a seam like [`court_close_chunked_carriage_v1`] does not quietly outlive its
    /// reason. It asserts against the CONSENSUS rule, not against the seam's own return value, so
    /// it cannot pass by agreeing with itself.
    #[test]
    fn a_split_close_is_still_refused_by_the_transition_that_assembles_it() {
        // The rule, read where it lives: only a FamilyCertified may be the assembled object.
        let refusal = kaspa_consensus_core::palw_state_v2::PalwStateV2Error::ChunkedObjectKindNotAllowed.to_string();
        assert!(
            refusal.contains("only a FamilyCertified may ride in chunks"),
            "the consensus refusal changed wording: {refusal} — re-read the ObjectChunk arm before trusting this seam"
        );
        let gap = court_close_chunked_carriage_v1().expect_err(
            "the seam says a split close may be filed. If consensus now admits a chunked CourtClosed, make \
             court_close_chunked_carriage_v1 return Ok(()) and delete this test.",
        );
        assert_eq!(gap.needs.len(), 3, "the gap list moved without this test being read");
    }

    /// `palw_court_move_cost_daa_v1` takes `close_blocks` for exactly this, and the report must
    /// carry the term rather than a second estimate: a `k`-carrier close costs the mover `k - 1`
    /// DAA more than the same close would whole.
    #[test]
    fn splitting_costs_the_mover_exactly_the_extra_blocks() {
        let court = shipped_court();
        let one = carriage_deadline_v1(&court, None, 1).expect("prices");
        for blocks in [1u64, 2, 3, 8] {
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

    /// A close too large to carry at all is refused BEFORE the first fee, by the chain's own
    /// cutter, and the refusal carries the two byte counts an operator needs.
    ///
    /// **The tool's own part cap is unreachable today and that is correct**: `court_close_max_
    /// parts_v1` answers [`PALW_OBJECT_CHUNK_MAX_COUNT`], which is exactly what the cutter already
    /// enforces, so the cutter always speaks first. The cap becomes live the day
    /// `max_close_chunks` lands in `PalwCourtParamsV2` and is SMALLER than eight — a court that
    /// will not let one move spend eight blocks. Asserted here so the day it lands, the reason the
    /// branch exists is on the record.
    #[test]
    fn a_close_too_large_to_carry_is_refused_before_anything_is_spent() {
        let court = shipped_court();
        let max = court_close_max_parts_v1(&court);
        assert_eq!(
            max, PALW_OBJECT_CHUNK_MAX_COUNT,
            "the part cap is no longer the object-chunk count — max_close_chunks has landed, so plan_carriage_v1's own \
             refusal is now reachable and needs its own case here"
        );
        // One carrier past what `max` carriers hold.
        let lanes = (PALW_OBJECT_CHUNK_MAX_BYTES * (max as usize + 1)) / 4;
        let object = close_with_binding(&sample_profile(), 1, lanes);
        let err = plan_carriage_v1(&object, &court).expect_err("a close over the cap must be refused");
        assert!(
            err.msg.contains("cannot be cut into carriers") && err.msg.contains("800000"),
            "the refusal does not name the ceiling it broke: {}",
            err.msg
        );
    }

    /// The command files closes and says so about anything else, rather than handing a stranger's
    /// object to a node that would refuse it less clearly.
    #[test]
    fn only_a_close_is_a_close() {
        assert_eq!(
            object_kind_v1(&PalwConsensusObjectV2::ObjectChunk { group: Hash64::default(), index: 0, count: 1, bytes: vec![0] }),
            "ObjectChunk"
        );
        assert_eq!(
            object_kind_v1(&PalwConsensusObjectV2::CourtClosed {
                session_id: Hash64::default(),
                verdict: kaspa_consensus_core::palw_state_v2::PalwCourtVerdictV2::ExecutorGuilty,
                proof: PalwCourtVerdictProofV2::DecodeToken {
                    binding: match close_with_binding(&sample_profile(), 0, 0) {
                        PalwConsensusObjectV2::CourtClosed { proof: PalwCourtVerdictProofV2::DecodeToken { binding, .. }, .. } =>
                            binding,
                        _ => unreachable!(),
                    },
                    pin: PalwBase0DecodeTokensV1 { logits_rows: Vec::new(), generated_token_ids: Vec::new() },
                    position: 0,
                },
            }),
            "CourtClosed"
        );
    }

    /// **The message an operator gets when the split path is blocked, read rather than assumed.**
    ///
    /// This branch cannot be reached on a shipped ruleset (see
    /// [`no_legal_close_can_need_splitting_on_a_shipped_ruleset`]), which is exactly why its text
    /// is asserted here: an unreachable branch whose wording nobody has read is how an operator
    /// under a deadline ends up re-running a command instead of escalating.
    #[test]
    fn the_blocked_message_names_every_missing_piece() {
        let gap = court_close_chunked_carriage_v1().expect_err("the split path is blocked on this build");
        let message = blocked_message_v1(&gap, 3);
        assert!(message.contains("this close needs 3 carriers"), "{message}");
        assert!(message.contains("Nothing was carried and no fee was spent"), "{message}");
        for need in gap.needs {
            assert!(message.contains(need), "the message drops a missing piece: {need}\n{message}");
        }
        assert!(message.contains("ChunkedObjectKindNotAllowed"), "the message does not name the rule that refuses: {message}");
    }

    /// **The headroom, checked the other way round: can any ADMISSIBLE close actually need a
    /// second carrier?**
    ///
    /// [`the_headroom_between_the_two_ceilings_is_measured_not_assumed`] says there are 4,084
    /// bytes of room. This says what that MEANS: the decode-token pin is the densest payload a
    /// close carries and the court charges it at four bytes a lane, which is what borsh spends on
    /// it too — so evidence bytes and carriage bytes rise together and the binding is the only
    /// wedge between them. A close grown until the court refuses it is still inside one carrier.
    ///
    /// So the split path this file drives is DORMANT on every shipped ruleset, and the command's
    /// single-carrier path is the whole of what runs. When a consensus stream raises
    /// `max_close_bytes`, this test is the one that says the dormancy ended.
    #[test]
    fn no_legal_close_can_need_splitting_on_a_shipped_ruleset() {
        let court = shipped_court();
        let profile = widest_profile();
        // Walk the pin up until the court refuses, and watch the carriage at the last admissible
        // size. Coarse steps: the point is the crossing, not the exact lane.
        let mut last_admissible_carriage = 0usize;
        let mut refused_at = None;
        for lanes in (1_000..=30_000).step_by(1_000) {
            let object = close_with_binding(&profile, 1, lanes);
            let PalwConsensusObjectV2::CourtClosed { proof, .. } = &object else { unreachable!() };
            let carriage = borsh::to_vec(&object).expect("serializes").len();
            match check_close_cost_v2(proof, &court) {
                Ok(()) => last_admissible_carriage = carriage,
                Err(_) => {
                    refused_at = Some((lanes, carriage));
                    break;
                }
            }
        }
        let (lanes, carriage) = refused_at.expect("the court admitted a 30,000-lane pin — the cost rule is not binding");
        println!(
            "the court refuses at {lanes} lanes ({carriage} bytes of carriage); the widest admissible close carried \
             {last_admissible_carriage} bytes against a {PALW_OBJECT_CHUNK_MAX_BYTES}-byte carrier"
        );
        assert!(
            last_admissible_carriage <= PALW_OBJECT_CHUNK_MAX_BYTES,
            "an ADMISSIBLE close ({last_admissible_carriage} bytes) no longer fits one carrier — the split path is live and \
             it is blocked on court_close_chunked_carriage_v1's gap list"
        );
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
        match &kaspa_consensus_core::config::params::palw_rc_shipped_params().palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.court,
            _ => panic!("the RC preset carries no V2 bundle, so it has no court"),
        }
    }

    fn sample_profile() -> PalwShapeProfileV3 {
        palw_shipped_court_rows_v1().expect("rows").first().expect("at least one row").profile.clone()
    }

    /// The cost ceiling and the carrier ceiling are read off two different rules, and this pins
    /// that the command is not conflating them: 80 KiB of proof against 100,000 bytes of carrier.
    #[test]
    fn the_two_ceilings_are_two_numbers() {
        let court = shipped_court();
        assert_eq!(court.max_close_bytes(), DEFAULT_MAX_CLOSE_BYTES, "the RC's close ceiling is not the shipped default");
        assert_ne!(
            court.max_close_bytes() as usize,
            PALW_OBJECT_CHUNK_MAX_BYTES,
            "the cost ceiling and the carrier ceiling became one number — this command's premise needs re-reading"
        );
        // An arithmetic proof is the arm the cost rule measures in these units.
        let object = close_with_binding(&sample_profile(), 0, 0);
        let PalwConsensusObjectV2::CourtClosed { proof, .. } = &object else { unreachable!() };
        assert_eq!(arithmetic_close_bytes_v2(proof), None, "a decode-token close is priced by its own arm, not the arithmetic one");
    }
}
