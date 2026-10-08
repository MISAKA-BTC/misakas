//! **K2-TIR-v4 on the real node (lane K2S, `docs/design/palw/k2-real-scale.md`)**: segmented claims through the mempool, the node's
//! template, the chain block's fold, the persisted tip and the read API — a tiled prompt past 4,096 ids as a multi-segment claim (an
//! honest element filed against it dismissed), a lie in one segment localized to one element and convicted with bounded bytes, a
//! withheld segment demanded and defaulted, a demanded position served on chain and convicted from the blocks' bytes, the mempool's
//! acceptance gate (GAP 10) and the cached ledger (GAP 8).
//!
//! The outsider is fresh: its verifier is the read API's segmented record (op 210's `public_record`, checked against the class
//! header the chain serves), the public artifact and whatever material the producer publishes — the bytes it reads are counted.
//! The class is the sketch's history-free wide layer under K2-TIR-v4 at 8,192 positions, registered under
//! OptimisticPublicVerification on an OPV network (a v4 class has no Panel route).

use super::*;
use misaka_palw_kernel::descriptor::k2_tir_v4_descriptor;
use misaka_palw_kernel::element::{SegFaultV1, SegFindingV1, SegMaterialV1, check_positions_v1, prove_element_v1};
use misaka_palw_kernel::evidence::EvidenceHeaderV1;
use misaka_palw_kernel::seg::{
    PromptTileOpeningV1, SEG_LEN_V4, SegmentedCommitmentsV1, TiledJobV1, build_segmented_evidence_v1, prompt_root_of_ids_v1,
    prompt_tiles_v1, seg_commitments_of_trace_v1,
};
use misaka_palw_kernel::seg_da::{assemble_position_v1, position_part_v1, position_parts_v1};
use misaka_palw_kernel::seg_ledger::SegmentedClaimRecordV1;
use std::cell::Cell;

const SEG_MAX_POSITIONS: u32 = 8192;

struct SegFixture {
    program: TirProgramV1,
    params: MapParams,
    plan: misaka_palw_kernel::VerificationPlanV1,
    pc: ParamCommitmentsV1,
}

fn seg_fixture() -> SegFixture {
    let fx = wide128_v1(7);
    let d = k2_tir_v4_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), SEG_MAX_POSITIONS).unwrap();
    let pc = ParamCommitmentsV1::of_v3(&fx.params);
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(pc.root()), 0);
    SegFixture { program: fx.program, params: fx.params, plan, pc }
}

fn seg_class_id(f: &SegFixture) -> Hash64 {
    Hash64::from_bytes(single_class_id_v1(
        k2_tir_v4_descriptor().digest(),
        &f.program.encode(),
        &f.plan,
        &f.pc,
        VerificationModeV1::OptimisticPublicVerification,
    ))
}

struct SegWorld {
    net: Net,
    f: SegFixture,
    class: Digest,
    jobs: u8,
}

/// A producer's segmented claim and the values it committed (honest, or with a lie).
struct SegClaim {
    id: Digest,
    values: Vec<Vec<Vec<Tensor>>>,
    c: SegmentedCommitmentsV1,
    tokens: Vec<u32>,
}

/// What the producer publishes off-chain: every position but the withheld ones; the bytes an outsider reads are counted.
struct SegDa<'a> {
    claim: &'a SegClaim,
    withhold: std::ops::Range<u32>,
    read: Cell<u128>,
}

impl SegMaterialV1 for SegDa<'_> {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        if self.withhold.contains(&p) {
            return None;
        }
        let v = self.claim.values.get(p as usize)?.clone();
        let bytes: u128 = v.iter().flatten().map(|t| (t.len() * t.dtype.width()) as u128).sum();
        self.read.set(self.read.get() + bytes);
        Some(v)
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        (!self.withhold.contains(&p) && p < self.claim.c.positions()).then(|| self.claim.c.position_path(p).1)
    }
}

/// Positions assembled from the parts the chain's blocks carry, with the bytes a verifier reads counted.
struct FromBlocks {
    positions: BTreeMap<u32, (Vec<Vec<Tensor>>, Vec<Digest>)>,
    read: Cell<u128>,
}

impl SegMaterialV1 for FromBlocks {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        let v = self.positions.get(&p)?.0.clone();
        let bytes: u128 = v.iter().flatten().map(|t| (t.len() * t.dtype.width()) as u128).sum();
        self.read.set(self.read.get() + bytes);
        Some(v)
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        self.positions.get(&p).map(|(_, siblings)| siblings.clone())
    }
}

/// **Every `Respond` part the selected chain carries for `(claim, position)`**, read back from the blocks' transactions the way a fresh
/// verifier reads them: the ledger keeps none of their bytes.
fn served_parts_from_blocks(net: &Net, claim: &Digest, position: u32) -> Vec<Vec<u8>> {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2;
    let mut out = Vec::new();
    for b in chain_blocks(&net.chain, net.chain.sink()) {
        for tx in b.transactions.iter() {
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
                continue;
            }
            let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
            let Obj::KernelRouteV1 { bytes, .. } = payload.object else { continue };
            if let Ok(K::Respond { claim: c, stage: 0, position: p, bytes }) = K::decode(&bytes)
                && c == *claim
                && p == position
            {
                out.push(bytes);
            }
        }
    }
    out
}

/// A `KernelRouteV1` carrying `bytes` exactly as given, genuinely signed by card `card`'s bond (the bytes need not be a route object).
fn signed_bytes(net: &mut Net, card: usize, bytes: Vec<u8>) -> Obj {
    let signer = net.bond(card);
    let message = palw_kernel_route_message_v1(net.domain, &signer, &bytes);
    net.rnd = net.rnd.wrapping_add(1);
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1,
        [net.rnd; 32],
    )
    .expect("ML-DSA-87 signs")
    .as_ref()
    .to_vec();
    Obj::KernelRouteV1 { bytes, signer, signature }
}

impl SegWorld {
    async fn new() -> SegWorld {
        let f = seg_fixture();
        let mut net = Net::over_cfg(kernel_config_opv(vec![seg_class_id(&f)]), TestConsensus::new);
        net.beat_to(1).await;
        let d = k2_tir_v4_descriptor();
        // A Panel-licensed registration of a v4 class is dropped by the ledger (no Panel can cover a real-scale claim).
        let panel = K::RegisterClass {
            descriptor: d.digest(),
            program_bytes: f.program.encode(),
            plan: f.plan.clone(),
            param_commitments: f.pc.clone(),
        };
        let o = net.route(1, &panel);
        net.send(vec![(1, o)]).await;
        assert!(net.api().is_none_or(|k| k.ledger().unwrap().classes.is_empty()), "no Panel route for a K2-TIR-v4 class");
        let register = K::RegisterClassV2 {
            mode: VerificationModeV1::OptimisticPublicVerification,
            descriptor: d.digest(),
            program_bytes: f.program.encode(),
            plan: f.plan.clone(),
            param_commitments: f.pc.clone(),
        };
        let o = net.route(1, &register);
        net.send(vec![(1, o)]).await;
        let class = seg_class_id(&f).as_bytes();
        assert!(net.ledger().classes.contains_key(&class), "the K2-TIR-v4 class registered through the real path under OPV");
        SegWorld { net, f, class, jobs: 0 }
    }

    /// A tiled job of `prompt` posted by card 1, then every tile of it (any bond: card 2).
    async fn tiled_job(&mut self, prompt: &[u32]) -> Digest {
        self.jobs += 1;
        let job = TiledJobV1 {
            class_binding_id: self.class,
            prompt_len: prompt.len() as u32,
            prompt_root: prompt_root_of_ids_v1(prompt),
            max_new_tokens: 2,
            decode: DecodeRuleV1::Greedy,
            nonce: [self.jobs; 64],
        };
        let o = self.net.route(1, &K::PostTiledJob { job: job.clone() });
        self.net.send(vec![(1, o)]).await;
        assert!(self.net.ledger().tiled_jobs.contains_key(&job.id()), "the tiled job posted");
        // Each tile from another card, so the tiles ride ONE block: every load after the block's first is the cached ledger (GAP 8).
        let mut tiles = Vec::new();
        for i in 0..prompt_tiles_v1(prompt.len() as u32) {
            let card = 2 + i as usize;
            tiles.push((
                card,
                self.net.route(card, &K::PostPromptTile { job: job.id(), tile: PromptTileOpeningV1::of(prompt, i).unwrap() }),
            ));
        }
        let hits =
            kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_LEDGER_CACHE_HITS_V1.load(std::sync::atomic::Ordering::Relaxed);
        self.net.send(tiles).await;
        if prompt_tiles_v1(prompt.len() as u32) > 1 {
            let now = kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_LEDGER_CACHE_HITS_V1
                .load(std::sync::atomic::Ordering::Relaxed);
            assert!(now > hits, "the block's later objects load the cached ledger");
        }
        assert!(self.net.ledger().tiled_jobs[&job.id()].complete(), "every tile posted");
        job.id()
    }

    /// **A segmented claim of `job` by card `producer`**, sealed one block and then revealed; `lie` bumps element 0 of one value.
    async fn claim(&mut self, producer: usize, job: Digest, prompt: &[u32], lie: Option<(u32, u16, u16)>) -> SegClaim {
        let post = (self.f.program.occurrences().len() - 1) as usize;
        let logits = self.f.program.logits as usize;
        let first = trace_v1(&self.f.program, &self.f.params, prompt).unwrap();
        let g0 = DecodeRuleV1::Greedy.select(&first.values[prompt.len() - 1][post][logits]).unwrap();
        let mut tokens = prompt.to_vec();
        tokens.push(g0);
        let trace = trace_v1(&self.f.program, &self.f.params, &tokens).unwrap();
        let g1 = DecodeRuleV1::Greedy.select(&trace.values[tokens.len() - 1][post][logits]).unwrap();
        let mut values = trace.values;
        if let Some((p, s, n)) = lie {
            bump(&mut values[p as usize][s as usize][n as usize], 0);
        }
        let c = seg_commitments_of_trace_v1(&TraceV1 { values: values.clone(), inputs: Vec::new() });
        let ledger = self.net.ledger();
        let class = &ledger.classes[&self.class];
        let evidence = build_segmented_evidence_v1(
            class.header(self.class),
            &class.descriptor,
            prompt.len() as u32,
            &prompt_root_of_ids_v1(prompt),
            &[g0],
            &c,
        );
        let kid = self.net.kid(producer);
        let claim = KernelClaimV1 { job_id: job, producer_bond: kid, generated: vec![g0, g1], evidence_root: evidence.root() };
        let id = claim.id();
        let seal = self.net.route(producer, &K::SealClaim { producer: kid, job, seal: claim_seal_v1(&id) });
        self.net.send(vec![(producer, seal)]).await;
        let commit = K::CommitSegmentedClaim { claim, evidence, segment_roots: c.segment_roots() };
        let o = self.net.route(producer, &commit);
        self.net.send(vec![(producer, o)]).await;
        assert!(self.net.ledger().claims.contains_key(&id), "the segmented claim committed over its seal");
        SegClaim { id, values, c, tokens }
    }

    /// **The outsider's verifier, from the read API alone**: the segmented record and the class header op 210 serves, checked
    /// against each other and against the committed ledger root.
    fn fresh_record(&self, claim: &Digest) -> misaka_palw_kernel::seg_ledger::SegClaimViewV1 {
        let api = self.net.api().expect("the read API serves the route");
        let read = api.claim_read_v1(claim).expect("the rows rebuild").expect("the claim is served");
        assert_eq!(read.kind, "segmented");
        assert_eq!(read.ledger_root, api.ledger_root());
        let header: EvidenceHeaderV1 = borsh::from_slice(&read.record_header).expect("the class header");
        SegmentedClaimRecordV1::from_bytes(&read.public_record).expect("the record").view(&header).expect("the record is the claim's")
    }
}

/// **A tiled prompt past J5b's 4,096 ids is a multi-segment claim on the real node**: the job and its two tiles through the mempool
/// and the fold, the claim's 4,501 positions in five segments with only their roots on chain, a fresh outsider checking one position
/// of every segment from the read API's record and public material, and the claim Final at the window's end with no Panel.
#[tokio::test]
async fn g14_k2s_a_tiled_prompt_past_4096_ids_commits_as_a_multi_segment_claim_on_the_real_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let prompt: Vec<u32> = (0..4500u32).map(|i| (i * 13 + 5) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let claim = w.claim(0, job, &prompt, None).await;
    let ledger = w.net.ledger();
    let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { segment_roots, evidence, .. } = &ledger.claims[&claim.id].body else {
        panic!("a segmented claim")
    };
    assert_eq!((segment_roots.len(), evidence.positions), (5, 4501), "five segments of 1,024 positions");
    let row = borsh::to_vec(&ledger.claims[&claim.id]).unwrap().len();
    assert!(row < 4096 + 5 * 64, "the claim's row is {row} B for 4,501 positions");
    // GAP 8: a cached ledger is the rows' own ledger (and every load in a debug build checks the cache against the rows).
    let api = w.net.api().unwrap();
    let rebuilt = KernelLedgerV1::from_rows(&api.template(), api.header.scalars, &api.rows).unwrap();
    let mut cached = api.clone();
    cached.ledger_cache.set(rebuilt.clone().as_rebuilt_from_v1(&cached.rows));
    assert_eq!(cached.ledger().unwrap().root(), rebuilt.root(), "the cached ledger is the rebuilt one");
    // A fresh outsider: one position of every segment (and the ones they read), from public material only.
    let view = w.fresh_record(&claim.id);
    let da = SegDa { claim: &claim, withhold: 0..0, read: Cell::new(0) };
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let checked: Vec<u32> = (0..5).map(|s| s * SEG_LEN_V4 + 100).chain([4500]).collect();
    assert_eq!(check_positions_v1(&view.context(), &da, &art, &claim.tokens, &checked), SegFindingV1::Clean);
    // An honest element filed anyway is dismissed by the fold: no conviction, no slash, and the claim still finalizes.
    let filing = prove_element_v1(&view.context(), &da, &art, &claim.tokens, (4100, 1, 2), 0).unwrap();
    let (outsider, slashed_before) = (3usize, w.net.slashed(0));
    let o = w.net.route(
        outsider,
        &K::FileProof {
            accuser: w.net.kid(outsider),
            claim: claim.id,
            proof: ProsecutionV1::Segmented(SegFaultV1::Element(filing).to_bytes()),
        },
    );
    w.net.send(vec![(outsider, o)]).await;
    assert!(!w.net.ledger().claims[&claim.id].convicted, "an honest element is never a conviction");
    assert_eq!(w.net.slashed(0), slashed_before, "and never a slash");
    // No Panel: Final at the OPV window's end.
    let ev_daa = w.net.daa() + 80;
    w.net.beat_to(ev_daa).await;
    assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of a segmented claim");
}

/// **A lie in one segment is localized to one element and convicted with bounded bytes**: the producer commits a wrong element of the
/// wide MatMul at position 1,200 (segment 1); segment 0 checks clean; the outsider's check of position 1,200 reads positions 1,199 and
/// 1,200 only, its filing (one output leaf, the operand lines, three node openings) is a few KB — far under the class's filing bound
/// and the carrier — and the fold convicts and slashes the real bond.
#[tokio::test]
async fn g14_k2s_a_lie_in_one_segment_is_localized_and_convicted_with_bounded_bytes() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let prompt: Vec<u32> = (0..1500u32).map(|i| (i * 7 + 1) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let view = w.fresh_record(&lie.id);
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let clean = SegDa { claim: &lie, withhold: 0..0, read: Cell::new(0) };
    assert_eq!(check_positions_v1(&view.context(), &clean, &art, &lie.tokens, &[3, 700, 1023]), SegFindingV1::Clean, "segment 0");
    let da = SegDa { claim: &lie, withhold: 0..0, read: Cell::new(0) };
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &da, &art, &lie.tokens, &[1200]) else {
        panic!("the lie is not found")
    };
    let SegFaultV1::Element(e) = fault.as_ref() else { panic!("an element fault") };
    assert_eq!((e.position, e.occurrence, e.node), (1200, s, n), "localized to the lying value");
    let bytes = fault.to_bytes();
    let bounds = w.net.ledger().classes[&w.class].bounds;
    let per_position: u128 = lie.values[1200].iter().flatten().map(|t| (t.len() * t.dtype.width()) as u128).sum();
    eprintln!(
        "[k2s] node: filing {} B (class bound {}), outsider read {} B of material (two positions = {} B), public bound {}",
        bytes.len(),
        bounds.max_filing_bytes,
        da.read.get(),
        2 * per_position,
        bounds.max_public_bytes
    );
    assert!(bytes.len() as u64 <= bounds.max_filing_bytes);
    assert!(bytes.len() < kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1);
    assert!(da.read.get() <= 2 * per_position + per_position / 2, "one prosecution reads two positions");
    assert!(da.read.get() <= bounds.max_public_bytes);
    let outsider = 3usize;
    let slashed_before = w.net.slashed(0);
    let o =
        w.net.route(outsider, &K::FileProof { accuser: w.net.kid(outsider), claim: lie.id, proof: ProsecutionV1::Segmented(bytes) });
    w.net.send(vec![(outsider, o)]).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "convicted by an outsider from public material");
    assert!(matches!(ledger.claims[&lie.id].life.state, ClaimStateV1::Convicted { .. }));
    assert!(w.net.slashed(0) > slashed_before, "the producer's real bond is slashed");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the conviction");
}

/// **A withheld segment is demanded and defaults, never a conviction**: the producer publishes segment 0 and withholds segment 1; the
/// outsider's check of a segment-1 position asks for exactly the two positions it reads; the demands are filed, a response that is
/// not the committed material is rejected, nothing valid is served, and at the deadline the claim is the producer's availability
/// default — its reservation charged, never the fraud slash, and the demanders' bonds returned.
#[tokio::test]
async fn g14_k2s_a_withheld_segment_is_demanded_and_defaults_never_a_conviction() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let prompt: Vec<u32> = (0..1100u32).map(|i| (i * 3 + 2) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let claim = w.claim(0, job, &prompt, None).await;
    let view = w.fresh_record(&claim.id);
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let withheld = SegDa { claim: &claim, withhold: SEG_LEN_V4..claim.c.positions(), read: Cell::new(0) };
    let SegFindingV1::Demand(missing) = check_positions_v1(&view.context(), &withheld, &art, &claim.tokens, &[1050]) else {
        panic!("a withheld position is a demand")
    };
    assert_eq!(missing, vec![1049, 1050]);
    let outsider = 3usize;
    let demands: Vec<(usize, Obj)> = missing
        .iter()
        .map(|p| {
            (
                outsider,
                w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: claim.id, stage: 0, position: *p }),
            )
        })
        .collect();
    w.net.send(demands).await;
    assert_eq!(w.net.ledger().demands.keys().filter(|(c, _, _)| *c == claim.id).count(), 2, "two sessions, one per position");
    assert!(w.net.kernel_reserved(outsider) > 0, "the demand bonds are reserved on the real bond");
    // A response with another position's values: its position root is not the committed one.
    let wrong =
        misaka_palw_kernel::seg_da::position_part_v1(&w.f.program, 1050, &claim.values[1048], claim.c.position_path(1050).1, 0)
            .unwrap();
    let o = w.net.route(0, &K::Respond { claim: claim.id, stage: 0, position: 1050, bytes: borsh::to_vec(&wrong).unwrap() });
    w.net.send(vec![(0, o)]).await;
    let at_demand = w.net.ledger();
    let d = &at_demand.demands[&(claim.id, 0, 1050)];
    assert_eq!(d.last_class(), Some("wrong_root"), "rejected and classified");
    let slashed_before = w.net.slashed(0);
    let deadline = d.deadline_daa;
    w.net.beat_to(deadline + 2).await;
    let ledger = w.net.ledger();
    let row = &ledger.claims[&claim.id];
    assert!(!row.convicted, "withholding is never a conviction");
    assert!(matches!(row.life.state, ClaimStateV1::Unavailable { .. }), "{:?}", row.life.state);
    assert!(w.net.slashed(0) > slashed_before, "the availability default is charged to the real bond");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bonds returned");
    assert!(ledger.seg_progress.keys().all(|(c, _, _)| *c != claim.id), "no progress row outlives the demand");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the default");
}

/// **GAP 10: the mempool runs the kernel route's acceptance gate** — a carrier whose signature its bond's key does not verify, and one
/// whose bytes the kernel cannot decode, are refused at admission by name; the genuine one is admitted.
#[tokio::test]
async fn g14_k2s_the_mempool_runs_the_kernel_acceptance_gate() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let job = TiledJobV1 {
        class_binding_id: w.class,
        prompt_len: 10,
        prompt_root: prompt_root_of_ids_v1(&[1; 10]),
        max_new_tokens: 2,
        decode: DecodeRuleV1::Greedy,
        nonce: [0xEE; 64],
    };
    let mut forged = w.net.route(2, &K::PostTiledJob { job: job.clone() });
    let Obj::KernelRouteV1 { signature, .. } = &mut forged else { unreachable!() };
    signature[0] ^= 1;
    let tx = w.net.carrier(2, &forged);
    match w.net.mempool(&tx) {
        Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwKernelRouteRefused(why)) => assert!(why.contains("signature"), "{why}"),
        other => panic!("a forged kernel carrier is refused at admission: {other:?}"),
    }
    // (A refused carrier never confirms: card 4's next carrier spends the output the refused one tried to.)
    let card4_funding = w.net.funding[4].clone();
    let mut junk = w.net.route(4, &K::PostTiledJob { job: job.clone() });
    let Obj::KernelRouteV1 { bytes, .. } = &mut junk else { unreachable!() };
    bytes.push(0);
    let tx = w.net.carrier(4, &junk);
    assert!(
        matches!(w.net.mempool(&tx), Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwKernelRouteRefused(_))),
        "bytes changed under their signature"
    );
    // The same non-canonical bytes, genuinely signed: the signature verifies and the kernel's strict decode refuses them, by name.
    let mut bytes = K::PostTiledJob { job: job.clone() }.encode();
    bytes.push(0);
    let signed_junk = signed_bytes(&mut w.net, 4, bytes);
    w.net.funding[4] = card4_funding;
    let tx = w.net.carrier(4, &signed_junk);
    match w.net.mempool(&tx) {
        Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwKernelRouteRefused(why)) => {
            assert!(why.contains("PostTiledJob refused (Malformed)") && !why.contains("signature"), "{why}")
        }
        other => panic!("a signed non-canonical encoding is refused at admission: {other:?}"),
    }
    let genuine = w.net.route(3, &K::PostTiledJob { job: job.clone() });
    w.net.send(vec![(3, genuine)]).await;
    assert!(w.net.ledger().tiled_jobs.contains_key(&job.id()), "the genuine carrier is admitted and folded");
}

/// **A demanded position is served on chain, and a fresh verifier convicts from the blocks.** The producer lies at position 1,200 and
/// publishes nothing off-chain. The outsider's check asks for positions 1,199 and 1,200, and it demands both. The producer answers each
/// with its parts (it must, or it defaults), and both demands close as served. The outsider reads the responses back from the chain's
/// blocks — the ledger keeps none of their bytes — assembles the two positions, checks them and files. The claim is convicted, the real
/// bond slashed, the demand bonds return, and a replaying node agrees.
#[tokio::test]
async fn g14_k2s_a_demanded_position_served_on_chain_is_checked_from_the_blocks_and_convicted() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 5 + 4) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let view = w.fresh_record(&lie.id);
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let nothing = SegDa { claim: &lie, withhold: 0..lie.c.positions(), read: Cell::new(0) };
    let SegFindingV1::Demand(missing) = check_positions_v1(&view.context(), &nothing, &art, &lie.tokens, &[1200]) else {
        panic!("material nobody published is a demand")
    };
    assert_eq!(missing, vec![1199, 1200]);
    let outsider = 3usize;
    let demands: Vec<(usize, Obj)> = missing
        .iter()
        .map(|p| {
            (outsider, w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: lie.id, stage: 0, position: *p }))
        })
        .collect();
    w.net.send(demands).await;
    assert!(w.net.kernel_reserved(outsider) > 0, "the demand bonds are reserved on the real bond");
    // The producer serves every part of both positions.
    let mut answers = Vec::new();
    for &p in &missing {
        for i in 0..position_parts_v1(&w.f.program, p).unwrap().len() as u32 {
            let part = position_part_v1(&w.f.program, p, &lie.values[p as usize], lie.c.position_path(p).1, i).unwrap();
            let o = w.net.route(0, &K::Respond { claim: lie.id, stage: 0, position: p, bytes: borsh::to_vec(&part).unwrap() });
            answers.push((0usize, o));
        }
    }
    w.net.send(answers).await;
    let ledger = w.net.ledger();
    for &p in &missing {
        assert!(ledger.seg_progress.get(&(lie.id, 0, p)).is_some_and(|g| g.complete_daa.is_some()), "position {p} served on chain");
    }
    assert!(ledger.demands.keys().all(|(c, _, _)| *c != lie.id), "both demands closed by service");
    // The fresh verifier: the record from the read API, the parts from the blocks.
    let mut positions = BTreeMap::new();
    for &p in &missing {
        let parts = served_parts_from_blocks(&w.net, &lie.id, p);
        assert!(!parts.is_empty(), "the chain carries position {p}'s parts");
        let assembled = assemble_position_v1(&w.f.program, &view.segment_roots, view.positions, p, &parts)
            .expect("every part of the position is in the blocks and authentic");
        positions.insert(p, assembled);
    }
    let chain = FromBlocks { positions, read: Cell::new(0) };
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &lie.tokens, &[1200]) else {
        panic!("the served lie is found")
    };
    eprintln!(
        "[k2s] node: a served lie is filed in {} B after reading {} B back from the blocks",
        fault.to_bytes().len(),
        chain.read.get()
    );
    let slashed_before = w.net.slashed(0);
    let o = w.net.route(
        outsider,
        &K::FileProof { accuser: w.net.kid(outsider), claim: lie.id, proof: ProsecutionV1::Segmented(fault.to_bytes()) },
    );
    w.net.send(vec![(outsider, o)]).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "the served values convict the claim");
    assert!(matches!(ledger.claims[&lie.id].life.state, ClaimStateV1::Convicted { .. }));
    assert!(w.net.slashed(0) > slashed_before, "the producer's real bond is slashed");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bonds return with the conviction");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of a conviction from served material");
}
