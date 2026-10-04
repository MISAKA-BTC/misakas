//! **RFC-0006 — layer-sharded panels for IR classes** (`docs/rfc/0006-palw-layer-sharded-panels.md`, adopted
//! 2026-10-03; dormant under `Params::palw_tir_shard_v1`, `None` on every preset and in no flag-day list).
//!
//! A seat of an IR class verifies a **cell** of a claim — a contiguous range of layers (a *shard*) crossed with a
//! position segment — from the claim's committed boundary rows and only that shard's weights. This module is the pure
//! half: everything two nodes must derive identically from the registered program and the declared plan.
//!
//! * **the plan** ([`PalwTirShardPlanV1`]): `S_L` layer shards, `S_P` position segments per shard, declared once by the
//!   class's registrant ([`PalwTirShardPlanV1::declared`], decision 8: `S_L` is at least the fewest shards whose
//!   widest shard fits the network's seat budget and at most twice that, decision 5: `S_P` is `1` or `s_shard − 1`);
//! * **the layer partition** ([`palw_tir_shard_partition_v1`], RFC §1.1): the contiguous partition of the layers that
//!   minimises the widest shard's weight — `pre` with shard 0, `post` with the last, global params on every shard
//!   that reads them;
//! * **the cell shares** ([`PalwTirShardShareTableV1`], RFC §6.1): each cell's share of the claim's structural work
//!   (PALW-TIR-16), in permille, from the class's own program — what a seat's lock and pay scale by;
//! * **the panel** (RFC §4.1/§4.2): shard-major, each shard `[outsider?] ++ s_shard class seats`
//!   ([`palw_tir_panel_stride_v1`]); the S1 assignment inside a shard ([`palw_tir_shard_assignment_v1`]);
//! * **the receipt** (RFC §4.3): [`PalwSeatReceiptV4`], signed over [`palw_receipt_message_v4`] so a relayer cannot
//!   move a receipt to another shard or widen its mask;
//! * **the recount** (RFC §4.4): `basis_k = min(3, min over cells of the distinct counted Valid signers covering the
//!   cell)` ([`palw_tir_shard_basis_k_v1`]);
//! * **the part** ([`PalwTirShardPartV1`], RFC §4.5): one shard's licensing, and [`palw_tir_shard_part_verdict_v1`]
//!   the one function that says whether its receipts license that shard;
//! * **the class room** (RFC §6.2): [`palw_tir_shard_room_v1`].
//!
//! The consensus half (the objects, the draw, the fold) lives in `palw_state_v2` and `palw_tir_shard_fold_v1`, and calls
//! these functions — the producer of an object and its verifier share one spelling.
//!
//! **Signing contexts.** [`PALW_RECEIPT_V4_MLDSA87_CONTEXT`] and the plan and readiness contexts are not in testnet-12's
//! committed context set (V5): that set is inside the genesis ruleset id. Like the batch licence's and the market's, they
//! are covered by the Some-only fence that gates every object signed under them (`palw_tir_shard_v1` is in
//! `consensus_params_id`), and live in their own registry ([`PALW_TIR_SHARD_V1_ALL_DOMAINS`]), outside the acceptance
//! families.

use std::ops::Range;

use crate::Hash64;
use crate::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use crate::palw_state_v2::{PalwBondKeyV2, PalwPanelSeatV2};
use crate::palw_verification_v2::{PalwSegmentMaskV2, palw_segment_assignment_v2};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir::program::Ref;

// ---------------------------------------------------------------------------------------------
// Constants (adopted 2026-10-03: every open question of the RFC at its recommended default)
// ---------------------------------------------------------------------------------------------

/// **`s_shard`**: the class seats drawn per shard (decision 2: two attesters per cell and the shard's outsider; a third
/// seat is the liveness margin, as the legacy panel's fifth is).
pub const PALW_TIR_SHARD_SEATS_PER_SHARD_V1: u16 = 3;
/// **Distinct class seats that must attest every cell** of a shard before its part licenses — Verification V2's own
/// rule (`PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT`), at cell grain.
pub const PALW_TIR_SHARD_ATTESTERS_PER_CELL_V1: u16 = crate::palw_verification_v2::PALW_VERIFICATION_V2_ATTESTATIONS_PER_SEGMENT;
/// The most layer shards a plan may have (the progress bitmap's, and well under a `u16` shard index).
pub const PALW_TIR_SHARD_MAX_SHARDS_V1: u16 = 64;
/// **The position segments a plan may declare** (decision 5): layers only (`S_P = 1`, the default) or S1 inside the
/// shard (`S_P = s_shard − 1`, the target).
pub const PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1: u16 = 1;
pub const PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1: u16 = PALW_TIR_SHARD_SEATS_PER_SHARD_V1 - 1;
/// **The network's seat budget in weight bytes** (decision 8): what the fewest-shards rule fits the widest shard
/// into. Testnet-12's seat ledger share is 3.5 GiB (RFC §1); a network that sets another budget moves this by its own
/// fence. A consensus constant, not a node's: `S_L` is checked against it.
pub const PALW_TIR_SHARD_SEAT_BUDGET_BYTES_V1: u128 = 3 * (1 << 30) + (1 << 29);
/// **A registrant may declare up to this many times the derived `S_L`** (decision 8: so a class cannot thin its own
/// panels by over-sharding).
pub const PALW_TIR_SHARD_OVERSHARD_FACTOR_V1: u16 = 2;
/// **The floor of a seat's lock and pay share, in permille of the claim's work** (decision 6): a seat on a cell of
/// 4‰ of the work still locks and is paid at least this share, so the lock stays worth slashing and a small cell is
/// worth sitting. One eighth.
pub const PALW_TIR_SHARD_SEAT_FLOOR_PERMILLE_V1: u32 = 125;
/// The job the cell shares are measured on, a fixed rule of the class's own `max_context` (positions): half prefill,
/// the rest decode, so `post` runs where a real job runs it. Not a claim's own job — a claim's `(P, G)` is not in the
/// fold — but the class's, which is what a registrant's plan is priced on.
pub fn palw_tir_shard_canonical_facts_v1(max_context: u32) -> crate::palw_canonical_work_v1::PalwCanonicalExecutionFactsV1 {
    let positions = max_context.max(2);
    let prefill = (positions / 2).max(1);
    let generated = positions - prefill + 1;
    crate::palw_canonical_work_v1::PalwCanonicalExecutionFactsV1::uncached(prefill, generated)
}

pub const PALW_TIR_SHARD_PLAN_DOMAIN_MESSAGE_V1: &[u8] = b"misaka-palw/tir-shard/plan/message/v1";
/// The registrant's ML-DSA-87 context for an IR shard plan. Not in V5; covered by the fence.
pub const PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/tir-shard/plan/mldsa87/v1";
pub const PALW_RECEIPT_V4_DOMAIN_MESSAGE: &[u8] = b"misaka-palw/receipt-v4/message/v1";
/// A seat's ML-DSA-87 context over a cell-masked receipt. Not in V5; covered by the fence.
pub const PALW_RECEIPT_V4_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/receipt-v4/mldsa87/v1";
pub const PALW_TIR_SHARD_READY_CLASS_DOMAIN_V1: &[u8] = b"misaka-palw/tir-shard/ready-class/v1";
pub const PALW_TIR_SHARD_SEED_DOMAIN_V1: &[u8] = b"misaka-palw/tir-shard/shard-seed/v1";
pub const PALW_TIR_SHARD_READINESS_DOMAIN_MESSAGE_V1: &[u8] = b"misaka-palw/tir-shard/readiness/message/v1";
/// A seat's ML-DSA-87 context over a shard readiness proof. Not in V5; covered by the fence.
pub const PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/tir-shard/readiness/mldsa87/v1";
pub const PALW_TIR_SHARD_READINESS_LEAVES_DOMAIN_V1: &[u8] = b"misaka-palw/tir-shard/readiness/leaves/v1";
pub const PALW_TIR_SHARD_V1_ALL_DOMAINS: &[&[u8]] = &[
    PALW_TIR_SHARD_PLAN_DOMAIN_MESSAGE_V1,
    PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT,
    PALW_RECEIPT_V4_DOMAIN_MESSAGE,
    PALW_RECEIPT_V4_MLDSA87_CONTEXT,
    PALW_TIR_SHARD_READY_CLASS_DOMAIN_V1,
    PALW_TIR_SHARD_SEED_DOMAIN_V1,
    PALW_TIR_SHARD_READINESS_DOMAIN_MESSAGE_V1,
    PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT,
    PALW_TIR_SHARD_READINESS_LEAVES_DOMAIN_V1,
];

// ---------------------------------------------------------------------------------------------
// The row request (D-S6 over the network): an interval-lane request index naming inventory rows
// ---------------------------------------------------------------------------------------------

/// **The request-index kind a shard-only seat asks inventory rows under** (RFC-0006 §4.2, D-S6). The interval lane's signed request
/// carries a `u32` index whose bits 29, 30 and 31 are the leaf-evidence/segment, resume and block-leaves kinds; a plain interval
/// index is far under 2^28. Bit 28 alone, with bits 29..31 clear, is none of those, and the low 28 bits name the FIRST inventory
/// leaf asked: the server answers the run of rows from there that fits the lane. The index is inside the signed request, so one
/// signature is one ask, not a standing right.
pub const PALW_TIR_ROWS_REQUEST_BIT_V1: u32 = 1 << 28;

/// The most rows one reply carries (the serving node may answer fewer, whatever fits the lane's byte cap).
pub const PALW_TIR_ROWS_PER_REPLY_MAX_V1: u32 = 256;

/// The request index asking for inventory rows from `first_leaf`; `None` past the 28 bits.
pub fn palw_tir_rows_request_index_v1(first_leaf: u32) -> Option<u32> {
    (first_leaf < PALW_TIR_ROWS_REQUEST_BIT_V1).then_some(PALW_TIR_ROWS_REQUEST_BIT_V1 | first_leaf)
}

/// **The request-index kind a seat asks a RUN of step leaves under** (RFC-0006 §3, off chain): bit 27 alone, bits 28..31 clear —
/// none of the other kinds, and the row kind needs bit 28. The low 27 bits name the first step leaf and the request's leaf slot
/// carries the run's length (at most [`crate::palw_tir_court_v1::PALW_TIR_STEP_RUN_MAX_LEAVES_V1`]); both are inside the signed
/// request. The answer is the same `PalwTirStepRunDisclosureV1` the chain's `TirStepRun` unit is answered with, so one verifier
/// (`check_tir_step_run_disclosure_v1`) checks it on and off chain; the off-chain ask carries no session, so a job of any size can be
/// pursued, and the four sessions of a seat are kept for the demands that enforce.
pub const PALW_TIR_RUNS_REQUEST_BIT_V1: u32 = 1 << 27;

pub fn palw_tir_runs_request_index_v1(first_leaf: u32) -> Option<u32> {
    (first_leaf < PALW_TIR_RUNS_REQUEST_BIT_V1).then_some(PALW_TIR_RUNS_REQUEST_BIT_V1 | first_leaf)
}

/// `Some(first_leaf)` for a run request index, `None` for any other kind.
pub fn palw_tir_runs_request_decode_v1(index: u32) -> Option<u32> {
    (index & (15 << 28) == 0 && index & PALW_TIR_RUNS_REQUEST_BIT_V1 != 0).then_some(index & !PALW_TIR_RUNS_REQUEST_BIT_V1)
}

/// `Some(first_leaf)` for a row request index, `None` for any other kind.
pub fn palw_tir_rows_request_decode_v1(index: u32) -> Option<u32> {
    (index & (7 << 29) == 0 && index & PALW_TIR_ROWS_REQUEST_BIT_V1 != 0).then_some(index & !PALW_TIR_ROWS_REQUEST_BIT_V1)
}

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirShardError {
    #[error("a plan of {0} layer shards: the range is 2 ..= {PALW_TIR_SHARD_MAX_SHARDS_V1}")]
    ShardCountOutOfRange(u16),
    #[error("{0} position segments: a plan has 1 (layers only) or {PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1} (S1 inside the shard)")]
    PositionSegmentsNotOffered(u16),
    #[error("the class has {layers} layers: a plan of {shards} shards would leave one empty")]
    MoreShardsThanLayers { shards: u16, layers: usize },
    #[error("a plan of {declared} shards is under the {min} the network's seat budget needs for this class")]
    UnderSharded { declared: u16, min: u16 },
    #[error("a plan of {declared} shards is over twice the {min} the network's seat budget needs for this class")]
    OverSharded { declared: u16, min: u16 },
    #[error("the program has no shard plan: {0}")]
    Program(String),
}

// ---------------------------------------------------------------------------------------------
// The plan
// ---------------------------------------------------------------------------------------------

/// **A class's layer-shard plan, declared once by its registrant** (`TirShardPlanDeclared`) and immutable like the
/// class's graph. `shard_cells_permille` is derived from the program at the declaration and stored: the structural
/// work of each cell, shard-major then segment, as permille of the whole (summing to exactly 1,000), what a seat's
/// lock and pay scale by.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirShardPlanV1 {
    pub s_l: u16,
    pub s_p: u16,
    pub declared_daa: u64,
    /// `s_l × s_p` entries, shard-major. Sum 1,000.
    pub cell_permille: Vec<u16>,
}

/// **The shape of a declared plan**, over the class's layer count and its derived minimum `S_L`:
/// `S_L ∈ [max(min, 2), max(2·min, 2)]` and at most one shard per layer; `S_P` one of the two offered values.
pub fn palw_tir_shard_plan_shape_v1(s_l: u16, s_p: u16, layers: usize, min_shards: u16) -> Result<(), PalwTirShardError> {
    if !(2..=PALW_TIR_SHARD_MAX_SHARDS_V1).contains(&s_l) {
        return Err(PalwTirShardError::ShardCountOutOfRange(s_l));
    }
    if s_p != PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1 && s_p != PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1 {
        return Err(PalwTirShardError::PositionSegmentsNotOffered(s_p));
    }
    if usize::from(s_l) > layers {
        return Err(PalwTirShardError::MoreShardsThanLayers { shards: s_l, layers });
    }
    let min = min_shards.max(2);
    if s_l < min {
        return Err(PalwTirShardError::UnderSharded { declared: s_l, min });
    }
    let max = min_shards.saturating_mul(PALW_TIR_SHARD_OVERSHARD_FACTOR_V1).max(2);
    if s_l > max {
        return Err(PalwTirShardError::OverSharded { declared: s_l, min: min_shards.max(1) });
    }
    Ok(())
}

/// The message a registrant signs to declare a plan: the network domain, the class, `S_L` and `S_P`.
pub fn palw_tir_shard_plan_message_v1(network_domain: Hash64, class_id: &Hash64, s_l: u16, s_p: u16) -> Hash64 {
    let mut s = keyed(PALW_TIR_SHARD_PLAN_DOMAIN_MESSAGE_V1);
    s.update(network_domain.as_byte_slice());
    s.update(class_id.as_byte_slice());
    s.update(&s_l.to_le_bytes());
    s.update(&s_p.to_le_bytes());
    finish(s)
}

// ---------------------------------------------------------------------------------------------
// The layer partition (RFC §1.1)
// ---------------------------------------------------------------------------------------------

/// What a shard of a program weighs: per-layer bytes plus the constant terms every shard that holds a layer, `pre`
/// or `post` carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirShardWeightsV1 {
    /// Bytes of layer `l`'s per-layer param instances and per-layer state at `max_context`.
    pub layer_bytes: Vec<u128>,
    /// Bytes of every global param and global state the layer blocks read — on every shard that holds a layer.
    pub layers_global_bytes: u128,
    /// Bytes of what `pre` reads beyond the layers' globals (the embedding).
    pub pre_extra_bytes: u128,
    /// Bytes of what `post` reads beyond the layers' globals (the head).
    pub post_extra_bytes: u128,
}

impl PalwTirShardWeightsV1 {
    /// The weight of the shard holding layers `[first, end)`, with `pre` iff `first == 0` and `post` iff
    /// `end == L`.
    pub fn of_range(&self, first: usize, end: usize) -> u128 {
        let l = self.layer_bytes.len();
        let mut w: u128 = self.layer_bytes[first..end].iter().fold(0u128, |a, b| a.saturating_add(*b));
        w = w.saturating_add(self.layers_global_bytes);
        if first == 0 {
            w = w.saturating_add(self.pre_extra_bytes);
        }
        if end == l {
            w = w.saturating_add(self.post_extra_bytes);
        }
        w
    }
}

fn dtype_lane_bytes() -> u128 {
    // PALW-TIR-5: a committed or held lane is 4 bytes whatever its dtype.
    4
}

/// **The weights of a program's layers** (RFC §1.1): the bytes of every param instance a layer's block reads, plus the
/// `Hist` and `Fixed` state it holds at `max_context` (the history in lanes, 4 bytes a value — what a seat holds).
pub fn palw_tir_shard_weights_v1(p: &TirProgramV1, max_context: u32) -> PalwTirShardWeightsV1 {
    use misaka_palw_tir::Prim;
    use misaka_palw_tir::program::StateKind;
    let layers = p.schedule.layers.len();
    // Per block: the params and states it touches.
    let touched = |bi: usize| -> (Vec<u16>, Vec<u16>) {
        let (mut params, mut states) = (Vec::new(), Vec::new());
        for n in &p.blocks[bi].nodes {
            for r in &n.inputs {
                match r {
                    Ref::Param(j) if !params.contains(j) => params.push(*j),
                    Ref::State(j) if !states.contains(j) => states.push(*j),
                    _ => {}
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim
                && !states.contains(&state)
            {
                states.push(state);
            }
        }
        (params, states)
    };
    let param_bytes = |j: u16| crate::palw_tir_artifact_v1::palw_tir_tensor_bytes_v1(p, j) as u128;
    let state_bytes = |j: u16| -> u128 {
        let d = &p.states[j as usize];
        let elements: u128 = d.shape.iter().map(|x| *x as u128).product::<u128>().max(1);
        let rows = match d.kind {
            StateKind::Hist { window } => (window as u128).min(max_context as u128).max(1),
            StateKind::Fixed { .. } => 1,
        };
        elements.saturating_mul(rows).saturating_mul(dtype_lane_bytes())
    };
    let mut layer_bytes = vec![0u128; layers];
    let mut layers_global: Vec<(bool, u16)> = Vec::new(); // (is_state, index) of every global read by a layer block
    for (l, bi) in p.schedule.layers.iter().enumerate() {
        let (params, states) = touched(*bi as usize);
        for j in params {
            if p.params[j as usize].per_layer {
                layer_bytes[l] = layer_bytes[l].saturating_add(param_bytes(j));
            } else if !layers_global.contains(&(false, j)) {
                layers_global.push((false, j));
            }
        }
        for j in states {
            if p.states[j as usize].per_layer {
                layer_bytes[l] = layer_bytes[l].saturating_add(state_bytes(j));
            } else if !layers_global.contains(&(true, j)) {
                layers_global.push((true, j));
            }
        }
    }
    let bytes_of = |(is_state, j): (bool, u16)| if is_state { state_bytes(j) } else { param_bytes(j) };
    let layers_global_bytes = layers_global.iter().map(|g| bytes_of(*g)).fold(0u128, u128::saturating_add);
    // What `pre` and `post` read beyond the layers' globals: per-layer params never appear in them (NF-11), so only globals.
    let extra = |bi: usize| -> u128 {
        let (params, states) = touched(bi);
        let mut seen: Vec<(bool, u16)> = layers_global.clone();
        let mut total = 0u128;
        for j in params {
            if !seen.contains(&(false, j)) {
                seen.push((false, j));
                total = total.saturating_add(param_bytes(j));
            }
        }
        for j in states {
            if !seen.contains(&(true, j)) {
                seen.push((true, j));
                total = total.saturating_add(state_bytes(j));
            }
        }
        total
    };
    PalwTirShardWeightsV1 {
        layer_bytes,
        layers_global_bytes,
        pre_extra_bytes: extra(p.schedule.pre as usize),
        post_extra_bytes: extra(p.schedule.post as usize),
    }
}

/// **The layer partition of `s` shards**: the contiguous partition of `[0, L)` into exactly `s` non-empty ranges that
/// minimises the widest shard's weight, ties broken by the greedy-left construction below (so two nodes derive the
/// same ranges). `None` if `s == 0` or `s > L`.
pub fn palw_tir_shard_partition_v1(w: &PalwTirShardWeightsV1, s: u16) -> Option<Vec<Range<usize>>> {
    let l = w.layer_bytes.len();
    let s = usize::from(s);
    if s == 0 || s > l {
        return None;
    }
    // The fewest shards a ceiling `cap` needs (greedy-left is optimal for a monotone range cost).
    let needed = |cap: u128| -> Option<usize> {
        let (mut shards, mut first) = (0usize, 0usize);
        while first < l {
            let mut end = first + 1;
            if w.of_range(first, end) > cap {
                return None;
            }
            while end < l && w.of_range(first, end + 1) <= cap {
                end += 1;
            }
            shards += 1;
            first = end;
        }
        Some(shards)
    };
    let (mut lo, mut hi) = (0u128, w.of_range(0, l));
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        match needed(mid) {
            Some(n) if n <= s => hi = mid,
            _ => lo = mid + 1,
        }
    }
    let cap = lo;
    // Build exactly `s` shards under `cap`: each takes as many layers as fit while leaving one layer for every shard
    // still to come.
    let mut out = Vec::with_capacity(s);
    let mut first = 0usize;
    for k in 0..s {
        let remaining_after = s - k - 1;
        let max_end = l - remaining_after;
        let mut end = first + 1;
        while end < max_end && w.of_range(first, end + 1) <= cap {
            end += 1;
        }
        if k == s - 1 {
            end = l;
        }
        out.push(first..end);
        first = end;
    }
    Some(out)
}

/// **The fewest shards whose widest shard fits `budget`** (decision 8) — at least 1, at most `L`. A class whose layers
/// are each over budget still returns `L` (one layer a shard is the floor of the geometry).
pub fn palw_tir_shard_min_shards_v1(w: &PalwTirShardWeightsV1, budget: u128) -> u16 {
    let l = w.layer_bytes.len().min(usize::from(u16::MAX));
    for s in 1..=l {
        if let Some(parts) = palw_tir_shard_partition_v1(w, s as u16)
            && parts.iter().all(|r| w.of_range(r.start, r.end) <= budget)
        {
            return s as u16;
        }
    }
    l.max(1) as u16
}

/// **Lane PA, S-1 / B-F5 (`palw_audit_1004_v1`): the same answer as [`palw_tir_shard_min_shards_v1`] in `O(L)`.** The fewest shards whose
/// widest fits `budget` is the greedy-left shard count under `budget` (the optimal partition's widest is the least cap the greedy
/// count meets), and `L` where one layer alone is over budget. The old search ran the whole partition for every `s = 1 … L` —
/// `O(L²·128)` per `TirShardPlanDeclared`, before the shape was looked at, with no nonce in the signature to make a repeat cost
/// anything. Prefix sums make a range's weight `O(1)`.
pub fn palw_tir_shard_min_shards_fast_v1(w: &PalwTirShardWeightsV1, budget: u128) -> u16 {
    let l = w.layer_bytes.len();
    let cap_l = l.min(usize::from(u16::MAX));
    if l == 0 {
        return 1;
    }
    let mut prefix = Vec::with_capacity(l + 1);
    prefix.push(0u128);
    for bytes in &w.layer_bytes {
        prefix.push(prefix[prefix.len() - 1].saturating_add(*bytes));
    }
    let range = |first: usize, end: usize| -> u128 {
        let mut weight = (prefix[end] - prefix[first]).saturating_add(w.layers_global_bytes);
        if first == 0 {
            weight = weight.saturating_add(w.pre_extra_bytes);
        }
        if end == l {
            weight = weight.saturating_add(w.post_extra_bytes);
        }
        weight
    };
    let (mut shards, mut first) = (0usize, 0usize);
    while first < l {
        if range(first, first + 1) > budget {
            return cap_l.max(1) as u16;
        }
        let mut end = first + 1;
        while end < l && range(first, end + 1) <= budget {
            end += 1;
        }
        shards += 1;
        first = end;
    }
    shards.min(cap_l).max(1) as u16
}

/// **Lane PA, S-1: the part of [`palw_tir_shard_plan_shape_v1`] that needs no derived minimum** — the shard count's range, the
/// position segments, and no more shards than layers. Asked before the weights are derived.
pub fn palw_tir_shard_plan_prelim_v1(s_l: u16, s_p: u16, layers: usize) -> Result<(), PalwTirShardError> {
    if !(2..=PALW_TIR_SHARD_MAX_SHARDS_V1).contains(&s_l) {
        return Err(PalwTirShardError::ShardCountOutOfRange(s_l));
    }
    if s_p != PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1 && s_p != PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1 {
        return Err(PalwTirShardError::PositionSegmentsNotOffered(s_p));
    }
    if usize::from(s_l) > layers {
        return Err(PalwTirShardError::MoreShardsThanLayers { shards: s_l, layers });
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Cell shares (RFC §6.1)
// ---------------------------------------------------------------------------------------------

/// **The structural work of every cell of a plan, in permille of the whole** — `w_cell(i, j) / w` of RFC §6.1, from
/// the class's own program (`palw_tir_work_shape_with_v1`) over the canonical job of
/// [`palw_tir_shard_canonical_facts_v1`], the occurrences of shard `i` (`pre` on shard 0, `post` on the last) over
/// segment `j`'s positions (equal thirds... halves of the job's positions). Largest-remainder rounding so the table sums
/// to exactly 1,000.
pub fn palw_tir_shard_cell_permille_v1(
    p: &TirProgramV1,
    max_context: u32,
    s_l: u16,
    s_p: u16,
    min_select_arms: bool,
) -> Result<(Vec<Range<usize>>, Vec<u16>), PalwTirShardError> {
    let weights = palw_tir_shard_weights_v1(p, max_context);
    let parts = palw_tir_shard_partition_v1(&weights, s_l)
        .ok_or(PalwTirShardError::MoreShardsThanLayers { shards: s_l, layers: weights.layer_bytes.len() })?;
    let shape = crate::palw_tir_work_v1::palw_tir_work_shape_with_v1(p, min_select_arms)
        .map_err(|e| PalwTirShardError::Program(e.to_string()))?;
    let facts = palw_tir_shard_canonical_facts_v1(max_context);
    let positions = u128::from(facts.prefill_tokens) + u128::from(facts.generated_tokens) - 1;
    let layers = weights.layer_bytes.len();
    let mut raw: Vec<u128> = Vec::with_capacity(usize::from(s_l) * usize::from(s_p));
    for (i, range) in parts.iter().enumerate() {
        // Occurrence indices: 0 = pre, 1 + l = layer l, L + 1 = post.
        let first = if i == 0 { 0 } else { 1 + range.start };
        let end = if range.end == layers { layers + 2 } else { 1 + range.end };
        for j in 0..u128::from(s_p) {
            let (from, to) = (positions * j / u128::from(s_p), positions * (j + 1) / u128::from(s_p));
            let v = shape.work_cell_v1(first..end, from..to, &facts).map_err(|e| PalwTirShardError::Program(e.to_string()))?;
            raw.push(v.provisional_scalar_v1());
        }
    }
    Ok((parts, palw_largest_remainder_permille_v1(&raw)))
}

/// **Occurrence indices of shard `i`** (0 = `pre`, `1 + l` = layer `l`, `L + 1` = `post`): the layer range `layers` of the shard's
/// partition, `pre` with the first shard and `post` with the last (`layers_total` = `L`).
pub fn palw_tir_shard_occurrences_v1(layers: &Range<usize>, layers_total: usize) -> Range<usize> {
    let first = if layers.start == 0 { 0 } else { 1 + layers.start };
    let end = if layers.end == layers_total { layers_total + 2 } else { 1 + layers.end };
    first..end
}

/// **The alignment of a segment cut** (RFC §1.2): `G = lcm(C, h_tile)`. `None` for a zero interval or tile, or an overflow.
pub fn palw_tir_shard_segment_align_v1(checkpoint_interval: u32, h_tile: u32) -> Option<u32> {
    if checkpoint_interval == 0 || h_tile == 0 {
        return None;
    }
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    (checkpoint_interval / gcd(checkpoint_interval, h_tile)).checked_mul(h_tile)
}

/// **Segment `j` of `s_p`, as positions** (RFC §1.2): `[⌊j·T/S_P⌋_G, ⌊(j+1)·T/S_P⌋_G)`, the last ending at `T`. `None` for
/// `j ≥ s_p`, `s_p == 0` or a zero alignment. A segment may be empty when the job is shorter than the alignment; the draw's
/// owner of an empty segment has nothing to verify.
pub fn palw_tir_shard_segment_positions_v1(positions: u32, align: u32, s_p: u16, j: u16) -> Option<Range<u32>> {
    if s_p == 0 || j >= s_p || align == 0 {
        return None;
    }
    let cut = |k: u16| -> u32 {
        if k >= s_p {
            return positions;
        }
        let x = (u64::from(positions) * u64::from(k) / u64::from(s_p)) as u32;
        x / align * align
    };
    Some(cut(j)..cut(j + 1))
}

/// `raw` as permille summing to exactly 1,000 (largest remainder, ties to the lowest index); all-zero raw is uniform.
pub fn palw_largest_remainder_permille_v1(raw: &[u128]) -> Vec<u16> {
    let n = raw.len();
    if n == 0 {
        return Vec::new();
    }
    let total: u128 = raw.iter().fold(0u128, |a, b| a.saturating_add(*b));
    if total == 0 {
        let mut out = vec![(1_000 / n) as u16; n];
        let mut rest = 1_000 - out.iter().map(|x| u32::from(*x)).sum::<u32>();
        let mut i = 0;
        while rest > 0 {
            out[i % n] += 1;
            rest -= 1;
            i += 1;
        }
        return out;
    }
    let mut floors: Vec<(usize, u128, u128)> =
        raw.iter().enumerate().map(|(i, r)| (i, r.saturating_mul(1_000) / total, r.saturating_mul(1_000) % total)).collect();
    let mut given: u128 = floors.iter().map(|(_, f, _)| *f).sum();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| floors[*b].2.cmp(&floors[*a].2).then(a.cmp(b)));
    let mut k = 0;
    while given < 1_000 {
        floors[order[k % n]].1 += 1;
        given += 1;
        k += 1;
    }
    floors.into_iter().map(|(_, f, _)| f as u16).collect()
}

/// The share (permille of the claim's work) of the cells of shard `shard` named by `mask`.
pub fn palw_tir_cells_share_permille_v1(cell_permille: &[u16], s_p: u16, shard: u16, mask: PalwSegmentMaskV2) -> u32 {
    (0..s_p)
        .filter(|j| mask.covers(*j))
        .map(|j| u32::from(cell_permille.get(usize::from(shard) * usize::from(s_p) + usize::from(j)).copied().unwrap_or(0)))
        .sum()
}

/// **A seat's lock** (decision 6): `lock × max(share, floor) / 1,000`, never above `lock`, never 0 for a nonzero lock.
pub fn palw_tir_shard_lock_v1(full_lock: u128, share_permille: u32) -> u128 {
    if full_lock == 0 {
        return 0;
    }
    let share = share_permille.clamp(PALW_TIR_SHARD_SEAT_FLOOR_PERMILLE_V1, 1_000) as u128;
    (full_lock.saturating_mul(share) / 1_000).max(1)
}

/// **The panel's split of a `Final` claim's reward** (decision 6): the pool `⌊reward × pool_permille / 1000⌋` is divided
/// over the DRAWN seats by their share of the work (each at least the floor), a credited seat is paid its part, and
/// what no seat was credited for, with the division's dust, is the reserve — never the producer's. `drawn[i]` is the
/// share of drawn seat `i`, `credited[i]` whether it was credited. `producer + Σ paid + reserve == reward`, always.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirShardSplitV1 {
    pub producer: u64,
    pub paid: Vec<u64>,
    pub reserve: u64,
}

pub fn palw_tir_shard_split_v1(reward: u64, pool_permille: u16, drawn: &[u32], credited: &[bool]) -> PalwTirShardSplitV1 {
    let pool = ((reward as u128) * (pool_permille.min(1_000) as u128) / 1_000) as u64;
    let producer = reward - pool;
    let weights: Vec<u128> = drawn.iter().map(|s| u128::from((*s).clamp(PALW_TIR_SHARD_SEAT_FLOOR_PERMILLE_V1, 1_000))).collect();
    let total: u128 = weights.iter().sum();
    let paid: Vec<u64> =
        weights.iter().zip(credited).map(|(w, c)| if *c && total > 0 { ((pool as u128) * w / total) as u64 } else { 0 }).collect();
    let spent: u64 = paid.iter().sum();
    PalwTirShardSplitV1 { producer, paid, reserve: pool - spent }
}

// ---------------------------------------------------------------------------------------------
// The panel (RFC §4.1, §4.2)
// ---------------------------------------------------------------------------------------------

/// **Seats per shard in a stored panel**: the class seats, and the shard's outsider first where the claim is
/// outsider-judged.
pub fn palw_tir_panel_stride_v1(outsider: bool) -> u16 {
    PALW_TIR_SHARD_SEATS_PER_SHARD_V1 + u16::from(outsider)
}

/// **The seats of `shard` in a stored panel** (`[outsider?] ++ class seats`), or `None` when the panel is not
/// `s_l × stride` seats — it was not drawn per shard — or the shard is out of range.
pub fn palw_tir_panel_shard_slice_v1(seats: &[PalwPanelSeatV2], s_l: u16, outsider: bool, shard: u16) -> Option<&[PalwPanelSeatV2]> {
    let stride = usize::from(palw_tir_panel_stride_v1(outsider));
    if shard >= s_l || seats.len() != usize::from(s_l).checked_mul(stride)? {
        return None;
    }
    let start = usize::from(shard) * stride;
    seats.get(start..start + stride)
}

/// **The readiness class of one shard**: the derived id a seat's possession proof of the shard's rows is recorded
/// under (`seat_readiness[(bond, ready_class)]`), so the draw seats exactly the bonds that proved they hold the shard.
pub fn palw_tir_shard_ready_class_v1(class_id: &Hash64, s_l: u16, shard: u16) -> Hash64 {
    let mut s = keyed(PALW_TIR_SHARD_READY_CLASS_DOMAIN_V1);
    s.update(class_id.as_byte_slice());
    s.update(&s_l.to_le_bytes());
    s.update(&shard.to_le_bytes());
    finish(s)
}

/// **A shard's own seed**: `H(domain ‖ panel seed ‖ claim ‖ shard)`. Every shard's draw, its outsider's ticket and its
/// S1 assignment read it, so the shards are independent permutations of one anchor and a claim.
pub fn palw_tir_shard_seed_v1(seed: &Hash64, claim_id: &Hash64, shard: u16) -> Hash64 {
    let mut s = keyed(PALW_TIR_SHARD_SEED_DOMAIN_V1);
    s.update(seed.as_byte_slice());
    s.update(claim_id.as_byte_slice());
    s.update(&shard.to_le_bytes());
    finish(s)
}

/// **The segments each class seat of a shard is assigned** (decision 5): with `S_P = 1` every class seat attests the
/// whole shard; with `S_P = s_shard − 1` it is Verification V2's assignment inside the shard — one full-shard seat and a
/// partial seat per segment, by the shard's own seed. `PalwSegmentMaskV2::NONE`-free: every seat holds a duty.
pub fn palw_tir_shard_assignment_v1(seed: &Hash64, claim_id: &Hash64, shard: u16, s_p: u16) -> Vec<PalwSegmentMaskV2> {
    let seats = PALW_TIR_SHARD_SEATS_PER_SHARD_V1;
    if s_p <= 1 {
        return vec![PalwSegmentMaskV2::full(1); usize::from(seats)];
    }
    let assignment = palw_segment_assignment_v2(palw_tir_shard_seed_v1(seed, claim_id, shard), *claim_id, seats);
    debug_assert_eq!(assignment.segments, s_p);
    assignment.masks
}

/// The outsider's mask: the whole shard (decision 3).
pub fn palw_tir_shard_outsider_mask_v1(s_p: u16) -> PalwSegmentMaskV2 {
    PalwSegmentMaskV2::full(s_p.max(1))
}

/// **The assigned mask of seat `index` of a shard's slice** (`[outsider?] ++ class seats`).
pub fn palw_tir_shard_seat_mask_v1(
    seed: &Hash64,
    claim_id: &Hash64,
    shard: u16,
    s_p: u16,
    outsider: bool,
    index: usize,
) -> Option<PalwSegmentMaskV2> {
    if outsider && index == 0 {
        return Some(palw_tir_shard_outsider_mask_v1(s_p));
    }
    palw_tir_shard_assignment_v1(seed, claim_id, shard, s_p).get(index - usize::from(outsider)).copied()
}

// ---------------------------------------------------------------------------------------------
// The receipt (RFC §4.3)
// ---------------------------------------------------------------------------------------------

/// **A cell-masked receipt**: the V2 receipt (verdict, seat, time, signature), the shard it is for and the segments of
/// that shard it attests. The signature covers both ([`palw_receipt_message_v4`]).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwSeatReceiptV4 {
    pub receipt: PalwSeatReceiptV2,
    pub shard: u16,
    pub segments: PalwSegmentMaskV2,
}

/// What a seat signs on a cell-masked receipt: the V2 message and the shard and mask, under their own domain.
pub fn palw_receipt_message_v4(
    network_domain: Hash64,
    claim: Hash64,
    verdict: PalwReceiptVerdictV2,
    signed_daa: u64,
    shard: u16,
    segments: PalwSegmentMaskV2,
) -> Hash64 {
    let mut state = keyed(PALW_RECEIPT_V4_DOMAIN_MESSAGE);
    state.update(crate::palw_panel_v2::palw_receipt_message_v2(network_domain, claim, verdict, signed_daa).as_byte_slice());
    state.update(&shard.to_le_bytes());
    state.update(&segments.0.to_le_bytes());
    finish(state)
}

/// **One shard's licensing part** (RFC §4.5): the claim, the shard and the shard's receipts (the class seats' and the
/// outsider's). `(s_shard + 1) × 4,772` bytes of receipts at most, one carrier at any shard count.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirShardPartV1 {
    pub claim: Hash64,
    pub shard: u16,
    pub receipts: Vec<PalwSeatReceiptV4>,
}

/// The most receipts a part may carry: every seat of the shard once.
pub const PALW_TIR_SHARD_PART_MAX_RECEIPTS_V1: usize = PALW_TIR_SHARD_SEATS_PER_SHARD_V1 as usize + 1;

// ---------------------------------------------------------------------------------------------
// The recount (RFC §4.4) and the part's verdict (RFC §4.5)
// ---------------------------------------------------------------------------------------------

/// **Distinct counted `Valid` signers covering each segment of a shard**, capped at
/// [`crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1`] (three).
pub fn palw_tir_shard_cell_counts_v1(s_p: u16, signer_masks: &[PalwSegmentMaskV2]) -> Vec<u8> {
    let cap = crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 as usize;
    (0..s_p.max(1)).map(|j| signer_masks.iter().filter(|m| m.covers(j)).count().min(cap) as u8).collect()
}

/// **Q-3's recount over cells**: `min(3, min over cells of the distinct counted signers covering it)`, over the
/// per-shard cell counts of every landed part (`shard-major, segment-minor`). An empty table is 0.
pub fn palw_tir_shard_basis_k_v1(cell_counts: &[u8]) -> u8 {
    cell_counts.iter().copied().min().unwrap_or(0)
}

/// What one shard's receipts amount to, over receipts whose signatures and windows the caller verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirShardPartVerdictV1 {
    /// Every cell of the shard is attested by [`PALW_TIR_SHARD_ATTESTERS_PER_CELL_V1`] distinct class seats, and the
    /// shard's outsider (where there is one) said `Valid`: the shard licenses.
    Licensed {
        /// The counted `Valid` signers (class seats and the outsider), each with the mask it attests.
        signers: Vec<(PalwBondKeyV2, PalwSegmentMaskV2)>,
        /// Per segment: distinct counted signers covering it, capped at three.
        cell_counts: Vec<u8>,
    },
    /// Not (yet) a licence.
    Short(&'static str),
}

/// **Does this shard's receipt set license the shard?** `slice` is the shard's seats (`[outsider?] ++ class seats`),
/// `receipts` `(seat, verdict, mask)` of receipts already checked (signature, window, shard, once per seat). Only a
/// `Valid` whose mask is exactly the seat's assigned mask counts (Q-6: the mask is the assignment), so the lock records
/// what the seat vouched for. The ONE function the acceptance layer, the fold and the assembler call.
pub fn palw_tir_shard_part_verdict_v1(
    slice: &[PalwPanelSeatV2],
    seed: &Hash64,
    claim_id: &Hash64,
    shard: u16,
    s_p: u16,
    outsider: bool,
    receipts: &[(PalwBondKeyV2, PalwReceiptVerdictV2, PalwSegmentMaskV2)],
) -> PalwTirShardPartVerdictV1 {
    let mut signers: Vec<(PalwBondKeyV2, PalwSegmentMaskV2)> = Vec::new();
    let mut outsider_valid = false;
    for (bond, verdict, mask) in receipts {
        let Some(index) = slice.iter().position(|seat| seat.bond == *bond) else { continue };
        if !matches!(verdict, PalwReceiptVerdictV2::Valid) || signers.iter().any(|(b, _)| b == bond) {
            continue;
        }
        let Some(assigned) = palw_tir_shard_seat_mask_v1(seed, claim_id, shard, s_p, outsider, index) else { continue };
        if *mask != assigned {
            continue;
        }
        if outsider && index == 0 {
            outsider_valid = true;
        }
        signers.push((*bond, *mask));
    }
    if outsider && !outsider_valid {
        return PalwTirShardPartVerdictV1::Short("the shard's outsider has not answered Valid");
    }
    let class_masks: Vec<PalwSegmentMaskV2> =
        signers.iter().filter(|(b, _)| !(outsider && slice.first().is_some_and(|o| o.bond == *b))).map(|(_, m)| *m).collect();
    let attested = palw_tir_shard_cell_counts_v1(s_p, &class_masks);
    if attested.iter().any(|c| u16::from(*c) < PALW_TIR_SHARD_ATTESTERS_PER_CELL_V1) {
        return PalwTirShardPartVerdictV1::Short("a cell of the shard has fewer than two class seats attesting it");
    }
    let all_masks: Vec<PalwSegmentMaskV2> = signers.iter().map(|(_, m)| *m).collect();
    PalwTirShardPartVerdictV1::Licensed { cell_counts: palw_tir_shard_cell_counts_v1(s_p, &all_masks), signers }
}

// ---------------------------------------------------------------------------------------------
// The claim's record (RFC §4.4, §4.5, §6)
// ---------------------------------------------------------------------------------------------

/// **What the chain keeps of a claim drawn per shard**, written when its panel binds and read by every part, the lock,
/// the pay and the recount. It is rooted state (`tir_shard_claims`), empty on every network that never armed the fence.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirShardClaimV1 {
    /// The plan the panel was drawn under (frozen at the bind: a later declaration cannot move it).
    pub s_l: u16,
    pub s_p: u16,
    /// Whether the claim was outsider-judged at the bind: each shard's slice then leads with the outsider's seat.
    pub outsider: bool,
    /// Which shards have licensed (a part landed).
    pub progress: crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1,
    /// Distinct counted signers covering each cell (capped at three), `s_l × s_p`, shard-major; 0 until the shard's
    /// part lands. [`palw_tir_shard_basis_k_v1`] of it, once every shard landed, is the claim's `basis_k`.
    pub cell_counts: Vec<u8>,
    /// The counted `Valid` signers of the parts landed so far: `(bond, shard, attested mask)` — what a lock records and
    /// what Q-6 reads a fault against.
    pub counted: Vec<(PalwBondKeyV2, u16, PalwSegmentMaskV2)>,
    /// Each drawn seat's share of the claim's work, permille of its assigned cells, in stored panel order — the weights
    /// the lock and the pay scale by, fixed at the bind.
    pub drawn_permille: Vec<u32>,
    /// A part carried an abstention (`Unavailable`, `Incapable`): the licence's latch (C1), as `unserved_seen` is.
    pub unserved_seen: bool,
}

impl PalwTirShardClaimV1 {
    /// A fresh record for a claim whose panel just bound: nothing licensed.
    pub fn bound(s_l: u16, s_p: u16, outsider: bool, drawn_permille: Vec<u32>) -> Option<Self> {
        Some(Self {
            s_l,
            s_p,
            outsider,
            progress: crate::palw_shard_licensing_v1::PalwShardLicensingProgressV1::new(u32::from(s_l)).ok()?,
            cell_counts: vec![0; usize::from(s_l) * usize::from(s_p)],
            counted: Vec::new(),
            drawn_permille,
            unserved_seen: false,
        })
    }

    /// The recount of the cells landed so far.
    pub fn basis_k(&self) -> u8 {
        palw_tir_shard_basis_k_v1(&self.cell_counts)
    }
}

/// **Each stored seat's share of the claim's work**, from the plan's cell table and the S1 assignment of the claim's own
/// seed: the class seats by their assigned masks, the outsider by the whole shard.
pub fn palw_tir_shard_drawn_permille_v1(plan: &PalwTirShardPlanV1, seed: &Hash64, claim_id: &Hash64, outsider: bool) -> Vec<u32> {
    let mut out = Vec::with_capacity(usize::from(plan.s_l) * usize::from(palw_tir_panel_stride_v1(outsider)));
    for shard in 0..plan.s_l {
        let masks = palw_tir_shard_assignment_v1(seed, claim_id, shard, plan.s_p);
        if outsider {
            out.push(palw_tir_cells_share_permille_v1(
                &plan.cell_permille,
                plan.s_p,
                shard,
                palw_tir_shard_outsider_mask_v1(plan.s_p),
            ));
        }
        for mask in masks {
            out.push(palw_tir_cells_share_permille_v1(&plan.cell_permille, plan.s_p, shard, mask));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The class room (RFC §6.2)
// ---------------------------------------------------------------------------------------------

/// **The class room of an IR-sharded class**: the binding shard's.
///
/// ```text
/// room(c) = min over shards i of ⌊ ready_eff(c, i) × per_seat × window / (2 × eccu × w_i / w) ⌋
/// ```
///
/// `ready_eff[i]` is the ready seats of shard `i`, `shard_permille[i]` its share `w_i / w` of the claim's work
/// (permille). A shard with a zero share costs nothing and is not binding; a class with no shard costing anything has no
/// room.
pub fn palw_tir_shard_room_v1(
    ready_eff: &[u128],
    shard_permille: &[u32],
    per_seat_per_span: u128,
    window_spans: u64,
    eccu: u128,
) -> u64 {
    let attesters = u128::from(PALW_TIR_SHARD_ATTESTERS_PER_CELL_V1);
    let mut room: Option<u64> = None;
    for (ready, permille) in ready_eff.iter().zip(shard_permille) {
        if *permille == 0 {
            continue;
        }
        // ⌊ ready × per_seat × window × 1000 / (attesters × eccu × permille) ⌋.
        let numerator = ready.saturating_mul(per_seat_per_span).saturating_mul(u128::from(window_spans)).saturating_mul(1_000);
        let denominator = attesters.saturating_mul(eccu).saturating_mul(u128::from(*permille));
        let one = if denominator == 0 { 0 } else { (numerator / denominator).min(u128::from(u64::MAX)) as u64 };
        room = Some(room.map_or(one, |r| r.min(one)));
    }
    room.unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// The shard's inventory leaves (RFC §4.2: what an outsider fetches, what a readiness proof opens)
// ---------------------------------------------------------------------------------------------

/// **The inventory leaf ranges of one shard** — the leaves of every param instance a shard's occurrences read, in
/// inventory order, as maximal ranges (`pre`'s params with shard 0, `post`'s with the last, a global on every shard that
/// reads one). A closed form of the declarations (`palw_tir_artifact_v1`'s layout), so a node fetches and a chain
/// challenges exactly the shard's rows.
pub fn palw_tir_shard_inventory_ranges_v1(p: &TirProgramV1, layers: Range<usize>, with_pre: bool, with_post: bool) -> Vec<Range<u32>> {
    let instances = crate::palw_tir_artifact_v1::palw_tir_param_instances_v1(p);
    let leaves_of = |j: usize| -> u64 { crate::palw_tir_artifact_v1::palw_tir_instance_leaves_v1(p, j as u16) };
    // The params each wanted block reads.
    let reads = |bi: u8| -> Vec<u16> {
        let mut v = Vec::new();
        for n in &p.blocks[bi as usize].nodes {
            for r in &n.inputs {
                if let Ref::Param(j) = r
                    && !v.contains(j)
                {
                    v.push(*j);
                }
            }
        }
        v
    };
    let mut wanted: Vec<(u16, Option<u16>)> = Vec::new();
    let mut want = |j: u16, layer: Option<u16>| {
        if !wanted.contains(&(j, layer)) {
            wanted.push((j, layer));
        }
    };
    if with_pre {
        for j in reads(p.schedule.pre) {
            want(j, None);
        }
    }
    if with_post {
        for j in reads(p.schedule.post) {
            want(j, None);
        }
    }
    for l in layers {
        for j in reads(p.schedule.layers[l]) {
            let layer = if p.params[j as usize].per_layer { Some(l as u16) } else { None };
            want(j, layer);
        }
    }
    let mut out: Vec<Range<u32>> = Vec::new();
    let mut before: u64 = 0;
    for (j, inst) in instances.iter().enumerate() {
        let per = leaves_of(j);
        for (k, layer) in inst.iter().enumerate() {
            if wanted.contains(&(j as u16, *layer)) {
                // Checked: a program whose inventory outgrows the u32 leaf index is no class the registry admits, and a hostile
                // one must not wrap a range (an overflow panic halts block processing).
                let Some(start) = (k as u64).checked_mul(per).and_then(|x| x.checked_add(before)).and_then(|x| u32::try_from(x).ok())
                else {
                    return Vec::new();
                };
                let Some(end) = u32::try_from(per).ok().and_then(|p| start.checked_add(p)) else { return Vec::new() };
                match out.last_mut() {
                    Some(last) if last.end == start => last.end = end,
                    _ => out.push(start..end),
                }
            }
        }
        before = per.saturating_mul(inst.len() as u64).saturating_add(before);
    }
    out
}

/// **The leaves a shard readiness proof opens**: `count` leaves drawn from the shard's own rows for `(class, shard,
/// bond, span)` — ascending, distinct, each inside [`palw_tir_shard_inventory_ranges_v1`]. Fewer than `count` when the
/// shard has fewer leaves.
pub fn palw_tir_shard_readiness_leaves_v1(
    class_id: &Hash64,
    s_l: u16,
    shard: u16,
    bond: &PalwBondKeyV2,
    span: u64,
    ranges: &[Range<u32>],
    count: usize,
) -> Vec<u32> {
    let total: u64 = ranges.iter().map(|r| u64::from(r.end - r.start)).sum();
    if total == 0 {
        return Vec::new();
    }
    let want = (count as u64).min(total) as usize;
    let mut picked: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
    let mut counter = 0u64;
    while picked.len() < want && counter < 64 * want as u64 + 64 {
        let mut s = keyed(PALW_TIR_SHARD_READINESS_LEAVES_DOMAIN_V1);
        s.update(class_id.as_byte_slice());
        s.update(&s_l.to_le_bytes());
        s.update(&shard.to_le_bytes());
        s.update(&borsh::to_vec(bond).expect("a bond key is borsh-serializable"));
        s.update(&span.to_le_bytes());
        s.update(&counter.to_le_bytes());
        let digest = finish(s);
        picked.insert(u64::from_le_bytes(digest.as_bytes()[..8].try_into().expect("8 bytes")) % total);
        counter += 1;
    }
    // Fill deterministically from the front if the draw collided too often (tiny shards).
    let mut next = 0u64;
    while picked.len() < want {
        picked.insert(next);
        next += 1;
    }
    picked
        .into_iter()
        .map(|mut ord| {
            for r in ranges {
                let len = u64::from(r.end - r.start);
                if ord < len {
                    return r.start + ord as u32;
                }
                ord -= len;
            }
            unreachable!("an ordinal inside the shard's leaves")
        })
        .collect()
}

/// What a seat signs to prove it holds a shard: the network domain, the bond, the class, the shard, the span and the
/// leaves it opened.
pub fn palw_tir_shard_readiness_message_v1(
    network_domain: Hash64,
    bond: &PalwBondKeyV2,
    class_id: &Hash64,
    s_l: u16,
    shard: u16,
    span: u64,
    opened: &[(u32, Hash64)],
) -> Hash64 {
    let mut s = keyed(PALW_TIR_SHARD_READINESS_DOMAIN_MESSAGE_V1);
    s.update(network_domain.as_byte_slice());
    s.update(&borsh::to_vec(bond).expect("a bond key is borsh-serializable"));
    s.update(class_id.as_byte_slice());
    s.update(&s_l.to_le_bytes());
    s.update(&shard.to_le_bytes());
    s.update(&span.to_le_bytes());
    s.update(&(opened.len() as u32).to_le_bytes());
    for (index, leaf) in opened {
        s.update(&index.to_le_bytes());
        s.update(leaf.as_byte_slice());
    }
    finish(s)
}

// ---------------------------------------------------------------------------------------------
// The fence (RFC-0006 §10): dormant everywhere, four writes, a mirror, a drill entry
// ---------------------------------------------------------------------------------------------

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a drill arms layer-sharded panels with** (`--palw-drill-tir-shard-at`,
/// [`crate::config::drill::palw_drill_tir_shard_at_v1`]). In NO testnet-12 flag-day list: dormant on every network until
/// the node-only shadow period (decision 1) has run and the user names a height. Its `set` writes the bundle's mirror.
pub const PALW_DRILL_TIR_SHARD_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_tir_shard_v1",
    set: |params, at| {
        params.palw_tir_shard_v1 = at;
        params.sync_palw_tir_shard_v1();
    },
};

/// The drill's one-entry list.
pub const PALW_DRILL_TIR_SHARD_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_DRILL_TIR_SHARD_ENTRY];

impl Params {
    /// `palw_tir_shard_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it with a real height (a
    /// `never()` value is dormant).
    pub fn palw_tir_shard_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_tir_shard_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Are layer-sharded panels in force at `daa_score`?** `false` on every shipped preset.
    pub fn palw_tir_shard_active_at(&self, daa_score: u64) -> bool {
        self.palw_tir_shard_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`tir_shard_from_daa`), which the fold reads. Written here
    /// and nothing else; `None` where the fence is not armed (or is `never()`).
    pub fn sync_palw_tir_shard_v1(&mut self) {
        let from_daa = self.palw_tir_shard_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_tir_shard_from_daa(from_daa);
        }
    }

    /// **Layer-sharded panels' own refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * a V2 bundle whose mirror of the height is not the fence's;
    /// * arming on a ruleset that is not `ConsensusV2`;
    /// * arming without each prerequisite in force at or below it, by name: `palw_tir_v1` (the IR one-move court that
    ///   convicts what a cell finds), `palw_tir_fence2` (the IR DA units a cell's inputs are demanded by),
    ///   `palw_verification_v2` (the S1 segments and masks), `palw_rcore_plus` (Q-1..Q-7, the recount the cells
    ///   generalise) and `palw_admission_independence` (the outsider rule the per-shard outsider generalises).
    ///
    /// A `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_tir_shard_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.tir_shard_from_daa(),
            _ => None,
        };
        let armed = self.palw_tir_shard_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_tir_shard_v1 after the bundle \
                 is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_tir_shard_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let at_or_below = |other: Option<ForkActivation>| other.is_some_and(|o| o != ForkActivation::never() && o.daa_score() <= at);
        if !at_or_below(self.palw_tir_v1.map(|f| f.activation)) {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_v1 needs palw_tir_v1 in force at or below it: the IR one-move court convicts what a cell finds",
            ));
        }
        if !at_or_below(self.palw_tir_fence2) {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_v1 needs palw_tir_fence2 in force at or below it: a cell's inputs are demanded by the IR DA units",
            ));
        }
        if !at_or_below(self.palw_verification_v2) {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_v1 needs palw_verification_v2 in force at or below it: the S1 segments and masks",
            ));
        }
        if !at_or_below(self.palw_rcore_plus) {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_v1 needs palw_rcore_plus in force at or below it: the recount over cells generalises Q-3",
            ));
        }
        if !at_or_below(self.palw_admission_independence) {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_shard_v1 needs palw_admission_independence in force at or below it: the per-shard outsider generalises ADR-0147",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;

    fn bond(v: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([v; 64]), 0))
    }
    fn seat(v: u8) -> PalwPanelSeatV2 {
        PalwPanelSeatV2 { bond: bond(v), operator_id: Hash64::from_u64_word(u64::from(v)) }
    }
    fn weights(layers: &[u128], global: u128, pre: u128, post: u128) -> PalwTirShardWeightsV1 {
        PalwTirShardWeightsV1 {
            layer_bytes: layers.to_vec(),
            layers_global_bytes: global,
            pre_extra_bytes: pre,
            post_extra_bytes: post,
        }
    }

    #[test]
    fn the_plan_shape_is_two_shards_up_to_twice_the_budget_minimum_and_one_of_two_position_cuts() {
        assert!(palw_tir_shard_plan_shape_v1(2, 1, 8, 1).is_ok());
        assert!(palw_tir_shard_plan_shape_v1(2, 2, 8, 1).is_ok(), "S1 inside the shard");
        assert_eq!(
            palw_tir_shard_plan_shape_v1(1, 1, 8, 1),
            Err(PalwTirShardError::ShardCountOutOfRange(1)),
            "one shard is the flat panel"
        );
        assert_eq!(palw_tir_shard_plan_shape_v1(65, 1, 128, 1), Err(PalwTirShardError::ShardCountOutOfRange(65)));
        assert_eq!(palw_tir_shard_plan_shape_v1(4, 3, 8, 2), Err(PalwTirShardError::PositionSegmentsNotOffered(3)));
        assert_eq!(palw_tir_shard_plan_shape_v1(4, 0, 8, 2), Err(PalwTirShardError::PositionSegmentsNotOffered(0)));
        assert_eq!(palw_tir_shard_plan_shape_v1(9, 1, 8, 2), Err(PalwTirShardError::MoreShardsThanLayers { shards: 9, layers: 8 }));
        // min 4: 4 ..= 8 allowed.
        assert_eq!(palw_tir_shard_plan_shape_v1(3, 1, 64, 4), Err(PalwTirShardError::UnderSharded { declared: 3, min: 4 }));
        assert!(palw_tir_shard_plan_shape_v1(4, 1, 64, 4).is_ok());
        assert!(palw_tir_shard_plan_shape_v1(8, 1, 64, 4).is_ok());
        assert_eq!(
            palw_tir_shard_plan_shape_v1(9, 1, 64, 4),
            Err(PalwTirShardError::OverSharded { declared: 9, min: 4 }),
            "a class cannot thin its own panels"
        );
        // A class that fits one seat may still declare exactly two shards.
        assert!(palw_tir_shard_plan_shape_v1(2, 1, 8, 1).is_ok());
        assert!(palw_tir_shard_plan_shape_v1(3, 1, 8, 1).is_err());
    }

    #[test]
    fn occurrences_and_segment_positions_follow_the_rfc_cuts() {
        // 8 layers: shard 0 holds layers 0..3 (pre with it), shard 1 3..8 (post with it).
        assert_eq!(palw_tir_shard_occurrences_v1(&(0..3), 8), 0..4);
        assert_eq!(palw_tir_shard_occurrences_v1(&(3..8), 8), 4..10);
        assert_eq!(palw_tir_shard_occurrences_v1(&(3..5), 8), 4..6, "a middle shard holds no pre or post");
        assert_eq!(palw_tir_shard_segment_align_v1(2, 4), Some(4));
        assert_eq!(palw_tir_shard_segment_align_v1(3, 2), Some(6));
        assert_eq!(palw_tir_shard_segment_align_v1(0, 2), None);
        assert_eq!(palw_tir_shard_segment_align_v1(u32::MAX, 2), None, "an overflow is refused");
        // T = 100, G = 6: cuts at floor(50) -> 48.
        assert_eq!(palw_tir_shard_segment_positions_v1(100, 6, 1, 0), Some(0..100));
        assert_eq!(palw_tir_shard_segment_positions_v1(100, 6, 2, 0), Some(0..48));
        assert_eq!(palw_tir_shard_segment_positions_v1(100, 6, 2, 1), Some(48..100));
        assert_eq!(palw_tir_shard_segment_positions_v1(100, 6, 2, 2), None);
        assert_eq!(palw_tir_shard_segment_positions_v1(100, 0, 2, 0), None);
        // A job shorter than the alignment: the first segment is empty, the last is everything.
        assert_eq!(palw_tir_shard_segment_positions_v1(5, 6, 2, 0), Some(0..0));
        assert_eq!(palw_tir_shard_segment_positions_v1(5, 6, 2, 1), Some(0..5));
    }

    #[test]
    fn the_partition_minimises_the_widest_shard_and_is_exact() {
        // Eight equal layers, 4 shards: two layers each; pre (10) with shard 0 and post (30) with the last.
        let w = weights(&[100; 8], 7, 10, 30);
        let parts = palw_tir_shard_partition_v1(&w, 4).unwrap();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts.first().unwrap().start, 0);
        assert_eq!(parts.last().unwrap().end, 8);
        for pair in parts.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "contiguous");
        }
        assert!(parts.iter().all(|r| !r.is_empty()), "no empty shard");
        let widest = parts.iter().map(|r| w.of_range(r.start, r.end)).max().unwrap();
        // The optimum over every contiguous 4-partition, by brute force.
        let mut best = u128::MAX;
        for a in 1..8 {
            for b in a + 1..8 {
                for c in b + 1..8 {
                    let cuts = [0, a, b, c, 8];
                    let m = cuts.windows(2).map(|x| w.of_range(x[0], x[1])).max().unwrap();
                    best = best.min(m);
                }
            }
        }
        assert_eq!(widest, best, "the widest shard is the minimum there is");
        // Deterministic.
        assert_eq!(palw_tir_shard_partition_v1(&w, 4), Some(parts));
        // Uneven layers: a heavy layer gets a shard to itself.
        let uneven = weights(&[10, 10, 500, 10, 10], 0, 0, 0);
        let p = palw_tir_shard_partition_v1(&uneven, 3).unwrap();
        assert!(p.iter().any(|r| r.start == 2 && r.end == 3), "{p:?}");
        assert_eq!(palw_tir_shard_partition_v1(&uneven, 0), None);
        assert_eq!(palw_tir_shard_partition_v1(&uneven, 6), None, "more shards than layers");
        assert_eq!(palw_tir_shard_partition_v1(&uneven, 5).unwrap().len(), 5);
    }

    #[test]
    fn the_fewest_shards_that_fit_the_budget() {
        let w = weights(&[100; 8], 0, 0, 0);
        assert_eq!(palw_tir_shard_min_shards_v1(&w, 800), 1);
        assert_eq!(palw_tir_shard_min_shards_v1(&w, 799), 2);
        assert_eq!(palw_tir_shard_min_shards_v1(&w, 400), 2);
        assert_eq!(palw_tir_shard_min_shards_v1(&w, 399), 3);
        assert_eq!(palw_tir_shard_min_shards_v1(&w, 100), 8);
        assert_eq!(palw_tir_shard_min_shards_v1(&w, 1), 8, "one layer a shard is the floor");
    }

    #[test]
    fn permille_tables_sum_to_exactly_one_thousand() {
        for raw in [vec![1u128, 1, 1], vec![5, 0, 0, 7], vec![0, 0], vec![u64::MAX as u128, 1], vec![333, 333, 334], vec![1]] {
            let t = palw_largest_remainder_permille_v1(&raw);
            assert_eq!(t.len(), raw.len());
            assert_eq!(t.iter().map(|x| u32::from(*x)).sum::<u32>(), 1_000, "{raw:?} -> {t:?}");
        }
        assert_eq!(palw_largest_remainder_permille_v1(&[1, 1, 1]), vec![334, 333, 333], "ties to the lowest index");
        assert!(palw_largest_remainder_permille_v1(&[]).is_empty());
    }

    #[test]
    fn a_seats_lock_scales_with_its_cells_and_is_floored() {
        assert_eq!(palw_tir_shard_lock_v1(1_000_000, 1_000), 1_000_000);
        assert_eq!(palw_tir_shard_lock_v1(1_000_000, 250), 250_000);
        assert_eq!(palw_tir_shard_lock_v1(1_000_000, 4), 125_000, "the floor: an eighth");
        assert_eq!(palw_tir_shard_lock_v1(1_000_000, 5_000), 1_000_000, "never above the full lock");
        assert_eq!(palw_tir_shard_lock_v1(0, 500), 0);
        assert_eq!(palw_tir_shard_lock_v1(1, 125), 1, "a nonzero lock is never 0");
        // The panel's total lock over a claim is unchanged by sharding (RFC §6.1): four shards of 250‰, each cell
        // attested by 2 class seats + the outsider.
        let per_seat = palw_tir_shard_lock_v1(1_000_000, 250);
        assert_eq!(per_seat * 3 * 4, 3_000_000, "3 attesters x the whole claim's lock");
    }

    #[test]
    fn the_pay_split_conserves_the_reward_and_never_gives_a_silent_seats_share_to_the_producer() {
        for reward in [0u64, 1, 999, 1_000, 123_456_789, u64::MAX / 2] {
            for credited in [[true; 5], [false; 5], [true, false, true, false, true]] {
                let s = palw_tir_shard_split_v1(reward, 200, &[250, 250, 250, 125, 1_000], &credited);
                assert_eq!(s.producer as u128 + s.paid.iter().map(|p| *p as u128).sum::<u128>() + s.reserve as u128, reward as u128);
                assert_eq!(
                    s.producer,
                    reward - ((reward as u128) * 200 / 1000) as u64,
                    "the producer's share is a function of the reward alone"
                );
                for (p, c) in s.paid.iter().zip(credited) {
                    assert!(c || *p == 0, "an uncredited seat is paid nothing");
                }
            }
        }
        let s = palw_tir_shard_split_v1(10_000, 200, &[250, 500], &[true, true]);
        assert_eq!(s.paid, vec![666, 1_333], "pay follows the work (at least the floor each)");
    }

    #[test]
    fn a_stored_panel_is_shard_major_and_recognised_by_its_length() {
        let seats: Vec<PalwPanelSeatV2> = (1..=8).map(seat).collect();
        let slice = palw_tir_panel_shard_slice_v1(&seats, 2, true, 1).expect("2 shards x (3 + outsider)");
        assert_eq!(slice.iter().map(|s| s.bond).collect::<Vec<_>>(), vec![bond(5), bond(6), bond(7), bond(8)]);
        assert_eq!(palw_tir_panel_shard_slice_v1(&seats, 2, false, 0), None, "a flat 8 is not 2 x 3");
        assert_eq!(palw_tir_panel_shard_slice_v1(&seats, 2, true, 2), None, "a shard out of range");
        let six: Vec<PalwPanelSeatV2> = (1..=6).map(seat).collect();
        assert_eq!(palw_tir_panel_shard_slice_v1(&six, 2, false, 1).unwrap().len(), 3);
        assert_eq!(palw_tir_panel_shard_slice_v1(&seats[..5], 2, true, 0), None, "a flat five seats is no stratified panel");
    }

    #[test]
    fn readiness_classes_and_seeds_are_independent_per_shard() {
        let class = Hash64::from_u64_word(9);
        assert_ne!(palw_tir_shard_ready_class_v1(&class, 4, 0), palw_tir_shard_ready_class_v1(&class, 4, 1));
        assert_ne!(palw_tir_shard_ready_class_v1(&class, 4, 0), palw_tir_shard_ready_class_v1(&class, 8, 0), "another plan");
        assert_ne!(palw_tir_shard_ready_class_v1(&class, 4, 0), class);
        let (seed, claim) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        assert_ne!(palw_tir_shard_seed_v1(&seed, &claim, 0), palw_tir_shard_seed_v1(&seed, &claim, 1));
        assert_eq!(palw_tir_shard_seed_v1(&seed, &claim, 0), palw_tir_shard_seed_v1(&seed, &claim, 0));
    }

    #[test]
    fn the_assignment_inside_a_shard_is_s1_or_the_whole_shard() {
        let (seed, claim) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        // Layers only: every class seat attests the shard.
        assert_eq!(palw_tir_shard_assignment_v1(&seed, &claim, 0, 1), vec![PalwSegmentMaskV2::full(1); 3]);
        // S1: one full seat and a partial per segment — every segment attested by exactly two seats.
        for shard in 0..8u16 {
            let masks = palw_tir_shard_assignment_v1(&seed, &claim, shard, 2);
            assert_eq!(masks.len(), 3);
            assert_eq!(masks.iter().filter(|m| m.is_full(2)).count(), 1, "one full-shard seat");
            for segment in 0..2 {
                assert_eq!(
                    masks.iter().filter(|m| m.covers(segment)).count(),
                    2,
                    "shard {shard} segment {segment}: the full seat and one partial"
                );
            }
        }
        // The shards draw independently: not every shard picks the same full seat.
        let fulls: std::collections::BTreeSet<usize> =
            (0..16u16).map(|s| palw_tir_shard_assignment_v1(&seed, &claim, s, 2).iter().position(|m| m.is_full(2)).unwrap()).collect();
        assert!(fulls.len() > 1, "{fulls:?}");
        // The outsider holds the whole shard.
        assert_eq!(palw_tir_shard_seat_mask_v1(&seed, &claim, 0, 2, true, 0), Some(PalwSegmentMaskV2::full(2)));
        assert_eq!(palw_tir_shard_seat_mask_v1(&seed, &claim, 0, 2, true, 4), None, "past the slice");
        assert_eq!(palw_tir_shard_seat_mask_v1(&seed, &claim, 0, 1, false, 2), Some(PalwSegmentMaskV2::full(1)));
    }

    fn valid(b: u8, mask: PalwSegmentMaskV2) -> (PalwBondKeyV2, PalwReceiptVerdictV2, PalwSegmentMaskV2) {
        (bond(b), PalwReceiptVerdictV2::Valid, mask)
    }

    #[test]
    fn a_shard_licenses_on_two_attesters_per_cell_and_its_outsiders_valid() {
        let (seed, claim) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let slice: Vec<PalwPanelSeatV2> = vec![seat(10), seat(11), seat(12), seat(13)]; // outsider, then three class seats
        let full1 = PalwSegmentMaskV2::full(1);
        // Layers only (S_P = 1).
        let ok =
            palw_tir_shard_part_verdict_v1(&slice, &seed, &claim, 0, 1, true, &[valid(10, full1), valid(11, full1), valid(12, full1)]);
        match ok {
            PalwTirShardPartVerdictV1::Licensed { signers, cell_counts } => {
                assert_eq!(signers.len(), 3);
                assert_eq!(cell_counts, vec![3], "two class seats and the outsider");
            }
            other => panic!("{other:?}"),
        }
        // Without the outsider: not a licence, however many class seats said Valid.
        assert!(matches!(
            palw_tir_shard_part_verdict_v1(&slice, &seed, &claim, 0, 1, true, &[valid(11, full1), valid(12, full1), valid(13, full1)]),
            PalwTirShardPartVerdictV1::Short(why) if why.contains("outsider")
        ));
        // One class seat and the outsider: a cell attested once.
        assert!(matches!(
            palw_tir_shard_part_verdict_v1(&slice, &seed, &claim, 0, 1, true, &[valid(10, full1), valid(11, full1)]),
            PalwTirShardPartVerdictV1::Short(why) if why.contains("fewer than two")
        ));
        // A mask that is not the assignment counts for nothing (Q-6).
        assert!(matches!(
            palw_tir_shard_part_verdict_v1(
                &slice,
                &seed,
                &claim,
                0,
                1,
                true,
                &[valid(10, full1), valid(11, PalwSegmentMaskV2::full(2)), valid(12, full1)]
            ),
            PalwTirShardPartVerdictV1::Short(_)
        ));
        // A stranger's receipt, an abstention and a duplicate count for nothing.
        let abstain = (bond(13), PalwReceiptVerdictV2::Incapable, full1);
        assert!(matches!(
            palw_tir_shard_part_verdict_v1(
                &slice,
                &seed,
                &claim,
                0,
                1,
                true,
                &[valid(10, full1), valid(99, full1), valid(11, full1), valid(11, full1), abstain]
            ),
            PalwTirShardPartVerdictV1::Short(_)
        ));
        // No outsider on the claim (a flat-population claim): two class seats suffice.
        let class_only = &slice[1..];
        assert!(matches!(
            palw_tir_shard_part_verdict_v1(class_only, &seed, &claim, 0, 1, false, &[valid(11, full1), valid(12, full1)]),
            PalwTirShardPartVerdictV1::Licensed { .. }
        ));
    }

    #[test]
    fn s1_inside_a_shard_needs_the_full_seat_and_the_partial_of_each_segment() {
        let (seed, claim) = (Hash64::from_u64_word(7), Hash64::from_u64_word(8));
        let slice: Vec<PalwPanelSeatV2> = vec![seat(20), seat(21), seat(22), seat(23)];
        let masks = palw_tir_shard_assignment_v1(&seed, &claim, 3, 2);
        let all: Vec<_> = (0..3).map(|i| valid(21 + i as u8, masks[i])).collect();
        let outsider = valid(20, palw_tir_shard_outsider_mask_v1(2));
        let mut set = vec![outsider];
        set.extend(all.iter().cloned());
        match palw_tir_shard_part_verdict_v1(&slice, &seed, &claim, 3, 2, true, &set) {
            PalwTirShardPartVerdictV1::Licensed { cell_counts, .. } => assert_eq!(cell_counts, vec![3, 3]),
            other => panic!("{other:?}"),
        }
        // Drop the full seat: each segment keeps one partial plus the outsider, but only ONE class seat.
        let full = masks.iter().position(|m| m.is_full(2)).unwrap();
        let without_full: Vec<_> =
            std::iter::once(outsider).chain(all.iter().enumerate().filter(|(i, _)| *i != full).map(|(_, r)| r.clone())).collect();
        assert!(matches!(
            palw_tir_shard_part_verdict_v1(&slice, &seed, &claim, 3, 2, true, &without_full),
            PalwTirShardPartVerdictV1::Short(_)
        ));
    }

    #[test]
    fn the_recount_is_the_weakest_cell() {
        assert_eq!(palw_tir_shard_basis_k_v1(&[3, 3, 3, 3]), 3);
        assert_eq!(palw_tir_shard_basis_k_v1(&[3, 2, 3, 3]), 2);
        assert_eq!(palw_tir_shard_basis_k_v1(&[3, 0]), 0);
        assert_eq!(palw_tir_shard_basis_k_v1(&[]), 0, "no cell, no recount");
        let masks =
            [PalwSegmentMaskV2::full(2), PalwSegmentMaskV2::single(0), PalwSegmentMaskV2::single(1), PalwSegmentMaskV2::full(2)];
        assert_eq!(palw_tir_shard_cell_counts_v1(2, &masks), vec![3, 3], "capped at three");
        assert_eq!(palw_tir_shard_cell_counts_v1(2, &masks[..2]), vec![2, 1]);
    }

    #[test]
    fn the_v4_message_binds_the_shard_and_the_mask() {
        let (net, claim) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let m = |shard: u16, mask: PalwSegmentMaskV2, daa: u64| {
            palw_receipt_message_v4(net, claim, PalwReceiptVerdictV2::Valid, daa, shard, mask)
        };
        let base = m(0, PalwSegmentMaskV2::single(0), 10);
        assert_ne!(base, m(1, PalwSegmentMaskV2::single(0), 10), "another shard");
        assert_ne!(base, m(0, PalwSegmentMaskV2::full(2), 10), "a widened mask");
        assert_ne!(base, m(0, PalwSegmentMaskV2::single(0), 11), "another time");
        assert_ne!(
            base,
            crate::palw_panel_v2::palw_receipt_message_v3(net, claim, PalwReceiptVerdictV2::Valid, 10, PalwSegmentMaskV2::single(0)),
            "never a V3 message"
        );
    }

    #[test]
    fn the_room_is_the_binding_shards() {
        // 4 shards of 250‰, 8 ready seats each, per_seat 1,000, window 10, eccu 100, 2 attesters:
        // ⌊8 × 1000 × 10 × 1000 / (2 × 100 × 250)⌋ = 1,600.
        let room = palw_tir_shard_room_v1(&[8, 8, 8, 8], &[250, 250, 250, 250], 1_000, 10, 100);
        assert_eq!(room, 1_600);
        // One thin shard binds the class.
        assert_eq!(palw_tir_shard_room_v1(&[8, 2, 8, 8], &[250, 250, 250, 250], 1_000, 10, 100), 400);
        // The whole-model room of the same seats: 8 seats, the whole 1,000‰ → a quarter of a shard's.
        assert_eq!(palw_tir_shard_room_v1(&[8], &[1_000], 1_000, 10, 100), 400);
        assert_eq!(palw_tir_shard_room_v1(&[], &[], 1_000, 10, 100), 0);
        assert_eq!(palw_tir_shard_room_v1(&[8, 8], &[0, 0], 1_000, 10, 100), 0, "no shard costs anything: no room is claimed");
        assert_eq!(palw_tir_shard_room_v1(&[8], &[500], 1_000, 10, 0), 0, "a class that costs nothing has no room");
    }

    #[test]
    fn a_row_request_is_none_of_the_other_interval_kinds() {
        let packed = palw_tir_rows_request_index_v1(12_345).unwrap();
        assert_eq!(palw_tir_rows_request_decode_v1(packed), Some(12_345));
        assert!(palw_tir_rows_request_index_v1(PALW_TIR_ROWS_REQUEST_BIT_V1).is_none());
        // Not read as a plain interval kind by any of the others' decoders.
        assert!(crate::palw_leaf_evidence_v1::palw_leaf_evidence_request_decode_v1(packed).is_none());
        assert!(crate::palw_segment_resume_v1::palw_segment_opening_request_decode_v1(packed).is_none());
        assert_eq!(packed & (7 << 29), 0);
        // And none of the others' indices is read as a row request.
        for other in [3u32, 1 << 29 | 7, 1 << 30 | 7, 1 << 31 | 7, 1 << 29 | 1 << 30] {
            assert!(palw_tir_rows_request_decode_v1(other).is_none(), "{other:#x}");
        }
        assert!(PALW_TIR_ROWS_PER_REPLY_MAX_V1 <= crate::palw_tir_court_v1::PALW_TIR_STEP_RUN_MAX_LEAVES_V1);
        // The run kind is neither the row kind nor any other, and they do not read each other's indices.
        let run = palw_tir_runs_request_index_v1(9_999).unwrap();
        assert_eq!(palw_tir_runs_request_decode_v1(run), Some(9_999));
        assert!(palw_tir_rows_request_decode_v1(run).is_none());
        assert!(palw_tir_runs_request_decode_v1(packed).is_none());
        assert!(palw_tir_runs_request_decode_v1(palw_tir_rows_request_index_v1(1 << 27 | 5).unwrap()).is_none());
        assert!(crate::palw_leaf_evidence_v1::palw_leaf_evidence_request_decode_v1(run).is_none());
        assert!(crate::palw_segment_resume_v1::palw_segment_opening_request_decode_v1(run).is_none());
        assert!(palw_tir_runs_request_index_v1(PALW_TIR_RUNS_REQUEST_BIT_V1).is_none());
    }

    #[test]
    fn every_domain_of_the_fence_is_its_own() {
        let unique: std::collections::BTreeSet<&&[u8]> = PALW_TIR_SHARD_V1_ALL_DOMAINS.iter().collect();
        assert_eq!(unique.len(), PALW_TIR_SHARD_V1_ALL_DOMAINS.len());
        for c in [PALW_TIR_SHARD_PLAN_MLDSA87_CONTEXT, PALW_RECEIPT_V4_MLDSA87_CONTEXT, PALW_TIR_SHARD_READINESS_MLDSA87_CONTEXT] {
            assert!(c.ends_with(b"mldsa87/v1"));
            assert!(
                !crate::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V5.contains(&c),
                "not in testnet-12's committed set: the Some-only fence covers it"
            );
        }
    }
}
