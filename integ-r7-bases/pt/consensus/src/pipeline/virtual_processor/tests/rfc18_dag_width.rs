//! Dormant width step exercised through the real header/template pipeline and proof replay.
use super::*;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::{config::params::ForkActivation, palw_rfc0018_v1::PalwDagWidthScheduleFenceV1};

fn config(step: Option<u64>) -> kaspa_consensus_core::config::Config {
    let mut c = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    c.params.blockrate.ghostdag_k = 1;
    c.params.blockrate.max_block_parents = 3;
    c.params.blockrate.mergeset_size_limit = 2;
    c.params.blockrate.merge_depth = 4;
    c.params.blockrate.finality_depth = 20;
    c.params.blockrate.pruning_depth = 50;
    c.params.pruning_proof_m = 10;
    c.params.max_block_level = 0;
    c.is_archival = true;
    c.params.palw_dag_width_schedule_v1 = step.map(|at| {
        let mut f = PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::new(at));
        f.schedule.ghostdag_k = 3;
        f.schedule.max_block_parents = 5;
        f.schedule.mergeset_size_limit = 30;
        f.schedule.merge_depth = 8;
        f
    });
    c
}

async fn land(c: &mut TestContext, parents: Vec<BlockHash>, nonce: u64) -> Block {
    let mut b = c.consensus.build_utxo_valid_block_with_parents(blockhash::NONE, parents, c.miner_data.clone(), vec![]);
    c.simulated_time += c.consensus.params().target_time_per_block();
    b.header.timestamp = c.simulated_time;
    b.header.nonce = nonce;
    b.header.finalize();
    let b = b.to_immutable();
    c.validate_and_insert_block(b.clone()).await;
    b
}

#[tokio::test]
async fn rfc18_width_header_template_coloring_and_reorg_at_the_parent_boundary() {
    let mut c = TestContext::new(TestConsensus::new(&config(Some(8))));
    let genesis = c.consensus.params().genesis.hash;
    let mut tip = genesis;
    let mut old = genesis;
    for i in 0..12 {
        let b = land(&mut c, vec![tip], i).await;
        tip = b.hash();
        if [8, 9].contains(&b.header.daa_score) {
            let data = c.consensus.ghostdag_store().get_data(tip).unwrap();
            let root = c.consensus.services.depth_manager.calc_merge_depth_root(&data, c.consensus.pruning_point());
            let root_score = c.consensus.ghostdag_store().get_blue_score(root).unwrap();
            assert_eq!(
                root_score,
                data.blue_score.saturating_sub(if b.header.daa_score == 8 { 4 } else { 8 }).saturating_sub(1),
                "depth increase must walk behind the cached parent root"
            );
        }
        if b.header.daa_score == 6 {
            old = tip;
        }
    }
    assert_ne!(old, genesis);
    for (parent, expected_blues, expected_parents) in [(old, 2, 3), (tip, 4, 5)] {
        let mut siblings = vec![];
        for nonce in 100..104 {
            siblings.push(land(&mut c, vec![parent], nonce).await.hash());
        }
        let g = c.consensus.ghostdag_manager().ghostdag(&siblings);
        assert_eq!(g.mergeset_blues.len(), expected_blues);
        if parent == old {
            let mut too_wide = c.consensus.build_header_only_block_with_parents(blockhash::NONE, siblings[..3].to_vec());
            too_wide.header.timestamp = c.simulated_time + 2;
            too_wide.header.finalize();
            let result = c.consensus.validate_and_insert_block(too_wide.to_immutable()).block_task.await;
            assert!(matches!(result, Err(crate::errors::RuleError::MergeSetTooBig(3, 2))), "{result:?}");
            let mut bad = c.consensus.build_header_only_block_with_parents(blockhash::NONE, siblings.clone());
            bad.header.timestamp = c.simulated_time + 1;
            bad.header.finalize();
            let result = c.consensus.validate_and_insert_block(bad.to_immutable()).block_task.await;
            assert!(matches!(result, Err(crate::errors::RuleError::TooManyParents(4, 3))), "{result:?}");
        } else {
            // Isolate width from randomized selection among stale tips on the old branch.
            let mut clean = TestContext::new(TestConsensus::new(&config(Some(8))));
            let chain = c.consensus.services.reachability_service.forward_chain_iterator(genesis, parent, true).collect::<Vec<_>>();
            for hash in chain.into_iter().skip(1) {
                clean.validate_and_insert_block(c.consensus.get_block(hash).unwrap()).await;
            }
            for hash in &siblings {
                clean.validate_and_insert_block(c.consensus.get_block(*hash).unwrap()).await;
            }
            let template = clean.build_block_template(900, c.simulated_time + 1);
            assert_eq!(template.block.header.direct_parents().len(), 4);
            assert!(template.block.header.direct_parents().len() <= expected_parents);
            let merged = land(&mut c, siblings.clone(), 901).await;
            assert_eq!(c.consensus.ghostdag_store().get_data(merged.hash()).unwrap().mergeset_blues.len(), 4);
        }
    }
    // The old branch keeps the old k even after a new branch crossed activation.
    let mut old_siblings = vec![];
    for n in 200..204 {
        old_siblings.push(land(&mut c, vec![old], n).await.hash());
    }
    assert_eq!(c.consensus.ghostdag_manager().ghostdag(&old_siblings).mergeset_blues.len(), 2);
    let replaced = c.consensus.get_sink();
    let mut alternative = old_siblings[0];
    for n in 300..318 {
        alternative = land(&mut c, vec![alternative], n).await.hash();
    }
    assert_ne!(alternative, replaced);
    assert_eq!(c.consensus.get_sink(), alternative, "a stronger fork from before activation is actually adopted");
}

#[tokio::test]
async fn rfc18_width_prefence_header_bytes_match_the_old_release_and_ibd_replay() {
    let a_config = config(Some(100));
    let b_config = config(None);
    let mut a = TestContext::new(TestConsensus::new(&a_config));
    let mut b = TestContext::new(TestConsensus::new(&b_config));
    // Shared miner data and nonce make the produced bytes comparable.
    b.miner_data = a.miner_data.clone();
    let mut tip = a_config.params.genesis.hash;
    let mut history = Vec::new();
    for row in 0..4 {
        let mut siblings = Vec::new();
        for n in 0..2 {
            let nonce = row * 3 + n;
            let sent = land(&mut a, vec![tip], nonce).await;
            let rebuilt = land(&mut b, vec![tip], nonce).await;
            assert_eq!(borsh::to_vec(sent.header.as_ref()).unwrap(), borsh::to_vec(rebuilt.header.as_ref()).unwrap());
            assert_eq!(borsh::to_vec(sent.transactions.as_ref()).unwrap(), borsh::to_vec(rebuilt.transactions.as_ref()).unwrap());
            siblings.push(sent.hash());
            history.push(sent);
        }
        let sent = land(&mut a, siblings.clone(), row * 3 + 2).await;
        let rebuilt = land(&mut b, siblings, row * 3 + 2).await;
        assert_eq!(borsh::to_vec(sent.header.as_ref()).unwrap(), borsh::to_vec(rebuilt.header.as_ref()).unwrap());
        assert_eq!(borsh::to_vec(sent.transactions.as_ref()).unwrap(), borsh::to_vec(rebuilt.transactions.as_ref()).unwrap());
        tip = sent.hash();
        history.push(sent);
    }
    // A valid old-era range request keeps the old capacity contract on the upgraded node.
    // Using the future upper bound here would panic on this legitimate request (3 > old 2).
    let range_a = a.consensus.get_hashes_between(a_config.params.genesis.hash, tip, 3).unwrap();
    let range_b = b.consensus.get_hashes_between(b_config.params.genesis.hash, tip, 3).unwrap();
    assert!(!range_a.0.is_empty());
    assert_eq!(range_a, range_b);
    let mut fresh = TestContext::new(TestConsensus::new(&a_config));
    // A fresh node receives complete DAG batches, with siblings in the opposite arrival order.
    for row in history.chunks_exact(3) {
        for index in [1, 0, 2] {
            fresh.validate_and_insert_block(row[index].clone()).await;
        }
    }
    assert_eq!(fresh.consensus.get_sink(), tip);
    let (sent, received) =
        (a.consensus.ghostdag_store().get_data(tip).unwrap(), fresh.consensus.ghostdag_store().get_data(tip).unwrap());
    assert_eq!(
        (sent.blue_score, sent.blue_work, sent.selected_parent),
        (received.blue_score, received.blue_work, received.selected_parent)
    );
    assert_eq!(
        (&sent.mergeset_blues, &sent.mergeset_reds, &sent.blues_anticone_sizes),
        (&received.mergeset_blues, &received.mergeset_reds, &received.blues_anticone_sizes)
    );
}

#[tokio::test]
async fn rfc18_width_pruning_proof_build_validate_apply_and_rebuild_cross_the_step() {
    use kaspa_consensus_core::pruning::PruningProofMetadata;
    let mut config = config(Some(85));
    config.params.max_block_level = 225; // retain the shipped proof-header budget and hierarchy
    let mut source = TestContext::new(TestConsensus::new(&config));
    let mut tip = config.params.genesis.hash;
    let mut pp;
    let mut width_merge = None;
    // Archival source retains the complete evidence used by the proof builder.
    loop {
        if source.consensus.virtual_processor().headers_store.get_daa_score(tip).unwrap() == 85 && width_merge.is_none() {
            let mut siblings = vec![];
            for n in 1000..1004 {
                siblings.push(land(&mut source, vec![tip], n).await.hash());
            }
            let merged = land(&mut source, siblings, 1005).await;
            assert_eq!(source.consensus.ghostdag_store().get_data(merged.hash()).unwrap().mergeset_blues.len(), 4);
            tip = merged.hash();
            width_merge = Some(tip);
        }
        let n = source.consensus.get_virtual_daa_score();
        assert!(n < 300, "pruning point did not advance");
        let b = land(&mut source, vec![tip], n).await;
        tip = b.hash();
        pp = source.consensus.pruning_point();
        if source.consensus.virtual_processor().headers_store.get_daa_score(pp).unwrap() >= 100 {
            break;
        }
    }
    // This hash-lineage fixture advances pruning naturally; serve its actual point.
    let proof = source.consensus.get_pruning_point_proof();
    pp = proof[0].last().unwrap().hash;
    assert!(proof[0].iter().any(|h| h.daa_score < 85));
    assert!(proof[0].iter().any(|h| h.daa_score >= 85));
    assert!(proof[0].iter().any(|h| Some(h.hash) == width_merge), "proof must exercise the wider coloring, not just a linear step");
    eprintln!(
        "[rfc18 width proof] headers={} start_daa={} end_daa={} widened_merge={}",
        proof[0].len(),
        proof[0].first().unwrap().daa_score,
        proof[0].last().unwrap().daa_score,
        width_merge.unwrap()
    );

    let fresh = TestContext::new(TestConsensus::new(&config));
    fresh
        .consensus
        .validate_pruning_proof(&proof, &PruningProofMetadata::new(source.consensus.get_block(tip).unwrap().header.blue_work))
        .unwrap();
    let mut staged_config = config.clone();
    staged_config.process_genesis = false;
    let staged = TestContext::new(TestConsensus::new(&staged_config));
    let trusted = vec![kaspa_consensus_core::trusted::TrustedBlock::new(
        source.consensus.get_block(pp).unwrap(),
        source.consensus.ghostdag_store().get_data(pp).unwrap().as_ref().into(),
    )];
    staged.consensus.apply_pruning_proof((*proof).clone(), &trusted).unwrap();
    let point_blue = source.consensus.virtual_processor().headers_store.get_blue_score(pp).unwrap();
    let points = source.consensus.pruning_point_headers().into_iter().filter(|h| h.blue_score <= point_blue).collect();
    staged.consensus.import_pruning_points(points).unwrap();
    let rebuilt = staged.consensus.get_pruning_point_proof();
    assert_eq!(proof[0].iter().map(|h| h.hash).collect::<Vec<_>>(), rebuilt[0].iter().map(|h| h.hash).collect::<Vec<_>>());
    for header in proof[0].iter().filter(|h| h.hash != config.params.genesis.hash) {
        let a = source.consensus.ghostdag_store().get_data(header.hash).unwrap();
        let b = staged.consensus.ghostdag_store().get_data(header.hash).unwrap();
        assert_eq!((&a.mergeset_blues, &a.mergeset_reds), (&b.mergeset_blues, &b.mergeset_reds));
    }
}
