//! `palw-record-gen` — DRILL-ONLY generator of binding/definition record files for the
//! Stage-0 re-executor run (ADR-0034 §6). Stage 0 has no registry store: the operator
//! assembles the records the agent consumes, from the SAME measured facts the fleet already
//! established — the worker's golden-registered `v2-manifest` (identities), the local gguf
//! (artifact size + sha256), and the fleet replay bench (`4 300 + 165·tok`, p99 90 716 ms).
//!
//! Honesty notes, so nobody mistakes this for registration:
//! * the shape profile inside the row is the DRILL MINI-SHAPE (the registry fixture's) — the
//!   real Qwen3.5 shape-profile transcription is a separate registration act, and nothing at
//!   Stage 0 reads the shape beyond its internal coherence;
//! * `publisher_signature` is a placeholder — publisher-signature verification is the
//!   registry's future act (the agent checks shape + joins, as documented);
//! * every derived field (ceiling, band) is re-derived here and the row must pass the full
//!   registry `validate()` before a single byte is written — fail-closed, like everything.

use kaspa_consensus_core::palw_routing::{PALW_REGISTERED_CLASS_TAGS, derived_model_band_v1, replay_work_ms_v1};
use kaspa_consensus_core::palw_schedule::{PalwReplayCostMeasurementV1, credited_ceiling_tokens_v1};
use misaka_palw_reexecutor::fixtures::{definition_with, test_binding_with_artifact};
use misaka_palw_reexecutor::{hex64, parse_hash64};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn die(msg: String) -> ! {
    eprintln!("[palw-record-gen] FATAL: {msg}");
    std::process::exit(1);
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let manifest_path = arg("--manifest-json")
        .unwrap_or_else(|| die("--manifest-json <file> is required (worker v2-manifest output, golden-registered)".into()));
    let gguf_path = arg("--gguf").unwrap_or_else(|| die("--gguf <file> is required".into()));
    let out_dir = PathBuf::from(arg("--out-dir").unwrap_or_else(|| die("--out-dir <dir> is required".into())));
    let fixed_ms: u64 = arg("--fixed-ms").as_deref().unwrap_or("4300").parse().unwrap_or_else(|e| die(format!("--fixed-ms: {e}")));
    let per_token_ms: u64 =
        arg("--per-token-ms").as_deref().unwrap_or("165").parse().unwrap_or_else(|e| die(format!("--per-token-ms: {e}")));
    let format_ceiling: u32 =
        arg("--format-ceiling").as_deref().unwrap_or("4095").parse().unwrap_or_else(|e| die(format!("--format-ceiling: {e}")));
    let p99_ms: u64 = arg("--p99-ms").as_deref().unwrap_or("90716").parse().unwrap_or_else(|e| die(format!("--p99-ms: {e}")));

    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap_or_else(|e| die(format!("read {manifest_path}: {e}"))))
            .unwrap_or_else(|e| die(format!("manifest json: {e}")));
    let field = |k: &str| -> String {
        manifest.get(k).and_then(|v| v.as_str()).unwrap_or_else(|| die(format!("manifest carries no {k}"))).to_owned()
    };
    let runtime_class_id = parse_hash64(&field("runtime_class_id")).unwrap_or_else(|e| die(e));
    let runtime_manifest_hash = parse_hash64(&field("runtime_manifest_hash_v2")).unwrap_or_else(|e| die(e));
    let model_profile_id = parse_hash64(&field("model_profile_id")).unwrap_or_else(|e| die(e));
    let tokenizer_id = parse_hash64(&field("tokenizer_id_v2")).unwrap_or_else(|e| die(e));
    if manifest.get("golden_registered").and_then(|v| v.as_bool()) != Some(true) {
        die("the manifest was produced WITHOUT the golden set registered — its identity hash is the unpopulated sentinel; \
             re-run `--mode v2-manifest` with MISAKA_PALW_GOLDEN exported"
            .into());
    }

    // The tag must come from the ledger AND derive the manifest's class id — a record for a
    // backend this build cannot name is exactly what the agent would refuse anyway.
    let class_tag = PALW_REGISTERED_CLASS_TAGS
        .iter()
        .find(|tag| kaspa_consensus_core::vlt::derive_runtime_class_id(tag) == runtime_class_id)
        .unwrap_or_else(|| die("the manifest's runtime_class_id matches no registered class tag".into()));

    // Artifact identity from the real file.
    let mut file = std::fs::File::open(&gguf_path).unwrap_or_else(|e| die(format!("open {gguf_path}: {e}")));
    let mut hasher = Sha256::new();
    let gguf_size = std::io::copy(&mut file, &mut hasher).unwrap_or_else(|e| die(format!("read {gguf_path}: {e}")));
    let gguf_sha256: [u8; 32] = hasher.finalize().into();

    // Start from the drill row shape, then replace every identity and measured fact with the
    // host's real ones, re-deriving what must be derived.
    let mut binding = test_binding_with_artifact(gguf_size);
    binding.label = format!("{class_tag} (stage-0 drill row)");
    binding.class_tag = (*class_tag).to_owned();
    binding.runtime_class_id = runtime_class_id;
    binding.runtime_manifest_hash = runtime_manifest_hash;
    binding.model_profile_id = model_profile_id;
    binding.tokenizer_id = tokenizer_id;
    let (family, family_version) = kaspa_consensus_core::palw_routing::routing_keys_for_class_tag_v1(class_tag)
        .unwrap_or_else(|| die("ledger tag does not parse (bug)".into()));
    binding.execution_family = family;
    binding.family_version = family_version;
    binding.replay_cost = PalwReplayCostMeasurementV1 {
        fixed_overhead_ms: fixed_ms,
        ms_per_decode_token: per_token_ms,
        format_ceiling_tokens: format_ceiling,
    };
    binding.credited_ceiling_tokens = credited_ceiling_tokens_v1(&binding.replay_cost, &binding.windows, 120_000);
    binding.p99_cold_replay_ms = p99_ms;
    let work_ms = replay_work_ms_v1(&binding.replay_cost, binding.credited_ceiling_tokens);
    binding.model_band =
        derived_model_band_v1(binding.model_artifact_bytes, binding.peak_memory_bytes, work_ms, binding.max_proof_material_bytes)
            .unwrap_or_else(|| die("the measured resources derive no band — not registrable".into()));

    let blockrate = kaspa_consensus_core::config::params::BlockrateParams::new_two_minute_bps();
    binding.validate(&blockrate, 120_000).unwrap_or_else(|e| die(format!("the assembled row does not validate: {e}")));

    let definition = {
        let mut d = definition_with(model_profile_id, gguf_size, gguf_sha256);
        d.tokenizer_id = tokenizer_id;
        d
    };
    definition.validate().unwrap_or_else(|e| die(format!("the assembled definition does not validate: {e}")));
    if !kaspa_consensus_core::palw_routing::binding_matches_definition_v1(&binding, &definition) {
        die("binding/definition join failed (bug)".into());
    }

    std::fs::create_dir_all(&out_dir).unwrap_or_else(|e| die(format!("mkdir {}: {e}", out_dir.display())));
    std::fs::write(out_dir.join("binding.bin"), borsh::to_vec(&binding).expect("borsh")).unwrap_or_else(|e| die(e.to_string()));
    std::fs::write(out_dir.join("definition.bin"), borsh::to_vec(&definition).expect("borsh")).unwrap_or_else(|e| die(e.to_string()));
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema": "misaka.palw-record-gen.v1",
            "binding_id": hex64(&binding.registration_id()),
            "class_tag": class_tag,
            "model_band": format!("{:?}", binding.model_band),
            "credited_ceiling_tokens": binding.credited_ceiling_tokens,
            "gguf_size": gguf_size,
            "gguf_sha256": faster_hex::hex_string(&gguf_sha256),
            "out_dir": out_dir.display().to_string(),
            "note": "drill records: mini shape profile + placeholder publisher signature; NOT a registration",
        }))
        .expect("serializable")
    );
}
