//! Offline COMPUTE job-spec dispatch — the node-side counterpart of the worker's P0-B gate.
//!
//! The shared-network `palw-worker` (runtime repo) refuses every job spec that is not
//! scheduler-signed under the LOCKED context `misaka-palw-v3/jobspec/mldsa87` and pinned to an
//! expected scheduler credential. This command is the emitter: it builds the worker wire schema
//! `misaka.palw.testnet-jobspec.v2+scheduler-mldsa87`, signs the slot-INDEPENDENT digest with the
//! scheduler's ML-DSA-87 key (libcrux), self-verifies the signature, and writes one JSON view per
//! replica slot. The worker side re-derives the digest and verifies with the RustCrypto `ml-dsa`
//! implementation, so every dispatched spec is also a cross-implementation FIPS-204 conformance
//! check.
//!
//! Dispatch is fail-closed: the same structural/policy bounds the worker enforces
//! (`JobSpecPolicy::t_shared_testnet`) are enforced here BEFORE signing, so a bonded scheduler
//! cannot be tricked into authorizing an out-of-policy job.

use clap::Parser;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw::PALW_COMPUTE_JOBSPEC_V2_MLDSA87_CONTEXT;
use kaspa_hashes::blake2b_512_keyed;
use kaspa_txscript::verify_mldsa87_with_context;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::da::load_key;
use super::parse_hash64;

/// Keyed-hash domain of the slot-independent job-spec signing digest (runtime
/// `testnet-jobspec-signing/v2`). HASH domain — deliberately not a row of the signature table.
const JOBSPEC_SIGNING_DOMAIN: &[u8] = b"testnet-jobspec-signing/v2";
const JOBSPEC_SIGNING_SCHEMA_VERSION: u16 = 2;
/// Runtime-local domains for deriving the testnet network / compute-set identities from labels.
const NETWORK_ID_DOMAIN: &[u8] = b"testnet-network-id/v1";
const NETWORK_ID_SCHEMA_VERSION: u16 = 3;
const COMPUTE_SET_DOMAIN: &[u8] = b"testnet-compute-set/v1";
const COMPUTE_SET_SCHEMA_VERSION: u16 = 3;

/// Worker-side `JobSpecPolicy::t_shared_testnet()` mirrored byte-for-byte: dispatch must never
/// sign what admission would refuse.
const MAX_PROMPT_TOKEN_COUNT: u32 = 32_768;
const MAX_EPOCH_WINDOW: u64 = 100_000;

#[derive(Parser, Debug)]
pub struct ComputeJobspecPayloadArgs {
    /// Network label; the network id is the framed keyed hash the runtime derives from it.
    #[arg(long, default_value = "t-shared-testnet", conflicts_with = "network_id_hex")]
    network: String,
    /// Explicit 128-hex network id (overrides --network).
    #[arg(long, value_parser = parse_hash64)]
    network_id_hex: Option<Hash64>,
    /// Compute-set label (the cross-vendor canonical-INTEGER calibration set by default).
    #[arg(long, default_value = "qwen2.5-0.5b-canonical-int", conflicts_with = "compute_set_id_hex")]
    compute_set: String,
    /// Explicit 128-hex compute-set id (overrides --compute-set).
    #[arg(long, value_parser = parse_hash64)]
    compute_set_id_hex: Option<Hash64>,
    /// Job challenge (128-hex, nonzero). Derive it from chain randomness (e.g. an accepted block
    /// hash / beacon output) — dispatch refuses an all-zero challenge.
    #[arg(long, value_parser = parse_hash64)]
    job_challenge: Hash64,
    /// Prompt/prefill token count both replicas must commit to.
    #[arg(long)]
    prompt_token_count: u32,
    /// Issue epoch (>= 1).
    #[arg(long)]
    issued_epoch: u64,
    /// Expiry epoch (>= issued epoch; bounded window).
    #[arg(long)]
    expires_epoch: u64,
    /// ML-DSA-87 seed of the (bonded) scheduler key that authorizes this job.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    scheduler_key: String,
    /// Directory receiving `jobspec.slot0.json` + `jobspec.slot1.json` (created; files must be new).
    #[arg(long)]
    out_dir: PathBuf,
}

fn framed_keyed_hash64(domain: &[u8], schema_version: u16, parts: &[&[u8]]) -> Hash64 {
    let mut data = Vec::new();
    data.extend_from_slice(&schema_version.to_be_bytes());
    for part in parts {
        data.extend_from_slice(&(part.len() as u64).to_be_bytes());
        data.extend_from_slice(part);
    }
    blake2b_512_keyed(domain, &data)
}

/// The slot-independent signing digest — byte-identical to the runtime's
/// `testnet::job_spec_signing_digest` (every consensus field except `replica_slot`, fixed-width LE).
fn job_spec_signing_digest(
    network_id: &Hash64,
    compute_set_id: &Hash64,
    job_challenge: &Hash64,
    prompt_token_count: u32,
    issued_epoch: u64,
    expires_epoch: u64,
) -> Hash64 {
    framed_keyed_hash64(
        JOBSPEC_SIGNING_DOMAIN,
        JOBSPEC_SIGNING_SCHEMA_VERSION,
        &[
            network_id.as_byte_slice(),
            compute_set_id.as_byte_slice(),
            job_challenge.as_byte_slice(),
            &prompt_token_count.to_le_bytes(),
            &issued_epoch.to_le_bytes(),
            &expires_epoch.to_le_bytes(),
        ],
    )
}

fn hex_of(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn write_new_file(path: &Path, contents: &str) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("refusing to overwrite {}: {error}", path.display()))?;
    file.write_all(contents.as_bytes()).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

pub fn compute_jobspec_payload(args: ComputeJobspecPayloadArgs) -> Result<(), String> {
    let network_id = args
        .network_id_hex
        .unwrap_or_else(|| framed_keyed_hash64(NETWORK_ID_DOMAIN, NETWORK_ID_SCHEMA_VERSION, &[args.network.as_bytes()]));
    let compute_set_id = args
        .compute_set_id_hex
        .unwrap_or_else(|| framed_keyed_hash64(COMPUTE_SET_DOMAIN, COMPUTE_SET_SCHEMA_VERSION, &[args.compute_set.as_bytes()]));

    // Fail-closed dispatch: mirror the worker's H-04 policy gate before any signature exists.
    let zero = Hash64::from_bytes([0u8; 64]);
    if network_id == zero {
        return Err("network id is all-zero".to_string());
    }
    if compute_set_id == zero {
        return Err("compute-set id is all-zero".to_string());
    }
    if args.job_challenge == zero {
        return Err("--job-challenge is all-zero; derive it from chain randomness".to_string());
    }
    if args.prompt_token_count == 0 {
        return Err("--prompt-token-count must be >= 1".to_string());
    }
    if args.prompt_token_count > MAX_PROMPT_TOKEN_COUNT {
        return Err(format!("--prompt-token-count {} exceeds the policy bound {MAX_PROMPT_TOKEN_COUNT}", args.prompt_token_count));
    }
    if args.issued_epoch == 0 {
        return Err("--issued-epoch 0 is reserved for genesis".to_string());
    }
    if args.expires_epoch < args.issued_epoch {
        return Err(format!("--expires-epoch {} precedes --issued-epoch {}", args.expires_epoch, args.issued_epoch));
    }
    if args.expires_epoch - args.issued_epoch > MAX_EPOCH_WINDOW {
        return Err(format!(
            "epoch window {} exceeds the policy bound {MAX_EPOCH_WINDOW}",
            args.expires_epoch - args.issued_epoch
        ));
    }

    let scheduler_key = load_key(&args.scheduler_key)?;
    let digest = job_spec_signing_digest(
        &network_id,
        &compute_set_id,
        &args.job_challenge,
        args.prompt_token_count,
        args.issued_epoch,
        args.expires_epoch,
    );
    let signature = scheduler_key.sign_with_context(digest.as_byte_slice(), PALW_COMPUTE_JOBSPEC_V2_MLDSA87_CONTEXT);
    // Self-verify with the consensus verifier before anything is written: a dispatch artifact
    // that does not verify locally must never leave this process.
    if !matches!(
        verify_mldsa87_with_context(
            scheduler_key.public_key(),
            digest.as_byte_slice(),
            &signature,
            PALW_COMPUTE_JOBSPEC_V2_MLDSA87_CONTEXT
        ),
        Ok(true)
    ) {
        return Err("freshly signed job spec failed self-verification; refusing to emit".to_string());
    }
    let scheduler_credential = kaspa_consensus_core::dns_finality::validator_id_from_pubkey(scheduler_key.public_key());

    fs::create_dir_all(&args.out_dir).map_err(|error| format!("cannot create {}: {error}", args.out_dir.display()))?;
    for replica_slot in [0u8, 1u8] {
        // The digest excludes the slot, so ONE scheduler signature authorizes both replica views.
        let spec = serde_json::json!({
            "schema": "misaka.palw.testnet-jobspec.v2+scheduler-mldsa87",
            "network_id": hex_of(network_id.as_byte_slice()),
            "compute_set_id": hex_of(compute_set_id.as_byte_slice()),
            "job_challenge": hex_of(args.job_challenge.as_byte_slice()),
            "replica_slot": replica_slot,
            "prompt_token_count": args.prompt_token_count,
            "issued_epoch": args.issued_epoch,
            "expires_epoch": args.expires_epoch,
            "scheduler_envelope": {
                "body_digest": hex_of(digest.as_byte_slice()),
                "algorithm": 1,
                "signer_credential_id": hex_of(scheduler_credential.as_byte_slice()),
                "signature": hex_of(&signature),
            },
            "scheduler_verifying_key": hex_of(scheduler_key.public_key()),
        });
        let path = args.out_dir.join(format!("jobspec.slot{replica_slot}.json"));
        write_new_file(&path, &spec.to_string())?;
        println!("jobspec_slot{replica_slot}: {}", path.display());
    }
    println!("payload_kind: compute-jobspec");
    println!("signing_context: {}", String::from_utf8_lossy(PALW_COMPUTE_JOBSPEC_V2_MLDSA87_CONTEXT));
    println!("network_id: {}", hex_of(network_id.as_byte_slice()));
    println!("compute_set_id: {}", hex_of(compute_set_id.as_byte_slice()));
    println!("body_digest: {}", hex_of(digest.as_byte_slice()));
    println!("scheduler_credential: {}", hex_of(scheduler_credential.as_byte_slice()));
    println!("worker_pin_flag: --scheduler-credential {}", hex_of(scheduler_credential.as_byte_slice()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_digest_is_slot_independent_framing_of_every_other_field() {
        let network = framed_keyed_hash64(NETWORK_ID_DOMAIN, NETWORK_ID_SCHEMA_VERSION, &[b"t-shared-testnet"]);
        let set = framed_keyed_hash64(COMPUTE_SET_DOMAIN, COMPUTE_SET_SCHEMA_VERSION, &[b"qwen2.5-0.5b-canonical-int"]);
        let challenge = Hash64::from_bytes([0x33u8; 64]);
        let digest = job_spec_signing_digest(&network, &set, &challenge, 4, 1, 100);
        // Independent manual framing: schema BE || (len BE || part)* under the keyed domain.
        let mut data = Vec::new();
        data.extend_from_slice(&2u16.to_be_bytes());
        let prompt = 4u32.to_le_bytes();
        let issued = 1u64.to_le_bytes();
        let expires = 100u64.to_le_bytes();
        let parts: [&[u8]; 6] =
            [network.as_byte_slice(), set.as_byte_slice(), challenge.as_byte_slice(), &prompt, &issued, &expires];
        for part in parts {
            data.extend_from_slice(&(part.len() as u64).to_be_bytes());
            data.extend_from_slice(part);
        }
        assert_eq!(digest, blake2b_512_keyed(JOBSPEC_SIGNING_DOMAIN, &data));
        // Any consensus-field change moves the digest; the slot is not an input at all.
        assert_ne!(digest, job_spec_signing_digest(&network, &set, &challenge, 5, 1, 100));
        assert_ne!(digest, job_spec_signing_digest(&network, &set, &challenge, 4, 2, 100));
    }
}
