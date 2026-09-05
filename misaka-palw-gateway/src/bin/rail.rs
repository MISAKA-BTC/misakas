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
//! **ADR-0077 Decision 4 — one handoff.** With `--submit --rpc <host:port>` this binary finishes
//! the job: it signs, then hands the transaction to `misaka-palw-fp-submit`, which is the SAME
//! library `misaka palw fp-submit` calls. There is one place that answers "is this still fresh,
//! which funding, which subnetwork, when does the material become real", and a second copy of any
//! of those answers is a way for two answers to disagree.
//!
//! The signing half lives here rather than in the gateway because ADR-0079 Decision 4 says the
//! process that parses a stranger's HTTP text holds no key. The gateway therefore queues the
//! commitment with its anchor deadline, and this binary — which legitimately holds the bond key,
//! or asks the signer sidecar for one digest — is the half that spends the fee.
//!
//! **SA-1(b) rides the whole way.** The gateway's sweep renames a lapsed artifact `…​.expired`;
//! this binary reads through `load_unsigned_commitment`, which refuses that name, and the submit
//! path re-checks the anchor against the NODE's own DAA before it stages or broadcasts anything.
//!
//! The reason this comment used to give — "no network accepts subnetwork `0x4a` yet" — was true
//! when it was written and is not now: `tx_validation_in_isolation` validates that subnetwork,
//! `calculate_l1_tag` has its algo-7 arm, and testnet-11 runs the `ConsensusV2` bundle. A stale
//! "it cannot work" is worse than no comment, because it stops the next person looking.
//!
//! **The key.** `--bond-key-seed <file>` reads a 32-byte ML-DSA-87 keygen seed as hex, through
//! `kaspa_pq_validator_core::load_validator_seed` — the same reader `misaka-cli` and `kaspad` use
//! for the same files, and the one that enforces audit M-02's 0600/regular-file guard. For drills and
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
use kaspa_addresses::Prefix;
use kaspa_consensus_core::config::params::DEVNET_PARAMS;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::palw_freeprompt_v3::{PalwFpWorkerResultV3, PalwFreePromptCommitmentV3};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{ATTESTATION_TX_FEE_FLOOR_SOMPI, VALIDATOR_SEED_LEN, ValidatorKey};
use kaspa_txscript::{pay_to_address_script, script_class::ScriptClass};

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

/// The queued commitment, read the ONE way that honours the gateway's anchor sweep (ADR-0077
/// SA-1b). A plain `read_borsh` here would read straight through a `…​.expired` rename and submit
/// exactly the stale claim the rename exists to stop.
fn read_queued_commitment(path: &Path) -> PalwFreePromptCommitmentV3 {
    let bytes = misaka_palw_fp_submit::load_unsigned_commitment(path).unwrap_or_else(|e| die(e.to_string()));
    borsh::from_slice(&bytes).unwrap_or_else(|e| die(format!("the unsigned commitment at {} does not decode: {e}", path.display())))
}

/// The bond key seed, read the ONE way every other consumer reads it.
///
/// This was a private reader that took the file as `VALIDATOR_SEED_LEN` RAW bytes, while
/// `kaspa_pq_validator_core::load_validator_seed` — what `misaka-cli` and `kaspad` call for the
/// same files — takes it as whitespace-trimmed HEX. So one seed file had two formats and this
/// binary held the minority one: every drill and devnet that wrote a seed the node could read
/// handed this binary 64 bytes of hex text and got "the bond key seed is 64 bytes, not 32".
///
/// The raw form also skipped audit M-02's guard, which is the part that matters beyond a drill:
/// `load_validator_seed` refuses a non-regular file (symlink/device/fifo, checked without
/// following the link) and a group- or world-readable mode. This binary legitimately holds the
/// bond key and spends the fee with it — it is the last process that should sign with a key
/// anyone on the host can read.
fn read_seed(path: &Path) -> [u8; VALIDATOR_SEED_LEN] {
    let path = path.to_str().unwrap_or_else(|| die(format!("the bond key seed path {} is not UTF-8", path.display())));
    kaspa_pq_validator_core::load_validator_seed(path).unwrap_or_else(|e| die(e))
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
    // None = size the fee from the transaction's own compute mass at the node's relay rate (the
    // same estimator every other overlay tx uses); Some = the operator's explicit value.
    let mut fee: Option<u64> = None;
    let mut print_claim = false;
    let mut print_pubkey = false;
    // ADR-0078 Decision 6: sign a derivation the gateway left unsigned in the outbox.
    let mut derive_stem: Option<PathBuf> = None;
    let mut print_derived_message = false;
    let mut class_id: Option<String> = None;
    // The class's canonical job in leaves — what a quantum is an eighth of (ADR-0074 Decision 5).
    // Defaults to the floor's; a rail for another class passes that class's `pwu_per_inference`.
    let mut class_leaves: u64 = 7_708;
    // ADR-0077 Decision 4: the handoff continues through `misaka-palw-fp-submit`.
    let mut submit = false;
    let mut rpc_endpoint: Option<String> = None;
    let mut retention_dir: Option<PathBuf> = None;
    let mut capture_path: Option<PathBuf> = None;
    let mut dsl_path: Option<PathBuf> = None;
    // ADR-0077 SA-1(b). The gateway's own sweep uses the same number; it is spelled once here
    // because the two halves must retire the same artifacts, and a rail with a longer TTL would
    // submit exactly what the gateway retired.
    let mut anchor_ttl_daa: u64 = 3_000;
    while let Some(arg) = args.pop_front() {
        let mut value = |what: &str| args.pop_front().unwrap_or_else(|| die(format!("{what} needs a value")));
        match arg.as_str() {
            "--artifact" => artifact_stem = Some(PathBuf::from(value("--artifact"))),
            "--bond-key-seed" => seed_path = Some(PathBuf::from(value("--bond-key-seed"))),
            "--funding-outpoint" => funding = Some(value("--funding-outpoint")),
            "--funding-amount" => funding_amount = value("--funding-amount").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--fee" => fee = Some(value("--fee").parse().unwrap_or_else(|e| die(format!("{e}")))),
            "--class-id" => class_id = Some(value("--class-id")),
            "--class-leaves" => class_leaves = value("--class-leaves").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--print-claim" => print_claim = true,
            "--submit" => submit = true,
            "--rpc" => rpc_endpoint = Some(value("--rpc")),
            "--retention-dir" => retention_dir = Some(PathBuf::from(value("--retention-dir"))),
            "--capture" => capture_path = Some(PathBuf::from(value("--capture"))),
            "--dsl" => dsl_path = Some(PathBuf::from(value("--dsl"))),
            "--anchor-ttl-daa" => anchor_ttl_daa = value("--anchor-ttl-daa").parse().unwrap_or_else(|e| die(format!("{e}"))),
            // The public key a `--bond-key-seed` file yields, so an operator can put the SAME key
            // in the gateway's identity file before any inference runs. Without this the two
            // halves of the rail can only be matched by a failed signing attempt.
            "--print-bond-pubkey" => print_pubkey = true,
            "--derive-artifact" => derive_stem = Some(PathBuf::from(value("--derive-artifact"))),
            "--print-derived-message" => print_derived_message = true,
            other => die(format!(
                "unknown argument {other:?}\nusage: misaka-palw-fp-rail --artifact <outbox/fp-job-XXXX> [--print-claim] \
                 [--bond-key-seed <file> [--print-bond-pubkey] --funding-outpoint <txid:index> --funding-amount <sompi> \
                 [--fee <sompi>]] [--class-id <128hex>] [--class-leaves <u64>] \
                 [--submit --rpc <host:port> [--retention-dir <dir>] [--capture <material.bin>] [--dsl <fpd1>] \
                 [--anchor-ttl-daa <n>]]\n       misaka-palw-fp-rail --derive-artifact <outbox/fp-job-XXXX> (--bond-key-seed <file> | --print-derived-message)"
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
        let seed = read_seed(
            &seed_path.unwrap_or_else(|| die("--bond-key-seed <file> is required to sign (or use --print-derived-message)".into())),
        );
        let key = ValidatorKey::from_seed(seed);
        if key.public_key() != object.executor_pubkey.as_slice() {
            die("the bond key does not match the derivation's executor_pubkey — this key cannot sign this derivation".into());
        }
        let signature = key.sign_with_context(message.as_byte_slice(), PALW_DERIVED_V1_MLDSA87_CONTEXT).to_vec();
        let consensus_object = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature };
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&consensus_object)
            .unwrap_or_else(|why| die(format!("the signed derivation would not ride: {why}")));
        let out = PathBuf::from(format!("{}.derived-object.borsh", stem.display()));
        std::fs::write(&out, borsh::to_vec(&consensus_object).unwrap())
            .unwrap_or_else(|e| die(format!("cannot write {}: {e}", out.display())));
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

    let mut commitment: PalwFreePromptCommitmentV3 = read_queued_commitment(&unsigned_path);
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
    // The funding UTXO is the bond key's own fee float — genesis and the registrar both pay it to
    // the key's ML-DSA-87 P2PKH (`blake2b_512_address_payload(vk)`), which is what `funding_address`
    // derives. The entry's script is not decorative: the ML-DSA sighash commits to it, and the
    // builder mirrors it into the change output, so an empty script here signed the wrong digest
    // AND produced a change output the mempool refuses as "non-standard script form". The drill's
    // first live stage 5b (2026-09-04) found it that way; the prefix only affects the bech32 text,
    // never the script bytes, so any prefix yields the same entry.
    let funding_spk = pay_to_address_script(&key.funding_address(Prefix::Mainnet));
    if !ScriptClass::from_script(&funding_spk).is_pq_standard() {
        die("the funding entry's script is not a form the mempool relays — the rail derived it wrongly".into());
    }
    let funding_entry = UtxoEntry::new(funding_amount, funding_spk, 0, false);

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

    // **The prompt-commitment form, derived from the result rather than declared** (ADR-0081
    // Decision 3). The worker committed the job under its network's form; the ids match the job's
    // `prompt_token_ids_hash` under exactly one of the two, and the builder re-checks the same
    // match, so a result that fits neither is refused here by name instead of after the fee.
    let prompt_ids_form = [
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
    ]
    .into_iter()
    .find(|form| {
        kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_match_v1(
            *form,
            &result.prompt_token_ids,
            &commitment.job.prompt_token_ids_hash,
        )
    })
    .unwrap_or_else(|| die("the worker result's prompt ids do not commit to the job under either prompt-id form".into()));
    let build = |fee: u64| {
        key.build_fp_commitment_tx(
            commitment.job.network_domain,
            prompt_ids_form,
            commitment.clone(),
            result.prompt_token_ids.clone(),
            &bundle.freeprompt,
            class_leaves,
            funding_outpoint,
            &funding_entry,
            fee,
        )
        .unwrap_or_else(|e| die(format!("cannot build the commitment transaction: {e}")))
    };
    // A commitment's payload carries the job's prompt token ids, so its mass — and with it the
    // node's minimum relay fee (10 sompi per gram of compute mass) — is not a constant: the first
    // live submit (2026-09-04, 8,186-byte payload) was refused as "250000 fees … under the required
    // amount of 263870" with the old flat default. Build once at the floor to learn the payload's
    // size, size the fee from that shape with the same estimator the panel and `palw fp` use, and
    // build again at that fee unless the operator named one.
    let fee = fee.unwrap_or_else(|| {
        let probe = build(ATTESTATION_TX_FEE_FLOOR_SOMPI);
        let p = &DEVNET_PARAMS;
        let calc =
            MassCalculator::new(p.mass_per_tx_byte, p.mass_per_script_pub_key_byte, p.mass_per_sig_op, p.storage_mass_parameter);
        key.estimate_overlay_fee(&calc, Prefix::Mainnet, probe.payload.len(), false)
    });
    if funding_amount <= fee {
        die(format!("--funding-amount {funding_amount} does not cover the fee {fee}"));
    }
    let tx = build(fee);

    let tx_path = PathBuf::from(format!("{}.commitment-tx.borsh", stem.display()));
    let tx_bytes = borsh::to_vec(&tx).unwrap_or_else(|e| die(format!("cannot serialize the transaction: {e}")));
    std::fs::write(&tx_path, &tx_bytes).unwrap_or_else(|e| die(format!("cannot write {}: {e}", tx_path.display())));

    // **ADR-0077 Decision 4 — the same step.** Sign, then submit and stage the material through
    // the one library; not two commands an operator has to remember to run in order, and not a
    // shell-out. Every refusal below is named by `FpSubmitError`, including the SA-1(b) one that
    // fires when the node's DAA has passed this commitment's anchor deadline.
    let submitted = if submit {
        let endpoint = rpc_endpoint.unwrap_or_else(|| die("--submit needs --rpc <host:port>".into()));
        let capture = capture_path.as_ref().map(|path| {
            let bytes = std::fs::read(path).unwrap_or_else(|e| die(format!("cannot read the capture at {}: {e}", path.display())));
            misaka_palw_fp_submit::check_capture_shape(&bytes).unwrap_or_else(|e| die(e.to_string()));
            bytes
        });
        let dsl = dsl_path
            .as_ref()
            .map(|path| std::fs::read(path).unwrap_or_else(|e| die(format!("cannot read the DSL at {}: {e}", path.display()))));
        Some(submit_through_the_one_path(
            &endpoint,
            &tx,
            retention_dir.as_deref(),
            capture.as_deref(),
            // ADR-0084 Decision 5: the answer's ids ride beside the material as the `FPA1`
            // envelope, from the same result frame the commitment was checked against.
            &result.output_token_ids,
            // ADR-0077 Decision 16: under `PanelDa` the payload carries no ids, and these are the
            // only copy the seats will ever be shown.
            &result.prompt_token_ids,
            dsl.as_deref(),
            misaka_palw_fp_submit::AnchorExpiry::new(commitment.job.anchor_daa, anchor_ttl_daa),
        ))
    } else {
        None
    };

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
        "fee_sompi": fee,
        "work_leaves": commitment.work_leaves,
        "quanta": quanta,
        "pwu": pwu,
        "prompt_tokens": commitment.job.prompt_tokens,
        "decode_tokens_executed": commitment.decode_tokens_executed,
        "trace_manifest_root": hex(commitment.trace_manifest_root),
        "trace_retention_daa": commitment.trace_retention_daa,
        "tx_file": tx_path.display().to_string(),
        "submitted": submitted.as_ref().map(|s| s.txid.clone()),
        "material_file": submitted.as_ref().and_then(|s| s.material_file.clone()),
        // ADR-0084 Decision 5: the answer envelope beside the material, and the directory the
        // node serves both from — the fact that was missing when the first two public
        // free-prompt claims were staged where the node never looked.
        "answer_file": submitted.as_ref().and_then(|s| s.answer_file.clone()),
        "retention_dir": submitted.as_ref().and_then(|s| s.retention_dir.clone()),
        "retention_dir_source": submitted.as_ref().map(|s| s.retention_source),
        "commit_by_anchor_daa": commitment.job.anchor_daa.saturating_add(anchor_ttl_daa),
        "not_done_here": if submitted.is_some() {
            vec!["funding selection (the outpoint and amount are supplied, not discovered)"]
        } else {
            vec![
                "submission (`--submit --rpc <host:port>`, or `misaka palw fp-submit --tx <this file> --yes`)",
                "funding selection (the outpoint and amount are supplied, not discovered)",
            ]
        },
    });
    let summary_path = PathBuf::from(format!("{}.rail.json", stem.display()));
    std::fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap())
        .unwrap_or_else(|e| die(format!("cannot write {}: {e}", summary_path.display())));
    println!("{summary}");
}

/// **The submit half of Decision 4's handoff.**
///
/// A one-shot connection and one call into `misaka-palw-fp-submit`: the freshness check against
/// the node's own DAA, the material staged `.partial` before the broadcast, the rename only after
/// acceptance. Returns `(txid, material path)`.
fn submit_through_the_one_path(
    endpoint: &str,
    tx: &kaspa_consensus_core::tx::Transaction,
    retention_dir: Option<&Path>,
    capture: Option<&[u8]>,
    output_token_ids: &[u32],
    prompt_token_ids: &[u32],
    dsl: Option<&[u8]>,
    expiry: misaka_palw_fp_submit::AnchorExpiry,
) -> RailSubmitted {
    use kaspa_rpc_core::api::rpc::RpcApi;
    use kaspa_wrpc_client::{
        KaspaRpcClient, WrpcEncoding,
        client::{ConnectOptions, ConnectStrategy},
    };
    let url = if endpoint.contains("://") { endpoint.to_string() } else { format!("ws://{endpoint}") };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap_or_else(|e| die(format!("cannot start the RPC runtime: {e}")));
    runtime.block_on(async {
        let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
            .unwrap_or_else(|e| die(format!("cannot build an RPC client for {url}: {e}")));
        let options = ConnectOptions {
            block_async_connect: true,
            connect_timeout: Some(std::time::Duration::from_secs(10)),
            strategy: ConnectStrategy::Fallback,
            ..Default::default()
        };
        client.connect(Some(options)).await.unwrap_or_else(|e| die(format!("cannot reach {url}: {e}")));
        // **Where the node reads** (ADR-0084 Decision 5). Without `--retention-dir` the node is
        // asked for the directory its panel serves from, and the files go there — on this host,
        // which is the only place the answer means anything. A node that names none (no panel,
        // or a pre-ADR-0084 build) leaves nothing to stage into, and the rail says so rather than
        // writing a material the operator will find later beside a claim nobody could serve.
        let (retention_dir, retention_source) = match retention_dir {
            Some(dir) => (Some(dir.to_path_buf()), "--retention-dir"),
            None => {
                let facts = client
                    .get_palw_producer_facts(String::new(), String::new(), 0, false)
                    .await
                    .unwrap_or_else(|e| die(format!("cannot read the node's producer facts for its retention directory: {e}")));
                if facts.palw_retention_dir.is_empty() {
                    eprintln!(
                        "warning: the node names no PALW retention directory (no panel, or a build before ADR-0084) and \
                         --retention-dir was not given — the material and the answer envelope are NOT staged; nothing \
                         will serve this claim to its seats"
                    );
                    (None, "none")
                } else {
                    let dir = PathBuf::from(&facts.palw_retention_dir);
                    if !dir.is_dir() {
                        die(format!(
                            "the node serves PALW material from {} and that directory is not on this host — run the rail on the \
                             node's host, or pass --retention-dir for a directory the node's panel reads",
                            dir.display()
                        ));
                    }
                    (Some(dir), "the node's own (getPalwProducerFacts)")
                }
            }
        };
        let staging = misaka_palw_fp_submit::FpStaging {
            retention_dir: retention_dir.as_deref(),
            capture,
            output_token_ids: Some(output_token_ids),
            dsl_payload: dsl,
            expiry: Some(expiry),
            // ADR-0077 Decision 16: under `PanelDa` these are the only copy the seats will see.
            prompt_token_ids: Some(prompt_token_ids),
        };
        let done = misaka_palw_fp_submit::submit_fp_commitment(&client, tx, staging)
            .await
            .unwrap_or_else(|e| die(format!("the commitment was not submitted: {e}")));
        let _ = client.disconnect().await;
        RailSubmitted {
            txid: done.txid,
            material_file: done.material_path.map(|p| p.display().to_string()),
            answer_file: done.answer_path.map(|p| p.display().to_string()),
            retention_dir: retention_dir.map(|p| p.display().to_string()),
            retention_source,
        }
    })
}

/// What `--submit` produced, for the summary: the transaction, and where the node will serve
/// this claim from (ADR-0084 Decision 5).
struct RailSubmitted {
    txid: String,
    material_file: Option<String>,
    answer_file: Option<String>,
    retention_dir: Option<String>,
    retention_source: &'static str,
}
