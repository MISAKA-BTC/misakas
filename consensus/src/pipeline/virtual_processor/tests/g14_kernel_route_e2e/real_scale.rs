//! **K2-TIR-v4 on the real node (lane K2S, `docs/design/palw/k2-real-scale.md`)**: segmented claims through the mempool, the node's
//! template, the chain block's fold, the persisted tip and the read API — a tiled prompt past 4,096 ids as a multi-segment claim (an
//! honest element filed against it dismissed), a lie in one segment localized to one element and convicted with bounded bytes, a
//! withheld segment demanded and defaulted, a demanded position served on chain and convicted from the blocks' bytes, the mempool's
//! acceptance gate (GAP 10) and the cached ledger (GAP 8) — and G14's own statement: the producer and every other bond collude (passing
//! receipts, nothing published, no prosecution), and one bond outside them convicts one lie and defaults another through the real path.
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
        f.plan.descriptor_digest,
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

/// The largest object one carrier takes here (the canonical harness's chunk size): a larger one rides `ObjectChunk`s, since a
/// `Respond` part (≤ `SEG_PART_BYTES_V4` = 1 MiB) or a class registration can exceed a block's compute mass in one carrier.
const SEG_CARRIER_CAP: usize = 100_000;

impl Net {
    /// [`Net::send`] for objects of any size: one carrier when the object fits [`SEG_CARRIER_CAP`], else its generic `ObjectChunk`s
    /// (judged on the assembled whole), each card's in order.
    async fn send_all(&mut self, items: Vec<(usize, Obj)>) -> Block {
        let mut out = Vec::new();
        for (card, o) in items {
            match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, SEG_CARRIER_CAP).expect("chunks") {
                None => out.push((card, o)),
                Some(chunks) => out.extend(chunks.into_iter().map(|c| (card, c))),
            }
        }
        self.send(out).await
    }
}

/// **Every `Respond` part the selected chain carries for `(claim, position)`**, read back from the blocks' transactions the way a fresh
/// verifier reads them (a chunked carrier reassembled from its `ObjectChunk`s): the ledger keeps none of their bytes.
fn served_parts_from_blocks(net: &Net, claim: &Digest, position: u32) -> Vec<Vec<u8>> {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2;
    let mut objects = Vec::new();
    let mut groups: BTreeMap<Vec<u8>, (u8, BTreeMap<u8, Vec<u8>>)> = BTreeMap::new();
    for b in chain_blocks(&net.chain, net.chain.sink()) {
        for tx in b.transactions.iter() {
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
                continue;
            }
            let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
            match payload.object {
                Obj::ObjectChunk { group, index, count, bytes } => {
                    let key = borsh::to_vec(&group).expect("a group id serializes");
                    let entry = groups.entry(key).or_insert_with(|| (count, BTreeMap::new()));
                    entry.1.insert(index, bytes);
                    if entry.1.len() == entry.0 as usize {
                        let whole: Vec<u8> = entry.1.values().flatten().copied().collect();
                        if let Ok(o) = borsh::from_slice::<Obj>(&whole) {
                            objects.push(o);
                        }
                    }
                }
                o => objects.push(o),
            }
        }
    }
    let mut out = Vec::new();
    for o in objects {
        let Obj::KernelRouteV1 { bytes, .. } = o else { continue };
        if let Ok(K::Respond { claim: c, stage: 0, position: p, bytes }) = K::decode(&bytes)
            && c == *claim
            && p == position
        {
            out.push(bytes);
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
        SegWorld::with(TestConsensus::new).await
    }

    /// The K2-TIR-v4 class registered on `net` through the real path (a Panel registration of it is dropped first).
    async fn register(mut net: Net, f: SegFixture) -> SegWorld {
        // The class is derived-eligible for OPV through the test seam (no onboarding binding on this node), as the parent's are.
        crate::pipeline::virtual_processor::processor::kernel_route_test_opv_eligible_v1(seg_class_id(&f));
        net.beat_to(1).await;
        // The plan's descriptor: K2-TIR-v4, or K2-TIR-v5 for an encoder.
        let descriptor = f.plan.descriptor_digest;
        // A Panel-licensed registration of a v4/v5 class is dropped by the ledger (no Panel can cover a real-scale claim).
        let panel =
            K::RegisterClass { descriptor, program_bytes: f.program.encode(), plan: f.plan.clone(), param_commitments: f.pc.clone() };
        let o = net.route(1, &panel);
        net.send(vec![(1, o)]).await;
        assert!(net.api().is_none_or(|k| k.ledger().unwrap().classes.is_empty()), "no Panel route for a K2-TIR-v4 class");
        let register = K::RegisterClassV2 {
            mode: VerificationModeV1::OptimisticPublicVerification,
            descriptor,
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

    /// **Seal, then reveal, a segmented claim** (its producer's): past `palw_panel_free_v1` — every OPV network here — the seal is
    /// claim seal v2 and the reveal carries its salt (`CommitClaimSalted { Segmented }`, G14R's GAP-B1a); below it, seal v1 and the
    /// plain `CommitSegmentedClaim`.
    async fn seal_and_commit(
        &mut self,
        producer: usize,
        claim: KernelClaimV1,
        evidence: misaka_palw_kernel::seg::SegmentedEvidenceV2,
        segment_roots: Vec<Digest>,
    ) {
        let (kid, job, id) = (self.net.kid(producer), claim.job_id, claim.id());
        let ledger = self.net.ledger();
        let (seal, reveal) = if ledger.salted_seals_from().is_some_and(|at| ledger.daa.saturating_add(1) >= at) {
            let salt = misaka_palw_kernel::hash::id(b"misaka-palw/test/claim-salt", &id);
            let commit = SaltedCommitV1::Segmented { claim, evidence, segment_roots };
            (K::SealClaim { producer: kid, job, seal: claim_seal_v2(&id, &salt) }, K::CommitClaimSalted { salt, commit })
        } else {
            (K::SealClaim { producer: kid, job, seal: claim_seal_v1(&id) }, K::CommitSegmentedClaim { claim, evidence, segment_roots })
        };
        let o = self.net.route(producer, &seal);
        self.net.send_all(vec![(producer, o)]).await;
        let o = self.net.route(producer, &reveal);
        self.net.send_all(vec![(producer, o)]).await;
    }

    /// A tiled job of `prompt` posted by card 1, then every tile of it (any bond: card 2).
    async fn tiled_job(&mut self, prompt: &[u32]) -> Digest {
        self.tiled_job_generating(prompt, 2).await
    }

    /// [`Self::tiled_job`] of a job that generates at most `max_new_tokens` ids.
    async fn tiled_job_generating(&mut self, prompt: &[u32], max_new_tokens: u32) -> Digest {
        self.jobs += 1;
        let job = TiledJobV1 {
            class_binding_id: self.class,
            prompt_len: prompt.len() as u32,
            prompt_root: prompt_root_of_ids_v1(prompt),
            max_new_tokens,
            decode: DecodeRuleV1::Greedy,
            nonce: [self.jobs; 64],
        };
        let o = self.net.route(1, &K::PostTiledJob { job: job.clone() });
        self.net.send_all(vec![(1, o)]).await;
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
        self.net.send_all(tiles).await;
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
        self.claim_with(producer, job, prompt, lie, false).await
    }

    /// [`Self::claim`]; with `wrong_last` the producer delivers a last id that is not the greedy decode of its (honest) logits — an
    /// output lie, bound by nothing at commit (the commitment binds the FED ids only).
    async fn claim_with(
        &mut self,
        producer: usize,
        job: Digest,
        prompt: &[u32],
        lie: Option<(u32, u16, u16)>,
        wrong_last: bool,
    ) -> SegClaim {
        let edit = move |values: &mut Vec<Vec<Vec<Tensor>>>| {
            if let Some((p, s, n)) = lie {
                bump(&mut values[p as usize][s as usize][n as usize], 0);
            }
        };
        self.claim_over(producer, job, prompt, prompt, &edit, wrong_last).await
    }

    /// **The general claim**: the commitment binds the job's prompt `bound`, the producer computes over `traced` (the same length;
    /// another id is an input lie, another prompt a borrowed trace), `edit` changes the values it commits, and `wrong_last` delivers a
    /// last id that is not the decode of its logits. The public token stream is `bound` and the fed id.
    async fn claim_over(
        &mut self,
        producer: usize,
        job: Digest,
        bound: &[u32],
        traced: &[u32],
        edit: &dyn Fn(&mut Vec<Vec<Vec<Tensor>>>),
        wrong_last: bool,
    ) -> SegClaim {
        assert_eq!(bound.len(), traced.len());
        let prompt = bound;
        let post = (self.f.program.occurrences().len() - 1) as usize;
        let logits = self.f.program.logits as usize;
        let first = trace_v1(&self.f.program, &self.f.params, traced).unwrap();
        let g0 = DecodeRuleV1::Greedy.select(&first.values[traced.len() - 1][post][logits]).unwrap();
        let mut fed = traced.to_vec();
        fed.push(g0);
        let trace = trace_v1(&self.f.program, &self.f.params, &fed).unwrap();
        let mut g1 = DecodeRuleV1::Greedy.select(&trace.values[fed.len() - 1][post][logits]).unwrap();
        if wrong_last {
            g1 = (g1 + 1) % self.f.program.token_bound;
        }
        let mut tokens = bound.to_vec();
        tokens.push(g0);
        let mut values = trace.values;
        edit(&mut values);
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
        self.seal_and_commit(producer, claim, evidence, c.segment_roots()).await;
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
    w.net.send_all(vec![(outsider, o)]).await;
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
    let e = &fault;
    assert_eq!(e.at(), Some((1200, s, n)), "localized to the lying value");
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
    w.net.send_all(vec![(outsider, o)]).await;
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
    w.net.send_all(demands).await;
    assert_eq!(w.net.ledger().demands.keys().filter(|(c, _, _)| *c == claim.id).count(), 2, "two sessions, one per position");
    assert!(w.net.kernel_reserved(outsider) > 0, "the demand bonds are reserved on the real bond");
    // A response with another position's values: its position root is not the committed one.
    let wrong =
        misaka_palw_kernel::seg_da::position_part_v1(&w.f.program, 1050, &claim.values[1048], claim.c.position_path(1050).1, 0)
            .unwrap();
    let o = w.net.route(0, &K::Respond { claim: claim.id, stage: 0, position: 1050, bytes: borsh::to_vec(&wrong).unwrap() });
    w.net.send_all(vec![(0, o)]).await;
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
    w.net.send_all(vec![(3, genuine)]).await;
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
    w.net.send_all(demands).await;
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
    w.net.send_all(answers).await;
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
    w.net.send_all(vec![(outsider, o)]).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "the served values convict the claim");
    assert!(matches!(ledger.claims[&lie.id].life.state, ClaimStateV1::Convicted { .. }));
    assert!(w.net.slashed(0) > slashed_before, "the producer's real bond is slashed");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bonds return with the conviction");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of a conviction from served material");
}

/// A receipt saying `claim` passed, signed by card `card` as if it sat the claim's Panel. No seat was drawn for the claim, so the
/// coalition forges every field it cannot read from the chain.
fn coalition_pass(w: &SegWorld, card: usize, claim: &Digest, positions: u32) -> misaka_palw_kernel::receipt::PalwConstraintReceiptV1 {
    let state = w.net.chain.tip_state().1;
    let operator = state.bond(&w.net.bond(card)).expect("a genesis card").operator_id.as_bytes();
    misaka_palw_kernel::receipt::PalwConstraintReceiptV1 {
        network_domain: w.net.domain.as_bytes(),
        claim_id: *claim,
        class_id: w.class,
        verification_profile_hash: [0x11; 64],
        verification_scheme_id: 1,
        scheme_version: 1,
        descriptor_digest: k2_tir_v4_descriptor().digest(),
        assignment_root: [0x22; 64],
        scope: misaka_palw_kernel::verify::ScopeV1::WholeClaim,
        scope_root: [0x33; 64],
        covered_positions: positions,
        challenge_anchor: [0x44; 64],
        sample_seed: [0x55; 64],
        sample_count: 1 << 20,
        field_policy_id: 1,
        freivalds_rounds: 2,
        soundness_policy_id: 1,
        derived_soundness_bits: 128,
        evidence_manifest_root: [0x66; 64],
        verdict: misaka_palw_kernel::receipt::ReceiptVerdictV1::Pass,
        seat_bond: w.net.kid(card),
        seat_operator: operator,
        signed_daa: w.net.daa(),
    }
}

impl SegWorld {
    /// **The coalition's whole Panel power**: every card in `coalition` signs a passing receipt for `claim`. Each carrier is put to the
    /// mempool first (its funding restored, so a refused one costs the card nothing); the admitted ones ride the node's template and
    /// fold. Returns how many the mempool refused (a bond the route never saw sign is refused by name).
    async fn coalition_passes(&mut self, coalition: &[usize], claim: &Digest, positions: u32) -> usize {
        let mut admitted = Vec::new();
        let mut refused = 0;
        for &card in coalition {
            let receipt = coalition_pass(self, card, claim, positions);
            let o = self.net.receipt(card, receipt);
            let funding = self.net.funding[card].clone();
            let tx = self.net.carrier(card, &o);
            let verdict = self.net.mempool(&tx);
            self.net.funding[card] = funding;
            match verdict {
                Ok(()) => admitted.push((card, o)),
                Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwKernelRouteRefused(why)) => {
                    assert!(why.contains("never assigned"), "card {card}: {why}");
                    refused += 1;
                }
                Err(e) => panic!("card {card}: {e}"),
            }
        }
        assert!(!admitted.is_empty(), "the bonds the route has seen sign get their receipts mined");
        self.net.send_all(admitted).await;
        refused
    }
}

impl SegWorld {
    /// **The outsider's demand, the producer's service and the read back from the blocks**: `missing` demanded by card `outsider`,
    /// every part of each served on chain by the producer (card 0), and the positions assembled from the chain's blocks.
    async fn demand_served_and_read(&mut self, outsider: usize, claim: &SegClaim, missing: &[u32]) -> FromBlocks {
        let demands: Vec<(usize, Obj)> = missing
            .iter()
            .map(|p| {
                let demand = K::FileDemand { demander: self.net.kid(outsider), claim: claim.id, stage: 0, position: *p };
                (outsider, self.net.route(outsider, &demand))
            })
            .collect();
        self.net.send_all(demands).await;
        self.serve_and_read(claim, missing).await
    }

    /// The producer (card 0) serves every part of each open demand of `missing` on chain, and the positions are assembled from the
    /// chain's blocks, as a fresh verifier reads them.
    async fn serve_and_read(&mut self, claim: &SegClaim, missing: &[u32]) -> FromBlocks {
        let view = self.fresh_record(&claim.id);
        let mut answers = Vec::new();
        for &p in missing {
            for i in 0..position_parts_v1(&self.f.program, p).unwrap().len() as u32 {
                let part = position_part_v1(&self.f.program, p, &claim.values[p as usize], claim.c.position_path(p).1, i).unwrap();
                let respond = K::Respond { claim: claim.id, stage: 0, position: p, bytes: borsh::to_vec(&part).unwrap() };
                answers.push((0usize, self.net.route(0, &respond)));
            }
        }
        self.net.send_all(answers).await;
        let mut positions = BTreeMap::new();
        for &p in missing {
            let parts = served_parts_from_blocks(&self.net, &claim.id, p);
            let assembled =
                assemble_position_v1(&self.f.program, &view.segment_roots, view.positions, p, &parts).expect("authentic parts");
            positions.insert(p, assembled);
        }
        FromBlocks { positions, read: Cell::new(0) }
    }
}

/// **G14 at real scale against the whole coalition: the producer and every other bond but one collude, and the one ordinary bond
/// outside them reaches a conviction and a default through the real chain path.** The class is K2-TIR-v4 under OPV, so the route draws
/// no seats for its claims; the coalition does everything a Panel could:
/// * it signs passing receipts for the lying claim (the bonds the route has seen sign get theirs through the mempool, the template and
///   the fold; the rest are refused at admission by name). No seat was drawn, so none counts: the claim is never passed and nobody is
///   paid;
/// * it publishes nothing off-chain and never prosecutes.
///
/// The outsider is the last genesis card, a bond the route has never seen. From the node's read API and the chain's blocks only, it
/// checks position 1,200 of the first lie. That check is a demand of positions 1,199 and 1,200. The coalition's producer serves them on
/// chain (else it defaults). The outsider reads the parts back from the blocks and files one element court, and the fold convicts.
/// The real bond is slashed, the slash splits exactly into the accuser's reward and the burn, and the next coinbase pays the reward.
/// The second lie's producer withholds instead: at the demand's deadline the claim is Unavailable — the default charged, never a
/// conviction, never Final. The third lie is in the output (a delivered id that is not the decode of honest logits): the decode court
/// convicts it from what the producer had to serve. A replaying node agrees with every root.
#[tokio::test]
async fn g14_k2s_the_producer_and_every_other_bond_collude_and_one_outside_bond_convicts_through_the_real_path() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let coalition: Vec<usize> = (0..outsider).collect();
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 11 + 3) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let positions = lie.c.positions();
    assert!(w.net.api().unwrap().assignment_of(&lie.id).is_none(), "the route draws no seats for an OPV claim");
    assert!(w.net.api().unwrap().bond_key_of(&w.net.kid(outsider)).is_none(), "the outsider is a bond the route has never seen");

    // The coalition's passes: mined where admitted, and of no effect.
    let refused = w.coalition_passes(&coalition, &lie.id, positions).await;
    assert!(refused > 0, "receipts of bonds the route never saw are refused at admission");
    let api = w.net.api().unwrap();
    assert!(api.receipts_of(&lie.id).is_empty() && api.assignment_of(&lie.id).is_none(), "no receipt counts for an OPV claim");
    let state = w.net.claim_state(&lie.id);
    assert!(!matches!(state, ClaimStateV1::ProbabilisticPass { .. } | ClaimStateV1::Final { .. }), "never passed: {state:?}");
    assert!(coalition.iter().all(|c| w.net.owed(*c) == 0), "and nobody is paid for it");

    // The outsider: the read API's record, nothing published off-chain.
    let view = w.fresh_record(&lie.id);
    // The verifier's own copy of the artifact (owned: the world is driven while it checks).
    let params = w.f.params.clone();
    let art = move |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).cloned();
    let nothing = SegDa { claim: &lie, withhold: 0..positions, read: Cell::new(0) };
    let SegFindingV1::Demand(missing) = check_positions_v1(&view.context(), &nothing, &art, &lie.tokens, &[1200]) else {
        panic!("material nobody published is a demand")
    };
    assert_eq!(missing, vec![1199, 1200]);
    // The producer must serve or default: it serves, on chain, in parts, and the outsider reads them back from the blocks.
    let chain = w.demand_served_and_read(outsider, &lie, &missing).await;
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &lie.tokens, &[1200]) else {
        panic!("the served lie is found")
    };
    let bytes = fault.to_bytes();
    let bounds = w.net.ledger().classes[&w.class].bounds;
    assert!(bytes.len() as u64 <= bounds.max_filing_bytes, "the filing is within the class's priced bound");
    let (collateral, burned) = (w.net.collateral(0), w.net.ledger().burned);
    let o =
        w.net.route(outsider, &K::FileProof { accuser: w.net.kid(outsider), claim: lie.id, proof: ProsecutionV1::Segmented(bytes) });
    w.net.send_all(vec![(outsider, o)]).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "convicted by the one outside bond");
    assert!(matches!(ledger.claims[&lie.id].life.state, ClaimStateV1::Convicted { .. }));
    // Conservation: the real bond's loss is exactly the accuser's reward plus the burn, at the policy's share.
    let slashed = collateral - w.net.collateral(0);
    let reward = w.net.owed(outsider);
    assert!(slashed > 0 && reward > 0, "the producer's real bond is slashed and the accuser is owed its share");
    assert_eq!(reward, slashed * u64::from(ledger.policy.accuser_reward_permille) / 1000, "the accuser's share");
    assert_eq!(ledger.burned - burned, slashed - reward, "the rest is burned: slash = reward + burn");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bonds return with the conviction");
    let ttpb = w.net.ttpb();
    let next = w.net.chain.heartbeat(ttpb, Vec::new()).await;
    assert!(
        next.transactions[0].outputs.iter().any(|o| o.value == reward && o.script_public_key == card_payout_spk(outsider)),
        "the next coinbase pays the outsider its share"
    );
    assert_eq!(w.net.owed(outsider), 0, "and the queue is drained");

    // The second lie: the coalition withholds. The outsider's demand runs out; the claim is the producer's default.
    let prompt2: Vec<u32> = (0..1100u32).map(|i| (i * 3 + 7) % 32).collect();
    let job2 = w.tiled_job(&prompt2).await;
    let lie2 = w.claim(0, job2, &prompt2, Some((1050, s, n))).await;
    w.coalition_passes(&coalition, &lie2.id, lie2.c.positions()).await;
    let view2 = w.fresh_record(&lie2.id);
    let nothing2 = SegDa { claim: &lie2, withhold: 0..lie2.c.positions(), read: Cell::new(0) };
    let SegFindingV1::Demand(missing2) = check_positions_v1(&view2.context(), &nothing2, &art, &lie2.tokens, &[1050]) else {
        panic!("a demand")
    };
    let demands: Vec<(usize, Obj)> = missing2
        .iter()
        .map(|p| {
            (outsider, w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: lie2.id, stage: 0, position: *p }))
        })
        .collect();
    w.net.send_all(demands).await;
    let deadline = w.net.ledger().demands.iter().filter(|((c, _, _), _)| *c == lie2.id).map(|(_, d)| d.deadline_daa).max().unwrap();
    let slashed_before = w.net.slashed(0);
    w.net.beat_to(deadline + 2).await;
    let ledger = w.net.ledger();
    let row = &ledger.claims[&lie2.id];
    assert!(!row.convicted, "withholding is never a conviction");
    assert!(matches!(row.life.state, ClaimStateV1::Unavailable { .. }), "{:?}", row.life.state);
    assert!(w.net.slashed(0) > slashed_before, "the default is charged to the real bond");
    assert_eq!(w.net.kernel_reserved(outsider), 0, "the demand bonds return");
    // The third lie is in the output: honest values, a last delivered id that is not their greedy decode. The commitment binds the fed
    // ids only, so it commits; the outsider checks the selecting position from what the producer must serve, and the decode court
    // convicts from two leaves of the committed logits.
    let prompt3: Vec<u32> = (0..900u32).map(|i| (i * 5 + 1) % 32).collect();
    let job3 = w.tiled_job(&prompt3).await;
    let lie3 = w.claim_with(0, job3, &prompt3, None, true).await;
    w.coalition_passes(&coalition, &lie3.id, lie3.c.positions()).await;
    let last = lie3.c.positions() - 1;
    let view3 = w.fresh_record(&lie3.id);
    let nothing3 = SegDa { claim: &lie3, withhold: 0..lie3.c.positions(), read: Cell::new(0) };
    let SegFindingV1::Demand(missing3) = check_positions_v1(&view3.context(), &nothing3, &art, &lie3.tokens, &[last]) else {
        panic!("a demand")
    };
    let chain3 = w.demand_served_and_read(outsider, &lie3, &missing3).await;
    let SegFindingV1::Fault(fault3) = check_positions_v1(&view3.context(), &chain3, &art, &lie3.tokens, &[last]) else {
        panic!("the output lie is found")
    };
    assert!(matches!(fault3.as_ref(), SegFaultV1::Decode(_)), "a decode fault: {fault3:?}");
    let bytes3 = fault3.to_bytes();
    assert!(bytes3.len() as u64 <= w.net.ledger().classes[&w.class].bounds.max_filing_bytes, "the decode filing is priced");
    let o =
        w.net.route(outsider, &K::FileProof { accuser: w.net.kid(outsider), claim: lie3.id, proof: ProsecutionV1::Segmented(bytes3) });
    w.net.send_all(vec![(outsider, o)]).await;
    assert!(w.net.ledger().claims[&lie3.id].convicted, "the output lie is convicted by the outside bond");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the coalition's convictions and default");
}

// ---- GAP-30: the lie types, the filers and recovery, for K2-TIR-v4 on the node -------------------------------------------------

/// A garbage trace: every committed value refilled with values of its dtype that no computation produced.
fn garble(values: &mut Vec<Vec<Vec<Tensor>>>) {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    for t in values.iter_mut().flatten().flatten() {
        let (lo, hi) = (t.dtype.min_value(), t.dtype.max_value());
        // (an `i128` value's span overflows `i128`: it is capped like every other)
        let span = hi.checked_sub(lo).map_or(1 << 16, |d| d.saturating_add(1).min(1 << 16));
        for d in t.data.iter_mut() {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            *d = lo + ((x >> 33) as i128) % span;
        }
    }
}

impl SegWorld {
    /// The world over a consensus the caller builds (a database it keeps, to restart the node over it).
    async fn with(make: impl FnOnce(&Config) -> TestConsensus) -> SegWorld {
        let f = seg_fixture();
        let net = Net::over_cfg(kernel_config_opv(vec![seg_class_id(&f)]), make);
        SegWorld::register(net, f).await
    }

    /// **A real restart** of the world's node over the same database.
    fn restart(self, db: std::sync::Arc<kaspa_database::prelude::DB>) -> SegWorld {
        SegWorld { net: self.net.restart(db), f: self.f, class: self.class, jobs: self.jobs }
    }

    /// A direct proof by `card` of `fault`.
    async fn file(&mut self, card: usize, claim: &Digest, fault: &SegFaultV1) {
        let o = self.net.route(
            card,
            &K::FileProof { accuser: self.net.kid(card), claim: *claim, proof: ProsecutionV1::Segmented(fault.to_bytes()) },
        );
        self.net.send_all(vec![(card, o)]).await;
    }

    /// The outsider's check of `positions` over what the producer published (everything it committed), by name.
    fn fault_from_published(&self, claim: &SegClaim, positions: &[u32]) -> SegFaultV1 {
        let view = self.fresh_record(&claim.id);
        let art = |j: u16, l: Option<u16>| self.f.params.tensors.get(&(j, l)).cloned();
        let da = SegDa { claim, withhold: 0..0, read: Cell::new(0) };
        match check_positions_v1(&view.context(), &da, &art, &claim.tokens, positions) {
            SegFindingV1::Fault(f) => *f,
            other => panic!("the lie is not found: {other:?}"),
        }
    }
}

/// **Input lies, a borrowed trace and a garbage trace are convicted on the node (GAP-30).** Three claims of one 4,500-id tiled job
/// whose ids sit in two tiles:
/// * an INPUT lie: the producer computes position 4,200 over an id that is not the posted tile's (the commitment binds the job's
///   prompt root, so it commits). The first wrong value is the embedding row read by the token; its court opens TILE 1 (authenticated
///   against the prompt root) and convicts;
/// * a BORROWED trace: an honest trace of another prompt committed under this job. Position 0's embedding row is already wrong; tile 0
///   convicts;
/// * a GARBAGE trace: every value refilled with in-range noise. The first value convicts.
///
/// Each is found by the outsider's check of one position and filed through the real path; a replaying node agrees.
#[tokio::test]
async fn g14_k2s_input_borrowed_and_garbage_traces_are_convicted_on_the_node() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..4500u32).map(|i| (i * 13 + 5) % 32).collect();
    // The input lie.
    let job = w.tiled_job(&prompt).await;
    let mut traced = prompt.clone();
    traced[4200] = (traced[4200] + 1) % 32;
    let lie = w.claim_over(0, job, &prompt, &traced, &|_| {}, false).await;
    let fault = w.fault_from_published(&lie, &[4200]);
    let e = &fault;
    assert_eq!(e.at(), Some((4200, 0, 0)), "the embedding row the token reads");
    assert_eq!(e.token().map(|t| t.index), Some(1), "opened by the job's second tile");
    w.file(outsider, &lie.id, &fault).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "an input lie is convicted");
    // The borrowed trace.
    let job = w.tiled_job(&prompt).await;
    let other: Vec<u32> = prompt.iter().map(|t| (t + 7) % 32).collect();
    let borrowed = w.claim_over(0, job, &prompt, &other, &|_| {}, false).await;
    let fault = w.fault_from_published(&borrowed, &[0]);
    let e = &fault;
    assert_eq!((e.at(), e.token().map(|t| t.index)), (Some((0, 0, 0)), Some(0)));
    w.file(outsider, &borrowed.id, &fault).await;
    assert!(w.net.ledger().claims[&borrowed.id].convicted, "a borrowed trace is convicted");
    // The garbage trace.
    let job = w.tiled_job(&prompt).await;
    let garbage = w.claim_over(0, job, &prompt, &prompt, &garble, false).await;
    let fault = w.fault_from_published(&garbage, &[0]);
    w.file(outsider, &garbage.id, &fault).await;
    assert!(w.net.ledger().claims[&garbage.id].convicted, "a garbage trace is convicted");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the input, borrowed and garbage convictions");
}

/// **A lie nobody prosecutes in the window is convicted after Final, inside the liability horizon (GAP-30).** The claim finalizes at
/// the window's end with no Panel and its producer is owed the Final reward; the outsider then convicts it from the published values.
/// The reservation is slashed post-Final and the accuser is owed its share.
#[tokio::test]
async fn g14_k2s_a_lie_is_convicted_after_final_within_the_liability_horizon() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1200u32).map(|i| (i * 7 + 3) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((700, s, n))).await;
    let daa = w.net.daa() + 80;
    w.net.beat_to(daa).await;
    assert!(matches!(w.net.claim_state(&lie.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&lie.id));
    assert!(w.net.ledger().claims[&lie.id].liability_until.is_some_and(|u| u > w.net.daa()), "liability runs");
    let (collateral, owed) = (w.net.collateral(0), w.net.owed(outsider));
    let fault = w.fault_from_published(&lie, &[700]);
    w.file(outsider, &lie.id, &fault).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "convicted post-Final");
    assert!(w.net.collateral(0) < collateral, "the reservation is slashed post-Final");
    assert!(w.net.owed(outsider) > owed, "the accuser is owed its share");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of a post-Final conviction");
}

/// **Permissionless filing against the coalition (GAP-30, C6).** The coalition opens every demand session its bonds may hold on the
/// lying claim (spam: positions the outsider never needs) — a direct proof never waits on them. Two outsiders file in the SAME block:
/// one conviction, one duplicate, one slash, exactly one reward. A third filing of the same proof later changes nothing, and the
/// spam demands settle moot (their bonds return).
#[tokio::test]
async fn g14_k2s_spam_demands_never_preempt_a_proof_and_simultaneous_filers_get_one_conviction() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let cards = w.net.chain.bonds.len();
    let (a, b) = (cards - 1, cards - 2);
    let coalition: Vec<usize> = (1..cards - 2).collect();
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 5 + 2) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    // The spam: each coalition bond holds SEG_OPEN_PER_DEMANDER_V4 sessions on positions nobody needs.
    let per = misaka_palw_kernel::seg_da::SEG_OPEN_PER_DEMANDER_V4;
    let mut spam = Vec::new();
    for (i, &c) in coalition.iter().enumerate() {
        for k in 0..per {
            let position = 10 + i as u32 * per + k;
            spam.push((c, w.net.route(c, &K::FileDemand { demander: w.net.kid(c), claim: lie.id, stage: 0, position })));
        }
    }
    // What each coalition bond holds reserved before its spam (card 1 also holds the jobs' escrows).
    let reserved_before: Vec<u128> = coalition.iter().map(|c| w.net.kernel_reserved(*c)).collect();
    w.net.send_all(spam).await;
    let open = w.net.ledger().demands.keys().filter(|(c, _, _)| *c == lie.id).count();
    assert_eq!(open, coalition.len() * per as usize, "every spam session is open");
    let fault = w.fault_from_published(&lie, &[1200]);
    let (collateral, ledger_burned) = (w.net.collateral(0), w.net.ledger().burned);
    let oa = w.net.route(a, &K::FileProof { accuser: w.net.kid(a), claim: lie.id, proof: ProsecutionV1::Segmented(fault.to_bytes()) });
    let ob = w.net.route(b, &K::FileProof { accuser: w.net.kid(b), claim: lie.id, proof: ProsecutionV1::Segmented(fault.to_bytes()) });
    w.net.send_all(vec![(a, oa), (b, ob)]).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&lie.id].convicted, "the direct proof is not pre-empted by the open sessions");
    let slashed = collateral - w.net.collateral(0);
    let reward = slashed * u64::from(ledger.policy.accuser_reward_permille) / 1000;
    let (ra, rb) = (w.net.owed(a), w.net.owed(b));
    assert!((ra == reward && rb == 0) || (ra == 0 && rb == reward), "exactly one accuser is paid: {ra} / {rb} (reward {reward})");
    assert_eq!(ledger.burned - ledger_burned, slashed - reward, "one slash: reward + burn");
    assert!(ledger.demands.keys().all(|(c, _, _)| *c != lie.id), "the spam demands are moot");
    let reserved_after: Vec<u128> = coalition.iter().map(|c| w.net.kernel_reserved(*c)).collect();
    assert_eq!(reserved_after, reserved_before, "and their demand bonds return");
    // A later duplicate changes nothing (the first reward has been paid by a coinbase meanwhile: nothing new is queued).
    let row = ledger.claims[&lie.id].clone();
    let collateral = w.net.collateral(0);
    w.file(a, &lie.id, &fault).await;
    assert_eq!(w.net.ledger().claims[&lie.id], row, "the claim row is untouched");
    assert_eq!((w.net.collateral(0), w.net.owed(a), w.net.owed(b)), (collateral, 0, 0), "no second slash or reward");
}

/// The rows of the K2-TIR-v4 tables (20: tiled jobs, 21: demand progress) in a route state.
fn seg_tables(route: &PalwKernelRouteStateV1) -> Vec<(misaka_palw_kernel::rows::RowKeyV1, Vec<u8>)> {
    use misaka_palw_kernel::rows::{TABLE_SEG_PROGRESS_V1, TABLE_TILED_JOBS_V1};
    route
        .rows
        .iter()
        .filter(|((t, _), _)| *t == TABLE_TILED_JOBS_V1 || *t == TABLE_SEG_PROGRESS_V1)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// **A reorg across live K2-TIR-v4 tables (GAP-30, C8).** At the fork the route holds a tiled job (table 20) and two open demands with
/// their progress rows (table 21). Chain A serves them and convicts; a heavier chain B from the fork never saw either. A node that
/// follows A and reorgs onto B is back at the fork's rows EXACTLY — tables 20 and 21 included, the slash returned, no reward queued —
/// and when A out-works B again the service and the conviction return identically.
#[tokio::test]
async fn g14_k2s_a_reorg_across_live_tiled_job_and_progress_rows_restores_them_exactly() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 3 + 1) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let demands: Vec<(usize, Obj)> = [1199u32, 1200]
        .iter()
        .map(|p| {
            (outsider, w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: lie.id, stage: 0, position: *p }))
        })
        .collect();
    w.net.send_all(demands).await;
    // The producer serves (the progress rows record it and hold the demand bonds through the proof grace).
    let chain = w.serve_and_read(&lie, &[1199, 1200]).await;
    // The fork: a shallow one, so a heavier branch is GHOSTDAG's to choose (a deep all-economic tie is the strict-win rule's).
    let fork = w.net.chain.sink();
    let at_fork = w.net.api().expect("the route");
    let tables = seg_tables(&at_fork);
    assert!(tables.iter().any(|((t, _), _)| *t == misaka_palw_kernel::rows::TABLE_TILED_JOBS_V1), "table 20 is live at the fork");
    assert_eq!(tables.iter().filter(|((t, _), _)| *t == misaka_palw_kernel::rows::TABLE_SEG_PROGRESS_V1).count(), 2, "table 21 too");
    let collateral = w.net.collateral(0);
    // A: the outsider convicts from the blocks.
    let view = w.fresh_record(&lie.id);
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &lie.tokens, &[1200]) else { panic!("found") };
    w.file(outsider, &lie.id, &fault).await;
    assert!(w.net.ledger().claims[&lie.id].convicted);

    // Z follows A.
    let z = w.net.replay().await;
    w.net.assert_same(&z, "Z on A");
    let zn = w.net.on_chain(z);
    // B: from the fork, heavier, without the service or the proof.
    let b = t12_genesis_chain(&w.net.config, &w.net.bundle, &w.net.premine, &w.net.floats);
    let up_to_fork = chain_blocks(&w.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = w.net.ttpb();
    let a_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len() - chain_blocks(&w.net.chain, fork).len();
    let mut b_blocks = Vec::new();
    for _ in 0..a_len.max(2) + 2 {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "B out-works A: Z reorgs onto B");
    let on_b = zn.api().expect("the route");
    assert_eq!(seg_tables(&on_b), seg_tables(&b.ctx.consensus.palw_kernel_route_v1().unwrap()), "Z's tables are B's");
    assert_eq!(seg_tables(&on_b), tables, "tables 20 and 21 are the fork's, exactly (no service on B)");
    assert!(!zn.ledger().claims[&lie.id].convicted, "the conviction is reorged out");
    assert_eq!(zn.collateral(0), collateral, "the slash is returned");
    assert_eq!(zn.owed(outsider), 0, "no reward queued");
    // A out-works B: Z comes back, service and conviction identical.
    let old_len = chain_blocks(&w.net.chain, w.net.chain.sink()).len();
    for _ in 0..b_blocks.len() + 2 {
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    for blk in chain_blocks(&w.net.chain, w.net.chain.sink()).into_iter().skip(old_len) {
        arrive(&zn.chain, blk, "A's later block").await;
    }
    w.net.assert_same(&zn.chain, "Z back on A");
    assert!(zn.ledger().claims[&lie.id].convicted, "the conviction is back");
}

/// **A real restart with live K2-TIR-v4 tables (GAP-30, C8).** A tiled job, a segmented claim and two open demands, one position
/// already served; the node stops and reopens over the same database: the route's rows (tables 20 and 21 included) come off disk,
/// the producer serves the other position after the restart, the outsider convicts, and a node replaying the whole chain agrees.
#[tokio::test]
async fn g14_k2s_survives_a_node_restart_with_live_tiled_job_and_progress_rows() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut w = SegWorld::with(|c| TestConsensus::with_db(db.clone(), c, sender)).await;
    w.net._keep.push(Box::new(receiver));
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 9 + 4) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let demands: Vec<(usize, Obj)> = [1199u32, 1200]
        .iter()
        .map(|p| {
            (outsider, w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: lie.id, stage: 0, position: *p }))
        })
        .collect();
    w.net.send_all(demands).await;
    let serve = |w: &mut SegWorld, p: u32| -> Vec<(usize, Obj)> {
        (0..position_parts_v1(&w.f.program, p).unwrap().len() as u32)
            .map(|i| {
                let part = position_part_v1(&w.f.program, p, &lie.values[p as usize], lie.c.position_path(p).1, i).unwrap();
                (0usize, w.net.route(0, &K::Respond { claim: lie.id, stage: 0, position: p, bytes: borsh::to_vec(&part).unwrap() }))
            })
            .collect()
    };
    let first = serve(&mut w, 1199);
    w.net.send_all(first).await;
    let (sink, root, route) = (w.net.chain.sink(), w.net.chain.tip_state().1.state_root(), w.net.api());
    let tables = seg_tables(route.as_ref().unwrap());
    assert!(tables.len() >= 3, "a tiled job and two progress rows are live: {}", tables.len());

    let mut w = w.restart(db.clone());
    assert_eq!(w.net.chain.sink(), sink, "the restarted node's sink");
    assert_eq!(w.net.chain.tip_state().1.state_root(), root, "the PALW tip off disk");
    assert_eq!(w.net.api(), route, "the route's rows off disk");
    assert_eq!(seg_tables(w.net.api().as_ref().unwrap()), tables, "tables 20 and 21 off disk");
    let second = serve(&mut w, 1200);
    w.net.send_all(second).await;
    let view = w.fresh_record(&lie.id);
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let mut served = BTreeMap::new();
    for p in [1199u32, 1200] {
        let parts = served_parts_from_blocks(&w.net, &lie.id, p);
        served.insert(p, assemble_position_v1(&w.f.program, &view.segment_roots, view.positions, p, &parts).expect("authentic"));
    }
    let chain = FromBlocks { positions: served, read: Cell::new(0) };
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &lie.tokens, &[1200]) else { panic!("found") };
    w.file(outsider, &lie.id, &fault).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "convicted by the restarted node");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "a node replaying the whole chain");
}

/// **A pruned import with live K2-TIR-v4 tables (GAP-30, C8).** The importer follows A through P — a tiled job (table 20) and an open
/// demand with its progress row (table 21) are live there — is left as a pruned join leaves a node, installs the carriage A serves
/// (the route rides tail 0xEC), and folds the service and the conviction after P to A's roots.
#[tokio::test]
async fn g14_k2s_survives_a_pruned_import_with_live_tiled_job_and_progress_rows() {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::new().await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..1300u32).map(|i| (i * 7 + 6) % 32).collect();
    let job = w.tiled_job(&prompt).await;
    let (_, s, n) = matmul_at(&w.f.program, 0);
    let lie = w.claim(0, job, &prompt, Some((1200, s, n))).await;
    let demands: Vec<(usize, Obj)> = [1199u32, 1200]
        .iter()
        .map(|p| {
            (outsider, w.net.route(outsider, &K::FileDemand { demander: w.net.kid(outsider), claim: lie.id, stage: 0, position: *p }))
        })
        .collect();
    w.net.send_all(demands).await;
    let p = w.net.chain.sink();
    let at_p = seg_tables(&w.net.api().unwrap());
    assert!(at_p.len() >= 3, "table 20 and table 21 rows are live at P");
    let chain = w.serve_and_read(&lie, &[1199, 1200]).await;
    let view = w.fresh_record(&lie.id);
    let art = |j: u16, l: Option<u16>| w.f.params.tensors.get(&(j, l)).cloned();
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &lie.tokens, &[1200]) else { panic!("found") };
    w.file(outsider, &lie.id, &fault).await;
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
    assert_eq!(imported.state_root(), root_at(&w.net.chain, p), "the imported state is P's");
    assert_eq!(
        seg_tables(imported.kernel_route().expect("the route came through the carriage")),
        at_p,
        "tables 20 and 21 came through"
    );
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "A's block after P").await;
        assert_eq!(importer.tip_state().1.state_root(), root_at(&w.net.chain, blk.header.hash), "the importer folds to A's root");
    }
    assert_eq!(importer.ctx.consensus.palw_kernel_route_v1(), w.net.api(), "the same route, rows and aux");
    assert!(importer.ctx.consensus.palw_kernel_route_v1().unwrap().ledger().unwrap().claims[&lie.id].convicted);
}

// ---- GAP-31: a history-bearing class held at 8,192 positions on the node ------------------------------------------------------

/// The sketch's dense-MoE layer (rotary attention over `Hist` k/v windows, a router, experts) as a K2-TIR-v4 class at 8,192
/// positions, its attention a sliding window of `window` rows.
fn held_fixture(window: u32) -> SegFixture {
    let fx = misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, window);
    let d = k2_tir_v4_descriptor();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&fx.program.encode()), SEG_MAX_POSITIONS).unwrap();
    let pc = ParamCommitmentsV1::of_v3(&fx.params);
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(pc.root()), 0);
    SegFixture { program: fx.program, params: fx.params, plan, pc }
}

/// The first `HistAppend` (a window) of occurrence `s`.
fn hist_append_at(program: &TirProgramV1, s: usize) -> u16 {
    let (b, _) = program.occurrences()[s];
    program.blocks[b as usize].nodes.iter().position(|n| matches!(n.prim, misaka_palw_tir::Prim::HistAppend { .. })).expect("a window")
        as u16
}

impl SegWorld {
    async fn with_fixture(f: SegFixture, make: impl FnOnce(&Config) -> TestConsensus) -> SegWorld {
        let net = Net::over_cfg(kernel_config_opv(vec![seg_class_id(&f)]), make);
        SegWorld::register(net, f).await
    }

    /// **A claim committed by a streaming producer**: every position of `prompt` traced once and dropped (`trace_streaming_v1`), its
    /// committed values hashed as they pass, `lie` bumping element 0 of one value; only the positions in `keep` are kept (what the
    /// producer serves). The claim delivers one id, the greedy decode of the last position's logits, so it is `prompt.len()` positions.
    async fn held_claim(&mut self, producer: usize, job: Digest, prompt: &[u32], lie: (u32, u16, u16), keep: &[u32]) -> SegClaim {
        let post = self.f.program.occurrences().len() - 1;
        let logits = self.f.program.logits as usize;
        let last = prompt.len() as u32 - 1;
        let mut commitments: Vec<Vec<Vec<Digest>>> = Vec::with_capacity(prompt.len());
        let mut values: Vec<Vec<Vec<Tensor>>> = vec![Vec::new(); prompt.len()];
        let mut g0 = 0u32;
        let started = std::time::Instant::now();
        misaka_palw_kernel::trace::trace_streaming_v1(&self.f.program, &self.f.params, prompt, &mut |p, honest| {
            let mut committed = honest.to_vec();
            if p == lie.0 {
                bump(&mut committed[lie.1 as usize][lie.2 as usize], 0);
            }
            commitments
                .push(committed.iter().map(|o| o.iter().map(misaka_palw_kernel::merkle3::tensor_commitment_v3).collect()).collect());
            if p == last {
                g0 = DecodeRuleV1::Greedy.select(&committed[post][logits]).expect("logits");
            }
            if keep.contains(&p) {
                values[p as usize] = committed;
            }
            Ok(())
        })
        .expect("the streamed trace");
        let c = SegmentedCommitmentsV1::new(commitments);
        eprintln!("[k2s] held: {} positions traced and committed by a streaming producer in {:?}", prompt.len(), started.elapsed());
        let ledger = self.net.ledger();
        let class = &ledger.classes[&self.class];
        let evidence = build_segmented_evidence_v1(
            class.header(self.class),
            &class.descriptor,
            prompt.len() as u32,
            &prompt_root_of_ids_v1(prompt),
            &[],
            &c,
        );
        let kid = self.net.kid(producer);
        let claim = KernelClaimV1 { job_id: job, producer_bond: kid, generated: vec![g0], evidence_root: evidence.root() };
        let id = claim.id();
        self.seal_and_commit(producer, claim, evidence, c.segment_roots()).await;
        assert!(self.net.ledger().claims.contains_key(&id), "the held claim committed over its seal");
        SegClaim { id, values, c, tokens: prompt.to_vec() }
    }
}

/// **GAP-31: a history-bearing class held at 8,192 positions on the node.** The class is the sketch's dense-MoE layer (rotary
/// attention over `Hist` k/v windows, a router and experts) under K2-TIR-v4 at 8,192 positions, with a sliding attention window of
/// 1,024 rows (so the window slides for 7,168 positions, as the 9B's 8,192-row window slides past 8k). A streaming producer commits
/// an 8,192-position claim (eight segments) holding one position at a time. It lies in a CONTINUITY value: the k window at position
/// 4,096, the first position of segment 4, whose previous window is at 4,095 in segment 3 — the boundary v1's entry/exit roots used
/// to state, which v4 replaces by wiring.
///
/// The coalition publishes nothing. The outsider demands positions 4,095 and 4,096, the producer serves them on chain (in parts),
/// the outsider reads them back from the blocks, finds the lie at the window's element court (the new row and the previous window,
/// across the segment boundary), files it within the class's priced bound, and the claim is convicted. A replaying node agrees.
#[tokio::test]
async fn g14_k2s_a_history_bearing_class_held_at_8192_positions_convicts_a_continuity_lie_across_segments() {
    kaspa_core::log::try_init_logger("warn");
    let mut w = SegWorld::with_fixture(held_fixture(1024), TestConsensus::new).await;
    let outsider = w.net.chain.bonds.len() - 1;
    let prompt: Vec<u32> = (0..SEG_MAX_POSITIONS).map(|i| (i * 11 + 5) % w.f.program.token_bound).collect();
    let job = w.tiled_job_generating(&prompt, 1).await;
    let window = hist_append_at(&w.f.program, 1);
    let lie = w.held_claim(0, job, &prompt, (4096, 1, window), &[4095, 4096]).await;
    let ledger = w.net.ledger();
    let misaka_palw_kernel::ledger::ClaimBodyV1::Segmented { segment_roots, evidence, .. } = &ledger.claims[&lie.id].body else {
        panic!("a segmented claim")
    };
    assert_eq!((segment_roots.len(), evidence.positions), (8, SEG_MAX_POSITIONS), "eight segments, 8,192 positions");
    let view = w.fresh_record(&lie.id);
    // The verifier's own copy of the artifact (owned: the world is driven while it checks).
    let params = w.f.params.clone();
    let art = move |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).cloned();
    let nothing = SegDa { claim: &lie, withhold: 0..SEG_MAX_POSITIONS, read: Cell::new(0) };
    let SegFindingV1::Demand(missing) = check_positions_v1(&view.context(), &nothing, &art, &lie.tokens, &[4096]) else {
        panic!("a demand")
    };
    assert_eq!(missing, vec![4095, 4096], "the window's previous position, in the previous segment");
    let chain = w.demand_served_and_read(outsider, &lie, &missing).await;
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &lie.tokens, &[4096]) else {
        panic!("the continuity lie is found")
    };
    let e = &fault;
    assert_eq!(e.at(), Some((4096, 1, window)), "localized to the window");
    let bytes = fault.to_bytes();
    let bounds = w.net.ledger().classes[&w.class].bounds;
    let parts = position_parts_v1(&w.f.program, 4096).unwrap().len();
    eprintln!(
        "[k2s] held: the continuity lie is filed in {} B (class bound {}), after reading {} B back from the blocks ({} parts a position); \
         per-prosecution public bound {}",
        bytes.len(),
        bounds.max_filing_bytes,
        chain.read.get(),
        parts,
        bounds.max_public_bytes
    );
    assert!(bytes.len() as u64 <= bounds.max_filing_bytes);
    assert!(chain.read.get() <= bounds.max_public_bytes);
    w.file(outsider, &lie.id, &fault).await;
    assert!(w.net.ledger().claims[&lie.id].convicted, "the continuity lie is convicted");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the held claim's conviction");
}

// ---- GAP-40: K2-TIR-v5 (encoders and heads) on the node -------------------------------------------------------------------------

/// **A tiny real encoder as a K2-TIR-v5 class**: the lowering's HF fixture `fixture` (a BERT encoder, or an XLM-R reranker head) with
/// its weights, calibrated on templated sequences, lowered by `lower::bidir`, its ids and count lifted into the version-2 program's
/// inputs and taken as the version-1 view (the v5 binding: the last two params are the job's). Returns the class fixture and the
/// template ids `[CLS]` and `[SEP]`.
fn v5_fixture(fixture: &str, lmax: u32) -> (SegFixture, u32, u32) {
    use misaka_palw_tir::ParamSource;
    use misaka_palw_tir_lower::float_ref::{ParamStore, stream::Resident};
    use misaka_palw_tir_lower::lower::bidir::{self, BidirCfg, Padded, Pooling, float_forward};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-tir-lower/tests/fixtures").join(fixture);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("config.json")).unwrap()).unwrap();
    let spec = misaka_palw_tir_lower::hf_schema::read_model(&json, None, &misaka_palw_tir_lower::hf_schema::ReadOptions::default())
        .unwrap_or_else(|f| panic!("{fixture}: {}", f.error))
        .spec;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).expect("the HL program");
    let bind = misaka_palw_tir_lower::hf_weights::bind(&spec, &hl).expect("the binding");
    let ck = misaka_palw_tir_lower::weights::Checkpoint::open(&dir).expect("the checkpoint");
    let (params_f, _) = ParamStore::from_source(&hl, &bind, &ck).expect("the float params");
    let out: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("outputs.json")).unwrap()).unwrap();
    let first = &out["sequences"][0];
    let padded: Vec<usize> = first["padded"].as_array().unwrap().iter().map(|t| t.as_u64().unwrap() as usize).collect();
    let count = first["count"].as_u64().unwrap() as usize;
    let (cls, sep, pad) = (padded[0], padded[count - 1], *padded.last().unwrap());
    let cfg = BidirCfg { lmax, pooling: Pooling::Cls, normalize: false };
    let mut stats = std::collections::BTreeMap::new();
    for (i, body) in
        misaka_palw_tir_lower::fidelity::random_sequences(spec.vocab_size, 8, lmax as usize - 2, 11).into_iter().enumerate()
    {
        let n = 1 + (i % (lmax as usize - 2));
        let mut ids: Vec<usize> = std::iter::once(cls).chain(body.into_iter().take(n)).chain(std::iter::once(sep)).collect();
        let count = ids.len().min(lmax as usize);
        ids.truncate(lmax as usize);
        ids.resize(lmax as usize, pad);
        float_forward(&hl, &spec, &cfg, &params_f, &Padded { ids, count }, Some(&mut stats)).expect("calibration");
    }
    let lw = bidir::lower_bidir(&hl, &spec, &cfg).expect("lower");
    let quiet = |_: usize, _: usize| {};
    let mat = misaka_palw_tir_lower::lower::materialise(
        &lw,
        &hl,
        &Resident(std::sync::Arc::new(params_f)),
        &stats,
        &misaka_palw_tir_lower::quant::QuantPolicy::default(),
        &quiet,
    )
    .expect("materialise");
    let p2 = misaka_palw_tir_lower::encoder::bidir_v2(&lw, spec.vocab_size as u32, lmax).expect("the version-2 program");
    let ints = misaka_palw_tir_lower::encoder::lifted_params(&lw.program, &[bidir::IDS_PARAM, bidir::COUNT_PARAM], &mat.params);
    let program = p2.v1_view();
    let params = MapParams { tensors: ints.tensors.keys().map(|k| (*k, ints.param(k.0, k.1).expect("a param"))).collect() };
    misaka_palw_kernel::seg_encoder::encoder_binding_v1(&program).expect("a v5 encoder program");
    let d = misaka_palw_kernel::descriptor::k2_tir_v5_descriptor();
    let plan = plan_for_tir_program_v1(&d, &program, program_root_v1(&program.encode()), 1).expect("a v5 plan");
    let pc = ParamCommitmentsV1::of_v3(&params);
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(pc.root()), 0);
    (SegFixture { program, params, plan, pc }, cls as u32, sep as u32)
}

impl SegWorld {
    /// **A K2-TIR-v5 claim** of the encoder job `job` over `prompt` (the job's ids): ONE position whose ids and count are the job's, no
    /// delivered id; `lie` bumps element 0 of one value.
    async fn encoder_claim(&mut self, producer: usize, job: Digest, prompt: &[u32], lie: Option<(u16, u16)>) -> SegClaim {
        let binding = misaka_palw_kernel::seg_encoder::encoder_binding_v1(&self.f.program).unwrap();
        let trace = misaka_palw_kernel::seg_encoder::trace_encoder_v1(&self.f.program, &self.f.params, &binding, prompt).unwrap();
        let mut values = trace.values;
        if let Some((s, n)) = lie {
            bump(&mut values[0][s as usize][n as usize], 0);
        }
        let c = seg_commitments_of_trace_v1(&TraceV1 { values: values.clone(), inputs: Vec::new() });
        let ledger = self.net.ledger();
        let class = &ledger.classes[&self.class];
        let evidence = build_segmented_evidence_v1(
            class.header(self.class),
            &class.descriptor,
            prompt.len() as u32,
            &prompt_root_of_ids_v1(prompt),
            &[],
            &c,
        );
        let kid = self.net.kid(producer);
        let claim = KernelClaimV1 { job_id: job, producer_bond: kid, generated: Vec::new(), evidence_root: evidence.root() };
        let id = claim.id();
        self.seal_and_commit(producer, claim, evidence, c.segment_roots()).await;
        assert!(self.net.ledger().claims.contains_key(&id), "the encoder claim committed over its seal");
        SegClaim { id, values, c, tokens: prompt.to_vec() }
    }
}

/// The first node of occurrence 0 that reads the job's ids (the embedding lookup: its court opens the job's prompt tile).
fn ids_lookup(program: &TirProgramV1) -> u16 {
    let ids = misaka_palw_kernel::seg_encoder::encoder_binding_v1(program).unwrap().first_input;
    let (b, _) = program.occurrences()[0];
    program.blocks[b as usize]
        .nodes
        .iter()
        .position(|n| n.inputs.iter().any(|r| matches!(r, misaka_palw_tir::Ref::Param(j) if *j == ids)))
        .expect("a node reads the ids") as u16
}

/// **GAP-40: a K2-TIR-v5 encoder class on the real node.** A real tiny BERT encoder (the lowering's HF fixture, with its weights) is
/// registered under OPV through the real path as ONE position whose ids are the job's. Its jobs are tiled jobs that generate nothing
/// (one tile: `[CLS] ‖ text ‖ [SEP]`). The coalition (every bond but the outsider) publishes nothing:
/// * a lie in the embedding lookup is demanded (position 0), served on chain, read back from the blocks and convicted: the court opens
///   the job's prompt tile against its root;
/// * a lie in the result (the pooled output) is convicted the same way;
/// * an honest claim is never convicted (an honest element filed against it is dismissed) and is Final at the window's end with no Panel.
///
/// A replaying node agrees with every root.
#[tokio::test]
async fn g14_k2s_v5_an_encoder_class_on_the_node_convicts_its_lies_and_finalizes_an_honest_claim() {
    kaspa_core::log::try_init_logger("warn");
    let (f, cls, sep) = v5_fixture("hf-enc/bert", 12);
    let mut w = SegWorld::with_fixture(f, TestConsensus::new).await;
    let outsider = w.net.chain.bonds.len() - 1;
    let vocab = w.f.program.token_bound;
    let prompt: Vec<u32> = vec![cls, 11 % vocab, 25 % vocab, 7 % vocab, sep];
    let lookup = ids_lookup(&w.f.program);
    let post = (w.f.program.occurrences().len() - 1) as u16;
    let result = w.f.program.logits;
    // The verifier's own copy of the artifact (owned: the world is driven while it checks).
    let params = w.f.params.clone();
    let art = move |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).cloned();
    for (what, lie) in [("the embedding lookup", (0u16, lookup)), ("the result", (post, result))] {
        let job = w.tiled_job_generating(&prompt, 0).await;
        let claim = w.encoder_claim(0, job, &prompt, Some(lie)).await;
        let view = w.fresh_record(&claim.id);
        assert!(view.encoder.is_some() && view.positions == 1, "{what}: a v5 claim of one position");
        let nothing = SegDa { claim: &claim, withhold: 0..1, read: Cell::new(0) };
        let SegFindingV1::Demand(missing) = check_positions_v1(&view.context(), &nothing, &art, &claim.tokens, &[0]) else {
            panic!("{what}: a demand")
        };
        assert_eq!(missing, vec![0]);
        let chain = w.demand_served_and_read(outsider, &claim, &missing).await;
        let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &chain, &art, &claim.tokens, &[0]) else {
            panic!("{what}: the lie is found")
        };
        let e = &fault;
        assert_eq!(e.at(), Some((0, lie.0, lie.1)), "{what}: localized");
        assert_eq!(e.token().is_some(), lie == (0, lookup), "{what}: the job's tile opens the ids");
        let bytes = fault.to_bytes();
        assert!(bytes.len() as u64 <= w.net.ledger().classes[&w.class].bounds.max_filing_bytes, "{what}: within the priced bound");
        w.file(outsider, &claim.id, &fault).await;
        assert!(w.net.ledger().claims[&claim.id].convicted, "{what}: convicted");
    }
    // The honest claim: an honest element filed is dismissed; Final at the window's end.
    let job = w.tiled_job_generating(&prompt, 0).await;
    let honest = w.encoder_claim(0, job, &prompt, None).await;
    let view = w.fresh_record(&honest.id);
    let published = SegDa { claim: &honest, withhold: 0..0, read: Cell::new(0) };
    assert_eq!(check_positions_v1(&view.context(), &published, &art, &honest.tokens, &[0]), SegFindingV1::Clean);
    let filing = prove_element_v1(&view.context(), &published, &art, &honest.tokens, (0, 0, lookup), 0).unwrap();
    let slashed = w.net.slashed(0);
    w.file(outsider, &honest.id, &SegFaultV1::Element(filing)).await;
    assert!(!w.net.ledger().claims[&honest.id].convicted && w.net.slashed(0) == slashed, "an honest element is dismissed");
    let daa = w.net.daa() + 80;
    w.net.beat_to(daa).await;
    assert!(matches!(w.net.claim_state(&honest.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&honest.id));
    let z = w.net.replay().await;
    w.net.assert_same(&z, "replay of the v5 class's convictions and Final");
}
