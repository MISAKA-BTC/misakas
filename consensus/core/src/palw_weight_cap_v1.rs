//! **ADR-0160 F-W (lane cap-weight): claims × N is not fork power × N** — staged claim weight, the
//! per-bond provisional weight cap (J-1), and the capped consensus reservation.
//!
//! # What changes, and only past `Params::palw_capacity_weight_cap`
//!
//! Below the fence every non-terminal claim adds its whole stored `immature_contribution` (β·pwu,
//! "raw") to `bounded_immature`, so a bond's provisional fork power is additive in its claim count
//! and one 2M claim weighs ≈ 555,611 floor claims before any panel has looked at it. Past the fence,
//! for an ATTEMPT claim whose `accepted_daa` is at or past the height (a "new-rule" claim — the
//! ADR-0145 keying, so a claim keeps one accounting for its whole life):
//!
//! ```text
//! x_c      = staged weight of c            (Created 0, Anchored 10‰, Licensed 1000‰ / S2 250‰, Final → safe)
//! W_full   = min(raw_c, C7 ceiling)        (the ceiling is 8k's raw weight: ADR-0160 D-3)
//! X_b      = Σ x_c over b's live new-rule claims
//! W_cap(b) = ⌊C_b / 6,500 MSK⌋ × FCW       (FCW = one genesis-target floor claim's raw weight)
//! bounded_immature = Σ_{old-rule live claims} raw_c  +  Σ_b min(X_b, W_cap(b))
//! reserved(c) = min(w_c, R_budget(b) − Σ reserved of b's live new-rule claims),
//! R_budget(b) = ⌊C_b / 6,500 MSK⌋ × w_FCW
//! ```
//!
//! `bounded_immature` stays the ONE rooted scalar fork choice reads (`PalwCandidateOrderV1::new`), so
//! the comparator is unchanged; only the value past the fence changes. The per-bond sums live in
//! [`PalwBondWeightIndexV1`], a DERIVED index of the claims: never hashed, never carried, rebuilt by
//! every load and delta path from the claims and the bonds, maintained incrementally by the fold's
//! `write_claim` (every phase door passes it) and `write_bond` (every collateral move passes it, so a
//! slash lowers the cap and a refund restores it), and checked against its re-derivation at load and
//! after every block in debug builds ([`palw_bounded_immature_v2`]).
//!
//! # Invariants (ADR-0160 §7.1), each pinned by a test in `palw_state_v2/tests/capacity_weight_cap_v1.rs`
//!
//! * **W-I1** — `bounded_immature − Σ old-rule raw ≤ Σ_b W_cap(b)`, at any claim count.
//! * **W-I2** — monotone: a new claim, a bind and a licence only raise x_c; at `Final` the bond's
//!   term falls by at most x_c ≤ W_full while `safe_weight` rises by the full contribution; live
//!   weight falls only on a void, a slash (the cap), a redraw or a conviction.
//! * **W-I3** — the running value equals [`palw_bounded_immature_v2`] after every block, on reorg
//!   and at load.
//! * **W-I4** — at every acceptance, `Σ reserved of b's live new-rule claims ≤ R_budget(b)`.
//! * **W-I5** — old-rule claims are byte-identical in accounting (a dormant fence moves nothing).
//! * **W-I6** — no underflow on slash, void or redraw.

use std::collections::BTreeMap;

use crate::palw_state_v2::{
    PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2,
    palw_rcore_counts_licensed_v1,
};

/// **FCW — one floor-claim weight**, in `immature_contribution` units: `⌊β·pwu⌋` of a genesis-target
/// floor claim, `⌊0.1 × 279 × 21,657,728⌋` (ADR-0160 Appendix A). The unit of the per-bond cap.
pub const PALW_CAPACITY_FCW_V1: u128 = 604_250_611;

/// **The collateral one FCW of provisional weight needs: 6,500 MSK** (half the 13,000 MSK producer
/// floor — so a floor bond holds exactly today's two instant floor claims of weight; ADR-0160 D-1).
pub const PALW_CAPACITY_COLLATERAL_PER_FCW_SOMPI_V1: u64 = 650_000_000_000;

/// **w_FCW — a floor claim's weight reservation, in sompi** (derived PWU × 5 sompi on testnet-12:
/// 0.1075266 MSK). The unit of the per-bond reservation budget `R_budget`.
pub const PALW_CAPACITY_W_FCW_SOMPI_V1: u128 = 10_752_660;

/// The staged weight of an ANCHORED claim (`PanelBound`), in permille of its full weight.
pub const PALW_CAPACITY_ANCHORED_PERMILLE_V1: u16 = 10;

/// The staged weight of an S2 licence (a recount below `PALW_RCORE_FINAL_BASIS_K_V1`: the full-replay
/// seat's `Valid` alone), in permille of its full weight.
pub const PALW_CAPACITY_S2_PERMILLE_V1: u16 = 250;

/// **The C7 ceiling (ADR-0160 D-3): a claim's full weight never exceeds the raw weight of the heaviest
/// ATTRIBUTABLE class, 8k** (229.86 FCW) — until ADR-0153 gives the 2M class a conviction route.
///
/// Applied class-blind, deliberately: every attributable class weighs at most this by the ceiling's own
/// definition, so the ceiling moves nothing for them, and a class-blind rule keeps [`palw_weight_full_v1`]
/// a pure function of the claim record — the C7 predicate (`palw_rcore_class_is_c7_v1`) also reads
/// `model_lifecycles`, which moves after a claim is accepted, and a weight that changes under a live
/// claim is one the re-derivation cannot reproduce.
pub const PALW_CAPACITY_C7_WEIGHT_CEILING_V1: u128 = 138_892_697_241;

/// **A claim's weight stage** (ADR-0160 §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwWeightStageV1 {
    /// `Provisional` (never bound, or redrawn): weighs 0.
    Created,
    /// `PanelBound`: [`PALW_CAPACITY_ANCHORED_PERMILLE_V1`] of its full weight.
    Anchored,
    /// `ReceiptLicensed`: `permille` of its full weight — 1000 where the licence counts
    /// (`palw_rcore_counts_licensed_v1`), [`PALW_CAPACITY_S2_PERMILLE_V1`] on an S2 recount, and (lane
    /// verify, future) the covered permille on a sampled door.
    Licensed { permille: u16 },
    /// `Final`: nothing in the immature set; its full weight is in `safe_weight`.
    Final,
    /// `Voided`, or retired: nothing.
    Terminal,
}

impl PalwWeightStageV1 {
    /// The permille of the claim's full weight this stage contributes to its bond's provisional sum.
    pub fn permille(self) -> u16 {
        match self {
            PalwWeightStageV1::Created | PalwWeightStageV1::Final | PalwWeightStageV1::Terminal => 0,
            PalwWeightStageV1::Anchored => PALW_CAPACITY_ANCHORED_PERMILLE_V1,
            PalwWeightStageV1::Licensed { permille } => permille.min(1000),
        }
    }
}

fn stage_of_phase(claim: &PalwClaimStateV2, phase: &PalwClaimPhaseV2) -> PalwWeightStageV1 {
    match phase {
        PalwClaimPhaseV2::Provisional => PalwWeightStageV1::Created,
        PalwClaimPhaseV2::PanelBound { .. } => PalwWeightStageV1::Anchored,
        PalwClaimPhaseV2::ReceiptLicensed { .. } => {
            // `palw_rcore_counts_licensed_v1` reads `claim.phase`; the licence record (`rcore`) is the
            // claim's whatever phase a dispute parked it in, so the recount is asked of the record.
            let counted = claim.rcore.licence_door.is_none() || claim.rcore.basis_k >= crate::palw_state_v2::PALW_RCORE_FINAL_BASIS_K_V1;
            PalwWeightStageV1::Licensed { permille: if counted { 1000 } else { PALW_CAPACITY_S2_PERMILLE_V1 } }
        }
        // **An open DA accusation keeps the stage the claim held** (`resumed`, the phase it restores
        // on a refutation). ADR-0160's table lists `DefaultDisputed` beside `PanelBound`, which is the
        // phase most accusations find; reading the resumed phase says the same there and keeps W-I2
        // everywhere: an accusation is not a conviction, so it must not lower live weight (a bonded
        // accuser could otherwise shave an honest chain's weight at will), and a refutation must not
        // raise it.
        PalwClaimPhaseV2::DefaultDisputed { resumed, .. } => stage_of_phase(claim, resumed),
        PalwClaimPhaseV2::Final { .. } => PalwWeightStageV1::Final,
        PalwClaimPhaseV2::Voided { .. } => PalwWeightStageV1::Terminal,
    }
}

/// **The stage of a claim** (a pure function of the record). Agrees with
/// [`palw_rcore_counts_licensed_v1`] on every undisputed licensed claim.
pub fn palw_weight_stage_of_claim_v1(claim: &PalwClaimStateV2) -> PalwWeightStageV1 {
    let stage = stage_of_phase(claim, &claim.phase);
    debug_assert!(
        !matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
            || (stage == PalwWeightStageV1::Licensed { permille: 1000 }) == palw_rcore_counts_licensed_v1(claim),
        "the Licensed stage is the room's counted-licensed predicate"
    );
    stage
}

/// **Is `claim` accounted under the weight cap?** An ATTEMPT claim accepted at or past the fence's
/// height (the mirror). A free-prompt claim never is: it adds no immature weight (its weight arrives
/// per spent quantum) and its reservation is its own lane's (ADR-0160 E-6).
pub fn palw_weight_cap_applies_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    matches!(claim.source, PalwClaimSourceV2::Attempt) && params.capacity_weight_cap_applies_at(claim.accepted_daa)
}

/// **W_full(c)** — the claim's full weight in `immature_contribution` units: its stored raw weight
/// (already ADR-0069 D7-gated at acceptance) under the C7 ceiling where the fence applies, the raw
/// weight itself where it does not.
pub fn palw_weight_full_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u128 {
    if palw_weight_cap_applies_v1(params, claim) {
        claim.immature_contribution.min(PALW_CAPACITY_C7_WEIGHT_CEILING_V1)
    } else {
        claim.immature_contribution
    }
}

/// **x_c — what a new-rule claim adds to its bond's provisional sum X_b**: `⌊W_full × stage‰⌋`. Zero
/// for a claim the fence does not apply to (an old-rule claim adds its raw weight to the uncapped
/// half of `bounded_immature` instead) and for every terminal or `Final` claim.
pub fn palw_staged_weight_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u128 {
    if !palw_weight_cap_applies_v1(params, claim) {
        return 0;
    }
    let full = palw_weight_full_v1(params, claim);
    match palw_weight_stage_of_claim_v1(claim) {
        PalwWeightStageV1::Licensed { permille: 1000 } => full,
        stage => full.saturating_mul(u128::from(stage.permille())) / 1000,
    }
}

/// **W_cap(b)** — the most provisional weight a bond of `collateral_sompi` can put into fork choice:
/// `⌊C / 6,500 MSK⌋` floor-claim weights. Class-neutral (ADR-0160 D-2).
pub fn palw_bond_weight_cap_v1(collateral_sompi: u64) -> u128 {
    u128::from(collateral_sompi / PALW_CAPACITY_COLLATERAL_PER_FCW_SOMPI_V1) * PALW_CAPACITY_FCW_V1
}

/// **R_budget(b)** — the most weight reservation a bond of `collateral_sompi` holds across its live
/// new-rule claims: `⌊C / 6,500 MSK⌋ × w_FCW` (0.215 MSK at 13k, 1.61 at 100k, 16.45 at 1M).
pub fn palw_bond_weight_budget_sompi_v1(collateral_sompi: u64) -> u128 {
    u128::from(collateral_sompi / PALW_CAPACITY_COLLATERAL_PER_FCW_SOMPI_V1) * PALW_CAPACITY_W_FCW_SOMPI_V1
}

/// **The capped consensus reservation**: `min(raw_w, budget − held)`, never negative. `raw_w` is the
/// reservation the claim would take below the fence (`exposure_pwu × slash_value × attempts`).
pub fn palw_capped_weight_reservation_v1(raw_w: u128, budget: u128, held: u128) -> u128 {
    raw_w.min(budget.saturating_sub(held))
}

/// **The weight reservation a claim accepted at `accepted_daa` on `bond` takes** — the ONE reading the
/// fold's `apply_attempt`, the admission ceiling (`palw_admission_v2`) and the producer's headroom
/// (`palw_producer_facts_v4`) share (ADR-0152 SR-7), so the gate can never admit a claim the ledger
/// would record at a different number. Below the fence (every shipped preset) it is `raw_w` itself.
pub fn palw_claim_weight_reservation_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    bond: &PalwBondKeyV2,
    raw_w: u128,
    accepted_daa: u64,
) -> u128 {
    let collateral = state.bond(bond).map(|record| record.collateral).unwrap_or(0);
    palw_claim_weight_reservation_of_v1(params, collateral, state.capacity_weight_index().bond(bond).reserved_w, raw_w, accepted_daa)
}

/// [`palw_claim_weight_reservation_v1`] over a bond record the caller already resolved — the admission
/// gate's bootstrap bond (a registration the parent state does not hold yet) has no row in the state,
/// so its collateral is handed in; `held` is its index row's `reserved_w` (0 for a bond with no claim).
pub fn palw_claim_weight_reservation_of_v1(
    params: &PalwStateParamsV2,
    collateral_sompi: u64,
    held: u128,
    raw_w: u128,
    accepted_daa: u64,
) -> u128 {
    if !params.capacity_weight_cap_applies_at(accepted_daa) {
        return raw_w;
    }
    palw_capped_weight_reservation_v1(raw_w, palw_bond_weight_budget_sompi_v1(collateral_sompi), held)
}

/// **The `safe_weight` a `Final` claim contributes, under the C7 ceiling (ADR-0160 D-3).** `contribution`
/// is today's (`palw_claim_safe_contribution_v3`: its pwu or derived work, D7-gated); a new-rule claim
/// whose raw weight passed the ceiling contributes the same fraction of it that its full weight is of
/// its raw weight, so the ceiling holds in whichever unit `safe_weight` is kept. Every attributable
/// class, and every old-rule claim, is returned `contribution` unchanged. A pure function of the claim
/// record and the mirror, so `finalize_claim`, `retire_claim` and the load-time re-derivation agree.
pub fn palw_weight_final_safe_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, contribution: u128) -> u128 {
    let raw = claim.immature_contribution;
    let full = palw_weight_full_v1(params, claim);
    if raw == 0 || full >= raw {
        return contribution;
    }
    match contribution.checked_mul(full) {
        Some(product) => product / raw,
        None => (contribution / raw).saturating_mul(full),
    }
}

/// **One bond's row of the derived index**: the staged provisional weight of its live new-rule claims
/// (X_b) and the weight reservation they hold (Σ reserved).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwBondWeightV1 {
    pub staged: u128,
    pub reserved_w: u128,
}

impl PalwBondWeightV1 {
    fn is_empty(&self) -> bool {
        self.staged == 0 && self.reserved_w == 0
    }
}

/// **The per-bond weight index** — derived from the claims, excluded from `state_root`, never
/// carried: a row exists exactly while its bond has a live new-rule claim with a non-zero staged
/// weight or reservation, so the index a fold maintains and the one a load rebuilds are EQUAL, not
/// merely equivalent (`PalwChainStateV2: PartialEq` compares them).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwBondWeightIndexV1(BTreeMap<PalwBondKeyV2, PalwBondWeightV1>);

impl PalwBondWeightIndexV1 {
    /// The index of a claim set, from nothing else (the rebuild every load and delta path runs).
    pub fn of<'a>(params: &PalwStateParamsV2, claims: impl Iterator<Item = &'a PalwClaimStateV2>) -> Self {
        let mut index = Self::default();
        if params.capacity_weight_cap_from_daa().is_none() {
            return index;
        }
        for claim in claims {
            index.note(params, claim, true);
        }
        index
    }

    /// A bond's row (zero when it has none).
    pub fn bond(&self, bond: &PalwBondKeyV2) -> PalwBondWeightV1 {
        self.0.get(bond).copied().unwrap_or_default()
    }

    /// Every row, in bond order.
    pub fn iter(&self) -> impl Iterator<Item = (&PalwBondKeyV2, &PalwBondWeightV1)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Add (`add`) or remove one claim record's contribution. Removal saturates at zero — which an
    /// index consistent with its claims never needs (W-I6), and which the load-time comparison would
    /// refuse if it ever did.
    pub(crate) fn note(&mut self, params: &PalwStateParamsV2, claim: &PalwClaimStateV2, add: bool) {
        if !palw_weight_cap_applies_v1(params, claim) {
            return;
        }
        let staged = palw_staged_weight_v1(params, claim);
        let reserved = if claim.phase.is_terminal() { 0 } else { claim.reserved };
        if staged == 0 && reserved == 0 {
            return;
        }
        let row = self.0.entry(claim.bond).or_default();
        if add {
            row.staged = row.staged.saturating_add(staged);
            row.reserved_w = row.reserved_w.saturating_add(reserved);
        } else {
            debug_assert!(row.staged >= staged && row.reserved_w >= reserved, "W-I6: the index holds what it releases");
            row.staged = row.staged.saturating_sub(staged);
            row.reserved_w = row.reserved_w.saturating_sub(reserved);
        }
        if row.is_empty() {
            self.0.remove(&claim.bond);
        }
    }

    /// **`min(X_b, W_cap(C_b))`** — what `bond` contributes to `bounded_immature`, for a bond holding
    /// `collateral` (`None`: the registry holds no such bond, so it carries no cap).
    pub fn term(&self, bond: &PalwBondKeyV2, collateral: Option<u64>) -> u128 {
        let staged = self.bond(bond).staged;
        if staged == 0 {
            return 0;
        }
        staged.min(collateral.map(palw_bond_weight_cap_v1).unwrap_or(0))
    }

    /// `Σ_b min(X_b, W_cap(C_b))` over every row, the bonds' collateral read from `state`.
    pub fn capped_total(&self, state: &PalwChainStateV2) -> u128 {
        self.0.keys().map(|bond| self.term(bond, state.bond(bond).map(|record| record.collateral))).fold(0u128, u128::saturating_add)
    }
}

/// **The re-derivation of `bounded_immature`** (W-I3): `Σ_{live old-rule claims} raw +
/// Σ_b min(X_b, W_cap(C_b))`, from the claims and the bonds alone. Below the fence (every shipped
/// preset) this is today's sum of every live claim's `immature_contribution`.
pub fn palw_bounded_immature_v2(state: &PalwChainStateV2, params: &PalwStateParamsV2) -> u128 {
    let old_rule: u128 = state
        .claims_iter()
        .filter(|(_, claim)| !claim.phase.is_terminal() && !palw_weight_cap_applies_v1(params, claim))
        .map(|(_, claim)| claim.immature_contribution)
        .fold(0u128, u128::saturating_add);
    let index = PalwBondWeightIndexV1::of(params, state.claims_iter().map(|(_, claim)| claim));
    old_rule.saturating_add(index.capped_total(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ADR-0160 §4.2 and Appendix A, from the constants alone.
    #[test]
    fn the_cap_and_the_budget_are_the_adrs_numbers() {
        const MSK: u64 = 100_000_000;
        assert_eq!(PALW_CAPACITY_FCW_V1, 279 * 21_657_728 / 10, "FCW = ⌊0.1 × 279 × 21,657,728⌋");
        for (collateral_msk, fcw, budget) in
            [(6_499, 0, 0), (6_500, 1, 10_752_660), (13_000, 2, 21_505_320), (100_000, 15, 161_289_900), (1_000_000, 153, 1_645_156_980)]
        {
            assert_eq!(palw_bond_weight_cap_v1(collateral_msk * MSK), fcw * PALW_CAPACITY_FCW_V1, "{collateral_msk} MSK");
            assert_eq!(palw_bond_weight_budget_sompi_v1(collateral_msk * MSK), budget, "{collateral_msk} MSK");
        }
        // A genesis card (939,063 MSK) holds 144 FCW.
        assert_eq!(palw_bond_weight_cap_v1(939_063 * MSK) / PALW_CAPACITY_FCW_V1, 144);
        // The budget at 13k is 0.215 MSK: a 2M claim's reservation falls from 59,742.94 MSK to at most that.
        assert!(palw_bond_weight_budget_sompi_v1(13_000 * MSK) <= 21_505_320);
        assert_eq!(palw_capped_weight_reservation_v1(5_974_294_206_820, 21_505_320, 0), 21_505_320);
        assert_eq!(palw_capped_weight_reservation_v1(10_752_660, 21_505_320, 10_752_660), 10_752_660, "the second floor claim");
        assert_eq!(palw_capped_weight_reservation_v1(10_752_660, 21_505_320, 21_505_320), 0, "the third takes nothing");
        assert_eq!(palw_capped_weight_reservation_v1(10_752_660, 21_505_320, u128::MAX), 0, "never negative");
        // The ceiling is 8k's raw weight, 229.86 FCW.
        assert_eq!(PALW_CAPACITY_C7_WEIGHT_CEILING_V1 * 100 / PALW_CAPACITY_FCW_V1, 22_985);
    }
}
