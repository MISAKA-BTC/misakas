//! **RFC-0035 A / ADR-0202 (G14 §6): the rooted funded-escrow row, its spend lock and its qualified spend (DORMANT).**
//!
//! Pure rules. The processor and the fold read them only past `palw_tir_public_conviction_v1` (the fence that owns the public
//! original TFP1 route the funded TFO1 carrier wraps; `None` on every preset). No fence of its own is added: the funded original is
//! a sub-route of that public original route. Rows are installed only by the funded-original coordinator (round 2,
//! `palw_funded_original_coordinator_fold_v1.rs`), whose schedule is typed-missing in production, so no row exists on any network.
//!
//! What this module decides, and what it does not:
//! * **The row** ([`PalwFundedEscrowRowV1`]) records the actual funding outpoint, its amount, the payer's key id and native return
//!   payload, the claim and contract id, the accepting DAA and the derived hard deadline, and once-only accounting
//!   (`paid`/`burned`/`returned`/`settled_by`). Conservation `amount = paid + burned + returned + unreleased` is checked arithmetic
//!   ([`PalwFundedEscrowRowV1::unreleased_v1`]); a row that violates it is refused, never clamped.
//! * **The lock** — an outpoint of an unsettled rooted row in the selected-parent state may be spent only by a spend this module
//!   qualifies ([`palw_escrow_qualified_spend_v1`]). Within one mergeset a TFO1 carrier's OWN declared funding output, matched
//!   against its literal payer-signed terms ([`palw_tfo1_provisional_lock_v1`]), is locked for every later transaction of that
//!   mergeset — never anyone else's outpoint. If the contract is then refused, the output is an ordinary payer P2PKH output and
//!   spendable by the payer from the next block on.
//! * **The qualified spend** checks natively — against the rooted row only — the exact escrow input, the payee/return outputs,
//!   the amounts and the change. It never skips a script check and never writes a signature cache: the escrow output is the
//!   payer's own P2PKH, so its ordinary signature check still runs. The ONLY path that works now is the payer return after the hard
//!   deadline with no open liability. Every payee path needs the settlement fee schedule, which is a typed-missing parameter
//!   ([`PalwEscrowFeeScheduleV1`] has no constructible value): amounts and fees are NOT adopted (ADR-0202 "承認の範囲").
//!
//! Never: a ZK/succinct proof, a caller-supplied `paid` flag, a caller DTO promoted to authority, a zero-filled missing term.

use crate::Hash64;
use crate::TransactionId;
use crate::tx::{Transaction, TransactionOutpoint};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, HashMap};

/// The carriage tail of the rooted escrow rows (appended after every other tail, written only when non-empty). `0xFB`: clear of
/// every tail taken or reserved in this tree (`0x87`–`0xBB`, `0xC0`–`0xDA`, `0xE0`–`0xF8`).
pub const PALW_CARRIAGE_FUNDED_ESCROW_TAIL_V1: u8 = 0xFB;
/// The row format's version.
pub const PALW_FUNDED_ESCROW_ROW_VERSION_V1: u16 = 1;

/// Why a row, a transition or a spend is refused. Every value is a named refusal; none is a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEscrowRefusalV1 {
    /// Unknown version, zero amount, or an accepting DAA after the hard deadline.
    RowShape,
    /// `paid + burned + returned` exceeds `amount`, or an accounting sum overflows.
    Conservation,
    /// A settled row with value left (or liability open, or no settlement DAA), an unsettled row with nothing left, a return on an
    /// unsettled row, or an unsettled row carrying a settlement DAA.
    OnceOnly,
    /// A transition that rewrites an immutable field (amount, payer, return payload, claim, contract, clocks; the outpoint except
    /// by a continuation).
    ImmutableField,
    /// A transition this module does not define (a removal of an unsettled row, a resettlement, an install of a non-fresh row, an
    /// accounting decrease).
    Transition,
    /// The transaction does not spend this row's outpoint.
    NotEscrowInput,
    /// The transaction spends this row's outpoint more than once.
    DuplicateEscrowInput,
    /// The populated input entry's amount differs from the row's unreleased value, or the entry list is misaligned.
    EntryMismatch,
    /// The row is already settled.
    AlreadySettled,
    /// A settlement path needs the fee schedule, which is not adopted (typed-missing).
    SettlementScheduleMissing,
    /// The payer return's first output is not exactly the rooted return payload and the unreleased amount.
    ReturnOutputMismatch,
    /// The transaction's non-escrow outputs draw on the escrow value (its fee or change is funded from the escrow).
    EscrowFundsOtherOutputs,
    /// The hard deadline has no source (X3 does not govern the acceptance) or overflows.
    HardDeadlineUnsourced,
    /// A spend carrying a payload that is not exactly this row's relay authorization template, or a templated spend whose escrow
    /// input is not input 0 (the only index a `SIGHASH_SINGLE` pre-signature pairs with output 0).
    RelayTemplateMismatch,
    /// **G14 r5:** the schedule handed in is not this row's (another contract), or the row's consumed value is not a prefix of the
    /// schedule's dues (a due paid twice, or an order the schedule never had).
    SettlementRuleMismatch,
    /// **G14 r5:** a payee output differs from its due (payload or exact amount), or is missing.
    PayeeOutputMismatch,
    /// **G14 r5:** a settlement that pays no due and returns nothing (the escrow would be spent for nothing).
    NothingToSettle,
    /// **audit A-2:** the escrow input is not input 0. Every qualified spend (the payer's own as well as a relay's) carries its escrow
    /// at input 0, so a transaction discharges at most ONE escrow row: a second rooted escrow input is never qualified (the spend
    /// filter refuses the whole transaction), so two rows can never be satisfied by one set of due outputs, an escrow input is never
    /// counted as the other rows' "foreign" funding, and no two continued rows can share one change-back outpoint.
    EscrowNotFirstInput,
}

/// **The rooted escrow row.** Keyed by `contract_id` in the state. Written only by the funded-original installer, by a qualified
/// spend's settlement or continuation, by the liability clearing and by the post-pruning retirement — each journaled as one delta
/// entry with `old`/`new` so revert restores row and lock together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFundedEscrowRowV1 {
    pub version: u16,
    /// The signed funding terms' identity (`FundingTermsV1::id_v1`).
    pub contract_id: Hash64,
    /// The original claim the terms bind.
    pub claim: Hash64,
    /// **The escrow's CURRENT outpoint.** At install: the actual carrier's funding output — obtained from the final accepted
    /// carrier, never from the signed terms. After a partial settlement (G14 r5): the settlement transaction's change-back output,
    /// which holds exactly the unreleased value.
    pub funding_outpoint: TransactionOutpoint,
    pub amount_sompi: u64,
    pub payer_key_id: Hash64,
    /// The native P2PKH-MLDSA87 address payload the funding output pays (and a payer return / change-back must pay).
    pub refund_payload: Hash64,
    /// The carrier's actual accepting DAA.
    pub accepted_daa: u64,
    /// Derived: the adopted X3 hard deadline at `accepted_daa` ([`palw_funded_escrow_hard_deadline_v1`]).
    pub hard_deadline_daa: u64,
    pub paid_sompi: u64,
    pub burned_sompi: u64,
    pub returned_sompi: u64,
    /// A liability this escrow still answers for (a live settlement obligation). Set by the coordinator at admission, cleared only
    /// by the fold's resolution sweep ([`palw_escrow_liability_resolved_v1`]), never by a spender.
    pub liability_open: bool,
    /// Once-only: the transaction that consumed the last outpoint. `Some` exactly when the row is settled.
    pub settled_by: Option<TransactionId>,
    /// **G14 r5:** the accepting DAA of the settling spend (`0` while unsettled) — what the post-pruning retirement counts from.
    pub settled_daa: u64,
}

impl PalwFundedEscrowRowV1 {
    /// A fresh row for an accepted, fully-checked funded original: no value moved, no settlement.
    #[allow(clippy::too_many_arguments)]
    pub fn fresh_v1(
        contract_id: Hash64,
        claim: Hash64,
        funding_outpoint: TransactionOutpoint,
        amount_sompi: u64,
        payer_key_id: Hash64,
        refund_payload: Hash64,
        accepted_daa: u64,
        hard_deadline_daa: u64,
        liability_open: bool,
    ) -> Result<Self, PalwEscrowRefusalV1> {
        let row = Self {
            version: PALW_FUNDED_ESCROW_ROW_VERSION_V1,
            contract_id,
            claim,
            funding_outpoint,
            amount_sompi,
            payer_key_id,
            refund_payload,
            accepted_daa,
            hard_deadline_daa,
            paid_sompi: 0,
            burned_sompi: 0,
            returned_sompi: 0,
            liability_open,
            settled_by: None,
            settled_daa: 0,
        };
        row.check_v1()?;
        Ok(row)
    }

    /// `amount − paid − burned − returned`, checked: the conservation identity `in = paid + burned + returned + unreleased`.
    pub fn unreleased_v1(&self) -> Result<u64, PalwEscrowRefusalV1> {
        self.amount_sompi
            .checked_sub(self.paid_sompi)
            .and_then(|v| v.checked_sub(self.burned_sompi))
            .and_then(|v| v.checked_sub(self.returned_sompi))
            .ok_or(PalwEscrowRefusalV1::Conservation)
    }

    /// `paid + burned`, checked: what settlements have consumed (the prefix of the schedule's dues already discharged).
    pub fn consumed_v1(&self) -> Result<u64, PalwEscrowRefusalV1> {
        self.paid_sompi.checked_add(self.burned_sompi).ok_or(PalwEscrowRefusalV1::Conservation)
    }

    /// Nothing has moved: the row as installed.
    pub fn is_fresh_v1(&self) -> bool {
        self.paid_sompi == 0 && self.burned_sompi == 0 && self.returned_sompi == 0 && self.settled_by.is_none()
    }

    /// The row's own invariants: shape, conservation and once-only.
    pub fn check_v1(&self) -> Result<(), PalwEscrowRefusalV1> {
        if self.version != PALW_FUNDED_ESCROW_ROW_VERSION_V1 || self.amount_sompi == 0 || self.accepted_daa > self.hard_deadline_daa {
            return Err(PalwEscrowRefusalV1::RowShape);
        }
        let unreleased = self.unreleased_v1()?;
        match self.settled_by {
            // One spend ends the row: the whole value is accounted in it, nothing is owed after it, and it carries its DAA.
            Some(_) if unreleased != 0 || self.liability_open || self.settled_daa < self.accepted_daa => Err(PalwEscrowRefusalV1::OnceOnly),
            // Unsettled: the current outpoint holds the unreleased value (> 0); a return only ever ends a row.
            None if unreleased == 0 || self.returned_sompi != 0 || self.settled_daa != 0 => Err(PalwEscrowRefusalV1::OnceOnly),
            _ => Ok(()),
        }
    }

    fn same_core_immutables(&self, other: &Self) -> bool {
        (
            self.version,
            self.contract_id,
            self.claim,
            self.amount_sompi,
            self.payer_key_id,
            self.refund_payload,
            self.accepted_daa,
            self.hard_deadline_daa,
        ) == (
            other.version,
            other.contract_id,
            other.claim,
            other.amount_sompi,
            other.payer_key_id,
            other.refund_payload,
            other.accepted_daa,
            other.hard_deadline_daa,
        )
    }
}

/// **The only transitions a row may make in a rooted delta** (G14 r5 widened round 1's two):
/// * install a fresh row (`None → fresh`);
/// * clear the liability of an unsettled row (`open → closed`, nothing else changes) — the fold's resolution sweep only;
/// * continue an unsettled row after a partial settlement (value paid/burned grew, outpoint moved to the change-back output, no
///   return, liability unchanged);
/// * settle it once (`unsettled → settled`, outpoint unchanged — it is the spent one — value fully accounted);
/// * retire a settled row (`settled → None`) — the writer adds the post-pruning horizon.
///
/// Accounting never decreases; resettlement and every other edit are refused. The delta writer asks this before journaling, so a
/// revert (which swaps `old` and `new`) restores exactly a row this admitted.
pub fn palw_escrow_row_transition_v1(
    key: &Hash64,
    old: Option<&PalwFundedEscrowRowV1>,
    new: Option<&PalwFundedEscrowRowV1>,
) -> Result<(), PalwEscrowRefusalV1> {
    match (old, new) {
        (None, Some(row)) => {
            row.check_v1()?;
            if row.contract_id != *key || !row.is_fresh_v1() {
                return Err(PalwEscrowRefusalV1::Transition);
            }
            Ok(())
        }
        (Some(row), None) => {
            row.check_v1()?;
            if row.contract_id != *key || row.settled_by.is_none() {
                return Err(PalwEscrowRefusalV1::Transition);
            }
            Ok(())
        }
        (Some(before), Some(after)) => {
            before.check_v1()?;
            after.check_v1()?;
            if before.contract_id != *key || !before.same_core_immutables(after) {
                return Err(PalwEscrowRefusalV1::ImmutableField);
            }
            if before.settled_by.is_some() {
                return Err(PalwEscrowRefusalV1::AlreadySettled);
            }
            if after.paid_sompi < before.paid_sompi || after.burned_sompi < before.burned_sompi || after.returned_sompi < before.returned_sompi {
                return Err(PalwEscrowRefusalV1::Transition);
            }
            if after.settled_by.is_some() {
                // The settling spend consumed exactly the current outpoint.
                if after.funding_outpoint != before.funding_outpoint {
                    return Err(PalwEscrowRefusalV1::ImmutableField);
                }
                return Ok(());
            }
            // Liability clearing: only the flag moves, and only closed.
            let mut cleared = *before;
            cleared.liability_open = false;
            if before.liability_open && *after == cleared {
                return Ok(());
            }
            // Continuation: value consumed, the escrow re-locked on the change-back output.
            if after.funding_outpoint != before.funding_outpoint
                && after.liability_open == before.liability_open
                && after.returned_sompi == before.returned_sompi
                && after.consumed_v1()? > before.consumed_v1()?
            {
                return Ok(());
            }
            Err(PalwEscrowRefusalV1::Transition)
        }
        _ => Err(PalwEscrowRefusalV1::Transition),
    }
}

/// **G14 r5: may a settled row leave the rooted state at `now`?** Only once `now` is more than `retention_daa` past the settling
/// spend's DAA (checked; an overflowing horizon never retires). `retention_daa` is the processor's
/// `PalwTransitionExtrasV1::funded_escrow_retention_daa` (`2 × pruning_depth`: DAA advances at least as fast as blue score, so the
/// pruning point has passed the settling block by then); `None` (below the fence) never retires. An unsettled row never retires.
pub fn palw_escrow_row_retirable_v1(row: &PalwFundedEscrowRowV1, now: u64, retention_daa: Option<u64>) -> bool {
    match (row.settled_by, retention_daa) {
        (Some(_), Some(retention)) => row.settled_daa.checked_add(retention).is_some_and(|horizon| now > horizon),
        _ => false,
    }
}

/// **G14 r5: what the resolution sweep reads about the row's claim** — the fold fills it from the rooted state (open court
/// sessions on the claim, whether the claim record still exists and whether it reached a court verdict, whether a post-Final
/// liability record is still kept). A view, not a caller's word: the sweep builds it from `PalwChainStateV2` only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEscrowClaimLiabilityViewV1 {
    /// `open_courts_of(claim)`: a pursuit is open.
    pub open_courts: u32,
    /// The claim record is still rooted.
    pub claim_live: bool,
    /// The rooted claim reached its court verdict (`Voided`).
    pub claim_voided: bool,
    /// A post-Final liability record (`panel_liabilities`) still keeps the claim convictable.
    pub post_final_liability: bool,
}

/// **G14 r5 (item 2): is the escrow's liability resolved at `now`?** Never while a pursuit is open. Otherwise when:
/// * the hard deadline has passed (`now > hard_deadline_daa`) — no new pursuit can start (X3 hard cap, RFC-0014 amendment);
/// * a court verdict landed: the claim is `Voided` and no post-Final liability record keeps it convictable;
/// * the claim was discharged (RFC-0034 S1 archived close / retirement): no claim record and no liability record remain.
///
/// A `Final` claim with a live liability record, or any live non-terminal claim, stays open until its record goes or the hard
/// deadline passes.
pub fn palw_escrow_liability_resolved_v1(row: &PalwFundedEscrowRowV1, view: &PalwEscrowClaimLiabilityViewV1, now: u64) -> bool {
    if view.open_courts != 0 {
        return false;
    }
    if now > row.hard_deadline_daa {
        return true;
    }
    if view.post_final_liability {
        return false;
    }
    !view.claim_live || view.claim_voided
}

/// The hard deadline of a funded escrow: the adopted X3 hard deadline (`palw_claim_hard_deadline_v1`'s policy; RFC-0014
/// 2026-10-19 amendment `a + 10,000`, RFC-0026 §2, RFC-0031) at the carrier's actual accepting DAA `a`, checked. Unsourced where X3
/// does not govern `a`; never saturated, never defaulted.
pub fn palw_funded_escrow_hard_deadline_v1(
    mirror: Option<crate::palw_ruling_fences_v1::PalwClaimHardDeadlineMirrorV1>,
    accepted_daa: u64,
) -> Result<u64, PalwEscrowRefusalV1> {
    let mirror = mirror.filter(|m| accepted_daa >= m.from_daa).ok_or(PalwEscrowRefusalV1::HardDeadlineUnsourced)?;
    let p = &mirror.policy;
    accepted_daa
        .checked_add(p.prepare_daa)
        .and_then(|d| d.checked_add(p.observe_daa))
        .and_then(|d| d.checked_add(p.resolve_daa))
        .ok_or(PalwEscrowRefusalV1::HardDeadlineUnsourced)
}

/// **G14 r5: what one due is for** (RFC-0035 §7.4/§7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEscrowDueKindV1 {
    /// Panel / relay work payment to the payee that performed it.
    WorkPayment,
    /// Court fee collection to its collector.
    CourtFee,
    /// A burn: destroyed by don't-mint (withheld from the accepting block's collectible fee), never an output.
    Burn,
}

/// **G14 r5: one confirmed due of a contract** — the schedule's derivation of what the escrow owes from the rooted fee receipts,
/// court results and settlement rule. `payee` is the P2PKH-MLDSA87 payload a paying output must carry (`None` exactly for a burn).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEscrowDueV1 {
    pub kind: PalwEscrowDueKindV1,
    pub payee: Option<Hash64>,
    pub amount_sompi: u64,
}

/// **The settlement fee schedule — TYPED-MISSING in production.** Amounts, fees, draw limits, payee shares and the derivation of
/// dues from rooted receipts are not adopted (ADR-0202 承認の範囲; RFC-0035 §7.5 "数値未決なら受付拒否を維持する"). The fields are
/// private and the only constructor is `#[cfg(test)]`, so no production value can exist: every path that needs it refuses with
/// [`PalwEscrowRefusalV1::SettlementScheduleMissing`] ([`palw_escrow_fee_schedule_v1`] always returns `None`). A reviewed change
/// that adopts the numbers adds the production derivation here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwEscrowFeeScheduleV1 {
    contract_id: Hash64,
    /// Every due confirmed so far, in the order they are discharged (cumulative: an earlier settlement's dues stay at the front).
    dues: Vec<PalwEscrowDueV1>,
}

impl PalwEscrowFeeScheduleV1 {
    /// **Tests only**: a schedule for one contract. There is no production constructor.
    #[cfg(test)]
    pub fn test_only_v1(contract_id: Hash64, dues: Vec<PalwEscrowDueV1>) -> Self {
        Self { contract_id, dues }
    }
}

/// **The production schedule source: none.** Always `None` until the numbers are adopted; the processor passes this to every
/// qualified-spend check, so every payee/burn/partial path refuses on every network.
pub fn palw_escrow_fee_schedule_v1(_row: &PalwFundedEscrowRowV1) -> Option<&'static PalwEscrowFeeScheduleV1> {
    None
}

/// Which settlement a qualified spend performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEscrowSpendPathV1 {
    /// The hard deadline has passed with no open liability: the whole unreleased value returns to the payer's rooted payload.
    PayerReturnAfterHardDeadline,
    /// **G14 r5:** the schedule's next dues are paid / burned; the rest either continues as escrow (change-back output, liability
    /// still open) or returns to the payer (liability resolved: the row settles).
    Settlement,
}

/// A qualified spend's plan: the exact escrow input, the row before and after (the transition the fold journals) and what this
/// one spend moved (`paid_now + burned_now + returned_now + continued_now = old unreleased`, checked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEscrowSpendPlanV1 {
    pub input_index: usize,
    pub path: PalwEscrowSpendPathV1,
    pub old: PalwFundedEscrowRowV1,
    pub new: PalwFundedEscrowRowV1,
    pub paid_now: u64,
    /// Destroyed by this spend: the processor withholds it from the block's collectible fee (RFC-0035 §7.1 one allocation of F).
    pub burned_now: u64,
    pub returned_now: u64,
    /// The value re-locked on the change-back output (`new.funding_outpoint`), 0 when the row settled.
    pub continued_now: u64,
}

fn sum_checked<I: IntoIterator<Item = u64>>(values: I) -> Result<u64, PalwEscrowRefusalV1> {
    values.into_iter().try_fold(0u64, |sum, v| sum.checked_add(v)).ok_or(PalwEscrowRefusalV1::Conservation)
}

fn payload_of(output: &crate::tx::TransactionOutput) -> Option<Hash64> {
    crate::mldsa87_primitives::p2pkh_mldsa87_payload(&output.script_public_key)
}

/// **Is `tx`'s spend of `row`'s outpoint qualified at `daa`?** Native check against the rooted row (and, for a settlement, the
/// schedule) only:
/// * exactly one input spends the row's current outpoint, and it is input 0 (audit A-2: one escrow row per transaction), and its
///   populated entry (`input_amounts`, aligned to `tx.inputs`)
///   carries exactly the row's unreleased value;
/// * after the hard deadline with no open liability: the **payer return** — output 0 pays exactly the unreleased value to the
///   rooted return payload (no schedule needed);
/// * otherwise a **settlement**, which needs `schedule` (typed-missing in production ⇒ `SettlementScheduleMissing`): the row's
///   consumed value must be a prefix of the schedule's dues; every remaining due is discharged — a paying due by the next output
///   (exact payee payload and amount, in order), a burn by withholding — then, if value remains, the next output pays it to the
///   rooted payload: a **change-back** that continues the escrow while the liability is open, or the **return** that settles it
///   once the liability is resolved;
/// * every output after the escrow's is funded by the other inputs (`Σ other in ≥ Σ other out`), so the relay's fee and change
///   never draw on the escrow; the burn is the only escrow value inside the fee;
/// * the payload is empty (the payer's own spend) or exactly this row's relay template for the path, with the escrow at input 0.
///
/// This decides authorization of the escrow ROLE only. The input's ordinary script check (the payer's own P2PKH signature) still
/// runs in the normal validator: nothing here skips or caches a signature.
pub fn palw_escrow_qualified_spend_v1(
    row: &PalwFundedEscrowRowV1,
    tx: &Transaction,
    input_amounts: &[u64],
    daa: u64,
    schedule: Option<&PalwEscrowFeeScheduleV1>,
) -> Result<PalwEscrowSpendPlanV1, PalwEscrowRefusalV1> {
    row.check_v1()?;
    if row.settled_by.is_some() {
        return Err(PalwEscrowRefusalV1::AlreadySettled);
    }
    let mut matching = tx.inputs.iter().enumerate().filter(|(_, input)| input.previous_outpoint == row.funding_outpoint);
    let (input_index, _) = matching.next().ok_or(PalwEscrowRefusalV1::NotEscrowInput)?;
    if matching.next().is_some() {
        return Err(PalwEscrowRefusalV1::DuplicateEscrowInput);
    }
    let unreleased = row.unreleased_v1()?;
    if input_amounts.len() != tx.inputs.len() || input_amounts[input_index] != unreleased {
        return Err(PalwEscrowRefusalV1::EntryMismatch);
    }
    let payer_return = !row.liability_open && daa >= row.hard_deadline_daa;
    let path = if payer_return { PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline } else { PalwEscrowSpendPathV1::Settlement };
    // Outputs the escrow funds come first; `next` walks them.
    let mut next = 0usize;
    let mut new = *row;
    let (mut paid_now, mut burned_now, mut returned_now, mut continued_now) = (0u64, 0u64, 0u64, 0u64);
    if payer_return {
        let first = tx.outputs.first().ok_or(PalwEscrowRefusalV1::ReturnOutputMismatch)?;
        if first.value != unreleased || payload_of(first) != Some(row.refund_payload) {
            return Err(PalwEscrowRefusalV1::ReturnOutputMismatch);
        }
        next = 1;
        returned_now = unreleased;
    } else {
        let schedule = schedule.ok_or(PalwEscrowRefusalV1::SettlementScheduleMissing)?;
        if schedule.contract_id != row.contract_id {
            return Err(PalwEscrowRefusalV1::SettlementRuleMismatch);
        }
        // Once-only: the consumed value must end exactly on a due boundary; the dues after it are the ones still owed.
        let consumed = row.consumed_v1()?;
        let mut prefix = 0u64;
        let mut first_open = None;
        for (i, due) in schedule.dues.iter().enumerate() {
            if prefix == consumed {
                first_open = Some(i);
                break;
            }
            prefix = prefix.checked_add(due.amount_sompi).ok_or(PalwEscrowRefusalV1::Conservation)?;
        }
        let first_open = match first_open {
            Some(i) => i,
            None if prefix == consumed => schedule.dues.len(),
            None => return Err(PalwEscrowRefusalV1::SettlementRuleMismatch),
        };
        for due in &schedule.dues[first_open..] {
            if due.amount_sompi == 0 {
                return Err(PalwEscrowRefusalV1::SettlementRuleMismatch);
            }
            match (due.kind, due.payee) {
                (PalwEscrowDueKindV1::Burn, None) => {
                    burned_now = burned_now.checked_add(due.amount_sompi).ok_or(PalwEscrowRefusalV1::Conservation)?;
                }
                (PalwEscrowDueKindV1::WorkPayment | PalwEscrowDueKindV1::CourtFee, Some(payee)) => {
                    let output = tx.outputs.get(next).ok_or(PalwEscrowRefusalV1::PayeeOutputMismatch)?;
                    if output.value != due.amount_sompi || payload_of(output) != Some(payee) {
                        return Err(PalwEscrowRefusalV1::PayeeOutputMismatch);
                    }
                    next += 1;
                    paid_now = paid_now.checked_add(due.amount_sompi).ok_or(PalwEscrowRefusalV1::Conservation)?;
                }
                _ => return Err(PalwEscrowRefusalV1::SettlementRuleMismatch),
            }
        }
        // Conservation: the dues may not exceed what the escrow still holds.
        let remaining = unreleased
            .checked_sub(paid_now)
            .and_then(|v| v.checked_sub(burned_now))
            .ok_or(PalwEscrowRefusalV1::Conservation)?;
        if remaining > 0 {
            let output = tx.outputs.get(next).ok_or(PalwEscrowRefusalV1::ReturnOutputMismatch)?;
            if output.value != remaining || payload_of(output) != Some(row.refund_payload) {
                return Err(PalwEscrowRefusalV1::ReturnOutputMismatch);
            }
            if row.liability_open {
                continued_now = remaining;
                new.funding_outpoint = TransactionOutpoint::new(tx.id(), next as u32);
            } else {
                returned_now = remaining;
            }
            next += 1;
        }
        if paid_now == 0 && burned_now == 0 && returned_now == 0 {
            // A pure change-back (nothing owed, liability still open) only re-locks the same value: refused.
            return Err(PalwEscrowRefusalV1::NothingToSettle);
        }
    }
    // The rest of the transaction is funded by its other inputs: the relay's fee and change never draw on the escrow.
    let other_in = sum_checked(input_amounts.iter().enumerate().filter(|(i, _)| *i != input_index).map(|(_, a)| *a))?;
    let other_out = sum_checked(tx.outputs.iter().skip(next).map(|o| o.value))?;
    if other_in < other_out {
        return Err(PalwEscrowRefusalV1::EscrowFundsOtherOutputs);
    }
    // Third-party relay (RFC-0035 §7.4), narrowest form: empty payload (the payer's own spend) or exactly this row's template
    // for this path, with the escrow at input 0 (see `palw_escrow_relay_template_payload_v1`).
    if !tx.payload.is_empty() && (tx.payload != palw_escrow_relay_template_payload_v1(row.contract_id, row.funding_outpoint, path) || input_index != 0) {
        return Err(PalwEscrowRefusalV1::RelayTemplateMismatch);
    }
    // audit A-2: one escrow row per transaction — the escrow is input 0 for every spend (the payer's own included). Each row's plan
    // walks the outputs from index 0 and counts every other input as funding, so a second escrow input would let two rows be
    // discharged by the same due outputs and alias one change-back outpoint. Exactly one input can be input 0, so at most one row of
    // a transaction is ever qualified; the block filter, the mempool and the template (one rule, `refusal_with_bond_burn`) refuse the
    // transaction for the other row's outpoint, and an escrow input never stands as "foreign" funding of an accepted spend.
    if input_index != 0 {
        return Err(PalwEscrowRefusalV1::EscrowNotFirstInput);
    }
    // Conservation of this one spend, checked: in = paid + burned + returned + continued.
    if sum_checked([paid_now, burned_now, returned_now, continued_now])? != unreleased {
        return Err(PalwEscrowRefusalV1::Conservation);
    }
    new.paid_sompi = new.paid_sompi.checked_add(paid_now).ok_or(PalwEscrowRefusalV1::Conservation)?;
    new.burned_sompi = new.burned_sompi.checked_add(burned_now).ok_or(PalwEscrowRefusalV1::Conservation)?;
    new.returned_sompi = new.returned_sompi.checked_add(returned_now).ok_or(PalwEscrowRefusalV1::Conservation)?;
    if continued_now == 0 {
        new.settled_by = Some(tx.id());
        new.settled_daa = daa;
        // A drained escrow answers for nothing more; the claim's own liability stays on its bond.
        new.liability_open = false;
    }
    new.check_v1()?;
    palw_escrow_row_transition_v1(&row.contract_id, Some(row), Some(&new))?;
    Ok(PalwEscrowSpendPlanV1 { input_index, path, old: *row, new, paid_now, burned_now, returned_now, continued_now })
}

/// The framing of a relay authorization template payload (RFC-0035 §7.4; G14 lane escrow-rfc08 round 2).
pub const PALW_ESCROW_RELAY_TEMPLATE_MAGIC_V1: &[u8; 4] = b"TER1";

/// **The settlement rule's identity** — a domain hash of the path, so a pre-signature for one rule never authorizes another.
pub fn palw_escrow_settlement_rule_id_v1(path: PalwEscrowSpendPathV1) -> Hash64 {
    let name: &[u8] = match path {
        PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline => b"payer-return-after-hard-deadline",
        PalwEscrowSpendPathV1::Settlement => b"scheduled-settlement",
    };
    let digest = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/funded-escrow/settlement-rule/v1").hash(name);
    Hash64::from_bytes(digest.as_bytes().try_into().expect("64-byte settlement rule id"))
}

/// **The relay authorization template payload**: `TER1 ‖ contract_id ‖ funding txid ‖ funding index (u32 LE) ‖ rule id`. A spend that
/// carries it is qualified only for exactly that row and rule ([`palw_escrow_qualified_spend_v1`]); the payer's pre-signature binds
/// it through the ML-DSA sighash's payload hash. Why the template is not inside the TFO1 consent itself: the carrier's txid (and so
/// the outpoint the signature must commit) depends on the TFO1 payload, so the payer signs the template right after building the
/// carrier and distributes it off-chain — still once, at funding time, never a future signature.
pub fn palw_escrow_relay_template_payload_v1(contract_id: Hash64, outpoint: TransactionOutpoint, path: PalwEscrowSpendPathV1) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 64 + 64 + 4 + 64);
    out.extend_from_slice(PALW_ESCROW_RELAY_TEMPLATE_MAGIC_V1);
    out.extend_from_slice(contract_id.as_byte_slice());
    out.extend_from_slice(outpoint.transaction_id.as_byte_slice());
    out.extend_from_slice(&outpoint.index.to_le_bytes());
    out.extend_from_slice(palw_escrow_settlement_rule_id_v1(path).as_byte_slice());
    out
}

/// **The same-mergeset provisional lock**: the outpoint of `tx`'s OWN declared funding output, when `tx` is a TFO1 carrier whose
/// literal payer-signed terms pair to this exact transaction (`PalwTirFundedOriginalEnvelopeV1::inspect_carrier_v1`: payload
/// equality, output index, amount, payer P2PKH return payload). `None` for anything else — a provisional lock never names an
/// outpoint the carrier did not itself create, so an unproven contract cannot lock someone else's coins. The lock lasts for the
/// rest of the mergeset only; the next block reads the rooted rows.
pub fn palw_tfo1_provisional_lock_v1(tx: &Transaction) -> Option<TransactionOutpoint> {
    use crate::palw_tir_funded_original_envelope_v1::{PalwTirFundedOriginalEnvelopeV1, is_funded_original_carrier_v1};
    if !is_funded_original_carrier_v1(tx) {
        return None;
    }
    let envelope = PalwTirFundedOriginalEnvelopeV1::decode_v1(&tx.payload).ok()?;
    envelope.inspect_carrier_v1(tx).ok().map(|facts| facts.outpoint())
}

/// The unsettled rows by outpoint — what the spend filter reads (a settled row's outpoint is already spent).
pub fn palw_escrow_rows_by_outpoint_v1(rows: &BTreeMap<Hash64, PalwFundedEscrowRowV1>) -> HashMap<TransactionOutpoint, PalwFundedEscrowRowV1> {
    rows.values().filter(|row| row.settled_by.is_none()).map(|row| (row.funding_outpoint, *row)).collect()
}

/// **A settlement the processor observed**: an accepted transaction that spent a rooted row's outpoint under a qualified plan. The
/// fold re-derives nothing from the processor's word: it re-runs [`palw_escrow_row_transition_v1`] against its own rooted row and
/// refuses a fact whose `old` is not exactly that row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEscrowSettlementFactV1 {
    pub contract_id: Hash64,
    pub old: PalwFundedEscrowRowV1,
    pub new: PalwFundedEscrowRowV1,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{ScriptPublicKey, TransactionInput, TransactionOutput};

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn payer_payload() -> Hash64 {
        kaspa_hashes::blake2b_512_address_payload(&[9u8; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN])
    }
    fn payer_spk() -> ScriptPublicKey {
        crate::mldsa87_primitives::p2pkh_mldsa87_spk(&payer_payload().as_bytes())
    }
    fn outpoint() -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_u64_word(77), 1)
    }
    fn row(liability_open: bool) -> PalwFundedEscrowRowV1 {
        PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), outpoint(), 1_000, h(3), payer_payload(), 100, 10_100, liability_open).unwrap()
    }
    fn spend(outputs: Vec<TransactionOutput>, extra_inputs: usize) -> Transaction {
        let mut inputs = vec![TransactionInput::new(outpoint(), vec![], 0, 0)];
        for i in 0..extra_inputs {
            inputs.push(TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(500 + i as u64), 0), vec![], 0, 0));
        }
        Transaction::new(0, inputs, outputs, 0, crate::subnets::SUBNETWORK_ID_NATIVE, 0, vec![])
    }

    #[test]
    fn funded_escrow_conservation_violations_are_refused() {
        let mut r = row(false);
        r.paid_sompi = 600;
        r.burned_sompi = 300;
        r.returned_sompi = 200; // 1,100 > 1,000
        assert_eq!(r.unreleased_v1(), Err(PalwEscrowRefusalV1::Conservation));
        assert_eq!(r.check_v1(), Err(PalwEscrowRefusalV1::Conservation));
        let mut overflow = row(false);
        overflow.paid_sompi = u64::MAX;
        assert_eq!(overflow.check_v1(), Err(PalwEscrowRefusalV1::Conservation));
        // A settled row must account for everything; an unsettled row must have moved nothing.
        let mut partial = row(false);
        partial.returned_sompi = 999;
        partial.settled_by = Some(Hash64::from_u64_word(1));
        partial.settled_daa = 200;
        assert_eq!(partial.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
        // G14 r5: an unsettled row may have paid (a continuation) but never returned, never drained, never dated.
        let mut moved = row(false);
        moved.paid_sompi = 1;
        assert_eq!(moved.check_v1(), Ok(()));
        let mut returned = row(false);
        returned.returned_sompi = 1;
        assert_eq!(returned.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
        let mut drained = row(false);
        drained.paid_sompi = 1_000;
        assert_eq!(drained.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
        let mut dated = row(false);
        dated.settled_daa = 5;
        assert_eq!(dated.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
        // A settled row carries its settlement DAA.
        let mut undated = row(false);
        undated.returned_sompi = 1_000;
        undated.settled_by = Some(Hash64::from_u64_word(1));
        assert_eq!(undated.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
        assert_eq!(PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), outpoint(), 0, h(3), payer_payload(), 1, 2, false), Err(PalwEscrowRefusalV1::RowShape));
    }

    #[test]
    fn funded_escrow_payer_return_after_hard_deadline_is_the_only_path_now() {
        let r = row(false);
        let ok = spend(vec![TransactionOutput::new(1_000, payer_spk()), TransactionOutput::new(40, ScriptPublicKey::default())], 1);
        // Before the hard deadline: a settlement, and the schedule is typed-missing.
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &ok, &[1_000, 50], 10_099, None), Err(PalwEscrowRefusalV1::SettlementScheduleMissing));
        // At the deadline with no liability: the payer return qualifies and settles the row once.
        let plan = palw_escrow_qualified_spend_v1(&r, &ok, &[1_000, 50], 10_100, None).unwrap();
        assert_eq!(plan.path, PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline);
        assert_eq!(plan.new.returned_sompi, 1_000);
        assert_eq!(plan.new.settled_by, Some(ok.id()));
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&plan.old), Some(&plan.new)), Ok(()));
        // Once settled, never again.
        assert_eq!(palw_escrow_qualified_spend_v1(&plan.new, &ok, &[1_000, 50], 10_200, None), Err(PalwEscrowRefusalV1::AlreadySettled));
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&plan.new), Some(&plan.new)), Err(PalwEscrowRefusalV1::AlreadySettled));
        // An open liability keeps it a settlement even after the deadline.
        assert_eq!(
            palw_escrow_qualified_spend_v1(&row(true), &ok, &[1_000, 50], 20_000, None),
            Err(PalwEscrowRefusalV1::SettlementScheduleMissing)
        );
    }

    #[test]
    fn funded_escrow_payer_return_must_return_exactly_and_not_fund_others() {
        let r = row(false);
        let short = spend(vec![TransactionOutput::new(999, payer_spk())], 0);
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &short, &[1_000], 10_100, None), Err(PalwEscrowRefusalV1::ReturnOutputMismatch));
        let elsewhere = spend(vec![TransactionOutput::new(1_000, ScriptPublicKey::default())], 0);
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &elsewhere, &[1_000], 10_100, None), Err(PalwEscrowRefusalV1::ReturnOutputMismatch));
        let skims = spend(vec![TransactionOutput::new(1_000, payer_spk()), TransactionOutput::new(60, ScriptPublicKey::default())], 1);
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &skims, &[1_000, 50], 10_100, None), Err(PalwEscrowRefusalV1::EscrowFundsOtherOutputs));
        let wrong_entry = spend(vec![TransactionOutput::new(1_000, payer_spk())], 0);
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &wrong_entry, &[1_001], 10_100, None), Err(PalwEscrowRefusalV1::EntryMismatch));
        let not_ours = Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(5), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(1_000, payer_spk())],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &not_ours, &[1_000], 10_100, None), Err(PalwEscrowRefusalV1::NotEscrowInput));
    }

    #[test]
    fn funded_escrow_transitions_are_install_or_settle_only() {
        let r = row(false);
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, None, Some(&r)), Ok(()));
        assert_eq!(palw_escrow_row_transition_v1(&h(99), None, Some(&r)), Err(PalwEscrowRefusalV1::Transition));
        // An unsettled row never leaves; a non-fresh row is never installed.
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r), None), Err(PalwEscrowRefusalV1::Transition));
        let mut continued = r;
        continued.paid_sompi = 10;
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, None, Some(&continued)), Err(PalwEscrowRefusalV1::Transition));
        let mut moved = r;
        moved.amount_sompi = 2_000;
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r), Some(&moved)), Err(PalwEscrowRefusalV1::ImmutableField));
        // Liability may only close on an unsettled row.
        let open = row(true);
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&open), Some(&r)), Ok(()));
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r), Some(&open)), Err(PalwEscrowRefusalV1::Transition));
    }

    #[test]
    fn funded_escrow_hard_deadline_is_derived_from_x3_or_refused() {
        assert_eq!(palw_funded_escrow_hard_deadline_v1(None, 100), Err(PalwEscrowRefusalV1::HardDeadlineUnsourced));
    }

    #[test]
    fn funded_escrow_provisional_lock_names_only_a_paired_tfo1_own_output() {
        // An ordinary transaction never locks anything, whatever it spends.
        let ordinary = spend(vec![TransactionOutput::new(1_000, payer_spk())], 0);
        assert_eq!(palw_tfo1_provisional_lock_v1(&ordinary), None);
        // A truncated TFO1 framing is a carrier but not a paired contract: no lock.
        let mut truncated = ordinary.clone();
        truncated.subnetwork_id = crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
        truncated.payload = b"TFO1".to_vec();
        truncated.finalize();
        assert_eq!(palw_tfo1_provisional_lock_v1(&truncated), None);
    }

    /// **G14 lane escrow-rfc08 round 2 (RFC-0035 §7.4): a relay presents the payer's pre-signed template.** The template binds the
    /// row's contract id, its exact outpoint and the rule; any other payload, another contract's or outpoint's template, or a
    /// templated spend whose escrow is not input 0 is refused. An empty payload (the payer's own spend) stays qualified.
    #[test]
    fn funded_escrow_relay_template_binds_contract_outpoint_and_rule() {
        let r = row(false);
        let rule = PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline;
        let ret = TransactionOutput::new(1_000, payer_spk());
        let change = TransactionOutput::new(40, ScriptPublicKey::default());
        let relay_in = TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(600), 0), vec![], 0, 0);
        let escrow_in = TransactionInput::new(outpoint(), vec![], 0, 0);
        let tx = |inputs: Vec<TransactionInput>, payload: Vec<u8>| {
            Transaction::new(0, inputs, vec![ret.clone(), change.clone()], 0, crate::subnets::SUBNETWORK_ID_NATIVE, 0, payload)
        };
        let template = palw_escrow_relay_template_payload_v1(r.contract_id, r.funding_outpoint, rule);
        let ok = tx(vec![escrow_in.clone(), relay_in.clone()], template.clone());
        let plan = palw_escrow_qualified_spend_v1(&r, &ok, &[1_000, 50], 10_100, None).unwrap();
        assert_eq!(plan.input_index, 0);
        assert_eq!(plan.new.settled_by, Some(ok.id()));
        // Payer's own spend, no template: still qualified.
        assert!(palw_escrow_qualified_spend_v1(&r, &tx(vec![escrow_in.clone(), relay_in.clone()], vec![]), &[1_000, 50], 10_100, None).is_ok());
        // Another contract's template, another outpoint's template, arbitrary payload, escrow not at input 0.
        let other_contract = palw_escrow_relay_template_payload_v1(h(9), r.funding_outpoint, rule);
        let other_outpoint = palw_escrow_relay_template_payload_v1(r.contract_id, TransactionOutpoint::new(Hash64::from_u64_word(77), 2), rule);
        for payload in [other_contract, other_outpoint, vec![1, 2, 3]] {
            assert_eq!(
                palw_escrow_qualified_spend_v1(&r, &tx(vec![escrow_in.clone(), relay_in.clone()], payload), &[1_000, 50], 10_100, None),
                Err(PalwEscrowRefusalV1::RelayTemplateMismatch)
            );
        }
        assert_eq!(
            palw_escrow_qualified_spend_v1(&r, &tx(vec![relay_in.clone(), escrow_in.clone()], template.clone()), &[50, 1_000], 10_100, None),
            Err(PalwEscrowRefusalV1::RelayTemplateMismatch)
        );
        // Before the hard deadline the template authorizes nothing.
        assert_eq!(
            palw_escrow_qualified_spend_v1(&r, &ok, &[1_000, 50], 10_099, None),
            Err(PalwEscrowRefusalV1::SettlementScheduleMissing)
        );
        // The rule id is domain-separated and stable.
        assert_ne!(palw_escrow_settlement_rule_id_v1(rule), Hash64::default());
        assert_eq!(&template[..4], PALW_ESCROW_RELAY_TEMPLATE_MAGIC_V1);
    }

    /// **No new script or opcode is needed:** under `SIGHASH_SINGLE | SIGHASH_ANYONECANPAY` the payer's ML-DSA sighash of the escrow
    /// input over the template transaction (escrow input 0 → return output 0, template payload) is identical after a relay adds its
    /// own fee input and change output — so the payer's one pre-signature verifies in the relay's transaction under the ordinary
    /// P2PKH check, and changing the payload (another contract/rule) or output 0 changes the sighash.
    #[test]
    fn funded_escrow_relay_presignature_survives_relay_inputs_and_change() {
        use crate::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
        use crate::hashing::sighash_type::{SIG_HASH_ANY_ONE_CAN_PAY, SIG_HASH_SINGLE, SigHashType};
        use crate::tx::{PopulatedTransaction, UtxoEntry};
        let r = row(false);
        let hash_type = SigHashType(SIG_HASH_SINGLE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0);
        let template = palw_escrow_relay_template_payload_v1(r.contract_id, r.funding_outpoint, PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline);
        let escrow_entry = UtxoEntry::new(1_000, payer_spk(), 100, false);
        let relay_entry = UtxoEntry::new(50, ScriptPublicKey::default(), 100, false);
        let ret = TransactionOutput::new(1_000, payer_spk());
        let escrow_in = TransactionInput::new(outpoint(), vec![], 0, 1);
        let presigned = Transaction::new(0, vec![escrow_in.clone()], vec![ret.clone()], 0, crate::subnets::SUBNETWORK_ID_NATIVE, 0, template.clone());
        let relayed = Transaction::new(
            0,
            vec![escrow_in.clone(), TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(600), 0), vec![], 0, 1)],
            vec![ret.clone(), TransactionOutput::new(40, ScriptPublicKey::default())],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            template.clone(),
        );
        let sighash = |tx: &Transaction, entries: Vec<UtxoEntry>| {
            calc_mldsa87_signature_hash(&PopulatedTransaction::new(tx, entries), 0, hash_type, &Mldsa87SigHashReusedValuesUnsync::new())
        };
        let signed = sighash(&presigned, vec![escrow_entry.clone()]);
        assert_eq!(signed, sighash(&relayed, vec![escrow_entry.clone(), relay_entry.clone()]));
        let mut other = relayed.clone();
        other.payload = palw_escrow_relay_template_payload_v1(h(9), r.funding_outpoint, PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline);
        assert_ne!(signed, sighash(&other, vec![escrow_entry.clone(), relay_entry.clone()]));
        let mut theft = relayed.clone();
        theft.outputs[0] = TransactionOutput::new(1_000, ScriptPublicKey::default());
        assert_ne!(signed, sighash(&theft, vec![escrow_entry, relay_entry]));
    }

    // ---- G14 r5 (lane r5-escrow-settlement): every settlement path, liability resolution, retirement ----

    fn party(n: u8) -> Hash64 {
        kaspa_hashes::blake2b_512_address_payload(&[n; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN])
    }
    fn pay(n: u8, value: u64) -> TransactionOutput {
        TransactionOutput::new(value, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&party(n).as_bytes()))
    }
    fn due(kind: PalwEscrowDueKindV1, payee: Option<u8>, amount: u64) -> PalwEscrowDueV1 {
        PalwEscrowDueV1 { kind, payee: payee.map(party), amount_sompi: amount }
    }
    fn spend_of(escrow: TransactionOutpoint, outputs: Vec<TransactionOutput>, relay_inputs: usize) -> Transaction {
        let mut inputs = vec![TransactionInput::new(escrow, vec![], 0, 0)];
        for i in 0..relay_inputs {
            inputs.push(TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(900 + i as u64), 0), vec![], 0, 0));
        }
        Transaction::new(0, inputs, outputs, 0, crate::subnets::SUBNETWORK_ID_NATIVE, 0, vec![])
    }
    fn first_dues() -> Vec<PalwEscrowDueV1> {
        vec![
            due(PalwEscrowDueKindV1::WorkPayment, Some(11), 300),
            due(PalwEscrowDueKindV1::CourtFee, Some(12), 100),
            due(PalwEscrowDueKindV1::Burn, None, 50),
        ]
    }

    /// **Work payment + court fee + burn + change back to escrow (partial payment), then the final payment + return.** Each step is
    /// a qualified spend checked against the rooted row and the schedule, conserves `in = paid + burned + returned + continued`,
    /// and its transition is one the row rules admit; the continuation re-locks the escrow on the change-back output.
    #[test]
    fn funded_escrow_settlement_pays_burns_continues_and_returns_once() {
        let r = row(true);
        let schedule = PalwEscrowFeeScheduleV1::test_only_v1(r.contract_id, first_dues());
        let tx1 = spend_of(outpoint(), vec![pay(11, 300), pay(12, 100), TransactionOutput::new(550, payer_spk()), pay(13, 20)], 1);
        // Production: no schedule exists ⇒ refused.
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &tx1, &[1_000, 20], 500, None), Err(PalwEscrowRefusalV1::SettlementScheduleMissing));
        assert_eq!(palw_escrow_fee_schedule_v1(&r), None);
        let plan = palw_escrow_qualified_spend_v1(&r, &tx1, &[1_000, 20], 500, Some(&schedule)).unwrap();
        assert_eq!(plan.path, PalwEscrowSpendPathV1::Settlement);
        assert_eq!((plan.paid_now, plan.burned_now, plan.returned_now, plan.continued_now), (400, 50, 0, 550));
        let r2 = plan.new;
        assert_eq!(r2.funding_outpoint, TransactionOutpoint::new(tx1.id(), 2), "the escrow continues on the change-back output");
        assert_eq!((r2.paid_sompi, r2.burned_sompi, r2.returned_sompi, r2.settled_by), (400, 50, 0, None));
        assert_eq!(r2.unreleased_v1(), Ok(550));
        assert!(r2.liability_open);
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r), Some(&r2)), Ok(()));
        // The old outpoint is gone: spending it again names nothing.
        assert_eq!(palw_escrow_qualified_spend_v1(&r2, &tx1, &[1_000, 20], 501, Some(&schedule)), Err(PalwEscrowRefusalV1::NotEscrowInput));
        // Once-only: the same dues against the continued row pay nothing more, and a bare change-back is not a settlement.
        let idle = spend_of(r2.funding_outpoint, vec![TransactionOutput::new(550, payer_spk())], 0);
        assert_eq!(palw_escrow_qualified_spend_v1(&r2, &idle, &[550], 501, Some(&schedule)), Err(PalwEscrowRefusalV1::NothingToSettle));
        // The entry must carry the CURRENT unreleased value, not the original amount.
        assert_eq!(palw_escrow_qualified_spend_v1(&r2, &idle, &[1_000], 501, Some(&schedule)), Err(PalwEscrowRefusalV1::EntryMismatch));

        // The liability resolves (the fold's sweep); one more work due, then everything left returns and the row settles.
        let mut r3 = r2;
        r3.liability_open = false;
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r2), Some(&r3)), Ok(()));
        let mut dues = first_dues();
        dues.push(due(PalwEscrowDueKindV1::WorkPayment, Some(11), 100));
        let schedule = PalwEscrowFeeScheduleV1::test_only_v1(r.contract_id, dues);
        let tx2 = spend_of(r3.funding_outpoint, vec![pay(11, 100), TransactionOutput::new(450, payer_spk())], 0);
        let last = palw_escrow_qualified_spend_v1(&r3, &tx2, &[550], 600, Some(&schedule)).unwrap();
        assert_eq!((last.paid_now, last.burned_now, last.returned_now, last.continued_now), (100, 0, 450, 0));
        let done = last.new;
        assert_eq!((done.paid_sompi, done.burned_sompi, done.returned_sompi), (500, 50, 450));
        assert_eq!(done.paid_sompi + done.burned_sompi + done.returned_sompi + done.unreleased_v1().unwrap(), done.amount_sompi);
        assert_eq!((done.settled_by, done.settled_daa, done.liability_open), (Some(tx2.id()), 600, false));
        assert_eq!(done.funding_outpoint, r3.funding_outpoint, "the settling spend consumed the current outpoint");
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r3), Some(&done)), Ok(()));
        assert_eq!(palw_escrow_qualified_spend_v1(&done, &tx2, &[550], 601, Some(&schedule)), Err(PalwEscrowRefusalV1::AlreadySettled));
    }

    /// Every way a settlement can lie is refused by name.
    #[test]
    fn funded_escrow_settlement_refuses_wrong_payees_prefixes_and_overdraws() {
        let r = row(true);
        let schedule = PalwEscrowFeeScheduleV1::test_only_v1(r.contract_id, first_dues());
        let ok_outputs = || vec![pay(11, 300), pay(12, 100), TransactionOutput::new(550, payer_spk())];
        // Another contract's schedule.
        let foreign = PalwEscrowFeeScheduleV1::test_only_v1(h(99), first_dues());
        assert_eq!(
            palw_escrow_qualified_spend_v1(&r, &spend_of(outpoint(), ok_outputs(), 0), &[1_000], 500, Some(&foreign)),
            Err(PalwEscrowRefusalV1::SettlementRuleMismatch)
        );
        // Wrong payee, wrong amount, missing payee output.
        for outputs in [
            vec![pay(14, 300), pay(12, 100), TransactionOutput::new(550, payer_spk())],
            vec![pay(11, 299), pay(12, 100), TransactionOutput::new(551, payer_spk())],
            vec![],
        ] {
            assert_eq!(
                palw_escrow_qualified_spend_v1(&r, &spend_of(outpoint(), outputs, 0), &[1_000], 500, Some(&schedule)),
                Err(PalwEscrowRefusalV1::PayeeOutputMismatch)
            );
        }
        // The change-back must be exact and the payer's.
        for change in [TransactionOutput::new(549, payer_spk()), pay(13, 550)] {
            assert_eq!(
                palw_escrow_qualified_spend_v1(&r, &spend_of(outpoint(), vec![pay(11, 300), pay(12, 100), change], 0), &[1_000], 500, Some(&schedule)),
                Err(PalwEscrowRefusalV1::ReturnOutputMismatch)
            );
        }
        // The burn is inside the fee: an extra output drawing on it is escrow-funded.
        let skim = spend_of(outpoint(), vec![pay(11, 300), pay(12, 100), TransactionOutput::new(550, payer_spk()), pay(13, 50)], 0);
        assert_eq!(palw_escrow_qualified_spend_v1(&r, &skim, &[1_000], 500, Some(&schedule)), Err(PalwEscrowRefusalV1::EscrowFundsOtherOutputs));
        // A consumed value that is not a due boundary (paid twice / out of order).
        let mut off = r;
        off.paid_sompi = 200;
        off.funding_outpoint = TransactionOutpoint::new(h(78), 0);
        assert_eq!(
            palw_escrow_qualified_spend_v1(&off, &spend_of(off.funding_outpoint, vec![TransactionOutput::new(800, payer_spk())], 0), &[800], 500, Some(&schedule)),
            Err(PalwEscrowRefusalV1::SettlementRuleMismatch)
        );
        // Dues beyond what the escrow holds.
        let greedy = PalwEscrowFeeScheduleV1::test_only_v1(r.contract_id, vec![due(PalwEscrowDueKindV1::WorkPayment, Some(11), 1_001)]);
        assert_eq!(
            palw_escrow_qualified_spend_v1(&r, &spend_of(outpoint(), vec![pay(11, 1_001)], 1), &[1_000, 5], 500, Some(&greedy)),
            Err(PalwEscrowRefusalV1::Conservation)
        );
        // A burn with a payee, or a payment without one, is not a due the rule knows.
        let malformed = PalwEscrowFeeScheduleV1::test_only_v1(r.contract_id, vec![due(PalwEscrowDueKindV1::Burn, Some(11), 10)]);
        assert_eq!(
            palw_escrow_qualified_spend_v1(&r, &spend_of(outpoint(), vec![TransactionOutput::new(990, payer_spk())], 0), &[1_000], 500, Some(&malformed)),
            Err(PalwEscrowRefusalV1::SettlementRuleMismatch)
        );
        // A burn-everything settlement drains the row and settles it (nothing left to answer with).
        let all_burn = PalwEscrowFeeScheduleV1::test_only_v1(r.contract_id, vec![due(PalwEscrowDueKindV1::Burn, None, 1_000)]);
        let burned = palw_escrow_qualified_spend_v1(&r, &spend_of(outpoint(), vec![], 0), &[1_000], 500, Some(&all_burn)).unwrap();
        assert_eq!((burned.burned_now, burned.new.burned_sompi, burned.new.liability_open), (1_000, 1_000, false));
        assert!(burned.new.settled_by.is_some());
    }

    /// **Item 2: the liability clears only when no pursuit is open** — on a verdict, a discharge, or past the hard deadline.
    #[test]
    fn funded_escrow_liability_resolves_on_verdict_discharge_or_hard_deadline_only() {
        let r = row(true); // hard deadline 10,100
        let live = PalwEscrowClaimLiabilityViewV1 { open_courts: 0, claim_live: true, claim_voided: false, post_final_liability: false };
        assert!(!palw_escrow_liability_resolved_v1(&r, &live, 5_000), "a live claim keeps its escrow");
        assert!(palw_escrow_liability_resolved_v1(&r, &live, 10_101), "past the hard deadline nothing new can start");
        assert!(!palw_escrow_liability_resolved_v1(&r, &live, 10_100), "the deadline itself is still actionable");
        let pursued = PalwEscrowClaimLiabilityViewV1 { open_courts: 1, ..live };
        assert!(!palw_escrow_liability_resolved_v1(&r, &pursued, 50_000), "an open pursuit holds it even past the deadline");
        let verdict = PalwEscrowClaimLiabilityViewV1 { claim_voided: true, ..live };
        assert!(palw_escrow_liability_resolved_v1(&r, &verdict, 5_000));
        let discharged = PalwEscrowClaimLiabilityViewV1 { claim_live: false, ..live };
        assert!(palw_escrow_liability_resolved_v1(&r, &discharged, 5_000));
        let post_final = PalwEscrowClaimLiabilityViewV1 { claim_live: false, post_final_liability: true, ..live };
        assert!(!palw_escrow_liability_resolved_v1(&r, &post_final, 5_000), "a post-Final liability record keeps it convictable");
    }

    /// **Item 3: a settled row retires only past its retention after the settling spend; an unsettled row never does.**
    #[test]
    fn funded_escrow_rows_retire_only_settled_and_past_retention() {
        let r = row(false);
        assert!(!palw_escrow_row_retirable_v1(&r, u64::MAX, Some(1)));
        let mut settled = r;
        settled.returned_sompi = 1_000;
        settled.settled_by = Some(h(5));
        settled.settled_daa = 10_200;
        settled.check_v1().unwrap();
        assert!(!palw_escrow_row_retirable_v1(&settled, 10_300, Some(100)));
        assert!(palw_escrow_row_retirable_v1(&settled, 10_301, Some(100)));
        assert!(!palw_escrow_row_retirable_v1(&settled, u64::MAX, None), "below the fence nothing retires");
        let mut far = settled;
        far.settled_daa = u64::MAX - 1;
        far.accepted_daa = 0;
        assert!(!palw_escrow_row_retirable_v1(&far, u64::MAX, Some(100)), "an overflowing horizon never retires");
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&settled), None), Ok(()));
        assert_eq!(palw_escrow_row_transition_v1(&h(98), Some(&settled), None), Err(PalwEscrowRefusalV1::Transition));
    }

    /// **audit(a) A-2 reproducer: two escrow inputs in one transaction share the same "escrow-funded" outputs.** Each row's plan
    /// starts its output walk at index 0 and counts every OTHER input (including the second escrow) as relay funding, so one
    /// payee output and one change-back/return output satisfy both rows. The books record two payments; the payee receives one; the
    /// second escrow's value leaves through an unconstrained output (here: back to the payer).
    #[test]
    fn audit_a_two_escrow_inputs_alias_one_set_of_due_outputs() {
        let a = PalwFundedEscrowRowV1::fresh_v1(h(1), h(2), outpoint(), 1_000, h(3), payer_payload(), 100, 10_100, false).unwrap();
        let b_out = TransactionOutpoint::new(Hash64::from_u64_word(78), 0);
        let b = PalwFundedEscrowRowV1::fresh_v1(h(21), h(22), b_out, 1_000, h(3), payer_payload(), 100, 10_100, false).unwrap();
        let dues = || vec![due(PalwEscrowDueKindV1::WorkPayment, Some(11), 300)];
        let sa = PalwEscrowFeeScheduleV1::test_only_v1(a.contract_id, dues());
        let sb = PalwEscrowFeeScheduleV1::test_only_v1(b.contract_id, dues());
        // One payee output (300), one return (700), and 1,000 to the payer: in 2,000 = out 2,000.
        let tx = Transaction::new(
            0,
            vec![TransactionInput::new(a.funding_outpoint, vec![], 0, 0), TransactionInput::new(b.funding_outpoint, vec![], 0, 0)],
            vec![pay(11, 300), TransactionOutput::new(700, payer_spk()), TransactionOutput::new(1_000, payer_spk())],
            0,
            crate::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let pa = palw_escrow_qualified_spend_v1(&a, &tx, &[1_000, 1_000], 500, Some(&sa)).expect("A qualifies");
        let pb = palw_escrow_qualified_spend_v1(&b, &tx, &[1_000, 1_000], 500, Some(&sb));
        // Liability still open: both rows continue on the SAME change-back outpoint (one 700 output backs 2 x 700 unreleased).
        let (ao, bo) = (row(true), PalwFundedEscrowRowV1 { contract_id: h(21), funding_outpoint: b_out, ..row(true) });
        let pao = palw_escrow_qualified_spend_v1(&ao, &tx, &[1_000, 1_000], 500, Some(&sa)).expect("A continues");
        let pbo = palw_escrow_qualified_spend_v1(&bo, &tx, &[1_000, 1_000], 500, Some(&sb));
        let booked_paid = pa.new.paid_sompi + pb.as_ref().map(|p| p.new.paid_sompi).unwrap_or(0);
        let actually_paid: u64 = tx.outputs.iter().filter(|o| payload_of(o) == Some(party(11))).map(|o| o.value).sum();
        let aliased = pbo.as_ref().is_ok_and(|p| p.new.funding_outpoint == pao.new.funding_outpoint);
        eprintln!(
            "[verify-1 A-2] B on A's outputs: {:?}; booked paid {booked_paid}, payee received {actually_paid}; continued rows share one outpoint: {aliased}",
            pb.as_ref().map(|p| (p.paid_now, p.returned_now))
        );
        // Expected: a transaction's outputs discharge at most one escrow row (the second row is refused, or books equal payments).
        assert_eq!(booked_paid, actually_paid, "A-2: two escrow rows booked as paid from ONE payee output");
        assert!(!aliased, "A-2: two continued rows re-locked on ONE outpoint");
    }
}
