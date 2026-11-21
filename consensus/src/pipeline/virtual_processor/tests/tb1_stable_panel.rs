//! Isolated heartbeat-only continuation of the EXEC/Original fixture. Ordinary
//! native reads certify the close. The settlement work and dealer provisioning
//! are test inputs, independent of the new claim's inference or rewards.
use super::*;
use kaspa_consensus_core::palw_tb1::{Tb1DealerPoint, reconstruction::reconstruct_dealer};
use kaspa_math::Uint576;
use misaka_palw_remote::tb1_exposure::exec_input::panel_cut::LocalPanelCut;
use misaka_palw_remote::tb1_exposure::exec_input::{
    DerivedExecInput, LocalOriginalBody,
    panel_cut::{LocalReplayPolicy, REPLAY_SIGNATURE_DOMAIN, actor_id},
};
use misaka_palw_tir_exec::node::qf1::LocalInstance;
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha512};

pub(super) fn arm(node: &mut Rig) {
    let clock = node.chain.ctx.simulated_time;
    node.chain.ctx.consensus.set_validation_time_for_test(clock);
    node.vp().template_clock.store(clock, std::sync::atomic::Ordering::Relaxed);
}

fn clocked(node: &Rig) -> bool {
    node.chain.ctx.consensus.has_validation_time_for_test()
}

pub(super) fn heartbeat(node: &mut Rig, miner: MinerData) -> MutableBlock {
    if clocked(node) {
        node.chain.ctx.simulated_time += node.config.params.target_time_per_block();
        arm(node);
    }
    let b = node.beat_with_miner(miner, vec![]);
    if clocked(node) {
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(b.header.timestamp);
        arm(node);
    }
    b
}

pub(super) async fn arrive(node: &mut Rig, b: Block, label: &str) {
    if clocked(node) {
        node.chain.ctx.simulated_time = node.chain.ctx.simulated_time.max(b.header.timestamp);
        arm(node);
    }
    node.arrive(b, label).await;
}

pub(super) fn carrier(node: &mut Rig, card: usize, miner: MinerData) -> MutableBlock {
    if clocked(node) { heartbeat(node, miner) } else { node.attempt_with_miner(card, vec![], no_evm(), vec![], miner) }
}

pub(super) async fn next(node: &mut Rig, card: usize) -> Block {
    if clocked(node) {
        let b = heartbeat(node, new_miner_data());
        node.take(b, "heartbeat before EXEC release").await
    } else {
        extend(node, &[card]).await.remove(0)
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct Provision {
    pub roots: [Hash64; 4],
    // This holder's evaluations only. Dealer d's intercept and slope never enter the transport.
    shares: Vec<Vec<u8>>,
}

impl Provision {
    pub(super) fn holder_shares(&self) -> Vec<(usize, Tb1Field)> {
        self.shares.iter().enumerate().map(|(d, b)| (d, Tb1Field::from_bytes(b.as_slice().try_into().unwrap()).unwrap())).collect()
    }
}

fn random_coefficient() -> Tb1Field {
    loop {
        let mut bytes = [0; 66];
        OsRng.fill_bytes(&mut bytes);
        bytes[0] &= 1;
        if let Some(value) = Tb1Field::from_bytes(&bytes) {
            return value;
        }
    }
}

fn digest(id: &[u8; 64], dealer: usize, coefficients: &[Tb1Field]) -> Hash64 {
    let mut h = Sha512::new()
        .chain_update(b"MISAKA/test/TB1/dealer-polynomial/v1")
        .chain_update(id)
        .chain_update((dealer as u32).to_be_bytes());
    for c in coefficients {
        h.update(c.to_bytes());
    }
    Hash64::from_bytes(h.finalize().into())
}

pub(super) fn provision(dir: &Path, child: Option<&Path>, slot: usize, id: &[u8; 64]) -> Provision {
    let file = |p: &Path, slot: usize| p.join(format!("panel-dealer-input-{slot}.json"));
    if let Some(p) = child {
        return serde_json::from_reader(File::open(file(p, slot)).unwrap()).unwrap();
    }
    let polynomials: [[Tb1Field; 2]; 4] = std::array::from_fn(|_| [random_coefficient(), random_coefficient()]);
    let roots = std::array::from_fn(|d| digest(id, d, &polynomials[d]));
    let modulus = (Uint576::from_u64(1) << 521) - Uint576::from_u64(1);
    for holder in 0..3 {
        let x = Uint576::from_u64(holder as u64 + 1);
        let mut shares = Vec::new();
        for [a, b] in &polynomials[..3] {
            let value = (Uint576::from_be_bytes_var(&a.to_bytes()).unwrap() + x * Uint576::from_be_bytes_var(&b.to_bytes()).unwrap())
                % modulus;
            shares.push(value.to_be_bytes()[6..].to_vec());
        }
        serde_json::to_writer(File::create_new(file(dir, holder)).unwrap(), &Provision { roots, shares }).unwrap();
    }
    serde_json::from_reader(File::open(file(dir, slot)).unwrap()).unwrap()
}

// The holder keys and the global verifier keys have independent roles. Each
// verifier alone generates its session key; the pool exchange contains only public keys.
pub(super) struct Actors {
    mine: ml_dsa_65::MLDSA65KeyPair,
    keys: std::collections::BTreeMap<Hash64, ml_dsa_65::MLDSA65VerificationKey>,
    pub policy: LocalReplayPolicy,
}

impl Actors {
    pub(super) fn enroll(parent: &mut Option<UnixStream>, peers: &mut [(Child, UnixStream)]) -> Self {
        let mine = ml_dsa_65::generate_key_pair(rand::random());
        let public = mine.verification_key.as_slice().to_vec();
        let all = if let Some(s) = parent {
            send(s, &public);
            receive::<Vec<Vec<u8>>>(s)
        } else {
            let mut all = vec![public];
            for (_, s) in peers.iter_mut() {
                all.push(receive(s));
            }
            for (_, s) in peers.iter_mut() {
                send(s, &all);
            }
            all
        };
        let keys: std::collections::BTreeMap<_, _> = all
            .iter()
            .map(|bytes| {
                let vk = ml_dsa_65::MLDSA65VerificationKey::new(bytes.as_slice().try_into().unwrap());
                (actor_id(&vk), vk)
            })
            .collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.contains_key(&actor_id(&mine.verification_key)));
        Self { mine, keys, policy: LocalReplayPolicy::new(2).unwrap() }
    }
    pub(super) fn candidates(&self) -> Vec<(Hash64, u64)> {
        self.keys.keys().copied().zip([1, 2, 4]).collect()
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn release(
    node: &mut Rig,
    parent: &mut Option<UnixStream>,
    peers: &mut [(Child, UnixStream)],
    dir: &Path,
    slot: usize,
    key: &ml_dsa_65::MLDSA65KeyPair,
    q: &Tb1Qualification<'_>,
    id: &[u8; 64],
    cut: &LocalPanelCut,
    close_parent: Hash64,
    original_receipt: Hash64,
    close: &Block,
    release_from_daa: u64,
    setup: &Provision,
    exec_seed: &[u8; 32],
    exec_rows: &[PublishedRelease<'_>],
    actors: &Actors,
    input: &DerivedExecInput<'_>,
    instance: &LocalInstance<'_>,
    body: &LocalOriginalBody,
    owner: &ml_dsa_65::MLDSA65Signature,
    original: Hash64,
) {
    let marker = cut.commitment(close_parent);
    let context = Tb1ReleaseContext { round_ref: id, qual_root: q.root(), freeze_id: marker };
    let shares = setup.holder_shares();
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &shares };
    Tb1ExposureJournal::provision(dir, id, &key.verification_key, slot as u32).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir, id, &key.verification_key, slot as u32).unwrap();
    let freeze = cut.resolve(node.api().as_ref(), close.hash(), q).unwrap();
    assert!(journal.reserve_signing_message(node.api().as_ref(), freeze, q, &row, 2048).is_err());

    let _later = local_carrier(node, parent, peers, 0, [81; 64], "PANEL confirmation heartbeat").await;
    plant_frontier(node, close.header.blue_score, close.hash());
    let work = local_carrier(node, parent, peers, 0, [82; 64], "independent mature work fixture").await;
    let fact = MatureUsefulWorkV1 {
        identity: Hash64::from_u64_word(0x1301),
        anchor: work.hash(),
        operator: Hash64::from_u64_word(0x1302),
        class: Hash64::from_u64_word(0x1303),
        anchor_blue: work.header.blue_score,
        accepted_blue: work.header.blue_score,
        anchor_daa: work.header.daa_score,
        accepted_daa: work.header.daa_score,
        matured_daa: work.header.daa_score,
        work: 10,
    };
    node.vp().native_fact_override.lock().entry(work.hash()).or_default().push(fact);
    node.vp().native_readiness_memo.lock().take();
    node.vp().pruning_point_store.write().set(close.hash(), 2).unwrap();
    let stable = local_carrier(node, parent, peers, 0, [83; 64], "PANEL stability observation").await;
    assert_eq!(snapshot_of(node).unwrap().finalized, Some(close.hash()), "{}", serde_json::to_string(&readiness_of(node)).unwrap());
    let mut n = 0;
    while node.chain.daa_of(node.chain.sink()) < release_from_daa {
        assert!(n < 240, "PANEL release DAA stalled at {}", node.chain.daa_of(node.chain.sink()));
        local_carrier(node, parent, peers, 0, [84; 64], "wait for PANEL DAA").await;
        n += 1;
    }
    let release_daa = node.chain.daa_of(node.chain.sink());
    let gate = cut.observe(node.api().as_ref(), close.hash(), q).unwrap();
    let msg = journal.reserve_signing_message(node.api().as_ref(), gate.holder_freeze(), q, &row, 2048).unwrap();
    let sig = ml_dsa_65::sign(&key.signing_key, &msg, RELEASE_CONTEXT, rand::random()).unwrap();
    let record = ReleaseRecord {
        holder: slot,
        shares: shares.iter().map(|(d, v)| (*d, v.to_bytes().to_vec())).collect(),
        signature: sig.as_slice().to_vec(),
    };
    let records = if let Some(s) = parent {
        send(s, &record);
        receive::<Vec<ReleaseRecord>>(s)
    } else {
        let mut records = vec![record];
        for (_, s) in peers.iter_mut() {
            records.push(receive(s));
        }
        for (_, s) in peers.iter_mut() {
            send(s, &records);
        }
        records
    };
    let decoded: Vec<_> = records.iter().map(ReleaseRecord::decode).collect();
    let rows = views(&decoded, context);
    let batch = local_carrier(node, parent, peers, 0, batch_commitment(&rows, LIMITS).unwrap(), "signed PANEL shares").await;
    let api = node.api().as_ref();
    assert!(gate.propose(api, batch.hash(), exec_rows, q, LIMITS).is_err());
    let certificate_marker = gate.propose(api, batch.hash(), &rows, q, LIMITS).unwrap();
    let cert = local_carrier(node, parent, peers, 0, certificate_marker, "PANEL seed certificate").await;
    let api = node.api().as_ref();
    let seed = gate.read_seed(api, batch.hash(), cert.hash(), &rows, q, LIMITS).unwrap();
    assert!(gate.read_seed(api, batch.hash(), cert.hash(), &rows[..2], q, LIMITS).is_err());
    assert_ne!(seed.as_bytes(), exec_seed);
    for d in 0..3 {
        let points: Vec<_> = decoded.iter().map(|r| Tb1DealerPoint { holder_slot: r.holder, value: r.shares[d].1 }).collect();
        let recovered = reconstruct_dealer(q.roster().coordinates, 1, &points, LIMITS.matrix_cells).unwrap();
        assert_eq!(digest(id, d, &recovered.coefficients), setup.roots[d]);
    }
    peer_log!(
        "TB1 stable PANEL pid={} slot={} close={} stable={} stable_daa={} release_daa={} batch={} certificate={} seed={}",
        std::process::id(),
        slot,
        close.hash(),
        stable.hash(),
        stable.header.daa_score,
        release_daa,
        batch.hash(),
        cert.hash(),
        faster_hex::hex_string(seed.as_bytes())
    );
    if let Some(s) = parent {
        send(s, seed.as_bytes());
    } else {
        for (_, s) in peers.iter_mut() {
            assert_eq!(&receive::<[u8; 32]>(s), seed.as_bytes());
        }
    }
    let api = node.api().as_ref();
    let draw =
        cut.draw_replays(api, close.hash(), &gate, batch.hash(), cert.hash(), &rows, q, LIMITS, original, actors.policy).unwrap();
    assert!(
        cut.draw_replays(
            api,
            close.hash(),
            &gate,
            batch.hash(),
            cert.hash(),
            &rows,
            q,
            LIMITS,
            original,
            LocalReplayPolicy::new(1).unwrap()
        )
        .is_err()
    );
    let key = &actors.mine.verification_key;
    let actor = actor_id(key);
    let selected = draw.selected().contains(&actor);
    let message = draw.replay_message(api, input, instance, body, owner, key);
    let before_stop = if inconsistent_readout_fixture() {
        let reason = message.unwrap_err().to_string();
        if selected {
            assert!(reason.contains("different replayed local QF1 Original"), "{reason}");
        } else {
            assert!(reason.contains("not selected"), "{reason}");
        }
        assert!(draw.reply_commitment(api, &[]).is_err());
        peer_log!(
            "TB1 replay duty pid={} actor={} replayed={} selected={:?} original={} replies=none",
            std::process::id(),
            actor,
            selected,
            draw.selected(),
            original
        );
        cert.hash()
    } else {
        let reply = if selected {
            Some((
                actor,
                ml_dsa_65::sign(&actors.mine.signing_key, &message.unwrap(), REPLAY_SIGNATURE_DOMAIN, rand::random())
                    .unwrap()
                    .as_slice()
                    .to_vec(),
            ))
        } else {
            assert!(message.unwrap_err().to_string().contains("not selected"));
            None
        };
        let ordered = if let Some(s) = parent {
            send(s, &reply);
            receive::<Vec<(Hash64, Vec<u8>)>>(s)
        } else {
            let mut votes = std::collections::BTreeMap::new();
            for opt in std::iter::once(reply).chain(peers.iter_mut().map(|(_, s)| receive(s))) {
                if let Some((id, signature)) = opt {
                    assert!(votes.insert(id, signature).is_none());
                }
            }
            let ordered: Vec<_> = draw.selected().iter().map(|id| (*id, votes.remove(id).unwrap())).collect();
            assert!(votes.is_empty());
            for (_, s) in peers.iter_mut() {
                send(s, &ordered);
            }
            ordered
        };
        let sigs: Vec<_> = ordered.iter().map(|(_, b)| ml_dsa_65::MLDSA65Signature::new(b.as_slice().try_into().unwrap())).collect();
        let replies: Vec<_> = ordered.iter().zip(&sigs).map(|((id, _), sig)| (&actors.keys[id], sig)).collect();
        assert!(draw.reply_commitment(api, &replies[..1]).is_err());
        assert!(draw.reply_commitment(api, &[replies[0], replies[0]]).is_err());
        assert!(draw.verify_reply(api, replies[0].0, replies[1].1).is_err());
        let commitment = draw.reply_commitment(api, &replies).unwrap();
        assert!(draw.read_replies(api, cert.hash(), &replies).is_err());
        let accepted = local_carrier(node, parent, peers, 0, commitment, "selected full replay replies").await;
        draw.read_replies(node.api().as_ref(), accepted.hash(), &replies).unwrap();
        peer_log!(
            "TB1 replay duty pid={} actor={} replayed={} selected={:?} original={} replies={}",
            std::process::id(),
            actor,
            selected,
            draw.selected(),
            original,
            accepted.hash()
        );
        accepted.hash()
    };
    use misaka_palw_remote::tb1_exposure::exec_input::stop_case::{StopOutcome, StopWitness};
    let wire = std::fs::read(dir.join(format!("{original}.stop"))).expect("Stop disclosure published with the Original");
    let api = node.api().as_ref();
    let witness = StopWitness::decode(body, &wire).unwrap();
    let mut changed = witness.clone();
    changed.stop[17] ^= 1;
    assert!(input.prepare_stop_case(api, original_receipt, body, owner, &changed).is_err());
    let case = input.prepare_stop_case(api, original_receipt, body, owner, &witness).unwrap();
    assert!(case.verdict_marker(api, before_stop).is_err());
    let opened = local_carrier(node, parent, peers, 0, case.marker(), "QF1 last answer / Stop case").await;
    let marker = case.verdict_marker(node.api().as_ref(), opened.hash()).unwrap();
    assert!(case.read_verdict(node.api().as_ref(), opened.hash(), opened.hash()).is_err());
    let verdict = local_carrier(node, parent, peers, 0, marker, "QF1 last answer / Stop verdict").await;
    let outcome = case.read_verdict(node.api().as_ref(), opened.hash(), verdict.hash()).unwrap();
    assert_eq!(outcome, StopOutcome::ChallengeNotEstablished);
    peer_log!(
        "TB1 Court Stop pid={} original={} case={} verdict={} result={:?} witness={}",
        std::process::id(),
        original,
        opened.hash(),
        verdict.hash(),
        outcome,
        faster_hex::hex_string(&wire)
    );
    let end_wire = std::fs::read(dir.join(format!("{original}.end"))).expect("terminal Readout and End published with the Original");
    let case = input.prepare_end_case(node.api().as_ref(), original_receipt, body, owner, &end_wire).unwrap();
    assert!(case.verdict_marker(node.api().as_ref(), opened.hash()).is_err());
    let opened = local_carrier(node, parent, peers, 0, case.marker(), "QF1 Readout / End case").await;
    let marker = case.verdict_marker(node.api().as_ref(), opened.hash()).unwrap();
    let verdict = local_carrier(node, parent, peers, 0, marker, "QF1 Readout / End verdict").await;
    let outcome = case.read_verdict(node.api().as_ref(), opened.hash(), verdict.hash()).unwrap();
    let expected = if inconsistent_readout_fixture() { StopOutcome::ComputationRefuted } else { StopOutcome::ChallengeNotEstablished };
    assert_eq!(outcome, expected);
    peer_log!(
        "TB1 Court End pid={} original={} case={} verdict={} result={:?} witness={}",
        std::process::id(),
        original,
        opened.hash(),
        verdict.hash(),
        outcome,
        faster_hex::hex_string(&end_wire)
    );
}
