//! Bounded signed Stop operands for a prospective Batch terminal.
use super::SignedConstraintStopIntentV1;
use crate::{
    Hash64,
    palw_court_shared_material_intake_v1::batch::constraint_stop::STOP_INTENT_BYTES_V1,
    palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1,
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, NativeRetainedV1, reserve_native_pending_requests_v1,
    },
    palw_state_v2::PalwBondKeyV2,
    palw_step_leg::{PalwStepOpeningV1, step_leg_max_opening_siblings_v1},
    palw_tokenizer_rendering_v2::ROOTED_TOKENIZER_LIMITS_V1,
};
use borsh::BorshDeserialize;
use std::io;

type Error = NativeDecodeErrorV1<String>;
const INTENT_BYTES: usize = STOP_INTENT_BYTES_V1 as usize;
const STOP_REASON_OFFSET: usize = 2 + 5 * 64 + 4;
const EOG_MAX: usize = ROOTED_TOKENIZER_LIMITS_V1.eog_ids as usize;
const SIBLINGS_MAX: usize = step_leg_max_opening_siblings_v1(ROOTED_TOKENIZER_LIMITS_V1.vocab as u64 + 1);
pub(crate) const MAX_BATCH_STOP_OPERANDS_BYTES_V1: usize = INTENT_BYTES + 4 + EOG_MAX * 4 + 76 + SIBLINGS_MAX * 64 + 4;

/// Stateless framing only. The registered row supplies the exact EOG size
/// during native verification, after locating the actual original.
pub(crate) fn check_batch_stop_operands_v1(bytes: &[u8]) -> io::Result<(usize, usize)> {
    let bad = |reason| io::Error::new(io::ErrorKind::InvalidData, reason);
    let sig = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
    let word = |offset| bytes.get(offset..)?.get(..4)?.try_into().ok().map(u32::from_le_bytes);
    if bytes.len() > MAX_BATCH_STOP_OPERANDS_BYTES_V1
        || !bytes.starts_with(&1u16.to_le_bytes())
        || !matches!(bytes.get(STOP_REASON_OFFSET), Some(0 | 1))
        || word(INTENT_BYTES - sig - 4) != Some(sig as u32)
    {
        return Err(bad("Batch Stop operand version, stop reason or signature length"));
    }
    let eog_count = word(INTENT_BYTES)
        .filter(|&count| (1..=EOG_MAX).contains(&(count as usize)))
        .ok_or_else(|| bad("Batch Stop EOG count exceeds registration limit"))? as usize;
    let siblings_at = INTENT_BYTES + 4 + eog_count * 4 + 8 + 64;
    let siblings = word(siblings_at)
        .filter(|&count| count as usize <= SIBLINGS_MAX)
        .ok_or_else(|| bad("Batch Stop EOG opening is too deep"))? as usize;
    let renderings_at = siblings_at + 4 + siblings * 64;
    if renderings_at + 4 != bytes.len() || word(renderings_at) != Some(0) {
        return Err(bad("Batch Stop requires exact EOG-only material"));
    }
    Ok((eog_count, siblings))
}

pub(crate) fn batch_stop_identity_v1(bytes: &[u8]) -> io::Result<(Hash64, PalwBondKeyV2)> {
    check_batch_stop_operands_v1(bytes)?;
    let claim = Hash64::deserialize_reader(&mut &bytes[2..])?;
    let filer = PalwBondKeyV2::deserialize_reader(&mut &bytes[STOP_REASON_OFFSET + 1..])?;
    Ok((claim, filer))
}

pub(crate) struct BatchStopPayloadV1 {
    pub intent: SignedConstraintStopIntentV1,
    pub eog: PalwTokenTableEogOpeningV1,
}

impl BatchStopPayloadV1 {
    /// Decode the signed intent and EOG-only material. The expected EOG byte
    /// length comes from the accepted registration used to reserve this slot.
    /// The native Stop verifier must still authenticate these operands.
    pub(crate) fn decode_v1(bytes: &[u8], expected_eog_bytes: u64) -> Result<NativeRetainedV1<Self>, Error> {
        Self::decode_with_v1(bytes, expected_eog_bytes, reserve_native_pending_requests_v1)
    }

    fn decode_with_v1(
        bytes: &[u8],
        expected_eog_bytes: u64,
        reserve: impl FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    ) -> Result<NativeRetainedV1<Self>, Error> {
        use NativeDecodeErrorV1::{Decode, Resource};
        let sig = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        if expected_eog_bytes.checked_add(STOP_INTENT_BYTES_V1 + 4) != Some(bytes.len() as u64) {
            return Err(Decode("Batch Stop differs from the registered EOG operand length".into()));
        }
        let (eog_count, siblings) = check_batch_stop_operands_v1(bytes).map_err(|why| Decode(why.to_string()))?;
        let heap = sig + eog_count * std::mem::size_of::<u32>() + siblings * std::mem::size_of::<Hash64>();
        let lease = reserve(heap as u64).map_err(Resource)?;
        let value = Self::decode_reserved_v1(bytes, eog_count, siblings).map_err(|why| {
            if why.kind() == io::ErrorKind::OutOfMemory {
                Resource(NativeCoverageV1::unsupported::<Self>("Batch Stop decode", &why.to_string()))
            } else {
                Decode(why.to_string())
            }
        })?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
    }

    fn decode_reserved_v1(mut bytes: &[u8], eog_count: usize, siblings: usize) -> io::Result<Self> {
        let version = u16::deserialize_reader(&mut bytes)?;
        if version != 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Batch Stop intent version"));
        }
        let intent = SignedConstraintStopIntentV1 {
            version,
            claim: BorshDeserialize::deserialize_reader(&mut bytes)?,
            scope_id: BorshDeserialize::deserialize_reader(&mut bytes)?,
            policy_id: BorshDeserialize::deserialize_reader(&mut bytes)?,
            semantics_id: BorshDeserialize::deserialize_reader(&mut bytes)?,
            material_id: BorshDeserialize::deserialize_reader(&mut bytes)?,
            position: BorshDeserialize::deserialize_reader(&mut bytes)?,
            stop_reason: BorshDeserialize::deserialize_reader(&mut bytes)?,
            filer: BorshDeserialize::deserialize_reader(&mut bytes)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(&mut bytes)?,
        };
        let _count = u32::deserialize_reader(&mut bytes)?;
        let ids = read_vec(&mut bytes, eog_count)?;
        let leaf_index = u64::deserialize_reader(&mut bytes)?;
        let leaf_hash = Hash64::deserialize_reader(&mut bytes)?;
        let _siblings = u32::deserialize_reader(&mut bytes)?;
        let siblings = read_vec(&mut bytes, siblings)?;
        let _renderings = u32::deserialize_reader(&mut bytes)?;
        debug_assert!(bytes.is_empty(), "the preflight checks the complete operand length");
        Ok(Self { intent, eog: PalwTokenTableEogOpeningV1 { ids, opening: PalwStepOpeningV1 { leaf_index, leaf_hash, siblings } } })
    }
}

fn read_vec<T: BorshDeserialize>(bytes: &mut &[u8], count: usize) -> io::Result<Vec<T>> {
    let mut result = Vec::new();
    result.try_reserve_exact(count).map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "Batch Stop vector allocation refused"))?;
    for _ in 0..count {
        result.push(T::deserialize_reader(bytes)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{palw_freeprompt_v3::PalwFpStopReasonV3, tx::TransactionOutpoint};

    #[test]
    fn stop_payload_preflights_geometry_and_classifies_capacity() {
        use crate::palw_batch_court_v1::carriage::{
            BATCH_COURT_CARRIER_MAGIC_V1, BATCH_STOP_WIRE_OVERHEAD_V1, BatchCourtWireV1, BatchStopControlV1,
            batch_stop_wire_shape_v1, decode_batch_court_payload_v1,
        };
        let h = Hash64::from_u64_word;
        let sig = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
        let intent = SignedConstraintStopIntentV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            policy_id: h(3),
            semantics_id: h(4),
            material_id: h(5),
            position: 9,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            filer: PalwBondKeyV2(TransactionOutpoint::new(h(6), 1)),
            signature: vec![7; sig],
        };
        let eog = PalwTokenTableEogOpeningV1 {
            ids: vec![1, 2],
            opening: PalwStepOpeningV1 { leaf_index: 8, leaf_hash: h(9), siblings: vec![h(10)] },
        };
        let eog_bytes = borsh::object_length(&eog).unwrap() as u64;
        assert_eq!(borsh::object_length(&intent).unwrap() as u64, STOP_INTENT_BYTES_V1);
        let bytes = borsh::to_vec(&(&intent, &eog, Vec::<u8>::new())).unwrap();
        assert_eq!(batch_stop_identity_v1(&bytes).unwrap(), (intent.claim, intent.filer));
        let stop = BatchCourtWireV1::Stop(BatchStopControlV1 { operands: bytes.clone() });
        let mut carrier = BATCH_COURT_CARRIER_MAGIC_V1.to_vec();
        carrier.extend(borsh::to_vec(&stop).unwrap());
        assert_eq!(carrier[BATCH_COURT_CARRIER_MAGIC_V1.len()], 2);
        assert_eq!(carrier.len() as u64, bytes.len() as u64 + BATCH_STOP_WIRE_OVERHEAD_V1);
        assert!(batch_stop_wire_shape_v1(&carrier).unwrap());
        assert_eq!(decode_batch_court_payload_v1(&carrier).unwrap(), Some(stop));
        assert!(batch_stop_wire_shape_v1(&carrier[..carrier.len() - 1]).is_err());
        let tx = crate::tx::Transaction::new(
            0, vec![], vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE, 0, carrier,
        );
        let fee = crate::palw_court_shared_material_intake_v1::material_batch_carrier_fee_v1(&tx, 17).unwrap();
        assert_eq!((fee.withheld_storage_rent, fee.collectible_fee), (0, 17));
        let decoded = BatchStopPayloadV1::decode_with_v1(&bytes, eog_bytes, |heap| {
            assert_eq!(heap, sig as u64 + 8 + 64);
            Ok(None)
        })
        .unwrap();
        assert_eq!(decoded.eog, eog);
        assert_eq!(borsh::to_vec(&decoded.intent).unwrap(), borsh::to_vec(&intent).unwrap());
        let denied = BatchStopPayloadV1::decode_with_v1(&bytes, eog_bytes, |_| {
            Err(NativeCoverageV1::unsupported::<BatchStopPayloadV1>("test", "capacity refused"))
        });
        assert!(matches!(denied, Err(NativeDecodeErrorV1::Resource(_))));
        let intent_bytes = STOP_INTENT_BYTES_V1 as usize;
        for offset in [0, STOP_REASON_OFFSET, intent_bytes - sig - 4, intent_bytes, intent_bytes + 4 + 8 + 8 + 64, bytes.len() - 4] {
            let mut malformed = bytes.clone();
            malformed[offset] = u8::MAX;
            assert!(batch_stop_identity_v1(&malformed).is_err());
            assert!(matches!(
                BatchStopPayloadV1::decode_with_v1(&malformed, eog_bytes, |_| panic!("invalid shape reached reservation")),
                Err(NativeDecodeErrorV1::Decode(_))
            ));
        }
        assert!(matches!(
            BatchStopPayloadV1::decode_with_v1(&bytes[..bytes.len() - 1], eog_bytes, |_| panic!(
                "truncated shape reached reservation"
            )),
            Err(NativeDecodeErrorV1::Decode(_))
        ));
    }
}
