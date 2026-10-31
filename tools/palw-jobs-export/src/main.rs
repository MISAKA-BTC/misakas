//! Turn what the chain already committed into what a reader can see.
//!
//! An LLM-class block commits its job by construction: the INPUT is the anchor-derived prompt —
//! a pure function of (network, the block's own pre-PoW position, class, producer bond) that
//! every panel seat re-derives before judging — and the OUTPUT is the generated token sequence
//! inside the producer's retained material, the same bytes a seat opens against the committed
//! roots. Nothing here is a new source of truth; this binary just performs, offline, the exact
//! derivations the verifiers perform, and writes them down as JSON for the explorer.
//!
//! Inputs: a blocks dump (`palw_census_full.py` — every wRPC header field verbatim) and the
//! producer's retention directory. Every rebuilt header is verified by recomputing its BLOCK
//! HASH and comparing with the dump — a header this tool mis-assembled would derive a wrong
//! anchor and display a wrong prompt while looking plausible, so a mismatch is fatal for that
//! row, never papered over.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::hashing::header::{hash as header_hash, pre_pow_hash_64};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{PalwAttemptUnsignedV2, attempt_id_v2, palw_job_anchor_v1, palw_network_domain_v2_for};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
// The material decoders are gone with the reads that used them: this tool derives the anchor
// prompt (public by construction) and opens nothing the executor retains.
use misaka_palw_base0::qwen25_a16_backend::qwen25_a16_prompt_for_anchor;
use misaka_palw_base0::qwen36_backend::qwen36_prompt_for_anchor;
use serde_json::{Value, json};
use std::str::FromStr;

const QWEN36_CLASS: &str =
    "5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e";
const QWEN25_CLASS: &str =
    "4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7";
const QWEN36_VOCAB: usize = 248_320;
const QWEN25_VOCAB: usize = 151_936;
const QWEN36_JOB: (u32, u32) = (7, 2); // Qwen3.6-35B-A3B/graph-v3 canonical (prefill 7 / decode 2)
const QWEN25_JOB: (u32, u32) = (63, 2); // graph-v5@512 canonical (palw-class ledger: prefill 63 / decode 2)

fn die(msg: String) -> ! {
    eprintln!("palw-jobs-export: {msg}");
    std::process::exit(1)
}

fn hex_bytes(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect()
}

/// wRPC serializes byte fields as JSON arrays of numbers; a dump may also carry them as hex.
fn json_bytes(v: &Value) -> Option<Vec<u8>> {
    if let Some(s) = v.as_str() {
        return hex_bytes(s);
    }
    v.as_array()?.iter().map(|x| x.as_u64().and_then(|n| u8::try_from(n).ok())).collect()
}

fn h64(v: &Value) -> Option<Hash64> {
    let b = hex_bytes(v.as_str()?)?;
    <[u8; 64]>::try_from(b.as_slice()).ok().map(Hash64::from_bytes)
}

fn u64of(v: &Value) -> u64 {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0)
}

/// Rebuild a consensus `Header` from the wRPC JSON header — every field, verbatim.
fn header_from_json(h: &Value) -> Option<Header> {
    let parents = h
        .get("parentsByLevel")?
        .as_array()?
        .iter()
        .map(|lvl| lvl.as_array().map(|xs| xs.iter().filter_map(h64).collect::<Vec<_>>()))
        .collect::<Option<Vec<Vec<Hash64>>>>()?;
    let parents = kaspa_consensus_core::header::CompressedParents::try_from(parents).ok()?;
    let blue_work = kaspa_consensus_core::BlueWorkType::from_hex(h.get("blueWork")?.as_str()?).ok()?;
    let mut header = Header::new_finalized(
        h.get("version")?.as_u64()? as u16,
        parents,
        h64(h.get("hashMerkleRoot")?)?,
        h64(h.get("acceptedIdMerkleRoot")?)?,
        h64(h.get("utxoCommitment")?)?,
        u64of(h.get("timestamp")?),
        h.get("bits")?.as_u64()? as u32,
        u64of(h.get("nonce")?),
        h.get("powAlgoId").map(u64of).unwrap_or(0) as u8,
        u64of(h.get("daaScore")?),
        blue_work,
        u64of(h.get("blueScore")?),
        h64(h.get("pruningPoint")?)?,
    );
    header.evm_payload_hash = h.get("evmPayloadHash").and_then(h64).unwrap_or_default();
    header.evm_commitment_root = h.get("evmCommitmentRoot").and_then(h64).unwrap_or_default();
    header.overlay_commitment_root = h.get("overlayCommitmentRoot").and_then(h64).unwrap_or_default();
    header.palw_state_root = h.get("palwStateRoot").and_then(h64).unwrap_or_default();
    header.palw_commitment = h.get("palwCommitment").and_then(json_bytes).unwrap_or_default();
    header.hash = header_hash(&header);
    Some(header)
}

/// Export identity for every model class; unavailable profile codecs cannot hide a claim.
/// This is a decoded submission, never an assertion of admission or verification.
fn attempt_row(header: &Header, a: &PalwAttemptUnsignedV2, domain: Hash64) -> Value {
    let class_hex = a.class_id.to_string();
    let bond = TransactionOutpoint::new(TransactionId::from_bytes(a.executor_bond.transaction_id.as_bytes()), a.executor_bond.index);
    let anchor = palw_job_anchor_v1(
        domain,
        pre_pow_hash_64(header),
        a.class_id,
        &bond,
        kaspa_consensus_core::palw_attempt_v2::palw_nonce_bucket_v1(header.nonce),
    );
    // Only these historical codecs are implemented here. In particular, never
    // apply their small prefill/decode shape to a newer Qwen/TIR registration.
    let profile = if class_hex == QWEN36_CLASS {
        Some((QWEN36_VOCAB, QWEN36_JOB, "QWEN36"))
    } else if class_hex == QWEN25_CLASS {
        Some((QWEN25_VOCAB, QWEN25_JOB, "QWEN25-A16"))
    } else {
        None
    };
    let prompt = profile.map(|(vocab, job, _)| {
        if class_hex == QWEN36_CLASS {
            qwen36_prompt_for_anchor(anchor, vocab, job.0)
        } else {
            qwen25_a16_prompt_for_anchor(anchor, vocab, job.0)
        }
    });
    json!({
        "block": header.hash.to_string(),
        "daa": header.daa_score,
        "ts": header.timestamp,
        "class": profile.map(|(_, _, name)| name).unwrap_or(&class_hex),
        "class_id": class_hex,
        "claim": attempt_id_v2(a).to_string(),
        "executor_bond": bond.to_string(),
        "execution_root": a.execution_root.to_string(),
        "pwu": a.pwu,
        "anchor": anchor.to_string(),
        "prompt_ids": prompt,
        "prompt_derived": profile.is_some(),
        "prefill": profile.map(|(_, job, _)| job.0),
        "decode": profile.map(|(_, job, _)| job.1),
        "acceptance": "unconfirmed: consult the node claim state",
        // Answers and retained material are never opened or published here.
        "disclosed": Vec::<Value>::new(),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut blocks_path = None;
    let mut retention = None;
    let mut generated_cache: Option<String> = None;
    // **The free-prompt lane, from the gateway's own outbox** (ADR-0044/0077). A free-prompt claim
    // rides a transaction, not a header, and the chain carries its prompt's HASH and its output's
    // ROOT — never the text (the prompt is its author's). Text can only come from the gateway that
    // executed it: `fp-job-<id>.json` (`fp_claim_id`, `family`, `answer_untrimmed`, counts) beside
    // `<claim>.material` in its traces dir (the folded capture's `prompt_token_ids`) and, when the
    // author committed one, `fp-job-<id>.derived.json` (ADR-0078). Comma-separated, like
    // `--retention`; each entry is an outbox dir whose `traces/` holds the materials.
    let mut fp_outbox: Option<String> = None;
    let mut out_path = None;
    // The DISPLAY prefix ("misaka-…") is not the consensus identity: the domain is derived from
    // `NetworkId::to_string()` ("testnet-11") plus the genesis, exactly as every verifier derives
    // it. The old prefixed default could never parse as a NetworkId, so the tool refused its own
    // default the moment the domain became genesis-bound (audit M2-18).
    let mut network = "testnet-11".to_string();
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--blocks" => blocks_path = it.next().cloned(),
            "--retention" => retention = it.next().cloned(),
            // **Decode each claim once.** The publish cron runs every two minutes and the answer
            // for a given claim never changes, but the material is 253 MB and its decode peaks at
            // ~575 MB — on the fleet's 24 GB producer host that is a swap event per row per run
            // (measured 2026-09-04, when two 34 GiB mappings had already pushed it into swap).
            // With a cache the read happens once per new claim.
            "--generated-cache" => generated_cache = it.next().cloned(),
            "--fp-outbox" => fp_outbox = it.next().cloned(),
            "--out" => out_path = it.next().cloned(),
            "--network" => network = it.next().cloned().unwrap_or(network),
            other => die(format!("unknown arg {other}")),
        }
    }
    let blocks_path = blocks_path.unwrap_or_else(|| die("--blocks required".into()));
    let retention = retention.unwrap_or_else(|| die("--retention required".into()));
    let out_path = out_path.unwrap_or_else(|| die("--out required".into()));

    let dump: Value = serde_json::from_slice(&std::fs::read(&blocks_path).unwrap_or_else(|e| die(format!("{blocks_path}: {e}"))))
        .unwrap_or_else(|e| die(format!("{blocks_path}: {e}")));
    let blocks = dump.get("blocks").and_then(|b| b.as_array()).unwrap_or_else(|| die("dump has no blocks[]".into()));
    // **The genesis-bound domain the chain actually uses** (audit M2-18, re-audit R-8). Deriving it
    // from the network name alone — which this did — reproduces a domain no verifier uses, so every
    // anchor below, and therefore every prompt this tool prints, would be wrong while looking
    // plausible. The genesis comes from the network's own shipped params, the same place consensus
    // reads it.
    let params = kaspa_consensus_core::network::NetworkId::from_str(&network)
        .map(kaspa_consensus_core::config::params::Params::from)
        .unwrap_or_else(|e| die(format!("--network {network}: {e}")));
    let domain = palw_network_domain_v2_for(network.as_bytes(), Some(params.genesis.hash));
    let floor_class = match &params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.base_class_id),
        _ => None,
    };

    // claim hex -> the generated ids that claim's material holds. Loaded before the walk, written
    // after it; a missing or unreadable file is an empty cache, never a failure.
    // **Accepted and no longer read.** `--retention` and `--generated-cache` name the executor's
    // own files; since 2026-09-06 this tool publishes commitments only, so it opens neither. They
    // stay on the command line because the publish script passes them, and dropping them would
    // turn a privacy change into a deployment break.
    let _ = (&retention, &generated_cache);
    #[allow(unused)]
    let mut cache: std::collections::BTreeMap<String, Vec<u32>> = generated_cache
        .as_ref()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let mut rows = Vec::new();
    let mut hash_mismatch = 0usize;
    for b in blocks {
        let Some(h) = b.get("header") else { continue };
        let Some(commitment) = h.get("palwCommitment").and_then(json_bytes) else { continue };
        if commitment.is_empty() {
            continue;
        }
        let Some(header) = header_from_json(h) else { continue };
        let dumped_hash = b.get("hash").or_else(|| h.get("hash")).and_then(|v| v.as_str()).unwrap_or("");
        if header.hash.to_string() != dumped_hash {
            hash_mismatch += 1;
            continue; // a mis-assembled header would derive a WRONG prompt; refuse, never guess
        }
        let Ok(envelope) = kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&header.palw_commitment) else {
            continue;
        };
        // The floor is not a model. Every other native class gets identity metadata,
        // including registered/TIR classes for which this exporter has no prompt codec.
        if floor_class.is_some_and(|floor| floor == envelope.attempt.class_id) {
            continue;
        }
        rows.push(attempt_row(&header, &envelope.attempt, domain));
    }
    if let Some(path) = generated_cache.as_ref()
        && let Ok(bytes) = serde_json::to_vec(&cache)
    {
        let _ = std::fs::write(path, bytes);
    }
    let fp_rows = fp_outbox.as_deref().map(free_prompt_rows).unwrap_or_default();
    if let Some(path) = generated_cache.as_ref()
        && let Ok(bytes) = serde_json::to_vec(&cache)
    {
        let _ = std::fs::write(path, bytes);
    }
    let out = json!({
        "network": network,
        // **What this file may carry, stated in the file** (ADR-0062, ADR-0077 Decision 16).
        //
        // The rule is the network's, not the exporter's: a claim publishes COMMITMENTS, and the
        // content behind them lives with the executor and reaches the drawn seats over an
        // authenticated pull. A piece of it becomes public exactly when somebody demands it — a
        // data-availability accusation the executor answers on chain — and the demand costs the
        // accuser if the answer comes. So this file publishes what the chain carries and what any
        // reader can recompute, and it carries `disclosed` for the pieces a demand has opened.
        "visibility": {
            "rule": "commitments and chain-derived facts are published; content held in an executor's retention is not",
            "attempt_prompt": "anchor-derived — a lottery draw any reader recomputes from the block",
            "answers": "not published: the chain carries output_root; the ids ride the answer envelope served to seats",
            "free_prompt": "not published: PanelDa carries no ids on chain at all, and a PublicDa payload's ids are the \
                            block's to show, not this file's",
            "disclosed": "content a data-availability accusation forced on chain (ADR-0062 SA-2); empty until palw_da_court is in force"
        },
        "rows": rows,
        "fp_rows": fp_rows,
        "hash_mismatches": hash_mismatch
    });
    std::fs::write(&out_path, serde_json::to_vec_pretty(&out).unwrap()).unwrap_or_else(|e| die(format!("{out_path}: {e}")));
    eprintln!(
        "palw-jobs-export: {} LLM rows, {} free-prompt rows, {} hash mismatches → {}",
        rows.len(),
        fp_rows.len(),
        hash_mismatch,
        out_path
    );
}

/// The gateway's committed free-prompt jobs as rows: one per `fp-job-*.json` with `committed: true`.
/// The prompt ids come from the retained material (cached under `fp:<claim>` so the 750 MB v5 capture
/// is decoded once); the answer text is the gateway's own `answer_untrimmed`; derived artifacts are
/// joined by claim id from `*.derived.json`. Nothing here is verified against the chain — the page
/// joins these rows to the `FreePromptCommitted` events it decodes from transactions, by claim id,
/// and a row with no event is shown as what it is: the gateway's word.
fn free_prompt_rows(dirs: &str) -> Vec<Value> {
    fn family_class(family: &str) -> String {
        match family {
            "qwen25-a16" | "a16-v5" | "a16" => "QWEN25-A16".to_string(),
            "qwen36" => "QWEN36".to_string(),
            "qwen38-27b" => "QWEN38-27B".to_string(),
            other => other.to_string(),
        }
    }
    let mut rows = Vec::new();
    for dir in dirs.split(',').map(str::trim).filter(|d| !d.is_empty()) {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut derived_by_claim: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
        let mut jobs: Vec<Value> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if !name.starts_with("fp-job-") {
                continue;
            }
            let Some(v) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
            if name.ends_with(".derived.json") {
                if let Some(claim) = v.get("claim_id").and_then(|c| c.as_str()) {
                    derived_by_claim.entry(claim.to_string()).or_default().push(json!({
                        "kind": v.get("kind"),
                        "kind_name": v.get("kind_name"),
                        "artifact_bytes": v.get("artifact_bytes"),
                        "transformer": v.get("transformer"),
                        "derived_id": v.get("derived_id"),
                    }));
                }
            } else if name.ends_with(".json") && !name.ends_with(".rail.json") {
                jobs.push(v);
            }
        }
        for v in jobs {
            if v.get("committed").and_then(|c| c.as_bool()) != Some(true) {
                continue;
            }
            let Some(claim) = v.get("fp_claim_id").and_then(|c| c.as_str()).map(str::to_string) else { continue };
            // **The prompt is not read at all.** It used to be decoded out of the retained
            // `.material` — the file a claim's five drawn seats pull under ADR-0077 Decision 16 —
            // so that this feed could print it. Not publishing it would be enough to fix the page;
            // not READING it is what makes the guarantee checkable by looking at this function.
            let family = v.get("family").and_then(|f| f.as_str()).unwrap_or("");
            let chain = v.get("chain").cloned().unwrap_or(Value::Null);
            // **Neither the prompt nor the answer is published** (2026-09-06). The prompt was read
            // out of the executor's retained `.material` — the file ADR-0077 Decision 16 serves
            // only to the claim's readers — and the answer out of the gateway's own outbox. What a
            // reader is owed is the commitment and the counts, which is what a claim puts on chain;
            // what a reader may DEMAND is a disclosure, and that is the `disclosed` list.
            //
            // The ids are still computed above, because the cache they feed is the node's own and
            // never leaves it. They simply do not enter this file.
            rows.push(json!({
                "claim": claim,
                "class": family_class(family),
                "family": family,
                "prompt_tokens": v.get("prompt_tokens"),
                "decode_tokens": v.get("decode_tokens_executed"),
                "work_leaves": v.get("work_leaves"),
                "daa": chain.get("daa_score").cloned().unwrap_or(Value::Null),
                // ADR-0078's derived artifacts ARE on chain — the kind, the id and the hash — so
                // they stay: they are the claim's public product, not its private input.
                "derived": derived_by_claim.get(&claim).cloned().unwrap_or_default(),
                "disclosed": Vec::<Value>::new(),
            }));
        }
    }
    rows
}

#[cfg(test)]
mod attempt_metadata_tests {
    use super::*;
    fn attempt(class_id: Hash64) -> PalwAttemptUnsignedV2 {
        let h = Hash64::from_u64_word;
        PalwAttemptUnsignedV2 {
            version: 6,
            network_domain: h(1),
            challenge: h(2),
            class_id,
            executor_bond: TransactionOutpoint::new(TransactionId::from_bytes(h(3).as_bytes()), 0),
            executor_pubkey: vec![],
            operator_id: h(4),
            artifact_root: h(5),
            trace_root: h(6),
            output_root: h(7),
            pwu: 42,
            trace_manifest_root: h(8),
            trace_chunk_count: 1,
            trace_retention_daa: 900,
            execution_root: h(9),
        }
    }
    #[test]
    fn current_qwen_and_unknown_registered_classes_export_exact_identity_without_invented_prompt() {
        let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let header = Header::from(&params.genesis);
        for class_id in [Hash64::from_str("ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01").unwrap(), Hash64::from_u64_word(777)] {
            let a = attempt(class_id);
            let row = attempt_row(&header, &a, a.network_domain);
            assert_eq!(row["claim"], attempt_id_v2(&a).to_string());
            assert_eq!(row["class_id"], class_id.to_string());
            assert_eq!(row["executor_bond"], a.executor_bond.to_string());
            assert_eq!(row["execution_root"], a.execution_root.to_string());
            assert_eq!(row["prompt_derived"], false);
            assert!(row["prompt_ids"].is_null() && row["prefill"].is_null() && row["decode"].is_null());
            assert!(row.get("accepted").is_none() && row.get("output_ids").is_none());
        }
    }
    #[test]
    fn historical_known_codec_keeps_its_anchor_derived_prompt() {
        let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let a = attempt(Hash64::from_str(QWEN25_CLASS).unwrap());
        let row = attempt_row(&Header::from(&params.genesis), &a, a.network_domain);
        assert_eq!(row["prompt_derived"], true);
        assert_eq!(row["prompt_ids"].as_array().unwrap().len(), QWEN25_JOB.0 as usize);
        assert_eq!(row["decode"], QWEN25_JOB.1);
    }
}
