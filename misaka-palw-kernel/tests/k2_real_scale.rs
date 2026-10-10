//! **K2-TIR-v4 at the ledger level** (`docs/design/palw/k2-real-scale.md`): segmented claims over tiled prompts past 4,096 ids, a lie in
//! one segment localized to one element and convicted with bounded bytes, a withheld position defaulted, a position served in parts
//! with its demanders' bonds held until the grace, and the 9B-8k fixture's shape-level verdict under the per-prosecution bounds.
//!
//! Every block goes through `KernelLedgerV1::apply_block`; the outsider is built from the ledger's public view
//! (`seg_claim_view_v1`), the public artifact and whatever material is served — never the producer's objects.

mod common;

use common::ledger_world::policy;
use common::opv_world::opv_example;
use misaka_palw_kernel::descriptor::{
    KernelDescriptorV1, KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v4_descriptor,
};
use misaka_palw_kernel::element::{SegClaimContextV1, SegFaultV1, SegFindingV1, SegMaterialV1, check_positions_v1, prove_element_v1};
use misaka_palw_kernel::gate::{ProsecutionPolicyV1, public_prosecution_complete_v1, public_prosecution_complete_v4};
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::job::{DecodeRuleV1, KernelClaimV1, KernelJobV1};
use misaka_palw_kernel::ledger::{
    AuthV1, KernelLedgerV1, KernelRouteObjectV1 as O, LedgerBlockV1, LedgerEventV1 as E, LedgerTxV1, ProsecutionV1, carrier_fit_v1,
    claim_seal_v1, single_class_id_v1,
};
use misaka_palw_kernel::lifecycle::ClaimStateV1;
use misaka_palw_kernel::mode::VerificationModeV1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{ProfileMaterialV1, program_root_v1};
use misaka_palw_kernel::seg::{
    PROMPT_TILE_IDS_V1, PromptTileOpeningV1, SEG_LEN_V4, SegmentedCommitmentsV1, TiledJobV1, build_segmented_evidence_v1,
    prompt_root_of_ids_v1, prompt_tiles_v1, seg_commitments_of_trace_v1,
};
use misaka_palw_kernel::seg_da::{assemble_position_v1, position_part_v1, position_parts_v1};
use misaka_palw_kernel::seg_detect::{check_claim_by_reexecution_v1, first_decode_mismatch_v1};
use misaka_palw_kernel::trace::{ParamCommitmentsV1, trace_v1};
use misaka_palw_kernel::{VerificationPlanV1, check_plan_v1};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN, Ref, TirProgramV1};
use misaka_palw_tir::{DType, Dim, MapParams, Tensor, TensorType};
use misaka_palw_tir_sketch::fixture::wide128_v1;
use std::cell::{Cell, RefCell};

const PROD: Digest = [0xA1; 64];
const OUT: Digest = [0x0B; 64];
const ANY: Digest = [0x77; 64];
const OPV: VerificationModeV1 = VerificationModeV1::OptimisticPublicVerification;
/// The node's carrier (`PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1`: 16 chunks of 100,000 B less the wrapper).
const CARRIER: usize = 100_000 * 16 - (16 << 10);

fn obj(signer: Digest, object: O) -> LedgerTxV1 {
    LedgerTxV1::Object { auth: AuthV1 { signer_bond: signer }, object }
}

fn refusal(ev: &[E]) -> Option<String> {
    ev.iter().find_map(|e| match e {
        E::Refused { why, .. } => Some(why.clone()),
        _ => None,
    })
}

/// A ledger with the OPV policy and a K2-TIR-v4 class of `program` registered under OPV.
struct W {
    l: KernelLedgerV1,
    daa: u64,
    d: KernelDescriptorV1,
    program: TirProgramV1,
    params: MapParams,
    plan: VerificationPlanV1,
    pc: ParamCommitmentsV1,
    class: Digest,
}

impl W {
    fn new(program: TirProgramV1, params: MapParams, max_positions: u32) -> W {
        let d = k2_tir_v4_descriptor();
        let schedule = KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 });
        let l = KernelLedgerV1::genesis(policy(), schedule, vec![k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), d.clone()])
            .unwrap()
            .with_opv_policy(opv_example())
            .unwrap();
        let bytes = program.encode();
        let plan = plan_for_tir_program_v1(&d, &program, program_root_v1(&bytes), max_positions).unwrap();
        let pc = ParamCommitmentsV1::of_v3(&params);
        let class = single_class_id_v1(d.digest(), &bytes, &plan, &pc, OPV);
        let mut w = W { l, daa: 0, d, program, params, plan, pc, class };
        let mut txs = vec![
            LedgerTxV1::SyncBond { bond: PROD, collateral: 1_000_000 },
            LedgerTxV1::SyncBond { bond: OUT, collateral: 1_000_000 },
            LedgerTxV1::SyncBond { bond: ANY, collateral: 1_000_000 },
            LedgerTxV1::AttestArtifact { artifact_root: w.pc.root() },
        ];
        // A Panel-licensed registration of a segmented class is refused by name.
        txs.push(w.register(VerificationModeV1::PanelLicensed));
        let ev = w.block(txs);
        assert!(refusal(&ev).is_some_and(|r| r.contains("only under OptimisticPublicVerification")), "{ev:?}");
        let ev = w.block(vec![LedgerTxV1::AdmitOptimisticClass { class: w.class }, w.register(OPV)]);
        assert!(ev.iter().any(|e| matches!(e, E::ClassRegistered { class } if *class == w.class)), "{ev:?}");
        w
    }

    fn register(&self, mode: VerificationModeV1) -> LedgerTxV1 {
        let object = if mode == VerificationModeV1::PanelLicensed {
            O::RegisterClass {
                descriptor: self.d.digest(),
                program_bytes: self.program.encode(),
                plan: self.plan.clone(),
                param_commitments: self.pc.clone(),
            }
        } else {
            O::RegisterClassV2 {
                mode,
                descriptor: self.d.digest(),
                program_bytes: self.program.encode(),
                plan: self.plan.clone(),
                param_commitments: self.pc.clone(),
            }
        };
        obj(PROD, object)
    }

    fn block(&mut self, txs: Vec<LedgerTxV1>) -> Vec<E> {
        self.daa += 1;
        self.l.apply_block(&LedgerBlockV1 { daa: self.daa, txs })
    }

    fn beat_to(&mut self, daa: u64) -> Vec<E> {
        let mut all = Vec::new();
        while self.daa < daa {
            all.extend(self.block(Vec::new()));
        }
        all
    }

    /// Post a tiled job of `prompt` and every tile of it; returns the job id.
    fn tiled_job(&mut self, prompt: &[u32], max_new: u32) -> Digest {
        let job = TiledJobV1 {
            class_binding_id: self.class,
            prompt_len: prompt.len() as u32,
            prompt_root: prompt_root_of_ids_v1(prompt),
            max_new_tokens: max_new,
            decode: DecodeRuleV1::Greedy,
            nonce: [self.daa as u8; 64],
        };
        let id = job.id();
        let ev = self.block(vec![obj(ANY, O::PostTiledJob { job })]);
        assert!(ev.iter().any(|e| matches!(e, E::JobPosted { .. })), "{ev:?}");
        let tiles: Vec<LedgerTxV1> = (0..prompt_tiles_v1(prompt.len() as u32))
            .map(|i| obj(ANY, O::PostPromptTile { job: id, tile: PromptTileOpeningV1::of(prompt, i).unwrap() }))
            .collect();
        let ev = self.block(tiles);
        assert!(refusal(&ev).is_none(), "{ev:?}");
        id
    }
}

/// A producer's claim: its values (honest, or with a lie), commitments, evidence and the claim object.
struct Produced {
    values: Vec<Vec<Vec<Tensor>>>,
    c: SegmentedCommitmentsV1,
    claim: KernelClaimV1,
    evidence: misaka_palw_kernel::seg::SegmentedEvidenceV2,
    tokens: Vec<u32>,
}

impl SegMaterialV1 for Produced {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        self.values.get(p as usize).cloned()
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        (p < self.c.positions()).then(|| self.c.position_path(p).1)
    }
}

fn produce(w: &W, job: Digest, prompt: &[u32], lie: Option<(u32, u16, u16)>) -> Produced {
    let post = (w.program.occurrences().len() - 1) as usize;
    let first = trace_v1(&w.program, &w.params, prompt).unwrap();
    let g0 = DecodeRuleV1::Greedy.select(&first.values[prompt.len() - 1][post][w.program.logits as usize]).unwrap();
    let mut tokens = prompt.to_vec();
    tokens.push(g0);
    let trace = trace_v1(&w.program, &w.params, &tokens).unwrap();
    let g1 = DecodeRuleV1::Greedy.select(&trace.values[tokens.len() - 1][post][w.program.logits as usize]).unwrap();
    let mut values = trace.values.clone();
    if let Some((p, s, n)) = lie {
        let t = &mut values[p as usize][s as usize][n as usize];
        let v = t.data[0];
        t.data[0] = if t.dtype.contains(v + 1) { v + 1 } else { v - 1 };
    }
    let c = seg_commitments_of_trace_v1(&misaka_palw_kernel::trace::TraceV1 { values: values.clone(), inputs: Vec::new() });
    let class = &w.l.classes[&w.class];
    let evidence =
        build_segmented_evidence_v1(class.header(w.class), &w.d, prompt.len() as u32, &prompt_root_of_ids_v1(prompt), &[g0], &c);
    let claim = KernelClaimV1 { job_id: job, producer_bond: PROD, generated: vec![g0, g1], evidence_root: evidence.root() };
    Produced { values, c, claim, evidence, tokens }
}

/// Seal, then reveal: the claim's events. Past `palw_panel_free_v1` (the OPV policy's activation) the seal is claim seal v2 and the
/// reveal carries its salt (`CommitClaimSalted { Segmented }`).
fn commit(w: &mut W, p: &Produced) -> Vec<E> {
    let id = p.claim.id();
    let salted = w.l.salted_seals_from().is_some_and(|at| w.l.daa.saturating_add(1) >= at);
    let salt = misaka_palw_kernel::hash::id(b"misaka-palw/test/claim-salt", &id);
    let seal = if salted { misaka_palw_kernel::ledger::claim_seal_v2(&id, &salt) } else { claim_seal_v1(&id) };
    let ev = w.block(vec![obj(PROD, O::SealClaim { producer: PROD, job: p.claim.job_id, seal })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let (claim, evidence, segment_roots) = (p.claim.clone(), p.evidence.clone(), p.c.segment_roots());
    let reveal = if salted {
        O::CommitClaimSalted { salt, commit: misaka_palw_kernel::ledger::SaltedCommitV1::Segmented { claim, evidence, segment_roots } }
    } else {
        O::CommitSegmentedClaim { claim, evidence, segment_roots }
    };
    w.block(vec![obj(PROD, reveal)])
}

fn artifact(w: &W) -> impl Fn(u16, Option<u16>) -> Option<Tensor> + '_ {
    |j, l| w.params.tensors.get(&(j, l)).cloned()
}

#[test]
fn f_b1_a_segmented_prefix_cannot_complete_a_fixed_reward_job() {
    for required in [1, 2, 3] {
        let fx = wide128_v1(7);
        let mut w = W::new(fx.program, fx.params, 32);
        let prompt = [1, 3, 5];
        let job = w.tiled_job(&prompt, required);
        let produced = produce(&w, job, &prompt, None); // Two correct tokens and an otherwise authentic trace.
        let id = produced.claim.id();
        let ev = commit(&mut w, &produced);
        if required == 2 {
            assert!(ev.contains(&E::ClaimCommitted { claim: id }), "{ev:?}");
        } else {
            assert!(refusal(&ev).is_some_and(|why| why.contains("WrongGenerationLength")), "{ev:?}");
            assert!(!w.l.claims.contains_key(&id));
        }
    }
}

#[test]
fn k2s_the_gate_rederives_the_plan_and_prices_decoded_working_memory_and_all_progress() {
    use misaka_palw_kernel::gate::ProsecutionGapV1 as G;
    let fx = wide128_v1(7);
    let d = k2_tir_v4_descriptor();
    let root = program_root_v1(&fx.program.encode());
    let p = plan_for_tir_program_v1(&d, &fx.program, root, 32).unwrap();
    let material = ProfileMaterialV1::kernel_route(true);
    let (bounds, seg) = public_prosecution_complete_v4(&d, &p, &fx.program, &material, &ROUTE).unwrap();
    let mut forged = p.clone();
    forged.budgets.worst_court_bytes = 0;
    assert_eq!(public_prosecution_complete_v4(&d, &forged, &fx.program, &material, &ROUTE), Err(vec![G::WrongProgram]));
    let mut invalid = fx.program.clone();
    invalid.schedule.pre = u8::MAX;
    assert_eq!(public_prosecution_complete_v4(&d, &p, &invalid, &material, &ROUTE), Err(vec![G::WrongProgram]));
    let old_wire_ram = p.budgets.artifact_bytes + 2 * seg.position_material_bytes;
    assert!(bounds.max_verifier_ram > old_wire_ram);
    let low_ram = ProsecutionPolicyV1 { max_verifier_ram: old_wire_ram, ..ROUTE };
    assert!(
        public_prosecution_complete_v4(&d, &p, &fx.program, &material, &low_ram)
            .unwrap_err()
            .iter()
            .any(|g| matches!(g, G::Unbounded { what: "verifier RAM", .. }))
    );
    let low_state = ProsecutionPolicyV1 { max_retained_state: bounds.max_retained_state - 1, ..ROUTE };
    assert!(
        public_prosecution_complete_v4(&d, &p, &fx.program, &material, &low_state)
            .unwrap_err()
            .iter()
            .any(|g| matches!(g, G::Unbounded { what: "retained state (on chain, per claim)", .. }))
    );
    assert_eq!(bounds.max_localization_rounds, 12);
    assert!(bounds.max_retained_state > bounds.max_commit_bytes + 32 * 64 * 40);
}

#[test]
fn k2s_full_shared_demand_and_join_limits_do_not_block_an_outside_proof_or_default() {
    use misaka_palw_kernel::gate::MAX_DEMANDERS_PER_SESSION_V1;
    for convict in [true, false] {
        let fx = wide128_v1(7);
        let mut w = W::new(fx.program, fx.params, 32);
        let prompt = [1, 2, 3, 4, 5];
        let job = w.tiled_job(&prompt, 2);
        let (s, n) = w
            .program
            .occurrences()
            .iter()
            .enumerate()
            .find_map(|(s, (b, _))| {
                w.program.blocks[*b as usize]
                    .nodes
                    .iter()
                    .position(|n| matches!(n.prim, misaka_palw_tir::Prim::MatMul))
                    .map(|n| (s as u16, n as u16))
            })
            .unwrap();
        let p = produce(&w, job, &prompt, convict.then_some((4, s, n)));
        commit(&mut w, &p);
        let id = p.claim.id();
        let demand = |bond, position| obj(bond, O::FileDemand { demander: bond, claim: id, stage: 0, position });
        let ev = w.block((0..4).map(|q| demand(OUT, q)).chain([demand(ANY, 4)]).collect());
        assert!(refusal(&ev).is_none(), "{ev:?}");
        let before = w.l.bonds[&OUT].reserved;
        let ev = w.block(vec![demand(OUT, 4)]);
        assert!(refusal(&ev).is_some_and(|r| r.contains("open sessions")), "joining bypasses the four-session limit: {ev:?}");
        assert_eq!(w.l.bonds[&OUT].reserved, before);
        let ev = w.block(vec![demand(OUT, 0)]);
        assert!(refusal(&ev).is_none(), "a repeated join is idempotent: {ev:?}");
        let spammers: Vec<_> = (0..MAX_DEMANDERS_PER_SESSION_V1 as u64)
            .map(|i| misaka_palw_kernel::hash::id(b"g14/seg-participants", &i.to_le_bytes()))
            .collect();
        let ev = w.block(
            spammers
                .iter()
                .map(|b| LedgerTxV1::SyncBond { bond: *b, collateral: 1_000_000 })
                .chain(spammers[..MAX_DEMANDERS_PER_SESSION_V1 - 1].iter().map(|b| demand(*b, 4)))
                .collect(),
        );
        assert!(refusal(&ev).is_none(), "{ev:?}");
        assert_eq!(w.l.demands[&(id, 0, 4)].demanders.len(), MAX_DEMANDERS_PER_SESSION_V1);
        let outside = *spammers.last().unwrap();
        let ev = w.block(vec![demand(outside, 4)]);
        assert!(refusal(&ev).is_some_and(|r| r.contains("maximum collateral participants")), "{ev:?}");
        assert_eq!(w.l.bonds[&outside].reserved, 0);
        let state_bytes = borsh::to_vec(&w.l.claims[&id]).unwrap().len()
            + w.l.demands.iter().filter(|((c, _, _), _)| *c == id).map(|(k, v)| borsh::to_vec(&(k, v)).unwrap().len()).sum::<usize>()
            + w.l
                .seg_progress
                .iter()
                .filter(|((c, _, _), _)| *c == id)
                .map(|(k, v)| borsh::to_vec(&(k, v)).unwrap().len())
                .sum::<usize>();
        assert!(state_bytes as u128 <= w.l.classes[&w.class].bounds.max_retained_state);
        if convict {
            let view = w.l.seg_claim_view_v1(&id).unwrap();
            let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &p, &artifact(&w), &p.tokens, &[4]) else {
                panic!("the public lie was not found")
            };
            let ev = w.block(vec![obj(
                outside,
                O::FileProof { accuser: outside, claim: id, proof: ProsecutionV1::Segmented(fault.to_bytes()) },
            )]);
            assert!(
                ev.iter().any(|e| matches!(e, E::Convicted { claim, accuser, .. } if *claim == id && *accuser == outside)),
                "{ev:?}"
            );
        } else {
            let ev = w.beat_to(w.daa + 25);
            assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { claim, .. } if *claim == id)), "{ev:?}");
            assert!(!w.l.claims[&id].convicted);
        }
        assert!(spammers.iter().all(|b| w.l.bonds[b].reserved == 0));
    }
}

#[test]
fn k2s_raw_tensor_aliases_of_public_bytes_cannot_authenticate() {
    let fx = wide128_v1(7);
    let mut w = W::new(fx.program, fx.params, 32);
    let prompt = [1, 2, 3];
    let job = w.tiled_job(&prompt, 2);
    let mut p = produce(&w, job, &prompt, None);
    commit(&mut w, &p);
    let view = w.l.seg_claim_view_v1(&p.claim.id()).unwrap();
    let j = w.program.params.iter().position(|p| p.dtype == DType::I8).unwrap() as u16;
    let bad_artifact = |index, layer| {
        let mut t = artifact(&w)(index, layer)?;
        if index == j {
            t.data[0] += 256;
        }
        Some(t)
    };
    assert!(matches!(check_positions_v1(&view.context(), &p, &bad_artifact, &p.tokens, &[0]), SegFindingV1::Inconsistent(_)));
    let t = p.values[0].iter_mut().flatten().find(|t| t.dtype == DType::I8).unwrap();
    let authentic = misaka_palw_kernel::merkle3::tensor_commitment_v3(t);
    t.data[0] += 256;
    assert_eq!(authentic, misaka_palw_kernel::merkle3::tensor_commitment_v3(t));
    assert_eq!(check_positions_v1(&view.context(), &p, &artifact(&w), &p.tokens, &[0]), SegFindingV1::Demand(vec![0]));
}

#[test]
fn k2s_a_tiled_prompt_past_4096_ids_commits_as_a_multi_segment_claim_and_an_honest_one_finalizes() {
    let fx = wide128_v1(7);
    let mut w = W::new(fx.program, fx.params, 8192);
    let prompt: Vec<u32> = (0..4500u32).map(|i| (i * 13 + 5) % 32).collect();
    assert!(prompt.len() > 4096 && prompt_tiles_v1(prompt.len() as u32) == 2);
    // A claim on a job whose tiles are not all posted is refused: the input is public before any claim.
    let job = TiledJobV1 {
        class_binding_id: w.class,
        prompt_len: prompt.len() as u32,
        prompt_root: prompt_root_of_ids_v1(&prompt),
        max_new_tokens: 2,
        decode: DecodeRuleV1::Greedy,
        nonce: [1; 64],
    };
    let ev = w.block(vec![obj(ANY, O::PostTiledJob { job: job.clone() })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let ev = w.block(vec![obj(ANY, O::PostPromptTile { job: job.id(), tile: PromptTileOpeningV1::of(&prompt, 0).unwrap() })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let early = produce(&w, job.id(), &prompt, None);
    let ev = commit(&mut w, &early);
    assert!(refusal(&ev).is_some_and(|r| r.contains("not fully posted")), "{ev:?}");
    // A forged tile and a second copy of a posted one are refused.
    let mut forged = PromptTileOpeningV1::of(&prompt, 1).unwrap();
    forged.ids[0] = (forged.ids[0] + 1) % 32;
    let ev = w.block(vec![obj(ANY, O::PostPromptTile { job: job.id(), tile: forged })]);
    assert!(refusal(&ev).is_some_and(|r| r.contains("not the job's")), "{ev:?}");
    let ev = w.block(vec![obj(ANY, O::PostPromptTile { job: job.id(), tile: PromptTileOpeningV1::of(&prompt, 0).unwrap() })]);
    assert!(refusal(&ev).is_some_and(|r| r.contains("already posted")), "{ev:?}");
    let ev = w.block(vec![obj(ANY, O::PostPromptTile { job: job.id(), tile: PromptTileOpeningV1::of(&prompt, 1).unwrap() })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    // The honest claim: 4,501 positions in 5 segments, the claim row a few KB however long the context.
    let honest = produce(&w, job.id(), &prompt, None);
    assert_eq!(honest.c.segment_roots().len(), 5);
    let ev = commit(&mut w, &honest);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
    let id = honest.claim.id();
    let row_bytes = borsh::to_vec(&w.l.claims[&id]).unwrap().len();
    assert!(row_bytes < 4096 + 5 * 64, "the claim row is {row_bytes} B");
    // A fresh outsider checks one position of every segment (and the positions they read) from public material: clean.
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let ctx = view.context();
    let positions: Vec<u32> = (0..5).map(|s| s * SEG_LEN_V4 + 7).chain([4499, 4500]).collect();
    assert_eq!(check_positions_v1(&ctx, &honest, &artifact(&w), &honest.tokens, &positions), SegFindingV1::Clean);
    // An honest element filed anyway is dismissed (the filer pays the fee), and the claim reaches Final.
    let filing = prove_element_v1(&ctx, &honest, &artifact(&w), &honest.tokens, (4100, 1, 2), 0).unwrap();
    let bytes = misaka_palw_kernel::element::SegFaultV1::Element(filing).to_bytes();
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(bytes) })]);
    assert!(ev.iter().any(|e| matches!(e, E::ProofDismissed { .. })), "{ev:?}");
    let ev = w.beat_to(w.daa + 80);
    assert!(ev.iter().any(|e| matches!(e, E::Final { claim, .. } if *claim == id)), "{ev:?}");
    // An inline job answered by a segmented claim binds the same prompt root.
    let inline = KernelJobV1 {
        class_binding_id: w.class,
        prompt: prompt[..1500].to_vec(),
        max_new_tokens: 2,
        decode: DecodeRuleV1::Greedy,
        nonce: [3; 64],
    };
    let ev = w.block(vec![obj(ANY, O::PostJob { job: inline.clone() })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let p = produce(&w, inline.id(), &inline.prompt, None);
    let ev = commit(&mut w, &p);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
}

#[test]
fn k2s_a_lie_in_one_segment_is_localized_to_one_element_and_convicted_with_bounded_bytes() {
    let fx = wide128_v1(7);
    let mut w = W::new(fx.program, fx.params, 8192);
    let prompt: Vec<u32> = (0..1500u32).map(|i| (i * 7 + 1) % 32).collect();
    let job = w.tiled_job(&prompt, 2);
    // The producer lies in the wide MatMul's output at position 1,200 (segment 1); everything else is honest.
    let occ = w.program.occurrences();
    let (s, n) = occ
        .iter()
        .enumerate()
        .find_map(|(s, (b, _))| {
            w.program.blocks[*b as usize]
                .nodes
                .iter()
                .position(|nd| matches!(nd.prim, misaka_palw_tir::Prim::MatMul))
                .map(|n| (s as u16, n as u16))
        })
        .unwrap();
    let liar = produce(&w, job, &prompt, Some((1200, s, n)));
    let ev = commit(&mut w, &liar);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
    let id = liar.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let ctx = view.context();
    // Segment 0 is clean; the outsider's check of position 1,200 reads positions 1,199 and 1,200 only.
    assert_eq!(check_positions_v1(&ctx, &liar, &artifact(&w), &liar.tokens, &[5, 600, 1023]), SegFindingV1::Clean);
    let SegFindingV1::Fault(fault) = check_positions_v1(&ctx, &liar, &artifact(&w), &liar.tokens, &[1200]) else {
        panic!("the lie is not found")
    };
    let bytes = fault.to_bytes();
    let bounds = w.l.classes[&w.class].bounds;
    assert!(bytes.len() as u64 <= bounds.max_filing_bytes && bytes.len() < CARRIER, "{} B filing", bytes.len());
    let (_, seg) =
        public_prosecution_complete_v4(&w.d, &w.plan, &w.program, &ProfileMaterialV1::kernel_route(true), &policy().prosecution)
            .unwrap();
    eprintln!(
        "[k2s] wide128 @8192: filing {} B (bound {}), public bytes per prosecution {}, position material {} B, parts {}",
        bytes.len(),
        bounds.max_filing_bytes,
        bounds.max_public_bytes,
        seg.position_material_bytes,
        seg.parts_per_position
    );
    let before = w.l.bonds[&PROD].collateral;
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(bytes.clone()) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, accuser, .. } if *claim == id && *accuser == OUT)), "{ev:?}");
    assert!(w.l.bonds[&PROD].collateral < before, "the producer's reservation is slashed");
    assert!(matches!(w.l.claims[&id].life.state, ClaimStateV1::Convicted { .. }));
    // The same filing again is a duplicate: one conviction, one slash.
    let ev = w.block(vec![obj(ANY, O::FileProof { accuser: ANY, claim: id, proof: ProsecutionV1::Segmented(bytes) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Duplicate { .. })), "{ev:?}");
}

#[test]
fn k2s_a_withheld_position_is_demanded_then_defaults_never_a_conviction() {
    let fx = wide128_v1(7);
    let mut w = W::new(fx.program, fx.params, 8192);
    let prompt: Vec<u32> = (0..1100u32).map(|i| (i * 3 + 2) % 32).collect();
    let job = w.tiled_job(&prompt, 2);
    let honest = produce(&w, job, &prompt, None);
    commit(&mut w, &honest);
    let id = honest.claim.id();
    // The outsider demands position 1,050 (segment 1); a response that is not the committed material is rejected; the producer
    // never serves; at the deadline the claim defaults (availability), never a conviction.
    let ev = w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: id, stage: 0, position: 1050 })]);
    assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { position: 1050, .. })), "{ev:?}");
    // (position 1,049's values posing as 1,050's: their position root is not the committed one)
    let wrong = position_part_v1(&w.program, 1050, &honest.values[1049], honest.c.position_path(1050).1, 0).unwrap();
    let ev = w.block(vec![obj(PROD, O::Respond { claim: id, stage: 0, position: 1050, bytes: borsh::to_vec(&wrong).unwrap() })]);
    assert!(ev.iter().any(|e| matches!(e, E::ResponseRejected { .. })), "{ev:?}");
    // A demander holds at most four open sessions on a claim; a fifth is refused, another bond is not affected.
    let ev = w.block((0..4).map(|i| obj(OUT, O::FileDemand { demander: OUT, claim: id, stage: 0, position: 10 + i })).collect());
    assert!(refusal(&ev).is_some_and(|r| r.contains("open sessions")), "{ev:?}");
    let ev = w.block(vec![obj(ANY, O::FileDemand { demander: ANY, claim: id, stage: 0, position: 20 })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let ev = w.beat_to(w.daa + 25);
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { claim, .. } if *claim == id)), "{ev:?}");
    assert!(!w.l.claims[&id].convicted, "a default is never a conviction");
    assert!(matches!(w.l.claims[&id].life.state, ClaimStateV1::Unavailable { .. }), "{:?}", w.l.claims[&id].life.state);
    assert!(w.l.seg_progress.keys().all(|(c, _, _)| *c != id), "the progress of the defaulted demands is gone");
    assert_eq!(w.l.bonds[&OUT].reserved, 0, "every demand bond returned");
}

/// What a verifier reads from a producer: values (bytes counted, positions listed) and position paths (probes counted).
struct Counted<'a> {
    inner: &'a Produced,
    bytes: Cell<u128>,
    positions: RefCell<Vec<u32>>,
    probes: Cell<u32>,
}

fn counted(inner: &Produced) -> Counted<'_> {
    Counted { inner, bytes: Cell::new(0), positions: RefCell::new(Vec::new()), probes: Cell::new(0) }
}

#[test]
fn k2s_fresh_streaming_replay_uses_only_registered_model_and_authenticated_public_material() {
    use misaka_palw_kernel::ledger::{OutsiderFindingV1, OutsiderV1, PublicSourceV1};
    struct NoPrivate;
    impl PublicSourceV1 for NoPrivate {
        fn node(&self, _: u8, _: u32, _: u16, _: u16) -> Option<Tensor> {
            panic!("private producer values")
        }
    }
    impl SegMaterialV1 for NoPrivate {
        fn position(&self, _: u32) -> Option<Vec<Vec<Tensor>>> {
            panic!("honest replay needs no position values")
        }
        fn position_siblings(&self, _: u32) -> Option<Vec<Digest>> {
            panic!("honest replay needs no producer paths")
        }
    }
    for lie in [false, true] {
        let fx = wide128_v1(7);
        let acquired_model = fx.params.clone();
        let mut w = W::new(fx.program, fx.params, 64);
        let prompt: Vec<u32> = (0..63).map(|p| (p * 7 + 1) % 32).collect();
        let job = w.tiled_job(&prompt, 2);
        let (s, n) = w
            .program
            .occurrences()
            .iter()
            .enumerate()
            .find_map(|(s, (b, _))| {
                w.program.blocks[*b as usize]
                    .nodes
                    .iter()
                    .position(|n| matches!(n.prim, misaka_palw_tir::Prim::MatMul))
                    .map(|n| (s as u16, n as u16))
            })
            .unwrap();
        let public_stream = produce(&w, job, &prompt, lie.then_some((61, s, n)));
        commit(&mut w, &public_stream);
        let id = public_stream.claim.id();
        let fresh = KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &w.l.to_rows()).unwrap();
        let verifier = OutsiderV1 { ledger: &fresh, claim: id, material: &NoPrivate, artifact: &acquired_model, salt: [0; 64] };
        let view = fresh.seg_claim_view_v1(&id).unwrap();
        let bad_artifact = |index, layer| {
            let mut t = acquired_model.tensors.get(&(index, layer))?.clone();
            if t.dtype == DType::I8 {
                t.data[0] += 256; // same wire hash, outside its canonical range
            }
            Some(t)
        };
        assert!(
            misaka_palw_kernel::seg_detect::prepare_reexecution_v1(&view.context(), &bad_artifact, &public_stream.tokens)
                .unwrap_err()
                .contains("registered model")
        );
        if !lie {
            assert_eq!(verifier.check_segmented_computation(&NoPrivate, &public_stream.tokens).unwrap(), OutsiderFindingV1::Clean);
        } else {
            let public = counted(&public_stream);
            let OutsiderFindingV1::Prosecute(proof) = verifier.check_segmented_computation(&public, &public_stream.tokens).unwrap()
            else {
                panic!("a fault whose position was not announced must be localized")
            };
            let mut read = public.positions.borrow().clone();
            read.sort_unstable();
            read.dedup();
            assert_eq!(read, vec![60, 61]);
            assert!(public.probes.get() <= 10);
            let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof })]);
            assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{ev:?}");
        }
        let mut wrong_ids = public_stream.tokens.clone();
        wrong_ids[0] = (wrong_ids[0] + 1) % 32;
        assert!(verifier.check_segmented_computation(&NoPrivate, &wrong_ids).unwrap_err().contains("authenticated public job"));
    }
}

impl SegMaterialV1 for Counted<'_> {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        let v = self.inner.position(p)?;
        self.bytes.set(self.bytes.get() + v.iter().flatten().map(|t| (t.len() * t.dtype.width()) as u128).sum::<u128>());
        self.positions.borrow_mut().push(p);
        Some(v)
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        self.inner.position_siblings(p)
    }
    fn commitments(&self, p: u32) -> Option<Vec<Vec<Digest>>> {
        self.inner.commitments(p)
    }
    fn position_path(&self, p: u32) -> Option<(Digest, Vec<Digest>)> {
        self.probes.set(self.probes.get() + 1);
        (p < self.inner.c.positions()).then(|| self.inner.c.position_path(p))
    }
}

/// The verifier's own re-execution of a claim's fed ids: its position roots and its first decode mismatch.
fn reexecute(w: &W, ctx: &SegClaimContextV1<'_>, tokens: &[u32]) -> (Vec<Digest>, Option<u32>) {
    let own = trace_v1(&w.program, &w.params, tokens).unwrap();
    let post = w.program.occurrences().len() - 1;
    let logits = |p: u32| own.values.get(p as usize).map(|v| v[post][w.program.logits as usize].clone());
    let mismatch = first_decode_mismatch_v1(ctx, &logits);
    (seg_commitments_of_trace_v1(&own).position_roots, mismatch)
}

/// **SOUND's SG-06: detection by re-execution, not by sampling** (`seg_detect`, design §11). A verifier that re-executes the claim
/// compares roots. An honest claim is Clean and no material is read. A one-element lie at position 3,000 (segment 2) is found with
/// certainty: its segment is descended in at most 10 position paths, and exactly positions 2,999 and 3,000 are read, then the claim is
/// convicted. A delivered id that is not the decode rule's, over honest values, is a decode fault found from one position. Sampling
/// two positions at random reads the same material and finds the lie with probability 1/4,501.
#[test]
fn k2s_a_reexecuting_verifier_finds_any_lie_with_certainty_reading_two_positions() {
    let fx = wide128_v1(7);
    let mut w = W::new(fx.program, fx.params, 8192);
    let prompt: Vec<u32> = (0..4500u32).map(|i| (i * 11 + 3) % 32).collect();
    let (s, n) = w
        .program
        .occurrences()
        .iter()
        .enumerate()
        .find_map(|(s, (b, _))| {
            w.program.blocks[*b as usize]
                .nodes
                .iter()
                .position(|nd| matches!(nd.prim, misaka_palw_tir::Prim::MatMul))
                .map(|n| (s as u16, n as u16))
        })
        .unwrap();
    // An honest claim: every root agrees; nothing of the producer's is read.
    let job = w.tiled_job(&prompt, 2);
    let honest = produce(&w, job, &prompt, None);
    commit(&mut w, &honest);
    let view = w.l.seg_claim_view_v1(&honest.claim.id()).unwrap();
    let ctx = view.context();
    let (own, mismatch) = reexecute(&w, &ctx, &honest.tokens);
    let m = counted(&honest);
    let r = check_claim_by_reexecution_v1(&ctx, &own, mismatch, &m, &artifact(&w), &honest.tokens);
    assert_eq!((r.finding, r.divergent, r.probes, m.bytes.get()), (SegFindingV1::Clean, None, 0, 0));
    // One wrong element at position 3,000.
    let job = w.tiled_job(&prompt, 2);
    let liar = produce(&w, job, &prompt, Some((3000, s, n)));
    commit(&mut w, &liar);
    let id = liar.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let ctx = view.context();
    let (own, mismatch) = reexecute(&w, &ctx, &liar.tokens);
    assert_eq!(mismatch, None, "the delivered ids are the rule's");
    let m = counted(&liar);
    let r = check_claim_by_reexecution_v1(&ctx, &own, mismatch, &m, &artifact(&w), &liar.tokens);
    assert_eq!(r.divergent, Some(3000), "the first divergent position");
    assert!((1..=10).contains(&r.probes) && r.probes == m.probes.get(), "{} probes", r.probes);
    let mut read = m.positions.borrow().clone();
    read.sort_unstable();
    read.dedup();
    assert_eq!(read, vec![2999, 3000], "two positions of material, whatever the context");
    let SegFindingV1::Fault(fault) = r.finding else { panic!("the lie is not found: {:?}", r.finding) };
    assert_eq!(fault.at(), Some((3000, s, n)), "{fault:?}");
    let bytes = fault.to_bytes();
    eprintln!(
        "[k2s] SG-06 re-execution check: {} positions, divergent position {:?} after {} position paths, material read {} B \
         (positions {read:?}), filing {} B",
        ctx.positions,
        r.divergent,
        r.probes,
        m.bytes.get(),
        bytes.len()
    );
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(bytes) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{ev:?}");
    // A wrong delivered id over honest values: every root agrees and the decode relation does not.
    let job = w.tiled_job(&prompt, 2);
    let mut cheat = produce(&w, job, &prompt, None);
    cheat.claim.generated[1] = (cheat.claim.generated[1] + 1) % 32;
    let ev = commit(&mut w, &cheat);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
    let id = cheat.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let ctx = view.context();
    let (own, mismatch) = reexecute(&w, &ctx, &cheat.tokens);
    assert_eq!(mismatch, Some(4500), "the last delivered id is not the rule's");
    let m = counted(&cheat);
    let r = check_claim_by_reexecution_v1(&ctx, &own, mismatch, &m, &artifact(&w), &cheat.tokens);
    assert_eq!((r.divergent, r.probes), (None, 0), "no root differs");
    let SegFindingV1::Fault(fault) = r.finding else { panic!("the decode lie is not found: {:?}", r.finding) };
    assert!(matches!(fault.as_ref(), SegFaultV1::Decode(_)), "{fault:?}");
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(fault.to_bytes()) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{ev:?}");
}

/// One layer that broadcasts the hidden vector to `[40,000, 8]` (1.28 MB a position) and sums it back: a position served in parts.
fn fat_v1(seed: u8) -> (TirProgramV1, MapParams) {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let tok = pb.param("tok", DType::I8, &[16, 8], false);
    let lm = pb.param("lm", DType::I8, &[16, 8], false);
    let carry = vec![TensorType::fixed(DType::I32, &[8])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.cast(row, DType::I32);
        b.finish(&[x])
    };
    let fat = {
        let mut b = pb.block("fat", carry.clone());
        let wide = b.iota(DType::I32, &[Dim::Fixed(40_000), Dim::Fixed(8)], 0, 0, 1);
        let s = b.reduce_sum(wide, 0, DType::I64);
        let s = b.reshape_fixed(s, &[8]);
        let c = b.clamp(s, -(1 << 20), 1 << 20, DType::I32);
        b.finish(&[c])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let h = b.clamp(Ref::CarryIn(0), -32767, 32767, DType::I16);
        let hc = b.reshape_fixed(h, &[8, 1]);
        let l = b.matmul(lm, hc, DType::I32);
        let l = b.reshape_fixed(l, &[16]);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = pb.finish(pre, vec![fat], post, logits);
    let mut tensors = std::collections::BTreeMap::new();
    for (j, d) in program.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let data: Vec<i128> = (0..n).map(|i| ((i as i128 * 37 + seed as i128 * 11 + j as i128) % 255) - 127).collect();
        tensors.insert((j as u16, None), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
    }
    (program, MapParams { tensors })
}

#[test]
fn k2s_small_model_values_are_never_served_and_a_model_holder_convicts_from_commitments() {
    use misaka_palw_kernel::seg_da::{AssembledPositionV1, ChunkBodyV1};
    struct PublicParts(std::collections::BTreeMap<u32, AssembledPositionV1>);
    impl SegMaterialV1 for PublicParts {
        fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
            self.0.get(&p).map(|x| x.0.clone())
        }
        fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
            self.0.get(&p).map(|x| x.1.clone())
        }
        fn commitments(&self, p: u32) -> Option<Vec<Vec<Digest>>> {
            self.0.get(&p).map(|x| x.2.clone())
        }
    }
    for fx in [wide128_v1(7), misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, 4)] {
        let mut w = W::new(fx.program, fx.params, 16);
        let scope = misaka_palw_kernel::seg_scope::seg_scope_report_v1(&w.program);
        assert!(scope.masked > 0 && scope.court_scope_complete(), "{scope:?}");
        let mask = misaka_palw_kernel::seg_scope::seg_withheld_mask_v1(&w.program);
        let prompt = [1, 3, 5, 7, 9];
        let job = w.tiled_job(&prompt, 2);
        let lying = produce(&w, job, &prompt, Some((3, 0, 0)));
        commit(&mut w, &lying);
        let id = lying.claim.id();
        let view = w.l.seg_claim_view_v1(&id).unwrap();
        let mut public = PublicParts(Default::default());
        assert_eq!(
            check_positions_v1(
                &view.context(),
                &public,
                &|_, _| panic!("a missing response must not trigger replay"),
                &lying.tokens,
                &[3]
            ),
            SegFindingV1::Demand(vec![2, 3])
        );
        for p in 0..view.positions {
            let mut encoded = Vec::new();
            for part in 0..position_parts_v1(&w.program, p).unwrap().len() as u32 {
                let response = position_part_v1(&w.program, p, &lying.values[p as usize], lying.c.position_path(p).1, part).unwrap();
                for c in &response.chunks {
                    assert_eq!(matches!(c.body, ChunkBodyV1::Withheld), mask[c.occurrence as usize][c.node as usize]);
                }
                encoded.push(borsh::to_vec(&response).unwrap());
            }
            public.0.insert(p, assemble_position_v1(&w.program, &view.segment_roots, view.positions, p, &encoded).unwrap());
        }
        // No producer value (including the wrong value) is an input to the verifier: only wire responses and its registered model.
        let r = misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &public, &artifact(&w), &lying.tokens);
        assert_eq!(r.divergent, Some(3));
        let SegFindingV1::Fault(fault) = r.finding else { panic!("{:?}", r.finding) };
        assert!(matches!(fault.as_ref(), SegFaultV1::WholeValue(_)));
        assert_eq!(fault.at(), Some((3, 0, 0)));
        let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(fault.to_bytes()) })]);
        assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, accuser, .. } if *claim == id && *accuser == OUT)), "{ev:?}");
    }
}

#[test]
fn k2s_a_position_is_served_in_parts_and_its_demand_bonds_wait_for_the_grace() {
    let (program, params) = fat_v1(3);
    let mut w = W::new(program, params, 64);
    let prompt: Vec<u32> = vec![3, 9, 14];
    let job = w.tiled_job(&prompt, 2);
    let honest = produce(&w, job, &prompt, None);
    commit(&mut w, &honest);
    let id = honest.claim.id();
    let parts = position_parts_v1(&w.program, 1).unwrap();
    assert!(parts.len() >= 2, "a 1.28 MB position is served in {} parts", parts.len());
    let ev = w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: id, stage: 0, position: 1 })]);
    assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { .. })), "{ev:?}");
    let reserved = w.l.bonds[&OUT].reserved;
    assert!(reserved > 0);
    let mut served = Vec::new();
    for i in 0..parts.len() as u32 {
        let part = position_part_v1(&w.program, 1, &honest.values[1], honest.c.position_path(1).1, i).unwrap();
        let bytes = borsh::to_vec(&part).unwrap();
        assert!(bytes.len() <= 1 << 20 && bytes.len() < CARRIER, "part {i}: {} B", bytes.len());
        let ev = w.block(vec![obj(PROD, O::Respond { claim: id, stage: 0, position: 1, bytes: bytes.clone() })]);
        served.push(bytes);
        let done = ev.iter().any(|e| matches!(e, E::Served { position: 1, .. }));
        assert_eq!(done, i + 1 == parts.len() as u32, "part {i}: {ev:?}");
    }
    // Public from now on; the demand bond is still held, then settled at the grace's end.
    let ev = w.block(vec![obj(ANY, O::FileDemand { demander: ANY, claim: id, stage: 0, position: 1 })]);
    assert!(refusal(&ev).is_some_and(|r| r.contains("already served")), "{ev:?}");
    assert_eq!(w.l.bonds[&OUT].reserved, reserved, "held until the grace ends");
    w.beat_to(w.daa + 12);
    assert_eq!(w.l.bonds[&OUT].reserved, 0, "settled at the grace's end");
    // A fresh verifier rebuilds the position from the served parts alone and checks it.
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let (values, siblings, commitments) = assemble_position_v1(&w.program, &view.segment_roots, view.positions, 1, &served).unwrap();
    assert_eq!(commitments, honest.commitments(1).unwrap(), "every node commitment, withheld values' included");
    let withheld = misaka_palw_kernel::seg_scope::seg_withheld_mask_v1(&w.program);
    for (s, occ) in values.iter().enumerate() {
        for (n, t) in occ.iter().enumerate() {
            if !withheld[s][n] {
                assert_eq!(t, &honest.values[1][s][n], "a served value is the committed one");
            }
        }
    }
    assert_eq!(siblings, honest.c.position_path(1).1);
}

// ---- the 9B-8k fixture, shape level -----------------------------------------------------------------------------------------------

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn fixture_program(name: &str) -> Option<(TirProgramV1, u32)> {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../consensus/src/pipeline/virtual_processor/tests/fixtures/g14/shipped")
        .join(name);
    let text = std::fs::read_to_string(path).ok()?;
    let field = |k: &str| -> String {
        let at = text.find(&format!("\"{k}\"")).unwrap();
        let rest = &text[at + k.len() + 2..];
        let rest = rest[rest.find(|c: char| c == '"' || c.is_ascii_digit()).unwrap()..].to_string();
        if let Some(stripped) = rest.strip_prefix('"') {
            stripped[..stripped.find('"').unwrap()].to_string()
        } else {
            rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap()].to_string()
        }
    };
    let program = TirProgramV1::decode_canonical(&unhex(&field("program_borsh_hex"))).unwrap();
    Some((program, field("max_context").parse().unwrap()))
}

/// The interim route policy's prosecution ceilings (`palw_kernel_route_policy_v1`).
const ROUTE: ProsecutionPolicyV1 = ProsecutionPolicyV1 {
    court_deadline_daa: 20,
    max_sessions_per_claim: 1 << 10,
    max_public_bytes: 1 << 40,
    max_verifier_ram: 1 << 36,
    max_retained_state: 1 << 32,
};

#[test]
fn k2s_huihui_qwen35_9b_8k_passes_the_real_scale_gate_and_the_carriers() {
    let Some((program, ctx)) = fixture_program("huihui-qwen3.5-9b-8k.json") else {
        eprintln!("[k2s] the 9B fixture is not present");
        return;
    };
    assert_eq!(ctx, 8192);
    let root = program_root_v1(&program.encode());
    let material = ProfileMaterialV1::kernel_route(true);
    // K2-TIR-v2 (as measured by COV-P1P2): the whole-claim gate refuses, and so do the carriers.
    let v2 = k2_tir_v2_descriptor();
    let plan2 = plan_for_tir_program_v1(&v2, &program, root, ctx).unwrap();
    let gaps = public_prosecution_complete_v1(&v2, &program, &plan2, &material, &ROUTE).unwrap_err();
    eprintln!(
        "[k2s] 9B-8k K2-TIR-v2: {} relations, worst court {} B; gate refuses: {gaps:?}",
        plan2.relations.len(),
        plan2.budgets.worst_court_bytes
    );
    assert!(format!("{gaps:?}").contains("public bytes") && format!("{gaps:?}").contains("concurrent sessions"));
    // K2-TIR-v4: the plan checks, the per-prosecution gate passes, every filing / response / commitment fits the carrier.
    let v4 = k2_tir_v4_descriptor();
    let plan = plan_for_tir_program_v1(&v4, &program, root, ctx).unwrap();
    let armed = KernelScheduleV1::default().with(v4.digest(), KernelStatusV1::Active { since_daa: 0 });
    let accepted = check_plan_v1(&armed, &v4, &program, root, &plan, 0).unwrap();
    let (bounds, seg) = public_prosecution_complete_v4(&v4, &plan, &program, &material, &ROUTE).unwrap();
    carrier_fit_v1(&bounds, CARRIER, CARRIER, CARRIER).unwrap();
    // GAP 8's figure: what rebuilding this class's row costs (`rows::class_row_of`: the program decode, then the v4 gate). The node
    // pays it on the block's first ledger load only; every later object of the block takes the cached ledger.
    let bytes = program.encode();
    let t = std::time::Instant::now();
    let decoded = TirProgramV1::decode_canonical(&bytes).unwrap();
    let decode_time = t.elapsed();
    let t = std::time::Instant::now();
    public_prosecution_complete_v4(&v4, &plan, &decoded, &material, &ROUTE).unwrap();
    let gate_time = t.elapsed();
    eprintln!(
        "[k2s] GAP 8: a 9B-8k class row rebuilds in {decode_time:?} (program decode, {} B) + {gate_time:?} (the v4 gate), this build \
         (debug: {})",
        bytes.len(),
        cfg!(debug_assertions)
    );
    let worst = {
        let mut best = (0u64, 0u8, 0u16);
        let nc = seg.node_count;
        let withheld = misaka_palw_kernel::seg_scope::seg_withheld_mask_v1(&program);
        for r in &plan.relations {
            let (b, _) = misaka_palw_kernel::element::element_court_cost_masked_v1(
                &program,
                r.block as usize,
                r.node as usize,
                nc,
                ctx,
                None,
                &withheld,
            );
            if b > best.0 {
                best = (b, r.block, r.node);
            }
        }
        best
    };
    eprintln!(
        "[k2s] 9B-8k K2-TIR-v4: eps <= 2^-{}; nodes/position {}; segments {}; position material {} B in {} parts; claim material (producer DA) {} B; \
         per prosecution: public {} B, RAM {} B, sessions {}, rounds {}; worst element court {} B / work {} (block {} node {}); response {} B; \
         retained on chain {} B",
        accepted.error_bits,
        seg.node_count,
        seg.segments,
        seg.position_material_bytes,
        seg.parts_per_position,
        seg.claim_material_bytes,
        bounds.max_public_bytes,
        bounds.max_verifier_ram,
        bounds.max_concurrent_sessions,
        bounds.max_localization_rounds,
        bounds.max_opening_bytes,
        bounds.max_court_work,
        worst.1,
        worst.2,
        bounds.max_response_bytes,
        bounds.max_retained_state,
    );
    assert!(bounds.max_filing_bytes < CARRIER as u64);
    assert_eq!(bounds.max_concurrent_sessions, 2);
    assert!(bounds.max_retained_state > bounds.max_commit_bytes + 8192 * 64 * 40);
    assert!(bounds.max_retained_state < ROUTE.max_retained_state);
    assert_eq!(bounds.max_localization_rounds, 12);
    // The full source context, informational: the same program at the class's history bound (the outcome is printed, not asserted).
    for positions in [262_144u32, 2_097_152] {
        match plan_for_tir_program_v1(&v4, &program, root, positions) {
            Ok(p) => match check_plan_v1(&armed, &v4, &program, root, &p, 0) {
                Ok(_) => match public_prosecution_complete_v4(&v4, &p, &program, &material, &ROUTE) {
                    Ok((b, s)) => eprintln!(
                        "[k2s] 9B @{positions}: gate PASS — public {} B, worst court {} B, carrier {:?}, segments {}, claim material {} B",
                        b.max_public_bytes,
                        b.max_opening_bytes,
                        carrier_fit_v1(&b, CARRIER, CARRIER, CARRIER),
                        s.segments,
                        s.claim_material_bytes
                    ),
                    Err(g) => eprintln!("[k2s] 9B @{positions}: gate refuses {g:?}"),
                },
                Err(o) => eprintln!(
                    "[k2s] 9B @{positions}: check_plan {} — {}",
                    o.code(),
                    o.to_string().chars().take(300).collect::<String>()
                ),
            },
            Err((f, why)) => eprintln!("[k2s] 9B @{positions}: no plan ({}: {why})", f.name()),
        }
    }
}

#[test]
fn k2s_every_tile_and_part_constant_fits_the_node_carrier() {
    assert!((misaka_palw_kernel::seg_da::SEG_PART_BYTES_V4 as usize) < CARRIER);
    assert!(PROMPT_TILE_IDS_V1 * 4 + 64 * 16 < misaka_palw_kernel::route::MAX_POST_PROMPT_TILE_BYTES_V1);
    assert!(misaka_palw_kernel::route::MAX_POST_PROMPT_TILE_BYTES_V1 < CARRIER);
}

fn conformance_world() -> W {
    let fx = wide128_v1(73);
    let mut w = W::new(fx.program, fx.params, 32);
    w.l =
        KernelLedgerV1::genesis(w.l.policy, w.l.schedule.clone(), w.l.known.clone()).unwrap().with_opv_policy(opv_example()).unwrap();
    w.l.sync_bond(PROD, 1_000_000);
    w.l.sync_bond(ANY, 1_000_000);
    w.l.attest_artifact(w.pc.root());
    w.l.begin_block(1).unwrap();
    w.daa = 1;
    w
}

fn candidate(w: &W, max_positions: u32) -> O {
    let bytes = w.program.encode();
    O::RegisterConformanceClass {
        descriptor: w.d.digest(),
        program_bytes: bytes.clone(),
        plan: plan_for_tir_program_v1(&w.d, &w.program, program_root_v1(&bytes), max_positions).unwrap(),
        param_commitments: w.pc.clone(),
    }
}

#[test]
fn conformance_candidate_is_rooted_replayable_and_grants_no_execution_rights() {
    use misaka_palw_kernel::rows::{TABLE_CONFORMANCE_CLASSES_V1, config_root_of, diff_rows, root_of_rows};
    let mut w = conformance_world();
    let o = candidate(&w, 32);
    assert_eq!(o.encode()[..2], [1, 21]);
    assert_eq!(O::decode(&o.encode()).unwrap(), o);
    let before = w.l.to_rows();
    let root = w.l.root();
    let bonds = w.l.bonds.clone();
    let ev = w.l.apply_object(&o, &AuthV1 { signer_bond: PROD }).unwrap();
    assert_eq!(ev, vec![E::ConformanceClassRegistered { class: w.class }]);
    assert!(w.l.conformance_classes.contains_key(&w.class));
    assert!(w.l.classes.is_empty() && w.l.opv.classes.is_empty() && w.l.opv.admitted.is_empty());
    assert_eq!(w.l.bonds, bonds, "no reward, reservation or collateral transfer");
    assert!(w.l.claims.is_empty() && w.l.claim_beacon_salts.is_empty() && w.l.seals.is_empty());
    assert_ne!(root, w.l.root());
    let mut empty = w.l.clone();
    empty.conformance_classes.clear();
    assert_eq!(empty.root(), root, "empty table preserves the historical root grammar");
    assert!(w.l.budget_used().court_work > 0, "candidate shares admission work, not only the run limit");
    let rows = w.l.to_rows();
    assert!(rows.keys().any(|(t, _)| *t == TABLE_CONFORMANCE_CLASSES_V1));
    assert_eq!(
        root_of_rows(&w.l.policy, config_root_of(&w.l.schedule, &w.l.known), w.l.scalars(), w.l.opv_policy(), &rows),
        w.l.root()
    );
    let restored = KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &rows).unwrap();
    assert_eq!(restored.root(), w.l.root());
    assert_eq!(restored.to_rows(), rows);
    let journal = diff_rows(&before, &rows);
    let mut rolled = rows.clone();
    for (k, old, _) in &journal {
        match old {
            Some(v) => {
                rolled.insert(k.clone(), v.clone());
            }
            None => {
                rolled.remove(k);
            }
        }
    }
    assert_eq!(rolled, before);
    for (k, _, new) in &journal {
        match new {
            Some(v) => {
                rolled.insert(k.clone(), v.clone());
            }
            None => {
                rolled.remove(k);
            }
        }
    }
    assert_eq!(rolled, rows);
    let mut wrong_key = rows.clone();
    let value = wrong_key.remove(&(TABLE_CONFORMANCE_CLASSES_V1, borsh::to_vec(&w.class).unwrap())).unwrap();
    wrong_key.insert((TABLE_CONFORMANCE_CLASSES_V1, borsh::to_vec(&[0xCCu8; 64]).unwrap()), value);
    assert!(KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &wrong_key).is_err());
    let job = KernelJobV1 {
        class_binding_id: w.class,
        prompt: vec![1, 3, 5],
        max_new_tokens: 1,
        decode: DecodeRuleV1::Greedy,
        nonce: [1; 64],
    };
    assert!(w.l.apply_object(&O::PostJob { job }, &AuthV1 { signer_bond: ANY }).is_err());
    let tiled = TiledJobV1 {
        class_binding_id: w.class,
        prompt_len: 3,
        prompt_root: prompt_root_of_ids_v1(&[1, 3, 5]),
        max_new_tokens: 1,
        decode: DecodeRuleV1::Greedy,
        nonce: [2; 64],
    };
    assert!(w.l.apply_object(&O::PostTiledJob { job: tiled }, &AuthV1 { signer_bond: ANY }).is_err());
    let registration = w.register(OPV);
    let LedgerTxV1::Object { object, auth } = registration else { unreachable!() };
    assert!(w.l.apply_object(&object, &auth).is_err(), "ordinary registration still needs eligibility");
    assert_eq!(w.l.root(), restored.root(), "refusals changed no rooted state");
    w.l.begin_block(2).unwrap();
    w.l.admit_optimistic_class(w.class).unwrap(); // consumer-derived eligibility is tested on the actual node separately.
    assert_eq!(w.l.apply_object(&object, &auth).unwrap(), vec![E::ClassRegistered { class: w.class }]);
    assert!(w.l.conformance_classes.is_empty());
    assert!(w.l.classes.contains_key(&w.class) && w.l.opv.classes.contains(&w.class));
    w.l.opv_invariants().unwrap();
}

#[test]
fn conformance_candidate_is_limited_by_bond_and_shared_block_work_and_refuses_bad_metadata() {
    use misaka_palw_kernel::ledger::{BlockBudgetV1, MAX_CONFORMANCE_CLASSES_PER_BOND_V1};
    let mut w = conformance_world();
    for i in 1..=MAX_CONFORMANCE_CLASSES_PER_BOND_V1 {
        w.l.begin_block(i as u64).unwrap();
        w.l.apply_object(&candidate(&w, i as u32), &AuthV1 { signer_bond: PROD }).unwrap();
    }
    w.l = KernelLedgerV1::from_rows(&w.l, w.l.scalars(), &w.l.to_rows()).unwrap();
    w.l.begin_block(9).unwrap();
    let root = w.l.root();
    let err = w.l.apply_object(&candidate(&w, 9), &AuthV1 { signer_bond: PROD }).unwrap_err();
    assert!(err.why.contains("candidate limit"));
    assert_eq!(w.l.root(), root);
    w.l.apply_object(&candidate(&w, 9), &AuthV1 { signer_bond: ANY }).unwrap();
    let mut w = conformance_world();
    w.l.restore_budget(BlockBudgetV1 { adjudications: 0, court_work: w.l.policy.admission_work_limit_v1() });
    let root = w.l.root();
    let err = w.l.apply_object(&candidate(&w, 32), &AuthV1 { signer_bond: PROD }).unwrap_err();
    assert_eq!(err.kind, misaka_palw_kernel::route::RefusalKindV1::OverBudget);
    assert_eq!(w.l.root(), root);
    assert!(w.l.conformance_classes.is_empty());
    for kind in 0..5 {
        w.l.begin_block(2 + kind).unwrap();
        let mut o = candidate(&w, 32);
        let O::RegisterConformanceClass { descriptor, program_bytes, plan, param_commitments } = &mut o else { unreachable!() };
        match kind {
            0 => *descriptor = [0; 64],
            1 => program_bytes.push(0),
            2 => plan.max_positions = 0,
            3 => param_commitments.by_instance.clear(),
            _ => w.l.attested_artifacts.clear(),
        }
        let root = w.l.root();
        assert!(w.l.apply_object(&o, &AuthV1 { signer_bond: PROD }).is_err());
        assert_eq!(w.l.root(), root);
        assert!(w.l.conformance_classes.is_empty());
    }
}
