//! **The typed roots on the ledger** (a child of [`crate::ledger`], so it applies the route's own private rules: registration, seal-
//! then-reveal, the OPV gates and admission, the block's adjudication budget).
//!
//! Every typed object first needs `K2-TR-v1` Active in the ledger's schedule; a `[WeightsV1]` specification is then applied by
//! **the same function** as route tags 1 / 13 (byte for byte); the typed kinds register only under `OptimisticPublicVerification`.

use super::*;
use crate::descriptor::KernelStandingV1;
use crate::job::verify_decode_fault_v1;
use crate::spec::composite::{
    ComponentV1, QuerySourceV1, StageClaimV1, StageInputV1, check_composite_root_v1, model_stage_record_v1, stage_claim_v1,
    stage_job_v1, stage_prompt_v1,
};
use crate::spec::memory::{
    MemoryLineV1, boundary_states_v1, check_memory_root_v1, check_post_state_v1, memory_root_v1, overlay_v1, slot_commitments_v1,
    step_claim_v1, step_header_v1, step_job_v1, step_record_v1,
};
use crate::spec::retrieval::{
    RetrievalRootV1, RetrievedV1, check_query_v1, check_result_v1, classify_entry_response_v1, classify_slice_response_v1,
    judge_retrieval_fault_v1, payload_digest_v1,
};
use crate::spec::{
    ComputationSpecV1, MEMORY_PRE_STATE_STAGE_V1, RETRIEVAL_ENTRY_STAGE_BASE_V1, SNAPSHOT_STAGE_BASE_V1, SpecClaimBodyV1, SpecClaimV1,
    SpecClassKindV1, SpecClassRowV1, SpecFaultV1, SpecJobV1, SpecObjectV1, SpecShapeV1, composite_bounds_v1, k2_tr_v1_descriptor,
    memory_bounds_v1, retrieval_bounds_v1,
};

impl KernelLedgerV1 {
    /// Whether the typed-roots extension `K2-TR-v1` is Active in this ledger's schedule now (the consumer's fence).
    pub fn typed_roots_active(&self) -> bool {
        self.schedule.standing_at(&k2_tr_v1_descriptor().digest(), self.daa) == KernelStandingV1::Active
    }

    /// **Apply one `Spec` object.** Transactional like every route object: a refusal leaves the state byte-identical.
    pub(super) fn apply_spec(
        &mut self,
        obj: &SpecObjectV1,
        auth: &AuthV1,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        let name = obj.name();
        if !self.typed_roots_active() {
            return Err(KernelRefusalV1::rule(
                name,
                "KERNEL_NOT_ACTIVE [typed-roots]: K2-TR-v1 is not Active in this ledger's schedule",
            ));
        }
        let len = borsh::to_vec(obj).map(|v| v.len()).unwrap_or(usize::MAX);
        if len > obj.max_bytes() {
            return Err(KernelRefusalV1::new(
                name,
                RefusalKindV1::Oversized,
                format!("{len} bytes past the {}-byte ceiling", obj.max_bytes()),
            ));
        }
        match obj {
            SpecObjectV1::RegisterClass { spec } => {
                let class = self.register_spec_class(spec)?;
                out.push(LedgerEventV1::ClassRegistered { class });
            }
            SpecObjectV1::PostJob { job } => {
                let id = self.post_spec_job(job, &auth.signer_bond)?;
                out.push(LedgerEventV1::JobPosted { job: id });
                // GAP-5 (user-pays): the job's escrow funds its Final reward, exactly as for `PostJob` (checked affordable above).
                self.open_job_escrow(&auth.signer_bond, id, out);
            }
            SpecObjectV1::CommitClaim { claim } => {
                self.commit_spec_claim(claim, None, out)?;
                out.push(LedgerEventV1::ClaimCommitted { claim: claim.id() });
            }
        }
        Ok(())
    }

    /// **The derived row of a typed class** — at registration (`registering`: the plan checked against the schedule now) and at every
    /// rebuild from rows (a pure function of the record, the policy and the rows of its components).
    pub(crate) fn spec_class_row_of(&self, spec: &ComputationSpecV1, registering: bool) -> Result<SpecClassRowV1, String> {
        let ext = k2_tr_v1_descriptor();
        let policy = &self.policy.prosecution;
        let (kind, bounds) = match spec.shape()? {
            SpecShapeV1::Weights(_) => return Err("a Weights-only specification is a legacy class row, not a typed one".into()),
            SpecShapeV1::Memory(w, m) => {
                let d = self.known_descriptor(&w.descriptor)?;
                let program = TirProgramV1::decode_canonical(&w.program_bytes).map_err(|e| format!("program: {e}"))?;
                if registering {
                    check_plan_v1(&self.schedule, &d, &program, program_root_v1(&w.program_bytes), &w.plan, self.daa)
                        .map_err(|o| o.to_string())?;
                }
                check_commitment_set(
                    &w.param_commitments,
                    &used_param_instances(&program, program.params.len()),
                    &declared_param_instances(&program.params, program.schedule.layers.len()),
                )?;
                let writers = check_memory_root_v1(m, &program, &w.param_commitments)?;
                let nodes: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
                let base = public_prosecution_complete_v1(&d, &w.plan, nodes, &ProfileMaterialV1::kernel_route(true), policy)
                    .map_err(|g| format!("the update rule is not publicly prosecutable: {g:?}"))?;
                let bounds = memory_bounds_v1(&base, &program, &w.plan, m, policy)?;
                let rule = ClassRowV1 {
                    descriptor: d,
                    program_bytes: w.program_bytes.clone(),
                    program,
                    plan: w.plan.clone(),
                    param_commitments: w.param_commitments.clone(),
                    network_domain: self.policy.network_domain,
                    ruleset_digest: self.policy.ruleset_digest,
                    bounds: base,
                };
                (SpecClassKindV1::Memory { rule: Box::new(rule), root: m.clone(), writers }, bounds)
            }
            SpecShapeV1::Retrieval(r) => {
                let bounds = retrieval_bounds_v1(r, &ext, policy)?;
                (SpecClassKindV1::Retrieval { root: r.clone() }, bounds)
            }
            SpecShapeV1::Composite(c) => {
                let mut components = Vec::with_capacity(c.stages.len());
                for (s, st) in c.stages.iter().enumerate() {
                    if let Some(m) = self.classes.get(&st.component) {
                        components.push(ComponentV1::Model(Box::new(m.clone())));
                        continue;
                    }
                    match self.typed.classes.get(&st.component).map(|r| &r.kind) {
                        Some(SpecClassKindV1::Retrieval { root }) => components.push(ComponentV1::Retrieval(root.clone())),
                        Some(SpecClassKindV1::Memory { .. }) => {
                            return Err(format!("KERNEL_EXTENSION_REQUIRED [composite-memory]: stage {s} is a memory class"));
                        }
                        Some(SpecClassKindV1::Composite { .. }) => {
                            return Err(format!("KERNEL_EXTENSION_REQUIRED [composite-nesting]: stage {s} is a composite"));
                        }
                        None => return Err(format!("stage {s}'s component is not a registered class")),
                    }
                }
                check_composite_root_v1(c, &components)?;
                let bounds = composite_bounds_v1(c, &components, &ext, policy)?;
                (SpecClassKindV1::Composite { root: c.clone(), components }, bounds)
            }
        };
        Ok(SpecClassRowV1 { spec: spec.clone(), kind, bounds })
    }

    fn register_spec_class(&mut self, spec: &ComputationSpecV1) -> Result<Digest, KernelRefusalV1> {
        const NAME: &str = "SpecRegisterClass";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let shape = spec.shape().map_err(rule)?;
        if let SpecShapeV1::Weights(w) = shape {
            // **Byte for byte**: today's registration, by the very function route tags 1 and 13 apply.
            return self.register_class(spec.mode, &w.descriptor, &w.program_bytes, &w.plan, &w.param_commitments);
        }
        spec.check_extension().map_err(rule)?;
        if !spec.mode.is_optimistic() {
            return Err(rule("a typed-root class registers only under OptimisticPublicVerification".into()));
        }
        let class = spec.class_id().map_err(rule)?;
        if self.typed.classes.contains_key(&class) || self.classes.contains_key(&class) {
            return Err(rule("the class is already registered".into()));
        }
        let opv = self.opv_register_gate(NAME)?;
        self.opv_class_admitted(NAME, &class)?;
        // What must be public by the consumer's fact: the update rule's artifact (base weights and `M0`), or the snapshot.
        let attested = match shape {
            SpecShapeV1::Memory(w, _) => Some(w.param_commitments.root()),
            SpecShapeV1::Retrieval(r) => Some(r.snapshot.root()),
            _ => None,
        };
        if attested.is_some_and(|root| !self.attested_artifacts.contains(&root)) {
            return Err(rule("the artifact or snapshot is not attested public by the consumer's registry".into()));
        }
        self.charge(NAME, 0)?;
        let row = self.spec_class_row_of(spec, true).map_err(rule)?;
        if row.bounds.max_court_work > self.policy.max_court_work_per_block {
            return Err(rule("the class's worst court does not fit one block's court budget: nobody could prosecute it".into()));
        }
        self.opv_class_economics(&opv, &row.bounds).map_err(rule)?;
        if let SpecClassKindV1::Memory { rule: r, root, .. } = &row.kind {
            let m0 = slot_commitments_v1(&root.slots, &r.param_commitments).expect("checked at registration");
            self.typed.lines.insert(class, MemoryLineV1::genesis(&root.slots, m0));
        }
        self.typed.classes.insert(class, row);
        self.opv.classes.insert(class);
        Ok(class)
    }

    /// `poster` is the signer: G14-R4's GAP-5 escrow (user-pays) opens against it exactly as for `PostJob` — affordable before the
    /// charge (here), opened after the insert (the caller). A memory job posted over a head that has since moved can never be
    /// claimed; its escrow goes back to the poster by the ordinary idle-escrow rule (`job_escrow_ttl_daa`, no live claim, no live
    /// seal), so a stranded job never strands its poster's collateral.
    fn post_spec_job(&mut self, job: &SpecJobV1, poster: &Digest) -> Result<Digest, KernelRefusalV1> {
        const NAME: &str = "SpecPostJob";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let id = job.id();
        if self.typed.jobs.contains_key(&id) {
            return Err(rule("the job is already posted".into()));
        }
        let class = self.typed.classes.get(&job.class()).ok_or_else(|| rule("no such class".into()))?;
        match (job, &class.kind) {
            (SpecJobV1::Memory(j), SpecClassKindV1::Memory { rule: r, root, .. }) => {
                j.well_formed(root, r).map_err(rule)?;
                let line = self.typed.lines.get(&j.class).ok_or_else(|| rule("the class has no memory line".into()))?;
                if j.pre_root != line.head_root {
                    return Err(rule(
                        "the job's pre-state is not the line head (memory is carried only from the chain's head)".into(),
                    ));
                }
            }
            (SpecJobV1::Retrieval(j), SpecClassKindV1::Retrieval { root }) => check_query_v1(root, &j.query).map_err(rule)?,
            (SpecJobV1::Composite(j), SpecClassKindV1::Composite { root, components }) => {
                for (s, (st, c)) in root.stages.iter().zip(components).enumerate() {
                    match (&st.input, c) {
                        (StageInputV1::Query(QuerySourceV1::JobQuery), ComponentV1::Retrieval(r)) => {
                            check_query_v1(r, &j.query).map_err(|e| rule(format!("stage {s}: {e}")))?
                        }
                        (StageInputV1::Tokens { sources, .. }, ComponentV1::Model(m))
                            if sources.contains(&crate::spec::composite::TokenSourceV1::JobPrompt) =>
                        {
                            if j.prompt.iter().any(|t| *t >= m.program.token_bound) {
                                return Err(rule(format!("stage {s}: a prompt token is past the model's token bound")));
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => return Err(rule("the job is not of its class's kind".into())),
        }
        self.job_escrow_affordable(NAME, poster)?;
        self.charge(NAME, 0)?;
        self.typed.jobs.insert(id, job.clone());
        Ok(id)
    }

    /// The structural checks of a model stage or a memory step: the sub-claim's binding, the header, the carried commitments, and
    /// everything a court checks before any relation (`FreshVerifierV1::structure`).
    #[allow(clippy::too_many_arguments)]
    fn check_kernel_part(
        &self,
        what: &str,
        job: &KernelJobV1,
        claim: &KernelClaimV1,
        evidence: &VerificationEvidenceV1,
        commitments: &[Vec<Vec<Digest>>],
        header: EvidenceHeaderV1,
        record: PublicClaimRecordV1,
        token_bound: u32,
    ) -> Result<(), String> {
        if let Some(f) = binding_fault_v1(job, claim, evidence, token_bound) {
            return Err(format!("{what}: binding fault {f:?}"));
        }
        if evidence.header != header {
            return Err(format!("{what}: binding fault {:?}", BindingFaultV1::WrongClass));
        }
        if EvidenceV1::new(commitments.to_vec()).root() != evidence.trace_root {
            return Err(format!("{what}: the carried trace commitments are not the evidence's trace root"));
        }
        FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, header)
            .and_then(|v| v.structure())
            .map_err(|why| format!("{what}: malformed evidence: {why}"))
    }

    /// **A typed claim's salted reveal** (inner kind 20 carrying `SaltedCommitV1::Spec`, OPV-BOOT GAP-B1a): the `Spec` object's own
    /// gate (the typed-roots extension Active, the claim's ceiling), then the commit over the producer's v2 seal; the salt is kept.
    pub(super) fn apply_salted_spec_claim(
        &mut self,
        claim: &SpecClaimV1,
        salt: &Digest,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "SpecCommitClaim";
        if !self.typed_roots_active() {
            return Err(KernelRefusalV1::rule(
                NAME,
                "KERNEL_NOT_ACTIVE [typed-roots]: K2-TR-v1 is not Active in this ledger's schedule",
            ));
        }
        let len = borsh::to_vec(claim).map(|v| v.len()).unwrap_or(usize::MAX);
        if len > crate::spec::MAX_SPEC_CLAIM_BYTES_V1 {
            return Err(KernelRefusalV1::new(
                NAME,
                RefusalKindV1::Oversized,
                format!("{len} bytes past the {}-byte ceiling", crate::spec::MAX_SPEC_CLAIM_BYTES_V1),
            ));
        }
        self.commit_spec_claim(claim, Some(salt), out)
    }

    fn commit_spec_claim(
        &mut self,
        claim: &SpecClaimV1,
        salt: Option<&Digest>,
        out: &mut Vec<LedgerEventV1>,
    ) -> Result<(), KernelRefusalV1> {
        const NAME: &str = "SpecCommitClaim";
        let rule = |why: String| KernelRefusalV1::rule(NAME, why);
        let id = claim.id();
        let job_id = claim.job_id();
        let producer = claim.producer();
        let job = self.typed.jobs.get(&job_id).ok_or_else(|| rule("no such job".into()))?;
        let class_id = job.class();
        let class = self.typed.classes.get(&class_id).ok_or_else(|| rule("no such class".into()))?;
        self.opv_claim_gate(&class_id).map_err(rule)?;
        if self.claims.contains_key(&id) {
            return Err(rule("an exact duplicate claim".into()));
        }
        // Cheap objective checks first (no court): shapes, the line head, the carried post-state, the result's order, the edges.
        let (pre_state, pre_source) = match (claim, job, &class.kind) {
            (SpecClaimV1::Memory(c), SpecJobV1::Memory(j), SpecClassKindV1::Memory { rule: r, root, writers }) => {
                let s = j.chunks.len();
                if c.generated.len() != s || c.steps.len() != s || c.step_roots.len() != s + 1 {
                    return Err(rule(format!("{s} steps: one token, one step commitment and S + 1 boundary roots")));
                }
                let line = self.typed.lines.get(&class_id).ok_or_else(|| rule("the class has no memory line".into()))?;
                if j.pre_root != line.head_root {
                    return Err(rule("the line head moved: the job's pre-state is stale".into()));
                }
                let states = boundary_states_v1(writers, &line.head, &c.steps)
                    .ok_or_else(|| rule("a step's commitments do not name a slot's write at its last position".into()))?;
                for (i, st) in states.iter().enumerate() {
                    if c.step_roots[i] != memory_root_v1(&root.slots, st) {
                        return Err(rule(format!("boundary root {i} is not the committed traces' (a fabricated memory state)")));
                    }
                }
                // The post-state is public before it can become the head: opened here, against what the traces commit.
                check_post_state_v1(&r.program, &root.slots, states.last().expect("S + 1 states"), &c.post_state).map_err(rule)?;
                (vec![line.head.clone()], line.head_source)
            }
            (SpecClaimV1::Retrieval(c), SpecJobV1::Retrieval(_), SpecClassKindV1::Retrieval { root }) => {
                check_result_v1(root, &c.result).map_err(rule)?;
                (Vec::new(), None)
            }
            (SpecClaimV1::Composite(c), SpecJobV1::Composite(j), SpecClassKindV1::Composite { root, components }) => {
                if c.stages.len() != root.stages.len() {
                    return Err(rule("one claim stage per class stage".into()));
                }
                for (s, ((st, c_st), comp)) in root.stages.iter().zip(&c.stages).zip(components).enumerate() {
                    match (&st.input, c_st, comp) {
                        (StageInputV1::Query(q), StageClaimV1::Retrieval { query, result, payloads }, ComponentV1::Retrieval(r)) => {
                            check_query_v1(r, query).map_err(|e| rule(format!("stage {s}: {e}")))?;
                            if *q == QuerySourceV1::JobQuery && *query != j.query {
                                return Err(rule(format!("stage {s}: the query is not the job's (an edge recomputed at inclusion)")));
                            }
                            check_result_v1(r, result).map_err(|e| rule(format!("stage {s}: {e}")))?;
                            if payloads.len() != result.len()
                                || payloads.iter().zip(result).any(|(p, e)| {
                                    p.len() > r.snapshot.max_payload as usize || payload_digest_v1(p) != e.payload_digest
                                })
                            {
                                return Err(rule(format!("stage {s}: the carried payloads are not the result's")));
                            }
                        }
                        (StageInputV1::Tokens { .. }, StageClaimV1::Model { .. }, ComponentV1::Model(_)) => {}
                        _ => return Err(rule(format!("stage {s}: the claim stage is not the class stage's kind"))),
                    }
                }
                (Vec::new(), None)
            }
            _ => return Err(rule("the claim is not of its job's kind".into())),
        };
        self.reveal_ready(&job_id, &producer, &id, salt).map_err(rule)?;
        self.opv_claim_capacity(&class_id, &producer, &job_id).map_err(rule)?;
        self.charge(NAME, 0)?;
        // Then every structure a court checks first, per step / model stage.
        let job = self.typed.jobs.get(&job_id).expect("checked");
        let class = self.typed.classes.get(&class_id).expect("checked");
        match (claim, job, &class.kind) {
            (SpecClaimV1::Memory(c), SpecJobV1::Memory(j), SpecClassKindV1::Memory { rule: r, root, writers }) => {
                let states = boundary_states_v1(writers, &pre_state[0], &c.steps).expect("checked");
                for (i, step) in c.steps.iter().enumerate() {
                    let overlay = overlay_v1(&r.param_commitments, &root.slots, &states[i]);
                    let sub_job = step_job_v1(&class_id, &job_id, i as u32, &j.chunks[i]);
                    let sub_claim = step_claim_v1(&sub_job, &producer, c.generated[i], step.evidence.root());
                    let header = step_header_v1(r, &class_id, &overlay);
                    let record = step_record_v1(&id, i as u32, r, step, &overlay, &j.chunks[i]);
                    self.check_kernel_part(
                        &format!("step {i}"),
                        &sub_job,
                        &sub_claim,
                        &step.evidence,
                        &step.commitments,
                        header,
                        record,
                        r.program.token_bound,
                    )
                    .map_err(rule)?;
                }
            }
            (SpecClaimV1::Composite(c), SpecJobV1::Composite(j), SpecClassKindV1::Composite { root, components }) => {
                for (s, ((st, c_st), comp)) in root.stages.iter().zip(&c.stages).zip(components).enumerate() {
                    let (
                        StageInputV1::Tokens { sources, max_new_tokens },
                        StageClaimV1::Model { generated, evidence, commitments },
                        ComponentV1::Model(m),
                    ) = (&st.input, c_st, comp)
                    else {
                        continue;
                    };
                    let prompt =
                        stage_prompt_v1(sources, j, &c.stages).ok_or_else(|| rule(format!("stage {s}: a source of another kind")))?;
                    let sub_job = stage_job_v1(&st.component, &job_id, s as u8, prompt, *max_new_tokens);
                    sub_job
                        .well_formed(m.program.token_bound, m.plan.max_positions)
                        .map_err(|e| rule(format!("stage {s}: the stage's input is not one its model admits: {e}")))?;
                    let sub_claim = stage_claim_v1(&sub_job, &producer, generated, evidence);
                    let record = model_stage_record_v1(&id, s as u8, m, &sub_job, &sub_claim, evidence, commitments);
                    self.check_kernel_part(
                        &format!("stage {s}"),
                        &sub_job,
                        &sub_claim,
                        evidence,
                        commitments,
                        m.header(st.component),
                        record,
                        m.program.token_bound,
                    )
                    .map_err(rule)?;
                }
            }
            _ => {}
        }
        let body = ClaimBodyV1::Spec(Box::new(SpecClaimBodyV1 { claim: claim.clone(), pre_state, pre_source }));
        self.admit(id, producer, class_id, job_id, body, out).map_err(rule)?;
        self.seals.remove(&(job_id, producer));
        if let Some(salt) = salt {
            self.claim_beacon_salts.insert(id, *salt);
        }
        Ok(())
    }

    /// **The typed courts**, over ledger state and the filing only.
    pub(super) fn adjudicate_spec(&self, claim: &Digest, bytes: &[u8]) -> Result<(), String> {
        let fault: SpecFaultV1 = borsh::from_slice(bytes).map_err(|e| format!("not a typed fault: {e}"))?;
        let row = self.claims.get(claim).ok_or("no such claim")?;
        let ClaimBodyV1::Spec(body) = &row.body else { return Err("not a typed claim".into()) };
        let class = self.typed.classes.get(&row.class_binding_id).ok_or("no such class")?;
        let job = self.typed.jobs.get(&row.job_id).ok_or("no such job")?;
        let dismissed = |d: crate::verify::DismissalV1| format!("{d:?}");
        match (&body.claim, job, &class.kind, &fault) {
            (SpecClaimV1::Memory(c), SpecJobV1::Memory(j), SpecClassKindV1::Memory { rule, root, writers }, f) => {
                let step = match f {
                    SpecFaultV1::MemoryStep { step, .. } | SpecFaultV1::MemoryDecode { step, .. } => *step as usize,
                    _ => return Err("a filing for another kind of claim".into()),
                };
                let st = c.steps.get(step).ok_or("no such step")?;
                let pre = body.pre_state.first().ok_or("no pre-state")?;
                let states = boundary_states_v1(writers, pre, &c.steps).ok_or("the boundary states do not derive")?;
                let overlay = overlay_v1(&rule.param_commitments, &root.slots, &states[step]);
                match f {
                    SpecFaultV1::MemoryStep { proof, .. } => {
                        let record = step_record_v1(claim, step as u32, rule, st, &overlay, &j.chunks[step]);
                        let header = step_header_v1(rule, &row.class_binding_id, &overlay);
                        FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, header)?
                            .try_proof(proof)
                            .map(|_| ())
                            .map_err(dismissed)
                    }
                    SpecFaultV1::MemoryDecode { logits, .. } => {
                        let sub_job = step_job_v1(&row.class_binding_id, &row.job_id, step as u32, &j.chunks[step]);
                        let sub_claim = step_claim_v1(&sub_job, &row.producer, c.generated[step], st.evidence.root());
                        let fault = DecodeFaultV1 { index: 0, logits: logits.clone() };
                        verify_decode_fault_v1(
                            &sub_job,
                            &sub_claim,
                            &EvidenceV1::new(st.commitments.clone()),
                            rule.logits_at(),
                            &fault,
                        )
                        .map(|_| ())
                        .map_err(|d| format!("{d:?}"))
                    }
                    _ => unreachable!("matched above"),
                }
            }
            (
                SpecClaimV1::Retrieval(c),
                SpecJobV1::Retrieval(j),
                SpecClassKindV1::Retrieval { root },
                SpecFaultV1::Retrieval { stage: 0, fault },
            ) => judge_retrieval_fault_v1(root, &j.query, &c.result, fault),
            (SpecClaimV1::Composite(c), SpecJobV1::Composite(j), SpecClassKindV1::Composite { root, components }, f) => {
                let s = match f {
                    SpecFaultV1::Retrieval { stage, .. }
                    | SpecFaultV1::StageKernel { stage, .. }
                    | SpecFaultV1::StageDecode { stage, .. }
                    | SpecFaultV1::Edge { stage, .. } => *stage as usize,
                    _ => return Err("a filing for another kind of claim".into()),
                };
                let (st, c_st, comp) = (root.stages.get(s).ok_or("no such stage")?, &c.stages[s], &components[s]);
                match (f, c_st, comp) {
                    (
                        SpecFaultV1::Retrieval { fault, .. },
                        StageClaimV1::Retrieval { query, result, .. },
                        ComponentV1::Retrieval(r),
                    ) => judge_retrieval_fault_v1(r, query, result, fault),
                    (SpecFaultV1::Edge { logits, .. }, StageClaimV1::Retrieval { query, .. }, ComponentV1::Retrieval(_)) => {
                        let StageInputV1::Query(QuerySourceV1::StageLogits { stage: t }) = st.input else {
                            return Err("the stage's query is not an upstream value: no edge court".into());
                        };
                        let (StageClaimV1::Model { commitments, .. }, ComponentV1::Model(m)) =
                            (&c.stages[t as usize], &components[t as usize])
                        else {
                            return Err("the upstream stage is not a model".into());
                        };
                        let opened = logits.decode()?;
                        let (post, node) = m.logits_at();
                        let last = commitments.len().checked_sub(1).ok_or("an empty upstream stage")? as u32;
                        if EvidenceV1::new(commitments.clone()).at(last, post, node) != Some(&tensor_commitment(&opened)) {
                            return Err("NotAuthentic: the opened logits are not the upstream stage's committed ones".into());
                        }
                        let same = opened.data.len() == query.len() && opened.data.iter().zip(query).all(|(a, b)| *a == *b as i128);
                        if same { Err("the carried query is the upstream logits: no fault".into()) } else { Ok(()) }
                    }
                    (
                        SpecFaultV1::StageKernel { .. } | SpecFaultV1::StageDecode { .. },
                        StageClaimV1::Model { generated, evidence, commitments },
                        ComponentV1::Model(m),
                    ) => {
                        let StageInputV1::Tokens { sources, max_new_tokens } = &st.input else {
                            return Err("not a model stage".into());
                        };
                        let prompt = stage_prompt_v1(sources, j, &c.stages).ok_or("a source of another kind")?;
                        let sub_job = stage_job_v1(&st.component, &row.job_id, s as u8, prompt, *max_new_tokens);
                        let sub_claim = stage_claim_v1(&sub_job, &row.producer, generated, evidence);
                        match f {
                            SpecFaultV1::StageKernel { proof, .. } => {
                                let record = model_stage_record_v1(claim, s as u8, m, &sub_job, &sub_claim, evidence, commitments);
                                FreshVerifierV1::from_public_bytes(&record.to_bytes(), &self.known, m.header(st.component))?
                                    .try_proof(proof)
                                    .map(|_| ())
                                    .map_err(dismissed)
                            }
                            SpecFaultV1::StageDecode { index, logits, .. } => {
                                let fault = DecodeFaultV1 { index: *index, logits: logits.clone() };
                                verify_decode_fault_v1(
                                    &sub_job,
                                    &sub_claim,
                                    &EvidenceV1::new(commitments.clone()),
                                    m.logits_at(),
                                    &fault,
                                )
                                .map(|_| ())
                                .map_err(|d| format!("{d:?}"))
                            }
                            _ => unreachable!("matched above"),
                        }
                    }
                    _ => Err("the filing names a stage of another kind".into()),
                }
            }
            _ => Err("a filing for another kind of claim".into()),
        }
    }

    /// **The public tensors of a memory state**, in slot order: the carried post-state of `source`, or (`None`) the registered `M0`
    /// read from the public artifact — each checked against `expect` (`None`: unavailable or not that state). What a producer runs the
    /// next job from and what an outsider checks a step-0 pre-state with: the chain and the artifact, never a producer's copy.
    pub fn memory_state_tensors_v1(
        &self,
        class: &Digest,
        source: Option<&Digest>,
        expect: &[Digest],
        artifact: &dyn PublicArtifactV1,
    ) -> Option<Vec<misaka_palw_tir::Tensor>> {
        let Some(SpecClassKindV1::Memory { root, .. }) = self.typed.classes.get(class).map(|r| &r.kind) else { return None };
        let tensors: Vec<misaka_palw_tir::Tensor> = match source {
            Some(c) => {
                let ClaimBodyV1::Spec(b) = &self.claims.get(c)?.body else { return None };
                let SpecClaimV1::Memory(m) = &b.claim else { return None };
                m.post_state.iter().map(|w| w.decode().ok()).collect::<Option<_>>()?
            }
            None => root.slots.iter().map(|s| artifact.param(0, s.param.0, s.param.1)).collect::<Option<_>>()?,
        };
        (tensors.len() == expect.len() && tensors.iter().zip(expect).all(|(t, c)| tensor_commitment(t) == *c)).then_some(tensors)
    }

    /// **The head of a memory line, as tensors** (`None`: no such line, or `M0` absent from `artifact`).
    pub fn memory_head_tensors_v1(&self, class: &Digest, artifact: &dyn PublicArtifactV1) -> Option<Vec<misaka_palw_tir::Tensor>> {
        let line = self.typed.lines.get(class)?;
        self.memory_state_tensors_v1(class, line.head_source.as_ref(), &line.head, artifact)
    }

    /// `derived[occurrence][node]` of a typed claim's demand stage.
    pub(super) fn spec_derived_mask(&self, row: &ClaimRowV1, body: &SpecClaimBodyV1, stage: u8) -> Vec<Vec<bool>> {
        match (self.typed.classes.get(&row.class_binding_id).map(|c| &c.kind), &body.claim) {
            (Some(SpecClassKindV1::Memory { rule, .. }), _) if stage == 0 => derived_mask_v1(&rule.program),
            (Some(SpecClassKindV1::Memory { root, .. }), _) if stage == MEMORY_PRE_STATE_STAGE_V1 => {
                vec![vec![false; root.slots.len()]]
            }
            (Some(SpecClassKindV1::Composite { components, .. }), _) => match components.get(stage as usize) {
                Some(ComponentV1::Model(m)) => derived_mask_v1(&m.program),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// The retrieval root behind a typed claim's snapshot-slice stage (`0x80 + s`), if that stage is one.
    fn slice_root(&self, row: &ClaimRowV1, stage: u8) -> Option<&crate::spec::retrieval::RetrievalRootV1> {
        let s = stage.checked_sub(SNAPSHOT_STAGE_BASE_V1)? as usize;
        match &self.typed.classes.get(&row.class_binding_id)?.kind {
            SpecClassKindV1::Retrieval { root } if s == 0 => Some(root),
            SpecClassKindV1::Composite { components, .. } => match components.get(s)? {
                ComponentV1::Retrieval(r) => Some(r),
                ComponentV1::Model(_) => None,
            },
            _ => None,
        }
    }

    /// Whether `(stage, position)` is a snapshot slice of a typed claim (demandable though no claim commits it: it is the snapshot's), or
    /// one of its retrieved entries (`0xC0 + s`, GAP-52: the claim's own stated output).
    pub(super) fn spec_slice_demandable(&self, row: &ClaimRowV1, stage: u8, position: u32) -> bool {
        matches!(row.body, ClaimBodyV1::Spec(_))
            && (self.slice_root(row, stage).is_some_and(|r| (position as u64) < r.snapshot.slices())
                || self.entry_of(row, stage, position).is_some())
    }

    /// **Retrieved entry `position` of a typed claim's retrieval stage** (`0xC0 + s`), with the root it is opened against: a plain
    /// retrieval claim's result (`s = 0`), or a composite claim's retrieval stage `s`.
    fn entry_of(&self, row: &ClaimRowV1, stage: u8, position: u32) -> Option<(&RetrievalRootV1, RetrievedV1)> {
        let s = stage.checked_sub(RETRIEVAL_ENTRY_STAGE_BASE_V1)? as usize;
        let ClaimBodyV1::Spec(body) = &row.body else { return None };
        match (&self.typed.classes.get(&row.class_binding_id)?.kind, &body.claim) {
            (SpecClassKindV1::Retrieval { root }, SpecClaimV1::Retrieval(c)) if s == 0 => {
                Some((root, *c.result.get(position as usize)?))
            }
            (SpecClassKindV1::Composite { components, .. }, SpecClaimV1::Composite(c)) => {
                match (components.get(s)?, c.stages.get(s)?) {
                    (ComponentV1::Retrieval(root), StageClaimV1::Retrieval { result, .. }) => {
                        Some((root, *result.get(position as usize)?))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The classification of a slice response (`None`: not a slice stage — the position path classifies it).
    pub(super) fn spec_classify_slice(
        &self,
        row: &ClaimRowV1,
        stage: u8,
        position: u32,
        bytes: &[u8],
    ) -> Option<Result<ServedPositionV1, &'static str>> {
        if !matches!(row.body, ClaimBodyV1::Spec(_)) {
            return None;
        }
        if let Some((root, entry)) = self.entry_of(row, stage, position) {
            return Some(classify_entry_response_v1(root, &entry, bytes));
        }
        self.slice_root(row, stage).map(|r| classify_slice_response_v1(r, position as u64, bytes))
    }

    /// A claim reached Final: a memory claim moves its line if the head is still its pre-state (else it is superseded).
    pub(super) fn spec_on_final(&mut self, claim: &Digest) {
        let Some(row) = self.claims.get(claim) else { return };
        let ClaimBodyV1::Spec(body) = &row.body else { return };
        let SpecClaimV1::Memory(c) = &body.claim else { return };
        let Some(SpecClassKindV1::Memory { root, writers, .. }) = self.typed.classes.get(&row.class_binding_id).map(|r| &r.kind)
        else {
            return;
        };
        let (Some(pre), Some(until)) = (body.pre_state.first(), row.liability_until) else { return };
        let Some(states) = boundary_states_v1(writers, pre, &c.steps) else { return };
        let slots = root.slots.clone();
        let (pre, post, class) = (pre.clone(), states.last().cloned().unwrap_or_default(), row.class_binding_id);
        if let Some(line) = self.typed.lines.get_mut(&class) {
            line.on_final(&slots, *claim, &pre, post, until);
        }
    }

    /// A claim was convicted after Final: a memory line it advanced rolls back to its pre-state.
    pub(super) fn spec_on_post_final_conviction(&mut self, claim: &Digest) {
        let Some(row) = self.claims.get(claim) else { return };
        if !matches!(row.body, ClaimBodyV1::Spec(_)) {
            return;
        }
        let class = row.class_binding_id;
        let Some(SpecClassKindV1::Memory { root, .. }) = self.typed.classes.get(&class).map(|r| &r.kind) else { return };
        let slots = root.slots.clone();
        if let Some(line) = self.typed.lines.get_mut(&class) {
            line.on_post_final_conviction(&slots, claim);
        }
    }

    /// Advances past their liability horizon can no longer be reached by a conviction.
    pub(super) fn spec_prune_lines(&mut self, daa: u64) {
        for line in self.typed.lines.values_mut() {
            line.prune(daa);
        }
    }
}
