//! **Lane G14-R, the external path: the kernel route's public prosecution driven ENTIRELY over a real socket** — every read through
//! the wRPC proxy (`KaspaRpcClient` → websocket on 127.0.0.1 → `kaspa_wrpc_server` → gRPC on 127.0.0.1 → `kaspa_grpc_server` → the
//! ops' handlers, `tests/g14r_socket_rpc.rs`) and every carrier through `submitTransaction` and the node's mempool check, with the RPC
//! connection dropped and the node behind the socket replaced mid-pursuit.
use super::super::g14r_socket_rpc::SocketRpc;
use super::canonical::{agree, ibd_node, sync};
use super::recovery::{assert_convicted_once, covered_lie, det_chunk, det_route};
use super::*;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{
    GetPalwConformanceEvidenceRequest, GetPalwKernelClaimRequest, GetPalwKernelFinalsRequest, GetPalwKernelRowsRequest,
    GetPalwStateProofRequest, RpcTransaction,
};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use misaka_palw_remote::pursuit::{
    KernelFilingV1, KernelGoalV1, KernelPursuitOutcomeV1, KernelPursuitStepV1, KernelPursuitV1, plan_kernel_pursuit_v1,
};

fn unhex(text: &str) -> Vec<u8> {
    let mut out = vec![0u8; text.len() / 2];
    faster_hex::hex_decode(text.as_bytes(), &mut out).expect("the RPC serves hex");
    out
}

/// **Op 211 over the socket, paged to the end**, reassembled and checked against the served roots (one snapshot across pages).
async fn route_over(client: &KaspaRpcClient) -> (PalwKernelRouteStateV1, Hash64) {
    use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, PalwKernelRouteHeaderV1};
    let mut request = GetPalwKernelRowsRequest { has_cursor: false, after_table: 0, after_key: String::new(), max_bytes: 16 << 10 };
    let (mut header, mut roots): (Option<PalwKernelRouteHeaderV1>, Option<(String, String)>) = (None, None);
    let (mut rows, mut aux) = (misaka_palw_kernel::rows::LedgerRowsV1::new(), BTreeMap::new());
    let mut seen = 0u64;
    let mut total: Option<u64>;
    loop {
        let page = client.get_palw_kernel_rows(request.clone()).await.expect("op 211 over the socket");
        assert!(page.available, "the node serves the kernel route");
        let h: PalwKernelRouteHeaderV1 = borsh::from_slice(&unhex(&page.header)).expect("the served header decodes");
        assert!(header.as_ref().is_none_or(|prev| prev == &h), "one snapshot across pages");
        header = Some(h);
        let r = (page.ledger_root.clone(), page.aux_root.clone());
        assert!(roots.as_ref().is_none_or(|prev| prev == &r), "one snapshot across pages");
        roots = Some(r);
        total = Some(page.total_rows);
        for row in &page.rows {
            seen += 1;
            let table = u8::try_from(row.table).expect("a table fits a u8");
            if table < PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1 {
                rows.insert((table, unhex(&row.key)), unhex(&row.row));
            } else {
                aux.insert((table, unhex(&row.key)), unhex(&row.row));
            }
        }
        if !page.more {
            break;
        }
        request = GetPalwKernelRowsRequest {
            has_cursor: true,
            after_table: page.next_table,
            after_key: page.next_key.clone(),
            max_bytes: 16 << 10,
        };
    }
    assert_eq!(Some(seen), total, "every row exactly once");
    let state = PalwKernelRouteStateV1 { header: header.unwrap(), rows, aux, ledger_cache: Default::default() };
    let (ledger_root, aux_root) = roots.unwrap();
    assert_eq!(state.ledger_root().to_string(), ledger_root, "the rows root to the served ledger root");
    assert_eq!(state.aux_root().to_string(), aux_root, "the aux rows to the served aux root");
    (state, ledger_root.parse().expect("a Hash64"))
}

/// **A verifier that holds no state and talks to its node only through `client`.** `node` is the node behind the socket (its funding
/// chain builds the carriers; its template mines what `submitTransaction` admitted).
struct SocketPursuer {
    card: usize,
    model: MapParams,
    chunks: usize,
}

impl SocketPursuer {
    async fn derive(&self, client: &KaspaRpcClient, node: &Net, claim: &Digest, da: &Da) -> (PalwKernelRouteStateV1, KernelPursuitV1) {
        let (state, root) = route_over(client).await;
        let fresh = Fresh::from_api(&state, root, 0x5A);
        let goal = match fresh.ledger.claims.get(claim) {
            Some(row) if !row.convicted => {
                // Op 210 over the socket agrees with the rebuilt ledger.
                let read = client
                    .get_palw_kernel_claim(GetPalwKernelClaimRequest { claim_id: Hash64::from_bytes(*claim).to_string() })
                    .await
                    .expect("op 210 over the socket");
                let (record, _) = fresh.ledger.public_record(claim).expect("a program claim's public record");
                assert_eq!(unhex(&read.public_record), record.to_bytes(), "op 210 and op 211 read one claim");
                match fresh.check(*claim, da, &self.model) {
                    OutsiderFindingV1::Prosecute(proof) => KernelGoalV1::Prove(proof),
                    OutsiderFindingV1::Demand(positions) => KernelGoalV1::Demand(positions),
                    OutsiderFindingV1::Clean => panic!("a lie"),
                }
            }
            _ => KernelGoalV1::Demand(vec![(0, 0)]),
        };
        (
            state,
            KernelPursuitV1 {
                claim: *claim,
                bond: node.bond(self.card),
                goal,
                synced_collateral: Some(super::recovery::synced_collateral(node, self.card)),
            },
        )
    }

    fn filing(&self, node: &Net, pursuit: &KernelPursuitV1) -> Option<(Obj, usize)> {
        let KernelGoalV1::Prove(proof) = &pursuit.goal else { return None };
        let o = det_route(node, self.card, &K::FileProof { accuser: pursuit.accuser(), claim: pursuit.claim, proof: proof.clone() });
        let len = borsh::to_vec(&o).unwrap().len();
        Some((o, if self.chunks > 1 { len.div_ceil(self.chunks) } else { 50_000 }))
    }

    async fn plan(&self, client: &KaspaRpcClient, node: &Net, claim: &Digest, da: &Da) -> KernelPursuitStepV1 {
        let (state, pursuit) = self.derive(client, node, claim, da).await;
        let filing = self.filing(node, &pursuit);
        let f = filing.as_ref().map(|(o, cap)| KernelFilingV1 { signed: o, chunk_cap: *cap });
        plan_kernel_pursuit_v1(&state, &pursuit, f.as_ref()).expect("the plan reads the rows")
    }

    /// One tick over the socket: plan from socket reads; each object rides a carrier submitted over the socket, which the node's
    /// template carries (one a block: a carrier spends the previous one's change); then one block folds them.
    async fn tick(
        &self,
        client: &KaspaRpcClient,
        sock: &SocketRpc,
        node: &mut Net,
        claim: &Digest,
        da: &Da,
        limit: usize,
    ) -> KernelPursuitStepV1 {
        let (state, pursuit) = self.derive(client, node, claim, da).await;
        let filing = self.filing(node, &pursuit);
        let f = filing.as_ref().map(|(o, cap)| KernelFilingV1 { signed: o, chunk_cap: *cap });
        let step = plan_kernel_pursuit_v1(&state, &pursuit, f.as_ref()).expect("the plan reads the rows");
        let card = self.card;
        let objects: Vec<Obj> = match &step {
            KernelPursuitStepV1::Seal { seal, .. } => {
                vec![det_route(node, card, &K::SealProof { accuser: pursuit.accuser(), claim: *claim, seal: *seal })]
            }
            KernelPursuitStepV1::FileWhole { .. } => vec![filing.unwrap().0],
            KernelPursuitStepV1::FileChunks { missing, .. } => {
                let (o, cap) = filing.unwrap();
                palw_kernel_chunks_v1(&o, node.bond(card), PalwKernelChunkTargetV1::Claim(*claim), cap)
                    .unwrap()
                    .into_iter()
                    .filter(|c| missing.contains(&c.index))
                    .take(limit)
                    .map(|c| det_chunk(node, card, c))
                    .collect()
            }
            KernelPursuitStepV1::Demand { stage, position, .. } => vec![det_route(
                node,
                card,
                &K::FileDemand { demander: pursuit.accuser(), claim: *claim, stage: *stage, position: *position },
            )],
            _ => Vec::new(),
        };
        let ttpb = node.ttpb();
        for o in &objects {
            let tx = node.carrier(card, o);
            let id = client.submit_transaction(RpcTransaction::from(&tx), false).await.expect("submitTransaction over the socket");
            assert_eq!(id, tx.id());
            let carried = sock.node.take_pending();
            assert_eq!(carried.len(), 1, "the node's mempool took the carrier");
            let block = node.chain.heartbeat(ttpb, carried).await;
            assert!(block.transactions.iter().any(|t| t.id() == tx.id()), "its template carries it");
        }
        if !objects.is_empty() || matches!(step, KernelPursuitStepV1::AwaitSeal { .. } | KernelPursuitStepV1::AwaitAnswer { .. }) {
            node.chain.heartbeat(ttpb, Vec::new()).await;
        }
        step
    }
}

/// **K2-TIR-v2, Panel-licensed with every seat colluding; the verifier's every read and carrier crosses a real socket**: it seals over
/// the borsh wRPC proxy; the proxy is stopped and restarted on its port (the connection drops); a new client sends the first chunk of
/// a three-chunk filing; then the node behind the socket is REPLACED by a node that synced every block from genesis (the old node
/// gone, its mempool with it) and the proxy restarted again; a new client reads the half-built group and finishes it. Op 202 proves
/// the verifier's bond against the header, op 231 answers for the class, op 210 serves the conviction, op 212 lists no Final; a JSON
/// proxy and a gRPC client read the same documents. One conviction, every chunk once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14r_kernel_v2_the_pursuit_over_a_real_socket_survives_an_rpc_disconnect_and_a_node_swap() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let (lie, card) = covered_lie(&mut w).await;
    let da = lie.published(&w.fx, &[]);
    let sp = SocketPursuer { card, model: w.fx.params.clone(), chunks: 3 };
    let mut sock = SocketRpc::start(w.net.chain.ctx.consensus.consensus_clone(), WrpcEncoding::Borsh).await;
    let client = sock.client().await;

    // Op 202: the verifier's bond, proven against the header that commits it (the caller recomputes; here: served and well-formed).
    let proof = client
        .get_palw_state_proof(GetPalwStateProofRequest { block_hash: String::new(), collection: "bonds".into() })
        .await
        .expect("op 202 over the socket");
    assert!(proof.available && proof.header.is_some() && !proof.rows.is_empty(), "op 202: {}", proof.reason);
    // Op 231 answers for the class (a Panel-route class with no onboarding record: served, not found).
    let class = Hash64::from_bytes(w.class);
    let conformance = client
        .get_palw_conformance_evidence(GetPalwConformanceEvidenceRequest { class_id: class.to_string() })
        .await
        .expect("op 231 over the socket");
    assert_eq!(conformance.class_id, class.to_string());

    // Seal over the socket.
    let step = sp.tick(&client, &sock, &mut w.net, &lie.id, &da, usize::MAX).await;
    assert!(matches!(step, KernelPursuitStepV1::Seal { .. }), "{step:?}");
    // The seal's precedence delay passes (the plan says when).
    while matches!(sp.plan(&client, &w.net, &lie.id, &da).await, KernelPursuitStepV1::AwaitSeal { .. }) {
        sp.tick(&client, &sock, &mut w.net, &lie.id, &da, usize::MAX).await;
    }

    // ---- the RPC connection drops (the proxy stops and comes back on its port) ----
    sock.restart_proxy().await;
    drop(client);
    let client = sock.client().await;
    let step = sp.tick(&client, &sock, &mut w.net, &lie.id, &da, 1).await;
    assert!(matches!(&step, KernelPursuitStepV1::FileChunks { missing, .. } if missing == &vec![0, 1, 2]), "no re-seal: {step:?}");

    // ---- the node behind the socket is replaced by an IBD node; the proxy restarts again ----
    let mut node = ibd_node(&w.net).await;
    sock.node.serve(node.chain.ctx.consensus.consensus_clone());
    sock.restart_proxy().await;
    drop(client);
    let client = sock.client().await;
    let step = sp.plan(&client, &node, &lie.id, &da).await;
    assert!(matches!(&step, KernelPursuitStepV1::FileChunks { missing, .. } if missing == &vec![1, 2]), "{step:?}");
    let mut end = None;
    for _ in 0..6 {
        let step = sp.tick(&client, &sock, &mut node, &lie.id, &da, usize::MAX).await;
        if matches!(step, KernelPursuitStepV1::Done(_)) {
            end = Some(step);
            break;
        }
    }
    assert_eq!(end, Some(KernelPursuitStepV1::Done(KernelPursuitOutcomeV1::Convicted)));
    // The filing's group, read off the chain's own chunk carriers.
    let group = super::recovery::carried_objects(&node)
        .into_iter()
        .find_map(|o| match o {
            Obj::KernelRouteChunkV1 { chunk, .. } if chunk.opener == node.bond(card) => Some(chunk.group),
            _ => None,
        })
        .expect("the filing rode the chunk lane");
    assert_convicted_once(&node, card, &lie.id, group, 3);

    // Op 210 serves the conviction; op 212 lists no Final for the claim.
    let read = client
        .get_palw_kernel_claim(GetPalwKernelClaimRequest { claim_id: Hash64::from_bytes(lie.id).to_string() })
        .await
        .expect("op 210");
    assert!(read.convicted && read.state.starts_with("Convicted"), "{}", read.state);
    let finals = client.get_palw_kernel_finals(GetPalwKernelFinalsRequest { limit: 1024 }).await.expect("op 212");
    assert!(finals.finals.iter().all(|f| unhex(&f.claim_id) != lie.id.to_vec()), "no Final for a convicted claim");

    // The same documents through a JSON proxy and a gRPC client.
    let json = SocketRpc::start(node.chain.ctx.consensus.consensus_clone(), WrpcEncoding::SerdeJson).await;
    let json_client = json.client().await;
    let json_read = json_client
        .get_palw_kernel_claim(GetPalwKernelClaimRequest { claim_id: Hash64::from_bytes(lie.id).to_string() })
        .await
        .expect("op 210 over JSON");
    assert_eq!(json_read, read, "borsh and JSON frame one document");
    let grpc = json.grpc_client().await;
    let grpc_read = grpc
        .get_palw_kernel_claim(GetPalwKernelClaimRequest { claim_id: Hash64::from_bytes(lie.id).to_string() })
        .await
        .expect("op 210 over gRPC");
    assert_eq!(grpc_read, read, "and the protowire too");
    let _ = grpc.disconnect().await;
    let _ = json_client.disconnect().await;
    json.stop().await;
    let _ = client.disconnect().await;
    sock.stop().await;

    sync(&node, &mut w.net).await;
    agree(&w.net, &node, "the node the socket fronted at the end");
}
