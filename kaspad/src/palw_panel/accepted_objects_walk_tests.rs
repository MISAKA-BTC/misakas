//! **`palw_panel`'s `accepted_objects_walk_tests` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

use super::*;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::BlockHashMap;
use kaspa_consensus_core::acceptance_data::{AcceptanceData, AcceptedTxEntry, MergesetBlockAcceptanceData};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::errors::consensus::{ConsensusError, ConsensusResult};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::trusted::ExternalGhostdagData;

/// A selected chain and the blocks it merged — the five reads the walk makes, nothing else.
#[derive(Default)]
struct Chain {
    sink: BlockHash,
    headers: HashMap<BlockHash, Arc<Header>>,
    parents: HashMap<BlockHash, BlockHash>,
    acceptance: HashMap<BlockHash, Arc<AcceptanceData>>,
    blocks: HashMap<BlockHash, Block>,
    targets: HashMap<Hash64, kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1>,
    views: HashMap<Hash64, kaspa_consensus_core::palw_state_v2::PalwLegacyDisputeViewV1>,
    answer_history: HashMap<
        (BlockHash, kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerKeyV2, Hash64, u8),
        kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2,
    >,
    answer_history_unavailable: bool,
    flip_sink_on_block: Option<(BlockHash, BlockHash)>,
    observed_sink: std::sync::Mutex<Option<BlockHash>>,
}

impl kaspa_consensus_core::api::ConsensusApi for Chain {
    fn palw_legacy_answer_history_v2(
        &self,
        block: BlockHash,
        chunk: Box<kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwLegacyAnswerChunkV2>,
        maximum: u64,
    ) -> Result<Option<kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldAnswerCarriageV2>, String> {
        if self.answer_history_unavailable {
            return Err("test history reader unavailable".into());
        }
        let Some(row) = self.answer_history.get(&(block, chunk.key(), chunk.answer_digest, chunk.index)) else { return Ok(None) };
        row.check(maximum)?;
        Ok(row.preview(&chunk).ok().flatten())
    }
    fn palw_legacy_dispute_v1(&self, claim: Hash64) -> Option<kaspa_consensus_core::palw_state_v2::PalwLegacyDisputeViewV1> {
        self.views.get(&claim).cloned()
    }
    fn palw_fraud_filer_target_v1(
        &self,
        claim: Hash64,
    ) -> Option<kaspa_consensus_core::palw_offence_attribution_v1::PalwOffenceTargetV1> {
        self.targets.get(&claim).cloned()
    }
    fn get_sink(&self) -> BlockHash {
        self.observed_sink.lock().unwrap().unwrap_or(self.sink)
    }
    fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
        self.headers.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
    }
    fn get_ghostdag_data(&self, hash: BlockHash) -> ConsensusResult<ExternalGhostdagData> {
        let selected_parent = *self.parents.get(&hash).ok_or(ConsensusError::HeaderNotFound(hash))?;
        Ok(ExternalGhostdagData {
            blue_score: 0,
            blue_work: 0.into(),
            selected_parent,
            mergeset_blues: vec![],
            mergeset_reds: vec![],
            blues_anticone_sizes: BlockHashMap::default(),
        })
    }
    fn get_block_acceptance_data(&self, hash: BlockHash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.acceptance.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
    }
    fn get_block(&self, hash: BlockHash) -> ConsensusResult<Block> {
        if let Some((at, sink)) = self.flip_sink_on_block
            && at == hash
        {
            *self.observed_sink.lock().unwrap() = Some(sink);
        }
        self.blocks.get(&hash).cloned().ok_or(ConsensusError::HeaderNotFound(hash))
    }
    fn is_chain_ancestor_of(&self, low: BlockHash, mut high: BlockHash) -> ConsensusResult<bool> {
        loop {
            if low == high {
                return Ok(true);
            }
            let parent = *self.parents.get(&high).ok_or(ConsensusError::HeaderNotFound(high))?;
            if parent == high {
                return Ok(false);
            }
            high = parent;
        }
    }
}

fn hash(n: u64) -> BlockHash {
    BlockHash::from_u64_word(n)
}

fn lifecycle(claim: u64) -> Transaction {
    let object = PalwConsensusObjectV2::ProducerDefaulted { claim: Hash64::from_u64_word(claim), receipts: vec![] };
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("encodes");
    Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload)
}

fn other() -> Transaction {
    Transaction::new(TX_VERSION, vec![], vec![], 0, kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE, 0, vec![1, 2, 3])
}

fn claim_of(object: &PalwConsensusObjectV2) -> u64 {
    match object {
        PalwConsensusObjectV2::ProducerDefaulted { claim, .. } => {
            (0..64u64).find(|n| Hash64::from_u64_word(*n) == *claim).expect("a test claim")
        }
        _ => panic!("only the test's objects ride this chain"),
    }
}

fn append(chain: &mut Chain, n: u64, daa: u64, parent: u64) {
    let mut header = Header::from_precomputed_hash(hash(n), vec![hash(parent)]);
    header.daa_score = daa;
    chain.headers.insert(hash(n), Arc::new(header.clone()));
    chain.parents.insert(hash(n), hash(parent));
    chain.blocks.insert(hash(n), Block::new(header, vec![lifecycle(n)]));
    chain.acceptance.insert(
        hash(n),
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(n),
            accepted_transactions: vec![AcceptedTxEntry {
                transaction_id: chain.blocks[&hash(n)].transactions[0].id(),
                index_within_block: 0,
            }],
        }]),
    );
    chain.sink = hash(n);
}

fn repeated_daa_chain() -> Chain {
    let mut chain = Chain::default();
    for n in 1..=8 {
        append(&mut chain, n, 100 + n / 4, n.saturating_sub(1).max(1));
    }
    chain
}

#[test]
fn the_fp_bootstrap_reads_only_the_recorded_accepted_commitment_and_pins_every_job_field() {
    use super::palw_fraud_filer::{input::tests::payload, palw_fraud_filer_fp_payload_v1 as read};
    use kaspa_consensus_core::palw_offence_attribution_v1::{PalwClaimSourceKindV1, PalwOffenceTargetV1};
    let payload = payload();
    let c = &payload.commitment;
    let claim = payload.claim_id();
    let target = PalwOffenceTargetV1 {
        claim_id: claim,
        class_id: c.job.class_id,
        artifact_root: hash(333),
        executor_bond: PalwBondKeyV2(c.job.executor_bond),
        execution_root: c.execution_root,
        lane: Some(PalwClaimSourceKindV1::FreePrompt),
        segment_count: Some(2),
        phase: None,
        job_identity: kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(c),
        trace_root: c.trace_root,
        output_root: c.output_root,
        registered_semantics: None,
    };
    let mut candidate = kaspa_consensus_core::palw_state_v2::PalwFraudFilerCandidateV1 {
        claim_id: claim,
        producer: target.executor_bond,
        accepted_daa: 101,
        seat: false,
        job: kaspa_consensus_core::palw_operator_da_v1::PalwOperatorDaJobV1 {
            accepted_block: hash(2),
            class_id: c.job.class_id,
            artifact_root: Some(target.artifact_root),
            execution_root: c.execution_root,
            trace_root: c.trace_root,
            output_root: c.output_root,
            work_leaves: c.work_leaves,
            free_prompt: true,
            held_to_final: false,
        },
    };
    let commitment_tx = |p: &kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3| {
        Transaction::new(
            TX_VERSION,
            vec![],
            vec![],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            borsh::to_vec(p).unwrap(),
        )
    };
    let mut foreign = payload.clone();
    foreign.commitment.job.job_nonce[0] ^= 1;
    let txs = vec![commitment_tx(&payload), commitment_tx(&foreign), commitment_tx(&payload)];
    let mut chain = Chain::default();
    chain.blocks.insert(hash(1), Block::new(Header::from_precomputed_hash(hash(1), vec![]), txs.clone()));
    chain.targets.insert(claim, target.clone());
    let accept = |indices: &[usize]| {
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(1),
            accepted_transactions: indices
                .iter()
                .map(|i| AcceptedTxEntry { transaction_id: txs[*i].id(), index_within_block: *i as u32 })
                .collect(),
        }])
    };
    chain.acceptance.insert(hash(2), accept(&[1]));
    assert!(
        read(&chain, &candidate, c.job.network_domain).unwrap_err().contains("not present"),
        "unaccepted exact decoys are ignored"
    );
    chain.acceptance.insert(hash(2), accept(&[1, 2]));
    assert_eq!(
        read(&chain, &candidate, c.job.network_domain).unwrap().claim_id(),
        claim,
        "reads an accepted mergeset block, not the fold point's txs"
    );
    assert!(read(&chain, &candidate, hash(999)).is_err());
    candidate.job.work_leaves += 1;
    assert!(read(&chain, &candidate, c.job.network_domain).is_err());
    candidate.job.work_leaves -= 1;
    chain.targets.get_mut(&claim).unwrap().job_identity = hash(987);
    assert!(read(&chain, &candidate, c.job.network_domain).is_err());
    chain.targets.insert(claim, target.clone());
    chain.targets.get_mut(&claim).unwrap().lane = Some(PalwClaimSourceKindV1::Attempt);
    assert!(read(&chain, &candidate, c.job.network_domain).is_err());
    chain.targets.insert(claim, target.clone());
    let saved = chain.acceptance.remove(&hash(2)).unwrap();
    assert!(read(&chain, &candidate, c.job.network_domain).unwrap_err().contains("acceptance"));
    chain.acceptance.insert(hash(2), saved);
    let saved = chain.blocks.remove(&hash(1)).unwrap();
    assert!(read(&chain, &candidate, c.job.network_domain).unwrap_err().contains("block"));
    chain.blocks.insert(hash(1), saved);
    let mut bad = (*accept(&[2])).clone();
    bad[0].accepted_transactions[0].transaction_id = hash(888);
    chain.acceptance.insert(hash(2), Arc::new(bad));
    assert!(read(&chain, &candidate, c.job.network_domain).unwrap_err().contains("transaction id"));
    chain.acceptance.insert(hash(2), accept(&[2]));
    assert_eq!(read(&chain, &candidate, c.job.network_domain).unwrap().claim_id(), claim);
    // Adapter evidence only: the real-node fixture deletes real block/acceptance DB rows.
    let metadata = borsh::to_vec(c).unwrap();
    let view = kaspa_consensus_core::palw_state_v2::PalwLegacyDisputeViewV1 {
        claim_id: claim,
        class_id: target.class_id,
        producer: target.executor_bond,
        phase: kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Provisional,
        execution_root: target.execution_root,
        trace_root: target.trace_root,
        accepted_daa: candidate.accepted_daa,
        trace_retention_daa: 9999,
        job_identity: target.job_identity,
        public_fp_job: Some(metadata),
        registered_semantics: None,
        work_leaves: c.work_leaves,
        deadline_daa: None,
        hard_deadline_daa: 9990,
        record: None,
        sessions: vec![],
        answered: vec![],
        flat_answered_rows: 0,
        open_courts: 0,
        executor_refuted: false,
        court_convicted: false,
        da_defaulted: false,
    };
    chain.views.insert(claim, view.clone());
    assert_eq!(
        read(&chain, &candidate, c.job.network_domain).unwrap().prompt_token_ids,
        payload.prompt_token_ids,
        "retained accepted PublicDA ids are preferred"
    );
    chain.acceptance.clear();
    chain.blocks.clear();
    let recovered = read(&chain, &candidate, c.job.network_domain).unwrap();
    assert_eq!(recovered.commitment, *c);
    assert!(recovered.prompt_token_ids.is_empty() && recovered.signature.is_empty());
    assert!(read(&chain, &candidate, hash(999)).is_err());
    let row = chain.views.get_mut(&claim).unwrap();
    row.accepted_daa += 1;
    assert!(read(&chain, &candidate, c.job.network_domain).is_err());
    chain.views.insert(claim, view.clone());
    chain.views.get_mut(&claim).unwrap().producer = PalwBondKeyV2(TransactionOutpoint::new(hash(444), 0));
    assert!(read(&chain, &candidate, c.job.network_domain).is_err());
    chain.views.insert(claim, view.clone());
    chain.views.get_mut(&claim).unwrap().public_fp_job = Some(borsh::to_vec(&foreign.commitment).unwrap());
    assert!(read(&chain, &candidate, c.job.network_domain).is_err(), "a foreign job cannot substitute for the accepted commitment");
    chain.views.insert(claim, view.clone());
    chain.views.get_mut(&claim).unwrap().public_fp_job.as_mut().unwrap().push(0);
    assert!(read(&chain, &candidate, c.job.network_domain).is_err(), "no noncanonical trailing bytes");
    chain.views.insert(claim, view);
    assert_eq!(read(&chain, &candidate, c.job.network_domain).unwrap().claim_id(), claim);
}

#[test]
fn public_filer_discards_authenticated_answers_if_the_native_branch_changes_during_the_page() {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    let backend = super::seat_s_tests::floor_backend();
    let payload = super::palw_fraud_filer::input::tests::payload();
    let run = backend
        .execute_free_prompt(&payload.commitment.job, &payload.prompt_token_ids.iter().map(|id| *id as usize).collect::<Vec<_>>())
        .unwrap();
    let event = backend.disclose_trace_event(&run.outcome.material, 0, 0).unwrap();
    let claim = Hash64::from_u64_word(45);
    let object = PalwConsensusObjectV2::MaterialDisclosedV2 {
        claim,
        unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::Event { row: 0, tile: 0 },
        answer: kaspa_consensus_core::palw_da_rcore_v1::PalwDaAnswerV1::Event(event),
        discloser: kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(payload.commitment.job.executor_bond),
        signature: vec![],
    };
    let tx = Transaction::new(
        TX_VERSION,
        vec![],
        vec![],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap(),
    );
    let mut chain = repeated_daa_chain();
    chain.blocks.insert(hash(8), Block::new(chain.headers[&hash(8)].as_ref().clone(), vec![tx]));
    chain.acceptance.insert(
        hash(8),
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(8),
            accepted_transactions: vec![AcceptedTxEntry {
                transaction_id: chain.blocks[&hash(8)].transactions[0].id(),
                index_within_block: 0,
            }],
        }]),
    );
    let wanted = BTreeMap::from([(claim, run.outcome.execution_root)]);
    let (_, reset, answers) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 8, &wanted, &BTreeMap::new()).unwrap();
    assert!(!reset);
    assert_eq!(answers.len(), 1, "the stable native branch reads the actual authenticated binding");
    append(&mut chain, 20, 104, 3);
    chain.sink = hash(8);
    chain.flip_sink_on_block = Some((hash(8), hash(20)));
    let (walk, reset, answers) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 8, &wanted, &BTreeMap::new()).unwrap();
    assert!(reset && answers.is_empty(), "end-of-page branch recheck discards the old branch's otherwise authentic bytes");
    assert_eq!(walk.anchor, hash(20));
    assert_eq!(walk.next, Some(hash(20)));
    assert_eq!(walk.floor, 100);
}

#[test]
fn public_filer_treats_native_body_acceptance_identity_mismatch_as_a_hole() {
    let mut chain = repeated_daa_chain();
    let mut row = chain.acceptance[&hash(7)].as_ref().clone();
    row[0].accepted_transactions[0].transaction_id = hash(999);
    chain.acceptance.insert(hash(7), Arc::new(row));
    let mut seen = Vec::new();
    let (next, hole) =
        super::palw_fraud_filer::history::walk_available_v1(&chain, chain.sink, 100, 8, &mut |_, object| seen.push(claim_of(&object)))
            .unwrap();
    assert!(next.is_none() && hole);
    assert_eq!(
        seen,
        vec![8, 6, 5, 4, 3, 2, 1],
        "an index cannot substitute another body for an accepted tx identity; older authenticated material remains reachable"
    );
}

#[test]
fn public_filer_skips_native_prune_holes_without_losing_older_bytes_or_completing_the_watermark() {
    let mut chain = repeated_daa_chain();
    chain.blocks.remove(&hash(6));
    chain.acceptance.remove(&hash(4));
    let mut seen = Vec::new();
    let mut cursor = Some(chain.sink);
    let mut incomplete = false;
    while let Some(start) = cursor {
        let (next, hole) =
            super::palw_fraud_filer::history::walk_available_v1(&chain, start, 100, 3, &mut |_, object| seen.push(claim_of(&object)))
                .unwrap();
        incomplete |= hole;
        cursor = next;
    }
    assert!(incomplete);
    assert_eq!(seen, vec![8, 7, 5, 3, 2, 1], "missing carrier/acceptance never hides older authenticated accepted bodies");
    assert!(
        walk_accepted_lifecycle_page_with_block_v2(&chain, chain.sink, 100, 8, &mut |_, _| {}).is_err(),
        "other callers retain the original strict walker contract"
    );
    let mut previous = None;
    for expected in [5, 2, 8] {
        let (walk, reset, answers) =
            super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, previous, 3, &BTreeMap::new(), &BTreeMap::new())
                .unwrap();
        assert!(!reset && answers.is_empty());
        assert_eq!(walk.next, Some(hash(expected)));
        assert!(walk.incomplete, "a hole on the first page survives later pagination to the oldest floor");
        previous = Some(walk);
    }
    // The finished incomplete scan starts again at a new tip, making fresh reanswers visible.
    append(&mut chain, 9, 103, 8);
    let (fresh, _, _) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, previous, 3, &BTreeMap::new(), &BTreeMap::new())
            .unwrap();
    assert_eq!(fresh.anchor, hash(9));
    assert_eq!(fresh.next, Some(hash(6)));
}

#[test]
fn paged_history_reads_every_block_even_when_daa_does_not_count_the_blocks() {
    let chain = repeated_daa_chain();
    let mut old = Vec::new();
    // The old filer used DAA difference as the block cap: three blocks cannot cover this eight-block interval.
    walk_accepted_lifecycle_objects_v1(&chain, 100, 102 - 100 + 1, &mut |o| old.push(claim_of(&o)));
    assert_eq!(old, vec![8, 7, 6]);
    let mut seen = Vec::new();
    let mut cursor = Some(chain.sink);
    let mut pages = 0;
    while let Some(start) = cursor {
        cursor = walk_accepted_lifecycle_page_v1(&chain, start, 100, 3, &mut |o| seen.push(claim_of(&o))).unwrap();
        pages += 1;
    }
    assert_eq!(pages, 3);
    assert_eq!(seen, vec![8, 7, 6, 5, 4, 3, 2, 1], "no gap or duplicate at page boundaries");
}

#[test]
fn false_valid_receipt_history_backfills_plateaus_and_retries_missing_suffixes_and_reorgs() {
    use crate::palw_filer_false_valid::palw_false_valid_history_page_v1 as page;
    let mut chain = repeated_daa_chain();
    let mut seen = Vec::new();
    let first = page(&chain, None, 100, 100, 3, &mut |o| seen.push(claim_of(&o))).unwrap();
    assert_eq!(seen, vec![8, 7, 6]);
    assert_eq!(first.next, Some(hash(5)));
    let saved = chain.acceptance.remove(&hash(4)).unwrap();
    let missing = page(&chain, Some(first), 102, 100, 3, &mut |o| seen.push(claim_of(&o))).unwrap();
    assert_eq!(seen, vec![8, 7, 6, 5], "read findings survive an unavailable suffix");
    assert_eq!(missing.next, Some(missing.anchor), "an unavailable page cannot declare completion");
    assert_eq!(missing.floor, 100);
    chain.acceptance.insert(hash(4), saved);
    seen.clear();
    let mut walk = Some(missing);
    loop {
        let found = page(&chain, walk, 102, 100, 3, &mut |o| seen.push(claim_of(&o))).unwrap();
        if found.next.is_none() {
            break;
        }
        walk = Some(found);
    }
    assert_eq!(seen, vec![8, 7, 6, 5, 4, 3, 2, 1]);
    chain.sink = hash(5);
    seen.clear();
    let replaced = page(&chain, Some(first), 102, 100, 3, &mut |o| seen.push(claim_of(&o))).unwrap();
    assert_eq!(replaced.anchor, hash(5));
    assert_eq!(replaced.floor, 100, "a replaced anchor restarts from the claim's original floor");
    assert_eq!(seen, vec![5, 4, 3]);
}

#[test]
fn a_missing_header_acceptance_block_or_parent_never_completes_a_history_page() {
    let mut chain = repeated_daa_chain();
    let read = |chain: &Chain| walk_accepted_lifecycle_page_v1(chain, hash(8), 100, 3, &mut |_| {});
    let header = chain.headers.remove(&hash(8)).unwrap();
    assert!(read(&chain).unwrap_err().contains("history header"));
    chain.headers.insert(hash(8), header);
    let accepted = chain.acceptance.remove(&hash(8)).unwrap();
    assert!(read(&chain).unwrap_err().contains("history acceptance"));
    chain.acceptance.insert(hash(8), accepted);
    let block = chain.blocks.remove(&hash(8)).unwrap();
    assert!(read(&chain).unwrap_err().contains("history block"));
    chain.blocks.insert(hash(8), block);
    let parent = chain.parents.remove(&hash(8)).unwrap();
    assert!(read(&chain).unwrap_err().contains("history selected parent"));
    chain.parents.insert(hash(8), parent);
    assert_eq!(read(&chain).unwrap(), Some(hash(5)), "the same page can be retried once data is restored");
}

#[test]
fn the_filer_resumes_at_its_original_tip_and_restarts_backfill_after_a_reorg() {
    use super::palw_fraud_filer::palw_fraud_filer_read_page_v1 as page;
    let mut chain = repeated_daa_chain();
    let wanted = BTreeMap::new();
    let legacy_wanted = BTreeMap::new();
    let (first, reset, _) = page(&chain, 100, 100, None, 3, &wanted, &legacy_wanted).unwrap();
    assert!(!reset);
    assert_eq!((first.anchor, first.anchor_daa, first.next), (hash(8), 102, Some(hash(5))));
    append(&mut chain, 9, 150, 8);
    let (second, reset, _) = page(&chain, 100, 100, Some(first), 3, &wanted, &legacy_wanted).unwrap();
    assert!(!reset);
    assert_eq!((second.anchor, second.anchor_daa, second.next), (hash(8), 102, Some(hash(2))));
    let (complete, _, _) = page(&chain, 100, 100, Some(second), 3, &wanted, &legacy_wanted).unwrap();
    assert_eq!((complete.anchor_daa, complete.next), (102, None), "arrivals do not move the in-progress watermark");
    let (recent, reset, _) = page(&chain, 100, 140, Some(complete), 3, &wanted, &legacy_wanted).unwrap();
    assert!(!reset);
    assert_eq!((recent.anchor, recent.floor, recent.next), (hash(9), 140, None));
    // Another branch forks below the completed incremental floor. Its old answers must be read, not skipped at DAA 140.
    append(&mut chain, 20, 150, 3);
    let (fork, reset, _) = page(&chain, 100, 140, Some(recent), 2, &wanted, &legacy_wanted).unwrap();
    assert!(reset);
    assert_eq!((fork.anchor, fork.floor, fork.next), (hash(20), 100, Some(hash(2))));
}

#[test]
fn the_filer_reads_only_the_requested_authenticated_legacy_unit_from_public_history() {
    use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1};
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
    use kaspa_consensus_core::palw_legacy_held_da_v2::{PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2};
    let backend = super::seat_s_tests::floor_backend();
    let anchor = Hash64::from_u64_word(0xCAFE);
    let (job, ids) = backend.job_for_anchor(anchor).unwrap();
    let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(job, true);
    let run = backend.execute(&job, &ids).unwrap();
    let ids: Vec<u32> = ids.into_iter().map(|id| id.try_into().unwrap()).collect();
    let binding = misaka_palw_base0::produce::base0_material_decode_any_v1(&run.material).unwrap().binding().clone();
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let selected = PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
    let roots = PalwClaimRootsV1 {
        execution_root: run.execution_root,
        trace_root: run.trace_root,
        anchor,
        attempt_draw: Some(true),
        output_root: None,
        job_pin: None,
    };
    let claim = Hash64::from_u64_word(0xC1);
    let make = |unit| {
        let (binding, answer) = crate::palw_legacy_held_v2::palw_legacy_held_capture_answer_v2(
            &backend,
            &run.material,
            &ids,
            roots,
            backend.prompt_ids_form(),
            &unit,
        )
        .unwrap();
        palw_da_built_answer_object_v1(
            &Hash64::from_u64_word(0xD0),
            claim,
            PalwDaUnitV1::LegacyHeldV2(unit),
            PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((binding, answer))),
            PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB0), 0)),
            backend.prompt_ids_form(),
            u64::MAX,
            |_, _| Some(vec![1]),
        )
        .unwrap()
    };
    let good = make(selected);
    let unused = make(PalwLegacyHeldUnitV2::StepNode { level: height - 1, index: 0 });
    let mut corrupt = good.clone();
    let PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } = &mut corrupt else { unreachable!() };
    let PalwLegacyHeldAnswerV2::Node { frontier, .. } = &mut answer.answer else { unreachable!() };
    frontier[0] = Hash64::from_u64_word(0xBAD);
    let mut chain = repeated_daa_chain();
    // A carrier can be accepted even when its lifecycle object was refused. Wrong bytes occur first in the newest-first read;
    // valid but unselected units and repeats must neither poison the selected answer nor enlarge this case's cache.
    let objects = vec![good.clone(), unused, good.clone(), corrupt];
    let txs = objects
        .into_iter()
        .map(|object| {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
            Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_PALW_LIFECYCLE, 0, payload)
        })
        .collect::<Vec<_>>();
    let count = txs.len();
    chain.blocks.insert(hash(2), Block::new(chain.headers[&hash(2)].as_ref().clone(), txs));
    chain.acceptance.insert(
        hash(2),
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(2),
            accepted_transactions: (0..count)
                .map(|i| AcceptedTxEntry { transaction_id: chain.blocks[&hash(2)].transactions[i].id(), index_within_block: i as u32 })
                .collect(),
        }]),
    );
    let wanted = BTreeMap::from([(claim, run.execution_root)]);
    let selected_units = BTreeMap::from([(claim, selected)]);
    let mut previous = None;
    let mut found = Vec::new();
    loop {
        let (walk, reset, answers) =
            super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, previous, 3, &wanted, &selected_units).unwrap();
        assert!(!reset);
        found.extend(answers);
        previous = Some(walk);
        if walk.next.is_none() {
            break;
        }
    }
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, (claim, PalwDaUnitV1::LegacyHeldV2(selected)));
    chain.acceptance.remove(&hash(1));
    let (partial, _, readable) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 100, &wanted, &selected_units).unwrap();
    assert_eq!(partial.next, Some(partial.anchor), "missing old history never completes the floor");
    assert_eq!(readable.len(), 1, "authenticated answers before the prune boundary are retained");
    let saved = chain.blocks.remove(&hash(2)).unwrap();
    append(&mut chain, 9, 160, 8);
    let tx = Transaction::new(
        TX_VERSION,
        vec![],
        vec![],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: good.clone() }).unwrap(),
    );
    chain.blocks.insert(hash(9), Block::new(chain.headers[&hash(9)].as_ref().clone(), vec![tx]));
    chain.acceptance.insert(
        hash(9),
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(9),
            accepted_transactions: vec![AcceptedTxEntry {
                transaction_id: chain.blocks[&hash(9)].transactions[0].id(),
                index_within_block: 0,
            }],
        }]),
    );
    let (fresh, _, delivered) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, Some(partial), 100, &wanted, &selected_units)
            .unwrap();
    assert_eq!(fresh.anchor, hash(9), "a stalled old walk revisits the new tip");
    assert!(fresh.next.is_some(), "still no completed watermark across missing history");
    assert_eq!(delivered.len(), 1, "the reacquired copy is readable although its original carrier is gone");
    // A non-held profile on a Merkle network needs the explicit network form for the new input unit.
    let input_unit = PalwLegacyHeldUnitV2::PromptIds { chunk: 0 };
    let input_object = make(input_unit);
    let mut bad_input = input_object.clone();
    let PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } = &mut bad_input else { unreachable!() };
    let PalwLegacyHeldAnswerV2::PromptIds { ids, .. } = &mut answer.answer else { unreachable!() };
    ids[0] ^= 1;
    let txs: Vec<_> = [bad_input, input_object]
        .into_iter()
        .map(|object| {
            Transaction::new(
                TX_VERSION,
                vec![],
                vec![],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap(),
            )
        })
        .collect();
    chain.acceptance.insert(
        hash(9),
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(9),
            accepted_transactions: (0..txs.len())
                .map(|i| AcceptedTxEntry { transaction_id: txs[i].id(), index_within_block: i as u32 })
                .collect(),
        }]),
    );
    chain.blocks.insert(hash(9), Block::new(chain.headers[&hash(9)].as_ref().clone(), txs));
    let input_selected = BTreeMap::from([(claim, input_unit)]);
    let (_, _, input_answers) = super::palw_fraud_filer::palw_fraud_filer_read_page_with_form_v1(
        &chain,
        100,
        100,
        None,
        100,
        &wanted,
        &input_selected,
        &BTreeMap::new(),
        &BTreeMap::new(),
        backend.prompt_ids_form(),
    )
    .unwrap();
    assert_eq!(input_answers.len(), 1, "only the valid reacquired input is cached before the missing suffix");
    assert_eq!(input_answers[0].0, (claim, PalwDaUnitV1::LegacyHeldV2(input_unit)));
    assert!(
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 100, &wanted, &input_selected,)
            .unwrap()
            .2
            .is_empty(),
        "a carrier does not choose a Merkle ruleset on a Flat network"
    );
    chain.blocks.insert(hash(2), saved);
    let PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer } = good else { unreachable!() };
    assert_eq!(found[0].1, PalwDaBuiltAnswerV1::LegacyHeldV2(Box::new((answer.binding.clone(), answer.answer.clone()))));
    // Adapter regression: public removal rows and a mergeset last carrier suffice across pages.
    // Synthetic signatures here do not prove MLDSA acceptance or the producer service path.
    let (declaration, chunks) = kaspa_consensus_core::palw_legacy_answer_chunks_v2::palw_legacy_answer_split_v2(
        &answer,
        (borsh::object_length(&answer).unwrap().div_ceil(2)) as u32,
        u64::MAX,
        |_| Ok(vec![1]),
    )
    .unwrap();
    let mut row = kaspa_consensus_core::palw_legacy_answer_chunks_v2::PalwPendingLegacyAnswerV2 {
        declaration,
        deadline_daa: 1000,
        parts: BTreeMap::new(),
    };
    for chunk in &chunks[..chunks.len() - 1] {
        row.parts.insert(chunk.index, chunk.bytes.clone());
    }
    let last = chunks.last().unwrap().clone();
    let history_key = (hash(2), last.key(), last.answer_digest, last.index);
    chain.answer_history.insert(history_key, row.clone());
    let mut corrupt = last.clone();
    corrupt.bytes[0] ^= 1;
    let txs: Vec<_> = [last.clone(), corrupt]
        .into_iter()
        .map(|chunk| {
            Transaction::new(
                TX_VERSION,
                vec![],
                vec![],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                    version: PALW_LIFECYCLE_TX_VERSION_V2,
                    object: PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk: Box::new(chunk) },
                })
                .unwrap(),
            )
        })
        .collect();
    chain.blocks.insert(hash(40), Block::new(Header::from_precomputed_hash(hash(40), vec![]), txs));
    chain.acceptance.insert(
        hash(2),
        Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: hash(40),
            accepted_transactions: (0..2)
                .map(|i| AcceptedTxEntry {
                    transaction_id: chain.blocks[&hash(40)].transactions[i as usize].id(),
                    index_within_block: i,
                })
                .collect(),
        }]),
    );
    let mut previous = None;
    let mut recovered = Vec::new();
    for _ in 0..9 {
        let (walk, _, answers) =
            super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, previous, 1, &wanted, &selected_units).unwrap();
        recovered.extend(answers);
        previous = Some(walk);
        if !recovered.is_empty() {
            break;
        }
    }
    assert_eq!(recovered, found, "the fold block, rather than mergeset carrier block, selects the bounded history row");
    chain.answer_history_unavailable = true;
    let (unavailable, _, answers) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 100, &wanted, &selected_units).unwrap();
    assert!(answers.is_empty());
    assert_eq!(unavailable.next, Some(unavailable.anchor));
    chain.answer_history_unavailable = false;
    chain.answer_history.get_mut(&history_key).unwrap().parts.values_mut().next().unwrap()[0] ^= 1;
    let (corrupt, _, answers) =
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 100, &wanted, &selected_units).unwrap();
    assert!(answers.is_empty());
    assert_eq!(corrupt.next, Some(corrupt.anchor));
    chain.answer_history.remove(&history_key);
    assert!(
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 100, &wanted, &selected_units,)
            .unwrap()
            .2
            .is_empty(),
        "an absent old index does not invent an answer; the case must reacquire"
    );
    chain.answer_history.insert(history_key, row);
    let wrong_root = BTreeMap::from([(claim, Hash64::from_u64_word(0xBAD))]);
    assert!(
        super::palw_fraud_filer::palw_fraud_filer_read_page_v1(&chain, 100, 100, None, 100, &wrong_root, &selected_units,)
            .unwrap()
            .2
            .is_empty()
    );
}

#[test]
fn the_held_reader_reaches_old_filings_before_publishing_and_restarts_on_a_fork() {
    use super::held_court::{PalwHeldChainReadV1, PalwHeldCourtV1, PalwHeldHistoryObjectKeyV1};
    let (duty, oldest, decoy, unit, chunk) = super::held_court_e2e::held_history_fixture_v1();
    let mut later = oldest.clone();
    let PalwConsensusObjectV2::CourtAttnRootClaimedHeld { signature, .. } = &mut later else { unreachable!() };
    *signature = vec![0xAC; 8];
    let mut chain = repeated_daa_chain();
    let insert = |chain: &mut Chain, block: u64, objects: Vec<PalwConsensusObjectV2>| {
        let txs = objects
            .into_iter()
            .map(|object| {
                Transaction::new(
                    TX_VERSION,
                    vec![],
                    vec![],
                    0,
                    SUBNETWORK_ID_PALW_LIFECYCLE,
                    0,
                    borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let count = txs.len();
        chain.blocks.insert(hash(block), Block::new(chain.headers[&hash(block)].as_ref().clone(), txs));
        chain.acceptance.insert(
            hash(block),
            Arc::new(vec![MergesetBlockAcceptanceData {
                block_hash: hash(block),
                accepted_transactions: (0..count)
                    .map(|i| AcceptedTxEntry {
                        transaction_id: chain.blocks[&hash(block)].transactions[i].id(),
                        index_within_block: i as u32,
                    })
                    .collect(),
            }]),
        );
    };
    insert(&mut chain, 2, vec![oldest.clone()]);
    insert(&mut chain, 6, vec![chunk.clone(), chunk.clone()]);
    insert(&mut chain, 8, vec![later.clone(), later.clone(), decoy.clone()]);
    let read = PalwHeldChainReadV1 { session_id: duty.session_id, claim_id: duty.claim_id, not_before_daa: 100 };
    let mut held = PalwHeldCourtV1::default();
    let page = |chain: &Chain, previous| {
        attn_held_objects_page_from_chain_v1(
            chain,
            read,
            previous,
            &duty,
            &[],
            kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
            3,
        )
    };
    let first = page(&chain, None).unwrap();
    assert_eq!(first.objects.len(), 1, "only an authenticated standing root is accumulated, regardless of repeats/decoys");
    assert_eq!(first.walk.next, Some(hash(5)));
    held.note_history_page_v1(duty.session_id, duty.claim_id, first, 102);
    assert!(held.history_walk_v1(duty.session_id).unwrap().next.is_some());
    assert!(
        held.filing_v1(&duty, kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1).is_none(),
        "a partial suffix is not a filing"
    );
    // A new tip does not move the in-progress anchor or skip the original chain's older filing.
    append(&mut chain, 9, 150, 8);
    let original_anchor = held.history_walk_v1(duty.session_id).unwrap().anchor;
    let second = page(&chain, held.history_walk_v1(duty.session_id)).unwrap();
    assert_eq!(second.walk.anchor, original_anchor);
    assert_eq!(second.walk.next, Some(hash(2)));
    let selected = attn_held_objects_page_from_chain_v1(
        &chain,
        read,
        held.history_walk_v1(duty.session_id),
        &duty,
        &[unit],
        kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
        3,
    )
    .unwrap();
    assert!(selected.rewind && !selected.reset, "a newly selected old answer invalidates the walk's previously filtered prefix");
    assert_eq!(selected.walk.anchor, hash(9));
    assert_eq!(selected.walk.next, Some(hash(6)));
    let chunk_page = attn_held_objects_page_from_chain_v1(
        &chain,
        read,
        Some(selected.walk),
        &duty,
        &[unit],
        kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
        3,
    )
    .unwrap();
    assert_eq!(chunk_page.objects.len(), 1);
    assert_eq!(
        chunk_page.objects[&PalwHeldHistoryObjectKeyV1::Chunk { checkpoint: unit.checkpoint, chunk: unit.chunk_index }],
        chunk,
        "repeated old selected chunk is read once from an authenticated carrier"
    );
    held.note_history_page_v1(duty.session_id, duty.claim_id, second, 150);
    let third = page(&chain, held.history_walk_v1(duty.session_id)).unwrap();
    assert_eq!(third.objects[&PalwHeldHistoryObjectKeyV1::Root], oldest);
    assert_eq!(third.walk.next, None);
    held.note_history_page_v1(duty.session_id, duty.claim_id, third, 150);
    assert_eq!(
        held.filing_v1(&duty, kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1).unwrap().0,
        held_court::palw_held_object_digest_v1(&oldest),
        "the oldest authenticated filing stands once the read reaches its floor"
    );
    // The completed anchor belongs to the old branch; a later-only request must still re-read the older filing on a fork.
    append(&mut chain, 20, 160, 3);
    held.invalidate_history_v1(duty.session_id, duty.claim_id);
    assert!(
        held.filing_v1(&duty, kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1).is_none(),
        "a cached old-branch filing cannot drive a challenger move"
    );
    let fork = attn_held_objects_page_from_chain_v1(
        &chain,
        PalwHeldChainReadV1 { not_before_daa: 140, ..read },
        held.history_walk_v1(duty.session_id),
        &duty,
        &[],
        kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
        3,
    )
    .unwrap();
    assert!(fork.reset);
    assert_eq!(fork.walk.floor, 100);
    assert_eq!(fork.objects[&PalwHeldHistoryObjectKeyV1::Root], oldest);
    let mut missing = chain;
    missing.acceptance.remove(&hash(20));
    assert!(page(&missing, held.history_walk_v1(duty.session_id)).unwrap_err().contains("history acceptance"));
    let cap = kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
    assert!(held_court::palw_held_history_object_key_v1(&chunk, &duty, &[], cap).is_none(), "a valid unselected chunk is not cached");
    assert_eq!(
        held_court::palw_held_history_object_key_v1(&chunk, &duty, &[unit], cap),
        Some(PalwHeldHistoryObjectKeyV1::Chunk { checkpoint: unit.checkpoint, chunk: unit.chunk_index })
    );
    let mut corrupt = chunk;
    let PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } = &mut corrupt else { unreachable!() };
    let kaspa_consensus_core::palw_held_da_v1::PalwHeldDisclosureV1::StateChunk { chunk, .. } = &mut disclosure.disclosure else {
        unreachable!()
    };
    chunk.chunk_bytes[0] ^= 1;
    assert!(
        held_court::palw_held_history_object_key_v1(&corrupt, &duty, &[unit], cap).is_none(),
        "a selected chunk must authenticate"
    );
}

/// **ADR-0093 Decision 7's read: accepted objects only, newest first, down to the height.**
/// Four chain blocks at DAA 10/20/30/40 merging three side blocks: a lifecycle object the chain
/// ACCEPTED is visited, one it did not is not, another subnetwork's transaction is not, and the
/// walk stops below `not_before_daa` and after `max_chain_blocks`.
#[test]
fn the_walk_visits_what_the_chain_accepted_newest_first_and_stops_at_the_height() {
    let mut chain = Chain { sink: hash(4), ..Default::default() };
    for (block, daa, parent) in [(1u64, 10u64, 1u64), (2, 20, 1), (3, 30, 2), (4, 40, 3)] {
        let mut header = Header::from_precomputed_hash(hash(block), vec![hash(parent)]);
        header.daa_score = daa;
        chain.headers.insert(hash(block), Arc::new(header));
        chain.parents.insert(hash(block), hash(parent));
    }
    // Side block 13 (merged by 3): claims 5 and 6, only index 0 accepted. Side block 14 (merged
    // by 4): claims 7 and 8 and a native transaction, all accepted. Side block 11 (merged by the
    // lowest chain block): claim 9, accepted — but below any height the tests ask for.
    let side = |n: u64, txs: Vec<Transaction>| Block::new(Header::from_precomputed_hash(hash(n), vec![]), txs);
    chain.blocks.insert(hash(13), side(13, vec![lifecycle(5), lifecycle(6)]));
    chain.blocks.insert(hash(14), side(14, vec![lifecycle(7), other(), lifecycle(8)]));
    chain.blocks.insert(hash(11), side(11, vec![lifecycle(9)]));
    let accepted = |block: u64, indices: &[u32]| MergesetBlockAcceptanceData {
        block_hash: hash(block),
        accepted_transactions: indices
            .iter()
            .map(|i| AcceptedTxEntry {
                transaction_id: chain.blocks[&hash(block)].transactions[*i as usize].id(),
                index_within_block: *i,
            })
            .collect(),
    };
    chain.acceptance.insert(hash(4), Arc::new(vec![accepted(14, &[0, 1, 2])]));
    chain.acceptance.insert(hash(3), Arc::new(vec![accepted(13, &[0])]));
    chain.acceptance.insert(hash(2), Arc::new(vec![]));
    chain.acceptance.insert(hash(1), Arc::new(vec![accepted(11, &[0])]));

    let walk = |not_before: u64, max: usize| {
        let mut seen = Vec::new();
        walk_accepted_lifecycle_objects_v1(&chain, not_before, max, &mut |o| seen.push(claim_of(&o)));
        seen
    };
    assert_eq!(walk(15, 100), vec![8, 7, 5], "accepted lifecycle objects only, newest first, down to DAA 15");
    assert_eq!(walk(0, 100), vec![8, 7, 5, 9], "and the lowest block when the height allows it");
    assert_eq!(walk(35, 100), vec![8, 7], "a height stops the walk");
    assert_eq!(walk(0, 1), vec![8, 7], "and so does the block cap");
    // Nothing in this chain is a root claim: the tile read finds none rather than a wrong one.
    assert!(attn_root_filings_from_chain_v1(&chain, Hash64::from_u64_word(1), 0, 100).is_empty());
}
