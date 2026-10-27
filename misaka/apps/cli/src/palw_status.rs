//! **`misaka palw` status reads (lane UX-A)**: a claim's lifecycle, the model tier, the arming status and the bond budget, with every
//! typed code printed beside its plain reason.
//!
//! * `palw lifecycle <CLAIM>` — op 227 `getPalwFenceSchedule`, view kind `claim-lifecycle`: Final / SecurityCertified / held / void, the typed
//!   code and its reason, the Gate C draws and approvals (k of m), the deadline, the held reward. A kernel-route claim id works too.
//! * `palw model-tier [--class <C>]` — op 227 view `model-tier`: each class's tier (1.0 / 0.3), its source, `c` and `κ`.
//! * `palw arming` — op 227 view `arming`: each G14 route family armed now, Gate C's m/k, the pinned records and typed waivers.
//! * `palw budget --bond <B> | --claim <C> | --model [<M>] | --assignment <B>` — op 227 views `bond-budget`, `claim-budget`, `model-allocation`, `capital-assignment` (the readers design ops 250–253 named).
//! * `palw explain <CODE>` — offline: the plain reason for a refusal / outcome code (`G14_ROUTE_ABSENT`, `pending`, …).
//!
//! All of them are view kinds of op 227 (no new op numbers): a node built before them refuses the version-2 request.

use kaspa_consensus_core::palw_user_status_v1::{PALW_STATUS_CODE_TEXTS_V1, palw_status_code_text_v1};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::GetPalwFenceScheduleRequest;
use serde_json::Value;

use crate::node::Ctx;
use crate::palw_user::{emit, parse_view, read_error, text};
use crate::{CliError, CliResult, exit};

/// What `palw budget` asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BudgetTarget {
    Bond(String),
    Claim(String),
    /// `None`: every model row.
    Model(Option<String>),
    Assignment(String),
}

/// `--bond` / `--claim` / `--model` / `--assignment`, exactly one.
pub(crate) fn budget_target(
    bond: Option<String>,
    claim: Option<String>,
    model: Option<Option<String>>,
    assignment: Option<String>,
) -> Result<BudgetTarget, CliError> {
    match (bond, claim, model, assignment) {
        (Some(b), None, None, None) => Ok(BudgetTarget::Bond(b)),
        (None, Some(c), None, None) => Ok(BudgetTarget::Claim(c)),
        (None, None, Some(m), None) => Ok(BudgetTarget::Model(m)),
        (None, None, None, Some(a)) => Ok(BudgetTarget::Assignment(a)),
        _ => Err(CliError::new(exit::GENERIC, "name exactly one of --bond, --claim, --model or --assignment")),
    }
}

/// One view kind of op 227 (`getPalwFenceSchedule`'s multiplexer, version-2 request).
async fn status_view(ctx: &Ctx, view: &str, key: String) -> Result<(bool, bool, String), CliError> {
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_fence_schedule(GetPalwFenceScheduleRequest { view: view.into(), key }).await;
    let _ = reader.client.disconnect().await;
    let r = answer.map_err(|e| {
        read_error(
            &format!("getPalwFenceSchedule view {view}"),
            227,
            "a node that predates the view kinds refuses them; `misaka palw claim-detail` and `misaka palw fences` read part of it",
            &e,
        )
    })?;
    Ok((r.available, r.found, r.view_json))
}

/// The lifecycle document for `claim`, or `None` when the node predates the view kinds or does not hold the claim (claim-detail's add-on).
pub(crate) async fn lifecycle_doc(ctx: &Ctx, claim: &str) -> Option<Value> {
    let (available, found, json) = status_view(ctx, "claim-lifecycle", claim.to_string()).await.ok()?;
    parse_view("getPalwFenceSchedule", available, found, &json, "such claim").ok()
}

pub(crate) async fn lifecycle(ctx: &Ctx, claim: &str, json: bool) -> CliResult {
    let claim = crate::palw_derived::parse_claim_id(claim)?;
    let (available, found, doc) = status_view(ctx, "claim-lifecycle", claim.to_string()).await?;
    let doc = parse_view("getPalwFenceSchedule", available, found, &doc, "such claim (chain or kernel route)")?;
    emit(ctx, json, doc, render_lifecycle);
    Ok(())
}

/// `code — reason` (the reason from the node's document, else this build's table).
fn reason_line(r: &Value) -> String {
    let code = text(&r["code"]);
    let why = r["text"].as_str().or_else(|| palw_status_code_text_v1(&code)).unwrap_or("no explanation in this build");
    format!("    {code}: {why}\n")
}

pub(crate) fn render_lifecycle(doc: &Value) -> String {
    let mut out = format!("claim {} ({} route)\n", text(&doc["claimId"]), text(&doc["route"]));
    if doc["route"] == "chain" {
        out += &format!("  class {}  bond {}\n", text(&doc["classId"]), text(&doc["bond"]));
        out += &format!("  phase {} (at DAA {})\n", text(&doc["phase"]), text(&doc["phaseDaa"]));
        out += &format!("  Gate C governs {}\n", text(&doc["gateCGoverns"]));
    }
    out += &format!(
        "  Final {}  SecurityCertified {}  held {}  voided {}\n",
        text(&doc["final"]),
        text(&doc["securityCertified"]),
        text(&doc["held"]),
        text(&doc.get("voided").cloned().unwrap_or(Value::Bool(false)))
    );
    out += &format!("  certification {}\n", text(&doc["certificationCode"]));
    let c = &doc["certification"];
    if c.is_object() {
        out += &format!(
            "  Gate C draw: drawn {} at DAA {}  approvals {} of k={} (m={} drawn {})  negatives {}  veto {}\n",
            text(&c["drawn"]),
            text(&c["drawnDaa"]),
            text(&c["approvals"]),
            text(&c["k"]),
            text(&c["m"]),
            text(&c["draws"]),
            text(&c["negatives"]),
            text(&c["veto"])
        );
        out += &format!(
            "  deadline DAA {}  held reward {} sompi  outcome {} at DAA {}\n",
            text(&c["deadlineDaa"]),
            text(&c["heldRewardSompi"]),
            text(&c["outcome"]),
            text(&c["outcomeDaa"])
        );
    } else {
        out += "  Gate C draw: no certification row (not governed, or not yet Final)\n";
    }
    if let Some(h) = doc["heldReward"].as_object() {
        out += &format!("  held Final reward {} sompi since DAA {}\n", text(&h["amountSompi"]), text(&h["heldDaa"]));
    }
    let n = &doc["newAttemptOfClassNow"];
    if n["admitted"] == false {
        out += &format!("  a new attempt of this class now: refused {} — {}\n", text(&n["code"]), text(&n["why"]));
    }
    out += "  reasons:\n";
    for r in doc["reasons"].as_array().into_iter().flatten() {
        out += &reason_line(r);
    }
    out += &format!("  judged at DAA {}\n", text(&doc["nowDaa"]));
    out
}

pub(crate) async fn model_tier(ctx: &Ctx, class: Option<&str>, json: bool) -> CliResult {
    let key = match class {
        Some(c) => crate::palw_derived::parse_claim_id(c)?.to_string(),
        None => String::new(),
    };
    let (available, found, doc) = status_view(ctx, "model-tier", key).await?;
    let doc = parse_view("getPalwFenceSchedule", available, found, &doc, "such class")?;
    emit(ctx, json, doc, render_model_tier);
    Ok(())
}

pub(crate) fn render_model_tier(doc: &Value) -> String {
    let f = &doc["fence"];
    let mut out = if f.is_object() {
        format!(
            "model tier fence: from DAA {}  active {}  registered c = {}‰  κ = {}‰  catalog editions {}\n",
            text(&f["fromDaa"]),
            text(&f["active"]),
            text(&f["registeredCPermille"]),
            text(&f["kappaPermille"]),
            f["catalogEditions"].as_array().map_or(0, |a| a.len())
        )
    } else {
        "model tier fence: not scheduled (every model weighs 1.0)\n".to_string()
    };
    for c in doc["classes"].as_array().into_iter().flatten() {
        out += &format!("  {}  {} ({})  c = {}‰\n", text(&c["classId"]), text(&c["tier"]), text(&c["source"]), text(&c["cPermille"]));
    }
    if doc["truncated"] == true {
        out += &format!("  … {} classes in all\n", text(&doc["classCount"]));
    }
    out += "  the tier is an economic right only: every claim still needs SecurityCertified\n";
    out
}

pub(crate) async fn arming(ctx: &Ctx, json: bool) -> CliResult {
    let (available, found, doc) = status_view(ctx, "arming", String::new()).await?;
    let doc = parse_view("getPalwFenceSchedule", available, found, &doc, "arming status")?;
    emit(ctx, json, doc, render_arming);
    Ok(())
}

pub(crate) fn render_arming(doc: &Value) -> String {
    let mut out = format!("arming at DAA {}\n", text(&doc["nowDaa"]));
    let g = &doc["g14RouteRequired"];
    out += &format!("  palw_g14_route_required_v1: from DAA {}  governs {}\n", text(&g["fromDaa"]), text(&g["governs"]));
    for f in doc["routeFamilies"].as_array().into_iter().flatten() {
        out += &format!("  route {:<22} armed {:<5} framed {}\n", text(&f["family"]), text(&f["armed"]), text(&f["armedFramed"]));
    }
    let c = &doc["gateC"];
    out += &format!(
        "  Gate C: from DAA {}  in force {}  m/k {}/{}  veto {}\n",
        text(&c["fromDaa"]),
        text(&c["inForce"]),
        text(&c["m"]),
        text(&c["k"]),
        text(&c["veto"])
    );
    let r = &doc["records"];
    out += &format!(
        "  pinned records: g14 {}  kernel route {}  rfc0018 {}  model tier {}\n",
        text(&r["g14"]),
        text(&r["kernelRoute"]),
        text(&r["rfc0018"]),
        text(&r["modelTier"])
    );
    for w in doc["waivers"].as_array().into_iter().flatten() {
        out += &format!(
            "  waivers {} @ {}: {} (not run {}, user decision {})\n",
            text(&w["fence"]),
            text(&w["activationDaa"]),
            text(&w["waivers"]),
            text(&w["notRun"]),
            text(&w["userDecision"])
        );
    }
    out += "  a waiver records a requirement that was NOT run as pinned evidence (or a user decision); never a pass\n";
    out
}

pub(crate) async fn budget(ctx: &Ctx, target: BudgetTarget, json: bool) -> CliResult {
    let (view, key, what) = match target {
        BudgetTarget::Bond(b) => {
            crate::bond::parse_outpoint(&b)?;
            ("bond-budget", b, "bond at that outpoint")
        }
        BudgetTarget::Claim(c) => ("claim-budget", crate::palw_derived::parse_claim_id(&c)?.to_string(), "budget row for that claim"),
        BudgetTarget::Model(m) => {
            let key = match m {
                Some(m) => crate::palw_derived::parse_claim_id(&m)?.to_string(),
                None => String::new(),
            };
            ("model-allocation", key, "allocation row for that model")
        }
        BudgetTarget::Assignment(b) => {
            crate::bond::parse_outpoint(&b)?;
            ("capital-assignment", b, "bond at that outpoint")
        }
    };
    let (available, found, doc) = status_view(ctx, view, key).await?;
    let doc = parse_view("getPalwFenceSchedule", available, found, &doc, what)?;
    emit(ctx, json, doc, render_flat);
    Ok(())
}

fn flat_into(out: &mut String, indent: usize, key: &str, v: &Value) {
    let pad = " ".repeat(indent);
    match v {
        Value::Object(m) => {
            *out += &format!("{pad}{key}:\n");
            for (k, v) in m {
                flat_into(out, indent + 2, k, v);
            }
        }
        Value::Array(a) if a.iter().any(|x| x.is_object()) => {
            *out += &format!("{pad}{key}: {} rows\n", a.len());
            for (i, x) in a.iter().enumerate() {
                flat_into(out, indent + 2, &format!("[{i}]"), x);
            }
        }
        other => *out += &format!("{pad}{key}: {}\n", text(other)),
    }
}

/// A budget document as indented `key: value` lines ("fence dormant" shows as `fence: status: fence dormant`).
pub(crate) fn render_flat(doc: &Value) -> String {
    let mut out = format!("{}\n", text(&doc["kind"]));
    if let Value::Object(m) = doc {
        for (k, v) in m.iter().filter(|(k, _)| !matches!(k.as_str(), "kind" | "version" | "schema")) {
            flat_into(&mut out, 2, k, v);
        }
    }
    out
}

/// `palw explain [CODE]`: one code's reason, or the whole table. Offline.
pub(crate) fn explain(code: Option<&str>) -> CliResult {
    match code {
        Some(c) => match palw_status_code_text_v1(c) {
            Some(t) => println!("{c}: {t}"),
            None => return Err(CliError::new(exit::GENERIC, format!("no explanation for code '{c}' in this build (`misaka palw explain` lists them)"))),
        },
        None => {
            for (c, t) in PALW_STATUS_CODE_TEXTS_V1 {
                println!("{c}: {t}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn the_status_subcommands_parse() {
        let id = "ab".repeat(64);
        let bond = format!("{}:0", "cd".repeat(32));
        for args in [
            vec!["misaka", "palw", "lifecycle", id.as_str(), "--json"],
            vec!["misaka", "palw", "model-tier"],
            vec!["misaka", "palw", "model-tier", "--class", id.as_str()],
            vec!["misaka", "palw", "arming"],
            vec!["misaka", "palw", "budget", "--bond", bond.as_str()],
            vec!["misaka", "palw", "budget", "--claim", id.as_str()],
            vec!["misaka", "palw", "budget", "--model"],
            vec!["misaka", "palw", "budget", "--assignment", bond.as_str()],
            vec!["misaka", "palw", "explain", "G14_ROUTE_ABSENT"],
            vec!["misaka", "palw", "explain"],
        ] {
            assert!(crate::Cli::try_parse_from(args.clone()).is_ok(), "{args:?}");
        }
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "budget", "--bond", &bond, "--claim", &id]).is_err());
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "budget"]).is_err());
    }

    #[test]
    fn budget_needs_exactly_one_target() {
        assert_eq!(budget_target(Some("b".into()), None, None, None).unwrap(), BudgetTarget::Bond("b".into()));
        assert_eq!(budget_target(None, None, Some(None), None).unwrap(), BudgetTarget::Model(None));
        assert!(budget_target(None, None, None, None).is_err());
        assert!(budget_target(Some("b".into()), Some("c".into()), None, None).is_err());
    }

    #[test]
    fn the_lifecycle_view_prints_codes_with_reasons() {
        let doc = serde_json::json!({
            "version": 1, "kind": "palwClaimLifecycle", "route": "chain", "nowDaa": 16_700, "claimId": "c", "classId": "k",
            "bond": "b:0", "phase": "final", "phaseDaa": 16_650, "final": true, "voided": false, "gateCGoverns": true,
            "securityCertified": false, "certificationCode": "pending", "held": true,
            "certification": { "drawn": true, "drawnDaa": 16_660, "deadlineDaa": 17_000, "draws": 75, "approvals": 12, "negatives": 0,
                               "k": 41, "m": 75, "veto": 0, "heldRewardSompi": 5, "outcome": null, "outcomeDaa": null },
            "newAttemptOfClassNow": { "admitted": false, "code": "G14_ROUTE_ABSENT", "why": "no route" },
            "reasons": [{ "code": "pending", "text": null }],
        });
        let s = render_lifecycle(&doc);
        assert!(s.contains("approvals 12 of k=41 (m=75"), "{s}");
        assert!(s.contains("held true") && s.contains("deadline DAA 17000"));
        assert!(s.contains("refused G14_ROUTE_ABSENT"));
        assert!(s.contains("pending: Final reached"), "the CLI's own table fills a missing reason: {s}");
    }

    #[test]
    fn the_flat_view_shows_a_dormant_fence() {
        let doc = serde_json::json!({ "version": 1, "kind": "palwBondBudget", "fence": { "status": "fence dormant" }, "caps": null });
        let s = render_flat(&doc);
        assert!(s.contains("status: fence dormant") && s.contains("caps: -"), "{s}");
        assert!(explain(Some("G14_ROUTE_ABSENT")).is_ok() && explain(Some("NO_SUCH")).is_err());
    }
}
