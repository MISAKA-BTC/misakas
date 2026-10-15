//! **Greedy Float32 Constraint source** — the restartable on-chain row court that feeds the
//! constrained-selection court from a `Float32` class's ORIGINAL v2 event tree (G14 uncovered
//! lane, dormant behind `palw_staged_float_constraint_v1`).
//!
//! A `Float32` class commits each decode call's whole f32 row as one keyed event hash under the
//! binding's `full_logits_trace_root` ([`crate::palw_v2::full_logits_trace_root_v2`]). An outside
//! owner acquires, from the claim's literal custody (producer + held Panel), only public bytes:
//!
//! 1. **Ids** — the trace summary, ordered event commitment and generated ids: one carrier,
//!    authenticated against the binding's trace root before anything reads an id;
//! 2. **Nominate** — the owner names a position (and an optional beating lane) over the
//!    authenticated ids, never over a guess;
//! 3. **Row** — the nominated event's f32 row in ordered parts of at most 64 KiB, folded into a
//!    stored [`PalwFloatRowStreamV1`] continuation (no vocabulary-sized resident buffer), then one
//!    finish carrying the event opening, which re-derives the trace root.
//!
//! Each literal custodian (and the owner, who may supply public bytes but carries no DA duty)
//! has an isolated slot, so a conspirator's partial or invalid stream never occupies an honest
//! party's slot. A custodian that misses its exclusive clock is returned to the native fold as an
//! unpaid identity (the same sanction as every staged source), never as a local guilt verdict.
//!
//! The authenticated selection facts — ids, position, the committed and beating lanes' f32 order
//! keys — seed [`crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1`] through
//! [`PalwFloatConstraintPinV1`]; the Constraint court core is scheme-agnostic after that point.
//!
//! **Greedy only.** A seeded/temperature Float32 job has no canonical float sampler; the quote,
//! this session and the seed all refuse it at every height (documented, not an uncovered lane
//! that a later fence silently arms).
use crate::{
    Hash64,
    palw_float_decode_v1::{PALW_FLOAT_ROW_PART_BYTES_V1, PalwFloatRowFactsV1, PalwFloatRowStreamV1},
    palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwRenderingResourcePricesV1},
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::{PalwStepBindingV2, verify_binding_v1},
    palw_step_refute::PalwDecodeTokensV1,
    palw_v2::{PALW_V2_MAX_TRACE_EVENTS, PALW_V2_MAX_TRACE_OPENING_SIBLINGS, PalwTraceEventOpeningV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

/// The Float32 source's paid signed carrier (the staged court's ordinary carrier bound).
pub const PALW_FLOAT_SOURCE_CARRIER_MAX_V1: usize = 90_000;
const SOURCE_DOMAIN_V1: &[u8] = b"misaka-palw/float-constraint-source/v1";

/// One custodian/owner public-bytes move. Decoding is bounded before allocation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PalwFloatSourceActionV1 {
    /// The claim's trace summary, ordered event commitment and generated ids.
    Ids(PalwDecodeTokensV1),
    /// The nominated row's lanes `[offset, offset + bytes.len() / 4)`, little-endian f32.
    RowPart { offset: u32, bytes: Vec<u8> },
    /// The nominated event's membership opening; re-derives the claim's trace root.
    RowFinish { opening: PalwTraceEventOpeningV2 },
}
impl BorshDeserialize for PalwFloatSourceActionV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        fn bounded<T: BorshDeserialize, R: std::io::Read>(reader: &mut R, max: usize) -> std::io::Result<Vec<T>> {
            let length = u32::deserialize_reader(reader)? as usize;
            if length > max {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Float32 source vector exceeds its paid carrier"));
            }
            let mut values = Vec::new();
            values
                .try_reserve_exact(length)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::OutOfMemory, "Float32 source allocation refused"))?;
            for _ in 0..length {
                values.push(T::deserialize_reader(reader)?);
            }
            Ok(values)
        }
        match u8::deserialize_reader(reader)? {
            0 => Ok(Self::Ids(PalwDecodeTokensV1 {
                summary: crate::palw_v2::PalwTraceSummaryV2::deserialize_reader(reader)?,
                ordered_event_commitment: Hash64::deserialize_reader(reader)?,
                generated_token_ids: bounded(reader, PALW_V2_MAX_TRACE_EVENTS)?,
            })),
            1 => Ok(Self::RowPart { offset: u32::deserialize_reader(reader)?, bytes: bounded(reader, PALW_FLOAT_ROW_PART_BYTES_V1)? }),
            2 => Ok(Self::RowFinish {
                opening: PalwTraceEventOpeningV2 {
                    event_index: u32::deserialize_reader(reader)?,
                    event_hash: Hash64::deserialize_reader(reader)?,
                    siblings: bounded(reader, PALW_V2_MAX_TRACE_OPENING_SIBLINGS)?,
                },
            }),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown Float32 source variant")),
        }
    }
}
impl PalwFloatSourceActionV1 {
    pub fn check_wire_v1(&self) -> Result<(), String> {
        let fits = match self {
            Self::Ids(ids) => ids.generated_token_ids.len() <= PALW_V2_MAX_TRACE_EVENTS,
            Self::RowPart { bytes, .. } => {
                !bytes.is_empty() && bytes.len() <= PALW_FLOAT_ROW_PART_BYTES_V1 && bytes.len().is_multiple_of(4)
            }
            Self::RowFinish { opening } => opening.siblings.len() <= PALW_V2_MAX_TRACE_OPENING_SIBLINGS,
        };
        if !fits || borsh::object_length(self).map_err(|e| e.to_string())? > PALW_FLOAT_SOURCE_CARRIER_MAX_V1 - 5632 {
            return Err("Float32 source part exceeds its bounded original envelope".into());
        }
        Ok(())
    }
}

/// Where one slot stands. Bound into every answer, so an accepted packet cannot stand in for
/// another turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwFloatSourceCursorV1 {
    Ids,
    /// Authenticated ids; the owner has not nominated yet.
    Nominate,
    RowPart {
        offset: u32,
    },
    RowFinish,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFloatSourceAnswerV1 {
    pub answerer: PalwBondKeyV2,
    pub source_id: Hash64,
    pub cursor: PalwFloatSourceCursorV1,
    pub action: PalwFloatSourceActionV1,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PalwFloatSourceSlotV1 {
    ids: Option<PalwDecodeTokensV1>,
    row: Option<PalwFloatRowStreamV1>,
    facts: Option<PalwFloatRowFactsV1>,
}

/// Exposed only from an authenticated slot ([`PalwFloatConstraintSourceSessionV1::authenticated_pin_v1`]);
/// never decoded from a caller.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwFloatConstraintPinV1 {
    binding_root: Hash64,
    job_context: Hash64,
    logits_root: Hash64,
    vocab: u32,
    generated: Vec<u32>,
    position: u32,
    /// The committed lane's f32 order key (`None` for a terminal position or an id outside the vocabulary).
    committed_key: Option<i32>,
    beat: Option<(u32, i32)>,
}
impl PalwFloatConstraintPinV1 {
    pub fn generated_ids_v1(&self) -> &[u32] {
        &self.generated
    }
    pub fn position_v1(&self) -> u32 {
        self.position
    }
    pub fn committed_id_v1(&self) -> Option<u32> {
        self.generated.get(self.position as usize).copied()
    }
    pub fn committed_key_v1(&self) -> Option<i32> {
        self.committed_key
    }
    pub fn beat_v1(&self) -> Option<(u32, i32)> {
        self.beat
    }
    pub fn matches_binding_v1(&self, binding: &PalwStepBindingV2) -> bool {
        binding.shape_profile.lane == crate::palw_step::PalwStepLaneV1::Float32
            && binding.committed_execution_root == self.binding_root
            && binding.job_context.context_hash() == self.job_context
            && binding.full_logits_trace_root == self.logits_root
            && binding.shape_profile.vocab_size == self.vocab
            && binding.job_context.exact_decode_tokens as usize == self.generated.len()
    }
}

/// The per-owner Float32 source. Clocks follow the deferred Flat source: every custodian owes the
/// authenticated ids by `nomination_until_daa`; after the owner's nomination every custodian owes
/// the nominated row by `respond_until_daa`; the reserved span between the two prices every slot's
/// full row stream with its reorg replay.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFloatConstraintSourceSessionV1 {
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
    binding_root: Hash64,
    job_context: Hash64,
    expected: Hash64,
    prefill: u32,
    decode: u32,
    vocab: u32,
    slots: BTreeMap<PalwBondKeyV2, PalwFloatSourceSlotV1>,
    nomination: Option<(u32, Option<u32>)>,
    authenticated: Option<PalwBondKeyV2>,
    ready: Option<PalwBondKeyV2>,
    collected: bool,
}

/// Moves one slot owes for one nominated row (parts + finish), before reorg replay.
pub fn float_row_moves_v1(vocab: u32) -> u64 {
    (u64::from(vocab) * 4).div_ceil(PALW_FLOAT_ROW_PART_BYTES_V1 as u64) + 1
}

impl PalwFloatConstraintSourceSessionV1 {
    fn identity_v1(&self) -> Result<Hash64, String> {
        let bytes = borsh::to_vec(&(
            self.claim,
            self.owner,
            self.reserved_daa,
            self.bundle_id,
            self.producer,
            &self.custodians,
            self.binding_root,
            self.job_context,
            self.expected,
            self.prefill,
            self.decode,
            self.vocab,
            self.respond_until_daa,
            self.collect_until_daa,
            self.nomination_until_daa,
        ))
        .map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(
            blake2b_simd::Params::new().hash_length(64).key(SOURCE_DOMAIN_V1).hash(&bytes).as_bytes().try_into().expect("64"),
        ))
    }

    /// Opens the deferred source over a verified Float32 binding and the claim's GREEDY job.
    #[allow(clippy::too_many_arguments)]
    pub fn new_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        producer: PalwBondKeyV2,
        custodians: BTreeSet<PalwBondKeyV2>,
        binding: &PalwStepBindingV2,
        job: &crate::palw_freeprompt_v3::PalwFreePromptJobV3,
        respond_until_daa: u64,
        collect_until_daa: u64,
        prices: PalwRenderingResourcePricesV1,
    ) -> Result<Self, String> {
        verify_binding_v1(binding).map_err(|e| e.to_string())?;
        if binding.shape_profile.lane != crate::palw_step::PalwStepLaneV1::Float32 {
            return Err("Float32 source cannot reinterpret an integer logits scheme".into());
        }
        if !job.sampling_v2().is_greedy() {
            return Err("a seeded Float32 job has no canonical float sampler; refused at every height".into());
        }
        let decode = binding.job_context.exact_decode_tokens;
        let vocab = binding.shape_profile.vocab_size;
        if !custodians.contains(&producer)
            || custodians.contains(&owner)
            || reserved_daa >= respond_until_daa
            || respond_until_daa >= collect_until_daa
            || vocab == 0
            || decode == 0
            || decode as usize > PALW_V2_MAX_TRACE_EVENTS
        {
            return Err("Float32 source requires literal producer custody, outside owner, original geometry and paid clocks".into());
        }
        let slots = custodians.len() as u128 + 1;
        let reserve = (u128::from(float_row_moves_v1(vocab)) + u128::from(prices.reorg_replay_moves))
            .checked_mul(slots)
            .and_then(|n| n.checked_add(2))
            .and_then(|n| n.checked_mul(u128::from(prices.inclusion_margin_daa)))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or("Float32 row deadline budget overflow")?;
        let nomination_until_daa = respond_until_daa
            .checked_sub(reserve)
            .filter(|until| *until > reserved_daa)
            .ok_or("original Float32 paid window cannot cover both the ids and the nominated row")?;
        let mut session = Self {
            claim,
            owner,
            reserved_daa,
            bundle_id,
            source_id: Hash64::default(),
            respond_until_daa,
            collect_until_daa,
            nomination_until_daa,
            producer,
            slots: custodians.iter().chain([&owner]).map(|key| (*key, PalwFloatSourceSlotV1::default())).collect(),
            custodians,
            binding_root: binding.committed_execution_root,
            job_context: binding.job_context.context_hash(),
            expected: binding.full_logits_trace_root,
            prefill: binding.job_context.declared_prefill_tokens,
            decode,
            vocab,
            nomination: None,
            authenticated: None,
            ready: None,
            collected: false,
        };
        session.source_id = session.identity_v1()?;
        Ok(session)
    }

    /// The row a nomination needs, if any: the committed lane (inside the vocabulary) and the beat.
    fn nominated_lanes_v1(&self, ids: &[u32], position: u32, beat: Option<u32>) -> Option<Vec<u32>> {
        let committed = ids.get(position as usize).copied().filter(|id| *id < self.vocab);
        let lanes: Vec<u32> = committed.into_iter().chain(beat).collect();
        (!lanes.is_empty()).then_some(lanes)
    }

    pub fn check_v1(&self) -> Result<(), String> {
        if self.identity_v1()? != self.source_id
            || !self.custodians.contains(&self.producer)
            || self.custodians.contains(&self.owner)
            || self.reserved_daa >= self.nomination_until_daa
            || self.nomination_until_daa >= self.respond_until_daa
            || self.respond_until_daa >= self.collect_until_daa
            || self.slots.len() != self.custodians.len() + 1
            || !self.slots.contains_key(&self.owner)
            || self.custodians.iter().any(|key| !self.slots.contains_key(key))
        {
            return Err("Float32 source loses immutable literal slots or paid clocks".into());
        }
        for slot in self.slots.values() {
            if let Some(ids) = &slot.ids {
                self.authenticate_ids_v1(ids)?;
            }
            if slot.ids.is_none() && (slot.row.is_some() || slot.facts.is_some()) {
                return Err("Float32 slot streamed a row before its authenticated ids".into());
            }
            if let Some(row) = &slot.row {
                row.check_v1()?;
                let (position, beat) = self.nomination.ok_or("Float32 slot streamed a row before the owner's nomination")?;
                let ids = &slot.ids.as_ref().expect("checked").generated_token_ids;
                if row.event_index_v1() != position
                    || row.n_vocab_v1() != self.vocab
                    || self.nominated_lanes_v1(ids, position, beat).is_none_or(|mut lanes| {
                        lanes.sort_unstable();
                        lanes.dedup();
                        lanes != row.nominated_v1()
                    })
                {
                    return Err("Float32 slot row changes the nominated original event or lanes".into());
                }
            }
            if slot.facts.as_ref().is_some_and(|f| Some(f.event_index) != self.nomination.map(|n| n.0) || slot.row.is_some()) {
                return Err("Float32 slot facts are not the nominated authenticated event".into());
            }
        }
        if self.authenticated.is_some_and(|key| self.slots.get(&key).is_none_or(|slot| slot.ids.is_none()))
            || self.ready.is_some_and(|key| self.slot_cursor_v1(&key).ok() != Some(PalwFloatSourceCursorV1::Complete))
            || self.nomination.is_some_and(|(position, beat)| {
                self.authenticated.is_none()
                    || position > self.decode
                    || beat.is_some_and(|id| id >= self.vocab)
                    || (position == self.decode && beat.is_some())
            })
        {
            return Err("Float32 accepted nomination/ids locator loses original authenticated context".into());
        }
        Ok(())
    }

    fn authenticate_ids_v1(&self, ids: &PalwDecodeTokensV1) -> Result<(), String> {
        let s = &ids.summary;
        if ids.generated_token_ids.len() != self.decode as usize
            || s.vocab_size != self.vocab
            || s.exact_decode_tokens != self.decode
            || s.event_count != self.decode
            || s.declared_prefill_tokens != self.prefill
            || crate::palw_v2::output_token_ids_hash_v2(&ids.generated_token_ids) != s.output_token_ids_hash
            || crate::palw_v2::full_logits_trace_root_v2(&self.job_context, s, &ids.ordered_event_commitment) != self.expected
        {
            return Err("Float32 ids do not reproduce the claim's committed trace root".into());
        }
        Ok(())
    }

    /// O(log custody), read from authenticated native state only.
    pub fn slot_cursor_v1(&self, signer: &PalwBondKeyV2) -> Result<PalwFloatSourceCursorV1, String> {
        let slot = self.slots.get(signer).ok_or("Float32 answerer has no protected literal slot")?;
        let Some(ids) = &slot.ids else { return Ok(PalwFloatSourceCursorV1::Ids) };
        let Some((position, beat)) = self.nomination else { return Ok(PalwFloatSourceCursorV1::Nominate) };
        if slot.facts.is_some() || self.nominated_lanes_v1(&ids.generated_token_ids, position, beat).is_none() {
            return Ok(PalwFloatSourceCursorV1::Complete);
        }
        Ok(match &slot.row {
            Some(row) if row.lanes_read_v1() == self.vocab => PalwFloatSourceCursorV1::RowFinish,
            Some(row) => PalwFloatSourceCursorV1::RowPart { offset: row.lanes_read_v1() },
            None => PalwFloatSourceCursorV1::RowPart { offset: 0 },
        })
    }

    pub fn response_deadline_v1(&self, signer: &PalwBondKeyV2) -> Option<u64> {
        let slot = self.slots.get(signer)?;
        Some(if slot.ids.is_none() { self.nomination_until_daa } else { self.respond_until_daa })
    }

    pub fn answer_v1(&mut self, now: u64, answer: &PalwFloatSourceAnswerV1) -> Result<(), String> {
        if now < self.reserved_daa
            || self.response_deadline_v1(&answer.answerer).is_none_or(|until| now >= until)
            || self.collected
            || answer.source_id != self.source_id
            || answer.cursor != self.slot_cursor_v1(&answer.answerer)?
        {
            return Err("Float32 source answer substitutes context, turn or exclusive paid deadline".into());
        }
        answer.action.check_wire_v1()?;
        // An invalid part leaves the persisted slot and every other slot untouched.
        let mut slot = self.slots.get(&answer.answerer).expect("checked").clone();
        match (answer.cursor, &answer.action) {
            (PalwFloatSourceCursorV1::Ids, PalwFloatSourceActionV1::Ids(ids)) => {
                self.authenticate_ids_v1(ids)?;
                slot.ids = Some(ids.clone());
            }
            (PalwFloatSourceCursorV1::RowPart { offset }, PalwFloatSourceActionV1::RowPart { offset: at, bytes }) if offset == *at => {
                let (position, beat) = self.nomination.expect("cursor requires nomination");
                let ids = &slot.ids.as_ref().expect("cursor requires ids").generated_token_ids;
                let mut row = match slot.row.take() {
                    Some(row) => row,
                    None => {
                        let lanes = self.nominated_lanes_v1(ids, position, beat).expect("cursor requires a nominated lane");
                        PalwFloatRowStreamV1::new_v1(&self.job_context, position, self.vocab, &lanes)?
                    }
                };
                row.part_v1(offset, bytes)?;
                slot.row = Some(row);
            }
            (PalwFloatSourceCursorV1::RowFinish, PalwFloatSourceActionV1::RowFinish { opening }) => {
                let ids = slot.ids.as_ref().expect("cursor requires ids");
                let row = slot.row.take().expect("cursor requires a complete row");
                slot.facts =
                    Some(row.finish_v1(&self.job_context, &self.expected, &ids.summary, &ids.ordered_event_commitment, opening)?);
            }
            _ => return Err("Float32 action changes exact original source cursor".into()),
        }
        self.slots.insert(answer.answerer, slot);
        if self.authenticated.is_none() && self.slots[&answer.answerer].ids.is_some() {
            self.authenticated = Some(answer.answerer);
        }
        if self.ready.is_none() && self.slot_cursor_v1(&answer.answerer)? == PalwFloatSourceCursorV1::Complete {
            self.ready = Some(answer.answerer);
        }
        Ok(())
    }

    /// Opening does not move either paid clock; the first phase keeps at least the reserved row span.
    pub fn check_open_window_v1(&self, now: u64) -> Result<(), String> {
        let span = self.respond_until_daa.checked_sub(self.nomination_until_daa).ok_or("Float32 clock underflow")?;
        if now < self.reserved_daa || now >= self.nomination_until_daa || now.checked_add(span).is_none_or(|end| end > self.nomination_until_daa)
        {
            return Err("Float32 source opened too late for its original paid ids window".into());
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
            || self.nomination.is_some()
            || now < self.reserved_daa
            || now >= self.nomination_until_daa
            || self.collected
            || self.authenticated.is_none()
            || position > self.decode
            || beat.is_some_and(|id| id >= self.vocab)
            || (position == self.decode && beat.is_some())
        {
            return Err("Float32 owner nomination changes authenticated ids, paid clock or repeats selection".into());
        }
        self.nomination = Some((position, beat));
        // A nomination that needs no row (terminal, or an out-of-vocabulary committed id with no
        // beat) is complete on the authenticated ids alone.
        let signer = self.authenticated.expect("checked");
        if self.ready.is_none() && self.slot_cursor_v1(&signer)? == PalwFloatSourceCursorV1::Complete {
            self.ready = Some(signer);
        }
        Ok(())
    }

    pub fn nomination_v1(&self) -> Option<(u32, Option<u32>)> {
        self.nomination
    }
    pub fn needs_nomination_v1(&self) -> bool {
        self.nomination.is_none() && self.authenticated.is_some()
    }
    pub fn ready_signer_v1(&self) -> Option<PalwBondKeyV2> {
        self.ready
    }
    pub fn authenticated_generated_ids_v1(&self) -> Result<&[u32], String> {
        let key = self.authenticated.ok_or("Float32 source has no authenticated ids")?;
        Ok(&self.slots.get(&key).and_then(|s| s.ids.as_ref()).ok_or("Float32 ids locator missing")?.generated_token_ids)
    }
    pub fn has_response_duty_v1(&self, signer: &PalwBondKeyV2) -> bool {
        !self.collected
            && self.custodians.contains(signer)
            && self
                .slot_cursor_v1(signer)
                .is_ok_and(|cursor| !matches!(cursor, PalwFloatSourceCursorV1::Complete | PalwFloatSourceCursorV1::Nominate))
    }
    pub fn collection_due_daa_v1(&self) -> u64 {
        if self.nomination.is_none() { self.nomination_until_daa } else { self.respond_until_daa }
    }
    pub fn collected_v1(&self) -> bool {
        self.collected
    }

    /// The selection facts of a slot that completed the nominated position.
    pub fn authenticated_pin_v1(&self, signer: &PalwBondKeyV2) -> Result<PalwFloatConstraintPinV1, String> {
        if self.slot_cursor_v1(signer)? != PalwFloatSourceCursorV1::Complete {
            return Err("Float32 source exposes no semantic pin before an authenticated nominated row".into());
        }
        let (position, beat) = self.nomination.expect("complete requires nomination");
        let slot = &self.slots[signer];
        let ids = slot.ids.as_ref().expect("complete requires ids");
        let committed = ids.generated_token_ids.get(position as usize).copied().filter(|id| *id < self.vocab);
        let key = |lane: u32| -> Result<i32, String> {
            slot.facts
                .as_ref()
                .and_then(|f| f.keys.get(&lane).copied())
                .ok_or_else(|| "Float32 pin lacks an authenticated nominated key".to_string())
        };
        Ok(PalwFloatConstraintPinV1 {
            binding_root: self.binding_root,
            job_context: self.job_context,
            logits_root: self.expected,
            vocab: self.vocab,
            generated: ids.generated_token_ids.clone(),
            position,
            committed_key: committed.map(key).transpose()?,
            beat: beat.map(|b| key(b).map(|k| (b, k))).transpose()?,
        })
    }

    /// Literal identities to sanction in the native fold, not a local assertion of conviction.
    pub fn collect_v1(&mut self, now: u64) -> Result<Vec<PalwBondKeyV2>, String> {
        if self.collected || now < self.collection_due_daa_v1() || now >= self.collect_until_daa {
            return Err("Float32 collection is outside original exclusive clocks or repeats a sanction".into());
        }
        let missing = self
            .custodians
            .iter()
            .filter(|key| match self.slot_cursor_v1(key) {
                Ok(PalwFloatSourceCursorV1::Complete) => false,
                Ok(PalwFloatSourceCursorV1::Nominate) => false,
                _ => true,
            })
            .copied()
            .collect();
        self.collected = true;
        Ok(missing)
    }
}

/// Full native custody multiplicity: every slot's ids carrier and one full nominated row with its
/// reorg replay, plus the owner's nomination and collection. The caller supplies the chain's
/// maximum qualified custody, not merely current signers.
pub fn float_constraint_source_resource_plan_v1(
    decode: u32,
    vocab: u32,
    profile_bytes: u64,
    maximum_custodians: u32,
    prices: PalwRenderingResourcePricesV1,
) -> Result<PalwRenderingResourcePlanV1, String> {
    if maximum_custodians == 0
        || vocab == 0
        || decode == 0
        || decode as usize > PALW_V2_MAX_TRACE_EVENTS
        || prices.inclusion_margin_daa == 0
    {
        return Err("Float32 source quote needs original custody, event and vocabulary envelope".into());
    }
    let n = |x: u128| u64::try_from(x).map_err(|_| "Float32 source quote overflow".to_string());
    let slots = u128::from(maximum_custodians) + 1;
    let d = u128::from(decode);
    let v = u128::from(vocab);
    let per_slot_moves = 1 + u128::from(float_row_moves_v1(vocab)) + u128::from(prices.reorg_replay_moves);
    let moves = per_slot_moves * slots + 2;
    // Ids carrier (summary, ordered root, ids) + one f32 row + opening, per slot.
    let per_slot_bytes = 256 + d * 4 + v * 4 + (PALW_V2_MAX_TRACE_OPENING_SIBLINGS as u128 + 1) * 64;
    let retained = per_slot_bytes * slots + moves * 5632;
    // Stored slot: ids + one row continuation (128-byte buffer, words, nominated keys) + facts.
    let row = slots * (d * 4 + 1024) + u128::from(profile_bytes) + 2048;
    let blocks = slots * ((v * 4 + 128).div_ceil(128) + (d * 4).div_ceil(128) + 64);
    let work = slots * (v + d) * 4;
    Ok(PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab,
        maximum_stream_bytes: n(retained)?,
        maximum_stream_leaves: u64::from(maximum_custodians) + 1,
        maximum_eog_bytes: 0,
        maximum_moves: n(moves)?,
        maximum_parser_steps: n(work)?,
        maximum_blake_blocks: n(blocks)?,
        inclusion_daa: n(moves * u128::from(prices.inclusion_margin_daa))?,
        rent_sompi: n(retained * u128::from(prices.rent_per_stream_byte)
            + row * u128::from(prices.rent_per_row_byte)
            + work * u128::from(prices.rent_per_parser_step))?,
        deposit_sompi: n(moves * u128::from(prices.deposit_per_move))?,
        row_bytes: n(row)?,
        resident_ram_bytes: n(row * 2 + PALW_FLOAT_ROW_PART_BYTES_V1 as u128 * 2 + 65_536)?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::palw_float_decode_v1::palw_float32_event_phase_v1;
    use crate::palw_v2::{
        PalwLogitsDtypeV2, PalwStopReasonV2, PalwTracePhaseV2, PalwTraceSummaryV2, logits_event_hash_v2, output_token_ids_hash_v2,
        trace_event_merkle_root_v2, trace_event_opening_v2,
    };

    /// A public Float32 capture: rows, ids, events, and the trace pin they commit to.
    pub(crate) struct FloatCaptureV1 {
        pub rows: Vec<Vec<f32>>,
        pub events: Vec<Hash64>,
        pub ids: PalwDecodeTokensV1,
        pub root: Hash64,
    }
    pub(crate) fn float_capture_v1(context: Hash64, prefill: u32, rows: Vec<Vec<f32>>, generated: Vec<u32>) -> FloatCaptureV1 {
        let vocab = rows[0].len() as u32;
        let mut scratch = Vec::new();
        let events: Vec<Hash64> = rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let (phase, step) = palw_float32_event_phase_v1(i as u32);
                logits_event_hash_v2(&context, phase, step, i as u32, vocab, row, &mut scratch).unwrap()
            })
            .collect();
        let ordered = trace_event_merkle_root_v2(&events).unwrap();
        let summary = PalwTraceSummaryV2 {
            vocab_size: vocab,
            logits_dtype: PalwLogitsDtypeV2::F32Le,
            declared_prefill_tokens: prefill,
            exact_decode_tokens: rows.len() as u32,
            event_count: rows.len() as u32,
            first_event_kind: PalwTracePhaseV2::Prefill,
            last_event_kind: if rows.len() == 1 { PalwTracePhaseV2::Prefill } else { PalwTracePhaseV2::Decode },
            output_token_ids_hash: output_token_ids_hash_v2(&generated),
            stop_reason: PalwStopReasonV2::ExactBudgetReached,
        };
        let root = crate::palw_v2::full_logits_trace_root_v2(&context, &summary, &ordered);
        FloatCaptureV1 {
            rows,
            events,
            ids: PalwDecodeTokensV1 { summary, ordered_event_commitment: ordered, generated_token_ids: generated },
            root,
        }
    }
    /// The next honest action for a slot at `cursor`, from public capture bytes only.
    pub(crate) fn float_source_action_v1(
        capture: &FloatCaptureV1,
        cursor: PalwFloatSourceCursorV1,
        position: u32,
        part_lanes: usize,
    ) -> Option<PalwFloatSourceActionV1> {
        match cursor {
            PalwFloatSourceCursorV1::Ids => Some(PalwFloatSourceActionV1::Ids(capture.ids.clone())),
            PalwFloatSourceCursorV1::RowPart { offset } => {
                let row = &capture.rows[position as usize];
                let end = row.len().min(offset as usize + part_lanes);
                Some(PalwFloatSourceActionV1::RowPart {
                    offset,
                    bytes: row[offset as usize..end].iter().flat_map(|v| v.to_le_bytes()).collect(),
                })
            }
            PalwFloatSourceCursorV1::RowFinish => {
                Some(PalwFloatSourceActionV1::RowFinish { opening: trace_event_opening_v2(&capture.events, position).unwrap() })
            }
            PalwFloatSourceCursorV1::Nominate | PalwFloatSourceCursorV1::Complete => None,
        }
    }

    #[test]
    fn row_continuation_survives_a_serialized_restart_and_rejects_a_moved_byte_position() {
        let context = Hash64::from_u64_word(0xF2);
        let capture = float_capture_v1(context, 2, vec![vec![0.5, 2.0, -1.0, 2.0, 7.0], vec![9.0, 1.0, 0.0, 3.0, -2.0]], vec![4, 0]);
        let bytes: Vec<u8> = capture.rows[1].iter().flat_map(|v| v.to_le_bytes()).collect();
        let mut stream = PalwFloatRowStreamV1::new_v1(&context, 1, 5, &[0, 3]).unwrap();
        stream.part_v1(0, &bytes[..8]).unwrap();
        // The chain stores the continuation between blocks and reloads it.
        let stored = borsh::to_vec(&stream).unwrap();
        let mut reloaded: PalwFloatRowStreamV1 = borsh::from_slice(&stored).unwrap();
        reloaded.check_v1().unwrap();
        reloaded.part_v1(2, &bytes[8..]).unwrap();
        reloaded.check_v1().unwrap();
        let facts = reloaded
            .finish_v1(
                &context,
                &capture.root,
                &capture.ids.summary,
                &capture.ids.ordered_event_commitment,
                &trace_event_opening_v2(&capture.events, 1).unwrap(),
            )
            .unwrap();
        assert_eq!((facts.greedy_lane, facts.keys.len()), (0, 2));
        // A stored continuation whose lane count disagrees with its hashed bytes is refused.
        let mut forged: PalwFloatRowStreamV1 = borsh::from_slice(&stored).unwrap();
        forged.lanes_read += 1;
        assert!(forged.check_v1().is_err());
    }

    #[test]
    fn source_authenticates_ids_then_the_nominated_row_per_isolated_slot() {
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let (binding, job, capture) = crate::palw_float_constraint_source_v1::tests::float_binding_case_for_tests_v1();
        let prices = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1().prices;
        let (producer, panel, owner) = (key(1), key(2), key(9));
        let mut session = PalwFloatConstraintSourceSessionV1::new_v1(
            Hash64::from_u64_word(7),
            owner,
            10,
            Hash64::from_u64_word(8),
            producer,
            BTreeSet::from([producer, panel]),
            &binding,
            &job,
            1_000_000,
            1_100_000,
            prices,
        )
        .unwrap();
        session.check_v1().unwrap();
        session.check_open_window_v1(11).unwrap();
        let answer = |session: &PalwFloatConstraintSourceSessionV1, signer, action| PalwFloatSourceAnswerV1 {
            answerer: signer,
            source_id: session.source_id,
            cursor: session.slot_cursor_v1(&signer).unwrap(),
            action,
        };
        // A forged ids carrier (one id changed) never authenticates.
        let mut forged = capture.ids.clone();
        forged.generated_token_ids[0] ^= 1;
        assert!(session.answer_v1(12, &answer(&session, panel, PalwFloatSourceActionV1::Ids(forged))).is_err());
        assert!(session.nominate_v1(12, owner, session.source_id, 0, Some(1)).is_err(), "no nomination before authenticated ids");
        session.answer_v1(12, &answer(&session, producer, PalwFloatSourceActionV1::Ids(capture.ids.clone()))).unwrap();
        session.nominate_v1(13, owner, session.source_id, 0, Some(1)).unwrap();
        // The producer withholds its row; the Panel member streams it in parts.
        session.answer_v1(14, &answer(&session, panel, PalwFloatSourceActionV1::Ids(capture.ids.clone()))).unwrap();
        while session.slot_cursor_v1(&panel).unwrap() != PalwFloatSourceCursorV1::Complete {
            let cursor = session.slot_cursor_v1(&panel).unwrap();
            let action = float_source_action_v1(&capture, cursor, 0, 2).unwrap();
            session.answer_v1(15, &answer(&session, panel, action)).unwrap();
            session.check_v1().unwrap();
        }
        let pin = session.authenticated_pin_v1(&panel).unwrap();
        assert!(pin.matches_binding_v1(&binding));
        assert_eq!(pin.committed_id_v1(), Some(capture.ids.generated_token_ids[0]));
        assert!(pin.beat_v1().is_some() && pin.committed_key_v1().is_some());
        assert!(session.authenticated_pin_v1(&producer).is_err());
        // The producer missed its row: the native fold collects it, never the honest Panel member.
        assert!(session.collect_v1(15).is_err(), "before the exclusive row clock");
        assert_eq!(session.collect_v1(session.respond_until_daa).unwrap(), vec![producer]);
    }

    /// A verified Float32 binding over a public capture whose committed token 0 is NOT the
    /// greedy lane (the producer picked lane 3; lane 1 wins the tie at a lower index).
    pub(crate) fn float_binding_case_for_tests_v1()
    -> (PalwStepBindingV2, crate::palw_freeprompt_v3::PalwFreePromptJobV3, FloatCaptureV1) {
        let (mut binding, job) = crate::palw_float_constraint_source_v1::tests::float_constrained_binding_for_tests_v1();
        let context = binding.job_context.context_hash();
        let vocab = binding.shape_profile.vocab_size as usize;
        let decode = binding.job_context.exact_decode_tokens as usize;
        let rows: Vec<Vec<f32>> =
            (0..decode).map(|r| (0..vocab).map(|l| if l == 1 || l == 3 { 2.0 } else { (r + l) as f32 * -0.25 }).collect()).collect();
        let mut generated = vec![1u32; decode];
        generated[0] = 3;
        let capture = float_capture_v1(context, binding.job_context.declared_prefill_tokens, rows, generated);
        binding.full_logits_trace_root = capture.root;
        crate::palw_step_refute::tests::rebind_committed_root(&mut binding);
        verify_binding_v1(&binding).unwrap();
        (binding, job, capture)
    }
    /// The Float32 lane of the existing constrained public fixture (the trace root is re-derived
    /// by the caller from its own capture; nothing else about the binding changes).
    pub(crate) fn float_constrained_binding_for_tests_v1() -> (PalwStepBindingV2, crate::palw_freeprompt_v3::PalwFreePromptJobV3) {
        crate::palw_decode_constraint_v1::tests::public_float_constrained_binding_for_tests_v1()
    }
}
