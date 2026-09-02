//! `palw-derive` — ADR-0078's tool, for the executor and for the consumer.
//!
//! ```text
//! palw-derive list
//! palw-derive derive --transformer <name|kind> --answer <file> --out <dir> [--claim <hex> --output-root <hex> --network-domain <hex> --executor-pubkey <hex>]
//! palw-derive verify --object <derived-unsigned.borsh|derived-object.borsh> --answer <file> [--artifact <file>]
//!                    [--output-token-ids <json array file> --job-context-hash <hex> --family <qwen25-a16|qwen36>]
//! palw-derive manifest --transformer <name|id> | --all
//! palw-derive drill [--corpus <dir>] [--report <file.json>] [--check <file.json>]
//! palw-derive inspect --object <file>
//! ```
//!
//! `derive` runs the derivation offline (the same code the gateway runs) and writes the DSL, the
//! artifact and the unsigned object. `verify` is Decision 5 / X6: from the answer and the object,
//! recompute `dsl_hash` and `artifact_hash`; with the ids, the job's context hash and the family,
//! recompute the claim's `output_root` too. `manifest` is SA-5's: the document behind a
//! `transformer_id`, with the exact preimage, so a consumer can recompute the id themselves — a
//! derivation whose manifest this tree does not publish is refused rather than made. `drill` is
//! X3's instrument: every registered transformer over every corpus file, compared with that
//! kind's `golden.json`, plus SA-2's generated bound-exhausting corpus; the hashes go to a report
//! — run it on two architectures and `--check` one report against the other; a transformer whose
//! bytes differ is not a transformer under this ADR. It exits 3 on a cross-architecture
//! divergence, 4 on a moved golden, 5 on a declared ceiling that did not refuse, and 6 on a
//! registered transformer the corpus never exercised.

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
    if !s.len().is_multiple_of(2) || faster_hex::hex_decode(s.as_bytes(), &mut out).is_err() {
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
        // ADR-0078 SA-2: the ceilings are part of what this build ships and of what names it, so
        // `list` shows them rather than making a reader fetch the manifest to learn what will be
        // refused.
        let limits = m.named_input_limits();
        println!(
            "      bounds: dsl {} B  artifact {} B  work {} {}  named inputs {} ({} B)",
            m.max_dsl_bytes,
            m.max_artifact_bytes,
            m.max_steps,
            m.step_unit(),
            limits.max_inputs,
            limits.max_bytes
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
    // **`signed: true` was a claim this tool cannot make.** It was `signature.is_some()` — a
    // BORSH field being present — printed under a name every reader takes to mean "the executor's
    // ML-DSA-87 signature verifies". It does not: nothing here checks it, so a `.derived-object`
    // whose signature is a byte of noise verified `consistent` with `signed: true` (reproduced by
    // flipping one byte of the rail's own output). Decision 4's signature IS verified, but by the
    // acceptance layer under `PALW_DERIVED_V1_MLDSA87_CONTEXT` — which is why a derivation read
    // back from a chain is signed by definition, and one handed over as a FILE is not checked at
    // all. So the field says what it knows, and names where the check lives. Renamed rather than
    // qualified: a reader who greps `signed` must not find a field that answers a different
    // question than the one they asked.
    verdict.insert("signature_bytes".into(), signature.as_ref().map(|s| s.len()).into());
    verdict.insert(
        "signature_verified".into(),
        "not checked here: this tool re-runs the DERIVATION (Decision 5 / X6) and does not hold a signature verifier. \
         Decision 4's signature is verified by the chain's acceptance layer under PALW_DERIVED_V1_MLDSA87_CONTEXT, so a \
         derivation read back from a chain is signed by definition — `misaka palw derived-verify <claim-id>` is the check \
         that covers it. A `.derived-object.borsh` handed to you out of band carries no proof of its own signer."
            .into(),
    );
    let mut all_ok = true;
    // **"I cannot check this" is not "this is a forgery."** An object naming a grammar or a
    // transformer THIS build does not publish is SA-5's case, and it is the ordinary consequence
    // of a rebuild: `transformer_id` covers the crate's source tree, so every edit under
    // `misaka-palw-derive/src/` moves all eight ids and orphans every derivation already filed
    // under the old ones. Reporting that as "a demonstrable false object" accuses an honest
    // executor of the one thing Decision 5 exists to make provable, on the strength of the
    // reader's own version. `misaka palw derived-verify` already separates UNVERIFIABLE from
    // MISMATCH; this said MISMATCH for both.
    let mut unverifiable: Option<String> = None;
    match verify(&object, &answer_bytes) {
        Ok(v) => {
            all_ok &= v.all_match();
            verdict.insert("dsl_hash_matches".into(), v.dsl_hash_matches.into());
            verdict.insert("artifact_hash_matches".into(), v.artifact_hash_matches.into());
            verdict.insert("artifact_bytes_matches".into(), v.artifact_bytes_matches.into());
            // X8: the chain checks `kind != 0` and interprets nothing else, so a disagreement
            // between an object's kind and its transformer's manifest is the consumer's to catch.
            verdict.insert("kind_matches".into(), v.kind_matches.into());
            verdict.insert("manifest_kind".into(), v.manifest_kind.into());
            verdict.insert("recomputed_dsl_hash".into(), hex(v.recomputed_dsl_hash).into());
            verdict.insert("recomputed_artifact_hash".into(), hex(v.recomputed_artifact_hash).into());
        }
        Err(e @ (misaka_palw_derive::DeriveError::UnknownGrammar(_) | misaka_palw_derive::DeriveError::UnknownTransformer(_))) => {
            all_ok = false;
            unverifiable = Some(format!(
                "{e} — this build does not publish that manifest (ADR-0078 SA-5), so nobody running it can check this derivation \
                 either way. `palw-derive manifest --all` prints the ids this build has; a derivation is checkable only against \
                 the build whose source tree its transformer_id names."
            ));
            verdict.insert("derivation_rerun".into(), unverifiable.clone().unwrap().into());
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
        match (all_ok, &unverifiable) {
            (true, _) => "consistent",
            // Exit 2 either way — a reader who cannot check an object must not treat it as
            // checked — but the WORD is the difference between "this executor lied" and "I am
            // the wrong build to ask".
            (false, Some(_)) => "UNVERIFIABLE — this build does not publish that manifest (ADR-0078 SA-5)",
            (false, None) => "MISMATCH — a demonstrable false object (ADR-0078 Decision 5)",
        }
        .into(),
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

/// The published manifest of one transformer (ADR-0078 SA-5): the document a consumer needs
/// before they can check a `transformer_id` at all.
fn cmd_manifest(mut args: VecDeque<String>) {
    let mut spec = None;
    let mut all = false;
    while let Some(arg) = args.pop_front() {
        match arg.as_str() {
            "--transformer" => spec = Some(flag(&mut args, "--transformer")),
            "--all" => all = true,
            other => die(format!("unknown argument {other:?}")),
        }
    }
    if all {
        let docs: Vec<serde_json::Value> = registry::transformer_names()
            .iter()
            .map(|(n, _, _)| registry::published_manifest_document(&registry::transformer_by_name(n).expect("registered").manifest()))
            .collect();
        println!("{}", serde_json::to_string_pretty(&docs).unwrap());
        return;
    }
    let spec = spec.unwrap_or_else(|| die("--transformer <name|id> (or --all) is required".into()));
    let Some(m) = registry::published_manifest(&spec) else {
        // SA-5, from the tool's side: an id this build does not publish is not a manifest a
        // consumer could fetch, and the tool says so instead of printing an empty document.
        die(format!(
            "no transformer manifest is published in this tree for {spec:?} (ADR-0078 SA-5); `palw-derive list` prints the ones \
             that are"
        ));
    };
    println!("{}", serde_json::to_string_pretty(&registry::published_manifest_document(&m)).unwrap());
}

/// Every corpus answer of one kind directory, sorted, with `golden.json` left out — it is the
/// pin, not a sample.
fn corpus_answers(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "golden.json"))
        .collect();
    files.sort();
    files
}

/// X3's instrument: every transformer over every corpus file, against the goldens, plus SA-2's
/// bound-exhausting corpus.
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
    // The goldens, per kind directory, loaded once.
    let mut goldens: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut golden_mismatched: Vec<String> = Vec::new();
    let mut golden_unpinned: Vec<String> = Vec::new();
    let mut golden_checked = 0usize;
    // Registered transformers the corpus never exercised — see the note where this is filled.
    let mut uncovered: Vec<String> = Vec::new();

    for (name, k, grammar) in registry::transformer_names() {
        // **A transformer is drilled over the corpus of its GRAMMAR when its kind has no
        // directory of its own.** Two transformers can share one grammar and differ only in the
        // kind they file under — `code/evm/v1` and `contract/evm/v1` are exactly that pair — and
        // a corpus laid out by kind name then has no `contract/` directory at all. The lookup
        // used to stop there: `corpus_answers` on a missing directory is an empty list, so
        // `contract/evm/v1` produced no rows, no refusals and no unpinned keys, and the drill's
        // own counters had nothing to say about the transformer it had silently skipped. The
        // goldens for those rows were already in `corpus/code/golden.json`
        // (`01-return-42.json#contract/evm/v1` and its two siblings) and were dead pins.
        let kind_name = kind::name(k).unwrap_or("unassigned").to_string();
        let mut dir_name = kind_name.clone();
        let mut kind_dir = corpus.join(&dir_name);
        if !kind_dir.is_dir() {
            let by_grammar = grammar.split('/').next().unwrap_or(grammar).to_string();
            if corpus.join(&by_grammar).is_dir() {
                kind_dir = corpus.join(&by_grammar);
                dir_name = by_grammar;
            }
        }
        let golden = goldens.entry(dir_name.clone()).or_insert_with(|| match std::fs::read(kind_dir.join("golden.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .unwrap_or_else(|e| die(format!("{}: not a golden document: {e}", kind_dir.join("golden.json").display()))),
            Err(_) => serde_json::Value::Null,
        });
        let kind_name = dir_name;
        let before = rows.len() + refused.len();
        for file in corpus_answers(&kind_dir) {
            let answer = std::fs::read(&file).unwrap_or_else(|e| die(format!("{}: {e}", file.display())));
            let leaf = file.file_name().unwrap().to_string_lossy().into_owned();
            let key = format!("{kind_name}/{leaf}#{name}");
            // The golden of a kind that pins its refusals (a corpus sample named `-refused-` is
            // a sample the kind must REFUSE, and WHICH wall it hit is the thing under test) is
            // compared here too — otherwise the drill would silently skip exactly the samples a
            // bound-exhausting corpus exists for.
            let pinned_for = |g: &serde_json::Value| g.get(format!("{leaf}#{name}")).or_else(|| g.get(&leaf)).cloned();
            let d = match derive_named(name, &binding, &answer) {
                Ok(d) => d,
                Err(e) => {
                    match pinned_for(golden) {
                        None => golden_unpinned.push(key.clone()),
                        Some(want) => {
                            golden_checked += 1;
                            match want.get("refused").and_then(|v| v.as_str()) {
                                Some(pinned) if pinned == e.to_string() => {}
                                Some(pinned) => {
                                    golden_mismatched.push(format!("{key}: refusal pinned {pinned:?} / here {:?}", e.to_string()))
                                }
                                None => golden_mismatched.push(format!("{key}: the golden pins a derivation and it was refused: {e}")),
                            }
                        }
                    }
                    refused.insert(key, e.to_string());
                    continue;
                }
            };
            rows.insert(
                key.clone(),
                serde_json::json!({
                    "grammar": grammar,
                    "dsl_hash": hex(d.dsl_hash),
                    "artifact_hash": hex(d.artifact_hash),
                    "artifact_bytes": d.artifact.bytes.len(),
                }),
            );
            // The goldens are the SAME comparison on one host that `--check` is across two, and
            // they are the half that catches a second architecture without shipping a file
            // between the hosts: run the drill there and the pins either hold or they do not.
            // Two key shapes exist in the tree because one kind directory can serve two
            // transformers (`code` serves `code/evm/v1` and `contract/evm/v1`), so a bare file
            // name would not say which; both are accepted rather than one being rewritten.
            match pinned_for(golden) {
                None => golden_unpinned.push(key),
                Some(want) => {
                    golden_checked += 1;
                    let eq = |field: &str, got: String| {
                        want.get(field).and_then(|v| v.as_str()).map(|s| s.eq_ignore_ascii_case(&got)).unwrap_or(false)
                    };
                    if !eq("dsl_hash", hex(d.dsl_hash)) {
                        golden_mismatched.push(format!("{key}: dsl_hash pinned {} / here {}", want["dsl_hash"], hex(d.dsl_hash)));
                    }
                    if !eq("artifact_hash", hex(d.artifact_hash)) {
                        golden_mismatched.push(format!(
                            "{key}: artifact_hash pinned {} / here {}",
                            want["artifact_hash"],
                            hex(d.artifact_hash)
                        ));
                    }
                    if want.get("artifact_bytes").and_then(|v| v.as_u64()) != Some(d.artifact.bytes.len() as u64) {
                        golden_mismatched.push(format!(
                            "{key}: artifact_bytes pinned {} / here {}",
                            want["artifact_bytes"],
                            d.artifact.bytes.len()
                        ));
                    }
                }
            }
        }
        // **A registered transformer the corpus never asked to run is not drilled.** X3 compares
        // what two architectures produced; a transformer that produced nothing on both agrees
        // with itself and is reported as held. That is how `contract/evm/v1` sat outside the
        // drill while every count in the report looked healthy, so the drill now says so by name.
        if rows.len() + refused.len() == before {
            uncovered.push(format!("{name}: no corpus answer under {}", kind_dir.display()));
        }
    }

    // ---- SA-2's bound-exhausting corpus, generated rather than stored -----------------------
    //
    // "X3's drill includes a bound-exhausting corpus." It is generated from each transformer's
    // OWN declared `max_dsl_bytes` rather than checked in as files, for three reasons: a stored
    // four-mebibyte file per kind would dwarf the repository; a stored file would go stale the
    // moment a kind tightened its bound and would then prove nothing; and a generated one is the
    // same bytes on both architectures, so the row belongs in the `--check` comparison like any
    // other. Each input is one byte over the bound and otherwise well-formed enough that only
    // the bound can be refusing it.
    let mut bounds_rows: BTreeMap<String, String> = BTreeMap::new();
    let mut unbounded: Vec<String> = Vec::new();
    for (name, _, _) in registry::transformer_names() {
        let m = registry::transformer_by_name(name).expect("registered").manifest();
        if let Err(e) = misaka_palw_derive::check_declared_bounds(&m) {
            unbounded.push(format!("{name}: {e}"));
            continue;
        }
        let mut over = vec![b' '; m.max_dsl_bytes as usize + 1];
        over[0] = b'{';
        *over.last_mut().unwrap() = b'}';
        match derive_named(name, &binding, &over) {
            // The refusal must name the ceiling: that is what says the wall ran on the byte COUNT
            // and not on something a parser found inside the bytes.
            Err(e) if e.is_refusal() && e.to_string().contains(&m.max_dsl_bytes.to_string()) => {
                bounds_rows.insert(name.to_string(), e.to_string());
            }
            Err(other) => unbounded
                .push(format!("{name}: an answer of {} bytes was refused, but not by the declared ceiling: {other}", over.len())),
            Ok(_) => unbounded.push(format!("{name}: an answer of {} bytes was DERIVED; max_dsl_bytes is not enforced", over.len())),
        }
    }

    let doc = serde_json::json!({
        "schema": "misaka.palw.derive-drill.v2",
        "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "source_tree_sha256": misaka_palw_derive::SOURCE_TREE_SHA256_HEX,
        "transformers": registry::transformer_names().iter().map(|(n, _, _)| {
            let m = registry::transformer_by_name(n).unwrap().manifest();
            serde_json::json!({
                "name": n,
                "transformer_id": hex(misaka_palw_derive::ids::transformer_id(&m)),
                "max_dsl_bytes": m.max_dsl_bytes,
                "max_artifact_bytes": m.max_artifact_bytes,
                "max_steps": m.max_steps,
                "step_unit": m.step_unit(),
            })
        }).collect::<Vec<_>>(),
        "rows": rows,
        "refused": refused,
        "bounds": bounds_rows,
        "golden": {
            "checked": golden_checked,
            "mismatched": golden_mismatched,
            "unpinned": golden_unpinned,
        },
        "uncovered": uncovered,
    });
    if let Some(path) = &report {
        std::fs::write(path, serde_json::to_vec_pretty(&doc).unwrap()).unwrap_or_else(|e| die(format!("{}: {e}", path.display())));
    }

    let mut exit = 0;
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
        // A corpus answer refused on one architecture and derived on the other is a divergence
        // the `rows` comparison alone would read as "absent there" — and WHICH wall refused it is
        // part of the answer, so the messages are compared and not only the fact of a refusal.
        let their_refusals = other.get("refused").and_then(|r| r.as_object()).cloned().unwrap_or_default();
        let refusals_compared = refused.len().max(their_refusals.len());
        for (key, why) in &refused {
            match their_refusals.get(key) {
                Some(t) if t.as_str() == Some(why.as_str()) => {}
                Some(t) => diverged.push(format!("{key}: refused here as {why:?}, there as {t}")),
                None => diverged.push(format!("{key}: refused here ({why}) and not there")),
            }
        }
        for key in their_refusals.keys() {
            if !refused.contains_key(key) {
                diverged.push(format!("{key}: refused there and not here"));
            }
        }
        if other["bounds"] != doc["bounds"] {
            diverged.push("the bound-exhausting corpus is refused differently on the two hosts".to_string());
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
                "refusals_compared": refusals_compared,
                "diverged": diverged,
                "verdict": if diverged.is_empty() { "X3 holds: byte-identical artifacts on both reports" } else { "X3 FAILS: a transformer whose bytes differ is not a transformer under ADR-0078" },
            })
        );
        if !diverged.is_empty() {
            exit = 3;
        }
    } else {
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.derive-drill.v2",
                "arch": std::env::consts::ARCH,
                "rows": rows.len(),
                "refused": refused.len(),
                "golden_checked": golden_checked,
                "golden_mismatched": golden_mismatched,
                "golden_unpinned": golden_unpinned,
                "bounds_enforced": bounds_rows.len(),
                "bounds_not_enforced": unbounded,
                "uncovered": uncovered,
                "report": report.map(|p| p.display().to_string()),
                "verdict": if golden_mismatched.is_empty() && unbounded.is_empty() && uncovered.is_empty() {
                    "every registered transformer ran, the corpus reproduces its goldens on this architecture, and every declared bound refused an over-bound answer"
                } else {
                    "FAILS: see golden_mismatched / bounds_not_enforced / uncovered"
                },
            })
        );
    }
    if !golden_mismatched.is_empty() && exit == 0 {
        exit = 4;
    }
    if !unbounded.is_empty() && exit == 0 {
        exit = 5;
    }
    // 6, and not folded into one of the three above: "a transformer nobody drilled" is a
    // different fact from "a golden moved" or "a ceiling did not refuse", and a caller that
    // cannot tell them apart cannot act on either.
    if !uncovered.is_empty() && exit == 0 {
        exit = 6;
    }
    if exit != 0 {
        std::process::exit(exit);
    }
}

fn main() {
    let mut args: VecDeque<String> = std::env::args().skip(1).collect();
    match args.pop_front().as_deref() {
        Some("list") => cmd_list(),
        Some("derive") => cmd_derive(args),
        Some("verify") => cmd_verify(args),
        Some("inspect") => cmd_inspect(args),
        Some("manifest") => cmd_manifest(args),
        Some("drill") => cmd_drill(args),
        _ => die("usage: palw-derive list | derive | verify | manifest | inspect | drill (see the module doc)".into()),
    }
}
