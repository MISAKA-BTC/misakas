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
    cut: &PreparedAudit<'_>,
    close: Hash64,
    q: &Tb1Qualification<'_>,
    round: Tb1MiningRound,
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
    for line in BufReader::new(File::open(path).unwrap()).lines() {
        let raw: RpcRawBlock = serde_json::from_str(&line.unwrap()).unwrap();
        let daa = raw.header.daa_score;
        let block = Block::try_from(raw).unwrap();
        let parent = node.chain.sink();
        let hash = block.hash();
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(block.header.timestamp);
        stable_panel::arm(node);
        let status = node.api().validate_and_insert_block(block).virtual_state_task.await.unwrap();
        assert!(status.has_block_body());
        assert_eq!((node.chain.sink(), node.api().get_ghostdag_data(hash).unwrap().selected_parent), (hash, parent));
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
    let (tip, state) = node.chain.tip_state();
    let expected: Hash64 = std::env::var("MISAKA_PRIVATE_FRESH_WINDOW_TIP").unwrap().parse().unwrap();
    assert_eq!(tip, expected, "truncated or different window");
    let point = node.api().get_header(tip).unwrap().pruning_point;
    legacy_work::await_pruning_capture(node, point).await;
    let api = node.api().as_ref();
    let gate = cut.observe(api, close, q);
    peer_log!(
        "fresh audit window end {} {}",
        std::process::id(),
        serde_json::json!({
        "slot":slot,"base":base,"close":close,"tip":tip,"state":state.state_root(),"blocks":count,
        "daa":node.chain.daa_of(tip),"readiness":readiness_of(node),"publication":match &gate {
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
}
