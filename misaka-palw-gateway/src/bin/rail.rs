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
    // `None` = read it from the node when `--rpc` is given (the class row's own
    // `pwu_per_inference`); without a node, the floor's 7,708 — which is wrong for every model
    // class, and said so below rather than silently.
    let mut class_leaves: Option<u64> = None;
    // `--watch <outbox>`: the submitter loop (see `watch`), which runs THIS binary once per job.
    let mut watch_outbox: Option<PathBuf> = None;
    let mut watch_interval_secs: u64 = 20;
    let mut watch_max_attempts: u32 = 3;
    let mut watch_once = false;
    let mut watch_coinbase_only = false;
    // `--print-identity`: the gateway's identity.json, assembled from the node and the key.
    let mut print_identity = false;
    let mut bond_flag: Option<String> = None;
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
            "--class-leaves" => class_leaves = Some(value("--class-leaves").parse().unwrap_or_else(|e| die(format!("{e}")))),
            "--watch" => watch_outbox = Some(PathBuf::from(value("--watch"))),
            "--interval" => watch_interval_secs = value("--interval").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--max-attempts" => watch_max_attempts = value("--max-attempts").parse().unwrap_or_else(|e| die(format!("{e}"))),
            "--once" => watch_once = true,
            "--coinbase-funding-only" => watch_coinbase_only = true,
            "--print-identity" => print_identity = true,
            "--bond" => bond_flag = Some(value("--bond")),
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
                 [--anchor-ttl-daa <n>]]\n       misaka-palw-fp-rail --watch <outbox> --bond-key-seed <file> --rpc <host:port> \
                 [--funding-outpoint <txid:index> --funding-amount <sompi>] [--coinbase-funding-only] [--interval <secs>] [--max-attempts <n>] [--once] \
                 [--fee <sompi>] [--class-leaves <u64>] [--retention-dir <dir>] [--anchor-ttl-daa <n>]\
                 \n       misaka-palw-fp-rail --print-identity --bond-key-seed <file> --rpc <host:port> --class-id <128hex> \
                 [--bond <txid:index>]\
                 \n       misaka-palw-fp-rail --derive-artifact <outbox/fp-job-XXXX> (--bond-key-seed <file> | --print-derived-message)"
            )),
        }
    }
    // **The gateway's identity, from the node and the key** — five fields an operator used to
    // gather from five places, one of which (`operator_id`) no command printed at all, and any of
    // which, copied wrong, surfaces only as a refused commitment after the inference.
    if print_identity {
        let seed = read_seed(&seed_path.unwrap_or_else(|| die("--print-identity needs --bond-key-seed <file>".into())));
        let endpoint = rpc_endpoint
            .unwrap_or_else(|| die("--print-identity needs --rpc <host:port>: every field but the key is the chain's".into()));
        let class = class_id.as_deref().unwrap_or_else(|| {
            die("--print-identity needs --class-id <128hex>: the class the gateway serves (kaspad --palw-dump-classes; \
                 `misaka palw certified <id>` says whether it is on the free-prompt lane)"
                .into())
        });
        print_gateway_identity(&endpoint, &ValidatorKey::from_seed(seed), class, bond_flag.as_deref());
        return;
    }
    // **The submitter the gateway needs beside it.** The gateway holds no key (ADR-0079 Decision
    // 4), so every commitment it writes waits in the outbox until something with the bond key
    // carries it — and until this mode existed, the only thing that did was a script on the pool's
    // own hosts. An outside operator ran the gateway, watched `v3 executed` scroll by, and had no
    // way to learn that nothing had reached the chain.
    if let Some(outbox) = watch_outbox {
        let seed = seed_path.unwrap_or_else(|| die("--watch needs --bond-key-seed <file>: it signs every job it submits".into()));
        let rpc = rpc_endpoint
            .unwrap_or_else(|| die("--watch needs --rpc <host:port>: it submits, and reads the chain between jobs".into()));
        let funding = match (funding, funding_amount) {
            (Some(outpoint), amount) if amount > 0 => Some((outpoint, amount)),
            (Some(_), _) => {
                die("--funding-outpoint needs --funding-amount <sompi> (the output's exact value: the signature commits to it)".into())
            }
            (None, _) => None,
        };
        let mut pass_through = Vec::new();
        if let Some(fee) = fee {
            pass_through.extend(["--fee".to_string(), fee.to_string()]);
        }
        if let Some(leaves) = class_leaves {
            pass_through.extend(["--class-leaves".to_string(), leaves.to_string()]);
        }
        if let Some(dir) = &retention_dir {
            pass_through.extend(["--retention-dir".to_string(), dir.display().to_string()]);
        }
        pass_through.extend(["--anchor-ttl-daa".to_string(), anchor_ttl_daa.to_string()]);
        watch::run(watch::WatchConfig {
            outbox,
            seed_path: seed,
            rpc,
            interval: std::time::Duration::from_secs(watch_interval_secs.max(1)),
            max_attempts: watch_max_attempts.max(1),
            once: watch_once,
            coinbase_funding_only: watch_coinbase_only,
            funding,
            pass_through,
        });
        return;
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
    // **The class's canonical leaves, from the chain whenever a node is named.** The default was
    // the floor's 7,708, and every model class's job is thousands of times that: the rail then
    // derived the cap (64 quanta) for any real job and printed a `quanta`/`pwu` the chain never
    // assigns — the numbers an operator reads to size the bond. The chain's own row is the only
    // source that cannot disagree with the transition.
    let class_leaves: u64 = match (class_leaves, rpc_endpoint.as_deref()) {
        (Some(leaves), _) => leaves,
        (None, Some(endpoint)) => chain_class_leaves(endpoint, commitment.job.class_id),
        (None, None) => {
            eprintln!(
                "warning: --class-leaves was not given and there is no --rpc to read it from, so this uses the floor's 7708. \
                 For a model class that misstates this job's quanta and pwu below: pass the class's pwu_per_inference \
                 (kaspad --palw-dump-classes), or --rpc so the node is asked"
            );
            7_708
        }
    };
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
    let funding_entry = UtxoEntry::new(funding_amount, funding_spk.clone(), 0, false);

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
    // **What this carrier leaves behind for the next one.** The commitment spends the one funding
    // input and pays its change back to the same script, so the change is a spendable outpoint the
    // moment the node accepts the transaction — the mempool resolves a child against its pending
    // parent. Named here, from the transaction itself, so a caller chaining submissions (the
    // `--watch` loop, or an operator by hand) never has to re-derive the index or the amount; the
    // amount matters because the signature commits to it.
    let next_funding = tx
        .outputs
        .iter()
        .enumerate()
        .find(|(_, output)| output.script_public_key == funding_spk)
        .map(|(index, output)| serde_json::json!({ "outpoint": format!("{}:{index}", tx.id()), "amount": output.value }));
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
        // The change output, spendable by the next submission once this one is accepted: pass it
        // as `--funding-outpoint`/`--funding-amount`. Null when the carrier left no change.
        "next_funding": next_funding,
        "class_leaves": class_leaves,
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
    let runtime = rpc_runtime();
    let client = try_rpc_connect(&runtime, endpoint).unwrap_or_else(|e| die(e));
    runtime.block_on(async {
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

/// The runtime every RPC exchange of this binary runs on — one worker thread is plenty for a
/// client that asks one question at a time.
fn rpc_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap_or_else(|e| die(format!("cannot start the RPC runtime: {e}")))
}

/// A wRPC-borsh client to `endpoint`, connected — the client shape `misaka --rpc` uses. Fallible
/// rather than fatal, because `--watch` has to outlive a node restart; the one-shot path turns the
/// error into `die`.
fn try_rpc_connect(runtime: &tokio::runtime::Runtime, endpoint: &str) -> Result<kaspa_wrpc_client::KaspaRpcClient, String> {
    use kaspa_wrpc_client::{
        KaspaRpcClient, WrpcEncoding,
        client::{ConnectOptions, ConnectStrategy},
    };
    let url = if endpoint.contains("://") { endpoint.to_string() } else { format!("ws://{endpoint}") };
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| format!("cannot build an RPC client for {url}: {e}"))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(std::time::Duration::from_secs(10)),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    runtime.block_on(client.connect(Some(options))).map_err(|e| format!("cannot reach {url}: {e}"))?;
    Ok(client)
}

/// **Print the gateway's `identity.json` for this key**, every field read from the node except
/// the key itself: the network domain (network id AND genesis, so a file made for another
/// incarnation of the network cannot be reused by accident), the bond — named, or found in the
/// registry by this key, the way `misaka bond status` finds it — its registered operator id, and a
/// check that the class exists and says whether it is seated on the free-prompt lane.
fn print_gateway_identity(endpoint: &str, key: &ValidatorKey, class_id: &str, bond: Option<&str>) {
    use kaspa_rpc_core::api::rpc::RpcApi;
    let class: Hash64 = class_id.trim().parse().unwrap_or_else(|_| die(format!("--class-id {class_id:?} is not a 128-hex class id")));
    let ours = faster_hex::hex_string(key.public_key());
    let named = bond.map(parse_outpoint);
    let runtime = rpc_runtime();
    let client = try_rpc_connect(&runtime, endpoint).unwrap_or_else(|e| die(e));
    let (info, found) = runtime.block_on(async {
        let info = client.get_server_info().await.unwrap_or_else(|e| die(format!("cannot read the node's server info: {e}")));
        let candidates: Vec<TransactionOutpoint> = match named {
            Some(outpoint) => vec![outpoint],
            None => client
                .get_palw_producer_facts(String::new(), String::new(), 0, false)
                .await
                .unwrap_or_else(|e| die(format!("cannot read the node's bond registry: {e}")))
                .locked_bond_outpoints
                .iter()
                .map(|s| parse_outpoint(s.as_str()))
                .collect(),
        };
        let mut found = None;
        for outpoint in candidates {
            let facts = client
                .get_palw_producer_facts(class.to_string(), outpoint.transaction_id.to_string(), outpoint.index, true)
                .await
                .unwrap_or_else(|e| die(format!("cannot read the facts of bond {}:{}: {e}", outpoint.transaction_id, outpoint.index)));
            if !facts.available {
                die(format!("this node's chain does not know class {class} — check the id against kaspad --palw-dump-classes"));
            }
            if facts.bond_known && facts.bond_registered_pubkey == ours {
                found = Some((outpoint, facts));
                break;
            }
            if named.is_some() {
                die(format!(
                    "bond {}:{} is {} on this chain",
                    outpoint.transaction_id,
                    outpoint.index,
                    if facts.bond_known { "registered to a different key than --bond-key-seed" } else { "not registered" }
                ));
            }
        }
        let _ = client.disconnect().await;
        (info, found)
    });
    let Some((outpoint, facts)) = found else {
        die("no bond registered to this key was found on this chain — register one (kaspad --palw-register-bond \
             --palw-producer-key <this seed>), or name it with --bond <txid:index> (the `registered bond` line in the node log)"
            .into())
    };
    let params = kaspa_consensus_core::config::params::Params::from(info.network_id);
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    if !facts.fp_certified {
        eprintln!(
            "warning: class {class} is NOT certified on this chain's free-prompt lane — every commitment the gateway writes \
             for it would be refused as FreePromptLaneUncertified (`misaka palw certified <id>`)"
        );
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "network_domain": network_domain.to_string(),
            "class_id": class.to_string(),
            "bond_txid": outpoint.transaction_id.to_string(),
            "bond_index": outpoint.index,
            "executor_pubkey": ours,
            "operator_id": facts.bond_operator_id,
        }))
        .expect("serializable")
    );
}

/// The class's canonical job in leaves, as the node's own class row prices it.
fn chain_class_leaves(endpoint: &str, class_id: Hash64) -> u64 {
    use kaspa_rpc_core::api::rpc::RpcApi;
    let runtime = rpc_runtime();
    let client = try_rpc_connect(&runtime, endpoint).unwrap_or_else(|e| die(e));
    let facts = runtime.block_on(async {
        let facts = client.get_palw_producer_facts(class_id.to_string(), String::new(), 0, false).await;
        let _ = client.disconnect().await;
        facts
    });
    let facts = facts.unwrap_or_else(|e| die(format!("cannot read class {class_id}'s facts from the node: {e}")));
    class_leaves_from_facts(&facts).unwrap_or_else(|why| die(why))
}

/// **`pwu_per_inference`, recovered from the facts** — through the lane's one inversion
/// (`misaka_palw_fp_submit::fp_class_canonical_leaves_v1`), with the refusals named for an
/// operator.
fn class_leaves_from_facts(facts: &kaspa_rpc_core::GetPalwProducerFactsResponse) -> Result<u64, String> {
    if !facts.available {
        return Err(format!(
            "the node does not know class {} — it is not registered on the chain this node follows (or the node is not a \
             ConsensusV2 network)",
            facts.class_id
        ));
    }
    let target: u128 =
        facts.class_target.parse().map_err(|_| format!("the node's class target {:?} is not a u128", facts.class_target))?;
    misaka_palw_fp_submit::fp_class_canonical_leaves_v1(target, facts.pwu).ok_or_else(|| {
        format!(
            "class {}'s pwu {} is not a whole number of inferences at its target; pass --class-leaves <pwu_per_inference> \
             (kaspad --palw-dump-classes)",
            facts.class_id, facts.pwu
        )
    })
}

// =============================================================================================
// `--watch <outbox>` — the submitter that runs beside the gateway
// =============================================================================================

/// **Every committed job in an outbox, carried to the chain one at a time.**
///
/// The gateway answers and commits; it holds no key (ADR-0079 Decision 4), so its commitments
/// wait in the outbox until a process with the bond key signs and submits them. Until this mode
/// the only thing that did so for every job was a script on the pool's own hosts, so an outside
/// operator who followed the docs ran the gateway, saw `v3 executed` for every prompt, and put
/// nothing on the chain. This is that script's job, in the binary that already owns the submit
/// path — and it runs THIS binary once per job, so the one-shot path's refusals (the sign gate,
/// the anchor expiry, the staging rules) stay the only answers there are.
///
/// What it adds is what one job cannot know about the next:
///
/// * **Funding chains.** Each carrier's change funds the next one (the rail names it as
///   `next_funding`); the mempool resolves a child against its pending parent, so claims are not
///   limited to one per block. The first funding is `--funding-outpoint`, or — on a node with
///   `--utxoindex` — the smallest unlocked, non-coinbase output at the bond key's address that is
///   not already spent in the mempool. The bond collateral and the panel's fee chain are excluded
///   by the node's own locked set, the same set `misaka wallet send` refuses.
/// * **One claim in flight.** A job is submitted only after the previous carrier has become a
///   claim, or has been reported dropped. A carrier can be accepted and mined while the chain
///   refuses the commitment inside it — `FreePromptExposureCeiling` when the bond's ceiling was
///   already full as it landed — and the fee is spent either way. On testnet-11 most of one pool
///   slot's claims went that way unseen (the rail had printed a txid for each), so the watcher
///   checks, and says so when a claim does not appear.
/// * **Exposure first.** Before spending a fee it asks the chain whether this job's claim fits the
///   bond: `pwu × slash`, with the pwu the chain's own quantization of the job's leaves, against
///   the ceiling less what the bond already backs. A job that does not fit waits for claims to
///   reach `Final`, which is when exposure is released; one that can never fit is given up.
mod watch {
    use super::{class_leaves_from_facts, die, rpc_runtime, try_rpc_connect};
    use kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3;
    use kaspa_pq_validator_core::ValidatorKey;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    pub struct WatchConfig {
        pub outbox: PathBuf,
        pub seed_path: PathBuf,
        pub rpc: String,
        pub interval: Duration,
        pub max_attempts: u32,
        pub once: bool,
        /// `--coinbase-funding-only`: when the watcher has to FIND funding, take only a mature
        /// coinbase output. For a submitter whose `--rpc` node is not the node whose panel spends
        /// from this address (a pool slot asks the host's explorer node): that node's locked set
        /// does not hold the other panel's fee float, and a float is carrier change, never
        /// coinbase — so this is the one filter that cannot reach for it.
        pub coinbase_funding_only: bool,
        /// `--funding-outpoint` / `--funding-amount`: spent once, by the first job that needs
        /// funding; every later job chains off the previous carrier's change.
        pub funding: Option<(String, u64)>,
        /// Flags handed to the one-shot run unchanged (`--fee`, `--class-leaves`, ...).
        pub pass_through: Vec<String>,
    }

    /// The watcher's memory, in the outbox beside the jobs it describes. Everything else it knows
    /// is re-read from the chain or from the outbox on every pass.
    const STATE_FILE: &str = "rail-watch-state.json";

    /// A chained change output below this is not used again. Each hop pays a fee, and a small
    /// output's storage mass grows as `1/value` (KIP-9) until the node refuses to relay the
    /// carrier as non-standard; stopping at 0.1 MSK stays far from that with a normal fee.
    pub(super) const MIN_FUNDING_SOMPI: u64 = 10_000_000;

    /// How long a carrier that has left the mempool may take to become a claim before the watcher
    /// reports it dropped: the block carrying it has to be merged by a chain block before the
    /// commitment is extracted, and on a slow or wide DAG that is more than one block.
    pub(super) const LANDING_GRACE_DAA: u64 = 30;

    #[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub(super) struct Funding {
        pub outpoint: String,
        pub amount: u64,
    }

    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    pub(super) struct Awaiting {
        pub stem: String,
        pub claim: String,
        pub txid: String,
        pub submitted_daa: u64,
    }

    #[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
    pub(super) struct State {
        #[serde(default)]
        pub next_funding: Option<Funding>,
        /// The operator's `--funding-outpoint` once it has been spent, so a restart with the same
        /// command line does not try to spend it again.
        #[serde(default)]
        pub operator_funding_used: Option<String>,
        #[serde(default)]
        pub awaiting: Option<Awaiting>,
        #[serde(default)]
        pub attempts: BTreeMap<String, u32>,
        /// Jobs this watcher has finished with, and how: `on-chain`, `dropped`, `expired`,
        /// `retired`, `gave-up`. A finished job is never looked at again.
        #[serde(default)]
        pub settled: BTreeMap<String, String>,
    }

    /// One committed job the gateway left in the outbox.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(super) struct Job {
        /// `<outbox>/fp-job-<16 hex>` — the path the rail's `--artifact` takes.
        pub stem: PathBuf,
        pub name: String,
        pub claim: String,
        pub commit_by_anchor_daa: Option<u64>,
        pub capture: Option<PathBuf>,
    }

    fn now_utc() -> String {
        // Civil date from Unix days (Howard Hinnant's algorithm): a timestamp on every line without
        // a date crate, because a watcher's log is read hours after the fact.
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rem / 3_600, rem % 3_600 / 60, rem % 60)
    }

    fn log(line: impl AsRef<str>) {
        println!("{} [misaka-palw-fp-rail] {}", now_utc(), line.as_ref());
    }

    /// **ADR-0122 Decision 8: one job's stage, as a line a program reads** — beside the prose
    /// `log` line, never instead of it. `work` is the claim id's first 16 hex (the id every later
    /// stage carries), `job` the outbox stem's; `key=value`, no spaces inside a value.
    fn event(stem: &str, claim: &str, stage: &str, extra: &str) {
        let job = stem.rsplit('/').next().unwrap_or(stem).trim_start_matches("fp-job-");
        let work = claim.get(..16).unwrap_or(claim);
        let extra = if extra.is_empty() { String::new() } else { format!(" {extra}") };
        log(format!("event work={work} job={job} lane=prompt stage={stage}{extra}"));
    }

    /// Say a waiting reason once, and again only when it changes — a queue that waits an hour on
    /// one cause must not bury the line that says what the cause is.
    #[derive(Default)]
    struct Quiet(BTreeMap<&'static str, String>);

    impl Quiet {
        fn say(&mut self, topic: &'static str, line: String) {
            if self.0.get(topic) != Some(&line) {
                log(&line);
                self.0.insert(topic, line);
            }
        }
        fn clear(&mut self, topic: &'static str) {
            self.0.remove(topic);
        }
    }

    pub(super) fn load_state(outbox: &Path) -> State {
        std::fs::read(outbox.join(STATE_FILE)).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    fn save_state(outbox: &Path, state: &State) -> Result<(), String> {
        let path = outbox.join(STATE_FILE);
        let tmp = outbox.join(format!("{STATE_FILE}.partial"));
        let bytes = serde_json::to_vec_pretty(state).map_err(|e| format!("cannot encode the watch state: {e}"))?;
        std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("cannot publish {}: {e}", path.display()))
    }

    /// **The queue: committed jobs with no rail record and no verdict, oldest first.**
    ///
    /// A gateway summary is `fp-job-<16 hex>.json`; the rail's record is `<stem>.rail.json`, and a
    /// job that has one was submitted by somebody (this watcher, or an operator by hand) and is
    /// not this queue's business. A summary renamed `.submit-failed` or `.expired` has no `.json`
    /// name any more and is never listed.
    pub(super) fn pending_jobs(outbox: &Path, state: &State, max_attempts: u32) -> Vec<Job> {
        let Ok(entries) = std::fs::read_dir(outbox) else { return Vec::new() };
        let mut found: Vec<(std::time::SystemTime, Job)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
            let Some(stem_name) = name.strip_suffix(".json") else { continue };
            if !stem_name.starts_with("fp-job-") || stem_name.contains('.') {
                continue; // `.rail.json`, `.derived.json` and every other suffixed record
            }
            if state.settled.contains_key(stem_name) || state.attempts.get(stem_name).copied().unwrap_or(0) >= max_attempts {
                continue;
            }
            let stem = outbox.join(stem_name);
            if PathBuf::from(format!("{}.rail.json", stem.display())).exists() {
                continue;
            }
            let Some(summary) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else {
                continue;
            };
            if summary.get("committed").and_then(|v| v.as_bool()) != Some(true) {
                continue; // answered, never committed: `not_committed_because` says why, and nothing can submit it
            }
            let Some(claim) = summary.get("fp_claim_id").and_then(|v| v.as_str()).map(str::to_string) else { continue };
            let job_id = summary.get("fp_job_id").and_then(|v| v.as_str()).unwrap_or_default();
            let trace_dir = summary
                .get("trace_dir")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| outbox.join("traces").join(job_id));
            // `--capture` is what lets the node open the claim's intervals for its seats. Without
            // it the node holds only the question and every seat's request is "not held" — the
            // claim is voided at the receipt deadline. Measured on testnet-11: a 300-token claim
            // staged 3,372 bytes of retention beside a 183 MB capture the node never saw.
            let capture = Some(trace_dir.join("material.bin")).filter(|p| p.is_file());
            let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            found.push((
                modified,
                Job {
                    stem,
                    name: stem_name.to_string(),
                    claim,
                    commit_by_anchor_daa: summary.get("commit_by_anchor_daa").and_then(|v| v.as_u64()),
                    capture,
                },
            ));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
        found.into_iter().map(|(_, job)| job).collect()
    }

    /// **The exposure this job's claim will reserve**, from the chain's own numbers: the pwu is
    /// the transition's quantization of the job's leaves (`fp_class_quantum_leaves_v1`,
    /// `fp_quanta_v3`), and the slash rate is read back out of the facts' canonical-claim
    /// exposure (`pwu_per_inference × slash_value_per_pwu` for a derived class). `None` when the
    /// facts do not divide cleanly — the pre-check is then skipped and the landing check still
    /// catches a refused commitment.
    pub(super) fn job_exposure(facts: &kaspa_rpc_core::GetPalwProducerFactsResponse, work_leaves: u64) -> Option<u128> {
        misaka_palw_fp_submit::fp_claim_exposure_v1(
            class_leaves_from_facts(facts).ok()?,
            facts.bond_claim_exposure.parse().ok()?,
            facts.fp_quanta_per_canonical_job,
            facts.fp_max_quanta_per_receipt,
            work_leaves,
        )
    }

    /// How a failed one-shot run is treated — by what it says about the CLAIM, because a queue
    /// that counts the node's restart against a good job renames it out of existence.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(super) enum Failure {
        /// The machine, not the job: retry next pass, count nothing.
        Transient,
        /// The funding outpoint is gone or taken: forget it, find another, count nothing.
        Funding,
        /// The job can never be submitted (its anchor lapsed, or the gateway retired it).
        Finished,
        /// Anything else: counted against the job.
        Job,
    }

    pub(super) fn classify(message: &str) -> Failure {
        let text = message.to_ascii_lowercase();
        if ["cannot reach", "connection", "timed out", "timeout", "websocket", "cannot read the node"].iter().any(|m| text.contains(m))
        {
            Failure::Transient
        } else if text.contains("anchor") && text.contains("expired") || text.contains("was retired by the gateway") {
            Failure::Finished
        } else if ["already spent", "orphan", "missing outpoint", "double spend", "does not cover the fee", "is not standard"]
            .iter()
            .any(|m| text.contains(m))
        {
            Failure::Funding
        } else {
            Failure::Job
        }
    }

    pub fn run(config: WatchConfig) {
        if !config.outbox.is_dir() {
            die(format!("--watch {} is not a directory (the gateway's --outbox)", config.outbox.display()));
        }
        // Read once, up front: a seed that cannot be read is a configuration error, not a job's.
        let key = ValidatorKey::from_seed(super::read_seed(&config.seed_path));
        let runtime = rpc_runtime();
        log(format!(
            "watching {} every {}s — node {}, up to {} attempts per job{}",
            config.outbox.display(),
            config.interval.as_secs(),
            config.rpc,
            config.max_attempts,
            if config.once { ", one pass" } else { "" }
        ));
        let mut quiet = Quiet::default();
        loop {
            match try_rpc_connect(&runtime, &config.rpc) {
                Ok(client) => {
                    let outcome = pass(&config, &runtime, &client, &key, &mut quiet);
                    let _ = runtime.block_on(client.disconnect());
                    match outcome {
                        Ok(()) => quiet.clear("pass"),
                        Err(why) => quiet.say("pass", format!("this pass stopped: {why}")),
                    }
                }
                Err(why) => quiet.say("pass", format!("the node is not reachable: {why}")),
            }
            if config.once {
                return;
            }
            std::thread::sleep(config.interval);
        }
    }

    fn pass(
        config: &WatchConfig,
        runtime: &tokio::runtime::Runtime,
        client: &kaspa_wrpc_client::KaspaRpcClient,
        key: &ValidatorKey,
        quiet: &mut Quiet,
    ) -> Result<(), String> {
        let info = runtime.block_on(client.get_server_info()).map_err(|e| format!("cannot read the node's server info: {e}"))?;
        if !info.is_synced {
            quiet.say("sync", "the node is not synced; nothing is submitted until it is".to_string());
            return Ok(());
        }
        quiet.clear("sync");
        let daa = info.virtual_daa_score;
        let mut state = load_state(&config.outbox);

        // 1. The previous carrier: did it become a claim?
        if let Some(awaiting) = state.awaiting.clone() {
            let claim = runtime
                .block_on(client.get_palw_free_prompt_claim(awaiting.claim.clone()))
                .map_err(|e| format!("cannot read the node's claim facts: {e}"))?;
            if claim.found {
                log(format!(
                    "{}: claim {} is on the chain — phase {}, accepted at DAA {}. Follow it with `misaka palw claim {}`",
                    awaiting.stem, awaiting.claim, claim.phase, claim.accepted_daa, awaiting.claim
                ));
                event(&awaiting.stem, &awaiting.claim, "ON_CHAIN", &format!("accepted_daa={}", claim.accepted_daa));
                state.settled.insert(awaiting.stem.clone(), "on-chain".into());
                state.awaiting = None;
                save_state(&config.outbox, &state)?;
            } else {
                let txid = awaiting.txid.parse::<kaspa_consensus_core::tx::TransactionId>().map_err(|_| "unreadable txid in state")?;
                let in_mempool = runtime.block_on(client.get_mempool_entry(txid, true, false)).is_ok();
                if in_mempool || daa < awaiting.submitted_daa.saturating_add(LANDING_GRACE_DAA) {
                    quiet.say(
                        "await",
                        format!(
                            "{}: carrier {} {} — the next job waits for it to become a claim",
                            awaiting.stem,
                            awaiting.txid,
                            if in_mempool { "is in the mempool" } else { "has left the mempool; waiting for the chain to extract it" }
                        ),
                    );
                    return Ok(());
                }
                log(format!(
                    "WARNING {}: carrier {} was submitted at DAA {} and the chain holds no claim {} at DAA {daa}. Either the \
                     transaction was never mined, or the chain refused the commitment inside it — most often because the \
                     bond's exposure ceiling was full when it landed: the node log then has \"a PALW lifecycle object was \
                     dropped … this claim would reserve …, above its exposure ceiling … (admission item 8, free-prompt \
                     lane)\". Its fee is spent; the job is not retried.",
                    awaiting.stem, awaiting.txid, awaiting.submitted_daa, awaiting.claim
                ));
                event(&awaiting.stem, &awaiting.claim, "DROPPED", &format!("txid={}", awaiting.txid));
                state.settled.insert(awaiting.stem.clone(), "dropped".into());
                state.awaiting = None;
                // The chained change is KEPT. A carrier whose commitment the chain refused is still
                // an accepted transaction — the state transition drops the payload, not the spend —
                // so its change is a real output; clearing it here would strand it on a node with
                // no UTXO index. If the carrier was never mined, the change does not exist and the
                // next submit fails on a missing input, which `classify` reads as a funding failure
                // and clears then.
                save_state(&config.outbox, &state)?;
            }
        }
        quiet.clear("await");

        // 2. The next job.
        let Some(job) = pending_jobs(&config.outbox, &state, config.max_attempts).into_iter().next() else {
            quiet.say("idle", "no committed job is waiting in the outbox".to_string());
            return Ok(());
        };
        quiet.clear("idle");
        if job.commit_by_anchor_daa.is_some_and(|deadline| daa > deadline) {
            log(format!(
                "{}: its anchor lapsed at DAA {} (the chain is at {daa}); a stale commitment is never submitted — re-ask the prompt",
                job.name,
                job.commit_by_anchor_daa.unwrap_or_default()
            ));
            state.settled.insert(job.name.clone(), "expired".into());
            return save_state(&config.outbox, &state);
        }
        let unsigned = PathBuf::from(format!("{}.commitment-unsigned.borsh", job.stem.display()));
        let commitment: PalwFreePromptCommitmentV3 = match misaka_palw_fp_submit::load_unsigned_commitment(&unsigned)
            .map_err(|e| e.to_string())
            .and_then(|bytes| borsh::from_slice(&bytes).map_err(|e| format!("does not decode: {e}")))
        {
            Ok(commitment) => commitment,
            Err(why) => {
                log(format!("{}: its commitment cannot be read ({why}); not submittable", job.name));
                state.settled.insert(job.name.clone(), "retired".into());
                return save_state(&config.outbox, &state);
            }
        };

        // 3. Does this job's claim fit the bond right now?
        let bond = commitment.job.executor_bond;
        let facts = runtime
            .block_on(client.get_palw_producer_facts(
                commitment.job.class_id.to_string(),
                bond.transaction_id.to_string(),
                bond.index,
                true,
            ))
            .map_err(|e| format!("cannot read the node's producer facts: {e}"))?;
        if !facts.available || !facts.fp_certified || !facts.bond_known {
            quiet.say(
                "chain",
                format!(
                    "{}: holding — {}",
                    job.name,
                    if !facts.available {
                        "the chain this node follows does not know the job's class".to_string()
                    } else if !facts.fp_certified {
                        "the job's class is not certified on the free-prompt lane (the chain would refuse the commitment)".to_string()
                    } else {
                        format!("the job's bond {}:{} is not registered on this chain", bond.transaction_id, bond.index)
                    }
                ),
            );
            return Ok(());
        }
        quiet.clear("chain");
        if let Some(need) = job_exposure(&facts, commitment.work_leaves) {
            let ceiling: u128 = facts.bond_exposure_ceiling.parse().unwrap_or(0);
            let backed: u128 = facts.bond_reserved_exposure.parse().unwrap_or(0);
            if need > ceiling {
                log(format!(
                    "{}: its claim would reserve {need} sompi of exposure and this bond's whole ceiling is {ceiling} — it can never \
                     fit. Register a bond sized for jobs this long (collateral ≥ 2 × the exposure), or ask for fewer tokens",
                    job.name
                ));
                give_up(&config.outbox, &job, &mut state, "gave-up")?;
                return Ok(());
            }
            if backed.saturating_add(need) > ceiling {
                quiet.say(
                    "room",
                    format!(
                        "{}: holding — its claim would reserve {need} sompi and the bond already backs {backed} of its {ceiling} \
                         ceiling. Exposure is released as claims reach Final",
                        job.name
                    ),
                );
                return Ok(());
            }
        }
        quiet.clear("room");

        // 4. Funding.
        let Some((outpoint, amount, source)) = pick_funding(config, runtime, client, key, &info, &state, quiet)? else {
            return Ok(());
        };
        quiet.clear("funding");

        // 5. The one-shot run, as its own process.
        log(format!("{}: submitting claim {} (funded by {outpoint}, {amount} sompi, {source})", job.name, job.claim));
        match submit_one(config, &job, &outpoint, amount) {
            Ok(summary) => {
                let txid = summary.get("submitted").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                if txid.is_empty() {
                    return Err(format!("{}: the rail exited cleanly but reported no submitted transaction", job.name));
                }
                if source == "--funding-outpoint" {
                    state.operator_funding_used = Some(outpoint.clone());
                }
                state.next_funding = summary
                    .get("next_funding")
                    .and_then(|v| serde_json::from_value::<Funding>(v.clone()).ok())
                    .filter(|f| f.amount >= MIN_FUNDING_SOMPI);
                state.attempts.remove(&job.name);
                state.awaiting =
                    Some(Awaiting { stem: job.name.clone(), claim: job.claim.clone(), txid: txid.clone(), submitted_daa: daa });
                save_state(&config.outbox, &state)?;
                log(format!(
                    "{}: SUBMITTED carrier {txid} — pwu {} quanta {} fee {} sompi{}",
                    job.name,
                    summary.get("pwu").and_then(|v| v.as_u64()).unwrap_or_default(),
                    summary.get("quanta").and_then(|v| v.as_u64()).unwrap_or_default(),
                    summary.get("fee_sompi").and_then(|v| v.as_u64()).unwrap_or_default(),
                    if job.capture.is_none() {
                        " — WARNING: no material.bin was found for this job, so the node holds only the question and seats cannot \
                         open its intervals"
                    } else {
                        ""
                    }
                ));
                event(&job.name, &job.claim, "SUBMITTED", &format!("txid={txid} submitted_daa={daa}"));
                Ok(())
            }
            Err(message) => {
                match classify(&message) {
                    Failure::Transient => quiet.say("pass", format!("{}: not submitted yet — {message}", job.name)),
                    Failure::Funding => {
                        log(format!("{}: the funding {outpoint} cannot be used ({message}); looking for another next pass", job.name));
                        if source == "--funding-outpoint" {
                            state.operator_funding_used = Some(outpoint);
                        }
                        state.next_funding = None;
                        save_state(&config.outbox, &state)?;
                    }
                    Failure::Finished => {
                        log(format!("{}: not submittable — {message}", job.name));
                        state.settled.insert(job.name.clone(), "expired".into());
                        save_state(&config.outbox, &state)?;
                    }
                    Failure::Job => {
                        let n = state.attempts.get(&job.name).copied().unwrap_or(0) + 1;
                        state.attempts.insert(job.name.clone(), n);
                        log(format!("{}: submit failed (attempt {n}/{}) — {message}", job.name, config.max_attempts));
                        if n >= config.max_attempts {
                            give_up(&config.outbox, &job, &mut state, "gave-up")?;
                        } else {
                            save_state(&config.outbox, &state)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Rename the summary out of the queue — `fp-job-<id>.json.submit-failed` — so neither this
    /// watcher nor an operator's next glob picks it up again, and remember why.
    fn give_up(outbox: &Path, job: &Job, state: &mut State, outcome: &str) -> Result<(), String> {
        let summary = PathBuf::from(format!("{}.json", job.stem.display()));
        let renamed = PathBuf::from(format!("{}.json.submit-failed", job.stem.display()));
        if let Err(e) = std::fs::rename(&summary, &renamed) {
            log(format!("{}: cannot rename {} out of the queue: {e}", job.name, summary.display()));
        } else {
            log(format!("{}: given up; renamed to {}", job.name, renamed.display()));
        }
        event(&job.name, &job.claim, "GAVE_UP", &format!("outcome={}", outcome.replace(' ', "_")));
        state.settled.insert(job.name.clone(), outcome.into());
        save_state(outbox, state)
    }

    /// **Which output pays the next carrier**: the previous carrier's change, then the operator's
    /// `--funding-outpoint` (once), then — on a node with a UTXO index — the lane's one funding
    /// selector (`misaka_palw_fp_submit::select_funding`: mature under both coinbase gates, not the
    /// node's locked collateral or its panel's fee chain, not already spent in the mempool, our own
    /// pending change eligible). `None` (after saying why) when there is nothing to spend.
    fn pick_funding(
        config: &WatchConfig,
        runtime: &tokio::runtime::Runtime,
        client: &kaspa_wrpc_client::KaspaRpcClient,
        key: &ValidatorKey,
        info: &kaspa_rpc_core::GetServerInfoResponse,
        state: &State,
        quiet: &mut Quiet,
    ) -> Result<Option<(String, u64, &'static str)>, String> {
        if let Some(funding) = &state.next_funding {
            return Ok(Some((funding.outpoint.clone(), funding.amount, "the previous carrier's change")));
        }
        if let Some((outpoint, amount)) = &config.funding
            && state.operator_funding_used.as_deref() != Some(outpoint.as_str())
        {
            return Ok(Some((outpoint.clone(), *amount, "--funding-outpoint")));
        }
        let address = key.funding_address(kaspa_addresses::Prefix::from(info.network_id));
        if !info.has_utxo_index {
            quiet.say(
                "funding",
                format!(
                    "no funding: the node runs without --utxoindex, so the watcher cannot look up {address} — pass \
                     --funding-outpoint <txid:index> --funding-amount <sompi> for an output at that address (the bond key's \
                     own; `misaka wallet send --to {address} --amount 1` makes one)"
                ),
            );
            return Ok(None);
        }
        let params = kaspa_consensus_core::config::params::Params::from(info.network_id);
        let policy = misaka_palw_fp_submit::FpFundingPolicy {
            virtual_daa: info.virtual_daa_score,
            coinbase_maturity: params.coinbase_maturity(),
            settlement_long_maturity_daa: params.dns_params.as_ref().map_or(0, |d| d.coinbase_settlement_long_maturity_daa),
        };
        let coinbase_only = config.coinbase_funding_only;
        match runtime.block_on(misaka_palw_fp_submit::select_funding_where(client, &address, MIN_FUNDING_SOMPI, policy, |f| {
            !coinbase_only || f.entry.is_coinbase
        })) {
            Ok(found) => {
                let outpoint = format!("{}:{}", found.outpoint.transaction_id, found.outpoint.index);
                if state.operator_funding_used.as_deref() == Some(outpoint.as_str()) {
                    quiet.say("funding", format!("no funding: the only output found at {address} is the one already spent"));
                    return Ok(None);
                }
                Ok(Some((
                    outpoint,
                    found.entry.amount,
                    if coinbase_only { "a mature coinbase output (--coinbase-funding-only)" } else { "the lane's funding selector" },
                )))
            }
            Err(misaka_palw_fp_submit::FpSubmitError::NoFunding { .. }) => {
                quiet.say(
                    "funding",
                    format!(
                        "no funding: {address} holds no mature, unlocked {}output above {MIN_FUNDING_SOMPI} sompi that the mempool \
                         is not already spending. Send it some (`misaka wallet send --to {address} --amount 1`), or pass \
                         --funding-outpoint",
                        if coinbase_only { "COINBASE " } else { "" }
                    ),
                );
                Ok(None)
            }
            Err(e) => Err(format!("cannot select funding at {address}: {e}")),
        }
    }

    /// Run this binary once, in its one-shot form, for one job. Its stdout's last line is the
    /// rail summary; a failure is its `fatal:` line.
    fn submit_one(config: &WatchConfig, job: &Job, outpoint: &str, amount: u64) -> Result<serde_json::Value, String> {
        let exe = std::env::current_exe().map_err(|e| format!("cannot find this binary to run it per job: {e}"))?;
        let mut command = std::process::Command::new(exe);
        command
            .arg("--artifact")
            .arg(&job.stem)
            .arg("--bond-key-seed")
            .arg(&config.seed_path)
            .arg("--funding-outpoint")
            .arg(outpoint)
            .arg("--funding-amount")
            .arg(amount.to_string())
            .arg("--submit")
            .arg("--rpc")
            .arg(&config.rpc)
            .args(&config.pass_through);
        if let Some(capture) = &job.capture {
            command.arg("--capture").arg(capture);
        }
        let output = command.output().map_err(|e| format!("cannot run the one-shot rail: {e}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.lines().filter(|l| l.starts_with("warning:")) {
            log(format!("{}: {line}", job.name));
        }
        if !output.status.success() {
            let why = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("the one-shot rail failed without a message")
                .trim_start_matches("[misaka-palw-fp-rail] fatal: ")
                .to_string();
            return Err(why);
        }
        let last = stdout.lines().rev().find(|l| l.trim_start().starts_with('{')).ok_or("the one-shot rail printed no summary")?;
        serde_json::from_str(last).map_err(|e| format!("the one-shot rail's summary does not parse: {e}"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn scratch(name: &str) -> PathBuf {
            let dir = std::env::temp_dir().join(format!("rail-watch-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn summary(committed: bool, claim: &str) -> String {
            serde_json::json!({
                "schema": "misaka.palw.fp-v3-gateway-artifact.v1",
                "committed": committed,
                "fp_claim_id": claim,
                "fp_job_id": "ab".repeat(64),
                "commit_by_anchor_daa": 4_303,
                "not_committed_because": if committed { serde_json::Value::Null } else { "the bond's exposure ceiling leaves no room".into() },
            })
            .to_string()
        }

        /// **The queue is exactly the committed, unsubmitted, unsettled jobs.** An answered-only
        /// job has nothing to submit; a job with a `.rail.json` was submitted by someone; a
        /// suffixed record is not a job; a settled or exhausted job is never retried.
        #[test]
        fn the_queue_is_the_committed_jobs_nobody_has_submitted() {
            let outbox = scratch("queue");
            std::fs::write(outbox.join("fp-job-0000000000000001.json"), summary(true, "c1")).unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000002.json"), summary(false, "c2")).unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000003.json"), summary(true, "c3")).unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000003.rail.json"), "{}").unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000004.json"), summary(true, "c4")).unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000005.json.submit-failed"), summary(true, "c5")).unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000006.derived.json"), summary(true, "c6")).unwrap();
            std::fs::write(outbox.join("fp-job-0000000000000007.json"), summary(true, "c7")).unwrap();
            let mut state = State::default();
            state.settled.insert("fp-job-0000000000000004".into(), "dropped".into());
            state.attempts.insert("fp-job-0000000000000007".into(), 3);

            let claims: Vec<String> = pending_jobs(&outbox, &state, 3).into_iter().map(|j| j.claim).collect();
            assert_eq!(claims, vec!["c1".to_string()]);
            // With a higher attempt budget the exhausted one comes back; the settled one never does.
            let claims: Vec<String> = pending_jobs(&outbox, &state, 4).into_iter().map(|j| j.claim).collect();
            assert!(claims.contains(&"c7".to_string()) && !claims.contains(&"c4".to_string()));
            let _ = std::fs::remove_dir_all(&outbox);
        }

        /// The capture is found where the gateway said it put it, and its absence is carried as
        /// `None` (the watcher warns rather than refusing: a question-only claim is still a claim).
        #[test]
        fn the_capture_is_taken_from_the_summarys_trace_dir() {
            let outbox = scratch("capture");
            let traces = outbox.join("traces").join("ab".repeat(64));
            std::fs::create_dir_all(&traces).unwrap();
            std::fs::write(traces.join("material.bin"), b"x").unwrap();
            std::fs::write(outbox.join("fp-job-00000000000000aa.json"), summary(true, "ca")).unwrap();
            let jobs = pending_jobs(&outbox, &State::default(), 3);
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].capture.as_deref(), Some(traces.join("material.bin").as_path()));
            assert_eq!(jobs[0].commit_by_anchor_daa, Some(4_303));
            std::fs::remove_file(traces.join("material.bin")).unwrap();
            assert_eq!(pending_jobs(&outbox, &State::default(), 3)[0].capture, None);
            let _ = std::fs::remove_dir_all(&outbox);
        }

        /// **A job's exposure is the chain's, not the canonical claim's.** testnet-11's A16 class:
        /// pwu_per_inference 6,630,544 at target MAX, slash 5, 8 quanta per canonical job, cap 64.
        /// A 256-token answer (33,152,720 leaves) is 40 quanta = 33,152,720 pwu = 165,763,600 sompi
        /// — five canonical claims, which is what a bond sized for "one claim" did not hold; the
        /// 300-token claim 019efe78… (42,272,640 leaves) is the 51 quanta / 42,269,718 pwu the
        /// chain assigned it.
        #[test]
        fn a_jobs_exposure_is_its_own_quanta_times_the_slash_rate() {
            let facts = kaspa_rpc_core::GetPalwProducerFactsResponse {
                available: true,
                class_id: "4277d84f".into(),
                class_target: u128::MAX.to_string(),
                pwu: 6_630_544,
                bond_claim_exposure: (6_630_544u128 * 5).to_string(),
                fp_quanta_per_canonical_job: 8,
                fp_max_quanta_per_receipt: 64,
                ..Default::default()
            };
            assert_eq!(job_exposure(&facts, 33_152_720), Some(165_763_600));
            assert_eq!(job_exposure(&facts, 42_272_640), Some(42_269_718 * 5));
            // Past the cap the quanta stop growing, and so does the exposure.
            assert_eq!(job_exposure(&facts, u64::MAX), Some(64 * 828_818 * 5));
            // Below one quantum there is no claim to price; the one-shot rail names that refusal.
            assert_eq!(job_exposure(&facts, 1), None);
            // A canonical exposure that is not a whole slash rate: no guess, the pre-check stands aside.
            let odd = kaspa_rpc_core::GetPalwProducerFactsResponse { bond_claim_exposure: "33152721".into(), ..facts.clone() };
            assert_eq!(job_exposure(&odd, 33_152_720), None);
        }

        /// The floor's facts invert too: its target is not MAX, so the facts' pwu is the expected
        /// executions times one inference, and the canonical leaves come back exactly.
        #[test]
        fn class_leaves_invert_the_attempt_pwu_at_any_target() {
            let target = u128::MAX >> 14;
            let executions = kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1(target);
            let facts = kaspa_rpc_core::GetPalwProducerFactsResponse {
                available: true,
                class_target: target.to_string(),
                pwu: kaspa_consensus_core::palw_pwu::palw_pwu_v1(target, 7_708),
                ..Default::default()
            };
            assert!(executions > 1);
            assert_eq!(super::super::class_leaves_from_facts(&facts), Ok(7_708));
            let unknown = kaspa_rpc_core::GetPalwProducerFactsResponse { available: false, ..facts };
            assert!(super::super::class_leaves_from_facts(&unknown).is_err());
        }

        /// **A failure is judged by what it says about the claim.** The node being down, or a
        /// funding outpoint an earlier carrier still holds, must not spend a job's attempts.
        #[test]
        fn a_failure_is_counted_only_when_it_is_the_jobs() {
            assert_eq!(classify("cannot reach ws://127.0.0.1:27210: connection refused"), Failure::Transient);
            assert_eq!(
                classify(
                    "the commitment was not submitted: submit ab: the funding UTXO is spent by a transaction still in the mempool (an earlier submission's change has not been mined yet) — wait for a block and re-run; nothing was carried: already spent"
                ),
                Failure::Funding
            );
            assert_eq!(classify("transaction abc is an orphan where orphan is disallowed"), Failure::Funding);
            assert_eq!(
                classify(
                    "the commitment was not submitted: this commitment's anchor is DAA 1 and it expired at 3001; the chain is at 4000"
                ),
                Failure::Finished
            );
            assert_eq!(classify("refusing to sign: the result and the commitment disagree on execution_root"), Failure::Job);
        }

        #[test]
        fn the_state_round_trips_and_an_unreadable_one_is_empty() {
            let outbox = scratch("state");
            let state = State {
                next_funding: Some(Funding { outpoint: format!("{}:0", "cd".repeat(64)), amount: 99_700_000 }),
                awaiting: Some(Awaiting { stem: "fp-job-1".into(), claim: "c".into(), txid: "t".into(), submitted_daa: 7 }),
                ..Default::default()
            };
            save_state(&outbox, &state).unwrap();
            let back = load_state(&outbox);
            assert_eq!(back.next_funding, state.next_funding);
            assert_eq!(back.awaiting, state.awaiting);
            std::fs::write(outbox.join(STATE_FILE), b"not json").unwrap();
            assert!(load_state(&outbox).next_funding.is_none(), "an unreadable state is a fresh start, not a crash");
            let _ = std::fs::remove_dir_all(&outbox);
        }

        #[test]
        fn the_timestamp_is_a_utc_instant() {
            let t = now_utc();
            assert_eq!(t.len(), 20, "{t}");
            assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T', "{t}");
        }
    }
}
