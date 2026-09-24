//! **ADR-0152 SR-10's V3 supplementary door at the processor, on testnet-12** (M4 review,
//! finding 1): the processor's own gate (`palw_v2_validate_objects`) is the only place a V3
//! supplementary set's signatures are verified in the live path — the fold re-derives every
//! structural fact but no signature — so the routing there is pinned here, not only the door's
//! two halves.
//!
//! Past `palw_rcore_plus` the gate routes a `ReceiptLicensedV2` on a claim already
//! `ReceiptLicensed` to `validate_supplementary_receipts_v3` and admits only `Supplementary` there.
//! Here: a sound set of V3 receipts signed by the harness cards under the chain's domain is
//! admitted, carried by the acceptance walk and folded as the door (credited, locked with its
//! masks, recounted); the same seats signing under the V2 receipt context, or over the V2 message,
//! are refused by name and dropped by the walk; a set naming a seat the licence already credited is
//! refused; a `Sampled` rides. On the fence-off twin (the fence and C7 unset, the bundle's mirrors
//! re-synced, and that bundle's state params handed to the gate and the fold) the same set on the
//! same licensed claim is refused by the coverage path's phase check, as before M4, and the fold
//! refuses it as the wrong phase.
//!
//! The claim is folded as the template block's own attempt (card 0, `palw_producer_facts_v2`'s
//! facts, signed), with roots no execution produced: nothing here reads them, and the panel's
//! verdicts are signatures, not replays. The panel (cards 1–5) is folded directly, and the licence
//! is a V1 `ReceiptLicensed` of three full V2 `Valid`s — the full-replay seat and two partial seats
//! — carried through the gate, the walk and the fold, so two partial seats are left to supplement.
//! No block is mined.
use super::TestContext;
use crate::consensus::test_consensus::TestConsensus;
use crate::pipeline::virtual_processor::VirtualStateProcessor;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
    PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3,
    palw_receipt_message_v2, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2,
    PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, revert_delta_v2,
};
use kaspa_consensus_core::palw_verification_v2::{PalwSegmentAssignmentV2, PalwSegmentMaskV2, palw_segment_assignment_v2};
use kaspa_hashes::Hash64;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Card 0 produces the claim; cards 1–5 are its panel.
const EXECUTOR: usize = 0;
const PANEL: [usize; 5] = [1, 2, 3, 4, 5];

fn card_pubkey(card: usize) -> Vec<u8> {
    TestConsensus::palw_v2_registry_keypair(card as u64).verification_key.as_ref().to_vec()
}

fn sign(card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&TestConsensus::palw_v2_registry_keypair(card as u64).signing_key, message, context, [0x52u8; 32])
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
}

/// How a V3 receipt is signed: as a seat signs it, or with one of the two halves the gate checks
/// taken from the V2 receipt.
#[derive(Clone, Copy, Debug)]
enum Signed {
    /// The V3 message under the V3 context.
    AsV3,
    /// The V3 message under the V2 receipt context.
    UnderTheV2Context,
    /// The V2 message (no mask) under the V3 context.
    OverTheV2Message,
}

struct Gate {
    ctx: TestContext,
    bundle: PalwConsensusParamsV2,
    /// `palw_network_domain_v2_for(network id, genesis)` — what the processor derives.
    domain: Hash64,
    /// The genesis cards' bond keys, in registry order.
    cards: Vec<PalwBondKeyV2>,
    state: PalwChainStateV2,
    daa: u64,
    blue: u64,
}

/// testnet-12 with harness cards at genesis; `armed = false` is the fence-off twin (the fence and
/// C7 unset, the bundle's mirrors re-synced to the dormant values), as `t12_rcore_skeleton_gate`'s.
fn gate(armed: bool) -> Gate {
    kaspa_core::log::try_init_logger("warn");
    let (config, _, _premine, _floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
    assert!(config.params.palw_rcore_plus.is_some_and(|f| f.is_active(0)), "testnet-12 arms R-core+ from genesis");
    let config: Config = if armed {
        config
    } else {
        let mut params = config.params.clone();
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    // The bundle the node runs — the twin's carries the dormant mirrors, and the gate and the fold
    // read the fence off the state params their caller hands them, as the pipeline hands its own.
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let bundle = PalwConsensusParamsV2::clone(bundle);
    assert_eq!(bundle.state.rcore_plus_active_at(0), armed, "the state params' mirror is the ruleset's fence");
    let ctx = TestContext::new(TestConsensus::new(&config));
    let (_, state) =
        ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the genesis tip loads");
    let cards: Vec<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    for (i, card) in cards.iter().enumerate() {
        assert_eq!(state.bond(card).expect("a genesis card").pubkey, card_pubkey(i), "card {i} carries its harness key");
    }
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let last = *state.last_point().expect("the genesis fold records its point");
    Gate { ctx, bundle, domain, cards, state, daa: last.daa_score, blue: last.blue_score }
}

impl Gate {
    fn vp(&self) -> &Arc<VirtualStateProcessor> {
        self.ctx.consensus.virtual_processor()
    }

    fn sp(&self) -> &PalwStateParamsV2 {
        &self.bundle.state
    }

    fn card_of(&self, bond: &PalwBondKeyV2) -> usize {
        self.cards.iter().position(|c| c == bond).expect("a genesis card")
    }

    /// The next block's point — one DAA and one blue score along.
    fn next(&self) -> PalwBlockContextV2 {
        PalwBlockContextV2 {
            block: Hash64::from_u64_word(0x5210_0000_0000 | (self.blue + 1)),
            daa_score: self.daa + 1,
            blue_score: self.blue + 1,
            subsidy: 0,
        }
    }

    fn advance(&mut self, point: &PalwBlockContextV2, state: PalwChainStateV2) {
        self.daa = point.daa_score;
        self.blue = point.blue_score;
        self.state = state;
    }

    // ---- the processor's three doors -------------------------------------------------------

    fn validate(&self, point: &PalwBlockContextV2, object: &Obj) -> Result<(), String> {
        self.vp().palw_v2_validate_objects(&self.state, self.sp(), point, std::slice::from_ref(object))
    }

    fn accepted(&self, point: &PalwBlockContextV2, object: &Obj) -> Vec<Obj> {
        self.vp().palw_v2_accepted_objects_for_tests(&self.state, self.sp(), point, vec![object.clone()], point.block)
    }

    fn fold(&self, point: &PalwBlockContextV2, object: &Obj) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
        self.vp().palw_v2_fold_accepted_with_delta_for_tests(&self.state, self.sp(), point, std::slice::from_ref(object))
    }

    /// **The whole path an object takes into a block**: the gate admits it, the walk carries it,
    /// the fold applies it. Returns the parent state and the delta.
    fn carry(&mut self, object: Obj) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let point = self.next();
        if let Err(why) = self.validate(&point, &object) {
            panic!("the gate refuses an object this test needs admitted: {why}");
        }
        assert_eq!(self.accepted(&point, &object), vec![object.clone()], "the walk carries what the gate admitted");
        let (next, delta) = self.fold(&point, &object).unwrap_or_else(|e| panic!("the fold applies what the walk carried: {e}"));
        let parent = self.state.clone();
        self.advance(&point, next);
        (parent, delta)
    }

    /// **A refusal at the gate** whose reason holds every one of `want`, and the walk drops it.
    fn refused(&self, point: &PalwBlockContextV2, object: &Obj, want: &[&str]) {
        let why = self.validate(point, object).expect_err("the gate refuses it");
        for needle in want {
            assert!(why.contains(needle), "the gate's reason names `{needle}`: {why}");
        }
        assert!(self.accepted(point, object).is_empty(), "the walk drops what the gate refused: {why}");
    }

    // ---- the claim, its panel and its licence ------------------------------------------------

    /// **A claim opened by the template block's own attempt**: card 0, the node's own producer
    /// facts, a signed envelope in the header's carriage, folded as the pipeline folds a block's own
    /// work. Its roots are fixed words — the gate and the door read none of them.
    fn open_claim(&mut self) -> Hash64 {
        use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
        let template = self.ctx.build_block_template_keeping_time(0);
        let mut header: Header = template.block.header.clone();
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id), "the attempt lane");
        let bond = self.cards[EXECUTOR];
        let facts = self
            .ctx
            .consensus
            .palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0))
            .expect("testnet-12 answers for its floor");
        facts.ready_to_produce(&card_pubkey(EXECUTOR)).expect("card 0 is ready to produce");
        let pre_pow = pre_pow_hash_64(&header);
        let trace_root = Hash64::from_u64_word(0x5210_7A);
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.domain,
            challenge: challenge_v2(self.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(EXECUTOR),
            operator_id: facts.bond.as_ref().expect("a genesis card is a registered bond").operator_id,
            artifact_root: facts.artifact_root,
            trace_root,
            output_root: Hash64::from_u64_word(0x0052_1032),
            execution_root: Hash64::from_u64_word(0x5210_41),
            pwu: facts.pwu,
            trace_manifest_root: attempt_trace_manifest_root_v1(trace_root, PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let signature = sign(EXECUTOR, attempt_id_v2(&attempt).as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
        let envelope = PalwAttemptEnvelopeV2 { attempt, signature };
        header.palw_commitment = envelope.encode_wire();
        header.finalize();
        let point = PalwBlockContextV2 {
            block: header.hash,
            daa_score: header.daa_score,
            blue_score: header.blue_score,
            subsidy: self.vp().coinbase_manager.calc_block_subsidy(header.daa_score),
        };
        assert!(point.blue_score > self.blue && point.daa_score >= self.daa, "the template block follows the walk");
        let (next, _delta, skips) = self
            .vp()
            .palw_v2_fold_attempt_for_tests(&self.state, self.sp(), &point, &[], &envelope, &header)
            .expect("the block's own attempt folds");
        assert!(skips.is_empty(), "the own attempt is not skipped: {skips:?}");
        let before: BTreeSet<Hash64> = PalwStateCarriageV2::from_state(&self.state).claims.keys().copied().collect();
        let opened: Vec<Hash64> =
            PalwStateCarriageV2::from_state(&next).claims.keys().copied().filter(|id| !before.contains(id)).collect();
        assert_eq!(opened, vec![attempt_id_v2(&envelope.attempt)], "the attempt opened one claim, keyed by its id");
        self.advance(&point, next);
        opened[0]
    }

    /// `PanelBound` with cards 1–5, folded directly (its derivation is the processor's and not what
    /// is measured here). Returns the assignment the anchor drew.
    fn bind(&mut self, claim: Hash64) -> PalwSegmentAssignmentV2 {
        let seats: Vec<PalwPanelSeatV2> = PANEL
            .iter()
            .map(|&i| PalwPanelSeatV2 {
                bond: self.cards[i],
                operator_id: self.state.bond(&self.cards[i]).expect("a card").operator_id,
            })
            .collect();
        let point = self.next();
        let object = Obj::PanelBound { claim, anchor: Hash64::from_u64_word(0x5210_00B1), seats };
        let (next, _) = self.fold(&point, &object).expect("the panel binds");
        assert!(matches!(next.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "every seat could lock");
        self.advance(&point, next);
        let panel = self.state.panel(&claim).expect("a bound panel");
        palw_segment_assignment_v2(panel.anchor, claim, panel.seats.len() as u16)
    }

    /// Card `card`'s full (V2) `Valid` on `claim`, signed under the chain's domain.
    fn full_receipt(&self, card: usize, claim: Hash64, signed_daa: u64) -> PalwSeatReceiptV2 {
        let message = palw_receipt_message_v2(self.domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
        PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: self.cards[card],
            signed_daa,
            signature: sign(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        }
    }

    /// Card `card`'s V3 receipt over `mask`, signed as `signed` says.
    fn v3_receipt(
        &self,
        card: usize,
        claim: Hash64,
        verdict: PalwReceiptVerdictV2,
        signed_daa: u64,
        mask: PalwSegmentMaskV2,
        signed: Signed,
    ) -> PalwSeatReceiptV3 {
        let v3 = palw_receipt_message_v3(self.domain, claim, verdict, signed_daa, mask);
        let v2 = palw_receipt_message_v2(self.domain, claim, verdict, signed_daa);
        let signature = match signed {
            Signed::AsV3 => sign(card, v3.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT),
            Signed::UnderTheV2Context => sign(card, v3.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
            Signed::OverTheV2Message => sign(card, v2.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT),
        };
        PalwSeatReceiptV3 {
            receipt: PalwSeatReceiptV2 { claim, verdict, seat_bond: self.cards[card], signed_daa, signature },
            segments: mask,
        }
    }
}

/// A claim licensed by the V1 door with the full-replay seat and the first two partial seats (in
/// panel order); returns the claim, its assignment and the two partial seats left out, as
/// `(card, panel index)`.
fn licensed(g: &mut Gate) -> (Hash64, PalwSegmentAssignmentV2, Vec<(usize, u16)>) {
    let claim = g.open_claim();
    let assignment = g.bind(claim);
    assert_eq!(assignment.segments, 4, "a five-seat panel cuts the job in four");
    let panel = g.state.panel(&claim).expect("a bound panel").clone();
    let full = assignment.full_seat as usize;
    let partials: Vec<usize> = (0..panel.seats.len()).filter(|i| *i != full).collect();
    let signers: Vec<usize> = [full, partials[0], partials[1]].iter().map(|&i| g.card_of(&panel.seats[i].bond)).collect();
    let point = g.next();
    let receipts = signers.iter().map(|&card| g.full_receipt(card, claim, point.daa_score)).collect();
    g.carry(Obj::ReceiptLicensed { claim, receipts });
    assert!(matches!(g.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the V1 licence folds");
    let left: Vec<(usize, u16)> = partials[2..].iter().map(|&i| (g.card_of(&panel.seats[i].bond), i as u16)).collect();
    for (card, _) in &left {
        assert!(g.state.slashable_lock(g.cards[*card], claim).is_none(), "card {card} is not counted by the licence");
    }
    (claim, assignment, left)
}

/// **Past the fence the gate routes a set on a licensed claim to the V3 door**: a sound set is
/// admitted, carried and folded as the door; the same seats signing under the V2 receipt context
/// or over the V2 message are refused by name; a set naming a seat the licence credited is
/// refused; a `Sampled` rides.
#[tokio::test]
async fn t12_the_gate_routes_a_set_on_a_licensed_claim_to_the_v3_door() {
    let mut g = gate(true);
    let (claim, assignment, left) = licensed(&mut g);
    let point = g.next();
    let set = |signed: Signed| Obj::ReceiptLicensedV2 {
        claim,
        receipts: left
            .iter()
            .map(|&(card, index)| {
                g.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(index), signed)
            })
            .collect(),
    };

    // A signature over the wrong half is refused by the gate — the fold verifies none.
    for signed in [Signed::UnderTheV2Context, Signed::OverTheV2Message] {
        g.refused(&point, &set(signed), &["supplementary V3 receipts are refused", "receipt signature does not verify"]);
    }
    // A seat the licence credited is counted already.
    let panel = g.state.panel(&claim).expect("a bound panel").clone();
    let credited_index = (0..panel.seats.len() as u16).find(|i| *i != assignment.full_seat).expect("a partial seat");
    let credited_card = g.card_of(&panel.seats[credited_index as usize].bond);
    let again = Obj::ReceiptLicensedV2 {
        claim,
        receipts: vec![g.v3_receipt(
            credited_card,
            claim,
            PalwReceiptVerdictV2::Valid,
            point.daa_score,
            assignment.mask_of(credited_index),
            Signed::AsV3,
        )],
    };
    g.refused(&point, &again, &["supplementary V3 receipts are refused", "already credited"]);
    // A `Sampled` is R-core+'s verdict and rides the door.
    let (sampler, sampler_index) = left[0];
    let sampled = Obj::ReceiptLicensedV2 {
        claim,
        receipts: vec![g.v3_receipt(
            sampler,
            claim,
            PalwReceiptVerdictV2::Sampled,
            point.daa_score,
            assignment.mask_of(sampler_index),
            Signed::AsV3,
        )],
    };
    assert_eq!(g.validate(&point, &sampled), Ok(()), "a Sampled rides the V3 door past the fence");

    // The sound set: admitted, carried, folded as the door — no phase moves, both seats credited
    // and locked with their own masks and the cut, their served bits set, and the recount never
    // lowered. (Integration, S-3 × F4: the V1 licence's three full-replay `Valid`s are recorded on
    // their locks as the full cut (L-3), so the licence alone recounts to 3; on F4's own branch
    // those locks carried no mask and read as their assignments, so the door's two made it 2.)
    assert_eq!(g.state.claim(&claim).unwrap().rcore.basis_k, 3, "S-3: the V1 licence's three full-cut Valids");
    let licensed_daa = match g.state.claim(&claim).unwrap().phase {
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
        ref other => panic!("licensed: {other:?}"),
    };
    let sound = set(Signed::AsV3);
    let (parent, delta) = g.carry(sound);
    let record = g.state.claim(&claim).unwrap().clone();
    assert!(matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: at } if at == licensed_daa), "no phase moves");
    for &(card, index) in &left {
        let seat = g.cards[card];
        let lock = *g.state.slashable_lock(seat, claim).expect("each new Valid locks");
        assert_eq!((lock.attested, lock.segments), (assignment.mask_of(index), 4), "card {card}: its mask and the cut");
        assert!(g.state.panel_duties_of(&claim).and_then(|row| row.get(&seat)).is_some_and(|at| *at != 0), "card {card} credited");
        assert_ne!(record.rcore.served_mask & (1 << index), 0, "card {card} served");
    }
    assert_eq!(record.rcore.basis_k, 3, "the licence's three and the door's two: 3, raised never lowered");
    assert_eq!(
        revert_delta_v2(&g.state, &delta, g.sp()).expect("the delta reverts").state_root(),
        parent.state_root(),
        "the supplementary set reverts"
    );
}

/// **The fence-off twin: below `palw_rcore_plus` nothing is routed.** The same set on the same
/// licensed claim goes to the coverage path, which refuses a claim that is not `PanelBound` — the
/// refusal a licensed claim always met — and the fold refuses it as the wrong phase.
#[tokio::test]
async fn t12_below_the_fence_the_gate_refuses_the_set_as_before() {
    let mut g = gate(false);
    let (claim, assignment, left) = licensed(&mut g);
    let point = g.next();
    let set = Obj::ReceiptLicensedV2 {
        claim,
        receipts: left
            .iter()
            .map(|&(card, index)| {
                g.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(index), Signed::AsV3)
            })
            .collect(),
    };
    g.refused(&point, &set, &["segment receipts do not license", "wrong phase for ReceiptCoverageV2"]);
    let err = g.fold(&point, &set).expect_err("the fold refuses it too");
    assert!(matches!(err, PalwStateV2Error::WrongPhase { edge: "ReceiptLicensedV2", .. }), "{err}");
}
