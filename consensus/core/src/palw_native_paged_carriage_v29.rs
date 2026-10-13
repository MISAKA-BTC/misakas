//! New full-court25 carriage plan, sharing the original22 physical global.
//! Its u16/512-part contract is never passed through old22's64-part checker.
use crate::{
    palw_flat_execution_capacity_v22::{NativeCarriageGlobalV22, NativeCarriageLeaseV22, NativeCarriageServiceV22},
    palw_native_carriage_v22::{NativeCarriageMassParamsV22, NativeCarrierEnvelopeV22},
    palw_native_paged_proof_v29::CheckedNativePagedProofPlanV29,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    hash::{Digest, object_id},
    merkle::{AXIS_COL, AXIS_ROW},
    merkle3::LayoutV3,
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::program::TirProgramV1;
const SCALE: u64 = 1_000_000_000;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativePagedCarriageRecordV29 {
    pub version: u16,
    pub class: Digest,
    pub original_job_plan: Digest,
    pub parameter: Digest,
    pub pursuit: Digest,
    pub paid_paged_plan: Digest,
    pub service: Digest,
    pub answer_envelope: NativeCarrierEnvelopeV22,
    pub proof_envelope: NativeCarrierEnvelopeV22,
    pub answer_packets: u32,
    pub proof_packets: u32,
    pub answer_window: u64,
    pub proof_window: u64,
    pub density: u64,
    pub minimum_deadline: u64,
}
impl NativePagedCarriageRecordV29 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-full-court-carriage/v29", self)
    }
}
pub struct CheckedNativePagedCarriagePlanV29 {
    record: NativePagedCarriageRecordV29,
}
fn envelope(body: u64) -> Result<NativeCarrierEnvelopeV22, String> {
    Ok(NativeCarrierEnvelopeV22 {
        max_payload_bytes: body.checked_add(8 << 10).ok_or("paged29 envelope overflow")?,
        max_inputs: 1,
        max_outputs: 1,
        max_signature_script_bytes: 8192,
        max_output_script_bytes: 69,
        max_sig_ops: 1,
        max_storage_mass: 100000,
    })
}
fn density(n: u32, window: u64, period: u64) -> Result<u64, String> {
    if window == 0 {
        return Err("paged29 zero original service window".into());
    }
    u64::try_from(
        (n as u128)
            .checked_mul(period as u128)
            .and_then(|n| n.checked_mul(SCALE as u128))
            .ok_or("paged29 density overflow")?
            .div_ceil(window as u128),
    )
    .map_err(|_| "paged29 density exceeds global bound".into())
}
impl CheckedNativePagedCarriagePlanV29 {
    pub fn record(&self) -> &NativePagedCarriageRecordV29 {
        &self.record
    }
    pub fn derive(
        program: &TirProgramV1,
        real_pc: &ParamCommitmentsV1,
        real_params: usize,
        paid: &CheckedNativePagedProofPlanV29,
        class: Digest,
        job_plan: Digest,
        pursuit: Digest,
        answer_window: u64,
        proof_window: u64,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
    ) -> Result<Self, String> {
        if real_params > program.params.len()
            || answer_window <= service.period_daa
            || proof_window <= service.period_daa
            || service.version != 22
            || service.mass_params != mass.root()
            || service.slot_mass == 0
            || service.period_daa == 0
            || [class, job_plan, pursuit].contains(&[0; 64])
        {
            return Err("paged29 original checked source/job/service/window absent".into());
        }
        let expected = program.params[..real_params]
            .iter()
            .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
            .ok_or("paged29 original real-PC count overflow")?;
        if expected != real_pc.by_instance.len() {
            return Err("paged29 carriage real source namespace differs".into());
        }
        for (j, p) in program.params[..real_params].iter().enumerate() {
            if p.per_layer {
                for l in 0..program.schedule.layers.len() {
                    if !real_pc.by_instance.contains_key(&(j as u16, Some(l as u16))) {
                        return Err("paged29 source layer absent".into());
                    }
                }
            } else if !real_pc.by_instance.contains_key(&(j as u16, None)) {
                return Err("paged29 source parameter absent".into());
            }
        }
        let mut source_bytes = 163u64;
        for p in &program.params[..real_params] {
            let dims: Vec<_> = p.shape.iter().map(|d| *d as usize).collect();
            let layout = LayoutV3::try_of(&dims).ok_or("paged29 source layout overflow")?;
            for axis in [AXIS_ROW, AXIS_COL] {
                let leaves = layout.leaves(axis);
                if leaves == 0 {
                    return Err("paged29 source tree empty".into());
                }
                let cells = layout.leaf_element_count(axis, 0, 0).ok_or("paged29 original complete leaf absent")?;
                let depth = if leaves <= 1 { 0 } else { 64 - (leaves - 1).leading_zeros() as u64 };
                let b = 1 + 1 + 4 + dims.len() as u64 * 8 + 1 + 8 + 8 + 4 + 4 + 64;
                source_bytes = source_bytes.max(
                    b.checked_add(cells.checked_mul(p.dtype.width() as u64).ok_or("paged29 source width overflow")?)
                        .and_then(|b| b.checked_add(depth * 64))
                        .ok_or("paged29 source answer overflow")?,
                );
            }
        }
        let answer_envelope = envelope(source_bytes.max(16 << 10).checked_add(75).ok_or("paged29 answer wrapper overflow")?)?;
        let proof_envelope = envelope((32 << 10) + 202 + 7)?;
        if mass.packet_mass(&answer_envelope)? > service.slot_mass || mass.packet_mass(&proof_envelope)? > service.slot_mass {
            return Err("paged29 original complete leaf requires an unconnected source-part lane".into());
        }
        let answer_packets = 11 * 2;
        let proof_packets = 4u32
            .checked_mul(paid.record().maximum_parts as u32)
            .and_then(|n| n.checked_mul(2))
            .ok_or("paged29 proof burst overflow")?;
        let density = density(answer_packets, answer_window, service.period_daa)?
            .checked_add(density(proof_packets, proof_window, service.period_daa)?)
            .ok_or("paged29 combined answer/proof density overflow")?;
        Ok(Self {
            record: NativePagedCarriageRecordV29 {
                version: 29,
                class,
                original_job_plan: job_plan,
                parameter: real_pc.root(),
                pursuit,
                paid_paged_plan: paid.record().root(),
                service: service.root(),
                answer_envelope,
                proof_envelope,
                answer_packets,
                proof_packets,
                answer_window,
                proof_window,
                density,
                minimum_deadline: answer_window.min(proof_window),
            },
        })
    }
}
impl NativeCarriageGlobalV22 {
    /// Original global conservation, with a freshly derived NEW paged plan.
    /// Probe math may be recorded; Final27 separately requires actual readiness.
    pub fn admit_paged_v29(
        &mut self,
        plan: &CheckedNativePagedCarriagePlanV29,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
        job: Digest,
        consent: Digest,
    ) -> Result<NativeCarriageLeaseV22, String> {
        let p = plan.record();
        let deadline = self.minimum_deadline.min(p.minimum_deadline);
        let next = self.density.checked_add(p.density).ok_or("paged29 shared physical density overflow")?;
        let blocked = density(1, deadline, service.period_daa)?;
        if self.version != 22
            || self.service != service.root()
            || p.service != service.root()
            || service.mass_params != mass.root()
            || self.live >= 128
            || job == [0; 64]
            || consent == [0; 64]
            || next.checked_add(blocked).is_none_or(|n| n > SCALE)
            || self.live == 0 && (self.density != 0 || self.minimum_deadline != u64::MAX)
            || mass.packet_mass(&p.answer_envelope)? > service.slot_mass
            || mass.packet_mass(&p.proof_envelope)? > service.slot_mass
        {
            return Err("paged29 synchronous original paid obligations exceed shared physical service".into());
        }
        self.live += 1;
        self.density = next;
        self.minimum_deadline = deadline;
        Ok(NativeCarriageLeaseV22 {
            version: 22,
            original_job: job,
            original_consent: consent,
            plan: p.root(),
            density: p.density,
            minimum_deadline: p.minimum_deadline,
            retired: false,
        })
    }
}
