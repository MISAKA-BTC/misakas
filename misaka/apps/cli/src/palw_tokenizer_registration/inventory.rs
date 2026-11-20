//! Builds the existing native EOF inventory carriers, before TKR1 registration.
//! Local stream replay checks byte identity only. It creates no accepted row.
use super::{input_error, read_bounded};
use crate::{CliError, CliResult, keys::KeySource, node::Ctx, wallet::NodeView};
use kaspa_consensus_core::{
    palw_class_semantics_v1::PalwClassSemanticsDeclarationV1,
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1, palw_staged_verify_mldsa87_v1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1,
    palw_state_v2::PalwConsensusObjectV2,
    palw_tokenizer_artifact_inventory_v1::{
        PALW_ARTIFACT_INVENTORY_CONTEXT_V1, PALW_ARTIFACT_INVENTORY_PART_BYTES_V1, PalwTokenizerArtifactInventoryActionV1 as Action,
        PalwTokenizerArtifactInventoryOfferV1 as Offer, PalwTokenizerArtifactInventoryV1 as Inventory,
    },
    palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1,
};
use kaspa_pq_validator_core::ValidatorKey;
use kaspa_rpc_core::{GetPalwFenceScheduleRequest, api::rpc::RpcApi};
use std::{fs, io::Write, path::Path};

// TKR1's existing V1 rendering limit, not a new protocol maximum for legacy EOF.
const RAW_LIMIT: u64 = 8 << 20;

fn checked_raw(offer: &Offer, raw: &[u8]) -> Result<(), String> {
    if raw.is_empty() || raw.len() as u64 != offer.maximum_bytes || palw_tokenizer_raw_commitment_v1(raw) != offer.artifact_hash {
        return Err("EOF input differs from the signed exact raw length or tokenizer hash".into());
    }
    Ok(())
}

async fn inventory_open_context(nv: &NodeView) -> Result<(u64, bool), CliError> {
    if !nv.params.has_protected_lifecycle_drill_genesis_v1() {
        let now = nv.virtual_daa;
        return Ok((now, nv.params.palw_staged_public_court_v1_active_at(now)));
    }

    // The signer knows the isolated genesis, while the node owns its private schedule.
    let response = nv
        .client
        .get_palw_fence_schedule(GetPalwFenceScheduleRequest::default())
        .await
        .map_err(|err| CliError::connection(format!("cannot read the protected drill's fence schedule: {err}")))?;
    let doc = crate::palw_user::parse_view(
        "getPalwFenceSchedule",
        response.available,
        response.found,
        &response.view_json,
        "protected drill fence schedule",
    )?;
    let active = doc["fences"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == "palw_staged_public_court_v1"))
        .and_then(|row| row["armed"].as_bool())
        .ok_or_else(|| input_error("protected drill fence schedule did not report the staged public court"))?;
    Ok((response.now_daa, active))
}

fn object(offer: &Offer, daa: u64, action: PalwConstraintStreamActionV1, key: &ValidatorKey) -> Result<Vec<u8>, String> {
    let mut movement = PalwConstraintStreamMoveV1 {
        version: 1,
        claim: offer.artifact_hash,
        reserver: offer.opener,
        reserved_daa: daa,
        seed_id: offer.id_v1(),
        phase: PalwConstraintStreamPhaseV1::PrefixCourt,
        action,
        signature: Vec::new(),
    };
    movement.signature = key.sign_with_context(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).to_vec();
    let object = PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: Box::new(movement) };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)?;
    borsh::to_vec(&object).map_err(|e| e.to_string())
}

fn write(output: &Path, files: Vec<(String, Vec<u8>)>, report: serde_json::Value) -> CliResult {
    fs::create_dir(output).map_err(|e| input_error(format!("{}: {e}", output.display())))?;
    let mut created = Vec::new();
    let result = (|| -> Result<(), String> {
        for (name, bytes) in files
            .into_iter()
            .chain(std::iter::once(("plan.json".into(), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)))
        {
            let path = output.join(name);
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
            created.push(path);
            file.write_all(&bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in created {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(output);
        return Err(input_error(format!("incomplete inventory bundle: {error}")));
    }
    println!("{report}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn open(
    ctx: &Ctx,
    semantics_path: &Path,
    raw_path: &Path,
    bond: &str,
    accept_until: u64,
    maximum_prepaid: u64,
    keys: KeySource,
    output: &Path,
) -> CliResult {
    let semantics: PalwClassSemanticsDeclarationV1 =
        borsh::from_slice(&read_bounded(semantics_path, 4096)?).map_err(|e| input_error(e.to_string()))?;
    let raw = read_bounded(raw_path, RAW_LIMIT)?;
    let (nv, key, opener) = crate::palw_user::bond_signer_v1(ctx, &keys, bond).await?;
    let context = inventory_open_context(&nv).await;
    let _ = nv.client.disconnect().await;
    let (now, active) = context?;
    if !active {
        return Err(input_error("native EOF inventory is not active at the connected node's DAA"));
    }
    let mut offer = Offer {
        version: 1,
        artifact_hash: semantics.tokenizer_id,
        opener,
        maximum_bytes: semantics.raw_tokenizer_bytes,
        accept_until_daa: accept_until,
        signature: Vec::new(),
    };
    checked_raw(&offer, &raw).map_err(input_error)?;
    if !semantics.as_bound_row(now).well_formed_for(&semantics.class_id, &semantics.artifact_root) {
        return Err(input_error("invalid class semantics declaration"));
    }
    offer.signature = key.sign_with_context(offer.id_v1().as_byte_slice(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1).to_vec();
    let row = Inventory::new_v1(
        offer.clone(),
        now.checked_add(1).ok_or_else(|| input_error("prospective DAA overflow"))?,
        PalwStagedPublicCourtConfigV1::network_v1(),
    )
    .map_err(input_error)?;
    let prepaid = row.plan.rent_sompi.checked_add(row.plan.deposit_sompi).ok_or_else(|| input_error("inventory cost overflow"))?;
    if prepaid > maximum_prepaid {
        return Err(input_error(format!("native inventory prepaid cost {prepaid} exceeds {maximum_prepaid}")));
    }
    let opening = object(
        &offer,
        0,
        PalwConstraintStreamActionV1::ArtifactInventoryOpen { class_id: semantics.class_id, offer: offer.clone() },
        &key,
    )
    .map_err(input_error)?;
    let id = offer.id_v1().to_string();
    write(
        output,
        vec![
            ("offer.borsh".into(), borsh::to_vec(&offer).map_err(|e| input_error(e.to_string()))?),
            ("open.object.borsh".into(), opening),
        ],
        serde_json::json!({"inventoryId":id,"classId":semantics.class_id.to_string(),"tokenizerId":offer.artifact_hash.to_string(),
        "rawBytes":raw.len(),"quotedAtDaa":now,"acceptUntilDaa":accept_until,"prepaidSompi":prepaid,
        "submitted":false,"nativeInventoryAccepted":false,"classParentsAuthenticated":false,
        "next":"submit open.object.borsh with submit-object, read its accepting DAA, then inventory-continue"}),
    )
}

fn continuation_files(
    offer: &Offer,
    raw: &[u8],
    opened: u64,
    now: u64,
    key: &ValidatorKey,
) -> Result<(Vec<(String, Vec<u8>)>, u64), String> {
    checked_raw(offer, raw)?;
    offer.validate_signature_v1(key.public_key(), palw_staged_verify_mldsa87_v1)?;
    if opened > now {
        return Err("accepted Open DAA is after the connected node's tip".into());
    }
    let mut row = Inventory::new_v1(offer.clone(), opened, PalwStagedPublicCourtConfigV1::network_v1())?;
    let prospective = now.checked_add(1).ok_or("prospective DAA overflow")?;
    let mut files = Vec::new();
    for (index, bytes) in raw.chunks(PALW_ARTIFACT_INVENTORY_PART_BYTES_V1).enumerate() {
        let action = Action::Part { offset: (index * PALW_ARTIFACT_INVENTORY_PART_BYTES_V1) as u64, bytes: bytes.to_vec() };
        row.apply_v1(&action, prospective)?;
        files.push((
            format!("part-{index:04}.object.borsh"),
            object(offer, opened, PalwConstraintStreamActionV1::ArtifactInventoryMove(action), key)?,
        ));
    }
    // Checks the complete native hash stream locally. Never persist its return
    // value as an accepted certificate or treat the supplied clock as readback.
    row.apply_v1(&Action::Finish, prospective)?.ok_or("native local EOF did not finish")?;
    files.push((
        "finish.object.borsh".into(),
        object(offer, opened, PalwConstraintStreamActionV1::ArtifactInventoryMove(Action::Finish), key)?,
    ));
    Ok((files, row.until_daa))
}

pub(super) async fn continuation(
    ctx: &Ctx,
    offer_path: &Path,
    raw_path: &Path,
    opened: u64,
    keys: KeySource,
    output: &Path,
) -> CliResult {
    let offer: Offer = borsh::from_slice(&read_bounded(offer_path, 8192)?).map_err(|e| input_error(e.to_string()))?;
    offer.check_v1().map_err(input_error)?;
    let raw = read_bounded(raw_path, RAW_LIMIT)?;
    checked_raw(&offer, &raw).map_err(input_error)?;
    let bond = format!("{}:{}", offer.opener.0.transaction_id, offer.opener.0.index);
    let (nv, key, _) = crate::palw_user::bond_signer_v1(ctx, &keys, &bond).await?;
    let now = nv.virtual_daa;
    let _ = nv.client.disconnect().await;
    let (files, until) = continuation_files(&offer, &raw, opened, now, &key).map_err(input_error)?;
    write(
        output,
        files,
        serde_json::json!({"inventoryId":offer.id_v1().to_string(),"tokenizerId":offer.artifact_hash.to_string(),
        "acceptedOpenDaa":opened,"acceptedOpenDaaAuthenticated":false,"untilDaa":until,
        "rawBytes":raw.len(),"parts":raw.chunks(PALW_ARTIFACT_INVENTORY_PART_BYTES_V1).len(),"localStreamChecked":true,
        "submitted":false,"nativeInventoryAccepted":false,"next":"submit Parts in offset order, then Finish; require the live TKR1 quote to authenticate the completed EOF"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{Hash64, palw_state_v2::PalwBondKeyV2, tx::TransactionOutpoint};

    #[test]
    fn eof_carriers_authenticate_exact_stream_and_original_open_point() {
        let key = ValidatorKey::from_seed([57; 32]);
        let raw = vec![b'a'; PALW_ARTIFACT_INVENTORY_PART_BYTES_V1 + 1];
        let mut offer = Offer {
            version: 1,
            artifact_hash: palw_tokenizer_raw_commitment_v1(&raw),
            opener: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(1), 0)),
            maximum_bytes: raw.len() as u64,
            accept_until_daa: 105,
            signature: Vec::new(),
        };
        offer.signature = key.sign_with_context(offer.id_v1().as_byte_slice(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1).to_vec();
        let opening = object(
            &offer,
            0,
            PalwConstraintStreamActionV1::ArtifactInventoryOpen { class_id: Hash64::from_u64_word(2), offer: offer.clone() },
            &key,
        )
        .unwrap();
        let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement: open } = borsh::from_slice(&opening).unwrap() else {
            panic!("inventory Open")
        };
        assert_eq!(open.reserved_daa, 0);
        assert!(palw_staged_verify_mldsa87_v1(
            key.public_key(),
            &open.signing_message(),
            &open.signature,
            PALW_CONSTRAINT_STREAM_CONTEXT_V1
        ));
        let (files, until) = continuation_files(&offer, &raw, 101, 106, &key).unwrap();
        assert_eq!(files.len(), 3);
        let mut native = Inventory::new_v1(offer.clone(), 101, PalwStagedPublicCourtConfigV1::network_v1()).unwrap();
        assert_eq!(until, native.until_daa);
        for (_, bytes) in &files {
            let PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } = borsh::from_slice(bytes).unwrap() else {
                panic!("inventory continuation")
            };
            assert_eq!(
                (movement.claim, movement.seed_id, movement.reserver, movement.reserved_daa),
                (offer.artifact_hash, offer.id_v1(), offer.opener, 101)
            );
            assert!(palw_staged_verify_mldsa87_v1(
                key.public_key(),
                &movement.signing_message(),
                &movement.signature,
                PALW_CONSTRAINT_STREAM_CONTEXT_V1
            ));
            let mut altered = movement.clone();
            altered.reserved_daa += 1;
            assert!(!palw_staged_verify_mldsa87_v1(
                key.public_key(),
                &altered.signing_message(),
                &altered.signature,
                PALW_CONSTRAINT_STREAM_CONTEXT_V1
            ));
            let PalwConstraintStreamActionV1::ArtifactInventoryMove(action) = movement.action else { panic!("Part/Finish") };
            native.apply_v1(&action, 107).unwrap();
        }
        let certificate = native.certificate.unwrap();
        assert_eq!(
            (certificate.actual_bytes, certificate.artifact_hash, certificate.opened_daa),
            (raw.len() as u64, offer.artifact_hash, 101)
        );
        assert!(continuation_files(&offer, &raw[..raw.len() - 1], 101, 106, &key).is_err());
        let mut foreign = raw.clone();
        foreign[0] ^= 1;
        assert!(continuation_files(&offer, &foreign, 101, 106, &key).is_err());
        assert!(continuation_files(&offer, &raw, 101, 106, &ValidatorKey::from_seed([58; 32])).is_err());
        assert!(continuation_files(&offer, &raw, 107, 106, &key).is_err());
        assert!(continuation_files(&offer, &raw, 105, 106, &key).is_err());
        assert!(continuation_files(&offer, &raw, 101, until, &key).is_err());
    }
}
