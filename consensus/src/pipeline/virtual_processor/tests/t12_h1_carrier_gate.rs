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
/// tip read says which escalate.** A proof can now buy a reserved place and the head of the template
/// lane once its row nears staleness, so the gate refuses a junk proof exactly as it refuses a junk
/// accusation; and the escalation read (`palw_readiness_escalated_at`) answers from the tip's row:
/// a seat with no row yet escalates this span's proof — nothing counts it ready — and the fence-off
/// twin escalates nothing and gates nothing, so every network but testnet-12 is untouched.
#[tokio::test]
async fn m1_every_possession_proof_is_put_to_the_fold_and_the_tip_read_escalates() {
    use kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1;
    let g = gate(true);
    let (bond, class_id) = (g.cards[1], Hash64::from_u64_word(0xC1A5));
    let daa = g.ctx.consensus.get_virtual_daa_score();
    assert!(g.tx_refusal(junk_proof(bond, class_id, daa)).is_some(), "a junk proof buys nothing: the gate refuses it");
    let carrier = PalwReadinessCarrierV1 { bond, class_id, span: daa, proof_version: 2 };
    assert!(g.state.seat_readiness(&bond, &class_id).is_none(), "no row at genesis");
    assert!(g.ctx.consensus.virtual_processor().palw_readiness_escalated_at(carrier, daa), "no row: this span's proof escalates");
    let v1 = PalwReadinessCarrierV1 { proof_version: 1, ..carrier };
    assert!(!g.ctx.consensus.virtual_processor().palw_readiness_escalated_at(v1, daa), "a V1 proof renews nothing past V2");
    assert!(g.ctx.consensus.palw_readiness_escalated_v1(carrier), "the API reads the same answer at the virtual's DAA");

    let off = gate(false);
    let daa = off.ctx.consensus.get_virtual_daa_score();
    assert_eq!(off.tx_refusal(junk_proof(bond, class_id, daa)), None, "below R-core+ the gate asks nothing, as before");
    assert!(!off.ctx.consensus.virtual_processor().palw_readiness_escalated_at(carrier, daa), "and nothing escalates");
}
