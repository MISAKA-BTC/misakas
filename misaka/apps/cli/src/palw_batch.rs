//! Fund and file one response in its rooted BCT1 service slot.
use crate::{CliError, CliResult, exit, keys::KeySource, node::Ctx, wallet::NodeView};
use kaspa_consensus_core::{
    Hash64,
    palw_batch_court_v1::{
        MAX_BATCH_PART_BYTES_V1, MAX_BATCH_PARTS_V1,
        carriage::{BatchCourtWireV1, BatchMaterialControlV1},
    },
    palw_court_shared_material_intake_v1::{
        MATERIAL_BATCH_CONTEXT_V1, MATERIAL_BATCH_VERSION, MaterialActionV1, MaterialTargetV1, SignedMaterialIntakeV1,
        material_batch_carrier_fee_v1, material_batch_draft_id_v1,
    },
    palw_state_v2::PalwBondKeyV2,
};
use kaspa_rpc_core::{GetPalwCourtStateRequest, api::rpc::RpcApi};
use serde::Deserialize;
use std::{io::Read, path::Path};

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
    parts: Vec<PartSlot>,
}

fn refused(reason: impl Into<String>) -> CliError {
    CliError::new(exit::GENERIC, reason.into())
}

async fn read_quote(nv: &NodeView, request: GetPalwCourtStateRequest) -> Result<(u64, String), CliError> {
    let answer = nv.client.get_palw_court_state(request).await.map_err(|e| CliError::connection(e.to_string()))?;
    if !answer.available || !answer.found {
        return Err(refused("the node does not report an accepted Batch response reservation"));
    }
    Ok((answer.now_daa, answer.view_json))
}

impl ResponseQuote {
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
    let addr = key.funding_address(nv.params.prefix());
    let (outpoint, entry) = crate::palw_fp::lifecycle_candidates_v1(&nv, &addr)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| refused("no mature unspent UTXO funds the Batch response"))?;
    let floor = kaspa_pq_validator_core::PALW_TX_FEE_FLOOR_SOMPI;
    let probe = key.build_palw_batch_court_tx(&wire, outpoint, &entry, floor).map_err(refused)?;
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
        return Err(refused(format!("Batch response fee {fee} exceeds the maximum fee")));
    }
    let tx = key.build_palw_batch_court_tx(&wire, outpoint, &entry, fee).map_err(refused)?;
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

fn bond_text(bond: PalwBondKeyV2) -> String {
    kaspa_consensus_core::palw_user_views_v1::palw_bond_key_text_v1(&bond)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
