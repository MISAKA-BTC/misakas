//! **RFC-0002 Phase F, step F2: the IR class, its identity, and a registration object that changes
//! nothing until its fence.**
//!
//! `ClassRegisteredTirV1` is APPENDED (tag 61) so no earlier discriminant moves; it rides the
//! stateless gate at every height (a block carrying it must be valid on this build and on an older one
//! that skips it undecoded), rents nothing, and is not a carrier a halt must let through. Below
//! `palw_tir_v1` the processor's acceptance walk drops it by name before any slot is charged and the
//! fold refuses it — so a block carrying one folds exactly as the same block without it; past the
//! fence admission v10 decides (`palw_tir_admission.rs`) and it folds (`palw_tir_registration_fold.rs`).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_object_v1;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_lifecycle_object_may_ride_v2, validate_palw_lifecycle_tx,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateV2Error,
    apply_palw_transition_v2, palw_class_registration_buyer_v1, palw_object_rent_ceiling_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirAdmissionCarriageV1, PalwTirClassV1, PalwTirLayoutV1,
    palw_genesis_registers_tir_class_v1, palw_tir_class_registration_message_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// A real canonical program: the golden `hist-window` vector's bytes.
fn program_bytes() -> Vec<u8> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs/hist-window.json");
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("the golden vector")).expect("json");
    unhex(v["program_borsh_hex"].as_str().expect("program bytes"))
}

fn class() -> PalwTirClassV1 {
    PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program_bytes(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 64,
            checkpoint_interval: 4,
            h_tile: 16,
            commit_tiles: vec![4, 4],
            state_tiles: vec![4],
        },
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    }
}

/// A canonical job (its values are the registrant's; admission counts them in F6).
fn job() -> PalwJobContextV2 {
    let z = Hash64::from_bytes([0u8; 64]);
    PalwJobContextV2 {
        version: 2,
        network_id: b"testnet-12".to_vec(),
        job_id: z,
        job_nullifier: z,
        assignment_id: z,
        execution_seed: [0u8; 32],
        model_profile_id: z,
        runtime_manifest_hash: z,
        runtime_class_id: z,
        shape_profile_id: z,
        trace_scheme_id: z,
        cu_ruleset_id: z,
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
        prompt_token_ids_hash: z,
        declared_prefill_tokens: 7,
        exact_decode_tokens: 2,
        max_context_tokens: 64,
    }
}

fn registration(class: PalwTirClassV1) -> PalwConsensusObjectV2 {
    let artifact_root = Hash64::from_bytes([0xA7; 64]);
    PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id: class.class_id(&artifact_root),
        artifact_root,
        slash_value_per_pwu: 1,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 },
        initial_target: 1 << 100,
        share_permille: 0,
        activation_daa: 0,
        admission: Box::new(PalwTirAdmissionCarriageV1 {
            class,
            canonical: job(),
            registrant_bond: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3)),
            signature: vec![9; 4627],
        }),
    }
}

#[test]
fn the_object_is_appended_after_every_existing_tag() {
    let object = registration(class());
    let bytes = borsh::to_vec(&object).expect("encodes");
    assert_eq!(bytes[0], 61, "tag 61, the next free after the audit batch's 60: no earlier discriminant moves");
    let audit = PalwConsensusObjectV2::AuditReceiptBatchV1 {
        auditor: PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(0)),
        entries: Vec::new(),
        signature: Vec::new(),
    };
    assert_eq!(borsh::to_vec(&audit).unwrap()[0], 60, "the previous last variant keeps its tag");
    let back: PalwConsensusObjectV2 = borsh::from_slice(&bytes).expect("decodes");
    assert_eq!(back, object);
}

#[test]
fn it_rides_statelessly_rents_nothing_and_takes_no_slot() {
    let object = registration(class());
    assert_eq!(palw_lifecycle_object_may_ride_v2(&object), Ok(()), "a block carrying it is valid on this build");
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap();
    assert!(validate_palw_lifecycle_tx(&payload, false).is_ok());
    assert!(validate_palw_lifecycle_tx(&payload, true).is_ok());
    assert_eq!(palw_object_rent_ceiling_v1(&object), 0, "no rent: an older build that skips it burns nothing either");
    // Past the fence it is a bought registration (a slot, the burn); below it the acceptance walk
    // drops it by name before any slot is charged (`palw_object_is_tir_v1`).
    assert_eq!(
        palw_class_registration_buyer_v1(&object),
        Some(PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3)))
    );
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_tir_v1(&object));
    assert!(!palw_h1_carrier_object_v1(&object), "a registration, not a conviction a halt must let through");

    // What an OLDER build meets: a tag its enum does not have. Under the audit declaration it is
    // tolerated at admission (and the extraction walk skips it); without it the block would fail.
    let mut unknown = payload.clone();
    unknown[2] = 62;
    assert!(borsh::from_slice::<PalwLifecycleTxPayloadV2>(&unknown).is_err(), "tag 62 is past this build's enum");
    assert!(validate_palw_lifecycle_tx(&unknown, true).is_ok(), "A-2: tolerated where palw_audit_2026_09_11 is declared");
    assert!(validate_palw_lifecycle_tx(&unknown, false).is_err(), "…and refused where it is not, which is why the fence needs it");
}

/// Below `palw_tir_v1` (dormant on testnet-12): the fold's second lock. Past it the registration
/// folds (`palw_tir_registration_fold.rs`).
#[test]
fn the_fold_refuses_it_by_name_below_the_fence() {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is V2") };
    let ctx = PalwBlockContextV2 { block: Default::default(), daa_score: 10, blue_score: 10, subsidy: 0 };
    let refused = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &bundle.state, &ctx, &[registration(class())], None);
    assert!(matches!(refused, Err(PalwStateV2Error::TirRegistrationRefused(_))), "{refused:?}");
}

#[test]
fn the_class_id_commits_to_the_program_the_layout_the_weights_and_the_tokenizer() {
    let c = class();
    let root = Hash64::from_bytes([0xA7; 64]);
    let id = c.class_id(&root);
    let decoded = c.decode_program().expect("a canonical program");
    assert_eq!(decoded.encode(), c.program, "the bytes are the program");
    assert_ne!(id, c.class_id(&Hash64::from_bytes([0xA8; 64])), "the weights");
    let mut other = c.clone();
    other.tokenizer_id = Hash64::from_bytes([0x71; 64]);
    assert_ne!(id, other.class_id(&root), "the tokenizer");
    let mut other = c.clone();
    other.layout.commit_tiles[0] = 8;
    assert_ne!(id, other.class_id(&root), "the layout");
    let mut other = c.clone();
    other.layout.checkpoint_interval = 8;
    assert_ne!(id, other.class_id(&root), "the checkpoint interval");
    let mut other = c.clone();
    let last = other.program.len() - 1;
    other.program[last] ^= 1; // the last byte of `logits_scheme_id`: another valid program
    assert_ne!(id, other.class_id(&root), "the program — its logits scheme included");
    assert!(other.decode_program().is_ok());
    assert_ne!(c.graph_ir_root(), other.graph_ir_root());
    let mut other = c.clone();
    other.program.pop();
    assert!(other.decode_program().is_err(), "bytes that are not the canonical encoding of a program do not decode");
}

#[test]
fn the_registration_message_binds_every_field_and_the_class() {
    let c = class();
    let bond = PalwBondKeyV2(kaspa_consensus_core::config::premine::premine_outpoint(3));
    let rule = PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 7 };
    let job = job();
    let root = Hash64::from_bytes([0xA7; 64]);
    let domain = Hash64::from_bytes([0xD0; 64]);
    let m = |c: &PalwTirClassV1, share: u16| {
        palw_tir_class_registration_message_v1(domain, c.class_id(&root), share, 0, &bond, root, 1, 1 << 100, &rule, &job, c)
    };
    let base = m(&c, 0);
    assert_ne!(base, m(&c, 1), "the share");
    let mut other = c.clone();
    other.layout.h_tile = 32;
    assert_ne!(base, m(&other, 0), "the class");
}

#[test]
fn no_genesis_registers_an_ir_class_before_phase_h() {
    let mut p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &mut p.palw_consensus_mode else { panic!("V2") };
    assert!(!palw_genesis_registers_tir_class_v1(bundle));
    bundle.genesis_objects.push(registration(class()));
    assert!(palw_genesis_registers_tir_class_v1(bundle));
    let refused = p.validate_palw_v2().expect_err("a genesis IR row is refused");
    assert!(refused.to_string().contains("IR class"), "refused by name: {refused}");
}
