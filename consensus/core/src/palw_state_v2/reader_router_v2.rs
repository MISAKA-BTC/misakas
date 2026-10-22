//! Prospective routing V2 identity and its immutable native auxiliary journal.
//! No row authenticates arithmetic or grants claim credit.
use super::*;
use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, retain_native_value_decode_classified_v1};
use crate::palw_reader_router_v2::*;

fn refused(s: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::ImmutableModelUpdate(s)
}
fn routing_error(e: PalwReaderRouterErrorV2) -> PalwStateV2Error {
    match e {
        PalwReaderRouterErrorV2::Identity(s) => refused(s),
        PalwReaderRouterErrorV2::Resource(e) => PalwStateV2Error::NativeResource(e),
    }
}
fn decode_error(e: NativeDecodeErrorV1<std::io::Error>) -> PalwStateV2Error {
    match e {
        NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        NativeDecodeErrorV1::Decode(_) => refused("routing native row is malformed"),
    }
}
fn key(prefix: u8, id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = prefix;
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
fn raw<'a>(state: &'a PalwChainStateV2, key: &[u8]) -> Option<&'a [u8]> {
    // Borrowed lookup avoids a temporary Vec allocation. Cost remains O(aux)
    // pending the complete physical inventory and 100k acceptance phase.
    state
        .kernel_route()?
        .aux
        .iter()
        .find_map(|((table, k), bytes)| (*table == PALW_READER_ROUTER_TABLE_V2 && k.as_slice() == key).then_some(bytes.as_slice()))
}
fn first(state: &PalwChainStateV2, class: Hash64) -> Result<Option<u64>, PalwStateV2Error> {
    raw(state, &key(0xA1, class))
        .map(|b| borsh::from_slice(b).map_err(|_| refused("routing first-claim marker is malformed")))
        .transpose()
}

impl PalwChainStateV2 {
    /// Only a row from this already-authenticated native state may choose a
    /// reader's router. The retained decode owns its full bindings allocation.
    pub fn reader_router_declaration_v2(
        &self,
        class: Hash64,
    ) -> Result<Option<NativeRetainedV1<PalwReaderRouterRowV2>>, PalwStateV2Error> {
        let row: Option<NativeRetainedV1<PalwReaderRouterRowV2>> = raw(self, class.as_byte_slice())
            .map(|bytes| retain_native_value_decode_classified_v1(bytes, || borsh::from_slice(bytes)).map_err(decode_error))
            .transpose()?;
        if let Some(row) = &row {
            let owner = self.class(&class).ok_or_else(|| refused("routing declaration has no class"))?;
            if row.declaration.class_id != class
                || row.declaration.artifact_root != owner.artifact_root
                || Some(row.registrant) != owner.registrant_bond
                || row.bound_from_daa < owner.registered_daa
            {
                return Err(refused("routing declaration differs from its native class identity"));
            }
        }
        Ok(row)
    }

    pub fn has_reader_router_claim_stamp_v2(&self, claim_id: Hash64) -> bool {
        raw(self, &key(0xA2, claim_id)).is_some()
    }

    /// Guard old implicit-selector consumers without allocating/decoding the
    /// declaration in the signature-only outer walk. Full identity validation
    /// remains the typed V2 consumer's. A present malformed stamp is refused.
    pub fn require_implicit_reader_slot_v1(&self, claim_id: Hash64, slot: u32) -> Result<(), PalwStateV2Error> {
        let Some(bytes) = raw(self, &key(0xA2, claim_id)) else { return Ok(()) };
        let class_id: Hash64 = borsh::from_slice(bytes).map_err(|_| refused("routing claim stamp is malformed"))?;
        let claim = self.claim(&claim_id).ok_or_else(|| refused("routing claim stamp is orphaned"))?;
        if claim.class_id != class_id {
            return Err(refused("routing stamp names another class"));
        }
        let profile = self.fp_work_profile_of(&class_id).ok_or_else(|| refused("routing stamp lacks native class graph"))?;
        let (node, _) = profile.resolve_node_slot(slot).ok_or_else(|| refused("V2 consumer reader slot is unresolved"))?;
        if needs_reader_router_v2(node) {
            return Err(refused("V2-stamped routed reader cannot use the V1 implicit router consumer"));
        }
        Ok(())
    }

    /// Explicit per-claim consumer stamp avoids retroactive interpretation,
    /// including old and new claims accepted at the same DAA score. Missing
    /// stamps keep V1 semantics; an invalid present stamp is never a fallback.
    pub fn reader_router_of_claim_v2(
        &self,
        claim_id: Hash64,
    ) -> Result<Option<NativeRetainedV1<PalwReaderRouterRowV2>>, PalwStateV2Error> {
        let Some(bytes) = raw(self, &key(0xA2, claim_id)) else { return Ok(None) };
        let class_id: Hash64 = borsh::from_slice(bytes).map_err(|_| refused("routing claim stamp is malformed"))?;
        let claim = self.claim(&claim_id).ok_or_else(|| refused("routing claim stamp is orphaned"))?;
        if claim.class_id != class_id {
            return Err(refused("routing claim stamp names another class"));
        }
        let row = self.reader_router_declaration_v2(class_id)?.ok_or_else(|| refused("routing claim stamp lacks declaration"))?;
        row.declaration
            .check_profile(self.fp_work_profile_of(&class_id).ok_or_else(|| refused("V2 routed claim lacks native graph"))?)
            .map_err(routing_error)?;
        if claim.accepted_daa < row.bound_from_daa || first(self, class_id)?.is_none_or(|at| at > claim.accepted_daa) {
            return Err(refused("routing claim stamp predates its prospective declaration"));
        }
        Ok(Some(row))
    }

    pub(crate) fn assert_reader_router_consistency_v2(&self) -> Result<(), PalwStateV2Error> {
        let Some(route) = self.kernel_route() else { return Ok(()) };
        let bad = |s: &str| PalwStateV2Error::CarriageInconsistent(format!("reader/router V2: {s}"));
        let now = self.last_point.as_ref().map(|p| p.daa_score);
        for ((table, k), bytes) in &route.aux {
            if *table != PALW_READER_ROUTER_TABLE_V2 {
                continue;
            }
            if k.len() == 64 {
                let row = self
                    .reader_router_declaration_v2(Hash64::from_bytes(k.as_slice().try_into().expect("64 bytes")))?
                    .ok_or_else(|| bad("declaration vanished"))?;
                if row.declaration.class_id.as_byte_slice() != k.as_slice() {
                    return Err(bad("declaration key mismatch"));
                }
                let class = self.class(&row.declaration.class_id).ok_or_else(|| bad("declaration has no class"))?;
                if class.artifact_root != row.declaration.artifact_root
                    || class.registrant_bond != Some(row.registrant)
                    || row.bound_from_daa < class.registered_daa
                    || now.is_some_and(|n| row.bound_from_daa > n)
                {
                    return Err(bad("declaration owner/artifact/clock mismatch"));
                }
                let profile =
                    self.fp_work_profile_of(&row.declaration.class_id).ok_or_else(|| bad("declaration lacks native profile"))?;
                row.declaration.check_profile(profile).map_err(routing_error)?;
            } else if k.len() == 65 && k[0] == 0xA1 {
                let class = Hash64::from_bytes(k[1..].try_into().expect("64 bytes"));
                let at: u64 = borsh::from_slice(bytes).map_err(|_| bad("first marker malformed"))?;
                let row = self.reader_router_declaration_v2(class)?.ok_or_else(|| bad("first marker lacks declaration"))?;
                if at < row.bound_from_daa || now.is_some_and(|n| at > n) {
                    return Err(bad("first marker clock mismatch"));
                }
            } else if k.len() == 65 && k[0] == 0xA2 {
                let claim = Hash64::from_bytes(k[1..].try_into().expect("64 bytes"));
                self.reader_router_of_claim_v2(claim)?.ok_or_else(|| bad("claim stamp vanished"))?;
            } else {
                return Err(bad("unknown row family"));
            }
        }
        Ok(())
    }
}

pub(super) fn declare(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    declaration: &PalwReaderRouterDeclarationV2,
) -> Result<(), PalwStateV2Error> {
    let limits = builder.extras.memory_routing_court.ok_or(PalwStateV2Error::ShardCourtDormant)?;
    if borsh::object_length(declaration).map_err(|_| refused("routing declaration cannot be measured"))? as u64
        > limits.max_close_bytes
    {
        return Err(refused("routing declaration exceeds native court ceiling"));
    }
    let class = builder.state.class(&declaration.class_id).ok_or_else(|| refused("routing class is absent"))?;
    if class.artifact_root != declaration.artifact_root {
        return Err(refused("routing artifact differs from registered class"));
    }
    let registrant = class.registrant_bond.ok_or_else(|| refused("routing declaration needs its original signing registrant"))?;
    let bond = builder.state.bond(&registrant).ok_or_else(|| refused("routing registrant bond is absent"))?;
    if !matches!(bond.status, PalwBondStatusV2::Active) {
        return Err(refused("routing registrant is not Active"));
    }
    let profile = builder
        .state
        .fp_work_profile_of(&declaration.class_id)
        .ok_or_else(|| refused("routing declaration lacks registered profile"))?;
    declaration.check_profile(profile).map_err(routing_error)?;
    if let Some(previous) = builder.state.reader_router_declaration_v2(declaration.class_id)? {
        if previous.declaration != *declaration || previous.registrant != registrant {
            return Err(refused("reader/router declaration is immutable"));
        }
        return Ok(()); // Idempotent repeated publication never advances its clock.
    }
    if first(&builder.state, declaration.class_id)?.is_some() {
        return Err(refused("first V2 claim exists without its declaration"));
    }
    let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
    q.vec_v1::<PalwReaderRouterBindingV2>(declaration.bindings.len() as u64).map_err(PalwStateV2Error::NativeResource)?;
    let _copy = crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let row = PalwReaderRouterRowV2 { declaration: declaration.clone(), registrant, bound_from_daa: ctx.daa_score };
    builder.write_reader_router_cell_v2(declaration.class_id.as_byte_slice(), Some(&row))
}

impl TransitionBuilder<'_> {
    /// Reserve exact encoded inputs before allocation, then map/journal copies,
    /// membership and journal growth before publication. Same 0xEC delta path.
    pub(super) fn write_reader_router_cell_v2<T: borsh::BorshSerialize>(
        &mut self,
        k: &[u8],
        row: Option<&T>,
    ) -> Result<(), PalwStateV2Error> {
        let Some(route) = self.state.kernel_route() else {
            return if row.is_none() { Ok(()) } else { Err(refused("routing V2 journal requires native kernel route")) };
        };
        let absent = !route.aux.iter().any(|((t, key), _)| *t == PALW_READER_ROUTER_TABLE_V2 && key.as_slice() == k);
        if absent && row.is_none() {
            return Ok(());
        }
        let length = row.map(borsh::object_length).transpose().map_err(|_| refused("routing row cannot be measured"))?.unwrap_or(0);
        let n = (k.len() as u64).checked_add(length as u64).ok_or(PalwStateV2Error::Overflow("routing row input buffers"))?;
        self.native_reservation.grow_v1(n).map_err(PalwStateV2Error::NativeResource)?;
        let k = (PALW_READER_ROUTER_TABLE_V2, k.to_vec());
        let bytes = if let Some(row) = row {
            let mut encoded = Vec::with_capacity(length);
            borsh::to_writer(&mut encoded, row).map_err(|_| refused("routing row cannot be encoded"))?;
            if encoded.len() != length {
                return Err(refused("routing row encoding changed its measured length"));
            }
            Some(encoded)
        } else {
            None
        };
        self.reserve_map_journal_v1(&k, &bytes, absent, "reader/router V2 cell writer")?;
        self.write_kernel_row(k.0, k.1, bytes);
        Ok(())
    }

    pub(super) fn stamp_reader_router_claim_v2(&mut self, claim_id: Hash64, claim: &PalwClaimStateV2) -> Result<(), PalwStateV2Error> {
        if self.extras.memory_routing_court.is_none() {
            return Ok(());
        }
        // These classes carry an independently admitted typed TIR/GenTensor
        // program, rather than a legacy PalwShapeProfileV3 reader graph. Their
        // canonical inventory remains their own consumer's. Derive this scope
        // from the native class record, never from a missing profile or a
        // caller-provided family flag. An unresolved legacy graph still fails
        // below rather than silently bypassing the prospective routing gate.
        if self.state.tir_classes.contains_key(&claim.class_id) || self.state.gen_classes.contains_key(&claim.class_id) {
            return Ok(());
        }
        let profile =
            self.state.fp_work_profile_of(&claim.class_id).ok_or_else(|| refused("V2 claim needs its native canonical profile"))?;
        if !(0..profile.global_node_count()).filter_map(|slot| profile.resolve_node_slot(slot)).any(|(n, _)| needs_reader_router_v2(n))
        {
            return Ok(());
        }
        let row = self
            .state
            .reader_router_declaration_v2(claim.class_id)?
            .ok_or_else(|| refused("V2 routed claim lacks immutable router declaration"))?;
        if row.bound_from_daa > claim.accepted_daa {
            return Err(refused("V2 claim predates its router declaration"));
        }
        row.declaration.check_profile(profile).map_err(routing_error)?;
        match first(&self.state, claim.class_id)? {
            None => self.write_reader_router_cell_v2(&key(0xA1, claim.class_id), Some(&claim.accepted_daa))?,
            Some(at) if at > claim.accepted_daa => return Err(refused("V2 routing first-claim clock moved backward")),
            Some(_) => {}
        }
        self.write_reader_router_cell_v2(&key(0xA2, claim_id), Some(&claim.class_id))
    }
    pub(super) fn remove_reader_router_claim_v2(&mut self, claim_id: Hash64) -> Result<(), PalwStateV2Error> {
        self.write_reader_router_cell_v2::<Hash64>(&key(0xA2, claim_id), None)
    }
    pub(super) fn remove_reader_router_class_v2(&mut self, class_id: Hash64) -> Result<(), PalwStateV2Error> {
        // Preserve flags-off legacy class removal exactly when no V2 cell exists.
        if raw(&self.state, class_id.as_byte_slice()).is_none() && first(&self.state, class_id)?.is_none() {
            return Ok(());
        }
        // Retain the first-use marker through ordinary claim GC and restart.
        if self.state.claims.values().any(|c| c.class_id == class_id) {
            return Err(refused("cannot remove routing class with live claims"));
        }
        self.write_reader_router_cell_v2::<PalwReaderRouterRowV2>(class_id.as_byte_slice(), None)?;
        self.write_reader_router_cell_v2::<u64>(&key(0xA1, class_id), None)
    }
}
