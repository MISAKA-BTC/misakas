//! Delegate Loaded's private resident plan/params to the authoritative Root
//! one-pass wide Panel producer. No raw plan/params getter, public plan closure,
//! second compile/run, new capture codec or all-parameter i128 mirror.
//! Dependency: Root R5 positive_wide_material_v2 + required-reserver wide runner.
use super::{E, LoadedWideArtifactV2, Result, cap, plus};
use crate::node::{
    memory::TirMemoryReserverV1,
    positive_wide_material_v2::{
        CaptureExpectedV2, WidePanelCaptureV2, WidePanelDiskReserverV2, WidePanelLimitsV2, WidePanelWorkV2, capture_one_pass_v2,
    },
    run_wide_q24_v2::{TirClassRunnerWideQ24V2, TirWideRunLimitsV2},
};
use std::fs::File;

impl LoadedWideArtifactV2<'_> {
    /// Construct ONCE for the independently admitted local Panel attempt, then
    /// retain/reuse this same shared work handle across capture/builder/tape/
    /// materials and failures. A new handle is not native funding/permission.
    ///
    /// Here limits.external_retained_bytes is ADDITIONAL caller ownership:
    /// actual prompt backing/capacity, provider/session/callback bookkeeping,
    /// previous exports, other preparations and all independent control owners.
    /// The whole prepaid Loaded floor (already including original input tickets
    /// and startup prior claims) is added internally with checked arithmetic.
    /// No supplied maximum is raised. Mandatory disk guard is still acquired by
    /// the existing Work producer; this is not a new disk/RAM policy or ledger.
    pub fn wide_panel_work(&self, mut limits: WidePanelLimitsV2, disk: &WidePanelDiskReserverV2) -> Result<WidePanelWorkV2> {
        limits.external_retained_bytes = plus(self.quote.operation_retained_bytes, limits.external_retained_bytes)?;
        cap(limits.external_retained_bytes, limits.max_live_heap_bytes)?;
        WidePanelWorkV2::new(limits, self.inputs.verified.class_id(), self.inputs.verified.context_hash(), disk)
            .map_err(|_| E::Prepared)
    }

    /// EXACTLY the existing Root one-pass capture on this private resident
    /// source. All original class/artifact/job/space pins come from Loaded,
    /// never supplied replacements or a changed canonical context/tokenizer.
    /// Only fixed-length greedy FLAT input is implemented by that producer.
    ///
    /// The same shared Work is required; its floor/scope and one-use state are
    /// checked before comparing the borrowed program in runner construction.
    /// A separate prefix metadata charge covers that one comparison. Existing
    /// capture and run producers then retain BOTH required same-ledger RAM
    /// guards, plus Work's cumulative disk/work/IO counters. No optional-global
    /// fallback, clone of plan/params, new re-encoding or second replay is added.
    ///
    /// Return borrows self and prompt: Loaded/startup tickets cannot be dropped
    /// while this capture can still serve a builder. Provider openings remain
    /// independently authenticated by the existing Builder/A1 source contract;
    /// this method builds no full-instance re-encoding inventory/opener.
    /// Caller exports/signatures/Node preparation stay independently reserved.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_wide_panel_one_pass<'run>(
        &'run self,
        prompt: &'run [u32],
        run_limits: TirWideRunLimitsV2,
        work: &WidePanelWorkV2,
        tape: File,
        reserver: &TirMemoryReserverV1,
    ) -> Result<WidePanelCaptureV2<'run>> {
        work.admit_loaded_runner_v2(
            self.quote.operation_retained_bytes,
            self.inputs.verified.class_id(),
            self.inputs.verified.context_hash(),
        )
        .map_err(|_| E::Prepared)?;
        let runner = TirClassRunnerWideQ24V2::new(
            self.inputs.verified.space(),
            self.compiled.plan(),
            &self.params,
            self.inputs.verified.class_id(),
        )
        .map_err(|_| E::Prepared)?;
        let expected = CaptureExpectedV2 {
            class: &self.inputs.expected.class,
            artifact: self.artifact_root,
            job: self.inputs.original_context,
            space: self.inputs.verified.space(),
            prompt,
        };
        capture_one_pass_v2(expected, &runner, run_limits, work, tape, reserver).map_err(|_| E::Prepared)
    }
}
