//! **`misaka palw` user surface (tooling lane): read a claim, a court, a bond and the fences; build, show, sign and submit objects.**
//!
//! Reads (one node RPC each, read-only, no rule reads them):
//!
//! * `palw claim-detail <CLAIM>` — op 225 `getPalwClaimDetail`: phase and the date it ends by itself, Gate C certification,
//!   accusation pending, receipt pause, court clocks (notice 3,120 / hard 10,000 / per-turn), dispute record, DA and court sessions.
//! * `palw court-state --claim <C> | --session <S>` — op 226 `getPalwCourtState`.
//! * `palw bond-detail <TXID:INDEX>` — op 228 `getPalwBondDetail`: collateral, exposure, live locks, claims, reservations.
//! * `palw fences` — op 227 `getPalwFenceSchedule`: every PALW fence, its height, whether armed, the release lists.
//! * `palw status` — the fence summary and the not-adopted register in one screen (optionally one bond).
//!
//! Offline: `palw not-adopted` (the typed-missing values this build's consensus holds), `palw object-show` (decode a borsh
//! `PalwConsensusObjectV2`: tag, variant, owning fence, whether it may ride, whether the fence is in force on `--network` at a DAA).
//!
//! Builders (`palw build …`) write a SIGNED object file and nothing else; filing it is `palw submit-object --object <f>` (a dry-run
//! preview unless `--submit` / `--yes`). A builder whose interaction needs a value consensus holds typed-missing (a fee schedule, a
//! slash amount) refuses with exit 10 and names the value — it never guesses one.
//!
//! **Old nodes.** A node built before ops 225–228 drops the WebSocket on them (wRPC has no unknown-op reply): the read reports exit 36
//! (`COMPONENT_DOWN`) and names the older read that answers part of the question.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_DISPUTE_RESERVATION_VERSION_V1, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, PalwDisputeReservationV1,
    palw_dispute_released_object_v1, palw_dispute_reserved_object_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_user_views_v1::{PalwTypedMissingV1, palw_typed_missing_json_v1, palw_typed_missing_values_v1};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{
    GetPalwBondDetailRequest, GetPalwClaimDetailRequest, GetPalwCourtStateRequest, GetPalwFenceScheduleRequest,
    GetPalwLegacyDisputeRequest,
};
use kaspa_wrpc_client::error::rpc_error_is_connection_loss;
use serde_json::Value;

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};

/// The schema tag every `--json` document of this module carries beside the node's own `version`.
pub(crate) const PALW_USER_CLI_SCHEMA: &str = "misaka.palw.user-view.v1";

/// `palw build …`: the object builders. Each writes one signed borsh `PalwConsensusObjectV2` to `--out`; nothing is submitted.
#[derive(clap::Subcommand, Debug)]
pub(crate) enum BuildCmd {
    /// **Reserve a claim for dispute (tag 154, `DisputeReservedV1`)** — a public filer's first move (RFC-0014 LG14-A). The claim's
    /// roots are read from the node (`getPalwLegacyDispute`, op 204) unless both `--execution-root` and `--trace-root` are given.
    /// Signed with the reserver bond's key under the chain's own domain. Refused while `palw_legacy_public_filer_v1` is not armed at
    /// the node's DAA, unless `--allow-dormant` (a drill writing an object for a later height).
    DisputeReserve {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
        /// The reserver's bond outpoint, `<txid>:<index>` (its registered key is the one --key-file names).
        #[arg(long)]
        bond: String,
        /// The claim's execution root, 128 hex (read from the node when omitted).
        #[arg(long, requires = "trace_root")]
        execution_root: Option<String>,
        /// The claim's trace root, 128 hex (read from the node when omitted).
        #[arg(long, requires = "execution_root")]
        trace_root: Option<String>,
        /// Write the object even though the fence is not armed at the node's DAA.
        #[arg(long)]
        allow_dormant: bool,
        /// Where to write the signed object.
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// **Release a dispute reservation (tag 155, `DisputeReleasedV1`)** — the reserver ends its own reservation.
    DisputeRelease {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
        /// The reserver's bond outpoint, `<txid>:<index>`.
        #[arg(long)]
        bond: String,
        /// Write the object even though the fence is not armed at the node's DAA.
        #[arg(long)]
        allow_dormant: bool,
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Funded original (TFO1 envelope with payer consent). Refused: the funded-original schedule is not adopted.
    FundedOriginal {
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
    },
    /// Court paid ingress (outsider lane session fee). Refused: the ingress schedule is not adopted.
    CourtIngress {
        #[arg(long)]
        claim: String,
    },
    /// Material intake parts (tag 255). Refused: the intake schedule (sompi per byte, TTL, cover) is not adopted.
    MaterialIntake {
        #[arg(long)]
        claim: String,
    },
    /// D2 readiness demand (tag 231). Refused: the D2 disclosure fee schedule is not adopted.
    D2Demand {
        /// 128-hex class id.
        #[arg(long)]
        class: String,
    },
    /// Position terminal close / lookahead (tags 201–204). Refused: the witness budget and frontier bound are not adopted.
    PositionClose {
        #[arg(long)]
        claim: String,
    },
}

// ---- reads -----------------------------------------------------------------------------------------------------------------------

pub(crate) fn read_error(op_name: &str, op: u16, fallback: &str, e: &kaspa_rpc_core::RpcError) -> CliError {
    if rpc_error_is_connection_loss(e) {
        CliError::new(
            exit::COMPONENT_DOWN,
            format!("{op_name}: {e} — the node closed the connection on op {op}, so it predates the PALW user views; {fallback}"),
        )
    } else {
        CliError::new(exit::GENERIC, format!("{op_name}: {e}"))
    }
}

/// The node answered: parse its document, refuse a version this CLI does not know.
pub(crate) fn parse_view(op_name: &str, available: bool, found: bool, json: &str, what: &str) -> Result<Value, CliError> {
    if !available {
        return Err(CliError::new(
            exit::GENERIC,
            format!("{op_name}: the node holds no PALW ConsensusV2 state (wrong network, or not synced)"),
        ));
    }
    if !found {
        return Err(CliError::new(
            exit::GENERIC,
            format!("{op_name}: the chain holds no {what} (retired, never accepted, or misspelt)"),
        ));
    }
    let doc: Value = serde_json::from_str(json).map_err(|e| CliError::new(exit::GENERIC, format!("{op_name}: bad document: {e}")))?;
    match doc["version"].as_u64() {
        Some(1) => Ok(doc),
        other => Err(CliError::new(exit::GENERIC, format!("{op_name}: document version {other:?} is newer than this CLI (1)"))),
    }
}

pub(crate) fn emit(ctx: &Ctx, json: bool, mut doc: Value, human: impl FnOnce(&Value) -> String) {
    if json || ctx.output == OutputFormat::Json {
        doc["schema"] = PALW_USER_CLI_SCHEMA.into();
        println!("{}", serde_json::to_string_pretty(&doc).expect("a JSON value serializes"));
    } else {
        print!("{}", human(&doc));
    }
}

pub(crate) fn text(v: &Value) -> String {
    match v {
        Value::Null => "-".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

pub(crate) async fn claim_detail(ctx: &Ctx, claim: &str, json: bool) -> CliResult {
    let claim = crate::palw_derived::parse_claim_id(claim)?;
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_claim_detail(GetPalwClaimDetailRequest { claim_hex: claim.to_string() }).await;
    let _ = reader.client.disconnect().await;
    let r = answer
        .map_err(|e| read_error("getPalwClaimDetail", 225, "`misaka palw claim` and `misaka bond status` read part of it", &e))?;
    let mut doc = parse_view("getPalwClaimDetail", r.available, r.found, &r.view_json, "such claim")?;
    // Lane UX-A: the lifecycle view (op 227 view kind) adds SecurityCertified / held, the Gate C draw and the typed codes with their reasons.
    // Read after op 225 on its own connection, so a node that predates the view kinds still answers the detail (`lifecycle: null`).
    doc["lifecycle"] = crate::palw_status::lifecycle_doc(ctx, &claim.to_string()).await.unwrap_or(Value::Null);
    emit(ctx, json, doc, render_claim_detail);
    Ok(())
}

pub(crate) fn render_claim_detail(doc: &Value) -> String {
    let c = &doc["claim"];
    let mut out = String::new();
    out += &format!("claim {}\n", text(&c["claimId"]));
    out += &format!("  class {}  executor {}\n", text(&c["classId"]), text(&c["executorBond"]));
    out += &format!(
        "  phase {} (at DAA {}){}  — ends by itself at DAA {}\n",
        text(&c["phase"]),
        text(&c["phaseDaa"]),
        c["voidReason"].as_str().map(|r| format!(", void reason {r}")).unwrap_or_default(),
        text(&c["phaseDeadlineDaa"])
    );
    out += &format!(
        "  certification (Gate C) {}  governs {}  — Final is not SecurityCertified\n",
        text(&c["securityCertification"]),
        text(&doc["gateCGoverns"])
    );
    out += &format!("  claim-local settlement {}\n", text(&c["claimLocalSettlement"]));
    out += &format!(
        "  reserved {} sompi  committed {} sompi  escrow {}  payout pending {}\n",
        text(&c["reservedSompi"]),
        text(&c["committedSompi"]),
        text(&c["escrowedRewardSompi"]),
        text(&c["payoutPendingSompi"])
    );
    out += &format!("  accusation pending {}\n", text(&doc["accusationPending"]));
    let p = &doc["receiptPause"];
    out += &format!(
        "  receipt pause: applies {}  open {}  since {}  paused total {} DAA\n",
        text(&p["applies"]),
        text(&p["open"]),
        text(&p["openSinceDaa"]),
        text(&p["pausedTotalDaa"])
    );
    let k = &doc["courtClocks"];
    out += &format!(
        "  court clocks: v2 governs {}  notice until {}  hard {}  extended hard {}  per-turn {} DAA  window_court {}\n",
        text(&k["clockV2Governs"]),
        text(&k["noticeUntilDaa"]),
        text(&k["hardDeadlineDaa"]),
        text(&k["extendedHardDeadlineDaa"]),
        text(&k["turnDeadlineDaa"]),
        text(&k["windowCourt"])
    );
    let d = &doc["dispute"];
    if d["record"] == Value::Bool(true) {
        out += &format!(
            "  dispute record: holds {}  hard {}  live reservations {}\n",
            text(&d["holds"]),
            text(&d["hardDeadlineDaa"]),
            d["live"].as_array().map_or(0, |a| a.len())
        );
    } else {
        out += "  dispute record: none\n";
    }
    for s in doc["daSessions"].as_array().into_iter().flatten() {
        out += &format!(
            "  DA session: accuser {}  stage {}  deadline {}{}\n",
            text(&s["accuser"]),
            text(&s["stage"]),
            text(&s["deadlineDaa"]),
            if s["deadlinePassed"] == Value::Bool(true) { " (passed)" } else { "" }
        );
    }
    for s in doc["courtSessions"].as_array().into_iter().flatten() {
        out += &render_session_line(s);
    }
    if doc["lifecycle"].is_object() {
        for line in crate::palw_status::render_lifecycle(&doc["lifecycle"]).lines().skip(1) {
            out += line;
            out += "\n";
        }
    } else {
        out += "  lifecycle: unavailable (the node predates op 227's view kinds)\n";
    }
    out += &format!("  judged at DAA {} (tip {})\n", text(&doc["nowDaa"]), text(&doc["tipDaa"]));
    out
}

fn render_session_line(s: &Value) -> String {
    let l = &s["ladder"];
    format!(
        "  court session {}: challenger {}  turn {} (deadline {})  round {}  backstop {}{}\n",
        text(&s["sessionId"]),
        text(&s["challengerBond"]),
        text(&l["turn"]),
        text(&l["turnDeadlineDaa"]),
        text(&l["round"]),
        text(&s["backstopDaa"]),
        if s["backstopPassed"] == Value::Bool(true) { " (passed)" } else { "" }
    )
}

pub(crate) async fn court_state(ctx: &Ctx, claim: Option<&str>, session: Option<&str>, json: bool) -> CliResult {
    let request = court_state_request(claim, session)?;
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_court_state(request).await;
    let _ = reader.client.disconnect().await;
    let r = answer.map_err(|e| read_error("getPalwCourtState", 226, "`misaka palw claim` lists open courts per claim", &e))?;
    let doc = parse_view("getPalwCourtState", r.available, r.found, &r.view_json, "such claim or court session")?;
    emit(ctx, json, doc, |doc| {
        let mut out = format!(
            "court around claim {} (held {}, phase {}): accusation pending {}, open courts {}\n",
            text(&doc["claimId"]),
            text(&doc["claimHeld"]),
            text(&doc["phase"]),
            text(&doc["accusationPending"]),
            text(&doc["openCourts"])
        );
        for s in doc["courtSessions"].as_array().into_iter().flatten() {
            out += &render_session_line(s);
        }
        let k = &doc["courtClocks"];
        out += &format!("  hard deadline {}  notice until {}\n", text(&k["hardDeadlineDaa"]), text(&k["noticeUntilDaa"]));
        out
    });
    Ok(())
}

/// Exactly one of `--claim` / `--session`, each a 128-hex id.
pub(crate) fn court_state_request(claim: Option<&str>, session: Option<&str>) -> Result<GetPalwCourtStateRequest, CliError> {
    let id = |s: &str| crate::palw_derived::parse_claim_id(s).map(|h| h.to_string());
    match (claim, session) {
        (Some(c), None) => Ok(GetPalwCourtStateRequest { claim_hex: id(c)?, session_hex: String::new() }),
        (None, Some(s)) => Ok(GetPalwCourtStateRequest { claim_hex: String::new(), session_hex: id(s)? }),
        _ => Err(CliError::new(exit::GENERIC, "name exactly one of --claim or --session")),
    }
}

pub(crate) async fn bond_detail(ctx: &Ctx, bond: &str, json: bool) -> CliResult {
    crate::bond::parse_outpoint(bond)?;
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_bond_detail(GetPalwBondDetailRequest { bond: bond.to_string() }).await;
    let _ = reader.client.disconnect().await;
    let r = answer.map_err(|e| read_error("getPalwBondDetail", 228, "`misaka bond status` reads part of it", &e))?;
    let doc = parse_view("getPalwBondDetail", r.available, r.found, &r.view_json, "bond at that outpoint")?;
    emit(ctx, json, doc, render_bond_detail);
    Ok(())
}

pub(crate) fn render_bond_detail(doc: &Value) -> String {
    let l = &doc["locks"];
    let mut out =
        format!("bond {}  ({}, registered at DAA {})\n", text(&doc["bond"]), text(&doc["status"]), text(&doc["registeredDaa"]));
    out += &format!("  collateral {} sompi  slashed {} sompi\n", text(&doc["collateralSompi"]), text(&doc["slashedSompi"]));
    out += &format!(
        "  reserved exposure {} sompi (claims)  {} sompi (registrations)\n",
        text(&doc["reservedExposureSompi"]),
        text(&doc["registrationExposureSompi"])
    );
    out += &format!(
        "  live locks {} ({} sompi), DAA clock release {}, retire refused while locked {}\n",
        text(&l["liveCount"]),
        text(&l["liveLockedSompi"]),
        text(&l["daaClockRelease"]),
        text(&l["retireRefusedWhileLocked"])
    );
    for (label, key) in [("as executor", "claimsAsExecutor"), ("as seat", "claimsAsSeat")] {
        let rows = doc[key]["rows"].as_array().cloned().unwrap_or_default();
        out +=
            &format!("  live claims {label}: {}{}\n", rows.len(), if doc[key]["truncated"] == Value::Bool(true) { "+" } else { "" });
        for r in rows {
            out += &format!(
                "    {}  {}  ends {}  cert {}\n",
                text(&r["claimId"]),
                text(&r["phase"]),
                text(&r["phaseDeadlineDaa"]),
                text(&r["securityCertification"])
            );
        }
    }
    let reservations = doc["disputeReservations"].as_array().map_or(0, |a| a.len());
    let prosecuting = doc["prosecutingSessions"].as_array().map_or(0, |a| a.len());
    out += &format!("  dispute reservations held {reservations}  court sessions prosecuted {prosecuting}\n");
    out
}

async fn fence_schedule(ctx: &Ctx) -> Result<(u64, Value), CliError> {
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_fence_schedule(GetPalwFenceScheduleRequest::default()).await;
    let _ = reader.client.disconnect().await;
    let r = answer.map_err(|e| {
        read_error(
            "getPalwFenceSchedule",
            227,
            "`misaka mining status` reads the node's boot fences (getPalwNodeStatus) without names",
            &e,
        )
    })?;
    Ok((r.now_daa, parse_view("getPalwFenceSchedule", r.available, r.found, &r.view_json, "fence schedule")?))
}

/// `palw fences`: `armed_only` keeps the armed rows; `list` keeps one release list's members.
pub(crate) async fn fences(ctx: &Ctx, armed_only: bool, list: Option<&str>, json: bool) -> CliResult {
    let (_, doc) = fence_schedule(ctx).await?;
    let doc = filter_fences(doc, armed_only, list)?;
    emit(ctx, json, doc, render_fences);
    Ok(())
}

pub(crate) fn filter_fences(mut doc: Value, armed_only: bool, list: Option<&str>) -> Result<Value, CliError> {
    if let Some(list) = list {
        let known: Vec<String> =
            doc["lists"].as_array().into_iter().flatten().filter_map(|l| l["list"].as_str().map(String::from)).collect();
        if !known.iter().any(|k| k == list) {
            return Err(CliError::new(exit::GENERIC, format!("no release list '{list}' (known: {})", known.join(", "))));
        }
    }
    let keep = |f: &Value| {
        (!armed_only || f["armed"] == Value::Bool(true))
            && list.is_none_or(|l| f["lists"].as_array().is_some_and(|ls| ls.iter().any(|x| x == l)))
    };
    let rows: Vec<Value> = doc["fences"].as_array().cloned().unwrap_or_default().into_iter().filter(keep).collect();
    doc["fences"] = Value::Array(rows);
    Ok(doc)
}

pub(crate) fn render_fences(doc: &Value) -> String {
    let mut out = format!(
        "{} PALW fences on {} at DAA {}: {} armed\n",
        text(&doc["fenceCount"]),
        text(&doc["network"]),
        text(&doc["nowDaa"]),
        text(&doc["armedCount"])
    );
    let fa = &doc["fullActivation"];
    out += &format!(
        "full activation: adopted height {} (target {}, not scheduled until adopted)\n",
        if fa["adoptedDaa"].is_null() { "not adopted".to_string() } else { text(&fa["adoptedDaa"]) },
        text(&fa["targetDaa"])
    );
    for l in doc["lists"].as_array().into_iter().flatten() {
        out += &format!(
            "  list {:<17} {:<12} {}/{} armed\n",
            text(&l["list"]),
            if l["adoptedDaa"].is_null() { "not adopted".to_string() } else { format!("DAA {}", text(&l["adoptedDaa"])) },
            text(&l["armedEntries"]),
            text(&l["entries"])
        );
    }
    out += &format!("  {:<52} {:>12}  {:<6} lists\n", "fence", "height", "armed");
    for f in doc["fences"].as_array().into_iter().flatten() {
        let lists: Vec<String> = f["lists"].as_array().into_iter().flatten().map(text).collect();
        out += &format!(
            "  {:<52} {:>12}  {:<6} {}\n",
            text(&f["name"]),
            if f["activationDaa"].is_null() { "-".to_string() } else { text(&f["activationDaa"]) },
            if f["armed"] == Value::Bool(true) { "yes" } else { "no" },
            lists.join(",")
        );
    }
    out
}

/// `palw status`: the fence summary and the not-adopted register; with `--bond`, that bond's standing too.
pub(crate) async fn status(ctx: &Ctx, bond: Option<&str>, json: bool) -> CliResult {
    let (now_daa, fences) = fence_schedule(ctx).await?;
    let bond_doc = match bond {
        Some(b) => {
            crate::bond::parse_outpoint(b)?;
            let reader = crate::palw_derived::connect(ctx).await?;
            let answer = reader.client.get_palw_bond_detail(GetPalwBondDetailRequest { bond: b.to_string() }).await;
            let _ = reader.client.disconnect().await;
            let r = answer.map_err(|e| read_error("getPalwBondDetail", 228, "`misaka bond status` reads part of it", &e))?;
            Some(parse_view("getPalwBondDetail", r.available, r.found, &r.view_json, "bond at that outpoint")?)
        }
        None => None,
    };
    let doc = serde_json::json!({
        "version": 1,
        "kind": "palwUserStatus",
        "nowDaa": now_daa,
        "network": fences["network"],
        "fenceCount": fences["fenceCount"],
        "armedCount": fences["armedCount"],
        "fullActivation": fences["fullActivation"],
        "lists": fences["lists"],
        "notAdopted": palw_typed_missing_json_v1()["values"],
        "bond": bond_doc.clone().unwrap_or(Value::Null),
    });
    emit(ctx, json, doc, |doc| {
        let mut out = format!(
            "PALW on {} at DAA {}: {} of {} fences armed\n",
            text(&doc["network"]),
            text(&doc["nowDaa"]),
            text(&doc["armedCount"]),
            text(&doc["fenceCount"])
        );
        let fa = &doc["fullActivation"];
        out += &format!(
            "full activation: {}\n",
            if fa["adoptedDaa"].is_null() { "not adopted (dormant)".to_string() } else { format!("DAA {}", text(&fa["adoptedDaa"])) }
        );
        out += &render_not_adopted(&palw_typed_missing_values_v1());
        if let Some(b) = &bond_doc {
            out += &render_bond_detail(b);
        }
        out
    });
    Ok(())
}

pub(crate) fn render_not_adopted(values: &[PalwTypedMissingV1]) -> String {
    let mut out = String::from("values consensus holds typed-missing (builders that need them refuse):\n");
    for v in values {
        out += &format!("  {:<32} {:<12} needed by {}\n", v.name, if v.adopted { "adopted" } else { "not adopted" }, v.needed_by);
    }
    out
}

pub(crate) fn not_adopted(ctx: &Ctx, json: bool) -> CliResult {
    emit(ctx, json, palw_typed_missing_json_v1(), |_| render_not_adopted(&palw_typed_missing_values_v1()));
    Ok(())
}

// ---- object-show (offline) -------------------------------------------------------------------------------------------------------

/// What `palw object-show` reports about one object file.
pub(crate) fn object_summary(
    bytes: &[u8],
    params: Option<&kaspa_consensus_core::config::params::Params>,
    at_daa: u64,
) -> Result<Value, CliError> {
    let object: PalwConsensusObjectV2 =
        borsh::from_slice(bytes).map_err(|e| CliError::new(exit::GENERIC, format!("not a borsh PalwConsensusObjectV2: {e}")))?;
    let tag = bytes.first().copied().unwrap_or_default();
    let debug = format!("{object:?}");
    let variant: String = debug.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    let owner = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_kind_owner_v1(&object);
    let ride = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object);
    let in_force = params.map(|p| p.palw_lifecycle_kind_fences_v1().kind_in_force_at(&object, at_daa));
    Ok(serde_json::json!({
        "version": 1,
        "kind": "palwObjectSummary",
        "tag": tag,
        "variant": variant,
        "bytes": bytes.len(),
        "owningFence": format!("{owner:?}"),
        "mayRide": ride.is_ok(),
        "rideRefusal": ride.err(),
        "atDaa": at_daa,
        "kindInForce": in_force,
        "note": "stateless checks only; the fold judges signatures, bonds and deadlines at the block that carries it",
    }))
}

pub(crate) fn object_show(ctx: &Ctx, object: &std::path::Path, at_daa: Option<u64>, json: bool) -> CliResult {
    let bytes = std::fs::read(object).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", object.display())))?;
    let net = <kaspa_consensus_core::network::NetworkId as std::str::FromStr>::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let (params, _) = crate::wallet::chain_params(ctx, net)?;
    let doc = object_summary(&bytes, Some(&params), at_daa.unwrap_or(u64::MAX - 1))?;
    emit(ctx, json, doc, |d| {
        format!(
            "object: tag {} {} ({} bytes)\n  owning fence {}\n  may ride a carrier: {}{}\n  kind in force on {} at DAA {}: {}\n  {}\n",
            text(&d["tag"]),
            text(&d["variant"]),
            text(&d["bytes"]),
            text(&d["owningFence"]),
            text(&d["mayRide"]),
            d["rideRefusal"].as_str().map(|r| format!(" ({r})")).unwrap_or_default(),
            ctx.network,
            text(&d["atDaa"]),
            text(&d["kindInForce"]),
            text(&d["note"])
        )
    });
    Ok(())
}

// ---- builders ----------------------------------------------------------------------------------------------------------------------

/// **Refuse a builder whose interaction needs a typed-missing value**: exit 10, naming the value and where consensus reads it.
pub(crate) fn refuse_unless_adopted(name: &str) -> Result<(), CliError> {
    let values = palw_typed_missing_values_v1();
    let value = values.iter().find(|v| v.name == name).expect("a builder names a value of the register");
    if value.adopted {
        return Err(CliError::new(
            exit::UNSAFE_REFUSED,
            format!("{} is adopted in this build, but this CLI has no builder for it yet — use the SDK", value.name),
        ));
    }
    Err(CliError::new(
        exit::UNSAFE_REFUSED,
        format!(
            "refused: {} needs the {} ({}), which consensus holds typed-missing (not adopted) — the chain refuses every such object, \
             so nothing is built or guessed. `misaka palw not-adopted` lists every such value.",
            value.needed_by, value.name, value.accessor
        ),
    ))
}

/// Tag 154 signed with `key` under `domain` — the consensus builder, the consensus message, the consensus context.
pub(crate) fn dispute_reserved_object(
    domain: Hash64,
    reservation: PalwDisputeReservationV1,
    key: &kaspa_pq_validator_core::ValidatorKey,
) -> PalwConsensusObjectV2 {
    palw_dispute_reserved_object_v1(domain, reservation, |m| key.sign_with_context(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).to_vec())
}

/// Tag 155 signed with `key` under `domain`.
pub(crate) fn dispute_released_object(
    domain: Hash64,
    claim: Hash64,
    reserver: PalwBondKeyV2,
    key: &kaspa_pq_validator_core::ValidatorKey,
) -> PalwConsensusObjectV2 {
    palw_dispute_released_object_v1(domain, claim, reserver, |m| {
        key.sign_with_context(m, PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1).to_vec()
    })
}

fn parse_hash(what: &str, s: &str) -> Result<Hash64, CliError> {
    s.trim().parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("{what} '{s}' is not 128 hex digits")))
}

/// The node, the bond's key check (a signature under another key is refused on chain and the fee is gone) and the fence check.
async fn signer_preflight(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    bond: &str,
    allow_dormant: bool,
) -> Result<(crate::wallet::NodeView, kaspa_pq_validator_core::ValidatorKey, PalwBondKeyV2), CliError> {
    let reserver = PalwBondKeyV2(crate::bond::parse_outpoint(bond)?);
    let key = ks.load_key()?;
    let nv = crate::wallet::connect(ctx).await?;
    let lookup_class = crate::bond::bond_lookup_class(&nv)?;
    let facts = nv
        .client
        .get_palw_producer_facts(lookup_class.clone(), reserver.0.transaction_id.to_string(), reserver.0.index, true)
        .await
        .map_err(|e| CliError::connection(format!("cannot read the bond's facts from the node: {e}")))?;
    crate::bond::bond_facts_answered(facts.available, &lookup_class, bond)?;
    if !facts.bond_known {
        return Err(CliError::new(exit::GENERIC, format!("the chain knows no bond at {bond}")));
    }
    let ours: String = key.public_key().iter().map(|b| format!("{b:02x}")).collect();
    if facts.bond_registered_pubkey.to_ascii_lowercase() != ours {
        return Err(CliError::new(
            exit::GENERIC,
            format!("the bond at {bond} registered a different key than the one loaded — the chain refuses this signature"),
        ));
    }
    let armed = nv.params.palw_legacy_public_filer_v1.is_some_and(|a| a.is_active(nv.virtual_daa));
    if !armed && !allow_dormant {
        return Err(CliError::new(
            exit::UNSAFE_REFUSED,
            format!(
                "refused: palw_legacy_public_filer_v1 is not armed on {} at DAA {} — the chain drops a dispute object below it \
                 (`misaka palw fences` shows the schedule; --allow-dormant writes it anyway for a drill)",
                nv.params.net, nv.virtual_daa
            ),
        ));
    }
    Ok((nv, key, reserver))
}

fn write_object(out: &std::path::Path, object: &PalwConsensusObjectV2) -> Result<(), CliError> {
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(object)
        .map_err(|why| CliError::new(exit::GENERIC, format!("the object cannot ride a carrier: {why}")))?;
    let bytes = borsh::to_vec(object).map_err(|e| CliError::new(exit::GENERIC, format!("cannot serialize the object: {e}")))?;
    std::fs::write(out, &bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", out.display())))?;
    println!("show it:   misaka palw object-show --object {}", out.display());
    println!(
        "file it:   misaka palw submit-object --object {} --key-file <same key>   (dry-run; add --submit to broadcast)",
        out.display()
    );
    Ok(())
}

pub(crate) async fn build(ctx: &Ctx, cmd: BuildCmd) -> CliResult {
    match cmd {
        BuildCmd::DisputeReserve { key, claim, bond, execution_root, trace_root, allow_dormant, out } => {
            let claim = parse_hash("--claim", &claim)?;
            let (nv, signer, reserver) = signer_preflight(ctx, &key.source(), &bond, allow_dormant).await?;
            let (execution_root, trace_root) = match (execution_root, trace_root) {
                (Some(e), Some(t)) => (parse_hash("--execution-root", &e)?, parse_hash("--trace-root", &t)?),
                _ => {
                    let view = nv
                        .client
                        .get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: claim.to_string() })
                        .await
                        .map_err(|e| read_error("getPalwLegacyDispute", 204, "pass --execution-root and --trace-root", &e))?;
                    if !view.available || view.json.is_empty() {
                        return Err(CliError::new(exit::GENERIC, format!("the chain holds no claim {claim} to reserve")));
                    }
                    let doc: Value = serde_json::from_str(&view.json)
                        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwLegacyDispute: bad document: {e}")))?;
                    (parse_hash("executionRoot", &text(&doc["executionRoot"]))?, parse_hash("traceRoot", &text(&doc["traceRoot"]))?)
                }
            };
            let domain = crate::bond::network_domain(&nv);
            let reservation =
                PalwDisputeReservationV1 { version: PALW_DISPUTE_RESERVATION_VERSION_V1, claim, execution_root, trace_root, reserver };
            let object = dispute_reserved_object(domain, reservation, &signer);
            println!("DisputeReservedV1 (tag 154): claim {claim}, reserver {bond}");
            write_object(&out, &object)
        }
        BuildCmd::DisputeRelease { key, claim, bond, allow_dormant, out } => {
            let claim = parse_hash("--claim", &claim)?;
            let (nv, signer, reserver) = signer_preflight(ctx, &key.source(), &bond, allow_dormant).await?;
            let object = dispute_released_object(crate::bond::network_domain(&nv), claim, reserver, &signer);
            println!("DisputeReleasedV1 (tag 155): claim {claim}, reserver {bond}");
            write_object(&out, &object)
        }
        BuildCmd::FundedOriginal { claim } => {
            parse_hash("--claim", &claim)?;
            refuse_unless_adopted("funded original schedule")
        }
        BuildCmd::CourtIngress { claim } => {
            parse_hash("--claim", &claim)?;
            refuse_unless_adopted("court paid-ingress schedule")
        }
        BuildCmd::MaterialIntake { claim } => {
            parse_hash("--claim", &claim)?;
            refuse_unless_adopted("material intake schedule")
        }
        BuildCmd::D2Demand { class } => {
            parse_hash("--class", &class)?;
            refuse_unless_adopted("D2 disclosure fee schedule")
        }
        BuildCmd::PositionClose { claim } => {
            parse_hash("--claim", &claim)?;
            refuse_unless_adopted("TIR position witness budget")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use kaspa_consensus_core::palw_legacy_public_filer_v1::palw_legacy_dispute_message_v1;

    fn claim() -> Hash64 {
        Hash64::from_bytes([7u8; 64])
    }

    #[test]
    fn the_read_subcommands_parse() {
        let id = "ab".repeat(64);
        let cli = crate::Cli::try_parse_from(["misaka", "palw", "claim-detail", &id, "--json"]).unwrap();
        assert!(format!("{:?}", cli.command).contains("ClaimDetail"));
        let cli = crate::Cli::try_parse_from(["misaka", "palw", "court-state", "--session", &id]).unwrap();
        assert!(format!("{:?}", cli.command).contains("CourtState"));
        let bond = format!("{}:0", "cd".repeat(64));
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "bond-detail", &bond]).is_ok());
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "fences", "--armed-only", "--list", "fullActivation"]).is_ok());
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "status", "--bond", &bond]).is_ok());
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "not-adopted", "--json"]).is_ok());
        // `--submit` is the explicit broadcast flag on submit-object (an alias of `--yes`).
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "submit-object", "--object", "o.borsh", "--submit"]).is_ok());
    }

    #[test]
    fn the_builders_parse_and_roots_come_in_pairs() {
        let id = "ab".repeat(64);
        let bond = format!("{}:1", "cd".repeat(64));
        let base = ["misaka", "palw", "build", "dispute-reserve", "--claim", &id, "--bond", &bond, "--out", "r.borsh"];
        assert!(crate::Cli::try_parse_from(base).is_ok());
        let mut one_root = base.to_vec();
        one_root.extend(["--execution-root", &id]);
        assert!(crate::Cli::try_parse_from(one_root).is_err(), "a single root is refused: both or neither");
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "build", "funded-original", "--claim", &id]).is_ok());
    }

    #[test]
    fn court_state_needs_exactly_one_target() {
        let id = "ab".repeat(64);
        assert!(court_state_request(Some(&id), Some(&id)).is_err());
        assert!(court_state_request(None, None).is_err());
        assert!(court_state_request(Some("zz"), None).is_err());
        let r = court_state_request(None, Some(&id)).unwrap();
        assert!(r.claim_hex.is_empty() && r.session_hex.len() == 128);
    }

    #[test]
    fn builders_that_need_a_typed_missing_value_refuse_with_its_name() {
        for name in
            ["funded original schedule", "court paid-ingress schedule", "material intake schedule", "D2 disclosure fee schedule"]
        {
            let e = refuse_unless_adopted(name).unwrap_err();
            assert_eq!(e.code, exit::UNSAFE_REFUSED);
            assert!(e.msg.contains("not adopted") && e.msg.contains(name), "{}", e.msg);
        }
    }

    /// Build → borsh → decode → the consensus acceptance pieces a validating node runs on tag 154 without state: the kind's owning
    /// fence in force (armed in test params via the full-activation list; refused on the shipped dormant ruleset), the stateless
    /// carriage check, and the ML-DSA-87 signature over `palw_legacy_dispute_message_v1` under the consensus context — the exact
    /// message and verifier the processor's `palw_legacy_dispute_signature_ok` uses.
    #[test]
    fn a_built_reservation_round_trips_and_passes_the_consensus_checks_where_the_fence_is_armed() {
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([9u8; 32]);
        let reserver = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([3u8; 64]), 2));
        let mut params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            params.net.to_string().as_bytes(),
            Some(params.genesis.hash),
        );
        let reservation = PalwDisputeReservationV1 {
            version: PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim: claim(),
            execution_root: Hash64::from_bytes([1u8; 64]),
            trace_root: Hash64::from_bytes([2u8; 64]),
            reserver,
        };
        let object = dispute_reserved_object(domain, reservation.clone(), &key);
        let bytes = borsh::to_vec(&object).unwrap();
        assert_eq!(bytes[0], 154, "tag 154 on the wire");
        let decoded: PalwConsensusObjectV2 = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded, object);
        let PalwConsensusObjectV2::DisputeReservedV1 { reservation: got, signature } = &decoded else { panic!("tag 154") };
        assert_eq!(**got, reservation);
        let message = palw_legacy_dispute_message_v1(
            domain,
            kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RESERVED_TAG_V1,
            &reserver,
            &kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reserved_payload_v1(got),
        );
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                message.as_byte_slice(),
                signature,
                PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1
            )
            .unwrap_or(false)
        );
        // A signature under another network's domain does not verify: the domain binds the chain.
        let other = Hash64::from_bytes([0xEE; 64]);
        let wrong = palw_legacy_dispute_message_v1(
            other,
            kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RESERVED_TAG_V1,
            &reserver,
            &kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_reserved_payload_v1(got),
        );
        assert!(
            !kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                wrong.as_byte_slice(),
                signature,
                PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1
            )
            .unwrap_or(false)
        );
        kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&decoded).expect("tag 154 may ride");
        // The shipped ruleset holds the fence dormant: the kind is not read as a statement there.
        assert!(!params.palw_lifecycle_kind_fences_v1().kind_in_force_at(&decoded, 1_000_000));
        // Armed in test params (the full-activation list at DAA 100): in force from 100, not before.
        kaspa_consensus_core::config::params::palw_t12_arm_full_activation_at_v1(&mut params, Some(100));
        let fences = params.palw_lifecycle_kind_fences_v1();
        assert!(fences.kind_in_force_at(&decoded, 100));
        assert!(!fences.kind_in_force_at(&decoded, 99));
        // object-show reports the same.
        let summary = object_summary(&bytes, Some(&params), 100).unwrap();
        assert_eq!(summary["tag"], 154);
        assert_eq!(summary["variant"], "DisputeReservedV1");
        assert_eq!(summary["kindInForce"], true);
        assert_eq!(summary["mayRide"], true);
    }

    #[test]
    fn a_built_release_round_trips_and_its_signature_verifies() {
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([5u8; 32]);
        let reserver = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([4u8; 64]), 0));
        let domain = Hash64::from_bytes([0x11; 64]);
        let object = dispute_released_object(domain, claim(), reserver, &key);
        let bytes = borsh::to_vec(&object).unwrap();
        assert_eq!(bytes[0], 155);
        let PalwConsensusObjectV2::DisputeReleasedV1 { claim: c, reserver: r, signature } = borsh::from_slice(&bytes).unwrap() else {
            panic!("tag 155")
        };
        let message = palw_legacy_dispute_message_v1(
            domain,
            kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RELEASED_TAG_V1,
            &r,
            &kaspa_consensus_core::palw_legacy_public_filer_v1::palw_dispute_released_payload_v1(&c),
        );
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                message.as_byte_slice(),
                &signature,
                PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1
            )
            .unwrap_or(false)
        );
    }

    #[test]
    fn fences_filter_by_armed_and_list_and_refuses_an_unknown_list() {
        let mut params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
        kaspa_consensus_core::config::params::palw_t12_arm_full_activation_at_v1(&mut params, Some(500));
        let doc = kaspa_consensus_core::palw_user_views_v1::palw_fence_schedule_json_v1(&params, 600);
        let full = filter_fences(doc.clone(), true, Some("fullActivation")).unwrap();
        let rows = full["fences"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.iter().all(|f| f["armed"] == true));
        assert!(filter_fences(doc.clone(), false, Some("noSuchList")).is_err());
        let rendered = render_fences(&full);
        assert!(rendered.contains("palw_independently_work_certified_v1"));
        assert!(rendered.contains("full activation: adopted height not adopted"));
    }

    #[test]
    fn the_human_claim_view_names_the_clocks_and_certification() {
        let doc = serde_json::json!({
            "version": 1, "nowDaa": 10, "tipDaa": 9,
            "claim": { "claimId": "c", "classId": "k", "executorBond": "b:0", "phase": "receipt_licensed", "phaseDaa": 5,
                       "voidReason": null, "phaseDeadlineDaa": 1205, "securityCertification": "pending", "claimLocalSettlement": null,
                       "reservedSompi": "1", "committedSompi": "2", "escrowedRewardSompi": 0, "payoutPendingSompi": null },
            "gateCGoverns": true, "accusationPending": true,
            "receiptPause": { "applies": true, "open": true, "openSinceDaa": 7, "pausedTotalDaa": 0 },
            "courtClocks": { "clockV2Governs": true, "noticeUntilDaa": 3125, "hardDeadlineDaa": 3125, "extendedHardDeadlineDaa": 10005,
                             "turnDeadlineDaa": 120, "windowCourt": 1200 },
            "dispute": { "record": false }, "daSessions": [], "courtSessions": []
        });
        let s = render_claim_detail(&doc);
        assert!(s.contains("notice until 3125") && s.contains("extended hard 10005") && s.contains("per-turn 120"));
        assert!(s.contains("certification (Gate C) pending") && s.contains("Final is not SecurityCertified"));
    }

    #[test]
    fn an_unknown_document_version_is_refused() {
        assert!(parse_view("op", true, true, "{\"version\":2}", "x").is_err());
        assert!(parse_view("op", false, false, "", "x").is_err());
        assert!(parse_view("op", true, false, "", "x").is_err());
        assert!(parse_view("op", true, true, "{\"version\":1}", "x").is_ok());
    }
}
