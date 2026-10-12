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
