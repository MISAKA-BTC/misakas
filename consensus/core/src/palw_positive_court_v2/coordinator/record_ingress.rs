//! Borrowed preflight of a WitnessCommit's A0 execution record, before any
//! candidate/attempt mutation. This checks framing and DTO cross-bindings only.
//! It does NOT authenticate the Panel, admit the case, reject a witness, establish
//! computation truth, or authorize credit. A1 must reconstruct the TIR binding
//! from independently resolved class/job data, verify all roots/relations and
//! derive the output from the generated IDs. No class/job encoding is added here.
use super::native_body::{MAX_OPAQUE_BYTES_V2, PositiveNativeBodyV2, PositiveNativeOperationV2};
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::SIGNATURE_BYTES,
    messages::{MinerResultCommit, PanelWitnessCommit},
    record::ExecutionRecord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordIngressErrorV2 {
    BodyVersion,
    WrongOperation,
    Budget,
    Contract(ContractError),
}

/// A DTO-level borrowed view, not a role, admission, computation or fault
/// capability. Private fields ensure construction runs the framing/binding
/// checks; there is deliberately no Deserialize or conversion to a Court fact.
#[derive(Clone, Copy, Debug)]
pub struct BindingCheckedRecordV2<'a> {
    rooted_result: &'a MinerResultCommit,
    witness: &'a PanelWitnessCommit,
    signature: &'a [u8; SIGNATURE_BYTES],
    bytes: &'a [u8],
    record: ExecutionRecord<'a>,
}
impl<'a> BindingCheckedRecordV2<'a> {
    pub fn rooted_result(&self) -> &'a MinerResultCommit {
        self.rooted_result
    }
    pub fn witness(&self) -> &'a PanelWitnessCommit {
        self.witness
    }
    /// Literal carried signature only. The caller must authenticate the actual
    /// rooted Panel/duty separately before attributing any later check failure.
    pub fn panel_signature(&self) -> &'a [u8; SIGNATURE_BYTES] {
        self.signature
    }
    pub fn record_bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn execution_record(&self) -> ExecutionRecord<'a> {
        self.record
    }
}

/// `rooted_result` must be the literal Result independently resolved from the
/// current admitted case, never an expectation copied from the submitted body.
/// The body is still treated as untrusted even if an outer decoder ran earlier:
/// its public DTO can be directly constructed, so versions are checked again.
///
/// Codec/binding errors refuse THIS delivery without consuming an attempt or
/// changing candidate/Miner status. Even successful preflight is not acceptance.
/// Step count, trace/step roots and token semantics remain A1's mandatory checks.
pub fn preflight_witness_record_v2<'a>(
    rooted_result: &'a MinerResultCommit,
    body: &'a PositiveNativeBodyV2,
    max_record_bytes: u64,
) -> Result<BindingCheckedRecordV2<'a>, RecordIngressErrorV2> {
    if body.version != CONTRACT_VERSION || body.revision != CONTRACT_REVISION {
        return Err(RecordIngressErrorV2::BodyVersion);
    }
    let PositiveNativeOperationV2::WitnessCommit { witness, panel_signature, record: bytes } = &body.operation else {
        return Err(RecordIngressErrorV2::WrongOperation);
    };
    if max_record_bytes == 0 || max_record_bytes > MAX_OPAQUE_BYTES_V2 || bytes.len() as u64 > max_record_bytes {
        return Err(RecordIngressErrorV2::Budget);
    }
    rooted_result.validate_shape().map_err(RecordIngressErrorV2::Contract)?;
    witness.validate_binding(rooted_result).map_err(RecordIngressErrorV2::Contract)?;
    let record = ExecutionRecord::decode(bytes, max_record_bytes, rooted_result.decode).map_err(RecordIngressErrorV2::Contract)?;
    record.validate_binding(rooted_result, witness).map_err(RecordIngressErrorV2::Contract)?;
    Ok(BindingCheckedRecordV2 { rooted_result, witness, signature: panel_signature, bytes, record })
}
