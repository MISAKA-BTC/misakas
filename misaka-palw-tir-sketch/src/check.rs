//! **The seat-local algebraic check of one job** (RFC-0007 Part II, §II.3).
//!
//! Positions in order; in each, occurrences in schedule order; in each, nodes in index order:
//!
//! * a **served** node (a `MatMul` the witness carries) is first checked against its declaration —
//!   dtype, shape at the running `H`, and every element inside its REFINED proven interval (an
//!   element outside it is a malformed witness, as an out-of-interval committed operand is a
//!   malformed commitment, PALW-TIR-33) — and then, when its turn comes, checked algebraically:
//!   against the store's sketch of its weight (per expert for a routed weight, the expert being
//!   the one the seat itself computed), or, for an activation × activation product, with a fresh
//!   vector, over every modulus its interval needs;
//! * every **other** node the check needs is recomputed exactly by the reference evaluator from
//!   values already established ([`crate::walk`]): never from the producer's say-so, and never
//!   reading a weight a sketch stands for;
//! * after each position, the committed rows the seat derived are compared with the served ones,
//!   and after the job, their root with the claim's.
//!
//! **Why that is sound.** By induction over the evaluation order: if every value an operand reads
//! is the honest one, an exactly recomputed node is honest, and a served node that passes its
//! check is honest except with probability `1/p` (`crate::geom`). So if every check passes, every
//! committed row the seat derives is the honest one except with probability at most `1/p` per
//! dishonest node — and a claim whose committed rows are not the honest ones fails the last
//! comparison. Nothing here trusts a served committed row: they are compared, never read.
//!
//! **What a failure is.** A failed check proves that the WITNESS is wrong, not that the claim is:
//! a producer can serve a bad witness for an honest claim. So a failure is never a verdict. The
//! seat files no `Valid` (ADR-0098 Decision 2) and escalates by recomputing exactly from the first
//! failure until the first committed row it disagrees with — the named leaf the exact court tries
//! (ADR-0111), unchanged (RFC-0007 Part II, §II.8). [`TirCheckFaultV1::CommitMismatch`] already is
//! that leaf.

use std::collections::BTreeMap;

use misaka_palw_tir::{Interpreter, ParamSource, Prim, Tensor};
use misaka_palw_tir_exec::TirPlan;
use misaka_palw_tir_exec::plan::BlockPlan;

use crate::analysis::{TirCheckPolicyV1, TirMatMulKindV1, TirSideV1, TirSketchAnalysisV1, TirWeightSourceV1};
use crate::field::{TirSketchModulusV1, tir_sketch_moduli_for_span_v1};
use crate::geom::TirCheckGeomV1;
use crate::secret::TirSketchKeysV1;
use crate::sketch::TirSketchStoreV1;
use crate::walk::{OccCtxV1, RunStateV1, decode_select, eval_node, ref_value};
use crate::witness::{TirCommitRowV1, TirSketchJobV1, TirWitnessStepV1, TirWitnessV1, h_at, tir_commit_root_v1};

/// Why a check failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirCheckFaultV1 {
    /// The witness does not describe the job (positions, prompt, the selected tokens' count).
    Job(String),
    /// A served value is missing, unexpected, or not of its node's type.
    WitnessShape(String),
    /// A served element outside its node's refined proven interval.
    WitnessOutOfRange { lo: i128, hi: i128, value: i128 },
    /// A served `MatMul` output fails its check over `modulus`.
    Freivalds { modulus: u64 },
    /// An exact recompute failed (it cannot on honest values of an admitted program).
    Recompute(String),
    /// The first committed row the seat derives differently: the named leaf of an accusation.
    CommitMismatch { slot: u32 },
    /// The committed rows' root differs from the claim's.
    CommitRoot,
    /// A selected token is not the `argmax` of its logits.
    Token { selected: u32, claimed: u32 },
}

/// Where a check failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirCheckFailureV1 {
    pub pos: u32,
    pub occurrence: u16,
    pub node: Option<u16>,
    pub fault: TirCheckFaultV1,
    /// For a weight `MatMul` that failed its check: the free-axis blocks whose own check fails (§II.8), so a seat fetches only those
    /// blocks' `block_fetch_bytes` of weight instead of the whole site. Empty for every other fault.
    pub blocks: Vec<u32>,
}

/// What a passed check did (the measurement's per-job counts).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TirCheckReportV1 {
    pub positions: u32,
    /// Served `MatMul` outputs: elements, and bytes at their declared width.
    pub served_elements: u64,
    pub served_bytes: u64,
    /// `MatMul` instances checked against a sketch, and with a fresh vector.
    pub weight_checks: u64,
    pub fresh_checks: u64,
    /// Field multiply-adds of every check (a fresh sketch included).
    pub check_terms: u64,
    /// Elements recomputed exactly, and the multiply-adds of the `MatMul`s among them.
    pub exact_elements: u64,
    pub exact_macs: u64,
    /// Multiply-adds a recompute of the served `MatMul`s would have cost.
    pub avoided_macs: u64,
    /// Checks over one, two and three moduli.
    pub checks_by_moduli: [u64; 3],
    /// The root of the committed rows the seat derived.
    pub commit_root: [u8; 32],
}

/// **A seat's checker for one class in one epoch.**
pub struct TirSketchCheckerV1<'a> {
    plan: &'a TirPlan,
    interp: Interpreter<'a>,
    analysis: &'a TirSketchAnalysisV1,
    store: &'a TirSketchStoreV1,
    keys: &'a TirSketchKeysV1,
    params: &'a dyn ParamSource,
    occ_plans: Vec<BlockPlan>,
    policy: TirCheckPolicyV1,
    fresh_moduli: Option<Vec<TirSketchModulusV1>>,
}

type Fail = Box<TirCheckFailureV1>;

impl<'a> TirSketchCheckerV1<'a> {
    /// `params` is what the seat holds: [`TirSketchAnalysisV1::held_params`] is enough.
    pub fn new(
        plan: &'a TirPlan,
        analysis: &'a TirSketchAnalysisV1,
        store: &'a TirSketchStoreV1,
        keys: &'a TirSketchKeysV1,
        params: &'a dyn ParamSource,
        policy: TirCheckPolicyV1,
    ) -> misaka_palw_tir::TirResult<Self> {
        Ok(Self {
            plan,
            interp: Interpreter::new(&plan.program)?,
            analysis,
            store,
            keys,
            params,
            occ_plans: store.refined_plans(plan),
            policy,
            fresh_moduli: None,
        })
    }

    /// Check every fresh product over `moduli` whatever its interval — the tests' twin of
    /// [`TirSketchStoreV1::build_with`]. No seat does this.
    #[doc(hidden)]
    pub fn with_fresh_moduli(mut self, moduli: &[TirSketchModulusV1]) -> Self {
        self.fresh_moduli = Some(moduli.to_vec());
        self
    }

    /// **Check `witness` as the execution of `job`** (module note). `job_id` keys the fresh vectors.
    pub fn check(
        &self,
        job: &TirSketchJobV1,
        job_id: &[u8; 32],
        witness: &TirWitnessV1,
    ) -> Result<TirCheckReportV1, Box<TirCheckFailureV1>> {
        let p = &self.plan.program;
        let fail = |pos: u32, occurrence: u16, node: Option<u16>, fault: TirCheckFaultV1| -> Fail {
            Box::new(TirCheckFailureV1 { pos, occurrence, node, fault, blocks: Vec::new() })
        };
        let positions = job.positions();
        let prompt_len = job.prompt.len() as u32;
        if witness.prompt_len != prompt_len
            || witness.steps.len() != positions as usize
            || witness.tokens.len() != positions as usize
            || witness.generated.len() != job.decode.max(1) as usize
            || witness.tokens[..prompt_len as usize] != job.prompt[..]
        {
            return Err(fail(0, 0, None, TirCheckFaultV1::Job("the witness is not of this job".into())));
        }
        let mut report = TirCheckReportV1 { positions, ..Default::default() };
        let mut run = RunStateV1::default();
        let mut derived: Vec<TirWitnessStepV1> = Vec::with_capacity(positions as usize);
        let post_occ = self.plan.occurrences.len() - 1;
        for (pos, step) in (0..positions).zip(&witness.steps) {
            if step.pos != pos {
                return Err(fail(pos, 0, None, TirCheckFaultV1::Job(format!("step {} is at position {pos}", step.pos))));
            }
            let token = witness.tokens[pos as usize];
            let mut served: BTreeMap<(u16, u16), &Tensor> = BTreeMap::new();
            for v in &step.values {
                if served.insert((v.occurrence, v.node), &v.value).is_some() {
                    return Err(fail(pos, v.occurrence, Some(v.node), TirCheckFaultV1::WitnessShape("served twice".into())));
                }
            }
            let mut commits: Vec<TirCommitRowV1> = Vec::new();
            let (mut writes, mut appends) = (Vec::new(), Vec::new());
            let mut carry: Vec<Tensor> = Vec::new();
            for (occ, &(block, layer)) in self.plan.occurrences.iter().enumerate() {
                let o = occ as u16;
                if occ == post_occ && pos + 1 < prompt_len {
                    break;
                }
                let b = &p.blocks[block as usize];
                let h = h_at(self.plan, block, pos);
                let needed = self.analysis.checker_needed(p, block, h, &self.policy);
                let mut values: Vec<Option<Tensor>> = vec![None; b.nodes.len()];
                let mut is_served = vec![false; b.nodes.len()];
                for (ni, node) in b.nodes.iter().enumerate() {
                    if !self.analysis.witnessed(p, block, ni as u16, h, &self.policy) {
                        continue;
                    }
                    let t = served.remove(&(o, ni as u16)).ok_or_else(|| {
                        fail(pos, o, Some(ni as u16), TirCheckFaultV1::WitnessShape("a served value is missing".into()))
                    })?;
                    if t.dtype != node.out.dtype || t.shape != node.out.resolve(h) || t.data.len() != t.shape.iter().product::<usize>()
                    {
                        return Err(fail(pos, o, Some(ni as u16), TirCheckFaultV1::WitnessShape("not of its node's type".into())));
                    }
                    let iv = self.occ_plans[occ].nodes[ni].facts.out;
                    if let Some(v) = t.data.iter().find(|v| !iv.contains(**v)) {
                        return Err(fail(
                            pos,
                            o,
                            Some(ni as u16),
                            TirCheckFaultV1::WitnessOutOfRange { lo: iv.lo, hi: iv.hi, value: *v },
                        ));
                    }
                    report.served_elements += t.data.len() as u64;
                    report.served_bytes += (t.data.len() * t.dtype.width()) as u64;
                    values[ni] = Some(t.clone());
                    is_served[ni] = true;
                }
                let ctx = OccCtxV1 { pos, token, block, layer, carry: &carry };
                for ni in 0..b.nodes.len() {
                    if is_served[ni] {
                        self.check_matmul(&ctx, o, ni, h, job_id, &run, &values, &mut report)?;
                        continue;
                    }
                    if !needed[ni] {
                        continue;
                    }
                    let v = eval_node(&self.interp, self.params, &ctx, &run, &values, ni)
                        .map_err(|e| fail(pos, o, Some(ni as u16), TirCheckFaultV1::Recompute(e.to_string())))?;
                    report.exact_elements += v.data.len() as u64;
                    if b.nodes[ni].prim == Prim::MatMul {
                        let np = &self.occ_plans[occ].nodes[ni];
                        let k = np.in_types[0].resolve(h).last().copied().unwrap_or(1);
                        report.exact_macs += (v.data.len() * k) as u64;
                    }
                    values[ni] = Some(v);
                }
                let base = self.plan.slot_bases[occ];
                for (ni, node) in b.nodes.iter().enumerate() {
                    let value = || {
                        values[ni]
                            .clone()
                            .ok_or_else(|| fail(pos, o, Some(ni as u16), TirCheckFaultV1::Recompute("not derived".into())))
                    };
                    if node.commit {
                        commits.push(TirCommitRowV1 { slot: base + ni as u32, value: value()? });
                    }
                    match node.prim {
                        Prim::StateWrite { state } => writes.push((state, layer, value()?)),
                        Prim::HistAppend { state } => {
                            let row = ref_value(&self.interp, self.params, &ctx, &run, &values, node.inputs[0])
                                .map_err(|e| fail(pos, o, Some(ni as u16), TirCheckFaultV1::Recompute(e.to_string())))?;
                            appends.push((state, layer, row));
                        }
                        _ => {}
                    }
                }
                if occ == post_occ {
                    let logits = values[p.logits as usize].as_ref().expect("the logits are a root");
                    let selected = decode_select(logits);
                    let i = (pos + 1 - prompt_len) as usize;
                    let claimed = witness.generated[i];
                    let next = witness.tokens.get(pos as usize + 1).copied().unwrap_or(claimed);
                    if selected != claimed || next != claimed {
                        return Err(fail(pos, o, Some(p.logits), TirCheckFaultV1::Token { selected, claimed }));
                    }
                } else {
                    carry = b.carry_out.iter().map(|c| values[*c as usize].clone().expect("a carry-out is a root")).collect();
                }
            }
            if let Some(((o, n), _)) = served.into_iter().next() {
                return Err(fail(pos, o, Some(n), TirCheckFaultV1::WitnessShape("a value no check reads".into())));
            }
            // The committed rows, compared in slot order: the first difference is the named leaf.
            for (mine, theirs) in commits.iter().zip(&step.commits) {
                if mine != theirs {
                    let occ = self.plan.slot_bases.iter().rposition(|b| *b <= mine.slot).unwrap_or(0) as u16;
                    return Err(fail(pos, occ, None, TirCheckFaultV1::CommitMismatch { slot: mine.slot.min(theirs.slot) }));
                }
            }
            if commits.len() != step.commits.len() {
                return Err(fail(pos, 0, None, TirCheckFaultV1::WitnessShape("a different number of committed rows".into())));
            }
            run.apply(p, writes, appends);
            derived.push(TirWitnessStepV1 { pos, values: Vec::new(), commits });
        }
        report.commit_root = tir_commit_root_v1(&derived);
        if report.commit_root != witness.commit_root {
            return Err(fail(positions.saturating_sub(1), 0, None, TirCheckFaultV1::CommitRoot));
        }
        Ok(report)
    }

    /// The algebraic check of served node `ni` (module note).
    #[allow(clippy::too_many_arguments)]
    fn check_matmul(
        &self,
        ctx: &OccCtxV1<'_>,
        occ: u16,
        ni: usize,
        h: usize,
        job_id: &[u8; 32],
        run: &RunStateV1,
        values: &[Option<Tensor>],
        report: &mut TirCheckReportV1,
    ) -> Result<(), Fail> {
        let p = &self.plan.program;
        let node = &p.blocks[ctx.block as usize].nodes[ni];
        let np = &self.occ_plans[occ as usize].nodes[ni];
        let fail =
            |fault: TirCheckFaultV1| Box::new(TirCheckFailureV1 { pos: ctx.pos, occurrence: occ, node: Some(ni as u16), fault, blocks: Vec::new() });
        let operand =
            |r| ref_value(&self.interp, self.params, ctx, run, values, r).map_err(|e| fail(TirCheckFaultV1::Recompute(e.to_string())));
        let out = values[ni].as_ref().expect("served");
        let site = self.analysis.site(ctx.block, ni as u16).expect("a served node is a MatMul site");
        match site.kind {
            TirMatMulKindV1::Weight { side, source } => {
                let sk = self
                    .store
                    .get(occ, ni as u16)
                    .ok_or_else(|| fail(TirCheckFaultV1::WitnessShape("no sketch for this site".into())))?;
                let routed_rank = match source {
                    TirWeightSourceV1::Routed { idx_rank, .. } => idx_rank as usize,
                    TirWeightSourceV1::Static(_) => 0,
                };
                let g = TirCheckGeomV1::new(side, &np.in_types[0], &np.in_types[1], h, routed_rank);
                let x = operand(if side == TirSideV1::Right { node.inputs[0] } else { node.inputs[1] })?;
                let idx = match source {
                    TirWeightSourceV1::Routed { idx, .. } => Some(operand(idx)?),
                    TirWeightSourceV1::Static(_) => None,
                };
                for (mi, md) in sk.moduli.iter().enumerate() {
                    let lhs = g.lhs(&out.data, &sk.v[mi], *md);
                    let s_all = &sk.s[mi];
                    let mut sketch_of = |beta: &[usize]| -> Option<&[u64]> {
                        let e = match &idx {
                            None => 0usize,
                            Some(t) => {
                                let coords = g.routed_coords(beta);
                                let flat = t.shape.iter().zip(coords).fold(0usize, |acc, (e, c)| acc * e + c);
                                usize::try_from(*t.data.get(flat)?).ok()?
                            }
                        };
                        (e < sk.experts).then(|| &s_all[e * sk.s_len..(e + 1) * sk.s_len])
                    };
                    let rhs = g
                        .rhs(&x.data, &mut sketch_of, *md)
                        .ok_or_else(|| fail(TirCheckFaultV1::Recompute("a routed index names no expert".into())))?;
                    report.check_terms += g.check_terms();
                    if lhs != rhs {
                        let mut failure = fail(TirCheckFaultV1::Freivalds { modulus: md.p() });
                        // §II.8: name the failing blocks (each is a check of its own with the vector masked to it).
                        for b in 0..sk.blocks {
                            let Some(sb) = sk.block_sketch(mi, b) else { continue };
                            let vb = if sk.blocks > 1 { g.mask_block(&sk.v[mi], sk.blocks, b) } else { sk.v[mi].clone() };
                            let lhs_b = g.lhs(&out.data, &vb, *md);
                            let mut block_of = |beta: &[usize]| -> Option<&[u64]> {
                                let e = match &idx {
                                    None => 0usize,
                                    Some(t) => {
                                        let coords = g.routed_coords(beta);
                                        let flat = t.shape.iter().zip(coords).fold(0usize, |acc, (e, c)| acc * e + c);
                                        usize::try_from(*t.data.get(flat)?).ok()?
                                    }
                                };
                                (e < sk.experts).then(|| &sb[e * sk.s_len..(e + 1) * sk.s_len])
                            };
                            if g.rhs(&x.data, &mut block_of, *md).is_none_or(|r| r != lhs_b) {
                                failure.blocks.push(b as u32);
                            }
                        }
                        return Err(failure);
                    }
                }
                report.weight_checks += 1;
                report.checks_by_moduli[sk.moduli.len() - 1] += 1;
                report.avoided_macs += g.recompute_macs();
            }
            TirMatMulKindV1::ActAct => {
                let mods = match &self.fresh_moduli {
                    Some(m) => m.clone(),
                    None => tir_sketch_moduli_for_span_v1(np.facts.out.hi.abs_diff(np.facts.out.lo)),
                };
                let g = TirCheckGeomV1::new(TirSideV1::Right, &np.in_types[0], &np.in_types[1], h, 0);
                let (a, b) = (operand(node.inputs[0])?, operand(node.inputs[1])?);
                for md in &mods {
                    let v = self.keys.fresh_vector(job_id, ctx.pos, occ, ni as u16, *md, g.v_len());
                    let s = g.sketch(&b.data, &v, *md);
                    let lhs = g.lhs(&out.data, &v, *md);
                    let rhs = g.rhs(&a.data, &mut |_| Some(&s[..]), *md).expect("an unrouted sketch");
                    report.check_terms += g.check_terms() + g.body_len() as u64;
                    if lhs != rhs {
                        return Err(fail(TirCheckFaultV1::Freivalds { modulus: md.p() }));
                    }
                }
                report.fresh_checks += 1;
                report.checks_by_moduli[mods.len() - 1] += 1;
                report.avoided_macs += g.recompute_macs();
            }
            TirMatMulKindV1::Exact => unreachable!("an exact MatMul is never served"),
        }
        Ok(())
    }
}
