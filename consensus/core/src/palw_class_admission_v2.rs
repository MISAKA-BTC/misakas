//! **The gate a class must pass to join a chain that is already running** (ADR-0039's
//! "weightless until the catalog closes", used in the direction it was written for).
//!
//! # Why this exists
//!
//! `verify_palw_genesis_v2` checks the class set a network is born with, against a catalog whose
//! root is inside `PalwConsensusParamsV2` and therefore inside `palw_ruleset_id_v2`. That is the
//! right shape for genesis and the wrong shape for later: a class that does not exist yet has no
//! `artifact_root`, so it cannot be in the genesis catalog, so under the genesis gate alone **a
//! second class is a flag day** — a new ruleset id, a coordinated upgrade, a different network.
//!
//! The plan the RC is built on is the opposite: ship the BASE-0 liveness floor now, add a larger
//! class once its weights and its PTQ pipeline exist. This module is the missing half of that —
//! the checks the genesis loader runs, restated so they can run against ONE registration and the
//! shape profile it carries, with no pre-committed catalog to read from.
//!
//! # Derive, never declare
//!
//! The genesis catalog can afford to state `reachable_kernels`, `canonical_step_leaf_count` and
//! `max_step_leaf_count`, because its root is hashed into the ruleset and an operator who lies
//! contradicts a commitment the chain already made. A post-genesis registration has no such
//! anchor, so **nothing here is read from the registration that can be computed from the graph**:
//! the reachable set comes from the profile's own nodes, both leaf counts come from
//! `palw_step`, and the entry this returns is a derivation rather than a copy. The only fields a
//! registrant supplies are the ones no function can invent — `artifact_root` (the weights) and the
//! economic terms — and `pwu_per_inference`, which is checked against the count rather than
//! trusted.
//!
//! # What this does NOT do
//!
//! It does not admit anything. There is no carrier for `ClassRegistered` outside the genesis
//! object list in this tree, so this module is consensus-inert: nothing calls it, exactly like
//! every other V2 brick before its wiring lands. When the carriage does carry a registration, this
//! is the function it must call before the transition runs — and the transition is deliberately
//! left alone, because `apply_palw_transition_v2` is a pure state machine and adjudicability is an
//! arithmetic fact about a graph, not a fact about state.
//!
//! Nor does it decide share. `granted_share_table_v2` owns that, and it refuses a zero grant by
//! construction (`min_grantable_share_permille` is at least 1): a class with no share has a zero
//! epoch budget, which is a class that can never mine. So "register it weightless and activate it
//! later" is not available, and the honest version of the plan is **register it at the minimum
//! grantable share** — one permille, donated from the incumbents, which is the smallest weight the
//! ruleset admits rather than none.

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;

use crate::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
use crate::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2};
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use crate::palw_step::{PalwShapeProfileV3, step_leaf_count, worst_case_step_leaf_count_v1};
use crate::palw_step_refute::catalogued_kernel_ids_v1;
use crate::palw_v2::PalwJobContextV2;

/// **The `max_step_leaf_count` a network must freeze at genesis to keep a second class possible.**
///
/// `PalwCourtParamsV2::max_step_leaf_count` is a `PalwConsensusParamsV2` field, and the bundle is
/// what `palw_ruleset_id_v2` hashes. A class whose worst case is deeper than the ladder therefore
/// cannot join a running chain at all — it needs a new ruleset, which is a flag day. Unlike every
/// other obstacle to adding a class later, **this one cannot be repaired later**: by the time the
/// second class exists, the number is already inside the network's identity.
///
/// Provisioning it at the step space's own cap costs almost nothing, because the ladder is
/// `ceil(log2(leaves)) + terminal` ROUNDS. Measured on this tree
/// (`misaka-palw-base0/src/bin/base0-class-sizing.rs`), and pinned by
/// `provisioning_the_whole_step_space_costs_four_rounds`:
///
/// | provisioned for | leaves | bisection rounds |
/// |---|---|---|
/// | the RC floor alone | 184,456 | 18 |
/// | the whole step space | 4,194,304 | 22 |
///
/// The floor's figure is its WHOLE CONTEXT as prefill (`worst_case_step_leaf_count_v1`), not the
/// 47,020 of its declared 64/64 job — the ladder must reach the longest job a class admits, or it
/// admits a class an attacker picks the job length for. An earlier draft of this table used the
/// declared job and put the price at six rounds; the test below is what corrected it.
///
/// Four extra rounds of worst-case prosecution — paid only when a court actually runs to its worst
/// case — buys every class that could ever be adjudicable, because nothing deeper than
/// `PALW_STEP_MAX_LEAVES` is admissible in the first place (`worst_case_step_leaf_count_v1`
/// refuses it).
pub const PALW_RC_COURT_MAX_STEP_LEAF_COUNT: u64 = crate::palw_step::PALW_STEP_MAX_LEAVES;

/// **The three cost ceilings an RC identity must freeze, and why they are constants rather than
/// an operator's choice** (ADR-0049 Decision C; the second of the road map's two decisions that
/// expire).
///
/// `PALW_RC_COURT_MAX_STEP_LEAF_COUNT` above bounds how many rounds a dispute takes. These bound
/// what a round COSTS, and they sit in the same place for the same reason: they are
/// `PalwConsensusParamsV2` fields, so they are inside `palw_ruleset_id_v2`, so a class that exceeds
/// them cannot join a running chain — it needs a new ruleset, which is a flag day. The ladder gate
/// was already a refusal in `assemble_palw_rc_identity_v2`; these were not, and an RC genesis could
/// be minted with any ceiling at all, including the generous default nobody had checked against a
/// transaction.
///
/// They are the shipped defaults, restated here as an RC commitment. Keeping two names is
/// deliberate: [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] is what a bundle gets when a caller
/// does not say, and this is what the RC network's identity IS. A future ruleset may move the
/// default; moving what testnet-11's genesis froze would be a different network.
pub const PALW_RC_COURT_MAX_CLOSE_BYTES: u64 = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
/// See [`PALW_RC_COURT_MAX_CLOSE_BYTES`]. The floor's widest step recomputes 32,768
/// multiply-accumulates; this is 512 times that.
pub const PALW_RC_COURT_MAX_TERMINAL_MACS: u64 = crate::palw_mode_v2::DEFAULT_MAX_TERMINAL_MACS;
/// See [`PALW_RC_COURT_MAX_CLOSE_BYTES`]. The floor's widest step reads two operands; a
/// gated-delta-net recurrence reads five.
pub const PALW_RC_COURT_MAX_OPERAND_COUNT: u32 = crate::palw_mode_v2::DEFAULT_MAX_OPERAND_COUNT;

/// **What one terminal adjudication of this class costs, derived from its graph** (ADR-0049
/// Decision C).
///
/// The ladder bounds how many rounds a dispute takes. Nothing bounded what a round COSTS, and the
/// answer used to be the model's size: the matmul arm opened `out_dim x in_len` — the whole matrix —
/// which is ~223 MiB for Qwen2.5-1.5B's unembed against a court-close budget of 152 KB. Decision B
/// made the opening tile-local; this is what turns "small" into "bounded", so a class cannot be
/// admitted whose disputes are unprosecutable in a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCourtCostV1 {
    /// Bytes the most expensive single close costs to carry: the artifact opening, the disputed
    /// step's own leaves and input leaves, the generated-token pin where a gather needs one, and
    /// every Merkle path element proving any of them. The units
    /// `palw_court_v2::arithmetic_close_bytes_v2` counts on a real object.
    pub max_close_bytes: u64,
    /// Multiply-accumulates a full node performs to recompute that step — its own CPU, on
    /// peer-supplied input.
    pub max_terminal_macs: u64,
    /// Rows a single step reads: its `input_refs` plus at most one weight operand. Bounds
    /// deserialization work before any arithmetic runs.
    pub max_operand_count: u32,
}

/// Widths a node can produce, worst case, from the profile's own geometry.
///
/// `KvScaled` is resolved at the CONTEXT maximum rather than at some typical position, because a
/// ceiling that held for a typical job and not for the longest one would admit a class an attacker
/// picks the job length for — the same rule `worst_case_step_leaf_count_v1` states.
fn node_out_width_v1(node: &crate::palw_step::PalwStepNodeV1, profile: &PalwShapeProfileV3) -> Option<u64> {
    match node.out_len {
        crate::palw_step::PalwStepOutLenV1::Fixed { elements } => Some(elements as u64),
        crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => (multiplier as u64).checked_mul(profile.n_ctx as u64),
    }
}

/// The width of one input reference: a sentinel's own width, or an earlier node's output.
fn input_width_v1(r: u16, table: &[crate::palw_step::PalwStepNodeV1], profile: &PalwShapeProfileV3) -> Option<u64> {
    use crate::palw_step::{PALW_STEP_INPUT_CHECKPOINT_STATE, PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN};
    let kv_dim = (profile.attn_kv_heads as u64).checked_mul(profile.attn_head_dim as u64)?;
    match r {
        PALW_STEP_INPUT_LAYER_IN => Some(profile.hidden_dim as u64),
        PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => kv_dim.checked_mul(profile.n_ctx as u64),
        // The recurrent state chunk. Bounded by the widest state a GDN head can carry; a profile
        // with no GDN reaches this only if it names the sentinel, and then zero is the honest width.
        PALW_STEP_INPUT_CHECKPOINT_STATE => {
            (profile.gdn_heads as u64).checked_mul(profile.gdn_head_k_dim as u64)?.checked_mul(profile.gdn_head_v_dim as u64)
        }
        // An intra-table index: references are backwards, so the node exists by validation.
        i => table.get(i as usize).and_then(|n| node_out_width_v1(n, profile)),
    }
}

/// **How many Merkle path elements an ARTIFACT opening can carry.**
///
/// `open_artifact_leaf_v1` addresses a leaf by `u32`, so an inventory has at most `2^32` leaves
/// however finely a registrant tiles its weights, and a path over them is at most 32 elements.
/// Derived from the format rather than from the inventory, because the inventory is the
/// registrant's and this is a bound on what the registrant may do.
pub const PALW_ARTIFACT_MAX_PATH_ELEMENTS: u64 = 32;

/// A Merkle path element is one `Hash64`.
const PATH_ELEMENT_BYTES: u64 = 64;

/// The tile length of whichever node a step's input reference resolves to — that is the tiling the
/// input openings are cut on, and it is not this node's own.
fn source_tile_len_v1(table: &[crate::palw_step::PalwStepNodeV1], node: &crate::palw_step::PalwStepNodeV1, r: u16) -> u64 {
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwStepNodeRoleV1};
    let by_role = |want: PalwStepNodeRoleV1| table.iter().find(|n| n.role == want).map(|n| n.tile_len as u64);
    match r {
        PALW_STEP_INPUT_KV_K => by_role(PalwStepNodeRoleV1::KCacheWrite),
        PALW_STEP_INPUT_KV_V => by_role(PalwStepNodeRoleV1::VCacheWrite),
        i if (i as usize) < table.len() => Some(table[i as usize].tile_len as u64),
        _ => None,
    }
    .unwrap_or(node.tile_len as u64)
    .max(1)
}

/// **What the generated-token pin costs, by the scheme the CLASS committed to** (ADR-0049
/// Decision E, and the tiled scheme that followed it).
///
/// `logits_scheme_id` is a field of `PalwShapeProfileV3`, so it is inside `shape_profile_id` and
/// therefore inside the class id: a class cannot change how its decode tokens are pinned without
/// becoming a different class. That is what makes this dispatchable at all. The earlier shape —
/// a `trace_scheme_id` on the job context — would have let a class register under the cheap scheme
/// and produce under the expensive one, which is "an attacker picks the job length" with the
/// scheme substituted for the length.
///
/// The two prices are the two the court's own cost gate counts on a real object
/// (`check_close_cost_v2`), restated from the geometry:
///
/// * **flat** — `base0_logits_trace_root_v1` is a hash over every row, so recomputing it needs all
///   of them: `decode x vocabulary x 4`. At Qwen3.6's 248,320 lanes ONE row is 993 KB.
/// * **tiled** — two tile openings, three Merkle paths and the ids. `PALW_LOGITS_TILE_LANES` is
///   4,096, so a tile is 16 KiB and the paths are `ceil(log2(vocab / lanes))` deep within a row
///   plus `ceil(log2(decode))` over the rows tree.
///
/// An unrecognised scheme is priced at the most expensive one this function knows. Admission
/// refuses such a class outright, so this is belt and braces — but the belt has to fail closed,
/// because a scheme nobody can price is not a scheme nobody can register.
fn decode_pin_price_v1(profile: &PalwShapeProfileV3, decode: u64) -> Option<u64> {
    // The float lane's pin is a trace SUMMARY and the ids; the rows stay in the event tree the
    // summary roots, so there is nothing here for it to carry.
    if profile.lane == crate::palw_step::PalwStepLaneV1::Float32 {
        return Some(0);
    }
    let vocab = profile.vocab_size as u64;
    let flat = decode.checked_mul(vocab)?.checked_mul(4)?;
    if profile.logits_scheme_id != crate::palw_step_refute::tiled_logits_scheme_id_v1() {
        return Some(flat);
    }
    let lanes = crate::palw_step_refute::PALW_LOGITS_TILE_LANES as u64;
    let tile_bytes = lanes.min(vocab.max(1)).checked_mul(4)?;
    let ceil_log2 = |n: u64| -> u64 { if n <= 1 { 0 } else { u64::from((n - 1).next_power_of_two().trailing_zeros()) } };
    let within_row = ceil_log2(vocab.div_ceil(lanes));
    let across_rows = ceil_log2(decode);
    let paths = within_row.checked_mul(2)?.checked_add(across_rows)?.checked_mul(64)?;
    // Two tiles (the committed lane's and the beating lane's), their paths, the row opening's
    // path, and every generated id.
    tile_bytes.checked_mul(2)?.checked_add(paths)?.checked_add(decode.checked_mul(4)?)
}

/// The cost of the most expensive step this profile can be challenged at.
///
/// Every quantity is read off the graph. Nothing is declared, so a registration cannot understate
/// what prosecuting it will cost — which is the same reason `pwu_per_inference` is checked against
/// a count rather than trusted.
///
/// # It measures the CLOSE, not the weight bytes
///
/// The bound is `max_close_bytes` and the units are the ones
/// `palw_court_v2::arithmetic_close_bytes_v2` counts on a real object: opened payload plus every
/// Merkle path element proving it, artifact side and step side alike. That matters because the
/// step side is the larger one. A `MatMulQuant` at an attention site opens no weight at all and
/// reads the whole KV HISTORY — `n_ctx` positions, each cut into tiles, each tile carrying its own
/// path — so the shipped floor's most expensive close at a 64/64 job is 750,716 bytes beside a
/// derived 32,768. Deriving the weight bytes alone said "32 KiB" about a close no block could
/// carry.
///
/// # Over the class's LONGEST job
///
/// Path length comes from `worst_case_step_leaf_count_v1` and the KV history from `n_ctx`, for the
/// reason `node_out_width_v1` already gives: a ceiling that held for a typical job and not for the
/// longest one would admit a class an attacker picks the job length for. Nothing charges an
/// attempt more for a longer job — `pwu_per_inference` is per inference — so the longest job the
/// class admits is a job the class must be prosecutable at.
pub fn derive_court_cost_v1(profile: &PalwShapeProfileV3) -> Result<PalwCourtCostV1, PalwClassAdmissionError> {
    use crate::palw_step::{PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PalwStepOpKindV1 as Op};
    let over = || PalwClassAdmissionError::Profile("the class's court cost overflows a u64".to_string());
    let mut cost = PalwCourtCostV1 { max_close_bytes: 0, max_terminal_macs: 0, max_operand_count: 0 };

    // The deepest step tree this class can be disputed in, and therefore the longest path any one
    // step leaf can carry.
    let worst_leaves = worst_case_step_leaf_count_v1(profile).map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    let step_path_bytes = u64::from(worst_leaves.max(2).next_power_of_two().trailing_zeros()) * PATH_ELEMENT_BYTES;
    let kv_dim = (profile.attn_kv_heads as u64).checked_mul(profile.attn_head_dim as u64).ok_or_else(over)?;
    let n_ctx = profile.n_ctx as u64;

    for table in [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes] {
        for node in table.iter() {
            let out_w = node_out_width_v1(node, profile).ok_or_else(over)?;
            let tile = (node.tile_len as u64).min(out_w.max(1));
            let in_w = match node.input_refs.first() {
                Some(r) => input_width_v1(*r, table, profile).ok_or_else(over)?,
                None => 0,
            };

            // The artifact bytes this node's parameters occupy, per catalogued kernel. A node with
            // no weight operand opens nothing from the artifact — its inputs ride the leg.
            // ONE list, in the module that owns the slice derivations — this arm carried its own
            // copy for a round and priced a lane-sliced node as if it opened whole rows.
            let lane_sliced = crate::palw_step_refute::palw_kernel_is_lane_sliced_v1(node.kernel_semantics_id)
                || crate::palw_step_refute::palw_kernel_is_head_sliced_v1(node.kernel_semantics_id);
            let strided_combine =
                node.kernel_semantics_id == crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_MOE_COMBINE);
            let head_sliced_gdn = node.op_kind == Op::GatedDeltaNet
                && node.kernel_semantics_id == crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_GDN_STEP);
            let sliced_conv = node.op_kind == Op::SsmConv
                && node.kernel_semantics_id == crate::palw_step::kernel_semantics_id_v1(crate::palw_step_refute::KDESC_Q36_SSM_CONV);
            let opening = if node.weight_name.is_empty() {
                0
            } else {
                let attn_reduction = [crate::palw_step_refute::KDESC_A16_ATTN_SCORES, crate::palw_step_refute::KDESC_A16_ATTN_VALUES]
                    .iter()
                    .any(|d| crate::palw_step::kernel_semantics_id_v1(d) == node.kernel_semantics_id);
                let payload = match node.op_kind {
                    // The attention reductions multiply ACTIVATIONS (q·K, probs·V); their named
                    // "weight" is one narrowing triple, not a matrix, so charging `tile × in_w`
                    // priced an opening the executor never requests — 65 KiB of nothing at the
                    // scores step, which was the class's binding node for one whole round of the
                    // derivation.
                    Op::MatMulQuant if attn_reduction => 17,
                    // The head-sliced recurrence opens its head's four registered triples.
                    Op::GatedDeltaNet if head_sliced_gdn => 4 * 17,
                    // Tile-local since Decision B: the tile's own weight rows, one byte per int8.
                    Op::MatMulQuant => tile.checked_mul(in_w).ok_or_else(over)?,
                    // (multiplier LE, shift, zero LE) per channel — per SLICE channel where the
                    // kernel is lane-sliced, because the executor reads its triples at the
                    // slice's own offset and nothing else is served.
                    Op::MulElem if lane_sliced || strided_combine => 9u64.checked_mul(tile.min(out_w)).ok_or_else(over)?,
                    // Four taps and one triple per SLICE channel.
                    Op::SsmConv if sliced_conv => 21u64.checked_mul(tile.min(out_w)).ok_or_else(over)?,
                    Op::MulElem => 9u64.checked_mul(out_w).ok_or_else(over)?,
                    // cos then sin, four bytes each, one pair per two lanes.
                    Op::RopeImrope => 8u64.checked_mul(out_w / 2).ok_or_else(over)?,
                    // One (multiplier, shift) for the whole node.
                    Op::Scale => 5,
                    // A gather opens the row it gathers; a norm's gain is one value per channel.
                    Op::EmbedLookup => out_w,
                    _ => 4u64.checked_mul(out_w).ok_or_else(over)?,
                };
                payload.checked_add(PALW_ARTIFACT_MAX_PATH_ELEMENTS * PATH_ELEMENT_BYTES).ok_or_else(over)?
            };

            // The challenged output tile: its own values, and the path that proves it.
            let leaf_bytes = |values: u64| values.checked_mul(4).and_then(|v| v.checked_add(step_path_bytes));
            let mut evidence = leaf_bytes(tile).ok_or_else(over)?;

            // And every leaf the court requires the challenger to open beside it. One row per
            // reference, except the KV arms, which are one row PER POSITION — that arm is the
            // reason this function had to grow past the weight bytes.
            //
            // **The un-anchored form, deliberately.** ADR-0030 §3's `PalwCheckpointKvOperandsV1`
            // replaces that history with one checkpoint opening and its state chunks, and it is
            // strictly cheaper — but it is not available at the coordinates that cost most. A
            // checkpoint covers a DECODE call, so a dispute at decode call `c` needs the one
            // covering `c − 1`: there is none for `c = 1`, and none at all for a prefill position.
            // The class's worst job is its whole context as prefill, so the worst step is exactly
            // where no anchor exists. Bounding the cheap form would have understated the shipped
            // floor by 2x, and `the_derived_close_cost_bounds_a_real_one` is what said so.
            // **How many POSITIONS this step opens its inputs at, from the node's own kernel.**
            //
            // `required_positions` expands a `GdnCore` step over every prior position — `0..=p`
            // inside the prefill call, and all of prefill plus every decode call at a decode step —
            // and each of those positions opens every one of the node's refs. Every other program
            // opens one position. The multiplier therefore belongs to the OP, not to the kind of
            // reference: a real GatedDeltaNet node wires its five inputs as ordinary intra-table
            // indices, so keying this off the checkpoint sentinel charged them once and priced the
            // recurrence at 6,592 bytes where the court requires 327,680 of Merkle path alone.
            // `the_gdn_arm_covers_what_the_court_actually_opens` is that measurement.
            // **The sliced kernels price what their court opens** — the same derivation
            // (`qwen36_gdn_slice_v1` / `qwen36_conv_slice_v1`) the leaf set and the executor
            // read, restated here as widths because the bound integrates over every tile while
            // those functions answer for one. A head-sliced recurrence still replays every
            // position, but each position opens one head's slices; the channel-sliced conv opens
            // four window positions of ONE ref's channel range (it was priced at a single
            // position before, which under-charged the window by 4x — found while the slicing
            // moved the width the other way).
            let positions = match node.op_kind {
                Op::GatedDeltaNet => n_ctx,
                Op::SsmConv if sliced_conv => 4,
                _ => 1,
            };
            for (ordinal, r) in node.input_refs.iter().enumerate() {
                let mut width = input_width_v1(*r, table, profile).ok_or_else(over)?;
                if head_sliced_gdn {
                    // Ref order is the kernel's: [unit_k, conv, unit_q, decay, beta] — one head's
                    // slice of each.
                    width = match ordinal {
                        0 | 2 => profile.gdn_head_k_dim as u64,
                        1 => profile.gdn_head_v_dim as u64,
                        _ => 1,
                    };
                }
                if sliced_conv {
                    if ordinal != 0 {
                        // One ref per position carries the challenged channels; the others open
                        // nothing. Priced as ref 0 because the three regions share a tile width
                        // and the bound wants the shape, not the identity.
                        continue;
                    }
                    width = tile;
                }
                if lane_sliced {
                    // Every ref opens the challenged tile's own lane range.
                    width = tile.min(width);
                }
                if strided_combine && ordinal == 0 {
                    // The outputs ref: `k` expert blocks, each contributing the tile's lanes.
                    // `k` is the blocks in the concatenation — outputs width over the node's row.
                    let k = width / out_w.max(1);
                    width = k.max(1).checked_mul(tile).ok_or_else(over)?;
                }
                let src_tile = source_tile_len_v1(table, node, *r);
                // **Priced as RANGE RUNS, because that is the carrier's form.** One position of
                // one ref is a contiguous run of tiles (the enumeration puts a node's tiles at
                // consecutive indices), and a run costs its lanes once plus ONE bounded sibling
                // set — `depth + ceil(log2 k) + 1` hashes, the range walk's own worst case —
                // plus a small per-leaf preimage header. Per-leaf full paths priced a 2,048-lane
                // row at tile 8 at 327 KiB of path for 8 KiB of lanes, and the whole class's
                // admissibility hung on exactly that difference.
                let (runs, run_tiles, run_lanes) = match *r {
                    // The cache: one run per position, each `kv_dim` wide.
                    PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => (n_ctx, kv_dim.div_ceil(src_tile), kv_dim),
                    // Everything else: `positions` runs of the (possibly sliced) row. The
                    // checkpoint sentinel stays priced pessimistically on purpose: the leaf set
                    // refuses it outright today, so a class reaching it is unadjudicable rather
                    // than cheap, and a low price here would read as approval.
                    _ => (positions, width.div_ceil(src_tile), width),
                };
                let run_path = step_path_bytes
                    .checked_add(64 * (u64::from(run_tiles.max(1).next_power_of_two().trailing_zeros()) + 1))
                    .ok_or_else(over)?;
                let per_run = run_lanes
                    .checked_mul(4)
                    .and_then(|lanes| lanes.checked_add(run_path))
                    .and_then(|v| v.checked_add(24u64.checked_mul(run_tiles)?))
                    .ok_or_else(over)?;
                evidence = evidence.checked_add(runs.checked_mul(per_run).ok_or_else(over)?).ok_or_else(over)?;
            }

            // **The generated-token pin** (ADR-0049 Decision E). A gather at a DECODE position
            // cannot be adjudicated without the ids the model produced, and the integer lane pins
            // them by carrying every logits row so the court can recompute
            // `base0_logits_trace_root_v1` — a flat hash, not a tree, so one row cannot be opened
            // on its own. That makes this arm `calls x vocabulary`, and a job may be almost all
            // decode: the bound is the whole context. Only a gather pays it.
            if node.op_kind == Op::EmbedLookup {
                let ids = n_ctx.checked_mul(4).ok_or_else(over)?;
                let pin = decode_pin_price_v1(profile, n_ctx).ok_or_else(over)?;
                evidence = evidence.checked_add(ids).and_then(|e| e.checked_add(pin)).ok_or_else(over)?;
            }
            // The prompt ids ride every refutation that addresses a gather, and a challenger may
            // carry them on any close: they are checked against `prompt_token_ids_hash` before one
            // is read, so they cost bytes rather than trust.
            evidence = evidence.checked_add(n_ctx.checked_mul(4).ok_or_else(over)?).ok_or_else(over)?;

            // Recomputation: what a full node spends to redo this one step, on peer-supplied input.
            //
            // The recurrence is its own case for the same reason it is in the byte arm, and it was
            // wrong the same way. `gdn_core_genesis_replay` walks EVERY prior position and, per
            // position and head, makes a small constant number of passes over a `k_dim x v_dim`
            // state: decay it, take `v_dim` dot products of length `k_dim` against it, then update
            // it. Charging `out_w` — the width of the row it finally emits — priced the fixture's
            // recurrence at 32 against 32,768 actually performed, and a Qwen3.6-shaped one at 2,048
            // against ~67 M, which is four times the ceiling it would have been admitted under.
            // Four passes is above the three the kernel makes, so this is a bound rather than an
            // estimate.
            let macs = match node.op_kind {
                Op::MatMulQuant => tile.checked_mul(in_w).ok_or_else(over)?,
                // The head-sliced form divides the recomputation by the head count: the court
                // replays ONE head's `k_dim x v_dim` state, which is what lets a 40-layer hybrid
                // have a context at all (the whole-graph form priced 536 M at the declared
                // context, 32x the terminal ceiling — measured, not estimated, on the first
                // profile to reach this arm).
                Op::GatedDeltaNet if head_sliced_gdn => positions
                    .checked_mul(profile.gdn_head_k_dim as u64)
                    .and_then(|v| v.checked_mul(profile.gdn_head_v_dim as u64))
                    .and_then(|v| v.checked_mul(4))
                    .ok_or_else(over)?,
                Op::GatedDeltaNet => positions
                    .checked_mul(profile.gdn_heads as u64)
                    .and_then(|v| v.checked_mul(profile.gdn_head_k_dim as u64))
                    .and_then(|v| v.checked_mul(profile.gdn_head_v_dim as u64))
                    .and_then(|v| v.checked_mul(4))
                    .ok_or_else(over)?,
                _ => out_w,
            };

            let operands = node.input_refs.len() as u32 + u32::from(!node.weight_name.is_empty());
            cost.max_close_bytes = cost.max_close_bytes.max(opening.checked_add(evidence).ok_or_else(over)?);
            cost.max_terminal_macs = cost.max_terminal_macs.max(macs);
            cost.max_operand_count = cost.max_operand_count.max(operands);
        }
    }
    Ok(cost)
}

/// Why a class may not join.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwClassAdmissionError {
    #[error("the object is not a class registration")]
    NotARegistration,
    /// A class IS its graph. Two registrations of the same profile are the same class and a
    /// different profile cannot borrow an id, so the id is checked against the profile rather than
    /// accepted as a label.
    #[error("the declared class id is not this profile's id")]
    ClassIdIsNotTheProfileId { declared: Hash64, derived: Hash64 },
    #[error("the profile is not well-formed: {0}")]
    Profile(String),
    /// ADR-0038 A4. Every kernel the graph can reach must be one this build can adjudicate,
    /// or every dispute over the uncatalogued one ends `Unadjudicable` — rejected but unslashed.
    #[error("the class reaches kernels this build cannot adjudicate")]
    CoverageGap,
    /// The class's LONGEST job — its whole context as prefill — must fit the ladder the ruleset
    /// already froze. Checking the typical job instead would admit a class an attacker picks the
    /// job length for.
    #[error("the class's worst-case trace is deeper than the court's ladder: {worst} > {ladder}")]
    DeeperThanTheLadder { worst: u64, ladder: u64 },
    /// ADR-0049 Decision C. A class whose cheapest prosecutable step still costs more than the
    /// ruleset allows is a class whose disputes nobody can raise — coverage-clean, ladder-deep
    /// enough, and unpolicable.
    #[error("the class's {what} of {got} exceeds the ruleset's ceiling of {ceiling}")]
    CourtCostExceedsCeiling { what: &'static str, got: u64, ceiling: u64 },
    /// A network that carries value registers only derived classes (the genesis loader's rule,
    /// restated: `MaxPerAttempt` bounds rather than checks, which makes PALW weight a collateral
    /// measure instead of a work measure).
    #[error("the class is not a derived-pwu class")]
    ClassIsNotDerived,
    #[error("the declared pwu_per_inference is not the canonical job's counted leaves: {declared} != {counted}")]
    PwuPerInferenceMismatch { declared: u64, counted: u64 },
    /// The canonical job is what the class is PAID per, so it may not be longer than the worst
    /// case the ladder was checked against.
    #[error("the canonical job is deeper than the class's own worst case: {canonical} > {worst}")]
    CanonicalDeeperThanWorstCase { canonical: u64, worst: u64 },
    /// ADR-0066 Decision 2. The free-prompt pricing (`quantum_cu`, the CU weights) is a FROZEN
    /// protocol parameter, and a class joins only if that pricing can see it: the largest job
    /// its declared context admits must certify at least one quantum. The alternative — retuning
    /// `quantum_cu` to fit each new model — moves the ruleset id, and "add a model" must never
    /// mean "re-mint the network". A model too small for the frozen quantum is refused here,
    /// before any fee is spent, not accommodated afterwards.
    #[error("the pricing cannot see this class: its largest admissible job certifies {max_cu} CU against a quantum of {quantum_cu} — no job of this class ever earns a draw, and the quantum is frozen (ADR-0066)")]
    PricingUnreachable { max_cu: u128, quantum_cu: u128 },
}

/// Every kernel a profile's graph can reach, read off the graph.
///
/// Public because the coverage claim and the catalog entry must be built from the same traversal —
/// two traversals that merely happen to agree is how A4 certifies a set nobody derived.
pub fn reachable_kernels_v1(profile: &PalwShapeProfileV3) -> BTreeSet<Hash64> {
    [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .map(|node| node.kernel_semantics_id)
        .collect()
}

/// The gate: `Ok(entry)` iff this class may join a chain running `bundle`.
///
/// `canonical` is the job the class is paid per. It is an argument rather than a derivation
/// because no function can choose it — it is the registrant's declaration of what one unit of this
/// class's work is — and a carrier must therefore commit it inside the signed registration, beside
/// `artifact_root`. Everything the catalog entry needs BESIDES that is computed here.
///
/// The returned entry is what a genesis catalog would have held for this class. A caller that
/// keeps it has the same object the genesis path produces, so the two lanes cannot drift into
/// describing a class differently.
pub fn verify_class_admission_v2(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    registration: &PalwConsensusObjectV2,
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, pwu_rule, .. } = registration else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };

    // **Every class that reaches this gate is court-adjudicable, and there is no arm that skips
    // it** (ADR-0053). The withdrawn Metal/GGUF family was admitted by a sibling arm that ran
    // `validate_geometry` instead of `validate_shape`, skipped the A4 coverage gate, skipped the
    // ladder-depth check and the court-cost ceilings, and wrote a catalog entry whose
    // `reachable_kernels` was EMPTY — a class the court could not adjudicate, admitted by a gate
    // whose whole job is refusing those. It is gone, and with it the question "which family is
    // this?" that every consumer downstream had to get right.

    // **Well-formedness first, before anything reads the shape.** Every check below — the id
    // derivation, the kernel walk, the leaf enumeration — is driven by `n_ctx` and `layer_count`,
    // so an unbounded shape decides how much work it costs to reject it. `validate_shape` is where
    // those bounds live; running it after the first consumer is running it too late.
    profile.validate_shape().map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;

    let derived_id = profile.shape_profile_id();
    if *class_id != derived_id {
        return Err(PalwClassAdmissionError::ClassIdIsNotTheProfileId { declared: *class_id, derived: derived_id });
    }

    // **The logits scheme must be one THIS BUILD adjudicates and prices.** `validate_shape` only
    // required the class to state one — well-formedness — but a scheme the close arms cannot
    // dispatch is a class whose every decode-token dispute ends unadjudicated, which is the same
    // fail-open A4 refuses for kernels. Enumerated here rather than in `validate_shape` because
    // "which schemes exist" is a property of the adjudicator, exactly like the kernel catalog.
    let known_schemes = [crate::palw_step_refute::flat_logits_scheme_id_v1(), crate::palw_step_refute::tiled_logits_scheme_id_v1()];
    if !known_schemes.contains(&profile.logits_scheme_id) {
        return Err(PalwClassAdmissionError::Profile(format!(
            "the class commits its logits under scheme {}, which this build cannot adjudicate",
            profile.logits_scheme_id
        )));
    }

    // **The canonical job must fit the context the class registered — in the ENUMERATION's own
    // form.** The step space's largest cached-position count is `prefill + exact_decode − 1`
    // (`step_leaf_count`: prefill runs kv_len = p+1 for p < prefill, decode call c runs
    // kv_len = prefill + c with decode_calls = exact_decode − 1), and `n_ctx` is the bound every
    // court cost is derived over. NOT `prefill + decode <= n_ctx`: that form is one stricter and
    // would refuse the floor's own declared worst case, (11, 2) at n_ctx 12 — footprint exactly
    // 12 under the enumeration, span 13 under the stricter reading. Two hand-written descriptions
    // of one computation is the defect class this file keeps recording.
    let footprint =
        (canonical.declared_prefill_tokens as u64).saturating_add(canonical.exact_decode_tokens.max(1) as u64).saturating_sub(1);
    if footprint > profile.n_ctx as u64 {
        return Err(PalwClassAdmissionError::Profile(format!(
            "the canonical job touches {footprint} cached positions and the class registers n_ctx {}",
            profile.n_ctx
        )));
    }

    // A4 first: a class whose disputes cannot be adjudicated must not reach any later check, so
    // that a coverage gap can never be reported as some more specific failure.
    let kernel_ids = reachable_kernels_v1(profile);
    verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: derived_id, kernel_ids: kernel_ids.clone() })
        .map_err(|_| PalwClassAdmissionError::CoverageGap)?;
    // The catalogued set is read from the adjudication table itself, which is what
    // `verify_catalog_coverage_v1` compares against — asserted here so a future refactor that
    // pointed the gate at a hand-kept list fails a test rather than certifying quietly.
    debug_assert!(kernel_ids.is_subset(&catalogued_kernel_ids_v1()), "coverage passed against a set that is not the table");
    // **And the STRONG gate, which had no non-test caller at all** (audit H-02).
    //
    // The id check above is set inclusion: it asks whether every kernel this profile names is in
    // the table. It cannot ask whether the adjudicator has an arm for the SHAPE each node wants —
    // an op needs operands of a particular arity and width, so a node can name a catalogued id
    // while asking for something nothing can produce. `verify_profile_coverage_v1` asks the
    // adjudicator itself, node by node and for both call classes.
    //
    // Near-inert while classes could only come from genesis; ADR-0049 Decision H made post-genesis
    // registration a live, permissionless path, and then a stranger could register a class whose
    // every dispute ends `Unadjudicable` — rejected but UNSLASHED, which is unfalsifiable work on a
    // chain where bonds are supposed to be at risk. The BASE-0 profile shipped 2026-08-20 did
    // exactly this at two nodes per layer and passed the id gate.
    crate::palw_catalog_coverage::verify_profile_coverage_v1(profile).map_err(|_| PalwClassAdmissionError::CoverageGap)?;

    let worst = worst_case_step_leaf_count_v1(profile).map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    let ladder = bundle.court.max_step_leaf_count();
    if worst > ladder {
        return Err(PalwClassAdmissionError::DeeperThanTheLadder { worst, ladder });
    }

    // **Decision C: what prosecuting this class costs, against what the ruleset allows.**
    //
    // Ordered after the ladder because the two answer different halves of one question — the ladder
    // bounds how many rounds a dispute takes, these bound what a round costs — and a class that
    // fails both should be told about the deeper problem first.
    let cost = derive_court_cost_v1(profile)?;
    for (what, got, ceiling) in [
        ("court close", cost.max_close_bytes, bundle.court.max_close_bytes()),
        ("terminal multiply-accumulates", cost.max_terminal_macs, bundle.court.max_terminal_macs()),
        ("operand count", cost.max_operand_count as u64, bundle.court.max_operand_count() as u64),
    ] {
        if got > ceiling {
            return Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling });
        }
    }

    // **ADR-0066 Decision 2: the pricing is frozen, so the class proves itself against it.**
    //
    // `quantum_cu` and the CU weights sit inside the ruleset id; sizing them to whatever class
    // is being onboarded — which is how the first calibration happened — makes every new model a
    // re-mint. This check inverts that: a class whose largest admissible job (its whole declared
    // context, priced at this ruleset's own table) cannot certify one quantum is refused at
    // registration, and the quantum never moves again. Placed after coverage and the court
    // ceilings so an unadjudicable class is still told about the deeper problem first.
    let max_cu = bundle.freeprompt.max_admissible_cu_for_context(profile.n_ctx);
    if crate::palw_freeprompt_v3::fp_quanta_v3(max_cu, bundle.freeprompt.quantum_cu(), bundle.freeprompt.max_quanta_per_receipt()) == 0 {
        return Err(PalwClassAdmissionError::PricingUnreachable { max_cu, quantum_cu: bundle.freeprompt.quantum_cu() });
    }

    let counted = step_leaf_count(profile, canonical).map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    if counted > worst {
        return Err(PalwClassAdmissionError::CanonicalDeeperThanWorstCase { canonical: counted, worst });
    }
    match pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(_) => return Err(PalwClassAdmissionError::ClassIsNotDerived),
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => {
            if *pwu_per_inference != counted {
                return Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared: *pwu_per_inference, counted });
            }
        }
    }

    Ok(PalwClassCatalogEntryV2 {
        class_id: derived_id,
        artifact_root: *artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: kernel_ids,
        // The cost this gate just checked, carried so the entry a registration folds is the same
        // shape a genesis catalog holds — one derivation, two doors.
        court_cost: cost,
    })
}

/// **A registration for a chain that is ALREADY RUNNING** (ADR-0049 Decision H).
///
/// A network is born with the classes its ruleset id commits to, and a class added later has no
/// such entry: nothing on chain can check its graph, its canonical job or its `pwu` rule, so the
/// object carries them and [`verify_class_admission_v2`] decides. Without a builder, a second
/// class means re-minting the network.
///
/// This replaces the withdrawn family's `family_m_post_genesis_registration_v1` (ADR-0053), and
/// the replacement is a generalization rather than a port: the old one could only ever build the
/// one black-box class its own crate pinned, because a black-box class's identity was a set of
/// runtime pins that crate held. A deterministic class's identity is its graph, which the caller
/// already has, so this builds a registration for ANY profile — the floor, a converted checkpoint,
/// whatever the node holds an artifact for.
///
/// Three things are NOT the caller's to choose, and are therefore not parameters:
///
/// * **the share** — a post-genesis entrant joins at the ruleset's minimum grantable share, and a
///   registrant naming its own permille would be donating itself a slice of every incumbent's
///   cadence. The validator refuses anything else;
/// * **the class id** — it is the profile's id. A class IS its graph;
/// * **`pwu_per_inference`** — the canonical job's counted step leaves. A declared value is checked
///   against the count, so the only thing a choice could do here is fail.
///
/// The signature is the caller's because the bond key is: this crate never sees key material.
/// Build once with an empty signature to learn the `class_id` to sign over, then again with it.
#[allow(clippy::too_many_arguments)]
pub fn palw_post_genesis_registration_v1(
    profile: PalwShapeProfileV3,
    canonical: PalwJobContextV2,
    artifact_root: Hash64,
    min_grantable_share_permille: u16,
    initial_target: u128,
    slash_value_per_pwu: u64,
    activation_daa: u64,
    registrant_bond: crate::palw_state_v2::PalwBondKeyV2,
    signature: Vec<u8>,
) -> Result<PalwConsensusObjectV2, PalwClassAdmissionError> {
    let class_id = profile.shape_profile_id();
    // Counted here, from the same canonical job the carriage carries, so the object the gate reads
    // and the number it recounts come from one value. A caller that computed the count separately
    // could hand the gate two statements about one class.
    let counted = crate::palw_step::step_leaf_count(&profile, &canonical)
        .map_err(|e| PalwClassAdmissionError::Profile(format!("the canonical job does not count against this profile: {e}")))?;
    Ok(PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target,
        share_permille: min_grantable_share_permille,
        activation_daa,
        admission: Some(Box::new(crate::palw_state_v2::PalwClassAdmissionCarriageV2 {
            profile,
            canonical,
            registrant_bond,
            signature,
        })),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use crate::palw_mode_v2::{PalwCourtParamsV2, tests::conforming_bundle};
    use crate::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_3B, qwen25_profile_v1};
    use crate::palw_step::PALW_STEP_MAX_LEAVES;
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, trace_scheme_id_v2};

    /// The measured Qwen2.5-1.5B graph, at the `tile_len` that actually admits its own declared
    /// context.
    ///
    /// `palw_qwen25_profile` ships `tile_len` 128 with `n_ctx` 4096, and at that pair the class's
    /// LONGEST job is 132,354,910 step leaves against a `PALW_STEP_MAX_LEAVES` of 4,194,304 — so
    /// the class as declared cannot be registered at all. Coverage says nothing about this: the
    /// graph reaches ten catalogued kernels and passes A4 either way. It is the leaf count that
    /// refuses, and `the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context` below is
    /// the tripwire for it.
    ///
    /// **Derived against a court with GENEROUS cost ceilings**, because what these fixtures are
    /// about is the ladder — whether a chain provisioned for the whole step space can admit a
    /// class the genesis catalog never named. Under the RC's own `max_close_bytes` no Qwen2.5
    /// geometry is admissible at any legal `tile_len`, and that is a separate fact with its own
    /// test (`no_qwen_geometry_has_a_close_a_transaction_could_carry`); binding it into this
    /// helper would make every ladder test fail for a cost reason and say nothing about ladders.
    fn qwen_admissible() -> PalwShapeProfileV3 {
        // DERIVED against the full ladder, not a hand-picked tile. It was `tile_len: 16_384` at
        // the declared 4096 context, which was admissible against a hand-written 27-node layer
        // table; the table is projected from `BASE0_LAYER_IR` now (ADR-0049 Decision F) and the
        // engine performs 38 steps, so that pair is 4,194,650 leaves against a 4,194,304 cap.
        // A literal here would be a third description of the class, rotting on its own schedule.
        let court = generous_court();
        let g = crate::palw_qwen25_profile::qwen25_admissible_geometry_v1(QWEN25_1_5B, &court)
            .expect("some pair is admissible under the over-provisioned ladder");
        qwen25_profile_v1(g).expect("the derived geometry is expressible")
    }

    /// The full ladder, with cost ceilings wide enough that only the ladder can refuse.
    /// **Deliberately over-provisioned, and deliberately not relayable.**
    ///
    /// The tests that use this fixture are about ADMISSION MECHANICS with a big class present —
    /// catalog roots, ladder sizing, share tables — not about any such class being deliverable. A
    /// network actually configured this way could assemble closes it could not relay, which is
    /// what `DEFAULT_MAX_CLOSE_BYTES` exists to refuse at registration; nothing here may be read
    /// as a statement that a class admitted under it would work.
    fn generous_court() -> PalwCourtParamsV2 {
        PalwCourtParamsV2::with_cost_ceilings(PALW_STEP_MAX_LEAVES, 20, 2, u64::MAX, u64::MAX, u32::MAX)
            .expect("a court that refuses only on depth is legal")
    }

    fn context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
        let mut ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: Hash64::default(),
            job_nullifier: Hash64::default(),
            assignment_id: Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = trace_scheme_id_v2();
        ctx
    }

    /// A bundle whose ladder is provisioned for the whole step space rather than for one class.
    ///
    /// This is the move the plan turns on. `max_step_leaf_count` is a bundle field and the bundle
    /// is `palw_ruleset_id_v2`, so a class deeper than the ladder cannot join a running chain —
    /// it needs a new ruleset. But the ladder is `ceil(log2(leaves)) + terminal` ROUNDS, so
    /// provisioning it at `PALW_STEP_MAX_LEAVES` covers every class that could ever be
    /// adjudicable, and costs six rounds over provisioning it for the floor alone (16 → 22).
    fn bundle_with_full_ladder() -> PalwConsensusParamsV2 {
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 20, 2).expect("the full ladder is a legal court");
        bundle
    }

    /// A network that has DECIDED, at genesis, to carry a Qwen-scale class and to pay for its
    /// courts. The ceilings are the measured cost of that class rather than a round number, which
    /// is the only honest way to choose a value that is inside the ruleset id forever.
    fn bundle_that_pays_for_qwen() -> PalwConsensusParamsV2 {
        // **Measured, not typed.** The ceilings are exactly what prosecuting this class costs, so
        // the fixture cannot drift away from the class it claims to pay for — and the number it
        // prints is the finding: a network that wanted Qwen2.5-1.5B at the widest context its
        // ladder admits would have to declare a close of gigabytes, which no transaction carries.
        // That is why the RC does not choose it.
        let cost = derive_court_cost_v1(&qwen_admissible()).expect("the derived geometry has a derivable cost");
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::with_cost_ceilings(
            PALW_STEP_MAX_LEAVES,
            20,
            2,
            cost.max_close_bytes,
            cost.max_terminal_macs,
            cost.max_operand_count,
        )
        .expect("a court sized for a 1.5B class is legal, and expensive on purpose");
        bundle
    }

    fn registration(class_id: Hash64, pwu_per_inference: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            artifact_root: Hash64::from_u64_word(0xA271FAC7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target: 1,
            share_permille: 1,
            activation_daa: 0,
            admission: None,
        }
    }

    /// **The scheme is class law and the gate enforces both halves of it.** A scheme this build
    /// cannot adjudicate is refused (the kernel-catalog rule, applied to the logits commitment);
    /// and the two shipped schemes are ADMITTED, so the gate separates rather than merely refuses.
    #[test]
    fn a_scheme_this_build_cannot_adjudicate_is_refused() {
        let mut profile = qwen_admissible();
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let bundle = bundle_that_pays_for_qwen();
        // Known schemes pass this gate (the flat default is the fixture's own).
        verify_class_admission_v2(&bundle, &profile, &canonical, &registration(profile.shape_profile_id(), counted))
            .expect("the flat scheme is adjudicable");
        // An invented scheme is refused BY the scheme gate, not downstream.
        profile.logits_scheme_id = Hash64::from_u64_word(0xDEAD_5C11E);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let err = verify_class_admission_v2(&bundle, &profile, &canonical, &registration(profile.shape_profile_id(), counted))
            .expect_err("a scheme nothing can adjudicate must not admit");
        assert!(format!("{err}").contains("cannot adjudicate"), "got {err}");
    }

    /// **The canonical job must fit the registered context — in the ENUMERATION's form.** The
    /// footprint is `prefill + exact_decode − 1` cached positions, so a job that touches exactly
    /// `n_ctx` positions is admissible and one position more is refused; the stricter
    /// `prefill + decode <= n_ctx` reading would refuse the boundary case, which is the floor's
    /// own declared worst shape.
    #[test]
    fn the_canonical_job_is_bounded_by_the_registered_context_in_the_enumerations_form() {
        // A context small enough that the probe is refused by THIS gate and not by the global
        // leaf cap — the fixture's own 4096 sits at the ladder's edge, where an oversized job
        // trips `TooManyLeaves` before the span rule can be the answer.
        let mut profile = qwen_admissible();
        profile.n_ctx = 64;
        let bundle = bundle_that_pays_for_qwen();
        // Exactly at the bound: footprint = 63 + 2 − 1 = 64 = n_ctx. Admissible.
        let at_bound = context(&profile, 63, 2);
        let counted = step_leaf_count(&profile, &at_bound).expect("the boundary job counts");
        verify_class_admission_v2(&bundle, &profile, &at_bound, &registration(profile.shape_profile_id(), counted))
            .expect("a job whose footprint is exactly n_ctx is the declared worst case, not a violation");
        // One past it: refused by the span gate, by name.
        let past = context(&profile, 64, 2);
        let counted =
            step_leaf_count(&profile, &past).expect("still enumerable — the violation is the class's bound, not the ladder's");
        let err = verify_class_admission_v2(&bundle, &profile, &past, &registration(profile.shape_profile_id(), counted))
            .expect_err("a canonical job past the registered context prices work the class never bounded");
        assert!(format!("{err}").contains("cached positions"), "got {err}");
    }

    /// **The plan's load-bearing claim, as a test.** A Qwen-scale BASE-0 class passes every gate a
    /// class must pass to join a running chain: its graph reaches only adjudicable kernels, its
    /// longest job fits a ladder provisioned at the step-space cap, and its declared pwu is the
    /// counted one.
    #[test]
    fn a_qwen_scale_class_can_join_a_chain_provisioned_for_the_step_space() {
        let profile = qwen_admissible();
        // The floor's shape of canonical job. (64, 64) needs 128 positions and the DERIVED context is
        // smaller than that — the class is admissible at a context the projected graph allows, not
        // at the one the hand-written table pretended to.
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("the canonical job counts");
        let entry = verify_class_admission_v2(
            &bundle_that_pays_for_qwen(),
            &profile,
            &canonical,
            &registration(profile.shape_profile_id(), counted),
        )
        .expect("the measured Qwen2.5-1.5B class is admissible on a network that pays for its courts");

        assert_eq!(entry.class_id, profile.shape_profile_id(), "a class is its graph");
        assert_eq!(entry.canonical_step_leaf_count, counted);
        assert!(entry.max_step_leaf_count <= PALW_STEP_MAX_LEAVES, "the worst case is inside the step space");
        assert_eq!(entry.reachable_kernels.len(), 10, "the Qwen graph reaches ten of the catalog's kernels");
    }

    /// **Registering a model does not make a new network.** The property every "add an LLM" flow
    /// depends on, asserted where it can be broken.
    ///
    /// `class_catalog_root` commits to the classes a network is BORN with. A post-genesis
    /// registration is a consensus object that lands in chain state — it never re-enters the
    /// bundle — so the ruleset id, and therefore the `consensus_params_id` two nodes compare at
    /// handshake, cannot move because someone registered a class.
    ///
    /// If that ever stopped being true, adding a model would fork the network silently: every node
    /// that had not yet seen the registration would compute a different fingerprint and refuse to
    /// peer, and the failure would look like a connectivity problem rather than a rule change.
    #[test]
    fn registering_a_class_does_not_move_the_ruleset_id() {
        // The Qwen-scale class needs a network provisioned for its step space; that is a fact
        // about the CLASS, not about this property, and using the floor's bundle would fail on the
        // ladder before it ever reached the question being asked here.
        let bundle = bundle_that_pays_for_qwen();
        let before = crate::palw_mode_v2::palw_ruleset_id_v2(&bundle);

        // A second class, admitted by the chain's own gate — the same call a node makes.
        let profile = qwen_admissible();
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let object = registration(profile.shape_profile_id(), counted);
        verify_class_admission_v2(&bundle, &profile, &canonical, &object).expect("admissible");

        // The bundle is untouched by admitting it: admission RETURNS a catalog entry, it does not
        // put one into the params. The entry lives in chain state from here on.
        assert_eq!(
            crate::palw_mode_v2::palw_ruleset_id_v2(&bundle),
            before,
            "admitting a class must not move the ruleset id — a handshake would refuse across it"
        );
        assert_eq!(
            bundle.class_catalog_root,
            bundle_that_pays_for_qwen().class_catalog_root,
            "and the genesis catalog root is still the genesis one"
        );
    }
    /// The floor is not disturbed by the second class existing — the property that makes "add it
    /// later" different from "run a different network".
    #[test]
    fn admitting_a_second_class_does_not_move_the_floors_id() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor geometry is expressible");
        let before = floor.shape_profile_id();
        let big = qwen_admissible();
        let canonical = context(&big, 8, 4);
        let counted = step_leaf_count(&big, &canonical).expect("counts");
        verify_class_admission_v2(&bundle_that_pays_for_qwen(), &big, &canonical, &registration(big.shape_profile_id(), counted))
            .expect("admissible");
        assert_eq!(base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("re-derives").shape_profile_id(), before);
        assert_ne!(before, big.shape_profile_id(), "two geometries are two classes");
    }

    /// **A class whose kernel ids are all catalogued but whose SHAPE nothing can adjudicate is
    /// refused at registration** (audit H-02).
    ///
    /// The id gate is set inclusion: it asks whether every kernel a profile names is in the table.
    /// It cannot ask whether the adjudicator has an arm for the shape each node wants — an op needs
    /// operands of a particular arity, so a node can name a catalogued id while asking for
    /// something nothing can produce. `verify_profile_coverage_v1` asks the adjudicator itself, and
    /// until now had NO non-test caller.
    ///
    /// Near-inert while classes came only from genesis; ADR-0049 Decision H made post-genesis
    /// registration a live, permissionless path, and then this is a stranger registering a class
    /// whose every dispute ends `Unadjudicable` — rejected but UNSLASHED, which is unfalsifiable
    /// work on a chain where bonds are supposed to be at risk.
    #[test]
    fn a_class_the_adjudicator_cannot_serve_is_refused_even_when_every_id_is_catalogued() {
        let bundle = conforming_bundle();
        let good = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor is expressible");
        let canonical = context(&good, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);

        // Strip a node's operands: same catalogued kernel id, a shape the adjudicator refuses. This
        // is the exact defect the BASE-0 profile shipped on 2026-08-20 at its attention matmuls.
        let mut lame = good.clone();
        let victim = lame.attn_nodes.iter_mut().find(|n| n.input_refs.len() >= 2).expect("a binary node exists");
        victim.input_refs.truncate(1);

        // Its kernel ids are unchanged, so the WEAK gate still passes…
        let ids = reachable_kernels_v1(&lame);
        assert!(
            verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: lame.shape_profile_id(), kernel_ids: ids })
                .is_ok(),
            "every id it names is still catalogued — which is why the id gate alone certified this"
        );

        // …and admission refuses it anyway, now that the strong gate is wired.
        let reg = registration(lame.shape_profile_id(), 1);
        let lame_ctx = context(&lame, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        assert!(
            matches!(verify_class_admission_v2(&bundle, &lame, &lame_ctx, &reg), Err(PalwClassAdmissionError::CoverageGap)),
            "a class the adjudicator cannot serve must not be registrable"
        );

        // And the honest floor still admits, so the gate is a bound rather than a blanket refusal.
        let ok = registration(good.shape_profile_id(), 7_900);
        assert!(
            !matches!(verify_class_admission_v2(&bundle, &good, &canonical, &ok), Err(PalwClassAdmissionError::CoverageGap)),
            "the floor's own graph is servable"
        );
    }

    /// A ladder provisioned for the floor alone refuses the bigger class — which is exactly why
    /// the provisioning decision has to be made at genesis and cannot be made later.
    #[test]
    fn a_floor_sized_ladder_refuses_the_bigger_class() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let floor_worst = worst_case_step_leaf_count_v1(&floor).expect("the floor's worst case is inside the cap");
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::new(floor_worst, 20, 2).expect("a floor-sized court is legal");

        let big = qwen_admissible();
        let canonical = context(&big, 8, 4);
        let counted = step_leaf_count(&big, &canonical).expect("counts");
        let err = verify_class_admission_v2(&bundle, &big, &canonical, &registration(big.shape_profile_id(), counted))
            .expect_err("the ladder cannot reach it");
        assert!(matches!(err, PalwClassAdmissionError::DeeperThanTheLadder { .. }), "got {err:?}");
    }

    /// **ADR-0066 Decision 2: the pricing is frozen, and a class the pricing cannot see is
    /// refused — the quantum is never invited to move.**
    ///
    /// The first calibration went the other way: `quantum_cu` was resized to fit the classes of
    /// the day, and since the value sits inside the ruleset id, "add a model" had become
    /// "re-mint the network". This test pins the inversion from both sides: a context of one
    /// position prices its largest job at 65 CU — under any ≥66-CU quantum, forever — and is
    /// refused at registration with the pricing named; while the smallest context the shipped
    /// classes actually use (the hybrids' close-budget ceiling of 8) clears the frozen 100-CU
    /// quantum with room, so no registered class is collateral damage.
    #[test]
    fn the_pricing_is_reachable_on_registered_classes_and_frozen_against_the_rest() {
        let bundle = bundle_with_full_ladder();

        // Every shipped context rung, priced at the frozen quantum: the smallest (8) yields
        // 1 + 8×64 = 513 CU — five quanta — and larger rungs only grow.
        for n_ctx in [8u32, 9, 12, 16] {
            let max_cu = bundle.freeprompt.max_admissible_cu_for_context(n_ctx);
            assert!(
                crate::palw_freeprompt_v3::fp_quanta_v3(max_cu, bundle.freeprompt.quantum_cu(), bundle.freeprompt.max_quanta_per_receipt()) >= 1,
                "an n_ctx-{n_ctx} class must be visible to the frozen pricing, got {max_cu} CU"
            );
        }

        // And a class the quantum genuinely cannot see is refused AT THE GATE, before any fee:
        // n_ctx 1 admits only (1 prompt, 1 decode) = 65 CU.
        let mut profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        profile.n_ctx = 1;
        let canonical = context(&profile, 1, 1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let err = verify_class_admission_v2(&bundle, &profile, &canonical, &registration(profile.shape_profile_id(), counted))
            .expect_err("a sub-quantum class is refused, not accommodated");
        assert!(matches!(err, PalwClassAdmissionError::PricingUnreachable { max_cu: 65, quantum_cu: 100 }), "got {err:?}");
    }

    /// `pwu_per_inference` is a declaration and pwu is a direct multiplier on fork-choice weight,
    /// so the count is what decides it. Overstating by one is refused.
    #[test]
    fn an_overstated_pwu_is_refused_against_the_count() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let err = verify_class_admission_v2(
            &bundle_with_full_ladder(),
            &profile,
            &canonical,
            &registration(profile.shape_profile_id(), counted + 1),
        )
        .expect_err("an overstated pwu is a lie the count catches");
        assert!(matches!(err, PalwClassAdmissionError::PwuPerInferenceMismatch { .. }), "got {err:?}");
    }

    /// A class may not borrow another graph's id, and a network carrying value may not register a
    /// `MaxPerAttempt` class — the genesis loader's two rules, restated for the later lane so the
    /// two entry points cannot drift.
    #[test]
    fn the_id_must_be_the_graphs_and_the_rule_must_be_derived() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let bundle = bundle_with_full_ladder();

        let borrowed = verify_class_admission_v2(&bundle, &profile, &canonical, &registration(Hash64::from_u64_word(7), counted))
            .expect_err("an id that is not the graph's is refused");
        assert!(matches!(borrowed, PalwClassAdmissionError::ClassIdIsNotTheProfileId { .. }), "got {borrowed:?}");

        let mut bounded = registration(profile.shape_profile_id(), counted);
        if let PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } = &mut bounded {
            *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1_000);
        }
        let err = verify_class_admission_v2(&bundle, &profile, &canonical, &bounded).expect_err("bounded is not derived");
        assert!(matches!(err, PalwClassAdmissionError::ClassIsNotDerived), "got {err:?}");
    }

    /// **The GatedDeltaNet arm, against the court's own input rule.**
    ///
    /// This arm had never been reached by anything. Both shipped deterministic profiles set
    /// `gdn_nodes: Vec::new()` — `base0_profile_v1` and `qwen25_profile_v1` — and BASE-0's own test
    /// asserts a plain decoder-only transformer has no GatedDeltaNet arm, so the only class that
    /// would reach it is one nobody has registered yet. A cost bound that has never priced a class
    /// of a given shape is a bound about that shape only in the sense that a comment is.
    ///
    /// What it got wrong, and in the fail-open direction: `required_positions` expands a `GdnCore`
    /// step over EVERY prior position — `0..=position` within the prefill call, and all of prefill
    /// plus every decode call at a decode step — and each of those positions opens every one of the
    /// node's input refs. The derivation charged the refs ONCE, because a real GDN node wires its
    /// inputs as ordinary intra-table indices and those fell to the single-position arm. The
    /// `n_ctx` multiplier sat on `PALW_STEP_INPUT_CHECKPOINT_STATE` instead — a sentinel
    /// `canonical_input_leaves` refuses outright ("registration-opaque today"), so it priced a
    /// shape the court cannot adjudicate while under-pricing the one it can, by a factor of the
    /// context.
    ///
    /// The bound is compared against a floor that owes nothing to this function's arithmetic: the
    /// PATH bytes alone of the leaves the court actually requires, ignoring every tile's values.
    /// A real close is strictly larger than that.
    #[test]
    fn the_gdn_arm_covers_what_the_court_actually_opens() {
        use crate::palw_step_refute::canonical_input_leaves_v1;

        let p = crate::palw_step_refute::tests::profile();
        // The longest job this class admits, which is what the bound is derived over.
        let mut ctx = crate::palw_step_refute::tests::context();
        ctx.declared_prefill_tokens = p.n_ctx - 1;
        ctx.exact_decode_tokens = 2;

        // The recurrence itself, found by op rather than by slot number.
        let slot = (0..p.global_node_count())
            .find(|s| {
                p.resolve_node_slot(*s).map(|(n, _)| n.op_kind == crate::palw_step::PalwStepOpKindV1::GatedDeltaNet).unwrap_or(false)
            })
            .expect("the fixture has a GatedDeltaNet node");
        // Its last decode call, where the expansion is widest.
        let coord = crate::palw_step::PalwStepCoordinateV1 { call_index: 1, node_slot: slot, position: 0, tile_index: 0 };

        let required = canonical_input_leaves_v1(&p, &ctx, &coord).expect("the court can enumerate this step's inputs");
        let leaves: u64 = required.iter().map(|row| row.len() as u64).sum();
        assert!(
            leaves > u64::from(p.n_ctx),
            "the fixture must actually exercise the expansion — {leaves} leaves at n_ctx {}",
            p.n_ctx
        );

        // The model-free floor, in the CARRIER's own form: since the range openings landed, a
        // canonical row rides one sibling set per contiguous run, so the floor is the runs'
        // sibling bytes — counted by the same `step_range_sibling_count_v1` the verifier's walk
        // consumes, over the run structure the required set itself implies. (The per-leaf floor
        // this test first asserted became an OVERSTATEMENT the day paths were shared; a real
        // close is strictly larger than the runs' paths, and no longer larger than the leaves'.)
        let worst = worst_case_step_leaf_count_v1(&p).expect("inside the cap");
        let mut floor_bytes = 0u64;
        for row in &required {
            let mut runs: Vec<(u64, u64)> = Vec::new();
            for (idx, _) in row {
                match runs.last_mut() {
                    Some((start, len)) if *start + *len == *idx => *len += 1,
                    _ => runs.push((*idx, 1)),
                }
            }
            for (first, k) in runs {
                floor_bytes += crate::palw_step_leg::step_range_sibling_count_v1(worst, first, k) * 64;
            }
        }

        let cost = derive_court_cost_v1(&p).expect("derivable");
        assert!(
            cost.max_close_bytes >= floor_bytes,
            "the derivation charges {} for this class; the court requires {leaves} leaves whose PATHS alone \
             are {floor_bytes} bytes — the bound is below the evidence it is supposed to bound",
            cost.max_close_bytes
        );

        // **And the same shape in the sibling arm.** The recurrence's recomputation walks every
        // prior position over a `k_dim x v_dim` state per head; charging the emitted row's width
        // priced this fixture's step at 32 multiply-accumulates. The floor here owes nothing to the
        // derivation's constant: one pass over the state, at every position the court opens.
        let state_pass = u64::from(p.n_ctx) * u64::from(p.gdn_heads) * u64::from(p.gdn_head_k_dim) * u64::from(p.gdn_head_v_dim);
        assert!(
            cost.max_terminal_macs >= state_pass,
            "the derivation charges {} multiply-accumulates; one pass over the recurrence's own state costs {state_pass}",
            cost.max_terminal_macs
        );
    }

    /// **The pin arm dispatches on the class's own commitment, and that is what the floor's
    /// vocabulary is priced by.**
    ///
    /// `logits_scheme_id` rides `PalwShapeProfileV3`, so it is inside the class id: a class cannot
    /// register under the cheap scheme and produce under the expensive one. Under the FLAT scheme
    /// the pin is `decode x vocab x 4` and it is what holds `PALW_RC_BASE0_GEOMETRY`'s vocabulary
    /// at 1,024 — 2,048 is already 124% of the ceiling at `n_ctx` 12. Under the TILED scheme the
    /// pin stops binding at any vocabulary at all, and the KV history becomes the sole constraint.
    ///
    /// That is the lever, and this is where a change to it becomes visible. The floor does not pull
    /// it: `misaka-palw-base0`'s producer builds `base0_logits_trace_root_v1` and nothing else, so a
    /// floor that COMMITTED the tiled scheme could not produce the commitment it committed to —
    /// the liveness class would mint and then make no blocks. Moving it is a producer change first
    /// and a geometry change second, in that order.
    #[test]
    fn the_pin_price_follows_the_class_and_the_floors_vocabulary_follows_the_pin() {
        use crate::palw_step_refute::{flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1};
        let ceiling = crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;

        // The floor as it ships: flat, and inside the ceiling with the margin it was sized for.
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        assert_eq!(floor.logits_scheme_id, flat_logits_scheme_id_v1(), "the floor commits the scheme its producer can build");
        // Re-frozen with the range-opening carrier — see `no_qwen_geometry_...` for the trail.
        assert_eq!(derive_court_cost_v1(&floor).expect("derivable").max_close_bytes, 52_704);

        let at = |vocab: u32, scheme| {
            let mut p =
                base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 { vocab_size: vocab, ..PALW_RC_BASE0_GEOMETRY })
                    .expect("expressible");
            p.logits_scheme_id = scheme;
            derive_court_cost_v1(&p).expect("derivable").max_close_bytes
        };

        // Flat: the vocabulary IS the constraint, and one doubling leaves the ceiling behind.
        assert!(at(2_048, flat_logits_scheme_id_v1()) > ceiling, "vocab 2,048 flat must not fit — it is why the floor is 1,024");
        assert_eq!(at(4_096, flat_logits_scheme_id_v1()), 200_160);

        // Tiled: the pin stops being the binding arm at any vocabulary, so the same close price
        // comes back however wide the head gets. Two tiles and three paths do not grow with it.
        // Re-frozen 61,040 → 39,408 with the range-opening carrier (the same move the floor's
        // own number made); the INVARIANT is the vocabulary-independence, and it is asserted as
        // such — three widths, one price.
        for vocab in [2_048u32, 4_096, 16_384] {
            assert_eq!(
                at(vocab, tiled_logits_scheme_id_v1()),
                39_408,
                "under the tiled scheme vocab {vocab} must cost what the non-pin evidence alone costs"
            );
        }
        assert!(at(16_384, tiled_logits_scheme_id_v1()) * 5 <= ceiling * 4, "and still inside the 80% rule");
    }

    /// **The GatedDeltaNet arm, against a class somebody actually built.**
    ///
    /// `the_gdn_arm_covers_what_the_court_actually_opens` checks the arm against the court's own
    /// input rule on the court's own fixture. This checks it against the real thing: Qwen3.6-35B-A3B
    /// is the class whose measured recurrence found the arm was fail-open in the first place, and a
    /// bound that priced the fixture correctly and the shipped class wrongly would be no better
    /// than the one it replaced.
    ///
    /// It is deliberately NOT an admissibility assertion. Whether this class fits the ceiling is a
    /// question about constants its authors are still deriving; what must hold today is that the
    /// derivation SEES its recurrence — that the number is large because the class is large, rather
    /// than small because an arm never fired.
    #[test]
    fn the_gdn_arm_prices_the_real_hybrid_class() {
        let g = crate::palw_qwen36_profile::QWEN36_35B_A3B;
        let p = crate::palw_qwen36_profile::qwen36_profile_v1(g).expect("the shipped hybrid geometry is expressible");
        assert!(!p.gdn_nodes.is_empty(), "the fixture must actually carry a recurrence");

        let cost = derive_court_cost_v1(&p).expect("derivable");
        // One pass over ONE HEAD's state, at every position the court can open. The whole-graph
        // floor (`x gdn_heads`) this test first asserted was superseded BY DESIGN: this class's
        // recurrence registers the head-sliced kernel, whose court replays the challenged head
        // alone — dividing the recomputation by the head count is what lets a 40-layer hybrid
        // have a context at all. The floor that still detects a dead arm is the per-head pass:
        // a derivation below it is once again pricing a row width instead of a replay.
        let head_pass = u64::from(p.n_ctx) * u64::from(p.gdn_head_k_dim) * u64::from(p.gdn_head_v_dim);
        assert!(
            cost.max_terminal_macs >= head_pass,
            "the derivation charges {} multiply-accumulates for the real hybrid class; one pass over one head's state is {head_pass}",
            cost.max_terminal_macs
        );
        // And the whole-graph form still prices a class that does NOT register the head-sliced
        // kernel — the fixture test covers that side; here the sliced kernel must be what changed
        // the price, not a vanished arm.
        assert!(
            cost.max_terminal_macs < u64::from(p.gdn_heads) * head_pass,
            "the head-sliced class must not be charged the whole-graph replay"
        );
        // And the byte side sees the context rather than one position of it.
        let one_position = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        assert!(
            cost.max_close_bytes > derive_court_cost_v1(&one_position).expect("derivable").max_close_bytes,
            "a 40-layer hybrid at n_ctx {} must not price below the four-layer floor",
            p.n_ctx
        );
    }

    /// **The other genesis decision, as arithmetic: the close ceiling is what a transaction can
    /// carry, and the floor is inside it with the margin it was chosen for.**
    ///
    /// A close is one `SUBNETWORK_ID_PALW_LIFECYCLE` transaction — there is no chunked-evidence
    /// path for a `PalwConsensusObjectV2` — so the largest close that can be RAISED is what a
    /// standard transaction holds. This test is the arithmetic that produced
    /// `DEFAULT_MAX_CLOSE_BYTES`, run rather than recited, and it fails on either side: if the
    /// ceiling grows past what a carrier can hold, or if the floor grows into the ceiling.
    #[test]
    fn the_close_ceiling_is_what_a_standard_transaction_can_carry() {
        use crate::palw_mode_v2::{DEFAULT_MAX_CLOSE_BYTES, PALW_STANDARD_TX_BYTES};

        // Transient mass is `size x 4` and the mempool refuses a transaction over the standard
        // limit on EITHER mass, so this — not the 480,000 — is the number in bytes.
        assert_eq!(PALW_STANDARD_TX_BYTES, 120_000);

        // What has to fit beside the close: a carrier the challenger builds. One ML-DSA-87 input
        // and a change output measures 7,457 bytes; the standard cap on a single signature script
        // is 16,384, so 18,000 covers the worst carrier a challenger could need. And the encoded
        // object runs about 1.2x the bytes this ceiling counts, because every opening carries its
        // own coordinate and length prefixes (measured 90,888 borsh against 77,568 counted).
        const CARRIER_ALLOWANCE: u64 = 18_000;
        const FRAMING_NUMERATOR: u64 = 12;
        const FRAMING_DENOMINATOR: u64 = 10;
        assert!(
            DEFAULT_MAX_CLOSE_BYTES * FRAMING_NUMERATOR / FRAMING_DENOMINATOR + CARRIER_ALLOWANCE <= PALW_STANDARD_TX_BYTES,
            "a close at the ceiling must still fit a standard transaction"
        );
        // And it is not needlessly small: doubling it would not.
        assert!(
            DEFAULT_MAX_CLOSE_BYTES * 2 * FRAMING_NUMERATOR / FRAMING_DENOMINATOR + CARRIER_ALLOWANCE > PALW_STANDARD_TX_BYTES,
            "the ceiling is within a factor of two of the carriage limit, so it forecloses nothing carriable"
        );

        // The floor, under it, with the margin `PALW_RC_BASE0_GEOMETRY` was chosen for. The
        // geometry comment carries the sweep; this is the pin.
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let cost = derive_court_cost_v1(&floor).expect("derivable");
        // Re-frozen with the range-opening carrier — see `no_qwen_geometry_...` for the trail.
        assert_eq!(cost.max_close_bytes, 52_704, "the floor's most expensive close");
        assert_eq!(cost.max_terminal_macs, 32_768, "and what a node recomputes to close it");
        assert_eq!(cost.max_operand_count, 2);
        assert!(
            cost.max_close_bytes * 5 <= PALW_RC_COURT_MAX_CLOSE_BYTES * 4,
            "the floor must stay under 80% of the ceiling — {} of {PALW_RC_COURT_MAX_CLOSE_BYTES}",
            cost.max_close_bytes
        );
        assert!(cost.max_terminal_macs <= PALW_RC_COURT_MAX_TERMINAL_MACS);
        assert!(u64::from(cost.max_operand_count) <= u64::from(PALW_RC_COURT_MAX_OPERAND_COUNT));
    }

    /// **A ceiling that counts only the weight bytes is not a bound on anything a block carries.**
    ///
    /// The regression this exists for: `derive_court_cost_v1` used to return the artifact opening
    /// alone, and on the shipped floor at a 64/64 job that was 32,768 against a real close of
    /// 750,716. Both arms it missed are asserted here, because either one going quiet would restore
    /// the old, unfalsifiable answer.
    #[test]
    fn the_close_cost_counts_the_arms_that_actually_grow() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let base = derive_court_cost_v1(&floor).expect("derivable").max_close_bytes;

        // The KV history: doubling the context must roughly double the close, because the anchored
        // history is the checkpoint's state chunks and those are `2 x layers x n_ctx x kv_dim`.
        let wider = base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 {
            n_ctx: PALW_RC_BASE0_GEOMETRY.n_ctx * 2,
            ..PALW_RC_BASE0_GEOMETRY
        })
        .expect("expressible");
        let wider_cost = derive_court_cost_v1(&wider).expect("derivable").max_close_bytes;
        assert!(wider_cost > base * 3 / 2, "twice the context must cost materially more: {base} -> {wider_cost}");

        // The generated-token pin: quadrupling the vocabulary must too, because
        // `base0_logits_trace_root_v1` is a flat hash and no single logits row can be opened.
        let fatter = base0_profile_v1(crate::palw_base0_profile::PalwBase0GeometryV1 {
            vocab_size: PALW_RC_BASE0_GEOMETRY.vocab_size * 4,
            ..PALW_RC_BASE0_GEOMETRY
        })
        .expect("expressible");
        let fatter_cost = derive_court_cost_v1(&fatter).expect("derivable").max_close_bytes;
        assert!(fatter_cost > base, "a bigger vocabulary must cost more: {base} -> {fatter_cost}");
    }

    /// **The genesis decision, as arithmetic.** Provisioning the ladder for the whole step space
    /// rather than for the floor alone costs eight rounds, and buys every admissible class — because
    /// `worst_case_step_leaf_count_v1` refuses anything deeper than the cap, so there is no class
    /// this ladder can fail to reach.
    #[test]
    fn provisioning_the_whole_step_space_costs_four_rounds() {
        let rounds = |leaves: u64| leaves.max(2).next_power_of_two().trailing_zeros();

        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        // The floor's LONGEST job — whole context as prefill — and not its declared 64/64 one,
        // which is 47,020. The ladder is checked against the longest job a class admits, so using
        // the declared one here understated the floor's own ladder by two rounds and the price of
        // provisioning by the same.
        let floor_worst = worst_case_step_leaf_count_v1(&floor).expect("the floor is inside the cap");
        // Re-measured five times, each because the declared graph grew to be the computation or
        // the class's own shape moved. The layer table's narrowings took 184,456 to 366,728; the
        // post table's narrowing added eight; declaring attention PER QUERY HEAD — the engine runs
        // score/amplify/softmax/narrow once per head and the table declared them once per layer —
        // took it to 465,040; ADR-0050's two residual gains took it to 481,424; and `n_ctx` 512 to
        // 12 (the cost ceiling's doing, see `PALW_RC_BASE0_GEOMETRY`) takes it to 8,352. The step
        // space is quadratic in the context, so this is the largest single move of the five.
        assert_eq!(floor_worst, 8_352, "the floor's longest job, measured");
        assert_eq!(rounds(floor_worst), 14);

        assert_eq!(PALW_RC_COURT_MAX_STEP_LEAF_COUNT, PALW_STEP_MAX_LEAVES);
        assert_eq!(rounds(PALW_RC_COURT_MAX_STEP_LEAF_COUNT), 22);
        assert_eq!(rounds(PALW_RC_COURT_MAX_STEP_LEAF_COUNT) - rounds(floor_worst), 8, "the price of the whole step space");

        // And it really is every class: the cap is what `worst_case_step_leaf_count_v1` enforces,
        // so a class the ladder cannot reach is a class that was already inadmissible.
        let big = qwen_admissible();
        assert!(worst_case_step_leaf_count_v1(&big).expect("inside the cap") <= PALW_RC_COURT_MAX_STEP_LEAF_COUNT);
    }

    /// **ADR-0049 Decision C, at the number a genesis actually has to look at: no Qwen2.5 geometry
    /// has a close a transaction could carry.**
    ///
    /// The claim used to be "it fits at a 125-token context". That was measured against the weight
    /// bytes alone, and a close is not its weight bytes: it carries the disputed step's KV history
    /// (anchored, the checkpoint's state chunks; un-anchored, one opening per position per ref) and,
    /// at a decode gather, every logits row of the job. Under
    /// [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`] — which is what a standard transaction can
    /// carry, not a round number — the answer is now the same at every legal `tile_len`: no.
    ///
    /// That is what makes the ceiling free to freeze. A larger one would not buy this class; it
    /// would only admit a class whose disputes nobody could raise, which is the exact shape
    /// ADR-0049 exists to refuse. What buys Qwen is an openable logits commitment and a per-layer
    /// slice of the checkpoint — code, not a bigger number in a genesis.
    ///
    /// The earlier drafts of this test are worth remembering: one asserted "refused at EVERY tile
    /// length" and failed on tile 64, and its successor asserted a 125-token window that only
    /// existed because the metric was short. Both times the assertion was one measurement ahead of
    /// the claim.
    #[test]
    fn no_qwen_geometry_has_a_close_a_transaction_could_carry() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let floor_cost = derive_court_cost_v1(&floor).expect("derivable");
        // Re-frozen 61,040 → 52,704 when the carrier learned range openings: a canonical row now
        // rides one sibling set per contiguous run instead of one full path per leaf, and the
        // floor's binding row was four tiles wide. The ceiling did not move; the close got
        // cheaper, which is the direction a format change is allowed to move a frozen number.
        assert_eq!(floor_cost.max_close_bytes, 52_704, "the floor's most expensive close, measured");
        assert!(
            floor_cost.max_close_bytes * 5 <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES * 4,
            "the floor stays under 80% of the ceiling — the margin `PALW_RC_BASE0_GEOMETRY` was chosen for"
        );

        // Each tile is priced at the LONGEST context it can adjudicate, because that is the class a
        // network would actually register — a tile bought for its context and then not used for it
        // is a ceiling question nobody is asking.
        let widest_adjudicable_ctx = |tile: u32| -> u32 {
            let fits = |n_ctx: u32| {
                qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B })
                    .ok()
                    .and_then(|p| worst_case_step_leaf_count_v1(&p).ok())
                    .is_some()
            };
            let (mut lo, mut hi) = (2u32, 1u32 << 16);
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if fits(mid) { lo = mid } else { hi = mid }
            }
            lo
        };

        let default_bundle = bundle_with_full_ladder();
        let mut fits: Vec<u32> = Vec::new();
        for tile in [16u32, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16_384] {
            let n_ctx = widest_adjudicable_ctx(tile);
            let p = qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B }).expect("expressible");
            let cost = derive_court_cost_v1(&p).expect("derivable");
            let canonical = context(&p, 8, 4);
            let Ok(counted) = step_leaf_count(&p, &canonical) else { continue };
            match verify_class_admission_v2(&default_bundle, &p, &canonical, &registration(p.shape_profile_id(), counted)) {
                Ok(_) => {
                    assert!(cost.max_close_bytes <= crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES);
                    fits.push(tile);
                }
                Err(PalwClassAdmissionError::CourtCostExceedsCeiling { .. }) => {
                    assert!(cost.max_close_bytes > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES);
                }
                Err(e) => panic!("tile {tile}: unexpected {e:?}"),
            }
        }
        assert!(fits.is_empty(), "no tile length gives Qwen2.5-1.5B a close a transaction could carry, got {fits:?}");
        assert!(
            derive_court_cost_v1(&qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx: 2, tile_len: 64, ..QWEN25_1_5B }).unwrap())
                .unwrap()
                .max_close_bytes
                > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES,
            "even at the smallest expressible context the cheapest close is over the carrier's budget"
        );

        // **The declared 4,096-token context is not adjudicable at ANY legal tile length**, and
        // that is a change: against the hand-written 27-node layer table `tile_len` 16,384 reached
        // it and cost 24 MiB an opening. The table is projected from `BASE0_LAYER_IR` now and the
        // engine performs 38 steps per layer, so the whole legal tile range — up to
        // `PALW_STEP_MAX_TILE_LEN` — is short. Swept rather than asserted at one tile, because
        // "16,384 works" was exactly the shape of claim that went stale silently.
        let mut admits_full_ctx = Vec::new();
        let mut tile = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
        while tile <= crate::palw_step::PALW_STEP_MAX_TILE_LEN {
            if let Ok(p) = qwen25_profile_v1(PalwQwen25GeometryV1 { tile_len: tile, ..QWEN25_1_5B })
                && worst_case_step_leaf_count_v1(&p).is_ok()
            {
                admits_full_ctx.push(tile);
            }
            tile = tile.saturating_mul(2);
        }
        assert!(
            admits_full_ctx.is_empty(),
            "a tile length reached the declared 4096 context ({admits_full_ctx:?}) — the sizing table needs updating with it"
        );
    }

    /// **The shipped Qwen geometries do not admit their own declared context**, and coverage
    /// cannot see it.
    ///
    /// `worst_case_step_leaf_count_v1` is the whole context as prefill — the longest job a class
    /// admits — and both shipped constants are far past `PALW_STEP_MAX_LEAVES` at `tile_len` 128:
    /// 132.4 M leaves for 1.5B and 219.7 M for 3B. `tile_len` is the only knob that moves it, and
    /// measured, 1.5B needs 16,384 to reach `n_ctx` 4096 while 3B needs 65,536 — which is
    /// `PALW_STEP_MAX_TILE_LEN` exactly, so the 3B class at 4096 sits on the type's own ceiling
    /// with no headroom.
    ///
    /// This test fails the moment either constant changes, which is the point: it is a tripwire on
    /// a pair of numbers that pass every other gate.
    #[test]
    fn the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context() {
        for shipped in [QWEN25_1_5B, QWEN25_3B] {
            let as_shipped = qwen25_profile_v1(shipped).expect("expressible");
            assert!(
                worst_case_step_leaf_count_v1(&as_shipped).is_err(),
                "a shipped Qwen geometry became admissible — update this tripwire and the sizing table with it"
            );
        }
        // The DERIVED geometry is admissible — that is what makes the class registrable at all.
        assert!(worst_case_step_leaf_count_v1(&qwen_admissible()).is_ok(), "the derived 1.5B geometry is inside the step space");
        // And the declared context is not reachable at the maximum legal tile either, for either
        // member. Against the hand-written table 1.5B reached 4096 at 16,384 and 3B at 65,536;
        // the projected graph is bigger and neither does.
        for shipped in [QWEN25_1_5B, QWEN25_3B] {
            let widest = qwen25_profile_v1(PalwQwen25GeometryV1 { tile_len: crate::palw_step::PALW_STEP_MAX_TILE_LEN, ..shipped })
                .expect("expressible at the maximum tile");
            assert!(
                worst_case_step_leaf_count_v1(&widest).is_err(),
                "the declared context became reachable at the maximum tile — update the sizing table"
            );
        }
    }

    /// **The one gate, on the one family.** Every registration goes through the step-space gate:
    /// counted leaves, a full coverage walk, the ladder and the court's cost ceilings. There is no
    /// second arm to fall into (ADR-0053).
    #[test]
    fn every_registration_goes_through_the_step_space_gate() {
        let bundle = bundle_with_full_ladder();
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("the floor counts");
        let reg = PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(0xA1),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let entry = verify_class_admission_v2(&bundle, &profile, &canonical, &reg).expect("the floor is admissible");
        assert_eq!(entry.canonical_step_leaf_count, counted, "still counted in STEP LEAVES, not decode tokens");
        assert!(!entry.reachable_kernels.is_empty(), "and still coverage-checked");
    }
}
