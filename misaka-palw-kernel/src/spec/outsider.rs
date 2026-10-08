//! **A fresh outsider for typed claims** — built from the ledger (rebuilt from the rows a node serves), a public DA directory, the
//! public artifact and whatever public copy of a snapshot it can fetch, with its own salt. It reports exactly what it must demand
//! (one round), or the filing that convicts, or `Clean`.
//!
//! * **memory**: the pre-state (stage `0x40`) and every step position (stage 0); then, step by step, the decode and every relation of
//!   the step's record over its overlay — the first fault is filed as `MemoryDecode` / `MemoryStep` naming the step;
//! * **retrieval**: every snapshot slice (served on chain, else the public copy; missing slices are demanded); then every entry
//!   re-opened, then a scan for a better excluded item;
//! * **composite**: the `StageLogits` edges, then each stage in order with its component's check; the first fault names its stage.

use std::collections::BTreeMap;

use misaka_palw_tir::Tensor;

use crate::hash::Digest;
use crate::job::DecodeRuleV1;
use crate::ledger::{ClaimBodyV1, ClassRowV1, KernelLedgerV1, OutsiderFindingV1, ProsecutionV1, PublicArtifactV1, PublicSourceV1};
use crate::public::{FaultProofWireV1, FreshVerifierV1, PublicClaimRecordV1, TensorWireV1};
use crate::trace::{ParamCommitmentsV1, derived_mask_v1, tensor_commitment};
use crate::verify::{MaterialV1, ScopeV1, ScopeVerdictV1};

use super::composite::{
    ComponentV1, QuerySourceV1, StageClaimV1, StageInputV1, model_stage_record_v1, stage_claim_v1, stage_job_v1, stage_prompt_v1,
};
use super::memory::{boundary_states_v1, overlay_v1, step_header_v1, step_offsets_v1, step_record_v1};
use super::retrieval::{RetrievalItemV1, RetrievalRootV1, SnapshotDataV1, items_of_served_v1};
use super::{MEMORY_PRE_STATE_STAGE_V1, SNAPSHOT_STAGE_BASE_V1, SpecClaimV1, SpecClassKindV1, SpecFaultV1, SpecJobV1};

/// **Any public copy of a snapshot's items** (a mirror, a DA provider). Whatever it returns is checked against the class's snapshot root.
pub trait SnapshotSourceV1 {
    fn item(&self, snapshot_root: &Digest, id: u64) -> Option<RetrievalItemV1>;
}

/// No public copy at all: every slice must come from the chain.
impl SnapshotSourceV1 for () {
    fn item(&self, _: &Digest, _: u64) -> Option<RetrievalItemV1> {
        None
    }
}

/// Whole snapshots, by snapshot root.
impl SnapshotSourceV1 for BTreeMap<Digest, SnapshotDataV1> {
    fn item(&self, snapshot_root: &Digest, id: u64) -> Option<RetrievalItemV1> {
        self.get(snapshot_root)?.items.get(id as usize).cloned()
    }
}

/// A fresh outsider's view of one typed claim.
pub struct SpecOutsiderV1<'a> {
    pub ledger: &'a KernelLedgerV1,
    pub claim: Digest,
    pub material: &'a dyn PublicSourceV1,
    /// The public artifacts: program 0 of a memory class (its rule's base weights), program `s` of a composite's model stage `s`.
    pub artifact: &'a dyn PublicArtifactV1,
    pub snapshots: &'a dyn SnapshotSourceV1,
    pub salt: Digest,
}

/// One kernel record's material: node values of one demand stage (from `offset`), params authenticated against `commitments`.
struct KernelMaterial<'b> {
    o: &'b SpecOutsiderV1<'b>,
    stage: u8,
    offset: u32,
    program: u16,
    commitments: &'b ParamCommitmentsV1,
    /// Memory: the slot params' values (the pre-state of the step).
    slots: BTreeMap<(u16, Option<u16>), Tensor>,
}

impl MaterialV1 for KernelMaterial<'_> {
    fn node_value(&self, p: u32, s: u16, n: u16) -> Option<Tensor> {
        self.o.node(self.stage, self.offset + p, s, n)
    }
    fn param(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        let want = self.commitments.by_instance.get(&(index, layer))?;
        let t = match self.slots.get(&(index, layer)) {
            Some(t) => t.clone(),
            None => self.o.artifact.param(self.program, index, layer)?,
        };
        (tensor_commitment(&t) == *want).then_some(t)
    }
}

fn spec(fault: SpecFaultV1) -> OutsiderFindingV1 {
    OutsiderFindingV1::Prosecute(ProsecutionV1::Spec(fault.to_bytes()))
}

impl SpecOutsiderV1<'_> {
    /// A committed value: served on chain first, then the public source.
    fn node(&self, stage: u8, p: u32, s: u16, n: u16) -> Option<Tensor> {
        if let Some(sp) = self.ledger.served.get(&(self.claim, stage, p)) {
            return sp.values.get(s as usize)?.get(n as usize)?.as_ref()?.decode().ok();
        }
        self.material.node(stage, p, s, n)
    }

    /// The positions of a kernel part any non-derived committed value of which nobody serves.
    fn missing_positions(
        &self,
        stage: u8,
        offset: u32,
        program: &misaka_palw_tir::program::TirProgramV1,
        commitments: &[Vec<Vec<Digest>>],
    ) -> Vec<(u8, u32)> {
        let mask = derived_mask_v1(program);
        let mut out = Vec::new();
        for (p, pos) in commitments.iter().enumerate() {
            let g = offset + p as u32;
            let ok = pos.iter().enumerate().all(|(s, occ)| {
                occ.iter().enumerate().all(|(n, c)| {
                    mask.get(s).and_then(|m| m.get(n)).copied().unwrap_or(false)
                        || self.node(stage, g, s as u16, n as u16).is_some_and(|t| tensor_commitment(&t) == *c)
                })
            });
            if !ok {
                out.push((stage, g));
            }
        }
        out
    }

    /// One kernel record checked with the outsider's salt: `None` (clean) or the fault proof's bytes.
    fn check_record(
        &self,
        record: PublicClaimRecordV1,
        header: crate::evidence::EvidenceHeaderV1,
        mat: &KernelMaterial<'_>,
    ) -> Result<Option<Vec<u8>>, String> {
        let fresh = FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.ledger.known, header)?;
        match fresh.check_salted(mat, &ScopeV1::WholeClaim, self.salt) {
            ScopeVerdictV1::Pass { .. } => Ok(None),
            ScopeVerdictV1::Fault(p) => Ok(Some(FaultProofWireV1::of(&p).to_bytes())),
            ScopeVerdictV1::Unavailable { what } => Err(format!("unavailable with every value in hand: {what}")),
            ScopeVerdictV1::EvidenceMalformed { why } => Err(format!("malformed evidence on chain: {why}")),
            ScopeVerdictV1::Inconsistent { why } => Err(format!("verifier inconsistency: {why}")),
        }
    }

    /// The whole snapshot of demand stage `stage`, or the slices nobody serves.
    fn snapshot(&self, stage: u8, root: &RetrievalRootV1) -> Result<Result<SnapshotDataV1, Vec<(u8, u32)>>, String> {
        let s = &root.snapshot;
        let mut items = Vec::with_capacity(s.items as usize);
        let mut missing = Vec::new();
        let snapshot_root = s.root();
        for t in 0..s.slices() {
            if let Some(sp) = self.ledger.served.get(&(self.claim, stage, t as u32)) {
                items.extend(items_of_served_v1(sp).ok_or("a served slice does not decode")?);
                continue;
            }
            let range = s.slice_range(t).expect("a slice of the snapshot");
            let got: Option<Vec<RetrievalItemV1>> = range.map(|id| self.snapshots.item(&snapshot_root, id)).collect();
            match got {
                Some(v) => items.extend(v),
                None => missing.push((stage, t as u32)),
            }
        }
        if !missing.is_empty() {
            return Ok(Err(missing));
        }
        let data = SnapshotDataV1::new(items, s.dim, s.max_payload, s.slice_items)?;
        if data.snapshot != *s {
            return Err("the snapshot copy is not the class's snapshot (its root differs)".into());
        }
        Ok(Ok(data))
    }

    /// **Check the claim.**
    pub fn check(&self) -> Result<OutsiderFindingV1, String> {
        let l = self.ledger;
        let row = l.claims.get(&self.claim).ok_or("no such claim")?;
        let ClaimBodyV1::Spec(body) = &row.body else { return Err("not a typed claim".into()) };
        let class = l.typed.classes.get(&row.class_binding_id).ok_or("no such class")?;
        let job = l.typed.jobs.get(&row.job_id).ok_or("no such job")?;
        match (&body.claim, job, &class.kind) {
            (SpecClaimV1::Memory(c), SpecJobV1::Memory(j), SpecClassKindV1::Memory { rule, root, writers }) => {
                let pre = body.pre_state.first().ok_or("no pre-state")?;
                let mut missing = Vec::new();
                let pre_values: Vec<Option<Tensor>> = (0..root.slots.len())
                    .map(|k| self.node(MEMORY_PRE_STATE_STAGE_V1, 0, 0, k as u16).filter(|t| tensor_commitment(t) == pre[k]))
                    .collect();
                if pre_values.iter().any(Option::is_none) {
                    missing.push((MEMORY_PRE_STATE_STAGE_V1, 0));
                }
                let offsets = step_offsets_v1(&c.steps);
                for (i, st) in c.steps.iter().enumerate() {
                    missing.extend(self.missing_positions(0, offsets[i], &rule.program, &st.commitments));
                }
                if !missing.is_empty() {
                    return Ok(OutsiderFindingV1::Demand(missing));
                }
                let states = boundary_states_v1(writers, pre, &c.steps).ok_or("the boundary states do not derive")?;
                let (post, logits) = rule.logits_at();
                for (i, st) in c.steps.iter().enumerate() {
                    let last = offsets[i + 1] - 1;
                    let t = self.node(0, last, post, logits).ok_or("the step's logits vanished")?;
                    if DecodeRuleV1::Greedy.select(&t) != Some(c.generated[i]) {
                        return Ok(spec(SpecFaultV1::MemoryDecode { step: i as u32, logits: TensorWireV1::of(&t) }));
                    }
                    let overlay = overlay_v1(&rule.param_commitments, &root.slots, &states[i]);
                    // The step's pre-state values: the claim's pre-state for step 0, else step i−1's committed writes.
                    let slots = root
                        .slots
                        .iter()
                        .zip(writers)
                        .enumerate()
                        .map(|(k, (slot, (ws, wn)))| {
                            let v = if i == 0 { pre_values[k].clone() } else { self.node(0, offsets[i] - 1, *ws, *wn) };
                            v.map(|v| (slot.param, v)).ok_or("a slot's pre-state vanished")
                        })
                        .collect::<Result<BTreeMap<_, _>, _>>()?;
                    let mat = KernelMaterial { o: self, stage: 0, offset: offsets[i], program: 0, commitments: &overlay, slots };
                    let record = step_record_v1(&self.claim, i as u32, rule, st, &overlay, &j.chunks[i]);
                    if let Some(proof) = self.check_record(record, step_header_v1(rule, &row.class_binding_id, &overlay), &mat)? {
                        return Ok(spec(SpecFaultV1::MemoryStep { step: i as u32, proof }));
                    }
                }
                Ok(OutsiderFindingV1::Clean)
            }
            (SpecClaimV1::Retrieval(c), SpecJobV1::Retrieval(j), SpecClassKindV1::Retrieval { root }) => {
                match self.snapshot(SNAPSHOT_STAGE_BASE_V1, root)? {
                    Err(missing) => Ok(OutsiderFindingV1::Demand(missing)),
                    Ok(data) => Ok(match data.find_fault(root, &j.query, &c.result) {
                        Some(fault) => spec(SpecFaultV1::Retrieval { stage: 0, fault }),
                        None => OutsiderFindingV1::Clean,
                    }),
                }
            }
            (SpecClaimV1::Composite(c), SpecJobV1::Composite(j), SpecClassKindV1::Composite { root, components }) => {
                let mut missing = Vec::new();
                let mut snapshots = BTreeMap::new();
                for (s, (st, comp)) in c.stages.iter().zip(components).enumerate() {
                    match (st, comp) {
                        (StageClaimV1::Model { commitments, .. }, ComponentV1::Model(m)) => {
                            missing.extend(self.missing_positions(s as u8, 0, &m.program, commitments))
                        }
                        (StageClaimV1::Retrieval { .. }, ComponentV1::Retrieval(r)) => {
                            match self.snapshot(SNAPSHOT_STAGE_BASE_V1 + s as u8, r)? {
                                Ok(data) => {
                                    snapshots.insert(s, data);
                                }
                                Err(m) => missing.extend(m),
                            }
                        }
                        _ => return Err("a claim stage of another kind than its class stage".into()),
                    }
                }
                if !missing.is_empty() {
                    return Ok(OutsiderFindingV1::Demand(missing));
                }
                for (s, ((cs, st), comp)) in root.stages.iter().zip(&c.stages).zip(components).enumerate() {
                    match (st, comp) {
                        (StageClaimV1::Retrieval { query, result, .. }, ComponentV1::Retrieval(r)) => {
                            if let StageInputV1::Query(QuerySourceV1::StageLogits { stage: t }) = cs.input {
                                let (StageClaimV1::Model { commitments, .. }, ComponentV1::Model(m)) =
                                    (&c.stages[t as usize], &components[t as usize])
                                else {
                                    return Err("the upstream stage is not a model".into());
                                };
                                let (post, node) = m.logits_at();
                                let logits =
                                    self.node(t, commitments.len() as u32 - 1, post, node).ok_or("the upstream logits vanished")?;
                                let same =
                                    logits.data.len() == query.len() && logits.data.iter().zip(query).all(|(a, b)| *a == *b as i128);
                                if !same {
                                    return Ok(spec(SpecFaultV1::Edge { stage: s as u8, logits: TensorWireV1::of(&logits) }));
                                }
                            }
                            if let Some(fault) = snapshots[&s].find_fault(r, query, result) {
                                return Ok(spec(SpecFaultV1::Retrieval { stage: s as u8, fault }));
                            }
                        }
                        (StageClaimV1::Model { generated, evidence, commitments }, ComponentV1::Model(m)) => {
                            let StageInputV1::Tokens { sources, max_new_tokens } = &cs.input else {
                                return Err("not a model stage".into());
                            };
                            let prompt = stage_prompt_v1(sources, j, &c.stages).ok_or("a source of another kind")?;
                            let sub_job = stage_job_v1(&cs.component, &row.job_id, s as u8, prompt, *max_new_tokens);
                            let sub_claim = stage_claim_v1(&sub_job, &row.producer, generated, evidence);
                            let (post, logits) = m.logits_at();
                            for r in 0..generated.len() {
                                let p = sub_claim.select_position(&sub_job, r);
                                let t = self.node(s as u8, p, post, logits).ok_or("a stage's logits vanished")?;
                                if sub_job.decode.select(&t) != Some(generated[r]) {
                                    return Ok(spec(SpecFaultV1::StageDecode {
                                        stage: s as u8,
                                        index: r as u32,
                                        logits: TensorWireV1::of(&t),
                                    }));
                                }
                            }
                            let record = model_stage_record_v1(&self.claim, s as u8, m, &sub_job, &sub_claim, evidence, commitments);
                            let mat = model_material(self, s as u8, m);
                            if let Some(proof) = self.check_record(record, m.header(cs.component), &mat)? {
                                return Ok(spec(SpecFaultV1::StageKernel { stage: s as u8, proof }));
                            }
                        }
                        _ => return Err("a claim stage of another kind than its class stage".into()),
                    }
                }
                Ok(OutsiderFindingV1::Clean)
            }
            _ => Err("a claim of another kind than its class".into()),
        }
    }
}

fn model_material<'b>(o: &'b SpecOutsiderV1<'b>, stage: u8, m: &'b ClassRowV1) -> KernelMaterial<'b> {
    KernelMaterial { o, stage, offset: 0, program: stage as u16, commitments: &m.param_commitments, slots: BTreeMap::new() }
}
