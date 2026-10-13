//! Opt-in persisted quota transitions for explicitly funded native subjects.
//!
//! All twenty-one axes use cumulative conservative allowances, including the
//! payload/storage axes: these are neither process RSS nor a live-disk lease.
//! A native consumer must authenticate the network policy, trusted chain epoch,
//! class and input bindings, original coin funding and action authorization.
//! This module supplies no coin, reward, availability or execution-Final right.
//! Expensive failed attempts keep their receipt. Unused guarantees are not
//! refunded at close; interval rollover expires them but retains subject roots.
use crate::{
    flat_court9::complete_scope_api_bounds_v9,
    flat_resources10::{FlatResourcePlanV10, ResourceAxisV10, ResourceQuantitiesV10, derive_flat_resources_v10},
    flat_scope9::{FlatResourceScopeV9, flat_resource_scope_v9},
    hash::{Digest, object_id},
    seg_encoder::EncoderBindingV1,
    trace::ParamCommitmentsV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::program::TirProgramV1;
use std::{collections::BTreeMap, io::Cursor};
mod protected;
pub use protected::*;
mod sharded;
pub use sharded::*;
/// Explicit v13 pre-proof discovery over the same authenticated aggregates.
/// Legacy v11 paid proof seals retain their wire and quota rules.
pub mod discovery13;
pub mod source16;
pub mod trace17;
#[cfg(test)]
mod protected_tests;
/// Explicit typed-stage bindings reuse the frozen 21-axis accounting engine;
/// they cannot enter the legacy text admission constructors.
pub mod stage14;

pub const QUOTA_AXES_V11: usize = 21;
pub const FUTURE_ACTION_KINDS_V11: usize = 6;
pub const ACTION_OUTER_ALLOWANCE_V11: u64 = 16 << 10;
pub const MAX_DA_REQUEST_BYTES_V11: u64 = 1024;
pub const MAX_DA_ANSWER_BYTES_V11: u64 = 16 << 10;
pub const MAX_FIXED_ACTION_BYTES_V11: u64 = 256;
const QUOTA_ACTOR_ROW_BYTES_V11: u64 = 64 + 2 * 21 * 16;
const QUOTA_SUBJECT_ROW_BYTES_V11: u64 = 64 + 2 + 11 * 64 + 4 + 8 + 8 + 3 * 21 * 16 + 6 * 4 + 1 + 4;
const QUOTA_STATE_FIXED_BYTES_V11: u64 = 64 + 64 + 8 + 8 + 4 * 21 * 16 + 4 + 4 + 4;

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaQuantitiesV11(pub [u128; QUOTA_AXES_V11]);
impl From<&ResourceQuantitiesV10> for QuotaQuantitiesV11 {
    fn from(value: &ResourceQuantitiesV10) -> Self {
        Self(value.0)
    }
}
impl QuotaQuantitiesV11 {
    pub fn get(&self, axis: ResourceAxisV10) -> u128 {
        self.0[axis as usize]
    }
    fn set(&mut self, axis: ResourceAxisV10, value: u128) {
        self.0[axis as usize] = value;
    }
    pub fn checked_add(&self, rhs: &Self) -> Result<Self, String> {
        let mut value = Self::default();
        for (out, (a, b)) in value.0.iter_mut().zip(self.0.iter().zip(rhs.0.iter())) {
            *out = a.checked_add(*b).ok_or("quota addition overflow")?;
        }
        Ok(value)
    }
    pub fn checked_sub(&self, rhs: &Self) -> Result<Self, String> {
        let mut value = Self::default();
        for (out, (a, b)) in value.0.iter_mut().zip(self.0.iter().zip(rhs.0.iter())) {
            *out = a.checked_sub(*b).ok_or("subject guarantee exhausted")?;
        }
        Ok(value)
    }
    fn checked_mul(&self, times: u32) -> Result<Self, String> {
        let mut value = Self::default();
        for (out, a) in value.0.iter_mut().zip(self.0.iter()) {
            *out = a.checked_mul(times as u128).ok_or("quota multiplication overflow")?;
        }
        Ok(value)
    }
    fn within(&self, cap: &Self) -> Result<(), String> {
        for axis in ResourceAxisV10::ALL {
            if self.get(axis) > cap.get(axis) {
                return Err(format!("quota cap exceeded: {axis:?}"));
            }
        }
        Ok(())
    }
}

/// Fixed compiled action grammar, arithmetic and guarantee semantics. A native
/// configuration supplies the network domain and caps, never an action carrier.
pub fn quota_ruleset_id_v11() -> Digest {
    object_id(
        b"misaka-palw/kernel/resource-quota-ruleset/v11",
        &(
            crate::flat_scope9::flat_court_set_id_v9(),
            QUOTA_AXES_V11 as u32,
            FUTURE_ACTION_KINDS_V11 as u32,
            ACTION_OUTER_ALLOWANCE_V11,
            MAX_DA_REQUEST_BYTES_V11,
            MAX_DA_ANSWER_BYTES_V11,
            MAX_FIXED_ACTION_BYTES_V11,
            b"cumulative21;baseline-once;used+reserved;deadline-in-one-interval;block-holds-remaining;failed-attempts-spent;no-terminal-refund;producer-reserved-only;public-challenger-own-pool;expired-close-current-ordinary;Close-prepaid-open;mark-after-genuine-terminal;baseline-native-unspent-token;GC-closed-earlier-interval-only;permanent-native-operator-highwater-terminal-anchors;protected-paid-seal-pre-reserved;Court2-Chunk1-DARequest2;fragment32KiB-two-attempts;protected-live-GC-guard;rooted-native-state-restore;court-class-max;native9-class-restore;bounded-stored-typed-post;wholeChunk-assembly-copies;source-court-original-PCmap-plus128KiB;Court-and-assembly-max-computation-source-plus7;source-PC-tile-inventory-hash-payload-tree-axes;protected-retained-per-job4-native-producer-answer-ordinary-plus8;fixed-native-selected-row-journal;selected-original-plus-seal-reserve-sum".to_vec(),
        ),
    )
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaPolicyV11 {
    pub version: u16,
    pub enabled: bool,
    pub network_domain: Digest,
    pub ruleset_digest: Digest,
    pub interval_daa: u64,
    pub plan: QuotaQuantitiesV11,
    pub block: QuotaQuantitiesV11,
    pub actor: QuotaQuantitiesV11,
    pub network: QuotaQuantitiesV11,
    pub max_subjects: u32,
    pub max_actors: u32,
    /// Metadata, request, answer, court, chunk, close attempts respectively.
    /// Invalid expensive bodies consume their corresponding prepaid slot.
    pub future_attempts: [u32; FUTURE_ACTION_KINDS_V11],
}
impl QuotaPolicyV11 {
    /// Native supplies its independently fixed ordinary lifetime request cap
    /// from compiled authenticated network configuration, never an action
    /// caller. Quote all ordinary plus protected producer answers at Fund.
    /// This checker alone neither authenticates configuration nor admits a job.
    pub fn check_native_protected_capacity(&self, ordinary_lifetime_requests: u32) -> Result<(), String> {
        self.check()?;
        let required = ordinary_lifetime_requests
            .checked_add(PROTECTED_DA_REQUESTS_PER_JOB_MAX_V11)
            .ok_or("native producer answer count overflow")?;
        if self.future_attempts[2] < required {
            return Err("native producer answers do not cover ordinary and all protected demand slots".into());
        }
        Ok(())
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/resource-quota-policy/v11", self)
    }
    pub fn check(&self) -> Result<(), String> {
        if self.version != 11 || self.ruleset_digest != quota_ruleset_id_v11() || self.interval_daa == 0 {
            return Err("invalid fixed quota policy".into());
        }
        if !self.enabled {
            return Err("native quota policy is dormant".into());
        }
        if self.max_subjects == 0 || self.max_actors == 0 || self.future_attempts[5] == 0 {
            return Err("quota policy lacks bounded subjects/actors/close".into());
        }
        self.future_attempts.iter().try_fold(1u32, |n, count| n.checked_add(*count)).ok_or("quota attempt count overflow")?;
        Ok(())
    }
}

/// The constructor derives the interval quotient. The native caller must use
/// its actual chain context; a user's claimed block/DAA is not a trusted epoch.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TrustedEpochV11 {
    pub block_hash: Digest,
    pub block_daa: u64,
    pub network_interval: u64,
}
impl TrustedEpochV11 {
    pub fn from_chain(policy: &QuotaPolicyV11, block_hash: Digest, block_daa: u64) -> Result<Self, String> {
        policy.check()?;
        Ok(Self { block_hash, block_daa, network_interval: block_daa / policy.interval_daa })
    }
    fn check(&self, policy: &QuotaPolicyV11) -> Result<(), String> {
        if self.network_interval != self.block_daa / policy.interval_daa {
            return Err("quota epoch interval differs from trusted DAA".into());
        }
        Ok(())
    }
    fn interval_end(&self, policy: &QuotaPolicyV11) -> Result<u64, String> {
        self.network_interval
            .checked_add(1)
            .and_then(|v| v.checked_mul(policy.interval_daa))
            .ok_or("quota interval end overflow".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SubjectBindingV11 {
    pub version: u16,
    pub policy_root: Digest,
    pub class_id: Digest,
    pub class_scope9_root: Digest,
    pub program_root: Digest,
    pub parameter_root: Digest,
    pub job_id: Digest,
    pub job_input_root: Digest,
    pub resource10_root: Digest,
    pub quota_plan_root: Digest,
    pub actual_positions: u32,
    pub requester: Digest,
    pub producer: Digest,
    pub admitted_interval: u64,
    /// Includes proof/DA/chunk/retention and any post-Final liability period.
    pub deadline_daa: u64,
}
impl SubjectBindingV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/resource-quota-subject/v11", self)
    }
}

/// Non-wire native view of aux87's ORIGINAL requester/operator and aux88's
/// permanent monotone counter. Native reads original fields from authenticated
/// aux87, and counter/root from actual aux88; the caller's current rotated bond
/// and action carrier cannot supply them. The counter is never deleted/reset.
#[derive(Clone, Debug)]
pub struct NativeRequesterNonceWitnessV11 {
    pub requester: Digest,
    pub requester_operator_id: Digest,
    pub counter_root: Digest,
    pub next_nonce: u64,
}
impl NativeRequesterNonceWitnessV11 {
    fn check(&self, job: &crate::flat_execution11::ExecutionJobRecordV11) -> Result<(), String> {
        use crate::flat_execution11::{canonical_nonce_v11, native_requester_nonce_counter_root_v11};
        if self.requester != job.requester
            || self.next_nonce <= canonical_nonce_v11(&job.nonce)?
            || self.counter_root != native_requester_nonce_counter_root_v11(&self.requester_operator_id, self.next_nonce)
        {
            return Err("quota native original requester/monotone nonce counter mismatch".into());
        }
        Ok(())
    }
}
pub struct NativeQuotaAdmissionAnchorV11 {
    pub job_id: Digest,
    pub funded_job_root: Digest,
    pub book_root: Digest,
    pub request_key: Digest,
    pub nonce_witness: NativeRequesterNonceWitnessV11,
    pub terminal_book_root: Option<Digest>,
}
/// A genuine authenticated original terminal aux87 and the same operator's
/// permanent monotone counter. No per-job index or unbounded tombstone is added.
pub struct NativeQuotaTerminalAnchorV11 {
    pub job_id: Digest,
    pub funded_job_root: Digest,
    pub terminal_book_root: Digest,
    pub request_key: Digest,
    pub nonce_witness: NativeRequesterNonceWitnessV11,
}
pub struct CheckedQuotaAdmissionV11 {
    binding: SubjectBindingV11,
    native_admission_epoch: Option<(Digest, u64)>,
}
impl CheckedQuotaAdmissionV11 {
    pub fn binding(&self) -> &SubjectBindingV11 {
        &self.binding
    }
}
pub struct CheckedQuotaTerminalV11 {
    binding: SubjectBindingV11,
    funded_job_root: Digest,
    terminal_book_root: Digest,
    request_key: Digest,
}
/// Mint only from a fresh fully checked class and a live original funded book.
/// An authenticated restoration class has can_admit=false. Native authenticates
/// aux87/88 first; this constant record check does not verify a claimed chain.
pub fn check_native_quota_admission_v11<C: crate::flat_execution11::ExecutionClassContextV11>(
    class: &C,
    book: &crate::flat_execution11::ExecutionJobStateV11,
    native: &NativeQuotaAdmissionAnchorV11,
) -> Result<CheckedQuotaAdmissionV11, String> {
    use crate::flat_execution11::ExecutionTerminalV11;
    let job = book.job();
    if !class.can_admit()
        || book.terminal() != ExecutionTerminalV11::Funded
        || book.claim().is_some()
        || book.settlement().is_some()
        || book.escrow_remaining() != job.escrow_original_debit
        || native.job_id != job.id()
        || native.funded_job_root != job.root()
        || native.book_root != book.root()
        || native.request_key != job.request_key()
        || native.terminal_book_root.is_some()
    {
        return Err("quota baseline lacks a fresh checked class and authenticated original unspent native job/index".into());
    }
    native.nonce_witness.check(job)?;
    book.check_restored(class, native.funded_job_root)?;
    Ok(CheckedQuotaAdmissionV11 {
        binding: job.quota_subject(class.record())?,
        native_admission_epoch: class.native_admission_epoch(),
    })
}
/// A restored class is sufficient for cleanup of an already authenticated
/// terminal; it confers no new baseline or payout. Native preserves aux88 and
/// the exact compact terminal aux87 root after quota row collection.
pub fn check_native_quota_terminal_v11<C: crate::flat_execution11::ExecutionClassContextV11>(
    class: &C,
    book: &crate::flat_execution11::ExecutionJobStateV11,
    native: &NativeQuotaTerminalAnchorV11,
) -> Result<CheckedQuotaTerminalV11, String> {
    use crate::flat_execution11::ExecutionTerminalV11;
    let job = book.job();
    if matches!(book.terminal(), ExecutionTerminalV11::Funded | ExecutionTerminalV11::Challengeable)
        || book.escrow_remaining() != 0
        || native.job_id != job.id()
        || native.funded_job_root != job.root()
        || native.terminal_book_root != book.root()
        || native.request_key != job.request_key()
    {
        return Err("quota collection lacks the exact original terminal native book/index".into());
    }
    native.nonce_witness.check(job)?;
    book.check_restored(class, native.funded_job_root)?;
    Ok(CheckedQuotaTerminalV11 {
        binding: job.quota_subject(class.record())?,
        funded_job_root: job.root(),
        terminal_book_root: book.root(),
        request_key: job.request_key(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum QuotaActionKindV11 {
    BaselineJob,
    Metadata,
    DARequest,
    DAAnswer,
    Court,
    Chunk,
    Close,
}
impl QuotaActionKindV11 {
    fn future_index(self) -> Option<usize> {
        match self {
            Self::BaselineJob => None,
            Self::Metadata => Some(0),
            Self::DARequest => Some(1),
            Self::DAAnswer => Some(2),
            Self::Court => Some(3),
            Self::Chunk => Some(4),
            Self::Close => Some(5),
        }
    }
}

/// Constructors are the only authority for quantities. This type deliberately
/// has no wire deserializer or public quantity fields.
#[derive(Clone, Debug)]
pub struct DerivedQuotaActionV11 {
    kind: QuotaActionKindV11,
    resource_root: Digest,
    class_scope_root: Digest,
    positions: u32,
    quantities: QuotaQuantitiesV11,
}
impl DerivedQuotaActionV11 {
    pub fn kind(&self) -> QuotaActionKindV11 {
        self.kind
    }
    pub fn quantities(&self) -> &QuotaQuantitiesV11 {
        &self.quantities
    }
}

/// Internally checked metadata, not an execution or availability certificate.
/// The native route must predebit bounded decode and scope derivation before
/// calling this factory; later actions need not rederive the reference vector.
pub struct CheckedJobQuotaPlanV11 {
    program_root: Digest,
    parameter_root: Digest,
    resource_root: Digest,
    class_scope_root: Digest,
    actual_scope_root: Digest,
    native_restore_work: u64,
    native_post_carrier_cap: u64,
    source_court_bound: crate::flat_execution11::NativeSourceCourtBoundV11,
    positions: u32,
    baseline: QuotaQuantitiesV11,
    action_envelopes: [QuotaQuantitiesV11; FUTURE_ACTION_KINDS_V11],
    action_carrier_caps: [u64; FUTURE_ACTION_KINDS_V11],
}
/// A native admission creates this record internally and commits its root in
/// the authenticated job. It is not a wire-supplied price certificate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaPlanRecordV11 {
    pub version: u16,
    pub program_root: Digest,
    pub parameter_root: Digest,
    pub resource_root: Digest,
    pub class_scope_root: Digest,
    pub actual_scope_root: Digest,
    pub native_restore_work: u64,
    pub native_post_carrier_cap: u64,
    pub source_court_bound: crate::flat_execution11::NativeSourceCourtBoundV11,
    pub positions: u32,
    pub baseline: QuotaQuantitiesV11,
    pub action_envelopes: [QuotaQuantitiesV11; FUTURE_ACTION_KINDS_V11],
    pub action_carrier_caps: [u64; FUTURE_ACTION_KINDS_V11],
}
impl QuotaPlanRecordV11 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/resource-quota-plan/v11", self)
    }
}
impl CheckedJobQuotaPlanV11 {
    pub fn record(&self) -> QuotaPlanRecordV11 {
        QuotaPlanRecordV11 {
            version: 11,
            program_root: self.program_root,
            parameter_root: self.parameter_root,
            resource_root: self.resource_root,
            class_scope_root: self.class_scope_root,
            actual_scope_root: self.actual_scope_root,
            native_restore_work: self.native_restore_work,
            native_post_carrier_cap: self.native_post_carrier_cap,
            source_court_bound: self.source_court_bound.clone(),
            positions: self.positions,
            baseline: self.baseline.clone(),
            action_envelopes: self.action_envelopes.clone(),
            action_carrier_caps: self.action_carrier_caps,
        }
    }
    /// The expected root must come from the authenticated native job/state row,
    /// whose admission independently derived this exact record. The caller must
    /// never take it from the action body or an unauthenticated price declaration.
    pub fn restore(record: &QuotaPlanRecordV11, expected_root: &Digest) -> Result<Self, String> {
        if record.version != 11 || record.positions == 0 || record.root() != *expected_root {
            return Err("stored quota plan differs from admitted root".into());
        }
        Ok(Self {
            program_root: record.program_root,
            parameter_root: record.parameter_root,
            resource_root: record.resource_root,
            class_scope_root: record.class_scope_root,
            actual_scope_root: record.actual_scope_root,
            native_restore_work: record.native_restore_work,
            native_post_carrier_cap: record.native_post_carrier_cap,
            source_court_bound: record.source_court_bound.clone(),
            positions: record.positions,
            baseline: record.baseline.clone(),
            action_envelopes: record.action_envelopes.clone(),
            action_carrier_caps: record.action_carrier_caps,
        })
    }
    pub fn resource_root(&self) -> Digest {
        self.resource_root
    }
    pub fn class_scope_root(&self) -> Digest {
        self.class_scope_root
    }
    pub fn actual_scope_root(&self) -> Digest {
        self.actual_scope_root
    }
    pub fn native_restore_work(&self) -> u64 {
        self.native_restore_work
    }
    /// Guard the stored typed post before copying/decoding it on a future
    /// court action. Fixed post/evidence/outer overhead fits the 16KiB reserve;
    /// decoder prompt+generated count is P+1, encoder input count is at most L.
    pub fn source_court_bound(&self) -> &crate::flat_execution11::NativeSourceCourtBoundV11 {
        &self.source_court_bound
    }
    pub fn native_post_carrier_cap(&self) -> u64 {
        self.native_post_carrier_cap
    }
    pub fn baseline(&self) -> DerivedQuotaActionV11 {
        DerivedQuotaActionV11 {
            kind: QuotaActionKindV11::BaselineJob,
            resource_root: self.resource_root,
            class_scope_root: self.class_scope_root,
            positions: self.positions,
            quantities: self.baseline.clone(),
        }
    }
    /// The native codec enforces this cap before body allocation/decoding. The
    /// same envelope is prepaid even for a smaller or subsequently invalid body.
    pub fn action_carrier_cap(&self, kind: QuotaActionKindV11) -> Result<u64, String> {
        Ok(self.action_carrier_caps[kind.future_index().ok_or("baseline has no future carrier")?])
    }
    pub fn action(&self, kind: QuotaActionKindV11, carrier: &[u8]) -> Result<DerivedQuotaActionV11, String> {
        let index = kind.future_index().ok_or("use baseline constructor")?;
        if carrier.len() as u128 > self.action_carrier_caps[index] as u128 {
            return Err("quota action carrier exceeds independently derived bound".into());
        }
        Ok(DerivedQuotaActionV11 {
            kind,
            resource_root: self.resource_root,
            class_scope_root: self.class_scope_root,
            positions: self.positions,
            quantities: self.action_envelopes[index].clone(),
        })
    }
    fn guarantee(&self, policy: &QuotaPolicyV11) -> Result<QuotaQuantitiesV11, String> {
        self.action_envelopes
            .iter()
            .zip(policy.future_attempts)
            .try_fold(QuotaQuantitiesV11::default(), |q, (action, count)| q.checked_add(&action.checked_mul(count)?))
    }
}

fn action_envelope(metadata_work: u128, bytes: u64, court: Option<(u64, u64, u64)>) -> Result<QuotaQuantitiesV11, String> {
    let mut q = QuotaQuantitiesV11::default();
    let bytes = bytes as u128;
    let copies = bytes.checked_mul(3).ok_or("quota carrier copies overflow")?;
    let storage = bytes.checked_add(16 << 10).ok_or("quota carrier storage overflow")?;
    for (axis, quantity) in [
        (ResourceAxisV10::ScalarSteps, bytes.checked_mul(16).ok_or("quota scalar cost overflow")?),
        (ResourceAxisV10::IndexSteps, bytes.checked_mul(16).ok_or("quota index cost overflow")?),
        (ResourceAxisV10::ValidatedElements, bytes),
        (ResourceAxisV10::CopiedReferenceBytes, copies),
        (ResourceAxisV10::TraceHashBytes, copies),
        (ResourceAxisV10::ContainerAllocatorAllowance, storage),
        (ResourceAxisV10::StoredEvidenceBytes, bytes),
        (ResourceAxisV10::DeliveryBytes, bytes),
        (ResourceAxisV10::ReplyCalls, 1),
        (ResourceAxisV10::Attempts, 1),
        (ResourceAxisV10::MetadataWork, metadata_work),
        (ResourceAxisV10::OwnedPeakBytes, copies.checked_add(storage).ok_or("quota carrier peak overflow")?),
    ] {
        q.set(axis, quantity);
    }
    if let Some((work, filing, scratch)) = court {
        q.set(ResourceAxisV10::CourtWork, work as u128);
        q.set(ResourceAxisV10::CourtBytes, filing as u128);
        q.set(ResourceAxisV10::CourtScratchBytes, scratch as u128);
        q.set(
            ResourceAxisV10::OwnedPeakBytes,
            q.get(ResourceAxisV10::OwnedPeakBytes).checked_add(scratch as u128).ok_or("quota court peak overflow")?,
        );
    }
    Ok(q)
}

pub fn derive_job_quota_plan_v11(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
    class_max_positions: u32,
    actual_positions: u32,
    encoder: Option<&EncoderBindingV1>,
) -> Result<CheckedJobQuotaPlanV11, String> {
    if actual_positions == 0 || actual_positions > class_max_positions {
        return Err("quota job extent exceeds admitted class".into());
    }
    let plan = derive_flat_resources_v10(program, pc, actual_positions, encoder)?;
    let class_scope = flat_resource_scope_v9(program, class_max_positions, encoder).map_err(|e| format!("{e:?}"))?;
    let native_restore_work = native_candidate_restore_work_from_scope_v11(program, pc, &class_scope)?;
    build_job_quota_plan_v11(program, pc, actual_positions, plan, &class_scope, native_restore_work, encoder)
}

/// A freshly independently checked class owns the canonical program, PC and
/// maximum scope. The private class fields cannot be supplied or changed by a
/// wire carrier. Reuse that exact maximum scope and derive the actual extent
/// once; native still prepays this factory's actual resource/metadata work.
pub(crate) fn derive_job_quota_plan_from_checked_class_v11(
    class: &crate::flat_execution11::CheckedExecutionClassV11,
    actual_positions: u32,
) -> Result<CheckedJobQuotaPlanV11, String> {
    let record = class.record();
    let view = class.view();
    if actual_positions == 0
        || actual_positions > record.max_positions
        || view.plan.max_positions != record.max_positions
        || view.plan.program_root != record.program_root
        || view.plan.scope_root != record.class_scope9_root
        || view.resources.court_scope != record.class_scope9_root
        || view.resources.parameters != record.parameter_root
        || view.resources.root() != record.resource10_root
        || view.encoder.is_some() != record.encoder
        || crate::flat_execution11::native_source_court_bound_v11(view.program.encode().len(), class.parameters())?
            != record.source_court_bound
    {
        return Err("checked quota class context differs from exact admitted roots".into());
    }
    let plan = derive_flat_resources_v10(&view.program, class.parameters(), actual_positions, view.encoder.as_ref())?;
    if plan.vector.parameters != record.parameter_root {
        return Err("checked quota actual parameter root differs from admitted class".into());
    }
    build_job_quota_plan_v11(
        &view.program,
        class.parameters(),
        actual_positions,
        plan,
        &view.scope,
        view.plan.native_restore_work,
        view.encoder.as_ref(),
    )
}

/// Native checked canonical class context reuses its independently verified
/// maximum scope. It contains no invented class-max resource quantity vector.
/// This is a readonly metadata factory; only checked native eligibility and
/// original coin/book/nonce admission can confer a new baseline entitlement.
pub fn derive_job_quota_plan_from_checked_native_class_v11(
    class: &crate::flat_execution11::CheckedNativeExecutionClassV11,
    actual_positions: u32,
) -> Result<CheckedJobQuotaPlanV11, String> {
    let record = class.record();
    let view = class.view();
    if actual_positions == 0
        || actual_positions > record.max_positions
        || view.plan.max_positions != record.max_positions
        || view.plan.program_root != record.program_root
        || view.plan.scope_root != record.class_scope9_root
        || view.scope.root() != record.class_scope9_root
        || view.plan.resource_root != record.resource10_root
        || class.parameters().root() != record.parameter_root
        || view.encoder.is_some() != record.encoder
        || crate::flat_execution11::native_source_court_bound_v11(view.program.encode().len(), class.parameters())?
            != record.source_court_bound
    {
        return Err("native checked quota class differs from its exact admitted roots/source bounds".into());
    }
    let actual = derive_flat_resources_v10(&view.program, class.parameters(), actual_positions, view.encoder.as_ref())?;
    if actual.vector.parameters != record.parameter_root {
        return Err("native actual quota PC root differs from original class".into());
    }
    build_job_quota_plan_v11(
        &view.program,
        class.parameters(),
        actual_positions,
        actual,
        &view.scope,
        view.plan.native_restore_work,
        view.encoder.as_ref(),
    )
}

/// Exact independently derived frozen native9 class restore tariff. This
/// duplicates its explicit quantity formula, not its admission authority;
/// fused private classes reuse the already independently checked plan quote.
fn native_candidate_restore_work_from_scope_v11(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
    scope: &FlatResourceScopeV9,
) -> Result<u64, String> {
    let program_bytes = u64::try_from(program.encode().len()).map_err(|_| "quota program byte count overflow")?;
    let pc_count = u64::try_from(pc.by_instance.len()).map_err(|_| "quota PC count overflow")?;
    let scope_metadata = u64::try_from(scope.metadata_derivation_work).map_err(|_| "quota class scope work overflow")?;
    let scope_bytes =
        u64::try_from(borsh::to_vec(scope).map_err(|e| e.to_string())?.len()).map_err(|_| "quota scope bytes overflow")?;
    let nodes = u64::try_from(scope.relations.len()).map_err(|_| "quota class relation count overflow")?;
    program_bytes
        .checked_mul(1024)
        .and_then(|x| x.checked_add(pc_count.checked_mul(69 * 128)?))
        .and_then(|x| x.checked_add(scope_metadata))
        .and_then(|x| x.checked_add(scope_bytes.checked_mul(64)?))
        .and_then(|x| x.checked_add(nodes.checked_mul(4 * 1024)?))
        .and_then(|x| x.checked_add(65536))
        .ok_or("quota native class restore work overflow".into())
}

fn bounded_native_post_carrier_v11(positions: u32, encoder: Option<&EncoderBindingV1>) -> Result<u64, String> {
    let ids = encoder.map(|e| e.l as u64).unwrap_or(positions as u64 + 1);
    ACTION_OUTER_ALLOWANCE_V11
        .checked_add(ids.checked_mul(4).ok_or("quota native text carrier overflow")?)
        .and_then(|n| n.checked_add((positions as u64).div_ceil(crate::seg::SEG_LEN_V4 as u64).checked_mul(64)?))
        .ok_or("quota native post carrier overflow".into())
}

fn native_action_metadata_v11(restore: u64, post_cap: u64, assembled: u64, chunk: bool) -> Result<u128, String> {
    // Native restore then stored-post/wire parse+clone and wire metadata/hash.
    // Complete court work separately includes its scope derivation exactly once.
    let mut work = restore as u128;
    for bytes in [post_cap as u128 + assembled as u128, assembled as u128 + 1024] {
        work = work
            .checked_add(bytes.checked_mul(64).ok_or("quota native carrier work overflow")?)
            .ok_or("quota native work overflow")?;
    }
    if chunk {
        // Whole finish immediately assembles/copies and verifies. A distinct
        // 32KiB fragment transport delta still needs its own bounded count API.
        let assembly = (32u128 << 10)
            .checked_add((assembled as u128).checked_mul(3).ok_or("quota assembly copies overflow")?)
            .and_then(|n| n.checked_add(1024))
            .ok_or("quota assembly metadata overflow")?;
        work =
            work.checked_add(assembly.checked_mul(64).ok_or("quota assembly work overflow")?).ok_or("quota native work overflow")?;
    }
    Ok(work)
}

fn include_native_post_io_v11(q: &mut QuotaQuantitiesV11, post: u64) -> Result<(), String> {
    let post = post as u128;
    for (axis, extra) in [
        (ResourceAxisV10::ScalarSteps, post.checked_mul(16).ok_or("quota post validation work overflow")?),
        (ResourceAxisV10::IndexSteps, post.checked_mul(16).ok_or("quota post index work overflow")?),
        (ResourceAxisV10::ValidatedElements, post),
        (ResourceAxisV10::CopiedReferenceBytes, post.checked_mul(3).ok_or("quota post copies overflow")?),
        (ResourceAxisV10::TraceHashBytes, post.checked_mul(3).ok_or("quota post hash bytes overflow")?),
        (ResourceAxisV10::ContainerAllocatorAllowance, post),
        (ResourceAxisV10::OwnedPeakBytes, post.checked_mul(4).ok_or("quota post payload overflow")?),
    ] {
        q.set(axis, q.get(axis).checked_add(extra).ok_or("quota post axis overflow")?);
    }
    Ok(())
}

/// Source proof2's bounded decoder carries one PC map, one 4096-value tiled
/// opening, one 32KiB inventory piece, rank <=4 and paths <=64 / <=32. Charge
/// their hashing and copied decoded payload on parameter axes independently of
/// court CPU and ordinary carrier hashing; no complete tensor/tree is built.
fn include_source_court_io_v11(q: &mut QuotaQuantitiesV11, pc: &ParamCommitmentsV1) -> Result<(), String> {
    let pc_body = pc.by_instance.keys().try_fold(4u128, |n, (_, layer)| {
        n.checked_add(67 + u128::from(layer.is_some()) * 2).ok_or("quota source PC extent overflow")
    })?;
    let pc_hash = pc_body.checked_add(4).ok_or("quota source PC hash overflow")?;
    let tile_payload = crate::merkle3::TILE_V3 as u128 * 16;
    let inventory_payload = 32u128 << 10;
    let shape_payload = misaka_palw_tir::types::MAX_RANK as u128 * 8;
    let paths = (64 + 32) * 64u128;
    let payload = pc_body
        .checked_add(tile_payload + inventory_payload + shape_payload + paths)
        .ok_or("quota source decoded payload overflow")?;
    // PC root, tile prefix/raw-width values, 64 kernel node preimages,
    // tensor dtype/rank/dimensions/two roots, named inventory leaf and32 nodes.
    let hash = pc_hash
        .checked_add(26 + tile_payload + 64 * 128 + 1 + 4 + shape_payload + 128)
        .and_then(|n| n.checked_add(16 + misaka_palw_tir::program::MAX_NAME_BYTES as u128 + inventory_payload + 32 * 128))
        .ok_or("quota source parameter hash overflow")?;
    let tree = tile_payload + shape_payload + paths + 4096 + 4096;
    let container = (pc.by_instance.len() as u128)
        .checked_mul(512)
        .and_then(|n| n.checked_add(4096))
        .ok_or("quota source map/container overflow")?;
    let copies = payload.checked_mul(3).ok_or("quota source copies overflow")?;
    for (axis, extra) in [
        (ResourceAxisV10::ParameterHashBytes, hash),
        (ResourceAxisV10::RawParameterStorage, pc_body + tile_payload + inventory_payload),
        (ResourceAxisV10::ParameterTreeScratch, tree),
        (ResourceAxisV10::CopiedReferenceBytes, copies),
        (ResourceAxisV10::ContainerAllocatorAllowance, container),
        (
            ResourceAxisV10::OwnedPeakBytes,
            copies.checked_add(tree).and_then(|n| n.checked_add(container)).ok_or("quota source peak overflow")?,
        ),
    ] {
        q.set(axis, q.get(axis).checked_add(extra).ok_or("quota source axis overflow")?);
    }
    Ok(())
}

fn build_job_quota_plan_v11(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
    actual_positions: u32,
    plan: FlatResourcePlanV10,
    class_scope: &FlatResourceScopeV9,
    native_restore_work: u64,
    encoder: Option<&EncoderBindingV1>,
) -> Result<CheckedJobQuotaPlanV11, String> {
    // Native court9 restores/independently checks the registered class-max
    // scope. Actual job scope/resource remains an explicitly distinct root.
    let computation_court = complete_scope_api_bounds_v9(class_scope).map_err(|e| format!("{e:?}"))?;
    let source_court_bound = crate::flat_execution11::native_source_court_bound_v11(program.encode().len(), pc)?;
    let filing = computation_court
        .filing_bytes
        .max(source_court_bound.max_proof_bytes)
        .checked_add(7)
        .ok_or("quota court wrapper extent overflow")?;
    let work = computation_court.work.max(source_court_bound.work);
    let scratch = computation_court.scratch_bytes.max(source_court_bound.scratch_bytes);
    if filing > crate::flat_court4::FLAT_FAULT_MAX_BYTES_V4
        || work > crate::flat_court4::FLAT_FAULT_MAX_WORK_V4
        || scratch > crate::flat_court4::FLAT_FAULT_MAX_SCRATCH_V4
    {
        return Err("combined computation/source court exceeds frozen resource caps".into());
    }
    // The assembled proof limit comes from the independently derived header,
    // including a fixed bounded outer carrier; it is not an unguarded 1MiB.
    let assembled = filing.checked_add(ACTION_OUTER_ALLOWANCE_V11).ok_or("quota filing bound overflow")?;
    let metadata_carrier = (program.encode().len() as u64)
        .checked_add((pc.by_instance.len() as u64).checked_mul(80).ok_or("quota PC carrier overflow")?)
        .and_then(|b| b.checked_add(ACTION_OUTER_ALLOWANCE_V11))
        .ok_or("quota metadata carrier overflow")?;
    let native_post_carrier_cap = bounded_native_post_carrier_v11(actual_positions, encoder)?;
    let metadata = plan.vector.quantities.get(ResourceAxisV10::MetadataWork);
    let mut envelopes = [
        action_envelope(metadata, metadata_carrier, None)?,
        action_envelope(16 << 10, MAX_DA_REQUEST_BYTES_V11, None)?,
        action_envelope(16 << 10, MAX_DA_ANSWER_BYTES_V11, None)?,
        action_envelope(
            native_action_metadata_v11(native_restore_work, native_post_carrier_cap, assembled, false)?,
            assembled,
            Some((work, filing, scratch)),
        )?,
        action_envelope(
            native_action_metadata_v11(native_restore_work, native_post_carrier_cap, assembled, true)?,
            assembled,
            Some((work, filing, scratch)),
        )?,
        action_envelope(16 << 10, MAX_FIXED_ACTION_BYTES_V11, None)?,
    ];
    for action in &mut envelopes[3..5] {
        include_native_post_io_v11(action, native_post_carrier_cap)?;
        include_source_court_io_v11(action, pc)?;
    }
    Ok(CheckedJobQuotaPlanV11 {
        // Native classes use the frozen program-byte root. The resource vector
        // also commits its separate domain10 semantic program digest internally.
        program_root: crate::public::program_root_v1(&program.encode()),
        parameter_root: pc.root(),
        resource_root: plan.vector.root(),
        class_scope_root: class_scope.root(),
        actual_scope_root: plan.court.root(),
        native_restore_work,
        native_post_carrier_cap,
        source_court_bound,
        positions: actual_positions,
        baseline: QuotaQuantitiesV11::from(&plan.vector.quantities),
        action_envelopes: envelopes,
        action_carrier_caps: [
            metadata_carrier,
            MAX_DA_REQUEST_BYTES_V11,
            MAX_DA_ANSWER_BYTES_V11,
            assembled,
            assembled,
            MAX_FIXED_ACTION_BYTES_V11,
        ],
    })
}

/// Ordinary class/admission metadata has no claim on another subject's mature
/// guarantee. The native route still prepays structural decode before this
/// bounded canonical-program factory and authenticates the charging actor.
pub struct DerivedOrdinaryMetadataV11 {
    quantities: QuotaQuantitiesV11,
}
impl DerivedOrdinaryMetadataV11 {
    pub fn quantities(&self) -> &QuotaQuantitiesV11 {
        &self.quantities
    }
}
pub fn derive_ordinary_metadata_v11(program: &TirProgramV1, pc: &ParamCommitmentsV1) -> Result<DerivedOrdinaryMetadataV11, String> {
    let bytes =
        crate::flat_court4::size_with_limit(program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64).map_err(|e| e.to_string())?;
    let structural = crate::ledger::flat_candidate7::flat_candidate_registration_work_v7(bytes as usize, pc.by_instance.len())
        .ok_or("quota ordinary structural work overflow")?;
    let metadata =
        crate::ledger::flat_candidate7::flat_candidate_metadata_work_v7(program).ok_or("quota ordinary metadata work overflow")?;
    let carrier = bytes
        .checked_add((pc.by_instance.len() as u64).checked_mul(80).ok_or("quota ordinary carrier overflow")?)
        .and_then(|b| b.checked_add(ACTION_OUTER_ALLOWANCE_V11))
        .ok_or("quota ordinary carrier overflow")?;
    Ok(DerivedOrdinaryMetadataV11 { quantities: action_envelope(structural as u128 + metadata as u128, carrier, None)? })
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaIntervalSummaryV11 {
    pub consumed: QuotaQuantitiesV11,
    pub reserved: QuotaQuantitiesV11,
}
impl QuotaIntervalSummaryV11 {
    fn total(&self) -> Result<QuotaQuantitiesV11, String> {
        self.consumed.checked_add(&self.reserved)
    }
    fn admit(&self, baseline: &QuotaQuantitiesV11, guarantee: &QuotaQuantitiesV11, cap: &QuotaQuantitiesV11) -> Result<Self, String> {
        let value = Self { consumed: self.consumed.checked_add(baseline)?, reserved: self.reserved.checked_add(guarantee)? };
        value.total()?.within(cap)?;
        Ok(value)
    }
    fn consume(&self, delta: &QuotaQuantitiesV11, cap: &QuotaQuantitiesV11) -> Result<Self, String> {
        let value = Self { consumed: self.consumed.checked_add(delta)?, reserved: self.reserved.checked_sub(delta)? };
        value.total()?.within(cap)?;
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaSubjectSummaryV11 {
    pub binding: SubjectBindingV11,
    pub committed: QuotaQuantitiesV11,
    pub consumed: QuotaQuantitiesV11,
    pub guarantee_remaining: QuotaQuantitiesV11,
    pub future_attempts_used: [u32; FUTURE_ACTION_KINDS_V11],
    pub closed: bool,
    /// Retained paid seal rows, including spent rows in this interval. This
    /// rooted count bounds protected DA requests against producer answers.
    pub retained_protected_seals: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct QuotaStateV11 {
    policy_root: Digest,
    epoch: TrustedEpochV11,
    block: QuotaIntervalSummaryV11,
    network: QuotaIntervalSummaryV11,
    actors: BTreeMap<Digest, QuotaIntervalSummaryV11>,
    subjects: BTreeMap<Digest, QuotaSubjectSummaryV11>,
    protected_seals: BTreeMap<Digest, ProtectedSealSummaryV11>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct QuotaReceiptV11 {
    pub policy_root: Digest,
    pub epoch: TrustedEpochV11,
    pub subject_binding: Digest,
    pub kind: QuotaActionKindV11,
    pub consumed: QuotaQuantitiesV11,
    pub added_guarantee: QuotaQuantitiesV11,
    pub remaining_guarantee: QuotaQuantitiesV11,
}
/// A real current-epoch ordinary debit, created only by the late-terminal
/// preflight below. No deserializer or public constructor may fabricate it.
pub struct ExpiredTerminalDebitV11 {
    receipt: QuotaReceiptV11,
    actor: Digest,
    after_block: QuotaQuantitiesV11,
    after_network: QuotaQuantitiesV11,
    after_actor: QuotaQuantitiesV11,
}
/// A prepaid current-interval Close attempt. A body failure retains its debit
/// and attempt slot but leaves the subject open. The private token authorizes
/// marking only after a genuine checked native terminal and UTXO transition.
pub struct TerminalDebitV11 {
    paid: ExpiredTerminalDebitV11,
}
impl TerminalDebitV11 {
    pub fn receipt(&self) -> &QuotaReceiptV11 {
        self.paid.receipt()
    }
    pub fn actor(&self) -> Digest {
        self.paid.actor()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct QuotaCollectionReceiptV11 {
    pub policy_root: Digest,
    pub epoch: TrustedEpochV11,
    pub subject_binding: Digest,
    pub funded_job_root: Digest,
    pub terminal_book_root: Digest,
    pub request_key: Digest,
    pub consumed_current: QuotaQuantitiesV11,
}
impl ExpiredTerminalDebitV11 {
    pub fn receipt(&self) -> &QuotaReceiptV11 {
        &self.receipt
    }
    pub fn actor(&self) -> Digest {
        self.actor
    }
}
impl QuotaStateV11 {
    /// Native restore authenticates an expected root read from its actual
    /// journal/state anchor. It must never come from the same mutable carrier.
    /// A structurally conserved snapshot alone can erase ordinary consumed
    /// counters or old rows; only this rooted entry point authorizes native use.
    pub fn from_rooted_bytes(bytes: &[u8], policy: &QuotaPolicyV11, expected_native_state_root: &Digest) -> Result<Self, String> {
        let state = Self::from_bytes(policy, bytes)?;
        if state.root() != *expected_native_state_root {
            return Err("restored quota differs from authenticated native state root".into());
        }
        Ok(state)
    }
    /// Reject map counts, truncated rows, duplicate/noncanonical ordering and
    /// excess bytes before constructing map entries. The policy is trusted
    /// network configuration; its fixed counts bound snapshot allocation.
    /// This checks codec/conservation only, not native state authenticity. Real
    /// native consumers must use from_rooted_bytes with the journal/state root.
    pub fn from_bytes(policy: &QuotaPolicyV11, bytes: &[u8]) -> Result<Self, String> {
        policy.check()?;
        let maximum = QUOTA_STATE_FIXED_BYTES_V11
            .checked_add(
                QUOTA_ACTOR_ROW_BYTES_V11.checked_mul(policy.max_actors as u64).ok_or("quota snapshot actor extent overflow")?,
            )
            .and_then(|n| n.checked_add(QUOTA_SUBJECT_ROW_BYTES_V11.checked_mul(policy.max_subjects as u64)?))
            .and_then(|n| n.checked_add(PROTECTED_SEAL_ROW_BYTES_V11.checked_mul(PROTECTED_SEALS_MAX_V11 as u64)?))
            .ok_or("quota snapshot maximum extent overflow")?;
        if bytes.len() as u128 > maximum as u128 {
            return Err("quota snapshot exceeds fixed policy extent".into());
        }
        fn read<T: BorshDeserialize>(reader: &mut Cursor<&[u8]>) -> Result<T, String> {
            T::deserialize_reader(reader).map_err(|e| format!("malformed quota snapshot: {e}"))
        }
        fn check_rows(reader: &Cursor<&[u8]>, count: u32, cap: u32, width: u64) -> Result<(), String> {
            if count > cap
                || (count as u64)
                    .checked_mul(width)
                    .is_none_or(|n| n > (reader.get_ref().len() as u64).saturating_sub(reader.position()))
            {
                return Err("quota snapshot map exceeds bounded available rows".into());
            }
            Ok(())
        }
        let mut reader = Cursor::new(bytes);
        let policy_root = read(&mut reader)?;
        let epoch = read(&mut reader)?;
        let block = read(&mut reader)?;
        let network = read(&mut reader)?;
        let actor_count: u32 = read(&mut reader)?;
        check_rows(&reader, actor_count, policy.max_actors, QUOTA_ACTOR_ROW_BYTES_V11)?;
        let mut actors = BTreeMap::new();
        let mut previous = None;
        for _ in 0..actor_count {
            let key: Digest = read(&mut reader)?;
            if previous.is_some_and(|old| old >= key) {
                return Err("quota snapshot actor keys are not canonical".into());
            }
            previous = Some(key);
            actors.insert(key, read(&mut reader)?);
        }
        let subject_count: u32 = read(&mut reader)?;
        check_rows(&reader, subject_count, policy.max_subjects, QUOTA_SUBJECT_ROW_BYTES_V11)?;
        let mut subjects = BTreeMap::new();
        previous = None;
        for _ in 0..subject_count {
            let key: Digest = read(&mut reader)?;
            if previous.is_some_and(|old| old >= key) {
                return Err("quota snapshot subject keys are not canonical".into());
            }
            previous = Some(key);
            subjects.insert(key, read(&mut reader)?);
        }
        let protected_count: u32 = read(&mut reader)?;
        check_rows(&reader, protected_count, PROTECTED_SEALS_MAX_V11, PROTECTED_SEAL_ROW_BYTES_V11)?;
        let mut protected_seals = BTreeMap::new();
        previous = None;
        for _ in 0..protected_count {
            let key: Digest = read(&mut reader)?;
            if previous.is_some_and(|old| old >= key) {
                return Err("quota snapshot protected keys are not canonical".into());
            }
            previous = Some(key);
            protected_seals.insert(key, read(&mut reader)?);
        }
        if reader.position() != bytes.len() as u64 {
            return Err("quota snapshot has trailing bytes".into());
        }
        let state = Self { policy_root, epoch, block, network, actors, subjects, protected_seals };
        state.check_restored(policy)?;
        Ok(state)
    }
    pub fn new(policy: &QuotaPolicyV11, epoch: TrustedEpochV11) -> Result<Self, String> {
        policy.check()?;
        epoch.check(policy)?;
        Ok(Self {
            policy_root: policy.root(),
            epoch,
            block: QuotaIntervalSummaryV11::default(),
            network: QuotaIntervalSummaryV11::default(),
            actors: BTreeMap::new(),
            subjects: BTreeMap::new(),
            protected_seals: BTreeMap::new(),
        })
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/resource-quota-state/v11", self)
    }
    pub fn block(&self) -> &QuotaIntervalSummaryV11 {
        &self.block
    }
    pub fn network(&self) -> &QuotaIntervalSummaryV11 {
        &self.network
    }
    pub fn actor(&self, actor: &Digest) -> Option<&QuotaIntervalSummaryV11> {
        self.actors.get(actor)
    }
    pub fn subject(&self, job: &Digest) -> Option<&QuotaSubjectSummaryV11> {
        self.subjects.get(job)
    }
    /// A persisted consensus row is bounded by this fixed policy. Deserializers
    /// used for untrusted snapshots must additionally cap map counts before
    /// allocation; ordinary Borsh allocation is not a wire preflight guard.
    pub fn check_restored(&self, policy: &QuotaPolicyV11) -> Result<(), String> {
        policy.check()?;
        self.epoch.check(policy)?;
        if self.policy_root != policy.root()
            || self.subjects.len() > policy.max_subjects as usize
            || self.actors.len() > policy.max_actors as usize
            || self.protected_seals.len() > PROTECTED_SEALS_MAX_V11 as usize
        {
            return Err("restored quota policy/count mismatch".into());
        }
        self.block.total()?.within(&policy.block)?;
        self.network.total()?.within(&policy.network)?;
        let mut reserved = QuotaQuantitiesV11::default();
        let mut actor_reserved: BTreeMap<Digest, QuotaQuantitiesV11> = BTreeMap::new();
        let mut actor_subject_consumed: BTreeMap<Digest, QuotaQuantitiesV11> = BTreeMap::new();
        for (key, subject) in &self.subjects {
            if *key != subject.binding.job_id || subject.binding.version != 11 || subject.binding.policy_root != self.policy_root {
                return Err("restored quota subject identity mismatch".into());
            }
            if subject.consumed.checked_add(&subject.guarantee_remaining)? != subject.committed {
                return Err("restored quota subject conservation mismatch".into());
            }
            subject.committed.within(&policy.plan)?;
            if subject.future_attempts_used.iter().zip(policy.future_attempts).any(|(used, cap)| *used > cap) {
                return Err("restored quota attempts exceeded".into());
            }
            if subject.retained_protected_seals > PROTECTED_SEALS_PER_JOB_V11 {
                return Err("restored quota original job seal count exceeded".into());
            }
            if subject.binding.admitted_interval == self.epoch.network_interval {
                if subject.binding.deadline_daa >= self.epoch.interval_end(policy)? {
                    return Err("restored quota liability crosses interval".into());
                }
                reserved = reserved.checked_add(&subject.guarantee_remaining)?;
                let old = actor_reserved.get(&subject.binding.producer).cloned().unwrap_or_default();
                actor_reserved.insert(subject.binding.producer, old.checked_add(&subject.guarantee_remaining)?);
                let old = actor_subject_consumed.get(&subject.binding.producer).cloned().unwrap_or_default();
                actor_subject_consumed.insert(subject.binding.producer, old.checked_add(&subject.consumed)?);
            }
        }
        self.check_protected_restored(policy, &mut reserved, &mut actor_reserved, &mut actor_subject_consumed)?;
        if self.block.reserved != reserved || self.network.reserved != reserved {
            return Err("restored quota lost a subject guarantee".into());
        }
        let mut actor_consumed = QuotaQuantitiesV11::default();
        for (key, actor) in &self.actors {
            actor.total()?.within(&policy.actor)?;
            actor_consumed = actor_consumed.checked_add(&actor.consumed)?;
            actor_subject_consumed.remove(key).unwrap_or_default().within(&actor.consumed)?;
            if actor.reserved != actor_reserved.remove(key).unwrap_or_default() {
                return Err("restored actor guarantee differs from subjects".into());
            }
        }
        if !actor_reserved.is_empty() || !actor_subject_consumed.is_empty() {
            return Err("restored quota subject lacks actor summary".into());
        }
        if actor_consumed != self.network.consumed {
            return Err("restored network consumption differs from actors".into());
        }
        self.block.consumed.within(&self.network.consumed)?;
        Ok(())
    }
    fn check_context(&self, policy: &QuotaPolicyV11, epoch: &TrustedEpochV11) -> Result<(bool, bool), String> {
        policy.check()?;
        epoch.check(policy)?;
        if policy.root() != self.policy_root {
            return Err("persisted quota policy cannot change".into());
        }
        if epoch.block_hash == self.epoch.block_hash {
            if epoch != &self.epoch {
                return Err("same quota block has changed trusted identity".into());
            }
            return Ok((false, false));
        }
        if epoch.block_daa < self.epoch.block_daa || epoch.network_interval < self.epoch.network_interval {
            return Err("quota epoch did not advance".into());
        }
        Ok((true, epoch.network_interval != self.epoch.network_interval))
    }
    /// Paid ordinary preparation uses only unreserved capacity. No subject is
    /// created and no mature court/DA/chunk guarantee is consumed by a flood.
    pub fn debit_ordinary(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        action: &DerivedOrdinaryMetadataV11,
    ) -> Result<(), String> {
        self.debit_unreserved(policy, epoch, actor, &action.quantities)
    }
    /// Collect one closed earlier-interval row only after native authenticates
    /// its permanent operator nonce counter and genuine original terminal state. Charge
    /// current ordinary work; neither current consumed/reserved counters nor an
    /// old guarantee is returned. Same-interval collection is forbidden because
    /// unused guarantees must stay held until trusted interval rollover.
    pub fn collect_closed_expired(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        terminal: &CheckedQuotaTerminalV11,
    ) -> Result<QuotaCollectionReceiptV11, String> {
        self.check_context(policy, epoch)?;
        let binding = &terminal.binding;
        let subject = self.subjects.get(&binding.job_id).ok_or("collected quota subject is absent")?;
        if binding.policy_root != self.policy_root
            || subject.binding != *binding
            || !subject.closed
            || binding.admitted_interval >= epoch.network_interval
            || epoch.block_daa <= binding.deadline_daa
            || self.protected_seals.values().any(|seal| seal.key.job_id == binding.job_id)
        {
            return Err("quota collection requires an exact closed earlier-expired-interval subject".into());
        }
        // One fixed quota row plus authenticated native roots/index. Native
        // additionally prices its actual row restore/aggregate/codec processing.
        let quantities = action_envelope(16 << 10, QUOTA_SUBJECT_ROW_BYTES_V11 + 7 * 64, None)?;
        self.debit_unreserved(policy, epoch, actor, &quantities)?;
        self.subjects.remove(&binding.job_id);
        Ok(QuotaCollectionReceiptV11 {
            policy_root: self.policy_root,
            epoch: epoch.clone(),
            subject_binding: binding.root(),
            funded_job_root: terminal.funded_job_root,
            terminal_book_root: terminal.terminal_book_root,
            request_key: terminal.request_key,
            consumed_current: quantities,
        })
    }
    /// An authenticated public bond may pay the fixed current ordinary close
    /// cost even when the original liability interval has expired. Native
    /// authenticates the immutable job/plan first. This buys no old baseline,
    /// guarantee or reward; a dismissed late attempt keeps its current debit.
    /// Do not mark the old subject closed until the original escrow transition
    /// really succeeds through mark_expired_terminal_closed below.
    pub fn debit_expired_terminal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
    ) -> Result<ExpiredTerminalDebitV11, String> {
        self.check_context(policy, epoch)?;
        let old_interval_end = binding
            .admitted_interval
            .checked_add(1)
            .and_then(|i| i.checked_mul(policy.interval_daa))
            .ok_or("expired terminal interval overflow")?;
        if binding.version != 11
            || binding.policy_root != self.policy_root
            || binding.admitted_interval > epoch.network_interval
            || epoch.block_daa <= binding.deadline_daa
            || binding.deadline_daa >= old_interval_end
            || binding.resource10_root != plan.resource_root
            || binding.class_scope9_root != plan.class_scope_root
            || binding.actual_positions != plan.positions
            || binding.program_root != plan.program_root
            || binding.parameter_root != plan.parameter_root
            || binding.quota_plan_root != plan.record().root()
        {
            return Err("expired terminal differs from immutable admitted job/plan or is not expired".into());
        }
        let remaining = if let Some(subject) = self.subjects.get(&binding.job_id) {
            if subject.binding != *binding || subject.closed {
                return Err("expired terminal subject is rebound or already closed".into());
            }
            subject.guarantee_remaining.clone()
        } else {
            // A funded but unclaimed job never had a producer baseline. Its
            // authenticated permanent native job/nonce row guards duplicates.
            QuotaQuantitiesV11::default()
        };
        // Derive from the fixed CURRENT rule, never from an old plan allowance.
        let quantities = action_envelope(16 << 10, MAX_FIXED_ACTION_BYTES_V11, None)?;
        self.debit_unreserved(policy, epoch, actor, &quantities)?;
        Ok(ExpiredTerminalDebitV11 {
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
            after_actor: self.actors.get(&actor).ok_or("late terminal debit lost actor")?.consumed.clone(),
        })
    }
    /// Complete only after the genuine checked terminal book AND native
    /// original escrow/UTXO delta succeed atomically. The private book restores
    /// through an authenticated full-state root; never take it from a close
    /// carrier. Fixed beneficiaries cannot be changed by the paying public bond.
    /// Old committed/consumed/remaining vectors are retained without refunds.
    pub fn mark_expired_terminal_closed<C: crate::flat_execution11::ExecutionClassContextV11>(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
        class: &C,
        terminal: &crate::flat_execution11::ExecutionJobStateV11,
        paid: &ExpiredTerminalDebitV11,
    ) -> Result<(), String> {
        use crate::flat_execution11::ExecutionTerminalV11;
        self.check_context(policy, epoch)?;
        if self.epoch != *epoch
            || paid.receipt.policy_root != self.policy_root
            || paid.receipt.epoch != *epoch
            || paid.receipt.subject_binding != binding.root()
            || paid.after_block != self.block.consumed
            || paid.after_network != self.network.consumed
            || self.actors.get(&paid.actor).is_none_or(|a| a.consumed != paid.after_actor)
            || epoch.block_daa <= binding.deadline_daa
            || terminal.job().quota_subject(class.record())? != *binding
            || terminal.escrow_remaining() != 0
            || matches!(terminal.terminal(), ExecutionTerminalV11::Funded | ExecutionTerminalV11::Challengeable)
        {
            return Err("late terminal lacks its exact prepaid current debit and checked original terminal book".into());
        }
        let settlement = terminal.settlement().ok_or("late terminal book has no original settlement")?;
        settlement.check()?;
        if settlement.job_id != binding.job_id
            || settlement.terminal != terminal.terminal()
            || settlement.escrow_debit != terminal.job().escrow_original_debit
        {
            return Err("late terminal settlement differs from original escrow".into());
        }
        if let Some(subject) = self.subjects.get_mut(&binding.job_id) {
            if subject.binding != *binding || subject.closed {
                return Err("late terminal subject is rebound or already closed".into());
            }
            subject.closed = true;
        }
        Ok(())
    }
    /// A public challenger pays its own unreserved allowance, including an
    /// expensive dismissed filing. It cannot consume the producer's admitted
    /// guarantee. A mature paid seal with its own resource guarantee needs a
    /// separate authenticated reservation, rather than this public entry point.
    pub fn debit_public_action(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        action: &DerivedQuotaActionV11,
    ) -> Result<(), String> {
        self.check_context(policy, epoch)?;
        if actor == binding.producer
            || !matches!(action.kind, QuotaActionKindV11::Court | QuotaActionKindV11::DARequest | QuotaActionKindV11::Chunk)
        {
            return Err("public challenger action has wrong actor/kind".into());
        }
        let subject = self.subjects.get(&binding.job_id).ok_or("public challenger subject is not admitted")?;
        if subject.binding != *binding
            || subject.closed
            || binding.admitted_interval != epoch.network_interval
            || binding.deadline_daa < epoch.block_daa
            || binding.deadline_daa >= epoch.interval_end(policy)?
            || binding.quota_plan_root != plan.record().root()
            || binding.program_root != plan.program_root
            || binding.parameter_root != plan.parameter_root
            || binding.resource10_root != plan.resource_root
            || binding.class_scope9_root != plan.class_scope_root
            || binding.actual_positions != plan.positions
            || action.resource_root != plan.resource_root
            || action.class_scope_root != plan.class_scope_root
            || action.positions != plan.positions
        {
            return Err("public challenger action differs from admitted subject/plan/interval".into());
        }
        let index = action.kind.future_index().ok_or("public action is not a baseline")?;
        if action.quantities != plan.action_envelopes[index] {
            return Err("public action differs from independently derived envelope".into());
        }
        self.debit_unreserved(policy, epoch, actor, &action.quantities)
    }
    fn debit_unreserved(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        quantities: &QuotaQuantitiesV11,
    ) -> Result<(), String> {
        let (new_block, new_interval) = self.check_context(policy, epoch)?;
        let actor_old = if new_interval { None } else { self.actors.get(&actor) };
        if actor_old.is_none() && (if new_interval { 0 } else { self.actors.len() }) >= policy.max_actors as usize {
            return Err("quota interval actor count exceeded".into());
        }
        let mut block = if new_interval {
            QuotaIntervalSummaryV11::default()
        } else if new_block {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.block.reserved.clone() }
        } else {
            self.block.clone()
        };
        let mut network = if new_interval { QuotaIntervalSummaryV11::default() } else { self.network.clone() };
        let mut actor_next = actor_old.cloned().unwrap_or_default();
        for (summary, cap) in [(&mut block, &policy.block), (&mut network, &policy.network), (&mut actor_next, &policy.actor)] {
            summary.consumed = summary.consumed.checked_add(quantities)?;
            summary.total()?.within(cap)?;
        }
        if new_interval {
            self.actors.clear();
        }
        self.actors.insert(actor, actor_next);
        self.block = block;
        self.network = network;
        self.epoch = epoch.clone();
        Ok(())
    }
    /// Authenticated native consumers call before body decoding or callbacks,
    /// then persist the receipt even when the expensive action is dismissed.
    /// Summary changes are atomic on arithmetic, cap and binding failures.
    pub fn debit(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        action: &DerivedQuotaActionV11,
    ) -> Result<QuotaReceiptV11, String> {
        if matches!(action.kind, QuotaActionKindV11::BaselineJob | QuotaActionKindV11::Close) {
            return Err("quota baseline/Close requires its private checked admission/prepaid terminal token".into());
        }
        self.debit_inner(policy, epoch, actor, binding, plan, action)
    }
    /// Prepay the exact bounded Close envelope before decoding its body. This
    /// changes consumed/remaining and attempts only; it never closes a subject.
    pub fn debit_terminal(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        carrier: &[u8],
    ) -> Result<TerminalDebitV11, String> {
        let action = plan.action(QuotaActionKindV11::Close, carrier)?;
        let receipt = self.debit_inner(policy, epoch, actor, binding, plan, &action)?;
        Ok(TerminalDebitV11 {
            paid: ExpiredTerminalDebitV11 {
                receipt,
                actor,
                after_block: self.block.consumed.clone(),
                after_network: self.network.consumed.clone(),
                after_actor: self.actors.get(&actor).ok_or("terminal debit lost actor")?.consumed.clone(),
            },
        })
    }
    /// Native first commits the genuine original escrow/UTXO terminal delta
    /// atomically with this transition, then supplies the checked terminal
    /// book/index token. Neither receipt bytes nor a carrier flag may mark it.
    pub fn mark_terminal_closed(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        terminal: &CheckedQuotaTerminalV11,
        debit: &TerminalDebitV11,
    ) -> Result<(), String> {
        self.check_context(policy, epoch)?;
        let binding = &terminal.binding;
        let paid = &debit.paid;
        let subject = self.subjects.get(&binding.job_id).ok_or("terminal quota subject is absent")?;
        if self.epoch != *epoch
            || binding.admitted_interval != epoch.network_interval
            || binding.deadline_daa < epoch.block_daa
            || subject.binding != *binding
            || subject.closed
            || subject.future_attempts_used[5] == 0
            || paid.actor != binding.producer
            || paid.receipt.policy_root != self.policy_root
            || paid.receipt.epoch != *epoch
            || paid.receipt.subject_binding != binding.root()
            || paid.receipt.kind != QuotaActionKindV11::Close
            || paid.after_block != self.block.consumed
            || paid.after_network != self.network.consumed
            || self.actors.get(&paid.actor).is_none_or(|a| a.consumed != paid.after_actor)
        {
            return Err("terminal closure lacks its exact private current Close debit and checked original terminal book/index".into());
        }
        self.subjects.get_mut(&binding.job_id).expect("checked subject").closed = true;
        Ok(())
    }
    /// The sole public baseline path requires a private native checked token.
    /// Native retains permanent operator nonce counters and compact terminal anchors,
    /// even after collect_closed_expired removes the quota subject row.
    pub fn admit_baseline(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        admission: &CheckedQuotaAdmissionV11,
        plan: &CheckedJobQuotaPlanV11,
    ) -> Result<QuotaReceiptV11, String> {
        if admission.native_admission_epoch.is_some_and(|(hash, daa)| hash != epoch.block_hash || daa != epoch.block_daa) {
            return Err("native quota baseline eligibility is stale for this trusted block".into());
        }
        self.debit_inner(policy, epoch, actor, &admission.binding, plan, &plan.baseline())
    }
    fn debit_inner(
        &mut self,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        actor: Digest,
        binding: &SubjectBindingV11,
        plan: &CheckedJobQuotaPlanV11,
        action: &DerivedQuotaActionV11,
    ) -> Result<QuotaReceiptV11, String> {
        if actor != binding.producer {
            return Err("reserved quota action requires the authenticated producer".into());
        }
        let (new_block, new_interval) = self.check_context(policy, epoch)?;
        if binding.version != 11
            || binding.policy_root != self.policy_root
            || binding.resource10_root != plan.resource_root
            || binding.class_scope9_root != plan.class_scope_root
            || binding.actual_positions != plan.positions
            || binding.program_root != plan.program_root
            || binding.parameter_root != plan.parameter_root
            || binding.quota_plan_root != plan.record().root()
            || action.resource_root != plan.resource_root
            || action.class_scope_root != plan.class_scope_root
            || action.positions != plan.positions
        {
            return Err("quota subject/action differs from independently checked job/class plan".into());
        }
        if binding.admitted_interval != epoch.network_interval
            || binding.deadline_daa < epoch.block_daa
            || binding.deadline_daa >= epoch.interval_end(policy)?
        {
            return Err("quota subject liability crosses or outlives its fixed interval".into());
        }
        let old = self.subjects.get(&binding.job_id);
        if let Some(old) = old {
            if old.binding != *binding {
                return Err("quota subject binding cannot be rebound".into());
            }
            if old.closed {
                return Err("quota subject is already closed".into());
            }
        }
        let initial = action.kind == QuotaActionKindV11::BaselineJob;
        if initial != old.is_none() {
            return Err("quota baseline must be reserved exactly once before future actions".into());
        }
        if initial && self.subjects.len() >= policy.max_subjects as usize {
            return Err("quota retained subject count exceeded".into());
        }
        // The producer's authenticated reservation funds all action allowances.
        // Native challenger/requester coin debits remain separate subject rules.
        let actor_old = if new_interval { None } else { self.actors.get(&binding.producer) };
        if actor_old.is_none() && (if new_interval { 0 } else { self.actors.len() }) >= policy.max_actors as usize {
            return Err("quota interval actor count exceeded".into());
        }
        let block_old = if new_interval {
            QuotaIntervalSummaryV11::default()
        } else if new_block {
            QuotaIntervalSummaryV11 { consumed: QuotaQuantitiesV11::default(), reserved: self.block.reserved.clone() }
        } else {
            self.block.clone()
        };
        let network_old = if new_interval { QuotaIntervalSummaryV11::default() } else { self.network.clone() };
        let actor_old = actor_old.cloned().unwrap_or_default();
        let (subject, added, block, network, actor) = if initial {
            if action.quantities != plan.baseline {
                return Err("wrong independently derived baseline quantities".into());
            }
            let guarantee = plan.guarantee(policy)?;
            let committed = action.quantities.checked_add(&guarantee)?;
            committed.within(&policy.plan)?;
            (
                QuotaSubjectSummaryV11 {
                    binding: binding.clone(),
                    committed,
                    consumed: action.quantities.clone(),
                    guarantee_remaining: guarantee.clone(),
                    future_attempts_used: [0; FUTURE_ACTION_KINDS_V11],
                    closed: false,
                    retained_protected_seals: 0,
                },
                guarantee.clone(),
                block_old.admit(&action.quantities, &guarantee, &policy.block)?,
                network_old.admit(&action.quantities, &guarantee, &policy.network)?,
                actor_old.admit(&action.quantities, &guarantee, &policy.actor)?,
            )
        } else {
            let mut subject = old.expect("checked existing subject").clone();
            let index = action.kind.future_index().ok_or("future action has no index")?;
            if action.quantities != plan.action_envelopes[index]
                || subject.future_attempts_used[index] >= policy.future_attempts[index]
            {
                return Err("quota action exceeds its fixed prepaid attempt count".into());
            }
            subject.future_attempts_used[index] =
                subject.future_attempts_used[index].checked_add(1).ok_or("quota subject attempt count overflow")?;
            subject.consumed = subject.consumed.checked_add(&action.quantities)?;
            subject.guarantee_remaining = subject.guarantee_remaining.checked_sub(&action.quantities)?;
            if subject.consumed.checked_add(&subject.guarantee_remaining)? != subject.committed {
                return Err("quota subject conservation mismatch".into());
            }
            // A paid attempt is not a terminal result. Failed Close bodies
            // retain their debit/slot while court/DA and later settlement stay
            // possible. Only a genuine checked native terminal marks closed.
            (
                subject,
                QuotaQuantitiesV11::default(),
                block_old.consume(&action.quantities, &policy.block)?,
                network_old.consume(&action.quantities, &policy.network)?,
                actor_old.consume(&action.quantities, &policy.actor)?,
            )
        };
        let receipt = QuotaReceiptV11 {
            policy_root: self.policy_root,
            epoch: epoch.clone(),
            subject_binding: binding.root(),
            kind: action.kind,
            consumed: action.quantities.clone(),
            added_guarantee: added,
            remaining_guarantee: subject.guarantee_remaining.clone(),
        };
        // All arithmetic and policy checks completed before touching a map.
        if new_interval {
            self.actors.clear();
        }
        self.actors.insert(binding.producer, actor);
        self.subjects.insert(binding.job_id, subject);
        self.block = block;
        self.network = network;
        self.epoch = epoch.clone();
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_court4::tests::primitive_fixture;

    pub(super) fn fixture() -> (CheckedJobQuotaPlanV11, QuotaPolicyV11, TrustedEpochV11, SubjectBindingV11) {
        let (program, params, _) = primitive_fixture(11);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let plan = derive_job_quota_plan_v11(&program, &pc, 3, 1, None).unwrap();
        let mut policy = QuotaPolicyV11 {
            version: 11,
            enabled: true,
            network_domain: [51; 64],
            ruleset_digest: quota_ruleset_id_v11(),
            interval_daa: 100,
            plan: QuotaQuantitiesV11::default(),
            block: QuotaQuantitiesV11::default(),
            actor: QuotaQuantitiesV11::default(),
            network: QuotaQuantitiesV11::default(),
            max_subjects: 4,
            max_actors: 4,
            future_attempts: [1, 2, 2, 2, 2, 1],
        };
        let need = plan.baseline.checked_add(&plan.guarantee(&policy).unwrap()).unwrap();
        policy.plan = need.clone();
        policy.block = need.checked_mul(4).unwrap();
        policy.actor = policy.block.clone();
        policy.network = policy.block.clone();
        let epoch = TrustedEpochV11::from_chain(&policy, [52; 64], 10).unwrap();
        let binding = SubjectBindingV11 {
            version: 11,
            policy_root: policy.root(),
            class_id: [53; 64],
            class_scope9_root: plan.class_scope_root,
            program_root: plan.program_root,
            parameter_root: plan.parameter_root,
            job_id: [54; 64],
            job_input_root: [55; 64],
            resource10_root: plan.resource_root,
            quota_plan_root: plan.record().root(),
            actual_positions: 1,
            requester: [56; 64],
            producer: [57; 64],
            admitted_interval: 0,
            deadline_daa: 99,
        };
        (plan, policy, epoch, binding)
    }
    fn future(plan: &CheckedJobQuotaPlanV11, kind: QuotaActionKindV11) -> DerivedQuotaActionV11 {
        plan.action(kind, &[0; 64]).unwrap()
    }
    // Private synthetic tokens exercise counter algebra only. Real native
    // admission/collection authority is exercised independently below.
    fn synthetic_admission(binding: &SubjectBindingV11) -> CheckedQuotaAdmissionV11 {
        CheckedQuotaAdmissionV11 { binding: binding.clone(), native_admission_epoch: None }
    }
    fn admit(
        state: &mut QuotaStateV11,
        plan: &CheckedJobQuotaPlanV11,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        binding: &SubjectBindingV11,
    ) {
        state.admit_baseline(policy, epoch, binding.producer, &synthetic_admission(binding), plan).unwrap();
    }
    #[test]
    fn actual_class_scopes_and_rooted_stored_plan_are_independently_bound() {
        let (plan, _, _, binding) = fixture();
        let (program, params, _) = primitive_fixture(11);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let actual = derive_flat_resources_v10(&program, &pc, 1, None).unwrap();
        assert_eq!(plan.resource_root, actual.vector.root());
        assert_eq!(plan.actual_scope_root(), actual.court.root());
        assert_ne!(plan.class_scope_root, actual.court.root());
        let record = plan.record();
        let restored = CheckedJobQuotaPlanV11::restore(&record, &binding.quota_plan_root).unwrap();
        assert_eq!(restored.record(), record);
        assert_eq!(restored.action(QuotaActionKindV11::Court, &[1]).unwrap().quantities, plan.action_envelopes[3]);
        for change in 0..7 {
            let mut wrong = record.clone();
            match change {
                0 => wrong.version = 10,
                1 => wrong.positions += 1,
                2 => wrong.resource_root[0] ^= 1,
                3 => wrong.class_scope_root[0] ^= 1,
                4 => wrong.action_envelopes[3].0[14] -= 1,
                5 => wrong.action_carrier_caps[4] += 1,
                _ => wrong.actual_scope_root[0] ^= 1,
            }
            assert!(CheckedJobQuotaPlanV11::restore(&wrong, &binding.quota_plan_root).is_err());
        }
        assert!(derive_job_quota_plan_v11(&program, &pc, 1, 2, None).is_err());
    }
    pub(super) fn checked_class_fixture()
    -> (crate::flat_execution11::CheckedExecutionClassV11, TirProgramV1, ParamCommitmentsV1, QuotaPolicyV11) {
        checked_class_fixture_with_limit(4)
    }
    fn checked_class_fixture_with_limit(
        max_subjects: u32,
    ) -> (crate::flat_execution11::CheckedExecutionClassV11, TirProgramV1, ParamCommitmentsV1, QuotaPolicyV11) {
        checked_class_fixture_with_close_attempts(max_subjects, 1)
    }
    fn checked_class_fixture_with_close_attempts(
        max_subjects: u32,
        close_attempts: u32,
    ) -> (crate::flat_execution11::CheckedExecutionClassV11, TirProgramV1, ParamCommitmentsV1, QuotaPolicyV11) {
        use crate::{
            flat_execution11::{ExecutionHorizonsV11, ExecutionPolicyV11, NativeSourceBindingV11, derive_execution_class_v11},
            ledger::flat_candidate9::{FlatCandidateRecordV9, derive_flat_candidate_v9},
        };
        let (program, params, _) = primitive_fixture(7);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let (_, mut policy, _, _) = fixture();
        policy.interval_daa = 1000;
        policy.max_subjects = max_subjects;
        if close_attempts != 1 {
            policy.future_attempts[5] = close_attempts;
            policy.plan = policy.plan.checked_mul(close_attempts).unwrap();
            policy.block = policy.block.checked_mul(close_attempts).unwrap();
            policy.actor = policy.actor.checked_mul(close_attempts).unwrap();
            policy.network = policy.network.checked_mul(close_attempts).unwrap();
        }
        let descriptor = crate::descriptor::k2_tir_v4_descriptor().digest();
        let view = derive_flat_candidate_v9(&descriptor, &program.encode(), &pc, 3).unwrap();
        let source = NativeSourceBindingV11 {
            descriptor,
            program_root: view.plan.program_root,
            parameter_root: pc.root(),
            inventory_root: [72; 64],
            bound_daa: 1,
            matures_daa: 2,
            refuted: false,
        };
        let candidate = FlatCandidateRecordV9 {
            binding: source.id(),
            plan: view.plan,
            program_bytes: program.encode(),
            param_commitments: pc.clone(),
            scope_bytes: borsh::to_vec(&view.scope).unwrap(),
            replay_bytes: borsh::to_vec(&view.replay).unwrap(),
        };
        let execution = ExecutionPolicyV11 {
            version: 11,
            enabled: true,
            network_domain: policy.network_domain,
            quota_policy_root: policy.root(),
            horizons: ExecutionHorizonsV11 {
                commit_window: 10,
                challenge_window: 10,
                seal_maturity: 3,
                seal_ttl: 100,
                da_answer: 10,
                proof_grace: 3,
                retention: 10,
            },
            reward_cap: 10,
            external_gain_cap: 0,
            default_obligation: 0,
            required_producer_reserve: 10,
            finder_permille: 0,
        };
        let class = derive_execution_class_v11(&candidate, &source, &execution, &policy, 10).unwrap();
        (class, program, pc, policy)
    }
    #[test]
    fn freshly_checked_class_factory_matches_independent_actual_and_maximum_derivation() {
        let (class, program, pc, _) = checked_class_fixture();
        for positions in 1..=3 {
            let fused = derive_job_quota_plan_from_checked_class_v11(&class, positions).unwrap();
            let independent = derive_job_quota_plan_v11(&program, &pc, 3, positions, None).unwrap();
            assert_eq!(fused.record(), independent.record());
            assert_eq!(fused.class_scope_root(), class.record().class_scope9_root);
            assert_eq!(fused.actual_scope_root(), independent.actual_scope_root());
        }
        assert!(derive_job_quota_plan_from_checked_class_v11(&class, 0).is_err());
        assert!(derive_job_quota_plan_from_checked_class_v11(&class, 4).is_err());
    }
    #[test]
    fn actual_one_class_thirty_two_reserves_class_court_restore_post_and_whole_chunk_without_underpricing() {
        let (program, params, _) = primitive_fixture(7);
        let pc = ParamCommitmentsV1::of_v3(&params);
        let actual = derive_flat_resources_v10(&program, &pc, 1, None).unwrap();
        let class = flat_resource_scope_v9(&program, 32, None).unwrap();
        let class_court = complete_scope_api_bounds_v9(&class).unwrap();
        let actual_court = complete_scope_api_bounds_v9(&actual.court).unwrap();
        assert!(class_court.work > actual_court.work);
        let plan = derive_job_quota_plan_v11(&program, &pc, 32, 1, None).unwrap();
        assert_eq!(plan.actual_scope_root(), actual.court.root());
        assert_eq!(plan.class_scope_root(), class.root());
        assert_ne!(plan.actual_scope_root(), plan.class_scope_root());
        let candidate = crate::ledger::flat_candidate9::derive_flat_candidate_v9(
            &crate::descriptor::k2_tir_v4_descriptor().digest(),
            &program.encode(),
            &pc,
            32,
        )
        .unwrap();
        assert_eq!(plan.native_restore_work(), candidate.plan.native_restore_work);
        assert_eq!(plan.native_post_carrier_cap(), ACTION_OUTER_ALLOWANCE_V11 + 2 * 4 + 64);
        let source = crate::flat_execution11::native_source_court_bound_v11(program.encode().len(), &pc).unwrap();
        let filing = class_court.filing_bytes.max(source.max_proof_bytes) + 7;
        let assembled = filing + ACTION_OUTER_ALLOWANCE_V11;
        for kind in [QuotaActionKindV11::Court, QuotaActionKindV11::Chunk] {
            let action = plan.action(kind, &[]).unwrap();
            assert_eq!(action.quantities.get(ResourceAxisV10::CourtWork), class_court.work.max(source.work) as u128);
            assert_eq!(action.quantities.get(ResourceAxisV10::CourtBytes), filing as u128);
            assert_eq!(
                action.quantities.get(ResourceAxisV10::CourtScratchBytes),
                class_court.scratch_bytes.max(source.scratch_bytes) as u128
            );
            assert!(action.quantities.get(ResourceAxisV10::MetadataWork) > plan.native_restore_work() as u128);
            assert_eq!(
                action.quantities.get(ResourceAxisV10::CopiedReferenceBytes),
                3 * ((assembled + plan.native_post_carrier_cap()) as u128
                    + 4
                    + 67 * pc.by_instance.len() as u128
                    + 4096 * 16
                    + 32768
                    + 4 * 8
                    + 96 * 64)
            );
            assert_eq!(plan.action_carrier_cap(kind).unwrap(), assembled);
        }
        let court = plan.action(QuotaActionKindV11::Court, &[]).unwrap();
        let chunk = plan.action(QuotaActionKindV11::Chunk, &[]).unwrap();
        assert_eq!(
            chunk.quantities.get(ResourceAxisV10::MetadataWork) - court.quantities.get(ResourceAxisV10::MetadataWork),
            64 * ((32 << 10) + 3 * assembled + 1024) as u128
        );
        let (_, mut policy, epoch, mut binding) = fixture();
        let need = plan.baseline.checked_add(&plan.guarantee(&policy).unwrap()).unwrap();
        policy.plan = need.clone();
        policy.block = need.checked_mul(4).unwrap();
        policy.actor = policy.block.clone();
        policy.network = policy.block.clone();
        binding.policy_root = policy.root();
        binding.class_scope9_root = plan.class_scope_root;
        binding.program_root = plan.program_root;
        binding.parameter_root = plan.parameter_root;
        binding.resource10_root = plan.resource_root;
        binding.quota_plan_root = plan.record().root();
        let mut quota = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut quota, &plan, &policy, &epoch, &binding);
        for axis in [ResourceAxisV10::CourtWork, ResourceAxisV10::MetadataWork, ResourceAxisV10::CopiedReferenceBytes] {
            let mut underpriced = court.clone();
            underpriced.quantities.0[axis as usize] -= 1;
            let before = quota.root();
            assert!(quota.debit(&policy, &epoch, binding.producer, &binding, &plan, &underpriced).is_err());
            assert_eq!(quota.root(), before);
        }
        for field in 0..2 {
            let mut wrong = plan.record();
            if field == 0 {
                wrong.native_restore_work -= 1;
            } else {
                wrong.native_post_carrier_cap -= 1;
            }
            assert!(CheckedJobQuotaPlanV11::restore(&wrong, &binding.quota_plan_root).is_err());
        }
        quota.debit(&policy, &epoch, binding.producer, &binding, &plan, &court).unwrap();
        quota.check_restored(&policy).unwrap();
    }
    fn nonce_witness(book: &crate::flat_execution11::ExecutionJobStateV11) -> NativeRequesterNonceWitnessV11 {
        let next_nonce = crate::flat_execution11::canonical_nonce_v11(&book.job().nonce).unwrap().checked_add(1).unwrap();
        let operator = [119; 64];
        NativeRequesterNonceWitnessV11 {
            requester: book.job().requester,
            requester_operator_id: operator,
            counter_root: crate::flat_execution11::native_requester_nonce_counter_root_v11(&operator, next_nonce),
            next_nonce,
        }
    }
    pub(super) fn native_admission(book: &crate::flat_execution11::ExecutionJobStateV11) -> NativeQuotaAdmissionAnchorV11 {
        let job = book.job();
        NativeQuotaAdmissionAnchorV11 {
            job_id: job.id(),
            funded_job_root: job.root(),
            book_root: book.root(),
            request_key: job.request_key(),
            nonce_witness: nonce_witness(book),
            terminal_book_root: None,
        }
    }
    pub(super) fn native_terminal(book: &crate::flat_execution11::ExecutionJobStateV11) -> NativeQuotaTerminalAnchorV11 {
        let job = book.job();
        NativeQuotaTerminalAnchorV11 {
            job_id: job.id(),
            funded_job_root: job.root(),
            terminal_book_root: book.root(),
            request_key: job.request_key(),
            nonce_witness: nonce_witness(book),
        }
    }
    pub(super) fn prepared_native_job(
        class: &crate::flat_execution11::CheckedExecutionClassV11,
        policy: &QuotaPolicyV11,
        epoch: &TrustedEpochV11,
        nonce: u64,
    ) -> crate::flat_execution11::PreparedExecutionJobV11 {
        use crate::flat_execution11::{ExecutionJobInputV11, prepare_execution_job_v11};
        prepare_execution_job_v11(
            class,
            policy,
            epoch,
            ExecutionJobInputV11 {
                version: 11,
                prompt_root: [112; 64],
                prompt_len: 1,
                generated_count: 1,
                lengths: crate::flat_lengths7::FlatJobLengthsV7 { token: 1, patch: 0, audio: 0 },
                positions: 1,
            },
            [113; 64],
            [114; 64],
            crate::flat_execution11::nonce_bytes_v11(nonce),
            10,
            10,
            [116; 64],
        )
        .unwrap()
    }
    #[test]
    fn native_baseline_factory_rejects_every_foreign_anchor_and_public_baseline_backdoor() {
        use crate::flat_execution11::ExecutionJobStateV11;
        let (class, _, _, policy) = checked_class_fixture();
        let epoch = TrustedEpochV11::from_chain(&policy, [111; 64], 10).unwrap();
        let prepared = prepared_native_job(&class, &policy, &epoch, 115);
        let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
        let checked = check_native_quota_admission_v11(&class, &book, &native_admission(&book)).unwrap();
        for field in 0..7 {
            let mut wrong = native_admission(&book);
            match field {
                0 => wrong.job_id[0] ^= 1,
                1 => wrong.funded_job_root[0] ^= 1,
                2 => wrong.book_root[0] ^= 1,
                3 => wrong.request_key[0] ^= 1,
                4 => wrong.nonce_witness.requester[0] ^= 1,
                5 => wrong.nonce_witness.counter_root[0] ^= 1,
                _ => wrong.terminal_book_root = Some(book.root()),
            }
            assert!(check_native_quota_admission_v11(&class, &book, &wrong).is_err(), "anchor field {field}");
        }
        let mut quota = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        let before = quota.root();
        assert!(
            quota
                .debit(
                    &policy,
                    &epoch,
                    prepared.job.producer,
                    checked.binding(),
                    &prepared.quota_plan,
                    &prepared.quota_plan.baseline()
                )
                .is_err()
        );
        assert_eq!(quota.root(), before);
        assert!(quota.admit_baseline(&policy, &epoch, prepared.job.requester, &checked, &prepared.quota_plan).is_err());
        assert_eq!(quota.root(), before);
        quota.admit_baseline(&policy, &epoch, prepared.job.producer, &checked, &prepared.quota_plan).unwrap();
        let admitted = quota.root();
        assert!(quota.admit_baseline(&policy, &epoch, prepared.job.producer, &checked, &prepared.quota_plan).is_err());
        assert_eq!(quota.root(), admitted);
        assert!(check_native_quota_terminal_v11(&class, &book, &native_terminal(&book)).is_err());
        book.cancel_unclaimed(class.record(), prepared.job.deadline_daa + 1).unwrap();
        assert!(check_native_quota_admission_v11(&class, &book, &native_admission(&book)).is_err());
        check_native_quota_terminal_v11(&class, &book, &native_terminal(&book)).unwrap();
    }
    #[test]
    fn failed_current_close_keeps_debit_open_and_only_exact_genuine_terminal_marks_closed() {
        use crate::flat_execution11::ExecutionJobStateV11;
        let (class, _, _, policy) = checked_class_fixture_with_close_attempts(4, 2);
        let epoch = TrustedEpochV11::from_chain(&policy, [111; 64], 10).unwrap();
        let prepared = prepared_native_job(&class, &policy, &epoch, 115);
        let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
        let admission = check_native_quota_admission_v11(&class, &book, &native_admission(&book)).unwrap();
        let binding = admission.binding();
        let mut quota = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        quota.admit_baseline(&policy, &epoch, binding.producer, &admission, &prepared.quota_plan).unwrap();
        let before = quota.root();
        assert!(
            quota
                .debit(
                    &policy,
                    &epoch,
                    binding.producer,
                    binding,
                    &prepared.quota_plan,
                    &future(&prepared.quota_plan, QuotaActionKindV11::Close)
                )
                .is_err()
        );
        assert!(quota.debit_terminal(&policy, &epoch, binding.requester, binding, &prepared.quota_plan, &[]).is_err());
        assert!(
            quota
                .debit_terminal(
                    &policy,
                    &epoch,
                    binding.producer,
                    binding,
                    &prepared.quota_plan,
                    &vec![0; MAX_FIXED_ACTION_BYTES_V11 as usize + 1]
                )
                .is_err()
        );
        assert_eq!(quota.root(), before);
        let failed = quota.debit_terminal(&policy, &epoch, binding.producer, binding, &prepared.quota_plan, &[]).unwrap();
        assert!(book.cancel_unclaimed(class.record(), epoch.block_daa).is_err());
        assert!(check_native_quota_terminal_v11(&class, &book, &native_terminal(&book)).is_err());
        let subject = quota.subject(&binding.job_id).unwrap();
        assert!(!subject.closed);
        assert_eq!(subject.future_attempts_used[5], 1);
        assert_eq!(failed.receipt().consumed, prepared.quota_plan.action_envelopes[5]);
        let total = quota.network.total().unwrap();
        quota
            .debit(
                &policy,
                &epoch,
                binding.producer,
                binding,
                &prepared.quota_plan,
                &future(&prepared.quota_plan, QuotaActionKindV11::Court),
            )
            .unwrap();
        assert_eq!(quota.network.total().unwrap(), total);
        assert!(!quota.subject(&binding.job_id).unwrap().closed);
        let closing = TrustedEpochV11::from_chain(&policy, [117; 64], prepared.job.commit_deadline + 1).unwrap();
        let paid = quota.debit_terminal(&policy, &closing, binding.producer, binding, &prepared.quota_plan, &[]).unwrap();
        assert!(!quota.subject(&binding.job_id).unwrap().closed);
        let refund = book.cancel_unclaimed(class.record(), closing.block_daa).unwrap();
        assert_eq!(refund.requester_refund, prepared.job.escrow_original_debit);
        let terminal = check_native_quota_terminal_v11(&class, &book, &native_terminal(&book)).unwrap();
        let before_mark = quota.root();
        assert!(quota.mark_terminal_closed(&policy, &closing, &terminal, &failed).is_err());
        assert_eq!(quota.root(), before_mark);
        quota.mark_terminal_closed(&policy, &closing, &terminal, &paid).unwrap();
        assert!(quota.subject(&binding.job_id).unwrap().closed);
        assert_eq!(quota.subject(&binding.job_id).unwrap().future_attempts_used[5], 2);
        assert_eq!(quota.network.total().unwrap(), total);
        assert_ne!(quota.subject(&binding.job_id).unwrap().guarantee_remaining, QuotaQuantitiesV11::default());
        let closed = quota.root();
        assert!(quota.mark_terminal_closed(&policy, &closing, &terminal, &paid).is_err());
        assert!(quota.debit_terminal(&policy, &closing, binding.producer, binding, &prepared.quota_plan, &[]).is_err());
        assert!(book.cancel_unclaimed(class.record(), closing.block_daa).is_err());
        assert_eq!(quota.root(), closed);
        quota.check_restored(&policy).unwrap();
    }
    #[test]
    fn rooted_native_restore_refuses_locally_conserved_counter_row_erasure_and_guarantee_increase() {
        let (plan, policy, epoch, binding) = fixture();
        let mut quota = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut quota, &plan, &policy, &epoch, &binding);
        let authenticated = quota.root();
        let original = borsh::to_vec(&quota).unwrap();
        assert_eq!(QuotaStateV11::from_rooted_bytes(&original, &policy, &authenticated).unwrap(), quota);
        let empty = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        let erased = borsh::to_vec(&empty).unwrap();
        assert!(QuotaStateV11::from_bytes(&policy, &erased).is_ok());
        assert!(QuotaStateV11::from_rooted_bytes(&erased, &policy, &authenticated).is_err());
        // Every structural conservation equation still holds after forging one
        // spent unit into a fresh guarantee. The native root forbids this rewind.
        let mut increased = quota.clone();
        let axis = ResourceAxisV10::MetadataWork as usize;
        assert!(increased.subjects[&binding.job_id].consumed.0[axis] > 0);
        let subject = increased.subjects.get_mut(&binding.job_id).unwrap();
        subject.consumed.0[axis] -= 1;
        subject.guarantee_remaining.0[axis] += 1;
        for summary in [&mut increased.block, &mut increased.network, increased.actors.get_mut(&binding.producer).unwrap()] {
            summary.consumed.0[axis] -= 1;
            summary.reserved.0[axis] += 1;
        }
        let rewound = borsh::to_vec(&increased).unwrap();
        assert!(QuotaStateV11::from_bytes(&policy, &rewound).is_ok());
        assert!(QuotaStateV11::from_rooted_bytes(&rewound, &policy, &authenticated).is_err());
        // An old closed row can also be deleted while all current-interval
        // counters remain conserved. Only authenticated paid GC may do so.
        let later = TrustedEpochV11::from_chain(&policy, [171; 64], 150).unwrap();
        quota.debit_expired_terminal(&policy, &later, [172; 64], &binding, &plan).unwrap();
        quota.subjects.get_mut(&binding.job_id).unwrap().closed = true; // test-only genuine-closure analogue
        let with_old_closed = quota.root();
        quota.check_restored(&policy).unwrap();
        let mut removed = quota.clone();
        removed.subjects.remove(&binding.job_id);
        let removed_bytes = borsh::to_vec(&removed).unwrap();
        assert!(QuotaStateV11::from_bytes(&policy, &removed_bytes).is_ok());
        assert!(QuotaStateV11::from_rooted_bytes(&removed_bytes, &policy, &with_old_closed).is_err());
        let mut wrong_root = authenticated;
        wrong_root[0] ^= 1;
        assert!(QuotaStateV11::from_rooted_bytes(&original, &policy, &wrong_root).is_err());
    }
    #[test]
    fn closed_expired_collection_allows_new_jobs_without_nonce_reuse_or_old_guarantee_refunds() {
        use crate::flat_execution11::ExecutionJobStateV11;
        let (class, _, _, policy) = checked_class_fixture_with_limit(1);
        let epoch = TrustedEpochV11::from_chain(&policy, [111; 64], 10).unwrap();
        let prepared = prepared_native_job(&class, &policy, &epoch, 115);
        let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
        let admission = check_native_quota_admission_v11(&class, &book, &native_admission(&book)).unwrap();
        let binding = admission.binding().clone();
        let mut quota = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        quota.admit_baseline(&policy, &epoch, binding.producer, &admission, &prepared.quota_plan).unwrap();
        let committed = quota.subject(&binding.job_id).unwrap().clone();
        let late = TrustedEpochV11::from_chain(&policy, [121; 64], prepared.job.deadline_daa + 1).unwrap();
        let paid = quota.debit_expired_terminal(&policy, &late, [122; 64], &binding, &prepared.quota_plan).unwrap();
        let settlement = book.cancel_unclaimed(class.record(), late.block_daa).unwrap();
        assert_eq!(settlement.requester_refund, prepared.job.escrow_original_debit);
        assert_eq!(settlement.producer_credit, 0);
        quota.mark_expired_terminal_closed(&policy, &late, &binding, &class, &book, &paid).unwrap();
        let terminal = check_native_quota_terminal_v11(&class, &book, &native_terminal(&book)).unwrap();
        let same_interval = quota.root();
        assert!(quota.collect_closed_expired(&policy, &late, [122; 64], &terminal).is_err());
        assert_eq!(quota.root(), same_interval);
        assert_eq!(quota.subject(&binding.job_id).unwrap().consumed, committed.consumed);
        assert_eq!(quota.subject(&binding.job_id).unwrap().guarantee_remaining, committed.guarantee_remaining);
        let next = TrustedEpochV11::from_chain(&policy, [123; 64], 1100).unwrap();
        let fresh = prepared_native_job(&class, &policy, &next, 124);
        let fresh_book = ExecutionJobStateV11::from_native_funded_job(fresh.job.clone()).unwrap();
        let fresh_admission = check_native_quota_admission_v11(&class, &fresh_book, &native_admission(&fresh_book)).unwrap();
        let before_collection = quota.root();
        assert!(quota.admit_baseline(&policy, &next, fresh.job.producer, &fresh_admission, &fresh.quota_plan).is_err());
        assert_eq!(quota.root(), before_collection);
        let receipt = quota.collect_closed_expired(&policy, &next, [122; 64], &terminal).unwrap();
        assert_eq!(receipt.funded_job_root, prepared.job.root());
        assert_eq!(receipt.terminal_book_root, book.root());
        assert_eq!(receipt.request_key, prepared.job.request_key());
        assert!(quota.subject(&binding.job_id).is_none());
        assert_eq!(quota.network.consumed, receipt.consumed_current);
        assert_eq!(quota.block.consumed, receipt.consumed_current);
        assert_eq!(quota.actors[&[122; 64]].consumed, receipt.consumed_current);
        assert_eq!(quota.network.reserved, QuotaQuantitiesV11::default());
        let collected = quota.root();
        assert!(quota.collect_closed_expired(&policy, &next, [122; 64], &terminal).is_err());
        assert!(quota.admit_baseline(&policy, &next, binding.producer, &admission, &prepared.quota_plan).is_err());
        assert_eq!(quota.root(), collected);
        // The immutable old nonce index defeats a proposed live replacement,
        // even though resource GC removed the old quota catalogue row.
        let reused = prepared_native_job(&class, &policy, &next, 115);
        assert_eq!(reused.job.id(), prepared.job.id());
        assert_ne!(reused.job.root(), prepared.job.root());
        let reused_book = ExecutionJobStateV11::from_native_funded_job(reused.job).unwrap();
        let mut old_index = native_admission(&reused_book);
        old_index.funded_job_root = prepared.job.root();
        old_index.terminal_book_root = Some(book.root());
        assert!(check_native_quota_admission_v11(&class, &reused_book, &old_index).is_err());
        quota.admit_baseline(&policy, &next, fresh.job.producer, &fresh_admission, &fresh.quota_plan).unwrap();
        assert!(quota.subject(&fresh.job.id()).is_some());
        assert!(quota.subject(&binding.job_id).is_none());
        assert_eq!(quota.subjects.len(), 1);
        quota.check_restored(&policy).unwrap();
        assert!(book.cancel_unclaimed(class.record(), next.block_daa).is_err());
    }
    #[test]
    fn terminal_anchor_and_restored_class_cannot_authorize_new_baseline_or_early_collection() {
        use crate::{
            flat_execution11::{ExecutionJobStateV11, ExecutionPolicyV11, NativeSourceBindingV11, restore_execution_class_v11},
            ledger::flat_candidate9::FlatCandidateRecordV9,
        };
        let (class, _, _, policy) = checked_class_fixture();
        let r = class.record();
        let view = class.view();
        let candidate = FlatCandidateRecordV9 {
            binding: r.binding_id,
            plan: view.plan.clone(),
            program_bytes: view.program.encode(),
            param_commitments: class.parameters().clone(),
            scope_bytes: borsh::to_vec(&view.scope).unwrap(),
            replay_bytes: borsh::to_vec(&view.replay).unwrap(),
        };
        let source = NativeSourceBindingV11 {
            descriptor: r.descriptor,
            program_root: r.program_root,
            parameter_root: r.parameter_root,
            inventory_root: r.inventory_root,
            bound_daa: 1,
            matures_daa: 2,
            refuted: true,
        };
        let execution = ExecutionPolicyV11 {
            version: 11,
            enabled: true,
            network_domain: policy.network_domain,
            quota_policy_root: policy.root(),
            horizons: r.horizons.clone(),
            reward_cap: r.reward_cap,
            external_gain_cap: 0,
            default_obligation: 0,
            required_producer_reserve: r.required_producer_reserve,
            finder_permille: r.finder_permille,
        };
        let restored = restore_execution_class_v11(&candidate, &source, r, &execution, &policy, 1100).unwrap();
        assert!(!restored.can_admit());
        let epoch = TrustedEpochV11::from_chain(&policy, [111; 64], 10).unwrap();
        let prepared = prepared_native_job(&class, &policy, &epoch, 115);
        let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
        assert!(check_native_quota_admission_v11(&restored, &book, &native_admission(&book)).is_err());
        let admission = check_native_quota_admission_v11(&class, &book, &native_admission(&book)).unwrap();
        let mut quota = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        quota.admit_baseline(&policy, &epoch, prepared.job.producer, &admission, &prepared.quota_plan).unwrap();
        // A native terminal token alone cannot close a quota row: the genuine
        // prepaid terminal transition must have marked it closed first.
        book.cancel_unclaimed(r, prepared.job.deadline_daa + 1).unwrap();
        let checked_terminal = check_native_quota_terminal_v11(&restored, &book, &native_terminal(&book)).unwrap();
        for field in 0..6 {
            let mut wrong = native_terminal(&book);
            match field {
                0 => wrong.job_id[0] ^= 1,
                1 => wrong.funded_job_root[0] ^= 1,
                2 => wrong.terminal_book_root[0] ^= 1,
                3 => wrong.request_key[0] ^= 1,
                4 => wrong.nonce_witness.requester[0] ^= 1,
                _ => wrong.nonce_witness.counter_root[0] ^= 1,
            }
            assert!(check_native_quota_terminal_v11(&restored, &book, &wrong).is_err(), "terminal field {field}");
        }
        let next = TrustedEpochV11::from_chain(&policy, [123; 64], 1100).unwrap();
        let before = quota.root();
        assert!(quota.collect_closed_expired(&policy, &next, [122; 64], &checked_terminal).is_err());
        assert_eq!(quota.root(), before);
        let mut wrong_policy = policy.clone();
        wrong_policy.max_subjects += 1;
        assert!(quota.collect_closed_expired(&wrong_policy, &next, [122; 64], &checked_terminal).is_err());
        assert_eq!(quota.root(), before);
    }
    #[test]
    fn expired_terminal_current_debits_survive_failures_and_ordinary_floods_without_old_credit() {
        let (plan, mut policy, _, mut binding) = fixture();
        let close = action_envelope(16 << 10, MAX_FIXED_ACTION_BYTES_V11, None).unwrap();
        policy.block = close.clone();
        policy.actor = close.clone();
        policy.network = close.clone();
        binding.policy_root = policy.root();
        let before_deadline = TrustedEpochV11::from_chain(&policy, [101; 64], 99).unwrap();
        let mut state = QuotaStateV11::new(&policy, before_deadline.clone()).unwrap();
        let before = state.root();
        assert!(state.debit_expired_terminal(&policy, &before_deadline, [102; 64], &binding, &plan).is_err());
        assert_eq!(state.root(), before);
        let after_jump = TrustedEpochV11::from_chain(&policy, [103; 64], 150).unwrap();
        let paid = state.debit_expired_terminal(&policy, &after_jump, [102; 64], &binding, &plan).unwrap();
        assert_eq!(paid.actor(), [102; 64]);
        assert_eq!(paid.receipt().consumed, close);
        assert_eq!(paid.receipt().added_guarantee, QuotaQuantitiesV11::default());
        assert_eq!(state.network.consumed, close);
        assert_eq!(state.network.reserved, QuotaQuantitiesV11::default());
        assert!(state.subject(&binding.job_id).is_none());
        // No terminal callback succeeds: the ordinary attempt is still spent.
        let spent = state.root();
        assert!(state.debit_expired_terminal(&policy, &after_jump, [104; 64], &binding, &plan).is_err());
        assert_eq!(state.root(), spent);
        assert_eq!(state.network.consumed, close);
        state.check_restored(&policy).unwrap();
        let next_interval = TrustedEpochV11::from_chain(&policy, [105; 64], 250).unwrap();
        state.debit_expired_terminal(&policy, &next_interval, [104; 64], &binding, &plan).unwrap();
        assert_eq!(state.network.consumed, close);
        assert_eq!(state.network.reserved, QuotaQuantitiesV11::default());
        assert!(state.subject(&binding.job_id).is_none());
    }
    #[test]
    fn late_native_terminal_mark_requires_exact_prepaid_book_and_never_refunds_old_guarantee() {
        use crate::flat_execution11::{
            ExecutionJobInputV11, ExecutionJobStateV11, NativeCloseSnapshotV11, NativeProtectedTraceV11, prepare_execution_job_v11,
        };
        let (class, _, _, policy) = checked_class_fixture();
        let original = TrustedEpochV11::from_chain(&policy, [111; 64], 10).unwrap();
        let input = ExecutionJobInputV11 {
            version: 11,
            prompt_root: [112; 64],
            prompt_len: 1,
            generated_count: 1,
            lengths: crate::flat_lengths7::FlatJobLengthsV7 { token: 1, patch: 0, audio: 0 },
            positions: 1,
        };
        let prepared = prepare_execution_job_v11(
            &class,
            &policy,
            &original,
            input,
            [113; 64],
            [114; 64],
            crate::flat_execution11::nonce_bytes_v11(115),
            10,
            10,
            [116; 64],
        )
        .unwrap();
        let binding = prepared.job.quota_subject(class.record()).unwrap();
        let mut state = QuotaStateV11::new(&policy, original.clone()).unwrap();
        admit(&mut state, &prepared.quota_plan, &policy, &original, &binding);
        let original_summary = state.subject(&binding.job_id).unwrap().clone();
        let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
        let r = class.record();
        let trace = NativeProtectedTraceV11 {
            vector: [117; 64],
            candidate9_id: r.candidate9_id,
            binding_id: r.binding_id,
            class_scope9_root: r.class_scope9_root,
            replay9_root: r.replay9_root,
            class_resource10_root: r.resource10_root,
            prompt_root: prepared.job.input.prompt_root,
            prompt_len: 1,
            positions: 1,
            generated_count: 1,
            trace_root: [118; 64],
            output_root: [119; 64],
            final_state_root: [120; 64],
        };
        book.commit(r, binding.producer, &trace, 10, r.required_producer_reserve, [121; 64]).unwrap();
        let after_jump = TrustedEpochV11::from_chain(&policy, [122; 64], 1100).unwrap();
        let failed = state.debit_expired_terminal(&policy, &after_jump, [123; 64], &binding, &prepared.quota_plan).unwrap();
        let retention = book.claim().unwrap().retention_until;
        let mut close = NativeCloseSnapshotV11 {
            current_daa: 1100,
            pending_own_proofs: 0,
            open_da: 1,
            latest_answer_proof_until: retention,
            required_retained_until: retention,
        };
        assert!(book.finalize(r, &close).is_err());
        assert_eq!(state.subject(&binding.job_id).unwrap(), &original_summary);
        let failed_spent = state.network.consumed.clone();
        let paid = state.debit_expired_terminal(&policy, &after_jump, [123; 64], &binding, &prepared.quota_plan).unwrap();
        assert_eq!(state.network.consumed, failed_spent.checked_mul(2).unwrap());
        close.open_da = 0;
        let settlement = book.finalize(r, &close).unwrap();
        assert_eq!(settlement.producer_credit, 10);
        assert_eq!(settlement.requester_refund, 0);
        let before_mark = state.root();
        assert!(state.mark_expired_terminal_closed(&policy, &after_jump, &binding, &class, &book, &failed).is_err());
        assert_eq!(state.root(), before_mark);
        let mut wrong_binding = binding.clone();
        wrong_binding.job_input_root[0] ^= 1;
        assert!(state.mark_expired_terminal_closed(&policy, &after_jump, &wrong_binding, &class, &book, &paid).is_err());
        assert_eq!(state.root(), before_mark);
        state.mark_expired_terminal_closed(&policy, &after_jump, &binding, &class, &book, &paid).unwrap();
        let subject = state.subject(&binding.job_id).unwrap();
        assert!(subject.closed);
        assert_eq!(subject.guarantee_remaining, original_summary.guarantee_remaining);
        assert_eq!(subject.consumed, original_summary.consumed);
        assert_eq!(subject.committed, original_summary.committed);
        assert_eq!(state.network.reserved, QuotaQuantitiesV11::default());
        state.check_restored(&policy).unwrap();
        let final_root = state.root();
        assert!(state.mark_expired_terminal_closed(&policy, &after_jump, &binding, &class, &book, &paid).is_err());
        assert!(state.debit_expired_terminal(&policy, &after_jump, [123; 64], &binding, &prepared.quota_plan).is_err());
        assert!(book.finalize(r, &close).is_err());
        assert_eq!(state.root(), final_root);
        // Unclaimed original escrow refunds after the same jump without ever
        // creating a full-model quota baseline or another subject catalogue row.
        let mut unclaimed_job = prepared.job.clone();
        unclaimed_job.nonce = crate::flat_execution11::nonce_bytes_v11(124);
        let unclaimed_binding = unclaimed_job.quota_subject(r).unwrap();
        let mut unclaimed = ExecutionJobStateV11::from_native_funded_job(unclaimed_job).unwrap();
        let paid = state.debit_expired_terminal(&policy, &after_jump, [123; 64], &unclaimed_binding, &prepared.quota_plan).unwrap();
        let refund = unclaimed.cancel_unclaimed(r, after_jump.block_daa).unwrap();
        assert_eq!(refund.requester_refund, 10);
        assert_eq!(refund.producer_credit, 0);
        state.mark_expired_terminal_closed(&policy, &after_jump, &unclaimed_binding, &class, &unclaimed, &paid).unwrap();
        assert!(state.subject(&unclaimed_binding.job_id).is_none());
        assert!(unclaimed.cancel_unclaimed(r, after_jump.block_daa).is_err());
    }
    #[test]
    fn every_axis_and_all_four_cap_scopes_are_atomic_at_baseline() {
        let (plan, policy, _, binding) = fixture();
        let need = policy.plan.clone();
        for axis in ResourceAxisV10::ALL {
            assert!(need.get(axis) > 0, "fixture must exercise {axis:?}");
            for scope in 0..4 {
                let mut p = policy.clone();
                let cap = match scope {
                    0 => &mut p.plan,
                    1 => &mut p.block,
                    2 => &mut p.actor,
                    _ => &mut p.network,
                };
                cap.set(axis, need.get(axis) - 1);
                let epoch = TrustedEpochV11::from_chain(&p, [52; 64], 10).unwrap();
                let mut b = binding.clone();
                b.policy_root = p.root();
                let mut state = QuotaStateV11::new(&p, epoch.clone()).unwrap();
                let before = state.clone();
                assert!(state.admit_baseline(&p, &epoch, b.producer, &synthetic_admission(&b), &plan).is_err(), "{axis:?}/{scope}");
                assert_eq!(state, before);
            }
        }
    }
    #[test]
    fn ordinary_admission_flood_cannot_take_mature_court_da_chunk_or_close_guarantee() {
        let (plan, mut policy, _, mut binding) = fixture();
        policy.block = policy.plan.clone();
        policy.actor = policy.plan.clone();
        policy.network = policy.plan.clone();
        binding.policy_root = policy.root();
        let epoch = TrustedEpochV11::from_chain(&policy, [52; 64], 10).unwrap();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let total = state.network.total().unwrap();
        let (program, params, _) = primitive_fixture(11);
        let ordinary = derive_ordinary_metadata_v11(&program, &ParamCommitmentsV1::of_v3(&params)).unwrap();
        for actor in [[57; 64], [60; 64], [61; 64]] {
            let before = state.clone();
            assert!(state.debit_ordinary(&policy, &epoch, actor, &ordinary).is_err());
            assert_eq!(state, before);
        }
        for kind in [QuotaActionKindV11::DARequest, QuotaActionKindV11::DAAnswer, QuotaActionKindV11::Court, QuotaActionKindV11::Chunk]
        {
            state.debit(&policy, &epoch, binding.producer, &binding, &plan, &future(&plan, kind)).unwrap();
            assert_eq!(state.network.total().unwrap(), total);
            assert_eq!(state.block.total().unwrap(), total);
            assert_eq!(state.actor(&binding.producer).unwrap().total().unwrap(), total);
            state.check_restored(&policy).unwrap();
        }
        let paid = state.debit_terminal(&policy, &epoch, binding.producer, &binding, &plan, &[0; 64]).unwrap();
        assert_eq!(state.network.total().unwrap(), total);
        assert!(!state.subject(&binding.job_id).unwrap().closed);
        // Pure counter fixture supplies a test-only private terminal token;
        // production factory requires the genuine checked book/native index.
        let terminal = CheckedQuotaTerminalV11 {
            binding: binding.clone(),
            funded_job_root: [61; 64],
            terminal_book_root: [62; 64],
            request_key: [63; 64],
        };
        state.mark_terminal_closed(&policy, &epoch, &terminal, &paid).unwrap();
        let closed = state.subject(&binding.job_id).unwrap();
        assert!(closed.closed);
        assert_ne!(closed.guarantee_remaining, QuotaQuantitiesV11::default());
        let before = state.clone();
        assert!(state.debit(&policy, &epoch, binding.producer, &binding, &plan, &future(&plan, QuotaActionKindV11::Court)).is_err());
        assert_eq!(state, before);
    }
    #[test]
    fn trusted_equal_daa_blocks_keep_interval_and_lifetime_credit_and_rollback_replays_exactly() {
        let (plan, policy, epoch, binding) = fixture();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let ancestor = state.clone();
        let equal_daa = TrustedEpochV11::from_chain(&policy, [62; 64], epoch.block_daa).unwrap();
        let court = future(&plan, QuotaActionKindV11::Court);
        let receipt = state.debit(&policy, &equal_daa, binding.producer, &binding, &plan, &court).unwrap();
        assert_eq!(state.network.total().unwrap(), ancestor.network.total().unwrap());
        assert_eq!(state.subject(&binding.job_id).unwrap().committed, ancestor.subject(&binding.job_id).unwrap().committed);
        assert_eq!(state.block.consumed, court.quantities);
        state.check_restored(&policy).unwrap();
        let descendant = state.clone();
        // Native journal rollback restores the complete ancestor state, never
        // individual quota credits. Replaying the same block reproduces roots.
        state = ancestor.clone();
        assert_eq!(state.debit(&policy, &equal_daa, binding.producer, &binding, &plan, &court).unwrap(), receipt);
        assert_eq!(state, descendant);
        for invalid in [
            TrustedEpochV11::from_chain(&policy, [63; 64], 9).unwrap(),
            TrustedEpochV11::from_chain(&policy, equal_daa.block_hash, 11).unwrap(),
        ] {
            let before = state.clone();
            assert!(state.debit(&policy, &invalid, binding.producer, &binding, &plan, &court).is_err());
            assert_eq!(state, before);
        }
    }
    #[test]
    fn interval_crossing_is_refused_and_rollover_never_rebinds_old_subject() {
        let (plan, policy, epoch, binding) = fixture();
        for deadline in [9, 100, u64::MAX] {
            let mut b = binding.clone();
            b.deadline_daa = deadline;
            let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
            let before = state.clone();
            assert!(state.admit_baseline(&policy, &epoch, b.producer, &synthetic_admission(&b), &plan).is_err());
            assert_eq!(state, before);
        }
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let old = state.subject(&binding.job_id).unwrap().clone();
        let next = TrustedEpochV11::from_chain(&policy, [64; 64], 100).unwrap();
        let (program, params, _) = primitive_fixture(11);
        let ordinary = derive_ordinary_metadata_v11(&program, &ParamCommitmentsV1::of_v3(&params)).unwrap();
        state.debit_ordinary(&policy, &next, [65; 64], &ordinary).unwrap();
        assert_eq!(state.subject(&binding.job_id).unwrap(), &old);
        assert_eq!(state.network.reserved, QuotaQuantitiesV11::default());
        assert!(state.actor(&binding.producer).is_none());
        state.check_restored(&policy).unwrap();
        let before = state.clone();
        assert!(state.debit(&policy, &next, binding.producer, &binding, &plan, &future(&plan, QuotaActionKindV11::Court)).is_err());
        let mut rebound = binding.clone();
        rebound.admitted_interval = 1;
        rebound.deadline_daa = 199;
        assert!(state.admit_baseline(&policy, &next, binding.producer, &synthetic_admission(&rebound), &plan).is_err());
        assert_eq!(state, before);
    }
    #[test]
    fn failed_expensive_attempts_stay_spent_and_oversized_carriers_refuse_before_debit() {
        let (plan, policy, epoch, binding) = fixture();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        for used in 1..=2 {
            // The native body/court can dismiss this garbage after the prepaid
            // transition; there is no rollback/refund API for that failure.
            state.debit(&policy, &epoch, binding.producer, &binding, &plan, &future(&plan, QuotaActionKindV11::Court)).unwrap();
            assert_eq!(state.subject(&binding.job_id).unwrap().future_attempts_used[3], used);
        }
        let before = state.clone();
        assert!(state.debit(&policy, &epoch, binding.producer, &binding, &plan, &future(&plan, QuotaActionKindV11::Court)).is_err());
        let size = plan.action_carrier_cap(QuotaActionKindV11::Chunk).unwrap();
        assert!(plan.action(QuotaActionKindV11::Chunk, &vec![0; size as usize + 1]).is_err());
        assert_eq!(state, before);
        let record = plan.record();
        assert_eq!(
            record.action_carrier_caps[4],
            record.action_envelopes[4].get(ResourceAxisV10::CourtBytes) as u64 + ACTION_OUTER_ALLOWANCE_V11
        );
        assert!(record.action_carrier_caps[4] < (1 << 20) + ACTION_OUTER_ALLOWANCE_V11);
    }
    #[test]
    fn subject_input_class_actor_and_plan_bindings_cannot_change_or_duplicate_baseline() {
        let (plan, policy, epoch, binding) = fixture();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let before = state.clone();
        assert!(state.admit_baseline(&policy, &epoch, binding.producer, &synthetic_admission(&binding), &plan).is_err());
        for field in 0..16 {
            let mut bad = binding.clone();
            match field {
                0 => bad.version -= 1,
                1 => bad.policy_root[0] ^= 1,
                2 => bad.class_id[0] ^= 1,
                3 => bad.class_scope9_root[0] ^= 1,
                4 => bad.job_id[0] ^= 1,
                5 => bad.job_input_root[0] ^= 1,
                6 => bad.resource10_root[0] ^= 1,
                7 => bad.quota_plan_root[0] ^= 1,
                8 => bad.actual_positions += 1,
                9 => bad.requester[0] ^= 1,
                10 => bad.producer[0] ^= 1,
                11 => bad.admitted_interval += 1,
                12 => bad.deadline_daa -= 1,
                13 => bad.deadline_daa = 100,
                14 => bad.program_root[0] ^= 1,
                _ => bad.parameter_root[0] ^= 1,
            }
            assert!(
                state.debit(&policy, &epoch, binding.producer, &bad, &plan, &future(&plan, QuotaActionKindV11::Court)).is_err(),
                "field {field}"
            );
            assert_eq!(state, before);
        }
    }
    #[test]
    fn duplicate_jobs_and_multiple_actors_share_caps_and_bounded_retained_counts() {
        let (plan, mut policy, _, mut binding) = fixture();
        policy.network = policy.plan.checked_mul(2).unwrap();
        binding.policy_root = policy.root();
        let epoch = TrustedEpochV11::from_chain(&policy, [52; 64], 10).unwrap();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let mut other = binding.clone();
        other.job_id = [66; 64];
        other.producer = [67; 64];
        admit(&mut state, &plan, &policy, &epoch, &other);
        assert_eq!(state.network.total().unwrap(), policy.network);
        let mut third = binding.clone();
        third.job_id = [68; 64];
        let before = state.clone();
        assert!(state.admit_baseline(&policy, &epoch, binding.producer, &synthetic_admission(&third), &plan).is_err());
        assert_eq!(state, before);
        for limit in 0..2 {
            let mut p = policy.clone();
            if limit == 0 {
                p.max_subjects = 1;
            } else {
                p.max_actors = 1;
            }
            let e = TrustedEpochV11::from_chain(&p, [52; 64], 10).unwrap();
            let mut b = binding.clone();
            b.policy_root = p.root();
            let mut state = QuotaStateV11::new(&p, e.clone()).unwrap();
            admit(&mut state, &plan, &p, &e, &b);
            let before = state.clone();
            let mut second = other.clone();
            second.policy_root = p.root();
            assert!(state.admit_baseline(&p, &e, second.producer, &synthetic_admission(&second), &plan).is_err());
            assert_eq!(state, before);
        }
    }
    #[test]
    fn overflow_fixed_policy_and_restored_guarantee_tampering_refuse_atomically() {
        let (plan, policy, epoch, binding) = fixture();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let encoded = borsh::to_vec(&state).unwrap();
        let restored: QuotaStateV11 = borsh::from_slice(&encoded).unwrap();
        restored.check_restored(&policy).unwrap();
        assert_eq!(restored.root(), state.root());
        for field in 0..4 {
            let mut wrong = state.clone();
            match field {
                0 => wrong.network.reserved.0[0] += 1,
                1 => wrong.block.reserved.0[0] += 1,
                2 => wrong.actors.get_mut(&binding.producer).unwrap().reserved.0[0] += 1,
                _ => wrong.subjects.get_mut(&binding.job_id).unwrap().consumed.0[0] -= 1,
            }
            assert!(wrong.check_restored(&policy).is_err());
        }
        let (program, params, _) = primitive_fixture(11);
        let ordinary = derive_ordinary_metadata_v11(&program, &ParamCommitmentsV1::of_v3(&params)).unwrap();
        state.network.consumed.0[ResourceAxisV10::MetadataWork as usize] = u128::MAX;
        let before = state.clone();
        assert!(state.debit_ordinary(&policy, &epoch, binding.producer, &ordinary).is_err());
        assert_eq!(state, before);
        let mut changed_policy = policy.clone();
        changed_policy.network_domain[0] ^= 1;
        assert!(state.debit_ordinary(&changed_policy, &epoch, binding.producer, &ordinary).is_err());
        assert_eq!(state, before);
        changed_policy = policy.clone();
        changed_policy.enabled = false;
        assert!(QuotaStateV11::new(&changed_policy, epoch.clone()).is_err());
        changed_policy = policy;
        changed_policy.future_attempts[0] = u32::MAX;
        assert!(changed_policy.check().is_err());
    }
    #[test]
    fn bounded_snapshot_codec_refuses_count_floods_truncation_duplicates_and_trailing_bytes() {
        let (plan, policy, epoch, binding) = fixture();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let encoded = borsh::to_vec(&state).unwrap();
        assert_eq!(QuotaStateV11::from_bytes(&policy, &encoded).unwrap(), state);
        let actor_count_offset = QUOTA_STATE_FIXED_BYTES_V11 as usize - 8;
        let subject_count_offset = actor_count_offset + 4 + QUOTA_ACTOR_ROW_BYTES_V11 as usize;
        for offset in [actor_count_offset, subject_count_offset] {
            let mut wrong = encoded.clone();
            wrong[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(QuotaStateV11::from_bytes(&policy, &wrong).is_err());
        }
        for length in [0, actor_count_offset, subject_count_offset, encoded.len() - 1] {
            assert!(QuotaStateV11::from_bytes(&policy, &encoded[..length]).is_err());
        }
        let mut wrong = encoded.clone();
        wrong.push(0);
        assert!(QuotaStateV11::from_bytes(&policy, &wrong).is_err());
        // Two identical actor rows are not a canonical map and cannot hide
        // duplicate resources by BTreeMap's last-write-wins semantics.
        let row_begin = actor_count_offset + 4;
        let row_end = row_begin + QUOTA_ACTOR_ROW_BYTES_V11 as usize;
        let mut duplicate = encoded.clone();
        duplicate[actor_count_offset..actor_count_offset + 4].copy_from_slice(&2u32.to_le_bytes());
        duplicate.splice(row_end..row_end, encoded[row_begin..row_end].iter().copied());
        assert!(QuotaStateV11::from_bytes(&policy, &duplicate).is_err());
    }
    #[test]
    fn public_challenger_failures_spend_only_own_unreserved_pool_preserving_mature_owner_guarantee() {
        let (plan, policy, epoch, binding) = fixture();
        let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
        admit(&mut state, &plan, &policy, &epoch, &binding);
        let guarantee = state.subject(&binding.job_id).unwrap().guarantee_remaining.clone();
        let producer = state.actor(&binding.producer).unwrap().clone();
        let actor = [91; 64];
        let court = future(&plan, QuotaActionKindV11::Court);
        let before = state.clone();
        assert!(state.debit(&policy, &epoch, actor, &binding, &plan, &court).is_err());
        assert_eq!(state, before);
        state.debit_public_action(&policy, &epoch, actor, &binding, &plan, &court).unwrap();
        assert_eq!(state.actor(&actor).unwrap().consumed, court.quantities);
        assert_eq!(state.actor(&actor).unwrap().reserved, QuotaQuantitiesV11::default());
        assert_eq!(state.actor(&binding.producer).unwrap(), &producer);
        assert_eq!(state.subject(&binding.job_id).unwrap().guarantee_remaining, guarantee);
        assert_eq!(state.subject(&binding.job_id).unwrap().future_attempts_used[3], 0);
        let before = state.clone();
        assert!(state.debit_public_action(&policy, &epoch, binding.producer, &binding, &plan, &court).is_err());
        assert_eq!(state, before);
        // Owner guarantee remains usable after the dismissed stranger's debit.
        state.debit(&policy, &epoch, binding.producer, &binding, &plan, &court).unwrap();
        state.check_restored(&policy).unwrap();
    }
}
