//! Independent preparation and checking of the explicit non-reward v9 class.
//! Native admission authenticates its binding/bond and fixed resource policy.
//! These offline helpers grant no source fidelity, availability, rewards or Final.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_route_message_v1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
};
use kaspa_pq_validator_core::MessageSigner;
use misaka_palw_kernel::{
    hash::Digest,
    ledger::flat_candidate9::{
        FLAT_CANDIDATE_METADATA_MAX_BYTES_V9, FlatCandidatePlanV9, FlatCandidateRecordV9, FlatCandidateViewV9, decode_replay_v9,
        decode_scope_v9, derive_flat_candidate_v9,
    },
    route::KernelRouteObjectV1 as K,
    trace::ParamCommitmentsV1,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::Path,
};

/// The route carrier bounds the program/PC part; generated complete metadata
/// has its separate existing 8MiB cap and a fixed-size bounded replay record.
pub const FLAT_CANDIDATE_RECORD_MAX_BYTES_V9: usize = FLAT_CANDIDATE_METADATA_MAX_BYTES_V9 + 1_583_616 + 2048;
fn registration_extent(program_bytes: usize, params: &ParamCommitmentsV1) -> Option<usize> {
    // Existing route version/tag, binding/descriptor, program+PC lengths,
    // maximum positions and scope/replay/resource roots. Each PC map key has
    // its u16 index and option byte, plus two bytes only when layer is Some.
    let fixed: usize = 2 + 2 * 64 + 4 + 4 + 4 + 3 * 64;
    params
        .by_instance
        .keys()
        .try_fold(fixed.checked_add(program_bytes)?, |n, (_, layer)| n.checked_add(67 + 2 * usize::from(layer.is_some())))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedFlatCandidateV9 {
    pub class: Digest,
    pub plan: FlatCandidatePlanV9,
    pub record: FlatCandidateRecordV9,
    pub object: K,
}
pub fn prepare_flat_candidate_v9(
    binding: Digest,
    descriptor: Digest,
    program_bytes: &[u8],
    params: &ParamCommitmentsV1,
    max_positions: u32,
) -> Result<PreparedFlatCandidateV9, String> {
    if program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES || params.by_instance.len() > 1_583_616 / 67 {
        return Err("flat-v9 preparation exceeds bounded program/parameter carrier".into());
    }
    if registration_extent(program_bytes.len(), params).is_none_or(|n| n > 1_583_616) {
        return Err("flat-v9 program and parameter map exceed registration carrier".into());
    }
    let view = derive_flat_candidate_v9(&descriptor, program_bytes, params, max_positions)?;
    let scope_bytes = borsh::to_vec(&view.scope).map_err(|e| e.to_string())?;
    let replay_bytes = borsh::to_vec(&view.replay).map_err(|e| e.to_string())?;
    if scope_bytes.len() > FLAT_CANDIDATE_METADATA_MAX_BYTES_V9 || replay_bytes.len() > 1024 {
        return Err("flat-v9 generated metadata exceeds its codec bounds".into());
    }
    let record = FlatCandidateRecordV9 {
        binding,
        plan: view.plan.clone(),
        program_bytes: program_bytes.to_vec(),
        param_commitments: params.clone(),
        scope_bytes,
        replay_bytes,
    };
    let object = K::RegisterFlatModelCandidateV9 {
        binding,
        descriptor,
        program_bytes: program_bytes.to_vec(),
        param_commitments: params.clone(),
        max_positions,
        scope_root: view.plan.scope_root,
        replay_root: view.plan.replay_root,
        resource_root: view.plan.resource_root,
    };
    if object.encoded_len() > object.max_encoded_bytes() {
        return Err("flat-v9 candidate exceeds registration tag48 carrier".into());
    }
    Ok(PreparedFlatCandidateV9 { class: record.id(), plan: view.plan, record, object })
}

/// Decode fixed headers and bounded nested byte/map counts before allocation.
/// A matching supplied class id proves consistency only; authenticate the class
/// and its binding independently from the locally verified chain state.
pub fn decode_flat_candidate_record_v9(bytes: &[u8], expected_class: &Digest) -> Result<FlatCandidateRecordV9, String> {
    if bytes.len() > FLAT_CANDIDATE_RECORD_MAX_BYTES_V9 {
        return Err("flat-v9 stored record exceeds its fixed byte bound".into());
    }
    fn read<T: BorshDeserialize>(reader: &mut Cursor<&[u8]>) -> Result<T, String> {
        T::deserialize_reader(reader).map_err(|e| format!("flat-v9 record: {e}"))
    }
    fn read_bytes(reader: &mut Cursor<&[u8]>, maximum: usize) -> Result<Vec<u8>, String> {
        let count: u32 = read(reader)?;
        let offset = reader.position() as usize;
        if count as usize > maximum || count as usize > reader.get_ref().len().saturating_sub(offset) {
            return Err("flat-v9 record nested byte count exceeds bounded available data".into());
        }
        let end = offset.checked_add(count as usize).ok_or("flat-v9 record byte offset overflow")?;
        let out = reader.get_ref()[offset..end].to_vec();
        reader.set_position(end as u64);
        Ok(out)
    }
    let mut reader = Cursor::new(bytes);
    let binding = read(&mut reader)?;
    let plan: FlatCandidatePlanV9 = read(&mut reader)?;
    if plan.version != 9 {
        return Err("flat-v9 stored plan has wrong version".into());
    }
    let program_bytes = read_bytes(&mut reader, misaka_palw_tir::program::MAX_PROGRAM_BYTES)?;
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&program_bytes).map_err(|e| e.to_string())?;
    let real = if plan.descriptor == misaka_palw_kernel::descriptor::k2_tir_v5_descriptor().digest() {
        misaka_palw_kernel::seg_encoder::encoder_binding_v1(&program)?.first_input as usize
    } else if plan.descriptor == misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest() {
        program.params.len()
    } else {
        return Err("flat-v9 stored role is unknown".into());
    };
    let declared = program.params[..real]
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
        .ok_or("flat-v9 stored namespace count overflow")?;
    let count: u32 = read(&mut reader)?;
    if count as usize != declared
        || count as usize > 1_583_616 / 67
        || count as usize > reader.get_ref().len().saturating_sub(reader.position() as usize) / 67
    {
        return Err("flat-v9 stored parameter count differs from bounded declaration".into());
    }
    let mut by_instance = BTreeMap::new();
    let mut previous = None;
    for _ in 0..count {
        let key: (u16, Option<u16>) = read(&mut reader)?;
        if previous.is_some_and(|old| old >= key) {
            return Err("flat-v9 stored parameter map is noncanonical".into());
        }
        let declaration = program
            .params
            .get(key.0 as usize)
            .filter(|_| (key.0 as usize) < real)
            .ok_or("flat-v9 stored parameter index is absent")?;
        if declaration.per_layer != key.1.is_some() || key.1.is_some_and(|l| l as usize >= program.schedule.layers.len()) {
            return Err("flat-v9 stored parameter layer differs from declaration".into());
        }
        previous = Some(key);
        by_instance.insert(key, read(&mut reader)?);
    }
    let scope_bytes = read_bytes(&mut reader, FLAT_CANDIDATE_METADATA_MAX_BYTES_V9)?;
    let replay_bytes = read_bytes(&mut reader, 1024)?;
    if reader.position() != reader.get_ref().len() as u64 {
        return Err("flat-v9 stored record has trailing data".into());
    }
    let record = FlatCandidateRecordV9 {
        binding,
        plan,
        program_bytes,
        param_commitments: ParamCommitmentsV1 { by_instance },
        scope_bytes,
        replay_bytes,
    };
    check_flat_candidate_record_v9(&record, expected_class)?;
    Ok(record)
}

pub fn check_flat_candidate_record_v9(record: &FlatCandidateRecordV9, expected_class: &Digest) -> Result<FlatCandidateViewV9, String> {
    if record.program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || record.param_commitments.by_instance.len() > 1_583_616 / 67
        || record.scope_bytes.len() > FLAT_CANDIDATE_METADATA_MAX_BYTES_V9
        || record.replay_bytes.len() > 1024
    {
        return Err("flat-v9 supplied class or record extent differs".into());
    }
    if registration_extent(record.program_bytes.len(), &record.param_commitments).is_none_or(|n| n > 1_583_616) {
        return Err("flat-v9 stored program/parameter carrier exceeds admission".into());
    }
    if record.id() != *expected_class {
        return Err("flat-v9 supplied class identity differs".into());
    }
    // Check nested grammar bounds first, then independently derive the complete
    // ordinary/TopK scope, resource vector, replay and native restore tariff.
    decode_scope_v9(&record.scope_bytes)?;
    decode_replay_v9(&record.replay_bytes)?;
    record.checked()
}

pub fn encode_flat_candidate_record_v9(record: &FlatCandidateRecordV9, expected_class: &Digest) -> Result<Vec<u8>, String> {
    check_flat_candidate_record_v9(record, expected_class)?;
    struct BoundedWriter(Vec<u8>);
    impl Write for BoundedWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > FLAT_CANDIDATE_RECORD_MAX_BYTES_V9.saturating_sub(self.0.len()) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "flat-v9 serialized record exceeds fixed byte bound",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = BoundedWriter(Vec::new());
    record.serialize(&mut writer).map_err(|e| e.to_string())?;
    Ok(writer.0)
}

/// Bound the read even when a concurrently replaced file grows after metadata.
pub fn read_flat_candidate_record_v9(path: &Path, expected_class: &Digest) -> Result<FlatCandidateRecordV9, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > FLAT_CANDIDATE_RECORD_MAX_BYTES_V9 as u64 {
        return Err("flat-v9 record file exceeds its fixed byte bound".into());
    }
    let mut bytes = Vec::new();
    file.take(FLAT_CANDIDATE_RECORD_MAX_BYTES_V9 as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    decode_flat_candidate_record_v9(&bytes, expected_class)
}

pub fn sign_flat_candidate_v9(
    key: &dyn MessageSigner,
    domain: Hash64,
    bond: PalwBondKeyV2,
    object: &K,
) -> Result<PalwConsensusObjectV2, String> {
    let K::RegisterFlatModelCandidateV9 { binding, descriptor, program_bytes, param_commitments, max_positions, .. } = object else {
        return Err("flat-v9 signer requires candidate registration tag48".into());
    };
    if prepare_flat_candidate_v9(*binding, *descriptor, program_bytes, param_commitments, *max_positions)?.object != *object {
        return Err("flat-v9 registration differs from independently derived scope/replay/resources".into());
    }
    let bytes = object.encode();
    K::decode(&bytes).map_err(|e| e.to_string())?;
    let message = palw_kernel_route_message_v1(domain, &bond, &bytes);
    let signature = key.sign_message(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)?;
    let signed = PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&signed).map_err(str::to_string)?;
    Ok(signed)
}
