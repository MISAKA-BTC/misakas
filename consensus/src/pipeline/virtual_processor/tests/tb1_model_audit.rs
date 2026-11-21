//! Registered, non-paying evaluation with independent test dealer material.
//! Use the native chain after the registered EXEC close without injecting work.
use super::*;
#[path = "tb1_audit_history.rs"]
mod history;
#[path = "tb1_audit_legacy_work.rs"]
mod legacy_work;
use misaka_palw_remote::tb1_exposure::model_audit::{AuditCandidate, AuditPlan, PreparedAudit};
use misaka_palw_tir_exec::node::qf1::Identity;

pub(super) async fn registered_cut(
    node: &mut Rig,
    parent: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    dir: &Path,
    shared: Option<&Path>,
    slot: usize,
    roster: Tb1Roster<'_>,
    keys: &[ml_dsa_65::MLDSA65KeyPair],
    model: Identity,
    exec: Tb1MiningRound,
    exec_q: &Tb1Qualification<'_>,
) {
    let start = node.api().get_header(node.chain.sink()).unwrap();
    let audit = Tb1MiningRound {
        purpose: Tb1MiningPurpose::ModelAudit,
        logical_round: 4,
        close_blue_score: start.blue_score + 2,
        release_from_daa: start.daa_score + 4,
        deadline_exclusive_daa: start.daa_score + 16,
        ..exec
    };
    let panel = Tb1MiningRound {
        purpose: Tb1MiningPurpose::Panel,
        logical_round: 5,
        close_blue_score: start.blue_score + 40,
        release_from_daa: start.daa_score + 26,
        deadline_exclusive_daa: start.daa_score + 48,
        ..audit
    };
    let ids = [audit.id(), panel.id()];
    let dealer_dirs = ["model-audit-dealers", "evaluation-panel-dealers"].map(|name| shared.unwrap_or(dir).join(name));
    if parent.is_none() {
        for path in &dealer_dirs {
            std::fs::create_dir(path).unwrap();
        }
    }
    let mut setups = Vec::new();
    for (i, path) in dealer_dirs.iter().enumerate() {
        if let Some(s) = parent {
            assert_eq!(receive::<usize>(s), i);
            setups.push(stable_panel::provision(path, Some(path), slot, &ids[i]));
        } else {
            setups.push(stable_panel::provision(path, None, slot, &ids[i]));
            for (_, s) in peers.iter_mut() {
                send(s, &i);
            }
        }
    }
    let dealers = [0, 1, 2];
    let q = qualified(&ids[0], roster, keys, &dealers, 38, Some(&setups[0].roots));
    let panel_q = qualified(&ids[1], roster, keys, &dealers, 39, Some(&setups[1].roots));
    let qualify = PreparedAudit::preparation_marker;
    assert!(qualify(&exec, exec_q, &panel, &panel_q).is_err());
    assert!(qualify(&audit, exec_q, &panel, &panel_q).is_err());
    assert!(qualify(&audit, &q, &panel, &q).is_err());
    assert!(QualifiedExecPanel::new(audit, &q, panel, &panel_q).is_err());
    let marker = qualify(&audit, &q, &panel, &panel_q).unwrap();
    let prep = local_carrier(node, parent, peers, 0, marker, "MODEL_AUDIT preparation").await;
    let candidates = [AuditCandidate { class: model.class, artifact: model.inventory, descriptor: model.descriptor, generation: 1 }];
    // An isolated suite and capacity reference exercise binding only.
    let plan = AuditPlan {
        suite: Hash64::from_u64_word(0xa501),
        candidate_budget: 1,
        tests_per_candidate: 2,
        reservation: Hash64::from_u64_word(0xa502),
        case_deadline_exclusive_daa: start.daa_score + 54,
    };
    let read = |p, c: &[AuditCandidate]| {
        PreparedAudit::read(node.api().as_ref(), prep.hash(), audit, &q, &panel, &panel_q, p, c)
            .map(|cut| cut.close_commitment(prep.hash()))
    };
    assert!(read(plan, &[]).is_err());
    assert!(read(AuditPlan { candidate_budget: 2, ..plan }, &[candidates[0]; 2]).is_err());
    assert!(read(plan, &[AuditCandidate { artifact: Hash64::default(), ..candidates[0] }]).is_err());
    assert!(read(AuditPlan { tests_per_candidate: 0, ..plan }, &candidates).is_err());
    let marker = read(plan, &candidates).unwrap();
    let changed_plans = [
        AuditPlan { tests_per_candidate: 3, ..plan },
        AuditPlan { suite: Hash64::from_u64_word(0xbeef), ..plan },
        AuditPlan { reservation: Hash64::from_u64_word(0xcafe), ..plan },
        AuditPlan { case_deadline_exclusive_daa: plan.case_deadline_exclusive_daa + 1, ..plan },
    ];
    for changed in changed_plans {
        assert_ne!(read(changed, &candidates).unwrap(), marker);
    }
    let close = local_carrier(node, parent, peers, 0, marker, "MODEL_AUDIT candidate close").await;
    while node.chain.daa_of(node.chain.sink()) < audit.release_from_daa {
        assert!(node.api().get_header(node.chain.sink()).unwrap().blue_score < start.blue_score + 120);
        local_carrier(node, parent, peers, 0, [0; 64], "MODEL_AUDIT release window").await;
    }
    let api = node.api().as_ref();
    let observe = |p, c: &[AuditCandidate]| {
        PreparedAudit::read(api, prep.hash(), audit, &q, &panel, &panel_q, p, c)
            .and_then(|cut| cut.observe(api, close.hash(), &q))
            .err()
            .expect("audit must remain closed")
            .kind()
    };
    for changed in changed_plans {
        assert_eq!(observe(changed, &candidates), std::io::ErrorKind::InvalidData);
    }
    assert_eq!(observe(plan, &[AuditCandidate { generation: 2, ..candidates[0] }]), std::io::ErrorKind::InvalidData);
    assert_eq!(observe(plan, &[AuditCandidate { descriptor: model.class, ..candidates[0] }]), std::io::ErrorKind::InvalidData);
    assert_eq!(observe(plan, &candidates), std::io::ErrorKind::WouldBlock);
    let daa = node.chain.daa_of(node.chain.sink());
    assert!((audit.release_from_daa..audit.deadline_exclusive_daa).contains(&daa));
    let cut = PreparedAudit::read(api, prep.hash(), audit, &q, &panel, &panel_q, plan, &candidates).unwrap();
    let freeze = cut.holder_freeze(api, close.hash(), &q).unwrap();
    let key = &keys[slot].verification_key;
    let shares = setups[0].holder_shares();
    let row = Tb1HolderRelease {
        context: Tb1ReleaseContext { round_ref: &ids[0], qual_root: q.root(), freeze_id: marker },
        holder_slot: slot,
        shares: &shares,
    };
    Tb1ExposureJournal::provision(dir, &ids[0], key, slot as u32).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir, &ids[0], key, slot as u32).unwrap();
    assert_eq!(journal.reserve_signing_message(api, freeze, &q, &row, 2048).unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
    peer_log!(
        "MAS1 audit plan pid={} audit={} panel={} generation={} suite={} reservation={} candidates={} tests={} case_deadline={}",
        std::process::id(),
        Hash64::from_bytes(ids[0]),
        Hash64::from_bytes(ids[1]),
        candidates[0].generation,
        plan.suite,
        plan.reservation,
        plan.candidate_budget,
        plan.tests_per_candidate,
        plan.case_deadline_exclusive_daa
    );
    let receipt =
        (prep.hash(), close.hash(), Hash64::from_bytes(marker), Hash64::from_bytes(q.root()), Hash64::from_bytes(panel_q.root()), daa);
    if let Some(s) = parent {
        send(s, &receipt);
    } else {
        for (_, s) in peers.iter_mut() {
            assert_eq!(receive::<(Hash64, Hash64, Hash64, Hash64, Hash64, u64)>(s), receipt);
        }
    }
    peer_log!(
        "MAS1 registered audit pid={} slot={slot} class={} descriptor={} preparation={} close={} freeze={} audit_qual={} evaluation_panel_qual={} daa={daa} from={} deadline={} publication=WouldBlock signing=WouldBlock",
        std::process::id(),
        model.class,
        model.descriptor,
        receipt.0,
        receipt.1,
        receipt.2,
        receipt.3,
        receipt.4,
        audit.release_from_daa,
        audit.deadline_exclusive_daa
    );
    // A fresh governed attempt at or after this close cannot certify earlier
    // than its Final plus the configured prosecution window. Even a Final at
    // the close itself would miss these pre-committed test deadlines.
    let window = node.chain.bundle.state.rfc0018_v1().unwrap().certification.prosecution_window_daa;
    let lower_bound = close.header.daa_score.checked_add(window).unwrap();
    assert!(lower_bound >= audit.deadline_exclusive_daa);
    let through = exec.deadline_exclusive_daa.max(audit.deadline_exclusive_daa);
    assert!(through > daa);
    let mut observed_blocks = 0;
    while node.chain.daa_of(node.chain.sink()) < through {
        assert!(observed_blocks < 200);
        local_carrier(node, parent, peers, 0, [0; 64], "original disclosure deadlines").await;
        assert!(snapshot_of(node).unwrap().finalized.is_none());
        observed_blocks += 1;
    }
    let at = node.chain.sink();
    let api = node.api().as_ref();
    assert!(journal.reserve_signing_message(api, freeze, &q, &row, 2048).is_err());
    peer_log!(
        "MAS1 audit expiry pid={} tip={} daa={} blocks={} close_daa={} prosecution_daa={} earliest_attempt_cert_daa={} finalized=none",
        std::process::id(),
        at,
        node.chain.daa_of(at),
        observed_blocks,
        close.header.daa_score,
        window,
        lower_bound
    );
}

/// A different round, scheduled before disclosure with enough time for the
/// unmodified Gate C prosecution and verifier-maturity windows. Only ordinary
/// blocks advance its frontier, including a genuinely replayed BASE-0 claim.
#[tokio::test]
#[ignore = "requires the full QF1 asset and ordinary verifier maturity and certification"]
async fn private_registered_audit_certified_frontier() {
    registered_audit_history(false).await;
}

#[tokio::test]
#[ignore = "requires the signed audit archive and both registered model artifacts"]
async fn private_registered_audit_legacy_work() {
    registered_audit_history(true).await;
}

async fn registered_audit_history(legacy: bool) {
    use super::super::super::{
        g14_registration_e2e::{private_moe2::arm_private_registration, tb1_registration_prefix},
        t12_round_lane_e2e::certified_floor_history,
    };
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let asset = std::env::var_os("MISAKA_PRIVATE_QF1_FULL").expect("full QF1 model");
    let held = misaka_palw_tir_exec::node::TirArtifactV1::open(Path::new(&asset)).unwrap();
    let model = misaka_palw_sdk::qf1_local::bind(&held).unwrap();
    let identity = model.identity();
    let (config, _, premine, floats) = parts_custom(Some(FENCE), false, |p| arm_private_registration(p, true));
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let parts = (config.clone(), bundle.clone(), premine, floats.clone());
    let mut node = Rig::new(&parts, 0x12_c500_0000);
    stable_panel::arm(&mut node);
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let archive = std::env::var_os(history::OUTPUT).map(std::path::PathBuf::from).unwrap_or_else(|| dir.path().join("blocks.jsonl"));
    let mut reader = history::Reader::open(node.chain.sink());
    if let Some(r) = &mut reader {
        while node.chain.sink() != r.header.registered {
            r.next(&mut node).await;
        }
    } else {
        for block in tb1_registration_prefix(&parts, held.class().unwrap(), identity.inventory).await {
            stable_panel::arrive(&mut node, block, "registered prior to long audit").await;
        }
    }
    let registered = node.chain.sink();
    let registered_state = node.chain.tip_state().1;
    assert_eq!(registered_state.class(&identity.class).unwrap().artifact_root, identity.inventory);
    assert!(registered_state.model_market(&identity.class).is_some());
    let before = node.api().get_header(registered).unwrap();
    let policy = node.chain.bundle.state.rfc0018_v1().unwrap().certification;
    let keys: Vec<_> = (1..=4).map(|i| ml_dsa_65::generate_key_pair([i; 32])).collect();
    let public: Vec<_> = keys.iter().map(|k| k.verification_key.clone()).collect();
    let coordinates: Vec<_> = (1..=4).map(field).collect();
    let roster = Tb1Roster { coordinates: &coordinates, holder_keys: &public, f: 1 };
    let deadline = before.daa_score + policy.verifier_maturity_daa + policy.prosecution_window_daa + policy.lifetime_daa();
    let audit = Tb1MiningRound {
        chain_id: node.domain.as_bytes(),
        roster_root: roster.root().unwrap(),
        role_epoch: 1,
        logical_round: 11,
        purpose: Tb1MiningPurpose::ModelAudit,
        schedule_id: [11; 64],
        close_blue_score: before.blue_score + 2,
        release_from_daa: before.daa_score + 5,
        deadline_exclusive_daa: deadline,
    };
    let panel = Tb1MiningRound {
        logical_round: 12,
        purpose: Tb1MiningPurpose::Panel,
        close_blue_score: 2 * deadline + 100,
        release_from_daa: deadline + 100,
        deadline_exclusive_daa: 2 * deadline,
        ..audit
    };
    let a = audit.id();
    let p = panel.id();
    let roots = if let Some(r) = &reader {
        r.header.dealer_roots
    } else {
        [("audit", &a), ("panel", &p)].map(|(name, id)| {
            let path = dir.path().join(name);
            std::fs::create_dir(&path).unwrap();
            stable_panel::provision(&path, None, 0, id).roots
        })
    };
    let dealers = [0, 1, 2];
    let mut signed = [Vec::new(), Vec::new()];
    let mut resolve = |round: usize, id, session: u8| {
        if let Some(r) = &reader {
            let rows: &Vec<Vec<u8>> = &r.header.signatures[round];
            assert_eq!(rows.len(), 9);
            // An otherwise identical cut with a modified original signature falls short of qualification.
            assert!(
                qualification_from_ready(id, roster, &dealers, session, Some(&roots[round]), |i, _| {
                    let mut sig = rows[i].clone();
                    if i == 0 {
                        sig[0] ^= 1;
                    }
                    sig
                })
                .is_err()
            );
        }
        qualification_from_ready(id, roster, &dealers, session, Some(&roots[round]), |i, row| {
            if let Some(r) = &reader {
                r.header.signatures[round][i].clone()
            } else {
                let sig =
                    ml_dsa_65::sign(&keys[row.holder_slot].signing_key, &ready_message(row, 2048).unwrap(), READY_CONTEXT, [9; 32])
                        .unwrap()
                        .as_slice()
                        .to_vec();
                signed[round].push(sig.clone());
                sig
            }
        })
        .unwrap()
    };
    let aq = resolve(0usize, &a, 41);
    let pq = resolve(1usize, &p, 42);
    let pair = PreparedAudit::preparation_marker(&audit, &aq, &panel, &pq).unwrap();
    let prep = if let Some(r) = &mut reader {
        r.next(&mut node).await
    } else {
        local_carrier(&mut node, &mut None, &mut [], 0, pair, "long audit qualification").await
    };
    misaka_palw_sdk::qf1_local::registration_at(node.api().as_ref(), prep.hash(), &model).unwrap();
    let candidates =
        [AuditCandidate { class: identity.class, artifact: identity.inventory, descriptor: identity.descriptor, generation: 1 }];
    let plan = AuditPlan {
        suite: Hash64::from_u64_word(0xa501),
        candidate_budget: 1,
        tests_per_candidate: 2,
        reservation: Hash64::from_u64_word(0xa502),
        case_deadline_exclusive_daa: panel.deadline_exclusive_daa + 1,
    };
    let cut = PreparedAudit::read(node.api().as_ref(), prep.hash(), audit, &aq, &panel, &pq, plan, &candidates).unwrap();
    let marker = cut.close_commitment(prep.hash());
    let close = if let Some(r) = &mut reader {
        r.next(&mut node).await
    } else {
        local_carrier(&mut node, &mut None, &mut [], 0, marker, "long audit close").await
    };
    cut.holder_freeze(node.api().as_ref(), close.hash(), &aq).unwrap();
    if legacy {
        assert!(reader.is_some(), "must start from the recorded audit close");
        legacy_work::exercise(node, parts, close.hash(), identity.class, cut, &aq, deadline).await;
        return;
    }
    let floor = if let Some(r) = &mut reader {
        r.finish(&mut node).await;
        r.header.floor
    } else {
        certified_floor_history(&mut node.chain, &mut floats[0].clone()).await
    };
    let state = node.chain.tip_state().1;
    let frontier = state.safe_frontier().1;
    let claim = state.claim(&floor).unwrap();
    use kaspa_consensus_core::palw_rfc0018_v1::{PalwGateCViewV1, palw_independently_work_certified_v1};
    assert!(palw_independently_work_certified_v1(&floor, claim, &PalwGateCViewV1::of(&node.chain.bundle.state, &state)));
    assert!(matches!(claim.phase, kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Final { .. }));
    assert_eq!(claim.class_id, node.chain.bundle.base_class_id);
    let accepted = claim.accepted_block;
    let tip = node.chain.sink();
    let now = node.chain.daa_of(tip);
    let api = node.api().as_ref();
    for (parent, child) in
        [(registered, prep.hash()), (prep.hash(), close.hash()), (close.hash(), accepted), (accepted, frontier), (frontier, tip)]
    {
        assert!(api.is_chain_ancestor_of(parent, child).unwrap());
    }
    assert!((audit.release_from_daa..audit.deadline_exclusive_daa).contains(&now));
    assert_eq!(
        cut.observe(api, close.hash(), &aq).err().expect("frontier alone cannot make a freeze stable").kind(),
        std::io::ErrorKind::WouldBlock
    );
    let ready = readiness_of(&node);
    assert_eq!(ready.generation, tip);
    assert!(ready.finalized.finalized.is_none());
    peer_log!(
        "MAS1 certified audit frontier pid={} registration={registered} preparation={} close={} class={} floor={floor} frontier={frontier} tip={tip} daa={now} deadline={deadline} readiness={}",
        std::process::id(),
        prep.hash(),
        close.hash(),
        identity.class,
        serde_json::to_string(&ready).unwrap()
    );
    let observation = serde_json::json!({
        "registered": registered, "preparation": prep.hash(), "close": close.hash(), "freeze": Hash64::from_bytes(marker),
        "class": identity.class, "artifact": identity.inventory, "claim": floor, "accepted": accepted, "frontier": frontier,
        "state": state.state_root(), "deadline": deadline, "readiness": ready,
    });
    peer_log!(
        "MAS1 audit replay pid={} source={} state={} freeze={} accepted={} ready_signatures={} blocks={}",
        std::process::id(),
        if reader.is_some() { "archive" } else { "producer" },
        state.state_root(),
        Hash64::from_bytes(marker),
        accepted,
        if let Some(r) = &reader {
            r.header.signatures.iter().map(Vec::len).sum::<usize>()
        } else {
            signed.iter().map(Vec::len).sum::<usize>()
        },
        node.api().get_header(tip).unwrap().blue_score
    );
    if reader.is_none() {
        history::save(&node, &archive, registered, floor, roots, signed);
    }
    history::verify_peers(&archive, dir.path(), &observation);
}
