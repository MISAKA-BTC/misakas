//! The RFC-0012 rig supplies its test-only mature work fact. Fork choice,
//! executed headers, settlement generations and withdrawal readback are native.
//! VSS outcomes, allocated resources and the schedule are fixture inputs.
//! Each native database resolves the closure from the same accepted close block.
use super::*;
#[path = "tb1_exec_input.rs"]
mod exec_input;
use kaspa_rpc_core::RpcRawBlock;
use misaka_palw_remote::tb1_exposure::exec_input::{ExecInputBatch, REGISTRATION_DOMAIN, SignedExecInput};
use misaka_palw_remote::tb1_exposure::publication::{NativePublicationGate, PublicationLimits, PublishedRelease, batch_commitment};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsFd, OwnedFd},
        unix::net::UnixStream,
    },
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

const READER_DIR: &str = "MISAKA_PRIVATE_TB1_FREEZE_READER";
const READER_SLOT: &str = "MISAKA_PRIVATE_TB1_FREEZE_SLOT";

use kaspa_consensus_core::palw_tb1::{
    Tb1Field,
    freeze::{Tb1MiningClosure, Tb1MiningPurpose, Tb1MiningRound},
    qualification::{Tb1DealerEnd, Tb1PreparationCut, Tb1Qualification, Tb1Ready, Tb1Roster, Tb1VssEnd},
    releases::{Tb1HolderRelease, Tb1ReleaseContext},
    signatures::{READY_CONTEXT, RELEASE_CONTEXT, ready_message},
};
use libcrux_ml_dsa::ml_dsa_65;
use misaka_palw_remote::tb1_exposure::{Tb1ExposureJournal, Tb1HolderFreeze};

const LIMITS: PublicationLimits = PublicationLimits { records: 4, message_bytes: 2048, matrix_cells: 100 };

#[derive(Serialize, Deserialize)]
struct ReleaseRecord {
    holder: usize,
    shares: Vec<(usize, Vec<u8>)>,
    signature: Vec<u8>,
}

struct DecodedRecord {
    holder: usize,
    shares: Vec<(usize, Tb1Field)>,
    signature: ml_dsa_65::MLDSA65Signature,
}

impl ReleaseRecord {
    fn decode(&self) -> DecodedRecord {
        DecodedRecord {
            holder: self.holder,
            shares: self.shares.iter().map(|(d, v)| (*d, Tb1Field::from_bytes(v.as_slice().try_into().unwrap()).unwrap())).collect(),
            signature: ml_dsa_65::MLDSA65Signature::new(self.signature.as_slice().try_into().unwrap()),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Publication {
    batch: RpcRawBlock,
    certificate: RpcRawBlock,
    rows: Vec<ReleaseRecord>,
}

// Private fixture transport between live instances of the same executable.
// A length-delimited full-duplex channel also separates data from test output.
fn send<T: Serialize>(socket: &mut UnixStream, value: &T) {
    let bytes = serde_json::to_vec(value).unwrap();
    socket.write_all(&u32::try_from(bytes.len()).unwrap().to_be_bytes()).unwrap();
    socket.write_all(&bytes).unwrap();
}

fn receive<T: serde::de::DeserializeOwned>(socket: &mut UnixStream) -> T {
    let mut size = [0; 4];
    socket.read_exact(&mut size).unwrap();
    let size = u32::from_be_bytes(size) as usize;
    assert!(size < 4_000_000);
    let mut bytes = vec![0; size];
    socket.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn child_reader(dir: &Path, slot: usize) -> (Child, UnixStream) {
    let (parent, child_input) = UnixStream::pair().unwrap();
    for s in [&parent, &child_input] {
        s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        s.set_write_timeout(Some(Duration::from_secs(60))).unwrap();
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .arg("native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected")
        .args(["--test-threads=1", "--nocapture"])
        .env(READER_DIR, dir)
        .env(READER_SLOT, slot.to_string())
        .stdin(Stdio::from(OwnedFd::from(child_input)))
        .spawn()
        .unwrap();
    (child, parent)
}

fn views<'a>(rows: &'a [DecodedRecord], context: Tb1ReleaseContext<'a>) -> Vec<PublishedRelease<'a>> {
    rows.iter()
        .map(|r| PublishedRelease {
            row: Tb1HolderRelease { context, holder_slot: r.holder, shares: &r.shares },
            signature: &r.signature,
        })
        .collect()
}

fn field(v: usize) -> Tb1Field {
    let mut bytes = [0; 66];
    bytes[65] = v as u8;
    Tb1Field::from_bytes(&bytes).unwrap()
}

#[tokio::test]
async fn native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected() {
    let parts = parts(Some(FENCE));
    let mut node = Rig::new(&parts, 0x12_c500_0000);
    let parent = node.chain.sink();
    let template = node.beat();
    let keys: Vec<_> = (1..=4).map(|i| ml_dsa_65::generate_key_pair([i; 32])).collect();
    let public: Vec<_> = keys.iter().map(|kp| kp.verification_key.clone()).collect();
    let coordinates: Vec<_> = (1..=4).map(field).collect();
    let roster = Tb1Roster { coordinates: &coordinates, holder_keys: &public, f: 1 };
    let round = Tb1MiningRound {
        chain_id: node.domain.as_bytes(),
        roster_root: roster.root().unwrap(),
        role_epoch: 1,
        logical_round: 2,
        purpose: Tb1MiningPurpose::Exec,
        schedule_id: [9; 64],
        close_blue_score: template.header.blue_score,
        release_from_daa: template.header.daa_score + 1,
        deadline_exclusive_daa: template.header.daa_score + 11,
    };
    let round_ref = round.id();
    let round_ref = round_ref.as_slice();
    let dealers = [0, 1, 2];
    let ends: Vec<_> =
        (0..4).map(|d| Some(Tb1DealerEnd { end: Tb1VssEnd { end_anchor: [5; 64], transcript_root: [d; 64] }, position: 1 })).collect();
    let mut ready = Vec::new();
    for dealer in dealers {
        for holder_slot in 0..3 {
            ready.push(Tb1Ready {
                round_ref,
                dealer_slot: dealer,
                holder_slot,
                position: 2,
                end: ends[dealer].unwrap().end,
                signature: &[],
            });
        }
    }
    let signatures: Vec<_> = ready
        .iter()
        .map(|r| ml_dsa_65::sign(&keys[r.holder_slot].signing_key, &ready_message(r, 2048).unwrap(), READY_CONTEXT, [9; 32]).unwrap())
        .collect();
    for (r, sig) in ready.iter_mut().zip(&signatures) {
        r.signature = sig.as_slice();
    }
    let cut = Tb1PreparationCut { round_ref, deadline_position: 3, observed_through: 3, dealer_ends: &ends, ready: &ready };
    let q = Tb1Qualification::resolve(
        roster, round_ref, [7; 64], // reserved resources reference
        &dealers, &cut, 9, 2048,
    )
    .unwrap();
    let terms = exec_input::terms(parent);
    let panel = Tb1MiningRound {
        purpose: Tb1MiningPurpose::Panel,
        logical_round: 3,
        close_blue_score: round.close_blue_score + 20,
        release_from_daa: 20,
        deadline_exclusive_daa: 30,
        ..round
    };
    let owner = ml_dsa_65::generate_key_pair([82; 32]);
    let key = terms.registration_key(&round, &panel, &owner.verification_key, 16).unwrap();
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let owner_sig = if let Some(path) = std::env::var_os(READER_DIR) {
        let bytes = std::fs::read(Path::new(&path).join("input.sig")).unwrap();
        ml_dsa_65::MLDSA65Signature::new(bytes.as_slice().try_into().unwrap())
    } else {
        let sig = ml_dsa_65::sign(&owner.signing_key, &key, REGISTRATION_DOMAIN, [83; 32]).unwrap();
        std::fs::write(dir.path().join("input.sig"), sig.as_slice()).unwrap();
        sig
    };
    let entry =
        SignedExecInput { terms: &terms, owner: &owner.verification_key, signature: &owner_sig, slot: 7, funding_ref: [23; 64] };
    let changed = misaka_palw_remote::tb1_exposure::exec_input::ExecInputTerms { job_tokens: &[42, 108, 18], ..terms };
    assert!(ExecInputBatch::from_signed(&round, &panel, &[SignedExecInput { terms: &changed, ..entry }], 4, 16).is_err());
    assert!(
        ExecInputBatch::from_signed(&round, &panel, &[SignedExecInput { slot: 8, ..entry }, SignedExecInput { ..entry }], 4, 16)
            .is_err()
    );
    let inputs = ExecInputBatch::from_signed(&round, &panel, &[entry], 4, 16).unwrap();
    let closure = Tb1MiningClosure { round, qual_root: q.root(), consumer_set_root: inputs.root(), policy_root: terms.policy };

    let freeze_id = closure.commitment(parent);
    let child_dir = std::env::var_os(READER_DIR).map(std::path::PathBuf::from);
    let slot = if child_dir.is_some() { std::env::var(READER_SLOT).unwrap().parse::<usize>().unwrap() } else { 0 };
    let (prefix, work, tip) = if let Some(dir) = &child_dir {
        let raw: [RpcRawBlock; 6] = serde_json::from_reader(File::open(dir.join("blocks.json")).unwrap()).unwrap();
        let [close, e1, b1, e2, work, tip] = raw.map(|r| Block::try_from(r).unwrap());
        let prefix = vec![close, e1, b1, e2];
        for block in &prefix {
            node.arrive(block.clone(), "recorded close prefix").await;
        }
        let close = &prefix[0];
        plant_frontier(&node, close.header.blue_score, close.hash());
        node.arrive(work.clone(), "recorded work").await;
        place_maturing_fact(&node, &work, 10, 0);
        node.vp().pruning_point_store.write().set(close.hash(), 1).unwrap();
        node.arrive(tip.clone(), "recorded settlement tip").await;
        (prefix, work, tip)
    } else {
        let mut miner = new_miner_data();
        miner.extra_data = freeze_id.to_vec();
        let close = node.beat_with_miner(miner, vec![]);
        let close = node.take(close, "committed TB1 close").await;
        let mut prefix = vec![close];
        prefix.extend(extend(&mut node, &[0]).await);
        let beat = node.beat();
        prefix.push(node.take(beat, "b1").await);
        prefix.extend(extend(&mut node, &[1]).await);
        let close = &prefix[0];
        plant_frontier(&node, close.header.blue_score, close.hash());
        let work = extend(&mut node, &[2]).await.remove(0);
        place_maturing_fact(&node, &work, 10, 0);
        node.vp().pruning_point_store.write().set(close.hash(), 1).unwrap();
        let tip = extend(&mut node, &[3]).await.remove(0);
        (prefix, work, tip)
    };
    let anchor = prefix[0].hash();
    let before = snapshot_of(&node).unwrap();
    assert_eq!(before.generation, tip.hash());
    assert_eq!(before.finalized, Some(anchor));
    let context = Tb1ReleaseContext { round_ref, qual_root: q.root(), freeze_id };
    let api = node.api().as_ref();
    let freeze = Tb1HolderFreeze::resolve(api, anchor, &closure, &q).unwrap();
    let gate = NativePublicationGate::observe(api, anchor, &closure, &q).unwrap();
    let jobs = inputs.observe(api, anchor, &closure, &q).unwrap();
    let mut other_chain = round.chain_id;
    other_chain[0] ^= 1;
    assert!(!api.palw_coinbase_commits_to(other_chain, anchor, &freeze_id).unwrap());
    for changed in [
        Tb1MiningClosure { consumer_set_root: [65; 64], ..closure },
        Tb1MiningClosure { policy_root: [65; 64], ..closure },
        Tb1MiningClosure { round: Tb1MiningRound { purpose: Tb1MiningPurpose::Panel, ..round }, ..closure },
        Tb1MiningClosure { round: Tb1MiningRound { deadline_exclusive_daa: round.deadline_exclusive_daa + 1, ..round }, ..closure },
    ] {
        assert!(Tb1HolderFreeze::resolve(api, anchor, &changed, &q).is_err());
    }
    assert!(Tb1HolderFreeze::resolve(api, tip.hash(), &closure, &q).is_err());
    let shares = |slot: usize| dealers.map(|d| (d, field(10 + d + slot + 1)));
    let values = shares(slot);
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &values };
    Tb1ExposureJournal::provision(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
    let msg = journal.reserve_signing_message(api, freeze, &q, &row, 2048).unwrap();
    let sig = ml_dsa_65::sign(&keys[slot].signing_key, &msg, RELEASE_CONTEXT, [71; 32]).unwrap();
    eprintln!(
        "TB1 native acceptance pid {}, slot {slot}, anchor {anchor}, sink {}, qualification {}",
        std::process::id(),
        before.generation,
        Hash64::from_bytes(q.root())
    );
    let own_record = ReleaseRecord {
        holder: slot,
        shares: values.iter().map(|(d, v)| (*d, v.to_bytes().to_vec())).collect(),
        signature: sig.as_slice().to_vec(),
    };
    let mut peers = Vec::new();
    let mut parent_socket = None;
    let (records, batch, certificate) = if child_dir.is_some() {
        let mut socket = UnixStream::from(std::io::stdin().as_fd().try_clone_to_owned().unwrap());
        send(&mut socket, &own_record);
        let publication: Publication = receive(&mut socket);
        let batch = Block::try_from(publication.batch).unwrap();
        let certificate = Block::try_from(publication.certificate).unwrap();
        node.arrive(batch.clone(), "native signed batch").await;
        parent_socket = Some(socket);
        (publication.rows, batch, certificate)
    } else {
        let raw: Vec<_> = prefix.iter().chain([&work, &tip]).map(RpcRawBlock::from).collect();
        serde_json::to_writer(File::create_new(dir.path().join("blocks.json")).unwrap(), &raw).unwrap();
        peers.extend((1..3).map(|s| child_reader(dir.path(), s)));
        let mut records = vec![own_record];
        records.extend(peers.iter_mut().map(|(_, s)| receive::<ReleaseRecord>(s)));
        let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
        let rows = views(&decoded, context);
        let miner = MinerData::new(card_payout_spk(4), batch_commitment(&rows, LIMITS).unwrap().to_vec());
        let template = node.attempt_with_miner(4, vec![], no_evm(), vec![], miner);
        let batch = node.take(template, "TB1 signed batch").await;
        let marker = gate.propose(node.api().as_ref(), batch.hash(), &rows, &q, LIMITS).unwrap();
        assert!(gate.read_seed(node.api().as_ref(), batch.hash(), batch.hash(), &rows, &q, LIMITS).is_err());
        let miner = MinerData::new(card_payout_spk(5), marker.to_vec());
        let template = node.attempt_with_miner(5, vec![], no_evm(), vec![], miner);
        let certificate = template.to_immutable();
        let publication = Publication { batch: (&batch).into(), certificate: (&certificate).into(), rows: records };
        for (_, socket) in &mut peers {
            send(socket, &publication);
        }
        (publication.rows, batch, certificate)
    };
    let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
    let mut rows = views(&decoded, context);
    assert_eq!(node.chain.sink(), batch.hash());
    node.arrive(certificate.clone(), "native TB1 certificate").await;
    assert_eq!(node.chain.sink(), certificate.hash());
    let api = node.api().as_ref();
    let seed = gate.read_seed(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap();
    assert!(gate.read_seed(api, batch.hash(), certificate.hash(), &rows[..2], &q, LIMITS).is_err());
    rows.swap(0, 1);
    assert!(gate.read_seed(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS).is_err());
    rows.swap(0, 1);
    eprintln!(
        "TB1 publication pid {} batch {} certificate {} seed {}",
        std::process::id(),
        batch.hash(),
        certificate.hash(),
        faster_hex::hex_string(seed.as_bytes())
    );
    let request = jobs.derive(api, 7, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap();
    assert_eq!(request.registration_key(), &key);
    assert_eq!(request.tokens().len(), 38);
    assert_eq!(&request.tokens()[35..], terms.job_tokens);
    assert!(jobs.derive(api, 8, batch.hash(), certificate.hash(), &rows, &q, LIMITS).is_err());
    let ids = request.tokens().to_vec();
    eprintln!(
        "TB1 EXEC input pid {} slot 7 attempt {} tokens {:?}",
        std::process::id(),
        faster_hex::hex_string(request.attempt()),
        ids
    );
    if let Some(socket) = parent_socket.as_mut() {
        socket.write_all(seed.as_bytes()).unwrap();
    }
    let execution = std::env::var_os("MISAKA_PRIVATE_TB1_TIR_ARTIFACT").map(|path| exec_input::infer(api, Path::new(&path), &request));
    if let Some(run) = &execution {
        eprintln!("TB1 TIR inference pid {} {}", std::process::id(), serde_json::to_string(run).unwrap());
    }
    if let Some(path) = child_dir {
        serde_json::to_writer(File::create_new(path.join(format!("result-{slot}.json"))).unwrap(), &(ids, execution)).unwrap();
        return;
    }
    for (idx, (child, socket)) in peers.iter_mut().enumerate() {
        let mut received = [0; 32];
        socket.read_exact(&mut received).unwrap();
        assert_eq!(&received, seed.as_bytes());
        assert!(child.wait().unwrap().success());
        let peer: (Vec<u32>, Option<exec_input::Inference>) =
            serde_json::from_reader(File::open(dir.path().join(format!("result-{}.json", idx + 1))).unwrap()).unwrap();
        assert_eq!(peer.0, ids);
        assert_eq!(peer.1, execution);
    }
    let later = extend(&mut node, &[6]).await.remove(0);
    let api = node.api().as_ref();
    assert_eq!(gate.read_seed(api, batch.hash(), certificate.hash(), &rows, &q, LIMITS).unwrap(), seed);
    assert!(gate.read_seed(api, batch.hash(), later.hash(), &rows, &q, LIMITS).is_err());
    let mut rival = follower(&parts, 0x12_d500_0000, &prefix).await;
    plant_frontier(&rival, prefix[0].header.blue_score, anchor);
    let alternative = extend(&mut rival, &[4, 5, 6, 0, 1, 2]).await;
    for block in &alternative {
        node.arrive(block.clone(), "heavier branch").await;
    }
    let after = snapshot_of(&node).unwrap();
    assert_eq!(after.generation, alternative.last().unwrap().hash());
    assert!(node.api().is_chain_ancestor_of(anchor, after.generation).unwrap());
    assert_eq!(after.finalized, None);
    assert!(request.check_live(node.api().as_ref()).is_err());
    assert!(gate.read_seed(node.api().as_ref(), batch.hash(), certificate.hash(), &rows, &q, LIMITS).is_err());
    assert_eq!(readiness_of(&node).finalized.withdrawn_from, Some(anchor));
    assert!(journal.observe_branch(node.api().as_ref()).unwrap_err().to_string().contains("halted"));
    drop(journal);
    assert!(
        matches!(Tb1ExposureJournal::resume(dir.path(),context.round_ref,&public[0],0),Err(err) if err.to_string().contains("halted"))
    );
    eprintln!(
        "TB1 native custody: anchor {anchor}, prior sink {}, replacement {}, stop {:?}",
        before.generation, after.generation, after.stop
    );
}
