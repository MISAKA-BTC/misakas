//! Actual DB proof reconstruction, temporary GHOSTDAG and reverse counts.
use super::*;
use crate::consensus::{services::ConsensusServices, storage::ConsensusStorage};
use kaspa_consensus_core::{
    BlueWorkType,
    config::{
        ConfigBuilder,
        params::{DEVNET_PARAMS, ForkActivation},
    },
};
use kaspa_database::utils::DbLifetime;
use std::sync::atomic::AtomicBool;

fn fixture() -> (DbLifetime, Arc<DB>, Arc<ConsensusServices>) {
    let mut config = ConfigBuilder::new(DEVNET_PARAMS).skip_proof_of_work().build();
    config.params.max_block_level = 0;
    config.params.pow_blake2b_sha3_activation = ForkActivation::never();
    let config = Arc::new(config);
    let (lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let storage = ConsensusStorage::new(db.clone(), config.clone());
    reachability::init(&mut *storage.reachability_store.write()).unwrap();
    crate::processes::relations::init(&mut *storage.reachability_relations_store.write());
    let services = ConsensusServices::new(db.clone(), storage, config, Default::default(), Arc::new(AtomicBool::new(false)));
    (lifetime, db, services)
}

fn header(hash: u64, parents: &[u64], work: u64) -> Arc<Header> {
    let mut header =
        Header::from_precomputed_hash(BlockHash::from_u64_word(hash), parents.iter().copied().map(BlockHash::from_u64_word).collect());
    header.blue_work = BlueWorkType::from_u64(work);
    header.bits = DEVNET_PARAMS.genesis.bits;
    Arc::new(header)
}

fn diamond() -> Vec<Arc<Header>> {
    // Old minimum work/hash queue: root -> parent -> child 100 -> child 90,
    // except child 40 is visible via parent 200 before its other parent 90.
    vec![
        header(300, &[], 10),
        header(200, &[300], 10),
        header(100, &[300, 200], 10),
        header(90, &[300, 200], 10),
        header(40, &[100, 90], 10),
    ]
}

#[test]
fn builder_equal_work_diamond_descriptor_and_actual_ghostdag_keep_every_parent() {
    let (_lifetime, db, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let headers = diamond();
    ppm.populate_reachability_and_headers(&vec![headers.clone()]).unwrap();
    let root = headers[0].hash;
    let tip = headers[4].hash;
    let descriptor = PruningProofDescriptor::new(tip, vec![tip], vec![root], vec![5]);
    let rebuilt = ppm.proof_from_descriptor(&descriptor).unwrap();
    assert!(is_parent_first(&rebuilt[0], |header| header.hash, |header| header.direct_parents().to_vec()));
    assert_eq!(
        rebuilt[0].iter().map(|header| header.hash).collect_vec(),
        vec![root, headers[1].hash, headers[3].hash, headers[2].hash, tip]
    );
    let cache = CachePolicy::Count(32);
    let mut relations = DbRelationsStore::new_temp(db.clone(), 0, 0, cache, cache);
    for header in &headers {
        relations.insert(header.hash, header.direct_parents().to_vec().into()).unwrap();
    }
    let mut ghostdag_factory = GhostdagStoreFactory::new(db.clone(), cache, 0);
    let mut reachability_factory = ReachabilityStoreFactory::new(db, cache, 0);
    let (ghostdag, required, count) = ppm
        .populate_level_proof_ghostdag_data(
            &relations,
            &mut ghostdag_factory,
            &mut reachability_factory,
            root,
            tip,
            headers[3].hash,
            0,
            ppm.ghostdag_k,
        )
        .unwrap();
    assert!(required);
    assert_eq!(count, 5);
    assert_eq!(ghostdag.get_selected_parent(tip).unwrap(), headers[2].hash, "GHOSTDAG's existing maximum-hash tie remains unchanged");
}

#[test]
fn builder_reverse_order_counts_all_equal_work_children_before_their_ancestor() {
    let (_lifetime, db, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let headers = diamond();
    ppm.populate_reachability_and_headers(&vec![headers.clone()]).unwrap();
    let graph = ppm.collect_proof_graph(headers[4].hash, None, 0, None, &mut BlockHashMap::new()).unwrap();
    let reverse = order_graph(&graph, true).unwrap();
    let cache = CachePolicy::Count(32);
    let mut relations = DbRelationsStore::new_temp(db, 0, 0, cache, cache);
    let mut sizes = BlockHashMap::new();
    for hash in reverse {
        relations.insert(hash, graph[&hash].parents.clone().into()).unwrap();
        let size = ppm.count_future_size(&relations, hash, &sizes).unwrap();
        sizes.insert(hash, size);
    }
    assert_eq!(sizes[&headers[0].hash], 4);
    assert_eq!(sizes[&headers[1].hash], 3);
    assert_eq!(sizes[&headers[4].hash], 0);
    let mut missing = sizes.clone();
    missing.remove(&headers[1].hash);
    assert!(ppm.count_future_size(&relations, headers[0].hash, &missing).is_err());
}

#[test]
fn builder_positive_work_full_proof_rebuild_keeps_old_order_and_validates() {
    let (_lifetime, _db, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let mut root = (*header(300, &[], 0)).clone();
    root.hash = ppm.genesis_hash;
    let work = crate::processes::difficulty::calc_work(root.bits);
    let mut parent = (*header(200, &[], 0)).clone();
    parent.parents_by_level = vec![vec![root.hash]].try_into().unwrap();
    parent.blue_work = work;
    parent.blue_score = 1;
    let mut tip = (*header(100, &[200], 0)).clone();
    tip.blue_work = work + work;
    tip.blue_score = 2;
    let headers = vec![Arc::new(root), Arc::new(parent), Arc::new(tip)];
    ppm.populate_reachability_and_headers(&vec![headers.clone()]).unwrap();
    let rebuilt = ppm.try_build_pruning_point_proof(headers[2].hash).unwrap();
    assert_eq!(rebuilt[0].iter().map(|header| header.hash).collect_vec(), headers.iter().map(|header| header.hash).collect_vec());
    ppm.validate_pruning_point_proof_standalone(&rebuilt).unwrap();
    let rebuilt_cached_descriptor = ppm.try_build_pruning_point_proof(headers[2].hash).unwrap();
    assert_eq!(
        rebuilt[0].iter().map(|header| header.hash).collect_vec(),
        rebuilt_cached_descriptor[0].iter().map(|header| header.hash).collect_vec()
    );
}

#[test]
fn builder_order_rejects_cycles_and_missing_included_dependencies() {
    let mut graph = BlockHashMap::new();
    let a = header(100, &[200], 10);
    let b = header(200, &[100], 10);
    graph.insert(a.hash, ProofGraphEntry { header: a.clone(), parents: vec![b.hash] });
    assert!(order_graph(&graph, false).is_err());
    graph.insert(b.hash, ProofGraphEntry { header: b.clone(), parents: vec![a.hash] });
    assert!(order_graph(&graph, false).is_err());
    assert!(order_graph(&graph, true).is_err());
}

#[test]
fn builder_region_excludes_global_descendant_without_level_path_from_root() {
    let (_lifetime, db, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let headers = vec![header(400, &[], 10), header(300, &[400], 10), header(200, &[300], 10), header(100, &[300, 200], 10)];
    ppm.populate_reachability_and_headers(&vec![headers.clone()]).unwrap();
    let cache = CachePolicy::Count(32);
    let mut higher_level = DbRelationsStore::new_temp(db, 0, 0, cache, cache);
    // X=200 is globally after root=300, but at this level its parent is 400.
    for (hash, parents) in [(400, vec![]), (300, vec![400]), (200, vec![400]), (100, vec![300, 200])] {
        higher_level
            .insert(BlockHash::from_u64_word(hash), parents.into_iter().map(BlockHash::from_u64_word).collect_vec().into())
            .unwrap();
    }
    let graph =
        ppm.collect_proof_graph(headers[3].hash, Some(headers[1].hash), 0, Some(&higher_level), &mut BlockHashMap::new()).unwrap();
    assert_eq!(graph.len(), 2);
    assert!(!graph.contains_key(&headers[2].hash));
    assert_eq!(order_graph(&graph, false).unwrap(), vec![headers[1].hash, headers[3].hash]);
}
