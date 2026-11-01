//! Reuse R5 capture with independently pinned, trace-free resident parameters.
//! No previous dense run/binding, parameter export, second compiler or codec.
use super::{F, LocalForwardProfileV2, LocalWideForwardArtifactV2, cap, plus};
use crate::node::{
    memory::TirMemoryReserverV1,
    positive_wide_material_v2::{
        CaptureExpectedV2, WidePanelCaptureV2, WidePanelDiskReserverV2, WidePanelLimitsV2, WidePanelWorkV2, capture_one_pass_v2,
    },
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
};
use std::fs::File;

impl LocalWideForwardArtifactV2<'_> {
    /// One shared, finite local Panel attempt. Caller external inventory is
    /// ADDITIONAL: prompt backing, provider, prior exports, Node preparation and
    /// control owners. The entire metadata/load floor, including prior load
    /// claims, is added internally without raising any configured ceiling.
    /// Keep this same handle through capture/material failures and retries;
    /// creating a new handle supplies no native admission or funding authority.
    pub fn wide_panel_work(&self, mut limits: WidePanelLimitsV2, disk: &WidePanelDiskReserverV2) -> Result<WidePanelWorkV2, F> {
        if self.profile != LocalForwardProfileV2::WideI64Q24 {
            return Err(F::UnsupportedProfile);
        }
        limits.external_retained_bytes = plus(self.quote.operation_retained_bytes, limits.external_retained_bytes)?;
        cap(limits.external_retained_bytes, limits.max_live_heap_bytes)?;
        WidePanelWorkV2::new(limits, self.inputs.expected_class, self.inputs.expected_execution, disk).map_err(|_| F::Guard)
    }

    /// Exactly the existing R5 one-pass wide/greedy/FLAT producer. Independent
    /// class/artifact/job/space pins and root-checked parameters come from this
    /// owner, with no full trace accepted as startup input. Capture borrows self
    /// and prompt, so all metadata/parameter tickets outlive its use by builders.
    ///
    /// Required RAM/disk guards, the same shared one-use Work, cumulative visit,
    /// IO/hash/tape counters and independently authenticated provider openings
    /// retain their existing meanings. Failure is local refusal, never a Miner
    /// conviction, source default or computation credit. No model-wide review.
    pub fn capture_wide_panel_one_pass<'run>(
        &'run self,
        prompt: &'run [u32],
        run_limits: TirWideRunLimitsV2,
        work: &WidePanelWorkV2,
        tape: File,
        reserver: &TirMemoryReserverV1,
    ) -> Result<WidePanelCaptureV2<'run>, F> {
        if self.profile != LocalForwardProfileV2::WideI64Q24 {
            return Err(F::UnsupportedProfile);
        }
        let space = self.inputs.space.wide_space().ok_or(F::UnsupportedProfile)?;
        work.admit_loaded_runner_v2(self.quote.operation_retained_bytes, self.inputs.expected_class, self.inputs.expected_execution)
            .map_err(|_| F::Limit)?;
        let runner = TirClassRunnerWideQ24V2::new(space, self.inputs.compiled.plan(), &self.params, self.inputs.expected_class)
            .map_err(|_| F::Execute)?;
        let expected = CaptureExpectedV2 {
            class: self.inputs.class,
            artifact: self.inputs.artifact,
            job: self.inputs.original_context,
            space,
            prompt,
        };
        capture_one_pass_v2(expected, &runner, run_limits, work, tape, reserver).map_err(|_| F::Execute)
    }
}
