//! **The canonical artifact inventory, built from a real artifact** (ADR-0049 Decision G, audit
//! C-06).
//!
//! `PalwArtifactInventoryV1` defined the RULES — one leaf per operand row, ordered by
//! `(tensor_name, layer, byte_offset)`, every tensor tiled from byte 0 with no gap and no overlap
//! — and the only inventories that existed were four-row test fixtures. That left the whole
//! opening path untested against a real class: a court that opens `blk.2.attn_q.weight` at some
//! byte offset can only be right if some producer emits exactly that row, and nothing did.
//!
//! # What a row is, and why it is not a choice
//!
//! The coordinates are the REFUTER'S, not this module's. `palw_step_refute` asks the weight oracle
//! for a specific `(tensor, layer, byte_offset, byte_len)` per op kind, and an inventory whose rows
//! disagree with those coordinates serves nothing:
//!
//! | op | request |
//! |---|---|
//! | `EmbedLookup` | `(token · width, width)` — one row per token id |
//! | `MatMulQuant` | `(tile · tile_len · in_dim, tile_width · in_dim)` — one row per output tile |
//! | `Requantize` | `(0, 9 · channels)`, or `(0, 9)` for a uniform narrowing |
//! | `Rescale` | `(0, 5)` |
//! | `Rope` | `(0, 8 · pairs)` |
//!
//! So the row shape is derived from the graph, and this module's job is to emit exactly those rows
//! and nothing else. A row nobody asks for is a leaf that makes every OTHER opening's Merkle path
//! longer for no reason; a row somebody asks for and nobody emitted is a step that adjudicates
//! `Unadjudicable`, which is the coverage-clean-but-unprosecutable shape ADR-0049 exists to refuse.
//!
//! # What building it found
//!
//! Three defects that no amount of reading the rules could surface, because they are all
//! *correspondence* between four hand-written descriptions of one computation:
//!
//! * the graph named `blk.{layer}.attn_norm.weight`, `blk.{layer}.ffn_norm.weight` and
//!   `output_norm.weight` — three tensor families the engine never reads, since BASE-0's `RmsNorm`
//!   takes no gain vector. No honest artifact could carry them, so no honest inventory could cover
//!   the graph;
//! * the post table declared the final `RmsNorm` and not the narrowing after it, exactly as the
//!   layer table had before it was generated from the IR;
//! * the three narrowings the engine held as `const` (`qk_to_code`, `code_product`, `rope_clamp`)
//!   were named by the graph as registered tensors, and a `const` in a binary is precisely a
//!   parameter nothing can open.

use kaspa_consensus_core::palw_artifact::{PalwArtifactInventoryV1, PalwArtifactOperandV1, PalwInventoryError};
use kaspa_consensus_core::palw_base0_ops::ScaleParams;
use kaspa_consensus_core::palw_base0_profile::{PalwBase0GeometryV1, base0_tensor_names_v1};

use crate::artifact::{ArtifactError, Base0ArtifactV1};
use crate::operands::{BASE0_LAYER_PREFIX, Base0OperandV1, OperandError, base0_resolve_operand_v1};
use crate::plan::BASE0_ENGINE_HEAD_TENSOR;

/// Why an artifact yields no inventory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InventoryBuildError {
    /// The geometry is not this artifact's (see [`Base0ArtifactV1::check_geometry`]).
    Geometry(ArtifactError),
    /// A tile length of zero tiles nothing.
    ZeroTile,
    /// The rows are laid out but do not satisfy the canonical-layout rules — a bug in this builder
    /// rather than in its input, surfaced rather than shipped.
    NotCanonical(PalwInventoryError),
    /// The graph names a tensor this artifact does not carry. The same refusal the engine gives
    /// when it compiles the graph, because it is the same question: an operand nothing can serve
    /// is a step nothing can open.
    Operand(OperandError),
}

/// `(multiplier LE, shift)` — the five bytes op 9 reads.
fn scale_bytes(s: ScaleParams) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    out.extend_from_slice(&s.multiplier.to_le_bytes());
    out.push(s.shift);
    out
}

/// The pinned rotary table for one position row: `cos` then `sin`, four bytes each, one pair per
/// two lanes — the layout `Base0Op::Rope` reads.
///
/// One row per POSITION, because that is what a rotation at position `p` opens. The table is
/// `[position][pair]` row-major, so a position's slice is contiguous and the rows tile it exactly.
fn rope_row_bytes(table: &crate::rope::RopeTableV1, d_head: usize, position: usize) -> Vec<u8> {
    let pairs = d_head / 2;
    let start = position * pairs;
    let mut out = Vec::with_capacity(8 * pairs);
    for v in &table.cos_q[start..start + pairs] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in &table.sin_q[start..start + pairs] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Tile one weight matrix into the rows a `MatMulQuant` opening addresses.
///
/// `weights` is `[out_dim][in_dim]` row-major `int8`, so a tile of output rows is a contiguous byte
/// range and the byte offset IS the row offset — which is what lets the refuter ask in bytes and
/// the leaf answer in bytes without either side converting.
fn tile_matrix(name: &str, layer: Option<u16>, weights: &[i8], in_dim: usize, tile_len: usize) -> Vec<PalwArtifactOperandV1> {
    let mut rows = Vec::new();
    let stride = tile_len * in_dim;
    let mut offset = 0usize;
    while offset < weights.len() {
        let end = (offset + stride).min(weights.len());
        rows.push(PalwArtifactOperandV1 {
            tensor_name: name.to_string(),
            layer,
            row_start: offset as u32,
            bytes: weights[offset..end].iter().map(|v| *v as u8).collect(),
        });
        offset = end;
    }
    rows
}

/// **Every operand row a BASE-0 execution can open, for one artifact at one geometry.**
///
/// The geometry is an argument for the same reason it is one on
/// [`Base0ArtifactV1::execution_class_id`]: `tile_len` decides where a matmul row starts, and no
/// weight file contains it. The artifact is checked against the geometry first, so an inventory can
/// never describe a layout for a class this artifact does not belong to.
pub fn base0_inventory_v1(
    artifact: &Base0ArtifactV1,
    geometry: PalwBase0GeometryV1,
) -> Result<PalwArtifactInventoryV1, InventoryBuildError> {
    artifact.check_geometry(&geometry).map_err(InventoryBuildError::Geometry)?;
    if geometry.tile_len == 0 {
        return Err(InventoryBuildError::ZeroTile);
    }
    let tile = geometry.tile_len as usize;
    let shape = artifact.shape;
    let mut rows: Vec<PalwArtifactOperandV1> = Vec::new();

    // **Every tensor the graph names, resolved through the one binding the engine reads**
    // (ADR-0049 Decision F). The list is `base0_tensor_names_v1`, projected from `BASE0_LAYER_IR`;
    // the bytes come from `base0_resolve_operand_v1`, which is what the forward pass computes
    // against. This module used to carry a second copy of that mapping — twenty-five suffixes
    // beside twenty-five artifact fields — and one entry of it was already wrong: `attn_q.requant`
    // served `layer.requant[0]` unconditionally, while the engine narrows through the per-channel
    // table whenever the artifact carries one. A court opening that tensor against a class with a
    // projection bias would have recomputed an honest step from parameters nobody applied.
    //
    // The ROW SHAPE is still this module's, because it is the refuter's: a matmul opens a tile of
    // output rows, a gather opens the row it gathered, a narrowing opens its whole parameter
    // block, a rotation opens one position.
    let emit = |name: &'static str, layer: Option<usize>, rows: &mut Vec<PalwArtifactOperandV1>| -> Result<(), InventoryBuildError> {
        let li = layer.map(|l| l as u16);
        let block = |bytes: Vec<u8>| PalwArtifactOperandV1 { tensor_name: name.to_string(), layer: li, row_start: 0, bytes };
        match base0_resolve_operand_v1(artifact, name, layer, BASE0_ENGINE_HEAD_TENSOR).map_err(InventoryBuildError::Operand)? {
            Base0OperandV1::Matrix { data, in_dim } => rows.extend(tile_matrix(name, li, data, in_dim, tile)),
            Base0OperandV1::Gather { data, width } => {
                for token in 0..data.len() / width.max(1) {
                    rows.push(PalwArtifactOperandV1 {
                        tensor_name: name.to_string(),
                        layer: li,
                        row_start: (token * width) as u32,
                        bytes: data[token * width..(token + 1) * width].iter().map(|v| *v as u8).collect(),
                    });
                }
            }
            // Nine bytes for a tensor-wide narrowing, nine per channel for a per-channel one —
            // the two shapes `palw_step_refute` accepts, and which one this is is the artifact's
            // answer rather than this module's guess.
            Base0OperandV1::Quant(q) => rows.push(block(q.bytes())),
            Base0OperandV1::Scale(s) => rows.push(block(scale_bytes(s))),
            Base0OperandV1::Rope(table) => {
                let mut offset = 0u32;
                for position in 0..shape.max_position {
                    let bytes = rope_row_bytes(table, shape.d_head, position);
                    let len = bytes.len() as u32;
                    rows.push(PalwArtifactOperandV1 { tensor_name: name.to_string(), layer: li, row_start: offset, bytes });
                    offset += len;
                }
            }
        }
        Ok(())
    };

    for name in base0_tensor_names_v1() {
        if name.starts_with(BASE0_LAYER_PREFIX) {
            for li in 0..shape.n_layers {
                emit(name, Some(li), &mut rows)?;
            }
        } else {
            emit(name, None, &mut rows)?;
        }
    }

    // **Audit finding 11 (per-channel q/k/v narrowings) is closed here by NOT being here.**
    //
    // The finding was real and its consequence the worst kind: the loop this replaced emitted one
    // tensor-wide 9-byte narrowing per tensor while the engine narrows through the per-CHANNEL
    // table when the artifact carries one, `operand_bytes` refused the exact-length request, and
    // `palw_step_refute`'s `.cycle()` fallback answered with the uniform leaf repeated —
    // recomputing every channel with ZERO bias and convicting an honest producer of arithmetic it
    // never performed.
    //
    // Two lines of work found it independently and fixed it in different places. The fix that
    // survives is ADR-0049 Decision F's: `operands.rs` is the ONE resolver the engine and this
    // inventory both read through (`emit` above goes through it), and it serves
    // `qkv_channel_requant` when the artifact has one. Re-adding the hand-written emission here
    // would restore exactly the defect both fixes were about — two name-to-bytes mappings, free to
    // disagree — so the audit's own remedy is deliberately not taken, and its FINDING is what this
    // comment preserves.

    // The canonical order is `(tensor_name, layer, row_start)` ascending, and the constructor
    // refuses anything else. Sorting HERE rather than emitting in order keeps the layout above
    // readable as the graph — and the constructor still checks, so a sort that got it wrong is a
    // refusal rather than a silently different root.
    rows.sort_by(|a, b| (a.tensor_name.as_str(), a.layer, a.row_start).cmp(&(b.tensor_name.as_str(), b.layer, b.row_start)));
    PalwArtifactInventoryV1::new(rows).map_err(InventoryBuildError::NotCanonical)
}

/// **Every operand row an A16-tier execution can open, for one artifact under one registered
/// profile** — the model tier's answer to [`base0_inventory_v1`], and the other half of the
/// court-side parameter conventions `palw_step_refute`'s A16 arms encode.
///
/// The layout normalises the artifact's parameter store to the shapes the arms request:
///
/// * a matmul's codes tile at the NODE's own `tile_len` (the per-node budget, not one global
///   number), and its per-channel triples ride the `.a16` suffix at the same tiling — with the
///   `.sink0` variants carried verbatim where the store registers them;
/// * a `Fixed`-width narrowing's triples are served per lane. A site whose store registers ONE
///   triple is EXPANDED — the engine tiles that triple across the row, so the expansion commits
///   exactly the parameters the execution applied, and the court's one per-lane rule serves
///   every site;
/// * a `KvScaled` narrowing (the probs), the scores and the values keep their single registered
///   triple at offset zero — their lane counts are the job's, so no fixed table can exist;
/// * the softmax widening is its registered single byte; the rotation is one row per position,
///   `cos` then `sin`, exactly the floor's layout.
///
/// The root over these rows is what a court-capable A16 registration pins as `artifact_root`:
/// a flat digest can answer "are these the same bytes" but nothing can be OPENED against it,
/// and a close needs openings.
pub fn a16_inventory_v1(
    artifact: &Base0ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<PalwArtifactInventoryV1, InventoryBuildError> {
    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    use kaspa_consensus_core::palw_step::PalwStepOutLenV1;
    use kaspa_consensus_core::palw_step::kernel_semantics_id_v1 as kid;
    use kaspa_consensus_core::palw_step_refute as kd;

    let store = artifact.a16_params.as_ref().ok_or(InventoryBuildError::Operand(OperandError::UnknownTensor {
        name: "the artifact carries no A16 parameter store".to_string(),
    }))?;
    let w = A16QuantParams::WIRE_BYTES;

    let store_row = |name: &str, layer: Option<u16>| -> Option<&[u8]> {
        let key = match layer {
            Some(l) => name.replace("{layer}", &l.to_string()),
            None => name.to_string(),
        };
        store.iter().find(|(n, _)| *n == key).map(|(_, b)| b.as_slice())
    };
    let missing = |name: &str| InventoryBuildError::Operand(OperandError::UnknownTensor { name: name.to_string() });

    let mut rows: Vec<PalwArtifactOperandV1> = Vec::new();
    let mut seen: std::collections::BTreeSet<(String, Option<u16>, u32)> = std::collections::BTreeSet::new();
    let push = |rows: &mut Vec<PalwArtifactOperandV1>,
                seen: &mut std::collections::BTreeSet<(String, Option<u16>, u32)>,
                name: &str,
                layer: Option<u16>,
                start: u32,
                bytes: Vec<u8>| {
        if seen.insert((name.to_string(), layer, start)) {
            rows.push(PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start: start, bytes });
        }
    };

    // A per-lane triple table for a `Fixed`-width site: the store's own table where it holds one,
    // the single triple expanded across the width where it holds one triple — the engine's own
    // tiling, committed.
    let lane_table = |bytes: &[u8], width: usize, name: &str| -> Result<Vec<u8>, InventoryBuildError> {
        if bytes.len() == width * w {
            Ok(bytes.to_vec())
        } else if bytes.len() == w {
            Ok(bytes.iter().copied().cycle().take(width * w).collect())
        } else {
            Err(missing(&format!("{name}: {} bytes serve neither one triple nor {width}", bytes.len())))
        }
    };
    // Emit a per-lane table chunked at the node's tile — the offsets the arm's
    // `(lane_base × wire, lanes × wire)` requests land on.
    let push_tiled = |rows: &mut Vec<PalwArtifactOperandV1>,
                      seen: &mut std::collections::BTreeSet<(String, Option<u16>, u32)>,
                      name: &str,
                      layer: Option<u16>,
                      table: &[u8],
                      unit: usize,
                      tile_len: usize| {
        let stride = tile_len.max(1) * unit;
        let mut offset = 0usize;
        while offset < table.len() {
            let end = (offset + stride).min(table.len());
            push(rows, seen, name, layer, offset as u32, table[offset..end].to_vec());
            offset = end;
        }
    };

    let matmul_codes = |name: &str, layer: Option<u16>| -> Result<&[i8], InventoryBuildError> {
        let suffix = name.strip_prefix("blk.{layer}.").unwrap_or(name);
        let l = layer.map(|l| l as usize).unwrap_or(0);
        let layer_of = |f: fn(&crate::artifact::Base0LayerWeightsV1) -> &Vec<i8>| -> Result<&[i8], InventoryBuildError> {
            artifact.layers.get(l).map(|lw| f(lw).as_slice()).ok_or_else(|| missing(name))
        };
        match suffix {
            "attn_q.weight" => layer_of(|lw| &lw.wq),
            "attn_k.weight" => layer_of(|lw| &lw.wk),
            "attn_v.weight" => layer_of(|lw| &lw.wv),
            "attn_output.weight" => layer_of(|lw| &lw.wo),
            "ffn_gate.weight" => layer_of(|lw| &lw.w_gate),
            "ffn_up.weight" => layer_of(|lw| &lw.w_up),
            "ffn_down.weight" => layer_of(|lw| &lw.w_down),
            // Both head spellings serve the engine's unembedding — the v2 class names the head
            // view so the gather's rows and these tiles stop sharing a name.
            "output.weight" | "token_embd.weight" => Ok(&artifact.unembed),
            _ => Err(missing(name)),
        }
    };

    let k_embed = kid(kd::KDESC_A16_EMBED);
    let k_mm = kid(kd::KDESC_A16_MATMUL_REQUANT);
    let k_rs = kid(kd::KDESC_A16_MATMUL_RESCALE);
    let k_req = kid(kd::KDESC_A16_REQUANTIZE);
    let k_scores = kid(kd::KDESC_A16_ATTN_SCORES);
    let k_values = kid(kd::KDESC_A16_ATTN_VALUES);
    let k_soft = kid(kd::KDESC_A16_SOFTMAX);
    let k_fused = kid(kd::KDESC_A16_ATTN_FUSED);
    let k_rope = kid(kd::KDESC_A16_ROPE);
    let k_none = [kid(kd::KDESC_A16_RMS_NORM), kid(kd::KDESC_A16_ADD_ELEM), kid(kd::KDESC_A16_MUL_ELEM), kid(kd::KDESC_Q36_SILU)];

    for slot in 0..profile.global_node_count() {
        let Some((node, layer)) = profile.resolve_node_slot(slot) else { continue };
        let name = node.weight_name.as_str();
        let kidv = node.kernel_semantics_id;
        let fixed = match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => Some(elements as usize),
            PalwStepOutLenV1::KvScaled { .. } => None,
        };
        if kidv == k_embed {
            let width = fixed.ok_or_else(|| missing(name))?;
            for token in 0..artifact.embed.len() / width.max(1) {
                push(
                    &mut rows,
                    &mut seen,
                    name,
                    layer,
                    (token * width) as u32,
                    artifact.embed[token * width..(token + 1) * width].iter().map(|v| *v as u8).collect(),
                );
            }
        } else if kidv == k_mm || kidv == k_rs {
            let out_dim = fixed.ok_or_else(|| missing(name))?;
            let codes = matmul_codes(name, layer)?;
            if out_dim == 0 || !codes.len().is_multiple_of(out_dim) {
                return Err(missing(&format!("{name}: {} code bytes over {out_dim} rows", codes.len())));
            }
            let in_dim = codes.len() / out_dim;
            let tile = node.tile_len as usize;
            for (t, chunk) in codes.chunks(tile.max(1) * in_dim).enumerate() {
                push(&mut rows, &mut seen, name, layer, (t * tile * in_dim) as u32, chunk.iter().map(|v| *v as u8).collect());
            }
            for variant in ["", ".sink0"] {
                let triple_name = format!("{name}.a16{variant}");
                // The head's triples live in the store under the TIED spelling — the engine loads
                // `logits_out` from `token_embd.weight.a16` — while the v2 class addresses the
                // head view by its own name. The inventory is the canonical layout, so it aliases:
                // the bytes are the store's, the coordinate is the class's.
                let store_key = if name == "output.weight" { format!("token_embd.weight.a16{variant}") } else { triple_name.clone() };
                match store_row(&store_key, layer) {
                    Some(bytes) => {
                        let table = lane_table(bytes, out_dim, &triple_name)?;
                        push_tiled(&mut rows, &mut seen, &triple_name, layer, &table, w, tile);
                    }
                    None if variant == ".sink0" => {} // a site without the sink convention
                    None => return Err(missing(&triple_name)),
                }
            }
        } else if kidv == k_req {
            match fixed {
                Some(width) => {
                    for variant in ["", ".sink0"] {
                        let triple_name = if variant.is_empty() { name.to_string() } else { format!("{name}{variant}") };
                        match store_row(&triple_name, layer) {
                            Some(bytes) => {
                                let table = lane_table(bytes, width, &triple_name)?;
                                push_tiled(&mut rows, &mut seen, &triple_name, layer, &table, w, node.tile_len as usize);
                            }
                            None if variant == ".sink0" => {}
                            None => return Err(missing(&triple_name)),
                        }
                    }
                }
                // The probs: one registered triple, tiled by the kernel at the job's width.
                None => {
                    let bytes = store_row(name, layer).ok_or_else(|| missing(name))?;
                    if bytes.len() != w {
                        return Err(missing(&format!("{name}: a job-scaled site registers exactly one triple")));
                    }
                    push(&mut rows, &mut seen, name, layer, 0, bytes.to_vec());
                }
            }
        } else if kidv == k_scores || kidv == k_values {
            let bytes = store_row(name, layer).ok_or_else(|| missing(name))?;
            if bytes.len() != w {
                return Err(missing(&format!("{name}: the attention sites register exactly one triple")));
            }
            push(&mut rows, &mut seen, name, layer, 0, bytes.to_vec());
        } else if kidv == k_soft {
            let bytes = store_row(name, layer).ok_or_else(|| missing(name))?;
            if bytes.len() != 1 {
                return Err(missing(&format!("{name}: the softmax widening is one registered byte")));
            }
            push(&mut rows, &mut seen, name, layer, 0, bytes.to_vec());
        } else if kidv == k_fused {
            // **ADR-0082 Decision 1: ONE node, FOUR registered operands, and the artifact is
            // unchanged.** A fused site reads exactly the tensors the four nodes it replaces read
            // — W9's score triple, the probability triple, W10's value triple and the softmax's
            // widening byte — so the inventory it implies is byte for byte the one the v2 graph
            // implies at this site, and no re-conversion follows from graph v5.
            //
            // The three it does not NAME are derived from the one it does, through the single
            // description the engine's plan compiler and the court's arm also read
            // (`palw_attn_fused_tensors_v1`). Two spellings of this mapping would be an operand
            // the court resolves and the inventory cannot open, which is `Unadjudicable` on
            // honest material.
            let t = kd::palw_attn_fused_tensors_v1(name).ok_or_else(|| missing(name))?;
            let up = store_row(&t.softmax_up, layer).ok_or_else(|| missing(&t.softmax_up))?;
            if up.len() != 1 {
                return Err(missing(&format!("{}: the softmax widening is one registered byte", t.softmax_up)));
            }
            push(&mut rows, &mut seen, &t.softmax_up, layer, 0, up.to_vec());
            for triple in [&t.scores, &t.probs, &t.values] {
                let bytes = store_row(triple, layer).ok_or_else(|| missing(triple))?;
                if bytes.len() != w {
                    return Err(missing(&format!("{triple}: the attention sites register exactly one triple")));
                }
                push(&mut rows, &mut seen, triple, layer, 0, bytes.to_vec());
            }
        } else if kidv == k_rope {
            let mut offset = 0u32;
            for position in 0..artifact.shape.max_position {
                let bytes = rope_row_bytes(&artifact.rope, artifact.shape.d_head, position);
                let len = bytes.len() as u32;
                push(&mut rows, &mut seen, name, layer, offset, bytes);
                offset += len;
            }
        } else if k_none.contains(&kidv) {
            // Parameterless: nothing to open, nothing to emit.
        } else {
            return Err(missing(&format!("{name}: kernel this inventory does not lay out")));
        }
    }

    rows.sort_by(|a, b| (a.tensor_name.as_str(), a.layer, a.row_start).cmp(&(b.tensor_name.as_str(), b.layer, b.row_start)));
    PalwArtifactInventoryV1::new(rows).map_err(InventoryBuildError::NotCanonical)
}

/// **Every operand row a Qwen3.6-family execution can open, for one artifact under one
/// registered profile** — the hybrid tier's answer to [`a16_inventory_v1`], and the other half
/// of the court-side parameter conventions `palw_step_refute`'s shared arms encode.
///
/// The layout normalises the artifact's store to the shapes the arms request, committing in
/// every case exactly the parameters the ENGINE applied (`qwen36_plan.rs`'s RESOLUTION_V2 rules,
/// restated as rows):
///
/// * a grouped matmul's codes tile at the node's own `tile_len`; its per-row triples ride the
///   `.a16` suffix at the same tiling, and its per-32 exponents ride `.exp` — ZERO-filled where
///   the artifact stores none, because the engine's exponent-less dispatch IS the zero-exponent
///   arithmetic (`q36_matmul_grouped*` at `exp = 0` is `a16_matmul_requant`/`_rescale` bit for
///   bit);
/// * the routed projections' and the routed gated-multiply's stores are PER EXPERT
///   (`blk.N.ffn_expert.{e}_gate.weight`, `…{e}_silu.a16`, …), chunked at the node's tile
///   restarting at each expert's own byte 0 — the covering-chunk discipline the arms read;
/// * a `Fixed`-width narrowing's triples are served per lane, singletons expanded — and the
///   QK-norm requants at the ENGINE's head tiling (a `head_dim` store repeated per head);
/// * the recurrence's four narrowings interleave under the declared `linear_gdn.a16`
///   coordinate, `[read, delta, write, out]` per value head — the bytes are the four stores',
///   the coordinate is the class's (the same aliasing license the A16 head triples use);
/// * the decay's two calibration rows keep their own store names at per-head width; the wide
///   norm's eps rows are one triple per value head;
/// * the rotation's table rows ride under the node's declared name (one row per position, `cos`
///   then `sin` at the class's ROTARY width) with the clamp triple under `.clamp`;
/// * the softmax widening is ONE byte (the store's scalar, clamped to the op's domain); the
///   scores, values, gate, router and combine sites keep their single registered triple.
pub fn qwen36_inventory_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<PalwArtifactInventoryV1, InventoryBuildError> {
    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    use kaspa_consensus_core::palw_qwen36_ops::QWEN36_WEIGHT_GROUP;
    use kaspa_consensus_core::palw_step::PalwStepOutLenV1;
    use kaspa_consensus_core::palw_step::kernel_semantics_id_v1 as kid;
    use kaspa_consensus_core::palw_step_refute as kd;

    let w = A16QuantParams::WIRE_BYTES;
    let missing = |name: &str| InventoryBuildError::Operand(OperandError::UnknownTensor { name: name.to_string() });
    let sub = |name: &str, layer: Option<u16>| -> String {
        match layer {
            Some(l) => name.replace("{layer}", &l.to_string()),
            None => name.to_string(),
        }
    };
    let param_rows =
        |name: &str| -> Result<Vec<A16QuantParams>, InventoryBuildError> { artifact.param_rows(name).map_err(|_| missing(name)) };
    let wire = |rows: &[A16QuantParams]| -> Vec<u8> { rows.iter().flat_map(|p| p.to_wire()).collect() };
    // The engine's own widening rules, committed: a singleton tiles across the width, a full
    // table rides verbatim, anything else is a store this class cannot serve per lane.
    let expand = |rows: Vec<A16QuantParams>, width: usize, name: &str| -> Result<Vec<A16QuantParams>, InventoryBuildError> {
        if rows.len() == width {
            Ok(rows)
        } else if rows.len() == 1 {
            Ok(vec![rows[0]; width])
        } else {
            Err(missing(&format!("{name}: {} triples serve neither one lane nor {width}", rows.len())))
        }
    };
    // The engine's per-head read (`rows[vh.min(len - 1)]`), one value head at a time.
    let per_head = |rows: &[A16QuantParams], vh: usize| -> A16QuantParams { rows[vh.min(rows.len().saturating_sub(1))] };

    let mut rows: Vec<PalwArtifactOperandV1> = Vec::new();
    let mut seen: std::collections::BTreeSet<(String, Option<u16>, u32)> = std::collections::BTreeSet::new();
    let push = |rows: &mut Vec<PalwArtifactOperandV1>,
                seen: &mut std::collections::BTreeSet<(String, Option<u16>, u32)>,
                name: &str,
                layer: Option<u16>,
                start: u32,
                bytes: Vec<u8>| {
        if seen.insert((name.to_string(), layer, start)) {
            rows.push(PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start: start, bytes });
        }
    };
    // A table chunked at `chunk` units of `unit` bytes each, restarting at byte 0 — the offsets
    // the arms' covering-chunk reads land on.
    let push_chunked = |rows: &mut Vec<PalwArtifactOperandV1>,
                        seen: &mut std::collections::BTreeSet<(String, Option<u16>, u32)>,
                        name: &str,
                        layer: Option<u16>,
                        table: &[u8],
                        unit: usize,
                        chunk: usize| {
        let stride = chunk.max(1) * unit.max(1);
        let mut offset = 0usize;
        while offset < table.len() {
            let end = (offset + stride).min(table.len());
            push(rows, seen, name, layer, offset as u32, table[offset..end].to_vec());
            offset = end;
        }
    };

    let k_embed = kid(kd::KDESC_A16_EMBED);
    let k_req = kid(kd::KDESC_A16_REQUANTIZE);
    let k_soft = kid(kd::KDESC_A16_SOFTMAX);
    let k_scores = kid(kd::KDESC_A16_ATTN_SCORES);
    let k_values = kid(kd::KDESC_A16_ATTN_VALUES);
    let k_fused = kid(kd::KDESC_A16_ATTN_FUSED);
    let k_grouped = kid(kd::KDESC_Q36_MATMUL_GROUPED);
    let k_grouped_wide = kid(kd::KDESC_Q36_MATMUL_GROUPED_WIDE);
    let k_conv = kid(kd::KDESC_Q36_SSM_CONV);
    let k_decay = kid(kd::KDESC_Q36_DECAY);
    let k_gdn = kid(kd::KDESC_Q36_GDN_STEP);
    let k_rms_wide = kid(kd::KDESC_Q36_RMS_NORM_WIDE);
    let k_rescale_row = kid(kd::KDESC_Q36_RESCALE_ROW);
    let k_mul_wide = kid(kd::KDESC_Q36_MUL_WIDE);
    let k_gate_apply = kid(kd::KDESC_Q36_GATE_APPLY);
    let k_rope = kid(kd::KDESC_Q36_ROPE_PARTIAL);
    let k_topk = kid(kd::KDESC_Q36_ROUTER_TOPK);
    let k_combine = kid(kd::KDESC_Q36_MOE_COMBINE);
    let k_none = [
        kid(kd::KDESC_A16_RMS_NORM),
        kid(kd::KDESC_A16_ADD_ELEM),
        kid(kd::KDESC_Q36_SILU),
        kid(kd::KDESC_Q36_SIGMOID),
        kid(kd::KDESC_Q36_L2_NORM),
        kid(kd::KDESC_Q36_HEAD_RMS_NORM),
    ];

    // One registered triple at offset zero — the sites whose lane counts are the job's, or whose
    // parameter is a single scalar.
    let one_triple = |rows: &mut Vec<PalwArtifactOperandV1>,
                      seen: &mut std::collections::BTreeSet<(String, Option<u16>, u32)>,
                      name: &str,
                      layer: Option<u16>|
     -> Result<(), InventoryBuildError> {
        let store = param_rows(&sub(name, layer))?;
        if store.len() != 1 {
            return Err(missing(&format!("{name}: this site registers exactly one triple")));
        }
        push(rows, seen, name, layer, 0, wire(&store));
        Ok(())
    };
    // A per-expert grouped-projection store: codes, exponents (zeros where the artifact carries
    // none) and per-row triples, chunked at the node's tile restarting at the expert's byte 0.
    let push_expert_projection = |rows: &mut Vec<PalwArtifactOperandV1>,
                                  seen: &mut std::collections::BTreeSet<(String, Option<u16>, u32)>,
                                  template: &str,
                                  store: &str,
                                  layer: Option<u16>,
                                  block_rows: usize,
                                  tile: usize|
     -> Result<(), InventoryBuildError> {
        let codes = artifact.tensor(store).map_err(|_| missing(store))?;
        if block_rows == 0 || !codes.len().is_multiple_of(block_rows) {
            return Err(missing(&format!("{store}: {} code bytes over {block_rows} rows", codes.len())));
        }
        let in_dim = codes.len() / block_rows;
        let groups = in_dim.div_ceil(QWEN36_WEIGHT_GROUP);
        let chunk = tile.min(block_rows).max(1);
        let code_bytes: Vec<u8> = codes.iter().map(|v| *v as u8).collect();
        push_chunked(rows, seen, template, layer, &code_bytes, in_dim, chunk);
        let exp_name = format!("{store}.exp");
        let exps: Vec<u8> = match artifact.tensor(&exp_name) {
            Ok(e) => {
                if e.len() != block_rows * groups {
                    return Err(missing(&format!("{exp_name}: {} exponents over {block_rows}x{groups}", e.len())));
                }
                e.iter().map(|v| *v as u8).collect()
            }
            // Absent means every group's exponent is zero — the arithmetic the engine's
            // exponent-less dispatch performs, committed as the bytes it is.
            Err(_) => vec![0u8; block_rows * groups],
        };
        push_chunked(rows, seen, &format!("{template}.exp"), layer, &exps, groups, chunk);
        let triples = expand(param_rows(&format!("{store}.a16"))?, block_rows, store)?;
        push_chunked(rows, seen, &format!("{template}.a16"), layer, &wire(&triples), w, chunk);
        Ok(())
    };
    // How many routed experts this layer's artifact stores, by the store's own naming.
    let expert_count = |layer: u16, suffix: &str| -> usize {
        let mut e = 0usize;
        while e < 65_536 && artifact.tensor(&format!("blk.{layer}.ffn_expert.{e}{suffix}")).is_ok() {
            e += 1;
        }
        e
    };

    for slot in 0..profile.global_node_count() {
        let Some((node, layer)) = profile.resolve_node_slot(slot) else { continue };
        let name = node.weight_name.as_str();
        let kidv = node.kernel_semantics_id;
        let tile = node.tile_len as usize;
        let fixed = match node.out_len {
            PalwStepOutLenV1::Fixed { elements } => Some(elements as usize),
            PalwStepOutLenV1::KvScaled { .. } => None,
        };
        if name.is_empty() || k_none.contains(&kidv) {
            continue; // parameterless: nothing to open, nothing to emit
        }
        if kidv == k_embed {
            let width = fixed.ok_or_else(|| missing(name))?;
            let table = artifact.tensor(&sub(name, layer)).map_err(|_| missing(name))?;
            for token in 0..table.len() / width.max(1) {
                push(
                    &mut rows,
                    &mut seen,
                    name,
                    layer,
                    (token * width) as u32,
                    table[token * width..(token + 1) * width].iter().map(|v| *v as u8).collect(),
                );
            }
        } else if kidv == k_grouped || kidv == k_grouped_wide {
            if name.ends_with(".routed") {
                let (suffix, block_rows) = if name.ends_with(".ffn_down_exps.routed") {
                    ("_down.weight", profile.hidden_dim as usize)
                } else if name.ends_with(".ffn_up_exps.routed") {
                    ("_up.weight", profile.ffn_dim as usize)
                } else {
                    ("_gate.weight", profile.ffn_dim as usize)
                };
                let l = layer.ok_or_else(|| missing(name))?;
                let prefix = name.rfind("ffn_").map(|i| &name[..i]).ok_or_else(|| missing(name))?;
                let experts = expert_count(l, suffix);
                if experts == 0 {
                    return Err(missing(&format!("{name}: the artifact stores no routed expert")));
                }
                for e in 0..experts {
                    push_expert_projection(
                        &mut rows,
                        &mut seen,
                        &format!("{prefix}ffn_expert.{e}{suffix}"),
                        &format!("blk.{l}.ffn_expert.{e}{suffix}"),
                        layer,
                        block_rows,
                        tile,
                    )?;
                }
            } else {
                let out_dim = fixed.ok_or_else(|| missing(name))?;
                push_expert_projection(&mut rows, &mut seen, name, &sub(name, layer), layer, out_dim, tile)?;
            }
        } else if kidv == k_req {
            match fixed {
                Some(width) => {
                    let store = param_rows(&sub(name, layer))?;
                    // The QK-norm requants ride the ENGINE's head tiling: a `head_dim` store
                    // (or a singleton) repeated per head, which is exactly the per-lane table
                    // the execution applied.
                    let head_tiled = if name.ends_with(".attn_q_norm.a16") {
                        Some(profile.attn_heads as usize)
                    } else if name.ends_with(".attn_k_norm.a16") {
                        Some(profile.attn_kv_heads as usize)
                    } else {
                        None
                    };
                    let table = match head_tiled {
                        Some(heads) if heads > 0 && width.is_multiple_of(heads) => {
                            let per = expand(store, width / heads, name)?;
                            let mut t = Vec::with_capacity(width);
                            for _ in 0..heads {
                                t.extend_from_slice(&per);
                            }
                            t
                        }
                        _ => expand(store, width, name)?,
                    };
                    push_chunked(&mut rows, &mut seen, name, layer, &wire(&table), w, tile);
                }
                // The probs: one registered triple, tiled by the kernel at the job's width.
                None => one_triple(&mut rows, &mut seen, name, layer)?,
            }
        } else if kidv == k_rescale_row {
            let width = fixed.ok_or_else(|| missing(name))?;
            let table = expand(param_rows(&sub(name, layer))?, width, name)?;
            push_chunked(&mut rows, &mut seen, name, layer, &wire(&table), w, tile);
        } else if kidv == k_rms_wide {
            // Per value head, ONE triple per row: the arm reads head `vh`'s eps at offset
            // `vh · wire`, one at a time.
            let width = fixed.ok_or_else(|| missing(name))?;
            let hd = (profile.gdn_head_v_dim as usize).max(1);
            if !width.is_multiple_of(hd) {
                return Err(missing(&format!("{name}: the wide-norm row is not a whole number of heads")));
            }
            let store = param_rows(&sub(name, layer))?;
            for vh in 0..width / hd {
                push(&mut rows, &mut seen, name, layer, (vh * w) as u32, per_head(&store, vh).to_wire().to_vec());
            }
        } else if kidv == k_mul_wide {
            if name.ends_with(".ffn_expert_gated.a16") {
                let l = layer.ok_or_else(|| missing(name))?;
                let prefix = name.rfind("ffn_").map(|i| &name[..i]).ok_or_else(|| missing(name))?;
                let block_rows = profile.ffn_dim as usize;
                let chunk = tile.min(block_rows).max(1);
                let experts = expert_count(l, "_gate.weight");
                for e in 0..experts {
                    for stage in ["_silu.a16", "_gated.a16"] {
                        let table = expand(param_rows(&format!("blk.{l}.ffn_expert.{e}{stage}"))?, block_rows, name)?;
                        push_chunked(&mut rows, &mut seen, &format!("{prefix}ffn_expert.{e}{stage}"), layer, &wire(&table), w, chunk);
                    }
                }
            } else if name.ends_with(".ffn_shared_expert_gated.a16") {
                let width = fixed.ok_or_else(|| missing(name))?;
                let stem = name.strip_suffix("ffn_shared_expert_gated.a16").ok_or_else(|| missing(name))?;
                for stage in ["ffn_shared_expert_silu.a16", "ffn_shared_expert_gated.a16"] {
                    let template = format!("{stem}{stage}");
                    let table = expand(param_rows(&sub(&template, layer))?, width, &template)?;
                    push_chunked(&mut rows, &mut seen, &template, layer, &wire(&table), w, tile);
                }
            } else if name.ends_with(".ffn_shared_gated.a16") {
                one_triple(&mut rows, &mut seen, name, layer)?;
            } else {
                let width = fixed.ok_or_else(|| missing(name))?;
                let table = expand(param_rows(&sub(name, layer))?, width, name)?;
                push_chunked(&mut rows, &mut seen, name, layer, &wire(&table), w, tile);
            }
        } else if kidv == k_gate_apply || kidv == k_scores || kidv == k_values || kidv == k_topk || kidv == k_combine {
            one_triple(&mut rows, &mut seen, name, layer)?;
        } else if kidv == k_soft {
            // ONE raw byte — the widening the engine reads through `scalar()`, at the domain the
            // op clamps to.
            let store = param_rows(&sub(name, layer))?;
            if store.len() != 1 {
                return Err(missing(&format!("{name}: the softmax widening is one registered scalar")));
            }
            push(&mut rows, &mut seen, name, layer, 0, vec![store[0].zero.clamp(0, 62) as u8]);
        } else if kidv == k_fused {
            // **ADR-0082 Decision 1**, the hybrid's half: the same four operands the four nodes it
            // replaces read, derived from the one the node names — the softmax byte normalised the
            // way this family's softmax arm normalises it, and one triple each for the scores, the
            // probabilities and the values.
            let t = kd::palw_attn_fused_tensors_v1(name).ok_or_else(|| missing(name))?;
            let store = param_rows(&sub(&t.softmax_up, layer))?;
            if store.len() != 1 {
                return Err(missing(&format!("{}: the softmax widening is one registered scalar", t.softmax_up)));
            }
            push(&mut rows, &mut seen, &t.softmax_up, layer, 0, vec![store[0].zero.clamp(0, 62) as u8]);
            for triple in [&t.scores, &t.probs, &t.values] {
                one_triple(&mut rows, &mut seen, triple, layer)?;
            }
        } else if kidv == k_decay {
            let width = fixed.ok_or_else(|| missing(name))?;
            let stem = name.strip_suffix("linear_decay.a16").ok_or_else(|| missing(name))?;
            for store in ["linear_decay_c.a16", "linear_dt_bias.a16"] {
                let template = format!("{stem}{store}");
                let table = param_rows(&sub(&template, layer))?;
                let effective: Vec<A16QuantParams> = (0..width).map(|vh| per_head(&table, vh)).collect();
                push(&mut rows, &mut seen, &template, layer, 0, wire(&effective));
            }
        } else if kidv == k_gdn {
            // `[read, delta, write, out]` per value head, interleaved under the DECLARED
            // coordinate: the bytes are the four stores', the coordinate is the class's.
            let stem = name.strip_suffix("linear_gdn.a16").ok_or_else(|| missing(name))?;
            let heads = (profile.gdn_heads as usize).max(1);
            let stores: Vec<Vec<A16QuantParams>> = ["linear_read.a16", "linear_delta.a16", "linear_write.a16", "linear_out.a16"]
                .iter()
                .map(|s| param_rows(&sub(&format!("{stem}{s}"), layer)))
                .collect::<Result<_, _>>()?;
            for vh in 0..heads {
                let mut bytes = Vec::with_capacity(4 * w);
                for store in &stores {
                    bytes.extend_from_slice(&per_head(store, vh).to_wire());
                }
                push(&mut rows, &mut seen, name, layer, (vh * 4 * w) as u32, bytes);
            }
        } else if kidv == k_conv {
            let width = fixed.ok_or_else(|| missing(name))?;
            let taps = artifact.tensor(&sub(name, layer)).map_err(|_| missing(name))?;
            if taps.len() != 4 * width {
                return Err(missing(&format!("{name}: {} taps over {width} channels", taps.len())));
            }
            let tap_bytes: Vec<u8> = taps.iter().map(|v| *v as u8).collect();
            push_chunked(&mut rows, &mut seen, name, layer, &tap_bytes, 4, tile);
            let stem = name.strip_suffix(".weight").ok_or_else(|| missing(name))?;
            let template = format!("{stem}.a16");
            let table = expand(param_rows(&sub(&template, layer))?, width, &template)?;
            push_chunked(&mut rows, &mut seen, &template, layer, &wire(&table), w, tile);
        } else if kidv == k_rope {
            // One row per position: `cos` then `sin` at the class's ROTARY width — the slice of
            // the pinned table the partial rotation reads.
            let pairs = (profile.rope_dims as usize) / 2;
            let table_pairs = artifact.rope.d_head / 2;
            if pairs == 0 || pairs > table_pairs {
                return Err(missing(&format!("{name}: a {pairs}-pair rotation over a {table_pairs}-pair table")));
            }
            let mut offset = 0u32;
            for position in 0..artifact.shape.max_position {
                let start = position * table_pairs;
                let mut bytes = Vec::with_capacity(8 * pairs);
                for v in &artifact.rope.cos_q[start..start + pairs] {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                for v in &artifact.rope.sin_q[start..start + pairs] {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                let len = bytes.len() as u32;
                push(&mut rows, &mut seen, name, layer, offset, bytes);
                offset += len;
            }
            // The clamp triple, under the suffix that keeps it from colliding with the table's
            // byte 0 — the store's own row is the node's bare name.
            let store = param_rows(&sub(name, layer))?;
            if store.len() != 1 {
                return Err(missing(&format!("{name}: the rotation registers exactly one clamp triple")));
            }
            push(&mut rows, &mut seen, &format!("{name}.clamp"), layer, 0, wire(&store));
        } else {
            return Err(missing(&format!("{name}: kernel this inventory does not lay out")));
        }
    }

    rows.sort_by(|a, b| (a.tensor_name.as_str(), a.layer, a.row_start).cmp(&(b.tensor_name.as_str(), b.layer, b.row_start)));
    PalwArtifactInventoryV1::new(rows).map_err(InventoryBuildError::NotCanonical)
}

#[cfg(test)]
mod tests {

    /// **A tied head is adjudicable, because the two views are two tensors.**
    ///
    /// One tensor cannot serve both a per-token GATHER (whose operand is the single row it read)
    /// and a tile-width head MATMUL (whose operand is a tile spanning many rows) — tiling the
    /// gather would make every lookup open neighbours it never touched, and not tiling the matmul
    /// would make one operand the whole table. A class that ties its embedding to its head would
    /// then have a logits step no canonical inventory could open, and the court would be unable to
    /// adjudicate the one step that decides the token.
    ///
    /// `Base0ArtifactV1` settles it upstream: `embed` and `unembed` are separate fields, and an
    /// artifact that ties them "does so by carrying equal bytes". The inventory emits both views
    /// unconditionally, so a tied class is a size question — the same weights appear twice — and
    /// never an adjudicability one. Pinned here because the property is invisible at the point it
    /// is relied on.
    #[test]
    fn a_tied_head_still_gets_both_views_in_the_inventory() {
        let mut g = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY;
        g.layer_count = 2;
        g.hidden_dim = 64;
        g.ffn_dim = 128;
        g.attn_heads = 2;
        g.attn_head_dim = 32;
        g.vocab_size = 128;
        g.n_ctx = 32;
        let shape = crate::artifact::Base0ShapeV1 {
            n_layers: g.layer_count as usize,
            n_heads: g.attn_heads as usize,
            n_kv_heads: g.attn_heads as usize,
            d_head: g.attn_head_dim as usize,
            d_ff: g.ffn_dim as usize,
            vocab: g.vocab_size as usize,
            max_position: g.n_ctx as usize,
            eps_q: g.rms_eps_q,
            ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
        };
        let mut artifact = Base0ArtifactV1::derive_deterministic(shape, 0x71ED).expect("derivable");
        // Tie them, the only way this format can: equal bytes.
        artifact.unembed = artifact.embed.clone();
        let inventory = base0_inventory_v1(&artifact, g).expect("a tied artifact still roots");
        let names: std::collections::BTreeSet<&str> = inventory.operands().iter().map(|o| o.tensor_name.as_str()).collect();
        assert!(names.contains("token_embd.weight"), "the gather's view is present");
        assert!(names.contains("output.weight"), "and so is the head matmul's, for the same bytes");

        // And they really are different SHAPES of the same weights: one row per token against
        // tiles of `tile_len`.
        let gather_rows = inventory.operands().iter().filter(|o| o.tensor_name == "token_embd.weight").count();
        let head_tiles = inventory.operands().iter().filter(|o| o.tensor_name == "output.weight").count();
        assert_eq!(gather_rows, artifact.shape.vocab, "one operand per token id");
        assert_ne!(head_tiles, gather_rows, "the head is tiled, which is why one view could not serve both");
    }
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use kaspa_consensus_core::palw_base0_ops::QuantParams;
    use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};

    fn rc_shape() -> Base0ShapeV1 {
        let g = PALW_RC_BASE0_GEOMETRY;
        Base0ShapeV1 {
            n_layers: g.layer_count as usize,
            n_heads: g.attn_heads as usize,
            n_kv_heads: g.attn_heads as usize,
            d_head: g.attn_head_dim as usize,
            d_ff: g.ffn_dim as usize,
            vocab: g.vocab_size as usize,
            max_position: g.n_ctx as usize,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: g.rms_eps_q,
        }
    }

    fn rc_artifact() -> Base0ArtifactV1 {
        Base0ArtifactV1::derive_deterministic(rc_shape(), 20_260_821).unwrap()
    }

    /// **A narrowing is served as the producer APPLIED it, not as one field of the container.**
    ///
    /// The engine narrows q, k and v through `qkv_channel_requant` whenever the artifact carries
    /// one — that is where a projection bias lives, in each channel's `zero` — and this module used
    /// to emit `layer.requant[0]` for `attn_q.requant` unconditionally, because it held its own
    /// copy of the name-to-field mapping.
    ///
    /// The failure that makes is not a refusal. `palw_step_refute` asks for `9 × channels` bytes
    /// and, finding a nine-byte row, CYCLES it across every channel (`palw_step_refute.rs:719`) —
    /// so the court recomputes an honest step from a tensor-wide narrowing the producer never
    /// applied, gets a different row, and convicts. Silently, and only for classes with a bias,
    /// which is every Qwen2.5 member and not the floor.
    ///
    /// Against the old builder this fails on the row's LENGTH: nine bytes where the producer's
    /// parameters are nine per channel. The old bytes are reconstructed here so the difference is
    /// asserted rather than described.
    #[test]
    fn a_per_channel_narrowing_is_served_as_the_producer_applied_it() {
        let mut artifact = rc_artifact();
        let d = artifact.shape.d_model();
        let kv = artifact.shape.kv_dim();
        let table = |n: usize, zero: i32| -> Vec<QuantParams> {
            (0..n).map(|i| QuantParams { multiplier: i32::MAX, shift: 7, zero: zero + i as i32 }).collect()
        };
        for l in artifact.layers.iter_mut() {
            l.qkv_channel_requant = Some([table(d, 1_000), table(kv, 2_000), table(kv, 3_000)]);
        }
        let inventory = base0_inventory_v1(&artifact, PALW_RC_BASE0_GEOMETRY).expect("a legal layout");

        // What the old builder emitted: nine bytes of the tensor-wide parameter.
        let uniform = artifact.layers[0].requant[0];
        let mut old = Vec::with_capacity(9);
        old.extend_from_slice(&uniform.multiplier.to_le_bytes());
        old.push(uniform.shift);
        old.extend_from_slice(&uniform.zero.to_le_bytes());

        let row = inventory
            .operands()
            .iter()
            .find(|o| o.tensor_name == "blk.{layer}.attn_q.requant" && o.layer == Some(0))
            .expect("the narrowing is carried");
        assert_eq!(row.bytes.len(), 9 * d, "one block of nine bytes per output channel");
        assert_ne!(row.bytes, old, "the old builder's row is not the one the producer applied");
        // Each channel's own zero point, in order — the bias the court would otherwise cycle away.
        for (i, chunk) in row.bytes.chunks_exact(9).enumerate() {
            assert_eq!(i32::from_le_bytes([chunk[5], chunk[6], chunk[7], chunk[8]]), 1_000 + i as i32, "channel {i}");
        }

        // The other two narrowings follow their own tables, not q's.
        for (name, base, len) in [("attn_k.requant", 2_000, kv), ("attn_v.requant", 3_000, kv)] {
            let row = inventory
                .operands()
                .iter()
                .find(|o| o.tensor_name == format!("blk.{{layer}}.{name}") && o.layer == Some(1))
                .unwrap_or_else(|| panic!("{name} is carried"));
            assert_eq!(row.bytes.len(), 9 * len);
            assert_eq!(i32::from_le_bytes([row.bytes[5], row.bytes[6], row.bytes[7], row.bytes[8]]), base, "{name}");
        }
    }

    /// **Audit C-06: a real inventory, from a real artifact, covering the real graph.**
    ///
    /// The rules existed and the only inventories that existed were four-row fixtures, so the
    /// whole opening path was untested against a class anyone could register. `verify_covers_profile`
    /// is the assertion that matters: every tensor the graph names is carried, which is what makes
    /// an opening's ABSENCE mean something rather than meaning the producer forgot a tensor.
    #[test]
    fn the_floors_inventory_covers_the_floors_graph() {
        let artifact = rc_artifact();
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's graph");
        let inventory = base0_inventory_v1(&artifact, PALW_RC_BASE0_GEOMETRY).expect("a real artifact yields a real inventory");

        inventory.verify_covers_profile(&profile).expect("every tensor the graph names is carried");
        assert_ne!(inventory.root(), kaspa_hashes::Hash64::default(), "and it has an artifact root");

        // The root is a function of the artifact: different weights, different root, same rules.
        let other = Base0ArtifactV1::derive_deterministic(rc_shape(), 20_260_822).unwrap();
        let other_inventory = base0_inventory_v1(&other, PALW_RC_BASE0_GEOMETRY).unwrap();
        assert_ne!(other_inventory.root(), inventory.root());
        assert_eq!(other_inventory.operands().len(), inventory.operands().len(), "the LAYOUT is the class's, not the weights'");
    }

    /// **The rows are the refuter's coordinates, not this builder's opinion.**
    ///
    /// An inventory whose rows disagree with what `palw_step_refute` asks for serves nothing: the
    /// oracle finds an operand by `(tensor, layer, byte_offset)` and then requires the length to
    /// match exactly, so a row at the wrong offset or the wrong width is a step that adjudicates
    /// `Unadjudicable` — coverage-clean and unprosecutable.
    #[test]
    fn every_row_is_addressable_at_the_coordinates_the_court_asks_for() {
        use kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1;
        use kaspa_consensus_core::palw_step_refute::PalwWeightOracleV1;

        let artifact = rc_artifact();
        let geometry = PALW_RC_BASE0_GEOMETRY;
        let inventory = base0_inventory_v1(&artifact, geometry).unwrap();
        let root = inventory.root();
        let d = artifact.shape.d_model();
        let tile = geometry.tile_len as usize;
        // Prove the rows this test asks for — through the real prover, against the real root, into
        // the real oracle — so it exercises the path a close takes rather than a lookup beside it.
        // One opening per queried row rather than the whole inventory: opening all of them is
        // quadratic in the leaf count and proves nothing the sample does not.
        let wanted = [
            ("token_embd.weight", None, (7 * d) as u32),
            ("blk.{layer}.attn_q.weight", Some(2u16), (tile * d) as u32),
            ("blk.{layer}.qk_to_code.requant", Some(0), 0),
            ("blk.{layer}.attn_logit.scale", Some(1), 0),
            ("blk.{layer}.rope_table", Some(3), 0),
            ("blk.{layer}.attn_q.weight", Some(0), 0),
        ];
        let openings: Vec<_> = wanted
            .iter()
            .map(|(name, layer, start)| {
                let index = inventory
                    .operands()
                    .iter()
                    .position(|o| o.tensor_name == *name && o.layer == *layer && o.row_start == *start)
                    .unwrap_or_else(|| panic!("{name} at {start} is not in the inventory"));
                kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(inventory.operands(), index as u32).unwrap()
            })
            .collect();
        let proven = PalwProvenOperandsV1::from_openings_v1(&openings, root).expect("every row proves against its own root");
        // An embedding lookup: `(token · width, width)`.
        assert_eq!(proven.operand_bytes("token_embd.weight", None, (7 * d) as u32, d as u32).map(|b| b.len()), Some(d));
        // A matmul tile: `(tile · tile_len · in_dim, tile_width · in_dim)`.
        assert_eq!(
            proven.operand_bytes("blk.{layer}.attn_q.weight", Some(2), (tile * d) as u32, (tile * d) as u32).map(|b| b.len()),
            Some(tile * d)
        );
        // A uniform narrowing: nine bytes at offset zero.
        assert_eq!(proven.operand_bytes("blk.{layer}.qk_to_code.requant", Some(0), 0, 9).map(|b| b.len()), Some(9));
        // A rescale: five bytes at offset zero.
        assert_eq!(proven.operand_bytes("blk.{layer}.attn_logit.scale", Some(1), 0, 5).map(|b| b.len()), Some(5));
        // One rotary position: cos then sin, four bytes each, one pair per two lanes.
        let pairs = artifact.shape.d_head / 2;
        assert_eq!(proven.operand_bytes("blk.{layer}.rope_table", Some(3), 0, (8 * pairs) as u32).map(|b| b.len()), Some(8 * pairs));

        // And the negative: a byte range nobody committed is not served, which is what makes an
        // opening's absence a fact rather than a gap.
        assert_eq!(proven.operand_bytes("blk.{layer}.attn_q.weight", Some(0), 1, d as u32), None);
        assert_eq!(
            proven.operand_bytes("blk.{layer}.attn_norm.weight", Some(0), 0, 9),
            None,
            "the phantom gain is gone from both sides"
        );
    }

    /// The builder refuses a geometry that is not this artifact's, so an inventory can never
    /// describe a layout for a class the weights do not belong to.
    #[test]
    fn a_foreign_geometry_yields_no_inventory() {
        let artifact = rc_artifact();
        let foreign = PalwBase0GeometryV1 { ffn_dim: PALW_RC_BASE0_GEOMETRY.ffn_dim * 2, ..PALW_RC_BASE0_GEOMETRY };
        assert!(matches!(base0_inventory_v1(&artifact, foreign), Err(InventoryBuildError::Geometry(_))));
        assert!(matches!(
            base0_inventory_v1(&artifact, PalwBase0GeometryV1 { tile_len: 0, ..PALW_RC_BASE0_GEOMETRY }),
            Err(InventoryBuildError::ZeroTile)
        ));
    }
}
