//! PQ wallet commands (Tier B). L1 is PQ-only ML-DSA-87 P2PKH UTXO. These wrap
//! the node wRPC + the consensus-proven tx builders in kaspa-pq-validator-core
//! (the SAME signing path the validator bonds with), adding the large-UTXO
//! remedy:
//!
//!   misaka wallet utxo list  --address misakatest:q… | --key-file …   (read-only, PAGED)
//!   misaka wallet consolidate --key-file …  [--max-inputs 20] [--yes]  (self-spend)
//!   misaka wallet send       --key-file … --to misakatest:q… --amount … [--yes]
//!
//! Keyed ops DEFAULT to a dry-run preview; a live submit requires --yes.

use std::str::FromStr;
use std::time::Duration;

use kaspa_addresses::Address;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_pq_validator_core::{is_spendable, relay_fee_for_compute_mass, ValidatorKey};
use kaspa_rpc_core::{api::rpc::RpcApi, RpcTransaction};
use kaspa_txscript::pay_to_address_script;
use kaspa_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    KaspaRpcClient, WrpcEncoding,
};
use serde_json::json;

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::{exit, CliError, CliResult, OutputFormat};

/// A self funding UTXO already converted to consensus types + its maturity.
struct Funding {
    outpoint: TransactionOutpoint,
    entry: UtxoEntry,
    mature: bool,
    amount: u64,
}

/// One connect + getServerInfo, shared by all wallet commands.
struct NodeView {
    client: KaspaRpcClient,
    params: Params,
    virtual_daa: u64,
    coinbase_maturity: u64,
}

async fn connect(ctx: &Ctx) -> Result<NodeView, CliError> {
    // Derive the borsh endpoint (honor --rpc; else this network's default).
    let net = kaspa_consensus_core::network::NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let hostport = ctx.rpc.clone().unwrap_or_else(|| format!("127.0.0.1:{}", net.network_type().default_borsh_rpc_port()));
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| CliError::new(exit::CONNECTION, format!("build wRPC client: {e}")))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(ctx.timeout_secs.clamp(2, 15))),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client
        .connect(Some(options))
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("connect {url}: {e} (node up with --rpclisten-borsh?)")))?;
    let server = client.get_server_info().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getServerInfo: {e}")))?;
    if server.network_id.to_string() != ctx.network {
        return Err(CliError::new(exit::NETWORK_MISMATCH, format!("node is '{}' but --network is '{}'", server.network_id, ctx.network)));
    }
    if !server.has_utxo_index {
        return Err(CliError::new(exit::GENERIC, "node has no UTXO index (start it with --utxoindex)".to_string()));
    }
    let params = Params::from(server.network_id);
    let coinbase_maturity = params.coinbase_maturity();
    Ok(NodeView { client, params, virtual_daa: server.virtual_daa_score, coinbase_maturity })
}

/// Page the ENTIRE UTXO set of `address` (op 160, ≤1000/page) — never the
/// unbounded get_utxos_by_addresses (that is what blows up on a 951k-UTXO addr).
async fn page_all(nv: &NodeView, address: &Address) -> Result<Vec<Funding>, CliError> {
    let mut out = Vec::new();
    let mut cursor = String::new();
    loop {
        let resp = nv
            .client
            .get_utxos_by_address_page(address.clone(), cursor.clone(), 1000)
            .await
            .map_err(|e| CliError::new(exit::GENERIC, format!("getUtxosByAddressPage: {e}")))?;
        for e in resp.entries {
            let amount = e.utxo_entry.amount;
            let mature = is_spendable(e.utxo_entry.is_coinbase, e.utxo_entry.block_daa_score, nv.virtual_daa, nv.coinbase_maturity);
            out.push(Funding { outpoint: e.outpoint.into(), entry: e.utxo_entry.into(), mature, amount });
        }
        if resp.next_cursor.is_empty() {
            break;
        }
        cursor = resp.next_cursor;
    }
    Ok(out)
}

fn mass_calc(p: &Params) -> MassCalculator {
    MassCalculator::new(p.mass_per_tx_byte, p.mass_per_script_pub_key_byte, p.mass_per_sig_op, p.storage_mass_parameter)
}

/// Mass-based fee for an `n`-input native tx of the given kind (send vs
/// consolidate), built from dummy self-UTXOs (field SIZES drive the mass).
fn estimate_fee(key: &ValidatorKey, p: &Params, n_inputs: usize, consolidate: bool) -> u64 {
    let spk = pay_to_address_script(&key.funding_address(p.prefix()));
    let n = n_inputs.max(1);
    let per = u64::MAX / (2 * n as u64);
    let dummies: Vec<(TransactionOutpoint, UtxoEntry)> = (0..n)
        .map(|i| {
            let mut id = [0u8; 64];
            id[0] = i as u8;
            id[1] = (i >> 8) as u8;
            (TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_bytes(id), 0), UtxoEntry::new(per, spk.clone(), 0, false))
        })
        .collect();
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let built = if consolidate {
        key.build_funded_consolidate_tx(&dummies, floor, p.storage_mass_parameter)
    } else {
        key.build_funded_send_tx(spk, 1, &dummies, floor, p.storage_mass_parameter)
    };
    match built {
        Ok(tx) => relay_fee_for_compute_mass(mass_calc(p).calc_non_contextual_masses(&tx).compute_mass),
        Err(_) => floor,
    }
}

const MAX_INPUTS_PER_TX: usize = 20; // each ML-DSA-87 input ≈ 7 KB; keep the tx within block mass

fn sompi_to_msk(s: u64) -> String {
    format!("{}.{:08}", s / 100_000_000, s % 100_000_000)
}

// ---------------------------------------------------------------------------
// wallet utxo list — read-only
// ---------------------------------------------------------------------------

pub async fn utxo_list(ctx: &Ctx, address: Option<&str>, ks: &KeySource) -> CliResult {
    let nv = connect(ctx).await?;
    let addr = resolve_address(ctx, address, ks, &nv)?;
    let utxos = page_all(&nv, &addr).await?;
    let (mut mature_n, mut mature_sum, mut imm_n, mut imm_sum) = (0u64, 0u64, 0u64, 0u64);
    for u in &utxos {
        if u.mature {
            mature_n += 1;
            mature_sum += u.amount;
        } else {
            imm_n += 1;
            imm_sum += u.amount;
        }
    }
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "address": addr.to_string(), "total": utxos.len(),
                    "mature": { "count": mature_n, "sompi": mature_sum },
                    "immature": { "count": imm_n, "sompi": imm_sum } })
        ),
        OutputFormat::Human => {
            println!("Address      : {addr}");
            println!("UTXOs total  : {}", utxos.len());
            println!("  mature     : {mature_n}  ({} MSK)", sompi_to_msk(mature_sum));
            println!("  immature   : {imm_n}  ({} MSK)  [coinbase < {} blocks]", sompi_to_msk(imm_sum), nv.coinbase_maturity);
            if utxos.len() > MAX_INPUTS_PER_TX {
                println!();
                println!("note: {} UTXOs > {MAX_INPUTS_PER_TX}/tx — `misaka wallet consolidate` merges them in chunks.", utxos.len());
            }
        }
    }
    let _ = nv.client.disconnect().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// wallet consolidate — self-spend, chunked
// ---------------------------------------------------------------------------

pub async fn consolidate(ctx: &Ctx, ks: &KeySource, max_inputs: usize, dry_run: bool, yes: bool) -> CliResult {
    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());
    let max_inputs = max_inputs.clamp(2, MAX_INPUTS_PER_TX);

    let mut mature: Vec<Funding> = page_all(&nv, &addr).await?.into_iter().filter(|u| u.mature).collect();
    if mature.len() < 2 {
        return Err(CliError::new(exit::GENERIC, format!("nothing to consolidate: {} mature UTXO(s) at {addr}", mature.len())));
    }
    // Largest-first is irrelevant for consolidate; keep input order. Chunk it.
    let submit = yes && !dry_run;
    let mut planned = Vec::new();
    let mut remaining = mature.len();
    let chunks: Vec<Vec<Funding>> = {
        let mut v = Vec::new();
        while !mature.is_empty() {
            let take = mature.len().min(max_inputs);
            v.push(mature.drain(..take).collect());
        }
        v
    };
    for (i, chunk) in chunks.iter().enumerate() {
        let n = chunk.len();
        if n < 2 {
            continue; // a 1-UTXO tail is already consolidated
        }
        let fee = estimate_fee(&key, &nv.params, n, true);
        let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = chunk.iter().map(|u| (u.outpoint, u.entry.clone())).collect();
        let sum: u64 = chunk.iter().map(|u| u.amount).sum();
        let tx = key
            .build_funded_consolidate_tx(&fundings, fee, nv.params.storage_mass_parameter)
            .map_err(|e| CliError::new(exit::GENERIC, format!("build consolidate #{i}: {e}")))?;
        let txid = if submit {
            Some(
                nv.client
                    .submit_transaction(RpcTransaction::from(&tx), false)
                    .await
                    .map_err(|e| CliError::new(exit::TX_REJECTED, format!("submit consolidate #{i}: {e}")))?
                    .to_string(),
            )
        } else {
            None
        };
        planned.push((n, sum, fee, sum - fee, txid));
        remaining -= n;
    }

    match ctx.output {
        OutputFormat::Json => {
            let arr: Vec<_> = planned
                .iter()
                .map(|(n, sum, fee, out, txid)| json!({ "inputs": n, "inSompi": sum, "feeSompi": fee, "outSompi": out, "txid": txid }))
                .collect();
            println!("{}", json!({ "ok": true, "dryRun": !submit, "address": addr.to_string(), "txs": arr }));
        }
        OutputFormat::Human => {
            println!("Address      : {addr}");
            println!("Mode         : {}", if submit { "SUBMIT" } else { "dry-run (no submit; pass --yes to broadcast)" });
            for (i, (n, sum, fee, out, txid)) in planned.iter().enumerate() {
                println!("  tx#{i}: {n} inputs, {} MSK in -> {} MSK out (fee {} sompi){}",
                    sompi_to_msk(*sum), sompi_to_msk(*out), fee, txid.as_ref().map(|t| format!("  txid {t}")).unwrap_or_default());
            }
            println!("Result       : {} tx(s){}", planned.len(), if remaining > 0 { format!(", {remaining} UTXO(s) left as a tail", ) } else { String::new() });
        }
    }
    let _ = nv.client.disconnect().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// wallet send — to an arbitrary recipient
// ---------------------------------------------------------------------------

pub async fn send(ctx: &Ctx, ks: &KeySource, to: &str, amount_sompi: u64, dry_run: bool, yes: bool) -> CliResult {
    if amount_sompi == 0 {
        return Err(CliError::new(exit::GENERIC, "--amount must be > 0 (sompi)".to_string()));
    }
    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let from_addr = key.funding_address(nv.params.prefix());
    // recipient must parse for THIS network (prefix guard).
    let to_addr = Address::try_from(to).map_err(|e| CliError::new(exit::GENERIC, format!("bad --to address: {e}")))?;
    if to_addr.prefix != nv.params.prefix() {
        return Err(CliError::new(exit::GENERIC, format!("--to is a {:?} address but --network is {}", to_addr.prefix, ctx.network)));
    }
    let recipient_spk = pay_to_address_script(&to_addr);

    // Largest-first greedy select over MATURE self-UTXOs, re-estimating the fee as inputs are added.
    let mut mature: Vec<Funding> = page_all(&nv, &from_addr).await?.into_iter().filter(|u| u.mature).collect();
    mature.sort_by(|a, b| b.amount.cmp(&a.amount));
    let mut selected: Vec<&Funding> = Vec::new();
    let mut sum = 0u64;
    let mut fee = estimate_fee(&key, &nv.params, 1, false);
    for u in mature.iter() {
        if selected.len() >= MAX_INPUTS_PER_TX {
            break;
        }
        selected.push(u);
        sum += u.amount;
        fee = estimate_fee(&key, &nv.params, selected.len(), false);
        if sum >= amount_sompi.saturating_add(fee) {
            break;
        }
    }
    let needed = amount_sompi.saturating_add(fee);
    if selected.is_empty() || sum < needed {
        return Err(CliError::new(
            exit::GENERIC,
            format!("insufficient mature funds at {from_addr}: have {} MSK across {} UTXO(s) (cap {MAX_INPUTS_PER_TX}), need {} MSK (amount {} + fee {fee}). Consolidate or lower --amount.",
                sompi_to_msk(sum), selected.len(), sompi_to_msk(needed), sompi_to_msk(amount_sompi)),
        ));
    }
    let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = selected.iter().map(|u| (u.outpoint, u.entry.clone())).collect();
    let tx = key
        .build_funded_send_tx(recipient_spk, amount_sompi, &fundings, fee, nv.params.storage_mass_parameter)
        .map_err(|e| CliError::new(exit::GENERIC, format!("build send: {e}")))?;
    let change = sum - needed;
    let submit = yes && !dry_run;
    let txid = if submit {
        Some(
            nv.client
                .submit_transaction(RpcTransaction::from(&tx), false)
                .await
                .map_err(|e| CliError::new(exit::TX_REJECTED, format!("submit send: {e}")))?
                .to_string(),
        )
    } else {
        None
    };
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "dryRun": !submit, "from": from_addr.to_string(), "to": to_addr.to_string(),
                    "amountSompi": amount_sompi, "feeSompi": fee, "changeSompi": change, "inputs": fundings.len(), "txid": txid })
        ),
        OutputFormat::Human => {
            println!("From    : {from_addr}");
            println!("To      : {to_addr}");
            println!("Amount  : {} MSK", sompi_to_msk(amount_sompi));
            println!("Fee     : {fee} sompi   Inputs: {}   Change: {} MSK", fundings.len(), sompi_to_msk(change));
            println!("Mode    : {}", if submit { "SUBMIT" } else { "dry-run (no submit; pass --yes to broadcast)" });
            if let Some(t) = &txid {
                println!("Txid    : {t}");
            }
        }
    }
    let _ = nv.client.disconnect().await;
    Ok(())
}

/// Resolve the address to inspect: explicit --address, else the key's funding address.
fn resolve_address(ctx: &Ctx, address: Option<&str>, ks: &KeySource, nv: &NodeView) -> Result<Address, CliError> {
    match address {
        Some(a) => Address::try_from(a).map_err(|e| CliError::new(exit::GENERIC, format!("bad --address: {e}"))),
        None => {
            if ks.key_file.is_none() && !ks.key_stdin {
                return Err(CliError::new(exit::GENERIC, "pass --address <addr> or a key source (--key-file/--key-stdin)".to_string()));
            }
            let _ = ctx;
            Ok(ks.load_key()?.funding_address(nv.params.prefix()))
        }
    }
}
