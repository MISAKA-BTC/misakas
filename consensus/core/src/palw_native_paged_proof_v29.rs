//! Dedicated u16-index paid proof transport. Old outer113/targets6-7 and their
//! u8 counts keep their original bytes and limits. This lane owns all parts,
//! assembly and court attempts from the original signed maximum contract.
use crate::{
    palw_flat_execution_capacity_v22::{NativeCarriageLeaseV22, NativeCarriageServiceV22, NativePaidCarrierEligibilityV22},
    palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11,
    palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1},
    palw_native_carriage_v22::{NativeCarriageMassParamsV22, NativeCarrierEnvelopeV22},
    palw_native_text_stop_v29::*,
    palw_state_v2::PalwBondKeyV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_quota11::{QuotaPolicyV11, QuotaQuantitiesV11},
    flat_resources10::ResourceAxisV10 as A,
    flat_resources29::FlatResourceVectorV29,
    flat_scope25::CheckedResourceScopeV25,
    hash::{Digest, object_id},
};

pub const PAGED_PART_HEADER_BYTES_V29: usize = 2 + 3 * 64 + 2 + 2 + 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowedNativePagedPartV29<'a> {
    pub job: Digest,
    pub seal: Digest,
    pub owner: Digest,
    pub index: u16,
    pub count: u16,
    pub bytes: &'a [u8],
}
/// Signature/owner preflight borrows ONLY the fixed original authorization
/// coordinates. A hostile nested byte count is left for the priced native fold.
pub fn borrowed_native_paged_metadata_v29(raw: &[u8]) -> Option<BorrowedNativePagedPartV29<'_>> {
    if raw.len() < 202 || raw.len() > 202 + TEXT_STOP_PART_BYTES_V29 || raw[..2] != 29u16.to_le_bytes() {
        return None;
    }
    let index = u16::from_le_bytes(raw[194..196].try_into().ok()?);
    let count = u16::from_le_bytes(raw[196..198].try_into().ok()?);
    if count == 0 || count > 512 || index >= count {
        return None;
    }
    Some(BorrowedNativePagedPartV29 {
        job: raw[2..66].try_into().ok()?,
        seal: raw[66..130].try_into().ok()?,
        owner: raw[130..194].try_into().ok()?,
        index,
        count,
        bytes: &raw[202..],
    })
}
pub fn borrowed_native_paged_part_v29(raw: &[u8]) -> Option<BorrowedNativePagedPartV29<'_>> {
    if raw.len() < PAGED_PART_HEADER_BYTES_V29
        || raw.len() > PAGED_PART_HEADER_BYTES_V29 + TEXT_STOP_PART_BYTES_V29
        || raw[..2] != 29u16.to_le_bytes()
    {
        return None;
    }
    let index = u16::from_le_bytes(raw[194..196].try_into().ok()?);
    let count = u16::from_le_bytes(raw[196..198].try_into().ok()?);
    let n = u32::from_le_bytes(raw[198..202].try_into().ok()?) as usize;
    if count == 0 || count > 512 || index >= count || n == 0 || n > TEXT_STOP_PART_BYTES_V29 || raw.len() != 202 + n {
        return None;
    }
    Some(BorrowedNativePagedPartV29 {
        job: raw[2..66].try_into().ok()?,
        seal: raw[66..130].try_into().ok()?,
        owner: raw[130..194].try_into().ok()?,
        index,
        count,
        bytes: &raw[202..],
    })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePagedProofPlanRecordV29 {
    pub version: u16,
    pub max_scope25: Digest,
    pub max_resource29: Digest,
    pub quota: Digest,
    pub maximum_bytes: u32,
    pub maximum_parts: u16,
    pub court_work: u64,
    pub court_scratch: u64,
    pub admission: QuotaQuantitiesV11,
    pub part: QuotaQuantitiesV11,
    pub assembly: QuotaQuantitiesV11,
    pub court: QuotaQuantitiesV11,
    pub conversion: QuotaQuantitiesV11,
    pub guarantee: QuotaQuantitiesV11,
}
impl NativePagedProofPlanRecordV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-paid-paged-proof-plan/v29", self)
    }
}
pub struct CheckedNativePagedProofPlanV29 {
    record: NativePagedProofPlanRecordV29,
}
fn q(bytes: u64, work: u128, scratch: u128) -> Result<QuotaQuantitiesV11, String> {
    let b = bytes as u128;
    let mut q = QuotaQuantitiesV11::default();
    for (a, n) in [
        (A::ScalarSteps, b * 16),
        (A::IndexSteps, b * 16),
        (A::ValidatedElements, b),
        (A::CopiedReferenceBytes, b * 3),
        (A::TraceHashBytes, b * 3),
        (A::ContainerAllocatorAllowance, b + 16384),
        (A::StoredEvidenceBytes, b),
        (A::DeliveryBytes, b),
        (A::ReplyCalls, 1),
        (A::Attempts, 1),
        (A::MetadataWork, work),
        (A::OwnedPeakBytes, scratch + b * 4 + 16384),
    ] {
        q.0[a as usize] = n;
    }
    Ok(q)
}
impl CheckedNativePagedProofPlanV29 {
    pub fn record(&self) -> &NativePagedProofPlanRecordV29 {
        &self.record
    }
    pub fn derive(
        scope: &CheckedResourceScopeV25,
        resources: &FlatResourceVectorV29,
        quota: &QuotaPolicyV11,
        source_max_bytes: u64,
        source_work: u64,
        source_scratch: u64,
        original_restore_bytes: u64,
    ) -> Result<Self, String> {
        quota.check()?;
        if resources.version != 29
            || resources.court_scope != scope.root()
            || resources.positions != scope.max_positions()
            || source_max_bytes == 0
            || source_work == 0
            || source_scratch == 0
            || original_restore_bytes == 0
        {
            return Err("paged29 original checked full-scope/source/restore quote differs".into());
        }
        let (filing, work, scratch, _) = scope.paid_court_vector();
        // Includes exact new source/trace envelope framing. Oversize is refused
        // before Fund rather than squeezing into old64 fragments.
        let bytes = filing
            .max(source_max_bytes)
            .checked_add(4096)
            .filter(|b| *b <= TEXT_STOP_MAX_FILING_BYTES_V29 as u64)
            .ok_or("paged29 full original filing exceeds independent16MiB contract")?;
        let parts = u16::try_from(bytes.div_ceil(TEXT_STOP_PART_BYTES_V29 as u64)).map_err(|_| "paged29 part count overflow")?;
        let court_work = work
            .max(source_work)
            .checked_add(u64::try_from(resources.quantities.get(A::MetadataWork)).map_err(|_| "paged29 metadata work overflow")?)
            .ok_or("paged29 court work overflow")?;
        let court_scratch = scratch.max(source_scratch).checked_add(original_restore_bytes).ok_or("paged29 scratch overflow")?;
        let admission = q(16384, 16384 * 64, 0)?;
        let part = q(
            (TEXT_STOP_PART_BYTES_V29 + PAGED_PART_HEADER_BYTES_V29 + 16384) as u64,
            ((TEXT_STOP_PART_BYTES_V29 + 16384) * 64) as u128,
            0,
        )?;
        let assembly_bytes =
            bytes.checked_add(original_restore_bytes).and_then(|b| b.checked_add(16384)).ok_or("paged29 assembly restore overflow")?;
        let assembly = q(assembly_bytes, assembly_bytes as u128 * 64, assembly_bytes as u128)?;
        let mut court = q(assembly_bytes, court_work as u128, court_scratch as u128)?;
        court.0[A::CourtWork as usize] = court_work as u128;
        court.0[A::CourtBytes as usize] = bytes as u128;
        court.0[A::CourtScratchBytes as usize] = court_scratch as u128;
        // Original R/PC source verification consumes parameter hashing/storage
        // and tree workspace as well as trace work. Price the complete bound,
        // including documentary instance-map bytes and original leaf framing.
        court.0[A::ParameterHashBytes as usize] = (source_max_bytes as u128) * 4;
        court.0[A::RawParameterStorage as usize] = source_max_bytes as u128;
        court.0[A::ParameterTreeScratch as usize] = source_scratch as u128;
        let conversion = admission.clone();
        let guarantee = conversion
            .checked_add(&part.checked_mul(parts as u32 * 2)?)?
            .checked_add(&assembly.checked_mul(2)?)?
            .checked_add(&court.checked_mul(2)?)?;
        admission.checked_add(&guarantee)?.within(&quota.plan)?;
        Ok(Self {
            record: NativePagedProofPlanRecordV29 {
                version: 29,
                max_scope25: scope.root(),
                max_resource29: resources.root(),
                quota: quota.root(),
                maximum_bytes: bytes as u32,
                maximum_parts: parts,
                court_work,
                court_scratch,
                admission,
                part,
                assembly,
                court,
                conversion,
                guarantee,
            },
        })
    }
    /// Expected root comes only from the INITIAL native paid maximum/consent.
    pub fn restore(raw: &[u8], expected: Digest, quota: &QuotaPolicyV11) -> Result<Self, String> {
        if raw.len() > 4096 {
            return Err("paged29 fixed quote cap".into());
        }
        let record: NativePagedProofPlanRecordV29 = borsh::from_slice(raw).map_err(|_| "paged29 fixed quote grammar")?;
        if record.root() != expected
            || record.version != 29
            || record.quota != quota.root()
            || record.maximum_bytes == 0
            || record.maximum_bytes as usize > TEXT_STOP_MAX_FILING_BYTES_V29
            || record.maximum_parts == 0
            || record.maximum_parts > 512
            || record.maximum_parts as u64 != (record.maximum_bytes as u64).div_ceil(32 << 10)
            || record.guarantee
                != record
                    .conversion
                    .checked_add(&record.part.checked_mul(record.maximum_parts as u32 * 2)?)?
                    .checked_add(&record.assembly.checked_mul(2)?)?
                    .checked_add(&record.court.checked_mul(2)?)?
        {
            return Err("paged29 original rooted maximum/count/conservation differs".into());
        }
        record.admission.checked_add(&record.guarantee)?.within(&quota.plan)?;
        Ok(Self { record })
    }
}
/// Fixed original paid owner row. Part allocations remain separate native rows
/// and cannot enlarge this structure or borrow a different job/owner/interval.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePagedSealV29 {
    pub version: u16,
    pub namespace: u16,
    pub job: Digest,
    pub claim: Digest,
    pub original_funding: Digest,
    pub maximum_contract: Digest,
    pub original_plan: Digest,
    pub original_service: Digest,
    pub original_lease: Digest,
    pub maximum_scope: Digest,
    pub real_parameter: Digest,
    pub inventory: Digest,
    pub bond: PalwBondKeyV2,
    pub actor: Digest,
    pub operator: Digest,
    pub producer_operator: Digest,
    pub proof_root: Digest,
    pub seal_root: Digest,
    pub original_paid_receipt: Digest,
    pub count: u16,
    pub total_bytes: u32,
    pub posted: u64,
    pub matures: u64,
    pub reveal_until: u64,
    pub horizon: u64,
    pub admitted_interval: u64,
    pub admitted_block: Digest,
    pub part_attempts: [u8; 512],
    pub accepted: [u8; 64],
    pub assembly_attempts: u8,
    pub court_attempts: u8,
    pub discovery: bool,
    pub original_ticket: Digest,
    pub conversion_attempts: u8,
    pub retired: bool,
    pub spent: bool,
    pub committed: QuotaQuantitiesV11,
    pub consumed: QuotaQuantitiesV11,
    pub remaining: QuotaQuantitiesV11,
}
impl NativePagedSealV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-paid-paged-seal/v29", self)
    }
    pub fn id(&self) -> Digest {
        object_id(
            b"misaka-palw/native/paged-seal-key/v29",
            &(29u16, self.namespace, self.job, self.claim, self.actor, self.proof_root, self.seal_root, self.horizon),
        )
    }
    pub fn accepted(&self, index: u16) -> bool {
        index < 512 && self.accepted[index as usize / 8] & (1 << (index as usize % 8)) != 0
    }
    pub fn check(&self, plan: &CheckedNativePagedProofPlanV29) -> Result<(), String> {
        let p = plan.record();
        let part_count =
            self.part_attempts.iter().try_fold(0u32, |a, n| a.checked_add(*n as u32)).ok_or("paged29 attempt overflow")?;
        let spent = p
            .admission
            .checked_add(&p.conversion.checked_mul(self.conversion_attempts as u32)?)?
            .checked_add(&p.part.checked_mul(part_count)?)?
            .checked_add(&p.assembly.checked_mul(self.assembly_attempts as u32)?)?
            .checked_add(&p.court.checked_mul(self.court_attempts as u32)?)?;
        if self.version != 29
            || ![29, 28].contains(&self.namespace)
            || self.original_plan != p.root()
            || self.maximum_scope != p.max_scope25
            || self.actor != palw_kernel_bond_id_v1(&self.bond)
            || self.operator == self.producer_operator
            || self.count == 0
            || self.count > p.maximum_parts
            || self.total_bytes == 0
            || self.total_bytes > p.maximum_bytes
            || self.count as u64 != (self.total_bytes as u64).div_ceil(32 << 10)
            || self.part_attempts.iter().any(|n| *n > 2)
            || self.part_attempts[self.count as usize..].iter().any(|n| *n != 0)
            || (self.count..512).any(|i| self.accepted(i))
            || (0..self.count).any(|i| self.accepted(i) && self.part_attempts[i as usize] == 0)
            || self.conversion_attempts > 1
            || self.discovery
                && (self.original_ticket == [0; 64]
                    || self.conversion_attempts != 0
                    || part_count != 0
                    || self.accepted != [0; 64]
                    || self.assembly_attempts != 0
                    || self.court_attempts != 0)
            || !self.discovery && self.original_ticket != [0; 64] && self.conversion_attempts != 1
            || self.original_ticket == [0; 64] && self.conversion_attempts != 0
            || self.assembly_attempts > 2
            || self.court_attempts > 2
            || self.matures <= self.posted
            || self.matures >= self.reveal_until
            || self.reveal_until > self.horizon
            || self.committed != p.admission.checked_add(&p.guarantee)?
            || (!self.retired && self.consumed != spent)
            || (self.retired && (self.consumed != self.committed || self.remaining != QuotaQuantitiesV11::default()))
            || self.consumed.checked_add(&self.remaining)? != self.committed
            || [
                self.original_paid_receipt,
                self.job,
                self.claim,
                self.original_funding,
                self.maximum_contract,
                self.proof_root,
                self.seal_root,
                self.real_parameter,
                self.inventory,
                self.original_service,
                self.original_lease,
            ]
            .contains(&[0; 64])
        {
            return Err("paged29 original owner/maximum/clock/attempt/aggregate conservation differs".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePagedLeaseJournalV29 {
    pub version: u16,
    pub namespace: u16,
    pub job: Digest,
    pub funding: Digest,
    pub maximum: Digest,
    pub service: Digest,
    pub lease_root: Digest,
    pub lease: NativeCarriageLeaseV22,
    pub proof_envelope: NativeCarrierEnvelopeV22,
}
impl NativePagedLeaseJournalV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-paged-lease-journal/v29", self)
    }
}
/// Fixed native Commit/terminal projection. It is written alongside the genuine
/// funded book; scheduling cannot mint one from a pending-body header.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePagedJobStatusV29 {
    pub version: u16,
    pub namespace: u16,
    pub job: Digest,
    pub funding: Digest,
    pub claim: Digest,
    pub maximum: Digest,
    pub paid_plan: Digest,
    pub parameter: Digest,
    pub inventory: Digest,
    pub active: bool,
    pub paid_owners: [Digest; 4],
    /// Monotone lifetime capacity; retiring a retained owner never creates another producer obligation.
    pub owners_admitted: u8,
    pub latest_paid_horizon: u64,
    pub admitted_interval: u64,
    pub deadline: u64,
    pub challenge_end: u64,
}
impl NativePagedJobStatusV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/paged-original-job-status/v29", self)
    }
}
impl PalwKernelRouteStateV1 {
    pub fn native_paged_seal_v29(&self, job: Digest, id: Digest) -> Option<NativePagedSealV29> {
        self.native_paged_seal_record_v29(job, id, false)
    }
    /// Archival bounded read only. GC independently authenticates the genuine
    /// original native live/terminal book and complete per-owner horizon.
    pub fn native_paged_seal_original_v29(&self, job: Digest, id: Digest) -> Option<NativePagedSealV29> {
        self.native_paged_seal_record_v29(job, id, true)
    }
    fn native_paged_seal_record_v29(&self, job: Digest, id: Digest, archival: bool) -> Option<NativePagedSealV29> {
        let raw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(29u8, job, id)).ok()?))?;
        if raw.len() > 4096 {
            return None;
        }
        let (root, s): (Digest, NativePagedSealV29) = borsh::from_slice(raw).ok()?;
        let statusraw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(29u8, job, 0u8)).ok()?))?;
        if statusraw.len() > 1024 {
            return None;
        }
        let (statusroot, status): (Digest, NativePagedJobStatusV29) = borsh::from_slice(statusraw).ok()?;
        if status.root() != statusroot
            || status.version != 29
            || status.owners_admitted > 4
            || status.paid_owners.iter().filter(|id| **id != [0; 64]).count() > status.owners_admitted as usize
            || status.namespace != s.namespace
            || status.job != job
            || !archival && !status.active
            || status.funding != s.original_funding
            || status.claim != s.claim
            || status.maximum != s.maximum_contract
            || status.paid_plan != s.original_plan
            || status.parameter != s.real_parameter
            || status.inventory != s.inventory
            || !(status.paid_owners.contains(&id) || archival && s.retired)
            || status.latest_paid_horizon < s.horizon
            || status.admitted_interval != s.admitted_interval
            || status.deadline < s.horizon
        {
            return None;
        }
        let planraw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(29u8, job, 1u8)).ok()?))?;
        let policyraw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(29u8, job, 3u8)).ok()?))?;
        if policyraw.len() > 8192 {
            return None;
        }
        let quota: QuotaPolicyV11 = borsh::from_slice(policyraw).ok()?;
        let plan = CheckedNativePagedProofPlanV29::restore(planraw, s.original_plan, &quota).ok()?;
        (s.root() == root && s.id() == id && s.job == job && s.check(&plan).is_ok()).then_some(s)
    }
    /// Cheap original namespace resolution for the consumer's independent
    /// fence BEFORE MLDSA or any proof payload allocation.
    pub fn native_paged_proof_namespace_v29(&self, job: Digest, id: Digest) -> Option<u16> {
        let raw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(29u8, job, id)).ok()?))?;
        if raw.len() > 4096 {
            return None;
        }
        let (root, s): (Digest, NativePagedSealV29) = borsh::from_slice(raw).ok()?;
        (s.root() == root && s.job == job && s.id() == id && [28, 29].contains(&s.namespace)).then_some(s.namespace)
    }
    pub fn native_paged_proof_owner_v29(
        &self,
        job: Digest,
        id: Digest,
        owner: Digest,
        index: u16,
        count: u16,
        now: u64,
    ) -> Option<PalwBondKeyV2> {
        let s = self.native_paged_seal_v29(job, id)?;
        if s.spent
            || s.retired
            || s.discovery
            || s.actor != owner
            || count != s.count
            || index >= count
            || now < s.matures
            || now >= s.reveal_until
            || s.part_attempts[index as usize] >= 2
            || s.accepted(index)
        {
            return None;
        }
        Some(s.bond)
    }
    pub fn native_paid_paged_carrier_eligibility_v29(
        &self,
        job: Digest,
        id: Digest,
        owner: Digest,
        index: u16,
        count: u16,
        now: u64,
        mass: &NativeCarriageMassParamsV22,
        service: &NativeCarriageServiceV22,
    ) -> Option<NativePaidCarrierEligibilityV22> {
        let bond = self.native_paged_proof_owner_v29(job, id, owner, index, count, now)?;
        let s = self.native_paged_seal_v29(job, id)?;
        let raw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(29u8, job, 22u8)).ok()?))?;
        if raw.len() > 2048 {
            return None;
        }
        let (root, j): (Digest, NativePagedLeaseJournalV29) = borsh::from_slice(raw).ok()?;
        if !service.implemented
            || service.mass_params != mass.root()
            || root != j.root()
            || j.version != 29
            || j.namespace != s.namespace
            || j.job != job
            || j.funding != s.original_funding
            || j.maximum != s.maximum_contract
            || j.service != service.root()
            || s.original_service != j.service
            || s.original_lease != j.lease_root
            || j.lease_root != j.lease.root()
            || j.lease.retired
            || j.lease.original_job != job
            || mass.packet_mass(&j.proof_envelope).ok()? > service.slot_mass
        {
            return None;
        }
        let qraw = self.aux.get(&(EXECUTION_QUOTA_TABLE_V11, borsh::to_vec(&(30u8, job, 1u8)).ok()?))?;
        if qraw.len() > 1024 {
            return None;
        }
        let (qroot, q): (Digest, crate::palw_native_queue_v30::NativeQueueLeaseV30) = borsh::from_slice(qraw).ok()?;
        if q.root() != qroot
            || q.version != 30
            || q.retired
            || q.job != job
            || q.original_funding != s.original_funding
            || q.original_physical_lease != j.lease_root
            || q.original_plan == [0; 64]
            || q.original_policy == [0; 64]
            || q.not_before < s.horizon
            || q.packets == 0
            || q.owned_bytes == 0
        {
            return None;
        }
        Some(NativePaidCarrierEligibilityV22 {
            original_job: job,
            unit: id,
            owner: bond,
            deadline: s.reveal_until,
            kind: 3,
            lease_root: j.lease_root,
            original_service: j.service,
            envelope: j.proof_envelope,
        })
    }
}

pub fn paged_proof_root_v29(bytes: &[u8]) -> Digest {
    misaka_palw_kernel::hash::id(b"misaka-palw/native/paged-whole-file/v29", bytes)
}
pub fn paged_proof_seal_v29(
    namespace: &u16,
    job: &Digest,
    claim: &Digest,
    actor: &Digest,
    maximum: &Digest,
    proof: &Digest,
    salt: &Digest,
) -> Digest {
    object_id(b"misaka-palw/native/original-paged-own-file-seal/v29", &(*namespace, *job, *claim, *actor, *maximum, *proof, *salt))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn borrowed_u16_pages_are_exact_and_do_not_allocate_a_hostile_count() {
        let mut b = vec![0; 202 + 3];
        b[..2].copy_from_slice(&29u16.to_le_bytes());
        b[196..198].copy_from_slice(&512u16.to_le_bytes());
        b[194..196].copy_from_slice(&511u16.to_le_bytes());
        b[198..202].copy_from_slice(&3u32.to_le_bytes());
        let p = borrowed_native_paged_part_v29(&b).unwrap();
        assert_eq!(p.count, 512);
        assert_eq!(p.index, 511);
        assert_eq!(p.bytes.as_ptr(), b[202..].as_ptr());
        b[198..202].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(borrowed_native_paged_part_v29(&b).is_none());
        b[198..202].copy_from_slice(&3u32.to_le_bytes());
        b.push(0);
        assert!(borrowed_native_paged_part_v29(&b).is_none());
    }
}
