//! Direct registry DATA fixtures for expected-field derivation, not accepted
//! Assignment intake, eligibility, UTXO funding, leases, Court or credit.
use super::*;
use crate::{
    palw_positive_assignment_v2::AssignmentJobBindingV2, palw_positive_metadata_v2::PositiveMetadataLimitsV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
};
use misaka_palw_positive_contract::{assignment::*, identity::*, messages::*, rows::*};

fn digest(n: u8) -> Digest {
    Digest([n; 64])
}
fn outpoint(n: u8) -> NativeOutpoint {
    NativeOutpoint { transaction: NativeHash64([n; 64]), index: 0 }
}
fn native_bond(p: NativeOutpoint) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(crate::TransactionId::from_bytes(p.transaction.0), p.index))
}
fn fixture() -> (PalwChainStateV2, AssignmentTermsV2, crate::palw_tir_class_v1::PalwTirLayoutV1) {
    let execution = crate::palw_tir_court_v1::test_support::tiny_execution(None);
    let mut definition = execution.binding.class;
    definition.layout.max_context = 64;
    let artifact_root = execution.binding.artifact_root;
    let id = definition.class_id(&artifact_root);
    let record = crate::palw_tir_admission_v1::palw_tir_class_record_v1(&definition, &artifact_root).unwrap().0;
    let mut state = PalwChainStateV2::genesis();
    state.classes.insert(
        id,
        PalwClassStateV2 {
            artifact_root,
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
            status: PalwClassStatusV2::Active,
            registered_daa: 0,
            registrant_bond: None,
            fused_attention: false,
        },
    );
    state.tir_classes.insert(id, record);
    let mut payer = PrincipalId(digest(0));
    for n in 1..=3 {
        let key = libcrux_ml_dsa::ml_dsa_87::portable::generate_key_pair([n; 32]);
        let public: &[u8; PUBKEY_BYTES] = key.verification_key.as_ref();
        if n == 1 {
            payer = principal(public);
        }
        state.bonds.insert(
            native_bond(outpoint(n)),
            palw_bond_state_from_registration_v2(public, public, 100_000_000, Hash64::default(), 0, [id].into_iter().collect()),
        );
    }
    let terms = AssignmentTermsV2 {
        version: 2,
        revision: 1,
        network: digest(11),
        ruleset: digest(12),
        branch_anchor: NativeHash64([13; 64]),
        mode: AssignmentModeV2::CanonicalTirAssignmentV1,
        registered_class: Digest(id.as_bytes()),
        poster: outpoint(1),
        worker: outpoint(2),
        prover: outpoint(3),
        payer,
        refund_payload: digest(14),
        poster_nonce: 0,
        derivation_policy: digest(15),
        source_policy: digest(16),
        reserve_schedule: digest(17),
        worker_reward: 100,
        max_assignment_fee: 20,
        max_assignment_capital: 30,
        max_assignment_locked: 200,
        accept_before_daa: 100,
        result_before_daa: 200,
        reserve_until_daa: 500,
    };
    (state, terms, definition.layout)
}
fn limits() -> PositiveMetadataLimitsV2 {
    PositiveMetadataLimitsV2 { max_program_bytes: 1 << 20, max_layout_entries: 1 << 16, max_prompt_tokens: 64 }
}
fn result(binding: &AssignmentJobBindingV2) -> MinerResultCommit {
    let a = binding.admission();
    let job = binding.job();
    MinerResultCommit {
        version: 2,
        revision: 1,
        scope: a.scope,
        job: a.expected_job,
        execution: a.expected_execution,
        miner: a.expected_miner,
        registered_class: a.registered_class,
        artifact: a.artifact,
        program: a.program,
        input: a.expected_input,
        tokenizer: a.tokenizer,
        execution_rules: a.execution_rules,
        selector: a.selector,
        generation_seed: job.execution_seed,
        temperature_q24: 0,
        prefill: job.declared_prefill_tokens,
        decode: job.exact_decode_tokens,
        max_context: job.max_context_tokens,
        output: digest(90),
        dependencies: binding.terms().empty_dependencies_id(),
        service_contract: digest(91),
    }
}

#[test]
fn assigned_result_binds_every_native_expected_field_and_canonical_count() {
    let (state, terms, layout) = fixture();
    let binding = AssignmentJobBindingV2::derive(&state, terms, &layout, limits(), PalwPromptIdsFormV1::Flat).unwrap();
    let original = result(&binding);
    binding.check_result(&original).unwrap();
    assert_eq!((original.prefill, original.decode), (7, 2));
    assert_eq!(original.max_context, 65);
    assert_eq!(original.tokenizer, digest(0));
    assert_eq!(original.execution_rules, digest(0));
    assert_eq!(original.job, terms.id().0);
    for mutate in [
        |r: &mut MinerResultCommit| r.scope.network = digest(40),
        |r: &mut MinerResultCommit| r.scope.ruleset = digest(41),
        |r: &mut MinerResultCommit| r.scope.branch = digest(42),
        |r: &mut MinerResultCommit| r.job = digest(43),
        |r: &mut MinerResultCommit| r.execution = digest(44),
        |r: &mut MinerResultCommit| r.miner = PrincipalId(digest(45)),
        |r: &mut MinerResultCommit| r.registered_class = digest(46),
        |r: &mut MinerResultCommit| r.artifact = digest(47),
        |r: &mut MinerResultCommit| r.program = digest(48),
        |r: &mut MinerResultCommit| r.input = digest(49),
        |r: &mut MinerResultCommit| r.tokenizer = digest(50),
        |r: &mut MinerResultCommit| r.execution_rules = digest(51),
        |r: &mut MinerResultCommit| r.selector = digest(52),
        |r: &mut MinerResultCommit| r.generation_seed = [53; 32],
        |r: &mut MinerResultCommit| r.temperature_q24 = 1,
        |r: &mut MinerResultCommit| r.prefill = 1,
        |r: &mut MinerResultCommit| r.decode = 1,
        |r: &mut MinerResultCommit| r.max_context = 64,
        |r: &mut MinerResultCommit| r.dependencies = digest(54),
    ] {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(binding.check_result(&changed).is_err());
    }
    let mut changed = original;
    changed.output = digest(92);
    binding.check_result(&changed).unwrap(); // A signed output still needs the independent full Court.
    let mut changed_terms = terms;
    changed_terms.poster_nonce += 1;
    let other = AssignmentJobBindingV2::derive(&state, changed_terms, &layout, limits(), PalwPromptIdsFormV1::Flat).unwrap();
    assert!(other.check_result(&changed).is_err());
}

#[test]
fn missing_or_changed_registry_and_worker_key_never_use_result_supplied_expectations() {
    let (mut state, terms, mut layout) = fixture();
    let before = AssignmentJobBindingV2::derive(&state, terms, &layout, limits(), PalwPromptIdsFormV1::Flat).unwrap();
    let r = result(&before);
    state.bonds.remove(&native_bond(terms.worker));
    assert!(AssignmentJobBindingV2::derive(&state, terms, &layout, limits(), PalwPromptIdsFormV1::Flat).is_err());
    let (state, _, _) = fixture();
    layout.checkpoint_interval += 1;
    assert!(AssignmentJobBindingV2::derive(&state, terms, &layout, limits(), PalwPromptIdsFormV1::Flat).is_err());
    assert!(
        AssignmentJobBindingV2::derive(&PalwChainStateV2::genesis(), terms, &layout, limits(), PalwPromptIdsFormV1::Flat).is_err()
    );
    assert_eq!(r.miner, before.admission().expected_miner);
}
