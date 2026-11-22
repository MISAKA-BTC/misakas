//! Execute the funded legacy class's actual canonical draws before the MODEL_AUDIT
//! deadline. A reader needs both the raw winning header and its retained material.
use super::*;
use crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::{
    AfterPanelFinal, RealAttemptV1, executed_certification, review_executed_claim,
};
use kaspa_consensus_core::{
    config::class_manifest_const_v1::{QWEN25_A16_8K_MANIFEST_V1, class_id_of_class, inventory_root_of_class},
    palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1,
};
use misaka_palw_base0::{
    artifact::decode_artifact_file_mapped_v1, classes::canonical_classes_v1, inventory::a16_inventory_root_and_count_v1,
    mmap::ReadOnlyMap, qwen25_a16_backend::Qwen25A16Backend,
};
use sha2::{Digest, Sha256};

#[path = "tb1_legacy_genesis_anchor.rs"]
mod funded_anchor;

#[derive(Serialize, Deserialize)]
struct WinningBlock {
    base: Hash64,
    claim: Hash64,
    raw: RpcRawBlock,
}

#[derive(Serialize, Deserialize)]
struct CertifiedBranch {
    base: Hash64,
    claim: Hash64,
    tip: Hash64,
    blocks: Vec<RpcRawBlock>,
}

fn load_backend(node: &Rig, held: &Path, id: Hash64, root: Hash64, daa: u64) -> Qwen25A16Backend {
    let artifact = std::sync::Arc::new(decode_artifact_file_mapped_v1(std::sync::Arc::new(ReadOnlyMap::open(held).unwrap())).unwrap());
    let class = canonical_classes_v1(&node.bundle.court).into_iter().find(|c| c.class_id() == id).unwrap();
    assert_eq!(a16_inventory_root_and_count_v1(&artifact, &class.profile).unwrap().0, root);
    let form = palw_prompt_ids_form_of_class_v1(node.config.params.palw_prompt_ids_form_at(daa), &class.profile);
    Qwen25A16Backend::new(artifact, node.config.params.net.to_string().into_bytes(), class.profile, class.canonical_job)
        .expect("held genesis class must be locally executable")
        .with_step_ladder_cap(node.bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(form)
}

pub(in super::super) async fn extend_with_certified_work(
    node: &mut Rig,
    held: &Path,
    funding: TransactionOutpoint,
    close_blue: u64,
    output: &Path,
) {
    let base = node.chain.sink();
    let mut funds = node
        .api()
        .get_virtual_utxos(None, 1_000_000, false)
        .into_iter()
        .find(|(outpoint, _)| *outpoint == funding)
        .expect("recovery work requires an unspent lifecycle funding output");
    genesis_seats::renew(node, held, &mut funds).await;

    let id = class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let root = inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let facts = node.api().palw_producer_facts_v2(id, Some(node.chain.bonds[1].0)).unwrap();
    assert!(!facts.is_base_class && facts.artifact_root == root);
    facts
        .ready_to_produce_v3(
            TestConsensus::palw_v2_registry_keypair(1).verification_key.as_ref(),
            node.bundle.state.rcore_plus_active_at(facts.daa_score),
        )
        .unwrap();
    let backend = load_backend(node, held, id, root, facts.daa_score);
    let step = node.config.params.target_time_per_block();
    let result = node.chain.real_attempt_with_backend(1, step, id, &backend).await;
    let digest = result.replay_with_backend(&node.chain, &backend).expect("verify the new work on its own selected branch");
    review_executed_claim(&mut node.chain, &mut funds, &result, &backend, digest, AfterPanelFinal::Executed).await;
    let cert = executed_certification(&node.chain, result.claim);
    let tip = node.chain.sink();
    assert!(node.api().get_header(tip).unwrap().blue_score < close_blue, "additional work ran past the original assessment close");
    assert!(node.chain.tip_state().1.safe_frontier().0 >= result.block.header.blue_score);
    funded_anchor::save(node, result.claim, base, output);
    peer_log!(
        "arithmetic PANEL recovery work {}",
        serde_json::json!({"base":base,"claim":result.claim,"accepted":result.block.hash(),"tip":tip,
            "certified_daa":cert.outcome.unwrap().1,"approvals":cert.positive_positions()})
    );
}

pub(super) async fn run(node: &mut Rig, held: &Path, dir: &Path, produce: bool, renewed: Hash64) {
    let id = class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let root = inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let (base, state) = node.chain.tip_state();
    let facts = node.api().palw_producer_facts_v2(id, Some(node.chain.bonds[1].0)).unwrap();
    assert_eq!(facts.artifact_root, root);
    facts
        .ready_to_produce_v3(
            TestConsensus::palw_v2_registry_keypair(1).verification_key.as_ref(),
            node.bundle.state.rcore_plus_active_at(facts.daa_score),
        )
        .unwrap();
    assert!(!facts.is_base_class && facts.class_target > 0);
    assert!(state.bond_budget().unwrap().model_blocks_available_v1(&id, facts.daa_score) > 0);
    let backend = load_backend(node, held, id, root, facts.daa_score);
    let record = dir.join("winning.json");
    let capture = dir.join("capture.bin");
    let result = if produce {
        std::fs::create_dir(dir).expect("fresh output directory for the real draw");
        let step = node.config.params.target_time_per_block();
        let result = node.chain.draw_with_backend(1, step, id, &backend, None).await;
        let row = WinningBlock { base, claim: result.claim, raw: RpcRawBlock::from(&result.block) };
        let mut bytes = File::create_new(&capture).unwrap();
        bytes.write_all(&result.material).unwrap();
        bytes.sync_all().unwrap();
        let mut head = File::create_new(&record).unwrap();
        serde_json::to_writer(&mut head, &row).unwrap();
        head.sync_all().unwrap();
        result
    } else {
        let row: WinningBlock = serde_json::from_reader(File::open(&record).unwrap()).unwrap();
        assert_eq!(row.base, base);
        let block = Block::try_from(row.raw.clone()).unwrap();
        genesis_seats::replay(node, &row.raw).await;
        RealAttemptV1 { block, claim: row.claim, material: std::fs::read(&capture).unwrap() }
    };
    let digest = result.replay_with_backend(&node.chain, &backend).expect("independent replay of actual non-floor job");
    let (tip, state) = node.chain.tip_state();
    let claim = state.claim(&result.claim).unwrap();
    assert_eq!((claim.class_id, claim.accepted_block, claim.bond), (id, tip, node.chain.bonds[1]));
    peer_log!(
        "legacy genesis claim {} {}",
        std::process::id(),
        serde_json::json!({
            "base":base,"class":id,"artifact":root,"claim":result.claim,"tip":tip,"state":state.state_root(),"daa":node.chain.daa_of(tip),
            "target":format!("{:032x}",facts.class_target),"phase":format!("{:?}",claim.phase),"replay":faster_hex::hex_string(&digest),
            "capture_bytes":result.material.len(),"capture_sha256":faster_hex::hex_string(&Sha256::digest(&result.material)),
            "trace":claim.trace_root,"output":claim.output_root,"execution":claim.execution_root
        })
    );
    let write = std::env::var_os("MISAKA_PRIVATE_GENESIS_CERTIFY_WRITE");
    let read = std::env::var_os("MISAKA_PRIVATE_GENESIS_CERTIFY_READ");
    let checkpoint = std::env::var_os("MISAKA_PRIVATE_GENESIS_FINAL_WRITE");
    assert!(checkpoint.is_none() || (write.is_none() && read.is_none()), "choose one Final or certification output");
    if let Some(path) = std::env::var_os("MISAKA_PRIVATE_GENESIS_SUFFIX_READ") {
        assert!(checkpoint.is_none() && write.is_none() && read.is_none(), "cold suffix inspection must not mine or attest");
        let branch: CertifiedBranch = serde_json::from_reader(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
        assert_eq!((branch.base, branch.claim), (tip, result.claim));
        assert!(!branch.blocks.is_empty());
        for raw in &branch.blocks {
            genesis_seats::replay(node, raw).await;
        }
        let (end, state) = node.chain.tip_state();
        assert_eq!(end, branch.tip);
        let claim = state.claim(&result.claim).unwrap();
        assert_eq!(claim.accepted_block, tip);
        use kaspa_consensus_core::palw_security_certification_v1::*;
        let route = state.kernel_route_state_v1().unwrap();
        let row: PalwClaimCertificationRowV1 = route
            .aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_claim_key_v1(&palw_certification_id_v1(&result.claim)))
            .unwrap();
        let network: PalwCertificationNetworkRowV1 =
            route.aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_network_key_v1()).unwrap_or_default();
        let daa = node.chain.daa_of(end);
        let anchor = state.round_seed_anchor().map(|a| (a.span, a.block));
        let policy = node.chain.bundle.state.rfc0018_v1().unwrap().certification;
        peer_log!(
            "legacy genesis recovered {} {}",
            std::process::id(),
            serde_json::json!({
                "claim":result.claim,"accepted":tip,"tip":end,"state":state.state_root(),"blocks":branch.blocks.len(),"daa":daa,
                "phase":format!("{:?}",claim.phase),"final_daa":row.final_daa,"committed_span":row.committed_span,
                "current_seed_anchor":anchor,"drawn_daa":row.drawn_daa,"draws":row.draws.len(),"approvals":row.positive_positions(),
                "outcome":format!("{:?}",row.outcome),"epoch":network.epoch,"network_draws":network.draws,
                "epoch_cap":policy.draws_per_epoch,
                "anchor_delay_spans":policy.anchor_delay_spans,"certify_by_daa":row.committed_daa+policy.certify_by_daa,
                "claim_local_settlement":node.chain.bundle.state.claim_local_settlement_active_at(claim.accepted_daa),
                "replay":faster_hex::hex_string(&digest)
            })
        );
        if let Some(path) = std::env::var_os("MISAKA_PRIVATE_GENESIS_FUNDED_ANCHOR") {
            assert!(row.outcome.is_none() && row.anchor.is_none() && row.draws.is_empty());
            funded_anchor::continue_final(
                node,
                &branch,
                &result,
                &backend,
                digest,
                row.committed_daa + policy.certify_by_daa,
                Path::new(&path),
            )
            .await;
        }
        return;
    }
    let final_only = checkpoint.is_some();
    match (checkpoint.or(write), read) {
        (Some(path), None) => {
            let mut funds = genesis_seats::funding_change(node, renewed);
            let slot = claim.bind_base_daa() + node.bundle.panel.anchor_delay();
            let registry = node.api().palw_model_registry_v1().unwrap();
            let expires: Vec<_> = registry
                .readiness
                .iter()
                .filter(|r| r.class_id == id)
                .map(|r| (r.bond, r.row.proved_daa.saturating_add(registry.readiness_max_age_daa)))
                .collect();
            let stale_at_slot = expires.iter().filter(|(_, until)| *until < slot).count();
            peer_log!(
                "legacy genesis seat horizon claim={} slot={} max_age={} expiring_before_slot={} seats={:?}",
                result.claim,
                slot,
                registry.readiness_max_age_daa,
                stale_at_slot,
                expires
            );
            if stale_at_slot > 0 {
                genesis_seats::renew(node, held, &mut funds).await;
            }
            let after_final = if final_only { AfterPanelFinal::Checkpoint } else { AfterPanelFinal::Executed };
            review_executed_claim(&mut node.chain, &mut funds, &result, &backend, digest, after_final).await;
            funded_anchor::save(node, result.claim, tip, Path::new(&path));
            if final_only {
                let (end, state) = node.chain.tip_state();
                let row = state.claim(&result.claim).unwrap();
                assert_eq!(row.accepted_block, tip);
                peer_log!(
                    "legacy genesis final checkpoint claim={} accepted={} tip={} phase={:?} state={}",
                    result.claim,
                    tip,
                    end,
                    row.phase,
                    state.state_root()
                );
                return;
            }
        }
        (None, Some(path)) => {
            let branch: CertifiedBranch = serde_json::from_reader(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
            assert_eq!((branch.base, branch.claim), (tip, result.claim));
            assert!(!branch.blocks.is_empty());
            for raw in &branch.blocks {
                genesis_seats::replay(node, raw).await;
            }
            assert_eq!(node.chain.sink(), branch.tip);
        }
        (None, None) => return,
        _ => panic!("certification either writes or reads one branch"),
    }
    let row = executed_certification(&node.chain, result.claim);
    assert!(row.attestations.iter().all(|a| a.transcript == digest));
    let (end, state) = node.chain.tip_state();
    let claim = state.claim(&result.claim).unwrap();
    assert_eq!(claim.accepted_block, tip);
    assert!(matches!(claim.phase, kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Final { .. }));
    peer_log!(
        "legacy genesis certification {} {}",
        std::process::id(),
        serde_json::json!({"claim":result.claim,"class":claim.class_id,"accepted":tip,"final_daa":row.final_daa,
            "certified_daa":row.outcome.unwrap().1,"drawn":row.draws.len(),"approvals":row.positive_positions(),
            "replay":faster_hex::hex_string(&digest),"tip":end,"state":state.state_root(),"frontier":state.safe_frontier()})
    );
}

/// Export a retained private suffix from a read-only database copy. Readers
/// validate the raw blocks from the signed job as usual; no state is imported.
#[test]
#[ignore = "requires a copied private consensus DB and the accepted native job"]
fn export_genesis_claim_suffix() {
    use crate::model::stores::{
        block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
        evm::{DbEvmPayloadStore, EvmPayloadStoreReader},
        ghostdag::{DbGhostdagStore, GhostdagStoreReader},
        headers::{DbHeadersStore, HeaderStoreReader},
        palw_state_v2::DbPalwStateV2Store,
    };
    use kaspa_database::prelude::{CachePolicy::Empty, StoreResultExt};
    use std::sync::Arc;
    let files_limit = 32;
    let guard = kaspa_utils::fd_budget::acquire_guard(files_limit).unwrap();
    let mut options = rocksdb::Options::default();
    options.set_max_open_files(files_limit);
    let inner = rocksdb::DBWithThreadMode::<rocksdb::MultiThreaded>::open_for_read_only(
        &options,
        std::env::var_os("MISAKA_PRIVATE_GENESIS_SUFFIX_DB").unwrap(),
        false,
    )
    .unwrap();
    let db = Arc::new(kaspa_database::prelude::DB::new(inner, guard));
    let held = std::path::PathBuf::from(std::env::var_os("MISAKA_PRIVATE_GENESIS_ATTEMPT_READ").unwrap());
    let winning: WinningBlock = serde_json::from_reader(File::open(held.join("winning.json")).unwrap()).unwrap();
    let base = Block::try_from(winning.raw).unwrap().hash();
    let headers = DbHeadersStore::new(db.clone(), Empty, Empty);
    let ghost = DbGhostdagStore::new(db.clone(), 0, Empty, Empty);
    let bodies = DbBlockTransactionsStore::new(db.clone(), Empty);
    let payloads = DbEvmPayloadStore::new(db.clone(), Empty);
    use crate::pipeline::virtual_processor::tests::g14_registration_e2e::private_moe2::arm_private_registration;
    use kaspa_consensus_core::{
        palw_attempt_v2::{PalwAttemptEnvelopeV2, attempt_id_v2},
        palw_mode_v2::PalwConsensusMode,
        palw_security_certification_v1::*,
        palw_state_v2::PalwDeltaEntryV2,
    };
    let (config, _, _, _) = parts_custom(Some(FENCE), false, |params| arm_private_registration(params, true));
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let store = DbPalwStateV2Store::new(db, Empty)
        .with_uncertified_weightless(config.params.palw_uncertified_weightless)
        .with_canonical_work_daa(config.params.palw_canonical_work_daa())
        .with_native39_frontier_context_v1(&config.params);
    let point = store.tip_record().unwrap().unwrap();
    let (tip, state) = store.load_tip(&bundle.state).unwrap().unwrap();
    assert_eq!((tip, state.state_root()), (point.block, point.state_root));
    let route = state.kernel_route_state_v1().unwrap();
    let row: PalwClaimCertificationRowV1 = route
        .aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_claim_key_v1(&palw_certification_id_v1(&winning.claim)))
        .unwrap();
    let network: PalwCertificationNetworkRowV1 =
        route.aux_row(PALW_SECURITY_CERTIFICATION_TABLE_V1, &certification_network_key_v1()).unwrap_or_default();
    let mut candidates = 0usize;
    let mut admitted = 0usize;
    let mut seeded = 0usize;
    let start_daa = headers.get_header(base).unwrap().daa_score;
    let mut at = point.block;
    let mut blocks = Vec::new();
    while at != base {
        let header = headers.get_header(at).unwrap();
        assert_eq!(kaspa_consensus_core::hashing::header::hash(&header), at);
        assert!(header.daa_score >= start_daa);
        if header.daa_score >= row.final_daa && !header.palw_commitment.is_empty() {
            let envelope = PalwAttemptEnvelopeV2::decode_wire(&header.palw_commitment).unwrap();
            let claim = attempt_id_v2(&envelope.attempt);
            let (_, delta) = store.delta_of(at).unwrap();
            candidates += 1;
            admitted += usize::from(
                delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::Claim { key, old: None, new: Some(_) } if *key == claim)),
            );
            seeded += usize::from(
                delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::RoundSeedAnchor { new: Some(a), .. } if a.block == at)),
            );
        }
        let block = Block {
            header,
            transactions: bodies.get(at).unwrap(),
            evm_payload: Arc::new(payloads.get(at).optional().unwrap().unwrap_or_default()),
        };
        let raw = RpcRawBlock::from(&block);
        assert_eq!(Block::try_from(raw.clone()).unwrap().hash(), at);
        blocks.push(raw);
        at = ghost.get_selected_parent(at).unwrap();
    }
    blocks.reverse();
    assert!(!blocks.is_empty());
    let count = blocks.len();
    let end_daa = blocks.last().unwrap().header.daa_score;
    peer_log!(
        "legacy genesis journal {}",
        serde_json::json!({
            "claim": winning.claim, "tip": tip, "state_root":point.state_root, "daa":end_daa,
            "final_daa":row.final_daa,"committed_span":row.committed_span,"drawn_daa":row.drawn_daa,
            "draws":row.draws.len(),"outcome":format!("{:?}",row.outcome),"approvals":row.positive_positions(),
            "epoch":network.epoch,"network_draws":network.draws,"epoch_cap":bundle.state.rfc0018_v1().unwrap().certification.draws_per_epoch,
            "post_final_attempt_blocks":candidates,"attempt_claims_created":admitted,"seed_anchor_writes":seeded,
            "latest_seed":state.round_seed_anchor().map(|a| (a.span,a.block))
        })
    );
    let mut writer =
        std::io::BufWriter::new(File::create_new(std::env::var_os("MISAKA_PRIVATE_GENESIS_SUFFIX_WRITE").unwrap()).unwrap());
    serde_json::to_writer(&mut writer, &CertifiedBranch { base, claim: winning.claim, tip: point.block, blocks }).unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
    eprintln!(
        "exported accepted job={} base={} tip={} daa={} state={} blocks={}",
        winning.claim, base, point.block, end_daa, point.state_root, count
    );
}
