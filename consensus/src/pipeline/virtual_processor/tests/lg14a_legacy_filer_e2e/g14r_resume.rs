//! **Lane G14-R on the legacy V2 route: the public filer's pursuit RESUMES from chain state alone** — across a reorg that removes
//! its reservation, a full IBD node started mid-descent, and a pruned import after the reservation and the binding.
//!
//! The verifier is the parent's [`Verifier`]: the ONE engine the node's filer runs (`palw_fraud_filer_next_v1`) over the node's read
//! API and accepted blocks, and the verifier's own replay. Each case drops it and builds a new one on the other node (nothing carried:
//! its binding and localizer are re-learned from authenticated answers on chain), and counts what rode: one live reservation per
//! branch, no unit demanded twice.
use super::*;

impl Lg {
    /// This network's actors around another node's chain (a replay / a reorged / an imported node), with `funding` as their outputs.
    fn on_chain(&self, chain: T12Chain, funding: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)>) -> Lg {
        Lg {
            chain,
            config: self.config.clone(),
            bundle: self.bundle.clone(),
            premine: self.premine.clone(),
            floats: self.floats.clone(),
            funding,
            domain: self.domain,
            backend: backend_of(&self.config, &self.bundle),
            newcomer: self.newcomer,
            nonce: self.nonce,
            rnd: self.rnd.wrapping_add(0x40),
            _keep: Vec::new(),
        }
    }

    /// A node replaying every block of this one's selected chain from genesis (a full IBD).
    async fn ibd(&self) -> T12Chain {
        let z = t12_genesis_chain_on(TestConsensus::new(&self.config), &self.config, &self.bundle, &self.premine, &self.floats);
        for block in chain_blocks(&self.chain, self.chain.sink()) {
            arrive(&z, block, "a block of the first node").await;
        }
        z
    }
}

/// `(reservations by `card` of `claim`, demands by `card` of `claim`, distinct demanded event units)` on the node's selected chain.
fn carried(lg: &Lg, card: usize, claim: Hash64) -> (usize, usize, usize) {
    let bond = lg.bond(card);
    let (mut reservations, mut demands) = (0usize, 0usize);
    let mut units = std::collections::BTreeSet::new();
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        for tx in block.transactions.iter() {
            let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
            match payload.object {
                Obj::DisputeReservedV1 { reservation, .. } if reservation.claim == claim && reservation.reserver == bond => {
                    reservations += 1
                }
                Obj::DefaultAccused { claim: c, missing_event_index, accuser, .. } if c == claim && accuser == bond => {
                    demands += 1;
                    units.insert(missing_event_index);
                }
                _ => {}
            }
        }
    }
    (reservations, demands, units.len())
}

/// **A reorg removes the reservation; the pursuit re-derives on the new tip and convicts.** Post-Final (both branches keep the same
/// Final work, so the shipped strict-economic-win rule lets a shallow GHOSTDAG win decide, as in the parent's reorg case): on `A` the
/// newcomer reserves; `Z` follows `A`, then takes a heavier branch `B` from just before the reservation (≤ 2 DAA: the shallow-tie
/// window). On `Z`'s tip the dispute view holds no reservation; a NEW verifier (nothing carried) re-derives from that tip. In a DAG
/// `A`'s reservation carrier is not lost — `Z`'s next block merges it — so whichever lands first stands and the other is refused
/// (one reservation per reserver); funding comes from the virtual UTXO set (where `A`'s carrier already sits). The verifier
/// convicts after Final with one live reservation, its deposit refunded, no unit demanded twice.
#[tokio::test]
async fn g14r_lg14a_a_reorg_removes_the_reservation_and_a_new_verifier_re_derives_and_convicts_on_the_new_branch() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 3).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }));
    let fork = lg.chain.sink();
    let before = lg.collateral(NEWCOMER);
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    v.step(&mut lg, &produced, &|_| false).await;
    assert!(lg.view(id).record.is_some(), "A holds the reservation");

    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        arrive(&z, block, "A's block").await;
    }
    let mut b = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    let up_to_fork = chain_blocks(&lg.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for block in up_to_fork {
        arrive(&b, block, "a block up to the fork").await;
    }
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = lg.ttpb();
    let a_work = lg.chain.ctx.consensus.get_block(lg.chain.sink()).unwrap().header.blue_work;
    let mut b_blocks = Vec::new();
    while b.ctx.consensus.get_block(b.sink()).unwrap().header.blue_work <= a_work {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    let shallow = kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1;
    assert!(b.daa_of(b.sink()) - b.daa_of(fork) <= shallow, "B stays inside the real shallow-tie window");
    for block in &b_blocks {
        arrive(&z, block.clone(), "B's block").await;
    }
    assert_eq!(z.sink(), b.sink(), "Z reorgs onto B");
    let mut z = z;
    z.ctx.simulated_time = z.ctx.simulated_time.max(b.ctx.simulated_time);
    let mut zl = lg.on_chain(z, lg.funding.clone());
    assert!(zl.view(id).record.is_none(), "no reservation on B's tip");

    let mut v = Verifier::check(&zl, NEWCOMER, id);
    let verdict = v.pursue(&mut zl, &produced, &|_| false, 400).await;
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted, "convicted on the branch the reorg left");
    let (reservations, demands, units) = carried(&zl, NEWCOMER, id);
    eprintln!("[g14r] Z's chain blocks carry {reservations} reservation(s) of the newcomer (A's rides a merged block)");
    assert!(reservations <= 1, "at most one reservation of ours in Z's chain blocks");
    assert_eq!(demands, units, "no unit demanded twice: {demands} demands, {units} units");
    assert!(zl.view(id).record.is_none(), "every deposit refunded");
    assert_eq!(zl.collateral(NEWCOMER), before, "the honest verifier nets zero: no second deposit was kept");
}

/// **A full IBD node started mid-descent finishes the pursuit.** On `A` the verifier reserves and runs four steps (the binding read
/// and the first ranges); a node syncing every block from genesis takes over, a NEW verifier on it re-learns the binding and the
/// localizer from the authenticated answers on chain and convicts. One reservation, no unit demanded twice.
#[tokio::test]
async fn g14r_lg14a_an_ibd_node_started_mid_descent_carries_the_pursuit_to_the_conviction() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    for _ in 0..4 {
        v.step(&mut lg, &produced, &|_| false).await;
    }
    assert!(lg.view(id).record.is_some(), "reserved on A");
    let z = lg.ibd().await;
    let mut z = z;
    z.ctx.simulated_time = lg.chain.ctx.simulated_time;
    let mut zl = lg.on_chain(z, lg.funding.clone());
    assert_eq!(zl.chain.tip_state().1.state_root(), lg.chain.tip_state().1.state_root(), "the IBD node is at A's root");
    let mut v = Verifier::check(&zl, NEWCOMER, id);
    let verdict = v.pursue(&mut zl, &produced, &|_| false, 400).await;
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted, "convicted from the IBD node");
    let (reservations, demands, units) = carried(&zl, NEWCOMER, id);
    assert_eq!(reservations, 1, "never reserved twice");
    assert_eq!(demands, units, "no unit demanded twice: {demands} demands, {units} units");
}

/// **A node importing the PALW carriage at a pruning point after the reservation and the binding finishes the pursuit.** The importer
/// follows `A` through `P`, discards its own PALW tip and deltas through `P`, installs the carriage `A` serves for `P` (the dispute
/// record rides it), takes the blocks after `P`; a NEW verifier on it reads the live reservation and the answered binding off the rows
/// and the blocks, and convicts. (The importer here still holds `P`'s block bodies — a harness limit; the answers the descent needs
/// after the import are all above `P`, and a body below `P` lost to real pruning is the reacquisition path, codex's.)
#[tokio::test]
async fn g14r_lg14a_a_pruned_import_after_the_reservation_and_binding_carries_the_pursuit_to_the_conviction() {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    for _ in 0..2 {
        v.step(&mut lg, &produced, &|_| false).await;
    }
    assert!(lg.view(id).record.is_some(), "reserved before P");
    let p = lg.chain.sink();
    lg.beat(1).await;
    let all = chain_blocks(&lg.chain, lg.chain.sink());
    let k = all.iter().position(|b| b.header.hash == p).expect("P is on the chain");
    let t = all[k + 1].clone();
    let importer = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for b in &all[..=k] {
        arrive(&importer, b.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;
    let vp = lg.chain.vp();
    vp.capture_pruning_point_palw_state(p);
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");
    {
        let ivp = importer.vp();
        let mut store = ivp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(importer.config.params.genesis.hash).chain(all[..=k].iter().map(|b| b.header.hash)) {
            store.delete_delta_for_tests(blk).expect("no delta row at or below the pruning point");
        }
    }
    // codex RFC-12 (R6781-F merge): an import installs only against the pruning target its session established.
    importer.establish_pruning_import_target(p);
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs against T's committed root");
    assert_eq!(importer.tip_state().1.state_root(), root_at(&lg.chain, p), "the imported state is P's");
    assert!(importer.tip_state().1.legacy_dispute_v1(&id).is_some(), "the reservation rode the carriage");
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "a block after P").await;
    }
    let mut importer = importer;
    importer.ctx.simulated_time = lg.chain.ctx.simulated_time;
    let mut il = lg.on_chain(importer, lg.funding.clone());
    let mut v = Verifier::check(&il, NEWCOMER, id);
    let verdict = v.pursue(&mut il, &produced, &|_| false, 400).await;
    assert_eq!(verdict, PalwFilerPhaseV1::Convicted, "convicted from the pruned-import node");
    let (reservations, demands, units) = carried(&il, NEWCOMER, id);
    assert_eq!(reservations, 1, "never reserved twice");
    assert_eq!(demands, units, "no unit demanded twice");
}

/// **The legacy filer's public reads over a REAL socket, across an RPC disconnect and a node swap** (ops 204–206 and 202 through the
/// wRPC proxy and the gRPC server on 127.0.0.1, `tests/g14r_socket_rpc.rs`): after the newcomer reserves, an external monitor reads
/// its live reservation (op 206), the claim in its list (op 205) and the claim's held dispute document (op 204); the proxy restarts
/// (the connection drops) while the pursuit goes on; the node behind the socket is replaced by an IBD node, on which a NEW verifier
/// carries the pursuit to the conviction; a reconnected client reads the objective outcome (op 204 `Voided`, op 206 nothing live)
/// and a state proof of the claims collection (op 202) — through a JSON proxy too.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14r_lg14a_the_dispute_reads_over_a_real_socket_follow_the_pursuit_across_a_disconnect_and_a_node_swap() {
    use super::super::g14r_socket_rpc::SocketRpc;
    use kaspa_rpc_core::api::rpc::RpcApi;
    use kaspa_rpc_core::{
        GetPalwFraudFilerStatusRequest, GetPalwLegacyDisputeRequest, GetPalwLegacyDisputesRequest, GetPalwStateProofRequest,
    };
    use kaspa_wrpc_client::WrpcEncoding;
    kaspa_core::log::try_init_logger("warn");
    let mut lg = Lg::new(true);
    let (produced, _, _) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let newcomer = lg.bond(NEWCOMER);
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    v.step(&mut lg, &produced, &|_| false).await;
    let mut sock = SocketRpc::start(lg.chain.ctx.consensus.consensus_clone(), WrpcEncoding::Borsh).await;
    let client = sock.client().await;
    let status = client
        .get_palw_fraud_filer_status(GetPalwFraudFilerStatusRequest { bond: palw_bond_text_v1(&newcomer) })
        .await
        .expect("op 206 over the socket");
    assert!(status.available && status.json.contains(&id.to_string()), "op 206 shows the live reservation: {}", status.json);
    let listed = client
        .get_palw_legacy_disputes(GetPalwLegacyDisputesRequest { reserver: palw_bond_text_v1(&newcomer), limit: 16 })
        .await
        .expect("op 205 over the socket");
    assert_eq!(listed.claim_ids, vec![id.to_string()], "op 205 lists it");
    let held = client.get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: id.to_string() }).await.expect("op 204");
    assert!(held.available && held.json.contains("\"held\":true"), "op 204: {}", held.json);

    // The connection drops while the pursuit goes on.
    sock.restart_proxy().await;
    drop(client);
    for _ in 0..2 {
        v.step(&mut lg, &produced, &|_| false).await;
    }
    // The node behind the socket is replaced by a node that synced every block; a new verifier on it finishes.
    let mut z = lg.ibd().await;
    z.ctx.simulated_time = lg.chain.ctx.simulated_time;
    let mut zl = lg.on_chain(z, lg.funding.clone());
    sock.node.serve(zl.chain.ctx.consensus.consensus_clone());
    let client = sock.client().await;
    let mid = client.get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: id.to_string() }).await.expect("op 204");
    assert!(mid.json.contains("\"held\":true"), "the IBD node serves the same held dispute: {}", mid.json);
    let mut v = Verifier::check(&zl, NEWCOMER, id);
    assert_eq!(v.pursue(&mut zl, &produced, &|_| false, 400).await, PalwFilerPhaseV1::Convicted);
    let end = client.get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: id.to_string() }).await.expect("op 204");
    assert!(end.json.contains("Voided"), "op 204 serves the outcome: {}", end.json);
    let status = client
        .get_palw_fraud_filer_status(GetPalwFraudFilerStatusRequest { bond: palw_bond_text_v1(&newcomer) })
        .await
        .expect("op 206");
    assert!(status.json.contains("\"live\":[]"), "nothing live after the conviction: {}", status.json);
    let proof = client
        .get_palw_state_proof(GetPalwStateProofRequest { block_hash: String::new(), collection: "claims".into() })
        .await
        .expect("op 202 over the socket");
    assert!(proof.available && proof.header.is_some(), "op 202: {}", proof.reason);
    let json = SocketRpc::start(zl.chain.ctx.consensus.consensus_clone(), WrpcEncoding::SerdeJson).await;
    let json_client = json.client().await;
    let json_end = json_client.get_palw_legacy_dispute(GetPalwLegacyDisputeRequest { claim_id: id.to_string() }).await.unwrap();
    assert_eq!(json_end, end, "borsh and JSON frame one document");
    let _ = json_client.disconnect().await;
    json.stop().await;
    let _ = client.disconnect().await;
    sock.stop().await;
}
