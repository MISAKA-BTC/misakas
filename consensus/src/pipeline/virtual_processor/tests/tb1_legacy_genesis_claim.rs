//! Execute the funded legacy class's actual canonical draws before the MODEL_AUDIT
//! deadline. A reader needs both the raw winning header and its retained material.
use super::*;
use crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::{RealAttemptV1, certify_executed_claim, executed_certification};
use kaspa_consensus_core::{
    config::class_manifest_const_v1::{QWEN25_A16_8K_MANIFEST_V1, class_id_of_class, inventory_root_of_class},
    palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1,
};
use misaka_palw_base0::{
    artifact::decode_artifact_file_mapped_v1, classes::canonical_classes_v1, inventory::a16_inventory_root_and_count_v1,
    mmap::ReadOnlyMap, qwen25_a16_backend::Qwen25A16Backend,
};
use sha2::{Digest, Sha256};

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
    let artifact = std::sync::Arc::new(decode_artifact_file_mapped_v1(std::sync::Arc::new(ReadOnlyMap::open(held).unwrap())).unwrap());
    let class = canonical_classes_v1(&node.bundle.court).into_iter().find(|c| c.class_id() == id).unwrap();
    assert_eq!(a16_inventory_root_and_count_v1(&artifact, &class.profile).unwrap().0, root);
    let form = palw_prompt_ids_form_of_class_v1(node.config.params.palw_prompt_ids_form_at(facts.daa_score), &class.profile);
    let backend = Qwen25A16Backend::new(artifact, node.config.params.net.to_string().into_bytes(), class.profile, class.canonical_job)
        .expect("held genesis class must be locally executable")
        .with_step_ladder_cap(node.bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(form);
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
    match (write, read) {
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
            certify_executed_claim(&mut node.chain, &mut funds, &result, &backend, digest).await;
            let end = node.chain.sink();
            let blocks = node.blocks_through(end);
            let split = blocks.iter().position(|b| b.hash() == tip).unwrap();
            let branch = CertifiedBranch {
                base: tip,
                claim: result.claim,
                tip: end,
                blocks: blocks[split + 1..].iter().map(RpcRawBlock::from).collect(),
            };
            let mut writer = std::io::BufWriter::new(File::create_new(path).unwrap());
            serde_json::to_writer(&mut writer, &branch).unwrap();
            writer.flush().unwrap();
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
