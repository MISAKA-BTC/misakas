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

use crate::palw_freeprompt_v3::{PalwFpCommitmentTxPayloadV3, PalwFreePromptParamsV3};
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
pub fn palw_fp_objects_from_accepted_txs_v3<V>(
    txs: &[Transaction],
    network_domain: Hash64,
    // Kept on the signature so every caller still hands the walk the bundle it reads under; the
    // price it used to derive from these params is the transition's now (ADR-0074 Decision 5).
    freeprompt: &PalwFreePromptParamsV3,
    accepted_block: BlockHash,
    // **The ML-DSA-87 verifier, and it is not optional.** Without it this walk turned any
    // stranger's 0x4a transaction into a claim bound to any bond outpoint it named — including the
    // genesis premine bond, a published constant — because the commitment's signature was checked
    // on no path in the tree. Taking it as an argument rather than leaving it to a caller is what
    // makes "somebody else verifies it" unrepresentable; the previous arrangement said exactly that
    // in a doc comment, and nobody did.
    verify_mldsa87: V,
) -> PalwFpExtractionV3
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    // `false`: this entry cannot resolve a height, so it answers the ADR-0077 Decision 16 arming
    // the way every build answered it before the fence existed. A caller that holds the block's
    // DAA score calls [`palw_fp_objects_from_accepted_txs_under_v3`] instead.
    palw_fp_objects_from_accepted_txs_under_v3(txs, network_domain, freeprompt, accepted_block, false, verify_mldsa87)
}

/// The same walk **under this block's `PanelDa` arming** (ADR-0077 Decision 16).
///
/// `panel_da_armed` is `Params::palw_panel_da_at(block_daa)` — the accepting block's own DAA
/// score, not the node's tip, for the reason every rule on this lane is candidate-scoped: two
/// nodes folding the same block must fold it identically however far either has synced.
///
/// A separate entry rather than a parameter on the old one so the processor's switch to it is a
/// one-line, reviewable change, and so a caller that has NOT switched keeps refusing mode 2
/// rather than silently acquiring it.
pub fn palw_fp_objects_from_accepted_txs_under_v3<V>(
    txs: &[Transaction],
    network_domain: Hash64,
    freeprompt: &PalwFreePromptParamsV3,
    accepted_block: BlockHash,
    panel_da_armed: bool,
    verify_mldsa87: V,
) -> PalwFpExtractionV3
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    // No bundle in scope, so no ladder: the structural top, exactly as the door uses it. A caller
    // that holds the bundle calls [`palw_fp_objects_from_accepted_txs_under_ruleset_v3`].
    palw_fp_objects_from_accepted_txs_under_ruleset_v3(
        txs,
        network_domain,
        freeprompt,
        accepted_block,
        panel_da_armed,
        crate::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP,
        verify_mldsa87,
    )
}

/// The same walk **under this ruleset's ladder** (ADR-0082 Decision 1).
///
/// `max_step_leaf_count` is `PalwCourtParamsV2::max_step_leaf_count` off the bundle the accepting
/// block is folded under — `2^26` on every shipped preset. It is the ONE place a free-prompt
/// commitment meets the number its network's court can actually prosecute to: a claim whose
/// `work_leaves` is past the ladder has a step index no dissection can reach, so crediting it
/// would be weight for work nobody can be convicted over. A carrier that fails it is SKIPPED, like
/// every other refusal in this walk, so a peer's payload can never reject the block.
///
/// A separate entry rather than a parameter on the old one, for the reason the `_under_v3` split
/// gives: a caller that has not switched keeps the weaker structural bound rather than silently
/// acquiring a stricter rule it did not ask for.
pub fn palw_fp_objects_from_accepted_txs_under_ruleset_v3<V>(
    txs: &[Transaction],
    network_domain: Hash64,
    _freeprompt: &PalwFreePromptParamsV3,
    _accepted_block: BlockHash,
    panel_da_armed: bool,
    max_step_leaf_count: u64,
    verify_mldsa87: V,
) -> PalwFpExtractionV3
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
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
        if payload.validate_stateless_under_ruleset_v3(network_domain, panel_da_armed, max_step_leaf_count).is_err() {
            out.skipped.push((id, "payload is not stateless-admissible"));
            continue;
        }
        // Who authored this commitment. Skipped rather than fatal, for the reason this whole walk
        // is total over whatever was accepted: rejecting a peer-supplied payload here would be a
        // remote denial of service wearing a consensus rule's clothes.
        if payload.validate_signature_v3(&verify_mldsa87).is_err() {
            out.skipped.push((id, "commitment signature does not verify under the carried key"));
            continue;
        }
        // **Priced by the TRANSITION, against the class's own canonical job** (ADR-0074
        // Decision 5). Extraction holds no class state, so it carries the leaves and the facts
        // the work identity is made of; the state derives quanta and pwu where the class rule
        // lives, and refuses sub-quantum work there with a readable reason.
        let commitment = &payload.commitment;
        out.objects.push(PalwConsensusObjectV3Carrier {
            carrier: id,
            object: PalwConsensusObjectV2::FreePromptCommitted {
                claim: payload.claim_id(),
                class_id: commitment.job.class_id,
                bond: PalwBondKeyV2(commitment.job.executor_bond),
                executor_pubkey: commitment.job.executor_pubkey.clone(),
                work_leaves: commitment.work_leaves,
                prompt_token_ids_hash: commitment.job.prompt_token_ids_hash,
                decode_tokens_executed: commitment.decode_tokens_executed,
                trace_root: commitment.trace_root,
                output_root: commitment.output_root,
                // The DA trio and the court binding travel WITH the claim: they are the
                // producer's obligations, and the panel/court read them off the state record.
                execution_root: commitment.execution_root,
                trace_chunk_count: commitment.trace_chunk_count,
                trace_retention_daa: commitment.trace_retention_daa,
            },
        });
    }
    out
}

/// **Transaction-level admission for [`SUBNETWORK_ID_PALW_FP_COMMITMENT`] (0x4a).**
///
/// Until this existed the id was DEFINED but not ROUTED: `check_transaction_subnetwork` fell
/// through to the blanket `SubnetworksDisabled`, so no block could carry a free-prompt
/// commitment, no claim of that source could ever be created, and the whole receipt lane was
/// unreachable on a live network — the extraction walk beside it was a total function over a set
/// that was always empty.
///
/// Context-free by construction, because isolation validation is: the payload must decode, and it
/// must pass the shape half of its own stateless rules. The two checks it does NOT run here — the
/// network domain and the derived CU price — need the network's bundle, which isolation has no
/// access to, and both are re-run by [`palw_fp_objects_from_accepted_txs_v3`] where a failure
/// SKIPS the carrier instead of rejecting the block. That asymmetry is the safe direction: this
/// gate is strictly weaker than the walk, so it can never reject something the walk would have
/// accepted, and it cannot admit anything the walk will silently credit.
pub fn validate_palw_fp_commitment_tx(payload: &[u8]) -> Result<(), crate::palw_freeprompt_v3::PalwFpV3Error> {
    // `false`: on every shipped preset `PanelDa` is dormant, so this door answers exactly as it
    // did before ADR-0077 Decision 16 existed and no block becomes acceptable to this build that
    // was not acceptable to the last one.
    validate_palw_fp_commitment_tx_under_v3(payload, false)
}

/// The same door **on a network whose ruleset carries `PanelDa`** (ADR-0077 Decision 16).
///
/// `panel_da_admissible` is `Params::palw_panel_da_admissible` — `is_some()` on the fence, not
/// `is_active(daa)`, because isolation validation holds no DAA score and its own contract is that
/// it never will. Reading the fence's PRESENCE rather than its height is what keeps this gate
/// weaker than the walk at every height: `palw_panel_da_at(h) ⇒ palw_panel_da_admissible` for all
/// `h`, so the door can never reject a carrier the walk would have credited, while a mode-2
/// carrier that the walk will refuse (before the height) is admitted and then skipped — the same
/// direction of failure this whole gate is built around.
///
/// The consequence, stated rather than buried: a network that SCHEDULES the fence starts
/// admitting mode-2 shape into blocks the moment it ships that preset, which is a coordinated
/// release exactly like the 0x30/0x31 token band's ("admitting the ids is part of the coordinated
/// release... what the DAA fence governs is the *effect*"). A network that has not scheduled it
/// — every shipped preset — refuses the shape here, as it always did.
pub fn validate_palw_fp_commitment_tx_under_v3(
    payload: &[u8],
    panel_da_admissible: bool,
) -> Result<(), crate::palw_freeprompt_v3::PalwFpV3Error> {
    let payload: PalwFpCommitmentTxPayloadV3 =
        borsh::from_slice(payload).map_err(|_| crate::palw_freeprompt_v3::PalwFpV3Error::PayloadUndecodable)?;
    payload.validate_shape_under_v3(panel_da_admissible)
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
        crate::palw_fp_devnet_v3::palw_fp_devnet_bundle_for_tests(h64(1), h64(0xCA7), h64(0xC0757)).unwrap().freeprompt
    }

    /// A commitment whose CU earns real quanta under the devnet bundle.
    fn payload(prompt_tokens: u32, decode: u32) -> PalwFpCommitmentTxPayloadV3 {
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
            prompt_mode: crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_USER,
            sampling_seed: crate::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
            temperature_q: crate::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        };
        let events: Vec<Hash64> = (0..decode as u64).map(|i| h64(i + 1)).collect();
        let (manifest_root, chunk_count, _) = fp_trace_manifest_v3(h64(0xB1), &events);
        let commitment = PalwFreePromptCommitmentV3 {
            trace_root: h64(0x7A),
            output_root: h64(0x0B),
            execution_root: h64(0x4E),
            schedule_root: h64(0x5C),
            decode_tokens_executed: decode,
            stop_reason: PalwFpStopReasonV3::EndOfGeneration,
            work_leaves: (prompt_tokens as u64 + decode as u64) * 64,
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

    /// **A commitment nobody signed for creates no claim** (launch blockers §4).
    ///
    /// `validate_stateless_v3`'s own doc said "the signature is verified by the caller" and there
    /// was no caller: `PalwFreePromptCommitmentEnvelopeV3::validate_signature_v3` had no use
    /// anywhere in the tree — the one call site of that method name is the SPEND envelope's. So a
    /// 0x4a transaction from any stranger created a claim bound to any bond outpoint it named,
    /// including the genesis premine bond pinned in `params.rs`, and the walk dropped
    /// `executor_pubkey` on the way.
    ///
    /// The verifier is an ARGUMENT now, so "somebody else checks it" is no longer expressible.
    #[test]
    fn a_commitment_whose_signature_does_not_verify_creates_no_claim() {
        let fp = freeprompt();
        let p = payload(96, 256);
        let carrier = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap());

        // A verifier that answers honestly — this fixture carries no real signature.
        let refused = palw_fp_objects_from_accepted_txs_v3(std::slice::from_ref(&carrier), net(), &fp, h64(1), |_, _, _, _| false);
        assert!(refused.objects.is_empty(), "an unsigned commitment must not become a claim");
        assert_eq!(refused.skipped.len(), 1, "and it is SKIPPED with a reason, not silently dropped");
        assert!(refused.skipped[0].1.contains("signature"), "got {}", refused.skipped[0].1);

        // The same carrier with a verifier that accepts still becomes exactly one object, so the
        // refusal above is the signature and nothing else.
        assert_eq!(palw_fp_objects_from_accepted_txs_v3(&[carrier], net(), &fp, h64(1), |_, _, _, _| true).objects.len(), 1);
    }

    /// The signed message is the claim id under the commitment's own context, and the key it is
    /// checked against is the CARRIED one — whether that key is the named bond's is the stateful
    /// side's question, against the candidate-chain bond record.
    #[test]
    fn the_commitment_signature_is_checked_over_the_claim_id_in_its_own_context() {
        let fp = freeprompt();
        let p = payload(96, 256);
        let seen = std::cell::RefCell::new(Vec::new());
        let _ = palw_fp_objects_from_accepted_txs_v3(
            &[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap())],
            net(),
            &fp,
            h64(1),
            |key, message, _sig, context| {
                seen.borrow_mut().push((key.to_vec(), message.to_vec(), context.to_vec()));
                true
            },
        );
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 1, "the verifier was consulted exactly once");
        assert_eq!(seen[0].0, p.commitment.job.executor_pubkey, "under the carried key");
        assert_eq!(seen[0].1, p.claim_id().as_byte_slice(), "over the claim id");
        assert_eq!(seen[0].2, crate::palw_freeprompt_v3::PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, "in its own domain");
    }

    /// The happy path: an accepted commitment becomes exactly one object, priced by the bundle    /// The happy path: an accepted commitment becomes exactly one object carrying the leaves the
    /// transition prices against the class's own job (ADR-0074 Decision 5) — never a price.
    #[test]
    fn an_accepted_commitment_becomes_one_object_carrying_its_leaves() {
        let fp = freeprompt();
        let p = payload(96, 256);
        let expected_claim = p.claim_id();

        let extracted = palw_fp_objects_from_accepted_txs_v3(
            &[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap())],
            net(),
            &fp,
            h64(1),
            |_, _, _, _| true,
        );
        assert_eq!(extracted.objects.len(), 1);
        assert!(extracted.skipped.is_empty());
        match &extracted.objects[0].object {
            PalwConsensusObjectV2::FreePromptCommitted {
                claim,
                class_id,
                bond,
                work_leaves,
                decode_tokens_executed,
                trace_root,
                output_root,
                ..
            } => {
                assert_eq!(*claim, expected_claim);
                assert_eq!(*class_id, h64(1));
                assert_eq!(bond.0.transaction_id, TransactionId::from_u64_word(7));
                assert_eq!((*work_leaves, *decode_tokens_executed), (p.commitment.work_leaves, 256));
                assert_eq!(*trace_root, h64(0x7A));
                assert_eq!(*output_root, h64(0x0B));
            }
            other => panic!("expected a free-prompt object, got {other:?}"),
        }
    }

    /// A leaf count above the ladder is refused outright by the stateless rule, so it never
    /// becomes an object at all (a count within the ladder is the seats' to verify against the
    /// capture — ADR-0074 Decision 5).
    ///
    /// **The cap is the RULESET's, and the walk that has one is the one that applies it**
    /// (ADR-0082 Decision 1). This asked the arming-free entry about `PALW_STEP_MAX_LEAVES + 1`,
    /// which pinned the executor's `2^22` as the chain's ladder on a `2^26` network; the
    /// `_under_ruleset_v3` walk below is where the number lives now.
    #[test]
    fn a_leaf_count_above_the_cap_never_becomes_an_object() {
        let fp = freeprompt();
        let mut p = payload(96, 256);
        p.commitment.work_leaves = crate::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP + 1;
        let extracted = palw_fp_objects_from_accepted_txs_v3(
            &[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap())],
            net(),
            &fp,
            h64(1),
            |_, _, _, _| true,
        );
        assert!(extracted.objects.is_empty());
        assert_eq!(
            extracted.skipped,
            vec![(tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&p).unwrap()).id(), "payload is not stateless-admissible")]
        );
    }

    /// Every skip reason is reachable and NAMED, so an operator can see a carrier being dropped:
    /// undecodable bytes and a foreign network. (Sub-quantum work is the transition's refusal,
    /// where the class's quantum lives — ADR-0074 Decision 5.)
    #[test]
    fn every_skip_reason_is_named() {
        let fp = freeprompt();

        let junk = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, vec![0xFF; 32]);
        let out = palw_fp_objects_from_accepted_txs_v3(std::slice::from_ref(&junk), net(), &fp, h64(1), |_, _, _, _| true);
        assert_eq!(out.skipped, vec![(junk.id(), "payload does not decode")]);

        let foreign = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, borsh::to_vec(&payload(96, 256)).unwrap());
        let out = palw_fp_objects_from_accepted_txs_v3(std::slice::from_ref(&foreign), h64(0x99), &fp, h64(1), |_, _, _, _| true);
        assert_eq!(out.skipped, vec![(foreign.id(), "payload is not stateless-admissible")], "a foreign network's payload");
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
        let out = palw_fp_objects_from_accepted_txs_v3(&txs, net(), &fp, h64(1), |_, _, _, _| true);
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

    /// **The door in front of the walk (0x4a's routing).**
    ///
    /// Admission is context-free, so it is deliberately WEAKER than the walk: it must never
    /// reject a carrier the walk would have credited, and everything it lets through that the
    /// walk refuses is skipped with a named reason rather than credited. Both directions are
    /// asserted, because only one of them is safe to get wrong.
    #[test]
    fn admission_never_refuses_what_the_walk_would_credit() {
        let fp = freeprompt();

        // A payload the walk credits is admitted.
        let good = borsh::to_vec(&payload(96, 256)).unwrap();
        assert!(validate_palw_fp_commitment_tx(&good).is_ok());
        assert_eq!(
            palw_fp_objects_from_accepted_txs_v3(&[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, good)], net(), &fp, h64(1), |_, _, _, _| true)
                .objects
                .len(),
            1
        );

        // Undecodable bytes are refused at the door — the one shape failure that needs no
        // parameters at all.
        assert_eq!(validate_palw_fp_commitment_tx(&[0xFF; 32]), Err(crate::palw_freeprompt_v3::PalwFpV3Error::PayloadUndecodable));

        // A shape failure the walk would skip is refused at the door instead, which is the same
        // answer reached earlier.
        let mut zero_decode = payload(96, 256);
        zero_decode.commitment.decode_tokens_executed = 0;
        assert!(validate_palw_fp_commitment_tx(&borsh::to_vec(&zero_decode).unwrap()).is_err());

        // The two checks the door CANNOT run: a foreign network domain and an inflated price.
        // Both are admitted here — this node has no bundle in isolation — and both are skipped by
        // the walk, never credited. That is the deliberate asymmetry, pinned so a later "tighten
        // the door" edit has to argue with it.
        let foreign = borsh::to_vec(&payload(96, 256)).unwrap();
        assert!(validate_palw_fp_commitment_tx(&foreign).is_ok(), "the door cannot know whose network this is");
        let out = palw_fp_objects_from_accepted_txs_v3(
            &[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, foreign)],
            h64(0x99),
            &fp,
            h64(1),
            |_, _, _, _| true,
        );
        assert!(out.objects.is_empty(), "but the walk knows, and credits nothing");
        assert_eq!(out.skipped.len(), 1);

        // An inflated leaf count WITHIN the cap passes the door and the walk alike: neither holds
        // the capture, and the price is verified by the seats against it (ADR-0074 Decision 5).
        // Above the cap, the door itself refuses — that much it can know.
        let mut inflated = payload(96, 256);
        inflated.commitment.work_leaves *= 10;
        let inflated = borsh::to_vec(&inflated).unwrap();
        assert!(validate_palw_fp_commitment_tx(&inflated).is_ok(), "the door cannot price without the capture");
        let out = palw_fp_objects_from_accepted_txs_v3(
            &[tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, inflated)],
            net(),
            &fp,
            h64(1),
            |_, _, _, _| true,
        );
        assert_eq!(out.objects.len(), 1, "and the walk carries the leaves for the seats to judge");
        let mut above = payload(96, 256);
        above.commitment.work_leaves = crate::palw_freeprompt_v3::PALW_FP_STRUCTURAL_WORK_LEAVES_CAP + 1;
        assert!(validate_palw_fp_commitment_tx(&borsh::to_vec(&above).unwrap()).is_err(), "the cap the door does know");
    }

    /// **The ladder the network froze is the walk's, and the door is never stricter than it**
    /// (ADR-0082 Decision 1).
    ///
    /// The graph-v5 dense 512 row's canonical job counts more leaves than the executor's `2^22`,
    /// and the RC bundle admits the class at `2^26`. Both facts are asserted here against one
    /// commitment: transaction isolation admits it (the door holds no bundle, so it bounds at the
    /// structural top), the ruleset walk credits it at `2^26`, and a ruleset that froze `2^22`
    /// skips it by name. Before this, the door refused the carrier and no block could hold one.
    #[test]
    fn the_walk_bounds_a_commitment_at_the_rulesets_ladder_and_the_door_never_narrower() {
        let fp = freeprompt();
        let mut wide = payload(96, 256);
        wide.commitment.work_leaves = crate::palw_step::PALW_STEP_MAX_LEAVES + 1;
        let bytes = borsh::to_vec(&wide).unwrap();
        assert!(
            validate_palw_fp_commitment_tx(&bytes).is_ok(),
            "isolation must not refuse a carrier the 2^26 walk would credit — that is the one direction the door may not fail in"
        );
        let carrier = tx(SUBNETWORK_ID_PALW_FP_COMMITMENT, bytes);
        let at_rc = palw_fp_objects_from_accepted_txs_under_ruleset_v3(
            std::slice::from_ref(&carrier),
            net(),
            &fp,
            h64(1),
            false,
            crate::palw_fp_devnet_v3::COURT_MAX_STEP_LEAVES,
            |_, _, _, _| true,
        );
        assert_eq!(at_rc.objects.len(), 1, "the shipped ruleset's 2^26 ladder admits it: {:?}", at_rc.skipped);
        let at_executor_constant = palw_fp_objects_from_accepted_txs_under_ruleset_v3(
            std::slice::from_ref(&carrier),
            net(),
            &fp,
            h64(1),
            false,
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            |_, _, _, _| true,
        );
        assert!(at_executor_constant.objects.is_empty(), "a ruleset that froze 2^22 refuses it");
        assert_eq!(at_executor_constant.skipped, vec![(carrier.id(), "payload is not stateless-admissible")]);
    }
}
