//! Staged PALW overlay submission for a closed shared testnet.
//!
//! A PALW lifecycle is past-relative: a manifest must be accepted before its leaf chunks, and all
//! leaves must be accepted before the certificate. This command therefore submits exactly one wire
//! payload and, by default, waits until its change outpoint appears in the node's selected-chain UTXO
//! view. This proves current inclusion, not finality.
//! Operators invoke it once per dependency layer instead of placing a whole lifecycle in the mempool
//! at once (where a miner could include dependent entries in the same block).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw::da::{PalwDaChallengeV1, PalwDaResponseV1};
use kaspa_consensus_core::palw::search_snapshot::{PalwSearchChallengeTxV1, PalwSearchTimeoutTxV1};
use kaspa_consensus_core::palw::{
    PalwBatchManifestV1, PalwLeafChunkV1, PalwProviderBondPayloadV1, PalwProviderUnbondRequestV1, provider_bond_lock_spk,
    ticket_nullifier_commitment, validate_palw_overlay_payload, validate_palw_overlay_tx,
};
use kaspa_consensus_core::subnets::{
    SUBNETWORK_ID_PALW_BATCH_CERT, SUBNETWORK_ID_PALW_BATCH_MANIFEST, SUBNETWORK_ID_PALW_DA_CHALLENGE, SUBNETWORK_ID_PALW_DA_RESPONSE,
    SUBNETWORK_ID_PALW_DA_TIMEOUT_EVIDENCE, SUBNETWORK_ID_PALW_LEAF_CHUNK, SUBNETWORK_ID_PALW_PROVIDER_BOND,
    SUBNETWORK_ID_PALW_PROVIDER_UNBOND, SUBNETWORK_ID_PALW_SEARCH_CHALLENGE, SUBNETWORK_ID_PALW_SEARCH_RESPONSE,
    SUBNETWORK_ID_PALW_SEARCH_TIMEOUT, SubnetworkId,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_core::{info, warn};
use kaspa_pq_validator_core::{
    ATTESTATION_TX_FEE_FLOOR_SOMPI, TicketSecretStore, ValidatorKey, load_validator_seed, parse_stake_bond_ref,
};
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};
use kaspa_wrpc_client::KaspaRpcClient;
use misaka_palw_miner::authorization::ticket_authority_pk_hash;

use super::{MempoolStatus, connect, mempool_status, prefix_for, resolve_node_rpc, top_mature_funding_paged};

const MAX_PALW_FUNDING_INPUTS: usize = 20;
const MEMPOOL_POLL_MILLIS: u64 = 1_000;
/// Mirrors `mining::mempool::check_transaction_standard::MAXIMUM_STANDARD_TRANSACTION_MASS`.
/// Keep a PALW dry-run honest about all three relay axes: compute, transient, and contextual KIP-9
/// storage mass. The consensus block ceiling is checked as an additional (currently looser) cap.
const PALW_STANDARD_TX_MASS_LIMIT: u64 = 480_000;
/// The production PQ relay policy's ML-DSA-87 dust threshold is below 250k sompi. Reusing the
/// overlay fee floor leaves a conservative, spendable change output and keeps dry-run from approving
/// a carrier the node will reject as dust.
const MIN_PALW_CHANGE_SOMPI: u64 = ATTESTATION_TX_FEE_FLOOR_SOMPI;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PalwSubmitKind {
    ProviderBond,
    ProviderUnbond,
    BatchManifest,
    LeafChunk,
    Certificate,
    DaChallenge,
    DaResponse,
    DaTimeout,
    SearchChallenge,
    SearchResponse,
    SearchTimeout,
    RegistryProposal,
    RegistryCert,
    RegistryPolicy,
    RegistryPlan,
    RegistryHalt,
}

impl PalwSubmitKind {
    fn is_compute_registry(self) -> bool {
        matches!(self, Self::RegistryProposal | Self::RegistryCert | Self::RegistryPolicy | Self::RegistryPlan | Self::RegistryHalt)
    }

    fn subnetwork_id(self) -> SubnetworkId {
        match self {
            Self::ProviderBond => SUBNETWORK_ID_PALW_PROVIDER_BOND,
            Self::ProviderUnbond => SUBNETWORK_ID_PALW_PROVIDER_UNBOND,
            Self::BatchManifest => SUBNETWORK_ID_PALW_BATCH_MANIFEST,
            Self::LeafChunk => SUBNETWORK_ID_PALW_LEAF_CHUNK,
            Self::Certificate => SUBNETWORK_ID_PALW_BATCH_CERT,
            Self::DaChallenge => SUBNETWORK_ID_PALW_DA_CHALLENGE,
            Self::DaResponse => SUBNETWORK_ID_PALW_DA_RESPONSE,
            Self::DaTimeout => SUBNETWORK_ID_PALW_DA_TIMEOUT_EVIDENCE,
            Self::SearchChallenge => SUBNETWORK_ID_PALW_SEARCH_CHALLENGE,
            Self::SearchResponse => SUBNETWORK_ID_PALW_SEARCH_RESPONSE,
            Self::SearchTimeout => SUBNETWORK_ID_PALW_SEARCH_TIMEOUT,
            Self::RegistryProposal => kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_COMPUTE_SET_PROPOSAL,
            Self::RegistryCert => kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_COMPUTE_SET_ACTIVATION_CERT,
            Self::RegistryPolicy => kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_COMPUTE_SET_POLICY_UPDATE,
            Self::RegistryPlan => kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_MODEL_ALLOCATION_PLAN,
            Self::RegistryHalt => kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_COMPUTE_SET_EMERGENCY_HALT,
        }
    }

    fn subnetwork_byte(self) -> u8 {
        match self {
            Self::ProviderBond => 0x30,
            Self::ProviderUnbond => 0x37,
            Self::BatchManifest => 0x31,
            Self::LeafChunk => 0x32,
            Self::Certificate => 0x33,
            Self::DaChallenge => 0x3a,
            Self::DaResponse => 0x3b,
            Self::DaTimeout => 0x3c,
            Self::SearchChallenge => 0x3d,
            Self::SearchResponse => 0x3e,
            Self::SearchTimeout => 0x3f,
            Self::RegistryProposal => 0x40,
            Self::RegistryCert => 0x41,
            Self::RegistryPolicy => 0x42,
            Self::RegistryPlan => 0x43,
            Self::RegistryHalt => 0x44,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ProviderBond => "provider-bond",
            Self::ProviderUnbond => "provider-unbond",
            Self::BatchManifest => "batch-manifest",
            Self::LeafChunk => "leaf-chunk",
            Self::Certificate => "certificate",
            Self::DaChallenge => "da-challenge",
            Self::DaResponse => "da-response",
            Self::DaTimeout => "da-timeout",
            Self::SearchChallenge => "search-challenge",
            Self::SearchResponse => "search-response",
            Self::SearchTimeout => "search-timeout",
            Self::RegistryProposal => "registry-proposal",
            Self::RegistryCert => "registry-cert",
            Self::RegistryPolicy => "registry-policy",
            Self::RegistryPlan => "registry-plan",
            Self::RegistryHalt => "registry-halt",
        }
    }
}

/// Submit one consensus-wire PALW payload, then wait for selected-chain acceptance by default.
#[derive(Parser, Debug)]
pub struct PalwSubmitArgs {
    /// Local node wRPC (Borsh) endpoint. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// Funding ML-DSA key. Its mature UTXOs pay the required lock value and transaction fee.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// PALW wire payload kind (provider-bond, provider-unbond, batch-manifest, leaf-chunk,
    /// certificate, da-challenge, da-response, da-timeout). Prefer `palw-provider-unbond request`
    /// for owner-signed unbond construction.
    #[arg(long, value_enum)]
    kind: PalwSubmitKind,

    /// File containing the raw Borsh payload bytes produced by misaka-palw-miner's constructors.
    #[arg(long)]
    payload_file: PathBuf,

    /// Expected node network id. Use testnet-110 or devnet-111 for the PALW presets.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Explicit fee in sompi. Omit to calculate it from the exact payload/output/input shape.
    #[arg(long)]
    fee: Option<u64>,

    /// Maximum mature funding UTXOs to aggregate (1..=20).
    #[arg(long, default_value_t = MAX_PALW_FUNDING_INPUTS)]
    max_inputs: usize,

    /// A locked DNS/PALW bond outpoint which must never be selected as ordinary funding. Repeat for
    /// every bond controlled by the funding key.
    #[arg(long)]
    exclude_funding_outpoint: Vec<String>,

    /// Ticket-authority seed used by every leaf in a leaf-chunk. Required for leaf-chunk submission
    /// unless --unsafe-skip-ticket-secret-check is explicitly set.
    #[arg(long)]
    ticket_authority_key: Option<String>,

    /// Durable TicketSecretStore file. Every leaf's stored raw nullifier must open its on-chain
    /// commitment before the chunk is submitted.
    #[arg(long)]
    ticket_secret_file: Option<PathBuf>,

    /// Permit relaying a leaf chunk without proving this operator has persisted its ticket secrets.
    /// Such leaves can be permanently unmineable; intended only for an explicit third-party relay.
    #[arg(long)]
    unsafe_skip_ticket_secret_check: bool,

    /// Minimum DAA headroom required before submitting a manifest whose registration_epoch equals the
    /// current epoch. Avoids crossing the 100-DAA PALW epoch boundary while the tx is in flight.
    #[arg(long, default_value_t = 20)]
    min_epoch_headroom_daa: u64,

    /// Return after mempool acceptance instead of waiting for selected-chain inclusion of the change
    /// outpoint. This can make a dependent next-stage payload land in the same block and be ignored by
    /// PALW state.
    #[arg(long)]
    no_wait: bool,

    /// Seconds to wait for the change outpoint to enter the selected-chain UTXO index.
    #[arg(long, default_value_t = 120)]
    inclusion_timeout_secs: u64,

    /// Build, sign and validate locally, but do not submit.
    #[arg(long)]
    dry_run: bool,

    /// Accept a node that reports not-synced. A single-node bring-up (fresh re-genesis, no
    /// peers) never reports synced; this is the palw-submit analogue of the node's
    /// --enable-unsynced-mining. Never use it against a network with peers you have not synced.
    #[arg(long)]
    assume_synced: bool,
}

/// Arguments shared by high-level commands which construct a PALW payload in memory and then use
/// the same funding, relay-mass, validation, submission, and selected-chain waiting path as
/// `palw-submit`.
#[derive(Debug)]
pub(crate) struct GeneratedPalwSubmitArgs {
    pub node_rpc: Option<String>,
    pub validator_key: String,
    pub network: Option<String>,
    pub fee: Option<u64>,
    pub max_inputs: usize,
    pub exclude_funding_outpoint: Vec<String>,
    pub no_wait: bool,
    pub inclusion_timeout_secs: u64,
    pub dry_run: bool,
}

pub async fn palw_submit(args: PalwSubmitArgs) -> Result<(), String> {
    let payload =
        std::fs::read(&args.payload_file).map_err(|err| format!("cannot read PALW payload {}: {err}", args.payload_file.display()))?;
    palw_submit_payload(args, payload).await
}

pub(crate) async fn palw_submit_generated(
    args: GeneratedPalwSubmitArgs,
    kind: PalwSubmitKind,
    payload: Vec<u8>,
) -> Result<(), String> {
    palw_submit_payload(
        PalwSubmitArgs {
            node_rpc: args.node_rpc,
            validator_key: args.validator_key,
            kind,
            payload_file: PathBuf::new(),
            network: args.network,
            fee: args.fee,
            max_inputs: args.max_inputs,
            exclude_funding_outpoint: args.exclude_funding_outpoint,
            ticket_authority_key: None,
            ticket_secret_file: None,
            unsafe_skip_ticket_secret_check: false,
            min_epoch_headroom_daa: 20,
            no_wait: args.no_wait,
            inclusion_timeout_secs: args.inclusion_timeout_secs,
            assume_synced: false,
            dry_run: args.dry_run,
        },
        payload,
    )
    .await
}

async fn palw_submit_payload(args: PalwSubmitArgs, payload: Vec<u8>) -> Result<(), String> {
    if !(1..=MAX_PALW_FUNDING_INPUTS).contains(&args.max_inputs) {
        return Err(format!("--max-inputs must be in 1..={MAX_PALW_FUNDING_INPUTS}"));
    }
    if args.fee.is_some_and(|fee| fee < ATTESTATION_TX_FEE_FLOOR_SOMPI) {
        return Err(format!(
            "--fee must be at least {ATTESTATION_TX_FEE_FLOOR_SOMPI} sompi; smaller PALW carriers are below the safe relay floor"
        ));
    }

    if args.kind.is_compute_registry() {
        // ADR-MA band 0x40-0x44: strict canonical decode through the node's OWN admission parser.
        kaspa_consensus_core::palw_compute_set::parse_palw_compute_registry(args.kind.subnetwork_byte(), &payload)
            .map_err(|err| format!("{} payload failed consensus validation: {err}", args.kind.label()))?;
    } else {
        validate_palw_overlay_payload(args.kind.subnetwork_byte(), &payload)
            .map_err(|err| format!("{} payload failed consensus validation: {err}", args.kind.label()))?;
    }

    let mut payer_seed = load_validator_seed(&args.validator_key)?;
    let key = ValidatorKey::from_seed(payer_seed);
    payer_seed.fill(0);
    verify_payload_owner(args.kind, &payload, key.public_key())?;
    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|err| format!("getServerInfo failed: {err}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    if !server.has_utxo_index {
        return Err(format!("node '{node_network}' has no UTXO index; restart kaspad with --utxoindex before PALW submission"));
    }
    if !server.is_synced && !args.assume_synced {
        return Err(
            "node is not synced; refusing to register fork-relative PALW state (pass --assume-synced on a single-node bring-up)"
                .to_string(),
        );
    }
    let params = Params::from(server.network_id);
    if !params.is_palw_active(server.virtual_daa_score) {
        return Err(format!("PALW is not active on {node_network}; use testnet-110 or devnet-111"));
    }
    let prefix = prefix_for(server.network_id.network_type);
    let funding_address = key.funding_address(prefix);

    preflight_payload(&args, &payload, &params, server.virtual_daa_score)?;
    let required_outputs = required_outputs(args.kind, &payload)?;
    let required_total = required_outputs.iter().try_fold(0u64, |sum, output| {
        sum.checked_add(output.value).ok_or_else(|| "PALW required output total overflows u64".to_string())
    })?;

    let excluded: HashSet<TransactionOutpoint> = args
        .exclude_funding_outpoint
        .iter()
        .map(|raw| parse_stake_bond_ref(raw).map_err(|err| format!("invalid --exclude-funding-outpoint '{raw}': {err}")))
        .collect::<Result<_, _>>()?;
    let scan_count = args.max_inputs.saturating_add(excluded.len()).min(256);
    let (candidates, mature_seen) =
        top_mature_funding_paged(&client, &funding_address, server.virtual_daa_score, params.coinbase_maturity(), scan_count, None)
            .await?;

    let mass_calculator = kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&params);
    let mass_limit = PALW_STANDARD_TX_MASS_LIMIT.min(params.max_block_mass);
    let mut fundings: Vec<(TransactionOutpoint, UtxoEntry)> = Vec::new();
    let mut total = 0u64;
    let mut fee = args.fee.unwrap_or(ATTESTATION_TX_FEE_FLOOR_SOMPI);
    let mut prepared: Option<(kaspa_consensus_core::tx::Transaction, u64, u64)> = None;
    let mut last_oversized_storage_mass = None;
    for candidate in candidates.into_iter().filter(|candidate| !excluded.contains(&candidate.outpoint)) {
        if fundings.len() == args.max_inputs {
            break;
        }
        total = total.checked_add(candidate.entry.amount).ok_or_else(|| "funding total overflows u64".to_string())?;
        fundings.push((candidate.outpoint, candidate.entry));
        if args.fee.is_none() {
            fee = key.estimate_overlay_fee_with_outputs_for_inputs(
                &mass_calculator,
                prefix,
                args.kind.subnetwork_id(),
                payload.clone(),
                required_outputs.clone(),
                fundings.len(),
            );
        }
        let target = required_total
            .checked_add(fee)
            .and_then(|value| value.checked_add(MIN_PALW_CHANGE_SOMPI))
            .ok_or_else(|| "required PALW outputs + fee + minimum change overflow u64".to_string())?;
        if total < target {
            continue;
        }

        // KIP-9 storage mass depends on the REAL input and output values. A fixed dust floor is not
        // enough: for example a 500k input returning 250k to a plurality-2 ML-DSA script has millions
        // of storage mass. Build the exact signed candidate, commit its contextual mass before signing,
        // and keep adding the largest available funding entries until every relay mass axis fits.
        let candidate_tx = key.build_funded_overlay_tx_with_outputs_multi_and_storage_mass(
            args.kind.subnetwork_id(),
            payload.clone(),
            required_outputs.clone(),
            &fundings,
            fee,
            params.storage_mass_parameter,
        )?;
        let non_contextual = mass_calculator.calc_non_contextual_masses(&candidate_tx);
        if non_contextual.compute_mass > mass_limit || non_contextual.transient_mass > mass_limit {
            return Err(format!(
                "{}-input {} carrier exceeds the relay mass limit {mass_limit}: compute={} transient={}; reduce payload/input count",
                fundings.len(),
                args.kind.label(),
                non_contextual.compute_mass,
                non_contextual.transient_mass
            ));
        }
        if candidate_tx.mass() > mass_limit {
            last_oversized_storage_mass = Some(candidate_tx.mass());
            continue;
        }
        prepared = Some((candidate_tx, non_contextual.compute_mass, non_contextual.transient_mass));
        break;
    }
    let minimum_shape_fee = key.estimate_overlay_fee_with_outputs_for_inputs(
        &mass_calculator,
        prefix,
        args.kind.subnetwork_id(),
        payload.clone(),
        required_outputs.clone(),
        fundings.len(),
    );
    if fee < minimum_shape_fee {
        return Err(format!(
            "--fee {fee} is below the mass-based safe relay fee {minimum_shape_fee} for this {}-input {} carrier",
            fundings.len(),
            args.kind.label()
        ));
    }
    let needed = required_total.checked_add(fee).ok_or_else(|| "required output total + fee overflows u64".to_string())?;
    let target = needed
        .checked_add(MIN_PALW_CHANGE_SOMPI)
        .ok_or_else(|| "required output total + fee + minimum change overflows u64".to_string())?;
    if fundings.is_empty() || total < target {
        return Err(format!(
            "not enough unlocked MATURE funding at {funding_address}: selected {total} sompi across {} input(s), need at least \
             {target} (required lock {required_total} + fee {fee} + spendable change {MIN_PALW_CHANGE_SOMPI}); scanned {mature_seen} mature UTXO(s). \
             Mine/fund more, wait for maturity, or list locked bond outpoints with --exclude-funding-outpoint.",
            fundings.len()
        ));
    }
    let Some((tx, compute_mass, transient_mass)) = prepared else {
        return Err(format!(
            "available funding leaves this {} carrier above the KIP-9 storage-mass limit {mass_limit} (last candidate: {}); \
             use a larger-value mature funding UTXO or consolidate funds before retrying",
            args.kind.label(),
            last_oversized_storage_mass.map_or_else(|| "not computable".to_string(), |mass| mass.to_string())
        ));
    };

    let required_output_count = required_outputs.len();
    if args.kind.is_compute_registry() {
        kaspa_consensus_core::palw_compute_set::parse_palw_compute_registry(args.kind.subnetwork_byte(), &payload)
            .map_err(|err| format!("built {} carrier failed consensus validation: {err}", args.kind.label()))?;
    } else {
        validate_palw_overlay_tx(args.kind.subnetwork_byte(), &payload, &tx.outputs)
            .map_err(|err| format!("built {} carrier failed consensus validation: {err}", args.kind.label()))?;
    }
    let local_txid = tx.id();
    let change_outpoint = TransactionOutpoint::new(local_txid, required_output_count as u32);
    let change_amount = tx.outputs[required_output_count].value;
    if change_amount < MIN_PALW_CHANGE_SOMPI {
        return Err(format!(
            "internal funding invariant failed: change {change_amount} is below the spendable floor {MIN_PALW_CHANGE_SOMPI}"
        ));
    }

    info!(
        "[kaspa-pq-validator] built PALW {} tx {} from {} input(s): required={} fee={} change={} mass(compute/transient/storage)={}/{}/{}",
        args.kind.label(),
        local_txid,
        fundings.len(),
        required_total,
        fee,
        change_amount,
        compute_mass,
        transient_mass,
        tx.mass()
    );
    if args.dry_run {
        println!("dry_run_txid: {local_txid}");
        println!("predicted_change_outpoint: {change_outpoint}");
        println!("mass_compute: {compute_mass}");
        println!("mass_transient: {transient_mass}");
        println!("mass_storage: {}", tx.mass());
        println!("dry_run_scope: local construction, signatures, all relay mass ceilings, and available contextual preflights only");
        let _ = client.disconnect().await;
        return Ok(());
    }

    let submitted =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|err| format!("submitTransaction failed: {err}"))?;
    if submitted != local_txid {
        return Err(format!("node returned txid {submitted}, but the locally signed transaction id is {local_txid}"));
    }
    println!("{}_txid: {submitted}", args.kind.label().replace('-', "_"));
    if args.kind == PalwSubmitKind::ProviderBond {
        println!("locked_provider_bond_outpoint: {submitted}:0");
    }
    println!("change_outpoint: {change_outpoint}");

    if args.no_wait {
        warn!(
            "[kaspa-pq-validator] {} is only in the mempool; do not submit a dependent PALW layer until {change_outpoint} appears in the selected-chain UTXO view",
            args.kind.label()
        );
    } else {
        wait_for_selected_chain_outpoint(&client, &funding_address, change_outpoint, Duration::from_secs(args.inclusion_timeout_secs))
            .await?;
        println!("carrier_selected_chain_change_outpoint: {change_outpoint}");
        warn!(
            "[kaspa-pq-validator] selected-chain inclusion is not finality and alone does not prove a PALW effect; inspect the provider registry or bounded batch carried-view/blob diagnostics before submitting a dependent layer"
        );
    }
    let _ = client.disconnect().await;
    Ok(())
}

/// Provider lifecycle carriers are single-owner operations. A provider bond locks the payer's coins
/// to the payload key, while an unbond request changes that key's registry eligibility. Never accept
/// an opaque owner-sensitive payload under a different funding key unless a future command adds an
/// explicit, loudly-confirmed sponsorship/relay workflow.
fn verify_payload_owner(kind: PalwSubmitKind, payload: &[u8], payer_public_key: &[u8]) -> Result<(), String> {
    let (owner_public_key, operation) = match kind {
        PalwSubmitKind::ProviderBond => {
            let bond: PalwProviderBondPayloadV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode provider-bond payload after validation: {err}"))?;
            (bond.owner_public_key, "lock this payer's coins to another key")
        }
        PalwSubmitKind::ProviderUnbond => {
            let request: PalwProviderUnbondRequestV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode provider-unbond payload after validation: {err}"))?;
            (request.owner_public_key, "submit an exit request for another key")
        }
        PalwSubmitKind::DaChallenge => {
            let challenge: PalwDaChallengeV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode da-challenge payload after validation: {err}"))?;
            (challenge.challenger_owner_public_key, "submit a bonded DA challenge for another key")
        }
        PalwSubmitKind::DaResponse => {
            let response: PalwDaResponseV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode da-response payload after validation: {err}"))?;
            (response.provider_owner_public_key, "submit a challenged provider response for another key")
        }
        PalwSubmitKind::SearchChallenge => {
            let challenge = PalwSearchChallengeTxV1::decode_strict(payload)
                .map_err(|err| format!("cannot decode search-challenge payload after validation: {err}"))?;
            (challenge.challenger_public_key, "submit a bonded search challenge for another key")
        }
        PalwSubmitKind::SearchTimeout => {
            let timeout = PalwSearchTimeoutTxV1::decode_strict(payload)
                .map_err(|err| format!("cannot decode search-timeout payload after validation: {err}"))?;
            (timeout.reporter_public_key, "submit bonded search timeout evidence for another key")
        }
        // A search response is deliberately unsigned (the verifying chunk proof IS the
        // authorization), so any funder may carry it.
        _ => return Ok(()),
    };
    if owner_public_key != payer_public_key {
        return Err(format!("{} payload owner does not match --validator-key; refusing to {operation}", kind.label()));
    }
    Ok(())
}

fn required_outputs(kind: PalwSubmitKind, payload: &[u8]) -> Result<Vec<TransactionOutput>, String> {
    if kind != PalwSubmitKind::ProviderBond {
        return Ok(Vec::new());
    }
    let bond: PalwProviderBondPayloadV1 =
        borsh::from_slice(payload).map_err(|err| format!("cannot decode provider-bond payload after validation: {err}"))?;
    Ok(vec![TransactionOutput::new(bond.amount_sompi, provider_bond_lock_spk(&bond.owner_public_key))])
}

fn require_secure_existing_ticket_secret_file(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|err| format!("ticket-secret store {} must already exist before leaf submission: {err}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("ticket-secret store {} is not a regular file (symlink/device/fifo refused)", path.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(format!(
                "ticket-secret store {} is group/world-accessible (mode {mode:o}); restrict it to 0600 (chmod 600)",
                path.display()
            ));
        }
    }
    Ok(())
}

fn preflight_payload(args: &PalwSubmitArgs, payload: &[u8], params: &Params, virtual_daa: u64) -> Result<(), String> {
    match args.kind {
        PalwSubmitKind::ProviderBond => {
            let bond: PalwProviderBondPayloadV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode provider-bond payload after validation: {err}"))?;
            let admission = params.palw_batch_admission;
            if bond.amount_sompi < admission.min_provider_bond_sompi {
                return Err(format!(
                    "provider amount {} is below this network's registry floor {}; the carrier could be included but the bond would be omitted",
                    bond.amount_sompi, admission.min_provider_bond_sompi
                ));
            }
            if bond.unbond_delay_epochs < admission.provider_unbond_floor_epochs {
                return Err(format!(
                    "provider unbond delay {} is below this network's floor {}; regenerate the payload with the actual registered delay",
                    bond.unbond_delay_epochs, admission.provider_unbond_floor_epochs
                ));
            }
        }
        PalwSubmitKind::ProviderUnbond => {}
        PalwSubmitKind::BatchManifest => {
            let manifest: PalwBatchManifestV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode batch-manifest payload after validation: {err}"))?;
            let epoch_len = params.palw_epoch_length_daa.max(1);
            let current_epoch = virtual_daa / epoch_len;
            if manifest.registration_epoch != current_epoch {
                return Err(format!(
                    "manifest registration_epoch {} does not equal the node's current PALW epoch {current_epoch}; regenerate it now",
                    manifest.registration_epoch
                ));
            }
            let admission = params.palw_batch_admission;
            if !manifest.admission_valid(
                current_epoch,
                admission.max_batch_leaves,
                admission.max_leaf_chunk_leaves,
                admission.registration_lead_epochs,
                admission.active_window_epochs,
                admission.audit_window_epochs,
                admission.min_leaf_bond_sompi,
            ) {
                return Err(
                    "manifest fails the network's complete contextual admission predicate (content id, leaf/chunk bounds, bond floor, or activation/expiry windows)"
                        .to_string(),
                );
            }
            let headroom = epoch_len - virtual_daa % epoch_len;
            if headroom < args.min_epoch_headroom_daa {
                return Err(format!(
                    "only {headroom} DAA remain in PALW epoch {current_epoch}, below --min-epoch-headroom-daa {}; wait for the next epoch and regenerate the manifest",
                    args.min_epoch_headroom_daa
                ));
            }
        }
        PalwSubmitKind::LeafChunk if !args.unsafe_skip_ticket_secret_check => {
            let key_path = args.ticket_authority_key.as_deref().ok_or_else(|| {
                "--ticket-authority-key is required for leaf-chunk submission (or explicitly use --unsafe-skip-ticket-secret-check)"
                    .to_string()
            })?;
            let store_path = args.ticket_secret_file.clone().ok_or_else(|| {
                "--ticket-secret-file is required for leaf-chunk submission (or explicitly use --unsafe-skip-ticket-secret-check)"
                    .to_string()
            })?;
            let mut seed = load_validator_seed(key_path)?;
            let authority_key = ValidatorKey::from_seed(seed);
            seed.fill(0);
            let authority_hash = ticket_authority_pk_hash(authority_key.public_key());
            require_secure_existing_ticket_secret_file(&store_path)?;
            let store = TicketSecretStore::load_or_empty(store_path, authority_hash)?;
            let chunk: PalwLeafChunkV1 =
                borsh::from_slice(payload).map_err(|err| format!("cannot decode leaf-chunk payload after validation: {err}"))?;
            for leaf in &chunk.leaves {
                if leaf.ticket_authority_pk_hash != authority_hash {
                    return Err(format!(
                        "leaf {} names ticket authority {}, but --ticket-authority-key derives {}",
                        leaf.leaf_index, leaf.ticket_authority_pk_hash, authority_hash
                    ));
                }
                let raw = store.secret_for(&chunk.batch_id, leaf.leaf_index).ok_or_else(|| {
                    format!(
                        "ticket-secret store has no nullifier for batch {} leaf {}; persist it before registration",
                        chunk.batch_id, leaf.leaf_index
                    )
                })?;
                if ticket_nullifier_commitment(&raw) != leaf.ticket_nullifier_commitment {
                    return Err(format!(
                        "stored nullifier does not open ticket commitment for batch {} leaf {}; refusing to register a dead ticket",
                        chunk.batch_id, leaf.leaf_index
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) async fn wait_for_selected_chain_outpoint(
    client: &KaspaRpcClient,
    address: &kaspa_addresses::Address,
    wanted: TransactionOutpoint,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut selected_chain_scan_delay = Duration::from_secs(1);
    loop {
        match mempool_status(client, wanted.transaction_id).await {
            MempoolStatus::Present | MempoolStatus::Unknown => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "transaction was submitted, but outpoint {wanted} did not enter the selected-chain UTXO view within {}s; verify selected-chain inclusion before proceeding",
                        timeout.as_secs()
                    ));
                }
                tokio::time::sleep(Duration::from_millis(MEMPOOL_POLL_MILLIS)).await;
                continue;
            }
            MempoolStatus::Gone => {}
        }
        let mut cursor = String::new();
        loop {
            let page = client
                .get_utxos_by_address_page(address.clone(), cursor, 1000)
                .await
                .map_err(|err| format!("getUtxosByAddressPage failed while waiting for {wanted}: {err}"))?;
            if page.entries.into_iter().any(|entry| TransactionOutpoint::from(entry.outpoint) == wanted) {
                return Ok(());
            }
            if page.next_cursor.is_empty() {
                break;
            }
            cursor = page.next_cursor;
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "transaction was submitted, but outpoint {wanted} did not enter the selected-chain UTXO view within {}s; verify selected-chain inclusion before proceeding",
                timeout.as_secs()
            ));
        }
        tokio::time::sleep(selected_chain_scan_delay).await;
        selected_chain_scan_delay = (selected_chain_scan_delay * 2).min(Duration::from_secs(15));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::Hash64;
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    use kaspa_consensus_core::palw::{PALW_PAYLOAD_VERSION_V1, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT};

    fn submit_args(kind: PalwSubmitKind) -> PalwSubmitArgs {
        PalwSubmitArgs {
            node_rpc: None,
            validator_key: "unused".to_string(),
            kind,
            payload_file: PathBuf::from("unused"),
            network: None,
            fee: None,
            max_inputs: MAX_PALW_FUNDING_INPUTS,
            exclude_funding_outpoint: Vec::new(),
            ticket_authority_key: None,
            ticket_secret_file: None,
            unsafe_skip_ticket_secret_check: false,
            min_epoch_headroom_daa: 20,
            no_wait: false,
            inclusion_timeout_secs: 120,
            assume_synced: false,
            dry_run: true,
        }
    }

    #[test]
    fn provider_bond_preflight_derives_the_consensus_mandated_output_zero() {
        let key = ValidatorKey::from_seed([0x61; 32]);
        let bond = PalwProviderBondPayloadV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            owner_public_key: key.public_key().to_vec(),
            operator_group_id: Hash64::from_bytes([1; 64]),
            runtime_classes: vec![Hash64::from_bytes([2; 64])],
            capacity_by_shape: vec![(1, 1)],
            reward_key_root: Hash64::from_bytes([3; 64]),
            amount_sompi: 42,
            unbond_delay_epochs: 2,
        };
        let payload = borsh::to_vec(&bond).unwrap();
        let outputs = required_outputs(PalwSubmitKind::ProviderBond, &payload).unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].value, 42);
        assert_eq!(outputs[0].script_public_key, provider_bond_lock_spk(key.public_key()));
        assert_eq!(validate_palw_overlay_tx(0x30, &payload, &outputs), Ok(()));
        assert_eq!(verify_payload_owner(PalwSubmitKind::ProviderBond, &payload, key.public_key()), Ok(()));
        let other = ValidatorKey::from_seed([0x62; 32]);
        assert!(
            verify_payload_owner(PalwSubmitKind::ProviderBond, &payload, other.public_key())
                .unwrap_err()
                .contains("does not match --validator-key")
        );
        assert!(required_outputs(PalwSubmitKind::BatchManifest, &[]).unwrap().is_empty());
        assert_eq!(verify_payload_owner(PalwSubmitKind::BatchManifest, &[], other.public_key()), Ok(()));

        let mut unbond = PalwProviderUnbondRequestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            bond_outpoint: TransactionOutpoint::new(Hash64::from_bytes([0x63; 64]), 0),
            owner_public_key: key.public_key().to_vec(),
            signature: Vec::new(),
        };
        let digest = unbond.signing_hash(110);
        unbond.signature = key.sign_with_context(digest.as_bytes().as_slice(), PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT).to_vec();
        let unbond_payload = borsh::to_vec(&unbond).unwrap();
        assert_eq!(validate_palw_overlay_payload(0x37, &unbond_payload), Ok(()));
        assert_eq!(verify_payload_owner(PalwSubmitKind::ProviderUnbond, &unbond_payload, key.public_key()), Ok(()));
        assert!(
            verify_payload_owner(PalwSubmitKind::ProviderUnbond, &unbond_payload, other.public_key())
                .unwrap_err()
                .contains("submit an exit request for another key")
        );
    }

    #[test]
    fn contextual_preflight_rejects_registry_noops_and_invalid_manifest_windows() {
        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 110));
        let admission = params.palw_batch_admission;
        let key = ValidatorKey::from_seed([0x63; 32]);
        let mut bond = PalwProviderBondPayloadV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            owner_public_key: key.public_key().to_vec(),
            operator_group_id: Hash64::from_bytes([1; 64]),
            runtime_classes: vec![Hash64::from_bytes([2; 64])],
            capacity_by_shape: vec![(1, 1)],
            reward_key_root: Hash64::from_bytes([3; 64]),
            amount_sompi: admission.min_provider_bond_sompi - 1,
            unbond_delay_epochs: admission.provider_unbond_floor_epochs,
        };
        let provider_args = submit_args(PalwSubmitKind::ProviderBond);
        assert!(
            preflight_payload(&provider_args, &borsh::to_vec(&bond).unwrap(), &params, 0)
                .unwrap_err()
                .contains("carrier could be included but the bond would be omitted")
        );
        bond.amount_sompi = admission.min_provider_bond_sompi;
        assert_eq!(preflight_payload(&provider_args, &borsh::to_vec(&bond).unwrap(), &params, 0), Ok(()));

        let activation = admission.registration_lead_epochs.saturating_add(admission.audit_window_epochs);
        let mut manifest = PalwBatchManifestV1 {
            version: PALW_PAYLOAD_VERSION_V1,
            batch_id: Hash64::default(),
            registration_epoch: 0,
            model_profile_id: Hash64::from_bytes([4; 64]),
            runtime_class_id: Hash64::from_bytes([5; 64]),
            leaf_count: 1,
            chunk_count: 1,
            leaf_root: Hash64::from_bytes([6; 64]),
            descriptor_root: Hash64::from_bytes([7; 64]),
            total_leaf_bond_sompi: admission.min_leaf_bond_sompi,
            audit_policy_id: Hash64::from_bytes([8; 64]),
            activation_not_before_epoch: activation,
            expiry_epoch: activation + 1,
        };
        manifest.batch_id = manifest.content_id();
        let manifest_args = submit_args(PalwSubmitKind::BatchManifest);
        assert_eq!(preflight_payload(&manifest_args, &borsh::to_vec(&manifest).unwrap(), &params, 0), Ok(()));

        manifest.expiry_epoch = manifest.activation_not_before_epoch.saturating_add(admission.active_window_epochs + 1);
        manifest.batch_id = manifest.content_id();
        assert!(
            preflight_payload(&manifest_args, &borsh::to_vec(&manifest).unwrap(), &params, 0)
                .unwrap_err()
                .contains("complete contextual admission predicate")
        );
    }

    #[test]
    fn storage_mass_rejects_a_dust_only_change_floor_and_accepts_real_headroom() {
        let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 110));
        let calculator = kaspa_consensus_core::mass::MassCalculator::new_with_consensus_params(&params);
        let key = ValidatorKey::from_seed([0x64; 32]);
        let funding_script = provider_bond_lock_spk(key.public_key());
        let outpoint = TransactionOutpoint::new(Hash64::from_bytes([0x65; 64]), 0);
        let build = |amount| {
            key.build_funded_overlay_tx_with_outputs_multi_and_storage_mass(
                PalwSubmitKind::BatchManifest.subnetwork_id(),
                vec![0x66; 32],
                Vec::new(),
                &[(outpoint, UtxoEntry::new(amount, funding_script.clone(), 0, false))],
                ATTESTATION_TX_FEE_FLOOR_SOMPI,
                params.storage_mass_parameter,
            )
            .unwrap()
        };

        let dust_change = build(ATTESTATION_TX_FEE_FLOOR_SOMPI + MIN_PALW_CHANGE_SOMPI);
        assert!(
            dust_change.mass() > PALW_STANDARD_TX_MASS_LIMIT,
            "a fixed 250k change floor is dust-safe but catastrophically unsafe under KIP-9 storage mass"
        );

        let healthy_change = build(10_000_000);
        let non_contextual = calculator.calc_non_contextual_masses(&healthy_change);
        assert!(healthy_change.mass() <= PALW_STANDARD_TX_MASS_LIMIT);
        assert!(non_contextual.compute_mass <= PALW_STANDARD_TX_MASS_LIMIT);
        assert!(non_contextual.transient_mass <= PALW_STANDARD_TX_MASS_LIMIT);
    }

    #[cfg(unix)]
    #[test]
    fn ticket_secret_preflight_requires_an_existing_regular_owner_only_file() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.secrets");
        assert!(require_secure_existing_ticket_secret_file(&missing).unwrap_err().contains("must already exist"));

        let regular = dir.path().join("ticket.secrets");
        std::fs::write(&regular, b"secret-store").unwrap();
        std::fs::set_permissions(&regular, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(require_secure_existing_ticket_secret_file(&regular), Ok(()));

        std::fs::set_permissions(&regular, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(require_secure_existing_ticket_secret_file(&regular).unwrap_err().contains("group/world-accessible"));

        let link = dir.path().join("ticket-link.secrets");
        symlink(&regular, &link).unwrap();
        assert!(require_secure_existing_ticket_secret_file(&link).unwrap_err().contains("not a regular file"));
        assert!(require_secure_existing_ticket_secret_file(dir.path()).unwrap_err().contains("not a regular file"));
    }
}
