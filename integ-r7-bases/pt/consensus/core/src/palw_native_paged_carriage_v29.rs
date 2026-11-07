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
    pub original_answer_bytes: u32,
    pub answer_parts: u16,
    pub proof_packets: u32,
    pub answer_window: u64,
    pub proof_window: u64,
    pub density: u64,
    pub minimum_deadline: u64,
}
impl NativePagedCarriageRecordV29 {
    pub fn root(&self) -> Digest {
        if self.version == 39 {
            object_id(b"misaka-palw/native/original-complete-source-carriage/v39", self)
        } else {
            if self.version == 32 {
                object_id(b"misaka-palw/native/original-phase-exclusive-producer-carriage/v32", self)
            } else {
                object_id(b"misaka-palw/native/original-full-court-carriage/v29", self)
            }
        }
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
    /// Explicit32 phase-exclusive producer Commit lane. Before a claim exists
    /// no source unit/proof owner can be admitted; once Commit succeeds this
    /// lane becomes ineligible. Queue30 reserves the larger complete burst and
    /// physical22 reserves the smaller service window conservatively. Neither
    /// an old29 record nor an ordinary carrier receives this interpretation.
    pub fn derive_invocation_v32(
        program: &TirProgramV1,
        real_pc: &ParamCommitmentsV1,
        real_params: usize,
        paid: &CheckedNativePagedProofPlanV29,
        class: Digest,
        job_plan: Digest,
        pursuit: Digest,
        answer_window: u64,
        proof_window: u64,
        maximum_positions: u32,
        commit_window: u64,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
    ) -> Result<Self, String> {
        use misaka_palw_kernel::flat_quota11::stage14::bounded28::{
            invocation_commit_bytes_bound_v32, invocation_commit_parts_bound_v32,
        };
        let mut p =
            Self::derive(program, real_pc, real_params, paid, class, job_plan, pursuit, answer_window, proof_window, service, mass)?;
        let bytes = invocation_commit_bytes_bound_v32(maximum_positions)?;
        let parts = invocation_commit_parts_bound_v32(maximum_positions)?;
        if commit_window <= service.period_daa {
            return Err("invocation32 producer Commit has no complete service window".into());
        }
        let r = &mut p.record;
        r.version = 32;
        r.pursuit = object_id(
            b"misaka-palw/native/original-commit-source-phase-contract/v32",
            &(pursuit, maximum_positions, bytes, parts, commit_window),
        );
        r.answer_packets =
            r.answer_packets.max((parts as u32 + 1).checked_mul(2).ok_or("invocation32 original producer attempt burst overflow")?);
        r.answer_window = r.answer_window.min(commit_window);
        r.density = density(r.answer_packets, r.answer_window, service.period_daa)?
            .checked_add(density(r.proof_packets, r.proof_window, service.period_daa)?)
            .ok_or("invocation32 combined original carriage density overflow")?;
        r.minimum_deadline = r.answer_window.min(r.proof_window);
        Ok(p)
    }
    /// Owning39 resource/court/source profile. No old25scope or512-part proof
    /// token is fabricated: all counts come from the private checked39 quota.
    pub fn derive_v39(
        source: &misaka_palw_kernel::flat_source39::CheckedSourceV39,
        maximum: &misaka_palw_kernel::flat_job39::CheckedJobMaximumV39,
        quota: &misaka_palw_kernel::flat_quota11::owning39::CheckedQuotaV39,
        pursuit: Digest,
        answer_window: u64,
        proof_window: u64,
        commit_window: u64,
        service: &NativeCarriageServiceV22,
        mass: &NativeCarriageMassParamsV22,
    ) -> Result<Self, String> {
        let s = quota.source();
        let p = quota.paged();
        let max_post = quota.plan().record().native_post_carrier_cap;
        if quota.maximum() != maximum.root()
            || maximum.request().source != source.root()
            || s.source != source.root()
            || p.source != source.root()
            || s.maximum != maximum.root()
            || p.maximum != maximum.root()
            || s.answer_parts != 4
            || s.answer_attempts != 2
            || p.pages == 0
            || p.pages > 2048
            || p.part_attempts != 2
            || p.retained_owners != 4
            || p.part_bytes != 32 << 10
            || p.maximum_bytes > 64 << 20
            || [answer_window, proof_window, commit_window].iter().any(|w| *w <= service.period_daa)
            || !service.implemented
            || service.check(mass).is_err()
            || pursuit == [0; 64]
        {
            return Err("carriage39 original checked source/maximum/quota/service/windows differ".into());
        }
        let answer_envelope = envelope((32 << 10) + 143 + 7)?;
        let proof_envelope = envelope((32 << 10) + 202 + 7)?;
        if mass.packet_mass(&answer_envelope)? > service.slot_mass || mass.packet_mass(&proof_envelope)? > service.slot_mass {
            return Err("carriage39 complete native packet exceeds original block room".into());
        }
        let commit_packets = u32::try_from(max_post.div_ceil(32 << 10))
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_mul(2))
            .ok_or("carriage39 complete producer transport burst overflow")?;
        let answer_packets = (11u32 * 2 * 4).max(commit_packets);
        let proof_packets = (p.pages as u32).checked_mul(2)
            .and_then(|n| n.checked_add(2 + 1)) // two assemblies and one discovery conversion
            .and_then(|n| n.checked_mul(4))
            .ok_or("carriage39 complete paid proof burst overflow")?;
        let answer_window = answer_window.min(commit_window);
        let density = density(answer_packets, answer_window, service.period_daa)?
            .checked_add(density(proof_packets, proof_window, service.period_daa)?)
            .ok_or("carriage39 original shared density overflow")?;
        Ok(Self {
            record: NativePagedCarriageRecordV29 {
                version: 39,
                class: quota.class(),
                original_job_plan: quota.plan().record().root(),
                parameter: source.parameters().root(),
                pursuit,
                paid_paged_plan: p.root(),
                service: service.root(),
                answer_envelope,
                proof_envelope,
                answer_packets,
                original_answer_bytes: s.answer_bytes,
                answer_parts: s.answer_parts,
                proof_packets,
                answer_window,
                proof_window,
                density,
                minimum_deadline: answer_window.min(proof_window),
            },
        })
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
        // All original v3 row/column cells (up to4096*16 bytes), typed
        // trace nodes/headers and their original Merkle paths fit this bounded
        // response. New29 transports it using ORIGINAL prepaid reply pages;
        // no raw prefix is relabeled as an independently committed tile.
        let answer_bytes = source_bytes
            .max(80 << 10)
            .checked_add(1024)
            .filter(|n| *n <= 128 << 10)
            .ok_or("paged29 original full source/trace response cap")?;
        let answer_parts = u16::try_from(answer_bytes.div_ceil(32 << 10)).map_err(|_| "paged29 answer parts overflow")?;
        let answer_envelope = envelope((32 << 10) + 143 + 7)?;
        let proof_envelope = envelope((32 << 10) + 202 + 7)?;
        if mass.packet_mass(&answer_envelope)? > service.slot_mass || mass.packet_mass(&proof_envelope)? > service.slot_mass {
            return Err("paged29 original paid reply/proof page exceeds actual physical packet mass".into());
        }
        let answer_packets = 11u32
            .checked_mul(2)
            .and_then(|n| n.checked_mul(answer_parts as u32))
            .ok_or("paged29 complete response page burst overflow")?;
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
                original_answer_bytes: answer_bytes as u32,
                answer_parts,
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
