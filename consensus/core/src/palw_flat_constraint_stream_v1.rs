//! Bounded acquisition of the ORIGINAL flat integer logits commitment. Tentative
//! IDs and selected scalars remain inaccessible until authenticated EOF. A guessed
//! committed lane is only an accelerator: a differing authenticated ID starts an
//! independently authenticated second pass instead of trusting a replay prediction.
use crate::{
    Hash64,
    palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1},
    palw_rendering_stream_v1::LeafHashContinuationV1,
    palw_step_leg::{PalwStepBindingV2, verify_binding_v1},
    palw_step_refute::{PALW_BASE0_DOMAIN_LOGITS_TRACE, flat_logits_scheme_id_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_FLAT_CONSTRAINT_PART_WORDS_V1: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub enum PalwFlatConstraintActionV1 {
    RowBegin { row: u32, width: u32 },
    RowPart { row: u32, offset: u32, values: Vec<i32> },
    IdsPart { offset: u32, ids: Vec<u32> },
    Finish,
}
impl PalwFlatConstraintActionV1 {
    pub fn check_wire_v1(&self) -> Result<(), String> {
        let length = match self {
            Self::RowPart { values, .. } => values.len(),
            Self::IdsPart { ids, .. } => ids.len(),
            _ => return Ok(()),
        };
        if length == 0 || length > PALW_FLAT_CONSTRAINT_PART_WORDS_V1 {
            return Err("flat acquisition part exceeds its bounded original-word envelope".into());
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub enum PalwFlatConstraintCursorV1 {
    RowBegin { pass: u8, row: u32 },
    RowPart { pass: u8, row: u32, offset: u32 },
    Ids { pass: u8, offset: u32 },
    Finish { pass: u8 },
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwFlatConstraintStreamV1 {
    context: Hash64,
    binding_root: Hash64,
    job_context: Hash64,
    expected: Hash64,
    prefill: u32,
    decode: u32,
    vocab: u32,
    position: u32,
    committed_lane: Option<u32>,
    beat_lane: Option<u32>,
    committed_value: Option<i32>,
    beat_value: Option<i32>,
    ids: Vec<u32>,
    authenticated_ids: Option<Vec<u32>>,
    cursor: PalwFlatConstraintCursorV1,
    hash: LeafHashContinuationV1,
    moves: u64,
    maximum_moves: u64,
}

/// Construction through this stream is not admission authority. Native users must
/// obtain it from their rooted accepted stream, not deserialize a private checkpoint.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwFlatConstraintPinV1 {
    binding_root: Hash64,
    job_context: Hash64,
    logits_root: Hash64,
    vocab: u32,
    generated: Vec<u32>,
    position: u32,
    committed_value: Option<i32>,
    beat: Option<(u32, i32)>,
}
impl PalwFlatConstraintPinV1 {
    pub fn generated_ids_v1(&self) -> &[u32] {
        &self.generated
    }
    pub fn position_v1(&self) -> u32 {
        self.position
    }
    pub fn committed_id_v1(&self) -> Option<u32> {
        self.generated.get(self.position as usize).copied()
    }
    pub fn committed_value_v1(&self) -> Option<i32> {
        self.committed_value
    }
    pub fn beat_v1(&self) -> Option<(u32, i32)> {
        self.beat
    }
    pub fn matches_binding_v1(&self, binding: &PalwStepBindingV2) -> bool {
        binding.shape_profile.logits_scheme_id == flat_logits_scheme_id_v1()
            && binding.committed_execution_root == self.binding_root
            && binding.job_context.context_hash() == self.job_context
            && binding.full_logits_trace_root == self.logits_root
            && binding.shape_profile.vocab_size == self.vocab
            && binding.job_context.exact_decode_tokens as usize == self.generated.len()
    }
}
impl PalwFlatConstraintStreamV1 {
    pub fn new_v1(
        context: Hash64,
        binding: &PalwStepBindingV2,
        position: u32,
        guessed_committed: Option<u32>,
        beat: Option<u32>,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        verify_binding_v1(binding).map_err(|e| e.to_string())?;
        if binding.shape_profile.logits_scheme_id != flat_logits_scheme_id_v1() {
            return Err("flat acquisition cannot reinterpret a registered tiled or floating scheme".into());
        }
        Self::new_inner(
            context,
            binding.committed_execution_root,
            binding.job_context.context_hash(),
            binding.full_logits_trace_root,
            binding.job_context.declared_prefill_tokens,
            binding.job_context.exact_decode_tokens,
            binding.shape_profile.vocab_size,
            position,
            guessed_committed,
            beat,
            prices,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn new_inner(
        context: Hash64,
        binding_root: Hash64,
        job_context: Hash64,
        expected: Hash64,
        prefill: u32,
        decode: u32,
        vocab: u32,
        position: u32,
        guessed_committed: Option<u32>,
        beat: Option<u32>,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        // This is the existing native event-count domain, not a new vocabulary cap.
        // All u32 vocabulary widths, including u32::MAX, remain representable.
        if decode as usize > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || vocab == 0
            || position > decode
            || beat.is_some_and(|id| id >= vocab)
            || (position == decode && beat.is_some())
        {
            return Err("flat source changes original event/selection geometry".into());
        }
        let plan = flat_constraint_resource_plan_v1(decode, vocab, prices)?;
        let mut row = Self {
            context,
            binding_root,
            job_context,
            expected,
            prefill,
            decode,
            vocab,
            position,
            committed_lane: guessed_committed.filter(|id| *id < vocab),
            beat_lane: beat,
            committed_value: None,
            beat_value: None,
            ids: Vec::new(),
            authenticated_ids: None,
            cursor: PalwFlatConstraintCursorV1::Complete,
            hash: LeafHashContinuationV1::new(PALW_BASE0_DOMAIN_LOGITS_TRACE),
            moves: 0,
            maximum_moves: plan.maximum_moves,
        };
        row.restart_pass(0);
        Ok(row)
    }
    fn restart_pass(&mut self, pass: u8) {
        self.hash = LeafHashContinuationV1::new(PALW_BASE0_DOMAIN_LOGITS_TRACE);
        self.hash.update(self.job_context.as_byte_slice());
        self.hash.update(&u64::from(self.prefill).to_le_bytes());
        self.hash.update(&u64::from(self.decode).to_le_bytes());
        self.hash.update(&u64::from(self.decode).to_le_bytes());
        self.ids.clear();
        self.committed_value = None;
        self.beat_value = None;
        self.cursor = if self.decode == 0 {
            self.hash.update(&0u64.to_le_bytes());
            PalwFlatConstraintCursorV1::Finish { pass }
        } else {
            PalwFlatConstraintCursorV1::RowBegin { pass, row: 0 }
        };
    }
    pub fn cursor_v1(&self) -> PalwFlatConstraintCursorV1 {
        self.cursor
    }
    pub fn check_v1(&self) -> Result<(), String> {
        if self.vocab == 0
            || self.decode as usize > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || self.position > self.decode
            || self.ids.len() > self.decode as usize
            || self.authenticated_ids.as_ref().is_some_and(|v| v.len() != self.decode as usize)
            || self.moves > self.maximum_moves
            || self.beat_lane.is_some_and(|id| id >= self.vocab)
            || self.committed_lane.is_some_and(|id| id >= self.vocab)
        {
            return Err("flat continuation violates its bounded original metadata".into());
        }
        let base = 88u128;
        let row_bytes = 8 + u128::from(self.vocab) * 4;
        let (pass, bytes) = match self.cursor {
            PalwFlatConstraintCursorV1::RowBegin { pass, row } if row < self.decode && self.ids.is_empty() => {
                (pass, base + u128::from(row) * row_bytes)
            }
            PalwFlatConstraintCursorV1::RowPart { pass, row, offset }
                if row < self.decode && offset < self.vocab && self.ids.is_empty() =>
            {
                (pass, base + u128::from(row) * row_bytes + 8 + u128::from(offset) * 4)
            }
            PalwFlatConstraintCursorV1::Ids { pass, offset } if offset < self.decode && self.ids.len() == offset as usize => {
                (pass, base + u128::from(self.decode) * row_bytes + 8 + u128::from(offset) * 4)
            }
            PalwFlatConstraintCursorV1::Finish { pass } if self.ids.len() == self.decode as usize => {
                (pass, base + u128::from(self.decode) * row_bytes + 8 + u128::from(self.decode) * 4)
            }
            PalwFlatConstraintCursorV1::Complete
                if self.ids.len() == self.decode as usize && self.authenticated_ids.as_ref() == Some(&self.ids) =>
            {
                (0, base + u128::from(self.decode) * row_bytes + 8 + u128::from(self.decode) * 4)
            }
            _ => return Err("flat continuation cursor/word count is noncanonical".into()),
        };
        if pass > 1 || (pass == 1 && self.authenticated_ids.is_none()) || self.hash.byte_count() != Some(128 + bytes) {
            return Err("flat continuation changed original hash byte position".into());
        }
        Ok(())
    }
    pub fn movement_v1(&mut self, context: Hash64, action: &PalwFlatConstraintActionV1) -> Result<(), String> {
        self.check_v1()?;
        action.check_wire_v1()?;
        if context != self.context || self.moves >= self.maximum_moves {
            return Err("flat move changes immutable owner context or paid work".into());
        }
        match (self.cursor, action) {
            (PalwFlatConstraintCursorV1::RowBegin { pass, row }, PalwFlatConstraintActionV1::RowBegin { row: at, width })
                if row == *at && *width == self.vocab =>
            {
                self.hash.update(&u64::from(self.vocab).to_le_bytes());
                self.cursor = PalwFlatConstraintCursorV1::RowPart { pass, row, offset: 0 };
            }
            (
                PalwFlatConstraintCursorV1::RowPart { pass, row, offset },
                PalwFlatConstraintActionV1::RowPart { row: at, offset: begin, values },
            ) if row == *at && offset == *begin => {
                let end = offset
                    .checked_add(u32::try_from(values.len()).map_err(|_| "flat word count overflow")?)
                    .ok_or("flat lane end overflow")?;
                if end > self.vocab {
                    return Err("flat part exceeds original row width".into());
                }
                for (i, value) in values.iter().enumerate() {
                    let lane = offset + i as u32;
                    self.hash.update(&value.to_le_bytes());
                    if row == self.position {
                        if self.committed_lane == Some(lane) {
                            self.committed_value = Some(*value);
                        }
                        if self.beat_lane == Some(lane) {
                            self.beat_value = Some(*value);
                        }
                    }
                }
                self.cursor = if end < self.vocab {
                    PalwFlatConstraintCursorV1::RowPart { pass, row, offset: end }
                } else if row + 1 < self.decode {
                    PalwFlatConstraintCursorV1::RowBegin { pass, row: row + 1 }
                } else {
                    self.hash.update(&u64::from(self.decode).to_le_bytes());
                    PalwFlatConstraintCursorV1::Ids { pass, offset: 0 }
                };
            }
            (PalwFlatConstraintCursorV1::Ids { pass, offset }, PalwFlatConstraintActionV1::IdsPart { offset: at, ids })
                if offset == *at =>
            {
                let end = offset
                    .checked_add(u32::try_from(ids.len()).map_err(|_| "flat ID count overflow")?)
                    .ok_or("flat ID end overflow")?;
                if end > self.decode {
                    return Err("flat IDs exceed original generated count".into());
                }
                if pass == 1 && self.authenticated_ids.as_ref().is_none_or(|prior| prior[offset as usize..end as usize] != ids[..]) {
                    return Err("second flat pass substitutes authenticated generated IDs".into());
                }
                for id in ids {
                    self.hash.update(&id.to_le_bytes());
                }
                self.ids.extend_from_slice(ids);
                self.cursor = if end == self.decode {
                    PalwFlatConstraintCursorV1::Finish { pass }
                } else {
                    PalwFlatConstraintCursorV1::Ids { pass, offset: end }
                };
            }
            (PalwFlatConstraintCursorV1::Finish { pass }, PalwFlatConstraintActionV1::Finish) => {
                if self.hash.finish() != self.expected {
                    return Err("flat original hash does not authenticate at exact EOF".into());
                }
                let actual = self.ids.get(self.position as usize).copied();
                self.authenticated_ids = Some(self.ids.clone());
                if pass == 0 && actual.is_some_and(|id| id < self.vocab && self.committed_lane != Some(id)) {
                    self.committed_lane = actual;
                    self.restart_pass(1);
                } else {
                    self.cursor = PalwFlatConstraintCursorV1::Complete;
                }
            }
            _ => return Err("flat action changes exact original stream cursor".into()),
        }
        self.moves += 1;
        self.check_v1()
    }
    pub fn authenticated_pin_v1(&self) -> Result<PalwFlatConstraintPinV1, String> {
        self.check_v1()?;
        if self.cursor != PalwFlatConstraintCursorV1::Complete || self.hash.finish() != self.expected {
            return Err("flat acquisition exposes no semantic pin before authenticated EOF".into());
        }
        let committed = self.ids.get(self.position as usize).copied();
        if committed.is_some_and(|id| id < self.vocab && (self.committed_lane != Some(id) || self.committed_value.is_none()))
            || (committed.is_some() && self.beat_lane.is_some() && self.beat_value.is_none())
        {
            return Err("flat acquisition lacks authenticated selected scalars".into());
        }
        Ok(PalwFlatConstraintPinV1 {
            binding_root: self.binding_root,
            job_context: self.job_context,
            logits_root: self.expected,
            vocab: self.vocab,
            generated: self.ids.clone(),
            position: self.position,
            committed_value: committed.filter(|id| *id < self.vocab).and(self.committed_value),
            beat: self.beat_lane.zip(self.beat_value),
        })
    }
}

/// Acquires up to two full ORIGINAL preimages, without allocating a vocabulary row.
/// This supplements the table/grammar semantic plan; it does not shrink that format.
pub fn flat_constraint_resource_plan_v1(
    decode: u32,
    vocab: u32,
    prices: PalwRenderingResourcePricesV1,
) -> Result<PalwRenderingResourcePlanV1, String> {
    if decode as usize > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS || vocab == 0 || prices.inclusion_margin_daa == 0 {
        return Err("flat quote requires original native event/vocabulary domain and positive inclusion margin".into());
    }
    let d = u128::from(decode);
    let v = u128::from(vocab);
    let original_bytes = 96 + d * (12 + v * 4);
    let moves = 2
        * (d * (1 + v.div_ceil(PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u128))
            + d.div_ceil(PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u128)
            + 1)
        + u128::from(prices.reorg_replay_moves);
    let row_bytes = 2048 + d * 8;
    let carrier_bytes = original_bytes * 2 + moves * 5632;
    let narrow = |v: u128| u64::try_from(v).map_err(|_| "flat acquisition quote exceeds exact u64 accounting".to_string());
    Ok(PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab,
        maximum_stream_bytes: narrow(original_bytes * 2)?,
        maximum_stream_leaves: 1,
        maximum_eog_bytes: 0,
        maximum_moves: narrow(moves)?,
        maximum_parser_steps: 0,
        maximum_blake_blocks: narrow(2 * (original_bytes.div_ceil(128) + 1))?,
        inclusion_daa: narrow(moves * u128::from(prices.inclusion_margin_daa))?,
        rent_sompi: narrow(
            carrier_bytes * u128::from(prices.rent_per_stream_byte) + row_bytes * u128::from(prices.rent_per_row_byte),
        )?,
        deposit_sompi: narrow(moves * u128::from(prices.deposit_per_move))?,
        row_bytes: narrow(row_bytes)?,
        resident_ram_bytes: narrow(row_bytes * 2 + PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u128 * 8 + 4096)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_step_refute::PalwBase0DecodeTokensV1;
    fn prices() -> PalwRenderingResourcePricesV1 {
        crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices
    }
    fn fixture() -> (PalwStepBindingV2, PalwBase0DecodeTokensV1, u32) {
        let (binding, a) = crate::palw_decode_constraint_v1::tests::public_flat_constrained_case_for_tests_v1(false);
        (binding, a.pin, a.position)
    }
    fn next(row: &PalwFlatConstraintStreamV1, source: &PalwBase0DecodeTokensV1) -> PalwFlatConstraintActionV1 {
        match row.cursor_v1() {
            PalwFlatConstraintCursorV1::RowBegin { row, .. } => {
                PalwFlatConstraintActionV1::RowBegin { row, width: source.logits_rows[row as usize].len() as u32 }
            }
            PalwFlatConstraintCursorV1::RowPart { row, offset, .. } => {
                let values = &source.logits_rows[row as usize];
                PalwFlatConstraintActionV1::RowPart {
                    row,
                    offset,
                    values: values[offset as usize..values.len().min(offset as usize + PALW_FLAT_CONSTRAINT_PART_WORDS_V1)].to_vec(),
                }
            }
            PalwFlatConstraintCursorV1::Ids { offset, .. } => PalwFlatConstraintActionV1::IdsPart {
                offset,
                ids: source.generated_token_ids
                    [offset as usize..source.generated_token_ids.len().min(offset as usize + PALW_FLAT_CONSTRAINT_PART_WORDS_V1)]
                    .to_vec(),
            },
            PalwFlatConstraintCursorV1::Finish { .. } => PalwFlatConstraintActionV1::Finish,
            _ => panic!("no movement after complete"),
        }
    }
    fn finish(row: &mut PalwFlatConstraintStreamV1, source: &PalwBase0DecodeTokensV1) {
        let context = row.context;
        for _ in 0..1000 {
            if row.cursor == PalwFlatConstraintCursorV1::Complete {
                return;
            }
            row.movement_v1(context, &next(row, source)).unwrap();
        }
        panic!("small original source failed to finish");
    }
    #[test]
    fn original_flat_hash_streams_across_restart_then_exposes_only_exact_generated_ids_and_selected_scalars() {
        let (binding, source, position) = fixture();
        let context = Hash64::from_u64_word(71);
        let committed = source.generated_token_ids[position as usize];
        let beat = (committed + 1) % binding.shape_profile.vocab_size;
        let mut row = PalwFlatConstraintStreamV1::new_v1(context, &binding, position, Some(committed), Some(beat), prices()).unwrap();
        assert!(row.authenticated_pin_v1().is_err());
        row.movement_v1(context, &next(&row, &source)).unwrap();
        row.movement_v1(context, &next(&row, &source)).unwrap();
        let mut restarted: PalwFlatConstraintStreamV1 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        restarted.check_v1().unwrap();
        assert!(restarted.authenticated_pin_v1().is_err());
        finish(&mut restarted, &source);
        let pin = restarted.authenticated_pin_v1().unwrap();
        assert!(pin.matches_binding_v1(&binding));
        assert_eq!(pin.generated_ids_v1(), source.generated_token_ids);
        assert_eq!(pin.committed_id_v1(), Some(committed));
        assert_eq!(pin.committed_value_v1(), Some(source.logits_rows[position as usize][committed as usize]));
        assert_eq!(pin.beat_v1(), Some((beat, source.logits_rows[position as usize][beat as usize])));
    }
    #[test]
    fn wrong_replay_guess_cannot_authenticate_a_committed_scalar_and_forces_original_second_pass() {
        let (binding, source, position) = fixture();
        let context = Hash64::from_u64_word(73);
        let committed = source.generated_token_ids[position as usize];
        let mut row = PalwFlatConstraintStreamV1::new_v1(
            context,
            &binding,
            position,
            Some((committed + 1) % binding.shape_profile.vocab_size),
            None,
            prices(),
        )
        .unwrap();
        while !matches!(row.cursor, PalwFlatConstraintCursorV1::Finish { pass: 0 }) {
            row.movement_v1(context, &next(&row, &source)).unwrap();
        }
        row.movement_v1(context, &PalwFlatConstraintActionV1::Finish).unwrap();
        assert!(matches!(row.cursor, PalwFlatConstraintCursorV1::RowBegin { pass: 1, row: 0 }));
        assert!(row.authenticated_pin_v1().is_err());
        finish(&mut row, &source);
        assert_eq!(
            row.authenticated_pin_v1().unwrap().committed_value_v1(),
            Some(source.logits_rows[position as usize][committed as usize])
        );
    }
    #[test]
    fn stale_scope_part_order_wrong_original_lane_and_late_bad_eof_never_become_semantic_evidence() {
        let (binding, mut source, position) = fixture();
        let context = Hash64::from_u64_word(75);
        let mut row = PalwFlatConstraintStreamV1::new_v1(context, &binding, position, None, None, prices()).unwrap();
        let initial = row.clone();
        assert!(row.movement_v1(Hash64::from_u64_word(99), &next(&row, &source)).is_err());
        assert_eq!(row, initial);
        assert!(row.movement_v1(context, &PalwFlatConstraintActionV1::Finish).is_err());
        row.movement_v1(context, &next(&row, &source)).unwrap();
        assert!(row.movement_v1(context, &PalwFlatConstraintActionV1::RowPart { row: 0, offset: 1, values: vec![0] }).is_err());
        assert!(
            row.movement_v1(
                context,
                &PalwFlatConstraintActionV1::RowPart { row: 0, offset: 0, values: vec![0; PALW_FLAT_CONSTRAINT_PART_WORDS_V1 + 1] }
            )
            .is_err()
        );
        source.logits_rows[0][0] ^= 1;
        while !matches!(row.cursor, PalwFlatConstraintCursorV1::Finish { .. }) {
            row.movement_v1(context, &next(&row, &source)).unwrap();
        }
        assert!(row.movement_v1(context, &PalwFlatConstraintActionV1::Finish).is_err());
        assert!(row.authenticated_pin_v1().is_err());
    }
    #[test]
    fn maximum_u32_vocabulary_keeps_original_format_without_vocabulary_resident_allocation() {
        let plan = flat_constraint_resource_plan_v1(crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS as u32, u32::MAX, prices()).unwrap();
        assert!(plan.maximum_stream_bytes > u64::from(u32::MAX));
        assert!(plan.resident_ram_bytes < 1 << 20);
        let context = Hash64::from_u64_word(77);
        let mut row = PalwFlatConstraintStreamV1::new_inner(
            context,
            context,
            context,
            context,
            0,
            1,
            u32::MAX,
            0,
            None,
            Some(u32::MAX - 1),
            prices(),
        )
        .unwrap();
        row.movement_v1(context, &PalwFlatConstraintActionV1::RowBegin { row: 0, width: u32::MAX }).unwrap();
        row.movement_v1(
            context,
            &PalwFlatConstraintActionV1::RowPart { row: 0, offset: 0, values: vec![0; PALW_FLAT_CONSTRAINT_PART_WORDS_V1] },
        )
        .unwrap();
        assert!(borsh::object_length(&row).unwrap() < 2048);
        assert!(row.authenticated_pin_v1().is_err(), "maximum metadata exercise is not a completed enormous preimage trial");
    }
    #[test]
    fn empty_original_trace_can_authenticate_a_terminal_scope_without_inventing_a_logit_row() {
        let context = Hash64::from_u64_word(79);
        let mut hash = LeafHashContinuationV1::new(PALW_BASE0_DOMAIN_LOGITS_TRACE);
        hash.update(context.as_byte_slice());
        for _ in 0..4 {
            hash.update(&0u64.to_le_bytes());
        }
        let mut row =
            PalwFlatConstraintStreamV1::new_inner(context, context, context, hash.finish(), 0, 0, 1, 0, None, None, prices()).unwrap();
        row.movement_v1(context, &PalwFlatConstraintActionV1::Finish).unwrap();
        let pin = row.authenticated_pin_v1().unwrap();
        assert!(pin.generated_ids_v1().is_empty());
        assert_eq!(pin.committed_id_v1(), None);
        assert_eq!(pin.committed_value_v1(), None);
    }
    #[test]
    fn authenticated_flat_pin_uses_original_constraint_table_eog_and_refutation_kernel() {
        use crate::{
            palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1, PalwConstraintStreamPhaseV1},
            palw_state_v2::PalwBondKeyV2,
            tx::{TransactionId, TransactionOutpoint},
        };
        for faulty in [false, true] {
            let (binding, proof) = crate::palw_decode_constraint_v1::tests::public_flat_constrained_case_for_tests_v1(faulty);
            let context = Hash64::from_u64_word(81);
            let mut source =
                PalwFlatConstraintStreamV1::new_v1(context, &binding, proof.position, None, proof.beat_lane, prices()).unwrap();
            finish(&mut source, &proof.pin);
            let pin = source.authenticated_pin_v1().unwrap();
            let owner = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(82), index: 0 });
            let mut court = PalwConstraintStreamCourtV1::new_from_flat_pin_v1(
                Hash64::from_u64_word(83),
                owner,
                100,
                binding.committed_execution_root,
                &binding,
                &proof.job,
                &pin,
                proof.witness,
                proof.eog.ids.len() as u32,
                &proof.eog.opening,
            )
            .unwrap();
            let seed = court.seed_id();
            court.push_eog(seed, 0, &proof.eog.ids).unwrap();
            court.finish_eog().unwrap();
            while court.phase() != PalwConstraintStreamPhaseV1::Complete {
                let id = court.next_rendering_id().unwrap();
                if court.is_eog(id) {
                    court.finish_eog_token(seed).unwrap();
                    continue;
                }
                let opening = proof.renderings.iter().find(|o| o.id == id).unwrap();
                let bytes = opening.rendering.as_deref().unwrap_or_default();
                court.begin_rendering(seed, id, opening.rendering.is_some(), bytes.len() as u32, &opening.opening).unwrap();
                for (n, part) in bytes.chunks(crate::palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1).enumerate() {
                    court
                        .push_rendering(seed, (n * crate::palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1) as u32, part)
                        .unwrap();
                }
                court.finish_rendering().unwrap();
            }
            assert_eq!(court.fault().unwrap().is_some(), faulty);
            assert_eq!(
                crate::palw_step_refute::check_constrained_flat_decode_token_at_v2(&binding, &proof, 1 << 25, true).is_ok(),
                faulty
            );
            assert!(
                PalwConstraintStreamCourtV1::new_from_flat_pin_v1(
                    Hash64::from_u64_word(83),
                    owner,
                    100,
                    Hash64::from_u64_word(999),
                    &binding,
                    &proof.job,
                    &pin,
                    proof.witness,
                    proof.eog.ids.len() as u32,
                    &proof.eog.opening
                )
                .is_err()
            );
        }
    }
}
