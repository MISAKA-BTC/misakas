//! `misaka palw panel` — participation, local status, chain seats, per-claim assignments.
//!
//! Four surfaces, kept apart on purpose:
//!
//! * `join` / `leave` / `readiness prove` — participation
//! * `status` — this node's local facts (artifact, working set, replay, bond, proof)
//! * `list` — chain facts only (bonded / ready / selected / receipts)
//! * `assignments` — which seats a claim drew, and which segment each partial holds
//!
//! `holds`, `Ready`, and `panel selected` are never the same number.

use crate::bond;
use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::{sompi_to_msk, connect};
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_panel_view_v1::palw_parse_class_alias_v1;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{
    GetPalwClassPanelStatusRequest, GetPalwPanelAssignmentsRequest, GetPalwPanelSeatsRequest, GetPalwPanelStatusRequest,
    RpcPalwClassPanelStatus, RpcPalwLocalPanelClass, RpcPalwPanelSeat,
};

fn short_id(id: &str) -> String {
    let take = id.find(':').unwrap_or(id.len()).min(12);
    format!("{}…{}", &id[..take], id.rsplit_once(':').map(|(_, i)| format!(":{i}")).unwrap_or_default())
}

fn resolve_class(raw: &str) -> Result<String, CliError> {
    palw_parse_class_alias_v1(raw).map(|id| id.to_string()).map_err(|e| CliError::new(exit::GENERIC, e))
}

fn fmt_gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / ((1u64 << 30) as f64))
}

fn fmt_msk(sompi: u64) -> String {
    let s = sompi_to_msk(sompi);
    if s.contains('.') { s } else { format!("{s} MSK") }
}

fn render_local(c: &RpcPalwLocalPanelClass) -> String {
    let class = if c.model_name.is_empty() { short_id(&c.class_id) } else { c.model_name.clone() };
    let mut out = String::new();
    out.push_str(&format!("Seat             {}\n", if c.seat_id.is_empty() { "—".into() } else { short_id(&c.seat_id) }));
    out.push_str(&format!("Class            {class}\n"));
    out.push_str(&format!("Artifact         {}\n", if c.artifact_loaded { "loaded" } else { "missing" }));
    out.push_str(&format!("Artifact root    {}\n", if c.artifact_root.is_empty() { "—".into() } else { short_id(&c.artifact_root) }));
    out.push_str(&format!("Working set      {}\n", if c.working_set_bytes == 0 { "—".into() } else { fmt_gib(c.working_set_bytes) }));
    out.push_str(&format!("Replay capable   {}\n", if c.replay_capable { "yes" } else { "no" }));
    out.push_str(&format!("Synced           {}\n", if c.synced { "yes" } else { "no" }));
    out.push_str(&format!("Bond active      {}\n", if c.bond_active { "yes" } else { "no" }));
    out.push_str(&format!("Collateral       {}\n", if c.collateral_sompi == 0 { "—".into() } else { fmt_msk(c.collateral_sompi) }));
    out.push_str(&format!(
        "Readiness proof  {}\n",
        if c.readiness_proof_accepted { format!("accepted @ DAA {}", c.readiness_proved_daa) } else { "missing".into() }
    ));
    out.push_str(&format!("Chain state      {}\n", if c.chain_state.is_empty() { "—".into() } else { c.chain_state.clone() }));
    out.push_str(&format!("Assignments      {}\n", c.assignments));
    if let Some(h) = &c.hold {
        out.push_str(&format!("Hold             {} — {}\n", h.code, h.message));
    }
    out
}

fn render_class_explorer(s: &RpcPalwClassPanelStatus) -> String {
    let name = if s.model_name.is_empty() { short_id(&s.class_id) } else { s.model_name.clone() };
    format!(
        "{name}\n{} Ready / {} required\n{} seats per claim\n{} receipts required\n{} full + {} segment verification\n{} claims inflight\n",
        s.ready_seats,
        s.required_ready_seats,
        s.panel_size,
        s.receipt_quorum,
        s.full_seats_per_panel,
        s.partial_seats_per_panel,
        s.inflight_claims,
    )
}

fn render_class_chain(s: &RpcPalwClassPanelStatus) -> String {
    let name = if s.model_name.is_empty() { short_id(&s.class_id) } else { s.model_name.clone() };
    let mut out = String::new();
    out.push_str(&format!("{name}\n"));
    out.push_str(&format!("registry state       {}\n", s.registry_state));
    out.push_str(&format!("requiredReadySeats   {}\n", s.required_ready_seats));
    out.push_str(&format!("readySeats           {}\n", s.ready_seats));
    out.push_str(&format!("bondedSeats          {}\n", s.bonded_seats));
    out.push_str(&format!("selectedPanelSeats   {}\n", s.selected_panel_seats));
    out.push_str(&format!("validReceiptSeats    {}\n", s.valid_receipt_seats));
    out.push_str(&format!("panelSize            {}\n", s.panel_size));
    out.push_str(&format!("receiptQuorum        {}\n", s.receipt_quorum));
    out.push_str(&format!("full + partial       {} + {}\n", s.full_seats_per_panel, s.partial_seats_per_panel));
    out.push_str(&format!("segmentCount         {}\n", s.segment_count));
    out.push_str(&format!("inflightClaims       {}\n", s.inflight_claims));
    out.push_str(&format!("verificationMode     {} (S1 {}/{}, S3 {}/{}, S2 {}/{})\n",
        s.verification_mode,
        if s.s1_active { "active" } else { "scheduled" }, s.s1_scheduled_daa,
        if s.s3_active { "active" } else { "scheduled" }, s.s3_scheduled_daa,
        if s.s2_active { "active" } else { "scheduled" }, s.s2_scheduled_daa,
    ));
    if !s.missing.is_empty() {
        let missing: u32 = s.missing.iter().map(|m| m.seats).sum();
        out.push_str(&format!("\n{missing} seats not ready:\n"));
        for m in &s.missing {
            out.push_str(&format!("  {} {}\n", m.seats, m.code));
        }
    }
    out
}

fn render_seats(class: &str, seats: &[RpcPalwPanelSeat]) -> String {
    let mut out = String::new();
    out.push_str(&format!("Seat          Ready  Last proof  Collateral         Assigned\n"));
    for s in seats.iter().filter(|s| class.is_empty() || s.class_id == class || s.class_id.starts_with(class)) {
        let ready = if s.ready { "yes" } else { "no " };
        let proof = if s.readiness_proved_daa == 0 { "—".into() } else { s.readiness_proved_daa.to_string() };
        let coll = s.collateral_available.parse::<u128>().ok().map(|n| fmt_msk(n.min(u128::from(u64::MAX)) as u64)).unwrap_or_else(|| s.collateral_available.clone());
        out.push_str(&format!("{:<13} {ready}   {:<10} {:<18} {}\n", short_id(&s.seat_id), proof, coll, s.assigned));
        if let Some(h) = &s.hold {
            out.push_str(&format!("              {}\n", h.code));
        }
    }
    out
}

pub async fn status(ctx: &Ctx, class: Option<&str>) -> CliResult {
    let nv = connect(ctx).await?;
    let class_id = class.map(resolve_class).transpose()?.unwrap_or_default();
    let r = nv
        .client
        .get_palw_panel_status(GetPalwPanelStatusRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelStatus: {e}")))?;
    match ctx.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&r).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?);
        }
        OutputFormat::Human => {
            if !r.available {
                println!("this node has no ConsensusV2 panel state");
                return Ok(());
            }
            println!("panel running    {}", if r.panel_running { "yes" } else { "no" });
            println!("submitter        {}", if r.panel_submitter { "yes" } else { "no" });
            println!("synced           {}", if r.synced { "yes" } else { "no" });
            println!("tip DAA          {}", r.tip_daa);
            println!();
            if r.classes.is_empty() {
                println!("no local class rows — start the verifier, or `palw panel list` for chain seats");
            }
            for (i, c) in r.classes.iter().enumerate() {
                if i > 0 {
                    println!();
                }
                print!("{}", render_local(c));
            }
        }
    }
    Ok(())
}

pub async fn list(ctx: &Ctx, class: Option<&str>) -> CliResult {
    let nv = connect(ctx).await?;
    let class_id = class.map(resolve_class).transpose()?.unwrap_or_default();
    if class_id.is_empty() {
        let seats = nv
            .client
            .get_palw_panel_seats(GetPalwPanelSeatsRequest { class_id: String::new() })
            .await
            .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelSeats: {e}")))?;
        match ctx.output {
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&seats).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?);
            }
            OutputFormat::Human => {
                if !seats.available {
                    println!("this chain has no ConsensusV2 panel seats");
                    return Ok(());
                }
                let mut class_ids: Vec<String> = seats.seats.iter().map(|s| s.class_id.clone()).collect();
                class_ids.sort();
                class_ids.dedup();
                for (i, id) in class_ids.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    match nv.client.get_palw_class_panel_status(GetPalwClassPanelStatusRequest { class_id: id.clone() }).await {
                        Ok(st) if st.available && st.found => print!("{}", render_class_explorer(&st.status)),
                        _ => println!("{}\n", short_id(id)),
                    }
                }
                if class_ids.is_empty() {
                    println!("no bonded verifier seats");
                } else {
                    println!();
                    print!("{}", render_seats("", &seats.seats));
                }
            }
        }
        return Ok(());
    }
    let status = nv
        .client
        .get_palw_class_panel_status(GetPalwClassPanelStatusRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwClassPanelStatus: {e}")))?;
    let seats = nv
        .client
        .get_palw_panel_seats(GetPalwPanelSeatsRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelSeats: {e}")))?;
    match ctx.output {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::json!({ "status": status, "seats": seats })
            );
        }
        OutputFormat::Human => {
            if !status.available || !status.found {
                println!("class {class_id} is not on this chain");
                return Ok(());
            }
            print!("{}", render_class_chain(&status.status));
            println!();
            print!("{}", render_seats(&class_id, &seats.seats));
        }
    }
    Ok(())
}

pub async fn assignments(ctx: &Ctx, claim: Option<&str>) -> CliResult {
    let nv = connect(ctx).await?;
    let claim_id = claim.map(|c| c.trim().to_string()).unwrap_or_default();
    let r = nv
        .client
        .get_palw_panel_assignments(GetPalwPanelAssignmentsRequest { claim_id: claim_id.clone(), seat_id: String::new() })
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelAssignments: {e}")))?;
    match ctx.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&r).map_err(|e| CliError::new(exit::GENERIC, e.to_string()))?);
        }
        OutputFormat::Human => {
            if !r.available {
                println!("this chain has no ConsensusV2 panel assignments");
                return Ok(());
            }
            if r.assignments.is_empty() {
                println!("no inflight panel assignments");
                return Ok(());
            }
            for a in &r.assignments {
                println!(
                    "claim {}  class {}  {}  deadline {}  coverage {:#010x}  valid {}/{}",
                    short_id(&a.claim_id),
                    short_id(&a.class_id),
                    a.licensed_state,
                    a.deadline_daa,
                    a.coverage_mask,
                    a.valid_receipt_seats,
                    a.selected_panel_seats
                );
                println!("  full seat {}", short_id(&a.full_seat));
                for s in &a.seats {
                    let kind = if s.full_seat {
                        "full".to_string()
                    } else {
                        format!("seg {}", s.segment_index.map(|i| i.to_string()).unwrap_or_else(|| "—".into()))
                    };
                    println!("  {kind:<8} {}  {}  mask {:#010x}", short_id(&s.seat_id), s.receipt_status, s.mask);
                }
                println!();
            }
            if r.truncated {
                println!("(truncated at 512 assignments)");
            }
        }
    }
    Ok(())
}

pub async fn join(
    ctx: &Ctx,
    ks: &KeySource,
    bond: Option<&str>,
    class: &str,
    artifact: &str,
    yes: bool,
) -> CliResult {
    let class_id = resolve_class(class)?;
    let nv = connect(ctx).await?;
    let mut declare = Vec::new();
    if let Some(bond_s) = bond {
        let claims = nv
            .client
            .get_palw_claims(bond_s.to_string(), "seat".into(), false, 1)
            .await
            .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwClaims: {e}")))?;
        declare = claims.bond_capable_classes;
    }
    if !declare.iter().any(|c| c.eq_ignore_ascii_case(&class_id)) {
        declare.push(class_id.clone());
    }
    println!("Declaring class {class_id} on the bond (artifact {artifact} is loaded by verifier start, not by this command).");
    bond::capability(ctx, ks, bond, Some(&class_id), &declare, false, yes).await
}

pub async fn leave(ctx: &Ctx, ks: &KeySource, bond: Option<&str>, class: &str, yes: bool) -> CliResult {
    let class_id = resolve_class(class)?;
    let nv = connect(ctx).await?;
    let Some(bond_s) = bond else {
        return Err(CliError::new(exit::GENERIC, "leave needs --bond <txid:index>"));
    };
    let claims = nv
        .client
        .get_palw_claims(bond_s.to_string(), "seat".into(), false, 1)
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwClaims: {e}")))?;
    let declare: Vec<String> = claims.bond_capable_classes.into_iter().filter(|c| !c.eq_ignore_ascii_case(&class_id)).collect();
    bond::capability(ctx, ks, bond, Some(&class_id), &declare, false, yes).await
}

pub async fn readiness_prove(ctx: &Ctx, class: &str, bond: &str) -> CliResult {
    let class_id = resolve_class(class)?;
    let nv = connect(ctx).await?;
    let status = nv
        .client
        .get_palw_panel_status(GetPalwPanelStatusRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelStatus: {e}")))?;
    let seats = nv
        .client
        .get_palw_panel_seats(GetPalwPanelSeatsRequest { class_id: class_id.clone() })
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("getPalwPanelSeats: {e}")))?;
    match ctx.output {
        OutputFormat::Json => {
            println!("{}", serde_json::json!({ "status": status, "seats": seats, "bond": bond }));
        }
        OutputFormat::Human => {
            let seat = seats.seats.iter().find(|s| s.seat_id == bond || s.bond_outpoint == bond);
            println!("The running panel submits a possession proof when one is due — this command does not carry one.");
            println!();
            if let Some(s) = seat {
                println!("Seat   {}", s.seat_id);
                println!("Ready  {}", if s.ready { "yes" } else { "no" });
                if s.ready {
                    println!("Proof  accepted @ DAA {} (expires {})", s.readiness_proved_daa, s.readiness_expires_daa);
                } else if let Some(h) = &s.hold {
                    println!("Hold   {} — {}", h.code, h.message);
                }
            } else {
                println!("bond {bond} is not a chain-recognised seat for this class");
            }
            if !status.panel_running {
                println!();
                println!("This node is not running a panel. Start the verifier with the class artifact loaded.");
            }
        }
    }
    Ok(())
}
