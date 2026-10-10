//! **The canonical G14 node harness (lane G14C, milestone 2: GAP-01 to GAP-04 of `docs/design/palw/g14-completion-matrix.md`).**
//!
//! The property (RFC-0015 §1.1): the producer and EVERY Panel seat collude; ONE public bonded verifier outside the Panel carries the
//! case to an objective outcome — a conviction before Final, a conviction after Final inside the liability horizon, the correct DA
//! default for withheld material, or the dismissal of a wrong challenge against an honest claim — using only public, authenticated
//! material. The class is the K2 single-program class of the parent module (`wide128_v1(7)`, K2-TIR-v2), under both modes:
//! Panel-licensed (every interim seat signs a passing receipt) and RFC-0015 OptimisticPublicVerification (no Panel at all).
//!
//! What this module adds over the parent's cases, one gap each:
//!
//! * **GAP-01, ordinary public entry.** The prosecutor is NOT one of testnet-12's eight genesis cards. It is a public participant
//!   ([`NEWCOMER`], registry row 8, which no card uses) whose bond is registered after the claim, through the published door: a real
//!   `0x4b` `BondRegistered` carrier that locks the registration collateral, signed by the bond key and by the operator key, taken by
//!   its own node's mempool and carried by its own node's template. Its coins are testnet-12's main premine output re-addressed to its
//!   key — funds, not a bond.
//! * **GAP-02, a fresh node.** The prosecutor runs a node STARTED AFTER THE CLAIM: [`ibd_node`] (every block from genesis, as a syncing
//!   node takes them) or [`pruned_import_node`] (the PALW state carried at a pruning point after the claim, then the blocks after it).
//!   Its verifier is built from THAT node's reads; its registration, demand and proof go through THAT node's mempool and template, and
//!   the blocks it mines reach the producer's node as a peer's blocks do. The two nodes end on one sink and one PALW root.
//! * **GAP-03, reads through RPC.** Every read the verifier makes is an RPC op served by its node: op 211 (`getPalwKernelRows`, paged)
//!   rebuilds the route and its ledger, op 210 (`getPalwKernelClaim`) the claim, op 212 (`getPalwKernelFinals`) the Finals, op 231
//!   (`getPalwConformanceEvidence`) the onboarding record. [`Rpc`] calls the ops' own request parsers and response builders
//!   (`kaspa_rpc_core::convert::palw_kernel`, the functions the RPC service's handlers call and nothing else) against the node, and
//!   carries every request and answer through the RPC's JSON wire form. **What the harness cannot do** is open the node's RPC socket:
//!   that needs a running kaspad with the kernel route armed, which `validate_palw_v2` refuses at every height by design (so does every
//!   drill flag). That last hop is a drill's (EXTERNAL), not this harness's.
//! * **GAP-04, ADR-0177 non-interference.** A model peer that refuses to serve the registered weights changes no verdict, reward,
//!   weight or Final. A verifier whose every peer refuses cannot check and files nothing — there is no object for "unavailable", so
//!   the chain has no input to react with — while a verifier that acquired the weights from another peer (authenticated against the
//!   registered commitments; a peer serving other weights is refused) convicts exactly as before. Every node ends on one root.
//!
//! **The conviction path's branch-specific parts are behind [`Rules`]** (the filing objects and what each outcome pays), now
//! G14R's rules (accuser seal-then-reveal, the pre-Final default that keeps the reservation liable, the escrow-funded Final reward).
//!
//! The fences are armed WITHOUT their validation, exactly as in the parent module (its doc): nothing here can run on a network.
use super::*;
use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, premine_outpoint_for};
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::palw_kernel_route_v1::{PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, PalwKernelRouteHeaderV1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, palw_bond_registration_message_v2,
    palw_operator_possession_message_v1,
};
use kaspa_consensus_core::tx::TransactionId;
use kaspa_rpc_core::convert::palw_kernel::{
    palw_conformance_evidence_request_v1, palw_conformance_evidence_response_v1, palw_kernel_claim_request_v1,
    palw_kernel_claim_response_v1, palw_kernel_finals_request_v1, palw_kernel_finals_response_v1, palw_kernel_rows_request_v1,
    palw_kernel_rows_response_v1,
};
use kaspa_rpc_core::{
    GetPalwConformanceEvidenceRequest, GetPalwConformanceEvidenceResponse, GetPalwKernelClaimRequest, GetPalwKernelClaimResponse,
    GetPalwKernelFinalsRequest, GetPalwKernelFinalsResponse, GetPalwKernelRowsRequest,
};
use misaka_palw_kernel::job::DecodeFaultV1;
use misaka_palw_kernel::ledger::LedgerPolicyV1;
use misaka_palw_kernel::ledger::proof_seal_v1;
use misaka_palw_kernel::opv::OpvPolicyV1;
use misaka_palw_kernel::rows::LedgerRowsV1;

/// The public participant: registry row 8, the first harness key no genesis card holds.
const NEWCOMER: usize = 8;
/// The registration carrier's collateral output (output 0 is the change).
const COLLATERAL_INDEX: u32 = 1;
/// What the registration carrier pays (10 BILI).
const REGISTRATION_FEE: u64 = 1_000_000_000;
/// The page budget the verifier asks op 211 for: small, so a route of a few claims spans several pages.
const ROWS_PAGE_BYTES: u32 = 16 << 10;

// ---- the network: testnet-12 with harness cards, the kernel route armed, and a funded public participant ----------------------

/// [`kernel_config_with`]'s network, with testnet-12's main premine output re-addressed to the [`NEWCOMER`]'s key (the genesis UTXO
/// commitment and hash recomputed from the edited set, as `t12_seat_maturity_fence` does). `opv` arms RFC-0015's fence with the
/// interim terms and the fixture's class admitted.
fn canonical_config(opv: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, mut premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    let (_, entry) = premine.iter_mut().find(|(o, _)| *o == main).expect("testnet-12 mints a main premine output");
    entry.script_public_key = card_payout_spk(NEWCOMER);
    assert!(entry.amount > 2 * PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, "the main output funds a registration");
    let mut multiset = kaspa_muhash::MuHash::new();
    for (outpoint, entry) in &premine {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    // OPV eligibility is DERIVED (OPVB): the pre-derivation fixture class is named through the processor's test seam, as the parent's
    // OPV worlds do (`kernel_config_opv`) — the integration head's harness still relied on the retired admission list.
    if opv {
        opv_test_eligible(&opv_admitted());
    }
    params.palw_panel_free_v1 =
        if opv { Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new())) } else { None };
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    params.skip_proof_of_work = true;
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fence; only this harness bypasses it");
    (Config::new(params), bundle, premine, floats)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// Panel-licensed: an interim Panel is assigned and EVERY seat signs a passing receipt.
    Panel,
    /// RFC-0015 OptimisticPublicVerification: no Panel.
    Opv,
}

/// The world: the class registered by card 1 on the canonical network of `mode`.
async fn world(mode: Mode) -> World {
    match mode {
        Mode::Panel => World::on(Net::over_cfg(canonical_config(false), TestConsensus::new)).await,
        Mode::Opv => World::on_opv(Net::over_cfg(canonical_config(true), TestConsensus::new)).await,
    }
}

/// The participant's coins: the re-addressed main premine output.
fn newcomer_funding(net: &Net) -> (TransactionOutpoint, UtxoEntry) {
    let main = premine_outpoint_for(net.config.params.net, MAIN_PREMINE_INDEX);
    let (outpoint, entry) = net.premine.iter().find(|(o, _)| *o == main).expect("the main premine output").clone();
    assert_eq!(entry.script_public_key, card_payout_spk(NEWCOMER), "re-addressed to the participant");
    (outpoint, entry)
}

// ---- GAP-02: a node started after the claim ---------------------------------------------------------------------------------

/// **A node started now that syncs by IBD**: every block of `main`'s selected chain from genesis, as a peer serves them. It carries
/// `main`'s actors (keys, funding) and continues `main`'s clock.
pub(super) async fn ibd_node(main: &Net) -> Net {
    let chain = main.replay().await;
    let mut node = main.on_chain(chain);
    node.chain.ctx.simulated_time = main.chain.ctx.simulated_time;
    agree(main, &node, "the IBD node");
    node
}

/// **A node started now that syncs from a pruning point**: `main` mines one block `T` past its sink `P`; the node takes the blocks
/// through `P`, discards its own PALW tip and deltas through `P`, installs the PALW state `main` serves for `P` against `T`'s
/// committed root (tail `0xEC` carries the route), and then takes the blocks after `P` (the parent's `..._survives_a_pruned_import`,
/// here a node that then prosecutes).
async fn pruned_import_node(main: &mut Net) -> Net {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let p = main.chain.sink();
    let ttpb = main.ttpb();
    main.chain.heartbeat(ttpb, Vec::new()).await;
    let all = chain_blocks(&main.chain, main.chain.sink());
    let k = all.iter().position(|b| b.header.hash == p).expect("P is on the chain");
    let t = all[k + 1].clone();
    let importer = t12_genesis_chain(&main.config, &main.bundle, &main.premine, &main.floats);
    for b in &all[..=k] {
        arrive(&importer, b.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;
    let vp = main.chain.vp();
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
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs against T's committed root");
    assert_eq!(importer.tip_state().1.state_root(), root_at(&main.chain, p), "the imported state is P's, root for root");
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "a block after P").await;
    }
    let mut node = main.on_chain(importer);
    node.chain.ctx.simulated_time = main.chain.ctx.simulated_time;
    agree(main, &node, "the pruned-import node");
    node
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    Ibd,
    PrunedImport,
}

async fn start_node(main: &mut Net, how: Start) -> Net {
    match how {
        Start::Ibd => ibd_node(main).await,
        Start::PrunedImport => pruned_import_node(main).await,
    }
}

/// `from`'s blocks that `to` lacks arrive at `to` (a peer relaying its chain); `to`'s clock continues from the later of the two.
async fn sync(from: &Net, to: &mut Net) {
    for b in chain_blocks(&from.chain, from.chain.sink()) {
        if to.chain.ctx.consensus.get_block(b.header.hash).is_err() {
            arrive(&to.chain, b, "a peer's block").await;
        }
    }
    assert_eq!(to.chain.sink(), from.chain.sink(), "one sink");
    to.chain.ctx.simulated_time = to.chain.ctx.simulated_time.max(from.chain.ctx.simulated_time);
}

/// **Two nodes at one tip agree**: the sink, the PALW state root, the kernel route's rows (as served), and the delta root of every
/// chain block both hold a delta for (a pruned-import node has none at or below its pruning point, by construction).
fn agree(main: &Net, node: &Net, what: &str) {
    assert_eq!(node.chain.sink(), main.chain.sink(), "{what}: same sink");
    assert_eq!(node.chain.tip_state().1.state_root(), main.chain.tip_state().1.state_root(), "{what}: same PALW state root");
    assert_eq!(node.chain.ctx.consensus.palw_kernel_route_v1(), main.api(), "{what}: same kernel route rows");
    let (mut compared, store) = (0usize, node.chain.vp().palw_state_v2_store.clone());
    for b in chain_blocks(&main.chain, main.chain.sink()) {
        if let Ok(root) = store.read().state_root_of(b.header.hash) {
            assert_eq!(root, root_at(&main.chain, b.header.hash), "{what}: delta root of {}", b.header.hash);
            compared += 1;
        }
    }
    assert!(compared > 0, "{what}: some delta compared");
}

// ---- GAP-01: a bond registered after genesis, through the published door ----------------------------------------------------

/// **The participant's registration, from its own node**: the collateral at output 1 paid to the payout the registration names, the
/// bond named by index with a zero id, the bond key's signature over the registration and the operator key's over possession (one key
/// for both), under this chain's domain; change at output 0. Taken by `node`'s mempool, carried by `node`'s template, folded by the
/// next block. Returns the bond, which is then this network's actor [`NEWCOMER`] on `node` (its key and its change output).
async fn register_newcomer(node: &mut Net, funding: (TransactionOutpoint, UtxoEntry)) -> PalwBondKeyV2 {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let key = TestConsensus::palw_v2_registry_keypair(NEWCOMER as u64);
    let pubkey = key.verification_key.as_ref().to_vec();
    let payout = Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes());
    let signed = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), COLLATERAL_INDEX));
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let classes = std::collections::BTreeSet::from([node.bundle.base_class_id]);
    let sign = |message: &[u8], context: &[u8]| {
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x6Eu8; 32]).expect("sign").as_ref().to_vec()
    };
    let message = palw_bond_registration_message_v2(node.domain, &signed, &pubkey, &pubkey, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(node.domain, &signed, &pubkey, &pubkey);
    let mut signature = sign(message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(sign(possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let object = Obj::BondRegistered {
        bond: signed,
        pubkey: pubkey.clone(),
        operator_pubkey: pubkey,
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    let (outpoint, entry) = funding;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![
            TransactionOutput::new(entry.amount - collateral - REGISTRATION_FEE, card_payout_spk(NEWCOMER)),
            TransactionOutput::new(collateral, p2pkh_mldsa87_spk(payout.as_byte_slice())),
        ],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, NEWCOMER, node.config.params.storage_mass_parameter);
    node.mempool(&tx).unwrap_or_else(|e| panic!("the participant's own node takes its registration: {e}"));
    let ttpb = node.ttpb();
    let carrying = node.chain.heartbeat(ttpb, vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "its node's template carries the registration");
    node.chain.heartbeat(ttpb, Vec::new()).await;
    let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), COLLATERAL_INDEX));
    let record = node.chain.tip_state().1.bond(&bond).cloned().expect("the chain registered the participant's bond");
    assert_eq!(record.collateral, collateral, "the published registration collateral, locked by the carrier");
    assert!(record.registered_daa > 0, "registered after genesis");
    assert!(!node.chain.bonds.contains(&bond), "not a genesis card");
    assert_eq!((node.chain.bonds.len(), node.funding.len()), (NEWCOMER, NEWCOMER), "the eight cards, then the participant");
    node.chain.bonds.push(bond);
    node.funding
        .push((TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, card_payout_spk(NEWCOMER), 0, false)));
    bond
}

/// The participant's bond, known to `net` too (for the reads that name actors by index); its funding stays its own node's.
fn know_newcomer(net: &mut Net, bond: PalwBondKeyV2) {
    assert_eq!(net.chain.bonds.len(), NEWCOMER);
    net.chain.bonds.push(bond);
}

// ---- GAP-03: the reads, through the RPC ops ------------------------------------------------------------------------------------

fn unhex(text: &str) -> Vec<u8> {
    let mut out = vec![0u8; text.len() / 2];
    faster_hex::hex_decode(text.as_bytes(), &mut out).expect("the RPC serves hex");
    out
}

/// **An RPC client of one node**: each call is the op's own request parser and response builder (`kaspa_rpc_core::convert::
/// palw_kernel`, what the RPC service's handler runs) against that node, with the request and the answer carried through the
/// RPC's JSON wire form (module doc for the one hop it cannot take).
pub(super) struct Rpc {
    node: std::sync::Arc<crate::consensus::Consensus>,
}

impl Rpc {
    pub(super) fn of(chain: &T12Chain) -> Rpc {
        Rpc { node: chain.ctx.consensus.consensus_clone() }
    }

    fn api(&self) -> &dyn ConsensusApi {
        self.node.as_ref()
    }

    /// Through the wire: serialized as the JSON RPC sends it, and deserialized as a client reads it.
    fn wire<T: serde::Serialize + serde::de::DeserializeOwned>(value: T) -> T {
        serde_json::from_str(&serde_json::to_string(&value).expect("serializes")).expect("deserializes")
    }

    /// Op 210 `getPalwKernelClaim`.
    pub(super) fn claim(&self, claim: &Digest) -> GetPalwKernelClaimResponse {
        let request = Self::wire(GetPalwKernelClaimRequest { claim_id: Hash64::from_bytes(*claim).to_string() });
        let id = palw_kernel_claim_request_v1(&request).expect("a well-formed claim id");
        Self::wire(palw_kernel_claim_response_v1(self.api(), id).expect("op 210 answers"))
    }

    /// Op 212 `getPalwKernelFinals`, every Final the route holds (newest first, as served).
    pub(super) fn finals(&self) -> GetPalwKernelFinalsResponse {
        let request = Self::wire(GetPalwKernelFinalsRequest { limit: 1024 });
        let limit = palw_kernel_finals_request_v1(&request);
        let page = Self::wire(palw_kernel_finals_response_v1(self.api(), limit).expect("op 212 answers"));
        assert_eq!(page.total as usize, page.finals.len(), "one page holds every Final here");
        page
    }

    /// Op 231 `getPalwConformanceEvidence`.
    pub(super) fn conformance(&self, class: Hash64) -> GetPalwConformanceEvidenceResponse {
        let request = Self::wire(GetPalwConformanceEvidenceRequest { class_id: class.to_string() });
        let class = palw_conformance_evidence_request_v1(&request).expect("a well-formed class id");
        Self::wire(palw_conformance_evidence_response_v1(self.api(), class).expect("op 231 answers"))
    }

    /// **Op 211 `getPalwKernelRows`, paged to the end**: the route's header and every row, reassembled into the route state and checked
    /// against the ledger and aux roots the op serves (every page must report one snapshot). Returns the state and the served ledger
    /// root.
    pub(super) fn route(&self) -> (PalwKernelRouteStateV1, Hash64) {
        let mut request =
            GetPalwKernelRowsRequest { has_cursor: false, after_table: 0, after_key: String::new(), max_bytes: ROWS_PAGE_BYTES };
        let mut header: Option<PalwKernelRouteHeaderV1> = None;
        let mut roots: Option<(String, String)> = None;
        let (mut rows, mut aux) = (LedgerRowsV1::new(), BTreeMap::new());
        let (mut seen, mut total, mut pages) = (0u64, None, 0u32);
        loop {
            let request_wire = Self::wire(request.clone());
            let (after, max_bytes) = palw_kernel_rows_request_v1(&request_wire).expect("a well-formed cursor");
            let page = Self::wire(palw_kernel_rows_response_v1(self.api(), after, max_bytes).expect("op 211 answers"));
            pages += 1;
            assert!(page.available, "the node serves the kernel route");
            let h: PalwKernelRouteHeaderV1 = borsh::from_slice(&unhex(&page.header)).expect("the served header decodes");
            match &header {
                Some(prev) => assert_eq!(prev, &h, "one snapshot across pages"),
                None => header = Some(h),
            }
            let r = (page.ledger_root.clone(), page.aux_root.clone());
            match &roots {
                Some(prev) => assert_eq!(prev, &r, "one snapshot across pages"),
                None => roots = Some(r),
            }
            total = Some(page.total_rows);
            for row in &page.rows {
                seen += 1;
                let table = u8::try_from(row.table).expect("a table fits a u8");
                let key = (table, unhex(&row.key));
                if table < PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1 {
                    rows.insert(key, unhex(&row.row));
                } else {
                    aux.insert(key, unhex(&row.row));
                }
            }
            if !page.more {
                break;
            }
            request = GetPalwKernelRowsRequest {
                has_cursor: true,
                after_table: page.next_table,
                after_key: page.next_key.clone(),
                max_bytes: ROWS_PAGE_BYTES,
            };
        }
        assert_eq!(Some(seen), total, "every row exactly once, over {pages} page(s)");
        let state = PalwKernelRouteStateV1 { header: header.expect("a first page"), rows, aux, ledger_cache: Default::default() };
        let (ledger_root, aux_root) = roots.expect("a first page");
        assert_eq!(state.ledger_root().to_string(), ledger_root, "the served rows root to the served ledger root");
        assert_eq!(state.aux_root().to_string(), aux_root, "and the served aux rows to the served aux root");
        (state, ledger_root.parse().expect("a Hash64"))
    }

    /// **The fresh verifier, built from op 211 alone** (the parent's [`Fresh`], over the rows the RPC served), with its own salt; and
    /// cross-checked: op 210's public record of `claim` is the rebuilt ledger's.
    pub(super) fn verifier(&self, claim: &Digest, salt: u8) -> Fresh {
        let (state, root) = self.route();
        let fresh = Fresh::from_api(&state, root, salt);
        assert!(fresh.ledger.claims.contains_key(claim), "the claim is discovered in the served rows");
        let read = self.claim(claim);
        assert!(read.available && read.found && read.kind == "program", "op 210 serves the claim: {read:?}");
        let (record, _) = fresh.ledger.public_record(claim).expect("a program claim's public record");
        assert_eq!(unhex(&read.public_record), record.to_bytes(), "op 210's public record is the rebuilt ledger's");
        assert_eq!(read.ledger_root, root.to_string(), "op 210 and op 211 read one state");
        fresh
    }
}

// ---- the model: acquired off-chain from peers, authenticated against the registered commitments (ADR-0177) ---------------------

/// A model peer: off-chain, optional, never a consensus input.
#[derive(Clone)]
enum ModelPeer {
    /// Serves the weights it holds.
    Serves(MapParams),
    /// Refuses (a closed model, or a peer that will not share it).
    Refuses,
}

/// **Acquire the registered model**: the first peer whose weights commit to the class's registered `ParamCommitmentsV1` root. A peer
/// serving other weights is refused here, exactly as the court would refuse their openings; a refusing peer is skipped.
fn acquire(peers: &[ModelPeer], registered: &Digest) -> Option<MapParams> {
    peers.iter().find_map(|peer| match peer {
        ModelPeer::Serves(weights) if ParamCommitmentsV1::of(weights).root() == *registered => Some(weights.clone()),
        ModelPeer::Serves(_) | ModelPeer::Refuses => None,
    })
}

/// The registered commitments root of `claim`'s class, read from the verifier's rebuilt ledger.
fn registered_root(fresh: &Fresh, claim: &Digest) -> Digest {
    let row = &fresh.ledger.claims[claim];
    fresh.ledger.classes[&row.class_binding_id].param_commitments.root()
}

// ---- the conviction path on this branch (the adapter G14R's merge replaces) -----------------------------------------------------

/// **The conviction path's specifics on the integration line since `86ade72ba` (G14R's `g14/r4-fixes` merged)**: what a filing is,
/// and what each outcome pays. An accuser seals its proof first (GAP-R7: the bounty goes to the earliest seal of the convicting bytes,
/// so a copyist lifting the public carrier pays the sealer); a pre-Final default takes the penalty split like a slash and keeps the
/// rest of the reservation liable through `default + liability_daa` (F-C4R3-02); the Final reward is paid once out of the poster's
/// escrow (GAP-5). Everything else in this module is branch-independent.
struct Rules;

impl Rules {
    /// File `proof` against `claim` from `node`, signed by actor `card`: its seal, then (one seal delay later) the `FileProof`, each
    /// through `node`'s mempool and template.
    async fn file_proof(node: &mut Net, card: usize, claim: Digest, proof: ProsecutionV1) {
        let accuser = node.kid(card);
        let seal = node.route(card, &K::SealProof { accuser, claim, seal: proof_seal_v1(&claim, &accuser, &proof) });
        node.send(vec![(card, seal)]).await;
        let o = node.route(card, &K::FileProof { accuser, claim, proof });
        // A filing larger than one carrier rides the route's own chunk lane (tag 113, F-C4R3-03), signed per chunk by the filer.
        match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, 50_000).expect("chunks") {
            None => {
                node.send(vec![(card, o)]).await;
            }
            Some(_) => {
                let chunks = node.kernel_chunks(card, &o, PalwKernelChunkTargetV1::Claim(claim), 50_000);
                node.send(chunks.into_iter().map(|c| (card, c)).collect()).await;
            }
        }
    }

    /// Demand `position` of `claim` from `node`, signed by actor `card`.
    async fn file_demand(node: &mut Net, card: usize, claim: Digest, position: u32) {
        let o = node.route(card, &K::FileDemand { demander: node.kid(card), claim, stage: 0, position });
        node.send(vec![(card, o)]).await;
    }

    /// What admitting the producer's claim costs it outright (F-C4R3-05: an OPV admission fee, burned whatever becomes of the claim;
    /// a Panel-licensed claim pays none).
    fn admission_cost(w: &World, mode: Mode) -> u64 {
        match mode {
            Mode::Panel => 0,
            Mode::Opv => w.net.api().unwrap().header.opv.expect("an OPV network").economics.admission_fee,
        }
    }

    /// The accuser's share of a conviction that slashed `slashed`.
    fn bounty(policy: &LedgerPolicyV1, slashed: u64) -> u64 {
        slashed * u64::from(policy.accuser_reward_permille) / 1000
    }

    /// A sole demander's share of the producer's default (the parent's `default_share`: split like a slash, an OPV claim's capped).
    fn default_share(policy: &LedgerPolicyV1, opv: Option<&OpvPolicyV1>) -> u64 {
        default_share(policy, opv)
    }

    /// What stays reserved on the producer after a pre-Final default: the rest of the reservation, liable until the horizon.
    fn reserved_after_default(policy: &LedgerPolicyV1, reservation: u64) -> u128 {
        u128::from(reservation - policy.default_penalty)
    }

    /// What the coinbase queue owes the producer at its claim's Final: the job's escrow (the route's reward), paid once.
    fn final_reward(policy: &LedgerPolicyV1) -> u64 {
        policy.claim_reward
    }
}

// ---- the scenario -----------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    ConvictedBeforeFinal,
    ConvictedAfterFinal,
    DefaultOnWithheldMaterial,
    WrongChallengeDismissed,
}

/// The claim's Final DAA by its mode's clock (Panel: the window the colluding Panel's pass opened; OPV: the window from admission).
fn final_floor(w: &World, mode: Mode, claim: &Digest) -> u64 {
    match mode {
        Mode::Panel => {
            let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(claim) else {
                panic!("the colluding Panel passed it: {:?}", w.net.claim_state(claim))
            };
            window_end_daa
        }
        Mode::Opv => opv_view(w, claim).1.final_floor_daa,
    }
}

/// The latest Final the claim's clock allows with nothing pending (Panel: the window's end; OPV: the hard deadline).
fn final_ceiling(w: &World, mode: Mode, claim: &Digest) -> u64 {
    match mode {
        Mode::Panel => final_floor(w, mode, claim),
        Mode::Opv => opv_view(w, claim).1.hard_deadline_daa,
    }
}

/// The reservation a conviction slashes.
fn reservation(w: &World, mode: Mode) -> u64 {
    match mode {
        Mode::Panel => w.policy().claim_collateral,
        Mode::Opv => w.net.api().unwrap().header.opv.expect("an OPV network").economics.reservation_per_claim,
    }
}

/// **The producer's claim, made and (Panel) covered by EVERY seat**: returns the seats, none of which is the producer.
async fn claimed(w: &mut World, mode: Mode, producer: usize, lie: bool) -> (Claim, Vec<usize>) {
    let job = w.job().await;
    let claim = w.claim(producer, &job, lie).await;
    let seats = match mode {
        Mode::Panel => {
            let seats = w.seats(&claim.id);
            assert_eq!(seats.len(), 3, "three distinct-operator seats");
            assert!(!seats.contains(&producer), "never the producer");
            w.cover(&claim.id).await; // every assigned seat signs a passing receipt
            seats
        }
        Mode::Opv => {
            let route = w.net.api().unwrap();
            assert!(route.assignment_of(&claim.id).is_none() && route.receipts_of(&claim.id).is_empty(), "OPV: no Panel");
            Vec::new()
        }
    };
    (claim, seats)
}

/// **One canonical G14 case** (module doc): `mode` × how the participant's node starts × the outcome the participant reaches.
async fn canonical(mode: Mode, start: Start, outcome: Outcome) {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let before = w.net.collateral(0) - Rules::admission_cost(&w, mode);
    let lie = outcome != Outcome::WrongChallengeDismissed;
    let (claim, seats) = claimed(&mut w, mode, 0, lie).await;
    let floor = final_floor(&w, mode, &claim.id);
    let ceiling = final_ceiling(&w, mode, &claim.id);
    let slashed = reservation(&w, mode);
    let policy = w.policy();

    if outcome == Outcome::ConvictedAfterFinal {
        // Nobody prosecutes in the window: the lie finalizes and the producer is paid.
        w.net.beat_to(floor).await;
        assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
        assert_eq!(w.net.owed(0), Rules::final_reward(&policy), "the Final reward is queued for the producer");
    }

    // ---- the participant: a node started after the claim, a bond registered after it ----
    let mut node = start_node(&mut w.net, start).await;
    let bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut w.net).await;
    know_newcomer(&mut w.net, bond);
    assert!(!seats.contains(&NEWCOMER), "the participant is no seat of the claim");
    assert!(w.net.chain.tip_state().1.bond(&bond).is_some(), "the producer's node holds the participant's bond too");

    // ---- its verifier: op 211's rows, op 210's claim; the model from a peer, authenticated ----
    let rpc = Rpc::of(&node.chain);
    let fresh = rpc.verifier(&claim.id, 0x5A);
    let read = rpc.claim(&claim.id);
    match (mode, outcome) {
        (_, Outcome::ConvictedAfterFinal) => assert!(read.state.starts_with("Final"), "{}", read.state),
        (Mode::Panel, _) => assert!(read.state.starts_with("ProbabilisticPass"), "{}", read.state),
        (Mode::Opv, _) => assert!(read.state.starts_with("Challengeable") && read.seats.is_empty(), "{}", read.state),
    }
    let model =
        acquire(&[ModelPeer::Serves(w.fx.params.clone())], &registered_root(&fresh, &claim.id)).expect("a peer serves the model");

    match outcome {
        Outcome::ConvictedBeforeFinal | Outcome::ConvictedAfterFinal => {
            let da = claim.published(&w.fx, &[]);
            let OutsiderFindingV1::Prosecute(proof) = fresh.check(claim.id, &da, &model) else {
                panic!("the participant's verifier finds the lie from public material")
            };
            let seats_slashed: Vec<(usize, u64)> = seats.iter().map(|c| (*c, w.net.slashed(*c))).collect();
            Rules::file_proof(&mut node, NEWCOMER, claim.id, proof).await;
            sync(&node, &mut w.net).await;
            let ledger = w.net.ledger();
            assert!(ledger.claims[&claim.id].convicted, "the producer's node took the participant's conviction");
            assert_eq!(w.net.collateral(0), before - slashed, "the REAL producer bond lost the reservation");
            assert_eq!(w.net.kernel_reserved(0), 0);
            assert_eq!(w.net.owed(NEWCOMER), Rules::bounty(&policy, slashed), "the participant's share of the slash is queued");
            for (card, before) in &seats_slashed {
                assert_eq!(w.net.slashed(*card), *before, "the colluding seats are not charged by the producer's conviction");
            }
            let read = Rpc::of(&node.chain).claim(&claim.id);
            assert!(read.convicted, "the participant's own node serves the verdict");
            if outcome == Outcome::ConvictedBeforeFinal {
                assert!(read.state.starts_with("Convicted"), "{}", read.state);
                // Final is blocked: the window passes and the claim never finalizes, never pays.
                w.net.beat_to(floor + 2).await;
                assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Convicted { .. }), "{:?}", w.net.claim_state(&claim.id));
                assert_eq!(w.net.owed(0), 0, "a convicted claim is never paid");
                sync(&w.net, &mut node).await;
                let finals = Rpc::of(&node.chain).finals();
                assert!(finals.finals.iter().all(|f| unhex(&f.claim_id) != claim.id.to_vec()), "and op 212 lists no Final for it");
            } else {
                assert!(read.state.starts_with("Final"), "Final stays Final: the conviction is the liability's: {}", read.state);
                if mode == Mode::Opv {
                    let finals = Rpc::of(&node.chain).finals();
                    let f = finals.finals.iter().find(|f| unhex(&f.claim_id) == claim.id.to_vec()).expect("op 212 lists the Final");
                    assert_eq!(f.standing, "ConvictedAfterFinal", "op 212 withdraws the fact");
                }
            }
        }
        Outcome::DefaultOnWithheldMaterial => {
            let da = claim.published(&w.fx, &[claim.at]);
            assert_eq!(
                fresh.check(claim.id, &da, &model),
                OutsiderFindingV1::Demand(vec![(0, claim.at.0)]),
                "no pass and no conviction from what is public: one position to demand"
            );
            Rules::file_demand(&mut node, NEWCOMER, claim.id, claim.at.0).await;
            sync(&node, &mut w.net).await;
            let read = Rpc::of(&node.chain).claim(&claim.id);
            assert_eq!(read.demands.len(), 1, "op 210 serves the open demand");
            assert!(!read.state.starts_with("Final") && !read.convicted, "an open demand holds Final: {}", read.state);
            if mode == Mode::Panel {
                assert!(read.state.starts_with("Disputed"), "{}", read.state);
            }
            let deadline = read.demands[0].deadline_daa;
            // The producer stays silent.
            w.net.beat_to(deadline).await;
            assert!(
                matches!(w.net.claim_state(&claim.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
                "{:?}",
                w.net.claim_state(&claim.id)
            );
            assert!(!w.net.ledger().claims[&claim.id].convicted, "a default is not a conviction");
            assert_eq!(w.net.collateral(0), before - policy.default_penalty, "the fixed penalty, not the fraud slash");
            assert_eq!(w.net.kernel_reserved(0), Rules::reserved_after_default(&policy, slashed));
            let opv = w.net.api().unwrap().header.opv;
            assert_eq!(w.net.owed(NEWCOMER), Rules::default_share(&policy, opv.as_ref()), "the sole demander's share");
            assert_eq!(w.net.kernel_reserved(NEWCOMER), 0, "and its demand bond returns");
            sync(&w.net, &mut node).await;
            let read = Rpc::of(&node.chain).claim(&claim.id);
            assert!(read.state.starts_with("Unavailable") && !read.convicted, "op 210: {}", read.state);
        }
        Outcome::WrongChallengeDismissed => {
            let da = claim.published(&w.fx, &[]);
            assert_eq!(fresh.check(claim.id, &da, &model), OutsiderFindingV1::Clean, "an honest claim checks clean");
            // A wrong challenge: the honest decode accused (its true logits opened whole).
            let row = &fresh.ledger.claims[&claim.id];
            let misaka_palw_kernel::ledger::ClaimBodyV1::Program { claim: c, .. } = &row.body else { panic!("a program claim") };
            let job = &fresh.ledger.jobs[&row.job_id];
            let (post, logits) = fresh.ledger.classes[&row.class_binding_id].logits_at();
            let p = c.select_position(job, 0);
            let t = da.node(0, p, post, logits).expect("the published logits");
            let wrong = ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: TensorWireV1::of(&t) });
            let mine = w.net.collateral(NEWCOMER);
            Rules::file_proof(&mut node, NEWCOMER, claim.id, wrong).await;
            sync(&node, &mut w.net).await;
            assert!(!w.net.ledger().claims[&claim.id].convicted, "a wrong challenge convicts nothing");
            // F-C4R4-05: a dismissed filing pays one fee per share of the block's court work it was charged, at least one fee.
            let paid = mine - w.net.collateral(NEWCOMER);
            let (fee, most) = (policy.dismissed_proof_fee, policy.dismissed_proof_fee * u64::from(policy.max_adjudications_per_block));
            assert!(paid >= fee && paid <= most, "and costs its filer the dismissal fee: {paid} not in [{fee}, {most}]");
            assert_eq!(w.net.collateral(0), before, "the honest producer is not charged");
            w.net.beat_to(floor).await;
            let ClaimStateV1::Final { final_daa } = w.net.claim_state(&claim.id) else {
                panic!("the honest claim finalizes on its own clock: {:?}", w.net.claim_state(&claim.id))
            };
            assert!(final_daa >= floor && final_daa <= ceiling, "the dismissed challenge did not move Final: {final_daa}");
            assert_eq!(w.net.owed(0), Rules::final_reward(&policy), "and the producer is paid");
            sync(&w.net, &mut node).await;
            let finals = Rpc::of(&node.chain).finals();
            assert!(finals.finals.iter().any(|f| unhex(&f.claim_id) == claim.id.to_vec()), "op 212 lists the Final");
        }
    }

    // Both nodes agree on everything.
    let ttpb = w.net.ttpb();
    w.net.chain.heartbeat(ttpb, Vec::new()).await;
    sync(&w.net, &mut node).await;
    agree(&w.net, &node, "the participant's node");
}

// ---- the cases: each outcome in both modes, each mode over both ways a node starts ---------------------------------------------

/// **Panel-licensed, every seat colluding; a post-genesis bond on a node that synced by IBD after the claim convicts the lie before
/// Final**, through its own mempool and template, from op 211/210 reads; the lie never finalizes.
#[tokio::test]
async fn g14_canonical_panel_a_post_genesis_bond_on_an_ibd_node_convicts_a_covered_lie_before_final() {
    canonical(Mode::Panel, Start::Ibd, Outcome::ConvictedBeforeFinal).await;
}

/// Panel-licensed: the covered lie finalized and was paid; a post-genesis bond on a node started from a pruning point after the Final
/// convicts it inside the liability horizon.
#[tokio::test]
async fn g14_canonical_panel_a_post_genesis_bond_on_a_pruned_import_node_convicts_a_covered_lie_after_final() {
    canonical(Mode::Panel, Start::PrunedImport, Outcome::ConvictedAfterFinal).await;
}

/// Panel-licensed: withheld material is a demand from the participant's node and then the producer's default, never a conviction.
#[tokio::test]
async fn g14_canonical_panel_withheld_material_is_a_demand_from_a_pruned_import_node_then_a_default() {
    canonical(Mode::Panel, Start::PrunedImport, Outcome::DefaultOnWithheldMaterial).await;
}

/// Panel-licensed: an honest covered claim checks clean; a wrong challenge from the participant is dismissed with its fee and the claim
/// finalizes on its own clock.
#[tokio::test]
async fn g14_canonical_panel_a_wrong_challenge_of_an_honest_claim_from_an_ibd_node_is_dismissed_and_it_finalizes() {
    canonical(Mode::Panel, Start::Ibd, Outcome::WrongChallengeDismissed).await;
}

/// OPV (no Panel): a post-genesis bond on a pruned-import node convicts the lie before Final.
#[tokio::test]
async fn g14_canonical_opv_a_post_genesis_bond_on_a_pruned_import_node_convicts_a_lie_before_final() {
    canonical(Mode::Opv, Start::PrunedImport, Outcome::ConvictedBeforeFinal).await;
}

/// OPV: the lie finalized; a post-genesis bond on an IBD node convicts it after Final and op 212 withdraws its fact.
#[tokio::test]
async fn g14_canonical_opv_a_post_genesis_bond_on_an_ibd_node_convicts_a_lie_after_final() {
    canonical(Mode::Opv, Start::Ibd, Outcome::ConvictedAfterFinal).await;
}

/// OPV: withheld material is a demand from the participant's IBD node and then the producer's default (a share burned).
#[tokio::test]
async fn g14_canonical_opv_withheld_material_is_a_demand_from_an_ibd_node_then_a_default() {
    canonical(Mode::Opv, Start::Ibd, Outcome::DefaultOnWithheldMaterial).await;
}

/// OPV: a wrong challenge of an honest claim from a pruned-import node is dismissed with its fee; the claim finalizes.
#[tokio::test]
async fn g14_canonical_opv_a_wrong_challenge_of_an_honest_claim_from_a_pruned_import_node_is_dismissed_and_it_finalizes() {
    canonical(Mode::Opv, Start::PrunedImport, Outcome::WrongChallengeDismissed).await;
}

// ---- GAP-04: ADR-0177 non-interference --------------------------------------------------------------------------------------

/// **A peer refusing the model changes no verdict, reward, weight or Final** (module doc). Two claims are made and (Panel) covered by
/// every seat: an honest one and a lie. Two verifier nodes start after them:
///
/// * R's only model peer refuses. R cannot check either claim, and files nothing: no consensus object says "unavailable", so the
///   refusal has nothing to change. R's RPC reads equal S's.
/// * S's first peer refuses and its second serves other weights (refused by authentication against the registered commitments); its
///   third serves the registered weights. S's participant convicts the lie, exactly as with a willing producer.
///
/// The honest claim finalizes at its window's end with the producer paid, on every node; the lie is convicted on every node; the three
/// nodes end on one sink and one PALW root (consensus weight included).
async fn non_interference(mode: Mode) {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let liar_before = w.net.collateral(4) - Rules::admission_cost(&w, mode);
    let (honest, _) = claimed(&mut w, mode, 0, false).await;
    let (lie, _) = claimed(&mut w, mode, 4, true).await;
    let floor = final_floor(&w, mode, &honest.id);
    let ceiling = final_ceiling(&w, mode, &honest.id);
    let policy = w.policy();
    let slashed = reservation(&w, mode);

    let mut r = ibd_node(&w.net).await;
    let mut s = pruned_import_node(&mut w.net).await;
    sync(&w.net, &mut r).await;
    let bond = register_newcomer(&mut s, funding).await;
    sync(&s, &mut w.net).await;
    sync(&w.net, &mut r).await;
    know_newcomer(&mut w.net, bond);

    // R: every peer refuses. Nothing to check with, nothing to file — and nothing in consensus moves.
    let rpc_r = Rpc::of(&r.chain);
    let fresh_r = rpc_r.verifier(&lie.id, 0x21);
    let root = registered_root(&fresh_r, &lie.id);
    let rows_before = rpc_r.route();
    assert!(acquire(&[ModelPeer::Refuses], &root).is_none(), "R cannot acquire the model");
    assert_eq!(rpc_r.route(), rows_before, "a refusal leaves the route's rows as they were");
    let rpc_s = Rpc::of(&s.chain);
    for c in [&honest.id, &lie.id] {
        assert_eq!(rpc_r.claim(c), rpc_s.claim(c), "R's and S's nodes serve the same claim");
    }

    // S: a refusing peer, a peer serving other weights (authentication refuses them), a peer serving the registered weights.
    let other = misaka_palw_tir_sketch::fixture::wide128_v1(8).params;
    assert_ne!(ParamCommitmentsV1::of(&other).root(), root, "other weights commit to another root");
    let peers = [ModelPeer::Refuses, ModelPeer::Serves(other), ModelPeer::Serves(w.fx.params.clone())];
    let model = acquire(&peers, &root).expect("S acquires the registered model from the third peer");
    assert_eq!(ParamCommitmentsV1::of(&model).root(), root);
    let fresh_s = rpc_s.verifier(&lie.id, 0x22);
    assert_eq!(fresh_s.check(honest.id, &honest.published(&w.fx, &[]), &model), OutsiderFindingV1::Clean);
    let OutsiderFindingV1::Prosecute(proof) = fresh_s.check(lie.id, &lie.published(&w.fx, &[]), &model) else {
        panic!("S finds the lie")
    };
    Rules::file_proof(&mut s, NEWCOMER, lie.id, proof).await;
    sync(&s, &mut w.net).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the lie is convicted by the verifier that acquired the model");
    assert_eq!(w.net.collateral(4), liar_before - slashed);
    assert_eq!(w.net.owed(NEWCOMER), Rules::bounty(&policy, slashed));

    // The honest claim: Final on its own clock, the producer paid — the refusal moved neither.
    w.net.beat_to(floor).await;
    let ClaimStateV1::Final { final_daa } = w.net.claim_state(&honest.id) else {
        panic!("the honest claim finalizes: {:?}", w.net.claim_state(&honest.id))
    };
    assert!(final_daa >= floor && final_daa <= ceiling, "at the end of its window: {final_daa}");
    assert_eq!(w.net.owed(0), Rules::final_reward(&policy), "with its reward");
    assert!(!w.net.ledger().claims[&honest.id].convicted);

    // Every node: one sink, one PALW root, the same verdicts served.
    sync(&w.net, &mut r).await;
    sync(&w.net, &mut s).await;
    agree(&w.net, &r, "R, whose every model peer refused");
    agree(&w.net, &s, "S, which acquired the model");
    let (rpc_r, rpc_s) = (Rpc::of(&r.chain), Rpc::of(&s.chain));
    for c in [&honest.id, &lie.id] {
        assert_eq!(rpc_r.claim(c), rpc_s.claim(c), "the same verdict served by both nodes");
    }
    assert!(rpc_r.claim(&lie.id).convicted && !rpc_r.claim(&honest.id).convicted);
    assert_eq!(rpc_r.finals(), rpc_s.finals(), "the same Finals");
}

#[tokio::test]
async fn g14_canonical_panel_a_peer_refusing_the_model_changes_no_verdict_reward_weight_or_final() {
    non_interference(Mode::Panel).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_peer_refusing_the_model_changes_no_verdict_reward_weight_or_final() {
    non_interference(Mode::Opv).await;
}

// ==== Milestone 3 (GAP-10): the other K2 lie types, on the canonical harness ====================================================
//
// Each lie is placed in what the producer commits for the parent's single-program class (`wide128_v1(7)`: an embedding gather, a
// wide MatMul, a rounding division and clamps, the logits), every seat covers it (Panel) or nobody does (OPV), and the participant
// of milestone 2 — a bond registered after the claim, on its own node started after it — finds it from the RPC reads and convicts it
// through its own mempool and template. The three inclusion faults (a fabricated segment boundary, a copied claim, a borrowed trace)
// are refused at inclusion: the participant's node serves no such claim and the job stays the honest producer's.
//
// **Routing and history are NOT reachable on the node yet.** A class that has them (the sketch's dense + MoE program: attention over
// a derived history window, a TopK router) declares a worst filing of ~193 MB under K2-TIR-v2's whole-instance courts, and the route
// refuses to register a class whose worst filing, response or commitment cannot be carried (1.58 MB). The test below pins that
// refusal with the measured bounds; the remedy is K2-TIR-v4's element courts (lane K2S, `k2/real-scale`, GAP-30/31), not this lane.

const DENSE_MAX_POSITIONS: u32 = 6;

/// The dense + MoE fixture as a registered kernel class (its artifact attested by the test hook, as the parent's).
fn dense_fixture() -> Fixture {
    let fx = misaka_palw_tir_sketch::fixture::dense_moe_v1(7);
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), DENSE_MAX_POSITIONS).unwrap();
    let pc = ParamCommitmentsV1::of(&fx.params);
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(pc.root()), 0);
    Fixture { program: fx.program, params: fx.params, plan, pc }
}

/// **One object from `net`, signed by actor `card`**: one carrier when it fits, else `ObjectChunk`s (judged on the assembled whole).
async fn deliver(net: &mut Net, card: usize, object: Obj) {
    match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&object, 50_000).expect("chunks") {
        None => {
            net.send(vec![(card, object)]).await;
        }
        Some(chunks) => {
            net.send(chunks.into_iter().map(|c| (card, c)).collect()).await;
        }
    }
}

/// **A class with routing and history is refused by the node's carrier fit** (the GAP-10 residual, module note above): the kernel
/// ledger itself takes the dense + MoE class, but its worst filing under K2-TIR-v2 is far past what the route can carry, so the node
/// drops the registration and no claim of it can ever exist. Pinned with the measured bound.
#[tokio::test]
async fn g14_canonical_a_routing_and_history_class_is_refused_by_the_node_carrier_fit_until_element_courts() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(Mode::Panel).await; // the parent's class registered first: the route exists
    let fx = dense_fixture();
    let d = k2_tir_v2_descriptor();
    let register = K::RegisterClass {
        descriptor: d.digest(),
        program_bytes: fx.program.encode(),
        plan: fx.plan.clone(),
        param_commitments: fx.pc.clone(),
    };
    // The kernel ledger alone takes it...
    let mut l = w.net.ledger();
    l.begin_block(w.net.daa() + 1).expect("a later block");
    l.attest_artifact(fx.pc.root());
    l.sync_bond(w.net.kid(1), w.net.collateral(1));
    let events =
        l.apply_object(&register, &misaka_palw_kernel::route::AuthV1 { signer_bond: w.net.kid(1) }).expect("the ledger takes it");
    let class = events
        .iter()
        .find_map(|e| match e {
            misaka_palw_kernel::ledger::LedgerEventV1::ClassRegistered { class } => Some(*class),
            _ => None,
        })
        .expect("registered in the ledger");
    let b = l.bounds_of(&class).expect("bounds");
    let cap = kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1;
    let why = misaka_palw_kernel::ledger::carrier_fit_v1(&b, cap, cap, cap).expect_err("past the carriers");
    assert!((b.max_filing_bytes as u128).max(b.max_response_bytes) > 100 * cap as u128, "far past the carriers: {why} ({b:?})");
    // ...and the node refuses it.
    let o = w.net.route(1, &register);
    deliver(&mut w.net, 1, o).await;
    assert!(!w.net.ledger().classes.contains_key(&class), "the node drops a class it could not prosecute: {why}");
}

/// A job of `prompt` posted by card 1.
async fn job_of(w: &mut World, prompt: Vec<u32>) -> KernelJobV1 {
    w.jobs += 1;
    let job = KernelJobV1 { class_binding_id: w.class, prompt, max_new_tokens: 3, decode: DecodeRuleV1::Greedy, nonce: [w.jobs; 64] };
    let o = w.net.route(1, &K::PostJob { job: job.clone() });
    w.net.send(vec![(1, o)]).await;
    assert!(w.net.ledger().jobs.contains_key(&job.id()), "the job posted");
    job
}

/// The first node at or after position `from` whose primitive `pick` accepts.
fn find_node(program: &TirProgramV1, from: u32, pick: impl Fn(&misaka_palw_tir::Prim) -> bool) -> (u32, u16, u16) {
    for (s, (b, _)) in program.occurrences().iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            if pick(&node.prim) {
                return (from, s as u16, n as u16);
            }
        }
    }
    panic!("the program has no such node")
}

/// **A claim the producer reveals**: `object` (a `CommitClaim`) sealed by `producer` in one block, revealed in the next (salted where
/// the ledger requires it). Returns the claim id and whether the ledger committed it.
async fn reveal(w: &mut World, producer: usize, object: &K) -> (Digest, bool) {
    let ledger = w.net.ledger();
    let K::CommitClaim { claim, .. } = object else { panic!("a single-program commit") };
    let id = claim.id();
    let (seal, reveal) = seal_and_reveal(&ledger, w.net.kid(producer), object);
    let o = w.net.route(producer, &seal);
    w.net.send(vec![(producer, o)]).await;
    let o = w.net.route(producer, &reveal);
    deliver(&mut w.net, producer, o).await;
    (id, w.net.ledger().claims.contains_key(&id))
}

/// The other lie types a committed claim can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lie {
    /// A rounding division's committed result off by one (the exact quantization / rounding families).
    Quantize,
    /// The last delivered token not the greedy selection over its committed logits (output / decode).
    Decode,
    /// Every weight perturbed and the whole trace and tokens recomputed consistently under them (a self-consistent garbage trace).
    Garbage,
}

/// **The producer's lying claim of `job`**, revealed, and (Panel) covered by every seat; returns it and its seats.
async fn lying_claim(w: &mut World, mode: Mode, producer: usize, job: &KernelJobV1, lie: Lie) -> (Claim, Vec<usize>) {
    let ledger = w.net.ledger();
    let weights = match lie {
        Lie::Garbage => {
            let mut g = w.fx.params.clone();
            for t in g.tensors.values_mut() {
                for i in 0..t.data.len() {
                    bump(t, i);
                }
            }
            g
        }
        _ => w.fx.params.clone(),
    };
    let fx = Fixture { program: w.fx.program.clone(), params: weights, plan: w.fx.plan.clone(), pc: w.fx.pc.clone() };
    let mut generated = greedy(&fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    if lie == Lie::Decode {
        let last = generated.len() - 1;
        generated[last] = (generated[last] + 1) % misaka_palw_tir_sketch::fixture::FX_V;
    }
    let program = w.fx.program.clone();
    let at = match lie {
        Lie::Quantize => find_node(&program, 1, |p| matches!(p, misaka_palw_tir::Prim::Div { .. })),
        Lie::Decode | Lie::Garbage => (0, 0, 0),
    };
    let produced = produce(&fx, &ledger, &w.class, job, w.net.kid(producer), generated, |t| {
        let v = &mut t.values[at.0 as usize][at.1 as usize][at.2 as usize];
        let before = v.clone();
        match lie {
            Lie::Quantize => bump(v, 0),
            Lie::Decode | Lie::Garbage => return,
        }
        assert_ne!(*v, before, "the lie changes the committed value");
    });
    let (id, committed) = reveal(w, producer, &produced.object).await;
    assert!(committed, "the lying claim is committed (its lie is in a relation, not in its structure)");
    let claim = Claim { id, producer, trace: produced.trace, at };
    let seats = if mode == Mode::Panel {
        let seats = w.seats(&id);
        w.cover(&id).await; // every assigned seat signs a passing receipt
        seats
    } else {
        Vec::new()
    };
    (claim, seats)
}

/// **One lie, found and convicted by the participant** (milestone 2's path): its node started after the claim, its bond registered
/// after it, its verifier from ops 211/210, the registered model from a peer; the conviction through its own mempool and template.
async fn lie_convicted(mode: Mode, start: Start, lie: Lie) {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(mode).await;
    let funding = newcomer_funding(&w.net);
    let before = w.net.collateral(0) - Rules::admission_cost(&w, mode);
    let job = job_of(&mut w, vec![3, 17, 9]).await;
    let (claim, seats) = lying_claim(&mut w, mode, 0, &job, lie).await;
    let slashed = reservation(&w, mode);
    let policy = w.policy();
    let mut node = start_node(&mut w.net, start).await;
    let bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut w.net).await;
    know_newcomer(&mut w.net, bond);
    assert!(!seats.contains(&NEWCOMER));
    let rpc = Rpc::of(&node.chain);
    let fresh = rpc.verifier(&claim.id, 0x3C);
    let model = acquire(&[ModelPeer::Serves(w.fx.params.clone())], &registered_root(&fresh, &claim.id)).expect("the model");
    let OutsiderFindingV1::Prosecute(proof) = fresh.check(claim.id, &claim.published(&w.fx, &[]), &model) else {
        panic!("{lie:?}: the participant's verifier finds the lie from public material")
    };
    match lie {
        Lie::Decode => assert!(matches!(proof, ProsecutionV1::Decode(_)), "{lie:?}: the decode court's fault"),
        _ => assert!(matches!(proof, ProsecutionV1::Kernel(_)), "{lie:?}: a kernel relation's fault"),
    }
    let seats_slashed: Vec<(usize, u64)> = seats.iter().map(|c| (*c, w.net.slashed(*c))).collect();
    Rules::file_proof(&mut node, NEWCOMER, claim.id, proof).await;
    sync(&node, &mut w.net).await;
    assert!(w.net.ledger().claims[&claim.id].convicted, "{lie:?}: convicted");
    assert_eq!(w.net.collateral(0), before - slashed, "{lie:?}: the REAL producer bond lost the reservation");
    assert_eq!(w.net.owed(NEWCOMER), Rules::bounty(&policy, slashed), "{lie:?}: the participant's share");
    for (card, before) in &seats_slashed {
        assert_eq!(w.net.slashed(*card), *before, "the colluding seats are not charged");
    }
    let read = Rpc::of(&node.chain).claim(&claim.id);
    assert!(read.convicted && read.state.starts_with("Convicted"), "{lie:?}: op 210 serves the verdict: {}", read.state);
    let ttpb = w.net.ttpb();
    w.net.chain.heartbeat(ttpb, Vec::new()).await;
    sync(&w.net, &mut node).await;
    agree(&w.net, &node, "the participant's node");
}

/// The inclusion faults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unbound {
    /// A segment's entry state root that is not its predecessor's exit (a fabricated checkpoint).
    Boundary,
    /// Another bond re-signing a published, held claim for the same job.
    Copy,
    /// A valid trace of another job's prompt, claimed for this job.
    Borrowed,
}

/// **An inclusion fault is refused, and the job stays the honest producer's**: the faulty reveal commits nothing (the participant's
/// node serves no such claim), then the honest claim of the same job commits and reaches Final with its reward.
async fn unbound_refused(mode: Mode, fault: Unbound) {
    kaspa_core::log::try_init_logger("warn");
    let mut w = world(mode).await;
    let policy = w.policy();
    let job = job_of(&mut w, vec![3, 17, 9]).await;
    let ledger = w.net.ledger();
    let honest_tokens = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    let honest = produce(&w.fx, &ledger, &w.class, &job, w.net.kid(0), honest_tokens.clone(), |_| {});
    let K::CommitClaim { claim, evidence, commitments } = honest.object.clone() else { unreachable!() };
    let (faulty, faulty_producer) = match fault {
        Unbound::Boundary => {
            let mut forged = evidence.clone();
            assert!(forged.segments.len() >= 2, "a multi-segment claim");
            forged.segments[1].entry_state_root = [0xEE; 64];
            let c = KernelClaimV1 { evidence_root: forged.root(), ..claim.clone() };
            (K::CommitClaim { claim: c, evidence: forged, commitments: commitments.clone() }, 0)
        }
        Unbound::Copy => {
            // The original commits (and, Panel, is covered) first; the copy names another producer bond.
            let (id, committed) = reveal(&mut w, 0, &honest.object).await;
            assert!(committed);
            if mode == Mode::Panel {
                w.cover(&id).await;
            }
            let c = KernelClaimV1 { producer_bond: w.net.kid(4), ..claim.clone() };
            (K::CommitClaim { claim: c, evidence: evidence.clone(), commitments: commitments.clone() }, 4)
        }
        Unbound::Borrowed => {
            // A valid trace of another prompt (another job's input), claimed for this job.
            let other = KernelJobV1 { prompt: vec![5, 1, 2], ..job.clone() };
            let tokens = greedy(&w.fx, &ledger, &w.class, &other.prompt, other.max_new_tokens as usize);
            let borrowed = produce(&w.fx, &ledger, &w.class, &other, w.net.kid(0), tokens, |_| {});
            let K::CommitClaim { claim: bc, evidence: be, commitments: bm } = borrowed.object else { unreachable!() };
            let c = KernelClaimV1 { job_id: job.id(), ..bc };
            (K::CommitClaim { claim: c, evidence: be, commitments: bm }, 0)
        }
    };
    let (faulty_id, committed) = reveal(&mut w, faulty_producer, &faulty).await;
    assert!(!committed, "{fault:?}: refused at inclusion");
    // The participant's node, started now, serves no such claim.
    let node = ibd_node(&w.net).await;
    let read = Rpc::of(&node.chain).claim(&faulty_id);
    assert!(read.available && !read.found, "{fault:?}: op 210 serves no such claim");
    // The job is the honest producer's to take (Copy: it already holds it) and reaches Final with its reward.
    let honest_id = claim.id();
    if fault != Unbound::Copy {
        let (id, committed) = reveal(&mut w, 0, &honest.object).await;
        assert!(committed && id == honest_id, "{fault:?}: the honest claim of the job commits");
        if mode == Mode::Panel {
            w.cover(&id).await;
        }
    }
    assert_eq!(w.net.ledger().job_claims.get(&job.id()), Some(&honest_id), "{fault:?}: the honest claim holds the job");
    let floor = final_floor(&w, mode, &honest_id);
    w.net.beat_to(floor).await;
    assert!(matches!(w.net.claim_state(&honest_id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&honest_id));
    assert_eq!(w.net.owed(0), Rules::final_reward(&policy), "{fault:?}: the honest producer is paid once");
    assert_eq!(w.net.owed(4), 0, "{fault:?}: the copyist is paid nothing");
}

#[tokio::test]
async fn g14_canonical_panel_a_quantization_lie_every_seat_covered_is_convicted_by_the_participant() {
    lie_convicted(Mode::Panel, Start::Ibd, Lie::Quantize).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_quantization_lie_is_convicted_by_the_participant() {
    lie_convicted(Mode::Opv, Start::PrunedImport, Lie::Quantize).await;
}

#[tokio::test]
async fn g14_canonical_panel_a_substituted_token_every_seat_covered_is_convicted_by_the_decode_court() {
    lie_convicted(Mode::Panel, Start::PrunedImport, Lie::Decode).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_substituted_token_is_convicted_by_the_decode_court() {
    lie_convicted(Mode::Opv, Start::Ibd, Lie::Decode).await;
}

#[tokio::test]
async fn g14_canonical_panel_a_self_consistent_garbage_trace_every_seat_covered_is_convicted_against_the_registered_weights() {
    lie_convicted(Mode::Panel, Start::Ibd, Lie::Garbage).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_self_consistent_garbage_trace_is_convicted_against_the_registered_weights() {
    lie_convicted(Mode::Opv, Start::PrunedImport, Lie::Garbage).await;
}

#[tokio::test]
async fn g14_canonical_panel_a_fabricated_segment_boundary_is_refused_at_inclusion() {
    unbound_refused(Mode::Panel, Unbound::Boundary).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_fabricated_segment_boundary_is_refused_at_inclusion() {
    unbound_refused(Mode::Opv, Unbound::Boundary).await;
}

#[tokio::test]
async fn g14_canonical_panel_a_copied_claim_is_refused_and_the_job_paid_once() {
    unbound_refused(Mode::Panel, Unbound::Copy).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_copied_claim_is_refused_and_the_job_paid_once() {
    unbound_refused(Mode::Opv, Unbound::Copy).await;
}

#[tokio::test]
async fn g14_canonical_panel_a_borrowed_trace_of_another_prompt_is_refused_at_inclusion() {
    unbound_refused(Mode::Panel, Unbound::Borrowed).await;
}

#[tokio::test]
async fn g14_canonical_opv_a_borrowed_trace_of_another_prompt_is_refused_at_inclusion() {
    unbound_refused(Mode::Opv, Unbound::Borrowed).await;
}
