//! Shared claim fixtures for the kernel's end-to-end tests: a producer's honest trace under a descriptor, its §15.3 evidence
//! object, a verifier, and a court that holds only public material.
#![allow(dead_code)]

pub mod chain;
pub mod ledger_world;
pub mod opv_world;

use misaka_palw_kernel::KernelFaultProofV1;
use misaka_palw_kernel::challenge::ChallengeBindingV1;
use misaka_palw_kernel::descriptor::{KernelDescriptorV1, KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor};
use misaka_palw_kernel::evidence::{EvidenceHeaderV1, VerificationEvidenceV1, build_evidence_v1};
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::trace::WiringV1;
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1, trace_v1};
use misaka_palw_kernel::verify::{
    ClaimContextV1, ClaimVerdictV1, ConvictionV1, DismissalV1, MaterialV1, ScopeV1, verify_fault_proof_v1, verify_scope_v1,
};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, dense_moe_v1};

pub type Court = Box<dyn Fn(&KernelFaultProofV1) -> Result<ConvictionV1, DismissalV1>>;

pub const TOKENS: [u32; 5] = [3, 17, 9, 30, 1];
pub const MAX_POSITIONS: u32 = 64;

pub fn root_of(program: &TirProgramV1) -> [u8; 64] {
    misaka_palw_kernel::public::program_root_v1(&program.encode())
}

pub fn active() -> KernelScheduleV1 {
    KernelScheduleV1::default().with(k2_tir_v1_descriptor().digest(), KernelStatusV1::Active { since_daa: 0 })
}

/// An armed schedule for `d` alone.
pub fn active_for(d: &KernelDescriptorV1) -> KernelScheduleV1 {
    KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 })
}

pub struct Claim {
    pub descriptor: KernelDescriptorV1,
    pub program: TirProgramV1,
    pub params: MapParams,
    pub plan: misaka_palw_kernel::VerificationPlanV1,
    pub trace: TraceV1,
    pub pc: ParamCommitmentsV1,
}

impl Claim {
    pub fn honest() -> Self {
        Self::of(dense_moe_v1(7), k2_tir_v1_descriptor())
    }

    /// An honest claim of `fx` under `d` (the reference frontend's plan, the honest trace).
    pub fn of(fx: TirSketchFixtureV1, d: KernelDescriptorV1) -> Self {
        let plan = plan_for_tir_program_v1(&d, &fx.program, root_of(&fx.program), MAX_POSITIONS).unwrap();
        let trace = trace_v1(&fx.program, &fx.params, &TOKENS).unwrap();
        let pc = ParamCommitmentsV1::of(&fx.params);
        Claim { descriptor: d, program: fx.program, params: fx.params, plan, trace, pc }
    }

    pub fn header(&self) -> EvidenceHeaderV1 {
        EvidenceHeaderV1 {
            network_domain: [9; 64],
            ruleset_digest: [3; 64],
            class_binding_id: [7; 64],
            program_root: root_of(&self.program),
            artifact_root: self.pc.root(),
            plan_root: self.plan.root(),
        }
    }

    /// The §15.3 evidence object a producer commits for `committed` (segments of 2 positions).
    pub fn evidence_of(&self, committed: &TraceV1) -> VerificationEvidenceV1 {
        let w = WiringV1::new(&self.program).unwrap();
        build_evidence_v1(&w, &committed.evidence(), &TOKENS, self.header(), &self.descriptor, 2).unwrap()
    }

    pub fn verify_with(&self, committed: &TraceV1, material: &dyn MaterialV1) -> (ClaimVerdictV1, Court) {
        self.verify_scope(committed, material, &ScopeV1::WholeClaim)
    }

    /// Verify one scope of `committed` (the producer's COMMITTED values), served by `material`.
    pub fn verify_scope(&self, committed: &TraceV1, material: &dyn MaterialV1, scope: &ScopeV1) -> (ClaimVerdictV1, Court) {
        let ev = self.evidence_of(committed);
        self.verify_object(committed, &ev, material, scope)
    }

    pub fn verify_object(
        &self,
        committed: &TraceV1,
        ev: &VerificationEvidenceV1,
        material: &dyn MaterialV1,
        scope: &ScopeV1,
    ) -> (ClaimVerdictV1, Court) {
        let d = self.descriptor.clone();
        let trace = committed.evidence();
        let header = self.header();
        let binding = ChallengeBindingV1 {
            network_domain: header.network_domain,
            claim_id: [8; 64],
            class_binding_id: header.class_binding_id,
            plan_root: self.plan.root(),
            evidence_root: ev.root(),
            beacon: [0x42; 64],
        };
        let ctx = ClaimContextV1 {
            descriptor: &d,
            program: &self.program,
            plan: &self.plan,
            trace: &trace,
            evidence: ev,
            header,
            params: &self.pc,
            tokens: &TOKENS,
            binding,
            stage: None,
        };
        let verdict = verify_scope_v1(&ctx, material, scope);
        // The court, as a fresh party holding only the public material (the evidence object, the trace commitments, the param
        // commitments, the tokens).
        let (program, plan, pc, ev) = (self.program.clone(), self.plan.clone(), self.pc.clone(), ev.clone());
        let court = move |proof: &KernelFaultProofV1| {
            let ctx = ClaimContextV1 {
                descriptor: &d,
                program: &program,
                plan: &plan,
                trace: &trace,
                evidence: &ev,
                header,
                params: &pc,
                tokens: &TOKENS,
                binding,
                stage: None,
            };
            verify_fault_proof_v1(&ctx, proof)
        };
        (verdict, Box::new(court))
    }

    /// **What the claim publishes** for `committed`: the record's canonical bytes (claim id and beacon as `verify_object` binds them).
    pub fn publish(&self, committed: &TraceV1) -> Vec<u8> {
        let ev = self.evidence_of(committed);
        misaka_palw_kernel::public::PublicClaimRecordV1 {
            claim_id: [8; 64],
            program_bytes: self.program.encode(),
            plan: self.plan.clone(),
            evidence: ev,
            trace_commitments: committed.evidence().commitments,
            param_commitments: self.pc.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
            tokens: TOKENS.to_vec(),
            beacon: [0x42; 64],
        }
        .to_bytes()
    }

    /// The first `(position, occurrence, node)` whose primitive satisfies `pick`, from position `from`.
    pub fn find(&self, from: u32, pick: impl Fn(&Prim) -> bool) -> (u32, u16, u16) {
        let occ = self.program.occurrences();
        for p in from..TOKENS.len() as u32 {
            for (s, (b, _)) in occ.iter().enumerate() {
                for (n, node) in self.program.blocks[*b as usize].nodes.iter().enumerate() {
                    if pick(&node.prim) {
                        return (p, s as u16, n as u16);
                    }
                }
            }
        }
        panic!("no such node")
    }
}

pub fn bump(t: &mut Tensor, at: usize) {
    // Stay inside the dtype so the lie is arithmetic, not a malformed value.
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}
