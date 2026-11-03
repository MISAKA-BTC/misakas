//! Independently funded draft reservations. The signed quote prices one actual
//! draft; target count never bounds how many uploaders may pay for drafts.
use super::*;
use crate::palw_court_family_material_v1::{FamilyDraftQuoteV2, FamilyDraftReservationV2};
use crate::palw_court_service_v1::{ServiceCost, ServicePacket};
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1, NativeRetainedV1};
fn add(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_add(b).ok_or_else(|| refused("draft reservation sum overflow"))
}
fn mul(a: u64, b: u64) -> Result<u64, PalwStateV2Error> {
    a.checked_mul(b).ok_or_else(|| refused("draft reservation product overflow"))
}
fn hash<T: BorshSerialize>(domain: &[u8], v: &T) -> Result<Hash64, PalwStateV2Error> {
    crate::palw_court_shared_tir_stream_v1::stream_digest(domain, v).map_err(refused)
}
const ROW: u8 = 5;
/// This profile changes only the explicit new upload contract. Existing V1
/// cohorts remain readable and their Begin fee/domain/IDs retain their meaning.
pub(super) fn requires_reserved_begin_v2(
    contract: &crate::palw_court_family_supplier_v1::FamilySupplierRowV1,
) -> Result<bool, PalwStateV2Error> {
    Ok(crate::palw_court_family_supplier_v1::signed_profile_generation_v3(&contract.consent.terms).map_err(refused)? >= 2)
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Geometry {
    pub memory: u64,
    pub journal: u64,
    pub retained: u64,
    pub packets: u64,
    pub capital: u64,
}
pub(super) fn geometry(target: m::FamilyMaterialTargetV1, total: u64) -> Result<Geometry, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    if total == 0 {
        return Err(refused("empty draft reservation"));
    }
    let t = borsh::object_length(&target).map_err(refused)? as u64;
    let parts = total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64);
    let node = std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<(u8, Vec<u8>), Vec<u8>>>() as u64;
    let draft = 64 + t + 68 + 8 + 64 + 8 + 8 + 8 + 74;
    let part = 64 + 8 + 4 + 8 + 82;
    // Exact fixed maximum: both quote copies and the uploader's real signature.
    let quote = 2 + 3 * 8 + 2 * 64 + 10 * 8;
    let reservation = 64
        + 2
        + 64
        + 64
        + 5 * 8
        + quote
        + 2
        + 64
        + t
        + 68
        + 1
        + 8
        + 64
        + quote
        + 4
        + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64
        + 74;
    let rows = add(parts, 2)?;
    let retained = add(add(draft, reservation)?, add(total, add(mul(parts, part)?, mul(rows, node)?)?)?)?;
    let projection = super::super::palw_claim_local_material_projection_v2::projection_geometry_v2(
        rows,
        add(130, mul(parts, 73)?)?,
        1,
        draft - 74,
        0,
    )?;
    // Draft create/delete and each full cursor rewrite, all part creates and
    // deletes, immutable birth create/delete, plus each real journal member.
    let entries = add(4, mul(parts, 4)?)?;
    let journal = add(
        add(mul(add(parts, 1)?, mul(2, add(draft, 3)?)?)?, mul(2, add(add(total, mul(parts, part)?)?, add(reservation, 3)?)?)?)?,
        mul(entries, std::mem::size_of::<PalwDeltaEntryV2>() as u64)?,
    )?;
    let memory = add(add(retained, journal)?, projection.retained_heap)?;
    let holding = memory.div_ceil(g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes);
    let packets = add(add(parts, 2)?, holding)?;
    Ok(Geometry { memory, journal, retained, packets, capital: mul(packets, g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1)? })
}
fn begin_fields(s: &m::SignedFamilyMaterialV1) -> Result<(u64, Hash64, &FamilyDraftQuoteV2), PalwStateV2Error> {
    match &s.action {
        m::FamilyMaterialActionV1::BeginReserved { total_bytes, body_hash, quote } if s.version == 2 => {
            Ok((*total_bytes, *body_hash, quote))
        }
        _ => Err(refused("draft reservation requires the dedicated signed V2 Begin quote")),
    }
}
/// Quote against the concrete prefix state and DAA. The book digest and exact
/// amounts are signed: replay cannot make an uploader pay for a larger book.
pub(super) fn quote(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    contract: Hash64,
    target: m::FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    total: u64,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    let row = supplier::family_contract_for_material_v1(&b.state, contract)?
        .ok_or_else(|| refused("draft quote supplier contract missing"))?;
    if !requires_reserved_begin_v2(&row)? {
        return Err(refused("draft V2 quote is not the legacy upload contract"));
    }
    let inv = m::derive_contract_family_inventory_classified_v3(&b.state, &row).map_err(native)?;
    if total == 0
        || total > inv.target_bound_v1(target).map_err(refused)?
        || ctx.daa_score < row.accepted_daa
        || ctx.daa_score >= row.consent.terms.respond_until_daa
    {
        return Err(refused("draft quote exceeds native scope/interval"));
    }
    let _ = b.state.bond(&submitter).ok_or(PalwStateV2Error::MissingBond(submitter))?;
    let book = b.state.court_service_state_v1().ok_or_else(|| refused("draft quote service book absent"))?;
    if !book.claims.contains_key(&row.consent.terms.claim) {
        return Err(refused("draft quote original binding absent"));
    }
    let accept_until =
        row.consent.terms.respond_until_daa.checked_sub(1).ok_or_else(|| refused("draft quote original hard absent"))?;
    let (book_bytes, book_heap) = book_bounds(&b.state, book)?;
    let (prior_rows, prior_key_bytes, prior_events) = prior_envelope(&b.state, row.consent.terms.claim)?;
    quoted_geometry_for_contract_v3(
        &row.consent.terms,
        target,
        total,
        ctx.daa_score,
        accept_until,
        row.consent.terms.respond_until_daa,
        book_bytes,
        book_heap,
        book_scope(&b.state, book)?,
        prior_rows,
        prior_key_bytes,
        prior_events,
    )
}
#[cfg(test)]
// Keep canonical quote geometry fields explicit across released-law replay dispatch.
#[allow(clippy::too_many_arguments)]
fn quoted_geometry(
    target: m::FamilyMaterialTargetV1,
    total: u64,
    accept_from_daa: u64,
    accept_until_daa: u64,
    respond_until_daa: u64,
    book_bytes: u64,
    book_heap: u64,
    book: Hash64,
    prior_rows: u64,
    prior_key_bytes: u64,
    prior_events: u64,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    quoted_geometry_scope_v3(
        target,
        total,
        accept_from_daa,
        accept_until_daa,
        respond_until_daa,
        book_bytes,
        book_heap,
        book,
        prior_rows,
        prior_key_bytes,
        prior_events,
        false,
    )
}
#[cfg(test)]
// Keep canonical quote geometry fields explicit across released-law replay dispatch.
#[allow(clippy::too_many_arguments)]
fn quoted_geometry_scope_v3(
    target: m::FamilyMaterialTargetV1,
    total: u64,
    accept_from_daa: u64,
    accept_until_daa: u64,
    respond_until_daa: u64,
    book_bytes: u64,
    book_heap: u64,
    book: Hash64,
    prior_rows: u64,
    prior_key_bytes: u64,
    prior_events: u64,
    contribution: bool,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    quoted_geometry_scope_v4(
        target,
        total,
        accept_from_daa,
        accept_until_daa,
        respond_until_daa,
        book_bytes,
        book_heap,
        book,
        prior_rows,
        prior_key_bytes,
        prior_events,
        contribution,
        false,
    )
}
// Keep canonical quote geometry fields explicit across released-law replay dispatch.
#[allow(clippy::too_many_arguments)]
fn quoted_geometry_scope_v4(
    target: m::FamilyMaterialTargetV1,
    total: u64,
    accept_from_daa: u64,
    accept_until_daa: u64,
    respond_until_daa: u64,
    book_bytes: u64,
    book_heap: u64,
    book: Hash64,
    prior_rows: u64,
    prior_key_bytes: u64,
    prior_events: u64,
    contribution: bool,
    index_local: bool,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    let geo = geometry(target, total)?;
    let parts = total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64);
    let rows = add(parts, 2)?;
    let keys = add(130, mul(parts, 73)?)?;
    let index_growth = add(add(mul(rows, 77)?, keys)?, mul(rows, 64)?)?;
    let old_index = add(add(add(74, mul(prior_rows, 77)?)?, prior_key_bytes)?, mul(prior_rows, 64)?)?;
    let events = add(mul(parts, 2)?, 4)?; // Begin/Finish/receipt GC and each Part create/delete
    let external = mul(mul(index_growth, 2)?, prior_events)?;
    let own = mul(mul(add(old_index, index_growth)?, 2)?, events)?;
    // Existing latest Q/M replacement is transition-local work. A new upload
    // owns its new canonical index contribution and its create/delete/reorg
    // copies, not a forecast copy of the whole prior index at every old event.
    let cross = if index_local { mul(add(74, index_growth)?, 2)? } else { add(external, own)? };
    let geo_memory = add(add(geo.memory, cross)?, mul(add(parts, 1)?, std::mem::size_of::<ServiceCost>() as u64)?)?;
    let transfers = add(total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64), 1)?;
    let entry = if contribution {
        book_packet_heap_v3()
    } else {
        (std::mem::size_of::<ServicePacket>()
            + 3 * std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<[u8; 64], u64>>()) as u64
    };
    // Whole existing-book clones/serialization are real per-transition local
    // workspace, NOT new durable draft holding or an old-row retention tariff.
    let workspace =
        if contribution { validator_book_workspace_v3(book_bytes, book_heap)? } else { add(mul(book_heap, 4)?, mul(book_bytes, 2)?)? };
    let old_book_holding = if contribution { 0 } else { workspace };
    let quantum = g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes;
    let denominator = quantum
        .checked_sub(mul(entry, 2)?)
        .filter(|v| *v > 0)
        .ok_or_else(|| refused("service quantum cannot cover its own reservation metadata"))?;
    // Closed-form conservative bound closes memory -> holding packets ->
    // larger book -> memory, without an invented cap or unbounded iteration.
    let base = add(add(geo_memory, old_book_holding)?, mul(mul(transfers, entry)?, 2)?)?;
    let upper_packets = add(transfers, base.div_ceil(denominator))?;
    let extension = mul(upper_packets, entry)?;
    let memory = add(add(geo_memory, old_book_holding)?, mul(extension, 2)?)?;
    let count = add(transfers, memory.div_ceil(quantum))?;
    if count > upper_packets {
        return Err(refused("draft holding closure did not bound its own book growth"));
    }
    let journal = add(add(geo.journal, cross)?, add(if contribution { 0 } else { mul(book_bytes, 2)? }, extension)?)?;
    let storage = add(add(geo.retained, journal)?, memory)?;
    let price = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
        .map_err(|_| refused("draft storage price absent"))?
        .sompi_per_byte;
    if price == 0 {
        return Err(refused("new independently reserved draft requires positive adopted storage price"));
    }
    Ok(FamilyDraftQuoteV2 {
        version: 2,
        accept_from_daa,
        accept_until_daa,
        respond_until_daa,
        profile: hash(b"misaka-palw/family-draft/profile/v2", &(supplier::family_supplier_profiles_v2()?, price))?,
        book,
        book_bytes,
        book_heap,
        fee_sompi: mul(storage, price)?,
        collateral_sompi: mul(count, g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1)?,
        reserved_memory: memory,
        retained_bytes: geo.retained,
        journal_bytes: journal,
        prior_rows,
        prior_key_bytes,
        prior_events,
    })
}
/// Keep old profile2 quote bytes/amounts exact, while binding new uploads
/// to their profile3 supplier purpose without changing V2 wire or draft domains.
#[allow(clippy::too_many_arguments)]
fn quoted_geometry_for_contract_v3(
    terms: &crate::palw_court_family_supplier_v1::FamilySupplierTermsV1,
    target: m::FamilyMaterialTargetV1,
    total: u64,
    accept_from_daa: u64,
    accept_until_daa: u64,
    respond_until_daa: u64,
    book_bytes: u64,
    book_heap: u64,
    book: Hash64,
    prior_rows: u64,
    prior_key_bytes: u64,
    prior_events: u64,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    quoted_geometry_for_law_v3(
        terms,
        target,
        total,
        accept_from_daa,
        accept_until_daa,
        respond_until_daa,
        book_bytes,
        book_heap,
        book,
        prior_rows,
        prior_key_bytes,
        prior_events,
        true,
        true,
    )
}
// Keep canonical quote geometry fields explicit across released-law replay dispatch.
#[allow(clippy::too_many_arguments)]
fn quoted_geometry_for_law_v3(
    terms: &crate::palw_court_family_supplier_v1::FamilySupplierTermsV1,
    target: m::FamilyMaterialTargetV1,
    total: u64,
    accept_from_daa: u64,
    accept_until_daa: u64,
    respond_until_daa: u64,
    book_bytes: u64,
    book_heap: u64,
    book: Hash64,
    prior_rows: u64,
    prior_key_bytes: u64,
    prior_events: u64,
    contribution: bool,
    index_local: bool,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    let generation = crate::palw_court_family_supplier_v1::signed_profile_generation_v3(terms).map_err(refused)?;
    let mut q = quoted_geometry_scope_v4(
        target,
        total,
        accept_from_daa,
        accept_until_daa,
        respond_until_daa,
        book_bytes,
        book_heap,
        book,
        prior_rows,
        prior_key_bytes,
        prior_events,
        contribution,
        index_local,
    )?;
    if generation == 3 {
        let price = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
            .map_err(|_| refused("draft storage price absent"))?
            .sompi_per_byte;
        q.profile =
            hash(b"misaka-palw/family-draft/generated-semantic-profile/v3", &(supplier::family_supplier_profiles_v3()?, price))?;
    } else if generation != 2 {
        return Err(refused("independent draft requires profile2 or profile3 contract"));
    }
    if contribution {
        q.profile = hash(b"misaka-palw/family-draft/contribution-geometry/v3", &(q.profile, DRAFT_CONTRIBUTION_GEOMETRY_V3))?;
    }
    if index_local {
        q.profile = hash(b"misaka-palw/family-draft/contribution-scope/v4", &(DRAFT_INDEX_CONTRIBUTION_GEOMETRY_V4, q.profile))?;
    }
    Ok(q)
}
const DRAFT_INDEX_CONTRIBUTION_GEOMETRY_V4: &[u8] = b"own-index-publication/local-latest-rewrite/v1";
const DRAFT_CONTRIBUTION_GEOMETRY_V3: &[u8] = b"own-draft-index-packets/local-book-workspace/v1";
fn validator_book_workspace_v3(wire: u64, heap: u64) -> Result<u64, PalwStateV2Error> {
    heap.checked_mul(4).and_then(|h| wire.checked_mul(2).and_then(|w| h.checked_add(w))).ok_or_else(|| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::overflow::<FamilyDraftReservationV2>(
            "independent draft local book workspace",
        ))
    })
}
fn book_packet_heap_v3() -> u64 {
    use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceSupply};
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1 as Node;
    (std::mem::size_of::<Node<u64, ServiceCost>>()
        + std::mem::size_of::<Node<[u8; 64], u64>>()
        + std::mem::size_of::<Node<[u8; 64], ServicePacket>>()
        + std::mem::size_of::<Node<u64, ServiceSupply>>()
        + std::mem::size_of::<Node<[u8; 64], ()>>()) as u64
}
fn quoted_geometry_for_signed_v3(
    terms: &crate::palw_court_family_supplier_v1::FamilySupplierTermsV1,
    target: m::FamilyMaterialTargetV1,
    total: u64,
    offered: &FamilyDraftQuoteV2,
    respond: u64,
    rows: u64,
    keys: u64,
    events: u64,
) -> Result<(FamilyDraftQuoteV2, bool), PalwStateV2Error> {
    let new = quoted_geometry_for_law_v3(
        terms,
        target,
        total,
        offered.accept_from_daa,
        offered.accept_until_daa,
        respond,
        offered.book_bytes,
        offered.book_heap,
        offered.book,
        rows,
        keys,
        events,
        true,
        true,
    )?;
    if new.profile == offered.profile {
        return Ok((new, true));
    }
    let prior = quoted_geometry_for_law_v3(
        terms,
        target,
        total,
        offered.accept_from_daa,
        offered.accept_until_daa,
        respond,
        offered.book_bytes,
        offered.book_heap,
        offered.book,
        rows,
        keys,
        events,
        true,
        false,
    )?;
    if prior.profile == offered.profile {
        return Ok((prior, true));
    }
    let old = quoted_geometry_for_law_v3(
        terms,
        target,
        total,
        offered.accept_from_daa,
        offered.accept_until_daa,
        respond,
        offered.book_bytes,
        offered.book_heap,
        offered.book,
        rows,
        keys,
        events,
        false,
        false,
    )?;
    Ok((old, false))
}
pub(super) fn install(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    s: &m::SignedFamilyMaterialV1,
    contract: &crate::palw_court_family_supplier_v1::FamilySupplierRowV1,
) -> Result<u64, PalwStateV2Error> {
    let (total, body_hash, offered) = begin_fields(s)?;
    if ctx.daa_score < offered.accept_from_daa
        || ctx.daa_score > offered.accept_until_daa
        || offered.accept_from_daa < contract.accepted_daa
        || offered.accept_until_daa >= contract.consent.terms.respond_until_daa
    {
        return Err(refused("draft outside signed quote acceptance interval"));
    }
    let book = b.state.court_service_state_v1().ok_or_else(|| refused("draft actual book missing"))?;
    let actual_wire = borsh::object_length(book).map_err(refused)? as u64;
    let heap = book.native_heap_v1("draft actual book");
    let actual_heap = add(heap.bytes().ok_or(PalwStateV2Error::NativeResource(heap))?, std::mem::size_of_val(book) as u64)?;
    let (stable_wire, stable_heap) = book_bounds(&b.state, book)?;
    if book_scope(&b.state, book)? != offered.book
        || stable_wire != offered.book_bytes
        || stable_heap != offered.book_heap
        || actual_wire > offered.book_bytes
        || actual_heap > offered.book_heap
    {
        return Err(refused("draft immutable universe changed or actual book exceeds signed finite bound; requote"));
    }
    let (rows, keys, events) = prior_envelope(&b.state, contract.consent.terms.claim)?;
    let (expected, contribution) = quoted_geometry_for_signed_v3(
        &contract.consent.terms,
        s.target,
        total,
        offered,
        contract.consent.terms.respond_until_daa,
        rows,
        keys,
        events,
    )?;
    if *offered != expected {
        return Err(refused("draft signed quote differs from native price/geometry/obligation envelope"));
    }
    let _local_workspace = if contribution {
        Some(
            crate::palw_native_resource_v1::reserve_native_requests_v1(validator_book_workspace_v3(stable_wire, stable_heap)?)
                .map_err(PalwStateV2Error::NativeResource)?,
        )
    } else {
        None
    };
    let _local_material_index = if contribution {
        Some(
            crate::palw_native_resource_v1::reserve_native_requests_v1(
                super::super::palw_claim_local_material_projection_v2::requests(&b.state)?,
            )
            .map_err(PalwStateV2Error::NativeResource)?,
        )
    } else {
        None
    };
    let id = m::family_material_draft_id_v2(s.contract, s.target, s.submitter, total, body_hash, offered).map_err(refused)?;
    let key = m::material_aux_key_v1(ROW, id, None);
    if read::<FamilyDraftReservationV2>(&b.state, &key)?.is_some() {
        return Err(refused("draft birth quote already consumed"));
    }
    let need = u128::from(offered.fee_sompi)
        .checked_add(u128::from(offered.collateral_sompi))
        .ok_or_else(|| refused("draft real principal sum"))?;
    if supplier::family_free_principal_v1(b, &s.submitter, ctx.daa_score)? < need {
        return Err(refused("draft quote exceeds free real principal"));
    }
    b.native_reservation.grow_v1(offered.reserved_memory).map_err(PalwStateV2Error::NativeResource)?;
    let schedule =
        crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1().map_err(|_| refused("draft TTL absent"))?;
    let inv = m::derive_contract_family_inventory_classified_v3(&b.state, contract).map_err(native)?;
    let native_span =
        crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1.max(inv.native_inclusion_margin_v2());
    let mut row = FamilyDraftReservationV2 {
        contract: s.contract,
        version: 2,
        claim: contract.consent.terms.claim,
        draft: id,
        accepted_daa: ctx.daa_score,
        expires_daa: add(ctx.daa_score, schedule.draft_ttl_daa)?.min(contract.consent.terms.respond_until_daa),
        material_until_daa: contract.consent.terms.material_until_daa,
        quote: offered.clone(),
        transfer_first_daa: add(ctx.daa_score, native_span)?,
        transfer_span_daa: native_span,
        begin: s.clone(),
    };
    let old = b.state.court_service_state_v1().ok_or_else(|| refused("draft service book absent"))?;
    let _lease = crate::palw_native_resource_v1::reserve_native_requests_v1(
        crate::palw_native_resource_v1::native_value_requests_v1(old).map_err(PalwStateV2Error::NativeResource)?,
    )
    .map_err(PalwStateV2Error::NativeResource)?;
    let transfers = add(total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64), 1)?;
    let duration = mul(transfers, row.transfer_span_daa)?;
    let last = row.expires_daa.checked_sub(duration).ok_or_else(|| refused("independent multipart chain cannot fit existing TTL"))?;
    let _calendar_lease = crate::palw_native_resource_v1::reserve_native_requests_v1(add(
        mul(crate::palw_native_resource_v1::native_value_requests_v1(old).map_err(PalwStateV2Error::NativeResource)?, 2)?,
        row.quote.reserved_memory,
    )?)
    .map_err(PalwStateV2Error::NativeResource)?;
    let ps = packets(&row)?;
    let _cost_lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(mul(transfers, std::mem::size_of::<ServiceCost>() as u64)?)
            .map_err(PalwStateV2Error::NativeResource)?;
    let costs = ps.iter().take(usize::try_from(transfers).map_err(refused)?).map(|p| p.cost).collect::<Vec<_>>();
    row.transfer_first_daa =
        supplier::calendar_search::first_fit_width_v2(&old.book, &costs, add(ctx.daa_score, native_span)?, last, native_span, 1)
            .map_err(refused)?
            .ok_or_else(|| refused("independent multipart causal chain cannot fit native book/TTL"))?;
    let packets = packets(&row)?;
    let mut next = old.clone();
    next.book
        .reserve(&packets, contract.consent.terms.respond_until_daa)
        .map_err(|e| refused(format!("independent selected calendar/holding: {e:?}")))?;
    next.claims
        .get_mut(&row.claim)
        .ok_or_else(|| refused("draft service claim disappeared"))?
        .packet_ids
        .extend(packets.iter().map(|p| p.id));
    put(b, &key, &row)?;
    b.write_court_service_state_v1(next)?;
    Ok(offered.fee_sompi)
}
pub(crate) fn packets(row: &FamilyDraftReservationV2) -> Result<NativeRetainedV1<Vec<ServicePacket>>, PalwStateV2Error> {
    use crate::palw_g14_values_v1 as g;
    let (total, _, _) = begin_fields(&row.begin)?;
    let geo = geometry(row.begin.target, total)?;
    let holding = row.quote.reserved_memory.div_ceil(g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes);
    let transfers = total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64) + 1;
    let count = add(transfers, holding)?;
    // Capital covers the exact effective holding nodes as well as transfers.
    if row.quote.collateral_sompi != mul(count, g::PALW_G14_CAPITAL_PER_SOURCE_NODE_SOMPI_V1)? {
        return Err(refused("draft quote capital differs from complete reservation"));
    }
    let _ = geo;
    let requests = mul(count, std::mem::size_of::<ServicePacket>() as u64)?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(requests).map_err(PalwStateV2Error::NativeResource)?;
    let mut out = Vec::new();
    out.try_reserve_exact(usize::try_from(count).map_err(refused)?).map_err(refused)?;
    // The signed contract's response horizon is carried by the birth's exact
    // draft expiry upper bound; install/restore sets the actual original hard.
    let hard = row.begin.reserved_response_horizon_v2().ok_or_else(|| refused("draft quote response horizon missing"))?;
    for ordinal in 0..count {
        let bytes = if ordinal < transfers {
            g::PALW_G14_SOURCE_PACKET_COST_V1
                .bytes
                .max(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64 + crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64 + 512)
        } else {
            let at = ordinal - transfers;
            (row.quote.reserved_memory - at * g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes)
                .min(g::PALW_G14_COURT_SERVICE_PROFILE_ESTIMATE_V1.budget.bytes)
        };
        out.push(ServicePacket {
            id: hash(b"misaka-palw/family-draft/service/v2", &(row.draft, &row.quote, ordinal))?.as_bytes(),
            release_daa: if ordinal < transfers {
                add(row.transfer_first_daa, mul(ordinal, row.transfer_span_daa)?)?
            } else {
                add(row.accepted_daa, 1)?
            },
            service_before: if ordinal < transfers {
                add(add(row.transfer_first_daa, mul(ordinal, row.transfer_span_daa)?)?, 1)?
            } else {
                hard
            },
            cost: ServiceCost {
                bytes,
                work: g::PALW_G14_SOURCE_PACKET_COST_V1.work,
                io: if ordinal < transfers { g::PALW_G14_SOURCE_PACKET_COST_V1.io } else { 0 },
                controls: 1,
            },
        });
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
pub(crate) fn rows(
    state: &PalwChainStateV2,
) -> Result<NativeRetainedV1<Vec<NativeRetainedV1<FamilyDraftReservationV2>>>, PalwStateV2Error> {
    let n = state
        .kernel_route_state_v1()
        .map_or(0, |r| r.aux.keys().filter(|(t, k)| *t == m::FAMILY_MATERIAL_TABLE_V1 && k.len() == 65 && k[0] == ROW).count());
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(mul(
        n as u64,
        std::mem::size_of::<NativeRetainedV1<FamilyDraftReservationV2>>() as u64,
    )?)
    .map_err(PalwStateV2Error::NativeResource)?;
    let mut out = Vec::with_capacity(n);
    if let Some(route) = state.kernel_route_state_v1() {
        for (t, k) in route.aux.keys() {
            if *t == m::FAMILY_MATERIAL_TABLE_V1 && k.len() == 65 && k[0] == ROW {
                let birth = read::<FamilyDraftReservationV2>(state, k)?.ok_or_else(|| refused("draft birth row disappeared"))?;
                if *k != m::material_aux_key_v1(ROW, birth.draft, None) {
                    return Err(refused("draft birth storage identity changed"));
                }
                out.push(birth);
            }
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
pub(crate) fn reserved_principal(state: &PalwChainStateV2, bond: &PalwBondKeyV2, now: u64) -> Result<u128, PalwStateV2Error> {
    let mut sum = 0u128;
    for row in rows(state)?.iter() {
        if row.begin.submitter == *bond && now <= row.material_until_daa {
            sum =
                sum.checked_add(u128::from(row.quote.collateral_sompi)).ok_or_else(|| refused("draft capital aggregate overflow"))?;
        }
    }
    Ok(sum)
}
pub(crate) fn validate(state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    for row in rows(state)?.iter() {
        let c = supplier::family_contract_for_material_v1(state, row.contract)?
            .ok_or_else(|| refused("draft birth lost supplier consent"))?;
        let (total, body_hash, q) = begin_fields(&row.begin)?;
        let id = m::family_material_draft_id_v2(row.contract, row.begin.target, row.begin.submitter, total, body_hash, q)
            .map_err(refused)?;
        if !requires_reserved_begin_v2(&c)?
            || body_hash == Hash64::default()
            || q.book == Hash64::default()
            || row.version != 2
            || row.draft != id
            || row.begin.contract != row.contract
            || row.claim != c.consent.terms.claim
            || (row.accepted_daa < q.accept_from_daa || row.accepted_daa > q.accept_until_daa)
            || row.quote != *q
            || q.accept_from_daa < c.accepted_daa
            || q.accept_from_daa > q.accept_until_daa
            || q.accept_until_daa >= c.consent.terms.respond_until_daa
            || row.accepted_daa < c.accepted_daa
            || row.accepted_daa >= c.consent.terms.respond_until_daa
            || row.material_until_daa != c.consent.terms.material_until_daa
        {
            return Err(refused("draft birth restored scope/amount/window"));
        }
        let inv = m::derive_contract_family_inventory_classified_v3(state, &c).map_err(native)?;
        if total > inv.target_bound_v1(row.begin.target).map_err(refused)? {
            return Err(refused("draft birth changed native byte bound"));
        }
        let bond = state.bond(&row.begin.submitter).ok_or(PalwStateV2Error::MissingBond(row.begin.submitter))?;
        row.begin.verify_v1(c.consent.terms.network, &bond.pubkey).map_err(refused)?;
        // Re-authenticate the stored fee/capital quote without requiring today's
        // free principal or Active status, or reusing today's expanded book hash.
        let (expected, _) = quoted_geometry_for_signed_v3(
            &c.consent.terms,
            row.begin.target,
            total,
            q,
            c.consent.terms.respond_until_daa,
            q.prior_rows,
            q.prior_key_bytes,
            q.prior_events,
        )?;
        let ttl = crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
            .map_err(|_| refused("draft restored TTL absent"))?
            .draft_ttl_daa;
        if *q != expected || row.expires_daa != add(row.accepted_daa, ttl)?.min(c.consent.terms.respond_until_daa) {
            return Err(refused("draft restored exact quote/fee/capital/expiry geometry"));
        }
        let controls = add(total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64), 1)?;
        if row.transfer_span_daa
            != crate::palw_g14_values_v1::PALW_G14_COURT_SERVICE_INCLUSION_DELAY_DAA_ESTIMATE_V1.max(inv.native_inclusion_margin_v2())
            || row.transfer_first_daa < add(row.accepted_daa, row.transfer_span_daa)?
            || add(row.transfer_first_daa, mul(controls, row.transfer_span_daa)?)? > row.expires_daa
        {
            return Err(refused("draft restored causal ordering/TTL/inclusion window"));
        }
        let service = state.court_service_state_v1().ok_or_else(|| refused("draft birth restored service absent"))?;
        let ps = packets(row)?;
        if let Some(binding) = service.claims.get(&row.claim) {
            for p in ps.iter() {
                if !binding.packet_ids.contains(&p.id) || service.book.packet(&p.id) != Some(p) {
                    return Err(refused("draft birth restored reservation changed"));
                }
            }
        }
    }
    Ok(())
}
/// Strictly add only the exact packet universe of actual signed birth rows.
/// No old packet, assignment, supply, scope, or resolved record is replaceable.
pub(crate) fn normalized_prior(
    state: &PalwChainStateV2,
    old: &crate::palw_court_service_state_v2::CourtServiceStateV2,
    next: &crate::palw_court_service_state_v2::CourtServiceStateV2,
) -> Result<NativeRetainedV1<crate::palw_court_service_state_v2::CourtServiceStateV2>, PalwStateV2Error> {
    let mut added = 0u64;
    for row in rows(state)?.iter() {
        for p in packets(row)?.iter() {
            if !old.book.packets.contains_key(&p.id) {
                added = add(added, 1)?;
            }
        }
    }
    let request = add(
        crate::palw_native_resource_v1::native_value_requests_v1(old).map_err(PalwStateV2Error::NativeResource)?,
        mul(added, std::mem::size_of::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<[u8; 64], ()>>() as u64)?,
    )?;
    let lease = crate::palw_native_resource_v1::reserve_native_requests_v1(request).map_err(PalwStateV2Error::NativeResource)?;
    let mut prior = old.clone();
    for row in rows(state)?.iter() {
        if let Some(binding) = prior.claims.get_mut(&row.claim) {
            for p in packets(row)?.iter() {
                if !old.book.packets.contains_key(&p.id) {
                    if next.book.packet(&p.id) != Some(p) {
                        return Err(refused("draft extension packet differs from signed native reservation"));
                    }
                    binding.packet_ids.insert(p.id);
                }
            }
        }
    }
    next.check_retains_unresolved(&prior).map_err(refused)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(prior, lease))
}
/// Native state owners reserve each birth's concrete finite envelope for the
/// full retained lifetime, including drafts with no parts and expired receipts.
/// Stack-only scalar extraction is a conservative predecode request; validate()
/// subsequently authenticates the signed quote, scope, and complete packet set.
pub(crate) fn retained_requests(route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1) -> NativeCoverageV1 {
    let mut total = 0u64;
    for ((t, k), raw) in &route.aux {
        if *t != m::FAMILY_MATERIAL_TABLE_V1 || k.len() != 65 || k[0] != ROW {
            continue;
        }
        // contract/version/claim/draft/three clocks then quote version, DAA,
        // two hashes, fee, collateral, reserved_memory.
        let offset = 64 + 2 + 64 + 64 + 3 * 8 + 2 + 3 * 8 + 2 * 64 + 4 * 8;
        let Some(raw) = raw.get(offset..offset + 8) else {
            return NativeCoverageV1::unsupported::<FamilyDraftReservationV2>(
                "draft held native requests",
                "short typed reservation header",
            );
        };
        let n = u64::from_le_bytes(raw.try_into().expect("exact width"));
        let Some(next) = total.checked_add(n) else {
            return NativeCoverageV1::overflow::<FamilyDraftReservationV2>("draft held native requests");
        };
        total = next;
    }
    NativeCoverageV1::Bounded { bytes: total }
}

/// Public SDK quote. Apply the same native block-prefix preparation before
/// calling; a changed prefix is refused atomically and can be quoted again.
pub fn palw_family_material_begin_quote_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    ctx: &PalwBlockContextV2,
    contract: Hash64,
    target: m::FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    total: u64,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    let b = TransitionBuilder::try_new(state, params, false, false, false, false, extras)?;
    quote(&b, ctx, contract, target, submitter, total)
}

/// First legal supplier delivery spends its original's already funded storage
/// service, capital, packets and holding envelope. No future free-principal
/// assumption finances that required first supply. Others sign a separate quote.
pub(super) fn begin_fee(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    s: &m::SignedFamilyMaterialV1,
    c: &crate::palw_court_family_supplier_v1::FamilySupplierRowV1,
    total: u64,
    body_hash: Hash64,
) -> Result<Option<u64>, PalwStateV2Error> {
    if !requires_reserved_begin_v2(c)? {
        return match s.action {
            m::FamilyMaterialActionV1::Begin { .. } if s.version == 1 => Ok(None),
            _ => Err(refused("legacy upload contract does not authorize a V2 reservation")),
        };
    }
    if matches!(s.action, m::FamilyMaterialActionV1::BeginReserved { .. }) {
        return install(b, ctx, s, c).map(Some);
    }
    if s.version != 1 || s.submitter != c.consent.terms.supplier {
        return Err(refused("additional draft requires its own signed V2 quote and real principal"));
    }
    if c.consent.terms.fee_sompi == 0 {
        return Err(refused("primary draft has no positive original prepaid fee"));
    }
    let inv = m::derive_contract_family_inventory_classified_v3(&b.state, c).map_err(native)?;
    let target = inv.target_id_v1(s.target).map_err(refused)?;
    let key = m::material_aux_key_v1(6, target, None);
    if read::<m::FamilyPrimaryDraftV2>(&b.state, &key)?.is_some() {
        return Err(refused("primary prepaid draft was already consumed; sign an independent V2 quote"));
    }
    let draft = m::family_material_draft_id_v1(s.contract, s.target, s.submitter, total, body_hash).map_err(refused)?;
    let marker = m::FamilyPrimaryDraftV2 {
        contract: s.contract,
        version: 2,
        claim: c.consent.terms.claim,
        target,
        draft,
        material_until_daa: c.consent.terms.material_until_daa,
    };
    put(b, &key, &marker)?;
    Ok(Some(0))
}
pub(super) fn validate_primary(state: &PalwChainStateV2, key: &[u8]) -> Result<(), PalwStateV2Error> {
    let row = read::<m::FamilyPrimaryDraftV2>(state, key)?.ok_or_else(|| refused("primary draft marker missing"))?;
    let c = supplier::family_contract_for_material_v1(state, row.contract)?
        .ok_or_else(|| refused("primary marker lost original signed supplier"))?;
    if row.version != 2
        || row.claim != c.consent.terms.claim
        || row.material_until_daa != c.consent.terms.material_until_daa
        || !requires_reserved_begin_v2(&c)?
        || *key != m::material_aux_key_v1(6, row.target, None)
        || c.targets.binary_search_by_key(&row.target, |t| t.id).is_err()
    {
        return Err(refused("primary draft marker changed original scope"));
    }
    if let Some(d) = read::<m::FamilyMaterialDraftV1>(state, &m::material_aux_key_v1(1, row.draft, None))? {
        let inv = m::derive_contract_family_inventory_classified_v3(state, &c).map_err(native)?;
        if inv.target_id_v1(d.target).map_err(refused)? != row.target
            || d.contract != row.contract
            || d.submitter != c.consent.terms.supplier
            || m::family_material_draft_id_v1(d.contract, d.target, d.submitter, d.total_bytes, d.body_hash).map_err(refused)?
                != row.draft
        {
            return Err(refused("primary draft changed original supplier/material identity"));
        }
    }
    Ok(())
}

/// A finite reservation envelope, not a claim that these rows already exist or
/// any source has delivered. Each primary has its original one-shot marker;
/// arbitrary independent drafts are counted individually from actual births.
fn prior_envelope(state: &PalwChainStateV2, claim: Hash64) -> Result<(u64, u64, u64), PalwStateV2Error> {
    let mut rows = 0;
    let mut keys = 0;
    let mut events = 0;
    for (_, c) in state.family_supplier_rows_v1()?.iter().filter(|(_, c)| c.consent.terms.claim == claim) {
        let inv = m::derive_contract_family_inventory_classified_v3(state, c).map_err(native)?;
        let n = inv.targets_v1().len() as u64;
        let p = inv.targets_v1().iter().try_fold(0, |sum, (_, b)| add(sum, b.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64)))?;
        rows = add(rows, add(add(mul(n, 6)?, p)?, 3)?)?;
        keys = add(keys, add(add(194, mul(n, 582)?)?, mul(p, 73)?)?)?;
        events = add(events, add(add(mul(n, 2)?, mul(p, 2)?)?, 5)?)?;
        if crate::palw_court_family_supplier_v1::signed_profile_generation_v3(&c.consent.terms).map_err(refused)? == 3 {
            rows = add(rows, 1)?;
            keys = add(keys, 65)?;
            events = add(events, 2)?;
        }
    }
    for birth in self::rows(state)?.iter().filter(|b| b.claim == claim) {
        let (total, _, _) = begin_fields(&birth.begin)?;
        let p = total.div_ceil(m::FAMILY_MATERIAL_PART_BYTES_V1 as u64);
        rows = add(rows, add(p, 2)?)?;
        keys = add(keys, add(130, mul(p, 73)?)?)?;
        events = add(events, add(mul(p, 2)?, 4)?)?;
    }
    let binding = state
        .court_service_state_v1()
        .and_then(|s| s.claims.get(&claim))
        .ok_or_else(|| refused("draft prior envelope original binding absent"))?;
    // Source graph packet universe also bounds original non-material control
    // create/delete events. Retention packets conservatively overcount events.
    events = add(events, mul(binding.packet_ids.len() as u64, 2)?)?;
    Ok((rows, keys, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_draft_geometry_prices_metadata_and_closes_holding_book_recursion() {
        let target = m::FamilyMaterialTargetV1::TokenizerRaw;
        let tiny = quoted_geometry(target, 1, 100, 10099, 10100, 0, 0, Hash64::from_u64_word(9), 0, 0, 0).unwrap();
        assert!(tiny.fee_sompi > crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.sompi_per_byte);
        let crowded =
            quoted_geometry(target, 1, 100, 10099, 10100, 50000, 150000, Hash64::from_u64_word(10), 100, 6500, 1000).unwrap();
        assert!(crowded.fee_sompi > tiny.fee_sompi);
        assert!(crowded.collateral_sompi >= tiny.collateral_sompi);
        assert!(crowded.reserved_memory > tiny.reserved_memory);
        assert!(geometry(target, u64::MAX).is_err());
        assert!(quoted_geometry(target, 1, 100, 10099, 10100, u64::MAX, 0, Hash64::from_u64_word(9), 0, 0, 0).is_err());
    }
}

pub(super) fn matches_independent_id(
    state: &PalwChainStateV2,
    d: &m::FamilyMaterialDraftV1,
    id: Hash64,
) -> Result<bool, PalwStateV2Error> {
    let Some(birth) = read::<FamilyDraftReservationV2>(state, &m::material_aux_key_v1(ROW, id, None))? else { return Ok(false) };
    let (total, hash, quote) = begin_fields(&birth.begin)?;
    Ok(birth.contract == d.contract
        && birth.begin.target == d.target
        && birth.begin.submitter == d.submitter
        && total == d.total_bytes
        && hash == d.body_hash
        && birth.expires_daa == d.expires_daa
        && birth.material_until_daa == d.material_until_daa
        && m::family_material_draft_id_v2(d.contract, d.target, d.submitter, total, hash, quote).map_err(refused)? == id)
}

/// Observation block hashes and reassignable slots are never pre-signed.
/// A genuinely different packet/claim universe requires a fresh consent.
fn book_scope(
    state: &PalwChainStateV2,
    book: &crate::palw_court_service_state_v2::CourtServiceStateV2,
) -> Result<Hash64, PalwStateV2Error> {
    let extra = state.court_service_row_v3_v1().map(|r| (r.version, &r.resolved, &r.causal));
    hash(
        b"misaka-palw/family-draft/immutable-book/v2",
        &(book.version, book.profile_id, book.source_profile_id, book.book.budget, &book.book.packets, &book.claims, extra),
    )
}
fn book_bounds(
    state: &PalwChainStateV2,
    book: &crate::palw_court_service_state_v2::CourtServiceStateV2,
) -> Result<(u64, u64), PalwStateV2Error> {
    use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding};
    use crate::palw_native_resource_v1::BTreeNodeEnvelopeV1 as Node;
    book.check_live_v1().map_err(refused)?;
    let packets = book.book.packets.len() as u64;
    let claims = book.claims.len() as u64;
    // ServiceReservations::check fixes assignments==N, remaining<=N and
    // supply<=remaining. Each live packet also occurs in its owner's set once.
    // These bounds are invariant under unknown observation hashes and clocks.
    let mut wire = add(191, add(mul(packets, 456)?, mul(claims, 276)?)?)?;
    let per = book_packet_heap_v3();
    let mut heap = add(
        std::mem::size_of::<CourtServiceStateV2>() as u64,
        add(mul(packets, per)?, mul(claims, std::mem::size_of::<Node<Hash64, ServiceClaimBinding>>() as u64)?)?,
    )?;
    if let Some(v3) = state.court_service_row_v3_v1() {
        // The real writer copies and journals the whole released form, including
        // resolved records and causal vectors. A change here requires a fresh quote.
        wire = add(wire, add(2, borsh::object_length(&(&v3.resolved, &v3.causal)).map_err(refused)? as u64)?)?;
        let extra = v3
            .resolved
            .native_heap_v1("draft V3 resolved book")
            .add::<FamilyDraftReservationV2>(v3.causal.native_heap_v1("draft V3 causal book"), "draft V3 immutable book");
        heap = add(
            heap,
            add(
                extra.bytes().ok_or(PalwStateV2Error::NativeResource(extra))?,
                (std::mem::size_of_val(v3) - std::mem::size_of::<CourtServiceStateV2>()) as u64,
            )?,
        )?;
    }
    Ok((wire, heap))
}

#[cfg(test)]
pub(in crate::palw_state_v2) fn legacy_quote_diagnostic_v3(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    ctx: &PalwBlockContextV2,
    contract: Hash64,
    target: m::FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    total: u64,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    let b = TransitionBuilder::try_new(state, params, false, false, false, false, extras)?;
    let q = quote(&b, ctx, contract, target, submitter, total)?;
    let row =
        supplier::family_contract_for_material_v1(state, contract)?.ok_or_else(|| refused("legacy diagnostic contract absent"))?;
    let legacy = quoted_geometry_for_law_v3(
        &row.consent.terms,
        target,
        total,
        q.accept_from_daa,
        q.accept_until_daa,
        q.respond_until_daa,
        q.book_bytes,
        q.book_heap,
        q.book,
        q.prior_rows,
        q.prior_key_bytes,
        q.prior_events,
        false,
        false,
    )?;
    let (rederived_old, old_new) = quoted_geometry_for_signed_v3(
        &row.consent.terms,
        target,
        total,
        &legacy,
        q.respond_until_daa,
        q.prior_rows,
        q.prior_key_bytes,
        q.prior_events,
    )?;
    let (rederived_new, new_new) = quoted_geometry_for_signed_v3(
        &row.consent.terms,
        target,
        total,
        &q,
        q.respond_until_daa,
        q.prior_rows,
        q.prior_key_bytes,
        q.prior_events,
    )?;
    assert!(!old_new && new_new);
    assert_eq!(rederived_old, legacy);
    assert_eq!(rederived_new, q);
    let mut disguised = q.clone();
    disguised.profile = legacy.profile;
    assert_ne!(
        quoted_geometry_for_signed_v3(
            &row.consent.terms,
            target,
            total,
            &disguised,
            q.respond_until_daa,
            q.prior_rows,
            q.prior_key_bytes,
            q.prior_events
        )?
        .0,
        disguised,
        "new cheap contribution cannot borrow an old wholebook quote identity"
    );
    Ok(legacy)
}

#[cfg(test)]
mod contribution_scope_tests_v3 {
    use super::*;
    #[test]
    fn independent_draft_oldbook_workspace_is_local_not_new_durable_holding_v3() {
        let target = m::FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::PromptFlat);
        let count = 7486u64;
        let heap = mul(count, book_packet_heap_v3()).unwrap();
        let wire = add(191, mul(count, 456).unwrap()).unwrap();
        let old = quoted_geometry(target, 28, 100, 10099, 10100, wire, heap, Hash64::from_u64_word(9), 0, 0, 0).unwrap();
        let own =
            quoted_geometry_scope_v3(target, 28, 100, 10099, 10100, wire, heap, Hash64::from_u64_word(9), 0, 0, 0, true).unwrap();
        let same_own =
            quoted_geometry_scope_v3(target, 28, 100, 10099, 10100, 0, 0, Hash64::from_u64_word(10), 0, 0, 0, true).unwrap();
        assert_eq!(
            (own.reserved_memory, own.fee_sompi, own.collateral_sompi),
            (same_own.reserved_memory, same_own.fee_sompi, same_own.collateral_sompi),
            "unrelated old book changes local workspace and signed universe, not new durable contribution"
        );
        assert!(old.reserved_memory > own.reserved_memory && old.collateral_sompi > own.collateral_sompi);
        assert_eq!(validator_book_workspace_v3(wire, heap).unwrap(), 4 * heap + 2 * wire);
        assert!(validator_book_workspace_v3(wire, heap).unwrap() > own.reserved_memory);
        assert!(book_packet_heap_v3() > 0);
        assert!(validator_book_workspace_v3(u64::MAX, heap).is_err(), "actual local workspace overflow remains a refusal");
    }
}

#[cfg(test)]
pub(in crate::palw_state_v2) fn prior_contribution_quote_diagnostic_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    ctx: &PalwBlockContextV2,
    contract: Hash64,
    target: m::FamilyMaterialTargetV1,
    submitter: PalwBondKeyV2,
    total: u64,
) -> Result<FamilyDraftQuoteV2, PalwStateV2Error> {
    let b = TransitionBuilder::try_new(state, params, false, false, false, false, extras)?;
    let q = quote(&b, ctx, contract, target, submitter, total)?;
    let row =
        supplier::family_contract_for_material_v1(state, contract)?.ok_or_else(|| refused("prior contribution diagnostic absent"))?;
    let prior = quoted_geometry_for_law_v3(
        &row.consent.terms,
        target,
        total,
        q.accept_from_daa,
        q.accept_until_daa,
        q.respond_until_daa,
        q.book_bytes,
        q.book_heap,
        q.book,
        q.prior_rows,
        q.prior_key_bytes,
        q.prior_events,
        true,
        false,
    )?;
    let (exact, contribution) = quoted_geometry_for_signed_v3(
        &row.consent.terms,
        target,
        total,
        &prior,
        q.respond_until_daa,
        q.prior_rows,
        q.prior_key_bytes,
        q.prior_events,
    )?;
    assert!(contribution);
    assert_eq!(exact, prior);
    let mut disguised = q.clone();
    disguised.profile = prior.profile;
    assert_ne!(
        quoted_geometry_for_signed_v3(
            &row.consent.terms,
            target,
            total,
            &disguised,
            q.respond_until_daa,
            q.prior_rows,
            q.prior_key_bytes,
            q.prior_events
        )?
        .0,
        disguised
    );
    Ok(prior)
}
#[cfg(test)]
mod index_contribution_scope_tests_v4 {
    use super::*;
    #[test]
    fn independent_draft_latest_index_rewrites_are_local_and_old_laws_remain_exact_v4() {
        let target = m::FamilyMaterialTargetV1::Input(crate::palw_court_shared_material_intake_v1::MaterialTargetV1::PromptFlat);
        let make = |rows, keys, events, index_local| {
            quoted_geometry_scope_v4(
                target,
                28,
                100,
                10099,
                10100,
                0,
                0,
                Hash64::from_u64_word(9),
                rows,
                keys,
                events,
                true,
                index_local,
            )
            .unwrap()
        };
        let own = make(0, 0, 0, true);
        let busy = make(100, 20000, 15000, true);
        assert_eq!(
            (own.reserved_memory, own.journal_bytes, own.fee_sompi, own.collateral_sompi),
            (busy.reserved_memory, busy.journal_bytes, busy.fee_sompi, busy.collateral_sompi)
        );
        let old = make(100, 20000, 15000, false);
        let legacy =
            quoted_geometry_scope_v3(target, 28, 100, 10099, 10100, 0, 0, Hash64::from_u64_word(9), 100, 20000, 15000, true).unwrap();
        assert_eq!(old, legacy);
        assert!(old.reserved_memory > busy.reserved_memory);
        assert!(own.fee_sompi > 0 && own.collateral_sompi > 0 && own.retained_bytes > 0 && own.journal_bytes > 0);
    }
}
