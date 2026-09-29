//! **RFC-0002 Phase F: the second IR fence, `Params::palw_tir_fence2`** — the consensus half of the
//! fixes after testnet-12's DAA-2,000 release, one dormant height for a later flag day:
//!
//! * **ref2's H7** (`docs/design/palw/tir/ref2-findings.md`, spec 04b §10.3 and §9.5.6): the box
//!   demand's `TopK` row counts the TopK rows a run of `d` demanded elements can touch —
//!   `min(rows, d)` along a non-innermost axis, `min(rows, ⌈d / k⌉ + 1)` along the last — where the
//!   row before it counted `⌈d / k⌉` and understated an unaligned or non-innermost tile; the value
//!   bound `V` inherits it ([`PalwTirDemandRulesV1`]);
//! * **the `Select`-arm work credit**: the structural work vector credits each `Select`'s arm-only,
//!   uncommitted work at the smaller arm's, the least any execution must do, where it credited both
//!   (`crate::palw_tir_work_v1`);
//! * **the IR data-availability units `TirStepLeaf { index }` and `TirStepNode { level, index }`**
//!   (`crate::palw_da_rcore_v1`): a demand keyed by the claim alone (`DefaultAccusedTirStep`, no
//!   binding, no draws) names one committed step leaf — answered by its preimage and its opening under
//!   the step root — or one interior node of the step tree — answered by its frontier eight levels
//!   down and its opening (`crate::palw_tir_court_v1::PalwTirStepNodeDisclosureV1`) — or, past the
//!   claim's execution, by the claim's binding proving so (`TirStepOutOfRange`); the accused defaults
//!   if it answers none inside `W_disclose`. A seat descends eight levels a session, so a 2^22-leaf
//!   execution's first disputed leaf is three node sessions and one leaf session away: inside one
//!   seat's four.
//!
//! Below the height every rule is the DAA-2,000 release's, byte for byte, and an object only this
//! fence makes legal is one an older build cannot decode (A-2): the acceptance layer drops it by name,
//! and the fold refuses it as the second lock. The fold learns the height through a `#[borsh(skip)]`
//! mirror on the V2 state params (`tir_fence2_from_daa`), written by [`Params::sync_palw_tir_fence2`].

use crate::config::params::{ForkActivation, Params, PalwPostLaunchFenceV1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};

/// **The entry a testnet-12 flag-day list takes to arm the second IR fence**, through its own `set`,
/// which writes the bundle's mirror. On [`crate::config::params::PALW_T12_TIR_FENCE2_FENCES_V1`],
/// whose height is `None` until the user sets it.
pub const PALW_T12_TIR_FENCE2_ENTRY: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_tir_fence2",
    set: |params, at| {
        params.palw_tir_fence2 = at;
        params.sync_palw_tir_fence2();
    },
};

/// **Which box-demand rules a sizing reads** (spec 04b §10.3) — the release's, or this fence's (ref2's
/// H7 `TopK` row). Defined beside the rule it selects (`misaka_palw_tir::admit`); admission v10 and the
/// value bound `V` are asked under the registering block's rules ([`palw_tir_demand_rules_at_v1`]).
pub use misaka_palw_tir::admit::TirDemandRulesV1 as PalwTirDemandRulesV1;

/// The box-demand rules in force at `daa_score` on the fold's copy of the fence.
pub fn palw_tir_demand_rules_at_v1(params: &crate::palw_state_v2::PalwStateParamsV2, daa_score: u64) -> PalwTirDemandRulesV1 {
    if params.tir_fence2_active_at(daa_score) { PalwTirDemandRulesV1::H7 } else { PalwTirDemandRulesV1::Release2000 }
}

impl Params {
    /// The box-demand rules in force at `daa_score` on this ruleset.
    pub fn palw_tir_demand_rules_at(&self, daa_score: u64) -> PalwTirDemandRulesV1 {
        if self.palw_tir_fence2_active_at(daa_score) { PalwTirDemandRulesV1::H7 } else { PalwTirDemandRulesV1::Release2000 }
    }

    /// `palw_tir_fence2`, resolved: `Some` only on a `ConsensusV2` network that armed it with a real
    /// height (a `never()` value is dormant).
    pub fn palw_tir_fence2_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_tir_fence2.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is the second IR fence in force at `daa_score`?** `false` on every shipped preset.
    pub fn palw_tir_fence2_active_at(&self, daa_score: u64) -> bool {
        self.palw_tir_fence2_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`tir_fence2_from_daa`), which the fold
    /// reads. Written here and nothing else; `None` where the fence is not armed (or is `never()`).
    /// Call it wherever the fence is set on an assembled ruleset; [`Self::validate_palw_tir_fence2`]
    /// refuses a ruleset whose copy disagrees.
    pub fn sync_palw_tir_fence2(&mut self) {
        let from_daa = self.palw_tir_fence2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_tir_fence2_from_daa(from_daa);
        }
    }

    /// **The second IR fence's own refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * a V2 bundle whose mirror of the height is not the fence's;
    /// * arming on a ruleset that is not `ConsensusV2`;
    /// * arming without `palw_tir_v1` in force at or below it — every rule it changes is an IR rule,
    ///   and an IR object below `palw_tir_v1` is dropped by name whatever this fence says.
    ///
    /// A `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_tir_fence2(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.tir_fence2_from_daa(),
            _ => None,
        };
        let armed = self.palw_tir_fence2.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_fence2 disagrees with the V2 bundle's mirror: mirror it with Params::sync_palw_tir_fence2 after the bundle is \
                 assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_tir_fence2 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let tir_ok = self.palw_tir_v1.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at);
        if !tir_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_tir_fence2 needs palw_tir_v1 in force at or below it: every rule it changes is an IR rule",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **H7's row, against ref2's example and the release's**: `TopK { axis 0, k 4 }` of a `[4, 3]`
    /// operand, a tile of 4 — the release counts one row (4 elements), H7 the three rows the run
    /// touches (all 12); along the innermost axis an unaligned run touches one row more; both are
    /// capped at the operand and never below the release's row.
    #[test]
    fn h7_counts_the_rows_a_run_touches() {
        let (old, new) = (PalwTirDemandRulesV1::Release2000, PalwTirDemandRulesV1::H7);
        assert_eq!(old.topk_operand_demand(4, 4, 0, &[4, 3]), 4, "the release: ⌈4/4⌉ · 4");
        assert_eq!(new.topk_operand_demand(4, 4, 0, &[4, 3]), 12, "H7: min(3 rows, 4) · 4, the whole operand");
        // The innermost axis: `[3, 8]`, k 4, a run of 4 → ⌈4/4⌉ + 1 = 2 rows of 8.
        assert_eq!(old.topk_operand_demand(4, 4, 1, &[3, 8]), 8);
        assert_eq!(new.topk_operand_demand(4, 4, 1, &[3, 8]), 16);
        // Capped at the rows there are, and at the operand.
        assert_eq!(new.topk_operand_demand(1_000, 4, 1, &[3, 8]), 24);
        assert_eq!(new.topk_operand_demand(1_000, 1, 0, &[5, 2]), 10);
        for (d, k, axis, shape) in
            [(1u64, 1u64, 0usize, vec![4u64, 3]), (7, 2, 1, vec![6, 5]), (64, 8, 2, vec![2, 3, 16]), (3, 3, 1, vec![9, 3, 2])]
        {
            let (a, b) = (old.topk_operand_demand(d, k, axis, &shape), new.topk_operand_demand(d, k, axis, &shape));
            assert!(b >= a, "H7 never counts less than the release: {d} {k} {axis} {shape:?}: {b} < {a}");
        }
    }
}
