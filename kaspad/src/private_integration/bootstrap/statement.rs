//! Honest metadata for the seeded complete-only private code fixture.
//! All three complete result roles use the same exhaustive interpreter. This
//! statement makes no independent implementation or trained-model assertion.
use kaspa_consensus_core::{
    config::params::Params,
    palw_conformance_evidence_v1::tool_root,
    palw_opv_bootstrap_v1::{
        PALW_COMPLETE_CHECK_MAX_ARTIFACT_BYTES_V1, PALW_COMPLETE_CHECK_MAX_INPUTS_V1, PALW_COMPLETE_CHECK_MAX_LEAVES_V1,
        PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1, PALW_COMPLETE_CHECK_MAX_WORK_V1, palw_onboarding_complete_check_policy_v1,
    },
};
use misaka_palw_sdk::bootstrap_check::{CompleteBootstrapArtifactV1, ConformanceCommitmentV1, RootV1, SubjectKindV1};

const DOMAIN: &[u8] = b"misaka.rfc12.private-complete-code-fixture-statement.v1";
const SOURCE: &str = include_str!("../../../../misaka-palw-tir-sketch/src/fixture.rs");
const EXPORTER: &str = include_str!("../bootstrap.rs");
const INTERPRETER: &str = include_str!("../../../../misaka-palw-tir/src/interp.rs");
const COMPLETE_CHECK: &str = include_str!("../../../../consensus/core/src/palw_opv_bootstrap_v1.rs");
const SDK_CHECK: &str = include_str!("../../../../misaka-palw-sdk/src/complete_check.rs");

pub(super) fn scope() -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.rfc12.private-complete-code-fixture-provenance.v1",
        "source": "compiled wide128_v1(37) fixture source and exact exported PALWTIR1 file digest",
        "implementations": "the same compiled exhaustive interpreter for reference/independent/backend result fields",
        "scope": "every finite input and every artifact leaf within the existing complete-check domain",
        "resourceProfile": "the consensus complete-check CPU/artifact/input/leaf/post limits, not measured host performance",
        "independentImplementationClaim": false, "trainedModelVerified": false,
        "matureUsefulWorkMeasured": false, "securitySoundnessMeasured": false, "releaseApproval": false,
    })
}

pub(super) fn for_code_fixture(
    params: &Params,
    artifact: &CompleteBootstrapArtifactV1,
    file_digest: [u8; 64],
) -> ConformanceCommitmentV1 {
    let d = artifact.domain;
    // Matches the actual processor's native route policy representation. It is
    // a fixed factory identity, not the response of an untrusted remote node.
    let mut ruleset = [0; 64];
    ruleset[..32].copy_from_slice(&params.consensus_identity_id().as_bytes());
    let source = tool_root(DOMAIN, &("source", 37u64, SOURCE, EXPORTER, file_digest));
    let implementations =
        tool_root(DOMAIN, &("same-exhaustive-interpreter-three-result-roles", INTERPRETER, COMPLETE_CHECK, SDK_CHECK));
    let test_scope =
        tool_root(DOMAIN, &("all-finite-inputs-and-leaves", d.token_bound, d.positions, d.inputs, d.leaves, d.artifact_bytes, d.work));
    let resource = tool_root(
        DOMAIN,
        &(
            "consensus-complete-limits",
            PALW_COMPLETE_CHECK_MAX_INPUTS_V1,
            PALW_COMPLETE_CHECK_MAX_LEAVES_V1,
            PALW_COMPLETE_CHECK_MAX_ARTIFACT_BYTES_V1,
            PALW_COMPLETE_CHECK_MAX_WORK_V1,
            PALW_COMPLETE_CHECK_MAX_POST_BYTES_V1 as u64,
        ),
    );
    ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: params.genesis.hash.as_bytes(),
        ruleset_id: ruleset,
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: artifact.class().as_bytes(),
        kernel_descriptor_id: artifact.kernel_descriptor_id(),
        challenge_policy_id: palw_onboarding_complete_check_policy_v1().id(),
        artifact_root: artifact.artifact_root().as_bytes(),
        program_root: artifact.program_root(),
        source_root: RootV1::Present(source),
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: artifact.layout_root(),
        verification_plan_root: artifact.verification_plan_root(),
        constraint_root: RootV1::Absent,
        implementation_set_root: implementations,
        test_scope_root: test_scope,
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: resource,
        commitment_object_id: None,
        canonical_commitment_position: None,
    }
}
