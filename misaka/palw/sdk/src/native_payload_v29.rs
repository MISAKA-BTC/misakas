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
/// Raw exact-grammar carrier for actions whose native body is not a borsh record
/// (fixed digests, paged/reply parts). Native consensus re-reads every byte.
fn text_raw(action: u8, body: Vec<u8>) -> Result<PreparedNativePayloadV29, String> {
    let object = K::FundedExecutionV11 { action, body };
    if object.encoded_len() > object.max_encoded_bytes() {
        return Err("text29 raw action exceeds explicitly selected native carrier".into());
    }
    Ok(PreparedNativePayloadV29 { object })
}
/// Action 33: own one paid proof (or a discovery ticket) on a Challengeable job.
pub fn prepare_text_own_v29(record: &NativeTextOwnV29) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 29 || record.job == [0; 64] || record.filing_bytes as usize > TEXT_STOP_MAX_FILING_BYTES_V29 {
        return Err("text29 owner record fields".into());
    }
    if record.discovery != (record.proof_root == [0; 64] && record.seal_root == [0; 64]) {
        return Err("text29 discovery carries no proof hashes; a sealed proof carries both".into());
    }
    let p = text(TEXT_OWN_ACTION_V29, record)?;
    match p.object() {
        K::FundedExecutionV11 { body, .. } if body.len() == 263 => Ok(p),
        _ => Err("text29 owner exact 263-byte grammar".into()),
    }
}
/// The whole paid filing a sealed owner reveals: `salt || u32 len || filing`.
/// Returns the file, its proof root and the seal root that `prepare_text_own_v29` binds.
pub fn native_paged_file_v29(
    namespace: u16,
    job: &Digest,
    claim: &Digest,
    actor: &Digest,
    maximum_contract: &Digest,
    salt: &Digest,
    filing: &[u8],
) -> Result<(Vec<u8>, Digest, Digest), String> {
    use kaspa_consensus_core::palw_native_paged_proof_v29::{paged_proof_root_v29, paged_proof_seal_v29};
    if filing.is_empty() || filing.len() + 68 > TEXT_STOP_MAX_FILING_BYTES_V29 {
        return Err("paged29 filing extent".into());
    }
    let mut file = Vec::with_capacity(68 + filing.len());
    file.extend_from_slice(salt);
    file.extend_from_slice(&(filing.len() as u32).to_le_bytes());
    file.extend_from_slice(filing);
    let proof = paged_proof_root_v29(&file);
    let seal = paged_proof_seal_v29(&namespace, job, claim, actor, maximum_contract, &proof, salt);
    Ok((file, proof, seal))
}
/// Action 32: the exact 32-KiB parts of one paid file. The last part's assembly
/// reveals it; there is no separate reveal action (34 is closed).
pub fn prepare_paged_parts_v29(
    job: &Digest,
    seal: &Digest,
    owner: &Digest,
    file: &[u8],
) -> Result<Vec<PreparedNativePayloadV29>, String> {
    let count = file.len().div_ceil(TEXT_STOP_PART_BYTES_V29);
    if file.is_empty() || count > TEXT_STOP_MAX_PARTS_V29 as usize {
        return Err("paged29 part count".into());
    }
    file.chunks(TEXT_STOP_PART_BYTES_V29)
        .enumerate()
        .map(|(index, bytes)| {
            let mut raw = Vec::with_capacity(202 + bytes.len());
            raw.extend_from_slice(&29u16.to_le_bytes());
            raw.extend_from_slice(job);
            raw.extend_from_slice(seal);
            raw.extend_from_slice(owner);
            raw.extend_from_slice(&(index as u16).to_le_bytes());
            raw.extend_from_slice(&(count as u16).to_le_bytes());
            raw.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            raw.extend_from_slice(bytes);
            kaspa_consensus_core::palw_native_paged_proof_v29::borrowed_native_paged_part_v29(&raw)
                .ok_or("paged29 part self-check")?;
            text_raw(TEXT_PAGE_ACTION_V29, raw)
        })
        .collect()
}
/// Action 35: close (cancel an uncommitted Funded job, or Final after every liability).
pub fn prepare_text_close_v29(job: &Digest) -> Result<PreparedNativePayloadV29, String> {
    text_raw(TEXT_CLOSE_ACTION_V29, job.to_vec())
}
/// Action 36: one public (or paid-owner) request for an original unit.
pub fn prepare_text_request_v29(
    request: &kaspa_consensus_core::palw_native_trace_da_v29::NativePublicRequestV29,
) -> Result<PreparedNativePayloadV29, String> {
    if request.version != 29 || request.slot >= 2 {
        return Err("text29 public request version/slot".into());
    }
    let p = text(TEXT_REQUEST_ACTION_V29, request)?;
    match p.object() {
        K::FundedExecutionV11 { body, .. } if (132..=256).contains(&body.len()) => Ok(p),
        _ => Err("text29 public request bounded grammar".into()),
    }
}
/// Action 37: the producer's reply pages (at most four 32-KiB parts) for one pending unit.
pub fn prepare_text_reply_parts_v29(pending: &Digest, reply: &[u8]) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use kaspa_consensus_core::palw_native_reply_parts_v29::{borrowed_native_reply_part_v29, native_reply_group_v29};
    let count = reply.len().div_ceil(32 << 10);
    if reply.is_empty() || count > 4 {
        return Err("text29 reply extent".into());
    }
    let group = native_reply_group_v29(*pending, reply);
    reply
        .chunks(32 << 10)
        .enumerate()
        .map(|(index, bytes)| {
            let mut raw = Vec::with_capacity(143 + bytes.len());
            raw.extend_from_slice(&29u16.to_le_bytes());
            raw.push(0);
            raw.extend_from_slice(pending);
            raw.extend_from_slice(&group);
            raw.extend_from_slice(&(index as u16).to_le_bytes());
            raw.extend_from_slice(&(count as u16).to_le_bytes());
            raw.extend_from_slice(&(reply.len() as u32).to_le_bytes());
            raw.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            raw.extend_from_slice(bytes);
            borrowed_native_reply_part_v29(&raw).ok_or("text29 reply part self-check")?;
            text_raw(TEXT_ANSWER_ACTION_V29, raw)
        })
        .collect()
}
/// Action 38: adjudicate one unanswered public request after its deadline.
pub fn prepare_text_default_v29(pending: &Digest) -> Result<PreparedNativePayloadV29, String> {
    text_raw(TEXT_DEFAULT_ACTION_V29, pending.to_vec())
}
/// Action 39: convert an owned discovery ticket into a sealed paid proof.
pub fn prepare_text_discovery_v29(
    job: &Digest,
    ticket: &Digest,
    proof_root: &Digest,
    seal_root: &Digest,
    total_bytes: u32,
    count: u16,
) -> Result<PreparedNativePayloadV29, String> {
    if count == 0 || count > TEXT_STOP_MAX_PARTS_V29 || (total_bytes as usize).div_ceil(TEXT_STOP_PART_BYTES_V29) != count as usize {
        return Err("text29 discovery conversion extent".into());
    }
    let mut raw = Vec::with_capacity(262);
    for d in [job, ticket, proof_root, seal_root] {
        raw.extend_from_slice(d);
    }
    raw.extend_from_slice(&total_bytes.to_le_bytes());
    raw.extend_from_slice(&count.to_le_bytes());
    text_raw(TEXT_DISCOVERY_ACTION_V29, raw)
}
/// Action 40 collection kinds: 0 retire one expired paid owner, 1 collect the
/// next public cache of a job, 2 retire a genuinely terminal job.
pub enum NativeTextCollectV29 {
    Owner { job: Digest, owner: Digest },
    PublicCache { job: Digest },
    Terminal { job: Digest },
}
pub fn prepare_text_collect_v29(kind: &NativeTextCollectV29) -> Result<PreparedNativePayloadV29, String> {
    let mut raw = Vec::with_capacity(129);
    match kind {
        NativeTextCollectV29::Owner { job, owner } => {
            raw.push(0);
            raw.extend_from_slice(job);
            raw.extend_from_slice(owner);
        }
        NativeTextCollectV29::PublicCache { job } => {
            raw.push(1);
            raw.extend_from_slice(job);
        }
        NativeTextCollectV29::Terminal { job } => {
            raw.push(2);
            raw.extend_from_slice(job);
        }
    }
    text_raw(TEXT_COLLECT_ACTION_V29, raw)
}
/// Funded action 45 (§II.14.60): typed window class adopting LengthContractV36 with a
/// complete WindowGraphV37. Native re-derives every field from the Text29 class; this
/// preparation only rejects an obviously inconsistent body before signing.
pub fn prepare_window_class_v37(
    record: &kaspa_consensus_core::palw_window_class_v37::NativeWindowClassRegisterV37,
) -> Result<PreparedNativePayloadV29, String> {
    record.graph.validate()?;
    if record.version != 37 || record.length.program_root != record.graph.program_root {
        return Err("window37 class version/program binding".into());
    }
    let body = borsh::to_vec(record).map_err(|e| e.to_string())?;
    if body.len() > kaspa_consensus_core::palw_window_class_v37::WINDOW_CLASS_MAX_BODY_BYTES_V37 {
        return Err("window37 class body cap".into());
    }
    text_raw(kaspa_consensus_core::palw_window_class_v37::TEXT_WINDOW_CLASS_ACTION_V37, body)
}
/// Funded action 46 (§II.14.60): the PRODUCER binds its Funded, uncommitted Text29 job
/// to a registered window class. Fund/Commit/Final/redemption remain Text29's.
pub fn prepare_window_attach_v37(job: &Digest, window_class: &Digest) -> Result<PreparedNativePayloadV29, String> {
    let mut raw = Vec::with_capacity(kaspa_consensus_core::palw_window_class_v37::WINDOW_ATTACH_BYTES_V37);
    raw.extend_from_slice(&37u16.to_le_bytes());
    raw.extend_from_slice(job);
    raw.extend_from_slice(window_class);
    text_raw(kaspa_consensus_core::palw_window_class_v37::TEXT_WINDOW_ATTACH_ACTION_V37, raw)
}
/// The paid filing BODY for a window court (wrapped by `native_paged_file_v29` with the
/// owner's salt): kind 3 = `WindowFaultV37`, kind 4 = the job's v8 `HistorySelectionFaultV8`.
pub fn window_filing_v37(kind: u8, fault: &[u8]) -> Result<Vec<u8>, String> {
    if !matches!(kind, 3 | 4) || fault.is_empty() {
        return Err("window37 filing kind 3|4 and nonempty fault required".into());
    }
    let mut raw = Vec::with_capacity(7 + fault.len());
    raw.extend_from_slice(&37u16.to_le_bytes());
    raw.push(kind);
    raw.extend_from_slice(&(fault.len() as u32).to_le_bytes());
    raw.extend_from_slice(fault);
    Ok(raw)
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
/// Explicit Master32 carrier. Nested objects retain their old wire formats;
/// invocation32 owns their new master/input binding. This is a signed payload,
/// not a certificate that the upstream jobs have finalized.
fn master_invocation_v32(
    action: u8,
    envelope: &kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32,
    positions: Option<u32>,
) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32;
    let raw = borsh::to_vec(envelope).map_err(|e| e.to_string())?;
    let checked = NativeStageInvocationEnvelopeV32::from_bytes(&raw)?;
    if checked.master == [0; 64] || checked.upstream_jobs.len() != checked.stage as usize {
        return Err("master32 carrier must include every earlier original component".into());
    }
    if checked.stage == 0 {
        if !checked.upstream_outputs.is_empty() {
            return Err("master32 first stage has no upstream outputs".into());
        }
    } else {
        let outputs = kaspa_consensus_core::palw_native_audit_proof_v32::decode_native_master_outputs_v32(&checked.upstream_outputs)?;
        if outputs.len() != checked.upstream_jobs.len() {
            return Err("master32 upstream outputs differ from original stage catalogue".into());
        }
    }
    match action {
        34 => {
            kaspa_consensus_core::palw_flat_stage_v28::StageAcceptSourceV28::from_bytes(&checked.body)?;
        }
        35 => {
            kaspa_consensus_core::palw_flat_stage_v14::StageFundRequestV14::from_bytes(&checked.body)?;
        }
        36 => {
            misaka_palw_kernel::flat_quota11::stage14::StageTraceV14::from_invocation_bytes_v32(
                &checked.body,
                positions.ok_or("master32 Commit requires exact actual positions")?,
            )?;
        }
        _ => return Err("master32 invocation action must be34/35/36".into()),
    }
    stage(action, &checked)
}
pub fn prepare_master_acceptance_v32(
    envelope: &kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32,
) -> Result<PreparedNativePayloadV29, String> {
    master_invocation_v32(34, envelope, None)
}
pub fn prepare_master_funding_v32(
    envelope: &kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32,
) -> Result<PreparedNativePayloadV29, String> {
    master_invocation_v32(35, envelope, None)
}
/// Check the exact actual invocation and ORIGINAL maximum quota before signing.
/// Same-length generated-token substitutions change trace_input_root.
pub fn prepare_master_post_v32(
    envelope: &kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32,
    actual: &misaka_palw_kernel::flat_inputs12::composition32::CheckedStageInvocationV32<'_>,
    maximum_job: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageJobV28,
    job_id: Digest,
) -> Result<PreparedNativePayloadV29, String> {
    use misaka_palw_kernel::flat_quota11::stage14::{StageTraceV14, bounded28::derive_stage_quota_v32};
    let trace = StageTraceV14::from_invocation_bytes_v32(&envelope.body, actual.positions())?;
    let quota = derive_stage_quota_v32(actual.class(), maximum_job)?;
    if envelope.master != actual.master_root()
        || envelope.stage != actual.class().wire().stage
        || envelope.generated != actual.generated()
        || trace.class != actual.class().root()
        || trace.job_id != job_id
        || trace.job_input != actual.trace_input_root()
        || trace.admission != actual.job().admission().root()
        || trace.quota != quota.root()
    {
        return Err("master32 Commit changes original maximum quota or actual invocation/input/decode".into());
    }
    master_invocation_v32(36, envelope, Some(actual.positions()))
}

/// Compact Commit32 fixes actual stopped IDs once, after original Fund. Output
/// openings are delivered with the separate authenticated output certificate.
pub fn prepare_master_compact_post_v32(
    record: &kaspa_consensus_core::palw_flat_stage_v28::NativeInvocationCompactCommitV32,
    actual: &misaka_palw_kernel::flat_inputs12::composition32::CheckedStageInvocationV32<'_>,
    maximum_job: &misaka_palw_kernel::flat_inputs12::bounded28::CheckedStageJobV28,
    job_id: Digest,
) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_flat_stage_v28::NativeInvocationCompactCommitV32;
    let raw = borsh::to_vec(record).map_err(|e| e.to_string())?;
    NativeInvocationCompactCommitV32::from_bytes(&raw, maximum_job.job().positions)?;
    let trace =
        misaka_palw_kernel::flat_quota11::stage14::StageTraceV14::from_invocation_bytes_v32(&record.trace, actual.positions())?;
    let quota = misaka_palw_kernel::flat_quota11::stage14::bounded28::derive_stage_quota_v32(actual.class(), maximum_job)?;
    if record.job != job_id
        || record.generated != actual.generated()
        || trace.version != 32
        || !trace.output_openings.is_empty()
        || trace.class != actual.class().root()
        || trace.job_id != job_id
        || trace.job_input != actual.trace_input_root()
        || trace.admission != actual.job().admission().root()
        || trace.quota != quota.root()
    {
        return Err("master32 compact post changes original job/maximum or actual stopped invocation".into());
    }
    stage(38, record)
}
/// Prepares the dedicated before-claim paid page lane; it cannot carry an
/// arbitrary initial registration or reuse the old u8 fragment contract.
pub fn prepare_master_compact_parts_v32(
    record: &kaspa_consensus_core::palw_flat_stage_v28::NativeInvocationCompactCommitV32,
    maximum_positions: u32,
) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use kaspa_consensus_core::palw_flat_stage_v28::*;
    let raw = borsh::to_vec(record).map_err(|e| e.to_string())?;
    NativeInvocationCompactCommitV32::from_bytes(&raw, maximum_positions)?;
    let group = invocation_commit_group_v32(record.job, &raw);
    let count = raw.len().div_ceil(32 << 10);
    let bound = misaka_palw_kernel::flat_quota11::stage14::bounded28::invocation_commit_bytes_bound_v32(maximum_positions)?;
    let part_bound = misaka_palw_kernel::flat_quota11::stage14::bounded28::invocation_commit_parts_bound_v32(maximum_positions)?;
    if raw.len() as u64 > bound || count > part_bound as usize {
        return Err("master32 compact post exceeds original maximum-derived reservation".into());
    }
    if count == 0 || count > 512 {
        return Err("master32 compact original page bound".into());
    }
    let mut parts = Vec::with_capacity(count + 1);
    for (index, bytes) in raw.chunks(32 << 10).enumerate() {
        let mut body = Vec::with_capacity(INVOCATION_COMMIT_PART_PREFIX_V32 + bytes.len());
        body.extend_from_slice(&32u16.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&record.job);
        body.extend_from_slice(&group);
        body.extend_from_slice(&(index as u16).to_le_bytes());
        body.extend_from_slice(&(count as u16).to_le_bytes());
        body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        body.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        body.extend_from_slice(bytes);
        borrowed_invocation_commit_part_v32(&body).ok_or("master32 compact page framing differs")?;
        let object = K::StageExecutionV14 { action: 39, body };
        if object.encoded_len() > object.max_encoded_bytes() {
            return Err("master32 page outer carrier cap".into());
        }
        parts.push(PreparedNativePayloadV29 { object });
    }
    let mut body = Vec::with_capacity(130);
    body.extend_from_slice(&32u16.to_le_bytes());
    body.extend_from_slice(&record.job);
    body.extend_from_slice(&group);
    parts.push(PreparedNativePayloadV29 { object: K::StageExecutionV14 { action: 40, body } });
    Ok(parts)
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
    if kind.starts_with("source-") && kind.ends_with("39") {
        return decode_native_payload_v39(kind, raw, positions);
    }
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
            // Length caps come from the job's own versioned maximum (request29), exactly as
            // the consensus decoder reads them — never from the frozen v1 history constant.
            let req = request.ok_or("--request-borsh original request required")?;
            let max_generated = req.maximum_new_tokens as usize;
            let max_segments = req.maximum_positions()?.div_ceil(misaka_palw_kernel::seg::SEG_LEN_V4) as usize;
            let mut r = Cursor::new(raw);
            let version = read(&mut r)?;
            let job = read(&mut r)?;
            let original_funding = read(&mut r)?;
            let maximum_contract = read(&mut r)?;
            let requester_nonce = read(&mut r)?;
            let generated = vector(&mut r, max_generated, 4)?;
            let actual_scope25 = read(&mut r)?;
            let actual_resource29 = read(&mut r)?;
            let evidence: misaka_palw_kernel::seg::SegmentedEvidenceV2 = read(&mut r)?;
            let segment_roots = vector(&mut r, max_segments, 64)?;
            let final_state = read(&mut r)?;
            if r.position() as usize != raw.len() || version != 29 {
                return Err("text29 Commit original bounded grammar".into());
            }
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
        "text-own29" => {
            if raw.len() != 263 {
                return Err("text29 owner exact 263-byte grammar".into());
            }
            prepare_text_own_v29(&borsh::from_slice(raw).map_err(|e| e.to_string())?)
        }
        "text-close29" => prepare_text_close_v29(&raw.try_into().map_err(|_| "text29 close exact job digest")?),
        "text-request29" => {
            if !(132..=256).contains(&raw.len()) {
                return Err("text29 public request bounded grammar".into());
            }
            prepare_text_request_v29(&borsh::from_slice(raw).map_err(|e| e.to_string())?)
        }
        "text-default29" => prepare_text_default_v29(&raw.try_into().map_err(|_| "text29 default exact pending digest")?),
        "text-discovery29" => {
            if raw.len() != 262 {
                return Err("text29 discovery exact 262-byte grammar".into());
            }
            let d = |i: usize| -> Digest { raw[i * 64..(i + 1) * 64].try_into().unwrap() };
            prepare_text_discovery_v29(
                &d(0),
                &d(1),
                &d(2),
                &d(3),
                u32::from_le_bytes(raw[256..260].try_into().unwrap()),
                u16::from_le_bytes(raw[260..262].try_into().unwrap()),
            )
        }
        "text-collect29" => {
            let d = |at: usize| -> Result<Digest, String> {
                raw.get(at..at + 64)
                    .ok_or("text29 collection digest absent")?
                    .try_into()
                    .map_err(|_| "text29 collection digest".to_string())
            };
            let kind = match (raw.first(), raw.len()) {
                (Some(0), 129) => NativeTextCollectV29::Owner { job: d(1)?, owner: d(65)? },
                (Some(1), 65) => NativeTextCollectV29::PublicCache { job: d(1)? },
                (Some(2), 65) => NativeTextCollectV29::Terminal { job: d(1)? },
                _ => return Err("text29 collection exact kind grammar".into()),
            };
            prepare_text_collect_v29(&kind)
        }
        "window-attach37" => {
            if raw.len() != 128 {
                return Err("window37 attach body is job || window class (128 bytes)".into());
            }
            prepare_window_attach_v37(&raw[..64].try_into().unwrap(), &raw[64..].try_into().unwrap())
        }
        "window-class37" => {
            prepare_window_class_v37(&kaspa_consensus_core::palw_window_class_v37::NativeWindowClassRegisterV37::from_bytes(raw)?)
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
        "stage-compact-commit32" => {
            let cap = positions.ok_or("--positions original maximum positions required")?;
            let record = kaspa_consensus_core::palw_flat_stage_v28::NativeInvocationCompactCommitV32::from_bytes(raw, cap)?;
            stage(38, &record)
        }
        "stage-invocation-accept32" | "stage-invocation-fund32" | "stage-invocation-commit32" => {
            let envelope = kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32::from_bytes(raw)?;
            let action = match kind {
                "stage-invocation-accept32" => 34,
                "stage-invocation-fund32" => 35,
                _ => 36,
            };
            master_invocation_v32(action, &envelope, positions)
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
    fn master_invocation_cannot_omit_prior_component_catalogue_or_substitute_raw_body() {
        use kaspa_consensus_core::palw_flat_stage_v28::NativeStageInvocationEnvelopeV32;
        let mut e = NativeStageInvocationEnvelopeV32 {
            version: 32,
            master: [1; 64],
            stage: 1,
            generated: vec![],
            upstream_jobs: vec![],
            upstream_outputs: vec![],
            body: vec![],
        };
        assert!(prepare_master_acceptance_v32(&e).is_err());
        e.stage = 0;
        assert!(prepare_master_acceptance_v32(&e).is_err());
        assert!(prepare_master_funding_v32(&e).is_err());
        assert!(master_invocation_v32(36, &e, None).is_err());
        e.version = 31;
        assert!(prepare_master_acceptance_v32(&e).is_err());
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

/// Owning39 upload uses its own paid source journal, u16 pages and canonical
/// Source39 extent. It never routes a multi-MiB source through direct action41
/// or the frozen object-chunks contract. Native consensus charges every step.
pub fn prepare_source_upload_v39(
    raw: &[u8],
    owner: PalwBondKeyV2,
    sequence: u64,
    now: u64,
    deadline: u64,
    interval_end: u64,
) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use kaspa_consensus_core::palw_native_execution_v39::*;
    let source = misaka_palw_kernel::flat_source39::FlatSourceWireV39::from_bytes(raw)?.check()?;
    if raw.len() > SOURCE_UPLOAD_MAX_BYTES_V39 {
        return Err("source39 complete original body exceeds its selected upload contract".into());
    }
    let count = raw.len().div_ceil(SOURCE_PART_BYTES_V39);
    let start = NativeSourceUploadStartV39 {
        version: 39,
        source: source.root(),
        bytes: u32::try_from(raw.len()).map_err(|_| "source39 upload length")?,
        count: u16::try_from(count).map_err(|_| "source39 upload page count")?,
        sequence,
        deadline,
        body: misaka_palw_kernel::hash::object_id(b"misaka-palw/native/source-body/v39", &raw.to_vec()),
    };
    start.check(now, interval_end)?;
    let id = start.id(owner);
    let mut objects = Vec::with_capacity(count + 2);
    objects.push(stage39_raw(SOURCE_START_ACTION_V39, borsh::to_vec(&start).map_err(|e| e.to_string())?)?);
    for (index, page) in raw.chunks(SOURCE_PART_BYTES_V39).enumerate() {
        let mut body = Vec::with_capacity(72 + page.len());
        body.extend_from_slice(&39u16.to_le_bytes());
        body.extend_from_slice(&id);
        body.extend_from_slice(&(index as u16).to_le_bytes());
        body.extend_from_slice(&(page.len() as u32).to_le_bytes());
        body.extend_from_slice(page);
        borrowed_source_upload_part_v39(&body).ok_or("source39 upload exact page grammar")?;
        objects.push(stage39_raw(SOURCE_PART_ACTION_V39, body)?);
    }
    objects.push(stage39_raw(SOURCE_ASSEMBLE_ACTION_V39, id.to_vec())?);
    Ok(objects)
}
fn stage39_raw(action: u8, body: Vec<u8>) -> Result<PreparedNativePayloadV29, String> {
    if !(41..=79).contains(&action) {
        return Err("native39 action outside owning namespace".into());
    }
    let object = K::StageExecutionV14 { action, body };
    if object.encoded_len() > object.max_encoded_bytes() {
        return Err("native39 complete object exceeds explicit carrier; select its paid paged contract".into());
    }
    K::decode(&object.encode()).map_err(|e| e.to_string())?;
    Ok(PreparedNativePayloadV29 { object })
}
/// These are bounded transport readers; checked native maximum/Fund/private
/// Final capabilities are exclusively produced by the genuine consensus fold.
pub fn decode_native_payload_v39(kind: &str, raw: &[u8], maximum: Option<u32>) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::{palw_native_execution_v39::*, palw_native_job_v39::*};
    match kind {
        "source-class39" => {
            let w = NativeClassWireV39::from_bytes(raw)?;
            stage39_raw(42, borsh::to_vec(&w).map_err(|e| e.to_string())?)
        }
        "source-consent39" => {
            let w = NativeConsentWireV39::from_bytes(raw)?;
            stage39_raw(43, borsh::to_vec(&w).map_err(|e| e.to_string())?)
        }
        "source-fund39" => {
            let w = NativeFundWireV39::from_bytes(raw)?;
            stage39_raw(44, borsh::to_vec(&w).map_err(|e| e.to_string())?)
        }
        "source-commit39" => {
            let w = NativeCommitWireV39::from_bytes(raw, maximum.ok_or("native39 commit requires original before-Fund maximum")?)?;
            stage39_raw(45, borsh::to_vec(&w).map_err(|e| e.to_string())?)
        }
        "source-owner39" => {
            if raw.len() != 139 || raw[..2] != 39u16.to_le_bytes() || raw[138] > 1 {
                return Err("native39 original owner framing".into());
            }
            prepare_owner_v39(
                raw[2..66].try_into().map_err(|_| "owner job")?,
                raw[66..130].try_into().map_err(|_| "owner filing")?,
                u64::from_le_bytes(raw[130..138].try_into().map_err(|_| "owner extent")?),
                raw[138] == 1,
            )
        }
        "source-request39" => {
            if raw.len() < 134
                || raw.len() > 134 + misaka_palw_kernel::flat_public39::REQUEST_MAX_BYTES_V39
                || raw[..2] != 39u16.to_le_bytes()
                || u32::from_le_bytes(raw[130..134].try_into().map_err(|_| "request extent")?) as usize != raw.len() - 134
            {
                return Err("native39 public request framing".into());
            }
            prepare_public_request_v39(
                raw[2..66].try_into().map_err(|_| "request job")?,
                raw[66..130].try_into().map_err(|_| "request owner")?,
                &misaka_palw_kernel::flat_public39::FlatDemandV39::from_bytes(&raw[134..])?,
            )
        }
        "source-terminal39" => {
            if raw.len() != 67 || raw[..2] != 39u16.to_le_bytes() || raw[2] > 1 {
                return Err("native39 terminal framing".into());
            }
            prepare_terminal_v39(raw[3..67].try_into().map_err(|_| "terminal job")?, raw[2] == 1)
        }
        "source-default39" | "source-owner-expire39" => {
            let n = if kind == "source-default39" { 66 } else { 130 };
            if raw.len() != n || raw[..2] != 39u16.to_le_bytes() {
                return Err("native39 exact original default/owner expiry framing".into());
            }
            stage39_raw(if n == 66 { 49 } else { 52 }, raw.to_vec())
        }
        "source-owner-convert39" => {
            if raw.len() != 202 || raw[..2] != 39u16.to_le_bytes() {
                return Err("native39 original discovery conversion framing".into());
            }
            let total = u64::from_le_bytes(raw[194..202].try_into().map_err(|_| "conversion extent")?);
            if total == 0 || total > 64 << 20 || raw[130..194] == [0; 64] {
                return Err("native39 discovery converted file extent/hash".into());
            }
            stage39_raw(56, raw.to_vec())
        }
        "source-master-close39" | "source-metadata-collect39" => {
            let id: Digest = raw.try_into().map_err(|_| "native39 original fixed Master/metadata identity")?;
            if id == [0; 64] {
                return Err("native39 original identity absent".into());
            }
            stage39_raw(if kind == "source-master-close39" { 66 } else { 70 }, id.to_vec())
        }
        "source-collect39" => {
            let id: Digest = raw.try_into().map_err(|_| "source39 collect original fixed id width")?;
            stage39_raw(60, id.to_vec())
        }
        "source-release39" => {
            if raw.len() != 65 || raw[0] > 1 {
                return Err("native39 release exact kind/id frame".into());
            }
            prepare_retention_release_v39(raw[0] == 1, raw[1..].try_into().unwrap())
        }
        "source-hold-reap39" => {
            let id: Digest = raw.try_into().map_err(|_| "native39 hold reap original fixed id width")?;
            prepare_consent_hold_reap_v39(id)
        }
        "source-collect-job39" => {
            let id: Digest = raw.try_into().map_err(|_| "native39 collection original fixed job width")?;
            prepare_collect_v39(id)
        }
        "source-commit-assemble39" => {
            let (job, group) = borrowed_native_commit_assembly_v39(raw).ok_or("native39 exact commit assembly frame")?;
            prepare_commit_assembly_v39(job, group)
        }
        _ => Err("unknown owning39 native transport kind".into()),
    }
}
/// SDK-side semantic class preparation derives the full original39 scope and
/// resource vector. No stored class ID or opaque quoted vector is accepted.
pub fn prepare_source_class_v39(
    source: &misaka_palw_kernel::flat_source39::CheckedSourceV39,
    policy: &kaspa_consensus_core::palw_native_execution_v39::NativeExecutionPoliciesV39,
    maximum: u32,
) -> Result<PreparedNativePayloadV29, String> {
    let w = kaspa_consensus_core::palw_native_execution_v39::NativeClassWireV39::derive(source, policy, maximum)?;
    w.check(source, policy)?;
    stage39_raw(42, borsh::to_vec(&w).map_err(|e| e.to_string())?)
}

pub fn prepare_source_consent_v39(
    record: &kaspa_consensus_core::palw_native_job_v39::NativeConsentWireV39,
    maximum: &misaka_palw_kernel::flat_job39::CheckedJobMaximumV39,
    native: &kaspa_consensus_core::palw_native_job_v39::CheckedNativeMaximumV39,
) -> Result<PreparedNativePayloadV29, String> {
    let request = maximum.request();
    let expected = native.record();
    if record.version != 39
        || record.class != expected.class
        || record.maximum_contract != expected.root()
        || expected.maximum != maximum.root()
        || expected.source != request.source
        || expected.input_manifest != maximum.input_manifest().root()
        || kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1(&record.requester) != request.requester
        || record.maximum != request.to_bytes()
        || record.material != maximum.input_manifest().to_bytes()
        || [record.requester_operator, record.producer_operator].contains(&[0; 64])
        || record.requester_operator == record.producer_operator
    {
        return Err("native39 producer consent changes original checked maximum/source/input/actors".into());
    }
    let raw = borsh::to_vec(record).map_err(|e| e.to_string())?;
    kaspa_consensus_core::palw_native_job_v39::NativeConsentWireV39::from_bytes(&raw)?;
    stage39_raw(43, raw)
}
pub fn prepare_source_funding_v39(
    record: &kaspa_consensus_core::palw_native_job_v39::NativeFundWireV39,
    accepted: &kaspa_consensus_core::palw_native_job_v39::NativeConsentV39,
) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 39 || record.consent != accepted.root() {
        return Err("native39 Fund changes originally accepted producer consent".into());
    }
    stage39_raw(44, borsh::to_vec(record).map_err(|e| e.to_string())?)
}
/// The original native job is checked again by consensus. This SDK method
/// constructs only a commitment; private native Final authority is not returned.
pub fn prepare_source_post_v39<'a>(
    record: &'a kaspa_consensus_core::palw_native_job_v39::NativeCommitWireV39,
    source: &'a misaka_palw_kernel::flat_source39::CheckedSourceV39,
    maximum: &misaka_palw_kernel::flat_job39::CheckedJobMaximumV39,
    inputs: &'a misaka_palw_kernel::flat_job39::ActualInputResolverV39<'a>,
    book: &kaspa_consensus_core::palw_native_job_v39::NativeJobBookV39,
) -> Result<PreparedNativePayloadV29, String> {
    if record.version != 39
        || record.job != book.id()
        || book.original().source != source.root()
        || book.original().maximum != maximum.root()
        || record.generated != inputs.generated()
    {
        return Err("native39 Commit changes original funded job/source/max/firstEOS".into());
    }
    misaka_palw_kernel::flat_point39::CheckedPointContextV39::check(source, maximum, record.claim.root(), &record.claim, inputs)?;
    stage39_raw(45, borsh::to_vec(record).map_err(|e| e.to_string())?)
}

/// New39 material transport only. The native fold separately authenticates the
/// prior upstream Final/once-credit; an output plan alone grants no authority.
pub fn prepare_material_start_v39(
    class: &kaspa_consensus_core::palw_native_execution_v39::CheckedNativeClassV39,
    material: &misaka_palw_kernel::flat_material39::CheckedMaterialPlanV39,
    certificate: Option<&misaka_palw_kernel::flat_material_output39::CheckedOutputCertificatePlanV39>,
) -> Result<PreparedNativePayloadV29, String> {
    use misaka_palw_kernel::flat_material39::MaterialOriginV39;
    let descriptor = material.descriptor();
    if descriptor.source != class.source().root() {
        return Err("native39 material class differs from original source".into());
    }
    match (&descriptor.origin, certificate) {
        (MaterialOriginV39::External { .. }, None) => {}
        (MaterialOriginV39::Upstream { output, .. }, Some(c))
            if *output == c.root()
                && descriptor.tensor == c.record().tensor
                && descriptor.dtype == c.record().dtype
                && descriptor.shape == c.record().shape => {}
        _ => return Err("native39 material requires its exact original output certificate contract".into()),
    }
    let raw = descriptor.to_bytes();
    let mut body = Vec::with_capacity(79 + raw.len() + 512);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&class.root());
    body.extend_from_slice(&material.record().deadline.to_le_bytes());
    body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    body.extend_from_slice(&raw);
    body.push(u8::from(certificate.is_some()));
    if let Some(c) = certificate {
        let raw = c.record().to_bytes();
        if raw.len() > 512 {
            return Err("native39 certificate plan extent".into());
        }
        body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        body.extend_from_slice(&raw);
    }
    stage39_raw(61, body)
}
pub fn prepare_material_part_v39(
    material: &misaka_palw_kernel::flat_material39::CheckedMaterialPlanV39,
    page: &misaka_palw_kernel::flat_material39::MaterialPageV39,
    certificate: Option<&misaka_palw_kernel::flat_material_output39::CheckedOutputCertificatePlanV39>,
) -> Result<PreparedNativePayloadV29, String> {
    use misaka_palw_kernel::{flat_material_output39::*, flat_material39::*};
    let descriptor = material.descriptor();
    let (lane, pages, bytes, expected, leaf, hash): (u8, u64, usize, Digest, Digest, fn(&Digest, &Digest) -> Digest) =
        if let Some(c) = certificate {
            let MaterialOriginV39::Upstream { output, .. } = &descriptor.origin else {
                return Err("native39 external material has no certificate lane".into());
            };
            if *output != c.root() {
                return Err("native39 original certificate plan differs".into());
            }
            let remaining = c
                .record()
                .bytes
                .checked_sub(page.index as u64 * MATERIAL_PAGE_BYTES_V39 as u64)
                .ok_or("native39 certificate part index")?;
            (
                1,
                c.pages() as u64,
                remaining.min(MATERIAL_PAGE_BYTES_V39 as u64) as usize,
                c.record().page_root,
                output_certificate_page_leaf_v39(c.record(), page.index, &page.body),
                output_certificate_page_node_v39,
            )
        } else {
            (
                0,
                material.record().pages as u64,
                material.part_bytes(page.index)?,
                descriptor.page_root,
                material_page_leaf_v39(descriptor, page.index, &page.body),
                material_page_node_v39,
            )
        };
    if page.index as u64 >= pages
        || page.body.len() != bytes
        || page.siblings.len() > 14
        || misaka_palw_kernel::merkle3::tree_root_from_path(leaf, page.index as u64, pages, &page.siblings, hash) != Some(expected)
    {
        return Err("native39 part differs from original authenticated raw/certificate page".into());
    }
    let mut body = Vec::with_capacity(69 + bytes + 1024);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.push(lane);
    body.extend_from_slice(&descriptor.root());
    body.extend_from_slice(&page.index.to_le_bytes());
    body.extend_from_slice(&borsh::to_vec(page).map_err(|e| e.to_string())?);
    stage39_raw(62, body)
}
pub fn prepare_material_close_v39(descriptor: Digest, collect: bool) -> Result<PreparedNativePayloadV29, String> {
    if descriptor == [0; 64] {
        return Err("native39 original material identity absent".into());
    }
    let mut body = Vec::with_capacity(66);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&descriptor);
    stage39_raw(if collect { 64 } else { 63 }, body)
}

/// Complete source39 registration, with every class/stage retained in order.
/// A body above the original direct packet cap MUST use new67..69 transport.
pub fn prepare_master_registration_v39(
    wire: &kaspa_consensus_core::palw_native_master_v39::NativeMasterRegisterV39,
    complete: &misaka_palw_kernel::flat_component_source39::CheckedCompleteSourceV39,
    classes: &[kaspa_consensus_core::palw_native_execution_v39::CheckedNativeClassV39],
    now: u64,
    interval_end: u64,
) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_native_master_v39::NativeMasterRegisterV39;
    use misaka_palw_kernel::flat_invocation39::MasterPublicRequestV39;
    let raw = borsh::to_vec(wire).map_err(|e| e.to_string())?;
    NativeMasterRegisterV39::from_bytes(&raw)?;
    let source_bundle = complete.wire();
    if wire.deadline <= now
        || wire.deadline > interval_end
        || wire.manifest != source_bundle.manifest
        || wire.pipeline != source_bundle.pipeline
        || classes.len() != complete.sources().len()
        || wire.classes.len() != classes.len()
        || wire.producer_bonds.len() != classes.len()
    {
        return Err("native39 Master original complete source/class/time contract differs".into());
    }
    for (stage, (class, source)) in classes.iter().zip(complete.sources()).enumerate() {
        if wire.classes[stage] != class.root()
            || class.source().root() != source.root()
            || class.record().max_positions != source.original_context_max()
            || complete.pipeline().wire().stages[stage].max_trip < source.original_context_max()
        {
            return Err("native39 Master ordered registered source/class differs".into());
        }
    }
    let request = MasterPublicRequestV39::from_bytes(&wire.request)?;
    request.check(complete.pipeline())?;
    if request.complete_source != complete.root() {
        return Err("native39 Master original request binds another complete component bundle".into());
    }
    stage39_raw(65, raw)
}

/// Read the concrete preparation directory and construct all original paid
/// material objects. The explicit aggregate limit applies before page reads.
pub fn prepare_material_directory_v39(
    class: &kaspa_consensus_core::palw_native_execution_v39::CheckedNativeClassV39,
    directory: &std::path::Path,
    upstream: Option<(&misaka_palw_kernel::flat_source39::CheckedSourceV39, &misaka_palw_kernel::flat_point39::FlatClaimV39)>,
    maximum_transport_bytes: u64,
) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use misaka_palw_kernel::{flat_material_output39::*, flat_material39::*};
    let read = |name: &str, cap| crate::native_audit_io_v32::read_bounded_file_v32(&directory.join(name), cap);
    let descriptor = MaterialDescriptorV39::from_bytes(&read("descriptor39.borsh", 2048)?)?;
    let raw = read("plan39.borsh", 2048)?;
    let record: MaterialPlanV39 = borsh::from_slice(&raw).map_err(|e| e.to_string())?;
    let plan = CheckedMaterialPlanV39::derive(&descriptor, record.start, record.deadline)?;
    if plan.record() != &record {
        return Err("native39 prepared material plan differs from original derivation".into());
    }
    let certificate = if let Some((source, claim)) = upstream {
        Some(CheckedOutputCertificatePlanV39::check(
            source,
            claim,
            &descriptor,
            &OutputCertificatePlanV39::from_bytes(&read("certificate-plan39.borsh", 512)?)?,
        )?)
    } else {
        None
    };
    let count = plan.record().pages as u64 + certificate.as_ref().map_or(0, |c| c.pages() as u64);
    let total = plan
        .record()
        .raw_bytes
        .checked_add(certificate.as_ref().map_or(0, |c| c.stored_bytes()))
        .and_then(|n| n.checked_add(count.checked_mul(2048)?))
        .and_then(|n| n.checked_add(8192))
        .ok_or("native39 original material transport extent overflow")?;
    if maximum_transport_bytes == 0 || maximum_transport_bytes > 600 << 20 || total > maximum_transport_bytes {
        return Err("native39 material transport exceeds explicit aggregate byte budget before page reads".into());
    }
    let mut objects = Vec::with_capacity(count as usize + 2);
    objects.push(prepare_material_start_v39(class, &plan, certificate.as_ref())?);
    for index in 0..plan.record().pages {
        let page = MaterialPageV39::from_bytes(&read(&format!("page-{index:04}.borsh"), (32 << 10) + 2048)?)?;
        if page.index != index {
            return Err("native39 original raw page index differs".into());
        }
        objects.push(prepare_material_part_v39(&plan, &page, None)?);
    }
    if let Some(c) = certificate.as_ref() {
        for index in 0..c.pages() {
            let page = MaterialPageV39::from_bytes(&read(&format!("certificate-page-{index:04}.borsh"), (32 << 10) + 2048)?)?;
            if page.index != index {
                return Err("native39 original output certificate page index differs".into());
            }
            objects.push(prepare_material_part_v39(&plan, &page, Some(c))?);
        }
    }
    objects.push(prepare_material_close_v39(descriptor.root(), false)?);
    Ok(objects)
}

/// Explicit new67..69 upload, shared only by the two new owning39 metadata
/// targets. It never reinterprets Source57, legacy chunks or a funded DA lease.
pub fn prepare_metadata_upload_v39(
    raw: &[u8],
    target: kaspa_consensus_core::palw_native_master_v39::NativeMetadataTargetV39,
    owner: PalwBondKeyV2,
    sequence: u64,
    now: u64,
    deadline: u64,
    interval_end: u64,
) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use kaspa_consensus_core::{palw_native_job_v39::NativeConsentWireV39, palw_native_master_v39::*};
    let object = match target {
        NativeMetadataTargetV39::Master => NativeMasterRegisterV39::from_bytes(raw)?.root(owner),
        NativeMetadataTargetV39::ProducerConsent => NativeConsentWireV39::from_bytes(raw)?.class,
    };
    let count = raw.len().div_ceil(32768);
    let start = NativeMetadataUploadStartV39 {
        version: 39,
        target,
        object,
        bytes: u32::try_from(raw.len()).map_err(|_| "metadata39 original byte extent")?,
        count: u16::try_from(count).map_err(|_| "metadata39 page count")?,
        sequence,
        deadline,
        body: metadata_body_hash_v39(raw),
    };
    start.check(now, interval_end)?;
    let id = start.id(owner);
    let mut objects = Vec::with_capacity(count + 2);
    objects.push(stage39_raw(67, borsh::to_vec(&start).map_err(|e| e.to_string())?)?);
    for (index, page) in raw.chunks(32768).enumerate() {
        let mut body = Vec::with_capacity(72 + page.len());
        body.extend_from_slice(&39u16.to_le_bytes());
        body.extend_from_slice(&id);
        body.extend_from_slice(&(index as u16).to_le_bytes());
        body.extend_from_slice(&(page.len() as u32).to_le_bytes());
        body.extend_from_slice(page);
        kaspa_consensus_core::palw_native_execution_v39::borrowed_source_upload_part_v39(&body)
            .ok_or("metadata39 exact bounded page frame")?;
        objects.push(stage39_raw(68, body)?);
    }
    objects.push(stage39_raw(69, id.to_vec())?);
    Ok(objects)
}

pub fn prepare_public_request_v39(
    job: Digest,
    paid_owner: Digest,
    demand: &misaka_palw_kernel::flat_public39::FlatDemandV39,
) -> Result<PreparedNativePayloadV29, String> {
    if job == [0; 64] {
        return Err("native39 public request original job absent".into());
    }
    let raw = demand.to_bytes();
    misaka_palw_kernel::flat_public39::FlatDemandV39::from_bytes(&raw)?;
    let mut body = Vec::with_capacity(134 + raw.len());
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&job);
    body.extend_from_slice(&paid_owner);
    body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    body.extend_from_slice(&raw);
    stage39_raw(47, body)
}
pub fn prepare_public_reply_parts_v39(pending: Digest, raw: &[u8]) -> Result<Vec<PreparedNativePayloadV29>, String> {
    if pending == [0; 64] {
        return Err("native39 reply original pending unit absent".into());
    }
    misaka_palw_kernel::flat_public39::FlatReplyV39::from_bytes(raw)?;
    let group = misaka_palw_kernel::hash::object_id(b"misaka-palw/native/original-public-reply-file/v39", &raw.to_vec());
    let count = raw.len().div_ceil(32768);
    if count == 0 || count > 4 {
        return Err("native39 public reply exceeds original four-part contract".into());
    }
    let mut parts = Vec::with_capacity(count);
    for (index, page) in raw.chunks(32768).enumerate() {
        let mut body = Vec::with_capacity(143 + page.len());
        body.extend_from_slice(&39u16.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&pending);
        body.extend_from_slice(&group);
        body.extend_from_slice(&(index as u16).to_le_bytes());
        body.extend_from_slice(&(count as u16).to_le_bytes());
        body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        body.extend_from_slice(&(page.len() as u32).to_le_bytes());
        body.extend_from_slice(page);
        parts.push(stage39_raw(48, body)?);
    }
    Ok(parts)
}
/// Commit to a fixed filing file, or pay for discovery at the independently
/// derived whole-file maximum. Neither path asserts correctness or conviction.
pub fn prepare_owner_v39(job: Digest, proof: Digest, total: u64, discovery: bool) -> Result<PreparedNativePayloadV29, String> {
    if job == [0; 64] || total == 0 || total > 64 << 20 || discovery != (proof == [0; 64]) {
        return Err("native39 owner original proof/discovery/whole-file bound".into());
    }
    let mut body = Vec::with_capacity(139);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&job);
    body.extend_from_slice(&proof);
    body.extend_from_slice(&total.to_le_bytes());
    body.push(u8::from(discovery));
    stage39_raw(46, body)
}
pub fn prepare_owner_conversion_v39(job: Digest, owner: Digest, file: &[u8]) -> Result<PreparedNativePayloadV29, String> {
    let proof = owned_file_root_v39(file)?;
    if [job, owner].contains(&[0; 64]) {
        return Err("native39 discovery conversion original IDs absent".into());
    }
    let mut body = Vec::with_capacity(202);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&job);
    body.extend_from_slice(&owner);
    body.extend_from_slice(&proof);
    body.extend_from_slice(&(file.len() as u64).to_le_bytes());
    stage39_raw(56, body)
}
pub fn owned_file_root_v39(file: &[u8]) -> Result<Digest, String> {
    if file.len() < 68
        || file.len() > 64 << 20
        || u32::from_le_bytes(file[64..68].try_into().map_err(|_| "native39 owned file length")?) as usize != file.len() - 68
    {
        return Err("native39 original salt/filing whole-file extent differs".into());
    }
    Ok(misaka_palw_kernel::hash::object_id(b"misaka-palw/native/original-owned-proof-file/v39", &file.to_vec()))
}
pub fn prepare_protected_parts_v39(job: Digest, owner: Digest, file: &[u8]) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use kaspa_consensus_core::palw_native_job_v39::borrowed_native_proof_part_v39;
    owned_file_root_v39(file)?;
    if [job, owner].contains(&[0; 64]) {
        return Err("native39 proof parts original IDs absent".into());
    }
    let count = file.len().div_ceil(32768);
    let mut objects = Vec::with_capacity(count + 1);
    let frame = |mode: u8, index: u16, page: &[u8]| {
        let mut body = Vec::with_capacity(143 + page.len());
        body.extend_from_slice(&39u16.to_le_bytes());
        body.push(mode);
        body.extend_from_slice(&job);
        body.extend_from_slice(&owner);
        body.extend_from_slice(&index.to_le_bytes());
        body.extend_from_slice(&(count as u16).to_le_bytes());
        body.extend_from_slice(&(file.len() as u32).to_le_bytes());
        body.extend_from_slice(&(page.len() as u32).to_le_bytes());
        body.extend_from_slice(page);
        body
    };
    for (index, page) in file.chunks(32768).enumerate() {
        let body = frame(0, index as u16, page);
        borrowed_native_proof_part_v39(&body).ok_or("native39 original paid proof page frame")?;
        objects.push(stage39_raw(50, body)?);
    }
    let body = frame(1, 0, &[]);
    borrowed_native_proof_part_v39(&body).ok_or("native39 original paid whole-proof assembly frame")?;
    objects.push(stage39_raw(50, body)?);
    Ok(objects)
}
pub fn prepare_terminal_v39(job: Digest, refund: bool) -> Result<PreparedNativePayloadV29, String> {
    if job == [0; 64] {
        return Err("native39 terminal original job absent".into());
    }
    let mut body = Vec::with_capacity(67);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.push(u8::from(refund));
    body.extend_from_slice(&job);
    stage39_raw(51, body)
}
// ---- Owning39 mining (stage actions 73–77/79) and the PNR9 receipt-block carriage ----
// Frames only: the dormant fold re-derives everything (economic class, work, price, budget, Gate C) from original rows.
// There is no model-level approval action (user decision 2026-10-15, round 5).
pub fn prepare_mining_register_v39(
    template_master: Digest,
    sequence: u64,
    share_permille: u16,
) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_native_mining_v39::*;
    let w = NativeMiningModelWireV39 { version: 39, template_master, sequence, share_permille };
    let raw = borsh::to_vec(&w).map_err(|e| e.to_string())?;
    NativeMiningModelWireV39::from_bytes(&raw)?;
    stage39_raw(REGISTER_MODEL_ACTION_V39, raw)
}
/// `kind`: 0 seat (action 74), 1 seat exit / 2 model retirement (action 76).
pub fn prepare_mining_seat_v39(model: Digest, kind: u8) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_native_mining_v39::*;
    let w = NativeMiningSeatWireV39 { version: 39, model, kind };
    let raw = borsh::to_vec(&w).map_err(|e| e.to_string())?;
    NativeMiningSeatWireV39::from_bytes(&raw)?;
    stage39_raw(if kind == 0 { MODEL_SEAT_ACTION_V39 } else { MODEL_RETIRE_ACTION_V39 }, raw)
}
pub fn prepare_mining_commit_v39(model: Digest, master: Digest, sequence: u64) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_native_mining_v39::*;
    let w = NativeMiningCommitWireV39 { version: 39, model, master, sequence };
    let raw = borsh::to_vec(&w).map_err(|e| e.to_string())?;
    NativeMiningCommitWireV39::from_bytes(&raw)?;
    stage39_raw(MINING_COMMIT_ACTION_V39, raw)
}
/// Action 77 (`kind 0`, a claim) or 79 (`kind 0` claim / `kind 1` receipt).
pub fn prepare_mining_subject_v39(action: u8, kind: u8, id: Digest) -> Result<PreparedNativePayloadV29, String> {
    use kaspa_consensus_core::palw_native_mining_v39::*;
    if !matches!(action, ISSUE_RECEIPT_ACTION_V39 | COLLECT_MINING_ACTION_V39) {
        return Err("mining39 subject frames are actions 77 and 79".into());
    }
    let w = NativeMiningSubjectWireV39 { version: 39, kind, id };
    let raw = borsh::to_vec(&w).map_err(|e| e.to_string())?;
    NativeMiningSubjectWireV39::from_bytes(&raw)?;
    stage39_raw(action, raw)
}
/// The unsigned PNR9 spend for one header position and the exact message the
/// receipt producer's ML-DSA-87 key signs (context
/// `PALW_NATIVE_RECEIPT_MLDSA87_CONTEXT_V39`). The miner signs; no key is held here.
#[allow(clippy::too_many_arguments)]
pub fn native_receipt_spend_unsigned_v39(
    network_domain: kaspa_consensus_core::Hash64,
    pre_pow_hash: kaspa_consensus_core::Hash64,
    timestamp: u64,
    nonce: u64,
    receipt: kaspa_consensus_core::Hash64,
    quantum_index: u32,
    producer_bond: kaspa_consensus_core::tx::TransactionOutpoint,
    producer_pubkey: Vec<u8>,
) -> (kaspa_consensus_core::palw_native_receipt_v39::PalwNativeReceiptSpendV39, kaspa_consensus_core::Hash64) {
    use kaspa_consensus_core::palw_native_receipt_v39::*;
    let spend = PalwNativeReceiptSpendV39 {
        version: 39,
        network_domain,
        challenge: native_receipt_spend_challenge_v39(
            network_domain,
            pre_pow_hash,
            timestamp,
            nonce,
            receipt,
            quantum_index,
            &producer_bond,
        ),
        receipt,
        quantum_index,
        producer_bond,
        producer_pubkey,
    };
    let message = native_receipt_spend_id_v39(&spend);
    (spend, message)
}
pub fn encode_native_receipt_carriage_v39(
    spend: kaspa_consensus_core::palw_native_receipt_v39::PalwNativeReceiptSpendV39,
    signature: Vec<u8>,
) -> Result<Vec<u8>, String> {
    use kaspa_consensus_core::palw_native_receipt_v39::*;
    let bytes = PalwNativeReceiptSpendEnvelopeV39 { spend, signature }.encode();
    PalwNativeReceiptSpendEnvelopeV39::decode(&bytes)?;
    Ok(bytes)
}
/// **Retention release71** (RFC-0002 row 14): the retention payer releases one
/// of ITS active source (`class = false`) or class (`class = true`) slots.
/// Exact 67-byte body `39u16 ‖ kind ‖ id`; the fold refuses while any live
/// consent, job, Master, model or class still retains it, and refuses any
/// signer other than the recorded payer. Freeing a slot never adds Q/B/R/F.
pub fn prepare_retention_release_v39(class: bool, id: Digest) -> Result<PreparedNativePayloadV29, String> {
    if id == [0; 64] {
        return Err("native39 release original identity absent".into());
    }
    let mut body = Vec::with_capacity(67);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.push(u8::from(class));
    body.extend_from_slice(&id);
    stage39_raw(71, body)
}
/// **Expired producer-consent hold collection72** (RFC-0002 row 14): the
/// exact 64-byte hold id. Anyone may pay it; the fold re-reads the hold and
/// refuses unless it is a canonical consent hold strictly past its `until`.
pub fn prepare_consent_hold_reap_v39(hold: Digest) -> Result<PreparedNativePayloadV29, String> {
    if hold == [0; 64] {
        return Err("native39 hold original identity absent".into());
    }
    stage39_raw(72, hold.to_vec())
}
/// **Expiry scan for collection72**: every canonical producer-consent hold in
/// a fully authenticated row stream whose `until` lies strictly below
/// `accept_daa`, in key order. Candidates only: the fold decides at the DAA
/// the carrier is actually accepted at.
pub fn native_expired_consent_holds_v39<R: kaspa_consensus_core::palw_native_audit_proof_v39::NativeAuditRowReaderV39>(
    rows: &kaspa_consensus_core::palw_native_audit_proof_v39::VerifiedNativeAuxRowsV39<R>,
    accept_daa: u64,
) -> Result<Vec<Digest>, String> {
    let keys: Vec<Vec<u8>> =
        rows.keys().filter(|(t, k, _)| *t == 112 && k.len() == 66 && k[0] == 39 && k[1] == 94).map(|(_, k, _)| k.to_vec()).collect();
    let mut out = Vec::new();
    for k in keys {
        let Some(v) = rows.row(112, &k, 256)? else { continue };
        if let Some((id, until)) = kaspa_consensus_core::palw_state_v2::native_consent_hold_expiry_v39(&k, &v) {
            if accept_daa > until {
                out.push(id);
            }
        }
    }
    Ok(out)
}
/// Collection53 of a settled job past its complete original retention. The
/// fold decides eligibility from original rows; this frame asserts nothing.
pub fn prepare_collect_v39(job: Digest) -> Result<PreparedNativePayloadV29, String> {
    if job == [0; 64] {
        return Err("native39 collection original job absent".into());
    }
    let mut body = Vec::with_capacity(66);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&job);
    stage39_raw(kaspa_consensus_core::palw_native_job_v39::NATIVE_COLLECT_ACTION_V39, body)
}
pub fn prepare_commit_assembly_v39(job: Digest, group: Digest) -> Result<PreparedNativePayloadV29, String> {
    if [job, group].contains(&[0; 64]) {
        return Err("native39 commit assembly original identities absent".into());
    }
    let mut body = Vec::with_capacity(130);
    body.extend_from_slice(&39u16.to_le_bytes());
    body.extend_from_slice(&job);
    body.extend_from_slice(&group);
    stage39_raw(kaspa_consensus_core::palw_native_job_v39::NATIVE_COMMIT_ASSEMBLE_ACTION_V39, body)
}
/// Producer compact-post transport54/55 for a canonical Commit wire that is
/// too large for one carrier. `maximum` is the job's ORIGINAL before-Fund
/// maximum positions; the wire is re-decoded under it before paging, and the
/// group never exceeds the bound the fold derives from the same maximum.
pub fn prepare_commit_parts_v39(raw: &[u8], maximum: u32) -> Result<Vec<PreparedNativePayloadV29>, String> {
    use kaspa_consensus_core::palw_native_job_v39::*;
    let wire = NativeCommitWireV39::from_bytes(raw, maximum)?;
    if raw.len() as u64 > native_commit_bytes_bound_v39(maximum)? as u64 {
        return Err("native39 compact Commit exceeds its original maximum transport bound".into());
    }
    let group = native_commit_group_v39(wire.job, raw);
    let count = raw.len().div_ceil(NATIVE_COMMIT_PAGE_BYTES_V39);
    let mut objects = Vec::with_capacity(count + 1);
    for (index, page) in raw.chunks(NATIVE_COMMIT_PAGE_BYTES_V39).enumerate() {
        let mut body = Vec::with_capacity(NATIVE_COMMIT_PART_PREFIX_V39 + page.len());
        body.extend_from_slice(&39u16.to_le_bytes());
        body.push(0);
        body.extend_from_slice(&wire.job);
        body.extend_from_slice(&group);
        body.extend_from_slice(&(index as u16).to_le_bytes());
        body.extend_from_slice(&(count as u16).to_le_bytes());
        body.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        body.extend_from_slice(&(page.len() as u32).to_le_bytes());
        body.extend_from_slice(page);
        borrowed_native_commit_part_v39(&body).ok_or("native39 original commit page exact frame")?;
        objects.push(stage39_raw(NATIVE_COMMIT_PART_ACTION_V39, body)?);
    }
    objects.push(prepare_commit_assembly_v39(wire.job, group)?);
    Ok(objects)
}
