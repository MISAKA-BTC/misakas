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

/// **Where a class is in its life** (ADR-0122 §8.2's `REGISTERED → CERTIFIED → LIVE`), from the class
/// table's status (its `Debug` spelling) and its share.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ClassStage {
    /// In the registry for a future DAA: adjudicable, no weight until the clock flips it.
    Registered {
        activation_daa: u64,
        pending_share: u16,
    },
    /// Active and holding no share: judged like any class, weightless until its block lane is
    /// certified.
    Weightless,
    /// Active and weight-bearing.
    Live {
        share: u16,
    },
    Frozen {
        since_daa: u64,
    },
    Dormant {
        since_daa: u64,
    },
    Unknown(String),
}

pub(crate) fn class_stage(status: &str, share: Option<u16>) -> ClassStage {
    let num = |key: &str| -> u64 {
        status
            .split(key)
            .nth(1)
            .map(|rest| rest.trim_start_matches([':', ' ']))
            .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|d| d.parse().ok())
            .unwrap_or(0)
    };
    match status.split([' ', '{']).next().unwrap_or(status) {
        "Active" => match share {
            Some(s) if s > 0 => ClassStage::Live { share: s },
            _ => ClassStage::Weightless,
        },
        "Registered" => ClassStage::Registered {
            activation_daa: num("activation_daa"),
            pending_share: num("pending_share_permille").min(u16::MAX as u64) as u16,
        },
        "Frozen" => ClassStage::Frozen { since_daa: num("since_daa") },
        "Dormant" => ClassStage::Dormant { since_daa: num("since_daa") },
        other => ClassStage::Unknown(other.to_string()),
    }
}

/// `misaka model status <model>`: one class's life on one screen — registered, active, its lanes
/// certified or not, live — with the artifact, the bond it takes, its market, and the next command.
pub(crate) async fn model_status(ctx: &crate::node::Ctx, profile: Profile, selector: &str) -> CliResult {
    let node = crate::operator::snapshot::connect_to(
        &profile.network,
        profile.rpc.as_deref().or(ctx.rpc.as_deref()),
        std::time::Duration::from_secs(ctx.timeout_secs.clamp(2, 15)),
    )
    .await
    .map_err(|(url, e)| CliError::new(exit::CONNECTION, format!("{url}: {e}")))?;
    if !node.ops_0122 {
        return Err(CliError::new(
            exit::COMPONENT_DOWN,
            format!("the node at {} predates getPalwClasses (ADR-0122) — rebuild it from this tree", node.url),
        ));
    }
    let table = node.client().get_palw_classes().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwClasses: {e}")))?;
    let classes = crate::operator::wizard::class_choices(&node, &table.classes).await;
    let class = crate::operator::wizard::resolve_model(&classes, selector)
        .map_err(|why| CliError::new(exit::MODEL, format!("{why} — misaka model list shows every class")))?
        .clone();
    let row = table.classes.iter().find(|c| c.class_id == class.id).expect("resolved from this table");
    let stage = class_stage(&row.status, row.share_permille);
    let facts = node.client().get_palw_producer_facts(class.id.clone(), String::new(), 0, false).await.ok().filter(|f| f.available);
    let market = node.client().get_palw_model_market(class.id.clone()).await.ok().filter(|m| m.found);
    let here: Vec<std::path::PathBuf> = crate::operator::wizard::artifacts_here(&profile.network, Some(&profile.appdir))
        .into_iter()
        .chain(profile.artifacts.iter().filter(|p| p.is_file()).cloned())
        .collect();
    let tip = node.daa();
    let name = class.label();
    let selector_arg = if class.is_base {
        "base".to_string()
    } else if class.name.is_empty() {
        class.id.clone()
    } else {
        class.name.clone()
    };
    if ctx.output == OutputFormat::Json {
        let doc = json!({
            "schema": "misaka.model.status.v1",
            "network": profile.network,
            "class_id": class.id,
            "name": class.name,
            "base": class.is_base,
            "status": row.status,
            "stage": format!("{stage:?}"),
            "share_permille": row.share_permille,
            "block_lane_weighted": matches!(stage, ClassStage::Live { .. }),
            "prompt_lane_certified": row.fp_certified,
            "held": row.held,
            "registered_daa": row.registered_daa,
            "artifact_root": row.artifact_root,
            "artifacts_here": here.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "bond_collateral_sompi": class.collateral,
            "epoch": facts.as_ref().map(|f| json!({ "index": f.epoch_index, "produced": f.epoch_produced_blocks, "budget": f.epoch_budget_blocks })),
            "market": market.as_ref().map(|m| json!({ "opened": m.opened, "price_sompi_per_position": m.price_sompi_per_position, "reserve_sompi": m.msk_reserve, "seed_pledged_sompi": m.seed_pledged_sompi, "seed_min_sompi": m.seed_min_sompi })),
            "tip_daa": tip,
        });
        println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
        return Ok(());
    }
    let group = crate::operator::status::group;
    println!("{}", paint::bold(&format!("MODEL · {name} · {}", profile.network)));
    println!("{}", paint::dim(&format!("class {} · registered at DAA {}", class.id, group(row.registered_daa))));
    println!();
    let epoch = facts.as_ref().map(|f| {
        if f.is_base_class {
            format!("{} blocks this epoch (the floor has no cap)", group(f.epoch_produced_blocks))
        } else {
            format!("{} of its {} blocks this epoch", group(f.epoch_produced_blocks), group(f.epoch_budget_blocks))
        }
    });
    let head = match &stage {
        ClassStage::Live { share } => paint::green(&format!(
            "● LIVE — weight-bearing at {share} ‰{}",
            epoch.as_ref().map(|e| format!(" · {e}")).unwrap_or_default()
        )),
        ClassStage::Weightless => {
            paint::yellow("◐ ACTIVE, WEIGHTLESS — its claims are judged, but it carries no weight until its block lane is certified")
        }
        ClassStage::Registered { activation_daa, pending_share } => paint::yellow(&format!(
            "◐ REGISTERED — becomes active at DAA {} ({}), at {pending_share} ‰",
            group(*activation_daa),
            if *activation_daa > tip { format!("in {}", group(activation_daa - tip)) } else { "due now".into() }
        )),
        ClassStage::Frozen { since_daa } => {
            paint::red(&format!("✗ FROZEN since DAA {} — contradicted: no weight and no new claims", group(*since_daa)))
        }
        ClassStage::Dormant { since_daa } => paint::dim(&format!("○ DORMANT since DAA {}", group(*since_daa))),
        ClassStage::Unknown(s) => format!("? {s}"),
    };
    println!("{head}");
    let mark = |done: bool| if done { paint::green("✓") } else { paint::dim("·") };
    let active = matches!(stage, ClassStage::Live { .. } | ClassStage::Weightless);
    let weighted = matches!(stage, ClassStage::Live { .. });
    println!("  {} {:<13} at DAA {}", mark(true), "registered", group(row.registered_daa));
    println!(
        "  {} {:<13} {}",
        mark(active),
        "active",
        if active { "adjudicable: its claims are judged like any class's".to_string() } else { "not yet".to_string() }
    );
    println!(
        "  {} {:<13} {}",
        mark(weighted),
        "block lane",
        match &stage {
            ClassStage::Live { share } => format!("weight-bearing at {share} ‰"),
            ClassStage::Weightless => "not certified — the next step".to_string(),
            _ => "—".to_string(),
        }
    );
    println!(
        "  {} {:<13} {}",
        mark(row.fp_certified),
        "prompt lane",
        if row.fp_certified { "certified: free-prompt claims enter state" } else { "not certified (optional)" }
    );
    println!();
    let root = &row.artifact_root;
    let artifact = if class.is_base {
        "none — the floor is derived; every node runs it".to_string()
    } else {
        match here.first() {
            Some(p) => format!(
                "root {}… · on this host: {} (root not checked: misaka mining setup --verify-artifact)",
                &root[..16.min(root.len())],
                crate::operator::host::tilde(p)
            ),
            None => format!("root {}… · none on this host", &root[..16.min(root.len())]),
        }
    };
    println!("  {:<11}{artifact}", "Artifact");
    if let Some(c) = class.collateral {
        println!("  {:<11}a bond for it locks {}", "Bond", crate::operator::catalog::msk(c as u128));
    }
    let market_line = match &market {
        Some(m) if m.opened => format!("open · {} per position · reserve {}", msk(m.price_sompi_per_position), msk(m.msk_reserve)),
        Some(m) if m.seed_pledged_sompi > 0 => format!("not open · seed {} of {}", msk(m.seed_pledged_sompi), msk(m.seed_min_sompi)),
        _ => "not open".to_string(),
    };
    println!("  {:<11}{market_line}", "Market");
    let quoted = if selector_arg.contains(' ') { format!("\"{selector_arg}\"") } else { selector_arg.clone() };
    let next: Vec<(String, &str)> = match &stage {
        ClassStage::Live { .. } => {
            let mut v = vec![(format!("misaka mining setup --model {quoted}"), "mine it")];
            if market.as_ref().is_some_and(|m| m.opened) {
                v.push((format!("misaka position quote {} --msk 100", class.id), "hold its positions"));
            }
            if !row.fp_certified && !class.is_base {
                v.push(("palw-certify drill --model-id \"<model id>\" --lane fp …".to_string(), "certify the prompt lane (optional)"));
            }
            v
        }
        ClassStage::Weightless => vec![
            (
                "palw-certify drill --model-id \"<model id>\" --lane attempt --out family-attempt.obj".to_string(),
                "once per family: skip it when a certified family already covers the kernels",
            ),
            (
                "misaka palw submit-object --key-file <seed> --object family-attempt.obj.chunkN --yes".to_string(),
                "each chunk, in order",
            ),
            (
                "palw-certify bind --model-id \"<model id>\" --lane attempt --out class-attempt.obj".to_string(),
                "bind this class to the family",
            ),
            (
                "misaka palw submit-object --key-file <seed> --object class-attempt.obj --yes".to_string(),
                "then misaka model status again",
            ),
        ],
        ClassStage::Registered { .. } => {
            vec![("misaka model status again at that DAA".to_string(), "nothing to do: the flip is a clock, not an object")]
        }
        _ => Vec::new(),
    };
    for (i, (cmd, why)) in next.iter().enumerate() {
        println!("  {:<11}{:<72} {}", if i == 0 { "Next" } else { "" }, cmd, paint::dim(why));
    }
    if matches!(stage, ClassStage::Weightless) {
        println!("  {:<11}{}", "", paint::dim("docs/palw-certify-a-new-model.md"));
    }
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
    /// The class table spells a status with `Debug`; the stage is read back from that spelling and
    /// the share, and an Active class with no share is weightless, not live.
    #[test]
    fn a_class_stage_is_read_from_the_tables_own_spelling() {
        use super::{ClassStage as S, class_stage};
        assert_eq!(class_stage("Active", Some(150)), S::Live { share: 150 });
        assert_eq!(class_stage("Active", None), S::Weightless);
        assert_eq!(class_stage("Active", Some(0)), S::Weightless, "a share of nothing carries nothing");
        assert_eq!(
            class_stage("Registered { activation_daa: 7000, pending_share_permille: 150 }", None),
            S::Registered { activation_daa: 7000, pending_share: 150 }
        );
        assert_eq!(class_stage("Frozen { since_daa: 812 }", None), S::Frozen { since_daa: 812 });
        assert_eq!(class_stage("Dormant { since_daa: 9 }", None), S::Dormant { since_daa: 9 });
        assert_eq!(class_stage("Retired", None), S::Unknown("Retired".into()));
    }
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
