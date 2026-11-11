//! Actual candidate measurements for singleton acceptance. No paid/source or
//! accepted-work authority is constructed from the unadmitted own header.
use super::*;
use kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3;
use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
use kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1,
    retain_native_value_decode_classified_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwChainStateV2, PalwMergedWorkV1, PalwNativeOriginalInputContextV19, PalwStateParamsV2,
    PalwStateV2Error,
};

enum RawAttemptV19 {
    Legacy(NativeRetainedV1<PalwAttemptEnvelopeV2>),
    Material(NativeRetainedV1<PalwAttemptMaterialEnvelopeV3>),
}
impl RawAttemptV19 {
    fn base(&self) -> &PalwAttemptEnvelopeV2 {
        match self {
            Self::Legacy(e) => e,
            Self::Material(e) => &e.base,
        }
    }
    fn decode(bytes: &[u8]) -> Result<Self, NativeDecodeErrorV1<String>> {
        if PalwAttemptMaterialEnvelopeV3::is_carriage(bytes) {
            let body = bytes
                .strip_prefix(&kaspa_consensus_core::palw_attempt_material_v3::PALW_ATTEMPT_MATERIAL_WIRE_MAGIC_V3)
                .expect("matched native magic");
            retain_native_value_decode_classified_v1(body, || PalwAttemptMaterialEnvelopeV3::decode_wire(bytes)).map(Self::Material)
        } else {
            let body = bytes
                .strip_prefix(&kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_WIRE_MAGIC)
                .ok_or_else(|| NativeDecodeErrorV1::Decode("candidate attempt magic".into()))?;
            retain_native_value_decode_classified_v1(body, || PalwAttemptEnvelopeV2::decode_wire(bytes).map_err(|e| e.to_string()))
                .map(Self::Legacy)
        }
    }
}
impl VirtualStateProcessor {
    /// Project work already resolved by the caller. Header/receipt validation
    /// and bootstrap agreement remain at the accepting caller.
    pub(super) fn palw_native_own_work_v1<'a>(
        &self,
        header: &Header,
        attempt: Option<&'a PalwAttemptEnvelopeV2>,
        receipt: Option<&'a PalwReceiptSpendEnvelopeV3>,
    ) -> (PalwBlockWorkV3<'a>, Hash64, Hash64) {
        if let Some(attempt) = attempt {
            let (execution_key, job_anchor) = self.palw_attempt_keys_v1(Some(header), &attempt.attempt);
            return (PalwBlockWorkV3::Attempt(attempt), execution_key, job_anchor);
        }
        let work = receipt.map_or(PalwBlockWorkV3::None, |receipt| PalwBlockWorkV3::ReceiptSpend(&receipt.spend));
        (work, Hash64::default(), Hash64::default())
    }

    /// The same merge-work projection supplies the accepting fold and the
    /// native input measurement. The caller keeps the allocation owner until
    /// the final reader of these borrowed work entries has finished.
    pub(super) fn palw_native_merged_work_refs_v1<'a>(
        &self,
        merged: &'a [PalwMergedOwnedWorkV1],
    ) -> Result<(Vec<PalwMergedWorkV1<'a>>, Option<NativeDemandLeaseV1>), PalwLocalTransitionFaultV1> {
        let path = "actual merged work reference array";
        let count = u64::try_from(merged.len()).map_err(|_| NativeCoverageV1::overflow::<PalwMergedWorkV1<'_>>(path))?;
        let bytes = count
            .checked_mul(std::mem::size_of::<PalwMergedWorkV1<'_>>() as u64)
            .ok_or_else(|| NativeCoverageV1::overflow::<PalwMergedWorkV1<'_>>(path))?;
        let owner = reserve_native_requests_v1(bytes)?;
        let mut refs = Vec::new();
        let exhausted = |why| NativeCoverageV1::unsupported::<PalwMergedWorkV1<'_>>(path, why);
        refs.try_reserve_exact(merged.len()).map_err(|_| exhausted("work reference allocation failed"))?;
        if refs.capacity() > merged.len() {
            return Err(exhausted("work reference allocation exceeds its reservation").into());
        }
        refs.extend(merged.iter().map(|m| match m {
            PalwMergedOwnedWorkV1::Attempt(blue, e, subsidy, carve, bits, origin) => {
                self.palw_merged_attempt_work_v1(*blue, Some(origin.as_ref()), e, *subsidy, *carve, *bits)
            }
            PalwMergedOwnedWorkV1::Spend(blue, e) => PalwMergedWorkV1 {
                carrying_block: *blue,
                work: PalwBlockWorkV3::ReceiptSpend(&e.spend),
                execution_key: Default::default(),
                subsidy: 0,
                escrow_carve: None,
                bits: 0,
                job_anchor: Default::default(),
            },
        }));
        Ok((refs, owner))
    }

    #[allow(clippy::type_complexity, clippy::too_many_arguments)]
    pub(super) fn palw_native_original_input_context_for_header_v19<'a>(
        &self,
        parent: &'a PalwChainStateV2,
        params: &PalwStateParamsV2,
        point: &PalwBlockContextV2,
        header: &Header,
        candidates: &[PalwCarriedObjectV1],
        merged: &[PalwMergedOwnedWorkV1],
    ) -> Result<
        (Option<NativeRetainedV1<PalwNativeOriginalInputContextV19<'a>>>, Result<Option<PalwReceiptSpendEnvelopeV3>, String>),
        PalwLocalTransitionFaultV1,
    > {
        let receipt = self.palw_v2_check_receipt_spend(header, parent, params, point);
        let raw = if kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
            match RawAttemptV19::decode(&header.palw_commitment) {
                Ok(work) => Some(work),
                Err(NativeDecodeErrorV1::Resource(e)) => return Err(e.into()),
                Err(NativeDecodeErrorV1::Decode(e)) => {
                    info!("Block {}: original input context unavailable for malformed actual work: {e}", point.block);
                    return Ok((None, receipt));
                }
            }
        } else {
            None
        };
        // A failed receipt check is NOT no work. Keep its existing downstream
        // verdict and withhold this measurement instead of fabricating None.
        let actual_receipt = match &receipt {
            Ok(s) => s,
            Err(_) => return Ok((None, receipt)),
        };
        let (work, key, _) = self.palw_native_own_work_v1(header, raw.as_ref().map(RawAttemptV19::base), actual_receipt.as_ref());
        let (refs, _refs_owner) = self.palw_native_merged_work_refs_v1(merged)?;
        let context = kaspa_consensus_core::palw_state_v2::palw_native_original_input_context_v19(
            parent,
            point,
            candidates.iter().map(|c| &c.object),
            work,
            &refs,
            key,
        )
        .map_err(|e| match e {
            PalwStateV2Error::NativeResource(r) => PalwLocalTransitionFaultV1::Resource(r),
            PalwStateV2Error::NativeContextUnavailable(c) => PalwLocalTransitionFaultV1::Context(c),
            other => {
                warn!("actual candidate input measurement failed: {other}");
                PalwLocalTransitionFaultV1::Context(
                    kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::StoreUnreadable,
                )
            }
        })?;
        Ok((Some(context), receipt))
    }
}
