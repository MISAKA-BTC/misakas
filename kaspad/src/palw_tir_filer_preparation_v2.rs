//! Admitted actual filer workspace. No journal bytes confer current authority.
use crate::palw_memory_ledger::{PalwMemoryReservationKeyV1, PalwMemoryReservationV1, host_ledger_v1};
use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, NativeHeapV1, reserve_native_requests_v1},
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_state_v2::PalwBondKeyV2,
    palw_tir_public_carriage_v1::{
        PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1, PALW_TIR_PUBLIC_CONVICTION_MAX_IDS_V1, PALW_TIR_PUBLIC_WITNESS_PART_BYTES_V1,
        PalwTirPublicControlTargetV1, PalwTirPublicControlV1, PalwTirPublicConvictionEvidenceV1 as Evidence,
    },
    palw_tir_public_claim_v1::PalwTirPublicFrameViewV1,
};
#[derive(Debug)]
pub(super) enum FilerPreparationFaultV2 {
    Resource(NativeCoverageV1),
    Semantic(String),
}
fn capacity(why: &str) -> FilerPreparationFaultV2 {
    FilerPreparationFaultV2::Resource(NativeCoverageV1::unsupported::<PreparedFilingV2>("TIR filer preparation", why))
}
fn reserve(bytes: u64, role: &'static str) -> Result<(Option<NativeDemandLeaseV1>, PalwMemoryReservationV1), FilerPreparationFaultV2> {
    let native = reserve_native_requests_v1(bytes).map_err(FilerPreparationFaultV2::Resource)?;
    let host = host_ledger_v1()
        .reserve(PalwMemoryReservationKeyV1 { role, class_id: Hash64::default(), job: Hash64::default() }, bytes)
        .map_err(|_| capacity("host ledger admission refused"))?;
    Ok((native, host))
}
pub(super) struct PreparedFilingV2 {
    pub evidence: Evidence,
    pub encoded: Vec<u8>,
    pub staged: bool,
    _native: Option<NativeDemandLeaseV1>,
    _host: PalwMemoryReservationV1,
}
impl PreparedFilingV2 {
    pub fn new(
        evidence: &Evidence,
        view: &PalwTirPublicFrameViewV1,
        filer: PalwBondKeyV2,
        network: Hash64,
    ) -> Result<Self, FilerPreparationFaultV2> {
        let encoded_len = borsh::object_length(evidence).map_err(|_| capacity("evidence encoded length overflow"))?;
        let staged = matches!(evidence,Evidence::Output{ids,..} if ids.len()>PALW_TIR_PUBLIC_CONVICTION_MAX_IDS_V1)
            || (encoded_len as u64).checked_add(4096).ok_or_else(|| capacity("control length overflow"))?
                > PALW_TIR_PUBLIC_CONTROL_MAX_BYTES_V1;
        if staged && encoded_len as u64 > view.maximum_witness_bytes {
            return Err(FilerPreparationFaultV2::Semantic("witness exceeds accepted frame prepaid ceiling".into()));
        }
        let heap = evidence.native_heap_v1("TIR filer evidence copies");
        let heap = heap.bytes().ok_or_else(|| FilerPreparationFaultV2::Resource(heap))?;
        let mut q = PalwMaterialConstructionRequestsV2::new();
        // At most three live deep copies beyond the book's original: this
        // snapshot, the ordinary owned API argument/build object, and a shape
        // check's stripped proof. These are actual typed heap, not codec bytes.
        for _ in 0..3 {
            q.add_v1(heap).map_err(FilerPreparationFaultV2::Resource)?;
        }
        q.vec_v1::<u8>(encoded_len as u64).map_err(FilerPreparationFaultV2::Resource)?;
        q.vec_v1::<PalwTirPublicControlV1>(1).map_err(FilerPreparationFaultV2::Resource)?;
        q.vec_v1::<u8>(kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN as u64)
            .map_err(FilerPreparationFaultV2::Resource)?;
        let part = encoded_len.min(PALW_TIR_PUBLIC_WITNESS_PART_BYTES_V1);
        q.vec_v1::<u8>(part as u64).map_err(FilerPreparationFaultV2::Resource)?;
        // Actual signing tuple prefix, without the action. Both action tags are
        // one byte; staged fields are total/digest/offset/u32 bytes length.
        let prefix = borsh::object_length(&(
            network,
            1u16,
            view.claim,
            view.allocation_id,
            view.source_id,
            PalwTirPublicControlTargetV1::ConvictionWitness,
            0u64,
            view.conviction_rent_sompi,
            filer,
        ))
        .map_err(|_| capacity("signing prefix length overflow"))?;
        let action = if staged { part.checked_add(8 + 64 + 8 + 4) } else { Some(encoded_len) }
            .ok_or_else(|| capacity("action length overflow"))?;
        let message = prefix.checked_add(1).and_then(|n| n.checked_add(action)).ok_or_else(|| capacity("signing length overflow"))?;
        // borsh::to_vec starts at1024 and Vec doubling can retain old/new
        // buffers during growth. Two complete power-of-two envelopes cover
        // that live peak (initial allocation included), not decoded storage.
        let envelope = message.max(1024).checked_next_power_of_two().ok_or_else(|| capacity("signing buffer capacity overflow"))?;
        q.vec_v1::<u8>(envelope as u64).map_err(FilerPreparationFaultV2::Resource)?;
        q.vec_v1::<u8>(envelope as u64).map_err(FilerPreparationFaultV2::Resource)?;
        let (native, host) = reserve(q.bytes_v1(), "public-tir-filer-workspace")?;
        evidence.check_staged_v1().map_err(FilerPreparationFaultV2::Semantic)?;
        let mut encoded = Vec::new();
        encoded.try_reserve_exact(encoded_len).map_err(|_| capacity("evidence encode allocation refused"))?;
        borsh::to_writer(&mut encoded, evidence).map_err(|_| capacity("admitted evidence encode failed"))?;
        if encoded.len() != encoded_len {
            return Err(capacity("evidence encoding changed length"));
        }
        Ok(Self { evidence: evidence.clone(), encoded, staged, _native: native, _host: host })
    }
}

pub(super) struct FilerClaimListV2 {
    pub claims: Vec<Hash64>,
    _native: Option<NativeDemandLeaseV1>,
    _host: PalwMemoryReservationV1,
}
impl FilerClaimListV2 {
    pub fn new<'a>(keys: impl ExactSizeIterator<Item = &'a Hash64>) -> Result<Self, FilerPreparationFaultV2> {
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<Hash64>(keys.len() as u64).map_err(FilerPreparationFaultV2::Resource)?;
        let (native, host) = reserve(q.bytes_v1(), "public-tir-filer-claims")?;
        let mut claims = Vec::new();
        claims.try_reserve_exact(keys.len()).map_err(|_| capacity("claim list allocation refused"))?;
        claims.extend(keys.copied());
        Ok(Self { claims, _native: native, _host: host })
    }
}
