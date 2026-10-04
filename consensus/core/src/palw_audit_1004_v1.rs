//! **The 2026-10-04 audit's consensus fixes, behind one dormant fence** (lane PA; `palw_audit_1004_v1`; no height on any preset —
//! the lead sets it after the DAA-5,300 release). The findings are `lanes/evidence/palw-fatal-audit-1004/FINDINGS.md`; each rule
//! below is one finding, named by its id, and every one reads [`PalwStateParamsV2::audit_1004_active_at`] (the fence's mirror on the
//! V2 bundle's state params) or the same height off `Params`. Below the fence every path folds byte for byte as at 0b1c11b87.
//!
//! The rules (what changes past the fence):
//!
//! * **P-F1** — a `TrapRevealed` never voids a claim that has a dispute open (a court session or a DA session): the claim's
//!   own court decides it. (Before: the void closed the court neutrally and the fraudulent executor walked.)
//! * **P-F4** — a trap commitment must precede the audit draw it is revealed against (`committed_daa < drawn_daa`) and may not be a
//!   single-tile trap (`tiles ≥ 2`; with one tile every ticket lands on the planted tile).
//! * **B-F3** — a vertex `Valid`/`Invalid` leaf of an RFC-0006 layer-sharded claim is Ignored, not a vertex-dropping refusal.
//! * **B-F4** — equivocation evidence signed past the block's DAA is refused (the plain vertex has that check).
//! * **C-F1 / B-F6** — a rider's cheap state tests run before the full-state checkpoint and before any ML-DSA verification, and the
//!   processor's object filter requires `executor_pubkey == bond.pubkey`.
//! * **S-1 / B-F5** — a `TirShardPlanDeclared`'s shape is refused before the O(L²) minimum-shards search, and the search is bounded.
//! * **RF-1** — a `CandidateSubmitted` / `DatasetRegistered` is accepted only from the class's `registrant_bond`.
//! * **RF-3 / P-F5** — the improvement epoch draw and the mesh audit seed read a block `AUDIT_1004_BEACON_DEPTH_V1` chain blocks
//!   below the drawing block's own selected-parent chain (not the drawing block's own hash, which its producer can grind).
//! * **RF-4** — an improvement fee below its floor, a teacher licence past its cap or expiry bound, is refused.
//! * **G-3** — a `ClassRegisteredGenV1` is gated like a TIR registration (one per block) and the cap is checked before the rehearsal fold.
//! * **G-2** — a tensor claim's token binding is bounded by a byte cap before it is padded.
//! * **C-F4** — a mesh audit is paid only on a checked answer, and the pay is re-snapshotted at a rider split.
//!
//! No new state field, delta, object tag or carriage tail: every rule is a refusal or a skipped write.

use crate::config::params::{ForkActivation, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **How many selected-chain blocks below the drawing block's selected parent the beacon of RF-3 / P-F5 is read** (the beacon is that
/// block's hash). A companion value of the fence, hashed with it. A drawing block's producer cannot choose it (it is fixed by the
/// parents it builds on), and the improvement draw waits `beacon_delay` DAA past the pool's close, so with
/// `beacon_delay ≥ 2 × depth` ([`PALW_AUDIT_1004_MIN_BEACON_DELAY_V1`], refused by the fenced policy check) the beacon block is
/// itself later than the pool's close.
pub const PALW_AUDIT_1004_BEACON_DEPTH_V1: u64 = 4;

/// RF-3: the least `beacon_delay` a fenced improvement policy may carry (twice the beacon's depth).
pub const PALW_AUDIT_1004_MIN_BEACON_DELAY_V1: u64 = 2 * PALW_AUDIT_1004_BEACON_DEPTH_V1;

/// RF-4: fee floors, in sompi, a fenced improvement policy must meet (a zero fee is a free spam channel and a free epoch).
pub const PALW_AUDIT_1004_MIN_REGISTRATION_FEE_V1: u64 = 1_000_000;
pub const PALW_AUDIT_1004_MIN_EVAL_FEE_PER_JOB_V1: u64 = 10_000;
pub const PALW_AUDIT_1004_MIN_HARD_CASE_FEE_V1: u64 = 100_000;
/// RF-4: the least any of the four bonds (candidate, artifact, setter, dataset) may be.
pub const PALW_AUDIT_1004_MIN_BOND_V1: u64 = 1_000_000;
/// RF-4: a teacher licence's expiry may be at most this many DAA past the block that registers it.
pub const PALW_AUDIT_1004_MAX_LICENCE_LIFE_DAA_V1: u64 = 1 << 21;
/// P-F4: a trap has at least two tiles past the fence — with one, every audit ticket lands on the planted tile.
pub const PALW_AUDIT_1004_MIN_TRAP_TILES_V1: u64 = 2;
/// G-2: the most prompt tokens a tensor claim's token binding may be padded over.
pub const PALW_AUDIT_1004_MAX_BOUND_TOKENS_V1: u64 = 1 << 16;

/// **The values the fingerprint hashes beside the fence's height.**
pub const fn palw_audit_1004_value_v1() -> [u64; 8] {
    [
        PALW_AUDIT_1004_BEACON_DEPTH_V1,
        PALW_AUDIT_1004_MIN_REGISTRATION_FEE_V1,
        PALW_AUDIT_1004_MIN_EVAL_FEE_PER_JOB_V1,
        PALW_AUDIT_1004_MIN_HARD_CASE_FEE_V1,
        PALW_AUDIT_1004_MIN_BOND_V1,
        PALW_AUDIT_1004_MAX_LICENCE_LIFE_DAA_V1,
        PALW_AUDIT_1004_MAX_BOUND_TOKENS_V1,
        PALW_AUDIT_1004_MIN_TRAP_TILES_V1,
    ]
}

/// **The fence's entry for a flag-day list** (not on any list yet: the lead adds it with the height).
pub const PALW_T12_AUDIT_1004_ENTRY: crate::config::params::PalwPostLaunchFenceV1 = crate::config::params::PalwPostLaunchFenceV1 {
    name: "palw_audit_1004_v1",
    set: |params, at| {
        params.palw_audit_1004_v1 = at;
        params.sync_palw_audit_1004_v1();
    },
};

impl Params {
    /// `palw_audit_1004_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it.
    pub fn palw_audit_1004_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_audit_1004_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Are the audit-1004 rules in force at `daa_score`? `false` on every shipped preset.
    pub fn palw_audit_1004_active_at(&self, daa_score: u64) -> bool {
        self.palw_audit_1004_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params, which the fold reads.
    pub fn sync_palw_audit_1004_v1(&mut self) {
        let from_daa = self.palw_audit_1004_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_audit_1004_from_daa(from_daa);
        }
    }

    /// **The fence's refusals**, asked by [`Params::validate_palw_v2`]: the mirror disagrees with the fence; the fence off a
    /// `ConsensusV2` network; without the model registry or the economic-safety bundle at or below it (the rules it changes live in
    /// those bundles' folds; a rule whose own feature is not armed is moot, so it needs no further prerequisite).
    pub fn validate_palw_audit_1004_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.audit_1004_from_daa(),
            _ => None,
        };
        let armed = self.palw_audit_1004_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_audit_1004_v1 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_audit_1004_v1",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_audit_1004_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let below = |fence: Option<ForkActivation>| fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !below(self.palw_economic_safety) {
            return Err(PalwModeV2Error::Invalid("palw_audit_1004_v1 needs palw_economic_safety at or below it: it amends that bundle's folds"));
        }
        if !below(self.palw_model_registry) {
            return Err(PalwModeV2Error::Invalid("palw_audit_1004_v1 needs palw_model_registry at or below it: it amends the registry's folds"));
        }
        Ok(())
    }
}
