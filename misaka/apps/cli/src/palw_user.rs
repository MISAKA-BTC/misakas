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
    /// **Open a court against a claim (`CourtOpened`)** — an outside prosecutor's move after its own replay disagrees. The claim's
    /// trace root and producer bond are read from the node (`getPalwLegacyDispute`, op 204); the space is the ruleset's
    /// `StepLeaves × max_step_leaf_count` (acceptance refuses any other); the session id is signed with the challenger bond's key
    /// under the court's open context. Past `palw_court_outsider_lane_v1` the fold burns the V4 paid-ingress fee (estimate
    /// 1,100,000 sompi per session, inside openers too) from the opener — the build prints what is due. `--dry-run` prints the
    /// session and fee and writes nothing.
    CourtOpen {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
        /// The challenger's bond outpoint, `<txid>:<index>`.
        #[arg(long)]
        bond: String,
        /// Print the session id and the fee due; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Where to write the signed object (required unless --dry-run).
        #[arg(long, required_unless_present = "dry_run")]
        out: Option<std::path::PathBuf>,
    },
    /// **Post a kernel-route job (`KernelRouteV1 / PostJob`)** with a price quote. The escrow is not a choice: the route reserves
    /// `claim_reward` (+ the verifier check-fee escrow once `palw_verifier_pay_v1` is in force) from the POSTER's bond free
    /// collateral and burns `job_fee` (GAP-5, user-pays escrow); the poster's bond must have joined the kernel route. Refused below
    /// `palw_probabilistic_constraints_v1` unless `--allow-dormant`. `--dry-run` prints the job id and quote and writes nothing.
    KernelPostJob {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// The poster's bond outpoint, `<txid>:<index>`.
        #[arg(long)]
        bond: String,
        /// 128-hex kernel class binding id (the class the job runs on).
        #[arg(long)]
        class: String,
        /// Prompt token ids, comma separated (`3,17,9`).
        #[arg(long)]
        prompt: String,
        /// Exactly this many generated tokens (greedy decode).
        #[arg(long)]
        max_new: u32,
        /// 128-hex nonce (two identical requests are two jobs); default: derived from the bond and the virtual DAA.
        #[arg(long)]
        nonce: Option<String>,
        /// Write the object even though the kernel route is not armed at the node's DAA.
        #[arg(long)]
        allow_dormant: bool,
        /// Print the job id and the quote; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Where to write the signed object (required unless --dry-run).
        #[arg(long, required_unless_present = "dry_run")]
        out: Option<std::path::PathBuf>,
    },
    /// **File a FileProof against a kernel route claim (outside prosecutor)** — the node verifier's own pursuit, as a library
    /// (`misaka_palw_remote::pursuit`): the route's rooted rows are read (op 211), the claim is replayed from its public commitments
    /// with `--model` (a `PALWKM01` file whose commitments the class registered), and the planner says the next step — `SealProof`
    /// first, then (once the seal is folded, run again) the `FileProof`, whole or in chunks — each dry-run against the next block's
    /// ledger. Nothing is written when the claim checks clean. No cryptographic proof of computation: the court re-executes.
    FileProof {
        #[command(flatten)]
        key: crate::KeyArgs,
        /// 128-hex kernel claim id.
        #[arg(long)]
        claim: String,
        /// The accuser's bond outpoint, `<txid>:<index>`.
        #[arg(long)]
        bond: String,
        /// The kernel model file (`PALWKM01`) the claim's class registered.
        #[arg(long)]
        model: std::path::PathBuf,
        /// Print the planned step; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Directory the signed objects are written to (`<n>-<kind>.bin`), in carriage order (required unless --dry-run).
        #[arg(long, required_unless_present = "dry_run")]
        out_dir: Option<std::path::PathBuf>,
    },
    /// Funded original (TFO1 envelope with payer consent). Refused: consensus holds the funded-original schedule typed-missing in
    /// this build (`palw_funded_original_schedule_v1` always answers `Schedule` missing); lane SETTLE's estimate is not in the release.
    FundedOriginal {
        /// 128-hex claim id.
        #[arg(long)]
        claim: String,
    },
    /// Court paid ingress (V4). There is no separate ingress object: the fee is burned from the opener of `CourtOpened` —
    /// this prints the adopted schedule and points to `palw build court-open`.
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

fn read_error(op_name: &str, op: u16, fallback: &str, e: &kaspa_rpc_core::RpcError) -> CliError {
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
fn parse_view(op_name: &str, available: bool, found: bool, json: &str, what: &str) -> Result<Value, CliError> {
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

fn emit(ctx: &Ctx, json: bool, mut doc: Value, human: impl FnOnce(&Value) -> String) {
    if json || ctx.output == OutputFormat::Json {
        doc["schema"] = PALW_USER_CLI_SCHEMA.into();
        println!("{}", serde_json::to_string_pretty(&doc).expect("a JSON value serializes"));
    } else {
        print!("{}", human(&doc));
    }
}

fn text(v: &Value) -> String {
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
    let doc = parse_view("getPalwClaimDetail", r.available, r.found, &r.view_json, "such claim")?;
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
    let answer = reader.client.get_palw_fence_schedule(GetPalwFenceScheduleRequest {}).await;
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

use misaka_palw_kernel::route::KernelRouteObjectV1 as KernelObj;

/// `CourtOpened` over the ruleset's step-leaf space, the session id signed under the court's open context — the bytes the
/// node's own challenger builds (`palw_panel`).
pub(crate) fn court_opened_object_v1(
    claim: Hash64,
    trace_root: Hash64,
    executor: PalwBondKeyV2,
    challenger: PalwBondKeyV2,
    space_size: u64,
    key: &kaspa_pq_validator_core::ValidatorKey,
) -> PalwConsensusObjectV2 {
    use kaspa_consensus_core::palw_court_v2::{PALW_COURT_V2_MLDSA87_OPEN_CONTEXT, court_session_id_v2};
    let space = kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves;
    let session_id = court_session_id_v2(&claim, &trace_root, &executor, &challenger, space, space_size);
    let signature = key.sign_with_context(session_id.as_byte_slice(), PALW_COURT_V2_MLDSA87_OPEN_CONTEXT).to_vec();
    PalwConsensusObjectV2::CourtOpened { session_id, claim, challenger_bond: challenger, space, space_size, signature }
}

/// What the fold charges the opener of a court at this DAA (V4).
pub(crate) fn court_ingress_fee_line_v1(outsider_lane_in_force: bool) -> String {
    match kaspa_consensus_core::palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1() {
        Ok(s) if outsider_lane_in_force => {
            format!("paid ingress due: {} sompi, burned from the opener by the fold (V4 estimate)", s.fee_sompi_per_session)
        }
        Ok(s) => format!(
            "paid ingress: none due yet (palw_court_outsider_lane_v1 not in force); {} sompi per session once it is",
            s.fee_sompi_per_session
        ),
        Err(_) => "paid ingress: schedule typed-missing in this build, no fee".to_string(),
    }
}

/// A kernel route object signed by `signer`'s key under the route's object context.
pub(crate) fn kernel_route_object_v1(
    domain: Hash64,
    signer: PalwBondKeyV2,
    object: &KernelObj,
    key: &kaspa_pq_validator_core::ValidatorKey,
) -> PalwConsensusObjectV2 {
    use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_route_message_v1};
    let bytes = object.encode();
    let message = palw_kernel_route_message_v1(domain, &signer, &bytes);
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1).to_vec();
    PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature }
}

use kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_bond_id_v1;

/// The poster's cost of one `PostJob`, from the route's interim policy (the amounts the fold debits).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct KernelJobQuoteV1 {
    pub reward_escrow: u64,
    pub check_fee_escrow: u64,
    pub job_fee_burned: u64,
}

impl KernelJobQuoteV1 {
    pub fn free_collateral_needed(&self) -> u64 {
        self.reward_escrow.saturating_add(self.check_fee_escrow).saturating_add(self.job_fee_burned)
    }
    pub fn render(&self) -> String {
        format!(
            "quote: escrow {} sompi (Final reward, paid once to the first Final producer; returned after the escrow TTL if unused) \
             + check-fee escrow {} + job fee {} burned = {} sompi of the poster bond's free collateral",
            self.reward_escrow,
            self.check_fee_escrow,
            self.job_fee_burned,
            self.free_collateral_needed()
        )
    }
}

pub(crate) fn kernel_job_quote_v1(verifier_pay_activation: Option<u64>) -> KernelJobQuoteV1 {
    let p = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
    let check_fee_escrow = verifier_pay_activation
        .map(kaspa_consensus_core::palw_verifier_pay_v1::palw_verifier_pay_interim_policy_v1)
        .map_or(0, |v| v.check_fee.saturating_mul(u64::from(v.slots)));
    KernelJobQuoteV1 { reward_escrow: p.claim_reward, check_fee_escrow, job_fee_burned: p.job_fee }
}

/// The kernel route as the next block's fold will judge it: the rooted rows (op 211), rebuilt and checked against the served
/// roots, with the fences the fold injects (provider court, verifier pay) in force at `virtual DAA + 1`.
async fn kernel_route_view_v1(
    nv: &crate::wallet::NodeView,
) -> Result<
    (
        kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1,
        kaspa_consensus_core::palw_native_resource_v1::ReservedKernelLedgerV1,
    ),
    CliError,
> {
    let (snapshot, rows) = crate::operator::model_complete::kernel_snapshot(nv).await?;
    let route = kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1::from_served_rows_v1(
        &snapshot.header,
        rows,
        &snapshot.ledger_root,
        &snapshot.aux_root,
    )
    .map_err(|e| CliError::new(exit::GENERIC, format!("the served kernel rows do not rebuild: {e}")))?;
    let next = nv.virtual_daa.saturating_add(1);
    let in_force = |f: Option<kaspa_consensus_core::config::params::ForkActivation>| {
        f.filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never() && f.is_active(next)).map(|f| f.daa_score())
    };
    let verifier_pay = nv
        .params
        .palw_verifier_pay_v1
        .filter(|f| *f != kaspa_consensus_core::config::params::ForkActivation::never())
        .map(|f| f.daa_score());
    let ledger = misaka_palw_remote::pursuit::kernel_consumer_ledger_v1(&route, in_force(nv.params.palw_provider_court_v1), verifier_pay)
        .map_err(|e| CliError::new(exit::GENERIC, e))?;
    Ok((route, ledger))
}

/// The prosecution standing of one kernel claim, from its rooted row and the route's policy (`palw prosecution-status`).
pub(crate) fn prosecution_status_json_v1(ledger: &misaka_palw_kernel::ledger::KernelLedgerV1, claim: Hash64) -> Value {
    let Some(row) = ledger.claims.get(&claim.as_bytes()) else {
        return serde_json::json!({ "claim": claim.to_string(), "found": false });
    };
    let share = ledger.policy.accuser_reward_permille;
    serde_json::json!({
        "claim": claim.to_string(),
        "found": true,
        "state": format!("{:?}", row.life.state),
        "convicted": row.convicted,
        // Before a conviction: what a conviction would slash (the claim's live reservation, capped by the bond at that time).
        // After it: the reservation is zeroed; the slashed amount is in the block's `Convicted` event, not retained in the rows.
        "reservedSompi": row.reserved,
        "reporterSharePermille": share,
        "reporterBountyIfConvictedNowSompi": (row.reserved as u128 * share as u128 / 1000) as u64,
        // The bounty is credited to the accuser's route bond in the same fold as the conviction: convicted ⇒ paid.
        "bountyPaid": row.convicted,
        "liabilityUntilDaa": row.liability_until,
        "ledgerDaa": ledger.daa,
    })
}

pub(crate) async fn prosecution_status(ctx: &Ctx, claim: &str, json: bool) -> CliResult {
    let claim = parse_hash("CLAIM_ID", claim)?;
    let nv = crate::wallet::connect(ctx).await?;
    let (_route, ledger) = kernel_route_view_v1(&nv).await?;
    let doc = prosecution_status_json_v1(&ledger, claim);
    emit(ctx, json, doc, |d| {
        if d["found"] == false {
            return format!("the kernel route holds no claim {}\n", text(&d["claim"]));
        }
        format!(
            "claim {}: {} — convicted {}, bounty paid {}\n  reservation {} sompi, reporter share {}‰ (bounty now {} sompi), ledger DAA {}\n",
            text(&d["claim"]),
            text(&d["state"]),
            text(&d["convicted"]),
            text(&d["bountyPaid"]),
            text(&d["reservedSompi"]),
            text(&d["reporterSharePermille"]),
            text(&d["reporterBountyIfConvictedNowSompi"]),
            text(&d["ledgerDaa"])
        )
    });
    Ok(())
}

fn parse_token_list_v1(s: &str) -> Result<Vec<u32>, CliError> {
    let tokens: Result<Vec<u32>, _> = s.split(',').map(|t| t.trim().parse::<u32>()).collect();
    match tokens {
        Ok(t) if !t.is_empty() => Ok(t),
        _ => Err(CliError::new(exit::GENERIC, format!("--prompt '{s}' is not a comma-separated list of token ids"))),
    }
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
    let (nv, key, reserver) = bond_signer_v1(ctx, ks, bond).await?;
    let armed = nv.params.palw_legacy_public_filer_v1.is_some_and(|a| a.is_active(nv.virtual_daa));
    refuse_unless_armed(&nv, "palw_legacy_public_filer_v1", armed, allow_dormant, "a dispute object")?;
    Ok((nv, key, reserver))
}

/// Refuse (exit 10) when `fence` is not in force at the node's DAA, unless `--allow-dormant`.
fn refuse_unless_armed(
    nv: &crate::wallet::NodeView,
    fence: &str,
    armed: bool,
    allow_dormant: bool,
    what: &str,
) -> Result<(), CliError> {
    if !armed && !allow_dormant {
        return Err(CliError::new(
            exit::UNSAFE_REFUSED,
            format!(
                "refused: {fence} is not armed on {} at DAA {} — the chain drops {what} below it \
                 (`misaka palw fences` shows the schedule; --allow-dormant writes it anyway for a drill)",
                nv.params.net, nv.virtual_daa
            ),
        ));
    }
    Ok(())
}

/// The node, and the bond's key check (a signature under another key is refused on chain and the fee is gone) — no fence check.
async fn bond_signer_v1(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    bond: &str,
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
        BuildCmd::CourtOpen { key, claim, bond, dry_run, out } => {
            let claim = parse_hash("--claim", &claim)?;
            let (nv, signer, challenger) = bond_signer_v1(ctx, &key.source(), &bond).await?;
            let view = nv
                .client
                .get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: claim.to_string() })
                .await
                .map_err(|e| read_error("getPalwLegacyDispute", 204, "the claim's trace root and producer are needed", &e))?;
            if !view.available || view.json.is_empty() {
                return Err(CliError::new(exit::GENERIC, format!("the chain holds no claim {claim} to open a court against")));
            }
            let doc: Value = serde_json::from_str(&view.json)
                .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwLegacyDispute: bad document: {e}")))?;
            let trace_root = parse_hash("traceRoot", &text(&doc["traceRoot"]))?;
            let executor = PalwBondKeyV2(crate::bond::parse_outpoint(&text(&doc["producer"]))?);
            let space_size = match &nv.params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => b.court.max_step_leaf_count(),
                _ => return Err(CliError::new(exit::GENERIC, "this network has no PALW V2 court".to_string())),
            };
            let object = court_opened_object_v1(claim, trace_root, executor, challenger, space_size, &signer);
            let PalwConsensusObjectV2::CourtOpened { session_id, .. } = &object else { unreachable!() };
            let outsider_lane = nv.params.palw_court_outsider_lane_v1.is_some_and(|a| a.is_active(nv.virtual_daa));
            println!("CourtOpened: claim {claim}, challenger {bond}, session {session_id}, space StepLeaves × {space_size}");
            println!("{}", court_ingress_fee_line_v1(outsider_lane));
            match (dry_run, out) {
                (true, _) => {
                    println!("dry run: nothing written");
                    Ok(())
                }
                (false, Some(out)) => write_object(&out, &object),
                (false, None) => unreachable!("clap requires --out without --dry-run"),
            }
        }
        BuildCmd::KernelPostJob { key, bond, class, prompt, max_new, nonce, allow_dormant, dry_run, out } => {
            let class = parse_hash("--class", &class)?;
            let prompt = parse_token_list_v1(&prompt)?;
            if max_new == 0 {
                return Err(CliError::new(exit::GENERIC, "--max-new must be at least 1".to_string()));
            }
            let nonce = nonce.map(|n| parse_hash("--nonce", &n)).transpose()?;
            let (nv, signer, poster) = bond_signer_v1(ctx, &key.source(), &bond).await?;
            let armed = nv.params.palw_probabilistic_constraints_v1.is_some_and(|a| a.is_active(nv.virtual_daa));
            refuse_unless_armed(&nv, "palw_probabilistic_constraints_v1 (the kernel route)", armed, allow_dormant, "a kernel route object")?;
            let nonce = nonce.unwrap_or_else(|| {
                let mut seed = palw_kernel_bond_id_v1(&poster).to_vec();
                seed.extend_from_slice(&nv.virtual_daa.to_le_bytes());
                Hash64::from_bytes(misaka_palw_kernel::hash::id(b"misaka-palw/cli/kernel-post-job/nonce/v1", &seed))
            });
            let job = misaka_palw_kernel::job::KernelJobV1 {
                class_binding_id: class.as_bytes(),
                prompt,
                max_new_tokens: max_new,
                decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
                nonce: nonce.as_bytes(),
            };
            let verifier_pay = nv.params.palw_verifier_pay_v1.filter(|a| a.is_active(nv.virtual_daa)).map(|a| a.daa_score());
            let quote = kernel_job_quote_v1(verifier_pay);
            println!(
                "PostJob: job {}, class {class}, {} prompt tokens, {max_new} new tokens, poster {bond}",
                Hash64::from_bytes(job.id()),
                job.prompt.len()
            );
            println!("{}", quote.render());
            let object = kernel_route_object_v1(crate::bond::network_domain(&nv), poster, &KernelObj::PostJob { job }, &signer);
            match (dry_run, out) {
                (true, _) => {
                    println!("dry run: nothing written");
                    Ok(())
                }
                (false, Some(out)) => write_object(&out, &object),
                (false, None) => unreachable!("clap requires --out without --dry-run"),
            }
        }
        BuildCmd::FileProof { key, claim, bond, model, dry_run, out_dir } => {
            use misaka_palw_remote::pursuit::{
                KERNEL_SERVICE_CHUNK_CAP_V1, KernelFilingV1, KernelGoalV1, KernelPursuitV1, KernelSendV1, kernel_check_program_v1,
                kernel_class_model_root_v1, kernel_model_read_v1, kernel_model_roots_v1, kernel_step_sends_v1, plan_kernel_pursuit_on_v1,
            };
            let claim = parse_hash("--claim", &claim)?;
            let params = kernel_model_read_v1(&model).map_err(|e| CliError::new(exit::GENERIC, e))?;
            let (nv, signer, accuser) = bond_signer_v1(ctx, &key.source(), &bond).await?;
            let (route, ledger) = kernel_route_view_v1(&nv).await?;
            let id = claim.as_bytes();
            let row = ledger.claims.get(&id).ok_or_else(|| CliError::new(exit::GENERIC, format!("the kernel route holds no claim {claim}")))?;
            let root = kernel_class_model_root_v1(&ledger, &row.class_binding_id)
                .ok_or_else(|| CliError::new(exit::GENERIC, "this claim's class is not checkable with one model (pipeline/composite)"))?;
            if !kernel_model_roots_v1(&params).contains(&root) {
                return Err(CliError::new(exit::GENERIC, "--model does not open to the class's registered parameter commitments"));
            }
            let me = palw_kernel_bond_id_v1(&accuser);
            let salt = misaka_palw_kernel::hash::id(b"misaka-palw/cli/kernel-filer/salt/v1", &me);
            let goal = kernel_check_program_v1(&ledger, &id, &params, salt).map_err(|e| CliError::new(exit::GENERIC, e))?;
            let Some(goal) = goal else {
                println!("claim {claim}: the replay checks clean — nothing to file");
                return Ok(());
            };
            let collateral = nv
                .client
                .get_palw_producer_facts(
                    crate::bond::bond_lookup_class(&nv)?,
                    accuser.0.transaction_id.to_string(),
                    accuser.0.index,
                    true,
                )
                .await
                .map_err(|e| CliError::connection(format!("cannot read the bond's facts: {e}")))?
                .bond_collateral;
            let synced = collateral.saturating_sub(route.chunk_deposits_of_v1(&accuser));
            let domain = crate::bond::network_domain(&nv);
            let pursuit = KernelPursuitV1 { claim: id, bond: accuser, goal, synced_collateral: Some(synced) };
            let filing = match &pursuit.goal {
                KernelGoalV1::Prove(proof) => {
                    Some(kernel_route_object_v1(domain, accuser, &KernelObj::FileProof { accuser: me, claim: id, proof: proof.clone() }, &signer))
                }
                KernelGoalV1::Demand(_) => None,
            };
            let f = filing.as_ref().map(|signed| KernelFilingV1 { signed, chunk_cap: KERNEL_SERVICE_CHUNK_CAP_V1 });
            let step = plan_kernel_pursuit_on_v1(&route, &ledger, &pursuit, f.as_ref()).map_err(|e| CliError::new(exit::GENERIC, e))?;
            let shown: String = format!("{step:?}").chars().take(400).collect();
            println!("claim {claim}: next step {shown}");
            let sends = kernel_step_sends_v1(&step, &pursuit, f.as_ref()).map_err(|e| CliError::new(exit::GENERIC, e))?;
            let objects: Vec<(String, PalwConsensusObjectV2)> = sends
                .into_iter()
                .map(|send| match send {
                    KernelSendV1::Route(KernelObj::FileProof { .. }) => ("FileProof".to_string(), filing.clone().expect("a Prove goal")),
                    KernelSendV1::Route(k) => (format!("{:?}", k).split([' ', '{', '(']).next().unwrap_or("Route").to_string(), kernel_route_object_v1(domain, accuser, &k, &signer)),
                    KernelSendV1::Chunk(chunk) => {
                        use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1, palw_kernel_chunk_message_v1};
                        let m = palw_kernel_chunk_message_v1(domain, &chunk);
                        let signature = signer.sign_with_context(m.as_byte_slice(), PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1).to_vec();
                        ("Chunk".to_string(), PalwConsensusObjectV2::KernelRouteChunkV1 { chunk: Box::new(chunk), signature })
                    }
                })
                .collect();
            if objects.is_empty() {
                println!("nothing to send at this step (wait or re-run later)");
                return Ok(());
            }
            println!("{} object(s) to carry, in order", objects.len());
            if dry_run {
                println!("dry run: nothing written");
                return Ok(());
            }
            let dir = out_dir.expect("clap requires --out-dir without --dry-run");
            std::fs::create_dir_all(&dir).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", dir.display())))?;
            for (i, (kind, object)) in objects.iter().enumerate() {
                write_object(&dir.join(format!("{i:03}-{kind}.bin")), object)?;
            }
            Ok(())
        }
        BuildCmd::FundedOriginal { claim } => {
            parse_hash("--claim", &claim)?;
            refuse_unless_adopted("funded original schedule").map_err(|e| {
                CliError::new(
                    e.code,
                    format!(
                        "{} The schedule estimate exists only on lane SETTLE's branch (claude/rfc-settle) and is not in this \
                         release, so no funded original can be accepted on this chain yet.",
                        e.msg
                    ),
                )
            })
        }
        BuildCmd::CourtIngress { claim } => {
            parse_hash("--claim", &claim)?;
            match kaspa_consensus_core::palw_court_paid_ingress_v1::palw_court_ingress_schedule_v1() {
                Ok(s) => {
                    println!(
                        "court paid ingress (V4, adopted estimate): {} sompi per session, burned from the opener of CourtOpened \
                         (inside openers too) once palw_court_outsider_lane_v1 is in force — there is no separate ingress object.",
                        s.fee_sompi_per_session
                    );
                    println!("open the court: misaka palw build court-open --claim {claim} --bond <txid:index> --key-file <key> --out <file>");
                    Ok(())
                }
                Err(_) => refuse_unless_adopted("court paid-ingress schedule"),
            }
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
        // Read from the register, not a fixed list: a value pinned since (V4 ingress, V5 intake, P3's D2) refuses differently.
        for v in palw_typed_missing_values_v1() {
            let e = refuse_unless_adopted(v.name).unwrap_err();
            assert_eq!(e.code, exit::UNSAFE_REFUSED);
            if v.adopted {
                assert!(e.msg.contains("is adopted"), "{}", e.msg);
            } else {
                assert!(e.msg.contains("not adopted") && e.msg.contains(v.name), "{}", e.msg);
            }
        }
        assert!(!palw_typed_missing_values_v1().iter().find(|v| v.name == "funded original schedule").unwrap().adopted);
        assert!(palw_typed_missing_values_v1().iter().find(|v| v.name == "court paid-ingress schedule").unwrap().adopted);
    }

    /// `court-open`: the built object borsh-round-trips, its session id is the consensus derivation over the declared space, and
    /// the signature verifies under the challenger's key and the court's open context (the acceptance layer's checks without state).
    #[test]
    fn a_built_court_opening_round_trips_and_its_signature_verifies() {
        use kaspa_consensus_core::palw_court_v2::{PALW_COURT_V2_MLDSA87_OPEN_CONTEXT, court_session_id_v2};
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([5u8; 32]);
        let bond = |b: u8| PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0));
        let object = court_opened_object_v1(claim(), Hash64::from_bytes([3u8; 64]), bond(1), bond(2), 4096, &key);
        let back: PalwConsensusObjectV2 = borsh::from_slice(&borsh::to_vec(&object).unwrap()).unwrap();
        assert_eq!(back, object);
        let PalwConsensusObjectV2::CourtOpened { session_id, claim: c, challenger_bond, space, space_size, signature } = back else {
            panic!("not a CourtOpened")
        };
        assert_eq!((c, challenger_bond, space_size), (claim(), bond(2), 4096));
        assert_eq!(session_id, court_session_id_v2(&claim(), &Hash64::from_bytes([3u8; 64]), &bond(1), &bond(2), space, 4096));
        assert!(
            kaspa_txscript::verify_mldsa87_with_context(
                key.public_key(),
                session_id.as_byte_slice(),
                &signature,
                PALW_COURT_V2_MLDSA87_OPEN_CONTEXT
            )
            .unwrap_or(false)
        );
        assert!(court_ingress_fee_line_v1(true).contains("1100000"));
        assert!(court_ingress_fee_line_v1(false).contains("none due yet"));
    }

    /// `kernel-post-job`: the route object decodes back to the same `PostJob` (the fold's decoder), and the quote is the policy's.
    #[test]
    fn a_built_kernel_job_decodes_on_the_route_and_is_quoted_from_the_policy() {
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([6u8; 32]);
        let poster = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([4; 64]), 1));
        let job = misaka_palw_kernel::job::KernelJobV1 {
            class_binding_id: [8u8; 64],
            prompt: parse_token_list_v1("3, 17,9").unwrap(),
            max_new_tokens: 3,
            decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
            nonce: [1u8; 64],
        };
        let object = kernel_route_object_v1(Hash64::from_bytes([2u8; 64]), poster, &KernelObj::PostJob { job: job.clone() }, &key);
        let back: PalwConsensusObjectV2 = borsh::from_slice(&borsh::to_vec(&object).unwrap()).unwrap();
        let PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, .. } = back else { panic!("not a kernel route object") };
        assert_eq!(signer, poster);
        assert_eq!(KernelObj::decode(&bytes).unwrap(), KernelObj::PostJob { job });
        assert!(parse_token_list_v1("3,x").is_err() && parse_token_list_v1("").is_err());
        let q = kernel_job_quote_v1(None);
        let p = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
        assert_eq!(q, KernelJobQuoteV1 { reward_escrow: p.claim_reward, check_fee_escrow: 0, job_fee_burned: p.job_fee });
        assert!(kernel_job_quote_v1(Some(0)).check_fee_escrow > 0);
    }

    #[test]
    fn the_new_builders_parse_with_dry_run_and_require_out_otherwise() {
        let id = "ab".repeat(64);
        let bond = format!("{}:0", "cd".repeat(32));
        let ok = |a: &[&str]| crate::Cli::try_parse_from([&["misaka", "palw", "build"], a].concat()).is_ok();
        assert!(ok(&["court-open", "--claim", &id, "--bond", &bond, "--key-file", "k", "--dry-run"]));
        assert!(!ok(&["court-open", "--claim", &id, "--bond", &bond, "--key-file", "k"]));
        assert!(ok(&["kernel-post-job", "--bond", &bond, "--class", &id, "--prompt", "1,2", "--max-new", "2", "--key-file", "k", "--out", "x"]));
        assert!(ok(&["court-ingress", "--claim", &id]));
        assert!(ok(&["file-proof", "--claim", &id, "--bond", &bond, "--model", "m.palwkm", "--key-file", "k", "--dry-run"]));
        assert!(!ok(&["file-proof", "--claim", &id, "--bond", &bond, "--model", "m.palwkm", "--key-file", "k"]));
        assert!(crate::Cli::try_parse_from(["misaka", "palw", "prosecution-status", &id, "--json"]).is_ok());
    }

    #[test]
    fn prosecution_status_reports_an_unknown_claim_and_the_policy_share() {
        let policy = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_policy_v1(Hash64::default(), Hash64::default());
        let ledger = kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_route_template_opv_v1(policy, None);
        let doc = prosecution_status_json_v1(&ledger, claim());
        assert_eq!(doc["found"], false);
        assert_eq!(policy.accuser_reward_permille, 490);
    }

    /// Build → borsh → decode → the consensus acceptance pieces a validating node runs on tag 154 without state: the kind's owning
    /// fence in force (armed in test params via the full-activation list; refused on the shipped dormant ruleset), the stateless
    /// carriage check, and the ML-DSA-87 signature over `palw_legacy_dispute_message_v1` under the consensus context — the exact
    /// message and verifier the processor's `palw_legacy_dispute_signature_ok` uses.
    #[test]
    fn a_built_reservation_round_trips_and_passes_the_consensus_checks_where_the_fence_is_armed() {
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed([9u8; 32]);
        let reserver = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_bytes([3u8; 64]), 2));
        let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
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
        // The shipped t12 ruleset arms the fence (the DAA 16,600 flag day): in force from its height, not before.
        let at = params.palw_legacy_public_filer_v1.expect("shipped t12 arms palw_legacy_public_filer_v1").daa_score();
        let fences = params.palw_lifecycle_kind_fences_v1();
        assert!(fences.kind_in_force_at(&decoded, at));
        assert!(fences.kind_in_force_at(&decoded, 1_000_000));
        assert!(!fences.kind_in_force_at(&decoded, at - 1));
        // object-show reports the same.
        let summary = object_summary(&bytes, Some(&params), at).unwrap();
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
