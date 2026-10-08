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
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, t12_genesis_chain, t12_with_harness_cards};
use crate::consensus::test_consensus::TestConsensus;
use crate::pipeline::virtual_processor::processor::kernel_route_test_attest_artifact_v1;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
    palw_kernel_route_template_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerEventV1, OutsiderFindingV1, OutsiderV1, ProsecutionV1, PublicSourceV1};
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
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.skip_proof_of_work = true;
    // `ConfigBuilder::build` runs `validate_palw_v2`, which refuses this fence by design (module doc): construct the config directly.
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fence; only this harness bypasses it");
    (Config::new(params), bundle, premine, floats)
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
}

impl Net {
    fn new() -> Net {
        let (config, bundle, premine, floats) = kernel_config();
        let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
        let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let funding = floats.clone();
        Net { chain, config, bundle, premine, floats, funding, domain, rnd: 0 }
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

    /// **Send objects, one per card in this call**: each through the mempool, into the node's own template, and folded by the next
    /// chain block. Returns the folding block.
    async fn send(&mut self, items: Vec<(usize, Obj)>) -> Block {
        let mut txs = Vec::new();
        for (card, object) in &items {
            let tx = self.carrier(*card, object);
            self.mempool(&tx).unwrap_or_else(|e| panic!("the mempool takes the carrier: {e}"));
            txs.push(tx);
        }
        let ttpb = self.ttpb();
        let carrying = self.chain.heartbeat(ttpb, txs.clone()).await;
        for tx in &txs {
            assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()), "the node's template carries the kernel carrier");
        }
        self.chain.heartbeat(ttpb, Vec::new()).await
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
        let template = palw_kernel_route_template_v1(api.header.policy);
        let ledger = KernelLedgerV1::from_rows(&template, api.header.scalars, &api.rows).expect("the served rows rebuild a ledger");
        assert_eq!(ledger.root(), committed_ledger_root.as_bytes(), "and the rebuilt ledger's own root agrees");
        Fresh { ledger, salt: [salt; 64] }
    }

    fn check(&self, claim: Digest, da: &Da, artifact: &MapParams) -> OutsiderFindingV1 {
        OutsiderV1 { ledger: &self.ledger, claim, material: da, artifact, salt: self.salt }.check().expect("the fresh verifier concludes")
    }
}

/// The colluding seats' receipts for `claim`: each assigned seat signs a passing receipt for its scope (it did no work).
fn colluding_receipts(net: &mut Net, claim: Digest, fx: &Fixture, signed_daa: u64) -> Vec<(usize, Obj)> {
    let state = net.chain.tip_state().1;
    let kr = state.kernel_route().expect("the route");
    let assignment = kr.assignment_of(&claim).expect("the claim has an interim assignment");
    let ledger = net.ledger();
    let row = &ledger.claims[&claim];
    let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &row.body else { panic!("a program claim") };
    let class = &ledger.classes[&row.class_binding_id];
    let _ = fx;
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

// ---- the scenarios ---------------------------------------------------------------------------------------

/// Registers the class (signed by card 1), posts a job (card 1) and returns `(class id, job)`.
async fn class_and_job(net: &mut Net, fx: &Fixture) -> (Digest, KernelJobV1) {
    let d = k2_tir_v2_descriptor();
    let register =
        K::RegisterClass { descriptor: d.digest(), program_bytes: fx.program.encode(), plan: fx.plan.clone(), param_commitments: fx.pc.clone() };
    let o = net.route(1, &register);
    net.send(vec![(1, o)]).await;
    let ledger = net.ledger();
    let class = *ledger.classes.keys().next().expect("the class registered through the real path");
    let job = KernelJobV1 { class_binding_id: class, prompt: vec![3, 17, 9], max_new_tokens: 3, decode: DecodeRuleV1::Greedy, nonce: [1; 64] };
    let o = net.route(1, &K::PostJob { job: job.clone() });
    net.send(vec![(1, o)]).await;
    assert!(net.ledger().jobs.contains_key(&job.id()), "the job posted");
    (class, job)
}

/// **Milestone: a lying claim, covered by every colluding seat, is convicted by an outsider through the real path** — class
/// registration, job, commitment, interim assignment, the seats' receipts, the Panel's pass, the outsider's filing from public material
/// alone, the conviction in the fold, the real bond slashed, the accuser's reward queued for the coinbase.
#[tokio::test]
async fn g14_kernel_route_a_covered_lie_is_convicted_by_an_outsider_through_the_real_path() {
    kaspa_core::log::try_init_logger("warn");
    let fx = fixture();
    let mut net = Net::new();
    let (class, job) = class_and_job(&mut net, &fx).await;
    let producer_card = 0usize;

    // ---- the producer commits a lie (a MatMul at position 1 off by one) ----
    let ledger = net.ledger();
    let generated = greedy(&fx, &ledger, &class, &job.prompt, 3);
    let at = matmul_at(&fx.program, 1);
    let lie = produce(&fx, &ledger, &class, &job, net.kid(producer_card), generated, |t| {
        bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1)
    });
    let claim_id = lie.claim.id();
    let before = net.collateral(producer_card);
    let o = net.route(producer_card, &lie.object);
    net.send(vec![(producer_card, o)]).await;
    let ledger = net.ledger();
    assert!(ledger.claims.contains_key(&claim_id), "the claim committed");
    let state = net.chain.tip_state().1;
    assert_eq!(
        state.kernel_reserved(&net.bond(producer_card)),
        u128::from(ledger.policy.claim_collateral),
        "V2 sees the kernel's reservation against the producer's real bond"
    );
    let assignment = state.kernel_route().unwrap().assignment_of(&claim_id).expect("interim assignment");
    assert_eq!(assignment.seats.len(), 3, "three distinct-operator seats");
    assert!(assignment.seats.iter().all(|s| s.bond != net.bond(producer_card)), "never the producer");

    // ---- every seat colludes: their signed receipts are the Panel's pass ----
    let daa = net.daa();
    let receipts = colluding_receipts(&mut net, claim_id, &fx, daa);
    let seat_cards: Vec<usize> = receipts.iter().map(|(c, _)| *c).collect();
    assert!(!seat_cards.contains(&producer_card), "the producer is not its own seat");
    // The outsider: a bonded card the interim assignment did NOT seat (a bonded verifier outside the Panel).
    let outsider_card = (0..net.chain.bonds.len()).find(|c| *c != producer_card && !seat_cards.contains(c)).expect("a card outside the Panel");
    for (card, object) in receipts {
        net.send(vec![(card, object)]).await;
    }
    let ledger = net.ledger();
    assert!(
        matches!(ledger.claims[&claim_id].life.state, misaka_palw_kernel::lifecycle::ClaimStateV1::ProbabilisticPass { .. }),
        "the Panel passed the claim: {:?}",
        ledger.claims[&claim_id].life.state
    );

    // ---- the fresh outsider: the node's read API + the producer's published DA, its own salt ----
    let api = net.api().expect("the read API serves the route");
    let committed = api.ledger_root();
    let verifier = Fresh::from_api(&api, committed, 0x5A);
    let da = Da::publishing(&fx.program, &lie.trace, &[]);
    let finding = verifier.check(claim_id, &da, &fx.params);
    let OutsiderFindingV1::Prosecute(proof) = finding else { panic!("the fresh verifier convicts a covered lie: {finding:?}") };
    let ProsecutionV1::Kernel(_) = &proof else { panic!("a kernel fault proof") };

    // ---- it files; the node folds the conviction ----
    let o = net.route(outsider_card, &K::FileProof { accuser: net.kid(outsider_card), claim: claim_id, proof });
    net.send(vec![(outsider_card, o)]).await;
    let ledger = net.ledger();
    assert!(ledger.claims[&claim_id].convicted, "the claim is convicted");
    let slashed = u64::from(net.ledger().policy.claim_collateral);
    assert_eq!(net.collateral(producer_card), before - slashed, "the REAL producer bond lost the reservation");
    assert_eq!(net.chain.tip_state().1.kernel_reserved(&net.bond(producer_card)), 0, "and the reservation is gone");
    let state = net.chain.tip_state().1;
    let reward = slashed * u64::from(ledger.policy.accuser_reward_permille) / 1000;
    let payee = state.bond(&net.bond(outsider_card)).unwrap().payout_payload;
    let paid = state
        .pending_payouts_iter()
        .find(|(k, _)| **k == kaspa_consensus_core::palw_kernel_route_v1::palw_kernel_payout_key_v1(&payee))
        .map(|(_, p)| p.amount);
    assert_eq!(paid, Some(reward), "the accuser's reward is queued for the next coinbase");
    // The next block's coinbase pays it (the harness asserts every inserted block UTXO-valid).
    let ttpb = net.ttpb();
    let next = net.chain.heartbeat(ttpb, Vec::new()).await;
    assert!(
        next.transactions[0].outputs.iter().any(|o| o.value == reward && o.script_public_key == card_payout_spk(outsider_card)),
        "the coinbase pays the outsider its share of the slash"
    );
    let _ = LedgerEventV1::Refused { tx: "", why: String::new() };
    let _ = MaterialResponseV1::Omitted;
    let _ = PositionResponseV1 { values: vec![], inputs: vec![] };
}
