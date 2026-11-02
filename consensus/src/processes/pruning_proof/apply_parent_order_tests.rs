//! Import order must follow dependencies even when zero-weight lane blocks tie.
use super::*;
use crate::consensus::{services::ConsensusServices, storage::ConsensusStorage};
use crate::model::stores::{headers::HeaderStoreReader, reachability::ReachabilityStoreReader, relations::RelationsStoreReader};
use kaspa_consensus_core::{
    BlueWorkType,
    config::{
        ConfigBuilder,
        params::{DEVNET_PARAMS, ForkActivation},
    },
};
use kaspa_database::{create_temp_db, prelude::ConnBuilder, utils::DbLifetime};
use std::sync::atomic::AtomicBool;

fn fixture() -> (DbLifetime, Arc<ConsensusServices>) {
    let mut config = ConfigBuilder::new(DEVNET_PARAMS).skip_proof_of_work().build();
    config.params.max_block_level = 1;
    config.params.pow_blake2b_sha3_activation = ForkActivation::never();
    let config = Arc::new(config);
    let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let storage = ConsensusStorage::new(db.clone(), config.clone());
    reachability::init(&mut *storage.reachability_store.write()).unwrap();
    crate::processes::relations::init(&mut *storage.reachability_relations_store.write());
    let services = ConsensusServices::new(db, storage, config, Default::default(), Arc::new(AtomicBool::new(false)));
    (lifetime, services)
}

fn header(hash: u64, parents: &[u64]) -> Arc<Header> {
    let mut header =
        Header::from_precomputed_hash(BlockHash::from_u64_word(hash), parents.iter().copied().map(BlockHash::from_u64_word).collect());
    header.blue_work = BlueWorkType::from_u64(10);
    header.bits = DEVNET_PARAMS.genesis.bits;
    Arc::new(header)
}

#[test]
fn equal_work_import_builds_actual_reachability_before_parent_antichain_queries() {
    let (_lifetime, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let root = header(300, &[]);
    let parent = header(200, &[300]);
    // Redundant ancestor + descendant: the old hash heap queried a parent
    // before either parent's reachability row existed (the live line-63 panic).
    let child = header(100, &[200, 300]);
    ppm.populate_reachability_and_headers(&vec![vec![child.clone(), parent.clone(), root.clone()]]).unwrap();
    let store = ppm.reachability_store.read();
    assert!(store.try_is_dag_ancestor_of(root.hash, child.hash).unwrap());
    assert!(store.try_is_dag_ancestor_of(parent.hash, child.hash).unwrap());
    assert_eq!(store.get_parent(child.hash).unwrap(), parent.hash);
    assert_eq!(ppm.reachability_relations_store.read().get_parents(child.hash).unwrap().as_slice(), &[parent.hash]);
}

#[test]
fn reachability_import_orders_dependencies_from_higher_parent_levels_too() {
    let (_lifetime, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let root = header(300, &[]);
    let parent = header(200, &[300]);
    let mut child = (*header(100, &[300])).clone();
    child.parents_by_level.push(vec![parent.hash]);
    let child = Arc::new(child);
    ppm.populate_reachability_and_headers(&vec![vec![child.clone(), parent.clone(), root.clone()]]).unwrap();
    assert!(ppm.reachability_store.read().try_is_dag_ancestor_of(parent.hash, child.hash).unwrap());
}

#[test]
fn expanded_level_order_cannot_drop_equal_work_parent_appended_as_trusted() {
    let (_lifetime, services) = fixture();
    let root = header(300, &[]);
    let parent = header(200, &[300]);
    let child = header(100, &[200]);
    let mut proof = vec![vec![root.clone(), child.clone(), parent.clone()]];
    services.pruning_proof_manager.order_expanded_proof(&mut proof).unwrap();
    assert_eq!(proof[0].iter().map(|header| header.hash).collect::<Vec<_>>(), vec![root.hash, parent.hash, child.hash]);
    // An already parent-first input keeps the same entries, allocation for allocation.
    let mut ordered = proof.clone();
    services.pruning_proof_manager.order_expanded_proof(&mut ordered).unwrap();
    assert!(ordered[0].iter().zip(&proof[0]).all(|(a, b)| Arc::ptr_eq(a, b)));
}

#[test]
fn cyclic_import_is_typed_error_before_any_header_or_reachability_write() {
    let (_lifetime, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let a = header(100, &[200]);
    let b = header(200, &[100]);
    let proof = vec![vec![a.clone(), b.clone()]];
    assert!(matches!(
        ppm.populate_reachability_and_headers(&proof),
        Err(PruningImportError::PruningPointImportContextUnavailable(_, _))
    ));
    for header in [&a, &b] {
        assert!(ppm.headers_store.get_header(header.hash).is_err());
        assert!(!ppm.reachability_store.read().has(header.hash).unwrap());
    }
    assert!(matches!(
        ppm.order_expanded_proof(&mut proof.clone()),
        Err(PruningImportError::PruningPointImportContextUnavailable(_, _))
    ));
}
