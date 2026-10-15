//! Outer public predicate for huge EXISTING V1 token renderings, with no producer state.
//! This is a rooted continuation, not a caller-provided parser-state credential. Admission
//! must pin `expected_root` to the actual claim and preserve this row under the owned fence.
//! Every byte move is bounded; semantic facts become usable only after old leaf authentication.
use crate::Hash64;
use crate::palw_decode_constraint_v1::{PalwConstraintStateV1, PalwDecodeConstraintV1, constraint_start_state_v1};
use crate::palw_eog_stream_v1::PalwEogStreamV1;
use crate::palw_fp_constraint_job_v1::{palw_fp_constraint_shape_v1, palw_fp_constraint_table_root_v1};
use crate::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptCommitmentV3, PalwFreePromptJobV3, fp_claim_id_v3, fp_job_id_v3};
use crate::palw_rendering_stream_v1::PalwRenderingStreamV1;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepFaultV1, PalwStepOpeningV1, verify_binding_v1};
use crate::palw_step_refute::{PALW_LOGITS_TILE_LANES, PalwTiledDecodePinV1, tiled_tile_authenticate_v1};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwConstraintStreamPhaseV1 {
    Eog,
    Prefix(u32),
    Committed,
    Auxiliary,
    Complete,
    Tokenizer,
    PrefixCourt,
}
/// Immutable certificate DERIVED by the rooted court, reusable against only an attestation
/// whose authenticated duty contains this position. Producer liability still names the claim.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamFaultV1 {
    pub claim_id: Hash64,
    pub reservation_owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub execution_root: Hash64,
    pub job_id: Hash64,
    pub seed_id: Hash64,
    pub position: u32,
    pub fault: PalwStepFaultV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamCourtV1 {
    claim_id: Hash64,
    owner: PalwBondKeyV2,
    reserved_daa: u64,
    root: Hash64,
    job_id: Hash64,
    context: Hash64,
    table_root: Hash64,
    vocab: u32,
    constraint: PalwDecodeConstraintV1,
    generated: Vec<u32>,
    position: u32,
    witness: Option<u32>,
    beat: Option<u32>,
    beats: bool,
    phase: PalwConstraintStreamPhaseV1,
    eog: Option<PalwEogStreamV1>,
    lowest_eog: Option<u32>,
    eog_members: Vec<u32>,
    prefix_state: PalwConstraintStateV1,
    rendering: Option<PalwRenderingStreamV1>,
    convicted: bool,
    terminal_stop: Option<PalwFpStopReasonV3>,
    output_fault_position: Option<u32>,
}

/// Journal checkpoint excludes immutable grammar/binding/generated IDs. These bytes may be
/// restored only by the rooted delta engine, never accepted as a challenger credential.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwConstraintStreamCheckpointV1 {
    seed_id: Hash64,
    witness: Option<u32>,
    phase: PalwConstraintStreamPhaseV1,
    eog: Option<PalwEogStreamV1>,
    lowest_eog: Option<u32>,
    eog_members: Vec<u32>,
    prefix_state: PalwConstraintStateV1,
    rendering: Option<PalwRenderingStreamV1>,
    convicted: bool,
    output_fault_position: Option<u32>,
}

impl PalwConstraintStreamCourtV1 {
    pub(crate) fn checkpoint_v1(&self) -> PalwConstraintStreamCheckpointV1 {
        PalwConstraintStreamCheckpointV1 {
            seed_id: self.context,
            witness: self.witness,
            phase: self.phase,
            eog: self.eog.clone(),
            lowest_eog: self.lowest_eog,
            eog_members: self.eog_members.clone(),
            prefix_state: self.prefix_state.clone(),
            rendering: self.rendering.clone(),
            convicted: self.convicted,
            output_fault_position: self.output_fault_position,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwConstraintStreamCheckpointV1) -> Result<(), String> {
        if cp.seed_id != self.context {
            return Err("journal checkpoint belongs to another immutable court seed".into());
        }
        if let Some(eog) = &cp.eog {
            eog.check()?;
        }
        if let Some(rendering) = &cp.rendering {
            rendering.check()?;
        }
        if cp.eog_members.len() > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS + 2
            || cp.eog_members.windows(2).any(|w| w[0] >= w[1])
            || cp.output_fault_position.is_some_and(|position| position as usize >= self.generated.len())
        {
            return Err("journal checkpoint exceeds bounded EOG membership".into());
        }
        self.witness = cp.witness;
        self.phase = cp.phase;
        self.eog = cp.eog.clone();
        self.lowest_eog = cp.lowest_eog;
        self.eog_members = cp.eog_members.clone();
        self.prefix_state = cp.prefix_state.clone();
        self.rendering = cp.rendering.clone();
        self.convicted = cp.convicted;
        self.output_fault_position = cp.output_fault_position;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &PalwTiledDecodePinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        ladder: u64,
    ) -> Result<Self, String> {
        Self::new_with_seed_context_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            binding,
            job,
            pin,
            witness,
            eog_count,
            eog_opening,
            ladder,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_with_seed_context_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &PalwTiledDecodePinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        ladder: u64,
        signed_context: Option<Hash64>,
    ) -> Result<Self, String> {
        if binding.committed_execution_root != expected_root {
            return Err("stream binding is not the actual claim root".into());
        }
        verify_binding_v1(binding).map_err(|e| e.to_string())?;
        Self::new_from_tiled_view_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            &crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding),
            &borsh::to_vec(binding).map_err(|e| e.to_string())?,
            job,
            pin,
            witness,
            eog_count,
            eog_opening,
            ladder,
            signed_context,
        )
    }
    /// The tiled seed over a decode view. The caller derives `view` natively (a verified legacy
    /// binding, or a TIR claim's admitted header and registered record) and passes the exact
    /// bytes that identified it at admission as `view_identity` (the legacy binding's borsh, or
    /// the TIR header's), which enter the canonical seed identity unchanged.
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_tiled_view_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        view_identity: &[u8],
        job: &PalwFreePromptJobV3,
        pin: &PalwTiledDecodePinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        ladder: u64,
        signed_context: Option<Hash64>,
    ) -> Result<Self, String> {
        if view.committed_execution_root != expected_root {
            return Err("stream binding is not the actual claim root".into());
        }
        if !job.is_constraint() || fp_job_id_v3(job) != view.job_context.job_id {
            return Err("stream job is not the job authenticated by the claim".into());
        }
        if pin.generated_token_ids.len() > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS {
            return Err("stream seed exceeds authenticated trace format".into());
        }
        let (ctx, vocab, tiles) = crate::palw_step_refute::tiled_row_authenticate_view_v1(
            view,
            &pin.generated_token_ids,
            pin.position,
            &pin.row_root,
            &pin.row_opening,
            ladder,
        )
        .map_err(|e| e.to_string())?;
        let committed = pin.generated_token_ids[pin.position as usize];
        if committed as usize >= vocab || witness.is_some_and(|w| w as usize >= vocab) {
            return Err("stream selected token is outside registered vocabulary".into());
        }
        let beat = (!pin.committed_tile_lanes.is_empty() || !pin.beat_tile_lanes.is_empty()).then_some(pin.beat_lane);
        let mut beats = false;
        if let Some(b) = beat {
            if b as usize >= vocab {
                return Err("stream beating lane is outside registered vocabulary".into());
            }
            let value = |id: u32, lanes: &[i32], opening: &PalwStepOpeningV1| -> Result<i32, String> {
                tiled_tile_authenticate_v1(
                    &ctx,
                    pin.position,
                    vocab,
                    tiles,
                    &pin.row_root,
                    (id as usize / PALW_LOGITS_TILE_LANES) as u64,
                    lanes,
                    opening,
                    ladder,
                )
                .map_err(|e| e.to_string())?;
                Ok(lanes[id as usize % PALW_LOGITS_TILE_LANES])
            };
            let a = value(committed, &pin.committed_tile_lanes, &pin.committed_opening)?;
            let bval = value(b, &pin.beat_tile_lanes, &pin.beat_opening)?;
            let sampling = job.sampling_v2();
            beats = crate::palw_decode_select_v2::decode_lane_beats_v2(
                sampling.lane_key(bval, pin.position, b as usize),
                b as usize,
                sampling.lane_key(a, pin.position, committed as usize),
                committed as usize,
            );
        }
        Self::from_authenticated_selection_view_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            view,
            view_identity,
            job,
            &pin.generated_token_ids,
            pin.position,
            witness,
            beat,
            beats,
            eog_count,
            eog_opening,
            signed_context,
            &borsh::to_vec(pin).map_err(|e| e.to_string())?,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_flat_pin_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
    ) -> Result<Self, String> {
        Self::new_from_flat_pin_with_seed_context_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            binding,
            job,
            pin,
            witness,
            eog_count,
            eog_opening,
            None,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new_from_flat_pin_with_seed_context_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        signed_context: Option<Hash64>,
    ) -> Result<Self, String> {
        Self::flat_selection_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            binding,
            job,
            pin,
            witness,
            eog_count,
            eog_opening,
            signed_context,
            None,
        )
    }
    /// The caller must supply the native retained ORIGINAL commitment, not a stop reason
    /// asserted by the challenger. Its exact claim identity binds the terminal predicate.
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_flat_pin_with_original_stop_context_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        original: &PalwFreePromptCommitmentV3,
        signed_context: Option<Hash64>,
    ) -> Result<Self, String> {
        if fp_claim_id_v3(original) != claim_id
            || original.job != *job
            || original.execution_root != expected_root
            || original.decode_tokens_executed != binding.job_context.exact_decode_tokens
            || original.decode_tokens_executed == 0
            || original.decode_tokens_executed > job.decode_token_limit
            || match original.stop_reason {
                PalwFpStopReasonV3::ExactBudgetReached => original.decode_tokens_executed != job.decode_token_limit,
                PalwFpStopReasonV3::EndOfGeneration => original.decode_tokens_executed >= job.decode_token_limit,
            }
        {
            return Err("terminal Flat predicate changes native original claim/count/stop facts".into());
        }
        Self::flat_selection_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            binding,
            job,
            pin,
            witness,
            eog_count,
            eog_opening,
            signed_context,
            Some(original.stop_reason),
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn flat_selection_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &crate::palw_flat_constraint_stream_v1::PalwFlatConstraintPinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        signed_context: Option<Hash64>,
        original_stop: Option<PalwFpStopReasonV3>,
    ) -> Result<Self, String> {
        if !pin.matches_binding_v1(binding) {
            return Err("flat semantic pin does not preserve the original claim/job/scheme".into());
        }
        let terminal = pin.position_v1() as usize == pin.generated_ids_v1().len();
        if terminal && original_stop.is_none() {
            return Err("terminal Flat predicate requires native original stop facts".into());
        }
        let committed = pin.committed_id_v1();
        let value = pin.committed_value_v1();
        if committed.is_some_and(|id| id < binding.shape_profile.vocab_size) && value.is_none() {
            return Err("in-vocabulary Flat selection lacks its authenticated scalar".into());
        }
        let beat = pin.beat_v1();
        let sampling = job.sampling_v2();
        let beats = beat.zip(committed.zip(value)).is_some_and(|((id, b), (committed, value))| {
            crate::palw_decode_select_v2::decode_lane_beats_v2(
                sampling.lane_key(b, pin.position_v1(), id as usize),
                id as usize,
                sampling.lane_key(value, pin.position_v1(), committed as usize),
                committed as usize,
            )
        });
        Self::from_authenticated_selection_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            binding,
            job,
            pin.generated_ids_v1(),
            pin.position_v1(),
            witness,
            beat.map(|b| b.0),
            beats,
            eog_count,
            eog_opening,
            signed_context,
            &borsh::to_vec(pin).map_err(|e| e.to_string())?,
            if terminal { original_stop } else { None },
        )
    }
    /// **Greedy Float32 selection** (behind `palw_staged_float_constraint_v1`): the selection facts
    /// come from an authenticated [`crate::palw_float_constraint_source_v1::PalwFloatConstraintPinV1`]
    /// — the ids re-derived the claim's v2 trace root, and the committed/beating lanes' f32 order
    /// keys come from a row whose event hash re-derived it too. The order key reproduces the
    /// reference worker's strict `>` with first-index ties, so `beats` is the integer lane's own
    /// rule over the keys. A seeded job is refused (no canonical float sampler). The caller supplies
    /// the native retained original commitment for the terminal predicate, as for Flat.
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_float_pin_with_original_stop_context_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        pin: &crate::palw_float_constraint_source_v1::PalwFloatConstraintPinV1,
        witness: Option<u32>,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        original: &PalwFreePromptCommitmentV3,
        signed_context: Option<Hash64>,
    ) -> Result<Self, String> {
        if fp_claim_id_v3(original) != claim_id
            || original.job != *job
            || original.execution_root != expected_root
            || original.decode_tokens_executed != binding.job_context.exact_decode_tokens
            || original.decode_tokens_executed == 0
            || original.decode_tokens_executed > job.decode_token_limit
            || match original.stop_reason {
                PalwFpStopReasonV3::ExactBudgetReached => original.decode_tokens_executed != job.decode_token_limit,
                PalwFpStopReasonV3::EndOfGeneration => original.decode_tokens_executed >= job.decode_token_limit,
            }
        {
            return Err("terminal Float32 predicate changes native original claim/count/stop facts".into());
        }
        if !job.sampling_v2().is_greedy() {
            return Err("a seeded Float32 job has no canonical float sampler to adjudicate".into());
        }
        if !pin.matches_binding_v1(binding) {
            return Err("Float32 semantic pin does not preserve the original claim/job/lane".into());
        }
        let terminal = pin.position_v1() as usize == pin.generated_ids_v1().len();
        let committed = pin.committed_id_v1().filter(|id| *id < binding.shape_profile.vocab_size);
        if committed.is_some() != pin.committed_key_v1().is_some() && !terminal {
            return Err("in-vocabulary Float32 selection lacks its authenticated order key".into());
        }
        let beat = pin.beat_v1();
        let beats = beat.zip(committed.zip(pin.committed_key_v1())).is_some_and(|((id, b), (committed, key))| {
            crate::palw_decode_select_v2::decode_lane_beats_v2(i64::from(b), id as usize, i64::from(key), committed as usize)
        });
        Self::from_authenticated_selection_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            binding,
            job,
            pin.generated_ids_v1(),
            pin.position_v1(),
            witness,
            beat.map(|b| b.0),
            beats,
            eog_count,
            eog_opening,
            signed_context,
            &borsh::to_vec(pin).map_err(|e| e.to_string())?,
            if terminal { Some(original.stop_reason) } else { None },
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn from_authenticated_selection_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        generated: &[u32],
        position: u32,
        witness: Option<u32>,
        beat: Option<u32>,
        beats: bool,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        signed_context: Option<Hash64>,
        canonical_pin: &[u8],
        terminal_stop: Option<PalwFpStopReasonV3>,
    ) -> Result<Self, String> {
        verify_binding_v1(binding).map_err(|e| e.to_string())?;
        Self::from_authenticated_selection_view_v1(
            claim_id,
            owner,
            reserved_daa,
            expected_root,
            &crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding),
            &borsh::to_vec(binding).map_err(|e| e.to_string())?,
            job,
            generated,
            position,
            witness,
            beat,
            beats,
            eog_count,
            eog_opening,
            signed_context,
            canonical_pin,
            terminal_stop,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn from_authenticated_selection_view_v1(
        claim_id: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        expected_root: Hash64,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        view_identity: &[u8],
        job: &PalwFreePromptJobV3,
        generated: &[u32],
        position: u32,
        witness: Option<u32>,
        beat: Option<u32>,
        beats: bool,
        eog_count: u32,
        eog_opening: &PalwStepOpeningV1,
        signed_context: Option<Hash64>,
        canonical_pin: &[u8],
        terminal_stop: Option<PalwFpStopReasonV3>,
    ) -> Result<Self, String> {
        let vocab_size = view.vocab_size;
        if view.committed_execution_root != expected_root
            || !job.is_constraint()
            || fp_job_id_v3(job) != view.job_context.job_id
            || generated.len() != view.job_context.exact_decode_tokens as usize
            || generated.len() > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || (generated.get(position as usize).is_none() && !(position as usize == generated.len() && terminal_stop.is_some()))
            || witness.is_some_and(|id| id >= vocab_size)
            || beat.is_some_and(|id| id >= vocab_size)
        {
            return Err("authenticated semantic selection changes original claim/job/position/vocabulary".into());
        }
        let constraint = palw_fp_constraint_shape_v1(job).map_err(|e| e.to_string())?;
        let table_root = palw_fp_constraint_table_root_v1(job).ok_or("stream job names no legacy table root")?;
        let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/constraint-stream/seed/v1").to_state();
        for hash in [claim_id, expected_root] {
            h.update(hash.as_byte_slice());
        }
        h.update(&borsh::to_vec(&owner).map_err(|e| e.to_string())?);
        h.update(&reserved_daa.to_le_bytes());
        // Canonical seed identity retains the actual pin, job and binding, not a caller-reencoded fault.
        h.update(view_identity);
        h.update(&borsh::to_vec(job).map_err(|e| e.to_string())?);
        h.update(canonical_pin);
        h.update(&borsh::to_vec(&witness).map_err(|e| e.to_string())?);
        h.update(&eog_count.to_le_bytes());
        h.update(&borsh::to_vec(eog_opening).map_err(|e| e.to_string())?);
        let context = signed_context.unwrap_or_else(|| Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap()));
        let mut relevant = generated.to_vec();
        relevant.extend(witness);
        relevant.extend(beat);
        // An original selected ID outside the registered vocabulary cannot be output by
        // the same selection kernel. No rendering, EOG membership or logit is required.
        let invalid = if terminal_stop.is_some() {
            generated.iter().position(|id| *id >= vocab_size).map(|p| p as u32)
        } else {
            generated.get(position as usize).filter(|id| **id >= vocab_size).map(|_| position)
        };
        let eog = if invalid.is_some() {
            None
        } else {
            Some(PalwEogStreamV1::new(context, table_root, vocab_size, eog_count, &relevant, eog_opening)?)
        };
        let prefix_state = constraint_start_state_v1(&constraint);
        Ok(Self {
            claim_id,
            owner,
            reserved_daa,
            root: expected_root,
            job_id: view.job_context.job_id,
            context,
            table_root,
            vocab: vocab_size,
            constraint,
            generated: generated.to_vec(),
            position,
            witness,
            beat,
            beats,
            phase: if invalid.is_some() { PalwConstraintStreamPhaseV1::Complete } else { PalwConstraintStreamPhaseV1::Eog },
            eog,
            lowest_eog: None,
            eog_members: Vec::new(),
            prefix_state,
            rendering: None,
            convicted: invalid.is_some(),
            terminal_stop,
            output_fault_position: invalid,
        })
    }
    pub fn seed_id(&self) -> Hash64 {
        self.context
    }
    pub fn execution_root_v1(&self) -> Hash64 {
        self.root
    }
    pub fn phase(&self) -> PalwConstraintStreamPhaseV1 {
        self.phase
    }
    pub fn eog_offset_v1(&self) -> Option<u32> {
        self.eog.as_ref().map(|e| e.offset())
    }
    pub fn rendering_cursor_v1(&self) -> Option<(u32, bool, u32, u32)> {
        self.rendering.as_ref().map(|r| r.cursor_v1())
    }
    pub fn push_eog(&mut self, seed_id: Hash64, offset: u32, ids: &[u32]) -> Result<(), String> {
        if self.phase != PalwConstraintStreamPhaseV1::Eog {
            return Err("EOG phase is closed".into());
        }
        self.eog.as_mut().ok_or("EOG continuation missing")?.push(seed_id, offset, ids)
    }
    pub fn finish_eog(&mut self) -> Result<(), String> {
        if self.phase != PalwConstraintStreamPhaseV1::Eog {
            return Err("EOG phase is closed".into());
        }
        let authenticated = self.eog.as_ref().ok_or("EOG continuation missing")?.finish()?;
        self.lowest_eog = Some(authenticated.lowest);
        self.eog_members = authenticated.relevant_members;
        self.eog = None;
        if let Some(stop) = self.terminal_stop {
            // The ORIGINAL constrained decoder stops only after committing its lowest
            // EOG or the budget. A prior EOG is the existing post-stop mismatch arm.
            let prior_stop = self.generated.iter().take(self.generated.len().saturating_sub(1)).position(|id| self.is_eog(*id));
            let premature = stop == PalwFpStopReasonV3::EndOfGeneration && self.generated.last().copied() != self.lowest_eog;
            self.output_fault_position =
                prior_stop.map(|p| p as u32).or_else(|| premature.then_some(self.generated.len().saturating_sub(1) as u32));
            self.convicted = self.output_fault_position.is_some();
            self.phase = PalwConstraintStreamPhaseV1::Complete;
            return Ok(());
        }
        self.phase = if self.position == 0 { PalwConstraintStreamPhaseV1::Committed } else { PalwConstraintStreamPhaseV1::Prefix(0) };
        Ok(())
    }
    pub(crate) fn is_eog(&self, id: u32) -> bool {
        self.eog_members.binary_search(&id).is_ok()
    }
    pub fn next_rendering_id(&self) -> Result<u32, String> {
        match self.phase {
            PalwConstraintStreamPhaseV1::Prefix(p) => {
                self.generated.get(p as usize).copied().ok_or_else(|| "prefix cursor outside trace".into())
            }
            PalwConstraintStreamPhaseV1::Committed => Ok(self.generated[self.position as usize]),
            PalwConstraintStreamPhaseV1::Auxiliary => {
                self.witness.or(self.beat).ok_or_else(|| "auxiliary selected lane missing".into())
            }
            _ => Err("court is not in a rendering phase".into()),
        }
    }
    /// Only the forced next ID may open; a first writer cannot substitute or skip a prefix.
    pub fn begin_rendering(
        &mut self,
        seed_id: Hash64,
        id: u32,
        present: bool,
        length: u32,
        opening: &PalwStepOpeningV1,
    ) -> Result<(), String> {
        if seed_id != self.context || self.rendering.is_some() || id != self.next_rendering_id()? {
            return Err("rendering declaration changes owned court/ordered token identity".into());
        }
        if matches!(self.phase, PalwConstraintStreamPhaseV1::Prefix(_)) && self.is_eog(id) {
            return Err("prefix contains an authenticated EOG; challenge that earlier position".into());
        }
        self.rendering = Some(PalwRenderingStreamV1::new(
            self.context,
            &self.constraint,
            Some(self.prefix_state.clone()),
            self.table_root,
            self.vocab,
            id,
            present,
            length,
            opening,
        )?);
        Ok(())
    }
    pub fn push_rendering(&mut self, seed_id: Hash64, offset: u32, bytes: &[u8]) -> Result<(), String> {
        self.rendering.as_mut().ok_or("rendering declaration missing")?.push_owned(seed_id, &self.constraint, offset, bytes)
    }
    pub fn finish_rendering(&mut self) -> Result<(), String> {
        let next = self.rendering.as_ref().ok_or("rendering declaration missing")?.finish()?;
        let id = self.next_rendering_id()?;
        let next = if self.is_eog(id) { None } else { next };
        self.apply_admission(id, next)?;
        self.rendering = None;
        Ok(())
    }
    /// EOG membership is already authenticated; its unrelated rendering leaf is unnecessary.
    pub fn finish_eog_token(&mut self, seed_id: Hash64) -> Result<(), String> {
        let id = self.next_rendering_id()?;
        if seed_id != self.context || self.rendering.is_some() || !self.is_eog(id) {
            return Err("selected token is not authenticated EOG or has a pending rendering".into());
        }
        self.apply_admission(id, None)
    }
    fn apply_admission(&mut self, id: u32, next: Option<PalwConstraintStateV1>) -> Result<(), String> {
        match self.phase {
            PalwConstraintStreamPhaseV1::Prefix(p) => {
                let next = next.ok_or("prefix is not admitted; challenge that earlier position")?;
                self.prefix_state = next;
                self.phase = if p + 1 == self.position {
                    PalwConstraintStreamPhaseV1::Committed
                } else {
                    PalwConstraintStreamPhaseV1::Prefix(p + 1)
                };
            }
            PalwConstraintStreamPhaseV1::Committed => {
                if self.is_eog(id) && (self.position as usize) + 1 < self.generated.len() {
                    self.convicted = true;
                    self.phase = PalwConstraintStreamPhaseV1::Complete;
                } else if next.is_none() {
                    if Some(id) != self.lowest_eog {
                        self.convicted = true;
                        self.phase = PalwConstraintStreamPhaseV1::Complete;
                    } else if self.witness.is_some() {
                        self.phase = PalwConstraintStreamPhaseV1::Auxiliary;
                    } else {
                        self.phase = PalwConstraintStreamPhaseV1::Complete;
                    }
                } else if self.beat.is_some() {
                    // A witness is for the stop arm only; the admitted-lane arm must use beat.
                    self.witness = None;
                    self.phase = PalwConstraintStreamPhaseV1::Auxiliary;
                } else {
                    self.phase = PalwConstraintStreamPhaseV1::Complete;
                }
            }
            PalwConstraintStreamPhaseV1::Auxiliary => {
                self.convicted = next.is_some() && (self.witness.is_some() || self.beats);
                self.phase = PalwConstraintStreamPhaseV1::Complete;
            }
            _ => return Err("court has no pending rendering phase".into()),
        }
        Ok(())
    }
    /// Fault scope is small and objective; malformed or incomplete material cannot return one.
    pub fn fault(&self) -> Result<Option<PalwConstraintStreamFaultV1>, String> {
        if self.phase != PalwConstraintStreamPhaseV1::Complete || self.rendering.is_some() {
            return Err("stream court is incomplete".into());
        }
        let position = self.output_fault_position.unwrap_or(self.position);
        Ok(self.convicted.then_some(PalwConstraintStreamFaultV1 {
            claim_id: self.claim_id,
            reservation_owner: self.owner,
            reserved_daa: self.reserved_daa,
            execution_root: self.root,
            job_id: self.job_id,
            seed_id: self.context,
            position,
            fault: PalwStepFaultV1::DecodeTokenMismatch { position },
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1;
    fn owner() -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(2), index: 0 })
    }
    fn court(fault: bool) -> (PalwConstraintStreamCourtV1, crate::palw_fp_constraint_job_v1::PalwConstrainedDecodeAccusationV1) {
        let (binding, accusation) = public_constrained_case_for_tests_v1(fault);
        let row = PalwConstraintStreamCourtV1::new(
            Hash64::from_u64_word(1),
            owner(),
            100,
            binding.committed_execution_root,
            &binding,
            &accusation.job,
            &accusation.pin,
            accusation.witness,
            accusation.eog.ids.len() as u32,
            &accusation.eog.opening,
            1 << 25,
        )
        .unwrap();
        (row, accusation)
    }
    fn complete(row: &mut PalwConstraintStreamCourtV1, a: &crate::palw_fp_constraint_job_v1::PalwConstrainedDecodeAccusationV1) {
        let context = row.seed_id();
        row.push_eog(context, 0, &a.eog.ids).unwrap();
        row.finish_eog().unwrap();
        while row.phase() != PalwConstraintStreamPhaseV1::Complete {
            let id = row.next_rendering_id().unwrap();
            if row.is_eog(id) {
                row.finish_eog_token(context).unwrap();
                continue;
            }
            let o = a.renderings.iter().find(|o| o.id == id).unwrap();
            row.begin_rendering(context, id, o.rendering.is_some(), o.rendering.as_ref().map_or(0, |b| b.len()) as u32, &o.opening)
                .unwrap();
            if let Some(bytes) = &o.rendering {
                for (n, part) in bytes.chunks(65536).enumerate() {
                    row.push_rendering(context, (n * 65536) as u32, part).unwrap();
                }
            }
            row.finish_rendering().unwrap();
            let bytes = borsh::to_vec(row).unwrap();
            assert!(bytes.len() < 350_000);
            *row = borsh::from_slice(&bytes).unwrap();
        }
    }
    #[test]
    fn ordered_public_stream_matches_direct_court_fault_and_honest_refusal() {
        for fault in [false, true] {
            let (mut row, a) = court(fault);
            assert!(row.fault().is_err());
            complete(&mut row, &a);
            let outcome = row.fault().unwrap();
            assert_eq!(outcome.is_some(), fault);
            if let Some(cert) = outcome {
                assert_eq!(cert.position, a.pin.position);
                assert_eq!(cert.seed_id, row.seed_id());
                assert_eq!(cert.fault, PalwStepFaultV1::DecodeTokenMismatch { position: 1 });
            }
        }
    }
    #[test]
    fn wrong_seed_root_skip_corrupt_prefix_and_incomplete_leaf_never_convict() {
        let (mut row, a) = court(true);
        let context = row.seed_id();
        let before = row.clone();
        assert!(row.push_eog(Hash64::from_u64_word(99), 0, &a.eog.ids).is_err());
        assert_eq!(row, before);
        row.push_eog(context, 0, &a.eog.ids).unwrap();
        row.finish_eog().unwrap();
        let id = row.next_rendering_id().unwrap();
        let o = a.renderings.iter().find(|o| o.id == id).unwrap();
        let before = row.clone();
        assert!(row.begin_rendering(context, id + 1, true, 1, &o.opening).is_err());
        assert_eq!(row, before);
        row.begin_rendering(context, id, true, o.rendering.as_ref().unwrap().len() as u32, &o.opening).unwrap();
        assert!(row.finish_rendering().is_err());
        assert!(row.fault().is_err());
        let bytes = vec![b'x'; o.rendering.as_ref().unwrap().len()];
        row.push_rendering(context, 0, &bytes).unwrap();
        let before = row.clone();
        assert!(row.finish_rendering().is_err());
        assert_eq!(row, before);
        assert!(row.fault().is_err());
        let (binding, a) = public_constrained_case_for_tests_v1(true);
        assert!(
            PalwConstraintStreamCourtV1::new(
                Hash64::default(),
                owner(),
                0,
                Hash64::default(),
                &binding,
                &a.job,
                &a.pin,
                None,
                a.eog.ids.len() as u32,
                &a.eog.opening,
                1 << 25
            )
            .is_err()
        );
    }
}
