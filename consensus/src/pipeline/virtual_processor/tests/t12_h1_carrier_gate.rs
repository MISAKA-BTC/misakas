//! **ADR-0152 v3.1 H-1 at the processor, on testnet-12 (P2-9 review, finding 5)**: the node's H-1
//! gate (`palw_mempool_h1_carrier_refusal`) puts every carrier H-1 names to the fold before this node
//! admits, relays, spares or mines it — the acceptance layer, then the object's own arm, on the tip —
//! with real ML-DSA-87 signatures from the harness cards (`t12_round_lane_e2e::t12_with_harness_cards`).
//!
//! * A reporter's commitment signed by its own bond's key passes: the lane may take it.
//! * The same commitment under another card's key, the review's probe (an accusation with an
//!   8-byte signature) and a reveal with no reward pending are refused — the first two by the
//!   acceptance layer, the last by the fold's arm, which is the half a kind-only check never asked.
//! * A licence is not an H-1 carrier, so the gate does not ask: it competes for fees as before.
//! * The fence-off twin asks nothing, so every network without R-core+ admits exactly as before.
//!
//! No block is mined: the gate is asked on the genesis state the processor stored at construction.
use super::TestContext;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_state_v2::{
    PALW_REPORTER_COMMIT_MLDSA87_CONTEXT, PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj,
    PalwStateParamsV2, palw_reporter_commit_message_v1,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionOutput};
use kaspa_hashes::Hash64;

struct Gate {
    ctx: TestContext,
    params: PalwStateParamsV2,
    state: PalwChainStateV2,
    point: PalwBlockContextV2,
    cards: Vec<PalwBondKeyV2>,
    network_domain: Hash64,
}

/// testnet-12 with harness cards at genesis; `armed = false` is the fence-off twin (R-core+ and C7
/// unset, the bundle's mirrors re-synced), as `t12_rcore_s7_reporter_gate` builds it.
fn gate(armed: bool) -> Gate {
    let (config, _harness_bundle, _premine, _floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
    let config: Config = if armed {
        config
    } else {
        let mut params = config.params.clone();
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    let params = match &config.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.clone(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    let genesis_bonds = match &config.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.genesis_objects.clone(),
        _ => unreachable!(),
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let ctx = TestContext::new(TestConsensus::new(&config));
    let (_, state) = ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&params).unwrap().expect("the tip loads");
    let cards = genesis_bonds
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    let point = PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: ctx.consensus.get_virtual_daa_score(),
        blue_score: 1,
        subsidy: 0,
    };
    Gate { ctx, params, state, point, cards, network_domain }
}

impl Gate {
    /// The gate on the object, at the harness's state and point.
    fn refusal(&self, object: &Obj) -> Option<String> {
        self.ctx.consensus.virtual_processor().palw_h1_carrier_refusal_on(&self.state, &self.params, &self.point, object)
    }

    /// The gate as the mempool and the template ask it: on a transaction, at the virtual's DAA.
    fn tx_refusal(&self, object: Obj) -> Option<String> {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        self.ctx.consensus.virtual_processor().palw_mempool_h1_carrier_refusal(&tx, self.ctx.consensus.get_virtual_daa_score())
    }

    /// A `ReporterCommitted` naming card `reporter`, signed with card `signer`'s harness key.
    fn commitment(&self, commitment: Hash64, reporter: usize, signer: usize) -> Obj {
        let reporter = self.cards[reporter];
        let message = palw_reporter_commit_message_v1(&self.network_domain, &commitment, &reporter);
        let key = TestConsensus::palw_v2_registry_keypair(signer as u64);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &message, PALW_REPORTER_COMMIT_MLDSA87_CONTEXT, [0u8; 32])
            .expect("sign")
            .as_ref()
            .to_vec();
        Obj::ReporterCommitted { commitment, reporter, signature }
    }
}

/// **T38 (node half): the gate passes what the fold takes and refuses what it drops** — a
/// kind-valid object is not enough to buy the lane, the reserve or the relay's pass.
#[tokio::test]
async fn h1_the_gate_passes_what_the_fold_takes_and_refuses_what_it_drops() {
    let g = gate(true);
    let honest = g.commitment(Hash64::from_u64_word(0x3801), 1, 1);
    assert_eq!(g.refusal(&honest), None, "the reporter's own key signed it and the fold roots it");
    assert_eq!(g.tx_refusal(honest), None, "the transaction gate gives the object gate's answer");

    let forged = g.commitment(Hash64::from_u64_word(0x3802), 1, 2);
    let why = g.refusal(&forged).expect("card 2's key does not sign for card 1");
    assert!(why.contains("is not signed by"), "{why}");

    // The review's probe B: an accusation with an 8-byte signature decodes, rides the may-ride
    // table and paid the floor — and signs nothing.
    let junk = Obj::DefaultAccused {
        claim: Hash64::from_u64_word(0x38FF),
        missing_event_index: 0,
        accuser: g.cards[1],
        signature: vec![1; 8],
    };
    assert!(g.tx_refusal(junk).is_some(), "a kind-valid accusation the acceptance layer refuses buys nothing");

    // The acceptance layer passes a reveal unsigned; only the fold's arm knows nothing is pending.
    let reveal = Obj::ReporterRevealed { offence_key: Hash64::from_u64_word(0x3803), reporter: g.cards[1], salt: [0x38; 32] };
    let why = g.refusal(&reveal).expect("no reward is pending under its key");
    assert!(!why.is_empty(), "the fold's own refusal is the reason");

    let licence = Obj::ReceiptLicensed { claim: Hash64::from_u64_word(0x3804), receipts: vec![] };
    assert_eq!(g.tx_refusal(licence), None, "a licence is the fee market's, not the gate's");
}

/// **Below R-core+ the gate asks nothing**, so every network but testnet-12 admits and templates
/// exactly as it did.
#[tokio::test]
async fn h1_the_gate_is_inert_below_rcore_plus() {
    let g = gate(false);
    let junk = Obj::DefaultAccused {
        claim: Hash64::from_u64_word(0x38FF),
        missing_event_index: 0,
        accuser: g.cards[1],
        signature: vec![1; 8],
    };
    assert_eq!(g.tx_refusal(junk), None);
}

/// A possession proof naming card `bond` for `class_id` at `span`, with an 8-byte signature and an
/// empty multiproof — kind-valid, and nothing the fold would take.
fn junk_proof(bond: PalwBondKeyV2, class_id: Hash64, span: u64) -> Obj {
    Obj::SeatReadinessProvedV2 {
        bond,
        class_id,
        span,
        proof: Box::new(kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 {
            leaf_count: 1,
            opened: vec![(
                0,
                kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1 {
                    tensor_name: String::new(),
                    layer: None,
                    row_start: 0,
                    bytes: vec![1],
                },
            )],
            siblings: vec![],
        }),
        signature: vec![1; 8],
    }
}

/// **M1 (the 2026-09-25 model-registry review): every possession proof is put to the fold, and the
/// tip read says how urgently each row needs its proof.** A proof can buy a reserved place and the
/// head of the template lane once its row nears staleness, so the gate refuses a junk proof exactly
/// as it refuses a junk accusation; and the urgency read (`palw_readiness_urgency_at`) answers from
/// the tip's row: a seat with no row escalates nothing (the M1 review, MEDIUM 5 — an absent row was
/// a free key to the head for any Active bond), a row within two DAA of its last fresh DAA is
/// lapsing, one past it lapsed; the fence-off twin escalates nothing and gates nothing, so every
/// network but testnet-12 is untouched.
#[tokio::test]
async fn m1_every_possession_proof_is_put_to_the_fold_and_the_tip_read_says_how_urgent() {
    use kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1;
    use kaspa_consensus_core::palw_readiness_escalation_v1::{PalwReadinessCarrierV1, PalwReadinessUrgencyV1};
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let g = gate(true);
    let (bond, class_id) = (g.cards[1], Hash64::from_u64_word(0xC1A5));
    let daa = g.ctx.consensus.get_virtual_daa_score();
    assert!(g.tx_refusal(junk_proof(bond, class_id, daa)).is_some(), "a junk proof buys nothing: the gate refuses it");
    let carrier = PalwReadinessCarrierV1 { bond, class_id, span: daa, proof_version: 2 };
    assert!(g.state.seat_readiness(&bond, &class_id).is_none(), "no row at genesis");
    let v1 = PalwReadinessCarrierV1 { proof_version: 1, ..carrier };
    assert_eq!(
        g.ctx.consensus.virtual_processor().palw_readiness_urgency_at(&[carrier, v1], daa),
        vec![None, None],
        "no row: a first proof rides the ordinary lane"
    );
    assert_eq!(g.ctx.consensus.palw_readiness_urgency_v1(&[v1, carrier]), vec![None, None], "the API reads the same tip");
    assert!(g.ctx.consensus.palw_readiness_urgency_v1(&[]).is_empty());

    // A row proved at DAA 100 for one of testnet-12's genesis classes, on the tip's own registry
    // clock (one-DAA spans, 24-DAA rows — `Params::palw_readiness_v2_max_age_spans`, user decision
    // 2026-09-25, readiness capacity option (a)): the escalation's threshold is the configured
    // `max − 2`, read through the same function.
    let class_id = *g.state.classes_iter().map(|(id, _)| id).last().expect("testnet-12 registers classes at genesis");
    let carrier = PalwReadinessCarrierV1 { class_id, ..carrier };
    let mut carriage = PalwStateCarriageV2::from_state(&g.state);
    carriage.seat_readiness.insert(
        (bond, class_id),
        PalwSeatReadinessRowV1 { proved_daa: 100, proved_span: 100, leaf_index: 0, proof_version: 2, chunks: 16 },
    );
    let state = carriage.into_state(&g.params, None).expect("a consistent state");
    let at = |now: u64| {
        let proof = PalwReadinessCarrierV1 { span: now, ..carrier };
        let v1 = PalwReadinessCarrierV1 { proof_version: 1, ..proof };
        g.ctx.consensus.virtual_processor().palw_readiness_urgency_on(&state, &[proof, v1], now)
    };
    assert_eq!(at(121), vec![None, None], "age 21: the ordinary lane");
    assert_eq!(at(122), vec![Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 124 }), None], "age 22 (24 − 2): lapsing");
    assert_eq!(at(124), vec![Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: 124 }), None], "the last DAA it counts");
    assert_eq!(at(125), vec![Some(PalwReadinessUrgencyV1::Lapsed), None], "past it: lapsed; a V1 proof renews nothing");

    let off = gate(false);
    let daa = off.ctx.consensus.get_virtual_daa_score();
    assert_eq!(off.tx_refusal(junk_proof(bond, class_id, daa)), None, "below R-core+ the gate asks nothing, as before");
    assert_eq!(
        off.ctx.consensus.virtual_processor().palw_readiness_urgency_at(&[carrier, v1], daa),
        vec![None, None],
        "and nothing escalates"
    );
    assert_eq!(
        off.ctx.consensus.virtual_processor().palw_readiness_urgency_on(&state, &[carrier], 106),
        vec![None],
        "whatever the row"
    );
}

/// **M1: an honest possession proof passes the gate** — the half the junk-proof refusal above does
/// not show. Putting every proof to the fold at admission must never refuse the seat's own: a card
/// proves one of testnet-12's genesis classes (re-rooted, in a copy of the genesis state, onto a
/// synthetic inventory whose leaves the test can open) for this span — the challenge's sixteen
/// leaves, the multiproof, the bond's own ML-DSA-87 signature — and the acceptance layer and the
/// fold's arm both take it; under another card's key it is refused. A proof of the NEXT span (a
/// seat one block ahead of this node) is refused at this node's DAA and taken at its span's first
/// DAA — the DAA the gate judges it at (`palw_readiness_gate_daa_v1`), so a peer the newest block
/// has not reached does not turn it away.
#[tokio::test]
async fn m1_an_honest_possession_proof_passes_the_gate() {
    use kaspa_consensus_core::palw_artifact::{
        PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, palw_artifact_multiproof_v1,
    };
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1,
        palw_readiness_v2_opening_is_the_challenge_v1, palw_seat_readiness_message_v2,
    };
    use kaspa_consensus_core::palw_readiness_escalation_v1::palw_readiness_gate_daa_v1;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let g = gate(true);
    let class_id = *g.state.classes_iter().map(|(id, _)| id).last().expect("testnet-12 registers classes at genesis");
    const LEAVES: u32 = 4_096;
    let (card, daa) = (1usize, g.point.daa_score);
    let bond = g.cards[card];
    let bond_bytes = borsh::to_vec(&bond).unwrap();
    let operand = |index: u32| PalwArtifactOperandV1 {
        tensor_name: "blk.0.ffn_up".into(),
        layer: Some(0),
        row_start: index,
        bytes: vec![index as u8; 64],
    };
    let leaves: Vec<Hash64> = (0..LEAVES).map(|index| artifact_leaf_v1(&operand(index))).collect();
    let mut carriage = PalwStateCarriageV2::from_state(&g.state);
    carriage.classes.get_mut(&class_id).unwrap().artifact_root = artifact_root_v1(&leaves).unwrap();
    let state = carriage.into_state(&g.params, None).expect("the re-rooted genesis state is consistent");
    // testnet-12's one-DAA spans: the span IS the DAA.
    let proved = |signer: u64, span: u64| {
        let draw = palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&class_id, &bond_bytes, span), LEAVES);
        let mut opened: Vec<_> = draw.iter().map(|index| (*index, operand(*index))).collect();
        palw_readiness_v2_opening_is_the_challenge_v1(&draw, &opened.iter().map(|(i, o)| (*i, o.bytes.len())).collect::<Vec<_>>())
            .expect("sixteen small leaves are the whole challenge");
        opened.sort_by_key(|(index, _)| *index);
        let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("the leaves are the inventory's");
        let message = palw_seat_readiness_message_v2(g.network_domain, &bond_bytes, &class_id, span, &proof);
        let key = TestConsensus::palw_v2_registry_keypair(signer);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT,
            [0u8; 32],
        )
        .expect("sign")
        .as_ref()
        .to_vec();
        Obj::SeatReadinessProvedV2 { bond, class_id, span, proof: Box::new(proof), signature }
    };
    let gate_at = |daa_score: u64, object: &Obj| {
        let point = PalwBlockContextV2 { daa_score, ..g.point };
        g.ctx.consensus.virtual_processor().palw_h1_carrier_refusal_on(&state, &g.params, &point, object)
    };
    assert_eq!(gate_at(daa, &proved(card as u64, daa)), None, "the seat's own proof of this span passes");
    assert!(
        gate_at(daa, &proved(2, daa)).is_some_and(|why| why.contains("not signed by")),
        "another card's key does not sign for card 1"
    );
    let ahead = proved(card as u64, daa + 1);
    assert!(gate_at(daa, &ahead).is_some(), "at this node's DAA the fold refuses a span it has not reached");
    assert_eq!(palw_readiness_gate_daa_v1(daa, daa + 1, 1), daa + 1, "the gate judges it at its own span's first DAA");
    assert_eq!(gate_at(daa + 1, &ahead), None, "where the fold takes it");
    assert!(gate_at(daa, &proved(card as u64, daa + 2)).is_some(), "two blocks ahead: refused as before");

    // **The same through the mempool's transaction gate** (the M1 review, LOW 1): the DAA it judges a
    // proof at is the wiring the lines above only compute. At virtual DAA `daa` the seat's proof of
    // the next span passes — this node is one block behind the seat — and one two spans ahead does
    // not; the current span passes and another card's key does not.
    let through_the_gate = |object: Obj| {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        let object = kaspa_consensus_core::palw_readiness_escalation_v1::palw_gated_carrier_object_of_tx_v1(&tx).expect("gated");
        g.ctx.consensus.virtual_processor().palw_mempool_h1_carrier_refusal_with(&object, daa, g.point.block, &state)
    };
    assert_eq!(through_the_gate(proved(card as u64, daa)), None, "this span, at this node's DAA");
    assert_eq!(through_the_gate(proved(card as u64, daa + 1)), None, "the next span: judged at its own first DAA, and taken");
    assert!(through_the_gate(proved(card as u64, daa + 2)).is_some(), "two spans ahead: refused");
    assert!(through_the_gate(proved(2, daa + 1)).is_some(), "the skew buys no pass to a forgery");
}

/// **The processor hands testnet-12's fold the configured readiness horizon, and every reader of the
/// fold takes it** (user decision 2026-09-25, readiness capacity option (a);
/// `Params::palw_readiness_v2_max_age_spans`): the fold's globals, the age the registry judges a row
/// by, the registry read the RPC serves and the node's duty reads (`readiness_max_age_daa`, `globals`),
/// and the M1 escalation's `max − 2` — 24 spans on testnet-12, and eight on the twin with the horizon
/// taken away (its mirror synced), which is still a legal ruleset. One function builds the globals
/// (`palw_registry_globals_of_bundle_v1`); nothing here restates the rule.
#[tokio::test]
async fn t12_the_fold_the_registry_read_and_the_escalation_take_the_configured_horizon() {
    use kaspa_consensus_core::palw_model_registry_v1::PalwSeatReadinessRowV1;
    use kaspa_consensus_core::palw_readiness_escalation_v1::{PalwReadinessCarrierV1, PalwReadinessUrgencyV1};
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let (config, _harness_bundle, _premine, _floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
    let mut twin = config.params.clone();
    twin.palw_readiness_v2_max_age_spans = None;
    twin.sync_palw_readiness_v2_max_age_spans();
    twin.validate_palw_v2().expect("the horizon is optional: testnet-12 without it is a legal ruleset");
    let twin: Config = ConfigBuilder::new(twin).skip_proof_of_work().build();
    for (config, max_age) in [(config, 24u64), (twin, 8)] {
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else {
            unreachable!("testnet-12 is ConsensusV2")
        };
        let (params, genesis_objects) = (bundle.state.clone(), bundle.genesis_objects.clone());
        let ctx = TestContext::new(TestConsensus::new(&config));
        let vp = ctx.consensus.virtual_processor();
        let daa = ctx.consensus.get_virtual_daa_score();
        let fold = vp.palw_model_registry_fold_at(daa).expect("testnet-12's registry is in force from genesis");
        assert_eq!((fold.span_daa, fold.globals.readiness_v2_max_age_spans as u64), (1, max_age), "the fold's globals");
        assert_eq!(fold.readiness_max_age_daa(), max_age, "the age every judge of a row asks");
        let read = ctx.consensus.palw_model_registry_v1().expect("the registry read");
        assert_eq!(read.readiness_max_age_daa, max_age, "the RPC's `expires` and `readinessProbeMaxAgeSpans`");
        assert_eq!(read.globals.map(|g| g.readiness_v2_max_age_spans as u64), Some(max_age), "the globals the node's duty reads");

        // The escalation, on a row proved at DAA 100: lapsing from `max − 2`, lapsed past `max`.
        let (_, state) = vp.palw_state_v2_store.read().load_tip(&params).unwrap().expect("the tip loads");
        let bond = genesis_objects
            .iter()
            .find_map(|o| match o {
                Obj::BondRegistered { bond, .. } => Some(*bond),
                _ => None,
            })
            .expect("testnet-12 registers bonds at genesis");
        let class_id = *state.classes_iter().map(|(id, _)| id).last().expect("testnet-12 registers classes at genesis");
        let mut carriage = PalwStateCarriageV2::from_state(&state);
        carriage.seat_readiness.insert(
            (bond, class_id),
            PalwSeatReadinessRowV1 { proved_daa: 100, proved_span: 100, leaf_index: 0, proof_version: 2, chunks: 16 },
        );
        let state = carriage.into_state(&params, None).expect("a consistent state");
        let at = |now: u64| {
            vp.palw_readiness_urgency_on(&state, &[PalwReadinessCarrierV1 { bond, class_id, span: now, proof_version: 2 }], now)[0]
        };
        let last = 100 + max_age;
        assert_eq!(at(last - 3), None, "{max_age}: the ordinary lane");
        assert_eq!(at(last - 2), Some(PalwReadinessUrgencyV1::Lapsing { last_fresh_daa: last }), "{max_age}: max − 2");
        assert_eq!(at(last + 1), Some(PalwReadinessUrgencyV1::Lapsed), "{max_age}: past the horizon");
    }
}
