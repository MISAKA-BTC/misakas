//! **Public prosecution of pipeline claims** (K2-TIR-v3, Kernel design §K.3, RFC-0003 pipelines; ADR-0173) — the pipeline analogue of
//! [`crate::public::FreshVerifierV1`]: a party that holds only canonical public bytes rebuilds a pipeline claim's context, checks
//! every stage, every edge and the text stream's decode, and files a proof any node adjudicates from the same bytes.
//!
//! * **`R` is public and bound.** A pipeline's random inputs are `dist(R(seed, domain, step, item, lane))` over the JOB's seed and
//!   item ([`PipelineRandomV1`], the same draw as RFC-0003 §I.1 and `misaka-palw-gen`). The evidence's `random_binding` must be
//!   [`PipelineRandomV1::binding`] of the job's seed, so a producer can neither keep `R` private nor draw it from another seed.
//! * **The text stream's output is bound.** A pipeline with a stream stage delivers generated ids; each must be the job's decode
//!   rule applied to the stream stage's committed logits row that selects it (the row `|prompt| − 1 + r`, as the run selects it).
//!   A delivered id that is not is a [`PipelineFaultWireV1::Decode`] fault, proved by opening one logits row.
//! * **Every fault has a public court**: a stage relation ([`crate::pipeline::verify_stage_fault_v1`]), an edge
//!   ([`crate::pipeline::verify_edge_fault_v1`], `R`'s draw included) and the decode — over the record's bytes alone.

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_tir::Tensor;
use misaka_palw_tir::pipeline::{JobImageV1, PipelineJob, RandomSource, TirPipelineV1, stage_rows_from, stream_stage};
use misaka_palw_tir::program_v2::{RandomDist, TirProgramV2};
use misaka_palw_tir::types::DType;

use crate::descriptor::KernelDescriptorV1;
use crate::hash::{Digest, finish, keyed};
use crate::job::DecodeRuleV1;
use crate::pipeline::{
    EdgeFaultProofV1, PipelineContextV1, PipelineEvidenceV1, PipelineHeaderV1, PipelinePlanV1, PipelineVerdictV1, pipeline_root_v1,
    pipeline_structure_v1, stage_view_v1, verify_edge_fault_v1, verify_pipeline_v1, verify_stage_fault_v1,
};
use crate::public::{FaultProofWireV1, TensorWireV1};
use crate::trace::{EvidenceV1, ParamCommitmentsV1, tensor_commitment};
use crate::verify::MaterialV1;

pub const PIPELINE_RANDOM_BINDING_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-random/v1";
pub const PIPELINE_CLASS_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-class/v1";
/// A pipeline record's parse bound (the job's images are in it).
pub const MAX_PIPELINE_RECORD_BYTES_V1: usize = 256 << 20;

/// **`R` for a pipeline job**: the job's seed and item index (RFC-0003 §I.1.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineRandomV1 {
    pub seed: [u8; 32],
    pub item: u32,
}

impl PipelineRandomV1 {
    /// The commitment the evidence's `random_binding` must be.
    pub fn binding(&self) -> Digest {
        let mut s = keyed(PIPELINE_RANDOM_BINDING_DOMAIN_V1);
        s.update(&self.seed).update(&self.item.to_le_bytes());
        finish(s)
    }
}

impl RandomSource for PipelineRandomV1 {
    fn random(&self, domain: u16, dist: RandomDist, step: u32, shape: &[u32]) -> Option<Tensor> {
        let n: u64 = shape.iter().fold(1u64, |acc, d| acc.saturating_mul(*d as u64));
        let (d, dtype) = match dist {
            RandomDist::Uniform { .. } => (misaka_palw_gen::RandDistV1::Uniform, DType::Idx),
            RandomDist::Normal => (misaka_palw_gen::RandDistV1::Normal, DType::I32),
        };
        let v = misaka_palw_gen::rand_values_v1(domain, d, &self.seed, step, self.item, n).ok()?;
        Tensor::new(dtype, shape.iter().map(|x| *x as usize).collect(), v.into_iter().map(|x| x as i128).collect()).ok()
    }
}

/// **A pipeline job's public facts** as posted: everything [`PipelineJob`] holds but the generated ids (the claim's output),
/// plus `R`'s seed and item.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineJobFactsV1 {
    pub prompt: Vec<u32>,
    pub negative: Vec<u32>,
    pub steps: u32,
    pub scalars: Vec<i64>,
    /// `(h, w, rgb)` per image.
    pub images: Vec<(u32, u32, Vec<u8>)>,
    pub source: Vec<u32>,
    pub key: Vec<u32>,
    pub finalized: Vec<(u8, u8, Vec<u32>)>,
    pub seed: [u8; 32],
    pub item: u32,
}

impl PipelineJobFactsV1 {
    pub fn of(job: &PipelineJob, random: PipelineRandomV1) -> Self {
        Self {
            prompt: job.prompt.clone(),
            negative: job.negative.clone(),
            steps: job.steps,
            scalars: job.scalars.clone(),
            images: job.images.iter().map(|i| (i.h, i.w, i.rgb.clone())).collect(),
            source: job.source.clone(),
            key: job.key.clone(),
            finalized: job.finalized.iter().map(|((c, s), v)| (*c, *s, v.clone())).collect(),
            seed: random.seed,
            item: random.item,
        }
    }

    /// The job a claim delivering `generated` answers.
    pub fn job(&self, generated: &[u32]) -> PipelineJob {
        PipelineJob {
            prompt: self.prompt.clone(),
            negative: self.negative.clone(),
            steps: self.steps,
            scalars: self.scalars.clone(),
            images: self.images.iter().map(|(h, w, rgb)| JobImageV1 { h: *h, w: *w, rgb: rgb.clone() }).collect(),
            generated: generated.to_vec(),
            source: self.source.clone(),
            key: self.key.clone(),
            finalized: self.finalized.iter().map(|(c, s, v)| ((*c, *s), v.clone())).collect(),
        }
    }

    pub fn random(&self) -> PipelineRandomV1 {
        PipelineRandomV1 { seed: self.seed, item: self.item }
    }
}

/// One stage's committed values: node commitments `[position][occurrence][node]` and input commitments `[position][input]`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StageCommitmentsV1 {
    pub commitments: Vec<Vec<Vec<Digest>>>,
    pub inputs: Vec<Vec<Digest>>,
}

impl StageCommitmentsV1 {
    pub fn of(e: &EvidenceV1) -> Self {
        Self { commitments: e.commitments.clone(), inputs: e.inputs.clone() }
    }

    pub fn evidence(&self) -> EvidenceV1 {
        EvidenceV1 { commitments: self.commitments.clone(), inputs: self.inputs.clone() }
    }
}

/// **What a pipeline class binds**: the kernel, the pipeline (bytes and every program), the plan and each program's artifact.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineClassV1 {
    pub descriptor_digest: Digest,
    pub pipeline_root: Digest,
    pub plan_root: Digest,
    pub artifact_roots: Vec<Digest>,
    /// The decode rule a stream stage's output is selected by (`None` for a pipeline without one).
    pub decode: Option<DecodeRuleV1>,
}

impl PipelineClassV1 {
    pub fn class_binding_id(&self) -> Digest {
        crate::hash::object_id(PIPELINE_CLASS_DOMAIN_V1, self)
    }
}

pub const PIPELINE_JOB_POST_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-job-post/v1";
pub const PIPELINE_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/pipeline-claim/v1";

/// **A pipeline job, as posted on chain**: the class, its public facts (`R`'s seed among them) and the generation budget (zero for
/// a pipeline without a stream stage).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineJobPostV1 {
    pub class_binding_id: Digest,
    pub facts: PipelineJobFactsV1,
    pub max_new_tokens: u32,
    pub nonce: Digest,
}

impl PipelineJobPostV1 {
    pub fn id(&self) -> Digest {
        crate::hash::object_id(PIPELINE_JOB_POST_DOMAIN_V1, self)
    }
}

/// **A pipeline claim**: the delivered ids (a text pipeline's output), the delivered output's commitment, and the evidence root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelineClaimV1 {
    pub job_id: Digest,
    pub producer_bond: Digest,
    pub generated: Vec<u32>,
    /// The commitment of what the producer delivers (the output stage's committed output, [`PipelineEvidenceV1::output_root`]).
    pub output_root: Digest,
    pub evidence_root: Digest,
}

impl PipelineClaimV1 {
    pub fn id(&self) -> Digest {
        crate::hash::object_id(PIPELINE_CLAIM_DOMAIN_V1, self)
    }
}

/// **What a pipeline claim publishes.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PipelinePublicRecordV1 {
    pub claim_id: Digest,
    pub pipeline_bytes: Vec<u8>,
    pub program_bytes: Vec<Vec<u8>>,
    pub plan: PipelinePlanV1,
    pub evidence: PipelineEvidenceV1,
    pub stages: Vec<StageCommitmentsV1>,
    /// Each program's param commitments `(param, layer, commitment)`.
    pub param_commitments: Vec<Vec<(u16, Option<u16>, Digest)>>,
    pub facts: PipelineJobFactsV1,
    pub generated: Vec<u32>,
    pub beacon: Digest,
}

impl PipelinePublicRecordV1 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PIPELINE_RECORD_BYTES_V1 {
            return Err("the record exceeds the parse bound".into());
        }
        borsh::from_slice(bytes).map_err(|e| format!("not a pipeline public record: {e}"))
    }
}

/// **A pipeline fault's canonical bytes.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PipelineFaultWireV1 {
    /// A relation inside stage `stage`.
    Stage { stage: u8, proof: FaultProofWireV1 },
    /// An edge: stage `stage`'s committed input `input` at `position` is not its binding's value (`R`'s draw included).
    Edge { stage: u8, input: u16, position: u32, claimed: TensorWireV1, upstream: Vec<TensorWireV1> },
    /// Delivered id `index` is not the decode of the committed logits row that selects it.
    Decode { index: u32, logits: TensorWireV1 },
}

impl PipelineFaultWireV1 {
    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }
}

/// What a fresh pipeline verifier concludes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipelineFindingV1 {
    Pass,
    Fault(PipelineFaultWireV1),
    Unavailable(String),
    Malformed(String),
    Inconsistent(String),
}

/// **A pipeline verifier that knows only public bytes** and the descriptors its own binary implements.
pub struct FreshPipelineVerifierV1 {
    pub record: PipelinePublicRecordV1,
    pub descriptor: KernelDescriptorV1,
    pub pipeline: TirPipelineV1,
    pub programs: Vec<TirProgramV2>,
    pub traces: Vec<EvidenceV1>,
    pub params: Vec<ParamCommitmentsV1>,
    pub job: PipelineJob,
    pub random: PipelineRandomV1,
    pub header: PipelineHeaderV1,
    pub decode: Option<DecodeRuleV1>,
}

impl FreshPipelineVerifierV1 {
    /// Build from a record's bytes. `known` is the binary's descriptors; `header` and `class` are what the chain says the claim's
    /// class is.
    pub fn from_public_bytes(
        bytes: &[u8],
        known: &[KernelDescriptorV1],
        header: PipelineHeaderV1,
        class: &PipelineClassV1,
    ) -> Result<Self, String> {
        Self::from_public_bytes_in_mode(bytes, known, header, class, crate::mode::VerificationModeV1::PanelLicensed)
    }

    /// [`Self::from_public_bytes`] for a class registered under `mode`: the header's class id is the class binding's id **under
    /// that mode** (RFC-0015 §4.1 — the mode is part of the class identity).
    pub fn from_public_bytes_in_mode(
        bytes: &[u8],
        known: &[KernelDescriptorV1],
        header: PipelineHeaderV1,
        class: &PipelineClassV1,
        mode: crate::mode::VerificationModeV1,
    ) -> Result<Self, String> {
        let record = PipelinePublicRecordV1::from_bytes(bytes)?;
        if header.class_binding_id != crate::mode::class_id_for_mode_v1(&class.class_binding_id(), mode) {
            return Err("the header names another class".into());
        }
        let descriptor = known
            .iter()
            .find(|d| d.digest() == record.plan.descriptor_digest && d.digest() == class.descriptor_digest)
            .cloned()
            .ok_or("the plan names a kernel this binary does not implement, or not the class's (never success)")?;
        let programs = record
            .program_bytes
            .iter()
            .map(|b| TirProgramV2::decode_canonical(b).map_err(|e| format!("program bytes: {e}")))
            .collect::<Result<Vec<_>, _>>()?;
        let pipeline = TirPipelineV1::decode_canonical(&record.pipeline_bytes, &programs).map_err(|e| format!("pipeline: {e}"))?;
        if pipeline_root_v1(&pipeline, &programs) != class.pipeline_root || record.plan.root() != class.plan_root {
            return Err("the published pipeline or plan is not the class's".into());
        }
        let params: Vec<ParamCommitmentsV1> = record
            .param_commitments
            .iter()
            .map(|v| ParamCommitmentsV1 { by_instance: v.iter().map(|(j, l, d)| ((*j, *l), *d)).collect() })
            .collect();
        if params.iter().map(ParamCommitmentsV1::root).collect::<Vec<_>>() != class.artifact_roots
            || params.iter().zip(&record.param_commitments).any(|(p, v)| p.by_instance.len() != v.len())
        {
            return Err("the published param commitments are not the class's artifacts".into());
        }
        let has_stream = stream_stage(&pipeline).is_some();
        if has_stream != class.decode.is_some() || (!has_stream && !record.generated.is_empty()) {
            return Err("a stream stage's output needs the class's decode rule; a pipeline without one delivers no ids".into());
        }
        let traces = record.stages.iter().map(StageCommitmentsV1::evidence).collect();
        let job = record.facts.job(&record.generated);
        let random = record.facts.random();
        Ok(Self { record, descriptor, pipeline, programs, traces, params, job, random, header, decode: class.decode })
    }

    fn ctx(&self) -> PipelineContextV1<'_> {
        self.ctx_with(self.record.beacon)
    }

    fn ctx_with(&self, beacon: Digest) -> PipelineContextV1<'_> {
        PipelineContextV1 {
            descriptor: &self.descriptor,
            pipeline: &self.pipeline,
            programs: &self.programs,
            plan: &self.record.plan,
            header: self.header,
            params: &self.params,
            traces: &self.traces,
            evidence: &self.record.evidence,
            job: &self.job,
            random: &self.random,
            random_binding: self.random.binding(),
            claim_id: self.record.claim_id,
            beacon,
        }
    }

    /// The structural checks every court runs first: what a chain refuses at inclusion.
    pub fn structure(&self) -> Result<(), String> {
        pipeline_structure_v1(&self.ctx())
    }

    /// `(stage, rows_from, post occurrence, output node)` of the stream stage, if there is one.
    fn stream(&self) -> Option<(usize, u32, u16, u16)> {
        let si = stream_stage(&self.pipeline)?;
        let st = &self.pipeline.stages[si];
        let v = stage_view_v1(&self.programs[st.program as usize]);
        Some((si, stage_rows_from(st, &self.job), v.post_occurrence, v.output_node))
    }

    /// **Check the claim**: the decode of every delivered id, then every stage's edges and relations. `materials[s]` serves stage
    /// `s` (each value authenticated against its commitment).
    pub fn check(&self, materials: &[&dyn MaterialV1]) -> PipelineFindingV1 {
        self.check_salted(materials, self.record.beacon)
    }

    /// [`Self::check`] with the checker's OWN randomness (`salt` replaces the public beacon in every stage's challenge): grinding
    /// the beacon gains a producer nothing against such a checker, and the courts never read the vectors.
    pub fn check_salted(&self, materials: &[&dyn MaterialV1], salt: Digest) -> PipelineFindingV1 {
        if let Err(why) = self.structure() {
            return PipelineFindingV1::Malformed(why);
        }
        if let (Some((si, from, post, out)), Some(rule)) = (self.stream(), self.decode) {
            for (r, id) in self.record.generated.iter().enumerate() {
                let p = from + r as u32;
                let Some(logits) = materials.get(si).and_then(|m| m.node_value(p, post, out)) else {
                    return PipelineFindingV1::Unavailable(format!("stage {si} logits at {p} were not served"));
                };
                if self.traces[si].at(p, post, out) != Some(&tensor_commitment(&logits)) {
                    return PipelineFindingV1::Unavailable(format!("stage {si} logits at {p}: not the committed value"));
                }
                if rule.select(&logits) != Some(*id) {
                    return PipelineFindingV1::Fault(PipelineFaultWireV1::Decode {
                        index: r as u32,
                        logits: TensorWireV1::of(&logits),
                    });
                }
            }
        }
        match verify_pipeline_v1(&self.ctx_with(salt), materials) {
            PipelineVerdictV1::Pass { .. } => PipelineFindingV1::Pass,
            PipelineVerdictV1::StageFault { stage, proof } => {
                PipelineFindingV1::Fault(PipelineFaultWireV1::Stage { stage, proof: FaultProofWireV1::of(&proof) })
            }
            PipelineVerdictV1::EdgeFault(e) => PipelineFindingV1::Fault(PipelineFaultWireV1::Edge {
                stage: e.stage,
                input: e.input,
                position: e.position,
                claimed: TensorWireV1::of(&e.claimed),
                upstream: e.upstream.iter().map(TensorWireV1::of).collect(),
            }),
            PipelineVerdictV1::Unavailable { what } => PipelineFindingV1::Unavailable(what),
            PipelineVerdictV1::EvidenceMalformed { why } => PipelineFindingV1::Malformed(why),
            PipelineVerdictV1::Inconsistent { why } => PipelineFindingV1::Inconsistent(why),
        }
    }

    /// **The court**, from a filing's bytes: `Ok` convicts, `Err` dismisses (with why).
    pub fn try_proof(&self, bytes: &[u8]) -> Result<(), String> {
        let wire: PipelineFaultWireV1 = borsh::from_slice(bytes).map_err(|e| format!("not a pipeline fault: {e}"))?;
        let ctx = self.ctx();
        match wire {
            PipelineFaultWireV1::Stage { stage, proof } => {
                let proof = proof.decode()?;
                verify_stage_fault_v1(&ctx, stage, &proof).map(|_| ()).map_err(|d| format!("{d:?}"))
            }
            PipelineFaultWireV1::Edge { stage, input, position, claimed, upstream } => {
                let proof = EdgeFaultProofV1 {
                    stage,
                    input,
                    position,
                    claimed: claimed.decode()?,
                    upstream: upstream.iter().map(TensorWireV1::decode).collect::<Result<_, _>>()?,
                };
                verify_edge_fault_v1(&ctx, &proof).map(|_| ()).map_err(|d| format!("{d:?}"))
            }
            PipelineFaultWireV1::Decode { index, logits } => {
                pipeline_structure_v1(&ctx)?;
                let ((si, from, post, out), rule) = self.stream().zip(self.decode).ok_or("the pipeline delivers no ids")?;
                let id = *self.record.generated.get(index as usize).ok_or("no such delivered id")?;
                let logits = logits.decode()?;
                if self.traces[si].at(from + index, post, out) != Some(&tensor_commitment(&logits)) {
                    return Err("the opened logits are not the committed ones".into());
                }
                match rule.select(&logits) {
                    Some(t) if t == id => Err("no fault: the delivered id is the decode".into()),
                    _ => Ok(()),
                }
            }
        }
    }
}
