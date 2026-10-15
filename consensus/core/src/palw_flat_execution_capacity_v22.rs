//! Shared physical carriage admission for new native contracts.
//!
//! A service declaration is useful only when the actual block builder supplies
//! its dedicated, nonpreemptible slots. `implemented` is deliberately false for
//! the probe profile. Bytes, resource caps and a class signature cannot turn it
//! into a delivery theorem. The producer's original consent must bind this
//! record; the new native lane also enforces per-slot deadline cooldown.
use crate::{
    palw_flat_execution_pursuit_v19::NativePublicPursuitV19,
    palw_native_carriage_v22::{NativeCarriageMassParamsV22, NativeCarrierEnvelopeV22},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_execution11::CheckedNativeExecutionClassV11,
    flat_quota11::{CheckedJobQuotaPlanV11, derive_protected_seal_plan_v11},
    hash::{Digest, object_id},
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::LayoutV3,
};

const DENSITY_SCALE: u64 = 1_000_000_000;
const ANSWER_SLOTS: u32 = 11; // ordinary 3 + original protected owners 4 * 2
const PROOF_SLOTS: u32 = 4;
const ANSWER_ATTEMPTS: u32 = 2;
const FRAGMENT_ATTEMPTS: u32 = 2;
const PART_BYTES: u64 = 32 << 10;
const MAX_LEASES: u32 = 128;

/// Explicit producer signature over complete public pursuit, physical service
/// and slot replenishment. The old availability18 authorization is a distinct
/// immutable object, and cannot manufacture this additional promise.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeExecutionConsentV23 {
    pub version: u16,
    pub preparation: Digest,
    pub original_consent13: Digest,
    pub original_availability18: Digest,
    pub public_pursuit19: Digest,
    pub carriage_plan22: Digest,
    pub physical_service22: Digest,
    pub slot_cooldown_terms: Digest,
}
impl NativeExecutionConsentV23 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-complete-pursuit-consent/v23", self)
    }
    pub fn cooldown_terms() -> Digest {
        object_id(b"misaka-palw/native/original-slot-replenishment/v23",&(23u16,
            b"eleven-shared-answer-slots;two-answer-attempts;slot-refill-not-before-original-deadline;four-retained-proof-slots;two-fragment-attempts;one-original-owned-object-per-complete-carrier;no-ordinary-preemption;physical-service-not-a-speed-or-profit-measurement".to_vec()))
    }
}

/// Eleven fixed physical streams. Ordinary aliases share three streams; each
/// original protected owner receives exactly two. A served packet retains its
/// original cooldown, so early successful delivery cannot create extra rate.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePhysicalSlotsV22 {
    pub version: u16,
    pub job: Digest,
    pub original_consent23: Digest,
    pub owners: [Digest; 4],
    pub owner_horizons: [u64; 4],
    pub not_before: [u64; 11],
    pub units: [Digest; 11],
    pub kinds: [u8; 11],
}
impl NativePhysicalSlotsV22 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-physical-slot-streams/v22", self)
    }
    pub fn empty(job: Digest, original_consent23: Digest) -> Self {
        Self {
            version: 22,
            job,
            original_consent23,
            owners: [[0; 64]; 4],
            owner_horizons: [0; 4],
            not_before: [0; 11],
            units: [[0; 64]; 11],
            kinds: [0; 11],
        }
    }
    pub fn reserve_owner(&mut self, owner: Digest, horizon: u64) -> Result<(), String> {
        if owner == [0; 64] || horizon == 0 || self.owners.contains(&owner) {
            return Err("native physical protected owner duplicate/zero".into());
        }
        let i = self.owners.iter().position(|v| *v == [0; 64]).ok_or("native four original physical owners exhausted")?;
        self.owners[i] = owner;
        self.owner_horizons[i] = horizon;
        Ok(())
    }
    pub fn transfer_owner(&mut self, old: Digest, new: Digest, horizon: u64) -> Result<(), String> {
        let i = self.owners.iter().position(|v| *v == old).ok_or("native physical conversion lost original owner")?;
        if new == [0; 64] || self.owners.contains(&new) || horizon < self.owner_horizons[i] {
            return Err("native physical conversion changes ownership/horizon".into());
        }
        self.owners[i] = new;
        self.owner_horizons[i] = horizon;
        Ok(())
    }
    pub fn reserve_request(
        &mut self,
        lane: u8,
        owner: Digest,
        slot: u8,
        kind: u8,
        unit: Digest,
        now: u64,
        deadline: u64,
    ) -> Result<(), String> {
        if kind > 1 || slot >= 2 || unit == [0; 64] || deadline <= now || self.units.contains(&unit) {
            return Err("native physical request identity differs/duplicate".into());
        }
        let i = match lane {
            0 if owner == [0; 64] => (0..3)
                .position(|i| self.units[i] == [0; 64] || self.not_before[i] < now)
                .ok_or("native ordinary physical streams retain original cooldown")?,
            1 | 2 => {
                let i = self.owners.iter().position(|v| *v == owner).ok_or("native original physical owner was not reserved")?;
                if now > self.owner_horizons[i] {
                    return Err("native original physical owner expired".into());
                }
                i * 2 + 3 + slot as usize
            }
            _ => return Err("native physical request changes original ordinary/paid lane".into()),
        };
        if self.units[i] != [0; 64] && now <= self.not_before[i] {
            return Err("native physical stream cannot refill before original answer deadline".into());
        }
        self.not_before[i] = deadline;
        self.units[i] = unit;
        self.kinds[i] = kind;
        Ok(())
    }
    pub fn retire_owner(&mut self, owner: Digest, now: u64) -> Result<(), String> {
        let i = self.owners.iter().position(|v| *v == owner).ok_or("native physical owner absent/already retired")?;
        if now <= self.owner_horizons[i] || self.not_before[3 + i * 2..5 + i * 2].iter().any(|d| now < *d) {
            return Err("native original physical owner/cooldown retained".into());
        }
        self.owners[i] = [0; 64];
        self.owner_horizons[i] = 0;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarriageServiceV22 {
    pub version: u16,
    pub mass_params: Digest,
    /// Complete packet occupancy, including all transaction and script fields.
    pub slot_mass: u64,
    /// Guaranteed duration between dedicated native slots (DAA units).
    pub period_daa: u64,
    /// Set only by trusted consumer plumbing of the actual reserved scheduler.
    pub implemented: bool,
}
impl NativeCarriageServiceV22 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-physical-service/v22", self)
    }
    pub fn probe(mass: &NativeCarriageMassParamsV22) -> Self {
        Self {
            version: 22,
            mass_params: mass.root(),
            slot_mass: (mass.max_block_mass as u128 * 3 / 4) as u64,
            period_daa: 2,
            implemented: false,
        }
    }
    /// Consumer profile for the native EDF template path and its provisioned
    /// original queue30. Readiness of reference/detection/delivery measurements
    /// remains a separate trusted Final27 condition. This is not an upgrade of
    /// an old consent23: its producer must sign the NEW complete contract.
    pub fn reserved_scheduler_v30(
        mass: &NativeCarriageMassParamsV22,
        queue: &crate::palw_native_queue_v30::NativeQueuePolicyV30,
    ) -> Result<Self, String> {
        queue.check(queue.network_domain)?;
        let mut service = Self::probe(mass);
        service.implemented = true;
        service.check(mass)?;
        Ok(service)
    }
    pub fn check(&self, mass: &NativeCarriageMassParamsV22) -> Result<(), String> {
        if self.version != 22
            || self.mass_params != mass.root()
            || self.slot_mass == 0
            || self.slot_mass > mass.max_block_mass
            || self.period_daa == 0
        {
            return Err("native physical service changes original mass/slot contract".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarriagePlanV22 {
    pub version: u16,
    pub class: Digest,
    pub original_job_plan: Digest,
    pub parameter_root: Digest,
    pub public_pursuit: Digest,
    pub service: Digest,
    pub answer_envelope: NativeCarrierEnvelopeV22,
    pub proof_envelope: NativeCarrierEnvelopeV22,
    pub original_source_answer_bytes: u64,
    pub source_parts: u32,
    pub answer_burst_packets: u32,
    pub proof_burst_packets: u32,
    pub answer_window: u64,
    pub proof_window: u64,
    pub density: u64,
    pub minimum_deadline: u64,
    pub requires_source_parts: bool,
}
impl NativeCarriagePlanV22 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-shared-carriage-plan/v22", self)
    }
    fn check(&self, service: &NativeCarriageServiceV22, mass: &NativeCarriageMassParamsV22) -> Result<(), String> {
        service.check(mass)?;
        if self.version != 22
            || self.service != service.root()
            || self.source_parts == 0
            || self.source_parts > 4
            || self.answer_burst_packets
                != ANSWER_SLOTS * ANSWER_ATTEMPTS * self.source_parts * if self.requires_source_parts { FRAGMENT_ATTEMPTS } else { 1 }
            || self.proof_burst_packets == 0
            || self.proof_burst_packets > PROOF_SLOTS * 64 * FRAGMENT_ATTEMPTS
            || self.minimum_deadline != self.answer_window.min(self.proof_window)
            || self.minimum_deadline <= service.period_daa
            || self.density
                != density(self.answer_burst_packets, self.answer_window, service.period_daa)?
                    .checked_add(density(self.proof_burst_packets, self.proof_window, service.period_daa)?)
                    .ok_or("native shared carriage density overflow")?
            || mass.packet_mass(&self.answer_envelope)? > service.slot_mass
            || mass.packet_mass(&self.proof_envelope)? > service.slot_mass
        {
            return Err("native carriage original burst/window/envelope conservation differs".into());
        }
        Ok(())
    }
}
fn density(packets: u32, deadline: u64, period: u64) -> Result<u64, String> {
    if deadline == 0 {
        return Err("native carriage zero deadline".into());
    }
    let n = (packets as u128)
        .checked_mul(period as u128)
        .and_then(|n| n.checked_mul(DENSITY_SCALE as u128))
        .ok_or("native carriage utilization overflow")?;
    u64::try_from(n.div_ceil(deadline as u128)).map_err(|_| "native carriage utilization exceeds fixed bound".into())
}
fn envelope(body: u64) -> Result<NativeCarrierEnvelopeV22, String> {
    Ok(NativeCarrierEnvelopeV22 {
        // This new physical lane carries exactly one original native object.
        // Its bounded MLDSA87 object framing fits 8 KiB; the transaction
        // input signature is separately retained below. Resource11 keeps
        // its frozen, more conservative 16-KiB accounting unchanged.
        max_payload_bytes: body.checked_add(8 << 10).ok_or("native carriage wrapper extent overflow")?,
        max_inputs: 1,
        max_outputs: 1,
        max_signature_script_bytes: 8192,
        max_output_script_bytes: 69,
        max_sig_ops: 1,
        max_storage_mass: 100_000,
    })
}
fn log(n: u64) -> u64 {
    if n <= 1 { 0 } else { (64 - (n - 1).leading_zeros()) as u64 }
}
pub fn derive_native_carriage_plan_v22(
    class: &CheckedNativeExecutionClassV11,
    job: &CheckedJobQuotaPlanV11,
    pursuit: &NativePublicPursuitV19,
    service: &NativeCarriageServiceV22,
    mass: &NativeCarriageMassParamsV22,
) -> Result<NativeCarriagePlanV22, String> {
    service.check(mass)?;
    let c = class.record();
    if pursuit.version != 19
        || pursuit.class != c.id()
        || pursuit.original_job_plan != job.record().root()
        || job.record().parameter_root != c.parameter_root
    {
        return Err("native carriage changes original independently derived pursuit/job".into());
    }
    let mut source_bytes = 1 + 162; // canonical AnswerV16::Header
    for param in &class.view().program.params {
        let shape: Vec<_> = param.shape.iter().map(|d| *d as usize).collect();
        let l = LayoutV3::try_of(&shape).ok_or("native carriage original PC layout overflow")?;
        for axis in [AXIS_ROW, AXIS_COL] {
            let leaves = l.leaves(axis);
            if leaves == 0 {
                return Err("native carriage empty original PC tree".into());
            }
            let values = l.leaf_element_count(axis, 0, 0).ok_or("native carriage original PC leaf absent")?;
            // enum + dtype + Vec(shape) + axis + line/tile + Vec(values) +
            // Vec(original siblings) + opposite original root. No subtiles.
            let bytes: u64 = 1 + 1 + 4 + shape.len() as u64 * 8 + 1 + 8 + 8 + 4 + 4 + 64;
            let bytes = bytes
                .checked_add(values.checked_mul(param.dtype.width() as u64).ok_or("native PC width overflow")?)
                .and_then(|n| n.checked_add(log(leaves).checked_mul(64)?))
                .ok_or("native PC opening extent overflow")?;
            source_bytes = source_bytes.max(bytes);
        }
    }
    if source_bytes > misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16 {
        return Err("native original PC opening exceeds paid source answer cap".into());
    }
    let direct = envelope(source_bytes.max(16 << 10).checked_add(68 + 7).ok_or("native answer framing overflow")?)?;
    let fits_direct = mass.packet_mass(&direct).is_ok_and(|n| n <= service.slot_mass);
    let source_parts =
        if fits_direct { 1 } else { u32::try_from(source_bytes.div_ceil(PART_BYTES)).map_err(|_| "native PC parts count")? };
    let answer_envelope = if fits_direct { direct } else { envelope(PART_BYTES + 512)? };
    let protected = derive_protected_seal_plan_v11(job)?;
    let proof_burst_packets = PROOF_SLOTS
        .checked_mul(protected.fragment_count_cap)
        .and_then(|n| n.checked_mul(protected.fragment_attempts_per_part))
        .ok_or("native proof carriage extent overflow")?;
    let answer_burst_packets = ANSWER_SLOTS
        .checked_mul(ANSWER_ATTEMPTS)
        .and_then(|n| n.checked_mul(source_parts))
        .and_then(|n| n.checked_mul(if fits_direct { 1 } else { FRAGMENT_ATTEMPTS }))
        .ok_or("native answer carriage extent overflow")?;
    let answer_window = c.horizons.da_answer;
    let proof_window =
        c.horizons.seal_ttl.checked_sub(c.horizons.seal_maturity).ok_or("native proof maturity exhausts carriage window")?;
    let plan = NativeCarriagePlanV22 {
        version: 22,
        class: c.id(),
        original_job_plan: job.record().root(),
        parameter_root: c.parameter_root,
        public_pursuit: pursuit.root(),
        service: service.root(),
        answer_envelope,
        proof_envelope: envelope(PART_BYTES + 512)?,
        original_source_answer_bytes: source_bytes,
        source_parts,
        answer_burst_packets,
        proof_burst_packets,
        answer_window,
        proof_window,
        density: density(answer_burst_packets, answer_window, service.period_daa)?
            .checked_add(density(proof_burst_packets, proof_window, service.period_daa)?)
            .ok_or("native carriage total density overflow")?,
        minimum_deadline: answer_window.min(proof_window),
        requires_source_parts: !fits_direct,
    };
    plan.check(service, mass)?;
    Ok(plan)
}

/// Fixed shared row, independent of the resource21 and monetary pools. Removing
/// one lease never raises the conservative minimum deadline while others live;
/// no hidden scan is needed and no previous scheduling credit is resurrected.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarriageGlobalV22 {
    pub version: u16,
    pub service: Digest,
    pub live: u32,
    pub density: u64,
    pub minimum_deadline: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeCarriageLeaseV22 {
    pub version: u16,
    pub original_job: Digest,
    pub original_consent: Digest,
    pub plan: Digest,
    pub density: u64,
    pub minimum_deadline: u64,
    pub retired: bool,
}
impl NativeCarriageLeaseV22 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-shared-carriage-lease/v22", self)
    }
}
impl NativeCarriageGlobalV22 {
    pub fn empty(service: &NativeCarriageServiceV22) -> Self {
        Self { version: 22, service: service.root(), live: 0, density: 0, minimum_deadline: u64::MAX }
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-shared-carriage-global/v22", self)
    }
    pub fn admit(
        &mut self,
        plan: &NativeCarriagePlanV22,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
        original_job: Digest,
        original_consent: Digest,
    ) -> Result<NativeCarriageLeaseV22, String> {
        plan.check(service, mass)?;
        if self.version != 22
            || self.service != service.root()
            || self.live >= MAX_LEASES
            || original_job == [0; 64]
            || original_consent == [0; 64]
            || (self.live == 0 && (self.density != 0 || self.minimum_deadline != u64::MAX))
        {
            return Err("native shared physical lease row/identity/catalog differs".into());
        }
        let next_density = self.density.checked_add(plan.density).ok_or("native live carriage density overflow")?;
        let deadline = self.minimum_deadline.min(plan.minimum_deadline);
        // For arbitrary synchronous bursts, dbf(t) <= utilization*t for t at
        // least the smallest deadline. One in-flight nonpreemptive packet costs
        // at most one service period. Upward rounding makes the check stricter.
        let blocking = density(1, deadline, service.period_daa)?;
        if next_density.checked_add(blocking).is_none_or(|n| n > DENSITY_SCALE) {
            return Err("native simultaneous answer/proof obligations exceed guaranteed physical slots".into());
        }
        self.live += 1;
        self.density = next_density;
        self.minimum_deadline = deadline;
        Ok(NativeCarriageLeaseV22 {
            version: 22,
            original_job,
            original_consent,
            plan: plan.root(),
            density: plan.density,
            minimum_deadline: plan.minimum_deadline,
            retired: false,
        })
    }
    /// Caller must authenticate the original native terminal and absence of
    /// every pending answer/seal before invoking this conservation operation.
    pub fn retire(&mut self, lease: &mut NativeCarriageLeaseV22, expected_root: Digest) -> Result<(), String> {
        if lease.version != 22
            || lease.retired
            || lease.root() != expected_root
            || self.live == 0
            || self.density < lease.density
            || self.minimum_deadline > lease.minimum_deadline
        {
            return Err("native original carriage lease retirement differs/already spent".into());
        }
        self.live -= 1;
        self.density -= lease.density;
        lease.retired = true;
        if self.live == 0 {
            self.density = 0;
            self.minimum_deadline = u64::MAX;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativePaidCarrierRequestV22 {
    /// Owning39 uses its own 2048-part/local-court and original preclaim lanes.
    ProofPartV39 {
        job: Digest,
        seal: Digest,
        index: u16,
        count: u16,
        total: u32,
        mode: u8,
    },
    AnswerPartV39 {
        pending: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
    },
    CommitPartV39 {
        job: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
    },
    CommitAssembleV39 {
        job: Digest,
        group: Digest,
    },

    /// Original32 producer phase, authorized only before its actual Commit.
    FundedCommitPartV32 {
        job: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
    },
    FundedCommitAssembleV32 {
        job: Digest,
        group: Digest,
    },
    /// New full source/trace reply pages; not an old singleton answer.
    ProducerAnswerPartV29 {
        pending: Digest,
        group: Digest,
        index: u16,
        count: u16,
        total: u32,
    },
    SourceAnswer {
        pending: Digest,
    },
    TraceAnswer {
        pending: Digest,
    },
    OwnProofPart {
        seal: Digest,
        index: u8,
        count: u8,
    },
    /// Nonwire scheduling request for the independent paid 29 transport.
    /// Does not reinterpret a v11 seal or outer113's byte-sized counts.
    OwnProofPartV29 {
        job: Digest,
        seal: Digest,
        owner: Digest,
        index: u16,
        count: u16,
    },
}
/// Read-only scheduling evidence, not an execution, court or monetary grant.
/// The caller additionally verifies the COMPLETE populated transaction against
/// `envelope` and exactly one signed object matching `owner`/`unit`/`kind`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePaidCarrierEligibilityV22 {
    pub original_job: Digest,
    pub unit: Digest,
    pub owner: crate::palw_state_v2::PalwBondKeyV2,
    pub deadline: u64,
    pub kind: u8,
    pub lease_root: Digest,
    pub original_service: Digest,
    pub envelope: NativeCarrierEnvelopeV22,
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    /// Only bounded native rows are read: no program, class, full post, proof,
    /// parameter payload or assembled fragment is restored in the mempool.
    pub fn native_paid_carrier_eligibility_v22(
        &self,
        request: &NativePaidCarrierRequestV22,
        now: u64,
        mass: &NativeCarriageMassParamsV22,
        service: &NativeCarriageServiceV22,
    ) -> Option<NativePaidCarrierEligibilityV22> {
        use crate::palw_flat_execution_v11::*;
        use crate::palw_kernel_route_v1::palw_kernel_bond_id_v1;
        use misaka_palw_kernel::flat_execution11::ExecutionTerminalV11;
        use misaka_palw_kernel::flat_quota11::*;
        if !service.implemented || service.check(mass).is_err() {
            return None;
        }
        match request {
            NativePaidCarrierRequestV22::ProofPartV39 { job, seal, index, count, total, mode } => {
                return self.native_paid_proof_eligibility_v39(*job, *seal, *index, *count, *total, *mode, now, mass, service);
            }
            NativePaidCarrierRequestV22::AnswerPartV39 { pending, group, index, count, total } => {
                return self.native_paid_answer_eligibility_v39(*pending, *group, *index, *count, *total, now, mass, service);
            }
            _ => {}
        }
        if let NativePaidCarrierRequestV22::ProducerAnswerPartV29 { pending, group, index, count, total } = request {
            return self.native_paid_text_reply_eligibility_v29(*pending, *index, *count, *total, *group, now, mass, service);
        }
        if let NativePaidCarrierRequestV22::OwnProofPartV29 { job, seal, owner, index, count } = request {
            return self.native_paid_paged_carrier_eligibility_v29(*job, *seal, *owner, *index, *count, now, mass, service);
        }
        let bounded = |table, key: Vec<u8>, cap: usize| self.aux.get(&(table, key)).filter(|raw| raw.len() <= cap);
        let (job, unit, owner, deadline, kind) = match request {
            NativePaidCarrierRequestV22::SourceAnswer { pending } => {
                let d = self.execution_source_demand_v16(pending)?;
                if d.settled || d.attempts >= 2 || now > d.deadline {
                    return None;
                }
                (d.request.job, *pending, d.producer, d.deadline, 0u8)
            }
            NativePaidCarrierRequestV22::TraceAnswer { pending } => {
                let d = self.execution_demand_v11(&crate::Hash64::from_bytes(*pending))?;
                if d.body.settled || now > d.body.deadline_daa {
                    return None;
                }
                // New23 explicitly caps two attempts PER original pending unit;
                // an absent counter grants no reserved physical priority.
                let used: u8 =
                    borsh::from_slice(bounded(EXECUTION_DEMAND_TABLE_V11, borsh::to_vec(&(22u8, *pending)).ok()?, 1)?).ok()?;
                if used >= 2 {
                    return None;
                }
                (d.job_id, *pending, d.body.poster, d.body.deadline_daa, 1u8)
            }
            NativePaidCarrierRequestV22::OwnProofPart { seal, index, count } => {
                let s = self.execution_seal_v11(seal)?;
                if s.spent || now < s.mature_daa || now > s.reveal_until || *count == 0 || *index >= *count {
                    return None;
                }
                let mut qkey = vec![3u8];
                qkey.extend_from_slice(seal);
                let (root, q): (Digest, ProtectedSealSummaryV11) =
                    borsh::from_slice(bounded(EXECUTION_QUOTA_TABLE_V11, qkey, 4096)?).ok()?;
                if q.row_root() != root
                    || q.key.root() != *seal
                    || q.original_paid_seal_root != s.original_paid_record_root
                    || *count as u32 > q.fragment_count_cap
                    || q.fragments.attempts_by_index[*index as usize] >= 2
                    || q.fragments.accepted_bitmap & (1u64 << *index) != 0
                    || q.fragments.declared_count != 0 && q.fragments.declared_count != *count as u32
                {
                    return None;
                }
                (s.job_id, *seal, s.bond, s.reveal_until, 2u8)
            }
            NativePaidCarrierRequestV22::ProofPartV39 { .. }
            | NativePaidCarrierRequestV22::AnswerPartV39 { .. }
            | NativePaidCarrierRequestV22::CommitPartV39 { .. }
            | NativePaidCarrierRequestV22::CommitAssembleV39 { .. }
            | NativePaidCarrierRequestV22::OwnProofPartV29 { .. }
            | NativePaidCarrierRequestV22::ProducerAnswerPartV29 { .. }
            | NativePaidCarrierRequestV22::FundedCommitPartV32 { .. }
            | NativePaidCarrierRequestV22::FundedCommitAssembleV32 { .. } => return None,
        };
        let h = self.execution_job_header_v11(&job)?;
        if h.terminal != ExecutionTerminalV11::Challengeable || kind != 2 && h.producer != owner {
            return None;
        }
        let (root, lease): (Digest, NativeCarriageLeaseV22) =
            borsh::from_slice(bounded(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(22u8, job)).ok()?, 512)?).ok()?;
        if root != lease.root() || lease.version != 22 || lease.original_job != job || lease.retired {
            return None;
        }
        let plan: NativeCarriagePlanV22 =
            borsh::from_slice(bounded(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(22u8, job, 1u8)).ok()?, 1024)?).ok()?;
        let consent: NativeExecutionConsentV23 =
            borsh::from_slice(bounded(EXECUTION_POLICY_TABLE_V11, borsh::to_vec(&(23u8, job)).ok()?, 512)?).ok()?;
        if plan.root() != lease.plan
            || plan.original_job_plan != h.quota_plan_root
            || plan.class != h.class_id
            || plan.check(service, mass).is_err()
            || plan.requires_source_parts
            || consent.root() != lease.original_consent
            || consent.carriage_plan22 != lease.plan
            || consent.physical_service22 != service.root()
            || consent.slot_cooldown_terms != NativeExecutionConsentV23::cooldown_terms()
        {
            return None;
        }
        let (slotroot, slots): (Digest, NativePhysicalSlotsV22) =
            borsh::from_slice(bounded(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(22u8, job, 2u8)).ok()?, 2048)?).ok()?;
        if slots.version != 22 || slots.job != job || slots.root() != slotroot || slots.original_consent23 != consent.root() {
            return None;
        }
        if kind == 2 {
            if !slots.owners.contains(&unit) {
                return None;
            }
        } else {
            let i = slots.units.iter().position(|id| *id == unit)?;
            if slots.kinds[i] != kind || slots.not_before[i] != deadline {
                return None;
            }
        }
        if kind == 0 {
            let (root, row): (Digest, misaka_palw_kernel::flat_quota11::source16::SourceObligationV16) =
                borsh::from_slice(bounded(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(16u8, job)).ok()?, 2048)?).ok()?;
            if row.root() != root
                || row.retired
                || row.job_id != job
                || row.producer != palw_kernel_bond_id_v1(&owner)
                || row.original_consent_root != consent.original_availability18
                || row.quota_plan_root != h.quota_plan_root
                || row.answers_used >= row.requests_admitted.checked_mul(2)?
            {
                return None;
            }
        } else if kind == 1 {
            let (root, row): (Digest, misaka_palw_kernel::flat_quota11::trace17::TraceSupplementV17) =
                borsh::from_slice(bounded(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(17u8, 0u8, job)).ok()?, 2048)?).ok()?;
            if row.root() != root
                || row.retired
                || row.key != job
                || row.job != job
                || row.original_paid_root != consent.original_availability18
                || row.role != 0
                || row.actor != palw_kernel_bond_id_v1(&owner)
                || row.attempts_used >= row.attempts_cap
            {
                return None;
            }
        }
        Some(NativePaidCarrierEligibilityV22 {
            original_job: job,
            unit,
            owner,
            deadline,
            kind,
            lease_root: root,
            original_service: service.root(),
            envelope: if kind == 2 { plan.proof_envelope } else { plan.answer_envelope },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::TESTNET_PARAMS;

    fn plan(service: &NativeCarriageServiceV22, window: u64, parts: u32) -> NativeCarriagePlanV22 {
        let answers = ANSWER_SLOTS * ANSWER_ATTEMPTS * parts * if parts > 1 { FRAGMENT_ATTEMPTS } else { 1 };
        let proofs = PROOF_SLOTS * 3 * FRAGMENT_ATTEMPTS;
        NativeCarriagePlanV22 {
            version: 22,
            class: [1; 64],
            original_job_plan: [2; 64],
            parameter_root: [3; 64],
            public_pursuit: [4; 64],
            service: service.root(),
            answer_envelope: envelope(PART_BYTES + 512).unwrap(),
            proof_envelope: envelope(PART_BYTES + 512).unwrap(),
            original_source_answer_bytes: parts as u64 * PART_BYTES,
            source_parts: parts,
            answer_burst_packets: answers,
            proof_burst_packets: proofs,
            answer_window: window,
            proof_window: window,
            density: density(answers, window, service.period_daa).unwrap() + density(proofs, window, service.period_daa).unwrap(),
            minimum_deadline: window,
            requires_source_parts: parts > 1,
        }
    }
    #[test]
    fn shared_bursts_retry_and_blocking_mass_refuse_overbook_atomically() {
        let mass = NativeCarriageMassParamsV22::from_consensus_params(&TESTNET_PARAMS);
        let service = NativeCarriageServiceV22::probe(&mass);
        assert!(!service.implemented, "a probe arithmetic result grants no physical builder readiness");
        let p = plan(&service, 1000, 3);
        assert_eq!(p.answer_burst_packets, 11 * 2 * 3 * 2);
        assert_eq!(p.proof_burst_packets, 4 * 3 * 2);
        let mut g = NativeCarriageGlobalV22::empty(&service);
        let mut leases = Vec::new();
        for id in 1..=3u8 {
            leases.push(g.admit(&p, &service, &mass, [id; 64], [9; 64]).unwrap());
        }
        let before = g.clone();
        assert!(g.admit(&p, &service, &mass, [4; 64], [9; 64]).is_err());
        assert_eq!(g, before);
        // Two full-source attempts and two failed fragments are independently
        // counted. Reducing either number cannot manufacture a cheaper lease.
        let mut forged = p.clone();
        forged.answer_burst_packets /= 2;
        assert!(g.admit(&forged, &service, &mass, [5; 64], [9; 64]).is_err());
        let mut forged = p.clone();
        forged.answer_envelope.max_payload_bytes = 128 << 10;
        assert!(g.admit(&forged, &service, &mass, [5; 64], [9; 64]).is_err());
        assert_eq!(g, before);
        let root = leases[0].root();
        g.retire(&mut leases[0], root).unwrap();
        assert!(g.retire(&mut leases[0], root).is_err());
        g.admit(&p, &service, &mass, [4; 64], [9; 64]).unwrap();
    }
    #[test]
    fn smaller_deadline_and_original_slot_cooldown_are_part_of_shared_guard() {
        let mass = NativeCarriageMassParamsV22::from_consensus_params(&TESTNET_PARAMS);
        let service = NativeCarriageServiceV22::probe(&mass);
        let long = plan(&service, 1000, 1);
        let short = plan(&service, 120, 1);
        let mut g = NativeCarriageGlobalV22::empty(&service);
        let mut lease = g.admit(&short, &service, &mass, [1; 64], [9; 64]).unwrap();
        g.admit(&long, &service, &mass, [2; 64], [9; 64]).unwrap();
        let root = lease.root();
        g.retire(&mut lease, root).unwrap();
        assert_eq!(g.minimum_deadline, 120, "retirement does not scan unrelated jobs or invent scheduling credit");
        let before = g.clone();
        let tiny = plan(&service, service.period_daa, 1);
        assert!(g.admit(&tiny, &service, &mass, [3; 64], [9; 64]).is_err());
        assert_eq!(g, before);
        let mut different = service.clone();
        different.period_daa += 1;
        assert!(g.admit(&long, &different, &mass, [3; 64], [9; 64]).is_err());
        assert_ne!(different.root(), service.root());
    }
    #[test]
    fn physical_streams_survive_early_delivery_alias_churn_and_ticket_conversion() {
        let mut s = NativePhysicalSlotsV22::empty([1; 64], [2; 64]);
        for owner in 3..=6u8 {
            s.reserve_owner([owner; 64], 100).unwrap();
        }
        for i in 0..3u8 {
            s.reserve_request(0, [0; 64], 0, 0, [20 + i; 64], 0, 10).unwrap();
        }
        let before = s.clone();
        assert!(s.reserve_request(0, [0; 64], 1, 1, [23; 64], 1, 11).is_err());
        assert_eq!(s, before, "three served ordinary aliases retain original cooldown");
        for owner in 3..=6u8 {
            for slot in 0..2u8 {
                s.reserve_request(2, [owner; 64], slot, slot, [30 + owner * 2 + slot; 64], 0, 10).unwrap();
            }
        }
        let before = s.clone();
        assert!(s.reserve_request(1, [3; 64], 0, 1, [70; 64], 1, 11).is_err());
        assert_eq!(s, before, "one paid owner cannot consume another owner's two streams");
        s.transfer_owner([3; 64], [7; 64], 150).unwrap();
        assert!(s.reserve_request(1, [7; 64], 0, 1, [71; 64], 1, 11).is_err(), "conversion creates no fresh rate");
        assert!(
            s.reserve_request(1, [7; 64], 0, 1, [71; 64], 10, 20).is_err(),
            "original answer is still valid at its inclusive deadline"
        );
        s.reserve_request(1, [7; 64], 0, 1, [71; 64], 11, 21).unwrap();
        assert!(s.retire_owner([7; 64], 150).is_err());
        s.retire_owner([7; 64], 151).unwrap();
        assert!(s.retire_owner([7; 64], 151).is_err());
        s.reserve_owner([8; 64], 250).unwrap();
        s.reserve_request(2, [8; 64], 0, 0, [72; 64], 151, 161).unwrap();
    }
}
