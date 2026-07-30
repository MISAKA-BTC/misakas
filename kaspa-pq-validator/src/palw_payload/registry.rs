//! ADR-MA §17 — Compute Set registry payload constructors (band 0x40-0x44) plus the §17.3
//! activation-vote signing/assembly workflow.
//!
//! JSON in, canonical Borsh out. Every derived id (`compute_set_id`, `policy_id`, `plan_id`,
//! votes root, validator-set commitment) is computed here with the SAME consensus-core functions
//! the node runs, then printed, so the operator pastes ids instead of re-deriving them.
//!
//! The §17.3 certificate workflow is three commands, in dependency order:
//!   1. `registry-validator-set` — bonds JSON → `validator_set_commitment` + total stake
//!      (the summary a vote signs must already pin these, so they come first);
//!   2. `registry-cert-vote` — summary JSON + validator key → one signed vote (JSON);
//!   3. `registry-cert-assemble` — summary + votes → tally + votes_root recomputed, canonical
//!      certificate Borsh for `palw-submit --kind registry-cert`.

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_compute_set::{
    ComputeSetState, PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION, PALW_COMPUTE_SET_ACTIVATION_MLDSA87_CONTEXT,
    PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION, PALW_COMPUTE_SET_POLICY_UPDATE_VERSION, PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION,
    PalwComputeSetActivationCertificateV1, PalwComputeSetActivationVoteV1, PalwComputeSetDescriptorV2, PalwComputeSetEmergencyHaltV1,
    PalwComputeSetPolicyUpdateV1, PalwComputeSetPolicyV1, PalwComputeSetProposalV1, PalwModelAllocationPlanV1,
    palw_activation_validator_set_commitment, palw_activation_votes_root, parse_palw_compute_registry,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed, parse_stake_bond_ref};

/// Shared output flags.
#[derive(Args, Debug)]
pub struct RegistryOutArgs {
    /// Output file for the canonical Borsh payload.
    #[arg(long)]
    pub out: PathBuf,
}

fn write_payload(out: &PathBuf, subnet_byte: u8, bytes: Vec<u8>) -> Result<(), String> {
    // Round-trip through the node's own strict parser before anything is written: what this tool
    // emits is exactly what admission will decode, or the command fails here.
    parse_palw_compute_registry(subnet_byte, &bytes).map_err(|e| format!("self-check parse failed: {e}"))?;
    std::fs::write(out, &bytes).map_err(|e| format!("write {}: {e}", out.display()))?;
    println!("wrote {} bytes to {}", bytes.len(), out.display());
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf, what: &str) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {what} JSON {}: {e}", path.display()))
}

// =============================================================================================
// 0x40 — proposal
// =============================================================================================

#[derive(Args, Debug)]
pub struct RegistryProposalArgs {
    /// PalwComputeSetDescriptorV2 as JSON (Hash64 fields are 128-char hex).
    #[arg(long)]
    descriptor_json: PathBuf,
    /// Proposer credential id (128-hex).
    #[arg(long)]
    proposer_credential: String,
    /// §23.2 proposal bond outpoint, `txid:index`.
    #[arg(long)]
    proposal_bond_ref: String,
    /// Artifact distribution root (128-hex).
    #[arg(long)]
    artifact_distribution_root: String,
    /// Independent build attestation root (128-hex).
    #[arg(long)]
    build_attestation_root: String,
    /// Requested Shadow activation DAA.
    #[arg(long)]
    requested_shadow_activation_daa: u64,
    #[command(flatten)]
    out: RegistryOutArgs,
}

fn parse_hash(hex: &str, what: &str) -> Result<Hash64, String> {
    hex.parse::<Hash64>().map_err(|e| format!("{what}: {e}"))
}

fn parse_outpoint(s: &str, what: &str) -> Result<TransactionOutpoint, String> {
    parse_stake_bond_ref(s).map_err(|e| format!("{what}: {e}"))
}

pub(super) fn registry_proposal(args: RegistryProposalArgs) -> Result<(), String> {
    let descriptor: PalwComputeSetDescriptorV2 = read_json(&args.descriptor_json, "descriptor")?;
    descriptor.validate_in_isolation().map_err(|e| format!("descriptor rejected: {e}"))?;
    let proposal = PalwComputeSetProposalV1 {
        version: PALW_COMPUTE_SET_PROPOSAL_PAYLOAD_VERSION,
        descriptor,
        proposer_credential: parse_hash(&args.proposer_credential, "proposer_credential")?,
        proposal_bond_ref: parse_outpoint(&args.proposal_bond_ref, "proposal_bond_ref")?,
        artifact_distribution_root: parse_hash(&args.artifact_distribution_root, "artifact_distribution_root")?,
        independent_build_attestation_root: parse_hash(&args.build_attestation_root, "build_attestation_root")?,
        requested_shadow_activation_daa: args.requested_shadow_activation_daa,
    };
    println!("compute_set_id: {}", proposal.descriptor.compute_set_id());
    write_payload(&args.out.out, 0x40, borsh::to_vec(&proposal).expect("borsh"))
}

// =============================================================================================
// 0x42 — policy update / 0x43 — allocation plan / 0x44 — emergency halt
// =============================================================================================

#[derive(Args, Debug)]
pub struct RegistryPolicyArgs {
    /// PalwComputeSetPolicyV1 as JSON.
    #[arg(long)]
    policy_json: PathBuf,
    #[command(flatten)]
    out: RegistryOutArgs,
}

pub(super) fn registry_policy(args: RegistryPolicyArgs) -> Result<(), String> {
    let policy: PalwComputeSetPolicyV1 = read_json(&args.policy_json, "policy")?;
    println!("policy_id: {}", policy.policy_id());
    println!("compute_set_id: {}  sequence: {}  state: {:?}", policy.compute_set_id, policy.policy_sequence, policy.state);
    let update = PalwComputeSetPolicyUpdateV1 { version: PALW_COMPUTE_SET_POLICY_UPDATE_VERSION, policy };
    write_payload(&args.out.out, 0x42, borsh::to_vec(&update).expect("borsh"))
}

#[derive(Args, Debug)]
pub struct RegistryPlanArgs {
    /// PalwModelAllocationPlanV1 as JSON. `plan_id` may be zero/absent-equivalent — it is
    /// re-derived here (the §10.2 zeroed-self-reference rule) and overwritten.
    #[arg(long)]
    plan_json: PathBuf,
    #[command(flatten)]
    out: RegistryOutArgs,
}

pub(super) fn registry_plan(args: RegistryPlanArgs) -> Result<(), String> {
    let mut plan: PalwModelAllocationPlanV1 = read_json(&args.plan_json, "plan")?;
    plan.plan_id = plan.derive_plan_id();
    println!("plan_id: {}", plan.plan_id);
    for entry in &plan.entries {
        println!("  {} -> {} bps", entry.compute_set_id, entry.target_share_bps);
    }
    write_payload(&args.out.out, 0x43, borsh::to_vec(&plan).expect("borsh"))
}

#[derive(Args, Debug)]
pub struct RegistryHaltArgs {
    /// The set to halt (128-hex).
    #[arg(long)]
    compute_set_id: String,
    /// Evidence root (128-hex; zero for a pure governance stop).
    #[arg(long, default_value = "")]
    evidence_root: String,
    #[command(flatten)]
    out: RegistryOutArgs,
}

pub(super) fn registry_halt(args: RegistryHaltArgs) -> Result<(), String> {
    let evidence_root =
        if args.evidence_root.is_empty() { Hash64::default() } else { parse_hash(&args.evidence_root, "evidence_root")? };
    let halt = PalwComputeSetEmergencyHaltV1 {
        version: PALW_COMPUTE_SET_EMERGENCY_HALT_VERSION,
        compute_set_id: parse_hash(&args.compute_set_id, "compute_set_id")?,
        evidence_root,
    };
    write_payload(&args.out.out, 0x44, borsh::to_vec(&halt).expect("borsh"))
}

// =============================================================================================
// §17.3 — activation certificate workflow (0x41)
// =============================================================================================

/// One active DNS stake bond, as the certificate author predicts the active set (§17.3 — the
/// commitment is a pure set fingerprint; get these three values from each bond you registered).
#[derive(serde::Serialize, serde::Deserialize)]
struct ActiveBondJson {
    /// `txid:index`.
    bond_outpoint: String,
    /// 128-hex validator id (BLAKE2b-512 of the ML-DSA-87 verification key).
    validator_id: String,
    /// Bond stake in sompi.
    amount: u64,
}

fn bonds_to_records(bonds: &[ActiveBondJson]) -> Result<Vec<kaspa_consensus_core::dns_finality::StakeBondRecord>, String> {
    let mut records = Vec::with_capacity(bonds.len());
    for bond in bonds {
        // Only the three fingerprint fields matter for the commitment; the rest are inert
        // placeholders (the commitment function reads outpoint/validator_id/amount only).
        records.push(kaspa_consensus_core::dns_finality::StakeBondRecord {
            version: 1,
            bond_outpoint: parse_outpoint(&bond.bond_outpoint, "bond_outpoint")?,
            owner_pubkey_hash: Hash64::default(),
            validator_pubkey_hash: parse_hash(&bond.validator_id, "validator_id")?,
            validator_pubkey: Vec::new(),
            amount: bond.amount,
            activation_daa_score: 0,
            created_daa_score: 0,
            unbonding_period_blocks: 0,
            owner_reward_spk_payload: [0u8; 64],
            unbond_request_daa_score: None,
            slashed_at_daa_score: None,
            status: kaspa_consensus_core::dns_finality::BondStatus::Active,
            last_attested_epoch: None,
            dormant_at_daa_score: None,
            dormant_at_epoch: None,
        });
    }
    records.sort_by(|a, b| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    });
    Ok(records)
}

#[derive(Args, Debug)]
pub struct RegistryValidatorSetArgs {
    /// JSON array of the ACTIVE stake bonds: [{"bond_outpoint":"txid:index","validator_id":hex,"amount":sompi}].
    #[arg(long)]
    bonds_json: PathBuf,
}

pub(super) fn registry_validator_set(args: RegistryValidatorSetArgs) -> Result<(), String> {
    let bonds: Vec<ActiveBondJson> = read_json(&args.bonds_json, "bonds")?;
    let records = bonds_to_records(&bonds)?;
    let commitment = palw_activation_validator_set_commitment(&records)
        .ok_or("bond list has duplicate outpoints (the fingerprint requires unique ascending outpoints)")?;
    let total: u128 = records.iter().map(|b| b.amount as u128).sum();
    println!("validator_set_commitment: {commitment}");
    println!("total_selected_stake: {total}");
    println!("bonds: {}", records.len());
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum ActivationVote {
    Pass,
    Reject,
}

#[derive(Args, Debug)]
pub struct RegistryCertVoteArgs {
    /// The certificate SUMMARY as JSON: a full PalwComputeSetActivationCertificateV1 whose
    /// `approving_stake` / `votes_root` / `votes` are zero/empty — the signature covers only the
    /// summary + this vote's own position, so those stay unfilled until assembly.
    #[arg(long)]
    summary_json: PathBuf,
    /// This validator's ML-DSA-87 key file (the DNS stake bond's key).
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,
    /// This validator's stake bond outpoint, `txid:index`.
    #[arg(long)]
    bond_outpoint: String,
    /// pass / reject.
    #[arg(long, value_enum)]
    vote: ActivationVote,
    /// Network number (e.g. 20 for compute-registry-palw).
    #[arg(long)]
    network_id: u32,
    /// Output file for the signed vote (JSON).
    #[arg(long)]
    out: PathBuf,
}

pub(super) fn registry_cert_vote(args: RegistryCertVoteArgs) -> Result<(), String> {
    let summary: PalwComputeSetActivationCertificateV1 = read_json(&args.summary_json, "certificate summary")?;
    if summary.version != PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION {
        return Err(format!("summary version {} is not {}", summary.version, PALW_COMPUTE_SET_ACTIVATION_CERT_VERSION));
    }
    let seed = load_validator_seed(&args.validator_key)?;
    let key = ValidatorKey::from_seed(seed);
    let mut vote = PalwComputeSetActivationVoteV1 {
        bond_outpoint: parse_outpoint(&args.bond_outpoint, "bond_outpoint")?,
        vote: match args.vote {
            ActivationVote::Pass => 1,
            ActivationVote::Reject => 0,
        },
        signature: Vec::new(),
    };
    let digest = vote.signing_hash(args.network_id, &summary);
    vote.signature = key.sign_with_context(&digest.as_bytes(), PALW_COMPUTE_SET_ACTIVATION_MLDSA87_CONTEXT).to_vec();
    println!("validator_id: {}", key.validator_id);
    println!("set: {}  vote: {}", summary.compute_set_id, vote.vote);
    let json = serde_json::to_string_pretty(&vote).map_err(|e| format!("serialize vote: {e}"))?;
    std::fs::write(&args.out, json).map_err(|e| format!("write {}: {e}", args.out.display()))?;
    println!("wrote signed vote to {}", args.out.display());
    Ok(())
}

#[derive(Args, Debug)]
pub struct RegistryCertAssembleArgs {
    /// The SAME summary JSON every vote signed.
    #[arg(long)]
    summary_json: PathBuf,
    /// Signed vote JSON files (repeat per validator).
    #[arg(long = "vote")]
    votes: Vec<PathBuf>,
    /// The bonds JSON used for `registry-validator-set` (tally weights come from it).
    #[arg(long)]
    bonds_json: PathBuf,
    #[command(flatten)]
    out: RegistryOutArgs,
}

pub(super) fn registry_cert_assemble(args: RegistryCertAssembleArgs) -> Result<(), String> {
    let mut cert: PalwComputeSetActivationCertificateV1 = read_json(&args.summary_json, "certificate summary")?;
    let bonds: Vec<ActiveBondJson> = read_json(&args.bonds_json, "bonds")?;
    let records = bonds_to_records(&bonds)?;
    let commitment = palw_activation_validator_set_commitment(&records).ok_or("bond list has duplicate outpoints")?;
    if commitment != cert.validator_set_commitment {
        return Err(format!(
            "summary validator_set_commitment {} does not match the bonds file ({commitment}) — every vote signed the summary, so fix the summary and re-collect votes",
            cert.validator_set_commitment
        ));
    }
    let total: u128 = records.iter().map(|b| b.amount as u128).sum();
    if total != cert.total_selected_stake {
        return Err(format!("summary total_selected_stake {} does not match the bonds file ({total})", cert.total_selected_stake));
    }

    let mut votes: Vec<PalwComputeSetActivationVoteV1> = Vec::with_capacity(args.votes.len());
    for path in &args.votes {
        votes.push(read_json(path, "signed vote")?);
    }
    votes.sort_by(|a, b| {
        (a.bond_outpoint.transaction_id, a.bond_outpoint.index).cmp(&(b.bond_outpoint.transaction_id, b.bond_outpoint.index))
    });
    let stake_of = |outpoint: &TransactionOutpoint| -> u128 {
        records.iter().find(|b| b.bond_outpoint == *outpoint).map(|b| b.amount as u128).unwrap_or(0)
    };
    cert.approving_stake = votes.iter().filter(|v| v.vote == 1).map(|v| stake_of(&v.bond_outpoint)).sum();
    cert.votes_root = palw_activation_votes_root(&votes).ok_or("votes are not unique per bond outpoint")?;
    cert.votes = votes;
    println!("approving_stake: {} / total {}", cert.approving_stake, cert.total_selected_stake);
    println!("votes_root: {}", cert.votes_root);
    write_payload(&args.out.out, 0x41, borsh::to_vec(&cert).expect("borsh"))
}

// =============================================================================================
// Helper: dump a descriptor JSON template with every field zeroed (fill in real roots).
// =============================================================================================

#[derive(Args, Debug)]
pub struct RegistryDescriptorTemplateArgs {
    /// Output file for the zeroed descriptor JSON.
    #[arg(long)]
    out: PathBuf,
}

pub(super) fn registry_descriptor_template(args: RegistryDescriptorTemplateArgs) -> Result<(), String> {
    let zero = Hash64::default();
    let descriptor = PalwComputeSetDescriptorV2 {
        version: kaspa_consensus_core::palw_compute_set::PALW_COMPUTE_SET_DESCRIPTOR_VERSION,
        // The REAL frozen Compute VM surface (ADR-MA-006) — the one field no operator should
        // ever fill by hand.
        compute_vm_id: kaspa_consensus_core::palw_compute_ir::compute_vm_id_v1(),
        model_family_id: zero,
        model_artifact_root: zero,
        model_manifest_root: zero,
        tokenizer_root: zero,
        chat_template_root: zero,
        preprocessing_root: zero,
        decode_policy_root: zero,
        semantic_program_root: zero,
        shape_table_root: zero,
        shape_cost_table_root: zero,
        arithmetic_rules_root: zero,
        overflow_budget_root: zero,
        lut_root: zero,
        trace_policy_root: zero,
        checkpoint_policy_root: zero,
        conformance_vector_root: zero,
        modality_mask: 1,
        resource_limits_root: zero,
    };
    let json = serde_json::to_string_pretty(&descriptor).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&args.out, json).map_err(|e| format!("write {}: {e}", args.out.display()))?;
    println!("wrote descriptor template to {} — fill every root with a REAL value before proposing", args.out.display());
    Ok(())
}

/// Print the current policy state names accepted in policy JSON.
pub(super) fn state_names() -> &'static [(&'static str, ComputeSetState)] {
    &[
        ("Proposed", ComputeSetState::Proposed),
        ("Shadow", ComputeSetState::Shadow),
        ("Active", ComputeSetState::Active),
        ("Deprecated", ComputeSetState::Deprecated),
        ("EmergencyHalted", ComputeSetState::EmergencyHalted),
        ("Retired", ComputeSetState::Retired),
    ]
}
