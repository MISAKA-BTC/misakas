//! Owning39 payout and redemption evidence from own-node anchored points.
//!
//! Final settlement, pending payout, actual coinbase delivery and an actual
//! UTXO spend are four separate facts (Core `palw_native_redemption_v33`):
//! a pending row is reported as pending; an absent row is never read as a
//! delivery — the bracketing states of the paying block and its real coinbase
//! leaf must be supplied; a spend is proved only by acceptance in a pinned
//! chain block (spend35). Nothing here says PASS or grants any right.
use crate::native_audit_io_v39::{
    AnchoredNativeAuxStreamV39, read_native_aux_stream_anchored_v39, read_native_payout_stream_anchored_v39,
};
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    palw_native_execution_v39::NativeOriginalSourceArchiveV39,
    palw_native_redemption_v33::{
        NativePayoutStatusV39, derive_native_redemption_v39, native_payout_status_v39, verify_native_coinbase_v33,
    },
    palw_state_v2::PalwChainStateV2,
};
use misaka_palw_kernel::hash::Digest;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One collected own-node point (palw-native-audit-stream39 layout).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointPathsV39 {
    pub dir: PathBuf,
    pub child: Hash64,
    pub selected_parent: Hash64,
}
pub fn open_point_v39(root: &Path, p: &PointPathsV39, params: &Params) -> Result<AnchoredNativeAuxStreamV39, String> {
    let dir = root.join(&p.dir);
    read_native_aux_stream_anchored_v39(
        root,
        &dir.join("aux-stream39.jsonl"),
        &dir.join("child-header.json"),
        &dir.join("selected-parent-header.json"),
        &dir.join("envelope39.bin"),
        p.child,
        p.selected_parent,
        params,
    )
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpendPathsV39 {
    pub proof: PathBuf,
    pub chain_header: PathBuf,
    pub chain: Hash64,
    pub selected_parent_header: PathBuf,
    pub selected_parent: Hash64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedemptionSpecV39 {
    pub job: Hash64,
    /// Any point at which the job's settlement/credit is observed.
    pub final_point: PointPathsV39,
    /// The point whose child is the paying block (pre-state of the payment).
    pub before_point: Option<PointPathsV39>,
    /// The point whose selected parent is the paying block (its post-state).
    pub after_point: Option<PointPathsV39>,
    pub coinbase_proof: Option<PathBuf>,
    pub output_index: Option<u32>,
    pub spend: Option<SpendPathsV39>,
}

/// Evaluate the four facts in order and stop at the first one not evidenced.
pub fn audit_native_redemption_v39(
    root: &Path,
    spec: &RedemptionSpecV39,
    params: &Params,
    archive: Option<&dyn NativeOriginalSourceArchiveV39>,
) -> Result<serde_json::Value, String> {
    let hex = crate::runtime_pack::commit::hex;
    let job: Digest = spec.job.as_bytes();
    let fin = open_point_v39(root, &spec.final_point, params)?;
    let final_audit = PalwChainStateV2::native_job_audit_from_stream_v39(fin.auxiliary(), fin.anchor(), params, job, archive)?;
    let payouts =
        read_native_payout_stream_anchored_v39(root, &root.join(&spec.final_point.dir).join("payouts-stream39.jsonl"), &fin)?;
    let status = native_payout_status_v39(&final_audit, &payouts.verified_rows()?)?;
    let mut report = serde_json::json!({
        "schema": "misaka.palw.native-redemption-audit.v39",
        "job": hex(&job),
        "terminal": format!("{:?}", final_audit.terminal()),
        "final_state": fin.anchor().inner().to_string(),
        "single_credit": final_audit.credit().map(|c| serde_json::json!({
            "root": hex(&c.root()), "queue": c.queue.to_string(), "amount": c.amount, "established": c.established})),
        "payout_at_final_state": format!("{status:?}"),
        "delivery": serde_json::Value::Null,
        "spend": serde_json::Value::Null,
        "authority": "measurement record only; not PASS",
    });
    if matches!(status, NativePayoutStatusV39::NotSettled) {
        report["named_status"] = "NOT_SETTLED".into();
        return Ok(report);
    }
    let (Some(bp), Some(ap), Some(cb), Some(index)) = (&spec.before_point, &spec.after_point, &spec.coinbase_proof, spec.output_index)
    else {
        report["named_status"] = match status {
            NativePayoutStatusV39::Pending { .. } => "PENDING_PAYOUT",
            _ => "DELIVERY_WITNESS_REQUIRED",
        }
        .into();
        return Ok(report);
    };
    let before = open_point_v39(root, bp, params)?;
    let after = open_point_v39(root, ap, params)?;
    if ap.selected_parent != bp.child {
        return Err("native39 delivery witness points do not bracket one paying block".into());
    }
    let before_audit = PalwChainStateV2::native_job_audit_from_stream_v39(before.auxiliary(), before.anchor(), params, job, archive)?;
    let before_payouts = read_native_payout_stream_anchored_v39(root, &root.join(&bp.dir).join("payouts-stream39.jsonl"), &before)?;
    let after_payouts = read_native_payout_stream_anchored_v39(root, &root.join(&ap.dir).join("payouts-stream39.jsonl"), &after)?;
    let payment_header = crate::native_audit_io_v32::read_header_v32(&root.join(&ap.dir).join("selected-parent-header.json"))?;
    let proof = crate::native_redemption_v33::read_coinbase_proof_v33(&root.join(cb))?;
    let paid = verify_native_coinbase_v33(&proof, &payment_header, bp.child, params)?;
    let delivery = derive_native_redemption_v39(
        &final_audit,
        &before_audit,
        &before_payouts.verified_rows()?,
        &after_payouts.verified_rows()?,
        &paid,
        index,
    )?;
    report["delivery"] = serde_json::json!({
        "block": delivery.block().to_string(), "outpoint": format!("{}:{}", delivery.outpoint().transaction_id, delivery.outpoint().index),
        "amount": delivery.amount(), "before_state": delivery.before_state().to_string(), "after_state": delivery.after_state().to_string(),
        "root": hex(&delivery.root())});
    let Some(s) = &spec.spend else {
        report["named_status"] = "DELIVERED_SPEND_NOT_EVIDENCED".into();
        return Ok(report);
    };
    let spend_proof = crate::native_spend_v35::read_accepted_spend_proof_v35(&root.join(&s.proof))?;
    let chain = crate::native_audit_io_v32::read_header_v32(&root.join(&s.chain_header))?;
    let sp = crate::native_audit_io_v32::read_header_v32(&root.join(&s.selected_parent_header))?;
    let spent = kaspa_consensus_core::palw_native_spend_v35::verify_native_accepted_spend_v35(
        &delivery,
        &payment_header,
        &spend_proof,
        &chain,
        s.chain,
        &sp,
        s.selected_parent,
        params,
    )?;
    report["spend"] = serde_json::json!({"transaction": spent.spend().to_string(), "chain_block": spent.chain_block().to_string(),
        "chain_daa": spent.chain_daa(), "root": hex(&spent.root())});
    report["named_status"] = "DELIVERED_AND_SPENT_IN_PINNED_CHAIN".into();
    Ok(report)
}
