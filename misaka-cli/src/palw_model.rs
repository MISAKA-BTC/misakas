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
    PALW_MODEL_POSITION_SUPPLY_V1, PALW_MODEL_POSITION_UNITS_V1, PALW_MODEL_SELL_MLDSA87_CONTEXT, PalwModelMarketV1,
    palw_model_buy_quote_v1, palw_model_holder_of_pubkey_v1, palw_model_sell_message_v1, palw_model_sell_quote_v1,
    palw_model_sink_spk_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::tx::{TransactionOutput, UtxoEntry};
use kaspa_rpc_core::api::rpc::RpcApi;

const SOMPI_PER_MSK: u64 = 100_000_000;

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

fn msk(sompi: u64) -> String {
    format!("{}.{:08} MSK", sompi / SOMPI_PER_MSK, sompi % SOMPI_PER_MSK)
}

fn market_from_response(r: &kaspa_rpc_core::GetPalwModelMarketResponse) -> PalwModelMarketV1 {
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
        "seeded_by": r.seeded_by,
        "seed_min_sompi": r.seed_min_sompi,
        "closed_to_buys": r.closed_to_buys,
        "price_sompi_per_position": r.price_sompi_per_position,
        "supply_units": r.supply_units,
        "virtual_sompi": r.virtual_sompi,
        "class_status": r.class_status,
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
        Some(msk_in) if r.found => palw_model_buy_quote_v1(&market_from_response(&r), msk_in).map(|q| (msk_in, q)),
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
        } else {
            println!("  seed           none yet — the market opens with `model-seed` (at least {})", msk(r.seed_min_sompi));
        }
        println!("  in the curve   {} positions ({} units)", r.position_units / PALW_MODEL_POSITION_UNITS_V1, r.position_units);
        println!("  price          {} per position", msk(r.price_sompi_per_position));
        println!("  sold (gross)   {} positions", r.sold_units / PALW_MODEL_POSITION_UNITS_V1);
        println!("  burned         {}", msk(r.burned_sompi));
        println!("  owner          {} paid (the 1 % leg)", msk(r.registrant_paid_sompi));
        println!("  contributor    {} paid", msk(r.contributor_paid_sompi));
        if let Some((msk_in, q)) = &quote {
            println!("quote: a buy of {} now", msk(*msk_in));
            println!("  burn 5 %       {}", msk(q.fees.burn));
            println!("  registrant 1 % {}", msk(q.fees.registrant));
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
    if msk_seed < r.seed_min_sompi {
        return Err(CliError::new(
            exit::GENERIC,
            format!("a seed of {} is under this network's least seed of {}", msk(msk_seed), msk(r.seed_min_sompi)),
        ));
    }
    let object = PalwConsensusObjectV2::ModelSeed { line_id: line, seeder, msk_seed, sink_index: 1 };
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(&nv, &addr).await?;
    let (outpoint, entry) = candidates
        .into_iter()
        .find(|(_, e)| e.amount > msk_seed.saturating_add(kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI))
        .ok_or_else(|| {
            CliError::new(exit::GENERIC, format!("no mature, unbonded UTXO at {addr} holds {} plus a fee", msk(msk_seed)))
        })?;
    let sink = TransactionOutput::new(msk_seed, palw_model_sink_spk_v1(&line));
    let (tx, fee) = build_move_carrier(&key, &nv, &object, outpoint, &entry, vec![sink])?;
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
    let Some(quote) = palw_model_buy_quote_v1(&market, msk_in) else {
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
        println!("  burn 5 %       {}", msk(quote.fees.burn));
        println!("  registrant 1 % {}", msk(quote.fees.registrant));
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
    let min_msk_out = min_msk_text.as_deref().map(parse_msk_amount).transpose()?.unwrap_or(0);
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
    let Some(quote) = palw_model_sell_quote_v1(&market, units_in) else {
        return Err(CliError::new(exit::GENERIC, "the curve pays nothing for this sell".to_string()));
    };
    if quote.fees.net < min_msk_out {
        return Err(CliError::new(
            exit::GENERIC,
            format!("the curve pays {} now, under your floor of {}", msk(quote.fees.net), msk(min_msk_out)),
        ));
    }
    let message = palw_model_sell_message_v1(&line, &holder, units_in, min_msk_out);
    let signature = key.sign_with_context(&message, PALW_MODEL_SELL_MLDSA87_CONTEXT).to_vec();
    let object = PalwConsensusObjectV2::ModelSell {
        line_id: line,
        holder,
        units_in,
        min_msk_out,
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
        println!("  burn 5 %       {}", msk(quote.fees.burn));
        println!("  registrant 1 % {}", msk(quote.fees.registrant));
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
