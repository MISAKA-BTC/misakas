//! `misaka-palw-fp-rail` — the executor rail (ADR-0044 FP-08/09): turn a gateway outbox artifact
//! into a signed, fundable free-prompt commitment transaction.
//!
//! ```text
//! <outbox>/fp-job-<id>.commitment-unsigned.borsh   (what the gateway assembled)
//!            + the PublicDA prompt ids (from the result artifact beside it)
//!            + the executor bond key (a seed file, or the signer sidecar in production)
//!            ▼
//!  build_fp_commitment_tx  ──▶  <outbox>/fp-job-<id>.commitment-tx.borsh  + a JSON summary
//! ```
//!
//! **What this binary does not do, deliberately.** It does not submit. Submission needs a funded
//! UTXO from a live wallet on a network whose consensus accepts subnetwork `0x4a` — and no
//! network does yet (see `docs/palw-fp-wiring-atomicity.md`). Building and signing is the part
//! that is fully determined today, so it is the part that ships; a binary that pretended to
//! submit would be a demo, not a rail.
//!
//! **The key.** `--bond-key-seed <file>` reads a raw 32-byte ML-DSA-87 keygen seed for drills and
//! devnets. Production keeps the bond key in `kaspa-pq-signer` and asks it for a
//! `SigningPurpose::PalwFpCommitmentV3` signature over the claim id; this binary's `--print-claim`
//! mode emits exactly that digest so a signer-backed rail can be scripted today without the key
//! ever reaching this process.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpWorkerResultV3, PalwFreePromptCommitmentV3, fp_claim_id_v3, fp_cu_v3};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{VALIDATOR_SEED_LEN, ValidatorKey};

fn die(msg: String) -> ! {
    eprintln!("[misaka-palw-fp-rail] fatal: {msg}");
    std::process::exit(1);
}

fn hex(h: Hash64) -> String {
    faster_hex::hex_string(h.as_byte_slice())
}

fn read_borsh<T: borsh::BorshDeserialize>(path: &Path, what: &str) -> T {
    let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("cannot read the {what} at {}: {e}", path.display())));
    borsh::from_slice(&bytes).unwrap_or_else(|e| die(format!("the {what} at {} does not decode: {e}", path.display())))
}

fn read_seed(path: &Path) -> [u8; VALIDATOR_SEED_LEN] {
    let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("cannot read the bond key seed: {e}")));
    if bytes.len() != VALIDATOR_SEED_LEN {
        die(format!("the bond key seed is {} bytes, not {VALIDATOR_SEED_LEN}", bytes.len()));
    }
    let mut seed = [0u8; VALIDATOR_SEED_LEN];
    seed.copy_from_slice(&bytes);
    seed
}

fn parse_outpoint(s: &str) -> TransactionOutpoint {
    let (txid, index) = s.split_once(':').unwrap_or_else(|| die(format!("{s:?} is not txid:index")));
    let mut out = [0u8; 64];
    if txid.len() != 128 || faster_hex::hex_decode(txid.as_bytes(), &mut out).is_err() {
        die(format!("{txid:?} is not a 128-hex transaction id"));
    }
    let index: u32 = index.parse().unwrap_or_else(|e| die(format!("{index:?} is not an output index: {e}")));
    TransactionOutpoint::new(Hash64::from_bytes(out), index)
}

fn main() {
    let mut args: VecDeque<String> = std::env::args().skip(1).collect();
    let mut artifact_stem: Option<PathBuf> = None;
    let mut seed_path: Option<PathBuf> = None;
    let mut funding: Option<String> = None;
    let mut funding_amount: u64 = 0;
    let mut fee: u64 = 250_000;
    let mut print_claim = false;
    let mut print_pubkey = false;
    let mut class_id: Option<String> = None;
    while let Some(arg) = args.pop_front() {
        let mut value = |what: &str| args.pop_front().unwrap_or_else(|| die(format!("{what} needs a value")));
        match arg.as_str() {
            "--artifact" => artifact_stem = Some(PathBuf::from(value("--artifact"))),
            "--bond-key-seed" => seed_path = Some(PathBuf::from(value("--bond-key-seed"))),
            "--funding-outpoint" => funding = Some(value("--funding-outpoint")),
            "--funding-amount" => funding_amount = value("--funding-amount").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--fee" => fee = value("--fee").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--class-id" => class_id = Some(value("--class-id")),
            "--print-claim" => print_claim = true,
            // The public key a `--bond-key-seed` file yields, so an operator can put the SAME key
            // in the gateway's identity file before any inference runs. Without this the two
            // halves of the rail can only be matched by a failed signing attempt.
            "--print-bond-pubkey" => print_pubkey = true,
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-fp-rail --artifact <outbox/fp-job-XXXX> [--print-claim] \
                 [--bond-key-seed <file> [--print-bond-pubkey] --funding-outpoint <txid:index> --funding-amount <sompi> \
                 [--fee <sompi>]] [--class-id <128hex>]"
            )),
        }
    }
    if print_pubkey {
        let seed = read_seed(&seed_path.unwrap_or_else(|| die("--print-bond-pubkey needs --bond-key-seed <file>".into())));
        let key = ValidatorKey::from_seed(seed);
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.fp-rail-bond-key.v1",
                "executor_pubkey": faster_hex::hex_string(key.public_key()),
                "validator_id": hex(key.validator_id),
            })
        );
        return;
    }
    let stem = artifact_stem.unwrap_or_else(|| die("--artifact <outbox/fp-job-XXXX> is required (the path WITHOUT a suffix)".into()));
    let unsigned_path = PathBuf::from(format!("{}.commitment-unsigned.borsh", stem.display()));
    let result_path = PathBuf::from(format!("{}.result.borsh", stem.display()));

    let mut commitment: PalwFreePromptCommitmentV3 = read_borsh(&unsigned_path, "unsigned commitment");
    let result: PalwFpWorkerResultV3 = read_borsh(&result_path, "worker result");

    // The rail re-derives what it is about to sign from the two artifacts, rather than trusting
    // either alone: the commitment must be the one this result produces, prompt ids included.
    // A mismatch here means the outbox was edited between the inference and the signature —
    // exactly the moment a rail must refuse.
    if commitment.job != result.job {
        die("the unsigned commitment and the worker result describe different jobs".into());
    }
    if commitment.trace_root != result.trace_root
        || commitment.output_root != result.output_root
        || commitment.schedule_root != result.schedule_root
        || commitment.trace_manifest_root != result.trace_manifest_root
        || commitment.trace_chunk_count != result.trace_chunk_count
        || commitment.decode_tokens_executed != result.decode_tokens_executed
        || commitment.stop_reason != result.stop_reason
    {
        die("the unsigned commitment does not match the execution it claims".into());
    }
    if result.job.prompt_token_ids_hash != kaspa_consensus_core::palw_v2::prompt_token_ids_hash_v2(&result.prompt_token_ids) {
        die("the worker result's prompt ids are not the ones its job binds".into());
    }
    // The one commitment field with NO counterpart in the worker result: the retention deadline
    // is a chain-time promise the caller makes, so it cannot be cross-checked — a different value
    // is a different, still-honest promise (and a different claim id). What CAN be checked is
    // that the promise was not already broken when it was made: a deadline at or before the job's
    // own anchor commits to serving nothing.
    if commitment.trace_retention_daa <= commitment.job.anchor_daa {
        die(format!(
            "the retention deadline {} is at or before the job's anchor {} — a promise to serve nothing",
            commitment.trace_retention_daa, commitment.job.anchor_daa
        ));
    }

    // A class id may be supplied to bind the commitment to the network's registered class (the
    // gateway's devnet identity file carries a placeholder). Rewriting it changes the claim id,
    // which is why it is an explicit flag rather than a silent default.
    if let Some(id) = class_id.as_deref() {
        let mut out = [0u8; 64];
        if id.len() != 128 || faster_hex::hex_decode(id.as_bytes(), &mut out).is_err() {
            die("--class-id is not 128 hex chars".into());
        }
        commitment.job.class_id = Hash64::from_bytes(out);
    }

    let claim_id = fp_claim_id_v3(&commitment);
    if print_claim {
        // The digest a signer sidecar signs under SigningPurpose::PalwFpCommitmentV3 — emitted so
        // a signer-backed rail can be scripted without the bond key ever entering this process.
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.fp-rail-claim.v1",
                "fp_claim_id": hex(claim_id),
                "signing_purpose": "PalwFpCommitmentV3",
                "prompt_tokens": commitment.job.prompt_tokens,
                "decode_tokens_executed": commitment.decode_tokens_executed,
                "cu": commitment.cu.to_string(),
            })
        );
        return;
    }

    let seed =
        read_seed(&seed_path.unwrap_or_else(|| die("--bond-key-seed <file> is required to sign (or use --print-claim)".into())));
    let key = ValidatorKey::from_seed(seed);
    if key.public_key() != commitment.job.executor_pubkey.as_slice() {
        die("the bond key does not match the commitment's executor_pubkey — this key cannot sign this job".into());
    }
    let funding_outpoint =
        parse_outpoint(&funding.unwrap_or_else(|| die("--funding-outpoint <txid:index> is required to sign".into())));
    if funding_amount <= fee {
        die(format!("--funding-amount {funding_amount} does not cover the fee {fee}"));
    }
    let funding_entry = UtxoEntry::new(funding_amount, kaspa_consensus_core::tx::ScriptPublicKey::default(), 0, false);

    // The bundle the network runs decides the price table and the quantization — the rail reads
    // the devnet bundle here because that is the only bundle that exists; an RC rail takes the
    // network's own. The builder re-applies every stateless rule before spending a fee.
    let bundle = kaspa_consensus_core::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(
        commitment.job.class_id,
        Hash64::from_u64_word(0xCA7),
        Hash64::from_u64_word(0xC0757),
        4_096,
        Hash64::from_u64_word(0xA7),
        kaspa_consensus_core::palw_fp_devnet_v3::palw_devnet_bond_registry_v1(
            kaspa_consensus_core::palw_fp_devnet_v3::palw_v2_min_genesis_bonds_v1(),
        ),
    )
    .unwrap_or_else(|e| die(format!("cannot construct the devnet bundle: {e}")));
    let weights = *bundle.freeprompt.cu_weights();
    let derived_cu = fp_cu_v3(commitment.job.prompt_tokens, commitment.decode_tokens_executed, &weights);
    if commitment.cu != derived_cu {
        die(format!(
            "the artifact's cu {} is not this bundle's price for the executed shape ({derived_cu}) — the gateway and the rail \
             disagree about the network's weights",
            commitment.cu
        ));
    }

    let tx = key
        .build_fp_commitment_tx(
            commitment.job.network_domain,
            commitment.clone(),
            result.prompt_token_ids.clone(),
            &weights,
            &bundle.freeprompt,
            funding_outpoint,
            &funding_entry,
            fee,
        )
        .unwrap_or_else(|e| die(format!("cannot build the commitment transaction: {e}")));

    let tx_path = PathBuf::from(format!("{}.commitment-tx.borsh", stem.display()));
    let tx_bytes = borsh::to_vec(&tx).unwrap_or_else(|e| die(format!("cannot serialize the transaction: {e}")));
    std::fs::write(&tx_path, &tx_bytes).unwrap_or_else(|e| die(format!("cannot write {}: {e}", tx_path.display())));

    let (quanta, pwu) = bundle.freeprompt.derive_quanta_and_pwu(commitment.cu).expect("the builder already refused a sub-quantum job");
    let summary = serde_json::json!({
        "schema": "misaka.palw.fp-rail-tx.v1",
        "fp_claim_id": hex(claim_id),
        "subnetwork": "0x4a (PALW_FP_COMMITMENT)",
        "transaction_bytes": tx_bytes.len(),
        "payload_bytes": tx.payload.len(),
        "cu": commitment.cu.to_string(),
        "quanta": quanta,
        "pwu": pwu,
        "prompt_tokens": commitment.job.prompt_tokens,
        "decode_tokens_executed": commitment.decode_tokens_executed,
        "trace_manifest_root": hex(commitment.trace_manifest_root),
        "trace_retention_daa": commitment.trace_retention_daa,
        "tx_file": tx_path.display().to_string(),
        "not_done_here": [
            "submission (no network accepts subnetwork 0x4a yet — docs/palw-fp-wiring-atomicity.md)",
            "funding selection (the outpoint and amount are supplied, not discovered)",
        ],
    });
    let summary_path = PathBuf::from(format!("{}.rail.json", stem.display()));
    std::fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap())
        .unwrap_or_else(|e| die(format!("cannot write {}: {e}", summary_path.display())));
    println!("{summary}");
}
