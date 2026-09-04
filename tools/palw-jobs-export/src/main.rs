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
use kaspa_consensus_core::hashing::header::{hash as header_hash, pre_pow_hash_64};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2, palw_job_anchor_v1, palw_network_domain_v2_for};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use misaka_palw_base0::qwen25_a16_backend::{qwen25_a16_material_decode_v1, qwen25_a16_prompt_for_anchor};
use misaka_palw_base0::qwen36_backend::{qwen36_material_decode_v1, qwen36_prompt_for_anchor};
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
    let genesis = kaspa_consensus_core::network::NetworkId::from_str(&network)
        .map(|net| kaspa_consensus_core::config::params::Params::from(net).genesis.hash)
        .unwrap_or_else(|e| die(format!("--network {network}: {e}")));
    let domain = palw_network_domain_v2_for(network.as_bytes(), Some(genesis));

    // claim hex -> the generated ids that claim's material holds. Loaded before the walk, written
    // after it; a missing or unreadable file is an empty cache, never a failure.
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
        let Ok(envelope) = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment) else { continue };
        let a = &envelope.attempt;
        let class_hex = a.class_id.to_string();
        let (vocab, job, name) = if class_hex == QWEN36_CLASS {
            (QWEN36_VOCAB, QWEN36_JOB, "QWEN36")
        } else if class_hex == QWEN25_CLASS {
            (QWEN25_VOCAB, QWEN25_JOB, "QWEN25-A16")
        } else {
            continue; // the floor's job is integer arithmetic, not tokens — out of scope here
        };
        let pre_pow = pre_pow_hash_64(&header);
        let bond =
            TransactionOutpoint::new(TransactionId::from_bytes(a.executor_bond.transaction_id.as_bytes()), a.executor_bond.index);
        // The bucket the block's own nonce names — the same fact a verifier reads, so an export
        // and a court derive one job (ADR-0071 Decision 2).
        let anchor = palw_job_anchor_v1(
            domain,
            pre_pow,
            a.class_id,
            &bond,
            kaspa_consensus_core::palw_attempt_v2::palw_nonce_bucket_v1(header.nonce),
        );
        let prompt: Vec<usize> = if class_hex == QWEN36_CLASS {
            qwen36_prompt_for_anchor(anchor, vocab, job.0)
        } else {
            qwen25_a16_prompt_for_anchor(anchor, vocab, job.0)
        };
        let claim = attempt_id_v2(a);
        // `--retention` is comma-separated: each producer holds only its own executions, and this
        // host may run several (the QWEN36 producer and the QWEN25 producer are different nodes).
        //
        // **One codec, asked in the order the producer might have written.** The per-class
        // decoders (`qwen36_material_decode_v1` / `qwen25_a16_material_decode_v1`) read the flat
        // rows-then-generated layout; since ADR-0077 a producer retains the FOLDED capture
        // (`base0_fp_material_encode_v2`: version, binding, sparse step tree, logits rows,
        // generated ids, prompt ids, checkpoints), and every 5f material on the public chain is
        // that shape — 253 MB each, and the flat decoder returns None on the first field, so the
        // page's answer column was empty for every row. `base0_material_decode_any_v1` tries the
        // folded form and falls back to the flat one, which is exactly what a seat does.
        let cached = cache.get(&claim.to_string()).cloned();
        let generated: Option<Vec<u32>> = if cached.is_some() {
            cached
        } else {
            retention.split(',').find_map(|dir| std::fs::read(format!("{dir}/{claim}.material")).ok()).and_then(|bytes| {
                match misaka_palw_base0::produce::base0_material_decode_any_v1(&bytes) {
                    Ok(misaka_palw_base0::produce::Base0RetentionV1::Folded(folded)) => Some(folded.generated_token_ids),
                    // **The attempt lane's retention is the DENSE tuple, and its fourth element is
                    // the answer** — `(binding, tiles, logits_rows, generated, checkpoints)`. A
                    // 5f QWEN36 material is 253 MB of which 2,685,360 entries are tiles, and it
                    // decodes in 0.15 s at 575 MB peak; measured on e78441c7…, generated
                    // `[979, 7287]`. Reading this arm and then falling through to the flat
                    // decoders — which is what this did for one revision — throws away the answer
                    // it just decoded and prints an empty column.
                    Ok(misaka_palw_base0::produce::Base0RetentionV1::Dense((_, _, _, generated, ..))) => Some(generated),
                    // The flat layout the earlier producers wrote, kept so an archived material
                    // still renders: the class decides which of the two readers understands it.
                    Err(_) => {
                        if class_hex == QWEN36_CLASS {
                            qwen36_material_decode_v1(&bytes).map(|r| r.generated)
                        } else {
                            qwen25_a16_material_decode_v1(&bytes).map(|r| r.generated)
                        }
                    }
                }
            })
        };
        if let Some(ids) = generated.as_ref() {
            cache.insert(claim.to_string(), ids.clone());
        }
        rows.push(json!({
            "block": dumped_hash,
            "daa": h.get("daaScore").map(u64of),
            "ts": h.get("timestamp").map(u64of),
            "class": name,
            "claim": claim.to_string(),
            "anchor": anchor.to_string(),
            "prompt_ids": prompt,
            "generated_ids": generated,
            "prefill": job.0,
            "decode": job.1,
        }));
    }
    if let Some(path) = generated_cache.as_ref()
        && let Ok(bytes) = serde_json::to_vec(&cache)
    {
        let _ = std::fs::write(path, bytes);
    }
    let fp_rows = fp_outbox.as_deref().map(|dirs| free_prompt_rows(dirs, &mut cache)).unwrap_or_default();
    if let Some(path) = generated_cache.as_ref()
        && let Ok(bytes) = serde_json::to_vec(&cache)
    {
        let _ = std::fs::write(path, bytes);
    }
    let out = json!({ "network": network, "rows": rows, "fp_rows": fp_rows, "hash_mismatches": hash_mismatch });
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
fn free_prompt_rows(dirs: &str, cache: &mut std::collections::BTreeMap<String, Vec<u32>>) -> Vec<Value> {
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
            let key = format!("fp:{claim}");
            let prompt_ids: Option<Vec<u32>> = if let Some(ids) = cache.get(&key) {
                Some(ids.clone())
            } else {
                // **The free-prompt lane's payload is not the attempt lane's, and four magic
                // bytes say so.** A free-prompt claim's material is `FPM1` — the job and the
                // prompt ids, which is all a seat needs because it RE-EXECUTES the job rather
                // than re-hashing a run's rows — or `FPC1`/`FPA1`, the same material with the
                // capture or the answer's ids attached. `base0_material_decode_any_v1` reads the
                // attempt lane's retention and returns None on the first field of all three, so
                // every free-prompt row on the public page printed an empty Input column while
                // its Output sat right beside it, decoded, from the same job.
                let ids = std::fs::read(format!("{dir}/traces/{claim}.material"))
                    .ok()
                    .and_then(|bytes| kaspa_consensus_core::palw_freeprompt_v3::palw_fp_job_material_decode_v1(&bytes))
                    .map(|material| material.prompt_token_ids);
                if let Some(ids) = ids.as_ref() {
                    cache.insert(key, ids.clone());
                }
                ids
            };
            let family = v.get("family").and_then(|f| f.as_str()).unwrap_or("");
            let chain = v.get("chain").cloned().unwrap_or(Value::Null);
            rows.push(json!({
                "claim": claim,
                "class": family_class(family),
                "family": family,
                "prompt_ids": prompt_ids,
                "prompt_tokens": v.get("prompt_tokens"),
                "generated_text": v.get("answer_untrimmed"),
                "decode_tokens": v.get("decode_tokens_executed"),
                "work_leaves": v.get("work_leaves"),
                "daa": chain.get("daa_score").cloned().unwrap_or(Value::Null),
                "derived": derived_by_claim.get(&claim).cloned().unwrap_or_default(),
            }));
        }
    }
    rows
}
