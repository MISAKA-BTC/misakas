//! Local Open construction. Supplied identity pins remain untrusted until the
//! existing live quote and native acceptance validate their rooted parents.
use kaspa_consensus_core::{
    Hash64,
    mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
    palw_state_v2::PalwBondKeyV2,
    palw_tokenizer_registration_v1::{
        TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1, TokenizerRegistrationFinishBodyV1, TokenizerRegistrationFinishV1,
        TokenizerRegistrationOpenBodyV1, TokenizerRegistrationPolicyV1,
    },
    palw_tokenizer_rendering_v2::{
        PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingBudgetV2, PalwTokenizerRenderingStatementV2,
        derive_tokenizer_rendering_v2,
    },
    tx::TransactionOutpoint,
};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Specification {
    version: u16,
    network: Hash64,
    policy_id: Hash64,
    class_id: Hash64,
    container_root: Hash64,
    semantics_id: Hash64,
    inventory_id: Hash64,
    artifact_root: Hash64,
    tokenizer_id: Hash64,
    table_root: Hash64,
    vocab: u32,
    rendering_rule: u8,
    opener_transaction_id: Hash64,
    opener_index: u32,
    nonce: Hash64,
    proposed_accepting_daa: u64,
    accept_until_daa: u64,
    finish_until_daa: u64,
    max_registration_charge: u64,
    declared_key_events: u64,
}

fn read(path: &Path, maximum: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path).and_then(|f| f.take(maximum + 1).read_to_end(&mut bytes)).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() as u64 > maximum {
        return Err(format!("{} exceeds {maximum} bytes", path.display()));
    }
    Ok(bytes)
}

fn build(
    spec: &Specification,
    policy: &TokenizerRegistrationPolicyV1,
    raw: &[u8],
    eog: &[u8],
) -> Result<(Vec<u8>, serde_json::Value), String> {
    policy.check_v1()?;
    if spec.version != 1 || spec.policy_id != policy.id_v1()? {
        return Err("Open draft version or policy ID differs from the supplied policy".into());
    }
    let rule = borsh::from_slice(&[spec.rendering_rule]).map_err(|e| format!("unsupported rendering rule: {e}"))?;
    let mut open = TokenizerRegistrationOpenBodyV1 {
        version: 1,
        network: spec.network,
        policy: spec.policy_id,
        pins: PalwTfp1TokenizerIntakePinsV2 {
            class_id: spec.class_id,
            container_root: spec.container_root,
            semantics_id: spec.semantics_id,
            inventory_id: spec.inventory_id,
        },
        artifact_root: spec.artifact_root,
        statement: PalwTokenizerRenderingStatementV2 {
            tokenizer_id: spec.tokenizer_id,
            exact_raw_bytes: raw.len() as u64,
            table_root: spec.table_root,
            vocab: spec.vocab,
            rule,
        },
        // Replaced below with the native digest of the bounded EOG decoding.
        eog_digest: spec.table_root,
        opener: PalwBondKeyV2(TransactionOutpoint::new(spec.opener_transaction_id, spec.opener_index)),
        nonce: spec.nonce,
        accept_until_daa: spec.accept_until_daa,
        finish_until_daa: spec.finish_until_daa,
        material_until_daa: spec.finish_until_daa.checked_add(policy.retention_daa).ok_or("Open retention clock overflow")?,
        max_registration_charge: spec.max_registration_charge,
    };
    open.check_v1()?;
    // Reuse the native Finish decoder's vector limits before allocating any
    // caller-declared EOG IDs or Merkle siblings. This is not a signed Finish.
    let mut framed = borsh::to_vec(&TokenizerRegistrationFinishBodyV1 {
        version: 1,
        network: open.network,
        policy: open.policy,
        open_id: open.id_v1()?,
        opener: open.opener,
        eog_digest: open.eog_digest,
        finish_until_daa: open.finish_until_daa,
    })
    .map_err(|e| e.to_string())?;
    framed.resize(framed.len() + MLDSA87_SIGNATURE_LEN, 0);
    framed.extend_from_slice(eog);
    let mut finish: TokenizerRegistrationFinishV1 = borsh::from_slice(&framed).map_err(|e| format!("bounded EOG opening: {e}"))?;
    open.eog_digest = TokenizerRegistrationFinishV1::eog_digest_v1(&finish.eog)?;
    finish.body.open_id = open.id_v1()?;
    finish.body.eog_digest = open.eog_digest;
    finish.check_open_v1(&open, policy)?;
    let quote = policy.quote_v1(&open, spec.proposed_accepting_daa, spec.declared_key_events)?;
    let facts = derive_tokenizer_rendering_v2(
        open.statement,
        raw,
        &finish.eog,
        PalwTokenizerRenderingBudgetV2 {
            raw_bytes: policy.max_raw_bytes,
            vocab: policy.max_vocab,
            key_events: spec.declared_key_events,
            eog_ids: policy.max_eog_ids,
            total_rendering_bytes: policy.max_rendering_bytes,
        },
    )?;
    let body = borsh::to_vec(&open).map_err(|e| e.to_string())?;
    if body.len() != TOKENIZER_REGISTRATION_OPEN_BODY_BYTES_V1 {
        return Err("native Open body length changed".into());
    }
    let report = serde_json::json!({
        "version": 1, "openId": open.id_v1()?.to_string(), "policyId": open.policy.to_string(),
        "proposedAcceptingDaa": spec.proposed_accepting_daa, "acceptUntilDaa": open.accept_until_daa,
        "finishUntilDaa": open.finish_until_daa, "materialUntilDaa": open.material_until_daa,
        "rawBytes": raw.len(), "parts": quote.geometry.part_count,
        "declaredKeyEvents": spec.declared_key_events, "observedKeyEvents": facts.key_events,
        "localRegistrationChargeSompi": quote.charge, "maxRegistrationChargeSompi": open.max_registration_charge,
        "nativeQuoteRequired": true, "parentPinsAuthenticated": false, "slotsReserved": false,
        "signed": false, "submitted": false, "registrationAccepted": false,
    });
    Ok((body, report))
}

pub(super) fn write(specification: &Path, policy: &Path, raw: &Path, eog: &Path, output: &Path) -> Result<(), String> {
    let spec: Specification = serde_json::from_slice(&read(specification, 16 * 1024)?).map_err(|e| e.to_string())?;
    let policy: TokenizerRegistrationPolicyV1 = borsh::from_slice(&read(policy, 4096)?).map_err(|e| e.to_string())?;
    policy.check_v1()?;
    let raw = read(raw, policy.max_raw_bytes)?;
    let eog = read(eog, u64::from(policy.max_eog_ids) * 4 + 4096)?;
    let (body, report) = build(&spec, &policy, &raw, &eog)?;
    let report_bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    fs::create_dir(output).map_err(|e| format!("{}: {e}", output.display()))?;
    let mut created = Vec::new();
    let result = (|| -> Result<(), String> {
        for (name, bytes) in [("open-body.borsh", body.as_slice()), ("draft.json", report_bytes.as_slice())] {
            let path = output.join(name);
            let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
            created.push(path);
            file.write_all(bytes).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for path in created {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_dir(output);
        return Err(format!("incomplete Open draft at {}: {error}", output.display()));
    }
    println!("{report}");
    Ok(())
}
