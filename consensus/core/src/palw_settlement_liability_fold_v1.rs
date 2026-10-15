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
//! | hard deadline (X3) | `palw_x3_hard_deadline_at_v1` where X3 governs the claim's acceptance | the producer-signed paid-claim lifetime |
//! | material TTL | a healthy event-relative monetary covenant (`u64::MAX`), else the claim's X3 retention obligation | a healthy event-relative covenant |
//! | retire | `retirement()` + the staged floor (claim present), else `final + claim_retirement_daa` (lower bound) | the kernel keeps the row through `liability_until` |
//! | export cap | `⌊K · export_cap_permille / 1000⌋` (PESG §6) | same |
//!
//! A leg with no liable claim behind it (a sponsor's prepaid funding, a provider's default charge, a reporter's slash-funded
//! award) is not subject to the predicate, by name. A leg whose payee's terms have no authenticated source at all (a verifier's
//! check fee outside a paid V2 claim past `palw_paid_claim_trap_v1`, a material provider's fee, a native mining receipt) is
//! REFUSED, by name. A paid V2 claim's check fee past the trap fence is held and released under the verifier's own terms
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

    fn export_cap_of_v1(&self, liability: u128) -> PalwSettlementTermV1<u64> {
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
                let hard = route
                    .paid_claim_signed_lifetime_v2(&claim)
                    .map(|lifetime| lifetime.liability_until)
                    .ok_or("the signed hard deadline: the kernel claim has no producer-signed paid lifetime");
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
        // RFC-0006: the marker lookup's owned 64-byte key, admitted before it exists.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("settlement-once marker lookup key");
            q.exact::<u8>(64)?;
            self.admit_native_write_v1(q)?;
        }
        if let Some(row) = super::palw_settlement_once_fold_v1::settled_row_v1(&self.state, &obligation)? {
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
                    let hard = route
                        .and_then(|r| r.paid_claim_signed_lifetime_v2(&claim))
                        .map(|lifetime| lifetime.liability_until)
                        .or(self.paid_lifetime_admission_v1.filter(|(base, _)| *base == claim).map(|(_, until)| until))
                        .ok_or("the signed hard deadline: the kernel claim has no producer-signed paid lifetime");
                    let ttl = self
                        .covenant_ttl_v1(Hash64::from_bytes(claim))
                        .ok_or("the material TTL: no healthy event-relative monetary covenant for the kernel claim");
                    (hard, ttl, hard)
                }
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
    /// **Admission of a claim type whose liability terms have no authenticated source** (no signed hard deadline, no material
    /// TTL — the native funded, native text, native job and native mining claims): refused past the fence, by name; a no-op below.
    pub(super) fn refuse_unsourced_admission_v1(&self, now: u64, why: &'static str) -> Result<(), PalwStateV2Error> {
        if self.liability_fence_from_v1(now).is_some() {
            return Err(liability_refused_v1(PalwSettlementLiabilityRefusalV1::Unsourced(why)));
        }
        Ok(())
    }
}
