//! Strict offline PALW manifest, leaf-chunk, audit-vote, and certificate artifacts.
//!
//! JSON files are operator interchange formats only. Transaction payload outputs are always the exact
//! raw Borsh bytes consumed by `palw-submit`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use borsh::BorshDeserialize;
use clap::Parser;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::dns_finality::{STAKE_VALIDATOR_PUBKEY_LEN, validator_id_from_pubkey};
use kaspa_consensus_core::palw::{
    PALW_AUDITOR_V2_MLDSA87_CONTEXT, PALW_MAX_OVERLAY_PAYLOAD_BYTES, PALW_MAX_PROVIDER_CAPACITY_ENTRIES_V1,
    PALW_MAX_PROVIDER_RUNTIME_CLASSES_V1, PalwAuditorVoteV2, PalwBatchCertificateV2, PalwBatchLifecycleV1, PalwBatchManifestV1,
    PalwBatchStatus, PalwLeafChunkV1, PalwProviderBondRecord, PalwPublicLeafV1, ProviderBondView,
    palw_audit_epoch_inclusion_window_epochs, palw_certificate_included_within_audit_window, palw_verify_leaf_membership,
    validate_palw_overlay_payload,
};
use kaspa_consensus_core::palw_audit::{MAX_PALW_AUDIT_FACT_PROVIDER_RECORDS, PalwAuditRoundFacts, derive_palw_audit_selection};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed, parse_stake_bond_ref};
use kaspa_rpc_core::{GetPalwAuditFactsRequest, api::rpc::RpcApi};
use kaspa_txscript::verify_mldsa87_with_context;
use misaka_palw_miner::audit::{
    AuditCertificate, AuditRound, Auditor, BATCH_CERTIFICATE_SUBNETWORK_BYTE, LeafAuditExecutor, QuorumPolicy, assemble_certificate,
    execute_audit_round, sign_vote,
};
use misaka_palw_miner::registration::{
    BATCH_MANIFEST_SUBNETWORK_BYTE, BatchPolicy, LEAF_CHUNK_SUBNETWORK_BYTE, build_batch_manifest, build_leaf_chunk,
    manifest_leaf_root, restamp_leaves,
};
use serde::{Deserialize, Serialize};

use super::{PalwArtifactNetwork, write_new_payload};
use crate::{connect, parse_hash64, resolve_node_rpc};

const LEAF_SET_SCHEMA: &str = "misaka.palw.leaf-set.v1";
const AUDIT_FACTS_SCHEMA: &str = "misaka.palw.audit-round-facts.v1";
const MAX_JSON_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;

/// Arguments for a content-addressed `0x31` manifest and the matching re-stamped leaves.
#[derive(Parser, Debug)]
pub(super) struct BatchManifestPayloadArgs {
    /// PALW-active preset whose admission policy fixes the manifest epoch windows.
    #[arg(long, value_enum, default_value = "testnet-110")]
    network: PalwArtifactNetwork,

    /// `misaka.palw.leaf-set.v1` JSON containing unbound (`batch_id == 0`) leaves in index order.
    #[arg(long)]
    leaves_file: PathBuf,

    /// Epoch in which the manifest transaction will be accepted. It must equal every leaf's
    /// `registered_epoch`; submit it during this epoch or rebuild the artifact.
    #[arg(long)]
    registration_epoch: u64,

    /// Commitment to the off-chain batch descriptor (128 hex characters).
    #[arg(long, value_parser = parse_hash64)]
    descriptor_root: Hash64,

    /// Commitment to the audit policy (128 hex characters).
    #[arg(long, value_parser = parse_hash64)]
    audit_policy_id: Hash64,

    /// New file to receive raw `PalwBatchManifestV1` Borsh bytes.
    #[arg(long)]
    out: PathBuf,

    /// New JSON file to receive the same leaves re-stamped with the content-derived batch id.
    #[arg(long)]
    restamped_leaves_out: PathBuf,
}

/// Arguments for one canonical `0x32` chunk from a complete, re-stamped leaf set.
#[derive(Parser, Debug)]
pub(super) struct LeafChunkPayloadArgs {
    #[arg(long, value_enum, default_value = "testnet-110")]
    network: PalwArtifactNetwork,

    /// Raw Borsh manifest emitted by `palw-payload batch-manifest`.
    #[arg(long)]
    manifest_file: PathBuf,

    /// D3-b: raw Borsh `Vec<(u32, PalwLeafPcpbWitnessV1)>` — one PCPB witness per leaf index,
    /// exported by the bridge's PCPB flow. A v3 chunk cannot be built without them: the acceptance
    /// arm verifies clauses 11/12 against these witnesses BEFORE storing any leaf.
    #[arg(long)]
    pcpb_witness_file: PathBuf,

    /// Re-stamped `misaka.palw.leaf-set.v1` JSON emitted alongside the manifest.
    #[arg(long)]
    leaves_file: PathBuf,

    /// Zero-based canonical chunk index.
    #[arg(long)]
    chunk_index: u16,

    /// New file to receive raw `PalwLeafChunkV1` Borsh bytes.
    #[arg(long)]
    out: PathBuf,
}

/// Arguments for exporting one complete node-derived audit snapshot.
#[derive(Parser, Debug)]
pub(super) struct AuditFactsPayloadArgs {
    #[arg(long, value_enum, default_value = "testnet-110")]
    network: PalwArtifactNetwork,

    /// Node wRPC (Borsh) endpoint. Omit to use the endpoint registry/network loopback default.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Content-derived batch id (128 hex characters).
    #[arg(long, value_parser = parse_hash64)]
    batch_id: Hash64,

    /// Audit beacon epoch whose predecessor seed selects the committee and leaf sample.
    #[arg(long)]
    audit_beacon_epoch: u64,

    /// New file to receive `misaka.palw.audit-round-facts.v1` JSON.
    #[arg(long)]
    out: PathBuf,
}

/// One leaf's outcome, as recorded by whatever actually re-executed it.
///
/// This file REPLACES a `--verdict pass|reject` flag plus a hand-typed
/// `--checked-leaf-bitmap-root`. Those two let an auditor sign a batch-wide verdict without
/// naming a single leaf, and let the bitmap commitment say anything at all; the vote was a real
/// ML-DSA-87 signature over an assertion nobody had to make good on.
///
/// The file must cover EXACTLY the beacon-selected sample: `execute_audit_round` re-derives that
/// sample and fails on any leaf this file omits, so the auditor cannot narrow what it answers for,
/// and the bitmap is derived from the sample rather than supplied beside it.
#[derive(Debug, Serialize, Deserialize)]
struct LeafVerdictEntry {
    leaf_index: u32,
    /// True when re-executing this leaf reproduced its on-chain commitments.
    reproduced: bool,
}

/// The per-leaf audit results file (`--leaf-verdicts`), bound to the batch it was produced for so
/// results cannot be carried across rounds.
#[derive(Debug, Serialize, Deserialize)]
struct LeafVerdictFile {
    batch_id: Hash64,
    audit_beacon_epoch: u64,
    leaves: Vec<LeafVerdictEntry>,
}

/// Answers from [`LeafVerdictFile`]. A beacon-selected leaf with no entry is INCONCLUSIVE, which
/// aborts the round — an auditor may not sign a verdict covering leaves it never reports on.
struct RecordedLeafAudit {
    results: std::collections::HashMap<u32, bool>,
}

impl LeafAuditExecutor for RecordedLeafAudit {
    fn audit_leaf(&mut self, leaf_index: u32, _leaf: &PalwPublicLeafV1) -> Result<bool, String> {
        self.results.get(&leaf_index).copied().ok_or_else(|| {
            format!("--leaf-verdicts has no entry for beacon-selected leaf {leaf_index}; the file must cover the whole sample")
        })
    }
}

/// Arguments for one independently signed auditor vote.
#[derive(Parser, Debug)]
pub(super) struct AuditVotePayloadArgs {
    #[arg(long, value_enum, default_value = "testnet-110")]
    network: PalwArtifactNetwork,

    /// This auditor's own synced node. Its frozen round inputs must equal `--facts-file`; a harmless
    /// later tip is allowed, but seed/selection/provider-view drift is not.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// `misaka.palw.audit-round-facts.v1` JSON exported by a node for this frozen audit round.
    #[arg(long)]
    facts_file: PathBuf,

    /// ML-DSA-87 seed belonging to the selected representative provider bond.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// Selected representative provider bond (`TXID:INDEX`) this key owns.
    #[arg(long, value_parser = parse_stake_bond_ref)]
    auditor_bond: TransactionOutpoint,

    /// `misaka.palw.leaf-verdicts.v1` JSON: one entry per BEACON-SELECTED leaf, recording whether
    /// re-executing it reproduced its on-chain commitments. The batch-wide verdict and the
    /// `checked_leaf_bitmap_root` are DERIVED from this — neither can be asserted directly.
    #[arg(long)]
    leaf_verdicts: PathBuf,

    /// Certificate-wide passed-leaf count this auditor independently approves (1..=manifest leaf count).
    #[arg(long)]
    passed_leaf_count: u32,

    /// Certificate-wide rejected-leaf bitmap commitment this auditor independently approves.
    #[arg(long, value_parser = parse_hash64)]
    rejected_leaf_bitmap_root: Hash64,

    /// New file to receive raw `PalwAuditorVoteV2` Borsh bytes.
    #[arg(long)]
    out: PathBuf,
}

/// Arguments for a canonical `0x33` stake-weighted quorum certificate.
#[derive(Parser, Debug)]
pub(super) struct AuditCertificatePayloadArgs {
    #[arg(long, value_enum, default_value = "testnet-110")]
    network: PalwArtifactNetwork,

    /// Synced node used to authenticate the frozen round and supply the fresh certificate epoch.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// The same frozen-round facts file every auditor used when signing.
    #[arg(long)]
    facts_file: PathBuf,

    /// Raw Borsh vote file. Repeat for each independently produced vote.
    #[arg(long = "vote-file", required = true)]
    vote_files: Vec<PathBuf>,

    /// New file to receive raw `PalwBatchCertificateV2` Borsh bytes.
    #[arg(long)]
    out: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeafSetFile {
    schema: String,
    leaves: Vec<PalwPublicLeafV1>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditFactsFile {
    schema: String,
    facts: PalwAuditRoundFacts,
}

pub(super) fn batch_manifest_payload(args: BatchManifestPayloadArgs) -> Result<(), String> {
    ensure_distinct_outputs(&args.out, &args.restamped_leaves_out)?;
    let leaves = read_leaf_set(&args.leaves_file)?;
    let (manifest, payload, restamped) =
        build_manifest_artifacts(args.network, leaves, args.registration_epoch, args.descriptor_root, args.audit_policy_id)?;
    let restamped_json = encode_leaf_set(&restamped)?;
    write_new_pair(&args.out, &payload, &args.restamped_leaves_out, &restamped_json)?;

    println!("payload_kind: batch-manifest");
    println!("payload_file: {}", args.out.display());
    println!("payload_bytes: {}", payload.len());
    println!("restamped_leaves_file: {}", args.restamped_leaves_out.display());
    println!("network: {}", args.network.network_id());
    println!("batch_id: {}", manifest.batch_id);
    println!("leaf_count: {}", manifest.leaf_count);
    println!("chunk_count: {}", manifest.chunk_count);
    println!("registration_epoch: {}", manifest.registration_epoch);
    println!("activation_not_before_epoch: {}", manifest.activation_not_before_epoch);
    println!("expiry_epoch: {}", manifest.expiry_epoch);
    println!("next: submit the manifest during registration_epoch, then build and submit every leaf chunk");
    Ok(())
}

pub(super) fn leaf_chunk_payload(args: LeafChunkPayloadArgs) -> Result<(), String> {
    let manifest: PalwBatchManifestV1 = read_borsh_file(&args.manifest_file, "manifest")?;
    let leaves = read_leaf_set(&args.leaves_file)?;
    let witnesses = read_pcpb_witness_set(&args.pcpb_witness_file)?;
    validate_manifest_and_leaves(args.network, &manifest, &leaves, Some(&witnesses))?;
    let (subnetwork_byte, payload) = build_leaf_chunk(manifest.batch_id, args.chunk_index, &leaves, &witnesses)
        .map_err(|err| format!("cannot build leaf chunk {}: {err}", args.chunk_index))?;
    if subnetwork_byte != LEAF_CHUNK_SUBNETWORK_BYTE {
        return Err(format!(
            "leaf-chunk constructor returned unexpected subnetwork byte 0x{subnetwork_byte:02x} (expected 0x{LEAF_CHUNK_SUBNETWORK_BYTE:02x})"
        ));
    }
    validate_palw_overlay_payload(subnetwork_byte, &payload)
        .map_err(|err| format!("built leaf-chunk payload failed consensus validation: {err}"))?;
    verify_chunk_membership(&manifest, &payload)?;
    write_new_payload(&args.out, &payload)?;

    let chunk: PalwLeafChunkV1 = decode_borsh(&payload, "built leaf chunk")?;
    println!("payload_kind: leaf-chunk");
    println!("payload_file: {}", args.out.display());
    println!("payload_bytes: {}", payload.len());
    println!("network: {}", args.network.network_id());
    println!("batch_id: {}", manifest.batch_id);
    println!("chunk_index: {}", chunk.chunk_index);
    println!("chunk_leaves: {}", chunk.leaves.len());
    println!("next: kaspa-pq-validator palw-submit --kind leaf-chunk --payload-file {} ...", args.out.display());
    Ok(())
}

pub(super) async fn audit_facts_payload(args: AuditFactsPayloadArgs) -> Result<(), String> {
    let facts = fetch_audit_facts(args.network, &args.node_rpc, args.batch_id, args.audit_beacon_epoch).await?;
    let bytes = encode_audit_facts(&facts)?;
    write_new_payload(&args.out, &bytes)?;

    println!("artifact_kind: audit-round-facts");
    println!("artifact_file: {}", args.out.display());
    println!("artifact_bytes: {}", bytes.len());
    println!("network: {}", args.network.network_id());
    println!("sink: {}", facts.sink);
    println!("sink_daa_score: {}", facts.sink_daa_score);
    println!("batch_id: {}", facts.batch_id);
    println!("audit_beacon_epoch: {}", facts.audit_beacon_epoch);
    println!("provider_bonds: {}", facts.selection.provider_bonds.len());
    println!("selected_auditors: {}", facts.selection.selected_auditors.len());
    println!("next: every auditor must authenticate this frozen round against its own synced node before signing");
    Ok(())
}

pub(super) async fn audit_vote_payload(args: AuditVotePayloadArgs) -> Result<(), String> {
    let facts = read_audit_facts(&args.facts_file, args.network)?;
    let live = fetch_matching_live_facts(args.network, &args.node_rpc, &facts).await?;
    let mut seed = load_validator_seed(&args.validator_key)?;
    let key = ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);

    let vote = build_audit_vote(
        args.network,
        &facts,
        key,
        args.auditor_bond,
        &args.leaf_verdicts,
        args.passed_leaf_count,
        args.rejected_leaf_bitmap_root,
    )?;
    let payload = borsh::to_vec(&vote).map_err(|err| format!("cannot encode auditor vote: {err}"))?;
    write_new_payload(&args.out, &payload)?;

    println!("artifact_kind: audit-vote");
    println!("artifact_file: {}", args.out.display());
    println!("artifact_bytes: {}", payload.len());
    println!("network: {}", args.network.network_id());
    println!("facts_sink: {}", facts.sink);
    println!("live_sink: {}", live.sink);
    println!("batch_id: {}", facts.batch_id);
    println!("audit_beacon_epoch: {}", facts.audit_beacon_epoch);
    println!("auditor_bond: {}", args.auditor_bond);
    // The verdict is printed by `build_audit_vote` as it is DERIVED, alongside the leaves it
    // covers — there is no argument left to echo here.
    println!("passed_leaf_count: {}", vote.passed_leaf_count);
    println!("rejected_leaf_bitmap_root: {}", vote.rejected_leaf_bitmap_root);
    println!("next: transfer this vote to the certificate assembler without modifying its facts snapshot");
    Ok(())
}

pub(super) async fn audit_certificate_payload(args: AuditCertificatePayloadArgs) -> Result<(), String> {
    let file_facts = read_audit_facts(&args.facts_file, args.network)?;
    let facts = fetch_matching_live_facts(args.network, &args.node_rpc, &file_facts).await?;
    let votes = args
        .vote_files
        .iter()
        .map(|path| read_borsh_file::<PalwAuditorVoteV2>(path, "auditor vote"))
        .collect::<Result<Vec<_>, _>>()?;
    let certificate = build_audit_certificate(args.network, &facts, votes)?;
    write_new_payload(&args.out, &certificate.payload)?;

    println!("payload_kind: certificate");
    println!("payload_file: {}", args.out.display());
    println!("payload_bytes: {}", certificate.payload.len());
    println!("network: {}", args.network.network_id());
    println!("facts_sink: {}", file_facts.sink);
    println!("live_sink: {}", facts.sink);
    println!("batch_id: {}", certificate.cert.batch_id);
    println!("certificate_hash: {}", certificate.cert.hash());
    println!("certificate_epoch: {}", certificate.cert.certificate_epoch);
    println!("vote_count: {}", certificate.cert.votes.len());
    println!("approving_stake: {}", certificate.cert.approving_stake);
    println!("passed_leaf_count: {}", certificate.cert.passed_leaf_count);
    println!("rejected_leaf_bitmap_root: {}", certificate.cert.rejected_leaf_bitmap_root);
    println!("selected_total_stake: {}", facts.selection.selected_total_stake);
    println!("next: submit promptly as kind certificate; assembly used the node's fresh live inclusion epoch");
    Ok(())
}

fn build_manifest_artifacts(
    network: PalwArtifactNetwork,
    leaves: Vec<PalwPublicLeafV1>,
    registration_epoch: u64,
    descriptor_root: Hash64,
    audit_policy_id: Hash64,
) -> Result<(PalwBatchManifestV1, Vec<u8>, Vec<PalwPublicLeafV1>), String> {
    let first = leaves.first().ok_or_else(|| "leaf set is empty".to_string())?;
    if leaves.iter().any(|leaf| leaf.batch_id != Hash64::default()) {
        return Err("input leaves must be unbound (every batch_id must be zero); use the separate restamped output for chunks".into());
    }
    if leaves.iter().any(|leaf| leaf.leaf_index as usize >= leaves.len())
        || leaves.iter().enumerate().any(|(position, leaf)| leaf.leaf_index != position as u32)
    {
        return Err("input leaves must be in canonical contiguous leaf_index order 0..leaf_count".into());
    }
    if leaves.iter().any(|leaf| leaf.registered_epoch != registration_epoch) {
        return Err(format!(
            "every leaf registered_epoch must equal --registration-epoch {registration_epoch}; rebuild/re-aim the leaves before constructing a manifest"
        ));
    }
    if leaves.iter().any(|leaf| leaf.model_profile_id != first.model_profile_id) {
        return Err("all leaves in one manifest must share model_profile_id".into());
    }
    if leaves.iter().any(|leaf| leaf.runtime_class_id != first.runtime_class_id) {
        return Err("all leaves in one manifest must share runtime_class_id".into());
    }
    let total_leaf_bond_sompi = checked_leaf_bond_sum(&leaves)?;
    let params = Params::from(network.network_id());
    let admission = params.palw_batch_admission;
    let policy = BatchPolicy {
        registration_epoch,
        registration_lead_epochs: admission.registration_lead_epochs,
        audit_window_epochs: admission.audit_window_epochs,
        active_window_epochs: admission.active_window_epochs,
        min_leaf_bond_sompi: admission.min_leaf_bond_sompi,
        max_batch_leaves: admission.max_batch_leaves,
    };
    let (_batch_id, (subnetwork_byte, payload)) = build_batch_manifest(
        &leaves,
        first.model_profile_id,
        first.runtime_class_id,
        descriptor_root,
        audit_policy_id,
        total_leaf_bond_sompi,
        &policy,
    )
    .map_err(|err| format!("cannot build batch manifest: {err}"))?;
    if subnetwork_byte != BATCH_MANIFEST_SUBNETWORK_BYTE {
        return Err(format!(
            "manifest constructor returned unexpected subnetwork byte 0x{subnetwork_byte:02x} (expected 0x{BATCH_MANIFEST_SUBNETWORK_BYTE:02x})"
        ));
    }
    validate_palw_overlay_payload(subnetwork_byte, &payload)
        .map_err(|err| format!("built manifest payload failed consensus validation: {err}"))?;
    let manifest: PalwBatchManifestV1 = decode_borsh(&payload, "built manifest")?;
    let restamped = restamp_leaves(manifest.batch_id, &leaves);
    validate_manifest_and_leaves(network, &manifest, &restamped, None)?;
    Ok((manifest, payload, restamped))
}

fn validate_manifest_and_leaves(
    network: PalwArtifactNetwork,
    manifest: &PalwBatchManifestV1,
    leaves: &[PalwPublicLeafV1],
    // D3-b: chunk reconstruction needs the per-leaf PCPB witnesses (a v3 chunk carries them).
    // Manifest-time callers run before the bridge has assembled evidence and pass `None`, which
    // skips ONLY the chunk-reconstruction preflight — the real chunk is still fully validated at
    // build time (`leaf_chunk_payload`) and at consensus acceptance.
    pcpb_witnesses: Option<&std::collections::BTreeMap<u32, kaspa_consensus_core::palw::PalwLeafPcpbWitnessV1>>,
) -> Result<(), String> {
    let manifest_payload = borsh::to_vec(manifest).map_err(|err| format!("cannot encode manifest for validation: {err}"))?;
    validate_palw_overlay_payload(BATCH_MANIFEST_SUBNETWORK_BYTE, &manifest_payload)
        .map_err(|err| format!("manifest failed consensus validation: {err}"))?;
    if !manifest.batch_id_is_content_derived() {
        return Err("manifest batch_id is not its content-derived id".into());
    }
    let params = Params::from(network.network_id());
    let admission = params.palw_batch_admission;
    if !manifest.admission_valid(
        manifest.registration_epoch,
        admission.max_batch_leaves,
        admission.max_leaf_chunk_leaves,
        admission.registration_lead_epochs,
        admission.active_window_epochs,
        admission.audit_window_epochs,
        admission.min_leaf_bond_sompi,
    ) {
        return Err(format!("manifest is not admissible under {} consensus parameters", network.network_id()));
    }
    if leaves.len() != manifest.leaf_count as usize {
        return Err(format!("leaf set has {} leaves but manifest fixes {}", leaves.len(), manifest.leaf_count));
    }
    for (position, leaf) in leaves.iter().enumerate() {
        if leaf.leaf_index != position as u32 {
            return Err(format!("leaf set is not canonical: position {position} carries leaf_index {}", leaf.leaf_index));
        }
        if leaf.batch_id != manifest.batch_id {
            return Err(format!("leaf {} does not carry manifest batch_id", leaf.leaf_index));
        }
        if leaf.model_profile_id != manifest.model_profile_id || leaf.runtime_class_id != manifest.runtime_class_id {
            return Err(format!("leaf {} model/runtime class differs from manifest", leaf.leaf_index));
        }
        if leaf.registered_epoch != manifest.registration_epoch
            || leaf.activation_epoch != manifest.activation_not_before_epoch
            || leaf.expiry_epoch != manifest.expiry_epoch
        {
            return Err(format!("leaf {} lifecycle epochs differ from manifest", leaf.leaf_index));
        }
    }
    if checked_leaf_bond_sum(leaves)? != manifest.total_leaf_bond_sompi {
        return Err("sum of leaf_bond_sompi does not equal manifest total_leaf_bond_sompi".into());
    }
    let root = manifest_leaf_root(leaves).map_err(|err| format!("cannot derive manifest leaf root: {err}"))?;
    if root != manifest.leaf_root {
        return Err("leaf set does not open to manifest leaf_root".into());
    }
    let Some(witnesses) = pcpb_witnesses else {
        return Ok(());
    };
    for chunk_index in 0..manifest.chunk_count {
        let (subnetwork_byte, payload) = build_leaf_chunk(manifest.batch_id, chunk_index, leaves, witnesses)
            .map_err(|err| format!("cannot reconstruct leaf chunk {chunk_index}: {err}"))?;
        if subnetwork_byte != LEAF_CHUNK_SUBNETWORK_BYTE {
            return Err(format!("chunk {chunk_index} constructor returned unexpected subnetwork byte 0x{subnetwork_byte:02x}"));
        }
        validate_palw_overlay_payload(subnetwork_byte, &payload)
            .map_err(|err| format!("reconstructed leaf chunk {chunk_index} failed consensus validation: {err}"))?;
        verify_chunk_membership(manifest, &payload)?;
    }
    Ok(())
}

fn verify_chunk_membership(manifest: &PalwBatchManifestV1, payload: &[u8]) -> Result<(), String> {
    let chunk: PalwLeafChunkV1 = decode_borsh(payload, "leaf chunk")?;
    if chunk.batch_id != manifest.batch_id || chunk.chunk_index >= manifest.chunk_count {
        return Err("leaf chunk does not belong to the supplied manifest".into());
    }
    for (leaf, proof) in chunk.leaves.iter().zip(&chunk.proofs) {
        let mut projected = leaf.clone();
        projected.batch_id = Hash64::default();
        if !palw_verify_leaf_membership(&projected.leaf_hash(), leaf.leaf_index, manifest.leaf_count, proof, &manifest.leaf_root) {
            return Err(format!("leaf {} membership proof does not open to manifest leaf_root", leaf.leaf_index));
        }
    }
    Ok(())
}

fn build_audit_vote(
    network: PalwArtifactNetwork,
    facts: &PalwAuditRoundFacts,
    key: ValidatorKey,
    auditor_bond: TransactionOutpoint,
    leaf_verdicts: &Path,
    passed_leaf_count: u32,
    rejected_leaf_bitmap_root: Hash64,
) -> Result<PalwAuditorVoteV2, String> {
    validate_audit_facts(network, facts)?;
    if passed_leaf_count == 0 || passed_leaf_count > facts.manifest.leaf_count {
        return Err(format!("--passed-leaf-count must be in 1..={}, got {passed_leaf_count}", facts.manifest.leaf_count));
    }
    let selected = facts
        .selection
        .selected_credential_stakes
        .iter()
        .find(|member| member.representative == auditor_bond)
        .ok_or_else(|| format!("--auditor-bond {auditor_bond} is outside the beacon-selected representative slate"))?;
    let record = selected_record(facts, &auditor_bond)?;
    if selected.credential != key.validator_id
        || record.owner_pubkey_hash != key.validator_id
        || record.owner_public_key.as_slice() != key.public_key()
    {
        return Err(format!("validator key does not own selected representative bond {auditor_bond}"));
    }
    // AUDIT-EXEC-01: the verdict is the OUTPUT of running the round over the beacon-selected
    // sample, never an input beside it. `execute_audit_round` re-derives that sample with the same
    // primitive consensus uses, so this file cannot answer for a sample of the auditor's choosing.
    let text = std::fs::read_to_string(leaf_verdicts).map_err(|e| format!("read {}: {e}", leaf_verdicts.display()))?;
    let recorded: LeafVerdictFile =
        serde_json::from_str(&text).map_err(|e| format!("parse leaf verdicts {}: {e}", leaf_verdicts.display()))?;
    if recorded.batch_id != facts.batch_id {
        return Err(format!("--leaf-verdicts is for batch {}, this round is {}", recorded.batch_id, facts.batch_id));
    }
    if recorded.audit_beacon_epoch != facts.audit_beacon_epoch {
        return Err(format!(
            "--leaf-verdicts is for audit beacon epoch {}, this round is {}",
            recorded.audit_beacon_epoch, facts.audit_beacon_epoch
        ));
    }
    let mut executor =
        RecordedLeafAudit { results: recorded.leaves.iter().map(|entry| (entry.leaf_index, entry.reproduced)).collect() };
    let verdict =
        execute_audit_round(&facts.previous_epoch_seed, &facts.batch_id, &facts.leaves, facts.sample_size as u32, &mut executor)
            .map_err(|e| format!("audit round could not be executed: {e}"))?;
    println!("audited_leaves: {:?}", verdict.checked_leaf_indices());
    println!("rejected_leaves: {:?}", verdict.rejected_leaf_indices());
    println!("verdict: {}", if verdict.passes() { "pass" } else { "reject" });
    println!("checked_leaf_bitmap_root: {}", verdict.checked_leaf_bitmap_root());

    let round = audit_round(facts, passed_leaf_count, rejected_leaf_bitmap_root);
    let vote = sign_vote(&round, &Auditor { key, bond: auditor_bond }, &verdict);
    verify_vote_signature(facts, &vote)?;
    Ok(vote)
}

fn build_audit_certificate(
    network: PalwArtifactNetwork,
    facts: &PalwAuditRoundFacts,
    votes: Vec<PalwAuditorVoteV2>,
) -> Result<AuditCertificate, String> {
    validate_audit_facts(network, facts)?;
    let first_vote = votes.first().ok_or_else(|| "a certificate needs at least one auditor vote".to_string())?;
    let passed_leaf_count = first_vote.passed_leaf_count;
    let rejected_leaf_bitmap_root = first_vote.rejected_leaf_bitmap_root;
    if passed_leaf_count == 0 || passed_leaf_count > facts.manifest.leaf_count {
        return Err(format!("signed passed_leaf_count must be in 1..={}, got {passed_leaf_count}", facts.manifest.leaf_count));
    }
    let params = Params::from(network.network_id());
    if !params.palw_spam.is_inert()
        && (passed_leaf_count != facts.manifest.leaf_count || rejected_leaf_bitmap_root != Hash64::default())
    {
        return Err(
            "Header-v4 certificates must use the canonical all-pass summary: passed_leaf_count == manifest.leaf_count and rejected_leaf_bitmap_root == 0; partial-pass certificates need a future witness-bearing version"
                .to_string(),
        );
    }
    for vote in &votes {
        verify_vote_signature(facts, vote)?;
    }
    let round = audit_round(facts, passed_leaf_count, rejected_leaf_bitmap_root);
    let certificate = assemble_certificate(
        &round,
        votes,
        &facts.selection.selected_credential_stakes,
        QuorumPolicy { num: facts.quorum_num, den: facts.quorum_den },
    )
    .map_err(|err| format!("cannot assemble audit certificate: {err}"))?;
    if certificate.subnetwork_byte != BATCH_CERTIFICATE_SUBNETWORK_BYTE {
        return Err(format!(
            "certificate constructor returned unexpected subnetwork byte 0x{:02x} (expected 0x{BATCH_CERTIFICATE_SUBNETWORK_BYTE:02x})",
            certificate.subnetwork_byte
        ));
    }
    validate_palw_overlay_payload(certificate.subnetwork_byte, &certificate.payload)
        .map_err(|err| format!("built certificate payload failed consensus validation: {err}"))?;
    let decoded: PalwBatchCertificateV2 = decode_borsh(&certificate.payload, "built certificate")?;
    if decoded != certificate.cert {
        return Err("certificate constructor produced a non-round-tripping Borsh payload".into());
    }
    Ok(certificate)
}

fn verify_vote_signature(facts: &PalwAuditRoundFacts, vote: &PalwAuditorVoteV2) -> Result<(), String> {
    if vote.vote > 1 {
        return Err(format!("vote from {} has invalid verdict byte {}", vote.bond_outpoint, vote.vote));
    }
    let record = selected_record(facts, &vote.bond_outpoint)?;
    let digest = vote.signing_hash(facts.network_id, &facts.batch_id, facts.audit_beacon_epoch, &facts.selection.audit_sample_root);
    match verify_mldsa87_with_context(&record.owner_public_key, &digest.as_bytes(), &vote.signature, PALW_AUDITOR_V2_MLDSA87_CONTEXT) {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!("vote from {} has an invalid ML-DSA-87 signature", vote.bond_outpoint)),
        Err(err) => Err(format!("cannot verify vote from {}: {err}", vote.bond_outpoint)),
    }
}

fn selected_record<'a>(facts: &'a PalwAuditRoundFacts, bond: &TransactionOutpoint) -> Result<&'a PalwProviderBondRecord, String> {
    facts
        .selection
        .selected_auditors
        .iter()
        .find(|record| record.bond_outpoint == *bond)
        .ok_or_else(|| format!("vote names bond {bond} outside the beacon-selected representative slate"))
}

fn audit_round(facts: &PalwAuditRoundFacts, passed_leaf_count: u32, rejected_leaf_bitmap_root: Hash64) -> AuditRound {
    AuditRound {
        network_id: facts.network_id,
        batch_id: facts.batch_id,
        manifest_hash: facts.manifest_hash,
        leaf_root: facts.manifest.leaf_root,
        audit_beacon_epoch: facts.audit_beacon_epoch,
        audit_sample_root: facts.selection.audit_sample_root,
        passed_leaf_count,
        rejected_leaf_bitmap_root,
        certificate_epoch: facts.inclusion_epoch,
        activation_epoch: facts.manifest.activation_not_before_epoch,
        expiry_epoch: facts.manifest.expiry_epoch,
        auditor_set_commitment: facts.selection.auditor_set_commitment,
    }
}

fn validate_audit_facts(network: PalwArtifactNetwork, facts: &PalwAuditRoundFacts) -> Result<(), String> {
    let params = Params::from(network.network_id());
    let expected_network_id = network.network_id().suffix().ok_or_else(|| "PALW artifact network has no suffix".to_string())?;
    if facts.network_id != expected_network_id {
        return Err(format!(
            "facts network_id {} does not match selected {} suffix {expected_network_id}",
            facts.network_id,
            network.network_id()
        ));
    }
    let epoch_length = params.palw_epoch_length_daa.max(1);
    if facts.inclusion_epoch != facts.sink_daa_score / epoch_length {
        return Err(
            "facts proposed carrier inclusion_epoch is not derived from sink_daa_score and the selected network epoch length".into()
        );
    }
    if facts.batch_id != facts.manifest.batch_id || facts.manifest_hash != facts.manifest.content_id() {
        return Err("facts batch_id/manifest_hash do not bind to the supplied manifest".into());
    }
    validate_manifest_and_leaves(network, &facts.manifest, &facts.leaves, None)?;
    validate_lifecycle(&facts.manifest, &facts.lifecycle)?;

    let admission = params.palw_batch_admission;
    let expected_window = palw_audit_epoch_inclusion_window_epochs(&admission);
    if facts.inclusion_window_epochs != expected_window
        || facts.committee_size != params.palw_audit_committee_size
        || facts.sample_size != params.palw_audit_sample_size
        || facts.quorum_num != params.palw_audit_quorum_num
        || facts.quorum_den != params.palw_audit_quorum_den
    {
        return Err("facts audit window/committee/sample/quorum parameters do not match the selected network".into());
    }
    if facts.quorum_den == 0 || facts.quorum_num == 0 || facts.quorum_num > facts.quorum_den {
        return Err("facts carry a degenerate audit quorum".into());
    }
    if facts.audit_beacon_epoch < facts.manifest.registration_epoch
        || facts.audit_beacon_epoch >= facts.manifest.activation_not_before_epoch
    {
        return Err("facts audit_beacon_epoch is outside the manifest audit interval".into());
    }
    if !palw_certificate_included_within_audit_window(facts.audit_beacon_epoch, facts.inclusion_epoch, facts.inclusion_window_epochs) {
        return Err("facts sink is outside the certificate inclusion window".into());
    }
    let expected_snapshot_daa = facts.audit_beacon_epoch.saturating_mul(epoch_length);
    if facts.snapshot_daa_score != expected_snapshot_daa {
        return Err("facts snapshot_daa_score is not the start of audit_beacon_epoch".into());
    }
    if facts.previous_epoch_seed == Hash64::default() {
        return Err("facts previous_epoch_seed is zero; the audit seed is not safely resolved".into());
    }

    let mut seen = HashSet::with_capacity(facts.selection.provider_bonds.len());
    if facts.selection.provider_bonds.len() > MAX_PALW_AUDIT_FACT_PROVIDER_RECORDS {
        return Err(format!(
            "facts contain {} provider records, above the {}-record audit-facts bound",
            facts.selection.provider_bonds.len(),
            MAX_PALW_AUDIT_FACT_PROVIDER_RECORDS
        ));
    }
    let mut previous: Option<&TransactionOutpoint> = None;
    for record in &facts.selection.provider_bonds {
        validate_provider_record(record, admission.min_provider_bond_sompi, admission.provider_unbond_floor_epochs)?;
        if !seen.insert(record.bond_outpoint) {
            return Err(format!("facts repeat provider bond {}", record.bond_outpoint));
        }
        if previous.is_some_and(|prior| compare_outpoint(prior, &record.bond_outpoint).is_ge()) {
            return Err("facts provider_bonds are not in strict canonical outpoint order".into());
        }
        previous = Some(&record.bond_outpoint);
    }
    let provider_view =
        ProviderBondView::from_records(facts.selection.provider_bonds.iter().cloned().map(|record| (record.bond_outpoint, record)));
    let derived = derive_palw_audit_selection(
        &facts.previous_epoch_seed,
        &facts.batch_id,
        &provider_view,
        facts.snapshot_daa_score,
        &facts.leaves,
        facts.committee_size as usize,
        facts.sample_size as u32,
    )
    .map_err(|err| format!("cannot rederive audit selection from facts: {err}"))?;
    if derived != facts.selection {
        return Err("facts committee/sample selection does not match consensus re-derivation".into());
    }
    if derived.selected_credential_stakes.is_empty() {
        return Err("facts select no eligible auditors; a certificate cannot be assembled".into());
    }
    Ok(())
}

fn validate_lifecycle(manifest: &PalwBatchManifestV1, lifecycle: &PalwBatchLifecycleV1) -> Result<(), String> {
    if !matches!(lifecycle.status, PalwBatchStatus::Committed | PalwBatchStatus::Auditing) {
        return Err(format!("batch lifecycle is not auditable: {:?}", lifecycle.status));
    }
    if lifecycle.registration_epoch != manifest.registration_epoch
        || lifecycle.activation_not_before_epoch != manifest.activation_not_before_epoch
        || lifecycle.expiry_epoch != manifest.expiry_epoch
        || lifecycle.leaf_count != manifest.leaf_count
        || lifecycle.chunk_count != manifest.chunk_count
        || lifecycle.leaf_root != manifest.leaf_root
    {
        return Err("facts lifecycle does not match the manifest".into());
    }
    for bit in 0..256usize {
        let present = (lifecycle.chunks_present[bit / 64] >> (bit % 64)) & 1 == 1;
        if present != (bit < manifest.chunk_count as usize) {
            return Err("facts lifecycle chunk bitmap is not exactly complete for the manifest".into());
        }
    }
    if lifecycle.cert_hash.is_some()
        || lifecycle.cert_activation_epoch != 0
        || lifecycle.cert_expiry_epoch != 0
        || lifecycle.cert_approving_stake != 0
        || lifecycle.first_cert_daa.is_some()
        || lifecycle.revoked_from_daa.is_some()
    {
        return Err("facts lifecycle already carries certificate/revocation state and is not a fresh auditable round".into());
    }
    Ok(())
}

fn validate_provider_record(record: &PalwProviderBondRecord, amount_floor: u64, unbond_delay_floor: u64) -> Result<(), String> {
    if record.version != 1 {
        return Err(format!("provider bond {} has unsupported version {}", record.bond_outpoint, record.version));
    }
    if record.owner_public_key.len() != STAKE_VALIDATOR_PUBKEY_LEN
        || validator_id_from_pubkey(&record.owner_public_key) != record.owner_pubkey_hash
    {
        return Err(format!("provider bond {} has an invalid owner key binding", record.bond_outpoint));
    }
    if record.amount_sompi < amount_floor {
        return Err(format!("provider bond {} is below the network collateral floor", record.bond_outpoint));
    }
    if record.activation_daa_score != record.created_daa_score
        || record.unbond_delay_epochs < unbond_delay_floor
        || record.unbond_request_daa_score.is_some_and(|daa| daa < record.created_daa_score)
        || record.slashed_at_daa_score.is_some_and(|daa| daa < record.created_daa_score)
    {
        return Err(format!("provider bond {} has impossible lifecycle metadata", record.bond_outpoint));
    }
    if record.runtime_classes.is_empty()
        || record.runtime_classes.len() > PALW_MAX_PROVIDER_RUNTIME_CLASSES_V1
        || !record.runtime_classes.windows(2).all(|pair| pair[0].as_byte_slice() < pair[1].as_byte_slice())
    {
        return Err(format!("provider bond {} has non-canonical runtime classes", record.bond_outpoint));
    }
    if record.capacity_by_shape.is_empty()
        || record.capacity_by_shape.len() > PALW_MAX_PROVIDER_CAPACITY_ENTRIES_V1
        || record.capacity_by_shape.iter().any(|(_, capacity)| *capacity == 0)
        || !record.capacity_by_shape.windows(2).all(|pair| pair[0].0 < pair[1].0)
    {
        return Err(format!("provider bond {} has non-canonical capacities", record.bond_outpoint));
    }
    Ok(())
}

fn checked_leaf_bond_sum(leaves: &[PalwPublicLeafV1]) -> Result<u64, String> {
    leaves.iter().try_fold(0u64, |sum, leaf| {
        sum.checked_add(leaf.leaf_bond_sompi).ok_or_else(|| "sum of leaf_bond_sompi overflows u64".to_string())
    })
}

fn compare_outpoint(a: &TransactionOutpoint, b: &TransactionOutpoint) -> std::cmp::Ordering {
    a.transaction_id.as_byte_slice().cmp(b.transaction_id.as_byte_slice()).then(a.index.cmp(&b.index))
}

/// D3-b: read the bridge-exported PCPB witness set — raw Borsh `Vec<(u32, witness)>`, strict
/// (trailing bytes rejected), duplicate leaf indices rejected.
fn read_pcpb_witness_set(
    path: &Path,
) -> Result<std::collections::BTreeMap<u32, kaspa_consensus_core::palw::PalwLeafPcpbWitnessV1>, String> {
    let bytes = read_limited(path, "PCPB witness set", MAX_JSON_ARTIFACT_BYTES)?;
    let rows: Vec<(u32, kaspa_consensus_core::palw::PalwLeafPcpbWitnessV1)> =
        borsh::from_slice(&bytes).map_err(|err| format!("cannot decode PCPB witness set '{}': {err}", path.display()))?;
    let mut map = std::collections::BTreeMap::new();
    for (leaf_index, witness) in rows {
        if map.insert(leaf_index, witness).is_some() {
            return Err(format!("PCPB witness set '{}' has duplicate leaf index {leaf_index}", path.display()));
        }
    }
    Ok(map)
}

fn read_leaf_set(path: &Path) -> Result<Vec<PalwPublicLeafV1>, String> {
    let bytes = read_limited(path, "leaf-set JSON", MAX_JSON_ARTIFACT_BYTES)?;
    let file: LeafSetFile =
        serde_json::from_slice(&bytes).map_err(|err| format!("cannot decode leaf-set JSON '{}': {err}", path.display()))?;
    if file.schema != LEAF_SET_SCHEMA {
        return Err(format!("leaf-set JSON '{}' has schema '{}', expected '{LEAF_SET_SCHEMA}'", path.display(), file.schema));
    }
    Ok(file.leaves)
}

fn encode_leaf_set(leaves: &[PalwPublicLeafV1]) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(&LeafSetFile { schema: LEAF_SET_SCHEMA.to_string(), leaves: leaves.to_vec() })
        .map_err(|err| format!("cannot encode restamped leaf-set JSON: {err}"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn encode_audit_facts(facts: &PalwAuditRoundFacts) -> Result<Vec<u8>, String> {
    // Keep the complete provider snapshot compact. Pretty-printing ML-DSA public-key byte arrays can
    // inflate an otherwise valid, server-bounded response beyond the operator artifact bound.
    let mut bytes = serde_json::to_vec(&AuditFactsFile { schema: AUDIT_FACTS_SCHEMA.to_string(), facts: facts.clone() })
        .map_err(|err| format!("cannot encode audit-facts JSON: {err}"))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_JSON_ARTIFACT_BYTES {
        return Err(format!("audit-facts JSON is {} bytes, above the {MAX_JSON_ARTIFACT_BYTES}-byte artifact bound", bytes.len()));
    }
    Ok(bytes)
}

async fn fetch_audit_facts(
    network: PalwArtifactNetwork,
    node_rpc: &Option<String>,
    batch_id: Hash64,
    audit_beacon_epoch: u64,
) -> Result<PalwAuditRoundFacts, String> {
    let expected_network = network.network_id().to_string();
    let endpoint = resolve_node_rpc(&Some(expected_network.clone()), node_rpc);
    let client = connect(&endpoint).await?;
    let result = async {
        let server = client.get_server_info().await.map_err(|err| format!("getServerInfo failed: {err}"))?;
        if server.network_id.to_string() != expected_network {
            return Err(format!(
                "network mismatch: node is '{}' but PALW artifact network is '{expected_network}'",
                server.network_id
            ));
        }
        if !server.is_synced {
            return Err("node is not synced; refusing to export or authenticate auditor-selection facts".to_string());
        }
        let response = client
            .get_palw_audit_facts(GetPalwAuditFactsRequest { batch_id: batch_id.to_string(), audit_beacon_epoch })
            .await
            .map_err(|err| format!("getPalwAuditFacts failed: {err}"))?;
        if response.facts_json.len() as u64 > MAX_JSON_ARTIFACT_BYTES {
            return Err(format!(
                "node returned {} bytes of PALW audit facts, above the {MAX_JSON_ARTIFACT_BYTES}-byte client bound",
                response.facts_json.len()
            ));
        }
        let facts: PalwAuditRoundFacts = serde_json::from_str(&response.facts_json)
            .map_err(|err| format!("node returned malformed PALW audit facts JSON: {err}"))?;
        validate_audit_facts(network, &facts)?;
        if facts.batch_id != batch_id || facts.audit_beacon_epoch != audit_beacon_epoch {
            return Err("node returned PALW audit facts for a different requested round".to_string());
        }
        Ok(facts)
    }
    .await;
    let _ = client.disconnect().await;
    result
}

async fn fetch_matching_live_facts(
    network: PalwArtifactNetwork,
    node_rpc: &Option<String>,
    file_facts: &PalwAuditRoundFacts,
) -> Result<PalwAuditRoundFacts, String> {
    let live = fetch_audit_facts(network, node_rpc, file_facts.batch_id, file_facts.audit_beacon_epoch).await?;
    ensure_frozen_round_matches(file_facts, &live)?;
    Ok(live)
}

fn frozen_round_projection(facts: &PalwAuditRoundFacts) -> PalwAuditRoundFacts {
    let mut projected = facts.clone();
    // These fields are the freshness cursor, not vote-signing inputs. The certificate assembler uses
    // the freshly fetched inclusion epoch; a harmless tip advance must not invalidate an audit that
    // still has the identical frozen seed, manifest, leaves, provider projection and selection.
    projected.sink = Hash64::default();
    projected.sink_daa_score = 0;
    projected.inclusion_epoch = 0;
    // Epoch advancement may move a complete pre-certificate batch from Committed to Auditing. Both
    // states pass `validate_lifecycle` and carry the same immutable round content.
    projected.lifecycle.status = PalwBatchStatus::Committed;
    projected
}

fn ensure_frozen_round_matches(file_facts: &PalwAuditRoundFacts, live_facts: &PalwAuditRoundFacts) -> Result<(), String> {
    if frozen_round_projection(live_facts) != frozen_round_projection(file_facts) {
        return Err(
            "facts file does not match this operator's own synced node for the frozen audit round; the seed/fork, manifest, leaves, selection-relevant provider view, parameters, committee, or sample changed"
                .to_string(),
        );
    }
    Ok(())
}

fn read_audit_facts(path: &Path, network: PalwArtifactNetwork) -> Result<PalwAuditRoundFacts, String> {
    let bytes = read_limited(path, "audit-facts JSON", MAX_JSON_ARTIFACT_BYTES)?;
    let file: AuditFactsFile =
        serde_json::from_slice(&bytes).map_err(|err| format!("cannot decode audit-facts JSON '{}': {err}", path.display()))?;
    if file.schema != AUDIT_FACTS_SCHEMA {
        return Err(format!("audit-facts JSON '{}' has schema '{}', expected '{AUDIT_FACTS_SCHEMA}'", path.display(), file.schema));
    }
    validate_audit_facts(network, &file.facts)?;
    Ok(file.facts)
}

fn read_borsh_file<T: BorshDeserialize>(path: &Path, label: &str) -> Result<T, String> {
    let bytes = read_limited(path, label, PALW_MAX_OVERLAY_PAYLOAD_BYTES as u64)?;
    decode_borsh(&bytes, &format!("{label} '{}'", path.display()))
}

fn decode_borsh<T: BorshDeserialize>(bytes: &[u8], label: &str) -> Result<T, String> {
    T::try_from_slice(bytes).map_err(|err| format!("cannot decode {label} as strict Borsh (trailing bytes are refused): {err}"))
}

fn read_limited(path: &Path, label: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path).map_err(|err| format!("cannot stat {label} '{}': {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{label} '{}' is not a regular file", path.display()));
    }
    if metadata.len() > max_bytes {
        return Err(format!("{label} '{}' is {} bytes, above the {max_bytes}-byte limit", path.display(), metadata.len()));
    }
    let bytes = std::fs::read(path).map_err(|err| format!("cannot read {label} '{}': {err}", path.display()))?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!("{label} '{}' grew above the {max_bytes}-byte limit while being read", path.display()));
    }
    Ok(bytes)
}

fn ensure_distinct_outputs(first: &Path, second: &Path) -> Result<(), String> {
    if first == second {
        return Err(format!("manifest and restamped-leaf outputs must be different paths ('{}')", first.display()));
    }
    Ok(())
}

fn write_new_pair(first_path: &Path, first: &[u8], second_path: &Path, second: &[u8]) -> Result<(), String> {
    write_new_payload(first_path, first)?;
    if let Err(err) = write_new_payload(second_path, second) {
        let _ = std::fs::remove_file(first_path);
        return Err(err);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
    use kaspa_consensus_core::palw_audit::PalwAuditSelectionFacts;
    use tempfile::tempdir;

    /// Per-leaf audit results covering the whole batch, so the beacon-selected sample is always
    /// answered for. Returns the file `build_audit_vote` reads. Kept in a leaked temp dir because
    /// the returned path outlives the helper.
    fn write_leaf_verdicts(facts: &PalwAuditRoundFacts, reproduced: bool) -> PathBuf {
        let dir = tempdir().expect("temp dir").into_path();
        let path = dir.join("leaf-verdicts.json");
        let file = LeafVerdictFile {
            batch_id: facts.batch_id,
            audit_beacon_epoch: facts.audit_beacon_epoch,
            leaves: (0..facts.leaves.len() as u32).map(|leaf_index| LeafVerdictEntry { leaf_index, reproduced }).collect(),
        };
        std::fs::write(&path, serde_json::to_string(&file).expect("serialize")).expect("write");
        path
    }

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn op(byte: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(h(byte), 0)
    }

    fn leaf(index: u32, provider_a: TransactionOutpoint, provider_b: TransactionOutpoint) -> PalwPublicLeafV1 {
        let reward_spk = p2pkh_mldsa87_spk(&[0x71; 64]);
        PalwPublicLeafV1 {
            version: 1,
            batch_id: Hash64::default(),
            leaf_index: index,
            job_nullifier: h(0x10 + index as u8),
            ticket_nullifier_commitment: h(0x20 + index as u8),
            model_profile_id: h(0x30),
            runtime_class_id: h(0x31),
            shape_id: 1,
            quantum_count: 1,
            proof_type: 1,
            provider_a_bond: provider_a,
            provider_b_bond: provider_b,
            provider_a_reward_script: reward_spk.clone(),
            provider_b_reward_script: reward_spk,
            ticket_authority_pk_hash: h(0x32),
            private_match_commitment: h(0x33 + index as u8),
            receipt_da_object_version: 1,
            receipt_da_root: h(0x40 + index as u8),
            receipt_da_object_len: 1,
            receipt_da_chunk_count: 1,
            receipt_v3_compute_set_id: Hash64::default(),
            receipt_v3_job_challenge: Hash64::default(),
            receipt_v3_issued_epoch: 0,
            receipt_v3_expires_epoch: 0,
            registered_epoch: 3,
            activation_epoch: 11,
            expiry_epoch: 17,
            leaf_bond_sompi: 0,
            a_commit: Hash64::default(),
            a_commit_epoch: 0,
            provider_snapshot_root: Hash64::from_bytes([0x7c; 64]),
            assignment_proof_root: Hash64::from_bytes([0x7d; 64]),
            dispatch_kind: 0, // BeaconAssigned (external sentinels)
        }
    }

    fn base_leaves() -> Vec<PalwPublicLeafV1> {
        vec![leaf(0, op(1), op(2)), leaf(1, op(1), op(2))]
    }

    #[test]
    fn manifest_restamp_and_chunk_are_one_strict_consensus_round_trip() {
        let (manifest, manifest_payload, leaves) =
            build_manifest_artifacts(PalwArtifactNetwork::Testnet110, base_leaves(), 3, h(0x50), h(0x51)).unwrap();
        assert_eq!(validate_palw_overlay_payload(BATCH_MANIFEST_SUBNETWORK_BYTE, &manifest_payload), Ok(()));
        assert!(leaves.iter().all(|leaf| leaf.batch_id == manifest.batch_id));
        validate_manifest_and_leaves(PalwArtifactNetwork::Testnet110, &manifest, &leaves, None).unwrap();

        let witnesses: std::collections::BTreeMap<_, _> = {
            use kaspa_consensus_core::palw::{
                BeaconAssignedProof, PalwAssignmentInterval, PalwDispatchEvidence, PalwLeafPcpbWitnessV1, PalwMerkleMembership,
                PalwProviderSnapshotEntry, SlotAssignmentWitness,
            };
            let th = |b: u8| Hash64::from_bytes([b; 64]);
            let slot = |b: u8, lo: u128| SlotAssignmentWitness {
                entry: PalwProviderSnapshotEntry {
                    provider_id: th(b),
                    ml_dsa_pk_hash: th(b.wrapping_add(1)),
                    bond_sompi: 10,
                    reward_script_commitment: th(b.wrapping_add(2)),
                },
                snapshot_membership: PalwMerkleMembership { index: 0, leaf: th(b), siblings: Vec::new() },
                interval: PalwAssignmentInterval { provider_id: th(b), bond_sompi: 10, cumulative_lo: lo },
                assignment_membership: PalwMerkleMembership { index: 0, leaf: th(b), siblings: Vec::new() },
            };
            leaves
                .iter()
                .map(|l| {
                    (
                        l.leaf_index,
                        PalwLeafPcpbWitnessV1 {
                            scheduler_job_id: th(0xe1),
                            requester_credential: th(0xe2),
                            request_commitment: th(0xe3),
                            dispatch: PalwDispatchEvidence::BeaconAssigned(BeaconAssignedProof {
                                slot_a: slot(0xe4, 0),
                                slot_b: slot(0xe8, 10),
                            }),
                        },
                    )
                })
                .collect()
        };
        let (_, chunk_payload) = build_leaf_chunk(manifest.batch_id, 0, &leaves, &witnesses).unwrap();
        assert_eq!(validate_palw_overlay_payload(LEAF_CHUNK_SUBNETWORK_BYTE, &chunk_payload), Ok(()));
        verify_chunk_membership(&manifest, &chunk_payload).unwrap();

        let mut changed = leaves;
        changed[0].receipt_da_root = h(0xee);
        assert!(validate_manifest_and_leaves(PalwArtifactNetwork::Testnet110, &manifest, &changed, None).is_err());
    }

    #[test]
    fn manifest_builder_refuses_prebound_or_lifecycle_drifting_leaves() {
        let mut prebound = base_leaves();
        prebound[0].batch_id = h(0xaa);
        assert!(build_manifest_artifacts(PalwArtifactNetwork::Testnet110, prebound, 3, h(1), h(2)).unwrap_err().contains("unbound"));

        let mut wrong_window = base_leaves();
        wrong_window[0].expiry_epoch += 1;
        let err = build_manifest_artifacts(PalwArtifactNetwork::Testnet110, wrong_window, 3, h(1), h(2)).unwrap_err();
        assert!(err.contains("lifecycle epochs"));
    }

    #[test]
    fn interchange_json_is_versioned_and_rejects_unknown_top_level_fields() {
        let encoded = encode_leaf_set(&base_leaves()).unwrap();
        let decoded: LeafSetFile = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded.schema, LEAF_SET_SCHEMA);
        assert_eq!(decoded.leaves, base_leaves());
        let unknown = format!(r#"{{"schema":"{LEAF_SET_SCHEMA}","leaves":[],"unexpected":true}}"#);
        assert!(serde_json::from_str::<LeafSetFile>(&unknown).is_err());
    }

    #[test]
    fn paired_create_new_write_rolls_back_first_artifact_if_second_exists() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("manifest.borsh");
        let second = dir.path().join("leaves.json");
        std::fs::write(&second, b"owned").unwrap();
        assert!(write_new_pair(&first, b"manifest", &second, b"leaves").is_err());
        assert!(!first.exists());
        assert_eq!(std::fs::read(second).unwrap(), b"owned");
    }

    fn provider_record(seed_byte: u8, amount: u64, unbond_delay_epochs: u64) -> PalwProviderBondRecord {
        let key = ValidatorKey::from_seed([seed_byte; 32]);
        PalwProviderBondRecord {
            version: 1,
            bond_outpoint: op(seed_byte),
            owner_pubkey_hash: key.validator_id,
            owner_public_key: key.public_key().to_vec(),
            operator_group_id: h(0x80 ^ seed_byte),
            runtime_classes: vec![h(0x90)],
            capacity_by_shape: vec![(1, 1)],
            reward_key_root: h(0x91),
            amount_sompi: amount,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbond_delay_epochs,
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
        }
    }

    fn audit_fixture() -> (PalwAuditRoundFacts, Vec<(u8, TransactionOutpoint)>) {
        let network = PalwArtifactNetwork::Testnet110;
        let params = Params::from(network.network_id());
        let admission = params.palw_batch_admission;
        let (manifest, _, leaves) = build_manifest_artifacts(network, base_leaves(), 3, h(0x50), h(0x51)).expect("fixture manifest");
        let mut records: Vec<_> = (1..=5)
            .map(|seed| provider_record(seed, admission.min_provider_bond_sompi, admission.provider_unbond_floor_epochs))
            .collect();
        records.sort_by(|a, b| compare_outpoint(&a.bond_outpoint, &b.bond_outpoint));
        let view = ProviderBondView::from_records(records.into_iter().map(|record| (record.bond_outpoint, record)));
        let audit_beacon_epoch = 5;
        let previous_epoch_seed = h(0xa0);
        let snapshot_daa_score = audit_beacon_epoch * params.palw_epoch_length_daa;
        let selection = derive_palw_audit_selection(
            &previous_epoch_seed,
            &manifest.batch_id,
            &view,
            snapshot_daa_score,
            &leaves,
            params.palw_audit_committee_size as usize,
            params.palw_audit_sample_size as u32,
        )
        .unwrap();
        let mut chunks_present = [0u64; 4];
        for chunk_index in 0..manifest.chunk_count as usize {
            chunks_present[chunk_index / 64] |= 1u64 << (chunk_index % 64);
        }
        let lifecycle = PalwBatchLifecycleV1 {
            status: PalwBatchStatus::Committed,
            registration_epoch: manifest.registration_epoch,
            activation_not_before_epoch: manifest.activation_not_before_epoch,
            expiry_epoch: manifest.expiry_epoch,
            leaf_count: manifest.leaf_count,
            chunk_count: manifest.chunk_count,
            chunks_present,
            leaf_root: manifest.leaf_root,
            cert_hash: None,
            cert_activation_epoch: 0,
            cert_expiry_epoch: 0,
            cert_approving_stake: 0,
            first_cert_daa: None,
            revoked_from_daa: None,
        };
        let facts = PalwAuditRoundFacts {
            network_id: 110,
            sink: h(0xb0),
            sink_daa_score: 600,
            inclusion_epoch: 6,
            batch_id: manifest.batch_id,
            manifest_hash: manifest.content_id(),
            manifest,
            lifecycle,
            leaves,
            audit_beacon_epoch,
            previous_epoch_seed,
            snapshot_daa_score,
            inclusion_window_epochs: palw_audit_epoch_inclusion_window_epochs(&admission),
            committee_size: params.palw_audit_committee_size,
            sample_size: params.palw_audit_sample_size,
            quorum_num: params.palw_audit_quorum_num,
            quorum_den: params.palw_audit_quorum_den,
            selection,
        };
        let auditors = facts
            .selection
            .selected_auditors
            .iter()
            .map(|record| (record.bond_outpoint.transaction_id.as_byte_slice()[0], record.bond_outpoint))
            .collect();
        (facts, auditors)
    }

    fn rederive_selection(facts: &PalwAuditRoundFacts, records: Vec<PalwProviderBondRecord>) -> PalwAuditSelectionFacts {
        let view = ProviderBondView::from_records(records.into_iter().map(|record| (record.bond_outpoint, record)));
        derive_palw_audit_selection(
            &facts.previous_epoch_seed,
            &facts.batch_id,
            &view,
            facts.snapshot_daa_score,
            &facts.leaves,
            facts.committee_size as usize,
            facts.sample_size as u32,
        )
        .unwrap()
    }

    #[test]
    fn independently_signed_votes_assemble_only_after_signature_and_quorum_checks() {
        let (facts, auditors) = audit_fixture();
        validate_audit_facts(PalwArtifactNetwork::Testnet110, &facts).unwrap();
        let encoded_facts = encode_audit_facts(&facts).unwrap();
        assert_eq!(encoded_facts.iter().filter(|&&byte| byte == b'\n').count(), 1, "large facts snapshots stay compact");
        let decoded_facts: AuditFactsFile = serde_json::from_slice(&encoded_facts).unwrap();
        assert_eq!(decoded_facts.schema, AUDIT_FACTS_SCHEMA);
        assert_eq!(decoded_facts.facts, facts);
        assert_eq!(auditors.len(), 3);
        // AUDIT-EXEC-01: the vote builder now DERIVES the verdict by executing the round, so the
        // test supplies per-leaf results instead of asserting a batch-wide `pass`.
        let leaf_verdicts = write_leaf_verdicts(&facts, true);
        let votes: Vec<_> = auditors
            .iter()
            .take(2)
            .map(|(seed, bond)| {
                build_audit_vote(
                    PalwArtifactNetwork::Testnet110,
                    &facts,
                    ValidatorKey::from_seed([*seed; 32]),
                    *bond,
                    &leaf_verdicts,
                    2,
                    Hash64::default(),
                )
                .unwrap()
            })
            .collect();
        let certificate = build_audit_certificate(PalwArtifactNetwork::Testnet110, &facts, votes.clone()).unwrap();
        assert_eq!(certificate.cert.votes.len(), 2);
        assert_eq!(validate_palw_overlay_payload(BATCH_CERTIFICATE_SUBNETWORK_BYTE, &certificate.payload), Ok(()));

        let below_quorum = build_audit_certificate(PalwArtifactNetwork::Testnet110, &facts, vec![votes[0].clone()]);
        assert!(below_quorum.unwrap_err().contains("quorum"));

        let mut tampered = votes;
        tampered[0].signature[0] ^= 1;
        assert!(build_audit_certificate(PalwArtifactNetwork::Testnet110, &facts, tampered).is_err());
        let wrong_key = build_audit_vote(
            PalwArtifactNetwork::Testnet110,
            &facts,
            ValidatorKey::from_seed([0xf0; 32]),
            auditors[0].1,
            &leaf_verdicts,
            2,
            Hash64::default(),
        );
        assert!(wrong_key.unwrap_err().contains("does not own"));
    }

    #[test]
    fn certificate_summary_repackaging_is_rejected_by_vote_binding() {
        let (facts, auditors) = audit_fixture();
        let leaf_verdicts = write_leaf_verdicts(&facts, true);
        let votes: Vec<_> = auditors
            .iter()
            .take(2)
            .map(|(seed, bond)| {
                build_audit_vote(
                    PalwArtifactNetwork::Testnet110,
                    &facts,
                    ValidatorKey::from_seed([*seed; 32]),
                    *bond,
                    &leaf_verdicts,
                    2,
                    Hash64::default(),
                )
                .unwrap()
            })
            .collect();
        let first = build_audit_certificate(PalwArtifactNetwork::Testnet110, &facts, votes.clone()).unwrap();
        assert_eq!(validate_palw_overlay_payload(BATCH_CERTIFICATE_SUBNETWORK_BYTE, &first.payload), Ok(()));

        // Rewriting only the certificate envelope is rejected before contextual signature checks.
        let mut repackaged = first.cert.clone();
        repackaged.passed_leaf_count = 1;
        repackaged.rejected_leaf_bitmap_root = h(0xee);
        let repackaged_payload = borsh::to_vec(&repackaged).unwrap();
        assert!(validate_palw_overlay_payload(BATCH_CERTIFICATE_SUBNETWORK_BYTE, &repackaged_payload).is_err());

        // Two individually valid signatures over different summaries cannot be combined.
        let different_summary_vote = build_audit_vote(
            PalwArtifactNetwork::Testnet110,
            &facts,
            ValidatorKey::from_seed([auditors[1].0; 32]),
            auditors[1].1,
            &leaf_verdicts,
            1,
            h(0xee),
        )
        .unwrap();
        let mixed = build_audit_certificate(PalwArtifactNetwork::Testnet110, &facts, vec![votes[0].clone(), different_summary_vote])
            .unwrap_err();
        assert!(mixed.contains("does not bind the audit round's passed/rejected summary"));

        // Rewriting both the envelope and embedded vote fields still invalidates each V2 signature.
        let mut resigned_summary_without_resigning = votes;
        for vote in &mut resigned_summary_without_resigning {
            vote.passed_leaf_count = 1;
            vote.rejected_leaf_bitmap_root = h(0xee);
        }
        assert!(build_audit_certificate(PalwArtifactNetwork::Testnet110, &facts, resigned_summary_without_resigning).is_err());
    }

    #[test]
    fn self_consistent_provider_omission_is_caught_only_by_live_node_comparison() {
        let (live, _) = audit_fixture();
        let mut omitted = live.clone();
        let mut records = omitted.selection.provider_bonds.clone();
        records.retain(|record| record.bond_outpoint != op(5));
        omitted.selection = rederive_selection(&omitted, records);
        assert_eq!(omitted.sink, live.sink, "a sink label alone does not authenticate registry completeness");
        assert!(validate_audit_facts(PalwArtifactNetwork::Testnet110, &omitted).is_ok());
        let err = ensure_frozen_round_matches(&omitted, &live).unwrap_err();
        assert!(err.contains("selection-relevant provider view"));
    }

    #[test]
    fn harmless_tip_advance_and_irrelevant_future_provider_preserve_the_frozen_round() {
        let (file, _) = audit_fixture();
        let mut live = file.clone();
        live.sink = h(0xb1);
        live.sink_daa_score = 700;
        live.inclusion_epoch = 7;
        live.lifecycle.status = PalwBatchStatus::Auditing;
        let mut current_records = live.selection.provider_bonds.clone();
        let params = Params::from(PalwArtifactNetwork::Testnet110.network_id());
        let mut irrelevant_future = provider_record(
            0xfe,
            params.palw_batch_admission.min_provider_bond_sompi,
            params.palw_batch_admission.provider_unbond_floor_epochs,
        );
        irrelevant_future.created_daa_score = live.snapshot_daa_score + 1;
        irrelevant_future.activation_daa_score = live.snapshot_daa_score + 1;
        current_records.push(irrelevant_future);
        live.selection = rederive_selection(&live, current_records);
        assert_eq!(live.selection, file.selection, "an unreferenced future row is outside the frozen view");

        ensure_frozen_round_matches(&file, &live).unwrap();
    }

    #[test]
    fn pre_snapshot_fork_drift_is_rejected_even_if_the_tip_cursor_is_newer() {
        let (file, _) = audit_fixture();
        let mut fork = file.clone();
        fork.sink = h(0xb2);
        fork.sink_daa_score = 700;
        fork.inclusion_epoch = 7;
        fork.previous_epoch_seed = h(0xa1);
        fork.selection = rederive_selection(&fork, fork.selection.provider_bonds.clone());
        validate_audit_facts(PalwArtifactNetwork::Testnet110, &fork).unwrap();

        let err = ensure_frozen_round_matches(&file, &fork).unwrap_err();
        assert!(err.contains("seed/fork"));
    }

    #[test]
    fn certificate_uses_fresh_live_inclusion_epoch_after_tip_advance() {
        let (file, auditors) = audit_fixture();
        let mut live = file.clone();
        live.sink = h(0xb3);
        live.sink_daa_score = 700;
        live.inclusion_epoch = 7;
        ensure_frozen_round_matches(&file, &live).unwrap();

        let leaf_verdicts = write_leaf_verdicts(&file, true);
        let votes = auditors
            .iter()
            .take(2)
            .map(|(seed, bond)| {
                build_audit_vote(
                    PalwArtifactNetwork::Testnet110,
                    &file,
                    ValidatorKey::from_seed([*seed; 32]),
                    *bond,
                    &leaf_verdicts,
                    2,
                    Hash64::default(),
                )
                .unwrap()
            })
            .collect();
        let certificate = build_audit_certificate(PalwArtifactNetwork::Testnet110, &live, votes).unwrap();
        assert_eq!(certificate.cert.certificate_epoch, 7);
    }

    #[test]
    fn local_facts_validation_enforces_the_rpc_provider_bound() {
        let (mut facts, _) = audit_fixture();
        let template = facts.selection.provider_bonds[0].clone();
        facts.selection.provider_bonds.resize(MAX_PALW_AUDIT_FACT_PROVIDER_RECORDS + 1, template);
        let err = validate_audit_facts(PalwArtifactNetwork::Testnet110, &facts).unwrap_err();
        assert!(err.contains("above the 1024-record audit-facts bound"));
    }
}
