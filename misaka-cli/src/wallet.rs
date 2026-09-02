//! PQ wallet commands (Tier B). L1 is PQ-only ML-DSA-87 P2PKH UTXO. These wrap
//! the node wRPC + the consensus-proven tx builders in kaspa-pq-validator-core
//! (the SAME signing path the validator bonds with), adding the large-UTXO
//! remedy:
//!
//!   misaka wallet utxo list  --address misakatest:q… | --key-file …   (read-only, PAGED)
//!   misaka wallet utxo consolidate --key-file … [--max-inputs 20] [--max-txs-per-run 100] [--yes]
//!   misaka wallet send       --key-file … --to misakatest:q… --amount … [--yes]
//!
//! Keyed ops DEFAULT to a dry-run preview; a live submit requires --yes.

use std::str::FromStr;
use std::time::Duration;

use kaspa_addresses::Address;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::mass::MassCalculator;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_pq_validator_core::{ValidatorKey, is_spendable_settled, relay_fee_for_compute_mass};
use kaspa_rpc_core::{RpcTransaction, api::rpc::RpcApi};
use kaspa_txscript::pay_to_address_script;
use kaspa_wrpc_client::{
    KaspaRpcClient, WrpcEncoding,
    client::{ConnectOptions, ConnectStrategy},
};
use serde_json::json;

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

/// A self funding UTXO already converted to consensus types + its maturity.
pub(crate) struct Funding {
    pub(crate) outpoint: TransactionOutpoint,
    pub(crate) entry: UtxoEntry,
    pub(crate) mature: bool,
    pub(crate) amount: u64,
    /// This outpoint is a validator StakeBond whose collateral consensus still LOCKS, as the NODE
    /// reports it. A bond past its unbonding period is not marked — see `locked_bond_outpoints`.
    ///
    /// Spending it is never what an operator meant: the block carrying the spend is disqualified
    /// from the chain, and where the mergeset spend gate is not armed it is accepted anyway and the
    /// bond record survives with no backing (audit M1-1). Every other spender in this tree already
    /// excludes it — `validator_service.rs` calls it "a validator self-wedge", and the sidecar
    /// threads an exclusion through bond, unbond and equivocate. The wallet, which wraps the SAME
    /// signing path a validator bonds with, did not (audit M1-3): the bond is typically the largest
    /// UTXO at that address, and selection is largest-first.
    pub(crate) bonded: bool,
}

/// One connect + getServerInfo, shared by all wallet commands.
pub(crate) struct NodeView {
    pub(crate) client: KaspaRpcClient,
    pub(crate) params: Params,
    pub(crate) virtual_daa: u64,
    coinbase_maturity: u64,
    /// **The second gate on a coinbase spend** (ADR-0018), which the maturity floor is not.
    ///
    /// A wallet that checks only `coinbase_maturity` offers money the node refuses: on testnet-11
    /// the floor is 1 and this is 600, so every coinbase younger than 600 DAA read as spendable
    /// and the send was rejected for spending an immature UTXO. `0` is the feature off.
    settlement_long_maturity_daa: u64,
}

pub(crate) async fn connect(ctx: &Ctx) -> Result<NodeView, CliError> {
    // Derive the Borsh endpoint: explicit --rpc wins, else the local endpoint registry,
    // else this network's default loopback port.
    let net = NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let registry = misaka_endpoints::EndpointRegistry::load(&ctx.network);
    let hostport = misaka_endpoints::resolve(&net, EndpointKind::NodeWrpcBorsh, ctx.rpc.as_deref(), registry.as_ref());
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
        return Err(CliError::new(
            exit::NETWORK_MISMATCH,
            format!("node is '{}' but --network is '{}'", server.network_id, ctx.network),
        ));
    }
    if !server.has_utxo_index {
        return Err(CliError::new(exit::GENERIC, "node has no UTXO index (start it with --utxoindex)".to_string()));
    }
    let params = Params::from(server.network_id);
    let coinbase_maturity = params.coinbase_maturity();
    let settlement_long_maturity_daa = params.dns_params.as_ref().map_or(0, |d| d.coinbase_settlement_long_maturity_daa);
    Ok(NodeView { client, params, virtual_daa: server.virtual_daa_score, coinbase_maturity, settlement_long_maturity_daa })
}

/// Page the ENTIRE UTXO set of `address` (op 160, ≤1000/page) — never the
/// unbounded get_utxos_by_addresses (that is what blows up on a 951k-UTXO addr).
/// Every outpoint the node reports as a StakeBond whose collateral consensus still LOCKS, at any
/// owner.
///
/// Deliberately unfiltered by owner: the wallet may hold a key that is not the bond's declared
/// owner, and a still-locked outpoint must not be selected. A failure here is surfaced rather than
/// swallowed — a wallet that cannot ask which of its outputs are locked must not guess, because the
/// guess it made before this existed was "none of them".
///
/// **It is the LOCK that is mirrored here, not the existence of a bond** (re-audit R-4). Excluding
/// every bond at every status was too strong in the one direction that costs an honest operator
/// everything: `BondStatus` has no terminal "withdrawn" state (`dns_finality.rs:344-358`), so a
/// bond that has completed its unbonding period keeps its record and keeps being returned here —
/// while consensus positively ALLOWS the spend (`PalwSpendLocks::locks`,
/// `utxo_validation.rs:238-250`, is false exactly when the bond is `Unbonding` and past its release
/// height). The sidecar's `unbond` only files the request and explicitly refuses to touch output-0,
/// so `wallet send` is the only shipped way to reclaim it. Excluding it unconditionally stranded a
/// mainnet validator's 20M KAS behind a hand-built transaction.
///
/// So the predicate below is `locks`, read back: skip a bond that is releasable at the node's
/// current DAA, exclude every other one.
async fn locked_bond_outpoints(nv: &NodeView) -> Result<std::collections::HashSet<TransactionOutpoint>, CliError> {
    let mut out = std::collections::HashSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let resp = nv
            .client
            .get_stake_bonds(kaspa_rpc_core::GetStakeBondsRequest {
                owner_pubkey_hash: None,
                status_in: None,
                cursor: cursor.clone(),
                limit: 1000,
                // `None` = the sink, which is what `effective_status` below is resolved against and
                // what `virtual_daa` is compared to. Asking at one height and judging at another is
                // how a released bond would read as locked again.
                pov_daa_score: None,
            })
            .await
            .map_err(|e| {
                CliError::new(
                    exit::GENERIC,
                    format!(
                        "getStakeBonds: {e} — refusing to select inputs without knowing which outputs are bonded collateral \
                         (spending a bond disqualifies the carrying block and can leave the bond record unbacked)"
                    ),
                )
            })?;
        for b in resp.bonds {
            // **A shape this wallet does not understand is an error, not a silent pass** (re-audit
            // R-5). Dropping an unparseable entry left exactly the outpoint this function exists to
            // exclude selectable, which is the opposite of the fail-closed contract above.
            let outpoint = parse_outpoint_str(&b.bond_outpoint).ok_or_else(|| {
                CliError::new(
                    exit::GENERIC,
                    format!(
                        "getStakeBonds returned bond outpoint '{}', which is not 'txid_hex:index' — refusing to select \
                         inputs against a bond set this wallet cannot read",
                        b.bond_outpoint
                    ),
                )
            })?;
            if bond_is_releasable(&b, nv.virtual_daa) {
                continue;
            }
            out.insert(outpoint);
        }
        match resp.next_cursor {
            Some(next) if !next.is_empty() => cursor = Some(next),
            _ => break,
        }
    }

    // **And the PALW half, which this function did not have** (audit3 H3).
    //
    // `getStakeBonds` reads the DNS overlay store and nothing else, so on a ConsensusV2 network —
    // testnet-11 is one — a producer's bond collateral was structurally invisible here. The
    // consensus `locks` predicate has two branches and this mirrored one of them. That collateral
    // sits at the producer's own pay address by construction (the registration carrier requires
    // output 0 to pay the producer's payout payload), it is usually the LARGEST output there, and
    // the selector below sorts largest-first — so it went in at input 0 of the next `wallet send`,
    // the block carrying it is disqualified, and the operator's send silently never lands.
    //
    // An empty class id asks only this question, which is what a wallet can answer with: it has no
    // class id to offer.
    let palw = nv
        .client
        .get_palw_producer_facts(String::new(), String::new(), 0, false)
        .await
        .map_err(|e| {
            CliError::new(
                exit::GENERIC,
                format!(
                    "getPalwProducerFacts: {e} — refusing to select inputs without knowing which outputs this node has reserved: PALW bond collateral (spending it disqualifies the carrying block) and the panel's own fee outpoint (spending it leaves the node unable to answer a court, which costs the bond)"
                ),
            )
        })?;
    for outpoint in &palw.locked_bond_outpoints {
        // Same fail-closed contract as the DNS half above: a shape this wallet cannot read is an
        // error, never a silent pass.
        let parsed = parse_outpoint_str(outpoint).ok_or_else(|| {
            CliError::new(
                exit::GENERIC,
                format!(
                    "getPalwProducerFacts returned locked bond outpoint '{outpoint}', which is not 'txid_hex:index' — \
                     refusing to select inputs against a bond set this wallet cannot read"
                ),
            )
        })?;
        out.insert(parsed);
    }
    Ok(out)
}

/// `PalwSpendLocks::locks` read back: the collateral is free exactly when the bond is effectively
/// `Unbonding` and the chain has passed `unbond_request_daa_score + unbonding_period_blocks`.
///
/// An `Unbonding` bond with no recorded request height is NOT releasable — the release height is
/// unknown, and "unknown" must read as locked.
fn bond_is_releasable(bond: &kaspa_rpc_core::RpcStakeBondEntry, virtual_daa: u64) -> bool {
    if bond.effective_status != "unbonding" {
        return false;
    }
    bond.unbond_request_daa_score
        .and_then(|requested| requested.checked_add(bond.unbonding_period_blocks))
        .is_some_and(|release| virtual_daa >= release)
}

/// "txid_hex:index", the shape every overlay RPC uses for an outpoint.
fn parse_outpoint_str(s: &str) -> Option<TransactionOutpoint> {
    let (tx, ix) = s.rsplit_once(':')?;
    Some(TransactionOutpoint::new(tx.parse().ok()?, ix.parse().ok()?))
}

pub(crate) async fn page_all(nv: &NodeView, address: &Address) -> Result<Vec<Funding>, CliError> {
    let bonds = locked_bond_outpoints(nv).await?;
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
            // Both gates, as the node applies them. The confirmed anchor is not exposed over RPC,
            // so `None` is passed: that only ever makes this stricter than the node, which is the
            // safe direction for a wallet — it may hold back a spendable output, never offer an
            // unspendable one.
            let mature = is_spendable_settled(
                e.utxo_entry.is_coinbase,
                e.utxo_entry.block_daa_score,
                nv.virtual_daa,
                nv.coinbase_maturity,
                nv.settlement_long_maturity_daa,
                None,
            );
            let outpoint: TransactionOutpoint = e.outpoint.into();
            let bonded = bonds.contains(&outpoint);
            out.push(Funding { outpoint, entry: e.utxo_entry.into(), mature, amount, bonded });
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
pub(crate) fn estimate_fee(key: &ValidatorKey, p: &Params, n_inputs: usize, consolidate: bool) -> u64 {
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
const MAX_TXS_PER_RUN_HARD_CAP: usize = 200;

pub(crate) fn sompi_to_msk(s: u64) -> String {
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
    // **Bonded collateral is reported as bonded, not as spendable** (audit3, the wallet's low).
    //
    // `page_all` computes `bonded` for every entry and this loop was the only place that had it and
    // discarded it, while BOTH spenders drop those outputs (`u.mature && !u.bonded`). So the
    // command operators are told to use for a balance printed 20,000 MSK for an address whose only
    // output is locked collateral, and `wallet send` on the same address answered "have
    // 0.00000000 MSK across 0 UTXO(s)". Two shipped commands, one node, one address, two answers,
    // and no line anywhere saying the gap is a bond.
    let mut bonded_n = 0usize;
    let mut bonded_sum = 0u64;
    let mut imm_cb_daa: Option<(u64, u64)> = None; // (min, max) block daa of immature coinbase
    for u in &utxos {
        if u.bonded {
            bonded_n += 1;
            bonded_sum += u.amount;
        } else if u.mature {
            mature_n += 1;
            mature_sum += u.amount;
        } else {
            imm_n += 1;
            imm_sum += u.amount;
            if u.entry.is_coinbase {
                let d = u.entry.block_daa_score;
                imm_cb_daa = Some(imm_cb_daa.map_or((d, d), |(lo, hi)| (lo.min(d), hi.max(d))));
            }
        }
    }
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "address": addr.to_string(), "total": utxos.len(),
                    "mature": { "count": mature_n, "sompi": mature_sum },
                    "immature": { "count": imm_n, "sompi": imm_sum },
                    "bonded": { "count": bonded_n, "sompi": bonded_sum } })
        ),
        OutputFormat::Human => {
            println!("Address      : {addr}");
            println!("UTXOs total  : {}", utxos.len());
            println!("  mature     : {mature_n}  ({} MSK)", sompi_to_msk(mature_sum));
            println!(
                "  immature   : {imm_n}  ({} MSK)  [coinbase younger than {} DAA: maturity {} + settlement {}]",
                sompi_to_msk(imm_sum),
                nv.coinbase_maturity.max(nv.settlement_long_maturity_daa),
                nv.coinbase_maturity,
                nv.settlement_long_maturity_daa
            );
            if let Some((lo, hi)) = imm_cb_daa {
                let bound = nv.coinbase_maturity.max(nv.settlement_long_maturity_daa);
                println!(
                    "               earliest coinbase daa {lo}, latest {hi}, virtual {} — first matures at daa {}",
                    nv.virtual_daa,
                    lo + bound
                );
            }
            if bonded_n > 0 {
                println!(
                    "  bonded     : {bonded_n}  ({} MSK)  [locked bond collateral — NOT spendable; `wallet send` will not select it]",
                    sompi_to_msk(bonded_sum)
                );
            }
            if utxos.len() > MAX_INPUTS_PER_TX {
                println!();
                println!(
                    "note: {} UTXOs > {MAX_INPUTS_PER_TX}/tx — `misaka wallet utxo consolidate` merges them in chunks.",
                    utxos.len()
                );
            }
        }
    }
    let _ = nv.client.disconnect().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// wallet utxo consolidate — self-spend, chunked
// ---------------------------------------------------------------------------

pub async fn consolidate(
    ctx: &Ctx,
    ks: &KeySource,
    max_inputs: usize,
    dry_run: bool,
    yes: bool,
    max_txs_per_run: usize,
    sleep_ms: u64,
) -> CliResult {
    if max_txs_per_run == 0 {
        return Err(CliError::new(exit::GENERIC, "--max-txs-per-run must be > 0".to_string()));
    }
    if max_txs_per_run > MAX_TXS_PER_RUN_HARD_CAP {
        return Err(CliError::new(exit::GENERIC, format!("--max-txs-per-run must be <= {MAX_TXS_PER_RUN_HARD_CAP}")));
    }
    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());
    let max_inputs = max_inputs.clamp(2, MAX_INPUTS_PER_TX);

    // `!bonded`: never consolidate a validator's locked collateral into a change output (M1-3).
    let mut mature: Vec<Funding> = page_all(&nv, &addr).await?.into_iter().filter(|u| u.mature && !u.bonded).collect();
    if mature.len() < 2 {
        return Err(CliError::new(exit::GENERIC, format!("nothing to consolidate: {} mature UTXO(s) at {addr}", mature.len())));
    }
    // Largest-first is irrelevant for consolidate; keep input order. Chunk it.
    let submit = yes && !dry_run;
    let mut planned: Vec<(usize, u64, u64, u64, Option<String>)> = Vec::new();
    let mut submit_error = None;
    let mut failed_chunk_len = 0usize;
    while mature.len() >= 2 && planned.len() < max_txs_per_run {
        let i = planned.len();
        let take = mature.len().min(max_inputs);
        let chunk: Vec<Funding> = mature.drain(..take).collect();
        let n = chunk.len();
        if n < 2 {
            break; // a 1-UTXO tail is already consolidated
        }
        let fee = estimate_fee(&key, &nv.params, n, true);
        let fundings: Vec<(TransactionOutpoint, UtxoEntry)> = chunk.iter().map(|u| (u.outpoint, u.entry.clone())).collect();
        let sum: u64 = chunk.iter().map(|u| u.amount).sum();
        let tx = key
            .build_funded_consolidate_tx(&fundings, fee, nv.params.storage_mass_parameter)
            .map_err(|e| CliError::new(exit::GENERIC, format!("build consolidate #{i}: {e}")))?;
        let txid = if submit {
            match nv.client.submit_transaction(RpcTransaction::from(&tx), false).await {
                Ok(txid) => Some(txid.to_string()),
                Err(e) => {
                    failed_chunk_len = n;
                    let submitted: Vec<_> = planned.iter().filter_map(|(_, _, _, _, txid)| txid.as_deref()).collect();
                    let mut msg = format!("submit consolidate #{i}: {e}");
                    if !submitted.is_empty() {
                        msg.push_str("; successfully submitted txids before failure: ");
                        msg.push_str(&submitted.join(", "));
                    }
                    submit_error = Some(msg);
                    break;
                }
            }
        } else {
            None
        };
        planned.push((n, sum, fee, sum - fee, txid));
        if submit && sleep_ms > 0 && mature.len() >= 2 && planned.len() < max_txs_per_run {
            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
        }
    }
    let remaining = mature.len().saturating_add(failed_chunk_len);
    let remaining_txs = if remaining >= 2 { remaining.div_ceil(max_inputs) } else { 0 };
    let ok = submit_error.is_none();

    match ctx.output {
        OutputFormat::Json => {
            let arr: Vec<_> = planned
                .iter()
                .map(|(n, sum, fee, out, txid)| json!({ "inputs": n, "inSompi": sum, "feeSompi": fee, "outSompi": out, "txid": txid }))
                .collect();
            println!(
                "{}",
                json!({
                    "ok": ok,
                    "dryRun": !submit,
                    "address": addr.to_string(),
                    "maxTxsPerRun": max_txs_per_run,
                    "sleepMs": sleep_ms,
                    "remainingUtxos": remaining,
                    "remainingTxs": remaining_txs,
                    "error": submit_error.as_deref(),
                    "txs": arr,
                })
            );
        }
        OutputFormat::Human => {
            println!("Address      : {addr}");
            println!("Mode         : {}", if submit { "SUBMIT" } else { "dry-run (no submit; pass --yes to broadcast)" });
            println!("Run limit    : {max_txs_per_run} tx(s), sleep {sleep_ms}ms");
            for (i, (n, sum, fee, out, txid)) in planned.iter().enumerate() {
                println!(
                    "  tx#{i}: {n} inputs, {} MSK in -> {} MSK out (fee {} sompi){}",
                    sompi_to_msk(*sum),
                    sompi_to_msk(*out),
                    fee,
                    txid.as_ref().map(|t| format!("  txid {t}")).unwrap_or_default()
                );
            }
            println!(
                "Result       : {} tx(s){}",
                planned.len(),
                if remaining > 0 { format!(", {remaining} UTXO(s) left ({remaining_txs} more run tx(s))",) } else { String::new() }
            );
            if let Some(e) = &submit_error {
                println!("Submit error : {e}");
            }
        }
    }
    let _ = nv.client.disconnect().await;
    if let Some(e) = submit_error {
        return Err(CliError::new(exit::TX_REJECTED, e));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// wallet send — to an arbitrary recipient
// ---------------------------------------------------------------------------

pub async fn send(ctx: &Ctx, ks: &KeySource, to: &str, amount_sompi: u64, dry_run: bool, yes: bool, coinbase_only: bool) -> CliResult {
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
    // `!bonded`: the bond is usually the LARGEST output at a validator's address, and selection
    // below is largest-first, so without this the default `wallet send` reaches for it first (M1-3).
    let mut mature: Vec<Funding> =
        page_all(&nv, &from_addr).await?.into_iter().filter(|u| u.mature && !u.bonded && (!coinbase_only || u.entry.is_coinbase)).collect();
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
            format!(
                "insufficient mature funds at {from_addr}: have {} MSK across {} UTXO(s) (cap {MAX_INPUTS_PER_TX}), need {} MSK (amount {} + fee {fee}). Consolidate or lower --amount.",
                sompi_to_msk(sum),
                selected.len(),
                sompi_to_msk(needed),
                sompi_to_msk(amount_sompi)
            ),
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
                return Err(CliError::new(
                    exit::GENERIC,
                    "pass --address <addr> or a key source (--key-file/--key-stdin)".to_string(),
                ));
            }
            let _ = ctx;
            Ok(ks.load_key()?.funding_address(nv.params.prefix()))
        }
    }
}

#[cfg(test)]
mod bond_lock_tests {
    //! The wallet's exclusion must mirror consensus's LOCK, not the existence of a bond
    //! (re-audit R-4). Getting this backwards in either direction is expensive: too weak and
    //! `send` spends a validator's collateral out from under an Active bond (audit M1-3); too
    //! strong and an honest validator that has served its unbonding period cannot reclaim 20M KAS
    //! with any shipped command.
    use super::bond_is_releasable;
    use kaspa_rpc_core::RpcStakeBondEntry;

    fn bond(effective_status: &str, requested: Option<u64>, period: u64) -> RpcStakeBondEntry {
        RpcStakeBondEntry {
            bond_outpoint: "00".repeat(64) + ":0",
            owner_pubkey_hash: "00".repeat(64),
            validator_id: "00".repeat(64),
            amount: 20_000_000,
            activation_daa_score: 0,
            unbonding_period_blocks: period,
            unbond_request_daa_score: requested,
            stored_status: effective_status.to_string(),
            effective_status: effective_status.to_string(),
        }
    }

    #[test]
    fn an_active_bond_is_never_releasable() {
        assert!(!bond_is_releasable(&bond("active", None, 100), u64::MAX));
        assert!(!bond_is_releasable(&bond("pending", None, 100), u64::MAX));
        // A slashed bond's output-0 is removed by the slashing side-effect; it is not the wallet's
        // to offer either.
        assert!(!bond_is_releasable(&bond("slashed", Some(0), 100), u64::MAX));
    }

    #[test]
    fn unbonding_is_releasable_only_past_the_release_height() {
        let b = bond("unbonding", Some(1_000), 100);
        assert!(!bond_is_releasable(&b, 1_099), "one block short of release is still locked");
        assert!(bond_is_releasable(&b, 1_100), "at the release height the collateral is spendable");
        assert!(bond_is_releasable(&b, 5_000));
    }

    #[test]
    fn an_unbonding_bond_with_no_request_height_reads_as_locked() {
        // The release height is unknown, and unknown must fail closed.
        assert!(!bond_is_releasable(&bond("unbonding", None, 100), u64::MAX));
        // And an overflowing period cannot wrap into "releasable".
        assert!(!bond_is_releasable(&bond("unbonding", Some(u64::MAX), 1), u64::MAX));
    }
}
