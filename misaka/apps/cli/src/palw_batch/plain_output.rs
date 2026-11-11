//! Read the rooted verification request and prepare its purpose signature.
//! A BCT2 payload is not a submitted transaction or an accepted ruling.
use super::*;
use kaspa_consensus_core::palw_batch_court_v1::carriage::PLAIN_OUTPUT_SIGNATURE_CONTEXT_V2;
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

#[cfg(test)]
mod tests {
    use super::*;

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
