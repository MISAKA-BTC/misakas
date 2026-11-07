//! **Spec 17 §17.13: the candidates lane's golden vectors** (`consensus-vectors/improve-v1/`):
//! `composite.json` — composite artifact roots and the class ids Phase F's formula gives over them — and
//! `material.json`'s companion `material-ids.json` — every id, commitment and signed message the
//! material objects derive (cases, keys, setter sets, datasets, teaching artifacts, licences, opt-in
//! pins).
//!
//! Regenerated from the pure functions and compared byte for byte with the files; `IMPROVE_BLESS=1`
//! rewrites them, which is a change of the semantics and is reviewed as one.

use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_candidate_v1::{
    PalwCandidateDeclarationsV1, PalwCandidateSubmissionV1, palw_candidate_declarations_digest_v1,
    palw_candidate_submission_message_v1,
};
use kaspa_consensus_core::palw_improve_composite_v1::palw_improve_composite_root_v1;
use kaspa_consensus_core::palw_improve_material_v1::*;
use kaspa_consensus_core::palw_improve_state_v1::{PalwTeacherClassV1, PalwTeachingArtifactKindV1, PalwVerificationTypeV1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use serde_json::{Value, json};

fn h(byte: u8) -> Hash64 {
    Hash64::from_bytes([byte; 64])
}

fn hex(h: &Hash64) -> String {
    h.to_string()
}

fn bond(byte: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: h(byte), index: 0 })
}

/// A candidate class: the program bytes only feed `graph_ir_root`, so any bytes pin the formula.
fn class(program: &[u8]) -> PalwTirClassV1 {
    PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.to_vec(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: 64,
            checkpoint_interval: 8,
            h_tile: 8,
            commit_tiles: vec![16],
            state_tiles: vec![16],
        },
        tokenizer_id: h(0x70),
    }
}

fn composite() -> Value {
    let mut rows = Vec::new();
    for (parent_class, parent_root, adapter_root, p) in [
        (h(0x01), h(0x02), h(0x03), 1u32),
        (h(0x01), h(0x02), h(0x03), 290),
        (h(0x11), h(0x02), h(0x03), 290),
        (h(0x01), h(0x12), h(0x13), 7),
    ] {
        let root = palw_improve_composite_root_v1(&parent_class, &parent_root, &adapter_root, p);
        let reference = PalwTirArtifactRefV1::Composite { parent_class, parent_root, adapter_root, p };
        assert_eq!(reference.artifact_root(), root, "one spelling of the root");
        rows.push(json!({
            "parent_class": hex(&parent_class),
            "parent_root": hex(&parent_root),
            "adapter_root": hex(&adapter_root),
            "p": p,
            "artifact_root": hex(&root),
            "class_id": hex(&class(b"a candidate's program").class_id(&root)),
        }));
    }
    json!({
        "definition": "artifact_root = H(\"misaka-palw/improve/composite-artifact/v1\", parent_class ‖ parent_root ‖ adapter_root ‖ LE u32 p); class_id = Phase F's tir_class_id_v1 over it",
        "class": { "program_utf8": "a candidate's program", "tokenizer_id": hex(&h(0x70)), "layout": "max_context 64, checkpoint 8, h_tile 8, commit [16], state [16]" },
        "rows": rows,
    })
}

fn material_ids() -> Value {
    let line = h(0x10);
    let key = vec![5u32, 6];
    let reference = PalwCaseReferenceV1::ExactKey { commitment: palw_case_key_commitment_v1(&line, &key, &h(0x51)) };
    let case_id = palw_hard_case_id_v1(&line, 3, &[1, 2, 3], &reference);
    let prompts = vec![vec![1u32], vec![2, 3]];
    let keys = vec![vec![5u32], vec![]];
    let mut set = PalwSetterSetCommitmentV1 {
        line_id: line,
        epoch: 2,
        set_id: Hash64::default(),
        items: 2,
        prompts_commitment: palw_setter_prompts_commitment_v1(&line, 2, &prompts, &h(0x5A)),
        keys_commitment: palw_setter_keys_commitment_v1(&line, 2, &keys, &h(0x5B)),
    };
    set.set_id = palw_setter_set_id_v1(&set);
    let mut dataset = PalwDatasetV1 {
        line_id: line,
        dataset_id: Hash64::default(),
        content_root: h(0x30),
        items: 10,
        license_classes: vec![h(0x53)],
        teacher_classes: PalwTeacherClassV1::PublicData.bit(),
        provenance_commitment: h(0x31),
    };
    dataset.dataset_id = palw_dataset_id_v1(&dataset);
    let artifact = PalwTeachingArtifactV1 {
        line_id: line,
        kind: PalwTeachingArtifactKindV1::Answer,
        task_id: case_id,
        teacher_type: PalwTeacherClassV1::OpenDistill,
        teacher_id: h(0x42),
        license_class: h(0x53),
        provenance_commitment: h(0x43),
        output_hash: h(0x44),
        verification_type: PalwVerificationTypeV1::Exact,
        answer_span: key.clone(),
        salt: h(0x45),
    };
    let mut licence = PalwTeacherLicenceV1 {
        licence_id: Hash64::default(),
        rights_holder_key: vec![3; PALW_IMPROVE_MLDSA87_PUBKEY_BYTES_V1],
        model_family: h(0x41),
        domains: vec![3],
        uses: PALW_IMPROVE_LICENCE_USE_TRAINING_V1,
        per_use_fee: 1,
        expiry_daa: 10_000,
    };
    licence.licence_id = palw_teacher_licence_id_v1(&licence);
    let job = PalwFpJobFactsV1 {
        job_id: h(0x61),
        execution_seed: [9; 32],
        tokenizer_id: h(0x70),
        prompt_token_ids_hash: h(0x62),
        prompt_tokens: 3,
        decode_tokens_executed: 4,
        max_context_tokens: 64,
    };
    let network = h(0xA1);
    let case = PalwHardCaseV1 {
        line_id: line,
        case_id,
        domain: 3,
        prompt_ids: vec![1, 2, 3],
        reference,
        source: PalwCaseSourceV1::Setter,
        head_evidence: None,
    };
    let declarations = PalwCandidateDeclarationsV1 {
        datasets: vec![(dataset.dataset_id, 1_000)],
        licences: vec![licence.licence_id],
        teacher_classes: PalwTeacherClassV1::LicensedDistill.bit(),
    };
    let candidate = PalwCandidateSubmissionV1 {
        line_id: line,
        epoch: 2,
        class_id: h(0x20),
        artifact: PalwTirArtifactRefV1::Composite { parent_class: h(0x01), parent_root: h(0x02), adapter_root: h(0x03), p: 290 },
        layout: class(b"").layout,
        declarations: declarations.clone(),
    };
    json!({
        "line_id": hex(&line),
        "case": { "domain": 3, "prompt_ids": [1, 2, 3], "key": key, "salt": hex(&h(0x51)), "key_commitment": hex(&palw_case_key_commitment_v1(&line, &key, &h(0x51))), "case_id": hex(&case_id) },
        "setter_set": {
            "epoch": 2, "prompts": prompts, "keys": keys,
            "prompts_commitment": hex(&set.prompts_commitment), "keys_commitment": hex(&set.keys_commitment), "set_id": hex(&set.set_id),
        },
        "dataset_id": hex(&dataset.dataset_id),
        "artifact_commit": hex(&palw_teaching_artifact_commit_v1(&artifact)),
        "licence_id": hex(&licence.licence_id),
        "opt_in_pin": hex(&job.pin()),
        // Tag 86: the key reveal as a consensus object, its borsh bytes (the discriminant first).
        "key_reveal_object_tag86": borsh::to_vec(&PalwConsensusObjectV2::HardCaseKeyRevealed {
            payload: Box::new(PalwCaseKeyRevealV1 { line_id: line, case_id, key: key.clone(), salt: h(0x51) }),
        })
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>(),
        "messages": {
            "hard_case_tag71": hex(&palw_improve_material_message_v1(71, &network, &case, Some(&bond(2)))),
            "licence_tag79": hex(&palw_improve_material_message_v1(79, &network, &licence, None)),
            "candidate_tag80": hex(&palw_candidate_submission_message_v1(&network, &candidate, &bond(2))),
        },
        "declarations_digest": hex(&palw_candidate_declarations_digest_v1(&declarations)),
        "contexts": {
            "material": String::from_utf8(PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1.to_vec()).unwrap(),
            "candidate": String::from_utf8(kaspa_consensus_core::palw_improve_candidate_v1::PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1.to_vec()).unwrap(),
        },
    })
}

fn check(name: &str, value: Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/improve-v1").join(name);
    let text = serde_json::to_string_pretty(&value).unwrap() + "\n";
    if std::env::var("IMPROVE_BLESS").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (IMPROVE_BLESS=1 writes it)", path.display()));
    assert_eq!(on_disk, text, "{name} moved: a change of the semantics, reviewed as one");
}

#[test]
fn the_composite_roots_and_class_ids_are_the_pinned_ones() {
    check("composite.json", composite());
}

#[test]
fn the_material_ids_commitments_and_messages_are_the_pinned_ones() {
    check("material-ids.json", material_ids());
}
