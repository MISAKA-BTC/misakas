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
    /// RFC-0009 (`palw_receipt_spend_v4`): a refusal specific to a V4 spend — see [`crate::palw_receipt_v4::PalwReceiptV4Error`].
    #[error("receipt spend V4: {0}")]
    ReceiptV4(#[from] crate::palw_receipt_v4::PalwReceiptV4Error),
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
    check_palw_receipt_spend_admission_v4(state, ctx, receipt_maturity_daa, receipt_use_window_daa, beacon, envelope, None)
}

/// [`check_palw_receipt_spend_admission_v3`] with **ADR-0148's pricing**: `pricing` is the chain's
/// free-prompt pricing (the canonical-work fence's height and the floor), and a compute-era claim's
/// quantum is drawn against the lane's pooled target scaled by the compute the quantum carries
/// ([`crate::palw_state_v2::palw_fp_quantum_receipt_target_v1`]) rather than against its class's
/// own target. `None` — every caller that predates the fence — is the leaves-era rule exactly.
pub fn check_palw_receipt_spend_admission_v4(
    state: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    beacon: &PalwBeaconFactV3,
    envelope: &PalwReceiptSpendEnvelopeV3,
    pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
) -> Result<Hash64, PalwFpAdmissionV3Error> {
    let spend = &envelope.spend;
    let facts = ReceiptSpendFacts {
        network_domain: spend.network_domain,
        claim_id: spend.claim_id,
        quantum_index: spend.quantum_index,
        beacon_block: spend.beacon_block,
    };
    // Items 1-5 are shared with the V4 (RFC-0009) admission; the order is unchanged.
    let claim = receipt_items_1_to_5(state, ctx, receipt_maturity_daa, receipt_use_window_daa, beacon, &facts, pricing)?;

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
    receipt_item_8(state, claim)?;

    Ok(fp_spend_id_v3(spend))
}

/// The four facts of a spend that items 1-5 read — the same four whether the spend is a V3 envelope or an RFC-0009 V4 one.
pub(crate) struct ReceiptSpendFacts {
    pub network_domain: Hash64,
    pub claim_id: Hash64,
    pub quantum_index: u32,
    pub beacon_block: Hash64,
}

/// Items 1-5 of ADR-0044 Decision 6: the claim, the quantum, the beacon, the use window and the lottery. Returns the claim.
pub(crate) fn receipt_items_1_to_5<'a>(
    state: &'a PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    beacon: &PalwBeaconFactV3,
    spend: &ReceiptSpendFacts,
    pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
) -> Result<&'a crate::palw_state_v2::PalwClaimStateV2, PalwFpAdmissionV3Error> {
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
    //    ADR-0148: one expression for both eras, so the admission and the producer's finder read
    //    the same target the chain would.
    let target = match pricing {
        Some(pricing) => crate::palw_state_v2::palw_fp_quantum_receipt_target_v1(state, claim, *quanta, pricing),
        None => state.receipt_target(&claim.class_id).map(|target| target.target),
    }
    .ok_or(PalwFpAdmissionV3Error::ReceiptTargetMissing(claim.class_id))?;
    let ticket = fp_quantum_ticket_v3(spend.network_domain, spend.beacon_block, spend.claim_id, spend.quantum_index);
    if !palw_ticket_admits_v1(ticket, target) {
        return Err(PalwFpAdmissionV3Error::TicketRejected { ticket, target });
    }

    Ok(claim)
}

/// Item 8: the class still stands (exists, not frozen).
pub(crate) fn receipt_item_8(
    state: &PalwChainStateV2,
    claim: &crate::palw_state_v2::PalwClaimStateV2,
) -> Result<(), PalwFpAdmissionV3Error> {
    // 8. The class still stands.
    let class = state.class(&claim.class_id).ok_or(PalwFpAdmissionV3Error::ClassMissing(claim.class_id))?;
    if let PalwClassStatusV2::Frozen { .. } = class.status {
        return Err(PalwFpAdmissionV3Error::ClassFrozen(claim.class_id));
    }
    Ok(())
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
    check_palw_receipt_spend_admission_full_v4(
        state,
        ctx,
        network_domain,
        pre_pow_hash,
        timestamp,
        nonce,
        receipt_maturity_daa,
        receipt_use_window_daa,
        beacon,
        envelope,
        verify_mldsa87,
        None,
    )
}

/// [`check_palw_receipt_spend_admission_full_v3`] with ADR-0148's pricing — the entry point the
/// consensus wiring calls (see [`check_palw_receipt_spend_admission_v4`]).
#[allow(clippy::too_many_arguments)]
pub fn check_palw_receipt_spend_admission_full_v4<V>(
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
    pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
) -> Result<Hash64, PalwFpAdmissionV3Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    envelope.validate_stateless_v3(network_domain, pre_pow_hash, timestamp, nonce)?;
    envelope.validate_signature_v3(verify_mldsa87)?;
    check_palw_receipt_spend_admission_v4(state, ctx, receipt_maturity_daa, receipt_use_window_daa, beacon, envelope, pricing)
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
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 800, 0).unwrap().with_fp_quanta(8, 64).unwrap()
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
                // A canonical job of 160 leaves: the quantum is 20, so 60 leaves are 3 quanta.
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 160 },
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
            job_pin: kaspa_hashes::Hash64::default(),
            eval: None,
            claim: h64(0xFC),
            class_id: h64(1),
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
            executor_pubkey: vec![7; 4],
            work_leaves: 60,
            prompt_token_ids_hash: h64(0x7E),
            // ADR-0145 §5's two execution facts. This fixture folds below the derived-work fence
            // (`PalwTransitionExtrasV1::default()`), where nothing reads them, so they are the
            // honest placeholders a pre-fence object carries and not a claim about this prompt.
            prompt_tokens: 0,
            prompt_token_ids: Vec::new(),
            decode_tokens_executed: 8,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(h64(1)),
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
            signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
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
            job_pin: kaspa_hashes::Hash64::default(),
            eval: None,
            claim: h64(0xFC),
            class_id: h64(1),
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
            executor_pubkey: vec![7; 4],
            work_leaves: 60,
            prompt_token_ids_hash: h64(0x7E),
            // ADR-0145 §5's two execution facts. This fixture folds below the derived-work fence
            // (`PalwTransitionExtrasV1::default()`), where nothing reads them, so they are the
            // honest placeholders a pre-fence object carries and not a claim about this prompt.
            prompt_tokens: 0,
            prompt_token_ids: Vec::new(),
            decode_tokens_executed: 8,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 999_999,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(h64(1)),
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
        // The receipt lane seeds at its own `PALW_RECEIPT_TARGET_SEED_V1`, not at the
        // registration's `initial_target` (the attempt lane's), so the generous and the stingy
        // receipt targets are pinned directly on the state.
        let mut generous = certified_state(u128::MAX);
        generous.set_receipt_target_for_tests(h64(1), u128::MAX);
        let env = spend(0);
        assert!(admit(&generous, &ctx(6, 135, 6), &beacon(), &env).is_ok());

        let mut stingy = certified_state(u128::MAX);
        stingy.set_receipt_target_for_tests(h64(1), 1);
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

    // -----------------------------------------------------------------------------------------------
    // RFC-0009 stage C: the V4 (public redemption) admission, over the same certified fixture.
    // -----------------------------------------------------------------------------------------------

    use crate::palw_receipt_v4::{
        PALW_RECEIPT_V4_BEACON_RULE_SLOT, PALW_RECEIPT_V4_VERSION, PalwReceiptSpendEnvelopeV4, PalwReceiptSpendUnsignedV4,
        PalwReceiptV4Error, PalwRedemptionAuthV4, check_palw_receipt_spend_admission_v5, fp_spend_id_v4, spend_challenge_v4,
    };

    /// The certified fixture plus a SECOND, unrelated bond (bond 2, key `[8; 4]`) — the builder.
    fn certified_with_builder() -> PalwChainStateV2 {
        let p = params();
        let state = certified_state(u128::MAX);
        let builder = PalwConsensusObjectV2::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(2)),
            pubkey: vec![8; 4],
            operator_pubkey: vec![22; 8],
            collateral: 1_000,
            payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A22),
            capable_classes: Default::default(),
            signature: Vec::new(),
        };
        let (with_builder, _) = apply_palw_transition_v2(&state, &p, &ctx(6, 125, 6), &[builder], None).unwrap();
        with_builder
    }

    fn spend_v4(quantum_index: u32) -> PalwReceiptSpendEnvelopeV4 {
        PalwReceiptSpendEnvelopeV4 {
            spend: PalwReceiptSpendUnsignedV4 {
                version: PALW_RECEIPT_V4_VERSION,
                network_domain: h64(999),
                challenge: spend_challenge_v4(h64(999), h64(SPEND_PPH), SPEND_TS, SPEND_NONCE, h64(0xFC), quantum_index, &bond_op(1), &bond_op(2)),
                claim_id: h64(0xFC),
                quantum_index,
                beacon_block: h64(0xBEAC),
                executor_bond: bond_op(1),
                builder_bond: bond_op(2),
                builder_pubkey: vec![8; 4],
                authorization: PalwRedemptionAuthV4 {
                    version: PALW_RECEIPT_V4_VERSION,
                    network_domain: h64(999),
                    claim_id: h64(0xFC),
                    executor_bond: bond_op(1),
                    quantum_lo: 0,
                    quantum_hi: 3,
                    beacon_rule: PALW_RECEIPT_V4_BEACON_RULE_SLOT,
                    builder_fee_bps: 500,
                    expiry_daa: u64::MAX,
                },
                executor_pubkey: vec![7; 4],
                authorization_signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
            builder_signature: vec![0x5A; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }

    fn admit_v4(state: &PalwChainStateV2, c: &PalwBlockContextV2, env: &PalwReceiptSpendEnvelopeV4) -> Result<Hash64, PalwFpAdmissionV3Error> {
        check_palw_receipt_spend_admission_v5(state, c, MATURITY, USE_WINDOW, &beacon(), env, None)
    }

    fn v4_refusal(e: PalwFpAdmissionV3Error) -> PalwReceiptV4Error {
        match e {
            PalwFpAdmissionV3Error::ReceiptV4(inner) => inner,
            other => panic!("expected a V4 refusal, got {other:?}"),
        }
    }

    #[test]
    fn v4_an_honest_redemption_by_another_party_admits_and_returns_its_id() {
        let state = certified_with_builder();
        let env = spend_v4(1);
        let id = admit_v4(&state, &ctx(7, 135, 7), &env).expect("a bonded builder redeems the executor's quantum");
        assert_eq!(id, fp_spend_id_v4(&env.spend));
        // The very same quantum is NOT redeemable under V3 by that builder: V3's rule is untouched.
        let mut v3 = spend(1);
        v3.spend.producer_bond = bond_op(2);
        v3.spend.producer_pubkey = vec![8; 4];
        assert_eq!(admit(&state, &ctx(7, 135, 7), &beacon(), &v3).unwrap_err(), PalwFpAdmissionV3Error::ProducerNotExecutor);
    }

    #[test]
    fn v4_items_one_to_five_and_eight_are_the_v3_ones() {
        let state = certified_with_builder();
        // item 2 range, item 4 window edges, item 3 beacon naming, item 5 ticket: the same helper, the same refusals.
        assert!(matches!(
            admit_v4(&state, &ctx(7, 135, 7), &spend_v4(3)).unwrap_err(),
            PalwFpAdmissionV3Error::QuantumOutOfRange { index: 3, quanta: 3, .. }
        ));
        assert!(matches!(admit_v4(&state, &ctx(7, 181, 7), &spend_v4(0)).unwrap_err(), PalwFpAdmissionV3Error::OutsideUseWindow { .. }));
        let mut wrong_beacon = spend_v4(0);
        wrong_beacon.spend.beacon_block = h64(0xBAD);
        assert!(matches!(admit_v4(&state, &ctx(7, 135, 7), &wrong_beacon).unwrap_err(), PalwFpAdmissionV3Error::BeaconMismatch { .. }));
        let mut stingy = certified_with_builder();
        stingy.set_receipt_target_for_tests(h64(1), 1);
        assert!(matches!(admit_v4(&stingy, &ctx(7, 135, 7), &spend_v4(0)).unwrap_err(), PalwFpAdmissionV3Error::TicketRejected { .. }));
        // A claim that is not Final licenses no redemption either.
        let p = params();
        let genesis = PalwChainStateV2::genesis();
        let (registered, _) = apply_palw_transition_v2(&genesis, &p, &ctx(1, 100, 1), &registrations(u128::MAX), None).unwrap();
        assert_eq!(admit_v4(&registered, &ctx(7, 135, 7), &spend_v4(0)).unwrap_err(), PalwFpAdmissionV3Error::ClaimMissing(h64(0xFC)));
    }

    #[test]
    fn v4_the_executor_must_be_the_claims_executor_standing_and_holding_its_key() {
        let p = params();
        let state = certified_with_builder();
        // The spend (and its authorization) name a real bond that is not the claim's executor.
        let mut foreign = spend_v4(0);
        foreign.spend.executor_bond = bond_op(2);
        foreign.spend.authorization.executor_bond = bond_op(2);
        assert_eq!(v4_refusal(admit_v4(&state, &ctx(7, 135, 7), &foreign).unwrap_err()), PalwReceiptV4Error::ExecutorBondMismatch);
        // The carried executor key is not the bond's registered key.
        let mut wrong_key = spend_v4(0);
        wrong_key.spend.executor_pubkey = vec![9; 4];
        assert_eq!(v4_refusal(admit_v4(&state, &ctx(7, 135, 7), &wrong_key).unwrap_err()), PalwReceiptV4Error::ExecutorKeyMismatch);
        // A retiring executor backs no new blocks (default for RFC §8: spend before you retire, as V3).
        let retire = PalwConsensusObjectV2::BondRetireRequested {
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(1)),
            signature: vec![0xEE; 8],
        };
        let (retiring, _) = apply_palw_transition_v2(&state, &p, &ctx(7, 130, 7), &[retire], None).unwrap();
        assert!(matches!(
            v4_refusal(admit_v4(&retiring, &ctx(8, 135, 8), &spend_v4(0)).unwrap_err()),
            PalwReceiptV4Error::ExecutorBondRetiring(_)
        ));
    }

    #[test]
    fn v4_the_authorization_expires_on_its_own_daa_and_the_boundary_is_inclusive() {
        let state = certified_with_builder();
        let mut env = spend_v4(0);
        env.spend.authorization.expiry_daa = 135;
        assert!(admit_v4(&state, &ctx(7, 135, 7), &env).is_ok(), "the expiry score itself is inside");
        assert_eq!(
            v4_refusal(admit_v4(&state, &ctx(7, 136, 7), &env).unwrap_err()),
            PalwReceiptV4Error::AuthorizationExpired { block_daa: 136, expiry: 135 }
        );
    }

    #[test]
    fn v4_the_builder_must_be_a_standing_bond_holding_the_key_that_signed() {
        let p = params();
        let state = certified_with_builder();
        let mut missing = spend_v4(0);
        missing.spend.builder_bond = bond_op(3);
        assert!(matches!(
            v4_refusal(admit_v4(&state, &ctx(7, 135, 7), &missing).unwrap_err()),
            PalwReceiptV4Error::BuilderBondMissing(_)
        ));
        let mut wrong_key = spend_v4(0);
        wrong_key.spend.builder_pubkey = vec![1; 4];
        assert_eq!(v4_refusal(admit_v4(&state, &ctx(7, 135, 7), &wrong_key).unwrap_err()), PalwReceiptV4Error::BuilderKeyMismatch);
        let retire = PalwConsensusObjectV2::BondRetireRequested {
            bond: crate::palw_state_v2::PalwBondKeyV2(bond_op(2)),
            signature: vec![0xEE; 8],
        };
        let (retiring, _) = apply_palw_transition_v2(&state, &p, &ctx(7, 130, 7), &[retire], None).unwrap();
        assert!(matches!(
            v4_refusal(admit_v4(&retiring, &ctx(8, 135, 8), &spend_v4(0)).unwrap_err()),
            PalwReceiptV4Error::BuilderBondRetiring(_)
        ));
        // The executor may also be its own builder: a V4 spend by the claim's own bond is a valid (if pointless) redemption.
        let mut itself = spend_v4(0);
        itself.spend.builder_bond = bond_op(1);
        itself.spend.builder_pubkey = vec![7; 4];
        assert!(admit_v4(&state, &ctx(7, 135, 7), &itself).is_ok());
    }

    #[test]
    fn v4_the_composed_entry_point_orders_stateless_then_signatures_then_state() {
        use crate::palw_receipt_v4::check_palw_receipt_spend_admission_full_v5;
        let state = certified_with_builder();
        // Shape-valid keys/signatures are 2592/4627 bytes; the fixture's stateful keys are short, so this exercises the ORDER only.
        let env = spend_v4(0);
        let short_keys = check_palw_receipt_spend_admission_full_v5(
            &state,
            &ctx(7, 135, 7),
            h64(999),
            h64(SPEND_PPH),
            SPEND_TS,
            SPEND_NONCE,
            MATURITY,
            USE_WINDOW,
            &beacon(),
            &env,
            |_, _, _, _| true,
            None,
        );
        assert!(matches!(
            short_keys.unwrap_err(),
            PalwFpAdmissionV3Error::ReceiptV4(PalwReceiptV4Error::PublicKeyLength { .. })
        ), "stateless shape comes first");
    }

    /// The point of the whole lane: another party's block spends the executor's winning quantum, ONCE, and the chain's accounting is
    /// exactly what a V3 spend of the same quantum would have produced — weight, census, spent set, and the reorg revert.
    #[test]
    fn v4_spend_by_another_party_folds_identically_to_v3_once_and_reverts() {
        use crate::palw_state_v2::{PalwBlockWorkV3, PalwStateV2Error, apply_palw_transition_v3, revert_delta_v2};
        let p = params();
        let state = certified_with_builder();
        let v4 = spend_v4(1);
        let fold_view = v4.to_fold_envelope();
        let (after_v4, delta) =
            apply_palw_transition_v3(&state, &p, &ctx(8, 135, 8), &[], PalwBlockWorkV3::ReceiptSpend(&fold_view.spend)).unwrap();
        // The same quantum spent by the executor's own V3 block gives the SAME state: nothing in the accounting moved.
        let v3 = spend(1);
        let (after_v3, _) = apply_palw_transition_v3(&state, &p, &ctx(8, 135, 8), &[], PalwBlockWorkV3::ReceiptSpend(&v3.spend)).unwrap();
        assert_eq!(after_v4, after_v3, "V4 and V3 fold to one state: weight, receipt census and the spent set are shared");
        assert!(after_v4.safe_weight() > state.safe_weight(), "the redemption adds the claim's per-quantum weight");
        // Spent once: the same quantum cannot be redeemed again, by anyone, on this chain…
        assert!(matches!(
            admit_v4(&after_v4, &ctx(9, 136, 9), &spend_v4(1)).unwrap_err(),
            PalwFpAdmissionV3Error::QuantumAlreadySpent { index: 1, .. }
        ));
        assert!(matches!(
            apply_palw_transition_v3(&after_v4, &p, &ctx(9, 136, 9), &[], PalwBlockWorkV3::ReceiptSpend(&fold_view.spend)),
            Err(PalwStateV2Error::QuantumAlreadySpent { index: 1, .. })
        ));
        // …while another quantum of the same claim is still redeemable.
        // (The fixture's receipt target retargets at the epoch boundary the spend crossed; pin it open so only the spent set is under test.)
        let mut open = after_v4.clone();
        open.set_receipt_target_for_tests(h64(1), u128::MAX);
        assert_eq!(admit_v4(&open, &ctx(9, 136, 9), &spend_v4(2)).map(|_| ()), Ok(()));
        // Reorg: the delta reverts the spend bit-for-bit.
        assert_eq!(revert_delta_v2(&after_v4, &delta, &p).unwrap(), state);
    }

    /// RFC-0009 stage D, on a populated state: a bond, a class and a claim are each proven against the root a (pinned) header commits, absence is
    /// proven, and every way of lying — a changed row, a missing row, another root, another collection — is refused.
    #[test]
    fn stage_d_a_bond_a_class_and_a_claim_are_proven_against_the_committed_root() {
        use crate::palw_state_proof_v1::{
            PalwProofErrorV1, prove_bonds_v1, prove_claims_v1, prove_classes_v1, verify_bond_v1, verify_claim_v1, verify_class_v1,
        };
        use crate::palw_state_v2::palw_state_root_of_preimage_v1;
        let state = certified_with_builder();
        let root = state.state_root();
        assert_eq!(palw_state_root_of_preimage_v1(&state.state_root_preimage()), root, "the preimage IS what the root hashes");
        let b1 = crate::palw_state_v2::PalwBondKeyV2(bond_op(1));
        let b2 = crate::palw_state_v2::PalwBondKeyV2(bond_op(2));
        let absent = crate::palw_state_v2::PalwBondKeyV2(bond_op(9));

        let bonds = prove_bonds_v1(&state);
        let bond = verify_bond_v1(&bonds, root, &b1).expect("the executor's bond is in the committed state");
        assert_eq!(bond.pubkey, vec![7; 4], "its registered key, proven");
        assert_eq!(verify_bond_v1(&bonds, root, &b2).unwrap().pubkey, vec![8; 4], "and the builder's");
        assert_eq!(verify_bond_v1(&bonds, root, &absent), Err(PalwProofErrorV1::Absent), "absence is a proof too");
        assert_eq!(verify_class_v1(&prove_classes_v1(&state), root, &h64(1)).unwrap().artifact_root, h64(11));
        let claim = verify_claim_v1(&prove_claims_v1(&state), root, &h64(0xFC)).unwrap();
        assert_eq!(claim.bond, b1, "the claim's executor bond, proven");
        assert!(matches!(claim.phase, crate::palw_state_v2::PalwClaimPhaseV2::Final { .. }));

        // Lies. A row changed by one byte (e.g. a swapped key) no longer hashes to a root the state contains.
        let mut changed = bonds.clone();
        let last = changed.collection.rows[0].1.len() - 1;
        changed.collection.rows[0].1[last] ^= 1;
        assert_eq!(verify_bond_v1(&changed, root, &b1), Err(PalwProofErrorV1::CollectionNotInState));
        // A row dropped (to fake absence) or added (to fake presence) changes the root the same way.
        let mut dropped = bonds.clone();
        dropped.collection.rows.pop();
        assert_eq!(verify_bond_v1(&dropped, root, &b2), Err(PalwProofErrorV1::CollectionNotInState));
        // The bond table opened as the class table, and a proof for another state's root.
        assert!(matches!(verify_class_v1(&bonds, root, &h64(1)), Err(PalwProofErrorV1::WrongCollection { .. })));
        let other = certified_state(u128::MAX);
        assert!(matches!(verify_bond_v1(&bonds, other.state_root(), &b1), Err(PalwProofErrorV1::OpeningDoesNotMatchRoot(_))));
    }
}
