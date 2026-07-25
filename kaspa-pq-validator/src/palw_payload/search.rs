//! Offline search-availability (0x3d-0x3f) operator tooling — ADR node-anchored-web-search-da
//! dispatch. No RPC or mutable node state is touched here; every artifact is a canonical wire
//! payload for `palw-submit`, revalidated with the consensus stateless validator before writing.

use clap::Parser;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw::da::{PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, palw_receipt_da_chunk_proof};
use kaspa_consensus_core::palw::search_snapshot::{
    PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT, PALW_SEARCH_MAX_ONCHAIN_CHALLENGE_BYTES, PALW_SEARCH_TIMEOUT_MLDSA87_CONTEXT,
    PALW_SEARCH_TX_VERSION_V1, PalwSearchChallengeTxV1, PalwSearchJobSpecV1, PalwSearchResponseTxV1, PalwSearchSnapshotV1,
    PalwSearchTimeoutTxV1,
};
use kaspa_pq_validator_core::parse_stake_bond_ref;
use kaspa_txscript::verify_mldsa87_with_context;
use std::{fs, path::PathBuf};

use super::da::{load_key, write_da_payload};
use super::parse_hash64;

fn mldsa_verify(public_key: &[u8], message: &[u8], signature: &[u8], context: &[u8]) -> bool {
    matches!(verify_mldsa87_with_context(public_key, message, signature, context), Ok(true))
}

/// Load and self-verify a JobSpec JSON file produced by `palw-search-retrieval --scheduler-seed`.
fn load_jobspec(path: &PathBuf) -> Result<PalwSearchJobSpecV1, String> {
    let bytes = fs::read(path).map_err(|error| format!("cannot read JobSpec {}: {error}", path.display()))?;
    let jobspec: PalwSearchJobSpecV1 =
        serde_json::from_slice(&bytes).map_err(|error| format!("JobSpec {} is not valid JSON: {error}", path.display()))?;
    jobspec.verify(mldsa_verify).map_err(|error| format!("JobSpec self-verification failed: {error}"))?;
    Ok(jobspec)
}

#[derive(Parser, Debug)]
pub struct SearchChallengePayloadArgs {
    /// Consensus PALW network-domain u32 (must match the node's configured `palw_network_id`).
    #[arg(long)]
    network_id: u32,
    /// Challenged DA object root (128-hex Hash64). Optional when --jobspec is given (derived from
    /// its anchor); required for a plain challenge against an already registered obligation.
    #[arg(long, value_parser = parse_hash64)]
    object_root: Option<Hash64>,
    /// Challenged chunk index.
    #[arg(long, default_value_t = 0)]
    chunk_index: u16,
    /// Active challenger provider bond, `txid:index`.
    #[arg(long)]
    challenger_bond: String,
    /// ML-DSA-87 seed for the challenger bond owner.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    owner_key: String,
    /// Scheduler-signed JobSpec JSON (from `palw-search-retrieval --scheduler-seed`). Attaching it
    /// makes this a REGISTERING challenge: dispatch validates the scheduler's bond in the on-chain
    /// registry and registers the obligation atomically before opening the challenge.
    #[arg(long)]
    jobspec: Option<PathBuf>,
    /// New file receiving the canonical 0x3d payload.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
pub struct SearchResponsePayloadArgs {
    /// Consensus PALW network-domain u32.
    #[arg(long)]
    network_id: u32,
    /// Canonical snapshot object bytes (from `palw-search-retrieval`); the proof is built over
    /// exactly these bytes and the object root is recomputed from them.
    #[arg(long)]
    object_file: PathBuf,
    /// Challenged chunk index the proof must cover.
    #[arg(long)]
    chunk_index: u16,
    /// New file receiving the canonical 0x3e payload.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Parser, Debug)]
pub struct SearchTimeoutPayloadArgs {
    /// Consensus PALW network-domain u32.
    #[arg(long)]
    network_id: u32,
    /// Object root whose challenge deadline elapsed.
    #[arg(long, value_parser = parse_hash64)]
    object_root: Hash64,
    /// Reporter's active provider bond, `txid:index`.
    #[arg(long)]
    reporter_bond: String,
    /// ML-DSA-87 seed for the reporter bond owner.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    owner_key: String,
    /// New file receiving the canonical 0x3f payload.
    #[arg(long)]
    out: PathBuf,
}

pub fn search_challenge_payload(args: SearchChallengePayloadArgs) -> Result<(), String> {
    let owner_key = load_key(&args.owner_key)?;
    let challenger_bond = parse_stake_bond_ref(&args.challenger_bond)?;
    let registration = args.jobspec.as_ref().map(load_jobspec).transpose()?;
    let object_root = match (&registration, args.object_root) {
        (Some(jobspec), maybe_root) => {
            let anchored = jobspec.signed_anchor.anchor.object_root;
            if let Some(root) = maybe_root
                && root != anchored
            {
                return Err(format!("--object-root {root} does not match the JobSpec anchor root {anchored}"));
            }
            if jobspec.assignment.network_id != args.network_id {
                return Err(format!(
                    "JobSpec assignment is bound to network {}, not --network-id {}",
                    jobspec.assignment.network_id, args.network_id
                ));
            }
            if args.chunk_index >= jobspec.signed_anchor.anchor.chunk_count {
                return Err(format!(
                    "--chunk-index {} is outside the anchored chunk count {}",
                    args.chunk_index, jobspec.signed_anchor.anchor.chunk_count
                ));
            }
            anchored
        }
        (None, Some(root)) => root,
        (None, None) => return Err("--object-root is required for a plain (non-registering) challenge".to_string()),
    };
    let mut challenge = PalwSearchChallengeTxV1 {
        version: PALW_SEARCH_TX_VERSION_V1,
        network_id: args.network_id,
        object_root,
        chunk_index: args.chunk_index,
        challenger_bond,
        challenger_public_key: owner_key.public_key().to_vec(),
        registration,
        signature: Vec::new(),
    };
    challenge.signature =
        owner_key.sign_with_context(challenge.signing_hash().as_byte_slice(), PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT).to_vec();
    let payload = challenge.encode().map_err(|error| format!("cannot encode search challenge: {error}"))?;
    if payload.len() > PALW_SEARCH_MAX_ONCHAIN_CHALLENGE_BYTES {
        return Err(format!("search challenge payload is {} bytes, above the isolation cap", payload.len()));
    }
    write_da_payload(&args.out, 0x3d, &payload)?;
    println!("payload_kind: search-challenge");
    println!("subnetwork_byte: 0x3d");
    println!("object_root: {object_root}");
    println!("chunk_index: {}", challenge.chunk_index);
    println!("registering: {}", challenge.registration.is_some());
    if let Some(registration) = &challenge.registration {
        println!("scheduler_bond: {}:{}", registration.assignment.scheduler_bond.transaction_id, registration.assignment.scheduler_bond.index);
        println!("availability_deadline_daa_score: {}", registration.signed_anchor.anchor.availability_deadline_daa_score);
    }
    println!("payload_file: {}", args.out.display());
    Ok(())
}

pub fn search_response_payload(args: SearchResponsePayloadArgs) -> Result<(), String> {
    let bytes = fs::read(&args.object_file).map_err(|error| format!("cannot read {}: {error}", args.object_file.display()))?;
    // The object file must be the exact canonical snapshot bytes: decode fail-closed, then the
    // recomputed commitment names the on-chain root this response answers.
    let snapshot = PalwSearchSnapshotV1::decode_strict(&bytes).map_err(|error| format!("snapshot decode: {error}"))?;
    let commitment = snapshot.da_commitment().map_err(|error| format!("snapshot commitment: {error}"))?;
    let proof = palw_receipt_da_chunk_proof(PALW_SEARCH_SNAPSHOT_DA_OBJECT_VERSION_V1, &bytes, args.chunk_index)
        .map_err(|error| format!("cannot build chunk proof: {error}"))?;
    let response = PalwSearchResponseTxV1 {
        version: PALW_SEARCH_TX_VERSION_V1,
        network_id: args.network_id,
        object_root: commitment.root,
        proof,
    };
    let payload = response.encode().map_err(|error| format!("cannot encode search response: {error}"))?;
    write_da_payload(&args.out, 0x3e, &payload)?;
    println!("payload_kind: search-response");
    println!("subnetwork_byte: 0x3e");
    println!("object_root: {}", response.object_root);
    println!("chunk_index: {}", response.proof.chunk_index);
    println!("payload_file: {}", args.out.display());
    Ok(())
}

pub fn search_timeout_payload(args: SearchTimeoutPayloadArgs) -> Result<(), String> {
    let owner_key = load_key(&args.owner_key)?;
    let reporter_bond = parse_stake_bond_ref(&args.reporter_bond)?;
    let mut timeout = PalwSearchTimeoutTxV1 {
        version: PALW_SEARCH_TX_VERSION_V1,
        network_id: args.network_id,
        object_root: args.object_root,
        reporter_bond,
        reporter_public_key: owner_key.public_key().to_vec(),
        signature: Vec::new(),
    };
    timeout.signature =
        owner_key.sign_with_context(timeout.signing_hash().as_byte_slice(), PALW_SEARCH_TIMEOUT_MLDSA87_CONTEXT).to_vec();
    let payload = timeout.encode().map_err(|error| format!("cannot encode search timeout: {error}"))?;
    write_da_payload(&args.out, 0x3f, &payload)?;
    println!("payload_kind: search-timeout");
    println!("subnetwork_byte: 0x3f");
    println!("object_root: {}", timeout.object_root);
    println!("payload_file: {}", args.out.display());
    Ok(())
}
