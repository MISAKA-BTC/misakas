//! **ADR-0152 v3.1 S-7 (R-3) at the processor, on testnet-12**: the reporter's commitment (tag 53)
//! and its reveal (tag 54) past `Params::palw_rcore_plus` — through the processor's own gate
//! (`palw_v2_validate_objects`), its acceptance walk and the fold the pipeline runs — with real
//! ML-DSA-87 signatures from the harness cards (`t12_round_lane_e2e::t12_with_harness_cards`).
//!
//! * A commitment signed by the reporter bond's registered key over
//!   `palw_reporter_commit_message_v1(network, commitment, reporter)` under
//!   `PALW_REPORTER_COMMIT_MLDSA87_CONTEXT` passes the gate, rides the walk and is rooted by the fold
//!   at the block's DAA, counted against the reporter's 64.
//! * One signed by another key, or naming a bond the chain does not have, is dropped by the gate.
//! * A reveal carries no signature: it passes the gate, and the fold judges it — with no pending
//!   reward it is refused, and the walk drops it with the block standing.
//! * The fence-off twin (testnet-12 with `palw_rcore_plus` unset) drops both by name, as the v22
//!   skeleton did (`t12_rcore_skeleton_gate`).
//!
//! No block is mined: the gate, the walk and the fold are asked on the genesis state the processor
//! stored at construction.
use super::TestContext;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PALW_REPORTER_COMMIT_MLDSA87_CONTEXT, PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2 as Obj,
    PalwReporterCommitV1, PalwStateV2Error, palw_reporter_commit_message_v1,
};
use kaspa_hashes::Hash64;

struct Gate {
    ctx: TestContext,
    bundle: PalwConsensusParamsV2,
    state: PalwChainStateV2,
    point: PalwBlockContextV2,
    cards: Vec<PalwBondKeyV2>,
    network_domain: Hash64,
}

/// testnet-12 with harness cards at genesis; `armed = false` is the fence-off twin (the fence and
/// C7 unset, the bundle's mirrors re-synced to the dormant values).
fn gate(armed: bool) -> Gate {
    let (config, _harness_bundle, _premine, _floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
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
    // The bundle the FOLD reads is the config's own: the fence-off twin re-synced its mirrors
    // (`PalwStateParamsV2::rcore_plus_active_at` is what the fold asks), so the harness's copy —
    // mirrored armed — would fold the twin as armed (S-7 review).
    let bundle = match &config.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => {
            assert_eq!(b.state.rcore_plus_active_at(0), armed, "the fold's mirror follows the fence");
            b.clone()
        }
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
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
    let point = PalwBlockContextV2 {
        block: ctx.consensus.get_sink(),
        daa_score: ctx.consensus.get_virtual_daa_score() + 10,
        blue_score: 5,
        subsidy: 0,
    };
    Gate { ctx, bundle, state, point, cards, network_domain }
}

impl Gate {
    fn validate(&self, object: &Obj) -> Result<(), String> {
        self.ctx.consensus.virtual_processor().palw_v2_validate_objects(
            &self.state,
            &self.bundle.state,
            &self.point,
            std::slice::from_ref(object),
        )
    }

    fn accepted(&self, object: &Obj) -> Vec<Obj> {
        self.ctx.consensus.virtual_processor().palw_v2_accepted_objects_for_tests(
            &self.state,
            &self.bundle.state,
            &self.point,
            vec![object.clone()],
            self.point.block,
        )
    }

    fn fold(&self, object: &Obj) -> Result<PalwChainStateV2, PalwStateV2Error> {
        self.ctx.consensus.virtual_processor().palw_v2_fold_accepted_for_tests(
            &self.state,
            &self.bundle.state,
            &self.point,
            std::slice::from_ref(object),
        )
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

/// **A commitment signed by the reporter's own key passes the gate, rides the walk and is rooted
/// by the fold** at the block's DAA, one of the reporter's 64.
#[tokio::test]
async fn s7_a_signed_reporter_commitment_is_admitted_and_rooted_past_the_fence() {
    let g = gate(true);
    let commitment = Hash64::from_u64_word(0x5301);
    let object = g.commitment(commitment, 1, 1);
    g.validate(&object).expect("the reporter's own key signed it");
    assert_eq!(g.accepted(&object), vec![object.clone()], "the walk keeps it");
    let next = g.fold(&object).expect("the fold roots it");
    assert_eq!(
        next.reporter_commitment_of(&commitment, &g.cards[1]),
        Some(&PalwReporterCommitV1 { reporter: g.cards[1], committed_daa: g.point.daa_score }),
        "rooted at the block's DAA, in the reporter's own slot"
    );
    assert_eq!(next.reporter_open_commitments(&g.cards[1]), 1, "one of the reporter's 64");
    assert_ne!(next.state_root(), g.state.state_root(), "the R-core+ root block turns on");
}

/// **A commitment signed by another bond's key, or naming a bond the chain does not have, is
/// dropped by the gate** — a bond key is a public outpoint, and a commitment filed under a stranger's
/// bond would take one of its 64 slots.
#[tokio::test]
async fn s7_a_commitment_under_another_key_or_an_unknown_bond_is_dropped_by_the_gate() {
    let g = gate(true);
    let forged = g.commitment(Hash64::from_u64_word(0x5302), 1, 2);
    let why = g.validate(&forged).expect_err("card 2's key does not sign for card 1");
    assert!(why.contains("is not signed by"), "{why}");
    assert!(g.accepted(&forged).is_empty(), "the walk drops it, the block stands");

    let Obj::ReporterCommitted { commitment, signature, .. } = g.commitment(Hash64::from_u64_word(0x5303), 1, 1) else {
        unreachable!()
    };
    let stranger = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(0x5303),
        index: 7,
    });
    let unknown = Obj::ReporterCommitted { commitment, reporter: stranger, signature };
    let why = g.validate(&unknown).expect_err("no such bond");
    assert!(why.contains("this chain does not have"), "{why}");
    assert!(g.accepted(&unknown).is_empty());
}

/// **A reveal rides the gate unsigned and the fold judges it**: with no reward pending under its
/// key the fold refuses it by name, and the walk drops it with the block standing.
#[tokio::test]
async fn s7_a_reveal_passes_the_gate_and_the_fold_refuses_it_without_a_pending_reward() {
    let g = gate(true);
    let reveal = Obj::ReporterRevealed { offence_key: Hash64::from_u64_word(0x5401), reporter: g.cards[1], salt: [0x54; 32] };
    g.validate(&reveal).expect("a reveal carries no signature; the fold judges it");
    assert!(g.accepted(&reveal).is_empty(), "the rehearsal drops what the fold refuses");
    let err = g.fold(&reveal).expect_err("nothing is pending");
    assert!(matches!(err, PalwStateV2Error::ReporterRevealRefused { .. }), "{err}");
}

/// **The fence-off twin: both are dropped by name at the gate and refused by name in the fold**,
/// a well-signed commitment included — byte for byte the v22 skeleton's behaviour.
#[tokio::test]
async fn s7_fence_off_twin_drops_both_by_name() {
    let g = gate(false);
    let commitment = g.commitment(Hash64::from_u64_word(0x5304), 1, 1);
    let reveal = Obj::ReporterRevealed { offence_key: Hash64::from_u64_word(0x5402), reporter: g.cards[1], salt: [0x54; 32] };
    for object in [commitment, reveal] {
        let why = g.validate(&object).expect_err("dropped by name below the fence");
        assert!(why.contains("declared by the v22 layout"), "{why}");
        assert!(g.accepted(&object).is_empty());
        let err = g.fold(&object).expect_err("refused by name below the fence");
        assert!(matches!(err, PalwStateV2Error::RcoreObjectNotLanded(_)), "{err}");
    }
}
