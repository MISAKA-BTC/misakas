//! Original simultaneous packet residency, reserved before new native Fund.
//! EDF density is a delivery bound; it does not bound the burst retained in
//! the mempool. This new contract accounts for BOTH axes and never upgrades a
//! live consent23 into a promise about memory that its producer did not sign.
use crate::{
    palw_flat_execution_capacity_v22::{
        NativeCarriageGlobalV22, NativeCarriageLeaseV22, NativeCarriagePlanV22, NativeCarriageServiceV22,
        derive_native_carriage_plan_v22,
    },
    palw_flat_execution_pursuit_v19::NativePublicPursuitV19,
    palw_native_carriage_v22::{NativeCarriageMassParamsV22, NativeCarrierEnvelopeV22},
    palw_native_paged_carriage_v29::CheckedNativePagedCarriagePlanV29,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_execution11::CheckedNativeExecutionClassV11,
    flat_quota11::CheckedJobQuotaPlanV11,
    hash::{Digest, object_id},
};

pub const MAX_NATIVE_QUEUE_LEASES_V30: usize = 128;
pub const MAX_NATIVE_QUEUE_PACKETS_V30: u32 = 16_384;
pub const NATIVE_QUEUE_ROW_LIMIT_V30: usize = 48 << 10;

/// Trusted network parameters. Packet storage has four complete copies plus
/// 4 KiB of bounded index/transaction/UTXO metadata per copy. This is a charged
/// envelope, not a claim that sizeof(Transaction) measures all resident RAM.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeQueuePolicyV30 {
    pub version: u16,
    pub network_domain: Digest,
    pub max_contracts: u32,
    pub max_packets: u32,
    pub max_owned_bytes: u64,
    pub retained_copies: u32,
    pub metadata_bytes_per_copy: u64,
    pub max_input_utxo_script_bytes: u64,
    pub confirmed_inputs_only: bool,
}
impl NativeQueuePolicyV30 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/trusted-burst-residency-policy/v30", self)
    }
    pub fn probe(network_domain: Digest) -> Self {
        Self {
            version: 30,
            network_domain,
            max_contracts: 128,
            max_packets: MAX_NATIVE_QUEUE_PACKETS_V30,
            max_owned_bytes: 1 << 30,
            retained_copies: 4,
            metadata_bytes_per_copy: 4096,
            max_input_utxo_script_bytes: 69,
            confirmed_inputs_only: true,
        }
    }
    pub fn check(&self, network_domain: Digest) -> Result<(), String> {
        if self.version != 30
            || self.network_domain != network_domain
            || network_domain == [0; 64]
            || self.max_contracts == 0
            || self.max_contracts as usize > MAX_NATIVE_QUEUE_LEASES_V30
            || self.max_packets == 0
            || self.max_packets > MAX_NATIVE_QUEUE_PACKETS_V30
            || self.max_owned_bytes == 0
            || self.max_owned_bytes > 4 << 30
            || self.retained_copies < 4
            || self.retained_copies > 8
            || self.metadata_bytes_per_copy < 4096
            || self.metadata_bytes_per_copy > 16384
            || self.max_input_utxo_script_bytes != 69
            || !self.confirmed_inputs_only
        {
            return Err("queue30 lacks bounded original trusted residency terms".into());
        }
        Ok(())
    }
    pub fn packet_owned_bytes(&self, e: &NativeCarrierEnvelopeV22, mass: &NativeCarriageMassParamsV22) -> Result<u64, String> {
        self.check(self.network_domain)?;
        mass.packet_mass(e)?;
        // Complete serialized fields, plus the populated input UTXO scripts.
        // Unlike the source proof's byte length, this covers the native frame,
        // MLDSA object, transaction signature, output and all index copies.
        if e.max_inputs != 1
            || e.max_outputs != 1
            || e.max_signature_script_bytes > 8192
            || e.max_output_script_bytes > 69
            || e.max_payload_bytes > 128 << 10
        {
            return Err("queue30 carrier differs from original complete singleton envelope".into());
        }
        e.max_payload_bytes
            .checked_add(e.max_signature_script_bytes)
            .and_then(|n| n.checked_add(e.max_output_script_bytes))
            .and_then(|n| n.checked_add(self.max_input_utxo_script_bytes))
            .and_then(|n| n.checked_add(self.metadata_bytes_per_copy))
            .and_then(|n| n.checked_mul(self.retained_copies as u64))
            .ok_or_else(|| "queue30 complete packet residency overflow".into())
    }
    /// A consumer with less RAM cannot advertise this native service. Local
    /// settings never change consensus admission or producer consent.
    pub fn consumer_capacity(&self, packets: usize, owned_bytes: usize) -> bool {
        self.check(self.network_domain).is_ok()
            && packets >= self.max_packets as usize
            && u64::try_from(owned_bytes).is_ok_and(|n| n >= self.max_owned_bytes)
    }
    pub fn check_transaction(
        &self,
        envelope: &NativeCarrierEnvelopeV22,
        mass: &NativeCarriageMassParamsV22,
        tx: &impl crate::tx::VerifiableTransaction,
    ) -> Result<u64, String> {
        mass.check_transaction(envelope, tx)?;
        for i in 0..tx.tx().inputs.len() {
            let entry = tx.utxo(i).ok_or("queue30 carrier has no original populated UTXO")?;
            if entry.block_daa_score == crate::constants::UNACCEPTED_DAA_SCORE {
                return Err("queue30 reserved carrier requires a confirmed original input".into());
            }
            if entry.script_public_key.script().len() as u64 > self.max_input_utxo_script_bytes {
                return Err("queue30 input UTXO exceeds original resident envelope".into());
            }
        }
        self.packet_owned_bytes(envelope, mass)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeQueuePlanRecordV30 {
    pub version: u16,
    pub policy: Digest,
    pub class: Digest,
    pub original_job_plan: Digest,
    pub original_parameter: Digest,
    pub original_carriage: Digest,
    pub original_service: Digest,
    pub original_mass: Digest,
    pub packets: u32,
    pub answer_packet_owned_bytes: u64,
    pub proof_packet_owned_bytes: u64,
    pub owned_bytes: u64,
}
impl NativeQueuePlanRecordV30 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-burst-residency-plan/v30", self)
    }
}
enum CheckedCarriageV30 {
    Fixed(NativeCarriagePlanV22),
    Paged(CheckedNativePagedCarriagePlanV29),
}
/// Private, nonwire quote. Callers cannot supply raw packet counts or a cheaper
/// memory record; both derivations retain the independently checked carriage.
pub struct CheckedNativeQueuePlanV30 {
    record: NativeQueuePlanRecordV30,
    carriage: CheckedCarriageV30,
}
impl CheckedNativeQueuePlanV30 {
    pub fn record(&self) -> &NativeQueuePlanRecordV30 {
        &self.record
    }
    pub fn paged_carriage(&self) -> Option<&CheckedNativePagedCarriagePlanV29> {
        match &self.carriage {
            CheckedCarriageV30::Paged(p) => Some(p),
            _ => None,
        }
    }
    pub fn fixed_carriage(&self) -> Option<&NativeCarriagePlanV22> {
        match &self.carriage {
            CheckedCarriageV30::Fixed(p) => Some(p),
            _ => None,
        }
    }
    fn record_from(
        policy: &NativeQueuePolicyV30,
        mass: &NativeCarriageMassParamsV22,
        class: Digest,
        job: Digest,
        parameter: Digest,
        carriage: Digest,
        service: Digest,
        answers: u32,
        proofs: u32,
        answer: &NativeCarrierEnvelopeV22,
        proof: &NativeCarrierEnvelopeV22,
    ) -> Result<NativeQueuePlanRecordV30, String> {
        policy.check(policy.network_domain)?;
        if [class, job, parameter, carriage, service].contains(&[0; 64]) || answers == 0 || proofs == 0 {
            return Err("queue30 original class/source/paid-carriage absent".into());
        }
        let a = policy.packet_owned_bytes(answer, mass)?;
        let p = policy.packet_owned_bytes(proof, mass)?;
        let packets = answers.checked_add(proofs).ok_or("queue30 packet count overflow")?;
        let bytes = a
            .checked_mul(answers as u64)
            .and_then(|n| p.checked_mul(proofs as u64).and_then(|m| n.checked_add(m)))
            .ok_or("queue30 original burst residency overflow")?;
        if packets > policy.max_packets || bytes > policy.max_owned_bytes {
            return Err("queue30 original full burst exceeds consumer residency".into());
        }
        Ok(NativeQueuePlanRecordV30 {
            version: 30,
            policy: policy.root(),
            class,
            original_job_plan: job,
            original_parameter: parameter,
            original_carriage: carriage,
            original_service: service,
            original_mass: mass.root(),
            packets,
            answer_packet_owned_bytes: a,
            proof_packet_owned_bytes: p,
            owned_bytes: bytes,
        })
    }
    pub fn derive_paged(
        policy: &NativeQueuePolicyV30,
        mass: &NativeCarriageMassParamsV22,
        carriage: CheckedNativePagedCarriagePlanV29,
    ) -> Result<Self, String> {
        let p = carriage.record();
        let record = Self::record_from(
            policy,
            mass,
            p.class,
            p.original_job_plan,
            p.parameter,
            p.root(),
            p.service,
            p.answer_packets,
            p.proof_packets,
            &p.answer_envelope,
            &p.proof_envelope,
        )?;
        Ok(Self { record, carriage: CheckedCarriageV30::Paged(carriage) })
    }
    /// New Final27 over an original fixed11 class still derives its original
    /// checked carriage; it never accepts a serialized caller-made quote22.
    pub fn derive_fixed(
        policy: &NativeQueuePolicyV30,
        class: &CheckedNativeExecutionClassV11,
        job: &CheckedJobQuotaPlanV11,
        pursuit: &NativePublicPursuitV19,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
    ) -> Result<Self, String> {
        let p = derive_native_carriage_plan_v22(class, job, pursuit, service, mass)?;
        let record = Self::record_from(
            policy,
            mass,
            p.class,
            p.original_job_plan,
            p.parameter_root,
            p.root(),
            p.service,
            p.answer_burst_packets,
            p.proof_burst_packets,
            &p.answer_envelope,
            &p.proof_envelope,
        )?;
        Ok(Self { record, carriage: CheckedCarriageV30::Fixed(p) })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeQueueLeaseV30 {
    pub version: u16,
    pub job: Digest,
    pub original_funding: Digest,
    pub original_consent: Digest,
    pub original_plan: Digest,
    pub original_policy: Digest,
    pub original_physical_lease: Digest,
    pub packets: u32,
    pub owned_bytes: u64,
    pub not_before: u64,
    pub retired: bool,
}
impl NativeQueueLeaseV30 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-burst-residency-lease/v30", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct QueueEntryV30 {
    job: Digest,
    funding: Digest,
    lease: Digest,
    packets: u32,
    owned_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeQueueGlobalV30 {
    pub version: u16,
    pub policy: Digest,
    pub service: Digest,
    pub live: u32,
    pub packets: u32,
    pub owned_bytes: u64,
    entries: Vec<QueueEntryV30>,
}
impl NativeQueueGlobalV30 {
    pub fn empty(policy: &NativeQueuePolicyV30, service: &NativeCarriageServiceV22) -> Result<Self, String> {
        policy.check(policy.network_domain)?;
        Ok(Self {
            version: 30,
            policy: policy.root(),
            service: service.root(),
            live: 0,
            packets: 0,
            owned_bytes: 0,
            entries: Vec::new(),
        })
    }
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/shared-original-burst-residency/v30", self)
    }
    pub fn check(&self, policy: &NativeQueuePolicyV30, physical: &NativeCarriageGlobalV22) -> Result<(), String> {
        policy.check(policy.network_domain)?;
        if self.version != 30
            || self.policy != policy.root()
            || self.service != physical.service
            || physical.version != 22
            || self.live != physical.live
            || self.live as usize != self.entries.len()
            || self.live > policy.max_contracts
            || self.packets > policy.max_packets
            || self.owned_bytes > policy.max_owned_bytes
        {
            return Err("queue30 original shared rows differ or live legacy23 lease lacks original queue consent".into());
        }
        let mut packets = 0u32;
        let mut bytes = 0u64;
        let mut previous = None;
        for e in &self.entries {
            if [e.job, e.funding, e.lease].contains(&[0; 64])
                || e.packets == 0
                || e.owned_bytes == 0
                || previous.is_some_and(|j| j >= e.job)
            {
                return Err("queue30 duplicate or malformed original live lease".into());
            }
            previous = Some(e.job);
            packets = packets.checked_add(e.packets).ok_or("queue30 aggregate packets overflow")?;
            bytes = bytes.checked_add(e.owned_bytes).ok_or("queue30 aggregate residency overflow")?;
        }
        if packets != self.packets || bytes != self.owned_bytes {
            return Err("queue30 original counters differ from retained original leases".into());
        }
        Ok(())
    }
    /// Both global mutations are transactional. The caller persists the two
    /// exact resulting rows with original funding/consent, never separately.
    pub fn admit(
        &mut self,
        physical: &mut NativeCarriageGlobalV22,
        policy: &NativeQueuePolicyV30,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
        plan: &CheckedNativeQueuePlanV30,
        job: Digest,
        funding: Digest,
        consent: Digest,
        now: u64,
        not_before: u64,
    ) -> Result<(NativeQueueLeaseV30, NativeCarriageLeaseV22), String> {
        self.check(policy, physical)?;
        let r = plan.record();
        if r.version != 30
            || r.policy != self.policy
            || r.original_service != self.service
            || r.original_mass != mass.root()
            || service.root() != self.service
            || [job, funding, consent].contains(&[0; 64])
            || not_before <= now
            || self.live >= policy.max_contracts
            || self.entries.iter().any(|e| e.job == job)
            || self.packets.checked_add(r.packets).is_none_or(|n| n > policy.max_packets)
            || self.owned_bytes.checked_add(r.owned_bytes).is_none_or(|n| n > policy.max_owned_bytes)
        {
            return Err("queue30 original Fund exceeds simultaneously retained packet/count/byte bound".into());
        }
        let mut next_physical = physical.clone();
        let physical_lease = match &plan.carriage {
            CheckedCarriageV30::Fixed(p) => next_physical.admit(p, service, mass, job, consent)?,
            CheckedCarriageV30::Paged(p) => next_physical.admit_paged_v29(p, service, mass, job, consent)?,
        };
        let lease = NativeQueueLeaseV30 {
            version: 30,
            job,
            original_funding: funding,
            original_consent: consent,
            original_plan: r.root(),
            original_policy: policy.root(),
            original_physical_lease: physical_lease.root(),
            packets: r.packets,
            owned_bytes: r.owned_bytes,
            not_before,
            retired: false,
        };
        let mut next = self.clone();
        next.entries.push(QueueEntryV30 { job, funding, lease: lease.root(), packets: r.packets, owned_bytes: r.owned_bytes });
        next.entries.sort_unstable_by_key(|e| e.job);
        next.live += 1;
        next.packets += r.packets;
        next.owned_bytes += r.owned_bytes;
        next.check(policy, &next_physical)?;
        *self = next;
        *physical = next_physical;
        Ok((lease, physical_lease))
    }
    pub fn retire(
        &mut self,
        physical: &mut NativeCarriageGlobalV22,
        policy: &NativeQueuePolicyV30,
        lease: &mut NativeQueueLeaseV30,
        physical_lease: &mut NativeCarriageLeaseV22,
        closure: &CheckedNativeQueueClosureV30,
    ) -> Result<(), String> {
        self.check(policy, physical)?;
        if lease.version != 30
            || lease.retired
            || lease.root() != closure.queue_lease
            || lease.original_policy != self.policy
            || lease.job != closure.job
            || lease.original_funding != closure.funding
            || closure.observed_daa <= lease.not_before
            || closure.terminal == [0; 64]
            || physical_lease.root() != closure.physical_lease
            || physical_lease.root() != lease.original_physical_lease
            || physical_lease.original_job != lease.job
            || physical_lease.original_consent != lease.original_consent
        {
            return Err("queue30 genuine original closure/horizon/lease differs".into());
        }
        let i = self
            .entries
            .iter()
            .position(|e| e.job == lease.job && e.funding == lease.original_funding && e.lease == lease.root())
            .ok_or("queue30 original lease absent/already retired")?;
        let mut next = self.clone();
        let mut p = physical.clone();
        let mut pl = physical_lease.clone();
        p.retire(&mut pl, closure.physical_lease)?;
        let e = next.entries.remove(i);
        next.live = next.live.checked_sub(1).ok_or("queue30 live underflow")?;
        next.packets = next.packets.checked_sub(e.packets).ok_or("queue30 packets underflow")?;
        next.owned_bytes = next.owned_bytes.checked_sub(e.owned_bytes).ok_or("queue30 bytes underflow")?;
        next.check(policy, &p)?;
        *self = next;
        *physical = p;
        *physical_lease = pl;
        lease.retired = true;
        Ok(())
    }
}

/// No Borsh/Serde or public constructor. Native terminal/GC adapters construct
/// this AFTER checking original book, terminal root, all paid pending rows,
/// protected owner/seal fragments, and the retained physical cooldowns.
pub struct CheckedNativeQueueClosureV30 {
    job: Digest,
    funding: Digest,
    queue_lease: Digest,
    physical_lease: Digest,
    terminal: Digest,
    observed_daa: u64,
}
impl CheckedNativeQueueClosureV30 {
    pub(crate) fn from_current_native(
        job: Digest,
        funding: Digest,
        queue_lease: Digest,
        physical_lease: Digest,
        terminal: Digest,
        observed_daa: u64,
        outstanding_original_rows: bool,
    ) -> Result<Self, String> {
        if [job, funding, queue_lease, physical_lease, terminal].contains(&[0; 64]) || outstanding_original_rows {
            return Err("queue30 original genuine native terminal still owns pending state".into());
        }
        Ok(Self { job, funding, queue_lease, physical_lease, terminal, observed_daa })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::TESTNET_PARAMS;
    fn envelope() -> NativeCarrierEnvelopeV22 {
        NativeCarrierEnvelopeV22 {
            max_payload_bytes: 41 << 10,
            max_inputs: 1,
            max_outputs: 1,
            max_signature_script_bytes: 8192,
            max_output_script_bytes: 69,
            max_sig_ops: 1,
            max_storage_mass: 100_000,
        }
    }
    // Internal fixture only: public construction requires original checked
    // class/PC/job/pursuit or the private full-court carriage29 token.
    fn quote(
        policy: &NativeQueuePolicyV30,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
    ) -> CheckedNativeQueuePlanV30 {
        let p = NativeCarriagePlanV22 {
            version: 22,
            class: [1; 64],
            original_job_plan: [2; 64],
            parameter_root: [3; 64],
            public_pursuit: [4; 64],
            service: service.root(),
            answer_envelope: envelope(),
            proof_envelope: envelope(),
            original_source_answer_bytes: 32 << 10,
            source_parts: 1,
            answer_burst_packets: 22,
            proof_burst_packets: 24,
            answer_window: 1_000_000,
            proof_window: 1_000_000,
            density: 92_000,
            minimum_deadline: 1_000_000,
            requires_source_parts: false,
        };
        let record = CheckedNativeQueuePlanV30::record_from(
            policy,
            mass,
            p.class,
            p.original_job_plan,
            p.parameter_root,
            p.root(),
            p.service,
            p.answer_burst_packets,
            p.proof_burst_packets,
            &p.answer_envelope,
            &p.proof_envelope,
        )
        .unwrap();
        CheckedNativeQueuePlanV30 { record, carriage: CheckedCarriageV30::Fixed(p) }
    }
    #[test]
    fn long_delivery_windows_do_not_create_extra_burst_memory() {
        let mass = NativeCarriageMassParamsV22::from_consensus_params(&TESTNET_PARAMS);
        let service = NativeCarriageServiceV22::probe(&mass);
        let mut policy = NativeQueuePolicyV30::probe([9; 64]);
        let one = quote(&policy, &service, &mass).record.owned_bytes;
        policy.max_owned_bytes = one;
        let plan = quote(&policy, &service, &mass);
        let mut physical = NativeCarriageGlobalV22::empty(&service);
        let mut global = NativeQueueGlobalV30::empty(&policy, &service).unwrap();
        let (mut lease, mut physical_lease) =
            global.admit(&mut physical, &policy, &service, &mass, &plan, [10; 64], [11; 64], [12; 64], 1, 1_000_001).unwrap();
        let before = (global.clone(), physical.clone());
        assert!(
            global.admit(&mut physical, &policy, &service, &mass, &plan, [13; 64], [14; 64], [15; 64], 1, 1_000_001).is_err(),
            "EDF fits but simultaneous retained bytes do not"
        );
        assert_eq!((global.clone(), physical.clone()), before, "failed admission changes neither shared global");
        assert!(
            CheckedNativeQueueClosureV30::from_current_native(
                lease.job,
                lease.original_funding,
                lease.root(),
                physical_lease.root(),
                [16; 64],
                1_000_002,
                true
            )
            .is_err()
        );
        let early = CheckedNativeQueueClosureV30::from_current_native(
            lease.job,
            lease.original_funding,
            lease.root(),
            physical_lease.root(),
            [16; 64],
            1_000_001,
            false,
        )
        .unwrap();
        assert!(global.retire(&mut physical, &policy, &mut lease, &mut physical_lease, &early).is_err());
        assert_eq!((global.clone(), physical.clone()), before);
        let closed = CheckedNativeQueueClosureV30::from_current_native(
            lease.job,
            lease.original_funding,
            lease.root(),
            physical_lease.root(),
            [16; 64],
            1_000_002,
            false,
        )
        .unwrap();
        global.retire(&mut physical, &policy, &mut lease, &mut physical_lease, &closed).unwrap();
        assert_eq!((global.live, global.packets, global.owned_bytes, physical.live), (0, 0, 0, 0));
        assert!(global.retire(&mut physical, &policy, &mut lease, &mut physical_lease, &closed).is_err());
    }
    #[test]
    fn old_live_consent_and_forged_counter_cannot_gain_a_new_queue_promise() {
        let mass = NativeCarriageMassParamsV22::from_consensus_params(&TESTNET_PARAMS);
        let service = NativeCarriageServiceV22::probe(&mass);
        let policy = NativeQueuePolicyV30::probe([9; 64]);
        let global = NativeQueueGlobalV30::empty(&policy, &service).unwrap();
        let mut physical = NativeCarriageGlobalV22::empty(&service);
        global.check(&policy, &physical).unwrap();
        physical.live = 1;
        assert!(global.check(&policy, &physical).is_err());
        physical.live = 0;
        let mut forged = global.clone();
        forged.owned_bytes = 1;
        assert!(forged.check(&policy, &physical).is_err());
        assert!(borsh::to_vec(&global).unwrap().len() < NATIVE_QUEUE_ROW_LIMIT_V30);
        assert!(!policy.consumer_capacity(policy.max_packets as usize, policy.max_owned_bytes as usize - 1));
        assert!(policy.consumer_capacity(policy.max_packets as usize, policy.max_owned_bytes as usize));
        let mut overflow = envelope();
        overflow.max_payload_bytes = u64::MAX;
        assert!(policy.packet_owned_bytes(&overflow, &mass).is_err());
        assert!(
            CheckedNativeQueuePlanV30::record_from(
                &policy,
                &mass,
                [1; 64],
                [2; 64],
                [3; 64],
                [4; 64],
                service.root(),
                22,
                MAX_NATIVE_QUEUE_PACKETS_V30,
                &envelope(),
                &envelope()
            )
            .is_err()
        );
    }
}
