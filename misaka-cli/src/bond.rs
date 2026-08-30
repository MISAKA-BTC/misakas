//! **The PALW operator's exit** — ADR-0063 Decisions 2 and 3.
//!
//! `BondRetireRequested` has been verified by the virtual processor since it was written and built
//! by nothing: its only other reference in the tree was a label in a log formatter. So the sentence
//! the join runbook makes to every operator —
//!
//! > the collateral … is reclaimable at your pay address once the bond is retired
//!
//! was true about the consensus rule and false about the software. PALW collateral went in and did
//! not come out. This module is the caller that rule has been waiting for, plus the one read
//! (`bond status`) an operator needs to name the bond at all: the outpoint is printed once, in a
//! log line at registration, and stored nowhere else.

use crate::keys::KeySource;
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use crate::wallet::{NodeView, connect, estimate_fee, page_all, sompi_to_msk};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwConsensusObjectV2, palw_bond_retirement_message_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::str::FromStr;

/// `<txid>:<index>` — the spelling every other PALW flag uses (`--palw-producer-bond`,
/// `--palw-fee-outpoint`), so an operator can paste between them without translating.
pub(crate) fn parse_outpoint(s: &str) -> Result<TransactionOutpoint, CliError> {
    let (txid, index) = s
        .split_once(':')
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("'{s}' is not <txid>:<index>")))?;
    let transaction_id = TransactionId::from_str(txid)
        .map_err(|e| CliError::new(exit::GENERIC, format!("'{txid}' is not a transaction id: {e}")))?;
    let index: u32 =
        index.parse().map_err(|_| CliError::new(exit::GENERIC, format!("'{index}' is not an output index")))?;
    Ok(TransactionOutpoint { transaction_id, index })
}

/// The network domain a PALW signature is made under — bound to the GENESIS, not just the network
/// name (audit M2-18), so a signature is a statement about one incarnation of a network. The node
/// tells us both, which is what keeps this from being a constant the CLI could get wrong.
fn network_domain(nv: &NodeView) -> kaspa_consensus_core::Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        nv.params.net.to_string().as_bytes(),
        Some(nv.params.genesis.hash),
    )
}

/// **`misaka bond status`** (D3) — what the chain says about this key's collateral.
///
/// The bond outpoint appears exactly once in an operator's life: a log line at registration, which
/// the runbook tells them to keep because the node stores it nowhere else. An operator who lost
/// that line has a funded, working bond they cannot name — and `--palw-producer-bond` takes the
/// outpoint. `getPalwProducerFacts` already returns the locked set (the wallet calls it to avoid
/// spending collateral), so this is a read the node has been able to answer all along.
pub async fn status(ctx: &Ctx, ks: &KeySource) -> CliResult {
    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());

    let facts = nv
        .client
        .get_palw_producer_facts(String::new(), String::new(), 0, false)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwProducerFacts: {e}")))?;

    // Every UTXO at this address, with the node's own view of which are consensus-locked. The
    // intersection is this key's bonds; the rest is spendable.
    let all = page_all(&nv, &addr).await?;
    let locked: Vec<&crate::wallet::Funding> = all.iter().filter(|u| u.bonded).collect();
    let spendable: u64 = all.iter().filter(|u| !u.bonded && u.mature).map(|u| u.amount).sum();

    match ctx.output {
        OutputFormat::Human => {
            println!("address: {addr}");
            if locked.is_empty() {
                println!("bonds:   none at this address");
                println!();
                println!("If you registered a bond with a DIFFERENT key, run this with that key.");
                println!("The node reports {} locked outpoint(s) network-wide.", facts.locked_bond_outpoints.len());
            } else {
                println!("bonds:   {} locked outpoint(s) — pass one to --palw-producer-bond", locked.len());
                for u in &locked {
                    println!("  {}:{}  {} MSK", u.outpoint.transaction_id, u.outpoint.index, sompi_to_msk(u.amount));
                }
            }
            println!("spendable (mature, unbonded): {} MSK", sompi_to_msk(spendable));
        }
        OutputFormat::Json => {
            let bonds: Vec<serde_json::Value> = locked
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "outpoint": format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index),
                        "sompi": u.amount,
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "address": addr.to_string(),
                    "bonds": bonds,
                    "spendable_sompi": spendable,
                    "node_locked_outpoints": facts.locked_bond_outpoints,
                })
            );
        }
    }
    Ok(())
}

/// **`misaka bond retire`** (D2) — sign the release the consensus rule has always accepted.
///
/// The object carries the owner's ML-DSA-87 over `palw_bond_retirement_message_v2`, verified
/// against the bond's own registered `pubkey` — the signature rides the OBJECT rather than the
/// carrier because the carrier is a transaction anyone can build, and what must be proven is that
/// the party asking to release this collateral is the party that posted it.
///
/// This moves the bond to `Retiring`; the collateral is released after the withdrawal delay, which
/// is why the delay must outlast the whole claim lattice (a bond that could leave sooner could
/// commit fraud and withdraw before it was provable).
pub async fn retire(
    ctx: &Ctx,
    ks: &KeySource,
    bond_arg: Option<&str>,
    class_id: Option<&str>,
    dry_run: bool,
    yes: bool,
) -> CliResult {
    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());
    let all = page_all(&nv, &addr).await?;

    // Which bond. Named explicitly, or inferred when this key holds exactly one — never guessed
    // when it holds several, because retiring the wrong one is not an error an operator can undo.
    let bond_outpoint = match bond_arg {
        Some(s) => parse_outpoint(s)?,
        None => {
            let locked: Vec<&crate::wallet::Funding> = all.iter().filter(|u| u.bonded).collect();
            match locked.as_slice() {
                [one] => one.outpoint,
                [] => {
                    return Err(CliError::new(
                        exit::GENERIC,
                        format!(
                            "no locked bond at {addr}. Run `misaka bond status` to see what this key holds, or pass --bond <txid>:<index> if the bond is at another address."
                        ),
                    ));
                }
                many => {
                    let list: Vec<String> =
                        many.iter().map(|u| format!("{}:{}", u.outpoint.transaction_id, u.outpoint.index)).collect();
                    return Err(CliError::new(
                        exit::GENERIC,
                        format!("this key holds {} bonds — name one with --bond: {}", many.len(), list.join(", ")),
                    ));
                }
            }
        }
    };

    // **Refuse while the bond still owes a court** (ADR-0063 D2). A retirement that outran a
    // claim's data obligation would take the collateral out from under a dispute, and the operator
    // deserves to be told WHICH claims rather than a generic refusal.
    // The request takes the bond as THREE fields — a bare 128-hex txid, the index, and `with_bond`
    // — not as the `<txid>:<index>` string every flag spells it with. Passing the joined form (and
    // `with_bond: false`, which tells the node not to read the bond at all) left `bond_known`
    // permanently false, so the refusal below could never fire.
    //
    // **And an EMPTY class id returns before the bond is ever looked at.** The handler answers an
    // empty class id with the locked-outpoint set alone — a deliberate arm, added so a wallet can
    // skip collateral without knowing a class — which means the exposure this guard exists to read
    // is unreachable without naming a class. There is no RPC that lists classes, so the CLI cannot
    // discover one; the operator has to supply it.
    //
    // So when it is not supplied this command **refuses**. Retiring without the check is the exact
    // thing the guard is for, and a guard that steps aside when it cannot see is worth less than no
    // guard at all, because it still reads like one.
    let Some(class_id) = class_id else {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "cannot check whether {}:{} still owes a court without --class-id.\n\
                 The node reports a bond's reserved exposure only alongside a class it knows, and no RPC lists classes.\n\
                 Pass any registered class id (128-hex) — the node logs the one it produces under; the exposure belongs to the bond, not the class.",
                bond_outpoint.transaction_id, bond_outpoint.index
            ),
        ));
    };
    let facts = nv
        .client
        .get_palw_producer_facts(class_id.to_string(), bond_outpoint.transaction_id.to_string(), bond_outpoint.index, true)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwProducerFacts: {e}")))?;
    // **Reserved exposure IS the live-claim count, in the unit that matters.** A claim reserves
    // `pwu x slash_value` against its bond until it resolves, so a non-zero reservation means at
    // least one claim can still be disputed — and retiring under it would pull the collateral out
    // from under a court. Reading the reservation rather than a claim tally also means the CLI and
    // consensus agree by construction: it is the same number admission checks against the ceiling.
    //
    // **A bond the chain does not know is not a bond that owes nothing** — it is a question this
    // command cannot answer, so it refuses rather than signing a release for an outpoint no
    // registry entry backs. Before the request above was fixed this arm was unreachable, which is
    // why it must be an error and not a shrug.
    if !facts.available {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "this node did not answer for class {class_id} — either it is not a ConsensusV2 network or it does not know that class. Pass a class id this chain has registered."
            ),
        ));
    }
    if !facts.bond_known {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the chain has no bond registered at {}:{}. Check the outpoint with `misaka bond status` — retiring is only meaningful for a bond the registry knows.",
                bond_outpoint.transaction_id, bond_outpoint.index
            ),
        ));
    }
    let reserved = facts.bond_reserved_exposure.trim();
    let still_owing = !reserved.is_empty() && reserved != "0";
    if still_owing {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "this bond still reserves {reserved} of exposure, which means at least one claim against it can still be disputed. Retiring now would take the collateral out from under a live court. Stop producing, wait for the reservations to release (they do as claims reach Final or void), and retry — `misaka bond status` shows the bond and the node's log reports the lattice."
            ),
        ));
    }

    // Fund the carrier from a MATURE, UNBONDED UTXO at this address. `page_all` already marks the
    // bonded ones, and spending collateral to pay for its own release is the one input choice that
    // could not possibly be meant.
    let mut spendable: Vec<&crate::wallet::Funding> = all.iter().filter(|u| u.mature && !u.bonded).collect();
    spendable.sort_by(|a, b| b.amount.cmp(&a.amount));
    let fee = estimate_fee(&key, &nv.params, 1, false);
    let funding = spendable.first().ok_or_else(|| {
        CliError::new(
            exit::GENERIC,
            format!("no mature, unbonded UTXO at {addr} to pay the carrier's {fee} sompi fee — fund the address and retry"),
        )
    })?;
    if funding.amount <= fee {
        return Err(CliError::new(
            exit::GENERIC,
            format!("largest spendable UTXO at {addr} holds {} sompi, under the {fee} sompi fee", funding.amount),
        ));
    }

    let bond = PalwBondKeyV2(bond_outpoint);
    let message = palw_bond_retirement_message_v2(network_domain(&nv), &bond);
    // `sign_with_context` is the same signing path the validator and the panel use, so a
    // retirement is signed exactly as a registration was — one helper, one context argument.
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT).to_vec();

    let object = PalwConsensusObjectV2::BondRetireRequested { bond, signature };
    let tx = key
        .build_palw_lifecycle_tx(&object, funding.outpoint, &funding.entry, fee)
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the retirement carrier: {e}")))?;
    let txid = tx.id();

    if dry_run || !yes {
        match ctx.output {
            OutputFormat::Human => {
                println!("bond:    {}:{}", bond_outpoint.transaction_id, bond_outpoint.index);
                println!("carrier: {txid} (fee {fee} sompi from {}:{})", funding.outpoint.transaction_id, funding.outpoint.index);
                println!();
                println!("This asks the chain to move the bond to Retiring. The collateral is released");
                println!("after the withdrawal delay, not immediately — the delay is what makes fraud");
                println!("provable after a producer stops.");
                println!();
                println!("{}", if dry_run { "--dry-run: nothing submitted." } else { "Re-run with --yes to submit." });
            }
            OutputFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "ok": true, "submitted": false, "txid": txid.to_string(),
                    "bond": format!("{}:{}", bond_outpoint.transaction_id, bond_outpoint.index), "fee_sompi": fee,
                })
            ),
        }
        return Ok(());
    }

    let rpc_tx: kaspa_rpc_core::RpcTransaction = (&tx).into();
    nv.client
        .submit_transaction(rpc_tx, false)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("submitTransaction: {e}")))?;
    match ctx.output {
        OutputFormat::Human => {
            println!("submitted {txid}");
            println!("The bond moves to Retiring when this transaction is accepted; the collateral");
            println!("is spendable after the withdrawal delay. `misaka bond status` reflects it.");
        }
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({
                "ok": true, "submitted": true, "txid": txid.to_string(),
                "bond": format!("{}:{}", bond_outpoint.transaction_id, bond_outpoint.index),
            })
        ),
    }
    Ok(())
}
