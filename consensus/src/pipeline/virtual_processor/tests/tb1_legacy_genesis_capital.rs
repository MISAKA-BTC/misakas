//! Assign independent locked capital to the genesis 8K class on the same
//! selected audit branch, then wait for the first spendable native opportunity.
use super::*;
use kaspa_consensus_core::config::class_manifest_const_v1::{QWEN25_A16_8K_MANIFEST_V1, class_id_of_class};
use libcrux_ml_dsa::ml_dsa_87;

#[derive(Serialize, Deserialize)]
struct CapitalBranch {
    base: Hash64,
    assigned: Hash64,
    blocks: Vec<RpcRawBlock>,
}

pub(super) async fn run(node: &mut Rig, last_duty: Hash64, deadline: u64) -> Hash64 {
    let id = class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1);
    let (base, state) = node.chain.tip_state();
    let bond = node.chain.bonds[1];
    let previous = state.bond_budget().unwrap();
    assert_eq!(previous.model_blocks_available_v1(&id, node.chain.daa_of(base)), 0);
    assert!(previous.assignment_row(&bond).is_none_or(|r| r.sequence == 0 && r.effective.is_empty() && r.pending.is_none()));
    let amount = state.bond(&bond).unwrap().collateral / 2;
    let allocations = vec![(id, amount)];
    let pending = |node: &Rig| {
        let (tip, state) = node.chain.tip_state();
        let budget = state.bond_budget().unwrap();
        let r = budget.assignment_row(&bond).unwrap();
        assert_eq!(r.sequence, 1);
        assert_eq!(r.pending.as_ref(), Some(&allocations));
        assert!(r.effective.is_empty());
        assert_eq!(budget.model_blocks_available_v1(&id, node.chain.daa_of(tip)), 0);
    };
    let assigned;
    if let Some(path) = std::env::var_os("MISAKA_PRIVATE_GENESIS_CAPITAL_READ") {
        let archive: CapitalBranch = serde_json::from_reader(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
        assert_eq!(archive.base, base);
        assigned = archive.assigned;
        let mut saw_assignment = false;
        for raw in archive.blocks {
            genesis_seats::replay(node, &raw).await;
            if node.chain.sink() == assigned {
                pending(node);
                saw_assignment = true;
            }
        }
        assert!(saw_assignment);
    } else {
        let mut funds = genesis_seats::funding_change(node, last_duty);
        let message = palw_capital_assignment_message_v1(node.domain, &bond, &allocations, 1);
        let signature = ml_dsa_87::sign(
            &TestConsensus::palw_v2_registry_keypair(1).signing_key,
            message.as_byte_slice(),
            PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1,
            [0x79; 32],
        )
        .unwrap()
        .as_slice()
        .to_vec();
        send_lifecycle(
            &mut node.chain,
            &mut funds,
            PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments: allocations.clone(), sequence: 1, signature },
        )
        .await;
        assigned = node.chain.sink();
        pending(node);
        let issuer = node.api().get_ghostdag_data(assigned).unwrap().selected_parent;
        let mut blocks: Vec<_> =
            [issuer, assigned].into_iter().map(|h| RpcRawBlock::from(&node.api().get_block(h).unwrap())).collect();
        let step = node.config.params.target_time_per_block();
        loop {
            let b = node.chain.heartbeat(step, vec![]).await;
            let daa = b.header.daa_score;
            blocks.push(RpcRawBlock::from(&b));
            let state = node.chain.tip_state().1;
            if state.bond_budget().unwrap().model_blocks_available_v1(&id, daa) > 0 {
                break;
            }
            assert!(daa < deadline, "original audit deadline reached while seasoning capital");
        }
        let path = std::env::var_os("MISAKA_PRIVATE_GENESIS_CAPITAL_WRITE").unwrap();
        let mut writer = std::io::BufWriter::new(File::create_new(path).unwrap());
        serde_json::to_writer(&mut writer, &CapitalBranch { base, assigned, blocks }).unwrap();
        writer.flush().unwrap();
    }
    let (tip, state) = node.chain.tip_state();
    let budget = state.bond_budget().unwrap();
    let row = budget.assignment_row(&bond).unwrap();
    assert_eq!(row.effective, allocations);
    assert!(row.pending.is_none());
    let daa = node.chain.daa_of(tip);
    assert!(daa < deadline);
    let available = budget.model_blocks_available_v1(&id, daa);
    assert!(available > 0);
    let registry = node.api().palw_model_registry_v1().unwrap();
    let class = registry.classes.iter().find(|r| r.class_id == id).unwrap();
    peer_log!(
        "legacy genesis capital {} {}",
        std::process::id(),
        serde_json::json!({
            "base":base,"assigned":assigned,"assignment_daa":node.chain.daa_of(assigned),"class":id,"bond":bond.0.to_string(),
            "collateral":state.bond(&bond).unwrap().collateral,"assigned_sompi":amount,"epoch":budget.header.allocation.unwrap().index,
            "available_microblocks":available,"target":format!("{:032x}",class.class_target),"ready":class.ready_seats_now,
            "lifecycle":format!("{:?}",class.row.as_ref().unwrap().state),"claims":state.claims_iter().count(),
            "tip":tip,"state":state.state_root(),"daa":daa,"deadline":deadline
        })
    );
    assigned
}
