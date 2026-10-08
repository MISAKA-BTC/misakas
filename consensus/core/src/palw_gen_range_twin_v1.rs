//! **RFC-0003 PALW-GEN-20 by the range twin — the fence `palw_gen_range_twin_v1`** (testnet-12 arms it with the int-13 list at DAA 9,000).
//!
//! The pipeline admission prices every close of a generative class with the close-sizing twin, under a work cap
//! ([`crate::palw_gen_close_price_v1::PALW_GEN_CLOSE_SIZING_WORK_CAP_V1`], 2^26 steps). Below this fence it is the ELEMENT twin
//! (`palw_tir_close_size_v1`), which visits every demanded element one at a time: a real-size image stage — a vision tower whose
//! one position reads a whole image's patch rows — passes the cap before its closes are sized, so every vision-language class is
//! refused at the cap whatever its closes weigh (the census, 2026-10-04: every Qwen2/2.5-VL text class at a 196-px slot).
//!
//! Past the fence the admission sizes with the RANGE twin (`palw_tir_close_range_v1`, `palw_tir_fence2`'s twin, extended to a
//! pipeline stage's inputs — edges, job images — and its `post`-written states): **the same read sets and the same bounds**, far
//! fewer steps of the same cap. Nothing a close carries changes, and no ceiling moves; only the sizing's own CPU does.
//!
//! **Dormant but for testnet-12's int-13 flag day** (`PALW_T12_INT13_FENCES_V1`, DAA 9,000): `None` on every other preset and on every
//! earlier testnet-12 release; hashed Some-only into `consensus_params_id` and
//! `consensus_schedule_id`, collapsed whole from `Some(never())` by the identity's normaliser, its activation alone visited by
//! `for_each_fence` — `palw_fp_job_v5`'s shape. It needs `palw_gen_v1` in force at or below it.

use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_tir_close_range_v1::PalwTirCloseTwinV1;

/// The fence's entry for a drill, a probe or a flag-day list that arms it (it moves this one field and its bundle mirror).
pub const PALW_DRILL_GEN_RANGE_TWIN_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_gen_range_twin_v1",
    set: |params, at| {
        params.palw_gen_range_twin_v1 = at;
        params.sync_palw_gen_range_twin_v1();
    },
};

impl Params {
    /// Whether the generative range twin sizes closes at `daa_score`.
    pub fn palw_gen_range_twin_active_at(&self, daa_score: u64) -> bool {
        self.palw_gen_range_twin_v1.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// The twin the pipeline admission sizes a generative class's closes with at `daa_score`.
    pub fn palw_gen_close_twin_at(&self, daa_score: u64) -> PalwTirCloseTwinV1 {
        if self.palw_gen_range_twin_active_at(daa_score) { PalwTirCloseTwinV1::Range } else { PalwTirCloseTwinV1::Element }
    }

    /// **Mirror the fence onto the V2 bundle** (the acceptance path reads the bundle): call after the bundle is assembled.
    pub fn sync_palw_gen_range_twin_v1(&mut self) {
        let from_daa = self.palw_gen_range_twin_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_gen_range_twin_from_daa(from_daa);
        }
    }

    /// **The fence's refusals**: the bundle's mirror equal to the fence; armed only on a ConsensusV2 network, with `palw_gen_v1` in
    /// force at or below it.
    pub fn validate_palw_gen_range_twin_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.gen_range_twin_from_daa(),
            _ => None,
        };
        let armed = self.palw_gen_range_twin_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_gen_range_twin_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_gen_range_twin_v1",
            ));
        }
        let Some(fence) = self.palw_gen_range_twin_v1.filter(|f| *f != ForkActivation::never()) else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_gen_range_twin_v1 is armed on a network that is not ConsensusV2"));
        }
        let gen_at_or_below =
            self.palw_gen_v1.map(|g| g.activation).is_some_and(|g| g != ForkActivation::never() && g.daa_score() <= fence.daa_score());
        if !gen_at_or_below {
            return Err(PalwModeV2Error::Invalid(
                "palw_gen_range_twin_v1 needs palw_gen_v1 in force at or below it: it sizes a generative class's closes",
            ));
        }
        Ok(())
    }
}
