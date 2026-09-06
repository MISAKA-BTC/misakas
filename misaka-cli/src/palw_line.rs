//! **`misaka palw line-… / version-… / proposal-… / evaluate` — ADR-0088's registry, read and
//! written.**
//!
//! The reads are the tip's (`getPalwModelLine`, `getPalwModelVersion`, `getPalwModelLines`,
//! `getPalwModelProposals`). The writers each build one of the registry's ten objects, signed by
//! the bond the fold will attribute it to — the line's owner or developer for a role move, the
//! bond named by `--bond` for a founding, a proposal or an evaluation — under
//! `PALW_MODEL_LINE_MLDSA87_CONTEXT` over the message `palw_model_lines_v1` spells, with the
//! network domain the node names; and they carry it exactly as `model-sell` does (one lifecycle
//! carrier, priced from its own mass, the chain's rent on top for the three rent-priced objects).
//!
//! Every writer reads the line first and refuses locally what the fold would refuse: a key that
//! is not the attributed bond's key, a version number that is not the next, a preview past the
//! bound, a proposal the line does not hold. Consensus verifies the signature against the
//! ATTRIBUTED bond's registered key, so a carrier signed by any other key is a fee spent on a
//! transaction that can never apply — and the operator should learn that here, not from a dropped
//! object. Nothing is sent without `--yes`.

use crate::bond::{network_domain, parse_outpoint};
use crate::node::Ctx;
use crate::wallet::{NodeView, connect};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_model_lines_v1::{
    PALW_MODEL_EVALUATIONS_PER_VERSION_V1, PALW_MODEL_LINE_MLDSA87_CONTEXT, PALW_MODEL_LINE_NAME_MAX_BYTES,
    PALW_MODEL_LINES_PER_CLASS_V1, PALW_MODEL_PREVIEWS_V1, PALW_MODEL_PROPOSALS_PER_LINE_V1, PALW_MODEL_VERSION_HISTORY_V1,
    model_line_id_v1, model_proposal_id_v1, palw_model_evaluation_message_v1, palw_model_line_founded_message_v1,
    palw_model_proposal_close_message_v1, palw_model_proposal_message_v1, palw_model_retire_message_v1, palw_model_roles_message_v1,
    palw_model_transfer_message_v1, palw_model_version_message_v1, palw_model_version_move_message_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_pq_validator_core::ValidatorKey;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwModelLineResponse, RpcPalwModelLine, RpcPalwModelVersion, RpcTransactionOutpoint};

// ---- parsing and printing ---------------------------------------------------------------------

fn parse_hash(text: &str, what: &str) -> Result<Hash64, CliError> {
    text.parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("{what} '{text}' is not a 128-hex Hash64")))
}

fn parse_bond(text: &str) -> Result<PalwBondKeyV2, CliError> {
    parse_outpoint(text).map(PalwBondKeyV2)
}

fn outpoint_text(o: &RpcTransactionOutpoint) -> String {
    format!("{}:{}", o.transaction_id, o.index)
}

fn bond_text(b: &PalwBondKeyV2) -> String {
    format!("{}:{}", b.0.transaction_id, b.0.index)
}

fn bond_of(o: &RpcTransactionOutpoint) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::from(*o))
}

/// A role as the row holds it: the bond, or "the owner" when the row names none (Decision 6).
fn role_text(role: Option<&RpcTransactionOutpoint>, payload: Option<&String>, owner_payload: Option<&String>) -> String {
    match role {
        Some(o) => format!("{}  payout {}", outpoint_text(o), payload.map(String::as_str).unwrap_or("(bond not in the registry)")),
        None => format!("(the owner)  payout {}", owner_payload.map(String::as_str).unwrap_or("-")),
    }
}

fn line_json(l: &RpcPalwModelLine) -> serde_json::Value {
    serde_json::json!({
        "line_id": l.line_id,
        "class_id": l.class_id,
        "has_row": l.has_row,
        "owner": l.owner.as_ref().map(outpoint_text),
        "owner_payout_payload": l.owner_payout_payload,
        "developer": l.developer.as_ref().map(outpoint_text),
        "developer_payout_payload": l.developer_payout_payload,
        "maintainer": l.maintainer.as_ref().map(outpoint_text),
        "maintainer_payout_payload": l.maintainer_payout_payload,
        "name": l.name,
        "name_hex": l.name_hex,
        "founded_daa": l.founded_daa,
        "current": l.current,
        "previews": l.previews,
        "versions_published": l.versions_published,
        "contributor_permille_of_leg": l.contributor_permille_of_leg,
        "status": l.status,
        "retired_daa": l.retired_daa,
    })
}

fn version_json(v: &RpcPalwModelVersion) -> serde_json::Value {
    serde_json::json!({
        "line_id": v.line_id,
        "version": v.version,
        "root": v.root,
        "parent": v.parent,
        "adopted_from": v.adopted_from,
        "declared": {
            "runtime_hash": v.runtime_hash,
            "dataset_commitment": v.dataset_commitment,
            "training_config_hash": v.training_config_hash,
            "notes_hash": v.notes_hash,
        },
        "published_daa": v.published_daa,
        "published_by": v.published_by.as_ref().map(outpoint_text),
        "status": v.status,
        "until_daa": v.until_daa,
        "in_force": v.in_force,
        "usage": {
            "attempt_claims": v.attempt_claims,
            "fp_claims": v.fp_claims,
            "work_leaves": v.work_leaves,
            "first_used_daa": v.first_used_daa,
            "last_used_daa": v.last_used_daa,
        },
    })
}

fn print_line(l: &RpcPalwModelLine) {
    println!("line {}", l.line_id);
    println!("  class          {}", l.class_id);
    println!(
        "  name           {}{}",
        if l.name.is_empty() { "(none on the chain)" } else { &l.name },
        if l.has_row { "" } else { "   [founding line, no row yet]" }
    );
    println!("  status         {}{}", l.status, l.retired_daa.map(|d| format!(" (roots leave force at DAA {d})")).unwrap_or_default());
    println!("  founded        DAA {}", l.founded_daa);
    match &l.owner {
        Some(o) => println!(
            "  owner          {}  payout {}",
            outpoint_text(o),
            l.owner_payout_payload.as_deref().unwrap_or("(bond not in the registry)")
        ),
        None => println!("  owner          (none — a genesis class's line; nobody may publish)"),
    }
    println!(
        "  developer      {}",
        role_text(l.developer.as_ref(), l.developer_payout_payload.as_ref(), l.owner_payout_payload.as_ref())
    );
    println!(
        "  maintainer     {}",
        role_text(l.maintainer.as_ref(), l.maintainer_payout_payload.as_ref(), l.owner_payout_payload.as_ref())
    );
    println!("  current        V{}", l.current);
    println!(
        "  previews       {}",
        if l.previews.is_empty() {
            "none".to_string()
        } else {
            l.previews.iter().map(|v| format!("V{v}")).collect::<Vec<_>>().join(", ")
        }
    );
    println!("  published      {} version(s); the next is V{}", l.versions_published, l.versions_published.saturating_add(1));
    println!("  contributor    {} ‰ of the owner's leg while an adopted version is current", l.contributor_permille_of_leg);
}

fn version_status_text(v: &RpcPalwModelVersion) -> String {
    match (v.status.as_str(), v.until_daa) {
        ("Superseded", Some(until)) => format!("Superseded (root in force until DAA {until})"),
        (s, _) => s.to_string(),
    }
}

fn print_version(v: &RpcPalwModelVersion) {
    println!("V{}  {}{}", v.version, version_status_text(v), if v.in_force { "  [in force]" } else { "" });
    println!("  root           {}", v.root);
    println!("  parent         {}", v.parent.map(|p| format!("V{p}")).unwrap_or_else(|| "none".to_string()));
    println!("  adopted from   {}", v.adopted_from.as_deref().unwrap_or("none"));
    println!(
        "  published      DAA {} by {}",
        v.published_daa,
        v.published_by.as_ref().map(outpoint_text).unwrap_or_else(|| "(the registration)".to_string())
    );
    println!("  declared       runtime {}", v.runtime_hash.as_deref().unwrap_or("-"));
    println!("                 dataset {}", v.dataset_commitment.as_deref().unwrap_or("-"));
    println!("                 training {}", v.training_config_hash.as_deref().unwrap_or("-"));
    println!("                 notes {}", v.notes_hash.as_deref().unwrap_or("-"));
    println!(
        "  usage          {} attempt claims, {} free-prompt claims, {} leaves{}",
        v.attempt_claims,
        v.fp_claims,
        v.work_leaves,
        match (v.first_used_daa, v.last_used_daa) {
            (Some(a), Some(b)) => format!(" (DAA {a}..{b})"),
            _ => String::new(),
        }
    );
}

// ---- reads -----------------------------------------------------------------------------------

async fn read_line(nv: &NodeView, line: Hash64) -> Result<GetPalwModelLineResponse, CliError> {
    nv.client
        .get_palw_model_line(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelLine: {e}")))
}

/// The line, or the refusal `exists: false` is.
async fn require_line(nv: &NodeView, line: Hash64) -> Result<(GetPalwModelLineResponse, RpcPalwModelLine), CliError> {
    let r = read_line(nv, line).await?;
    match r.line.clone() {
        Some(row) if r.exists => Ok((r, row)),
        _ => Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line} (and no class of that id)"))),
    }
}

/// `misaka palw line-show <line>`: the row, the current root, the roots in force for the class.
pub async fn line_show(ctx: &Ctx, line_id: &str, json: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let nv = connect(ctx).await?;
    let r = read_line(&nv, line).await?;
    let _ = nv.client.disconnect().await;
    if json || ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "misaka.palw.model-line.v1",
                "exists": r.exists,
                "line_id": r.line_id,
                "line": r.line.as_ref().map(line_json),
                "current_root": r.current_root,
                "roots_in_force": r.roots_in_force,
                "tip_daa": r.tip_daa,
            }))
            .expect("serializable")
        );
    } else if let Some(l) = r.line.as_ref().filter(|_| r.exists) {
        print_line(l);
        println!("  current root   {}", r.current_root.as_deref().unwrap_or("(the current version is not in state)"));
        println!("  in force       {} root(s) for the class at DAA {}", r.roots_in_force.len(), r.tip_daa);
        for root in &r.roots_in_force {
            println!("                 {root}");
        }
    } else {
        println!("this chain holds no line {line} (and no class of that id)");
    }
    if !r.exists {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line}")));
    }
    Ok(())
}

/// `misaka palw line-log <line>`: every version the node holds, oldest first. The node keeps the
/// last `PALW_MODEL_VERSION_HISTORY_V1` per line; older ones are named as evicted.
pub async fn line_log(ctx: &Ctx, line_id: &str, json: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    let first = row.versions_published.saturating_sub(PALW_MODEL_VERSION_HISTORY_V1 - 1).max(1);
    let mut versions: Vec<(u32, Option<RpcPalwModelVersion>)> = Vec::new();
    for n in first..=row.versions_published {
        let v = nv
            .client
            .get_palw_model_version(line.to_string(), n)
            .await
            .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelVersion({n}): {e}")))?;
        versions.push((n, v.version.filter(|_| v.exists)));
    }
    let _ = nv.client.disconnect().await;
    if json || ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "misaka.palw.model-line-log.v1",
                "line_id": line.to_string(),
                "current": row.current,
                "versions_published": row.versions_published,
                "evicted_before": first,
                "versions": versions.iter().map(|(n, v)| match v {
                    Some(v) => version_json(v),
                    None => serde_json::json!({ "version": n, "evicted": true }),
                }).collect::<Vec<_>>(),
            }))
            .expect("serializable")
        );
        return Ok(());
    }
    println!("line {}  ({} version(s) published, V{} current)", line, row.versions_published, row.current);
    if first > 1 {
        println!("  V1..V{} left the state (the explorer holds them)", first - 1);
    }
    for (n, v) in &versions {
        match v {
            Some(v) => print_version(v),
            None => println!("V{n}  (not in state)"),
        }
    }
    Ok(())
}

/// `misaka palw line-list <class>`: every line of the class, the founding line included.
pub async fn line_list(ctx: &Ctx, class_id: &str, json: bool) -> CliResult {
    let class = parse_hash(class_id, "class id")?;
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_lines(class.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelLines: {e}")))?;
    let _ = nv.client.disconnect().await;
    if json || ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "misaka.palw.model-lines.v1",
                "exists": r.exists,
                "class_id": r.class_id,
                "lines": r.lines.iter().map(line_json).collect::<Vec<_>>(),
            }))
            .expect("serializable")
        );
    } else if !r.exists {
        println!("this chain registers no class {class}");
    } else {
        println!("class {}  {} line(s)", r.class_id, r.lines.len());
        for l in &r.lines {
            println!(
                "  {}  {}  {}  V{} current, {} published{}",
                l.line_id,
                if l.name.is_empty() { "(unnamed)" } else { &l.name },
                l.status,
                l.current,
                l.versions_published,
                if l.has_row { "" } else { "  [founding line, no row yet]" }
            );
        }
    }
    if !r.exists {
        return Err(CliError::new(exit::GENERIC, format!("this chain registers no class {class}")));
    }
    Ok(())
}

/// `misaka palw proposals <line>`: the proposals attached to a line.
pub async fn proposals(ctx: &Ctx, line_id: &str, json: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let nv = connect(ctx).await?;
    let r = nv
        .client
        .get_palw_model_proposals(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelProposals: {e}")))?;
    let _ = nv.client.disconnect().await;
    if json || ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema": "misaka.palw.model-proposals.v1",
                "exists": r.exists,
                "line_id": r.line_id,
                "proposals": r.proposals.iter().map(|p| serde_json::json!({
                    "proposal_id": p.proposal_id, "root": p.root, "note_hash": p.note_hash,
                    "by": outpoint_text(&p.by), "posted_daa": p.posted_daa, "adopted_in": p.adopted_in,
                })).collect::<Vec<_>>(),
            }))
            .expect("serializable")
        );
    } else if !r.exists {
        println!("this chain holds no line {line}");
    } else if r.proposals.is_empty() {
        println!("line {line} holds no proposal");
    } else {
        println!("line {}  {} proposal(s)", r.line_id, r.proposals.len());
        for p in &r.proposals {
            println!("  {}", p.proposal_id);
            println!("    root       {}", p.root);
            println!("    note       {}", p.note_hash);
            println!("    by         {}  at DAA {}", outpoint_text(&p.by), p.posted_daa);
            println!("    adopted    {}", p.adopted_in.map(|v| format!("in V{v}")).unwrap_or_else(|| "not yet".to_string()));
        }
    }
    if !r.exists {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line}")));
    }
    Ok(())
}

// ---- the writers' common path -------------------------------------------------------------------

/// **The key must be the attributed bond's key.** Consensus checks a registry object's signature
/// against the stored key of the bond it is attributed to (the fold names which — a founder, a
/// line's developer, its owner, a proposer, an evaluator), so a signature from any other key is a
/// carrier fee spent on nothing. The node reports a bond's registered key only beside a class it
/// knows, which the line's class is.
async fn assert_key_owns_bond(
    nv: &NodeView,
    key: &ValidatorKey,
    class_id: Hash64,
    bond: &PalwBondKeyV2,
    role: &str,
) -> Result<(), CliError> {
    let facts = nv
        .client
        .get_palw_producer_facts(class_id.to_string(), bond.0.transaction_id.to_string(), bond.0.index, true)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwProducerFacts: {e}")))?;
    if !facts.available || !facts.bond_known {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "this node did not answer for bond {} ({role}) under class {class_id} — either it is not a ConsensusV2 network, or it does not know that class or that bond",
                bond_text(bond)
            ),
        ));
    }
    let ours = faster_hex::hex_string(key.public_key());
    if facts.bond_registered_pubkey.is_empty() || !ours.eq_ignore_ascii_case(&facts.bond_registered_pubkey) {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the {role} is bond {}, which was registered by a different key — consensus verifies the signature against that bond's key, so this key's signature would be refused",
                bond_text(bond)
            ),
        ));
    }
    Ok(())
}

/// The bond a role names on the row, or the refusal an unowned line is.
fn role_bond(row: &RpcPalwModelLine, role: &str) -> Result<PalwBondKeyV2, CliError> {
    let raw = match role {
        "owner" => row.owner.as_ref(),
        // Decision 6: `None` means the owner.
        "developer" => row.developer.as_ref().or(row.owner.as_ref()),
        _ => row.maintainer.as_ref().or(row.owner.as_ref()),
    };
    raw.map(bond_of).ok_or_else(|| {
        CliError::new(exit::GENERIC, format!("line {} has no {role}: an unowned (genesis) line, nobody may act on it", row.line_id))
    })
}

fn require_active(row: &RpcPalwModelLine) -> Result<(), CliError> {
    if row.status != "Active" {
        return Err(CliError::new(exit::GENERIC, format!("line {} is {}, not Active", row.line_id, row.status)));
    }
    Ok(())
}

/// Sign the registry message with the key, under the registry's one context.
fn sign(key: &ValidatorKey, message: &Hash64) -> Vec<u8> {
    key.sign_with_context(message.as_byte_slice(), PALW_MODEL_LINE_MLDSA87_CONTEXT).to_vec()
}

/// One carrier, funded from the key's address and priced like every other lifecycle carrier
/// (`build_carrier_v1`: mass-priced, the chain's rent on top), then the dry-run / submit split
/// `model-sell` uses.
async fn carry(ctx: &Ctx, nv: &NodeView, key: &ValidatorKey, object: &PalwConsensusObjectV2, what: &str, yes: bool) -> CliResult {
    let rent = kaspa_consensus_core::palw_state_v2::palw_object_rent_ceiling_v1(object);
    let floor = kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI;
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(nv, &addr).await?;
    let (outpoint, entry) = candidates.into_iter().find(|(_, e)| e.amount > rent.saturating_add(floor)).ok_or_else(|| {
        CliError::new(
            exit::GENERIC,
            format!(
                "no mature, unbonded UTXO at {addr} holds the carrier's fee{}",
                if rent > 0 { format!(" and its {rent} sompi rent") } else { String::new() }
            ),
        )
    })?;
    let (tx, fee) =
        crate::palw_fp::build_carrier_v1(key, nv, object, outpoint, &entry).map_err(|e| CliError::new(exit::GENERIC, e))?;
    if rent > 0 && ctx.output != OutputFormat::Json {
        println!("  rent           {rent} sompi of the {fee} sompi fee is burned (ADR-0088 Decision 11)");
    }
    crate::palw_model::submit_move(ctx, nv, tx, fee, what, yes).await
}

// ---- writers ----------------------------------------------------------------------------------

/// `misaka palw line-found --class <id> --name <text> --root <hex> --bond <outpoint> --key … [--yes]`.
pub async fn line_found(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    class_id: &str,
    name: &str,
    root: &str,
    bond: &str,
    yes: bool,
) -> CliResult {
    let class = parse_hash(class_id, "class id")?;
    let root = parse_hash(root, "root")?;
    let founder = parse_bond(bond)?;
    let name_bytes = name.as_bytes().to_vec();
    if name_bytes.is_empty() || name_bytes.len() > PALW_MODEL_LINE_NAME_MAX_BYTES {
        return Err(CliError::new(
            exit::GENERIC,
            format!("a line's name is 1..={PALW_MODEL_LINE_NAME_MAX_BYTES} bytes; '{name}' is {}", name_bytes.len()),
        ));
    }
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    // The class exists iff its founding line reads (its id is the class id).
    let (_, founding) = require_line(&nv, class).await?;
    if founding.class_id != class.to_string() {
        return Err(CliError::new(exit::GENERIC, format!("{class} is a line of class {}, not a class", founding.class_id)));
    }
    let lines = nv
        .client
        .get_palw_model_lines(class.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelLines: {e}")))?;
    if lines.lines.len() >= PALW_MODEL_LINES_PER_CLASS_V1 {
        return Err(CliError::new(exit::GENERIC, format!("class {class} already holds {} lines, the most it may", lines.lines.len())));
    }
    let line_id = model_line_id_v1(&class, &founder, &name_bytes);
    if lines.lines.iter().any(|l| l.line_id == line_id.to_string()) {
        return Err(CliError::new(exit::GENERIC, format!("line {line_id} — this founder's '{name}' on this class — already exists")));
    }
    assert_key_owns_bond(&nv, &key, class, &founder, "founder").await?;
    let message = palw_model_line_founded_message_v1(network_domain(&nv), &class, &name_bytes, &founder, &root);
    let object =
        PalwConsensusObjectV2::ModelLineFounded { class_id: class, name: name_bytes, founder, root, signature: sign(&key, &message) };
    if ctx.output != OutputFormat::Json {
        println!("found line '{name}' on class {class}");
        println!("  line id        {line_id}");
        println!("  founder        {} (owner, developer and maintainer)", bond_text(&founder));
        println!("  V1 root        {root}");
    }
    let what = format!("ModelLineFounded '{name}' on class {class} → line {line_id}");
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw version-publish --line <id> --root <hex> [--parent n] [--adopted-from <id>] [hashes…] [--preview] --key … [--yes]`.
/// The version number is read off the chain — `versions_published + 1` — never guessed.
#[allow(clippy::too_many_arguments)]
pub async fn version_publish(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    line_id: &str,
    root: &str,
    parent: Option<u32>,
    adopted_from: Option<String>,
    runtime_hash: Option<String>,
    dataset_commitment: Option<String>,
    training_config_hash: Option<String>,
    notes_hash: Option<String>,
    preview: bool,
    yes: bool,
) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let root = parse_hash(root, "root")?;
    let adopted_from = adopted_from.as_deref().map(|h| parse_hash(h, "proposal id")).transpose()?;
    let runtime_hash = runtime_hash.as_deref().map(|h| parse_hash(h, "runtime hash")).transpose()?;
    let dataset_commitment = dataset_commitment.as_deref().map(|h| parse_hash(h, "dataset commitment")).transpose()?;
    let training_config_hash = training_config_hash.as_deref().map(|h| parse_hash(h, "training config hash")).transpose()?;
    let notes_hash = notes_hash.as_deref().map(|h| parse_hash(h, "notes hash")).transpose()?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (r, row) = require_line(&nv, line).await?;
    require_active(&row)?;
    let version = row.versions_published.saturating_add(1);
    if let Some(p) = parent.filter(|p| *p == 0 || *p > row.versions_published) {
        return Err(CliError::new(
            exit::GENERIC,
            format!("--parent V{p} does not exist; the line has published V1..V{}", row.versions_published),
        ));
    }
    if r.current_root.as_deref() == Some(root.to_string().as_str()) {
        return Err(CliError::new(
            exit::GENERIC,
            format!("{root} is already the current root of line {line}; a no-op version is refused on chain"),
        ));
    }
    if preview && row.previews.len() >= PALW_MODEL_PREVIEWS_V1 {
        return Err(CliError::new(
            exit::GENERIC,
            format!("line {line} already holds {} preview(s), the most it may; promote or withdraw one first", row.previews.len()),
        ));
    }
    if let Some(proposal) = adopted_from {
        let held = nv
            .client
            .get_palw_model_proposals(line.to_string())
            .await
            .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelProposals: {e}")))?;
        if !held.proposals.iter().any(|p| p.proposal_id == proposal.to_string()) {
            return Err(CliError::new(exit::GENERIC, format!("line {line} holds no proposal {proposal}")));
        }
    }
    let developer = role_bond(&row, "developer")?;
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &developer, "line's developer").await?;
    let message = palw_model_version_message_v1(
        network_domain(&nv),
        &line,
        version,
        &root,
        parent,
        adopted_from.as_ref(),
        runtime_hash.as_ref(),
        dataset_commitment.as_ref(),
        training_config_hash.as_ref(),
        notes_hash.as_ref(),
        preview,
    );
    let object = PalwConsensusObjectV2::ModelVersionPublished {
        line_id: line,
        version,
        root,
        parent,
        adopted_from,
        runtime_hash,
        dataset_commitment,
        training_config_hash,
        notes_hash,
        preview,
        signature: sign(&key, &message),
    };
    if ctx.output != OutputFormat::Json {
        let effect = if preview {
            format!("as a PREVIEW (the current stays V{})", row.current)
        } else {
            format!("— it becomes current, V{} superseded with a grace", row.current)
        };
        println!("publish V{version} of line {line} {effect}");
        println!("  root           {root}");
        println!("  parent         {}", parent.map(|p| format!("V{p}")).unwrap_or_else(|| "none".to_string()));
        println!("  adopted from   {}", adopted_from.map(|h| h.to_string()).unwrap_or_else(|| "none".to_string()));
        println!("  developer      {}", bond_text(&developer));
        println!("  declared       the four hashes are recorded and labelled, never checked");
    }
    let what = format!("ModelVersionPublished V{version} of line {line}{}", if preview { " (preview)" } else { "" });
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw version-promote` / `version-withdraw --line <id> --version n --key … [--yes]`.
pub async fn version_move(ctx: &Ctx, ks: &crate::keys::KeySource, line_id: &str, version: u32, promote: bool, yes: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    require_active(&row)?;
    let v = nv
        .client
        .get_palw_model_version(line.to_string(), version)
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelVersion: {e}")))?;
    let Some(v) = v.version.filter(|_| v.exists) else {
        return Err(CliError::new(exit::GENERIC, format!("line {line} holds no V{version} in state")));
    };
    if promote && v.status != "Preview" {
        return Err(CliError::new(exit::GENERIC, format!("V{version} is {}, not a preview; only a preview is promoted", v.status)));
    }
    if !promote && v.status != "Preview" && v.status != "Superseded" {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "V{version} is {}; only a preview or a superseded version is withdrawn — a current version is succeeded, never withdrawn",
                v.status
            ),
        ));
    }
    let developer = role_bond(&row, "developer")?;
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &developer, "line's developer").await?;
    let kind: &[u8] = if promote { b"promote" } else { b"withdraw" };
    let message = palw_model_version_move_message_v1(network_domain(&nv), &line, version, kind);
    let signature = sign(&key, &message);
    let object = if promote {
        PalwConsensusObjectV2::ModelVersionPromoted { line_id: line, version, signature }
    } else {
        PalwConsensusObjectV2::ModelVersionWithdrawn { line_id: line, version, signature }
    };
    if ctx.output != OutputFormat::Json {
        if promote {
            println!("promote V{version} of line {line} to current (V{} becomes superseded with a grace)", row.current);
        } else {
            println!("withdraw V{version} of line {line} from force at once");
        }
        println!("  root           {}", v.root);
        println!("  developer      {}", bond_text(&developer));
    }
    let what = format!("{} V{version} of line {line}", if promote { "ModelVersionPromoted" } else { "ModelVersionWithdrawn" });
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// A role flag: omitted keeps the row's value, `owner` resets it to the owner (Decision 6's
/// `None`), anything else is an outpoint.
fn role_arg(text: Option<&str>, current: Option<&RpcTransactionOutpoint>) -> Result<Option<PalwBondKeyV2>, CliError> {
    match text {
        None => Ok(current.map(bond_of)),
        Some("owner") => Ok(None),
        Some(s) => parse_bond(s).map(Some),
    }
}

/// `misaka palw line-roles --line <id> [--developer <outpoint>|owner] [--maintainer <outpoint>|owner] [--contributor-permille n] --key … [--yes]`.
pub async fn line_roles(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    line_id: &str,
    developer: Option<String>,
    maintainer: Option<String>,
    contributor_permille: Option<u16>,
    yes: bool,
) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    require_active(&row)?;
    let developer = role_arg(developer.as_deref(), row.developer.as_ref())?;
    let maintainer = role_arg(maintainer.as_deref(), row.maintainer.as_ref())?;
    let permille = match contributor_permille {
        Some(p) if p > 1000 => return Err(CliError::new(exit::GENERIC, format!("--contributor-permille {p} is over 1000"))),
        Some(p) => p,
        None => row.contributor_permille_of_leg.min(1000) as u16,
    };
    let owner = role_bond(&row, "owner")?;
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &owner, "line's owner").await?;
    let message = palw_model_roles_message_v1(network_domain(&nv), &line, developer.as_ref(), maintainer.as_ref(), permille);
    let object = PalwConsensusObjectV2::ModelLineRolesSet {
        line_id: line,
        developer,
        maintainer,
        contributor_permille_of_leg: permille,
        signature: sign(&key, &message),
    };
    if ctx.output != OutputFormat::Json {
        println!("set the roles of line {line}");
        println!("  owner          {}", bond_text(&owner));
        println!("  developer      {}", developer.as_ref().map(bond_text).unwrap_or_else(|| "(the owner)".to_string()));
        println!("  maintainer     {}", maintainer.as_ref().map(bond_text).unwrap_or_else(|| "(the owner)".to_string()));
        println!("  contributor    {permille} ‰ of the owner's leg while an adopted version is current");
    }
    let what = format!("ModelLineRolesSet on line {line}");
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw line-transfer --line <id> --new-owner <outpoint> --key … [--yes]`.
pub async fn line_transfer(ctx: &Ctx, ks: &crate::keys::KeySource, line_id: &str, new_owner: &str, yes: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let new_owner = parse_bond(new_owner)?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    require_active(&row)?;
    let owner = role_bond(&row, "owner")?;
    if owner == new_owner {
        return Err(CliError::new(exit::GENERIC, format!("{} already owns line {line}", bond_text(&owner))));
    }
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &owner, "line's owner").await?;
    let message = palw_model_transfer_message_v1(network_domain(&nv), &line, &new_owner);
    let object = PalwConsensusObjectV2::ModelLineOwnerTransferred { line_id: line, new_owner, signature: sign(&key, &message) };
    if ctx.output != OutputFormat::Json {
        println!("transfer line {line}");
        println!("  from           {}", bond_text(&owner));
        println!("  to             {} (must be an Active bond; developer and maintainer reset to it)", bond_text(&new_owner));
        println!("  positions      unmoved — development rights transfer, positions do not (ADR-0087 Decision 5)");
    }
    let what = format!("ModelLineOwnerTransferred line {line} to {}", bond_text(&new_owner));
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw line-retire --line <id> --key … [--yes]`.
pub async fn line_retire(ctx: &Ctx, ks: &crate::keys::KeySource, line_id: &str, yes: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    require_active(&row)?;
    let owner = role_bond(&row, "owner")?;
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &owner, "line's owner").await?;
    let message = palw_model_retire_message_v1(network_domain(&nv), &line);
    let object = PalwConsensusObjectV2::ModelLineRetired { line_id: line, signature: sign(&key, &message) };
    if ctx.output != OutputFormat::Json {
        println!("retire line {line}");
        println!("  owner          {}", bond_text(&owner));
        println!(
            "  effect         the market closes to buys (sells continue), the roots leave force after the grace, the history stays"
        );
    }
    let what = format!("ModelLineRetired line {line}");
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw proposal-post --line <id> --root <hex> --note-hash <hex> --bond <outpoint> --key … [--yes]`.
pub async fn proposal_post(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    line_id: &str,
    root: &str,
    note_hash: &str,
    bond: &str,
    yes: bool,
) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let root = parse_hash(root, "root")?;
    let note_hash = parse_hash(note_hash, "note hash")?;
    let by = parse_bond(bond)?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    require_active(&row)?;
    let held = nv
        .client
        .get_palw_model_proposals(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelProposals: {e}")))?;
    if held.proposals.len() >= PALW_MODEL_PROPOSALS_PER_LINE_V1 {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "line {line} already holds {} proposals, the most it may; the developer closes one to make room",
                held.proposals.len()
            ),
        ));
    }
    let proposal_id = model_proposal_id_v1(&line, &root, &by);
    if held.proposals.iter().any(|p| p.proposal_id == proposal_id.to_string()) {
        return Err(CliError::new(
            exit::GENERIC,
            format!("proposal {proposal_id} — this root from this bond on this line — is already posted"),
        ));
    }
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &by, "proposer").await?;
    let message = palw_model_proposal_message_v1(network_domain(&nv), &line, &root, &note_hash, &by);
    let object = PalwConsensusObjectV2::ModelProposalPosted { line_id: line, root, note_hash, by, signature: sign(&key, &message) };
    if ctx.output != OutputFormat::Json {
        println!("post a proposal on line {line}");
        println!("  proposal id    {proposal_id}");
        println!("  root           {root}");
        println!("  note           {note_hash}");
        println!("  by             {}", bond_text(&by));
    }
    let what = format!("ModelProposalPosted {proposal_id} on line {line}");
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw proposal-close --line <id> --proposal <id> --key … [--yes]`.
pub async fn proposal_close(ctx: &Ctx, ks: &crate::keys::KeySource, line_id: &str, proposal: &str, yes: bool) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let proposal_id = parse_hash(proposal, "proposal id")?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    let held = nv
        .client
        .get_palw_model_proposals(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelProposals: {e}")))?;
    let Some(p) = held.proposals.iter().find(|p| p.proposal_id == proposal_id.to_string()) else {
        return Err(CliError::new(exit::GENERIC, format!("line {line} holds no proposal {proposal_id}")));
    };
    let developer = role_bond(&row, "developer")?;
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &developer, "line's developer").await?;
    let message = palw_model_proposal_close_message_v1(network_domain(&nv), &line, &proposal_id);
    let object = PalwConsensusObjectV2::ModelProposalClosed { line_id: line, proposal_id, signature: sign(&key, &message) };
    if ctx.output != OutputFormat::Json {
        println!("close proposal {proposal_id} on line {line}");
        println!("  root           {}", p.root);
        println!("  by             {}", outpoint_text(&p.by));
        println!("  developer      {}", bond_text(&developer));
    }
    let what = format!("ModelProposalClosed {proposal_id} on line {line}");
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// `misaka palw evaluate --line <id> --version n --evaluator-id <hex> --score-permille n --report-hash <hex> --bond <outpoint> --key … [--yes]`.
#[allow(clippy::too_many_arguments)]
pub async fn evaluate(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    line_id: &str,
    version: u32,
    evaluator_id: &str,
    score_permille: u32,
    report_hash: &str,
    bond: &str,
    yes: bool,
) -> CliResult {
    let line = parse_hash(line_id, "line id")?;
    let evaluator_id = parse_hash(evaluator_id, "evaluator id")?;
    let report_hash = parse_hash(report_hash, "report hash")?;
    let by = parse_bond(bond)?;
    if score_permille > 1000 {
        return Err(CliError::new(exit::GENERIC, format!("--score-permille {score_permille} is over 1000")));
    }
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let (_, row) = require_line(&nv, line).await?;
    let v = nv
        .client
        .get_palw_model_version(line.to_string(), version)
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwModelVersion: {e}")))?;
    if !v.exists {
        return Err(CliError::new(exit::GENERIC, format!("line {line} holds no V{version} in state")));
    }
    if v.evaluations.len() >= PALW_MODEL_EVALUATIONS_PER_VERSION_V1 {
        return Err(CliError::new(
            exit::GENERIC,
            format!("V{version} of line {line} already holds {} evaluations, the most it may", v.evaluations.len()),
        ));
    }
    if v.evaluations.iter().any(|e| bond_of(&e.by) == by) {
        return Err(CliError::new(
            exit::GENERIC,
            format!("bond {} already evaluated V{version} of line {line}; one per bond per version", bond_text(&by)),
        ));
    }
    let class = parse_hash(&row.class_id, "class id")?;
    assert_key_owns_bond(&nv, &key, class, &by, "evaluator").await?;
    let lines_own = [row.developer.as_ref().or(row.owner.as_ref()), row.maintainer.as_ref().or(row.owner.as_ref())]
        .into_iter()
        .flatten()
        .any(|o| bond_of(o) == by);
    let message =
        palw_model_evaluation_message_v1(network_domain(&nv), &line, version, &evaluator_id, score_permille, &report_hash, &by);
    let object = PalwConsensusObjectV2::ModelEvaluationPosted {
        line_id: line,
        version,
        evaluator_id,
        score_permille,
        report_hash,
        by,
        signature: sign(&key, &message),
    };
    if ctx.output != OutputFormat::Json {
        println!("evaluate V{version} of line {line}");
        println!("  evaluator      {evaluator_id}");
        println!("  score          {score_permille} ‰");
        println!("  report         {report_hash}");
        println!("  by             {}{}", bond_text(&by), if lines_own { " (the line's own word)" } else { " (a stranger's)" });
        println!("  declared       recorded and labelled; no rule reads a score");
    }
    let what = format!("ModelEvaluationPosted on V{version} of line {line}");
    let out = carry(ctx, &nv, &key, &object, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outpoint(i: u64, index: u32) -> RpcTransactionOutpoint {
        RpcTransactionOutpoint { transaction_id: Hash64::from_u64_word(i), index }
    }

    /// A role flag omitted keeps the row's bond, `owner` resets it, an outpoint names one — the
    /// three readings a `line-roles` call can mean, pinned so an omitted flag never silently
    /// resets a developer to the owner.
    #[test]
    fn a_role_flag_keeps_resets_or_names() {
        let current = outpoint(7, 1);
        assert_eq!(role_arg(None, Some(&current)).unwrap(), Some(bond_of(&current)));
        assert_eq!(role_arg(None, None).unwrap(), None);
        assert_eq!(role_arg(Some("owner"), Some(&current)).unwrap(), None);
        let named = format!("{}:3", Hash64::from_u64_word(9));
        assert_eq!(role_arg(Some(&named), None).unwrap(), Some(PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(9), 3))));
        assert!(role_arg(Some("nonsense"), None).is_err());
    }

    /// Decision 6: a row's `None` developer is the owner, and an unowned line has nobody.
    #[test]
    fn the_role_bond_defaults_to_the_owner_and_an_unowned_line_refuses() {
        let owner = outpoint(1, 0);
        let mut row = RpcPalwModelLine { line_id: "l".into(), owner: Some(owner), ..Default::default() };
        assert_eq!(role_bond(&row, "developer").unwrap(), bond_of(&owner));
        assert_eq!(role_bond(&row, "maintainer").unwrap(), bond_of(&owner));
        let dev = outpoint(2, 0);
        row.developer = Some(dev);
        assert_eq!(role_bond(&row, "developer").unwrap(), bond_of(&dev));
        assert_eq!(role_bond(&row, "owner").unwrap(), bond_of(&owner));
        row.owner = None;
        assert!(role_bond(&row, "owner").is_err(), "a genesis line has no owner to act");
    }
}
