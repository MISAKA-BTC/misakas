//! **The kernel route of a lowered program** (ADR-0172, RFC-0011 §16.2/§16.4): the reference kernels' static outcome for the
//! shape-only program — v1/v2 and segmented v4, including the node prosecution/carrier/block bounds — twice, under the
//! schedule this binary ships (where the kernel is `Implemented`, never active, so a program the
//! kernel can express reads `KERNEL_NOT_ACTIVE`) and **hypothetically armed** (what the kernel would say if it were active), with the
//! §16.4 coverage bucket. Reported, never judged: no stage verdict reads it, and no row counts as covered because of it — a bucket of
//! `supported_active_kernel` needs on-chain registration evidence, which a preflight never has.

use misaka_palw_kernel::check::registration_outcome_v1;
use misaka_palw_kernel::descriptor::{
    KernelDescriptorV1, KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor, k2_tir_v2_descriptor,
    k2_tir_v4_descriptor,
};
use misaka_palw_kernel::outcome::RegistrationOutcomeV1;
use misaka_palw_tir::program::TirProgramV1;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct KernelRouteInfo {
    /// The descriptor judged (the oldest single-program kernel fitting the node bounds) and its digest's first 16 hex.
    pub kernel: String,
    /// The outcome under the shipped schedule.
    pub shipped: String,
    /// The outcome with the descriptor armed hypothetically at the judged DAA.
    pub hypothetical: String,
    /// The hypothetical outcome in words (the family and relation a `KERNEL_EXTENSION_REQUIRED` names; the derived bound of an
    /// `ELIGIBLE_AT`).
    pub detail: String,
    /// RFC-0011 §16.4's bucket of the shipped outcome (no on-chain evidence): never `supported_active_kernel` here.
    pub bucket: String,
    /// The positions the plan was checked at.
    pub max_positions: u32,
    /// Segmented v4 requires a separately admitted OPV class; this report never supplies that admission.
    #[serde(default)]
    pub verification_mode_requirement: String,
}

/// The route of `program` at exactly `max_positions`, judged at `daa`: the first shipped kernel that would
/// accept it if armed (oldest first), else the newest kernel's blocker.
pub fn kernel_route_of(program: &TirProgramV1, max_positions: u32, daa: u64) -> KernelRouteInfo {
    let kernels = reference_program_kernels();
    let routes: Vec<KernelRouteInfo> = kernels.iter().map(|(name, d)| route_under(name, d, program, max_positions, daa)).collect();
    let pick = routes.iter().position(|r| r.hypothetical == "ELIGIBLE_AT").unwrap_or(routes.len() - 1);
    routes[pick].clone()
}

/// Single-program generative descriptors the node implements, oldest first. v3 has pipeline inputs; v5 has encoder job inputs.
/// Neither can be selected by treating those inputs as immutable artifact parameters.
pub fn reference_program_kernels() -> [(&'static str, KernelDescriptorV1); 3] {
    [("K2-TIR-v1", k2_tir_v1_descriptor()), ("K2-TIR-v2", k2_tir_v2_descriptor()), ("K2-TIR-v4", k2_tir_v4_descriptor())]
}

/// Static node bounds for a public artifact, under the SAME interim prosecution policy and carrier limits as the node fold.
/// Public availability, OPV admission/economics and activation remain independent chain facts; this function asserts none of them.
pub fn node_program_outcome(
    schedule: &KernelScheduleV1,
    descriptor: &KernelDescriptorV1,
    program: &TirProgramV1,
    root: misaka_palw_kernel::hash::Digest,
    positions: u32,
    daa: u64,
) -> RegistrationOutcomeV1 {
    use misaka_palw_kernel::outcome::RegistrationOutcomeV1 as O;
    let judge = || {
        if root != misaka_palw_kernel::public::program_root_v1(&program.encode()) {
            return O::PlanForged { why: "not the node's canonical kernel program root".into() };
        }
        let semantic = registration_outcome_v1(schedule, descriptor, program, root, positions, daa);
        if !matches!(semantic, O::EligibleAt { .. } | O::KernelNotActive { .. }) {
            return semantic;
        }
        let plan = match misaka_palw_kernel::plan::plan_for_tir_program_v1(descriptor, program, root, positions) {
            Ok(p) => p,
            Err((family, why)) => {
                return O::KernelExtensionRequired {
                    family: Some(family),
                    relation: why.clone(),
                    required: "a complete plan".into(),
                    available: why,
                };
            }
        };
        let policy = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(
            kaspa_hashes::Hash64::from_bytes([0; 64]),
            kaspa_hashes::Hash64::from_bytes([0; 64]),
        );
        let bounds = match misaka_palw_kernel::gate::class_prosecution_bounds_v1(
            descriptor,
            &plan,
            program,
            &misaka_palw_kernel::public::ProfileMaterialV1::kernel_route(true),
            &policy.prosecution,
        ) {
            Ok(b) => b,
            Err(gaps) => {
                if let Some(misaka_palw_kernel::gate::ProsecutionGapV1::Unbounded { what, required, limit }) =
                    gaps.iter().find(|g| matches!(g, misaka_palw_kernel::gate::ProsecutionGapV1::Unbounded { .. }))
                {
                    return O::BoundsExceeded { what, required: *required, limit: *limit };
                }
                return O::ExternalBlocker { why: format!("not publicly prosecutable: {gaps:?}") };
            }
        };
        let carrier = kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 as u128;
        let admission_work =
            match misaka_palw_kernel::gate::program_claim_admission_work_v1(&bounds, program.encode().len(), program, &plan) {
                Ok(work) => work,
                Err(why) => return O::ExternalBlocker { why },
            };
        for (what, required, limit) in [
            ("prosecution filing carrier", bounds.max_filing_bytes as u128, carrier),
            ("prosecution response carrier", bounds.max_response_bytes, carrier),
            ("claim commitment carrier", bounds.max_commit_bytes, carrier),
            ("guaranteed proof work per block", bounds.max_court_work as u128, policy.guaranteed_proof_work_v1() as u128),
            ("claim admission work per block", admission_work as u128, policy.admission_work_limit_v1() as u128),
        ] {
            if required > limit {
                return O::BoundsExceeded { what, required, limit };
            }
        }
        semantic
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(judge))
        .unwrap_or_else(|_| O::FrontendRequired { reason: "the kernel check panicked".into() })
}

fn route_under(name: &str, d: &KernelDescriptorV1, program: &TirProgramV1, max_positions: u32, daa: u64) -> KernelRouteInfo {
    let root = misaka_palw_kernel::public::program_root_v1(&program.encode());
    let positions = max_positions;
    let judge = |schedule: &KernelScheduleV1| node_program_outcome(schedule, d, program, root, positions, daa);
    let shipped = judge(&builtin_schedule_v1());
    let armed = judge(&KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 }));
    KernelRouteInfo {
        kernel: format!("{name} {}", misaka_palw_kernel::hash::hex(&d.digest()[..8])),
        shipped: shipped.code().to_string(),
        hypothetical: armed.code().to_string(),
        detail: armed.to_string().chars().take(400).collect(),
        bucket: shipped.coverage_bucket(misaka_palw_kernel::outcome::CoverageEvidenceV1::NONE).name().to_string(),
        max_positions: positions,
        verification_mode_requirement: if misaka_palw_kernel::descriptor::is_segmented_v1(d) {
            "OptimisticPublicVerification: separate chain admission and economics required".into()
        } else {
            "node-configured verification mode".into()
        },
    }
}

/// **The kernel route of a pipeline class** (a vision-chat model's tower + text stage): K2-TIR-v3's media-pipeline family, as
/// shipped and hypothetically armed. Only v3 implements the family, so v1/v2 are not judged.
pub fn pipeline_route_of(
    pipeline: &misaka_palw_tir::pipeline::TirPipelineV1,
    programs: &[misaka_palw_tir::program_v2::TirProgramV2],
    daa: u64,
) -> KernelRouteInfo {
    use misaka_palw_kernel::pipeline::pipeline_registration_outcome_v1;
    let d = misaka_palw_kernel::descriptor::k2_tir_v3_descriptor();
    let judge = |schedule: &KernelScheduleV1| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pipeline_registration_outcome_v1(schedule, &d, pipeline, programs, daa)
        }))
        .unwrap_or_else(|_| RegistrationOutcomeV1::FrontendRequired { reason: "the kernel check panicked".into() })
    };
    let shipped = judge(&builtin_schedule_v1());
    let armed = judge(&KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 }));
    KernelRouteInfo {
        kernel: format!("K2-TIR-v3 {}", misaka_palw_kernel::hash::hex(&d.digest()[..8])),
        shipped: shipped.code().to_string(),
        hypothetical: armed.code().to_string(),
        detail: armed.to_string().chars().take(400).collect(),
        bucket: shipped.coverage_bucket(misaka_palw_kernel::outcome::CoverageEvidenceV1::NONE).name().to_string(),
        max_positions: pipeline.stages.get(pipeline.output_stage as usize).map(|s| s.max_trip).unwrap_or(0),
        verification_mode_requirement: "node-configured pipeline verification mode".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_bounds_choose_segmented_moe_and_keep_the_proven_i128_relation() {
        let fx = misaka_palw_tir_sketch::fixture::dense_moe_v1(1);
        let k = kernel_route_of(&fx.program, 64, 0);
        assert_eq!(
            (k.shipped.as_str(), k.hypothetical.as_str(), k.bucket.as_str()),
            ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT", "kernel_extension_gap")
        );
        // Semantic v1 eligibility does not include the node's whole-position response/memory ceilings.
        let d = k2_tir_v1_descriptor();
        let root = misaka_palw_kernel::public::program_root_v1(&fx.program.encode());
        let armed = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        assert!(registration_outcome_v1(&armed, &d, &fx.program, root, 64, 0).is_eligible());
        let refused = node_program_outcome(&armed, &d, &fx.program, root, 64, 0);
        assert!(matches!(refused, RegistrationOutcomeV1::BoundsExceeded { .. }), "{refused:?}");
        assert!(k.kernel.starts_with("K2-TIR-v4"), "{k:?}");
        // An i128 accumulator with proven ranges: v1 cannot express it, v2's multi-modulus relation can.
        let wide = misaka_palw_tir_sketch::fixture::wide128_v1(1);
        let k = kernel_route_of(&wide.program, 64, 0);
        assert_eq!((k.shipped.as_str(), k.hypothetical.as_str()), ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT"), "{k:?}");
        assert!(k.kernel.starts_with("K2-TIR-v2"), "{k:?}");
        // Ranges not proven: the frontend's to narrow, under every kernel.
        let wide = misaka_palw_tir_sketch::fixture::wide_v1(1);
        let k = kernel_route_of(&wide.program, 64, 0);
        assert_eq!(k.shipped, "FRONTEND_REQUIRED", "{k:?}");
    }

    #[test]
    fn semantic_eligibility_cannot_skip_the_node_prosecution_limits() {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(1);
        let root = misaka_palw_kernel::public::program_root_v1(&fx.program.encode());
        let d = k2_tir_v2_descriptor();
        let armed = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        assert!(registration_outcome_v1(&armed, &d, &fx.program, root, 8192, 0).is_eligible());
        assert!(matches!(node_program_outcome(&armed, &d, &fx.program, root, 8192, 0), RegistrationOutcomeV1::BoundsExceeded { .. }));
        let route = kernel_route_of(&fx.program, 8192, 0);
        assert!(route.kernel.starts_with("K2-TIR-v4"), "{route:?}");
        assert_eq!((route.shipped.as_str(), route.hypothetical.as_str()), ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT"));
        assert!(route.verification_mode_requirement.contains("separate chain admission"));
        let admission = crate::runtime_pack::commit::static_admission(&fx.program, root, 8192).unwrap();
        assert_eq!(admission.kernel, "K2-TIR-v4");
        // Census and conformance use the exact same node plan, not an artifact-domain root or a widened court policy.
        let d = k2_tir_v4_descriptor();
        let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &fx.program, root, 8192).unwrap();
        assert_eq!(admission.plan_root, plan.root());
    }

    #[test]
    fn artifact_graph_domains_and_oversized_jobs_do_not_admit_a_kernel_plan() {
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(1);
        let d = k2_tir_v4_descriptor();
        let armed = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        let wrong = *kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&fx.program.encode()).as_byte_slice();
        assert!(matches!(node_program_outcome(&armed, &d, &fx.program, wrong, 64, 0), RegistrationOutcomeV1::PlanForged { .. }));
        let root = misaka_palw_kernel::public::program_root_v1(&fx.program.encode());
        for positions in [0, fx.program.history_bound + 1] {
            assert!(matches!(
                node_program_outcome(&armed, &d, &fx.program, root, positions, 0),
                RegistrationOutcomeV1::BoundsExceeded { .. }
            ));
        }
    }

    #[test]
    fn a_vision_pipeline_needs_v3_and_is_eligible_if_armed() {
        let (p, programs) = vision_pipeline_fixture();
        let k = pipeline_route_of(&p, &programs, 0);
        assert_eq!((k.shipped.as_str(), k.hypothetical.as_str()), ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT"), "{k:?}");
        assert!(k.kernel.starts_with("K2-TIR-v3"));
    }

    /// A one-stage pipeline over a toy image encoder (the IR's own `tests/v2common` shape): a canonical job image in, a `Final` out.
    fn vision_pipeline_fixture() -> (misaka_palw_tir::pipeline::TirPipelineV1, Vec<misaka_palw_tir::program_v2::TirProgramV2>) {
        use misaka_palw_tir::DType;
        use misaka_palw_tir::builder::ProgramBuilder;
        use misaka_palw_tir::pipeline::*;
        use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, Ref};
        use misaka_palw_tir::program_v2::*;
        let mut pb = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
        let image = pb.param("vis.image", DType::I16, &[2, 3, 3], false);
        let w = pb.param("vis.w", DType::I8, &[3, 4], false);
        let pre = {
            let mut b = pb.block("vis.pre", vec![]);
            let x = b.cast(image, DType::I32);
            let x = b.reshape_fixed(x, &[6, 3]);
            let y = b.matmul(x, w, DType::I32);
            let s = b.reduce_sum(y, 0, DType::I32);
            b.finish(&[s])
        };
        let carry = {
            let b = &pb.blocks[pre as usize];
            vec![b.nodes[b.carry_out[0] as usize].out.clone()]
        };
        let (post, out) = {
            let mut b = pb.block("vis.post", carry);
            let o = b.clamp(Ref::CarryIn(0), -(1 << 20), 1 << 20, DType::I32);
            b.commit(o);
            let Ref::Node(n) = o else { unreachable!() };
            (b.finish(&[]), n)
        };
        let v1 = pb.finish(pre, vec![], post, out);
        let prog = TirProgramV2::from_v1_lifting_params(
            &v1,
            &[(0, InputSource::External { lo: 0, hi: 255 })],
            OutputDecl::Final { node: out },
        )
        .unwrap();
        let p = TirPipelineV1 {
            version: TIR_PIPELINE_VERSION_V1,
            stages: vec![StageDecl {
                name: "vision".into(),
                program: 0,
                trip: TripRule::Fixed { n: 1 },
                max_trip: 1,
                tokens: None,
                bind: vec![Binding::JobImage { index: 0 }],
            }],
            output_stage: 0,
        };
        (p, vec![prog])
    }
}
