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
use crate::wallet::{NodeView, connect, estimate_fee, page_all, sompi_to_msk};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwConsensusObjectV2, palw_bond_retirement_message_v2,
};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_rpc_core::api::rpc::RpcApi;
use std::str::FromStr;

/// `<txid>:<index>` — the spelling every other PALW flag uses (`--palw-producer-bond`,
/// `--palw-fee-outpoint`), so an operator can paste between them without translating.
pub(crate) fn parse_outpoint(s: &str) -> Result<TransactionOutpoint, CliError> {
    let (txid, index) = s.split_once(':').ok_or_else(|| CliError::new(exit::GENERIC, format!("'{s}' is not <txid>:<index>")))?;
    let transaction_id =
        TransactionId::from_str(txid).map_err(|e| CliError::new(exit::GENERIC, format!("'{txid}' is not a transaction id: {e}")))?;
    let index: u32 = index.parse().map_err(|_| CliError::new(exit::GENERIC, format!("'{index}' is not an output index")))?;
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
pub async fn status(ctx: &Ctx, ks: &KeySource, class_id: Option<&str>) -> CliResult {
    let nv = connect(ctx).await?;
    let key = ks.load_key()?;
    let addr = key.funding_address(nv.params.prefix());

    let facts = nv
        .client
        .get_palw_producer_facts(String::new(), String::new(), 0, false)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwProducerFacts: {e}")))?;

    // **The bond this key OWNS need not sit at this key's address.** A genesis bond's collateral is
    // posted by the main wallet while the bond is registered to the operator's key (ADR-0059), and
    // a sponsored registration does the same — so the address scan below reports "none" for a key
    // that holds a live, working bond. That is precisely the operator D3 exists for, and scanning
    // UTXOs could never find them.
    //
    // Ownership is a property of the REGISTRY, so ask the registry: walk the network's locked set
    // and keep the outpoints whose registered pubkey is this key's. Needs a class id for the same
    // reason `retire` does — the facts RPC will not read a bond without one — so it is offered
    // rather than required, and its absence is stated instead of being silently a "none".
    let mut owned: Vec<(TransactionOutpoint, String)> = Vec::new();
    // **How many outpoints this scan could not ASK about** — kept because an empty `owned` has two
    // very different causes and one line of output was reporting both as the first: "the registry
    // has none registered to this key" is a claim about the world, and a failed lookup is a claim
    // about the question. A mistyped `--class-id` makes every call return an error, so a key
    // holding a live bond was told, positively, that it holds none. This is the same sin the
    // retire path already refuses to commit; status swallowed it because it has something else to
    // print and no reason to stop.
    let mut unanswered = 0usize;
    if let Some(class_id) = class_id {
        let ours = faster_hex::hex_string(key.public_key());
        for spec in &facts.locked_bond_outpoints {
            let Ok(op) = parse_outpoint(spec) else {
                unanswered += 1;
                continue;
            };
            let Ok(f) = nv.client.get_palw_producer_facts(class_id.to_string(), op.transaction_id.to_string(), op.index, true).await
            else {
                unanswered += 1;
                continue;
            };
            if f.bond_known && f.bond_registered_pubkey.eq_ignore_ascii_case(&ours) {
                owned.push((op, f.bond_collateral.to_string()));
            }
        }
    }

    // Every UTXO at this address, with the node's own view of which are consensus-locked. The
    // intersection is this key's bonds; the rest is spendable.
    let all = page_all(&nv, &addr).await?;
    let locked: Vec<&crate::wallet::Funding> = all.iter().filter(|u| u.bonded).collect();
    let spendable: u64 = all.iter().filter(|u| !u.bonded && u.mature).map(|u| u.amount).sum();

    match ctx.output {
        OutputFormat::Human => {
            println!("address: {addr}");
            match class_id {
                None => {
                    println!("bonds:   not checked — pass --class-id to ask the registry which bonds this KEY owns");
                    println!("         (a bond's collateral often sits at another address, so the scan below can say");
                    println!("          \"none\" for a key that holds a live bond)");
                }
                // An absence the scan could not establish is not an absence.
                Some(_) if owned.is_empty() && unanswered > 0 => {
                    println!("bonds:   UNKNOWN — the node could not answer for {unanswered} of the");
                    println!("         {} locked outpoint(s), so this is not a \"none\".", facts.locked_bond_outpoints.len());
                    println!("         The usual cause is a --class-id this chain has not registered:");
                    println!("         the facts RPC refuses the whole lookup and every outpoint fails.");
                }
                Some(_) if owned.is_empty() => {
                    println!("bonds:   the registry has none registered to this key");
                }
                Some(_) => {
                    println!("bonds:   {} registered to THIS key — pass one to --palw-producer-bond", owned.len());
                    for (op, collateral) in &owned {
                        println!("  {}:{}  collateral {} sompi", op.transaction_id, op.index, collateral);
                    }
                    if unanswered > 0 {
                        println!("         ({unanswered} further outpoint(s) could not be checked — the list may be short)");
                    }
                }
            }
            println!();
            if locked.is_empty() {
                println!("locked:  none at this address");
                println!();
                println!("If you registered a bond with a DIFFERENT key, run this with that key.");
                println!("The node reports {} locked outpoint(s) network-wide.", facts.locked_bond_outpoints.len());
            } else {
                // **"Locked" is not the same as "bonded", and saying so cost an hour.** The node's
                // locked set is a UNION (rpc/service/src/service.rs:763): consensus-locked collateral
                // AND this node's own reserved PALW funding outpoints, so a running producer's panel
                // fee outpoint appears here and reads as a bond. It is not one — passing it to
                // `--palw-producer-bond` names a bond the registry has never heard of.
                //
                // The CLI cannot separate them from this call alone (the registry lookup needs a
                // class id), so it does not pretend to: it labels the set honestly and says how to
                // settle it.
                println!("locked:  {} outpoint(s) at this address", locked.len());
                for u in &locked {
                    println!("  {}:{}  {} MSK", u.outpoint.transaction_id, u.outpoint.index, sompi_to_msk(u.amount));
                }
                println!();
                println!("These are consensus-locked collateral AND this node's reserved PALW funding");
                println!("outpoints — the node reports them as one set. To learn which is a registry bond,");
                println!("run `misaka bond retire --bond <outpoint> --class-id <id> --dry-run`: it names the");
                println!("ones the registry does not know, and never submits anything.");
            }
            println!("spendable (mature, unbonded): {} MSK", sompi_to_msk(spendable));
        }
        OutputFormat::Json => {
            // Named `locked`, not `bonds`: see the human branch — this set unions collateral with the
            // node's reserved funding, and a JSON consumer must not be told otherwise.
            let locked_json: Vec<serde_json::Value> = locked
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
                    "bonds_registered_to_this_key": owned
                        .iter()
                        .map(|(op, c)| serde_json::json!({
                            "outpoint": format!("{}:{}", op.transaction_id, op.index),
                            "collateral_sompi": c,
                        }))
                        .collect::<Vec<_>>(),
                    "bonds_checked": class_id.is_some(),
                    // A consumer must be able to tell "none" from "could not ask" — the human
                    // branch says so in words, and JSON is the reading that gets automated.
                    "bonds_unanswered": unanswered,
                    "locked_outpoints": locked_json,
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
pub async fn retire(ctx: &Ctx, ks: &KeySource, bond_arg: Option<&str>, class_id: Option<&str>, dry_run: bool, yes: bool) -> CliResult {
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
    // **This key must be the key the bond registered.** The signature rides the object and is
    // verified against `bond.pubkey`, so a retirement signed by the wrong key is refused by
    // consensus — but the CLI would still build it, charge the operator a carrier fee, and submit a
    // transaction that can never succeed. Caught by running a complete dry-run with a key holding no
    // bond at all: it signed a release for someone else's collateral without a word.
    //
    // Checking here rather than at submission also means `--dry-run` tells the truth: a dry run that
    // "succeeds" for a key that cannot possibly retire this bond is a rehearsal of the wrong play.
    //
    // **And an ABSENT answer is not a pass.** This guard used to skip itself when the node
    // returned an empty `bond_registered_pubkey` — a fail-open on a field this CLI does not
    // compute and cannot vouch for. Consensus makes an empty registered key unreachable on a real
    // chain (the acceptance layer verifies the registration signature against it, and
    // `verify_mldsa87_with_context` refuses any key that is not `MLDSA87_PK_LEN`), so the skip was
    // guarding a condition that cannot arise HERE — which is exactly why it was wrong to write it
    // that way: the value arrives over RPC from a node this CLI does not control, and "the field
    // was blank" is the one answer that must never be read as "the key matches".
    let ours = faster_hex::hex_string(key.public_key());
    if facts.bond_registered_pubkey.is_empty() {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the node reported bond {}:{} as registered but returned no registered public key, so this CLI cannot tell whether \
                 the key you supplied owns it. Refusing rather than signing a release it cannot check — update the node, or query \
                 one that answers `getPalwProducerFacts` in full.",
                bond_outpoint.transaction_id, bond_outpoint.index
            ),
        ));
    }
    if !ours.eq_ignore_ascii_case(&facts.bond_registered_pubkey) {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "bond {}:{} was registered by a different key, so a retirement signed with this one would be refused by consensus.\n\
                 Run `misaka bond status` with the key that registered it — the chain verifies the signature against the bond's own registered public key, not against whoever pays the carrier.",
                bond_outpoint.transaction_id, bond_outpoint.index
            ),
        ));
    }

    let reserved = facts.bond_reserved_exposure.trim();
    // **The same shape as the ownership guard above, and it was left with the same hole.** An empty
    // `bond_reserved_exposure` was read as "this bond owes nothing" — the one answer that must not
    // be read as a zero, for the same reason a blank public key must not be read as a match: the
    // string arrives over RPC from a node this CLI does not control. Note the asymmetry that made
    // it easy to miss: every OTHER malformed value ("00", "0x0", garbage) already fails closed,
    // because the test is `!= "0"`. Only the empty string opened the gate.
    if reserved.is_empty() {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the node returned no reserved-exposure figure for bond {}:{}, so this CLI cannot tell whether a claim against \
                 it can still be disputed. Refusing rather than releasing collateral it cannot account for — retiring under a \
                 live claim takes the stake out from under a court.",
                bond_outpoint.transaction_id, bond_outpoint.index
            ),
        ));
    }
    let still_owing = reserved != "0";
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
    nv.client.submit_transaction(rpc_tx, false).await.map_err(|e| CliError::new(exit::GENERIC, format!("submitTransaction: {e}")))?;
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
