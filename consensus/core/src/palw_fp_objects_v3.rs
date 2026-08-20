//! From accepted transactions to consensus objects (ADR-0044 Unit C, item 3).
//!
//! One chain block's accepted free-prompt commitment transactions become the
//! `FreePromptCommitted` objects its PALW transition folds — in the block's deterministic
//! acceptance order, because the transition's ordering IS consensus.
//!
//! # What this layer decides, and what it refuses to decide
//!
//! It decides SHAPE and PRICE from facts the transaction carries: the payload decodes, the
//! carried prompt is the one the commitment binds, the CU is the bundle's price for the executed
//! shape, and the quanta/pwu derivation is the bundle's. All of that is a pure function of the
//! transaction and the ruleset, so every node computes it identically.
//!
//! It does NOT decide anything requiring chain state — that the executor's bond exists and is
//! Active, that its key is the carried key, that the class is registered and unfrozen. Those are
//! the transition's own referential checks (it refuses an object naming an absent bond or a
//! frozen class) and admission's. Splitting it this way keeps the extraction a pure function that
//! a test can drive without a chain.
//!
//! # Why a malformed carrier is SKIPPED, not fatal
//!
//! A transaction whose payload does not decode, or whose price does not check, contributes no
//! object — it does not reject the block. That mirrors `palw_carriage_records_from_accepted_txs`
//! beside it, and it is the right shape here for a specific reason: transaction-level validity is
//! the transaction validator's job (a network running this ruleset rejects such a transaction at
//! acceptance), while this walk must be a total function of whatever DID get accepted. A walk
//! that could panic or reject on a peer-supplied payload would be a remote denial of service
//! wearing a consensus rule's clothes.
//!
//! The skipped-object count is returned so a caller can log it: silently dropping a carrier that
//! a peer thought was valid is exactly the "reads as nothing" failure ADR-0042 Decision 5 warns
//! about, and the count is how an operator sees it happening.

use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFreePromptParamsV3, fp_cu_v3};
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use crate::tx::Transaction;
use crate::{BlockHash, Hash64};

/// What one block's extraction produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwFpExtractionV3 {
    /// The objects, in acceptance order.
    pub objects: Vec<PalwConsensusObjectV3Carrier>,
    /// Carriers routed to the free-prompt subnetwork that produced no object, and why — for the
    /// log line that keeps a silent drop from being silent.
    pub skipped: Vec<(crate::tx::TransactionId, &'static str)>,
}

/// An extracted object plus the transaction that carried it, so a caller can attribute it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwConsensusObjectV3Carrier {
    pub carrier: crate::tx::TransactionId,
    pub object: PalwConsensusObjectV2,
}

/// Extract one chain block's free-prompt objects.
///
/// `network_domain` is the network's own (see `palw_network_domain_v2`); a payload naming another
/// network produces nothing here, exactly as it would be refused at admission.
pub fn palw_fp_objects_from_accepted_txs_v3(
    txs: &[Transaction],
    network_domain: Hash64,
    freeprompt: &PalwFreePromptParamsV3,
    _accepted_block: BlockHash,
) -> PalwFpExtractionV3 {
    let mut out = PalwFpExtractionV3::default();
    for tx in txs {
        if tx.subnetwork_id != SUBNETWORK_ID_PALW_FP_COMMITMENT {
            continue;
        }
        let id = tx.id();
        let payload: PalwFpCommitmentTxPayloadV3 = match borsh::from_slice(&tx.payload) {
            Ok(payload) => payload,
            Err(_) => {
                out.skipped.push((id, "payload does not decode"));
                continue;
            }
        };
        // The same stateless rules a peer applies — re-run here rather than assumed, because this
        // walk must be total over whatever was accepted.
        if payload.validate_stateless_v3(network_domain, freeprompt.cu_weights()).is_err() {
            out.skipped.push((id, "payload is not stateless-admissible"));
            continue;
        }
        // The price is re-derived, never read: the payload's `cu` was checked against the shape
        // above, and the quanta/pwu split is the bundle's alone (invariant F7).
        let commitment = &payload.commitment;
        let derived_cu = fp_cu_v3(commitment.job.prompt_tokens, commitment.decode_tokens_executed, freeprompt.cu_weights());
        let Some((quanta, pwu)) = freeprompt.derive_quanta_and_pwu(derived_cu) else {
            // Sub-quantum work certifies nothing the chain can act on, so it never becomes a
            // claim — the state machine would refuse a zero-quanta object anyway, and refusing
            // it here keeps the reason readable.
            out.skipped.push((id, "job earns no quanta"));
            continue;
        };
        out.objects.push(PalwConsensusObjectV3Carrier {
            carrier: id,
            object: PalwConsensusObjectV2::FreePromptCommitted {
                claim: payload.claim_id(),
                class_id: commitment.job.class_id,
                bond: PalwBondKeyV2(commitment.job.executor_bond),
                pwu,
                quanta,
                trace_root: commitment.trace_root,
                output_root: commitment.output_root,
            },
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::TX_VERSION;
    use crate::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_V3_VERSION, PalwFpStopReasonV3, PalwFreePromptCommitmentV3, PalwFreePromptJobV3,
        fp_trace_manifest_v3,
    };
    use crate::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_COMMITMENT};
    use crate::tx::{Transaction, TransactionId, TransactionOutpoint};
    use kaspa_hashes::Hash64 as H;

    fn h64(v: u64) -> Hash64 {
        H::from_u64_word(v)
    }

    fn net() -> Hash64 {
        h64(0x4E)
    }

    fn freeprompt() -> PalwFreePromptParamsV3 {
        crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_v3(h64(1), h64(0xCA7), h64(0xC0757)).unwrap().freeprompt
    }

    /// A commitment whose CU earns real quanta under the devnet bundle.
    fn payload(prompt_tokens: u32, decode: u32) -> PalwFpCommitmentTxPayloadV3 {
        let fp = freeprompt();
        let ids: Vec<u32> = (0..prompt_tokens).collect();
        let job = PalwFreePromptJobV3 {
            version: PALW_FP_V3_VERSION,
            network_domain: net(),
            class_id: h64(1),
            executor_bond: TransactionOutpoint { transaction_id: TransactionId::from_u64_word(7), index: 0 },
            executor_pubkey: vec![7; 32],
            operator_id: h64(0xE0),
            anchor_block: h64(0xA0),
            anchor_daa: 5_000,
            job_nonce: [0x11; 32],
            tokenizer_id: h64(0x70),
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(&ids),
            prompt_tokens,
            decode_token_limit: decode.max(1) + 1,
            max_context_tokens: 4_096,
            privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        };
        let events: Vec<Hash64> = (0..decode as u64).map(|i| h64(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(h64(0xB1), &events);
        let commitment = PalwFreePromptCommitmentV3 {
            trace_root: h64(0x7A),
            output_root: h64(0x0B),
            schedule_root: h64(0x5C),
            decode_tokens_executed: decode,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            cu: fp_cu_v3(prompt_tokens, decode, fp.cu_weights()),
            trace_manifest_root: manifest_root,
            trace_chunk_count: chunk_count,
            trace_retention_daa: 505_000,
            job,
        };
        PalwFpCommitmentTxPayloadV3 {
            version: PALW_FP_V3_VERSION,
            commitment,
            prompt_token_ids: ids,
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    fn tx(subnetwork: crate::subnets::SubnetworkId, payload_bytes: Vec<u8>) -> Transaction {
        Transaction::new(TX_VERSION, vec![], vec![], 0, subnetwork, 0, payload_bytes)
    }

    /// The happy path: an accepted commitment becomes exactly one object, priced by the bundle
    /// and NOT by the payload's own claim.
    #[test]
    fn an_accepted_commitment_becomes_one_priced_object() {
        let fp = freeprompt();
        let p = payload(96, 256);
        let expected_claim = p.claim_id();
        let expected_cu = fp_cu_v3(96, 256, fp.cu_weights());
        let (expected_quanta, expected_pwu) = fp.derive_quanta_and_pwu(expected_cu).unwrap();

        let extracted =
            palw_fp_objects_from_accepted_txs_v3(&[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap())], net(), &fp, h64(1));
        assert_eq!(extracted.objects.len(), 1);
        assert!(extracted.skipped.is_empty());
        match &extracted.objects[0].object {
            PalwConsensusObjectV2::FreePromptCommitted { claim, class_id, bond, pwu, quanta, trace_root, output_root } => {
                assert_eq!(*claim, expected_claim);
                assert_eq!(*class_id, h64(1));
                assert_eq!(bond.0.transaction_id, TransactionId::from_u64_word(7));
                assert_eq!((*quanta, *pwu), (expected_quanta, expected_pwu));
                assert_eq!(*trace_root, h64(0x7A));
                assert_eq!(*output_root, h64(0x0B));
            }
            other => panic!("expected a free-prompt object, got {other:?}"),
        }
    }

    /// **The price is the bundle's.** A payload claiming a different CU does not get it — it is
    /// refused outright (the stateless rule re-derives CU), so an inflated claim never becomes an
    /// object at all.
    #[test]
    fn an_inflated_price_never_becomes_an_object() {
        let fp = freeprompt();
        let mut p = payload(96, 256);
        p.commitment.cu *= 10;
        let extracted =
            palw_fp_objects_from_accepted_txs_v3(&[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap())], net(), &fp, h64(1));
        assert!(extracted.objects.is_empty());
        assert_eq!(extracted.skipped, vec![(tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap()).id(), "payload is not stateless-admissible")]);
    }

    /// Every skip reason is reachable and NAMED, so an operator can see a carrier being dropped:
    /// undecodable bytes, a foreign network, and sub-quantum work.
    #[test]
    fn every_skip_reason_is_named() {
        let fp = freeprompt();

        let junk = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, vec![0xFF; 32]);
        let out = palw_fp_objects_from_accepted_txs_v3(&[junk.clone()], net(), &fp, h64(1));
        assert_eq!(out.skipped, vec![(junk.id(), "payload does not decode")]);

        let foreign = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&payload(96, 256)).unwrap());
        let out = palw_fp_objects_from_accepted_txs_v3(&[foreign.clone()], h64(0x99), &fp, h64(1));
        assert_eq!(out.skipped, vec![(foreign.id(), "payload is not stateless-admissible")], "a foreign network's payload");

        // 8 prompt tokens and 4 decode tokens is far under one quantum.
        let tiny = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&payload(8, 4)).unwrap());
        let out = palw_fp_objects_from_accepted_txs_v3(&[tiny.clone()], net(), &fp, h64(1));
        assert_eq!(out.skipped, vec![(tiny.id(), "job earns no quanta")]);
    }

    /// Only the free-prompt subnetwork is read, and acceptance ORDER is preserved — the
    /// transition's ordering is consensus, so the extraction must not reorder or interleave.
    #[test]
    fn only_this_subnetwork_is_read_and_order_is_preserved() {
        let fp = freeprompt();
        let first = payload(96, 256);
        let second = payload(128, 192);
        let txs = vec![
            tx(SUBNETWORK_ID_NATIVE, vec![]),
            tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&first).unwrap()),
            // A V2-lineage carriage id is a different band with a different validator — reading
            // it here would hand one band's payload to the other band's rules.
            tx(SUBNETWORK_ID_PALW_COMMITMENT, borsh::to_vec(&first).unwrap()),
            tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&second).unwrap()),
        ];
        let out = palw_fp_objects_from_accepted_txs_v3(&txs, net(), &fp, h64(1));
        assert_eq!(out.objects.len(), 2, "two free-prompt carriers, and only those");
        assert!(out.skipped.is_empty());
        let claims: Vec<Hash64> = out
            .objects
            .iter()
            .map(|c| match c.object {
                PalwConsensusObjectV2::FreePromptCommitted { claim, .. } => claim,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(claims, vec![first.claim_id(), second.claim_id()], "acceptance order is preserved");
    }
}
