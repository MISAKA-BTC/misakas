//! **The fold of the once-only settlement registry** (`crate::palw_settlement_once_v1`; kernel route aux 140). A child module of
//! `palw_state_v2`, like the kernel route's own fold. Below `palw_settlement_once_v1` (every preset) [`TransitionBuilder::settle_once_v1`]
//! returns `Dormant` before reading or writing anything, so every consumer's historical bytes are unchanged.
use super::*;
use crate::palw_settlement_once_v1::*;

/// What a consumer may do with one obligation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PalwSettleOnceV1 {
    /// Below the fence: no registry; the consumer runs exactly its historical rule.
    Dormant,
    /// The marker was absent and is now written in this transition: the consumer pays (once).
    First,
    /// Another consumer (or this one, earlier) already settled this exact obligation: the consumer must not pay.
    AlreadySettled(PalwSettlementOnceRowV1),
    /// RFC08-2: the combined liability predicate refused the leg (`settle_once_liable_v1`); the obligation is closed with a
    /// zero-amount marker and the consumer must not pay.
    Refused(crate::palw_settlement_liability_v1::PalwSettlementLiabilityRefusalV1),
}

impl PalwSettleOnceV1 {
    /// Whether the consumer may move value for this obligation now.
    pub(super) fn pays(&self) -> bool {
        !matches!(self, Self::AlreadySettled(_) | Self::Refused(_))
    }
}

fn refused(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(why.to_string())
}

impl TransitionBuilder<'_> {
    /// **Check and write the obligation's marker in the same transition, before any value moves.** `amount` is what this
    /// consumer is about to pay (or debit) for the leg, `debit` the real source it is paid from; a payment beyond its debit is
    /// refused (`KernelRouteRefused`, semantic). A missing route header past the fence fails closed — no payment can be made
    /// without its marker. A local capacity refusal is `NativeResource`, never a settlement verdict.
    pub(super) fn settle_once_v1(
        &mut self,
        now: u64,
        obligation: PalwSettlementObligationV1,
        consumer: PalwSettlementConsumerV1,
        amount: u64,
        debit: PalwSettlementDebitV1,
    ) -> Result<PalwSettleOnceV1, PalwStateV2Error> {
        if !self.params.settlement_once_active_at(now) {
            return Ok(PalwSettleOnceV1::Dormant);
        }
        if self.state.kernel_route.is_none() {
            return Err(refused("the settlement-once registry needs the kernel route header (fail closed: no unmarked payment)"));
        }
        let id = obligation.id_v1();
        if let Some(row) = self.settled_row_checked_v1(&obligation)? {
            return Ok(PalwSettleOnceV1::AlreadySettled(row));
        }
        let row = PalwSettlementOnceRowV1 {
            version: PALW_SETTLEMENT_ONCE_ROW_VERSION_V1,
            obligation,
            consumer,
            amount,
            debit,
            settled_daa: now,
        };
        row.validate_v1(&id).map_err(refused)?;
        self.write_kernel_row_encoded_v1(PALW_SETTLEMENT_ONCE_TABLE_V1, &id, Some(&row))?;
        Ok(PalwSettleOnceV1::First)
    }
}

/// **The settled marker of `obligation`**, validated under its own key — `Err` (fail closed) for an undecodable or foreign row,
/// `Ok(None)` when the obligation is open (or there is no route).
impl TransitionBuilder<'_> {
    pub(super) fn settled_row_checked_v1(
        &mut self,
        obligation: &PalwSettlementObligationV1,
    ) -> Result<Option<PalwSettlementOnceRowV1>, PalwStateV2Error> {
        let id = obligation.id_v1();
        // Every consumer, including pre-payment probes, reserves the owned
        // BTreeMap key before constructing it. The decoded marker is fixed-size.
        if self.native_writers_tracking_v1() {
            let mut q = super::native_writers::NativeWriteQuoteV1::new("settlement-once marker lookup key");
            q.exact::<u8>(id.len() as u64)?;
            self.admit_native_write_v1(q)?;
        }
        let Some(bytes) =
            self.state.kernel_route.as_ref().and_then(|route| route.aux.get(&(PALW_SETTLEMENT_ONCE_TABLE_V1, id.to_vec())))
        else {
            return Ok(None);
        };
        let row: PalwSettlementOnceRowV1 =
            borsh::from_slice(bytes).map_err(|_| refused("an undecodable settlement-once marker (fail closed)"))?;
        row.validate_v1(&id).map_err(refused)?;
        Ok(Some(row))
    }
}

/// The settled marker of `obligation` in `state`, if any (source controls read the journal through this).
#[cfg(test)]
pub(super) fn palw_settled_once_v1(
    state: &PalwChainStateV2,
    obligation: &PalwSettlementObligationV1,
) -> Option<PalwSettlementOnceRowV1> {
    let id = obligation.id_v1();
    let bytes = state.kernel_route.as_ref()?.aux.get(&(PALW_SETTLEMENT_ONCE_TABLE_V1, id.to_vec()))?;
    borsh::from_slice::<PalwSettlementOnceRowV1>(bytes).ok().filter(|row| row.validate_v1(&id).is_ok())
}

/// **The kernel GAP-5 producer reward triple** at `index`: `PayJobEscrow(poster, paid)`, `ReleaseJobEscrow(poster, rest)`,
/// `FinalReward(producer, paid)` for one claim, with NO `CheckFeePaid` for that recipient after it — a verifier check fee emits the
/// same triple followed by `CheckFeePaid`, and is a separate per-slot obligation ([`kernel_check_fee_at_v1`],
/// `PalwSettlementLegV1::VerifierCheckFee`). Returns `(claim, poster, producer, paid)` for a producer reward; `None` for anything
/// else.
pub(super) fn kernel_producer_reward_at_v1(
    events: &[misaka_palw_kernel::ledger::LedgerEventV1],
    index: usize,
) -> Option<([u8; 64], [u8; 64], [u8; 64], u64)> {
    use misaka_palw_kernel::ledger::LedgerEventV1 as E;
    use misaka_palw_kernel::settle::SettlementKindV1 as K;
    let settlement = |i: usize| match events.get(i) {
        Some(E::Settlement(s)) => Some(*s),
        _ => None,
    };
    let pay = settlement(index).filter(|s| s.kind == K::PayJobEscrow)?;
    let claim = pay.claim?;
    settlement(index + 1).filter(|s| s.kind == K::ReleaseJobEscrow && s.claim == Some(claim) && s.bond == pay.bond)?;
    let reward = settlement(index + 2).filter(|s| s.kind == K::FinalReward && s.claim == Some(claim) && s.amount == pay.amount)?;
    let check_fee =
        matches!(events.get(index + 3), Some(E::CheckFeePaid { claim: c, verifier, .. }) if *c == claim && *verifier == reward.bond);
    (!check_fee).then_some((claim, pay.bond, reward.bond, pay.amount))
}

/// **A verifier's drawn-slot check-fee quad** at `index` (`palw_verifier_pay_v1::settle_draw_slots_v1`): `PayJobEscrow(poster, fee)`,
/// `ReleaseJobEscrow(poster, rest)`, `FinalReward(verifier, fee)`, then `CheckFeePaid { claim, verifier, fee }` naming exactly that
/// verifier and amount. Returns `(claim, poster, verifier, fee)`; `None` for anything else (a producer reward, a fee returned unpaid —
/// which emits no `FinalReward` — or a malformed sequence). Pure.
pub(super) fn kernel_check_fee_at_v1(
    events: &[misaka_palw_kernel::ledger::LedgerEventV1],
    index: usize,
) -> Option<([u8; 64], [u8; 64], [u8; 64], u64)> {
    use misaka_palw_kernel::ledger::LedgerEventV1 as E;
    use misaka_palw_kernel::settle::SettlementKindV1 as K;
    let settlement = |i: usize| match events.get(i) {
        Some(E::Settlement(s)) => Some(*s),
        _ => None,
    };
    let pay = settlement(index).filter(|s| s.kind == K::PayJobEscrow)?;
    let claim = pay.claim?;
    settlement(index + 1).filter(|s| s.kind == K::ReleaseJobEscrow && s.claim == Some(claim) && s.bond == pay.bond)?;
    let reward = settlement(index + 2).filter(|s| s.kind == K::FinalReward && s.claim == Some(claim) && s.amount == pay.amount)?;
    match events.get(index + 3) {
        Some(E::CheckFeePaid { claim: c, verifier, amount }) if *c == claim && *verifier == reward.bond && *amount == pay.amount => {
            Some((claim, pay.bond, reward.bond, pay.amount))
        }
        _ => None,
    }
}
