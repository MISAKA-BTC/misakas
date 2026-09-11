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
use crate::palw_state_chunk_map::{
    palw_state_chunk_count_at_v1, palw_state_chunk_leaf_for_map_v1, palw_state_chunk_membership_root_v1,
};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_leg::{
    PalwStepBindingV2, PalwStepRangeOpeningV1, checkpoint_leaf_hash_v2, step_opening_root_capped_v1,
    step_range_opening_root_capped_v1, verify_binding_v1,
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
    /// **One step leaf's evidence** (ADR-0109 Decision 3): what a one-move accusation at `leaf`
    /// carries, demanded by a seat of the claim's panel for a leaf its own draw assigned it. Its
    /// answer is adjudicated, not only hash-checked (ADR-0109 Decision 4). Appended last.
    StepLeaf { leaf: u64 },
}

/// **The answer**, in the unit accused.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwHeldDisclosureV1 {
    PromptIdsTile {
        opening: PalwPromptIdsOpeningV1,
    },
    StateChunk {
        anchor: PalwAttnCheckpointAnchorV1,
        chunk: PalwAttnChunkOpeningV1,
    },
    StepRange {
        opening: PalwStepRangeOpeningV1,
    },
    /// A leaf's evidence (ADR-0109 Decision 1) — the one-move court's object without its accuser.
    StepLeaf {
        evidence: Box<crate::palw_shard_court_v1::PalwLeafEvidenceV1>,
    },
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
    #[error("the evidence is of another execution than the demand's binding")]
    EvidenceIsAnotherExecution,
}

/// The binding, authenticated against the claim's own `execution_root`.
fn authenticated(binding: &PalwStepBindingV2, claim_execution_root: &Hash64) -> Result<(Hash64, Hash64), PalwHeldDaError> {
    if binding.committed_execution_root != *claim_execution_root {
        return Err(PalwHeldDaError::NotTheClaimsExecution {
            binding: binding.committed_execution_root,
            claim: *claim_execution_root,
        });
    }
    let (context_hash, _, checkpoint_profile_hash) =
        verify_binding_v1(binding).map_err(|e| PalwHeldDaError::Binding(e.to_string()))?;
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
            let positions =
                crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&binding.shape_profile, &binding.job_context, covered);
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
        // ADR-0109 Decision 3: a leaf of the step space, and not a fused-attention site — that
        // leaf's terminal is ADR-0103 Decision 5's dissection, whose responder is already clocked.
        // Who may demand it (a seat of the panel, for a leaf its draw assigned it, once) is the
        // chain's to answer where the panel is read; this is the half the binding alone decides.
        PalwHeldMissingV1::StepLeaf { leaf } => {
            if leaf >= binding.step_leaf_count {
                return Err(outside("the step space ends before the leaf"));
            }
            let coord = crate::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf)
                .ok_or(outside("the leaf has no coordinates in this job"))?;
            if binding
                .shape_profile
                .resolve_node_slot(coord.node_slot)
                .is_some_and(|(node, _)| node.op_kind == crate::palw_step::PalwStepOpKindV1::AttnFused)
            {
                return Err(outside("a fused-attention leaf is tried by its dissection, not demanded"));
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
            verify_prompt_ids_opening_v1(
                &binding.job_context.prompt_token_ids_hash,
                binding.job_context.declared_prefill_tokens,
                opening,
            )
            .map_err(|e| PalwHeldDaError::PromptTile(e.to_string()))?;
        }
        (PalwHeldMissingV1::StateChunk { checkpoint, chunk }, PalwHeldDisclosureV1::StateChunk { anchor, chunk: opening }) => {
            let leaf = &anchor.leaf;
            if leaf.checkpoint_index != checkpoint || opening.chunk_index != chunk {
                return Err(PalwHeldDaError::AnswerIsAnotherUnit);
            }
            if anchor.opening.leaf_index != u64::from(checkpoint)
                || checkpoint_leaf_hash_v2(&context_hash, &checkpoint_profile_hash, &binding.state_chunk_map_id, leaf)
                    != anchor.opening.leaf_hash
            {
                return Err(PalwHeldDaError::CheckpointNotCommitted);
            }
            let root = step_opening_root_capped_v1(u64::from(binding.checkpoint_count), &anchor.opening, ladder)
                .map_err(|_| PalwHeldDaError::CheckpointNotCommitted)?;
            if root != binding.checkpoint_merkle_root {
                return Err(PalwHeldDaError::CheckpointNotCommitted);
            }
            let positions = crate::palw_context_ladder::palw_checkpoint_positions_at_v1(
                &binding.shape_profile,
                &binding.job_context,
                leaf.covered_decode_call,
            );
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
            let root = step_range_opening_root_capped_v1(binding.step_leaf_count, opening, ladder)
                .map_err(|_| PalwHeldDaError::RangeNotCommitted)?;
            if root != binding.step_merkle_root {
                return Err(PalwHeldDaError::RangeNotCommitted);
            }
        }
        // ADR-0109 Decision 4: the evidence is of the demanded leaf of THIS execution — its
        // refutation carries the demand's own authenticated binding — and whether it answers is the
        // one-move verdict, which the caller runs at the class's root (this function holds no class).
        (PalwHeldMissingV1::StepLeaf { leaf }, PalwHeldDisclosureV1::StepLeaf { evidence }) => {
            if evidence.leaf_index() != leaf {
                return Err(PalwHeldDaError::AnswerIsAnotherUnit);
            }
            if evidence.refutation.binding != *binding {
                return Err(PalwHeldDaError::EvidenceIsAnotherExecution);
            }
        }
        _ => return Err(PalwHeldDaError::AnswerIsAnotherUnit),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_checkpoint_court_v1::tests::{fixture_prompt_ids, held_fixture};
    use crate::palw_step_leg::{PalwStepOpeningV1, step_merkle_path_v1, step_merkle_range_siblings_v1};

    const LADDER: u64 = 1 << 26;

    /// **The sentinel is no event an accusation can name** — the proof the phase's shared shape
    /// rests on: ADR-0062's arm refuses a row at or past `palw_da_max_accusable_rows_v1(chunks)`,
    /// and no committed trace has that many rows at the sentinel's row half.
    #[test]
    fn the_sentinel_is_no_event_an_accusation_can_name() {
        let (row, _) = crate::palw_state_v2::palw_da_event_index_parts_v1(PALW_HELD_DA_EVENT_INDEX_SENTINEL_V1);
        let most = crate::palw_state_v2::palw_da_max_accusable_rows_v1(crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS as u32);
        assert!(row >= most, "row half {row} must be past every accusable row ({most})");
    }

    /// **Every unit answers with itself and its path, and only with that** — a state chunk under the
    /// held map, a run of step leaves, and a tile of the tiled prompt root.
    #[test]
    fn each_unit_is_answered_by_itself_and_its_path() {
        let fx = held_fixture(true, 20, None);
        let root = fx.binding.committed_execution_root;
        let b = &fx.binding;

        // A state chunk of checkpoint 9 (ten positions).
        let checkpoint = 9u32;
        let chunks = &fx.chunks[checkpoint as usize];
        let chunk = 3u32;
        let missing = PalwHeldMissingV1::StateChunk { checkpoint, chunk };
        palw_held_da_check_accusation_v1(&root, &missing, b).expect("a committed chunk is accusable");
        let answer = PalwHeldDisclosureV1::StateChunk {
            anchor: PalwAttnCheckpointAnchorV1 {
                leaf: fx.checkpoint_leaves[checkpoint as usize].clone(),
                opening: PalwStepOpeningV1 {
                    leaf_index: u64::from(checkpoint),
                    leaf_hash: fx.checkpoint_hashes[checkpoint as usize],
                    siblings: step_merkle_path_v1(&fx.checkpoint_hashes, checkpoint as usize).expect("a path"),
                },
            },
            chunk: PalwAttnChunkOpeningV1 {
                chunk_index: chunk,
                chunk_bytes: chunks[chunk as usize].clone(),
                siblings: crate::palw_state_chunk_map::palw_state_chunk_path_for_map_v1(
                    &b.shape_profile,
                    checkpoint + 1,
                    chunks,
                    chunk,
                )
                .expect("a path"),
            },
        };
        palw_held_da_check_disclosure_v1(&root, &missing, b, &answer, LADDER).expect("the chunk and its two-level path");
        let PalwHeldDisclosureV1::StateChunk { anchor, chunk: mut opened } = answer.clone() else { unreachable!() };
        opened.chunk_bytes[0] ^= 1;
        let tampered = PalwHeldDisclosureV1::StateChunk { anchor, chunk: opened };
        assert_eq!(
            palw_held_da_check_disclosure_v1(&root, &missing, b, &tampered, LADDER),
            Err(PalwHeldDaError::ChunkNotInCheckpoint)
        );
        // Checkpoint 9 holds ten positions: two kinds × two layers × one tile = four chunks.
        assert_eq!(chunks.len(), 4);
        let other = PalwHeldMissingV1::StateChunk { checkpoint, chunk: chunk - 1 };
        assert_eq!(palw_held_da_check_disclosure_v1(&root, &other, b, &answer, LADDER), Err(PalwHeldDaError::AnswerIsAnotherUnit));
        assert!(matches!(
            palw_held_da_check_accusation_v1(&root, &PalwHeldMissingV1::StateChunk { checkpoint, chunk: chunks.len() as u32 }, b),
            Err(PalwHeldDaError::OutsideTheCommitment(_))
        ));
        assert!(matches!(
            palw_held_da_check_accusation_v1(&root, &PalwHeldMissingV1::StateChunk { checkpoint: b.checkpoint_count, chunk: 0 }, b),
            Err(PalwHeldDaError::OutsideTheCommitment(_))
        ));

        // A run of step leaves.
        let (first, count) = (100u64, 64u32);
        let range = PalwHeldMissingV1::StepRange { first, count };
        palw_held_da_check_accusation_v1(&root, &range, b).expect("a committed range is accusable");
        let answer = PalwHeldDisclosureV1::StepRange {
            opening: crate::palw_step_leg::PalwStepRangeOpeningV1 {
                first_leaf_index: first,
                leaf_hashes: fx.leaves[first as usize..(first + u64::from(count)) as usize].to_vec(),
                siblings: step_merkle_range_siblings_v1(&fx.leaves, first as usize, count as usize).expect("siblings"),
            },
        };
        palw_held_da_check_disclosure_v1(&root, &range, b, &answer, LADDER).expect("the leaves and their frontier");
        let PalwHeldDisclosureV1::StepRange { opening: mut forged } = answer else { unreachable!() };
        forged.leaf_hashes[7] = crate::Hash64::from_u64_word(7);
        assert_eq!(
            palw_held_da_check_disclosure_v1(&root, &range, b, &PalwHeldDisclosureV1::StepRange { opening: forged }, LADDER),
            Err(PalwHeldDaError::RangeNotCommitted)
        );
        for bad in [
            PalwHeldMissingV1::StepRange { first, count: PALW_HELD_DA_MAX_RANGE_LEAVES_V1 + 1 },
            PalwHeldMissingV1::StepRange { first, count: 0 },
            PalwHeldMissingV1::StepRange { first: b.step_leaf_count - 1, count: 2 },
        ] {
            assert!(
                matches!(palw_held_da_check_accusation_v1(&root, &bad, b), Err(PalwHeldDaError::OutsideTheCommitment(_))),
                "{bad:?}"
            );
        }

        // A tile of the prompt ids, under the job's tiled root.
        let ids = fixture_prompt_ids(20);
        let tile = PalwHeldMissingV1::PromptIdsTile { tile: 0 };
        palw_held_da_check_accusation_v1(&root, &tile, b).expect("the prompt's first tile");
        let answer = PalwHeldDisclosureV1::PromptIdsTile {
            opening: crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&ids, 0).expect("an opening"),
        };
        palw_held_da_check_disclosure_v1(&root, &tile, b, &answer, LADDER).expect("the tile opens under the job's root");
        assert!(matches!(
            palw_held_da_check_accusation_v1(&root, &PalwHeldMissingV1::PromptIdsTile { tile: 1 }, b),
            Err(PalwHeldDaError::OutsideTheCommitment(_))
        ));

        // Another claim's execution is not this claim's.
        assert!(matches!(
            palw_held_da_check_accusation_v1(&crate::Hash64::from_u64_word(1), &tile, b),
            Err(PalwHeldDaError::NotTheClaimsExecution { .. })
        ));
    }
}
