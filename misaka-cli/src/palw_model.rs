//! **`misaka palw model-… ` — ADR-0087's two moves and two reads, keyed by LINE (ADR-0088
//! Decision 9: a class id names the class's founding line).**
//!
//! `show` and `positions` are reads of the tip (`getPalwModelMarket`, `getPalwModelPositions`);
//! `buy` files a `ModelBuy` in a carrier whose output 1 pays the class's sink; `sell` files a
//! `ModelSell` signed by the key whose payout payload is the holder. A quote is printed before
//! anything is sent and nothing is sent without `--yes`. The arithmetic is the chain's own
//! (`kaspa_consensus_core::palw_model_market_v1`), so the quote is what the fold will compute
//! against the market as this node holds it — a move that lands after another move fills at the
//! curve's price then, which is what `--min-units` and `--min-msk` are for.

use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_model_market_v1::{
    PALW_MODEL_POSITION_SUPPLY_V1, PALW_MODEL_POSITION_UNITS_V1, PALW_MODEL_SELL_MLDSA87_CONTEXT, PalwModelFeesV1, PalwModelMarketV1,
    palw_model_buy_quote_with, palw_model_holder_of_pubkey_v1, palw_model_sell_message_v1, palw_model_sell_quote_with,
    palw_model_sink_spk_v1,
};

/// ADR-0114: the schedule the node says a move is settled under at its tip (a node from before
/// ADR-0114 serves 50/10, which is what its fold does). Quotes are made with it, never with a
/// schedule this build assumes, so a preview agrees with the fold on either side of the fence.
pub(crate) fn served_schedule(r: &kaspa_rpc_core::GetPalwModelMarketResponse) -> PalwModelFeesV1 {
    PalwModelFeesV1 { burn_permille: r.burn_permille, leg_permille: r.leg_permille }
}

/// "5 %", "1 %": a permille as the percentage the CLI prints.
pub(crate) fn pct(permille: u64) -> String {
    if permille.is_multiple_of(10) { format!("{} %", permille / 10) } else { format!("{}.{} %", permille / 10, permille % 10) }
}
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::tx::{TransactionOutput, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;

const SOMPI_PER_MSK: u64 = 100_000_000;

/// **How long a sell this tool signs stays good** (mainnet audit 2026-09-06, M-11): 600 DAA, which
/// at the frozen 120 s cadence is about twenty hours — long enough for a carrier to be mined
/// through a quiet spell, short enough that a copied payload is not a standing authority. Well
/// under the chain's own ceiling (`PALW_MODEL_SELL_MAX_WINDOW_DAA_V1`), which acceptance enforces.
const PALW_MODEL_SELL_WINDOW_DAA: u64 = 600;

/// A line id — a class id names the class's founding line (ADR-0088 Decision 9).
fn parse_line(line_id: &str) -> Result<kaspa_consensus_core::Hash64, CliError> {
    line_id
        .parse::<kaspa_consensus_core::Hash64>()
        .map_err(|_| CliError::new(exit::GENERIC, format!("line id '{line_id}' is not a 128-hex Hash64")))
}

/// MSK with an optional fraction ("12.5"), or sompi with a `sompi` suffix ("1250000000sompi").
pub(crate) fn parse_msk_amount(text: &str) -> Result<u64, CliError> {
    let t = text.trim();
    if let Some(sompi) = t.strip_suffix("sompi") {
        return sompi.trim().parse::<u64>().map_err(|_| CliError::new(exit::GENERIC, format!("'{text}' is not a sompi amount")));
    }
    let (whole, frac) = match t.split_once('.') {
        Some((w, f)) => (w, f),
        None => (t, ""),
    };
    if frac.len() > 8 || frac.chars().any(|c| !c.is_ascii_digit()) || whole.chars().any(|c| !c.is_ascii_digit()) || whole.is_empty() {
        return Err(CliError::new(exit::GENERIC, format!("'{text}' is not an MSK amount (up to 8 decimals, or a `sompi` suffix)")));
    }
    let whole: u64 = whole.parse().map_err(|_| CliError::new(exit::GENERIC, format!("'{text}' is out of range")))?;
    let mut frac_sompi = 0u64;
    for (i, c) in frac.chars().enumerate() {
        frac_sompi += (c as u64 - '0' as u64) * 10u64.pow(7 - i as u32);
    }
    whole
        .checked_mul(SOMPI_PER_MSK)
        .and_then(|w| w.checked_add(frac_sompi))
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("'{text}' is out of range")))
}

pub(crate) fn msk(sompi: u64) -> String {
    format!("{}.{:08} MSK", sompi / SOMPI_PER_MSK, sompi % SOMPI_PER_MSK)
}

pub(crate) fn market_from_response(r: &kaspa_rpc_core::GetPalwModelMarketResponse) -> PalwModelMarketV1 {
    PalwModelMarketV1 {
        opened_daa: r.opened_daa,
        msk_reserve: r.msk_reserve,
        position_units: r.position_units,
        sold_units: r.sold_units,
        burned_sompi: r.burned_sompi,
        registrant_paid_sompi: r.registrant_paid_sompi,
        closed_to_buys: r.closed_to_buys,
        contributor_paid_sompi: r.contributor_paid_sompi,
        seed_sompi: r.seed_sompi,
        seeded_by: r.seeded_by.parse().unwrap_or_default(),
        seed_pledged_sompi: r.seed_pledged_sompi,
        buyback_sompi: r.buyback_sompi,
        retired_units: r.retired_units,
    }
}

fn market_json(r: &kaspa_rpc_core::GetPalwModelMarketResponse) -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.palw.model-market.v1",
        "found": r.found,
        "line_id": r.line_id,
        "opened": r.opened,
        "opened_daa": r.opened_daa,
        "msk_reserve_sompi": r.msk_reserve,
        "position_units": r.position_units,
        "positions_in_curve": r.position_units / PALW_MODEL_POSITION_UNITS_V1,
        "sold_units": r.sold_units,
        "burned_sompi": r.burned_sompi,
        "registrant_paid_sompi": r.registrant_paid_sompi,
        "contributor_paid_sompi": r.contributor_paid_sompi,
        "seed_sompi": r.seed_sompi,
        "seed_pledged_sompi": r.seed_pledged_sompi,
        "seeded_by": r.seeded_by,
        "seed_min_sompi": r.seed_min_sompi,
        "buyback_sompi": r.buyback_sompi,
        "retired_units": r.retired_units,
        "retired_positions": r.retired_units / PALW_MODEL_POSITION_UNITS_V1,
        "closed_to_buys": r.closed_to_buys,
        "price_sompi_per_position": r.price_sompi_per_position,
        "supply_units": r.supply_units,
        "virtual_sompi": r.virtual_sompi,
        "class_status": r.class_status,
        "burn_permille": r.burn_permille,
        "owner_leg_permille": r.leg_permille,
        "owner_leg_v2_activation_daa": r.leg_v2_activation_daa,
    })
}

/// `misaka palw model-show <line>`: the market as the tip holds it, and a quote for `--quote-msk`.
pub async fn show(ctx: &Ctx, line_id: &str, quote_msk: Option<String>, json: bool) -> CliResult {
    let line = parse_line(line_id)?;
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_market(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    let _ = nv.client.disconnect().await;
    let as_json = json || ctx.output == OutputFormat::Json;
    let quote = match quote_msk.as_deref().map(parse_msk_amount).transpose()? {
        Some(msk_in) if r.found => {
            palw_model_buy_quote_with(&market_from_response(&r), msk_in, served_schedule(&r)).map(|q| (msk_in, q))
        }
        _ => None,
    };
    if as_json {
        let mut v = market_json(&r);
        if let Some((msk_in, q)) = &quote {
            v["quote"] = serde_json::json!({
                "msk_in_sompi": msk_in, "burn_sompi": q.fees.burn, "registrant_sompi": q.fees.registrant,
                "net_sompi": q.fees.net, "units_out": q.units_out, "positions_out": q.units_out / PALW_MODEL_POSITION_UNITS_V1,
                "price_after_sompi_per_position": q.after.price_sompi_per_position_v1(),
            });
        }
        println!("{}", serde_json::to_string_pretty(&v).expect("serializable"));
    } else if !r.found {
        println!("this chain holds no line {line}");
    } else {
        println!("line {}", r.line_id);
        println!("  class status   {}{}", r.class_status, if r.closed_to_buys { " (closed to buys)" } else { "" });
        println!(
            "  market         {}",
            if r.opened { format!("opened at DAA {}", r.opened_daa) } else { "not yet opened (the first buy opens it)".to_string() }
        );
        println!("  reserve        {}", msk(r.msk_reserve));
        if r.opened {
            println!("  seed (locked)  {} by {}", msk(r.seed_sompi), r.seeded_by);
        } else if r.seed_pledged_sompi > 0 {
            // ADR-0094: paid into, not yet a market. What is locked and what is still owed.
            println!(
                "  seed           {} of {} collected, {} to go — locked in the sink already",
                msk(r.seed_pledged_sompi),
                msk(r.seed_min_sompi),
                msk(r.seed_min_sompi.saturating_sub(r.seed_pledged_sompi))
            );
            println!("  market         opens on the instalment that reaches the floor (ADR-0094)");
        } else {
            println!("  seed           none yet — the market opens with `model-seed` (at least {})", msk(r.seed_min_sompi));
        }
        println!("  in the curve   {} positions ({} units)", r.position_units / PALW_MODEL_POSITION_UNITS_V1, r.position_units);
        println!("  price          {} per position", msk(r.price_sompi_per_position));
        println!("  sold (gross)   {} positions", r.sold_units / PALW_MODEL_POSITION_UNITS_V1);
        println!("  burned         {}", msk(r.burned_sompi));
        println!(
            "  owner          {} paid (the owner's leg, {} of every move now)",
            msk(r.registrant_paid_sompi),
            pct(r.leg_permille)
        );
        if r.leg_v2_activation_daa > 0 {
            println!(
                "  fees           burn {} + owner {} (ADR-0114: the owner's leg is 5 % from DAA {})",
                pct(r.burn_permille),
                pct(r.leg_permille),
                r.leg_v2_activation_daa
            );
        }
        println!("  contributor    {} paid", msk(r.contributor_paid_sompi));
        println!(
            "  mining bought  {} (5 % of every block's reward on this line; {} positions retired)",
            msk(r.buyback_sompi),
            r.retired_units / PALW_MODEL_POSITION_UNITS_V1
        );
        if let Some((msk_in, q)) = &quote {
            println!("quote: a buy of {} now", msk(*msk_in));
            println!("  burn {:<9} {}", pct(r.burn_permille), msk(q.fees.burn));
            println!("  owner {:<8} {}", pct(r.leg_permille), msk(q.fees.registrant));
            println!("  into the curve {}", msk(q.fees.net));
            println!("  positions out  {} ({} units)", q.units_out / PALW_MODEL_POSITION_UNITS_V1, q.units_out);
            println!("  price after    {} per position", msk(q.after.price_sompi_per_position_v1()));
        }
    }
    if !r.found {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line}")));
    }
    Ok(())
}

/// `misaka palw model-positions [--holder <hex> | --key …]`.
pub async fn positions(ctx: &Ctx, holder: Option<String>, ks: Option<&crate::keys::KeySource>, json: bool) -> CliResult {
    let holder = match (holder, ks) {
        (Some(h), _) => h
            .parse::<kaspa_consensus_core::Hash64>()
            .map_err(|_| CliError::new(exit::GENERIC, format!("holder '{h}' is not a 128-hex Hash64")))?,
        (None, Some(ks)) => palw_model_holder_of_pubkey_v1(ks.load_key()?.public_key()),
        (None, None) => return Err(CliError::new(exit::GENERIC, "name a holder (--holder <128-hex>) or a key".to_string())),
    };
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_positions(holder.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelPositions: {e}")))?;
    let _ = nv.client.disconnect().await;
    if json || ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "misaka.palw.model-positions.v1",
                "holder": r.holder,
                "positions": r.positions.iter().map(|p| serde_json::json!({
                    "line_id": p.line_id, "units": p.units, "positions": p.units / PALW_MODEL_POSITION_UNITS_V1,
                })).collect::<Vec<_>>(),
            }))
            .expect("serializable")
        );
    } else if r.positions.is_empty() {
        println!("holder {} holds no position", r.holder);
    } else {
        println!("holder {}", r.holder);
        for p in &r.positions {
            println!("  line {}  {} positions ({} units)", p.line_id, p.units / PALW_MODEL_POSITION_UNITS_V1, p.units);
        }
    }
    Ok(())
}

/// One carrier, priced like every other lifecycle carrier, with `extra` value outputs after the
/// change (a buy's sink at index 1).
/// **ADR-0094 Decision 5: how many post-quantum inputs one carrier fits.**
///
/// Measured on testnet-11, not reasoned about: a 20-input consolidation was refused at a transient
/// storage mass of 585,524 against the 480,000 cap; 15 was accepted. An ML-DSA-87 signature is
/// large, so this is a property of the signature scheme rather than of any one transaction.
pub(crate) const PALW_CARRIER_MAX_INPUTS: usize = 15;

/// The multi-input twin of [`build_move_carrier`], priced the same way: build once to measure the
/// compute mass, then rebuild at the fee that mass earns.
fn build_move_carrier_multi(
    key: &kaspa_pq_validator_core::ValidatorKey,
    nv: &crate::wallet::NodeView,
    object: &PalwConsensusObjectV2,
    funding: &[(kaspa_consensus_core::tx::TransactionOutpoint, UtxoEntry)],
    extra: Vec<TransactionOutput>,
) -> Result<(kaspa_consensus_core::tx::Transaction, u64), CliError> {
    use kaspa_consensus_core::mass::MassCalculator;
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let calc = MassCalculator::new(
        nv.params.mass_per_tx_byte,
        nv.params.mass_per_script_pub_key_byte,
        nv.params.mass_per_sig_op,
        nv.params.storage_mass_parameter,
    );
    let probe = key
        .build_palw_lifecycle_tx_multi(object, funding, floor, extra.clone())
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the carrier: {e}")))?;
    let compute_mass = calc.calc_non_contextual_masses(&probe).compute_mass;
    let fee = kaspa_pq_validator_core::relay_fee_for_compute_mass(compute_mass).max(floor);
    let tx = key
        .build_palw_lifecycle_tx_multi(object, funding, fee, extra)
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the carrier: {e}")))?;
    Ok((tx, fee))
}

fn build_move_carrier(
    key: &kaspa_pq_validator_core::ValidatorKey,
    nv: &crate::wallet::NodeView,
    object: &PalwConsensusObjectV2,
    funding_outpoint: kaspa_consensus_core::tx::TransactionOutpoint,
    funding_entry: &UtxoEntry,
    extra: Vec<TransactionOutput>,
) -> Result<(kaspa_consensus_core::tx::Transaction, u64), CliError> {
    use kaspa_consensus_core::mass::MassCalculator;
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let calc = MassCalculator::new(
        nv.params.mass_per_tx_byte,
        nv.params.mass_per_script_pub_key_byte,
        nv.params.mass_per_sig_op,
        nv.params.storage_mass_parameter,
    );
    let probe = key
        .build_palw_lifecycle_tx_with_outputs(object, funding_outpoint, funding_entry, floor, extra.clone())
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the carrier: {e}")))?;
    let compute_mass = calc.calc_non_contextual_masses(&probe).compute_mass;
    let fee = kaspa_pq_validator_core::relay_fee_for_compute_mass(compute_mass).max(floor);
    let tx = key
        .build_palw_lifecycle_tx_with_outputs(object, funding_outpoint, funding_entry, fee, extra)
        .map_err(|e| CliError::new(exit::GENERIC, format!("build the carrier: {e}")))?;
    Ok((tx, fee))
}

pub(crate) async fn submit_move(
    ctx: &Ctx,
    nv: &crate::wallet::NodeView,
    tx: kaspa_consensus_core::tx::Transaction,
    fee: u64,
    what: &str,
    yes: bool,
) -> CliResult {
    if !yes {
        match ctx.output {
            OutputFormat::Json => {
                println!("{}", serde_json::json!({ "dry_run": true, "txid": tx.id().to_string(), "fee_sompi": fee, "move": what }))
            }
            _ => println!("dry run — {what}; carrier {} (fee {} sompi). Re-run with --yes to submit.", tx.id(), fee),
        }
        return Ok(());
    }
    let txid = tx.id();
    nv.client
        .submit_transaction((&tx).into(), false)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("submit the carrier {txid}: {e}")))?;
    match ctx.output {
        OutputFormat::Json => println!(
            "{}",
            serde_json::json!({ "ok": true, "submitted": true, "txid": txid.to_string(), "fee_sompi": fee, "move": what })
        ),
        _ => println!("submitted {txid} — {what} (fee {fee} sompi); the fold applies it in the block that accepts the carrier"),
    }
    Ok(())
}

/// `misaka palw model-seed --line <id> --msk <amount> --key … [--yes]` — ADR-0090: open the
/// line's market by locking the seed (at least the network's least seed) in its sink. The whole
/// seed becomes the reserve; nothing ever pays it back to anyone.
pub async fn seed(ctx: &Ctx, ks: &crate::keys::KeySource, line_id: &str, msk_text: &str, yes: bool) -> CliResult {
    let line = parse_line(line_id)?;
    let msk_seed = parse_msk_amount(msk_text)?;
    let key = ks.load_key()?;
    let seeder = palw_model_holder_of_pubkey_v1(key.public_key());
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_market(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    if !r.found {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line}")));
    }
    if r.opened {
        return Err(CliError::new(
            exit::GENERIC,
            format!("line {line} is already seeded ({} locked by {})", msk(r.seed_sompi), r.seeded_by),
        ));
    }
    // ADR-0094: a payment under the floor is an INSTALMENT, not an error — it lands in the sink
    // and is locked there, and the market opens on the payment that carries the total across.
    // What is still refused is a payment of nothing.
    if msk_seed == 0 {
        return Err(CliError::new(exit::GENERIC, "a seed of zero pays nothing toward the floor".to_string()));
    }
    let object = PalwConsensusObjectV2::ModelSeed { line_id: line, seeder, msk_seed, sink_index: 1 };
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(&nv, &addr).await?;
    // **ADR-0094 Decision 5: spend as many utxos as the mass cap fits, largest first.** A producer
    // is paid one coinbase output a block, so the money is in hundreds of small pieces and no
    // single one holds a seed. Fifteen ML-DSA-87 inputs is the most one transaction fits under the
    // 480,000 storage-mass cap (measured: 15 accepted, 20 refused at 585,524).
    let mut sorted: Vec<_> = candidates.into_iter().collect();
    sorted.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    let want = msk_seed.saturating_add(kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI);
    let mut funding: Vec<_> = Vec::new();
    let mut have: u64 = 0;
    for (o, e) in sorted.into_iter().take(PALW_CARRIER_MAX_INPUTS) {
        have = have.saturating_add(e.amount);
        funding.push((o, e));
        if have > want {
            break;
        }
    }
    if funding.is_empty() || have <= want {
        let reach = have;
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the {} mature utxo(s) this carrier can spend at {addr} hold {}, which does not cover {} plus a fee.\n                   One transaction fits at most {PALW_CARRIER_MAX_INPUTS} post-quantum inputs, so pay the seed in instalments: \n                   `misaka palw model-seed --line {line} --msk {}` now, and again until the line has {} in all.\n                   Every instalment is locked in the line's sink the moment it lands (ADR-0094); the market opens on the one that crosses the floor.",
                funding.len(),
                msk(reach),
                msk(msk_seed),
                (reach.saturating_sub(kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI)) / 100_000_000,
                msk(r.seed_min_sompi)
            ),
        ));
    }
    let sink = TransactionOutput::new(msk_seed, palw_model_sink_spk_v1(&line));
    let (tx, fee) = build_move_carrier_multi(&key, &nv, &object, &funding, vec![sink])?;
    if ctx.output != OutputFormat::Json {
        println!("seed {} into line {line}", msk(msk_seed));
        println!("  seeder         {seeder} (for the record; the seeder holds no position)");
        println!(
            "  first price    {} per position ({} positions in the curve)",
            msk(msk_seed / PALW_MODEL_POSITION_SUPPLY_V1),
            PALW_MODEL_POSITION_SUPPLY_V1
        );
        println!(
            "  LOCKED FOR GOOD: no object pays a seed out; only a holder's sell moves MSK out of the curve, and never below the seed."
        );
    }
    let what = format!("ModelSeed {} into line {}", msk(msk_seed), line);
    let out = submit_move(ctx, &nv, tx, fee, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw model-buy --line <id> --msk <amount> [--min-positions <n>] --key … [--yes]`.
pub async fn buy(ctx: &Ctx, ks: &crate::keys::KeySource, line_id: &str, msk_text: &str, min_positions: u64, yes: bool) -> CliResult {
    let line = parse_line(line_id)?;
    let msk_in = parse_msk_amount(msk_text)?;
    let key = ks.load_key()?;
    let holder = palw_model_holder_of_pubkey_v1(key.public_key());
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_market(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    if !r.found {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line}")));
    }
    let market = market_from_response(&r);
    let Some(quote) = palw_model_buy_quote_with(&market, msk_in, served_schedule(&r)) else {
        return Err(CliError::new(exit::GENERIC, format!("a buy of {} releases nothing (closed to buys, or too small)", msk(msk_in))));
    };
    let min_units_out = min_positions.saturating_mul(PALW_MODEL_POSITION_UNITS_V1);
    if quote.units_out < min_units_out {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the curve releases {} positions now, under your floor of {min_positions}",
                quote.units_out / PALW_MODEL_POSITION_UNITS_V1
            ),
        ));
    }
    let object = PalwConsensusObjectV2::ModelBuy { line_id: line, holder, msk_in, min_units_out, sink_index: 1 };
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(&nv, &addr).await?;
    let (outpoint, entry) = candidates
        .into_iter()
        .find(|(_, e)| e.amount > msk_in.saturating_add(kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI))
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("no mature, unbonded UTXO at {addr} holds {} plus a fee", msk(msk_in))))?;
    let sink = TransactionOutput::new(msk_in, palw_model_sink_spk_v1(&line));
    let (tx, fee) = build_move_carrier(&key, &nv, &object, outpoint, &entry, vec![sink])?;
    if ctx.output != OutputFormat::Json {
        println!("buy {} of line {line}", msk(msk_in));
        println!("  holder         {holder}");
        println!("  burn {:<9} {}", pct(r.burn_permille), msk(quote.fees.burn));
        println!("  owner {:<8} {}", pct(r.leg_permille), msk(quote.fees.registrant));
        println!("  into the curve {}", msk(quote.fees.net));
        println!(
            "  positions out  {} at least {min_positions} ({} units)",
            quote.units_out / PALW_MODEL_POSITION_UNITS_V1,
            quote.units_out
        );
        println!("  price after    {} per position", msk(quote.after.price_sompi_per_position_v1()));
    }
    let what = format!("ModelBuy {} of line {}", msk(msk_in), line);
    let out = submit_move(ctx, &nv, tx, fee, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw model-sell --line <id> --positions <n> [--min-msk <amount>] --key … [--yes]`.
pub async fn sell(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    line_id: &str,
    positions: u64,
    min_msk_text: Option<String>,
    yes: bool,
) -> CliResult {
    let line = parse_line(line_id)?;
    let units_in = positions
        .checked_mul(PALW_MODEL_POSITION_UNITS_V1)
        .ok_or_else(|| CliError::new(exit::GENERIC, "too many positions".to_string()))?;
    // **A floor of zero is a signature authorising a sale at any price** (mainnet audit
    // 2026-09-06, M-11). ADR-0087 M5 makes `min_msk_out` the holder's only protection and the
    // shipped tool defaulted it away. Required now: the holder states a floor or the tool does not
    // sign.
    let min_msk_out = match min_msk_text.as_deref() {
        Some(text) => parse_msk_amount(text)?,
        None => {
            return Err(CliError::new(
                exit::GENERIC,
                "--min-msk is required: it is the floor your signature authorises, and a sell signed with a floor of 0 \
                 authorises a sale at any price the curve happens to be at"
                    .to_string(),
            ));
        }
    };
    let key = ks.load_key()?;
    let holder = palw_model_holder_of_pubkey_v1(key.public_key());
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_market(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    if !r.found || !r.opened {
        return Err(CliError::new(exit::GENERIC, format!("line {line} has no market to sell into")));
    }
    let held = nv
        .client
        .get_palw_model_positions(holder.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelPositions: {e}")))?
        .positions
        .iter()
        .find(|p| p.line_id == line.to_string())
        .map(|p| p.units)
        .unwrap_or(0);
    if units_in == 0 || units_in > held {
        return Err(CliError::new(
            exit::GENERIC,
            format!("you hold {} positions of line {line}, not {positions}", held / PALW_MODEL_POSITION_UNITS_V1),
        ));
    }
    let market = market_from_response(&r);
    let Some(quote) = palw_model_sell_quote_with(&market, units_in, served_schedule(&r)) else {
        return Err(CliError::new(exit::GENERIC, "the curve pays nothing for this sell".to_string()));
    };
    if quote.fees.net < min_msk_out {
        return Err(CliError::new(
            exit::GENERIC,
            format!("the curve pays {} now, under your floor of {}", msk(quote.fees.net), msk(min_msk_out)),
        ));
    }
    // ADR-0087 M8 (audit M-11): the signature is bound to this chain, to the position it is sold
    // out of, and to a window that ends. `held` is the number this command already read from
    // `getPalwModelPositions` above and checked against; the window is measured from the tip the
    // node reported when this command connected, so the holder signs the window they were shown.
    let not_after_daa = nv.virtual_daa.saturating_add(PALW_MODEL_SELL_WINDOW_DAA);
    let message =
        palw_model_sell_message_v1(crate::bond::network_domain(&nv), &line, &holder, units_in, min_msk_out, held, not_after_daa);
    let signature = key.sign_with_context(&message, PALW_MODEL_SELL_MLDSA87_CONTEXT).to_vec();
    let object = PalwConsensusObjectV2::ModelSell {
        line_id: line,
        holder,
        units_in,
        min_msk_out,
        held_units: held,
        not_after_daa,
        pubkey: key.public_key().to_vec(),
        signature,
    };
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(&nv, &addr).await?;
    let (outpoint, entry) = candidates
        .into_iter()
        .next()
        .ok_or_else(|| CliError::new(exit::GENERIC, format!("no mature, unbonded UTXO at {addr} to fund the carrier")))?;
    let (tx, fee) = build_move_carrier(&key, &nv, &object, outpoint, &entry, Vec::new())?;
    if ctx.output != OutputFormat::Json {
        println!("sell {positions} positions of line {line}");
        println!("  holder         {holder}");
        println!("  gross          {}", msk(quote.fees.gross));
        println!("  burn {:<9} {}", pct(r.burn_permille), msk(quote.fees.burn));
        println!("  owner {:<8} {}", pct(r.leg_permille), msk(quote.fees.registrant));
        println!("  paid to you    {} (coinbase payout), at least {}", msk(quote.fees.net), msk(min_msk_out));
        println!("  price after    {} per position", msk(quote.after.price_sompi_per_position_v1()));
    }
    let what = format!("ModelSell {positions} positions of line {line}");
    let out = submit_move(ctx, &nv, tx, fee, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

#[cfg(test)]
mod tests {
    use super::parse_msk_amount;

    #[test]
    fn msk_amounts_parse_to_sompi() {
        assert_eq!(parse_msk_amount("1").unwrap(), 100_000_000);
        assert_eq!(parse_msk_amount("12.5").unwrap(), 1_250_000_000);
        assert_eq!(parse_msk_amount("0.00000001").unwrap(), 1);
        assert_eq!(parse_msk_amount("1250000000sompi").unwrap(), 1_250_000_000);
        assert!(parse_msk_amount("1.123456789").is_err());
        assert!(parse_msk_amount("abc").is_err());
    }
}
