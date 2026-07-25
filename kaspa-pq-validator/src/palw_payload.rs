//! Offline construction of consensus-wire PALW lifecycle payloads.
//!
//! `palw-submit` deliberately consumes already-built Borsh bytes so transaction funding and
//! lifecycle staging stay separate from producer policy. This module supplies the missing operator
//! path for lifecycle objects while keeping private keys and audit evidence off the submission host.

mod da;
mod lifecycle;
mod search;

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw::{validate_palw_overlay_payload, validate_palw_overlay_tx};
use kaspa_pq_validator_core::{ValidatorKey, load_validator_seed};
use misaka_palw_miner::registration::{PROVIDER_BOND_SUBNETWORK_BYTE, build_provider_bond};

use self::da::{DaChallengePayloadArgs, DaInspectArgs, DaResponsePayloadArgs, DaTimeoutPayloadArgs};
use self::lifecycle::{
    AuditCertificatePayloadArgs, AuditFactsPayloadArgs, AuditVotePayloadArgs, BatchManifestPayloadArgs, LeafChunkPayloadArgs,
};
use self::search::{SearchChallengePayloadArgs, SearchResponsePayloadArgs, SearchTimeoutPayloadArgs};

use super::{parse_amount_sompi, parse_hash64};

/// Build an offline PALW payload artifact for a later staged `palw-submit` invocation.
#[derive(Parser, Debug)]
pub struct PalwPayloadArgs {
    #[command(subcommand)]
    command: PalwPayloadCommand,
}

#[derive(Subcommand, Debug)]
enum PalwPayloadCommand {
    /// Build a canonical provider-bond payload whose owner is derived from an ML-DSA-87 key.
    ProviderBond(ProviderBondPayloadArgs),
    /// Build a content-addressed batch manifest and its batch-id-restamped leaf set.
    BatchManifest(BatchManifestPayloadArgs),
    /// Build one canonical leaf chunk, including manifest membership proofs.
    LeafChunk(LeafChunkPayloadArgs),
    /// Sign one selected auditor vote against a sink-pinned audit-facts snapshot.
    AuditVote(AuditVotePayloadArgs),
    /// Export complete, sink-pinned audit facts from a synced node.
    AuditFacts(AuditFactsPayloadArgs),
    /// Assemble verified auditor votes into a stake-weighted quorum certificate.
    Certificate(AuditCertificatePayloadArgs),
    /// Inspect a canonical DA receipt object and optionally export one fixed-chunk Merkle proof.
    DaInspect(DaInspectArgs),
    /// Sign an on-chain DA availability challenge (subnetwork 0x3a).
    DaChallenge(DaChallengePayloadArgs),
    /// Sign an owner-authorized DA chunk response (subnetwork 0x3b).
    DaResponse(DaResponsePayloadArgs),
    /// Build objective expired-challenge timeout evidence (subnetwork 0x3c).
    DaTimeout(DaTimeoutPayloadArgs),
    /// Sign a search-availability challenge (subnetwork 0x3d); attach a JobSpec to register the
    /// obligation atomically against the bonded scheduler registry.
    SearchChallenge(SearchChallengePayloadArgs),
    /// Build a self-authorizing search chunk-proof response (subnetwork 0x3e).
    SearchResponse(SearchResponsePayloadArgs),
    /// Sign search-availability timeout evidence (subnetwork 0x3f).
    SearchTimeout(SearchTimeoutPayloadArgs),
}

/// The two shipped PALW-active, closed-testnet presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum PalwArtifactNetwork {
    #[value(name = "testnet-110")]
    Testnet110,
    #[value(name = "devnet-111")]
    Devnet111,
}

impl PalwArtifactNetwork {
    fn network_id(self) -> NetworkId {
        match self {
            Self::Testnet110 => NetworkId::with_suffix(NetworkType::Testnet, 110),
            Self::Devnet111 => NetworkId::with_suffix(NetworkType::Devnet, 111),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ShapeCapacity {
    shape_id: u16,
    capacity: u32,
}

/// Arguments needed to build the exact Borsh object accepted by `palw-submit --kind provider-bond`.
#[derive(Parser, Debug)]
struct ProviderBondPayloadArgs {
    /// ML-DSA-87 seed file. Only its public key is embedded; the seed is scrubbed after derivation.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// PALW-active preset whose provider amount and exit-delay floors must be enforced.
    #[arg(long, value_enum, default_value = "testnet-110")]
    network: PalwArtifactNetwork,

    /// Operator-group commitment (128 hex characters). Credentials run by one operator must use the
    /// same group id so committee selection can exclude related providers.
    #[arg(long, value_parser = parse_hash64)]
    operator_group_id: Hash64,

    /// Supported runtime-class commitment (128 hex characters). Repeat for every class; order is
    /// canonicalized and duplicates are rejected.
    #[arg(long = "runtime-class", required = true, value_parser = parse_hash64)]
    runtime_classes: Vec<Hash64>,

    /// Per-shape capacity as `SHAPE_ID=COUNT` (for example `7=4`). Repeat for every shape; shape ids
    /// are canonicalized and zero/duplicate capacities are rejected.
    #[arg(long = "capacity", required = true, value_parser = parse_shape_capacity)]
    capacities: Vec<ShapeCapacity>,

    /// Commitment to the provider's reward-key set (128 hex characters).
    #[arg(long, value_parser = parse_hash64)]
    reward_key_root: Hash64,

    /// Coins locked in provider-bond output 0. Accepts `10MSK`, `10KAS`, raw sompi, or `<n>sompi`.
    /// Must meet the selected preset's provider-bond floor.
    #[arg(long, value_parser = parse_amount_sompi)]
    amount: u64,

    /// Declared PALW-epoch delay after an authorized unbond request. Must meet the network floor.
    #[arg(long, default_value_t = 6)]
    unbond_delay_epochs: u64,

    /// New file to receive raw Borsh payload bytes. Existing files and symlinks are never replaced.
    #[arg(long)]
    out: PathBuf,
}

pub async fn palw_payload(args: PalwPayloadArgs) -> Result<(), String> {
    match args.command {
        PalwPayloadCommand::ProviderBond(args) => provider_bond_payload(args),
        PalwPayloadCommand::BatchManifest(args) => lifecycle::batch_manifest_payload(args),
        PalwPayloadCommand::LeafChunk(args) => lifecycle::leaf_chunk_payload(args),
        PalwPayloadCommand::AuditVote(args) => lifecycle::audit_vote_payload(args).await,
        PalwPayloadCommand::AuditFacts(args) => lifecycle::audit_facts_payload(args).await,
        PalwPayloadCommand::Certificate(args) => lifecycle::audit_certificate_payload(args).await,
        PalwPayloadCommand::DaInspect(args) => da::da_inspect(args),
        PalwPayloadCommand::DaChallenge(args) => da::da_challenge_payload(args),
        PalwPayloadCommand::DaResponse(args) => da::da_response_payload(args),
        PalwPayloadCommand::DaTimeout(args) => da::da_timeout_payload(args),
        PalwPayloadCommand::SearchChallenge(args) => search::search_challenge_payload(args),
        PalwPayloadCommand::SearchResponse(args) => search::search_response_payload(args),
        PalwPayloadCommand::SearchTimeout(args) => search::search_timeout_payload(args),
    }
}

fn provider_bond_payload(args: ProviderBondPayloadArgs) -> Result<(), String> {
    let mut seed = load_validator_seed(&args.validator_key)?;
    let key = ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);

    let payload = build_provider_bond_artifact(&key, &args)?;
    write_new_payload(&args.out, &payload)?;

    println!("payload_kind: provider-bond");
    println!("payload_file: {}", args.out.display());
    println!("payload_bytes: {}", payload.len());
    println!("network: {}", args.network.network_id());
    println!("owner_validator_id: {}", key.validator_id);
    println!("locked_amount_sompi: {}", args.amount);
    println!("required_output_index: 0");
    println!("next: kaspa-pq-validator palw-submit --kind provider-bond --payload-file {} ...", args.out.display());
    Ok(())
}

fn build_provider_bond_artifact(key: &ValidatorKey, args: &ProviderBondPayloadArgs) -> Result<Vec<u8>, String> {
    let params = Params::from(args.network.network_id());
    let admission = params.palw_batch_admission;
    if args.amount < admission.min_provider_bond_sompi {
        return Err(format!(
            "--amount is {} sompi, below the {} provider-bond floor of {} sompi; such a transaction is accepted as bytes but omitted from the provider registry",
            args.amount,
            args.network.network_id(),
            admission.min_provider_bond_sompi
        ));
    }
    if args.unbond_delay_epochs < admission.provider_unbond_floor_epochs {
        return Err(format!(
            "--unbond-delay-epochs is {}, below the {} floor of {}; consensus would silently clamp the registered delay upward",
            args.unbond_delay_epochs,
            args.network.network_id(),
            admission.provider_unbond_floor_epochs
        ));
    }

    let capacities = args.capacities.iter().map(|entry| (entry.shape_id, entry.capacity)).collect();
    let (subnetwork_byte, payload, required_output) = build_provider_bond(
        key.public_key().to_vec(),
        args.operator_group_id,
        args.runtime_classes.clone(),
        capacities,
        args.reward_key_root,
        args.amount,
        args.unbond_delay_epochs,
    )
    .map_err(|err| format!("cannot build provider-bond payload: {err}"))?;
    if subnetwork_byte != PROVIDER_BOND_SUBNETWORK_BYTE {
        return Err(format!(
            "provider constructor returned unexpected subnetwork byte 0x{subnetwork_byte:02x} (expected 0x{PROVIDER_BOND_SUBNETWORK_BYTE:02x})"
        ));
    }
    validate_palw_overlay_payload(subnetwork_byte, &payload)
        .map_err(|err| format!("built provider-bond payload failed consensus validation: {err}"))?;
    validate_palw_overlay_tx(subnetwork_byte, &payload, &[required_output])
        .map_err(|err| format!("built provider-bond carrier shape failed consensus validation: {err}"))?;
    Ok(payload)
}

fn parse_shape_capacity(raw: &str) -> Result<ShapeCapacity, String> {
    let (shape, capacity) =
        raw.split_once('=').ok_or_else(|| format!("invalid capacity '{raw}' (expected SHAPE_ID=COUNT, for example 7=4)"))?;
    if shape.is_empty() || capacity.is_empty() || capacity.contains('=') {
        return Err(format!("invalid capacity '{raw}' (expected exactly one SHAPE_ID=COUNT pair)"));
    }
    let shape_id = shape.parse::<u16>().map_err(|_| format!("invalid shape id in capacity '{raw}' (expected u16)"))?;
    let capacity = capacity.parse::<u32>().map_err(|_| format!("invalid count in capacity '{raw}' (expected u32)"))?;
    if capacity == 0 {
        return Err(format!("invalid capacity '{raw}' (COUNT must be greater than zero)"));
    }
    Ok(ShapeCapacity { shape_id, capacity })
}

fn write_new_payload(path: &Path, payload: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|err| format!("cannot create payload file '{}' (it must not already exist): {err}", path.display()))?;
    if let Err(err) = file.write_all(payload).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(path);
        return Err(format!("cannot durably write payload file '{}': {err}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshDeserialize;
    use kaspa_consensus_core::palw::{PalwProviderBondPayloadV1, provider_bond_lock_spk};

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn args(amount: u64) -> ProviderBondPayloadArgs {
        ProviderBondPayloadArgs {
            validator_key: "unused-in-pure-builder".to_string(),
            network: PalwArtifactNetwork::Testnet110,
            operator_group_id: h(0x40),
            runtime_classes: vec![h(3), h(1), h(2)],
            capacities: vec![ShapeCapacity { shape_id: 7, capacity: 4 }, ShapeCapacity { shape_id: 2, capacity: 1 }],
            reward_key_root: h(0x50),
            amount,
            unbond_delay_epochs: 6,
            out: PathBuf::from("unused"),
        }
    }

    #[test]
    fn provider_bond_artifact_is_canonical_and_submit_compatible() {
        let key = ValidatorKey::from_seed([0x61; 32]);
        let floor = Params::from(PalwArtifactNetwork::Testnet110.network_id()).palw_batch_admission.min_provider_bond_sompi;
        let payload = build_provider_bond_artifact(&key, &args(floor)).unwrap();
        let bond = PalwProviderBondPayloadV1::try_from_slice(&payload).unwrap();

        assert_eq!(bond.owner_public_key, key.public_key());
        assert_eq!(bond.runtime_classes, vec![h(1), h(2), h(3)]);
        assert_eq!(bond.capacity_by_shape, vec![(2, 1), (7, 4)]);
        assert_eq!(bond.amount_sompi, floor);
        let output = kaspa_consensus_core::tx::TransactionOutput::new(floor, provider_bond_lock_spk(key.public_key()));
        assert_eq!(validate_palw_overlay_tx(PROVIDER_BOND_SUBNETWORK_BYTE, &payload, &[output]), Ok(()));
    }

    #[test]
    fn provider_bond_artifact_rejects_registry_noops_and_surprising_delay_clamps() {
        let key = ValidatorKey::from_seed([0x62; 32]);
        let params = Params::from(PalwArtifactNetwork::Testnet110.network_id()).palw_batch_admission;
        let err = build_provider_bond_artifact(&key, &args(params.min_provider_bond_sompi - 1)).unwrap_err();
        assert!(err.contains("omitted from the provider registry"));

        let mut below_delay = args(params.min_provider_bond_sompi);
        below_delay.unbond_delay_epochs = params.provider_unbond_floor_epochs - 1;
        let err = build_provider_bond_artifact(&key, &below_delay).unwrap_err();
        assert!(err.contains("silently clamp"));
    }

    #[test]
    fn capacity_parser_is_strict() {
        assert_eq!(parse_shape_capacity("7=4").unwrap(), ShapeCapacity { shape_id: 7, capacity: 4 });
        assert!(parse_shape_capacity("7:4").is_err());
        assert!(parse_shape_capacity("7=0").is_err());
        assert!(parse_shape_capacity("7=4=2").is_err());
        assert!(parse_shape_capacity("65536=1").is_err());
    }

    #[test]
    fn lifecycle_subcommands_have_stable_cli_names_and_required_shapes() {
        let hash = "11".repeat(64);
        let bond = format!("{hash}:0");
        let facts = PalwPayloadArgs::try_parse_from([
            "palw-payload",
            "audit-facts",
            "--batch-id",
            &hash,
            "--audit-beacon-epoch",
            "5",
            "--out",
            "facts.json",
        ])
        .unwrap();
        assert!(matches!(facts.command, PalwPayloadCommand::AuditFacts(_)));

        let vote = PalwPayloadArgs::try_parse_from([
            "palw-payload",
            "audit-vote",
            "--facts-file",
            "facts.json",
            "--validator-key",
            "validator.key",
            "--auditor-bond",
            &bond,
            "--verdict",
            "pass",
            "--checked-leaf-bitmap-root",
            &hash,
            "--passed-leaf-count",
            "1",
            "--rejected-leaf-bitmap-root",
            &hash,
            "--out",
            "vote.borsh",
        ])
        .unwrap();
        assert!(matches!(vote.command, PalwPayloadCommand::AuditVote(_)));

        let certificate = PalwPayloadArgs::try_parse_from([
            "palw-payload",
            "certificate",
            "--facts-file",
            "facts.json",
            "--vote-file",
            "vote.borsh",
            "--out",
            "certificate.borsh",
        ])
        .unwrap();
        assert!(matches!(certificate.command, PalwPayloadCommand::Certificate(_)));

        assert!(
            PalwPayloadArgs::try_parse_from([
                "palw-payload",
                "certificate",
                "--facts-file",
                "facts.json",
                "--vote-file",
                "vote.borsh",
                "--passed-leaf-count",
                "1",
                "--out",
                "certificate.borsh",
            ])
            .is_err(),
            "certificate assembly must not accept assembler-authored summary fields"
        );
    }
}
