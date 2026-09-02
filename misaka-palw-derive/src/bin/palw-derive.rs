//! `palw-derive` — ADR-0078's tool, for the executor and for the consumer.
//!
//! ```text
//! palw-derive list
//! palw-derive derive --transformer <name|kind> --answer <file> --out <dir> [--claim <hex> --output-root <hex> --network-domain <hex> --executor-pubkey <hex>]
//! palw-derive verify --object <derived-unsigned.borsh|derived-object.borsh> --answer <file> [--artifact <file>]
//!                    [--output-token-ids <json array file> --job-context-hash <hex> --family <qwen25-a16|qwen36>]
//! palw-derive drill --corpus <dir> --report <file.json> [--check <file.json>]
//! palw-derive inspect --object <file>
//! ```
//!
//! `derive` runs the derivation offline (the same code the gateway runs) and writes the DSL, the
//! artifact and the unsigned object. `verify` is Decision 5 / X6: from the answer and the object,
//! recompute `dsl_hash` and `artifact_hash`; with the ids, the job's context hash and the family,
//! recompute the claim's `output_root` too. `drill` is X3's instrument: every registered
//! transformer over every corpus file, the artifact hashes written to a report — run it on two
//! architectures and `--check` one report against the other; a transformer whose bytes differ
//! is not a transformer under this ADR.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_derived_v1::{PalwDerivedArtifactV1, derived_id_v1, kind};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_hashes::Hash64;
use misaka_palw_derive::{ClaimBinding, derive_named, registry, verify, verify_artifact_bytes};

fn die(msg: String) -> ! {
    eprintln!("[palw-derive] fatal: {msg}");
    std::process::exit(1);
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

fn hex64(s: &str, what: &str) -> Hash64 {
    let mut out = [0u8; 64];
    if s.len() != 128 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not 128 hex chars"));
    }
    Hash64::from_bytes(out)
}

fn hex_bytes(s: &str, what: &str) -> Vec<u8> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let mut out = vec![0u8; s.len() / 2];
    if s.len() % 2 != 0 || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
        die(format!("{what} is not hex"));
    }
    out
}

fn flag(args: &mut VecDeque<String>, name: &str) -> String {
    args.pop_front().unwrap_or_else(|| die(format!("{name} needs a value")))
}

fn read_object(path: &Path) -> (PalwDerivedArtifactV1, Option<Vec<u8>>) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
    if let Ok(PalwConsensusObjectV2::DerivedArtifactV1 { object, signature }) = borsh::from_slice::<PalwConsensusObjectV2>(&bytes) {
        return (*object, Some(signature));
    }
    match borsh::from_slice::<PalwDerivedArtifactV1>(&bytes) {
        Ok(object) => (object, None),
        Err(e) => die(format!("{} is neither a DerivedArtifactV1 consensus object nor an unsigned derivation: {e}", path.display())),
    }
}

/// The family's rendered-output hash rule (ADR-0078 X6), by family name.
fn rendered_output_hash(family: &str, ids: &[u32]) -> Hash64 {
    match family {
        "qwen25-a16" => misaka_palw_base0::qwen25_a16_backend::rendered_output_hash_v1(ids),
        "qwen36" => misaka_palw_base0::qwen36_backend::rendered_output_hash_v1(ids),
        other => die(format!("unknown family {other:?}: this build knows qwen25-a16 and qwen36")),
    }
}

fn cmd_list() {
    println!("grammars:");
    for g in registry::grammar_names() {
        println!("  {g}  id {}", &hex(kaspa_consensus_core::palw_derived_v1::grammar_id_v1(g))[..16]);
    }
    println!("transformers:");
    for (name, k, grammar) in registry::transformer_names() {
        let t = registry::transformer_by_name(name).expect("registered");
        let m = t.manifest();
        println!(
            "  {name}  kind {k} ({})  grammar {grammar}  discipline {}  writer {}  id {}",
            kind::name(k).unwrap_or("?"),
            m.discipline.as_str(),
            m.writer,
            &hex(misaka_palw_derive::ids::transformer_id(&m))[..16]
        );
    }
    println!("build source tree sha256: {}", misaka_palw_derive::SOURCE_TREE_SHA256_HEX);
}

fn cmd_derive(mut args: VecDeque<String>) {
    let mut transformer = None;
    let mut answer = None;
    let mut out = None;
    let mut claim = Hash64::default();
    let mut output_root = Hash64::default();
    let mut network_domain = Hash64::default();
    let mut executor_pubkey = vec![0u8; kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN];
    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "--transformer" => transformer = Some(flag(&mut args, "--transformer")),
            "--answer" => answer = Some(PathBuf::from(flag(&mut args, "--answer"))),
            "--out" => out = Some(PathBuf::from(flag(&mut args, "--out"))),
            "--claim" => claim = hex64(&flag(&mut args, "--claim"), "--claim"),
            "--output-root" => output_root = hex64(&flag(&mut args, "--output-root"), "--output-root"),
            "--network-domain" => network_domain = hex64(&flag(&mut args, "--network-domain"), "--network-domain"),
            "--executor-pubkey" => executor_pubkey = hex_bytes(&flag(&mut args, "--executor-pubkey"), "--executor-pubkey"),
            other => die(format!("unknown argument {other:?}")),
        }
    }
    let spec = transformer.unwrap_or_else(|| die("--transformer <name|kind> is required".into()));
    let name = match registry::transformer_by_name(&spec) {
        Some(t) => t.manifest().name,
        None => match kind::id(&spec).and_then(|k| registry::transformer_names().into_iter().find(|(_, kk, _)| *kk == k)) {
            Some((n, _, _)) => n,
            None => die(format!("no transformer or kind named {spec:?} (see `palw-derive list`)")),
        },
    };
    let answer_path = answer.unwrap_or_else(|| die("--answer <file> is required".into()));
    let answer_bytes = std::fs::read(&answer_path).unwrap_or_else(|e| die(format!("{}: {e}", answer_path.display())));
    let out_dir = out.unwrap_or_else(|| die("--out <dir> is required".into()));
    std::fs::create_dir_all(&out_dir).unwrap_or_else(|e| die(format!("{}: {e}", out_dir.display())));
    let binding = ClaimBinding { network_domain, claim_id: claim, output_root, executor_pubkey };
    let d = derive_named(name, &binding, &answer_bytes).unwrap_or_else(|e| die(format!("derivation refused: {e}")));
    let stem = out_dir.join(format!("derived-{}", &hex(d.derived_id())[..16]));
    let dsl_path = PathBuf::from(format!("{}.dsl", stem.display()));
    let artifact_path = PathBuf::from(format!("{}.artifact.{}", stem.display(), d.artifact.extension));
    let object_path = PathBuf::from(format!("{}.derived-unsigned.borsh", stem.display()));
    std::fs::write(&dsl_path, &d.canonical_dsl).unwrap_or_else(|e| die(format!("{}: {e}", dsl_path.display())));
    std::fs::write(&artifact_path, &d.artifact.bytes).unwrap_or_else(|e| die(format!("{}: {e}", artifact_path.display())));
    std::fs::write(&object_path, borsh::to_vec(&d.object).unwrap()).unwrap_or_else(|e| die(format!("{}: {e}", object_path.display())));
    println!(
        "{}",
        serde_json::json!({
            "schema": "misaka.palw.derive-offline.v1",
            "transformer": name,
            "kind": d.kind,
            "kind_name": kind::name(d.kind),
            "derived_id": hex(d.derived_id()),
            "grammar_id": hex(d.grammar_id),
            "transformer_id": hex(d.transformer_id),
            "dsl_hash": hex(d.dsl_hash),
            "artifact_hash": hex(d.artifact_hash),
            "artifact_bytes": d.artifact.bytes.len(),
            "files": { "dsl": dsl_path.display().to_string(), "artifact": artifact_path.display().to_string(), "object": object_path.display().to_string() },
            "note": "the object is UNSIGNED and its claim binding is whatever was passed; sign with misaka-palw-fp-rail --derive-artifact",
        })
    );
}

fn cmd_verify(mut args: VecDeque<String>) {
    let mut object_path = None;
    let mut answer = None;
    let mut artifact = None;
    let mut ids_path = None;
    let mut job_context_hash = None;
    let mut family = None;
    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "--object" => object_path = Some(PathBuf::from(flag(&mut args, "--object"))),
            "--answer" => answer = Some(PathBuf::from(flag(&mut args, "--answer"))),
            "--artifact" => artifact = Some(PathBuf::from(flag(&mut args, "--artifact"))),
            "--output-token-ids" => ids_path = Some(PathBuf::from(flag(&mut args, "--output-token-ids"))),
            "--job-context-hash" => job_context_hash = Some(hex64(&flag(&mut args, "--job-context-hash"), "--job-context-hash")),
            "--family" => family = Some(flag(&mut args, "--family")),
            other => die(format!("unknown argument {other:?}")),
        }
    }
    let (object, signature) = read_object(&object_path.unwrap_or_else(|| die("--object <file> is required".into())));
    let answer_path = answer.unwrap_or_else(|| die("--answer <file> is required".into()));
    let answer_bytes = std::fs::read(&answer_path).unwrap_or_else(|e| die(format!("{}: {e}", answer_path.display())));
    let mut verdict = serde_json::Map::new();
    verdict.insert("schema".into(), "misaka.palw.derive-verify.v1".into());
    verdict.insert("derived_id".into(), hex(derived_id_v1(&object)).into());
    verdict.insert("claim_id".into(), hex(object.claim_id).into());
    verdict.insert("kind".into(), object.kind.into());
    verdict.insert("kind_name".into(), kind::name(object.kind).into());
    verdict.insert("signed".into(), signature.is_some().into());
    let mut all_ok = true;
    match verify(&object, &answer_bytes) {
        Ok(v) => {
            all_ok &= v.all_match();
            verdict.insert("dsl_hash_matches".into(), v.dsl_hash_matches.into());
            verdict.insert("artifact_hash_matches".into(), v.artifact_hash_matches.into());
            verdict.insert("artifact_bytes_matches".into(), v.artifact_bytes_matches.into());
            verdict.insert("recomputed_dsl_hash".into(), hex(v.recomputed_dsl_hash).into());
            verdict.insert("recomputed_artifact_hash".into(), hex(v.recomputed_artifact_hash).into());
        }
        Err(e) => {
            all_ok = false;
            verdict.insert(
                "derivation_rerun".into(),
                format!("could not re-run: {e} — the object names a computation the answer does not admit").into(),
            );
        }
    }
    if let Some(path) = artifact {
        let bytes = std::fs::read(&path).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
        let ok = verify_artifact_bytes(&object, &bytes);
        all_ok &= ok;
        verdict.insert("artifact_file_matches".into(), ok.into());
    }
    if let (Some(ids_path), Some(ctx), Some(family)) = (ids_path, job_context_hash, family) {
        let text = std::fs::read_to_string(&ids_path).unwrap_or_else(|e| die(format!("{}: {e}", ids_path.display())));
        let ids: Vec<u32> =
            serde_json::from_str(&text).unwrap_or_else(|e| die(format!("{} is not a JSON array of ids: {e}", ids_path.display())));
        let rendered = rendered_output_hash(&family, &ids);
        let recomputed = kaspa_consensus_core::palw_v2::output_commitment_v2(&ctx, &ids, &rendered);
        let ok = recomputed == object.output_root;
        all_ok &= ok;
        verdict.insert("output_root_matches".into(), ok.into());
        verdict.insert("recomputed_output_root".into(), hex(recomputed).into());
    } else {
        verdict.insert(
            "output_root".into(),
            "not checked: pass --output-token-ids, --job-context-hash and --family to recompute the claim's output_root (ADR-0078 X6)"
                .into(),
        );
    }
    verdict.insert(
        "verdict".into(),
        if all_ok { "consistent" } else { "MISMATCH — a demonstrable false object (ADR-0078 Decision 5)" }.into(),
    );
    println!("{}", serde_json::Value::Object(verdict));
    if !all_ok {
        std::process::exit(2);
    }
}

fn cmd_inspect(mut args: VecDeque<String>) {
    let mut object_path = None;
    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "--object" => object_path = Some(PathBuf::from(flag(&mut args, "--object"))),
            other => die(format!("unknown argument {other:?}")),
        }
    }
    let (o, signature) = read_object(&object_path.unwrap_or_else(|| die("--object <file> is required".into())));
    println!(
        "{}",
        serde_json::json!({
            "derived_id": hex(derived_id_v1(&o)),
            "version": o.version,
            "network_domain": hex(o.network_domain),
            "claim_id": hex(o.claim_id),
            "output_root": hex(o.output_root),
            "grammar_id": hex(o.grammar_id),
            "grammar": registry::grammar_by_id(&o.grammar_id).map(|g| g.name()),
            "transformer_id": hex(o.transformer_id),
            "transformer": registry::transformer_by_id(&o.transformer_id).map(|t| t.manifest().name),
            "kind": o.kind,
            "kind_name": kind::name(o.kind),
            "dsl_hash": hex(o.dsl_hash),
            "artifact_hash": hex(o.artifact_hash),
            "artifact_bytes": o.artifact_bytes,
            "executor_pubkey": faster_hex::hex_string(&o.executor_pubkey),
            "signature_bytes": signature.map(|s| s.len()),
        })
    );
}

/// X3's instrument: every transformer over every corpus file.
fn cmd_drill(mut args: VecDeque<String>) {
    let mut corpus = None;
    let mut report = None;
    let mut check = None;
    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "--corpus" => corpus = Some(PathBuf::from(flag(&mut args, "--corpus"))),
            "--report" => report = Some(PathBuf::from(flag(&mut args, "--report"))),
            "--check" => check = Some(PathBuf::from(flag(&mut args, "--check"))),
            other => die(format!("unknown argument {other:?}")),
        }
    }
    let corpus = corpus.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus"));
    let binding = ClaimBinding {
        network_domain: Hash64::default(),
        claim_id: Hash64::default(),
        output_root: Hash64::default(),
        executor_pubkey: vec![0u8; kaspa_consensus_core::palw_derived_v1::PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
    };
    // report: "<kind-dir>/<file>#<transformer>" -> { dsl_hash, artifact_hash, artifact_bytes }
    let mut rows: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut refused: BTreeMap<String, String> = BTreeMap::new();
    for (name, k, grammar) in registry::transformer_names() {
        let kind_dir = corpus.join(kind::name(k).unwrap_or("unassigned"));
        let Ok(entries) = std::fs::read_dir(&kind_dir) else { continue };
        let mut files: Vec<PathBuf> =
            entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
        files.sort();
        for file in files {
            if file.file_name().is_some_and(|f| f == "golden.json") {
                continue;
            }
            let answer = std::fs::read(&file).unwrap_or_else(|e| die(format!("{}: {e}", file.display())));
            let key = format!("{}/{}#{name}", kind::name(k).unwrap_or("?"), file.file_name().unwrap().to_string_lossy());
            match derive_named(name, &binding, &answer) {
                Ok(d) => {
                    rows.insert(
                        key,
                        serde_json::json!({
                            "grammar": grammar,
                            "dsl_hash": hex(d.dsl_hash),
                            "artifact_hash": hex(d.artifact_hash),
                            "artifact_bytes": d.artifact.bytes.len(),
                        }),
                    );
                }
                Err(e) => {
                    refused.insert(key, e.to_string());
                }
            }
        }
    }
    let doc = serde_json::json!({
        "schema": "misaka.palw.derive-drill.v1",
        "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "source_tree_sha256": misaka_palw_derive::SOURCE_TREE_SHA256_HEX,
        "transformers": registry::transformer_names().iter().map(|(n, _, _)| serde_json::json!({
            "name": n,
            "transformer_id": hex(misaka_palw_derive::ids::transformer_id(&registry::transformer_by_name(n).unwrap().manifest())),
        })).collect::<Vec<_>>(),
        "rows": rows,
        "refused": refused,
    });
    if let Some(path) = &report {
        std::fs::write(path, serde_json::to_vec_pretty(&doc).unwrap()).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
    }
    if let Some(other_path) = check {
        let other: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&other_path).unwrap_or_else(|e| die(format!("{}: {e}", other_path.display()))))
                .unwrap_or_else(|e| die(format!("{} is not a drill report: {e}", other_path.display())));
        let theirs = other.get("rows").and_then(|r| r.as_object()).cloned().unwrap_or_default();
        let mut diverged = Vec::new();
        for (key, mine) in &doc["rows"].as_object().cloned().unwrap_or_default() {
            match theirs.get(key) {
                Some(t) if t == mine => {}
                Some(t) => diverged.push(format!("{key}: here {} / there {}", mine["artifact_hash"], t["artifact_hash"])),
                None => diverged.push(format!("{key}: absent in {}", other_path.display())),
            }
        }
        for key in theirs.keys() {
            if !rows.contains_key(key) {
                diverged.push(format!("{key}: absent here"));
            }
        }
        if other["source_tree_sha256"] != doc["source_tree_sha256"] {
            diverged.push(format!("source tree differs: here {} / there {}", doc["source_tree_sha256"], other["source_tree_sha256"]));
        }
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.derive-drill-check.v1",
                "here": format!("{}-{}", doc["arch"].as_str().unwrap(), doc["os"].as_str().unwrap()),
                "there": format!("{}-{}", other["arch"].as_str().unwrap_or("?"), other["os"].as_str().unwrap_or("?")),
                "rows_compared": rows.len(),
                "diverged": diverged,
                "verdict": if diverged.is_empty() { "X3 holds: byte-identical artifacts on both reports" } else { "X3 FAILS: a transformer whose bytes differ is not a transformer under ADR-0078" },
            })
        );
        if !diverged.is_empty() {
            std::process::exit(3);
        }
    } else {
        println!(
            "{}",
            serde_json::json!({ "schema": "misaka.palw.derive-drill.v1", "rows": rows.len(), "refused": refused.len(), "report": report.map(|p| p.display().to_string()) })
        );
    }
}

fn main() {
    let mut args: VecDeque<String> = std::env::args().skip(1).collect();
    match args.pop_front().as_deref() {
        Some("list") => cmd_list(),
        Some("derive") => cmd_derive(args),
        Some("verify") => cmd_verify(args),
        Some("inspect") => cmd_inspect(args),
        Some("drill") => cmd_drill(args),
        _ => die("usage: palw-derive list | derive | verify | inspect | drill (see the module doc)".into()),
    }
}
