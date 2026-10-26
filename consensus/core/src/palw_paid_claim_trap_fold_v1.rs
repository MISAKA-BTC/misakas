//! **RFC-0007 §4.1 T6 in the fold: paid-claim commitments and traps, the receipt signers' liability and held verifier check fees**
//! (dormant `palw_paid_claim_trap_v1`; kernel route aux 142). A child module of `palw_state_v2`, like the mesh and kernel route
//! folds, so it reads the builder and writes only through the state's journaled writers. The rows and the pure rules are
//! [`crate::palw_paid_claim_trap_v1`]'s. Below the fence every entry point returns before reading or writing anything, so every
//! historical byte is unchanged (the mesh's own trap rule, the paid lock, the RFC08-2 refusal of check fees).
//!
//! * **Commitment** ([`apply_paid_trap_commitment_v1`], the paid-claim object family's appended `TrapCommitment`): every
//!   non-staged paid claim's producer posts exactly one trap-or-null commitment before the claim's beacon seal window (aux 142,
//!   `0x03 ‖ enrollment`); the paid lock draws no seat without it ([`paid_trap_lock_admits_v1`]). It lives to the claim's own
//!   reveal-window end and holds the same deposit (if any) whatever it opens to.
//! * **Reveal** ([`apply_paid_trap_revealed_v1`], reached from the mesh's `TrapRevealed` arm when the reveal names a paid V2 base
//!   claim): judged by [`PalwChainStateV2::paid_trap_revealed_admissible_v1`] — it opens the claim's commitment to a fault that
//!   counts (the beacon seed fixed after the commitment, and the salt) and the court convicted; the commitment is spent and its
//!   deposit returned; the conviction's accuser, when it is a drawn seat other than the setter, is paid the existing trap bounty
//!   (once, from the panel reserve); the Clean signers' liability is charged (idempotent — the conviction already charged it).
//! * **Conviction** ([`on_paid_claim_convictions_v1`], after every kernel settlement batch): a paid V2 base claim's `Convicted`
//!   event records the conviction once (its accuser) and charges every Clean receipt signer the signer liability on its own
//!   collateral, once per (claim, seat) — a trap or an ordinary paid claim alike.
//! * **Check fees** ([`check_fee_hold_until_v1`] / [`write_held_check_fee_v1`] in the kernel settlement loop;
//!   [`release_held_check_fees_v1`] in the closing tick): a paid V2 claim's verifier check fee is held at its attestation and
//!   released once the trap reveal window has passed and the claim is terminal — less the forfeiture when the court convicted
//!   the claim and the verifier signed a Clean receipt — through the once-only registry under the combined liability predicate
//!   (`PalwLegLiabilityV1::VerifierCheck`). The same tick forgets commitments and convictions past their window.
//!
//! No credit and no weight (RFC-0018 Gate C stays the only credit path); a trap's fault is the court's own conviction, never a
//! proof of computation (PRINCIPLES §8).
use super::native_writers::local_capacity_or_v1;
use super::palw_settlement_liability_fold_v1::PalwLegLiabilityV1;
use super::*;
use crate::palw_claim_challenge_v2::{PAID_CLAIM_LOCKS_V2, PAID_CLAIM_RECEIPTS_V2, PalwPaidClaimLockV2};
use crate::palw_kernel_route_v1::{PalwKernelRouteStateV1, palw_kernel_bond_id_v1};
use crate::palw_mesh_v1::PalwTrapRevealedV1;
use crate::palw_paid_claim_trap_v1::{
    PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1, PALW_PAID_CLAIM_TRAP_COMMITMENT_PREFIX_V1, PALW_PAID_CLAIM_TRAP_CONVICTION_PREFIX_V1,
    PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1, PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1, PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
    PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1, PALW_PAID_CLAIM_TRAP_TABLE_V1, PalwHeldCheckFeeRowV1, PalwPaidClaimConvictionRowV1,
    PalwPaidTrapClaimV1, PalwPaidTrapCommitInputV1, PalwPaidTrapCommitmentRowV1, PalwPaidTrapRevealInputV1,
    palw_paid_trap_commit_admits_v1, palw_paid_trap_commitment_key_v1, palw_paid_trap_conviction_key_v1, palw_paid_trap_counts_v1,
    palw_paid_trap_forfeit_v1, palw_paid_trap_held_fee_key_v1, palw_paid_trap_reveal_admits_v1,
};
use crate::palw_settlement_once_v1::{PalwSettlementConsumerV1, PalwSettlementDebitV1, PalwSettlementLegV1, PalwSettlementObligationV1};

fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::MeshRefused(why.to_string())
}

/// The receipt id domain of a paid V2 seat receipt (`palw_claim_challenge_v2`'s receipt rows, aux 117).
const PAID_CLAIM_RECEIPT_DOMAIN_V2: &[u8] = b"MISAKA/PALW/PAID-CLAIM-RECEIPT/V2";

/// The paid claim's seat lock (aux 116), if it names this enrollment.
pub(super) fn paid_lock_v1(route: &PalwKernelRouteStateV1, enrollment: &[u8; 64]) -> Option<PalwPaidClaimLockV2> {
    route.aux_row::<PalwPaidClaimLockV2>(PAID_CLAIM_LOCKS_V2, enrollment).filter(|lock| lock.enrollment == *enrollment)
}

/// The paid claim's commitment row (aux 142, `0x03 ‖ enrollment`), if it stands and names this enrollment.
pub(super) fn paid_commitment_v1(route: &PalwKernelRouteStateV1, enrollment: &[u8; 64]) -> Option<PalwPaidTrapCommitmentRowV1> {
    route
        .aux_row::<PalwPaidTrapCommitmentRowV1>(PALW_PAID_CLAIM_TRAP_TABLE_V1, &palw_paid_trap_commitment_key_v1(enrollment))
        .filter(|row| row.version == PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1 && row.enrollment == *enrollment)
}

/// **The paid lock's gate** (`palw_claim_challenge_fold_v2::lock`): past the fence no seat is drawn for a paid claim without its
/// producer's one trap-or-null commitment. `Ok` below the fence (the historical lock, byte for byte).
pub(super) fn paid_trap_lock_admits_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    enrollment: &[u8; 64],
    daa: u64,
) -> Result<(), &'static str> {
    if !params.paid_claim_trap_active_at(daa) {
        return Ok(());
    }
    if state.kernel_route.as_ref().and_then(|route| paid_commitment_v1(route, enrollment)).is_none() {
        return Err("paid claim lock needs its producer's trap-or-null commitment (palw_paid_claim_trap_v1)");
    }
    Ok(())
}

/// **Did `signer` (kernel id) sign a Clean receipt on `enrollment`?** Its receipt row (aux 117, under the receipt id of
/// `(enrollment, signer)`) exists, decodes, names this enrollment and signer, and is complete (round 0, the non-interactive form).
pub(super) fn clean_receipt_signer_v1(route: &PalwKernelRouteStateV1, enrollment: &[u8; 64], signer: &[u8; 64]) -> bool {
    let id = misaka_palw_kernel::hash::object_id(PAID_CLAIM_RECEIPT_DOMAIN_V2, &(*enrollment, *signer));
    route
        .aux_row::<misaka_palw_kernel::claim_paid_v2::PaidClaimReceiptV2>(PAID_CLAIM_RECEIPTS_V2, &id)
        .is_some_and(|receipt| receipt.complete && receipt.round == 0 && receipt.enrollment == *enrollment && receipt.signer == *signer)
}

/// **The lock seats of a paid claim that signed a Clean receipt**, in seat order (each seat once). An interactive (staged) paid
/// claim has no single Clean receipt: none (the trap does not cover it — reported, not guessed).
pub(super) fn clean_receipt_signers_v1(route: &PalwKernelRouteStateV1, paid: &PalwPaidTrapClaimV1) -> Vec<PalwBondKeyV2> {
    if paid.interactive {
        return Vec::new();
    }
    let Some(lock) = paid_lock_v1(route, &paid.enrollment) else { return Vec::new() };
    let mut signers: Vec<PalwBondKeyV2> = Vec::new();
    for seat in lock.seats {
        if !signers.contains(&seat) && clean_receipt_signer_v1(route, &paid.enrollment, &palw_kernel_bond_id_v1(&seat)) {
            signers.push(seat);
        }
    }
    signers
}

/// **`PaidClaimObjectV2::TrapCommitment`** (the caller resolved the enrollment's authority into `paid` and its seal window start):
/// judged by [`palw_paid_trap_commit_admits_v1`]; the deposit (if the measured record sets one) is reserved on the producer's bond
/// and the row written. Refused below the fence (the object is dropped; the block stands).
pub(super) fn apply_paid_trap_commitment_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    signer: &PalwBondKeyV2,
    paid: &PalwPaidTrapClaimV1,
    seal_start: u64,
    commitment: &[u8; 64],
) -> Result<(), PalwStateV2Error> {
    if !builder.params.paid_claim_trap_active_at(ctx.daa_score) {
        return Err(refused("a paid trap commitment before palw_paid_claim_trap_v1 is in force"));
    }
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("paid trap commitment: no kernel route"))?;
    let free = builder
        .state
        .bonds
        .get(&paid.producer)
        .map_or(0, |record| super::palw_mesh_fold_v1::free_collateral_v1(&builder.state, &paid.producer, record));
    palw_paid_trap_commit_admits_v1(&PalwPaidTrapCommitInputV1 {
        signer: *signer,
        producer: paid.producer,
        seal_start,
        locked: paid_lock_v1(route, &paid.enrollment).is_some(),
        committed: route.aux.contains_key(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, palw_paid_trap_commitment_key_v1(&paid.enrollment))),
        free_collateral: free,
        now: ctx.daa_score,
    })
    .map_err(refused)?;
    let deposit = PALW_PAID_CLAIM_TRAP_COMMIT_DEPOSIT_SOMPI_V1;
    builder.mesh_reserve(paid.producer, u128::from(deposit))?;
    let row = PalwPaidTrapCommitmentRowV1 {
        version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
        enrollment: paid.enrollment,
        base_claim: paid.base_claim,
        producer: paid.producer,
        commitment: *commitment,
        committed_daa: ctx.daa_score,
        deposit,
        forget_after_daa: paid.reveal_end(),
    };
    builder.write_kernel_row_encoded_v1(PALW_PAID_CLAIM_TRAP_TABLE_V1, &palw_paid_trap_commitment_key_v1(&paid.enrollment), Some(&row))
}

impl PalwChainStateV2 {
    /// **The paid V2 claim a reveal over `claim` targets** past `palw_paid_claim_trap_v1`: `None` below the fence or when `claim` is
    /// not a paid V2 base claim — the mesh's own rule then judges the reveal, byte for byte.
    pub fn paid_trap_target_v1(&self, params: &PalwStateParamsV2, claim: &Hash64, daa: u64) -> Option<PalwPaidTrapClaimV1> {
        if !params.paid_claim_trap_active_at(daa) {
            return None;
        }
        self.paid_trap_target_checked_v1(params, claim, daa).ok().flatten()
    }

    /// Native lookup: malformed present rows and capacity never select mesh
    /// fallback. The Option-only method above is a historical read wrapper.
    pub fn paid_trap_target_checked_v1(
        &self, params: &PalwStateParamsV2, claim: &Hash64, daa: u64,
    ) -> Result<Option<PalwPaidTrapClaimV1>, PalwStateV2Error> {
        if !params.paid_claim_trap_active_at(daa) { return Ok(None); }
        let Some(route) = self.kernel_route.as_ref() else { return Ok(None) };
        route.paid_claim_authority_for_base_checked_v1(&claim.as_bytes())
            .map(|a| a.map(|a| PalwPaidTrapClaimV1::of(&a)))
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))
    }

    /// **The stateful half of a `TrapRevealed`'s admission, for the acceptance walk and the fold alike**: a paid-claim trap past the
    /// fence ([`Self::paid_trap_revealed_admissible_v1`]), otherwise exactly the mesh's rule (`mesh_trap_revealed_admissible_v2`,
    /// with lane PA's rule where `palw_audit_1004_v1` is in force). The ML-DSA signature stays the walk's.
    pub fn trap_revealed_admissible_v3(&self, params: &PalwStateParamsV2, reveal: &PalwTrapRevealedV1, daa: u64) -> Result<(), String> {
        match self.paid_trap_target_checked_v1(params, &reveal.claim, daa).map_err(|why| why.to_string())? {
            Some(paid) => self.paid_trap_revealed_admissible_v1(&paid, reveal, daa).map_err(str::to_string),
            None => self.mesh_trap_revealed_admissible_v2(reveal, daa, params.audit_1004_active_at(daa)).map_err(|e| e.to_string()),
        }
    }

    /// **A paid-claim trap's reveal** ([`palw_paid_trap_reveal_admits_v1`] over the state): the fault's evidence is the kernel
    /// claim row's own conviction (the court's deterministic FileProof/FileDemand check), never anything the reveal carries.
    pub fn paid_trap_revealed_admissible_v1(&self, paid: &PalwPaidTrapClaimV1, reveal: &PalwTrapRevealedV1, daa: u64) -> Result<(), &'static str> {
        let convicted =
            self.kernel_route.as_ref().and_then(|route| route.kernel_claim_row_v1(&reveal.claim)).is_some_and(|row| row.convicted);
        palw_paid_trap_reveal_admits_v1(&self.paid_trap_reveal_input_v1(paid, reveal, daa, convicted))
    }

    /// What the reveal rule is asked about, from the state (`convicted` is the kernel row's verdict, read by the caller).
    pub(super) fn paid_trap_reveal_input_v1(
        &self,
        paid: &PalwPaidTrapClaimV1,
        reveal: &PalwTrapRevealedV1,
        daa: u64,
        convicted: bool,
    ) -> PalwPaidTrapRevealInputV1 {
        let route = self.kernel_route.as_ref();
        // The claim's commitment, when this reveal opens it (a reveal over another claim than this paid claim's base opens nothing).
        let trap = route
            .filter(|_| reveal.claim.as_bytes() == paid.base_claim)
            .and_then(|route| paid_commitment_v1(route, &paid.enrollment))
            .filter(|row| row.commitment == reveal.commitment().as_bytes())
            .map(|row| (row.producer, row.committed_daa));
        let lock = route.and_then(|route| paid_lock_v1(route, &paid.enrollment));
        PalwPaidTrapRevealInputV1 {
            tiles: reveal.tiles,
            fault_leaf: reveal.fault_leaf,
            trap,
            setter: reveal.setter_bond,
            producer: paid.producer,
            interactive: paid.interactive,
            lock_daa: lock.as_ref().map(|lock| lock.locked_daa),
            counts: lock.as_ref().is_some_and(|lock| palw_paid_trap_counts_v1(&lock.seed, &paid.enrollment, &reveal.salt)),
            checking_until: paid.lifetime.checking_until,
            reveal_end: paid.reveal_end(),
            convicted,
            now: daa,
        }
    }
}

/// **The seat a trap's bounty pays**: the accuser the court recorded at the paid claim's conviction (aux 142), when it is one of
/// the claim's drawn lock seats and not the setter (a setter convicting its own trap earns nothing). `None` otherwise — a
/// conviction filed by anybody else stands, unpaid by the trap.
pub(super) fn trap_bounty_seat_v1(route: &PalwKernelRouteStateV1, paid: &PalwPaidTrapClaimV1, setter: &PalwBondKeyV2) -> Option<PalwBondKeyV2> {
    let conviction = route
        .aux_row::<PalwPaidClaimConvictionRowV1>(PALW_PAID_CLAIM_TRAP_TABLE_V1, &palw_paid_trap_conviction_key_v1(&paid.base_claim))
        .filter(|row| row.version == PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1 && row.base_claim == paid.base_claim)?;
    paid_lock_v1(route, &paid.enrollment)?
        .seats
        .into_iter()
        .find(|seat| palw_kernel_bond_id_v1(seat) == conviction.accuser)
        .filter(|seat| seat != setter)
}

/// Spend a commitment: the row goes and its deposit (if any) returns to the producer.
fn spend_commitment_v1(builder: &mut TransitionBuilder<'_>, row: &PalwPaidTrapCommitmentRowV1) -> Result<(), PalwStateV2Error> {
    builder.write_kernel_row_encoded_v1::<PalwPaidTrapCommitmentRowV1>(
        PALW_PAID_CLAIM_TRAP_TABLE_V1,
        &palw_paid_trap_commitment_key_v1(&row.enrollment),
        None,
    )?;
    let _ = local_capacity_or_v1(builder.mesh_release(row.producer, u128::from(row.deposit)))?;
    Ok(())
}

/// **`TrapRevealed` over a paid V2 base claim** (module doc). Every refusal is an error: the object is dropped, the block stands.
pub(super) fn apply_paid_trap_revealed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    reveal: &PalwTrapRevealedV1,
    paid: &PalwPaidTrapClaimV1,
) -> Result<(), PalwStateV2Error> {
    builder.state.paid_trap_revealed_admissible_v1(paid, reveal, ctx.daa_score).map_err(refused)?;
    let route = builder.state.kernel_route.as_ref().ok_or_else(|| refused("paid trap: no kernel route"))?;
    let commitment = paid_commitment_v1(route, &paid.enrollment).ok_or_else(|| refused("paid trap: the commitment is gone"))?;
    let seat = trap_bounty_seat_v1(route, paid, &commitment.producer);
    // 1. The commitment is spent; its deposit comes back.
    spend_commitment_v1(builder, &commitment)?;
    // 2. The checker that convicted the planted fault: the existing trap bounty, once per (claim, seat), from the panel reserve.
    if let Some(seat) = seat {
        builder.mesh_pay_from_reserve(
            ctx.daa_score,
            &seat,
            crate::palw_mesh_v1::palw_mesh_trap_bounty_v1(crate::palw_verifier_pay_v1::PALW_VERIFIER_PAY_CHECK_FEE_SOMPI_V1),
            PalwSettlementLegV1::MeshTrapBounty,
            &Hash64::from_bytes(paid.base_claim),
            PalwLegLiabilityV1::NoLiableClaim("a paid-claim trap's bounty is a court outcome paid from the panel reserve"),
        )?;
    }
    // 3. The Clean signers answer for the planted fault (already charged at the conviction; once per seat).
    charge_receipt_signers_v1(builder, paid, ctx.daa_score)?;
    Ok(())
}

/// **The Clean receipt signers of a convicted paid claim are charged** the signer liability on their own collateral, once per
/// (claim, seat) through the once-only registry. Never refuses the batch it rides: a seat whose charge cannot be journaled is
/// skipped (a local capacity refusal still propagates, retryable).
pub(super) fn charge_receipt_signers_v1(
    builder: &mut TransitionBuilder<'_>,
    paid: &PalwPaidTrapClaimV1,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.paid_claim_trap_active_at(now) {
        return Ok(());
    }
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let signers = clean_receipt_signers_v1(route, paid);
    let amount = PALW_PAID_CLAIM_TRAP_SIGNER_LIABILITY_SOMPI_V1;
    for seat in signers {
        let kid = palw_kernel_bond_id_v1(&seat);
        let once = builder.settle_once_liable_v1(
            now,
            PalwSettlementObligationV1::with_party(PalwSettlementLegV1::ReceiptSignerLiability, paid.base_claim, kid, 0),
            PalwSettlementConsumerV1::PaidClaimTrap,
            amount,
            PalwSettlementDebitV1::Collateral { bond: kid, amount },
            PalwLegLiabilityV1::NoLiableClaim("a receipt signer's liability: a court charge on its own collateral, not a payment"),
        );
        let Ok(once) = local_capacity_or_v1(once)? else { continue };
        if !once.pays() {
            continue;
        }
        let _ = local_capacity_or_v1(builder.slash_bond(seat, u128::from(amount)))?;
    }
    Ok(())
}

/// **A batch's paid-claim convictions** (after every kernel settlement batch, object arm or closing tick): each `Convicted` of a
/// paid V2 base claim is recorded once (aux 142: its accuser, forgotten after the claim's reveal window) and its Clean receipt
/// signers are charged. A no-op below the fence and for every other claim.
pub(super) fn on_paid_claim_convictions_v1(
    builder: &mut TransitionBuilder<'_>,
    events: &[misaka_palw_kernel::ledger::LedgerEventV1],
    now: u64,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.paid_claim_trap_active_at(now) {
        return Ok(());
    }
    // Check every relevant target before the first conviction row/charge can
    // write. This linear preflight allocates no batch map and rebuilds no ledger.
    if let Some(route) = builder.state.kernel_route.as_ref() {
        for event in events {
            if let misaka_palw_kernel::ledger::LedgerEventV1::Convicted { claim, .. } = event {
                route.paid_claim_authority_for_base_checked_v1(claim)
                    .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?;
            }
        }
    }
    for event in events {
        let misaka_palw_kernel::ledger::LedgerEventV1::Convicted { claim, accuser, .. } = event else { continue };
        let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
        let Some(paid) = route.paid_claim_authority_for_base_checked_v1(claim)
            .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?
            .map(|a| PalwPaidTrapClaimV1::of(&a)) else { continue };
        let key = palw_paid_trap_conviction_key_v1(claim);
        if route.aux.contains_key(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, key.clone())) {
            continue; // recorded once
        }
        let row = PalwPaidClaimConvictionRowV1 {
            version: PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1,
            base_claim: *claim,
            enrollment: paid.enrollment,
            accuser: *accuser,
            convicted_daa: now,
            forget_after_daa: paid.reveal_end().max(now),
        };
        builder.write_kernel_row_encoded_v1(PALW_PAID_CLAIM_TRAP_TABLE_V1, &key, Some(&row))?;
        charge_receipt_signers_v1(builder, &paid, now)?;
    }
    Ok(())
}

/// **Is this check fee held?** Past the fence, the release DAA of a paid V2 claim's verifier check fee (its trap reveal window end);
/// `None` below the fence or for a claim with no paid V2 enrollment (the RFC08-2 path then runs unchanged).
pub(super) fn check_fee_hold_until_v1(builder: &TransitionBuilder<'_>, claim: &[u8; 64], now: u64) -> Option<u64> {
    if !builder.params.paid_claim_trap_active_at(now) {
        return None;
    }
    check_fee_hold_until_checked_v1(builder, claim, now).ok().flatten()
}

pub(super) fn check_fee_hold_until_checked_v1(
    builder: &TransitionBuilder<'_>, claim: &[u8; 64], now: u64,
) -> Result<Option<u64>, PalwStateV2Error> {
    builder.state.paid_trap_target_checked_v1(builder.params, &Hash64::from_bytes(*claim), now)
        .map(|paid| paid.map(|paid| paid.reveal_end()))
}

/// **May this slot's fee be held now?** Neither held already nor settled by any consumer (a repeat is refused by a strict arm and
/// skipped whole by the tick, as the RFC08-1 marker does).
pub(super) fn check_fee_hold_admits_v1(
    builder: &mut TransitionBuilder<'_>,
    obligation: &PalwSettlementObligationV1,
) -> Result<bool, PalwStateV2Error> {
    if builder.settled_row_checked_v1(obligation)?.is_some() {
        return Ok(false);
    }
    let key = palw_paid_trap_held_fee_key_v1(&obligation.subject, &obligation.party, obligation.ordinal);
    Ok(!builder.state.kernel_route.as_ref().is_some_and(|route| route.aux.contains_key(&(PALW_PAID_CLAIM_TRAP_TABLE_V1, key))))
}

/// Write a held check fee (its `fee` is what the poster's escrow actually gave up). A zero fee holds nothing.
pub(super) fn write_held_check_fee_v1(builder: &mut TransitionBuilder<'_>, row: &PalwHeldCheckFeeRowV1) -> Result<(), PalwStateV2Error> {
    if row.fee == 0 {
        return Ok(());
    }
    let key = palw_paid_trap_held_fee_key_v1(&row.claim, &row.verifier, row.slot);
    builder.write_kernel_row_encoded_v1(PALW_PAID_CLAIM_TRAP_TABLE_V1, &key, Some(row))
}

/// **Release one held check fee** whose window has passed (the row is deleted first, so it is released once whatever follows):
/// the forfeiture is taken when the court convicted the claim and the verifier signed a Clean receipt on it; the rest is settled
/// once (`VerifierCheckFee` of `(claim, verifier, slot)`) under the combined liability predicate and paid to the verifier's
/// payout payload. A forfeited or refused part is not paid (the poster's debit was a real slash: it stays burned).
pub(super) fn release_held_check_fee_v1(
    builder: &mut TransitionBuilder<'_>,
    row: &PalwHeldCheckFeeRowV1,
    convicted_clean_signer: bool,
    now: u64,
) -> Result<(), PalwStateV2Error> {
    let key = palw_paid_trap_held_fee_key_v1(&row.claim, &row.verifier, row.slot);
    builder.write_kernel_row_encoded_v1::<PalwHeldCheckFeeRowV1>(PALW_PAID_CLAIM_TRAP_TABLE_V1, &key, None)?;
    let (_, kept) = if convicted_clean_signer { palw_paid_trap_forfeit_v1(row.fee) } else { (0, row.fee) };
    let once = builder.settle_once_liable_v1(
        now,
        PalwSettlementObligationV1::with_party(PalwSettlementLegV1::VerifierCheckFee, row.claim, row.verifier, row.slot),
        PalwSettlementConsumerV1::PaidClaimTrap,
        kept,
        PalwSettlementDebitV1::Escrow { poster: row.poster, amount: row.fee },
        PalwLegLiabilityV1::VerifierCheck { claim: row.claim, fee: row.fee, horizon: row.release_after_daa },
    );
    let Ok(once) = local_capacity_or_v1(once)? else { return Ok(()) };
    if !once.pays() || kept == 0 {
        return Ok(());
    }
    let Some(bond) = builder.state.kernel_route.as_ref().and_then(|route| route.bond_key_of(&row.verifier)) else { return Ok(()) };
    let Some(payload) = builder.state.bonds.get(&bond).map(|record| record.payout_payload) else { return Ok(()) };
    // The same R/F accounting the kernel's `FinalReward` arm does for a check fee, then the payout row.
    if local_capacity_or_v1(builder.consume_kernel_final_reward_v1(&row.claim, bond, payload, now, kept))?.is_err() {
        return Ok(());
    }
    let _ = local_capacity_or_v1(builder.add_kernel_payout(payload, kept))?;
    Ok(())
}

/// **The closing tick's sweep** (before the ledger's own tick; a no-op below the fence): held check fees whose reveal window has
/// passed and whose claim is terminal (convicted or Final) are released; commitments and conviction rows past their window are
/// forgotten (a commitment's deposit returned) — at most [`PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1`] of each kind a block, in key
/// order. A held fee of a claim not yet terminal waits.
pub(super) fn release_held_check_fees_v1(builder: &mut TransitionBuilder<'_>, now: u64) -> Result<(), PalwStateV2Error> {
    if !builder.params.paid_claim_trap_active_at(now) {
        return Ok(());
    }
    let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
    let table = PALW_PAID_CLAIM_TRAP_TABLE_V1;
    let held_range = (table, vec![PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1])..(table, vec![PALW_PAID_CLAIM_TRAP_HELD_FEE_PREFIX_V1 + 1]);
    let mut due: Vec<(PalwHeldCheckFeeRowV1, bool)> = Vec::new();
    for ((_, key), bytes) in route.aux.range(held_range) {
        if due.len() >= PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1 {
            break;
        }
        let Ok(row) = borsh::from_slice::<PalwHeldCheckFeeRowV1>(bytes) else { continue };
        if row.version != PALW_PAID_CLAIM_TRAP_ROW_VERSION_V1
            || *key != palw_paid_trap_held_fee_key_v1(&row.claim, &row.verifier, row.slot)
            || row.release_after_daa >= now
        {
            continue;
        }
        let Some(kernel) = route.kernel_claim_row_v1(&Hash64::from_bytes(row.claim)) else { continue };
        if !kernel.convicted && !matches!(kernel.life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::Final { .. }) {
            continue; // not terminal yet: the fee waits for the verdict
        }
        let clean = if kernel.convicted {
            // A persisted held fee is already a paid obligation. Missing
            // authority here is corruption, never an ordinary unpaid claim.
            let authority = route.paid_claim_authority_for_base_checked_v1(&row.claim)
                .map_err(|why| crate::palw_kernel_route_v1::kernel_ledger_projection_state_error_v1(why, refused))?
                .ok_or_else(|| refused("held paid check fee has no base enrollment authority"))?;
            let paid = PalwPaidTrapClaimV1::of(&authority);
            !paid.interactive && clean_receipt_signer_v1(route, &paid.enrollment, &row.verifier)
        } else { false };
        due.push((row, clean));
    }
    let commitment_range =
        (table, vec![PALW_PAID_CLAIM_TRAP_COMMITMENT_PREFIX_V1])..(table, vec![PALW_PAID_CLAIM_TRAP_COMMITMENT_PREFIX_V1 + 1]);
    let expired: Vec<PalwPaidTrapCommitmentRowV1> = route
        .aux
        .range(commitment_range)
        .filter_map(|(_, bytes)| borsh::from_slice::<PalwPaidTrapCommitmentRowV1>(bytes).ok())
        .filter(|row| row.forget_after_daa < now)
        .take(PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1)
        .collect();
    let conviction_range =
        (table, vec![PALW_PAID_CLAIM_TRAP_CONVICTION_PREFIX_V1])..(table, vec![PALW_PAID_CLAIM_TRAP_CONVICTION_PREFIX_V1 + 1]);
    let stale: Vec<Vec<u8>> = route
        .aux
        .range(conviction_range)
        .filter(|(_, bytes)| {
            borsh::from_slice::<PalwPaidClaimConvictionRowV1>(bytes).map_or(true, |row| row.forget_after_daa < now)
        })
        .map(|((_, key), _)| key.clone())
        .take(PALW_PAID_CLAIM_TRAP_RELEASES_PER_BLOCK_V1)
        .collect();
    for (row, clean) in due {
        release_held_check_fee_v1(builder, &row, clean, now)?;
    }
    // A commitment past the claim's reveal window: a null one (or an unrevealed trap) simply ends, its deposit returned — the
    // same for both, so its end says nothing either.
    for row in expired {
        spend_commitment_v1(builder, &row)?;
    }
    for key in stale {
        builder.write_kernel_row_encoded_v1::<PalwPaidClaimConvictionRowV1>(table, &key, None)?;
    }
    Ok(())
}
