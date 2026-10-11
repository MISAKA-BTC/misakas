//! Complete-check readback and signed post preparation over actual wRPC.
use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwConformanceEvidenceRequest, GetPalwConformanceEvidenceResponse, GetPalwKernelRowsRequest};
use misaka_palw_sdk::complete_check::VerifiedCompleteCheckV1;
use misaka_palw_sdk::kernel_rows::{KernelRowV1, KernelRowsCollectorV1, KernelRowsLimitsV1, KernelRowsSnapshotV1};
use misaka_palw_sdk::onboarding_chain::EnvelopeSigner;
use std::{path::Path, str::FromStr};

fn bad(message: impl Into<String>) -> CliError {
    CliError::new(exit::GENERIC, message.into())
}
fn unhex(s: &str) -> Result<Vec<u8>, CliError> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return Err(bad("the node served malformed hex"));
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| bad("the node served malformed hex"))).collect()
}

/// Shared bounded op-211 capture for complete-check and source-contract tooling.
/// Caller must authenticate the resulting roots against its trusted chain view.
pub(super) async fn kernel_snapshot(nv: &crate::wallet::NodeView) -> Result<(KernelRowsSnapshotV1, Vec<KernelRowV1>), CliError> {
    let limits = KernelRowsLimitsV1::default();
    let mut collector = KernelRowsCollectorV1::new(limits);
    let mut request = GetPalwKernelRowsRequest::default();
    loop {
        let page = nv
            .client
            .get_palw_kernel_rows(request)
            .await
            .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwKernelRows: {e}")))?;
        if !page.available {
            return Err(CliError::new(exit::NOT_READY, "this node serves no kernel route rows"));
        }
        let bytes = page
            .rows
            .iter()
            .try_fold(page.header.len(), |n, row| n.checked_add(row.key.len()).and_then(|n| n.checked_add(row.row.len())))
            .ok_or_else(|| bad("kernel page size overflow"))?;
        if bytes > limits.maximum_bytes.saturating_mul(2)
            || page.total_rows > limits.maximum_rows
            || page.rows.len() as u64 > page.total_rows
            || page.next_key.len() > limits.maximum_bytes.saturating_mul(2)
        {
            return Err(bad("kernel page exceeds the tooling read limits"));
        }
        let snapshot = KernelRowsSnapshotV1 {
            tip_daa: page.tip_daa,
            ledger_root: page.ledger_root.parse().map_err(|_| bad("malformed ledger root"))?,
            aux_root: page.aux_root.parse().map_err(|_| bad("malformed aux root"))?,
            header: unhex(&page.header)?,
            total_rows: page.total_rows,
        };
        let rows = page
            .rows
            .iter()
            .map(|row| Ok((u8::try_from(row.table).map_err(|_| bad("kernel table exceeds u8"))?, unhex(&row.key)?, unhex(&row.row)?)))
            .collect::<Result<Vec<_>, CliError>>()?;
        let next = if page.more {
            Some((u8::try_from(page.next_table).map_err(|_| bad("kernel continuation table exceeds u8"))?, unhex(&page.next_key)?))
        } else {
            None
        };
        collector.append(snapshot, rows, next).map_err(bad)?;
        let Some((table, key)) = collector.cursor() else {
            break;
        };
        request = GetPalwKernelRowsRequest {
            has_cursor: true,
            after_table: *table as u32,
            after_key: misaka_palw_sdk::runtime_pack::commit::hex(key),
            max_bytes: 0,
        };
    }
    collector.finish().map_err(bad)
}

async fn read(
    ctx: &Ctx,
    nv: &crate::wallet::NodeView,
    r: &GetPalwConformanceEvidenceResponse,
) -> Result<VerifiedCompleteCheckV1, CliError> {
    if r.attempt_row.len() > 2 << 20 || r.program.len() > misaka_palw_sdk::complete_check::COMPLETE_CHECK_MAX_PROGRAM_BYTES_V1 * 2 {
        return Err(bad("the complete-check convenience response exceeds its read limits"));
    }
    let (snapshot, rows) = kernel_snapshot(nv).await?;
    if snapshot.tip_daa != r.tip_daa
        || snapshot.ledger_root.to_string() != r.ledger_root
        || snapshot.aux_root.to_string() != r.aux_root
    {
        return Err(bad("complete-check snapshot moved between op-231 and op-211; read again"));
    }
    let network = kaspa_consensus_core::network::NetworkId::from_str(&ctx.network).map_err(|e| bad(e.to_string()))?;
    let params = crate::wallet::chain_params(ctx, network)?.0;
    let class = Hash64::from_str(&r.class_id).map_err(|_| bad("the node served a malformed class id"))?;
    let verified = VerifiedCompleteCheckV1::from_snapshot(
        &snapshot,
        rows,
        class,
        params.genesis.hash,
        misaka_palw_sdk::onboarding_chain::network_domain_v1(&params),
    )
    .map_err(|e| bad(e.to_string()))?;
    verified.check_rpc_reads(&unhex(&r.attempt_row)?, &unhex(&r.program)?).map_err(|e| bad(e.to_string()))?;
    Ok(verified)
}

pub(crate) async fn verify(
    ctx: &Ctx,
    nv: &crate::wallet::NodeView,
    r: &GetPalwConformanceEvidenceResponse,
    artifact: Option<&Path>,
) -> CliResult {
    let path = artifact.ok_or_else(|| CliError::new(exit::CONFIG, "complete-check verification requires --artifact"))?;
    let verified = read(ctx, nv, r).await?;
    let report = verified.verify_file(path).map_err(|e| bad(e.to_string()))?;
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({
                "schema":"misaka.palw.onboard.complete-verify.v1", "class":r.class_id,
                "ledger_root":r.ledger_root,"aux_root":r.aux_root,"tip_daa":r.tip_daa,
                "inputs_reexecuted":verified.domain.inputs,"leaves_rechecked":verified.domain.leaves,
                "artifact_bytes":verified.domain.artifact_bytes,"work":verified.domain.work,
                "honest_post_id":misaka_palw_sdk::runtime_pack::commit::hex(&report.honest_post_id),
                "chain_post_id":report.chain_post_id.map(|id|misaka_palw_sdk::runtime_pack::commit::hex(&id)),
                "honest_passes":report.honest_passes,"chain_says_passed":report.chain_says_passed,
                "agrees":report.agrees,"why":report.why,
            })
        );
    } else {
        println!(
            "complete check   {} inputs, {} leaves, {} artifact bytes",
            verified.domain.inputs, verified.domain.leaves, verified.domain.artifact_bytes
        );
        println!("chain agrees     {} — {}", report.agrees, report.why);
    }
    if !report.agrees {
        return Err(bad("the public complete-check verdict disagrees with the exhaustive execution"));
    }
    Ok(())
}

struct Signer(kaspa_pq_validator_core::ValidatorKey);
impl EnvelopeSigner for Signer {
    fn public_key(&self) -> Vec<u8> {
        self.0.public_key().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.0.sign_with_context(message, context).to_vec())
    }
}

/// Prepare the signed object locally. Submission remains the existing
/// `palw submit-object` path, with its normal fee and inclusion checks.
pub(crate) async fn post(ctx: &Ctx, class: &str, artifact: &Path, bond: &str, key: &crate::keys::KeySource, out: &Path) -> CliResult {
    let nv = crate::wallet::connect(ctx).await?;
    let r = nv
        .client
        .get_palw_conformance_evidence(GetPalwConformanceEvidenceRequest { class_id: class.to_string() })
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwConformanceEvidence: {e}")))?;
    if !r.found || r.class_id != class {
        return Err(CliError::new(exit::NOT_READY, "no conformance attempt for the requested class"));
    }
    let verified = read(ctx, &nv, &r).await?;
    let (txid, index) = bond.split_once(':').ok_or_else(|| bad("bond must be <txid>:<index>"))?;
    let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
        txid.parse().map_err(|_| bad("invalid bond transaction id"))?,
        index.parse().map_err(|_| bad("invalid bond index"))?,
    ));
    if bond != verified.registrant() {
        return Err(bad("the requested bond is not this rooted attempt's binder"));
    }
    let object = verified.signed_post_file(artifact, bond, &Signer(key.load_key()?)).map_err(|e| bad(e.to_string()))?;
    let bytes = borsh::to_vec(&object).map_err(|e| bad(e.to_string()))?;
    std::fs::write(out, &bytes).map_err(|e| CliError::new(exit::HOST, format!("{}: {e}", out.display())))?;
    if ctx.output == OutputFormat::Json {
        println!(
            "{}",
            serde_json::json!({"schema":"misaka.palw.onboard.complete-post.v1","class":class,"out":out.display().to_string(),"bytes":bytes.len(),"submitted":false})
        );
    } else {
        println!("wrote {} ({} bytes); submit with misaka palw submit-object --object {}", out.display(), bytes.len(), out.display());
    }
    Ok(())
}
