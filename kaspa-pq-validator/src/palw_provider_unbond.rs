//! Owner-authorized PALW provider exit and post-delay collateral recovery.
//!
//! The request carrier spends ordinary mature funding and carries a network/bond-bound ML-DSA-87
//! authorization. The provider bond itself is never used for the request fee. Once the registry's
//! release DAA score is reached, `sweep` spends that exact collateral output into the owner's normal
//! funding script with the same native ML-DSA signing path used by the rest of the validator CLI.

use std::time::Duration;

use clap::{Parser, Subcommand};
use kaspa_addresses::Address;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::palw::{
    PALW_PAYLOAD_VERSION_V1, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT, PalwProviderUnbondRequestV1, provider_bond_lock_spk,
    validate_palw_overlay_payload,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_core::{info, warn};
use kaspa_pq_validator_core::{ATTESTATION_TX_FEE_FLOOR_SOMPI, ValidatorKey, load_validator_seed, parse_stake_bond_ref};
use kaspa_rpc_core::{GetPalwStateRequest, RpcPalwProviderBondState, RpcTransaction, api::rpc::RpcApi};
use kaspa_wrpc_client::KaspaRpcClient;

use super::palw_submit::{GeneratedPalwSubmitArgs, PalwSubmitKind, palw_submit_generated, wait_for_selected_chain_outpoint};
use super::{connect, prefix_for, resolve_node_rpc};

const MAX_PALW_FUNDING_INPUTS: usize = 20;
/// Mirrors the node's maximum standard transaction mass. Consensus `max_block_mass` is an
/// additional ceiling and can only make this stricter.
const STANDARD_TX_MASS_LIMIT: u64 = 480_000;

/// Start a provider exit or recover its collateral after the consensus delay.
#[derive(Parser, Debug)]
pub struct PalwProviderUnbondArgs {
    #[command(subcommand)]
    command: PalwProviderUnbondCommand,
}

#[derive(Subcommand, Debug)]
enum PalwProviderUnbondCommand {
    /// Build, owner-sign, fund, and submit a PALW provider-unbond request.
    Request(PalwProviderUnbondRequestArgs),
    /// Spend provider collateral back to its owner after the registry's release DAA score.
    Sweep(PalwProviderSweepArgs),
}

#[derive(Parser, Debug)]
struct PalwProviderUnbondRequestArgs {
    /// Local node wRPC (Borsh) endpoint. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// ML-DSA-87 key which owns the provider bond and pays the request carrier fee.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// Provider collateral outpoint (`txid:index`, normally output 0).
    #[arg(long)]
    provider_bond: String,

    /// Expected node network id. Refuses to sign on mismatch because the signature is network-bound.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Explicit carrier fee in sompi. Omit to calculate it from the exact request/input shape.
    #[arg(long)]
    fee: Option<u64>,

    /// Maximum mature fee-funding UTXOs to aggregate (1..=20).
    #[arg(long, default_value_t = MAX_PALW_FUNDING_INPUTS)]
    max_inputs: usize,

    /// Another locked DNS/PALW bond which must not fund this carrier. Repeat as needed. The provider
    /// bond named by --provider-bond is always excluded automatically.
    #[arg(long)]
    exclude_funding_outpoint: Vec<String>,

    /// Return after mempool acceptance instead of waiting for selected-chain inclusion.
    #[arg(long)]
    no_wait: bool,

    /// Seconds to wait for the carrier change outpoint to enter the selected-chain UTXO view.
    #[arg(long, default_value_t = 120)]
    inclusion_timeout_secs: u64,

    /// Build, sign, validate, and perform live registry/funding preflights without submitting.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Parser, Debug)]
struct PalwProviderSweepArgs {
    /// Local node wRPC (Borsh) endpoint. The node must run --utxoindex.
    #[arg(long = "node-wrpc-borsh", visible_alias = "node-rpc", env = "KASPA_PQ_NODE_RPC")]
    node_rpc: Option<String>,

    /// ML-DSA-87 key which owns and unlocks the provider collateral output.
    #[arg(long, env = "KASPA_PQ_VALIDATOR_KEY")]
    validator_key: String,

    /// Provider collateral outpoint (`txid:index`, normally output 0).
    #[arg(long)]
    provider_bond: String,

    /// Expected node network id; refuses on mismatch.
    #[arg(long, visible_alias = "network-id", env = "KASPA_PQ_NETWORK")]
    network: Option<String>,

    /// Explicit native sweep fee in sompi. Omit to calculate it from the one-input/one-output shape.
    #[arg(long)]
    fee: Option<u64>,

    /// Return after mempool acceptance instead of waiting for the recovered output.
    #[arg(long)]
    no_wait: bool,

    /// Seconds to wait for the recovered output to enter the selected-chain UTXO view.
    #[arg(long, default_value_t = 120)]
    inclusion_timeout_secs: u64,

    /// Build, sign, and validate the sweep against live registry/UTXO state without submitting.
    #[arg(long)]
    dry_run: bool,
}

pub async fn palw_provider_unbond(args: PalwProviderUnbondArgs) -> Result<(), String> {
    match args.command {
        PalwProviderUnbondCommand::Request(args) => request(args).await,
        PalwProviderUnbondCommand::Sweep(args) => sweep(args).await,
    }
}

async fn request(args: PalwProviderUnbondRequestArgs) -> Result<(), String> {
    let bond_outpoint =
        parse_stake_bond_ref(&args.provider_bond).map_err(|err| format!("invalid --provider-bond '{}': {err}", args.provider_bond))?;
    let mut seed = load_validator_seed(&args.validator_key)?;
    let key = ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);

    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|err| format!("getServerInfo failed: {err}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'; refusing to sign"));
    }
    if !server.has_utxo_index {
        return Err(format!("node '{node_network}' has no UTXO index; restart kaspad with --utxoindex"));
    }
    if !server.is_synced {
        return Err("node is not synced; refusing to sign against stale provider state".to_string());
    }
    let params = Params::from(server.network_id);
    if !params.is_palw_active(server.virtual_daa_score) {
        return Err(format!("PALW is not active on {node_network}; use a PALW-enabled network"));
    }

    let state = client
        .get_palw_state(GetPalwStateRequest {
            batch_id: None,
            provider_bond_outpoint: Some(outpoint_ref(bond_outpoint)),
            pcpb_anchor_epoch: None,
            pcpb_a_commit: None,
        })
        .await
        .map_err(|err| format!("getPalwState failed: {err}"))?;
    require_provider_view(state.enabled, state.overlay_view_available)?;
    let provider = state
        .provider_bond
        .as_ref()
        .ok_or_else(|| format!("provider bond {bond_outpoint} is not present in the node's selected-chain registry view"))?;
    validate_provider_for_request(provider, bond_outpoint, &key.validator_id)?;

    let request = build_signed_request(&key, bond_outpoint, server.network_id.suffix().unwrap_or(0))?;
    let payload = borsh::to_vec(&request).map_err(|err| format!("cannot encode provider-unbond request: {err}"))?;
    validate_palw_overlay_payload(0x37, &payload)
        .map_err(|err| format!("built provider-unbond payload failed consensus validation: {err}"))?;

    println!("provider_unbond_bond_outpoint: {}", outpoint_ref(bond_outpoint));
    println!("provider_unbond_owner: {}", key.validator_id);
    println!("provider_status_before_request: {}", provider.effective_status);
    println!("provider_unbond_signing_network_suffix: {}", server.network_id.suffix().unwrap_or(0));
    let _ = client.disconnect().await;

    let mut exclusions = args.exclude_funding_outpoint;
    let already_excluded =
        exclusions.iter().filter_map(|raw| parse_stake_bond_ref(raw).ok()).any(|outpoint| outpoint == bond_outpoint);
    if !already_excluded {
        exclusions.push(outpoint_ref(bond_outpoint));
    }
    palw_submit_generated(
        GeneratedPalwSubmitArgs {
            node_rpc: args.node_rpc,
            validator_key: args.validator_key,
            network: args.network,
            fee: args.fee,
            max_inputs: args.max_inputs,
            exclude_funding_outpoint: exclusions,
            no_wait: args.no_wait,
            inclusion_timeout_secs: args.inclusion_timeout_secs,
            dry_run: args.dry_run,
        },
        PalwSubmitKind::ProviderUnbond,
        payload,
    )
    .await
}

async fn sweep(args: PalwProviderSweepArgs) -> Result<(), String> {
    if args.fee.is_some_and(|fee| fee < ATTESTATION_TX_FEE_FLOOR_SOMPI) {
        return Err(format!("--fee must be at least {ATTESTATION_TX_FEE_FLOOR_SOMPI} sompi"));
    }
    let bond_outpoint =
        parse_stake_bond_ref(&args.provider_bond).map_err(|err| format!("invalid --provider-bond '{}': {err}", args.provider_bond))?;
    let mut seed = load_validator_seed(&args.validator_key)?;
    let key = ValidatorKey::from_seed(seed);
    seed.fill(0);
    std::hint::black_box(&seed);

    let client = connect(&resolve_node_rpc(&args.network, &args.node_rpc)).await?;
    let server = client.get_server_info().await.map_err(|err| format!("getServerInfo failed: {err}"))?;
    let node_network = server.network_id.to_string();
    if let Some(expected) = args.network.as_deref()
        && node_network != expected
    {
        return Err(format!("network mismatch: node is '{node_network}' but --network is '{expected}'"));
    }
    if !server.has_utxo_index {
        return Err(format!("node '{node_network}' has no UTXO index; restart kaspad with --utxoindex"));
    }
    if !server.is_synced {
        return Err("node is not synced; refusing to sweep against stale provider state".to_string());
    }
    let params = Params::from(server.network_id);
    if !params.is_palw_active(server.virtual_daa_score) {
        return Err(format!("PALW is not active on {node_network}; use a PALW-enabled network"));
    }

    let state = client
        .get_palw_state(GetPalwStateRequest {
            batch_id: None,
            provider_bond_outpoint: Some(outpoint_ref(bond_outpoint)),
            pcpb_anchor_epoch: None,
            pcpb_a_commit: None,
        })
        .await
        .map_err(|err| format!("getPalwState failed: {err}"))?;
    require_provider_view(state.enabled, state.overlay_view_available)?;
    let provider = state
        .provider_bond
        .as_ref()
        .ok_or_else(|| format!("provider bond {bond_outpoint} is not present in the node's selected-chain registry view"))?;
    let release_daa = validate_provider_for_sweep(provider, bond_outpoint, &key.validator_id, state.sink_daa_score)?;

    let prefix = prefix_for(server.network_id.network_type);
    let funding_address = key.funding_address(prefix);
    let bond_entry = find_address_utxo(&client, &funding_address, bond_outpoint).await?;
    if bond_entry.amount != provider.amount_sompi {
        return Err(format!(
            "provider registry amount {} does not match collateral UTXO amount {}; refusing to sweep inconsistent state",
            provider.amount_sompi, bond_entry.amount
        ));
    }
    let expected_script = provider_bond_lock_spk(key.public_key());
    if bond_entry.script_public_key != expected_script {
        return Err("provider collateral UTXO script does not match --validator-key; refusing to sign".to_string());
    }
    if bond_entry.is_coinbase {
        return Err("provider collateral UTXO is unexpectedly marked coinbase; refusing to sign".to_string());
    }

    let mass_calculator = MassCalculator::new_with_consensus_params(&params);
    let minimum_fee = key.estimate_overlay_fee(&mass_calculator, prefix, SUBNETWORK_ID_NATIVE, Vec::new());
    let fee = args.fee.unwrap_or(minimum_fee);
    if fee < minimum_fee {
        return Err(format!("--fee {fee} is below the mass-based safe relay fee {minimum_fee} for the provider sweep"));
    }
    if bond_entry.amount <= fee {
        return Err(format!("provider collateral {} does not cover sweep fee {fee}", bond_entry.amount));
    }
    let tx = key.build_funded_consolidate_tx(&[(bond_outpoint, bond_entry)], fee, params.storage_mass_parameter)?;
    let masses = mass_calculator.calc_non_contextual_masses(&tx);
    let mass_limit = STANDARD_TX_MASS_LIMIT.min(params.max_block_mass);
    if masses.compute_mass > mass_limit || masses.transient_mass > mass_limit || tx.mass() > mass_limit {
        return Err(format!(
            "provider sweep exceeds relay mass limit {mass_limit}: compute={} transient={} storage={}",
            masses.compute_mass,
            masses.transient_mass,
            tx.mass()
        ));
    }

    let txid = tx.id();
    let recovered_outpoint = TransactionOutpoint::new(txid, 0);
    let recovered_amount = tx.outputs[0].value;
    info!(
        "[kaspa-pq-validator] built provider collateral sweep {txid}: bond={bond_outpoint} release_daa={release_daa} fee={fee} recovered={recovered_amount} mass(compute/transient/storage)={}/{}/{}",
        masses.compute_mass,
        masses.transient_mass,
        tx.mass()
    );
    if args.dry_run {
        println!("dry_run_txid: {txid}");
        println!("provider_release_daa_score: {release_daa}");
        println!("provider_state_sink_daa_score: {}", state.sink_daa_score);
        println!("predicted_recovered_outpoint: {}", outpoint_ref(recovered_outpoint));
        println!("predicted_recovered_amount_sompi: {recovered_amount}");
        println!("mass_compute: {}", masses.compute_mass);
        println!("mass_transient: {}", masses.transient_mass);
        println!("mass_storage: {}", tx.mass());
        println!(
            "dry_run_scope: live owner/status/release/UTXO checks plus local native construction, signature, fee, and relay-mass checks"
        );
        let _ = client.disconnect().await;
        return Ok(());
    }

    let submitted =
        client.submit_transaction(RpcTransaction::from(&tx), false).await.map_err(|err| format!("submitTransaction failed: {err}"))?;
    if submitted != txid {
        return Err(format!("node returned txid {submitted}, but the locally signed transaction id is {txid}"));
    }
    println!("provider_sweep_txid: {submitted}");
    println!("recovered_outpoint: {}", outpoint_ref(recovered_outpoint));
    println!("recovered_amount_sompi: {recovered_amount}");
    if args.no_wait {
        warn!("[kaspa-pq-validator] provider sweep is only in the mempool; collateral recovery is not yet selected-chain included");
    } else {
        wait_for_selected_chain_outpoint(
            &client,
            &funding_address,
            recovered_outpoint,
            Duration::from_secs(args.inclusion_timeout_secs),
        )
        .await?;
        println!("provider_sweep_selected_chain_outpoint: {}", outpoint_ref(recovered_outpoint));
    }
    let _ = client.disconnect().await;
    Ok(())
}

fn build_signed_request(
    key: &ValidatorKey,
    bond_outpoint: TransactionOutpoint,
    network_suffix: u32,
) -> Result<PalwProviderUnbondRequestV1, String> {
    let mut request = PalwProviderUnbondRequestV1 {
        version: PALW_PAYLOAD_VERSION_V1,
        bond_outpoint,
        owner_public_key: key.public_key().to_vec(),
        signature: Vec::new(),
    };
    let digest = request.signing_hash(network_suffix);
    request.signature = key.sign_with_context(digest.as_bytes().as_slice(), PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT).to_vec();
    if !key.verify_with_context(digest.as_bytes().as_slice(), &request.signature, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT) {
        return Err("internal provider-unbond signature self-check failed".to_string());
    }
    Ok(request)
}

fn require_provider_view(enabled: bool, available: bool) -> Result<(), String> {
    if !enabled {
        return Err("PALW state RPC reports the overlay disabled".to_string());
    }
    if !available {
        return Err("PALW selected-chain provider view is unavailable; refusing an ownership-sensitive operation".to_string());
    }
    Ok(())
}

fn outpoint_ref(outpoint: TransactionOutpoint) -> String {
    format!("{}:{}", outpoint.transaction_id, outpoint.index)
}

fn validate_provider_identity(
    provider: &RpcPalwProviderBondState,
    bond_outpoint: TransactionOutpoint,
    owner_id: &Hash64,
) -> Result<(), String> {
    let returned_outpoint = parse_stake_bond_ref(&provider.bond_outpoint)
        .map_err(|err| format!("node returned malformed provider bond outpoint '{}': {err}", provider.bond_outpoint))?;
    if returned_outpoint != bond_outpoint {
        return Err(format!(
            "node returned provider bond {returned_outpoint}, but the requested bond is {bond_outpoint}; refusing mismatched state"
        ));
    }
    if provider.owner_pubkey_hash != owner_id.to_string() {
        return Err(format!(
            "provider bond {bond_outpoint} is owned by {}, but --validator-key derives {owner_id}",
            provider.owner_pubkey_hash
        ));
    }
    Ok(())
}

fn validate_provider_for_request(
    provider: &RpcPalwProviderBondState,
    bond_outpoint: TransactionOutpoint,
    owner_id: &Hash64,
) -> Result<(), String> {
    validate_provider_identity(provider, bond_outpoint, owner_id)?;
    if provider.slashed_at_daa_score.is_some() {
        return Err(format!("provider bond {bond_outpoint} has a slashing record and cannot use the owner exit path"));
    }
    if provider.unbond_request_daa_score.is_some() {
        return Err(format!(
            "provider bond {bond_outpoint} already has an unbond request (release DAA {}); do not submit a duplicate request",
            provider.release_daa_score.map_or_else(|| "unknown".to_string(), |score| score.to_string())
        ));
    }
    match provider.effective_status.as_str() {
        "pending" | "active" => Ok(()),
        "unbonding" => Err(format!(
            "provider bond {bond_outpoint} is already unbonding (release DAA {}); do not submit a duplicate request",
            provider.release_daa_score.map_or_else(|| "unknown".to_string(), |score| score.to_string())
        )),
        "slashed" => Err(format!("provider bond {bond_outpoint} is slashed and cannot use the owner exit path")),
        status => Err(format!("provider bond {bond_outpoint} has unknown status '{status}'; refusing to sign")),
    }
}

fn validate_provider_for_sweep(
    provider: &RpcPalwProviderBondState,
    bond_outpoint: TransactionOutpoint,
    owner_id: &Hash64,
    sink_daa_score: u64,
) -> Result<u64, String> {
    validate_provider_identity(provider, bond_outpoint, owner_id)?;
    if provider.slashed_at_daa_score.is_some() || provider.effective_status == "slashed" {
        return Err(format!("provider bond {bond_outpoint} is slashed; owner collateral recovery is forbidden"));
    }
    if provider.effective_status != "unbonding" {
        return Err(format!(
            "provider bond {bond_outpoint} status is '{}', not 'unbonding'; submit and confirm an owner-authorized request first",
            provider.effective_status
        ));
    }
    if provider.unbond_request_daa_score.is_none() {
        return Err(format!("provider bond {bond_outpoint} is unbonding but has no request DAA score; refusing inconsistent state"));
    }
    let release_daa = provider
        .release_daa_score
        .ok_or_else(|| format!("provider bond {bond_outpoint} is unbonding but has no release DAA score"))?;
    if sink_daa_score < release_daa {
        return Err(format!(
            "provider bond {bond_outpoint} is still locked: selected-chain provider view is at DAA {sink_daa_score}, release is DAA {release_daa} ({} remaining)",
            release_daa - sink_daa_score
        ));
    }
    Ok(release_daa)
}

async fn find_address_utxo(client: &KaspaRpcClient, address: &Address, wanted: TransactionOutpoint) -> Result<UtxoEntry, String> {
    let mut cursor = String::new();
    let mut scanned = 0usize;
    loop {
        let page = client
            .get_utxos_by_address_page(address.clone(), cursor.clone(), 1000)
            .await
            .map_err(|err| format!("getUtxosByAddressPage failed while resolving provider collateral {wanted}: {err}"))?;
        for entry in page.entries {
            scanned += 1;
            if TransactionOutpoint::from(entry.outpoint) == wanted {
                return Ok(UtxoEntry::from(entry.utxo_entry));
            }
        }
        if page.next_cursor.is_empty() {
            break;
        }
        if page.next_cursor == cursor {
            return Err("getUtxosByAddressPage returned a non-advancing cursor".to_string());
        }
        cursor = page.next_cursor;
    }
    Err(format!(
        "provider collateral {wanted} is not an unspent output at owner address {address} (scanned {scanned} UTXOs); it may already be spent or the UTXO view may not yet include it"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bond(byte: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([byte; 64]), 0)
    }

    fn provider(key: &ValidatorKey, outpoint: TransactionOutpoint, status: &str) -> RpcPalwProviderBondState {
        RpcPalwProviderBondState {
            bond_outpoint: outpoint_ref(outpoint),
            owner_pubkey_hash: key.validator_id.to_string(),
            amount_sompi: 1_000_000_000,
            effective_status: status.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn signed_request_is_consensus_shaped_and_bound_to_network_and_bond() {
        let key = ValidatorKey::from_seed([0x71; 32]);
        let outpoint = bond(0x72);
        let request = build_signed_request(&key, outpoint, 110).unwrap();
        let payload = borsh::to_vec(&request).unwrap();
        assert_eq!(validate_palw_overlay_payload(0x37, &payload), Ok(()));
        let digest = request.signing_hash(110);
        assert!(key.verify_with_context(digest.as_bytes().as_slice(), &request.signature, PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT));

        let wrong_network = request.signing_hash(111);
        assert!(!key.verify_with_context(
            wrong_network.as_bytes().as_slice(),
            &request.signature,
            PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT
        ));
        let mut wrong_bond = request.clone();
        wrong_bond.bond_outpoint = bond(0x73);
        let wrong_bond_digest = wrong_bond.signing_hash(110);
        assert!(!key.verify_with_context(
            wrong_bond_digest.as_bytes().as_slice(),
            &request.signature,
            PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT
        ));
    }

    #[test]
    fn provider_state_checks_owner_status_and_release_boundary() {
        let key = ValidatorKey::from_seed([0x74; 32]);
        let outpoint = bond(0x75);
        let mut state = provider(&key, outpoint, "active");
        assert_eq!(validate_provider_for_request(&state, outpoint, &key.validator_id), Ok(()));

        let other = ValidatorKey::from_seed([0x76; 32]);
        assert!(validate_provider_for_request(&state, outpoint, &other.validator_id).unwrap_err().contains("is owned by"));
        state.effective_status = "unbonding".to_string();
        state.unbond_request_daa_score = Some(400);
        state.release_daa_score = Some(500);
        assert!(validate_provider_for_request(&state, outpoint, &key.validator_id).unwrap_err().contains("already"));
        assert!(validate_provider_for_sweep(&state, outpoint, &key.validator_id, 499).unwrap_err().contains("still locked"));
        assert_eq!(validate_provider_for_sweep(&state, outpoint, &key.validator_id, 500), Ok(500));

        state.slashed_at_daa_score = Some(450);
        assert!(validate_provider_for_sweep(&state, outpoint, &key.validator_id, 500).unwrap_err().contains("slashed"));
    }

    #[test]
    fn sweep_shape_returns_released_collateral_to_the_owner() {
        let key = ValidatorKey::from_seed([0x78; 32]);
        let outpoint = bond(0x79);
        let params = Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(
            kaspa_consensus_core::network::NetworkType::Testnet,
            110,
        ));
        let amount = 1_000_000_000;
        let fee = 300_000;
        let lock = provider_bond_lock_spk(key.public_key());
        let tx = key
            .build_funded_consolidate_tx(
                &[(outpoint, UtxoEntry::new(amount, lock.clone(), 10, false))],
                fee,
                params.storage_mass_parameter,
            )
            .unwrap();
        assert_eq!(tx.subnetwork_id, SUBNETWORK_ID_NATIVE);
        assert_eq!(tx.inputs.len(), 1);
        assert_eq!(tx.inputs[0].previous_outpoint, outpoint);
        assert_eq!(tx.outputs.len(), 1);
        assert_eq!(tx.outputs[0].value, amount - fee);
        assert_eq!(tx.outputs[0].script_public_key, lock);
        assert!(tx.mass() > 0);
    }

    #[test]
    fn nested_cli_parses_request_and_sweep_dry_runs() {
        let outpoint = outpoint_ref(bond(0x77));
        let parsed = PalwProviderUnbondArgs::try_parse_from([
            "palw-provider-unbond",
            "request",
            "--validator-key",
            "owner.seed",
            "--provider-bond",
            &outpoint,
            "--network",
            "testnet-110",
            "--dry-run",
        ])
        .unwrap();
        assert!(matches!(parsed.command, PalwProviderUnbondCommand::Request(PalwProviderUnbondRequestArgs { dry_run: true, .. })));

        let parsed = PalwProviderUnbondArgs::try_parse_from([
            "palw-provider-unbond",
            "sweep",
            "--validator-key",
            "owner.seed",
            "--provider-bond",
            &outpoint,
            "--dry-run",
        ])
        .unwrap();
        assert!(matches!(parsed.command, PalwProviderUnbondCommand::Sweep(PalwProviderSweepArgs { dry_run: true, .. })));
    }
}
