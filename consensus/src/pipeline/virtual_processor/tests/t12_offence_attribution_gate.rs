//! **ADR-0152 v2 F2's object gate on testnet-12** (spec §3.4): `Obj::ObjectiveOffence` through the
//! processor's own `palw_v2_validate_objects`, its acceptance walk and its fold, on the shipped
//! ruleset with harness keys (`t12_round_lane_e2e::t12_with_harness_cards`) — against the same
//! ruleset with `palw_offence_attribution` unset, which is the rule every other preset keeps.
//!
//! Past the fence the V1 `PanelFalseValid` is refused by name and `PanelFalseValidV2` is judged by
//! `palw_check_panel_false_valid_v2` with the signature half: the receipt under THIS chain's domain
//! and the key the accused bond registered, in the V2 or V3 form. Below it the V2 kind is dormant
//! and the V1 kind reaches the gate it always reached. No block is mined: the gate is asked on the
//! genesis state the processor stored at construction, and — for the conviction — on that state
//! with one retired claim's liability row written through the carriage (a `CourtFraud` void the
//! chain recorded, listing one card as a `Valid` signer), so what the gate, the walk and the fold
//! each decide is all that is measured. Convictions on a real claim (a producer-built fault, a
//! bound panel, a segmented licence) are spec §3.6's T46 suite.
use super::TestContext;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1, PalwPanelFalseValidEvidenceV2, palw_false_valid_offence_id_v2,
};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwOffenceVerifyError, PalwPanelContradictionV1,
    PalwPanelFalseValidEvidenceV1, palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3,
    palw_receipt_message_v2, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateCarriageV2, PalwVoidReasonV2,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_hashes::Hash64;

/// The DAA the recorded void names.
const VOIDED_DAA: u64 = 7;

struct Gate {
    ctx: TestContext,
    bundle: PalwConsensusParamsV2,
    state: PalwChainStateV2,
    point: PalwBlockContextV2,
    /// `palw_network_domain_v2_for(network id, genesis)` — what the processor derives.
    domain: Hash64,
    /// The genesis cards' bond keys, in registry order; card `i` signs with
    /// `palw_v2_registry_keypair(i)`.
    cards: Vec<PalwBondKeyV2>,
}

/// testnet-12 with harness cards, at genesis; `armed = false` unsets `palw_offence_attribution`.
fn gate(armed: bool) -> Gate {
    let (config, bundle, _premine, _floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
    assert!(config.params.palw_offence_attribution.is_some_and(|f| f.is_active(0)), "testnet-12 arms the fence from genesis");
    let config: Config = if armed {
        config
    } else {
        let mut params = config.params.clone();
        params.palw_offence_attribution = None;
        // ADR-0152: R-core+ is armed above this fence on testnet-12 and refuses to stand without
        // it, so the fence-off twin takes R-core+ off too (every R-core+ writer is dormant, so the
        // twin folds exactly as it did before the v22 skeleton).
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    let ctx = TestContext::new(TestConsensus::new(&config));
    let (_, state) =
        ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let cards: Vec<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    for (i, card) in cards.iter().enumerate() {
        assert_eq!(
            state.bond(card).expect("a genesis card").pubkey,
            TestConsensus::palw_v2_registry_keypair(i as u64).verification_key.as_ref().to_vec(),
            "card {i} carries its harness key"
        );
    }
    let point = PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: ctx.consensus.get_virtual_daa_score() + 10,
        blue_score: 5,
        subsidy: 0,
    };
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    Gate { ctx, bundle, state, point, domain, cards }
}

impl Gate {
    fn validate(&self, state: &PalwChainStateV2, object: &Obj) -> Result<(), String> {
        self.ctx.consensus.virtual_processor().palw_v2_validate_objects(
            state,
            &self.bundle.state,
            &self.point,
            std::slice::from_ref(object),
        )
    }

    fn accepted(&self, state: &PalwChainStateV2, object: &Obj) -> Vec<Obj> {
        self.ctx.consensus.virtual_processor().palw_v2_accepted_objects_for_tests(
            state,
            &self.bundle.state,
            &self.point,
            vec![object.clone()],
            self.point.block,
        )
    }

    /// Card `card`'s `Valid` on `claim`, signed under `domain` in the full (V2) form.
    fn full(&self, card: usize, claim: Hash64, domain: Hash64) -> PalwSeatReceiptV2 {
        let message = palw_receipt_message_v2(domain, claim, PalwReceiptVerdictV2::Valid, 0);
        PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: self.cards[card],
            signed_daa: 0,
            signature: sign(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        }
    }

    /// The same `Valid`, signed in the segmented (V3) form over `mask`.
    fn segmented(&self, card: usize, claim: Hash64, mask: PalwSegmentMaskV2) -> PalwSeatReceiptV3 {
        let message = palw_receipt_message_v3(self.domain, claim, PalwReceiptVerdictV2::Valid, 0, mask);
        let mut receipt = self.full(card, claim, self.domain);
        receipt.signature = sign(card, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT);
        PalwSeatReceiptV3 { receipt, segments: mask }
    }

    fn v2(&self, card: usize, claim: Hash64, receipt: PalwFalseValidReceiptV1, contradiction: PalwPanelContradictionV1) -> Obj {
        let payload = PalwPanelFalseValidEvidenceV2 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V2,
            claim_id: claim,
            accused_seat: self.cards[card].0,
            receipt,
            contradiction,
            reporter_reveal: Vec::new(),
        };
        offence(PalwOffenceKindV1::PanelFalseValidV2, self.cards[card], borsh::to_vec(&payload).unwrap())
    }

    fn v1(&self, card: usize, claim: Hash64, contradiction: PalwPanelContradictionV1) -> Obj {
        let payload = PalwPanelFalseValidEvidenceV1 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V1,
            claim_id: claim,
            network_domain: self.domain,
            accused_seat: self.cards[card].0,
            valid_receipt: self.full(card, claim, self.domain),
            executor_pubkey: Vec::new(),
            contradiction,
        };
        offence(PalwOffenceKindV1::PanelFalseValid, self.cards[card], borsh::to_vec(&payload).unwrap())
    }

    /// The genesis state with claim `claim` retired as `CourtFraud`-voided at [`VOIDED_DAA`], its
    /// liability row listing `signers` as `Valid` signers (card 0 its executor).
    fn with_voided_row(&self, claim: Hash64, signers: &[usize]) -> PalwChainStateV2 {
        let mut carriage = PalwStateCarriageV2::from_state(&self.state);
        carriage.panel_liabilities.insert(
            claim,
            kaspa_consensus_core::palw_panel_var_v1::PalwPanelLiabilityRecordV1 {
                claim_id: claim,
                work_id: Hash64::from_u64_word(0xF2_0E),
                class_id: self.bundle.base_class_id,
                execution_root: Hash64::from_u64_word(0xF2_0E),
                output_root: Hash64::from_u64_word(0xF2_00),
                executor_bond: self.cards[0],
                voided_daa: Some(VOIDED_DAA),
                void_reason: Some(PalwVoidReasonV2::CourtFraud),
                valid_signers: signers.iter().map(|i| (self.cards[*i].0, claim)).collect(),
                locked_sompi: 0,
                expiry_daa: 1_000_000,
                settled_at_final: 0,
                job_identity: kaspa_consensus_core::Hash64::default(),
                free_prompt: false,
                trace_root: kaspa_consensus_core::Hash64::default(),
                segment_count: 0,
                licence_door: None,
                basis_k: 0,
                g_res_sompi: 0,
                escrowed_reward: 0,
            },
        );
        carriage.into_state(&self.bundle.state, None).expect("consistent")
    }
}

fn sign(card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&TestConsensus::palw_v2_registry_keypair(card as u64).signing_key, message, context, [0xF2u8; 32])
        .expect("sign")
        .as_ref()
        .to_vec()
}

fn offence(kind: PalwOffenceKindV1, accused: PalwBondKeyV2, evidence: Vec<u8>) -> Obj {
    Obj::ObjectiveOffence { kind, accused, evidence_id: palw_offence_evidence_digest_v1(&evidence), evidence }
}

/// **The fence routes the two kinds.** Past it the V1 kind is refused by name and the V2 kind is
/// judged by the adjudicator — the chain's domain, both receipt forms, the digest; below it the V2
/// kind is dormant and the V1 kind reaches the V1 gate as it always did.
#[tokio::test]
async fn f2_the_gate_routes_the_v1_and_v2_kinds_by_the_fence() {
    let claim = Hash64::from_u64_word(0xF2_C1A1);
    let court_fraud = PalwPanelContradictionV1::CourtFraud { voided_daa: VOIDED_DAA };
    let armed = gate(true);
    let refused = |g: &Gate, object: &Obj| g.validate(&g.state, object).expect_err("refused");

    // Past the fence: the V1 kind, whatever it carries, is superseded — and dropped by the walk.
    let v1 = armed.v1(1, claim, court_fraud.clone());
    assert_eq!(refused(&armed, &v1), PalwOffenceVerifyError::SupersededOnThisNetwork.to_string());
    assert!(armed.accepted(&armed.state, &v1).is_empty(), "the walk drops it with the block standing");

    // The V2 kind reaches the adjudicator: a receipt signed under this chain's domain, in either
    // form, verifies — and is then refused only because genesis holds no such claim.
    let full = armed.v2(1, claim, PalwFalseValidReceiptV1::Full(armed.full(1, claim, armed.domain)), court_fraud.clone());
    assert_eq!(refused(&armed, &full), PalwOffenceVerifyError::NoTarget.to_string());
    let segmented = armed.v2(
        1,
        claim,
        PalwFalseValidReceiptV1::Segmented(armed.segmented(1, claim, PalwSegmentMaskV2::single(0))),
        court_fraud.clone(),
    );
    assert_eq!(refused(&armed, &segmented), PalwOffenceVerifyError::NoTarget.to_string());
    // Signed for another network, it verifies against nothing here.
    let elsewhere = Hash64::from_u64_word(0x0711);
    let foreign = armed.v2(1, claim, PalwFalseValidReceiptV1::Full(armed.full(1, claim, elsewhere)), court_fraud.clone());
    assert_eq!(refused(&armed, &foreign), PalwOffenceVerifyError::PanelFalseValidReceiptUnverified.to_string());
    // Signed by another card than the bond it accuses.
    let mut borrowed_receipt = armed.full(2, claim, armed.domain);
    borrowed_receipt.seat_bond = armed.cards[1];
    let borrowed = armed.v2(1, claim, PalwFalseValidReceiptV1::Full(borrowed_receipt), court_fraud.clone());
    assert_eq!(refused(&armed, &borrowed), PalwOffenceVerifyError::PanelFalseValidReceiptUnverified.to_string());
    // The digest is the gate's first question.
    let Obj::ObjectiveOffence { kind, accused, evidence, .. } = full.clone() else { unreachable!() };
    let renamed = Obj::ObjectiveOffence { kind, accused, evidence_id: Hash64::from_u64_word(1), evidence };
    assert_eq!(refused(&armed, &renamed), PalwOffenceVerifyError::EvidenceIdMismatch.to_string());

    // Below the fence: the V2 kind is dormant, and the V1 kind is the V1 gate's (which names the
    // missing claim, not the fence).
    let dormant = gate(false);
    let full = dormant.v2(1, claim, PalwFalseValidReceiptV1::Full(dormant.full(1, claim, dormant.domain)), court_fraud.clone());
    assert_eq!(refused(&dormant, &full), PalwOffenceVerifyError::AttributionDormant.to_string());
    assert!(dormant.accepted(&dormant.state, &full).is_empty());
    let v1 = dormant.v1(1, claim, court_fraud);
    assert_eq!(refused(&dormant, &v1), "PanelFalseValid names neither a live claim nor a liability row");
}

/// **A conviction through the gate, the walk and the fold — once per (seat, claim), and only of a
/// seat the chain relied on.** A retired claim's row records the `CourtFraud` void and lists card 1:
/// the gate admits card 1's V2 offence, the walk carries it, the fold writes one kind-3 row (charged
/// 0: the lock went with the claim, and only the row still names the seat). The same offence again
/// is refused at the gate. Card 2, which signed but is on no row, clears the gate — the lock is the
/// fold's question — and the walk's per-object rehearsal drops it.
#[tokio::test]
async fn f2_the_gate_the_walk_and_the_fold_convict_once_and_only_a_listed_seat() {
    let g = gate(true);
    let claim = Hash64::from_u64_word(0xF2_C1A2);
    let state = g.with_voided_row(claim, &[1]);
    let court_fraud = PalwPanelContradictionV1::CourtFraud { voided_daa: VOIDED_DAA };
    let listed = g.v2(1, claim, PalwFalseValidReceiptV1::Full(g.full(1, claim, g.domain)), court_fraud.clone());
    g.validate(&state, &listed).expect("the listed seat's false Valid clears the gate");
    assert_eq!(g.accepted(&state, &listed), vec![listed.clone()], "and the walk carries it");
    let vp = g.ctx.consensus.virtual_processor();
    let folded = vp.palw_v2_fold_accepted_for_tests(&state, &g.bundle.state, &g.point, std::slice::from_ref(&listed)).expect("folds");
    let row = folded.consumed_offence(&palw_false_valid_offence_id_v2(&g.cards[1].0, &claim)).expect("one kind-3 row");
    assert_eq!((row.kind, row.accused, row.amount), (PalwOffenceKindV1::PanelFalseValidV2, g.cards[1].0, 0));
    assert_eq!(row.execution_root, Hash64::default(), "a named void records no root: it names a claim, not an execution");
    assert_eq!(folded.bond(&g.cards[1]).unwrap().collateral, state.bond(&g.cards[1]).unwrap().collateral);

    // Once per (seat, claim): the same offence again is refused at the gate, and the walk drops it.
    let again = g.validate(&folded, &listed).expect_err("a (seat, claim) already convicted");
    assert!(again.contains("already convicted"), "{again}");
    assert!(g.accepted(&folded, &listed).is_empty());

    // A retired claim's segment cut is not in state until F1, so a segmented receipt on it cannot be
    // placed: the same seat's V3 `Valid` is refused before anything is charged.
    let segmented =
        g.v2(1, claim, PalwFalseValidReceiptV1::Segmented(g.segmented(1, claim, PalwSegmentMaskV2::full(4))), court_fraud.clone());
    assert_eq!(g.validate(&state, &segmented), Err(PalwOffenceVerifyError::SegmentsUnknown.to_string()));

    // A seat on no row: the gate cannot see the lock, the rehearsal can.
    let unlisted = g.v2(2, claim, PalwFalseValidReceiptV1::Full(g.full(2, claim, g.domain)), court_fraud);
    g.validate(&state, &unlisted).expect("the adjudicator does not read locks");
    assert!(g.accepted(&state, &unlisted).is_empty(), "the per-object rehearsal refuses a seat the chain never relied on");
}
