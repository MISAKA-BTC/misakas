//! **RFC-0008 RFC08-2: the combined liability predicate, wired into admission and settlement** (`palw_settlement_liability_v1`;
//! dormant — it runs only past `palw_settlement_once_v1`, which is `None` on every preset; a child module of `palw_state_v2`).
//!
//! Every settlement leg reaches the predicate through [`TransitionBuilder::settle_once_liable_v1`] (the once-only registry's pay
//! point) and every budgeted claim through [`TransitionBuilder::admit_claim_liability_v1`] at its plan. The terms are GATHERED
//! here from authenticated state — never defaulted:
//!
//! | term | V2 claim (attempt, rider, free-prompt) | kernel claim |
//! |---|---|---|
//! | reserved / consumed R | the issuer income its R moved into (kind 0), else its reservation row | its fee income (kind 3), else its row |
//! | `K` | its ADR-0176 reservation row (`claim id`) | its row (`kernel_budget_claim_id_v1`) |
//! | collateral | the bond's posted collateral | same |
//! | other live liabilities | the bond row's open `Σ K` less this claim's, plus the kept-liability shadow | same |
//! | bond lock | the budget's withdrawal hold (open-ended while a claim is open, else `max(H_L hold, window) − 1`) | same |
//! | liability horizon | the panel liability record's `expiry − 1` (else `final + window_court − 1`), or the consumer's row | the kernel row's `liability_until` |
//! | hard deadline (X3) | `palw_x3_hard_deadline_at_v1` where X3 governs the claim's acceptance | the producer-signed paid lifetime, or acceptance-bound X3 under ADR-0198 |
//! | material TTL | a healthy event-relative monetary covenant (`u64::MAX`), else the claim's X3 retention obligation | a healthy event-relative covenant |
//! | retire | `retirement()` + the staged floor (claim present), else `final + claim_retirement_daa` (lower bound) | the kernel keeps the row through `liability_until` |
//! | export cap | `⌊K · export_cap_permille / 1000⌋` (PESG §6) | same |
//!
//! A leg with no liable claim behind it (a sponsor's prepaid funding, a provider's default charge, a reporter's slash-funded
//! award) is not subject to the predicate, by name. A leg whose payee's terms have no authenticated source at all (a native
//! mining receipt, or an ordinary verifier fee or material provider fee with no RFC-0039 signed finite consent) is REFUSED, by
//! name. Those two fee legs read the signed consent: the ordinary verifier's `liability_until` / `material_until`, and the
//! provider fee-only row's enrollment horizon. A paid V2 claim's check fee past the trap fence is held and released under the verifier's own terms
//! ([`PalwLegLiabilityV1::VerifierCheck`]: the trap reveal window as its horizon, the held fee as its collateral). A claim accepted before the fence keeps its
//! old ruleset (as X3, N4 and D2 do): the predicate binds what was admitted under it.
//!
//! **No credit.** Nothing here grants credit or weight (RFC-0018 Gate C stays the only credit predicate); no computation proof
//! (PRINCIPLES §8).
use super::palw_settlement_once_fold_v1::PalwSettleOnceV1;
use super::*;
use crate::palw_settlement_liability_v1::*;
use crate::palw_settlement_once_v1::*;

/// **Where one settlement leg's liability terms are read from** (the consumer names it; the fold reads every value itself).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwLegLiabilityV1 {
    /// A leg of a V2 claim (or the V2 root of an EXEC leg). `final_daa`: the claim's Final, where the consumer knows it;
    /// `horizon`: an authenticated row's own last liable DAA that supersedes the panel record (an EXEC reward row, a vesting row);
    /// `draw`: this leg's NEW draw on the claim's R (0 for a release of a reward drawn at Final); `exported_before`: what the
    /// claim already exported at once (`None`: its consumed R — an over-count, never an under-count).
    V2Claim {
        claim: Hash64,
        final_daa: Option<u64>,
        horizon: Option<u64>,
        draw: u64,
        paid_now: u64,
        exported_before: Option<u64>,
        vest_until: Option<u64>,
    },
    /// A leg of a kernel claim (its escrow reward: its R draw is the payment).
    KernelClaim { claim: [u8; 64], draw: u64, paid_now: u64, vest_until: Option<u64> },
    /// **RFC-0007 §4.1 T6 (`palw_paid_claim_trap_v1`): a drawn verifier's held check fee on a paid V2 claim**, released after
    /// `horizon` (the claim's trap reveal window end, the row's own `release_after_daa`). The verifier's liability on this leg is
    /// what the forfeiture can take of the held `fee`, and the held row itself is its collateral, locked through the horizon by
    /// construction; the hard deadline is the producer-signed paid lifetime; the signer liability beyond the fee is its own leg.
    VerifierCheck { claim: [u8; 64], fee: u64, horizon: u64 },
    /// **G14 lane escrow-rfc08 (RFC-0008, RFC08-2): a leg of a native claim** (funded / text / job / mining) whose own budget row
    /// is `budget_id`. Its signed hard deadline is DERIVED from signed/consensus inputs — the original's actual accepting DAA under
    /// the adopted X3 hard deadline (`palw_claim_hard_deadline_v1`, RFC-0014 2026-10-19 amendment `a + 10,000`, RFC-0026 §2) —
    /// never defaulted: where X3 does not govern `accepted_daa` the leg stays unsourced, by name. `liability_until` is the claim's
    /// own deterministic horizon (the family computes it from the claim record and its rooted policy).
    ///
    /// G14 r5 (user decision 2026-10-22): NOT used for mining39 receipt quanta (they are issuance + weight with no requester fee and
    /// are not held to `H`; `palw_native_mining_fold_v39::native_mining_receipt_leg_v39`). Kept for a native family whose leg pays a
    /// requester-funded fee.
    #[allow(dead_code)]
    NativeClaim { budget_id: Hash64, accepted_daa: u64, liability_until: u64, draw: u64, paid_now: u64 },
    /// A payee whose liability terms have no authenticated source: the leg is refused past the fence.
    Unsourced(&'static str),
    /// A leg that pays no liable claim's party (a debit into the books, a slash-funded award): outside the predicate, by name.
    NoLiableClaim(&'static str),
}

/// What admission knows about a new budgeted claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PalwAdmissionLiabilityV1 {
    /// A V2 attempt, rider or free-prompt claim (X3 defines its hard deadline from its acceptance).
    V2Claim,
    /// A kernel claim (its producer-signed paid lifetime and its covenant are read by the claim's digest).
    KernelClaim { claim: [u8; 64] },
    /// **A native funded / text / job / mining claim** (ADR-0184 §1, user decision 2026-10-18): its liability horizon is the Final
    /// floor + `window_court`, derived from the producer-signed commit inputs; `hard` is the latest it can be — the family's signed
    /// deadline (which bounds the Final floor) + `window_court`. No covenant: the material TTL is not a term of these families.
    NativeClaim { hard: u64 },
}

/// **RFC08 / ADR-0184 §1: the native claim's maximum liability horizon at admission** — the producer-signed deadline that bounds the
/// claim's Final floor, plus `window_court` (the formula every other lane uses after Final). Deterministic from signed inputs.
pub(super) fn native_claim_hard_deadline_v1(params: &PalwStateParamsV2, signed_deadline: u64) -> u64 {
    signed_deadline.saturating_add(params.window_court())
}

/// **G14 lane escrow-rfc08 (RFC-0008, RFC08-2): a native claim's signed hard deadline, derived** — the adopted X3 hard deadline
/// (`palw_claim_hard_deadline_v1`'s policy, `a + prepare + observe + resolve` = RFC-0014's `a + 10,000`) at the claim's actual
/// accepting DAA `a`, read from the rooted mirror. `Err` (never a default) where X3 does not govern `a`: the fence is unarmed or
/// `a` precedes it. Checked: an overflowing deadline is refused, never saturated.
pub(super) fn native_claim_signed_hard_deadline_v1(params: &PalwStateParamsV2, accepted_daa: u64) -> PalwSettlementTermV1<u64> {
    let mirror = params
        .claim_hard_deadline_v1()
        .filter(|mirror| accepted_daa >= mirror.from_daa)
        .ok_or("the signed hard deadline: X3 (palw_claim_hard_deadline_v1) does not govern the native claim's acceptance")?;
    let p = &mirror.policy;
    accepted_daa
        .checked_add(p.prepare_daa)
        .and_then(|d| d.checked_add(p.observe_daa))
        .and_then(|d| d.checked_add(p.resolve_daa))
        .ok_or("the signed hard deadline: overflow")
}

/// ADR-0198 applies the acceptance-bound X3 clock to ordinary kernel claims too. A paid V2
/// enrollment keeps its explicitly authenticated lifetime. Otherwise the signed commit is governed
/// by the X3 policy at its actual acceptance; an old cohort, missing policy or overflow stays refused.
/// This supplies only the hard deadline: reservation, collateral and actual material covenant remain
/// independent requirements of the combined predicate.
pub(super) fn kernel_claim_signed_hard_deadline_v1(
    params: &PalwStateParamsV2,
    accepted_daa: u64,
    paid_lifetime_until: Option<u64>,
) -> PalwSettlementTermV1<u64> {
    if let Some(until) = paid_lifetime_until {
        return Ok(until);
    }
    let mirror = params
        .claim_hard_deadline_v1()
        .filter(|mirror| accepted_daa >= mirror.from_daa)
        .ok_or("the signed hard deadline: neither a paid lifetime nor X3 governs the kernel claim's acceptance")?;
    accepted_daa
        .checked_add(mirror.policy.prepare_daa)
        .and_then(|d| d.checked_add(mirror.policy.observe_daa))
        .and_then(|d| d.checked_add(mirror.policy.resolve_daa))
        .ok_or("the kernel signed hard deadline: overflow")
}

/// Refusal as a semantic consensus error (never a local capacity error).
pub(super) fn liability_refused_v1(why: PalwSettlementLiabilityRefusalV1) -> PalwStateV2Error {
    PalwStateV2Error::BondBudgetRefused(format!("RFC-0008 combined liability predicate: {}", why.name()))
}

impl TransitionBuilder<'_> {
    fn liability_fence_from_v1(&self, now: u64) -> Option<u64> {
        self.params.settlement_once_active_at(now).then(|| self.params.settlement_once_from()).flatten()
    }

    /// The bond-side terms every family shares, from the bond's actual rows: collateral, the other live liabilities (this claim's
    /// own `K` taken out only while its row is open — a kept shadow is counted in full) and the withdrawal-hold horizon.
    fn bond_liability_terms_v1(
        &self,
        bond: &PalwBondKeyV2,
        own_open_liability: u128,
    ) -> (PalwSettlementTermV1<u128>, PalwSettlementTermV1<u128>, PalwSettlementTermV1<u64>) {
        let collateral = self.state.bonds.get(bond).map(|b| u128::from(b.collateral)).ok_or("the claim's bond is not registered");
        let Some(budget) = self.state.bond_budget.as_ref() else {
            // No engine yet: no other liability is recorded, and nothing holds the bond beyond its status.
            return (collateral, Ok(0), Err("the ADR-0176 withdrawal hold (no bond budget engine)"));
        };
        let row = budget.bond_row(bond);
        let other = row
            .map_or(0, |r| r.open_liability_sompi)
            .saturating_sub(own_open_liability)
            .saturating_add(budget.kept_liability_v1(bond));
        let open_ended = row.is_some_and(|r| r.open_claims > 0) || budget.window_holds(bond, u64::MAX);
        let locked = if open_ended {
            u64::MAX
        } else {
            let hold = row.map_or(0, |r| r.liability_hold_until);
            let window = row.filter(|r| !r.window.is_zero()).map_or(0, |r| r.latest_release_daa);
            hold.max(window).saturating_sub(1)
        };
        (collateral, Ok(other), Ok(locked))
    }

    /// **The claim's R book**: `(reserved, consumed)` of the native income row its reservation was moved into at acceptance (an
    /// attempt's issuer income, kind 0; a kernel claim's fee income, kind 3 — `hold_native_income_v1` zeroes the claim row's R),
    /// else of its own reservation row (a free-prompt commitment draws per spend from it).
    fn reward_book_v1(&self, row: &crate::palw_bond_budget_v1::PalwBudgetClaimRowV1, root: Hash64, kind: u8) -> (u64, u64) {
        let income = super::palw_bond_budget_fold_v1::native_income_id_v1(root, &row.bond, kind);
        match self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&income)) {
            Some(held) => (held.reserved.reward_sompi, held.consumed.reward_sompi),
            None => (row.reserved.reward_sompi, row.consumed.reward_sompi),
        }
    }

    /// **What a V2 claim's Final can draw on its R book** — exactly `bond_budget_final_reward_v1`'s grant: the escrow within the
    /// issuer income held for it, else within the reservation row's remaining R; the escrow whole for an unbudgeted claim.
    pub(super) fn issuance_draw_v1(&self, claim: &Hash64, escrow: u64) -> u64 {
        let Some(budget) = self.state.bond_budget.as_ref() else { return escrow };
        let Some(row) = budget.claim_row(claim) else { return escrow };
        if let Some(held) = budget.claim_row(&super::palw_bond_budget_fold_v1::native_income_id_v1(*claim, &row.bond, 0)) {
            return escrow.min(held.reserved.reward_sompi);
        }
        let left = row.reserved.reward_sompi.saturating_sub(row.consumed.reward_sompi);
        escrow.min(left)
    }

    pub(super) fn export_cap_of_v1(&self, liability: u128) -> PalwSettlementTermV1<u64> {
        self.params
            .bond_budget()
            .map(|mirror| crate::palw_bond_budget_v1::palw_bond_budget_export_cap_v1(&mirror.policy, liability))
            .ok_or("the PESG export cap (no bond budget policy)")
    }

    /// A healthy, event-relative signed monetary covenant over `root` (its providers serve until every right terminated and the
    /// last physical payment + W passed): owed through any horizon.
    fn covenant_ttl_v1(&self, root: Hash64) -> Option<u64> {
        self.state
            .monetary_material_row_v1(root)
            .filter(|row| row.terms.payment_relative_until_finality && row.healthy(&self.state))
            .map(|_| u64::MAX)
    }

    /// **The settlement terms of one leg**, or `None` where the predicate does not bind (below the fence, a pre-fence claim, a
    /// leg that pays no liable claim). `Err` names an unsourced term.
    pub(super) fn leg_liability_terms_v1(
        &self,
        now: u64,
        leg: &PalwLegLiabilityV1,
        payment: u64,
    ) -> Result<Option<PalwSettlementTermsSourcedV1>, PalwSettlementLiabilityRefusalV1> {
        let Some(from) = self.liability_fence_from_v1(now) else { return Ok(None) };
        let unsourced = PalwSettlementLiabilityRefusalV1::Unsourced;
        match *leg {
            PalwLegLiabilityV1::NoLiableClaim(_) => Ok(None),
            PalwLegLiabilityV1::Unsourced(why) => Err(unsourced(why)),
            PalwLegLiabilityV1::V2Claim { claim, final_daa, horizon, draw, paid_now, exported_before, vest_until } => {
                // The ruleset is the claim's acceptance's (the claim record, else its reservation row once it retired).
                let accepted = self
                    .state
                    .claims
                    .get(&claim)
                    .map(|c| c.accepted_daa)
                    .or_else(|| self.state.bond_budget.as_ref().and_then(|b| b.claim_row(&claim)).map(|row| row.accepted_daa))
                    .ok_or(unsourced("the claim's acceptance (no claim record, no reservation row)"))?;
                if accepted < from {
                    return Ok(None); // accepted under the old ruleset
                }
                let budget = self.state.bond_budget.as_ref().ok_or(unsourced("the claim's ADR-0176 reservation (no engine)"))?;
                let row = *budget.claim_row(&claim).ok_or(unsourced("the claim's ADR-0176 reservation row"))?;
                if row.origin == crate::palw_bond_budget_v1::PalwBudgetOriginV1::Legacy {
                    return Ok(None); // a seeded pre-budget claim keeps the rule it was accepted under
                }
                let own = if row.open { row.liability_sompi } else { 0 };
                let (bond_collateral, other_live_liability, bond_locked_until) = self.bond_liability_terms_v1(&row.bond, own);
                let liability_until = horizon
                    .or_else(|| self.state.panel_liabilities.get(&claim).map(|r| r.expiry_daa.saturating_sub(1)))
                    .or_else(|| {
                        final_daa.map(|f| {
                            crate::palw_panel_var_v1::palw_panel_liability_expiry_v1(f, self.params.window_court()).saturating_sub(1)
                        })
                    })
                    .ok_or("the claim's post-Final liability horizon (no Final, no liability record)");
                let hard = self
                    .params
                    .claim_hard_deadline_v1()
                    .filter(|mirror| row.accepted_daa >= mirror.from_daa)
                    .map(|mirror| crate::palw_ruling_fences_v1::palw_x3_hard_deadline_at_v1(&mirror.policy, row.accepted_daa))
                    .ok_or("the signed hard deadline: X3 (palw_claim_hard_deadline_v1) does not govern the claim");
                let claim_state = self.state.claims.get(&claim);
                let material_ttl_until = match self.covenant_ttl_v1(claim) {
                    Some(ttl) => Ok(ttl),
                    None => match (claim_state, hard) {
                        (Some(c), _) if crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(self.params, c) => {
                            Ok(crate::palw_ruling_fences_v1::palw_claim_retention_obligation_v1(self.params, c))
                        }
                        // Retired: REQ-X3-6 raised its retention to at least the hard deadline when it was created.
                        (None, Ok(hard)) => Ok(hard),
                        _ => Err("the claim's material TTL (no covenant, X3 retention unknown)"),
                    },
                };
                let retire_not_before = if self.params.claim_retirement_daa == 0 {
                    Ok(u64::MAX)
                } else {
                    match (claim_state, final_daa) {
                        (Some(c), Some(f)) => {
                            let ordinary = f.saturating_add(self.params.claim_retirement_daa);
                            let at = palw_exec_v2_rewards_v1::retirement(&self.state, self.params, &claim, Some(ordinary), c)
                                .map_err(|_| unsourced("the claim's retirement (overflow)"))?
                                .unwrap_or(ordinary);
                            Ok(self.state.staged_retirement_floor_v1(&claim, at))
                        }
                        // A retired or not-yet-Final claim: the ordinary retirement is a lower bound (delays only move it later).
                        (_, Some(f)) => Ok(f.saturating_add(self.params.claim_retirement_daa)),
                        _ => Err("the claim's retirement (no Final DAA)"),
                    }
                };
                let (reserved, consumed) = self.reward_book_v1(&row, claim, 0);
                Ok(Some(PalwSettlementTermsSourcedV1 {
                    reserved_reward: Ok(reserved),
                    consumed_reward: Ok(consumed),
                    draw,
                    payment,
                    paid_now,
                    exported_before: Ok(exported_before.unwrap_or(consumed)),
                    export_cap: self.export_cap_of_v1(row.liability_sompi),
                    liability_sompi: Ok(row.liability_sompi),
                    bond_collateral,
                    other_live_liability,
                    bond_locked_until,
                    liability_until,
                    signed_hard_deadline: hard,
                    material_ttl_until,
                    vest_until,
                    retire_not_before,
                    now,
                }))
            }
            PalwLegLiabilityV1::NativeClaim { budget_id, accepted_daa, liability_until, draw, paid_now } => {
                if accepted_daa < from {
                    return Ok(None); // admitted under the old ruleset
                }
                let hard = native_claim_signed_hard_deadline_v1(self.params, accepted_daa);
                let budget = self.state.bond_budget.as_ref().ok_or(unsourced("the native claim's ADR-0176 reservation (no engine)"))?;
                let row = *budget.claim_row(&budget_id).ok_or(unsourced("the native claim's ADR-0176 reservation row"))?;
                let own = if row.open { row.liability_sompi } else { 0 };
                let (bond_collateral, other_live_liability, bond_locked_until) = self.bond_liability_terms_v1(&row.bond, own);
                Ok(Some(PalwSettlementTermsSourcedV1 {
                    reserved_reward: Ok(row.reserved.reward_sompi),
                    consumed_reward: Ok(row.consumed.reward_sompi),
                    draw,
                    payment,
                    paid_now,
                    exported_before: Ok(row.consumed.reward_sompi),
                    export_cap: self.export_cap_of_v1(row.liability_sompi),
                    liability_sompi: Ok(row.liability_sompi),
                    bond_collateral,
                    other_live_liability,
                    bond_locked_until,
                    liability_until: Ok(liability_until),
                    signed_hard_deadline: hard,
                    // No covenant: the material TTL is not a term of the native families (ADR-0184 §1, as at admission).
                    material_ttl_until: Ok(u64::MAX),
                    vest_until: None,
                    // The native kernel keeps the claim's row convictable through its own horizon (its retention rule).
                    retire_not_before: Ok(liability_until),
                    now,
                }))
            }
            PalwLegLiabilityV1::KernelClaim { claim, draw, paid_now, vest_until } => {
                let route = self.state.kernel_route.as_ref().ok_or(unsourced("the kernel route"))?;
                let kernel = route.kernel_claim_row_v1(&Hash64::from_bytes(claim)).ok_or(unsourced("the kernel claim row"))?;
                if kernel.committed_daa < from {
                    return Ok(None); // committed under the old ruleset
                }
                let budget = self.state.bond_budget.as_ref().ok_or(unsourced("the claim's ADR-0176 reservation (no engine)"))?;
                let id = super::palw_bond_budget_fold_v1::kernel_budget_claim_id_v1(&claim);
                let row = *budget.claim_row(&id).ok_or(unsourced("the kernel claim's ADR-0176 reservation row"))?;
                let own = if row.open { row.liability_sompi } else { 0 };
                let (bond_collateral, other_live_liability, bond_locked_until) = self.bond_liability_terms_v1(&row.bond, own);
                let liability_until = kernel.liability_until.ok_or("the kernel claim's liability horizon (not Final)");
                let hard = kernel_claim_signed_hard_deadline_v1(
                    self.params,
                    kernel.committed_daa,
                    route.paid_claim_signed_lifetime_v2(&claim).map(|lifetime| lifetime.liability_until),
                );
                let (reserved, consumed) = self.reward_book_v1(&row, Hash64::from_bytes(claim), 3);
                Ok(Some(PalwSettlementTermsSourcedV1 {
                    reserved_reward: Ok(reserved),
                    consumed_reward: Ok(consumed),
                    draw,
                    payment,
                    paid_now,
                    // The escrow reward is the kernel claim's one at-once payment (its marker makes it the only one).
                    exported_before: Ok(0),
                    export_cap: self.export_cap_of_v1(row.liability_sompi),
                    liability_sompi: Ok(row.liability_sompi),
                    bond_collateral,
                    other_live_liability,
                    bond_locked_until,
                    liability_until,
                    signed_hard_deadline: hard,
                    material_ttl_until: self
                        .covenant_ttl_v1(Hash64::from_bytes(claim))
                        .ok_or("the material TTL: no healthy event-relative monetary covenant for the kernel claim"),
                    vest_until,
                    // The kernel keeps a claim's row convictable through `liability_until` (its own retention rule).
                    retire_not_before: liability_until,
                    now,
                }))
            }
            PalwLegLiabilityV1::VerifierCheck { claim, fee, horizon } => {
                let route = self.state.kernel_route.as_ref().ok_or(unsourced("the kernel route"))?;
                let kernel = route.kernel_claim_row_v1(&Hash64::from_bytes(claim)).ok_or(unsourced("the kernel claim row"))?;
                if kernel.committed_daa < from {
                    return Ok(None); // committed under the old ruleset
                }
                if !self.params.paid_claim_trap_active_at(now) {
                    return Err(unsourced("a verifier's liability horizon: palw_paid_claim_trap_v1 is not in force"));
                }
                let lifetime = route
                    .paid_claim_signed_lifetime_v2(&claim)
                    .ok_or(unsourced("a verifier's liability horizon: the claim has no producer-signed paid lifetime"))?;
                let (forfeitable, _) = crate::palw_paid_claim_trap_v1::palw_paid_trap_forfeit_v1(fee);
                let liability = u128::from(forfeitable);
                // A convicted claim's verdict already landed (the horizon stands); a Final one can be convicted no later than its
                // own row's horizon, which the kernel keeps the row convictable through.
                let retained = if kernel.convicted {
                    Ok(u64::MAX)
                } else {
                    kernel.liability_until.ok_or("the paid claim's retention (neither convicted nor Final)")
                };
                let liability_until = retained.map(|kept| kept.min(horizon));
                Ok(Some(PalwSettlementTermsSourcedV1 {
                    // The fee was the poster's per-slot escrow, debited at the attestation: the release draws nothing more.
                    reserved_reward: Ok(fee),
                    consumed_reward: Ok(0),
                    draw: 0,
                    payment,
                    paid_now: payment,
                    exported_before: Ok(0),
                    export_cap: self.export_cap_of_v1(liability),
                    liability_sompi: Ok(liability),
                    // The held row is the verifier's collateral for this leg: it holds the whole fee until the horizon has passed.
                    bond_collateral: Ok(u128::from(fee)),
                    other_live_liability: Ok(0),
                    bond_locked_until: Ok(horizon),
                    liability_until,
                    signed_hard_deadline: Ok(lifetime.liability_until),
                    material_ttl_until: self
                        .covenant_ttl_v1(Hash64::from_bytes(claim))
                        .ok_or("the material TTL: no healthy event-relative monetary covenant for the paid claim"),
                    vest_until: Some(horizon.saturating_add(1)),
                    retire_not_before: retained,
                    now,
                }))
            }
        }
    }

    /// **The predicate at a settlement leg**: `Ok(None)` when it admits or does not bind, `Ok(Some(refusal))` when it refuses.
    pub(super) fn leg_liability_refusal_v1(
        &self,
        now: u64,
        leg: &PalwLegLiabilityV1,
        payment: u64,
    ) -> Option<PalwSettlementLiabilityRefusalV1> {
        match self.leg_liability_terms_v1(now, leg, payment) {
            Ok(None) => None,
            Ok(Some(terms)) => palw_settlement_liability_sourced_v1(&terms).err(),
            Err(refusal) => Some(refusal),
        }
    }

    /// **Settle once, under the combined predicate** (RFC08-1 + RFC08-2). Below the fence: `Dormant`, nothing read. A repeat:
    /// `AlreadySettled` (the predicate is not re-run for a closed obligation). Otherwise the predicate runs BEFORE any value moves:
    /// refused, the obligation is closed with a zero-amount marker (the journal records the refusal; nothing is paid, now or by
    /// another consumer) and `Refused` is returned; admitted, the marker is written as by
    /// [`TransitionBuilder::settle_once_v1`] and `First` is returned.
    pub(super) fn settle_once_liable_v1(
        &mut self,
        now: u64,
        obligation: PalwSettlementObligationV1,
        consumer: PalwSettlementConsumerV1,
        amount: u64,
        debit: PalwSettlementDebitV1,
        leg: PalwLegLiabilityV1,
    ) -> Result<PalwSettleOnceV1, PalwStateV2Error> {
        if !self.params.settlement_once_active_at(now) {
            return Ok(PalwSettleOnceV1::Dormant);
        }
        if let Some(row) = self.settled_row_checked_v1(&obligation)? {
            return Ok(PalwSettleOnceV1::AlreadySettled(row));
        }
        if let Some(refusal) = self.leg_liability_refusal_v1(now, &leg, amount) {
            self.settle_once_v1(now, obligation, consumer, 0, debit)?;
            return Ok(PalwSettleOnceV1::Refused(refusal));
        }
        self.settle_once_v1(now, obligation, consumer, amount, debit)
    }
}

impl TransitionBuilder<'_> {
    /// Source fees have their own producer-signed opt-in terms and prepaid sponsor promise.
    /// Read the already resource-reserved, checked PRE-transition ledger: its source deposit is
    /// still held when this tick releases it and pays the fulfilled promise. No caller supplies
    /// invented claim terms, and a missing actual source-material covenant remains a refusal.
    pub(super) fn beacon_source_fee_terms_v1(
        &self,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
        context: [u8; 64],
        producer: [u8; 64],
        now: u64,
        payment: u64,
    ) -> Result<Option<PalwSettlementTermsSourcedV1>, PalwSettlementLiabilityRefusalV1> {
        use PalwSettlementLiabilityRefusalV1::{Malformed, Unsourced};
        let Some(from) = self.liability_fence_from_v1(now) else { return Ok(None) };
        let funding = ledger.beacon_source_fees.get(&context).ok_or(Unsourced("the source fee's prepaid funding"))?;
        let promise = ledger.beacon_source_fee_promises.get(&(context, producer)).ok_or(Unsourced("the source fee's promise"))?;
        if promise.accepted_daa < from {
            return Ok(None); // The old signed opt-in keeps its original settlement rules.
        }
        let source = ledger.beacon_sources.get(&(context, producer)).ok_or(Unsourced("the producer's signed source opt-in"))?;
        let terms = &source.terms;
        if terms.validate().is_err()
            || source.producer != producer
            || terms.context != context
            || *terms != funding.terms.source
            || promise.terms_id != funding.terms.id()
            || promise.accepted_daa != source.opted_in_daa
            || promise.outcome.is_some()
            || source.outcome.is_some()
            || source.held != terms.deposit
            || promise.fee != payment
            || funding.held < payment
        {
            return Err(Malformed);
        }
        if now <= terms.hold_until {
            return Err(Unsourced("the source's signed liability horizon has not ended"));
        }
        let claim = source.revealed.ok_or(Unsourced("the source's revealed claim"))?.0;
        let claim_row = ledger.claims.get(&claim).ok_or(Unsourced("the source's retained claim"))?;
        if claim_row.producer != producer
            || claim_row.job_id != source.job
            || claim_row.class_binding_id != source.profile
            || claim_row.sealed_daa != source.sealed_daa
            || claim_row.convicted
            || !matches!(claim_row.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { final_daa } if final_daa <= terms.final_by)
            || claim_row.liability_until.is_none_or(|until| until < terms.hold_until)
        {
            return Err(Unsourced("the source's standing Final and retained liability horizon"));
        }
        let bond = ledger.bonds.get(&producer).ok_or(Unsourced("the source producer's bond"))?;
        if bond.reserved < source.held || bond.collateral < bond.reserved {
            return Err(PalwSettlementLiabilityRefusalV1::Uncollectible);
        }
        let deposit = u128::from(source.held);
        Ok(Some(PalwSettlementTermsSourcedV1 {
            reserved_reward: Ok(promise.fee),
            consumed_reward: Ok(0), // This exact promise is pending, with its fee still prepaid.
            draw: payment,
            payment,
            paid_now: payment,
            exported_before: Ok(0),
            export_cap: self.export_cap_of_v1(deposit),
            liability_sompi: Ok(deposit),
            // A distinct reservation tranche, not the bond's unreserved surplus. The complete
            // ledger's reservation sum was checked above before selecting this tranche.
            bond_collateral: Ok(deposit),
            other_live_liability: Ok(0),
            bond_locked_until: Ok(terms.hold_until),
            liability_until: Ok(terms.hold_until),
            signed_hard_deadline: Ok(terms.hold_until),
            material_ttl_until: self.covenant_ttl_v1(Hash64::from_bytes(claim))
                .ok_or("the source claim's actual healthy material covenant"),
            vest_until: None, // Only a payment after the entire signed hold is admitted.
            // Source rows are permanent consumed opt-ins; the retained claim is checked above.
            retire_not_before: Ok(terms.hold_until),
            now,
        }))
    }

    pub(super) fn settle_once_beacon_source_fee_v1(
        &mut self,
        now: u64,
        obligation: PalwSettlementObligationV1,
        consumer: PalwSettlementConsumerV1,
        amount: u64,
        debit: PalwSettlementDebitV1,
        ledger: &misaka_palw_kernel::ledger::KernelLedgerV1,
        context: [u8; 64],
        producer: [u8; 64],
    ) -> Result<PalwSettleOnceV1, PalwStateV2Error> {
        if !self.params.settlement_once_active_at(now) {
            return Ok(PalwSettleOnceV1::Dormant);
        }
        if let Some(row) = self.settled_row_checked_v1(&obligation)? {
            return Ok(PalwSettleOnceV1::AlreadySettled(row));
        }
        let refusal = match self.beacon_source_fee_terms_v1(ledger, context, producer, now, amount) {
            Ok(None) => None,
            Ok(Some(terms)) => palw_settlement_liability_sourced_v1(&terms).err(),
            Err(why) => Some(why),
        };
        if let Some(why) = refusal {
            self.settle_once_v1(now, obligation, consumer, 0, debit)?;
            return Ok(PalwSettleOnceV1::Refused(why));
        }
        self.settle_once_v1(now, obligation, consumer, amount, debit)
    }
}

impl TransitionBuilder<'_> {
    /// RFC-0039 / ADR-0207: one ordinary verifier slot's signed Clean consent. The hold is the
    /// tranche. Its signed `liability_until` and `material_until` are the finite horizon; the whole
    /// signed fee is payable once that horizon has passed. A missing hold is unsourced. Hashes
    /// authenticate the consent bytes only.
    pub(super) fn ordinary_verifier_fee_terms_v1(
        &self,
        claim: [u8; 64],
        verifier: [u8; 64],
        slot: u64,
        now: u64,
        payment: u64,
    ) -> Result<Option<PalwSettlementTermsSourcedV1>, PalwSettlementLiabilityRefusalV1> {
        use crate::palw_verifier_fee_consent_v1::{self as fee, HeldVerifierFeeV1};
        use PalwSettlementLiabilityRefusalV1::{Malformed, Unsourced};
        let Some(from) = self.liability_fence_from_v1(now) else { return Ok(None) };
        let key = fee::fee_key(fee::HELD, &claim, &verifier, slot);
        let Some(bytes) = self.state.kernel_route.as_ref().and_then(|route| route.aux.get(&(fee::TABLE, key.clone()))) else {
            return Err(Unsourced("the ordinary verifier fee's signed hold"));
        };
        let row: HeldVerifierFeeV1 = borsh::from_slice(bytes).map_err(|_| Malformed)?;
        if fee::fee_key(fee::HELD, &row.terms.claim, &row.terms.verifier, row.terms.slot) != key
            || row.terms.validate().is_err()
            || row.amount != row.terms.fee
            || (payment != 0 && payment != row.amount)
        {
            return Err(Malformed);
        }
        if let Some(committed) = self
            .state
            .kernel_route
            .as_ref()
            .and_then(|route| route.kernel_claim_row_v1(&Hash64::from_bytes(claim)))
            .map(|kernel| kernel.committed_daa)
        {
            if committed < from {
                return Ok(None);
            }
        }
        let moving = if payment == 0 { 0 } else { row.amount };
        Ok(Some(self.signed_fee_tranche_v1(row.amount, moving, row.terms.liability_until, row.terms.material_until, now)))
    }

    /// Admission of a signed ordinary consent, before the poster escrow is debited. The healthy
    /// payment-relative covenant must still cover the signed material deadline. Release does not
    /// repeat that check: a later verdict does not extend or cancel the signed fee horizon.
    pub(super) fn admit_ordinary_verifier_fee_v1(
        &self,
        terms: &crate::palw_verifier_fee_consent_v1::VerifierFeeConsentV1,
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.liability_fence_from_v1(now).is_none() {
            return Ok(());
        }
        terms.validate().map_err(|why| liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(why)))?;
        let route = self.state.kernel_route.as_ref().ok_or_else(|| {
            liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced("the ordinary verifier fee's route header"))
        })?;
        if terms.network != route.header.policy.network_domain || terms.ruleset != route.header.policy.ruleset_digest {
            return Err(liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(
                "the ordinary verifier fee's signed network or ruleset",
            )));
        }
        let ttl = self.covenant_ttl_v1(Hash64::from_bytes(terms.claim)).ok_or_else(|| {
            liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(
                "the ordinary verifier fee's healthy material covenant",
            ))
        })?;
        if ttl < terms.material_until {
            return Err(liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(
                "the ordinary verifier fee's material covenant ends before the signed material deadline",
            )));
        }
        let sourced = self.signed_fee_tranche_v1(terms.fee, terms.fee, terms.liability_until, terms.material_until, now);
        palw_settlement_liability_sourced_v1(&sourced).map_err(liability_refused_v1)
    }

    /// RFC-0039 §6: the provider's prepaid fee is its own signed tranche. `H` is the fee row's
    /// horizon (the original `enrollment_until_daa` both parties signed). Collateral is that held
    /// fee, not the provider's separate service bond. A missing row is unsourced.
    pub(super) fn material_provider_fee_terms_v1(
        &self,
        context: Hash64,
        provider: PalwBondKeyV2,
        now: u64,
        payment: u64,
    ) -> Result<Option<PalwSettlementTermsSourcedV1>, PalwSettlementLiabilityRefusalV1> {
        use crate::palw_monetary_material_v1::{PalwMaterialFeeStatusV2, fee_v2::VERSION};
        use PalwSettlementLiabilityRefusalV1::{Malformed, Unsourced};
        let Some(from) = self.liability_fence_from_v1(now) else { return Ok(None) };
        let fee = match self.state.material_fee_row_checked_v2(context, provider) {
            Ok(Some(fee)) => fee,
            Ok(None) => return Err(Unsourced("the material provider fee's signed finite hold")),
            Err(_) => return Err(Malformed),
        };
        if fee.accepted_daa < from {
            return Ok(None);
        }
        let route = self
            .state
            .kernel_route
            .as_ref()
            .ok_or(Unsourced("the material provider fee's route header"))?;
        if fee.version != VERSION
            || fee.ruleset == Hash64::default()
            || fee.ruleset.as_bytes() != route.header.policy.ruleset_digest
            || fee.horizon <= fee.accepted_daa
            || fee.horizon == u64::MAX
            || !matches!(fee.status, PalwMaterialFeeStatusV2::Held)
            || (payment != 0 && payment != fee.amount)
        {
            return Err(Malformed);
        }
        let moving = if payment == 0 { 0 } else { fee.amount };
        // Fee evidence is retained through the signed H. The V1 service covenant is a separate
        // lifetime; admission checks that it covers H, and settlement does not lengthen it.
        Ok(Some(self.signed_fee_tranche_v1(fee.amount, moving, fee.horizon, fee.horizon, now)))
    }

    /// Before the sponsor debit sticks: acceptance is strictly before H, and the original healthy
    /// payment-relative covenant still covers that signed fee horizon.
    pub(super) fn admit_material_provider_fee_v1(
        &self,
        context: Hash64,
        provider: PalwBondKeyV2,
        now: u64,
    ) -> Result<(), PalwStateV2Error> {
        if self.liability_fence_from_v1(now).is_none() {
            return Ok(());
        }
        let fee = self.state.material_fee_row_checked_v2(context, provider).map_err(|_| {
            liability_refused_v1(PalwSettlementLiabilityRefusalV1::Malformed)
        })?;
        let fee = fee.ok_or_else(|| {
            liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(
                "the material provider fee's signed finite hold",
            ))
        })?;
        if fee.accepted_daa < self.liability_fence_from_v1(now).unwrap_or(0) {
            return Ok(());
        }
        let ttl = self.covenant_ttl_v1(fee.root).ok_or_else(|| {
            liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(
                "the material provider fee's healthy covenant through its signed horizon",
            ))
        })?;
        if ttl < fee.horizon {
            return Err(liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(
                "the material provider fee's healthy covenant ends before the signed horizon",
            )));
        }
        match self.material_provider_fee_terms_v1(context, provider, now, fee.amount).map_err(liability_refused_v1)? {
            None => Ok(()),
            Some(terms) => palw_settlement_liability_sourced_v1(&terms).map_err(liability_refused_v1),
        }
    }

    /// The signed fee is the whole tranche: R, K, collateral, lock, hard deadline and retention.
    /// No separate forfeiture rate. Inside H the at-once amount is that same signed fee, which is
    /// what a timely refund moves; after H the predicate allows the same whole fee.
    fn signed_fee_tranche_v1(
        &self,
        amount: u64,
        moving: u64,
        horizon: u64,
        material_until: u64,
        now: u64,
    ) -> PalwSettlementTermsSourcedV1 {
        PalwSettlementTermsSourcedV1 {
            reserved_reward: Ok(amount),
            consumed_reward: Ok(0),
            draw: 0,
            payment: moving,
            paid_now: moving,
            exported_before: Ok(0),
            export_cap: Ok(amount),
            liability_sompi: Ok(u128::from(amount)),
            bond_collateral: Ok(u128::from(amount)),
            other_live_liability: Ok(0),
            bond_locked_until: Ok(horizon),
            liability_until: Ok(horizon),
            signed_hard_deadline: Ok(horizon),
            material_ttl_until: Ok(material_until),
            vest_until: horizon.checked_add(1),
            retire_not_before: Ok(horizon),
            now,
        }
    }
}

impl TransitionBuilder<'_> {
    /// **The predicate at admission** (a claim's budget plan, before any write): the claim's reserved R, its `K` against the bond's
    /// collateral and other live liabilities, and its maximum legal horizon — the hard deadline — against the bond's hold, the
    /// material TTL and the retirement floor. A refusal refuses the claim (as a plan refusal does). A no-op below the fence.
    pub(super) fn admit_claim_liability_v1(
        &self,
        now: u64,
        plan: &super::palw_bond_budget_fold_v1::PalwBudgetPlanV1,
        family: PalwAdmissionLiabilityV1,
    ) -> Result<(), PalwStateV2Error> {
        if self.liability_fence_from_v1(now).is_none() {
            return Ok(());
        }
        let (bond, capital, accepted_daa, liability, reserved) = plan.liability_terms_v1();
        let (_, other_live_liability, _) = self.bond_liability_terms_v1(&bond, 0);
        let (hard, material_ttl_until, retire_not_before): (PalwSettlementTermV1<u64>, PalwSettlementTermV1<u64>, PalwSettlementTermV1<u64>) =
            match family {
                PalwAdmissionLiabilityV1::V2Claim => {
                    let hard = self
                        .params
                        .claim_hard_deadline_v1()
                        .filter(|mirror| accepted_daa >= mirror.from_daa)
                        .map(|mirror| crate::palw_ruling_fences_v1::palw_x3_hard_deadline_at_v1(&mirror.policy, accepted_daa))
                        .ok_or("the signed hard deadline: X3 (palw_claim_hard_deadline_v1) does not govern a claim accepted now");
                    // REQ-X3-6: the claim writer raises the new claim's retention to at least the hard deadline.
                    let retire = if self.params.claim_retirement_daa == 0 {
                        Ok(u64::MAX)
                    } else {
                        // Terminal is no earlier than acceptance: a lower bound.
                        Ok(accepted_daa.saturating_add(self.params.claim_retirement_daa))
                    };
                    (hard, hard, retire)
                }
                PalwAdmissionLiabilityV1::KernelClaim { claim } => {
                    let route = self.state.kernel_route.as_ref();
                    // The paid commit fold applies the kernel commit before it writes the authority row, so the signed lifetime it
                    // validated rides the builder for that one call.
                    let paid_lifetime_until = route
                        .and_then(|r| r.paid_claim_signed_lifetime_v2(&claim))
                        .map(|lifetime| lifetime.liability_until)
                        .or(self.paid_lifetime_admission_v1.filter(|(base, _)| *base == claim).map(|(_, until)| until));
                    let hard = kernel_claim_signed_hard_deadline_v1(self.params, accepted_daa, paid_lifetime_until);
                    let ttl = self
                        .covenant_ttl_v1(Hash64::from_bytes(claim))
                        .ok_or("the material TTL: no healthy event-relative monetary covenant for the kernel claim");
                    (hard, ttl, hard)
                }
                PalwAdmissionLiabilityV1::NativeClaim { hard } => (Ok(hard), Ok(u64::MAX), Ok(hard)),
            };
        let terms = PalwSettlementTermsSourcedV1 {
            reserved_reward: Ok(reserved),
            consumed_reward: Ok(0),
            draw: 0,
            payment: 0,
            paid_now: 0,
            exported_before: Ok(0),
            export_cap: self.export_cap_of_v1(liability),
            liability_sompi: Ok(liability),
            bond_collateral: Ok(u128::from(capital)),
            other_live_liability,
            // The reservation itself opens a claim, which holds the bond until it is terminal and then for `H_L`.
            bond_locked_until: Ok(u64::MAX),
            liability_until: hard,
            signed_hard_deadline: hard,
            material_ttl_until,
            vest_until: None,
            retire_not_before,
            now,
        };
        palw_settlement_liability_sourced_v1(&terms).map_err(liability_refused_v1)
    }
}

impl TransitionBuilder<'_> {
    /// **Admission of a native funded / text / job / mining claim past the fence** (ADR-0184 §1, user decision 2026-10-18): the
    /// predicate with the derived horizon (`native_claim_hard_deadline_v1`) — was refused here as unsourced. `plan`: the claim's
    /// budget plan (`None` = unbudgeted, where the predicate has no reservation to read and does not bind). A no-op below the fence.
    pub(super) fn admit_native_claim_liability_v1(
        &self,
        now: u64,
        plan: Option<&super::palw_bond_budget_fold_v1::PalwBudgetPlanV1>,
        signed_deadline: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(plan) = plan else { return Ok(()) };
        let hard = native_claim_hard_deadline_v1(self.params, signed_deadline);
        self.admit_claim_liability_v1(now, plan, PalwAdmissionLiabilityV1::NativeClaim { hard })
    }
}
