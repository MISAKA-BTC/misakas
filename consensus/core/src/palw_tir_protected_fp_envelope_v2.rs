//! TFP2 is the literal signed FP and its TIR binding for protected TFO3.
//! Framing checks data identity. The protected funded owner authenticates the
//! signed FP, payer, companion, material, and native reservation separately.
use crate::{
    palw_fp_execution_v3::palw_fp_job_pin_v1,
    palw_freeprompt_v3::{PALW_FP_COMMITMENT_TX_MAX_BYTES, PalwFpCommitmentTxPayloadV3},
    palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1},
    palw_public_binding_v1::{PalwPublicExecutionBindingHeaderV1, PalwPublicTirBindingHeaderV1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    palw_tir_funded_original_envelope_v1::scan_original_fp_and_header_v1,
    palw_tir_public_fp_envelope_v1::{
        PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1, check_original_binding_v1, decode_original, match_original_object_v1,
    },
};
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{self, Read};

pub const MAGIC: &[u8; 4] = b"TFP2";
const VERSION: u16 = 1;
const MAX_BYTES: usize = MAGIC.len() + 2 + 4 + PALW_FP_COMMITMENT_TX_MAX_BYTES + PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirProtectedFpEnvelopeV2 {
    original_payload: Vec<u8>,
    header: PalwPublicTirBindingHeaderV1,
}

impl PalwTirProtectedFpEnvelopeV2 {
    pub fn new_v1(original_payload: Vec<u8>, header: PalwPublicTirBindingHeaderV1) -> Result<Self, String> {
        let envelope = Self { original_payload, header };
        envelope.check_original_fp_v1()?;
        Ok(envelope)
    }

    pub fn encode_v1(&self) -> Result<Vec<u8>, String> {
        let bytes =
            MAGIC.len() + 2 + 4 + self.original_payload.len() + borsh::object_length(&self.header).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        out.try_reserve_exact(bytes).map_err(|_| "TFP2 output allocation failed")?;
        out.extend_from_slice(MAGIC);
        VERSION.serialize(&mut out).map_err(|e| e.to_string())?;
        self.original_payload.serialize(&mut out).map_err(|e| e.to_string())?;
        self.header.serialize(&mut out).map_err(|e| e.to_string())?;
        Ok(out)
    }

    pub fn decode_v1(bytes: &[u8]) -> Result<Self, String> {
        Self::decode_with_fp_v1(bytes).map(|(envelope, _)| envelope)
    }

    pub(super) fn decode_with_fp_v1(bytes: &[u8]) -> Result<(Self, PalwFpCommitmentTxPayloadV3), String> {
        if bytes.len() > MAX_BYTES {
            return Err("TFP2 exceeds the original FP/header bounds".into());
        }
        let mut scan = NativeWireReaderV1::new(bytes, bytes.len() as u64);
        Self::scan_native_wire_v1(&mut scan).map_err(|e| e.to_string())?;
        let mut body = &bytes[MAGIC.len() + 2..];
        let original_payload = crate::palw_public_binding_v1::read_bounded_bytes_v1(&mut body, PALW_FP_COMMITMENT_TX_MAX_BYTES)
            .map_err(|e| e.to_string())?;
        let header = PalwPublicTirBindingHeaderV1::deserialize_reader(&mut body).map_err(|e| e.to_string())?;
        if !body.is_empty() {
            return Err("TFP2 has trailing bytes".into());
        }
        let envelope = Self { original_payload, header };
        let fp = envelope.check_original_fp_v1()?;
        Ok((envelope, fp))
    }

    pub fn original_payload_v1(&self) -> &[u8] {
        &self.original_payload
    }

    pub fn header_v1(&self) -> &PalwPublicTirBindingHeaderV1 {
        &self.header
    }

    pub(crate) fn into_header_v1(self) -> PalwPublicTirBindingHeaderV1 {
        self.header
    }

    pub fn original_fp_v1(&self) -> Result<PalwFpCommitmentTxPayloadV3, String> {
        decode_original(&self.original_payload)
    }

    fn check_original_fp_v1(&self) -> Result<PalwFpCommitmentTxPayloadV3, String> {
        if self.header.version != crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1
            || self.original_payload.is_empty()
            || self.original_payload.len() > PALW_FP_COMMITMENT_TX_MAX_BYTES
            || borsh::object_length(&self.header).map_err(|e| e.to_string())? > PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1
        {
            return Err("TFP2 exceeds the original FP/header bounds".into());
        }
        PalwPublicExecutionBindingHeaderV1::Tir(self.header.clone()).check_wire_v1()?;
        let payload = self.original_fp_v1()?;
        check_original_binding_v1(&payload, &self.header)?;
        Ok(payload)
    }

    pub fn matches_object_v1(&self, object: &PalwConsensusObjectV2) -> Result<(), String> {
        let payload = self.original_fp_v1()?;
        let commitment = &payload.commitment;
        match_original_object_v1(&payload, PalwBondKeyV2(commitment.job.executor_bond), palw_fp_job_pin_v1(commitment), object)
    }
}

impl NativeWireScanV1 for PalwTirProtectedFpEnvelopeV2 {
    fn scan_native_wire_v1<R: Read>(r: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        r.step()?;
        let mut magic = [0; 4];
        r.read_exact(&mut magic)?;
        if &magic != MAGIC || r.short()? != VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "inner TFP2 framing/version"));
        }
        let shape = scan_original_fp_and_header_v1(r)?;
        if r.remaining() != 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "inner TFP2 trailing bytes"));
        }
        Ok(shape)
    }
}
