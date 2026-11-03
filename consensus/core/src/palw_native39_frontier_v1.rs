//! Mature native39 work anchors retained in the rooted journal. These are data
//! identities from accepted PNR9 spends, never computation-validity proofs.
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeHeapV1, NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1,
};
use crate::{
    Hash64,
    palw_state_v2::{PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error},
};
use borsh::{BorshDeserialize, BorshSerialize};
pub const NATIVE39_FRONTIER_TABLE_V1: u8 = 193;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Native39FrontierAnchorV1 {
    pub version: u16,
    pub receipt: Hash64,
    pub claim: Hash64,
    pub claim_root: Hash64,
    pub receipt_root: Hash64,
    pub approval: Hash64,
    pub work: Hash64,
    pub quantum: u32,
    pub anchor: Hash64,
    pub anchor_blue: u64,
    pub anchor_daa: u64,
    pub established_daa: u64,
    pub expires: u64,
    pub matured_daa: u64,
    pub quantum_maturity_daa: u64,
    pub priced_work: u128,
    pub issuance: u64,
    pub operator: Hash64,
    pub class: Hash64,
    pub claim_source: crate::palw_native_mining_v39::NativeMiningClaimRecordV39,
    pub receipt_source: crate::palw_native_mining_v39::NativeMiningReceiptRecordV39,
}
crate::palw_native_resource_v1::native_struct!(Native39FrontierAnchorV1;version,receipt,claim,claim_root,receipt_root,approval,work,quantum,anchor,anchor_blue,anchor_daa,established_daa,expires,matured_daa,quantum_maturity_daa,priced_work,issuance,operator,class,claim_source,receipt_source);
impl NativeWireScanV1 for Native39FrontierAnchorV1 {
    fn scan_native_wire_v1<R: std::io::Read>(
        r: &mut crate::palw_native_resource_v1::NativeWireReaderV1<R>,
    ) -> std::io::Result<crate::palw_native_resource_v1::NativeWireShapeV1> {
        let mut shape = u16::scan_native_wire_v1(r)?;
        for _ in 0..6 {
            shape = shape.merge(Hash64::scan_native_wire_v1(r)?)?;
        }
        shape = shape.merge(u32::scan_native_wire_v1(r)?)?;
        shape = shape.merge(Hash64::scan_native_wire_v1(r)?)?;
        for _ in 0..6 {
            shape = shape.merge(u64::scan_native_wire_v1(r)?)?;
        }
        shape = shape.merge(u128::scan_native_wire_v1(r)?)?;
        shape = shape.merge(u64::scan_native_wire_v1(r)?)?;
        for _ in 0..2 {
            shape = shape.merge(Hash64::scan_native_wire_v1(r)?)?;
        }
        let before = r.remaining();
        shape = shape.merge(crate::palw_native_mining_v39::NativeMiningClaimRecordV39::scan_native_wire_v1(r)?)?;
        if before - r.remaining() > 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native39 frozen claim exceeds existing native source bound",
            ));
        }
        let before = r.remaining();
        shape = shape.merge(crate::palw_native_mining_v39::NativeMiningReceiptRecordV39::scan_native_wire_v1(r)?)?;
        if before - r.remaining() > 16384 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native39 frozen receipt exceeds existing native source bound",
            ));
        }
        Ok(shape)
    }
}

fn bad(s: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.to_string())
}
pub(crate) fn native39_frontier_identity_v1(work: &Hash64, quantum: u32) -> Hash64 {
    crate::palw_native_settlement_v1::fp_slice_identity(work, quantum)
}
pub fn native39_frontier_anchors_v1(
    state: &PalwChainStateV2,
) -> Result<NativeRetainedV1<Vec<(Hash64, Native39FrontierAnchorV1)>>, PalwStateV2Error> {
    let count =
        state.kernel_route_state_v1().map_or(0, |r| r.aux.keys().filter(|(table, _)| *table == NATIVE39_FRONTIER_TABLE_V1).count());
    let mut requests = TirNativeRequestsV1::new();
    requests.exact_v1::<(Hash64, Native39FrontierAnchorV1)>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
    if let Some(route) = state.kernel_route_state_v1() {
        for ((table, _), bytes) in &route.aux {
            if *table == NATIVE39_FRONTIER_TABLE_V1 {
                let shape = crate::palw_native_resource_v1::preflight_native_value_v1::<Native39FrontierAnchorV1>(
                    bytes.as_slice(),
                    bytes.len() as u64,
                )
                .map_err(bad)?;
                requests.add_v1(shape.logical_requests_v1()).map_err(PalwStateV2Error::NativeResource)?;
            }
        }
    }
    let lease =
        crate::palw_native_resource_v1::reserve_native_requests_v1(requests.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
    let mut out = Vec::new();
    out.try_reserve_exact(count).map_err(|_| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Native39FrontierAnchorV1>(
            "native39 frontier anchors",
            "allocation refused",
        ))
    })?;
    if let Some(route) = state.kernel_route_state_v1() {
        for ((table, key), bytes) in &route.aux {
            if *table != NATIVE39_FRONTIER_TABLE_V1 {
                continue;
            }
            let id = Hash64::from_bytes(key.as_slice().try_into().map_err(|_| bad("native39 frontier key width"))?);
            let row = crate::palw_native_resource_v1::retain_native_value_decode_classified_v1::<
                Native39FrontierAnchorV1,
                std::io::Error,
            >(bytes, || borsh::from_slice(bytes))
            .map_err(|e| match e {
                NativeDecodeErrorV1::Resource(r) => PalwStateV2Error::NativeResource(r),
                NativeDecodeErrorV1::Decode(e) => bad(e),
            })?;
            if row.version != 1
                || row.work == Hash64::default()
                || row.anchor == Hash64::default()
                || row.claim == Hash64::default()
                || row.approval == Hash64::default()
                || id != native39_frontier_identity_v1(&row.work, row.quantum)
                || row.matured_daa < row.expires
                || row.anchor_daa > row.matured_daa
            {
                return Err(bad("native39 frontier anchor identity/maturity"));
            }
            validate_native39_anchor_source_v1(&row)?;
            out.push((id, (*row).clone()));
        }
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn native39_source_scratch_v1(
    claim: &crate::palw_native_mining_v39::NativeMiningClaimRecordV39,
    receipt: &crate::palw_native_mining_v39::NativeMiningReceiptRecordV39,
    copies: u64,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwStateV2Error> {
    let mut q = TirNativeRequestsV1::new();
    for coverage in [claim.native_heap_v1("native39 frozen claim"), receipt.native_heap_v1("native39 frozen receipt")] {
        q.add_v1(coverage.bytes().ok_or(PalwStateV2Error::NativeResource(coverage.clone()))?)
            .map_err(PalwStateV2Error::NativeResource)?;
    }
    let claim_bytes = borsh::object_length(claim).map_err(bad)?;
    let receipt_bytes = borsh::object_length(receipt).map_err(bad)?;
    if claim_bytes > 16384 || receipt_bytes > 16384 {
        return Err(bad("native39 frozen source exceeds existing receipt admission bound"));
    }
    let bytes = claim_bytes.checked_add(receipt_bytes).ok_or(PalwStateV2Error::Overflow("native39 frozen source length"))?;
    q.exact_v1::<u8>((bytes as u64).checked_mul(copies).ok_or(PalwStateV2Error::Overflow("native39 frozen source scratch"))?)
        .map_err(PalwStateV2Error::NativeResource)?;
    crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)
}
fn validate_native39_anchor_source_v1(row: &Native39FrontierAnchorV1) -> Result<(), PalwStateV2Error> {
    let c = &row.claim_source;
    let r = &row.receipt_source;
    let _scratch = native39_source_scratch_v1(c, r, 2)?;
    if c.version != 39
        || r.version != 39
        || c.root() != row.claim_root.as_bytes()
        || r.root() != row.receipt_root.as_bytes()
        || c.id != row.claim.as_bytes()
        || r.id != row.receipt.as_bytes()
        || r.claim != c.id
        || r.id != crate::palw_native_mining_v39::native_mining_receipt_id_v39(c.id)
        || r.approval != row.approval.as_bytes()
        || r.price != c.price
        || r.price.work != row.work.as_bytes()
        || r.price.work != c.work.root()
        || r.model != c.model
        || r.master != c.master
        || r.operator != c.operator
        || r.price.model != c.model
        || r.price.master != c.master
        || r.established_daa != row.established_daa
        || r.expires != row.expires
        || r.operator != row.operator.as_bytes()
        || r.producer != c.producer
        || row.class != Hash64::from_bytes(crate::palw_native_mining_v39::native_mining_economic_class_v39(r.model))
        || row.priced_work != u128::from(r.price.quantum_mac)
        || u64::from(row.quantum) >= r.price.quanta
        || !r.is_spent(row.quantum)
        || !c.closed.is_some_and(|(_, issued)| issued)
    {
        return Err(bad("native39 frozen source identity/approval/price/actual quantum differs"));
    }
    Ok(())
}
/// Called only by the owning receipt fold after its native Gate C, budget and
/// once-only spend checks. All preimages are retained before their rows retire.
pub(crate) fn prepare_native39_frontier_anchor_v1(
    params: &PalwStateParamsV2,
    ctx: &crate::palw_state_v2::PalwBlockContextV2,
    maturity: u64,
    c: &crate::palw_native_mining_v39::NativeMiningClaimRecordV39,
    r: &crate::palw_native_mining_v39::NativeMiningReceiptRecordV39,
    quantum: u32,
    issuance: u64,
) -> Result<NativeRetainedV1<Native39FrontierAnchorV1>, PalwStateV2Error> {
    let lease = native39_source_scratch_v1(c, r, 2)?;
    let matured_daa =
        crate::palw_native_settlement_v1::native39_matured_daa_v1(params, maturity, ctx.daa_score, r.established_daa, r.expires)
            .ok_or_else(|| bad("native39 frontier certification clock absent"))?;
    let row = Native39FrontierAnchorV1 {
        version: 1,
        receipt: Hash64::from_bytes(r.id),
        claim: c.claim_hash(),
        claim_root: Hash64::from_bytes(c.root()),
        receipt_root: Hash64::from_bytes(r.root()),
        approval: Hash64::from_bytes(r.approval),
        work: Hash64::from_bytes(r.price.work),
        quantum,
        anchor: ctx.block,
        anchor_blue: ctx.blue_score,
        anchor_daa: ctx.daa_score,
        established_daa: r.established_daa,
        expires: r.expires,
        matured_daa,
        quantum_maturity_daa: maturity,
        priced_work: u128::from(r.price.quantum_mac),
        issuance,
        operator: Hash64::from_bytes(r.operator),
        class: Hash64::from_bytes(crate::palw_native_mining_v39::native_mining_economic_class_v39(r.model)),
        claim_source: c.clone(),
        receipt_source: r.clone(),
    };
    validate_native39_anchor_source_v1(&row)?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(row, lease))
}
/// Existing native collection leaves a permanent authenticated tombstone over
/// the original claim root; collecting never manufactures an approval.
pub(crate) fn native39_anchor_collected_v1(state: &PalwChainStateV2, row: &Native39FrontierAnchorV1) -> bool {
    use crate::palw_native_mining_v39::*;
    let Some(route) = state.kernel_route_state_v1() else { return false };
    let key = crate::palw_native_receipt_v39::native_mining_row_key_v39(MINING_COLLECTED_KIND_V39, &row.claim.as_bytes());
    route
        .aux
        .get(&(MINING_TABLE_V39, key))
        .and_then(|bytes| borsh::from_slice::<([u8; 64], Option<(u64, bool)>, u64)>(bytes).ok())
        .is_some_and(|(root, closed, _)| root == row.claim_root.as_bytes() && closed.is_some_and(|(_, issued)| issued))
}
fn reserve_native39_source_v1(
    state: &PalwChainStateV2,
    row: &Native39FrontierAnchorV1,
) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, PalwStateV2Error> {
    use crate::palw_native_mining_v39::*;
    use crate::palw_native_resource_v1::preflight_native_value_v1;
    let Some(route) = state.kernel_route_state_v1() else { return Ok(None) };
    let mut quote = TirNativeRequestsV1::new();
    quote.exact_v1::<u8>(3 * 66).map_err(PalwStateV2Error::NativeResource)?;
    let mut add = |bytes: &[u8],
                   shape: std::io::Result<crate::palw_native_resource_v1::NativeWireValuePreflightV1>|
     -> Result<(), PalwStateV2Error> {
        let shape = shape.map_err(bad)?;
        quote.add_v1(shape.logical_requests_v1()).map_err(PalwStateV2Error::NativeResource)?;
        quote
            .exact_v1::<u8>((bytes.len() as u64).checked_mul(2).ok_or(PalwStateV2Error::Overflow("native39 source hash scratch"))?)
            .map_err(PalwStateV2Error::NativeResource)?;
        Ok(())
    };
    for (kind, id) in [(MINING_CLAIM_KIND_V39, row.claim), (MINING_RECEIPT_KIND_V39, row.receipt)] {
        let key = crate::palw_native_receipt_v39::native_mining_row_key_v39(kind, &id.as_bytes());
        if let Some(bytes) = route.aux.get(&(MINING_TABLE_V39, key)) {
            if kind == MINING_CLAIM_KIND_V39 {
                add(bytes, preflight_native_value_v1::<([u8; 64], NativeMiningClaimRecordV39)>(bytes.as_slice(), bytes.len() as u64))?;
            } else {
                add(
                    bytes,
                    preflight_native_value_v1::<([u8; 64], NativeMiningReceiptRecordV39)>(bytes.as_slice(), bytes.len() as u64),
                )?;
            }
        }
    }
    let key = crate::palw_security_certification_v1::certification_native_key_v39(
        &crate::palw_security_certification_v1::palw_native_certification_id_v39(&row.claim),
    );
    if let Some(bytes) = route.aux.get(&(crate::palw_security_certification_v1::PALW_SECURITY_CERTIFICATION_TABLE_V1, key)) {
        add(
            bytes,
            preflight_native_value_v1::<crate::palw_security_certification_v1::PalwClaimCertificationRowV1>(
                bytes.as_slice(),
                bytes.len() as u64,
            ),
        )?;
    }
    crate::palw_native_resource_v1::reserve_native_requests_v1(quote.bytes_v1()).map_err(PalwStateV2Error::NativeResource)
}
pub(crate) fn native39_frontier_standing_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    row: &Native39FrontierAnchorV1,
) -> bool {
    use crate::palw_native_receipt_v39::*;
    if native_mining_convicted_v39(state, &row.claim) || native_mining_accusation_pending_v39(state, &row.claim) {
        return false;
    }
    if let Some((receipt, claim)) = native_mining_receipt_view_v39(state, &row.receipt) {
        receipt.claim == row.claim.as_bytes()
            && receipt.approval == row.approval.as_bytes()
            && claim.root() == row.claim_root.as_bytes()
            && receipt.price.work == row.work.as_bytes()
            && receipt.is_spent(row.quantum)
            && receipt.established_daa == row.established_daa
            && receipt.expires == row.expires
            && row.operator == Hash64::from_bytes(receipt.operator)
            && row.class == Hash64::from_bytes(crate::palw_native_mining_v39::native_mining_economic_class_v39(receipt.model))
            && row.priced_work == u128::from(receipt.price.quantum_mac)
            && crate::palw_rfc0018_v1::palw_native_independently_work_certified_v39(params, state, &row.claim, claim.admitted_daa)
    } else {
        native39_anchor_collected_v1(state, row)
    }
}
pub(crate) fn validate_native39_frontier_v1(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
    for (_, row) in native39_frontier_anchors_v1(state)?.iter() {
        let minimum = crate::palw_native_settlement_v1::native39_matured_daa_v1(
            params,
            row.quantum_maturity_daa,
            row.anchor_daa,
            row.established_daa,
            row.expires,
        )
        .ok_or_else(|| bad("native39 frontier certification clock absent"))?;
        if row.matured_daa != minimum {
            return Err(bad("native39 frontier differs from exact birth maturity/liability/retirement"));
        }
        // A conviction retains a tombstone and removes eligibility. The anchor
        // remains journalled until its ordinary collection/retirement sweep.
        if crate::palw_native_receipt_v39::native_mining_convicted_v39(state, &row.claim) {
            continue;
        }
        let _source_lease = reserve_native39_source_v1(state, row)?;
        if !native39_frontier_standing_v1(state, params, row)
            && !crate::palw_native_receipt_v39::native_mining_accusation_pending_v39(state, &row.claim)
        {
            return Err(bad("native39 frontier lost its selected approval/source or collection tombstone"));
        }
    }
    Ok(())
}

pub fn native39_frontier_anchor_ready_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    row: &Native39FrontierAnchorV1,
    ctx: &crate::palw_state_v2::PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    let _source_lease = reserve_native39_source_v1(state, row)?;
    Ok(ctx.daa_score >= row.matured_daa
        && row.priced_work > 0
        && row.class != params.base_class_id()
        && native39_frontier_standing_v1(state, params, row))
}

/// Full network context at node state installation/load/import. The ruleset's
/// value is checked separately from the anchor's exact arithmetic; a header root
/// authenticates identity and does not replace this semantic check.
pub fn validate_native39_frontier_context_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    expected_quantum_maturity_daa: u64,
) -> Result<(), PalwStateV2Error> {
    validate_native39_frontier_v1(state, params)?;
    for (_, row) in native39_frontier_anchors_v1(state)?.iter() {
        if row.quantum_maturity_daa != expected_quantum_maturity_daa {
            return Err(bad("native39 frontier birth maturity differs from network rules"));
        }
    }
    Ok(())
}

/// The retained anchor is an accepted quantum from the same canonical work as
/// its spend delta. Keeping this reader rooted lets a real pruning checkpoint
/// retain the evidence without depending on a rebuildable delta cache.
pub fn native39_frontier_fact_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    row: &Native39FrontierAnchorV1,
    ctx: &crate::palw_state_v2::PalwBlockContextV2,
) -> Result<Option<crate::palw_native_settlement_v1::MatureUsefulWorkV1>, PalwStateV2Error> {
    if !native39_frontier_anchor_ready_v1(state, params, row, ctx)? {
        return Ok(None);
    }
    Ok(Some(crate::palw_native_settlement_v1::MatureUsefulWorkV1 {
        identity: native39_frontier_identity_v1(&row.work, row.quantum),
        anchor: row.anchor,
        operator: row.operator,
        class: row.class,
        anchor_blue: row.anchor_blue,
        accepted_blue: row.anchor_blue,
        anchor_daa: row.anchor_daa,
        accepted_daa: row.anchor_daa,
        matured_daa: row.matured_daa,
        work: row.priced_work,
    }))
}
pub fn native39_frontier_retained_until_v1(params: &PalwStateParamsV2, row: &Native39FrontierAnchorV1) -> u64 {
    row.matured_daa.saturating_add(params.claim_retirement_daa())
}
