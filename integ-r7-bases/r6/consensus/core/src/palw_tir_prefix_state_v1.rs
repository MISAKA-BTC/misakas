//! **The TIR prefix-state relation**: the relation core of the TIR prefix-state family court
//! ([`crate::palw_tir_prefix_owned_v1`]), wired behind `palw_staged_tir_route_v1`. The class
//! layout the step space needs is published on chain once per class (tag 169).
//!
//! A prefix-state job (FP version 11) consumes a declared state: `state_root =
//! palw_fp_prefix_state_root_v1(class_id, k, kv_digest)`. For a legacy class, `kv_digest` is its
//! family's cache digest (dense A16, Qwen36 hybrid, BASE-0 i8). For a **TIR class**, the
//! relation is structural, so no semantics declaration is needed. The state after the first `k`
//! positions is exactly what the claim's own step leaves commit at the prefix boundary, provided
//! `k` is aligned:
//!
//! * **Alignment (admission rule).** `0 < k < prefill`, `k ≡ 0 (mod checkpoint_interval)` and
//!   `k ≡ 0 (mod h_tile)`. Then the `Fixed` states after position `k − 1` are that position's
//!   checkpoint leaves, and the `Hist` rows `0..k` are exactly the `k / h_tile` history chunks
//!   committed at positions `h_tile − 1, …, k − 1`. An unaligned `k` would need a recurrence
//!   replay from the previous checkpoint (a kernel court), so it stays refused.
//! * **Canonical digest** (job-independent, so the same prefix of any job yields the same root;
//!   a leaf HASH binds its job context and could not be shared). Keyed BLAKE2b-512 under
//!   [`PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1`] over `le32(k)`, then over every boundary leaf in
//!   ascending leaf-index order ([`crate::palw_tir_step_v1::PalwTirStepSpaceV1::prefix_state_leaf_v1`]):
//!   `kind (u8: 0 Fixed, 1 Hist) ‖ le16(state) ‖ le16(layer or 0xFFFF) ‖ le64(first element or
//!   first lane) ‖ le32(first position, 0 for Fixed) ‖ le32(value_count) ‖ values_le`.
//!   `values_le` is the leaf's canonical 4-byte lanes. RFC-0002's TIR executor computes this same
//!   digest when it declares a prefix state.
//! * **Court.** [`PalwTirPrefixStateStreamV1`] takes the boundary leaves one per move, each with
//!   its values and its opening against the claim's step Merkle root. The leaf hash is
//!   `step_tile_leaf_hash_v1(context_hash, class_id, preimage)`, as the TIR step leg commits it.
//!   The stream is restartable: the stored continuation's byte count is re-checked against the
//!   ordinal. At EOF it re-derives the state root; a root that differs from the job's declared
//!   one is an objective fault, from the producer's own authenticated leaves alone. Leaf
//!   arithmetic is the step court's question, not this one.
use crate::Hash64;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_opening_root_capped_v1, step_tile_leaf_hash_v1};
use crate::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepSpaceV1};
use crate::palw_v2::PalwJobContextV2;
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1: &[u8] = b"misaka-palw/tir-prefix-state/v1";

/// The digest bytes one boundary leaf contributes ahead of its lanes.
fn leaf_prelude_v1(leaf: &crate::palw_tir_step_v1::PalwTirLeafV1) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(32);
    let (kind, state, layer, first, position) = match leaf.kind {
        PalwTirLeafKindV1::State { state, layer, first_element, .. } => (0u8, state, layer, first_element, 0u32),
        PalwTirLeafKindV1::HistTile { state, layer, first_lane, first_position, .. } => (1u8, state, layer, first_lane, first_position),
        PalwTirLeafKindV1::Commit { .. } => return Err("a commit leaf is not prefix state".into()),
    };
    out.push(kind);
    out.extend_from_slice(&state.to_le_bytes());
    out.extend_from_slice(&layer.unwrap_or(u16::MAX).to_le_bytes());
    out.extend_from_slice(&first.to_le_bytes());
    out.extend_from_slice(&position.to_le_bytes());
    out.extend_from_slice(&leaf.value_count.to_le_bytes());
    Ok(out)
}

/// The verdict of a completed stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTirPrefixStateVerdictV1 {
    Honest,
    /// The claim's own boundary leaves re-derive another state root than the job declared.
    StateRootMismatch { derived: Hash64 },
}

/// Restartable authenticated digest over one claim's prefix boundary leaves.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirPrefixStateStreamV1 {
    class_id: Hash64,
    context_hash: Hash64,
    step_merkle_root: Hash64,
    step_leaf_count: u64,
    maximum_leaves: u64,
    prefix_tokens: u32,
    declared_state_root: Hash64,
    total: u64,
    ordinal: u64,
    /// Bytes hashed so far (key block excluded), re-checked against the stored continuation.
    hashed: u128,
    digest: LeafHashContinuationV1,
    verdict: Option<PalwTirPrefixStateVerdictV1>,
}

impl PalwTirPrefixStateStreamV1 {
    /// Opens over the claim's admitted header facts and the job's declared prefix state.
    #[allow(clippy::too_many_arguments)]
    pub fn new_v1(
        space: &PalwTirStepSpaceV1,
        class_id: Hash64,
        job_context: &PalwJobContextV2,
        step_merkle_root: Hash64,
        step_leaf_count: u64,
        maximum_leaves: u64,
        prefix_tokens: u32,
        declared_state_root: Hash64,
    ) -> Result<Self, String> {
        let total = space
            .prefix_state_leaf_count_v1(job_context, prefix_tokens)
            .ok_or("TIR prefix-state needs k aligned to the checkpoint interval and h_tile, inside the prompt")?;
        if total == 0 || step_leaf_count == 0 || step_leaf_count > maximum_leaves {
            return Err("TIR prefix-state boundary has no leaves or exceeds the class ladder".into());
        }
        let mut digest = LeafHashContinuationV1::new(PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1);
        digest.update(&prefix_tokens.to_le_bytes());
        Ok(Self {
            class_id,
            context_hash: job_context.context_hash(),
            step_merkle_root,
            step_leaf_count,
            maximum_leaves,
            prefix_tokens,
            declared_state_root,
            total,
            ordinal: 0,
            hashed: 4,
            digest,
            verdict: None,
        })
    }

    pub fn ordinal_v1(&self) -> u64 {
        self.ordinal
    }
    pub fn total_v1(&self) -> u64 {
        self.total
    }
    pub fn verdict_v1(&self) -> Option<PalwTirPrefixStateVerdictV1> {
        self.verdict
    }

    /// The immutable contract two streams of one court must share (everything but progress).
    pub fn same_contract_v1(&self, other: &Self) -> bool {
        (
            self.class_id,
            self.context_hash,
            self.step_merkle_root,
            self.step_leaf_count,
            self.maximum_leaves,
            self.prefix_tokens,
            self.declared_state_root,
            self.total,
        ) == (
            other.class_id,
            other.context_hash,
            other.step_merkle_root,
            other.step_leaf_count,
            other.maximum_leaves,
            other.prefix_tokens,
            other.declared_state_root,
            other.total,
        )
    }

    pub fn check_v1(&self) -> Result<(), String> {
        if self.ordinal > self.total
            || self.verdict.is_some() != (self.ordinal == self.total)
            || self.digest.byte_count() != Some(128 + self.hashed)
        {
            return Err("TIR prefix-state continuation changed its original byte position".into());
        }
        Ok(())
    }

    /// Fold the next boundary leaf: its canonical lanes and its opening against the claim's step
    /// tree. A leaf that does not authenticate changes nothing.
    pub fn leaf_v1(
        &mut self,
        space: &PalwTirStepSpaceV1,
        job_context: &PalwJobContextV2,
        values_le: &[u8],
        opening: &PalwStepOpeningV1,
    ) -> Result<(), String> {
        self.check_v1()?;
        if job_context.context_hash() != self.context_hash {
            return Err("TIR prefix-state leaf is read under another job context".into());
        }
        let leaf = space
            .prefix_state_leaf_v1(job_context, self.prefix_tokens, self.ordinal)
            .ok_or("TIR prefix-state stream is complete or its boundary changed")?;
        crate::palw_tir_step_v1::palw_tir_lane_values_v1(leaf.dtype, values_le).map_err(|e| e.to_string())?;
        if values_le.len() != leaf.value_count as usize * 4 {
            return Err("TIR prefix-state leaf carries another lane count".into());
        }
        let preimage = PalwStepTileLeafV1 {
            version: crate::palw_tir_step_v1::PALW_TIR_STEP_LEAF_VERSION_V1,
            coord: leaf.coord,
            value_count: leaf.value_count,
            values_le: values_le.to_vec(),
        };
        let hash = step_tile_leaf_hash_v1(&self.context_hash, &self.class_id, &preimage);
        if opening.leaf_index != leaf.index
            || opening.leaf_hash != hash
            || step_opening_root_capped_v1(self.step_leaf_count, opening, self.maximum_leaves).map_err(|e| e.to_string())?
                != self.step_merkle_root
        {
            return Err("TIR prefix-state leaf does not authenticate against the claim's step tree".into());
        }
        let prelude = leaf_prelude_v1(&leaf)?;
        let mut next = self.clone();
        next.digest.update(&prelude);
        next.digest.update(values_le);
        next.hashed += (prelude.len() + values_le.len()) as u128;
        next.ordinal += 1;
        if next.ordinal == next.total {
            let derived = crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1(&self.class_id, self.prefix_tokens, &next.digest.finish());
            next.verdict = Some(if derived == self.declared_state_root {
                PalwTirPrefixStateVerdictV1::Honest
            } else {
                PalwTirPrefixStateVerdictV1::StateRootMismatch { derived }
            });
        }
        *self = next;
        Ok(())
    }
}

/// The producer-side digest of the same relation, from the boundary leaves' lanes in order
/// (the reference the TIR executor must reproduce when it declares a prefix state).
pub fn palw_tir_prefix_kv_digest_v1(
    space: &PalwTirStepSpaceV1,
    job_context: &PalwJobContextV2,
    prefix_tokens: u32,
    mut lanes_of: impl FnMut(&crate::palw_tir_step_v1::PalwTirLeafV1) -> Result<Vec<u8>, String>,
) -> Result<Hash64, String> {
    let total = space.prefix_state_leaf_count_v1(job_context, prefix_tokens).ok_or("unaligned TIR prefix boundary")?;
    let mut digest = LeafHashContinuationV1::new(PALW_TIR_PREFIX_STATE_DIGEST_DOMAIN_V1);
    digest.update(&prefix_tokens.to_le_bytes());
    for ordinal in 0..total {
        let leaf = space.prefix_state_leaf_v1(job_context, prefix_tokens, ordinal).ok_or("TIR prefix boundary leaf missing")?;
        let lanes = lanes_of(&leaf)?;
        if lanes.len() != leaf.value_count as usize * 4 {
            return Err("TIR prefix boundary lanes have another count".into());
        }
        digest.update(&leaf_prelude_v1(&leaf)?);
        digest.update(&lanes);
    }
    Ok(digest.finish())
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_tir_prefix_state_v1.rs"]
mod native_resource_inventory_v1;
