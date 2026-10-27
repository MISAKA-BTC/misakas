//! Independent stage28 maximum reservation and actual authenticated claim.
//! Shared quota11 is accounting algebra only: its original wire entry points
//! cannot construct this private namespace28 admission or enlarge its maxima.
use super::*;
use crate::flat_inputs12::bounded28::{CheckedStageClassV28, CheckedStageJobV28};
pub fn derive_stage_restore_metadata_v28(
    source: u32,
    class: u32,
    book: u32,
    inputs: u32,
    carrier: u32,
) -> Result<DerivedOrdinaryMetadataV11, String> {
    if source > 16 << 20 || class > 1 << 20 || book > 16384 || inputs > 1 << 20 || carrier > 16 << 20 {
        return Err("stage28 bounded original metadata extent exceeded".into());
    }
    let bytes = (source as u64)
        .checked_add(class as u64)
        .and_then(|n| n.checked_add(book as u64))
        .and_then(|n| n.checked_add(inputs as u64))
        .and_then(|n| n.checked_add(carrier as u64))
        .and_then(|n| n.checked_add(32768))
        .ok_or("stage28 bounded metadata sum overflow")?;
    let work = (source as u128) * (source as u128) * 64
        + (class as u128) * (class as u128) * 64
        + (book as u128 + inputs as u128 + carrier as u128) * 1024
        + bytes as u128 * 64;
    let quantities = action_envelope(work, bytes, None)?;
    Ok(DerivedOrdinaryMetadataV11 { quantities })
}

pub struct CheckedStageQuotaV28 {
    class: Digest,
    source: Digest,
    admission: Digest,
    policy: Digest,
    plan: CheckedJobQuotaPlanV11,
    requests_per_owner: u32,
    restore_bytes: u64,
    invocation32: bool,
}
impl CheckedStageQuotaV28 {
    pub fn is_invocation_v32(&self) -> bool {
        self.invocation32
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/stage-original-quota/v28",
            &(
                28u16,
                self.class,
                self.source,
                self.admission,
                self.policy,
                self.plan.record().root(),
                self.requests_per_owner,
                self.restore_bytes,
            ),
        )
    }
    pub fn plan(&self) -> &CheckedJobQuotaPlanV11 {
        &self.plan
    }
    pub fn requests_per_owner(&self) -> u32 {
        self.requests_per_owner
    }
    pub fn restore_bytes(&self) -> u64 {
        self.restore_bytes
    }
    pub fn restore_native_record(
        class: Digest,
        source: Digest,
        admission: Digest,
        policy: Digest,
        requests: u32,
        restore_bytes: u64,
        record: &QuotaPlanRecordV11,
        expected: Digest,
    ) -> Result<Self, String> {
        let plan = CheckedJobQuotaPlanV11::restore(record, &record.root())?;
        let value = Self { class, source, admission, policy, plan, requests_per_owner: requests, restore_bytes, invocation32: false };
        if [class, source, admission, policy].contains(&[0; 64])
            || requests == 0
            || requests > 1 << 18
            || restore_bytes == 0
            || restore_bytes > 32 << 20
            || value.root() != expected
        {
            return Err("stage28 documentary cached original quota differs".into());
        }
        Ok(value)
    }
    pub fn source_plan(
        &self,
        class: &CheckedStageClassV28,
        quota: &QuotaPolicyV11,
        discovery: &discovery13::DiscoveryPolicyV13,
    ) -> Result<source16::SourcePlanV16, String> {
        discovery.check(quota)?;
        if self.class != class.root() || self.source != class.source().id() || self.policy != class.wire().policy {
            return Err("stage28 source quotation changes original class".into());
        }
        let mut requests = self
            .requests_per_owner
            .checked_mul(PROTECTED_SEALS_PER_JOB_V11)
            .and_then(|n| n.checked_add(discovery.ordinary_lifetime_requests))
            .ok_or("stage28 source count overflow")?;
        // Preclaim producer transport is a NEW32 obligation. Its units are
        // reserved in addition to every owner's entire original source path.
        if self.invocation32 {
            requests = requests
                .checked_add(invocation_commit_parts_bound_v32(self.plan.positions)? as u32 + 1)
                .ok_or("invocation32 producer post count overflow")?;
        }
        source16::derive_source_plan_from_stage_v28(class, self, quota, requests)
    }
}
/// Independent maximum relation geometry determines every original-PC header,
/// row/column bisection and operand leaf; no caller supplies a count or vector.
pub fn derive_stage_quota_v28(class: &CheckedStageClassV28, job: &CheckedStageJobV28) -> Result<CheckedStageQuotaV28, String> {
    let checked = class.check_job(job.job())?;
    if checked.admission() != job.admission() {
        return Err("stage28 quota actual admission differs".into());
    }
    let maximum = class.maximum_scope().scope();
    let source = class.source();
    let source_bound = crate::flat_execution11::native_source_court_bound_v11(source.program().encode().len(), source.parameters())?;
    let filing =
        maximum.worst_court_bytes.max(source_bound.max_proof_bytes).checked_add(4096).ok_or("stage28 full court extent overflow")?;
    let work = maximum.worst_court_work.max(source_bound.work);
    let scratch = maximum.worst_court_scratch_bytes.max(source_bound.scratch_bytes);
    if filing > 16 << 20 || work > 1 << 34 || scratch > 256 << 20 {
        return Err("stage28 full original court25 exceeds independent capacities".into());
    }
    let maximum_program = class.template().lengths.program(class.template().lengths.maxima)?;
    let requests_per_owner =
        super::super::text29::complete_public_callback_bound_v29(&maximum_program, class.maximum_scope(), job.combined_parameters())?;
    let source_wire = class.source_restore_bytes();
    let class_wire = borsh::to_vec(class.wire()).map_err(|e| e.to_string())?.len() as u64;
    let restore_bytes = source_wire
        .checked_add(class_wire)
        .and_then(|n| n.checked_add(u64::try_from(job.input_raw_bytes()).ok()?))
        .and_then(|n| n.checked_add(16384))
        .ok_or("stage28 original restore extent overflow")?;
    let restore_work = restore_bytes
        .checked_mul(restore_bytes)
        .and_then(|n| n.checked_mul(64))
        .and_then(|n| n.checked_add(restore_bytes.checked_mul(1024)?))
        .ok_or("stage28 original restore work overflow")?;
    let program = job.program().v1_view();
    let prepared = crate::element::PreparedV1::without_scope(&program, None)?;
    let outputs = match job.program().output {
        misaka_palw_tir::program_v2::OutputDecl::Rows { .. } | misaka_palw_tir::program_v2::OutputDecl::Logits { .. } => {
            job.job().positions as u64
        }
        misaka_palw_tir::program_v2::OutputDecl::Final { .. } => 1,
        _ => return Err("stage28 output semantics".into()),
    };
    let post = outputs
        .checked_mul(crate::element::node_opening_bytes_v1(prepared.node_count))
        .and_then(|n| n.checked_add(crate::seg::segment_count_v1(job.job().positions) as u64 * 64 + 4096))
        .ok_or("stage28 original Commit carrier overflow")?;
    let full = filing.checked_add(restore_bytes).ok_or("stage28 full restore filing overflow")?;
    let mut actions = [
        action_envelope(
            restore_work as u128 + job.resources().quantities.get(ResourceAxisV10::MetadataWork),
            post.max(restore_bytes),
            None,
        )?,
        action_envelope(restore_work as u128, restore_bytes.checked_add(16384).ok_or("stage28 request extent overflow")?, None)?,
        action_envelope(restore_work as u128, restore_bytes.checked_add(128 << 10).ok_or("stage28 answer extent overflow")?, None)?,
        action_envelope(restore_work as u128, full, Some((work, filing, scratch)))?,
        action_envelope(restore_work as u128, full, Some((work, filing, scratch)))?,
        action_envelope(restore_work as u128, restore_bytes, None)?,
    ];
    for a in &mut actions {
        include_native_post_io_v11(a, post)?;
        include_source_court_io_v11(a, source.parameters())?;
    }
    let mut baseline = QuotaQuantitiesV11::from(&job.resources().quantities);
    // External actual images must be decoded/authenticated before Fund and held
    // throughout their original claim; maximum reference remains a separate root.
    for (axis, n) in [
        (ResourceAxisV10::RawParameterStorage, job.input_raw_bytes()),
        (ResourceAxisV10::CopiedReferenceBytes, job.input_reference_bytes()),
        (ResourceAxisV10::StoredEvidenceBytes, job.input_raw_bytes()),
    ] {
        baseline.set(axis, baseline.get(axis).checked_add(n).ok_or("stage28 input baseline overflow")?);
    }
    let plan = CheckedJobQuotaPlanV11 {
        program_root: crate::public::program_root_v1(&program.encode()),
        parameter_root: job.combined_parameters().root(),
        resource_root: job.resources().root(),
        class_scope_root: class.maximum_scope().root(),
        actual_scope_root: job.scope().root(),
        native_restore_work: restore_work,
        native_post_carrier_cap: post,
        source_court_bound: source_bound,
        positions: job.job().positions,
        baseline,
        action_envelopes: actions,
        action_carrier_caps: [post.max(restore_bytes), restore_bytes + 16384, restore_bytes + (128 << 10), full, full, restore_bytes],
    };
    Ok(CheckedStageQuotaV28 {
        class: class.root(),
        source: source.id(),
        admission: job.admission().root(),
        policy: class.wire().policy,
        plan,
        requests_per_owner,
        restore_bytes,
        invocation32: false,
    })
}

/// New invocation32 quotation. Old28 byte meanings remain unchanged. All
/// position-specific input images and the bounded original Master/upstream
/// certificate are retained and restored under the ORIGINAL maximum plan.
pub fn derive_stage_quota_v32(class: &CheckedStageClassV28, job: &CheckedStageJobV28) -> Result<CheckedStageQuotaV28, String> {
    let mut stage = derive_stage_quota_v28(class, job)?;
    let scope = crate::flat_inputs12::court32::derive_stage_court_scope_v32(class)?;
    let input =
        job.input_raw_bytes().checked_mul(job.job().positions as u128).ok_or("stage32 all-position input resource overflow")?;
    if input > 1 << 20 {
        return Err("stage32 maximum position inputs exceed original paid cap".into());
    }
    // Envelope32 has <=2MiB upstream tensor material and <=2MiB base carrier.
    // The16MiB supplement also covers raw i8->i128 tensor expansion and
    // certificate/native class metadata ownership. The original plan reserves
    // the full cap, independently of actual
    // EOS/number of prior Finals; repeated restore is priced at every action.
    let extra = (16u64 << 20)
        .checked_add(invocation_commit_bytes_bound_v32(job.job().positions)?)
        .and_then(|n| n.checked_add(input as u64))
        .and_then(|n| n.checked_add(32768))
        .ok_or("stage32 original restore overflow")?;
    let restore = stage.restore_bytes.checked_add(extra).ok_or("stage32 full original restore overflow")?;
    let work = restore
        .checked_mul(restore)
        .and_then(|n| n.checked_mul(64))
        .and_then(|n| n.checked_add(restore.checked_mul(1024)?))
        .ok_or("stage32 full original restore work overflow")?;
    let c = scope.scope();
    let prior = stage.plan.source_court_bound.clone();
    let filing = prior.max_proof_bytes.max(c.maximum_bytes).checked_add(4096).ok_or("stage32 maximum filing overflow")?;
    let court_work = prior.work.max(c.maximum_work);
    let scratch = prior.scratch_bytes.max(c.maximum_scratch);
    if filing > 16 << 20 || court_work > 1 << 34 || scratch > 256 << 20 {
        return Err("stage32 complete original court capacity exceeded".into());
    }
    let supplement = action_envelope(work as u128, extra, None)?;
    stage.plan.baseline = stage.plan.baseline.checked_add(&supplement)?;
    for axis in [ResourceAxisV10::RawParameterStorage, ResourceAxisV10::CopiedReferenceBytes, ResourceAxisV10::StoredEvidenceBytes] {
        stage.plan.baseline.set(axis, stage.plan.baseline.get(axis).checked_add(input).ok_or("stage32 full position input overflow")?);
    }
    let post = invocation_commit_bytes_bound_v32(job.job().positions)?;
    let full = filing.checked_add(restore).ok_or("stage32 full filing restore overflow")?;
    let sizes = [post.max(restore), restore + 16384, restore + (128 << 10), full, full, restore];
    for (i, a) in stage.plan.action_envelopes.iter_mut().enumerate() {
        *a = action_envelope(work as u128, sizes[i], if i == 3 || i == 4 { Some((court_work, filing, scratch)) } else { None })?;
        include_native_post_io_v11(a, post)?;
        include_source_court_io_v11(a, class.source().parameters())?;
    }
    stage.plan.native_restore_work = work;
    stage.plan.native_post_carrier_cap = post;
    stage.plan.action_carrier_caps = sizes;
    let leaves = c.relations.iter().map(|r| r.operand_leaves.iter().copied().sum::<u64>()).max().unwrap_or(0);
    stage.requests_per_owner = stage
        .requests_per_owner
        .checked_add(u32::try_from(leaves).map_err(|_| "stage32 callback count overflow")?)
        .and_then(|n| n.checked_add(64))
        .filter(|n| *n <= 1 << 18)
        .ok_or("stage32 bounded full-context pursuit count exceeded")?;
    stage.restore_bytes = restore;
    stage.invocation32 = true;
    Ok(stage)
}
impl CheckedStageQuotaV28 {
    /// Native mode comes from the original producer-signed invocation32 row,
    /// never an arbitrary on-wire flag or a legacy28 cache.
    pub fn restore_native_record_v32(
        class: Digest,
        source: Digest,
        admission: Digest,
        policy: Digest,
        requests: u32,
        restore_bytes: u64,
        record: &QuotaPlanRecordV11,
        expected: Digest,
    ) -> Result<Self, String> {
        let plan = CheckedJobQuotaPlanV11::restore(record, &record.root())?;
        let p = Self { class, source, admission, policy, plan, requests_per_owner: requests, restore_bytes, invocation32: true };
        if [class, source, admission, policy].contains(&[0; 64])
            || requests == 0
            || requests > 1 << 18
            || restore_bytes == 0
            || restore_bytes > 64 << 20
            || p.root() != expected
        {
            return Err("invocation32 original cached quota differs".into());
        }
        Ok(p)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StageTerminalV28 {
    Funded,
    Challengeable,
    Refunded,
    Convicted,
    Final,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageSettlementV28 {
    pub original: Digest,
    pub terminal: StageTerminalV28,
    pub claim: Option<Digest>,
    pub proof: Digest,
    pub established: u64,
    pub escrow_debit: u64,
    pub producer_credit: u64,
    pub requester_credit: u64,
    pub actual_slash: u64,
    pub finder_credit: u64,
    pub burned: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageJobBookV28 {
    version: u16,
    class: Digest,
    source: Digest,
    policy: Digest,
    job: FlatStageJobV14,
    admission_root: Digest,
    maximum_scope: Digest,
    maximum_resources: Digest,
    actual_scope: Digest,
    actual_resources: Digest,
    quota: Digest,
    shared_plan: Digest,
    consent: Digest,
    contract: Digest,
    temporal: Digest,
    source_plan: Digest,
    caller_source: Digest,
    paged_plan: Digest,
    final_policy: Digest,
    final_basis: Digest,
    physical: Digest,
    requester_operator: Digest,
    producer_operator: Digest,
    reward: u64,
    escrow: u64,
    reserve: u64,
    finder: u64,
    admitted: u64,
    interval: u64,
    commit_until: u64,
    challenge_window: u64,
    challenge_end: Option<u64>,
    final_wait: u64,
    final_after: Option<u64>,
    deadline: u64,
    claim: Option<Digest>,
    terminal: StageTerminalV28,
    settlement: Option<StageSettlementV28>,
}
pub struct NativeStageFundingV28 {
    pub class: Digest,
    pub source: Digest,
    pub policy: Digest,
    pub consent: Digest,
    pub contract: Digest,
    pub temporal: Digest,
    pub source_plan: Digest,
    pub caller_source: Digest,
    pub paged_plan: Digest,
    pub final_policy: Digest,
    pub final_basis: Digest,
    pub physical: Digest,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub reward: u64,
    pub actual_debit: u64,
    pub debit_receipt: Digest,
    pub reserve: u64,
    pub finder: u64,
    pub nonce: NativeRequesterNonceWitnessV11,
    pub admitted: u64,
    pub commit_until: u64,
    pub challenge_window: u64,
    pub final_wait: u64,
    pub deadline: u64,
}
impl StageJobBookV28 {
    pub fn id(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-funded-job/v28", &(28u16, self.class, self.job.root(), self.reward, self.contract))
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-funded-book/v28", self)
    }
    pub fn original_root(&self) -> Digest {
        let mut b = self.clone();
        b.escrow = b.reward;
        b.claim = None;
        b.challenge_end = None;
        b.final_after = None;
        b.terminal = StageTerminalV28::Funded;
        b.settlement = None;
        b.root()
    }
    pub fn job(&self) -> &FlatStageJobV14 {
        &self.job
    }
    pub fn class(&self) -> Digest {
        self.class
    }
    pub fn source(&self) -> Digest {
        self.source
    }
    pub fn consent(&self) -> Digest {
        self.consent
    }
    pub fn contract(&self) -> Digest {
        self.contract
    }
    pub fn quota_root(&self) -> Digest {
        self.quota
    }
    pub fn source_plan_root(&self) -> Digest {
        self.source_plan
    }
    pub fn caller_source_root(&self) -> Digest {
        self.caller_source
    }
    pub fn paged_plan_root(&self) -> Digest {
        self.paged_plan
    }
    /// Read-only roots for independent audit extraction (no state/consensus meaning change).
    pub fn maximum_scope_root(&self) -> Digest {
        self.maximum_scope
    }
    pub fn maximum_resources_root(&self) -> Digest {
        self.maximum_resources
    }
    pub fn actual_scope_root(&self) -> Digest {
        self.actual_scope
    }
    pub fn actual_resources_root(&self) -> Digest {
        self.actual_resources
    }
    pub fn final_policy_root(&self) -> Digest {
        self.final_policy
    }
    pub fn final_basis_root(&self) -> Digest {
        self.final_basis
    }
    pub fn physical_root(&self) -> Digest {
        self.physical
    }
    pub fn requester_operator(&self) -> Digest {
        self.requester_operator
    }
    pub fn producer_operator(&self) -> Digest {
        self.producer_operator
    }
    pub fn reward(&self) -> u64 {
        self.reward
    }
    pub fn reserve(&self) -> u64 {
        self.reserve
    }
    pub fn finder(&self) -> u64 {
        self.finder
    }
    pub fn admitted_interval(&self) -> u64 {
        self.interval
    }
    pub fn deadline(&self) -> u64 {
        self.deadline
    }
    pub fn challenge_end(&self) -> Option<u64> {
        self.challenge_end
    }
    pub fn final_after(&self) -> Option<u64> {
        self.final_after
    }
    pub fn claim(&self) -> Option<Digest> {
        self.claim
    }
    pub fn terminal(&self) -> StageTerminalV28 {
        self.terminal
    }
    pub fn settlement(&self) -> Option<&StageSettlementV28> {
        self.settlement.as_ref()
    }
    pub fn check(&self, class: &CheckedStageClassV28, job: &CheckedStageJobV28, quota: &CheckedStageQuotaV28) -> Result<(), String> {
        if self.version != 28
            || self.class != class.root()
            || self.source != class.source().id()
            || self.policy != class.wire().policy
            || self.job != *job.job()
            || self.admission_root != job.admission().root()
            || self.maximum_scope != job.maximum_scope().root()
            || self.maximum_resources != job.maximum_resources().root()
            || self.actual_scope != job.scope().root()
            || self.actual_resources != job.resources().root()
            || self.quota != quota.root()
            || self.shared_plan != quota.plan.record().root()
            || self.escrow > self.reward
            || self.reward == 0
            || self.requester_operator == self.producer_operator
            || self.commit_until <= self.admitted
            || self.deadline <= self.commit_until
            || self.challenge_window == 0
            || self.challenge_end.is_some_and(|end| end > self.deadline)
            || self.terminal == StageTerminalV28::Funded && (self.claim.is_some() || self.escrow == 0)
            || self.terminal == StageTerminalV28::Challengeable && (self.claim.is_none() || self.escrow != self.reward)
            || matches!(self.terminal, StageTerminalV28::Final | StageTerminalV28::Refunded | StageTerminalV28::Convicted)
                && self.escrow != 0
        {
            return Err("stage28 original immutable maximum/source/input/coin/terminal roots differ".into());
        }
        match (&self.settlement, self.terminal) {
            (None, StageTerminalV28::Funded | StageTerminalV28::Challengeable) => {}
            (Some(s), StageTerminalV28::Final | StageTerminalV28::Refunded | StageTerminalV28::Convicted) => {
                if s.original != self.original_root()
                    || s.terminal != self.terminal
                    || s.claim != self.claim
                    || s.escrow_debit != self.reward
                    || s.producer_credit.checked_add(s.requester_credit) != Some(self.reward)
                    || s.finder_credit.checked_add(s.burned) != Some(s.actual_slash)
                    || s.actual_slash > self.reserve
                    || self.terminal == StageTerminalV28::Final
                        && (s.producer_credit != self.reward
                            || s.requester_credit != 0
                            || s.actual_slash != 0
                            || self.final_after.is_none_or(|d| s.established <= d))
                    || self.terminal == StageTerminalV28::Refunded
                        && (s.requester_credit != self.reward
                            || s.producer_credit != 0
                            || s.actual_slash != 0
                            || s.claim.is_some()
                            || s.established <= self.deadline)
                    || self.terminal == StageTerminalV28::Convicted
                        && (s.requester_credit != self.reward
                            || s.producer_credit != 0
                            || s.actual_slash != self.reserve
                            || s.finder_credit != self.finder
                            || s.proof == [0; 64]
                            || s.established > self.deadline)
                {
                    return Err("stage28 terminal actual coin conservation differs".into());
                }
            }
            _ => return Err("stage28 terminal lacks original actual settlement".into()),
        }
        Ok(())
    }
    pub fn fund(
        class: &CheckedStageClassV28,
        job: &CheckedStageJobV28,
        quota: &CheckedStageQuotaV28,
        policy: &QuotaPolicyV11,
        n: &NativeStageFundingV28,
    ) -> Result<Self, String> {
        policy.check()?;
        let nonce = canonical_nonce_v11(&job.job().nonce)?;
        let end = (n.admitted / policy.interval_daa)
            .checked_add(1)
            .and_then(|k| k.checked_mul(policy.interval_daa))
            .ok_or("stage28 original interval overflow")?;
        if n.class != class.root()
            || n.source != class.source().id()
            || n.policy != class.wire().policy
            || n.requester_operator == n.producer_operator
            || n.actual_debit != n.reward
            || n.reward == 0
            || n.reserve == 0
            || n.finder == 0
            || n.finder > n.reserve
            || n.nonce.requester != job.job().requester
            || n.nonce.requester_operator_id != n.requester_operator
            || n.nonce.next_nonce != nonce.checked_add(1).ok_or("stage28 nonce overflow")?
            || n.nonce.counter_root != native_requester_nonce_counter_root_v11(&n.requester_operator, n.nonce.next_nonce)
            || n.commit_until <= n.admitted
            || n.challenge_window == 0
            || n.commit_until.checked_add(n.challenge_window).is_none_or(|x| x >= n.deadline)
            || n.deadline >= end
            || [
                n.consent,
                n.contract,
                n.temporal,
                n.source_plan,
                n.caller_source,
                n.paged_plan,
                n.final_policy,
                n.final_basis,
                n.physical,
                n.debit_receipt,
            ]
            .contains(&[0; 64])
        {
            return Err("stage28 Fund lacks exact consent/coin/deadline/shared nonce and original Final contracts".into());
        }
        if n.final_wait == 0
            || n.commit_until
                .checked_add(n.challenge_window)
                .and_then(|at| at.checked_add(n.final_wait))
                .is_none_or(|at| at >= n.deadline)
        {
            return Err("stage28 original final/delivery tail exceeds original funded deadline".into());
        }
        let b = Self {
            version: 28,
            class: n.class,
            source: n.source,
            policy: n.policy,
            job: job.job().clone(),
            admission_root: job.admission().root(),
            maximum_scope: job.maximum_scope().root(),
            maximum_resources: job.maximum_resources().root(),
            actual_scope: job.scope().root(),
            actual_resources: job.resources().root(),
            quota: quota.root(),
            shared_plan: quota.plan.record().root(),
            consent: n.consent,
            contract: n.contract,
            temporal: n.temporal,
            source_plan: n.source_plan,
            caller_source: n.caller_source,
            paged_plan: n.paged_plan,
            final_policy: n.final_policy,
            final_basis: n.final_basis,
            physical: n.physical,
            requester_operator: n.requester_operator,
            producer_operator: n.producer_operator,
            reward: n.reward,
            escrow: n.reward,
            reserve: n.reserve,
            finder: n.finder,
            admitted: n.admitted,
            interval: n.admitted / policy.interval_daa,
            commit_until: n.commit_until,
            challenge_window: n.challenge_window,
            challenge_end: None,
            final_wait: n.final_wait,
            final_after: None,
            deadline: n.deadline,
            claim: None,
            terminal: StageTerminalV28::Funded,
            settlement: None,
        };
        b.check(class, job, quota)?;
        Ok(b)
    }
    pub fn binding(&self, quota: &CheckedStageQuotaV28, policy: &QuotaPolicyV11) -> Result<SubjectBindingV11, String> {
        if self.quota != quota.root() || self.shared_plan != quota.plan.record().root() {
            return Err("stage28 shared quota original plan differs".into());
        }
        Ok(SubjectBindingV11 {
            version: 11,
            policy_root: policy.root(),
            class_id: self.class,
            class_scope9_root: self.maximum_scope,
            program_root: quota.plan.program_root,
            parameter_root: quota.plan.parameter_root,
            job_id: self.id(),
            job_input_root: self.job.root(),
            resource10_root: self.actual_resources,
            quota_plan_root: self.shared_plan,
            actual_positions: self.job.positions,
            requester: self.job.requester,
            producer: self.job.producer,
            admitted_interval: self.interval,
            deadline_daa: self.deadline,
        })
    }
    pub fn commit(
        &mut self,
        class: &CheckedStageClassV28,
        job: &CheckedStageJobV28,
        quota: &CheckedStageQuotaV28,
        producer: Digest,
        daa: u64,
        trace: &StageTraceV14,
        actual_reserved: u64,
    ) -> Result<(), String> {
        self.check(class, job, quota)?;
        if self.terminal != StageTerminalV28::Funded
            || producer != self.job.producer
            || daa < self.admitted
            || daa >= self.commit_until
            || actual_reserved < self.reserve
            || trace.version != 14
            || trace.class != self.class
            || trace.job_id != self.id()
            || trace.job_input != self.job.root()
            || trace.admission != self.admission_root
            || trace.quota != self.quota
            || trace.segment_roots.len() != crate::seg::segment_count_v1(self.job.positions) as usize
        {
            return Err("stage28 Commit lacks original own maximum/coin/claim membership".into());
        }
        let p = job.program().v1_view();
        let prepared = crate::element::PreparedV1::without_scope(&p, None)?;
        let occurrence = u16::try_from(prepared.w.occurrences.len().checked_sub(1).ok_or("stage28 output occurrence absent")?)
            .map_err(|_| "stage28 occurrence overflow")?;
        let node = job.program().output.node();
        let count = match job.program().output {
            misaka_palw_tir::program_v2::OutputDecl::Rows { .. } => self.job.positions as usize,
            _ => 1,
        };
        if trace.output_openings.len() != count {
            return Err("stage28 output count differs".into());
        }
        for (i, o) in trace.output_openings.iter().enumerate() {
            let position = if count == 1 && matches!(job.program().output, misaka_palw_tir::program_v2::OutputDecl::Final { .. }) {
                self.job.positions - 1
            } else {
                i as u32
            };
            if o.position != position
                || o.occurrence != occurrence
                || o.node != node
                || !o.authenticates(&trace.segment_roots, self.job.positions, prepared.index(occurrence, node), prepared.node_count)
            {
                return Err("stage28 delivered output unauthenticated".into());
            }
        }
        self.claim = Some(object_id(b"misaka-palw/kernel/stage-authenticated-trace/v28", &(self.contract, trace.root())));
        self.challenge_end = Some(daa.checked_add(self.challenge_window).ok_or("stage28 challenge clock overflow")?);
        self.final_after = Some(
            self.challenge_end
                .unwrap()
                .checked_add(self.final_wait)
                .ok_or("stage28 original complete delivery final clock overflow")?,
        );
        self.terminal = StageTerminalV28::Challengeable;
        self.check(class, job, quota)
    }
    pub fn commit_invocation_v32(
        &mut self,
        class: &CheckedStageClassV28,
        maximum: &CheckedStageJobV28,
        quota: &CheckedStageQuotaV28,
        invocation: &crate::flat_inputs12::composition32::CheckedStageInvocationV32<'_>,
        producer: Digest,
        daa: u64,
        trace: &StageTraceV14,
        actual_reserved: u64,
    ) -> Result<(), String> {
        self.check(class, maximum, quota)?;
        if !quota.is_invocation_v32()
            || self.terminal != StageTerminalV28::Funded
            || producer != self.job.producer
            || daa < self.admitted
            || daa >= self.commit_until
            || actual_reserved < self.reserve
            || invocation.positions() > self.job.positions
            || (trace.version != 14 && trace.version != 32)
            || trace.class != self.class
            || trace.job_id != self.id()
            || trace.job_input != invocation.trace_input_root()
            || trace.admission != invocation.job().admission().root()
            || trace.quota != quota.root()
        {
            return Err("stage32 Commit changes original maximum/actor/claim/actual invocation".into());
        }
        let view = invocation.actual_program().v1_view();
        let p = crate::element::PreparedV1::without_stage_v32(&view, invocation.actual_program())?;
        let occurrence = u16::try_from(p.w.occurrences.len().checked_sub(1).ok_or("stage32 output occurrence absent")?)
            .map_err(|_| "stage32 output occurrence overflow")?;
        let node = invocation.actual_program().output.node();
        let final_only = matches!(invocation.actual_program().output, misaka_palw_tir::program_v2::OutputDecl::Final { .. });
        let count = if final_only { 1 } else { invocation.positions() as usize };
        if trace.segment_roots.len() != crate::seg::segment_count_v1(invocation.positions()) as usize
            || (trace.version == 14 && trace.output_openings.len() != count
                || trace.version == 32 && !trace.output_openings.is_empty())
        {
            return Err("stage32 actual trace directory/output count differs".into());
        }
        for (i, o) in trace.output_openings.iter().enumerate() {
            let q = if final_only { invocation.positions() - 1 } else { i as u32 };
            if o.position != q
                || o.occurrence != occurrence
                || o.node != node
                || !o.authenticates(&trace.segment_roots, invocation.positions(), p.index(occurrence, node), p.node_count)
            {
                return Err("stage32 actual output opening differs".into());
            }
        }
        self.claim = Some(object_id(b"misaka-palw/kernel/stage-authenticated-trace/v28", &(self.contract, trace.root())));
        self.challenge_end = Some(daa.checked_add(self.challenge_window).ok_or("stage32 challenge overflow")?);
        self.final_after = Some(self.challenge_end.unwrap().checked_add(self.final_wait).ok_or("stage32 final clock overflow")?);
        self.terminal = StageTerminalV28::Challengeable;
        self.check(class, maximum, quota)
    }
    /// Consumer authenticates original numerical Final27 grant, all original
    /// source/trace/own owner rows, genuine pending closure and exact coin queue
    /// before calling. A route contains only a job key, never these authorities.
    pub fn finalize(&mut self, n: &NativeStageFinalV28) -> Result<StageSettlementV28, String> {
        if self.terminal != StageTerminalV28::Challengeable
            || self.settlement.is_some()
            || n.job != self.id()
            || n.original != self.original_root()
            || Some(n.claim) != self.claim
            || n.consent != self.consent
            || n.contract != self.contract
            || n.final_policy != self.final_policy
            || n.final_basis != self.final_basis
            || n.physical != self.physical
            || n.producer_operator != self.producer_operator
            || self.final_after.is_none_or(|at| n.established <= at)
            || n.pending_units != 0
            || n.retained_owners != 0
            || n.source_original != self.source_plan
            || !n.original_source_closed
            || !n.original_source_equivalent
            || !n.original_trace_closed
            || n.actual_escrow != self.reward
            || n.actual_producer_credit != self.reward
            || n.producer_queue == [0; 64]
            || n.original_native_conditions == [0; 64]
        {
            return Err("stage28 Final lacks exact original measured/economic/source/delivery/closure/actual income authority".into());
        }
        let s = StageSettlementV28 {
            original: self.original_root(),
            terminal: StageTerminalV28::Final,
            claim: self.claim,
            proof: n.original_native_conditions,
            established: n.established,
            escrow_debit: self.reward,
            producer_credit: self.reward,
            requester_credit: 0,
            actual_slash: 0,
            finder_credit: 0,
            burned: 0,
        };
        self.escrow = 0;
        self.terminal = StageTerminalV28::Final;
        self.settlement = Some(s.clone());
        Ok(s)
    }
    pub fn refund_unclaimed(&mut self, daa: u64) -> Result<StageSettlementV28, String> {
        if self.terminal != StageTerminalV28::Funded
            || self.claim.is_some()
            || self.escrow != self.reward
            || self.settlement.is_some()
            || daa <= self.deadline
        {
            return Err("stage28 refund lacks original expired unclaimed escrow".into());
        }
        let s = StageSettlementV28 {
            original: self.original_root(),
            terminal: StageTerminalV28::Refunded,
            claim: None,
            proof: [0; 64],
            established: daa,
            escrow_debit: self.reward,
            producer_credit: 0,
            requester_credit: self.reward,
            actual_slash: 0,
            finder_credit: 0,
            burned: 0,
        };
        self.escrow = 0;
        self.terminal = StageTerminalV28::Refunded;
        self.settlement = Some(s.clone());
        Ok(s)
    }
    pub fn convict(&mut self, n: &NativeStageConvictionV28) -> Result<StageSettlementV28, String> {
        if self.terminal != StageTerminalV28::Challengeable
            || n.job != self.id()
            || n.original != self.original_root()
            || Some(n.claim) != self.claim
            || n.producer_operator != self.producer_operator
            || n.established > self.deadline
            || n.established < self.admitted
            || n.actual_collected != self.reserve
            || n.finder == self.job.producer
            || n.finder_operator == self.producer_operator
            || n.proof == [0; 64]
            || self.escrow != self.reward
            || self.settlement.is_some()
        {
            return Err("stage28 conviction lacks original independent verified proof/default and actual collected collateral".into());
        }
        let s = StageSettlementV28 {
            original: self.original_root(),
            terminal: StageTerminalV28::Convicted,
            claim: self.claim,
            proof: n.proof,
            established: n.established,
            escrow_debit: self.reward,
            producer_credit: 0,
            requester_credit: self.reward,
            actual_slash: n.actual_collected,
            finder_credit: self.finder,
            burned: n.actual_collected.checked_sub(self.finder).ok_or("stage28 collected award deficit")?,
        };
        self.escrow = 0;
        self.terminal = StageTerminalV28::Convicted;
        self.settlement = Some(s.clone());
        Ok(s)
    }
}
pub struct NativeStageFinalV28 {
    pub job: Digest,
    pub original: Digest,
    pub claim: Digest,
    pub consent: Digest,
    pub contract: Digest,
    pub final_policy: Digest,
    pub final_basis: Digest,
    pub physical: Digest,
    pub producer_operator: Digest,
    pub established: u64,
    pub pending_units: u32,
    pub retained_owners: u32,
    pub source_original: Digest,
    pub original_source_closed: bool,
    pub original_source_equivalent: bool,
    pub original_trace_closed: bool,
    pub actual_escrow: u64,
    pub actual_producer_credit: u64,
    pub producer_queue: Digest,
    pub original_native_conditions: Digest,
}
pub struct NativeStageConvictionV28 {
    pub job: Digest,
    pub original: Digest,
    pub claim: Digest,
    pub producer_operator: Digest,
    pub finder: Digest,
    pub finder_operator: Digest,
    pub proof: Digest,
    pub actual_collected: u64,
    pub established: u64,
}
pub struct NativeStageTerminalAnchorV28 {
    pub job: Digest,
    pub original: Digest,
    pub terminal: Digest,
    pub nonce: NativeRequesterNonceWitnessV11,
    pub producer_credit: u64,
    pub requester_credit: u64,
    pub actual_slash: u64,
    pub finder_credit: u64,
    pub queue: Digest,
    pub proof: Digest,
    pub established: u64,
}
pub fn check_stage_terminal_v28(
    book: &StageJobBookV28,
    plan: &CheckedStageQuotaV28,
    policy: &QuotaPolicyV11,
    n: &NativeStageTerminalAnchorV28,
) -> Result<CheckedQuotaTerminalV11, String> {
    let s = book.settlement.as_ref().ok_or("stage28 genuine settlement absent")?;
    if n.job != book.id()
        || n.original != book.original_root()
        || n.terminal != book.root()
        || n.nonce.requester != book.job.requester
        || n.nonce.requester_operator_id != book.requester_operator
        || n.nonce.next_nonce <= canonical_nonce_v11(&book.job.nonce)?
        || n.nonce.counter_root != native_requester_nonce_counter_root_v11(&book.requester_operator, n.nonce.next_nonce)
        || n.producer_credit != s.producer_credit
        || n.requester_credit != s.requester_credit
        || n.actual_slash != s.actual_slash
        || n.finder_credit != s.finder_credit
        || n.queue == [0; 64]
        || n.proof != s.proof
        || n.established != s.established
        || book.escrow != 0
        || !matches!(book.terminal, StageTerminalV28::Final | StageTerminalV28::Refunded | StageTerminalV28::Convicted)
    {
        return Err("stage28 terminal grant lacks genuine original settlement/money/queue/nonce".into());
    }
    Ok(CheckedQuotaTerminalV11 {
        binding: book.binding(plan, policy)?,
        funded_job_root: book.original_root(),
        terminal_book_root: book.root(),
        request_key: book.consent,
    })
}
/// Native reads authenticated original28 header and permanent89 counter AFTER
/// reserving exact shared baseline and collecting requester coins. Raw wire
/// bytes can never create this admission token.
pub fn check_stage_quota_admission_v28(
    book: &StageJobBookV28,
    plan: &CheckedStageQuotaV28,
    policy: &QuotaPolicyV11,
    original_book: Digest,
    nonce: &NativeRequesterNonceWitnessV11,
    epoch: &TrustedEpochV11,
) -> Result<CheckedQuotaAdmissionV11, String> {
    epoch.check(policy)?;
    if book.terminal != StageTerminalV28::Funded
        || book.root() != original_book
        || book.original_root() != original_book
        || book.admitted != epoch.block_daa
        || book.interval != epoch.network_interval
        || book.consent == [0; 64]
        || book.escrow != book.reward
        || nonce.requester != book.job.requester
        || nonce.requester_operator_id != book.requester_operator
        || nonce.next_nonce != canonical_nonce_v11(&book.job.nonce)?.checked_add(1).ok_or("stage28 nonce overflow")?
        || nonce.counter_root != native_requester_nonce_counter_root_v11(&book.requester_operator, nonce.next_nonce)
    {
        return Err("stage28 shared admission lacks original paid native book/consent/epoch/nonce".into());
    }
    Ok(CheckedQuotaAdmissionV11 {
        binding: book.binding(plan, policy)?,
        native_admission_epoch: Some((epoch.block_hash, epoch.block_daa)),
    })
}
pub(crate) struct CheckedStageSourceAdmissionV28 {
    binding: SubjectBindingV11,
    consent: Digest,
    plan: Digest,
    operator: Digest,
    epoch: TrustedEpochV11,
}
impl CheckedStageSourceAdmissionV28 {
    pub(crate) fn into_parts(self) -> (SubjectBindingV11, Digest, Digest, Digest, TrustedEpochV11) {
        (self.binding, self.consent, self.plan, self.operator, self.epoch)
    }
}
pub fn check_stage_source_admission_v28(
    class: &CheckedStageClassV28,
    job: &CheckedStageJobV28,
    book: &StageJobBookV28,
    plan: &CheckedStageQuotaV28,
    policy: &QuotaPolicyV11,
    discovery: &discovery13::DiscoveryPolicyV13,
    source: &source16::SourcePlanV16,
    original_book: Digest,
    nonce: &NativeRequesterNonceWitnessV11,
    epoch: &TrustedEpochV11,
) -> Result<source16::CheckedSourceAdmissionV16, String> {
    book.check(class, job, plan)?;
    check_stage_quota_admission_v28(book, plan, policy, original_book, nonce, epoch)?;
    let own = plan.source_plan(class, policy, discovery)?;
    if own.root() != source.root() || source.root() != book.source_plan {
        return Err("stage28 original consent producer source quotation differs".into());
    }
    let checked = CheckedStageSourceAdmissionV28 {
        binding: book.binding(plan, policy)?,
        consent: book.consent,
        plan: source.root(),
        operator: book.producer_operator,
        epoch: epoch.clone(),
    };
    source16::from_checked_stage_admission_v28(checked, source)
}

/// Readonly original clocks for an independent authenticated receipt-window audit.
/// No wire bytes, book root or admission authority changes.
impl StageJobBookV28 {
    pub fn receipt_clocks_v1(&self) -> (u64, u64, Option<u64>) {
        (self.admitted, self.commit_until, self.challenge_end.and_then(|d| d.checked_sub(self.challenge_window)))
    }
}

/// Upper bound for NEW32 compact Commit (actual generated IDs + compact trace
/// segment roots). It deliberately excludes an all-position node directory.
/// Frozen Stage14 output openings and carrier limits are unaffected.
pub fn invocation_commit_bytes_bound_v32(positions: u32) -> Result<u64, String> {
    if positions == 0 || positions > misaka_palw_tir::program::HISTORY_BOUND_V1_HELD {
        return Err("invocation32 original Commit position range".into());
    }
    let ids = (positions as u64).checked_mul(4).ok_or("invocation32 IDs overflow")?;
    ids.checked_add(crate::seg::segment_count_v1(positions) as u64 * 64)
        .and_then(|n| n.checked_add(1024))
        .ok_or("invocation32 compact Commit overflow".into())
}
pub fn invocation_commit_parts_bound_v32(positions: u32) -> Result<u16, String> {
    u16::try_from(invocation_commit_bytes_bound_v32(positions)?.div_ceil(32 << 10))
        .ok()
        .filter(|n| *n > 0 && *n <= 512)
        .ok_or_else(|| "invocation32 original producer Commit exceeds512 parts".into())
}
