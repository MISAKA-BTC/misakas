//! **What a Family-M producer retains and broadcasts** (ADR-0051 Decisions 3 and 4).
//!
//! A seat cannot judge an execution it cannot see. For the integer floor "seeing it" means every
//! step tile, because the court opens one; for Family M there is no court, and what a seat needs
//! instead is enough to **re-run sampled positions itself** and compare against what the producer
//! committed. That is: the job (so the seat runs the same one), the prompt (so it feeds the same
//! tokens), and the producer's own committed projection and binding (so there is something to
//! compare to that the CHAIN also holds).
//!
//! # Why the logits rows are not in here yet
//!
//! Decision 4's spot replay compares a recomputed position against the producer's committed row.
//! The worker commits those rows into `full_logits_trace_root_v2` as per-position event hashes and
//! does not currently hand them back — `--mode v2-legs-open` exists to reveal *named coordinates*,
//! which is exactly the shape the seat rule wants, and wiring it is ADR-0051 step 3. Carrying an
//! empty row list now and calling it a verification would be the worse mistake: a seat signing
//! `Valid` on self-consistency alone attests to something that is always true.
//!
//! So this codec is deliberately the *transport* half, versioned so step 3 extends it rather than
//! replaces it.

use kaspa_consensus_core::palw_legs::PalwLegsBindingV1;
use kaspa_consensus_core::palw_v2::{PalwJobContextV2, PalwResultProjectionV2};

/// Magic + version of the Family-M material container.
pub const PALW_METAL_MATERIAL_MAGIC: &[u8; 8] = b"PALWMM01";

/// The retained material of one Family-M execution.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwMetalMaterialV1 {
    /// The job the producer ran. A seat re-derives this from the anchor and must get the same
    /// thing; carrying it makes the disagreement visible instead of silent.
    pub job: PalwJobContextV2,
    /// The tokens fed. Bound by `job.prompt_token_ids_hash`, so a tampered prompt is caught
    /// without trusting the carrier.
    pub prompt_token_ids: Vec<u32>,
    /// The producer's committed projection — trace root, output commitment, schedule commitment,
    /// compute units, token counts.
    pub projection: PalwResultProjectionV2,
    /// The composite the claim's `execution_root` is.
    pub binding: PalwLegsBindingV1,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MetalMaterialError {
    Magic,
    Decode,
    Encode,
}

impl std::fmt::Display for MetalMaterialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Magic => write!(f, "not a PALW Family-M material container"),
            Self::Decode => write!(f, "the material does not decode"),
            Self::Encode => write!(f, "the material does not encode"),
        }
    }
}

impl std::error::Error for MetalMaterialError {}

pub fn metal_material_encode_v1(m: &PalwMetalMaterialV1) -> Result<Vec<u8>, MetalMaterialError> {
    let mut out = PALW_METAL_MATERIAL_MAGIC.to_vec();
    out.extend(borsh::to_vec(m).map_err(|_| MetalMaterialError::Encode)?);
    Ok(out)
}

pub fn metal_material_decode_v1(bytes: &[u8]) -> Result<PalwMetalMaterialV1, MetalMaterialError> {
    let body = bytes.strip_prefix(PALW_METAL_MATERIAL_MAGIC.as_slice()).ok_or(MetalMaterialError::Magic)?;
    borsh::from_slice(body).map_err(|_| MetalMaterialError::Decode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash64;

    fn material() -> PalwMetalMaterialV1 {
        let job = PalwJobContextV2 {
            version: kaspa_consensus_core::palw_v2::PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"net".to_vec(),
            job_id: Hash64::from_u64_word(1),
            job_nullifier: Hash64::from_u64_word(2),
            assignment_id: Hash64::from_u64_word(3),
            execution_seed: [4; 32],
            model_profile_id: Hash64::from_u64_word(5),
            runtime_manifest_hash: Hash64::from_u64_word(6),
            runtime_class_id: Hash64::from_u64_word(7),
            shape_profile_id: Hash64::from_u64_word(8),
            trace_scheme_id: Hash64::from_u64_word(9),
            cu_ruleset_id: Hash64::from_u64_word(10),
            tokenizer_id: Hash64::from_u64_word(11),
            prompt_token_ids_hash: Hash64::from_u64_word(12),
            declared_prefill_tokens: 8,
            exact_decode_tokens: 4,
            max_context_tokens: 4096,
        };
        PalwMetalMaterialV1 {
            projection: PalwResultProjectionV2 {
                job_context_hash: job.context_hash(),
                full_logits_trace_root: Hash64::from_u64_word(20),
                output_commitment: Hash64::from_u64_word(21),
                operation_schedule_commitment: Hash64::from_u64_word(22),
                canonical_compute_units: 33,
                prefill_tokens: 8,
                decode_tokens: 4,
                trace_event_count: 4,
                stop_reason: kaspa_consensus_core::palw_v2::PalwStopReasonV2::ExactBudgetReached,
            },
            binding: PalwLegsBindingV1 {
                version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
                job_context: job.clone(),
                tap_profile: kaspa_consensus_core::palw_legs::PalwActivationTapProfileV1 {
                    version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
                    tap_semantics_id: Hash64::from_u64_word(30),
                    tap_layer_indices: vec![0],
                    model_total_layers: 1,
                    hidden_dim: 8,
                    dtype: kaspa_consensus_core::palw_v2::PalwLogitsDtypeV2::F32Le,
                },
                checkpoint_profile: kaspa_consensus_core::palw_legs::PalwCheckpointProfileV1 {
                    version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
                    checkpoint_interval: 8,
                    state_layout_id: Hash64::from_u64_word(31),
                },
                full_logits_trace_root: Hash64::from_u64_word(20),
                activation_leaf_count: 0,
                activation_merkle_root: Hash64::from_u64_word(23),
                checkpoint_count: 0,
                checkpoint_merkle_root: Hash64::from_u64_word(24),
                committed_execution_root: Hash64::from_u64_word(25),
            },
            prompt_token_ids: vec![1, 2, 3, 4, 5, 6, 7, 8],
            job,
        }
    }

    #[test]
    fn the_material_round_trips() {
        let m = material();
        let back = metal_material_decode_v1(&metal_material_encode_v1(&m).unwrap()).unwrap();
        assert_eq!(back, m);
    }

    /// A container without the magic is refused by name rather than mis-parsed as borsh — the
    /// integer family's material is also borsh, and decoding one as the other would be a wrong
    /// answer instead of an error.
    #[test]
    fn foreign_bytes_are_refused_by_magic() {
        assert_eq!(metal_material_decode_v1(b"PALWB0A1 and then some"), Err(MetalMaterialError::Magic));
        assert_eq!(metal_material_decode_v1(b""), Err(MetalMaterialError::Magic));
        let mut truncated = metal_material_encode_v1(&material()).unwrap();
        truncated.truncate(PALW_METAL_MATERIAL_MAGIC.len() + 4);
        assert_eq!(metal_material_decode_v1(&truncated), Err(MetalMaterialError::Decode));
    }
}
