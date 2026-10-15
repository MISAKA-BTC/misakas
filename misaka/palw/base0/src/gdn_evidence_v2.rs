//! **RFC-0006 consumer v2: GDN evidence from material an accuser actually holds** — capture → evidence → the anchored step court
//! (object tag 243 / tag 244's `GdnTransition` and `GdnConvWindow`).
//!
//! The consensus crate's assemblers (`memory_v2::gdn_refutation_from_capture_v2`, `gdn_transition_refutation_from_capture_v2`,
//! `gdn_conv_window_refutation_from_capture_v2`) take a binding, the step and checkpoint trees, a tile source and a checkpoint
//! source. This module is the executor side that feeds them from a base0 retention, for every family whose kernels implement
//! [`Base0FpRecomputeKernelsV1`] — the hybrid tiers that every Qwen3.5/3.6/3.8 genesis model belongs to (held v4/v5, the v3
//! composition, consumer v2's composed maps) and the solo recurrence maps:
//!
//! * the **step tiles** are the claim's committed tiles (`Base0StepTilesV1`: a dense retention's, or a fold re-executed by the
//!   family's backend);
//! * the **checkpoint leg** is rebuilt from what the cadence retains — the producer's served chunks on a per-decode-call class, its
//!   leaves on a per-position one — and must hash to the binding's `checkpoint_merkle_root`;
//! * the **anchor state** a step stands on is the accuser's OWN recomputation of the job's prefix
//!   (`base0_fp_recompute_state_at_covered_v1`), never a chunk the producer chose. When that honest state does not hash to the
//!   committed leaf, the step terminal has nothing honest to stand on, and the answer is [`Base0GdnEvidenceV2::ForgedCheckpoint`] —
//!   the transition terminal's case, not an approximation.
//!
//! What this module cannot do, said rather than papered over: a forged checkpoint of a **per-position** (held) class is convicted
//! by the transition terminal only with the producer's own bytes for it, and a per-position retention serves none (its chunks are
//! re-derived, `Base0FpMaterialV2::checkpoint_chunks` is empty). Those bytes reach an accuser only through a chunk demand the
//! producer answers; [`base0_gdn_transition_evidence_v2`] refuses by name until one is carried.
//!
//! Nothing here is a proof of computation (PRINCIPLES §8): every output authenticates data membership, and the verdict is the
//! court's deterministic recomputation over it. Nothing here grants credit.
use crate::fp_recompute::{Base0FpRecomputeKernelsV1, base0_fp_recompute_state_at_covered_v1};
use crate::legs::{Base0CheckpointCaptureV1, Base0CheckpointsV1, Base0StepTilesV1};
use kaspa_consensus_core::palw_context_ladder::{PalwCheckpointCadenceV1, palw_checkpoint_cadence_v1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, canonical_step_leaf_index};
use kaspa_consensus_core::palw_step_leg::{PalwCheckpointLeafV2, PalwStepBindingV2, PalwStepTileLeafV1};
use kaspa_consensus_core::palw_step_refute::memory_v2::{
    PalwGdnCaptureV2, PalwGdnConvWindowRefutationV2, PalwGdnStepRefutationV2, PalwGdnTransitionRefutationV2,
    gdn_conv_window_refutation_from_capture_v2, gdn_refutation_from_capture_v2, gdn_replay_start_v2,
    gdn_transition_refutation_from_capture_v2,
};

/// **What an accuser holds of one claim's execution.**
pub struct Base0GdnMaterialV2<'a> {
    /// The claim's binding (its `committed_execution_root` is the claim's).
    pub binding: &'a PalwStepBindingV2,
    /// The claim's committed step tiles and every step leaf hash.
    pub tiles: &'a Base0StepTilesV1,
    /// The leg's leaves as served (`Base0FpMaterialV2::checkpoint_leaves`); empty where the producer served chunks instead.
    pub checkpoint_leaves: &'a [PalwCheckpointLeafV2],
    /// The producer's served chunks per checkpoint (a per-decode-call retention); EMPTY on a per-position class.
    pub served_chunks: &'a [Vec<Vec<u8>>],
    /// The job's prompt ids (checked against the context's commitment by the recompute).
    pub prompt_token_ids: &'a [u32],
    /// The committed generated ids (the decode calls' inputs).
    pub output_token_ids: &'a [u32],
    /// The network's prompt-commitment form.
    pub prompt_ids_form: PalwPromptIdsFormV1,
    /// The ruleset's step ladder (the path depth every opening is cut at).
    pub max_leaves: u64,
}

/// What the step-evidence builder found.
#[derive(Debug)]
pub enum Base0GdnEvidenceV2 {
    /// The anchored step refutation (tag 243's carrier), standing on a checkpoint the accuser recomputed and authenticated.
    Step(Box<PalwGdnStepRefutationV2>),
    /// The predecessor checkpoint this step would stand on is not the state the job reaches (the accuser's recompute does not hash
    /// to the committed leaf): accuse THAT checkpoint with the transition terminal instead.
    ForgedCheckpoint { index: u32 },
}

/// **Rebuild the claim's checkpoint leg** from what its cadence retains, and refuse one that is not the binding's.
pub fn base0_gdn_leg_v2(material: &Base0GdnMaterialV2<'_>) -> Result<Base0CheckpointsV1, String> {
    let b = material.binding;
    let leg = match palw_checkpoint_cadence_v1(&b.shape_profile) {
        PalwCheckpointCadenceV1::PerDecodeCall if !material.served_chunks.is_empty() => {
            Base0CheckpointCaptureV1::from_chunks_v1(&b.job_context, &b.shape_profile, &b.checkpoint_profile, material.served_chunks)
        }
        _ => Base0CheckpointCaptureV1::from_leaves_v1(
            &b.job_context,
            &b.shape_profile,
            &b.checkpoint_profile,
            material.checkpoint_leaves,
        ),
    }
    .map_err(|e| format!("the checkpoint leg does not rebuild: {e:?}"))?;
    if leg.merkle_root != b.checkpoint_merkle_root || leg.leaf_hashes.len() as u32 != b.checkpoint_count {
        return Err("the rebuilt checkpoint leg is not the one the claim's binding commits".to_string());
    }
    Ok(leg)
}

/// The accuser's own state at `leaf`'s covered counter, with fresh kernels; `Ok(None)` when it does not hash to the leaf (forged).
fn honest_chunks_v2<K: Base0FpRecomputeKernelsV1>(
    material: &Base0GdnMaterialV2<'_>,
    leaf: &PalwCheckpointLeafV2,
    mut kernels: K,
) -> Result<Option<Vec<Vec<u8>>>, String> {
    let b = material.binding;
    let state = base0_fp_recompute_state_at_covered_v1(
        &b.shape_profile,
        &b.job_context,
        material.prompt_token_ids,
        material.output_token_ids,
        leaf.covered_decode_call,
        &mut kernels,
        material.prompt_ids_form,
    )
    .map_err(|e| format!("the accuser cannot recompute checkpoint {}: {e}", leaf.checkpoint_index))?;
    Ok((state.state_chunks_root == leaf.state_chunks_root && state.chunks.len() as u32 == leaf.state_chunk_count)
        .then_some(state.chunks))
}

fn tile_source_v2<'m>(material: &'m Base0GdnMaterialV2<'_>) -> impl Fn(PalwStepCoordinateV1) -> Option<PalwStepTileLeafV1> + 'm {
    move |coord| {
        let b = material.binding;
        let index = canonical_step_leaf_index(&b.shape_profile, &b.job_context, &coord)?;
        material.tiles.tiles.iter().find(|(i, _)| *i == index).map(|(_, leaf)| leaf.clone())
    }
}

fn capture_v2<'m>(material: &'m Base0GdnMaterialV2<'_>, leg: &'m Base0CheckpointsV1) -> PalwGdnCaptureV2<'m> {
    PalwGdnCaptureV2 {
        binding: material.binding,
        step_leaf_hashes: &material.tiles.leaves,
        checkpoint_leaf_hashes: &leg.leaf_hashes,
        max_leaves: material.max_leaves,
    }
}

/// **The anchored step evidence for the recurrence step at `coord`** — the predecessor checkpoint the court will demand, the
/// accuser's own recomputed chunks for it (authenticated against the committed leaf), the bounded input suffix and the challenged
/// tile. `kernels` builds fresh recompute kernels (one prefix walk).
pub fn base0_gdn_step_evidence_v2<K: Base0FpRecomputeKernelsV1>(
    material: &Base0GdnMaterialV2<'_>,
    coord: PalwStepCoordinateV1,
    kernels: impl FnOnce() -> K,
) -> Result<Base0GdnEvidenceV2, String> {
    let leg = base0_gdn_leg_v2(material)?;
    let start = gdn_replay_start_v2(material.binding, &coord).map_err(|e| e.to_string())?;
    let anchor = match start.checkpoint_index {
        None => None,
        Some(index) => {
            let leaf = leg.leaves.get(index as usize).ok_or("the leg lacks the predecessor checkpoint")?;
            match honest_chunks_v2(material, leaf, kernels())? {
                Some(chunks) => Some((index, leaf.clone(), chunks)),
                None => return Ok(Base0GdnEvidenceV2::ForgedCheckpoint { index }),
            }
        }
    };
    let tile = tile_source_v2(material);
    let checkpoint = |i: u32| anchor.as_ref().filter(|(at, ..)| *at == i).map(|(_, leaf, chunks)| (leaf.clone(), chunks.clone()));
    gdn_refutation_from_capture_v2(&capture_v2(material, &leg), coord, &tile, &checkpoint)
        .map(|r| Base0GdnEvidenceV2::Step(Box::new(r)))
        .map_err(|e| e.to_string())
}

/// The producer's own bytes for checkpoint `index`, which only a per-decode-call retention serves.
fn served_v2<'m>(material: &'m Base0GdnMaterialV2<'_>, leg: &Base0CheckpointsV1, index: u32) -> Result<&'m Vec<Vec<u8>>, String> {
    if leg.chunks.is_empty() && material.served_chunks.is_empty() {
        return Err(format!(
            "checkpoint {index} of a per-position class has no served state: its bytes reach an accuser only through a chunk demand \
             the producer answers (not carried by this builder)"
        ));
    }
    material.served_chunks.get(index as usize).ok_or_else(|| format!("the producer served no chunks for checkpoint {index}"))
}

/// **The checkpoint-transition evidence against checkpoint `index`** (`node_slot`'s recurrence, `head`'s slice): the producer's
/// served bytes for the accused checkpoint, its predecessor checkpoint's served bytes after the accuser's recompute confirmed that
/// predecessor IS the job's state, and the inputs between them. Refused when the accused checkpoint is honest (nothing to accuse),
/// when the predecessor is itself forged (accuse the first forged checkpoint), and on a per-position class (no served bytes).
pub fn base0_gdn_transition_evidence_v2<K: Base0FpRecomputeKernelsV1>(
    material: &Base0GdnMaterialV2<'_>,
    node_slot: u32,
    head: u32,
    index: u32,
    mut kernels: impl FnMut() -> K,
) -> Result<PalwGdnTransitionRefutationV2, String> {
    let leg = base0_gdn_leg_v2(material)?;
    served_v2(material, &leg, index)?;
    let accused = leg.leaves.get(index as usize).ok_or("the leg has no such checkpoint")?;
    if honest_chunks_v2(material, accused, kernels())?.is_some() {
        return Err(format!("checkpoint {index} is the state the job reaches: there is no transition to accuse"));
    }
    // Per decode call (the only cadence with served bytes), the recurrence's predecessor is the previous checkpoint.
    if let Some(prev) = index.checked_sub(1) {
        let leaf = leg.leaves.get(prev as usize).ok_or("the leg lacks the predecessor checkpoint")?;
        if honest_chunks_v2(material, leaf, kernels())?.is_none() {
            return Err(format!("checkpoint {prev} is already not the job's state: accuse the first forged checkpoint"));
        }
    }
    let tile = tile_source_v2(material);
    let checkpoint = |i: u32| {
        let leaf = leg.leaves.get(i as usize)?;
        material.served_chunks.get(i as usize).map(|chunks| (leaf.clone(), chunks.clone()))
    };
    gdn_transition_refutation_from_capture_v2(&capture_v2(material, &leg), node_slot, head, index, &tile, &checkpoint)
        .map_err(|e| e.to_string())
}

/// **The convolution-window evidence against checkpoint `index`** (`conv_slot`'s window, `head`'s slice) — the producer's served
/// bytes for that checkpoint and the committed projection rows the window must equal. Refused on a per-position class (no served
/// bytes) and when the accuser's recompute finds the checkpoint honest.
pub fn base0_gdn_conv_window_evidence_v2<K: Base0FpRecomputeKernelsV1>(
    material: &Base0GdnMaterialV2<'_>,
    conv_slot: u32,
    head: u32,
    index: u32,
    kernels: impl FnOnce() -> K,
) -> Result<PalwGdnConvWindowRefutationV2, String> {
    let leg = base0_gdn_leg_v2(material)?;
    served_v2(material, &leg, index)?;
    let accused = leg.leaves.get(index as usize).ok_or("the leg has no such checkpoint")?;
    if honest_chunks_v2(material, accused, kernels())?.is_some() {
        return Err(format!("checkpoint {index} is the state the job reaches: there is no window to accuse"));
    }
    let tile = tile_source_v2(material);
    let checkpoint = |i: u32| {
        let leaf = leg.leaves.get(i as usize)?;
        material.served_chunks.get(i as usize).map(|chunks| (leaf.clone(), chunks.clone()))
    };
    gdn_conv_window_refutation_from_capture_v2(&capture_v2(material, &leg), conv_slot, head, index, &tile, &checkpoint)
        .map_err(|e| e.to_string())
}

/// **The first committed checkpoint that is not the state the job reaches** — one prefix walk on the accuser's own kernels, the
/// state root compared at every leaf in order. `None`: every checkpoint is honest (a step lie, if any, is the step terminal's).
/// Cost: one forward per position of the job plus one serialization per checkpoint (on a per-position class, one per position).
pub fn base0_gdn_first_forged_checkpoint_v2<K: Base0FpRecomputeKernelsV1>(
    material: &Base0GdnMaterialV2<'_>,
    mut kernels: K,
) -> Result<Option<u32>, String> {
    let leg = base0_gdn_leg_v2(material)?;
    let b = material.binding;
    let (profile, ctx) = (&b.shape_profile, &b.job_context);
    if !kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
        material.prompt_ids_form,
        material.prompt_token_ids,
        &ctx.prompt_token_ids_hash,
    ) {
        return Err("the prompt ids are not the job's".to_string());
    }
    let prefill = ctx.declared_prefill_tokens as usize;
    if material.prompt_token_ids.len() != prefill {
        return Err("the prompt is not the job's length".to_string());
    }
    let mut walked = 0usize;
    for leaf in &leg.leaves {
        let positions =
            kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, ctx, leaf.covered_decode_call);
        while walked < positions as usize {
            let token = if walked < prefill {
                material.prompt_token_ids[walked]
            } else {
                *material.output_token_ids.get(walked - prefill).ok_or("the committed generated ids are short")?
            };
            kernels.forward_no_capture(token as usize, walked).map_err(|e| e.to_string())?;
            walked += 1;
        }
        let chunks = kernels.state_chunks(profile, positions).map_err(|e| e.to_string())?;
        let root = crate::fp_interval::base0_state_chunks_root_for_v1(profile, positions, &chunks).map_err(|e| format!("{e:?}"))?;
        if root != leaf.state_chunks_root || chunks.len() as u32 != leaf.state_chunk_count {
            return Ok(Some(leaf.checkpoint_index));
        }
    }
    Ok(None)
}
