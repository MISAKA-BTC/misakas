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
