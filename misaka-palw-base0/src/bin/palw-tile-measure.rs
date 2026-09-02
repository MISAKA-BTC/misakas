//! **Is a disputed attention leaf's close FLAT in the position under the graph-v4 tiled map?**
//! — U-00's measurement half, against `derive_court_cost_shaped_v1` and its per-node breakdown.
//!
//! `PALW_TILED_KV_STATE_CHUNK_MAP_NAME_V3` says of itself that its tile "is a CONSTANT and not a
//! function of `n_ctx`, which is the property the ladder needs: … the close a v4 attention node
//! derives is flat in the context (W1) instead of linear in it." That is a claim about ONE term of
//! the close — the checkpoint-chunk opening — and ADR-0080, ADR-0081 and the competing
//! chunk-the-reductions design all rest on the whole close behaving that way. Nothing measured it.
//!
//! This binary measures it, and prints a TABLE rather than a verdict: every number here is
//! `derive_court_cost_shaped_v1`'s own arithmetic, and every row names the node that produced it
//! through `derive_court_cost_rows_v1`, so a reader can see WHICH term moved rather than being told
//! that one did.
//!
//! # What it changes: nothing
//!
//! No consensus rule, no shipped fingerprint, no registration. `palw_class_ladder_rules_v1` is
//! itself behind `Params::palw_context_ladder`, which is `None` on every preset, and this binary
//! only reads. Where it prices the v3 map it does so by constructing the cost shape by hand —
//! because, as the report below states in its own section, the shipped rule does NOT read the v3
//! map. That is a finding, not a fixture.
//!
//! Run: `cargo run -p misaka-palw-base0 --bin palw-tile-measure`

use kaspa_consensus_core::palw_class_admission_v2::{
    PalwCourtCostRowV1, PalwCourtCostShapeV1, derive_court_cost_rows_v1, derive_court_cost_shaped_v1, derive_court_cost_v1,
};
use kaspa_consensus_core::palw_context_ladder::{
    PALW_CONTEXT_LADDER_MAX_STEP_LEAVES, palw_a16_context_row_profile_v1, palw_checkpoint_interval_v1,
    palw_gdn_checkpoint_opening_bytes_for_map_v1, palw_kv_checkpoint_opening_bytes_v1, palw_qwen36_context_row_profile_v1,
};
use kaspa_consensus_core::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES;
use kaspa_consensus_core::palw_state_chunk_map::{
    PALW_ATTN_HISTORY_TILE_V4, hybrid_state_chunk_map_id_v2, hybrid_state_chunk_map_id_v3, integer_kv_state_chunk_map_id_v2,
    tiled_kv_chunk_bytes_v3, tiled_kv_state_chunk_map_id_v3,
};
use kaspa_consensus_core::palw_step::{
    PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_MAX_LEAVES, PalwShapeProfileV3, worst_case_step_leaf_count_capped_v1,
};

/// The ladder every anchored reading below is priced against — ADR-0077 Decision 12's `2^32`, the
/// one `palw_class_ladder_rules_v1` uses.
const LADDER: u64 = PALW_CONTEXT_LADDER_MAX_STEP_LEAVES;

/// One step-leaf Merkle path at [`LADDER`]. `palw_context_ladder::step_path_bytes_v1` is private;
/// this is its arithmetic, and `the_tile_measure_path_constant_is_the_ladders` in
/// `palw_context_ladder` pins the two equal so this copy cannot drift.
const fn step_path_bytes(ladder: u64) -> u64 {
    64 * (if ladder < 2 { 1 } else { ladder.next_power_of_two().trailing_zeros() as u64 })
}

/// Which enumeration of the attention cache a reading prices.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Map {
    /// `integer_kv_state_chunk_map_id_v2` — `chunk=floor(1048576/row)`, so at any context the leg
    /// admits, one chunk is the WHOLE history.
    IntegerV2,
    /// `tiled_kv_state_chunk_map_id_v3` — `chunk=min(positions,16)`, so one chunk is a tile.
    TiledV3,
}

impl Map {
    fn name(self) -> &'static str {
        match self {
            Map::IntegerV2 => "integer-kv v2",
            Map::TiledV3 => "tiled v3",
        }
    }

    /// What ONE checkpoint-chunk opening of the KV cache costs under this map, plus the path that
    /// proves it — the quantity `PalwCourtCostShapeV1::kv_checkpoint_bytes` holds.
    fn kv_checkpoint_bytes(self, profile: &PalwShapeProfileV3) -> u64 {
        match self {
            Map::IntegerV2 => palw_kv_checkpoint_opening_bytes_v1(profile, LADDER).expect("the v2 opening derives"),
            // No `palw_kv_checkpoint_opening_bytes_for_map_v1` exists to ask — see the report's
            // §0. `tiled_kv_chunk_bytes_v3` is the map module's own answer for one chunk; the path
            // term is the same one the v1 function adds.
            Map::TiledV3 => tiled_kv_chunk_bytes_v3(profile).expect("the v3 chunk derives") + step_path_bytes(LADDER),
        }
    }

    /// The dense (A16) profile at a context, declaring this map. `state_chunk_map_id` is inside
    /// `shape_profile_id`, so these are two DIFFERENT classes and not one class read two ways.
    fn dense(self, n_ctx: u32) -> PalwShapeProfileV3 {
        let mut profile = palw_a16_context_row_profile_v1(n_ctx).expect("the dense row projects");
        profile.state_chunk_map_id = match self {
            Map::IntegerV2 => integer_kv_state_chunk_map_id_v2(),
            Map::TiledV3 => tiled_kv_state_chunk_map_id_v3(),
        };
        profile
    }

    /// The hybrid (Qwen3.6) profile at a context, with its attention half on this map. The
    /// recurrence half stays at gdn v2 in both compositions, which is what makes the difference
    /// between the two readings the ATTENTION map and nothing else.
    fn hybrid(self, n_ctx: u32) -> PalwShapeProfileV3 {
        let mut profile = palw_qwen36_context_row_profile_v1(n_ctx).expect("the hybrid row projects");
        profile.state_chunk_map_id = match self {
            Map::IntegerV2 => hybrid_state_chunk_map_id_v2(),
            Map::TiledV3 => hybrid_state_chunk_map_id_v3(),
        };
        profile
    }
}

/// **What one anchored RECURRENCE opening costs on the composition this profile declares** — and
/// it is asked of the composition's `gdn=` HALF, not of the composition.
///
/// `gdn_state_terms_for_map_v1` dispatches on the whole id and knows `hybrid_state_chunk_map_id_v1`
/// and `…_v2` only, so it answers `None` for the graph-v4 composition — whose `gdn=` half is
/// `PALW_GDN_STATE_CHUNK_MAP_NAME_V2` verbatim, i.e. a recurrence opening that IS priceable and is
/// priced at 71,680 + path. The shipped rule turns that `None` into `unwrap_or(0)`, which is the
/// gap §0b measures. Here the half is asked directly so the attention measurement is not standing
/// on a recurrence charge of zero.
fn gdn_checkpoint_bytes(profile: &PalwShapeProfileV3, map: Map) -> u64 {
    let mut as_declared = profile.clone();
    if map == Map::TiledV3 && profile.state_chunk_map_id == hybrid_state_chunk_map_id_v3() {
        as_declared.state_chunk_map_id = hybrid_state_chunk_map_id_v2();
    }
    palw_gdn_checkpoint_opening_bytes_for_map_v1(&as_declared, LADDER).unwrap_or(0)
}

/// Decision 11's court for this class at this map: the interval as history, the `2^32` ladder,
/// both checkpoint openings priced from the maps the class registered.
fn anchored_shape(profile: &PalwShapeProfileV3, map: Map) -> PalwCourtCostShapeV1 {
    let interval = palw_checkpoint_interval_v1(profile.n_ctx);
    let mut shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(profile, interval, LADDER, 0);
    shape.kv_checkpoint_bytes = map.kv_checkpoint_bytes(profile);
    shape.gdn_checkpoint_bytes = gdn_checkpoint_bytes(profile, map);
    shape
}

/// Does this node read the KV HISTORY? The same predicate `derive_court_cost_walk_v1` uses for its
/// `reads_history` flag, restricted to the cache arms — the recurrence is the other kind.
fn is_attention_history_node(profile: &PalwShapeProfileV3, row: &PalwCourtCostRowV1) -> bool {
    let table = match row.table {
        "pre" => &profile.pre_nodes,
        "gdn" => &profile.gdn_nodes,
        "attn" => &profile.attn_nodes,
        _ => &profile.post_nodes,
    };
    table.get(row.index).is_some_and(|n| n.input_refs.iter().any(|r| *r == PALW_STEP_INPUT_KV_K || *r == PALW_STEP_INPUT_KV_V))
}

/// The most expensive node that reads the KV cache — "a disputed attention leaf", named.
fn worst_kv_row(profile: &PalwShapeProfileV3, rows: &[PalwCourtCostRowV1]) -> Option<PalwCourtCostRowV1> {
    rows.iter().find(|r| is_attention_history_node(profile, r)).cloned()
}

fn label(row: &PalwCourtCostRowV1) -> String {
    let weight = if row.weight_name.is_empty() { "-".to_string() } else { row.weight_name.clone() };
    format!("{}[{}] {:?} `{weight}`", row.table, row.index, row.op_kind)
}

/// Section 1 and 2: the disputed attention leaf's close at three contexts, both maps.
fn attention_leaf_table(kind: &str, build: impl Fn(Map, u32) -> PalwShapeProfileV3, contexts: &[u32]) {
    println!("### {kind}: a disputed attention leaf's close, anchored at the 2^32 ladder");
    println!();
    println!("| n_ctx | interval | map | kv_checkpoint_bytes | attention-KV node | its close | max_close_bytes | binding node |");
    println!("|---|---|---|---|---|---|---|---|");
    let mut series: Vec<(Map, Vec<(u32, u64, u64)>)> = vec![(Map::IntegerV2, vec![]), (Map::TiledV3, vec![])];
    for &n_ctx in contexts {
        for (map, points) in series.iter_mut() {
            let profile = build(*map, n_ctx);
            let shape = anchored_shape(&profile, *map);
            match derive_court_cost_rows_v1(&profile, shape) {
                Ok(rows) => {
                    let total = derive_court_cost_shaped_v1(&profile, shape).expect("the total derives").max_close_bytes;
                    let kv = worst_kv_row(&profile, &rows);
                    let (kv_label, kv_close) =
                        kv.map(|r| (label(&r), r.close_bytes)).unwrap_or_else(|| ("(none reads KV)".to_string(), 0));
                    let binding = rows.first().map(label).unwrap_or_else(|| "-".to_string());
                    println!(
                        "| {n_ctx} | {} | {} | {} | {kv_label} | {kv_close} | {total} | {binding} |",
                        palw_checkpoint_interval_v1(n_ctx),
                        map.name(),
                        shape.kv_checkpoint_bytes,
                    );
                    points.push((n_ctx, kv_close, total));
                }
                // `TooManyLeaves`'s `got` is the RUNNING TOTAL at the position where the walk
                // passed the cap, not the class's leaf count: the enumeration returns early on
                // purpose ("a cap tested at the end is an answer bound that has already paid the
                // whole cost of the answer"). §7 is where the true ceiling is measured.
                Err(e) => println!(
                    "| {n_ctx} | {} | {} | {} | — | — | **NOT PRICEABLE** — {e} (see §7: `got` is a partial sum) | — |",
                    palw_checkpoint_interval_v1(n_ctx),
                    map.name(),
                    shape.kv_checkpoint_bytes,
                ),
            }
        }
    }
    println!();
    println!("Growth of the attention-KV node's close, and of the whole class's:");
    println!();
    println!("| map | points (n_ctx → attn-KV close) | ratio | shape |");
    println!("|---|---|---|---|");
    for (map, points) in &series {
        if points.len() < 2 {
            println!("| {} | (fewer than two priceable points) | — | — |", map.name());
            continue;
        }
        let spelled = points.iter().map(|(n, c, _)| format!("{n} → {c}")).collect::<Vec<_>>().join(", ");
        let (n0, c0, _) = points[0];
        let (n1, c1, _) = points[points.len() - 1];
        let ctx_ratio = n1 as f64 / n0 as f64;
        let cost_ratio = if c0 == 0 { f64::NAN } else { c1 as f64 / c0 as f64 };
        // Named from the two ratios rather than from a guess: flat is 1x under a growing context,
        // linear tracks it, and anything strictly between the two is sub-linear.
        let shape = if (cost_ratio - 1.0).abs() < 0.01 {
            "FLAT"
        } else if cost_ratio > ctx_ratio * 0.9 {
            "LINEAR in n_ctx"
        } else if cost_ratio > 1.5 {
            "sub-linear but GROWING"
        } else {
            "near-flat"
        };
        println!("| {} | {spelled} | {cost_ratio:.2}x over a {ctx_ratio:.0}x context | **{shape}** |", map.name());
    }
    println!();
}

/// Section 3: the widest context the 80 KiB carrier admits, per map and per court.
fn widest_row(kind: &str, build: impl Fn(Map, u32) -> PalwShapeProfileV3) {
    println!("### {kind}: the widest n_ctx the carrier ({DEFAULT_MAX_CLOSE_BYTES} bytes) admits");
    println!();
    println!("| court | map | widest n_ctx | its max_close_bytes | first refused n_ctx | why it was refused |");
    println!("|---|---|---|---|---|---|");
    for (court, armed) in [("unfenced (genesis-anchored, derive_court_cost_v1)", false), ("armed (Decision 11 + 12)", true)] {
        for map in [Map::IntegerV2, Map::TiledV3] {
            let mut widest = 0u32;
            let mut widest_bytes = 0u64;
            let mut refused_at = 0u32;
            let mut refused_why = String::new();
            // Linear from 1: the admissible band is small enough that a sweep is exact, and an
            // exact answer is the point — a bisection over a predicate that is not monotone in
            // `n_ctx` (the interval steps at every multiple of 32) would report a boundary that
            // is not one.
            for n_ctx in 1..=4_096u32 {
                let profile = build(map, n_ctx);
                let derived = if armed {
                    let shape = anchored_shape(&profile, map);
                    derive_court_cost_shaped_v1(&profile, shape)
                } else {
                    derive_court_cost_v1(&profile)
                };
                match derived {
                    Ok(cost) if cost.max_close_bytes <= DEFAULT_MAX_CLOSE_BYTES => {
                        widest = n_ctx;
                        widest_bytes = cost.max_close_bytes;
                    }
                    Ok(cost) => {
                        refused_at = n_ctx;
                        refused_why = format!("close {} > carrier", cost.max_close_bytes);
                        break;
                    }
                    Err(e) => {
                        refused_at = n_ctx;
                        refused_why = format!("{e}");
                        break;
                    }
                }
            }
            println!("| {court} | {} | **{widest}** | {widest_bytes} | {refused_at} | {refused_why} |", map.name());
        }
    }
    println!();
}

/// Section 4: what is still linear after v3, node by node — the per-node delta between two
/// contexts, so a reader inherits the term rather than the total.
fn residue(kind: &str, build: impl Fn(Map, u32) -> PalwShapeProfileV3, narrow: u32, wide: u32) {
    println!("### {kind}: what still grows between n_ctx {narrow} and {wide}, under the v3 map");
    println!();
    let narrow_profile = build(Map::TiledV3, narrow);
    let wide_profile = build(Map::TiledV3, wide);
    let a = derive_court_cost_rows_v1(&narrow_profile, anchored_shape(&narrow_profile, Map::TiledV3));
    let b = derive_court_cost_rows_v1(&wide_profile, anchored_shape(&wide_profile, Map::TiledV3));
    let (Ok(a), Ok(b)) = (a, b) else {
        println!("_one of the two contexts is not priceable; no residue to report_");
        println!();
        return;
    };
    let key = |r: &PalwCourtCostRowV1| (r.table, r.index);
    // The two halves separately, because they answer different questions: `opening_bytes` is the
    // ARTIFACT side (the node's own weight rows, whose width is a graph property) and
    // `evidence_bytes` is the STEP side (the opened input runs, the ids, the checkpoint charge —
    // the side any anchoring or map reaches). A slope that lives in the opening is not one a state
    // chunk map could ever have moved.
    let mut deltas: Vec<(String, u64, u64, i128, i128, i128)> = Vec::new();
    for row in &b {
        if let Some(before) = a.iter().find(|r| key(r) == key(row)) {
            deltas.push((
                label(row),
                before.close_bytes,
                row.close_bytes,
                row.close_bytes as i128 - before.close_bytes as i128,
                row.opening_bytes as i128 - before.opening_bytes as i128,
                row.evidence_bytes as i128 - before.evidence_bytes as i128,
            ));
        }
    }
    deltas.sort_by(|x, y| y.3.cmp(&x.3));
    println!("| node | close at {narrow} | close at {wide} | delta | bytes/position | of which opening | of which evidence |");
    println!("|---|---|---|---|---|---|---|");
    let span = (wide - narrow) as f64;
    for (name, before, after, delta, opening, evidence) in deltas.iter().take(10) {
        println!(
            "| {name} | {before} | {after} | {delta:+} | {:.2} | {:.2} | {:.2} |",
            *delta as f64 / span,
            *opening as f64 / span,
            *evidence as f64 / span
        );
    }
    let flat = deltas.iter().filter(|d| d.3 == 0).count();
    println!();
    println!("{flat} of {} nodes are FLAT between the two contexts.", deltas.len());
    println!();
    // The prompt-id prediction, checked rather than asserted: `count_ids` off is the same walk with
    // the two `n_ctx`-shaped id terms removed, so the difference between the two readings IS the id
    // term. u04's justification is that this term is what remains.
    let mut bare_narrow = anchored_shape(&narrow_profile, Map::TiledV3);
    bare_narrow.count_ids = false;
    let mut bare_wide = anchored_shape(&wide_profile, Map::TiledV3);
    bare_wide.count_ids = false;
    let with_n = derive_court_cost_shaped_v1(&narrow_profile, anchored_shape(&narrow_profile, Map::TiledV3));
    let with_w = derive_court_cost_shaped_v1(&wide_profile, anchored_shape(&wide_profile, Map::TiledV3));
    let bare_n = derive_court_cost_shaped_v1(&narrow_profile, bare_narrow);
    let bare_w = derive_court_cost_shaped_v1(&wide_profile, bare_wide);
    if let (Ok(with_n), Ok(with_w), Ok(bare_n), Ok(bare_w)) = (with_n, with_w, bare_n, bare_w) {
        println!("| reading | close at {narrow} | close at {wide} | growth |");
        println!("|---|---|---|---|");
        println!(
            "| ids counted | {} | {} | {:+} |",
            with_n.max_close_bytes,
            with_w.max_close_bytes,
            with_w.max_close_bytes as i128 - with_n.max_close_bytes as i128
        );
        println!(
            "| ids NOT counted | {} | {} | {:+} |",
            bare_n.max_close_bytes,
            bare_w.max_close_bytes,
            bare_w.max_close_bytes as i128 - bare_n.max_close_bytes as i128
        );
        let id_only = 4 * (wide as i128 - narrow as i128);
        println!();
        println!(
            "The prompt-id term alone would be {id_only:+} bytes ({}x4). The residue with ids removed is {:+}.",
            wide - narrow,
            bare_w.max_close_bytes as i128 - bare_n.max_close_bytes as i128
        );
    }
    println!();
}

/// §5: the binding node's close split into the terms that make it, each read as a SLOPE — which
/// is the only form in which "flat" and "linear" are answerable.
///
/// Four readings of the same walk, differing by one lever each:
/// * **full** — the anchored court as `palw_class_ladder_rules_v1` would state it at the v3 price;
/// * **history pinned to 1** — the interval-scaled run term removed, so what is left is the
///   node's own WIDTH;
/// * **ids off** — the two `n_ctx × 4` id terms removed (`count_ids`, whose whole purpose is to
///   let this question be asked);
/// * **both** — the residue no map and no anchor reaches.
fn decompose(kind: &str, build: impl Fn(Map, u32) -> PalwShapeProfileV3, narrow: u32, wide: u32) {
    println!("### {kind}: the binding node's close, term by term, under the v3 map");
    println!();
    println!("| reading | close at {narrow} | close at {wide} | slope (bytes/position) |");
    println!("|---|---|---|---|");
    let span = (wide - narrow) as f64;
    let at = |n_ctx: u32, history_one: bool, ids: bool| -> Option<u64> {
        let profile = build(Map::TiledV3, n_ctx);
        let mut shape = anchored_shape(&profile, Map::TiledV3);
        if history_one {
            shape.history_positions = 1;
        }
        shape.count_ids = ids;
        derive_court_cost_shaped_v1(&profile, shape).ok().map(|c| c.max_close_bytes)
    };
    for (name, history_one, ids) in [
        ("full (anchored, v3 price, ids counted)", false, true),
        ("history pinned to 1 position", true, true),
        ("ids not counted", false, false),
        ("both — the residue", true, false),
    ] {
        match (at(narrow, history_one, ids), at(wide, history_one, ids)) {
            (Some(a), Some(b)) => {
                println!("| {name} | {a} | {b} | {:.1} |", (b as f64 - a as f64) / span)
            }
            _ => println!("| {name} | — | — | not priceable |"),
        }
    }
    println!();
}

/// §6: the counterfactual that separates the MAP from the INTERVAL RULE.
///
/// `palw_checkpoint_interval_v1` is `max(1, n_ctx / 32)`, so the anchored court's
/// `history_positions` is itself `n_ctx`-shaped: at 4,096 a refutation replays 128 positions where
/// at 1,000 it replays 31. That is a term the v3 map cannot touch and it is the reason a reading of
/// "does the tile make the close flat" has to hold the interval still to be an answer about the
/// tile. Here it is held at [`PALW_ATTN_HISTORY_TILE_V4`] — the tile's own width, the value at which
/// one anchored replay opens exactly one chunk.
fn constant_interval(kind: &str, build: impl Fn(Map, u32) -> PalwShapeProfileV3, contexts: &[u32]) {
    println!("### {kind}: v3 map with the interval held at {PALW_ATTN_HISTORY_TILE_V4}, not `n_ctx / 32`");
    println!();
    println!("| n_ctx | close (ids counted) | close (ids off) | attention-KV node close (ids off) | its slope since the row above |");
    println!("|---|---|---|---|---|");
    let mut previous: Option<(u32, u64)> = None;
    for &n_ctx in contexts {
        let profile = build(Map::TiledV3, n_ctx);
        let mut shape = PalwCourtCostShapeV1::checkpoint_anchored_v1(&profile, PALW_ATTN_HISTORY_TILE_V4, LADDER, 0);
        shape.kv_checkpoint_bytes = Map::TiledV3.kv_checkpoint_bytes(&profile);
        shape.gdn_checkpoint_bytes = gdn_checkpoint_bytes(&profile, Map::TiledV3);
        let counted = derive_court_cost_shaped_v1(&profile, shape).ok().map(|c| c.max_close_bytes);
        shape.count_ids = false;
        let bare = derive_court_cost_shaped_v1(&profile, shape).ok().map(|c| c.max_close_bytes);
        let kv = derive_court_cost_rows_v1(&profile, shape).ok().and_then(|rows| worst_kv_row(&profile, &rows)).map(|r| r.close_bytes);
        let show = |v: Option<u64>| v.map(|v| v.to_string()).unwrap_or_else(|| "not priceable".to_string());
        // The SLOPE is the answer to "flat or not", so it is printed rather than left to be
        // computed: a constant interval with a growing slope says the residue is the node's WIDTH.
        let slope = match (previous, kv) {
            (Some((p_ctx, p_kv)), Some(kv)) if n_ctx > p_ctx => {
                format!("{:.2} bytes/position", (kv as f64 - p_kv as f64) / (n_ctx - p_ctx) as f64)
            }
            _ => "-".to_string(),
        };
        println!("| {n_ctx} | {} | {} | {} | {slope} |", show(counted), show(bare), show(kv));
        if let Some(kv) = kv {
            previous = Some((n_ctx, kv));
        }
    }
    println!();
}

/// §7: the OTHER ceiling — the step ladder, which no state chunk map and no anchoring touches.
///
/// `derive_court_cost_shaped_v1` calls `worst_case_step_leaf_count_capped_v1` before it prices
/// anything, so a class whose whole context as prefill overruns the ladder is not expensive, it is
/// **unpriceable**. The 32,768 rows in §1 are refused there and not at the carrier, and by a margin
/// small enough that a reader who saw only "NOT PRICEABLE" would guess the wrong order of
/// magnitude. This finds the exact width at which each ladder stops.
fn ladder_ceiling(kind: &str, build: impl Fn(Map, u32) -> PalwShapeProfileV3) {
    println!("### {kind}: the widest n_ctx each step ladder admits, before any close is priced");
    println!();
    println!("| ladder | widest n_ctx | its worst-case leaf count | leaf count one position wider |");
    println!("|---|---|---|---|");
    for (name, cap) in [("PALW_STEP_MAX_LEAVES (2^22, shipped)", PALW_STEP_MAX_LEAVES), ("2^32 (Decision 12)", LADDER)] {
        // Doubling then bisecting: `worst_case_step_leaf_count_capped_v1` is monotone in `n_ctx`
        // (it sums a non-negative term per position), so a bisection is exact here in a way the
        // carrier sweep in §3 is not.
        let fits = |n_ctx: u32| n_ctx >= 1 && worst_case_step_leaf_count_capped_v1(&build(Map::TiledV3, n_ctx), cap).is_ok();
        let (mut lo, mut hi) = (1u32, 2u32);
        while fits(hi) && hi < 1 << 20 {
            lo = hi;
            hi *= 2;
        }
        while lo + 1 < hi {
            let mid = lo + (hi - lo) / 2;
            if fits(mid) { lo = mid } else { hi = mid }
        }
        let at = worst_case_step_leaf_count_capped_v1(&build(Map::TiledV3, lo), u64::MAX).map(|v| v.to_string());
        let over = worst_case_step_leaf_count_capped_v1(&build(Map::TiledV3, lo + 1), u64::MAX).map(|v| v.to_string());
        let show = |v: Result<String, _>| v.unwrap_or_else(|_| "(the enumeration bound refuses it)".to_string());
        println!("| {name} | **{lo}** | {} | {} |", show(at), show(over));
    }
    println!();
}

fn main() {
    println!("# U-00 — is graph-v4's tiled attention close FLAT in the position?");
    println!();
    println!("Every number below is `derive_court_cost_shaped_v1`'s, at the `2^32` ladder");
    println!("(`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`), against the {DEFAULT_MAX_CLOSE_BYTES}-byte carrier.");
    println!("The history tile is `PALW_ATTN_HISTORY_TILE_V4` = {PALW_ATTN_HISTORY_TILE_V4}.");
    println!();

    println!("## §0 — the v3 map is not priced by the shipped rule");
    println!();
    // Stated first because every table after it had to construct the v3 price by hand.
    let dense = Map::TiledV3.dense(512);
    let shipped = palw_kv_checkpoint_opening_bytes_v1(&dense, LADDER).expect("derives");
    let honest = Map::TiledV3.kv_checkpoint_bytes(&dense);
    println!(
        "`palw_class_ladder_rules_v1` sets `kv_checkpoint_bytes` from `palw_kv_checkpoint_opening_bytes_v1`\n\
         UNCONDITIONALLY — there is no `…_for_map_v1` twin for the cache, the way there is for the\n\
         recurrence (`palw_gdn_checkpoint_opening_bytes_for_map_v1`). So a class that registers the v3\n\
         map is charged the v2 map's opening at admission:"
    );
    println!();
    println!("| dense n_ctx 512 | bytes |");
    println!("|---|---|");
    println!("| what the shipped rule charges a v3 class | {shipped} |");
    println!("| what a v3 chunk actually opens (`tiled_kv_chunk_bytes_v3` + path) | {honest} |");
    println!("| overcharge factor | {:.1}x |", shipped as f64 / honest as f64);
    println!();

    println!("## §0b — and the graph-v4 HYBRID composition is priced with no recurrence anchor at all");
    println!();
    let hybrid_v3 = Map::TiledV3.hybrid(512);
    let shipped_gdn = palw_gdn_checkpoint_opening_bytes_for_map_v1(&hybrid_v3, LADDER);
    let honest_gdn = gdn_checkpoint_bytes(&hybrid_v3, Map::TiledV3);
    println!(
        "`gdn_state_terms_for_map_v1` dispatches on the WHOLE composition id and knows\n\
         `hybrid_state_chunk_map_id_v1` and `…_v2` only. `palw_hybrid_state_chunk_map_name_v3` spells\n\
         its `gdn=` half as `PALW_GDN_STATE_CHUNK_MAP_NAME_V2` verbatim, so the recurrence opening it\n\
         describes is priceable — but the dispatch answers `None`, and `palw_class_ladder_rules_v1`\n\
         turns that into `.unwrap_or(0)`. That is the UNDER-charging direction, which the comment two\n\
         lines above it names as 'the direction that admits a class whose disputes nobody can raise'."
    );
    println!();
    println!("| hybrid n_ctx 512 on the v4 composition | bytes |");
    println!("|---|---|");
    println!(
        "| what `palw_gdn_checkpoint_opening_bytes_for_map_v1` answers | {} |",
        shipped_gdn.map(|v| v.to_string()).unwrap_or_else(|| "None → charged 0".to_string())
    );
    println!("| what the composition's own `gdn=` half opens | {honest_gdn} |");
    println!();

    println!("## §1–2 — the leaf, and its growth");
    println!();
    let contexts = [1_000u32, 4_096, 32_768];
    attention_leaf_table("Dense (Qwen2.5-1.5B / A16)", |m, n| m.dense(n), &contexts);
    attention_leaf_table("Hybrid (Qwen3.6-35B-A3B)", |m, n| m.hybrid(n), &contexts);

    println!("## §3 — the widest row the carrier admits");
    println!();
    widest_row("Dense (Qwen2.5-1.5B / A16)", |m, n| m.dense(n));
    widest_row("Hybrid (Qwen3.6-35B-A3B)", |m, n| m.hybrid(n));

    println!("## §4 — what is still linear, node by node");
    println!();
    residue("Dense (Qwen2.5-1.5B / A16)", |m, n| m.dense(n), 1_000, 4_096);
    residue("Hybrid (Qwen3.6-35B-A3B)", |m, n| m.hybrid(n), 1_000, 4_096);

    println!("## §5 — the same close, term by term");
    println!();
    decompose("Dense (Qwen2.5-1.5B / A16)", |m, n| m.dense(n), 1_000, 4_096);
    decompose("Hybrid (Qwen3.6-35B-A3B)", |m, n| m.hybrid(n), 1_000, 4_096);

    println!("## §6 — the counterfactual: is it the MAP or the INTERVAL RULE?");
    println!();
    let band = [256u32, 512, 1_024, 4_096];
    constant_interval("Dense (Qwen2.5-1.5B / A16)", |m, n| m.dense(n), &band);
    constant_interval("Hybrid (Qwen3.6-35B-A3B)", |m, n| m.hybrid(n), &band);

    println!("## §7 — the ladder ceiling, which is not the carrier");
    println!();
    ladder_ceiling("Dense (Qwen2.5-1.5B / A16)", |m, n| m.dense(n));
    ladder_ceiling("Hybrid (Qwen3.6-35B-A3B)", |m, n| m.hybrid(n));
}
