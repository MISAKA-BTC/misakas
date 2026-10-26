//! Stage14 wrappers over frozen quota11 arithmetic. Maximum graph court bounds,
//! actual job resources and original V2 real-weight source bounds are separate
//! independent inputs. Only original native funding can create a baseline.
use super::*;
use crate::{
    flat_execution11::{ExecutionPolicyV11, canonical_nonce_v11, native_requester_nonce_counter_root_v11},
    flat_inputs12::{
        bounded14::{CheckedStageJobV14, FlatStageAdmissionV14, FlatStageJobV14},
        source14::CheckedStageClassV14,
    },
};
pub mod bounded28;
pub mod source26;

pub struct CheckedStageQuotaV14 {
    class: Digest,
    source: Digest,
    admission: Digest,
    policy: Digest,
    plan: CheckedJobQuotaPlanV11,
}
impl CheckedStageQuotaV14 {
    pub fn plan(&self) -> &CheckedJobQuotaPlanV11 {
        &self.plan
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/stage-quota-plan/v14",
            &(14u16, self.class, self.source, self.admission, self.policy, self.plan.record().root()),
        )
    }
    /// Readonly native fixed-record restoration. Expected wrapper root is
    /// authenticated by original native membership; no carrier may supply it.
    /// This grants no admission, baseline, source service or payout authority.
    pub fn restore_native_record(
        class: Digest,
        source: Digest,
        admission: Digest,
        policy: Digest,
        record: &QuotaPlanRecordV11,
        expected: Digest,
    ) -> Result<Self, String> {
        let plan = CheckedJobQuotaPlanV11::restore(record, &record.root())?;
        let checked = Self { class, source, admission, policy, plan };
        if class == [0; 64] || source == [0; 64] || admission == [0; 64] || policy == [0; 64] || checked.root() != expected {
            return Err("stage quote cache differs from authenticated original wrapper".into());
        }
        Ok(checked)
    }
}
/// Price source delivery over the ORIGINAL real-weight geometry and maximum
/// relation operands. Actual external job inputs do not become source weights.
/// Four retained owners plus the immutable public request lifetime are bounded
/// independently of frozen DA11's eight attempts.
pub fn derive_stage_source_plan_v14(
    class: &CheckedStageClassV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    discovery: &discovery13::DiscoveryPolicyV13,
) -> Result<source16::SourcePlanV16, String> {
    discovery.check(quota)?;
    if stage.class != class.root() || stage.source != class.source().id() || stage.policy != class.policy_root() {
        return Err("stage source delivery changes original class/source/policy".into());
    }
    let mut leaves = 1u64;
    for param in &class.source().program().params {
        let shape: Vec<_> = param.shape.iter().map(|n| *n as usize).collect();
        let layout = crate::merkle3::LayoutV3::try_of(&shape).ok_or("stage real source layout overflow")?;
        leaves = leaves.max(layout.leaves(crate::merkle::AXIS_ROW)).max(layout.leaves(crate::merkle::AXIS_COL));
    }
    let depth = if leaves <= 1 { 0 } else { 64 - (leaves - 1).leading_zeros() };
    let operands = class.maximum_scope().relations.iter().try_fold(0u64, |n, relation| {
        let count =
            relation.operand_leaves.iter().try_fold(0u64, |v, count| v.checked_add(*count).ok_or("stage operand count overflow"))?;
        Ok::<_, String>(n.max(count))
    })?;
    // Header, authenticated bisection replies, final bound-PC leaf and one
    // allowance for its real inventory piece. Computation operands are also
    // independently complete at maximum geometry.
    let each = (depth as u64).checked_add(3).ok_or("stage source depth overflow")?.max(operands);
    let requests = each
        .checked_mul(PROTECTED_SEALS_PER_JOB_V11 as u64)
        .and_then(|n| n.checked_add(discovery.ordinary_lifetime_requests as u64))
        .filter(|n| *n > 0 && *n <= 1 << 20)
        .ok_or("stage source lifetime request bound")? as u32;
    source16::derive_source_plan_base_v16(
        class.root(),
        class.source().parameters().root(),
        class.source().inventory_root(),
        class.source().parameters(),
        stage.plan(),
        quota,
        requests,
    )
}

pub fn derive_stage_trace_plan_v14(
    class: &CheckedStageClassV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
) -> Result<trace17::TracePlanV17, String> {
    if stage.class != class.root() || stage.source != class.source().id() || stage.policy != class.policy_root() {
        return Err("stage trace quote changes original source/class/policy".into());
    }
    let record = stage.plan.record();
    trace17::derive_trace_plan_base_v17(
        class.root(),
        record.program_root,
        record.parameter_root,
        class.maximum_scope().root(),
        stage.plan(),
        quota,
    )
}
pub(super) struct CheckedStageSourceGrantV14 {
    pub(super) binding: SubjectBindingV11,
    pub(super) consent: Digest,
    pub(super) producer_operator: Digest,
    pub(super) epoch: TrustedEpochV11,
    pub(super) source_plan: Digest,
}
pub(super) struct CheckedStageTraceGrantV14 {
    pub(super) binding: SubjectBindingV11,
    pub(super) consent: Digest,
    pub(super) epoch: TrustedEpochV11,
    pub(super) trace_plan: Digest,
}
pub fn check_stage_trace_admission_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    epoch: &TrustedEpochV11,
    trace: &trace17::TracePlanV17,
    native: &NativeStageQuotaAnchorV14,
) -> Result<trace17::CheckedTraceAdmissionV17, String> {
    epoch.check(quota)?;
    book.check_restored(class, stage)?;
    let grant = check_stage_quota_admission_v14(class, book, stage, quota, native)?;
    if epoch.block_hash != native.block
        || epoch.block_daa != native.daa
        || epoch.network_interval != book.admitted_interval
        || book.trace_plan != trace.root()
        || derive_stage_trace_plan_v14(class, stage, quota)?.root() != trace.root()
    {
        return Err("stage trace supplement lacks original consent18/funded epoch/checked quote".into());
    }
    trace17::from_checked_stage_admission_v17(
        CheckedStageTraceGrantV14 { binding: grant.binding, consent: book.acceptance, epoch: epoch.clone(), trace_plan: trace.root() },
        trace,
    )
}
pub fn check_stage_source_admission_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    discovery: &discovery13::DiscoveryPolicyV13,
    epoch: &TrustedEpochV11,
    source: &source16::SourcePlanV16,
    native: &NativeStageQuotaAnchorV14,
) -> Result<source16::CheckedSourceAdmissionV16, String> {
    epoch.check(quota)?;
    book.check_restored(class, stage)?;
    let granted = check_stage_quota_admission_v14(class, book, stage, quota, native)?;
    if epoch.block_daa != native.daa
        || epoch.block_hash != native.block
        || epoch.network_interval != book.admitted_interval
        || derive_stage_source_plan_v14(class, stage, quota, discovery)?.root() != source.root()
        || book.source_plan != source.root()
        || book.trace_plan != derive_stage_trace_plan_v14(class, stage, quota)?.root()
        || book.producer_operator == [0; 64]
    {
        return Err("stage source obligation lacks original funded epoch/real source/consent lineage".into());
    }
    source16::from_checked_stage_admission_v16(
        CheckedStageSourceGrantV14 {
            binding: granted.binding,
            consent: book.acceptance,
            producer_operator: book.producer_operator,
            epoch: epoch.clone(),
            source_plan: source.root(),
        },
        source,
    )
}
/// Native obtains these lengths from immutable bounded headers and prepays
/// before restoring the original source/class/book. This quote supplies only
/// ordinary work, never a baseline, seal, source obligation or payout grant.
pub fn derive_stage_restore_metadata_v14(
    source_bytes: u32,
    class_bytes: u32,
    book_bytes: u32,
    input_bytes: u64,
    carrier_bytes: u32,
) -> Result<DerivedOrdinaryMetadataV11, String> {
    let lengths = [source_bytes as u64, class_bytes as u64, book_bytes as u64, input_bytes, carrier_bytes as u64];
    let (mut work, mut bytes) = (0u128, 0u64);
    for n in lengths {
        bytes = bytes.checked_add(n).ok_or("stage restore bytes overflow")?;
        let n = n as u128;
        work = work
            .checked_add(n.checked_mul(n).and_then(|v| v.checked_mul(64)).ok_or("stage restore square overflow")?)
            .and_then(|v| v.checked_add(n.checked_mul(1024)?))
            .ok_or("stage restore work overflow")?;
    }
    let mut quantities = action_envelope(work.checked_add(65536).ok_or("stage restore work overflow")?, bytes, None)?;
    include_native_post_io_v11(&mut quantities, bytes)?;
    Ok(DerivedOrdinaryMetadataV11 { quantities })
}
/// An independent public source filing spends only its caller's ordinary pool.
/// A valid result can deny fresh aliases; it grants no funded-job conviction,
/// reserved seal, finder award or escrow authority.
pub fn derive_stage_source_filing_v14(
    source: &crate::flat_inputs12::source14::CheckedStageSourceV14,
    carrier_bytes: u64,
) -> Result<DerivedOrdinaryMetadataV11, String> {
    let bound = crate::flat_execution11::native_source_court_bound_v11(source.program().encode().len(), source.parameters())?;
    if carrier_bytes > bound.max_proof_bytes.checked_add(70).ok_or("stage source filing extent overflow")? {
        return Err("stage source public filing exceeds original real-PC quote".into());
    }
    let mut q = action_envelope(bound.work as u128, carrier_bytes, Some((bound.work, bound.max_proof_bytes, bound.scratch_bytes)))?;
    include_source_court_io_v11(&mut q, source.parameters())?;
    Ok(DerivedOrdinaryMetadataV11 { quantities: q })
}
/// Exact requester-owned image decoding, interval validation and commitment
/// work. Charge before acquiring images so invalid inputs cannot consume an
/// honest producer's guaranteed baseline or answers.
pub fn derive_stage_input_metadata_v14(job: &CheckedStageJobV14) -> Result<DerivedOrdinaryMetadataV11, String> {
    let (mut raw, mut elements) = (0u128, 0u128);
    for d in &job.program().inputs {
        let cells = d.shape.iter().try_fold(1u128, |n, x| n.checked_mul(*x as u128)).ok_or("stage input elements overflow")?;
        elements = elements.checked_add(cells).ok_or("stage input count overflow")?;
        raw = raw
            .checked_add(cells.checked_mul(d.dtype.width() as u128).ok_or("stage input raw overflow")?)
            .ok_or("stage input bytes overflow")?;
    }
    let carrier = u64::try_from(raw)
        .map_err(|_| "stage input carrier overflow")?
        .checked_add(ACTION_OUTER_ALLOWANCE_V11)
        .ok_or("stage input metadata carrier overflow")?;
    let mut q = action_envelope(raw.checked_mul(1024).ok_or("stage input validation work overflow")?, carrier, None)?;
    for (axis, count) in [
        (ResourceAxisV10::ValidatedElements, elements),
        (ResourceAxisV10::ParameterHashBytes, raw * 4),
        (ResourceAxisV10::RawParameterStorage, raw),
        (ResourceAxisV10::CopiedReferenceBytes, elements * 32),
        (ResourceAxisV10::ReferenceTensorPayload, elements * 16),
    ] {
        q.set(axis, q.get(axis).checked_add(count).ok_or("stage input resource addition overflow")?);
    }
    Ok(DerivedOrdinaryMetadataV11 { quantities: q })
}
pub fn derive_stage_quota_v14(class: &CheckedStageClassV14, job: &CheckedStageJobV14) -> Result<CheckedStageQuotaV14, String> {
    let own = class.check_job(job.job())?;
    if own.admission() != job.admission() {
        return Err("stage quota differs from exact checked class/job".into());
    }
    let program = job.program().v1_view();
    let pc = job.combined_parameters();
    let actual = derive_flat_resources_v10(&program, pc, job.job().positions, None)?;
    if actual.vector != *job.resources() || actual.court != *job.scope() {
        return Err("stage quota actual graph/resources independently differ".into());
    }
    let maximum = class.maximum_scope();
    let court = complete_scope_api_bounds_v9(maximum).map_err(|e| format!("{e:?}"))?;
    let source = class.source();
    // Original V2 bytes and real/source PC, never actual virtual input params.
    let source_bound = crate::flat_execution11::native_source_court_bound_v11(source.program().encode().len(), source.parameters())?;
    let filing = court.filing_bytes.max(source_bound.max_proof_bytes).checked_add(7).ok_or("stage court filing overflow")?;
    let work = court.work.max(source_bound.work);
    let scratch = court.scratch_bytes.max(source_bound.scratch_bytes);
    if filing > crate::flat_court4::FLAT_FAULT_MAX_BYTES_V4
        || work > crate::flat_court4::FLAT_FAULT_MAX_WORK_V4
        || scratch > crate::flat_court4::FLAT_FAULT_MAX_SCRATCH_V4
    {
        return Err("stage court exceeds fixed checker/court capacities".into());
    }
    let assembled = filing.checked_add(ACTION_OUTER_ALLOWANCE_V11).ok_or("stage proof carrier overflow")?;
    let source_bytes = source.program().encode().len() as u64;
    let source_wire = 2u64
        + 4
        + source.program().encode().len() as u64
        + 64
        + borsh::to_vec(source.parameters()).map_err(|e| e.to_string())?.len() as u64;
    let class_wire = 2u64 + 64 + 4 + class.template().lengths.to_bytes()?.len() as u64 + 4 + 64;
    let fixed_restore = derive_stage_restore_metadata_v14(
        u32::try_from(source_wire).map_err(|_| "stage source cache extent overflow")?,
        u32::try_from(class_wire).map_err(|_| "stage class cache extent overflow")?,
        4096,
        0,
        0,
    )?;
    let restore = native_candidate_restore_work_from_scope_v11(&program, pc, maximum)?
        .checked_add(source_bytes.checked_mul(source_bytes).ok_or("stage source restore work overflow")?)
        .and_then(|n| n.checked_add(source_bytes.checked_mul(128)?))
        .and_then(|n| n.checked_add(u64::try_from(fixed_restore.quantities().get(ResourceAxisV10::MetadataWork)).ok()?))
        .ok_or("stage restore work overflow")?;
    let output_count = match class.template().program.output {
        misaka_palw_tir::program_v2::OutputDecl::Rows { .. } => job.job().positions as u64,
        misaka_palw_tir::program_v2::OutputDecl::Final { .. } => 1,
        _ => return Err("stage quota has unsupported output semantics".into()),
    };
    let prepared = crate::element::PreparedV1::without_scope(&program, None)?;
    let opening_bytes = crate::element::node_opening_bytes_v1(prepared.node_count);
    let post = ACTION_OUTER_ALLOWANCE_V11
        .checked_add(output_count.checked_mul(opening_bytes).ok_or("stage output opening carrier overflow")?)
        .and_then(|n| n.checked_add((job.job().positions as u64).div_ceil(crate::seg::SEG_LEN_V4 as u64) * 64))
        .and_then(|n| n.checked_add(job.job().input_roots.len() as u64 * 64))
        .ok_or("stage post carrier overflow")?;
    let metadata_carrier = (program.encode().len() as u64 + pc.by_instance.len() as u64 * 80 + ACTION_OUTER_ALLOWANCE_V11).max(post);
    let raw_inputs = job
        .program()
        .inputs
        .iter()
        .try_fold(0u128, |n, d| {
            d.shape.iter().try_fold(d.dtype.width() as u128, |n, x| n.checked_mul(*x as u128)).and_then(|b| n.checked_add(b))
        })
        .ok_or("stage native input restore bytes overflow")?;
    let metadata_work = actual
        .vector
        .quantities
        .get(ResourceAxisV10::MetadataWork)
        .checked_add((post as u128 + raw_inputs + 4096).checked_mul(128).ok_or("stage native post metadata overflow")?)
        .and_then(|n| n.checked_add(restore as u128))
        .ok_or("stage native restore metadata overflow")?;
    let mut actions = [
        action_envelope(metadata_work, metadata_carrier, None)?,
        action_envelope(16 << 10, MAX_DA_REQUEST_BYTES_V11, None)?,
        action_envelope(16 << 10, MAX_DA_ANSWER_BYTES_V11, None)?,
        action_envelope(native_action_metadata_v11(restore, post, assembled, false)?, assembled, Some((work, filing, scratch)))?,
        action_envelope(native_action_metadata_v11(restore, post, assembled, true)?, assembled, Some((work, filing, scratch)))?,
        action_envelope(16 << 10, MAX_FIXED_ACTION_BYTES_V11, None)?,
    ];
    for axis in ResourceAxisV10::ALL {
        if axis != ResourceAxisV10::MetadataWork {
            actions[0].set(
                axis,
                actions[0]
                    .get(axis)
                    .checked_add(fixed_restore.quantities().get(axis))
                    .ok_or("stage original restore axis overflow")?,
            );
        }
    }
    let pc_bytes = borsh::to_vec(pc).map_err(|e| e.to_string())?.len() as u128;
    actions[0].set(
        ResourceAxisV10::ParameterHashBytes,
        actions[0]
            .get(ResourceAxisV10::ParameterHashBytes)
            .checked_add(pc_bytes.checked_mul(64).ok_or("stage repeated PC hash extent overflow")?)
            .ok_or("stage PC hash quota overflow")?,
    );
    include_native_post_io_v11(&mut actions[0], post)?;
    for (axis, extra) in [
        (ResourceAxisV10::CopiedReferenceBytes, raw_inputs * 3),
        (ResourceAxisV10::OwnedPeakBytes, raw_inputs * 4),
        (ResourceAxisV10::StoredEvidenceBytes, post as u128),
        (ResourceAxisV10::ParameterHashBytes, raw_inputs * 4),
    ] {
        actions[0].set(axis, actions[0].get(axis).checked_add(extra).ok_or("stage native metadata resource overflow")?);
    }
    for a in &mut actions[3..5] {
        include_native_post_io_v11(a, post)?;
        include_source_court_io_v11(a, source.parameters())?;
    }
    let plan = CheckedJobQuotaPlanV11 {
        program_root: crate::public::program_root_v1(&program.encode()),
        parameter_root: pc.root(),
        resource_root: actual.vector.root(),
        class_scope_root: maximum.root(),
        actual_scope_root: actual.court.root(),
        native_restore_work: restore,
        native_post_carrier_cap: post,
        source_court_bound: source_bound,
        positions: job.job().positions,
        baseline: QuotaQuantitiesV11::from(&actual.vector.quantities),
        action_envelopes: actions,
        action_carrier_caps: [
            metadata_carrier,
            MAX_DA_REQUEST_BYTES_V11,
            MAX_DA_ANSWER_BYTES_V11,
            assembled,
            assembled,
            MAX_FIXED_ACTION_BYTES_V11,
        ],
    };
    Ok(CheckedStageQuotaV14 {
        class: class.root(),
        source: source.id(),
        admission: job.admission().root(),
        policy: class.policy_root(),
        plan,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StageTerminalV14 {
    Funded,
    Challengeable,
    Refunded,
    Convicted,
    Final,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StageAdjudicationKindV14 {
    ComputationMismatch,
    SourceMismatch,
    SourceHeaderMismatch,
    BoundSourceUnavailable,
}
/// Native constructs this only after the independently verified original paid
/// proof or original producer-bound pending PC unit default. A negative source
/// fact by itself cannot supply this authenticated per-job adjudication.
pub struct NativeStageJudgmentV14 {
    pub job: Digest,
    pub original_book: Digest,
    pub book: Digest,
    pub class: Digest,
    pub claim: Digest,
    pub source: Digest,
    pub source_parameters: Digest,
    pub inventory: Digest,
    pub consent18: Digest,
    pub proof: Digest,
    pub finder: Digest,
    pub finder_operator: Digest,
    pub established_daa: u64,
    pub current_daa: u64,
    pub kind: StageAdjudicationKindV14,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageSettlementV14 {
    pub version: u16,
    pub job: Digest,
    pub class: Digest,
    pub claim: Digest,
    pub source: Digest,
    pub proof: Digest,
    pub terminal: StageTerminalV14,
    pub kind: StageAdjudicationKindV14,
    pub escrow_debit: u64,
    pub requester_refund: u64,
    pub producer_credit: u64,
    pub collected_original_slash: u64,
    pub finder: Digest,
    pub finder_credit: u64,
    pub burned: u64,
    pub established_daa: u64,
}
impl StageSettlementV14 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-original-settlement/v14", self)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.version != 14
            || self.terminal != StageTerminalV14::Convicted
            || self.job == [0; 64]
            || self.class == [0; 64]
            || self.claim == [0; 64]
            || self.source == [0; 64]
            || self.proof == [0; 64]
            || self.finder == [0; 64]
            || self.escrow_debit == 0
            || self.requester_refund != self.escrow_debit
            || self.producer_credit != 0
            || self.collected_original_slash == 0
            || self.finder_credit.checked_add(self.burned) != Some(self.collected_original_slash)
        {
            return Err("stage terminal settlement loses original escrow/slash conservation".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageJobBookV14 {
    version: u16,
    class: Digest,
    policy: Digest,
    source: Digest,
    job: FlatStageJobV14,
    acceptance: Digest,
    quota_policy: Digest,
    execution_policy: Digest,
    source_plan: Digest,
    trace_plan: Digest,
    fee_plan: Digest,
    availability_terms: Digest,
    admission: FlatStageAdmissionV14,
    quota_plan: Digest,
    requester_operator: Digest,
    producer_operator: Digest,
    admitted_daa: u64,
    admitted_interval: u64,
    commit_deadline_daa: u64,
    deadline_daa: u64,
    reward: u64,
    debit_receipt: Digest,
    original_debit: u64,
    escrow_remaining: u64,
    terminal: StageTerminalV14,
    claim_root: Option<Digest>,
    committed_daa: Option<u64>,
    challenge_end_daa: Option<u64>,
    producer_reserved: u64,
    settlement: Option<StageSettlementV14>,
}
/// Native supplies these from the original class/source/policy and real coins.
/// This is never a wire-deserialized grant or an availability assertion.
pub struct NativeStageFundingV14 {
    pub class: Digest,
    pub source: Digest,
    pub policy: Digest,
    pub acceptance: Digest,
    pub consent_version: u16,
    pub source_plan: Digest,
    pub trace_plan: Digest,
    pub fee_plan: Digest,
    pub availability_terms: Digest,
    pub source_matures_daa: u64,
    pub source_refuted: bool,
    pub requester: Digest,
    pub requester_operator: Digest,
    pub producer: Digest,
    pub producer_operator: Digest,
    pub block: Digest,
    pub daa: u64,
    pub reward: u64,
    pub original_debit: u64,
    pub next_nonce: u64,
    pub nonce_counter_root: Digest,
}
impl StageJobBookV14 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/funded-stage-job/v14", &(14u16, self.class, self.job.root(), self.reward))
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/funded-stage-book/v14", self)
    }
    pub fn job(&self) -> &FlatStageJobV14 {
        &self.job
    }
    pub fn admission(&self) -> &FlatStageAdmissionV14 {
        &self.admission
    }
    pub fn class_id(&self) -> Digest {
        self.class
    }
    pub fn source_binding(&self) -> Digest {
        self.source
    }
    pub fn native_policy_root(&self) -> Digest {
        self.policy
    }
    pub fn execution_policy_root(&self) -> Digest {
        self.execution_policy
    }
    pub fn acceptance(&self) -> Digest {
        self.acceptance
    }
    pub fn source_plan_root(&self) -> Digest {
        self.source_plan
    }
    pub fn trace_plan_root(&self) -> Digest {
        self.trace_plan
    }
    pub fn fee_plan_root(&self) -> Digest {
        self.fee_plan
    }
    pub fn availability_terms_root(&self) -> Digest {
        self.availability_terms
    }
    pub fn requester_operator(&self) -> Digest {
        self.requester_operator
    }
    pub fn producer_operator(&self) -> Digest {
        self.producer_operator
    }
    pub fn deadline_daa(&self) -> u64 {
        self.deadline_daa
    }
    pub fn commit_deadline_daa(&self) -> u64 {
        self.commit_deadline_daa
    }
    pub fn challenge_end_daa(&self) -> Option<u64> {
        self.challenge_end_daa
    }
    pub fn committed_daa(&self) -> Option<u64> {
        self.committed_daa
    }
    /// Immutable original Fund projection, independently reconstructed from
    /// the private book rather than a caller-supplied funded-root label.
    pub fn original_funded_root(&self) -> Digest {
        let mut original = self.clone();
        original.escrow_remaining = original.original_debit;
        original.terminal = StageTerminalV14::Funded;
        original.claim_root = None;
        original.committed_daa = None;
        original.challenge_end_daa = None;
        original.producer_reserved = 0;
        original.settlement = None;
        original.root()
    }
    pub fn terminal(&self) -> StageTerminalV14 {
        self.terminal
    }
    pub fn escrow_remaining(&self) -> u64 {
        self.escrow_remaining
    }
    pub fn original_debit(&self) -> u64 {
        self.original_debit
    }
    pub fn debit_receipt(&self) -> Digest {
        self.debit_receipt
    }
    pub fn producer_reserved(&self) -> u64 {
        self.producer_reserved
    }
    pub fn settlement(&self) -> Option<&StageSettlementV14> {
        self.settlement.as_ref()
    }
    pub fn claim_root(&self) -> Option<Digest> {
        self.claim_root
    }
    pub fn from_rooted_bytes(raw: &[u8], expected: Digest) -> Result<Self, String> {
        const INPUT_COUNT_AT: usize = 2 + 3 * 64 + 2 + 4 * 64 + 3 * 4 + 4;
        if raw.len() > 4096 || raw.len() < INPUT_COUNT_AT + 4 {
            return Err("stage book exceeds bounded native frame".into());
        }
        let n = u32::from_le_bytes(raw[INPUT_COUNT_AT..INPUT_COUNT_AT + 4].try_into().expect("checked prefix")) as usize;
        if n == 0 || n > crate::flat_inputs12::INPUT_COUNT_MAX_V12 || n > raw.len().saturating_sub(INPUT_COUNT_AT + 4) / 64 {
            return Err("stage book input roots exceed bounded available frame".into());
        }
        let book = Self::try_from_slice(raw).map_err(|e| e.to_string())?;
        if book.version != 14 || book.job.version != 14 || book.root() != expected {
            return Err("stage book differs from original native anchor".into());
        }
        Ok(book)
    }
    pub fn check_restored(&self, class: &CheckedStageClassV14, plan: &CheckedStageQuotaV14) -> Result<(), String> {
        let actual = class.check_job(&self.job)?;
        if actual.admission() != &self.admission
            || derive_stage_quota_v14(class, &actual)?.root() != plan.root()
            || self.class != class.root()
            || self.policy != class.policy_root()
            || self.source != class.source().id()
            || self.quota_plan != plan.root()
            || self.original_debit != self.reward
            || self.reward == 0
            || self.acceptance == [0; 64]
            || self.quota_policy == [0; 64]
            || self.execution_policy == [0; 64]
            || self.source_plan == [0; 64]
            || self.trace_plan == [0; 64]
            || self.fee_plan == [0; 64]
            || self.availability_terms == [0; 64]
            || self.escrow_remaining > self.original_debit
            || self.admitted_daa >= self.commit_deadline_daa
            || self.commit_deadline_daa >= self.deadline_daa
        {
            return Err("stage book differs from independently derived original class/job/resources".into());
        }
        match self.terminal {
            StageTerminalV14::Funded
                if self.escrow_remaining == self.original_debit
                    && self.settlement.is_none()
                    && self.claim_root.is_none()
                    && self.producer_reserved == 0
                    && self.committed_daa.is_none()
                    && self.challenge_end_daa.is_none() =>
            {
                ()
            }
            StageTerminalV14::Challengeable
                if self.escrow_remaining == self.original_debit
                    && self.settlement.is_none()
                    && self.claim_root.is_some()
                    && self.producer_reserved != 0
                    && self.committed_daa.is_some()
                    && self.challenge_end_daa.is_some() =>
            {
                ()
            }
            StageTerminalV14::Refunded
                if self.escrow_remaining == 0
                    && self.settlement.is_none()
                    && self.claim_root.is_none()
                    && self.producer_reserved == 0
                    && self.committed_daa.is_none()
                    && self.challenge_end_daa.is_none() =>
            {
                ()
            }
            StageTerminalV14::Convicted if self.escrow_remaining == 0 && self.producer_reserved == 0 => {
                let s = self.settlement.as_ref().ok_or("stage conviction lacks original settlement")?;
                s.check()?;
                if s.job != self.id()
                    || s.class != self.class
                    || s.source != self.source
                    || Some(s.claim) != self.claim_root
                    || s.escrow_debit != self.original_debit
                {
                    return Err("stage conviction settlement changes original source/job/escrow".into());
                }
            }
            _ => return Err("stage terminal money/reserve state is inconsistent".into()),
        }
        Ok(())
    }
    /// Own producer Commit. Native authenticates its real free original bond
    /// reserve first and atomically persists this returned liability transition.
    pub fn commit(
        &mut self,
        class: &CheckedStageClassV14,
        plan: &CheckedStageQuotaV14,
        execution: &ExecutionPolicyV11,
        producer: Digest,
        available_original_reserve: u64,
        trace: &StageTraceV14,
        daa: u64,
    ) -> Result<u64, String> {
        self.check_restored(class, plan)?;
        if self.terminal != StageTerminalV14::Funded
            || execution.root() != self.execution_policy
            || producer != self.job.producer
            || available_original_reserve < execution.required_producer_reserve
            || daa < self.admitted_daa
            || daa >= self.commit_deadline_daa
            || trace.version != 14
            || trace.job_id != self.id()
            || trace.class != self.class
            || trace.job_input != self.job.root()
            || trace.admission != self.admission.root()
            || trace.quota != self.quota_plan
            || trace.segment_roots.len() != crate::seg::segment_count_v1(self.job.positions) as usize
            || trace.output_openings.len()
                != match class.template().program.output {
                    misaka_palw_tir::program_v2::OutputDecl::Rows { .. } => self.job.positions as usize,
                    misaka_palw_tir::program_v2::OutputDecl::Final { .. } => 1,
                    _ => return Err("stage trace has unsupported output semantics".into()),
                }
        {
            return Err("stage Commit differs from original own producer/job/output bounds".into());
        }
        let program = class.check_job(&self.job)?.program().v1_view();
        let prepared = crate::element::PreparedV1::without_scope(&program, None)?;
        let occurrence = u16::try_from(prepared.w.occurrences.len() - 1).map_err(|_| "stage output occurrence overflow")?;
        let node = class.template().program.output.node();
        for (i, opening) in trace.output_openings.iter().enumerate() {
            let position = match class.template().program.output {
                misaka_palw_tir::program_v2::OutputDecl::Rows { .. } => i as u32,
                _ => self.job.positions - 1,
            };
            if opening.position != position
                || opening.occurrence != occurrence
                || opening.node != node
                || !opening.authenticates(
                    &trace.segment_roots,
                    self.job.positions,
                    prepared.index(occurrence, node),
                    prepared.node_count,
                )
            {
                return Err("stage delivered output differs from its authenticated full graph node".into());
            }
        }
        self.claim_root = Some(trace.root());
        self.committed_daa = Some(daa);
        self.challenge_end_daa = Some(daa.checked_add(execution.horizons.challenge_window).ok_or("stage challenge end overflow")?);
        self.producer_reserved = execution.required_producer_reserve;
        self.terminal = StageTerminalV14::Challengeable;
        Ok(self.producer_reserved)
    }
    /// An expired unclaimed job returns only its actual original escrow once.
    /// A claimed job needs the protected court/DA close contract instead.
    pub fn refund_unclaimed(&mut self, daa: u64) -> Result<u64, String> {
        if self.terminal != StageTerminalV14::Funded
            || self.claim_root.is_some()
            || daa <= self.deadline_daa
            || self.escrow_remaining != self.original_debit
            || self.producer_reserved != 0
        {
            return Err("stage refund lacks its original expired unclaimed escrow".into());
        }
        let amount = self.escrow_remaining;
        self.escrow_remaining = 0;
        self.terminal = StageTerminalV14::Refunded;
        Ok(amount)
    }
    pub fn adjudicate(
        &mut self,
        class: &CheckedStageClassV14,
        plan: &CheckedStageQuotaV14,
        execution: &ExecutionPolicyV11,
        native: &NativeStageJudgmentV14,
        collected_original: u64,
    ) -> Result<StageSettlementV14, String> {
        self.check_restored(class, plan)?;
        if self.terminal != StageTerminalV14::Challengeable
            || self.settlement.is_some()
            || execution.root() != self.execution_policy
            || native.job != self.id()
            || native.original_book != self.original_funded_root()
            || native.book != self.root()
            || native.class != self.class
            || Some(native.claim) != self.claim_root
            || native.source != self.source
            || native.source_parameters != class.source().parameters().root()
            || native.inventory != class.source().inventory_root()
            || native.consent18 != self.acceptance
            || native.proof == [0; 64]
            || native.finder == [0; 64]
            || native.finder == self.job.producer
            || native.finder_operator == [0; 64]
            || native.finder_operator == self.producer_operator
            || native.established_daa < self.committed_daa.ok_or("stage conviction commit clock absent")?
            || native.established_daa > self.deadline_daa
            || native.current_daa < native.established_daa
            || collected_original != self.producer_reserved
            || collected_original != execution.required_producer_reserve
        {
            return Err("stage adjudication lacks exact original18 per-job proof/default/source/claim/collected liability".into());
        }
        let finder_credit = u64::try_from((collected_original as u128) * (execution.finder_permille as u128) / 1000)
            .map_err(|_| "stage finder division overflow")?;
        let settlement = StageSettlementV14 {
            version: 14,
            job: self.id(),
            class: self.class,
            claim: native.claim,
            source: self.source,
            proof: native.proof,
            terminal: StageTerminalV14::Convicted,
            kind: native.kind,
            escrow_debit: self.original_debit,
            requester_refund: self.original_debit,
            producer_credit: 0,
            collected_original_slash: collected_original,
            finder: native.finder,
            finder_credit,
            burned: collected_original - finder_credit,
            established_daa: native.established_daa,
        };
        settlement.check()?;
        self.escrow_remaining = 0;
        self.producer_reserved = 0;
        self.terminal = StageTerminalV14::Convicted;
        self.settlement = Some(settlement.clone());
        Ok(settlement)
    }
    pub fn quota_subject(&self, plan: &CheckedStageQuotaV14, policy: &QuotaPolicyV11) -> Result<SubjectBindingV11, String> {
        if self.version != 14
            || policy.root() != self.quota_policy
            || self.class != plan.class
            || self.source != plan.source
            || self.policy != plan.policy
            || self.admission.root() != plan.admission
            || self.quota_plan != plan.root()
        {
            return Err("stage subject differs from exact original funded class/admission/quota".into());
        }
        Ok(SubjectBindingV11 {
            version: 11,
            policy_root: policy.root(),
            class_id: self.class,
            class_scope9_root: plan.plan.class_scope_root,
            program_root: plan.plan.program_root,
            parameter_root: plan.plan.parameter_root,
            job_id: self.id(),
            job_input_root: self.job.root(),
            resource10_root: plan.plan.resource_root,
            quota_plan_root: plan.plan.record().root(),
            actual_positions: self.job.positions,
            requester: self.job.requester,
            producer: self.job.producer,
            admitted_interval: self.admitted_interval,
            deadline_daa: self.deadline_daa,
        })
    }
    pub fn fund(
        class: &CheckedStageClassV14,
        job: &CheckedStageJobV14,
        plan: &CheckedStageQuotaV14,
        execution: &ExecutionPolicyV11,
        quota: &QuotaPolicyV11,
        native: &NativeStageFundingV14,
    ) -> Result<Self, String> {
        execution.check(quota)?;
        let own = derive_stage_quota_v14(class, job)?;
        let nonce = canonical_nonce_v11(&job.job().nonce)?;
        if own.root() != plan.root()
            || class.root() != native.class
            || class.source().id() != native.source
            || class.policy_root() != native.policy
            || native.acceptance == [0; 64]
            || native.consent_version != 18
            || native.source_plan == [0; 64]
            || native.trace_plan == [0; 64]
            || native.fee_plan == [0; 64]
            || native.availability_terms == [0; 64]
            || native.source_refuted
            || native.source_matures_daa > native.daa
            || job.job().requester != native.requester
            || job.job().producer != native.producer
            || native.block == [0; 64]
            || native.reward == 0
            || native.reward > execution.reward_cap
            || native.original_debit != native.reward
            || native.next_nonce != nonce
            || native.next_nonce == u64::MAX
            || native.nonce_counter_root != native_requester_nonce_counter_root_v11(&native.requester_operator, native.next_nonce)
        {
            return Err("stage fund lacks original native source/nonce/principal/coin binding".into());
        }
        let interval = native.daa / quota.interval_daa;
        let deadline = native.daa.checked_add(execution.horizons.maximum_job_horizon()?).ok_or("stage horizon overflow")?;
        if deadline >= interval.checked_add(1).and_then(|n| n.checked_mul(quota.interval_daa)).ok_or("stage interval overflow")? {
            return Err("stage job and retained guarantees cross native quota interval".into());
        }
        let debit_receipt = object_id(
            b"misaka-palw/kernel/stage-original-escrow-debit/v14",
            &(native.block, native.daa, native.class, job.job().root(), native.requester_operator, native.original_debit),
        );
        Ok(Self {
            version: 14,
            class: class.root(),
            policy: class.policy_root(),
            source: class.source().id(),
            job: job.job().clone(),
            acceptance: native.acceptance,
            quota_policy: quota.root(),
            execution_policy: execution.root(),
            source_plan: native.source_plan,
            trace_plan: native.trace_plan,
            fee_plan: native.fee_plan,
            availability_terms: native.availability_terms,
            admission: job.admission().clone(),
            quota_plan: plan.root(),
            requester_operator: native.requester_operator,
            producer_operator: native.producer_operator,
            admitted_daa: native.daa,
            admitted_interval: interval,
            commit_deadline_daa: native.daa.checked_add(execution.horizons.commit_window).ok_or("stage commit window overflow")?,
            deadline_daa: deadline,
            reward: native.reward,
            debit_receipt,
            original_debit: native.original_debit,
            escrow_remaining: native.original_debit,
            terminal: StageTerminalV14::Funded,
            claim_root: None,
            committed_daa: None,
            challenge_end_daa: None,
            producer_reserved: 0,
            settlement: None,
        })
    }
}

impl QuotaStateV11 {
    /// A mature original Stage seal pays its own Court attempt. Capture the
    /// real post-debit counters before class/proof restoration; a receipt or
    /// wire flag cannot fabricate this private terminal snapshot token.
    pub fn debit_stage_protected_verdict_v14(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        binding: &SubjectBindingV11,
        plan: &CheckedStageQuotaV14,
        key: &Digest,
        original_paid: &Digest,
        carrier: &[u8],
    ) -> Result<StageVerdictDebitV14, String> {
        let subject = self.subjects.get(&binding.job_id).ok_or("stage owncourt original subject absent")?;
        let seal = self.protected_seals.get(key).ok_or("stage owncourt original paid seal absent")?;
        if subject.binding != *binding
            || subject.closed
            || binding.class_id != plan.class
            || binding.quota_plan_root != plan.plan.record().root()
            || seal.subject_binding_root != binding.root()
            || seal.key.seal_actor != actor
            || seal.original_paid_seal_root != *original_paid
            || *original_paid == [0; 64]
            || seal.producer_operator_id == seal.seal_operator_id
        {
            return Err("stage owncourt original paid subject/actor/source/class differs".into());
        }
        let remaining = subject.guarantee_remaining.clone();
        let receipt = self.debit_protected(policy, epoch, actor, key, plan.plan(), ProtectedActionKindV11::Court, carrier)?;
        Ok(StageVerdictDebitV14 {
            paid: ExpiredTerminalDebitV11 {
                receipt: QuotaReceiptV11 {
                    policy_root: policy.root(),
                    epoch: epoch.clone(),
                    subject_binding: binding.root(),
                    kind: QuotaActionKindV11::Court,
                    consumed: receipt.consumed,
                    added_guarantee: QuotaQuantitiesV11::default(),
                    remaining_guarantee: remaining,
                },
                actor,
                after_block: self.block.consumed.clone(),
                after_network: self.network.consumed.clone(),
                after_actor: self.actors.get(&actor).ok_or("stage owncourt lost original actor")?.consumed.clone(),
            },
        })
    }
    /// Current public verdict work has its own ordinary debit; it cannot borrow
    /// the original producer Close slot or refund an old retained guarantee.
    pub fn debit_stage_verdict_v14(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        class: &CheckedStageClassV14,
        book: &StageJobBookV14,
        plan: &CheckedStageQuotaV14,
    ) -> Result<StageVerdictDebitV14, String> {
        book.check_restored(class, plan)?;
        let binding = book.quota_subject(plan, policy)?;
        let subject = self.subjects.get(&binding.job_id).ok_or("stage verdict original subject absent")?;
        if book.terminal != StageTerminalV14::Challengeable
            || subject.binding != binding
            || subject.closed
            || binding.admitted_interval > epoch.network_interval
        {
            return Err("stage verdict lacks original open claimed subject".into());
        }
        let remaining = subject.guarantee_remaining.clone();
        let quantities = action_envelope(16 << 10, MAX_FIXED_ACTION_BYTES_V11, None)?;
        self.debit_unreserved(policy, epoch, actor, &quantities)?;
        Ok(StageVerdictDebitV14 {
            paid: ExpiredTerminalDebitV11 {
                receipt: QuotaReceiptV11 {
                    policy_root: policy.root(),
                    epoch: epoch.clone(),
                    subject_binding: binding.root(),
                    kind: QuotaActionKindV11::Close,
                    consumed: quantities,
                    added_guarantee: QuotaQuantitiesV11::default(),
                    remaining_guarantee: remaining,
                },
                actor,
                after_block: self.block.consumed.clone(),
                after_network: self.network.consumed.clone(),
                after_actor: self.actors.get(&actor).ok_or("stage verdict lost current actor")?.consumed.clone(),
            },
        })
    }
    pub fn mark_stage_verdict_closed_v14(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        terminal: &CheckedQuotaTerminalV11,
        debit: &StageVerdictDebitV14,
    ) -> Result<(), String> {
        self.check_context(policy, epoch)?;
        let paid = &debit.paid;
        let subject = self.subjects.get_mut(&terminal.binding.job_id).ok_or("stage verdict original subject absent")?;
        if self.epoch != *epoch
            || subject.binding != terminal.binding
            || subject.closed
            || paid.receipt.policy_root != self.policy_root
            || paid.receipt.epoch != *epoch
            || paid.receipt.subject_binding != terminal.binding.root()
            || paid.after_block != self.block.consumed
            || paid.after_network != self.network.consumed
            || self.actors.get(&paid.actor).is_none_or(|a| a.consumed != paid.after_actor)
        {
            return Err("stage verdict lacks exact current paid debit and original native terminal".into());
        }
        subject.closed = true;
        Ok(())
    }
    /// Same frozen current ordinary expired-close debit, followed only by an
    /// authenticated stage14 terminal book and original escrow transition.
    /// Native authenticates the original row, beneficiary and shared nonce
    /// before passing this private book; this method creates no payout right.
    pub fn mark_expired_stage_terminal_closed_v14(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        class: &CheckedStageClassV14,
        plan: &CheckedStageQuotaV14,
        terminal: &StageJobBookV14,
        paid: &ExpiredTerminalDebitV11,
    ) -> Result<(), String> {
        self.check_context(policy, epoch)?;
        terminal.check_restored(class, plan)?;
        if self.epoch != *epoch
            || paid.receipt.policy_root != self.policy_root
            || paid.receipt.epoch != *epoch
            || paid.receipt.subject_binding != binding.root()
            || paid.after_block != self.block.consumed
            || paid.after_network != self.network.consumed
            || self.actors.get(&paid.actor).is_none_or(|a| a.consumed != paid.after_actor)
            || epoch.block_daa <= terminal.deadline_daa
            || terminal.quota_subject(plan, policy)? != *binding
            || terminal.terminal != StageTerminalV14::Refunded
            || terminal.escrow_remaining != 0
            || terminal.original_debit == 0
            || terminal.claim_root.is_some()
            || terminal.producer_reserved != 0
        {
            return Err("late stage terminal lacks exact prepaid current debit and original refunded book".into());
        }
        let subject = self.subjects.get_mut(&binding.job_id).ok_or("stage original baseline subject absent")?;
        if subject.binding != *binding || subject.closed {
            return Err("late stage original subject rebound or already closed".into());
        }
        subject.closed = true;
        Ok(())
    }
}

/// No wire codec or public constructor: only a real current ordinary debit
/// creates this token. It carries no original guarantee or payout authority.
pub struct StageVerdictDebitV14 {
    paid: ExpiredTerminalDebitV11,
}

/// Original native paid proof commitment, never a route-deserialized grant.
/// Source PC remains separate from the combined real-weight/input PC.
pub struct NativeStageProofAnchorV14 {
    pub job: Digest,
    pub original_book: Digest,
    pub current_book: Digest,
    pub claim: Digest,
    pub class: Digest,
    pub source: Digest,
    pub source_parameters: Digest,
    pub inventory: Digest,
    pub admission: Digest,
    pub combined_parameters: Digest,
    pub maximum_scope: Digest,
    pub policy: Digest,
    pub quota: Digest,
    pub consent18: Digest,
    pub proof_root: Digest,
    pub actor: Digest,
    pub actor_operator: Digest,
    pub producer_operator: Digest,
    pub posted_daa: u64,
    pub matures_daa: u64,
    pub horizon_daa: u64,
    pub original_paid_root: Digest,
    pub original_debit: u64,
    pub original_debit_receipt: Digest,
    pub shared_live: u32,
    pub actor_live: u32,
    pub job_live: u32,
}
pub fn stage_paid_seal_record_root_v14(n: &NativeStageProofAnchorV14) -> Digest {
    object_id(
        b"misaka-palw/kernel/stage-original-paid-seal/v14",
        &(
            (n.job, n.original_book, n.current_book, n.claim, n.class, n.source, n.source_parameters, n.inventory),
            (n.admission, n.combined_parameters, n.maximum_scope, n.policy, n.quota, n.consent18, n.proof_root),
            (
                n.actor,
                n.actor_operator,
                n.producer_operator,
                n.posted_daa,
                n.matures_daa,
                n.horizon_daa,
                n.original_debit,
                n.original_debit_receipt,
            ),
        ),
    )
}
pub(super) struct CheckedStageProofGrantV14 {
    binding: SubjectBindingV11,
    key: ProtectedSealKeyV11,
    original_paid: Digest,
    actor_operator: Digest,
    producer_operator: Digest,
    posted: u64,
    matures: u64,
}
impl CheckedStageProofGrantV14 {
    pub(super) fn into_parts(self) -> (SubjectBindingV11, ProtectedSealKeyV11, Digest, Digest, Digest, u64, u64) {
        (self.binding, self.key, self.original_paid, self.actor_operator, self.producer_operator, self.posted, self.matures)
    }
}
pub fn check_stage_paid_seal_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    plan: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    execution: &ExecutionPolicyV11,
    epoch: &TrustedEpochV11,
    n: &NativeStageProofAnchorV14,
) -> Result<CheckedPaidSealV11, String> {
    book.check_restored(class, plan)?;
    epoch.check(quota)?;
    execution.check(quota)?;
    let actual = class.check_job(book.job())?;
    if book.terminal != StageTerminalV14::Challengeable
        || book.settlement.is_some()
        || book.execution_policy != execution.root()
        || n.job != book.id()
        || n.original_book != book.original_funded_root()
        || n.current_book != book.root()
        || Some(n.claim) != book.claim_root
        || n.class != class.root()
        || n.source != class.source().id()
        || n.source_parameters != class.source().parameters().root()
        || n.inventory != class.source().inventory_root()
        || n.admission != actual.admission().root()
        || n.combined_parameters != actual.combined_parameters().root()
        || n.maximum_scope != class.maximum_resources().court_scope
        || n.policy != book.policy
        || n.quota != plan.root()
        || n.consent18 != book.acceptance
        || n.proof_root == [0; 64]
        || n.actor == [0; 64]
        || n.actor == book.job.producer
        || n.actor_operator == [0; 64]
        || n.actor_operator == n.producer_operator
        || n.producer_operator != book.producer_operator
        || n.posted_daa != epoch.block_daa
        || n.posted_daa < book.committed_daa.ok_or("stage paid seal commit absent")?
        || n.posted_daa > book.challenge_end_daa.ok_or("stage paid seal challenge window absent")?
        || n.matures_daa != n.posted_daa.checked_add(execution.horizons.seal_maturity).ok_or("stage seal maturity overflow")?
        || n.horizon_daa
            != n.posted_daa
                .checked_add(execution.horizons.seal_ttl)
                .and_then(|n| n.checked_add(execution.horizons.proof_grace))
                .ok_or("stage seal horizon overflow")?
        || n.horizon_daa > book.deadline_daa
        || epoch.network_interval != book.admitted_interval
        || n.original_paid_root != stage_paid_seal_record_root_v14(n)
        || n.original_debit == 0
        || n.original_debit_receipt == [0; 64]
        || n.shared_live == 0
        || n.shared_live > PROTECTED_SEALS_MAX_V11
        || n.actor_live == 0
        || n.actor_live > PROTECTED_SEALS_PER_ACTOR_V11
        || n.job_live == 0
        || n.job_live > PROTECTED_SEALS_PER_JOB_V11
    {
        return Err("stage protected proof lacks exact original paid independent native seal/claim/source/input/maximum/slots".into());
    }
    super::protected::from_checked_stage_proof_v14(CheckedStageProofGrantV14 {
        binding: book.quota_subject(plan, quota)?,
        key: ProtectedSealKeyV11 {
            job_id: book.id(),
            vector_id: n.claim,
            seal_actor: n.actor,
            proof_root: object_id(
                b"misaka-palw/kernel/stage-protected-proof-namespace/v14",
                &(n.proof_root, n.source, n.admission, n.quota),
            ),
            horizon_daa: n.horizon_daa,
        },
        original_paid: n.original_paid_root,
        actor_operator: n.actor_operator,
        producer_operator: n.producer_operator,
        posted: n.posted_daa,
        matures: n.matures_daa,
    })
}

pub(super) struct CheckedStageTraceProofGrantV14 {
    binding: SubjectBindingV11,
    key: Digest,
    actor: Digest,
    original: Digest,
    trace: Digest,
    horizon: u64,
    epoch: TrustedEpochV11,
}
impl CheckedStageTraceProofGrantV14 {
    pub(super) fn into_parts(self) -> (SubjectBindingV11, Digest, Digest, Digest, Digest, u64, TrustedEpochV11) {
        (self.binding, self.key, self.actor, self.original, self.trace, self.horizon, self.epoch)
    }
}
pub fn check_stage_trace_seal_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    stage: &CheckedStageQuotaV14,
    quota: &QuotaPolicyV11,
    execution: &ExecutionPolicyV11,
    epoch: &TrustedEpochV11,
    trace: &trace17::TracePlanV17,
    n: &NativeStageProofAnchorV14,
) -> Result<trace17::CheckedTraceAdmissionV17, String> {
    let seal = check_stage_paid_seal_v14(class, book, stage, quota, execution, epoch, n)?;
    if trace.root() != book.trace_plan_root() || derive_stage_trace_plan_v14(class, stage, quota)?.root() != trace.root() {
        return Err("stage paid seal restore quote changes original18 trace contract".into());
    }
    trace17::from_checked_stage_proof_v14(
        CheckedStageTraceProofGrantV14 {
            binding: seal.binding().clone(),
            key: seal.key().root(),
            actor: n.actor,
            original: n.original_paid_root,
            trace: trace.root(),
            horizon: n.horizon_daa,
            epoch: epoch.clone(),
        },
        trace,
    )
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageTraceV14 {
    pub version: u16,
    pub class: Digest,
    pub job_id: Digest,
    pub job_input: Digest,
    pub admission: Digest,
    pub quota: Digest,
    pub segment_roots: Vec<Digest>,
    pub output_openings: Vec<crate::seg::NodeOpeningV1>,
}
impl StageTraceV14 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-trace/v14", self)
    }
    pub fn from_bytes(raw: &[u8], positions: u32) -> Result<Self, String> {
        const PREFIX: usize = 2 + 5 * 64;
        if positions == 0
            || positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD
            || raw.len() < PREFIX + 8
            || raw.len() > (2 << 20)
        {
            return Err("stage trace exceeds fixed position/byte caps".into());
        }
        let a = u32::from_le_bytes(raw[PREFIX..PREFIX + 4].try_into().expect("checked prefix")) as usize;
        if a != crate::seg::segment_count_v1(positions) as usize {
            return Err("stage trace segment count differs".into());
        }
        let at = PREFIX + 4 + a * 64;
        let b =
            u32::from_le_bytes(raw.get(at..at + 4).ok_or("truncated stage output count")?.try_into().expect("checked count")) as usize;
        if b != 1 && b != positions as usize {
            return Err("stage output count exceeds immutable job".into());
        }
        let mut r = Cursor::new(&raw[at + 4..]);
        let mut output_openings = Vec::with_capacity(b);
        for _ in 0..b {
            output_openings.push(crate::flat_court4::read_node(&mut r).map_err(|e| e.to_string())?);
        }
        if r.position() as usize != raw.len() - at - 4 {
            return Err("stage output carrier has surplus bytes".into());
        }
        let version = u16::from_le_bytes(raw[..2].try_into().expect("checked version"));
        let digest = |i: usize| raw[2 + i * 64..2 + (i + 1) * 64].try_into().expect("checked fixed roots");
        let segment_roots =
            raw[PREFIX + 4..at].chunks_exact(64).map(|bytes| bytes.try_into().expect("checked segment count")).collect();
        let trace = Self {
            version,
            class: digest(0),
            job_id: digest(1),
            job_input: digest(2),
            admission: digest(3),
            quota: digest(4),
            segment_roots,
            output_openings,
        };
        if trace.version != 14 {
            return Err("unknown stage trace version".into());
        }
        Ok(trace)
    }
}
/// Original aux stage row and the shared text11/stage14 operator nonce counter.
pub struct NativeStageQuotaAnchorV14 {
    pub class: Digest,
    pub source: Digest,
    pub policy: Digest,
    pub acceptance: Digest,
    pub source_plan: Digest,
    pub trace_plan: Digest,
    pub fee_plan: Digest,
    pub availability_terms: Digest,
    pub funded_book_root: Digest,
    pub job_id: Digest,
    pub original_debit_receipt: Digest,
    pub original_debit: u64,
    pub nonce: NativeRequesterNonceWitnessV11,
    pub block: Digest,
    pub daa: u64,
    pub source_refuted: bool,
    pub source_matures_daa: u64,
}
/// Authenticated original native funded/header/terminal membership and the
/// permanent operator counter. This is never a deserialized route grant.
pub struct NativeStageTerminalAnchorV14 {
    pub job_id: Digest,
    pub original_funded_book_root: Digest,
    pub terminal_book_root: Digest,
    pub acceptance18: Digest,
    pub nonce: NativeRequesterNonceWitnessV11,
    pub established_daa: u64,
    pub principal_refund: u64,
    pub principal_queue: Digest,
}
pub fn check_stage_quota_terminal_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    plan: &CheckedStageQuotaV14,
    policy: &QuotaPolicyV11,
    native: &NativeStageTerminalAnchorV14,
) -> Result<CheckedQuotaTerminalV11, String> {
    book.check_restored(class, plan)?;
    if book.terminal != StageTerminalV14::Refunded
        || book.escrow_remaining != 0
        || book.claim_root.is_some()
        || book.producer_reserved != 0
        || native.job_id != book.id()
        || native.original_funded_book_root != book.original_funded_root()
        || native.terminal_book_root != book.root()
        || native.acceptance18 != book.acceptance
        || native.nonce.requester != book.job.requester
        || native.nonce.requester_operator_id != book.requester_operator
        || native.nonce.next_nonce <= canonical_nonce_v11(&book.job.nonce)?
        || native.nonce.counter_root != native_requester_nonce_counter_root_v11(&book.requester_operator, native.nonce.next_nonce)
        || native.established_daa <= book.deadline_daa
        || native.principal_refund != book.original_debit
        || native.principal_queue == [0; 64]
    {
        return Err("stage terminal grant lacks genuine original18 refunded escrow/header/shared nonce".into());
    }
    Ok(CheckedQuotaTerminalV11 {
        binding: book.quota_subject(plan, policy)?,
        funded_job_root: book.original_funded_root(),
        terminal_book_root: book.root(),
        request_key: object_id(
            b"misaka-palw/kernel/stage-original-request-key/v14",
            &(book.class, book.job.requester, book.job.producer, book.job.nonce, book.acceptance),
        ),
    })
}
/// Native supplies this only after collecting the exact original producer
/// collateral and recording the immutable original-principal credit. A source
/// denial flag or another job's verdict cannot manufacture a closed subject.
pub fn check_stage_conviction_terminal_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    plan: &CheckedStageQuotaV14,
    policy: &QuotaPolicyV11,
    native: &NativeStageTerminalAnchorV14,
    outcome: &StageSettlementV14,
    collected_original: u64,
) -> Result<CheckedQuotaTerminalV11, String> {
    book.check_restored(class, plan)?;
    outcome.check()?;
    if book.terminal != StageTerminalV14::Convicted
        || book.settlement.as_ref() != Some(outcome)
        || native.job_id != book.id()
        || native.original_funded_book_root != book.original_funded_root()
        || native.terminal_book_root != book.root()
        || native.acceptance18 != book.acceptance
        || native.nonce.requester != book.job.requester
        || native.nonce.requester_operator_id != book.requester_operator
        || native.nonce.next_nonce <= canonical_nonce_v11(&book.job.nonce)?
        || native.nonce.counter_root != native_requester_nonce_counter_root_v11(&book.requester_operator, native.nonce.next_nonce)
        || native.established_daa != outcome.established_daa
        || native.established_daa > book.deadline_daa
        || native.principal_refund != outcome.requester_refund
        || native.principal_queue == [0; 64]
        || collected_original != outcome.collected_original_slash
    {
        return Err("stage convicted terminal lacks its original per-job proof, actual slash and principal credit".into());
    }
    Ok(CheckedQuotaTerminalV11 {
        binding: book.quota_subject(plan, policy)?,
        funded_job_root: book.original_funded_root(),
        terminal_book_root: book.root(),
        request_key: object_id(
            b"misaka-palw/kernel/stage-original-request-key/v14",
            &(book.class, book.job.requester, book.job.producer, book.job.nonce, book.acceptance),
        ),
    })
}
pub fn check_stage_quota_admission_v14(
    class: &CheckedStageClassV14,
    book: &StageJobBookV14,
    plan: &CheckedStageQuotaV14,
    policy: &QuotaPolicyV11,
    native: &NativeStageQuotaAnchorV14,
) -> Result<CheckedQuotaAdmissionV11, String> {
    let checked = class.check_job(book.job())?;
    let own = derive_stage_quota_v14(class, &checked)?;
    if own.root() != plan.root()
        || book.root() != native.funded_book_root
        || book.id() != native.job_id
        || book.class != native.class
        || book.source != native.source
        || book.policy != native.policy
        || book.acceptance != native.acceptance
        || book.source_plan != native.source_plan
        || book.trace_plan != native.trace_plan
        || book.fee_plan != native.fee_plan
        || book.availability_terms != native.availability_terms
        || native.acceptance == [0; 64]
        || book.class != class.root()
        || book.source != class.source().id()
        || book.policy != class.policy_root()
        || book.terminal != StageTerminalV14::Funded
        || book.claim_root.is_some()
        || book.producer_reserved != 0
        || book.escrow_remaining != book.original_debit
        || book.original_debit != native.original_debit
        || book.debit_receipt != native.original_debit_receipt
        || native.original_debit == 0
        || native.nonce.requester != book.job.requester
        || native.nonce.requester_operator_id != book.requester_operator
        || native.nonce.next_nonce <= canonical_nonce_v11(&book.job.nonce)?
        || native.nonce.counter_root != native_requester_nonce_counter_root_v11(&book.requester_operator, native.nonce.next_nonce)
        || native.source_refuted
        || native.source_matures_daa > native.daa
        || native.block == [0; 64]
        || book.admitted_daa != native.daa
    {
        return Err("stage baseline lacks its exact original native funded book/source/shared nonce counter".into());
    }
    Ok(CheckedQuotaAdmissionV11 {
        binding: book.quota_subject(plan, policy)?,
        native_admission_epoch: Some((native.block, native.daa)),
    })
}
