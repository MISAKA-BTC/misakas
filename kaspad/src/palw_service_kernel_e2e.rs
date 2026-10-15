//! **Lane G14-K: the kernel route's public verifier, as kaspad's ACTUAL service** (`PalwPanelService` → `palw_kernel_verifier`).
//!
//! G14's bar on the kernel route, through the real service rather than a harness verifier: a K2-TIR-v2 class (the sketch's
//! `wide128_v1`, the canonical G14 class) registered through the PRODUCTION onboarding path — its V2 IR class registered, its artifact
//! bound to the kernel commitments and matured (no test attestation hook: kaspad's tests cannot reach consensus's `cfg(test)` seams,
//! and need not) — and claims licensed by a Panel in which EVERY interim seat signs a passing receipt. The verifier is an ordinary
//! public bond registered AFTER the claims (testnet-12's main premine re-addressed to its key: funds, not a bond), outside every
//! Panel; its node is an empty database that validated every public block (`Chain::import_fresh`), and its service starts after the
//! claims (and after Final for the post-Final ones). The service holds the registered model as a `PALWKM01` file (ADR-0177 D7), reads
//! the route through the node's read API, replays every claim from the public commitments, and its seals and filings ride the PQ
//! mempool → the production template → the real block pipeline. Outcomes are read back from consensus.
//!
//! Harness exceptions (as the parent's): test keys and premine, skipped PoW, inert EVM, and the dormant fences
//! (`palw_probabilistic_constraints_v1`, `palw_tir_v1`, `palw_reorg_strict_economic_win`; the parent's `palw_legacy_public_filer_v1`)
//! armed in the fixture while the shipping activation validator still refuses them (asserted).
use super::*;
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_kernel_route_v1::{
    PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, PalwKernelRouteStateV1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1,
};
use kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2;
use kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1;
use kaspa_consensus_core::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1};
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use kaspa_consensus_core::palw_tir_class_v1::{
    PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1, PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1,
    PalwTirLayoutV1, palw_tir_class_registration_message_v1,
};
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::evidence::build_evidence_v1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{KernelLedgerV1, claim_seal_v1};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::program_root_v1;
use misaka_palw_kernel::receipt::{CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1, ReceiptInputsV1, receipt_for_v1};
use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
use misaka_palw_kernel::trace::{ParamCommitmentsV1, TraceV1, WiringV1, trace_v1};
use misaka_palw_kernel::verify::{CheckCostV1, ScopeVerdictV1};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{MapParams, Tensor};
use std::borrow::Cow;
use std::collections::BTreeMap;

const MAX_POSITIONS: u32 = 64;
/// The class's registrant and the jobs' poster.
const REGISTRANT: usize = 1;

// ---- the network --------------------------------------------------------------------------------------------------------------

/// The parent's testnet-12 fixture with the kernel route, the IR class fence (the V2 class onboarding binds to) and the strict
/// economic tie rule armed at genesis — WITHOUT their validation, which still refuses them.
fn kernel_service_config() -> (Config, PalwConsensusParamsV2, Vec<Funding>, Vec<Funding>) {
    let (config, bundle, premine, floats) = fixture_config();
    let mut params = config.params.clone();
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_tir_v1 = Some(kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1::testnet12_v1(ForkActivation::new(0)));
    params.sync_palw_tir_v1();
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(0));
    assert!(params.validate_palw_v2().is_err(), "the shipping activation guard still refuses the kernel route");
    (Config::new(params), bundle, premine, floats)
}

/// The canonical class with its weights both as a V2 inventory (the registered `artifact_root`) and as kernel commitments.
struct KFixture {
    program: TirProgramV1,
    params: MapParams,
    plan: misaka_palw_kernel::VerificationPlanV1,
    pc: ParamCommitmentsV1,
    class: PalwTirClassV1,
    artifact_root: Hash64,
}

struct Tensors(BTreeMap<(u16, Option<u16>), Vec<u8>>);
impl PalwTirTensorSourceV1 for Tensors {
    fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
        self.0.get(&(param, layer)).map(|b| Cow::Borrowed(b.as_slice()))
    }
}

/// The court fixture's layout (`g14_registration_e2e::layout_of`): ragged commit tiles, a 4,096-lane logits tile, 2-position
/// checkpoints.
fn layout_of(class: &PalwTirClassV1, positions: u32) -> PalwTirLayoutV1 {
    let p = class.decode_program().expect("canonical");
    let mut tiles = Vec::new();
    let mut k = 0u32;
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            if !n.commit {
                continue;
            }
            let is_logits = bi == p.schedule.post as usize && ni == p.logits as usize;
            tiles.push(if is_logits { 4096 } else { 4 + (k * 7) % 6 });
            k += 1;
        }
    }
    PalwTirLayoutV1 {
        version: PALW_TIR_LAYOUT_VERSION_V1,
        max_context: positions,
        checkpoint_interval: 2,
        h_tile: 2,
        commit_tiles: tiles,
        state_tiles: (0..p.states.len() as u32).map(|j| 4 + j % 3).collect(),
    }
}

fn k_fixture() -> KFixture {
    let fx = misaka_palw_tir_sketch::fixture::wide128_v1(7);
    let mut program = fx.program.clone();
    program.logits_scheme_id.copy_from_slice(kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
    let d = k2_tir_v2_descriptor();
    let plan = plan_for_tir_program_v1(&d, &program, program_root_v1(&program.encode()), MAX_POSITIONS).expect("a K2-TIR-v2 plan");
    let pc = ParamCommitmentsV1::of(&fx.params);
    let mut class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: PalwTirLayoutV1 {
            version: PALW_TIR_LAYOUT_VERSION_V1,
            max_context: MAX_POSITIONS,
            checkpoint_interval: 2,
            h_tile: 2,
            commit_tiles: Vec::new(),
            state_tiles: Vec::new(),
        },
        tokenizer_id: Hash64::from_bytes([0x70; 64]),
    };
    class.layout = layout_of(&class, MAX_POSITIONS);
    let tensors = Tensors(fx.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
    let ops = palw_tir_inventory_operands_v1(&program, &tensors).expect("the inventory");
    let artifact_root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("a root");
    KFixture { program, params: fx.params, plan, pc, class, artifact_root }
}

// ---- reads ---------------------------------------------------------------------------------------------------------------------

fn route_of(chain: &Chain) -> PalwKernelRouteStateV1 {
    chain.tc.palw_kernel_route_v1().expect("the route exists")
}

fn ledger_of(chain: &Chain) -> KernelLedgerV1 {
    KernelLedgerV1::clone(&route_of(chain).ledger().expect("the rows rebuild"))
}

fn state_of(chain: &Chain, claim: &Digest) -> ClaimStateV1 {
    ledger_of(chain).claims[claim].life.state.clone()
}

fn collateral_of(chain: &Chain, bond: PalwBondKeyV2) -> (u64, u64) {
    let b = chain
        .tc
        .palw_claim_rows_v1(bond, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor, false, 0)
        .and_then(|r| r.bond)
        .expect("the bond");
    (b.collateral, b.slashed)
}

/// The kernel route objects `signer` carried for `claim` in the actual blocks, by name.
fn carried_by(chain: &Chain, signer: PalwBondKeyV2, claim: &Digest) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for object in chain
        .history
        .iter()
        .flat_map(|b| b.transactions.iter())
        .filter_map(|t| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.payload).ok())
        .map(|p| p.object)
    {
        match object {
            PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: s, .. } if s == signer => {
                if let Ok(k) = K::decode(&bytes) {
                    let about = match &k {
                        K::SealProof { claim: c, .. } | K::FileProof { claim: c, .. } | K::FileDemand { claim: c, .. } => c == claim,
                        _ => false,
                    };
                    if about {
                        *out.entry(k.name()).or_insert(0) += 1;
                    }
                }
            }
            PalwConsensusObjectV2::KernelRouteChunkV1 { chunk, .. }
                if chunk.opener == signer
                    && chunk.target == kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkTargetV1::Claim(*claim) =>
            {
                *out.entry("KernelRouteChunkV1").or_insert(0) += 1;
            }
            _ => {}
        }
    }
    out
}

// ---- the actors' objects -------------------------------------------------------------------------------------------------------

/// A `KernelRouteV1` signed by card `card`'s bond (the producer's and the registrant's side; hedged randomness is fine here).
fn route_obj(chain: &Chain, card: usize, signer: PalwBondKeyV2, k: &K) -> PalwConsensusObjectV2 {
    let bytes = k.encode();
    let message = palw_kernel_route_message_v1(chain.network_domain, &signer, &bytes);
    let signature = signed(card, message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1);
    PalwConsensusObjectV2::KernelRouteV1 { bytes, signer, signature }
}

fn onboarding_signed(chain: &Chain, card: usize, kind: u8, payload: &[u8]) -> Vec<u8> {
    let message =
        kaspa_consensus_core::palw_onboarding_v1::palw_onboarding_message_v1(chain.network_domain, kind, &chain.bonds[card], payload);
    signed(card, message.as_byte_slice(), kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_MLDSA87_CONTEXT_V1)
}

/// The V2 IR class registration of the fixture, signed by card `card` (the SDK's builder over the chain's registration terms).
fn v2_registration(chain: &Chain, f: &KFixture, card: usize, activation_daa: u64) -> PalwConsensusObjectV2 {
    let terms = chain.tc.palw_v2_registration_terms().expect("the chain's registration terms");
    let facts = PalwTirJobFactsV1::of_class(&f.class, f.class.class_id(&f.artifact_root)).expect("decodes");
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&f.class).expect("wide enough"));
    let mut o = palw_tir_post_genesis_registration_v1(
        f.class.clone(),
        canonical,
        f.artifact_root,
        terms.min_grantable_share_permille,
        terms.initial_target,
        terms.slash_value_per_pwu,
        activation_daa,
        chain.bonds[card],
        Vec::new(),
        chain.bundle.court.max_step_leaf_count(),
    )
    .expect("the builder counts the canonical job");
    let PalwConsensusObjectV2::ClassRegisteredTirV1 {
        class_id,
        share_permille,
        activation_daa,
        artifact_root,
        slash_value_per_pwu,
        initial_target,
        pwu_rule,
        admission,
    } = &mut o
    else {
        unreachable!()
    };
    let message = palw_tir_class_registration_message_v1(
        chain.network_domain,
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
    admission.signature = signed(card, message.as_byte_slice(), PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1);
    o
}

fn bump(t: &mut Tensor, at: usize) {
    let v = t.data[at];
    t.data[at] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
}

/// The covered element faults G14-V named for the canonical class: an embedding gather element, a wide I128 MatMul accumulator
/// element, a clamp (range / saturating nonlinearity) result — each one committed value at position 1 moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lie {
    Gather,
    MatMul,
    Clamp,
}

fn find_node(program: &TirProgramV1, pick: impl Fn(&misaka_palw_tir::Prim) -> bool) -> (u16, u16) {
    for (s, (b, _)) in program.occurrences().iter().enumerate() {
        for (n, node) in program.blocks[*b as usize].nodes.iter().enumerate() {
            if pick(&node.prim) {
                return (s as u16, n as u16);
            }
        }
    }
    panic!("the program has no such node")
}

// ---- the world -----------------------------------------------------------------------------------------------------------------

struct KWorld {
    chain: Chain,
    premine: Vec<Funding>,
    floats: Vec<Funding>,
    fx: KFixture,
    class: Digest,
    jobs: u8,
}

impl KWorld {
    /// **The class through the production onboarding path**: the V2 IR class registers (card 1), card 1 binds its artifact to the
    /// kernel commitments (bonded), the binding matures, and the kernel route registers the Panel-licensed class over the root it now
    /// attests — no test hook anywhere.
    async fn new() -> KWorld {
        let (config, bundle, premine, mut floats) = kernel_service_config();
        let mut chain = Chain::new(config, bundle, &premine);
        chain.advance(2).await;
        let fx = k_fixture();
        let class_obj = v2_registration(&chain, &fx, REGISTRANT, chain.daa() + 30);
        let PalwConsensusObjectV2::ClassRegisteredTirV1 { class_id: v2_class, .. } = &class_obj else { unreachable!() };
        let v2_class = *v2_class;
        chain.send(REGISTRANT, &class_obj, &mut floats[REGISTRANT], &[]).await;
        assert!(chain.tc.palw_tir_class_record_v1(v2_class).is_some(), "the V2 IR class registered");
        let root = Hash64::from_bytes(fx.pc.root());
        let payload = borsh::to_vec(&(v2_class, root)).unwrap();
        let signature = onboarding_signed(&chain, REGISTRANT, 104, &payload);
        let bound =
            PalwConsensusObjectV2::ArtifactBoundV1 { v2_class, kernel_param_root: root, signer: chain.bonds[REGISTRANT], signature };
        chain.send(REGISTRANT, &bound, &mut floats[REGISTRANT], &[]).await;
        let row = route_of(&chain).artifact_binding_v1(&v2_class, &root).expect("the binding row");
        chain.advance(row.matures_daa).await;
        let d = k2_tir_v2_descriptor();
        let register = K::RegisterClass {
            descriptor: d.digest(),
            program_bytes: fx.program.encode(),
            plan: fx.plan.clone(),
            param_commitments: fx.pc.clone(),
        };
        let o = route_obj(&chain, REGISTRANT, chain.bonds[REGISTRANT], &register);
        chain.send(REGISTRANT, &o, &mut floats[REGISTRANT], &[]).await;
        let class = *ledger_of(&chain).classes.keys().next().expect("the kernel class registered over the matured binding");
        assert_eq!(route_of(&chain).onboarding_attested_roots_v1(chain.daa()), vec![root], "attested by the binding alone");
        eprintln!("[service-kernel] class registered through onboarding at DAA {}", chain.daa());
        KWorld { chain, premine, floats, fx, class, jobs: 0 }
    }

    async fn job(&mut self) -> KernelJobV1 {
        self.jobs += 1;
        let job = KernelJobV1 {
            class_binding_id: self.class,
            prompt: vec![3, 17, 9],
            max_new_tokens: 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [self.jobs; 64],
        };
        let o = route_obj(&self.chain, REGISTRANT, self.chain.bonds[REGISTRANT], &K::PostJob { job: job.clone() });
        self.chain.send(REGISTRANT, &o, &mut self.floats[REGISTRANT], &[]).await;
        assert!(ledger_of(&self.chain).jobs.contains_key(&job.id()), "the job posted");
        job
    }

    /// **A claim of a fresh job by card `producer`** (sealed, revealed), its committed trace carrying `lie`, covered by its Panel
    /// (every interim seat signs a passing receipt). Returns the claim id and its seats.
    async fn covered_claim(&mut self, producer: usize, lie: Option<Lie>) -> (Digest, Vec<usize>) {
        let id = self.commit_claim(producer, lie).await;
        let seats = self.cover(&[id]).await.remove(0);
        assert!(!seats.contains(&producer));
        (id, seats)
    }

    /// **A claim of a fresh job by card `producer`**, sealed one block and revealed the next; its committed trace carries `lie`.
    async fn commit_claim(&mut self, producer: usize, lie: Option<Lie>) -> Digest {
        let job = self.job().await;
        let ledger = ledger_of(&self.chain);
        let row = &ledger.classes[&self.class];
        let (post, logits) = row.logits_at();
        // The honest greedy generation.
        let mut stream = job.prompt.clone();
        let mut generated = Vec::new();
        for _ in 0..job.max_new_tokens {
            let t = trace_v1(&self.fx.program, &self.fx.params, &stream).unwrap();
            let tok = DecodeRuleV1::Greedy.select(&t.values[stream.len() - 1][post as usize][logits as usize]).unwrap();
            generated.push(tok);
            stream.push(tok);
        }
        let fed = generated.len() - 1;
        let stream: Vec<u32> = job.prompt.iter().chain(&generated[..fed]).copied().collect();
        let mut trace: TraceV1 = trace_v1(&self.fx.program, &self.fx.params, &stream).unwrap();
        if let Some(lie) = lie {
            let (s, n) = find_node(&self.fx.program, |p| match lie {
                Lie::Gather => matches!(p, misaka_palw_tir::Prim::Gather { .. }),
                Lie::MatMul => matches!(p, misaka_palw_tir::Prim::MatMul),
                Lie::Clamp => matches!(p, misaka_palw_tir::Prim::Clamp { .. }),
            });
            let v = &mut trace.values[1][s as usize][n as usize];
            let before = v.clone();
            bump(v, 0);
            assert_ne!(*v, before, "the lie changes the committed value");
        }
        let w = WiringV1::new(&self.fx.program).unwrap();
        let ev = build_evidence_v1(&w, &trace.evidence(), &stream, row.header(self.class), &row.descriptor, 2).unwrap();
        let producer_bond = self.chain.bonds[producer];
        let claim = KernelClaimV1 {
            job_id: job.id(),
            producer_bond: palw_kernel_bond_id_v1(&producer_bond),
            generated,
            evidence_root: ev.root(),
        };
        let id = claim.id();
        assert!(ledger.salted_seals_from().is_none(), "a Panel network seals with seal v1");
        let seal = K::SealClaim { producer: claim.producer_bond, job: claim.job_id, seal: claim_seal_v1(&id) };
        let o = route_obj(&self.chain, producer, producer_bond, &seal);
        self.chain.send(producer, &o, &mut self.floats[producer], &[]).await;
        let reveal = K::CommitClaim { claim, evidence: ev, commitments: trace.evidence().commitments };
        let o = route_obj(&self.chain, producer, producer_bond, &reveal);
        self.chain.send(producer, &o, &mut self.floats[producer], &[]).await;
        assert!(ledger_of(&self.chain).claims.contains_key(&id), "the claim committed over its seal (its lie is in a relation)");
        id
    }

    /// **Every assigned seat of each claim colludes**: each signs a passing receipt for its scope; the receipts ride in as few blocks as
    /// the seats' cards allow (one carrier per card per block). Returns each claim's seats.
    async fn cover(&mut self, ids: &[Digest]) -> Vec<Vec<usize>> {
        let route = route_of(&self.chain);
        let ledger = ledger_of(&self.chain);
        let signed_daa = self.chain.daa();
        let mut receipts: Vec<(usize, PalwConsensusObjectV2)> = Vec::new();
        let mut all_seats = Vec::new();
        for id in ids {
            let assignment = route.assignment_of(id).expect("an interim assignment");
            let misaka_palw_kernel::ledger::ClaimBodyV1::Program { evidence, .. } = &ledger.claims[id].body else {
                panic!("a program")
            };
            let class = &ledger.classes[&self.class];
            let id = *id;
            let mut seats = Vec::new();
            for seat in &assignment.seats {
                let card = self.chain.bonds.iter().position(|b| *b == seat.bond).expect("a seat is a genesis card");
                let operator = self
                    .chain
                    .tc
                    .palw_producer_facts_v2(self.chain.bundle.base_class_id, Some(seat.bond.0))
                    .and_then(|f| f.bond)
                    .expect("the seat's facts")
                    .operator_id;
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
                    claim_id: id,
                    assignment_root: assignment.assignment_root,
                    challenge_anchor: assignment.challenge_anchor,
                    sample_seed: assignment.sample_seed,
                    seat_bond: seat.kernel_bond,
                    seat_operator: operator.as_bytes(),
                    signed_daa,
                };
                let receipt = receipt_for_v1(&inputs, &seat.scope, &verdict).expect("a passing receipt");
                let signature = signed(card, &receipt.signing_message(), CONSTRAINT_RECEIPT_MLDSA87_CONTEXT_V1);
                let o = PalwConsensusObjectV2::KernelConstraintReceiptV1 { receipt: Box::new(receipt), signature };
                receipts.push((card, o));
                seats.push(card);
            }
            all_seats.push(seats);
        }
        // Waves: each card at most once a block (its change chains to the next).
        while !receipts.is_empty() {
            let mut wave = Vec::new();
            let mut used = BTreeSet::new();
            receipts.retain(|(card, o)| {
                if used.insert(*card) {
                    wave.push((*card, o.clone()));
                    false
                } else {
                    true
                }
            });
            self.send_batch(wave).await;
        }
        for id in ids {
            assert!(
                matches!(state_of(&self.chain, id), ClaimStateV1::ProbabilisticPass { .. }),
                "the colluding Panel passed the claim: {:?}",
                state_of(&self.chain, id)
            );
        }
        all_seats
    }

    /// Several cards' objects in ONE carrying block (each card once), then the folding block — through the node's mempool check.
    async fn send_batch(&mut self, items: Vec<(usize, PalwConsensusObjectV2)>) {
        let daa = self.chain.daa();
        let mut txs = Vec::new();
        for (card, object) in &items {
            let funding = &mut self.floats[*card];
            let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
                &self.chain.config,
                key(*card),
                object,
                funding.0,
                &funding.1,
                &[],
                None,
                daa,
            )
            .unwrap();
            self.chain
                .tc
                .validate_mempool_transaction(&mut MutableTransaction::from_tx(tx.clone()), &Default::default())
                .unwrap_or_else(|e| panic!("the mempool takes card {card}'s carrier: {e}"));
            let index = tx.outputs.len() - 1;
            let output = &tx.outputs[index];
            *funding = (
                TransactionOutpoint::new(tx.id(), index as u32),
                UtxoEntry::new(output.value, output.script_public_key.clone(), daa + 1, false),
            );
            txs.push(tx);
        }
        self.chain.heartbeat(txs).await;
        self.chain.heartbeat(vec![]).await;
    }

    /// **The public participant**: an ordinary bond registered NOW (after every claim, so no assignment can have seated it), from
    /// testnet-12's main premine output re-addressed to its key. Returns the bond and its funding (the registration's change).
    async fn register_outsider(&mut self) -> (PalwBondKeyV2, Funding) {
        let signed_bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 0));
        let public_key = card_pubkey(OUTSIDER);
        let payout = card_payout_payload(OUTSIDER);
        let classes = BTreeSet::new();
        let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
        let message = palw_bond_registration_message_v2(
            self.chain.network_domain,
            &signed_bond,
            &public_key,
            &public_key,
            collateral,
            &payout,
            &classes,
        );
        let possession = palw_operator_possession_message_v1(self.chain.network_domain, &signed_bond, &public_key, &public_key);
        let mut signature = signed(OUTSIDER, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
        signature.extend(signed(OUTSIDER, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
        let registration = PalwConsensusObjectV2::BondRegistered {
            bond: signed_bond,
            pubkey: public_key.clone(),
            operator_pubkey: public_key.clone(),
            collateral,
            payout_payload: payout,
            capable_classes: classes,
            signature,
        };
        let tx = self
            .chain
            .send(
                OUTSIDER,
                &registration,
                &mut self.floats[OUTSIDER],
                &[TransactionOutput::new(collateral, card_payout_spk(OUTSIDER))],
            )
            .await;
        let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 0));
        assert_eq!(self.chain.tc.palw_bond_of_pubkey_v2(&public_key).unwrap().0, bond);
        assert!(!self.chain.bonds.contains(&bond), "not a genesis card");
        (bond, self.floats[OUTSIDER].clone())
    }

    /// The registered model, written where the service reads it (`--palw-kernel-model`).
    fn model_file(&self, dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("wide128.palwkm");
        super::super::palw_kernel_verifier::palw_kernel_model_write_v1(&path, &self.fx.params).unwrap();
        path
    }
}

// ---- the actual service --------------------------------------------------------------------------------------------------------

struct KNode {
    manager: Arc<ConsensusManager>,
    pool: MiningManagerProxy,
    running: ServiceRun,
    _flow: Arc<FlowContext>,
}

/// Start the actual `PalwPanelService` of `bond` over `chain` (a mempool and a flow context of its own), its state in `dir`, holding
/// the kernel model `model`.
fn start_kernel_service(
    chain: &Chain,
    dir: &std::path::Path,
    bond: PalwBondKeyV2,
    funding: &Funding,
    model: &std::path::Path,
    keep: &mut Vec<kaspa_database::utils::DbLifetime>,
) -> KNode {
    let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))));
    let config = Arc::new(chain.config.clone());
    let tick = Arc::new(TickService::new());
    let (meta_lifetime, meta_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    keep.push(meta_lifetime);
    let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), meta_db, tick.clone());
    let pool = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
        config.target_time_per_block(),
        false,
        config.max_block_mass,
        config.ram_scale,
        config.block_template_cache_lifetime,
        Default::default(),
        true,
        config.palw_model_market.is_some(),
        config.palw_activation_pool_fence().is_some(),
        None,
        AttestationMempoolPolicy::disabled(),
        kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&config.params),
    )));
    let hub = kaspa_p2p_lib::Hub::new();
    let rules = Arc::new(MiningRuleEngine::new(
        manager.clone(),
        config.clone(),
        Default::default(),
        tick.clone(),
        hub.clone(),
        Default::default(),
        Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
    ));
    let flow = Arc::new(FlowContext::new(
        manager.clone(),
        addresses,
        config.clone(),
        pool.clone(),
        tick,
        chain.tc.notification_root(),
        hub,
        rules,
    ));
    let mut panel = panel_config(chain, dir, bond, funding);
    panel.kernel_models = vec![model.to_path_buf()];
    let service = Arc::new(PalwPanelService::new(panel, manager.clone(), flow.clone(), config));
    assert!(service.keypair.is_some());
    assert_eq!(service.bond, Some(bond.0));
    let cloned = service.clone();
    let task = tokio::spawn(async move {
        cloned.start().await.unwrap();
    });
    KNode { manager, pool, running: ServiceRun { service, task: Some(task) }, _flow: flow }
}

/// Run `node`'s service, mining what it submits (two blocks: carrying, folding) and otherwise one heartbeat about every service tick
/// (the service acts once a DAA and ticks every 2 s, so idle blocks are paced to it), until `done(chain)`; at most `secs` of wall time
/// (a host-speed bound, reported with the state on failure).
async fn run_until(chain: &mut Chain, node: &KNode, secs: u64, what: &str, done: impl Fn(&Chain) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    let start = chain.daa();
    let mut idle = 0u32;
    loop {
        assert!(!node.running.task.as_ref().unwrap().is_finished(), "the service stopped by itself");
        if done(chain) {
            eprintln!("[service-kernel] {what}: reached at DAA {} (from {start})", chain.daa());
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{what}: no objective progress by DAA {} (from {start})", chain.daa());
        let (pending, _) = node.pool.clone().get_all_transactions(TransactionQuery::TransactionsOnly).await;
        if !pending.is_empty() {
            let ids: Vec<_> = pending.iter().map(|t| t.tx.id()).collect();
            let block = pool_heartbeat(chain, &node.manager, &node.pool).await;
            assert!(
                ids.iter().any(|id| block.transactions.iter().any(|tx| tx.id() == *id)),
                "the production template includes the service's carrier"
            );
            pool_heartbeat(chain, &node.manager, &node.pool).await;
            idle = 0;
        } else {
            idle += 1;
            if idle >= 16 {
                pool_heartbeat(chain, &node.manager, &node.pool).await;
                idle = 0;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// **G14 on the kernel route through kaspad's actual service**: six covered lies — a gather element, a wide MatMul accumulator
/// element and a clamp result, each once already FINAL (the node and the service start after Final) and once still before Final —
/// and an honest twin, all licensed by Panels in which every seat signed a passing receipt. One public bond registered after them
/// all, on a node that validated the public history from an empty database, holding the registered model: its service convicts each
/// lie (before Final where the claim had not reached it, inside the liability horizon where it had), slashing the producer's real
/// bond and charging no colluding seat, and files nothing against the honest twin, which reaches Final.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14k_the_kernel_service_convicts_covered_lies_pre_and_post_final_and_the_honest_twin_finalizes() {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let mut w = KWorld::new().await;
    let lies = [Lie::Gather, Lie::MatMul, Lie::Clamp];
    // Producers rotate over cards 0, 2, 3 (never the registrant card 1); the seats are drawn among the others.
    let producers = [0usize, 2, 3];
    // ---- the post-Final lies: committed, covered, and left to reach Final ----
    let mut after_final = Vec::new();
    for (i, lie) in lies.iter().enumerate() {
        let (id, seats) = w.covered_claim(producers[i], Some(*lie)).await;
        after_final.push((id, producers[i], *lie, seats));
    }
    let floor = after_final
        .iter()
        .map(|(id, ..)| match state_of(&w.chain, id) {
            ClaimStateV1::ProbabilisticPass { window_end_daa, .. } => window_end_daa,
            s => panic!("passed: {s:?}"),
        })
        .max()
        .unwrap();
    w.chain.advance(floor + 1).await;
    for (id, ..) in &after_final {
        assert!(matches!(state_of(&w.chain, id), ClaimStateV1::Final { .. }), "{:?}", state_of(&w.chain, id));
    }
    // ---- the pre-Final lies and the honest twin: committed, then covered together (their windows open at once) ----
    let mut committed = Vec::new();
    for (i, lie) in lies.iter().enumerate() {
        committed.push((w.commit_claim(producers[i], Some(*lie)).await, producers[i], *lie));
    }
    let honest = w.commit_claim(0, None).await;
    let mut ids: Vec<Digest> = committed.iter().map(|(id, ..)| *id).collect();
    ids.push(honest);
    let seats = w.cover(&ids).await;
    let before_final: Vec<(Digest, usize, Lie, Vec<usize>)> =
        committed.into_iter().zip(seats).map(|((id, producer, lie), seats)| (id, producer, lie, seats)).collect();
    // ---- the public participant, its fresh node, its service ----
    let (bond, funding) = w.register_outsider().await;
    let seat_slashed: Vec<(usize, u64)> = (0..8).map(|c| (c, collateral_of(&w.chain, w.chain.bonds[c]).1)).collect();
    let producer_before: BTreeMap<usize, u64> = producers.iter().map(|p| (*p, collateral_of(&w.chain, w.chain.bonds[*p]).0)).collect();
    let premine = w.premine.clone();
    w.chain.import_fresh(&premine).await;
    let dir = tempfile::tempdir().unwrap();
    let model = w.model_file(dir.path());
    let mut keep = Vec::new();
    let node = start_kernel_service(&w.chain, dir.path(), bond, &funding, &model, &mut keep);
    let all: Vec<Digest> = after_final.iter().chain(before_final.iter()).map(|(id, ..)| *id).collect();
    let pre: Vec<Digest> = before_final.iter().map(|(id, ..)| *id).collect();
    run_until(&mut w.chain, &node, 900, "six convictions", |chain| {
        let ledger = ledger_of(chain);
        // A pre-Final lie must be convicted before it reaches Final.
        for id in &pre {
            assert!(
                ledger.claims[id].convicted || !matches!(ledger.claims[id].life.state, ClaimStateV1::Final { .. }),
                "a pre-Final lie reached Final unconvicted"
            );
        }
        all.iter().all(|id| ledger.claims[id].convicted)
    })
    .await;
    // The honest twin: run on to its Final.
    run_until(&mut w.chain, &node, 600, "the honest twin's Final", |chain| {
        matches!(state_of(chain, &honest), ClaimStateV1::Final { .. })
    })
    .await;
    let ledger = ledger_of(&w.chain);
    let policy = ledger.policy;
    for (id, producer, lie, seats) in after_final.iter().chain(before_final.iter()) {
        let row = &ledger.claims[id];
        assert!(row.convicted, "{lie:?}: convicted");
        assert!(!seats.is_empty() && !seats.contains(producer));
        let carried = carried_by(&w.chain, bond, id);
        assert_eq!(carried.get("SealProof").copied(), Some(1), "{lie:?}: one seal: {carried:?}");
        assert_eq!(carried.get("FileProof").copied(), Some(1), "{lie:?}: one filing, whole: {carried:?}");
        let when = if pre.contains(id) { "pre-Final" } else { "post-Final" };
        eprintln!("[service-kernel] {lie:?} ({when}): convicted by the service; carried {carried:?}");
    }
    for (p, before) in &producer_before {
        let n = after_final.iter().chain(before_final.iter()).filter(|(_, q, ..)| q == p).count() as u64;
        assert_eq!(
            collateral_of(&w.chain, w.chain.bonds[*p]).0,
            before - n * policy.claim_collateral,
            "card {p}: each conviction slashed its real bond"
        );
    }
    // No colluding seat is charged. The jobs' poster (card 1) pays exactly one thing in the meantime: the honest twin's Final reward,
    // out of its job escrow (GAP-5: user-pays; the bond's `slashed` records the escrow spend). No convicted claim is paid.
    for (card, before) in &seat_slashed {
        if !producers.contains(card) {
            let paid = if *card == REGISTRANT { policy.claim_reward } else { 0 };
            assert_eq!(
                collateral_of(&w.chain, w.chain.bonds[*card]).1,
                before + paid,
                "card {card}: a colluding seat is not charged (seats: {:?})",
                after_final.iter().chain(before_final.iter()).map(|(.., s)| s.clone()).collect::<Vec<_>>()
            );
        }
    }
    assert!(carried_by(&w.chain, bond, &honest).is_empty(), "nothing filed against the honest twin");
    assert!(!ledger.claims[&honest].convicted);
    let mut running = node.running;
    running.finish().await;
}

/// **The service resumes mid-pursuit (G14-R's property, through the actual service)**: the service is stopped once its seal is on
/// chain and a NEW service (same state directory: a process restart) carries on — it re-derives the step from the chain (no second
/// seal, the filing once) and convicts before Final.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn g14k_the_kernel_service_resumes_after_a_restart_without_a_second_seal() {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let mut w = KWorld::new().await;
    let (lie, _) = w.covered_claim(0, Some(Lie::MatMul)).await;
    let (bond, funding) = w.register_outsider().await;
    let me = palw_kernel_bond_id_v1(&bond);
    let dir = tempfile::tempdir().unwrap();
    let model = w.model_file(dir.path());
    let mut keep = Vec::new();
    let first = start_kernel_service(&w.chain, dir.path(), bond, &funding, &model, &mut keep);
    run_until(&mut w.chain, &first, 300, "the seal on chain", |chain| ledger_of(chain).proof_seals.contains_key(&(lie, me))).await;
    let sealed_at = ledger_of(&w.chain).proof_seals[&(lie, me)].daa;
    let mut running = first.running;
    running.finish().await;
    drop(first.pool);
    // Ten blocks with no service.
    for _ in 0..10 {
        w.chain.heartbeat(vec![]).await;
    }
    let funding_now = funding.clone();
    let second = start_kernel_service(&w.chain, dir.path(), bond, &funding_now, &model, &mut keep);
    run_until(&mut w.chain, &second, 300, "the conviction after the restart", |chain| ledger_of(chain).claims[&lie].convicted).await;
    assert!(!matches!(state_of(&w.chain, &lie), ClaimStateV1::Final { .. }), "convicted before Final");
    let ledger = ledger_of(&w.chain);
    assert_eq!(ledger.proof_seals.get(&(lie, me)).map(|s| s.daa).unwrap_or(sealed_at), sealed_at, "the seal kept its DAA");
    let carried = carried_by(&w.chain, bond, &lie);
    assert_eq!(carried.get("SealProof").copied(), Some(1), "no second seal after the restart: {carried:?}");
    assert_eq!(carried.get("FileProof").copied(), Some(1), "one filing: {carried:?}");
    let mut running = second.running;
    running.finish().await;
}
