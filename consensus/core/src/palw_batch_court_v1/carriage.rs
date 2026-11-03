//! The bounded, network-bound opening of a previously reserved Batch demand.
use super::BatchDemandV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

pub use crate::palw_court_shared_tir_builder_store_v1::constraint_stop::{
    BatchStopSigningV1, batch_payload::MAX_BATCH_STOP_EOG_OPENING_BYTES_V1,
};

const SIG: usize = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
pub const BATCH_DEMAND_SIGNATURE_CONTEXT_V1: &[u8] = b"misaka-palw/batch-demand/v1/mldsa87";

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct SignedBatchDemandV1 {
    pub version: u16,
    pub claim: Hash64,
    pub scope_id: Hash64,
    pub ordinal: u64,
    pub notice_id: Hash64,
    pub signer: PalwBondKeyV2,
    pub demand: BatchDemandV1,
    pub signature: Vec<u8>,
}

/// Unsigned Open for a rooted original and an accepted notice. The caller
/// signs the result using the notice issuer's key and the real network domain.
/// The quote checks reserved capacity; the accepting fold checks live authority.
pub fn quote_batch_open_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    claim: Hash64,
    notice_id: Hash64,
    ordinal: u64,
    ranges: Vec<super::BatchRangeV1>,
) -> Result<SignedBatchDemandV1, String> {
    super::rooted::quote_open_v1(state, claim, notice_id, ordinal, ranges)
}

impl SignedBatchDemandV1 {
    pub fn signing_message_v1(&self, network: Hash64) -> Result<Hash64, String> {
        if self.version != 1 || [network, self.claim, self.scope_id, self.notice_id].contains(&Hash64::default()) {
            return Err("Batch demand version or signing scope".into());
        }
        let request_root = self.demand.checked_request_root_v1().map_err(|err| format!("Batch demand: {err:?}"))?;
        crate::palw_court_shared_tir_stream_v1::stream_digest(
            b"misaka-palw/batch-demand-signing/v1",
            &(network, self.version, self.claim, self.scope_id, self.ordinal, self.notice_id, self.signer, request_root),
        )
    }

    pub(crate) fn verify_v1(&self, network: Hash64, pubkey: &[u8]) -> Result<(), String> {
        if self.signature.len() != SIG
            || !crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                pubkey,
                self.signing_message_v1(network)?.as_byte_slice(),
                &self.signature,
                BATCH_DEMAND_SIGNATURE_CONTEXT_V1,
            )
        {
            return Err("invalid Batch demand signature".into());
        }
        Ok(())
    }
}

impl BorshDeserialize for SignedBatchDemandV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(r)?;
        if version != 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown Batch demand version"));
        }
        let claim = Hash64::deserialize_reader(r)?;
        let scope_id = Hash64::deserialize_reader(r)?;
        let ordinal = u64::deserialize_reader(r)?;
        let notice_id = Hash64::deserialize_reader(r)?;
        let signer = PalwBondKeyV2::deserialize_reader(r)?;
        let demand = BatchDemandV1::deserialize_reader(r)?;
        let signature = read_batch_signature_v1(r)?;
        Ok(Self { version, claim, scope_id, ordinal, notice_id, signer, demand, signature })
    }
}

/// Separate candidate payload for a native Batch dispatcher. Every variable
/// field is bounded at decode before allocation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum BatchCourtWireV1 {
    Open(SignedBatchDemandV1),
    Respond(BatchMaterialControlV1),
    Stop(BatchStopControlV1),
}

/// The signed Stop operands are framed before the original-specific EOG
/// size and native terminal slot are resolved.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct BatchStopControlV1 {
    pub operands: Vec<u8>,
}

impl BorshDeserialize for BatchStopControlV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        use crate::palw_court_shared_tir_builder_store_v1::constraint_stop::batch_payload::{
            MAX_BATCH_STOP_OPERANDS_BYTES_V1, check_batch_stop_operands_v1,
        };
        let operands = crate::palw_public_binding_v1::read_bounded_bytes_v1(r, MAX_BATCH_STOP_OPERANDS_BYTES_V1)?;
        check_batch_stop_operands_v1(&operands)?;
        Ok(Self { operands })
    }
}

impl crate::palw_native_resource_v1::wire::NativeWireScanV1 for BatchStopControlV1 {
    fn scan_native_wire_v1<R: Read>(
        reader: &mut crate::palw_native_resource_v1::wire::NativeWireReaderV1<R>,
    ) -> io::Result<crate::palw_native_resource_v1::wire::NativeWireShapeV1> {
        use crate::palw_court_shared_tir_builder_store_v1::constraint_stop::batch_payload::MAX_BATCH_STOP_OPERANDS_BYTES_V1;
        use crate::palw_native_resource_v1::wire::{NativeWireShapeV1, wire_bad};
        reader.step()?;
        let bytes = u64::from(reader.word()?);
        if bytes > MAX_BATCH_STOP_OPERANDS_BYTES_V1 as u64 {
            return Err(wire_bad("retained Batch Stop exceeds the operand bound"));
        }
        reader.skip(bytes)?;
        NativeWireShapeV1 { byte_entries: bytes, ..Default::default() }.allocation(bytes, 1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct BatchMaterialControlV1 {
    pub demand_ordinal: u64,
    pub intake: crate::palw_court_shared_material_intake_v1::SignedMaterialIntakeV1,
}

impl BorshDeserialize for BatchMaterialControlV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        use crate::palw_court_shared_material_intake_v1::{
            MATERIAL_BATCH_VERSION, MATERIAL_PIPELINED_MAX_BYTES, MaterialActionV1 as Action, MaterialTargetV1, SignedMaterialIntakeV1,
        };
        let invalid = |message| io::Error::new(io::ErrorKind::InvalidData, message);
        let demand_ordinal = u64::deserialize_reader(r)?;
        let version = u16::deserialize_reader(r)?;
        if version != MATERIAL_BATCH_VERSION {
            return Err(invalid("Batch response version"));
        }
        let claim = Hash64::deserialize_reader(r)?;
        let scope_id = Hash64::deserialize_reader(r)?;
        let target = MaterialTargetV1::deserialize_reader(r)?;
        let submitter = PalwBondKeyV2::deserialize_reader(r)?;
        let action = match u8::deserialize_reader(r)? {
            0 => {
                let total_bytes = u64::deserialize_reader(r)?;
                if !(1..=MATERIAL_PIPELINED_MAX_BYTES).contains(&total_bytes) {
                    return Err(invalid("Batch response declared byte cap"));
                }
                Action::Begin { total_bytes, body_hash: Hash64::deserialize_reader(r)? }
            }
            1 => {
                let draft_id = Hash64::deserialize_reader(r)?;
                let offset = u64::deserialize_reader(r)?;
                let bytes = crate::palw_public_binding_v1::read_bounded_bytes_v1(
                    r,
                    crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1,
                )?;
                if bytes.is_empty() {
                    return Err(invalid("Batch response part is empty"));
                }
                Action::Part { draft_id, offset, bytes }
            }
            2 => Action::Finish { draft_id: Hash64::deserialize_reader(r)? },
            _ => return Err(invalid("unknown Batch response action")),
        };
        let signature = read_batch_signature_v1(r)?;
        Ok(Self { demand_ordinal, intake: SignedMaterialIntakeV1 { version, claim, scope_id, target, submitter, action, signature } })
    }
}

fn read_batch_signature_v1(r: &mut impl Read) -> io::Result<Vec<u8>> {
    let length = u32::deserialize_reader(r)? as usize;
    if length != SIG {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Batch signature length"));
    }
    let mut signature = Vec::new();
    signature.try_reserve_exact(SIG).map_err(|_| io::Error::new(io::ErrorKind::OutOfMemory, "Batch signature allocation failed"))?;
    signature.resize(SIG, 0);
    r.read_exact(&mut signature)?;
    Ok(signature)
}

/// Distinct lifecycle payload prefix for the candidate Batch dispatcher.
pub const BATCH_COURT_CARRIER_MAGIC_V1: &[u8; 4] = b"BCT1";
pub(crate) const BATCH_STOP_WIRE_OVERHEAD_V1: u64 = BATCH_COURT_CARRIER_MAGIC_V1.len() as u64 + 1 + 4;
pub(crate) const BATCH_OPEN_WIRE_OVERHEAD_V1: u64 = BATCH_COURT_CARRIER_MAGIC_V1.len() as u64 + 1;
pub(crate) const BATCH_RESPONSE_WIRE_OVERHEAD_V1: u64 = BATCH_OPEN_WIRE_OVERHEAD_V1 + 8;

/// Canonical BCT1 opening size from its range count. Every field besides the
/// signature and ranges is fixed-width, and the native signature width is fixed.
pub(crate) fn batch_open_wire_bytes_v1(ranges: usize) -> Result<u64, String> {
    use super::{BatchCourtWindowV1, BatchRangeAuthenticationV1, BatchRangeV1, MAX_BATCH_RANGES_V1};
    if !(1..=MAX_BATCH_RANGES_V1).contains(&ranges) {
        return Err("Batch opening range count".into());
    }
    let zero = Hash64::default();
    let template = SignedBatchDemandV1 {
        version: 1,
        claim: zero,
        scope_id: zero,
        ordinal: 0,
        notice_id: zero,
        signer: PalwBondKeyV2(crate::tx::TransactionOutpoint::new(zero, 0)),
        demand: BatchDemandV1 {
            case_id: zero,
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 0,
                challenge_opened_daa: 0,
                collect_until_daa: 0,
                material_until_daa: 0,
            },
            opened_daa: 0,
            response_deadline_daa: 0,
            maximum_bytes: 0,
            part_bytes: 0,
            ranges: Vec::new(),
        },
        signature: Vec::new(),
    };
    let range = BatchRangeV1 { id: zero, bytes: 0, authentication: BatchRangeAuthenticationV1::WholeRange };
    let fixed = borsh::object_length(&template).map_err(|e| e.to_string())? as u64;
    let range = borsh::object_length(&range).map_err(|e| e.to_string())? as u64;
    Ok(fixed + BATCH_OPEN_WIRE_OVERHEAD_V1 + SIG as u64 + ranges as u64 * range)
}

pub fn decode_batch_court_carrier_v1(carrier: &crate::tx::Transaction) -> Result<Option<BatchCourtWireV1>, String> {
    if carrier.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
        return Ok(None);
    }
    decode_batch_court_payload_v1(&carrier.payload).map_err(|error| error.to_string())
}

/// Shape-only check used by fee projection and lifecycle admission, without
/// allocating a second copy of the signed Stop operands.
pub(crate) fn batch_stop_wire_shape_v1(payload: &[u8]) -> io::Result<bool> {
    let Some(operands) = batch_stop_wire_operands_v1(payload)? else { return Ok(false) };
    crate::palw_court_shared_tir_builder_store_v1::constraint_stop::batch_payload::check_batch_stop_operands_v1(operands)?;
    Ok(true)
}

/// Borrow the length-delimited operand slice without decoding EOG material.
pub(crate) fn batch_stop_wire_operands_v1(payload: &[u8]) -> io::Result<Option<&[u8]>> {
    let Some(body) = payload.strip_prefix(BATCH_COURT_CARRIER_MAGIC_V1).and_then(|body| body.strip_prefix(&[2])) else {
        return Ok(None);
    };
    let mut operands = body;
    let count = u32::deserialize_reader(&mut operands)? as usize;
    if count != operands.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Batch Stop carrier length"));
    }
    Ok(Some(operands))
}

pub(crate) fn decode_batch_court_payload_v1(payload: &[u8]) -> io::Result<Option<BatchCourtWireV1>> {
    let Some(body) = payload.strip_prefix(BATCH_COURT_CARRIER_MAGIC_V1) else { return Ok(None) };
    borsh::from_slice(body).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_batch_court_v1::{BatchCourtWindowV1, BatchRangeAuthenticationV1, BatchRangeV1};
    use crate::palw_court_shared_material_intake_v1::{
        MATERIAL_BATCH_VERSION, MaterialActionV1, MaterialTargetV1, SignedMaterialIntakeV1,
    };
    use crate::tx::TransactionOutpoint;

    #[test]
    fn batch_open_signature_binds_notice_ordinal_network_and_request() {
        let h = Hash64::from_u64_word;
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([101; 32]);
        let mut open = SignedBatchDemandV1 {
            version: 1,
            claim: h(1),
            scope_id: h(2),
            ordinal: 7,
            notice_id: h(3),
            signer: PalwBondKeyV2(TransactionOutpoint::new(h(4), 0)),
            demand: BatchDemandV1 {
                case_id: h(5),
                case_window: BatchCourtWindowV1 {
                    claim_accepted_daa: 10,
                    challenge_opened_daa: 20,
                    collect_until_daa: 1_000,
                    material_until_daa: 11_000,
                },
                opened_daa: 30,
                response_deadline_daa: 40,
                maximum_bytes: 100,
                part_bytes: 16,
                ranges: vec![BatchRangeV1 { id: h(6), bytes: 99, authentication: BatchRangeAuthenticationV1::WholeRange }],
            },
            signature: Vec::new(),
        };
        open.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &keys.signing_key,
            open.signing_message_v1(h(8)).unwrap().as_byte_slice(),
            BATCH_DEMAND_SIGNATURE_CONTEXT_V1,
            [102; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
        let wire = BatchCourtWireV1::Open(open.clone());
        assert_eq!(borsh::from_slice::<BatchCourtWireV1>(&borsh::to_vec(&wire).unwrap()).unwrap(), wire);
        assert_eq!(
            batch_open_wire_bytes_v1(open.demand.ranges.len()).unwrap(),
            borsh::object_length(&wire).unwrap() as u64 + BATCH_COURT_CARRIER_MAGIC_V1.len() as u64
        );
        let vk = keys.verification_key.as_ref();
        assert!(open.verify_v1(h(8), vk).is_ok());
        assert!(open.verify_v1(h(9), vk).is_err());
        let mut changed = open.clone();
        changed.notice_id = h(11);
        assert!(changed.verify_v1(h(8), vk).is_err());
        changed = open.clone();
        changed.ordinal += 1;
        assert!(changed.verify_v1(h(8), vk).is_err());
        changed = open;
        changed.demand.ranges[0].bytes -= 1;
        assert!(changed.verify_v1(h(8), vk).is_err());
    }

    #[test]
    fn batch_response_wire_refuses_oversized_part_before_accepting_bytes() {
        let h = Hash64::from_u64_word;
        let response = BatchCourtWireV1::Respond(BatchMaterialControlV1 {
            demand_ordinal: 7,
            intake: SignedMaterialIntakeV1 {
                version: MATERIAL_BATCH_VERSION,
                claim: h(1),
                scope_id: h(2),
                target: MaterialTargetV1::PromptFlat,
                submitter: PalwBondKeyV2(TransactionOutpoint::new(h(3), 0)),
                action: MaterialActionV1::Part { draft_id: h(4), offset: 0, bytes: vec![7] },
                signature: vec![5; SIG],
            },
        });
        let mut body = borsh::to_vec(&response).unwrap();
        assert_eq!(borsh::from_slice::<BatchCourtWireV1>(&body).unwrap(), response);
        let length = body.len() - SIG - 4 - 1 - 4;
        body[length..length + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<BatchCourtWireV1>(&body).is_err());
    }

    #[test]
    fn stop_carrier_framing_borrows_exact_operand_bytes() {
        let mut carrier = BATCH_COURT_CARRIER_MAGIC_V1.to_vec();
        carrier.push(2);
        carrier.extend(3u32.to_le_bytes());
        carrier.extend([4, 5, 6]);
        let borrowed = batch_stop_wire_operands_v1(&carrier).unwrap().unwrap();
        assert_eq!(borrowed, [4, 5, 6]);
        assert_eq!(borrowed.as_ptr(), carrier[carrier.len() - 3..].as_ptr());
        assert!(batch_stop_wire_shape_v1(&carrier).is_err(), "framing alone does not validate operands");
        carrier.pop();
        assert!(batch_stop_wire_operands_v1(&carrier).is_err());
        carrier[4] = 1;
        assert!(batch_stop_wire_operands_v1(&carrier).unwrap().is_none());
    }
}
