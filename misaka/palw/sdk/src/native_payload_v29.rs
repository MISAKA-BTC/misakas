//! Typed offline objects for new native Text29 / Stage28 actions. The native
//! fold authenticates public source/actor/nonce/fees and applies every money change.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_route_message_v1},
    palw_native_text_stop_v29::*,
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
};
use kaspa_pq_validator_core::MessageSigner;
use misaka_palw_kernel::{hash::Digest, route::KernelRouteObjectV1 as K, trace::ParamCommitmentsV1};

pub struct PreparedNativePayloadV29 {
    object: K,
}
impl PreparedNativePayloadV29 {
    pub fn object(&self) -> &K {
        &self.object
    }
    pub fn bytes(&self) -> Vec<u8> {
        self.object.encode()
    }
}
fn text<T: BorshSerialize>(action: u8, body: &T) -> Result<PreparedNativePayloadV29, String> {
    let body = borsh::to_vec(body).map_err(|e| e.to_string())?;
    let object = K::FundedExecutionV11 { action, body };
    if object.encoded_len() > object.max_encoded_bytes()
        && !(action == 27 && matches!(&object, K::FundedExecutionV11 { body, .. } if body.len() <= 16 << 20))
    {
        return Err("text29 exceeds explicitly selected native carrier".into());
    }
    Ok(PreparedNativePayloadV29 { object })
}
fn stage<T: BorshSerialize>(action: u8, body: &T) -> Result<PreparedNativePayloadV29, String> {
    let body = borsh::to_vec(body).map_err(|e| e.to_string())?;
    let object = K::StageExecutionV14 { action, body };
    if object.encoded_len() > object.max_encoded_bytes()
        && !(matches!(action, 19 | 20) && matches!(&object, K::StageExecutionV14 { body, .. } if body.len() <= 16 << 20))
    {
        return Err("stage28 exceeds explicitly selected native carrier; original source must use its dedicated paged path".into());
    }
    Ok(PreparedNativePayloadV29 { object })
}
pub fn prepare_text_registration_v29(record: &NativeTextRegisterV29) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 29
        || record.program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
        || record.source_binding == [0; 64]
        || record.original_source_components == [0; 64]
    {
        return Err("text29 original registration fields".into());
    }
    let p = misaka_palw_tir::TirProgramV1::decode_canonical(&record.program_bytes).map_err(|e| e.to_string())?;
    misaka_palw_kernel::flat_resources29::derive_flat_resources_v29(&p, &record.real_parameters, record.maximum_positions, None)?;
    text(27, record)
}
pub fn prepare_text_job_v29(
    record: &NativeTextPrepareV29,
    program: &misaka_palw_tir::TirProgramV1,
) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 29 {
        return Err("text29 prepare namespace".into());
    }
    record.request.check_prompt(program, &record.prompt)?;
    text(28, record)
}
pub fn prepare_text_consent_v29(
    record: &NativeTextConsentV29,
    request: &NativeTextRequestV29,
    maximum: &CheckedNativeTextMaximumV29,
) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 29
        || record.request != request.root()
        || record.maximum != *maximum.record()
        || record.nonce != request.nonce
        || record.producer != request.producer
        || record.requester != request.requester
        || record.producer_operator != request.producer_operator
        || record.requester_operator != request.requester_operator
        || record.accepted_daa >= record.expires_daa
        || [record.preparation, record.public_protocol, record.queue_policy, record.queue_plan].contains(&[0; 64])
    {
        return Err("text29 consent changes original actors/maxima/clock/queue/fees".into());
    }
    text(29, record)
}
pub fn prepare_text_funding_v29(
    record: &NativeTextFundV29,
    consent: &NativeTextConsentV29,
) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 29 || record.preparation != consent.preparation || record.initial_consent != consent.root() {
        return Err("text29 Fund changes initial producer consent".into());
    }
    text(30, record)
}
pub fn prepare_text_post_v29(
    record: &NativeTextPostV29,
    program: &misaka_palw_tir::TirProgramV1,
    parameters: &ParamCommitmentsV1,
    request: &NativeTextRequestV29,
    maximum: &CheckedNativeTextMaximumV29,
    original_funding: Digest,
    job: Digest,
) -> Result<PreparedNativePayloadV29, String> {
    let actual = maximum.check_actual(program, parameters, request, &record.generated)?;
    if record.version != 29
        || record.job != job
        || record.original_funding != original_funding
        || record.maximum_contract != maximum.record().root()
        || record.requester_nonce != request.nonce
        || record.actual_scope25 != actual.scope_root()
        || record.actual_resource29 != actual.resource_root()
        || record.evidence.positions != actual.positions()
        || record.final_state == [0; 64]
    {
        return Err("text29 Commit changes original funding/job/actual stop scope".into());
    }
    record.evidence.check_structure(&record.segment_roots)?;
    text(31, record)
}
pub fn prepare_stage_bundle_v28(
    bundle: &kaspa_consensus_core::palw_flat_stage_v28::NativeStageSourceBundleV28,
) -> Result<PreparedNativePayloadV29, String> {
    bundle.check()?;
    stage(19, bundle)
}
pub fn prepare_stage_class_v28(
    class: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageClassV28,
    bundle: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageBundleV28,
) -> Result<PreparedNativePayloadV29, String> {
    class.check_component_manifest(bundle.manifest())?;
    if class.wire().complete_source != bundle.root() {
        return Err("stage28 class changes complete original source bundle".into());
    }
    stage(20, class.wire())
}
pub fn prepare_stage_acceptance_v28(
    record: &kaspa_consensus_core::palw_flat_stage_v28::StageAcceptSourceV28,
) -> Result<PreparedNativePayloadV29, String> {
    let raw = borsh::to_vec(record).map_err(|e| e.to_string())?;
    kaspa_consensus_core::palw_flat_stage_v28::StageAcceptSourceV28::from_bytes(&raw)?;
    stage(21, record)
}
pub fn prepare_stage_funding_v28(
    record: &kaspa_consensus_core::palw_flat_stage_v14::StageFundRequestV14,
    class: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageClassV28,
) -> Result<PreparedNativePayloadV29, String> {
    let raw = borsh::to_vec(record).map_err(|e| e.to_string())?;
    kaspa_consensus_core::palw_flat_stage_v14::StageFundRequestV14::from_bytes(&raw)?;
    let job = misaka_palw_kernel::flat_inputs12::bounded14::FlatStageJobV14::from_bytes(&record.job_bytes)?;
    if record.class != class.root() || record.acceptance == [0; 64] || record.reward == 0 {
        return Err("stage28 Fund changes class or lacks initial producer acceptance".into());
    }
    class.check_job(&job)?;
    stage(22, record)
}
pub fn prepare_stage_post_v28(
    trace: &misaka_palw_kernel::flat_quota11::stage14::StageTraceV14,
    class: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageClassV28,
    job: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageJobV28,
    job_id: Digest,
    quota: Digest,
) -> Result<PreparedNativePayloadV29, String> {
    let raw = borsh::to_vec(trace).map_err(|e| e.to_string())?;
    misaka_palw_kernel::flat_quota11::stage14::StageTraceV14::from_bytes(&raw, job.job().positions)?;
    if trace.class != class.root()
        || trace.job_id != job_id
        || trace.job_input != job.job().root()
        || trace.admission != job.admission().root()
        || trace.quota != quota
    {
        return Err("stage28 Commit changes original scoped job/admission/quota".into());
    }
    stage(23, trace)
}
pub fn sign_native_payload_v29(
    key: &dyn MessageSigner,
    domain: Hash64,
    bond: PalwBondKeyV2,
    payload: &PreparedNativePayloadV29,
) -> Result<PalwConsensusObjectV2, String> {
    let bytes = payload.bytes();
    K::decode(&bytes).map_err(|e| e.to_string())?;
    let message = palw_kernel_route_message_v1(domain, &bond, &bytes);
    let signature = key.sign_message(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)?;
    let object = PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(str::to_string)?;
    Ok(object)
}

/// Bounded offline transport preparation. Deep actor/source/maximum validation
/// uses the typed builders above and is repeated by native consensus. Dynamic
/// text lengths and real PC namespaces are rejected before allocation here.
pub fn decode_native_payload_v29(
    kind: &str,
    raw: &[u8],
    program: Option<&misaka_palw_tir::TirProgramV1>,
    request: Option<&NativeTextRequestV29>,
    positions: Option<u32>,
) -> Result<PreparedNativePayloadV29, String> {
    use std::io::Cursor;
    fn read<T: BorshDeserialize>(r: &mut Cursor<&[u8]>) -> Result<T, String> {
        T::deserialize_reader(r).map_err(|e| e.to_string())
    }
    fn vector<T: BorshDeserialize>(r: &mut Cursor<&[u8]>, max: usize, minimum: usize) -> Result<Vec<T>, String> {
        let n: u32 = read(r)?;
        let n = n as usize;
        if n > max || n > r.get_ref().len().saturating_sub(r.position() as usize) / minimum.max(1) {
            return Err("native29 nested count exceeds bounded available bytes".into());
        }
        let mut v = Vec::new();
        v.try_reserve_exact(n).map_err(|_| "native29 allocation")?;
        for _ in 0..n {
            v.push(read(r)?);
        }
        Ok(v)
    }
    if raw.len() > 16 << 20 {
        return Err("new native payload hard byte cap".into());
    }
    match kind {
        "text-register29" => {
            let mut r = Cursor::new(raw);
            let version = read(&mut r)?;
            let source_binding = read(&mut r)?;
            let original_source_components = read(&mut r)?;
            let maximum_positions = read(&mut r)?;
            let program_bytes: Vec<u8> = vector(&mut r, misaka_palw_tir::program::MAX_PROGRAM_BYTES, 1)?;
            let p = misaka_palw_tir::TirProgramV1::decode_canonical(&program_bytes).map_err(|e| e.to_string())?;
            let n: u32 = read(&mut r)?;
            let want = p
                .params
                .iter()
                .try_fold(0usize, |n, d| n.checked_add(if d.per_layer { p.schedule.layers.len() } else { 1 }))
                .ok_or("realPC namespace overflow")?;
            if n as usize != want || want > 16384 || want > raw.len().saturating_sub(r.position() as usize) / 67 {
                return Err("text29 realPC original count".into());
            }
            let mut real_parameters = ParamCommitmentsV1::default();
            let mut previous = None;
            for _ in 0..n {
                let k: (u16, Option<u16>) = read(&mut r)?;
                if previous.is_some_and(|v| v >= k) {
                    return Err("text29 noncanonical realPC".into());
                }
                previous = Some(k);
                real_parameters.by_instance.insert(k, read(&mut r)?);
            }
            if r.position() as usize != raw.len() {
                return Err("text29 registration trailing bytes".into());
            }
            prepare_text_registration_v29(&NativeTextRegisterV29 {
                version,
                source_binding,
                original_source_components,
                maximum_positions,
                program_bytes,
                real_parameters,
            })
        }
        "text-prepare29" => {
            let mut r = Cursor::new(raw);
            let version = read(&mut r)?;
            let request = read(&mut r)?;
            let prompt = vector(&mut r, misaka_palw_tir::program::HISTORY_BOUND_V1_HELD as usize, 4)?;
            if r.position() as usize != raw.len() {
                return Err("text29 preparation trailing bytes".into());
            }
            prepare_text_job_v29(
                &NativeTextPrepareV29 { version, request, prompt },
                program.ok_or("--program canonical TIR required")?,
            )
        }
        "text-consent29" => {
            if raw.len() > 4096 {
                return Err("text29 fixed consent cap".into());
            }
            let c: NativeTextConsentV29 = borsh::from_slice(raw).map_err(|e| e.to_string())?;
            if c.version != 29
                || c.producer_operator == c.requester_operator
                || c.accepted_daa >= c.expires_daa
                || c.preparation == [0; 64]
                || c.maximum.request != c.request
            {
                return Err("text29 original consent shape/actors/clock".into());
            }
            text(29, &c)
        }
        "text-fund29" => {
            if raw.len() != 130 {
                return Err("text29 fixed Fund extent".into());
            }
            let f: NativeTextFundV29 = borsh::from_slice(raw).map_err(|e| e.to_string())?;
            if f.version != 29 || f.preparation == [0; 64] || f.initial_consent == [0; 64] {
                return Err("text29 Fund original consent missing".into());
            }
            text(30, &f)
        }
        "text-commit29" => {
            let mut r = Cursor::new(raw);
            let version = read(&mut r)?;
            let job = read(&mut r)?;
            let original_funding = read(&mut r)?;
            let maximum_contract = read(&mut r)?;
            let requester_nonce = read(&mut r)?;
            let generated = vector(&mut r, misaka_palw_tir::program::HISTORY_BOUND_V1_HELD as usize, 4)?;
            let actual_scope25 = read(&mut r)?;
            let actual_resource29 = read(&mut r)?;
            let evidence: misaka_palw_kernel::seg::SegmentedEvidenceV2 = read(&mut r)?;
            let segment_roots = vector(&mut r, misaka_palw_kernel::seg::SEG_MAX_SEGMENTS_V4 as usize, 64)?;
            let final_state = read(&mut r)?;
            if r.position() as usize != raw.len() || version != 29 {
                return Err("text29 Commit original bounded grammar".into());
            }
            let req = request.ok_or("--request-borsh original request required")?;
            let p = program.ok_or("--program canonical TIR required")?;
            check_generated_v29(req, p.token_bound, &generated)?;
            evidence.check_structure(&segment_roots)?;
            if req.prompt_len.checked_add(generated.len() as u32).and_then(|n| n.checked_sub(1)) != Some(evidence.positions)
                || requester_nonce != req.nonce
                || [job, original_funding, maximum_contract, actual_scope25, actual_resource29, final_state].contains(&[0; 64])
            {
                return Err("text29 Commit differs from original requested stop/nonce/extent".into());
            }
            text(
                31,
                &NativeTextPostV29 {
                    version,
                    job,
                    original_funding,
                    maximum_contract,
                    requester_nonce,
                    generated,
                    actual_scope25,
                    actual_resource29,
                    evidence,
                    segment_roots,
                    final_state,
                },
            )
        }
        "stage-bundle28" => {
            let b = kaspa_consensus_core::palw_flat_stage_v28::NativeStageSourceBundleV28::from_bytes(raw)?;
            prepare_stage_bundle_v28(&b)
        }
        "stage-class28" => {
            let c = misaka_palw_kernel::flat_inputs12::bounded28::FlatStageClassWireV28::from_bytes(raw)?;
            stage(20, &c)
        }
        "stage-accept28" => {
            let a = kaspa_consensus_core::palw_flat_stage_v28::StageAcceptSourceV28::from_bytes(raw)?;
            prepare_stage_acceptance_v28(&a)
        }
        "stage-fund28" => {
            let f = kaspa_consensus_core::palw_flat_stage_v14::StageFundRequestV14::from_bytes(raw)?;
            stage(22, &f)
        }
        "stage-commit28" => {
            let t = misaka_palw_kernel::flat_quota11::stage14::StageTraceV14::from_bytes(
                raw,
                positions.ok_or("--positions exact original funded stage extent required")?,
            )?;
            stage(23, &t)
        }
        _ => Err("unknown explicit native29/28 payload kind".into()),
    }
}

/// A complete, ordered pre-Fund paid registration transport. Its namespace and
/// original canonical BODY are pinned separately from the outer transport.
/// Native consensus still authenticates the live global sequence, quota,
/// interval and all fees; preparing these objects grants no admission.
pub struct PreparedNativeInitialV31 {
    pub start: kaspa_consensus_core::palw_native_initial_v31::NativeInitialStartV31,
    pub id: Digest,
    pub carriers: Vec<PreparedNativePayloadV29>,
}
pub fn prepare_initial_transport_v31(
    payload: &PreparedNativePayloadV29,
    owner: &PalwBondKeyV2,
    sequence: u64,
    now: u64,
    interval_end: u64,
    expires_daa: u64,
) -> Result<PreparedNativeInitialV31, String> {
    use kaspa_consensus_core::palw_native_initial_v31::*;
    let (namespace, action, body) = match payload.object() {
        K::FundedExecutionV11 { action: 27, body } => (29, 27, body),
        K::StageExecutionV14 { action, body } if matches!(action, 19 | 20) => (28, *action, body),
        _ => return Err("initial31 may only carry canonical Text29 or Stage28 registration".into()),
    };
    if body.is_empty() || body.len() > INITIAL_MAX_BYTES_V31 {
        return Err("initial31 canonical body byte cap".into());
    }
    let start = NativeInitialStartV31 {
        version: 31,
        namespace,
        action,
        sequence,
        bytes: body.len() as u32,
        count: body.len().div_ceil(INITIAL_PART_BYTES_V31) as u16,
        original_object: initial_object_root_v31(body),
        expires_daa,
    };
    start.check(now, interval_end)?;
    let id = start.id(owner);
    let mut carriers = Vec::with_capacity(start.count as usize + 2);
    carriers.push(text(INITIAL_START_ACTION_V31, &start)?);
    for (index, bytes) in body.chunks(INITIAL_PART_BYTES_V31).enumerate() {
        carriers.push(text(
            INITIAL_PART_ACTION_V31,
            &NativeInitialPartV31 { version: 31, id, index: index as u16, bytes: bytes.to_vec() },
        )?);
    }
    // Exact 64-byte identity, without a vector prefix or copied full body.
    carriers
        .push(PreparedNativePayloadV29 { object: K::FundedExecutionV11 { action: INITIAL_ASSEMBLE_ACTION_V31, body: id.to_vec() } });
    Ok(PreparedNativeInitialV31 { start, id, carriers })
}

#[cfg(test)]
mod initial_tests {
    use super::*;
    #[test]
    fn initial_transport_preserves_u16_parts_and_original_body() {
        let owner = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0));
        // Test the transport itself; canonical source validation belongs to the
        // typed registration builder, which owns PreparedNativePayload outside
        // this module. No arbitrary external caller can create this value.
        let body = vec![7; (256 << 15) + 1];
        let payload = PreparedNativePayloadV29 { object: K::StageExecutionV14 { action: 19, body: body.clone() } };
        let plan = prepare_initial_transport_v31(&payload, &owner, 8, 10, 100, 99).unwrap();
        assert_eq!(plan.start.count, 257);
        let mut restored = Vec::new();
        for (index, carrier) in plan.carriers[1..plan.carriers.len() - 1].iter().enumerate() {
            let raw = carrier.bytes();
            K::decode(&raw).unwrap();
            let K::FundedExecutionV11 { action: 42, body } = carrier.object() else { panic!("wrong initial action") };
            let part: kaspa_consensus_core::palw_native_initial_v31::NativeInitialPartV31 = borsh::from_slice(body).unwrap();
            assert_eq!(part.index as usize, index);
            assert_eq!(part.id, plan.id);
            restored.extend(part.bytes);
        }
        assert_eq!(restored, body);
        assert_eq!(plan.start.original_object, kaspa_consensus_core::palw_native_initial_v31::initial_object_root_v31(&restored));
        let K::FundedExecutionV11 { action: 43, body } = plan.carriers.last().unwrap().object() else { panic!("missing assemble") };
        assert_eq!(body, &plan.id.to_vec());
        assert!(prepare_initial_transport_v31(&payload, &owner, 8, 10, 100, 100).is_err());
    }
    #[test]
    fn initial_registration_cannot_transport_money_or_final_actions() {
        let owner = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0));
        for action in [28, 29, 30, 31, 34, 35] {
            let payload = PreparedNativePayloadV29 { object: K::FundedExecutionV11 { action, body: vec![1] } };
            assert!(prepare_initial_transport_v31(&payload, &owner, 0, 1, 10, 9).is_err());
        }
    }
}
