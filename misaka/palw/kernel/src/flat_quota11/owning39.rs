//! Private39 admission factories over shared21 accounting. Legacy class/book
//! factories are not used, and no raw quantities can create a39 funding grant.
use super::*;
pub mod source;
use crate::{
    flat_execution11::{canonical_nonce_v11, native_requester_nonce_counter_root_v11},
    flat_job39::CheckedJobMaximumV39,
    flat_prepared39::PreparedV39,
    flat_resources39::*,
    flat_source39::CheckedSourceV39,
};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SourcePlanRecordV39 {
    pub version: u16,
    pub class: Digest,
    pub source: Digest,
    pub maximum: Digest,
    pub quota: Digest,
    pub parameter: Digest,
    pub inventory: Digest,
    pub requests_per_owner: u32,
    pub lifetime_owners: u8,
    pub ordinary_requests: u32,
    pub preclaim_units: u32,
    pub requests: u32,
    pub answer_bytes: u32,
    pub answer_parts: u16,
    pub answer_attempts: u8,
    pub restore_bytes: u64,
    pub answer: QuotaQuantitiesV11,
    pub total: QuotaQuantitiesV11,
}
impl SourcePlanRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-public-source-plan/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PagedProofPlanRecordV39 {
    pub version: u16,
    pub class: Digest,
    pub source: Digest,
    pub maximum: Digest,
    pub scope: Digest,
    pub maximum_bytes: u64,
    pub pages: u16,
    pub part_bytes: u32,
    pub part_attempts: u8,
    pub court_attempts: u8,
    pub retained_owners: u8,
    pub part: QuotaQuantitiesV11,
    pub assembly: QuotaQuantitiesV11,
    pub court: QuotaQuantitiesV11,
    pub guarantee: QuotaQuantitiesV11,
}
impl PagedProofPlanRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/original-paid-proof-plan/v39", self)
    }
}
pub struct CheckedQuotaV39 {
    class: Digest,
    source: Digest,
    maximum: Digest,
    policy: Digest,
    plan: CheckedJobQuotaPlanV11,
    source_plan: SourcePlanRecordV39,
    paged: PagedProofPlanRecordV39,
    restore_bytes: u64,
}
impl CheckedQuotaV39 {
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/original-checked-quota/v39",
            &(
                39u16,
                self.class,
                self.source,
                self.maximum,
                self.policy,
                self.plan.record().root(),
                self.source_plan.root(),
                self.paged.root(),
                self.restore_bytes,
            ),
        )
    }
    pub fn plan(&self) -> &CheckedJobQuotaPlanV11 {
        &self.plan
    }
    pub fn source(&self) -> &SourcePlanRecordV39 {
        &self.source_plan
    }
    pub fn paged(&self) -> &PagedProofPlanRecordV39 {
        &self.paged
    }
    pub fn restore_bytes(&self) -> u64 {
        self.restore_bytes
    }
    pub fn class(&self) -> Digest {
        self.class
    }
    pub fn maximum(&self) -> Digest {
        self.maximum
    }
    pub fn policy(&self) -> Digest {
        self.policy
    }
}
fn checked_sum(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or("quota39 original extent overflow".into())
}
/// Class/source/root and original requested maximum have already been checked
/// by their owning39 factories. No maximum vector is accepted from a carrier.
pub fn derive_quota_v39(
    source: &CheckedSourceV39,
    job: &CheckedJobMaximumV39,
    class: Digest,
    native_policy: Digest,
    class_positions: u32,
    ordinary_requests: u32,
) -> Result<CheckedQuotaV39, String> {
    if [class, native_policy].contains(&[0; 64])
        || job.request().source != source.root()
        || job.maximum_positions() > class_positions
        || class_positions > source.original_context_max()
        || ordinary_requests > 32
    {
        return Err("quota39 original class/source/maximum request scope".into());
    }
    let full_class = derive_flat_resources_v39(source, class_positions)?;
    let requested = derive_flat_resources_v39(source, job.maximum_positions())?;
    if requested.root() != job.resources().root() {
        return Err("quota39 independently rederived requested maximum differs".into());
    }
    let pr = PreparedV39::new(source, job.maximum_positions())?;
    let restore_bytes = checked_sum(
        source.wire().to_bytes().len() as u64,
        checked_sum(borsh::to_vec(job.request()).map_err(|e| e.to_string())?.len() as u64, 65536)?,
    )?;
    // Restore39 uses the bounded canonical descriptor walk and single typed
    // program graph. Raw large material is paged/typed separately, not cloned as
    // part of every class restore or public DA request.
    let restore_work = restore_bytes
        .checked_mul(8192)
        .and_then(|n| n.checked_add(pr.node_count().checked_mul(4096)?))
        .ok_or("quota39 original restore work overflow")?;
    let post = checked_sum(
        crate::seg::segment_count_v1(job.maximum_positions()) as u64 * 64,
        checked_sum(65536, job.maximum_positions() as u64 * 4)?,
    )?;
    let source_filing = checked_sum(borsh::to_vec(source.parameters()).map_err(|e| e.to_string())?.len() as u64, 128 << 10)?;
    let scope = full_class.scope();
    let filing = checked_sum(scope.maximum_bytes.max(source_filing).max(256 << 10), 7)?;
    let work = scope.maximum_work.max(restore_work);
    let scratch = scope.maximum_scratch.max(32 << 20);
    let full = checked_sum(filing, checked_sum(restore_bytes, post)?)?;
    let sizes = [
        post.max(restore_bytes),
        checked_sum(restore_bytes, 16384)?,
        checked_sum(restore_bytes, 128 << 10)?,
        full,
        full,
        restore_bytes,
    ];
    let mut actions = [
        QuotaQuantitiesV11::default(),
        QuotaQuantitiesV11::default(),
        QuotaQuantitiesV11::default(),
        QuotaQuantitiesV11::default(),
        QuotaQuantitiesV11::default(),
        QuotaQuantitiesV11::default(),
    ];
    for (i, a) in actions.iter_mut().enumerate() {
        *a = action_envelope(restore_work as u128, sizes[i], if i == 3 || i == 4 { Some((work, filing, scratch)) } else { None })?;
        include_native_post_io_v11(a, post)?;
    }
    let plan = CheckedJobQuotaPlanV11 {
        program_root: source.program_root(),
        parameter_root: source.parameters().root(),
        resource_root: requested.root(),
        class_scope_root: full_class.scope().root(),
        actual_scope_root: requested.scope().root(),
        native_restore_work: restore_work,
        native_post_carrier_cap: post,
        source_court_bound: crate::flat_execution11::NativeSourceCourtBoundV11 {
            max_proof_bytes: source_filing,
            work: restore_work,
            scratch_bytes: scratch,
        },
        positions: job.maximum_positions(),
        baseline: QuotaQuantitiesV11::from(&requested.vector().quantities),
        action_envelopes: actions,
        action_carrier_caps: sizes,
    };
    let log = |n: u64| if n <= 1 { 0u64 } else { 64 - (n - 1).leading_zeros() as u64 };
    let mut pc_depth = 0u64;
    for d in &source.program().params {
        let sh: Vec<_> = d.shape.iter().map(|d| *d as usize).collect();
        let l = crate::merkle3::LayoutV3::try_of(&sh).ok_or("quota39 real-PC geometry")?;
        pc_depth = pc_depth.max(log(l.leaves(0).max(l.leaves(1))));
    }
    // Locate a divergent segment/position/node, then a leaf and one complete
    // scalar/selection/state relation. Both real-PC header bisection and trace
    // directory headers are charged for every operand; no fixed128 shortcut.
    let directory = checked_sum(log(pr.node_count()), checked_sum(log(job.maximum_positions() as u64), 4)?)?;
    let requests = scope
        .maximum_leaves
        .checked_mul(pc_depth.checked_add(directory).and_then(|n| n.checked_add(4)).ok_or("quota39 callback path overflow")?)
        .and_then(|n| n.checked_add(directory + pc_depth + 8))
        .ok_or("quota39 complete callback overflow")?;
    let requests_per_owner = u32::try_from(requests).map_err(|_| "quota39 callback bound exceeds u32")?;
    if requests_per_owner == 0 || requests_per_owner > 1 << 20 {
        return Err("quota39 complete public acquisition exceeds original cap".into());
    }
    let preclaim_units = u32::try_from(post.div_ceil(32 << 10))
        .ok()
        .and_then(|n| n.checked_add(1))
        .ok_or("quota39 original preclaim units overflow")?;
    let requests = requests_per_owner
        .checked_mul(4)
        .and_then(|n| n.checked_add(ordinary_requests))
        .and_then(|n| n.checked_add(preclaim_units))
        .ok_or("quota39 producer lifetime source obligation overflow")?;
    let fragment = action_envelope(65536, (32 << 10) + 4096, None)?;
    let answer =
        action_envelope(restore_work as u128, checked_sum(restore_bytes, 128 << 10)?, None)?.checked_add(&fragment.checked_mul(4)?)?;
    let total = answer.checked_mul(requests.checked_mul(2).ok_or("quota39 producer answer attempts overflow")?)?;
    let source_plan = SourcePlanRecordV39 {
        version: 39,
        class,
        source: source.root(),
        maximum: job.root(),
        quota: plan.record().root(),
        parameter: source.parameters().root(),
        inventory: source.inventory_root(),
        requests_per_owner,
        lifetime_owners: 4,
        ordinary_requests,
        preclaim_units,
        requests,
        answer_bytes: 128 << 10,
        answer_parts: 4,
        answer_attempts: 2,
        restore_bytes,
        answer,
        total,
    };
    let part = action_envelope(65536, 32 << 10, None)?;
    let maximum_bytes = checked_sum(filing, 68)?;
    let pages = u16::try_from(maximum_bytes.div_ceil(32 << 10)).map_err(|_| "quota39 original proof part count")?;
    let assembly = action_envelope(restore_work as u128, checked_sum(restore_bytes, maximum_bytes)?, None)?;
    let court = action_envelope(restore_work as u128, checked_sum(restore_bytes, maximum_bytes)?, Some((work, filing, scratch)))?;
    let guarantee = part.checked_mul(pages as u32 * 2)?.checked_add(&assembly.checked_mul(2)?)?.checked_add(&court.checked_mul(2)?)?;
    let paged = PagedProofPlanRecordV39 {
        version: 39,
        class,
        source: source.root(),
        maximum: job.root(),
        scope: scope.root(),
        maximum_bytes,
        pages,
        part_bytes: 32 << 10,
        part_attempts: 2,
        court_attempts: 2,
        retained_owners: 4,
        part,
        assembly,
        court,
        guarantee,
    };
    Ok(CheckedQuotaV39 {
        class,
        source: source.root(),
        maximum: job.root(),
        policy: native_policy,
        plan,
        source_plan,
        paged,
        restore_bytes,
    })
}
/// Non-wire metadata read ONLY by the native39 adapter from original
/// class/book/producer consent, actual escrow debit and principal89 counter.
pub struct NativeFundingAnchorV39 {
    pub class: Digest,
    pub source: Digest,
    pub maximum: Digest,
    pub job: Digest,
    pub funding: Digest,
    pub consent: Digest,
    pub requester: Digest,
    pub producer: Digest,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub nonce: Digest,
    pub admitted: u64,
    pub deadline: u64,
    pub reward: u64,
    pub actual_debit: u64,
    pub debit_receipt: Digest,
}
fn original_binding(
    plan: &CheckedQuotaV39,
    a: &NativeFundingAnchorV39,
    policy: &QuotaPolicyV11,
    nonce: &NativeRequesterNonceWitnessV11,
    epoch: &TrustedEpochV11,
    fresh: bool,
) -> Result<SubjectBindingV11, String> {
    epoch.check(policy)?;
    let used = canonical_nonce_v11(&a.nonce)?.checked_add(1).ok_or("quota39 principal nonce overflow")?;
    if a.class != plan.class
        || a.source != plan.source
        || a.maximum != plan.maximum
        || a.admitted > epoch.block_daa
        || fresh && a.admitted != epoch.block_daa
        || a.deadline <= epoch.block_daa
        || a.admitted / policy.interval_daa != epoch.network_interval
        || a.deadline >= epoch.interval_end(policy)?
        || a.requester == a.producer
        || a.requester_operator == a.producer_operator
        || a.reward == 0
        || a.reward != a.actual_debit
        || nonce.requester != a.requester
        || nonce.requester_operator_id != a.requester_operator
        || nonce.next_nonce < used
        || fresh && nonce.next_nonce != used
        || nonce.counter_root != native_requester_nonce_counter_root_v11(&a.requester_operator, nonce.next_nonce)
        || [a.job, a.funding, a.consent, a.debit_receipt, a.requester_operator, a.producer_operator].contains(&[0; 64])
    {
        return Err("quota39 lacks original current native class/max/book/consent/actual coins/operator89/epoch".into());
    }
    Ok(SubjectBindingV11 {
        version: 11,
        policy_root: policy.root(),
        class_id: plan.class,
        class_scope9_root: plan.plan.class_scope_root,
        program_root: plan.plan.program_root,
        parameter_root: plan.plan.parameter_root,
        job_id: a.job,
        job_input_root: a.maximum,
        resource10_root: plan.plan.resource_root,
        quota_plan_root: plan.plan.record().root(),
        actual_positions: plan.plan.positions,
        requester: a.requester,
        producer: a.producer,
        admitted_interval: epoch.network_interval,
        deadline_daa: a.deadline,
    })
}
pub fn check_quota_admission_v39(
    plan: &CheckedQuotaV39,
    a: &NativeFundingAnchorV39,
    policy: &QuotaPolicyV11,
    nonce: &NativeRequesterNonceWitnessV11,
    epoch: &TrustedEpochV11,
) -> Result<CheckedQuotaAdmissionV11, String> {
    Ok(CheckedQuotaAdmissionV11 {
        binding: original_binding(plan, a, policy, nonce, epoch, true)?,
        native_admission_epoch: Some((epoch.block_hash, epoch.block_daa)),
    })
}
pub fn check_current_binding_v39(
    plan: &CheckedQuotaV39,
    a: &NativeFundingAnchorV39,
    policy: &QuotaPolicyV11,
    nonce: &NativeRequesterNonceWitnessV11,
    epoch: &TrustedEpochV11,
) -> Result<SubjectBindingV11, String> {
    original_binding(plan, a, policy, nonce, epoch, false)
}
pub struct NativeTerminalAnchorV39 {
    pub job: Digest,
    pub funding: Digest,
    pub terminal_book: Digest,
    pub established: u64,
    pub producer_credit: u64,
    pub requester_credit: u64,
    pub actual_slash: u64,
    pub finder_credit: u64,
    pub escrow_closed: bool,
    pub proof: Digest,
}
pub fn check_terminal_v39(
    plan: &CheckedQuotaV39,
    a: &NativeFundingAnchorV39,
    policy: &QuotaPolicyV11,
    nonce: &NativeRequesterNonceWitnessV11,
    n: &NativeTerminalAnchorV39,
    epoch: &TrustedEpochV11,
) -> Result<CheckedQuotaTerminalV11, String> {
    epoch.check(policy)?;
    let used = canonical_nonce_v11(&a.nonce)?.checked_add(1).ok_or("quota39 original nonce overflow")?;
    if a.class != plan.class
        || a.source != plan.source
        || a.maximum != plan.maximum
        || a.reward == 0
        || a.actual_debit != a.reward
        || n.job != a.job
        || n.funding != a.funding
        || !n.escrow_closed
        || n.producer_credit.checked_add(n.requester_credit) != Some(a.reward)
        || n.finder_credit > n.actual_slash
        || n.established < a.admitted
        || n.established > epoch.block_daa
        || n.terminal_book == [0; 64]
        || nonce.requester != a.requester
        || nonce.requester_operator_id != a.requester_operator
        || nonce.next_nonce < used
        || nonce.counter_root != native_requester_nonce_counter_root_v11(&a.requester_operator, nonce.next_nonce)
    {
        return Err("quota39 terminal lacks original monetary/nonce/clock conservation".into());
    }
    if n.producer_credit != 0
        && (n.producer_credit != a.reward
            || n.requester_credit != 0
            || n.actual_slash != 0
            || n.finder_credit != 0
            || n.proof == [0; 64])
        || n.proof == [0; 64] && (n.requester_credit != a.reward || n.actual_slash != 0 || n.established <= a.deadline)
        || n.actual_slash > 0 && (n.producer_credit != 0 || n.requester_credit != a.reward || n.proof == [0; 64])
    {
        return Err("quota39 genuine terminal outcome differs".into());
    }
    let binding = SubjectBindingV11 {
        version: 11,
        policy_root: policy.root(),
        class_id: plan.class,
        class_scope9_root: plan.plan.class_scope_root,
        program_root: plan.plan.program_root,
        parameter_root: plan.plan.parameter_root,
        job_id: a.job,
        job_input_root: a.maximum,
        resource10_root: plan.plan.resource_root,
        quota_plan_root: plan.plan.record().root(),
        actual_positions: plan.plan.positions,
        requester: a.requester,
        producer: a.producer,
        admitted_interval: a.admitted / policy.interval_daa,
        deadline_daa: a.deadline,
    };
    Ok(CheckedQuotaTerminalV11 { binding, funded_job_root: a.funding, terminal_book_root: n.terminal_book, request_key: a.maximum })
}

/// Only the private original39 funding factory can create this source reserve
/// grant. Source39 child accounting has no public raw-root constructor.
pub struct CheckedSourceAdmissionV39 {
    binding: SubjectBindingV11,
    source_plan: Digest,
    consent: Digest,
    producer_operator: Digest,
    epoch: TrustedEpochV11,
}
impl CheckedSourceAdmissionV39 {
    pub(super) fn parts(&self) -> (&SubjectBindingV11, &Digest, &Digest, &TrustedEpochV11, &Digest) {
        (&self.binding, &self.producer_operator, &self.consent, &self.epoch, &self.source_plan)
    }
    pub(super) fn into_parts(self) -> (SubjectBindingV11, Digest, Digest, Digest, TrustedEpochV11) {
        (self.binding, self.source_plan, self.consent, self.producer_operator, self.epoch)
    }
}
pub fn check_source_admission_v39(
    plan: &CheckedQuotaV39,
    a: &NativeFundingAnchorV39,
    policy: &QuotaPolicyV11,
    nonce: &NativeRequesterNonceWitnessV11,
    epoch: &TrustedEpochV11,
) -> Result<CheckedSourceAdmissionV39, String> {
    let binding = original_binding(plan, a, policy, nonce, epoch, true)?;
    Ok(CheckedSourceAdmissionV39 {
        binding,
        source_plan: plan.source().root(),
        consent: a.consent,
        producer_operator: a.producer_operator,
        epoch: epoch.clone(),
    })
}
