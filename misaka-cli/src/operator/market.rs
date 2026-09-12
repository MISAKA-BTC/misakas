//! **`misaka model list` and `misaka position list | quote | buy | sell`** — ADR-0122 Decision 7,
//! the same treatment for models and positions: one screen per question, amounts in MSK, and a
//! move that computes its own protection instead of asking the holder to do the arithmetic.
//!
//! Every number is the chain's: the class table (`getPalwClasses`), the lines
//! (`getPalwModelLines`), the market (`getPalwModelMarket`) and the curve's own quote functions
//! (`palw_model_market_v1`), under the fee schedule the node says the fold settles at its tip. A
//! buy or a sell goes through the existing `palw model-buy` / `model-sell` paths, which print the
//! whole move and submit nothing without `--yes`.
//!
//! `--slippage` replaces the hand-computed floor. A buy's `--min-positions` and a sell's
//! `--min-msk` are derived from the quote less the slippage, because the floor is the holder's only
//! protection against a move that lands after someone else's (ADR-0087 M5; a sell signed with a
//! floor of zero authorises a sale at any price).

use crate::operator::finding::paint;
use crate::operator::profile::Profile;
use crate::palw_model::{market_from_response, msk, pct, served_schedule};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_model_market_v1::{
    PALW_MODEL_POSITION_UNITS_V1, palw_model_buy_quote_with, palw_model_holder_of_pubkey_v1, palw_model_sell_quote_with,
};
use kaspa_rpc_core::api::rpc::RpcApi;
use serde_json::json;

/// A context for `profile`'s network: every market read is a read of one chain, and the CLI's
/// global default (testnet-10) is not the one a miner's profile names.
fn ctx_for(ctx: &crate::node::Ctx, profile: &Profile) -> crate::node::Ctx {
    crate::node::Ctx {
        output: ctx.output,
        network: profile.network.clone(),
        rpc: profile.rpc.clone().or_else(|| ctx.rpc.clone()),
        node_grpc: ctx.node_grpc.clone(),
        evm_rpc: ctx.evm_rpc.clone(),
        timeout_secs: ctx.timeout_secs,
        quiet: ctx.quiet,
    }
}

/// `1 %`, `0.5 %`, `0.5` → permille. At most 50 %: a floor further away than that is not a floor.
pub(crate) fn parse_slippage(text: &str) -> Result<u64, CliError> {
    let t = text.trim().trim_end_matches('%').trim();
    let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
    let bad = || CliError::new(exit::GENERIC, format!("--slippage '{text}' is not a percentage like 1% or 0.5%"));
    if whole.is_empty() && frac.is_empty() || frac.len() > 1 {
        return Err(bad());
    }
    let w: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| bad())? };
    let f: u64 = if frac.is_empty() { 0 } else { frac.parse().map_err(|_| bad())? };
    let permille = w * 10 + f;
    if permille > 500 {
        return Err(CliError::new(exit::GENERIC, format!("--slippage {text} is more than 50 %: that floor protects nothing")));
    }
    Ok(permille)
}

/// The floor a quote leaves after `slippage` permille: what the move insists on or refuses.
pub(crate) fn floor_after(quoted: u64, slippage_permille: u64) -> u64 {
    ((quoted as u128) * (1000 - slippage_permille.min(1000)) as u128 / 1000) as u64
}

/// A line argument: a 128-hex line id, or a class id (which names its class's founding line).
fn parse_line(s: &str) -> Result<kaspa_consensus_core::Hash64, CliError> {
    s.trim().parse().map_err(|_| {
        CliError::new(exit::GENERIC, format!("'{s}' is not a line id — `misaka model list` shows each model's line (128 hex)"))
    })
}

/// `misaka model list`: every class with what an operator or a holder asks of it.
pub(crate) async fn model_list(ctx: &crate::node::Ctx, profile: Profile) -> CliResult {
    let ctx = ctx_for(ctx, &profile);
    let reader = crate::palw_derived::connect(&ctx).await?;
    let classes =
        reader.client.get_palw_classes().await.map_err(|e| {
            CliError::new(exit::CONNECTION, format!("getPalwClasses: {e} (a node older than ADR-0122 does not serve it)"))
        })?;
    let mut rows = Vec::new();
    for c in &classes.classes {
        let lines = reader.client.get_palw_model_lines(c.class_id.clone()).await.ok();
        let founding = lines.as_ref().and_then(|l| l.lines.first());
        let market = reader.client.get_palw_model_market(c.class_id.clone()).await.ok().filter(|m| m.found);
        rows.push((c, founding.map(|l| l.name.clone()).unwrap_or_default(), lines.map(|l| l.lines.len()).unwrap_or(0), market));
    }
    if ctx.output == OutputFormat::Json {
        let doc: Vec<serde_json::Value> = rows
            .iter()
            .map(|(c, name, n_lines, m)| {
                json!({
                    "class_id": c.class_id, "name": name, "lines": n_lines, "base": c.is_base_class, "status": c.status,
                    "share_permille": c.share_permille, "budget_blocks": c.budget_blocks, "fp_certified": c.fp_certified,
                    "held": c.held, "artifact_root": c.artifact_root, "canonical_leaves": c.canonical_leaves,
                    "market_open": m.as_ref().map(|m| m.opened), "price_sompi_per_position": m.as_ref().map(|m| m.price_sompi_per_position),
                    "reserve_sompi": m.as_ref().map(|m| m.msk_reserve),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "schema": "misaka.model.list.v1", "tip_daa": classes.tip_daa, "classes": doc }))
                .expect("serializable")
        );
        return Ok(());
    }
    println!("{}", paint::bold(&format!("MODELS · {} · {} classes at DAA {}", profile.network, rows.len(), classes.tip_daa)));
    println!(
        "{}",
        paint::dim(&format!(
            "  {:<11}{:<22}{:<10}{:<8}{:<10}{:<8}{}",
            "CLASS", "NAME", "STATUS", "SHARE", "BUDGET", "PROMPT", "MARKET"
        ))
    );
    for (c, name, _, m) in &rows {
        let share = c.share_permille.map(|s| format!("{s} ‰")).unwrap_or_else(|| "none".into());
        let budget = if c.is_base_class { "no cap".to_string() } else { c.budget_blocks.to_string() };
        let market = match m {
            Some(m) if m.opened => format!("{} / position", msk(m.price_sompi_per_position)),
            Some(m) if m.seed_pledged_sompi > 0 => format!("seed {} of {}", msk(m.seed_pledged_sompi), msk(m.seed_min_sompi)),
            _ => "not open".to_string(),
        };
        let mut label = if name.is_empty() { "—".to_string() } else { name.clone() };
        if c.is_base_class {
            label.push_str(" (base)");
        }
        if c.held {
            label.push_str(" (held)");
        }
        let status = c.status.split([' ', '{']).next().unwrap_or(&c.status).to_string();
        println!(
            "  {}{:<22}{:<10}{:<8}{:<10}{:<8}{market}",
            paint::cyan(&format!("{:<11}", format!("{}…", &c.class_id[..8.min(c.class_id.len())]))),
            label.chars().take(21).collect::<String>(),
            status,
            share,
            budget,
            if c.fp_certified { "yes" } else { "no" }
        );
    }
    println!("{}", paint::dim("  a class id names its founding line: misaka position quote <class id> --msk 100"));
    Ok(())
}

/// The key the position commands sign or read with: `--key-file`, else the mining profile's.
fn key_source(profile: &Profile) -> Result<crate::keys::KeySource, CliError> {
    let path = profile.key_path.as_ref().ok_or_else(|| {
        CliError::new(exit::CONFIG, "name the key: --key-file <seed file> (or set [mining] key in ~/.misaka/mining.toml)")
    })?;
    Ok(crate::keys::KeySource { key_file: Some(path.display().to_string()), key_stdin: false })
}

/// `misaka position list`: this key's positions, what the curve would pay for them now, and the
/// lines they are in.
pub(crate) async fn position_list(ctx: &crate::node::Ctx, profile: Profile, holder: Option<String>) -> CliResult {
    let ctx = ctx_for(ctx, &profile);
    let holder = match holder {
        Some(h) => h
            .parse::<kaspa_consensus_core::Hash64>()
            .map_err(|_| CliError::new(exit::GENERIC, format!("holder '{h}' is not 128 hex")))?,
        None => palw_model_holder_of_pubkey_v1(key_source(&profile)?.load_key()?.public_key()),
    };
    let reader = crate::palw_derived::connect(&ctx).await?;
    let held = reader
        .client
        .get_palw_model_positions(holder.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelPositions: {e}")))?;
    let mut rows = Vec::new();
    for p in &held.positions {
        let market = reader.client.get_palw_model_market(p.line_id.clone()).await.ok().filter(|m| m.found);
        let line =
            reader.client.get_palw_model_line(p.line_id.clone()).await.ok().and_then(|l| l.line).map(|l| l.name).unwrap_or_default();
        let value = market
            .as_ref()
            .and_then(|m| palw_model_sell_quote_with(&market_from_response(m), p.units, served_schedule(m)))
            .map(|q| q.fees.net);
        rows.push((p, line, value));
    }
    if ctx.output == OutputFormat::Json {
        let doc: Vec<serde_json::Value> = rows
            .iter()
            .map(|(p, name, value)| json!({ "line_id": p.line_id, "name": name, "positions": p.units / PALW_MODEL_POSITION_UNITS_V1, "sell_now_sompi": value }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({ "schema": "misaka.position.list.v1", "holder": held.holder, "held_on": "pq", "positions": doc })
            )
            .expect("serializable")
        );
        return Ok(());
    }
    println!("{}", paint::bold(&format!("POSITIONS · {} · holder {}…", profile.network, &held.holder[..16.min(held.holder.len())])));
    if rows.is_empty() {
        println!(
            "  {}",
            paint::dim(
                "none held by this key (positions bought over the EVM are held by the EVM address: misaka palw model-evm-position)"
            )
        );
        return Ok(());
    }
    println!("{}", paint::dim(&format!("  {:<12}{:<24}{:>12}  {}", "LINE", "NAME", "POSITIONS", "A SELL NOW PAYS")));
    let mut total = 0u64;
    for (p, name, value) in &rows {
        total = total.saturating_add(value.unwrap_or(0));
        println!(
            "  {}{:<24}{:>12}  {}",
            paint::cyan(&format!("{:<12}", format!("{}…", &p.line_id[..8.min(p.line_id.len())]))),
            if name.is_empty() { "—".to_string() } else { name.chars().take(23).collect() },
            crate::operator::status::group(p.units / PALW_MODEL_POSITION_UNITS_V1),
            value.map(msk).unwrap_or_else(|| "no market".into())
        );
    }
    println!("  {:<36}{:>12}  {}", "", "", paint::bold(&msk(total)));
    println!("  {}", paint::dim("PQ-held only: positions bought over the EVM are the EVM address's (misaka palw model-evm-position)"));
    Ok(())
}

/// `misaka position quote <line> (--msk N | --positions N)`.
pub(crate) async fn position_quote(
    ctx: &crate::node::Ctx,
    profile: Profile,
    line: &str,
    msk_in: Option<&str>,
    positions: Option<u64>,
) -> CliResult {
    let ctx = ctx_for(ctx, &profile);
    let line = parse_line(line)?;
    let reader = crate::palw_derived::connect(&ctx).await?;
    let m = reader
        .client
        .get_palw_model_market(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    if !m.found {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line}")));
    }
    let market = market_from_response(&m);
    match (msk_in, positions) {
        (Some(text), None) => {
            let amount = crate::palw_model::parse_msk_amount(text)?;
            let q = palw_model_buy_quote_with(&market, amount, served_schedule(&m)).ok_or_else(|| {
                CliError::new(exit::GENERIC, "the curve releases nothing for that amount (closed to buys, not open, or too small)")
            })?;
            println!("buy {} of line {}…", msk(amount), &m.line_id[..16]);
            println!(
                "  fees        burn {} {} · owner {} {}",
                pct(m.burn_permille),
                msk(q.fees.burn),
                pct(m.leg_permille),
                msk(q.fees.registrant)
            );
            println!("  you get     {} positions", crate::operator::status::group(q.units_out / PALW_MODEL_POSITION_UNITS_V1));
            println!(
                "  price       {} → {} per position",
                msk(m.price_sompi_per_position),
                msk(q.after.price_sompi_per_position_v1())
            );
            println!("  {}", paint::dim(&format!("misaka position buy {} --msk {text} [--slippage 1%]", m.line_id)));
        }
        (None, Some(n)) => {
            let q = palw_model_sell_quote_with(&market, n.saturating_mul(PALW_MODEL_POSITION_UNITS_V1), served_schedule(&m))
                .ok_or_else(|| {
                    CliError::new(exit::GENERIC, "the curve buys nothing back for that (not open, or more than it can take)")
                })?;
            println!("sell {n} positions of line {}…", &m.line_id[..16]);
            println!(
                "  fees        burn {} {} · owner {} {}",
                pct(m.burn_permille),
                msk(q.fees.burn),
                pct(m.leg_permille),
                msk(q.fees.registrant)
            );
            println!("  you get     {}", msk(q.fees.net));
            println!("  {}", paint::dim(&format!("misaka position sell {} --positions {n} [--slippage 1%]", m.line_id)));
        }
        _ => return Err(CliError::new(exit::GENERIC, "name one of --msk (a buy) or --positions (a sell)")),
    }
    Ok(())
}

/// `misaka position buy <line> --msk N [--slippage 1%] [--yes]`: the floor is the quote less the
/// slippage, and the existing buy path prints the whole move and submits only with `--yes`.
pub(crate) async fn position_buy(
    ctx: &crate::node::Ctx,
    profile: Profile,
    line: &str,
    msk_text: &str,
    slippage: &str,
    yes: bool,
) -> CliResult {
    let slip = parse_slippage(slippage)?;
    let ctx = ctx_for(ctx, &profile);
    let line_id = parse_line(line)?;
    let reader = crate::palw_derived::connect(&ctx).await?;
    let m = reader
        .client
        .get_palw_model_market(line_id.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    let amount = crate::palw_model::parse_msk_amount(msk_text)?;
    let q = palw_model_buy_quote_with(&market_from_response(&m), amount, served_schedule(&m)).ok_or_else(|| {
        CliError::new(exit::GENERIC, "the curve releases nothing for that amount (closed to buys, not open, or too small)")
    })?;
    let min_positions = floor_after(q.units_out, slip) / PALW_MODEL_POSITION_UNITS_V1;
    if ctx.output != OutputFormat::Json {
        println!(
            "slippage {} → at least {} positions, or the chain refuses the move",
            pct(slip),
            crate::operator::status::group(min_positions)
        );
    }
    crate::palw_model::buy(&ctx, &key_source(&profile)?, line, msk_text, min_positions, yes).await
}

/// `misaka position sell <line> --positions N [--slippage 1%] [--yes]`.
pub(crate) async fn position_sell(
    ctx: &crate::node::Ctx,
    profile: Profile,
    line: &str,
    positions: u64,
    slippage: &str,
    yes: bool,
) -> CliResult {
    let slip = parse_slippage(slippage)?;
    let ctx = ctx_for(ctx, &profile);
    let line_id = parse_line(line)?;
    let reader = crate::palw_derived::connect(&ctx).await?;
    let m = reader
        .client
        .get_palw_model_market(line_id.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelMarket: {e}")))?;
    let q = palw_model_sell_quote_with(
        &market_from_response(&m),
        positions.saturating_mul(PALW_MODEL_POSITION_UNITS_V1),
        served_schedule(&m),
    )
    .ok_or_else(|| CliError::new(exit::GENERIC, "the curve buys nothing back for that (not open, or more than it can take)"))?;
    let min_msk = floor_after(q.fees.net, slip);
    if ctx.output != OutputFormat::Json {
        println!("slippage {} → at least {}, or the chain refuses the move", pct(slip), msk(min_msk));
    }
    crate::palw_model::sell(&ctx, &key_source(&profile)?, line, positions, Some(format!("{min_msk}sompi")), yes).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slippage_reads_as_people_write_it_and_refuses_what_protects_nothing() {
        assert_eq!(parse_slippage("1%").unwrap(), 10);
        assert_eq!(parse_slippage("0.5 %").unwrap(), 5);
        assert_eq!(parse_slippage("2").unwrap(), 20);
        assert_eq!(parse_slippage(".5%").unwrap(), 5);
        assert!(parse_slippage("0.25%").is_err(), "one decimal: a permille");
        assert!(parse_slippage("60%").is_err(), "a floor more than half away protects nothing");
        assert!(parse_slippage("abc").is_err());
    }

    #[test]
    fn the_floor_is_the_quote_less_the_slippage_rounded_down() {
        assert_eq!(floor_after(44_612, 10), 44_165);
        assert_eq!(floor_after(1_000, 0), 1_000);
        assert_eq!(floor_after(u64::MAX, 10), ((u64::MAX as u128) * 990 / 1000) as u64, "no overflow");
    }
}
