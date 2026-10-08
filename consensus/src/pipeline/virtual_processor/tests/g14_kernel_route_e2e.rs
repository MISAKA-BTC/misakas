//! **G14 on the real node: the kernel route (`misaka-palw-kernel`) folded into `PalwChainStateV2`, reached through the mempool, the
//! node's own block template and the chain block's fold.**
//!
//! The property (g14-integration-matrix): the producer AND every Panel seat collude; one ordinary bonded verifier outside the Panel —
//! built from the node's public read API and a public DA directory of what the producer published, nothing else — convicts a
//! fraudulent claim (pre- or post-Final) or, for withheld material, obtains a correctly classified default; the consensus accepts it
//! deterministically, slashes the real bond, and a second node replaying the blocks, a reorg and a restart reach the same roots.
//!
//! **The fence is armed WITHOUT its validation.** `palw_probabilistic_constraints_v1` is refused on every real height by
//! `Params::validate_palw_probabilistic_constraints_v1` (RFC-0011 §15.7 assigns it none), so nothing here can run on a network; the
//! config below is built from the harness's validated testnet-12 and then arms the fence directly (as `rfc0010_permissionless_panel.rs`
//! bypasses its own refusal). **Artifact attestation is a test-only hook** (`kernel_route_test_attest_artifact_v1`, `cfg(test)`): there
//! is no on-chain artifact availability or conformance fact yet, and it is never derived from a declaration — GAP.
//!
//! **Seats are INTERIM** (a grindable integer race seeded by the claim id): every assigned seat colludes here.
use super::g14_registration_e2e::{arrive, chain_blocks, root_at};
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_genesis_chain_on, t12_reopened_chain, t12_with_harness_cards,
};
use crate::consensus::test_consensus::TestConsensus;
use crate::pipeline::virtual_processor::processor::kernel_route_test_attest_artifact_v1;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1, PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelChunkTargetV1, PalwKernelChunkV1,
    PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_chunk_message_v1, palw_kernel_chunks_v1, palw_kernel_payout_key_v1,
    palw_kernel_route_message_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{
    KernelLedgerV1, OutsiderFindingV1, OutsiderV1, ProsecutionV1, PublicSourceV1, claim_seal_v1, proof_seal_v1, single_class_id_v1,
};
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{MaterialResponseV1, PositionResponseV1, TensorWireV1, program_root_v1};
use misaka_palw_kernel::receipt::{CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1, PalwConstraintReceiptV1, ReceiptInputsV1, receipt_for_v1};
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1, WiringV1, derived_mask_v1, trace_v1};
use misaka_palw_kernel::verify::{CheckCostV1, ScopeVerdictV1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Tensor};
use misaka_palw_tir_sketch::fixture::wide128_v1;
use std::collections::BTreeMap;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

const FEE: u64 = 2_000_000;
const MAX_POSITIONS: u32 = 64;

// ---- the network -----------------------------------------------------------------------------------------

/// testnet-12 as launched, harness cards, the kernel route's fence armed at genesis WITHOUT its validation (module doc).
fn kernel_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    kernel_config_with(None)
}

/// **An OPV network** (RFC-0015): [`kernel_config`] with `Params::palw_panel_free_v1` carrying the network's admission list and the
/// interim OPV terms, activating at DAA 1. Like the route's fence it is armed WITHOUT its validation (which refuses every height).
fn kernel_config_opv(admitted: Vec<Hash64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    kernel_config_with(Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), admitted)))
}

fn kernel_config_with(opv: Option<PalwPanelFreeFenceV1>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_panel_free_v1 = opv;
    // A shallow all-economic tie (a heartbeat branch against a heartbeat branch) is GHOSTDAG's, not a hash race: the reorg tests need
    // a heavier branch to win deterministically (rcore/f1-strictwin-tie).
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    params.skip_proof_of_work = true;
    // `ConfigBuilder::build` runs `validate_palw_v2`, which refuses these fences by design (module doc): construct the config directly.
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fence; only this harness bypasses it");
    (Config::new(params), bundle, premine, floats)
}

/// The class ids the OPV network's policy admits: the fixture's class under `OptimisticPublicVerification`.
fn opv_admitted() -> Vec<Hash64> {
    let (d, fx) = (k2_tir_v2_descriptor(), fixture());
    vec![Hash64::from_bytes(single_class_id_v1(
        d.digest(),
        &fx.program.encode(),
        &fx.plan,
        &fx.pc,
        VerificationModeV1::OptimisticPublicVerification,
    ))]
}

/// The class every scenario uses: the sketch's history-free `i128`-accumulator layer (K2-TIR-v2), small enough that its worst
/// filing, response and commitments fit the real carriers.
struct Fixture {
    program: TirProgramV1,
    params: MapParams,
    plan: misaka_palw_kernel::VerificationPlanV1,
    pc: ParamCommitmentsV1,
}

fn fixture() -> Fixture {
    let fx = wide128_v1(7);
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), MAX_POSITIONS).unwrap();
    let pc = ParamCommitmentsV1::of(&fx.params);
    // The test-only attestation: this artifact is public (from DAA 0, before any block).
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(pc.root()), 0);
    Fixture { program: fx.program, params: fx.params, plan, pc }
}

struct Net {
    chain: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
    /// Each card's next funding output (a change chain).
    funding: Vec<(TransactionOutpoint, UtxoEntry)>,
    domain: Hash64,
    rnd: u8,
    /// Receivers the node's notification channels need to stay open (a persistent node).
    _keep: Vec<Box<dyn std::any::Any>>,
}

impl Net {
    fn new() -> Net {
        Net::over(TestConsensus::new)
    }

    /// The network over a consensus the caller builds (a database it keeps, to restart the node over it).
    fn over(make: impl FnOnce(&Config) -> TestConsensus) -> Net {
        Net::over_cfg(kernel_config(), make)
    }

    fn over_cfg(parts: (Config, PalwConsensusParamsV2, Premine, Premine), make: impl FnOnce(&Config) -> TestConsensus) -> Net {
        let (config, bundle, premine, floats) = parts;
        let chain = t12_genesis_chain_on(make(&config), &config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let funding = floats.clone();
        Net { chain, config, bundle, premine, floats, funding, domain, rnd: 0, _keep: Vec::new() }
    }

    fn ttpb(&self) -> u64 {
        self.config.params.target_time_per_block()
    }

    fn bond(&self, card: usize) -> PalwBondKeyV2 {
        self.chain.bonds[card]
    }

    fn kid(&self, card: usize) -> Digest {
        palw_kernel_bond_id_v1(&self.bond(card))
    }

    fn daa(&self) -> u64 {
        self.chain.daa_of(self.chain.sink())
    }

    /// A signed `KernelRouteV1` carrying `object` for card `card`'s bond.
    fn route(&mut self, card: usize, object: &K) -> Obj {
        let bytes = object.encode();
        let signer = self.bond(card);
        let message = palw_kernel_route_message_v1(self.domain, &signer, &bytes);
        self.rnd = self.rnd.wrapping_add(1);
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
            [self.rnd; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec();
        Obj::KernelRouteV1 { bytes, signer, signature }
    }

    /// **A chunk of the route's own chunk lane (tag 113)**, signed by card `card`'s bond as its opener.
    fn sign_chunk(&mut self, card: usize, chunk: PalwKernelChunkV1) -> Obj {
        let message = palw_kernel_chunk_message_v1(self.domain, &chunk);
        self.rnd = self.rnd.wrapping_add(1);
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1,
            [self.rnd; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec();
        Obj::KernelRouteChunkV1 { chunk: Box::new(chunk), signature }
    }

    /// **`object` through the route's own chunk lane** (C4 F-C4R3-03): cut into chunks of at most `cap` bytes for `target`, each signed
    /// by card `card`'s bond as the group's opener.
    fn kernel_chunks(&mut self, card: usize, object: &Obj, target: PalwKernelChunkTargetV1, cap: usize) -> Vec<Obj> {
        let chunks = palw_kernel_chunks_v1(object, self.bond(card), target, cap).expect("the object cuts into the lane's chunks");
        chunks.into_iter().map(|c| self.sign_chunk(card, c)).collect()
    }

    /// A seat's signed constraint receipt (tag 111).
    fn receipt(&mut self, card: usize, receipt: PalwConstraintReceiptV1) -> Obj {
        self.rnd = self.rnd.wrapping_add(1);
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            &receipt.signing_message(),
            CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1,
            [self.rnd; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec();
        Obj::KernelConstraintReceiptV1 { receipt: Box::new(receipt), signature }
    }

    /// A `0x4b` carrier for `object`, funded by card `card`'s change chain (which it advances).
    fn carrier(&mut self, card: usize, object: &Obj) -> Transaction {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).expect("serializes");
        let (outpoint, entry) = self.funding[card].clone();
        // A chunk group's opener pays the slot's rent (ADR-0075 SA-1, armed on testnet-12's harness); every chunk is sent with it.
        let fee = if matches!(object, Obj::ObjectChunk { .. }) {
            FEE + kaspa_consensus_core::palw_state_v2::palw_object_chunk_group_rent_v1()
        } else {
            FEE
        };
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - fee, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, self.config.params.storage_mass_parameter);
        self.funding[card] =
            (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(tx.outputs[0].value, card_payout_spk(card), 0, false));
        tx
    }

    fn mempool(&self, tx: &Transaction) -> Result<(), kaspa_consensus_core::errors::tx::TxRuleError> {
        self.chain
            .vp()
            .validate_mempool_transaction(&mut kaspa_consensus_core::tx::MutableTransaction::from_tx(tx.clone()), &Default::default())
    }

    /// **Send objects**: each through the mempool, into the node's own template, and folded by the block after the one that carries
    /// it. A card's carriers chain (each spends the previous one's change) and the template takes only confirmed outputs, so the
    /// i-th object of a card rides the i-th carrying block (the pool judges each wave on the confirmed set); the objects fold one
    /// block after their carrying block, in wave order. Returns the last folding block.
    async fn send(&mut self, items: Vec<(usize, Obj)>) -> Block {
        let mut waves: Vec<Vec<Transaction>> = Vec::new();
        let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
        for (card, object) in &items {
            let tx = self.carrier(*card, object);
            let wave = *seen.entry(*card).and_modify(|n| *n += 1).or_insert(0);
            if waves.len() <= wave {
                waves.resize(wave + 1, Vec::new());
            }
            waves[wave].push(tx);
        }
        let ttpb = self.ttpb();
        let from = self.daa();
        for txs in &waves {
            for tx in txs {
                self.mempool(tx).unwrap_or_else(|e| panic!("the mempool takes the carrier: {e}"));
            }
            let carrying = self.chain.heartbeat(ttpb, txs.clone()).await;
            for tx in txs {
                assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the node's template carries the kernel carrier");
            }
        }
        let folding = self.chain.heartbeat(ttpb, Vec::new()).await;
        if std::env::var("G14_TRACE").is_ok() {
            eprintln!("[send] {} object(s): daa {from} -> {}", items.len(), self.daa());
        }
        folding
    }

    async fn beat_to(&mut self, daa: u64) {
        let ttpb = self.ttpb();
        while self.daa() < daa {
            self.chain.heartbeat(ttpb, Vec::new()).await;
        }
    }

    /// The route's state at the tip, through the node's read API.
    fn api(&self) -> Option<PalwKernelRouteStateV1> {
        self.chain.ctx.consensus.palw_kernel_route_v1()
    }

    fn ledger(&self) -> KernelLedgerV1 {
        self.api().expect("the route exists").ledger().expect("the stored rows rebuild")
    }

    fn collateral(&self, card: usize) -> u64 {
        self.chain.tip_state().1.bond(&self.bond(card)).expect("the bond").collateral
    }

    /// **A real restart**: the node is shut down and a new `Consensus` is opened over the SAME database; the actors carry on.
    fn restart(self, db: std::sync::Arc<kaspa_database::prelude::DB>) -> Net {
        let Net { chain, config, bundle, premine, floats, funding, domain, rnd, _keep } = self;
        let (simulated_time, nonce) = (chain.ctx.simulated_time, chain.nonce_for_reopen());
        drop(chain); // TestContext's Drop shuts the processors down and releases the node's handles on the database
        let mut resumed = config.clone();
        resumed.process_genesis = false;
        let (sender, receiver) = async_channel::unbounded();
        let second = TestConsensus::with_db(db, &resumed, sender);
        let chain = t12_reopened_chain(second, &resumed, &bundle, simulated_time, nonce);
        let mut keep = _keep;
        keep.push(Box::new(receiver));
        Net { chain, config, bundle, premine, floats, funding, domain, rnd, _keep: keep }
    }

    /// This network's actors around another node's chain (a replay of this one): the same funding outputs, the same keys.
    fn on_chain(&self, chain: T12Chain) -> Net {
        Net {
            chain,
            config: self.config.clone(),
            bundle: self.bundle.clone(),
            premine: self.premine.clone(),
            floats: self.floats.clone(),
            funding: self.funding.clone(),
            domain: self.domain,
            rnd: self.rnd,
            _keep: Vec::new(),
        }
    }

    /// The bond's slashed total (burn at release) and its collateral, the V2 truth a slash lands on.
    fn slashed(&self, card: usize) -> u64 {
        self.chain.tip_state().1.bond(&self.bond(card)).expect("the bond").slashed
    }

    fn kernel_reserved(&self, card: usize) -> u128 {
        self.chain.tip_state().1.kernel_reserved(&self.bond(card))
    }

    /// What the coinbase queue owes card `card`'s payee (the kernel's `0xFD` prefix).
    fn owed(&self, card: usize) -> u64 {
        let state = self.chain.tip_state().1;
        let payee = state.bond(&self.bond(card)).unwrap().payout_payload;
        let key = palw_kernel_payout_key_v1(&payee);
        state.pending_payouts_iter().find(|(k, _)| **k == key).map(|(_, p)| p.amount).unwrap_or(0)
    }

    fn claim_state(&self, claim: &Digest) -> ClaimStateV1 {
        self.ledger().claims[claim].life.state.clone()
    }

    /// A second node that replays this one's selected chain from genesis, the way a syncing node does.
    async fn replay(&self) -> T12Chain {
        let z = t12_genesis_chain(&self.config, &self.bundle, &self.premine, &self.floats);
        for b in chain_blocks(&self.chain, self.chain.sink()) {
            arrive(&z, b, "a block of the first node").await;
        }
        z
    }

    /// Everything two nodes at one tip must agree on: the sink, the PALW root, the kernel route's rows (as served), every block's delta
    /// root, and the chain's own UTXO commitment (the payout queue is paid through the coinbase).
    fn assert_same(&self, other: &T12Chain, what: &str) {
        assert_eq!(other.sink(), self.chain.sink(), "{what}: same sink");
        assert_eq!(other.tip_state().1.state_root(), self.chain.tip_state().1.state_root(), "{what}: same PALW state root");
        assert_eq!(other.ctx.consensus.palw_kernel_route_v1(), self.api(), "{what}: same kernel route rows");
        for b in chain_blocks(&self.chain, self.chain.sink()) {
            assert_eq!(root_at(other, b.header.hash), root_at(&self.chain, b.header.hash), "{what}: delta root of {}", b.header.hash);
        }
    }

}

// ---- the producer ----------------------------------------------------------------------------------------

struct Produced {
    claim: KernelClaimV1,
    trace: TraceV1,
    object: K,
}

/// The values a producer PUBLISHED (its DA directory), minus what it withholds; never a derived value.
struct Da(BTreeMap<(u32, u16, u16), Vec<u8>>);

impl Da {
    fn publishing(program: &TirProgramV1, trace: &TraceV1, withhold: &[(u32, u16, u16)]) -> Self {
        let mask = derived_mask_v1(program);
        let mut m = BTreeMap::new();
        for (p, pos) in trace.values.iter().enumerate() {
            for (s, occ) in pos.iter().enumerate() {
                for (n, t) in occ.iter().enumerate() {
                    let k = (p as u32, s as u16, n as u16);
                    if !withhold.contains(&k) && !mask[s][n] {
                        m.insert(k, borsh::to_vec(&TensorWireV1::of(t)).unwrap());
                    }
                }
            }
        }
        Da(m)
    }
}

impl PublicSourceV1 for Da {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if stage != 0 {
            return None;
        }
        borsh::from_slice::<TensorWireV1>(self.0.get(&(p, s, n))?).ok()?.decode().ok()
    }
}

fn bump(t: &mut Tensor, at: usize) {
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}

/// The honest greedy generation of `n` tokens.
fn greedy(fx: &Fixture, ledger: &KernelLedgerV1, class: &Digest, prompt: &[u32], n: usize) -> Vec<u32> {
    let (post, logits) = ledger.classes[class].logits_at();
    let mut stream = prompt.to_vec();
    let mut out = vec![];
    for _ in 0..n {
        let t = trace_v1(&fx.program, &fx.params, &stream).unwrap();
        let tok = DecodeRuleV1::Greedy.select(&t.values[stream.len() - 1][post as usize][logits as usize]).unwrap();
        out.push(tok);
        stream.push(tok);
    }
    out
}

/// A claim of `job` by `producer` delivering `generated`, its committed trace edited by `lie`.
fn produce(
    fx: &Fixture,
    ledger: &KernelLedgerV1,
    class: &Digest,
    job: &KernelJobV1,
    producer: Digest,
    generated: Vec<u32>,
    lie: impl FnOnce(&mut TraceV1),
) -> Produced {
    let fed = generated.len() - 1;
    let stream: Vec<u32> = job.prompt.iter().chain(&generated[..fed]).copied().collect();
    let mut trace = trace_v1(&fx.program, &fx.params, &stream).unwrap();
    lie(&mut trace);
    let row = &ledger.classes[class];
    let w = WiringV1::new(&fx.program).unwrap();
    let ev = build_evidence_v1(&w, &trace.evidence(), &stream, row.header(*class), &row.descriptor, 2).unwrap();
    let claim = KernelClaimV1 { job_id: job.id(), producer_bond: producer, generated, evidence_root: ev.root() };
    let object = K::CommitClaim { claim: claim.clone(), evidence: ev, commitments: trace.evidence().commitments };
    Produced { claim, trace, object }
}

fn matmul_at(program: &TirProgramV1, from: u32) -> (u32, u16, u16) {
    for (s, (b, _)) in program.occurrences().iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            if matches!(node.prim, misaka_palw_tir::Prim::MatMul) {
                return (from, s as u16, n as u16);
            }
        }
    }
    panic!("no MatMul")
}

/// **A fresh verifier: built ONLY from the node's read API (the rows and the root the chain committed) and a public DA directory,
/// with its own salt.** It shares no memory with the producer's objects; it rebuilds the ledger from the served rows and refuses them
/// unless they root to the committed ledger root.
struct Fresh {
    ledger: KernelLedgerV1,
    salt: Digest,
}

impl Fresh {
    fn from_api(api: &PalwKernelRouteStateV1, committed_ledger_root: Hash64, salt: u8) -> Fresh {
        assert_eq!(api.ledger_root(), committed_ledger_root, "the served rows root to the committed ledger root");
        let template = api.template();
        let ledger = KernelLedgerV1::from_rows(&template, api.header.scalars, &api.rows).expect("the served rows rebuild a ledger");
        assert_eq!(ledger.root(), committed_ledger_root.as_bytes(), "and the rebuilt ledger's own root agrees");
        Fresh { ledger, salt: [salt; 64] }
    }

    fn check(&self, claim: Digest, da: &Da, artifact: &MapParams) -> OutsiderFindingV1 {
        OutsiderV1 { ledger: &self.ledger, claim, material: da, artifact, salt: self.salt }.check().expect("the fresh verifier concludes")
    }
}

/// The colluding seats' receipts for `claim`: each assigned seat signs a passing receipt for its scope (it did no work).
fn colluding_receipts(net: &mut Net, claim: Digest, signed_daa: u64) -> Vec<(usize, Obj)> {
    let state = net.chain.tip_state().1;
    let kr = state.kernel_route().expect("the route");
    let assignment = kr.assignment_of(&claim).expect("the claim has an interim assignment");
    let ledger = net.ledger();
    let row = &ledger.claims[&claim];
    let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &row.body else { panic!("a program claim") };
    let class = &ledger.classes[&row.class_binding_id];
    let mut out = Vec::new();
    for seat in &assignment.seats {
        let card = net.chain.bonds.iter().position(|b| *b == seat.bond).expect("a seat is a genesis card");
        let bond = state.bond(&seat.bond).expect("the seat bond");
        let verdict = ScopeVerdictV1::Pass {
            scope_root: seat.scope.root(evidence),
            positions: evidence.positions,
            probabilistic_checks: 1_000_000,
            error_bits: 80,
            cost: CheckCostV1::default(),
        };
        let inputs = ReceiptInputsV1 {
            descriptor: &class.descriptor,
            evidence,
            claim_id: claim,
            assignment_root: assignment.assignment_root,
            challenge_anchor: assignment.challenge_anchor,
            sample_seed: assignment.sample_seed,
            seat_bond: seat.kernel_bond,
            seat_operator: bond.operator_id.as_bytes(),
            signed_daa,
        };
        let receipt = receipt_for_v1(&inputs, &seat.scope, &verdict).expect("a passing receipt");
        out.push((card, net.receipt(card, receipt)));
    }
    out
}

// ---- the world: a registered class and the actors around it ---------------------------------------------------

/// The producer's private claim: its trace never reaches a verifier except through what it publishes.
struct Claim {
    id: Digest,
    producer: usize,
    trace: TraceV1,
    at: (u32, u16, u16),
}

impl Claim {
    /// The DA directory the producer publishes: everything but `withhold`.
    fn published(&self, fx: &Fixture, withhold: &[(u32, u16, u16)]) -> Da {
        Da::publishing(&fx.program, &self.trace, withhold)
    }

    /// A position demand's response: every committed value of position `p`, whole, then `edit`ed.
    fn position(&self, fx: &Fixture, p: u32, edit: impl FnOnce(&mut Vec<Vec<MaterialResponseV1>>)) -> Vec<u8> {
        let mask = derived_mask_v1(&fx.program);
        let mut r: Vec<Vec<MaterialResponseV1>> = self.trace.values[p as usize]
            .iter()
            .enumerate()
            .map(|(s, o)| {
                o.iter()
                    .enumerate()
                    .map(|(n, t)| if mask[s][n] { MaterialResponseV1::Omitted } else { MaterialResponseV1::Whole(TensorWireV1::of(t)) })
                    .collect()
            })
            .collect();
        edit(&mut r);
        borsh::to_vec(&PositionResponseV1 { values: r, inputs: vec![] }).unwrap()
    }
}

/// How the producer's reveal reaches the chain.
#[derive(Clone, Copy)]
enum Delivery {
    Direct,
    /// Cut into `ObjectChunk`s of at most this many bytes.
    Chunked(usize),
    /// The same, with the assembled object's signature corrupted: the completing chunk must be dropped.
    ChunkedTampered(usize),
}

struct World {
    net: Net,
    fx: Fixture,
    class: Digest,
    jobs: u8,
}

impl World {
    async fn new() -> World {
        World::on(Net::new()).await
    }

    /// **An OPV network** (RFC-0015): `Params::palw_panel_free_v1` carries the network's admission list and the interim terms
    /// (activating at DAA 1), the chain is beaten to DAA 1 and the class registered under `OptimisticPublicVerification` by card 1.
    async fn opv() -> World {
        World::on_opv(Net::over_cfg(kernel_config_opv(opv_admitted()), TestConsensus::new)).await
    }

    async fn on_opv(mut net: Net) -> World {
        let fx = fixture();
        let d = k2_tir_v2_descriptor();
        net.beat_to(1).await;
        let id = single_class_id_v1(d.digest(), &fx.program.encode(), &fx.plan, &fx.pc, VerificationModeV1::OptimisticPublicVerification);
        let register = K::RegisterClassV2 {
            mode: VerificationModeV1::OptimisticPublicVerification,
            descriptor: d.digest(),
            program_bytes: fx.program.encode(),
            plan: fx.plan.clone(),
            param_commitments: fx.pc.clone(),
        };
        let o = net.route(1, &register);
        net.send(vec![(1, o)]).await;
        let class = *net.ledger().classes.keys().next().expect("the OPV class registered through the real path");
        assert_eq!(class, id, "the class id binds the mode");
        assert!(net.ledger().opv.classes.contains(&class), "and the ledger holds it as an OPV class");
        World { net, fx, class, jobs: 0 }
    }

    /// Registers the class (signed by card 1) over `net`.
    async fn on(mut net: Net) -> World {
        let fx = fixture();
        let d = k2_tir_v2_descriptor();
        let register = K::RegisterClass {
            descriptor: d.digest(),
            program_bytes: fx.program.encode(),
            plan: fx.plan.clone(),
            param_commitments: fx.pc.clone(),
        };
        let o = net.route(1, &register);
        net.send(vec![(1, o)]).await;
        let class = *net.ledger().classes.keys().next().expect("the class registered through the real path");
        World { net, fx, class, jobs: 0 }
    }

    fn restart(self, db: std::sync::Arc<kaspa_database::prelude::DB>) -> World {
        let World { net, fx, class, jobs } = self;
        World { net: net.restart(db), fx, class, jobs }
    }

    fn policy(&self) -> misaka_palw_kernel::ledger::LedgerPolicyV1 {
        self.net.ledger().policy
    }

    /// A job posted by card 1.
    async fn job(&mut self) -> KernelJobV1 {
        self.jobs += 1;
        let job = KernelJobV1 {
            class_binding_id: self.class,
            prompt: vec![3, 17, 9],
            max_new_tokens: 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [self.jobs; 64],
        };
        let o = self.net.route(1, &K::PostJob { job: job.clone() });
        self.net.send(vec![(1, o)]).await;
        assert!(self.net.ledger().jobs.contains_key(&job.id()), "the job posted");
        job
    }

    /// **A claim of `job` by card `producer`, sealed one block and then revealed**; `lie` bumps one MatMul value at position 1 of
    /// the trace the producer commits to.
    async fn claim(&mut self, producer: usize, job: &KernelJobV1, lie: bool) -> Claim {
        let claim = self.claim_with(producer, job, lie, Delivery::Direct).await;
        assert!(self.net.ledger().claims.contains_key(&claim.id), "the claim committed over its seal");
        claim
    }

    /// [`Self::claim`] with the reveal delivered as `delivery` (one carrier, or `ObjectChunk`s of a given size).
    async fn claim_with(&mut self, producer: usize, job: &KernelJobV1, lie: bool, delivery: Delivery) -> Claim {
        let ledger = self.net.ledger();
        let generated = greedy(&self.fx, &ledger, &self.class, &job.prompt, job.max_new_tokens as usize);
        let at = matmul_at(&self.fx.program, 1);
        let kid = self.net.kid(producer);
        let produced = produce(&self.fx, &ledger, &self.class, job, kid, generated, |t| {
            if lie {
                bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1)
            }
        });
        let id = produced.claim.id();
        let seal = K::SealClaim { producer: kid, job: produced.claim.job_id, seal: claim_seal_v1(&id) };
        let o = self.net.route(producer, &seal);
        self.net.send(vec![(producer, o)]).await;
        let mut o = self.net.route(producer, &produced.object);
        match delivery {
            Delivery::Direct => {
                self.net.send(vec![(producer, o)]).await;
            }
            Delivery::Chunked(cap) | Delivery::ChunkedTampered(cap) => {
                if matches!(delivery, Delivery::ChunkedTampered(_)) {
                    let Obj::KernelRouteV1 { signature, .. } = &mut o else { unreachable!() };
                    let last = signature.len() - 1;
                    signature[last] ^= 0x01; // the assembled object's signature no longer verifies
                }
                let chunks = kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, cap)
                    .expect("chunks")
                    .expect("the object is larger than one chunk");
                assert!(chunks.len() >= 2, "a genuinely multi-chunk delivery: {}", chunks.len());
                self.net.send(chunks.into_iter().map(|c| (producer, c)).collect()).await;
            }
        }
        Claim { id, producer, trace: produced.trace, at }
    }

    /// The cards the interim assignment seated for `claim` (while its receipts are being counted).
    fn seats(&self, claim: &Digest) -> Vec<usize> {
        let state = self.net.chain.tip_state().1;
        let assignment = state.kernel_route().and_then(|k| k.assignment_of(claim)).expect("the claim has an interim assignment");
        assignment.seats.iter().map(|s| self.net.chain.bonds.iter().position(|b| *b == s.bond).expect("a genesis card")).collect()
    }

    /// `n` bonded cards that are neither the producer nor one of the claim's seats (nor card 1, the class's registrant).
    fn outsiders(&self, claim: &Claim, seats: &[usize], n: usize) -> Vec<usize> {
        let picked: Vec<usize> = (0..self.net.chain.bonds.len())
            .filter(|c| *c != claim.producer && *c != 1 && !seats.contains(c))
            .take(n)
            .collect();
        assert_eq!(picked.len(), n, "enough bonded cards outside the Panel");
        picked
    }

    /// **Every assigned seat colludes**: each signs a passing receipt for its scope and the Panel's tally covers the claim.
    async fn cover(&mut self, claim: &Digest) {
        let daa = self.net.daa();
        let receipts = colluding_receipts(&mut self.net, *claim, daa);
        self.net.send(receipts).await;
        assert!(
            matches!(self.net.claim_state(claim), ClaimStateV1::ProbabilisticPass { .. }),
            "the colluding Panel passed the claim: {:?}",
            self.net.claim_state(claim)
        );
    }

    /// A fresh verifier built from the read API alone.
    fn fresh(&self, salt: u8) -> Fresh {
        let api = self.net.api().expect("the read API serves the route");
        Fresh::from_api(&api, api.ledger_root(), salt)
    }

    async fn demand(&mut self, card: usize, claim: &Digest, position: u32) {
        let o = self.net.route(card, &K::FileDemand { demander: self.net.kid(card), claim: *claim, stage: 0, position });
        self.net.send(vec![(card, o)]).await;
    }

    async fn proof(&mut self, card: usize, claim: &Digest, proof: ProsecutionV1) {
        let o = self.net.route(card, &K::FileProof { accuser: self.net.kid(card), claim: *claim, proof });
        self.net.send(vec![(card, o)]).await;
    }

    async fn serve(&mut self, card: usize, claim: &Claim, position: u32) {
        let bytes = claim.position(&self.fx, position, |_| {});
        let o = self.net.route(card, &K::Respond { claim: claim.id, stage: 0, position, bytes });
        self.net.send(vec![(card, o)]).await;
    }

    /// Fresh-verify `claim` against `da` and demand the proof: panics unless the finding is a prosecution.
    fn prosecution(&self, claim: &Digest, da: &Da, salt: u8) -> ProsecutionV1 {
        match self.fresh(salt).check(*claim, da, &self.fx.params) {
            OutsiderFindingV1::Prosecute(proof) => proof,
            other => panic!("the fresh verifier should prosecute: {other:?}"),
        }
    }
}

/// A lifecycle state's spelled-out name (`Challengeable { .. }`, `Final { .. }`, ...).
trait StateName {
    fn to_string_state(&self) -> String;
}

impl StateName for ClaimStateV1 {
    fn to_string_state(&self) -> String {
        format!("{self:?}")
    }
}

/// **What a pre-Final default pays its (sole) demander** (C4 F-C4R3-02): the penalty split like a slash — the accuser's share, and an
/// OPV claim's no more than `1000 − default_burn_permille`.
fn default_share(pol: &misaka_palw_kernel::ledger::LedgerPolicyV1, opv: Option<&misaka_palw_kernel::opv::OpvPolicyV1>) -> u64 {
    let slash_burn = 1000 - u64::from(pol.accuser_reward_permille);
    let burn = opv.map_or(slash_burn, |o| slash_burn.max(u64::from(o.economics.default_burn_permille)));
    pol.default_penalty - pol.default_penalty * burn / 1000
}

fn mega(n: u64) -> u64 {
    n * kaspa_consensus_core::constants::SOMPI_PER_KASPA
}

// ---- G14, the claim: producer and every seat collude, one outsider convicts -----------------------------------

/// **Milestone: a lying claim, covered by every colluding seat, is convicted by an outsider through the real path** — class
/// registration, job, seal, commitment, interim assignment, the seats' receipts, the Panel's pass, the outsider's filing from public
/// material alone, the conviction in the fold, the real bond slashed, the accuser's reward queued for the coinbase and paid by it.
#[tokio::test]
async fn g14_kernel_route_a_covered_lie_is_convicted_by_an_outsider_through_the_real_path() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let state = w.net.chain.tip_state().1;
    assert_eq!(
        state.kernel_reserved(&w.net.bond(0)),
        u128::from(w.policy().claim_collateral),
        "V2 sees the kernel's reservation against the producer's real bond"
    );
    let seats = w.seats(&lie.id);
    assert_eq!(seats.len(), 3, "three distinct-operator seats");
    assert!(!seats.contains(&0), "never the producer");
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];

    // The fresh outsider: the node's read API + the producer's published DA, its own salt.
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x5A);
    assert!(matches!(proof, ProsecutionV1::Kernel(_)), "a kernel fault proof");
    w.proof(outsider, &lie.id, proof).await;

    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "the claim is convicted");
    let slashed = w.policy().claim_collateral;
    assert_eq!(w.net.collateral(0), before - slashed, "the REAL producer bond lost the reservation");
    assert_eq!(w.net.slashed(0), slashed, "recorded as a slash (burn at release)");
    assert_eq!(w.net.kernel_reserved(0), 0, "and the reservation is gone");
    let reward = slashed * u64::from(w.policy().accuser_reward_permille) / 1000;
    assert_eq!(w.net.owed(outsider), reward, "the accuser's reward is queued for the next coinbase");
    // The next block's coinbase pays it (the harness asserts every inserted block UTXO-valid).
    let ttpb = w.net.ttpb();
    let next = w.net.chain.heartbeat(ttpb, Vec::new()).await;
    assert!(
        next.transactions[0].outputs.iter().any(|o| o.value == reward && o.script_public_key == card_payout_spk(outsider)),
        "the coinbase pays the outsider its share of the slash"
    );
    assert_eq!(w.net.owed(outsider), 0, "and the queue is drained");

    // A second node replaying the chain reaches the same roots, rows and deltas.
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **A covered lie after Final, inside the liability horizon, is still convicted** — the producer was paid its (INTERIM) reward at
/// Final; the horizon's proof slashes the whole reservation.
#[tokio::test]
async fn g14_kernel_route_a_covered_lie_is_convicted_after_final_within_the_liability_horizon() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let poster_slashed = w.net.slashed(1);
    let job = w.job().await;
    // GAP-5: posting the job reserved its escrow on the poster's REAL bond (card 1) and burned the fee; the Final reward is paid out
    // of the escrow, never issued.
    let escrow = w.policy().claim_reward;
    assert_eq!(w.net.slashed(1), poster_slashed + w.policy().job_fee, "the posting fee is a real slash of its poster");
    assert_eq!(w.net.ledger().job_escrows[&job.id()].amount, escrow);
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let covered_at = w.net.daa();
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&lie.id) else { panic!("covered") };
    assert_eq!(window_end_daa, covered_at + w.policy().challenge_window_daa);

    // Nobody prosecutes inside the window: the claim finalizes and the producer is paid — out of the job's escrow (GAP-5).
    w.net.beat_to(window_end_daa).await;
    let ClaimStateV1::Final { final_daa } = w.net.claim_state(&lie.id) else { panic!("{:?}", w.net.claim_state(&lie.id)) };
    assert_eq!(final_daa, window_end_daa);
    assert_eq!(w.net.owed(0), w.policy().claim_reward, "the Final reward is queued for the producer's payee");
    assert!(w.net.ledger().job_escrows.is_empty(), "paid out of the job's escrow, which pays once");
    assert!(w.net.ledger().claims[&lie.id].liability_until.is_some(), "and liability runs");

    // The outsider, from the read API and the public DA alone, convicts post-Final.
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x33);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "convicted after Final");
    let slashed = w.policy().claim_collateral;
    assert_eq!(w.net.collateral(0), before - slashed, "the real bond lost the whole reservation");
    assert_eq!(w.net.owed(outsider), slashed * u64::from(w.policy().accuser_reward_permille) / 1000);
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }), "Final stays Final: the conviction is the liability's");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Withheld material is an availability default, never fraud.** The producer publishes everything but the lying value; the
/// outsider (who cannot convict from what it holds) files a signed demand; nobody answers; the producer is defaulted for the fixed
/// penalty, the demander is paid, the claim is Unavailable — and it was never convicted.
#[tokio::test]
async fn g14_kernel_route_a_withheld_position_is_a_demand_then_a_default_never_a_conviction() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];

    let da = lie.published(&w.fx, &[lie.at]);
    let finding = w.fresh(0x77).check(lie.id, &da, &w.fx.params);
    assert_eq!(finding, OutsiderFindingV1::Demand(vec![(0, lie.at.0)]), "no pass, no conviction: one position to demand");
    w.demand(outsider, &lie.id, lie.at.0).await;
    let ledger = w.net.ledger();
    assert!(ledger.demands.contains_key(&(lie.id, 0, lie.at.0)), "the demand is open");
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Disputed { .. }), "an open demand holds Final");
    let deadline = ledger.demands[&(lie.id, 0, lie.at.0)].deadline_daa;
    assert_eq!(w.net.kernel_reserved(outsider), u128::from(w.policy().demand_bond), "the demander's bond is reserved on the real bond");

    // Nobody answers: the deadline's tick is the producer's availability default.
    w.net.beat_to(deadline).await;
    let pol = w.policy();
    let penalty = pol.default_penalty;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }), "{:?}", w.net.claim_state(&lie.id));
    assert!(!w.net.ledger().claims[&lie.id].convicted, "a default is not a conviction");
    assert_eq!(w.net.collateral(0), before - penalty, "the producer pays the fixed penalty, not the fraud slash");
    assert_eq!(w.net.slashed(0), penalty);
    // C4 F-C4R3-02: the rest of the reservation is HELD through the default's liability horizon (a valid proof filed in it would
    // still convict); the penalty is split like a slash.
    assert_eq!(w.net.kernel_reserved(0), u128::from(pol.claim_collateral - penalty), "the rest of the reservation is held");
    assert_eq!(w.net.owed(outsider), default_share(&pol, None), "the sole demander takes the accuser's share of the penalty");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "and its demand bond returns");
    let horizon = w.net.ledger().claims[&lie.id].liability_until.expect("a default sets the claim's liability horizon");
    assert_eq!(horizon, deadline + pol.liability_daa);
    w.net.beat_to(horizon + 1).await;
    assert_eq!(w.net.kernel_reserved(0), 0, "no valid proof arrived: released at the horizon, never slashed for withholding");
    assert!(!w.net.ledger().claims[&lie.id].convicted);
    assert_eq!(w.net.collateral(0), before - penalty);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// **Responses are classified; none of the rejected ones answers the demand**; the deadline then defaults the producer.
#[tokio::test]
async fn g14_kernel_route_malformed_wrong_root_and_fake_opening_responses_are_rejected_then_the_producer_defaults() {
    use misaka_palw_kernel::merkle::TensorOpeningV1;
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    w.demand(outsider, &lie.id, lie.at.0).await;
    let deadline = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].deadline_daa;

    let (p, s, n) = (lie.at.0, lie.at.1 as usize, lie.at.2 as usize);
    let other = lie.trace.values[((p + 1) as usize) % lie.trace.values.len()][s][n].clone();
    let mut fake = TensorOpeningV1::row(&lie.trace.values[p as usize][s][n], 0).unwrap();
    fake.siblings.extend([[0; 64]; 3]);
    let fx = &w.fx;
    let responses: Vec<Vec<u8>> = vec![
        vec![0xFF, 0x00],
        lie.position(fx, p, |r| {
            r.pop();
        }),
        lie.position(fx, p, |r| r[s][n] = MaterialResponseV1::Whole(TensorWireV1::of(&other))),
        lie.position(fx, p, |r| r[s][n] = MaterialResponseV1::Part(TensorOpeningV1::row(&other, 0).unwrap())),
        lie.position(fx, p, |r| r[s][n] = MaterialResponseV1::Part(fake)),
    ];
    let mut items = Vec::new();
    for bytes in responses {
        let o = w.net.route(0, &K::Respond { claim: lie.id, stage: 0, position: p, bytes });
        items.push((0, o));
    }
    w.net.send(items).await;
    let ledger = w.net.ledger();
    assert!(ledger.demands.contains_key(&(lie.id, 0, p)), "no rejected response answered the demand");
    assert!(!ledger.served.contains_key(&(lie.id, 0, p)), "and nothing was served");
    assert!(ledger.demands[&(lie.id, 0, p)].last.is_some(), "the last rejection's class is recorded");
    let before = w.net.collateral(0);
    w.net.beat_to(deadline).await;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }));
    assert!(!w.net.ledger().claims[&lie.id].convicted);
    assert_eq!(w.net.collateral(0), before - w.policy().default_penalty, "a fixed penalty, not the fraud slash");
    assert_eq!(w.net.owed(outsider), default_share(&w.policy(), None));
}

/// **A real response convicts via the served values** (an authentic opening is not an acquittal), and a served position cannot be
/// demanded again.
#[tokio::test]
async fn g14_kernel_route_a_served_position_completes_the_check_and_convicts() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let da = lie.published(&w.fx, &[lie.at]);
    w.demand(outsider, &lie.id, lie.at.0).await;
    w.serve(0, &lie, lie.at.0).await; // the producer answers (it must, or it defaults)
    assert!(w.net.ledger().served.contains_key(&(lie.id, 0, lie.at.0)), "served on chain: public from then on");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bond returns on service");
    let proof = w.prosecution(&lie.id, &da, 0x11);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the served values convicted the claim");
    assert_eq!(w.net.slashed(0), w.policy().claim_collateral);
}

/// **Court pre-emption**: a crowd of demand sessions opened by the producer's friends on most positions never pre-empts a direct
/// proof; the proof convicts in the block that carries it, every session settles moot and every bond returns.
#[tokio::test]
async fn g14_kernel_route_spam_demands_never_preempt_a_direct_proof_and_settle_moot() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let cards = w.outsiders(&lie, &seats, 3);
    let (spam1, spam2, outsider) = (cards[0], cards[1], cards[2]);
    let da = lie.published(&w.fx, &[]);

    let mut items = Vec::new();
    for (card, position) in [(spam1, 0u32), (spam1, 1), (spam2, 2), (spam2, 3), (outsider, 4)] {
        let o = w.net.route(card, &K::FileDemand { demander: w.net.kid(card), claim: lie.id, stage: 0, position });
        items.push((card, o));
    }
    w.net.send(items).await;
    assert_eq!(w.net.ledger().demands.len(), 5, "five sessions are open");
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Disputed { .. }));

    // The outsider's direct proof from public material alone convicts regardless of the crowd.
    let proof = w.prosecution(&lie.id, &da, 0x42);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the direct proof convicts");
    assert!(w.net.ledger().demands.is_empty(), "every open session settled as moot");
    for c in [spam1, spam2, outsider] {
        assert_eq!(w.net.kernel_reserved(c), 0, "card {c}: the demand bond returned");
    }
    assert_eq!(w.net.kernel_reserved(0), 0);
    assert_eq!(w.net.slashed(0), w.policy().claim_collateral, "slashed once");
}

/// **Simultaneous challengers**: two outsiders file proofs in the same block; exactly one is the conviction, the other is a
/// Duplicate; the producer is slashed once and only one reward is queued.
#[tokio::test]
async fn g14_kernel_route_simultaneous_challengers_one_conviction_one_duplicate_and_one_slash() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let cards = w.outsiders(&lie, &seats, 2);
    let da = lie.published(&w.fx, &[]);
    // Each challenger builds its own proof with its own salt, from the same read API.
    let (p1, p2) = (w.prosecution(&lie.id, &da, 0x01), w.prosecution(&lie.id, &da, 0x02));
    let o1 = w.net.route(cards[0], &K::FileProof { accuser: w.net.kid(cards[0]), claim: lie.id, proof: p1 });
    let o2 = w.net.route(cards[1], &K::FileProof { accuser: w.net.kid(cards[1]), claim: lie.id, proof: p2 });
    w.net.send(vec![(cards[0], o1), (cards[1], o2)]).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    let slashed = w.policy().claim_collateral;
    assert_eq!(w.net.collateral(0), before - slashed, "one slash");
    let reward = slashed * u64::from(w.policy().accuser_reward_permille) / 1000;
    let (a, b) = (w.net.owed(cards[0]), w.net.owed(cards[1]));
    assert!(
        (a == reward && b == 0) || (a == 0 && b == reward),
        "exactly one challenger is the accuser: {a} / {b} (reward {reward})"
    );
}

/// **A duplicate proof after replay**: a third challenger refiles the very same proof on a node that replayed the chain; it changes
/// nothing (no second slash, no second reward).
#[tokio::test]
async fn g14_kernel_route_a_duplicate_proof_after_a_replay_changes_nothing() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let cards = w.outsiders(&lie, &seats, 2);
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x09);
    w.proof(cards[0], &lie.id, proof.clone()).await;
    let slashed_once = w.net.slashed(0);
    let claim_row = w.net.ledger().claims[&lie.id].clone();

    // The replaying node extends ITS chain with the duplicate.
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
    let mut zn = w.net.on_chain(z);
    let o = zn.route(cards[1], &K::FileProof { accuser: zn.kid(cards[1]), claim: lie.id, proof });
    zn.send(vec![(cards[1], o)]).await;
    assert_eq!(zn.slashed(0), slashed_once, "no second slash");
    assert_eq!(zn.ledger().claims[&lie.id], claim_row, "the claim row is untouched");
    assert_eq!(zn.owed(cards[1]), 0, "and the duplicate earns nothing");
}

/// **The proof grace**: a demand in the window's last block, served at its deadline, makes Final wait `proof_grace_daa`, so the
/// proof the served values enable convicts BEFORE Final and the producer never earns the reward.
#[tokio::test]
async fn g14_kernel_route_final_waits_the_proof_grace_so_a_late_served_lie_is_convicted_before_final() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let da = lie.published(&w.fx, &[lie.at]);
    let pol = w.policy();
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&lie.id) else { panic!() };

    // The demand folds in the window's last block (the carrier rides one block earlier).
    w.net.beat_to(window_end_daa - 3).await;
    w.demand(outsider, &lie.id, lie.at.0).await;
    assert!(w.net.daa() < window_end_daa, "the demand folded inside the window (DAA is not one per block): {} < {window_end_daa}", w.net.daa());
    let row = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].clone();
    assert!(row.filed_daa + 3 >= window_end_daa, "in the window's last blocks: filed {} of {window_end_daa}", row.filed_daa);
    let deadline = row.deadline_daa;
    assert_eq!(deadline, row.filed_daa + pol.court_deadline_daa);

    // The producer answers at the last moment.
    w.net.beat_to(deadline - 3).await;
    w.serve(0, &lie, lie.at.0).await;
    let served_at = w.net.daa();
    assert!(served_at < deadline, "served before the default");
    assert!(!matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }), "the serving block hands no Final");
    let proof = w.prosecution(&lie.id, &da, 0x21);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.daa() < served_at + pol.proof_grace_daa);
    assert!(w.net.ledger().claims[&lie.id].convicted, "convicted inside the grace");
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Convicted { .. }), "BEFORE Final: {:?}", w.net.claim_state(&lie.id));
    w.net.beat_to(served_at + 3 * pol.proof_grace_daa).await;
    assert_eq!(w.net.owed(0), 0, "the producer never earned the reward");
}

/// **Spam cannot hold Final past a bound**: every position demanded in the window's last block by two bonds; the producer serves
/// each at its deadline; Final lands by `window end + court deadline + proof grace`.
#[tokio::test]
async fn g14_kernel_route_spam_demands_cannot_hold_final_past_window_end_plus_court_deadline_plus_proof_grace() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let honest = w.claim(0, &job, false).await;
    let seats = w.seats(&honest.id);
    w.cover(&honest.id).await;
    let spam = w.outsiders(&honest, &seats, 2);
    let pol = w.policy();
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&honest.id) else { panic!() };
    let positions = {
        let ledger = w.net.ledger();
        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &ledger.claims[&honest.id].body else { panic!() };
        evidence.positions
    };

    w.net.beat_to(window_end_daa - 3 - 2 * u64::from(positions)).await;
    let mut items = Vec::new();
    for p in 0..positions {
        for card in &spam {
            let o = w.net.route(*card, &K::FileDemand { demander: w.net.kid(*card), claim: honest.id, stage: 0, position: p });
            items.push((*card, o));
        }
    }
    w.net.send(items).await;
    assert!(w.net.daa() < window_end_daa, "all the demands landed inside the window");
    assert_eq!(w.net.ledger().demands.len(), positions as usize);
    // Too late: no demand opens or joins past the window.
    w.net.beat_to(window_end_daa).await;
    let late = w.net.route(spam[0], &K::FileDemand { demander: w.net.kid(spam[0]), claim: honest.id, stage: 0, position: 0 });
    w.net.send(vec![(spam[0], late)]).await;
    assert_eq!(w.net.ledger().demands.len(), positions as usize, "no new session past the window");

    // The producer serves everything at the deadline.
    let deadline = w.net.ledger().demands.values().map(|d| d.deadline_daa).max().unwrap();
    w.net.beat_to(deadline - 2 - u64::from(positions)).await;
    let mut items = Vec::new();
    for p in 0..positions {
        let bytes = honest.position(&w.fx, p, |_| {});
        let o = w.net.route(0, &K::Respond { claim: honest.id, stage: 0, position: p, bytes });
        items.push((0, o));
    }
    w.net.send(items).await;
    assert!(w.net.ledger().demands.is_empty(), "every demand was answered");
    w.net.beat_to(window_end_daa + pol.court_deadline_daa + pol.proof_grace_daa + 5).await;
    let ClaimStateV1::Final { final_daa } = w.net.claim_state(&honest.id) else { panic!("{:?}", w.net.claim_state(&honest.id)) };
    assert!(
        final_daa <= window_end_daa + pol.court_deadline_daa + pol.proof_grace_daa,
        "Final at {final_daa} within window end {window_end_daa} + court {} + grace {}",
        pol.court_deadline_daa,
        pol.proof_grace_daa
    );
}

/// **A bond that stands behind a live claim cannot leave**: the kernel refuses its Withdraw while collateral is reserved, V2's
/// duty gate holds it, and both let go only when the liability ends.
#[tokio::test]
async fn g14_kernel_route_a_bond_with_liability_cannot_exit_until_the_horizon_releases_it() {
    use kaspa_consensus_core::palw_state_v2::palw_bond_backs_live_duty_v1;
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let honest = w.claim(0, &job, false).await;
    w.cover(&honest.id).await;
    let kid = w.net.kid(0);
    let key = w.net.bond(0);
    let duty = |w: &World| {
        let state = w.net.chain.tip_state().1;
        palw_bond_backs_live_duty_v1(&state, &key, w.net.daa(), None)
    };
    assert!(duty(&w), "V2's duty gate sees the kernel reservation");

    // The kernel's exit: requested, but the withdrawal waits on the reservation.
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&honest.id) else { panic!() };
    let o = w.net.route(0, &K::RequestExit { bond: kid });
    w.net.send(vec![(0, o)]).await;
    assert!(w.net.ledger().bonds[&kid].exit_requested.is_some());
    let exit_delay = w.policy().exit_delay_daa;
    w.net.beat_to(w.net.daa() + exit_delay + 1).await;
    let o = w.net.route(0, &K::Withdraw { bond: kid });
    w.net.send(vec![(0, o)]).await;
    assert!(w.net.ledger().bonds.contains_key(&kid), "the withdrawal was refused: collateral is still reserved");
    assert!(duty(&w), "still held");

    // Final, then the liability horizon: the reservation is released and the bond may go.
    w.net.beat_to(window_end_daa).await;
    let ClaimStateV1::Final { final_daa } = w.net.claim_state(&honest.id) else { panic!("{:?}", w.net.claim_state(&honest.id)) };
    let horizon = w.net.ledger().claims[&honest.id].liability_until.expect("liability runs");
    assert_eq!(horizon, final_daa + w.policy().liability_daa);
    w.net.beat_to(horizon + 1).await;
    assert_eq!(w.net.kernel_reserved(0), 0, "released at the horizon");
    assert!(!duty(&w), "and V2 no longer holds the bond for it");
    let o = w.net.route(0, &K::Withdraw { bond: kid });
    w.net.send(vec![(0, o)]).await;
    assert!(!w.net.ledger().bonds.contains_key(&kid), "the route forgot the bond");
    assert_eq!(w.net.collateral(0), before, "the real V2 bond is untouched by a kernel Withdraw");
}

// ---- the large-object path: ObjectChunk -------------------------------------------------------------------------

/// **A kernel object larger than one carrier rides `ObjectChunk`s**; its signature is checked on the ASSEMBLED object at the
/// completing chunk. A tampered signature drops the completing chunk (the block stands, the claim is not committed); the genuine
/// object, cut the same way, commits through the same group machinery every other chunked object uses.
#[tokio::test]
async fn g14_kernel_route_a_chunked_object_is_signature_checked_at_the_completing_chunk() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let cap = 4 << 10;
    let bad = w.claim_with(0, &job, true, Delivery::ChunkedTampered(cap)).await;
    assert!(!w.net.ledger().claims.contains_key(&bad.id), "a chunked object whose signature does not verify commits nothing");
    assert_eq!(w.net.kernel_reserved(0), 0, "and reserves nothing");
    let good = w.claim_with(0, &job, true, Delivery::Chunked(cap)).await;
    assert_eq!(good.id, bad.id, "the same claim");
    assert!(w.net.ledger().claims.contains_key(&good.id), "the genuine chunked object commits");
    assert_eq!(w.net.kernel_reserved(0), u128::from(w.policy().claim_collateral));
    // The chunked claim is as prosecutable as a direct one.
    let seats = w.seats(&good.id);
    w.cover(&good.id).await;
    let outsider = w.outsiders(&good, &seats, 1)[0];
    let proof = w.prosecution(&good.id, &good.published(&w.fx, &[]), 0x66);
    w.proof(outsider, &good.id, proof).await;
    assert!(w.net.ledger().claims[&good.id].convicted);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// ---- more than one node: reorg, restart, pruned import ------------------------------------------------------------

/// **Replay and reorg**: a node that replays the chain reaches its roots; a heavier chain that never saw the conviction takes the
/// replaying node back to the covered-but-unprosecuted rows EXACTLY (rows, aux, collateral, payout queue); and the original chain
/// out-working it returns the conviction identically.
#[tokio::test]
async fn g14_kernel_route_replay_and_reorg_reach_the_same_roots() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let fork = w.net.chain.sink();
    let at_fork = w.net.api().expect("the route");
    let collateral = w.net.collateral(0);
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x5A);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);

    // ---- Z replays A ----
    let z = w.net.replay().await;
    w.net.assert_same(&z, "Z on A");
    let mut zn = w.net.on_chain(z);

    // ---- B, from the same fork, mines a heavier chain that never saw the proof ----
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    let mut b_blocks = Vec::new();
    for _ in 0..4 {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "B out-works A's two blocks: Z reorgs onto B");
    assert_eq!(zn.chain.tip_state().1.state_root(), b.tip_state().1.state_root(), "Z on B: the root of B's fresh replay");
    let on_b = zn.api().expect("the route survives (the claim was covered below the fork)");
    assert_eq!((on_b.rows.clone(), on_b.aux.clone()), (at_fork.rows.clone(), at_fork.aux.clone()), "the reorged-out conviction left no row, aux or trace");
    assert!(!zn.ledger().claims[&lie.id].convicted);
    assert_eq!(zn.collateral(0), collateral, "the slash is returned with the reorg");
    assert_eq!(zn.slashed(0), 0);
    assert_eq!(zn.owed(outsider), 0, "and the queued reward");
    assert_eq!(zn.chain.ctx.consensus.palw_kernel_route_v1(), b.ctx.consensus.palw_kernel_route_v1());

    // ---- A out-works B again: Z reorgs back and the conviction returns identically ----
    let old_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len();
    for _ in 0..4 {
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    for blk in chain_blocks(&w.net.chain, w.net.chain.sink()).into_iter().skip(old_len) {
        arrive(&zn.chain, blk, "A's later block").await;
    }
    w.net.assert_same(&zn.chain, "Z back on A");
    assert!(zn.ledger().claims[&lie.id].convicted, "the conviction is back");
}

/// **A real restart**: the node is shut down and reopened over the SAME database. The route's rows, the PALW tip, every delta row
/// and the payout queue come off disk; it carries on (a second claim is convicted after the restart) and a node replaying the whole
/// chain agrees with every root.
#[tokio::test]
async fn g14_kernel_route_survives_a_node_restart_over_the_same_database() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over(|c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on(net).await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x5A);
    w.proof(outsider, &lie.id, proof).await;
    let (sink, root, route) = (w.net.chain.sink(), w.net.chain.tip_state().1.state_root(), w.net.api());
    let owed = w.net.owed(outsider);
    assert!(owed > 0 && w.net.ledger().claims[&lie.id].convicted);
    let deltas: Vec<Hash64> = chain_blocks(&w.net.chain, sink).iter().map(|b| root_at(&w.net.chain, b.header.hash)).collect();

    // ---- stop it, start it again on the same database ----
    let mut w = w.restart(db.clone());
    assert_eq!(w.net.chain.sink(), sink, "the restarted node's sink is the stopped node's");
    assert_eq!(w.net.chain.tip_state().1.state_root(), root, "the PALW tip it loads off disk");
    assert_eq!(w.net.api(), route, "the kernel route's rows, aux and header, off disk");
    assert_eq!(w.net.owed(outsider), owed, "the payout queue");
    for (b, want) in chain_blocks(&w.net.chain, sink).iter().zip(&deltas) {
        assert_eq!(root_at(&w.net.chain, b.header.hash), *want, "the delta row of {} survived", b.header.hash);
    }

    // ---- it carries on: the reward is paid, a second lie is committed, covered and convicted ----
    let ttpb = w.net.ttpb();
    let next = w.net.chain.heartbeat(ttpb, Vec::new()).await;
    assert!(next.transactions[0].outputs.iter().any(|o| o.value == owed && o.script_public_key == card_payout_spk(outsider)));
    let job = w.job().await;
    let lie2 = w.claim(0, &job, true).await;
    let seats = w.seats(&lie2.id);
    w.cover(&lie2.id).await;
    let other = w.outsiders(&lie2, &seats, 1)[0];
    let proof = w.prosecution(&lie2.id, &lie2.published(&w.fx, &[]), 0x6B);
    w.proof(other, &lie2.id, proof).await;
    assert!(w.net.ledger().claims[&lie2.id].convicted, "folded by the restarted node");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying the whole chain");
}

/// **A pruned join with live kernel rows**: the importer follows A through P (a claim committed, its interim assignment and seat
/// bond rows still live), is left as a pruned join leaves a node (no PALW tip, no delta below P) and installs the carriage A
/// serves — the route rides tail 0xEC. Everything it then folds (the receipts, the conviction) gives A's roots.
#[tokio::test]
async fn g14_kernel_route_survives_a_pruned_import() {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let p = w.net.chain.sink();
    let seats = w.seats(&lie.id);
    assert!(w.net.api().unwrap().assignment_of(&lie.id).is_some(), "the interim assignment is live at P");
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x5A);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);

    let all = chain_blocks(&w.net.chain, w.net.chain.sink());
    let k = all.iter().position(|b| b.header.hash == p).expect("P is on the chain");
    let t = all[k + 1].clone();
    let importer = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    for b in &all[..=k] {
        arrive(&importer, b.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;
    let vp = w.net.chain.vp();
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
    let (at, imported) = importer.tip_state();
    assert_eq!(at, p);
    assert_eq!(imported.state_root(), root_at(&w.net.chain, p), "the imported state is P's, root for root");
    let route = imported.kernel_route().expect("the route came through the carriage");
    assert!(route.assignment_of(&lie.id).is_some(), "with its live interim assignment");
    assert!(route.ledger().unwrap().claims.contains_key(&lie.id));
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "A's block after P").await;
        assert_eq!(importer.sink(), blk.header.hash);
        assert_eq!(importer.tip_state().1.state_root(), root_at(&w.net.chain, blk.header.hash), "the importer folds to A's root at {}", blk.header.hash);
    }
    assert_eq!(importer.ctx.consensus.palw_kernel_route_v1(), w.net.api(), "the same route, rows and aux");
    assert!(importer.ctx.consensus.palw_kernel_route_v1().unwrap().ledger().unwrap().claims[&lie.id].convicted);
}

// ---- the per-block adjudication budget -------------------------------------------------------------------------------

/// **The adjudication budget bounds the BLOCK, however the fold is split into single-object rehearsals.** (Tests run with a
/// four-adjudication block.) Five junk class registrations — each decodes, names an attested artifact, charges the block and is then
/// refused for a program nobody can decode — ride one block: the first four spend the budget (refusals still spend it, so junk is
/// never free), the fifth finds it spent and charges nothing; the next block starts afresh.
#[tokio::test]
async fn g14_kernel_route_the_block_adjudication_budget_bounds_the_block_not_each_object() {
    use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1;
    use misaka_palw_kernel::plan::{PlanBudgetsV1, VerificationPlanV1};
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    assert_eq!(w.policy().max_adjudications_per_block, 4, "the test node runs a four-adjudication block");
    let junk = |n: u8| K::RegisterClass {
        descriptor: k2_tir_v2_descriptor().digest(),
        program_bytes: vec![n],
        plan: VerificationPlanV1 {
            grammar: 1,
            descriptor_digest: [0; 64],
            program_root: [0; 64],
            max_positions: 4,
            relations: vec![],
            boundaries: vec![],
            declared_error_bits: 0,
            budgets: PlanBudgetsV1::default(),
        },
        param_commitments: w.fx.pc.clone(),
    };
    let budget = |w: &World| -> Option<(u64, u32, u64)> {
        w.net.chain.tip_state().1.kernel_route().and_then(|k| k.aux_row(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]))
    };
    let classes = w.net.ledger().classes.len();
    let mut items = Vec::new();
    for (n, card) in [2usize, 3, 4, 5, 6].into_iter().enumerate() {
        let o = w.net.route(card, &junk(n as u8 + 1));
        items.push((card, o));
    }
    let blue_before = w.net.chain.tip_state().1.kernel_route().map(|k| k.aux.len());
    w.net.send(items).await;
    let (blue, adjudications, work) = budget(&w).expect("the spent budget is recorded");
    // C4 F-C4R3-05 (round 2): two of the block's four runs are reserved for proofs, so registrations take only the other two.
    assert_eq!((adjudications, work), (2, 0), "two charged; the rest found the admissions' share spent (two runs reserved for proofs)");
    assert_eq!(w.net.ledger().classes.len(), classes, "and none of the junk registered");
    assert!(blue_before.is_some());
    // The next chain block starts a fresh budget: one more junk object is charged once, from zero.
    let o = w.net.route(2, &junk(9));
    w.net.send(vec![(2, o)]).await;
    let (blue_next, adjudications, _) = budget(&w).unwrap();
    assert!(blue_next > blue, "a later block");
    assert_eq!(adjudications, 1, "a fresh block, a fresh budget");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// ---- the public read model behind RPC ops 210 / 211 ---------------------------------------------------------------------

/// **What the RPC serves is enough, and exact**: the open demand and then the served position show up in the claim read; the public
/// record rebuilds a fresh verifier (the kernel's own constructor, from the bytes alone); and the route's rows, gathered page by page
/// with a small budget, rebuild a ledger whose root is the committed one.
#[tokio::test]
async fn g14_kernel_route_the_read_model_serves_a_claim_and_rows_that_rebuild_the_committed_root() {
    use misaka_palw_kernel::evidence::EvidenceHeaderV1;
    use misaka_palw_kernel::public::ServedPositionV1;
    use misaka_palw_kernel::rows::LedgerRowsV1;
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];

    // An open demand is public...
    w.demand(outsider, &lie.id, lie.at.0).await;
    let read = w.net.api().unwrap().claim_read_v1(&lie.id).unwrap().expect("the claim is known");
    assert_eq!((read.kind, read.convicted, read.rewarded), ("program", false, false));
    assert!(read.state.starts_with("Disputed"), "{}", read.state);
    assert_eq!(read.demands.len(), 1);
    assert_eq!((read.demands[0].position, read.demands[0].demanders, read.demands[0].last_rejection.clone()), (lie.at.0, 1, None));
    assert!(read.served.is_empty());
    assert_eq!(read.reserved, w.policy().claim_collateral);
    // ...and so is the position once served.
    w.serve(0, &lie, lie.at.0).await;
    let read = w.net.api().unwrap().claim_read_v1(&lie.id).unwrap().unwrap();
    assert!(read.demands.is_empty());
    assert_eq!(read.served.len(), 1);
    assert_eq!((read.served[0].0, read.served[0].1), (0, lie.at.0));
    borsh::from_slice::<ServedPositionV1>(&read.served[0].2).expect("the served bytes decode");
    assert!(w.net.api().unwrap().claim_read_v1(&[0xAB; 64]).unwrap().is_none(), "an unknown claim is not found");

    // The public record alone builds a fresh verifier (what an outsider does with the RPC's bytes).
    let route = w.net.api().unwrap();
    let template = route.template();
    let header: EvidenceHeaderV1 = borsh::from_slice(&read.record_header).expect("the header decodes");
    misaka_palw_kernel::public::FreshVerifierV1::from_public_bytes(&read.public_record, &template.known, header)
        .expect("a fresh verifier is built from the served public record");

    // The rows, a small page at a time, rebuild the committed ledger.
    let mut rows = LedgerRowsV1::new();
    let (mut cursor, mut pages) = (None, 0);
    let mut seen = 0u64;
    loop {
        let page = route.rows_page_v1(cursor.clone(), 8 << 10);
        pages += 1;
        assert_eq!(page.total_rows, (route.rows.len() + route.aux.len()) as u64);
        for (table, key, row) in page.rows {
            seen += 1;
            if table < kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1 {
                rows.insert((table, key), row);
            }
        }
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert!(pages > 1, "the budget forced several pages");
    assert_eq!(seen, (route.rows.len() + route.aux.len()) as u64, "every row exactly once");
    let rebuilt = KernelLedgerV1::from_rows(&template, route.header.scalars, &rows).expect("the pages rebuild a ledger");
    assert_eq!(rebuilt.root(), route.ledger_root().as_bytes(), "and its root is the committed one");
    assert_eq!(rebuilt.public_record(&lie.id).unwrap().0.to_bytes(), read.public_record, "the same public record");
}

// ---- hostile input -----------------------------------------------------------------------------------------------------

/// **Hostile, signature-valid objects are dropped or dismissed — none fails a block, none panics the fold** (C4's P0: a wire shape
/// whose element count overflows; the kernel's Result paths only). Signed by real bonds so each reaches the kernel: a response for a
/// claim nobody committed, one with no open demand, a proof of 64 junk bytes, a decode fault whose logits shape is `[u64::MAX, 2]`, a
/// pipeline proof for a program claim, a demand at stage 255 / position `u32::MAX`, a seal for no job, an exit for another's bond and
/// a withdrawal of nothing. The chain carries on, the claim is untouched — and the outsider's genuine proof still convicts it. The
/// junk proofs cost their filers the (small) dismissal fee, a real slash of the real bond.
#[tokio::test]
async fn g14_kernel_route_hostile_objects_are_dropped_or_dismissed_and_never_stop_the_chain() {
    use misaka_palw_kernel::job::DecodeFaultV1;
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let cards = w.outsiders(&lie, &seats, 3);
    let (a, b, c) = (cards[0], cards[1], cards[2]);
    let before = w.net.ledger().claims[&lie.id].clone();
    let (a_before, b_before) = (w.net.collateral(a), w.net.collateral(b));
    let fee = w.policy().dismissed_proof_fee;

    let huge = TensorWireV1 { dtype: 0, shape: vec![u64::MAX, 2], bytes: vec![0; 8] };
    let hostile: Vec<(usize, K)> = vec![
        (a, K::Respond { claim: [0x11; 64], stage: 0, position: 0, bytes: vec![0xFF; 4096] }),
        (a, K::Respond { claim: lie.id, stage: 0, position: 3, bytes: vec![0xFF; 64] }),
        (a, K::FileProof { accuser: w.net.kid(a), claim: lie.id, proof: ProsecutionV1::Kernel(vec![0xFF; 64]) }),
        (b, K::FileProof { accuser: w.net.kid(b), claim: lie.id, proof: ProsecutionV1::Decode(DecodeFaultV1 { index: 0, logits: huge }) }),
        (b, K::FileProof { accuser: w.net.kid(b), claim: lie.id, proof: ProsecutionV1::Pipeline(vec![1, 2, 3]) }),
        (c, K::FileDemand { demander: w.net.kid(c), claim: lie.id, stage: 255, position: u32::MAX }),
        (c, K::FileDemand { demander: w.net.kid(c), claim: [0x22; 64], stage: 0, position: 0 }),
        (c, K::SealClaim { producer: w.net.kid(c), job: [0x33; 64], seal: [0x44; 64] }),
        (c, K::RequestExit { bond: w.net.kid(a) }),
        (c, K::Withdraw { bond: [0x55; 64] }),
    ];
    let mut items = Vec::new();
    for (card, object) in &hostile {
        let o = w.net.route(*card, object);
        items.push((*card, o));
    }
    w.net.send(items).await;
    // The claim is untouched and the chain carries on.
    let after = w.net.ledger().claims[&lie.id].clone();
    assert_eq!(after, before, "no hostile object moved the claim");
    assert!(w.net.ledger().demands.is_empty() && !w.net.ledger().claims[&lie.id].convicted);
    // The junk proofs were dismissed against their filers: the fee is a real slash of the real bonds (a: one proof; b: two).
    assert!(w.net.collateral(a) < a_before && w.net.collateral(b) < b_before, "dismissed filings cost the filer");
    assert!(w.net.collateral(a) >= a_before - fee && w.net.collateral(b) >= b_before - 2 * fee, "and no more than the fee each");
    assert_eq!(w.net.slashed(c), 0, "the refused objects cost nothing");
    // The genuine proof still convicts.
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x77);
    w.proof(c, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the outsider still convicts after the noise");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// ---- GAP-5: the Final reward is the poster's escrow, paid once (user-pays escrow) ------------------------------------------

/// What `chain` has actually minted to card `card`'s payee and still holds unspent: every coinbase output of its selected chain paying
/// the payee's script that the virtual UTXO set holds (a reorged-out coinbase holds nothing), plus what the queue still owes it.
fn minted_and_owed(net: &Net, chain: &T12Chain, card: usize) -> (u64, u64) {
    let state = chain.tip_state().1;
    let payload = state.bond(&net.bond(card)).expect("the bond").payout_payload;
    let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes());
    let mut minted = 0u64;
    for b in chain_blocks(chain, chain.sink()) {
        let Some(cb) = b.transactions.first() else { continue };
        for (i, o) in cb.outputs.iter().enumerate() {
            if o.script_public_key == spk
                && chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(cb.id(), i as u32)).is_some()
            {
                minted += o.value;
            }
        }
    }
    let key = palw_kernel_payout_key_v1(&payload);
    let owed = state.pending_payouts_iter().find(|(k, _)| **k == key).map(|(_, p)| p.amount).unwrap_or(0);
    (minted, owed)
}

/// **GAP-5 on the real node (the user's ruling: user-pays escrow).** The poster's escrow is a reservation on its REAL bond and the
/// posting fee a real slash; the Final debits the escrow (a slash of the poster) and queues exactly that for the producer; the coinbase
/// mints it once. Across a replay, a reorg that undoes the Final (B re-finalizes the claim on its own branch) and back, a re-applied
/// claim, and the queue's redemption, the producer is paid exactly ONE reward and the poster debited exactly one escrow and one fee:
/// Σ kernel payouts ≤ Σ kernel debits, nothing issued.
#[tokio::test]
async fn g14_kernel_route_the_final_reward_is_paid_once_out_of_the_posters_escrow_across_reorg_replay_and_redemption() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let pol = w.policy();
    let reward = pol.claim_reward;
    let (slashed0, reserved0) = (w.net.slashed(1), w.net.kernel_reserved(1));
    let job = w.job().await; // card 1 posts every job
    assert_eq!(w.net.kernel_reserved(1), reserved0 + u128::from(reward), "the escrow is reserved on the poster's real bond");
    assert_eq!(w.net.slashed(1), slashed0 + pol.job_fee, "the posting fee is a real slash (burned at release)");
    assert_eq!(w.net.ledger().job_escrows[&job.id()].amount, reward);
    let claim = w.claim(0, &job, false).await;
    w.cover(&claim.id).await;
    let ClaimStateV1::ProbabilisticPass { window_end_daa, .. } = w.net.claim_state(&claim.id) else { panic!("covered") };
    w.net.beat_to(window_end_daa - 1).await;
    let fork = w.net.chain.sink();
    assert!(!matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }));
    w.net.beat_to(window_end_daa).await;
    assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }));
    assert_eq!(minted_and_owed(&w.net, &w.net.chain, 0), (0, reward), "queued once, out of the escrow");
    assert_eq!(w.net.slashed(1), slashed0 + pol.job_fee + reward, "the poster's escrow debited");
    assert_eq!(w.net.kernel_reserved(1), reserved0, "and its reservation spent");
    assert!(w.net.ledger().job_escrows.is_empty());

    // A second node replays; then B, from just before the Final, out-works A: the reorg undoes A's Final and B finalizes the claim on
    // its own branch — still exactly one reward and one debit.
    let z = w.net.replay().await;
    w.net.assert_same(&z, "Z on A");
    let mut zn = w.net.on_chain(z);
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    let mut b_blocks = Vec::new();
    for _ in 0..4 {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "B out-works A: Z reorgs onto B");
    let (minted, owed) = minted_and_owed(&w.net, &zn.chain, 0);
    eprintln!("[GAP-5] on B after the reorg: minted {minted}, owed {owed}, poster slashed {}", zn.slashed(1) - slashed0);
    assert_eq!(minted + owed, reward, "the reorg undid A's Final; B's Final pays once — never twice");
    assert_eq!(zn.slashed(1), slashed0 + pol.job_fee + reward, "and debits the escrow once");
    assert_eq!(zn.chain.ctx.consensus.palw_kernel_route_v1(), b.ctx.consensus.palw_kernel_route_v1());

    // A out-works B again: Z returns to A — one reward, one debit.
    let old_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len();
    for _ in 0..4 {
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    for blk in chain_blocks(&w.net.chain, w.net.chain.sink()).into_iter().skip(old_len) {
        arrive(&zn.chain, blk, "A's later block").await;
    }
    w.net.assert_same(&zn.chain, "Z back on A");
    let (minted, owed) = minted_and_owed(&w.net, &zn.chain, 0);
    assert_eq!((minted, owed), (reward, 0), "redeemed by A's next coinbase, once");
    assert_eq!(zn.slashed(1), slashed0 + pol.job_fee + reward);

    // A re-applied claim (its seal and its reveal carried again) is dropped: nothing is paid twice.
    let again = w.claim_with(0, &job, false, Delivery::Direct).await;
    assert_eq!(again.id, claim.id, "the same claim");
    w.net.beat_to(w.net.daa() + 3).await;
    assert_eq!(minted_and_owed(&w.net, &w.net.chain, 0), (reward, 0), "the queue redeemed it once; a re-application pays nothing");
    assert_eq!(w.net.slashed(1), slashed0 + pol.job_fee + reward, "the poster was debited one escrow");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// ---- C4 F-C4R3-03: the route's own chunk lane (tag 113, aux table 41) ---------------------------------------------------------

/// **The route's own chunk lane, end to end** (C4 F-C4R3-03): a group keyed by its SIGNING opener, the per-bond limit, the deposit
/// held from free collateral (V2 sees it) and returned at completion, a group's TTL bounded by its target's deadline, a group whose
/// assembled object is not a prosecution of its target dropped, an unsigned or mis-signed chunk dropped at acceptance, and an
/// abandoned group's deposit forfeited at its TTL — while the colluders' own groups never touch the outsider's room.
#[tokio::test]
async fn g14_kernel_route_the_routes_own_chunk_lane_is_per_bond_deposit_backed_and_bounded_by_its_target() {
    use kaspa_consensus_core::palw_kernel_route_v1::{
        PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1, PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1, PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1,
        PalwKernelChunkTargetV1 as Target,
    };
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::new().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let cards = w.outsiders(&lie, &seats, 2);
    let (colluder, outsider) = (cards[0], cards[1]);
    let route = |w: &World| w.net.chain.tip_state().1.kernel_route().cloned().expect("the route");

    // The colluder opens its two groups (junk: one part of two each) for the lie — its own room, and only its own.
    let junk: Vec<Obj> = (0..PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1 as u64 + 1)
        .map(|g| {
            let chunk = PalwKernelChunkV1 {
                opener: w.net.bond(colluder),
                group: Hash64::from_u64_word(0x113_0000 + g),
                target: Target::Claim(lie.id),
                index: 0,
                count: 2,
                bytes: vec![g as u8; 64],
            };
            w.net.sign_chunk(colluder, chunk)
        })
        .collect();
    let collateral = w.net.collateral(colluder);
    w.net.send(junk.into_iter().map(|o| (colluder, o)).collect()).await;
    let r = route(&w);
    assert_eq!(
        r.chunk_groups_of_v1(&w.net.bond(colluder)),
        PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1,
        "the per-bond limit refused the third"
    );
    let deposit = 2 * PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1;
    assert_eq!(r.chunk_deposits_of_v1(&w.net.bond(colluder)), PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1 as u64 * deposit);
    assert_eq!(
        w.net.kernel_reserved(colluder),
        u128::from(PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1 as u64 * deposit),
        "V2's committed-collateral ledger holds the deposits"
    );
    let groups = r.chunk_groups_v1();
    let horizon_bound = groups.iter().map(|(_, _, g)| g.expires_daa).max().unwrap();
    assert!(
        groups.iter().all(|(_, _, g)| g.expires_daa <= g.opened_daa + PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1),
        "a group lives at most the lane's TTL"
    );

    // A chunk signed by another key than its opener's is dropped at acceptance; so is a group for a decided target (below).
    let forged = PalwKernelChunkV1 {
        opener: w.net.bond(outsider),
        group: Hash64::from_u64_word(0xF0F0),
        target: Target::Claim(lie.id),
        index: 0,
        count: 2,
        bytes: vec![1; 32],
    };
    let Obj::KernelRouteChunkV1 { signature, .. } = w.net.sign_chunk(colluder, forged.clone()) else { unreachable!() };
    w.net.send(vec![(colluder, Obj::KernelRouteChunkV1 { chunk: Box::new(forged), signature })]).await;
    assert_eq!(route(&w).chunk_groups_of_v1(&w.net.bond(outsider)), 0, "a chunk the opener did not sign opens nothing");

    // A group whose assembled object is not a prosecution of its target (a demand, here) is dropped at its completing chunk.
    let other = w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: lie.id, stage: 0, position: 0 });
    let wrong = w.net.kernel_chunks(outsider, &other, Target::Claim(lie.id), 1024);
    assert!(wrong.len() >= 2);
    w.net.send(wrong.into_iter().map(|c| (outsider, c)).collect()).await;
    assert!(w.net.ledger().demands.is_empty(), "the assembled demand was never applied");
    assert_eq!(route(&w).chunk_groups_of_v1(&w.net.bond(outsider)), 1, "the incomplete group stands (its deposit at stake)");

    // The outsider's genuine chunked proof, in its own room, convicts while the colluder's groups sit in theirs.
    let outsider_before = w.net.collateral(outsider);
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x13);
    let o = w.net.route(outsider, &K::FileProof { accuser: w.net.kid(outsider), claim: lie.id, proof });
    let chunks = w.net.kernel_chunks(outsider, &o, Target::Claim(lie.id), 1024);
    assert!(chunks.len() >= 2, "a genuinely multi-chunk proof: {}", chunks.len());
    w.net.send(chunks.into_iter().map(|c| (outsider, c)).collect()).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the chunked proof convicts");
    assert_eq!(w.net.collateral(outsider), outsider_before, "the completed group's deposit came back (and the proof was true)");

    // A group for a decided target opens nothing (the claim is convicted: nothing a chunk could carry matters any more).
    let late = w.net.kernel_chunks(outsider, &o, Target::Claim(lie.id), 1024);
    w.net.send(vec![(outsider, late[0].clone())]).await;
    assert_eq!(route(&w).chunk_groups_of_v1(&w.net.bond(outsider)), 1, "only the abandoned demand group is open");

    // At their TTL the abandoned groups are dropped and their deposits FORFEITED: junk pays.
    let last_expiry = route(&w).chunk_groups_v1().iter().map(|(_, _, g)| g.expires_daa).max().unwrap();
    assert!(last_expiry >= horizon_bound);
    w.net.beat_to(last_expiry + 1).await;
    let r = route(&w);
    assert!(r.chunk_groups_v1().is_empty(), "every abandoned group expired");
    assert_eq!(w.net.collateral(colluder), collateral - PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1 as u64 * deposit, "the junk's deposits");
    assert_eq!(w.net.kernel_reserved(colluder), 0);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

// ---- RFC-0015 OptimisticPublicVerification on the real node ----------------------------------------------------------------
//
// An OPV network (`Params::palw_panel_free_v1`, activating at DAA 1) carries the OPV policy at genesis; a class registers under the
// mode by tag 13 (the network's admission is the test hook), a claim of it has NO Panel — no seats, no assignment, no receipts — and
// is Challengeable from its inclusion for a fixed window. Everything an outsider needs to stop it is the route's, unchanged.

fn opv_view(w: &World, claim: &Digest) -> (misaka_palw_kernel::opv::OpvPolicyV1, kaspa_consensus_core::palw_kernel_route_v1::KernelOpvReadV1) {
    let route = w.net.api().expect("the route");
    let policy = route.header.opv.expect("the network declares the OPV policy");
    let view = route.claim_read_v1(claim).unwrap().expect("the claim").opv.expect("an OPV claim has the clock view");
    (policy, view)
}

/// **An honest OPV claim finalizes with no Panel** — Challengeable from inclusion, nothing can pass it early, Final exactly when the
/// fixed window closes (the producer paid the INTERIM reward), and the route exports the beacon's fact for it: a Final whose path is
/// `PanelIndependent`, not a proof that the computation is correct.
#[tokio::test]
async fn g14_opv_an_honest_claim_finalizes_with_no_panel_and_exports_a_panel_independent_beacon_fact() {
    use misaka_palw_challenge::{FinalPathV1, WorkFinalEventV1, WorkSourceKindV1};
    use misaka_palw_kernel::opv::{CANONICAL_WORK_DOMAIN_V1, FinalStandingV1};
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let honest = w.claim(0, &job, false).await;
    let route = w.net.api().unwrap();
    assert!(route.assignment_of(&honest.id).is_none() && route.receipts_of(&honest.id).is_empty(), "no Panel: no seats, no receipts");
    let read = route.claim_read_v1(&honest.id).unwrap().unwrap();
    assert_eq!(read.mode, "OptimisticPublicVerification");
    assert!(read.seats.is_empty() && read.state.starts_with("Challengeable"), "{} / {} seats", read.state, read.seats.len());
    let (policy, view) = opv_view(&w, &honest.id);
    assert_eq!(view.reservation, policy.economics.reservation_per_claim);
    assert_eq!(view.final_floor_daa, view.admitted_daa + policy.window_daa());
    assert_eq!(view.hard_deadline_daa, policy.hard_deadline(view.admitted_daa, &w.policy()));
    assert!(view.statement.contains("not a proof that the computation is correct"));
    assert_eq!(w.net.kernel_reserved(0), u128::from(view.reservation), "V2 sees the OPV reservation against the producer's real bond");

    // Nothing passes it before the window closes...
    w.net.beat_to(view.final_floor_daa - 1).await;
    assert!(w.net.claim_state(&honest.id).to_string_state().starts_with("Challengeable"), "{:?}", w.net.claim_state(&honest.id));
    assert!(w.net.api().unwrap().finals_read_v1().unwrap().is_empty(), "and no Final is exported");
    // ...and the window's end finalizes it.
    w.net.beat_to(view.final_floor_daa).await;
    let ClaimStateV1::Final { final_daa } = w.net.claim_state(&honest.id) else { panic!("{:?}", w.net.claim_state(&honest.id)) };
    assert!(final_daa >= view.final_floor_daa && final_daa <= view.hard_deadline_daa);
    assert_eq!(w.net.owed(0), w.policy().claim_reward, "the Final reward is queued for the producer's payee (INTERIM, unfunded: GAP)");

    // The beacon fact.
    let finals = w.net.api().unwrap().finals_read_v1().unwrap();
    assert_eq!(finals.len(), 1);
    let f = &finals[0];
    assert_eq!((f.final_path, f.receipt.claim, f.receipt.standing), ("PanelIndependent", honest.id, FinalStandingV1::Standing));
    let event: WorkFinalEventV1 = borsh::from_slice(f.event.as_ref().expect("an OPV Final carries the beacon's event")).unwrap();
    assert_eq!(event.final_path, FinalPathV1::PanelIndependent);
    assert_eq!(event.kind, WorkSourceKindV1::RealUsefulWork);
    assert!(event.claim_final && event.da_satisfied && event.validity_independent && event.depends_on_profiles.is_empty());
    assert_eq!((event.accepted_position, event.settlement_position), (view.admitted_daa, final_daa));
    assert_eq!(event.source_profile_id, w.class);
    assert_eq!(event.canonical_work_id, misaka_palw_kernel::hash::object_id(CANONICAL_WORK_DOMAIN_V1, &(w.class, job.id())));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// A lying OPV claim — with no Panel to cover it — is convicted pre-Final by a fresh outsider built from the read API alone.
#[tokio::test]
async fn g14_opv_a_lying_claim_is_convicted_by_a_fresh_outsider_before_final() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let outsider = w.outsiders(&lie, &[], 1)[0];
    let (policy, view) = opv_view(&w, &lie.id);
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x5A);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Convicted { .. }));
    let slashed = policy.economics.reservation_per_claim;
    let fee = policy.economics.admission_fee; // C4 F-C4R3-05: every OPV admission pays a non-refundable fee
    assert_eq!(w.net.collateral(0), before - slashed - fee, "the real bond lost the whole OPV reservation (and the admission fee)");
    assert_eq!(w.net.owed(outsider), slashed * u64::from(w.policy().accuser_reward_permille) / 1000);
    assert_eq!(w.net.kernel_reserved(0), 0);
    w.net.beat_to(view.hard_deadline_daa + 2).await;
    assert!(w.net.api().unwrap().finals_read_v1().unwrap().is_empty(), "a convicted claim never finalizes");
    assert_eq!(w.net.owed(0), 0, "and is never paid");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// A lying OPV claim nobody prosecuted in the window finalizes; the liability horizon still convicts it, and the Final then reads
/// `ConvictedAfterFinal` with `claim_final: false` — the beacon will not take it.
#[tokio::test]
async fn g14_opv_a_lie_that_finalized_is_convicted_within_liability_and_its_fact_is_withdrawn() {
    use misaka_palw_challenge::WorkFinalEventV1;
    use misaka_palw_kernel::opv::FinalStandingV1;
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let outsider = w.outsiders(&lie, &[], 1)[0];
    let (policy, view) = opv_view(&w, &lie.id);
    w.net.beat_to(view.final_floor_daa).await;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }), "nobody prosecuted: Final");
    let f = w.net.api().unwrap().finals_read_v1().unwrap();
    let event: WorkFinalEventV1 = borsh::from_slice(f[0].event.as_ref().unwrap()).unwrap();
    assert!(event.claim_final, "at Final the fact stands");

    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x33);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "convicted after Final");
    let slashed = policy.economics.reservation_per_claim;
    assert_eq!(w.net.collateral(0), before - slashed - policy.economics.admission_fee);
    let f = w.net.api().unwrap().finals_read_v1().unwrap();
    assert_eq!(f[0].receipt.standing, FinalStandingV1::ConvictedAfterFinal);
    let event: WorkFinalEventV1 = borsh::from_slice(f[0].event.as_ref().unwrap()).unwrap();
    assert!(!event.claim_final, "the withdrawn fact fails the beacon's eligibility by its own reason");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// Withheld material is an availability default on an OPV claim too — with a share of the penalty burned — and the squatted job is
/// free again: an honest producer's claim for it is accepted.
#[tokio::test]
async fn g14_opv_withheld_material_defaults_the_producer_burns_a_share_and_frees_the_job() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let before = w.net.collateral(0);
    let lie = w.claim(0, &job, true).await;
    let outsider = w.outsiders(&lie, &[], 1)[0];
    let da = lie.published(&w.fx, &[lie.at]);
    assert_eq!(w.fresh(0x77).check(lie.id, &da, &w.fx.params), OutsiderFindingV1::Demand(vec![(0, lie.at.0)]));
    w.demand(outsider, &lie.id, lie.at.0).await;
    let deadline = w.net.ledger().demands[&(lie.id, 0, lie.at.0)].deadline_daa;
    w.net.beat_to(deadline).await;
    let pol = w.policy();
    let (opv, _) = opv_view(&w, &lie.id);
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }));
    assert!(!w.net.ledger().claims[&lie.id].convicted);
    assert_eq!(w.net.collateral(0), before - pol.default_penalty - opv.economics.admission_fee, "the penalty and the admission fee");
    assert_eq!(w.net.owed(outsider), default_share(&pol, Some(&opv)), "the demander is paid what the default did not burn");
    // C4 F-C4R3-02: the rest of the reservation is held through the default's liability horizon.
    assert_eq!(w.net.kernel_reserved(0), u128::from(opv.economics.reservation_per_claim - pol.default_penalty));

    // The job is free: an honest producer's claim for it is accepted.
    let honest = w.claim(4, &job, false).await;
    assert!(w.net.claim_state(&honest.id).to_string_state().starts_with("Challengeable"), "{:?}", w.net.claim_state(&honest.id));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// Spam cannot hold an OPV claim's Final past its hard deadline: every position demanded in the window's last block by two bonds,
/// each served at its deadline.
#[tokio::test]
async fn g14_opv_spam_demands_cannot_hold_final_past_the_hard_deadline() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let honest = w.claim(0, &job, false).await;
    let spam = w.outsiders(&honest, &[], 2);
    let (_, view) = opv_view(&w, &honest.id);
    let positions = {
        let ledger = w.net.ledger();
        let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &ledger.claims[&honest.id].body else { panic!() };
        evidence.positions
    };
    w.net.beat_to(view.final_floor_daa - 3 - 2 * u64::from(positions)).await;
    let mut items = Vec::new();
    for p in 0..positions {
        for card in &spam {
            let o = w.net.route(*card, &K::FileDemand { demander: w.net.kid(*card), claim: honest.id, stage: 0, position: p });
            items.push((*card, o));
        }
    }
    w.net.send(items).await;
    assert!(w.net.daa() < view.final_floor_daa, "all the demands landed inside the window");
    assert_eq!(w.net.ledger().demands.len(), positions as usize);
    let deadline = w.net.ledger().demands.values().map(|d| d.deadline_daa).max().unwrap();
    w.net.beat_to(deadline - 2 - u64::from(positions)).await;
    let mut items = Vec::new();
    for p in 0..positions {
        let bytes = honest.position(&w.fx, p, |_| {});
        let o = w.net.route(0, &K::Respond { claim: honest.id, stage: 0, position: p, bytes });
        items.push((0, o));
    }
    w.net.send(items).await;
    assert!(w.net.ledger().demands.is_empty());
    w.net.beat_to(view.hard_deadline_daa + 3).await;
    let ClaimStateV1::Final { final_daa } = w.net.claim_state(&honest.id) else { panic!("{:?}", w.net.claim_state(&honest.id)) };
    assert!(final_daa <= view.hard_deadline_daa, "Final at {final_daa} within the hard deadline {}", view.hard_deadline_daa);
}

/// **The mode is dropped unless the fence and the network's admission say so.** On a network that declares no OPV policy a tag-13
/// registration is dropped by name; on an OPV network a class the policy did not admit, and a "registration under PanelLicensed" by
/// the wrong tag, are refused; the legacy route keeps working beside the mode, and a Panel-covered lie there is still convicted.
#[tokio::test]
async fn g14_opv_registration_is_dropped_without_the_fence_the_admission_or_the_right_tag_and_the_legacy_route_coexists() {
    kaspa_core::log::try_init_logger("warn");
    let d = k2_tir_v2_descriptor();
    let opv_register = |fx: &Fixture, mode: VerificationModeV1, plan: misaka_palw_kernel::VerificationPlanV1| K::RegisterClassV2 {
        mode,
        descriptor: d.digest(),
        program_bytes: fx.program.encode(),
        plan,
        param_commitments: fx.pc.clone(),
    };

    // (a) A network that declares no OPV policy: dropped by name (the fence), the legacy class is the only one.
    let mut w = World::new().await;
    assert!(w.net.api().unwrap().header.opv.is_none());
    let before = w.net.api().unwrap().rows.clone();
    let o = w.net.route(2, &opv_register(&w.fx, VerificationModeV1::OptimisticPublicVerification, w.fx.plan.clone()));
    w.net.send(vec![(2, o)]).await;
    assert_eq!(w.net.ledger().classes.len(), 1, "no OPV class on a network without the policy");
    assert_eq!(w.net.api().unwrap().rows, before, "and the route's rows are untouched (the historical root form)");

    // (b) An OPV network: only the admitted class registers, and only by tag 13 under a non-legacy mode.
    let mut w = World::opv().await;
    let plan48 = plan_for_tir_program_v1(&d, &w.fx.program, program_root_v1(&w.fx.program.encode()), 48).unwrap();
    let o = w.net.route(2, &opv_register(&w.fx, VerificationModeV1::OptimisticPublicVerification, plan48));
    w.net.send(vec![(2, o)]).await;
    assert_eq!(w.net.ledger().classes.len(), 1, "a class the network's policy did not admit is refused: no registrant choice");
    let o = w.net.route(2, &opv_register(&w.fx, VerificationModeV1::PanelLicensed, w.fx.plan.clone()));
    w.net.send(vec![(2, o)]).await;
    assert_eq!(w.net.ledger().classes.len(), 1, "the legacy mode has one registration path (tag 1)");

    // (c) The legacy route beside the mode: the same program under tag 1 is another class, with the Panel's lifecycle.
    let legacy = K::RegisterClass {
        descriptor: d.digest(),
        program_bytes: w.fx.program.encode(),
        plan: w.fx.plan.clone(),
        param_commitments: w.fx.pc.clone(),
    };
    let o = w.net.route(1, &legacy);
    w.net.send(vec![(1, o)]).await;
    assert_eq!(w.net.ledger().classes.len(), 2, "two classes: the same program under two modes");
    let opv_class = w.class;
    let panel_class = *w.net.ledger().classes.keys().find(|c| **c != opv_class).unwrap();
    assert_eq!(w.net.ledger().mode_of_class(&panel_class), VerificationModeV1::PanelLicensed);
    w.class = panel_class;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let outsider = w.outsiders(&lie, &seats, 1)[0];
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x21);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "a Panel-covered lie of the legacy class is convicted beside the mode");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay");
}

/// OPV state across a reorg: the conviction (and the OPV rows it moved) is undone exactly by a heavier branch and returns
/// identically when the original out-works it again.
#[tokio::test]
async fn g14_opv_replay_and_reorg_reach_the_same_roots() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = World::opv().await;
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let outsider = w.outsiders(&lie, &[], 1)[0];
    let fork = w.net.chain.sink();
    let at_fork = w.net.api().expect("the route");
    // (the claim's admission fee is already a slash at the fork: C4 F-C4R3-05)
    let (collateral, slashed) = (w.net.collateral(0), w.net.slashed(0));
    let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0x5A);
    w.proof(outsider, &lie.id, proof).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);

    let z = w.net.replay().await;
    w.net.assert_same(&z, "Z on A");
    let mut zn = w.net.on_chain(z);
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    let mut b_blocks = Vec::new();
    for _ in 0..4 {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "B out-works A: Z reorgs onto B");
    let on_b = zn.api().unwrap();
    assert_eq!((on_b.rows.clone(), on_b.aux.clone()), (at_fork.rows.clone(), at_fork.aux.clone()), "no OPV row, no trace of the conviction");
    assert!(!zn.ledger().claims[&lie.id].convicted && zn.ledger().opv_invariants().is_ok());
    assert_eq!((zn.collateral(0), zn.slashed(0), zn.owed(outsider)), (collateral, slashed, 0));
    assert_eq!(zn.chain.ctx.consensus.palw_kernel_route_v1(), b.ctx.consensus.palw_kernel_route_v1());

    let old_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len();
    for _ in 0..4 {
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    for blk in chain_blocks(&w.net.chain, w.net.chain.sink()).into_iter().skip(old_len) {
        arrive(&zn.chain, blk, "A's later block").await;
    }
    w.net.assert_same(&zn.chain, "Z back on A");
    assert!(zn.ledger().claims[&lie.id].convicted);
}

/// OPV state across a real restart: the rows come off disk, the claim finalizes after the restart on the restarted node, and a node
/// replaying the whole chain agrees with every root.
#[tokio::test]
async fn g14_opv_survives_a_node_restart_and_finalizes_after_it() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(kernel_config_opv(opv_admitted()), |c| TestConsensus::with_db(db.clone(), c, sender));
    net._keep.push(Box::new(receiver));
    let mut w = World::on_opv(net).await;
    let job = w.job().await;
    let honest = w.claim(0, &job, false).await;
    let (sink, root, route) = (w.net.chain.sink(), w.net.chain.tip_state().1.state_root(), w.net.api());
    let (_, view) = opv_view(&w, &honest.id);

    let mut w = w.restart(db.clone());
    assert_eq!(w.net.chain.sink(), sink);
    assert_eq!(w.net.chain.tip_state().1.state_root(), root, "the PALW tip off disk");
    assert_eq!(w.net.api(), route, "the OPV route's rows, aux and header off disk");
    assert!(w.net.ledger().opv_invariants().is_ok() && w.net.ledger().opv.claims.contains_key(&honest.id));

    w.net.beat_to(view.final_floor_daa).await;
    assert!(matches!(w.net.claim_state(&honest.id), ClaimStateV1::Final { .. }), "finalized by the restarted node");
    assert_eq!(w.net.api().unwrap().finals_read_v1().unwrap().len(), 1);
    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying the whole chain");
}


// ---- phase 3: model onboarding on the real node --------------------------------------------------------------------------
//
// A V2 IR class is registered by card 1 over the sketch program and its weights; the kernel route learns the artifact ONLY through the
// registrant's bonded, refutable binding (tag 104) — the cfg(test) attestation hook is never told about these weights.

/// testnet-12 as launched with the route's fence, the IR fence and the signed-registration fence armed at genesis (all WITHOUT their
/// validation, which refuses them by design).
fn kernel_config_onboarding() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    kernel_config_onboarding_with(true)
}

/// [`kernel_config_onboarding`] with or without the signed-registration envelope's fence.
fn kernel_config_onboarding_with(envelope_fence: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
    params.sync_palw_tir_v1();
    params.palw_signed_registration_v1 = envelope_fence.then(|| ForkActivation::new(0));
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    params.skip_proof_of_work = true;
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences; only this harness bypasses it");
    (Config::new(params), bundle, premine, floats)
}

/// A fixture whose weights exist BOTH as a V2 inventory (the class's registered `artifact_root`) and as kernel commitments (`pc`).
struct OnbFixture {
    program: TirProgramV1,
    params: MapParams,
    plan: misaka_palw_kernel::VerificationPlanV1,
    pc: ParamCommitmentsV1,
    class: kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1,
    artifact_root: Hash64,
}

fn onb_fixture(seed: u64) -> OnbFixture {
    use super::g14_registration_e2e::{Tensors, layout_of};
    use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
    use kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1;
    use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
    use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1};
    let fx = wide128_v1(seed);
    let mut program = fx.program.clone();
    program.logits_scheme_id.copy_from_slice(tiled_logits_scheme_id_v1().as_byte_slice());
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &program, program_root_v1(&program.encode()), MAX_POSITIONS).expect("a K2-TIR-v2 plan");
    let pc = ParamCommitmentsV1::of(&fx.params);
    let mut class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: kaspa_consensus_core::palw_tir_class_v1::PalwTirLayoutV1 {
            version: kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1,
            max_context: MAX_POSITIONS,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: Vec::new(),
            state_tiles: Vec::new(),
        },
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    };
    class.layout = layout_of(&class, MAX_POSITIONS);
    let tensors = fx.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect();
    let ops = palw_tir_inventory_operands_v1(&program, &Tensors(tensors)).expect("the inventory");
    let artifact_root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root");
    OnbFixture { program, params: fx.params, plan, pc, class, artifact_root }
}

impl Net {
    /// **A V2 IR class registration for `f`**, signed by card `card` (the SDK's `build_tir_registration_v1`, over the chain's own pricing).
    fn v2_registration(&self, f: &OnbFixture, card: usize, activation_daa: u64) -> Obj {
        use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1;
        use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
        use kaspa_consensus_core::palw_tir_class_v1::{PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, palw_tir_class_registration_message_v1};
        let state = self.chain.tip_state().1;
        let floor = self.bundle.base_class_id;
        let (target, slash, ladder) =
            (state.class_target(&floor).unwrap().target, state.class(&floor).unwrap().slash_value_per_pwu, self.bundle.court.max_step_leaf_count());
        let facts = PalwTirJobFactsV1::of_class(&f.class, f.class.class_id(&f.artifact_root)).expect("decodes");
        let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&f.class).expect("wide enough"));
        let mut o = palw_tir_post_genesis_registration_v1(
            f.class.clone(),
            canonical,
            f.artifact_root,
            0,
            target,
            slash,
            activation_daa,
            self.bond(card),
            Vec::new(),
            ladder,
        )
        .expect("the builder counts the canonical job");
        let Obj::ClassRegisteredTirV1 { class_id, share_permille, activation_daa, artifact_root, slash_value_per_pwu, initial_target, pwu_rule, admission } =
            &mut o
        else {
            unreachable!()
        };
        let message = palw_tir_class_registration_message_v1(
            self.domain,
            *class_id,
            *share_permille,
            *activation_daa,
            &admission.registrant_bond,
            *artifact_root,
            *slash_value_per_pwu,
            *initial_target,
            pwu_rule,
            &admission.canonical,
            &admission.class,
        );
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        admission.signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1,
            [0x61u8; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec();
        o
    }

    /// An onboarding object signed by card `card` (`kind` is the object's tag; `payload` its Borsh without the signature).
    fn onboarding_signature(&mut self, card: usize, kind: u8, payload: &[u8]) -> Vec<u8> {
        self.rnd = self.rnd.wrapping_add(1);
        let signer = self.bond(card);
        let message = kaspa_consensus_core::palw_onboarding_v1::palw_onboarding_message_v1(self.domain, kind, &signer, payload);
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_MLDSA87_CONTEXT_V1,
            [self.rnd; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
    }

    fn artifact_bound(&mut self, card: usize, v2_class: Hash64, kernel_param_root: Hash64) -> Obj {
        let payload = borsh::to_vec(&(v2_class, kernel_param_root)).unwrap();
        let signature = self.onboarding_signature(card, 104, &payload);
        Obj::ArtifactBoundV1 { v2_class, kernel_param_root, signer: self.bond(card), signature }
    }

    fn artifact_challenged(
        &mut self,
        card: usize,
        v2_class: Hash64,
        kernel_param_root: Hash64,
        proof: kaspa_consensus_core::palw_onboarding_v1::ArtifactMismatchProofV1,
    ) -> Obj {
        let payload = borsh::to_vec(&(v2_class, kernel_param_root, &proof)).unwrap();
        let signature = self.onboarding_signature(card, 105, &payload);
        Obj::ArtifactBindingChallengedV1 { v2_class, kernel_param_root, challenger: self.bond(card), proof: Box::new(proof), signature }
    }

    fn kernel_bound(&mut self, card: usize, v2_class: Hash64, kernel_class: Hash64) -> Obj {
        let policy = Hash64::from_bytes(self.api().expect("the route").header.policy.challenge_policy_id);
        let payload = borsh::to_vec(&(v2_class, kernel_class, policy)).unwrap();
        let signature = self.onboarding_signature(card, 106, &payload);
        Obj::KernelBoundV1 { v2_class, kernel_class, challenge_policy_id: policy, signer: self.bond(card), signature }
    }

    fn conformance_committed(&mut self, card: usize, commitment: misaka_palw_challenge::ConformanceCommitmentV1) -> Obj {
        let payload = borsh::to_vec(&commitment).unwrap();
        let signature = self.onboarding_signature(card, 107, &payload);
        Obj::ConformanceCommittedV1 { commitment: Box::new(commitment), signer: self.bond(card), signature }
    }
}

/// The RFC-0013 statement of the bound class (what the SDK's pack writes), over the chain's own roots.
fn conformance_of(net: &Net, f: &OnbFixture, v2_class: Hash64) -> misaka_palw_challenge::ConformanceCommitmentV1 {
    use misaka_palw_challenge::{ConformanceCommitmentV1, RootV1, SubjectKindV1};
    let route = net.api().expect("the route");
    let binding = route.kernel_binding_v1(&v2_class).expect("the class is kernel-bound");
    let policy = &route.header.policy;
    ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: net.config.params.genesis.hash.as_bytes(),
        ruleset_id: policy.ruleset_digest,
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: v2_class.as_bytes(),
        kernel_descriptor_id: k2_tir_v2_descriptor().digest(),
        challenge_policy_id: binding.challenge_policy_id.as_bytes(),
        artifact_root: f.artifact_root.as_bytes(),
        program_root: program_root_v1(&f.program.encode()),
        source_root: RootV1::Absent,
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: [0x11; 64],
        verification_plan_root: f.plan.root(),
        constraint_root: RootV1::Absent,
        implementation_set_root: [0x22; 64],
        test_scope_root: [0x33; 64],
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: [0x44; 64],
        commitment_object_id: None,
        canonical_commitment_position: None,
    }
}

/// **The whole onboarding path, no hook**: the V2 class registers; its registrant binds the artifact (bonded); the kernel route refuses
/// a class over the root while the binding is Pending and registers one when it has matured; the registrant binds the V2 class to
/// that kernel class and commits the RFC-0013 statement; the class — due to activate long before — stays Registered until the
/// binding passes its refutation horizon, and then leaves it.
#[tokio::test]
async fn g14_onboarding_a_class_is_bound_attested_registered_and_released_by_the_gate() {
    use kaspa_consensus_core::palw_onboarding_v1::ArtifactBindingStateV1;
    use kaspa_consensus_core::palw_state_v2::PalwClassStatusV2;
    kaspa_core::log::try_init_logger("warn");
    let f = onb_fixture(11);
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let (registrant, collateral) = (1usize, net.collateral(1));

    // ---- the V2 class registers (due to activate 30 DAA after it lands) ----
    let class_obj = net.v2_registration(&f, registrant, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id: v2_class, .. } = &class_obj else { unreachable!() };
    let v2_class = *v2_class;
    net.send(vec![(registrant, class_obj)]).await;
    let state = net.chain.tip_state().1;
    assert!(matches!(state.class(&v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }), "the V2 class registered");
    assert_eq!(state.class(&v2_class).unwrap().artifact_root, f.artifact_root);
    let kernel_root = Hash64::from_bytes(f.pc.root());

    // ---- no binding: the route will not register a class over this artifact ----
    let d = k2_tir_v2_descriptor();
    let register = |f: &OnbFixture| K::RegisterClass {
        descriptor: d.digest(),
        program_bytes: f.program.encode(),
        plan: f.plan.clone(),
        param_commitments: f.pc.clone(),
    };
    let o = net.route(registrant, &register(&f));
    net.send(vec![(registrant, o)]).await;
    assert!(net.ledger().classes.is_empty(), "no hook, no binding: the artifact is not attested");

    // ---- the registrant binds the artifact (bonded) ----
    let o = net.artifact_bound(registrant, v2_class, kernel_root);
    net.send(vec![(registrant, o)]).await;
    let route = net.api().unwrap();
    let row = route.artifact_binding_v1(&v2_class, &kernel_root).expect("the binding row");
    assert_eq!(row.reserved, kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1);
    assert_eq!(row.state_at(net.daa()), ArtifactBindingStateV1::Pending);
    assert!(route.onboarding_attested_roots_v1(net.daa()).is_empty(), "Pending attests nothing");
    assert_eq!(net.chain.tip_state().1.onboarding_reserved(&net.bond(registrant)), u128::from(row.reserved), "V2 sees the reservation");
    let o = net.route(registrant, &register(&f));
    net.send(vec![(registrant, o)]).await;
    assert!(net.ledger().classes.is_empty(), "a Pending binding attests nothing: still no kernel class");

    // ---- the window passes: the root is attested and the kernel class registers ----
    net.beat_to(row.matures_daa).await;
    let o = net.route(registrant, &register(&f));
    net.send(vec![(registrant, o)]).await;
    let kernel_class = *net.ledger().classes.keys().next().expect("the kernel class registered over the matured binding");
    assert_eq!(net.api().unwrap().onboarding_attested_roots_v1(net.daa()), vec![kernel_root]);

    // ---- bound to the V2 class; the statement committed ----
    let o = net.kernel_bound(registrant, v2_class, Hash64::from_bytes(kernel_class));
    net.send(vec![(registrant, o)]).await;
    let kb = net.api().unwrap().kernel_binding_v1(&v2_class).expect("kernel-bound");
    assert_eq!((kb.kernel_class, kb.kernel_param_root), (Hash64::from_bytes(kernel_class), kernel_root));
    // The class was due 30 DAA after registration and the window has passed: the gate holds it (binding not Final, no commitment).
    assert!(matches!(net.chain.tip_state().1.class(&v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }));
    let commitment = conformance_of(&net, &f, v2_class);
    let statement = Hash64::from_bytes(commitment.statement_root());
    let o = net.conformance_committed(registrant, commitment);
    net.send(vec![(registrant, o)]).await;
    assert_eq!(net.api().unwrap().conformance_v1(&v2_class, &f.artifact_root).unwrap().statement_root, statement);
    // Still Registered: the binding is inside its refutation horizon.
    assert!(matches!(net.chain.tip_state().1.class(&v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }));
    assert!(matches!(
        net.api().unwrap().onboarding_gate_v1(&v2_class, &f.artifact_root, net.daa()),
        kaspa_consensus_core::palw_onboarding_v1::PalwOnboardingGateV1::Held { .. }
    ));

    // The read (RPC op 230's source) says where the class stands and why it waits.
    let read = net.chain.ctx.consensus.palw_onboarding_v1(v2_class).expect("the class is known");
    assert_eq!((read.artifact_bindings.len(), read.kernel_binding.is_some(), read.conformance.is_some()), (1, true, true));
    assert_eq!(read.artifact_bindings[0].2, "Matured");
    assert!(matches!(read.gate, kaspa_consensus_core::palw_onboarding_v1::PalwOnboardingGateV1::Held { code: "AVAILABILITY_REQUIRED", .. }), "{:?}", read.gate);

    // ---- the horizon ends: the reservation is released and the class leaves Registered ----
    net.beat_to(row.final_daa + 1).await;
    let read = net.chain.ctx.consensus.palw_onboarding_v1(v2_class).unwrap();
    assert_eq!(read.artifact_bindings[0].2, "Final");
    assert_eq!(read.gate, kaspa_consensus_core::palw_onboarding_v1::PalwOnboardingGateV1::Ready);
    assert!(read.status.starts_with("Active"), "{}", read.status);
    assert_eq!(net.chain.tip_state().1.onboarding_reserved(&net.bond(registrant)), 0, "released at the horizon");
    assert!(
        matches!(net.chain.tip_state().1.class(&v2_class).unwrap().status, PalwClassStatusV2::Active),
        "{:?}",
        net.chain.tip_state().1.class(&v2_class).unwrap().status
    );
    assert_eq!(net.collateral(registrant), collateral - kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, "only the registration burn left the bond");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// A refutation of the binding of `wrong`'s kernel commitments to `truth`'s V2 class: a row of the wrong tensor opened against its
/// commitment, and the V2 inventory leaf of the TRUE bytes at the same coordinates — two authenticated answers that disagree.
fn row_mismatch_proof(truth: &OnbFixture, wrong: &OnbFixture) -> kaspa_consensus_core::palw_onboarding_v1::ArtifactMismatchProofV1 {
    use super::g14_registration_e2e::Tensors;
    use kaspa_consensus_core::palw_onboarding_v1::ArtifactMismatchProofV1;
    use kaspa_consensus_core::palw_tir_artifact_v1::{palw_tir_leaf_index_v1, palw_tir_open_leaf_v1};
    use misaka_palw_kernel::merkle::{LayoutV1, TensorOpeningV1};
    assert_eq!(truth.program, wrong.program, "the two artifacts are weights of one program");
    let source = Tensors(truth.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
    for ((j, layer), wrong_t) in &wrong.params.tensors {
        let true_t = &truth.params.tensors[&(*j, *layer)];
        let l = LayoutV1::of(&true_t.shape);
        let w = true_t.dtype.width() as u64;
        let (tb, wb) = (true_t.to_le_bytes(), wrong_t.to_le_bytes());
        for r in 0..l.rows {
            let (a, b) = ((r * l.row_len * w) as usize, ((r + 1) * l.row_len * w) as usize);
            if tb[a..b] != wb[a..b] {
                let index = palw_tir_leaf_index_v1(&truth.program, *j, *layer, a as u64).expect("a leaf holds the bytes");
                return ArtifactMismatchProofV1::Row {
                    commitments: wrong.pc.clone(),
                    param: *j,
                    layer: *layer,
                    kernel_row: TensorOpeningV1::row(wrong_t, r).unwrap(),
                    v2_opening: palw_tir_open_leaf_v1(&truth.program, &source, index).expect("an opening"),
                };
            }
        }
    }
    panic!("the two artifacts are identical")
}

/// The V2 class of `f` registered by card 1 and its artifact bound (by `bind_pc`'s root) in the same go; returns `(class id, root)`.
async fn registered_and_bound(net: &mut Net, f: &OnbFixture, bind_pc: &ParamCommitmentsV1) -> (Hash64, Hash64) {
    let registrant = 1usize;
    let class_obj = net.v2_registration(f, registrant, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id, .. } = &class_obj else { unreachable!() };
    let v2_class = *class_id;
    net.send(vec![(registrant, class_obj)]).await;
    let root = Hash64::from_bytes(bind_pc.root());
    let o = net.artifact_bound(registrant, v2_class, root);
    net.send(vec![(registrant, o)]).await;
    assert!(net.api().unwrap().artifact_binding_v1(&v2_class, &root).is_some(), "the binding row exists");
    (v2_class, root)
}

/// **A false binding is refuted by two disagreeing openings** — the binder binds the kernel commitments of OTHER weights to its class;
/// an outsider (another operator) opens the same row under both roots; the binder's reservation is slashed, the challenger paid, the
/// root is never attested, and the V2 class cannot leave Registered. The same proof against an HONEST binding proves nothing.
#[tokio::test]
async fn g14_onboarding_a_false_artifact_binding_is_refuted_by_two_disagreeing_openings() {
    use kaspa_consensus_core::palw_onboarding_v1::{
        ArtifactBindingStateV1, ArtifactMismatchProofV1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as RESERVED,
        PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1 as REWARD, verify_artifact_mismatch_v1,
    };
    kaspa_core::log::try_init_logger("warn");
    let (truth, wrong) = (onb_fixture(11), onb_fixture(12));
    assert_ne!(truth.artifact_root, wrong.artifact_root);
    let proof = row_mismatch_proof(&truth, &wrong);

    // The proof against the TRUE binding proves nothing (the verifier is pure; it is the fold's gate).
    let program = &truth.program;
    let honest_rows = {
        use super::g14_registration_e2e::Tensors;
        use kaspa_consensus_core::palw_tir_artifact_v1::{palw_tir_leaf_index_v1, palw_tir_open_leaf_v1};
        let source = Tensors(truth.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
        let (&(j, layer), t) = truth.params.tensors.iter().next().unwrap();
        let index = palw_tir_leaf_index_v1(program, j, layer, 0).unwrap();
        ArtifactMismatchProofV1::Row {
            commitments: truth.pc.clone(),
            param: j,
            layer,
            kernel_row: misaka_palw_kernel::merkle::TensorOpeningV1::row(t, 0).unwrap(),
            v2_opening: palw_tir_open_leaf_v1(program, &source, index).unwrap(),
        }
    };
    assert_eq!(
        verify_artifact_mismatch_v1(program, truth.artifact_root, Hash64::from_bytes(truth.pc.root()), &honest_rows),
        Err("the two openings agree on every byte they share"),
        "an honest binding cannot be refuted"
    );
    assert_eq!(
        verify_artifact_mismatch_v1(program, truth.artifact_root, Hash64::from_bytes(wrong.pc.root()), &proof),
        Ok(()),
        "the false binding is refuted by the proof"
    );

    // ---- on the chain ----
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let (binder, challenger) = (1usize, 3usize);
    let before = net.collateral(binder);
    let (v2_class, wrong_root) = registered_and_bound(&mut net, &truth, &wrong.pc).await;
    let burn = kaspa_consensus_core::palw_state_v2::PALW_CLASS_REGISTRATION_BURN_SOMPI_V1;
    assert_eq!(net.chain.tip_state().1.onboarding_reserved(&net.bond(binder)), u128::from(RESERVED));

    // The binder cannot refute itself for the bounty.
    let o = net.artifact_challenged(binder, v2_class, wrong_root, proof.clone());
    net.send(vec![(binder, o)]).await;
    assert!(!net.api().unwrap().artifact_binding_v1(&v2_class, &wrong_root).unwrap().refuted, "not by its own operator");
    // A proof that does not hold is dropped (the honest rows against this binding's root do not root to it).
    let o = net.artifact_challenged(challenger, v2_class, wrong_root, honest_rows.clone());
    net.send(vec![(challenger, o)]).await;
    assert!(!net.api().unwrap().artifact_binding_v1(&v2_class, &wrong_root).unwrap().refuted);

    // The genuine refutation — a whole commitment map and two openings: delivered in ObjectChunks, judged on the assembled whole.
    let o = net.artifact_challenged(challenger, v2_class, wrong_root, proof);
    let chunks = kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, 6 << 10)
        .expect("chunks")
        .expect("a refutation is larger than one chunk");
    assert!(chunks.len() >= 2, "{} chunks", chunks.len());
    net.send(chunks.into_iter().map(|c| (challenger, c)).collect()).await;
    let row = net.api().unwrap().artifact_binding_v1(&v2_class, &wrong_root).unwrap();
    assert!(row.refuted && row.reserved == 0);
    assert_eq!(row.state_at(net.daa()), ArtifactBindingStateV1::Refuted);
    assert_eq!(net.collateral(binder), before - burn - RESERVED, "the binder lost its reservation (and paid the registration burn)");
    assert_eq!(net.slashed(binder), burn + RESERVED);
    assert_eq!(net.owed(challenger), RESERVED * REWARD / 1000, "the challenger is paid its share");
    assert_eq!(net.chain.tip_state().1.onboarding_reserved(&net.bond(binder)), 0);

    // The root is never attested: not even after the window, and the V2 class stays Registered.
    let d = k2_tir_v2_descriptor();
    net.beat_to(row.matures_daa + 1).await;
    assert!(net.api().unwrap().onboarding_attested_roots_v1(net.daa()).is_empty());
    let o = net.route(
        binder,
        &K::RegisterClass { descriptor: d.digest(), program_bytes: wrong.program.encode(), plan: wrong.plan.clone(), param_commitments: wrong.pc.clone() },
    );
    net.send(vec![(binder, o)]).await;
    assert!(net.ledger().classes.is_empty(), "no kernel class over a refuted root");
    net.beat_to(row.final_daa + 40).await;
    assert!(
        matches!(net.chain.tip_state().1.class(&v2_class).unwrap().status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Registered { .. }),
        "the class never left Registered: it began onboarding and its binding is refuted"
    );
    // A second refutation of the same binding is refused.
    let again = row_mismatch_proof(&truth, &wrong);
    let o = net.artifact_challenged(challenger, v2_class, wrong_root, again);
    net.send(vec![(challenger, o)]).await;
    assert_eq!(net.slashed(binder), burn + RESERVED, "slashed once");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// A binding whose commitments miss (or add) a tensor the program declares is refuted by the instance-set proof alone.
#[tokio::test]
async fn g14_onboarding_a_binding_over_the_wrong_set_of_tensors_is_refuted_without_any_opening() {
    use kaspa_consensus_core::palw_onboarding_v1::ArtifactMismatchProofV1;
    kaspa_core::log::try_init_logger("warn");
    let truth = onb_fixture(11);
    let mut short = truth.pc.clone();
    short.by_instance.pop_first();
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let (v2_class, root) = registered_and_bound(&mut net, &truth, &short).await;
    // The honest commitments (the full set) are not a refutation of THEIR OWN binding.
    let o = net.artifact_challenged(3, v2_class, root, ArtifactMismatchProofV1::Instances { commitments: truth.pc.clone() });
    net.send(vec![(3, o)]).await;
    assert!(!net.api().unwrap().artifact_binding_v1(&v2_class, &root).unwrap().refuted, "commitments that do not root to the bound root prove nothing");
    let o = net.artifact_challenged(3, v2_class, root, ArtifactMismatchProofV1::Instances { commitments: short });
    net.send(vec![(3, o)]).await;
    assert!(net.api().unwrap().artifact_binding_v1(&v2_class, &root).unwrap().refuted, "a missing tensor refutes the binding");
}

/// The onboarding objects refuse what they should: a non-registrant, a program that is not the class's, a policy the network did not
/// fix, a second kernel binding, a statement under another plan, a duplicate statement.
#[tokio::test]
async fn g14_onboarding_refusals_leave_the_rows_untouched() {
    kaspa_core::log::try_init_logger("warn");
    let f = onb_fixture(11);
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let registrant = 1usize;
    let class_obj = net.v2_registration(&f, registrant, net.daa() + 400);
    let Obj::ClassRegisteredTirV1 { class_id, .. } = &class_obj else { unreachable!() };
    let v2_class = *class_id;
    net.send(vec![(registrant, class_obj)]).await;
    let kernel_root = Hash64::from_bytes(f.pc.root());
    let route_rows = |net: &Net| net.api().map(|r| r.aux).unwrap_or_default();

    // A bond that is not the class's registrant cannot speak for its artifact; neither can anyone bind an unknown class.
    let before = route_rows(&net);
    let o = net.artifact_bound(2, v2_class, kernel_root);
    net.send(vec![(2, o)]).await;
    let o = net.artifact_bound(registrant, Hash64::from_bytes([0x99; 64]), kernel_root);
    net.send(vec![(registrant, o)]).await;
    assert_eq!(route_rows(&net), before, "not the registrant / no such class: nothing written");

    // The registrant binds; a second, different root is refused while the first stands.
    let o = net.artifact_bound(registrant, v2_class, kernel_root);
    net.send(vec![(registrant, o)]).await;
    let bound = route_rows(&net);
    let o = net.artifact_bound(registrant, v2_class, Hash64::from_bytes([0x55; 64]));
    net.send(vec![(registrant, o)]).await;
    let o = net.artifact_bound(registrant, v2_class, kernel_root);
    net.send(vec![(registrant, o)]).await;
    assert_eq!(route_rows(&net), bound, "one live binding per class; a duplicate is refused");

    // Mature, register the kernel class, then the kernel binding's refusals.
    let matures = net.api().unwrap().artifact_binding_v1(&v2_class, &kernel_root).unwrap().matures_daa;
    net.beat_to(matures).await;
    let d = k2_tir_v2_descriptor();
    let o = net.route(
        registrant,
        &K::RegisterClass { descriptor: d.digest(), program_bytes: f.program.encode(), plan: f.plan.clone(), param_commitments: f.pc.clone() },
    );
    net.send(vec![(registrant, o)]).await;
    let kernel_class = Hash64::from_bytes(*net.ledger().classes.keys().next().expect("registered over the matured binding"));
    let before = route_rows(&net);
    // another operator's bond cannot bind; an unknown kernel class; a policy the network did not fix
    let o = net.kernel_bound(2, v2_class, kernel_class);
    net.send(vec![(2, o)]).await;
    let o = net.kernel_bound(registrant, v2_class, Hash64::from_bytes([0x77; 64]));
    net.send(vec![(registrant, o)]).await;
    let policy_payload = borsh::to_vec(&(v2_class, kernel_class, Hash64::from_bytes([0xEE; 64]))).unwrap();
    let signature = net.onboarding_signature(registrant, 106, &policy_payload);
    let bad_policy = Obj::KernelBoundV1 {
        v2_class,
        kernel_class,
        challenge_policy_id: Hash64::from_bytes([0xEE; 64]),
        signer: net.bond(registrant),
        signature,
    };
    net.send(vec![(registrant, bad_policy)]).await;
    assert_eq!(route_rows(&net), before, "wrong signer / unknown kernel class / another challenge policy: nothing written");
    // No statement before the kernel binding.
    let early = conformance_of_unbound(&net, &f, v2_class);
    let o = net.conformance_committed(registrant, early);
    net.send(vec![(registrant, o)]).await;
    assert_eq!(route_rows(&net), before, "a conformance statement needs the kernel binding first");

    // The kernel binding lands; a second one (a plan or program substitution) is refused.
    let o = net.kernel_bound(registrant, v2_class, kernel_class);
    net.send(vec![(registrant, o)]).await;
    let bound = route_rows(&net);
    let o = net.kernel_bound(registrant, v2_class, kernel_class);
    net.send(vec![(registrant, o)]).await;
    assert_eq!(route_rows(&net), bound, "one kernel binding per class: a plan cannot be substituted afterwards");

    // Statements under another plan root / another artifact root / another policy are refused; the right one lands once.
    let good = conformance_of(&net, &f, v2_class);
    for edit in [
        (|c: &mut misaka_palw_challenge::ConformanceCommitmentV1| c.verification_plan_root = [9; 64]) as fn(&mut _),
        |c| c.challenge_policy_id = [8; 64],
        |c| c.artifact_root = [7; 64],
        |c| c.program_root = [6; 64],
        |c| c.kernel_descriptor_id = [5; 64],
        |c| c.chain_genesis = [4; 64],
        |c| c.ruleset_id = [3; 64],
    ] {
        let mut bad = good.clone();
        edit(&mut bad);
        let o = net.conformance_committed(registrant, bad);
        net.send(vec![(registrant, o)]).await;
        assert_eq!(route_rows(&net), bound, "a statement that is not the chain's own roots is refused");
    }
    let o = net.conformance_committed(registrant, good.clone());
    net.send(vec![(registrant, o)]).await;
    let committed = route_rows(&net);
    assert_ne!(committed, bound);
    let o = net.conformance_committed(registrant, good);
    net.send(vec![(registrant, o)]).await;
    assert_eq!(route_rows(&net), committed, "one statement per (class, artifact root)");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

/// A conformance statement for a class that is not kernel-bound yet: [`conformance_of`] needs the binding, this does not.
fn conformance_of_unbound(net: &Net, f: &OnbFixture, v2_class: Hash64) -> misaka_palw_challenge::ConformanceCommitmentV1 {
    use misaka_palw_challenge::{ConformanceCommitmentV1, RootV1, SubjectKindV1};
    let route = net.api().expect("the route");
    ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: net.config.params.genesis.hash.as_bytes(),
        ruleset_id: route.header.policy.ruleset_digest,
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: v2_class.as_bytes(),
        kernel_descriptor_id: k2_tir_v2_descriptor().digest(),
        challenge_policy_id: route.header.policy.challenge_policy_id,
        artifact_root: f.artifact_root.as_bytes(),
        program_root: program_root_v1(&f.program.encode()),
        source_root: RootV1::Absent,
        tokenizer_or_input_schema_root: RootV1::Absent,
        layout_root: [0x11; 64],
        verification_plan_root: f.plan.root(),
        constraint_root: RootV1::Absent,
        implementation_set_root: [0x22; 64],
        test_scope_root: [0x33; 64],
        calibration_id: RootV1::Absent,
        input_and_state_binding_root: RootV1::Absent,
        resource_profile_id: [0x44; 64],
        commitment_object_id: None,
        canonical_commitment_position: None,
    }
}

// ---- the signed-registration envelope (RFC-0009 G-EXPIRY / G-RULESET) ---------------------------------------------------------

impl Net {
    /// An envelope around `registration`, signed by card `card` for the ruleset `params_id` and valid until `valid_until_daa`.
    fn envelope(&mut self, card: usize, registration: Obj, valid_until_daa: u64, params_id: kaspa_consensus_core::Hash) -> Obj {
        self.rnd = self.rnd.wrapping_add(1);
        let signer = self.bond(card);
        let bytes = borsh::to_vec(&registration).unwrap();
        let message = kaspa_consensus_core::palw_onboarding_v1::palw_signed_registration_message_v1(
            self.domain,
            params_id,
            valid_until_daa,
            &signer,
            &bytes,
        );
        let key = TestConsensus::palw_v2_registry_keypair(card as u64);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            message.as_byte_slice(),
            kaspa_consensus_core::palw_onboarding_v1::PALW_SIGNED_REGISTRATION_MLDSA87_CONTEXT_V1,
            [self.rnd; 32],
        )
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec();
        Obj::SignedRegistrationV1 { registration: Box::new(registration), valid_until_daa, consensus_params_id: params_id, signer, signature }
    }
}

/// **A signed expiry and a signed ruleset**: a leaked signed registration dies at its `valid_until_daa`; one signed for another ruleset
/// is not valid here; one signed by a bond that is not the registrant is not a registration; the fence off, there is no envelope. The
/// good one lands as the registration it wraps.
#[tokio::test]
async fn g14_onboarding_a_signed_registration_envelope_expires_and_binds_its_ruleset() {
    use kaspa_consensus_core::palw_state_v2::PalwClassStatusV2;
    kaspa_core::log::try_init_logger("warn");
    let f = onb_fixture(11);
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let registrant = 1usize;
    let params_id = net.config.params.consensus_params_id();
    let registered = |net: &Net, c: &Hash64| net.chain.tip_state().1.class(c).is_some();
    let class_obj = net.v2_registration(&f, registrant, net.daa() + 30);
    let Obj::ClassRegisteredTirV1 { class_id, .. } = &class_obj else { unreachable!() };
    let v2_class = *class_id;

    // expired: valid until a DAA already behind the chain
    let expired = net.envelope(registrant, class_obj.clone(), net.daa().saturating_sub(1), params_id);
    net.send(vec![(registrant, expired)]).await;
    assert!(!registered(&net, &v2_class), "G-EXPIRY: an expired envelope is dropped");
    // another ruleset
    let mut other = *params_id.as_bytes().as_slice().first().unwrap_or(&0);
    other ^= 0xFF;
    let mut other_id = params_id.as_bytes();
    other_id[0] = other;
    let wrong_ruleset = net.envelope(registrant, class_obj.clone(), net.daa() + 200, kaspa_consensus_core::Hash::from_bytes(other_id));
    net.send(vec![(registrant, wrong_ruleset)]).await;
    assert!(!registered(&net, &v2_class), "G-RULESET: an envelope signed for another ruleset is dropped");
    // a signer that is not the registrant of the wrapped registration
    let wrong_signer = net.envelope(2, class_obj.clone(), net.daa() + 200, params_id);
    net.send(vec![(2, wrong_signer)]).await;
    assert!(!registered(&net, &v2_class), "the envelope's signer must be the wrapped registration's registrant");
    // a tampered signature
    let mut tampered = net.envelope(registrant, class_obj.clone(), net.daa() + 200, params_id);
    if let Obj::SignedRegistrationV1 { signature, .. } = &mut tampered {
        let last = signature.len() - 1;
        signature[last] ^= 1;
    }
    net.send(vec![(registrant, tampered)]).await;
    assert!(!registered(&net, &v2_class), "a forged envelope is dropped");
    // a tampered expiry (the signature covers it)
    let mut extended = net.envelope(registrant, class_obj.clone(), net.daa() + 3, params_id);
    if let Obj::SignedRegistrationV1 { valid_until_daa, .. } = &mut extended {
        *valid_until_daa += 1_000;
    }
    net.send(vec![(registrant, extended)]).await;
    assert!(!registered(&net, &v2_class), "an extended expiry breaks the signature");

    // the good one
    let good = net.envelope(registrant, class_obj, net.daa() + 200, params_id);
    net.send(vec![(registrant, good)]).await;
    assert!(matches!(net.chain.tip_state().1.class(&v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }), "registered through the envelope");

    // and with the fence off the envelope is no object at all
    let mut plain = Net::over_cfg(kernel_config_onboarding_with(false), TestConsensus::new);
    plain.beat_to(1).await;
    let class_obj = plain.v2_registration(&f, registrant, plain.daa() + 30);
    let params_id = plain.config.params.consensus_params_id();
    let envelope = plain.envelope(registrant, class_obj, plain.daa() + 200, params_id);
    plain.send(vec![(registrant, envelope)]).await;
    assert!(!registered(&plain, &v2_class), "without palw_signed_registration_v1 an envelope is dropped by name");
    let z = net.replay().await;
    net.assert_same(&z, "replay");
}

// C4 round 3 (independent adversarial review on the real node): `g14_kernel_route_e2e/c4r3.rs`.
mod c4r3;
