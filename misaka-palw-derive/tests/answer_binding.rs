//! **A derivation belongs to a claim only when its DSL is the RENDERING of that claim's ids.**
//!
//! Before the code these tests cover, `palw-derive verify` recomputed `dsl_hash` and
//! `artifact_hash` from the answer bytes the caller passed and `output_root` from the token ids
//! the caller passed, ANDed the two, and printed `verdict: consistent`. No function in either path
//! takes the other's input — `rendered_output_hash_v1` looks like the join and is not, because on
//! every shipped family it is a keyed hash of the IDS and never reaches a byte of text — so the
//! conjunction was two true sentences about two unrelated inputs, printed under the word every
//! reader takes to mean *this artifact came from that inference*.
//!
//! `the_old_conjunction_is_true_of_a_forgery` is that hole, executed: one honest claim, one
//! artifact derived from a completely different answer, and both legs of X6 green. The tests
//! around it are the closure — the rendering is the join, the tokenizer is the claim's to pin, and
//! the verdict WORD tells a reader which of the two sentences they are being handed.
//!
//! The binary-path tests run the shipped `palw-derive` rather than the library, because the defect
//! was in what the TOOL printed: a library that can be asked the right question is no use if the
//! command everyone runs asks the other one.

use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN, PalwDerivedArtifactV1};
use kaspa_consensus_core::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, PalwJobContextV2};
use kaspa_hashes::Hash64;
use misaka_palw_base0::e2e_drill::PalwRcFamilyV1;
use misaka_palw_base0::tokenizer::QwenTokenizer;
use misaka_palw_derive::{
    ClaimBinding, Derivation, check_tokenizer_pin_v1, derive_named, opened_tokenizer_id_v1, recompute_output_root, render_answer_v1,
    verify, verify_bound, verify_output_root,
};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The tool under test, as cargo built it for this run.
const BIN: &str = env!("CARGO_BIN_EXE_palw-derive");

/// The transformer these tests derive with. `music/smf/v1` because the corpus's smallest answer is
/// a few hundred bytes of JSON and the writer is a note loop: the defect is in the binding, not in
/// any kind, and a cheap kind keeps this file's cost in the binding.
const TRANSFORMER: &str = "music/smf/v1";
const FAMILY: PalwRcFamilyV1 = PalwRcFamilyV1::Qwen25A16;

fn h(b: u8) -> Hash64 {
    Hash64::from_bytes([b; 64])
}

fn corpus(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("music").join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A job context that pins one tokenizer. Every other field is a fixed nonsense value: the binding
/// is a statement about `tokenizer_id` and `context_hash()`, and the rest of the context is here
/// only because those two are functions of all of it.
fn job_context(tokenizer_id: Hash64) -> PalwJobContextV2 {
    PalwJobContextV2 {
        version: PALW_TRACE_COMMITMENT_VERSION_V2,
        network_id: b"palw-derive-answer-binding".to_vec(),
        job_id: h(0x11),
        job_nullifier: h(0x12),
        assignment_id: h(0x13),
        execution_seed: [0x14; 32],
        model_profile_id: h(0x15),
        runtime_manifest_hash: h(0x16),
        runtime_class_id: h(0x17),
        shape_profile_id: h(0x18),
        trace_scheme_id: h(0x19),
        cu_ruleset_id: h(0x1a),
        tokenizer_id,
        prompt_token_ids_hash: h(0x1b),
        declared_prefill_tokens: 8,
        exact_decode_tokens: 64,
        max_context_tokens: 512,
    }
}

/// The claim a derivation is filed against: its `output_root` is the one THESE ids imply under
/// THIS context, exactly as the chain holds it.
fn claim_for(ctx: &PalwJobContextV2, ids: &[u32]) -> ClaimBinding {
    ClaimBinding {
        network_domain: h(0x01),
        claim_id: h(0x02),
        output_root: recompute_output_root(FAMILY, &ctx.context_hash(), ids),
        executor_pubkey: vec![0x11; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
    }
}

/// **A tokenizer whose ids ARE chunks of one text**, as a `tokenizer.json` a real
/// `QwenTokenizer::from_json` reads.
///
/// Added tokens are matched whole and `token_bytes` returns their content bytes verbatim, so the
/// concatenation over the ids is the text and nothing here depends on the byte-level alphabet.
/// That keeps the library tests free of a 7 MB fixture while exercising the same
/// `render_answer_v1` the gateway runs; the binary test below uses a real tokenizer.
fn tokenizer_over(text: &[u8], chunk: usize) -> (String, Vec<u32>) {
    let text = std::str::from_utf8(text).expect("the corpus is UTF-8");
    let mut added = Vec::new();
    let mut ids = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let mut end = (start + chunk).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        let id = (added.len() + 1) as u32;
        added.push(serde_json::json!({ "id": id, "content": &text[start..end], "special": false }));
        ids.push(id);
        start = end;
    }
    let json = serde_json::json!({ "model": { "vocab": { "a": 0 }, "merges": [] }, "added_tokens": added });
    (serde_json::to_string(&json).expect("serializable"), ids)
}

/// One honest run: the tokenizer file, the ids, the context that pins it, and a derivation of the
/// answer those ids render to.
struct Honest {
    tokenizer_bytes: Vec<u8>,
    tokenizer: QwenTokenizer,
    ids: Vec<u32>,
    ctx: PalwJobContextV2,
    derivation: Derivation,
    answer: Vec<u8>,
}

fn honest() -> Honest {
    let answer = corpus("01-single-note.json");
    let (json, ids) = tokenizer_over(&answer, 24);
    let tokenizer_bytes = json.into_bytes();
    let tokenizer = QwenTokenizer::from_json(&tokenizer_bytes).expect("a readable tokenizer.json");
    assert_eq!(render_answer_v1(&tokenizer, &ids), answer, "the fixture tokenizer must render its own ids back to the answer");
    let ctx = job_context(opened_tokenizer_id_v1(&tokenizer_bytes));
    let derivation = derive_named(TRANSFORMER, &claim_for(&ctx, &ids), &answer).expect("the corpus answer derives");
    Honest { tokenizer_bytes, tokenizer, ids, ctx, derivation, answer }
}

// ------------------------------------------------------------------------------------------
// The library: two verdict shapes, and the forgery the unbound one cannot see
// ------------------------------------------------------------------------------------------

/// **The unbound shape**: `verify` alone answers "is this artifact what the named transformer
/// makes of THESE BYTES", which is true of any answer the caller happens to be holding.
#[test]
fn the_unbound_verification_covers_only_the_answer_it_was_handed() {
    let h = honest();
    let v = verify(&h.derivation.object, &h.answer).expect("re-runnable");
    assert!(v.all_match(), "the honest derivation re-runs: {:?}", v.mismatches());

    // The same call, over an answer from a different corpus file, is a mismatch — which is all
    // `verify` can ever say, because nothing here knows which answer belonged to the claim.
    let other = verify(&h.derivation.object, &corpus("03-overlapping-melody.json")).expect("re-runnable");
    assert!(other.mismatches().contains(&"dsl_hash"), "{:?}", other.mismatches());
    assert!(other.mismatches().contains(&"artifact_hash"), "{:?}", other.mismatches());
}

/// **The bound shape**: the same three recomputations, over the bytes the CLAIM's ids render to,
/// plus `output_root` from those same ids and that same context.
#[test]
fn the_bound_verification_renders_the_answer_from_the_claims_own_ids() {
    let h = honest();
    let b = verify_bound(&h.derivation.object, FAMILY, &h.ctx, &h.tokenizer, opened_tokenizer_id_v1(&h.tokenizer_bytes), &h.ids, None)
        .expect("re-runnable");
    assert!(b.all_match(), "an honest derivation binds to its claim: {:?}", b.mismatches());
    assert!(b.output_root_matches);
    assert_eq!(b.rendered_answer_bytes, h.answer.len());
    assert_eq!(b.tokenizer_id, h.ctx.tokenizer_id);
    assert_eq!(b.supplied_answer_is_the_rendering, None, "no answer was supplied beside the ids");
}

/// **The defect, executed.** Both legs of X6 are true and the artifact is from another answer
/// entirely. This is the state `verdict: consistent` was printed in.
#[test]
fn the_old_conjunction_is_true_of_a_forgery() {
    let h = honest();

    // The executor derives from a DIFFERENT answer and files the object against the honest
    // claim: same claim id, same output_root — the two fields the chain checks.
    let other_answer = corpus("03-overlapping-melody.json");
    let forged = derive_named(TRANSFORMER, &claim_for(&h.ctx, &h.ids), &other_answer).expect("the other answer derives too");

    // Leg one, over the answer the executor hands the consumer beside the artifact: green.
    let v = verify(&forged.object, &other_answer).expect("re-runnable");
    assert!(v.all_match(), "the derivation of the other answer re-runs perfectly — it is a real derivation");
    // Leg two, over the ids the claim really committed: green.
    assert!(
        verify_output_root(&forged.object, FAMILY, &h.ctx.context_hash(), &h.ids),
        "the object carries the claim's own output_root, so this leg cannot fail"
    );
    // ANDed, that was `consistent`. The two legs share no input.

    // The binding, which does share one: the rendering of the claim's ids is not this DSL.
    let b = verify_bound(
        &forged.object,
        FAMILY,
        &h.ctx,
        &h.tokenizer,
        opened_tokenizer_id_v1(&h.tokenizer_bytes),
        &h.ids,
        Some(&other_answer),
    )
    .expect("re-runnable");
    assert!(!b.all_match(), "a forgery must not bind");
    assert!(b.output_root_matches, "output_root is still the claim's — the forgery is not there");
    // Named field by field, including the answer the caller was handed beside the ids.
    for field in ["dsl_hash", "artifact_hash", "supplied_answer_is_the_rendering"] {
        assert!(b.mismatches().contains(&field), "{field} must be named: {:?}", b.mismatches());
    }
    assert_eq!(b.supplied_answer_is_the_rendering, Some(false));
}

/// The tokenizer is the claim's to pin, and a file that is not it is refused BY NAME — with the
/// second lineage named too, because a GGUF-embedded tokenizer's id is not computable from any
/// `tokenizer.json` and that is not the same thing as a lie.
#[test]
fn a_tokenizer_that_is_not_the_pinned_one_is_refused_by_name() {
    let h = honest();
    let other = opened_tokenizer_id_v1(b"{\"model\":{\"vocab\":{\"a\":0},\"merges\":[]}}");
    let err = check_tokenizer_pin_v1(&h.ctx, other).expect_err("a different tokenizer must be refused");
    let msg = err.to_string();
    assert!(msg.contains(&h.ctx.tokenizer_id.to_string()), "the refusal names what the claim pins: {msg}");
    assert!(msg.contains(&other.to_string()), "the refusal names what was opened: {msg}");
    assert!(msg.contains("tokenizer_id_v2_for_gguf"), "the refusal names the lineage it cannot check: {msg}");

    // And `verify_bound` refuses on it rather than rendering under it anyway.
    let bad = QwenTokenizer::from_json(b"{\"model\":{\"vocab\":{\"a\":0},\"merges\":[]}}").expect("readable");
    assert!(verify_bound(&h.derivation.object, FAMILY, &h.ctx, &bad, other, &h.ids, None).is_err());
}

// ------------------------------------------------------------------------------------------
// The binary: what the tool actually prints
// ------------------------------------------------------------------------------------------

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("palw-derive-answer-binding-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temp dir");
    dir.join(name)
}

fn write(name: &str, bytes: &[u8]) -> PathBuf {
    let path = scratch(name);
    std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    path
}

fn write_object(name: &str, object: &PalwDerivedArtifactV1) -> PathBuf {
    write(name, &borsh::to_vec(object).expect("borsh"))
}

/// Run `palw-derive verify` and return `(exit code, the parsed verdict, stderr)`.
fn run_verify(args: &[&str]) -> (i32, serde_json::Value, String) {
    let out = Command::new(BIN).arg("verify").args(args).output().expect("the tool runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let verdict = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout is not the verdict JSON ({e}):\nstdout: {stdout}\nstderr: {stderr}"));
    (out.status.code().unwrap_or(-1), verdict, stderr)
}

fn word(verdict: &serde_json::Value) -> String {
    verdict["verdict"].as_str().expect("a verdict word").to_string()
}

/// **Part 1: the tool must never print an unqualified `consistent` for a binding it did not
/// check.** Nothing here was wrong about the derivation — it is a real one, and every field it
/// reports is true. What was wrong was the word.
#[test]
fn the_binary_qualifies_its_verdict_when_nothing_bound_the_artifact_to_the_claim() {
    let h = honest();
    let object = write_object("unbound.derived-unsigned.borsh", &h.derivation.object);
    let answer = write("unbound-answer.json", &h.answer);
    let (code, verdict, stderr) = run_verify(&["--object", object.to_str().unwrap(), "--answer", answer.to_str().unwrap()]);

    assert_eq!(code, 0, "the derivation itself is consistent: {verdict}");
    assert_eq!(verdict["binding_checked"], serde_json::Value::Bool(false));
    assert!(
        word(&verdict).starts_with("consistent-given-the-supplied-answer"),
        "the WORD must not be readable as `this artifact came from that inference`: {}",
        word(&verdict)
    );
    assert!(verdict["binding_not_checked_because"].as_str().expect("a reason").contains("--output-token-ids"));
    assert!(verdict["dsl_hash_matches"].as_bool().expect("reported"));
    assert!(stderr.contains("binding_checked: false"), "the text line carries it too: {stderr}");
    assert!(verdict["exit_status"].as_str().expect("documented").contains("binding_checked"));
}

/// **Part 2: with the claim's ids, its context, its tokenizer and its family, the word is plain
/// `consistent` — and it has earned it**, because every hash above was recomputed over the bytes
/// those ids render to.
#[test]
fn the_binary_binds_the_artifact_to_the_claim_and_then_says_consistent() {
    let h = honest();
    let object = write_object("bound.derived-unsigned.borsh", &h.derivation.object);
    let ids = write("bound-ids.json", serde_json::to_string(&h.ids).expect("ids").as_bytes());
    let ctx = write("bound-job-context.borsh", &borsh::to_vec(&h.ctx).expect("borsh"));
    let tok = write("bound-tokenizer.json", &h.tokenizer_bytes);
    let (code, verdict, _) = run_verify(&[
        "--object",
        object.to_str().unwrap(),
        "--output-token-ids",
        ids.to_str().unwrap(),
        "--job-context",
        ctx.to_str().unwrap(),
        "--tokenizer",
        tok.to_str().unwrap(),
        "--family",
        "qwen25-a16",
    ]);
    assert_eq!(code, 0, "{verdict}");
    assert_eq!(verdict["binding_checked"], serde_json::Value::Bool(true));
    assert_eq!(word(&verdict), "consistent", "a checked binding gets the plain word");
    assert_eq!(verdict["output_root_matches"], serde_json::Value::Bool(true));
    assert_eq!(verdict["rendered_answer_bytes"].as_u64(), Some(h.answer.len() as u64));
    // No `--answer` was passed at all: the answer came from the claim's own ids.
    assert!(verdict.get("supplied_answer_is_the_rendering").is_none());
}

/// A derivation whose DSL is NOT the rendering of its ids is refused, and the refusal names the
/// fields. Same forgery as `the_old_conjunction_is_true_of_a_forgery`, through the tool.
#[test]
fn the_binary_refuses_a_derivation_whose_dsl_is_not_the_rendering_of_its_ids() {
    let h = honest();
    let other_answer = corpus("03-overlapping-melody.json");
    let forged = derive_named(TRANSFORMER, &claim_for(&h.ctx, &h.ids), &other_answer).expect("derives");

    let object = write_object("forged.derived-unsigned.borsh", &forged.object);
    let answer = write("forged-answer.json", &other_answer);
    let ids = write("forged-ids.json", serde_json::to_string(&h.ids).expect("ids").as_bytes());
    let ctx = write("forged-job-context.borsh", &borsh::to_vec(&h.ctx).expect("borsh"));
    let tok = write("forged-tokenizer.json", &h.tokenizer_bytes);

    // Unbound, exactly as the tool used to be asked: green, with the qualified word.
    let (code, verdict, _) = run_verify(&["--object", object.to_str().unwrap(), "--answer", answer.to_str().unwrap()]);
    assert_eq!(code, 0, "the forgery passes the unbound path — that is the defect");
    assert!(word(&verdict).starts_with("consistent-given-the-supplied-answer"));

    // Bound: refused by name.
    let (code, verdict, _) = run_verify(&[
        "--object",
        object.to_str().unwrap(),
        "--answer",
        answer.to_str().unwrap(),
        "--output-token-ids",
        ids.to_str().unwrap(),
        "--job-context",
        ctx.to_str().unwrap(),
        "--tokenizer",
        tok.to_str().unwrap(),
        "--family",
        "qwen25-a16",
    ]);
    assert_eq!(code, 2, "{verdict}");
    assert_eq!(verdict["binding_checked"], serde_json::Value::Bool(true));
    assert!(word(&verdict).starts_with("MISMATCH"), "{}", word(&verdict));
    assert_eq!(verdict["output_root_matches"], serde_json::Value::Bool(true), "the claim is real; the derivation is not its");
    assert_eq!(verdict["supplied_answer_is_the_rendering"], serde_json::Value::Bool(false));
    let named: Vec<&str> = verdict["mismatches"].as_array().expect("named").iter().map(|v| v.as_str().unwrap()).collect();
    assert!(named.contains(&"dsl_hash"), "{named:?}");
}

// ------------------------------------------------------------------------------------------
// The same, over a REAL dense class artifact and a REAL tokenizer
// ------------------------------------------------------------------------------------------

/// The dense artifacts this drill can use, each with the tokenizer file it pins.
///
/// **The pairing was measured, not assumed.** `instruct-bound.palwart` carries the tokenizer
/// commitment of `qwen25-tokenizer.json` at byte 1,777,209,032
/// (`fa9a43521e324f8482d88a2f4147ae23…`); `qwen25-1.5b-a16.palwart` carries 64 zero bytes there —
/// `TokenizerBindingV1::Undeclared`, the state the shipped dense artifact is in, which confirms
/// nothing and refuses nothing. A pair that is wrong does not pass quietly: the tool refuses by
/// name, and this test fails with that refusal in its message.
const DENSE_FIXTURES: &[(&str, Option<&str>)] = &[
    (
        "/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/71440f68-0f3b-4144-8b20-73c6aae7fb86/scratchpad/instruct-bound.palwart",
        Some("/Users/wata/Downloads/qwen25-tokenizer.json"),
    ),
    ("/Users/wata/Downloads/qwen25-1.5b-a16.palwart", None),
];

/// The tokenizer a fixture with no declared commitment may be checked with.
const ANY_REAL_TOKENIZER: &[&str] = &[
    "/Users/wata/Downloads/qwen25-tokenizer.json",
    "/private/tmp/claude-501/-Users-wata-Downloads-MISAKA-testnet/71440f68-0f3b-4144-8b20-73c6aae7fb86/scratchpad/qwen35-2b-tokenizer.json",
];

fn dense_pair() -> Option<(PathBuf, PathBuf)> {
    for (artifact, pinned) in DENSE_FIXTURES {
        let artifact = PathBuf::from(artifact);
        if !artifact.exists() {
            continue;
        }
        let tokenizer = match pinned {
            Some(p) => PathBuf::from(p),
            None => ANY_REAL_TOKENIZER.iter().map(PathBuf::from).find(|p| p.exists())?,
        };
        if tokenizer.exists() {
            return Some((artifact, tokenizer));
        }
    }
    None
}

/// **`--artifact <dense .palwart>`: the class's weights confirm the tokenizer, and a real
/// tokenizer renders real ids into the DSL the derivation was made from.**
///
/// Skipped BY NAME when the fixtures are absent — never a pass by absence: the message says which
/// paths were looked for, so a green run on a machine without them cannot be mistaken for the
/// check having happened.
#[test]
fn the_binary_binds_through_a_real_dense_artifact_and_a_real_tokenizer() {
    let Some((artifact, tokenizer_path)) = dense_pair() else {
        eprintln!(
            "SKIPPED the_binary_binds_through_a_real_dense_artifact_and_a_real_tokenizer: no dense fixture present. \
             Looked for {} (with {}) and {}. This check did NOT run.",
            DENSE_FIXTURES[0].0,
            DENSE_FIXTURES[0].1.unwrap_or("any tokenizer"),
            DENSE_FIXTURES[1].0
        );
        return;
    };

    let tokenizer_bytes = std::fs::read(&tokenizer_path).expect("the tokenizer file");
    let tokenizer = QwenTokenizer::from_json(&tokenizer_bytes).expect("a readable tokenizer.json");
    let answer = corpus("01-single-note.json");
    let text = std::str::from_utf8(&answer).expect("UTF-8");
    let ids = tokenizer.encode_without_specials(text).expect("the corpus answer tokenizes");
    assert_eq!(
        render_answer_v1(&tokenizer, &ids),
        answer,
        "a real tokenizer's round trip must be the identity, or this test is measuring the tokenizer"
    );

    let ctx = job_context(opened_tokenizer_id_v1(&tokenizer_bytes));
    let derivation = derive_named(TRANSFORMER, &claim_for(&ctx, &ids), &answer).expect("derives");

    let object = write_object("dense.derived-unsigned.borsh", &derivation.object);
    let ids_file = write("dense-ids.json", serde_json::to_string(&ids).expect("ids").as_bytes());
    let ctx_file = write("dense-job-context.borsh", &borsh::to_vec(&ctx).expect("borsh"));
    let (code, verdict, _) = run_verify(&[
        "--object",
        object.to_str().unwrap(),
        "--output-token-ids",
        ids_file.to_str().unwrap(),
        "--job-context",
        ctx_file.to_str().unwrap(),
        "--tokenizer",
        tokenizer_path.to_str().unwrap(),
        "--artifact",
        artifact.to_str().unwrap(),
        "--family",
        "qwen25-a16",
    ]);
    assert_eq!(code, 0, "{verdict}");
    assert_eq!(verdict["binding_checked"], serde_json::Value::Bool(true));
    assert_eq!(word(&verdict), "consistent");
    assert!(
        verdict["artifact_file_role"].as_str().expect("named").contains("class artifact"),
        "the verdict names which question the file answered: {verdict}"
    );
    assert!(verdict["class_artifact_digest"].is_string(), "the digest was recomputed on decode: {verdict}");
}

/// A tokenizer that is not the one the claim pins is a REFUSAL (exit 1), not a `MISMATCH`: the
/// caller is holding the wrong file, and filing that under "a demonstrable false object" would
/// accuse the executor of the reader's own mistake.
#[test]
fn a_tokenizer_the_claim_does_not_pin_is_refused_rather_than_called_a_forgery() {
    let h = honest();
    let object = write_object("wrongtok.derived-unsigned.borsh", &h.derivation.object);
    let ids = write("wrongtok-ids.json", serde_json::to_string(&h.ids).expect("ids").as_bytes());
    let ctx = write("wrongtok-job-context.borsh", &borsh::to_vec(&h.ctx).expect("borsh"));
    // A different, perfectly readable tokenizer.json — over the OTHER corpus answer.
    let (other_json, _) = tokenizer_over(&corpus("03-overlapping-melody.json"), 24);
    let tok = write("wrongtok-tokenizer.json", other_json.as_bytes());
    let out = Command::new(BIN)
        .args([
            "verify",
            "--object",
            object.to_str().unwrap(),
            "--output-token-ids",
            ids.to_str().unwrap(),
            "--job-context",
            ctx.to_str().unwrap(),
            "--tokenizer",
            tok.to_str().unwrap(),
            "--family",
            "qwen25-a16",
        ])
        .output()
        .expect("the tool runs");
    assert_eq!(out.status.code(), Some(1), "a wrong tokenizer is a refusal, not a verdict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains(&h.ctx.tokenizer_id.to_string()), "the refusal names what the claim pins: {stderr}");
    assert!(stderr.contains("tokenizer_id_v2_for_gguf"), "and the lineage it cannot check from a file: {stderr}");
    assert!(out.stdout.is_empty(), "a refusal prints no verdict at all");
}

/// A file that is not there is a refusal by name, never a check silently dropped.
#[test]
fn a_missing_artifact_file_is_refused_by_name() {
    let h = honest();
    let object = write_object("missing.derived-unsigned.borsh", &h.derivation.object);
    let answer = write("missing-answer.json", &h.answer);
    let out = Command::new(BIN)
        .args([
            "verify",
            "--object",
            object.to_str().unwrap(),
            "--answer",
            answer.to_str().unwrap(),
            "--artifact",
            "/nonexistent/palw-derive/no-such-artifact.glb",
        ])
        .output()
        .expect("the tool runs");
    assert_eq!(out.status.code(), Some(1), "a missing input is a refusal, not a verdict");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no-such-artifact.glb"), "the refusal names the file: {stderr}");
    assert!(out.stdout.is_empty(), "a refusal prints no verdict at all");
}
