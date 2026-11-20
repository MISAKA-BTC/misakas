//! Validate the receipt envelope captured by protected_claim_observation.py.
//! Uses native header hashing, PFS3 decoding and signature authentication.
//! Stateful admission/winning status remains an observation of the native node;
//! this tool is neither a consensus implementation nor a computation proof.
use kaspa_consensus_core::{
    Hash64,
    hashing::header::{hash, pre_pow_hash_64},
    header::{CompressedParents, Header},
    palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3,
    palw_protected_original_readback_v1::ProtectedOriginalReadbackV1,
};
use serde_json::{Value, json};
use std::{fs, io::Read, path::Path};

fn check(ok: bool, why: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(why.into()) }
}

fn hash_field(value: &Value) -> Result<Hash64, String> {
    serde_json::from_value(value.clone()).map_err(|e| e.to_string())
}

fn receipt_header(value: &Value) -> Result<Header, String> {
    let mut value = value.clone();
    // RPC expands levels; native Header serializes CompressedParents.
    let levels: Vec<Vec<Hash64>> = serde_json::from_value(value["parentsByLevel"].clone()).map_err(|e| e.to_string())?;
    let parents = CompressedParents::try_from(levels).map_err(|e| e.to_string())?;
    value["parentsByLevel"] = serde_json::to_value(parents).map_err(|e| e.to_string())?;
    let header: Header = serde_json::from_value(value).map_err(|e| e.to_string())?;
    check(hash(&header) == header.hash, "receipt block header hash differs")?;
    check(header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3, "not a PFS3 receipt block")?;
    Ok(header)
}

fn envelope(header: &Header, domain: Hash64, claim: Hash64) -> Result<PalwReceiptSpendEnvelopeV3, String> {
    let envelope = PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment).map_err(|e| e.to_string())?;
    check(envelope.spend.claim_id == claim, "receipt spends another claim")?;
    envelope.validate_stateless_v3(domain, pre_pow_hash_64(header), header.timestamp, header.nonce).map_err(|e| e.to_string())?;
    envelope.validate_signature_v3(palw_staged_verify_mldsa87_v1).map_err(|e| e.to_string())?;
    Ok(envelope)
}

fn audit(observed: &Value) -> Result<Value, String> {
    check(
        observed["version"] == 1 && observed["kind"] == "protectedClaimObservation" && observed["source"] == "node-rpc",
        "unknown observation schema",
    )?;
    let expected = &observed["expected"];
    let detail = &observed["detail"];
    check(
        observed["context"][3] == expected["genesisHash"] && observed["context"][4] == expected["consensusParamsId"],
        "captured chain/profile differs from expectation",
    )?;
    let claim = hash_field(&expected["claim"])?;
    let read: ProtectedOriginalReadbackV1 = serde_json::from_value(detail["protectedOriginal"].clone()).map_err(|e| e.to_string())?;
    read.check_carrier_v1(claim, hash_field(&expected["transactionId"])?, hash_field(&expected["executionRoot"])?)?;
    for field in [
        "claim",
        "executionRoot",
        "original",
        "carrierTransaction",
        "carrierBlock",
        "acceptingBlock",
        "acceptedTxOrdinal",
        "eventOrdinal",
        "acceptedDaa",
        "acceptedBlueScore",
    ] {
        check(observed["origin"][field] == detail["protectedOriginal"][field], "captured original differs from readback")?;
    }
    check(
        hash_field(&detail["tipBlock"])? == read.observed_tip && observed["context"][0] == detail["tipBlock"],
        "readback and captured sink differ",
    )?;
    check(read.full_replay_approval_recorded && !read.public_refutation_recorded, "approval missing or public refutation recorded")?;
    check(detail["claim"]["claimId"] == expected["claim"] && detail["claim"]["phase"] == "final", "same claim is not Final")?;
    let block = &observed["receiptBlock"];
    check(
        block["verboseData"]["isChainBlock"] == true && block["verboseData"]["isHeaderOnly"] == false,
        "receipt not observed as a selected validated block",
    )?;
    let header = receipt_header(&block["header"])?;
    check(hash_field(&block["verboseData"]["hash"])? == header.hash, "verbose receipt identity differs")?;
    check(read.observed_tip_daa >= header.daa_score, "claim readback predates receipt")?;
    let envelope = envelope(&header, hash_field(&expected["networkDomain"])?, claim)?;
    let bond = envelope.spend.producer_bond;
    check(
        detail["claim"]["executorBond"] == format!("{}:{}", bond.transaction_id, bond.index),
        "receipt producer differs from claim executor",
    )?;
    let quanta = &detail["receiptQuanta"];
    let indices: Vec<u32> = serde_json::from_value(quanta["spentIndices"].clone()).map_err(|e| e.to_string())?;
    check(quanta["spentIndicesComplete"] == true, "spent index observation is truncated")?;
    let unique: std::collections::BTreeSet<_> = indices.iter().copied().collect();
    check(unique.len() == indices.len() && quanta["spentCount"].as_u64() == Some(indices.len() as u64), "spent index counts differ")?;
    check(detail["claim"]["quantaSpent"].as_u64() == Some(indices.len() as u64), "claim spent count differs")?;
    check(unique.contains(&envelope.spend.quantum_index), "receipt quantum is not recorded spent")?;
    check(quanta["total"].as_u64().is_some_and(|n| u64::from(envelope.spend.quantum_index) < n), "receipt quantum outside claim")?;
    Ok(json!({
        "version": 1, "kind": "protectedReceiptObservationAudit", "claim": claim,
        "block": header.hash, "quantumIndex": envelope.spend.quantum_index,
        "beaconBlock": envelope.spend.beacon_block, "headerHashChecked": true,
        "nativeReceiptEnvelopeValidated": true, "spentIndexObserved": true,
        "statefulChecksSource": "native selected-chain RPC observation",
        "actualPanelWorkerExecutionVerified": false, "actualClaimCompletion": false,
    }))
}

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    check(args.len() == 3, "usage: protected-receipt-observe observation.json fresh-report.json")?;
    let mut input = Vec::new();
    fs::File::open(&args[1]).map_err(|e| e.to_string())?.take((64 << 20) + 1).read_to_end(&mut input).map_err(|e| e.to_string())?;
    check(input.len() <= 64 << 20, "observation exceeds local 64 MiB bound")?;
    let observed: Value = serde_json::from_slice(&input).map_err(|e| e.to_string())?;
    let report = audit(&observed)?;
    let output = fs::OpenOptions::new().write(true).create_new(true).open(Path::new(&args[2])).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(output, &report).map_err(|e| e.to_string())?;
    println!("{report}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_V3_MLDSA87_SPEND_CONTEXT, PALW_FP_V3_VERSION, PalwReceiptSpendUnsignedV3, fp_spend_id_v3, spend_challenge_v3,
    };
    #[test]
    fn native_receipt_binds_header_claim_and_signature() {
        let domain = Hash64::from_u64_word(1);
        let claim = Hash64::from_u64_word(2);
        let mut header = Header::new_finalized(
            2,
            vec![vec![domain]].try_into().unwrap(),
            domain,
            domain,
            domain,
            123456,
            0x207fffff,
            42,
            7,
            4400,
            0.into(),
            4400,
            domain,
        );
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x55; 32]);
        let bond = kaspa_consensus_core::tx::TransactionOutpoint { transaction_id: domain, index: 0 };
        let spend = PalwReceiptSpendUnsignedV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: domain,
            challenge: spend_challenge_v3(domain, pre_pow_hash_64(&header), header.timestamp, header.nonce, claim, 0, &bond),
            claim_id: claim,
            quantum_index: 0,
            beacon_block: domain,
            producer_bond: bond,
            producer_pubkey: key.verification_key.as_ref().to_vec(),
        };
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            fp_spend_id_v3(&spend).as_byte_slice(),
            PALW_FP_V3_MLDSA87_SPEND_CONTEXT,
            [0; 32],
        )
        .unwrap();
        let receipt = PalwReceiptSpendEnvelopeV3 { spend, signature: signature.as_ref().to_vec() };
        header.palw_commitment = receipt.encode();
        header.hash = hash(&header);
        let mut rpc = serde_json::to_value(&header).unwrap();
        rpc["parentsByLevel"] = json!(Vec::<Vec<Hash64>>::from(&header.parents_by_level));
        let decoded = receipt_header(&rpc).unwrap();
        assert_eq!(envelope(&decoded, domain, claim).unwrap(), receipt);
        let read = ProtectedOriginalReadbackV1 {
            version: 1,
            claim,
            execution_root: domain,
            original: domain,
            carrier_transaction: domain,
            carrier_block: domain,
            accepting_block: domain,
            accepted_tx_ordinal: 1,
            event_ordinal: 0,
            accepted_daa: 100,
            accepted_blue_score: 100,
            observed_tip: header.hash,
            observed_tip_daa: 4400,
            full_replay_approval_recorded: true,
            public_refutation_recorded: false,
        };
        // Constructed observation only; this validates the reader, not a chain.
        let observed = json!({
            "version": 1, "kind": "protectedClaimObservation", "source": "node-rpc",
            "expected": {"claim": claim, "transactionId": domain, "executionRoot": domain,
                "networkDomain": domain, "genesisHash": domain, "consensusParamsId": domain},
            "context": [header.hash, domain, 4401, domain, domain], "origin": read,
            "detail": {"tipBlock": header.hash, "protectedOriginal": read,
                "claim": {"claimId": claim, "phase": "final", "executorBond": format!("{domain}:0"), "quantaSpent": 1},
                "receiptQuanta": {"total": 1, "spentCount": 1, "spentIndices": [0], "spentIndicesComplete": true}},
            "receiptBlock": {"header": rpc, "verboseData": {"hash": header.hash, "isChainBlock": true, "isHeaderOnly": false}},
        });
        assert_eq!(audit(&observed).unwrap()["nativeReceiptEnvelopeValidated"], true);
        for (pointer, replacement) in [
            ("/origin/eventOrdinal", json!(1)),
            ("/detail/protectedOriginal/fullReplayApprovalRecorded", json!(false)),
            ("/detail/protectedOriginal/publicRefutationRecorded", json!(true)),
            ("/detail/claim/phase", json!("provisional")),
            ("/detail/receiptQuanta/spentIndices", json!([])),
            ("/detail/receiptQuanta/spentIndicesComplete", json!(false)),
            ("/receiptBlock/verboseData/isChainBlock", json!(false)),
        ] {
            let mut wrong = observed.clone();
            *wrong.pointer_mut(pointer).unwrap() = replacement;
            assert!(audit(&wrong).is_err(), "{pointer}");
        }
        assert!(envelope(&decoded, domain, Hash64::from_u64_word(3)).is_err());
        assert!(envelope(&decoded, Hash64::from_u64_word(3), claim).is_err());
        rpc["nonce"] = json!(43);
        assert!(receipt_header(&rpc).is_err());
        let mut moved = header.clone();
        moved.nonce += 1;
        assert!(envelope(&moved, domain, claim).is_err());
        let mut damaged = receipt;
        damaged.signature[0] ^= 1;
        header.palw_commitment = damaged.encode();
        assert!(envelope(&header, domain, claim).is_err());
    }
}
