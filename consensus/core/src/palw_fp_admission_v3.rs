//! Receipt-block admission — the STATEFUL side of ADR-0044 Decision 6, read entirely from the
//! candidate-scoped [`PalwChainStateV2`] (FP-04).
//!
//! Same two-phase split as the attempt lane, and for the same reason:
//!
//! * **Stateless** (`palw_freeprompt_v3`): version, network domain, sizes, and the signature over
//!   the spend id under the **carried** producer key — zero chain lookups.
//! * **Stateful** (this module): the eight facts below, every one read from the candidate
//!   chain's own state, never the node's sink.
//!
//! ADR-0044 Decision 6's list, in this module's checking order:
//!
//! ```text
//! 1. the claim exists, is a FREE-PROMPT claim, and is Final (certified)
//! 2. quantum_index < quanta, and this quantum is unspent on this chain
//! 3. the carried beacon IS the claim's draw beacon: the beacon fact validates for the slot
//!    final_daa + receipt_maturity_daa, and the spend names that fact's block
//! 4. the block's DAA sits inside [beacon_daa, beacon_daa + receipt_use_window_daa]
//! 5. the quantum ticket admits under the class's receipt target at the CANDIDATE point
//! 6. producer_bond == the claim's executor bond (receipts do not transfer), and that bond is
//!    Active — not retiring (spend before you retire; a retiring bond backs no new blocks)
//! 7. the bond record's pubkey == the carried producer_pubkey
//! 8. the class is Active (not frozen) at the candidate point
//! ```
//!
//! **Item 5's target point, precisely.** The BEACON fixes the draw — the randomness, historical
//! and grind-priced. The TARGET is the spending block's own difficulty context, read at the
//! candidate (parent) point like every difficulty check on this chain: past targets are not
//! state, and "the target as of the beacon" would demand a history the state deliberately does
//! not keep. A marginal ticket can therefore flip eligibility across a retarget boundary inside
//! its use window — deterministically, identically on every node, with no grinding surface
//! (the target is chain-derived) — which is a small economics wobble, not a soundness hole.
//!
//! **The wiring note this module exists to make explicit** (FP-08): for algo 7 the Layer-0
//! finalizer digest binds the header to `Expand(spend_id)` — identity, not lottery. A nonce is
//! free to a receipt producer, so a digest-vs-bits comparison would be a filter the producer
//! grinds through at zero cost while honest software stalls on it; the LOTTERY is item 5, here,
//! and only here. The algo-7 PoW arm must check tag binding and treat the bits comparison as
//! satisfied-by-construction — wiring that gives algo 7 a grindable bits filter has
//! misunderstood the design.
//!
//! Missing facts are errors, never permissive zeros.

use crate::Hash64;
use crate::palw_freeprompt_v3::{
    PalwBeaconFactV3, PalwFpV3Error, PalwReceiptSpendEnvelopeV3, fp_draw_slot_v3, fp_quantum_ticket_v3, fp_spend_id_v3,
    fp_spend_window_contains_v3, validate_beacon_fact_v3,
};
use crate::palw_pwu::palw_ticket_admits_v1;
use crate::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwBondStatusV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClassStatusV2,
};

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwFpAdmissionV3Error {
    #[error("stateless validation failed: {0}")]
    Stateless(#[from] PalwFpV3Error),
    #[error("claim {0} does not exist at the candidate chain point")]
    ClaimMissing(Hash64),
    #[error("claim {0} is not a free-prompt claim — an attempt's work was weighed at its own block")]
    NotFreePrompt(Hash64),
    #[error("claim {0} is not certified (Final) at the candidate chain point — a claim below Final licenses no block")]
    NotCertified(Hash64),
    #[error("claim {claim} has {quanta} quanta; index {index} does not exist")]
    QuantumOutOfRange { claim: Hash64, index: u32, quanta: u32 },
    #[error("claim {claim} quantum {index} is already spent on this chain")]
    QuantumAlreadySpent { claim: Hash64, index: u32 },
    #[error("the draw slot overflows the DAA space — this receipt draws never")]
    DrawSlotOverflow,
    #[error("the carried beacon fact does not hold for the claim's draw slot: {0}")]
    BeaconFactInvalid(PalwFpV3Error),
    #[error("the spend names beacon {named} but the validated fact's beacon is {fact}")]
    BeaconMismatch { named: Hash64, fact: Hash64 },
    #[error("block daa {block_daa} is outside the use window [{beacon_daa}, {beacon_daa} + {window}] — a stale win licenses nothing")]
    OutsideUseWindow { block_daa: u64, beacon_daa: u64, window: u64 },
    #[error("class {0} has no receipt target at the candidate chain point — a missing target admits nothing")]
    ReceiptTargetMissing(Hash64),
    #[error("the quantum ticket {ticket:#034x} does not admit under receipt target {target:#034x}")]
    TicketRejected { ticket: u128, target: u128 },
    #[error("the producer bond is not the claim's executor bond — receipts do not transfer")]
    ProducerNotExecutor,
    #[error("the producer bond {0:?} does not exist at the candidate chain point")]
    BondMissing(PalwBondKeyV2),
    #[error("the producer bond {0:?} is retiring and may back no new blocks")]
    BondRetiring(PalwBondKeyV2),
    #[error("the carried producer key is not the bond record's key — the signature authorises nothing about this bond")]
    BondKeyMismatch,
    #[error("class {0} does not exist at the candidate chain point")]
    ClassMissing(Hash64),
    #[error("class {0} is frozen and admits no new blocks")]
    ClassFrozen(Hash64),
}

/// The stateful admission verdict for one receipt spend against one candidate chain point.
///
/// `beacon` is the pipeline-attested fact for the claim's draw slot (the pipeline that built it
/// asserts, from its own candidate chain, that the named block IS a chain block of the attempt
/// class at the named score and that no attempt-class chain block sits between the slot and it);
/// this function checks everything checkable about it — the slot inequalities and that the spend
/// names ITS beacon and no other.
///
/// Returns the spend id — the identity the header's PoW tag expands — so a caller that admits
/// and then applies cannot recompute a different one in between.
pub fn check_palw_receipt_spend_admission_v3(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    beacon: &PalwBeaconFactV3,
    envelope: &PalwReceiptSpendEnvelopeV3,
) -> Result<Hash64, PalwFpAdmissionV3Error> {
    let spend = &envelope.spend;

    // 1. The claim: exists, free-prompt, certified.
    let claim = state.claim(&spend.claim_id).ok_or(PalwFpAdmissionV3Error::ClaimMissing(spend.claim_id))?;
    let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else {
        return Err(PalwFpAdmissionV3Error::NotFreePrompt(spend.claim_id));
    };
    let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
        return Err(PalwFpAdmissionV3Error::NotCertified(spend.claim_id));
    };

    // 2. The quantum: exists, unspent on this chain.
    if spend.quantum_index >= *quanta {
        return Err(PalwFpAdmissionV3Error::QuantumOutOfRange { claim: spend.claim_id, index: spend.quantum_index, quanta: *quanta });
    }
    if spent.contains(&spend.quantum_index) {
        return Err(PalwFpAdmissionV3Error::QuantumAlreadySpent { claim: spend.claim_id, index: spend.quantum_index });
    }

    // 3. The beacon is the claim's draw beacon — the fact holds for THIS claim's slot, and the
    //    spend names the fact's block (carrying the block in the spend keeps the ticket
    //    recomputable with zero lookups; this equality is what stops it lying).
    let slot = fp_draw_slot_v3(final_daa, receipt_maturity_daa).ok_or(PalwFpAdmissionV3Error::DrawSlotOverflow)?;
    validate_beacon_fact_v3(slot, beacon).map_err(PalwFpAdmissionV3Error::BeaconFactInvalid)?;
    if spend.beacon_block != beacon.beacon_block {
        return Err(PalwFpAdmissionV3Error::BeaconMismatch { named: spend.beacon_block, fact: beacon.beacon_block });
    }

    // 4. The win is used in time (invariant F14).
    if !fp_spend_window_contains_v3(beacon.beacon_daa, receipt_use_window_daa, ctx.daa_score) {
        return Err(PalwFpAdmissionV3Error::OutsideUseWindow {
            block_daa: ctx.daa_score,
            beacon_daa: beacon.beacon_daa,
            window: receipt_use_window_daa,
        });
    }

    // 5. The lottery — the one and only place a receipt block's work is priced (see module doc).
    let target = state.receipt_target(&claim.class_id).ok_or(PalwFpAdmissionV3Error::ReceiptTargetMissing(claim.class_id))?.target;
    let ticket = fp_quantum_ticket_v3(spend.network_domain, spend.beacon_block, spend.claim_id, spend.quantum_index);
    if !palw_ticket_admits_v1(ticket, target) {
        return Err(PalwFpAdmissionV3Error::TicketRejected { ticket, target });
    }

    // 6. Receipts do not transfer: the producer IS the executor, and the bond still stands.
    let producer_key = PalwBondKeyV2(spend.producer_bond);
    if producer_key != claim.bond {
        return Err(PalwFpAdmissionV3Error::ProducerNotExecutor);
    }
    let bond = state.bond(&producer_key).ok_or(PalwFpAdmissionV3Error::BondMissing(producer_key))?;
    if let PalwBondStatusV2::Retiring { .. } = bond.status {
        return Err(PalwFpAdmissionV3Error::BondRetiring(producer_key));
    }

    // 7. The carried key is the bond's key — what turns the stateless signature into authority.
    if bond.pubkey != spend.producer_pubkey {
        return Err(PalwFpAdmissionV3Error::BondKeyMismatch);
    }

    // 8. The class still stands.
    let class = state.class(&claim.class_id).ok_or(PalwFpAdmissionV3Error::ClassMissing(claim.class_id))?;
    if let PalwClassStatusV2::Frozen { .. } = class.status {
        return Err(PalwFpAdmissionV3Error::ClassFrozen(claim.class_id));
    }

    Ok(fp_spend_id_v3(spend))
}

/// The composed admission a wiring layer should call: stateless shape → stateless signature →
/// the stateful list, in that order, one entry point.
#[allow(clippy::too_many_arguments)]
pub fn check_palw_receipt_spend_admission_full_v3<V>(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    pre_pow_hash: Hash64,
    timestamp: u64,
    nonce: u64,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    beacon: &PalwBeaconFactV3,
    envelope: &PalwReceiptSpendEnvelopeV3,
    verify_mldsa87: V,
) -> Result<Hash64, PalwFpAdmissionV3Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    envelope.validate_stateless_v3(network_domain, pre_pow_hash, timestamp, nonce)?;
    envelope.validate_signature_v3(verify_mldsa87)?;
    check_palw_receipt_spend_admission_v3(state, ctx, receipt_maturity_daa, receipt_use_window_daa, beacon, envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_freeprompt_v3::{PALW_FP_V3_VERSION, PalwReceiptSpendUnsignedV3};
    use crate::palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2, PalwStateParamsV2,
        apply_palw_transition_v2,
    };
    use crate::tx::{TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64 as H;

    const MATURITY: u64 = 5;
    const USE_WINDOW: u64 = 50;

    fn h64(v: u64) -> Hash64 {
        H::from_u64_word(v)
    }

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 800, 0).unwrap()
    }

    fn bond_op(v: u64) -> TransactionOutpoint {
        TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 }
    }

    fn ctx(block_word: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: h64(block_word), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn registrations(initial_target: u128) -> Vec<PalwConsensusObjectV2> {
        vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ]
    }

    /// Register (target = MAX: every ticket admits), commit an FP claim (pwu 60, 3 quanta) and
    /// walk it to Final at daa 124. Returns the certified state.
    fn certified_state(initial_target: u128) -> PalwChainStateV2 {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (s1, _) = apply_palw_transition_v2(&genesis, &p, &ctx(1, 100, 1), &registrations(initial_target), None).unwrap();
        let commit = PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(0xFC),
            class_id: h64(1),
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
            executor_pubkey: vec![7; 4],
            pwu: 60,
            quanta: 3,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
        };
        let (s2, _) = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[commit], None).unwrap();
        let seats = vec![PalwPanelSeatV2 { bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)), operator_id: h64(90) }];
        let bind = PalwConsensusObjectV2::PanelBound { claim: h64(0xFC), anchor: h64(77), seats };
        let (s3, _) = apply_palw_transition_v2(&s2, &p, &ctx(3, 102, 3), &[bind], None).unwrap();
        let license = PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xFC), receipts: Vec::new() };
        let (s4, _) = apply_palw_transition_v2(&s3, &p, &ctx(4, 103, 4), &[license], None).unwrap();
        let (s5, _) = apply_palw_transition_v2(&s4, &p, &ctx(5, 124, 5), &[], None).unwrap();
        assert!(matches!(s5.claim(&h64(0xFC)).unwrap().phase, crate::palw_state_v2::PalwClaimPhaseV2::Final { .. }));
        s5
    }

    /// The certified fixture reaches Final at daa 124, so the draw slot is 124 + MATURITY = 129:
    /// a beacon at daa 130 whose predecessor attempt block sat at daa 120 is valid for it.
    fn beacon() -> PalwBeaconFactV3 {
        PalwBeaconFactV3 { beacon_block: h64(0xBEAC), beacon_daa: 130, prev_attempt_daa: 120 }
    }

    /// The header position the spend fixtures bind.
    const SPEND_PPH: u64 = 0xB0;
    const SPEND_TS: u64 = 1_700;
    const SPEND_NONCE: u64 = 9;

    fn spend(quantum_index: u32) -> PalwReceiptSpendEnvelopeV3 {
        PalwReceiptSpendEnvelopeV3 {
            spend: PalwReceiptSpendUnsignedV3 {
                version: PALW_FP_V3_VERSION,
                network_domain: h64(999),
                challenge: crate::palw_freeprompt_v3::spend_challenge_v3(
                    h64(999),
                    h64(SPEND_PPH),
                    SPEND_TS,
                    SPEND_NONCE,
                    h64(0xFC),
                    quantum_index,
                    &bond_op(1),
                ),
                claim_id: h64(0xFC),
                quantum_index,
                beacon_block: h64(0xBEAC),
                producer_bond: bond_op(1),
                producer_pubkey: vec![7; 4],
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn admit(
        state: &PalwChainStateV2,
        c: &PalwBlockContextV2,
        b: &PalwBeaconFactV3,
        env: &PalwReceiptSpendEnvelopeV3,
    ) -> Result<Hash64, PalwFpAdmissionV3Error> {
        check_palw_receipt_spend_admission_v3(state, c, MATURITY, USE_WINDOW, b, env)
    }

    /// The eight-item list admits an honest spend and returns the id the PoW tag expands.
    #[test]
    fn an_honest_spend_admits_and_returns_its_id() {
        let state = certified_state(u128::MAX);
        let env = spend(0);
        let id = admit(&state, &ctx(6, 135, 6), &beacon(), &env).expect("the honest spend admits");
        assert_eq!(id, fp_spend_id_v3(&env.spend));
    }

    /// Item 1: absent claim, wrong source, and uncertified phase are three different refusals.
    #[test]
    fn item_1_claim_existence_source_and_phase() {
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (registered, _) = apply_palw_transition_v2(&genesis, &p, &ctx(1, 100, 1), &registrations(u128::MAX), None).unwrap();

        assert_eq!(
            admit(&registered, &ctx(6, 135, 6), &beacon(), &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::ClaimMissing(h64(0xFC))
        );

        // Committed but not certified.
        let commit = PalwConsensusObjectV2::FreePromptCommitted {
            claim: h64(0xFC),
            class_id: h64(1),
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
            executor_pubkey: vec![7; 4],
            pwu: 60,
            quanta: 3,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
        };
        let (pending, _) = apply_palw_transition_v2(&registered, &p, &ctx(2, 101, 2), &[commit], None).unwrap();
        assert_eq!(
            admit(&pending, &ctx(3, 102, 3), &beacon(), &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::NotCertified(h64(0xFC))
        );
    }

    /// Item 2: out-of-range and already-spent quanta are named, and the spent set the STATE
    /// carries is what this check reads — one ledger, no second copy.
    #[test]
    fn item_2_quantum_range_and_double_spend() {
        let p = params();
        let state = certified_state(u128::MAX);
        assert!(matches!(
            admit(&state, &ctx(6, 135, 6), &beacon(), &spend(3)).unwrap_err(),
            PalwFpAdmissionV3Error::QuantumOutOfRange { index: 3, quanta: 3, .. }
        ));

        // Spend 0 through the transition, then try to admit it again on the child chain point.
        let env = spend(0);
        let (spent_state, _) = crate::palw_state_v2::apply_palw_transition_v3(
            &state,
            &p,
            &ctx(6, 135, 6),
            &[],
            crate::palw_state_v2::PalwBlockWorkV3::ReceiptSpend(&env.spend),
        )
        .unwrap();
        assert!(matches!(
            admit(&spent_state, &ctx(7, 136, 7), &beacon(), &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::QuantumAlreadySpent { index: 0, .. }
        ));
        // A different quantum of the same receipt still admits.
        assert!(admit(&spent_state, &ctx(7, 136, 7), &beacon(), &spend(1)).is_ok());
    }

    /// Item 3: a beacon fact from the wrong slot, and a spend naming a different block than the
    /// validated fact, are both refused.
    #[test]
    fn item_3_beacon_binding() {
        let state = certified_state(u128::MAX);
        // The fact's beacon sits BEFORE the claim's slot (129).
        let early = PalwBeaconFactV3 { beacon_block: h64(0xBEAC), beacon_daa: 128, prev_attempt_daa: 120 };
        assert!(matches!(
            admit(&state, &ctx(6, 135, 6), &early, &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::BeaconFactInvalid(PalwFpV3Error::BeaconBeforeSlot { .. })
        ));
        // An attempt block already occupied the slot — the named beacon is not the first.
        let not_first = PalwBeaconFactV3 { beacon_block: h64(0xBEAC), beacon_daa: 130, prev_attempt_daa: 129 };
        assert!(matches!(
            admit(&state, &ctx(6, 135, 6), &not_first, &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::BeaconFactInvalid(PalwFpV3Error::BeaconNotFirst { .. })
        ));
        // The spend names a block that is not the fact's beacon.
        let mut env = spend(0);
        env.spend.beacon_block = h64(0xBAD);
        assert!(matches!(admit(&state, &ctx(6, 135, 6), &beacon(), &env).unwrap_err(), PalwFpAdmissionV3Error::BeaconMismatch { .. }));
    }

    /// Item 4: the use window's ends are exact — the beacon's own score is in, one past the far
    /// end is out (invariant F14: a stale win licenses nothing).
    #[test]
    fn item_4_use_window_edges() {
        let state = certified_state(u128::MAX);
        assert!(admit(&state, &ctx(6, 130, 6), &beacon(), &spend(0)).is_ok(), "the beacon's own score is inside");
        assert!(admit(&state, &ctx(6, 180, 6), &beacon(), &spend(0)).is_ok(), "the far end is inclusive");
        assert!(matches!(
            admit(&state, &ctx(6, 181, 6), &beacon(), &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::OutsideUseWindow { block_daa: 181, beacon_daa: 130, window: USE_WINDOW }
        ));
        assert!(matches!(
            admit(&state, &ctx(6, 129, 6), &beacon(), &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::OutsideUseWindow { block_daa: 129, .. }
        ));
    }

    /// Item 5: the ticket is compared against the RECEIPT target — a tiny target refuses the
    /// same spend a full target admits, and the refusal carries both numbers.
    #[test]
    fn item_5_ticket_against_the_receipt_target() {
        let generous = certified_state(u128::MAX);
        let env = spend(0);
        assert!(admit(&generous, &ctx(6, 135, 6), &beacon(), &env).is_ok());

        let stingy = certified_state(1);
        let ticket = fp_quantum_ticket_v3(h64(999), h64(0xBEAC), h64(0xFC), 0);
        assert!(ticket > 1, "the fixture's ticket must actually lose against target 1");
        assert_eq!(
            admit(&stingy, &ctx(6, 135, 6), &beacon(), &env).unwrap_err(),
            PalwFpAdmissionV3Error::TicketRejected { ticket, target: 1 }
        );
    }

    /// Items 6–8: a foreign producer bond, a mismatched key, and a frozen class each refuse.
    #[test]
    fn items_6_7_8_producer_bond_key_and_class() {
        let p = params();
        let state = certified_state(u128::MAX);

        // 6. Receipts do not transfer.
        let mut foreign = spend(0);
        foreign.spend.producer_bond = bond_op(2);
        assert_eq!(admit(&state, &ctx(6, 135, 6), &beacon(), &foreign).unwrap_err(), PalwFpAdmissionV3Error::ProducerNotExecutor);

        // 6b. A retiring bond backs no new blocks.
        let retire = PalwConsensusObjectV2::BondRetireRequested {
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
            signature: vec![0xEE; 8],
        };
        let (retiring, _) = apply_palw_transition_v2(&state, &p, &ctx(6, 130, 6), &[retire], None).unwrap();
        assert!(matches!(
            admit(&retiring, &ctx(7, 135, 7), &beacon(), &spend(0)).unwrap_err(),
            PalwFpAdmissionV3Error::BondRetiring(_)
        ));

        // 7. The carried key must be the bond's key.
        let mut wrong_key = spend(0);
        wrong_key.spend.producer_pubkey = vec![9; 4];
        assert_eq!(admit(&state, &ctx(6, 135, 6), &beacon(), &wrong_key).unwrap_err(), PalwFpAdmissionV3Error::BondKeyMismatch);

        // 8. A frozen class admits no new blocks — certified receipts included; the freeze is
        //    the chain saying this class's arithmetic is in doubt.
        //    The floor itself may not be frozen (ADR-0039 W6′ — that would end the chain), so the
        //    class under test is registered as an entrant and the claim is made against it.
        let entrant = crate::palw_state_v2::tests::entrant_class(h64(2), 500);
        let freeze = crate::palw_state_v2::tests::freeze(h64(2));
        let (with_entrant, _) = apply_palw_transition_v2(&state, &p, &ctx(6, 130, 6), &[entrant], None).unwrap();
        let (frozen, _) = apply_palw_transition_v2(&with_entrant, &p, &ctx(7, 131, 7), &[freeze], None).unwrap();
        assert_eq!(
            frozen.class(&h64(2)).map(|c| matches!(c.status, crate::palw_state_v2::PalwClassStatusV2::Frozen { .. })),
            Some(true),
            "the entrant is frozen, which is the state the admission item reads"
        );
        // And the floor is refused outright rather than frozen.
        assert!(matches!(
            apply_palw_transition_v2(&state, &p, &ctx(6, 130, 6), &[crate::palw_state_v2::tests::freeze(h64(1))], None),
            Err(crate::palw_state_v2::PalwStateV2Error::BaseClassMayNotFreeze(_))
        ));
    }

    /// The composed entry point runs stateless first: a foreign-network spend never reaches a
    /// chain lookup, and a bad signature is named before any state is read.
    #[test]
    fn the_composed_entry_point_orders_its_refusals() {
        let state = certified_state(u128::MAX);
        let mut env = spend(0);
        env.spend.network_domain = h64(0x99);
        let refused = check_palw_receipt_spend_admission_full_v3(
            &state,
            &ctx(6, 135, 6),
            h64(999),
            h64(SPEND_PPH),
            SPEND_TS,
            SPEND_NONCE,
            MATURITY,
            USE_WINDOW,
            &beacon(),
            &env,
            |_, _, _, _| true,
        );
        assert_eq!(refused.unwrap_err(), PalwFpAdmissionV3Error::Stateless(PalwFpV3Error::NetworkDomainMismatch));

        let honest = spend(0);
        let rejected_signature = check_palw_receipt_spend_admission_full_v3(
            &state,
            &ctx(6, 135, 6),
            h64(999),
            h64(SPEND_PPH),
            SPEND_TS,
            SPEND_NONCE,
            MATURITY,
            USE_WINDOW,
            &beacon(),
            &honest,
            |_, _, _, _| false,
        );
        assert_eq!(rejected_signature.unwrap_err(), PalwFpAdmissionV3Error::Stateless(PalwFpV3Error::SignatureInvalid));

        let admitted = check_palw_receipt_spend_admission_full_v3(
            &state,
            &ctx(6, 135, 6),
            h64(999),
            h64(SPEND_PPH),
            SPEND_TS,
            SPEND_NONCE,
            MATURITY,
            USE_WINDOW,
            &beacon(),
            &honest,
            |_, _, _, _| true,
        );
        assert_eq!(admitted.unwrap(), fp_spend_id_v3(&honest.spend));
    }
}
