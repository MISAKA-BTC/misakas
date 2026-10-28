//! **The served IR annex** — RFC-0002's evidence transport, option B
//! (`docs/design/palw/tir/evidence-transport-scope.md`): one authenticated object per `(claim, leaf)`.
//!
//! A class the size of Qwen2.5-1.5B commits a capture no transport carries (an honest fold holds the
//! whole logits trace, ~40 MB; a dense one ~600 MB). A seat whose replay parts from such a claim holds
//! its own execution and the claim's roots, and nothing of the accused's. What it needs to accuse is
//! small: the first leaf at which the two executions differ is the only accused unit a cone close
//! reads that is not the seat's own (every leaf before it is), and the committed trace enters a close
//! only through the rows root, the ids and one row's tiles. So the executor serves, per leaf asked:
//!
//! * the claim's **binding** (program stripped: the chain holds the class's program),
//! * the leaf's **preimage** and its **opening** under the binding's step root,
//! * the **trace summary** — the rows root and the ids (the tiled scheme's trace root preimage beside
//!   the row count), and, at a leaf of the logits node, the **pin** of that row: the committed token's
//!   tile and the tile holding the leaf's first lane, each opened under the row, the row under the
//!   rows root. (The flat scheme commits one hash over every row, so its annex carries the rows —
//!   a small vocabulary's whole trace.)
//!
//! It is **authenticated by hash arithmetic alone** ([`palw_tir_leaf_annex_verify_v1`]): the binding
//! reproduces the claim's execution root, the opening walks to its step root, the preimage is the
//! opened leaf, the summary reproduces its trace root. No signature: a forged annex is refused, never
//! believed.
//!
//! **The seat finds the leaf by the annexes themselves** ([`tir_first_divergence_from_opening_v1`]).
//! An opening's siblings are the accused's subtree roots along the leaf's path; set against the
//! seat's own tree they say which subtree holds the first leaf the two differ at, and the next annex
//! asked is that subtree's first leaf. The target's level falls each round, so a job of `n` step leaves
//! is searched in at most `⌈log₂ n⌉` annexes.
//!
//! It rides the interval lane's leaf-evidence request (ADR-0111 Decision 2: bit 29, the leaf in the
//! signed request), under [`PALW_TIR_LEAF_ANNEX_MAGIC_V1`] so the legacy evidence is never mistaken for
//! it.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step_leg::{
    PalwStepOpeningV1, PalwStepTileLeafV1, step_opening_root_capped_v1, step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_step_refute::{
    PalwDecodeTokenPinV1, PalwTiledDecodePinV1, base0_logits_trace_root_v1, flat_logits_scheme_id_v1, tiled_decode_pin_v1,
    tiled_logits_outer_root_v1, tiled_logits_rows_root_v1, tiled_logits_scheme_id_v1,
};
use kaspa_consensus_core::palw_tir_court_v1::PalwTirStepLeafDisclosureV1;
use kaspa_consensus_core::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1, verify_tir_binding_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

use super::tree::TirStepTreeV1;

/// The wire version of [`PalwTirLeafAnnexV1`].
pub const PALW_TIR_LEAF_ANNEX_VERSION_V1: u16 = 1;
/// The 8-byte head of an encoded annex on the interval lane.
pub const PALW_TIR_LEAF_ANNEX_MAGIC_V1: [u8; 8] = *b"PALWTIRA";

/// **The committed trace, as an annex carries it.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwTirAnnexTraceV1 {
    /// The flat scheme: every committed row and id (one hash commits them all).
    Flat { logits_rows: Vec<Vec<i32>>, generated_token_ids: Vec<u32> },
    /// The tiled scheme: the rows root and the ids, and — at a leaf of the logits node — that row's pin.
    Tiled { rows_root: Hash64, generated_token_ids: Vec<u32>, pin: Option<PalwTiledDecodePinV1> },
}

/// **One leaf of one claim, served** (see the module doc).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirLeafAnnexV1 {
    /// [`PALW_TIR_LEAF_ANNEX_VERSION_V1`].
    pub version: u16,
    /// The claim's binding, its program EMPTY on the wire.
    pub binding: PalwTirStepBindingV1,
    pub opening: PalwStepOpeningV1,
    pub preimage: PalwStepTileLeafV1,
    pub trace: PalwTirAnnexTraceV1,
}

impl PalwTirLeafAnnexV1 {
    /// The magic, then the borsh object.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = PALW_TIR_LEAF_ANNEX_MAGIC_V1.to_vec();
        out.extend(borsh::to_vec(self).expect("an annex is borsh-serializable"));
        out
    }

    /// `Err` for bytes that are not an annex (no magic: the legacy lane's evidence, a plain opening).
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let body = bytes.strip_prefix(&PALW_TIR_LEAF_ANNEX_MAGIC_V1[..]).ok_or("not an IR annex")?;
        borsh::from_slice(body).map_err(|e| format!("the IR annex does not decode: {e}"))
    }

    /// The leaf this annex opens.
    pub fn leaf(&self) -> u64 {
        self.opening.leaf_index
    }

    /// The ids the trace summary carries.
    pub fn generated(&self) -> &[u32] {
        match &self.trace {
            PalwTirAnnexTraceV1::Flat { generated_token_ids, .. } | PalwTirAnnexTraceV1::Tiled { generated_token_ids, .. } => {
                generated_token_ids
            }
        }
    }

    /// **An on-chain `TirStepLeaf` answer, read as the annex of its leaf** (RFC-0002 evidence transport
    /// C, the second IR fence): the chain's disclosure carries what the served annex does — the
    /// binding (program empty), the leaf's preimage and opening, the ids in the class's scheme and, at
    /// a logits tile of a decode row, that row's pin aimed at the tile's first lane — so a seat whose
    /// executor served nothing pursues the claim on what the chain made it disclose, through the same
    /// step ([`palw_tir_leaf_annex_verify_v1`], then the descent). `Err` for ids in a scheme no IR class
    /// commits (`FloatV2`).
    pub fn from_step_leaf_disclosure_v1(disclosure: &PalwTirStepLeafDisclosureV1) -> Result<Self, String> {
        let trace = match &disclosure.decode {
            PalwDecodeTokenPinV1::TiledV1(tiled) => PalwTirAnnexTraceV1::Tiled {
                rows_root: tiled.rows_root,
                generated_token_ids: tiled.generated_token_ids.clone(),
                pin: disclosure.row_pin.clone(),
            },
            PalwDecodeTokenPinV1::Base0V1(flat) if disclosure.row_pin.is_none() => PalwTirAnnexTraceV1::Flat {
                logits_rows: flat.logits_rows.clone(),
                generated_token_ids: flat.generated_token_ids.clone(),
            },
            PalwDecodeTokenPinV1::Base0V1(_) => return Err("a flat-scheme disclosure carries no row pin".into()),
            PalwDecodeTokenPinV1::FloatV2(_) => return Err("an IR class commits no float-scheme ids".into()),
        };
        Ok(Self {
            version: PALW_TIR_LEAF_ANNEX_VERSION_V1,
            binding: disclosure.binding.clone(),
            opening: disclosure.opening.clone(),
            preimage: disclosure.preimage.clone(),
            trace,
        })
    }
}

/// **The trace an annex of leaf `leaf` carries**, from the executor's committed rows and ids: the flat
/// scheme's whole trace, or the tiled scheme's rows root and ids with — at a leaf of the logits node —
/// the pin of its row aimed at the leaf's first lane.
pub fn tir_annex_trace_v1(
    space: &PalwTirStepSpaceV1,
    ctx: &PalwJobContextV2,
    rows: &[Vec<i32>],
    generated: &[u32],
    leaf: u64,
) -> Result<PalwTirAnnexTraceV1, String> {
    let scheme = Hash64::from_bytes(space.program.logits_scheme_id);
    if scheme == flat_logits_scheme_id_v1() {
        return Ok(PalwTirAnnexTraceV1::Flat { logits_rows: rows.to_vec(), generated_token_ids: generated.to_vec() });
    }
    if scheme != tiled_logits_scheme_id_v1() {
        return Err("the program names no logits scheme".into());
    }
    let rows_root = tiled_logits_rows_root_v1(ctx, rows).ok_or("the committed rows build no rows root")?;
    let l = space.leaf_at(ctx, leaf).ok_or_else(|| format!("leaf {leaf} is not a leaf of this job"))?;
    let post = (space.occurrences().len() - 1) as u32;
    let pin = match l.kind {
        PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } if occurrence == post && node == space.program.logits => {
            let row = (l.position + 1).checked_sub(ctx.declared_prefill_tokens);
            match (row, u32::try_from(first_element)) {
                (Some(row), Ok(lane)) => tiled_decode_pin_v1(ctx, rows, generated, row, lane),
                _ => None,
            }
        }
        _ => None,
    };
    Ok(PalwTirAnnexTraceV1::Tiled { rows_root, generated_token_ids: generated.to_vec(), pin })
}

/// **Is this annex the claim's leaf?** — by hash arithmetic, against the claim's committed roots and
/// the class's own program (`program`, the chain's `tir_classes` row or the node's own copy, put back
/// into the binding the annex carries empty). `Ok` returns the binding with its program filled.
/// `max_step_leaf_count` is the court's ladder the claim's step tree is capped at.
pub fn palw_tir_leaf_annex_verify_v1(
    annex: &PalwTirLeafAnnexV1,
    program: &[u8],
    claim_execution_root: Hash64,
    claim_trace_root: Hash64,
    max_step_leaf_count: u64,
) -> Result<PalwTirStepBindingV1, String> {
    if annex.version != PALW_TIR_LEAF_ANNEX_VERSION_V1 {
        return Err(format!("annex version {} is not {PALW_TIR_LEAF_ANNEX_VERSION_V1}", annex.version));
    }
    if !annex.binding.class.program.is_empty() {
        return Err("the annex carries a program (the chain holds the class's)".into());
    }
    let mut binding = annex.binding.clone();
    binding.class.program = program.to_vec();
    if binding.committed_execution_root != claim_execution_root || binding.full_logits_trace_root != claim_trace_root {
        return Err("the annex's binding is not the claim's (its roots)".into());
    }
    let v = verify_tir_binding_v1(&binding, max_step_leaf_count).map_err(|e| format!("the annex's binding: {e}"))?;
    let root = step_opening_root_capped_v1(binding.step_leaf_count, &annex.opening, max_step_leaf_count)
        .map_err(|e| format!("the annex's opening: {e:?}"))?;
    if root != binding.step_merkle_root {
        return Err("the annex's opening does not walk to the claim's step root".into());
    }
    if step_tile_leaf_hash_v1(&v.context_hash, &v.class_id, &annex.preimage) != annex.opening.leaf_hash {
        return Err("the annex's preimage is not the opened leaf".into());
    }
    let ctx = &binding.job_context;
    let trace_root = match &annex.trace {
        PalwTirAnnexTraceV1::Flat { logits_rows, generated_token_ids } => {
            base0_logits_trace_root_v1(ctx, logits_rows, generated_token_ids)
        }
        PalwTirAnnexTraceV1::Tiled { rows_root, generated_token_ids, .. } => {
            tiled_logits_outer_root_v1(ctx, ctx.exact_decode_tokens as u64, rows_root, generated_token_ids)
        }
    };
    if trace_root != binding.full_logits_trace_root {
        return Err("the annex's trace summary does not reproduce the claim's trace root".into());
    }
    Ok(binding)
}

/// **Where the first differing leaf lies, as one opening of the accused's tree tells it.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TirDivergenceV1 {
    /// The opened leaf itself is the first leaf the two executions differ at.
    At(u64),
    /// It lies in the subtree of `level` whose first leaf is `first` — ask that leaf's annex next,
    /// searching below `level`.
    Within { level: usize, first: u64 },
    /// Nothing along the opening's path differs (within the searched levels).
    Agrees,
}

/// **The accused's opening of leaf `j` against the seat's own FULL tree** (`own`, the seat's execution
/// of the same job): the opening's siblings are the accused's subtree roots along `j`'s path, and the
/// first leaf the two differ at lies in the LEFTMOST differing one — a left sibling at the highest level
/// that differs, else leaf `j`, else the right sibling at the lowest level that differs. `below` limits
/// the search to the levels under a subtree already chosen (the previous round's), whose first leaf `j`
/// is. `None` when the opening is not of `own`'s shape.
pub fn tir_first_divergence_from_opening_v1(
    own: &TirStepTreeV1,
    opening: &PalwStepOpeningV1,
    below: Option<usize>,
) -> Option<TirDivergenceV1> {
    let n = own.leaf_count();
    let j = opening.leaf_index;
    if j >= n {
        return None;
    }
    let (mut left, mut right): (Option<(usize, u64)>, Option<(usize, u64)>) = (None, None);
    let mut at = j;
    let mut width = n;
    let mut level = 0usize;
    let mut siblings = opening.siblings.iter();
    while width > 1 {
        let promoted = width % 2 == 1 && at == width - 1;
        if !promoted {
            let s = *siblings.next()?;
            let pos = at ^ 1;
            if below.is_none_or(|b| level < b) && own.node(level, pos)? != s {
                if at % 2 == 1 {
                    left = Some((level, pos));
                } else if right.is_none() {
                    right = Some((level, pos));
                }
            }
        }
        at /= 2;
        width = width.div_ceil(2);
        level += 1;
    }
    if siblings.next().is_some() {
        return None;
    }
    Some(match (left, own.leaf_hash(j)? != opening.leaf_hash, right) {
        (Some((level, pos)), _, _) => TirDivergenceV1::Within { level, first: pos << level },
        (None, true, _) => TirDivergenceV1::At(j),
        (None, false, Some((level, pos))) => TirDivergenceV1::Within { level, first: pos << level },
        (None, false, None) => TirDivergenceV1::Agrees,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(i: u64, salt: u64) -> Hash64 {
        Hash64::from_u64_word(i.wrapping_mul(0x9E37_79B9).wrapping_add(salt))
    }

    /// **The descent finds the first differing leaf in at most `⌈log₂ n⌉ + 1` annexes**, for every tree
    /// width (odd levels promoted), every first difference, with later differences beside it, and says
    /// `Agrees` of an execution that differs nowhere — each round asking the first leaf of the subtree the
    /// previous opening named, searched below its level.
    #[test]
    fn the_annex_descent_finds_the_first_differing_leaf() {
        for n in 1u64..=70 {
            let own_leaves: Vec<Hash64> = (0..n).map(|i| h(i, 1)).collect();
            let own = TirStepTreeV1::full(&own_leaves);
            let depth = if n <= 1 { 0 } else { 64 - (n - 1).leading_zeros() as usize };
            // No difference: the first opening agrees.
            let first = own.opening(0).expect("an opening");
            assert_eq!(tir_first_divergence_from_opening_v1(&own, &first, None), Some(TirDivergenceV1::Agrees), "n={n}");
            for f in 0..n {
                for later in [None, Some(n - 1), Some((f + n) / 2)] {
                    let mut acc = own_leaves.clone();
                    acc[f as usize] = h(f, 2);
                    if let Some(l) = later.filter(|l| *l > f) {
                        acc[l as usize] = h(l, 3);
                    }
                    let accused = TirStepTreeV1::full(&acc);
                    let (mut j, mut below, mut rounds) = (0u64, None, 0usize);
                    let found = loop {
                        rounds += 1;
                        assert!(rounds <= depth + 1, "n={n} f={f}: {rounds} rounds");
                        let opening = accused.opening(j).expect("the accused's opening");
                        match tir_first_divergence_from_opening_v1(&own, &opening, below).expect("of own's shape") {
                            TirDivergenceV1::At(leaf) => break leaf,
                            TirDivergenceV1::Within { level, first } => {
                                assert!(below.is_none_or(|b| level < b), "the level falls");
                                (j, below) = (first, Some(level));
                            }
                            TirDivergenceV1::Agrees => panic!("n={n} f={f}: a differing tree agreed"),
                        }
                    };
                    assert_eq!(found, f, "n={n} later={later:?}");
                }
            }
        }
        // An opening that is not of the tree's shape is refused, never read.
        let own = TirStepTreeV1::full(&(0..9).map(|i| h(i, 1)).collect::<Vec<_>>());
        let mut long = own.opening(3).unwrap();
        long.siblings.push(Hash64::default());
        assert_eq!(tir_first_divergence_from_opening_v1(&own, &long, None), None);
        let mut short = own.opening(3).unwrap();
        short.siblings.pop();
        assert_eq!(tir_first_divergence_from_opening_v1(&own, &short, None), None);
        let past = PalwStepOpeningV1 { leaf_index: 9, ..own.opening(3).unwrap() };
        assert_eq!(tir_first_divergence_from_opening_v1(&own, &past, None), None);
    }
}
