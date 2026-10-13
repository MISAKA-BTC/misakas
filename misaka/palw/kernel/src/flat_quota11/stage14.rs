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
    let restore = native_candidate_restore_work_from_scope_v11(&program, pc, maximum)?
        .checked_add(source_bytes.checked_mul(source_bytes).ok_or("stage source restore work overflow")?)
        .and_then(|n| n.checked_add(source_bytes.checked_mul(128)?))
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
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageJobBookV14 {
    version: u16,
    class: Digest,
    policy: Digest,
    source: Digest,
    job: FlatStageJobV14,
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
}
/// Native supplies these from the original class/source/policy and real coins.
/// This is never a wire-deserialized grant or an availability assertion.
pub struct NativeStageFundingV14 {
    pub class: Digest,
    pub source: Digest,
    pub policy: Digest,
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
            || self.escrow_remaining > self.original_debit
            || self.admitted_daa >= self.commit_deadline_daa
            || self.commit_deadline_daa >= self.deadline_daa
        {
            return Err("stage book differs from independently derived original class/job/resources".into());
        }
        match self.terminal {
            StageTerminalV14::Funded
                if self.escrow_remaining == self.original_debit
                    && self.claim_root.is_none()
                    && self.producer_reserved == 0
                    && self.committed_daa.is_none()
                    && self.challenge_end_daa.is_none() =>
            {
                ()
            }
            StageTerminalV14::Challengeable
                if self.escrow_remaining == self.original_debit
                    && self.claim_root.is_some()
                    && self.producer_reserved != 0
                    && self.committed_daa.is_some()
                    && self.challenge_end_daa.is_some() =>
            {
                ()
            }
            StageTerminalV14::Refunded
                if self.escrow_remaining == 0
                    && self.claim_root.is_none()
                    && self.producer_reserved == 0
                    && self.committed_daa.is_none()
                    && self.challenge_end_daa.is_none() =>
            {
                ()
            }
            StageTerminalV14::Convicted | StageTerminalV14::Final if self.escrow_remaining == 0 => (),
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
    pub fn quota_subject(&self, plan: &CheckedStageQuotaV14, policy: &QuotaPolicyV11) -> Result<SubjectBindingV11, String> {
        if self.version != 14
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
        })
    }
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
