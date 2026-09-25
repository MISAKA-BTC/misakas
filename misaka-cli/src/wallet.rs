//! PQ wallet commands (Tier B). L1 is PQ-only ML-DSA-87 P2PKH UTXO. These wrap
//! the node wRPC + the consensus-proven tx builders in kaspa-pq-validator-core
//! (the SAME signing path PALW keys use), adding the large-UTXO remedy:
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
    /// bond record survives with no backing (audit M1-1). Every other spender excludes it — the
    /// in-node validator calls it "a validator self-wedge", and the sidecar threads an exclusion
    /// through bond, unbond and equivocate. The wallet, which wraps the SAME signing path a validator
    /// bonded with, did not (audit M1-3): the bond is typically the largest UTXO at that address, and
    /// selection is largest-first. The overlay keeps running and consensus keeps locking its bonds
    /// (ADR-0126, ADR-0128), so the exclusion stays.
    ///
    /// **Only a registered bond outpoint is bonded.** The PALW half of the node's must-not-spend
    /// list is a union (see [`LockedOutpoints`]), and the registry decides which member is a bond;
    /// the rest is [`Funding::reserved`].
    pub(crate) bonded: bool,
    /// **Held back by this node's own PALW panel, and NOT a bond** (audit3 H12): the outpoint the
    /// panel funds its lifecycle carriers from — its `--palw-fee-outpoint` or a rolling successor,
    /// i.e. output 0 of the panel's last carrier. The node lists it beside consensus-locked
    /// collateral, and reading the whole list as `bonded` labelled bond 7's 99.96 MSK fee change
    /// "locked bond collateral — NOT spendable" through the node hosting bond 7's panel, while bonds
    /// 2 and 5's identical change read "mature", and `model add` under bond 7's key found nothing to
    /// fund its carrier (testnet-12, 2026-09-23).
    ///
    /// A generic spender still leaves it alone ([`Funding::selectable`]): `wallet send` moving it
    /// to another address is what left a panel unable to pay for a court move. A carrier filed
    /// through `palw_fp::submit_objects` (`model add`) may take it last
    /// (`palw_fp::lifecycle_candidates_v1`), because its change comes back here.
    pub(crate) reserved: bool,
}

impl Funding {
    /// May a spender that moves value away — `wallet send`, consolidate, a stake, a deposit —
    /// select this output? Mature, not a bond's collateral, and not this node's panel's funding.
    pub(crate) fn selectable(&self) -> bool {
        self.mature && !self.bonded && !self.reserved
    }
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
    // ADR-0152 §8.2: every signature this view feeds is made under the node's GENESIS — take the
    // params of the chain the salt names, and refuse before any signing if the node runs another.
    let (params, salt) = chain_params(ctx, server.network_id)?;
    check_node_genesis(&client, &params, salt.as_ref()).await?;
    Ok(NodeView::from_parts(client, &server, params))
}

/// **The params of the chain this CLI signs for** (ADR-0152 §8.2; P2-12 review finding 1):
/// `Params::from(network)`, or a testnet-12 drill's params when `--palw-drill-genesis-salt` is
/// given — one constructor, shared with kaspad and the rail
/// (`config::drill::palw_chain_params_v1`). Every network domain the CLI signs under is
/// `palw_network_domain_v2_for(params.net, params.genesis.hash)` of what this returns.
pub(crate) fn chain_params(
    ctx: &Ctx,
    network: NetworkId,
) -> Result<(Params, Option<kaspa_consensus_core::config::drill::PalwDrillSaltV1>), CliError> {
    let salt = ctx
        .palw_drill_genesis_salt
        .as_deref()
        .map(kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex)
        .transpose()
        .map_err(|e| CliError::new(exit::CONFIG, format!("--palw-drill-genesis-salt: {e}")))?;
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network, salt.as_ref())
        .map_err(|e| CliError::new(exit::CONFIG, format!("--palw-drill-genesis-salt: {e}")))?;
    Ok((params, salt))
}

/// **Refuse to sign for a node whose genesis is not `params`'** (ADR-0152 §8.2; review finding 1).
/// Asked of testnet-12 nodes — the only network with a drill — and of any node when a salt is
/// given; the node's `getPalwNodeStatus` (version 4) says which genesis it runs and which drill,
/// and `palw_node_genesis_verdict_v1` names the fix. Fail-closed on testnet-12: a node that cannot
/// say which genesis it runs is not signed for, because the one mistake this guards against — a
/// drill object signed under public testnet-12's domain — is valid on the public network.
pub(crate) async fn check_node_genesis(
    client: &KaspaRpcClient,
    params: &Params,
    salt: Option<&kaspa_consensus_core::config::drill::PalwDrillSaltV1>,
) -> Result<(), CliError> {
    if !kaspa_consensus_core::config::drill::palw_node_genesis_check_applies_v1(params.net, salt) {
        return Ok(());
    }
    let status = client.get_palw_node_status().await.map_err(|e| {
        CliError::new(
            exit::NETWORK_MISMATCH,
            format!(
                "getPalwNodeStatus: {e} — cannot confirm which {} genesis the node runs, so nothing is signed for it (a drill and \
                 public {} share the network name)",
                params.net, params.net
            ),
        )
    })?;
    node_genesis_verdict(params, salt, &status)
}

/// [`check_node_genesis`]'s verdict on a status already read (the operator surface reads it once).
pub(crate) fn node_genesis_verdict(
    params: &Params,
    salt: Option<&kaspa_consensus_core::config::drill::PalwDrillSaltV1>,
    status: &kaspa_rpc_core::GetPalwNodeStatusResponse,
) -> Result<(), CliError> {
    kaspa_consensus_core::config::drill::palw_node_genesis_verdict_v1(params, salt, &status.genesis_hash, &status.drill_salt_id)
        .map_err(|why| CliError::new(exit::NETWORK_MISMATCH, why))
}

impl NodeView {
    /// A view over a connection someone else opened and checked — the operator surface
    /// (ADR-0122), which reads a node whether or not it has `--utxoindex` and says which facts that
    /// costs it, rather than refusing the whole screen the way a spender must. `params` is the
    /// chain's ([`chain_params`]); a signer reaches here only through [`connect`], which has
    /// checked them against the node's genesis.
    pub(crate) fn from_parts(client: KaspaRpcClient, server: &kaspa_rpc_core::GetServerInfoResponse, params: Params) -> NodeView {
        let coinbase_maturity = params.coinbase_maturity();
        let settlement_long_maturity_daa = params.dns_params.as_ref().map_or(0, |d| d.coinbase_settlement_long_maturity_daa);
        NodeView { client, params, virtual_daa: server.virtual_daa_score, coinbase_maturity, settlement_long_maturity_daa }
    }

    /// The DAA a coinbase output needs before this node lets it be spent: the maturity floor or
    /// the settlement maturity, whichever is longer (the two gates `page_all` applies).
    pub(crate) fn coinbase_spendable_after(&self) -> u64 {
        self.coinbase_maturity.max(self.settlement_long_maturity_daa)
    }
}

/// **What vests toward an address, and which of its bonded outputs B-3 holds** (ADR-0152, op 199).
/// A vesting row is not a UTXO: nothing here is an output, nothing here is selectable, and no
/// spender reads it — [`Funding::selectable`] is untouched. It exists so the one command operators
/// are told to use for a balance says where the rest of their reward is.
pub(crate) struct UtxoVesting {
    /// `getPalwVesting` for the address's payload.
    pub(crate) by_address: kaspa_rpc_core::GetPalwVestingResponse,
    /// The address's bonded outputs B-3 holds: `(outpoint, the unmatured rows the bond is payee of
    /// — every one the node matched, not a page of them — and the latest DAA clock among them)`.
    pub(crate) held: Vec<(TransactionOutpoint, u64, Option<u64>)>,
}

/// At most this many bonded outputs are asked about (an address holds one bond, rarely two).
const VESTING_BONDS_ASKED: usize = 8;

impl UtxoVesting {
    /// The lines `wallet utxo list` prints (`spend_after`: a minted output's coinbase maturity).
    pub(crate) fn lines(&self, spend_after: u64) -> Vec<String> {
        let v = &self.by_address;
        let sompi = |text: &str| u64::try_from(text.parse::<u128>().unwrap_or(0)).unwrap_or(u64::MAX);
        let mut out = Vec::new();
        let (vesting, latched) = (sompi(&v.maturing_sompi), sompi(&v.query_latched_sompi));
        if v.rows_total > 0 || !v.reporter_rewards.is_empty() {
            let next = v.rows.iter().map(|row| row.eta_daa).min().map(|daa| format!(", next moves ≥ DAA {daa}")).unwrap_or_default();
            out.push(format!(
                "  vesting    : {} row(s)  ({} MSK; {} latched){next}  [PALW rewards not minted yet — rows, not outputs: never \
                 selectable; each is minted when its row matures, then spendable {spend_after} DAA after that coinbase; a \
                 conviction first burns it]",
                v.rows_total,
                sompi_to_msk(vesting.saturating_add(latched)),
                sompi_to_msk(latched)
            ));
            let reporter: u64 = v.reporter_rewards.iter().fold(0u64, |sum, r| sum.saturating_add(r.sompi));
            if reporter > 0 {
                out.push(format!("               reporter rewards to this address: {} MSK", sompi_to_msk(reporter)));
            }
            if v.halted {
                out.push("               the chain is in a licence halt: no row matures until an anchor settles".to_string());
            }
        }
        for (outpoint, rows, expiry) in &self.held {
            out.push(format!(
                "  held (B-3) : {outpoint}  [bond collateral locked while the bond is payee of {rows} unmatured vesting row(s){}]",
                expiry.map(|d| format!("; the last DAA clock runs to {d}")).unwrap_or_default()
            ));
        }
        out
    }

    /// **The mark an output gets in the newest-outputs list when B-3 holds it**: a bonded output
    /// the bond's own collateral, locked for one more reason than its bond status — the bond is
    /// payee of rows the conviction window still holds. `None` for every other output.
    pub(crate) fn held_mark(&self, outpoint: &TransactionOutpoint) -> Option<String> {
        self.held
            .iter()
            .find(|(held, ..)| held == outpoint)
            .map(|(_, rows, _)| format!("held by B-3: payee of {rows} unmatured vesting row(s)"))
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        let v = &self.by_address;
        json!({
            "rows": v.rows_total,
            "vestingSompi": v.maturing_sompi,
            "latchedSompi": v.query_latched_sompi,
            "nextMoveDaa": v.rows.iter().map(|row| row.eta_daa).min(),
            "reporterSompi": v.reporter_rewards.iter().fold(0u64, |sum, r| sum.saturating_add(r.sompi)),
            "halted": v.halted,
            "selectable": false,
            "heldByVesting": self.held.iter().map(|(outpoint, rows, expiry)| json!({
                "outpoint": outpoint.to_string(), "unmaturedRows": rows, "lastExpiryDaa": expiry,
            })).collect::<Vec<_>>(),
        })
    }
}

/// One [`UtxoVesting::held`] entry from the bond's `getPalwVesting` answer: the node's count of
/// the rows whose lock is live (`lockLiveRows`, the per-row term of the B-3 walk that set
/// `payeeHoldsCollateral`) and the latest DAA clock among them — over EVERY row it matched. Never
/// counted off `rows`: that is one page in V-7's order, where the latched rows (which hold nobody)
/// come first and the rows that hold the bond come last, so a busy seat's first page can say
/// "held, by 0 rows" (review of P2-10, finding 1).
fn b3_held_entry(
    outpoint: TransactionOutpoint,
    by_bond: &kaspa_rpc_core::GetPalwVestingResponse,
) -> (TransactionOutpoint, u64, Option<u64>) {
    (outpoint, by_bond.lock_live_rows, by_bond.lock_live_last_expiry_daa)
}

/// [`UtxoVesting`] for `address`, or `None` when the node cannot answer op 199 (a node older than
/// it drops the connection — then the bonds are not asked either).
async fn utxo_vesting(nv: &NodeView, address: &Address, utxos: &[Funding]) -> Option<UtxoVesting> {
    let by_address = nv
        .client
        .get_palw_vesting(kaspa_rpc_core::GetPalwVestingRequest {
            payout_address: address.to_string(),
            limit: 50,
            ..Default::default()
        })
        .await
        .ok()
        .filter(|r| r.available && r.rcore_plus_active)?;
    let mut held = Vec::new();
    for u in utxos.iter().filter(|u| u.bonded).take(VESTING_BONDS_ASKED) {
        // One row: the hold and its count are whole-match totals, so the page is not read.
        let Ok(by_bond) = nv
            .client
            .get_palw_vesting(kaspa_rpc_core::GetPalwVestingRequest {
                bond: format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index),
                limit: 1,
                ..Default::default()
            })
            .await
        else {
            break;
        };
        if by_bond.payee_holds_collateral {
            held.push(b3_held_entry(u.outpoint, &by_bond));
        }
    }
    Some(UtxoVesting { by_address, held })
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
/// height). The sidecar's `unbond` only files the request and refuses to touch output-0, so
/// `wallet send` is the shipped way to reclaim it. Excluding it unconditionally stranded a
/// mainnet validator's 20M KAS behind a hand-built transaction.
///
/// So the predicate below is `locks`, read back: skip a bond that is releasable at the node's
/// current DAA, exclude every other one.
async fn locked_bond_outpoints(nv: &NodeView) -> Result<LockedOutpoints, CliError> {
    let mut out = LockedOutpoints::default();
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
            out.stake_bonds.insert(outpoint);
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
        out.palw.insert(parsed);
    }
    Ok(out)
}

/// **The node's must-not-spend set, kept by where each outpoint came from** — because only one of
/// the sources is a bond.
#[derive(Default)]
pub(crate) struct LockedOutpoints {
    /// DNS StakeBonds whose collateral consensus still locks (`getStakeBonds`): bonds by construction.
    stake_bonds: std::collections::HashSet<TransactionOutpoint>,
    /// `getPalwProducerFacts.locked_bond_outpoints`, which is deliberately ONE list of two things
    /// (`rpc/core` documents it): PALW collateral consensus locks, and the outpoints this node's
    /// panel reserved to fund its carriers. The wire cannot say which is which; the registry can.
    palw: std::collections::HashSet<TransactionOutpoint>,
}

/// **`(bonded, reserved)` for one outpoint: only a registered bond outpoint is bonded.**
///
/// `registered` is the PALW registry's answer for an outpoint in the PALW half — `Some(true)` a
/// bond is registered there, `Some(false)` the registry answered and holds none (so the node listed
/// it as its panel's reservation), `None` it could not be asked. Unanswered fails closed: a member
/// of the must-not-spend list the wallet cannot classify stays bonded.
///
/// What it replaces read every member of the union as bonded, and the union's reserved members are
/// exactly one node's panel's fee change: output 0 of a lifecycle carrier, at the key that panel
/// signs with. So the same output read "[bonded]" or "mature" depending on which node was asked.
pub(crate) fn classify_locked(outpoint: &TransactionOutpoint, locks: &LockedOutpoints, registered: Option<bool>) -> (bool, bool) {
    if locks.stake_bonds.contains(outpoint) {
        return (true, false);
    }
    if !locks.palw.contains(outpoint) {
        return (false, false);
    }
    match registered {
        Some(false) => (false, true),
        Some(true) | None => (true, false),
    }
}

/// Does the PALW registry hold a bond at `outpoint`? `None` when the node could not say — no PALW
/// state, or the call failed.
async fn palw_bond_registered(nv: &NodeView, outpoint: &TransactionOutpoint) -> Option<bool> {
    let bond = format!("{}:{}", outpoint.transaction_id, outpoint.index);
    let read = nv.client.get_palw_claims(bond, "seat".to_string(), false, 1).await.ok()?;
    read.available.then_some(read.bond_known)
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
    let locks = locked_bond_outpoints(nv).await?;
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
            // The PALW half is a union; the registry is asked about the members at THIS address
            // only — usually none, at most a bond and its panel's fee change.
            let registered = if locks.palw.contains(&outpoint) && !locks.stake_bonds.contains(&outpoint) {
                palw_bond_registered(nv, &outpoint).await
            } else {
                None
            };
            let (bonded, reserved) = classify_locked(&outpoint, &locks, registered);
            out.push(Funding { outpoint, entry: e.utxo_entry.into(), mature, amount, bonded, reserved });
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

pub async fn utxo_list(ctx: &Ctx, address: Option<&str>, ks: &KeySource, recent: usize) -> CliResult {
    let nv = connect(ctx).await?;
    let addr = resolve_address(ctx, address, ks, &nv)?;
    let utxos = page_all(&nv, &addr).await?;
    // ADR-0127 Decision 3: the newest outputs and their settlement depth, one read per distinct DAA
    // score — the connection's LAST reads, because a node built before getPalwSettlement closes the
    // WebSocket on it, and then the rows print without the column.
    let newest = newest_outputs(&utxos, recent);
    let depths = if newest.is_empty() {
        None
    } else {
        crate::palw_settlement::settlement_by_daa(&nv.client, newest.iter().map(|u| u.entry.block_daa_score)).await
    };
    // ADR-0152 P2-11: what still vests toward this address, and which bonded outputs B-3 holds —
    // asked only where this CLI's ruleset vests, and after every other read: a node built before
    // op 199 drops the connection on it, and then only these lines are lost.
    let vesting = if nv.params.palw_rcore_plus.is_some() { utxo_vesting(&nv, &addr, &utxos).await } else { None };
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
    // And this node's panel's funding is reported as what it is: an ordinary output the panel will
    // spend, not collateral (testnet-12, 2026-09-23 — see `Funding::reserved`).
    let mut reserved_n = 0usize;
    let mut reserved_sum = 0u64;
    let mut imm_cb_daa: Option<(u64, u64)> = None; // (min, max) block daa of immature coinbase
    for u in &utxos {
        if u.bonded {
            bonded_n += 1;
            bonded_sum += u.amount;
        } else if u.reserved {
            reserved_n += 1;
            reserved_sum += u.amount;
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
    // A newest output's JSON row, marked when B-3 holds it (ADR-0152).
    let recent_json = |u: &Funding| {
        let mut row = recent_output_json(u, depths.as_ref());
        if vesting.as_ref().is_some_and(|v| v.held_mark(&u.outpoint).is_some()) {
            row["heldByVesting"] = json!(true);
        }
        row
    };
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            json!({ "ok": true, "address": addr.to_string(), "total": utxos.len(),
                    "mature": { "count": mature_n, "sompi": mature_sum },
                    "immature": { "count": imm_n, "sompi": imm_sum },
                    "bonded": { "count": bonded_n, "sompi": bonded_sum },
                    "reserved": { "count": reserved_n, "sompi": reserved_sum },
                    "settlementAvailable": depths.is_some(),
                    "vesting": vesting.as_ref().map(UtxoVesting::json),
                    "recent": newest.iter().copied().map(&recent_json).collect::<Vec<_>>() })
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
            if reserved_n > 0 {
                println!(
                    "  reserved   : {reserved_n}  ({} MSK)  [this node's PALW panel funds its carriers from it — not a bond; `wallet send` leaves it to the panel, `model add` may fund a carrier from it last]",
                    sompi_to_msk(reserved_sum)
                );
            }
            if let Some(v) = &vesting {
                for line in v.lines(nv.coinbase_spendable_after()) {
                    println!("{line}");
                }
            }
            if !newest.is_empty() {
                println!();
                println!(
                    "Newest {} of {} outputs{}:",
                    newest.len(),
                    utxos.len(),
                    if depths.is_some() { " (settlement depth in PALW anchors)" } else { "" }
                );
                for u in &newest {
                    // ADR-0152 B-3: a bonded output also held because its bond is a vesting
                    // payee says so on its own line, beside the `bonded` mark.
                    match vesting.as_ref().and_then(|v| v.held_mark(&u.outpoint)) {
                        Some(mark) => println!("  {}  [{mark}]", recent_output_line(u, depths.as_ref())),
                        None => println!("  {}", recent_output_line(u, depths.as_ref())),
                    }
                }
                if depths.is_none() {
                    println!(
                        "  (no settlement depth: this node does not answer getPalwSettlement, or keeps no PALW state it can date)"
                    );
                }
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

/// The `n` newest outputs — highest block DAA score first, ties by outpoint — the ones whose
/// settlement a receiver is still waiting on.
fn newest_outputs(utxos: &[Funding], n: usize) -> Vec<&Funding> {
    let mut newest: Vec<&Funding> = utxos.iter().collect();
    newest.sort_by(|a, b| {
        b.entry
            .block_daa_score
            .cmp(&a.entry.block_daa_score)
            .then_with(|| (a.outpoint.transaction_id, a.outpoint.index).cmp(&(b.outpoint.transaction_id, b.outpoint.index)))
    });
    newest.truncate(n);
    newest
}

/// What an output is besides its amount: coinbase, not yet spendable, locked collateral, or held by
/// this node's panel.
fn output_marks(u: &Funding) -> Vec<&'static str> {
    let mut marks = Vec::new();
    if u.entry.is_coinbase {
        marks.push("coinbase");
    }
    if u.bonded {
        marks.push("bonded");
    } else if u.reserved {
        marks.push("reserved");
    } else if !u.mature {
        marks.push("immature");
    }
    marks
}

/// One row of `utxo list`'s newest outputs; the settlement column only where the node answered.
fn recent_output_line(
    u: &Funding,
    depths: Option<&std::collections::BTreeMap<u64, kaspa_rpc_core::GetPalwSettlementResponse>>,
) -> String {
    let marks = output_marks(u);
    let mut line =
        format!("{}:{}  {} MSK  daa {}", u.outpoint.transaction_id, u.outpoint.index, sompi_to_msk(u.amount), u.entry.block_daa_score);
    if !marks.is_empty() {
        line.push_str(&format!("  [{}]", marks.join(", ")));
    }
    if let Some(answer) = depths.and_then(|d| d.get(&u.entry.block_daa_score)) {
        line.push_str(&format!("  {}", crate::palw_settlement::settlement_cell(answer)));
    }
    line
}

fn recent_output_json(
    u: &Funding,
    depths: Option<&std::collections::BTreeMap<u64, kaspa_rpc_core::GetPalwSettlementResponse>>,
) -> serde_json::Value {
    let mut row = json!({
        "outpoint": format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index),
        "sompi": u.amount,
        "blockDaaScore": u.entry.block_daa_score,
        "coinbase": u.entry.is_coinbase,
        "mature": u.mature,
        "bonded": u.bonded,
        "reserved": u.reserved,
    });
    if let Some(answer) = depths.and_then(|d| d.get(&u.entry.block_daa_score)) {
        row["settlement"] = json!({
            "settled": answer.settled,
            "depth": answer.depth,
            "pendingAnchors": answer.pending_anchors,
            "depthIsLowerBound": answer.depth_is_lower_bound,
        });
    }
    row
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

    // `selectable`: never consolidate a validator's locked collateral into a change output (M1-3),
    // nor the panel's funding out from under it (H12).
    let mut mature: Vec<Funding> = page_all(&nv, &addr).await?.into_iter().filter(|u| u.selectable()).collect();
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
        page_all(&nv, &from_addr).await?.into_iter().filter(|u| u.selectable() && (!coinbase_only || u.entry.is_coinbase)).collect();
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

#[cfg(test)]
mod recent_output_tests {
    use super::*;

    fn funding(txid_word: u64, index: u32, daa: u64, coinbase: bool, mature: bool, bonded: bool) -> Funding {
        use kaspa_consensus_core::tx::ScriptPublicKey;
        Funding {
            outpoint: TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_u64_word(txid_word), index),
            entry: UtxoEntry::new(1_000 + daa, ScriptPublicKey::default(), daa, coinbase),
            mature,
            amount: 1_000 + daa,
            bonded,
            reserved: false,
        }
    }

    fn settled_at(depth: u64) -> kaspa_rpc_core::GetPalwSettlementResponse {
        kaspa_rpc_core::GetPalwSettlementResponse { available: true, settled: true, depth, ..Default::default() }
    }

    /// **ADR-0127: the newest outputs, newest first, each with the depth of the DAA score it was
    /// accepted at** — and without the column where the node did not answer.
    #[test]
    fn utxo_list_shows_the_newest_outputs_with_their_settlement_depth() {
        let utxos = vec![
            funding(1, 0, 100, true, true, false),
            funding(2, 1, 300, false, true, false),
            funding(3, 0, 200, true, false, false),
            funding(4, 0, 300, false, true, true),
        ];
        let newest = newest_outputs(&utxos, 3);
        let order: Vec<(u64, u32)> = newest.iter().map(|u| (u.entry.block_daa_score, u.outpoint.index)).collect();
        assert_eq!(order, vec![(300, 1), (300, 0), (200, 0)], "highest DAA first, ties by outpoint");
        assert!(newest_outputs(&utxos, 0).is_empty(), "--recent 0 lists none");
        assert_eq!(newest_outputs(&utxos, 99).len(), 4);

        let depths: std::collections::BTreeMap<u64, kaspa_rpc_core::GetPalwSettlementResponse> =
            [(300, settled_at(2)), (200, settled_at(9))].into_iter().collect();
        let row = recent_output_line(newest[2], Some(&depths));
        assert!(row.ends_with("daa 200  [coinbase, immature]  depth 9"), "{row}");
        let bonded = recent_output_line(newest[1], Some(&depths));
        assert!(bonded.ends_with("[bonded]  depth 2"), "{bonded}");
        let without = recent_output_line(newest[2], None);
        assert!(without.ends_with("daa 200  [coinbase, immature]"), "an old node: the row without the column: {without}");

        let json = recent_output_json(newest[0], Some(&depths));
        assert_eq!(json["blockDaaScore"], 300);
        assert_eq!(json["settlement"]["depth"], 2);
        assert!(recent_output_json(newest[0], None).get("settlement").is_none());
    }
}

#[cfg(test)]
mod locked_outpoint_tests {
    //! **Only a registered bond outpoint is bonded** (testnet-12, 2026-09-23). The node's PALW
    //! must-not-spend list unions consensus-locked collateral with the outpoints its own panel
    //! reserved; reading the union as bonds marked bond 7's 99.96 MSK panel fee change
    //! (`ec222814…:0`, DAA 11) "[bonded] locked bond collateral" on the node hosting that panel,
    //! while bonds 2 and 5's identical change read "mature", and `model add` under bond 7's key
    //! found nothing to fund its carrier.
    use super::*;

    fn op(word: u64, index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(kaspa_consensus_core::Hash64::from_u64_word(word), index)
    }

    /// The t12 shape: a genesis bond's collateral (premine index 7) and the panel's fee change
    /// (output 0 of its last carrier) are both on the node's list; the registry holds a bond at one.
    #[test]
    fn a_panel_reservation_is_reserved_not_bonded() {
        let collateral = op(0x6d69, 7);
        let fee_change = op(0xec22, 0);
        let stake_bond = op(0x5a, 0);
        let locks =
            LockedOutpoints { stake_bonds: [stake_bond].into_iter().collect(), palw: [collateral, fee_change].into_iter().collect() };
        assert_eq!(classify_locked(&collateral, &locks, Some(true)), (true, false), "the registered bond is bonded");
        assert_eq!(classify_locked(&fee_change, &locks, Some(false)), (false, true), "the panel's fee change is reserved, not a bond");
        assert_eq!(classify_locked(&op(0xec23, 0), &locks, None), (false, false), "an output 0 nothing lists is ordinary");
        assert_eq!(classify_locked(&stake_bond, &locks, None), (true, false), "a DNS StakeBond needs no registry read");
        assert_eq!(classify_locked(&fee_change, &locks, None), (true, false), "a member the registry cannot classify fails closed");
    }

    /// **T52: `wallet utxo list` shows vesting and never selects it** (ADR-0152): the address's
    /// rows are printed as rows — not outputs, never selectable — with B-3's hold named on the bonded
    /// output it locks; the output itself stays bonded and unselectable exactly as before.
    #[test]
    fn t52_vesting_is_printed_and_never_selected() {
        use kaspa_rpc_core::{GetPalwVestingResponse, RpcPalwReporterReward, RpcPalwVestingRow};
        let bond = Funding {
            outpoint: op(0x6d69, 7),
            entry: UtxoEntry::new(20_000_000_000_000, Default::default(), 1, false),
            mature: true,
            amount: 20_000_000_000_000,
            bonded: true,
            reserved: false,
        };
        assert!(!bond.selectable(), "B-3's hold adds nothing to select: the bond was never selectable");
        let v = UtxoVesting {
            by_address: GetPalwVestingResponse {
                available: true,
                rcore_plus_active: true,
                rows_total: 2,
                maturing_sompi: "30000000000".into(),
                query_latched_sompi: "10000000000".into(),
                rows: vec![
                    RpcPalwVestingRow { eta_daa: 9_000, ..Default::default() },
                    RpcPalwVestingRow { eta_daa: 9_500, ..Default::default() },
                ],
                reporter_rewards: vec![RpcPalwReporterReward { sompi: 100_000_000, ..Default::default() }],
                ..Default::default()
            },
            held: vec![(bond.outpoint, 1, Some(12_000))],
        };
        let lines = v.lines(600).join("\n");
        assert!(lines.contains("vesting    : 2 row(s)  (400.00000000 MSK; 100.00000000 latched), next moves ≥ DAA 9000"), "{lines}");
        assert!(lines.contains("never selectable") && lines.contains("spendable 600 DAA after that coinbase"), "{lines}");
        assert!(lines.contains("reporter rewards to this address: 1.00000000 MSK"), "{lines}");
        assert!(
            lines.contains(&format!("held (B-3) : {}", bond.outpoint)) && lines.contains("the last DAA clock runs to 12000"),
            "{lines}"
        );
        let doc = v.json();
        assert_eq!(
            (doc["selectable"].clone(), doc["rows"].clone(), doc["nextMoveDaa"].clone()),
            (json!(false), json!(2), json!(9_000))
        );
        assert_eq!(v.held_mark(&bond.outpoint).as_deref(), Some("held by B-3: payee of 1 unmatured vesting row(s)"));
        assert_eq!(v.held_mark(&op(0x6d69, 8)), None, "only the held bond is marked");
        let none = UtxoVesting { by_address: GetPalwVestingResponse::default(), held: Vec::new() };
        assert!(none.lines(600).is_empty(), "nothing vesting prints nothing");
        // The held entry is the node's whole-match count of the rows whose lock is live (B-3's own
        // row term), never the page's: here the page is the head of V-7's order — latched rows
        // that hold nobody — while 50 rows further on hold the bond (review of P2-10, finding 1).
        let by_bond = GetPalwVestingResponse {
            payee_holds_collateral: true,
            rows_total: 600,
            lock_live_rows: 50,
            lock_live_last_expiry_daa: Some(2_049),
            rows: vec![
                RpcPalwVestingRow { lock_live: false, expiry_daa: 100, matured_at: Some(700), ..Default::default() },
                RpcPalwVestingRow { lock_live: false, expiry_daa: 101, matured_at: Some(700), ..Default::default() },
            ],
            ..Default::default()
        };
        assert_eq!(b3_held_entry(bond.outpoint, &by_bond), (bond.outpoint, 50, Some(2_049)));
    }

    /// A reservation is marked as one and held back from a spender that moves value away.
    #[test]
    fn a_reservation_reads_reserved_and_is_not_selectable() {
        let reserved = Funding {
            outpoint: op(0xec22, 0),
            entry: UtxoEntry::new(9_996_000_000, Default::default(), 11, false),
            mature: true,
            amount: 9_996_000_000,
            bonded: false,
            reserved: true,
        };
        assert!(!reserved.selectable());
        assert_eq!(output_marks(&reserved), vec!["reserved"]);
        let row = recent_output_json(&reserved, None);
        assert_eq!((row["bonded"].clone(), row["reserved"].clone()), (json!(false), json!(true)));
        let ordinary = Funding { reserved: false, ..reserved };
        assert!(ordinary.selectable() && output_marks(&ordinary).is_empty());
    }
}
