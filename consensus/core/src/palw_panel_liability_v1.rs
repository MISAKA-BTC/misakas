//! **Panel collateral: Active and Resolved liability (audit G-13's capacity; user direction 2026-10-16,
//! `docs/design/palw/t12-daa14000-release-plan-2026-10-15.md` "court の全体容量と Panel 担保" §2).**
//!
//! The Valid lock is NOT released unconditionally at `F + 1,000` again: past LG14-A a claim's relied-on `Valid` lock lives to its
//! liability horizon (`F + window_court`, `PalwStateParamsV2::valid_lock_life_for_claim_v1`), so a fraud proven late still slashes every
//! `Valid` signer in full. To keep that from eating a Panel's capacity for new work, lane V02's existing split
//! (`Params::palw_final_lock_full_collateral`) carries the lock in two kinds on the SAME bond:
//!
//! ```text
//! Active liability     the claim is under review or adjudication: not yet terminal, or terminal with a live LG14-A reservation
//!                      (the reservation holds Final, or keeps a Final claim's post-Final stage in dispute). Consumes work capacity.
//! Resolved liability   the claim is terminal and nobody holds it in dispute; the lock is still live to its horizon. Off the work
//!                      ceiling, still subtracted from the total collateral, still slashable.
//!
//! R_new = max(0, min(C_work − L_active, C_total − L_active − L_resolved − R_other))
//! ```
//!
//! `C_work` is the ratio ceiling (`fp_max_exposure_ratio_permille` of the collateral), `R_other` the accuser ledger (A-6) and, for the
//! room lane V02's relief may spend, the accuser reserve. The fold's work gate (`palw_rcore_work_room_of_ceiling_v1`) is exactly this
//! room — never more than `R_new(accuser)`, never less than `R_new(accuser + reserve)` — so nothing is pledged twice:
//! `L_active + L_resolved + accuser ≤ C_total` at every gate. This module is the READ of that split ([`palw_panel_liability_v1`]); the
//! one rule it adds to the fold is the Active class of a terminal claim held by a live reservation
//! (`crate::palw_state_v2::palw_bond_resolved_locks_v1`), which reads LG14-A's dispute records and so is empty — byte for byte the
//! shipped split — below `palw_legacy_public_filer_v1`.
//!
//! No aggregate-liability coefficient (ADR-0160's ρ) is used: each lock is backed one-for-one by posted collateral.

use crate::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2, palw_accuser_exposure_v1, palw_bond_accuser_reserve_v1,
    palw_bond_committed_raw_v1, palw_bond_off_ceiling_raw_v1,
};

/// **One bond's collateral split at a DAA** (sompi).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwPanelLiabilityV1 {
    /// `C_total`: the posted collateral.
    pub collateral: u128,
    /// `C_work`: the ratio ceiling on work in flight.
    pub work_ceiling: u128,
    /// `L_active`: everything the bond has committed that is not Resolved (reserved work, registration, locks on claims under review).
    pub active: u128,
    /// `L_resolved`: live locks on terminal claims nobody holds in dispute — 0 where `palw_final_lock_full_collateral` is not in force.
    pub resolved: u128,
    /// The accuser ledger (A-6): what the bond holds as an accuser.
    pub accuser: u128,
    /// Lane V02's accuser reserve: what the relief past the unsplit room never spends.
    pub accuser_reserve: u128,
}

impl PalwPanelLiabilityV1 {
    /// Everything pledged: `L_active + L_resolved + accuser`.
    pub fn pledged(&self) -> u128 {
        self.active.saturating_add(self.resolved).saturating_add(self.accuser)
    }

    /// Pledged past the collateral — never, on any state the fold writes.
    pub fn over_pledged(&self) -> bool {
        self.pledged() > self.collateral
    }

    /// **`R_new`** with `R_other = r_other`: `max(0, min(C_work − L_active, C_total − L_active − L_resolved − R_other))`.
    pub fn r_new(&self, r_other: u128) -> u128 {
        let work = self.work_ceiling.saturating_sub(self.active);
        let total = self.collateral.saturating_sub(self.active).saturating_sub(self.resolved).saturating_sub(r_other);
        work.min(total)
    }
}

/// **`bond`'s split at `now_daa`** (the RAW second-clock depth, the escape computed as the work gate computes it); `None` for a missing
/// bond. The terms are the work gate's own: `committed` = `L_active + L_resolved`, `off_ceiling` = `L_resolved`.
pub fn palw_panel_liability_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    now_daa: u64,
    raw_depth: Option<u64>,
) -> Option<PalwPanelLiabilityV1> {
    let record = state.bond(bond)?;
    let collateral = u128::from(record.collateral);
    let committed = palw_bond_committed_raw_v1(state, params, bond, now_daa, raw_depth);
    let resolved = palw_bond_off_ceiling_raw_v1(state, params, bond, now_daa, raw_depth).min(committed);
    Some(PalwPanelLiabilityV1 {
        collateral,
        work_ceiling: collateral.saturating_mul(u128::from(params.fp_max_exposure_ratio_permille())) / 1000,
        active: committed - resolved,
        resolved,
        accuser: palw_accuser_exposure_v1(state, bond),
        accuser_reserve: palw_bond_accuser_reserve_v1(params, now_daa),
    })
}

impl crate::config::params::Params {
    /// **LG14-A beside lane V02's short lock life needs the split** (G-13's capacity, user direction 2026-10-16 §2). Past LG14-A a
    /// claim's `Valid` lock lives to `F + window_court` where V02's `palw_final_lock_life` would have released it at `F + 1,000`; that
    /// extension must be Resolved liability (off the work ceiling, inside the collateral), never work capacity — so wherever both are in
    /// force, `palw_final_lock_full_collateral` must be in force at or below the later of the two. Without the short life (testnet-12 as
    /// launched) a lock already lives to the horizon and nothing is required. No-op on every preset (LG14-A is `None`).
    pub fn validate_palw_g13_liability_split_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        let (Some(filer), Some(life)) = (self.palw_legacy_public_filer_v1_fence(), self.palw_final_lock_life_fence()) else {
            return Ok(());
        };
        let later = filer.daa_score().max(life.daa_score());
        if self.palw_final_lock_full_collateral_fence().is_some_and(|split| split.daa_score() <= later) {
            return Ok(());
        }
        Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
            "palw_legacy_public_filer_v1 beside palw_final_lock_life needs palw_final_lock_full_collateral in force at or below the \
             later of the two: G-13 keeps a Valid lock to F + window_court, and that extension must be Resolved liability (off the work \
             ceiling, inside the collateral), never work capacity",
        ))
    }
}
