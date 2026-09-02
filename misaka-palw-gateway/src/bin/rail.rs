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
//! **What this binary does not do, deliberately.** It does not submit, and it does not choose the
//! funding: the outpoint and amount are supplied, not discovered. Submission is
//! `misaka palw fp-submit`, which lives in the CLI because that is where the RPC client, the
//! endpoint registry and the network identity already are — a second answer to "which node" is a
//! way for two answers to disagree.
//!
//! The reason this comment used to give — "no network accepts subnetwork `0x4a` yet" — was true
//! when it was written and is not now: `tx_validation_in_isolation` validates that subnetwork,
//! `calculate_l1_tag` has its algo-7 arm, and testnet-11 runs the `ConsensusV2` bundle. A stale
//! "it cannot work" is worse than no comment, because it stops the next person looking.
//!
//! **The key.** `--bond-key-seed <file>` reads a raw 32-byte ML-DSA-87 keygen seed for drills and
//! devnets. Production keeps the bond key in `kaspa-pq-signer` and asks it for a
//! `SigningPurpose::PalwFpCommitmentV3` signature over the claim id; this binary's `--print-claim`
//! mode emits exactly that digest so a signer-backed rail can be scripted today without the key
//! ever reaching this process.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use kaspa_consensus_core::palw_derived_v1::{
    PALW_DERIVED_V1_MLDSA87_CONTEXT, PalwDerivedArtifactV1, derived_id_v1, palw_derived_message_v1,
};
// `fp_claim_id_v3` is deliberately NOT imported here: ADR-0079 SA-2 moved the claim id the rail
// signs behind `palw_fp_sign_gate::signable_claim_id`, so the rail cannot re-derive one that the
// gate never checked.
use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpWorkerResultV3, PalwFreePromptCommitmentV3};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
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
    // ADR-0078 Decision 6: sign a derivation the gateway left unsigned in the outbox.
    let mut derive_stem: Option<PathBuf> = None;
    let mut print_derived_message = false;
    let mut class_id: Option<String> = None;
    // The class's canonical job in leaves — what a quantum is an eighth of (ADR-0074 Decision 5).
    // Defaults to the floor's; a rail for another class passes that class's `pwu_per_inference`.
    let mut class_leaves: u64 = 7_708;
    while let Some(arg) = args.pop_front() {
        let mut value = |what: &str| args.pop_front().unwrap_or_else(|| die(format!("{what} needs a value")));
        match arg.as_str() {
            "--artifact" => artifact_stem = Some(PathBuf::from(value("--artifact"))),
            "--bond-key-seed" => seed_path = Some(PathBuf::from(value("--bond-key-seed"))),
            "--funding-outpoint" => funding = Some(value("--funding-outpoint")),
            "--funding-amount" => funding_amount = value("--funding-amount").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--fee" => fee = value("--fee").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--class-id" => class_id = Some(value("--class-id")),
            "--class-leaves" => class_leaves = value("--class-leaves").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--print-claim" => print_claim = true,
            // The public key a `--bond-key-seed` file yields, so an operator can put the SAME key
            // in the gateway's identity file before any inference runs. Without this the two
            // halves of the rail can only be matched by a failed signing attempt.
            "--print-bond-pubkey" => print_pubkey = true,
            "--derive-artifact" => derive_stem = Some(PathBuf::from(value("--derive-artifact"))),
            "--print-derived-message" => print_derived_message = true,
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-fp-rail --artifact <outbox/fp-job-XXXX> [--print-claim] \
                 [--bond-key-seed <file> [--print-bond-pubkey] --funding-outpoint <txid:index> --funding-amount <sompi> \
                 [--fee <sompi>]] [--class-id <128hex>] [--class-leaves <u64>]\n       misaka-palw-fp-rail --derive-artifact <outbox/fp-job-XXXX> (--bond-key-seed <file> | --print-derived-message)"
            )),
        }
    }
    // **A derivation is signed by the same key as the claim, under its own context** (ADR-0078
    // Decision 4). The gateway wrote `<stem>.derived-unsigned.borsh`; this writes the consensus
    // object `<stem>.derived-object.borsh`, which `misaka palw submit-object` carries — or, with
    // `--print-derived-message`, emits the digest a signer sidecar signs under
    // `SigningPurpose::PalwDerivedArtifactV1`.
    if let Some(stem) = derive_stem {
        let unsigned_path = PathBuf::from(format!("{}.derived-unsigned.borsh", stem.display()));
        let object: PalwDerivedArtifactV1 = read_borsh(&unsigned_path, "unsigned derivation");
        let message = palw_derived_message_v1(&object);
        if print_derived_message {
            println!(
                "{}",
                serde_json::json!({
                    "schema": "misaka.palw.fp-rail-derived-message.v1",
                    "derived_id": hex(derived_id_v1(&object)),
                    "claim_id": hex(object.claim_id),
                    "message": hex(message),
                    "signing_purpose": "PalwDerivedArtifactV1",
                })
            );
            return;
        }
        let seed = read_seed(&seed_path.unwrap_or_else(|| die("--bond-key-seed <file> is required to sign (or use --print-derived-message)".into())));
        let key = ValidatorKey::from_seed(seed);
        if key.public_key() != object.executor_pubkey.as_slice() {
            die("the bond key does not match the derivation's executor_pubkey — this key cannot sign this derivation".into());
        }
        let signature = key.sign_with_context(message.as_byte_slice(), PALW_DERIVED_V1_MLDSA87_CONTEXT).to_vec();
        let consensus_object = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature };
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&consensus_object)
            .unwrap_or_else(|why| die(format!("the signed derivation would not ride: {why}")));
        let out = PathBuf::from(format!("{}.derived-object.borsh", stem.display()));
        std::fs::write(&out, borsh::to_vec(&consensus_object).unwrap()).unwrap_or_else(|e| die(format!("cannot write {}: {e}", out.display())));
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw.fp-rail-derived-object.v1",
                "derived_id": hex(derived_id_v1(&object)),
                "claim_id": hex(object.claim_id),
                "kind": object.kind,
                "object_file": out.display().to_string(),
                "submit": "misaka palw submit-object --object <object_file> --yes",
            })
        );
        return;
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
    let mut result: PalwFpWorkerResultV3 = read_borsh(&result_path, "worker result");

    // A class id may be supplied to bind the commitment to the network's registered class (the
    // gateway's devnet identity file carries a placeholder). Rewriting it changes the claim id,
    // which is why it is an explicit flag rather than a silent default. It is applied BEFORE the
    // gate, so the gate sees exactly the object that will be signed — a rewrite the gate never saw
    // would be a free field inside a signed object.
    if let Some(id) = class_id.as_deref() {
        let mut out = [0u8; 64];
        if id.len() != 128 || faster_hex::hex_decode(id.as_bytes(), &mut out).is_err() {
            die("--class-id is not 128 hex chars".into());
        }
        commitment.job.class_id = Hash64::from_bytes(out);
        // The result frame must move with it, or the gate below correctly refuses the pair. The
        // rewrite is the operator saying "this execution belongs to that registered class"; it is
        // not a claim that some OTHER execution did.
        result.job.class_id = commitment.job.class_id;
    }

    // **ADR-0079 Decision 8 / SA-2 — the one message shape.** The claim id is RE-DERIVED from the
    // commitment, and only after the commitment has been checked field by field against the worker
    // result frame that produced it. The check lives in
    // `kaspa_pq_validator_core::palw_fp_sign_gate` rather than inline here, so the local-seed form
    // below and the `--print-claim` digest a signer sidecar signs cannot disagree about what may
    // be signed. The old inline check omitted `execution_root` and `work_leaves` — the field a
    // court binds refutations to, and the field that prices the claim.
    let claim_id = match kaspa_pq_validator_core::palw_fp_sign_gate::signable_claim_id(&commitment, &result) {
        Ok(id) => id,
        Err(e) => die(format!("refusing to sign: {e}")),
    };
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
                "work_leaves": commitment.work_leaves,
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

    let tx = key
        .build_fp_commitment_tx(
            commitment.job.network_domain,
            commitment.clone(),
            result.prompt_token_ids.clone(),
            &bundle.freeprompt,
            class_leaves,
            funding_outpoint,
            &funding_entry,
            fee,
        )
        .unwrap_or_else(|e| die(format!("cannot build the commitment transaction: {e}")));

    let tx_path = PathBuf::from(format!("{}.commitment-tx.borsh", stem.display()));
    let tx_bytes = borsh::to_vec(&tx).unwrap_or_else(|e| die(format!("cannot serialize the transaction: {e}")));
    std::fs::write(&tx_path, &tx_bytes).unwrap_or_else(|e| die(format!("cannot write {}: {e}", tx_path.display())));

    let (quanta, pwu) = bundle
        .freeprompt
        .derive_quanta_and_pwu(commitment.work_leaves, class_leaves)
        .expect("the builder already refused a sub-quantum job");
    let summary = serde_json::json!({
        "schema": "misaka.palw.fp-rail-tx.v1",
        "fp_claim_id": hex(claim_id),
        "subnetwork": "0x4a (PALW_FP_COMMITMENT)",
        "transaction_bytes": tx_bytes.len(),
        "payload_bytes": tx.payload.len(),
        "work_leaves": commitment.work_leaves,
        "quanta": quanta,
        "pwu": pwu,
        "prompt_tokens": commitment.job.prompt_tokens,
        "decode_tokens_executed": commitment.decode_tokens_executed,
        "trace_manifest_root": hex(commitment.trace_manifest_root),
        "trace_retention_daa": commitment.trace_retention_daa,
        "tx_file": tx_path.display().to_string(),
        "not_done_here": [
            "submission (`misaka palw fp-submit --tx <this file> --yes`)",
            "funding selection (the outpoint and amount are supplied, not discovered)",
        ],
    });
    let summary_path = PathBuf::from(format!("{}.rail.json", stem.display()));
    std::fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap())
        .unwrap_or_else(|e| die(format!("cannot write {}: {e}", summary_path.display())));
    println!("{summary}");
}
