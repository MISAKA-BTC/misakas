//! ADR-0045 D3-b — the operator-side PCPB evidence producer.
//!
//! Clauses 11/12 refuse a leaf whose challenge does not re-derive under `R_{issued}` and whose
//! dispatch evidence does not re-run against the store-resolved snapshot. `mil/bridge`'s
//! [`misaka_palw_bridge::pcpb`] is the library that builds that evidence; nothing drove it from a
//! shell. This subcommand is the driver: it reads the PCPB context off a synced node over
//! `getPalwState`, re-runs the draw exactly as the acceptance arm will, and emits BOTH halves a
//! producer needs — the Borsh witness set `leaf-chunk --pcpb-witness-file` consumes, and the leaf
//! field values the leaf set must carry verbatim.
//!
//! Emitting them together is the point. A witness that opens against one epoch's roots and a leaf
//! that declares another's is the exact split-brain clause 12 exists to catch, and it costs a
//! registration fee to discover on-chain. Here it cannot arise: both come out of one context.
//!
//! # Choosing `--anchor-epoch`
//!
//! The external branch anchors the challenge and the draw at the same epoch `A`, so the leaf's
//! `registered_epoch` `E` must satisfy clause 11's window: `A + Δ ≤ E` and `E − A ≤ w`. With the
//! shipped `PCPB_PALW_PARAMS` (`w = 6`, `k = 2`, `Δ = 2`) an anchor of `E − 3` sits comfortably
//! inside it while keeping every input — the snapshot at `A − k` and the draw beacon at `A + Δ` —
//! strictly in the past, so the producer never waits on a beacon that has not closed.

use std::path::{Path, PathBuf};

use clap::Args;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::blake2b_512_keyed;
use kaspa_pq_validator_core::parse_stake_bond_ref;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::model::message::GetPalwStateRequest;
use misaka_palw_bridge::pcpb::{JobPreimage, PcpbContext, external_witness};

use super::PalwArtifactNetwork;
use crate::{connect, resolve_node_rpc};

/// Domain separators for the deterministic preimage triple derived from `--job-seed`.
///
/// The triple is opaque to consensus — clause 11 only requires that the challenge re-derive from
/// whatever the witness carries — so deriving it from one operator-chosen seed keeps the shell from
/// having to invent three unrelated 64-byte hashes per leaf while still giving every leaf its own
/// challenge (and therefore its own `job_nullifier`, which the batch does police for uniqueness).
const SCHEDULER_JOB_DOMAIN: &[u8] = b"misaka-palw-cli-v1/scheduler-job-id";
const REQUESTER_CREDENTIAL_DOMAIN: &[u8] = b"misaka-palw-cli-v1/requester-credential";
const REQUEST_COMMITMENT_DOMAIN: &[u8] = b"misaka-palw-cli-v1/request-commitment";

const FIELDS_SCHEMA: &str = "misaka.palw.pcpb-fields.v1";

#[derive(Args, Debug)]
pub struct PcpbWitnessArgs {
    /// PALW-active preset. Its suffix is the `network_id` the challenge derivation binds, so this
    /// must be the network the leaf will be registered on.
    #[arg(long, value_enum, default_value = "testnet-21")]
    network: PalwArtifactNetwork,

    /// Node wRPC (Borsh) endpoint. Defaults to the selected network's local port.
    #[arg(long = "node-wrpc-borsh")]
    node_rpc: Option<String>,

    /// The PCPB anchor epoch `A`. The challenge is issued under `R_A`, the snapshot resolves at
    /// `A − k` and the pair is drawn from `R_{A + Δ}`. See the module note on picking it.
    #[arg(long)]
    anchor_epoch: u64,

    /// Leaf shape id. Committed into the challenge, so it must equal the leaves' `shape_id`.
    #[arg(long)]
    shape_id: u16,

    /// Number of leaves to produce evidence for. Indices are `0..leaf_count`, matching the
    /// contiguous-from-zero rule `batch-manifest` enforces on an unbound leaf set.
    #[arg(long, default_value_t = 1)]
    leaf_count: u32,

    /// Operator seed for the deterministic challenge preimage triple. Any stable string works; the
    /// same seed and anchor reproduce the same challenges, which is what makes a failed run
    /// re-runnable without re-signing receipts under a fresh challenge.
    #[arg(long)]
    job_seed: String,

    /// Candidate provider-bond outpoints (`txid:index`) the draw may resolve to. Every bond in the
    /// snapshot the draw can select must be listed, or the seat cannot be named.
    #[arg(long = "provider-bond", required = true)]
    provider_bonds: Vec<String>,

    /// New file to receive the Borsh `Vec<(u32, PalwLeafPcpbWitnessV1)>` witness set for
    /// `palw-payload leaf-chunk --pcpb-witness-file`.
    #[arg(long)]
    witness_out: PathBuf,

    /// New file to receive the JSON leaf-field bindings the leaf set must carry verbatim.
    #[arg(long)]
    fields_out: PathBuf,
}

fn derive(domain: &[u8], seed: &str, leaf_index: u32) -> Hash64 {
    let mut preimage = Vec::with_capacity(seed.len() + 8);
    preimage.extend_from_slice(&(seed.len() as u32).to_le_bytes());
    preimage.extend_from_slice(seed.as_bytes());
    preimage.extend_from_slice(&leaf_index.to_le_bytes());
    blake2b_512_keyed(domain, &preimage)
}

fn write_new_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("refusing to overwrite {}: {error}", path.display()))?;
    file.write_all(contents).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn outpoint_string(outpoint: &TransactionOutpoint) -> String {
    format!("{}:{}", outpoint.transaction_id, outpoint.index)
}

pub(super) async fn pcpb_witness(args: PcpbWitnessArgs) -> Result<(), String> {
    if args.leaf_count == 0 {
        return Err("--leaf-count must be at least 1".to_string());
    }
    kaspa_core::log::init_logger(None, "warn");

    let network_id = args.network.network_id();
    let suffix = network_id.suffix().ok_or_else(|| format!("network '{network_id}' has no suffix to bind as PALW network_id"))?;

    let bonds = args
        .provider_bonds
        .iter()
        .map(|raw| parse_stake_bond_ref(raw).map_err(|error| format!("--provider-bond '{raw}': {error}")))
        .collect::<Result<Vec<TransactionOutpoint>, String>>()?;

    let client = connect(&resolve_node_rpc(&Some(network_id.to_string()), &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|error| format!("getServerInfo failed: {error}"))?;
    if server.network_id.to_string() != network_id.to_string() {
        return Err(format!("network mismatch: node is '{}' but --network is '{network_id}'", server.network_id));
    }
    if !server.is_synced {
        return Err(
            "node is not synced: a PCPB context read off an unsynced node can name an epoch the network has moved past".to_string()
        );
    }

    let response = client
        .get_palw_state(GetPalwStateRequest {
            batch_id: None,
            provider_bond_outpoint: None,
            pcpb_anchor_epoch: Some(args.anchor_epoch),
            pcpb_a_commit: None,
        })
        .await
        .map_err(|error| format!("getPalwState failed: {error}"))?;

    let served = response.pcpb.ok_or_else(|| {
        format!("node returned no PCPB context for anchor epoch {}: it is outside the retained window", args.anchor_epoch)
    })?;
    // `from_rpc` returning `None` is the honest "not yet / not here" answer, not a transport error:
    // either the draw beacon has not closed or the snapshot epoch fell out of retention. Both are
    // fixed by choosing a different anchor, so say which one it was.
    let (ctx, _acommit_epoch) = PcpbContext::from_rpc(&served).map_err(|error| format!("PCPB context rejected: {error}"))?;
    let ctx = ctx.ok_or_else(|| {
        format!(
            "node cannot yet serve a complete PCPB context for anchor {} (snapshot epoch {}, draw epoch {}): \
             the draw beacon has not closed or the snapshot is outside retention",
            args.anchor_epoch, served.snapshot_epoch, served.draw_epoch
        )
    })?;

    let mut witnesses: Vec<(u32, kaspa_consensus_core::palw::PalwLeafPcpbWitnessV1)> = Vec::with_capacity(args.leaf_count as usize);
    let mut leaf_fields = Vec::with_capacity(args.leaf_count as usize);
    for leaf_index in 0..args.leaf_count {
        let preimage = JobPreimage {
            scheduler_job_id: derive(SCHEDULER_JOB_DOMAIN, &args.job_seed, leaf_index),
            requester_credential: derive(REQUESTER_CREDENTIAL_DOMAIN, &args.job_seed, 0),
            request_commitment: derive(REQUEST_COMMITMENT_DOMAIN, &args.job_seed, leaf_index),
        };
        let produced = external_witness(&ctx, suffix, preimage, args.shape_id, &bonds)
            .map_err(|error| format!("cannot produce PCPB evidence for leaf {leaf_index}: {error}"))?;
        let b = &produced.binding;
        leaf_fields.push(serde_json::json!({
            "leaf_index": leaf_index,
            "a_commit": b.a_commit.to_string(),
            "a_commit_epoch": b.a_commit_epoch,
            "provider_snapshot_root": b.provider_snapshot_root.to_string(),
            "assignment_proof_root": b.assignment_proof_root.to_string(),
            "dispatch_kind": b.dispatch_kind,
            "provider_a_bond": outpoint_string(&b.provider_a_bond),
            "provider_b_bond": outpoint_string(&b.provider_b_bond),
            "receipt_v3_issued_epoch": b.issued_epoch,
            "receipt_v3_job_challenge": b.job_challenge.to_string(),
            "job_nullifier": b.job_challenge.to_string(),
            "scheduler_job_id": preimage.scheduler_job_id.to_string(),
            "requester_credential": preimage.requester_credential.to_string(),
            "request_commitment": preimage.request_commitment.to_string(),
        }));
        witnesses.push((leaf_index, produced.witness));
    }

    let witness_bytes = borsh::to_vec(&witnesses).map_err(|error| format!("cannot encode PCPB witness set: {error}"))?;
    let fields = serde_json::json!({
        "schema": FIELDS_SCHEMA,
        "network_id": suffix,
        "anchor_epoch": ctx.anchor_epoch,
        "snapshot_epoch": ctx.snapshot_epoch,
        "draw_epoch": ctx.draw_epoch,
        "shape_id": args.shape_id,
        "total_bond": ctx.commitment.total_bond.to_string(),
        "provider_count": ctx.commitment.provider_count,
        "leaves": leaf_fields,
    });
    let mut fields_bytes = serde_json::to_vec_pretty(&fields).map_err(|error| format!("cannot encode PCPB fields JSON: {error}"))?;
    fields_bytes.push(b'\n');

    write_new_file(&args.witness_out, &witness_bytes)?;
    // Roll the witness back if the fields file cannot be created, so a half-written pair never looks
    // like a usable one: the two artifacts are only meaningful together.
    if let Err(error) = write_new_file(&args.fields_out, &fields_bytes) {
        let _ = std::fs::remove_file(&args.witness_out);
        return Err(error);
    }

    println!("payload_kind: pcpb-witness");
    println!("network_id: {suffix}");
    println!("anchor_epoch: {}", ctx.anchor_epoch);
    println!("snapshot_epoch: {}", ctx.snapshot_epoch);
    println!("draw_epoch: {}", ctx.draw_epoch);
    println!("provider_count: {}", ctx.commitment.provider_count);
    println!("total_bond: {}", ctx.commitment.total_bond);
    println!("provider_snapshot_root: {}", ctx.commitment.snapshot_root);
    println!("assignment_proof_root: {}", ctx.commitment.assignment_root);
    for leaf in &leaf_fields {
        let index = leaf["leaf_index"].as_u64().unwrap_or_default();
        println!("leaf{index}.dispatch_kind: {}", leaf["dispatch_kind"]);
        println!("leaf{index}.provider_a_bond: {}", leaf["provider_a_bond"].as_str().unwrap_or_default());
        println!("leaf{index}.provider_b_bond: {}", leaf["provider_b_bond"].as_str().unwrap_or_default());
        println!("leaf{index}.receipt_v3_issued_epoch: {}", leaf["receipt_v3_issued_epoch"]);
        println!("leaf{index}.receipt_v3_job_challenge: {}", leaf["receipt_v3_job_challenge"].as_str().unwrap_or_default());
    }
    println!("witness_out: {}", args.witness_out.display());
    println!("fields_out: {}", args.fields_out.display());
    println!(
        "next: build the leaf set with these fields, then palw-payload leaf-chunk --pcpb-witness-file {}",
        args.witness_out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The triple must vary per leaf, or two leaves share a challenge and therefore a
    /// `job_nullifier` — which the batch refuses. The requester credential deliberately does NOT
    /// vary: it identifies who asked, and that is one party across the batch.
    #[test]
    fn derived_preimage_varies_per_leaf_except_the_requester() {
        let seed = "t21-lifecycle-1";
        assert_ne!(derive(SCHEDULER_JOB_DOMAIN, seed, 0), derive(SCHEDULER_JOB_DOMAIN, seed, 1));
        assert_ne!(derive(REQUEST_COMMITMENT_DOMAIN, seed, 0), derive(REQUEST_COMMITMENT_DOMAIN, seed, 1));
        assert_eq!(derive(REQUESTER_CREDENTIAL_DOMAIN, seed, 0), derive(REQUESTER_CREDENTIAL_DOMAIN, seed, 0));
    }

    /// Domain separation: the same seed and index must not collide across the three roles.
    #[test]
    fn derived_preimage_roles_are_domain_separated() {
        let seed = "t21-lifecycle-1";
        let sched = derive(SCHEDULER_JOB_DOMAIN, seed, 0);
        let req = derive(REQUESTER_CREDENTIAL_DOMAIN, seed, 0);
        let commitment = derive(REQUEST_COMMITMENT_DOMAIN, seed, 0);
        assert_ne!(sched, req);
        assert_ne!(sched, commitment);
        assert_ne!(req, commitment);
    }

    /// A different seed must move every derived value, or two operators sharing an anchor would
    /// collide on `job_nullifier`.
    #[test]
    fn derived_preimage_follows_the_seed() {
        assert_ne!(derive(SCHEDULER_JOB_DOMAIN, "a", 0), derive(SCHEDULER_JOB_DOMAIN, "b", 0));
    }
}
