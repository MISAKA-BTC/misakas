//! Sign and fund Batch Court controls for their rooted BCT1 service slots.
use crate::{CliError, CliResult, exit, keys::KeySource, node::Ctx, wallet::NodeView};
use kaspa_consensus_core::{
    Hash64,
    palw_batch_court_v1::{
        BatchCourtWindowV1, BatchDemandV1, BatchRangeAuthenticationV1, BatchRangeV1, MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1,
        MAX_BATCH_RANGES_V1,
        carriage::{
            BATCH_COURT_CARRIER_MAGIC_V1, BATCH_DEMAND_SIGNATURE_CONTEXT_V1, BATCH_NUMERICAL_SIGNATURE_CONTEXT_V1, BatchCourtWireV1,
            BatchMaterialControlV1, BatchNumericalTerminalV1, BatchStopSigningV1, MAX_BATCH_STOP_EOG_OPENING_BYTES_V1,
            SignedBatchDemandV1, SignedNumericalBatchVerificationV1,
        },
    },
    palw_court_shared_material_intake_v1::{
        BATCH_STOP_ORDINAL_V1, MATERIAL_BATCH_CONTEXT_V1, MATERIAL_BATCH_VERSION, MaterialActionV1, MaterialTargetV1,
        SignedMaterialIntakeV1, batch_stop_full_ids_range_v1, material_batch_carrier_fee_v1, material_batch_draft_id_v1,
    },
    palw_freeprompt_v3::PalwFpStopReasonV3,
    palw_state_v2::PalwBondKeyV2,
    palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1,
    palw_tokenizer_rendering_v2::{
        MAX_REGISTERED_TOKENIZER_RAW_BYTES_V1, PalwTokenizerRenderingStatementV2, build_registered_tokenizer_eog_opening_v1,
    },
};
use kaspa_rpc_core::{GetPalwCourtStateRequest, api::rpc::RpcApi};
use serde::Deserialize;
use std::{io::Read, path::Path};

mod plain_output;
pub(crate) use plain_output::{convict_plain_output_v2, prepare_plain_output_v2, verify_plain_output_v2};

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ResponseStep {
    Begin,
    Part,
    Finish,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartSlot {
    offset: u64,
    bytes: u64,
    assigned_daa: u64,
    accepted_daa: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseQuote {
    request_root: String,
    draft_id: String,
    total_bytes: u64,
    begin_daa: u64,
    finish_daa: u64,
    draft_expires_daa: u64,
    retained_submitter: Option<String>,
    retained_body_hash: Option<String>,
    material_available: bool,
    parts: Vec<PartSlot>,
}

fn refused(reason: impl Into<String>) -> CliError {
    CliError::new(exit::GENERIC, reason.into())
}

async fn read_quote(nv: &NodeView, request: GetPalwCourtStateRequest) -> Result<(u64, String), CliError> {
    let answer = nv.client.get_palw_court_state(request).await.map_err(|e| CliError::connection(e.to_string()))?;
    if !answer.available || !answer.found {
        return Err(refused("the node did not return a Batch Court quote"));
    }
    Ok((answer.now_daa, answer.view_json))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenRange {
    id: String,
    bytes: u32,
    authentication: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenQuote {
    wire_version: u16,
    scope_id: String,
    signer: String,
    request_root: String,
    case_id: String,
    original_accepted_daa: String,
    notice_daa: String,
    collect_until_daa: String,
    material_until_daa: String,
    opened_daa: String,
    response_deadline_daa: String,
    part_bytes: u32,
    maximum_bytes: u64,
    ranges: Vec<OpenRange>,
}

impl OpenQuote {
    fn checked_open(
        self,
        claim: Hash64,
        notice: Hash64,
        ordinal: u64,
        signer: PalwBondKeyV2,
        ranges: Vec<BatchRangeV1>,
    ) -> Result<SignedBatchDemandV1, CliError> {
        use crate::palw_user::parse_hash;
        let daa = |value: &str| value.parse::<u64>().map_err(|_| refused("invalid DAA in numerical Open quote"));
        if self.wire_version != 1
            || self.signer != bond_text(signer)
            || self.part_bytes != MAX_BATCH_PART_BYTES_V1
            || self.ranges.len() != ranges.len()
            || self.ranges.iter().zip(&ranges).any(|(quoted, expected)| {
                quoted.id != expected.id.to_string() || quoted.bytes != expected.bytes || quoted.authentication != "whole-range"
            })
            || self.maximum_bytes != ranges.iter().map(|range| u64::from(range.bytes)).sum::<u64>()
        {
            return Err(refused("numerical Open quote substituted the signer or requested ranges"));
        }
        let demand = BatchDemandV1 {
            case_id: parse_hash("case", &self.case_id)?,
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: daa(&self.original_accepted_daa)?,
                challenge_opened_daa: daa(&self.notice_daa)?,
                collect_until_daa: daa(&self.collect_until_daa)?,
                material_until_daa: daa(&self.material_until_daa)?,
            },
            opened_daa: daa(&self.opened_daa)?,
            response_deadline_daa: daa(&self.response_deadline_daa)?,
            maximum_bytes: self.maximum_bytes,
            part_bytes: self.part_bytes,
            ranges,
        };
        let root = demand.checked_request_root_v1().map_err(|why| refused(format!("numerical Open demand: {why:?}")))?;
        if root != parse_hash("request root", &self.request_root)? {
            return Err(refused("numerical Open request root differs from the locally derived demand"));
        }
        Ok(SignedBatchDemandV1 {
            version: self.wire_version,
            claim,
            scope_id: parse_hash("scope", &self.scope_id)?,
            ordinal,
            notice_id: notice,
            signer,
            demand,
            signature: Vec::new(),
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn open(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    notice: &str,
    leaf: Option<u64>,
    ordinal: Option<u64>,
    entries: &[String],
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    use crate::palw_user::parse_hash;
    let claim = parse_hash("claim", claim)?;
    let notice = parse_hash("notice", notice)?;
    let (ordinal, kind) = match (leaf, ordinal) {
        (None, None) if entries.is_empty() => (BATCH_STOP_ORDINAL_V1, "palwBatchStopFullIdsOpen"),
        (Some(_), Some(ordinal)) if ordinal != BATCH_STOP_ORDINAL_V1 => (ordinal, "palwNumericalBatchOpen"),
        _ => return Err(refused("use --stop-full-ids or a numerical leaf, ordinal and ranges")),
    };
    if leaf.is_some() && !(1..=MAX_BATCH_RANGES_V1).contains(&entries.len()) {
        return Err(refused("provide between 1 and 64 --range values"));
    }
    let mut material = Vec::with_capacity(entries.len());
    for entry in entries {
        let (target, bytes) = entry.split_once('=').ok_or_else(|| refused("--range must be target=bytes"))?;
        let parsed: MaterialTargetV1 = target.parse().map_err(refused)?;
        let bytes = bytes
            .parse::<u32>()
            .ok()
            .filter(|&bytes| bytes != 0)
            .ok_or_else(|| refused("--range byte count must be a positive u32"))?;
        let range =
            BatchRangeV1 { id: parsed.id_v1(claim).map_err(refused)?, bytes, authentication: BatchRangeAuthenticationV1::WholeRange };
        material.push((range, format!("{target}={bytes}")));
    }
    material.sort_unstable_by_key(|(range, _)| range.id);
    if material.windows(2).any(|pair| pair[0].0.id == pair[1].0.id) {
        return Err(refused("duplicate --range target"));
    }
    let (mut ranges, lengths): (Vec<_>, Vec<_>) = material.into_iter().unzip();
    let batch_open = match leaf {
        Some(leaf) => format!("{notice}/{leaf}/{}", lengths.join(",")),
        None => format!("{notice}/stop-full-ids"),
    };
    let request = GetPalwCourtStateRequest {
        claim_hex: claim.to_string(),
        batch_ordinal: ordinal.to_string(),
        batch_open,
        ..Default::default()
    };
    let (nv, key, signer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let (now, raw) = read_quote(&nv, request.clone()).await?;
    let doc: serde_json::Value = serde_json::from_str(&raw).map_err(|e| refused(e.to_string()))?;
    if doc["version"] != 1
        || doc["kind"] != kind
        || doc["claimId"] != request.claim_hex
        || doc["noticeId"] != notice.to_string()
        || doc["ordinal"] != request.batch_ordinal
        || leaf.is_some_and(|leaf| doc["leaf"] != leaf.to_string())
        || doc["nowDaa"] != now
        || doc["tipBlock"].as_str().and_then(|tip| tip.parse::<Hash64>().ok()).is_none()
    {
        return Err(refused("the node returned a different Batch Open case"));
    }
    if let Some(error) = doc["error"].as_str() {
        return Err(refused(error));
    }
    if leaf.is_none() {
        let count = doc["fullIdsCount"]
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
            .ok_or_else(|| refused("the Batch Stop quote omitted its generated ID count"))?;
        ranges.push(batch_stop_full_ids_range_v1(claim, count).map_err(refused)?);
    }
    let quote: OpenQuote = serde_json::from_value(doc["open"].clone()).map_err(|e| refused(e.to_string()))?;
    let mut open = quote.checked_open(claim, notice, ordinal, signer, ranges)?;
    let slot = open.demand.opened_daa;
    if submit && now != slot {
        return Err(refused(format!("Batch Open is booked at DAA {slot}, current fold DAA is {now}")));
    }
    let message = open.signing_message_v1(crate::bond::network_domain(&nv)).map_err(refused)?;
    open.signature = key.sign_with_context(message.as_byte_slice(), BATCH_DEMAND_SIGNATURE_CONTEXT_V1).to_vec();
    let (tx, fee, _) = fund_control(&nv, &key, &BatchCourtWireV1::Open(open), maximum_fee).await?;
    if submit {
        if read_quote(&nv, request).await? != (now, raw) {
            return Err(refused("the Batch Open reservation or selected tip changed before submission"));
        }
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|e| refused(e.to_string()))?;
    }
    println!(
        "{}",
        serde_json::json!({ "submitted": submit, "txid": tx.id().to_string(), "assignedDaa": slot,
        "observedDaa": now, "feeSompi": fee, "requestRoot": doc["open"]["requestRoot"] })
    );
    Ok(())
}

impl ResponseQuote {
    fn check_material_needed(&self, step: ResponseStep) -> CliResult {
        if self.material_available && !matches!(step, ResponseStep::Finish) {
            return Err(refused("the original already has authenticated material for this target"));
        }
        Ok(())
    }

    fn action(
        &self,
        step: ResponseStep,
        offset: Option<u64>,
        draft: Hash64,
        signer: &str,
        body_hash: Hash64,
        body: &[u8],
    ) -> Result<(u64, MaterialActionV1), CliError> {
        let part_bytes = u64::from(MAX_BATCH_PART_BYTES_V1);
        if self.total_bytes != body.len() as u64
            || self.finish_daa >= self.draft_expires_daa
            || self.parts.len() as u64 != self.total_bytes.div_ceil(part_bytes)
            || self.parts.iter().enumerate().any(|(i, part)| {
                part.offset != i as u64 * part_bytes
                    || part.bytes != part_bytes.min(self.total_bytes.saturating_sub(part.offset))
                    || !(self.begin_daa < part.assigned_daa && part.assigned_daa < self.finish_daa)
            })
        {
            return Err(refused("the response reservation has inconsistent part geometry"));
        }
        if matches!(step, ResponseStep::Begin) {
            if offset.is_some()
                || self.retained_submitter.is_some()
                || self.retained_body_hash.is_some()
                || self.parts.iter().any(|part| part.accepted_daa.is_some())
            {
                return Err(refused("Begin requires an unopened draft and no --offset"));
            }
            return Ok((self.begin_daa, MaterialActionV1::Begin { total_bytes: self.total_bytes, body_hash }));
        }
        let body_hash_text = body_hash.to_string();
        if self.retained_submitter.as_deref() != Some(signer) || self.retained_body_hash.as_deref() != Some(body_hash_text.as_str()) {
            return Err(refused("the accepted Begin does not match the bond and body"));
        }
        match step {
            ResponseStep::Part => {
                let part = self
                    .parts
                    .iter()
                    .find(|part| Some(part.offset) == offset)
                    .ok_or_else(|| refused("--offset must name a reserved Part"))?;
                if part.accepted_daa.is_some() {
                    return Err(refused("the Part is already accepted"));
                }
                let bytes = body[part.offset as usize..(part.offset + part.bytes) as usize].to_vec();
                Ok((part.assigned_daa, MaterialActionV1::Part { draft_id: draft, offset: part.offset, bytes }))
            }
            ResponseStep::Finish if offset.is_none() && self.parts.iter().all(|part| part.accepted_daa.is_some()) => {
                Ok((self.finish_daa, MaterialActionV1::Finish { draft_id: draft }))
            }
            ResponseStep::Finish => Err(refused("Finish requires every Part to be accepted and no --offset")),
            ResponseStep::Begin => unreachable!(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn respond(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    ordinal: u64,
    target: &str,
    step: ResponseStep,
    offset: Option<u64>,
    body_path: &Path,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    let claim = crate::palw_user::parse_hash("--claim", claim)?;
    let material: MaterialTargetV1 = target.parse().map_err(refused)?;
    let (nv, key, signer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let request = GetPalwCourtStateRequest {
        claim_hex: claim.to_string(),
        batch_ordinal: ordinal.to_string(),
        batch_target: target.to_owned(),
        ..Default::default()
    };
    let (now, raw) = read_quote(&nv, request.clone()).await?;
    let doc: serde_json::Value = serde_json::from_str(&raw).map_err(|e| refused(e.to_string()))?;
    let target_id = material.id_v1(claim).map_err(refused)?;
    if doc["version"] != 1
        || doc["kind"] != "palwBatchResponse"
        || doc["claimId"] != claim.to_string()
        || doc["ordinal"] != request.batch_ordinal
        || doc["targetId"] != target_id.to_string()
        || doc["nowDaa"] != now
        || doc["tipBlock"].as_str().and_then(|tip| tip.parse::<Hash64>().ok()).is_none()
    {
        return Err(refused("the node returned a different Batch response reservation"));
    }
    if let Some(error) = doc["error"].as_str() {
        return Err(refused(error));
    }
    let quote: ResponseQuote = serde_json::from_value(doc["response"].clone()).map_err(|e| refused(e.to_string()))?;
    quote.check_material_needed(step)?;
    let maximum = u64::from(MAX_BATCH_PART_BYTES_V1) * MAX_BATCH_PARTS_V1 as u64;
    if !(1..=maximum).contains(&quote.total_bytes) {
        return Err(refused("the quoted material exceeds the Batch body bound"));
    }
    let request_root = crate::palw_user::parse_hash("request root", &quote.request_root)?;
    let draft = material_batch_draft_id_v1(request_root, claim, material).map_err(refused)?;
    if quote.draft_id != draft.to_string() {
        return Err(refused("the response reservation has a different draft identity"));
    }
    let mut body = Vec::new();
    std::fs::File::open(body_path)
        .and_then(|file| file.take(quote.total_bytes + 1).read_to_end(&mut body))
        .map_err(|e| refused(format!("{}: {e}", body_path.display())))?;
    let body_hash = kaspa_consensus_core::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
    let (slot, action) = quote.action(step, offset, draft, &bond_text(signer), body_hash, &body)?;
    if now > slot {
        return Err(refused(format!("Batch response slot {slot} has passed at DAA {now}")));
    }
    if submit && now != slot {
        return Err(refused(format!("Batch response is booked at DAA {slot}, current fold DAA is {now}")));
    }
    // Scope must come from the same accepted view as the request root.
    let scope = doc["scopeId"].as_str().ok_or_else(|| refused("the Batch response is missing its scope"))?;
    let mut intake = SignedMaterialIntakeV1 {
        version: MATERIAL_BATCH_VERSION,
        claim,
        scope_id: crate::palw_user::parse_hash("scope", scope)?,
        target: material,
        submitter: signer,
        action,
        signature: vec![],
    };
    let message = intake.batch_signing_message_v1(crate::bond::network_domain(&nv), request_root).map_err(refused)?;
    intake.signature = key.sign_with_context(message.as_byte_slice(), MATERIAL_BATCH_CONTEXT_V1).to_vec();
    let wire = BatchCourtWireV1::Respond(BatchMaterialControlV1 { demand_ordinal: ordinal, intake });
    let (tx, fee, rent) = fund_control(&nv, &key, &wire, maximum_fee).await?;
    if submit {
        if read_quote(&nv, request).await? != (now, raw) {
            return Err(refused("the Batch response reservation or selected tip changed before submission"));
        }
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|e| refused(e.to_string()))?;
    }
    println!(
        "{}",
        serde_json::json!({ "submitted": submit, "txid": tx.id().to_string(), "assignedDaa": slot,
        "observedDaa": now, "feeSompi": fee, "storageRentSompi": rent, "draftId": draft.to_string() })
    );
    Ok(())
}

pub(crate) async fn verify_numerical(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    notice: &str,
    leaf: u64,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    use crate::palw_user::parse_hash;
    let claim = parse_hash("claim", claim)?;
    let notice = parse_hash("notice", notice)?;
    let request =
        GetPalwCourtStateRequest { claim_hex: claim.to_string(), batch_open: "verify-numerical".into(), ..Default::default() };
    let (nv, key, filer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let network = crate::bond::network_domain(&nv);
    let (now, raw) = read_quote(&nv, request.clone()).await?;
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|err| refused(err.to_string()))?;
    let (mut signed, slot) = checked_numerical_verification_quote(&view, claim, notice, leaf, filer, network, now)?;
    if now > slot || (submit && now != slot) {
        return Err(refused(format!("numerical verification is booked at DAA {slot}, current fold DAA is {now}")));
    }
    let message = signed.signing_message_v1(network).map_err(refused)?;
    signed.signature = key.sign_with_context(message.as_byte_slice(), BATCH_NUMERICAL_SIGNATURE_CONTEXT_V1).to_vec();
    let wire = BatchCourtWireV1::VerifyNumerical(signed);
    let (tx, fee, _) = fund_control(&nv, &key, &wire, maximum_fee).await?;
    if submit {
        if read_quote(&nv, request).await? != (now, raw) {
            return Err(refused("the numerical verification reservation or selected tip changed before submission"));
        }
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|err| refused(err.to_string()))?;
    }
    println!(
        "{}",
        serde_json::json!({
            "submitted": submit, "txid": tx.id().to_string(), "claimId": claim.to_string(), "assignedDaa": slot,
            "observedDaa": now, "feeSompi": fee, "noticeId": notice.to_string(), "leaf": leaf.to_string(),
        })
    );
    Ok(())
}

fn checked_numerical_verification_quote(
    view: &serde_json::Value,
    claim: Hash64,
    notice: Hash64,
    leaf: u64,
    filer: PalwBondKeyV2,
    network: Hash64,
    now: u64,
) -> Result<(SignedNumericalBatchVerificationV1, u64), CliError> {
    use crate::palw_user::parse_hash;
    if view["version"] != 1
        || view["kind"] != "palwBatchVerifyNumerical"
        || view["claimId"] != claim.to_string()
        || view["nowDaa"] != now
        || view["tipBlock"].as_str().and_then(|tip| tip.parse::<Hash64>().ok()).is_none()
    {
        return Err(refused("the node returned a different numerical verification case"));
    }
    if let Some(error) = view["error"].as_str() {
        return Err(refused(error));
    }
    let quote = &view["verify"];
    if quote["wireVersion"] != 1
        || quote["noticeId"] != notice.to_string()
        || quote["leaf"] != leaf.to_string()
        || quote["signer"] != bond_text(filer)
        || quote["networkDomain"] != network.to_string()
    {
        return Err(refused("the numerical verification quote substituted the notice, leaf, signer or network"));
    }
    let read_hash =
        |key: &str| parse_hash(key, quote[key].as_str().ok_or_else(|| refused("the numerical verification quote omitted an ID"))?);
    let slot = quote["assignedDaa"].as_u64().ok_or_else(|| refused("the numerical verification quote omitted the booked DAA"))?;
    Ok((
        SignedNumericalBatchVerificationV1 {
            version: 1,
            claim,
            scope: read_hash("scopeId")?,
            notice,
            leaf,
            policy: read_hash("policyId")?,
            filer,
            signature: vec![],
        },
        slot,
    ))
}

pub(crate) async fn convict_numerical(
    ctx: &Ctx,
    key_source: &KeySource,
    claim: &str,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    use crate::palw_user::parse_hash;
    let claim = parse_hash("claim", claim)?;
    let request = GetPalwCourtStateRequest { claim_hex: claim.to_string(), ..Default::default() };
    let nv = crate::wallet::connect(ctx).await?;
    let (now, raw) = read_quote(&nv, request.clone()).await?;
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|err| refused(err.to_string()))?;
    let (control, fault, slot) = checked_numerical_terminal_quote(&view, claim, now, submit)?;
    let key = key_source.load_key()?;
    let wire = BatchCourtWireV1::ConvictNumerical(control);
    let (tx, fee, _) = fund_control(&nv, &key, &wire, maximum_fee).await?;
    if submit {
        if read_quote(&nv, request).await? != (now, raw) {
            return Err(refused("the numerical finding or selected tip changed before submission"));
        }
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|err| refused(err.to_string()))?;
    }
    println!(
        "{}",
        serde_json::json!({
            "submitted": submit, "txid": tx.id().to_string(), "claimId": claim.to_string(), "assignedDaa": slot,
            "observedDaa": now, "feeSompi": fee, "receiptDigest": control.receipt_digest.to_string(), "faultEvidenceId": fault.to_string(),
        })
    );
    Ok(())
}

fn checked_numerical_terminal_quote(
    view: &serde_json::Value,
    claim: Hash64,
    now: u64,
    submit: bool,
) -> Result<(BatchNumericalTerminalV1, Hash64, u64), CliError> {
    use crate::palw_user::parse_hash;
    let finding = &view["batchNumericalFinding"];
    if view["version"] != 1
        || view["kind"] != "palwCourtState"
        || view["claimId"] != claim.to_string()
        || view["nowDaa"] != now
        || finding["tipBlock"].as_str().and_then(|tip| tip.parse::<Hash64>().ok()).is_none()
    {
        return Err(refused("the node did not return a selected-branch numerical finding for this claim"));
    }
    let fault = parse_hash(
        "fault evidence",
        finding["faultEvidenceId"].as_str().ok_or_else(|| refused("this numerical finding contains no fault"))?,
    )?;
    let receipt_digest = parse_hash(
        "receipt",
        finding["receiptDigest"].as_str().ok_or_else(|| refused("the node omitted the numerical receipt digest"))?,
    )?;
    let slot = finding["terminalDaa"].as_u64().ok_or_else(|| refused("the claim has no booked numerical terminal"))?;
    if fault == Hash64::default()
        || receipt_digest == Hash64::default()
        || finding["faultMaterialAvailable"] != true
        || finding["verificationDaa"].as_u64().is_none_or(|daa| daa >= slot)
        || finding["materialUntilDaa"].as_u64().is_none_or(|daa| daa <= slot)
    {
        return Err(refused("the numerical finding or terminal slot is invalid"));
    }
    if now > slot || (submit && now != slot) {
        return Err(refused(format!("numerical terminal is booked at DAA {slot}, current fold DAA is {now}")));
    }
    Ok((BatchNumericalTerminalV1 { claim, receipt_digest }, fault, slot))
}

async fn fund_control(
    nv: &NodeView,
    key: &kaspa_pq_validator_core::ValidatorKey,
    wire: &BatchCourtWireV1,
    maximum_fee: Option<u64>,
) -> Result<(kaspa_consensus_core::tx::Transaction, u64, u64), CliError> {
    let addr = key.funding_address(nv.params.prefix());
    let (outpoint, entry) = crate::palw_fp::lifecycle_candidates_v1(nv, &addr)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| refused("no mature unspent UTXO funds the Batch control"))?;
    let floor = kaspa_pq_validator_core::PALW_TX_FEE_FLOOR_SOMPI;
    let probe = key.build_palw_batch_court_tx(wire, outpoint, &entry, floor).map_err(refused)?;
    let rent = material_batch_carrier_fee_v1(&probe, 0).map_err(refused)?.required_storage_rent;
    let params = &nv.params;
    let mass = kaspa_consensus_core::mass::MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    )
    .calc_non_contextual_masses(&probe)
    .compute_mass;
    let fee = kaspa_pq_validator_core::relay_fee_for_compute_mass(mass).max(floor).max(rent);
    if maximum_fee.is_some_and(|limit| fee > limit) {
        return Err(refused(format!("Batch control fee {fee} exceeds the maximum fee")));
    }
    let tx = key.build_palw_batch_court_tx(wire, outpoint, &entry, fee).map_err(refused)?;
    Ok((tx, fee, rent))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopQuote {
    assigned_daa: String,
    scope_id: String,
    policy_id: String,
    network_domain: String,
    semantics_id: String,
    signer: String,
    position: u32,
    stop_reason: String,
    table_root: String,
    vocab: u32,
    tokenizer: Option<StopTokenizerQuote>,
    eog_opening_bytes: u64,
    carrier_bytes: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopTokenizerQuote {
    id: String,
    raw_bytes: u64,
    rendering_rule: u8,
    eog_count: u32,
}

pub(crate) enum StopEogSource<'a> {
    Opening(&'a Path),
    Tokenizer { path: &'a Path, eog_ids: Vec<u32> },
}

fn read_exact_stop_file(path: &Path, expected_bytes: u64) -> Result<Vec<u8>, CliError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(expected_bytes + 1).read_to_end(&mut bytes))
        .map_err(|why| refused(format!("{}: {why}", path.display())))?;
    if bytes.len() as u64 != expected_bytes {
        return Err(refused(format!("{}: file length differs from the Batch Stop quote", path.display())));
    }
    Ok(bytes)
}

impl StopQuote {
    fn load_eog(&self, source: StopEogSource<'_>, table_root: Hash64) -> Result<Vec<u8>, CliError> {
        if !(1..=MAX_BATCH_STOP_EOG_OPENING_BYTES_V1 as u64).contains(&self.eog_opening_bytes) {
            return Err(refused("the quoted EOG opening exceeds the Batch Stop bound"));
        }
        match source {
            StopEogSource::Opening(path) => read_exact_stop_file(path, self.eog_opening_bytes),
            StopEogSource::Tokenizer { path, eog_ids } => {
                let tokenizer = self.tokenizer.as_ref().ok_or_else(|| refused("the node did not quote the tokenizer registration"))?;
                let rule = PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1;
                if tokenizer.rendering_rule != rule as u8
                    || !(1..=MAX_REGISTERED_TOKENIZER_RAW_BYTES_V1).contains(&tokenizer.raw_bytes)
                    || eog_ids.len() != tokenizer.eog_count as usize
                {
                    return Err(refused("the tokenizer rule, file length or EOG count differs from the supported registration"));
                }
                let statement = PalwTokenizerRenderingStatementV2 {
                    tokenizer_id: crate::palw_user::parse_hash("tokenizer", &tokenizer.id)?,
                    exact_raw_bytes: tokenizer.raw_bytes,
                    table_root,
                    vocab: self.vocab,
                    rule,
                };
                let raw = read_exact_stop_file(path, tokenizer.raw_bytes)?;
                let eog = build_registered_tokenizer_eog_opening_v1(statement, &raw, eog_ids).map_err(refused)?;
                let bytes = borsh::to_vec(&eog).map_err(|why| refused(why.to_string()))?;
                if bytes.len() as u64 != self.eog_opening_bytes {
                    return Err(refused("the derived EOG opening differs from its reserved size"));
                }
                Ok(bytes)
            }
        }
    }
}

pub(crate) async fn stop(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    eog_source: StopEogSource<'_>,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    use crate::palw_user::parse_hash;
    let claim = parse_hash("claim", claim)?;
    let request = GetPalwCourtStateRequest { claim_hex: claim.to_string(), batch_open: "stop".into(), ..Default::default() };
    let (nv, key, filer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let (now, raw) = read_quote(&nv, request.clone()).await?;
    let doc: serde_json::Value = serde_json::from_str(&raw).map_err(|why| refused(why.to_string()))?;
    if doc["version"] != 1
        || doc["kind"] != "palwBatchStop"
        || doc["claimId"] != request.claim_hex
        || doc["nowDaa"] != now
        || doc["tipBlock"].as_str().and_then(|tip| tip.parse::<Hash64>().ok()).is_none()
    {
        return Err(refused("the node returned a different Batch Stop case"));
    }
    if let Some(error) = doc["error"].as_str() {
        return Err(refused(error));
    }
    let quote: StopQuote = serde_json::from_value(doc["stop"].clone()).map_err(|why| refused(why.to_string()))?;
    let network = crate::bond::network_domain(&nv);
    let assigned_daa = quote.assigned_daa.parse::<u64>().map_err(|_| refused("invalid Batch Stop DAA"))?;
    if quote.signer != bond_text(filer) || quote.network_domain != network.to_string() {
        return Err(refused("the Batch Stop signer or network differs from the quote"));
    }
    if now > assigned_daa || submit && now != assigned_daa {
        return Err(refused(format!("Batch Stop is booked at DAA {assigned_daa}, current fold DAA is {now}")));
    }
    let table_root = parse_hash("table root", &quote.table_root)?;
    let eog = quote.load_eog(eog_source, table_root)?;
    let signing = BatchStopSigningV1 {
        claim,
        filer,
        scope_id: parse_hash("scope", &quote.scope_id)?,
        policy_id: parse_hash("policy", &quote.policy_id)?,
        semantics_id: parse_hash("semantics", &quote.semantics_id)?,
        position: quote.position,
        stop_reason: match quote.stop_reason.as_str() {
            "exact-budget-reached" => PalwFpStopReasonV3::ExactBudgetReached,
            "end-of-generation" => PalwFpStopReasonV3::EndOfGeneration,
            _ => return Err(refused("unknown native stop reason")),
        },
        table_root,
        vocab: quote.vocab,
    };
    let wire = signing
        .build_v1(network, &eog, |message, context| key.sign_with_context(message.as_byte_slice(), context).to_vec())
        .map_err(refused)?;
    let wire_len = borsh::object_length(&wire).map_err(|why| refused(why.to_string()))? + BATCH_COURT_CARRIER_MAGIC_V1.len();
    if wire_len as u64 != quote.carrier_bytes {
        return Err(refused("signed Batch Stop length differs from its reservation"));
    }
    let (tx, fee, rent) = fund_control(&nv, &key, &wire, maximum_fee).await?;
    if submit {
        if read_quote(&nv, request).await? != (now, raw) {
            return Err(refused("the Batch Stop reservation or selected tip changed before submission"));
        }
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|why| refused(why.to_string()))?;
    }
    println!(
        "{}",
        serde_json::json!({ "submitted": submit, "txid": tx.id().to_string(), "assignedDaa": assigned_daa,
        "observedDaa": now, "feeSompi": fee, "storageRentSompi": rent, "claimId": claim.to_string() })
    );
    Ok(())
}

fn bond_text(bond: PalwBondKeyV2) -> String {
    kaspa_consensus_core::palw_user_views_v1::palw_bond_key_text_v1(&bond)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numerical_verification_quote_binds_notice_leaf_signer_and_network() {
        let h = Hash64::from_u64_word;
        let filer = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(6), 1));
        let mut view = serde_json::json!({
            "version": 1, "kind": "palwBatchVerifyNumerical", "claimId": h(1).to_string(), "nowDaa": 20,
            "tipBlock": h(2).to_string(),
            "verify": {
                "wireVersion": 1, "scopeId": h(3).to_string(), "noticeId": h(4).to_string(), "leaf": u64::MAX.to_string(),
                "policyId": h(5).to_string(), "signer": bond_text(filer), "networkDomain": h(7).to_string(), "assignedDaa": 21,
            }
        });
        let (intent, slot) = checked_numerical_verification_quote(&view, h(1), h(4), u64::MAX, filer, h(7), 20).unwrap();
        assert_eq!((intent.notice, intent.leaf, intent.policy, slot), (h(4), u64::MAX, h(5), 21));
        assert!(checked_numerical_verification_quote(&view, h(1), h(9), u64::MAX, filer, h(7), 20).is_err());
        assert!(checked_numerical_verification_quote(&view, h(1), h(4), 0, filer, h(7), 20).is_err());
        assert!(checked_numerical_verification_quote(&view, h(1), h(4), u64::MAX, filer, h(9), 20).is_err());
        view["verify"]["signer"] = format!("{}:0", h(6)).into();
        assert!(checked_numerical_verification_quote(&view, h(1), h(4), u64::MAX, filer, h(7), 20).is_err());
    }

    #[test]
    fn numerical_terminal_quote_requires_the_exact_claim_fault_and_slot() {
        let h = Hash64::from_u64_word;
        let mut view = serde_json::json!({
            "version": 1, "kind": "palwCourtState", "claimId": h(1).to_string(), "nowDaa": 20,
            "batchNumericalFinding": {
                "tipBlock": h(2).to_string(), "faultEvidenceId": h(3).to_string(), "receiptDigest": h(4).to_string(),
                "verificationDaa": 15, "terminalDaa": 21, "materialUntilDaa": 22, "faultMaterialAvailable": true,
            }
        });
        let expected = (BatchNumericalTerminalV1 { claim: h(1), receipt_digest: h(4) }, h(3), 21);
        assert_eq!(checked_numerical_terminal_quote(&view, h(1), 20, false).unwrap(), expected);
        assert!(checked_numerical_terminal_quote(&view, h(1), 20, true).is_err());
        view["nowDaa"] = 21.into();
        assert_eq!(checked_numerical_terminal_quote(&view, h(1), 21, true).unwrap(), expected);
        assert!(checked_numerical_terminal_quote(&view, h(5), 21, true).is_err());
        view["batchNumericalFinding"]["faultMaterialAvailable"] = false.into();
        assert!(checked_numerical_terminal_quote(&view, h(1), 21, true).is_err());
        view["batchNumericalFinding"]["faultMaterialAvailable"] = true.into();
        view["batchNumericalFinding"]["faultEvidenceId"] = serde_json::Value::Null;
        assert!(checked_numerical_terminal_quote(&view, h(1), 21, true).is_err());
    }

    #[test]
    fn numerical_open_recomputes_the_root_and_checks_the_signing_bond() {
        let h = Hash64::from_u64_word;
        let signer = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(11), 0));
        let ranges = vec![BatchRangeV1 { id: h(5), bytes: 13, authentication: BatchRangeAuthenticationV1::WholeRange }];
        let demand = BatchDemandV1 {
            case_id: h(6),
            case_window: BatchCourtWindowV1 {
                claim_accepted_daa: 10,
                challenge_opened_daa: 20,
                collect_until_daa: 100,
                material_until_daa: 100,
            },
            opened_daa: 30,
            response_deadline_daa: 40,
            maximum_bytes: 13,
            part_bytes: MAX_BATCH_PART_BYTES_V1,
            ranges: ranges.clone(),
        };
        let quote = || OpenQuote {
            wire_version: 1,
            scope_id: h(7).to_string(),
            signer: bond_text(signer),
            request_root: demand.checked_request_root_v1().unwrap().to_string(),
            case_id: h(6).to_string(),
            original_accepted_daa: "10".into(),
            notice_daa: "20".into(),
            collect_until_daa: "100".into(),
            material_until_daa: "100".into(),
            opened_daa: "30".into(),
            response_deadline_daa: "40".into(),
            part_bytes: MAX_BATCH_PART_BYTES_V1,
            maximum_bytes: 13,
            ranges: vec![OpenRange { id: h(5).to_string(), bytes: 13, authentication: "whole-range".into() }],
        };
        let check = |q: OpenQuote| q.checked_open(h(1), h(2), 0, signer, ranges.clone());
        let opened = check(quote()).unwrap();
        assert_eq!(opened.demand, demand);
        assert_eq!((opened.claim, opened.notice_id, opened.signer), (h(1), h(2), signer));
        assert!(check(OpenQuote { opened_daa: "31".into(), ..quote() }).is_err());
        assert!(check(OpenQuote { signer: "other".into(), ..quote() }).is_err());
        let mut swapped = quote();
        swapped.ranges[0].id = h(9).to_string();
        assert!(check(swapped).is_err());
    }

    #[test]
    fn batches_sign_only_the_booked_step_and_resume_the_accepted_body() {
        let body = [1, 2, 3];
        let hash = Hash64::from_u64_word(1);
        let draft = Hash64::from_u64_word(2);
        let mut quote = ResponseQuote {
            request_root: String::new(),
            draft_id: String::new(),
            total_bytes: 3,
            begin_daa: 10,
            finish_daa: 12,
            draft_expires_daa: 13,
            retained_submitter: None,
            retained_body_hash: None,
            material_available: false,
            parts: vec![PartSlot { offset: 0, bytes: 3, assigned_daa: 11, accepted_daa: None }],
        };
        assert!(matches!(
            quote.action(ResponseStep::Begin, None, draft, "bond", hash, &body),
            Ok((10, MaterialActionV1::Begin { .. }))
        ));
        assert!(quote.action(ResponseStep::Part, Some(0), draft, "bond", hash, &body).is_err());
        quote.retained_submitter = Some("bond".into());
        quote.retained_body_hash = Some(hash.to_string());
        assert!(quote.action(ResponseStep::Part, Some(0), draft, "other", hash, &body).is_err());
        assert!(quote.action(ResponseStep::Finish, None, draft, "bond", hash, &body).is_err());
        assert!(matches!(quote.action(ResponseStep::Part, Some(0), draft, "bond", hash, &body),
            Ok((11, MaterialActionV1::Part { bytes, .. })) if bytes == body));
        quote.parts[0].accepted_daa = Some(11);
        assert!(quote.action(ResponseStep::Part, Some(0), draft, "bond", hash, &body).is_err());
        assert!(matches!(quote.action(ResponseStep::Finish, None, draft, "bond", hash, &body),
            Ok((12, MaterialActionV1::Finish { draft_id })) if draft_id == draft));
        quote.material_available = true;
        assert!(quote.check_material_needed(ResponseStep::Begin).is_err());
        assert!(quote.check_material_needed(ResponseStep::Part).is_err());
        assert!(quote.check_material_needed(ResponseStep::Finish).is_ok());
    }
}
