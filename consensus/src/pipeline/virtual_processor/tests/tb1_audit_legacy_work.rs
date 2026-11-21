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
    #[serde(default)]
    base: Option<Hash64>,
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

/// Replay the archived heartbeat bodies up to the certified claim's own
/// retirement. Inspect the two adjacent native states even when their DAA
/// scores repeat. The rule, due time and work identity come from the node.
async fn replay_work_retirement(
    node: &mut Rig,
    branch: &Path,
    audit_close: Hash64,
    deadline: u64,
    cut: &PreparedAudit<'_>,
    q: &Tb1Qualification<'_>,
) {
    use kaspa_consensus_core::{palw_native_settlement_v1::native_delta_evidence_v1, palw_state_v2::PalwDeltaEntryV2};
    use std::io::{BufRead, BufReader};

    let (base, initial) = node.chain.tip_state();
    let (_, delta) = node.vp().palw_state_v2_store.read().delta_of(base).unwrap();
    let cert = native_delta_evidence_v1(&delta);
    let &[id] = cert.certified_claims.as_slice() else { panic!("expected the original Gate C block") };
    let claim = initial.claim(&id).unwrap();
    let due = initial.deadline_of(&id).unwrap();
    let work = claim.work_id.expect("certified native work identity");
    let source_key = [id.as_byte_slice().as_slice(), work.as_byte_slice().as_slice()].concat();
    let snapshot = |node: &Rig| {
        let (tip, state) = node.chain.tip_state();
        let (_, delta) = node.vp().palw_state_v2_store.read().delta_of(tip).unwrap();
        let deleted = delta.entries.iter().any(
            |e| matches!(e, PalwDeltaEntryV2::Claim { key, old: Some(old), new: None } if *key == id && old.work_id == claim.work_id),
        );
        let route = state.kernel_route_state_v1().unwrap();
        let source = route.aux.get(&(kaspa_consensus_core::palw_state_v2::LEGACY_PENDING_FACT_TABLE_V1, source_key.clone()));
        let closed: Option<kaspa_consensus_core::palw_native_closed_facts_v1::NativeClosedFactRowV1> =
            route.aux_row(kaspa_consensus_core::palw_native_closed_facts_v1::NATIVE_CLOSED_FACTS_TABLE_V1, work.as_byte_slice());
        if let Some(row) = &closed {
            row.validate_v1(work).unwrap();
            assert_eq!(row.origin.subject, id);
        }
        let ready = readiness_of(node);
        let blocking = ready.blocking.unwrap();
        serde_json::json!({"tip":tip,"daa":node.chain.daa_of(tip),"state":state.state_root(),"claim_live":state.claim(&id).is_some(),
            "bond_live":state.bond(&claim.bond).is_some(),"due":state.deadline_of(&id),"deleted":deleted,
            "pending_source":source.is_some(),"closed_maturity":closed.map(|r|r.fact.matured_daa),"earliest_in":blocking.earliest_ready_in_daa,
            "blocking":blocking.block,"evidence":blocking.evidence,"safe":ready.safe,"finalized":ready.finalized.finalized})
    };
    let start = snapshot(node);
    let maturity = node.chain.daa_of(base).checked_add(start["earliest_in"].as_u64().unwrap()).unwrap();
    let mut n = 0usize;
    let mut sampled = false;
    for line in BufReader::new(File::open(branch).unwrap()).lines() {
        let raw: RpcRawBlock = serde_json::from_str(&line.unwrap()).unwrap();
        let block_daa = raw.header.daa_score;
        assert!(block_daa <= deadline);
        // A native deadline remains live throughout its own DAA score.
        let before = (!sampled && block_daa > due).then(|| snapshot(node));
        genesis_seats::replay(node, &raw).await;
        n += 1;
        if let Some(before) = before {
            let after = snapshot(node);
            if after["deleted"] != true {
                continue;
            }
            peer_log!(
                "signed work retirement {} {}",
                std::process::id(),
                serde_json::json!({"close":audit_close,"base":base,"deadline":deadline,"claim":id,"work":work,
                "trace_retention_daa":claim.trace_retention_daa,"local_at_acceptance":node.bundle.state.claim_local_settlement_active_at(claim.accepted_daa),
                "local_at_observation":node.bundle.state.claim_local_settlement_active_at(block_daa),
                "scheduled":due,"blocks":n,"start":start,"before":before,"after":after})
            );
            sampled = true;
        }
    }
    assert_eq!(node.chain.daa_of(node.chain.sink()), deadline);
    let check_expired = |node: &Rig| {
        assert_eq!(
            cut.observe(node.api().as_ref(), audit_close, q).err().expect("original release deadline").kind(),
            std::io::ErrorKind::TimedOut
        );
    };
    check_expired(node);
    let expired = snapshot(node);
    let write = std::env::var_os("MISAKA_PRIVATE_AUDIT_WORK_MATURITY_WRITE");
    let read = std::env::var_os("MISAKA_PRIVATE_AUDIT_WORK_MATURITY_READ");
    assert!(write.is_none() || read.is_none());
    if write.is_none() && read.is_none() {
        return;
    }
    let mut output = write.map(|path| std::io::BufWriter::new(File::create_new(path).unwrap()));
    let mut input = read.map(|path| BufReader::new(File::open(path).unwrap()).lines());
    let step = node.config.params.target_time_per_block();
    let mut count = 0;
    loop {
        let parent = node.chain.sink();
        let witness = node.chain.daa_of(parent) >= maturity;
        let observation = witness.then(|| {
            check_expired(node);
            (snapshot(node), node.chain.tip_state().1.state_root())
        });
        let tip = if let Some(input) = &mut input {
            let raw: RpcRawBlock = serde_json::from_str(&input.next().expect("incomplete maturity history").unwrap()).unwrap();
            genesis_seats::replay(node, &raw).await;
            node.chain.sink()
        } else {
            let b = node.chain.heartbeat(step, vec![]).await;
            let archive = output.as_mut().unwrap();
            serde_json::to_writer(&mut *archive, &RpcRawBlock::from(&b)).unwrap();
            archive.write_all(b"\n").unwrap();
            b.hash()
        };
        assert_eq!(node.api().get_ghostdag_data(tip).unwrap().selected_parent, parent);
        if let Some((observed, parent_root)) = observation {
            assert_eq!(node.api().get_header(tip).unwrap().palw_state_root, parent_root);
            if let Some(mut writer) = output {
                writer.flush().unwrap();
                writer.get_ref().sync_all().unwrap();
            }
            if let Some(mut lines) = input {
                assert!(lines.next().is_none(), "trailing maturity history");
            }
            peer_log!(
                "signed work maturity {} {}",
                std::process::id(),
                serde_json::json!({"close":audit_close,"base":base,"claim":id,
                "work":work,"scheduled":due,"start":start,"maturity":maturity,"expired":expired,"blocks":count,"observed":observed,"witness":tip})
            );
            break;
        }
        count += 1;
    }
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
) -> Rig {
    let path = std::env::var_os("MISAKA_PRIVATE_TB1_TIR_ARTIFACT").expect("held legacy Studio model");
    let held = TirArtifactV1::open(Path::new(&path)).unwrap();
    let class = held.class().unwrap();
    let inventory = held.inventory_root().unwrap().0;
    let id = class.class_id(&inventory);
    assert_ne!(id, qwen);
    assert_ne!(id, node.bundle.base_class_id);
    assert!(node.chain.tip_state().1.class(&id).is_none());
    let base = node.chain.sink();
    let original = node.chain.tip_state().1.state_root();
    let mut funding = parts.3[0].clone();
    let (registration, assignment, seasoned);
    if let Some(path) = std::env::var_os("MISAKA_PRIVATE_AUDIT_WORK_READ") {
        let b: Branch = serde_json::from_reader(std::io::BufReader::new(File::open(path).unwrap())).unwrap();
        assert_eq!(b.close, close);
        assert_eq!(b.base.unwrap_or(close), base);
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
            let i = blocks.iter().position(|b| b.hash() == base).unwrap();
            let branch = Branch {
                base: Some(base),
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
                    if let Some(input) = std::env::var_os("MISAKA_PRIVATE_AUDIT_WORK_RETIREMENT_READ") {
                        replay_work_retirement(&mut node, Path::new(&input), close, deadline, cut, qualification).await;
                    } else if let Some(out) = std::env::var_os("MISAKA_PRIVATE_AUDIT_FOLLOW_WINDOW") {
                        follow_release_window(&mut node, cut, close, qualification, deadline, Path::new(&out)).await;
                    }
                }
            }
        }
    }
    node
}
