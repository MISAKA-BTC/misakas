//! **RFC-0002 Phase F: one IR class registration a block reaches admission v10**
//! (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`), at the processor's acceptance walk, on testnet-12 as
//! launched with `palw_tir_v1` armed from genesis and harness keys on the eight cards.
//!
//! Sizing an IR program costs every node up to seconds, so the walk hands at most one
//! `ClassRegisteredTirV1` a block to the gate; a further one is dropped by name at the top of the
//! walk, before any rent, slot or fee, exactly as a below-fence IR object is, and the block stands.
//! A registration dropped before the gate (here: one nobody signed) never takes the place. The fold
//! refuses a second as its second lock (`TirRegistrationsPerBlockExceeded`). Every registration here
//! is admissible on its own — each one alone is accepted — so the only thing that drops the second
//! is the cap.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::ConfigBuilder;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{
    PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1, PalwBlockContextV2, PalwBondKeyV2, PalwConsensusObjectV2 as Obj, PalwStateV2Error,
};
use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1,
    PalwTirLayoutV1, palw_tir_class_registration_message_v1,
};
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
use kaspa_hashes::Hash64;
use std::path::PathBuf;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// The corpus's `moe-top2-shared` model as an IR class at 64 positions (the layout
/// `consensus/core/tests/palw_tir_fixture_common.rs` declares), under tokenizer `[tokenizer; 64]`.
fn class(tokenizer: u8) -> PalwTirClassV1 {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs/moe-top2-shared.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the corpus")).expect("json");
    let mut class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: unhex(v["program_borsh_hex"].as_str().expect("the program")),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 64,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: Vec::new(),
            state_tiles: Vec::new(),
        },
        tokenizer_id: Hash64::from_bytes([tokenizer; 64]),
    };
    let mut program = class.decode_program().expect("canonical");
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let mut k = 0u32;
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let is_logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                class.layout.commit_tiles.push(if is_logits { 4096 } else { 4 + (k * 7) % 6 });
                k += 1;
            }
        }
    }
    class.layout.state_tiles = (0..program.states.len() as u32).map(|j| 4 + j % 3).collect();
    class.program = program.encode();
    class
}

/// Card `card`'s signed registration of `class`, at the chain's floor target and slash value,
/// weightless (the share rule's 0‰ past admission independence), active at `daa`.
#[allow(clippy::too_many_arguments)]
fn registration(
    class: PalwTirClassV1,
    card: usize,
    registrant: PalwBondKeyV2,
    target: u128,
    slash: u64,
    daa: u64,
    ladder: u64,
    domain: Hash64,
    sign: bool,
) -> Obj {
    let root = Hash64::from_bytes([0x42; 64]);
    let facts = PalwTirJobFactsV1::of_class(&class, class.class_id(&root)).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).expect("wide enough"));
    let mut object = palw_tir_post_genesis_registration_v1(class, canonical, root, 0, target, slash, daa, registrant, Vec::new(), ladder)
        .expect("the builder counts the canonical job");
    if sign {
        let Obj::ClassRegisteredTirV1 {
            class_id,
            share_permille,
            activation_daa,
            artifact_root,
            slash_value_per_pwu,
            initial_target,
            pwu_rule,
            admission,
        } = &mut object
        else {
            unreachable!()
        };
        let message = palw_tir_class_registration_message_v1(
            domain,
            *class_id,
            *share_permille,
            *activation_daa,
            &admission.registrant_bond,
            *artifact_root,
            *slash_value_per_pwu,
            *initial_target,
            pwu_rule,
            &admission.canonical,
            &admission.class,
        );
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        admission.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
            [0x61u8; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec();
    }
    object
}

fn class_id_of(o: &Obj) -> Hash64 {
    match o {
        Obj::ClassRegisteredTirV1 { class_id, .. } => *class_id,
        _ => unreachable!(),
    }
}

#[tokio::test]
async fn t12_one_ir_registration_a_block_reaches_admission_v10() {
    assert_eq!(PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1, 1);
    let (config, _, premine, floats) = t12_with_harness_cards();
    let config = {
        let mut params = config.params.clone();
        params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
        params.sync_palw_tir_v1();
        let armed = ConfigBuilder::new(params).skip_proof_of_work().build();
        armed.params.validate_palw_v2().expect("testnet-12 with the IR armed from genesis is a runnable ruleset");
        armed
    };
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let chain = t12_genesis_chain(&config, bundle, &premine, &floats);
    let vp = chain.vp();
    let (block, state) = chain.tip_state();
    let point = PalwBlockContextV2 { block, daa_score: chain.daa_of(block), blue_score: 1, subsidy: 0 };
    let floor = bundle.base_class_id;
    let target = state.class_target(&floor).expect("the floor's target").target;
    let slash = state.class(&floor).expect("the floor class").slash_value_per_pwu;
    let ladder = bundle.court.max_step_leaf_count();
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let by = |tokenizer: u8, card: usize, sign: bool| {
        registration(class(tokenizer), card, chain.bonds[card], target, slash, point.daa_score, ladder, domain, sign)
    };
    let (a, b, c) = (by(0x71, 1, true), by(0x72, 2, true), by(0x73, 3, true));
    let unsigned = by(0x74, 4, false);
    let ids: Vec<Hash64> = [&a, &b, &c, &unsigned].iter().map(|o| class_id_of(o)).collect();
    assert!(ids.windows(2).all(|w| w[0] != w[1]), "four classes");

    let accepted = |objects: Vec<Obj>| vp.palw_v2_accepted_objects_for_tests(&state, &bundle.state, &point, objects, block);
    // Each alone reaches admission v10 and is admitted: only the cap can drop one below.
    for one in [&a, &b, &c] {
        assert_eq!(accepted(vec![one.clone()]), vec![one.clone()], "an admissible IR registration alone is accepted");
    }
    // Three signed ones: the first reaches v10, the rest are dropped by name.
    assert_eq!(accepted(vec![a.clone(), b.clone(), c.clone()]), vec![a.clone()], "one a block reaches admission v10");
    assert_eq!(accepted(vec![c.clone(), a.clone()]), vec![c.clone()], "whichever comes first");
    // One nobody signed, first in line, is dropped before the gate and does not take the place.
    assert_eq!(accepted(vec![unsigned.clone(), b.clone(), a.clone()]), vec![b.clone()], "a copy nobody signed takes no place");

    // The fold's second lock on the same number.
    vp.palw_v2_fold_accepted_for_tests(&state, &bundle.state, &point, std::slice::from_ref(&a)).expect("one folds");
    let two = vp.palw_v2_fold_accepted_for_tests(&state, &bundle.state, &point, &[a.clone(), b.clone()]);
    assert!(matches!(two, Err(PalwStateV2Error::TirRegistrationsPerBlockExceeded { max: 1, .. })), "{two:?}");
}
