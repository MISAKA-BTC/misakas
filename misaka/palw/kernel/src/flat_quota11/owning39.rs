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
    pub protocol: Digest,
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
    pub conversion: QuotaQuantitiesV11,
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
// Complete dynamic input-PC acquisition for one public action. Whole sealed
// material borrows its already authenticated C; row/random bindings derive a
// fresh original C and must not be priced as merely a128KiB reply. The current
// random-access page reader may reauthenticate a full32KiB page for each column
// cell, so its worst case is included until a different checked backend exists.
fn public_input_restore_v39(source: &CheckedSourceV39, job: &CheckedJobMaximumV39) -> Result<QuotaQuantitiesV11, String> {
    use crate::flat_job39::{InputBindingV39, MaterialSelectionV39};
    let mut out = QuotaQuantitiesV11::default();
    for (d, b) in source.program().inputs.iter().zip(&job.input_manifest().bindings) {
        if matches!(b, InputBindingV39::Material { selection: MaterialSelectionV39::Whole, .. }) {
            continue;
        }
        let cells = d.shape.iter().try_fold(1u128, |n, d| n.checked_mul(*d as u128)).ok_or("quota39 public input cells")?;
        let raw = cells.checked_mul(d.dtype.width() as u128).ok_or("quota39 public input raw")?;
        let material = matches!(b, InputBindingV39::Material { .. });
        let reads = if material { cells.checked_mul(2).ok_or("quota39 public input calls")? } else { 0 };
        let reread = reads.checked_mul((32 << 10) + 2048).ok_or("quota39 public input authenticated page IO")?;
        let mut q = QuotaQuantitiesV11::default();
        for (a, v) in [
            (ResourceAxisV10::ScalarSteps, cells.checked_mul(if material { 128 } else { 16384 }).ok_or("quota39 input work")?),
            (ResourceAxisV10::IndexSteps, cells.checked_mul(8192).ok_or("quota39 input index")?),
            (ResourceAxisV10::ValidatedElements, cells * 2),
            (ResourceAxisV10::CopiedReferenceBytes, raw * 4 + reread * 3),
            (ResourceAxisV10::ParameterHashBytes, raw * 2 + reread),
            (ResourceAxisV10::ParameterCalls, reads + cells * 2),
            (ResourceAxisV10::RawParameterStorage, reread),
            (ResourceAxisV10::MetadataWork, cells * 16384 + reread * 64),
            (ResourceAxisV10::ParameterTreeScratch, 512 << 10),
            (ResourceAxisV10::OwnedPeakBytes, 1 << 20),
        ] {
            q.set(a, v);
        }
        out = out.checked_add(&q)?;
    }
    Ok(out)
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
    let input_restore = public_input_restore_v39(source, job)?;
    // One genuine verdict closes every original pending request, owner index
    // and refund/fee journal (at most11). Small graphs still own this fixed
    // terminal work before Fund; it cannot depend on fresh ordinary capacity.
    let terminal_cleanup = action_envelope(64u128 * (1 << 20), 1 << 20, None)?;
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
        // Request and answer context checks can both authenticate original
        // actual input C; courts also share this exact paid context boundary.
        if matches!(i, 1..=4) {
            *a = a.checked_add(&input_restore)?;
        }
        if matches!(i, 3..=5) {
            *a = a.checked_add(&terminal_cleanup)?;
        }
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
    for d in &source.program().inputs {
        let shape = d.shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
        let layout = crate::merkle3::LayoutV3::try_of(&shape).ok_or("quota39 public input tree geometry")?;
        pc_depth = pc_depth.max(log(layout.leaves(0).max(layout.leaves(1))));
    }
    for (s, (block, _)) in pr.occurrences().iter().enumerate() {
        for n in 0..source.program().blocks[*block as usize].nodes.len() {
            let shape = pr.shapes(job.maximum_positions() - 1, s as u16, n as u16)?.out;
            let layout = crate::merkle3::LayoutV3::try_of(&shape).ok_or("quota39 public trace tree geometry")?;
            pc_depth = pc_depth.max(log(layout.leaves(0).max(layout.leaves(1))));
        }
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
    let answer = action_envelope(restore_work as u128, checked_sum(restore_bytes, 128 << 10)?, None)?
        .checked_add(&fragment.checked_mul(4)?)?
        .checked_add(&input_restore)?
        .checked_add(&terminal_cleanup)?;
    let total = answer.checked_mul(requests.checked_mul(2).ok_or("quota39 producer answer attempts overflow")?)?;
    let source_plan = SourcePlanRecordV39 {
        version: 39,
        class,
        source: source.root(),
        maximum: job.root(),
        quota: plan.record().root(),
        parameter: source.parameters().root(),
        protocol: crate::flat_public39::public_protocol_id_v39(),
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
    // Every attempted part restores and writes original progress, owner,
    // shared21 counters, fixed book/plan and pending journal before payload IO.
    let fixed_rows = 32u64 << 10;
    let fixed_work = fixed_rows.checked_mul(64).ok_or("quota39 fixed row restore work")?;
    let part = action_envelope(fixed_work as u128, checked_sum(32 << 10, fixed_rows)?, None)?;
    let maximum_bytes = checked_sum(filing, 68)?;
    let pages = u16::try_from(maximum_bytes.div_ceil(32 << 10)).map_err(|_| "quota39 original proof part count")?;
    if maximum_bytes > 64 << 20 || pages == 0 || pages > 2048 {
        return Err("quota39 total sealed proof exceeds original64MiB/2048parts including all wrappers".into());
    }
    let restored = checked_sum(restore_bytes, fixed_rows)?;
    let restored_work = checked_sum(restore_work, fixed_work)?;
    let assembly = action_envelope(restored_work as u128, checked_sum(restored, maximum_bytes)?, None)?;
    let court = action_envelope(restored_work as u128, checked_sum(restored, maximum_bytes)?, Some((work, filing, scratch)))?
        .checked_add(&input_restore)?
        .checked_add(&terminal_cleanup)?;
    let conversion = action_envelope(restored_work as u128, restored, None)?;
    let guarantee = part
        .checked_mul(pages as u32 * 2)?
        .checked_add(&assembly.checked_mul(2)?)?
        .checked_add(&court.checked_mul(2)?)?
        .checked_add(&conversion)?;
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
        conversion,
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
