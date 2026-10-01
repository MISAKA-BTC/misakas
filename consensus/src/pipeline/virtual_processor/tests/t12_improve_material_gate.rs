//! **RFC-0004 A4/A5 at the processor**: the material objects' and the candidate's signatures in the
//! object gate, and a licence through the acceptance walk and the fold, on testnet-12 as launched with
//! the IR fences, the generative fence and `palw_improvement_v1` armed from genesis and harness keys on
//! the eight cards.
//!
//! A bond-signed material object is signed by its bond's key over its tag, the network, the payload
//! and the bond; the same signature under another bond, another tag or another payload is refused. A
//! licence is signed by its rights holder's own key. A correctly signed object the fold refuses (here:
//! a case for a line nobody governs) passes the gate and is dropped by the walk's rehearsal.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1;
use kaspa_consensus_core::palw_improve_candidate_v1::{
    PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1, PalwCandidateDeclarationsV1, PalwCandidateSubmissionV1,
    palw_candidate_submission_message_v1,
};
use kaspa_consensus_core::palw_improve_material_v1::{
    PALW_IMPROVE_LICENCE_USE_TRAINING_V1, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1, PalwCaseReferenceV1, PalwCaseSourceV1,
    PalwHardCaseV1, PalwTeacherLicenceV1, palw_hard_case_id_v1, palw_improve_material_message_v1, palw_teacher_licence_id_v1,
};
use kaspa_consensus_core::palw_improve_v1::PalwImprovementFenceV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;

fn h(byte: u8) -> Hash64 {
    Hash64::from_bytes([byte; 64])
}

fn sign(key: &libcrux_ml_dsa::ml_dsa_87::MLDSA87SigningKey, message: &Hash64, context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(key, message.as_byte_slice(), context, [0x62u8; 32]).expect("ML-DSA-87 signs").as_ref().to_vec()
}

#[tokio::test]
async fn t12_material_objects_are_signed_by_their_signers() {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let config = {
        let mut params = config.params.clone();
        params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
        params.sync_palw_tir_v1();
        params.palw_tir_fence2 = Some(ForkActivation::new(0));
        params.sync_palw_tir_fence2();
        params.palw_gen_v1 = Some(PalwGenFenceV1::drill_v1(ForkActivation::new(0)));
        params.sync_palw_gen_v1();
        params.palw_improvement_v1 = Some(PalwImprovementFenceV1::drill_v1(ForkActivation::new(0)));
        params.sync_palw_improvement_v1();
        let armed = ConfigBuilder::new(params).skip_proof_of_work().build();
        armed.params.validate_palw_v2().expect("testnet-12 with the improvement protocol armed from genesis is a runnable ruleset");
        armed
    };
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
    let vp = chain.vp();
    let (block, state) = chain.tip_state();
    let point = PalwBlockContextV2 { block, daa_score: chain.daa_of(block), blue_score: 1, subsidy: 0 };
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let gate = |object: &Obj| vp.palw_v2_validate_objects(&state, &bundle.state, &point, std::slice::from_ref(object));
    let accepted = |objects: Vec<Obj>| vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, objects, block);

    // **A licence, signed by its rights holder's own key**, through the gate, the walk and the fold.
    let holder = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x77u8; 32]);
    let mut licence = PalwTeacherLicenceV1 {
        licence_id: Hash64::default(),
        rights_holder_key: holder.verification_key.as_ref().to_vec(),
        model_family: h(0x41),
        domains: vec![3],
        uses: PALW_IMPROVE_LICENCE_USE_TRAINING_V1,
        per_use_fee: 1,
        expiry_daa: point.daa_score + 10_000,
    };
    licence.licence_id = palw_teacher_licence_id_v1(&licence);
    let licence_message = palw_improve_material_message_v1(79, &domain, &licence, None);
    let registered = Obj::TeacherLicenceRegistered {
        payload: Box::new(licence.clone()),
        signature: sign(&holder.signing_key, &licence_message, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1),
    };
    gate(&registered).expect("a licence signed by its rights holder");
    assert_eq!(accepted(vec![registered.clone()]), vec![registered.clone()], "the walk and the fold take it");
    let stranger = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x78u8; 32]);
    let forged = Obj::TeacherLicenceRegistered {
        payload: Box::new(licence.clone()),
        signature: sign(&stranger.signing_key, &licence_message, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1),
    };
    assert!(gate(&forged).unwrap_err().contains("rights holder"), "another key's signature");
    assert!(accepted(vec![forged]).is_empty(), "and the walk drops it");
    let under_another_tag = Obj::TeacherLicenceRegistered {
        payload: Box::new(licence.clone()),
        signature: sign(
            &holder.signing_key,
            &palw_improve_material_message_v1(76, &domain, &licence, None),
            PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1,
        ),
    };
    assert!(gate(&under_another_tag).is_err(), "a signature over another tag's message");

    // **A hard case, signed by its submitter bond's key** (card 1's harness key signs for card 1).
    let line = h(0x10);
    let reference = PalwCaseReferenceV1::None;
    let case = PalwHardCaseV1 {
        line_id: line,
        case_id: palw_hard_case_id_v1(&line, 3, &[1, 2, 3], &reference),
        domain: 3,
        prompt_ids: vec![1, 2, 3],
        reference,
        source: PalwCaseSourceV1::Setter,
        head_evidence: None,
    };
    let card = |n: usize| TestConsensus::palw_v2_registry_keypair(n as u64);
    let case_signed = |by: usize, as_bond: usize| {
        let message = palw_improve_material_message_v1(71, &domain, &case, Some(&chain.bonds[as_bond]));
        Obj::HardCaseSubmitted {
            payload: Box::new(case.clone()),
            submitter: chain.bonds[as_bond],
            signature: sign(&card(by).signing_key, &message, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1),
        }
    };
    let honest = case_signed(1, 1);
    gate(&honest).expect("a case signed by its submitter bond");
    assert!(gate(&case_signed(2, 1)).is_err(), "another card's key under card 1's bond");
    let lifted = match case_signed(1, 1) {
        Obj::HardCaseSubmitted { payload, signature, .. } => Obj::HardCaseSubmitted { payload, submitter: chain.bonds[2], signature },
        _ => unreachable!(),
    };
    assert!(gate(&lifted).is_err(), "a signature lifted onto another bond");
    // The gate passes it; the fold refuses a case for a line nobody governs, so the walk drops it.
    assert!(accepted(vec![honest]).is_empty(), "no governed line holds it");

    // **A candidate, signed by its submitter bond's key**; the acceptance half then asks the line.
    let candidate = PalwCandidateSubmissionV1 {
        line_id: line,
        epoch: 1,
        class_id: h(0x20),
        artifact: kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1::Single { root: h(0x21) },
        layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1 {
            version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 8,
            checkpoint_interval: 1,
            h_tile: 1,
            commit_tiles: Vec::new(),
            state_tiles: Vec::new(),
        },
        declarations: PalwCandidateDeclarationsV1 { datasets: Vec::new(), licences: Vec::new(), teacher_classes: 0 },
    };
    let candidate_message = palw_candidate_submission_message_v1(&domain, &candidate, &chain.bonds[3]);
    let submitted = |key: usize| Obj::CandidateSubmitted {
        payload: Box::new(candidate.clone()),
        submitter: chain.bonds[3],
        signature: sign(&card(key).signing_key, &candidate_message, PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1),
    };
    assert!(gate(&submitted(4)).unwrap_err().contains("does not verify"), "another card's key");
    let err = gate(&submitted(3)).expect_err("a signed candidate for a line nobody governs");
    assert!(err.contains("not governed"), "past the signature, the acceptance half asks the line: {err}");
}
