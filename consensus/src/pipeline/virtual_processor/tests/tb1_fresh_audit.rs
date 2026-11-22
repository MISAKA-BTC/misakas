//! Private dealer fixture for a new signed MODEL_AUDIT and its independent PANEL.
//! The public archive carries only digests, signed READY rows and accepted blocks.
use super::*;
use sha2::{Digest, Sha512};
use std::{fs, os::unix::fs::DirBuilderExt, path::PathBuf};

fn seal(round: &[u8; 64], slot: usize, bytes: &[u8]) -> Hash64 {
    Hash64::from_bytes(
        Sha512::new()
            .chain_update(b"MISAKA/test/TB1/retained-input/v1")
            .chain_update(round)
            .chain_update((slot as u32).to_be_bytes())
            .chain_update(bytes)
            .finalize()
            .into(),
    )
}

fn private_dir(path: &Path) {
    fs::DirBuilder::new().mode(0o700).create(path).unwrap();
}

const ROLES: [&str; 2] = ["audit", "panel"];

pub(super) fn provision(
    scratch: &Path,
    ids: [&[u8; 64]; 2],
    public: &[ml_dsa_65::MLDSA65VerificationKey],
) -> ([[Hash64; 4]; 2], [[Hash64; 3]; 2]) {
    let root = PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_CUSTODY_CREATE").expect("new custody directory"));
    private_dir(&root);
    let holders: [_; 3] = std::array::from_fn(|slot| root.join(format!("holder-{slot}")));
    for point in &holders {
        private_dir(point);
    }
    let mut seals = [[Hash64::default(); 3]; 2];
    let roots = std::array::from_fn(|i| {
        let staging = scratch.join(ROLES[i]);
        private_dir(&staging);
        let setup = stable_panel::provision(&staging, None, 0, ids[i]);
        for (slot, holder) in holders.iter().enumerate() {
            let dir = holder.join(ROLES[i]);
            private_dir(&dir);
            let name = format!("panel-dealer-input-{slot}.json");
            let bytes = fs::read(staging.join(&name)).unwrap();
            let mut file = File::create_new(dir.join(&name)).unwrap();
            file.write_all(&bytes).unwrap();
            file.sync_all().unwrap();
            seals[i][slot] = seal(ids[i], slot, &bytes);
            Tb1ExposureJournal::provision(&dir, ids[i], &public[slot], slot as u32).unwrap();
            File::open(holder).unwrap().sync_all().unwrap();
        }
        setup.roots
    });
    File::open(&root).unwrap().sync_all().unwrap();
    File::open(root.parent().unwrap()).unwrap().sync_all().unwrap();
    (roots, seals)
}

pub(super) fn observe(
    node: &Rig,
    registered: Hash64,
    preparation: Hash64,
    close: Hash64,
    cut: &PreparedAudit<'_>,
    rounds: [Tb1MiningRound; 2],
    qualified: [&Tb1Qualification<'_>; 2],
    roots: [[Hash64; 4]; 2],
    seals: [[Hash64; 3]; 2],
    public: &[ml_dsa_65::MLDSA65VerificationKey],
    identity: Identity,
    reader: bool,
) {
    let slot: usize = if reader { std::env::var("MISAKA_PRIVATE_FRESH_SLOT").unwrap().parse().unwrap() } else { 0 };
    assert!(slot < 3);
    let holder = if reader {
        PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_HOLDER").expect("this holder's retained data"))
    } else {
        PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_CUSTODY_CREATE").unwrap()).join("holder-0")
    };
    let ids = rounds.map(|r| r.id());
    let api = node.api().as_ref();
    let marker = cut.close_commitment(preparation);
    let freeze = cut.holder_freeze(api, close, qualified[0]).unwrap();
    let (tip, state) = node.chain.tip_state();
    let now = node.chain.daa_of(tip);
    assert!((rounds[0].release_from_daa..rounds[0].deadline_exclusive_daa).contains(&now));
    assert_eq!(cut.observe(api, close, qualified[0]).err().unwrap().kind(), std::io::ErrorKind::WouldBlock);
    for (i, role) in ROLES.iter().enumerate() {
        let dir = holder.join(role);
        let bytes = fs::read(dir.join(format!("panel-dealer-input-{slot}.json"))).unwrap();
        assert_eq!(seal(&ids[i], slot, &bytes), seals[i][slot]);
        let provision: stable_panel::Provision = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(provision.roots, roots[i]);
        let shares = provision.holder_shares();
        assert_eq!(shares.len(), 3);
        let mut journal = Tb1ExposureJournal::resume(&dir, &ids[i], &public[slot], slot as u32).unwrap();
        if i == 0 {
            let row = Tb1HolderRelease {
                context: Tb1ReleaseContext { round_ref: &ids[i], qual_root: qualified[i].root(), freeze_id: marker },
                holder_slot: slot,
                shares: &shares,
            };
            assert_eq!(
                journal.reserve_signing_message(api, freeze, qualified[i], &row, 2048).unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        }
    }
    peer_log!(
        "fresh retained audit {} {}",
        std::process::id(),
        serde_json::json!({"slot":slot,"cold":reader,"class":identity.class,"inventory":identity.inventory,
        "descriptor":identity.descriptor,"registered":registered,"preparation":preparation,"close":close,"freeze":Hash64::from_bytes(marker),
        "rounds":ids.map(Hash64::from_bytes),"qualifications":qualified.map(|q|Hash64::from_bytes(q.root())),"seals":seals.map(|s|s[slot]),"tip":tip,"state":state.state_root(),
        "now":now,"blue":api.get_header(tip).unwrap().blue_score,"audit_from":rounds[0].release_from_daa,"audit_deadline":rounds[0].deadline_exclusive_daa,
        "panel_from":rounds[1].release_from_daa,"panel_deadline":rounds[1].deadline_exclusive_daa,"pruning_depth":node.config.params.pruning_depth(),
        "finality_depth":node.config.params.finality_depth(),"publication":"held","signing":"held"})
    );
}

/// Revalidate the writer's actual selected blocks on a restarted holder. The
/// retained shares are read only from this slot's original enrollment.
pub(super) async fn replay_window(
    node: &mut Rig,
    model: &misaka_palw_tir_exec::node::qf1::LocalInstance<'_>,
    cut: &PreparedAudit<'_>,
    close: Hash64,
    q: &Tb1Qualification<'_>,
    round: Tb1MiningRound,
    panel_qualification: &Tb1Qualification<'_>,
    marker: [u8; 64],
    roots: [Hash64; 4],
    seals: [Hash64; 3],
    keys: &[ml_dsa_65::MLDSA65KeyPair],
    path: &Path,
) {
    use kaspa_consensus_core::{
        palw_native_closed_facts_v1::{NATIVE_CLOSED_FACTS_TABLE_V1, NativeClosedFactRowV1},
        palw_native_readiness_v1::SafeWaitV1,
        palw_native_settlement_v1::native_delta_evidence_v1,
    };
    use std::io::{BufRead, BufReader, ErrorKind};
    let panel = cut.assessment_panel(panel_qualification).unwrap();
    let slot: usize = std::env::var("MISAKA_PRIVATE_FRESH_SLOT").unwrap().parse().unwrap();
    let dir = PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_HOLDER").unwrap()).join("audit");
    let id = round.id();
    let key = &keys[slot];
    let bytes = fs::read(dir.join(format!("panel-dealer-input-{slot}.json"))).unwrap();
    assert_eq!(seal(&id, slot, &bytes), seals[slot]);
    let local: stable_panel::Provision = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(local.roots, roots);
    let shares = local.holder_shares();
    let context = Tb1ReleaseContext { round_ref: &id, qual_root: q.root(), freeze_id: marker };
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &shares };
    let mut journal = Tb1ExposureJournal::resume(&dir, &id, &key.verification_key, slot as u32).unwrap();
    let base = node.chain.sink();
    let (_, delta) = node.vp().palw_state_v2_store.read().delta_of(base).unwrap();
    let cert = native_delta_evidence_v1(&delta);
    let &[claim] = cert.certified_claims.as_slice() else { panic!("expected the original certification block") };
    let work = node.chain.tip_state().1.claim(&claim).unwrap().work_id.unwrap();
    let readiness = readiness_of(node);
    let maturity = readiness
        .blocking
        .unwrap()
        .waits
        .iter()
        .find_map(|w| match w {
            SafeWaitV1::WaitingMaturity { ready_daa, .. } => *ready_daa,
            _ => None,
        })
        .unwrap();
    assert!(readiness.sink_daa < maturity && maturity < round.deadline_exclusive_daa);
    let mut crossed = false;
    let mut count = 0;
    let mut point = node.api().get_header(base).unwrap().pruning_point;
    for line in BufReader::new(File::open(path).unwrap()).lines() {
        let raw: RpcRawBlock = serde_json::from_str(&line.unwrap()).unwrap();
        let daa = raw.header.daa_score;
        let block = Block::try_from(raw).unwrap();
        let parent = node.chain.sink();
        let header_point = block.header.pruning_point;
        let hash = block.hash();
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(block.header.timestamp);
        stable_panel::arm(node);
        let status = node.api().validate_and_insert_block(block).virtual_state_task.await.unwrap();
        assert!(status.has_block_body());
        assert_eq!((node.chain.sink(), node.api().get_ghostdag_data(hash).unwrap().selected_parent), (hash, parent));
        if header_point != point {
            legacy_work::await_pruning_capture(node, header_point).await;
            point = header_point;
        }
        count += 1;
        if !crossed && daa >= maturity {
            crossed = true;
            let (_, state) = node.chain.tip_state();
            let fact: NativeClosedFactRowV1 =
                state.kernel_route_state_v1().unwrap().aux_row(NATIVE_CLOSED_FACTS_TABLE_V1, work.as_byte_slice()).unwrap();
            fact.validate_v1(work).unwrap();
            assert_eq!((fact.origin.subject, fact.fact.matured_daa), (claim, maturity));
            // Replaying a historical prefix must never mutate the holder's live exposure journal.
            assert_eq!(cut.observe(node.api().as_ref(), close, q).err().unwrap().kind(), ErrorKind::WouldBlock);
            peer_log!(
                "fresh audit maturity {} {}",
                std::process::id(),
                serde_json::json!({
                "base":base,"close":close,"claim":claim,"work":work,"tip":hash,"state":state.state_root(),"daa":daa,"blocks":count,
                "maturity":maturity,"readiness":readiness_of(node),"slot":slot,"publication":"held"})
            );
        }
    }
    assert!(crossed);
    let expected: Hash64 = std::env::var("MISAKA_PRIVATE_FRESH_WINDOW_TIP").unwrap().parse().unwrap();
    assert_eq!(node.chain.sink(), expected, "truncated or different window");
    // The pruning worker can finish after the last selected block committed its
    // settlement snapshot. Carry that same branch forward until custody observes
    // publication or the original deadline, and retain every successor for peers.
    if let Some(output) = std::env::var_os("MISAKA_PRIVATE_FRESH_EXTEND_WINDOW") {
        count += legacy_work::follow_release_window(node, cut, close, q, round, Path::new(&output)).await;
    }
    let (tip, state) = node.chain.tip_state();
    let api = node.api().as_ref();
    let gate = cut.observe(api, close, q);
    peer_log!(
        "fresh audit window end {} {}",
        std::process::id(),
        serde_json::json!({
        "slot":slot,"base":base,"close":close,"tip":tip,"state":state.state_root(),"blocks":count,
        "daa":node.chain.daa_of(tip),"committed_snapshot":api.get_native_settlement_snapshot().map_err(|e|e.to_string()),
        "readiness":readiness_of(node),"publication":match &gate {
            Ok(_) => "ready".to_owned(), Err(e) => format!("{:?}: {e}",e.kind()),
        }})
    );
    if let Some(output) = std::env::var_os("MISAKA_PRIVATE_FRESH_RELEASE_WRITE") {
        let gate = gate.as_ref().expect("sign only on an independently finalized close");
        let message = journal.reserve_signing_message(api, gate.publication_gate().holder_freeze(), q, &row, 2048).unwrap();
        let signature = ml_dsa_65::sign(&key.signing_key, &message, RELEASE_CONTEXT, rand::random()).unwrap();
        let record = ReleaseRecord {
            holder: slot,
            shares: shares.iter().map(|(d, v)| (*d, v.to_bytes().to_vec())).collect(),
            signature: signature.as_slice().to_vec(),
        };
        let file = File::create_new(output).unwrap();
        serde_json::to_writer(&file, &record).unwrap();
        file.sync_all().unwrap();
        peer_log!("fresh audit signed holder={slot} tip={tip} close={close}");
    }
    let write = std::env::var_os("MISAKA_PRIVATE_FRESH_PUBLICATION_WRITE");
    let read = std::env::var_os("MISAKA_PRIVATE_FRESH_PUBLICATION_READ");
    let submission_source = read.as_ref().map(|path| Path::new(path).with_extension("submission.json"));
    let submission_output = write.as_ref().map(|path| Path::new(path).with_extension("submission.json"));
    let publication = match (write, read) {
        (None, None) => return,
        (Some(path), None) => {
            let directory = PathBuf::from(std::env::var_os("MISAKA_PRIVATE_FRESH_SIGNED_ROWS").expect("three released audit rows"));
            let records: Vec<ReleaseRecord> = (0..3)
                .map(|i| {
                    let record: ReleaseRecord =
                        serde_json::from_reader(File::open(directory.join(format!("released-{i}.json"))).unwrap()).unwrap();
                    assert_eq!(record.holder, i);
                    record
                })
                .collect();
            let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
            let rows = views(&decoded, context);
            let gate = gate.as_ref().expect("current close must be finalized").publication_gate();
            let batch =
                local_carrier(node, &mut None, &mut [], 0, batch_commitment(&rows, LIMITS).unwrap(), "funded audit release batch")
                    .await;
            let marker = gate.propose(node.api().as_ref(), batch.hash(), &rows, q, LIMITS).unwrap();
            let certificate = local_carrier(node, &mut None, &mut [], 0, marker, "funded audit seed certificate").await;
            let result = Publication { batch: RpcRawBlock::from(&batch), certificate: RpcRawBlock::from(&certificate), rows: records };
            let file = File::create_new(path).unwrap();
            serde_json::to_writer(&file, &result).unwrap();
            file.sync_all().unwrap();
            result
        }
        (None, Some(path)) => {
            let result: Publication = serde_json::from_reader(File::open(path).unwrap()).unwrap();
            for raw in [&result.batch, &result.certificate] {
                let block = Block::try_from(raw.clone()).unwrap();
                let hash = block.hash();
                assert_eq!(block.header.direct_parents(), &[node.chain.sink()]);
                stable_panel::arrive(node, block, "archived audit seed publication").await;
                assert_eq!(node.chain.sink(), hash);
            }
            result
        }
        _ => panic!("choose one audit publication input or output"),
    };
    let frozen = gate.unwrap();
    let batch = Block::try_from(publication.batch).unwrap().hash();
    let certificate = Block::try_from(publication.certificate).unwrap().hash();
    let decoded: Vec<_> = publication.rows.iter().map(ReleaseRecord::decode).collect();
    let rows = views(&decoded, context);
    let api = node.api().as_ref();
    let seed = frozen.publication_gate().read_seed(api, batch, certificate, &rows, q, LIMITS).unwrap();
    assert!(frozen.publication_gate().read_seed(api, batch, certificate, &rows[..2], q, LIMITS).is_err());
    let cases = [0, 1].map(|case| frozen.challenge(api, batch, certificate, &rows, q, LIMITS, 0, case).unwrap());
    assert_ne!(cases[0], cases[1]);
    assert!(frozen.challenge(api, batch, certificate, &rows, q, LIMITS, 1, 0).is_err());
    peer_log!(
        "fresh audit certificate {} {}",
        std::process::id(),
        serde_json::json!({
            "slot":slot,"tip":tip,"close":close,"batch":batch,"certificate":certificate,"seed":faster_hex::hex_string(seed.as_bytes()),
            "cases":cases.map(|c|faster_hex::hex_string(&c)),"state":node.chain.tip_state().1.state_root()
        })
    );
    if std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_AUDIT").is_some() {
        use misaka_palw_remote::tb1_exposure::model_audit::arithmetic::{
            ArithmeticControlView, ArithmeticEvaluation, ArithmeticPanelPolicy, CASES, exact_digit_score, submission_marker,
        };
        let tokenizer = fs::read(std::env::var_os("MISAKA_PRIVATE_QF1_TOKENIZER").unwrap()).unwrap();
        assert_eq!(node.chain.sink(), certificate);
        let (block, results) = if let Some(path) = submission_source {
            let (raw, originals): (RpcRawBlock, [ArithmeticEvaluation; CASES as usize]) =
                serde_json::from_reader(File::open(path).unwrap()).unwrap();
            let block = Block::try_from(raw).unwrap();
            assert_eq!(block.header.direct_parents(), &[certificate]);
            stable_panel::arrive(node, block.clone(), "archived arithmetic submission").await;
            frozen
                .replay_arithmetic_submission(
                    node.api().as_ref(),
                    batch,
                    certificate,
                    &rows,
                    q,
                    LIMITS,
                    0,
                    block.hash(),
                    &originals,
                    model,
                    &tokenizer,
                )
                .unwrap();
            (block, originals)
        } else {
            let results = std::array::from_fn(|index| {
                frozen.evaluate_arithmetic(api, batch, certificate, &rows, q, LIMITS, 0, index as u32, model, &tokenizer).unwrap()
            });
            let marker = submission_marker(certificate, &results).unwrap();
            let block = local_carrier(node, &mut None, &mut [], 0, marker, "arithmetic evaluation submission").await;
            (block, results)
        };
        let read_submission = |candidate, cases: &[ArithmeticEvaluation; CASES as usize]| {
            frozen.read_arithmetic_submission(
                node.api().as_ref(),
                batch,
                certificate,
                &rows,
                q,
                LIMITS,
                candidate,
                block.hash(),
                cases,
            )
        };
        let marker = read_submission(0, &results).unwrap();
        assert!(read_submission(1, &results).is_err());
        let mut different_draw = results.clone();
        different_draw[0].challenge[0] ^= 1;
        assert!(read_submission(0, &different_draw).unwrap_err().to_string().contains("certified arithmetic draw"));
        for (index, row) in results.iter().enumerate() {
            assert_eq!(row.challenge, cases[index]);
            peer_log!(
                "fresh arithmetic evaluation {}",
                serde_json::json!({"pid":std::process::id(),
                "certificate":certificate,"evaluation":row,"correct":exact_digit_score(row.expected,&row.output)})
            );
        }
        assert_eq!(node.chain.sink(), block.hash());

        for (i, expected) in cases.iter().enumerate() {
            assert_eq!(frozen.challenge(node.api().as_ref(), batch, certificate, &rows, q, LIMITS, 0, i as u32).unwrap(), *expected);
        }
        if let Some(path) = submission_output {
            let file = File::create_new(path).unwrap();
            serde_json::to_writer(&file, &(RpcRawBlock::from(&block), &results)).unwrap();
            file.sync_all().unwrap();
        }
        peer_log!(
            "fresh arithmetic submission pid={} certificate={} block={} marker={}",
            std::process::id(),
            certificate,
            block.hash(),
            Hash64::from_bytes(marker)
        );
        let class = model.identity().class;
        if std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_BASE_OBSERVE").is_some() {
            peer_log!("arithmetic control population {} {}", std::process::id(), control_population(node, block.hash(), class));
        }
        let archive = match (
            std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_PANEL_PREFIX_WRITE"),
            std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_PANEL_PREFIX_READ"),
        ) {
            (None, None) => None,
            (Some(path), None) => Some(PrefixArchive::create(Path::new(&path))),
            (None, Some(path)) => {
                let expected = match (
                    std::env::var("MISAKA_PRIVATE_ARITHMETIC_PANEL_PARENT").ok(),
                    std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_PANEL_PARENT_REPORT"),
                ) {
                    (Some(hash), None) => PrefixParent::Known(hash.parse().unwrap()),
                    (None, Some(report)) => PrefixParent::VerifiedReport(PathBuf::from(report)),
                    _ => panic!("choose one expected PANEL parent source"),
                };
                Some(PrefixArchive::open(Path::new(&path), expected))
            }
            _ => panic!("choose one PANEL prefix source"),
        };
        let close_path = match (
            std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_ASSESSMENT_CLOSE_WRITE"),
            std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_ASSESSMENT_CLOSE_READ"),
        ) {
            (None, None) => None,
            (Some(path), None) => Some((path, true)),
            (None, Some(path)) => Some((path, false)),
            _ => panic!("choose one arithmetic assessment close source"),
        };
        let work_path = std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_ASSESSMENT_WORK_READ");
        assert!(work_path.is_none() || close_path.is_some(), "replaying PANEL work requires a pinned assessment close");
        assert!(archive.is_some() || close_path.is_none(), "an arithmetic assessment close requires its complete PANEL prefix");
        if let Some(archive) = archive {
            let parent = panel_prefix(node, block.hash(), panel.close_blue_score, panel.release_from_daa, archive).await;
            assert_eq!(
                frozen
                    .read_arithmetic_submission(node.api().as_ref(), batch, certificate, &rows, q, LIMITS, 0, block.hash(), &results)
                    .unwrap(),
                marker
            );
            if std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_BASE_OBSERVE").is_some() {
                assert_eq!(node.chain.sink(), parent);
                let observed = control_population_at(node, parent, class);
                peer_log!("arithmetic assessment parent state {} {}", std::process::id(), observed);
            }
            peer_log!(
                "arithmetic PANEL prefix pid={} submission={} parent={} round={} blue={} daa={}",
                std::process::id(),
                block.hash(),
                parent,
                Hash64::from_bytes(panel.id()),
                panel.close_blue_score - 1,
                node.chain.daa_of(parent)
            );

            if let Some((path, write)) = close_path {
                let (at, state) = node.vp().palw_observer_state_at_cached_v1(parent).unwrap();
                assert_eq!(at, parent);
                let fold = node.vp().palw_model_registry_fold_at(node.chain.daa_of(parent)).unwrap();
                let control = ArithmeticControlView { state: &state, config: &node.config.params, registry: &fold };
                let seats = std::env::var("MISAKA_PRIVATE_ARITHMETIC_CONTROL_SEATS").unwrap().parse().unwrap();
                let policy = ArithmeticPanelPolicy::new(seats).unwrap();
                let assessment = frozen
                    .prepare_arithmetic_panel(
                        node.api().as_ref(),
                        batch,
                        certificate,
                        &rows,
                        q,
                        LIMITS,
                        0,
                        block.hash(),
                        &results,
                        panel_qualification,
                        parent,
                        control,
                        policy,
                    )
                    .expect("eligible arithmetic PANEL control roster on the scheduled parent");
                let close = if write {
                    local_carrier(node, &mut None, &mut [], 0, assessment.commitment(), "arithmetic assessment PANEL close").await
                } else {
                    let raw: RpcRawBlock = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
                    let close = Block::try_from(raw).unwrap();
                    stable_panel::arrive(node, close.clone(), "archived arithmetic assessment PANEL close").await;
                    close
                };
                assessment.holder_freeze(node.api().as_ref(), close.hash(), panel_qualification).unwrap();
                assert_eq!(node.chain.sink(), close.hash());
                if write {
                    let file = File::create_new(path).unwrap();
                    serde_json::to_writer(&file, &RpcRawBlock::from(&close)).unwrap();
                    file.sync_all().unwrap();
                }
                peer_log!(
                    "arithmetic PANEL close {} {}",
                    std::process::id(),
                    serde_json::json!({"block":close.hash(), "parent":parent, "round":Hash64::from_bytes(panel.id()),
                        "blue":close.header.blue_score, "daa":close.header.daa_score, "state":close.header.palw_state_root,
                        "commitment":Hash64::from_bytes(assessment.commitment()), "policy":policy.id()})
                );
                if let Some(path) = work_path {
                    replay_assessment_work(node, &assessment, close.hash(), panel, panel_qualification, Path::new(&path)).await;
                }
            }
        }
    }
}

/// Independently process a real post-close suffix and stop at the first
/// selected tip at which both the signed clock and native finality allow release.
async fn replay_assessment_work(
    node: &mut Rig,
    assessment: &misaka_palw_remote::tb1_exposure::model_audit::arithmetic::ArithmeticPanelCut,
    close: Hash64,
    round: Tb1MiningRound,
    q: &Tb1Qualification<'_>,
    path: &Path,
) {
    use std::io::{BufRead, BufReader, ErrorKind};

    let mut archive = BufReader::new(File::open(path).unwrap()).lines();
    let mut tip = close;
    let mut pruning_point = node.api().get_header(tip).unwrap().pruning_point;
    loop {
        let daa = node.chain.daa_of(tip);
        assert!(daa < round.deadline_exclusive_daa, "arithmetic PANEL work exhausted the signed release window");
        if daa >= round.release_from_daa {
            match assessment.observe_publication(node.api().as_ref(), close, q) {
                Ok(_) => {
                    peer_log!(
                        "arithmetic PANEL release ready {} {}",
                        std::process::id(),
                        serde_json::json!({"close":close, "tip":tip, "daa":daa, "state":node.chain.tip_state().1.state_root()})
                    );
                    return;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("arithmetic PANEL release observation failed: {e}"),
            }
        }
        let raw: RpcRawBlock =
            serde_json::from_str(&archive.next().expect("PANEL work ends before native finality").unwrap()).unwrap();
        let block = Block::try_from(raw).unwrap();
        assert_eq!(block.header.direct_parents(), &[tip]);
        let point = block.header.pruning_point;
        tip = block.hash();
        stable_panel::arrive(node, block, "arithmetic PANEL post-close work").await;
        assert_eq!(node.chain.sink(), tip);
        if point != pruning_point {
            legacy_work::await_pruning_capture(node, point).await;
            pruning_point = point;
        }
    }
}

/// Observe the network's existing base population at the state committed by this
/// selected header. This is a population observation only; it assigns no seats.
fn control_population(node: &Rig, block: Hash64, class: Hash64) -> serde_json::Value {
    use kaspa_consensus_core::palw_state_proof_v1::state_root_of_pinned_header_v1;
    let native = node.api();
    let tip = native.get_sink();
    assert!(native.is_chain_ancestor_of(block, tip).unwrap());
    let header = native.get_header(block).unwrap();
    let source = native.get_ghostdag_data(block).unwrap().selected_parent;
    let mut observed = control_population_at(node, source, class);
    let committed = state_root_of_pinned_header_v1(&header, block).unwrap();
    assert_eq!(observed["state"], serde_json::json!(committed));
    assert_eq!(native.get_sink(), tip);
    observed["block"] = serde_json::json!(block);
    observed
}

/// Read the post-state of the specified selected block. At the pre-close tip this is
/// the state the next header will commit, including transitions made at this block.
fn control_population_at(node: &Rig, source: Hash64, class: Hash64) -> serde_json::Value {
    use kaspa_consensus_core::{
        mldsa87_primitives::mldsa87_key_id, palw_execution_lane_v1::palw_execution_span_v1, palw_state_v2::palw_base_population_v1,
    };
    let native = node.api();
    let tip = native.get_sink();
    assert!(native.is_chain_ancestor_of(source, tip).unwrap());
    let vp = node.vp();
    let (at, state) = vp.palw_observer_state_at_cached_v1(source).unwrap();
    assert_eq!(at, source);
    let root = state.state_root();
    let daa = native.get_header(source).unwrap().daa_score;
    let fold = vp.palw_model_registry_fold_at(daa).expect("the isolated control registry must be active");
    let span = palw_execution_span_v1(daa, fold.span_daa);
    let activation_pool = node.config.params.palw_activation_pool_at(daa).is_some();
    let registrant = state.class(&class).unwrap().registrant_bond;
    let owner = registrant.map(|key| state.bond(&key).expect("unresolved registrant bond"));
    let mut bonds = Vec::new();
    let mut operators = std::collections::BTreeSet::new();
    for (key, bond) in palw_base_population_v1(&state, &node.bundle.state, &fold, &class, daa, span, activation_pool) {
        assert_ne!(Some(*key), registrant);
        assert_ne!(Some(bond.operator_id), owner.map(|b| b.operator_id));
        if owner.is_some_and(|b| b.pubkey == bond.pubkey) {
            continue;
        }
        operators.insert(bond.operator_id);
        bonds.push(serde_json::json!({"txid":key.0.transaction_id,"index":key.0.index,"operator":bond.operator_id,
                                      "key_id":mldsa87_key_id(&bond.pubkey),"collateral":bond.collateral}));
    }
    assert_eq!(native.get_sink(), tip);
    serde_json::json!({"source":source,"source_daa":daa,"state":root,"class":class,
                       "registrant":registrant.map(|k|(k.0.transaction_id,k.0.index)), "registrant_operator":owner.map(|b|b.operator_id),
                       "registrant_key":owner.map(|b|mldsa87_key_id(&b.pubkey)),
                       "base_class":node.bundle.state.base_class_id(),"span":span,"operator_count":operators.len(),"bonds":bonds})
}

enum PrefixParent {
    Known(Hash64),
    // A streaming reader can start before its parent exists. Its feed must
    // close only after the complete writer archive has been verified.
    VerifiedReport(PathBuf),
}

enum PrefixArchive {
    Write(std::io::BufWriter<File>),
    Read(std::io::Lines<std::io::BufReader<File>>, PrefixParent),
}

impl PrefixArchive {
    fn create(path: &Path) -> Self {
        Self::Write(std::io::BufWriter::new(File::create_new(path).unwrap()))
    }

    fn open(path: &Path, parent: PrefixParent) -> Self {
        use std::io::BufRead;
        Self::Read(std::io::BufReader::new(File::open(path).unwrap()).lines(), parent)
    }
}

/// Retain or validate ordinary accepted blocks up to the parent of the PANEL close.
async fn panel_prefix(node: &mut Rig, submission: Hash64, close_blue: u64, release_from: u64, mut archive: PrefixArchive) -> Hash64 {
    use std::io::Write;
    assert_eq!(node.chain.sink(), submission);
    let submitted = node.api().get_header(submission).unwrap();
    let target = close_blue.checked_sub(1).unwrap();
    assert!(submitted.blue_score <= target);
    assert!(submitted.daa_score < release_from, "PANEL prefix reached the share release window");
    let mut tip = submission;
    let mut pruning_point = submitted.pruning_point;
    let step = node.config.params.target_time_per_block();
    for blue in submitted.blue_score + 1..=target {
        let block = match &mut archive {
            PrefixArchive::Write(_) => node.chain.heartbeat(step, vec![]).await,
            PrefixArchive::Read(lines, _) => {
                let raw: RpcRawBlock = serde_json::from_str(&lines.next().expect("truncated PANEL prefix").unwrap()).unwrap();
                let block = Block::try_from(raw).unwrap();
                assert_eq!(block.header.direct_parents(), &[tip]);
                node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(block.header.timestamp);
                stable_panel::arm(node);
                let status = node.api().validate_and_insert_block(block.clone()).virtual_state_task.await.unwrap();
                assert!(status.has_block_body());
                block
            }
        };
        assert_eq!(block.header.direct_parents(), &[tip]);
        assert_eq!(block.header.blue_score, blue);
        assert!(block.header.daa_score < release_from, "PANEL prefix reached the share release window");
        assert_eq!(block.header.pow_algo_id, kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID);
        assert_eq!(block.transactions.len(), 1);
        tip = block.hash();
        assert_eq!(node.chain.sink(), tip);
        if block.header.pruning_point != pruning_point {
            pruning_point = block.header.pruning_point;
            legacy_work::await_pruning_capture(node, pruning_point).await;
        }
        if let PrefixArchive::Write(writer) = &mut archive {
            serde_json::to_writer(&mut *writer, &RpcRawBlock::from(&block)).unwrap();
            writer.write_all(b"\n").unwrap();
        }
    }
    let daa = node.chain.daa_of(tip);
    match archive {
        PrefixArchive::Write(mut writer) => {
            writer.flush().unwrap();
            writer.get_ref().sync_all().unwrap();
        }
        PrefixArchive::Read(mut lines, expected) => {
            assert!(lines.next().is_none(), "extra PANEL prefix blocks");
            let expected = match expected {
                PrefixParent::Known(hash) => hash,
                PrefixParent::VerifiedReport(path) => {
                    let report: serde_json::Value = serde_json::from_reader(File::open(path).unwrap()).unwrap();
                    assert_eq!(report["submission"], serde_json::json!(submission));
                    assert_eq!(report["blue"], target);
                    assert_eq!(report["daa"], daa);
                    report["parent"].as_str().unwrap().parse().unwrap()
                }
            };
            assert_eq!(tip, expected);
        }
    }
    tip
}

#[tokio::test]
async fn replay_to_original_panel_parent() {
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let path = dir.path().join("prefix.jsonl");
    let parts = parts(Some(FENCE));
    let mut writer = Rig::new(&parts, 1);
    let start = writer.chain.sink();
    stable_panel::arm(&mut writer);
    let parent = panel_prefix(&mut writer, start, 5, 50, PrefixArchive::create(&path)).await;
    let report = dir.path().join("expected-parent.json");
    serde_json::to_writer(
        File::create_new(&report).unwrap(),
        &serde_json::json!({"submission":start, "parent":parent, "blue":4, "daa":writer.chain.daa_of(parent)}),
    )
    .unwrap();
    let mut reader = Rig::new(&parts, 2);
    let restored = panel_prefix(&mut reader, start, 5, 50, PrefixArchive::open(&path, PrefixParent::VerifiedReport(report))).await;
    assert_eq!(reader.api().get_header(restored).unwrap().blue_score, 4);
    assert_eq!(reader.chain.tip_state().1.state_root(), writer.chain.tip_state().1.state_root());
    let class = writer.bundle.state.base_class_id();
    let observed = control_population(&writer, parent, class);
    assert_eq!(control_population(&reader, restored, class), observed);
    assert_eq!(observed["source"], serde_json::json!(writer.api().get_ghostdag_data(parent).unwrap().selected_parent));
}

#[tokio::test]
#[should_panic(expected = "PANEL prefix reached the share release window")]
async fn reject_panel_parent_at_share_release() {
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let path = dir.path().join("prefix.jsonl");
    let parts = parts(Some(FENCE));
    let mut writer = Rig::new(&parts, 1);
    let start = writer.chain.sink();
    stable_panel::arm(&mut writer);
    let parent = panel_prefix(&mut writer, start, 9, 50, PrefixArchive::create(&path)).await;
    let release = writer.chain.daa_of(parent);
    assert!(release > writer.chain.daa_of(start));
    let mut reader = Rig::new(&parts, 2);
    panel_prefix(&mut reader, start, 9, release, PrefixArchive::open(&path, PrefixParent::Known(parent))).await;
}

/// Reopen the original short registration history without running the candidate
/// model. The model's own output cannot contribute bonds to this observation.
#[tokio::test]
#[ignore = "uses the archived registered arithmetic audit and its close record"]
async fn read_registered_arithmetic_control_population() {
    use crate::pipeline::virtual_processor::tests::g14_registration_e2e::private_moe2::arm_private_registration;
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let record: serde_json::Value =
        serde_json::from_reader(File::open(std::env::var_os("MISAKA_PRIVATE_ARITHMETIC_CLOSE_RECORD").unwrap()).unwrap()).unwrap();
    let audit = &record["audit"];
    let id = |name: &str| audit[name].as_str().unwrap().parse::<Hash64>().unwrap();
    let (config, _, premine, floats) = parts_custom(Some(FENCE), false, |p| arm_private_registration(p, true));
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    let mut node = Rig::new(&(config.clone(), bundle.clone(), premine, floats), 0x12_c500_0000);
    stable_panel::arm(&mut node);
    let mut reader = history::Reader::open(node.chain.sink()).unwrap();
    assert_eq!(reader.header.registered, id("registered"));
    while node.chain.sink() != id("preparation") {
        reader.next(&mut node).await;
    }
    let before_close = control_population_at(&node, node.chain.sink(), id("class"));
    let close = reader.next(&mut node).await;
    assert_eq!(close.hash(), id("close"));
    let mut confirmed = control_population(&node, close.hash(), id("class"));
    confirmed.as_object_mut().unwrap().remove("block");
    assert_eq!(confirmed, before_close);
    peer_log!("arithmetic preparation parent state {} {}", std::process::id(), before_close);
    reader.finish(&mut node).await;
    assert_eq!(node.chain.sink(), id("tip"));
    assert_eq!(node.chain.tip_state().1.class(&id("class")).unwrap().artifact_root, id("inventory"));
    let observed = control_population(&node, id("close"), id("class"));
    assert_eq!(observed["source"], serde_json::json!(id("preparation")));
    assert!(observed["registrant"].is_array() && observed["registrant_operator"].is_string());
    peer_log!("arithmetic control population {} {}", std::process::id(), observed);
}
