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

use kaspa_consensus_core::palw_artifact::{
    PalwArtifactInventoryDigestV1, PalwArtifactInventoryStreamV1, PalwArtifactInventorySummaryV1, PalwArtifactInventoryV1,
    PalwArtifactOperandV1, PalwArtifactRowDigestV1, PalwInventoryError, artifact_leaf_parts_v1,
};
use kaspa_consensus_core::palw_shard_plan_v1::PalwInventoryRowMetaV1;
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

/// **Does this hybrid graph register its operand-inventory root?** (ADR-0102.) Exactly when it
/// reads the embedding lift per token — `graph-v6`, the one hybrid graph whose inventory serves
/// every store the engine executes, so the one whose court can open what it reads. The rows before
/// it (graph-v1, graph-v3) register the root computed over the mapping, exactly as the chain
/// registered them; the ONE spelling the SDK's pairing and both of its resolution arms read.
pub fn qwen36_registers_inventory_root_v1(profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3) -> bool {
    let by_token =
        kaspa_consensus_core::palw_step::kernel_semantics_id_v1(kaspa_consensus_core::palw_step_refute::KDESC_A16_REQUANTIZE_BY_TOKEN);
    profile.pre_nodes.iter().any(|n| n.kernel_semantics_id == by_token)
}

/// **Every operand row a Qwen3.6-family execution can open, for one artifact under one
/// registered profile** — the hybrid tier's answer to [`a16_inventory_v1`], and the other half
/// of the court-side parameter conventions `palw_step_refute`'s shared arms encode.
///
/// The rows are [`qwen36_visit_inventory_rows_v1`]'s, each one's bytes kept. The same emitter with
/// a sink that keeps each row's LEAF is [`qwen36_inventory_digest_v1`], and with one that keeps
/// nothing per row, [`qwen36_inventory_summary_v1`] — one layout, three sinks (ADR-0103), so the
/// materialized and the streamed inventory cannot be two descriptions.
pub fn qwen36_inventory_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<PalwArtifactInventoryV1, InventoryBuildError> {
    let mut rows: Vec<PalwArtifactOperandV1> = Vec::new();
    qwen36_visit_inventory_rows_v1(artifact, profile, &mut |name, layer, row_start, bytes| {
        rows.push(PalwArtifactOperandV1 { tensor_name: name.to_string(), layer, row_start, bytes: bytes.to_vec() });
        Ok(())
    })?;
    PalwArtifactInventoryV1::new(rows).map_err(InventoryBuildError::NotCanonical)
}

/// **The same inventory as LEAVES** (ADR-0103): every row [`qwen36_inventory_v1`] holds, hashed
/// where it was read and dropped — a coordinate, a length and a leaf per row. What an opening is
/// built from without holding the bytes it opens ([`PalwArtifactInventoryDigestV1::opening_v1`]).
pub fn qwen36_inventory_digest_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<PalwArtifactInventoryDigestV1, InventoryBuildError> {
    let mut rows: Vec<PalwArtifactRowDigestV1> = Vec::new();
    qwen36_visit_inventory_rows_v1(artifact, profile, &mut |name, layer, row_start, bytes| {
        rows.push(PalwArtifactRowDigestV1 {
            tensor_name: name.to_string(),
            layer,
            row_start,
            byte_len: bytes.len() as u32,
            leaf_hash: artifact_leaf_parts_v1(name, layer, row_start, bytes),
        });
        Ok(())
    })?;
    PalwArtifactInventoryDigestV1::new(rows).map_err(InventoryBuildError::NotCanonical)
}

/// **Root, leaf count and bytes, with nothing kept per row** (ADR-0103): the rows arrive in
/// canonical order, so each is checked, hashed into the Merkle frontier and dropped. Holds one read
/// block and the frontier's `log₂ n` peaks, whatever the artifact's size.
pub fn qwen36_inventory_summary_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<PalwArtifactInventorySummaryV1, InventoryBuildError> {
    Ok(qwen36_inventory_measure_v1(artifact, profile)?.0)
}

/// **The summary and each tensor's bytes** — what a measurement reads: the placement of an
/// inventory's bytes (`palw_artifact_bytes_from_inventory_v1`) reads a row's tensor, layer and
/// length only, so one entry per tensor, its rows summed, places exactly what its rows would.
/// Per TENSOR, not per row: the hundred-thousand tensors of a 35B mixture, not its fourteen million
/// leaves.
pub fn qwen36_inventory_measure_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<(PalwArtifactInventorySummaryV1, Vec<PalwInventoryRowMetaV1>), InventoryBuildError> {
    let mut stream = PalwArtifactInventoryStreamV1::new();
    let mut tensors: Vec<PalwInventoryRowMetaV1> = Vec::new();
    qwen36_visit_inventory_rows_v1(artifact, profile, &mut |name, layer, row_start, bytes| {
        stream.push(name, layer, row_start, bytes).map_err(InventoryBuildError::NotCanonical)?;
        match tensors.last_mut() {
            Some(t) if t.tensor_name == name && t.layer == layer => t.bytes += bytes.len() as u64,
            _ => tensors.push(PalwInventoryRowMetaV1 { tensor_name: name.to_string(), layer, bytes: bytes.len() as u64 }),
        }
        Ok(())
    })?;
    Ok((stream.finish().map_err(InventoryBuildError::NotCanonical)?, tensors))
}

/// Where the emitter hands a row: `(tensor, layer, byte offset, bytes)`, the bytes borrowed for the
/// call only; a sink's refusal stops the pass.
type Qwen36RowSinkV1<'s> = dyn FnMut(&str, Option<u16>, u32, &[u8]) -> Result<(), InventoryBuildError> + 's;

/// One read: large enough that a whole-artifact pass is read-bound, and a constant — the emitter's
/// scratch is this or one row, whichever is larger, whatever the artifact's size.
const QWEN36_INVENTORY_READ_BLOCK: usize = 16 << 20;

fn q36_missing(name: &str) -> InventoryBuildError {
    InventoryBuildError::Operand(OperandError::UnknownTensor { name: name.to_string() })
}

/// **A parameter table the way the engine READS it, never widened into a copy** (ADR-0103): a table
/// as wide as its reader rides verbatim, a singleton answers every lane, and a head-tiled table
/// repeats its period — the three rules the builder once applied by materializing the table.
struct Q36ParamViewV1 {
    rows: Vec<kaspa_consensus_core::palw_base0_a16::A16QuantParams>,
    period: usize,
    width: usize,
}

impl Q36ParamViewV1 {
    fn get(&self, lane: usize) -> kaspa_consensus_core::palw_base0_a16::A16QuantParams {
        if self.rows.len() == 1 { self.rows[0] } else { self.rows[lane % self.period] }
    }
}

/// How a planned parameter tensor is read: per lane at `width` (a singleton tiled), per head
/// (`period` lanes repeated to `width`), or per token (a singleton tiled across the vocabulary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Q36ViewRuleV1 {
    Lanes { width: usize },
    HeadTiled { period: usize, width: usize },
    Tokens { vocab: usize },
}

/// The engine's own widening rules, checked: a singleton tiles across the width, a full table rides
/// verbatim, anything else is a store this class cannot serve per lane.
fn q36_view(
    rows: Vec<kaspa_consensus_core::palw_base0_a16::A16QuantParams>,
    rule: Q36ViewRuleV1,
    name: &str,
) -> Result<Q36ParamViewV1, InventoryBuildError> {
    let (period, width) = match rule {
        Q36ViewRuleV1::Lanes { width } => (width, width),
        Q36ViewRuleV1::HeadTiled { period, width } => (period, width),
        Q36ViewRuleV1::Tokens { vocab } => {
            if rows.len() == vocab || rows.len() == 1 {
                return Ok(Q36ParamViewV1 { rows, period: vocab, width: vocab });
            }
            return Err(q36_missing(&format!(
                "{name}: {} triples serve neither one token nor each of the {vocab} vocabulary rows",
                rows.len()
            )));
        }
    };
    if rows.len() == period || rows.len() == 1 {
        Ok(Q36ParamViewV1 { rows, period, width })
    } else {
        Err(q36_missing(&format!("{name}: {} triples serve neither one lane nor {period}", rows.len())))
    }
}

/// **How one tensor's rows are produced** — planned (and every refusal raised) before a byte is
/// read, so the rows can then be read tensor by tensor in canonical order. Rows are a function of
/// the plan and the artifact, so two EQUAL plans of one tensor are one set of rows.
#[derive(Debug, PartialEq, Eq)]
enum Q36RowsV1 {
    /// A stored tensor's first `total` bytes as rows of `stride` (the last may be short).
    Stored { store: String, total: usize, stride: usize },
    /// `total` zero bytes as rows of `stride` — the exponents an exponent-less store applies.
    Zeros { total: usize, stride: usize },
    /// A parameter store read through its view, as rows of `chunk` lanes.
    Params { stored: String, rule: Q36ViewRuleV1, chunk: usize },
    /// A few rows computed while planning: a registered triple, a softmax byte, a decay or
    /// recurrence row — each a handful of bytes.
    Small(Vec<(u32, Vec<u8>)>),
    /// The rotation table's rows, one per position, at `pairs` of the table's `table_pairs`.
    Rope { pairs: usize, table_pairs: usize },
}

struct Q36PlannedTensorV1 {
    name: String,
    layer: Option<u16>,
    rows: Q36RowsV1,
}

/// **The emitter's only state: one scratch buffer and the size of one read** (ADR-0103). Every
/// row it hands over is a slice of `buf` (or of a planned row), so what the emitter holds is one
/// read block or one row — never a tensor, whatever the artifact's size.
struct Q36RowReaderV1<'a> {
    artifact: &'a crate::qwen36::Qwen36ArtifactV1,
    buf: Vec<u8>,
    block: usize,
}

impl Q36RowReaderV1<'_> {
    /// Every row of one planned tensor, in ascending offset, into `sink`.
    fn stream(&mut self, planned: &Q36PlannedTensorV1, sink: &mut Qwen36RowSinkV1<'_>) -> Result<(), InventoryBuildError> {
        let (name, layer) = (planned.name.as_str(), planned.layer);
        match &planned.rows {
            Q36RowsV1::Stored { store, total, stride } => {
                // Read a block of whole rows at a time: every block starts on a row, so the rows are
                // exactly a slicing of the tensor whatever the block.
                let stride = (*stride).max(1);
                let per_block = (self.block / stride).max(1) * stride;
                let mut at = 0usize;
                while at < *total {
                    let take = per_block.min(total - at);
                    self.buf.clear();
                    self.buf.resize(take, 0);
                    self.artifact.read_tensor_range_into(store, at, &mut self.buf).map_err(|e| q36_missing(&e))?;
                    let mut offset = 0usize;
                    while offset < take {
                        let end = (offset + stride).min(take);
                        sink(name, layer, (at + offset) as u32, &self.buf[offset..end])?;
                        offset = end;
                    }
                    at += take;
                }
            }
            Q36RowsV1::Zeros { total, stride } => {
                let stride = (*stride).max(1);
                self.buf.clear();
                self.buf.resize(stride.min(*total), 0);
                let mut at = 0usize;
                while at < *total {
                    let end = (at + stride).min(*total);
                    sink(name, layer, at as u32, &self.buf[..end - at])?;
                    at = end;
                }
            }
            Q36RowsV1::Params { stored, rule, chunk } => {
                // Row `k` is lanes `[k·chunk, (k+1)·chunk)` at byte `k·chunk·17`, the chunking the
                // arms read; each row's wire bytes are written as it is emitted.
                let w = kaspa_consensus_core::palw_base0_a16::A16QuantParams::WIRE_BYTES;
                let view = q36_view(self.artifact.param_rows(stored).map_err(|_| q36_missing(stored))?, *rule, stored)?;
                let chunk = (*chunk).max(1);
                let mut lane = 0usize;
                while lane < view.width {
                    let end = (lane + chunk).min(view.width);
                    self.buf.clear();
                    for l in lane..end {
                        self.buf.extend_from_slice(&view.get(l).to_wire());
                    }
                    sink(name, layer, (lane * w) as u32, &self.buf)?;
                    lane = end;
                }
            }
            Q36RowsV1::Small(rows) => {
                for (start, bytes) in rows {
                    sink(name, layer, *start, bytes)?;
                }
            }
            Q36RowsV1::Rope { pairs, table_pairs } => {
                // One row per position: `cos` then `sin` at the class's ROTARY width — the slice of
                // the pinned table the partial rotation reads.
                let rope = &self.artifact.rope;
                let mut offset = 0u32;
                for position in 0..self.artifact.shape.max_position {
                    let start = position * table_pairs;
                    self.buf.clear();
                    for v in &rope.cos_q[start..start + pairs] {
                        self.buf.extend_from_slice(&v.to_le_bytes());
                    }
                    for v in &rope.sin_q[start..start + pairs] {
                        self.buf.extend_from_slice(&v.to_le_bytes());
                    }
                    sink(name, layer, offset, &self.buf)?;
                    offset += self.buf.len() as u32;
                }
            }
        }
        Ok(())
    }
}

/// **ADR-0103: the ONE place a Qwen3.6-family inventory's rows are laid out**, handed to `sink` in
/// CANONICAL order — by `(tensor, layer, byte offset)`, each coordinate once — as they are read.
///
/// Two passes over the graph, one over the bytes. The plan walks the profile slot by slot and
/// raises every refusal the layout has, in slot order, before a byte is read; then the planned
/// tensors are read in canonical order, each a block at a time through
/// [`read_tensor_range_into`](crate::qwen36::Qwen36ArtifactV1::read_tensor_range_into) (the file
/// descriptor, never the mapping, on a mapped store), a parameter table through the engine's own
/// reading rather than a widened copy. Where two slots plan one tensor, its rows are merged with the
/// first-planned row of an offset kept — the builder's old "first pushed wins", restated per tensor.
/// So a sink can check, hash and drop each row as it arrives: the root is a stream.
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
pub fn qwen36_visit_inventory_rows_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    sink: &mut Qwen36RowSinkV1<'_>,
) -> Result<(), InventoryBuildError> {
    qwen36_visit_inventory_rows_in_blocks_v1(artifact, profile, QWEN36_INVENTORY_READ_BLOCK, sink)
}

/// The emitter at a chosen read size — which must not matter, and a test holds it to that at sizes
/// down to one byte.
fn qwen36_visit_inventory_rows_in_blocks_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
    block: usize,
    sink: &mut Qwen36RowSinkV1<'_>,
) -> Result<(), InventoryBuildError> {
    let plan = qwen36_inventory_plan_v1(artifact, profile)?;
    qwen36_emit_plan_v1(artifact, &plan, block, sink)
}

/// The plan's rows, in canonical order.
fn qwen36_emit_plan_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    plan: &[Q36PlannedTensorV1],
    block: usize,
    sink: &mut Qwen36RowSinkV1<'_>,
) -> Result<(), InventoryBuildError> {
    // A STABLE sort: tensors planned by several slots keep their slot order, which is what the
    // first-planned-wins merge below reads.
    let mut order: Vec<usize> = (0..plan.len()).collect();
    order.sort_by(|&a, &b| (plan[a].name.as_str(), plan[a].layer).cmp(&(plan[b].name.as_str(), plan[b].layer)));
    let mut rd = Q36RowReaderV1 { artifact, buf: Vec::new(), block: block.max(1) };
    let mut i = 0usize;
    while i < order.len() {
        let key = (plan[order[i]].name.as_str(), plan[order[i]].layer);
        let mut j = i + 1;
        while j < order.len() && (plan[order[j]].name.as_str(), plan[order[j]].layer) == key {
            j += 1;
        }
        // A plan equal to an earlier one of the same tensor yields the same rows, every one of
        // which the earlier plan wins: it adds nothing. (Both rotations of a full-attention layer
        // plan its one table and its clamp — the case every hybrid graph has.)
        let mut distinct: Vec<usize> = Vec::with_capacity(j - i);
        for &k in &order[i..j] {
            if !distinct.iter().any(|&d| plan[d].rows == plan[k].rows) {
                distinct.push(k);
            }
        }
        if let [only] = distinct[..] {
            rd.stream(&plan[only], sink)?;
        } else {
            // One tensor, several different plans: its rows merged by offset, the first-planned
            // kept — held for this one tensor only. No shipped graph reaches this arm.
            let mut rows: Vec<(u32, Vec<u8>)> = Vec::new();
            for &k in &distinct {
                rd.stream(&plan[k], &mut |_, _, start, bytes| {
                    rows.push((start, bytes.to_vec()));
                    Ok(())
                })?;
            }
            rows.sort_by_key(|(start, _)| *start);
            rows.dedup_by(|later, earlier| later.0 == earlier.0);
            for (start, bytes) in &rows {
                sink(key.0, key.1, *start, bytes)?;
            }
        }
        i = j;
    }
    Ok(())
}

/// **The plan: every tensor the inventory holds and how its rows are produced**, walked slot by
/// slot with every refusal raised in slot order — the builder's checks, in the builder's order,
/// before any byte is read.
fn qwen36_inventory_plan_v1(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
) -> Result<Vec<Q36PlannedTensorV1>, InventoryBuildError> {
    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    use kaspa_consensus_core::palw_qwen36_ops::QWEN36_WEIGHT_GROUP;
    use kaspa_consensus_core::palw_step::PalwStepOutLenV1;
    use kaspa_consensus_core::palw_step::kernel_semantics_id_v1 as kid;
    use kaspa_consensus_core::palw_step_refute as kd;

    let w = A16QuantParams::WIRE_BYTES;
    let missing = q36_missing;
    let sub = |name: &str, layer: Option<u16>| -> String {
        match layer {
            Some(l) => name.replace("{layer}", &l.to_string()),
            None => name.to_string(),
        }
    };
    let param_rows =
        |name: &str| -> Result<Vec<A16QuantParams>, InventoryBuildError> { artifact.param_rows(name).map_err(|_| missing(name)) };
    let wire = |rows: &[A16QuantParams]| -> Vec<u8> { rows.iter().flat_map(|p| p.to_wire()).collect() };
    // The engine's per-head read (`rows[vh.min(len - 1)]`), one value head at a time.
    let per_head = |rows: &[A16QuantParams], vh: usize| -> A16QuantParams { rows[vh.min(rows.len().saturating_sub(1))] };
    let mut plan: Vec<Q36PlannedTensorV1> = Vec::new();
    let mut add = |name: &str, layer: Option<u16>, rows: Q36RowsV1| plan.push(Q36PlannedTensorV1 { name: name.to_string(), layer, rows });
    // A parameter tensor read through a view: checked now, read when its turn comes.
    let params = |stored: String, rule: Q36ViewRuleV1, chunk: usize, what: &str| -> Result<Q36RowsV1, InventoryBuildError> {
        q36_view(param_rows(&stored)?, rule, what)?;
        Ok(Q36RowsV1::Params { stored, rule, chunk })
    };
    // One registered triple at offset zero — the sites whose lane counts are the job's, or whose
    // parameter is a single scalar.
    let one_triple = |name: &str, layer: Option<u16>| -> Result<Q36RowsV1, InventoryBuildError> {
        let store = param_rows(&sub(name, layer))?;
        if store.len() != 1 {
            return Err(missing(&format!("{name}: this site registers exactly one triple")));
        }
        Ok(Q36RowsV1::Small(vec![(0, wire(&store))]))
    };
    // A per-expert grouped-projection store: codes, exponents (zeros where the artifact carries
    // none) and per-row triples, chunked at the node's tile restarting at the expert's byte 0.
    let projection =
        |template: &str, store: &str, block_rows: usize, tile: usize| -> Result<[(String, Q36RowsV1); 3], InventoryBuildError> {
            let len = artifact.tensor_len(store).map_err(|_| missing(store))?;
            if block_rows == 0 || !len.is_multiple_of(block_rows) {
                return Err(missing(&format!("{store}: {len} code bytes over {block_rows} rows")));
            }
            let in_dim = len / block_rows;
            let groups = in_dim.div_ceil(QWEN36_WEIGHT_GROUP);
            let chunk = tile.min(block_rows).max(1);
            let codes = Q36RowsV1::Stored { store: store.to_string(), total: len, stride: chunk * in_dim.max(1) };
            let exp_name = format!("{store}.exp");
            let exps = match artifact.tensor_len(&exp_name) {
                Ok(n) => {
                    if n != block_rows * groups {
                        return Err(missing(&format!("{exp_name}: {n} exponents over {block_rows}x{groups}")));
                    }
                    Q36RowsV1::Stored { store: exp_name, total: n, stride: chunk * groups.max(1) }
                }
                // Absent means every group's exponent is zero — the arithmetic the engine's
                // exponent-less dispatch performs, committed as the bytes it is.
                Err(_) => Q36RowsV1::Zeros { total: block_rows * groups, stride: chunk * groups.max(1) },
            };
            let triples = params(format!("{store}.a16"), Q36ViewRuleV1::Lanes { width: block_rows }, chunk, store)?;
            Ok([(template.to_string(), codes), (format!("{template}.exp"), exps), (format!("{template}.a16"), triples)])
        };

    let k_embed = kid(kd::KDESC_A16_EMBED);
    let k_req = kid(kd::KDESC_A16_REQUANTIZE);
    let k_req_by_token = kid(kd::KDESC_A16_REQUANTIZE_BY_TOKEN);
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

    // How many routed experts this layer's artifact stores, by the store's own naming.
    let expert_count = |layer: u16, suffix: &str| -> usize {
        let mut e = 0usize;
        while e < 65_536 && artifact.tensor_len(&format!("blk.{layer}.ffn_expert.{e}{suffix}")).is_ok() {
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
            if width == 0 {
                return Err(missing(&format!("{name}: a zero-width embedding row")));
            }
            let store = sub(name, layer);
            let len = artifact.tensor_len(&store).map_err(|_| missing(name))?;
            // One row per token id; a trailing partial row is no token's and was never a row.
            add(name, layer, Q36RowsV1::Stored { store, total: len / width * width, stride: width });
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
                    let template = format!("{prefix}ffn_expert.{e}{suffix}");
                    for (tensor, rows) in projection(&template, &format!("blk.{l}.ffn_expert.{e}{suffix}"), block_rows, tile)? {
                        add(&tensor, layer, rows);
                    }
                }
            } else {
                let out_dim = fixed.ok_or_else(|| missing(name))?;
                for (tensor, rows) in projection(name, &sub(name, layer), out_dim, tile)? {
                    add(&tensor, layer, rows);
                }
            }
        } else if kidv == k_req_by_token {
            // **ADR-0102: one row per TOKEN.** The court opens the triple at the position's
            // token's offset — `(token × 17, 17)` — so each vocabulary row of the store is its own
            // leaf, and an opening proves exactly the one triple the lift applied. The rows are
            // the ENGINE's reading (`qwen36.rs`: a one-row store lifts every token by that row, a
            // longer one is indexed by the token): a calibrated store rides verbatim, a singleton
            // tiles across the vocabulary, and anything else is refused by name.
            let vocab = profile.vocab_size as usize;
            add(name, layer, params(sub(name, layer), Q36ViewRuleV1::Tokens { vocab }, 1, name)?);
        } else if kidv == k_req {
            match fixed {
                Some(width) => {
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
                    let rule = match head_tiled {
                        Some(heads) if heads > 0 && width.is_multiple_of(heads) => Q36ViewRuleV1::HeadTiled { period: width / heads, width },
                        _ => Q36ViewRuleV1::Lanes { width },
                    };
                    add(name, layer, params(sub(name, layer), rule, tile, name)?);
                }
                // The probs: one registered triple, tiled by the kernel at the job's width.
                None => add(name, layer, one_triple(name, layer)?),
            }
        } else if kidv == k_rescale_row {
            let width = fixed.ok_or_else(|| missing(name))?;
            add(name, layer, params(sub(name, layer), Q36ViewRuleV1::Lanes { width }, tile, name)?);
        } else if kidv == k_rms_wide {
            // Per value head, ONE triple per row: the arm reads head `vh`'s eps at offset
            // `vh · wire`, one at a time.
            let width = fixed.ok_or_else(|| missing(name))?;
            let hd = (profile.gdn_head_v_dim as usize).max(1);
            if !width.is_multiple_of(hd) {
                return Err(missing(&format!("{name}: the wide-norm row is not a whole number of heads")));
            }
            let store = param_rows(&sub(name, layer))?;
            add(name, layer, Q36RowsV1::Small((0..width / hd).map(|vh| ((vh * w) as u32, per_head(&store, vh).to_wire().to_vec())).collect()));
        } else if kidv == k_mul_wide {
            if name.ends_with(".ffn_expert_gated.a16") {
                let l = layer.ok_or_else(|| missing(name))?;
                let prefix = name.rfind("ffn_").map(|i| &name[..i]).ok_or_else(|| missing(name))?;
                let block_rows = profile.ffn_dim as usize;
                let chunk = tile.min(block_rows).max(1);
                let experts = expert_count(l, "_gate.weight");
                for e in 0..experts {
                    for stage in ["_silu.a16", "_gated.a16"] {
                        let rows = params(format!("blk.{l}.ffn_expert.{e}{stage}"), Q36ViewRuleV1::Lanes { width: block_rows }, chunk, name)?;
                        add(&format!("{prefix}ffn_expert.{e}{stage}"), layer, rows);
                    }
                }
            } else if name.ends_with(".ffn_shared_expert_gated.a16") {
                let width = fixed.ok_or_else(|| missing(name))?;
                let stem = name.strip_suffix("ffn_shared_expert_gated.a16").ok_or_else(|| missing(name))?;
                for stage in ["ffn_shared_expert_silu.a16", "ffn_shared_expert_gated.a16"] {
                    let template = format!("{stem}{stage}");
                    let rows = params(sub(&template, layer), Q36ViewRuleV1::Lanes { width }, tile, &template)?;
                    add(&template, layer, rows);
                }
            } else if name.ends_with(".ffn_shared_gated.a16") {
                add(name, layer, one_triple(name, layer)?);
            } else {
                let width = fixed.ok_or_else(|| missing(name))?;
                add(name, layer, params(sub(name, layer), Q36ViewRuleV1::Lanes { width }, tile, name)?);
            }
        } else if kidv == k_gate_apply || kidv == k_scores || kidv == k_values || kidv == k_topk || kidv == k_combine {
            add(name, layer, one_triple(name, layer)?);
        } else if kidv == k_soft {
            // ONE raw byte — the widening the engine reads through `scalar()`, at the domain the
            // op clamps to.
            let store = param_rows(&sub(name, layer))?;
            if store.len() != 1 {
                return Err(missing(&format!("{name}: the softmax widening is one registered scalar")));
            }
            add(name, layer, Q36RowsV1::Small(vec![(0, vec![store[0].zero.clamp(0, 62) as u8])]));
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
            add(&t.softmax_up, layer, Q36RowsV1::Small(vec![(0, vec![store[0].zero.clamp(0, 62) as u8])]));
            for triple in [&t.scores, &t.probs, &t.values] {
                add(triple, layer, one_triple(triple, layer)?);
            }
        } else if kidv == k_decay {
            let width = fixed.ok_or_else(|| missing(name))?;
            let stem = name.strip_suffix("linear_decay.a16").ok_or_else(|| missing(name))?;
            for store in ["linear_decay_c.a16", "linear_dt_bias.a16"] {
                let template = format!("{stem}{store}");
                let table = param_rows(&sub(&template, layer))?;
                let effective: Vec<A16QuantParams> = (0..width).map(|vh| per_head(&table, vh)).collect();
                add(&template, layer, Q36RowsV1::Small(vec![(0, wire(&effective))]));
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
            let rows = (0..heads)
                .map(|vh| {
                    let mut bytes = Vec::with_capacity(4 * w);
                    for store in &stores {
                        bytes.extend_from_slice(&per_head(store, vh).to_wire());
                    }
                    ((vh * 4 * w) as u32, bytes)
                })
                .collect();
            add(name, layer, Q36RowsV1::Small(rows));
        } else if kidv == k_conv {
            let width = fixed.ok_or_else(|| missing(name))?;
            let store = sub(name, layer);
            let taps = artifact.tensor_len(&store).map_err(|_| missing(name))?;
            if taps != 4 * width {
                return Err(missing(&format!("{name}: {taps} taps over {width} channels")));
            }
            add(name, layer, Q36RowsV1::Stored { store, total: taps, stride: tile.max(1) * 4 });
            let stem = name.strip_suffix(".weight").ok_or_else(|| missing(name))?;
            let template = format!("{stem}.a16");
            let rows = params(sub(&template, layer), Q36ViewRuleV1::Lanes { width }, tile, &template)?;
            add(&template, layer, rows);
        } else if kidv == k_rope {
            let pairs = (profile.rope_dims as usize) / 2;
            let table_pairs = artifact.rope.d_head / 2;
            if pairs == 0 || pairs > table_pairs {
                return Err(missing(&format!("{name}: a {pairs}-pair rotation over a {table_pairs}-pair table")));
            }
            add(name, layer, Q36RowsV1::Rope { pairs, table_pairs });
            // The clamp triple, under the suffix that keeps it from colliding with the table's
            // byte 0 — the store's own row is the node's bare name.
            let store = param_rows(&sub(name, layer))?;
            if store.len() != 1 {
                return Err(missing(&format!("{name}: the rotation registers exactly one clamp triple")));
            }
            add(&format!("{name}.clamp"), layer, Q36RowsV1::Small(vec![(0, wire(&store))]));
        } else {
            return Err(missing(&format!("{name}: kernel this inventory does not lay out")));
        }
    }
    Ok(plan)
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

/// **ADR-0082 Decision 1's strongest claim, checked directly: fusing the attention site changes
/// the GRAPH and not the ARTIFACT.**
#[cfg(test)]
mod graph_v5 {
    use crate::artifact::{Base0ArtifactV1, Base0ShapeV1, LN_THETA_10000_GEN_Q};
    use crate::engine_a16::derived_a16_store;
    use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, qwen25_a16_profile_v2, qwen25_a16_profile_v5};
    use kaspa_consensus_core::palw_step::PalwStepOpKindV1;
    use kaspa_consensus_core::palw_step_refute::palw_attn_fused_tensors_v1;

    /// **The v5 inventory is the v2 inventory, row for row, and therefore root for root.**
    ///
    /// The four nodes the fusion replaces contribute exactly four operand rows between them — the
    /// scores triple, the probability triple, the values triple and the softmax's widening byte —
    /// and the fused node contributes the SAME four, derived from the one name it carries. So a
    /// v5 class registers the identical `artifact_root` a v2 class does, which is what "the
    /// artifact is UNCHANGED" means in bytes rather than in prose: an operator converts nothing,
    /// re-hashes nothing, and downloads nothing new to run graph v5.
    ///
    /// It is also the tightest available statement that the derivation is right. A wrong prefix, a
    /// wrong suffix or a missing operand would each show up here as a row the v2 inventory has and
    /// the v5 one does not — before any dispute, and without an oracle.
    #[test]
    fn the_v5_inventory_is_the_v2_inventory_and_names_every_derived_tensor() {
        let geometry = PalwQwen25GeometryV1 {
            layer_count: 2,
            hidden_dim: 16,
            ffn_dim: 12,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 4,
            vocab_size: 64,
            n_ctx: 16,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 4,
        };
        let shape = Base0ShapeV1 {
            n_layers: 2,
            n_heads: 4,
            n_kv_heads: 2,
            d_head: 4,
            d_ff: 12,
            vocab: 64,
            max_position: 32,
            ln_theta_gen_q: LN_THETA_10000_GEN_Q,
            eps_q: 1,
        };
        let artifact = Base0ArtifactV1::derive_deterministic(shape, 0xF022)
            .expect("a valid shape")
            .with_a16_params(derived_a16_store(&shape))
            .expect("sorted and unique");
        let v2 = qwen25_a16_profile_v2(geometry).expect("the v2 row projects");
        let v5 = qwen25_a16_profile_v5(geometry).expect("the v5 row projects");

        let inv_v2 = super::a16_inventory_v1(&artifact, &v2).expect("the v2 inventory builds");
        let inv_v5 = super::a16_inventory_v1(&artifact, &v5).expect("the v5 inventory builds");
        assert_eq!(inv_v5.operands(), inv_v2.operands(), "graph v5 implies a different artifact, which it must not");
        assert_eq!(inv_v5.root(), inv_v2.root(), "and therefore a different artifact_root");

        // …and every tensor the fused node derives is one of those rows, at every layer.
        let fused = v5.attn_nodes.iter().find(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
        let t = palw_attn_fused_tensors_v1(fused.weight_name.as_str()).expect("the site's operands derive");
        for layer in 0..geometry.layer_count {
            for name in [&t.softmax_up, &t.scores, &t.probs, &t.values] {
                assert!(
                    inv_v5.operands().iter().any(|o| o.tensor_name == *name && o.layer == Some(layer)),
                    "the fused site reads {name:?} at layer {layer}, and no inventory row can open it"
                );
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod adr0103_cases {
    //! The fixture cases ADR-0103's equality is held on — each an artifact and a profile the
    //! Qwen3.6 builder serves (or refuses by name).
    use kaspa_consensus_core::palw_base0_a16::A16QuantParams;
    use kaspa_consensus_core::palw_qwen36_profile::{
        PalwQwen36GeometryV1, qwen36_artifact_row_profile_v6, qwen36_profile_v1, qwen36_profile_v2, qwen36_profile_v5, qwen36_profile_v6,
    };
    use kaspa_consensus_core::palw_step::PalwShapeProfileV3;

    use crate::qwen36::Qwen36ArtifactV1;

    /// `(case, artifact, profile)`; a profile the geometry does not build is skipped by name.
    pub(crate) fn cases() -> Vec<(String, std::sync::Arc<Qwen36ArtifactV1>, PalwShapeProfileV3)> {
        let mut out = Vec::new();
        let base = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8));
        let g = crate::qwen36_plan::fixture_geometry_of(&base.shape, 4);
        let graphs: [(&str, fn(PalwQwen36GeometryV1) -> Result<PalwShapeProfileV3, _>); 5] = [
            ("v1", qwen36_profile_v1),
            ("v2", qwen36_profile_v2),
            ("v5", qwen36_profile_v5),
            ("v6", qwen36_profile_v6),
            ("row-v6", qwen36_artifact_row_profile_v6),
        ];
        for (graph, build) in graphs {
            if let Ok(p) = build(g) {
                out.push((format!("fixture-4x8/{graph}"), base.clone(), p));
            }
        }
        // A per-token lift, not all alike.
        let vocab = base.shape.vocab;
        let lift: Vec<A16QuantParams> =
            (0..vocab).map(|t| A16QuantParams { multiplier: 1 + (t % 3) as i64, shift: (t % 2) as u8, zero: 0 }).collect();
        let lifted = std::sync::Arc::new(crate::qwen36::test_fixture(4, 8).with_params("embed_lift.a16", &lift));
        for (graph, build) in graphs {
            if let Ok(p) = build(g) {
                out.push((format!("per-token-lift/{graph}"), lifted.clone(), p));
            }
        }
        // Group exponents on every layer-0 routed gate: the tensor-backed `.exp` path.
        let v6 = qwen36_profile_v6(g).expect("v6");
        let ffn = v6.ffn_dim as usize;
        let mut with_exp = crate::qwen36::test_fixture(4, 8);
        let mut e = 0usize;
        while let Ok(codes) = with_exp.tensor(&format!("blk.0.ffn_expert.{e}_gate.weight")) {
            let groups = (codes.len() / ffn).div_ceil(32);
            let exps: Vec<i8> = (0..ffn * groups).map(|i| ((i * 37 + e * 11) % 7) as i8 - 3).collect();
            with_exp = with_exp.with_tensor(format!("blk.0.ffn_expert.{e}_gate.weight.exp"), exps);
            e += 1;
        }
        out.push(("group-exponents/v6".to_string(), std::sync::Arc::new(with_exp), v6.clone()));
        // A wrong-length exponent table: refused by name, the same refusal from every sink.
        let bad = crate::qwen36::test_fixture(4, 8).with_tensor("blk.0.ffn_expert.0_gate.weight.exp", vec![1i8; 3]);
        out.push(("bad-exponents/v6".to_string(), std::sync::Arc::new(bad), v6));
        // The fuzz corpus's tiny class, both graphs.
        let (tiny, tiny_v5) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        let tg = PalwQwen36GeometryV1 { n_ctx: tiny_v5.n_ctx, ..crate::qwen36_plan::fixture_geometry_of(&tiny.shape, 4) };
        let tiny = std::sync::Arc::new(tiny);
        out.push(("tiny/v5".to_string(), tiny.clone(), tiny_v5));
        if let Ok(p) = qwen36_profile_v6(tg) {
            out.push(("tiny/v6".to_string(), tiny, p));
        }
        out
    }

    /// The line a case prints: what a materialized build of it yields.
    pub(crate) fn line(case: &str, artifact: &Qwen36ArtifactV1, profile: &PalwShapeProfileV3) -> String {
        match crate::inventory::qwen36_inventory_v1(artifact, profile) {
            Ok(inv) => {
                let bytes: u64 = inv.operands().iter().map(|o| o.bytes.len() as u64).sum();
                format!("{case} root={} leaves={} bytes={bytes}", inv.root(), inv.operands().len())
            }
            Err(e) => format!("{case} refused={e:?}"),
        }
    }

    /// **ADR-0103's acceptance, pinned: the emitter reproduces the materializing builder's record.**
    /// Every line below was printed by the builder BEFORE the emitter existed (the tree at
    /// `252b7b3b`), and every refusal is the same refusal by name — a root the refactor moved would
    /// be a defect, never a migration.
    #[test]
    fn the_emitter_reproduces_the_materializing_builders_record() {
        const RECORD: &[&str] = &[
            "fixture-4x8/v1 refused=Operand(UnknownTensor { name: \"blk.0.ffn_router_topk.a16\" })",
            "fixture-4x8/v2 root=7ad0621561a53fc81aeb9777111e0b0accdf0c7d34c7df0d97ca91ab02d3e7a5633e74a85bf92a40e936ef540ab94890045a24da4e8dbef82ea629d76d7d4140 leaves=738 bytes=187342",
            "fixture-4x8/v5 root=7ad0621561a53fc81aeb9777111e0b0accdf0c7d34c7df0d97ca91ab02d3e7a5633e74a85bf92a40e936ef540ab94890045a24da4e8dbef82ea629d76d7d4140 leaves=738 bytes=187342",
            "fixture-4x8/v6 root=7c7b58330a33736ab81d6f3618031cb724fa40dfcee0d26d6cd7ec2496ab24d25c694b7a6ead21bebe20a0087a727a3bf79d89a0237e8b92bef72de980763e11 leaves=801 bytes=187886",
            "fixture-4x8/row-v6 root=7c7b58330a33736ab81d6f3618031cb724fa40dfcee0d26d6cd7ec2496ab24d25c694b7a6ead21bebe20a0087a727a3bf79d89a0237e8b92bef72de980763e11 leaves=801 bytes=187886",
            "per-token-lift/v1 refused=Operand(UnknownTensor { name: \"embed_lift.a16: 64 triples serve neither one lane nor 32\" })",
            "per-token-lift/v2 refused=Operand(UnknownTensor { name: \"embed_lift.a16: 64 triples serve neither one lane nor 32\" })",
            "per-token-lift/v5 refused=Operand(UnknownTensor { name: \"embed_lift.a16: 64 triples serve neither one lane nor 32\" })",
            "per-token-lift/v6 root=60882b16d79707bc73326f9bae2a88449f75e36bd3e7d48b31054df66db65c3e6c7069f82af654d688d2585500e5f75b44b38db245fe35fe7890e7227886cd80 leaves=801 bytes=187886",
            "per-token-lift/row-v6 root=60882b16d79707bc73326f9bae2a88449f75e36bd3e7d48b31054df66db65c3e6c7069f82af654d688d2585500e5f75b44b38db245fe35fe7890e7227886cd80 leaves=801 bytes=187886",
            "group-exponents/v6 root=dc3e22f877ea6f883369a86e518173bcdd04e60898a92aacc80398687dbaa8fa543ff772047103a55ed257a7cc4a99a98abbfd5087dddab5a3dd00ac540caa29 leaves=801 bytes=187886",
            "bad-exponents/v6 refused=Operand(UnknownTensor { name: \"blk.0.ffn_expert.0_gate.weight.exp: 3 exponents over 16x1\" })",
            "tiny/v5 root=7ad0621561a53fc81aeb9777111e0b0accdf0c7d34c7df0d97ca91ab02d3e7a5633e74a85bf92a40e936ef540ab94890045a24da4e8dbef82ea629d76d7d4140 leaves=738 bytes=187342",
            "tiny/v6 root=7c7b58330a33736ab81d6f3618031cb724fa40dfcee0d26d6cd7ec2496ab24d25c694b7a6ead21bebe20a0087a727a3bf79d89a0237e8b92bef72de980763e11 leaves=801 bytes=187886",
        ];
        let lines: Vec<String> = cases().iter().map(|(case, artifact, profile)| line(case, artifact, profile)).collect();
        assert_eq!(lines, RECORD, "the case list and every line are the record's");
    }

    /// **The digest IS the inventory, on every case and at every read size** (ADR-0103 W2–W4): the
    /// same rows in the same order (each the materialized row digested), the same summary, the same
    /// refusal; read sizes down to one byte change nothing, because a block always starts on a row;
    /// and an opening built from the digest plus the row's own bytes is the materialized opening
    /// byte for byte.
    #[test]
    fn the_digest_is_the_inventory_on_every_case_and_every_read_size() {
        use kaspa_consensus_core::palw_artifact::{PalwArtifactRowDigestV1, open_artifact_leaf_v1};
        use kaspa_consensus_core::palw_shard_plan_v1::PalwInventoryRowMetaV1;
        let mut served = 0;
        for (case, artifact, profile) in cases() {
            let full = crate::inventory::qwen36_inventory_v1(&artifact, &profile);
            let digest = crate::inventory::qwen36_inventory_digest_v1(&artifact, &profile);
            let (full, digest) = match (full, digest) {
                (Ok(f), Ok(d)) => (f, d),
                (Err(f), Err(d)) => {
                    assert_eq!(f, d, "{case}: one refusal from either sink");
                    continue;
                }
                (f, d) => panic!("{case}: the sinks disagree on whether the artifact serves: {:?} / {:?}", f.err(), d.err()),
            };
            let expected: Vec<PalwArtifactRowDigestV1> = full.operands().iter().map(PalwArtifactRowDigestV1::of).collect();
            assert_eq!(digest.rows(), &expected[..], "{case}: the rows, digested, in order");
            assert_eq!(digest.summary(), full.summary(), "{case}: root, leaf count and bytes");
            let (summary, tensors) = crate::inventory::qwen36_inventory_measure_v1(&artifact, &profile).expect("the same serves");
            assert_eq!(summary, full.summary(), "{case}: the stream, with nothing kept per row");
            // Per tensor, its rows summed: placed exactly as the rows are.
            let per_row: Vec<PalwInventoryRowMetaV1> = full.operands().iter().map(PalwInventoryRowMetaV1::from).collect();
            assert_eq!(tensors.iter().map(|t| t.bytes).sum::<u64>(), summary.artifact_bytes, "{case}");
            assert_eq!(
                kaspa_consensus_core::palw_shard_plan_v1::palw_artifact_bytes_from_inventory_v1(&profile, &tensors),
                kaspa_consensus_core::palw_shard_plan_v1::palw_artifact_bytes_from_inventory_v1(&profile, &per_row),
                "{case}: one entry per tensor places what its rows place"
            );
            for block in [1usize, 7, 64, 4096] {
                let mut rows = Vec::new();
                crate::inventory::qwen36_visit_inventory_rows_in_blocks_v1(&artifact, &profile, block, &mut |name, layer, row_start, bytes| {
                    rows.push(PalwArtifactRowDigestV1 {
                        tensor_name: name.to_string(),
                        layer,
                        row_start,
                        byte_len: bytes.len() as u32,
                        leaf_hash: kaspa_consensus_core::palw_artifact::artifact_leaf_parts_v1(name, layer, row_start, bytes),
                    });
                    Ok(())
                })
                .expect("the same artifact serves at every read size");
                assert_eq!(rows, expected, "{case}: a {block}-byte read yields the same rows, already canonical");
            }
            let n = full.operands().len() as u32;
            for i in (0..n).step_by(37).chain([n - 1]) {
                let row = &full.operands()[i as usize];
                assert_eq!(
                    digest.opening_v1(i, row.bytes.clone()),
                    open_artifact_leaf_v1(full.operands(), i),
                    "{case}: leaf {i} opens the same from the digest"
                );
            }
            served += 1;
        }
        assert_eq!(served, 9, "every serving case of the record was compared, not skipped");
    }

    /// **Where two slots plan one tensor, the first-planned row of an offset is kept** — the old
    /// builder's "first pushed wins", restated per tensor — and a later, different plan still
    /// contributes the offsets the first did not reach. On the record's graphs the only tensors
    /// planned twice are a full-attention layer's rotation table and clamp (both rotations name
    /// them) with EQUAL plans, which stream once and buffer nothing; the buffered merge is pinned
    /// here on a synthetic plan because no shipped graph reaches it.
    #[test]
    fn a_tensor_planned_twice_keeps_the_first_planned_row_of_each_offset() {
        use crate::inventory::{Q36PlannedTensorV1, Q36RowsV1};
        let artifact = crate::qwen36::test_fixture(4, 8);
        let small = |name: &str, rows: Vec<(u32, Vec<u8>)>| Q36PlannedTensorV1 { name: name.to_string(), layer: None, rows: Q36RowsV1::Small(rows) };
        let plan = vec![
            small("b", vec![(0, vec![1]), (1, vec![2])]),
            small("a", vec![(0, vec![7])]),
            small("b", vec![(0, vec![9]), (1, vec![9]), (2, vec![3])]),
        ];
        let mut got = Vec::new();
        crate::inventory::qwen36_emit_plan_v1(&artifact, &plan, 1 << 20, &mut |name, _, start, bytes| {
            got.push((name.to_string(), start, bytes.to_vec()));
            Ok(())
        })
        .expect("emits");
        assert_eq!(
            got,
            vec![
                ("a".to_string(), 0, vec![7]),
                ("b".to_string(), 0, vec![1]),
                ("b".to_string(), 1, vec![2]),
                ("b".to_string(), 2, vec![3]),
            ]
        );
        for (case, artifact, profile) in cases() {
            let Ok(plan) = crate::inventory::qwen36_inventory_plan_v1(&artifact, &profile) else { continue };
            let mut first: std::collections::BTreeMap<(&str, Option<u16>), &Q36RowsV1> = std::collections::BTreeMap::new();
            let mut twice = Vec::new();
            for p in &plan {
                match first.get(&(p.name.as_str(), p.layer)) {
                    None => {
                        first.insert((p.name.as_str(), p.layer), &p.rows);
                    }
                    Some(earlier) => {
                        assert_eq!(*earlier, &p.rows, "{case}: {} is planned twice, differently", p.name);
                        twice.push(p.name.as_str());
                    }
                }
            }
            assert_eq!(twice, ["blk.{layer}.attn_rope.a16", "blk.{layer}.attn_rope.a16.clamp"], "{case}: the one full-attention layer's rotation");
        }
    }

    /// **A mapped store streams the same inventory** (W4): the rows are read through the file
    /// descriptor, never the mapping, and the digest of the written-and-reopened artifact is the
    /// in-memory artifact's materialized inventory — on the cases that exercise the per-token lift
    /// and the tensor-backed exponents.
    #[test]
    fn a_mapped_store_streams_the_same_inventory() {
        let mut compared = 0;
        for (case, owned, profile) in cases() {
            if !(case.starts_with("per-token-lift/v6") || case.starts_with("group-exponents/")) {
                continue;
            }
            let path = std::env::temp_dir().join(format!("misaka-adr0103-{}-{}.palwq36", case.replace('/', "-"), std::process::id()));
            let plan: Vec<(String, usize)> =
                owned.tensor_names().iter().map(|n| (n.to_string(), owned.tensor(n).expect("present").len())).collect();
            let mut writer =
                crate::qwen36::Qwen36Writer::create(&path, &owned.shape, &owned.rope, owned.params_map(), plan.clone()).expect("created");
            for (name, _) in &plan {
                writer.push(name, owned.tensor(name).expect("present")).expect("appended");
            }
            writer.finish().expect("closed");
            let mapped = crate::qwen36::open_artifact(&path).expect("opens");
            assert!(mapped.extent(&plan[0].0).is_some(), "{case}: the reopened artifact is the mapped store");
            let materialized = crate::inventory::qwen36_inventory_v1(&owned, &profile).expect("the owned store serves");
            let streamed = crate::inventory::qwen36_inventory_digest_v1(&mapped, &profile).expect("the mapped store serves");
            assert_eq!(streamed.summary(), materialized.summary(), "{case}: one inventory, two stores, two sinks");
            std::fs::remove_file(&path).ok();
            compared += 1;
        }
        assert_eq!(compared, 2, "both mapped cases ran");
    }
}
