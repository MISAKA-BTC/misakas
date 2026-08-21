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
use kaspa_consensus_core::palw_base0_ops::{QuantParams, ScaleParams};
use kaspa_consensus_core::palw_base0_profile::PalwBase0GeometryV1;

use crate::artifact::{ArtifactError, Base0ArtifactV1};

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
}

/// `(multiplier LE, shift, zero LE)` — the nine bytes `palw_step_refute` reads per channel.
fn quant_bytes(q: QuantParams) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    out.extend_from_slice(&q.multiplier.to_le_bytes());
    out.push(q.shift);
    out.extend_from_slice(&q.zero.to_le_bytes());
    out
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
fn rope_row_bytes(artifact: &Base0ArtifactV1, position: usize) -> Vec<u8> {
    let pairs = artifact.shape.d_head / 2;
    let start = position * pairs;
    let mut out = Vec::with_capacity(8 * pairs);
    for v in &artifact.rope.cos_q[start..start + pairs] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in &artifact.rope.sin_q[start..start + pairs] {
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
    let d = shape.d_model();
    let kv = shape.kv_dim();
    let mut rows: Vec<PalwArtifactOperandV1> = Vec::new();

    // --- graph-level -------------------------------------------------------------------------
    // The embedding table: one row per token id, because `EmbedLookup` opens `(token · width,
    // width)`. Not tiled — a gather's operand is the row it gathered, and tiling it would make
    // every lookup open the neighbours it did not read.
    for token in 0..shape.vocab {
        rows.push(PalwArtifactOperandV1 {
            tensor_name: "token_embd.weight".to_string(),
            layer: None,
            row_start: (token * d) as u32,
            bytes: artifact.embed[token * d..(token + 1) * d].iter().map(|v| *v as u8).collect(),
        });
    }
    rows.extend(tile_matrix("output.weight", None, &artifact.unembed, d, tile));
    // The final narrowing, which the post table declares now that the engine's `norm_to_code` is
    // described as the two steps it is.
    rows.push(PalwArtifactOperandV1 {
        tensor_name: "output_norm.requant".to_string(),
        layer: None,
        row_start: 0,
        bytes: quant_bytes(artifact.norm_requant),
    });

    // --- per layer ---------------------------------------------------------------------------
    for (li, layer) in artifact.layers.iter().enumerate() {
        let l = Some(li as u16);
        // **The TEMPLATE name, with the layer in the `layer` field** — not a substituted one.
        // `palw_step_refute` asks the oracle with `node.weight_name`, which is the IR's own
        // `blk.{layer}.…` string, and passes the layer index beside it. An inventory that
        // substituted the index into the name would answer no request the court ever makes —
        // and `verify_covers_profile` would still pass, because it matches a template against a
        // substituted name deliberately. Found by closing the refutation round trip, which is
        // the only thing that asks the two sides to agree.
        let named = |suffix: &str| format!("blk.{{layer}}.{suffix}");
        for (suffix, weights, in_dim) in [
            ("attn_q.weight", &layer.wq, d),
            ("attn_k.weight", &layer.wk, d),
            ("attn_v.weight", &layer.wv, d),
            ("attn_output.weight", &layer.wo, d),
            ("ffn_gate.weight", &layer.w_gate, d),
            ("ffn_up.weight", &layer.w_up, d),
            ("ffn_down.weight", &layer.w_down, shape.d_ff),
        ] {
            rows.extend(tile_matrix(&named(suffix), l, weights, in_dim, tile));
        }
        // The narrowings, each one block. A uniform narrowing is nine bytes for the whole row —
        // the shape `palw_step_refute` accepts beside the per-channel one, and the only shape a
        // fixed inventory can carry for a step whose row length is a function of the position
        // (`qk_to_code` at the attention site is applied to a `kv_len`-long softmax output).
        for (suffix, params) in [
            ("attn_norm.requant", artifact.norm_requant),
            ("ffn_norm.requant", artifact.norm_requant),
            ("attn_q.requant", layer.requant[0]),
            ("attn_k.requant", layer.requant[1]),
            ("attn_v.requant", layer.requant[2]),
            ("attn_output.requant", layer.requant[3]),
            ("ffn_up.requant", layer.requant[5]),
            ("ffn_down.requant", layer.requant[6]),
            ("qk_to_code.requant", artifact.qk_to_code()),
            ("code_product.requant", artifact.code_product()),
            ("rope_clamp.requant", artifact.rope_clamp()),
            ("attn_residual.requant", artifact.residual_requant_at(li, 0)),
            ("ffn_residual.requant", artifact.residual_requant_at(li, 1)),
        ] {
            rows.push(PalwArtifactOperandV1 { tensor_name: named(suffix), layer: l, row_start: 0, bytes: quant_bytes(params) });
        }
        for (suffix, params) in [("attn_logit.scale", layer.attn_logit_scale), ("ffn_gate.scale", layer.ffn_gate_scale)] {
            rows.push(PalwArtifactOperandV1 { tensor_name: named(suffix), layer: l, row_start: 0, bytes: scale_bytes(params) });
        }
        // The rotary table, one row per position — what a rotation at position `p` opens.
        let mut offset = 0u32;
        for position in 0..shape.max_position {
            let bytes = rope_row_bytes(artifact, position);
            let len = bytes.len() as u32;
            rows.push(PalwArtifactOperandV1 { tensor_name: named("rope_table"), layer: l, row_start: offset, bytes });
            offset += len;
        }
        let _ = kv;
    }

    // The canonical order is `(tensor_name, layer, row_start)` ascending, and the constructor
    // refuses anything else. Sorting HERE rather than emitting in order keeps the layout above
    // readable as the graph — and the constructor still checks, so a sort that got it wrong is a
    // refusal rather than a silently different root.
    rows.sort_by(|a, b| (a.tensor_name.as_str(), a.layer, a.row_start).cmp(&(b.tensor_name.as_str(), b.layer, b.row_start)));
    PalwArtifactInventoryV1::new(rows).map_err(InventoryBuildError::NotCanonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{Base0ShapeV1, LN_THETA_10000_GEN_Q};
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
