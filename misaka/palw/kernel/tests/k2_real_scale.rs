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
    let c =
        seg_commitments_of_trace_v1(&w.program, &misaka_palw_kernel::trace::TraceV1 { values: values.clone(), inputs: Vec::new() });
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
    fn commitments(&self, program: &TirProgramV1, p: u32) -> Option<Vec<Vec<Digest>>> {
        self.inner.commitments(program, p)
    }
    fn position_path(&self, _: &TirProgramV1, p: u32) -> Option<(Digest, Vec<Digest>)> {
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
    (seg_commitments_of_trace_v1(&w.program, &own).position_roots, mismatch)
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
        fn commitments(&self, _: &TirProgramV1, p: u32) -> Option<Vec<Vec<Digest>>> {
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
    assert_eq!(commitments, honest.commitments(&w.program, 1).unwrap(), "every node commitment, withheld values' included");
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
        .join("../../../consensus/src/pipeline/virtual_processor/tests/fixtures/g14/shipped")
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

// ---- G14-S (2026-10-11): DA16b option (a), flat 64-element hiding tiles; option (c) finished; the scope as a gate --------------------

use misaka_palw_kernel::hide::{HIDE_TILE_V1, hide_tile_range_v1, hide_tiles_v1};
use misaka_palw_kernel::seg_da::{ChunkBodyV1, HiddenLeavesV1, assemble_position_full_v1};
use misaka_palw_kernel::seg_scope::{SegValueModeV1, seg_scope_report_v2, seg_value_modes_v1};

/// A token embedding and an output projection of `16,384 × 64` `i8` (1 MiB each): every model-dependent value of this class is
/// larger than a whole-value court can file, so the embedding row, the projection and the logits are HIDING values; the casts and
/// views of the 64-element hidden vector are withheld (small whole-value courts).
fn hide_lm_v1(seed: u8) -> (TirProgramV1, MapParams) {
    const V: u32 = 16_384;
    const D: u32 = 64;
    let mut pb = ProgramBuilder::new(V, HISTORY_BOUND_V1_SMALL);
    let tok = pb.param("tok", DType::I8, &[V, D], false);
    let lm = pb.param("lm", DType::I8, &[V, D], false);
    let carry = vec![TensorType::fixed(DType::I32, &[D])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let row = b.gather(tok, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.cast(row, DType::I32);
        b.finish(&[x])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let h = b.clamp(Ref::CarryIn(0), -32767, 32767, DType::I16);
        let hc = b.reshape_fixed(h, &[D, 1]);
        let l = b.matmul(lm, hc, DType::I32);
        let l = b.reshape_fixed(l, &[V]);
        b.commit(l);
        b.finish(&[])
    };
    let logits = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let program = pb.finish(pre, vec![], post, logits);
    let mut tensors = std::collections::BTreeMap::new();
    for (j, d) in program.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let data: Vec<i128> = (0..n).map(|i| ((i as i128 * 131 + seed as i128 * 17 + j as i128 * 7) % 255) - 127).collect();
        tensors.insert((j as u16, None), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
    }
    (program, MapParams { tensors })
}

/// A fresh verifier's view of what the chain served: the assembled positions (values, siblings, commitments) and the served leaf
/// hashes of every hiding value — never the producer's objects.
struct ServedParts(std::collections::BTreeMap<u32, (misaka_palw_kernel::seg_da::AssembledPositionV1, HiddenLeavesV1)>);

impl SegMaterialV1 for ServedParts {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        self.0.get(&p).map(|x| x.0.0.clone())
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        self.0.get(&p).map(|x| x.0.1.clone())
    }
    fn commitments(&self, _: &TirProgramV1, p: u32) -> Option<Vec<Vec<Digest>>> {
        self.0.get(&p).map(|x| x.0.2.clone())
    }
    fn hidden_leaves(&self, _: &TirProgramV1, p: u32) -> Option<HiddenLeavesV1> {
        self.0.get(&p).map(|x| x.1.clone())
    }
}

/// Serve every position of `claim` part by part (what a demanded producer posts) and assemble what a fresh verifier reads; asserts
/// that no withheld or hiding value is served in the clear and that a hiding value is served as its leaf hashes alone.
fn serve_all(w: &W, claim: &Produced) -> ServedParts {
    let id = claim.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let modes = seg_value_modes_v1(&w.program);
    let mut out = ServedParts(Default::default());
    for p in 0..view.positions {
        let mut encoded = Vec::new();
        for part in 0..position_parts_v1(&w.program, p).unwrap().len() as u32 {
            let response = position_part_v1(&w.program, p, &claim.values[p as usize], claim.c.position_path(p).1, part).unwrap();
            for c in &response.chunks {
                let mode = modes[c.occurrence as usize][c.node as usize];
                match (&c.body, mode) {
                    (ChunkBodyV1::Hidden(range), SegValueModeV1::Hiding) => assert!(!range.leaves.is_empty()),
                    (ChunkBodyV1::Withheld, SegValueModeV1::Withheld) => {}
                    (ChunkBodyV1::Whole(_) | ChunkBodyV1::Rows(_), SegValueModeV1::Clear) => {}
                    (body, mode) => panic!("({}, {}) {mode:?} served as {body:?}", c.occurrence, c.node),
                }
            }
            encoded.push(borsh::to_vec(&response).unwrap());
        }
        out.0.insert(p, assemble_position_full_v1(&w.program, &view.segment_roots, view.positions, p, &encoded).unwrap());
    }
    out
}

fn final_of(w: &mut W, id: &Digest) {
    for _ in 0..400 {
        let ev = w.block(vec![]);
        if ev.iter().any(|e| matches!(e, E::Final { claim, .. } if claim == id)) {
            return;
        }
    }
    panic!("the claim never reached Final");
}

fn modes_count(program: &TirProgramV1) -> (usize, usize, usize) {
    let m = seg_value_modes_v1(program);
    let c = |x: SegValueModeV1| m.iter().flatten().filter(|y| **y == x).count();
    (c(SegValueModeV1::Clear), c(SegValueModeV1::Withheld), c(SegValueModeV1::Hiding))
}

/// **Option (a), honest and lying, before and after Final.** A class whose embedding row, projection and logits are hiding: a
/// demanded position serves them as leaf hashes only; a fresh verifier (served parts + its own registered model) dismisses nothing
/// and convicts a one-element lie in the projection with a hiding-leaf court that recomputes ONE 64-element tile — pre-Final and
/// post-Final. Every honest tile's filing is dismissed (and costs its filer the dismissal fee).
#[test]
fn g14s_a_hiding_tile_lie_is_convicted_pre_and_post_final_and_no_honest_tile_is() {
    let (program, params) = hide_lm_v1(5);
    let (clear, withheld, hiding) = modes_count(&program);
    let report = seg_scope_report_v2(&program, 16);
    eprintln!("[g14s] hide_lm: clear {clear}, withheld {withheld}, hiding {hiding}; report {report:?}");
    assert!(hiding >= 3 && withheld >= 1, "the embedding row, the projection and the logits are hiding");
    assert!(report.court_scope_complete() && report.clear_masked == 0, "{report:?}");
    let mut w = W::new(program, params, 16);
    let mm = w.program.blocks[w.program.occurrences()[1].0 as usize]
        .nodes
        .iter()
        .position(|n| matches!(n.prim, misaka_palw_tir::Prim::MatMul))
        .unwrap() as u16;
    let prompt = [3u32, 9, 14, 2, 11];
    // Honest: the verifier finds nothing; each honest tile filed is dismissed at the court and by the ledger.
    let job = w.tiled_job(&prompt, 2);
    let honest = produce(&w, job, &prompt, None);
    commit(&mut w, &honest);
    let hid = honest.claim.id();
    let view = w.l.seg_claim_view_v1(&hid).unwrap();
    let served = serve_all(&w, &honest);
    let r = misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &served, &artifact(&w), &honest.tokens);
    assert_eq!(r.finding, SegFindingV1::Clean, "an honest claim");
    let len = honest.values[3][1][mm as usize].data.len() as u64;
    assert_eq!(hide_tiles_v1(len), 256);
    for tile in [0, 1, 255] {
        let filing = misaka_palw_kernel::element::prove_hiding_leaf_v1(
            &view.context(),
            &honest,
            &artifact(&w),
            &honest.tokens,
            (3, 1, mm),
            tile,
        )
        .unwrap();
        let (a, b) = hide_tile_range_v1(len, tile).unwrap();
        assert!(b - a >= HIDE_TILE_V1, "a tile never holds fewer than 64 elements of a value that has them");
        assert_eq!(
            misaka_palw_kernel::element::verify_seg_fault_v1(&view.context(), &SegFaultV1::HidingLeaf(filing.clone())),
            Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
        );
        let bytes = SegFaultV1::HidingLeaf(filing).to_bytes();
        assert!(bytes.len() < CARRIER, "{} B", bytes.len());
        let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: hid, proof: ProsecutionV1::Segmented(bytes) })]);
        assert!(ev.iter().any(|e| matches!(e, E::ProofDismissed { claim, .. } if *claim == hid)), "{ev:?}");
    }
    // A one-element lie in the hiding projection at position 3: convicted before Final, from what was served.
    for post_final in [false, true] {
        let job = w.tiled_job(&prompt, 2);
        let lying = produce(&w, job, &prompt, Some((3, 1, mm)));
        commit(&mut w, &lying);
        let id = lying.claim.id();
        if post_final {
            final_of(&mut w, &id);
        }
        let view = w.l.seg_claim_view_v1(&id).unwrap();
        let served = serve_all(&w, &lying);
        let r = misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &served, &artifact(&w), &lying.tokens);
        assert_eq!(r.divergent, Some(3));
        let SegFindingV1::Fault(fault) = r.finding else { panic!("{:?}", r.finding) };
        let SegFaultV1::HidingLeaf(h) = fault.as_ref() else { panic!("a hiding-leaf court: {fault:?}") };
        assert_eq!((h.position, h.occurrence, h.node, h.leaf.tile), (3, 1, mm, 0));
        let bytes = fault.to_bytes();
        eprintln!(
            "[g14s] hiding-leaf filing for a 64-element tile of a 16,384-element projection: {} B (tile operands: {} v3 leaves, {} \
             hiding tiles, {} whole)",
            bytes.len(),
            h.inputs.iter().map(|i| i.leaves.len()).sum::<usize>(),
            h.inputs.iter().map(|i| i.hidden.len()).sum::<usize>(),
            h.inputs.iter().filter(|i| i.whole.is_some()).count()
        );
        let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(bytes) })]);
        assert!(
            ev.iter().any(|e| matches!(e, E::Convicted { claim, accuser, post_final: pf, .. } if *claim == id && *accuser == OUT && *pf == post_final)),
            "post_final {post_final}: {ev:?}"
        );
    }
}

/// **A decode lie over hiding logits** (16,384 ids: the logits are hiding): a wrong delivered id over honest values is convicted with
/// two hiding tiles of the verifier's own logits (which authenticate only as the producer's); an honest decode filing is dismissed.
#[test]
fn g14s_a_decode_lie_on_hiding_logits_is_convicted_from_two_tiles() {
    let (program, params) = hide_lm_v1(6);
    let mut w = W::new(program, params, 16);
    let post = (w.program.occurrences().len() - 1) as u16;
    assert_eq!(seg_value_modes_v1(&w.program)[post as usize][w.program.logits as usize], SegValueModeV1::Hiding);
    let prompt = [7u32, 1, 4, 4];
    let job = w.tiled_job(&prompt, 2);
    let mut cheat = produce(&w, job, &prompt, None);
    cheat.claim.generated[1] = (cheat.claim.generated[1] + 1) % 16_384;
    let ev = commit(&mut w, &cheat);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
    let id = cheat.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let served = serve_all(&w, &cheat);
    let r = misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &served, &artifact(&w), &cheat.tokens);
    assert_eq!(r.divergent, None, "every root agrees");
    let SegFindingV1::Fault(fault) = r.finding else { panic!("{:?}", r.finding) };
    let SegFaultV1::HidingDecode(d) = fault.as_ref() else { panic!("a hiding decode fault: {fault:?}") };
    assert!(!d.tiles.is_empty() && d.tiles.len() <= 2);
    // The same tiles naming a rival that does not beat the delivered id: dismissed.
    let mut honest_rival = d.clone();
    let logits = &cheat.values[view.positions as usize - 1][post as usize][w.program.logits as usize];
    let delivered = cheat.claim.generated[1] as usize;
    let weaker = d
        .tiles
        .iter()
        .flat_map(|t| t.elements(logits.dtype, &logits.shape).unwrap())
        .find(|(e, v)| *e as usize != delivered && *v < logits.data[delivered])
        .map(|(e, _)| e as u32);
    if let Some(rival) = weaker {
        honest_rival.rival = rival;
        assert_eq!(
            misaka_palw_kernel::element::verify_seg_fault_v1(&view.context(), &SegFaultV1::HidingDecode(honest_rival)),
            Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
        );
    }
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(fault.to_bytes()) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{ev:?}");
}

/// **wide128 (`d = 16`): option (c), no short leaf owed.** Every model-dependent value of the class is under 64 elements and has a
/// bounded whole-value court: all are withheld — served as their node opening alone, never as elements, leaves or leaf hashes — so
/// no 16-element line is ever hashed into an owed leaf. A lie is convicted by the whole-value court.
#[test]
fn g14s_wide128_d16_owes_no_short_leaf_and_a_lie_is_convicted_whole() {
    let fx = wide128_v1(7);
    let (clear, withheld, hiding) = modes_count(&fx.program);
    let report = seg_scope_report_v2(&fx.program, 16);
    eprintln!("[g14s] wide128: clear {clear}, withheld {withheld}, hiding {hiding}; report {report:?}");
    assert!(withheld > 0 && hiding == 0 && report.court_scope_complete(), "{report:?}");
    let mut w = W::new(fx.program, fx.params, 16);
    let prompt = [1u32, 3, 5, 7, 9];
    let (s, n) = (1u16, 0u16);
    assert_eq!(seg_value_modes_v1(&w.program)[s as usize][n as usize], SegValueModeV1::Withheld);
    let job = w.tiled_job(&prompt, 2);
    let lying = produce(&w, job, &prompt, Some((3, s, n)));
    commit(&mut w, &lying);
    let id = lying.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let served = serve_all(&w, &lying);
    let r = misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &served, &artifact(&w), &lying.tokens);
    let SegFindingV1::Fault(fault) = r.finding else { panic!("{:?}", r.finding) };
    assert!(matches!(fault.as_ref(), SegFaultV1::WholeValue(_)), "{fault:?}");
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(fault.to_bytes()) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{ev:?}");
}

/// **dense-MoE, rows of 4** (`hkv · dh = 2 × 4`): (1) a 4-row window — every model-dependent value withheld, complete; (2) the
/// 2^18-row window at a 16-position context — the windows and the attention values are hiding (a `[H, 2, 4]` window is tiled in
/// row-major order across its 8-element rows: every leaf of a ≥ 64-element value holds ≥ 64 elements), complete, and a lie in a
/// hiding window is convicted by a hiding-leaf court; (3) the same program at a context whose hiding attention courts no longer fit one
/// carrier — the report is incomplete and the per-claim reward gate refuses the class's claims by name.
#[test]
fn g14s_dense_moe_rows_of_4_windows_hide_flat_and_an_unbounded_scope_takes_no_claim() {
    use misaka_palw_tir_sketch::fixture::{dense_moe_v1, dense_moe_windowed_v1};
    // (1) A window of 4 rows.
    let small = dense_moe_windowed_v1(7, 4);
    let (_, w4, h4) = modes_count(&small.program);
    let r4 = seg_scope_report_v2(&small.program, 16);
    eprintln!("[g14s] dense-MoE window 4: withheld {w4}, hiding {h4}; {r4:?}");
    assert!(r4.court_scope_complete() && r4.masked > 0, "{r4:?}");
    // The node's GAP-31 class (window 1,024 held at 8,192 positions) and its wide128 class at 8,192: both complete (their claims commit).
    for (name, p) in [("window 1024", dense_moe_windowed_v1(7, 1024).program), ("wide128", wide128_v1(7).program)] {
        let r = seg_scope_report_v2(&p, 8192);
        eprintln!("[g14s] node fixture {name} : {r:?}");
        assert!(r.court_scope_complete(), "{name}: {r:?}");
    }
    // (2) The 2^18-row window at a 16-position context.
    let big = dense_moe_v1(7);
    let r16 = seg_scope_report_v2(&big.program, 16);
    eprintln!("[g14s] dense-MoE window 2^18 @16: {r16:?}");
    assert!(r16.hiding > 0 && r16.court_scope_complete(), "{r16:?}");
    let modes = seg_value_modes_v1(&big.program);
    // The first HistAppend window that is hiding.
    let (s, n) = big
        .program
        .occurrences()
        .iter()
        .enumerate()
        .find_map(|(s, (b, _))| {
            big.program.blocks[*b as usize].nodes.iter().enumerate().find_map(|(n, nd)| {
                (matches!(nd.prim, misaka_palw_tir::Prim::HistAppend { .. }) && modes[s][n] == SegValueModeV1::Hiding)
                    .then_some((s as u16, n as u16))
            })
        })
        .expect("a hiding window");
    let mut w = W::new(big.program.clone(), big.params.clone(), 16);
    let prompt: Vec<u32> = (0..12u32).map(|i| (i * 5 + 1) % 32).collect();
    let job = w.tiled_job(&prompt, 2);
    let lying = produce(&w, job, &prompt, Some((9, s, n)));
    let window = &lying.values[9][s as usize][n as usize];
    assert!(window.data.len() as u64 >= HIDE_TILE_V1, "a [10, 2, 4] window has {} elements", window.data.len());
    commit(&mut w, &lying);
    let id = lying.claim.id();
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let served = serve_all(&w, &lying);
    let r = misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &served, &artifact(&w), &lying.tokens);
    assert_eq!(r.divergent, Some(9));
    let SegFindingV1::Fault(fault) = r.finding else { panic!("{:?}", r.finding) };
    let SegFaultV1::HidingLeaf(h) = fault.as_ref() else { panic!("a hiding-leaf court: {fault:?}") };
    assert_eq!((h.position, h.occurrence, h.node, h.leaf.tile), (9, s, n, 0));
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(fault.to_bytes()) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, .. } if *claim == id)), "{ev:?}");
    // (3) A context whose hiding attention courts exceed one carrier: incomplete, and the class takes no claim.
    let (positions, report) = [2048u32, 4096, 8192, 16_384, 32_768]
        .into_iter()
        .map(|p| (p, seg_scope_report_v2(&big.program, p)))
        .find(|(_, r)| !r.court_scope_complete())
        .expect("some context outgrows the hiding court bound");
    eprintln!("[g14s] dense-MoE window 2^18 @{positions}: {report:?} — {}", report.refusal().unwrap());
    assert!(report.unbounded > 0 && report.refusal().unwrap().starts_with("COURT_SCOPE_INCOMPLETE"));
    // With G14-B merged, a context this large is refused EARLIER: its worst-case pursuit outlasts the 436-DAA lifetime ceiling, so
    // registration names PURSUIT_SCHEDULE and the per-claim scope gate is never reached for this class (both refusals hold; the
    // registration one comes first). Either way the class is never reward-bearing.
    let registered =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| W::new(big.program.clone(), big.params.clone(), positions)));
    match registered {
        Err(panic) => {
            let why = panic.downcast_ref::<String>().cloned().unwrap_or_default();
            assert!(why.contains("PURSUIT_SCHEDULE"), "refused at registration by the pursuit schedule: {why}");
        }
        Ok(mut wide) => {
            assert_eq!(wide.l.classes[&wide.class].court_scope.as_ref(), Some(&report), "the class row carries the gate");
            let job = wide.tiled_job(&prompt, 2);
            let honest = produce(&wide, job, &prompt, None);
            let ev = commit(&mut wide, &honest);
            let why = refusal(&ev).expect("the per-claim reward gate refuses the claim");
            assert!(why.contains("COURT_SCOPE_INCOMPLETE"), "{why}");
            assert!(!ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
        }
    }
}

/// **9B-scale activation sizing (a derivation over the shipped 9B-8k program's shapes, not a 9B run)**: every model-dependent value of
/// the class is withheld or hiding (never clear); leaf-hash DA per position; the hiding courts priced at 8,192 positions — which fit one
/// carrier and which do not — and the v4 gate's bounds with the hiding values kept.
#[test]
fn g14s_9b_8k_hiding_sizing_is_derived_from_the_shapes() {
    let Some((program, ctx)) = fixture_program("huihui-qwen3.5-9b-8k.json") else {
        eprintln!("[g14s] the 9B fixture is not present");
        return;
    };
    let (clear, withheld, hiding) = modes_count(&program);
    let report = seg_scope_report_v2(&program, ctx);
    let modes = seg_value_modes_v1(&program);
    let node_count: u64 = modes.iter().map(|o| o.len() as u64).sum();
    let occ = program.occurrences();
    // Leaf-hash DA: 64 bytes a hiding tile, at the last position.
    let mut leaf_hash_bytes = 0u128;
    let mut hiding_elements = 0u128;
    let mut priced = std::collections::BTreeMap::new();
    for (s, o) in modes.iter().enumerate() {
        let b = occ[s].0 as usize;
        for (n, m) in o.iter().enumerate() {
            if *m != SegValueModeV1::Hiding {
                continue;
            }
            let shape = program.blocks[b].nodes[n].out.resolve(8192usize.min(window_of(&program, b)));
            let len: u64 = shape.iter().map(|d| *d as u64).product();
            hiding_elements += len as u128;
            leaf_hash_bytes += hide_tiles_v1(len) as u128 * 64;
            priced.entry((b, n)).or_insert_with(|| {
                let (bytes, work) =
                    misaka_palw_kernel::element::hiding_leaf_court_cost_v1(&program, s, n, node_count, ctx, None, &modes);
                (program.blocks[b].nodes[n].prim.clone(), shape.clone(), bytes, work)
            });
        }
    }
    eprintln!(
        "[g14s] 9B-8k: values clear {clear}, withheld {withheld}, hiding {hiding}; report {report:?}; hiding elements a position \
         {hiding_elements}, leaf-hash DA a position {leaf_hash_bytes} B"
    );
    let mut over = 0;
    for ((b, n), (prim, shape, bytes, work)) in &priced {
        let fits = *bytes <= misaka_palw_kernel::seg_scope::SEG_HIDING_COURT_MAX_BYTES_V1;
        over += !fits as usize;
        eprintln!(
            "[g14s] 9B-8k hiding court block {b} node {n} {prim:?} {shape:?}: {bytes} B, work {work} ({})",
            if fits { "fits" } else { "UNBOUNDED" }
        );
    }
    assert_eq!(report.clear_masked, 0, "no model-dependent value is served in the clear");
    assert_eq!(report.unbounded > 0, over > 0);
    assert!(hiding > 0);
    // The v4 gate with the hiding values kept by the verifier's replay.
    let v4 = k2_tir_v4_descriptor();
    let root = program_root_v1(&program.encode());
    let plan = plan_for_tir_program_v1(&v4, &program, root, ctx).unwrap();
    match public_prosecution_complete_v4(&v4, &plan, &program, &ProfileMaterialV1::kernel_route(true), &ROUTE) {
        Ok((b, s)) => eprintln!(
            "[g14s] 9B-8k v4 gate PASS: RAM {} B, worst court {} B, position material {} B in {} parts, carrier {:?}",
            b.max_verifier_ram,
            b.max_opening_bytes,
            s.position_material_bytes,
            s.parts_per_position,
            carrier_fit_v1(&b, CARRIER, CARRIER, CARRIER)
        ),
        Err(g) => eprintln!("[g14s] 9B-8k v4 gate refuses: {g:?}"),
    }
}

fn window_of(program: &TirProgramV1, block: usize) -> usize {
    let mut w = 1usize;
    for node in &program.blocks[block].nodes {
        if let misaka_palw_tir::Prim::HistAppend { state } = node.prim
            && let misaka_palw_tir::program::StateKind::Hist { window } = program.states[state as usize].kind
        {
            w = w.max(window as usize);
        }
    }
    w
}

// ---- G14-B: maximum-profile budgets and deadlines (`misaka_palw_kernel::budget`) ----------------------------------------------------

/// The first MatMul of the program: `(occurrence, node)`.
fn first_matmul(program: &TirProgramV1) -> (u16, u16) {
    program
        .occurrences()
        .iter()
        .enumerate()
        .find_map(|(s, (b, _))| {
            program.blocks[*b as usize]
                .nodes
                .iter()
                .position(|nd| matches!(nd.prim, misaka_palw_tir::Prim::MatMul))
                .map(|n| (s as u16, n as u16))
        })
        .unwrap()
}

/// **G14-B, the profile table**: every shipped real-shape class at its maximum context through the v4 gate and the pursuit schedule
/// (search rounds × chunk-counted response windows + the filing's chunks against the liability horizon), at ceilings across `W`'s
/// range. The gate's verdict is the schedule's; the 9B-8k class is refused BY NAME at every ceiling: one position is 2,294 parts, its
/// window alone outlasts any horizon in the range.
#[test]
fn g14b_every_shipped_profile_is_admitted_with_its_derived_horizon_or_refused_by_name() {
    use misaka_palw_kernel::budget::*;
    let (l, opv) = (policy(), opv_example());
    let clock = PursuitClockV1::of_opv(&l, &opv);
    let v4 = k2_tir_v4_descriptor();
    let material = ProfileMaterialV1::kernel_route(true);
    let ceilings = [280u64, 300, 358, 400, 436];
    // The sketch's history-bearing dense + MoE class held at 8,192 positions, by its window (K2S GAP-31's node fixture is 1,024).
    for window in [16u32, 64, 128, 256, 512, 1024] {
        let fx = misaka_palw_tir_sketch::fixture::dense_moe_windowed_v1(7, window);
        let root = program_root_v1(&fx.program.encode());
        let Ok(plan) = plan_for_tir_program_v1(&v4, &fx.program, root, 8192) else {
            eprintln!("[g14b] dense-MoE window {window} @8192: no v4 plan");
            continue;
        };
        let bounds = match public_prosecution_complete_v4(&v4, &plan, &fx.program, &material, &ROUTE) {
            Ok((b, _)) => b,
            Err(g) => {
                eprintln!("[g14b] dense-MoE window {window} @8192: refused by the v4 gate {g:?}");
                continue;
            }
        };
        let (chunks, parts) = seg_position_chunks_v1(&fx.program, 8191).unwrap();
        let window_daa = response_window_daa_v1(l.court_deadline_daa, chunks);
        let filing_chunks = route_object_chunks_v1(bounds.max_filing_bytes as u128 + FILE_PROOF_ENVELOPE_BYTES_V1 as u128);
        let s = seg_pursuit_schedule_v1(&clock, seg_search_rounds_v1(8192), window_daa, filing_chunks);
        let verdicts: Vec<(u64, bool)> = ceilings
            .iter()
            .map(|c| {
                let ok = class_pursuit_with_ceiling_v1(
                    &v4,
                    &fx.program,
                    &plan,
                    bounds.max_filing_bytes,
                    bounds.max_response_bytes,
                    &l,
                    &opv,
                    *c,
                )
                .is_ok();
                assert_eq!(ok, s.final_daa + l.liability_daa.max(s.liability_needed_daa) <= *c, "dense-MoE window {window} at {c}");
                (*c, ok)
            })
            .collect();
        eprintln!(
            "[g14b] dense-MoE window {window} @8192: last position {parts} parts / {chunks} chunks -> window {window_daa} DAA; filing \
             {filing_chunks} chunks; Final +{}; liability needed {} DAA; lifetime {} DAA; admitted at lifetime ceilings {verdicts:?}",
            s.final_daa,
            s.liability_needed_daa,
            s.final_daa + l.liability_daa.max(s.liability_needed_daa)
        );
    }
    for name in ["mamba-370m.json", "granite-3.1-1b-a400m.json", "qwen3.5-0.8b.json", "smollm2-1.7b.json", "huihui-qwen3.5-9b-8k.json"]
    {
        let Some((program, ctx)) = fixture_program(name) else {
            eprintln!("[g14b] {name}: fixture absent");
            continue;
        };
        let root = program_root_v1(&program.encode());
        let plan = match plan_for_tir_program_v1(&v4, &program, root, ctx) {
            Ok(p) => p,
            Err((f, why)) => {
                eprintln!("[g14b] {name} @{ctx}: no v4 plan ({}: {why})", f.name());
                continue;
            }
        };
        let bounds = match public_prosecution_complete_v4(&v4, &plan, &program, &material, &ROUTE) {
            Ok((b, _)) => b,
            Err(g) => {
                eprintln!("[g14b] {name} @{ctx}: refused by the v4 gate {g:?}");
                continue;
            }
        };
        let (chunks, parts) = seg_position_chunks_v1(&program, ctx - 1).unwrap();
        let window = response_window_daa_v1(l.court_deadline_daa, chunks);
        let rounds = seg_search_rounds_v1(ctx);
        let filing_chunks = route_object_chunks_v1(bounds.max_filing_bytes as u128 + FILE_PROOF_ENVELOPE_BYTES_V1 as u128);
        let s = seg_pursuit_schedule_v1(&clock, rounds, window, filing_chunks);
        let pursuit = |c: u64| {
            class_pursuit_with_ceiling_v1(&v4, &program, &plan, bounds.max_filing_bytes, bounds.max_response_bytes, &l, &opv, c)
        };
        let verdicts: Vec<(u64, bool)> = ceilings.iter().map(|c| (*c, pursuit(*c).is_ok())).collect();
        eprintln!(
            "[g14b] {name} @{ctx}: last position {parts} parts / {chunks} route chunks -> response window {window} DAA; {rounds} rounds; \
             filing {} B in {filing_chunks} chunks; Final +{} ({} pre-Final rounds), last demand +{}, filed +{}; liability needed {} DAA; \
             admitted at ceilings {verdicts:?}",
            bounds.max_filing_bytes, s.final_daa, s.pre_final_rounds, s.last_demand_daa, s.filed_daa, s.liability_needed_daa
        );
        for (c, ok) in &verdicts {
            assert_eq!(
                *ok,
                s.final_daa + l.liability_daa.max(s.liability_needed_daa) <= *c,
                "{name}: the gate's verdict is the schedule's at {c}"
            );
        }
        if let Ok(p) = pursuit(LIFETIME_CEILING_DAA_V1) {
            assert_eq!(p.liability_daa, l.liability_daa.max(s.liability_needed_daa));
            assert_eq!((p.rounds, p.response_chunks, p.response_window_daa, p.filing_chunks), (rounds, chunks, window, filing_chunks));
        }
        if name.starts_with("huihui") {
            // G14-S's hiding tiles serve leaf hashes, not clear values: the last position is hundreds of parts (548), still far past
            // every horizon in W's range.
            assert!(ctx == 8192 && parts > 500, "the 9B-8k last position is hundreds of parts: {parts}");
            assert!(window > 436, "one position's own window outlasts every horizon in W's range: {window}");
            for c in ceilings {
                let g = pursuit(c).unwrap_err();
                assert!(format!("{g:?}").contains("PURSUIT_SCHEDULE"), "refused by name at {c}: {g:?}");
            }
        }
    }
}

/// **G14-B, the worst-case schedule on the ledger** (V-ref: every block through `apply_block`). A liar's segmented claim at 8,192
/// positions; a public verifier runs the deepest search the class admits (ten probes and the court's round, 11 demands) while the
/// producer answers every demand on the LAST DAA of its window; the claim reaches Final as early as the lifecycle lets it, and the
/// verifier waits out the dead zone between a served demand and Final. Every demand is admitted, every answer lands, and the filing
/// convicts inside the class's derived horizon — exactly at the times `budget::seg_pursuit_schedule_v1` derives. At the policy's own
/// 200 the deepest round could not even have opened.
#[test]
fn g14b_the_worst_case_pursuit_completes_on_the_ledger_inside_the_class_horizon() {
    use misaka_palw_kernel::budget::*;
    let fx = wide128_v1(7);
    let mut w = W::new(fx.program, fx.params, 8192);
    let (l, opv) = (policy(), opv_example());
    let clock = PursuitClockV1::of_opv(&l, &opv);
    let class_liability = w.l.classes[&w.class].liability_daa;
    let prompt: Vec<u32> = (0..2100u32).map(|i| (i * 7 + 1) % 32).collect();
    let job = w.tiled_job(&prompt, 2);
    let (s, n) = first_matmul(&w.program);
    let lie_at = 1200u32;
    let liar = produce(&w, job, &prompt, Some((lie_at, s, n)));
    let ev = commit(&mut w, &liar);
    assert!(ev.iter().any(|e| matches!(e, E::ClaimCommitted { .. })), "{ev:?}");
    let (c, id) = (w.daa, liar.claim.id());
    let window = w.l.demand_window_daa_v1(&id, lie_at);
    assert_eq!(window, l.court_deadline_daa, "a 1,104-byte position is one carrier");
    let view = w.l.seg_claim_view_v1(&id).unwrap();
    let SegFindingV1::Fault(fault) = check_positions_v1(&view.context(), &liar, &artifact(&w), &liar.tokens, &[lie_at]) else {
        panic!("the lie is not found")
    };
    let filing = fault.to_bytes();
    let filing_chunks = route_object_chunks_v1(filing.len() as u128 + FILE_PROOF_ENVELOPE_BYTES_V1 as u128);
    let rounds = seg_search_rounds_v1(8192);
    let plan = seg_pursuit_schedule_v1(&clock, rounds, window, filing_chunks);
    assert_eq!(class_liability, l.liability_daa.max(plan.liability_needed_daa), "registration derived this class's horizon");
    assert!(plan.liability_needed_daa > l.liability_daa, "the deepest search does not fit the policy's own 200");
    // Ten probe positions in the lie's segment, then the court's two positions together.
    let probes = [1024u32, 1536, 1280, 1152, 1088, 1120, 1136, 1144, 1148, 1150];
    let serve = |w: &W, p: u32| {
        let part = position_part_v1(&w.program, p, &liar.values[p as usize], liar.c.position_path(p).1, 0).unwrap();
        obj(PROD, O::Respond { claim: id, stage: 0, position: p, bytes: borsh::to_vec(&part).unwrap() })
    };
    let mut t = c + clock.first_step_daa;
    let (mut last_demand, mut last_served, mut final_daa) = (0u64, 0u64, None);
    for round in 0..rounds as usize {
        let positions = if round < probes.len() { vec![probes[round]] } else { vec![lie_at - 1, lie_at] };
        w.beat_to(t - 1);
        // In the dead zone (window closed, Final not yet reached) a demand is refused; the verifier retries the next block.
        let opened = loop {
            let demands =
                positions.iter().map(|p| obj(OUT, O::FileDemand { demander: OUT, claim: id, stage: 0, position: *p })).collect();
            let ev = w.block(demands);
            let refused = refusal(&ev);
            if let ClaimStateV1::Final { final_daa: f } = w.l.claims[&id].life.state {
                final_daa.get_or_insert(f);
            }
            match refused {
                None => break w.daa,
                Some(why) => {
                    assert!(why.contains("challenge window is closed"), "round {round} at {}: {why}", w.daa);
                    assert!(w.daa <= final_daa.unwrap_or(u64::MAX), "after Final the demand opens (round {round} at {})", w.daa);
                }
            }
        };
        let deadline = positions.iter().map(|p| w.l.demands[&(id, 0, *p)].deadline_daa).max().unwrap();
        assert_eq!(deadline, opened + window);
        // The producer stalls: every answer on the last DAA of its window.
        w.beat_to(deadline - 1);
        let answers = positions.iter().map(|p| serve(&w, *p)).collect();
        let ev = w.block(answers);
        assert_eq!(ev.iter().filter(|e| matches!(e, E::Served { .. })).count(), positions.len(), "round {round}: {ev:?}");
        if let ClaimStateV1::Final { final_daa: f } = w.l.claims[&id].life.state {
            final_daa.get_or_insert(f);
        }
        (last_demand, last_served) = (opened, w.daa);
        t = w.daa + clock.turnaround_daa;
    }
    let final_daa = final_daa.expect("Final during the pursuit");
    let until = w.l.claims[&id].liability_until.unwrap();
    assert_eq!(until, final_daa + class_liability, "the class's derived horizon");
    // The filing's chunks land after localization, assembly and inclusion.
    let file_at = last_served + clock.prove_daa + (filing_chunks - 1) * ROUTE_DAA_PER_CHUNK_V1;
    assert!(file_at <= until, "the filing lands inside the horizon: {file_at} <= {until}");
    w.beat_to(file_at - 1);
    let ev = w.block(vec![obj(OUT, O::FileProof { accuser: OUT, claim: id, proof: ProsecutionV1::Segmented(filing) })]);
    assert!(ev.iter().any(|e| matches!(e, E::Convicted { claim, accuser, .. } if *claim == id && *accuser == OUT)), "{ev:?}");
    // Exactly the derived schedule.
    assert_eq!(
        (final_daa - c, last_demand - c, file_at - c, until - final_daa),
        (plan.final_daa, plan.last_demand_daa, plan.filed_daa, plan.liability_needed_daa.max(l.liability_daa))
    );
    // At the policy's own horizon the last round's whole path would not have fit.
    assert!(last_demand + window + l.proof_grace_daa > final_daa + l.liability_daa);
    eprintln!(
        "[g14b] wide128 @8192 on the ledger: {rounds} rounds, Final +{}, last demand +{}, served +{}, filed +{} <= horizon +{} (class \
         liability {class_liability}, policy {})",
        final_daa - c,
        last_demand - c,
        last_served - c,
        file_at - c,
        until - c,
        l.liability_daa
    );
}

/// **G14-B, the maximal response's terminal**: a position of two parts (a full 1 MiB part and the rest) is demanded; its window counts
/// the route chunks of both parts, not the base deadline. A producer that sends one chunk a block, starting as late as it can, lands
/// the last part on the window's last DAA and is SERVED; under the old fixed window it would have defaulted by size alone. A producer
/// that withholds still defaults at the counted deadline: the declared layout fixes the window, the producer's answer cannot move it.
#[test]
fn g14b_a_maximal_response_lands_inside_its_chunk_counted_window_and_withholding_still_defaults() {
    use misaka_palw_kernel::budget::*;
    let (program, params) = fat_v1(3);
    let mut w = W::new(program, params, 64);
    let l = policy();
    let prompt: Vec<u32> = vec![3, 9, 14];
    let job = w.tiled_job(&prompt, 2);
    let honest = produce(&w, job, &prompt, None);
    commit(&mut w, &honest);
    let id = honest.claim.id();
    let sizes = misaka_palw_kernel::seg_da::position_part_bytes_v1(&w.program, 1).unwrap();
    let parts: Vec<Vec<u8>> = (0..sizes.len() as u32)
        .map(|i| borsh::to_vec(&position_part_v1(&w.program, 1, &honest.values[1], honest.c.position_path(1).1, i).unwrap()).unwrap())
        .collect();
    let per_part: Vec<u64> =
        parts.iter().map(|b| route_object_chunks_v1(b.len() as u128 + RESPOND_ENVELOPE_BYTES_V1 as u128)).collect();
    for (actual, bound) in parts.iter().zip(&sizes) {
        assert!(
            actual.len() as u64 <= *bound && *bound <= misaka_palw_kernel::seg_da::SEG_PART_BYTES_V4,
            "the layout's bound covers the part: {} <= {bound}",
            actual.len()
        );
    }
    let (chunks, n) = seg_position_chunks_v1(&w.program, 1).unwrap();
    assert_eq!(n as usize, parts.len());
    assert!(per_part.iter().sum::<u64>() <= chunks, "the declared count bounds the real chunks: {per_part:?} vs {chunks}");
    assert_eq!(per_part[0], 11, "a full part is eleven chunks: {per_part:?}");
    let window = response_window_daa_v1(l.court_deadline_daa, chunks);
    assert_eq!(w.l.demand_window_daa_v1(&id, 1), window);
    let ev = w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: id, stage: 0, position: 1 })]);
    let opened = w.daa;
    assert!(ev.iter().any(|e| matches!(e, E::DemandOpened { position: 1, deadline, .. } if *deadline == opened + window)), "{ev:?}");
    // One chunk a block, from as late as possible: part i lands when its last chunk does.
    let mut land = opened + window;
    let mut at: Vec<u64> = vec![0; parts.len()];
    for i in (0..parts.len()).rev() {
        at[i] = land;
        land -= per_part[i];
    }
    assert!(at[parts.len() - 1] > opened + l.court_deadline_daa, "the old fixed window would have defaulted this producer");
    for (i, bytes) in parts.iter().enumerate() {
        w.beat_to(at[i] - 1);
        let ev = w.block(vec![obj(PROD, O::Respond { claim: id, stage: 0, position: 1, bytes: bytes.clone() })]);
        assert!(refusal(&ev).is_none() && !ev.iter().any(|e| matches!(e, E::ProducerDefault { .. })), "part {i}: {ev:?}");
        assert_eq!(ev.iter().any(|e| matches!(e, E::Served { position: 1, .. })), i + 1 == parts.len(), "part {i}: {ev:?}");
    }
    assert!(!w.l.claims[&id].convicted && !matches!(w.l.claims[&id].life.state, ClaimStateV1::Unavailable { .. }));
    // The same position on a second claim, withheld: the default comes at the counted deadline, not later.
    let job2 = w.tiled_job(&[4, 1, 15], 2);
    let other = produce(&w, job2, &[4, 1, 15], None);
    commit(&mut w, &other);
    let id2 = other.claim.id();
    let ev = w.block(vec![obj(OUT, O::FileDemand { demander: OUT, claim: id2, stage: 0, position: 1 })]);
    assert!(refusal(&ev).is_none(), "{ev:?}");
    let opened = w.daa;
    let ev = w.beat_to(opened + window - 1);
    assert!(!ev.iter().any(|e| matches!(e, E::ProducerDefault { .. })), "no default before the counted deadline");
    let ev = w.block(Vec::new());
    assert!(ev.iter().any(|e| matches!(e, E::ProducerDefault { claim, .. } if *claim == id2)), "{ev:?}");
}

// ---- RFC-0002 (codex): conformance-only candidate classes (table 29 since the RFC02-T merge) --------------------------------------
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
