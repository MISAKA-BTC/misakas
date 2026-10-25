//! **RFC-0003 / RFC-0004 / RFC-0022 deferred features: the fold's refusal matrix over every shipped ruleset.**
//!
//! `None` in a `Params` field is not evidence that the runtime refuses: this file drives every one of the
//! fourteen improvement objects (tags 70-82 and the case-key reveal, tag 86) through the shared
//! `apply_palw_transition_v2_with_extras` walk -- the path every network, restart and resync folds through --
//! with the state params each shipped ruleset actually builds, at several DAA heights (including
//! `u64::MAX`), singly and mixed in one block. Each must be refused by name ("before palw_improvement_v1
//! is in force"), leave the input state byte-identical, and refuse identically when replayed from the
//! same state (restart/resync fold determinism). It is a fold-level (in-process) check; it is not a
//! process-level RPC/IBD run and does not replace one (see the lane-G report).
//!
//! Rulesets that arm the improvement fence on testnet-12's int-11 flag day are the only rows exempt: they are
//! asserted to be armed only at the flag-day height, and refused below it.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::drill::{PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1};
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, MAINNET_PARAMS, PALW_T12_INT11_FLAG_DAY_DAA, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_arm_int11_flag_day_at_v1, palw_t12_drill_params_v1,
    palw_t12_launch_params_v1, palw_t12_release_v1_params, palw_t12_release_v2_params, palw_t12_release_v3_params,
    palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_candidate_v1::{PalwCandidateDeclarationsV1, PalwCandidateSubmissionV1};
use kaspa_consensus_core::palw_improve_material_v1::{
    PalwCaseKeyRevealV1, PalwCaseReferenceV1, PalwCaseSourceV1, PalwDataUseOptInV1, PalwDatasetV1, PalwFpJobFactsV1, PalwHardCaseV1,
    PalwSetterKeysRevealV1, PalwSetterSetCommitmentV1, PalwSetterSetRevealV1, PalwTeacherLicenceV1, PalwTeachingArtifactCommitV1,
    PalwTeachingArtifactV1,
};
use kaspa_consensus_core::palw_improve_state_v1::{
    PalwImprovementPolicySetV1, PalwImprovementPolicyV1, PalwImprovementPoolFundingV1, PalwLineageRollbackV1, PalwRollbackCauseV1,
    PalwTeacherClassV1, PalwTeachingArtifactKindV1, PalwVerificationTypeV1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwStateV2Error, PalwTransitionExtrasV1,
    apply_palw_transition_v2_with_extras, palw_object_is_improvement_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_consensus_core::tx::TransactionOutpoint;

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x4d; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// A salted drill ruleset with the int-11 list back to dormant (what the drill movers' own tests move from).
fn dormant_drill() -> Params {
    let mut drill = palw_t12_drill_params_v1(&salt());
    palw_t12_arm_int11_flag_day_at_v1(&mut drill, None);
    drill
}

fn h(byte: u8) -> Hash64 {
    Hash64::from_bytes([byte; 64])
}

fn bond(byte: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
}

fn policy() -> PalwImprovementPolicyV1 {
    kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_example_v1()
}

/// One of each improvement object, in tag order.
fn improvement_objects() -> Vec<PalwConsensusObjectV2> {
    let sig = vec![0xAA; 4];
    vec![
        PalwConsensusObjectV2::ModelLineImprovementPolicySet {
            payload: Box::new(PalwImprovementPolicySetV1 { line_id: h(10), sequence: 1, policy: Some(policy()) }),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::HardCaseSubmitted {
            payload: Box::new(PalwHardCaseV1 {
                line_id: h(10),
                case_id: h(11),
                domain: 1,
                prompt_ids: vec![1, 2, 3],
                reference: PalwCaseReferenceV1::ExactKey { commitment: h(12) },
                source: PalwCaseSourceV1::Setter,
                head_evidence: None,
            }),
            submitter: bond(1),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::DataUseOptIn {
            payload: Box::new(PalwDataUseOptInV1 {
                job_pin: h(13),
                claim: h(34),
                job: PalwFpJobFactsV1 {
                    job_id: h(35),
                    execution_seed: [7; 32],
                    tokenizer_id: h(36),
                    prompt_token_ids_hash: h(37),
                    prompt_tokens: 3,
                    decode_tokens_executed: 5,
                    max_context_tokens: 64,
                },
            }),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::SetterSetCommitted {
            payload: Box::new(PalwSetterSetCommitmentV1 {
                line_id: h(10),
                epoch: 1,
                set_id: h(14),
                items: 8,
                prompts_commitment: h(15),
                keys_commitment: h(16),
            }),
            setter: bond(2),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::SetterSetRevealed {
            payload: Box::new(PalwSetterSetRevealV1 { line_id: h(10), epoch: 1, set_id: h(14), prompts: vec![vec![5]], salt: h(17) }),
        },
        PalwConsensusObjectV2::SetterKeysRevealed {
            payload: Box::new(PalwSetterKeysRevealV1 { line_id: h(10), epoch: 1, set_id: h(14), keys: vec![vec![6]], salt: h(18) }),
        },
        PalwConsensusObjectV2::DatasetRegistered {
            payload: Box::new(PalwDatasetV1 {
                line_id: h(10),
                dataset_id: h(19),
                content_root: h(20),
                items: 100,
                license_classes: vec![h(21)],
                teacher_classes: PalwTeacherClassV1::PublicData.bit(),
                provenance_commitment: h(22),
            }),
            registrant: bond(3),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::TeachingArtifactCommitted {
            payload: Box::new(PalwTeachingArtifactCommitV1 { line_id: h(10), commit: h(23) }),
            teacher: bond(4),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::TeachingArtifactRevealed {
            payload: Box::new(PalwTeachingArtifactV1 {
                line_id: h(10),
                kind: PalwTeachingArtifactKindV1::Answer,
                task_id: h(11),
                teacher_type: PalwTeacherClassV1::OpenDistill,
                teacher_id: h(24),
                license_class: h(21),
                provenance_commitment: h(25),
                output_hash: h(26),
                verification_type: PalwVerificationTypeV1::Exact,
                answer_span: vec![7, 8],
                salt: h(27),
            }),
        },
        PalwConsensusObjectV2::TeacherLicenceRegistered {
            payload: Box::new(PalwTeacherLicenceV1 {
                licence_id: h(28),
                rights_holder_key: vec![1, 2, 3],
                model_family: h(29),
                domains: vec![1],
                uses: 1,
                per_use_fee: 10,
                expiry_daa: 1_000_000,
            }),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::CandidateSubmitted {
            payload: Box::new(PalwCandidateSubmissionV1 {
                line_id: h(10),
                epoch: 1,
                class_id: h(30),
                artifact: PalwTirArtifactRefV1::Composite { parent_class: h(31), parent_root: h(32), adapter_root: h(33), p: 290 },
                layout: PalwTirLayoutV1 {
                    version: PALW_TIR_LAYOUT_VERSION_V1,
                    max_context: 64,
                    checkpoint_interval: 8,
                    h_tile: 8,
                    commit_tiles: vec![16],
                    state_tiles: vec![16],
                },
                declarations: PalwCandidateDeclarationsV1 {
                    datasets: vec![(h(19), 1_000)],
                    licences: Vec::new(),
                    teacher_classes: PalwTeacherClassV1::PublicData.bit(),
                },
            }),
            submitter: bond(5),
            signature: sig.clone(),
        },
        PalwConsensusObjectV2::LineageHeadRolledBack {
            payload: Box::new(PalwLineageRollbackV1 { line_id: h(10), epoch: 1, to_class: h(31), cause: PalwRollbackCauseV1::Owner }),
            filer: bond(6),
            signature: sig,
        },
        PalwConsensusObjectV2::ImprovementPoolFunded {
            payload: Box::new(PalwImprovementPoolFundingV1 { line_id: h(10), amount: 1_000, sink_index: 1 }),
        },
    ]
}

/// Every ruleset a node can run, as it ships — those that leave the improvement fence dormant (the armed rows are [`armed_rows`]).
fn rulesets() -> Vec<(&'static str, Params)> {
    vec![
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("from(mainnet)", Params::from(NetworkId::new(NetworkType::Mainnet))),
        ("from(testnet-10)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 10))),
        ("from(testnet-11)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11))),
        ("from(devnet)", Params::from(NetworkId::new(NetworkType::Devnet))),
        ("from(simnet)", Params::from(NetworkId::new(NetworkType::Simnet))),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_launch_params_v1", palw_t12_launch_params_v1()),
        ("palw_t12_release_v1_params", palw_t12_release_v1_params()),
        ("palw_t12_release_v2_params", palw_t12_release_v2_params()),
        ("palw_t12_release_v3_params", palw_t12_release_v3_params()),
        ("palw_t12_drill_params_v1 − int-11", dormant_drill()),
    ]
}


/// The 13 appended improvement objects plus the case-key reveal (tag 86).
fn all_deferred_improvement_objects() -> Vec<PalwConsensusObjectV2> {
    let mut objects = improvement_objects();
    objects.push(PalwConsensusObjectV2::HardCaseKeyRevealed {
        payload: Box::new(PalwCaseKeyRevealV1 { line_id: h(10), case_id: h(11), key: vec![5, 6], salt: h(12) }),
    });
    objects
}

fn fold(
    state: &PalwChainStateV2,
    params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
) -> Result<(), PalwStateV2Error> {
    let ctx = PalwBlockContextV2 { block: Hash64::from_bytes([0xB1; 64]), daa_score: daa, blue_score: daa, subsidy: 0 };
    apply_palw_transition_v2_with_extras(state, params, &ctx, objects, None, false, false, false, true, &PalwTransitionExtrasV1::default())
        .map(|_| ())
}

fn assert_refused_by_name(label: &str, result: Result<(), PalwStateV2Error>) {
    match result {
        Err(PalwStateV2Error::ImprovementObjectRefused { why, .. }) => {
            assert!(why.contains("before palw_improvement_v1 is in force"), "{label}: wrong reason {why}");
        }
        other => panic!("{label}: expected a named improvement refusal, got {other:?}"),
    }
}

#[test]
fn every_deferred_improvement_object_is_refused_by_the_shared_fold_on_every_unarmed_ruleset() {
    let objects = all_deferred_improvement_objects();
    assert_eq!(objects.len(), 14);
    assert!(objects.iter().all(palw_object_is_improvement_v1));
    let mut folded_rows = 0usize;
    for (name, p) in rulesets() {
        assert!(p.palw_improvement_v1.is_none(), "{name}: this matrix is for the unarmed rows");
        let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { continue };
        folded_rows += 1;
        let state = PalwChainStateV2::genesis();
        let before = state.clone();
        for daa in [0u64, 1, 9_999_995, 1 << 40, u64::MAX / 2] {
            for object in &objects {
                let label = format!("{name} @ {daa}: {object:?}").chars().take(160).collect::<String>();
                assert_refused_by_name(&label, fold(&state, &bundle.state, daa, std::slice::from_ref(object)));
                // restart/resync replay: the same input from the same state refuses the same way
                assert_refused_by_name(&label, fold(&state, &bundle.state, daa, std::slice::from_ref(object)));
            }
            // mixed in one block: the whole block is refused, not the objects one by one
            assert_refused_by_name(&format!("{name} @ {daa}: mixed block"), fold(&state, &bundle.state, daa, &objects));
        }
        assert!(state == before, "{name}: a refused fold leaves the input state untouched");
    }
    assert!(folded_rows >= 10, "the matrix must actually reach the fold on the shipped rulesets (reached {folded_rows})");
}

/// FINDING (recorded, not changed here): testnet-12's shipped ruleset arms `palw_improvement_v1` at the int-11 flag day
/// (`PALW_T12_INT11_FLAG_DAY_DAA`), so for that row the deferral is NOT a `None`: the fold refuses below H and its
/// admission rules apply from H. The deferral for a release that must keep RFC-0004 off therefore depends on that
/// release's own fence list; this test pins the current facts so a change is visible.
#[test]
fn the_t12_int11_rows_refuse_every_improvement_object_below_the_flag_day_height() {
    let h0 = PALW_T12_INT11_FLAG_DAY_DAA.expect("the int-11 flag day has a height");
    for (name, p) in [
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
        ("from(testnet-12)", Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))),
    ] {
        let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("{name}: ConsensusV2 expected") };
        assert!(!bundle.state.improve_active_at(h0 - 1) && bundle.state.improve_active_at(h0), "{name}: armed exactly at H");
        for daa in [0, 1, h0 - 1] {
            for object in all_deferred_improvement_objects() {
                assert_refused_by_name(name, fold(&PalwChainStateV2::genesis(), &bundle.state, daa, &[object]));
            }
        }
    }
}
