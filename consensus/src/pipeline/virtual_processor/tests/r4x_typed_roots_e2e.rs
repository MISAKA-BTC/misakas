//! **RFC-0004 Part II on the real node: computation specifications with typed roots** — `Weights` byte for byte, a `Memory` class
//! (per-step roots, a lie in one step convicted by an outsider, a withheld pre-state defaulted, memory carried across two jobs), a
//! `Retrieval` class (a wrong item and a missed better item convicted, a withheld snapshot slice defaulted) and a `Composite` class
//! with one verified tool stage convicted at the stage that lied — each through the mempool, the node's own template and the chain
//! block's fold, with the real bonds slashed and a second node replaying the chain to the same roots.
//!
//! **The fences are armed WITHOUT their validation** through the harness's `Config` seam (as `g14_kernel_route_e2e` does):
//! `palw_probabilistic_constraints_v1` (the route), `palw_panel_free_v1` (OPV: every typed class is OptimisticPublicVerification) and
//! `palw_typed_roots_v1` (this lane) are refused by `validate_palw_v2` on every real height. Artifact and snapshot attestation is the
//! route's test-only hook (`kernel_route_test_attest_artifact_v1`): there is no on-chain availability fact yet (GAP, onboarding).
//!
//! Every outsider here is built from the node's read API (the rows and the committed root) and a public DA directory — nothing the
//! producer keeps privately — with its own salt.
use super::g14_kernel_route_e2e::{assert_kernel_conserved_v1, kernel_money_v1};
use super::g14_registration_e2e::{arrive, chain_blocks, root_at};
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_genesis_chain_on, t12_with_harness_cards,
};
use crate::consensus::test_consensus::TestConsensus;
use crate::pipeline::virtual_processor::processor::kernel_route_test_attest_artifact_v1;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_payout_key_v1,
    palw_kernel_route_message_v1, palw_kernel_route_template_opv_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{KernelLedgerV1, OutsiderFindingV1, ProsecutionV1, PublicArtifactV1, PublicSourceV1, claim_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::program_root_v1;
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
use misaka_palw_kernel::rows::{LedgerRowsV1, TABLE_ATTESTED_V1, config_root_of, root_of_rows};
use misaka_palw_kernel::spec::composite::{
    CompositeClaimV1, CompositeJobV1, CompositeRootV1, CompositeStageV1, QuerySourceV1, StageClaimV1, StageInputV1, TokenSourceV1,
};
use misaka_palw_kernel::spec::memory::{MemoryJobV1, MemoryRootV1, MemorySlotV1, memory_root_v1, slot_commitments_v1};
use misaka_palw_kernel::spec::outsider::{SnapshotSourceV1, SpecOutsiderV1};
use misaka_palw_kernel::spec::produce::{MemoryProductionV1, produce_memory_v1, produce_model_stage_v1};
use misaka_palw_kernel::spec::retrieval::{
    IndexV1, RetrievalClaimV1, RetrievalFaultV1, RetrievalItemV1, RetrievalJobV1, RetrievalRootV1, RetrievalRuleV1, SnapshotDataV1,
    payload_digest_v1,
};
use misaka_palw_kernel::spec::{
    ComputationSpecV1, MEMORY_PRE_STATE_STAGE_V1, SNAPSHOT_STAGE_BASE_V1, SpecClaimV1, SpecClassKindV1, SpecFaultV1, SpecJobV1,
    SpecObjectV1, TypedRootV1, WeightsRootV1, k2_tr_v1_descriptor,
};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1};
use misaka_palw_tir::{MapParams, Prim, Tensor};
use misaka_palw_tir_sketch::fixture::{TirSketchFixtureV1, memory_v1, wide128_v1};
use std::collections::BTreeMap;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

const FEE: u64 = 2_000_000;
const MAX_POSITIONS: u32 = 64;
const OPV: VerificationModeV1 = VerificationModeV1::OptimisticPublicVerification;

// ---- the classes (pure functions: their ids are what the test seam names OPV-eligible) ------------------------------------------

fn weights(fx: &TirSketchFixtureV1) -> WeightsRootV1 {
    let d = k2_tir_v2_descriptor();
    WeightsRootV1 {
        descriptor: d.digest(),
        program_bytes: fx.program.encode(),
        plan: plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), MAX_POSITIONS).unwrap(),
        param_commitments: ParamCommitmentsV1::of(&fx.params),
    }
}

fn memory_fx() -> TirSketchFixtureV1 {
    memory_v1(21)
}

/// The memory class: `memory_v1`'s rule, slot `mem0` (param 1, layer 0) ↔ the `StateWrite` of state 0 in layer 0, 8 steps at most.
fn memory_spec() -> ComputationSpecV1 {
    let fx = memory_fx();
    let w = weights(&fx);
    let slots = vec![MemorySlotV1 { param: (1, Some(0)), state: (0, Some(0)) }];
    let m0 = slot_commitments_v1(&slots, &w.param_commitments).unwrap();
    let root = MemoryRootV1 {
        extension: k2_tr_v1_descriptor().digest(),
        line: [0x5E; 64],
        initial_root: memory_root_v1(&slots, &m0),
        slots,
        max_steps: 8,
    };
    ComputationSpecV1 { version: 1, mode: OPV, roots: vec![TypedRootV1::WeightsV1(w), TypedRootV1::MemoryV1(root)] }
}

fn corpus() -> SnapshotDataV1 {
    let items = (0..40usize)
        .map(|i| RetrievalItemV1 {
            key: (0..4usize).map(|j| ((i * 7 + j * 3 + i * j) % 13) as i32 - 6).collect(),
            payload: vec![(i % 31) as u32, ((i * 5 + 1) % 31) as u32],
        })
        .collect();
    SnapshotDataV1::new(items, 4, 2, 8).unwrap()
}

fn retrieval_spec(k: u16) -> ComputationSpecV1 {
    let root = RetrievalRootV1 {
        extension: k2_tr_v1_descriptor().digest(),
        snapshot: corpus().snapshot,
        index: IndexV1::Flat,
        rule: RetrievalRuleV1::TopKCountingV1 { k, score_bits: 24 },
    };
    ComputationSpecV1 { version: 1, mode: OPV, roots: vec![TypedRootV1::RetrievalV1(root)] }
}

/// The composite's model: `wide128_v1` as a Weights class (Panel-licensed; a component is judged by its own courts in any mode).
fn model_spec() -> ComputationSpecV1 {
    ComputationSpecV1 {
        version: 1,
        mode: VerificationModeV1::PanelLicensed,
        roots: vec![TypedRootV1::WeightsV1(weights(&wide128_v1(7)))],
    }
}

/// Retrieval (the verified tool, k = 2, query from the job) → the model (prompt = job prompt ‖ the retrieved payloads, 2 tokens).
fn composite_spec() -> ComputationSpecV1 {
    let stages = vec![
        CompositeStageV1 { component: retrieval_spec(2).class_id().unwrap(), input: StageInputV1::Query(QuerySourceV1::JobQuery) },
        CompositeStageV1 {
            component: model_spec().class_id().unwrap(),
            input: StageInputV1::Tokens {
                sources: vec![TokenSourceV1::JobPrompt, TokenSourceV1::StagePayloads { stage: 0 }],
                max_new_tokens: 2,
            },
        },
    ];
    ComputationSpecV1 {
        version: 1,
        mode: OPV,
        roots: vec![TypedRootV1::CompositeV1(CompositeRootV1 { extension: k2_tr_v1_descriptor().digest(), stages })],
    }
}

/// testnet-12 as launched, harness cards, the route + OPV + typed-roots fences armed WITHOUT their validation (module doc).
fn typed_config(typed: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    // OPV eligibility is DERIVED from chain state (OPV-BOOT), and a `Memory` / `Retrieval` / `Composite` class has no onboarding path
    // yet (GAP-B16): these mechanics worlds name their typed classes through the processor's `cfg(test)` seam, exactly as the
    // pre-derivation OPV worlds do. The fence's list is a DENY-list (nothing denied here), never an admission list.
    for s in [memory_spec(), retrieval_spec(3), retrieval_spec(2), composite_spec()] {
        crate::pipeline::virtual_processor::processor::kernel_route_test_opv_eligible_v1(Hash64::from_bytes(s.class_id().unwrap()));
    }
    params.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new()));
    params.palw_typed_roots_v1 = typed.then(|| ForkActivation::new(1));
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    params.skip_proof_of_work = true;
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences; only this harness bypasses it");
    // The artifacts and snapshots this file treats as public (the route's test-only attestation hook).
    let mfx = memory_fx();
    for root in
        [ParamCommitmentsV1::of(&mfx.params).root(), corpus().snapshot.root(), ParamCommitmentsV1::of(&wide128_v1(7).params).root()]
    {
        kernel_route_test_attest_artifact_v1(Hash64::from_bytes(root), 0);
    }
    (Config::new(params), bundle, premine, floats)
}

// ---- the network (the real node, its mempool, its template, its fold) -------------------------------------------------------

struct Net {
    chain: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
    funding: Vec<(TransactionOutpoint, UtxoEntry)>,
    domain: Hash64,
    rnd: u8,
}

impl Net {
    async fn new(typed: bool) -> Net {
        let (config, bundle, premine, floats) = typed_config(typed);
        let chain = t12_genesis_chain_on(TestConsensus::new(&config), &config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let funding = floats.clone();
        let mut net = Net { chain, config, bundle, premine, floats, funding, domain, rnd: 0 };
        net.beat_to(1).await;
        net
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

    fn spec(&mut self, card: usize, object: SpecObjectV1) -> Obj {
        self.route(card, &K::Spec { object })
    }

    fn carrier(&mut self, card: usize, object: &Obj) -> Transaction {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
            .expect("serializes");
        let (outpoint, entry) = self.funding[card].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - FEE, card_payout_spk(card))],
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

    /// Each object through the mempool and into the node's own template; folded by the block after its carrying block.
    async fn send(&mut self, items: Vec<(usize, Obj)>) {
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
        for txs in &waves {
            for tx in txs {
                self.mempool(tx).unwrap_or_else(|e| panic!("the mempool takes the carrier: {e}"));
            }
            let carrying = self.chain.heartbeat(ttpb, txs.clone()).await;
            for tx in txs {
                assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the node's template carries the kernel carrier");
            }
        }
        self.chain.heartbeat(ttpb, Vec::new()).await;
    }

    async fn beat_to(&mut self, daa: u64) {
        let ttpb = self.ttpb();
        while self.daa() < daa {
            self.chain.heartbeat(ttpb, Vec::new()).await;
        }
    }

    fn api(&self) -> Option<PalwKernelRouteStateV1> {
        self.chain.ctx.consensus.palw_kernel_route_v1()
    }

    /// **A fresh verifier's ledger**: rebuilt from the rows the read API serves, refused unless they root to the committed root.
    fn ledger(&self) -> KernelLedgerV1 {
        let api = self.api().expect("the route exists");
        let ledger = api.ledger().expect("the stored rows rebuild");
        assert_eq!(ledger.root(), api.ledger_root().as_bytes(), "the served rows root to the committed ledger root");
        ledger
    }

    fn collateral(&self, card: usize) -> u64 {
        self.chain.tip_state().1.bond(&self.bond(card)).expect("the bond").collateral
    }

    fn slashed(&self, card: usize) -> u64 {
        self.chain.tip_state().1.bond(&self.bond(card)).expect("the bond").slashed
    }

    fn owed(&self, card: usize) -> u64 {
        let state = self.chain.tip_state().1;
        let payee = state.bond(&self.bond(card)).unwrap().payout_payload;
        let key = palw_kernel_payout_key_v1(&payee);
        state.pending_payouts_iter().find(|(k, _)| **k == key).map(|(_, p)| p.amount).unwrap_or(0)
    }

    fn state_of(&self, claim: &Digest) -> ClaimStateV1 {
        self.ledger().claims[claim].life.state.clone()
    }

    async fn register(&mut self, spec: ComputationSpecV1) -> Digest {
        let class = spec.class_id().unwrap();
        let o = self.spec(1, SpecObjectV1::RegisterClass { spec });
        self.send(vec![(1, o)]).await;
        let l = self.ledger();
        assert!(l.typed.classes.contains_key(&class) || l.classes.contains_key(&class), "the class registered through the real path");
        class
    }

    async fn post(&mut self, job: SpecJobV1) -> Digest {
        let id = job.id();
        let o = self.spec(1, SpecObjectV1::PostJob { job });
        self.send(vec![(1, o)]).await;
        assert!(self.ledger().typed.jobs.contains_key(&id), "the job posted");
        id
    }

    /// Seal one block, then reveal (signed by the claim's producer card).
    async fn commit(&mut self, card: usize, claim: SpecClaimV1) -> Digest {
        let (id, job) = (claim.id(), claim.job_id());
        // Past `palw_panel_free_v1` (OPV-BOOT GAP-B1a) the seal is salted and the reveal carries the salt; before it, unsalted.
        let ledger = self.ledger();
        let salted = ledger.salted_seals_from().is_some_and(|at| ledger.daa.saturating_add(1) >= at);
        let salt = misaka_palw_kernel::hash::id(b"misaka-palw/test/claim-salt", &id);
        let seal = if salted { misaka_palw_kernel::ledger::claim_seal_v2(&id, &salt) } else { claim_seal_v1(&id) };
        let seal = K::SealClaim { producer: self.kid(card), job, seal };
        let o = self.route(card, &seal);
        self.send(vec![(card, o)]).await;
        let o = if salted {
            self.route(card, &K::CommitClaimSalted { salt, commit: misaka_palw_kernel::ledger::SaltedCommitV1::Spec { claim } })
        } else {
            self.spec(card, SpecObjectV1::CommitClaim { claim })
        };
        self.send(vec![(card, o)]).await;
        assert!(self.ledger().claims.contains_key(&id), "the claim committed over its seal");
        id
    }

    async fn file(&mut self, card: usize, claim: Digest, fault: SpecFaultV1) {
        let o = self.route(card, &K::FileProof { accuser: self.kid(card), claim, proof: ProsecutionV1::Spec(fault.to_bytes()) });
        self.send(vec![(card, o)]).await;
    }

    async fn demand(&mut self, card: usize, claim: Digest, stage: u8, position: u32) {
        let o = self.route(card, &K::FileDemand { demander: self.kid(card), claim, stage, position });
        self.send(vec![(card, o)]).await;
    }

    async fn final_of(&mut self, claim: &Digest) {
        let at = self.ledger().opv.claims[claim].window_end_daa;
        self.beat_to(at).await;
        assert!(matches!(self.state_of(claim), ClaimStateV1::Final { .. }), "{:?}", self.state_of(claim));
    }

    /// A second node replays this one's selected chain from genesis: same sink, PALW root, route rows and every delta root.
    async fn assert_replays(&self) {
        let z = t12_genesis_chain(&self.config, &self.bundle, &self.premine, &self.floats);
        for b in chain_blocks(&self.chain, self.chain.sink()) {
            arrive(&z, b, "a block of the first node").await;
        }
        assert_eq!(z.sink(), self.chain.sink());
        assert_eq!(z.tip_state().1.state_root(), self.chain.tip_state().1.state_root(), "same PALW state root");
        assert_eq!(z.ctx.consensus.palw_kernel_route_v1(), self.api(), "same kernel route rows");
        for b in chain_blocks(&self.chain, self.chain.sink()) {
            assert_eq!(root_at(&z, b.header.hash), root_at(&self.chain, b.header.hash), "delta root of {}", b.header.hash);
        }
    }
}

/// A public DA directory: `(stage, position, occurrence, node) → value`.
#[derive(Default)]
struct Da(BTreeMap<(u8, u32, u16, u16), Tensor>);

impl Da {
    fn stage(&mut self, stage: u8, offset: u32, trace: &TraceV1) {
        for (p, pos) in trace.values.iter().enumerate() {
            for (s, occ) in pos.iter().enumerate() {
                for (n, t) in occ.iter().enumerate() {
                    self.0.insert((stage, offset + p as u32, s as u16, n as u16), t.clone());
                }
            }
        }
    }

    fn memory(prod: &MemoryProductionV1, pre: Option<&[Tensor]>) -> Da {
        let mut da = Da::default();
        let mut off = 0;
        for t in &prod.traces {
            da.stage(0, off, t);
            off += t.values.len() as u32;
        }
        for (k, t) in pre.unwrap_or(&[]).iter().enumerate() {
            da.0.insert((MEMORY_PRE_STATE_STAGE_V1, 0, 0, k as u16), t.clone());
        }
        da
    }
}

impl PublicSourceV1 for Da {
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.0.get(&(stage, p, s, n)).cloned()
    }
}

/// A public copy of the snapshot missing the ids in `hole`.
struct Mirror<'a>(&'a SnapshotDataV1, std::ops::Range<u64>);

impl SnapshotSourceV1 for Mirror<'_> {
    fn item(&self, root: &Digest, id: u64) -> Option<RetrievalItemV1> {
        (*root == self.0.snapshot.root() && !self.1.contains(&id)).then(|| self.0.items[id as usize].clone())
    }
}

fn fresh(
    net: &Net,
    claim: Digest,
    da: &Da,
    artifact: &dyn PublicArtifactV1,
    snapshots: &dyn SnapshotSourceV1,
    salt: u8,
) -> OutsiderFindingV1 {
    let ledger = net.ledger();
    SpecOutsiderV1 { ledger: &ledger, claim, material: da, artifact, snapshots, salt: [salt; 64] }
        .check()
        .expect("the outsider concludes")
}

fn fault_of(finding: OutsiderFindingV1) -> SpecFaultV1 {
    let OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(bytes)) = finding else {
        panic!("the outsider should prosecute: {finding:?}")
    };
    borsh::from_slice(&bytes).unwrap()
}

fn bump(t: &mut Tensor, at: usize) {
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}

fn matmul_of(p: &misaka_palw_tir::program::TirProgramV1) -> (usize, usize) {
    for (s, (b, _)) in p.occurrences().iter().enumerate() {
        for (n, node) in p.blocks[*b as usize].nodes.iter().enumerate() {
            if matches!(node.prim, Prim::MatMul) {
                return (s, n);
            }
        }
    }
    panic!("no MatMul")
}

// ---- 1. Weights byte for byte; the unarmed route unchanged ---------------------------------------------------------------------

/// **`[Weights]` through the `Spec` object is the legacy registration byte for byte on the real node**: the same class id, the same
/// route rows and the same ledger root as route tag 1 on a second node. And on a network WITHOUT `palw_typed_roots_v1` the route's
/// `config_root` is the historical one and a `Spec` object is dropped by name (the rows untouched).
#[tokio::test]
async fn r4x_weights_only_spec_is_byte_for_byte_the_legacy_registration_and_the_unarmed_route_is_unchanged() {
    kaspa_core::log::try_init_logger("warn");
    let w = weights(&wide128_v1(7));
    let legacy = K::RegisterClass {
        descriptor: w.descriptor,
        program_bytes: w.program_bytes.clone(),
        plan: w.plan.clone(),
        param_commitments: w.param_commitments.clone(),
    };
    let mut a = Net::new(true).await;
    let o = a.route(1, &legacy);
    a.send(vec![(1, o)]).await;
    let mut b = Net::new(true).await;
    let class = b.register(model_spec()).await;
    let (la, lb) = (a.ledger(), b.ledger());
    assert!(la.classes.contains_key(&class) && lb.classes.contains_key(&class), "the same legacy class id on both nodes");
    assert!(lb.typed.is_empty(), "a Weights-only class is a legacy row");
    // The attested set is the test hook's process-wide list (a consumer fact other tests extend concurrently): compared without it.
    let strip = |rows: LedgerRowsV1| -> LedgerRowsV1 { rows.into_iter().filter(|((t, _), _)| *t != TABLE_ATTESTED_V1).collect() };
    assert_eq!(strip(la.to_rows()), strip(lb.to_rows()), "the same route rows, byte for byte");
    let root = |n: &Net, l: &KernelLedgerV1| {
        let h = n.api().unwrap().header;
        root_of_rows(&h.policy, h.config_root, h.scalars, h.opv.as_ref(), &strip(l.to_rows()))
    };
    assert_eq!(root(&a, &la), root(&b, &lb), "the same ledger root");

    // The unarmed network: the header's config root is the historical template's, and a Spec object never reaches the template.
    let mut u = Net::new(false).await;
    let o = u.route(1, &legacy);
    u.send(vec![(1, o)]).await;
    let route = u.api().unwrap();
    assert_eq!(route.header.typed_roots, None);
    let historical = palw_kernel_route_template_opv_v1(route.header.policy, route.header.opv);
    assert_eq!(
        route.header.config_root,
        config_root_of(&historical.schedule, &historical.known),
        "the unarmed config root is unchanged"
    );
    assert_ne!(a.api().unwrap().header.config_root, route.header.config_root, "arming schedules K2-TR-v1");
    let before = route.rows.clone();
    let spec = u.spec(1, SpecObjectV1::RegisterClass { spec: retrieval_spec(3) });
    u.send(vec![(1, spec)]).await;
    let after = u.api().unwrap();
    assert_eq!(after.rows, before, "a Spec object below palw_typed_roots_v1 is dropped by name: the route's rows are untouched");
    assert!(after.ledger().unwrap().typed.is_empty());
}

// ---- 2. Memory -----------------------------------------------------------------------------------------------------------------

struct MemWorld {
    net: Net,
    fx: TirSketchFixtureV1,
    class: Digest,
    jobs: u8,
}

impl MemWorld {
    async fn new() -> MemWorld {
        let mut net = Net::new(true).await;
        let class = net.register(memory_spec()).await;
        assert!(net.ledger().opv.classes.contains(&class), "an OPV class");
        MemWorld { net, fx: memory_fx(), class, jobs: 0 }
    }

    fn head(&self) -> Digest {
        self.net.ledger().typed.lines[&self.class].head_root
    }

    fn m0(&self) -> Vec<Tensor> {
        vec![self.fx.params.tensors[&(1, Some(0))].clone()]
    }

    /// **The line head's tensors from the node's read API** (the rows: the carried post-state of the claim that advanced it; `M0`
    /// from the public artifact) — what any producer runs the next job from. No producer's copy.
    fn chain_head(&self) -> Vec<Tensor> {
        self.net.ledger().memory_head_tensors_v1(&self.class, &self.fx.params).expect("the head is public")
    }

    async fn job(&mut self, chunks: Vec<Vec<u32>>) -> MemoryJobV1 {
        self.jobs += 1;
        let job = MemoryJobV1 { class: self.class, pre_root: self.head(), chunks, nonce: [self.jobs; 64] };
        self.net.post(SpecJobV1::Memory(job.clone())).await;
        job
    }

    fn produce(&self, job: &MemoryJobV1, card: usize, pre: &[Tensor], lie: impl FnMut(usize, &mut TraceV1)) -> MemoryProductionV1 {
        let l = self.net.ledger();
        let SpecClassKindV1::Memory { rule, root, writers } = &l.typed.classes[&self.class].kind else { panic!() };
        produce_memory_v1(&self.class, rule, root, writers, job, &self.net.kid(card), &self.fx.params, pre, 2, lie).unwrap()
    }
}

/// **The `Memory` class end to end**: registration → a job whose claim commits per-step pre/post roots → Final moves the chain-tracked
/// head → a second job over that head → a lie in ONE update step convicted by an outsider (the fault names the step; the same proof
/// against another step is dismissed) → the job freed → an honest claim carries the memory a second time. A replaying node agrees.
#[tokio::test]
async fn r4x_memory_class_end_to_end_a_lie_in_one_step_is_convicted_and_memory_is_carried_across_two_jobs() {
    kaspa_core::log::try_init_logger("warn");
    let mut m = MemWorld::new().await;
    let root0 = m.head();
    let job1 = m.job(vec![vec![3, 17, 9], vec![5, 2]]).await;
    let p1 = m.produce(&job1, 0, &m.m0(), |_, _| {});
    assert_eq!(p1.claim.step_roots.len(), 3, "S + 1 boundary roots");
    assert_eq!(p1.claim.step_roots[0], root0);
    let c1 = m.net.commit(0, SpecClaimV1::Memory(p1.claim.clone())).await;
    assert!(m.net.state_of(&c1).to_string_debug().starts_with("Challengeable"), "{:?}", m.net.state_of(&c1));
    assert_eq!(fresh(&m.net, c1, &Da::memory(&p1, Some(&m.m0())), &m.fx.params, &(), 0x11), OutsiderFindingV1::Clean);
    m.net.final_of(&c1).await;
    let root1 = m.head();
    assert_eq!(root1, *p1.claim.step_roots.last().unwrap(), "Final moved the line to the claim's post-state");
    // The head is public: cards 4 and 5 below never see card 0's memory, only the node's rows.
    let pre2 = m.chain_head();
    assert_eq!(pre2, p1.post, "the node serves exactly the memory job 1 left (its claim carried it, opened)");

    // Job 2 over job 1's post-state; card 4 lies in step 1.
    let job2 = m.job(vec![vec![4, 4], vec![8, 1, 6]]).await;
    assert_eq!(job2.pre_root, root1, "memory is carried: the job runs on the head");
    let (s_at, n_at) = matmul_of(&m.fx.program);
    let lie = m.produce(&job2, 4, &pre2, |i, t| {
        if i == 1 {
            bump(&mut t.values[0][s_at][n_at], 1)
        }
    });
    let (before, slashed4, money0) = (m.net.collateral(4), m.net.slashed(4), kernel_money_v1(&m.net.chain));
    let c2 = m.net.commit(4, SpecClaimV1::Memory(lie.claim.clone())).await;
    let fault = fault_of(fresh(&m.net, c2, &Da::memory(&lie, None), &m.fx.params, &(), 0x22));
    let SpecFaultV1::MemoryStep { step: 1, proof } = &fault else { panic!("localised to step 1: {fault:?}") };
    let outsider = 6;
    m.net.file(outsider, c2, SpecFaultV1::MemoryStep { step: 0, proof: proof.clone() }).await;
    assert!(!m.net.ledger().claims[&c2].convicted, "the step-1 proof filed against step 0 is dismissed");
    m.net.file(outsider, c2, fault).await;
    let l = m.net.ledger();
    assert!(l.claims[&c2].convicted && matches!(l.claims[&c2].life.state, ClaimStateV1::Convicted { .. }));
    let economics = m.net.api().unwrap().header.opv.unwrap().economics;
    let slashed = economics.reservation_per_claim;
    // (and the non-refundable OPV admission fee burned at the commit: G14-R4's F-C4R3-05)
    assert_eq!(m.net.collateral(4), before - slashed - economics.admission_fee, "the real bond lost the OPV reservation");
    assert_eq!(m.net.slashed(4), slashed4 + slashed + economics.admission_fee, "the fee and the slash are collected from the producer");
    assert_kernel_conserved_v1(money0, kernel_money_v1(&m.net.chain), "a typed claim's commit and conviction");
    assert_eq!(m.net.owed(outsider), slashed * u64::from(l.policy.accuser_reward_permille) / 1000, "the outsider's reward is queued");
    assert_eq!(m.head(), root1, "a convicted claim never moves the line");

    // An honest claim of job 2 (card 5, from the node's copy) finalizes: the head is job 2's post-state, computed from job 1's.
    let honest = m.produce(&job2, 5, &pre2, |_, _| {});
    let c3 = m.net.commit(5, SpecClaimV1::Memory(honest.claim.clone())).await;
    assert_eq!(fresh(&m.net, c3, &Da::memory(&honest, None), &m.fx.params, &(), 0x33), OutsiderFindingV1::Clean);
    m.net.final_of(&c3).await;
    assert_eq!(m.head(), *honest.claim.step_roots.last().unwrap(), "carried a second time");
    assert_eq!(m.chain_head(), honest.post, "and public again");
    let from_m0 = m.produce(&job2, 5, &m.m0(), |_, _| {});
    assert_ne!(*from_m0.claim.step_roots.last().unwrap(), m.head(), "the carried memory changed the result");
    m.net.assert_replays().await;
}

/// **A withheld pre-state is a default, never a conviction**: after the line moved, step 0's pre-state is public on the node (the
/// carried post-state of the claim that moved it), so an outsider needs no demand for it; step 1's pre-state — step 0's slot write
/// at its last position, a committed value of this claim — is the claim's DA. An outsider without it demands exactly that position;
/// the producer withholds it and defaults at the deadline (the fixed default penalty, not the fraud slash).
#[tokio::test]
async fn r4x_memory_a_withheld_pre_state_is_classified_as_a_default() {
    kaspa_core::log::try_init_logger("warn");
    let mut m = MemWorld::new().await;
    let job1 = m.job(vec![vec![1, 2, 3]]).await;
    let p1 = m.produce(&job1, 0, &m.m0(), |_, _| {});
    let c1 = m.net.commit(0, SpecClaimV1::Memory(p1.claim.clone())).await;
    m.net.final_of(&c1).await;

    let job2 = m.job(vec![vec![7, 7], vec![9]]).await;
    let p2 = m.produce(&job2, 4, &m.chain_head(), |_, _| {});
    let (before, slashed4, money0) = (m.net.collateral(4), m.net.slashed(4), kernel_money_v1(&m.net.chain));
    let c2 = m.net.commit(4, SpecClaimV1::Memory(p2.claim.clone())).await;
    let at = p2.traces[0].values.len() as u32 - 1;
    let mut da = Da::memory(&p2, None);
    da.0.retain(|(stage, p, _, _), _| !(*stage == 0 && *p == at));
    assert_eq!(
        fresh(&m.net, c2, &da, &m.fx.params, &(), 0x44),
        OutsiderFindingV1::Demand(vec![(0, at)]),
        "the outsider demands exactly step 1's pre-state"
    );
    let outsider = 6;
    m.net.demand(outsider, c2, 0, at).await;
    // The RFC's literal obligation (the claim's pre-state at stage 0x40) stands too, though the node already holds it.
    m.net.demand(outsider, c2, MEMORY_PRE_STATE_STAGE_V1, 0).await;
    let deadline = m.net.ledger().demands[&(c2, 0, at)].deadline_daa;
    m.net.beat_to(deadline).await;
    let l = m.net.ledger();
    assert!(
        matches!(l.claims[&c2].life.state, ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        l.claims[&c2].life.state
    );
    assert!(!l.claims[&c2].convicted, "withholding is a default, never fraud");
    // (and the non-refundable OPV admission fee burned at the commit: G14-R4's F-C4R3-05)
    let fee = m.net.api().unwrap().header.opv.unwrap().economics.admission_fee;
    assert_eq!(m.net.collateral(4), before - l.policy.default_penalty - fee, "the fixed default penalty, not the fraud slash");
    assert_eq!(m.net.slashed(4), slashed4 + l.policy.default_penalty + fee, "the penalty and the fee are collected from the producer");
    assert_kernel_conserved_v1(money0, kernel_money_v1(&m.net.chain), "a typed claim's commit and default");
    assert_eq!(m.head(), *p1.claim.step_roots.last().unwrap(), "a defaulted claim never moves the line");
    m.net.assert_replays().await;
}

// ---- 3. Retrieval --------------------------------------------------------------------------------------------------------------

/// **The `Retrieval` class end to end**: a wrong item and a missed better item each convicted by an outsider from the public
/// snapshot; a withheld snapshot slice demanded and defaulted.
#[tokio::test]
async fn r4x_retrieval_a_wrong_item_and_a_missed_better_item_are_convicted_and_a_withheld_slice_defaults() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::new(true).await;
    let class = net.register(retrieval_spec(3)).await;
    let data = corpus();
    let SpecClassKindV1::Retrieval { root } = &net.ledger().typed.classes[&class].kind else { panic!() };
    let root = root.clone();
    let none = MapParams::default();
    let whole = Mirror(&data, 0..0);
    let outsider = 6;

    // A wrong item (card 0): entry 1's payload is not the snapshot's.
    let j1 = RetrievalJobV1 { class, query: vec![1, 1, -2, 2], nonce: [1; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j1.clone())).await;
    let mut wrong = data.retrieve(&root, &j1.query).unwrap();
    wrong[1].payload_digest = payload_digest_v1(&[30, 30]);
    let c1 = net.commit(0, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(0), result: wrong })).await;
    let fault = fault_of(fresh(&net, c1, &Da::default(), &none, &whole, 0x51));
    assert!(matches!(fault, SpecFaultV1::Retrieval { stage: 0, fault: RetrievalFaultV1::WrongItem { index: 1, .. } }), "{fault:?}");
    net.file(outsider, c1, fault).await;
    assert!(net.ledger().claims[&c1].convicted);

    // A missed better item (card 4): the best item dropped, the rest shifted up.
    let j2 = RetrievalJobV1 { class, query: vec![-3, 2, 2, 0], nonce: [2; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j2.clone())).await;
    let mut r4 = root.clone();
    r4.rule = RetrievalRuleV1::TopKCountingV1 { k: 4, score_bits: 24 };
    let all = data.retrieve(&r4, &j2.query).unwrap();
    let missed = vec![all[1], all[2], all[3]];
    let c2 = net.commit(4, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(4), result: missed })).await;
    let fault = fault_of(fresh(&net, c2, &Da::default(), &none, &whole, 0x52));
    let SpecFaultV1::Retrieval { fault: RetrievalFaultV1::MissedBetter { id, .. }, .. } = &fault else { panic!("{fault:?}") };
    assert_eq!(*id, all[0].id, "the better item the claim missed");
    net.file(outsider, c2, fault).await;
    assert!(net.ledger().claims[&c2].convicted);

    // A withheld slice (card 5): the public copy lacks slice 2, the producer does not serve it.
    let j3 = RetrievalJobV1 { class, query: vec![0, 1, 0, 1], nonce: [3; 64] };
    let jid = net.post(SpecJobV1::Retrieval(j3.clone())).await;
    let honest = data.retrieve(&root, &j3.query).unwrap();
    let c3 = net.commit(5, SpecClaimV1::Retrieval(RetrievalClaimV1 { job_id: jid, producer_bond: net.kid(5), result: honest })).await;
    let stage = SNAPSHOT_STAGE_BASE_V1;
    assert_eq!(fresh(&net, c3, &Da::default(), &none, &Mirror(&data, 16..24), 0x53), OutsiderFindingV1::Demand(vec![(stage, 2)]));
    net.demand(outsider, c3, stage, 2).await;
    let deadline = net.ledger().demands[&(c3, stage, 2)].deadline_daa;
    net.beat_to(deadline).await;
    let l = net.ledger();
    assert!(
        matches!(l.claims[&c3].life.state, ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "{:?}",
        l.claims[&c3].life.state
    );
    assert!(!l.claims[&c3].convicted, "a withheld slice is a default, never fraud");
    net.assert_replays().await;
}

// ---- 4. Composite --------------------------------------------------------------------------------------------------------------

/// **A `Composite` class with one verified tool stage** (a retrieval kernel feeding a model): the tool lies (it misses the best item),
/// the model honestly consumes what it was handed; the outsider's fault names stage 0, a filing against the honest model stage is
/// dismissed, and the tool stage's fault convicts.
#[tokio::test]
async fn r4x_composite_a_lie_in_the_tool_stage_is_convicted_at_that_stage() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::new(true).await;
    let tool = net.register(retrieval_spec(2)).await;
    let model = net.register(model_spec()).await;
    let class = net.register(composite_spec()).await;
    let data = corpus();
    let model_fx = wide128_v1(7);
    let l = net.ledger();
    let SpecClassKindV1::Retrieval { root } = &l.typed.classes[&tool].kind else { panic!() };
    let row = l.classes[&model].clone();
    let job = CompositeJobV1 { class, prompt: vec![3, 17], query: vec![2, -1, 1, 3], nonce: [9; 64] };
    let jid = net.post(SpecJobV1::Composite(job.clone())).await;

    let mut r3 = root.clone();
    r3.rule = RetrievalRuleV1::TopKCountingV1 { k: 3, score_bits: 24 };
    let all = data.retrieve(&r3, &job.query).unwrap();
    let lied = vec![all[1], all[2]];
    let payloads: Vec<Vec<u32>> = lied.iter().map(|e| data.items[e.id as usize].payload.clone()).collect();
    let prompt: Vec<u32> = job.prompt.iter().copied().chain(payloads.iter().flatten().copied()).collect();
    let (model_stage, trace) =
        produce_model_stage_v1(&model, &row, &jid, 1, prompt, 2, &net.kid(0), &model_fx.params, 2, |_| {}).unwrap();
    let tool_stage = StageClaimV1::Retrieval { query: job.query.clone(), result: lied, payloads };
    let c = net
        .commit(
            0,
            SpecClaimV1::Composite(CompositeClaimV1 { job_id: jid, producer_bond: net.kid(0), stages: vec![tool_stage, model_stage] }),
        )
        .await;

    let mut da = Da::default();
    da.stage(1, 0, &trace);
    let artifact = vec![model_fx.params.clone(), model_fx.params.clone()];
    let fault = fault_of(fresh(&net, c, &da, &artifact, &Mirror(&data, 0..0), 0x61));
    assert!(
        matches!(fault, SpecFaultV1::Retrieval { stage: 0, fault: RetrievalFaultV1::MissedBetter { .. } }),
        "the tool stage: {fault:?}"
    );
    let outsider = 6;
    let (post, node) = row.logits_at();
    let honest_logits =
        misaka_palw_kernel::public::TensorWireV1::of(&trace.values[trace.values.len() - 2][post as usize][node as usize]);
    net.file(outsider, c, SpecFaultV1::StageDecode { stage: 1, index: 0, logits: honest_logits }).await;
    assert!(!net.ledger().claims[&c].convicted, "a filing against the honest model stage is dismissed");
    net.file(outsider, c, fault).await;
    assert!(net.ledger().claims[&c].convicted, "convicted at the stage that lied");
    net.assert_replays().await;
}

trait DebugName {
    fn to_string_debug(&self) -> String;
}

impl DebugName for ClaimStateV1 {
    fn to_string_debug(&self) -> String {
        format!("{self:?}")
    }
}
