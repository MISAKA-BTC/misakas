//! Exercise the pre-existing genesis class's ordinary seat duties on the
//! selected MODEL_AUDIT fork. Only the raw ordinary blocks travel to readers.
use super::*;
use kaspa_consensus_core::{
    config::class_manifest_const_v1::{QWEN25_A16_8K_MANIFEST_V1, class_id_of_class, inventory_root_of_class},
    palw_artifact::{PalwArtifactOpeningV1, palw_artifact_multiproof_from_openings_v1, verify_artifact_multiproof_v1},
    palw_model_registry_v1::*,
    tx::TransactionOutpoint,
};
use libcrux_ml_dsa::ml_dsa_87;
use misaka_palw_base0::{
    artifact::decode_artifact_file_mapped_v1,
    classes::canonical_classes_v1,
    inventory::{a16_inventory_root_and_count_v1, a16_readiness_openings_streamed_v1},
    mmap::ReadOnlyMap,
};
use std::collections::{BTreeMap, BTreeSet};

fn challenge(class: Hash64, bond: &[u8], span: u64, count: u32, v2: bool) -> Vec<u32> {
    if v2 {
        let seed = palw_readiness_v2_challenge_seed_v1(&class, bond, span);
        return palw_readiness_v2_draw_v1(&seed, count);
    }
    let seed = palw_readiness_challenge_seed_v1(&class, bond, span);
    let (start, width) = palw_readiness_window_v1(&seed, count);
    (0..width).map(|off| ((u64::from(start) + u64::from(off)) % u64::from(count)) as u32).collect()
}

#[derive(Serialize, Deserialize)]
struct Extension {
    base: Hash64,
    blocks: Vec<RpcRawBlock>,
    // Archives from the seat-only run end before the next registry evaluation.
    #[serde(default)]
    boundary_blocks: Vec<RpcRawBlock>,
}

pub(super) async fn replay(node: &mut Rig, raw: &RpcRawBlock) {
    let block = Block::try_from(raw.clone()).unwrap();
    let parent = node.chain.sink();
    let hash = block.hash();
    stable_panel::arrive(node, block, "recorded legacy seat duty").await;
    assert_eq!(node.chain.sink(), hash);
    assert_eq!(node.api().get_ghostdag_data(hash).unwrap().selected_parent, parent);
}

pub(super) fn funding_change(node: &Rig, assigned: Hash64) -> (TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry) {
    let issuer = node.api().get_ghostdag_data(assigned).unwrap().selected_parent;
    let carried = node.api().get_block(issuer).unwrap();
    let tx = carried
        .transactions
        .iter()
        .find(|tx| tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE)
        .unwrap();
    let change = TransactionOutpoint::new(tx.id(), 0);
    node.api().get_virtual_utxos(None, 1_000_000, false).into_iter().find(|(o, _)| o == &change).unwrap()
}

pub(super) async fn run(node: &mut Rig, assigned: Hash64, artifact_path: &Path, archive_prefix: &str) -> Hash64 {
    let id = class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let root = inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let (base, before) = node.chain.tip_state();
    assert_eq!(before.class(&id).unwrap().artifact_root, root);
    assert!(before.class(&id).unwrap().registrant_bond.is_none());
    let read = node.api().palw_model_registry_v1().unwrap();
    let span_daa = read.span_daa;
    let start = read.classes.iter().find(|r| r.class_id == id).unwrap();
    assert_eq!(start.artifact_root, root);
    let mut blocks = Vec::new();
    let mut boundary_blocks = Vec::new();
    if let Some(path) = std::env::var_os(format!("{archive_prefix}_READ")) {
        let ext: Extension = serde_json::from_reader(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
        assert_eq!(ext.base, base);
        assert_eq!(ext.blocks.len(), 16);
        blocks = ext.blocks;
        boundary_blocks = ext.boundary_blocks;
        for raw in &blocks {
            replay(node, raw).await;
        }
    } else {
        let map = std::sync::Arc::new(ReadOnlyMap::open(artifact_path).unwrap());
        let artifact = decode_artifact_file_mapped_v1(map).unwrap();
        let class = canonical_classes_v1(&node.bundle.court).into_iter().find(|c| c.class_id() == id).unwrap();
        let (held_root, count) = a16_inventory_root_and_count_v1(&artifact, &class.profile).unwrap();
        assert_eq!(held_root, root);
        let mut openings = BTreeMap::<u32, PalwArtifactOpeningV1>::new();
        let mut funds = funding_change(node, assigned);
        for card in 0..8 {
            let bond = node.chain.bonds[card];
            let daa = node.chain.daa_of(node.chain.sink());
            let span = daa / span_daa;
            let bytes = borsh::to_vec(&bond).unwrap();
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            let v2 = node.config.params.palw_readiness_v2_at(daa);
            let draw = challenge(id, &bytes, span, count, v2);
            if !draw.iter().all(|i| openings.contains_key(i)) {
                // Precompute upcoming paths for this cadence. Every signature uses
                // the draw at the actual selected tip; a drift reloads missing paths.
                let indices: BTreeSet<_> = node.chain.bonds[card..]
                    .iter()
                    .enumerate()
                    .flat_map(|(offset, b)| {
                        let at = daa + offset as u64;
                        challenge(id, &borsh::to_vec(b).unwrap(), at / span_daa, count, node.config.params.palw_readiness_v2_at(at))
                    })
                    .collect();
                let indices: Vec<_> = indices.into_iter().collect();
                let (opened_root, opened_count, paths) =
                    a16_readiness_openings_streamed_v1(&artifact, &class.profile, &indices).unwrap();
                assert_eq!((opened_root, opened_count), (root, count));
                openings = paths.into_iter().map(|p| (p.leaf_index, p)).collect();
                assert_eq!(openings.len(), indices.len());
            }
            let object = if v2 {
                let mut opened = Vec::new();
                let mut size = 0;
                for i in &draw {
                    if size >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                        break;
                    }
                    let path = &openings[i];
                    assert!(path.operand.bytes.len() <= PALW_READINESS_V2_LEAF_MAX_BYTES_V1);
                    size += path.operand.bytes.len();
                    opened.push(path.clone());
                }
                palw_readiness_v2_opening_is_the_challenge_v1(
                    &draw,
                    &opened.iter().map(|p| (p.leaf_index, p.operand.bytes.len())).collect::<Vec<_>>(),
                )
                .unwrap();
                let proof = palw_artifact_multiproof_from_openings_v1(&opened).unwrap();
                verify_artifact_multiproof_v1(&proof, root).unwrap();
                let message = palw_seat_readiness_message_v2(node.domain, &bytes, &id, span, &proof);
                let signature =
                    ml_dsa_87::sign(&key.signing_key, message.as_byte_slice(), PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, [0x6d; 32])
                        .unwrap()
                        .as_slice()
                        .to_vec();
                PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id: id, span, proof: Box::new(proof), signature }
            } else {
                let opening = draw
                    .iter()
                    .map(|i| &openings[i])
                    .find(|p| p.operand.bytes.len() <= PALW_READINESS_OPENING_MAX_BYTES_V1)
                    .unwrap()
                    .clone();
                let message = palw_seat_readiness_message_v1(node.domain, &bytes, &id, span, opening.leaf_index);
                let signature =
                    ml_dsa_87::sign(&key.signing_key, message.as_byte_slice(), PALW_SEAT_READINESS_V1_MLDSA87_CONTEXT, [0x6d; 32])
                        .unwrap()
                        .as_slice()
                        .to_vec();
                PalwConsensusObjectV2::SeatReadinessProved { bond, class_id: id, span, opening, signature }
            };
            send_lifecycle(&mut node.chain, &mut funds, object).await;
            let (tip, state) = node.chain.tip_state();
            let row = state.seat_readiness(&bond, &id).expect("ordinary signed seat duty recorded");
            assert_eq!(row.proved_span, span);
            let issuer = node.api().get_ghostdag_data(tip).unwrap().selected_parent;
            for h in [issuer, tip] {
                blocks.push(RpcRawBlock::from(&node.api().get_block(h).unwrap()));
            }
        }
    }
    let (tip, state) = node.chain.tip_state();
    let read = node.api().palw_model_registry_v1().unwrap();
    let r = read.classes.iter().find(|r| r.class_id == id).unwrap();
    let seats: Vec<_> = node
        .chain
        .bonds
        .iter()
        .map(|bond| {
            let row = state.seat_readiness(bond, &id).unwrap();
            (row.proved_span, row.chunks)
        })
        .collect();
    peer_log!(
        "legacy genesis seats pid={} base={} class={} artifact={} tip={} state={} daa={} span_daa={} seats={:?} ready={} lifecycle={:?}",
        std::process::id(),
        base,
        id,
        root,
        tip,
        state.state_root(),
        node.chain.daa_of(tip),
        span_daa,
        seats,
        r.ready_seats_now,
        r.row.as_ref().unwrap().state
    );
    // The lifecycle fold consumes ready seats at a span boundary. Advance with
    // ordinary empty blocks so the last signed duty gets a full boundary, and
    // replay those same headers on each reader.
    let funded_at = tip;
    let next = (node.chain.daa_of(tip) / span_daa + 1) * span_daa;
    let write = std::env::var_os(format!("{archive_prefix}_WRITE"));
    if boundary_blocks.is_empty() {
        assert!(write.is_some(), "a seat-only archive needs a writer for the next span");
        let step = node.config.params.target_time_per_block();
        while node.chain.daa_of(node.chain.sink()) <= next {
            let beat = node.chain.heartbeat(step, vec![]).await;
            boundary_blocks.push(RpcRawBlock::from(&beat));
        }
    } else {
        for raw in &boundary_blocks {
            assert!(node.chain.daa_of(node.chain.sink()) <= next);
            replay(node, raw).await;
        }
    }
    let (tip, state) = node.chain.tip_state();
    let daa = node.chain.daa_of(tip);
    assert_eq!(daa, next + 1);
    let read = node.api().palw_model_registry_v1().unwrap();
    let r = read.classes.iter().find(|r| r.class_id == id).unwrap();
    let mut cards = Vec::new();
    for (card, bond) in node.chain.bonds.iter().enumerate() {
        let duty = read.readiness.iter().find(|v| v.bond == *bond && v.class_id == id).unwrap();
        let f = node.api().palw_producer_facts_v2(id, Some(bond.0)).unwrap();
        let k = TestConsensus::palw_v2_registry_keypair(card as u64);
        let refusal = f.ready_to_produce_v3(k.verification_key.as_ref(), node.bundle.state.rcore_plus_active_at(f.daa_score)).err();
        cards.push(serde_json::json!({"card":card,"bond":bond.0.to_string(),"fresh":duty.fresh,"seat_reason":duty.not_ready_reason,
           "span":duty.row.proved_span,"chunks":duty.row.chunks,"producer_refusal":refusal,"class_refusal":f.class_admission_refusal}));
    }
    let budget = state.bond_budget().unwrap();
    peer_log!(
        "legacy genesis boundary {} {}",
        std::process::id(),
        serde_json::json!({
           "base":base,"seat_daa":blocks.last().unwrap().header.daa_score,"class":id,"artifact":root,"tip":tip,"state":state.state_root(),"daa":daa,
           "boundary":next,"span_daa":span_daa,"heartbeats":boundary_blocks.len(),"ready":r.ready_seats_now,"lifecycle":format!("{:?}",r.row.as_ref().unwrap().state),
           "registry_reason":r.reason,"room":r.panel_room,"share":r.share_permille,"target":format!("{:032x}",r.class_target),"cards":cards,
           "available_microblocks":budget.model_blocks_available_v1(&id,daa),"claims":state.claims_iter().count()
        })
    );
    if let Some(path) = write {
        let mut out = std::io::BufWriter::new(File::create_new(path).unwrap());
        serde_json::to_writer(&mut out, &Extension { base, blocks, boundary_blocks }).unwrap();
        out.flush().unwrap();
    }
    funded_at
}
