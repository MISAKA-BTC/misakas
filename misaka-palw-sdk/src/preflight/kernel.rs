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
}
