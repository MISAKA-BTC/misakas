//! **ADR-0110 — `misaka palw verify-context` and `misaka palw verify-receipt`.**
//!
//! `verify-context` runs a context vector through the network's own pipeline
//! (`misaka_palw_base0::context_vector`) and writes one canonical document: `consensus` and
//! `verdicts` must agree byte for byte on every honest machine and are what its `document_id`
//! covers; `host` is what this machine measured, reported and never compared. `--sign-with`
//! writes a receipt beside it — ADR-0108 Decision 4's rule for this kind: evidence that a named key
//! ran this build over this vector and reported this `document_id`, and never a vote. No code path
//! in a consensus, node or SDK crate reads a receipt (ADR-0110 invariant 6).
//!
//! No node is contacted: a vector is a pure function of its name, seed and geometry, and the
//! ruleset it is judged under is the one this build mints (`devnet-held`).

use std::path::Path;
use std::time::Instant;

use kaspa_consensus_core::Hash64;
use misaka_palw_base0::context_vector as cv;

use crate::keys::KeySource;
use crate::{CliError, exit};

/// The receipt's own name.
pub const PALW_CONTEXT_RECEIPT_V1: &str = "misaka-palw/context-receipt/v1";
/// **The receipt's signature context** (ADR-0110 Decision 4). A constant of the verifier and
/// deliberately not in the bundle's `signature_contexts_root`: the chain verifies nothing signed
/// under it, so a receipt cannot be replayed as anything the chain reads, and nothing signed for
/// the chain verifies as a receipt.
pub const PALW_CONTEXT_RECEIPT_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/context-receipt/v1";

pub struct VerifyContextArgs<'a> {
    pub vector: Option<&'a str>,
    pub vector_file: Option<&'a Path>,
    pub list: bool,
    pub stages: Option<&'a str>,
    pub out: Option<&'a Path>,
    pub receipt_out: Option<&'a Path>,
    pub key: Option<KeySource>,
}

fn io(what: &Path, e: std::io::Error) -> CliError {
    CliError::new(exit::GENERIC, format!("{}: {e}", what.display()))
}

/// The tree's one general canonicaliser and the vector module's own must agree on every document
/// this command writes — a disagreement is a bug in one of them, and a document hashed under the
/// wrong one would carry an id nobody else reproduces.
fn canonical(doc: &serde_json::Value) -> Result<Vec<u8>, CliError> {
    let ours = cv::palw_canonical_json_v1(doc);
    let tree = misaka_palw_derive::canon_json::canonicalize_json(&ours)
        .map_err(|e| CliError::new(exit::GENERIC, format!("the document does not canonicalise: {e:?}")))?;
    if tree != ours {
        return Err(CliError::new(
            exit::GENERIC,
            "the two canonical forms of one document disagree — a bug in one of them".to_string(),
        ));
    }
    Ok(ours)
}

/// What this host measured — reported, never compared, never inside the `document_id`.
fn host_facts(total_ms: u64) -> serde_json::Value {
    let threads = std::thread::available_parallelism().map(|n| n.get() as u64).unwrap_or(1);
    serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "threads": threads,
        "build": env!("CARGO_PKG_VERSION"),
        "peak_rss_bytes": peak_rss_bytes(),
        "total_ms": total_ms,
    })
}

/// The process's peak resident set, from the kernel's own accounting.
fn peak_rss_bytes() -> u64 {
    // SAFETY: `getrusage` writes into the struct it is handed and reads nothing else.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return 0;
    }
    let max = usage.ru_maxrss.max(0) as u64;
    // macOS reports bytes, Linux kilobytes.
    if cfg!(target_os = "macos") { max } else { max.saturating_mul(1024) }
}

fn cost_class(n_ctx: u32) -> &'static str {
    match n_ctx {
        0..=512 => "the default test suite",
        513..=32_768 => "the release-mode vector job",
        _ => "an external run",
    }
}

pub fn verify_context(args: VerifyContextArgs<'_>) -> Result<(), CliError> {
    if args.list {
        println!("{:<22} {:>9}  {:<30}  vector_id", "name", "n_ctx", "runs in");
        for v in cv::palw_context_vectors_v1() {
            println!("{:<22} {:>9}  {:<30}  {}", v.name, v.geometry.n_ctx, cost_class(v.geometry.n_ctx), v.vector_id());
        }
        return Ok(());
    }
    let vector = match (args.vector, args.vector_file) {
        (Some(name), None) => cv::palw_context_vector_v1(name).ok_or_else(|| {
            CliError::new(exit::GENERIC, format!("no shipped vector is named {name} (see `misaka palw verify-context --list`)"))
        })?,
        (None, Some(path)) => {
            let bytes = std::fs::read(path).map_err(|e| io(path, e))?;
            let value: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|e| CliError::new(exit::GENERIC, format!("{}: not JSON: {e}", path.display())))?;
            cv::PalwContextVectorV1::from_json_v1(&value)
                .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", path.display())))?
        }
        _ => return Err(CliError::new(exit::GENERIC, "name exactly one of --vector <name> or --vector-file <file>".to_string())),
    };
    let stages: Vec<cv::PalwContextStageV1> = match args.stages {
        None => cv::PalwContextStageV1::ALL.to_vec(),
        Some(list) => list
            .split(',')
            .map(|s| {
                cv::PalwContextStageV1::parse(s.trim()).ok_or_else(|| {
                    CliError::new(exit::GENERIC, format!("no stage is named `{s}` (produce, commit, seat, court, availability, fit)"))
                })
            })
            .collect::<Result<_, _>>()?,
    };
    let key = args.key.as_ref().map(|k| k.load_key()).transpose()?;
    let ruleset = cv::PalwContextRulesetV1::devnet_held_v1().map_err(|e| CliError::new(exit::GENERIC, e))?;
    eprintln!(
        "verifying {} ({} positions, vector {}) under {} — {}",
        vector.name,
        vector.geometry.n_ctx,
        vector.vector_id(),
        ruleset.name,
        if vector.shipped { "shipped" } else { "not a shipped vector" }
    );

    let t = Instant::now();
    let findings = cv::palw_verify_context_vector_v1(&vector, &ruleset, &stages);
    let host = host_facts(t.elapsed().as_millis() as u64);
    let document = findings.document_json_v1(host.clone());
    let bytes = canonical(&document)?;
    let document_id = findings.document_id();

    for stage in cv::PalwContextStageV1::ALL {
        let Some(verdict) = findings.verdicts.get(&stage) else { continue };
        let ms = findings.stage_ms.get(&stage).map(|ms| format!("{ms} ms")).unwrap_or_default();
        let said = match verdict {
            cv::PalwContextVerdictV1::Pass => "pass".to_string(),
            cv::PalwContextVerdictV1::Fail(why) => format!("FAIL — {why}"),
            cv::PalwContextVerdictV1::Skipped(why) => format!("skipped — {why}"),
        };
        println!("{:<13} {:>10}  {said}", stage.name(), ms);
    }
    println!("document_id   {document_id}");
    if let Some(out) = args.out {
        std::fs::write(out, &bytes).map_err(|e| io(out, e))?;
        println!("document      {} ({} bytes, canonical)", out.display(), bytes.len());
    }

    if let Some(key) = key {
        let receipt_path = match (args.receipt_out, args.out) {
            (Some(p), _) => p.to_path_buf(),
            (None, Some(out)) => out.with_extension("receipt.json"),
            (None, None) => {
                return Err(CliError::new(
                    exit::GENERIC,
                    "--sign-with needs --receipt-out or --out to name where the receipt goes".to_string(),
                ));
            }
        };
        let mut receipt = serde_json::json!({
            "receipt": PALW_CONTEXT_RECEIPT_V1,
            "document_id": document_id.to_string(),
            "vector_id": vector.vector_id().to_string(),
            "ruleset": { "name": ruleset.name, "consensus_params_id": findings.consensus_params_id },
            "host": host,
            "signer": faster_hex::hex_string(key.public_key()),
        });
        let message = canonical(&receipt)?;
        let signature = key.sign_with_context(&message, PALW_CONTEXT_RECEIPT_MLDSA87_CONTEXT);
        receipt["signature"] = serde_json::json!(faster_hex::hex_string(&signature));
        let receipt_bytes = canonical(&receipt)?;
        std::fs::write(&receipt_path, &receipt_bytes).map_err(|e| io(&receipt_path, e))?;
        println!("receipt       {} (signed by {})", receipt_path.display(), key.validator_id);
    }

    if findings.all_passed() {
        Ok(())
    } else {
        Err(CliError::new(exit::GENERIC, format!("a stage failed; the document ({document_id}) says which and why")))
    }
}

/// **A document's id, recomputed from its own bytes** — the agreed sections, without `host` and
/// without the id itself.
pub fn palw_document_id_of_v1(document: &serde_json::Value) -> Result<Hash64, CliError> {
    let mut agreed = document.clone();
    let map = agreed.as_object_mut().ok_or_else(|| CliError::new(exit::GENERIC, "the document is not an object".to_string()))?;
    if map.get("document").and_then(|d| d.as_str()) != Some(cv::PALW_CONTEXT_VERIFICATION_V1) {
        return Err(CliError::new(exit::GENERIC, format!("not a {} document", cv::PALW_CONTEXT_VERIFICATION_V1)));
    }
    map.remove("host");
    map.remove("document_id");
    let bytes = canonical(&agreed)?;
    let mut state = blake2b_simd::Params::new().hash_length(64).key(cv::PALW_CONTEXT_VERIFICATION_V1.as_bytes()).to_state();
    state.update(&bytes);
    Ok(Hash64::from_slice(state.finalize().as_bytes()))
}

/// What a receipt says, checked.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckedReceiptV1 {
    pub document_id: Hash64,
    pub signer: String,
    /// `Some(true)` when a document was given and its recomputed id is the receipt's.
    pub document_matches: Option<bool>,
}

/// **Check a receipt**: its own name, its signature under the receipt context, and — when a
/// document is given — that the document's recomputed id is the one the receipt signed.
pub fn palw_check_receipt_v1(receipt: &serde_json::Value, document: Option<&serde_json::Value>) -> Result<CheckedReceiptV1, CliError> {
    let refuse = |why: String| CliError::new(exit::GENERIC, why);
    if receipt.get("receipt").and_then(|r| r.as_str()) != Some(PALW_CONTEXT_RECEIPT_V1) {
        return Err(refuse(format!("not a {PALW_CONTEXT_RECEIPT_V1} receipt")));
    }
    let text = |k: &str| {
        receipt.get(k).and_then(|v| v.as_str()).map(str::to_string).ok_or_else(|| refuse(format!("the receipt has no `{k}`")))
    };
    let signer = text("signer")?;
    let signature = text("signature")?;
    let document_id: Hash64 = text("document_id")?.parse().map_err(|_| refuse("`document_id` is not 128 hex".to_string()))?;
    let decode = |h: &str, what: &str| -> Result<Vec<u8>, CliError> {
        let mut out = vec![0u8; h.len() / 2];
        faster_hex::hex_decode(h.as_bytes(), &mut out).map_err(|_| refuse(format!("`{what}` is not hex")))?;
        Ok(out)
    };
    let mut unsigned = receipt.clone();
    unsigned.as_object_mut().expect("checked an object above").remove("signature");
    let message = canonical(&unsigned)?;
    let verified = kaspa_txscript::verify_mldsa87_with_context(
        &decode(&signer, "signer")?,
        &message,
        &decode(&signature, "signature")?,
        PALW_CONTEXT_RECEIPT_MLDSA87_CONTEXT,
    )
    .map_err(|e| refuse(format!("the signer or the signature is malformed: {e:?}")))?;
    if !verified {
        return Err(refuse("the signature does not verify under the receipt context".to_string()));
    }
    let document_matches = document.map(palw_document_id_of_v1).transpose()?.map(|recomputed| recomputed == document_id);
    Ok(CheckedReceiptV1 { document_id, signer, document_matches })
}

pub fn verify_receipt(receipt: &Path, document: Option<&Path>) -> Result<(), CliError> {
    let read = |p: &Path| -> Result<serde_json::Value, CliError> {
        let bytes = std::fs::read(p).map_err(|e| io(p, e))?;
        serde_json::from_slice(&bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: not JSON: {e}", p.display())))
    };
    let receipt = read(receipt)?;
    let document = document.map(read).transpose()?;
    let checked = palw_check_receipt_v1(&receipt, document.as_ref())?;
    println!("signature     verifies (signer {}…)", &checked.signer[..16.min(checked.signer.len())]);
    println!("document_id   {}", checked.document_id);
    match checked.document_matches {
        Some(true) => println!("document      its recomputed id is the one the receipt signed"),
        Some(false) => {
            return Err(CliError::new(exit::GENERIC, "the document's recomputed id is NOT the one the receipt signed".to_string()));
        }
        None => println!("document      not given — the receipt says who reported this id, not that any document has it"),
    }
    println!("a receipt is evidence of reproduction, not a vote: nothing reads it, and no count of them arms anything (ADR-0110)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings_512(stages: &[cv::PalwContextStageV1]) -> cv::PalwContextFindingsV1 {
        let ruleset = cv::PalwContextRulesetV1::devnet_held_v1().expect("the held devnet");
        let v = cv::palw_context_vector_v1("0110-dense-v7-512").expect("shipped");
        cv::palw_verify_context_vector_v1(&v, &ruleset, stages)
    }

    /// **Invariant 3: the document id covers what must agree and nothing the host measured**, and
    /// the tree's canonicaliser agrees with the vector module's on the whole document.
    #[test]
    fn the_document_id_excludes_the_host_and_moves_with_any_consensus_fact() {
        let f = findings_512(&[cv::PalwContextStageV1::Fit]);
        let a = f.document_json_v1(serde_json::json!({ "os": "a", "total_ms": 1 }));
        let b = f.document_json_v1(serde_json::json!({ "os": "b", "total_ms": 999_999 }));
        assert!(canonical(&a).is_ok(), "both canonicalisers agree");
        assert_ne!(canonical(&a).unwrap(), canonical(&b).unwrap(), "the host section is in the document");
        assert_eq!(palw_document_id_of_v1(&a).unwrap(), f.document_id());
        assert_eq!(palw_document_id_of_v1(&b).unwrap(), f.document_id(), "and not in its id");
        let mut moved = a.clone();
        moved["consensus"]["fit"][0]["need"] = serde_json::json!(1_000_000);
        assert_ne!(palw_document_id_of_v1(&moved).unwrap(), f.document_id(), "a consensus fact moves the id");
        let mut renamed = a.clone();
        renamed["verdicts"]["fit"] = serde_json::json!({ "fail": "x" });
        assert_ne!(palw_document_id_of_v1(&renamed).unwrap(), f.document_id(), "so does a verdict");
    }

    /// **Invariant 5: a receipt round-trips, and nothing else verifies as one.**
    #[test]
    fn a_receipt_round_trips_and_refuses_a_changed_byte_or_another_context() {
        let f = findings_512(&[cv::PalwContextStageV1::Fit]);
        let document = f.document_json_v1(serde_json::json!({ "os": "test" }));
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([7u8; 32]);
        let mut receipt = serde_json::json!({
            "receipt": PALW_CONTEXT_RECEIPT_V1,
            "document_id": f.document_id().to_string(),
            "vector_id": f.vector.vector_id().to_string(),
            "ruleset": { "name": f.ruleset_name, "consensus_params_id": f.consensus_params_id },
            "host": { "os": "test" },
            "signer": faster_hex::hex_string(key.public_key()),
        });
        let message = canonical(&receipt).unwrap();
        receipt["signature"] =
            serde_json::json!(faster_hex::hex_string(&key.sign_with_context(&message, PALW_CONTEXT_RECEIPT_MLDSA87_CONTEXT)));
        let checked = palw_check_receipt_v1(&receipt, Some(&document)).expect("an honest receipt verifies");
        assert_eq!(checked.document_matches, Some(true));
        assert_eq!(checked.document_id, f.document_id());

        let mut changed = receipt.clone();
        changed["host"]["os"] = serde_json::json!("another");
        assert!(palw_check_receipt_v1(&changed, None).is_err(), "a changed byte fails");
        let mut other_context = receipt.clone();
        other_context["signature"] =
            serde_json::json!(faster_hex::hex_string(&key.sign_with_context(&message, b"misaka-palw/extension-receipt/v1")));
        assert!(palw_check_receipt_v1(&other_context, None).is_err(), "a signature under another context fails");
        let mut other_document = document.clone();
        other_document["consensus"]["fit"][0]["need"] = serde_json::json!(1_000_000);
        assert_eq!(palw_check_receipt_v1(&receipt, Some(&other_document)).unwrap().document_matches, Some(false));
    }

    /// **Invariant 6: no consensus, node or SDK crate reads a receipt or a document.** Pinned over
    /// the sources the way ADR-0108 I-9 pins its own receipts.
    #[test]
    fn no_consensus_node_or_sdk_crate_reads_a_receipt() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the workspace");
        let mut offenders = Vec::new();
        for dir in ["consensus/core/src", "consensus/src", "kaspad/src", "misaka-palw-sdk/src", "protocol/flows/src"] {
            let mut stack = vec![root.join(dir)];
            while let Some(path) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&path) else { continue };
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else if p.extension().is_some_and(|e| e == "rs") {
                        let text = std::fs::read_to_string(&p).unwrap_or_default();
                        for needle in ["context-receipt", "context-verification", "context_vector", "PalwContextFindingsV1"] {
                            if text.contains(needle) {
                                offenders.push(format!("{} names {needle}", p.display()));
                            }
                        }
                    }
                }
            }
        }
        assert!(offenders.is_empty(), "a receipt or a context document is read where the chain decides: {offenders:?}");
    }
}
