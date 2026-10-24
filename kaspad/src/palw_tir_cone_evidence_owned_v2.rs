//! Nonwire allocation handoff from a completed cone through the pending filer.
use kaspa_consensus_core::{
    palw_backend::{PalwTirPublicConeFaultV2 as Fault, PalwTirPublicConeReadyV2},
    palw_court_v2::PalwCourtVerdictProofV2,
    palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
    palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
};
pub(crate) struct ConeEvidenceHoldV2 {
    _native: Option<NativeDemandLeaseV1>,
    _extra: Option<NativeDemandLeaseV1>,
    _box_host: crate::palw_memory_ledger::PalwMemoryReservationV1,
    _host: Vec<Box<dyn Send + Sync>>,
}
pub(crate) struct OwnedConeEvidenceV2 {
    // Always drop evidence before its construction/retention resources.
    evidence: PalwTirPublicConvictionEvidenceV1,
    hold: ConeEvidenceHoldV2,
}
impl OwnedConeEvidenceV2 {
    pub(crate) fn from_ready_v2(ready: PalwTirPublicConeReadyV2) -> Result<Self, Fault> {
        // The wrapper stays on the stack; the actual new heap allocation is
        // one Box<Proof>, separately retained in both quota families.
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<PalwCourtVerdictProofV2>(1).map_err(|e| Fault::LocalResource(e.to_string()))?;
        let extra = reserve_native_requests_v1(q.bytes_v1()).map_err(|e| Fault::LocalResource(e.to_string()))?;
        let box_host = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-evidence-box",
                    class_id: Default::default(),
                    job: Default::default(),
                },
                std::mem::size_of::<PalwCourtVerdictProofV2>() as u64,
            )
            .map_err(|e| Fault::LocalResource(e.to_string()))?;
        // Existing shape validation clones the proof to verify its program is
        // stripped. Admit that transient deep copy, including when native
        // logical quotas are disabled but the host ledger is active.
        use kaspa_consensus_core::palw_native_resource_v1::NativeHeapV1;
        let copy = ready.proof_v2().native_heap_v1("TIR evidence shape check");
        let copy = copy
            .bytes()
            .ok_or_else(|| Fault::LocalResource(copy.to_string()))?
            .checked_add(std::mem::size_of::<PalwCourtVerdictProofV2>() as u64)
            .ok_or_else(|| Fault::LocalResource("TIR evidence shape check overflow".into()))?;
        let _check_native = reserve_native_requests_v1(copy).map_err(|e| Fault::LocalResource(e.to_string()))?;
        let _check = crate::palw_memory_ledger::host_ledger_v1()
            .reserve(
                crate::palw_memory_ledger::PalwMemoryReservationKeyV1 {
                    role: "public-tir-evidence-shape",
                    class_id: Default::default(),
                    job: Default::default(),
                },
                copy,
            )
            .map_err(|e| Fault::LocalResource(e.to_string()))?;
        let (mut proof, native, host, _) = ready.into_parts_v2();
        proof.tir_strip_program_v1();
        let evidence = PalwTirPublicConvictionEvidenceV1::Arithmetic { proof: Box::new(proof) };
        evidence.check_staged_v1().map_err(Fault::Unavailable)?;
        Ok(Self { evidence, hold: ConeEvidenceHoldV2 { _native: native, _extra: extra, _box_host: box_host, _host: host } })
    }
    pub(crate) fn into_parts_v2(self) -> (PalwTirPublicConvictionEvidenceV1, ConeEvidenceHoldV2) {
        (self.evidence, self.hold)
    }
}
