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
    /// A settled row with value left, an unsettled row with value moved, or a settled row with liability open.
    OnceOnly,
    /// A transition that rewrites an immutable field (outpoint, amount, payer, return payload, claim, contract, clocks).
    ImmutableField,
    /// A transition this module does not define (a removal, a resettlement, an install of a non-fresh row).
    Transition,
    /// The transaction does not spend this row's outpoint.
    NotEscrowInput,
    /// The transaction spends this row's outpoint more than once.
    DuplicateEscrowInput,
    /// The populated input entry's amount differs from the rooted amount, or the entry list is misaligned.
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
}

/// **The rooted escrow row.** Keyed by `contract_id` in the state. Written only by the funded-original installer and by a
/// qualified spend's settlement, both journaled as one delta entry with `old`/`new` so revert restores row and lock together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwFundedEscrowRowV1 {
    pub version: u16,
    /// The signed funding terms' identity (`FundingTermsV1::id_v1`).
    pub contract_id: Hash64,
    /// The original claim the terms bind.
    pub claim: Hash64,
    /// The actual carrier's funding output — obtained from the final accepted carrier, never from the signed terms.
    pub funding_outpoint: TransactionOutpoint,
    pub amount_sompi: u64,
    pub payer_key_id: Hash64,
    /// The native P2PKH-MLDSA87 address payload the funding output pays (and a payer return must pay).
    pub refund_payload: Hash64,
    /// The carrier's actual accepting DAA.
    pub accepted_daa: u64,
    /// Derived: the adopted X3 hard deadline at `accepted_daa` ([`palw_funded_escrow_hard_deadline_v1`]).
    pub hard_deadline_daa: u64,
    pub paid_sompi: u64,
    pub burned_sompi: u64,
    pub returned_sompi: u64,
    /// A liability this escrow still answers for (a live settlement obligation). Set by the coordinator, never by a spender.
    pub liability_open: bool,
    /// Once-only: the transaction that consumed the outpoint. `Some` exactly when the row is settled.
    pub settled_by: Option<TransactionId>,
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

    /// The row's own invariants: shape, conservation and once-only.
    pub fn check_v1(&self) -> Result<(), PalwEscrowRefusalV1> {
        if self.version != PALW_FUNDED_ESCROW_ROW_VERSION_V1 || self.amount_sompi == 0 || self.accepted_daa > self.hard_deadline_daa {
            return Err(PalwEscrowRefusalV1::RowShape);
        }
        let unreleased = self.unreleased_v1()?;
        match self.settled_by {
            // One UTXO, one spend: the whole value is accounted in that spend, and nothing is owed after it.
            Some(_) if unreleased != 0 || self.liability_open => Err(PalwEscrowRefusalV1::OnceOnly),
            None if unreleased != self.amount_sompi => Err(PalwEscrowRefusalV1::OnceOnly),
            _ => Ok(()),
        }
    }

    fn same_immutables(&self, other: &Self) -> bool {
        (
            self.version,
            self.contract_id,
            self.claim,
            self.funding_outpoint,
            self.amount_sompi,
            self.payer_key_id,
            self.refund_payload,
            self.accepted_daa,
            self.hard_deadline_daa,
        ) == (
            other.version,
            other.contract_id,
            other.claim,
            other.funding_outpoint,
            other.amount_sompi,
            other.payer_key_id,
            other.refund_payload,
            other.accepted_daa,
            other.hard_deadline_daa,
        )
    }
}

/// **The only transitions a row may make in a rooted delta**: install a fresh row (`None → fresh`), or settle it once
/// (`unsettled → settled`, immutables equal, value fully accounted). Removal, resettlement and in-place edits are refused. The
/// delta writer asks this before journaling, so a revert (which swaps `old` and `new`) restores exactly a row this admitted.
pub fn palw_escrow_row_transition_v1(
    key: &Hash64,
    old: Option<&PalwFundedEscrowRowV1>,
    new: Option<&PalwFundedEscrowRowV1>,
) -> Result<(), PalwEscrowRefusalV1> {
    match (old, new) {
        (None, Some(row)) => {
            row.check_v1()?;
            if row.contract_id != *key || row.settled_by.is_some() {
                return Err(PalwEscrowRefusalV1::Transition);
            }
            Ok(())
        }
        (Some(before), Some(after)) => {
            before.check_v1()?;
            after.check_v1()?;
            if !before.same_immutables(after) {
                return Err(PalwEscrowRefusalV1::ImmutableField);
            }
            if before.settled_by.is_some() {
                return Err(PalwEscrowRefusalV1::AlreadySettled);
            }
            if after.settled_by.is_none() {
                // Only the coordinator's liability flag may move on an unsettled row, and only closed.
                if before.liability_open && !after.liability_open && before.paid_sompi == after.paid_sompi {
                    return Ok(());
                }
                return Err(PalwEscrowRefusalV1::Transition);
            }
            Ok(())
        }
        _ => Err(PalwEscrowRefusalV1::Transition),
    }
}

/// **The hard deadline of a funded escrow**: the adopted X3 hard deadline (`palw_claim_hard_deadline_v1`'s policy; RFC-0014
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

/// **The settlement fee schedule — TYPED-MISSING.** Amounts, fees, draw limits and payee shares are not adopted (ADR-0202 承認の範囲;
/// RFC-0035 §7.5 "数値未決なら受付拒否を維持する"). The type has no constructible value, so every path that needs it refuses with
/// [`PalwEscrowRefusalV1::SettlementScheduleMissing`]. A reviewed change that adopts the numbers replaces the private field.
#[derive(Debug)]
#[allow(dead_code)]
pub struct PalwEscrowFeeScheduleV1 {
    unadopted: std::convert::Infallible,
}

/// Which settlement a qualified spend performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEscrowSpendPathV1 {
    /// The hard deadline has passed with no open liability: the whole unreleased value returns to the payer's rooted payload.
    PayerReturnAfterHardDeadline,
}

/// A qualified spend's plan: the exact escrow input and the row before and after (the settlement the fold journals).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEscrowSpendPlanV1 {
    pub input_index: usize,
    pub path: PalwEscrowSpendPathV1,
    pub old: PalwFundedEscrowRowV1,
    pub new: PalwFundedEscrowRowV1,
}

/// **Is `tx`'s spend of `row`'s outpoint qualified at `daa`?** Native check against the rooted row only:
/// * exactly one input spends the row's outpoint, and its populated entry (`input_amounts`, aligned to `tx.inputs`) carries the
///   rooted amount;
/// * before the hard deadline, or while a liability is open, the spend is a settlement and needs `schedule`, which cannot exist
///   yet — refused `SettlementScheduleMissing`;
/// * otherwise (payer return) output 0 pays exactly the unreleased value to the rooted return payload, and the rest of the
///   transaction is funded by its other inputs (`Σ other in ≥ Σ other out`), so neither the relay's fee nor any change is drawn
///   from the escrow.
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
    if input_amounts.len() != tx.inputs.len() || input_amounts[input_index] != row.amount_sompi {
        return Err(PalwEscrowRefusalV1::EntryMismatch);
    }
    if row.liability_open || daa < row.hard_deadline_daa {
        // No `PalwEscrowFeeScheduleV1` can exist (its only field is `Infallible`), so `schedule` is always `None` today; the
        // payee rule is written when the numbers are adopted, never inferred here.
        let _ = schedule;
        return Err(PalwEscrowRefusalV1::SettlementScheduleMissing);
    }
    let unreleased = row.unreleased_v1()?;
    let first = tx.outputs.first().ok_or(PalwEscrowRefusalV1::ReturnOutputMismatch)?;
    if first.value != unreleased || crate::mldsa87_primitives::p2pkh_mldsa87_payload(&first.script_public_key) != Some(row.refund_payload) {
        return Err(PalwEscrowRefusalV1::ReturnOutputMismatch);
    }
    let other_in = input_amounts
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != input_index)
        .try_fold(0u64, |sum, (_, amount)| sum.checked_add(*amount))
        .ok_or(PalwEscrowRefusalV1::Conservation)?;
    let other_out =
        tx.outputs.iter().skip(1).try_fold(0u64, |sum, output| sum.checked_add(output.value)).ok_or(PalwEscrowRefusalV1::Conservation)?;
    if other_in < other_out {
        return Err(PalwEscrowRefusalV1::EscrowFundsOtherOutputs);
    }
    // **Third-party relay (RFC-0035 §7.4), narrowest form.** The escrow output stays the payer's P2PKH, so its ordinary signature
    // check always runs; no input skips it. A relay can still execute the return without a FUTURE payer signature: the payer signs
    // once, when it builds the carrier, a `SIGHASH_SINGLE | SIGHASH_ANYONECANPAY` signature over THIS template (payload bound by the
    // ML-DSA sighash; outpoint, amount and output 0 bound by SINGLE|ANYONECANPAY) and hands it out. Here the payload must be empty
    // (the payer's own spend) or exactly the template for this row's contract, outpoint and rule, with the escrow at input 0.
    if !tx.payload.is_empty()
        && (tx.payload != palw_escrow_relay_template_payload_v1(row.contract_id, row.funding_outpoint, PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline)
            || input_index != 0)
    {
        return Err(PalwEscrowRefusalV1::RelayTemplateMismatch);
    }
    let mut new = *row;
    new.returned_sompi = new.returned_sompi.checked_add(unreleased).ok_or(PalwEscrowRefusalV1::Conservation)?;
    new.settled_by = Some(tx.id());
    new.check_v1()?;
    Ok(PalwEscrowSpendPlanV1 { input_index, path: PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline, old: *row, new })
}

/// The framing of a relay authorization template payload (RFC-0035 §7.4; G14 lane escrow-rfc08 round 2).
pub const PALW_ESCROW_RELAY_TEMPLATE_MAGIC_V1: &[u8; 4] = b"TER1";

/// **The settlement rule's identity** — a domain hash of the path, so a pre-signature for one rule never authorizes another.
pub fn palw_escrow_settlement_rule_id_v1(path: PalwEscrowSpendPathV1) -> Hash64 {
    let name: &[u8] = match path {
        PalwEscrowSpendPathV1::PayerReturnAfterHardDeadline => b"payer-return-after-hard-deadline",
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
        assert_eq!(partial.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
        let mut moved = row(false);
        moved.paid_sompi = 1;
        assert_eq!(moved.check_v1(), Err(PalwEscrowRefusalV1::OnceOnly));
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
        assert_eq!(palw_escrow_row_transition_v1(&r.contract_id, Some(&r), None), Err(PalwEscrowRefusalV1::Transition));
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
}
