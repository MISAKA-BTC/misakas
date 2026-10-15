//! Public frozen paid-source contract inspection, including expired onboarding
//! attempts. The caller pins both route roots; a node cannot choose those roots.
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_kernel_route_v1::PalwKernelRouteStateV1;
use std::str::FromStr;

pub(crate) async fn inspect(ctx: &Ctx, context: &str, ledger_root: &str, aux_root: &str) -> CliResult {
    let hash =
        |value: &str, name| Hash64::from_str(value).map_err(|_| CliError::new(exit::CONFIG, format!("{name} must be a 64-byte root")));
    let context = hash(context, "context")?.as_bytes();
    let expected_ledger = hash(ledger_root, "ledger-root")?;
    let expected_aux = hash(aux_root, "aux-root")?;
    let nv = crate::wallet::connect(ctx).await?;
    // The common collector rejects oversized, incomplete, repeated and moving
    // pages before this reconstruction. Do not use server-chosen roots to pin it.
    let (snapshot, rows) = super::model_complete::kernel_snapshot(&nv).await?;
    if (snapshot.ledger_root, snapshot.aux_root) != (expected_ledger, expected_aux) {
        return Err(CliError::new(exit::GENERIC, "served source inventory differs from the caller's pinned route roots"));
    }
    let route = PalwKernelRouteStateV1::from_served_rows_v1(&snapshot.header, rows, &expected_ledger, &expected_aux)
        .map_err(|e| CliError::new(exit::GENERIC, format!("source route verification: {e}")))?;
    let target = route
        .beacon_source_fee_target_v5(&context)
        .map_err(|e| CliError::new(exit::NOT_READY, format!("frozen paid source target: {e}")))?;
    let costs =
        route.beacon_source_cost_bounds_v5(&context).map_err(|e| CliError::new(exit::GENERIC, format!("frozen source costs: {e}")))?;
    // Verify the whole inventory, including failed promises. This is not the
    // current-attempt verifier: a completed/replaced attempt may still own cash.
    let material =
        route.paid_beacon_material_v5(&context).map_err(|e| CliError::new(exit::GENERIC, format!("source funding inventory: {e}")))?;
    let ledger = route.ledger().map_err(|e| CliError::new(exit::GENERIC, e))?;
    let funding = ledger.beacon_source_fees.get(&context);
    let source = target.target.terms;
    let hex = misaka_palw_sdk::runtime_pack::commit::hex;
    let funding = funding.map(|row| {
        serde_json::json!({
            "termsId":hex(&row.terms.id()), "sponsor":hex(&row.terms.sponsor),
            "feePerSourceSompi":row.terms.fee_per_source, "maximumSources":row.terms.max_sources,
            "fundedDaa":row.funded_daa, "heldSompi":row.held,
            "logicalPaidSompi":row.paid, "logicalRefundedSompi":row.refunded,
            "allocatedSources":row.allocated, "unusedRefunded":row.unused_refunded,
        })
    });
    let (sources, promises) = material.as_ref().map_or((0, 0), |(_, s, p)| (s.len(), p.len()));
    let report = serde_json::json!({
        "schema":"misaka.palw.source-contract.inspect.v1",
        "context":hex(&context), "servedTipDaa":snapshot.tip_daa,
        "rootedLedgerDaa":route.header.scalars.daa,
        "ledgerRoot":expected_ledger.to_string(), "auxRoot":expected_aux.to_string(),
        "verification":"caller-pinned-route-roots", "subject":format!("{:?}",target.target.context.subject_kind),
        "sealStartDaa":source.seal_start, "sealEndDaa":source.seal_end,
        "revealEndDaa":source.reveal_end, "finalByDaa":source.final_by, "holdUntilDaa":source.hold_until,
        "depositSompi":source.deposit, "maximumPerProducer":source.max_per_producer,
        "maximumTotal":source.max_total, "sponsor":hex(&target.sponsor),
        "minimumFeeSompi":target.minimum_fee, "maximumFundedSources":target.max_sources,
        "proposedCosts":{
            "executionSompi":costs.execution_sompi, "carrierSompi":costs.carrier_sompi,
            "materialSompi":costs.material_sompi, "publicCourtSompi":costs.public_court_sompi,
            "lockedCapitalSompi":costs.locked_capital_sompi,
            "minimumNetReceiptSompi":costs.minimum_net_receipt_sompi, "reportRoot":hex(&costs.report_root),
            "maximumSourceDepositSompi":costs.maximum_source_deposit_sompi,
            "maximumLiabilityDaa":costs.maximum_liability_daa,
        },
        "funding":funding, "sourceInventory":sources, "promiseInventory":promises,
        "nativeCoinbasePaymentsVerified":false, "economicApproval":false, "soundnessApproval":false,
    });
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&report).expect("source report serializes"));
    } else {
        println!("source context    {}", hex(&context));
        println!("verified roots    {} / {}", expected_ledger, expected_aux);
        println!("rooted clock      DAA {}; served tip DAA {} is observational", route.header.scalars.daa, snapshot.tip_daa);
        println!(
            "admission window  DAA {}..{}; Final by {}; liability through {}",
            source.seal_start, source.seal_end, source.final_by, source.hold_until
        );
        println!("fee floor         {} sompi/source; deposit {} sompi", target.minimum_fee, source.deposit);
        println!("inventory         {sources} sources; {promises} promises");
        println!("funding           {}", report["funding"]);
        println!("payment evidence  logical ledger totals; native cash not verified");
    }
    Ok(())
}

/// Export the exact op-211 kernel route snapshot for offline signed drafting (`palw-class claim-v2 --snapshot`). The served rows
/// are reconstructed against the caller's pinned roots first; a mismatch writes nothing. The file is created exclusively and
/// synced, and records no authority beyond those caller-pinned roots.
pub(crate) async fn snapshot(ctx: &Ctx, ledger_root: &str, aux_root: &str, out: &std::path::Path) -> CliResult {
    use std::io::Write;
    let hash =
        |value: &str, name| Hash64::from_str(value).map_err(|_| CliError::new(exit::CONFIG, format!("{name} must be a 64-byte root")));
    let expected_ledger = hash(ledger_root, "ledger-root")?;
    let expected_aux = hash(aux_root, "aux-root")?;
    let nv = crate::wallet::connect(ctx).await?;
    let (snapshot, rows) = super::model_complete::kernel_snapshot(&nv).await?;
    if (snapshot.ledger_root, snapshot.aux_root) != (expected_ledger, expected_aux) {
        return Err(CliError::new(exit::GENERIC, "served kernel inventory differs from the caller's pinned route roots"));
    }
    PalwKernelRouteStateV1::from_served_rows_v1(&snapshot.header, rows.clone(), &expected_ledger, &expected_aux)
        .map_err(|e| CliError::new(exit::GENERIC, format!("kernel route verification: {e}")))?;
    let bytes = borsh::to_vec(&(snapshot.header.clone(), rows))
        .map_err(|e| CliError::new(exit::GENERIC, format!("snapshot encoding: {e}")))?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out)
        .map_err(|e| CliError::new(exit::CONFIG, format!("{}: {e}", out.display())))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", out.display())))?;
    let report = serde_json::json!({
        "schema":"misaka.palw.kernel-snapshot.export.v1",
        "out":out.display().to_string(), "bytes":bytes.len(),
        "servedTipDaa":snapshot.tip_daa,
        "ledgerRoot":expected_ledger.to_string(), "auxRoot":expected_aux.to_string(),
        "verification":"caller-pinned-route-roots",
    });
    if ctx.output == OutputFormat::Json {
        println!("{report}");
    } else {
        println!("wrote {} ({} bytes) for ledger {expected_ledger} / aux {expected_aux}", out.display(), bytes.len());
    }
    Ok(())
}

/// Export the canonical PALW state carriage the pinned block's header commits (op 243) — the `--carriage` file offline
/// `palw-class panel-lifetime-v2` restores — and print the ADR-0043 root to give it as `--state-root`. Every page is held to the
/// pinned header, a contiguous cursor and the exact page length; the whole carriage must decode canonically, restore through the
/// same `into_state_v3` the offline CLI runs, be the parent's post-state, and commit exactly the pinned header's root, and (when
/// given) equal the caller's `--state-root`; any mismatch writes nothing. The file is created exclusively and synced.
pub(crate) async fn carriage(ctx: &Ctx, block: &str, state_root: Option<&str>, out: &std::path::Path) -> CliResult {
    use kaspa_consensus_core::palw_state_carriage_rpc_v1::*;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use std::io::Write;
    let hash =
        |value: &str, name| Hash64::from_str(value).map_err(|_| CliError::new(exit::CONFIG, format!("{name} must be a 64-byte hash")));
    let pinned = hash(block, "block")?;
    let expected = state_root.map(|root| hash(root, "state-root")).transpose()?;
    let bad = |why: String| CliError::new(exit::GENERIC, format!("state carriage: {why}"));
    let nv = crate::wallet::connect(ctx).await?;
    let limit = PALW_CARRIAGE_RPC_MAX_PAGE_BYTES_V1;
    let mut assembler: Option<PalwStateCarriageAssemblerV1> = None;
    loop {
        let offset = assembler.as_ref().map_or(0, |a| a.cursor());
        let response = nv
            .client
            .get_palw_state_carriage(kaspa_rpc_core::GetPalwStateCarriageRequest {
                block_hash: pinned.to_string(),
                offset,
                limit_bytes: limit as u32,
            })
            .await
            .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwStateCarriage: {e}")))?;
        if !response.available {
            return Err(CliError::new(exit::NOT_READY, format!("the node serves no carriage for {pinned}: {}", response.reason)));
        }
        let header = |h: Option<&kaspa_rpc_core::RpcHeader>, name: &str| {
            kaspa_consensus_core::header::Header::try_from(h.ok_or_else(|| bad(format!("the page omits the {name}")))?)
                .map_err(|e| bad(format!("{name}: {e}")))
        };
        let page = PalwStateCarriagePageV1 {
            header: header(response.header.as_ref(), "header")?,
            parent_header: header(response.parent_header.as_ref(), "parent header")?,
            fork_choice: if response.fork_choice_opening.is_empty() {
                None
            } else {
                Some(borsh::from_slice(&response.fork_choice_opening).map_err(|e| bad(format!("fork-choice opening: {e}")))?)
            },
            state_root: response.state_root.parse::<Hash64>().map_err(|e| bad(format!("served state root: {e}")))?,
            total_bytes: response.total_bytes,
            offset: response.offset,
            bytes: response.bytes,
        };
        // Exact page lengths: a node that trickles one byte per page cannot stretch the bounded download.
        let expected_len = (page.total_bytes.saturating_sub(offset)).min(limit as u64);
        if page.bytes.len() as u64 != expected_len {
            return Err(bad(format!("a page carries {} bytes where {expected_len} are due", page.bytes.len())));
        }
        if assembler.is_none() {
            assembler = Some(PalwStateCarriageAssemblerV1::new(pinned, &page, PALW_CARRIAGE_RPC_MAX_TOTAL_BYTES_V1).map_err(bad)?);
        }
        let a = assembler.as_mut().ok_or_else(|| bad("the carriage assembler is missing".into()))?;
        a.push(&page).map_err(bad)?;
        if a.is_complete() {
            break;
        }
    }
    let verified = assembler.ok_or_else(|| bad("no page was assembled".into()))?.finish(&nv.params).map_err(bad)?;
    if let Some(expected) = expected
        && expected != verified.state_root
    {
        return Err(bad(format!(
            "the pinned header's state has ADR-0043 root {}, not the caller's --state-root {expected}",
            verified.state_root
        )));
    }
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(out)
        .map_err(|e| CliError::new(exit::CONFIG, format!("{}: {e}", out.display())))?;
    file.write_all(&verified.carriage)
        .and_then(|()| file.sync_all())
        .map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", out.display())))?;
    let report = serde_json::json!({
        "schema":"misaka.palw.state-carriage.export.v1",
        "out":out.display().to_string(), "bytes":verified.carriage.len(),
        "block":pinned.to_string(), "parent":verified.parent.to_string(), "parentDaa":verified.parent_daa,
        "stateRoot":verified.state_root.to_string(), "committedRoot":verified.committed_root.to_string(),
        "verification":"pinned-header-committed-root",
    });
    if ctx.output == OutputFormat::Json {
        println!("{report}");
    } else {
        println!(
            "wrote {} ({} bytes): post-state of {} (DAA {})",
            out.display(),
            verified.carriage.len(),
            verified.parent,
            verified.parent_daa
        );
        println!("state root (--state-root)  {}", verified.state_root);
        println!("committed by {pinned}  {}", verified.committed_root);
    }
    Ok(())
}
