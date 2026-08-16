//! Offline end-to-end: the real `palw-reexecutor` binary driven over a mock worker — every
//! §9.3 step short of real inference, no node, no model, no network. This is the same bar the
//! shadow drill set ("offline全経路検証済み"): probe → scan → qualify → capability twice
//! (nonce monotonicity) → the quarantine path (a failing selftest starves the ready set, the
//! emission refuses WITHOUT burning a nonce, and recovery issues nonce 1).
//!
//! The mock worker enforces the real worker's `prefill + decode ≤ context` admission rule,
//! so the decode-override cap is exercised here — the fleet-blocking shape a permissive mock
//! once hid. The emitted record is parsed back through the TYPED `CapabilityRecordV1`, and
//! the signature is verified with the actual ML-DSA-87 key — a wrong-context or wrong-bytes
//! signing regression fails here, not on a Stage-1 verifier.
#![cfg(unix)]

use kaspa_consensus_core::palw_routing::{
    PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT, PalwVerifierCapabilityV1, verifier_capability_message_v1, verify_ready_binding_v1,
};
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use misaka_palw_reexecutor::fixtures::{definition_with, test_binding_with_artifact};
use misaka_palw_reexecutor::{CapabilityRecordV1, hex64, parse_hash64};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_palw-reexecutor")
}

fn write_executable(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A mock worker covering the three JSON modes the agent drives. The manifest reports the
/// SAME identities the binding row pins (class, runtime manifest, model pin), so admission
/// holds; the bench enforces the real worker's `12-prefill + decode ≤ 4096` rule and dies on
/// an over-budget override exactly as the pinned worker would; the bench numbers fit the
/// two-minute replay window at κ.
fn mock_worker_script(class_id_hex: &str, manifest_hex: &str, model_hex: &str, selftest_exit: i32) -> String {
    format!(
        r#"#!/bin/sh
mode="$2"
case "$mode" in
  v2-manifest)
    echo '{{"schema":"misaka.palw.v2-manifest.debug","runtime_class_id":"{class_id_hex}","runtime_manifest_hash_v2":"{manifest_hex}","model_profile_id":"{model_hex}"}}'
    ;;
  v2-selftest)
    if [ {selftest_exit} -ne 0 ]; then
      echo "mock selftest FAILED" 1>&2
      exit {selftest_exit}
    fi
    echo '{{"schema":"misaka.palw.v2-selftest.debug","status":"pass"}}'
    ;;
  v2-replay-bench)
    decode=0
    while [ $# -gt 0 ]; do
      if [ "$1" = "--decode" ]; then decode="$2"; fi
      shift
    done
    if [ "$decode" -gt 4084 ]; then
      echo "mock worker: --decode $decode is not a valid budget: 12 prefill + $decode > 4096 context" 1>&2
      exit 1
    fi
    echo '{{"schema":"misaka.palw.v2-replay-bench.v1","runs":3,"total_ms":{{"p50":600000,"p95":650000,"p99":680000,"max":700000}},"roots_identical_across_runs":true}}'
    ;;
  *)
    echo "mock worker: unknown mode $mode" 1>&2
    exit 2
    ;;
esac
"#
    )
}

fn run_ok(args: &[&str]) -> String {
    let out = Command::new(bin()).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "expected success for {args:?}\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn run_err(args: &[&str]) -> String {
    let out = Command::new(bin()).args(args).output().unwrap();
    assert!(!out.status.success(), "expected failure for {args:?}, got success");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

struct Harness {
    dir: PathBuf,
    policy_path: PathBuf,
    key_path: PathBuf,
    state_dir: PathBuf,
    binding_id_hex: String,
    class_id_hex: String,
    manifest_hex: String,
    model_hex: String,
}

impl Harness {
    fn swap_worker(&self, selftest_exit: i32) {
        write_executable(
            &self.dir.join("mock-worker.sh"),
            &mock_worker_script(&self.class_id_hex, &self.manifest_hex, &self.model_hex, selftest_exit),
        );
    }
}

fn harness(name: &str) -> Harness {
    let dir = std::env::temp_dir().join(format!("palw-reexecutor-e2e-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // A small deterministic mock artifact; its REAL digest goes into the signed definition.
    let artifact_bytes: Vec<u8> = (0u32..1024).flat_map(|i| i.to_le_bytes()).collect();
    let artifact_path = dir.join("mock-model.gguf");
    std::fs::write(&artifact_path, &artifact_bytes).unwrap();
    let digest: [u8; 32] = Sha256::digest(&artifact_bytes).into();

    let binding = test_binding_with_artifact(artifact_bytes.len() as u64);
    let definition = definition_with(binding.model_profile_id, artifact_bytes.len() as u64, digest);
    let binding_path = dir.join("binding.bin");
    let definition_path = dir.join("definition.bin");
    std::fs::write(&binding_path, borsh::to_vec(&binding).unwrap()).unwrap();
    std::fs::write(&definition_path, borsh::to_vec(&definition).unwrap()).unwrap();

    let h = Harness {
        binding_id_hex: hex64(&binding.registration_id()),
        class_id_hex: hex64(&binding.runtime_class_id),
        manifest_hex: hex64(&binding.runtime_manifest_hash),
        model_hex: hex64(&binding.model_profile_id),
        state_dir: dir.join("state"),
        policy_path: dir.join("policy.toml"),
        key_path: dir.join("reexecutor.seed"),
        dir,
    };
    h.swap_worker(0);

    std::fs::write(
        &h.policy_path,
        format!(
            r#"network_id = "misaka-palw-drill/v1"
network = "two-minute"
worker_bin = "{worker}"
golden_set = "{golden}"
model_paths = ["{artifact}"]
binding_paths = ["{binding}"]
definition_paths = ["{definition}"]
allow_models = ["*"]
max_band = "B1"
max_concurrency = 2
max_accepted_replay_secs = 3600
total_memory_bytes = 17179869184
ttl_daa = 15
heartbeat_secs = 300
state_dir = "{state}"
"#,
            worker = h.dir.join("mock-worker.sh").display(),
            golden = h.dir.join("golden.json").display(),
            artifact = artifact_path.display(),
            binding = binding_path.display(),
            definition = definition_path.display(),
            state = h.state_dir.display(),
        ),
    )
    .unwrap();

    run_ok(&["keygen", "--out", h.key_path.to_str().unwrap()]);
    h
}

#[test]
fn the_whole_sequence_emits_a_verifiable_capability_and_the_nonce_only_moves_forward() {
    let h = harness("happy");
    let config = h.policy_path.to_str().unwrap();
    let key = h.key_path.to_str().unwrap();

    // Probe names the backend (and the worker's single model pin) from the manifest alone.
    let probe: serde_json::Value = serde_json::from_str(&run_ok(&["probe", "--config", config])).unwrap();
    assert_eq!(probe["class_tag"], "misaka-palw-lite-cpu/x86_64/v1");
    assert_eq!(probe["execution_family"], "Cpu");
    assert_eq!(probe["model_profile_id"], h.model_hex.as_str());

    // Scan admits exactly the one held binding.
    let scan: serde_json::Value = serde_json::from_str(&run_ok(&["scan", "--config", config])).unwrap();
    assert_eq!(scan["eligible"].as_array().unwrap().len(), 1, "scan: {scan}");
    assert_eq!(scan["eligible"][0]["binding_id"], h.binding_id_hex.as_str());

    // Qualify runs goldens + bench through the mock. The mock enforces the worker's
    // prefill+decode≤context rule, so this passing PROVES the decode override was capped
    // (the raw ceiling 4095 would die; the capped 4084 runs).
    run_ok(&["qualify", "--config", config]);
    let quals = std::fs::read_to_string(h.state_dir.join("qualifications.jsonl")).unwrap();
    assert_eq!(quals.lines().count(), 1);
    assert!(quals.contains("\"selftest_passed\":true"), "quals: {quals}");

    // Capability: assembled, signed, self-verified, written — parsed back through the TYPED
    // record, so a field rename breaks here and not in a downstream matcher.
    let record: CapabilityRecordV1 =
        serde_json::from_str(&run_ok(&["capability", "--config", config, "--key", key, "--now-daa", "1000"])).unwrap();
    assert_eq!(record.capability_nonce, 1);
    assert_eq!(record.availability_expiry_daa, 1_015);
    assert_eq!(record.max_model_band, "B1");
    assert_eq!(record.ready_bindings.len(), 1);
    assert_eq!(record.ready_bindings[0].binding_id, h.binding_id_hex);
    assert!(record.not_ready.is_empty());

    // Independently re-verify everything the record claims, from the borsh bytes up.
    let cap_bytes = {
        let hex = &record.capability_borsh_hex;
        let mut bytes = vec![0u8; hex.len() / 2];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).unwrap();
        bytes
    };
    let capability: PalwVerifierCapabilityV1 = borsh::from_slice(&cap_bytes).unwrap();
    capability.validate().unwrap();
    assert_eq!(capability.capability_nonce, 1);
    let proof = record.ready_bindings[0].proof.to_proof().unwrap();
    let binding_id = parse_hash64(&h.binding_id_hex).unwrap();
    assert!(verify_ready_binding_v1(&capability.ready_binding_root, &binding_id, &proof), "the emitted proof must verify");

    // The signing message the record publishes is exactly the one the capability derives,
    // and the SIGNATURE actually verifies under the published key and the routing context —
    // a wrong-context or wrong-bytes regression is caught here, not by a Stage-1 verifier.
    let message = verifier_capability_message_v1(b"misaka-palw-drill/v1", &capability);
    assert_eq!(record.signing_message, faster_hex::hex_string(message.as_bytes().as_slice()));
    let signer = ValidatorKey::from_seed(load_validator_seed(key).unwrap());
    assert_eq!(record.verifier_id, hex64(&signer.validator_id), "the record's verifier_id is the signer's");
    assert_eq!(record.verifier_public_key, faster_hex::hex_string(signer.public_key()), "the raw key ships with the record");
    assert!(
        signer.verify_with_context(message.as_bytes().as_slice(), &capability.signature, PALW_ROUTING_MLDSA87_CAPABILITY_CONTEXT),
        "the capability signature must verify under the published key and the routing context"
    );

    // A second issuance supersedes the first: nonce forward, never equal.
    let second: CapabilityRecordV1 =
        serde_json::from_str(&run_ok(&["capability", "--config", config, "--key", key, "--now-daa", "1010"])).unwrap();
    assert_eq!(second.capability_nonce, 2);
    let second_cap: PalwVerifierCapabilityV1 = {
        let mut bytes = vec![0u8; second.capability_borsh_hex.len() / 2];
        faster_hex::hex_decode(second.capability_borsh_hex.as_bytes(), &mut bytes).unwrap();
        borsh::from_slice(&bytes).unwrap()
    };
    assert!(second_cap.supersedes(&capability), "the re-issued capability must supersede the first");

    // No DAA source is a refusal, not a guess; two DAA sources are ambiguity, same refusal.
    let err = run_err(&["capability", "--config", config, "--key", key]);
    assert!(err.contains("does not invent clocks"), "stderr: {err}");
    let err = run_err(&["capability", "--config", config, "--key", key, "--now-daa", "5", "--rpc", "127.0.0.1:1"]);
    assert!(err.contains("ambiguous DAA source"), "stderr: {err}");

    let _ = std::fs::remove_dir_all(&h.dir);
}

#[test]
fn a_failing_selftest_quarantines_without_burning_a_nonce_and_names_the_stage() {
    let h = harness("quarantine");
    let config = h.policy_path.to_str().unwrap();
    let key = h.key_path.to_str().unwrap();

    let scan: serde_json::Value = serde_json::from_str(&run_ok(&["scan", "--config", config])).unwrap();
    assert_eq!(scan["eligible"].as_array().unwrap().len(), 1);

    // Swap in a worker whose selftest dies non-zero — the quarantine signal. Qualification
    // records the refusal with its STAGE and reason (exit 0: the failure is data, not a
    // crash), and no fabricated zero-bench rides along.
    h.swap_worker(1);
    run_ok(&["qualify", "--config", config]);
    let quals = std::fs::read_to_string(h.state_dir.join("qualifications.jsonl")).unwrap();
    assert!(quals.contains("\"selftest_passed\":false"), "the quarantine must be recorded: {quals}");
    assert!(quals.contains("\"bench\":null"), "no measurement means NO bench, not zeros: {quals}");
    assert!(quals.contains("selftest refused"), "the failure names its stage: {quals}");

    // The capability refuses to emit over an empty ready set — BEFORE reserving a nonce, so
    // the refusal burns nothing and recovery starts at nonce 1.
    let err = run_err(&["capability", "--config", config, "--key", key, "--now-daa", "1000"]);
    assert!(err.contains("ready set is empty"), "stderr: {err}");
    assert!(!h.state_dir.join("capability.nonce").exists(), "an unemittable offer must not touch the nonce counter");

    h.swap_worker(0);
    run_ok(&["qualify", "--config", config, "--requalify"]);
    let record: CapabilityRecordV1 =
        serde_json::from_str(&run_ok(&["capability", "--config", config, "--key", key, "--now-daa", "1000"])).unwrap();
    assert_eq!(record.capability_nonce, 1, "no nonce was burned by the refused emission");

    let _ = std::fs::remove_dir_all(&h.dir);
}
