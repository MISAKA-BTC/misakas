//! Fork the original closed MODEL_AUDIT history before its original release
//! deadline. Register and capitalize an independent held legacy TIR class.
use super::super::super::super::{g14_registration_e2e::register_held_tir_on_branch, t12_round_lane_e2e::send_lifecycle};
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::{
    palw_bond_budget_v1::{PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1, palw_capital_assignment_message_v1},
    palw_producer_v2::PALW_NOT_READY_CLASS_NOT_ADMITTING_V2,
    palw_state_v2::PalwConsensusObjectV2,
};
use misaka_palw_tir_exec::node::TirArtifactV1;

#[path = "tb1_legacy_genesis_capital.rs"]
mod genesis_capital;
#[path = "tb1_legacy_genesis_claim.rs"]
mod genesis_claim;
#[path = "tb1_legacy_genesis_seats.rs"]
mod genesis_seats;

#[derive(Serialize, Deserialize)]
struct Branch {
    close: Hash64,
    registration: Hash64,
    assignment: Hash64,
    seasoned: Hash64,
    tip: Hash64,
    blocks: Vec<RpcRawBlock>,
}

fn observe_capital(node: &Rig, class: Hash64, what: &str) -> serde_json::Value {
    let (tip, state) = node.chain.tip_state();
    let budget = state.bond_budget().unwrap();
    let bond = node.chain.bonds[0];
    let row = budget.assignment_row(&bond);
    let epoch = budget.header.allocation.unwrap();
    let facts = node.api().palw_producer_facts_v2(class, Some(bond.0)).unwrap();
    let key = TestConsensus::palw_v2_registry_keypair(0);
    let refusal =
        facts.ready_to_produce_v3(key.verification_key.as_ref(), node.bundle.state.rcore_plus_active_at(facts.daa_score)).err();
    if what == "seasoned" {
        let mut models = 0;
        for (model, class_row) in state.classes_iter().filter(|(model, _)| **model != node.bundle.base_class_id) {
            models += 1;
            let lifecycle = &state.model_lifecycle(model).unwrap().state;
            use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1::{Candidate, Prefetching};
            let expected = if class_row.registered_daa == 0 { Prefetching } else { Candidate };
            assert_eq!(*lifecycle, expected);
            assert_eq!(node.chain.bonds.len(), 8);
            let mut ready = Vec::new();
            for (card, b) in node.chain.bonds.iter().enumerate() {
                let f = node.api().palw_producer_facts_v2(*model, Some(b.0)).unwrap();
                let k = TestConsensus::palw_v2_registry_keypair(card as u64);
                let verdict =
                    f.ready_to_produce_v3(k.verification_key.as_ref(), node.bundle.state.rcore_plus_active_at(f.daa_score)).err();
                assert_eq!(verdict, Some(PALW_NOT_READY_CLASS_NOT_ADMITTING_V2));
                assert_eq!(
                    f.class_admission_refusal,
                    Some(format!("class {model} is {expected:?} under the model registry and admits no new claims"))
                );
                ready.push(serde_json::json!({"card":card,"verdict":verdict,"class_reason":f.class_admission_refusal}));
            }
            peer_log!(
                "legacy-work inventory {} {}",
                std::process::id(),
                serde_json::json!({"model":model,"root":class_row.artifact_root,
                "tip":tip,"share":state.class_share_permille(model),"registered_daa":class_row.registered_daa,"cards":ready})
            );
        }
        assert_eq!(models, 4);
        let amount = state.bond(&bond).unwrap().collateral / 2;
        let row = row.unwrap();
        assert!(row.pending.is_none());
        assert!(row.effective.contains(&(class, amount)));
        assert!(budget.model_blocks_available_v1(&class, facts.daa_score) >= 1_000_000);
        assert!(!facts.fp_certified);
        assert!(matches!(state.class(&class).unwrap().status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Active));
    }
    let observation = serde_json::json!({
        "stage":what, "tip":tip, "root":state.state_root(), "daa":node.chain.daa_of(node.chain.sink()), "class":class, "artifact":facts.artifact_root,
        "bond":bond.0.to_string(), "collateral":state.bond(&bond).unwrap().collateral, "epoch":epoch.index,
        "pending":row.and_then(|r| r.pending.as_ref()).and_then(|r| r.iter().find(|(m,_)| *m==class)).map(|a| a.1),
        "effective":row.and_then(|r| r.effective.iter().find(|(m,_)| *m==class)).map(|a| a.1),
        "capital":budget.model_row(&class).map(|r| r.capital.to_string()),
        "opportunity":budget.model_blocks_available_v1(&class,facts.daa_score),
        "status":format!("{:?}",state.class(&class).unwrap().status), "legacy_epoch_budget":facts.epoch_budget_blocks,
        "legacy_budget_enforced":facts.epoch_budget_read, "fp_certified":facts.fp_certified,
        "target":format!("{:032x}",facts.class_target), "refusal":refusal,"class_refusal":facts.class_admission_refusal
    });
    peer_log!("legacy-work {} {}", std::process::id(), observation);
    observation
}

/// Continue the original signed audit on its certified branch, without
/// replacing the candidate, pruning point, clock policy or native work facts.
async fn follow_release_window(
    node: &mut Rig,
    cut: &PreparedAudit<'_>,
    close: Hash64,
    q: &Tb1Qualification<'_>,
    deadline: u64,
    path: &Path,
) {
    let base = node.chain.sink();
    let step = node.config.params.target_time_per_block();
    let mut archive = std::io::BufWriter::new(File::create_new(path).unwrap());
    let mut blocks = 0u64;
    let mut last_pruning = None;
    loop {
        let tip = node.chain.sink();
        let h = node.api().get_header(tip).unwrap();
        let pruning = node.chain.ctx.consensus.pruning_point();
        let covers = node.api().is_chain_ancestor_of(close, pruning).unwrap();
        // Native finality is bounded by the node's validated pruning point.
        // Query the whole decision whenever release is possible, at either end,
        // or when either the pruning point or its coverage changes.
        if covers || last_pruning != Some(pruning) || h.daa_score >= deadline - 1 {
            let gate = cut.observe(node.api().as_ref(), close, q);
            let status = match gate {
                Ok(_) => {
                    assert!(covers && h.daa_score < deadline);
                    "ready"
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && h.daa_score < deadline => "held",
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut && h.daa_score >= deadline => "expired",
                Err(e) => panic!("cannot resolve the signed audit on its original branch: {e}"),
            };
            if blocks == 0 || last_pruning != Some(pruning) || status != "held" || h.daa_score == deadline - 1 {
                peer_log!(
                    "signed audit release window {} {}",
                    std::process::id(),
                    serde_json::json!({
                        "base":base,"close":close,"tip":tip,"blocks":blocks,"daa":h.daa_score,"blue":h.blue_score,"deadline":deadline,
                        "pruning":pruning,"header_pruning":h.pruning_point,"pruning_depth":node.config.params.pruning_depth(),
                        "finality_depth":node.config.params.finality_depth(),"covers_close":covers,"publication":status,"readiness":readiness_of(node)
                    })
                );
            }
            if status != "held" {
                break;
            }
        }
        last_pruning = Some(pruning);
        let b = node.chain.heartbeat(step, vec![]).await;
        assert_eq!(node.api().get_ghostdag_data(b.hash()).unwrap().selected_parent, tip);
        serde_json::to_writer(&mut archive, &RpcRawBlock::from(&b)).unwrap();
        archive.write_all(b"\n").unwrap();
        blocks += 1;
    }
    archive.flush().unwrap();
    archive.get_ref().sync_all().unwrap();
}

pub(super) async fn exercise(
    mut node: Rig,
    parts: Parts,
    close: Hash64,
    qwen: Hash64,
    cut: &PreparedAudit<'_>,
    qualification: &Tb1Qualification<'_>,
    deadline: u64,
) {
    let path = std::env::var_os("MISAKA_PRIVATE_TB1_TIR_ARTIFACT").expect("held legacy Studio model");
    let held = TirArtifactV1::open(Path::new(&path)).unwrap();
    let class = held.class().unwrap();
    let inventory = held.inventory_root().unwrap().0;
    let id = class.class_id(&inventory);
    assert_ne!(id, qwen);
    assert_ne!(id, node.bundle.base_class_id);
    assert!(node.chain.tip_state().1.class(&id).is_none());
    let original = node.chain.tip_state().1.state_root();
    let mut funding = parts.3[0].clone();
    let (registration, assignment, seasoned);
    if let Some(path) = std::env::var_os("MISAKA_PRIVATE_AUDIT_WORK_READ") {
        let b: Branch = serde_json::from_reader(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
        assert_eq!(b.close, close);
        (registration, assignment, seasoned) = (b.registration, b.assignment, b.seasoned);
        let mut visited = 0;
        for raw in b.blocks {
            let block = Block::try_from(raw).unwrap();
            let parent = node.chain.sink();
            node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(block.header.timestamp);
            stable_panel::arm(&mut node);
            let hash = block.hash();
            let status = node.api().validate_and_insert_block(block).virtual_state_task.await.unwrap();
            assert!(status.has_block_body());
            assert_eq!(node.chain.sink(), hash);
            assert_eq!(node.api().get_ghostdag_data(hash).unwrap().selected_parent, parent);
            for (at, name) in [(registration, "registered"), (assignment, "assigned"), (seasoned, "seasoned")] {
                if hash == at {
                    observe_capital(&node, id, name);
                    visited += 1;
                }
            }
        }
        assert_eq!(visited, 3);
        assert_eq!(node.chain.sink(), b.tip);
    } else {
        node.chain = register_held_tir_on_branch(node.chain, parts.clone(), class.clone(), inventory, 3).await;
        registration = node.chain.sink();
        assert_eq!(observe_capital(&node, id, "registered")["effective"], serde_json::Value::Null);
        let (_, state) = node.chain.tip_state();
        let bond = node.chain.bonds[0];
        let amount = state.bond(&bond).unwrap().collateral / 2;
        let assignments = vec![(id, amount)];
        let message = palw_capital_assignment_message_v1(node.domain, &bond, &assignments, 1);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &TestConsensus::palw_v2_registry_keypair(0).signing_key,
            message.as_byte_slice(),
            PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1,
            [0x71; 32],
        )
        .unwrap()
        .as_ref()
        .to_vec();
        send_lifecycle(
            &mut node.chain,
            &mut funding,
            PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments, sequence: 1, signature },
        )
        .await;
        assignment = node.chain.sink();
        assert_eq!(observe_capital(&node, id, "assigned")["pending"], amount);
        let step = node.config.params.target_time_per_block();
        let policy = node.bundle.state.rfc0018_v1().unwrap().certification;
        let mature = state.bonds_iter().map(|(_, b)| b.registered_daa).max().unwrap() + policy.verifier_maturity_daa;
        while node.chain.daa_of(node.chain.sink()) < mature {
            node.chain.heartbeat(step, vec![]).await;
        }
        seasoned = node.chain.sink();
        let row = observe_capital(&node, id, "seasoned");
        assert_eq!(row["effective"], amount);
        assert!(row["opportunity"].as_u64().unwrap() > 0);
        assert!(node.chain.daa_of(node.chain.sink()) + policy.prosecution_window_daa < deadline);
        if let Some(out) = std::env::var_os("MISAKA_PRIVATE_AUDIT_WORK_WRITE") {
            let tip = node.chain.sink();
            let blocks = node.blocks_through(tip);
            let i = blocks.iter().position(|b| b.hash() == close).unwrap();
            let branch = Branch {
                close,
                registration,
                assignment,
                seasoned,
                tip,
                blocks: blocks[i + 1..].iter().map(RpcRawBlock::from).collect(),
            };
            let mut writer = std::io::BufWriter::new(File::create_new(out).unwrap());
            serde_json::to_writer(&mut writer, &branch).unwrap();
            writer.flush().unwrap();
        }
    }
    let (tip, state) = node.chain.tip_state();
    assert!(node.api().is_chain_ancestor_of(close, registration).unwrap());
    assert!(node.api().is_chain_ancestor_of(registration, assignment).unwrap());
    assert!(node.api().is_chain_ancestor_of(assignment, seasoned).unwrap());
    assert_eq!(seasoned, tip);
    let claims = state.claims_iter().count();
    assert_eq!(claims, 0);
    let err = cut.observe(node.api().as_ref(), close, qualification).err().expect("audit shares remain held");
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
    let readiness = readiness_of(&node);
    assert_eq!(readiness.stop, Some(kaspa_consensus_core::palw_native_settlement_v1::SettlementStopV1::FrontierNotCovered));
    assert!(readiness.safe.is_none());
    assert!(readiness.finalized.finalized.is_none());
    peer_log!(
        "legacy-work result {} {}",
        std::process::id(),
        serde_json::json!({"close":close,"close_state":original,"deadline":deadline,
        "registration":registration,"assignment":assignment,"seasoned":seasoned,"claims":claims,"tip":tip,"state":state.state_root(),
        "daa":node.chain.daa_of(node.chain.sink()),"publication_refusal":format!("{:?}",err.kind()),"readiness":readiness})
    );
    if let Some(path) = std::env::var_os("MISAKA_PRIVATE_TB1_GENESIS8K") {
        let path = Path::new(&path);
        let last_duty = genesis_seats::run(&mut node, assignment, path, "MISAKA_PRIVATE_GENESIS_SEATS").await;
        if std::env::var_os("MISAKA_PRIVATE_GENESIS_CAPITAL_READ").is_some()
            || std::env::var_os("MISAKA_PRIVATE_GENESIS_CAPITAL_WRITE").is_some()
        {
            let assigned = genesis_capital::run(&mut node, last_duty, deadline).await;
            if std::env::var_os("MISAKA_PRIVATE_GENESIS_RENEW_READ").is_some()
                || std::env::var_os("MISAKA_PRIVATE_GENESIS_RENEW_WRITE").is_some()
            {
                let renewed = genesis_seats::run(&mut node, assigned, path, "MISAKA_PRIVATE_GENESIS_RENEW").await;
                let attest = std::env::var_os("MISAKA_PRIVATE_GENESIS_CERTIFY_WRITE").is_some()
                    || std::env::var_os("MISAKA_PRIVATE_GENESIS_CERTIFY_READ").is_some();
                if let Some(location) = std::env::var_os("MISAKA_PRIVATE_GENESIS_ATTEMPT_WRITE") {
                    genesis_claim::run(&mut node, path, Path::new(&location), true, renewed).await;
                } else if let Some(location) = std::env::var_os("MISAKA_PRIVATE_GENESIS_ATTEMPT_READ") {
                    genesis_claim::run(&mut node, path, Path::new(&location), false, renewed).await;
                } else {
                    assert!(!attest, "certification requires an accepted execution");
                }
                if attest {
                    let tip = node.chain.sink();
                    let now = node.chain.daa_of(tip);
                    assert!(now < deadline);
                    let gate = cut.observe(node.api().as_ref(), close, qualification);
                    let publication = match gate {
                        Ok(_) => "ready",
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => "held",
                        Err(e) => panic!("invalid original audit after executing work: {e}"),
                    };
                    peer_log!(
                        "signed audit after nonfloor {} {}",
                        std::process::id(),
                        serde_json::json!({"close":close,"deadline":deadline,"tip":tip,"daa":now,
                            "state":node.chain.tip_state().1.state_root(),"publication":publication,"readiness":readiness_of(&node)})
                    );
                    if let Some(out) = std::env::var_os("MISAKA_PRIVATE_AUDIT_FOLLOW_WINDOW") {
                        follow_release_window(&mut node, cut, close, qualification, deadline, Path::new(&out)).await;
                    }
                }
            }
        }
    }
}
