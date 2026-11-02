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
    fixture_at_level(1, None)
}

fn fixture_at_level(max_level: u8, genesis: Option<BlockHash>) -> (DbLifetime, Arc<ConsensusServices>) {
    let mut config = ConfigBuilder::new(DEVNET_PARAMS).skip_proof_of_work().build();
    config.params.max_block_level = max_level;
    if let Some(genesis) = genesis {
        config.params.genesis.hash = genesis;
    }
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

fn apply_checkpoint_fixture(
    headers: Vec<Arc<Header>>,
    upper: Option<Vec<Arc<Header>>>,
    validate: bool,
) -> (DbLifetime, Arc<ConsensusServices>) {
    let genesis = headers.first().unwrap().hash;
    assert!(headers.first().unwrap().parents_by_level.is_empty(), "checkpoint fixture uses canonical genesis parents");
    let (lifetime, services) = fixture_at_level(if upper.is_some() { 1 } else { 0 }, Some(genesis));
    let ppm = &services.pruning_proof_manager;
    crate::processes::relations::init(&mut *ppm.relations_store.write());
    ppm.ghostdag_store.insert(ORIGIN, ppm.ghostdag_manager.origin_ghostdag_data()).unwrap();
    let root = headers.first().unwrap().clone();
    let point = headers.last().unwrap().clone();
    let proof = match upper {
        Some(upper) => vec![headers, upper],
        None => vec![headers],
    };
    if validate {
        ppm.validate_pruning_point_proof_standalone(&proof).unwrap();
    }
    ppm.apply_proof(proof, &[]).unwrap();
    ppm.import_pruning_points(&[root, point]).unwrap();
    (lifetime, services)
}

fn checkpoint_header(hash: u64, parents: &[u64], score: u64, daa: u64) -> Arc<Header> {
    let mut h = (*header(hash, parents)).clone();
    if parents.is_empty() {
        // A real genesis has zero parent levels. `[[]]` instead asks
        // ParentsManager to fall back to genesis at omitted higher levels,
        // creating a synthetic self-parent if this hash is also genesis.
        h.parents_by_level = Default::default();
    }
    h.blue_score = score;
    h.blue_work = crate::processes::difficulty::calc_work(h.bits) * BlueWorkType::from_u64(score);
    h.daa_score = daa;
    Arc::new(h)
}

fn checkpoint_chain() -> Vec<Arc<Header>> {
    vec![
        checkpoint_header(300, &[], 0, 0),
        checkpoint_header(200, &[300], 1, 100),
        checkpoint_header(100, &[200], 2, 100),
        checkpoint_header(50, &[100], 3, 2759),
    ]
}

#[test]
fn checkpoint_chain_is_available_after_actual_apply_and_history_import_without_block_statuses() {
    use crate::model::stores::statuses::{DbStatusesStore, StatusesStoreReader};
    use kaspa_database::prelude::CachePolicy;
    let headers = checkpoint_chain();
    let (_lifetime, services) = apply_checkpoint_fixture(headers.clone(), None, true);
    let ppm = &services.pruning_proof_manager;
    let statuses = DbStatusesStore::new(ppm.db.clone(), CachePolicy::Count(16));
    for h in &headers {
        assert!(ppm.headers_store.get_header(h.hash).is_ok());
        assert!(ppm.ghostdag_store.get_data(h.hash).is_ok());
        assert!(ppm.reachability_store.read().has(h.hash).unwrap());
        assert!(!statuses.has(h.hash).unwrap(), "proof import must not fabricate a normal block status");
    }
    let point = headers.last().unwrap().hash;
    for h in &headers {
        assert!(ppm.chain_contains_checkpoint(point, h.hash, h.daa_score).unwrap(), "equal-DAA chain blocks are not skipped");
    }
}

#[test]
fn checkpoint_dag_ancestor_off_the_selected_parent_chain_is_still_refused() {
    let root = checkpoint_header(300, &[], 0, 0);
    let checkpoint = checkpoint_header(200, &[300], 1, 100);
    let off_chain = checkpoint_header(100, &[300], 1, 100);
    let point = checkpoint_header(50, &[200, 100], 2, 2759);
    let (_lifetime, services) =
        apply_checkpoint_fixture(vec![root, checkpoint.clone(), off_chain.clone(), point.clone()], None, false);
    let ppm = &services.pruning_proof_manager;
    assert!(ppm.reachability_store.read().try_is_dag_ancestor_of(off_chain.hash, point.hash).unwrap());
    assert_eq!(ppm.ghostdag_store.get_selected_parent(point.hash).unwrap(), checkpoint.hash);
    assert!(ppm.chain_contains_checkpoint(point.hash, checkpoint.hash, checkpoint.daa_score).unwrap());
    assert!(!ppm.chain_contains_checkpoint(point.hash, off_chain.hash, off_chain.daa_score).unwrap());
}

#[test]
fn checkpoint_uses_level_zero_ghostdag_even_when_all_level_import_tree_takes_another_parent() {
    let root = checkpoint_header(300, &[], 0, 0);
    let checkpoint = checkpoint_header(100, &[300], 1, 100);
    let other = checkpoint_header(200, &[300], 1, 100);
    let mut point = (*checkpoint_header(50, &[100], 2, 2759)).clone();
    point.parents_by_level.push(vec![other.hash]);
    let point = Arc::new(point);
    let (_lifetime, services) =
        apply_checkpoint_fixture(vec![root.clone(), checkpoint.clone(), point.clone()], Some(vec![root, other.clone()]), false);
    let ppm = &services.pruning_proof_manager;
    assert_eq!(ppm.reachability_store.read().get_parent(point.hash).unwrap(), other.hash);
    assert!(!ppm.reachability_store.read().try_is_chain_ancestor_of(checkpoint.hash, point.hash).unwrap());
    assert_eq!(ppm.ghostdag_store.get_selected_parent(point.hash).unwrap(), checkpoint.hash);
    assert!(ppm.chain_contains_checkpoint(point.hash, checkpoint.hash, checkpoint.daa_score).unwrap());
}

#[test]
fn checkpoint_incomplete_or_corrupt_context_is_typed_local_failure_instead_of_a_false_verdict() {
    let (_lifetime, services) = fixture();
    let ppm = &services.pruning_proof_manager;
    let point = BlockHash::from_u64_word(50);
    let checkpoint = BlockHash::from_u64_word(200);
    assert!(matches!(
        ppm.chain_contains_checkpoint(point, checkpoint, 100),
        Err(PruningImportError::PruningPointImportContextUnavailable(_, _))
    ));
    for fault in ["wrong-target", "header", "ghostdag", "cycle", "premature-origin", "descriptor-count", "wrong-daa", "truncated-root"]
    {
        let (_lifetime, services) = apply_checkpoint_fixture(checkpoint_chain(), None, true);
        let ppm = &services.pruning_proof_manager;
        let mut target = point;
        match fault {
            "wrong-target" => target = BlockHash::from_u64_word(99),
            "wrong-daa" => {}
            "header" => ppm.headers_store.delete(checkpoint).unwrap(),
            "ghostdag" => ppm.ghostdag_store.delete(checkpoint).unwrap(),
            "descriptor-count" | "truncated-root" => {
                let mut descriptor = (*ppm.pruning_point_store.read().pruning_proof_descriptor().unwrap()).clone();
                if fault == "truncated-root" {
                    descriptor.roots[0] = checkpoint;
                } else {
                    descriptor.counts[0] = 1;
                }
                ppm.pruning_point_store.write().set_pruning_proof_descriptor(descriptor).unwrap();
            }
            _ => {
                let mut data = (*ppm.ghostdag_store.get_data(point).unwrap()).clone();
                data.selected_parent = if fault == "cycle" { point } else { ORIGIN };
                ppm.ghostdag_store.delete(point).unwrap();
                ppm.ghostdag_store.insert(point, Arc::new(data)).unwrap();
            }
        }
        assert!(
            matches!(
                ppm.chain_contains_checkpoint(target, checkpoint, if fault == "wrong-daa" { 101 } else { 100 }),
                Err(PruningImportError::PruningPointImportContextUnavailable(_, _))
            ),
            "{fault}"
        );
    }
}
