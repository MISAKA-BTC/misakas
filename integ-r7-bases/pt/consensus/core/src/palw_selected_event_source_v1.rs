//! Prospective selected tiled-event acquisition. Coordinates are full-width and each
//! answer authenticates the original logits root, never a reinterpreted flat scheme.
use crate::{
    Hash64,
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1},
    palw_step_refute::{PalwTraceEventDisclosureV1, tiled_tile_authenticate_v1},
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

/// Original row-zero IDs, then at most two selected position tiles.
pub const PALW_SELECTED_EVENT_MAX_REQUESTS_V1: usize = 3;

pub fn selected_event_response_span_v1(
    custodians: u64,
    config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
) -> Result<u64, String> {
    custodians
        .checked_add(2)
        .and_then(|n| n.checked_mul(config.prices.inclusion_margin_daa))
        .and_then(|n| n.checked_add(config.acquisition_daa))
        .ok_or_else(|| "selected response preparation/inclusion span overflow".into())
}
pub const PALW_SELECTED_EVENT_MAX_BYTES_V1: usize = 65_536;
pub const PALW_SELECTED_EVENT_CARRIER_MAX_V1: usize = 90_000;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventBindingV1 {
    pub binding: Box<PalwStepBindingV2>,
    pub profile: crate::palw_constraint_stream_carriage_v1::PalwRetainedCourtProfileV1,
}
impl PalwSelectedEventBindingV1 {
    pub fn from_binding_v1(binding: &PalwStepBindingV2) -> Result<Self, String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        let class_id = binding.shape_profile.shape_profile_id();
        let original_bytes = borsh::object_length(&binding.shape_profile).map_err(|e| e.to_string())? as u64;
        let mut small = binding.clone();
        small.shape_profile = crate::palw_constraint_stream_carriage_v1::palw_retained_profile_skeleton_v1(&binding.shape_profile);
        Ok(Self {
            binding: Box::new(small),
            profile: crate::palw_constraint_stream_carriage_v1::PalwRetainedCourtProfileV1 { class_id, original_bytes },
        })
    }
    pub fn resolve_v1(
        &self,
        actual_class: Hash64,
        profile: &crate::palw_step::PalwShapeProfileV3,
    ) -> Result<PalwStepBindingV2, String> {
        if self.profile.class_id != actual_class
            || self.binding.job_context.shape_profile_id != actual_class
            || profile.shape_profile_id() != actual_class
            || borsh::object_length(profile).map_err(|e| e.to_string())? as u64 != self.profile.original_bytes
            || self.binding.shape_profile != crate::palw_constraint_stream_carriage_v1::palw_retained_profile_skeleton_v1(profile)
        {
            return Err("selected source changes original retained class/profile".into());
        }
        let mut binding = *self.binding.clone();
        binding.shape_profile = profile.clone();
        crate::palw_step_leg::verify_binding_v1(&binding).map_err(|e| e.to_string())?;
        Ok(binding)
    }
}

/// The caller sums this quote into the immutable required-family bundle before admission.
pub fn selected_event_resource_plan_v1(
    vocab: u32,
    decode: u32,
    profile_bytes: u64,
    maximum_custodians: u32,
    config: crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
) -> Result<crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1, String> {
    if vocab == 0
        || u64::from(vocab) >= crate::palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_LEAVES_V1
        || decode == 0
        || decode as usize > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
        || maximum_custodians == 0
    {
        return Err("selected source quote has no bounded original vocabulary/decode/custody".into());
    }
    let n = |x: u128| u64::try_from(x).map_err(|_| "selected source resource quote overflow".to_string());
    let moves = (PALW_SELECTED_EVENT_MAX_REQUESTS_V1 as u64)
        .checked_mul(u64::from(maximum_custodians) + 2)
        .ok_or("selected source move quote overflow")?;
    let bytes = n(u128::from(moves) * PALW_SELECTED_EVENT_CARRIER_MAX_V1 as u128)?;
    let work = n(u128::from(moves) * (u128::from(decode) * u128::from(vocab) * 16 + u128::from(profile_bytes) * 16 + 64 * 256))?;
    let row = n(PALW_SELECTED_EVENT_MAX_REQUESTS_V1 as u128 * PALW_SELECTED_EVENT_CARRIER_MAX_V1 as u128
        + u128::from(profile_bytes)
        + u128::from(maximum_custodians) * 1280
        + 8192)?;
    let retained = n(u128::from(bytes) + u128::from(row) * u128::from(moves))?;
    Ok(crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab,
        maximum_stream_bytes: retained,
        maximum_stream_leaves: PALW_SELECTED_EVENT_MAX_REQUESTS_V1 as u64,
        maximum_eog_bytes: 0,
        maximum_moves: moves,
        maximum_parser_steps: work,
        maximum_blake_blocks: n(u128::from(work).div_ceil(64))?,
        inclusion_daa: n(u128::from(moves) * u128::from(config.prices.inclusion_margin_daa) + u128::from(config.acquisition_daa) * 2)?,
        rent_sompi: n(u128::from(retained) * u128::from(config.prices.rent_per_stream_byte)
            + u128::from(work) * u128::from(config.prices.rent_per_parser_step)
            + u128::from(row) * u128::from(config.prices.rent_per_row_byte))?,
        deposit_sompi: n(u128::from(moves) * u128::from(config.prices.deposit_per_move))?,
        row_bytes: row,
        resident_ram_bytes: n(u128::from(row) * 8 + 65_536)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventCoordinateV1 {
    pub row: u32,
    pub tile: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwSelectedTiledEventV1 {
    pub generated_token_ids: Vec<u32>,
    pub row_root: Hash64,
    pub row_opening: PalwStepOpeningV1,
    pub tile_lanes: Vec<i32>,
    pub tile_opening: PalwStepOpeningV1,
}
impl BorshDeserialize for PalwSelectedTiledEventV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        fn bounded<T: BorshDeserialize, R: std::io::Read>(reader: &mut R, max: usize) -> std::io::Result<Vec<T>> {
            let length = u32::deserialize_reader(reader)? as usize;
            if length > max {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "selected event vector exceeds bounded source"));
            }
            let mut values = Vec::new();
            values
                .try_reserve_exact(length)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::OutOfMemory, "selected event allocation refused"))?;
            for _ in 0..length {
                values.push(T::deserialize_reader(reader)?);
            }
            Ok(values)
        }
        fn opening<R: std::io::Read>(reader: &mut R) -> std::io::Result<PalwStepOpeningV1> {
            Ok(PalwStepOpeningV1 {
                leaf_index: u64::deserialize_reader(reader)?,
                leaf_hash: Hash64::deserialize_reader(reader)?,
                siblings: bounded(reader, 64)?,
            })
        }
        Ok(Self {
            generated_token_ids: bounded(reader, crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS)?,
            row_root: Hash64::deserialize_reader(reader)?,
            row_opening: opening(reader)?,
            tile_lanes: bounded(reader, crate::palw_step_refute::PALW_LOGITS_TILE_LANES)?,
            tile_opening: opening(reader)?,
        })
    }
}
impl PalwSelectedTiledEventV1 {
    /// Producer/custodian-side helper only. The verifier consumes the bounded authenticated
    /// result and never receives the capture used to produce these original openings.
    pub fn of_public_rows_v1(
        binding: &PalwStepBindingV2,
        rows: &[Vec<i32>],
        generated: &[u32],
        coordinate: PalwSelectedEventCoordinateV1,
        maximum_leaves: u64,
    ) -> Result<Self, String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        Self::of_public_rows_view_v1(
            &crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding),
            rows,
            generated,
            coordinate,
            maximum_leaves,
        )
    }
    /// The producer/custodian helper over a decode view (a TIR claim's: its custodian projects
    /// the TIR executor's committed logits rows). The verifier never receives the capture.
    pub fn of_public_rows_view_v1(
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        rows: &[Vec<i32>],
        generated: &[u32],
        coordinate: PalwSelectedEventCoordinateV1,
        maximum_leaves: u64,
    ) -> Result<Self, String> {
        let vocab = view.vocab_size as usize;
        if !view.is_tiled_v1()
            || rows.len() != view.job_context.exact_decode_tokens as usize
            || generated.len() != rows.len()
            || rows.iter().any(|row| row.len() != vocab)
            || coordinate.row as usize >= rows.len()
        {
            return Err("selected source capture is not the original tiled shape".into());
        }
        let context = view.job_context.context_hash();
        let row_leaf = |r: u64| {
            crate::palw_step_refute::tiled_logits_row_root_v1(&context, r as u32, &rows[r as usize])
                .expect("nonempty checked vocabulary")
        };
        let row_opening = crate::palw_step_leg::step_opening_from_leaf_source_capped_v1(
            rows.len() as u64,
            u64::from(coordinate.row),
            maximum_leaves,
            row_leaf,
        )
        .map_err(|e| e.to_string())?;
        let row_root = row_opening.leaf_hash;
        let row = &rows[coordinate.row as usize];
        let tiles = vocab.div_ceil(crate::palw_step_refute::PALW_LOGITS_TILE_LANES) as u64;
        let tile_leaf = |t: u64| {
            let start = t as usize * crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
            crate::palw_step_refute::tiled_logits_tile_leaf_v1(
                &context,
                coordinate.row,
                t as u32,
                &row[start..(start + crate::palw_step_refute::PALW_LOGITS_TILE_LANES).min(row.len())],
            )
        };
        let tile_opening = crate::palw_step_leg::step_opening_from_leaf_source_capped_v1(
            tiles,
            u64::from(coordinate.tile),
            maximum_leaves,
            tile_leaf,
        )
        .map_err(|e| e.to_string())?;
        let start = coordinate.tile as usize * crate::palw_step_refute::PALW_LOGITS_TILE_LANES;
        let lanes = row
            .get(start..(start + crate::palw_step_refute::PALW_LOGITS_TILE_LANES).min(row.len()))
            .ok_or("selected source tile is outside original vocabulary")?;
        let selected =
            Self { generated_token_ids: generated.to_vec(), row_root, row_opening, tile_lanes: lanes.to_vec(), tile_opening };
        selected.authenticate_view_v1(view, coordinate, maximum_leaves)?;
        Ok(selected)
    }
    pub fn check_shape_v1(&self) -> Result<(), String> {
        if self.generated_token_ids.len() > crate::palw_v2::PALW_V2_MAX_TRACE_EVENTS
            || self.tile_lanes.len() > crate::palw_step_refute::PALW_LOGITS_TILE_LANES
            || self.row_opening.siblings.len() > 64
            || self.tile_opening.siblings.len() > 64
            || borsh::object_length(self).map_err(|e| e.to_string())? > PALW_SELECTED_EVENT_MAX_BYTES_V1
        {
            return Err("selected public event exceeds its bounded source shape".into());
        }
        Ok(())
    }

    pub fn authenticate_v1(
        &self,
        binding: &PalwStepBindingV2,
        coordinate: PalwSelectedEventCoordinateV1,
        maximum_leaves: u64,
    ) -> Result<(), String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        self.authenticate_view_v1(&crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding), coordinate, maximum_leaves)
    }

    /// The same authentication over a decode view (legacy binding or native TIR header).
    pub fn authenticate_view_v1(
        &self,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        coordinate: PalwSelectedEventCoordinateV1,
        maximum_leaves: u64,
    ) -> Result<(), String> {
        self.check_shape_v1()?;
        let (context, vocab, tiles) = crate::palw_step_refute::tiled_row_authenticate_view_v1(
            view,
            &self.generated_token_ids,
            coordinate.row,
            &self.row_root,
            &self.row_opening,
            maximum_leaves,
        )
        .map_err(|e| e.to_string())?;
        tiled_tile_authenticate_v1(
            &context,
            coordinate.row,
            vocab,
            tiles,
            &self.row_root,
            u64::from(coordinate.tile),
            &self.tile_lanes,
            &self.tile_opening,
            maximum_leaves,
        )
        .map_err(|e| e.to_string())
    }

    pub fn from_disclosure_v1(event: &PalwTraceEventDisclosureV1) -> Result<Self, String> {
        let PalwTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } = event
        else {
            return Err("selected tiled source cannot convert a flat or out-of-range disclosure".into());
        };
        let selected = Self {
            generated_token_ids: generated_token_ids.clone(),
            row_root: *row_root,
            row_opening: row_opening.clone(),
            tile_lanes: tile_lanes.clone(),
            tile_opening: tile_opening.clone(),
        };
        selected.check_shape_v1()?;
        Ok(selected)
    }

    pub fn into_disclosure_v1(self, binding: PalwStepBindingV2) -> PalwTraceEventDisclosureV1 {
        PalwTraceEventDisclosureV1::Tiled {
            binding: Box::new(binding),
            generated_token_ids: self.generated_token_ids,
            row_root: self.row_root,
            row_opening: self.row_opening,
            tile_lanes: self.tile_lanes,
            tile_opening: self.tile_opening,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventDemandV1 {
    pub coordinate: PalwSelectedEventCoordinateV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventAnswerV1 {
    pub answerer: PalwBondKeyV2,
    pub demand_id: Hash64,
    pub coordinate: PalwSelectedEventCoordinateV1,
    pub binding: PalwSelectedEventBindingV1,
    pub event: PalwSelectedTiledEventV1,
}
impl PalwSelectedEventAnswerV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        self.event.check_shape_v1()?;
        if borsh::object_length(self).map_err(|e| e.to_string())? > PALW_SELECTED_EVENT_CARRIER_MAX_V1 - 5632 {
            return Err("selected retained-profile source exceeds paid signed carrier".into());
        }
        Ok(())
    }
}

/// **A TIR claim's selected answer** (behind `palw_staged_tir_route_v1`): no binding rides the
/// carrier. The fold derives the decode view from the claim's admitted TIR public header and its
/// registered class record, and authenticates `event` against that view only.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventNativeAnswerV1 {
    pub answerer: PalwBondKeyV2,
    pub demand_id: Hash64,
    pub coordinate: PalwSelectedEventCoordinateV1,
    pub event: PalwSelectedTiledEventV1,
}
impl PalwSelectedEventNativeAnswerV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        self.event.check_shape_v1()?;
        if borsh::object_length(self).map_err(|e| e.to_string())? > PALW_SELECTED_EVENT_CARRIER_MAX_V1 - 5632 {
            return Err("native selected source exceeds paid signed carrier".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventRequestV1 {
    pub coordinate: PalwSelectedEventCoordinateV1,
    pub demand_id: Hash64,
    pub answered: BTreeSet<PalwBondKeyV2>,
    pub event: Option<PalwSelectedTiledEventV1>,
    pub collected: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedEventSessionV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub bundle_id: Hash64,
    pub respond_until_daa: u64,
    pub collect_until_daa: u64,
    pub minimum_response_span_daa: u64,
    pub custodians: BTreeSet<PalwBondKeyV2>,
    pub requests: BTreeMap<PalwSelectedEventCoordinateV1, PalwSelectedEventRequestV1>,
}
impl PalwSelectedEventSessionV1 {
    pub fn new_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        respond_until_daa: u64,
        collect_until_daa: u64,
        custodians: BTreeSet<PalwBondKeyV2>,
    ) -> Result<Self, String> {
        Self::new_with_span_v1(claim, owner, reserved_daa, bundle_id, respond_until_daa, collect_until_daa, 1, custodians)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_span_v1(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        bundle_id: Hash64,
        respond_until_daa: u64,
        collect_until_daa: u64,
        minimum_response_span_daa: u64,
        custodians: BTreeSet<PalwBondKeyV2>,
    ) -> Result<Self, String> {
        if custodians.is_empty()
            || custodians.contains(&owner)
            || reserved_daa >= respond_until_daa
            || respond_until_daa > collect_until_daa
            || minimum_response_span_daa == 0
            || reserved_daa.checked_add(minimum_response_span_daa).is_none_or(|until| until > respond_until_daa)
        {
            return Err("selected source needs literal outside owner, signed custody and original clock".into());
        }
        Ok(Self {
            claim,
            owner,
            reserved_daa,
            bundle_id,
            respond_until_daa,
            collect_until_daa,
            minimum_response_span_daa,
            custodians,
            requests: BTreeMap::new(),
        })
    }
    pub fn can_demand_at_v1(&self, now: u64) -> bool {
        now >= self.reserved_daa
            && now < self.respond_until_daa
            && now.checked_add(self.minimum_response_span_daa).is_some_and(|until| until <= self.respond_until_daa)
    }
    pub fn demand_v1(&mut self, now: u64, binding: &PalwStepBindingV2, demand: &PalwSelectedEventDemandV1) -> Result<(), String> {
        self.demand_view_v1(now, &crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding), demand)
    }
    /// [`Self::demand_v1`] over a decode view (legacy binding or native TIR header).
    pub fn demand_view_v1(
        &mut self,
        now: u64,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        demand: &PalwSelectedEventDemandV1,
    ) -> Result<(), String> {
        let c = demand.coordinate;
        let tiles = view.tiles_v1();
        if !self.can_demand_at_v1(now)
            || c.row >= view.job_context.exact_decode_tokens
            || u64::from(c.tile) >= tiles
            || !view.is_tiled_v1()
            || self.requests.contains_key(&c)
            || self.requests.len() >= PALW_SELECTED_EVENT_MAX_REQUESTS_V1
        {
            return Err("selected demand changes original scheme, bounded coordinates or paid acquisition clock".into());
        }
        let bytes = borsh::to_vec(&(
            self.claim,
            self.owner,
            self.reserved_daa,
            self.bundle_id,
            c,
            self.respond_until_daa,
            self.collect_until_daa,
            self.minimum_response_span_daa,
            &self.custodians,
        ))
        .map_err(|e| e.to_string())?;
        let id = Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/selected-event-demand/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        );
        self.requests.insert(
            c,
            PalwSelectedEventRequestV1 { coordinate: c, demand_id: id, answered: BTreeSet::new(), event: None, collected: false },
        );
        Ok(())
    }
    pub fn answer_v1(
        &mut self,
        now: u64,
        binding: &PalwStepBindingV2,
        answer: &PalwSelectedEventAnswerV1,
        maximum_leaves: u64,
    ) -> Result<(), String> {
        if now < self.reserved_daa || now >= self.respond_until_daa || !self.custodians.contains(&answer.answerer) {
            return Err("selected answer has no literal custody or original response clock".into());
        }
        let request = self.requests.get(&answer.coordinate).ok_or("selected answer has no accepted exact-coordinate demand")?;
        if request.demand_id != answer.demand_id || request.collected || request.answered.contains(&answer.answerer) {
            return Err("selected answer substitutes demand or repeats a paid response".into());
        }
        answer.check_v1()?;
        if answer.binding.resolve_v1(binding.shape_profile.shape_profile_id(), &binding.shape_profile)? != *binding {
            return Err("selected answer substitutes original binding".into());
        }
        self.answer_event_view_v1(
            now,
            &crate::palw_decode_view_v1::PalwDecodeViewV1::of_verified_binding_v1(binding),
            answer.answerer,
            answer.demand_id,
            answer.coordinate,
            &answer.event,
            maximum_leaves,
        )
    }
    /// The answer core over a decode view: the native fold supplies the view (for a TIR claim,
    /// from its admitted header and registered record), never the answer carrier.
    #[allow(clippy::too_many_arguments)]
    pub fn answer_event_view_v1(
        &mut self,
        now: u64,
        view: &crate::palw_decode_view_v1::PalwDecodeViewV1,
        answerer: PalwBondKeyV2,
        demand_id: Hash64,
        coordinate: PalwSelectedEventCoordinateV1,
        event: &PalwSelectedTiledEventV1,
        maximum_leaves: u64,
    ) -> Result<(), String> {
        if now < self.reserved_daa || now >= self.respond_until_daa || !self.custodians.contains(&answerer) {
            return Err("selected answer has no literal custody or original response clock".into());
        }
        let request = self.requests.get(&coordinate).ok_or("selected answer has no accepted exact-coordinate demand")?;
        if request.demand_id != demand_id || request.collected || request.answered.contains(&answerer) {
            return Err("selected answer substitutes demand or repeats a paid response".into());
        }
        event.authenticate_view_v1(view, coordinate, maximum_leaves)?;
        if request.event.as_ref().is_some_and(|old| old != event) {
            return Err("selected answer conflicts with the authenticated original source".into());
        }
        let request = self.requests.get_mut(&coordinate).expect("checked");
        request.answered.insert(answerer);
        if request.event.is_none() {
            request.event = Some(event.clone());
        }
        Ok(())
    }
    /// Returned identities are obligations to collect, not a local assertion of guilt.
    /// The caller must apply the native sanction/collection fold atomically.
    pub fn collect_v1(
        &mut self,
        now: u64,
        coordinate: PalwSelectedEventCoordinateV1,
        demand_id: Hash64,
    ) -> Result<BTreeSet<PalwBondKeyV2>, String> {
        if now < self.respond_until_daa || now >= self.collect_until_daa {
            return Err("selected collection is outside original paid clock".into());
        }
        let request = self.requests.get_mut(&coordinate).ok_or("selected collection has no accepted demand")?;
        if request.collected || request.demand_id != demand_id {
            return Err("selected collection changes or repeats demand".into());
        }
        request.collected = true;
        Ok(self.custodians.difference(&request.answered).copied().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_response_span_rejects_last_moment_demands_without_extending_clock() {
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let session = PalwSelectedEventSessionV1::new_with_span_v1(
            Hash64::from_u64_word(1),
            key(9),
            5,
            Hash64::from_u64_word(2),
            30,
            40,
            10,
            BTreeSet::from([key(3)]),
        )
        .unwrap();
        assert!(session.can_demand_at_v1(20));
        assert!(!session.can_demand_at_v1(21));
        assert!(!session.can_demand_at_v1(29));
        assert!(!session.can_demand_at_v1(4));
        assert_eq!((session.respond_until_daa, session.collect_until_daa), (30, 40));
        assert!(
            PalwSelectedEventSessionV1::new_with_span_v1(
                Hash64::from_u64_word(1),
                key(9),
                5,
                Hash64::from_u64_word(2),
                30,
                40,
                26,
                BTreeSet::from([key(3)]),
            )
            .is_err()
        );
    }
    /// **G14, TIR lane, from public material only.** A TIR claim has no legacy binding: the fold
    /// derives the decode view from the claim's admitted TIR public header and its registered class
    /// record. The producer and its entire held Panel collude. Either they withhold — both are
    /// returned to the native sanction fold at the original clock — or one answers, and a forged
    /// tile never authenticates against the view while the committed one exposes that the
    /// admitted `[` outranks the committed `[7`: the Constraint court seeded from the
    /// authenticated event alone sees the beat. No step reads a capture, a vote or a local run.
    #[test]
    fn g14_tir_outside_verifier_convicts_from_the_native_view_under_full_collusion() {
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let case = crate::palw_decode_constraint_v1::tests::public_tir_constrained_case_for_tests_v1();
        let ladder = 1u64 << 20;
        let view = crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(&case.header, &case.record, ladder).unwrap();
        assert!(view.is_tiled_v1());
        let (producer, panel, owner) = (key(1), key(2), key(9));
        let custodians = BTreeSet::from([producer, panel]);
        let open = || {
            PalwSelectedEventSessionV1::new_with_span_v1(
                Hash64::from_u64_word(7),
                owner,
                10,
                Hash64::from_u64_word(8),
                100,
                200,
                10,
                custodians.clone(),
            )
            .unwrap()
        };
        let coordinate = PalwSelectedEventCoordinateV1 { row: case.position, tile: 0 };
        // (1) Total withholding: both custodians are collected at the original clock.
        let mut withheld = open();
        withheld.demand_view_v1(11, &view, &PalwSelectedEventDemandV1 { coordinate }).unwrap();
        let demand_id = withheld.requests[&coordinate].demand_id;
        assert!(withheld.collect_v1(99, coordinate, demand_id).is_err());
        assert_eq!(withheld.collect_v1(100, coordinate, demand_id).unwrap(), custodians);
        // (2) The colluders answer: a forged tile is refused, the committed one authenticates.
        let mut session = open();
        session.demand_view_v1(11, &view, &PalwSelectedEventDemandV1 { coordinate }).unwrap();
        let demand_id = session.requests[&coordinate].demand_id;
        let event = PalwSelectedTiledEventV1::of_public_rows_view_v1(&view, &case.rows, &case.ids, coordinate, ladder).unwrap();
        let mut forged = event.clone();
        forged.tile_lanes[case.ids[0] as usize] += 1_000_000;
        assert!(session.answer_event_view_v1(12, &view, panel, demand_id, coordinate, &forged, ladder).is_err());
        session.answer_event_view_v1(12, &view, producer, demand_id, coordinate, &event, ladder).unwrap();
        let authenticated = session.requests[&coordinate].event.clone().unwrap();
        // The seed's pin is built from the authenticated event only (both lanes share tile 0).
        let pin = crate::palw_step_refute::PalwTiledDecodePinV1 {
            position: case.position,
            generated_token_ids: authenticated.generated_token_ids.clone(),
            row_root: authenticated.row_root,
            row_opening: authenticated.row_opening.clone(),
            committed_tile_lanes: authenticated.tile_lanes.clone(),
            committed_opening: authenticated.tile_opening.clone(),
            beat_tile_lanes: authenticated.tile_lanes.clone(),
            beat_opening: authenticated.tile_opening.clone(),
            beat_lane: case.beat,
        };
        let identity = borsh::to_vec(&case.header).unwrap();
        let court = crate::palw_constraint_stream_court_v1::PalwConstraintStreamCourtV1::new_from_tiled_view_v1(
            Hash64::from_u64_word(7),
            owner,
            10,
            view.committed_execution_root,
            &view,
            &identity,
            &case.job,
            &pin,
            None,
            case.eog.ids.len() as u32,
            &case.eog.opening,
            ladder,
            None,
        )
        .unwrap();
        assert_eq!(court.selection_for_tests_v1(), (case.position, Some(case.beat), true));
        // A header with a substituted trace root, or another class's record, derives no view at all.
        let mut foreign = case.header.clone();
        foreign.full_logits_trace_root = Hash64::from_u64_word(1);
        assert!(crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(&foreign, &case.record, ladder).is_err());
        let other = crate::palw_tir_admission_v1::PalwTirClassRecordV1::test_row_v1(Hash64::from_u64_word(2));
        assert!(crate::palw_decode_view_v1::PalwDecodeViewV1::from_tir_header_v1(&case.header, &other, ladder).is_err());
    }
    #[test]
    fn selected_source_decoder_rejects_unpaid_vector_before_allocation() {
        assert!(borsh::from_slice::<PalwSelectedTiledEventV1>(&u32::MAX.to_le_bytes()).is_err());
        let config = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1();
        let small = selected_event_resource_plan_v1(4_194_303, 4096, 1_000_000, 5, config).unwrap();
        let full = selected_event_resource_plan_v1(4_194_303, 4096, 1_000_000, 65_536, config).unwrap();
        assert!(full.maximum_moves > small.maximum_moves);
        assert!(full.row_bytes > small.row_bytes);
        assert!(selected_event_resource_plan_v1(4_194_304, 4096, 1, 5, config).is_err());
        assert!(selected_event_resource_plan_v1(3, 4097, 1, 5, config).is_err());
        assert!(selected_event_resource_plan_v1(3, 1, u64::MAX, 5, config).is_err());
    }
    #[test]
    fn full_width_coordinates_are_literal_and_cannot_alias_legacy_event() {
        let wide = PalwSelectedEventCoordinateV1 { row: 17, tile: 1023 };
        let roundtrip: PalwSelectedEventCoordinateV1 = borsh::from_slice(&borsh::to_vec(&wide).unwrap()).unwrap();
        assert_eq!(wide, roundtrip);
        assert_ne!(wide, PalwSelectedEventCoordinateV1 { row: 17, tile: 255 });
    }
    #[test]
    fn selected_collection_keeps_original_deadline_and_literal_signed_custody() {
        let key = |n| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0));
        let custodians = BTreeSet::from([key(2), key(3)]);
        let mut session = PalwSelectedEventSessionV1::new_v1(
            Hash64::from_u64_word(1),
            key(9),
            5,
            Hash64::from_u64_word(8),
            10,
            20,
            custodians.clone(),
        )
        .unwrap();
        let c = PalwSelectedEventCoordinateV1 { row: 1, tile: 1023 };
        let demand_id = Hash64::from_u64_word(4);
        session.requests.insert(
            c,
            PalwSelectedEventRequestV1 { coordinate: c, demand_id, answered: BTreeSet::from([key(2)]), event: None, collected: false },
        );
        assert!(session.collect_v1(9, c, demand_id).is_err());
        assert!(session.collect_v1(10, c, Hash64::from_u64_word(6)).is_err());
        assert_eq!(session.collect_v1(10, c, demand_id).unwrap(), BTreeSet::from([key(3)]));
        assert!(session.collect_v1(11, c, demand_id).is_err());
        assert!(
            PalwSelectedEventSessionV1::new_v1(Hash64::from_u64_word(1), key(2), 5, Hash64::from_u64_word(8), 10, 20, custodians)
                .is_err()
        );
    }
}
