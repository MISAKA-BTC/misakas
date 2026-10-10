//! **The ledger's K2-TIR-v4 rules** (`docs/design/palw/k2-real-scale.md` §§1.3, 4, 5): tiled jobs and their tiles, segmented claims,
//! per-position demands served in parts, and the segmented court — beside the historical rules of [`crate::ledger`], which dispatches
//! here (inner tags 16–18, `ProsecutionV1::Segmented`, a demand or response on a segmented claim).
//!
//! * **Tiled jobs** (table 20): `PostTiledJob` binds a prompt by its length and tile root; `PostPromptTile` (any bond) posts one tile,
//!   authenticated against the root and inside the class's token bound; the ledger keeps a bitmap, never the ids. A claim on a tiled job
//!   commits only once every tile is posted.
//! * **Segmented claims**: at inclusion the ledger checks, in O(segments), the binding (job, evidence, class, suite, length, generated
//!   ids, the input root over the prompt's tile root), the segment count and the claim root — then the historical admission (seal,
//!   reservation, OPV capacity). Only a K2-TIR-v4 class has them, and only under OptimisticPublicVerification.
//! * **Demands** (table 21): a position demand asks for everything committed at the position, served in parts; each part authenticates
//!   or is classified; a position with a part missing at the deadline defaults (the historical default path). A demander holds at most
//!   [`crate::seg_da::SEG_OPEN_PER_DEMANDER_V4`] open sessions per claim and there is no global cap, so nobody can be starved. Once every
//!   part is served the position is public, the claim's proof grace starts, and the demanders' bonds stay RESERVED until the grace
//!   ends, where [`KernelLedgerV1::settle_served_demand_bonds_v4`] settles them (refund today; G14-R4 owns the burn).

use crate::element::{SegClaimContextV1, SegFaultV1, verify_seg_fault_v1};
use crate::hash::Digest;
use crate::job::{DecodeRuleV1, KernelClaimV1};
use crate::ledger::{
    ClaimBodyV1, DemandRowV1, KernelLedgerV1, KernelRefusalV1, LedgerEventV1, demand_window_open, response_class_code, settle,
};
use crate::lifecycle::{ClaimEventV1, ClaimStateV1};
use crate::seg::{
    PromptTileOpeningV1, SegmentedEvidenceV2, TiledJobRowV1, TiledJobV1, job_input_root_v2, prompt_root_of_ids_v1, prompt_tiles_v1,
};
use crate::seg_da::{SEG_OPEN_PER_DEMANDER_V4, SEG_PART_BYTES_V4, SegProgressV1, classify_part_v1, position_parts_v1};
use crate::settle::SettlementKindV1;

/// A job as a segmented claim reads it, inline or tiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobViewV1 {
    pub class_binding_id: Digest,
    pub prompt_len: u32,
    pub prompt_root: Digest,
    /// The prompt's ids when the job carries them (an inline job).
    pub inline_prompt: Option<Vec<u32>>,
    pub max_new_tokens: u32,
    pub decode: DecodeRuleV1,
    /// A tiled job whose every tile is posted (always true for an inline job).
    pub complete: bool,
}

/// **Everything public a segmented claim's courts and a fresh verifier read**, owned (the ledger's rows decoded once).
#[derive(Clone, Debug)]
pub struct SegClaimViewV1 {
    pub program: misaka_palw_tir::program::TirProgramV1,
    pub params: crate::trace::ParamCommitmentsV1,
    pub segment_roots: Vec<Digest>,
    pub positions: u32,
    pub job: JobViewV1,
    pub generated: Vec<u32>,
    /// K2-TIR-v5: the program's job-bound inputs (an encoder or a head).
    pub encoder: Option<crate::seg_encoder::EncoderBindingV1>,
}

impl SegClaimViewV1 {
    pub fn context(&self) -> SegClaimContextV1<'_> {
        SegClaimContextV1 {
            program: &self.program,
            params: &self.params,
            segment_roots: &self.segment_roots,
            positions: self.positions,
            prompt_len: self.job.prompt_len,
            prompt_root: self.job.prompt_root,
            inline_prompt: self.job.inline_prompt.as_deref(),
            generated: &self.generated,
            decode: self.job.decode,
            encoder: self.encoder,
        }
    }
}

/// **What a segmented claim publishes** (the read model's `public_record` for kind `segmented`): the class's program bytes, plan and
/// v3 param commitments, the evidence object and the segment roots, the job's prompt (by length and root, its ids when inline) and the
/// delivered ids. [`Self::view`] rebuilds the [`SegClaimViewV1`] a fresh verifier and the court read.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SegmentedClaimRecordV1 {
    pub claim_id: Digest,
    pub program_bytes: Vec<u8>,
    pub plan: crate::plan::VerificationPlanV1,
    pub param_commitments: Vec<(u16, Option<u16>, Digest)>,
    pub evidence: SegmentedEvidenceV2,
    pub segment_roots: Vec<Digest>,
    pub job_id: Digest,
    pub prompt_len: u32,
    pub prompt_root: Digest,
    pub inline_prompt: Option<Vec<u32>>,
    pub max_new_tokens: u32,
    pub decode: DecodeRuleV1,
    pub generated: Vec<u32>,
}

impl SegmentedClaimRecordV1 {
    pub fn of(l: &KernelLedgerV1, claim: &Digest) -> Option<Self> {
        let row = l.claims.get(claim)?;
        let ClaimBodyV1::Segmented { claim: c, evidence, segment_roots } = &row.body else { return None };
        let class = l.classes.get(&row.class_binding_id)?;
        let job = l.job_view_v1(&row.job_id)?;
        Some(Self {
            claim_id: *claim,
            program_bytes: class.program_bytes.clone(),
            plan: class.plan.clone(),
            param_commitments: class.param_commitments.by_instance.iter().map(|((j, l), d)| (*j, *l, *d)).collect(),
            evidence: evidence.clone(),
            segment_roots: segment_roots.clone(),
            job_id: row.job_id,
            prompt_len: job.prompt_len,
            prompt_root: job.prompt_root,
            inline_prompt: job.inline_prompt,
            max_new_tokens: job.max_new_tokens,
            decode: job.decode,
            generated: c.generated.clone(),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("in-memory borsh")
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > crate::public::MAX_PUBLIC_RECORD_BYTES_V1 {
            return Err("the record exceeds the parse bound".into());
        }
        borsh::from_slice(bytes).map_err(|e| format!("not a segmented claim record: {e}"))
    }

    /// The verifier's view, checked against the header the chain states (program root, artifact root, plan root, evidence).
    pub fn view(&self, header: &crate::evidence::EvidenceHeaderV1) -> Result<SegClaimViewV1, String> {
        let program = misaka_palw_tir::program::TirProgramV1::decode_canonical(&self.program_bytes).map_err(|e| e.to_string())?;
        let params =
            crate::trace::ParamCommitmentsV1 { by_instance: self.param_commitments.iter().map(|(j, l, d)| ((*j, *l), *d)).collect() };
        if crate::public::program_root_v1(&self.program_bytes) != header.program_root
            || params.root() != header.artifact_root
            || self.plan.root() != header.plan_root
            || self.evidence.header != *header
        {
            return Err("the record is not the class's or the claim's".into());
        }
        self.evidence.check_structure(&self.segment_roots)?;
        let encoder = if self.plan.descriptor_digest == crate::descriptor::k2_tir_v5_descriptor().digest() {
            Some(crate::seg_encoder::encoder_binding_v1(&program)?)
        } else {
            None
        };
        Ok(SegClaimViewV1 {
            program,
            params,
            segment_roots: self.segment_roots.clone(),
            positions: self.evidence.positions,
            job: JobViewV1 {
                class_binding_id: header.class_binding_id,
                prompt_len: self.prompt_len,
                prompt_root: self.prompt_root,
                inline_prompt: self.inline_prompt.clone(),
                max_new_tokens: self.max_new_tokens,
                decode: self.decode,
                complete: true,
            },
            generated: self.generated.clone(),
            encoder,
        })
    }
}

impl KernelLedgerV1 {
    /// A job by id, inline or tiled.
    pub fn job_view_v1(&self, job: &Digest) -> Option<JobViewV1> {
        if let Some(j) = self.jobs.get(job) {
            return Some(JobViewV1 {
                class_binding_id: j.class_binding_id,
                prompt_len: j.prompt.len() as u32,
                prompt_root: prompt_root_of_ids_v1(&j.prompt),
                inline_prompt: Some(j.prompt.clone()),
                max_new_tokens: j.max_new_tokens,
                decode: j.decode,
                complete: true,
            });
        }
        self.tiled_jobs.get(job).map(|row| JobViewV1 {
            class_binding_id: row.job.class_binding_id,
            prompt_len: row.job.prompt_len,
            prompt_root: row.job.prompt_root,
            inline_prompt: None,
            max_new_tokens: row.job.max_new_tokens,
            decode: row.job.decode,
            complete: row.complete(),
        })
    }

    /// **The public view of a segmented claim** (`None` for another kind of claim).
    pub fn seg_claim_view_v1(&self, claim: &Digest) -> Option<SegClaimViewV1> {
        let row = self.claims.get(claim)?;
        let ClaimBodyV1::Segmented { claim: c, evidence, segment_roots } = &row.body else { return None };
        let class = self.classes.get(&row.class_binding_id)?;
        Some(SegClaimViewV1 {
            program: class.program.clone(),
            params: class.param_commitments.clone(),
            segment_roots: segment_roots.clone(),
            positions: evidence.positions,
            job: self.job_view_v1(&row.job_id)?,
            generated: c.generated.clone(),
            encoder: if crate::descriptor::is_encoder_v1(&class.descriptor) {
                crate::seg_encoder::encoder_binding_v1(&class.program).ok()
            } else {
                None
            },
        })
    }

    pub(crate) fn post_tiled_job(&mut self, job: &TiledJobV1) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "PostTiledJob";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let class = self.classes.get(&job.class_binding_id).ok_or_else(|| rule("no such class".into()))?;
        if !crate::descriptor::is_segmented_v1(&class.descriptor) {
            return Err(rule("a tiled job names a class that is not K2-TIR-v4".into()));
        }
        if crate::descriptor::is_encoder_v1(&class.descriptor) {
            // K2-TIR-v5: the prompt is the encoder's input (1 ..= L ids), and nothing is generated.
            let e = crate::seg_encoder::encoder_binding_v1(&class.program).map_err(rule)?;
            if job.prompt_len == 0 || job.prompt_len > e.l || job.max_new_tokens != 0 {
                return Err(rule(format!("an encoder job has 1 ..= {} ids and generates nothing", e.l)));
            }
        } else {
            job.well_formed(class.plan.max_positions).map_err(rule)?;
        }
        let id = job.id();
        if self.tiled_jobs.contains_key(&id) || self.jobs.contains_key(&id) {
            return Err(rule("the job is already posted".into()));
        }
        self.tiled_jobs.insert(id, TiledJobRowV1::new(job.clone()));
        Ok(())
    }

    pub(crate) fn post_prompt_tile(&mut self, job: &Digest, tile: &PromptTileOpeningV1) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "PostPromptTile";
        let rule = |why: &str| KernelRefusalV1::rule(NAME, why);
        let row = self.tiled_jobs.get(job).ok_or_else(|| rule("no such tiled job"))?;
        if tile.index >= prompt_tiles_v1(row.job.prompt_len) || row.is_posted(tile.index) {
            return Err(rule("no such tile, or it is already posted"));
        }
        let bound = self.classes.get(&row.job.class_binding_id).map(|c| c.program.token_bound).ok_or_else(|| rule("no such class"))?;
        if tile.ids.iter().any(|t| *t >= bound) {
            return Err(rule("an id past the class's token bound"));
        }
        // Authenticating a tile is hashing work: charged to the block like every adjudication.
        self.charge(NAME, 0)?;
        let row = self.tiled_jobs.get(job).expect("checked");
        if !tile.authenticates(row.job.prompt_len, &row.job.prompt_root) {
            return Err(rule("the tile is not the job's"));
        }
        self.tiled_jobs.get_mut(job).expect("checked").mark(tile.index);
        Ok(())
    }

    // ADR-0176 hook (lane BUDGET's `palw_bond_budget_v1`, not built): once that engine exists, accepting a segmented claim reserves
    // Q = 1 and the claim's maximum R (its Final reward) and F (its OPV work credit) against the producer bond over the window W, and
    // a refused reservation refuses the claim; nothing is returned before `d + W` (`k2-real-scale.md` §8.2).
    pub(crate) fn commit_segmented_claim(
        &mut self,
        claim: &KernelClaimV1,
        evidence: &SegmentedEvidenceV2,
        segment_roots: &[Digest],
        salt: Option<&Digest>,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "CommitSegmentedClaim";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let job = self.job_view_v1(&claim.job_id).ok_or_else(|| rule("no such job".into()))?;
        if !job.complete {
            return Err(rule("the job's prompt is not fully posted: its input is not public yet".into()));
        }
        let class = self.classes.get(&job.class_binding_id).ok_or_else(|| rule("no such class".into()))?;
        if !crate::descriptor::is_segmented_v1(&class.descriptor) {
            return Err(rule("a segmented claim of a class that is not K2-TIR-v4".into()));
        }
        self.opv_claim_gate(&job.class_binding_id).map_err(rule)?;
        if claim.evidence_root != evidence.root() {
            return Err(rule("binding fault WrongEvidence".into()));
        }
        if evidence.header != class.header(job.class_binding_id) {
            return Err(rule("binding fault WrongClass".into()));
        }
        if evidence.suite != crate::evidence::SuiteParamsV1::of(&class.descriptor) {
            return Err(rule("the evidence names another suite than the class's descriptor".into()));
        }
        let fed: &[u32] = if crate::descriptor::is_encoder_v1(&class.descriptor) {
            // K2-TIR-v5: ONE position over the job's ids; no id is delivered (the result is the output node at position 0).
            if !claim.generated.is_empty() || job.max_new_tokens != 0 {
                return Err(rule("binding fault WrongGenerationLength: an encoder claim delivers no id".into()));
            }
            if evidence.positions != 1 || class.plan.max_positions != 1 {
                return Err(rule("binding fault WrongLength: an encoder claim is one position".into()));
            }
            &[]
        } else {
            if !crate::job::generation_length_matches_v1(job.max_new_tokens, claim.generated.len()) {
                return Err(rule("binding fault WrongGenerationLength".into()));
            }
            if claim.generated.iter().any(|t| *t >= class.program.token_bound) {
                return Err(rule("binding fault TokenOutOfRange".into()));
            }
            let fed = &claim.generated[..claim.generated.len() - 1];
            let positions = job.prompt_len as u64 + fed.len() as u64;
            if evidence.positions as u64 != positions || positions > class.plan.max_positions as u64 {
                return Err(rule("binding fault WrongLength".into()));
            }
            fed
        };
        if evidence.job_input_root != job_input_root_v2(job.prompt_len, &job.prompt_root, fed) {
            return Err(rule("binding fault WrongInput".into()));
        }
        evidence.check_structure(segment_roots).map_err(rule)?;
        let id = claim.id();
        if self.claims.contains_key(&id) {
            return Err(rule("an exact duplicate claim".into()));
        }
        self.reveal_ready(&claim.job_id, &claim.producer_bond, &id, salt).map_err(rule)?;
        self.opv_claim_capacity(&job.class_binding_id, &claim.producer_bond, &claim.job_id).map_err(rule)?;
        self.charge(NAME, 0)?;
        let body = ClaimBodyV1::Segmented { claim: claim.clone(), evidence: evidence.clone(), segment_roots: segment_roots.to_vec() };
        self.admit(id, claim.producer_bond, job.class_binding_id, claim.job_id, body, out).map_err(rule)?;
        self.seals.remove(&(claim.job_id, claim.producer_bond));
        // G14R's salted reveal (claim seal v2): the salt is kept for the sealed-source beacon, as for every other claim kind.
        if let Some(salt) = salt {
            self.claim_beacon_salts.insert(id, *salt);
        }
        Ok(())
    }

    /// The open sessions `demander` holds on `claim`.
    fn seg_open_sessions(&self, claim: &Digest, demander: &Digest) -> u32 {
        self.demands
            .range((*claim, 0, 0)..=(*claim, u8::MAX, u32::MAX))
            .filter(|(_, d)| d.demanders.iter().any(|(b, _)| b == demander))
            .count() as u32
    }

    pub(crate) fn seg_file_demand(
        &mut self,
        demander: &Digest,
        claim: &Digest,
        stage: u8,
        position: u32,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "FileDemand";
        let rule = |why: &str| KernelRefusalV1::rule(NAME, why);
        let daa = self.daa;
        let row = self.claims.get(claim).ok_or_else(|| rule("no such claim"))?;
        let class_id = row.class_binding_id;
        if row.terminal_for_demands() {
            return Err(rule("the claim is already decided"));
        }
        let is_final = matches!(row.life.state, ClaimStateV1::Final { .. });
        let path = self.policy.court_deadline_daa.saturating_add(self.policy.proof_grace_daa);
        let window_open = match &row.life.state {
            ClaimStateV1::Final { .. } => row.liability_until.is_some_and(|u| daa.saturating_add(path) <= u),
            s => demand_window_open(s, daa),
        };
        if !window_open {
            return Err(rule("the challenge window is closed"));
        }
        let ClaimBodyV1::Segmented { evidence, .. } = &row.body else { return Err(rule("not a segmented claim")) };
        if stage != 0 || position >= evidence.positions {
            return Err(rule("the claim commits no such position"));
        }
        let k = (*claim, stage, position);
        if self.seg_progress.get(&k).is_some_and(|p| p.complete_daa.is_some()) {
            return Err(rule("already served: it is public"));
        }
        let need = self.policy.demand_bond;
        let Some(b) = self.bonds.get(demander) else { return Err(rule("the demander is not a registered bond")) };
        if b.exit_requested.is_some() || b.free() < need {
            return Err(rule("the demander's free collateral does not cover the demand bond"));
        }
        let already_joined = self.demands.get(&k).is_some_and(|d| d.demanders.iter().any(|(b, _)| b == demander));
        if !already_joined && self.seg_open_sessions(claim, demander) >= SEG_OPEN_PER_DEMANDER_V4 {
            return Err(rule("the demander already holds its open sessions on this claim"));
        }
        if let Some(d) = self.demands.get_mut(&k) {
            if !already_joined {
                if d.demanders.len() >= crate::gate::MAX_DEMANDERS_PER_SESSION_V1 {
                    return Err(rule("the shared demand already has its maximum collateral participants"));
                }
                d.demanders.push((*demander, need));
                self.bonds.get_mut(demander).expect("checked").reserved += need;
                settle(out, *demander, need, SettlementKindV1::ReserveDemand, Some(*claim));
            }
            out.push(LedgerEventV1::DemandJoined { claim: *claim, stage, position });
            return Ok(());
        }
        let program = self.classes.get(&class_id).map(|c| c.program.clone()).ok_or_else(|| rule("no such class"))?;
        let parts = position_parts_v1(&program, position).map_err(|_| rule("the position does not lay out"))?.len() as u32;
        self.bonds.get_mut(demander).expect("checked").reserved += need;
        let deadline = daa + self.policy.court_deadline_daa;
        self.demands.insert(k, DemandRowV1 { demanders: vec![(*demander, need)], filed_daa: daa, deadline_daa: deadline, last: None });
        self.seg_progress.insert(k, SegProgressV1::new(parts));
        if !is_final {
            let row = self.claims.get_mut(claim).expect("checked");
            let _ = row.life.apply(ClaimEventV1::DisputeFiled { daa });
        }
        settle(out, *demander, need, SettlementKindV1::ReserveDemand, Some(*claim));
        out.push(LedgerEventV1::DemandOpened { claim: *claim, stage, position, deadline });
        Ok(())
    }

    pub(crate) fn seg_respond(
        &mut self,
        signer: &Digest,
        claim: &Digest,
        position: u32,
        bytes: &[u8],
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "Respond";
        let k = (*claim, 0u8, position);
        if !self.demands.contains_key(&k) {
            return Err(KernelRefusalV1::rule(NAME, "no open demand for this position"));
        }
        // C4 F-C4R4-14: a response spends no run of the block's shared adjudication budget (classifying a part is linear in its own
        // bytes); a rejected one costs its signer the dismissed-proof fee (checked by the caller before anything is classified).
        let fee = self.policy.dismissed_proof_fee;
        let by_producer = self.claims.get(claim).is_some_and(|r| r.producer == *signer);
        let verdict = if bytes.len() as u64 > SEG_PART_BYTES_V4 {
            Err("oversized")
        } else {
            let row = self.claims.get(claim).expect("a demand names a committed claim");
            let ClaimBodyV1::Segmented { evidence, segment_roots, .. } = &row.body else {
                return Err(KernelRefusalV1::rule(NAME, "not a segmented claim"));
            };
            let program = &self.classes.get(&row.class_binding_id).expect("a claim's class").program;
            classify_part_v1(program, segment_roots, evidence.positions, position, bytes)
        };
        match verdict {
            Ok(part) => {
                let progress = self.seg_progress.get_mut(&k).expect("opened with its demand");
                progress.mark(part);
                if !progress.all_served() {
                    return Ok(());
                }
                // Every part in: the position is public from now on; the proof grace starts; the demanders' bonds stay reserved
                // until it ends (`settle_served_demand_bonds_v4`).
                let daa = self.daa;
                let until = daa.saturating_add(self.policy.proof_grace_daa);
                let d = self.demands.remove(&k).expect("checked");
                let progress = self.seg_progress.get_mut(&k).expect("opened with its demand");
                progress.complete_daa = Some(daa);
                progress.served = Vec::new();
                progress.held = d.demanders;
                progress.grace_until = until;
                if let Some(row) = self.claims.get_mut(claim) {
                    let _ = row.life.apply(ClaimEventV1::ProofGrace { until_daa: until });
                    if matches!(row.life.state, ClaimStateV1::Disputed { .. }) {
                        let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: false });
                    }
                }
                out.push(LedgerEventV1::Served { claim: *claim, stage: 0, position });
            }
            Err(class) => {
                if by_producer && let Some(d) = self.demands.get_mut(&k) {
                    d.last = Some(response_class_code(class));
                }
                if let Some(b) = self.bonds.get_mut(signer) {
                    b.collateral -= fee;
                }
                self.burned += fee;
                out.push(LedgerEventV1::ResponseRejected { claim: *claim, stage: 0, position, class });
                settle(out, *signer, fee, SettlementKindV1::SlashFiling, Some(*claim));
                settle(out, *signer, fee, SettlementKindV1::Burn, Some(*claim));
            }
        }
        Ok(())
    }

    /// **The segmented court**, over ledger state and the filing's bytes only.
    pub(crate) fn seg_adjudicate(&self, claim: &Digest, bytes: &[u8]) -> Result<(), String> {
        let view = self.seg_claim_view_v1(claim).ok_or("no segmented claim")?;
        let fault = SegFaultV1::from_bytes(bytes)?;
        verify_seg_fault_v1(&view.context(), &fault).map(|_| ()).map_err(|d| format!("{d:?}"))
    }

    /// **The fate of a served position's demand bonds once its proof grace has ended** (and the claim was not convicted in it). Today:
    /// returned. The rule is G14-R4's (proposed: burned, so demanding an honest claim's positions costs the demander what serving
    /// costs the producer — `docs/design/palw/k2-real-scale.md` §4).
    pub fn settle_served_demand_bonds_v4(&mut self, claim: &Digest, held: &[(Digest, u64)], out: &mut Vec<LedgerEventV1>) {
        for (bond, amount) in held {
            if let Some(b) = self.bonds.get_mut(bond) {
                b.reserved = b.reserved.saturating_sub(*amount);
            }
            settle(out, *bond, *amount, SettlementKindV1::ReleaseDemand, Some(*claim));
        }
    }

    /// The claim is decided (convicted, unavailable): every bond held by its served positions returns. The number returned.
    pub(crate) fn seg_release_held(&mut self, claim: &Digest, out: &mut Vec<LedgerEventV1>) -> u32 {
        let keys: Vec<_> =
            self.seg_progress.iter().filter(|((c, _, _), p)| c == claim && !p.held.is_empty()).map(|(k, _)| *k).collect();
        let mut n = 0;
        for k in keys {
            let held = std::mem::take(&mut self.seg_progress.get_mut(&k).expect("listed").held);
            n += held.len() as u32;
            for (bond, amount) in &held {
                if let Some(b) = self.bonds.get_mut(bond) {
                    b.reserved = b.reserved.saturating_sub(*amount);
                }
                settle(out, *bond, *amount, SettlementKindV1::ReleaseDemand, Some(*claim));
            }
        }
        n
    }

    /// The block's K2-TIR-v4 step: served positions past their grace settle their bonds; the progress of a demand that ended without
    /// being served (a default) goes.
    pub(crate) fn seg_tick(&mut self, out: &mut Vec<LedgerEventV1>) {
        let daa = self.daa;
        let due: Vec<_> = self
            .seg_progress
            .iter()
            .filter(|(_, p)| p.complete_daa.is_some() && !p.held.is_empty() && daa >= p.grace_until)
            .map(|(k, _)| *k)
            .collect();
        for k in due {
            let held = std::mem::take(&mut self.seg_progress.get_mut(&k).expect("listed").held);
            self.settle_served_demand_bonds_v4(&k.0, &held, out);
        }
        let stale: Vec<_> = self
            .seg_progress
            .iter()
            .filter(|(k, p)| p.complete_daa.is_none() && !self.demands.contains_key(k))
            .map(|(k, _)| *k)
            .collect();
        for k in stale {
            self.seg_progress.remove(&k);
        }
    }
}
