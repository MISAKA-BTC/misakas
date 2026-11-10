//! One-control sender. A successful transaction RPC is never an accepted Open,
//! Part or Finish; the operator must read the rooted status before advancing.
use super::{decode_fixed_hex, input_error, read_bounded};
use crate::{CliError, CliResult, keys::KeySource, node::Ctx, wallet::NodeView};
use kaspa_consensus_core::{
    palw_tokenizer_registration_carriage_v1::decode_tokenizer_registration_payload_v1,
    palw_tokenizer_registration_v1::{
        TokenizerRegistrationOpenBodyV1 as Open, TokenizerRegistrationPolicyV1 as Policy, TokenizerRegistrationQuoteV1 as Quote,
        TokenizerRegistrationWireV1 as Wire,
    },
};
use kaspa_rpc_core::{GetPalwTokenizerRegistrationQuoteRequest, GetPalwTokenizerRegistrationStatusRequest, api::rpc::RpcApi};
use serde_json::{Value, json};
use std::path::Path;

struct Admission {
    observation: Value,
    open: Open,
    charge: u64,
    slot: Option<u64>,
}

fn observation(text: &str) -> Result<Value, CliError> {
    if text.len() > kaspa_rpc_core::TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1 {
        return Err(input_error("tokenizer observation exceeds its bound"));
    }
    let value: Value = serde_json::from_str(text).map_err(|e| input_error(e.to_string()))?;
    if value["schema"] != 1 {
        return Err(input_error("unsupported tokenizer observation schema"));
    }
    Ok(value)
}

fn number(value: &Value, field: &str) -> Result<u64, CliError> {
    value[field].as_u64().ok_or_else(|| input_error(format!("tokenizer observation omitted {field}")))
}

fn status_slot(view: &Value, wire: &Wire, policy: &Policy) -> Result<(Open, u64), CliError> {
    let row = &view["registration"];
    let open: Open = decode_fixed_hex(&row["openBodyHex"], "accepted Open")?;
    let id = open.id_v1().map_err(input_error)?;
    if view["openId"] != id.to_string()
        || row["openId"] != id.to_string()
        || row["policyId"] != open.policy.to_string()
        || policy.id_v1().map_err(input_error)? != open.policy
        || row["status"] != "pending"
    {
        return Err(input_error("the accepted registration is not this pending Open"));
    }
    let now = number(view, "prospectiveDaa")?;
    let slot = match wire {
        Wire::Part(part) => {
            part.check_open_v1(&open).map_err(input_error)?;
            let part_row = &row["parts"][part.body.index as usize];
            if number(part_row, "index")? != u64::from(part.body.index)
                || number(part_row, "offset")? != part.body.offset
                || number(part_row, "bytes")? != part.bytes.len() as u64
                || part_row["received"] != false
            {
                return Err(input_error("Part was already accepted or its indexed extent changed"));
            }
            number(part_row, "reservedDaa")?
        }
        Wire::Finish(finish) => {
            finish.check_open_v1(&open, policy).map_err(input_error)?;
            let quote: Quote = decode_fixed_hex(&row["quote"]["quoteHex"], "accepted quote")?;
            let parts = row["parts"].as_array().ok_or_else(|| input_error("accepted Parts missing"))?;
            if parts.len() != quote.geometry.part_count as usize || parts.iter().any(|part| part["received"] != true) {
                return Err(input_error("Finish requires all Parts accepted on the current branch"));
            }
            let slots = row["finishes"].as_array().filter(|s| s.len() == 2).ok_or_else(|| input_error("Finish slots missing"))?;
            slots
                .iter()
                .filter(|slot| slot["attempted"] == false)
                .filter_map(|slot| slot["reservedDaa"].as_u64())
                .find(|slot| *slot >= now)
                .ok_or_else(|| input_error("both immutable Finish opportunities were used or missed"))?
        }
        Wire::Open(_) => return Err(input_error("Open does not use a retained Part/Finish slot")),
    };
    if now > slot {
        return Err(input_error(format!("reserved DAA {slot} has passed; current prospective DAA is {now}")));
    }
    Ok((open, slot))
}

async fn admission(nv: &NodeView, wire: &Wire, policy: &Policy) -> Result<Admission, CliError> {
    if let Wire::Open(control) = wire {
        let body = &control.body;
        let reply = nv
            .client
            .get_palw_tokenizer_registration_quote(GetPalwTokenizerRegistrationQuoteRequest {
                open_body_hex: faster_hex::hex_string(&borsh::to_vec(body).map_err(|e| input_error(e.to_string()))?),
            })
            .await
            .map_err(|e| CliError::connection(e.to_string()))?;
        if !reply.available || !reply.quoted || !reply.refusal.is_empty() {
            return Err(input_error(format!("Open cannot be quoted: {}", reply.refusal)));
        }
        let view = observation(&reply.observation_json)?;
        let quoted_policy: Policy = decode_fixed_hex(&view["policyHex"], "current policy")?;
        let quote: Quote = decode_fixed_hex(&view["quote"]["quoteHex"], "current quote")?;
        if quoted_policy != *policy
            || policy.id_v1().map_err(input_error)? != body.policy
            || view["openId"] != body.id_v1().map_err(input_error)?.to_string()
            || policy.quote_v1(body, number(&view, "prospectiveDaa")?, number(&view, "declaredKeyEvents")?).map_err(input_error)?
                != quote
        {
            return Err(input_error("current quote substitutes the signed Open, policy or fee"));
        }
        return Ok(Admission { observation: view, open: *body, charge: quote.charge, slot: None });
    }
    let id = match wire {
        Wire::Part(part) => part.body.open_id,
        Wire::Finish(finish) => finish.body.open_id,
        _ => unreachable!(),
    };
    let reply = nv
        .client
        .get_palw_tokenizer_registration_status(GetPalwTokenizerRegistrationStatusRequest { open_id: id.to_string() })
        .await
        .map_err(|e| CliError::connection(e.to_string()))?;
    if !reply.available || !reply.found {
        return Err(input_error("Open has no retained accepted state on the current branch"));
    }
    let view = observation(&reply.observation_json)?;
    let (open, slot) = status_slot(&view, wire, policy)?;
    Ok(Admission { observation: view, open, charge: 0, slot: Some(slot) })
}

pub(super) async fn run(ctx: &Ctx, payload: &Path, saved_quote: &Path, keys: KeySource, maximum_fee: u64, submit: bool) -> CliResult {
    let quote_bytes = read_bounded(saved_quote, kaspa_rpc_core::TOKENIZER_REGISTRATION_OBSERVATION_MAX_BYTES_V1 as u64)?;
    let saved = observation(std::str::from_utf8(&quote_bytes).map_err(|e| input_error(e.to_string()))?)?;
    let policy: Policy = decode_fixed_hex(&saved["policyHex"], "saved policy")?;
    policy.check_v1().map_err(input_error)?;
    // Every v1 control is smaller than this local file ceiling (Parts have
    // 64 KiB data; Finish has at most 4096 EOG IDs). Native decoding retains
    // its own policy and field bounds below this outer ceiling.
    let payload = read_bounded(payload, policy.max_carrier_bytes.min(128 * 1024))?;
    let wire = decode_tokenizer_registration_payload_v1(&payload, &policy)
        .map_err(input_error)?
        .ok_or_else(|| input_error("expected a TKR1 payload"))?;
    let nv = crate::wallet::connect(ctx).await?;
    let read = admission(&nv, &wire, &policy).await?;
    if saved["openId"] != read.open.id_v1().map_err(input_error)?.to_string() {
        return Err(input_error("saved quote belongs to a different Open"));
    }
    let now = number(&read.observation, "prospectiveDaa")?;
    if submit && read.slot.is_some_and(|slot| slot != now) {
        return Err(input_error(format!("control is booked at DAA {}, current prospective DAA is {now}", read.slot.unwrap())));
    }
    let bond = read.open.opener.0;
    let lookup = crate::bond::bond_lookup_class(&nv)?;
    let facts = nv
        .client
        .get_palw_producer_facts(lookup.clone(), bond.transaction_id.to_string(), bond.index, true)
        .await
        .map_err(|e| CliError::connection(e.to_string()))?;
    crate::bond::bond_facts_answered(facts.available, &lookup, &format!("{}:{}", bond.transaction_id, bond.index))?;
    if !facts.bond_known || facts.bond_registered_pubkey.len() != 2 * kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN {
        return Err(input_error("accepted opener public key is unavailable"));
    }
    let minimum_collateral = match &nv.params.palw_consensus_mode {
        kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.state.min_collateral_sompi(),
        _ => return Err(input_error("tokenizer registration requires PALW consensus V2")),
    };
    if facts.bond_collateral < minimum_collateral {
        return Err(input_error("opener collateral is below the native registration minimum"));
    }
    let bond_text = format!("{}:{}", bond.transaction_id, bond.index);
    let detail = nv
        .client
        .get_palw_bond_detail(kaspa_rpc_core::GetPalwBondDetailRequest { bond: bond_text.clone() })
        .await
        .map_err(|e| CliError::connection(e.to_string()))?;
    if !detail.available || !detail.found || detail.view_json.len() > 4 * 1024 * 1024 {
        return Err(input_error("bounded current opener status is unavailable"));
    }
    let detail: Value = serde_json::from_str(&detail.view_json).map_err(|e| input_error(e.to_string()))?;
    if detail["kind"] != "palwBondDetail" || detail["bond"] != bond_text || detail["status"] != "active" {
        return Err(input_error("opener is not an active bond on the current branch"));
    }
    let mut public_key = vec![0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
    faster_hex::hex_decode(facts.bond_registered_pubkey.as_bytes(), &mut public_key)
        .map_err(|_| input_error("invalid opener public key"))?;
    match &wire {
        Wire::Open(control) => control.verify_signature_v1(&public_key),
        Wire::Part(control) => control.verify_signature_v1(&public_key),
        Wire::Finish(control) => control.verify_signature_v1(&public_key),
    }
    .map_err(input_error)?;
    let floor = kaspa_pq_validator_core::PALW_TX_FEE_FLOOR_SOMPI;
    let provisional = read
        .charge
        .checked_add(floor)
        .filter(|fee| *fee <= maximum_fee)
        .ok_or_else(|| input_error("registration charge plus ordinary fee exceeds --max-fee"))?;
    let key = keys.load_key()?;
    let address = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::lifecycle_candidates_v1(&nv, &address).await?;
    let (outpoint, entry) = candidates
        .iter()
        .find(|(_, entry)| entry.amount > provisional)
        .ok_or_else(|| input_error("no spendable UTXO covers this control"))?;
    let probe = key.build_palw_tokenizer_registration_tx(&wire, *outpoint, entry, provisional).map_err(input_error)?;
    let p = &nv.params;
    let mass = kaspa_consensus_core::mass::MassCalculator::new(
        p.mass_per_tx_byte,
        p.mass_per_script_pub_key_byte,
        p.mass_per_sig_op,
        p.storage_mass_parameter,
    )
    .calc_non_contextual_masses(&probe)
    .compute_mass;
    let ordinary = kaspa_pq_validator_core::relay_fee_for_compute_mass(mass).max(floor);
    let fee = read
        .charge
        .checked_add(ordinary)
        .filter(|fee| *fee <= maximum_fee)
        .ok_or_else(|| input_error("registration charge plus priced carriage fee exceeds --max-fee"))?;
    let (outpoint, entry) = candidates
        .iter()
        .find(|(_, entry)| entry.amount > fee)
        .ok_or_else(|| input_error("no spendable UTXO covers the complete priced fee"))?;
    let tx = key.build_palw_tokenizer_registration_tx(&wire, *outpoint, entry, fee).map_err(input_error)?;
    if submit {
        let fresh = admission(&nv, &wire, &policy).await?;
        if fresh.observation != read.observation || fresh.charge != read.charge || fresh.slot != read.slot {
            return Err(input_error("accepted parent, quote or reservation changed before submission; retry from the new view"));
        }
        let submitted_id = nv.client.submit_transaction(tx.as_ref().into(), false).await.map_err(|e| {
            CliError::connection(format!(
                "submission result unknown for transaction {}: {e}; inspect its mempool/accepted state before retrying",
                tx.id()
            ))
        })?;
        if submitted_id != tx.id() {
            return Err(input_error(format!(
                "submission returned a different transaction ID; inspect local transaction {} before retrying",
                tx.id()
            )));
        }
    }
    println!(
        "{}",
        json!({"submitted": submit, "acceptanceConfirmed": false, "txid": tx.id().to_string(),
        "openId": read.open.id_v1().map_err(input_error)?.to_string(), "reservedDaa": read.slot,
        "prospectiveDaa": now, "totalFeeSompi": fee, "ordinaryFeeSompi": ordinary,
        "registrationChargeSompi": read.charge, "acceptedParent": read.observation["acceptedParent"],
        "acceptedParentRoot": read.observation["acceptedParentRoot"]})
    );
    let _ = nv.client.disconnect().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        Hash64,
        palw_court_service_v1::ServiceCost,
        palw_fp_constraint_job_v1::PalwTokenTableV1,
        palw_state_v2::PalwBondKeyV2,
        palw_tokenizer_registration_v1::{
            TokenizerRegistrationFinishBodyV1, TokenizerRegistrationFinishV1, TokenizerRegistrationPartBodyV1,
            TokenizerRegistrationPartV1,
        },
        palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1,
        palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2},
        tx::TransactionOutpoint,
    };

    fn fixture() -> (Policy, Wire, Wire, Value) {
        let h = Hash64::from_u64_word;
        let policy = Policy {
            version: 1,
            max_raw_bytes: 65537,
            max_vocab: 8,
            max_key_events: 32,
            max_eog_ids: 2,
            max_rendering_bytes: 16,
            max_parser_scratch_bytes: 10_000_000,
            max_carrier_bytes: 80_000,
            max_retained_bytes: 1_000_000,
            max_retained_total_bytes: 2_000_000,
            service_budget: ServiceCost { bytes: 100_000, work: 70_000_000, io: 1_000_000, controls: 1 },
            maximum_source_blocks: 1,
            max_work: 70_000_000,
            max_pending_per_opener: 1,
            max_pending_total: 2,
            max_session_daa: 100,
            retention_daa: 20,
            carrier_overhead_bytes: 4,
            retention_copies: 2,
            metadata_bytes: 64_000,
            work_rates: [1; 7],
            bookkeeping_work_per_step: 1,
            rent_per_carrier_byte: 1,
            rent_per_retained_byte_daa: 1,
            rent_per_work: 1,
            max_registration_charge: 100_000_000,
        };
        let table = PalwTokenTableV1 { entries: vec![Some(b"A".to_vec())], eog_token_ids: vec![0], tokenizer_id: h(7) };
        let eog = table.eog_opening().unwrap();
        let open = Open {
            version: 1,
            network: h(1),
            policy: policy.id_v1().unwrap(),
            pins: PalwTfp1TokenizerIntakePinsV2 { class_id: h(2), container_root: h(3), semantics_id: h(4), inventory_id: h(5) },
            artifact_root: h(6),
            statement: PalwTokenizerRenderingStatementV2 {
                tokenizer_id: h(7),
                exact_raw_bytes: 1,
                table_root: table.root(),
                vocab: 1,
                rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
            },
            eog_digest: TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap(),
            opener: PalwBondKeyV2(TransactionOutpoint::new(h(8), 0)),
            nonce: h(9),
            accept_until_daa: 40,
            finish_until_daa: 80,
            material_until_daa: 100,
            max_registration_charge: 100_000_000,
        };
        let id = open.id_v1().unwrap();
        let part = Wire::Part(TokenizerRegistrationPartV1 {
            body: TokenizerRegistrationPartBodyV1 {
                version: 1,
                network: open.network,
                policy: open.policy,
                open_id: id,
                opener: open.opener,
                index: 0,
                offset: 0,
                payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(b"A").unwrap(),
            },
            signature: [0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            bytes: b"A".to_vec(),
        });
        let finish = Wire::Finish(TokenizerRegistrationFinishV1 {
            body: TokenizerRegistrationFinishBodyV1 {
                version: 1,
                network: open.network,
                policy: open.policy,
                open_id: id,
                opener: open.opener,
                eog_digest: open.eog_digest,
                finish_until_daa: open.finish_until_daa,
            },
            signature: [0; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            eog,
        });
        let view = json!({"schema": 1, "openId": id.to_string(), "prospectiveDaa": 21, "registration": {
            "openId": id.to_string(), "openBodyHex": faster_hex::hex_string(&borsh::to_vec(&open).unwrap()),
            "policyId": open.policy.to_string(), "status": "pending",
            "quote": {"quoteHex": faster_hex::hex_string(&borsh::to_vec(&policy.quote_v1(&open, 20, 32).unwrap()).unwrap())},
            "parts": [{"index": 0, "offset": 0, "bytes": 1, "reservedDaa": 21, "received": false}],
            "finishes": [{"reservedDaa": 22, "attempted": false}, {"reservedDaa": 23, "attempted": false}]
        }});
        (policy, part, finish, view)
    }

    #[test]
    fn tokenizer_registration_sender_preserves_slots_and_requires_accepted_parts() {
        let (policy, part, finish, view) = fixture();
        assert_eq!(status_slot(&view, &part, &policy).unwrap().1, 21);
        assert!(status_slot(&view, &finish, &policy).is_err());
        let mut late = view.clone();
        late["prospectiveDaa"] = json!(22);
        assert!(status_slot(&late, &part, &policy).is_err());
        let mut received = view.clone();
        received["registration"]["parts"][0]["received"] = json!(true);
        assert!(status_slot(&received, &part, &policy).is_err());
        assert_eq!(status_slot(&received, &finish, &policy).unwrap().1, 22);
        received["prospectiveDaa"] = json!(23);
        assert_eq!(status_slot(&received, &finish, &policy).unwrap().1, 23);
        received["prospectiveDaa"] = json!(24);
        assert!(status_slot(&received, &finish, &policy).is_err());
        let mut consumed = view.clone();
        consumed["registration"]["parts"][0]["received"] = json!(true);
        consumed["registration"]["finishes"][0]["attempted"] = json!(true);
        assert_eq!(status_slot(&consumed, &finish, &policy).unwrap().1, 23);
        consumed["registration"]["finishes"][1]["attempted"] = json!(true);
        assert!(status_slot(&consumed, &finish, &policy).is_err());
        let mut foreign = view.clone();
        foreign["registration"]["openId"] = json!(Hash64::from_u64_word(99).to_string());
        assert!(status_slot(&foreign, &part, &policy).is_err());
        let mut complete = view;
        complete["registration"]["status"] = json!("complete");
        assert!(status_slot(&complete, &finish, &policy).is_err());
    }
}
