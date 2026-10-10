//! **The kernel route of a lowered program** (ADR-0172, RFC-0011 §16.2/§16.4): the reference kernels' static outcome for the
//! shape-only program — K2-TIR-v1, and K2-TIR-v2 (its multi-modulus dense relation) where v1 cannot express it — twice — under the schedule this binary ships (where the kernel is `Implemented`, never active, so a program the
//! kernel can express reads `KERNEL_NOT_ACTIVE`) and **hypothetically armed** (what the kernel would say if it were active), with the
//! §16.4 coverage bucket. Reported, never judged: no stage verdict reads it, and no row counts as covered because of it — a bucket of
//! `supported_active_kernel` needs on-chain registration evidence, which a preflight never has.

use misaka_palw_kernel::check::registration_outcome_v1;
use misaka_palw_kernel::descriptor::{
    KernelDescriptorV1, KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor, k2_tir_v2_descriptor,
};
use misaka_palw_kernel::outcome::RegistrationOutcomeV1;
use misaka_palw_tir::program::TirProgramV1;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct KernelRouteInfo {
    /// The descriptor judged (`K2-TIR-v1`, or `K2-TIR-v2` when only it expresses the program) and its digest's first 16 hex.
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
}

/// The route of `program` at `max_positions` (clamped to its history bound), judged at `daa`: the first shipped kernel that would
/// accept it if armed (oldest first), else the newest kernel's blocker.
pub fn kernel_route_of(program: &TirProgramV1, max_positions: u32, daa: u64) -> KernelRouteInfo {
    let kernels = [("K2-TIR-v1", k2_tir_v1_descriptor()), ("K2-TIR-v2", k2_tir_v2_descriptor())];
    let routes: Vec<KernelRouteInfo> = kernels.iter().map(|(name, d)| route_under(name, d, program, max_positions, daa)).collect();
    let pick = routes.iter().position(|r| r.hypothetical == "ELIGIBLE_AT").unwrap_or(routes.len() - 1);
    routes[pick].clone()
}

fn route_under(name: &str, d: &KernelDescriptorV1, program: &TirProgramV1, max_positions: u32, daa: u64) -> KernelRouteInfo {
    let root = misaka_palw_kernel::hash::id(b"misaka-palw/census/program-placeholder/v1", &program.encode());
    let positions = max_positions.clamp(1, program.history_bound.max(1));
    let judge = |schedule: &KernelScheduleV1| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| registration_outcome_v1(schedule, d, program, root, positions, daa)))
            .unwrap_or_else(|_| RegistrationOutcomeV1::FrontendRequired { reason: "the kernel check panicked".into() })
    };
    let shipped = judge(&builtin_schedule_v1());
    let armed = judge(&KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 }));
    KernelRouteInfo {
        kernel: format!("{name} {}", misaka_palw_kernel::hash::hex(&d.digest()[..8])),
        shipped: shipped.code().to_string(),
        hypothetical: armed.code().to_string(),
        detail: armed.to_string().chars().take(400).collect(),
        bucket: shipped.coverage_bucket(misaka_palw_kernel::outcome::CoverageEvidenceV1::NONE).name().to_string(),
        max_positions: positions,
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
    }
}

/// **The kernel route of a bidirectional encoder's class** (K2-TIR-v5, lane K2S `k2-real-scale.md` §12; HFX 2026-10-10): the
/// class's one-stage pipeline is ONE position over a padded token axis, which the generative route's position-sized bounds refuse for
/// every encoder (the m3 finding). Under K2-TIR-v5 the unit of a court is one element of one committed value, so the same program —
/// the version-1 view of `encoder::bidir_v2`, whose last two params are the job's ids and count — is judged by the descriptor's plan,
/// `check_plan_with_v1` (ranges proven from the inputs' intervals), the per-prosecution gate under the interim route policy and the
/// node's carrier. `shipped` is the outcome under the schedule this binary ships (the descriptor is `Implemented`, never active);
/// `hypothetical` is the same checks with the descriptor armed. Reported beside the generative verdict and **never merged into it**
/// (a different registration route, behind its own dormant fences): no row counts as covered because of it.
pub fn encoder_route_of(program: &misaka_palw_tir::program_v2::TirProgramV2, daa: u64) -> KernelRouteInfo {
    let d = misaka_palw_kernel::descriptor::k2_tir_v5_descriptor();
    let name = format!("K2-TIR-v5 {}", misaka_palw_kernel::hash::hex(&d.digest()[..8]));
    let refused = |code: &str, why: String| KernelRouteInfo {
        kernel: name.clone(),
        shipped: code.to_string(),
        hypothetical: code.to_string(),
        detail: why.chars().take(400).collect(),
        bucket: "frontend_gap".to_string(),
        max_positions: 1,
    };
    let view = program.v1_view();
    let run = || -> Result<KernelRouteInfo, KernelRouteInfo> {
        let enc = misaka_palw_kernel::seg_encoder::encoder_binding_v1(&view)
            .map_err(|e| refused("FRONTEND_REQUIRED", format!("not a K2-TIR-v5 encoder: {e}")))?;
        misaka_palw_kernel::seg_encoder::prove_encoder_ranges_v1(&view, &enc)
            .map_err(|e| refused("FRONTEND_REQUIRED", format!("the ranges are not proven from the inputs' intervals: {e}")))?;
        let root = misaka_palw_kernel::public::program_root_v1(&view.encode());
        let plan = misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &view, root, 1).map_err(|(family, why)| {
            refused("KERNEL_EXTENSION_REQUIRED", format!("a {} relation: {why}", family.name()))
        })?;
        let judge = |schedule: &KernelScheduleV1| {
            misaka_palw_kernel::check::check_plan_with_v1(
                schedule,
                &d,
                &view,
                root,
                &plan,
                daa,
                misaka_palw_kernel::check::RangeRuleV1::ProvenByV2,
            )
        };
        let shipped = match judge(&builtin_schedule_v1()) {
            Ok(a) => RegistrationOutcomeV1::EligibleAt {
                daa,
                descriptor: d.digest(),
                plan_root: a.plan_root,
                error_bits: a.error_bits,
            },
            Err(o) => o,
        };
        let armed_schedule = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        let accepted = match judge(&armed_schedule) {
            Ok(a) => a,
            Err(o) => {
                return Ok(KernelRouteInfo {
                    kernel: name.clone(),
                    shipped: shipped.code().to_string(),
                    hypothetical: o.code().to_string(),
                    detail: o.to_string().chars().take(400).collect(),
                    bucket: shipped.coverage_bucket(misaka_palw_kernel::outcome::CoverageEvidenceV1::NONE).name().to_string(),
                    max_positions: 1,
                });
            }
        };
        // The route's per-prosecution ceilings and the carrier: a class whose priced court does not fit is refused by name.
        let policy = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(
            kaspa_hashes::Hash64::from_bytes([0; 64]),
            kaspa_hashes::Hash64::from_bytes([0; 64]),
        );
        let carrier = kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1;
        let gated = misaka_palw_kernel::gate::public_prosecution_complete_v4(
            &d,
            &plan,
            &view,
            &misaka_palw_kernel::public::ProfileMaterialV1::kernel_route(true),
            &policy.prosecution,
        )
        .map_err(|g| format!("PROSECUTION_BOUND: the gate refuses {g:?}"))
        .and_then(|(bounds, _seg)| {
            misaka_palw_kernel::ledger::carrier_fit_v1(&bounds, carrier, carrier, carrier)
                .map(|_| bounds)
                .map_err(|e| format!("CARRIER_FIT: {e}"))
        });
        let (hypothetical, detail) = match gated {
            Ok(b) => (
                "ELIGIBLE_AT".to_string(),
                format!(
                    "{} relations, eps <= 2^-{}; per prosecution: public {} B, verifier RAM {} B, worst filing {} B",
                    plan.relations.len(),
                    accepted.error_bits,
                    b.max_public_bytes,
                    b.max_verifier_ram,
                    b.max_filing_bytes
                ),
            ),
            Err(why) => {
                let (code, rest) = why.split_once(": ").unwrap_or(("PROSECUTION_BOUND", &why));
                (code.to_string(), rest.to_string())
            }
        };
        Ok(KernelRouteInfo {
            kernel: name.clone(),
            shipped: shipped.code().to_string(),
            hypothetical,
            detail: detail.chars().take(400).collect(),
            bucket: shipped.coverage_bucket(misaka_palw_kernel::outcome::CoverageEvidenceV1::NONE).name().to_string(),
            max_positions: 1,
        })
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
        .unwrap_or_else(|_| Err(refused("FRONTEND_REQUIRED", "the K2-TIR-v5 check panicked".into())))
        .unwrap_or_else(|r| r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dense_moe_program_is_not_active_as_shipped_and_eligible_if_armed_and_an_i128_product_needs_v2() {
        let fx = misaka_palw_tir_sketch::fixture::dense_moe_v1(1);
        let k = kernel_route_of(&fx.program, 64, 0);
        assert_eq!(
            (k.shipped.as_str(), k.hypothetical.as_str(), k.bucket.as_str()),
            ("KERNEL_NOT_ACTIVE", "ELIGIBLE_AT", "kernel_extension_gap")
        );
        assert!(k.kernel.starts_with("K2-TIR-v1"), "{k:?}");
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
