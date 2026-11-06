//! Actual candidate measurements for singleton acceptance. No paid/source or
//! accepted-work authority is constructed from the unadmitted own header.
use super::*;
use kaspa_consensus_core::palw_attempt_material_v3::PalwAttemptMaterialEnvelopeV3;
use kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2;
use kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, reserve_native_requests_v1, retain_native_value_decode_classified_v1,
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
        let work = match (raw.as_ref(), actual_receipt.as_ref()) {
            (Some(e), _) => PalwBlockWorkV3::Attempt(e.base()),
            (None, Some(s)) => PalwBlockWorkV3::ReceiptSpend(&s.spend),
            (None, None) => PalwBlockWorkV3::None,
        };
        let key = raw.as_ref().map(|e| self.palw_attempt_keys_v1(Some(header), &e.base().attempt).0).unwrap_or_default();
        let overflow = || NativeCoverageV1::overflow::<PalwMergedWorkV1<'_>>("actual merged work reference array");
        let count = u64::try_from(merged.len()).map_err(|_| overflow())?;
        let bytes = count.checked_mul(std::mem::size_of::<PalwMergedWorkV1<'_>>() as u64).ok_or_else(overflow)?;
        let _refs = reserve_native_requests_v1(bytes)?;
        let refs: Vec<_> = merged
            .iter()
            .map(|m| match m {
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
            })
            .collect();
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
