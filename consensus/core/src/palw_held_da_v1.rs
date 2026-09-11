//! **ADR-0103 Decision 4 — a data-availability accusation names a tile, a chunk or a block, and is
//! answered by that unit and its path.**
//!
//! ADR-0062's court names one trace EVENT: a logits row (and a tile of it), the unit a seat of the
//! whole-capture era could not obtain. Under the held regime a seat asks for other units — the
//! prompt ids it replays from (the commitment carries none of them under `PanelDa`), the checkpoint
//! chunks it RESUMES from (ADR-0103 Decision 2), the block of step leaves its interval opening named
//! as a fault's address (ADR-0086 Decision 6) — and an executor that withholds one of those holds a
//! seat hostage with nothing the event court can reach. This module is the same court over those
//! units: the accusation names one, the producer answers with it and its path, every validator
//! checks the answer by hash arithmetic against the claim's committed roots (SA-2 kept exactly),
//! and silence past the window is ADR-0062 Decision 5's default.
//!
//! **Every unit is bounded by chain facts before it is accepted as an accusation.** An accusation
//! of a unit the claim never committed is one the producer cannot answer, so it would be a
//! conviction by construction; the accusation therefore carries the claim's binding, authenticated
//! against the claim's `execution_root`, and the unit is refused unless it lies inside what that
//! binding commits (the prompt's tiles, the leg's checkpoints and their chunks, the step space).
//!
//! **Every answer fits one carrier by construction.** A prompt tile is `32 × 4` bytes and a path; a
//! state chunk is at most sixteen cache rows (or one recurrence head) and a two-level path; a step
//! range is at most [`PALW_HELD_DA_MAX_RANGE_LEAVES_V1`] leaf hashes (64 KiB) and a frontier.

use crate::Hash64;
use crate::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1};
use crate::palw_prompt_ids_v1::{PalwPromptIdsOpeningV1, prompt_ids_tile_count_v1, verify_prompt_ids_opening_v1};
use crate::palw_state_chunk_map::{palw_state_chunk_count_at_v1, palw_state_chunk_leaf_for_map_v1, palw_state_chunk_membership_root_v1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_leg::{
    PalwStepBindingV2, PalwStepRangeOpeningV1, checkpoint_leaf_hash_v2, step_opening_root_capped_v1, step_range_opening_root_capped_v1,
    verify_binding_v1,
};

pub const PALW_HELD_DA_VERSION_V1: u16 = 1;
pub const PALW_HELD_DA_DOMAIN_ACCUSATION_V1: &[u8] = b"misaka-palw/held-da/accusation/v1";
pub const PALW_HELD_DA_DOMAIN_DISCLOSURE_V1: &[u8] = b"misaka-palw/held-da/disclosure/v1";
/// The accuser's ML-DSA-87 context — in `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V4`.
pub const PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT: &[u8] = b"misaka-palw/held-da/accuse/mldsa87/v1";
/// The producer's ML-DSA-87 context over an answer — in `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V4`.
pub const PALW_HELD_DA_MLDSA87_DISCLOSE_CONTEXT: &[u8] = b"misaka-palw/held-da/disclose/mldsa87/v1";
pub const PALW_HELD_DA_ALL_DOMAINS: &[&[u8]] = &[
    PALW_HELD_DA_DOMAIN_ACCUSATION_V1,
    PALW_HELD_DA_DOMAIN_DISCLOSURE_V1,
    PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
    PALW_HELD_DA_MLDSA87_DISCLOSE_CONTEXT,
];

/// **The event index a held accusation leaves in the claim's `DefaultDisputed` phase.** The phase
/// keeps ADR-0062's shape — its clock, its reservation, its sweep and its resumption are the event
/// court's, unchanged — and the unit accused is recorded beside it. `u32::MAX` is never an event
/// index an accusation can name: ADR-0062's arm refuses a row at or past
/// `palw_da_max_accusable_rows_v1(trace_chunk_count)`, and `trace_chunk_count` is bounded by the
/// trace-event cap (4,096), so the row half of every accepted index is far below the row half of
/// this one (`the_sentinel_is_no_event_an_accusation_can_name` pins it).
pub const PALW_HELD_DA_EVENT_INDEX_SENTINEL_V1: u32 = u32::MAX;

/// The widest step range one answer carries: 1,024 leaf hashes are 64 KiB, inside one carrier with
/// the binding and the frontier beside them. A fault address wider than this (ADR-0086's block of
/// 4,096) is accused a quarter at a time.
pub const PALW_HELD_DA_MAX_RANGE_LEAVES_V1: u32 = 1024;

/// **What an accusation says is missing.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwHeldMissingV1 {
    /// One tile of the prompt ids, under the job's tiled root (ADR-0081 Decision 3).
    PromptIdsTile { tile: u32 },
    /// One chunk of one checkpoint's state, at its flat index.
    StateChunk { checkpoint: u32, chunk: u32 },
    /// A run of committed step leaves.
    StepRange { first: u64, count: u32 },
}

/// **The answer**, in the unit accused.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwHeldDisclosureV1 {
    PromptIdsTile { opening: PalwPromptIdsOpeningV1 },
    StateChunk { anchor: PalwAttnCheckpointAnchorV1, chunk: PalwAttnChunkOpeningV1 },
    StepRange { opening: PalwStepRangeOpeningV1 },
}

/// **The accusation**, with the binding its unit is bounded by.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwHeldAccusationV1 {
    pub version: u16,
    pub claim: Hash64,
    pub missing: PalwHeldMissingV1,
    pub accuser: PalwBondKeyV2,
    /// The claim's binding, authenticated against the claim's `execution_root`.
    pub binding: PalwStepBindingV2,
    /// The accuser's ML-DSA-87 over [`palw_held_da_accusation_message_v1`].
    pub signature: Vec<u8>,
}

/// **The producer's answer.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwHeldDisclosureCarriageV1 {
    pub version: u16,
    pub claim: Hash64,
    pub missing: PalwHeldMissingV1,
    pub binding: PalwStepBindingV2,
    pub disclosure: PalwHeldDisclosureV1,
    /// The producer's ML-DSA-87 over [`palw_held_da_disclosure_message_v1`], under the CLAIM's bond
    /// key — unsigned, a third party could bind the producer to material it never published.
    pub signature: Vec<u8>,
}

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// What the accuser signs: the network, the claim, the unit and the accuser.
pub fn palw_held_da_accusation_message_v1(network_domain: &[u8], a: &PalwHeldAccusationV1) -> Hash64 {
    let mut s = keyed(PALW_HELD_DA_DOMAIN_ACCUSATION_V1);
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(&borsh::to_vec(&a.missing).expect("borsh"));
    s.update(&borsh::to_vec(&a.accuser).expect("borsh"));
    finish(s)
}

/// What the producer signs: the network, the claim, the unit, and the answer's bytes.
pub fn palw_held_da_disclosure_message_v1(network_domain: &[u8], d: &PalwHeldDisclosureCarriageV1) -> Hash64 {
    let mut s = keyed(PALW_HELD_DA_DOMAIN_DISCLOSURE_V1);
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s.update(&d.version.to_le_bytes());
    s.update(d.claim.as_byte_slice());
    s.update(&borsh::to_vec(&d.missing).expect("borsh"));
    let answer = borsh::to_vec(&d.disclosure).expect("borsh");
    s.update(&(answer.len() as u64).to_le_bytes());
    s.update(&answer);
    finish(s)
}

/// The bytes a close ceiling prices, for either object.
pub fn palw_held_da_bytes_v1<T: borsh::BorshSerialize>(object: &T) -> u64 {
    borsh::to_vec(object).map(|b| b.len() as u64).unwrap_or(u64::MAX)
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwHeldDaError {
    #[error("version {got}; this build reads version {expected}")]
    Version { got: u16, expected: u16 },
    #[error("the binding does not authenticate: {0}")]
    Binding(String),
    #[error("the binding commits to {binding} and the claim to {claim}")]
    NotTheClaimsExecution { binding: Hash64, claim: Hash64 },
    #[error("the unit is outside what the claim committed: {0}")]
    OutsideTheCommitment(&'static str),
    #[error("the answer is not in the unit accused")]
    AnswerIsAnotherUnit,
    #[error("the prompt tile does not open under the job's root: {0}")]
    PromptTile(String),
    #[error("the checkpoint is not committed in the claim's leg")]
    CheckpointNotCommitted,
    #[error("the chunk is not in the checkpoint's state root")]
    ChunkNotInCheckpoint,
    #[error("the step range does not walk to the claim's step root")]
    RangeNotCommitted,
    #[error("the class's layout is not derivable here")]
    Layout,
}

/// The binding, authenticated against the claim's own `execution_root`.
fn authenticated(binding: &PalwStepBindingV2, claim_execution_root: &Hash64) -> Result<(Hash64, Hash64), PalwHeldDaError> {
    if binding.committed_execution_root != *claim_execution_root {
        return Err(PalwHeldDaError::NotTheClaimsExecution { binding: binding.committed_execution_root, claim: *claim_execution_root });
    }
    let (context_hash, _, checkpoint_profile_hash) = verify_binding_v1(binding).map_err(|e| PalwHeldDaError::Binding(e.to_string()))?;
    Ok((context_hash, checkpoint_profile_hash))
}

/// **Is `missing` a unit this claim committed?** The accusation half: refused unless the unit lies
/// inside what the authenticated binding commits, so no accusation can name a unit the producer
/// could never answer.
pub fn palw_held_da_check_accusation_v1(
    claim_execution_root: &Hash64,
    missing: &PalwHeldMissingV1,
    binding: &PalwStepBindingV2,
) -> Result<(), PalwHeldDaError> {
    use PalwHeldDaError::OutsideTheCommitment as outside;
    authenticated(binding, claim_execution_root)?;
    match *missing {
        PalwHeldMissingV1::PromptIdsTile { tile } => {
            let tiles = prompt_ids_tile_count_v1(u64::from(binding.job_context.declared_prefill_tokens))
                .ok_or(outside("the prompt has no tile tree"))?;
            if u64::from(tile) >= tiles {
                return Err(outside("the prompt has no such tile"));
            }
        }
        PalwHeldMissingV1::StateChunk { checkpoint, chunk } => {
            if checkpoint >= binding.checkpoint_count {
                return Err(outside("the leg has no such checkpoint"));
            }
            let covered = crate::palw_context_ladder::palw_checkpoint_covered_at_index_v1(
                &binding.shape_profile,
                checkpoint,
                binding.checkpoint_profile.checkpoint_interval,
            )
            .ok_or(outside("the checkpoint covers nothing the class can count"))?;
            let positions = crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&binding.shape_profile, &binding.job_context, covered);
            let count = palw_state_chunk_count_at_v1(&binding.shape_profile, positions).ok_or(PalwHeldDaError::Layout)?;
            if u64::from(chunk) >= count {
                return Err(outside("the checkpoint has no such chunk"));
            }
        }
        PalwHeldMissingV1::StepRange { first, count } => {
            if count == 0 || count > PALW_HELD_DA_MAX_RANGE_LEAVES_V1 {
                return Err(outside("a step range is one to 1,024 leaves"));
            }
            if first.checked_add(u64::from(count)).is_none_or(|end| end > binding.step_leaf_count) {
                return Err(outside("the step space ends before the range does"));
            }
        }
    }
    Ok(())
}

/// **Is this the unit, and is it the claim's?** The answer half — hash arithmetic against the
/// claim's committed roots, never an execution (ADR-0062 SA-2).
pub fn palw_held_da_check_disclosure_v1(
    claim_execution_root: &Hash64,
    missing: &PalwHeldMissingV1,
    binding: &PalwStepBindingV2,
    disclosure: &PalwHeldDisclosureV1,
    ladder: u64,
) -> Result<(), PalwHeldDaError> {
    palw_held_da_check_accusation_v1(claim_execution_root, missing, binding)?;
    let (context_hash, checkpoint_profile_hash) = authenticated(binding, claim_execution_root)?;
    match (*missing, disclosure) {
        (PalwHeldMissingV1::PromptIdsTile { tile }, PalwHeldDisclosureV1::PromptIdsTile { opening }) => {
            if opening.tile_index != tile {
                return Err(PalwHeldDaError::AnswerIsAnotherUnit);
            }
            verify_prompt_ids_opening_v1(&binding.job_context.prompt_token_ids_hash, binding.job_context.declared_prefill_tokens, opening)
                .map_err(|e| PalwHeldDaError::PromptTile(e.to_string()))?;
        }
        (PalwHeldMissingV1::StateChunk { checkpoint, chunk }, PalwHeldDisclosureV1::StateChunk { anchor, chunk: opening }) => {
            let leaf = &anchor.leaf;
            if leaf.checkpoint_index != checkpoint || opening.chunk_index != chunk {
                return Err(PalwHeldDaError::AnswerIsAnotherUnit);
            }
            if anchor.opening.leaf_index != u64::from(checkpoint)
                || checkpoint_leaf_hash_v2(&context_hash, &checkpoint_profile_hash, &binding.state_chunk_map_id, leaf) != anchor.opening.leaf_hash
            {
                return Err(PalwHeldDaError::CheckpointNotCommitted);
            }
            let root = step_opening_root_capped_v1(u64::from(binding.checkpoint_count), &anchor.opening, ladder)
                .map_err(|_| PalwHeldDaError::CheckpointNotCommitted)?;
            if root != binding.checkpoint_merkle_root {
                return Err(PalwHeldDaError::CheckpointNotCommitted);
            }
            let positions =
                crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&binding.shape_profile, &binding.job_context, leaf.covered_decode_call);
            let hash = palw_state_chunk_leaf_for_map_v1(&binding.shape_profile, positions, chunk, &opening.chunk_bytes)
                .ok_or(PalwHeldDaError::Layout)?;
            let folded = palw_state_chunk_membership_root_v1(&binding.shape_profile, positions, chunk, &hash, &opening.siblings)
                .map_err(|_| PalwHeldDaError::ChunkNotInCheckpoint)?;
            if folded != leaf.state_chunks_root {
                return Err(PalwHeldDaError::ChunkNotInCheckpoint);
            }
        }
        (PalwHeldMissingV1::StepRange { first, count }, PalwHeldDisclosureV1::StepRange { opening }) => {
            if opening.first_leaf_index != first || opening.leaf_hashes.len() != count as usize {
                return Err(PalwHeldDaError::AnswerIsAnotherUnit);
            }
            let root = step_range_opening_root_capped_v1(binding.step_leaf_count, opening, ladder).map_err(|_| PalwHeldDaError::RangeNotCommitted)?;
            if root != binding.step_merkle_root {
                return Err(PalwHeldDaError::RangeNotCommitted);
            }
        }
        _ => return Err(PalwHeldDaError::AnswerIsAnotherUnit),
    }
    Ok(())
}
