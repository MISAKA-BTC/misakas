//! Opt-in funded execution records over candidate9 and quota11.
//!
//! These bounded records and transitions do not authenticate a chain, debit a
//! coin, verify an R/PC court, serve evidence or activate a descriptor. Native
//! consumers supply authenticated source/trace headers and actual original
//! debit, reserve and court results; untrusted carriers must never supply them.
//! All money decisions are returned for atomic native application, once per job.
use crate::{
    flat_lengths7::FlatJobLengthsV7,
    flat_quota11::{
        CheckedJobQuotaPlanV11, QuotaPolicyV11, SubjectBindingV11, TrustedEpochV11, derive_job_quota_plan_from_checked_class_v11,
    },
    hash::{Digest, object_id},
    ledger::flat_candidate9::{FlatCandidateNativeViewV9, FlatCandidateRecordV9, FlatCandidateViewV9},
};
use borsh::{BorshDeserialize, BorshSerialize};

pub const EXECUTION_STATE_MAX_BYTES_V11: usize = 4096;

/// Native11 uses one monotone counter per authenticated requester operator.
/// Pure offline records can retain arbitrary diagnostic nonces; native funding
/// accepts only this fixed encoding and authenticates the exact next counter.
pub fn canonical_nonce_v11(nonce: &Digest) -> Result<u64, String> {
    if nonce[8..].iter().any(|b| *b != 0) {
        return Err("native execution nonce is not canonical u64 plus zero padding".into());
    }
    Ok(u64::from_le_bytes(nonce[..8].try_into().expect("fixed nonce prefix")))
}
pub fn nonce_bytes_v11(nonce: u64) -> Digest {
    let mut bytes = [0; 64];
    bytes[..8].copy_from_slice(&nonce.to_le_bytes());
    bytes
}
pub fn native_requester_nonce_counter_root_v11(operator: &Digest, next_nonce: u64) -> Digest {
    object_id(b"misaka-palw/kernel/native-requester-nonce-counter/v11", &(*operator, next_nonce))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionHorizonsV11 {
    pub commit_window: u64,
    pub challenge_window: u64,
    pub seal_maturity: u64,
    pub seal_ttl: u64,
    pub da_answer: u64,
    pub proof_grace: u64,
    pub retention: u64,
}
impl ExecutionHorizonsV11 {
    fn tail(&self) -> Result<u64, String> {
        [self.da_answer, self.proof_grace, self.seal_ttl, self.retention]
            .into_iter()
            .try_fold(0u64, |n, x| n.checked_add(x).ok_or_else(|| "execution horizon overflow".into()))
    }
    pub fn maximum_job_horizon(&self) -> Result<u64, String> {
        self.commit_window
            .checked_add(self.challenge_window)
            .and_then(|n| n.checked_add(self.tail().ok()?))
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| "execution horizon overflow".into())
    }
    fn check(&self) -> Result<(), String> {
        if self.commit_window == 0
            || self.challenge_window == 0
            || self.da_answer == 0
            || self.proof_grace == 0
            || self.retention == 0
            || self.seal_maturity != 3
            || self.seal_ttl != 100
            || self.proof_grace < self.seal_maturity
        {
            return Err("execution horizons lack fixed protected9 seal/answer/retention time".into());
        }
        self.maximum_job_horizon()?;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionPolicyV11 {
    pub version: u16,
    pub enabled: bool,
    pub network_domain: Digest,
    pub quota_policy_root: Digest,
    pub horizons: ExecutionHorizonsV11,
    pub reward_cap: u64,
    pub external_gain_cap: u64,
    pub default_obligation: u64,
    pub required_producer_reserve: u64,
    pub finder_permille: u16,
}
impl ExecutionPolicyV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-policy/v11", self)
    }
    pub fn check(&self, quota: &QuotaPolicyV11) -> Result<(), String> {
        quota.check()?;
        self.horizons.check()?;
        let gain = self
            .reward_cap
            .checked_add(self.external_gain_cap)
            .and_then(|n| n.checked_add(self.default_obligation))
            .ok_or("execution gain/reserve overflow")?;
        if self.version != 11
            || !self.enabled
            || self.network_domain != quota.network_domain
            || self.quota_policy_root != quota.root()
            || self.reward_cap == 0
            || self.required_producer_reserve < gain
            || self.finder_permille > 1000
            || quota.future_attempts.contains(&0)
            || self.horizons.maximum_job_horizon()? >= quota.interval_daa
        {
            return Err("execution policy is dormant, unfunded or lacks fixed quota guarantees".into());
        }
        Ok(())
    }
}
/// Fixed authenticated BindModelArtifactV2 header supplied by native state.
/// Its original binder liability may expire; each new claim has its own reserve.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceBindingV11 {
    pub descriptor: Digest,
    pub program_root: Digest,
    pub parameter_root: Digest,
    pub inventory_root: Digest,
    pub bound_daa: u64,
    pub matures_daa: u64,
    pub refuted: bool,
}
impl NativeSourceBindingV11 {
    pub fn id(&self) -> Digest {
        object_id(
            b"misaka-palw/model-artifact-binding/v2",
            &(self.descriptor, self.program_root, self.inventory_root, self.parameter_root),
        )
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceCourtBoundV11 {
    /// Exact PC-map bytes plus the frozen native2 identity-proof allowance.
    pub max_proof_bytes: u64,
    pub work: u64,
    pub scratch_bytes: u64,
}
/// Independently quote the native R/PC court from canonical original metadata.
/// B is the program byte count, never an adversary's proof byte declaration.
/// Native compares max_proof_bytes with its authenticated original binding.
pub fn native_source_court_bound_v11(
    program_bytes: usize,
    pc: &crate::trace::ParamCommitmentsV1,
) -> Result<NativeSourceCourtBoundV11, String> {
    let b = u64::try_from(program_bytes).map_err(|_| "native source program size overflow")?;
    let n = u64::try_from(pc.by_instance.len()).map_err(|_| "native source PC count overflow")?;
    let pc_bytes = pc.by_instance.keys().try_fold(4u64, |bytes, (_, layer)| {
        bytes.checked_add(67 + u64::from(layer.is_some()) * 2).ok_or("native source PC bytes overflow")
    })?;
    let max_proof_bytes = pc_bytes.checked_add(128 << 10).ok_or("native source proof bound overflow")?;
    let carried = max_proof_bytes.checked_add(7).ok_or("native source wrapper overflow")?;
    let work = b
        .checked_mul(b)
        .and_then(|w| w.checked_add(b.checked_mul(64)?))
        .and_then(|w| w.checked_add(n.checked_mul(8832)?))
        .and_then(|w| w.checked_add(carried.checked_mul(64)?))
        .and_then(|w| w.checked_add(65536))
        .ok_or("native source court work overflow")?;
    let scratch_bytes = b
        .checked_mul(8)
        .and_then(|w| w.checked_add(n.checked_mul(512)?))
        .and_then(|w| w.checked_add(carried.checked_mul(8)?))
        .and_then(|w| w.checked_add(65536))
        .ok_or("native source court scratch overflow")?;
    Ok(NativeSourceCourtBoundV11 { max_proof_bytes, work, scratch_bytes })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionClassRecordV11 {
    pub version: u16,
    pub candidate9_id: Digest,
    pub binding_id: Digest,
    pub descriptor: Digest,
    pub program_root: Digest,
    pub parameter_root: Digest,
    pub inventory_root: Digest,
    pub max_positions: u32,
    pub encoder: bool,
    pub class_scope9_root: Digest,
    pub replay9_root: Digest,
    pub resource10_root: Digest,
    pub source_court_bound: NativeSourceCourtBoundV11,
    pub execution_policy_root: Digest,
    pub quota_policy_root: Digest,
    pub horizons: ExecutionHorizonsV11,
    pub reward_cap: u64,
    pub required_producer_reserve: u64,
    pub finder_permille: u16,
}
impl ExecutionClassRecordV11 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-class/v11", self)
    }
}

mod execution_class_sealed_v11 {
    pub trait Sealed {}
}
/// Private constructors distinguish full public checking, native full-scope
/// checking and readonly fixed headers. A wire carrier cannot implement this
/// trait or grant admission by supplying a raw record or eligibility boolean.
pub trait ExecutionClassContextV11: execution_class_sealed_v11::Sealed {
    fn record(&self) -> &ExecutionClassRecordV11;
    fn encoder_input_limit(&self) -> Option<u32>;
    fn can_admit(&self) -> bool;
    fn native_admission_epoch(&self) -> Option<(Digest, u64)>;
}
pub struct CheckedExecutionClassV11 {
    record: ExecutionClassRecordV11,
    view: FlatCandidateViewV9,
    parameters: crate::trace::ParamCommitmentsV1,
    can_admit: bool,
    native_admission_epoch: Option<(Digest, u64)>,
}
impl CheckedExecutionClassV11 {
    pub fn record(&self) -> &ExecutionClassRecordV11 {
        &self.record
    }
    pub fn view(&self) -> &FlatCandidateViewV9 {
        &self.view
    }
    pub fn parameters(&self) -> &crate::trace::ParamCommitmentsV1 {
        &self.parameters
    }
    /// A restored original class grants only access to existing funded state.
    /// Native still authenticates its source eligibility and active fence.
    pub fn can_admit(&self) -> bool {
        self.can_admit
    }
}
impl execution_class_sealed_v11::Sealed for CheckedExecutionClassV11 {}
impl ExecutionClassContextV11 for CheckedExecutionClassV11 {
    fn record(&self) -> &ExecutionClassRecordV11 {
        &self.record
    }
    fn encoder_input_limit(&self) -> Option<u32> {
        self.view.encoder.map(|e| e.l)
    }
    fn can_admit(&self) -> bool {
        self.can_admit
    }
    fn native_admission_epoch(&self) -> Option<(Digest, u64)> {
        self.native_admission_epoch
    }
}

/// Native authenticates the independently admitted candidate9 row and prepays
/// its full-scope restore. Resource quantities at the class maximum were
/// independently derived on original admission; this view does not deserialize
/// or recompute them. Each actual job derives its own resource vector once.
pub struct CheckedNativeExecutionClassV11 {
    record: ExecutionClassRecordV11,
    view: FlatCandidateNativeViewV9,
    parameters: crate::trace::ParamCommitmentsV1,
    can_admit: bool,
    native_admission_epoch: Option<(Digest, u64)>,
}
impl CheckedNativeExecutionClassV11 {
    pub fn record(&self) -> &ExecutionClassRecordV11 {
        &self.record
    }
    pub fn view(&self) -> &FlatCandidateNativeViewV9 {
        &self.view
    }
    pub fn parameters(&self) -> &crate::trace::ParamCommitmentsV1 {
        &self.parameters
    }
    pub fn can_admit(&self) -> bool {
        self.can_admit
    }
    pub fn header(&self) -> NativeExecutionClassHeaderV11 {
        NativeExecutionClassHeaderV11 { version: 11, record: self.record.clone(), encoder_input_limit: self.encoder_input_limit() }
    }
}
impl execution_class_sealed_v11::Sealed for CheckedNativeExecutionClassV11 {}
impl ExecutionClassContextV11 for CheckedNativeExecutionClassV11 {
    fn record(&self) -> &ExecutionClassRecordV11 {
        &self.record
    }
    fn encoder_input_limit(&self) -> Option<u32> {
        self.view.encoder.map(|e| e.l)
    }
    fn can_admit(&self) -> bool {
        self.can_admit
    }
    fn native_admission_epoch(&self) -> Option<(Digest, u64)> {
        self.native_admission_epoch
    }
}

/// Fixed native class frame. Its expected root comes from original native
/// state independently of a Close carrier. Restoring it permits checking and
/// settling existing money only; it cannot prepare or fund a new job.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionClassHeaderV11 {
    pub version: u16,
    pub record: ExecutionClassRecordV11,
    pub encoder_input_limit: Option<u32>,
}
impl NativeExecutionClassHeaderV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/native-class-header/v11", self)
    }
    pub fn restore(&self, expected_root: Digest) -> Result<CheckedNativeExecutionClassHeaderV11, String> {
        if self.root() != expected_root
            || self.version != 11
            || self.record.version != 11
            || self.record.max_positions == 0
            || self.record.encoder != self.encoder_input_limit.is_some()
            || self.encoder_input_limit == Some(0)
            || self.record.encoder && self.record.max_positions != 1
        {
            return Err("native fixed class header differs from its independent original anchor".into());
        }
        self.record.horizons.check()?;
        Ok(CheckedNativeExecutionClassHeaderV11 { header: self.clone() })
    }
}
pub struct CheckedNativeExecutionClassHeaderV11 {
    header: NativeExecutionClassHeaderV11,
}
impl CheckedNativeExecutionClassHeaderV11 {
    pub fn record(&self) -> &ExecutionClassRecordV11 {
        &self.header.record
    }
}
impl execution_class_sealed_v11::Sealed for CheckedNativeExecutionClassHeaderV11 {}
impl ExecutionClassContextV11 for CheckedNativeExecutionClassHeaderV11 {
    fn record(&self) -> &ExecutionClassRecordV11 {
        &self.header.record
    }
    fn encoder_input_limit(&self) -> Option<u32> {
        self.header.encoder_input_limit
    }
    fn can_admit(&self) -> bool {
        false
    }
    fn native_admission_epoch(&self) -> Option<(Digest, u64)> {
        None
    }
}

pub fn restore_native_execution_class_header_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    original_class: Option<&ExecutionClassRecordV11>,
    original_policy: &ExecutionPolicyV11,
    original_quota: &QuotaPolicyV11,
    current_daa: u64,
) -> Result<CheckedNativeExecutionClassV11, String> {
    check_execution_source_v11(candidate, source, original_policy, original_quota, current_daa, false)?;
    let view = candidate.checked_native()?;
    let record = execution_class_record_v11(candidate, source, original_policy, original_quota, view.encoder.is_some())?;
    if original_class.is_some_and(|original| *original != record) {
        return Err("native original class header differs from exact checked candidate/source/policy".into());
    }
    Ok(CheckedNativeExecutionClassV11 {
        record,
        view,
        parameters: candidate.param_commitments.clone(),
        can_admit: false,
        native_admission_epoch: None,
    })
}
pub fn authorize_native_execution_class_header_v11(
    mut restored: CheckedNativeExecutionClassV11,
    source: &NativeSourceBindingV11,
    eligibility: &NativeClassEligibilityV11,
) -> Result<CheckedNativeExecutionClassV11, String> {
    check_native_execution_eligibility_v11(&restored, source, eligibility)?;
    restored.can_admit = true;
    restored.native_admission_epoch = Some((eligibility.block_hash, eligibility.current_daa));
    Ok(restored)
}
/// Native must prepay candidate metadata and authenticate binding/schedule and
/// policy first. Structural admission is not exhaustive source-model fidelity.
pub fn derive_execution_class_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    policy: &ExecutionPolicyV11,
    quota: &QuotaPolicyV11,
    current_daa: u64,
) -> Result<CheckedExecutionClassV11, String> {
    derive_execution_class_inner_v11(candidate, source, policy, quota, current_daa, true)
}
/// Independently reconstruct a native-anchored original class for restoring or
/// settling existing funded jobs. Source revocation cannot strand original
/// escrow, and this private checked value cannot admit another job. Native
/// authenticates the original class/policy rows and preserves their R/PC scope;
/// a refuted source flag is not a new verified fault or reward entitlement.
pub fn restore_execution_class_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    original_class: &ExecutionClassRecordV11,
    original_policy: &ExecutionPolicyV11,
    original_quota: &QuotaPolicyV11,
    current_daa: u64,
) -> Result<CheckedExecutionClassV11, String> {
    let restored = derive_execution_class_inner_v11(candidate, source, original_policy, original_quota, current_daa, false)?;
    if restored.record != *original_class {
        return Err("restored original execution class differs from its authenticated record".into());
    }
    Ok(restored)
}

/// Native-only restoration of an independently admitted original class. Native
/// authenticates the original class/resource/policy rows and prepays both the
/// candidate9 restore and the separate bounded resource-row restore tariff.
/// Public import and SDK validation must use the full independent derivation.
/// This path rechecks the complete scope, original roots and resource framing
/// but never recomputes resource quantities or grants fresh job admission.
pub fn restore_native_execution_class_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    original_class: &ExecutionClassRecordV11,
    original_policy: &ExecutionPolicyV11,
    original_quota: &QuotaPolicyV11,
    original_resource_bytes: &[u8],
    current_daa: u64,
) -> Result<CheckedExecutionClassV11, String> {
    check_execution_source_v11(candidate, source, original_policy, original_quota, current_daa, false)?;
    let native = candidate.checked_native()?;
    let resources = crate::flat_resources10::decode_rooted_flat_resources_v10(
        original_resource_bytes,
        original_class.resource10_root,
        native.scope.relations.len(),
    )?;
    if resources.program != object_id(b"misaka-palw/kernel/reference-resource-program/v10", &native.program)
        || resources.parameters != source.parameter_root
        || resources.court_scope != native.plan.scope_root
        || resources.positions != native.plan.max_positions
        || resources.encoder != native.plan.encoder
        || resources.primitives.iter().zip(&native.scope.relations).any(|(p, r)| {
            p.occurrence != r.occurrence
                || p.node != r.node
                || native
                    .program
                    .blocks
                    .get(r.block as usize)
                    .and_then(|b| b.nodes.get(r.node as usize))
                    .is_none_or(|n| n.prim.tag() != p.prim_tag)
        })
    {
        return Err("original native resource header or primitive namespace differs from the complete scope".into());
    }
    let view = FlatCandidateViewV9 {
        plan: native.plan,
        program: native.program,
        encoder: native.encoder,
        scope: native.scope,
        replay: native.replay,
        resources,
    };
    let restored = checked_execution_class_from_view_v11(candidate, source, original_policy, original_quota, view, false)?;
    if restored.record != *original_class {
        return Err("restored native execution class differs from its independent original record".into());
    }
    Ok(restored)
}

/// Non-wire witness constructed only from native authenticated current state.
/// It asserts exact live descriptor/binding and compiled policy selection at
/// this DAA. No carrier may construct it from raw hashes or eligibility flags.
#[derive(Clone, Debug)]
pub struct NativeClassEligibilityV11 {
    pub class_id: Digest,
    pub active_descriptor: Digest,
    pub active_binding: Digest,
    pub execution_policy_root: Digest,
    pub quota_policy_root: Digest,
    pub block_hash: Digest,
    pub current_daa: u64,
}
/// A readonly original-class restore cannot authorize another funded job by
/// itself. Native separately checks live source eligibility and active fence,
/// then supplies this exact non-wire witness; public/SDK checking stays full.
pub fn authorize_native_execution_class_v11(
    mut restored: CheckedExecutionClassV11,
    source: &NativeSourceBindingV11,
    eligibility: &NativeClassEligibilityV11,
) -> Result<CheckedExecutionClassV11, String> {
    check_native_execution_eligibility_v11(&restored, source, eligibility)?;
    restored.can_admit = true;
    restored.native_admission_epoch = Some((eligibility.block_hash, eligibility.current_daa));
    Ok(restored)
}
fn check_native_execution_eligibility_v11<C: ExecutionClassContextV11>(
    restored: &C,
    source: &NativeSourceBindingV11,
    eligibility: &NativeClassEligibilityV11,
) -> Result<(), String> {
    let c = restored.record();
    if restored.can_admit()
        || source.refuted
        || source.bound_daa > source.matures_daa
        || eligibility.current_daa < source.matures_daa
        || eligibility.class_id != c.id()
        || eligibility.active_descriptor != c.descriptor
        || eligibility.active_binding != c.binding_id
        || eligibility.execution_policy_root != c.execution_policy_root
        || eligibility.quota_policy_root != c.quota_policy_root
        || source.id() != c.binding_id
        || source.descriptor != c.descriptor
        || source.program_root != c.program_root
        || source.parameter_root != c.parameter_root
        || source.inventory_root != c.inventory_root
    {
        return Err("native class eligibility differs from its original scope or live source/policy fence".into());
    }
    Ok(())
}

fn check_execution_source_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    policy: &ExecutionPolicyV11,
    quota: &QuotaPolicyV11,
    current_daa: u64,
    can_admit: bool,
) -> Result<(), String> {
    policy.check(quota)?;
    if source.refuted && can_admit
        || source.bound_daa > source.matures_daa
        || current_daa < source.matures_daa
        || candidate.binding != source.id()
        || candidate.plan.descriptor != source.descriptor
        || candidate.plan.program_root != source.program_root
        || candidate.param_commitments.root() != source.parameter_root
    {
        return Err("execution class differs from matured authenticated source scope".into());
    }
    Ok(())
}
fn derive_execution_class_inner_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    policy: &ExecutionPolicyV11,
    quota: &QuotaPolicyV11,
    current_daa: u64,
    can_admit: bool,
) -> Result<CheckedExecutionClassV11, String> {
    check_execution_source_v11(candidate, source, policy, quota, current_daa, can_admit)?;
    let view = candidate.checked()?;
    checked_execution_class_from_view_v11(candidate, source, policy, quota, view, can_admit)
}
fn checked_execution_class_from_view_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    policy: &ExecutionPolicyV11,
    quota: &QuotaPolicyV11,
    view: FlatCandidateViewV9,
    can_admit: bool,
) -> Result<CheckedExecutionClassV11, String> {
    let record = execution_class_record_v11(candidate, source, policy, quota, view.encoder.is_some())?;
    Ok(CheckedExecutionClassV11 {
        record,
        view,
        parameters: candidate.param_commitments.clone(),
        can_admit,
        native_admission_epoch: None,
    })
}
fn execution_class_record_v11(
    candidate: &FlatCandidateRecordV9,
    source: &NativeSourceBindingV11,
    policy: &ExecutionPolicyV11,
    quota: &QuotaPolicyV11,
    encoder: bool,
) -> Result<ExecutionClassRecordV11, String> {
    Ok(ExecutionClassRecordV11 {
        version: 11,
        candidate9_id: candidate.id(),
        binding_id: source.id(),
        descriptor: source.descriptor,
        program_root: source.program_root,
        parameter_root: source.parameter_root,
        inventory_root: source.inventory_root,
        max_positions: candidate.plan.max_positions,
        encoder,
        class_scope9_root: candidate.plan.scope_root,
        replay9_root: candidate.plan.replay_root,
        resource10_root: candidate.plan.resource_root,
        source_court_bound: native_source_court_bound_v11(candidate.program_bytes.len(), &candidate.param_commitments)?,
        execution_policy_root: policy.root(),
        quota_policy_root: quota.root(),
        horizons: policy.horizons.clone(),
        reward_cap: policy.reward_cap,
        required_producer_reserve: policy.required_producer_reserve,
        finder_permille: policy.finder_permille,
    })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionJobInputV11 {
    pub version: u16,
    pub prompt_root: Digest,
    pub prompt_len: u32,
    pub generated_count: u32,
    pub lengths: FlatJobLengthsV7,
    pub positions: u32,
}
impl ExecutionJobInputV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-job-input/v11", &(self, crate::job::DecodeRuleV1::Greedy))
    }
    fn check<C: ExecutionClassContextV11>(&self, class: &C) -> Result<(), String> {
        let record = class.record();
        if self.version != 11
            || self.prompt_len == 0
            || self.lengths.patch != 0
            || self.lengths.audio != 0
            || self.positions == 0
            || self.positions > record.max_positions
        {
            return Err("initial funded execution11 requires exact bounded text input".into());
        }
        if let Some(input_limit) = class.encoder_input_limit() {
            if self.generated_count != 0
                || self.positions != 1
                || self.prompt_len > input_limit
                || self.lengths.token != self.prompt_len
            {
                return Err("funded encoder lengths differ from input declaration".into());
            }
        } else if self.generated_count == 0
            || self.prompt_len.checked_add(self.generated_count).and_then(|n| n.checked_sub(1)) != Some(self.positions)
            || self.lengths.token != self.positions
        {
            return Err("funded decoder must bind all prompt and fed-back positions".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionJobRecordV11 {
    pub version: u16,
    pub class_id: Digest,
    pub input: ExecutionJobInputV11,
    pub actual_scope9_root: Digest,
    pub actual_resource10_root: Digest,
    pub quota_plan_root: Digest,
    pub requester: Digest,
    pub producer: Digest,
    pub nonce: Digest,
    pub admitted_daa: u64,
    pub admitted_interval: u64,
    pub commit_deadline: u64,
    pub deadline_daa: u64,
    pub reward: u64,
    pub escrow_original_debit: u64,
    pub escrow_debit_receipt: Digest,
}
impl ExecutionJobRecordV11 {
    /// Admission time, tariffs and funding receipts cannot create another job
    /// identity for an identical request. Native also enforces requester/nonce
    /// uniqueness across class, producer and carrier substitutions.
    pub fn id(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/execution-job/v11",
            &(11u16, self.class_id, self.input.root(), self.requester, self.producer, self.nonce, self.reward),
        )
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-funded-job/v11", self)
    }
    pub fn request_key(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-request-nonce/v11", &(self.requester, self.nonce))
    }
    pub fn quota_subject(&self, class: &ExecutionClassRecordV11) -> Result<SubjectBindingV11, String> {
        if self.version != 11 || class.version != 11 || self.class_id != class.id() {
            return Err("funded job does not bind execution class".into());
        }
        Ok(SubjectBindingV11 {
            version: 11,
            policy_root: class.quota_policy_root,
            class_id: self.class_id,
            class_scope9_root: class.class_scope9_root,
            program_root: class.program_root,
            parameter_root: class.parameter_root,
            job_id: self.id(),
            job_input_root: self.input.root(),
            resource10_root: self.actual_resource10_root,
            quota_plan_root: self.quota_plan_root,
            actual_positions: self.input.positions,
            requester: self.requester,
            producer: self.producer,
            admitted_interval: self.admitted_interval,
            deadline_daa: self.deadline_daa,
        })
    }
}
pub struct PreparedExecutionJobV11 {
    pub job: ExecutionJobRecordV11,
    pub quota_plan: CheckedJobQuotaPlanV11,
}

/// Native writes this fixed record only after independently deriving the actual
/// quota/resource plan. Its root also binds prompt and actors; a plan root alone
/// cannot authorize funding a different request at the same position count.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionPreparationRecordV11 {
    pub version: u16,
    pub class_id: Digest,
    pub input: ExecutionJobInputV11,
    pub requester: Digest,
    pub producer: Digest,
    pub nonce: Digest,
    pub reward: u64,
    pub quota_plan_root: Digest,
}
impl NativeExecutionPreparationRecordV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/native-execution-preparation/v11", self)
    }
}
pub struct CheckedExecutionPreparationV11 {
    record: NativeExecutionPreparationRecordV11,
    plan: CheckedJobQuotaPlanV11,
}
impl CheckedExecutionPreparationV11 {
    pub fn record(&self) -> &NativeExecutionPreparationRecordV11 {
        &self.record
    }
    pub fn quota_plan(&self) -> &CheckedJobQuotaPlanV11 {
        &self.plan
    }
    /// Both expected roots ultimately originate in the independently checked
    /// native preparation row, never in the untrusted Fund body.
    pub fn restore(
        record: &NativeExecutionPreparationRecordV11,
        plan_record: &crate::flat_quota11::QuotaPlanRecordV11,
        expected_preparation_root: Digest,
    ) -> Result<Self, String> {
        if record.version != 11 || record.root() != expected_preparation_root {
            return Err("native preparation differs from its independently admitted request".into());
        }
        canonical_nonce_v11(&record.nonce)?;
        let plan = CheckedJobQuotaPlanV11::restore(plan_record, &record.quota_plan_root)?;
        Ok(Self { record: record.clone(), plan })
    }
}
pub fn bind_native_execution_preparation_v11<C: ExecutionClassContextV11>(
    class: &C,
    quota: &QuotaPolicyV11,
    input: ExecutionJobInputV11,
    plan: CheckedJobQuotaPlanV11,
    requester: Digest,
    producer: Digest,
    nonce: Digest,
    reward: u64,
) -> Result<CheckedExecutionPreparationV11, String> {
    quota.check()?;
    input.check(class)?;
    canonical_nonce_v11(&nonce)?;
    if !class.can_admit()
        || class.native_admission_epoch().is_none()
        || quota.root() != class.record().quota_policy_root
        || reward == 0
        || reward > class.record().reward_cap
    {
        return Err("native preparation lacks exact live native source/policy eligibility".into());
    }
    check_native_execution_plan_v11(class, &input, &plan)?;
    Ok(CheckedExecutionPreparationV11 {
        record: NativeExecutionPreparationRecordV11 {
            version: 11,
            class_id: class.record().id(),
            input,
            requester,
            producer,
            nonce,
            reward,
            quota_plan_root: plan.record().root(),
        },
        plan,
    })
}
fn check_native_execution_plan_v11<C: ExecutionClassContextV11>(
    class: &C,
    input: &ExecutionJobInputV11,
    plan: &CheckedJobQuotaPlanV11,
) -> Result<(), String> {
    let p = plan.record();
    let c = class.record();
    if p.version != 11
        || p.program_root != c.program_root
        || p.parameter_root != c.parameter_root
        || p.class_scope_root != c.class_scope9_root
        || p.source_court_bound != c.source_court_bound
        || p.positions != input.positions
    {
        return Err("native prepared actual plan differs from its class/input namespace".into());
    }
    Ok(())
}

/// Fund performs only fixed-record checking. Native authenticates preparation,
/// current eligibility, exact next operator nonce and the atomic original coin
/// debit first. Expensive actual-scope derivation was paid by Prepare once.
pub fn prepare_execution_job_from_checked_plan_v11<C: ExecutionClassContextV11>(
    class: &C,
    quota: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    preparation: CheckedExecutionPreparationV11,
    original_debit: u64,
    debit_receipt: Digest,
) -> Result<PreparedExecutionJobV11, String> {
    let p = preparation.record;
    if p.version != 11
        || p.class_id != class.record().id()
        || p.quota_plan_root != preparation.plan.record().root()
        || class.native_admission_epoch().is_none()
    {
        return Err("Fund does not use its exact original native preparation".into());
    }
    check_native_execution_plan_v11(class, &p.input, &preparation.plan)?;
    build_prepared_execution_job_v11(
        class,
        quota,
        epoch,
        p.input,
        preparation.plan,
        p.requester,
        p.producer,
        p.nonce,
        p.reward,
        original_debit,
        debit_receipt,
    )
}

fn check_execution_funding_v11<C: ExecutionClassContextV11>(
    class: &C,
    quota: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    input: &ExecutionJobInputV11,
    nonce: &Digest,
    reward: u64,
    original_debit: u64,
) -> Result<(u64, u64), String> {
    quota.check()?;
    input.check(class)?;
    let c = class.record();
    if !class.can_admit()
        || class.native_admission_epoch().is_some_and(|(hash, daa)| hash != epoch.block_hash || daa != epoch.block_daa)
        || class.native_admission_epoch().is_some() && canonical_nonce_v11(nonce).is_err()
        || quota.root() != c.quota_policy_root
        || epoch.network_interval != epoch.block_daa / quota.interval_daa
        || reward == 0
        || reward > c.reward_cap
        || original_debit != reward
    {
        return Err("funded job policy, epoch, reward or original escrow differs".into());
    }
    let commit_deadline = epoch.block_daa.checked_add(c.horizons.commit_window).ok_or("commit deadline overflow")?;
    let deadline = epoch.block_daa.checked_add(c.horizons.maximum_job_horizon()?).ok_or("job deadline overflow")?;
    let interval_end =
        epoch.network_interval.checked_add(1).and_then(|n| n.checked_mul(quota.interval_daa)).ok_or("quota interval overflow")?;
    if deadline >= interval_end {
        return Err("entire funded liability must fit its immutable quota interval".into());
    }
    Ok((commit_deadline, deadline))
}

#[allow(clippy::too_many_arguments)]
fn build_prepared_execution_job_v11<C: ExecutionClassContextV11>(
    class: &C,
    quota: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    input: ExecutionJobInputV11,
    plan: CheckedJobQuotaPlanV11,
    requester: Digest,
    producer: Digest,
    nonce: Digest,
    reward: u64,
    original_debit: u64,
    debit_receipt: Digest,
) -> Result<PreparedExecutionJobV11, String> {
    let (commit_deadline, deadline) = check_execution_funding_v11(class, quota, epoch, &input, &nonce, reward, original_debit)?;
    check_native_execution_plan_v11(class, &input, &plan)?;
    let c = class.record();
    let job = ExecutionJobRecordV11 {
        version: 11,
        class_id: c.id(),
        input,
        actual_scope9_root: plan.actual_scope_root(),
        actual_resource10_root: plan.resource_root(),
        quota_plan_root: plan.record().root(),
        requester,
        producer,
        nonce,
        admitted_daa: epoch.block_daa,
        admitted_interval: epoch.network_interval,
        commit_deadline,
        deadline_daa: deadline,
        reward,
        escrow_original_debit: original_debit,
        escrow_debit_receipt: debit_receipt,
    };
    Ok(PreparedExecutionJobV11 { job, quota_plan: plan })
}
/// `original_debit` and receipt come from an actual atomic native coin debit,
/// after duplicate-job/nonce and requester authorization checks. Never from a
/// carrier's claimed balance. Native must prepay full job scope derivation.
pub fn prepare_execution_job_v11(
    class: &CheckedExecutionClassV11,
    quota: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    input: ExecutionJobInputV11,
    requester: Digest,
    producer: Digest,
    nonce: Digest,
    reward: u64,
    original_debit: u64,
    debit_receipt: Digest,
) -> Result<PreparedExecutionJobV11, String> {
    check_execution_funding_v11(class, quota, epoch, &input, &nonce, reward, original_debit)?;
    // The public path still independently derives the class resource quantities.
    if class.view.resources.root() != class.record.resource10_root {
        return Err("funded class resource namespace mismatch".into());
    }
    let plan = derive_job_quota_plan_from_checked_class_v11(class, input.positions)?;
    build_prepared_execution_job_v11(
        class,
        quota,
        epoch,
        input,
        plan,
        requester,
        producer,
        nonce,
        reward,
        original_debit,
        debit_receipt,
    )
}

/// Derived from a native checked protected9 posting, not from a caller's
/// claimed verification flag. It binds the immutable request separately from
/// the trace's fed-back generated input root.
#[derive(Clone, Debug)]
pub struct NativeProtectedTraceV11 {
    pub vector: Digest,
    pub candidate9_id: Digest,
    pub binding_id: Digest,
    pub class_scope9_root: Digest,
    pub replay9_root: Digest,
    pub class_resource10_root: Digest,
    pub prompt_root: Digest,
    pub prompt_len: u32,
    pub positions: u32,
    pub generated_count: u32,
    pub trace_root: Digest,
    pub output_root: Digest,
    pub final_state_root: Digest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ExecutionTerminalV11 {
    Funded,
    Challengeable,
    Final,
    ComputationFault,
    SourceMismatch,
    DataUnavailable,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionClaimRecordV11 {
    pub job_id: Digest,
    pub producer: Digest,
    pub vector: Digest,
    pub trace_root: Digest,
    pub output_root: Digest,
    pub final_state_root: Digest,
    pub committed_daa: u64,
    pub challenge_end: u64,
    pub retention_until: u64,
    pub reserved_original: u64,
    pub reservation_receipt: Digest,
    pub terminal: ExecutionTerminalV11,
    pub credit_paid: bool,
}
impl ExecutionClaimRecordV11 {
    pub fn id(&self) -> Digest {
        // Terminal status or credit/payment carriers cannot change the claim.
        object_id(
            b"misaka-palw/kernel/execution-claim/v11",
            &(self.job_id, self.producer, self.vector, self.trace_root, self.output_root, self.final_state_root, self.committed_daa),
        )
    }
}
/// Fixed trusted chain snapshot; the native consumer counts its authenticated
/// own-seal/chunk/DA rows. Never deserialize these values from a close carrier.
pub struct NativeCloseSnapshotV11 {
    pub current_daa: u64,
    pub pending_own_proofs: u32,
    pub open_da: u32,
    pub latest_answer_proof_until: u64,
    pub required_retained_until: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAdjudicationKindV11 {
    ComputationFault,
    SourceMismatch,
    DataUnavailable,
}
/// A native verified result with exact subject roots and clock. The caller
/// verifies bounded court bytes, own seals and R/PC source tiles (or an actual
/// trace-DA default) first. Source-R absence has no adjudication variant.
pub struct NativeExecutionJudgmentV11 {
    pub job_id: Digest,
    pub claim_id: Digest,
    pub vector: Digest,
    pub class_id: Digest,
    pub inventory_root: Digest,
    pub parameter_root: Digest,
    pub current_daa: u64,
    /// Actual native court inclusion time, or immutable unanswered trace-DA
    /// due+1. Late collection cannot invent an earlier source/computation court.
    pub established_daa: u64,
    pub proof_id: Digest,
    pub finder: Digest,
    pub kind: NativeAdjudicationKindV11,
}
/// Non-wire original court snapshot from authenticated native state. Native
/// writes it only after paying for and verifying the actual exact-scope court.
/// A source binding's refuted flag cannot produce this witness. Immutable
/// proof identity and original inclusion time allow later collection without
/// accepting a new late court or silently repeating an expensive verifier.
pub struct NativeEstablishedExecutionCourtV11 {
    pub job_id: Digest,
    pub claim_id: Digest,
    pub vector: Digest,
    pub class_id: Digest,
    pub inventory_root: Digest,
    pub parameter_root: Digest,
    pub established_daa: u64,
    pub proof_id: Digest,
    pub kind: NativeAdjudicationKindV11,
    pub original_finder: Digest,
    /// Native compares the original finder and this job's producer operators.
    /// Same-operator collection burns the full slash. Otherwise only the
    /// original paid finder may be entitled; a Close caller cannot replace it.
    pub eligible_finder: Option<Digest>,
}
impl NativeEstablishedExecutionCourtV11 {
    fn matches(&self, result: &NativeExecutionJudgmentV11) -> bool {
        self.job_id == result.job_id
            && self.claim_id == result.claim_id
            && self.vector == result.vector
            && self.class_id == result.class_id
            && self.inventory_root == result.inventory_root
            && self.parameter_root == result.parameter_root
            && self.established_daa == result.established_daa
            && self.proof_id == result.proof_id
            && self.kind == result.kind
            && self.original_finder == result.finder
            && self.eligible_finder.is_none_or(|finder| finder == self.original_finder)
            && self.kind != NativeAdjudicationKindV11::DataUnavailable
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionSettlementV11 {
    pub job_id: Digest,
    pub claim_id: Option<Digest>,
    pub terminal: ExecutionTerminalV11,
    pub proof_id: Option<Digest>,
    pub escrow_debit: u64,
    pub requester_refund: u64,
    pub producer_credit: u64,
    pub collected_original_slash: u64,
    pub finder: Option<Digest>,
    pub finder_credit: u64,
    pub burned: u64,
    pub release_producer_reserve: u64,
}
impl ExecutionSettlementV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-settlement/v11", self)
    }
    pub fn check(&self) -> Result<(), String> {
        let debits = self.escrow_debit.checked_add(self.collected_original_slash).ok_or("execution settlement debit overflow")?;
        let credits = self
            .requester_refund
            .checked_add(self.producer_credit)
            .and_then(|n| n.checked_add(self.finder_credit))
            .and_then(|n| n.checked_add(self.burned))
            .ok_or("execution settlement payout overflow")?;
        if debits != credits
            || self.finder_credit > self.collected_original_slash
            || self.burned > self.collected_original_slash
            || self.finder_credit.checked_add(self.burned) != Some(self.collected_original_slash)
            || self.finder_credit != 0 && self.finder.is_none()
        {
            return Err("execution settlement lacks conserved original funding".into());
        }
        Ok(())
    }
}
/// Native stores one row per job and one monotone counter per requester operator.
/// It applies transition plus actual UTXO credits atomically with quota receipts.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionJobStateV11 {
    job: ExecutionJobRecordV11,
    escrow_remaining: u64,
    claim: Option<ExecutionClaimRecordV11>,
    terminal: ExecutionTerminalV11,
    settlement: Option<ExecutionSettlementV11>,
}
impl ExecutionJobStateV11 {
    /// Expected funded-job root comes from authenticated native state, never
    /// from the bytes being restored. The grammar contains no variable vectors.
    pub fn from_bytes<C: ExecutionClassContextV11>(
        bytes: &[u8],
        class: &C,
        expected_job_root: Digest,
        expected_state_root: Digest,
    ) -> Result<Self, String> {
        if bytes.len() > EXECUTION_STATE_MAX_BYTES_V11 {
            return Err("execution state exceeds fixed wire cap".into());
        }
        let state: Self = borsh::from_slice(bytes).map_err(|_| "invalid fixed execution state")?;
        if state.root() != expected_state_root {
            return Err("execution state differs from authenticated native anchor".into());
        }
        state.check_restored(class, expected_job_root)?;
        Ok(state)
    }
    pub fn check_restored<C: ExecutionClassContextV11>(&self, class: &C, expected_job_root: Digest) -> Result<(), String> {
        let record = class.record();
        self.class(record)?;
        let j = &self.job;
        j.input.check(class)?;
        if j.root() != expected_job_root
            || j.version != 11
            || j.reward == 0
            || j.reward > record.reward_cap
            || j.escrow_original_debit != j.reward
            || j.admitted_daa.checked_add(record.horizons.commit_window) != Some(j.commit_deadline)
            || j.admitted_daa.checked_add(record.horizons.maximum_job_horizon()?) != Some(j.deadline_daa)
        {
            return Err("restored execution job funding, identity or clock differs".into());
        }
        if let Some(c) = &self.claim
            && (c.job_id != j.id()
                || c.producer != j.producer
                || c.committed_daa < j.admitted_daa
                || c.committed_daa > j.commit_deadline
                || c.reserved_original != record.required_producer_reserve
                || c.committed_daa.checked_add(record.horizons.challenge_window) != Some(c.challenge_end)
                || c.challenge_end.checked_add(record.horizons.tail()?) != Some(c.retention_until)
                || c.retention_until >= j.deadline_daa
                || c.terminal != self.terminal
                || c.credit_paid != (self.terminal == ExecutionTerminalV11::Final))
        {
            return Err("restored execution claim differs from its original funded job".into());
        }
        match (&self.settlement, self.terminal) {
            (None, ExecutionTerminalV11::Funded | ExecutionTerminalV11::Challengeable) => {
                if self.escrow_remaining != j.reward || (self.terminal == ExecutionTerminalV11::Challengeable) != self.claim.is_some()
                {
                    return Err("restored live execution escrow/claim mismatch".into());
                }
            }
            (Some(s), terminal) if !matches!(terminal, ExecutionTerminalV11::Funded | ExecutionTerminalV11::Challengeable) => {
                s.check()?;
                if self.escrow_remaining != 0
                    || s.job_id != j.id()
                    || s.claim_id != self.claim.as_ref().map(|c| c.id())
                    || s.escrow_debit != j.reward
                    || s.terminal != terminal
                    || s.release_producer_reserve != self.claim.as_ref().map_or(0, |c| c.reserved_original)
                    || s.collected_original_slash > s.release_producer_reserve
                {
                    return Err("restored terminal execution settlement mismatch".into());
                }
                if terminal == ExecutionTerminalV11::Final {
                    if self.claim.is_none()
                        || s.producer_credit != j.reward
                        || s.requester_refund != 0
                        || s.collected_original_slash != 0
                        || s.proof_id.is_some()
                        || s.finder.is_some()
                    {
                        return Err("restored Final has invalid reward provenance".into());
                    }
                } else if s.producer_credit != 0
                    || s.requester_refund != j.reward
                    || (terminal == ExecutionTerminalV11::Cancelled) != self.claim.is_none()
                    || (terminal == ExecutionTerminalV11::Cancelled) != s.proof_id.is_none()
                {
                    return Err("restored refund has invalid execution provenance".into());
                }
            }
            _ => return Err("restored execution terminal state has no conserved settlement".into()),
        }
        Ok(())
    }
    pub fn from_native_funded_job(job: ExecutionJobRecordV11) -> Result<Self, String> {
        if job.version != 11
            || job.reward == 0
            || job.escrow_original_debit != job.reward
            || job.input.version != 11
            || job.admitted_daa > job.commit_deadline
            || job.commit_deadline >= job.deadline_daa
        {
            return Err("invalid native funded execution job".into());
        }
        Ok(Self { escrow_remaining: job.reward, job, claim: None, terminal: ExecutionTerminalV11::Funded, settlement: None })
    }
    pub fn job(&self) -> &ExecutionJobRecordV11 {
        &self.job
    }
    pub fn claim(&self) -> Option<&ExecutionClaimRecordV11> {
        self.claim.as_ref()
    }
    pub fn terminal(&self) -> ExecutionTerminalV11 {
        self.terminal
    }
    pub fn escrow_remaining(&self) -> u64 {
        self.escrow_remaining
    }
    pub fn settlement(&self) -> Option<&ExecutionSettlementV11> {
        self.settlement.as_ref()
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/execution-job-state/v11", self)
    }
    fn class(&self, class: &ExecutionClassRecordV11) -> Result<(), String> {
        if class.version != 11 || class.id() != self.job.class_id {
            return Err("execution transition uses another class".into());
        }
        Ok(())
    }
    /// Native signer, actual reserve and reservation receipt are authenticated
    /// before this call. Exactly one claim may occupy the original funded job.
    pub fn commit(
        &mut self,
        class: &ExecutionClassRecordV11,
        signer: Digest,
        trace: &NativeProtectedTraceV11,
        current_daa: u64,
        actual_reserved_original: u64,
        reservation_receipt: Digest,
    ) -> Result<(), String> {
        self.class(class)?;
        let j = &self.job;
        if self.terminal != ExecutionTerminalV11::Funded
            || self.claim.is_some()
            || self.settlement.is_some()
            || signer != j.producer
            || actual_reserved_original != class.required_producer_reserve
            || current_daa < j.admitted_daa
            || current_daa > j.commit_deadline
            || trace.candidate9_id != class.candidate9_id
            || trace.binding_id != class.binding_id
            || trace.class_scope9_root != class.class_scope9_root
            || trace.replay9_root != class.replay9_root
            || trace.class_resource10_root != class.resource10_root
            || trace.prompt_root != j.input.prompt_root
            || trace.prompt_len != j.input.prompt_len
            || trace.positions != j.input.positions
            || trace.generated_count != j.input.generated_count
        {
            return Err("execution claim is duplicate, unfunded, late or differs from its exact job".into());
        }
        let challenge_end = current_daa.checked_add(class.horizons.challenge_window).ok_or("claim challenge overflow")?;
        let retention_until = challenge_end.checked_add(class.horizons.tail()?).ok_or("claim retention overflow")?;
        if retention_until >= j.deadline_daa {
            return Err("claim liability exceeds admitted quota horizon".into());
        }
        self.claim = Some(ExecutionClaimRecordV11 {
            job_id: j.id(),
            producer: signer,
            vector: trace.vector,
            trace_root: trace.trace_root,
            output_root: trace.output_root,
            final_state_root: trace.final_state_root,
            committed_daa: current_daa,
            challenge_end,
            retention_until,
            reserved_original: actual_reserved_original,
            reservation_receipt,
            terminal: ExecutionTerminalV11::Challengeable,
            credit_paid: false,
        });
        self.terminal = ExecutionTerminalV11::Challengeable;
        Ok(())
    }
    fn finish(&mut self, settlement: ExecutionSettlementV11) -> Result<ExecutionSettlementV11, String> {
        settlement.check()?;
        if self.settlement.is_some()
            || settlement.job_id != self.job.id()
            || settlement.escrow_debit != self.escrow_remaining
            || self.escrow_remaining != self.job.escrow_original_debit
        {
            return Err("execution escrow already spent or mismatched".into());
        }
        if let Some(claim) = self.claim.as_mut() {
            if settlement.claim_id != Some(claim.id()) {
                return Err("settlement uses another execution claim".into());
            }
            claim.terminal = settlement.terminal;
            claim.credit_paid = settlement.producer_credit != 0;
        } else if settlement.claim_id.is_some() {
            return Err("settlement names absent claim".into());
        }
        self.escrow_remaining = 0;
        self.terminal = settlement.terminal;
        self.settlement = Some(settlement.clone());
        Ok(settlement)
    }
    pub fn finalize(
        &mut self,
        class: &ExecutionClassRecordV11,
        close: &NativeCloseSnapshotV11,
    ) -> Result<ExecutionSettlementV11, String> {
        self.class(class)?;
        let c = self.claim.as_ref().ok_or("no execution claim to Final")?;
        if self.terminal != ExecutionTerminalV11::Challengeable
            || c.credit_paid
            || c.terminal != self.terminal
            || close.current_daa <= c.retention_until
            || close.pending_own_proofs != 0
            || close.open_da != 0
            || close.current_daa <= close.latest_answer_proof_until
            || close.current_daa <= close.required_retained_until
            || close.required_retained_until < c.retention_until
        {
            return Err("execution Final requires all liability, evidence and answer grace to close".into());
        }
        self.finish(ExecutionSettlementV11 {
            job_id: self.job.id(),
            claim_id: Some(c.id()),
            terminal: ExecutionTerminalV11::Final,
            proof_id: None,
            escrow_debit: self.escrow_remaining,
            requester_refund: 0,
            producer_credit: self.job.reward,
            collected_original_slash: 0,
            finder: None,
            finder_credit: 0,
            burned: 0,
            release_producer_reserve: c.reserved_original,
        })
    }
    /// `collected_original_slash` is the actual collected native debit, not the
    /// promised reservation. Finder and burn are funded only by that collection.
    pub fn adjudicate(
        &mut self,
        class: &ExecutionClassRecordV11,
        result: &NativeExecutionJudgmentV11,
        collected_original_slash: u64,
    ) -> Result<ExecutionSettlementV11, String> {
        self.adjudicate_inner(class, result, None, collected_original_slash)
    }
    /// Collect an already verified immutable court. Original inclusion must be
    /// within this exact claim's liability; current time cannot backdate a new
    /// proof. Native authenticates the original court row before minting this
    /// witness and applies the once-only book transition and actual payout.
    pub fn adjudicate_established(
        &mut self,
        class: &ExecutionClassRecordV11,
        result: &NativeExecutionJudgmentV11,
        original_court: &NativeEstablishedExecutionCourtV11,
        collected_original_slash: u64,
    ) -> Result<ExecutionSettlementV11, String> {
        if !original_court.matches(result) {
            return Err("historical settlement differs from exact original verified native court".into());
        }
        self.adjudicate_inner(class, result, Some(original_court), collected_original_slash)
    }
    fn adjudicate_inner(
        &mut self,
        class: &ExecutionClassRecordV11,
        result: &NativeExecutionJudgmentV11,
        original_court: Option<&NativeEstablishedExecutionCourtV11>,
        collected_original_slash: u64,
    ) -> Result<ExecutionSettlementV11, String> {
        self.class(class)?;
        let c = self.claim.as_ref().ok_or("no execution claim to adjudicate")?;
        let eligible_finder = original_court.map(|court| court.eligible_finder).unwrap_or(Some(result.finder));
        if self.terminal != ExecutionTerminalV11::Challengeable
            || c.credit_paid
            || result.job_id != self.job.id()
            || result.claim_id != c.id()
            || result.vector != c.vector
            || result.class_id != class.id()
            || result.inventory_root != class.inventory_root
            || result.parameter_root != class.parameter_root
            || result.established_daa < c.committed_daa
            || result.established_daa > c.retention_until
            || result.current_daa < result.established_daa
            || result.current_daa > c.retention_until
                && result.kind != NativeAdjudicationKindV11::DataUnavailable
                && original_court.is_none()
            || eligible_finder == Some(self.job.producer)
            || collected_original_slash > c.reserved_original
        {
            return Err("native execution judgment subject, liability or collected original funds differ".into());
        }
        let terminal = match result.kind {
            NativeAdjudicationKindV11::ComputationFault => ExecutionTerminalV11::ComputationFault,
            NativeAdjudicationKindV11::SourceMismatch => ExecutionTerminalV11::SourceMismatch,
            NativeAdjudicationKindV11::DataUnavailable => ExecutionTerminalV11::DataUnavailable,
        };
        let finder_credit = if eligible_finder.is_some() {
            u64::try_from((collected_original_slash as u128 * class.finder_permille as u128) / 1000)
                .map_err(|_| "finder amount overflow")?
        } else {
            0
        };
        self.finish(ExecutionSettlementV11 {
            job_id: self.job.id(),
            claim_id: Some(c.id()),
            terminal,
            proof_id: Some(result.proof_id),
            escrow_debit: self.escrow_remaining,
            requester_refund: self.escrow_remaining,
            producer_credit: 0,
            collected_original_slash,
            finder: eligible_finder,
            finder_credit,
            burned: collected_original_slash.checked_sub(finder_credit).ok_or("finder exceeds collected slash")?,
            release_producer_reserve: c.reserved_original,
        })
    }
    pub fn cancel_unclaimed(&mut self, class: &ExecutionClassRecordV11, current_daa: u64) -> Result<ExecutionSettlementV11, String> {
        self.class(class)?;
        if self.terminal != ExecutionTerminalV11::Funded || self.claim.is_some() || current_daa <= self.job.commit_deadline {
            return Err("unclaimed execution job cannot refund yet".into());
        }
        self.finish(ExecutionSettlementV11 {
            job_id: self.job.id(),
            claim_id: None,
            terminal: ExecutionTerminalV11::Cancelled,
            proof_id: None,
            escrow_debit: self.escrow_remaining,
            requester_refund: self.escrow_remaining,
            producer_credit: 0,
            collected_original_slash: 0,
            finder: None,
            finder_credit: 0,
            burned: 0,
            release_producer_reserve: 0,
        })
    }
}
