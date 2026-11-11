//! Prepare and fund plain Output controls from authenticated parent quotes.
//! A BCT2 payload is not a submitted transaction or an accepted ruling.
use super::*;
use kaspa_consensus_core::palw_batch_court_v1::carriage::{
    BatchPlainOutputTerminalV2, BatchPlainOutputWireV2, PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2, decode_batch_plain_output_payload_v2,
};
use misaka_palw_sdk::batch_plain_output_v2::{BatchPlainOutputRequestV2, prepare_batch_plain_output_request_v2};

fn checked_request(
    view: &serde_json::Value,
    claim: Hash64,
    notice: Hash64,
    filer: PalwBondKeyV2,
    network: Hash64,
    now: u64,
) -> Result<(BatchPlainOutputRequestV2, Hash64, u64), CliError> {
    use crate::palw_user::parse_hash;
    if view["version"] != 2
        || view["kind"] != "palwBatchVerifyPlainOutput"
        || view["claimId"] != claim.to_string()
        || view["nowDaa"] != now
        || view["tipBlock"].as_str().and_then(|tip| tip.parse::<Hash64>().ok()).is_none_or(|tip| tip == Hash64::default())
        || view["acceptedParentRoot"]
            .as_str()
            .and_then(|root| root.parse::<Hash64>().ok())
            .is_none_or(|root| root == Hash64::default())
    {
        return Err(refused("the node returned a different plain Output request"));
    }
    if !view["error"].is_null() {
        return Err(refused(view["error"].as_str().unwrap_or("invalid plain Output quote error")));
    }
    let q = &view["verify"];
    if q["wireVersion"] != 2
        || q["claimId"] != claim.to_string()
        || q["noticeId"] != notice.to_string()
        || q["signer"] != bond_text(filer)
        || q["networkDomain"] != network.to_string()
        || q["signingContextHex"] != faster_hex::hex_string(PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2)
    {
        return Err(refused("plain Output quote substituted the claim, notice, signer, network or purpose"));
    }
    let hash = |field: &str| parse_hash(field, q[field].as_str().ok_or_else(|| refused(format!("quote omitted {field}")))?);
    let slot = q["assignedDaa"].as_u64().ok_or_else(|| refused("plain Output quote omitted its booked DAA"))?;
    if now > slot {
        return Err(refused(format!("plain Output verification slot {slot} has passed at DAA {now}")));
    }
    let request = BatchPlainOutputRequestV2 {
        network,
        claim,
        scope: hash("scopeId")?,
        source: hash("sourceId")?,
        notice,
        policy: hash("policyId")?,
        material: hash("materialId")?,
        filer,
    };
    let digest = hash("signingDigest")?;
    if request.signing_message_v2().map_err(refused)? != digest {
        return Err(refused("plain Output quote signing digest differs from the native request"));
    }
    Ok((request, digest, slot))
}

pub(crate) async fn prepare_plain_output_v2(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    notice: &str,
    output: &Path,
) -> CliResult {
    use crate::palw_user::parse_hash;
    let claim = parse_hash("claim", claim)?;
    let notice = parse_hash("notice", notice)?;
    let (nv, key, filer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let query =
        GetPalwCourtStateRequest { claim_hex: claim.to_string(), batch_open: "verify-plain-output-v2".into(), ..Default::default() };
    let (now, raw) = read_quote(&nv, query.clone()).await?;
    if raw.len() > 65_536 {
        return Err(refused("plain Output request quote exceeds its local response bound"));
    }
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|error| refused(error.to_string()))?;
    let (request, digest, slot) = checked_request(&view, claim, notice, filer, crate::bond::network_domain(&nv), now)?;
    let payload = prepare_batch_plain_output_request_v2(request, key.public_key(), &key).map_err(refused)?;
    if read_quote(&nv, query).await? != (now, raw) {
        return Err(refused("plain Output accepted state or reservation changed while preparing; retry"));
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(output).map_err(|e| refused(e.to_string()))?;
    if let Err(error) = file.write_all(&payload).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = std::fs::remove_file(output);
        return Err(refused(format!("plain Output payload write failed: {error}")));
    }
    println!(
        "{}",
        serde_json::json!({
            "submitted": false, "acceptanceConfirmed": false, "claimId": claim.to_string(),
            "noticeId": notice.to_string(), "signingDigest": digest.to_string(),
            "assignedDaa": slot, "observedDaa": now, "tipBlock": view["tipBlock"],
            "acceptedParentRoot": view["acceptedParentRoot"], "file": output,
        })
    );
    let _ = nv.client.disconnect().await;
    Ok(())
}

fn checked_plain_full_ids_open(
    view: &serde_json::Value,
    claim: Hash64,
    notice: Hash64,
    signer: PalwBondKeyV2,
    network: Hash64,
    now: u64,
) -> Result<SignedBatchDemandV1, CliError> {
    use kaspa_consensus_core::palw_court_shared_material_intake_v1::BATCH_PLAIN_OUTPUT_ORDINAL_V2;
    if view["version"] != 2
        || view["kind"] != "palwBatchPlainOutputFullIdsOpen"
        || view["claimId"] != claim.to_string()
        || view["noticeId"] != notice.to_string()
        || view["ordinal"] != BATCH_PLAIN_OUTPUT_ORDINAL_V2.to_string()
        || view["networkDomain"] != network.to_string()
        || view["nowDaa"] != now
        || !view["error"].is_null()
        || ["tipBlock", "acceptedParentRoot"]
            .iter()
            .any(|field| view[*field].as_str().and_then(|s| s.parse::<Hash64>().ok()).is_none_or(|id| id == Hash64::default()))
    {
        return Err(refused("the node returned no matching authenticated plain Output FullIds quote"));
    }
    let count = view["fullIdsCount"]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| refused("plain Output quote omitted its generated ID count"))?;
    let ranges = vec![batch_stop_full_ids_range_v1(claim, count).map_err(refused)?];
    let quote: OpenQuote = serde_json::from_value(view["open"].clone()).map_err(|err| refused(err.to_string()))?;
    let open = quote.checked_open(claim, notice, BATCH_PLAIN_OUTPUT_ORDINAL_V2, signer, ranges)?;
    checked_send_slot(now, open.demand.opened_daa, false)?;
    Ok(open)
}

pub(crate) async fn open_plain_output_v2(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    notice: &str,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    use kaspa_consensus_core::palw_court_shared_material_intake_v1::BATCH_PLAIN_OUTPUT_ORDINAL_V2;
    let claim = crate::palw_user::parse_hash("claim", claim)?;
    let notice = crate::palw_user::parse_hash("notice", notice)?;
    let (nv, key, signer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let network = crate::bond::network_domain(&nv);
    let request = GetPalwCourtStateRequest {
        claim_hex: claim.to_string(),
        batch_open: format!("{notice}/plain-output-full-ids"),
        batch_ordinal: BATCH_PLAIN_OUTPUT_ORDINAL_V2.to_string(),
        ..Default::default()
    };
    let (now, raw) = read_plain_quote(&nv, request.clone()).await?;
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|err| refused(err.to_string()))?;
    let mut open = checked_plain_full_ids_open(&view, claim, notice, signer, network, now)?;
    let slot = open.demand.opened_daa;
    checked_send_slot(now, slot, submit)?;
    let message = open.signing_message_v1(network).map_err(refused)?;
    open.signature = key.sign_with_context(message.as_byte_slice(), BATCH_DEMAND_SIGNATURE_CONTEXT_V1).to_vec();
    let (tx, fee, rent) = fund_control(&nv, &key, &BatchCourtWireV1::Open(open), maximum_fee).await?;
    if read_plain_quote(&nv, request).await? != (now, raw) {
        return Err(refused("plain Output FullIds accepted state or reservation changed before submission; retry"));
    }
    if submit {
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|err| refused(err.to_string()))?;
    }
    println!(
        "{}",
        serde_json::json!({
            "submitted": submit, "acceptanceConfirmed": false, "txid": tx.id().to_string(),
            "claimId": claim.to_string(), "noticeId": notice.to_string(),
            "ordinal": BATCH_PLAIN_OUTPUT_ORDINAL_V2.to_string(), "assignedDaa": slot,
            "observedDaa": now, "feeSompi": fee, "storageRentSompi": rent,
            "requestRoot": view["open"]["requestRoot"], "tipBlock": view["tipBlock"],
            "acceptedParentRoot": view["acceptedParentRoot"],
        })
    );
    let _ = nv.client.disconnect().await;
    Ok(())
}

fn checked_terminal(
    view: &serde_json::Value,
    claim: Hash64,
    network: Hash64,
    now: u64,
) -> Result<(BatchPlainOutputTerminalV2, u64), CliError> {
    use crate::palw_user::parse_hash;
    if view["version"] != 2
        || view["kind"] != "palwBatchTerminalPlainOutput"
        || view["claimId"] != claim.to_string()
        || view["nowDaa"] != now
        || !view["error"].is_null()
        || ["tipBlock", "acceptedParentRoot"]
            .iter()
            .any(|field| view[*field].as_str().and_then(|s| s.parse::<Hash64>().ok()).is_none_or(|id| id == Hash64::default()))
    {
        return Err(refused("the node returned no authenticated plain Output fault terminal"));
    }
    let q = &view["terminal"];
    if q["wireVersion"] != 2
        || q["claimId"] != claim.to_string()
        || q["networkDomain"] != network.to_string()
        || q["priorFaultReceipt"] != true
    {
        return Err(refused("plain Output terminal substituted the claim, network or prior fault"));
    }
    let receipt_digest =
        parse_hash("receipt digest", q["receiptDigest"].as_str().ok_or_else(|| refused("terminal omitted receipt"))?)?;
    let slot = q["assignedDaa"].as_u64().ok_or_else(|| refused("terminal omitted booked DAA"))?;
    if receipt_digest == Hash64::default() || now > slot {
        return Err(refused("plain Output fault receipt or terminal slot is invalid"));
    }
    Ok((BatchPlainOutputTerminalV2 { claim, receipt_digest }, slot))
}

fn checked_send_slot(now: u64, slot: u64, submit: bool) -> Result<(), CliError> {
    if now > slot || (submit && now != slot) {
        return Err(refused(format!("plain Output control is booked at DAA {slot}, current fold DAA is {now}")));
    }
    Ok(())
}

async fn fund_plain_control(
    nv: &NodeView,
    key: &kaspa_pq_validator_core::ValidatorKey,
    wire: &BatchPlainOutputWireV2,
    maximum_fee: Option<u64>,
) -> Result<(kaspa_consensus_core::tx::Transaction, u64, u64), CliError> {
    use kaspa_consensus_core::palw_court_shared_material_intake_v1::material_batch_plain_output_carrier_fee_v2;
    let addr = key.funding_address(nv.params.prefix());
    let (outpoint, entry) = crate::palw_fp::lifecycle_candidates_v1(nv, &addr)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| refused("no mature unspent UTXO funds the plain Output control"))?;
    let floor = kaspa_pq_validator_core::PALW_TX_FEE_FLOOR_SOMPI;
    let probe = key.build_palw_batch_plain_output_tx_v2(wire, outpoint, &entry, floor).map_err(refused)?;
    let rent = material_batch_plain_output_carrier_fee_v2(&probe, 0)
        .map_err(refused)?
        .ok_or_else(|| refused("funded transaction lost its BCT2 purpose"))?
        .required_storage_rent;
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
        return Err(refused(format!("plain Output fee {fee} exceeds the maximum fee")));
    }
    let tx = key.build_palw_batch_plain_output_tx_v2(wire, outpoint, &entry, fee).map_err(refused)?;
    Ok((tx, fee, rent))
}

async fn send_plain_control(
    nv: &NodeView,
    key: &kaspa_pq_validator_core::ValidatorKey,
    query: GetPalwCourtStateRequest,
    now: u64,
    raw: String,
    wire: BatchPlainOutputWireV2,
    slot: u64,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    checked_send_slot(now, slot, submit)?;
    let (tx, fee, rent) = fund_plain_control(nv, key, &wire, maximum_fee).await?;
    // Re-read after signing and funding, including for preview. The same accepted
    // parent, reserved slot and receipt/FullIds must still define this transaction.
    if read_plain_quote(nv, query).await? != (now, raw.clone()) {
        return Err(refused("plain Output accepted state or reservation changed before submission; retry"));
    }
    if submit {
        nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|err| refused(err.to_string()))?;
    }
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|err| refused(err.to_string()))?;
    let (claim, receipt) = match wire {
        BatchPlainOutputWireV2::Verify(request) => (request.claim, None),
        BatchPlainOutputWireV2::Terminal(control) => (control.claim, Some(control.receipt_digest.to_string())),
    };
    println!(
        "{}",
        serde_json::json!({
            "submitted": submit, "acceptanceConfirmed": false, "txid": tx.id().to_string(),
            "claimId": claim.to_string(), "receiptDigest": receipt,
            "assignedDaa": slot, "observedDaa": now, "feeSompi": fee, "storageRentSompi": rent,
            "tipBlock": view["tipBlock"], "acceptedParentRoot": view["acceptedParentRoot"],
        })
    );
    let _ = nv.client.disconnect().await;
    Ok(())
}

pub(crate) async fn verify_plain_output_v2(
    ctx: &Ctx,
    key_source: &KeySource,
    bond: &str,
    claim: &str,
    notice: &str,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    use crate::palw_user::parse_hash;
    let claim = parse_hash("claim", claim)?;
    let notice = parse_hash("notice", notice)?;
    let (nv, key, filer) = crate::palw_user::bond_signer_v1(ctx, key_source, bond).await?;
    let query =
        GetPalwCourtStateRequest { claim_hex: claim.to_string(), batch_open: "verify-plain-output-v2".into(), ..Default::default() };
    let (now, raw) = read_plain_quote(&nv, query.clone()).await?;
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|err| refused(err.to_string()))?;
    let (request, _, slot) = checked_request(&view, claim, notice, filer, crate::bond::network_domain(&nv), now)?;
    checked_send_slot(now, slot, submit)?;
    let payload = prepare_batch_plain_output_request_v2(request, key.public_key(), &key).map_err(refused)?;
    let wire = decode_batch_plain_output_payload_v2(&payload)
        .map_err(|err| refused(err.to_string()))?
        .ok_or_else(|| refused("prepared request lost its BCT2 purpose"))?;
    send_plain_control(&nv, &key, query, now, raw, wire, slot, submit, maximum_fee).await
}

pub(crate) async fn convict_plain_output_v2(
    ctx: &Ctx,
    key_source: &KeySource,
    claim: &str,
    submit: bool,
    maximum_fee: Option<u64>,
) -> CliResult {
    let claim = crate::palw_user::parse_hash("claim", claim)?;
    let nv = crate::wallet::connect(ctx).await?;
    let query =
        GetPalwCourtStateRequest { claim_hex: claim.to_string(), batch_open: "convict-plain-output-v2".into(), ..Default::default() };
    let (now, raw) = read_plain_quote(&nv, query.clone()).await?;
    let view: serde_json::Value = serde_json::from_str(&raw).map_err(|err| refused(err.to_string()))?;
    let (control, slot) = checked_terminal(&view, claim, crate::bond::network_domain(&nv), now)?;
    checked_send_slot(now, slot, submit)?;
    let key = key_source.load_key()?;
    send_plain_control(&nv, &key, query, now, raw, BatchPlainOutputWireV2::Terminal(control), slot, submit, maximum_fee).await
}

async fn read_plain_quote(nv: &NodeView, query: GetPalwCourtStateRequest) -> Result<(u64, String), CliError> {
    let (now, raw) = read_quote(nv, query).await?;
    if raw.len() > 65_536 {
        return Err(refused("plain Output quote exceeds its local response bound"));
    }
    Ok((now, raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_output_full_ids_quote_uses_canonical_range_and_reserved_ordinal() {
        use kaspa_consensus_core::palw_court_shared_material_intake_v1::BATCH_PLAIN_OUTPUT_ORDINAL_V2;
        let h = Hash64::from_u64_word;
        let claim = h(1);
        let notice = h(2);
        let network = h(3);
        let signer = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(4), 0));
        let range = batch_stop_full_ids_range_v1(claim, 5).unwrap();
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
            maximum_bytes: u64::from(range.bytes),
            part_bytes: MAX_BATCH_PART_BYTES_V1,
            ranges: vec![range.clone()],
        };
        let view = serde_json::json!({
            "version": 2, "kind": "palwBatchPlainOutputFullIdsOpen", "claimId": claim.to_string(),
            "noticeId": notice.to_string(), "networkDomain": network.to_string(), "fullIdsCount": 5,
            "nowDaa": 20, "tipBlock": h(7).to_string(), "acceptedParentRoot": h(8).to_string(), "error": null,
            "ordinal": BATCH_PLAIN_OUTPUT_ORDINAL_V2.to_string(),
            "open": {
                "wireVersion": 1, "scopeId": h(9).to_string(), "signer": bond_text(signer),
                "requestRoot": demand.checked_request_root_v1().unwrap().to_string(), "caseId": h(6).to_string(),
                "originalAcceptedDaa": "10", "noticeDaa": "20", "collectUntilDaa": "100", "materialUntilDaa": "100",
                "openedDaa": "30", "responseDeadlineDaa": "40", "partBytes": MAX_BATCH_PART_BYTES_V1,
                "maximumBytes": range.bytes,
                "ranges": [{"id": range.id.to_string(), "bytes": range.bytes, "authentication": "whole-range"}],
            }
        });
        let check = |value: &serde_json::Value| checked_plain_full_ids_open(value, claim, notice, signer, network, 20);
        let open = check(&view).unwrap();
        assert_eq!(open.demand, demand);
        assert_eq!(open.ordinal, BATCH_PLAIN_OUTPUT_ORDINAL_V2);
        for field in ["claimId", "noticeId", "networkDomain", "tipBlock", "acceptedParentRoot"] {
            let mut bad = view.clone();
            bad[field] = Hash64::default().to_string().into();
            assert!(check(&bad).is_err(), "substituted {field}");
        }
        for count in [0, 6, u64::MAX] {
            let mut bad = view.clone();
            bad["fullIdsCount"] = count.into();
            assert!(check(&bad).is_err());
        }
        let mut bad = view.clone();
        bad["ordinal"] = u64::MAX.to_string().into();
        assert!(check(&bad).is_err());
        bad = view.clone();
        bad["open"]["ranges"][0]["authentication"] = "chunk".into();
        assert!(check(&bad).is_err());
        bad = view.clone();
        bad["open"]["signer"] = "different".into();
        assert!(check(&bad).is_err());
        bad = view;
        bad["nowDaa"] = 31.into();
        assert!(checked_plain_full_ids_open(&bad, claim, notice, signer, network, 31).is_err());
    }

    #[test]
    fn plain_output_terminal_requires_fault_parent_network_and_exact_send_slot() {
        let h = Hash64::from_u64_word;
        let claim = h(1);
        let network = h(2);
        let view = serde_json::json!({
            "version": 2, "kind": "palwBatchTerminalPlainOutput", "claimId": claim.to_string(),
            "nowDaa": 20, "tipBlock": h(3).to_string(), "acceptedParentRoot": h(4).to_string(), "error": null,
            "terminal": {
                "wireVersion": 2, "claimId": claim.to_string(), "networkDomain": network.to_string(),
                "receiptDigest": h(5).to_string(), "assignedDaa": 21, "priorFaultReceipt": true,
            }
        });
        let check = |value: &serde_json::Value| checked_terminal(value, claim, network, 20);
        assert_eq!(check(&view).unwrap(), (BatchPlainOutputTerminalV2 { claim, receipt_digest: h(5) }, 21));
        for field in ["claimId", "networkDomain", "receiptDigest"] {
            let mut wrong = view.clone();
            wrong["terminal"][field] = serde_json::json!(Hash64::default().to_string());
            assert!(check(&wrong).is_err(), "substituted {field}");
        }
        for field in ["tipBlock", "acceptedParentRoot"] {
            let mut wrong = view.clone();
            wrong[field] = serde_json::Value::Null;
            assert!(check(&wrong).is_err(), "missing {field}");
        }
        let mut holds = view.clone();
        holds["terminal"]["priorFaultReceipt"] = false.into();
        assert!(check(&holds).is_err());
        let mut expired = view.clone();
        expired["terminal"]["assignedDaa"] = 19.into();
        assert!(check(&expired).is_err());
        let mut refused = view;
        refused["error"] = "profile inactive".into();
        assert!(check(&refused).is_err());
        assert!(checked_send_slot(20, 21, false).is_ok());
        assert!(checked_send_slot(20, 21, true).is_err());
        assert!(checked_send_slot(21, 21, true).is_ok());
        assert!(checked_send_slot(22, 21, false).is_err());
        assert!(checked_send_slot(22, 21, true).is_err());
    }

    #[test]
    fn plain_output_quote_requires_exact_subject_digest_and_live_slot() {
        let h = Hash64::from_u64_word;
        let r = BatchPlainOutputRequestV2 {
            network: h(1),
            claim: h(2),
            scope: h(3),
            source: h(4),
            notice: h(5),
            policy: h(6),
            material: h(7),
            filer: PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(h(8), 0)),
        };
        let view = serde_json::json!({
            "version": 2, "kind": "palwBatchVerifyPlainOutput", "claimId": r.claim.to_string(),
            "nowDaa": 20, "tipBlock": h(9).to_string(), "acceptedParentRoot": h(10).to_string(), "error": null,
            "verify": {
                "wireVersion": 2, "claimId": r.claim.to_string(), "scopeId": r.scope.to_string(),
                "sourceId": r.source.to_string(), "noticeId": r.notice.to_string(), "policyId": r.policy.to_string(),
                "materialId": r.material.to_string(), "signer": bond_text(r.filer), "networkDomain": r.network.to_string(),
                "assignedDaa": 21, "signingDigest": r.signing_message_v2().unwrap().to_string(),
                "signingContextHex": faster_hex::hex_string(PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2),
            }
        });
        let check = |value: &serde_json::Value| checked_request(value, r.claim, r.notice, r.filer, r.network, 20);
        assert_eq!(check(&view).unwrap(), (r, r.signing_message_v2().unwrap(), 21));
        for field in ["scopeId", "sourceId", "policyId", "materialId", "claimId", "noticeId", "networkDomain", "signingDigest"] {
            let mut changed = view.clone();
            changed["verify"][field] = serde_json::json!(h(99).to_string());
            assert!(check(&changed).is_err(), "substituted {field}");
        }
        let mut expired = view.clone();
        expired["verify"]["assignedDaa"] = serde_json::json!(19);
        assert!(check(&expired).is_err());
        let mut refused = view;
        refused["error"] = serde_json::json!("plain Output context unavailable");
        assert!(check(&refused).is_err());
    }
}
