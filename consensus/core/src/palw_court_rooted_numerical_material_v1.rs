//! Retained native numerical operands from committed material rows only.
use crate::palw_artifact::PalwArtifactOpeningV1 as Parameter;
use crate::palw_court_shared_material_intake_v1::MaterialTargetV1 as Target;
use crate::palw_native_resource_v1::{
    NativeDecodeErrorV1 as E, NativeRetainedV1 as Held, TirNativeRequestsV1, reserve_native_requests_v1,
};
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1 as PromptTile;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_public_source_v1::{
    PalwTirPublicAuthenticatedIdsV1 as Ids, PalwTirPublicAuthenticatedUnitV1 as Unit, PalwTirPublicRequestV1 as Q,
};
use crate::{Hash64, palw_state_v2::PalwChainStateV2};
fn bad(s: impl Into<String>) -> E<String> {
    E::Decode(s.into())
}
fn decode<T: crate::palw_native_resource_v1::NativeWireScanV1 + borsh::BorshDeserialize>(bytes: &[u8]) -> Result<Held<T>, E<String>> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(bytes, || borsh::from_slice::<T>(bytes)).map_err(|e| {
        match e {
            E::Resource(r) => E::Resource(r),
            E::Decode(e) => bad(e.to_string()),
        }
    })
}
pub struct RootedNumericalMaterialV1 {
    units: Vec<Held<Unit>>,
    parameters: Vec<Held<Parameter>>,
    ids: Option<Held<Ids>>,
    flat: Option<Held<Vec<u32>>>,
    tiles: Vec<PromptTile>,
    tile_leases: Vec<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>>,
}
impl RootedNumericalMaterialV1 {
    pub fn retain_v1(state: &PalwChainStateV2, claim: Hash64) -> Result<Held<Self>, E<String>> {
        let c = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(bad)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim).ok_or_else(|| bad("original missing"))?;
        let record = state.tir_class_v1(&frame.original_v1().job.class_id).ok_or_else(|| bad("class missing"))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, frame.original_v1().job.class_id)
            .ok_or_else(|| bad("layout missing"))?;
        let source = frame.source_identity_native_v2(record, layout, frame.descriptor_v1().maximum_leaves)?;
        if source.source_id_v1() != c.source_id {
            return Err(bad("material source differs from current original"));
        }
        let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, c.profile_id).map_err(bad)?;
        let lo = Hash64::from_bytes([0; 64]);
        let hi = Hash64::from_bytes([255; 64]);
        let lo_key = Key::CourtMaterial(claim, lo);
        let hi_key = Key::CourtMaterial(claim, hi);
        let rows = || state.staged_court_range_v1(&lo_key, &hi_key);
        let count = rows().count() as u64;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Self>(1).map_err(E::Resource)?;
        q.exact_v1::<Held<Unit>>(count.checked_add(set.len()).ok_or_else(|| bad("unit count overflow"))?).map_err(E::Resource)?;
        q.exact_v1::<Held<Parameter>>(count).map_err(E::Resource)?;
        q.exact_v1::<Held<PromptTile>>(count).map_err(E::Resource)?;
        q.exact_v1::<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>>(count).map_err(E::Resource)?;
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::Resource)?;
        let ids = match state.staged_court_row_v1(&Key::CourtSharedIds(claim)) {
            Some(Row::CourtSharedIds(r)) => {
                if r.scope_id != c.scope_id {
                    return Err(bad("material IDs scope"));
                }
                Some(source.retain_complete_ids_classified_v2(r.rows_root, &r.ids)?)
            }
            _ => None,
        };
        let anchor = ids
            .as_ref()
            .map(|ids| crate::palw_tir_public_source_v1::PalwTirPublicRowsAnchorV1::from_native_v1(&source, ids.rows_root_v1()));
        let unit = |query: Q, bytes: &[u8]| -> Result<Held<Unit>, E<String>> {
            let response = source.decode_response_native_v2(&query, bytes)?;
            source.authenticate_response_native_v2(&query, &response, ids.as_deref(), anchor.as_ref())
        };
        let mut out = Self {
            units: Vec::with_capacity(count as usize + set.len() as usize),
            parameters: Vec::with_capacity(count as usize),
            ids: None,
            flat: None,
            tiles: Vec::with_capacity(count as usize),
            tile_leases: Vec::with_capacity(count as usize),
        };
        for (key, row) in rows() {
            let Row::CourtMaterial(row) = row else { return Err(bad("material key holds foreign row")) };
            row.check(key).map_err(bad)?;
            match row.target {
                Target::Parameter { leaf } => {
                    let opening = decode::<Parameter>(&row.bytes)?;
                    c.check_parameter_opening(state, c.scope_id, &opening).map_err(bad)?;
                    if opening.leaf_index != leaf {
                        return Err(bad("parameter coordinate"));
                    }
                    out.parameters.push(opening);
                }
                Target::PromptFlat => {
                    if out.flat.is_some() {
                        return Err(bad("duplicate flat prompt"));
                    }
                    out.flat = Some(decode(&row.bytes)?);
                }
                Target::PromptTile { tile } => {
                    let opening = decode::<PromptTile>(&row.bytes)?;
                    if opening.tile_index != tile {
                        return Err(bad("prompt tile coordinate"));
                    }
                    let (value, ticket) = opening.into_parts_v1();
                    out.tiles.push(value);
                    out.tile_leases.push(ticket);
                }
                Target::Unit { ordinal } => out.units.push(unit(set.request_at(ordinal).map_err(bad)?, &row.bytes)?),
                Target::FullIds => return Err(bad("FullIds stored in wrong namespace")),
            }
        }
        for ordinal in 0..set.len() {
            let query = set.request_at(ordinal).map_err(bad)?;
            if matches!(query, Q::Ids { .. }) || out.units.iter().any(|u| u.request_v1() == query) {
                continue;
            }
            if let Some(Row::CourtSharedQuery(r)) =
                state.staged_court_row_v1(&Key::CourtSharedQuery(set.request_id(ordinal).map_err(bad)?))
            {
                r.check_context(state).map_err(bad)?;
                if let Some(body) = &r.body {
                    out.units.push(unit(query, &body.bytes)?);
                }
            }
        }
        out.ids = ids;
        out.parameters.sort_by_key(|p| p.leaf_index);
        out.tiles.sort_by_key(|p| p.tile_index);
        // Construction of Store rechecks immutable prompt form, count and all
        // parameter/source identities. No caller supply can fill a missing row.
        Ok(Held::from_owner_reserved_v1(out, lease))
    }
    pub(crate) fn store_v1<'a>(
        &'a self,
        state: &PalwChainStateV2,
        claim: Hash64,
        scope: Hash64,
    ) -> Result<crate::palw_court_shared_tir_builder_store_v1::SharedTirBuilderStoreV1<'a>, String> {
        use crate::palw_court_shared_tir_builder_store_v1::{PromptMaterialV1 as P, SharedTirBuilderStoreV1 as Store};
        let prompt = match (&self.flat, self.tiles.is_empty()) {
            (Some(p), true) => P::Flat(p),
            (None, false) => P::Merkle(&self.tiles),
            (None, true) => P::Missing,
            _ => return Err("rooted material has two prompt forms".into()),
        };
        Store::new_with_retained_material_v1(state, claim, scope, &self.units, &self.parameters, self.ids.as_deref(), prompt)
    }
}

/// Run one read-only evidence construction against authenticated material owned
/// by this exact rooted state. The retained decoder and source leases live
/// through the callback; missing material never falls back to producer data.
pub fn with_rooted_numerical_material_v1<T>(
    state: &PalwChainStateV2,
    claim: Hash64,
    build: impl FnOnce(&dyn crate::palw_tir_court_v1::PalwTirEvidenceStoreV1) -> T,
) -> Result<T, E<String>> {
    let material = RootedNumericalMaterialV1::retain_v1(state, claim)?;
    let scope = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(bad)?.scope_id;
    let store = material.store_v1(state, claim, scope).map_err(bad)?;
    Ok(build(&store))
}
