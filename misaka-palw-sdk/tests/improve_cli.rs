//! **RFC-0004 A10: `palw-class improve` writes objects the chain's own checks accept.** The binary is run
//! as an operator runs it: each kind from a JSON spec and a key file, its object read back (borsh) and held
//! to the chain's form checks and its signature to the chain's message — a policy under the owner's
//! context, the material objects under the material context, a candidate over a real composite (a LoRA
//! candidate's section loaded over its parent, its class and artifact reference the chain's), a rollback
//! under the filer's.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
use kaspa_consensus_core::palw_improve_material_v1::{
    PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1, palw_dataset_form_v1, palw_hard_case_form_v1, palw_improve_material_message_v1,
    palw_setter_keys_open_v1, palw_setter_prompts_open_v1, palw_setter_set_form_v1, palw_teacher_licence_form_v1,
    palw_teaching_artifact_form_v1,
};
use kaspa_consensus_core::palw_improve_state_v1::PalwRollbackCauseV1;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step_refute::{PALW_LOGITS_TILE_LANES, tiled_logits_scheme_id_v1};
use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_LAYOUT_VERSION_V1, PalwTirLayoutV1};
use kaspa_hashes::Hash64;
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use misaka_palw_sdk::tir_composite::{tir_composite_derive_v1, tir_composite_section_write_v1};
use misaka_palw_sdk::{PalwModelLineageV1, PalwWeightResidencyV1};
use misaka_palw_tir::TirProgramV1;
use misaka_palw_tir_artifact::{PalwTirContainerV1, write_container_v1};
use misaka_palw_tir_lower::fidelity;
use misaka_palw_tir_lower::float_ref::ParamStore;
use misaka_palw_tir_lower::float_ref::stream::Resident;
use misaka_palw_tir_lower::lower::{IntParams, LowerOpts, materialise};
use misaka_palw_tir_lower::quant::QuantPolicy;
use misaka_palw_tir_lower::weights::{Checkpoint, Overlay};

const SALT: &str = "53535353535353535353535353535353535353535353535353535353535353aa";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures")
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(name: &str) -> Self {
        let d = std::env::temp_dir().join(format!("palw-improve-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Scratch(d)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn hex(b: u8) -> String {
    Hash64::from_bytes([b; 64]).to_string()
}

fn key_file(dir: &Path, name: &str, seed: u8) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join(name);
    std::fs::write(&p, [seed; 32].iter().map(|b| format!("{b:02x}")).collect::<String>()).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    p
}

fn palw_class(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_palw-class")).args(args).output().expect("palw-class runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "palw-class {args:?} failed: {text}");
    text
}

fn object(path: &Path) -> PalwConsensusObjectV2 {
    borsh::from_slice(&std::fs::read(path).unwrap()).expect("a borsh consensus object")
}

fn public_of(seed: u8) -> Vec<u8> {
    kaspa_pq_validator_core::ValidatorKey::from_seed([seed; 32]).public_key().to_vec()
}

fn domain() -> Hash64 {
    let salt = kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(SALT).unwrap();
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(
        kaspa_consensus_core::config::drill::palw_drill_network_v1(),
        Some(&salt),
    )
    .unwrap();
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash))
}

fn verifies(public: &[u8], message: Hash64, signature: &[u8], context: &[u8]) -> bool {
    matches!(kaspa_txscript::verify_mldsa87_with_context(public, message.as_byte_slice(), signature, context), Ok(true))
}

#[test]
fn every_improve_kind_writes_an_object_the_chain_accepts() {
    let dir = Scratch::new("kinds");
    let d = &dir.0;
    let key = key_file(d, "owner.seed", 0x42);
    let bond = format!("{}:2", hex(0xB0));
    let public = public_of(0x42);
    let domain = domain();
    let common = |kind: &str, spec: &Path, out: &Path| -> Vec<String> {
        [
            "improve",
            kind,
            "--network",
            "testnet-12",
            "--drill-salt",
            SALT,
            "--key-file",
            key.to_str().unwrap(),
            "--bond",
            &bond,
            "--spec",
            spec.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]
        .map(String::from)
        .to_vec()
    };
    let run = |kind: &str, spec: serde_json::Value| -> (PathBuf, String) {
        let spec_path = d.join(format!("{kind}.json"));
        std::fs::write(&spec_path, spec.to_string()).unwrap();
        let out = d.join(format!("{kind}.obj"));
        let args = common(kind, &spec_path, &out);
        let text = palw_class(&args.iter().map(String::as_str).collect::<Vec<_>>());
        (out, text)
    };

    // 70: the policy, a drill-sized epoch; the tool says it passes the drill's ceilings.
    let (path, text) = run(
        "policy",
        serde_json::json!({
            "line": hex(1), "sequence": 1,
            "policy": {
                "usage": { "measure": "claims", "value": 3 },
                "windows": { "grid": 500, "w_collect": 60, "w_submit": 60, "w_holdout": 20, "w_eval": 120, "beacon_delay": 4, "court_margin": 170 },
                "eval": { "stages": [{ "kind": "exact_match", "open": -1, "close": -1, "key_cap": 4 }], "n": 8, "n_min": 4,
                          "delta_permille": 100, "max_new_tokens": 4, "stop_ids": [], "regression_items": 0, "safety_items": 0,
                          "setter_cap_permille": 1000, "max_eval_positions": 100000 },
                "k_max": 2
            }
        }),
    );
    assert!(text.contains("policy check") && text.contains("ok"), "{text}");
    let PalwConsensusObjectV2::ModelLineImprovementPolicySet { payload, signature } = object(&path) else { panic!("tag 70") };
    let message = kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_message_v1(domain, &payload.line_id, 1, payload.policy.as_ref());
    assert!(verifies(&public, message, &signature, kaspa_consensus_core::palw_improve_policy_v1::PALW_IMPROVE_POLICY_MLDSA87_CONTEXT));
    // An opt-out is the same object with no policy.
    let (path, _) = run("policy", serde_json::json!({ "line": hex(1), "sequence": 2, "opt_out": true }));
    assert!(matches!(object(&path), PalwConsensusObjectV2::ModelLineImprovementPolicySet { payload, .. } if payload.policy.is_none()));
    // A policy the chain would refuse is refused here, before anything is signed.
    let bad = d.join("bad.json");
    std::fs::write(&bad, serde_json::json!({ "line": hex(1), "sequence": 1, "policy": { "windows": { "grid": 1 } } }).to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(&common("policy", &bad, &d.join("bad.obj")))
        .output()
        .unwrap();
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains("would be refused"));

    // 71: a hard case, its key and salt written beside it for the reveal.
    let (path, text) = run("hard-case", serde_json::json!({ "line": hex(1), "prompt": [1, 2, 3], "reference": { "exact_key": [5, 6] } }));
    assert!(text.contains("case id"));
    let PalwConsensusObjectV2::HardCaseSubmitted { payload, submitter, signature } = object(&path) else { panic!("tag 71") };
    assert_eq!(palw_hard_case_form_v1(&payload), Ok(()));
    assert!(verifies(&public, palw_improve_material_message_v1(71, &domain, payload.as_ref(), Some(&submitter)), &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));
    let key_json: serde_json::Value = serde_json::from_slice(&std::fs::read(d.join("hard-case.obj.key.json")).unwrap()).unwrap();
    assert_eq!(key_json["key"], serde_json::json!([5, 6]));

    // 73, 74, 75: a setter set and its two reveals, which open its commitments.
    let (path, _) = run("setter-set", serde_json::json!({ "line": hex(1), "epoch": 1, "prompts": [[1, 2], [3]], "keys": [[7], [8]] }));
    let PalwConsensusObjectV2::SetterSetCommitted { payload: commitment, .. } = object(&path) else { panic!("tag 73") };
    assert_eq!(palw_setter_set_form_v1(&commitment), Ok(()));
    let PalwConsensusObjectV2::SetterSetRevealed { payload: prompts } = object(&d.join("setter-set.obj.prompts")) else { panic!("tag 74") };
    let PalwConsensusObjectV2::SetterKeysRevealed { payload: keys } = object(&d.join("setter-set.obj.keys")) else { panic!("tag 75") };
    assert_eq!(palw_setter_prompts_open_v1(&commitment, &prompts), Ok(()));
    assert_eq!(palw_setter_keys_open_v1(&commitment, &keys), Ok(()));

    // 76, 77/78, 79.
    let (path, _) = run(
        "dataset",
        serde_json::json!({ "line": hex(1), "content_root": hex(2), "items": 4, "license_classes": [hex(3)], "teacher_classes": 1, "provenance": hex(4) }),
    );
    assert!(matches!(object(&path), PalwConsensusObjectV2::DatasetRegistered { payload, .. } if palw_dataset_form_v1(&payload).is_ok()));
    let (path, _) = run(
        "artifact",
        serde_json::json!({ "line": hex(1), "kind": "answer", "task": hex(2), "teacher_type": "open_distill", "teacher_id": hex(3),
                            "license_class": hex(4), "provenance": hex(5), "output_hash": hex(6), "verification": "exact", "answer_span": [1] }),
    );
    let PalwConsensusObjectV2::TeachingArtifactCommitted { payload: commit, .. } = object(&path) else { panic!("tag 77") };
    let PalwConsensusObjectV2::TeachingArtifactRevealed { payload: artifact } = object(&d.join("artifact.obj.reveal")) else { panic!("tag 78") };
    assert_eq!(palw_teaching_artifact_form_v1(&artifact), Ok(()));
    assert_eq!(kaspa_consensus_core::palw_improve_material_v1::palw_teaching_artifact_commit_v1(&artifact), commit.commit);
    let (path, _) = run("licence", serde_json::json!({ "model_family": hex(7), "domains": [2, 1], "uses": 1, "per_use_fee": 5, "expiry_daa": 99999 }));
    let PalwConsensusObjectV2::TeacherLicenceRegistered { payload, signature } = object(&path) else { panic!("tag 79") };
    assert_eq!(palw_teacher_licence_form_v1(&payload), Ok(()));
    assert_eq!(payload.rights_holder_key, public, "the licence names the signing key");
    assert!(verifies(&public, palw_improve_material_message_v1(79, &domain, payload.as_ref(), None), &signature, PALW_IMPROVE_MATERIAL_MLDSA87_CONTEXT_V1));

    // 81: a rollback.
    let (path, _) = run("rollback", serde_json::json!({ "line": hex(1), "epoch": 3, "to_class": hex(9), "cause": "owner" }));
    assert!(matches!(object(&path), PalwConsensusObjectV2::LineageHeadRolledBack { payload, .. } if payload.cause == PalwRollbackCauseV1::Owner));
}

// ---- a candidate over a real composite ----

fn converted(adapter: &str) -> (TirProgramV1, IntParams, TirProgramV1, IntParams, u32) {
    let ad_dir = fixtures().join("hf-lora").join(adapter);
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(ad_dir.join("logits.json")).unwrap()).unwrap();
    let base_dir = fixtures().join("hf").join(meta["base"].as_str().unwrap());
    let cfg = std::fs::read_to_string(base_dir.join("config.json")).unwrap();
    let opts = LowerOpts { max_window: Some(64), ..LowerOpts::default() };
    let parent = fidelity::prepare(&cfg, &opts).expect("the parent");
    let (cand, p) = fidelity::prepare_candidate(&parent, &std::fs::read_to_string(ad_dir.join("adapter_config.json")).unwrap(), &opts)
        .expect("the candidate");
    let ck = Checkpoint::open(&base_dir).expect("checkpoint");
    let ad = Checkpoint::open(&ad_dir.join("adapter_model.safetensors")).expect("adapter");
    let (pf, _) = ParamStore::from_source(&parent.hl, &parent.binding, &ck).expect("parent params");
    let (cf, _) = ParamStore::from_source(&cand.hl, &cand.binding, &Overlay { base: &ck, over: &ad }).expect("candidate params");
    let calib = fidelity::random_sequences(parent.hl.vocab, 4, 32, 11);
    let quiet = |_: usize, _: usize| {};
    let (lp, lc) = (Resident(Arc::new(pf)), Resident(Arc::new(cf)));
    let stats_p = fidelity::calibrate(&parent.hl, &lp, &calib, &quiet).expect("parent calibration");
    let mat_p = materialise(&parent.lowered, &parent.hl, &lp, &stats_p, &QuantPolicy::default(), &quiet).expect("parent artifact");
    let own = fidelity::calibrate(&cand.hl, &lc, &calib, &quiet).expect("candidate calibration");
    let stats = fidelity::candidate_stats(&stats_p, own);
    let mat_c = materialise(&cand.lowered, &cand.hl, &lc, &stats, &QuantPolicy::default(), &quiet).expect("candidate artifact");
    (parent.lowered.program.clone(), mat_p.params, cand.lowered.program.clone(), mat_c.params, p as u32)
}

fn tiled(program: &TirProgramV1) -> TirProgramV1 {
    let mut p = program.clone();
    p.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    TirProgramV1::decode_canonical(&p.encode()).expect("still canonical")
}

fn layout(program: &TirProgramV1) -> PalwTirLayoutV1 {
    let mut commit_tiles = Vec::new();
    for (bi, b) in program.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if n.commit {
                let logits = bi == program.schedule.post as usize && ni == program.logits as usize;
                commit_tiles.push(if logits { PALW_LOGITS_TILE_LANES as u32 } else { 64 });
            }
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: 16,
        checkpoint_interval: 2,
        h_tile: 4,
        commit_tiles,
        state_tiles: program.states.iter().map(|_| 4).collect(),
    }
}

fn write(path: &Path, program: &TirProgramV1, params: &IntParams, meta: serde_json::Value) {
    write_container_v1(path, program, borsh::to_vec(&layout(program)).unwrap(), [0x61; 64], meta.to_string(), &mut |j, l| {
        params.tensors.get(&(j, l)).map(|t| t.le_bytes()).ok_or_else(|| format!("no tensor {j} {l:?}"))
    })
    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

#[test]
fn a_candidate_over_a_real_composite_names_the_chains_class_and_reference() {
    let dir = Scratch::new("candidate");
    let d = &dir.0;
    let (parent_program, parent_params, candidate_program, candidate_params, p) = converted("llama_r16");
    let (parent_program, candidate_program) = (tiled(&parent_program), tiled(&candidate_program));
    let (pp, cp, sp) = (d.join("parent.palwtir"), d.join("candidate.palwtir"), d.join("candidate.palwtirs"));
    write(&pp, &parent_program, &parent_params, serde_json::json!({ "model_id": "test/parent" }));
    write(&cp, &candidate_program, &candidate_params, serde_json::json!({ "model_id": "test/candidate", "composite": { "p": p } }));
    let lineage = TirLineageV1::new();
    lineage.load(&pp, PalwWeightResidencyV1::PageCache).expect("the parent loads");
    let parent = lineage.tir_classes().remove(0);
    let (pc, cc) = (PalwTirContainerV1::open(&pp).unwrap(), PalwTirContainerV1::open(&cp).unwrap());
    let composite = tir_composite_derive_v1(&pc, &cc, Some(parent.class_id())).expect("a composite");
    tir_composite_section_write_v1(&cc, &composite, &sp).expect("the section");
    lineage.load(&sp, PalwWeightResidencyV1::PageCache).expect("the section loads");
    let entry = lineage.tir_classes().into_iter().find(|e| e.artifact.composite_ref().is_some()).unwrap();

    let key = key_file(d, "submitter.seed", 0x43);
    let bond = format!("{}:1", hex(0xB1));
    let spec = d.join("candidate.json");
    std::fs::write(
        &spec,
        serde_json::json!({ "line": hex(1), "epoch": 2, "declarations": { "datasets": [[hex(5), 1000]], "teacher_classes": 4 } }).to_string(),
    )
    .unwrap();
    let out = d.join("candidate.obj");
    let text = palw_class(&[
        "improve", "candidate", "--network", "testnet-12", "--drill-salt", SALT, "--key-file", key.to_str().unwrap(), "--bond", &bond,
        "--spec", spec.to_str().unwrap(), "--parent", pp.to_str().unwrap(), "--section", sp.to_str().unwrap(), "--out", out.to_str().unwrap(),
    ]);
    assert!(text.contains(&entry.class_id().to_string()), "{text}");
    let PalwConsensusObjectV2::CandidateSubmitted { payload, submitter, signature } = object(&out) else { panic!("tag 80") };
    assert_eq!(payload.class_id, entry.class_id());
    assert_eq!(
        payload.artifact,
        PalwTirArtifactRefV1::Composite { parent_class: parent.class_id(), parent_root: parent.artifact_root, adapter_root: composite.adapter_root, p }
    );
    assert_eq!(payload.layout, entry.class.layout, "the candidate class's layout rides, as the chain keeps only its digest");
    assert_eq!(payload.declarations.datasets, vec![(Hash64::from_bytes([5; 64]), 1000)]);
    assert!(verifies(
        &public_of(0x43),
        kaspa_consensus_core::palw_improve_candidate_v1::palw_candidate_submission_message_v1(&domain(), payload.as_ref(), &submitter),
        &signature,
        kaspa_consensus_core::palw_improve_candidate_v1::PALW_IMPROVE_CANDIDATE_MLDSA87_CONTEXT_V1
    ));
    // A section without its parent is refused by name.
    let orphan = Command::new(env!("CARGO_BIN_EXE_palw-class"))
        .args(["improve", "candidate", "--network", "testnet-12", "--drill-salt", SALT, "--key-file", key.to_str().unwrap(), "--bond", &bond,
               "--spec", spec.to_str().unwrap(), "--parent", cp.to_str().unwrap(), "--section", sp.to_str().unwrap(), "--out", d.join("x.obj").to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!orphan.status.success(), "the full candidate is not the section's parent");
}
