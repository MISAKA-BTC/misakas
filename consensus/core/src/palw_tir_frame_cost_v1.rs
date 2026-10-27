//! **The TIR public frame's measured cost model and de-stacked clock** (G14 audit §10, `docs/design/palw/g14-completeness-proof-2026-10-16.md`).
//!
//! The frame of `palw_tir_public_conviction_v1` (the only G14 route of an IR class's Free Prompt claim) was priced and clocked as if
//! every accepting action re-read the whole frame 32 times at 32 parser steps a byte, as if every move of every actor — every 16 KiB
//! WorkDebit slice, every custodian's every answer part — waited one inclusion margin behind the one before it, and as if each actor
//! replayed everything 65 times (`reorg_replay_moves` = 64). At testnet-12's 6 custodians that made the minimum claim rent ≥ 2e7 MSK,
//! the outside owner's Open 0.9e6–3.8e6 DAA after the claim, and the Final floor 6e7–2.4e8 DAA after it (audit §9.5).
//!
//! This module holds what replaces those three factors, from MEASUREMENTS only (`misaka/palw/tir/exec/tests/frame_measure.rs`,
//! the fold-level `tir_frame_accepting_actions_are_measured_across_custodian_uppers`):
//!
//! * **Cost units per accepting action** ([`PalwTirFrameClockV1::action_parser_steps`]): the fold's measured work for one control,
//!   instead of `frame_bytes × 32 × 32`. A full-frame pass is charged only where the fold makes one — an outside owner's Open and a
//!   conviction filing ([`PalwTirFrameClockV1::frame_passes_per_check`]); every other control checks its own row and the frame's
//!   immutable identity (the rooted frame is authenticated by the state root and re-checked on every write).
//! * **Which moves are serial** ([`tir_public_turn_span_v1`], [`tir_public_preparation_door_v1`]): only moves that need an earlier
//!   move ACCEPTED ON CHAIN wait an inclusion margin — the owner's Demand before a custodian's answer, an answer's Begin / parts /
//!   Finish in order, a turn's answers before the owner's next Demand, a preparer's debit before its Ready. Different custodians answer
//!   in parallel (not end to end), preparers work in parallel, and one WorkDebit carries a whole pass's (or a whole turn's) slices
//!   — the fold already counts grants, not slices. What every actor at once puts on chain is bounded by the network's measured
//!   inclusion capacity ([`PalwTirFrameClockV1::parallel_bytes_per_daa`]).
//! * **Turns on the honest critical path** ([`tir_public_localization_turns_v1`]): the owner's localization (the IDs, the step-tree
//!   descent, the faulty leaf, its cone in runs, the trace row/tile path), not the turn BUDGET (which lets an owner fetch every unit
//!   and stays the capital bound). Restarts are an additive allowance of whole turns (`reorg_replay_moves`), not a multiplier.
//!
//! **No value is invented.** [`PALW_TIR_FRAME_COST_RECORDS_V1`] is EMPTY: arming `palw_tir_public_conviction_v1` on a real network is
//! refused until a reviewed change pins a record for that network and height whose measurement digest is the canonical measurement
//! record (slowest supported node, release build, the network's maximum profile) and whose network inputs (inclusion margin, parallel
//! capacity, restarts) are observed on that network. [`palw_tir_frame_test_only_cost_v1`] is a TEST-ONLY config (small windows and
//! prices) that no preset and no record carries.
//!
//! No cryptographic computation proof of any kind is used (PRINCIPLES §8): these are deadlines and prices for public answers, local
//! recomputation and a deterministic court.

use crate::Hash64;
use crate::palw_rendering_resources_v1::{PalwRenderingResourcePricesV1, PalwStagedPublicCourtConfigV1};
use crate::palw_tir_public_source_v1::PalwTirPublicSourceV1;
use borsh::{BorshDeserialize, BorshSerialize};

/// The clock / cost-unit format's version.
pub const PALW_TIR_FRAME_CLOCK_VERSION_V1: u16 = 1;
/// Proposed opt-in clock for the node's accepted-offset witness pipeline.
/// V1 keeps its exact old derivation; no preset or reviewed record selects V2.
pub const PALW_TIR_FRAME_CLOCK_VERSION_V2: u16 = 2;

/// WorkDebits on a preparer's serial path per pass: ONE debit carries the whole pass (the fold counts grants, not slices; the
/// preparer's own lane bounds its cumulative work). Then its Ready.
pub const PALW_TIR_FRAME_PREPARATION_DEBITS_PER_PASS_V1: u64 = 1;
/// WorkDebits on a custodian's serial path per answered turn (its warm reopen, response build and body hash in one debit).
pub const PALW_TIR_FRAME_RESPONSE_DEBITS_PER_TURN_V1: u64 = 1;
/// WorkDebits on the outside owner's serial path per turn (its body read, authentication, probe and replay in one debit).
pub const PALW_TIR_FRAME_OWNER_DEBITS_PER_TURN_V1: u64 = 1;
/// Concurrent outside owners a frame admits (K_owner, `PalwTirPublicClaimV1::MAXIMUM_OWNERS_V1`): each custodian may answer this many
/// owners' turns at once, so the parallel inclusion term counts them.
pub const PALW_TIR_FRAME_CONCURRENT_OWNERS_V1: u64 = crate::palw_tir_public_claim_v1::PALW_TIR_PUBLIC_MAX_OWNERS_PER_CLAIM_V1;

/// **The measured per-frame cost units and network inclusion capacity** — with the frame's court config (windows, margin, restarts,
/// prices) they make the frame's prices and clocks. Carried in the acquisition plan and the source descriptor so every re-derivation
/// in the fold reproduces the accepted frame exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirFrameClockV1 {
    pub version: u16,
    /// Parser steps (the rent unit) the fold spends on ONE accepting control that re-reads no full frame, before its per-custodian
    /// growth: the slowest such action's measured fold time plus one ML-DSA-87 verification, fitted over the measured uppers and
    /// converted at the measured parse rate of one full-frame pass (the existing tariff of 32 steps per byte), rounded up.
    pub action_parser_steps: u64,
    /// Parser steps each read action adds per custodian of the frame's upper (the measured growth of a read action from the smallest
    /// to the largest measured upper: its liable-signer and contract sets), so a small Panel's frame is not charged a large one's.
    pub action_parser_steps_per_custodian: u64,
    /// Full-frame passes (structure check, copy, encode) the fold makes at an outside owner's Open and at a conviction filing.
    pub frame_passes_per_check: u64,
    /// Carrier bytes of one control besides its answer bytes (the signed control's framing and its ML-DSA-87 signature, measured).
    pub control_bytes: u64,
    /// Bytes the network includes per DAA for ONE frame's actors moving at once: a lower bound observed on the network (or, as a
    /// ceiling before that observation, the shipped block's transient-mass byte budget at one block a DAA).
    pub parallel_bytes_per_daa: u64,
    /// **The longest accepted pursuit, in DAA after the claim's acceptance** (external audit 10-17 §6.3 / §7 P0(3)): a frame whose
    /// conviction door (`collect_until − accepted`) is longer is NOT built (`DoorBeyondLiabilityHorizon`), so under G-03 that
    /// claim is not accepted at all — no reward route, no indefinitely delayed Final. Validation at arming refuses a value above
    /// the network's liability horizon `window_challenge_at(fence) + window_court` (the window in which the legacy rule could
    /// still reverse a Final, `palw_legacy_fp_liability_floor_v1`), so every accepted pursuit ends inside the horizon Final,
    /// payment and the seat locks (`bond_exit_after_daa` = the frame's material door ≥ its collect door) are already held to.
    pub maximum_door_daa: u64,
}

impl PalwTirFrameClockV1 {
    /// Parser steps one read action costs on a frame whose custodian upper is `maximum_custodians`.
    pub fn action_steps_v1(&self, maximum_custodians: u64) -> Result<u64, String> {
        sum(&[self.action_parser_steps, product(self.action_parser_steps_per_custodian, maximum_custodians)?])
    }
    /// DAA the network needs to include `moves` controls carrying `body` answer bytes in all, at the parallel capacity.
    pub fn inclusion_daa_v1(&self, moves: u64, body: u64) -> Result<u64, String> {
        Ok(sum(&[product(moves, self.control_bytes)?, body])?.div_ceil(self.parallel_bytes_per_daa))
    }
    pub fn check_v1(&self) -> Result<(), String> {
        if !matches!(self.version, PALW_TIR_FRAME_CLOCK_VERSION_V1 | PALW_TIR_FRAME_CLOCK_VERSION_V2)
            || self.action_parser_steps == 0
            || self.frame_passes_per_check == 0
            || self.control_bytes == 0
            || self.maximum_door_daa == 0
            || self.parallel_bytes_per_daa
                < self.control_bytes.saturating_add(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64)
        {
            return Err("TIR frame clock needs its version and positive measured cost units and inclusion capacity".into());
        }
        Ok(())
    }
}

/// **The frame's whole cost model**: its court config (inclusion margin, restarts, response / collection windows, retention, prices)
/// and its measured clock. A `Params` mirror (`palw_tir_frame_cost_v1`), `None` on every shipped preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirFrameCostV1 {
    pub config: PalwStagedPublicCourtConfigV1,
    pub clock: PalwTirFrameClockV1,
}

impl PalwTirFrameCostV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        self.config.check()?;
        self.clock.check_v1()
    }
}

/// **The custodian upper of an ORIGINAL TIR frame** (G14 audit §11 (d)): the flat Panel's seats and the producer.
///
/// A frame's custodians are the producer and the drawn seats that file the atomic Valid + raw custody grant + Ready
/// (`apply_grant_ready_inner_v1`); a frame claim licenses ONLY on those exact credited V1 receipts (`require_qualified_receipts_v1`),
/// while every typed segmented / shard licence is refused for it (`require_qualified_typed_receipts_v1`). So no stratified (×256)
/// or shard-licensing (×1,024) Panel can ever add a custodian beyond the flat Panel's seat count; a grant past this upper is refused by
/// the contract's own `custodians ≤ maximum_custodians`. The general staged upper (`staged_registered_tir_maximum_custodians_v1`,
/// 257 on testnet-12) is the ceiling of routes that DO take those typed licences; pricing a frame at it made every turn's
/// all-custodian inclusion term ~43× what any frame can reach. If a typed raw-grant path is ever wired for frames, this upper must
/// follow it.
pub fn palw_tir_frame_custodian_upper_v1(
    now: u64,
    params: &crate::palw_state_v2::PalwStateParamsV2,
    extras: &crate::palw_state_v2::PalwTransitionExtrasV1,
) -> Result<u64, crate::palw_state_v2::PalwStateV2Error> {
    let flat = if params.panel_v3_rule_at(now) {
        u64::from(params.panel_v3().ok_or(crate::palw_state_v2::PalwStateV2Error::ObjectiveOffenceDormant)?.policy.seat_count)
    } else {
        u64::from(extras.model_registry.as_ref().map_or(u16::MAX, |r| r.globals.seat_count))
    };
    flat.checked_add(1).ok_or(crate::palw_state_v2::PalwStateV2Error::Overflow("TIR frame custodians"))
}

/// The record format's version.
pub const PALW_TIR_FRAME_COST_RECORD_VERSION_V1: u16 = 1;

/// **A frame cost record** (the arm-readiness record format of [`crate::palw_g14_gates_v1::PalwG14ArmingRecordV1`]): the network
/// (genesis) and the exact `palw_tir_public_conviction_v1` height it prices, the source commit of the binary that measured, the
/// digest of the canonical measurement record (`misaka-palw/tir-frame-measure/v1` and the fold record, one BLAKE2b-256 over both),
/// the review's digest, and the values those measurements give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirFrameCostRecordV1 {
    pub version: u16,
    pub genesis: Hash64,
    pub activation_daa: u64,
    pub source_commit: [u8; 20],
    pub measurement: [u8; 32],
    pub review: [u8; 32],
    pub cost: PalwTirFrameCostV1,
}

/// **The pinned frame cost records — EMPTY.** No real network's frame is priced or clocked until a reviewed change pins one.
// Lane ACT2 (2026-10-27): testnet-12 DAA 17,000 only, every slot a typed waiver (crate::palw_arming_waiver_v1).
pub const PALW_TIR_FRAME_COST_RECORDS_V1: &[PalwTirFrameCostRecordV1] = crate::palw_t12_17000_records_v1::PALW_T12_17000_FRAME_COST_RECORDS_V1;

/// Why a frame cost record does not price a network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirFrameCostRefusalV1 {
    NoRecord,
    Version,
    SourceCommit,
    Measurement,
    Review,
    Values,
    Differs,
}

impl PalwTirFrameCostRefusalV1 {
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoRecord => {
                "palw_tir_public_conviction_v1 cannot be armed: no measured TIR frame cost record (PALW_TIR_FRAME_COST_RECORDS_V1 is \
                 empty) names this network and height (audit §10)"
            }
            Self::Version => "the TIR frame cost record's version is not this binary's",
            Self::SourceCommit => "the TIR frame cost record names no source commit",
            Self::Measurement => "the TIR frame cost record carries no measurement digest",
            Self::Review => "the TIR frame cost record carries no review digest, or it equals the measurement",
            Self::Values => "the TIR frame cost record's values are incomplete",
            Self::Differs => "the ruleset's TIR frame cost differs from the pinned record for this network and height",
        }
    }
}

/// **Does `records` price the frame at `activation_daa` on the network whose genesis is `genesis` with `cost`?** The first record
/// naming exactly this (network, height) decides; none is [`PalwTirFrameCostRefusalV1::NoRecord`].
pub fn palw_tir_frame_cost_record_admits_v1(
    genesis: Hash64,
    activation_daa: u64,
    cost: Option<&PalwTirFrameCostV1>,
    records: &[PalwTirFrameCostRecordV1],
) -> Result<(), PalwTirFrameCostRefusalV1> {
    let Some(record) = records.iter().find(|r| r.genesis == genesis && r.activation_daa == activation_daa) else {
        return Err(PalwTirFrameCostRefusalV1::NoRecord);
    };
    if record.version != PALW_TIR_FRAME_COST_RECORD_VERSION_V1 {
        return Err(PalwTirFrameCostRefusalV1::Version);
    }
    if record.source_commit == [0; 20] {
        return Err(PalwTirFrameCostRefusalV1::SourceCommit);
    }
    if record.measurement == [0; 32] {
        return Err(PalwTirFrameCostRefusalV1::Measurement);
    }
    if record.review == [0; 32] || record.review == record.measurement {
        return Err(PalwTirFrameCostRefusalV1::Review);
    }
    if record.cost.check_v1().is_err() {
        return Err(PalwTirFrameCostRefusalV1::Values);
    }
    if cost != Some(&record.cost) {
        return Err(PalwTirFrameCostRefusalV1::Differs);
    }
    Ok(())
}

/// **TEST-ONLY** — small windows and prices for fold and node tests (T-N1). NOT a measurement, in no preset and no record; a real
/// network that names it is refused by [`palw_tir_frame_cost_record_admits_v1`] (no record carries it).
pub const fn palw_tir_frame_test_only_cost_v1() -> PalwTirFrameCostV1 {
    PalwTirFrameCostV1 {
        config: PalwStagedPublicCourtConfigV1 {
            prices: PalwRenderingResourcePricesV1 {
                inclusion_margin_daa: 1,
                rent_per_stream_byte: 1,
                rent_per_parser_step: 1,
                rent_per_row_byte: 1,
                deposit_per_move: 1,
                reorg_replay_moves: 1,
            },
            acquisition_daa: 2,
            collection_daa: 8,
            reorg_retention_daa: 8,
            maximum_owned_rows_per_reserver: 1,
        },
        clock: PalwTirFrameClockV1 {
            version: PALW_TIR_FRAME_CLOCK_VERSION_V1,
            action_parser_steps: 1_024,
            action_parser_steps_per_custodian: 8,
            frame_passes_per_check: 1,
            control_bytes: 8_192,
            // Eight controls a DAA: the owner queue's worst wait stays a few DAA in tests.
            parallel_bytes_per_daa: 64 << 10,
            // A test genesis's own horizon; the dedicated door test uses a smaller one.
            maximum_door_daa: 1 << 40,
        },
    }
}

/// The fixed per-slice overhead a preparer's local ledger charges before each slice's own quoted work (the slice's metadata
/// hashing and its parse), shared by the node's ledger and the measurement harness.
pub const PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1: crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1 =
    crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1 {
        attempts: 1,
        io_bytes: 0,
        hash_bytes: 8192,
        copy_bytes: 0,
        parser_steps: 1024,
        id_copy_words: 0,
        tree_hashes: 0,
        replay_leaves: 0,
        replay_positions: 0,
        replay_id_words: 0,
        arithmetic_work: 0,
        traffic_bytes: 0,
        eval_elements: 0,
        eval_terms: 0,
        source_reads: 0,
        target_requests: 0,
    };

/// **One preparation slice's work as a preparer's ledger counts it** (the backend's quote, 16 parser steps per IO byte), the
/// overhead [`PALW_TIR_PUBLIC_SLICE_OVERHEAD_V1`] excluded.
pub fn tir_public_slice_work_v1(
    q: &crate::palw_backend::PalwTirPublicCaptureSliceV1,
) -> Result<crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1, String> {
    Ok(crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1 {
        attempts: 1,
        io_bytes: q.work.io_bytes,
        hash_bytes: q.work.hash_bytes,
        parser_steps: q.work.io_bytes.checked_mul(16).ok_or("TIR preparation slice parser work overflow")?,
        tree_hashes: q.work.tree_hashes,
        replay_leaves: q.work.replay_leaves,
        replay_positions: q.work.replay_positions,
        replay_id_words: q.work.replay_id_words,
        id_copy_words: q.work.replay_id_words,
        arithmetic_work: q.work.arithmetic_work,
        traffic_bytes: q.work.traffic_bytes,
        ..Default::default()
    })
}

/// **The ONE WorkDebit a preparer files for a whole pass** (audit §10): its lane (`per_actor`, bought for `passes` passes)
/// divided by the passes, as ONE grant (`attempts = 1`). Every slice of the pass — the cold capture, then the prepared
/// generation's publication — runs under that one permit, its local ledger charging each slice's quoted work and overhead
/// against these quantities (the slice count is not a resource: the node's ledger does not count attempts within a pass).
/// The measurement harness asserts a real pass fits it on every corpus class.
pub fn tir_public_pass_budget_v1(
    per_actor: crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1,
    passes: u64,
) -> Result<crate::palw_tir_public_owned_v1::PalwTirPublicWorkV1, String> {
    let mut budget = per_actor.div_floor_v1(passes)?;
    budget.attempts = 1;
    Ok(budget)
}

fn sum(values: &[u64]) -> Result<u64, String> {
    values.iter().try_fold(0u64, |total, value| total.checked_add(*value).ok_or_else(|| "TIR frame clock sum overflow".into()))
}
fn product(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "TIR frame clock product overflow".into())
}
fn height(count: u64) -> u64 {
    if count <= 1 { 0 } else { u64::from(64 - (count - 1).leading_zeros()) }
}
/// Turns of a node descent of a tree of `count` leaves, `DEPTH` levels per node request, plus the leaf level.
fn descent(count: u64) -> u64 {
    height(count).div_ceil(u64::from(crate::palw_tir_court_v1::PALW_TIR_STEP_NODE_DEPTH_V1)) + 1
}

/// **The honest outside owner's turns on its critical path** — a deterministic function of the original source only:
///
/// * the complete committed IDs (one demand; its 1,024-ID chunks are the answer's parts) — the owner compares them with its own
///   replay first (G-T2);
/// * the step-tree descent to the first differing leaf ([`PalwTirPublicSourceV1::step_descent_units_v1`]), the leaf itself;
/// * the leaf's cone, in full runs of `PALW_TIR_STEP_RUN_MAX_LEAVES_V1` leaves: [`PalwTirPublicSourceV1::cone_runs_v1`], derived from
///   the program's dependency structure (checkpoint distance for state, the history tile and the attention window), never more than
///   every leaf of the claim in runs (G14 audit §11 (b));
/// * a decode-selection fault's trace path: the rows descent, the tiles descent and every tile of one row;
/// * one turn for the output / stop-rule check.
pub fn tir_public_localization_turns_v1(source: &PalwTirPublicSourceV1) -> Result<u64, String> {
    let n = source.leaf_count_v1();
    let d = u64::from(source.decode_count_v1());
    let run = u64::from(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1);
    let (ids, trace) = if source.supports_tiled_trace_v1() && d > 0 {
        let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        // The owned IDs are ONE demand (`Ids { offset: 0 }` acquires the complete sequence; its chunks are the answer's parts).
        (1, sum(&[descent(d), descent(tiles), tiles])?)
    } else {
        (0, 0)
    };
    let _ = (n, run);
    sum(&[ids, source.step_descent_units_v1(), 1, source.cone_runs_v1(), trace, 1])
}

/// **One turn's span for a request whose answer body is `body_bytes`** (`slots` answering literal custodians and the owner):
/// the owner's Demand and its own per-turn debit, then ONE custodian's serial chain (its per-turn debit, Begin, each 16 KiB part,
/// Finish) — custodians answer in parallel, so the span does not grow with them — one response window `acquisition_daa` for the
/// answer's build, and the time the network needs to include every custodian's chain for every concurrent owner at its measured
/// parallel capacity.
pub fn tir_public_turn_span_v1(
    body_bytes: u64,
    slots: u64,
    config: &PalwStagedPublicCourtConfigV1,
    clock: &PalwTirFrameClockV1,
) -> Result<u64, String> {
    clock.check_v1()?;
    let parts = body_bytes.div_ceil(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64).max(1);
    let custodian = sum(&[PALW_TIR_FRAME_RESPONSE_DEBITS_PER_TURN_V1, 2, parts])?;
    let owner = sum(&[1, PALW_TIR_FRAME_OWNER_DEBITS_PER_TURN_V1])?;
    let all = product(slots, PALW_TIR_FRAME_CONCURRENT_OWNERS_V1)?;
    let parallel = clock.inclusion_daa_v1(product(all, custodian)?, product(all, body_bytes)?)?;
    sum(&[config.acquisition_daa, product(sum(&[owner, custodian])?, config.prices.inclusion_margin_daa)?, parallel])
}

/// **The acquisition's critical path: `(turns, DAA)`** — the honest owner's localization turns ([`tir_public_localization_turns_v1`])
/// each at the span of ITS request's largest answer (the IDs, a root-level step node, the leaf, a full run of the cone, a rows /
/// tile node, a trace tile — `request_body_bound_v1`, the same bound every demand's span is computed from), plus
/// `reorg_replay_moves` restart turns at the widest of them (a reorg undoes at most the turns it rolls back and the same signed moves
/// are re-included). Before the frame pricing fix every one of `fresh × 65` budgeted turns stood end to end at `acquisition_daa`
/// plus a margin per move of every slot.
pub fn tir_public_localization_path_v1(
    source: &PalwTirPublicSourceV1,
    maximum_custodians: u64,
    config: &PalwStagedPublicCourtConfigV1,
    clock: &PalwTirFrameClockV1,
) -> Result<(u64, u64), String> {
    use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as R;
    let slots = maximum_custodians.checked_add(1).ok_or("TIR frame slots overflow")?;
    let span = |request: R| -> Result<u64, String> {
        let (bytes, _) = crate::palw_tir_public_owned_v1::request_body_bound_v1(source, request)?;
        tir_public_turn_span_v1(bytes, slots, config, clock)
    };
    let level = |count: u64| height(count).min(u64::from(crate::palw_tir_court_v1::PALW_TIR_STEP_NODE_DEPTH_V1)) as u8;
    // (turns, DAA, widest span) accumulated over the path.
    let mut path = (0u64, 0u64, 0u64);
    let add = |path: &mut (u64, u64, u64), count: u64, each: u64| -> Result<(), String> {
        path.0 = sum(&[path.0, count])?;
        path.1 = sum(&[path.1, product(count, each)?])?;
        path.2 = path.2.max(each);
        Ok(())
    };
    let n = source.leaf_count_v1();
    let d = u64::from(source.decode_count_v1());
    let tiled = source.supports_tiled_trace_v1() && d > 0;
    if tiled {
        add(&mut path, 1, span(R::Ids { offset: 0 })?)?;
    }
    let leaf = span(R::StepLeaf { index: 0 })?;
    let node = if n > 1 { span(R::StepNode { level: level(n), index: 0 })? } else { leaf };
    add(&mut path, source.step_descent_units_v1(), node)?;
    add(&mut path, 1, leaf)?;
    let run = u32::try_from(n.clamp(1, u64::from(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1)))
        .map_err(|_| "TIR run overflow")?;
    let cone = if run > 1 { span(R::StepRun { first: 0, count: run })? } else { leaf };
    // §11 (b): the cone's runs from the program's dependency structure (checkpoint distance, history tile and attention window),
    // never more than every leaf in runs.
    add(&mut path, source.cone_runs_v1(), cone)?;
    if tiled {
        let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
        let tile = span(R::TraceTile { row: 0, tile: 0 })?;
        add(&mut path, descent(d), if d > 1 { span(R::RowsNode { level: level(d), index: 0 })? } else { tile })?;
        add(&mut path, descent(tiles), if tiles > 1 { span(R::TileNode { row: 0, level: level(tiles), index: 0 })? } else { tile })?;
        add(&mut path, tiles, tile)?;
    }
    // The output / stop-rule check, then the restart turns, each at the widest span on the path.
    let widest = path.2;
    add(&mut path, 1, widest)?;
    add(&mut path, config.prices.reorg_replay_moves, widest)?;
    if path.0 != sum(&[tir_public_localization_turns_v1(source)?, config.prices.reorg_replay_moves])? {
        return Err("TIR localization path disagrees with its turn count".into());
    }
    Ok((path.0, path.1))
}

/// **The preparation door** (after the claim's acceptance): one response window for the pass's build, then each preparer's serial
/// chain — one batched debit per pass (its first pass and `reorg_replay_moves` restarts) and its Ready — every preparer at once,
/// included at the measured parallel capacity.
pub fn tir_public_preparation_door_v1(
    maximum_custodians: u64,
    config: &PalwStagedPublicCourtConfigV1,
    clock: &PalwTirFrameClockV1,
) -> Result<u64, String> {
    clock.check_v1()?;
    let passes = config.prices.reorg_replay_moves.checked_add(1).ok_or("TIR preparation passes overflow")?;
    let serial = sum(&[product(passes, PALW_TIR_FRAME_PREPARATION_DEBITS_PER_PASS_V1)?, 1])?;
    let parallel = clock.inclusion_daa_v1(product(maximum_custodians, serial)?, 0)?;
    sum(&[config.acquisition_daa, product(serial, config.prices.inclusion_margin_daa)?, parallel])
}

/// **The owner queue's worst wait** (G14 audit §11 (c), with the owner fair start of lane g14-court-capacity-r2): an Open past
/// K_owner waits in the claim's owner queue (arrival order) and is admitted within `⌈(q + 1) / K_owner⌉` tenures, a tenure being
/// `max(idle grace, open window / 2·K_owner)` (`tir_owner_tenure_v1` there; the grace is `max(reorg_retention, inclusion margin)`,
/// `owner_idle_grace_daa_v1`). Opens are accepted only inside the Open window, and the network includes at most
/// `⌊parallel_bytes_per_daa / control_bytes⌋` controls a DAA, so at most `q = open window × that` rows can be ahead of an honest
/// Open. The stream door holds this wait before the honest owner's localization path, so a newcomer admitted last still finishes.
/// Waiting rows demand nothing: the custodian load stays K_owner (the turn span's concurrency term).
pub fn tir_public_owner_queue_wait_v1(
    open_window: u64,
    config: &PalwStagedPublicCourtConfigV1,
    clock: &PalwTirFrameClockV1,
) -> Result<u64, String> {
    clock.check_v1()?;
    let k = PALW_TIR_FRAME_CONCURRENT_OWNERS_V1;
    // The court-capacity lane's own rules (single source): its idle grace and its tenure.
    let grace = crate::palw_tir_public_owned_v1::palw_tir_owner_idle_grace_v1(config);
    let tenure = crate::palw_tir_public_claim_v1::palw_tir_owner_tenure_of_window_v1(open_window, grace);
    let ahead = product(open_window, clock.parallel_bytes_per_daa / clock.control_bytes)?;
    product(sum(&[ahead, 1])?.div_ceil(k), tenure)
}

/// **Serial cone rounds of the bounded-round localisation (ADR-0190 step 1, DORMANT).** The disputed step's derived operand closure
/// is fetched in ONE batched request (round 1), and the value-dependent follow-ups in ONE request (round 2): the serial cone rounds
/// are this constant whatever the run count; bytes per round are carried by parts.
pub const PALW_TIR_BOUNDED_CONE_ROUNDS_V1: u64 = 2;

/// The bounded-round localisation path's breakdown ([`tir_public_localization_path_bounded_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirBoundedPathV1 {
    pub turns: u64,
    pub daa: u64,
    pub cone_runs: u64,
    pub round1_body: u64,
    pub round2_body: u64,
    pub round1_daa: u64,
    pub round2_daa: u64,
    pub output_check_daa: u64,
    pub restart_daa: u64,
    pub gather_bytes: u64,
}

/// **The bounded-round localisation path (ADR-0190; DORMANT: no preset selects it; the legacy path is untouched).**
///
/// Each term is priced from its own request span (no widest-turn reuse, ADR-0190 step 4):
/// * IDs and the step-tree descent to the first differing leaf, as the legacy path (bounded by tree height, not by runs);
/// * the faulty leaf (one turn);
/// * round 1: ONE batched request carrying the whole cone (`cone_runs × run body`) plus the gather / model-parameter operand bytes;
/// * round 2: ONE request carrying the leaf and the value-dependent trace path (rows descent, tile descent, every tile of one row);
/// * the output / stop-rule check: its own request, one trace tile;
/// * `reorg_replay_moves` restarts, each replaying the two cone rounds (2 turns per restart), at their own spans.
///
/// `uncovered_gather_bytes` is the gather / model-parameter operand bound, which the structural analyser does not yet derive. `None`
/// refuses the frame: the term is never assumed to be 0.
pub fn tir_public_localization_path_bounded_v1(
    source: &PalwTirPublicSourceV1,
    maximum_custodians: u64,
    config: &PalwStagedPublicCourtConfigV1,
    clock: &PalwTirFrameClockV1,
    uncovered_gather_bytes: Option<u64>,
) -> Result<PalwTirBoundedPathV1, String> {
    use crate::palw_tir_public_source_v1::PalwTirPublicRequestV1 as R;
    let gather = uncovered_gather_bytes.ok_or(
        "TIR bounded door: gather / model-parameter operand bytes are not bounded (ADR-0190 term); the frame is refused, not priced at 0",
    )?;
    let slots = maximum_custodians.checked_add(1).ok_or("TIR frame slots overflow")?;
    let body = |request: R| -> Result<u64, String> { Ok(crate::palw_tir_public_owned_v1::request_body_bound_v1(source, request)?.0) };
    let span = |bytes: u64| tir_public_turn_span_v1(bytes, slots, config, clock);
    let level = |count: u64| height(count).min(u64::from(crate::palw_tir_court_v1::PALW_TIR_STEP_NODE_DEPTH_V1)) as u8;
    let n = source.leaf_count_v1();
    let d = u64::from(source.decode_count_v1());
    let tiled = source.supports_tiled_trace_v1() && d > 0;
    let tiles = u64::from(source.vocab_v1()).div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64);
    let run = u32::try_from(n.clamp(1, u64::from(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1)))
        .map_err(|_| "TIR run overflow")?;
    let leaf_body = body(R::StepLeaf { index: 0 })?;
    let tile_body = body(R::TraceTile { row: 0, tile: 0 })?;
    let run_body = if run > 1 { body(R::StepRun { first: 0, count: run })? } else { leaf_body };
    let cone_runs = source.cone_runs_v1();

    let ids_daa = if tiled { span(body(R::Ids { offset: 0 })?)? } else { 0 };
    let node_body = if n > 1 { body(R::StepNode { level: level(n), index: 0 })? } else { leaf_body };
    let descent_turns = source.step_descent_units_v1();
    let descent_daa = product(descent_turns, span(node_body)?)?;
    let leaf_daa = span(leaf_body)?;

    let round1_body = sum(&[product(cone_runs, run_body)?, gather])?;
    let round1_daa = span(round1_body)?;
    let trace_body = if tiled {
        let rows_node = if d > 1 { body(R::RowsNode { level: level(d), index: 0 })? } else { tile_body };
        let tile_node = if tiles > 1 { body(R::TileNode { row: 0, level: level(tiles), index: 0 })? } else { tile_body };
        sum(&[product(descent(d), rows_node)?, product(descent(tiles), tile_node)?, product(tiles, tile_body)?])?
    } else {
        0
    };
    let round2_body = sum(&[leaf_body, trace_body])?;
    let round2_daa = span(round2_body)?;
    let output_check_daa = span(tile_body)?;

    let restarts = config.prices.reorg_replay_moves;
    let restart_daa = product(restarts, sum(&[round1_daa, round2_daa])?)?;
    let restart_turns = product(restarts, PALW_TIR_BOUNDED_CONE_ROUNDS_V1)?;
    let turns = sum(&[u64::from(tiled), descent_turns, 1, PALW_TIR_BOUNDED_CONE_ROUNDS_V1, 1, restart_turns])?;
    let daa = sum(&[ids_daa, descent_daa, leaf_daa, round1_daa, round2_daa, output_check_daa, restart_daa])?;
    Ok(PalwTirBoundedPathV1 {
        turns,
        daa,
        cone_runs,
        round1_body,
        round2_body,
        round1_daa,
        round2_daa,
        output_check_daa,
        restart_daa,
        gather_bytes: gather,
    })
}

/// **ADR-0191 dispute door (DORMANT, derivation).** Every term is declared: the span fit `span(B) = 44 + ceil(B / 920)` (from ADR-0190's
/// two measured one-round points), one bisection round of 46 DAA, the preparation (16) and the output check (53) of ADR-0191.
pub const PALW_TIR_DISPUTE_SPAN_INTERCEPT_DAA_V1: u64 = 44;
pub const PALW_TIR_DISPUTE_SPAN_BYTES_PER_DAA_V1: u64 = 920;
pub const PALW_TIR_DISPUTE_ROUND_DAA_V1: u64 = 46;
pub const PALW_TIR_DISPUTE_PREP_DAA_V1: u64 = 16;
pub const PALW_TIR_DISPUTE_OUTPUT_DAA_V1: u64 = 53;
/// The bisection arity of every level (ADR-0191 (1)).
pub const PALW_TIR_DISPUTE_ARITY_V1: u64 = 16;

/// The dispute door for one adjudication shape, with `levels` extra bisection levels below the shape's own rounds.
///
/// * `rounds`: the dependent rounds before the adjudication (positions, attention tiles, step ops, vocabulary tiles);
/// * `reduction_blocks`: the byte sizes of the divisible operand blocks (attention tile, GDN state block, matmul block); each one
///   shrinks by `PALW_TIR_DISPUTE_ARITY_V1` per extra level, rounded up;
/// * `floor_bytes`: the indivisible part (one leaf and one logits tile), which no level shrinks;
/// * `contenders`: owners per claim (`K_owner`), each of whose queued adjudication waits one budget;
/// * the door is `prep + (rounds + levels)·round + output + (K + 1)·span(B)`: own adjudication, restart, and the `(K − 1)` queued budgets.
///
/// Returns `None` on overflow. The adjudication itself is still a deterministic recomputation of the block from authenticated
/// operands; this function only prices it.
pub fn palw_tir_dispute_door_fit_v1(rounds: u64, levels: u32, reduction_blocks: &[u64], floor_bytes: u64, contenders: u64) -> Option<u64> {
    let mut block = floor_bytes;
    let divisor = PALW_TIR_DISPUTE_ARITY_V1.checked_pow(levels)?;
    for bytes in reduction_blocks {
        block = block.checked_add(bytes.div_ceil(divisor))?;
    }
    let span = PALW_TIR_DISPUTE_SPAN_INTERCEPT_DAA_V1.checked_add(block.div_ceil(PALW_TIR_DISPUTE_SPAN_BYTES_PER_DAA_V1))?;
    let rounds = rounds.checked_add(u64::from(levels))?.checked_mul(PALW_TIR_DISPUTE_ROUND_DAA_V1)?;
    let fixed = PALW_TIR_DISPUTE_PREP_DAA_V1.checked_add(PALW_TIR_DISPUTE_OUTPUT_DAA_V1)?;
    let carriers = contenders.checked_add(1)?.checked_mul(span)?;
    fixed.checked_add(rounds)?.checked_add(carriers)
}

/// The extra levels in `0..=max_levels` that minimise the door, with the door (ties take the fewer levels).
pub fn palw_tir_dispute_door_minimum_v1(rounds: u64, reduction_blocks: &[u64], floor_bytes: u64, contenders: u64, max_levels: u32) -> Option<(u32, u64)> {
    let mut best: Option<(u32, u64)> = None;
    for levels in 0..=max_levels {
        let door = palw_tir_dispute_door_fit_v1(rounds, levels, reduction_blocks, floor_bytes, contenders)?;
        if best.is_none_or(|(_, d)| door < d) {
            best = Some((levels, door));
        }
    }
    best
}

/// **The filing allowance** after the stream door: the filer's witness parts (in order) and its conviction, at the measured capacity.
pub fn tir_public_filing_daa_v1(
    witness_moves: u64,
    config: &PalwStagedPublicCourtConfigV1,
    clock: &PalwTirFrameClockV1,
) -> Result<u64, String> {
    clock.check_v1()?;
    let parts = product(witness_moves, crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64)?;
    // V1's fixed two margins must remain reproducible for signed old frames.
    // The current node filer waits for the accepted offset before sending the
    // next part. V2 therefore reserves a margin per possible carried move; the
    // aggregate byte term cannot replace this serial dependency. maximum_moves
    // is conservative (it also covers terminal controls/retries), never raised
    // here. Original maximum_door still rejects a frame that no longer fits.
    let serial = match clock.version {
        PALW_TIR_FRAME_CLOCK_VERSION_V1 => 2,
        PALW_TIR_FRAME_CLOCK_VERSION_V2 if witness_moves > 0 => witness_moves.max(2),
        _ => return Err("sequential filing needs at least one carried move".into()),
    };
    sum(&[product(serial, config.prices.inclusion_margin_daa)?, clock.inclusion_daa_v1(witness_moves, parts)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_filing_v2_keeps_v1_bytes_and_cannot_erase_serial_wait() {
        let cost = palw_tir_frame_test_only_cost_v1();
        let bytes = borsh::to_vec(&cost).unwrap();
        let moves = 100;
        let body = moves * crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64;
        let old = tir_public_filing_daa_v1(moves, &cost.config, &cost.clock).unwrap();
        assert_eq!(old, 2 * cost.config.prices.inclusion_margin_daa + cost.clock.inclusion_daa_v1(moves, body).unwrap());
        let mut v2 = cost;
        v2.clock.version = PALW_TIR_FRAME_CLOCK_VERSION_V2;
        v2.check_v1().unwrap();
        assert_ne!(borsh::to_vec(&v2).unwrap(), bytes, "explicit version binds the changed rule");
        assert!(tir_public_filing_daa_v1(moves, &v2.config, &v2.clock).unwrap() >= moves * v2.config.prices.inclusion_margin_daa);
        // Synthetic stress capacity only: high throughput does not remove the
        // actual filer's accepted-cursor dependency. No production value set.
        v2.clock.parallel_bytes_per_daa = 64 << 20;
        assert_eq!(tir_public_filing_daa_v1(moves, &v2.config, &v2.clock).unwrap(), 101);
        assert!(tir_public_filing_daa_v1(0, &v2.config, &v2.clock).is_err());
        assert_eq!(borsh::to_vec(&cost).unwrap(), bytes);
        assert_eq!(tir_public_filing_daa_v1(moves, &cost.config, &cost.clock).unwrap(), old);
        assert_eq!(
            palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, Some(&v2), PALW_TIR_FRAME_COST_RECORDS_V1),
            Err(PalwTirFrameCostRefusalV1::NoRecord)
        );
    }

    #[test]
    fn sequential_filing_v2_flows_through_real_plan_and_keeps_door_cap() {
        use crate::palw_tir_public_allocation_v1::{
            PalwTirPublicAllocationMissingV1, PalwTirPublicAllocationQuoteV1, derive_tir_public_allocation_v1,
        };
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_v1(None, 4);
        let frame = &fx.frame;
        let original = frame.original_v1();
        let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
            class_id: original.job.class_id,
            producer: frame.custody_v1().pledge_v1().producer,
            execution_root: original.execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original),
            accepted_daa: frame.plan_v1().terms.accepted_daa,
        };
        let quote = |rules| {
            derive_tir_public_allocation_v1(&geometry, original, frame.header_v1(), &fx.record, &fx.layout, None, None, rules).unwrap()
        };
        let PalwTirPublicAllocationQuoteV1::Complete(v1) = quote(fx.rules) else { panic!("V1 fixture plan") };
        assert_eq!(borsh::to_vec(&v1).unwrap(), borsh::to_vec(frame.plan_v1()).unwrap(), "V1 plan rederives byte-for-byte");
        // The original mixed fixture has a long network_v1 collection window;
        // it can dominate both filing allowances. Verify that max explicitly.
        let mut rules = fx.rules;
        rules.clock.version = PALW_TIR_FRAME_CLOCK_VERSION_V2;
        let PalwTirPublicAllocationQuoteV1::Complete(covered) = quote(rules) else { panic!("covered V2 plan") };
        let filing = tir_public_filing_daa_v1(covered.terminal.envelope.maximum_moves, &rules.config, &rules.clock).unwrap();
        assert_eq!(covered.terms.collect_until_daa, covered.terms.stream_until_daa + rules.config.collection_daa.max(filing));
        assert!(covered.terms.collect_until_daa >= v1.terms.collect_until_daa);
        // Same source/class, using the EXISTING synthetic small-window config
        // for both versions; no production value or ceiling is changed.
        rules = fx.rules;
        rules.config = palw_tir_frame_test_only_cost_v1().config;
        let PalwTirPublicAllocationQuoteV1::Complete(v1) = quote(rules) else { panic!("small-window V1 plan") };
        rules.clock.version = PALW_TIR_FRAME_CLOCK_VERSION_V2;
        let PalwTirPublicAllocationQuoteV1::Complete(v2) = quote(rules) else { panic!("small-window V2 plan") };
        assert_eq!(v1.terms.stream_until_daa, v2.terms.stream_until_daa);
        assert!(v2.terms.collect_until_daa > v1.terms.collect_until_daa);
        assert_eq!(v2.terms.material_until_daa, v2.terms.collect_until_daa + rules.config.reorg_retention_daa);
        assert_eq!(v2.terms.bond_exit_after_daa, v2.terms.material_until_daa);
        assert_eq!(v1.source_id, v2.source_id);
        assert_ne!(v1.resource_plan_id, v2.resource_plan_id);
        rules.clock.maximum_door_daa = v1.terms.collect_until_daa - geometry.accepted_daa;
        assert!(
            matches!(quote(rules), PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. }
            if missing.iter().any(|m| matches!(m, PalwTirPublicAllocationMissingV1::DoorBeyondLiabilityHorizon { .. }))),
            "the unchanged cap refuses a longer required door"
        );
    }

    #[test]
    fn the_cost_record_list_is_empty_and_refuses_every_network() {
        assert!(PALW_TIR_FRAME_COST_RECORDS_V1.iter().all(|r| r.activation_daa == 16_600 && r.genesis == crate::config::genesis::PALW_T12_GENESIS.hash));
        let cost = palw_tir_frame_test_only_cost_v1();
        cost.check_v1().expect("the test-only config is well formed");
        assert_eq!(
            palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, Some(&cost), PALW_TIR_FRAME_COST_RECORDS_V1),
            Err(PalwTirFrameCostRefusalV1::NoRecord)
        );
        // A well-formed record admits exactly its own values, network and height.
        let record = PalwTirFrameCostRecordV1 {
            version: PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
            genesis: Hash64::from_u64_word(1),
            activation_daa: 16_000,
            source_commit: [1; 20],
            measurement: [2; 32],
            review: [3; 32],
            cost,
        };
        let records = [record];
        assert_eq!(palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, Some(&cost), &records), Ok(()));
        assert_eq!(
            palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_001, Some(&cost), &records),
            Err(PalwTirFrameCostRefusalV1::NoRecord)
        );
        let mut other = cost;
        other.clock.parallel_bytes_per_daa += 1;
        assert_eq!(
            palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, Some(&other), &records),
            Err(PalwTirFrameCostRefusalV1::Differs)
        );
        assert_eq!(
            palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, None, &records),
            Err(PalwTirFrameCostRefusalV1::Differs)
        );
        let unreviewed = [PalwTirFrameCostRecordV1 { review: [2; 32], ..record }];
        assert_eq!(
            palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, Some(&cost), &unreviewed),
            Err(PalwTirFrameCostRefusalV1::Review)
        );
    }

    #[test]
    fn a_turn_span_does_not_grow_with_custodians_end_to_end() {
        let cost = palw_tir_frame_test_only_cost_v1();
        let six = tir_public_turn_span_v1(700, 7, &cost.config, &cost.clock).unwrap();
        let wide = tir_public_turn_span_v1(700, 258, &cost.config, &cost.clock).unwrap();
        // Serial: owner (Demand + debit) and ONE custodian chain (debit, Begin, 1 part, Finish) at margin 1, window 2.
        assert_eq!(six, 2 + 6 + (7 * 8 * (4 * 8_192 + 700u64)).div_ceil(64 << 10));
        // 257 custodians add only their inclusion at the parallel capacity, not 257 serial chains.
        assert_eq!(wide, 2 + 6 + (258 * 8 * (4 * 8_192 + 700u64)).div_ceil(64 << 10));
        assert!(wide < 258 * 4 * 8, "not end to end");
    }

    #[test]
    fn the_dispute_door_is_within_3120_at_32k_and_2m_at_the_minimising_level() {
        // ADR-0191: attention tile 65,536 B, GDN state block 65,536 B, matmul block 131,072 B; one leaf + one logits tile (16,384 B) floor.
        let blocks = [65_536u64, 65_536, 131_072];
        let floor = 16_384u64;
        // The declared fit reproduces the ADR's values.
        assert_eq!(palw_tir_dispute_door_fit_v1(12, 0, &blocks, floor, 8), Some(3_744), "32K, no extra level: over 3,120");
        assert_eq!(palw_tir_dispute_door_fit_v1(16, 0, &blocks, floor, 8), Some(3_928), "2M, no extra level: over 3,120");
        // The minimising extra level is the same at both sizes (two levels: block 17,408 B).
        assert_eq!(palw_tir_dispute_door_minimum_v1(12, &blocks, floor, 8, 7), Some((2, 1_280)), "32K minimum");
        assert_eq!(palw_tir_dispute_door_minimum_v1(16, &blocks, floor, 8, 7), Some((2, 1_464)), "2M minimum");
        assert!(palw_tir_dispute_door_fit_v1(16, 2, &blocks, floor, 8).unwrap() <= 3_120, "2M fits 3,120 at the minimum");
        // Fewer owners is NOT the fix (ADR-0191 decision): K = 8 is kept above.
        assert!(palw_tir_dispute_door_fit_v1(16, 2, &blocks, floor, 8).unwrap() > palw_tir_dispute_door_fit_v1(16, 2, &blocks, floor, 4).unwrap());
        // The floor is never shrunk: more levels stop helping once only the floor remains.
        let at7 = palw_tir_dispute_door_fit_v1(16, 7, &blocks, floor, 8).unwrap();
        let at2 = palw_tir_dispute_door_fit_v1(16, 2, &blocks, floor, 8).unwrap();
        assert!(at7 > at2, "extra levels past the minimum only add rounds");
    }

    /// **derived_upper (formula, not measured).** The bounded TIR public door for the Huihui-Qwen3.5-2B 32K class and its 2M
    /// extension under ADR-0191: b = 16 bisection, M = 1 admission, L = 2 extra levels (17,408 B step block), history-tile
    /// bisection (2,048 tiles = 3 rounds at 32K, 65,536 tiles = 4 rounds at 2M), and the terminal history read of ~135.6 KB.
    /// Only the existing formula functions and constants are used; nothing is measured and no rule or constant is changed.
    /// Prints every term. Asserts only arithmetic identities, never that the door is at or below 3,120.
    #[test]
    fn bounded_door_derived_upper_32k_and_2m_prints_every_term() {
        use crate::palw_tir_history_bisection_v1::palw_tir_history_rounds_v1;
        let ceil_log16 = |n: u64| -> u64 {
            let (mut rounds, mut reach) = (0u64, 1u64);
            while reach < n {
                reach = reach.saturating_mul(16);
                rounds += 1;
            }
            rounds
        };
        let k = PALW_TIR_FRAME_CONCURRENT_OWNERS_V1;
        let levels = 2u32;
        // Step reduction blocks (attention tile, GDN state block, matmul block) and the indivisible floor (one leaf + one logits tile).
        let blocks = [65_536u64, 65_536, 131_072];
        let floor = 16_384u64;
        // Non-cone carry-over terms of the shipped door that the dispute door does not replace (door-breakdown-1018): IDs 106,
        // filing allowance 82, open window 5. The shipped trace tiles (4,209), output (2,346), reorg (2,346) and the owner queue
        // (1,536) are replaced by the dispute formula's own terms, so they are not added here.
        let carry_over = [("IDs / leaf (door-breakdown-1018)", 106u64), ("filing allowance (door-breakdown-1018)", 82), ("open window", 5)];
        let carry_sum: u64 = carry_over.iter().map(|(_, v)| v).sum();
        // Terminal history read (given: ~135.6 KB; not recomputed here). Own adjudication plus one restart.
        let terminal_bytes = 135_600u64;
        // Position terminal (one more round, then one position): given ~12 KB, not derived from the shape.
        const POSITION_TERMINAL_BYTES_GIVEN: u64 = 12_000;
        let vocab_tiles = 248_320u64.div_ceil(4_096);
        let r_op = ceil_log16(1_024);
        let r_tile = ceil_log16(vocab_tiles);
        let cases = [("32K", 32_768u64, 2_048u64), ("2M", 2_097_152u64, 131_072u64)];
        for (label, positions, tiles) in cases {
            let r_pos = ceil_log16(positions);
            let r_hist = u64::from(palw_tir_history_rounds_v1(tiles));
            assert_eq!(r_hist, ceil_log16(tiles), "history rounds are ceil(log16 tiles)");
            let rounds = r_pos + r_hist + r_op + r_tile;
            println!("[{label}] derived_upper (formula, not measured)");
            println!("[{label}] positions {positions}, history tiles {tiles}, vocab tiles {vocab_tiles}, K_owner {k}, M 1, b 16");
            println!("[{label}] rounds: R_pos {r_pos} + R_hist {r_hist} + R_op {r_op} + R_tile {r_tile} = {rounds}");
            // The scale 1 is the stated 125,000 B/DAA sensitivity; the scale 10 is 1/10 of it (bytes multiplied by 10).
            for scale in [1u64, 10] {
                let blocks_s = blocks.map(|b| b * scale);
                let floor_s = floor * scale;
                let terminal_s = terminal_bytes * scale;
                let adj_block = floor_s + blocks_s.iter().map(|b| b.div_ceil(16u64.pow(levels))).sum::<u64>();
                let span_adj = PALW_TIR_DISPUTE_SPAN_INTERCEPT_DAA_V1 + adj_block.div_ceil(PALW_TIR_DISPUTE_SPAN_BYTES_PER_DAA_V1);
                let terminal_span = PALW_TIR_DISPUTE_SPAN_INTERCEPT_DAA_V1 + terminal_s.div_ceil(PALW_TIR_DISPUTE_SPAN_BYTES_PER_DAA_V1);
                let core = palw_tir_dispute_door_fit_v1(rounds, levels, &blocks_s, floor_s, k).expect("the door fits in u64");
                // Identity: prep + (R + L)·46 + output + (K + 1)·span(B_adj).
                assert_eq!(
                    core,
                    PALW_TIR_DISPUTE_PREP_DAA_V1
                        + (rounds + u64::from(levels)) * PALW_TIR_DISPUTE_ROUND_DAA_V1
                        + PALW_TIR_DISPUTE_OUTPUT_DAA_V1
                        + (k + 1) * span_adj,
                    "core door identity"
                );
                let door_a = core + carry_sum;
                let terminal_extra = 2 * terminal_span;
                let door_b = door_a + terminal_extra;
                let tag = if scale == 1 { "125,000 B/DAA (sensitivity)" } else { "12,500 B/DAA (1/10 sensitivity)" };
                println!("[{label}] --- {tag}, bytes scaled x{scale} ---");
                println!("[{label}]   step block B_adj = {adj_block} B (floor {floor_s} + blocks {blocks_s:?} / 16^{levels} rounded up)");
                println!("[{label}]   span(B_adj) = 44 + ceil(B/920) = {span_adj}");
                println!("[{label}]   prep = {PALW_TIR_DISPUTE_PREP_DAA_V1}");
                println!("[{label}]   bisection = (R + L) x 46 = {} x 46 = {}", rounds + u64::from(levels), (rounds + u64::from(levels)) * PALW_TIR_DISPUTE_ROUND_DAA_V1);
                println!("[{label}]   output check = {PALW_TIR_DISPUTE_OUTPUT_DAA_V1}");
                println!("[{label}]   admission queue + own + restart = (K + 1) x span(B_adj) = {} x {span_adj} = {}", k + 1, (k + 1) * span_adj);
                println!("[{label}]   core door (existing fn palw_tir_dispute_door_fit_v1) = {core}");
                for (name, v) in carry_over {
                    println!("[{label}]   carry-over {name} = {v}");
                }
                println!("[{label}]   door_A (no terminal) = {door_a}");
                println!("[{label}]   terminal history span(B={terminal_s}) = 44 + ceil(B/920) = {terminal_span}; own + restart = {terminal_extra}");
                println!("[{label}]   door_B (with terminal) = {door_b}");
                println!("[{label}]   replaced, not added: shipped trace 4,209, output 2,346, reorg 2,346, owner queue 1,536");
                // **Position terminal row (DORMANT, derived_upper).** The history bisection localises one tile, then ONE more
                // b = 16 round over its 16 slot partials localises one position; the terminal recomputes that position from
                // its query row, K row and V row. The round adds one to R; the terminal is ~12 KB (given, not derived).
                let position_rounds = rounds + 1;
                let core_p = palw_tir_dispute_door_fit_v1(position_rounds, levels, &blocks_s, floor_s, k).expect("the door fits in u64");
                assert_eq!(core_p, core + PALW_TIR_DISPUTE_ROUND_DAA_V1, "one extra round adds exactly one round of DAA");
                let door_a_p = core_p + carry_sum;
                let position_span = PALW_TIR_DISPUTE_SPAN_INTERCEPT_DAA_V1 + (POSITION_TERMINAL_BYTES_GIVEN * scale).div_ceil(PALW_TIR_DISPUTE_SPAN_BYTES_PER_DAA_V1);
                let door_b_p = door_a_p + 2 * position_span;
                println!("[{label}] --- position terminal, {tag} ---");
                println!("[{label}]   rounds = {position_rounds} (R + 1: one position round over 16 slots)");
                println!("[{label}]   core door (rounds + 1) = {core_p}");
                println!("[{label}]   door_A_position (no terminal) = {door_a_p}");
                println!("[{label}]   position terminal span({} B) = 44 + ceil(B/920) = {position_span}; own + restart = {}", POSITION_TERMINAL_BYTES_GIVEN * scale, 2 * position_span);
                println!("[{label}]   door_B_position (with position terminal) = {door_b_p}");
            }
        }
    }

    #[test]
    fn owner_queue_wait_scales_with_the_open_capacity_bound() {
        let mut cost = palw_tir_frame_test_only_cost_v1();
        let open_window = 5; // The node fixture's acquisition (4) plus one inclusion margin.
        assert_eq!(tir_public_owner_queue_wait_v1(open_window, &cost.config, &cost.clock).unwrap(), 48);

        // At 64 MiB/DAA, 8,192 controls can enter per DAA. The queue bound correctly counts
        // those opens, so increasing inclusion capacity cannot be used to erase queue latency.
        cost.clock.parallel_bytes_per_daa = 64 << 20;
        assert_eq!(tir_public_owner_queue_wait_v1(open_window, &cost.config, &cost.clock).unwrap(), 40_968);
    }
}
