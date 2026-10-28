//! Bounded acquisition of the ORIGINAL flat integer logits commitment. Tentative
//! IDs and selected scalars remain inaccessible until authenticated EOF. A guessed
//! committed lane is only an accelerator: a differing authenticated ID starts an
//! independently authenticated second pass instead of trusting a replay prediction.
use crate::{
    Hash64,
    palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1},
    palw_rendering_stream_v1::LeafHashContinuationV1,
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::PalwStepBindingV2,
    palw_step_refute::{PALW_BASE0_DOMAIN_LOGITS_TRACE, flat_logits_scheme_id_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_FLAT_CONSTRAINT_PART_WORDS_V1: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PalwFlatConstraintActionV1 {
    RowBegin { row: u32, width: u32 },
    RowPart { row: u32, offset: u32, values: Vec<i32> },
    IdsPart { offset: u32, ids: Vec<u32> },
    Finish,
    Nominate { position: u32, beat: Option<u32> },
}
impl BorshDeserialize for PalwFlatConstraintActionV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        fn words<T: BorshDeserialize, R: std::io::Read>(reader: &mut R) -> std::io::Result<Vec<T>> {
            let count = u32::deserialize_reader(reader)? as usize;
            if count == 0 || count > PALW_FLAT_CONSTRAINT_PART_WORDS_V1 {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Flat part word count exceeds paid carrier"));
            }
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(T::deserialize_reader(reader)?);
            }
            Ok(values)
        }
        match u8::deserialize_reader(reader)? {
            0 => Ok(Self::RowBegin { row: u32::deserialize_reader(reader)?, width: u32::deserialize_reader(reader)? }),
            1 => Ok(Self::RowPart {
                row: u32::deserialize_reader(reader)?,
                offset: u32::deserialize_reader(reader)?,
                values: words(reader)?,
            }),
            2 => Ok(Self::IdsPart { offset: u32::deserialize_reader(reader)?, ids: words(reader)? }),
            3 => Ok(Self::Finish),
            4 => Ok(Self::Nominate { position: u32::deserialize_reader(reader)?, beat: Option::<u32>::deserialize_reader(reader)? }),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown Flat part variant")),
        }
    }
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
    Nominate,
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
    deferred: bool,
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
        self.matches_view_v1(&crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding))
    }
    /// The native caller derives this view from the accepted original header and
    /// registered class. A deserialized/private pin is not admission authority.
    pub fn matches_view_v1(&self, view: &crate::palw_decode_view_v1::PalwDecodeViewV1) -> bool {
        view.logits_scheme_id == flat_logits_scheme_id_v1()
            && view.lane == crate::palw_step::PalwStepLaneV1::Int32
            && view.committed_execution_root == self.binding_root
            && view.job_context.context_hash() == self.job_context
            && view.full_logits_trace_root == self.logits_root
            && view.vocab_size == self.vocab
            && view.job_context.exact_decode_tokens as usize == self.generated.len()
    }
}
impl PalwFlatConstraintStreamV1 {
    pub fn new_deferred_v1(
        context: Hash64,
        binding: &PalwStepBindingV2,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_binding_v1(binding)?;
        Self::new_deferred_view_v1(context, &view, prices)
    }
    pub fn new_deferred_view_v1(
        context: Hash64,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        let mut row = Self::new_view_v1(context, view, 0, None, None, prices)?;
        row.deferred = true;
        row.position = u32::MAX;
        Ok(row)
    }
    pub fn new_v1(
        context: Hash64,
        binding: &PalwStepBindingV2,
        position: u32,
        guessed_committed: Option<u32>,
        beat: Option<u32>,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_binding_v1(binding)?;
        Self::new_view_v1(context, &view, position, guessed_committed, beat, prices)
    }
    /// Uses the actual Flat trace domain for both legacy and TIR executions. The
    /// view must be derived natively; no legacy execution root is synthesized.
    pub fn new_view_v1(
        context: Hash64,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        position: u32,
        guessed_committed: Option<u32>,
        beat: Option<u32>,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        if view.logits_scheme_id != flat_logits_scheme_id_v1() || view.lane != crate::palw_step::PalwStepLaneV1::Int32 {
            return Err("flat acquisition cannot reinterpret a registered tiled or floating scheme".into());
        }
        Self::new_inner(
            context,
            view.committed_execution_root,
            view.job_context.context_hash(),
            view.full_logits_trace_root,
            view.job_context.declared_prefill_tokens,
            view.job_context.exact_decode_tokens,
            view.vocab_size,
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
            deferred: false,
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
            || (self.position > self.decode && !(self.deferred && self.position == u32::MAX))
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
            PalwFlatConstraintCursorV1::Nominate
                if self.deferred
                    && self.position == u32::MAX
                    && self.ids.len() == self.decode as usize
                    && self.authenticated_ids.as_ref() == Some(&self.ids)
                    && self.hash.finish() == self.expected =>
            {
                (0, base + u128::from(self.decode) * row_bytes + 8 + u128::from(self.decode) * 4)
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
                if pass == 0 && self.deferred && self.position == u32::MAX {
                    self.cursor = PalwFlatConstraintCursorV1::Nominate;
                } else if pass == 0 && actual.is_some_and(|id| id < self.vocab && self.committed_lane != Some(id)) {
                    self.committed_lane = actual;
                    self.restart_pass(1);
                } else {
                    self.cursor = PalwFlatConstraintCursorV1::Complete;
                }
            }
            (PalwFlatConstraintCursorV1::Nominate, PalwFlatConstraintActionV1::Nominate { position, beat }) => {
                if *position > self.decode || beat.is_some_and(|id| id >= self.vocab) || (*position == self.decode && beat.is_some()) {
                    return Err("deferred Flat nomination exceeds authenticated original geometry".into());
                }
                self.position = *position;
                self.beat_lane = *beat;
                self.committed_lane =
                    self.authenticated_ids.as_ref().and_then(|ids| ids.get(*position as usize)).copied().filter(|id| *id < self.vocab);
                // These predicates use authenticated IDs only, never a selected logit. Keep
                // the first exact original EOF; a second pass cannot add authentication.
                if self.committed_lane.is_none() && beat.is_none() {
                    self.cursor = PalwFlatConstraintCursorV1::Complete;
                } else {
                    self.restart_pass(1);
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
    pub fn authenticated_generated_ids_v1(&self) -> Result<&[u32], String> {
        self.check_v1()?;
        self.authenticated_ids.as_deref().ok_or("Flat IDs have not reached authenticated original EOF".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFlatConstraintSourceAnswerV1 {
    pub answerer: PalwBondKeyV2,
    pub source_id: Hash64,
    /// Binds both passes, so an old accepted packet cannot stand in for a new turn.
    pub cursor: PalwFlatConstraintCursorV1,
    pub action: PalwFlatConstraintActionV1,
}

/// Each literal custodian has an isolated hash continuation. A partial or invalid stream from
/// one conspirator never occupies an honest party's slot. The outside owner may independently
/// supply public bytes but is not liable for a custodian's original DA obligation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFlatConstraintSourceSessionV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub source_id: Hash64,
    pub respond_until_daa: u64,
    pub collect_until_daa: u64,
    pub nomination_until_daa: u64,
    pub producer: PalwBondKeyV2,
    pub custodians: BTreeSet<PalwBondKeyV2>,
    template: PalwFlatConstraintStreamV1,
    slots: BTreeMap<PalwBondKeyV2, Option<PalwFlatConstraintStreamV1>>,
    collected: bool,
    nomination: Option<(u32, Option<u32>)>,
    authenticated: Option<PalwBondKeyV2>,
    ready: Option<PalwBondKeyV2>,
}
impl PalwFlatConstraintSourceSessionV1 {
    fn identity_v1(&self) -> Result<Hash64, String> {
        let bytes = borsh::to_vec(&(
            self.claim,
            self.owner,
            self.reserved_daa,
            self.bundle_id,
            self.producer,
            &self.custodians,
            self.template.binding_root,
            self.template.job_context,
            self.template.expected,
            self.template.position,
            self.template.committed_lane,
            self.template.beat_lane,
            self.respond_until_daa,
            self.collect_until_daa,
            self.nomination_until_daa,
        ))
        .map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/flat-constraint-source/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        ))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        producer: PalwBondKeyV2,
        custodians: BTreeSet<PalwBondKeyV2>,
        binding: &PalwStepBindingV2,
        position: u32,
        guessed_committed: Option<u32>,
        beat: Option<u32>,
        respond_until_daa: u64,
        collect_until_daa: u64,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_binding_v1(binding)?;
        Self::new_view_v1(
            claim,
            owner,
            reserved_daa,
            bundle_id,
            producer,
            custodians,
            &view,
            position,
            guessed_committed,
            beat,
            respond_until_daa,
            collect_until_daa,
            prices,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_view_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        producer: PalwBondKeyV2,
        custodians: BTreeSet<PalwBondKeyV2>,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        position: u32,
        guessed_committed: Option<u32>,
        beat: Option<u32>,
        respond_until_daa: u64,
        collect_until_daa: u64,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        if !custodians.contains(&producer)
            || custodians.contains(&owner)
            || reserved_daa >= respond_until_daa
            || respond_until_daa >= collect_until_daa
        {
            return Err("Flat source requires literal producer custody, outside owner and original paid clocks".into());
        }
        let bytes = borsh::to_vec(&(
            claim,
            owner,
            reserved_daa,
            bundle_id,
            producer,
            &custodians,
            view.committed_execution_root,
            view.job_context.context_hash(),
            view.full_logits_trace_root,
            position,
            guessed_committed.filter(|id| *id < view.vocab_size),
            beat,
            respond_until_daa,
            collect_until_daa,
            respond_until_daa,
        ))
        .map_err(|e| e.to_string())?;
        let source_id = Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/flat-constraint-source/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        );
        let template = PalwFlatConstraintStreamV1::new_view_v1(source_id, view, position, guessed_committed, beat, prices)?;
        let mut slots = custodians.iter().map(|key| (*key, None)).collect::<BTreeMap<_, _>>();
        slots.insert(owner, None);
        Ok(Self {
            claim,
            owner,
            reserved_daa,
            bundle_id,
            source_id,
            respond_until_daa,
            collect_until_daa,
            nomination_until_daa: respond_until_daa,
            producer,
            custodians,
            template,
            slots,
            collected: false,
            nomination: None,
            authenticated: None,
            ready: None,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_deferred_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        producer: PalwBondKeyV2,
        custodians: BTreeSet<PalwBondKeyV2>,
        binding: &PalwStepBindingV2,
        respond_until_daa: u64,
        collect_until_daa: u64,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_binding_v1(binding)?;
        Self::new_deferred_view_v1(
            claim,
            owner,
            reserved_daa,
            bundle_id,
            producer,
            custodians,
            &view,
            respond_until_daa,
            collect_until_daa,
            prices,
        )
    }
    /// Same immutable slots and two-pass clock as the legacy constructor, using
    /// the natively derived original decode view rather than a synthetic binding.
    #[allow(clippy::too_many_arguments)]
    pub fn new_deferred_view_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        producer: PalwBondKeyV2,
        custodians: BTreeSet<PalwBondKeyV2>,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        respond_until_daa: u64,
        collect_until_daa: u64,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        let mut session = Self::new_view_v1(
            claim,
            owner,
            reserved_daa,
            bundle_id,
            producer,
            custodians,
            view,
            0,
            None,
            None,
            respond_until_daa,
            collect_until_daa,
            prices,
        )?;
        session.template.deferred = true;
        session.template.position = u32::MAX;
        let slots = session.custodians.len() as u128 + 1;
        let d = u128::from(view.job_context.exact_decode_tokens);
        let per_pass = d * (1 + u128::from(view.vocab_size).div_ceil(PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u128))
            + d.div_ceil(PALW_FLAT_CONSTRAINT_PART_WORDS_V1 as u128)
            + 1;
        let index = (u128::from(view.step_leaf_count) + d + 4)
            .div_ceil(64)
            .checked_mul(u128::from(prices.reorg_replay_moves) + 1)
            .ok_or("Flat second-pass index budget overflow")?;
        let reserve = (per_pass + index + u128::from(prices.reorg_replay_moves))
            .checked_mul(slots)
            .and_then(|n| n.checked_add(2))
            .and_then(|n| n.checked_mul(u128::from(prices.inclusion_margin_daa)))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or("Flat second-pass deadline budget overflow")?;
        session.nomination_until_daa = respond_until_daa
            .checked_sub(reserve)
            .filter(|until| *until > reserved_daa)
            .ok_or("original Flat paid window cannot cover both exact source passes and restart")?;
        session.source_id = session.identity_v1()?;
        session.template.context = session.source_id;
        Ok(session)
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.template.check_v1()?;
        if self.template.context != self.source_id
            || self.template.moves != 0
            || self.identity_v1()? != self.source_id
            || !self.custodians.contains(&self.producer)
            || self.custodians.contains(&self.owner)
            || self.reserved_daa >= self.respond_until_daa
            || self.respond_until_daa >= self.collect_until_daa
            || self.nomination_until_daa <= self.reserved_daa
            || if self.template.deferred {
                self.nomination_until_daa >= self.respond_until_daa
            } else {
                self.nomination_until_daa != self.respond_until_daa
            }
            || self.slots.len() != self.custodians.len() + 1
            || !self.slots.contains_key(&self.owner)
            || self.custodians.iter().any(|key| !self.slots.contains_key(key))
        {
            return Err("Flat source loses immutable literal slots or paid clocks".into());
        }
        for row in self.slots.values().flatten() {
            row.check_v1()?;
            if row.context != self.source_id
                || row.binding_root != self.template.binding_root
                || row.job_context != self.template.job_context
                || row.expected != self.template.expected
                || row.prefill != self.template.prefill
                || row.decode != self.template.decode
                || row.vocab != self.template.vocab
                || row.deferred != self.template.deferred
                || if self.template.deferred {
                    (row.position == u32::MAX && row.beat_lane.is_some())
                        || (row.position != u32::MAX && self.nomination != Some((row.position, row.beat_lane)))
                } else {
                    row.position != self.template.position || row.beat_lane != self.template.beat_lane
                }
                || row.maximum_moves != self.template.maximum_moves
            {
                return Err("Flat source slot changes original authenticated context".into());
            }
        }
        if self.authenticated.is_some_and(|key| {
            self.slots.get(&key).and_then(Option::as_ref).is_none_or(|row| row.authenticated_generated_ids_v1().is_err())
        }) || self.ready.is_some_and(|key| {
            self.slots.get(&key).and_then(Option::as_ref).is_none_or(|row| row.cursor_v1() != PalwFlatConstraintCursorV1::Complete)
        }) || self.nomination.is_some_and(|(position, beat)| {
            !self.template.deferred
                || self.authenticated.is_none()
                || position > self.template.decode
                || beat.is_some_and(|id| id >= self.template.vocab)
                || (position == self.template.decode && beat.is_some())
        }) {
            return Err("Flat accepted nomination/EOF locator loses original authenticated context".into());
        }
        Ok(())
    }
    /// O(log custody), no full-session copy. Read from authenticated native state only.
    pub fn slot_cursor_v1(&self, signer: &PalwBondKeyV2) -> Result<PalwFlatConstraintCursorV1, String> {
        let cursor = self
            .slots
            .get(signer)
            .ok_or("Flat answerer has no protected literal slot")?
            .as_ref()
            .unwrap_or(&self.template)
            .cursor_v1();
        Ok(
            if cursor == PalwFlatConstraintCursorV1::Nominate
                && let Some((position, beat)) = self.nomination
            {
                let row = self.slots.get(signer).and_then(Option::as_ref).expect("authenticated EOF cursor");
                if beat.is_none()
                    && row.authenticated_ids.as_ref().and_then(|ids| ids.get(position as usize)).is_none_or(|id| *id >= row.vocab)
                {
                    PalwFlatConstraintCursorV1::Complete
                } else {
                    PalwFlatConstraintCursorV1::RowBegin { pass: 1, row: 0 }
                }
            } else {
                cursor
            },
        )
    }
    pub fn answer_v1(&mut self, now: u64, answer: &PalwFlatConstraintSourceAnswerV1) -> Result<(), String> {
        if now < self.reserved_daa
            || self.response_deadline_v1(&answer.answerer).is_none_or(|until| now >= until)
            || self.collected
            || answer.source_id != self.source_id
            || answer.cursor == PalwFlatConstraintCursorV1::Complete
            || matches!(answer.action, PalwFlatConstraintActionV1::Nominate { .. })
            || answer.cursor != self.slot_cursor_v1(&answer.answerer)?
        {
            return Err("Flat source answer substitutes context, turn or exclusive paid deadline".into());
        }
        answer.action.check_wire_v1()?;
        // Invalid EOF/order must not alter the persisted slot or another custodian's progress.
        let mut row = self.slots.get(&answer.answerer).expect("checked").as_ref().unwrap_or(&self.template).clone();
        if row.cursor_v1() == PalwFlatConstraintCursorV1::Nominate {
            let (position, beat) = self.nomination.ok_or("Flat source still awaits accepted owner nomination")?;
            row.movement_v1(self.source_id, &PalwFlatConstraintActionV1::Nominate { position, beat })?;
        }
        row.movement_v1(self.source_id, &answer.action)?;
        if row.cursor_v1() == PalwFlatConstraintCursorV1::Nominate
            && let Some((position, beat)) = self.nomination
            && beat.is_none()
            && row.authenticated_ids.as_ref().and_then(|ids| ids.get(position as usize)).is_none_or(|id| *id >= row.vocab)
        {
            row.movement_v1(self.source_id, &PalwFlatConstraintActionV1::Nominate { position, beat })?;
        }
        if self.authenticated.is_none() && row.authenticated_generated_ids_v1().is_ok() {
            self.authenticated = Some(answer.answerer);
        }
        if self.ready.is_none() && row.cursor_v1() == PalwFlatConstraintCursorV1::Complete {
            self.ready = Some(answer.answerer);
        }
        self.slots.insert(answer.answerer, Some(row));
        Ok(())
    }
    pub fn authenticated_pin_v1(&self, signer: &PalwBondKeyV2) -> Result<PalwFlatConstraintPinV1, String> {
        self.slots.get(signer).and_then(Option::as_ref).ok_or("Flat source has no accepted public bytes")?.authenticated_pin_v1()
    }
    pub fn ready_signer_v1(&self) -> Option<PalwBondKeyV2> {
        self.ready
    }
    pub fn authenticated_generated_ids_v1(&self) -> Result<&[u32], String> {
        let key = self.authenticated.ok_or("Flat source has no authenticated first EOF")?;
        self.slots.get(&key).and_then(Option::as_ref).ok_or("Flat source first EOF locator missing")?.authenticated_generated_ids_v1()
    }
    pub fn nomination_v1(&self) -> Option<(u32, Option<u32>)> {
        self.nomination
    }
    pub fn needs_nomination_v1(&self) -> bool {
        self.template.deferred && self.nomination.is_none() && self.authenticated.is_some()
    }
    pub fn response_deadline_v1(&self, signer: &PalwBondKeyV2) -> Option<u64> {
        let row = self.slots.get(signer)?;
        Some(if self.template.deferred && row.as_ref().is_none_or(|row| row.authenticated_ids.is_none()) {
            self.nomination_until_daa
        } else {
            self.respond_until_daa
        })
    }
    pub fn has_response_duty_v1(&self, signer: &PalwBondKeyV2) -> bool {
        !self.collected
            && self.custodians.contains(signer)
            && self
                .slot_cursor_v1(signer)
                .is_ok_and(|cursor| !matches!(cursor, PalwFlatConstraintCursorV1::Complete | PalwFlatConstraintCursorV1::Nominate))
    }
    pub fn collection_due_daa_v1(&self) -> u64 {
        if self.template.deferred && self.nomination.is_none() { self.nomination_until_daa } else { self.respond_until_daa }
    }
    /// Opening does not move either paid clock. Every mandatory first pass must still
    /// have the already-priced conservative pass/index/inclusion span to reach EOF.
    pub fn check_open_window_v1(&self, now: u64) -> Result<(), String> {
        let span = self.respond_until_daa.checked_sub(self.nomination_until_daa).ok_or("Flat first-pass clock underflow")?;
        if now < self.reserved_daa
            || now >= self.nomination_until_daa
            || now.checked_add(span).is_none_or(|end| end > self.nomination_until_daa)
        {
            return Err("Flat source opened too late for its original paid first EOF window".into());
        }
        Ok(())
    }
    pub fn nominate_v1(
        &mut self,
        now: u64,
        owner: PalwBondKeyV2,
        source_id: Hash64,
        position: u32,
        beat: Option<u32>,
    ) -> Result<(), String> {
        if owner != self.owner
            || source_id != self.source_id
            || !self.template.deferred
            || self.nomination.is_some()
            || now < self.reserved_daa
            || now >= self.nomination_until_daa
            || self.collected
            || self.authenticated_generated_ids_v1().is_err()
            || position > self.template.decode
            || beat.is_some_and(|id| id >= self.template.vocab)
            || (position == self.template.decode && beat.is_some())
        {
            return Err("Flat owner nomination changes authenticated IDs, paid clock or repeats selection".into());
        }
        let signer = self.authenticated.expect("checked");
        let row = self.slots.get(&signer).and_then(Option::as_ref).expect("authenticated first EOF locator");
        if beat.is_none()
            && row.authenticated_ids.as_ref().and_then(|ids| ids.get(position as usize)).is_none_or(|id| *id >= row.vocab)
        {
            let mut row = row.clone();
            row.movement_v1(self.source_id, &PalwFlatConstraintActionV1::Nominate { position, beat })?;
            self.slots.insert(signer, Some(row));
            self.ready = Some(signer);
        }
        self.nomination = Some((position, beat));
        Ok(())
    }
    pub fn collected_v1(&self) -> bool {
        self.collected
    }
    /// Literal identities to sanction in the native fold, not a local assertion of conviction.
    /// Every mandatory party owes EOF, even if an independent owner has already supplied bytes.
    pub fn collect_v1(&mut self, now: u64) -> Result<Vec<PalwBondKeyV2>, String> {
        if self.collected || now < self.collection_due_daa_v1() || now >= self.collect_until_daa {
            return Err("Flat collection is outside original exclusive clocks or repeats a sanction".into());
        }
        let missing = self
            .custodians
            .iter()
            .filter(|key| {
                self.slots.get(key).and_then(Option::as_ref).is_none_or(|row| {
                    if self.template.deferred && self.nomination.is_none() {
                        row.authenticated_ids.is_none()
                    } else {
                        self.slot_cursor_v1(key) != Ok(PalwFlatConstraintCursorV1::Complete)
                    }
                })
            })
            .copied()
            .collect();
        self.collected = true;
        Ok(missing)
    }
}

/// Full native custody multiplicity plus conservative dense structural indexing/restart work.
/// The caller must supply the chain's maximum qualified custody, not merely current signers.
pub fn flat_constraint_source_resource_plan_v1(
    decode: u32,
    vocab: u32,
    profile_bytes: u64,
    maximum_custodians: u32,
    step_leaf_count: u64,
    prices: PalwRenderingResourcePricesV1,
) -> Result<PalwRenderingResourcePlanV1, String> {
    if maximum_custodians == 0 || step_leaf_count == 0 {
        return Err("Flat source quote needs original custody/step envelope".into());
    }
    let one = flat_constraint_resource_plan_v1(decode, vocab, prices)?;
    let slots = u128::from(maximum_custodians) + 1;
    let n = |x: u128| u64::try_from(x).map_err(|_| "Flat custody source quote overflow".to_string());
    // Every restart replay budget gets a full bounded structural rebuild, never free work.
    let index = (u128::from(step_leaf_count) + u128::from(decode) + 4)
        .checked_mul(u128::from(prices.reorg_replay_moves) + 1)
        .and_then(|n| n.checked_mul(slots))
        .ok_or("Flat index work overflow")?;
    let index_polls = (u128::from(step_leaf_count) + u128::from(decode) + 4)
        .div_ceil(64)
        .checked_mul(u128::from(prices.reorg_replay_moves) + 1)
        .and_then(|n| n.checked_mul(slots))
        .ok_or("Flat index poll budget overflow")?;
    let moves = u128::from(one.maximum_moves) * slots + 2;
    let row = u128::from(one.row_bytes) * (slots + 1) + slots * 256 + u128::from(profile_bytes) + 2048;
    let retained = u128::from(one.maximum_stream_bytes) * slots + moves * 5632 + row * moves;
    let work = index
        .checked_mul(64)
        .and_then(|n| n.checked_add(moves * slots * (u128::from(decode) + 64)))
        .ok_or("Flat source validation work overflow")?;
    Ok(PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab,
        maximum_stream_bytes: n(retained)?,
        maximum_stream_leaves: maximum_custodians as u64 + 1,
        maximum_eog_bytes: 0,
        maximum_moves: n(moves)?,
        maximum_parser_steps: n(work)?,
        maximum_blake_blocks: n(u128::from(one.maximum_blake_blocks) * slots + index + 4 * retained.div_ceil(128))?,
        inclusion_daa: n((moves + index_polls) * u128::from(prices.inclusion_margin_daa))?,
        rent_sompi: n(retained * u128::from(prices.rent_per_stream_byte)
            + row * u128::from(prices.rent_per_row_byte)
            + work * u128::from(prices.rent_per_parser_step))?,
        deposit_sompi: n(moves * u128::from(prices.deposit_per_move))?,
        row_bytes: n(row)?,
        resident_ram_bytes: n(row * 8 + slots * (u128::from(decode) * 8 + 32768) + 65536)?,
    })
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
        + u128::from(prices.reorg_replay_moves)
        + 1;
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
        source_action(row.cursor_v1(), source)
    }
    fn source_action(cursor: PalwFlatConstraintCursorV1, source: &PalwBase0DecodeTokensV1) -> PalwFlatConstraintActionV1 {
        match cursor {
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

    #[test]
    fn native_decode_view_constructors_preserve_legacy_flat_source_bytes() {
        let (binding, _, position) = fixture();
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_binding_v1(&binding).unwrap();
        let context = Hash64::from_u64_word(0xF1A7);
        assert_eq!(
            borsh::to_vec(&PalwFlatConstraintStreamV1::new_deferred_v1(context, &binding, prices()).unwrap()).unwrap(),
            borsh::to_vec(&PalwFlatConstraintStreamV1::new_deferred_view_v1(context, &view, prices()).unwrap()).unwrap()
        );
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        for deferred in [false, true] {
            let legacy = if deferred {
                PalwFlatConstraintSourceSessionV1::new_deferred_v1(
                    context,
                    key(1),
                    10,
                    context,
                    key(2),
                    BTreeSet::from([key(2), key(3)]),
                    &binding,
                    1_000_000_000,
                    1_000_000_100,
                    prices(),
                )
            } else {
                PalwFlatConstraintSourceSessionV1::new_v1(
                    context,
                    key(1),
                    10,
                    context,
                    key(2),
                    BTreeSet::from([key(2), key(3)]),
                    &binding,
                    position,
                    None,
                    None,
                    1_000_000_000,
                    1_000_000_100,
                    prices(),
                )
            }
            .unwrap();
            let native = if deferred {
                PalwFlatConstraintSourceSessionV1::new_deferred_view_v1(
                    context,
                    key(1),
                    10,
                    context,
                    key(2),
                    BTreeSet::from([key(2), key(3)]),
                    &view,
                    1_000_000_000,
                    1_000_000_100,
                    prices(),
                )
            } else {
                PalwFlatConstraintSourceSessionV1::new_view_v1(
                    context,
                    key(1),
                    10,
                    context,
                    key(2),
                    BTreeSet::from([key(2), key(3)]),
                    &view,
                    position,
                    None,
                    None,
                    1_000_000_000,
                    1_000_000_100,
                    prices(),
                )
            }
            .unwrap();
            assert_eq!(borsh::to_vec(&legacy).unwrap(), borsh::to_vec(&native).unwrap());
            native.check_v1().unwrap();
        }
    }

    #[test]
    fn tir_flat_view_authenticates_original_eof_before_exposing_invalid_output_to_the_court() {
        use crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1;
        use crate::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptCommitmentV3, fp_claim_id_v3};
        // Native header/decode geometry fixture, not class-registration or node E2E evidence.
        let mut case = crate::palw_decode_constraint_v1::tests::public_tir_constrained_case_for_tests_v1();
        case.record.facts.tiled = false;
        case.ids[case.position as usize] = case.record.logits_vocab;
        let source = PalwBase0DecodeTokensV1 { logits_rows: case.rows, generated_token_ids: case.ids };
        case.header.full_logits_trace_root = crate::palw_step_refute::base0_logits_trace_root_v1(
            &case.header.job_context,
            &source.logits_rows,
            &source.generated_token_ids,
        );
        case.header.committed_execution_root = crate::palw_tir_step_v1::palw_tir_execution_root_v1(
            &case.header.job_context.context_hash(),
            &case.header.full_logits_trace_root,
            &case.record.facts.class_id,
            case.header.step_leaf_count,
            &case.header.step_merkle_root,
        );
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(&case.header, &case.record, 1 << 20).unwrap();
        let context = Hash64::from_u64_word(0xF1A8);
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let (outside, producer, panel) = (key(1), key(2), key(3));
        let withheld = PalwFlatConstraintSourceSessionV1::new_view_v1(
            context,
            outside,
            10,
            context,
            producer,
            BTreeSet::from([producer, panel]),
            &view,
            case.position,
            None,
            None,
            1_000_000_000,
            1_000_000_100,
            prices(),
        )
        .unwrap();
        let mut withheld: PalwFlatConstraintSourceSessionV1 = borsh::from_slice(&borsh::to_vec(&withheld).unwrap()).unwrap();
        withheld.check_v1().unwrap();
        assert!(withheld.collect_v1(999_999_999).is_err());
        assert_eq!(
            withheld.collect_v1(1_000_000_000).unwrap().into_iter().collect::<BTreeSet<_>>(),
            BTreeSet::from([producer, panel])
        );
        assert!(withheld.collect_v1(1_000_000_000).is_err());
        assert_eq!(withheld.respond_until_daa, 1_000_000_000);
        let mut row = PalwFlatConstraintStreamV1::new_deferred_view_v1(context, &view, prices()).unwrap();
        assert!(row.authenticated_generated_ids_v1().is_err());
        row.movement_v1(context, &next(&row, &source)).unwrap();
        let saved = borsh::to_vec(&row).unwrap();
        let mut row: PalwFlatConstraintStreamV1 = borsh::from_slice(&saved).unwrap();
        row.check_v1().unwrap();
        while row.cursor_v1() != PalwFlatConstraintCursorV1::Nominate {
            assert!(row.authenticated_generated_ids_v1().is_err());
            row.movement_v1(context, &next(&row, &source)).unwrap();
        }
        assert_eq!(row.authenticated_generated_ids_v1().unwrap(), source.generated_token_ids);
        row.movement_v1(context, &PalwFlatConstraintActionV1::Nominate { position: view.job_context.exact_decode_tokens, beat: None })
            .unwrap();
        let pin = row.authenticated_pin_v1().unwrap();
        assert!(pin.matches_view_v1(&view));
        let mut foreign = view.clone();
        foreign.full_logits_trace_root = Hash64::default();
        assert!(!pin.matches_view_v1(&foreign));
        let mut tiled = view.clone();
        tiled.logits_scheme_id = crate::palw_step_refute::tiled_logits_scheme_id_v1();
        assert!(PalwFlatConstraintStreamV1::new_deferred_view_v1(context, &tiled, prices()).is_err());
        let mut floating = view.clone();
        floating.lane = crate::palw_step::PalwStepLaneV1::Float32;
        assert!(PalwFlatConstraintStreamV1::new_deferred_view_v1(context, &floating, prices()).is_err());
        let original = PalwFreePromptCommitmentV3 {
            job: case.job.clone(),
            trace_root: view.full_logits_trace_root,
            output_root: Hash64::default(),
            schedule_root: Hash64::default(),
            execution_root: view.committed_execution_root,
            decode_tokens_executed: view.job_context.exact_decode_tokens,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: view.step_leaf_count,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: 1,
            trace_retention_daa: 100_000,
        };
        let owner = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(context, 0));
        let make = |original: &PalwFreePromptCommitmentV3| {
            PalwConstraintStreamCourtV1::new_from_flat_view_with_original_stop_context_v1(
                fp_claim_id_v3(&original),
                owner,
                10,
                view.committed_execution_root,
                &view,
                &borsh::to_vec(&case.header).unwrap(),
                &case.job,
                &pin,
                None,
                case.eog.ids.len() as u32,
                &case.eog.opening,
                original,
                None,
            )
        };
        let court = make(&original).unwrap();
        assert_eq!(court.fault().unwrap().unwrap().position, case.position);
        let mut forged_stop = original.clone();
        forged_stop.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
        assert!(make(&forged_stop).is_err());
        let mut corrupted = source.clone();
        corrupted.logits_rows[0][0] ^= 1;
        let mut forged = PalwFlatConstraintStreamV1::new_deferred_view_v1(context, &view, prices()).unwrap();
        while !matches!(forged.cursor_v1(), PalwFlatConstraintCursorV1::Finish { .. }) {
            forged.movement_v1(context, &next(&forged, &corrupted)).unwrap();
        }
        assert!(forged.movement_v1(context, &PalwFlatConstraintActionV1::Finish).is_err());
        assert!(forged.authenticated_generated_ids_v1().is_err());
    }

    #[test]
    fn first_original_eof_nominates_invalid_id_without_another_logit_pass_or_false_da() {
        let (mut binding, mut source, position) = fixture();
        source.generated_token_ids[position as usize] = binding.shape_profile.vocab_size;
        binding.full_logits_trace_root = crate::palw_step_refute::base0_logits_trace_root_v1(
            &binding.job_context,
            &source.logits_rows,
            &source.generated_token_ids,
        );
        crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let (owner, producer, panel) = (key(111), key(112), key(113));
        let mut session = PalwFlatConstraintSourceSessionV1::new_deferred_v1(
            Hash64::from_u64_word(114),
            owner,
            10,
            Hash64::from_u64_word(115),
            producer,
            [producer, panel].into_iter().collect(),
            &binding,
            100_000,
            100_100,
            prices(),
        )
        .unwrap();
        let last_open = session.nomination_until_daa - (session.respond_until_daa - session.nomination_until_daa);
        session.check_open_window_v1(last_open).unwrap();
        assert!(session.check_open_window_v1(last_open + 1).is_err());
        for signer in [producer, panel] {
            while session.slot_cursor_v1(&signer).unwrap() != PalwFlatConstraintCursorV1::Nominate {
                let cursor = session.slot_cursor_v1(&signer).unwrap();
                session
                    .answer_v1(
                        20,
                        &PalwFlatConstraintSourceAnswerV1 {
                            answerer: signer,
                            source_id: session.source_id,
                            cursor,
                            action: source_action(cursor, &source),
                        },
                    )
                    .unwrap();
            }
        }
        session.nominate_v1(20, owner, session.source_id, position, None).unwrap();
        let session: PalwFlatConstraintSourceSessionV1 = borsh::from_slice(&borsh::to_vec(&session).unwrap()).unwrap();
        let pin = session.authenticated_pin_v1(&session.ready_signer_v1().unwrap()).unwrap();
        assert_eq!(pin.committed_id_v1(), Some(binding.shape_profile.vocab_size));
        assert!(pin.committed_value_v1().is_none());
        assert!(!session.has_response_duty_v1(&panel));
        let mut session = session;
        assert!(session.collect_v1(session.respond_until_daa).unwrap().is_empty());
    }

    #[test]
    fn invalid_original_id_and_terminal_stop_bridge_to_real_bounded_decode_faults() {
        use crate::palw_constraint_stream_court_v1::{PalwConstraintStreamCourtV1, PalwConstraintStreamPhaseV1};
        use crate::palw_freeprompt_v3::{PalwFpStopReasonV3, PalwFreePromptCommitmentV3, fp_claim_id_v3};
        let (mut binding, mut proof) = crate::palw_decode_constraint_v1::tests::public_flat_constrained_case_for_tests_v1(false);
        let owner = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(116), 0));
        let context = Hash64::from_u64_word(117);
        let original_ids = proof.pin.generated_token_ids.clone();
        proof.pin.generated_token_ids[proof.position as usize] = binding.shape_profile.vocab_size;
        binding.full_logits_trace_root = crate::palw_step_refute::base0_logits_trace_root_v1(
            &binding.job_context,
            &proof.pin.logits_rows,
            &proof.pin.generated_token_ids,
        );
        crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
        let mut stream = PalwFlatConstraintStreamV1::new_deferred_v1(context, &binding, prices()).unwrap();
        while stream.cursor_v1() != PalwFlatConstraintCursorV1::Nominate {
            stream.movement_v1(context, &next(&stream, &proof.pin)).unwrap();
        }
        stream.movement_v1(context, &PalwFlatConstraintActionV1::Nominate { position: proof.position, beat: None }).unwrap();
        let pin = stream.authenticated_pin_v1().unwrap();
        let row = PalwConstraintStreamCourtV1::new_from_flat_pin_v1(
            Hash64::from_u64_word(118),
            owner,
            10,
            binding.committed_execution_root,
            &binding,
            &proof.job,
            &pin,
            None,
            proof.eog.ids.len() as u32,
            &proof.eog.opening,
        )
        .unwrap();
        assert_eq!(row.phase(), PalwConstraintStreamPhaseV1::Complete);
        assert_eq!(row.fault().unwrap().unwrap().position, proof.position);

        for ends_in_eog in [false, true] {
            proof.pin.generated_token_ids = original_ids.clone();
            if ends_in_eog {
                *proof.pin.generated_token_ids.last_mut().unwrap() = proof.eog.ids[0];
            }
            binding.full_logits_trace_root = crate::palw_step_refute::base0_logits_trace_root_v1(
                &binding.job_context,
                &proof.pin.logits_rows,
                &proof.pin.generated_token_ids,
            );
            crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
            let mut stream = PalwFlatConstraintStreamV1::new_deferred_v1(context, &binding, prices()).unwrap();
            while stream.cursor_v1() != PalwFlatConstraintCursorV1::Nominate {
                stream.movement_v1(context, &next(&stream, &proof.pin)).unwrap();
            }
            stream
                .movement_v1(
                    context,
                    &PalwFlatConstraintActionV1::Nominate { position: binding.job_context.exact_decode_tokens, beat: None },
                )
                .unwrap();
            let pin = stream.authenticated_pin_v1().unwrap();
            assert!(
                PalwConstraintStreamCourtV1::new_from_flat_pin_v1(
                    Hash64::from_u64_word(118),
                    owner,
                    10,
                    binding.committed_execution_root,
                    &binding,
                    &proof.job,
                    &pin,
                    None,
                    proof.eog.ids.len() as u32,
                    &proof.eog.opening,
                )
                .is_err(),
                "terminal stop facts are never supplied by a caller-only pin"
            );
            let original = PalwFreePromptCommitmentV3 {
                job: proof.job.clone(),
                trace_root: binding.full_logits_trace_root,
                output_root: Hash64::default(),
                schedule_root: Hash64::default(),
                execution_root: binding.committed_execution_root,
                decode_tokens_executed: binding.job_context.exact_decode_tokens,
                stop_reason: PalwFpStopReasonV3::EndOfGeneration,
                work_leaves: binding.step_leaf_count,
                trace_manifest_root: Hash64::default(),
                trace_chunk_count: 1,
                trace_retention_daa: 100_000,
            };
            let make = |original: &PalwFreePromptCommitmentV3| {
                PalwConstraintStreamCourtV1::new_from_flat_pin_with_original_stop_context_v1(
                    fp_claim_id_v3(original),
                    owner,
                    10,
                    binding.committed_execution_root,
                    &binding,
                    &proof.job,
                    &pin,
                    None,
                    proof.eog.ids.len() as u32,
                    &proof.eog.opening,
                    original,
                    None,
                )
            };
            let mut row = make(&original).unwrap();
            assert!(row.fault().is_err());
            row.push_eog(row.seed_id(), 0, &proof.eog.ids).unwrap();
            row.finish_eog().unwrap();
            let cp = row.checkpoint_v1();
            let mut restored = make(&original).unwrap();
            restored.restore_checkpoint_v1(&cp).unwrap();
            assert_eq!(restored.fault().unwrap().is_some(), !ends_in_eog);
            let mut wrong = original;
            wrong.stop_reason = PalwFpStopReasonV3::ExactBudgetReached;
            assert!(make(&wrong).is_err());
        }
    }

    #[test]
    fn deferred_selection_uses_authenticated_ids_for_later_row_with_only_two_passes() {
        let (binding, source, position) = fixture();
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let (owner, producer, panel) = (key(11), key(12), key(13));
        let mut session = PalwFlatConstraintSourceSessionV1::new_deferred_v1(
            Hash64::from_u64_word(14),
            owner,
            10,
            Hash64::from_u64_word(15),
            producer,
            [producer, panel].into_iter().collect(),
            &binding,
            100_000,
            100_100,
            prices(),
        )
        .unwrap();
        assert!(session.nominate_v1(20, owner, session.source_id, position, None).is_err());
        while session.slot_cursor_v1(&producer).unwrap() != PalwFlatConstraintCursorV1::Nominate {
            let cursor = session.slot_cursor_v1(&producer).unwrap();
            session
                .answer_v1(
                    20,
                    &PalwFlatConstraintSourceAnswerV1 {
                        answerer: producer,
                        source_id: session.source_id,
                        cursor,
                        action: source_action(cursor, &source),
                    },
                )
                .unwrap();
        }
        assert_eq!(session.authenticated_generated_ids_v1().unwrap(), source.generated_token_ids);
        assert!(session.authenticated_pin_v1(&producer).is_err());
        let mut abandoned = session.clone();
        let first_until = abandoned.nomination_until_daa;
        assert!(abandoned.nominate_v1(first_until, owner, abandoned.source_id, position, None).is_err());
        assert!(!abandoned.has_response_duty_v1(&producer));
        assert!(abandoned.has_response_duty_v1(&panel));
        assert_eq!(abandoned.response_deadline_v1(&panel), Some(first_until));
        // A party that served the complete original first preimage is not punished for the
        // owner's abandonment/late nomination; the truly silent Panel slot remains liable.
        assert_eq!(abandoned.collect_v1(first_until).unwrap(), vec![panel]);
        let original_id = session.source_id;
        let beat = (source.generated_token_ids[position as usize] + 1) % binding.shape_profile.vocab_size;
        assert!(session.nominate_v1(20, panel, session.source_id, position, Some(beat)).is_err());
        session.nominate_v1(20, owner, session.source_id, position, Some(beat)).unwrap();
        assert!(session.nominate_v1(20, owner, session.source_id, 0, None).is_err());
        let mut session: PalwFlatConstraintSourceSessionV1 = borsh::from_slice(&borsh::to_vec(&session).unwrap()).unwrap();
        while session.slot_cursor_v1(&producer).unwrap() != PalwFlatConstraintCursorV1::Complete {
            let cursor = session.slot_cursor_v1(&producer).unwrap();
            assert!(matches!(
                cursor,
                PalwFlatConstraintCursorV1::RowBegin { pass: 1, .. }
                    | PalwFlatConstraintCursorV1::RowPart { pass: 1, .. }
                    | PalwFlatConstraintCursorV1::Ids { pass: 1, .. }
                    | PalwFlatConstraintCursorV1::Finish { pass: 1 }
            ));
            session
                .answer_v1(
                    20,
                    &PalwFlatConstraintSourceAnswerV1 {
                        answerer: producer,
                        source_id: session.source_id,
                        cursor,
                        action: source_action(cursor, &source),
                    },
                )
                .unwrap();
        }
        let pin = session.authenticated_pin_v1(&producer).unwrap();
        assert_eq!(pin.position_v1(), position);
        assert_eq!(
            pin.committed_value_v1(),
            Some(source.logits_rows[position as usize][source.generated_token_ids[position as usize] as usize])
        );
        assert_eq!(session.source_id, original_id);
        assert_eq!(session.collect_v1(100_000).unwrap(), vec![panel]);
        session.check_v1().unwrap();
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
    fn literal_flat_slots_cannot_poison_public_owner_or_reset_deadline() {
        let (binding, source, position) = fixture();
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let (owner, producer, panel) = (key(1), key(2), key(3));
        let mut session = PalwFlatConstraintSourceSessionV1::new_v1(
            Hash64::from_u64_word(4),
            owner,
            10,
            Hash64::from_u64_word(5),
            producer,
            [producer, panel].into_iter().collect(),
            &binding,
            position,
            None,
            None,
            100,
            110,
            prices(),
        )
        .unwrap();
        let owner_genesis = session.slot_cursor_v1(&owner).unwrap();
        let old = session.clone();
        let wrong = PalwFlatConstraintSourceAnswerV1 {
            answerer: producer,
            source_id: session.source_id,
            cursor: owner_genesis,
            action: PalwFlatConstraintActionV1::Finish,
        };
        assert!(session.answer_v1(99, &wrong).is_err());
        assert_eq!(session, old);
        let first = next(&session.template, &source);
        session
            .answer_v1(
                99,
                &PalwFlatConstraintSourceAnswerV1 {
                    answerer: producer,
                    source_id: session.source_id,
                    cursor: owner_genesis,
                    action: first,
                },
            )
            .unwrap();
        assert_eq!(session.slot_cursor_v1(&owner).unwrap(), owner_genesis);
        while session.slot_cursor_v1(&owner).unwrap() != PalwFlatConstraintCursorV1::Complete {
            let row = session.slots.get(&owner).unwrap().as_ref().unwrap_or(&session.template);
            let answer = PalwFlatConstraintSourceAnswerV1 {
                answerer: owner,
                source_id: session.source_id,
                cursor: row.cursor_v1(),
                action: next(row, &source),
            };
            session.answer_v1(99, &answer).unwrap();
        }
        assert_eq!(session.authenticated_pin_v1(&owner).unwrap().generated_ids_v1(), source.generated_token_ids);
        assert!(session.authenticated_pin_v1(&producer).is_err());
        assert!(session.answer_v1(100, &wrong).is_err());
        assert!(session.collect_v1(99).is_err());
        assert_eq!(session.collect_v1(100).unwrap(), vec![producer, panel]);
        assert!(session.collect_v1(101).is_err());
        session.check_v1().unwrap();
        let mut tampered = session.clone();
        tampered.respond_until_daa += 1;
        assert!(tampered.check_v1().is_err());
        let mut expired = old;
        assert!(expired.collect_v1(110).is_err());
    }
    #[test]
    fn flat_custody_quote_includes_full_slots_and_dense_restart_work() {
        let small = flat_constraint_source_resource_plan_v1(1, 128, 4096, 5, 1000, prices()).unwrap();
        let many = flat_constraint_source_resource_plan_v1(1, 128, 4096, 65_536, 1000, prices()).unwrap();
        assert!(many.row_bytes > small.row_bytes);
        assert!(many.maximum_moves > small.maximum_moves);
        let large_step = flat_constraint_source_resource_plan_v1(1, 128, 4096, 5, 1_000_000, prices()).unwrap();
        assert!(large_step.inclusion_daa > small.inclusion_daa);
        assert!(large_step.maximum_parser_steps > small.maximum_parser_steps);
        let mut wire = vec![1];
        wire.extend(0u32.to_le_bytes());
        wire.extend(0u32.to_le_bytes());
        wire.extend(u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<PalwFlatConstraintActionV1>(&wire).is_err());
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

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_flat_constraint_stream_v1.rs"]
mod native_resource_inventory_v1;
