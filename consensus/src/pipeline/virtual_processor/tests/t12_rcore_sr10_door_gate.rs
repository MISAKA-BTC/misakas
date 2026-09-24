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
        self.open_claim_as(|_| {})
    }

    /// [`Self::open_claim`] with the floor's facts for card 0 restated by `restate` first (a claim of
    /// another class: its id, artifact root and pwu).
    fn open_claim_as(&mut self, restate: impl FnOnce(&mut kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2)) -> Hash64 {
        use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
        let template = self.ctx.build_block_template_keeping_time(0);
        let mut header: Header = template.block.header.clone();
        assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id), "the attempt lane");
        let bond = self.cards[EXECUTOR];
        let mut facts = self
            .ctx
            .consensus
            .palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0))
            .expect("testnet-12 answers for its floor");
        facts.ready_to_produce(&card_pubkey(EXECUTOR)).expect("card 0 is ready to produce");
        restate(&mut facts);
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
            output_root: Hash64::from_u64_word(0x5210_32),
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

// ---- F4 part 2: Q-5's gate and the V3 supplementary collector, at the processor ---------------

impl Gate {
    /// A block with no objects at `daa` (one blue score along), folded by the processor's own fold:
    /// the sweep is what Q-5's gate runs in. Returns the parent and the delta.
    fn quiet_block_at(&mut self, daa: u64) -> (PalwChainStateV2, PalwStateDeltaV2) {
        assert!(daa >= self.daa, "the walk never goes back");
        let point = PalwBlockContextV2 {
            block: Hash64::from_u64_word(0x5210_0000_0000 | (self.blue + 1)),
            daa_score: daa,
            blue_score: self.blue + 1,
            subsidy: 0,
        };
        let (next, delta) =
            self.vp().palw_v2_fold_accepted_with_delta_for_tests(&self.state, self.sp(), &point, &[]).expect("a quiet block folds");
        let parent = self.state.clone();
        self.advance(&point, next);
        (parent, delta)
    }

    /// The claim's DL-1 deadline and its doors' receipt deadline, on the walk's state.
    fn deadlines(&self, claim: Hash64) -> (Option<u64>, u64) {
        let record = self.state.claim(&claim).expect("the claim").clone();
        let receipt_deadline =
            kaspa_consensus_core::palw_state_v2::palw_claim_receipt_deadline_v1(&self.state, self.sp(), &claim, &record)
                .expect("no overflow")
                .expect("a bound panel");
        (self.state.deadline_of(&claim), receipt_deadline)
    }
}

/// **An S2 licence carried through the gate**: the full-replay seat's V3 `Valid` and the first partial
/// seat's, an `OptimisticLicensed` of two — what the optimistic door takes after the licence-stall fix.
/// Returns the claim, its assignment and the three partial seats it did not carry, as
/// `(card, panel index)`.
fn s2_licensed(g: &mut Gate) -> (Hash64, PalwSegmentAssignmentV2, Vec<(usize, u16)>) {
    let claim = g.open_claim();
    let assignment = g.bind(claim);
    let panel = g.state.panel(&claim).expect("a bound panel").clone();
    let full = assignment.full_seat;
    let partials: Vec<u16> = (0..panel.seats.len() as u16).filter(|i| *i != full).collect();
    let point = g.next();
    let receipts = [full, partials[0]]
        .iter()
        .map(|&i| {
            let card = g.card_of(&panel.seats[i as usize].bond);
            g.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(i), Signed::AsV3)
        })
        .collect();
    g.carry(Obj::OptimisticLicensed { claim, receipts });
    let record = g.state.claim(&claim).unwrap();
    assert!(matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the S2 licence folds");
    let left = partials[1..].iter().map(|&i| (g.card_of(&panel.seats[i as usize].bond), i)).collect();
    (claim, assignment, left)
}

/// **Q-5 end to end on testnet-12 (T72, T72b's silent half)**: an S2 licence that no seat upgrades is
/// due past both supplementary doors; the first quiet block past that redraws it (`Provisional`, the
/// redraw spent, the `Default` record, the signers' locks released); a second panel's S2 licence left
/// alone voids `NotReplayBacked` and debits the producer (S0′). Both transitions revert through their
/// deltas.
#[tokio::test]
async fn t12_q5_an_s2_licence_redraws_once_then_voids_not_replay_backed() {
    let mut g = gate(true);
    let (claim, assignment, _) = s2_licensed(&mut g);
    let record = g.state.claim(&claim).unwrap().clone();
    assert_eq!(record.rcore.basis_k, 1, "the full seat and one rider");
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = record.phase else { unreachable!() };
    let (deadline, receipt_deadline) = g.deadlines(claim);
    let gate_at = (licensed_daa + g.sp().window_challenge_at(licensed_daa)).max(receipt_deadline + 1);
    assert_eq!(deadline, Some(gate_at), "DL-1's Q-5 row");
    let panel = g.state.panel(&claim).unwrap().clone();
    let signers: Vec<PalwBondKeyV2> =
        panel.seats.iter().map(|seat| seat.bond).filter(|b| g.state.slashable_lock(*b, claim).is_some()).collect();
    assert_eq!(signers.len(), 2, "the S2 signers are locked");

    g.quiet_block_at(gate_at);
    assert!(matches!(g.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not at the gate itself");
    let (parent, delta) = g.quiet_block_at(gate_at + 1);
    let redrawn = g.state.claim(&claim).unwrap().clone();
    assert_eq!(redrawn.phase, PalwClaimPhaseV2::Provisional, "the first panel redraws");
    assert_eq!(redrawn.rebound_daa, Some(gate_at + 1));
    assert_eq!(redrawn.rcore, Default::default());
    for seat in &signers {
        assert!(g.state.slashable_lock(*seat, claim).is_none(), "the S2 signers' locks are released");
    }
    assert!(g.state.panel_duties_of(&claim).is_none(), "and their credit dropped");
    assert_eq!(revert_delta_v2(&g.state, &delta, g.sp()).unwrap().state_root(), parent.state_root(), "the redraw reverts");

    // The second panel, S2-licensed again and left alone.
    let assignment_again = g.bind(claim);
    assert_eq!(assignment_again.full_seat, assignment.full_seat, "the harness binds the same seats from the same anchor");
    let panel = g.state.panel(&claim).unwrap().clone();
    let partial = (0..panel.seats.len() as u16).find(|i| *i != assignment_again.full_seat).unwrap();
    let point = g.next();
    let receipts = [assignment_again.full_seat, partial]
        .iter()
        .map(|&i| {
            let card = g.card_of(&panel.seats[i as usize].bond);
            g.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment_again.mask_of(i), Signed::AsV3)
        })
        .collect();
    g.carry(Obj::OptimisticLicensed { claim, receipts });
    let second = g.state.claim(&claim).unwrap().clone();
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = second.phase else { panic!("the second S2 licence: {:?}", second.phase) };
    assert!(second.rebound_daa.is_some(), "on the second panel");
    let (deadline, receipt_deadline) = g.deadlines(claim);
    assert_eq!(deadline, Some((licensed_daa + g.sp().window_challenge_at(licensed_daa)).max(receipt_deadline + 1)), "gated again");
    let producer = g.cards[EXECUTOR];
    let slashed_before = g.state.bond(&producer).unwrap().slashed;
    let (parent, delta) = g.quiet_block_at(deadline.unwrap() + 1);
    assert!(matches!(
        g.state.claim(&claim).unwrap().phase,
        PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::NotReplayBacked, .. }
    ));
    assert!(g.state.bond(&producer).unwrap().slashed > slashed_before, "S0′ forfeits the commitment");
    assert_eq!(revert_delta_v2(&g.state, &delta, g.sp()).unwrap().state_root(), parent.state_root(), "the void reverts");
}

/// **The V3 supplementary collector builds exactly what the door credits (Q-7), and the upgrade
/// lifts the gate (Q-5)** — at the processor, with real signatures. The pool holds the three partial
/// seats the S2 licence left out, each signed as a seat signs, plus a copy under the V2 receipt
/// context, a copy over the V2 message, the licence's own rider again, and a `Sampled` beside a
/// `Valid`. The assembler (`palw_v2_supplementary_assemble_on_v1`, the one the node calls at
/// virtual) offers the three sound `Valid`s and nothing else; the gate admits the offer, the walk
/// carries it, the fold credits and locks exactly those seats and recounts to 2, and the deadline is
/// re-derived off the gate to `max(L + wc(L), U)`. On the fence-off twin the assembler offers nothing.
#[tokio::test]
async fn t12_the_collector_builds_what_the_door_credits_and_the_upgrade_lifts_the_gate() {
    let mut g = gate(true);
    let (claim, assignment, left) = s2_licensed(&mut g);
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = g.state.claim(&claim).unwrap().phase else { unreachable!() };
    let panel = g.state.panel(&claim).unwrap().clone();
    let rider = (0..panel.seats.len() as u16)
        .find(|i| *i != assignment.full_seat && g.state.slashable_lock(panel.seats[*i as usize].bond, claim).is_some())
        .expect("the licence's rider");
    let point = g.next();
    let signed = |card: usize, index: u16, verdict: PalwReceiptVerdictV2, how: Signed| {
        g.v3_receipt(card, claim, verdict, point.daa_score, assignment.mask_of(index), how)
    };
    let sound: Vec<PalwSeatReceiptV3> =
        left.iter().map(|&(card, index)| signed(card, index, PalwReceiptVerdictV2::Valid, Signed::AsV3)).collect();
    let mut pool = vec![
        signed(left[0].0, left[0].1, PalwReceiptVerdictV2::Valid, Signed::UnderTheV2Context),
        signed(left[1].0, left[1].1, PalwReceiptVerdictV2::Valid, Signed::OverTheV2Message),
        signed(g.card_of(&panel.seats[rider as usize].bond), rider, PalwReceiptVerdictV2::Valid, Signed::AsV3),
        signed(left[2].0, left[2].1, PalwReceiptVerdictV2::Sampled, Signed::AsV3),
    ];
    pool.extend(sound.iter().cloned());
    let offer = g
        .vp()
        .palw_v2_supplementary_assemble_on_v1(&g.state, g.sp(), &point, claim, &pool, &[])
        .expect("the collector has a set to offer");
    assert_eq!(offer.object, Obj::ReceiptLicensedV2 { claim, receipts: sound.clone() }, "exactly the sound Valids");
    assert_eq!((offer.effect.basis_k_before, offer.effect.basis_k_after, offer.effect.upgrades), (1, 2, true));
    let credited: Vec<PalwBondKeyV2> = left.iter().map(|&(card, _)| g.cards[card]).collect();
    let mut expected = credited.clone();
    expected.sort_by_key(|bond| panel.seats.iter().position(|seat| seat.bond == *bond));
    assert_eq!(offer.effect.credited, expected, "the fold credits every seat of the set, in panel order");

    g.carry(offer.object.clone());
    let record = g.state.claim(&claim).unwrap().clone();
    assert_eq!(record.rcore.basis_k, 2, "the upgrade");
    for bond in &credited {
        assert!(g.state.slashable_lock(*bond, claim).is_some(), "each offered seat is counted");
        assert!(g.state.panel_duties_of(&claim).and_then(|row| row.get(bond)).is_some_and(|at| *at != 0), "and credited");
    }
    let floor = licensed_daa + g.sp().window_challenge_at(licensed_daa);
    assert_eq!(g.state.deadline_of(&claim), Some(floor.max(g.daa)), "Q-5's re-arm: max(L + wc(L), U)");
    assert!(
        g.vp().palw_v2_supplementary_assemble_on_v1(&g.state, g.sp(), &g.next(), claim, &pool, &[]).is_none(),
        "nothing left to credit"
    );
    g.quiet_block_at(floor.max(g.daa) + 1);
    assert!(matches!(g.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "finalized, never redrawn");

    let mut twin = gate(false);
    let (claim, assignment, left) = s2_licensed(&mut twin);
    let point = twin.next();
    let set: Vec<PalwSeatReceiptV3> = left
        .iter()
        .map(|&(card, index)| {
            twin.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(index), Signed::AsV3)
        })
        .collect();
    assert!(
        twin.vp().palw_v2_supplementary_assemble_on_v1(&twin.state, twin.sp(), &point, claim, &set, &[]).is_none(),
        "no door below the fence"
    );
}

/// **V3S-01 through the V2 door, at the processor**: any seat's full-replay V2 `Valid` covers every
/// segment, so one partial seat the S2 licence left out — signing the whole-job V2 its replay files
/// beside its V3 (SEAT-R's `PartialReplays`) — lifts the claim off Q-5's gate on its own. The gate
/// admits it as a supplementary set, the walk carries it, the fold recounts to 2 and re-derives the
/// deadline to `max(L + wc(L), U)`, and the claim finalizes instead of redrawing.
#[tokio::test]
async fn t12_q5_one_seats_full_replay_v2_valid_upgrades_through_the_v2_door() {
    let mut g = gate(true);
    let (claim, _, left) = s2_licensed(&mut g);
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = g.state.claim(&claim).unwrap().phase else { unreachable!() };
    let (gated, _) = g.deadlines(claim);
    let floor = licensed_daa + g.sp().window_challenge_at(licensed_daa);
    assert!(gated.is_some_and(|at| at > floor), "gated past L + wc(L)");
    let point = g.next();
    let (card, _) = left[1];
    let whole_job = g.full_receipt(card, claim, point.daa_score);
    g.carry(Obj::ReceiptLicensed { claim, receipts: vec![whole_job] });
    let record = g.state.claim(&claim).unwrap().clone();
    assert_eq!(record.rcore.basis_k, 2, "a whole-job Valid covers every segment once more");
    assert!(g.state.slashable_lock(g.cards[card], claim).is_some(), "the V2 signer is counted");
    assert_eq!(g.state.deadline_of(&claim), Some(floor.max(g.daa)), "Q-5's re-arm");
    g.quiet_block_at(floor.max(g.daa) + 1);
    assert!(matches!(g.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "finalized, never redrawn");
}

/// **T72b at the processor (V3S-01; the M4 review's finding 1), with real signatures.** An S2 licence
/// of the full seat and one rider; of the three partial seats it left out one is silent and two
/// replayed — each filed its V3 `Valid` and its whole-job V2 `Valid`. A V3 set of those two `Valid`s
/// is carried FIRST, as any third party may (the gate admits it; the door credits and locks both
/// with their partial masks; segment coverage stays 1). The node's collector
/// (`palw_v2_supplementary_assemble_on_v1`, what `palw_v2_supplementary_assemble` runs at virtual),
/// handed both pools, then offers ONE whole-job V2 through the V2 door — a seat the pay set already
/// credited — and the gate admits it, the walk carries it, the fold widens that seat's lock to the
/// whole cut, recounts to 2 and re-arms the deadline, and the claim finalizes, never redrawn. On a
/// clean pool the collector offers the V2 upgrade and never the non-upgrading pay set. On the
/// fence-off twin the V2 door refuses a credited seat, as it always did.
#[tokio::test]
async fn t12_t72b_a_silent_seat_and_a_pay_set_first_still_upgrade() {
    let mut g = gate(true);
    let (claim, assignment, left) = s2_licensed(&mut g);
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = g.state.claim(&claim).unwrap().phase else { unreachable!() };
    let point = g.next();
    let replayed = &left[..2];
    let v3_pool: Vec<PalwSeatReceiptV3> = replayed
        .iter()
        .map(|&(card, index)| {
            g.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(index), Signed::AsV3)
        })
        .collect();
    let v2_pool: Vec<PalwSeatReceiptV2> = replayed.iter().map(|&(card, _)| g.full_receipt(card, claim, point.daa_score)).collect();

    // A clean pool: the collector offers the V2 upgrade, not the two Valids that cannot upgrade.
    let clean =
        g.vp().palw_v2_supplementary_assemble_on_v1(&g.state, g.sp(), &point, claim, &v3_pool, &v2_pool).expect("an upgrade on hand");
    assert_eq!(clean.object, Obj::ReceiptLicensed { claim, receipts: vec![v2_pool[0].clone()] }, "one whole-job V2");
    assert!(clean.effect.upgrades);

    // A third party carries the pay set first.
    g.carry(Obj::ReceiptLicensedV2 { claim, receipts: v3_pool.clone() });
    let paid = g.state.claim(&claim).unwrap().clone();
    assert_eq!(paid.rcore.basis_k, 1, "the silent seat's segment has one replay");
    let (first, first_index) = replayed[0];
    let before = *g.state.slashable_lock(g.cards[first], claim).expect("the pay set locked it");
    assert_eq!(before.attested, assignment.mask_of(first_index), "with its partial mask");

    // The collector's offer on the tip it would be carried into, and its whole path into a block.
    let point = g.next();
    let offer = g
        .vp()
        .palw_v2_supplementary_assemble_on_v1(&g.state, g.sp(), &point, claim, &v3_pool, &v2_pool)
        .expect("the credited seats' whole-job replays still upgrade");
    assert_eq!(offer.object, Obj::ReceiptLicensed { claim, receipts: vec![v2_pool[0].clone()] });
    assert!(offer.effect.upgrades && offer.effect.credited.is_empty(), "it widens a credited seat: nobody new is credited");
    assert_eq!(offer.effect.recounted, vec![g.cards[first]]);
    let (parent, delta) = g.carry(offer.object);
    let record = g.state.claim(&claim).unwrap().clone();
    assert_eq!(record.rcore.basis_k, 2, "every segment replayed twice");
    let widened = *g.state.slashable_lock(g.cards[first], claim).unwrap();
    assert_eq!((widened.attested, widened.segments), (PalwSegmentMaskV2::full(4), 4), "the whole cut");
    assert_eq!(widened.amount, before.amount, "never repriced");
    let floor = licensed_daa + g.sp().window_challenge_at(licensed_daa);
    assert_eq!(g.state.deadline_of(&claim), Some(floor.max(g.daa)), "Q-5's re-arm");
    assert_eq!(revert_delta_v2(&g.state, &delta, g.sp()).unwrap().state_root(), parent.state_root(), "the widening reverts");
    g.quiet_block_at(floor.max(g.daa) + 1);
    assert!(matches!(g.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "finalized, never redrawn");

    // The twin: below the fence the V2 door refuses the licence's credited rider, as always.
    let mut twin = gate(false);
    let (claim, assignment, _) = s2_licensed(&mut twin);
    let panel = twin.state.panel(&claim).unwrap().clone();
    let rider = (0..panel.seats.len() as u16)
        .find(|i| {
            *i != assignment.full_seat
                && twin
                    .state
                    .panel_duties_of(&claim)
                    .and_then(|row| row.get(&panel.seats[*i as usize].bond))
                    .is_some_and(|at| *at != 0)
        })
        .expect("the licence's rider is credited");
    let point = twin.next();
    let card = twin.card_of(&panel.seats[rider as usize].bond);
    let again = Obj::ReceiptLicensed { claim, receipts: vec![twin.full_receipt(card, claim, point.daa_score)] };
    twin.refused(&point, &again, &["already credited"]);
}

/// **X22 at the processor: three whole-job V2 `Valid`s on hand license V1, not S2** (the M4 review's
/// HIGH). The pool holds the full seat's V3 `Valid` and a partial seat's — an S2 set the optimistic
/// door would take — and the full seat's and two partial seats' whole-job V2 `Valid`s. Handed the
/// processor's own assemblers (`_on_v1`, what the node's calls run at virtual), the order past the
/// fence (`palw_licence_offer_order_v1(true, …)`, what kaspad's licence collector calls) offers the V1
/// licence: carried through the gate, the walk and the fold it records `basis_k` 3 and the plain
/// licence deadline, off Q-5's gate. The S2 set it passed over would have recorded 1 and been gated.
/// Below the fence the order is the old one and S2 is offered.
#[tokio::test]
async fn t12_x22_three_v2_valids_license_v1_not_s2() {
    use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1 as Door;
    let mut g = gate(true);
    let claim = g.open_claim();
    let assignment = g.bind(claim);
    let panel = g.state.panel(&claim).expect("a bound panel").clone();
    let full = assignment.full_seat;
    let partials: Vec<u16> = (0..panel.seats.len() as u16).filter(|i| *i != full).collect();
    let point = g.next();
    let card = |i: u16| g.card_of(&panel.seats[i as usize].bond);
    let v3_pool: Vec<PalwSeatReceiptV3> = [full, partials[0]]
        .iter()
        .map(|&i| g.v3_receipt(card(i), claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(i), Signed::AsV3))
        .collect();
    let v2_pool: Vec<PalwSeatReceiptV2> =
        [full, partials[0], partials[1]].iter().map(|&i| g.full_receipt(card(i), claim, point.daa_score)).collect();
    let panel_params = g.bundle.panel;
    let offer = |x22: bool| {
        kaspa_consensus_core::palw_panel_v2::palw_licence_offer_order_v1(
            x22,
            || g.vp().palw_v2_receipt_coverage_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &v3_pool),
            || g.vp().palw_v2_receipt_quorum_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &v2_pool),
            || g.vp().palw_v2_optimistic_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &v3_pool),
        )
    };
    let (object, door) = offer(true).expect("a licence on hand");
    assert_eq!(door, Door::Quorum, "V1 before S2");
    assert!(matches!(object, Obj::ReceiptLicensed { .. }));
    let (s2_object, s2_door) = offer(false).expect("the old order");
    assert_eq!(s2_door, Door::Optimistic, "below the fence S2 came first");

    // What each would record.
    let (s2_state, _) = g.fold(&point, &s2_object).expect("the S2 set folds");
    assert_eq!(s2_state.claim(&claim).unwrap().rcore.basis_k, 1, "S2: onto Q-5's gate");
    g.carry(object);
    let record = g.state.claim(&claim).unwrap().clone();
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = record.phase else { panic!("licensed: {:?}", record.phase) };
    assert_eq!(record.rcore.basis_k, 3, "V1: three whole-job replays");
    assert_eq!(
        g.state.deadline_of(&claim),
        Some(licensed_daa + g.sp().window_challenge_at(licensed_daa)),
        "the plain licence row, off the gate"
    );
}

// ---- Phase 2 P2-5: SR-6 at the V1 and coverage assemblers, X22's collector (T33, T45) ----------
//
// **The fixture is the 2M row, as the fold half's is** (`rcore_s3_one_ledger`'s
// `t33_a_licence_is_the_backed_subsets_and_the_predicate_agrees`). A seat is unbacked only where the
// licence's lock tops its duty up, and on testnet-12 that is the 2M row alone among attempt claims:
// with the vesting rows in, `lock_3` and `lock_2` sit within the duty the bind reserved on the floor
// and on the 8k row (`n1_on_the_floor_and_the_8k_row_the_licence_needs_no_top_up_and_five_valids_license`),
// so a seat there is backed whatever its room and the 8k row cannot hold the case. That 2M is closed
// at testnet-12's launch does not move the assembler's rule, which asks the fold and names no class:
// the fixture is only the one attempt class on which the fold's backed subset is a strict subset. (The
// launch-reachable case is the free-prompt lane, whose lock is priced on `max(R, rr)` above its duty;
// no processor fixture builds a free-prompt licence, and the path it would take is this one.) The row
// is seeded as the fold half seeds it — made `Active` and every card proved ready through the carriage —
// and the claim is the template block's own attempt, restated for the row.

impl Gate {
    /// `edit` applied to the walk's state through its carriage — the load path, as the core suites'
    /// `edited` does it.
    fn edit(&mut self, edit: impl FnOnce(&mut PalwStateCarriageV2)) {
        let mut carriage = PalwStateCarriageV2::from_state(&self.state);
        edit(&mut carriage);
        self.state = carriage.into_state(self.sp(), None).expect("a consistent carriage");
    }

    /// testnet-12's 2M row — the genesis model class with the most work per inference — and its
    /// target (the core suites' `model_classes(p).1`).
    fn row_2m(&self) -> (Hash64, u128) {
        use kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2;
        self.bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                Obj::ClassRegistered { class_id, pwu_rule, initial_target, .. } if *class_id != self.bundle.base_class_id => {
                    let leaves = match pwu_rule {
                        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
                        PalwPwuRuleV2::MaxPerAttempt(max) => *max,
                        #[allow(unreachable_patterns)]
                        _ => 0,
                    };
                    Some((leaves, *class_id, *initial_target))
                }
                _ => None,
            })
            .max_by_key(|(leaves, _, _)| *leaves)
            .map(|(_, class_id, target)| (class_id, target))
            .expect("testnet-12 registers its model rows at genesis")
    }

    /// Every card proved ready for `class` at the walk's DAA (testnet-12's readiness span is one DAA).
    fn ready(&mut self, class: Hash64) {
        use kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1;
        let (cards, daa) = (self.cards.clone(), self.daa);
        self.edit(|c| {
            for card in cards {
                c.seat_readiness.insert(
                    (card, class),
                    PalwSeatReadinessRowV1 { proved_daa: daa, proved_span: daa, leaf_index: 0, proof_version: 2, chunks: 8 },
                );
            }
        });
    }

    /// **A claim of the 2M row by card 0**: the row made `Active` and every card ready (the fold
    /// half's `model_chain`), then the template block's own attempt with the floor's facts restated
    /// for the row — its id, its registered artifact root and its derived pwu.
    fn open_2m_claim(&mut self) -> Hash64 {
        use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1;
        let (class, target) = self.row_2m();
        self.edit(|c| c.model_lifecycles.get_mut(&class).expect("the 2M row").state = PalwModelLifecycleV1::Active);
        self.ready(class);
        let artifact_root = self.state.class(&class).expect("the 2M class").artifact_root;
        // testnet-12 prices every attempt from DAA 0 (the core suites' `class_pwu`).
        let per_draw = self.state.palw_canonical_per_draw_v1(&class, self.daa + 1, Some(0)).expect("the row prices a draw");
        let pwu = kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(target, per_draw);
        let claim = self.open_claim_as(|facts| {
            facts.class_id = class;
            facts.artifact_root = artifact_root;
            facts.pwu = pwu;
        });
        assert_eq!(self.state.claim(&claim).expect("the claim").class_id, class, "the claim is the 2M row's");
        self.ready(class);
        claim
    }

    /// `bond`'s ceiling set one sompi under what it already backs at the next block — its room gone
    /// after the bind, the fold half's squeeze.
    fn squeeze(&mut self, bond: PalwBondKeyV2) {
        let at = self.daa + 1;
        let committed =
            kaspa_consensus_core::palw_state_v2::palw_bond_committed_v1(&self.state, &bond, at, None, self.sp().window_court());
        let collateral = u64::try_from(2 * committed - 2).expect("a collateral");
        self.edit(|c| c.bonds.get_mut(&bond).expect("the bond").collateral = collateral);
    }

    /// The fold's backed subset of `object` at `point` and whether it licenses — the two answers
    /// the assemblers read, bound as the processor binds them on testnet-12.
    fn fold_answers(&self, point: &PalwBlockContextV2, object: &Obj) -> (Option<Vec<PalwBondKeyV2>>, bool) {
        self.vp().palw_v2_licence_fold_answers_for_tests(&self.state, self.sp(), point, object)
    }
}

/// A 2M claim bound to cards 1–5 with one partial seat unbacked (its room gone after the bind):
/// the claim, the assignment, the panel's cards in seat order and the unbacked seat's panel index.
fn t33_claim(g: &mut Gate) -> (Hash64, PalwSegmentAssignmentV2, Vec<usize>, usize) {
    let claim = g.open_2m_claim();
    let assignment = g.bind(claim);
    let panel = g.state.panel(&claim).expect("a bound panel").clone();
    let cards: Vec<usize> = panel.seats.iter().map(|seat| g.card_of(&seat.bond)).collect();
    let victim = (assignment.full_seat as usize + 1) % cards.len();
    g.squeeze(panel.seats[victim].bond);
    (claim, assignment, cards, victim)
}

/// **T33, the processor half (ADR-0152 SR-6, Phase 2 P2-5): the V1 and coverage assemblers return
/// the backed subset.** Five whole-job V2 `Valid`s, one of an unbacked seat: the fold licenses the
/// five on the four backed ones (the predicate the assemblers read says so), and the V1 assembler —
/// which before P2-5 offered all five, the unbacked `Valid` carried for nothing — offers exactly the
/// four. The gate admits it, the walk carries it and the fold licenses it at `basis_k` 3: the four
/// locked and credited, the unbacked seat with no lock, no credit and no served bit, so the escrow is
/// held. Three `Valid`s with the unbacked one among them back only two, short of the quorum: inert,
/// and the V1 assembler offers nothing (before P2-5 it offered the inert three). The five seats' V3
/// `Valid`s over their masks do not cover once the unbacked partial seat is taken out (every segment
/// has one partial holder beside the full seat), so the coverage assembler offers nothing, as the fold
/// says.
#[tokio::test]
async fn t12_t33_the_v1_and_coverage_assemblers_return_the_backed_subset() {
    let mut g = gate(true);
    let (claim, assignment, cards, victim) = t33_claim(&mut g);
    let panel_params = g.bundle.panel;
    let point = g.next();
    let whole: Vec<PalwSeatReceiptV2> = cards.iter().map(|&card| g.full_receipt(card, claim, point.daa_score)).collect();
    let backed: Vec<PalwSeatReceiptV2> = whole.iter().enumerate().filter(|(i, _)| *i != victim).map(|(_, r)| r.clone()).collect();

    // The fold: the five license, on the four.
    let five = Obj::ReceiptLicensed { claim, receipts: whole.clone() };
    let (subset, licenses) = g.fold_answers(&point, &five);
    assert!(licenses, "four backed Valids are a quorum: the five license");
    assert_eq!(subset, Some(backed.iter().map(|r| r.seat_bond).collect()), "on the four backed seats");

    // V1: the backed subset, whatever the pool's order.
    let assemble = |pool: &[PalwSeatReceiptV2]| {
        g.vp().palw_v2_receipt_quorum_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, pool)
    };
    assert_eq!(assemble(&whole), Some(Obj::ReceiptLicensed { claim, receipts: backed.clone() }), "exactly the four");
    let mut reversed = whole.clone();
    reversed.reverse();
    let mut backed_reversed = backed.clone();
    backed_reversed.reverse();
    assert_eq!(assemble(&reversed), Some(Obj::ReceiptLicensed { claim, receipts: backed_reversed }));

    // Three with the unbacked one: two backed, short of the quorum — inert, so nothing is offered.
    let short: Vec<PalwSeatReceiptV2> = [victim, (victim + 1) % 5, (victim + 2) % 5].iter().map(|&i| whole[i].clone()).collect();
    assert_eq!(g.fold_answers(&point, &Obj::ReceiptLicensed { claim, receipts: short.clone() }), (None, false), "inert");
    assert_eq!(assemble(&short), None, "the V1 assembler offers no inert set");

    // Coverage: the backed four do not cover testnet-12's cut, so there is nothing to offer.
    let segmented: Vec<PalwSeatReceiptV3> = cards
        .iter()
        .enumerate()
        .map(|(i, &card)| {
            g.v3_receipt(card, claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(i as u16), Signed::AsV3)
        })
        .collect();
    assert_eq!(
        g.fold_answers(&point, &Obj::ReceiptLicensedV2 { claim, receipts: segmented.clone() }),
        (None, false),
        "the unbacked partial seat's segment has one replay: inert"
    );
    assert_eq!(
        g.vp().palw_v2_receipt_coverage_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &segmented),
        None,
        "the coverage assembler offers no inert set"
    );

    // The four, through the gate, the walk and the fold.
    let object = assemble(&whole).expect("the backed subset");
    let (parent, delta) = g.carry(object);
    let record = g.state.claim(&claim).unwrap().clone();
    assert!(matches!(record.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the 2M claim licenses on the backed subset");
    assert_eq!(record.rcore.basis_k, 3, "three whole-job replays or more: 3");
    let victim_bond = g.cards[cards[victim]];
    assert!(g.state.slashable_lock(victim_bond, claim).is_none(), "the unbacked seat takes no lock");
    assert!(g.state.panel_duties_of(&claim).and_then(|row| row.get(&victim_bond)).is_some_and(|at| *at == 0), "…no credit");
    assert_eq!(record.rcore.served_mask & (1 << victim), 0, "…and no served bit");
    assert_eq!(record.rcore.served_mask.count_ones(), 4);
    assert!(!record.rcore.escrow_released, "not every seat served: E held");
    for receipt in &backed {
        assert!(g.state.slashable_lock(receipt.seat_bond, claim).is_some(), "each backed signer locks");
    }
    assert_eq!(revert_delta_v2(&g.state, &delta, g.sp()).unwrap().state_root(), parent.state_root(), "the licence reverts");
}

/// **T45 at the processor (ADR-0152 X22 with SR-6): the collector prefers a backed V1 over S2, and
/// never waits on an unbacked one.** The node's order past the fence (`palw_licence_offer_order_v1(true,
/// …)`, what kaspad's licence collector calls) over the processor's own assemblers, on the 2M claim
/// with one unbacked partial seat and an S2 set on hand (the full seat's V3 `Valid` and a backed
/// rider's): with the five seats' whole-job V2 `Valid`s pooled it offers V1 — the backed four, at
/// `basis_k` 3 — and never builds S2; coverage, asked first, has nothing (the backed four do not
/// cover). With only the unbacked seat's and two others' V2 `Valid`s pooled, V1 is inert and offers
/// nothing, so S2 is offered — before P2-5 the V1 door returned that inert set and, asked before S2,
/// held the claim to its receipt window. The S2 licence folds; the inert V1 set would not have.
#[tokio::test]
async fn t12_t45_the_collector_prefers_a_backed_v1_over_s2_and_never_waits_on_an_unbacked_one() {
    use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1 as Door;
    let mut g = gate(true);
    let (claim, assignment, cards, victim) = t33_claim(&mut g);
    let panel_params = g.bundle.panel;
    let point = g.next();
    let full = assignment.full_seat as usize;
    let rider = (0..cards.len()).find(|i| *i != full && *i != victim).expect("a backed partial seat");
    let v3_pool: Vec<PalwSeatReceiptV3> = [full, rider]
        .iter()
        .map(|&i| {
            g.v3_receipt(cards[i], claim, PalwReceiptVerdictV2::Valid, point.daa_score, assignment.mask_of(i as u16), Signed::AsV3)
        })
        .collect();
    let whole: Vec<PalwSeatReceiptV2> = cards.iter().map(|&card| g.full_receipt(card, claim, point.daa_score)).collect();
    let offer = |v2_pool: &[PalwSeatReceiptV2]| {
        kaspa_consensus_core::palw_panel_v2::palw_licence_offer_order_v1(
            true,
            || g.vp().palw_v2_receipt_coverage_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &v3_pool),
            || g.vp().palw_v2_receipt_quorum_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, v2_pool),
            || g.vp().palw_v2_optimistic_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &v3_pool),
        )
    };

    // The backed V1 before S2.
    let (object, door) = offer(&whole).expect("a licence on hand");
    assert_eq!(door, Door::Quorum, "a backed V1 before S2");
    let backed: Vec<PalwSeatReceiptV2> = whole.iter().enumerate().filter(|(i, _)| *i != victim).map(|(_, r)| r.clone()).collect();
    assert_eq!(object, Obj::ReceiptLicensed { claim, receipts: backed }, "the backed subset");

    // An unbacked V1 is not waited on: S2.
    let short: Vec<PalwSeatReceiptV2> = [victim, (victim + 1) % 5, (victim + 2) % 5].iter().map(|&i| whole[i].clone()).collect();
    let (s2, door) = offer(&short).expect("the fast path still licenses");
    assert_eq!(door, Door::Optimistic, "an inert V1 set passes the claim to S2");
    assert!(matches!(&s2, Obj::OptimisticLicensed { receipts, .. } if receipts.len() == 2));
    let (s2_state, _) = g.fold(&point, &s2).expect("the S2 set folds");
    assert!(matches!(s2_state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "and licenses");
    let (inert, _) = g.fold(&point, &Obj::ReceiptLicensed { claim, receipts: short }).expect("the inert set folds");
    assert!(
        matches!(inert.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
        "the V1 set it passed over licenses nothing"
    );

    // The backed V1, carried: replay-backed at 3, off Q-5's gate.
    g.carry(object);
    let record = g.state.claim(&claim).unwrap().clone();
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = record.phase else { panic!("licensed: {:?}", record.phase) };
    assert_eq!(record.rcore.basis_k, 3);
    let (deadline, receipt_deadline) = g.deadlines(claim);
    assert_ne!(
        deadline,
        Some((licensed_daa + g.sp().window_challenge_at(licensed_daa)).max(receipt_deadline + 1)),
        "not on Q-5's gate"
    );
}

/// **The fence-off twin: below `palw_rcore_plus` the V1 assembler is what it was.** On the floor with
/// one seat unable to post its lock (the fence-off ledger reads locks at 100%, so a one-sompi bond),
/// the set is the whole set's or nothing — inert — and the assembler, which does not ask the fold
/// below the fence, offers the five as before P2-5; the fold's backed subset is not read there.
#[tokio::test]
async fn t12_t33_below_the_fence_the_v1_assembler_offers_the_whole_clean_set_as_before() {
    let mut g = gate(false);
    let claim = g.open_claim();
    let assignment = g.bind(claim);
    let panel = g.state.panel(&claim).expect("a bound panel").clone();
    let cards: Vec<usize> = panel.seats.iter().map(|seat| g.card_of(&seat.bond)).collect();
    let victim = (assignment.full_seat as usize + 1) % cards.len();
    let victim_bond = panel.seats[victim].bond;
    g.edit(|c| c.bonds.get_mut(&victim_bond).expect("the bond").collateral = 1);
    let panel_params = g.bundle.panel;
    let point = g.next();
    let whole: Vec<PalwSeatReceiptV2> = cards.iter().map(|&card| g.full_receipt(card, claim, point.daa_score)).collect();
    let five = Obj::ReceiptLicensed { claim, receipts: whole.clone() };
    assert_eq!(g.fold_answers(&point, &five), (None, false), "all or nothing below the fence: inert, and no subset is read");
    assert_eq!(
        g.vp().palw_v2_receipt_quorum_assemble_on_v1(&g.state, g.sp(), &panel_params, &point, claim, &whole),
        Some(five),
        "the whole clean set, as before"
    );
}
