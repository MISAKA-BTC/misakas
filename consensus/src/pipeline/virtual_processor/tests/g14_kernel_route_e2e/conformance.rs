//! **Onboarding P0 on the real node: conformance evidence carried on chain (tag 109).**
//!
//! ```text
//! SDK-signed envelope (108) → V2 class → 104 bind → kernel class (OPV) → 106 kernel-bind → 107 commit (an attempt, counted)
//!   → FUTURE OPV Finals of ANOTHER class fold (FinalPathV1::PanelIndependent, op 212's rows) → the beacon locks
//!   → 109 Post (chunked): judged in the fold against the chain's own beacon and seed, rebuilt from its outcomes
//!   → challenge window: an outsider's 109 Refute (LeafDecode from the public artifact / VectorTokens from a Final kernel claim)
//!   → closes unrefuted → CONFORMANCE_PASSED → G14_ELIGIBLE → the gate (binding Final) → V2 Active → ACTIVE_REWARDABLE
//! ```
//!
//! Every beacon here is folded from real OPV Finals on the test chain (the fences test-armed through the harness's `Config` seam, as
//! the rest of this file). Nothing synthetic reaches the fold: the evidence's material is what a pack would compute — leaf outcomes
//! from the artifact's own bytes, vector tokens from the class's own greedy run; only the logits / commits digests (`a`, `b`) of a
//! vector are stand-ins, because the chain has no court for them (stated residual).

use super::*;
use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, open_artifact_leaf_v1};
use kaspa_consensus_core::palw_conformance_evidence_v1::{
    CheckOutcomeV1, ConformanceEvidenceActionV1, ConformanceEvidencePostV1, ConformanceFaultV1, ConformanceScopeV1, FreshInputV1,
    PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1, PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1, ResultV1, SelectionV1, assemble_evidence_v1,
    derive_selection_v1, fresh_verify_v1, openings_root_v1, palw_onboarding_challenge_policy_v1, reference_leaf_result_v1,
};
use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1;
use kaspa_consensus_core::palw_onboarding_v1::{
    ConformanceAttemptEndV1, ConformanceAttemptRowV1, PalwOnboardingGateV1, conformance_gate_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwClassStatusV2;
use misaka_palw_challenge::{
    ConformanceCommitmentV1, OnboardingFailureV1 as F, OnboardingStateV1 as S, RootV1, SubjectKindV1, WorkBeaconStateV1,
    challenge_seed_v1, collect_attributed_work_beacon_v1,
};
use misaka_palw_sdk::onboarding_chain::{
    EnvelopeSigner, PublicConformanceReadsV1, SignedRegistrationRequestV1, conformance_evidence_object_v1, fresh_verify_from_reads_v1,
};

const REGISTRANT: usize = 1;
const OUTSIDER: usize = 3;
/// A bond that refutes wrongly first (C4 F-C4R4-11: it is judged once per window, so the convicting refutation is another bond's).
const JUNK_REFUTER: usize = 5;
/// Producers of the source class's OPV claims (and of the outsider's kernel claim), rotated: a producer holds at most three live OPV
/// claims, and a claim's reservation lives to Final + liability.
const PRODUCERS: [usize; 6] = [0, 2, 4, 5, 6, 7];

/// The committed scope: two vectors (prompts of 1..=3 tokens, two decoded) and two artifact leaves, at the densest fault the
/// unreviewed test soundness policy approves — exactly the interim policy's 2 bits.
fn test_scope() -> ConformanceScopeV1 {
    let mut s = ConformanceScopeV1::new(2, 3, 2, 2);
    (s.vector_fault_ppm, s.leaf_fault_ppm) = (1_000_000, 1_000_000);
    s
}

/// The onboarding network with RFC-0015's OPV fence (armed WITHOUT its validation, as everywhere in this file). The beacon's source
/// class and the candidate's kernel class are treated as eligible through the processor's `cfg(test)` seam: these conformance
/// MECHANICS predate derived eligibility; the bootstrap that derives it is `opv_bootstrap.rs`.
///
/// The fence also arms G14-for-rewards (`palw_reward_gate_v1`), whose E6 compares the passed attempt's EFFECTIVE bits with
/// `min_effective_bits`: the interim sampled policy is 2 bits (0 effective), so these mechanics run with the DRILL floor `floor`
/// (0 here; [`Cw::over_floor`] states another) — the ruled 128 is asserted where it decides (`g14_rewards_*`).
fn conformance_config(eligible: Vec<Hash64>, floor: u16) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = kernel_config_onboarding();
    opv_test_eligible(&eligible);
    let mut params = config.params.clone();
    let mut fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new());
    fence.min_effective_bits = floor;
    // GAP-70: on the drill floor a sampled conformance is also allowed to gate (refused by validation on any real network); on the
    // ruled floor the release terms hold — only the complete check gates rewards.
    fence.sampled_conformance_gates_reward = floor == 0;
    params.palw_panel_free_v1 = Some(fence);
    assert!(params.validate_palw_v2().is_err(), "the real validation still refuses the fences");
    (Config::new(params), bundle, premine, floats)
}

fn opv_class_id(program: &TirProgramV1, plan: &misaka_palw_kernel::VerificationPlanV1, pc: &ParamCommitmentsV1) -> Digest {
    single_class_id_v1(k2_tir_v2_descriptor().digest(), &program.encode(), plan, pc, VerificationModeV1::OptimisticPublicVerification)
}

/// A harness card's key behind the SDK's signer trait (throwaway test keys; the SDK never sees a seed).
struct CardSigner {
    card: u64,
    pubkey: Vec<u8>,
}

impl EnvelopeSigner for CardSigner {
    fn public_key(&self) -> Vec<u8> {
        self.pubkey.clone()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        let key = TestConsensus::palw_v2_registry_keypair(self.card);
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, message, context, [0x42; 32])
            .map(|s| s.as_ref().to_vec())
            .map_err(|e| format!("{e:?}"))
    }
}

/// **The conformance world**: the beacon's source class (OPV, `wide128_v1(7)`), the candidate V2 class (`onb_fixture(11)`) registered
/// through the SDK-signed envelope, bound, and kernel-bound to its own OPV kernel class.
struct Cw {
    net: Net,
    src: Fixture,
    src_class: Digest,
    f: OnbFixture,
    /// The candidate as a kernel fixture (its greedy run is the truth a Final kernel claim states).
    cand: Fixture,
    v2_class: Hash64,
    kernel_class: Digest,
    kernel_root: Hash64,
    /// The candidate's V2 inventory — the public artifact a refuter reads leaves from.
    ops: Vec<PalwArtifactOperandV1>,
    jobs: u8,
    turn: usize,
}

impl Cw {
    async fn new() -> Cw {
        Cw::over(|c| TestConsensus::new(c)).await
    }

    async fn over(make: impl FnOnce(&Config) -> TestConsensus) -> Cw {
        Cw::over_floor(make, 0).await
    }

    /// [`Cw::over`] on a network whose OPV fence states the effective-bits floor `floor` (the ruled value is 128).
    async fn over_floor(make: impl FnOnce(&Config) -> TestConsensus, floor: u16) -> Cw {
        use super::super::g14_registration_e2e::Tensors;
        use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
        let src = fixture();
        let f = onb_fixture(11);
        let src_class = opv_class_id(&src.program, &src.plan, &src.pc);
        let kernel_class = opv_class_id(&f.program, &f.plan, &f.pc);
        let mut net =
            Net::over_cfg(conformance_config(vec![Hash64::from_bytes(src_class), Hash64::from_bytes(kernel_class)], floor), make);
        net.beat_to(1).await;
        // ---- the beacon's source: a pre-existing OPV class (its artifact attested by the harness hook, §3) ----
        let d = k2_tir_v2_descriptor();
        let register_src = K::RegisterClassV2 {
            mode: VerificationModeV1::OptimisticPublicVerification,
            descriptor: d.digest(),
            program_bytes: src.program.encode(),
            plan: src.plan.clone(),
            param_commitments: src.pc.clone(),
        };
        let o = net.route(REGISTRANT, &register_src);
        net.send(vec![(REGISTRANT, o)]).await;
        assert!(net.ledger().opv.classes.contains(&src_class), "the source class registered under OPV");
        // ---- the candidate's V2 class, registered through the SDK-signed envelope (tag 108) ----
        let class_obj = net.v2_registration(&f, REGISTRANT, net.daa() + 30);
        let Obj::ClassRegisteredTirV1 { class_id: v2_class, .. } = &class_obj else { unreachable!() };
        let v2_class = *v2_class;
        let now = net.daa();
        let request = SignedRegistrationRequestV1::for_network(class_obj, net.bond(REGISTRANT), now, now + 200, &net.config.params)
            .expect("a registration");
        let envelope = request.sign(&net.signer(REGISTRANT)).expect("the SDK signs the envelope");
        net.send(vec![(REGISTRANT, envelope)]).await;
        assert!(
            matches!(net.chain.tip_state().1.class(&v2_class).map(|c| &c.status), Some(PalwClassStatusV2::Registered { .. })),
            "the node accepted the SDK-signed envelope and registered the class it wraps"
        );
        // ---- bind (104), mature, the candidate's OPV kernel class, kernel-bind (106) ----
        let kernel_root = Hash64::from_bytes(f.pc.root());
        let o = net.artifact_bound(REGISTRANT, v2_class, kernel_root);
        net.send(vec![(REGISTRANT, o)]).await;
        let matures = net.api().unwrap().artifact_binding_v1(&v2_class, &kernel_root).unwrap().matures_daa;
        net.beat_to(matures).await;
        let register_cand = K::RegisterClassV2 {
            mode: VerificationModeV1::OptimisticPublicVerification,
            descriptor: d.digest(),
            program_bytes: f.program.encode(),
            plan: f.plan.clone(),
            param_commitments: f.pc.clone(),
        };
        let o = net.route(REGISTRANT, &register_cand);
        net.send(vec![(REGISTRANT, o)]).await;
        assert!(net.ledger().classes.contains_key(&kernel_class), "the candidate's kernel class registered over the matured binding");
        let o = net.kernel_bound(REGISTRANT, v2_class, Hash64::from_bytes(kernel_class));
        net.send(vec![(REGISTRANT, o)]).await;
        assert!(net.api().unwrap().kernel_binding_v1(&v2_class).is_some(), "kernel-bound");
        let tensors = f.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect();
        let ops = palw_tir_inventory_operands_v1(&f.program, &Tensors(tensors)).expect("the inventory");
        let cand = Fixture { program: f.program.clone(), params: f.params.clone(), plan: f.plan.clone(), pc: f.pc.clone() };
        Cw { net, src, src_class, f, cand, v2_class, kernel_class, kernel_root, ops, jobs: 0, turn: 0 }
    }

    fn producer(&mut self) -> usize {
        self.turn += 1;
        PRODUCERS[self.turn % PRODUCERS.len()]
    }

    /// The RFC-0013 statement over the chain's roots, the committed scope and an implementation set named by `implementation`.
    fn commitment(&self, implementation: u8) -> ConformanceCommitmentV1 {
        let route = self.net.api().expect("the route");
        let binding = route.kernel_binding_v1(&self.v2_class).expect("kernel-bound");
        ConformanceCommitmentV1 {
            version: 1,
            chain_genesis: self.net.config.params.genesis.hash.as_bytes(),
            ruleset_id: route.header.policy.ruleset_digest,
            subject_kind: SubjectKindV1::ModelConformance,
            candidate_id: self.v2_class.as_bytes(),
            kernel_descriptor_id: k2_tir_v2_descriptor().digest(),
            challenge_policy_id: binding.challenge_policy_id.as_bytes(),
            artifact_root: self.f.artifact_root.as_bytes(),
            program_root: program_root_v1(&self.f.program.encode()),
            source_root: RootV1::Absent,
            tokenizer_or_input_schema_root: RootV1::Absent,
            layout_root: [0x11; 64],
            verification_plan_root: self.f.plan.root(),
            constraint_root: RootV1::Absent,
            implementation_set_root: [implementation; 64],
            test_scope_root: test_scope().root(),
            calibration_id: RootV1::Absent,
            input_and_state_binding_root: RootV1::Absent,
            resource_profile_id: [0x44; 64],
            commitment_object_id: None,
            canonical_commitment_position: None,
        }
    }

    async fn commit(&mut self, implementation: u8) -> ConformanceCommitmentV1 {
        let c = self.commitment(implementation);
        let o = self.net.conformance_committed(REGISTRANT, c.clone());
        self.net.send(vec![(REGISTRANT, o)]).await;
        let a = self.attempt();
        assert!(a.open() && a.commitment == c, "the commitment opened an attempt");
        c
    }

    fn attempt(&self) -> ConformanceAttemptRowV1 {
        self.net.api().expect("the route").conformance_attempt_v1(&self.v2_class).expect("a conformance record")
    }

    /// **`n` honest OPV claims of the SOURCE class**, committed now (after the attempt's start): each a job by the registrant, sealed
    /// and revealed by a rotating producer. They reach Final with no Panel when their window closes.
    async fn source_claims(&mut self, n: usize) -> Vec<Digest> {
        let mut jobs = Vec::new();
        for _ in 0..n {
            self.jobs += 1;
            jobs.push(KernelJobV1 {
                class_binding_id: self.src_class,
                prompt: vec![3, 17, 9],
                max_new_tokens: 3,
                decode: DecodeRuleV1::Greedy,
                nonce: [self.jobs; 64],
            });
        }
        let posts: Vec<(usize, Obj)> =
            jobs.iter().map(|j| (REGISTRANT, self.net.route(REGISTRANT, &K::PostJob { job: j.clone() }))).collect();
        self.net.send(posts).await;
        let src = self.src.clone_fixture();
        let class = self.src_class;
        self.claims_of(&src, class, &jobs).await
    }

    /// Honest claims of `jobs` (of `class`, whose fixture is `fx`) by rotating producers: sealed in one block, revealed in the next.
    async fn claims_of(&mut self, fx: &Fixture, class: Digest, jobs: &[KernelJobV1]) -> Vec<Digest> {
        let ledger = self.net.ledger();
        let mut seals = Vec::new();
        let mut reveals = Vec::new();
        let mut ids = Vec::new();
        for job in jobs {
            let producer = self.producer();
            let generated = greedy(fx, &ledger, &class, &job.prompt, job.max_new_tokens as usize);
            let produced = produce(fx, &ledger, &class, job, self.net.kid(producer), generated, |_| {});
            let id = produced.claim.id();
            let (seal, reveal) = seal_and_reveal(&ledger, self.net.kid(producer), &produced.object);
            seals.push((producer, self.net.route(producer, &seal)));
            reveals.push((producer, reveal));
            ids.push(id);
        }
        self.net.send(seals).await;
        let reveals: Vec<(usize, Obj)> = reveals.into_iter().map(|(p, o)| (p, self.net.route(p, &o))).collect();
        self.net.send(reveals).await;
        let ledger = self.net.ledger();
        for id in &ids {
            assert!(ledger.claims.contains_key(id), "the claim committed");
        }
        ids
    }

    /// Beat until the attempt's beacon locks (k = 2 future Panel-independent Finals, then depth D).
    async fn until_locked(&mut self) -> u64 {
        let ttpb = self.net.ttpb();
        for _ in 0..400 {
            let read = self.read();
            // Locked at the sink (what the next block's fold sees), not only at the virtual's DAA the read is taken at.
            let policy = palw_onboarding_challenge_policy_v1();
            let at_sink =
                collect_attributed_work_beacon_v1(&self.attempt().beacon_context(&policy), &self.events(), self.net.daa()).unwrap();
            if let (WorkBeaconStateV1::Locked(b), "LOCKED") = (at_sink, read.beacon) {
                return b.lock_position;
            }
            assert!(read.beacon != "UNAVAILABLE", "the beacon must not run out of window here");
            self.net.chain.heartbeat(ttpb, Vec::new()).await;
        }
        panic!("the beacon never locked: {:?}", self.read().beacon)
    }

    fn read(&self) -> kaspa_consensus_core::palw_onboarding_v1::ConformanceEvidenceReadV1 {
        self.net.chain.ctx.consensus.palw_conformance_evidence_v1(self.v2_class).expect("the class is known")
    }

    /// The Final facts the route serves (op 212's source), decoded: attributed events (the producer stands behind each).
    fn events(&self) -> Vec<misaka_palw_challenge::AttributedWorkV1> {
        self.net
            .api()
            .expect("the route")
            .finals_read_v1()
            .expect("the rows rebuild")
            .into_iter()
            .filter_map(|f| f.event.map(|e| borsh::from_slice(&e).expect("a beacon event")))
            .collect()
    }

    /// The seed and selection of the CURRENT attempt, derived as any outsider derives them: the attempt row and the Finals.
    fn selection(&self) -> (misaka_palw_challenge::beacon::VerifiedWorkBeaconV1, [u8; 64], SelectionV1) {
        let attempt = self.attempt();
        let policy = palw_onboarding_challenge_policy_v1();
        let ctx = attempt.beacon_context(&policy);
        let WorkBeaconStateV1::Locked(beacon) = collect_attributed_work_beacon_v1(&ctx, &self.events(), self.net.daa()).unwrap()
        else {
            panic!("the beacon is locked")
        };
        let seed = challenge_seed_v1(&ctx, &attempt.commitment.subject(), &beacon).expect("a seed");
        let selection = derive_selection_v1(&seed, &policy, &test_scope(), &self.f.program).expect("a selection");
        (beacon, seed, selection)
    }

    /// **What a pack computes for the current attempt**: every selected check run, each outcome edited by `edit` (a forgery or a
    /// failure), assembled into the evidence.
    fn post(&self, edit: impl Fn(&str, &mut CheckOutcomeV1)) -> ConformanceEvidencePostV1 {
        let attempt = self.attempt();
        let policy = palw_onboarding_challenge_policy_v1();
        let (beacon, seed, selection) = self.selection();
        let ledger = self.net.ledger();
        let mut outcomes = Vec::new();
        for v in &selection.vectors {
            let tokens = greedy(&self.cand, &ledger, &self.kernel_class, &v.prompt, v.decode as usize);
            let a = misaka_palw_kernel::hash::object_id(b"test/logits-digest-stand-in", &tokens);
            let mut a32 = [0u8; 32];
            a32.copy_from_slice(&a[..32]);
            let ran = ResultV1::Ran { a: a32, b: [7; 32], tokens, positions: v.prompt.len() as u32 + v.decode };
            outcomes.push(CheckOutcomeV1 {
                check_id: v.check_id.clone(),
                reference: ran.clone(),
                independent: ran.clone(),
                backend: ran,
                disagreement: None,
            });
        }
        for l in &selection.leaves {
            let bytes = &self.ops[l.leaf_index as usize].bytes;
            let a = reference_leaf_result_v1(&self.f.program, l.param, bytes).expect("the leaf decodes");
            let w = self.f.program.params[l.param as usize].dtype.width();
            let ran = ResultV1::Ran { a, b: [0; 32], tokens: vec![], positions: (bytes.len() / w) as u32 };
            outcomes.push(CheckOutcomeV1 {
                check_id: l.check_id.clone(),
                reference: ran.clone(),
                independent: ran.clone(),
                backend: ran,
                disagreement: None,
            });
        }
        for o in outcomes.iter_mut() {
            let id = o.check_id.clone();
            edit(&id, o);
        }
        let map = outcomes.iter().map(|o| (o.check_id.clone(), o.clone())).collect();
        let evidence = assemble_evidence_v1(
            &attempt.commitment,
            &policy,
            &test_scope(),
            &beacon,
            &seed,
            &selection,
            &openings_root_v1(&None),
            &map,
        );
        ConformanceEvidencePostV1 { evidence, scope: test_scope(), outcomes }
    }

    /// The tag-109 object `card` signs (the SDK's builder).
    fn evidence_object(&self, card: usize, action: ConformanceEvidenceActionV1) -> Obj {
        conformance_evidence_object_v1(self.net.domain, self.v2_class, action, self.net.bond(card), &self.net.signer(card))
            .expect("the SDK signs")
    }

    /// The registrant posts `post`, cut into `ObjectChunk`s of at most `cap` bytes when given.
    async fn send_post(&mut self, post: ConformanceEvidencePostV1, cap: Option<usize>) {
        let o = self.evidence_object(REGISTRANT, ConformanceEvidenceActionV1::Post(Box::new(post)));
        match cap {
            None => {
                self.net.send(vec![(REGISTRANT, o)]).await;
            }
            Some(cap) => {
                let chunks = kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, cap)
                    .expect("chunks")
                    .expect("the evidence is larger than one chunk");
                assert!(chunks.len() >= 2, "a genuinely multi-chunk delivery: {}", chunks.len());
                self.net.send(chunks.into_iter().map(|c| (REGISTRANT, c)).collect()).await;
            }
        }
    }

    async fn refute(&mut self, card: usize, fault: ConformanceFaultV1) {
        let evidence_id = self.attempt().evidence.expect("posted").evidence_id;
        let o = self.evidence_object(card, ConformanceEvidenceActionV1::Refute { evidence_id, fault: Box::new(fault) });
        self.net.send(vec![(card, o)]).await;
    }

    /// A leaf refutation of selected leaf `j` (index into the selection's leaves): its TRUE opening from the public artifact.
    fn leaf_fault(&self, selection: &SelectionV1, j: usize) -> ConformanceFaultV1 {
        let leaf = &selection.leaves[j];
        let opening = open_artifact_leaf_v1(&self.ops, leaf.leaf_index).expect("an opening of the public artifact");
        ConformanceFaultV1::LeafDecode { check: (selection.vectors.len() + j) as u32, opening }
    }

    fn gate(&self) -> PalwOnboardingGateV1 {
        self.read().gate
    }

    fn budget_row(&self) -> Option<Vec<u8>> {
        self.net.api().and_then(|r| r.aux.get(&(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, Vec::new())).cloned())
    }

    /// **The fresh verifier**, from public reads only (op 231's rows and program, op 212's Finals) and the public artifact.
    fn fresh(&self) -> (kaspa_consensus_core::palw_conformance_evidence_v1::FreshVerdictV1, bool) {
        let read = self.read();
        let events = self.events();
        // The SDK path (what `misaka model onboard verify` runs), without the artifact file.
        let reads = PublicConformanceReadsV1 {
            attempt_row: read.attempt_row.clone().expect("the attempt row"),
            evidence_row: read.evidence_row.clone(),
            events: events.clone(),
            sealed_sources: Vec::new(),
            tip_daa: self.net.daa(),
            program: read.program.clone().expect("the class's program"),
        };
        let report = fresh_verify_from_reads_v1(&reads, None).expect("the SDK verifier runs");
        // The same verifier with the public artifact's leaves (consensus-core's one implementation, a leaf source over the artifact).
        let attempt: ConformanceAttemptRowV1 = borsh::from_slice(&reads.attempt_row).unwrap();
        let post: Option<ConformanceEvidencePostV1> = reads.evidence_row.as_ref().map(|b| borsh::from_slice(b).unwrap());
        let program = TirProgramV1::decode_canonical(&reads.program).unwrap();
        let policy = palw_onboarding_challenge_policy_v1();
        let ctx = attempt.beacon_context(&policy);
        let leaf = |l: &kaspa_consensus_core::palw_conformance_evidence_v1::SelectedLeafV1| {
            Some(self.ops.get(l.leaf_index as usize)?.bytes.clone())
        };
        let verdict = fresh_verify_v1(&FreshInputV1 {
            commitment: &attempt.commitment,
            policy: &policy,
            ctx: &ctx,
            events: &events,
            sealed: &[],
            tip_daa: self.net.daa(),
            program: &program,
            post: post.as_ref(),
            leaf_source: Some(&leaf),
        });
        assert_eq!(
            (verdict.beacon_output, verdict.seed, &verdict.posted),
            (report.verdict.beacon_output, report.verdict.seed, &report.verdict.posted)
        );
        (verdict, report.agrees)
    }
}

impl Net {
    /// Card `card`'s key behind the SDK's signer trait, with the public key the chain registered for its bond.
    fn signer(&self, card: usize) -> CardSigner {
        let pubkey = self.chain.tip_state().1.bond(&self.bond(card)).expect("the card's bond").pubkey.clone();
        CardSigner { card: card as u64, pubkey }
    }
}

impl Fixture {
    fn clone_fixture(&self) -> Fixture {
        Fixture { program: self.program.clone(), params: self.params.clone(), plan: self.plan.clone(), pc: self.pc.clone() }
    }
}

/// An attempt row without its judged-refuter set (C4 F-C4R4-11): what a dismissed refutation leaves unchanged.
fn decided(a: &ConformanceAttemptRowV1) -> ConformanceAttemptRowV1 {
    ConformanceAttemptRowV1 { refuters_judged: Vec::new(), ..a.clone() }
}

fn state(cw: &Cw) -> (S, Option<F>, u32) {
    let a = cw.attempt();
    (a.record.state, a.record.last_failure, a.record.attempts())
}

// ---- the happy path ---------------------------------------------------------------------------------------------------------

/// **The whole path on the real node.** The SDK signs the registration envelope; the class binds, kernel-binds and commits; the
/// commitment alone waits (BEACON_UNAVAILABLE: no lock, so no seed and no evidence can be about it); future OPV Finals of ANOTHER class
/// fold and the beacon locks; the evidence rides `ObjectChunk`s and is judged in the fold on the assembled whole; the class is held
/// through the challenge window (a second evidence refused, an honest-leaf "refutation" dismissed and charged, the registrant cannot
/// refute itself); the fresh verifier — public reads and the public artifact only — agrees; the window closes unrefuted:
/// CONFORMANCE_PASSED → G14_ELIGIBLE; the gate still waits for the binding's horizon (AVAILABILITY_REQUIRED); then the V2 class
/// activates and the record says ACTIVE_REWARDABLE. A second node replaying the chain reaches every root.
#[tokio::test]
async fn g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;
    let commitment = cw.commit(0x22).await;
    let attempt = cw.attempt();
    assert_eq!(
        (attempt.challenge_epoch, attempt.excluded_profiles.len()),
        (0, 3),
        "the candidate, its own kernel class and that class under the other mode are excluded (OPV-BOOT)"
    );
    assert_eq!(attempt.eligible_profiles, vec![Hash64::from_bytes(cw.src_class)], "the source class, frozen at the commitment");
    // The gate names its first unmet condition (here the binding's horizon); the conformance record's own hold is the contract's
    // BEACON_UNAVAILABLE — waiting for randomness, never a pass.
    assert!(matches!(cw.gate(), PalwOnboardingGateV1::Held { .. }), "{:?}", cw.gate());
    assert!(matches!(conformance_gate_v1(&attempt), PalwOnboardingGateV1::Held { code: "BEACON_UNAVAILABLE", .. }));
    assert_eq!(cw.read().beacon, "COLLECTING");

    // ---- future work folds: two OPV claims of the source class, Final with no Panel ----
    cw.source_claims(2).await;
    let lock = cw.until_locked().await;
    assert!(cw.read().beacon_output.is_some());
    let sources: Vec<misaka_palw_challenge::WorkFinalEventV1> =
        cw.events().into_iter().map(|w| w.event).filter(|e| e.source_profile_id == cw.src_class).collect();
    assert_eq!(sources.len(), 2);
    assert!(
        sources.iter().all(|e| e.final_path == misaka_palw_challenge::FinalPathV1::PanelIndependent
            && e.accepted_position >= attempt.committed_daa + 2)
    );
    assert!(lock >= sources.iter().map(|e| e.settlement_position).max().unwrap() + 2, "the lock is D past the k-th settlement");

    // ---- the evidence, chunked ----
    let post = cw.post(|_, _| {});
    let (_, _, selection) = cw.selection();
    assert_eq!((selection.vectors.len(), selection.leaves.len()), (2, 2));
    let evidence_id = Hash64::from_bytes(post.evidence.id());
    cw.send_post(post.clone(), Some(1024)).await;
    let posted = cw.attempt().evidence.expect("the fold accepted the assembled evidence");
    assert_eq!(posted.evidence_id, evidence_id);
    assert_eq!(posted.window_end_daa, posted.posted_daa + PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1);
    assert_eq!(state(&cw), (S::ChallengePending, None, 0), "posting decides nothing: verification and the window do");
    assert!(matches!(conformance_gate_v1(&cw.attempt()), PalwOnboardingGateV1::Held { code: "CHALLENGE_PENDING", .. }));
    assert!(matches!(cw.gate(), PalwOnboardingGateV1::Held { .. }), "{:?}", cw.gate());
    let read = cw.read();
    assert_eq!(
        borsh::from_slice::<ConformanceEvidencePostV1>(read.evidence_row.as_ref().unwrap()).unwrap(),
        post,
        "op 231 serves the material"
    );

    // ---- inside the window: a second evidence is refused; an honest leaf proves nothing (dismissed, charged); no self-refutation ----
    let rows = cw.net.api().unwrap().aux.clone();
    cw.send_post(post.clone(), None).await;
    assert_eq!(cw.net.api().unwrap().aux, rows, "one evidence per attempt: the second is dropped, nothing written");
    let budget = cw.budget_row();
    cw.refute(OUTSIDER, cw.leaf_fault(&selection, 0)).await;
    assert_eq!(cw.attempt().evidence, Some(posted), "a refutation of a true outcome is dismissed");
    assert_eq!(state(&cw).0, S::ChallengePending);
    assert_ne!(cw.budget_row(), budget, "and it spent the block's adjudication budget");
    let before = cw.attempt();
    cw.refute(REGISTRANT, cw.leaf_fault(&selection, 1)).await;
    assert_eq!(cw.attempt(), before, "the registrant's own operator cannot refute");

    // ---- the fresh verifier agrees ----
    let (fresh, agrees) = cw.fresh();
    assert_eq!(fresh.posted, Some(Ok(())), "bound, rebuilt exactly, a pass");
    assert_eq!(
        (fresh.leaves_selected, fresh.leaves_rechecked, fresh.leaf_faults.len()),
        (2, 2, 0),
        "every selected leaf re-read agrees"
    );
    assert!(agrees, "an open attempt with nothing to refute: the verifier and the chain agree");

    // ---- the window closes unrefuted ----
    cw.net.beat_to(posted.window_end_daa - 1).await;
    assert_eq!(state(&cw).0, S::ChallengePending, "nothing passes before the window closes");
    cw.net.beat_to(posted.window_end_daa + 1).await;
    let a = cw.attempt();
    assert_eq!(a.record.state, S::G14Eligible, "CONFORMANCE_PASSED, then the kernel class's public-prosecution step");
    assert_eq!(a.record.conformance_evidence_id, Some(evidence_id.as_bytes()));
    let (fresh, agrees) = cw.fresh();
    assert_eq!(fresh.posted, Some(Ok(())));
    assert!(agrees, "the chain passed it and the fresh verifier finds bound, passing, unrefuted evidence");
    let binding = cw.net.api().unwrap().artifact_binding_v1(&cw.v2_class, &cw.kernel_root).unwrap();
    if cw.net.daa() < binding.final_daa {
        assert!(matches!(cw.gate(), PalwOnboardingGateV1::Held { code: "AVAILABILITY_REQUIRED", .. }), "{:?}", cw.gate());
        assert!(matches!(cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }));
        cw.net.beat_to(binding.final_daa + 1).await;
    }
    // ---- the gate opens: Active, and the record says ACTIVE_REWARDABLE ----
    assert_eq!(cw.gate(), PalwOnboardingGateV1::Ready);
    assert!(matches!(cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().status, PalwClassStatusV2::Active));
    assert_eq!(cw.attempt().record.state, S::ActiveRewardable);
    assert_eq!(cw.attempt().commitment, commitment);
    let z = cw.net.replay().await;
    cw.net.assert_same(&z, "replay");
}

// ---- refusals: another beacon, context, policy, scope, commitment; staleness ------------------------------------------------

/// **Evidence that is not about THIS attempt decides nothing** (dismissed, rows untouched): computed against another beacon (a
/// context of another epoch), naming another policy, carrying another scope, or bound to another commitment (another implementation
/// set). Then honest evidence of a FAILING run (a disagreement): CONFORMANCE_FAILED, counted. The registrant re-commits under a new
/// implementation set (attempt 2, a new epoch, a new seed): the first attempt's evidence is stale and dismissed.
#[tokio::test]
async fn g14_conformance_evidence_against_another_beacon_policy_scope_or_commitment_is_refused_and_stale_evidence_is_invalid() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;
    cw.commit(0x22).await;
    // Before the lock there is no seed: any evidence is dismissed.
    let early_rows = cw.attempt();
    cw.source_claims(2).await;
    cw.until_locked().await;
    let good = cw.post(|_, _| {});
    let unchanged = |cw: &Cw, what: &str| assert_eq!(cw.attempt(), early_rows, "{what}: nothing written");

    // another beacon: the same statement under another epoch's context (a different accumulator and seed)
    let mut other_beacon = good.clone();
    other_beacon.evidence.beacon_output[0] ^= 1;
    cw.send_post(other_beacon, None).await;
    unchanged(&cw, "another beacon");
    let mut other_seed = good.clone();
    other_seed.evidence.challenge_seed[0] ^= 1;
    cw.send_post(other_seed, None).await;
    unchanged(&cw, "another seed (context)");
    let mut other_policy = good.clone();
    other_policy.evidence.challenge_policy_id[0] ^= 1;
    cw.send_post(other_policy, None).await;
    unchanged(&cw, "another policy");
    let mut other_scope = good.clone();
    other_scope.scope.decode_tokens += 1;
    cw.send_post(other_scope, None).await;
    unchanged(&cw, "another scope");
    let mut other_commitment = good.clone();
    let mut c2 = cw.attempt().commitment;
    c2.implementation_set_root = [0x23; 64];
    other_commitment.evidence.commitment_root = c2.statement_root();
    cw.send_post(other_commitment, None).await;
    unchanged(&cw, "another implementation set (another commitment)");

    // a failing run, honestly reported: one check's implementations disagree
    let failing = cw.post(|id, o| {
        if id == "leaf/r0/k1" {
            o.disagreement = Some("the typed backend decodes the leaf differently".into());
        }
    });
    assert_eq!(failing.evidence.status, misaka_palw_challenge::ConformanceStatusV1::Failed);
    let failed_id = failing.evidence.id();
    cw.send_post(failing, None).await;
    assert_eq!(state(&cw), (S::RegisteredDormant, Some(F::ConformanceFailed), 1), "CONFORMANCE_FAILED, counted");
    assert!(matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::EvidenceFailed, _))));
    assert!(matches!(conformance_gate_v1(&cw.attempt()), PalwOnboardingGateV1::Held { code: "CONFORMANCE_FAILED", .. }));
    assert_eq!(
        cw.read().evidence_row.map(|b| borsh::from_slice::<ConformanceEvidencePostV1>(&b).unwrap().evidence.id()),
        Some(failed_id)
    );

    // a new commitment (a changed implementation set): attempt 2, epoch 1, its own beacon; the old evidence is stale
    cw.commit(0x23).await;
    assert_eq!((cw.attempt().challenge_epoch, cw.attempt().record.attempts()), (1, 1));
    cw.source_claims(2).await;
    cw.until_locked().await;
    let attempt2 = cw.attempt();
    cw.send_post(good, None).await;
    assert_eq!(cw.attempt(), attempt2, "evidence of the first commitment is stale: dismissed");
    let fresh2 = cw.post(|_, _| {});
    cw.send_post(fresh2, None).await;
    assert!(cw.attempt().evidence.is_some(), "the new attempt's own evidence is judged and enters its window");
    let z = cw.net.replay().await;
    cw.net.assert_same(&z, "replay");
}

// ---- refutation, default, exhaustion ----------------------------------------------------------------------------------------

/// **Forged evidence is refuted from public material; withheld evidence defaults; the limit holds.** Attempt 1: the posted outcome
/// of a selected leaf is not what its bytes decode to (forged consistently, so the fold rebuilds it) — an outsider opens the leaf from
/// the public artifact: REFUTED. Attempt 2: a vector's posted tokens are not the class's greedy run — an outsider posts the prompt as
/// a job on the BOUND kernel class, an honest OPV claim of it reaches Final, and that Final contradicts the tokens: REFUTED. Attempt 3:
/// the beacon locks and nothing is posted: at the deadline the attempt DEFAULTS (never a pass). Three counted attempts: a fourth
/// commitment is refused (AttemptsExhausted) and the class never leaves Registered.
#[tokio::test]
async fn g14_conformance_forged_evidence_is_refuted_withheld_evidence_defaults_and_attempts_are_exhausted() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;

    // ---- attempt 1: a forged leaf, refuted by an opening of the public artifact ----
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let (_, _, selection) = cw.selection();
    let forged = cw.post(|id, o| {
        if id == "leaf/r0/k0" {
            for r in [&mut o.reference, &mut o.independent, &mut o.backend] {
                if let ResultV1::Ran { a, .. } = r {
                    a[0] ^= 1;
                }
            }
        }
    });
    assert_eq!(
        forged.evidence.status,
        misaka_palw_challenge::ConformanceStatusV1::Passed,
        "consistent forgery: the fold cannot see it"
    );
    cw.send_post(forged, None).await;
    assert!(cw.attempt().evidence.is_some(), "the forged evidence rebuilt exactly and entered its window");
    let (fresh, _) = cw.fresh();
    assert_eq!(fresh.leaf_faults, vec![2], "the fresh verifier finds the forged leaf (check 2 = the first leaf)");
    let honest_leaf = cw.leaf_fault(&selection, 1);
    let attempt = cw.attempt();
    // (C4 F-C4R4-11: one judged refutation per bond per window — the mistaken refuter is another bond than the one that convicts.)
    cw.refute(JUNK_REFUTER, honest_leaf).await;
    assert_eq!(decided(&cw.attempt()), decided(&attempt), "the true leaf proves nothing");
    let rows = cw.net.api().unwrap().aux.clone();
    cw.refute(JUNK_REFUTER, cw.leaf_fault(&selection, 0)).await;
    assert_eq!(cw.net.api().unwrap().aux, rows, "a bond's second refutation of the same evidence is refused before any charge");
    cw.refute(OUTSIDER, cw.leaf_fault(&selection, 0)).await;
    assert_eq!(state(&cw), (S::RegisteredDormant, Some(F::ConformanceFailed), 1));
    assert!(matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::Refuted, _))));
    let rows = cw.net.api().unwrap().aux.clone();
    cw.refute(OUTSIDER, cw.leaf_fault(&selection, 0)).await;
    assert_eq!(cw.net.api().unwrap().aux, rows, "a closed attempt cannot be refuted again");

    // ---- attempt 2: forged vector tokens, refuted through a Final claim of the bound kernel class ----
    cw.commit(0x23).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let (_, _, selection) = cw.selection();
    let forged = cw.post(|id, o| {
        if id == "vec/r0/i0" {
            for r in [&mut o.reference, &mut o.independent, &mut o.backend] {
                if let ResultV1::Ran { tokens, .. } = r {
                    tokens[0] = tokens[0].wrapping_add(1) % 2;
                }
            }
        }
    });
    let truth = greedy(&cw.cand, &cw.net.ledger(), &cw.kernel_class, &selection.vectors[0].prompt, 2);
    let ResultV1::Ran { tokens: claimed, .. } = &forged.outcomes[0].reference else { unreachable!() };
    if *claimed == truth {
        panic!("the forgery must change the tokens");
    }
    cw.send_post(forged, None).await;
    let posted = cw.attempt().evidence.expect("entered its window");
    cw.jobs += 1;
    let job = KernelJobV1 {
        class_binding_id: cw.kernel_class,
        prompt: selection.vectors[0].prompt.clone(),
        max_new_tokens: selection.vectors[0].decode,
        decode: DecodeRuleV1::Greedy,
        nonce: [cw.jobs; 64],
    };
    let o = cw.net.route(OUTSIDER, &K::PostJob { job: job.clone() });
    cw.net.send(vec![(OUTSIDER, o)]).await;
    let cand = cw.cand.clone_fixture();
    let claim = cw.claims_of(&cand, cw.kernel_class, std::slice::from_ref(&job)).await[0];
    // a refutation naming the claim before it is Final proves nothing
    let attempt = cw.attempt();
    cw.refute(JUNK_REFUTER, ConformanceFaultV1::VectorTokens { check: 0, kernel_claim: Hash64::from_bytes(claim) }).await;
    assert_eq!(decided(&cw.attempt()), decided(&attempt), "an unfinalized claim states nothing");
    let ttpb = cw.net.ttpb();
    while !matches!(cw.net.claim_state(&claim), ClaimStateV1::Final { .. }) {
        assert!(cw.net.daa() < posted.window_end_daa, "the kernel claim must reach Final inside the evidence's window");
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    cw.refute(OUTSIDER, ConformanceFaultV1::VectorTokens { check: 0, kernel_claim: Hash64::from_bytes(claim) }).await;
    assert_eq!(state(&cw), (S::RegisteredDormant, Some(F::ConformanceFailed), 2), "REFUTED by a Final of the bound kernel class");
    assert!(matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::Refuted, _))));

    // ---- attempt 3: the beacon locks and the evidence is withheld: a default, never a pass ----
    cw.commit(0x24).await;
    cw.source_claims(2).await;
    let lock = cw.until_locked().await;
    cw.net.beat_to(lock + PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1 - 1).await;
    assert_eq!(state(&cw).0, S::ChallengePending, "not before the deadline");
    cw.net.beat_to(lock + PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1 + 1).await;
    assert_eq!(state(&cw), (S::RegisteredDormant, Some(F::ConformanceFailed), 3), "withheld: a counted default");
    assert!(matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::Withheld, _))));

    // ---- the limit ----
    let rows = cw.net.api().unwrap().aux.clone();
    let o = cw.net.conformance_committed(REGISTRANT, cw.commitment(0x25));
    cw.net.send(vec![(REGISTRANT, o)]).await;
    assert_eq!(cw.net.api().unwrap().aux, rows, "AttemptsExhausted: a fourth commitment is refused");
    assert!(
        matches!(conformance_gate_v1(&cw.attempt()), PalwOnboardingGateV1::Held { code: "CONFORMANCE_FAILED", why } if why.contains("exhausted"))
    );
    let binding = cw.net.api().unwrap().artifact_binding_v1(&cw.v2_class, &cw.kernel_root).unwrap();
    cw.net.beat_to(binding.final_daa + 1).await;
    assert!(
        matches!(cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }),
        "never Active"
    );
}

// ---- hostile evidence and the budget ---------------------------------------------------------------------------------------

/// **Hostile evidence never panics, never passes, and is never judged for free.** A refutation with a junk opening (absurd leaf count,
/// wrong coordinates, ragged bytes) and one naming a check that does not exist are dismissed — each charging the block's adjudication
/// budget; five in one block: the four-adjudication test block judges two (two runs are reserved for proofs, C4 F-C4R4-10) and the
/// rest are not charged. Evidence carrying an outcome the seed never selected, or two outcomes for one check, is a forgery: CONFORMANCE_FAILED, counted. The chain carries on.
#[tokio::test]
async fn g14_conformance_hostile_evidence_is_dismissed_or_failed_spends_budget_and_never_stops_the_chain() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let (_, _, selection) = cw.selection();
    cw.send_post(cw.post(|_, _| {}), None).await;
    let posted = cw.attempt();
    assert!(posted.evidence.is_some());
    let evidence_id = posted.evidence.unwrap().evidence_id;
    let mut junk = Vec::new();
    let ConformanceFaultV1::LeafDecode { opening, .. } = cw.leaf_fault(&selection, 0) else { unreachable!() };
    let mut absurd = opening.clone();
    absurd.leaf_count = u32::MAX;
    absurd.leaf_index = u32::MAX - 1;
    let mut ragged = opening.clone();
    ragged.operand.bytes.push(0);
    let mut moved = opening.clone();
    moved.operand.row_start = moved.operand.row_start.wrapping_add(1);
    for fault in [
        ConformanceFaultV1::LeafDecode { check: 2, opening: absurd },
        ConformanceFaultV1::LeafDecode { check: 2, opening: ragged },
        ConformanceFaultV1::LeafDecode { check: 2, opening: moved },
        ConformanceFaultV1::LeafDecode { check: u32::MAX, opening: opening.clone() },
        ConformanceFaultV1::VectorTokens { check: 0, kernel_claim: Hash64::from_bytes([0xAB; 64]) },
    ] {
        let card = [OUTSIDER, 0, 2, 4, 5][junk.len()];
        junk.push((card, cw.evidence_object(card, ConformanceEvidenceActionV1::Refute { evidence_id, fault: Box::new(fault) })));
    }
    let fee = cw.net.api().unwrap().header.policy.dismissed_proof_fee;
    // What left each bond as a slash or burn (rewards and releases move `collateral` too; `slashed` only rises by a slash or a burn).
    let slashed = |cw: &Cw, card: usize| cw.net.chain.tip_state().1.bond(&cw.net.bond(card)).expect("the bond").slashed;
    let before: Vec<u64> = junk.iter().map(|(card, _)| slashed(&cw, *card)).collect();
    let cards: Vec<usize> = junk.iter().map(|(card, _)| *card).collect();
    cw.net.send(junk).await;
    assert_eq!(decided(&cw.attempt()), decided(&posted), "every junk refutation is dismissed");
    let (blue, adjudications, _work): (u64, u32, u64) =
        borsh::from_slice(&cw.budget_row().expect("the budget row")).expect("a budget row decodes");
    // A refutation is a proof: it may spend the runs reserved for proofs (C4 F-C4R4-11) — and one that proves nothing pays for it.
    assert_eq!(adjudications, 4, "four junk refutations spent the four-adjudication block; the fifth found it spent");
    let judged = cw.attempt().refuters_judged;
    assert_eq!(judged.len(), 4, "four judged, each once for its bond");
    for (i, card) in cards.iter().enumerate() {
        let paid = slashed(&cw, *card) - before[i];
        let was_judged = judged.contains(&cw.net.bond(*card));
        assert_eq!(
            paid,
            if was_judged { fee } else { 0 },
            "card {card}: a dismissed refutation pays dismissed_proof_fee; an unjudged one nothing"
        );
    }
    assert!(blue > 0);
    cw.net.beat_to(posted.evidence.unwrap().window_end_daa + 1).await;
    assert_eq!(state(&cw).0, S::G14Eligible, "the honest evidence passed through the junk");
    let z = cw.net.replay().await;
    cw.net.assert_same(&z, "replay");
}

/// **A forged outcome list is a failed attempt.** An extra outcome for a check the seed never selected, or two outcomes for one check:
/// the material does not rebuild the evidence the registrant signed — CONFORMANCE_FAILED, counted, at bounded cost.
#[tokio::test]
async fn g14_conformance_a_forged_outcome_list_fails_the_attempt() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let mut post = cw.post(|_, _| {});
    let mut extra = post.outcomes[0].clone();
    extra.check_id = "vec/r9/i9".into();
    post.outcomes.push(extra);
    cw.send_post(post, None).await;
    assert_eq!(state(&cw), (S::RegisteredDormant, Some(F::ConformanceFailed), 1));
    assert!(matches!(cw.attempt().last_end, Some((ConformanceAttemptEndV1::EvidenceFailed, _))));
}

// ---- the fence ---------------------------------------------------------------------------------------------------------------

/// **Fence off: the object is dropped by name and the block stands.** On testnet-12 as shipped (no kernel route fence) a tag-109 object
/// rides a carrier, the node's template carries it, the block is accepted, and nothing of the route exists.
#[tokio::test]
async fn g14_conformance_evidence_is_dropped_by_name_without_the_fence() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert!(config.params.palw_probabilistic_constraints_v1.is_none(), "dormant on the shipped testnet-12");
    let mut net = Net::over_cfg((config, bundle, premine, floats), TestConsensus::new);
    net.beat_to(1).await;
    let h = Hash64::from_bytes([1; 64]);
    let action = ConformanceEvidenceActionV1::Refute {
        evidence_id: h,
        fault: Box::new(ConformanceFaultV1::VectorTokens { check: 0, kernel_claim: h }),
    };
    let o = conformance_evidence_object_v1(net.domain, h, action, net.bond(OUTSIDER), &net.signer(OUTSIDER)).unwrap();
    let before = net.daa();
    net.send(vec![(OUTSIDER, o)]).await;
    assert!(net.daa() > before, "the chain carries on");
    assert!(net.api().is_none(), "dropped by name: no route state");
}

// ---- reorg, restart, pruned import -------------------------------------------------------------------------------------------

/// **The conformance rows ride the route's deltas, tail and root.** With an attempt open and evidence posted: a second node replays
/// to the same roots; a heavier branch from before the evidence reorgs it away exactly (attempt row and material as at the fork) and
/// back; a real restart over the same database reads the same rows and passes the window after it; a pruned importer that installs
/// the carriage at a block inside the window folds to every root and passes the same evidence. (Chunked delivery: the happy path.)
#[tokio::test]
async fn g14_conformance_rows_survive_reorg_restart_and_pruned_import() {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut cw = Cw::over(|c| TestConsensus::with_db(db.clone(), c, sender)).await;
    cw.net._keep.push(Box::new(receiver));
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let fork = cw.net.chain.sink();
    let at_fork = cw.net.api().expect("the route");
    // One carrier and its folding block past the fork (as the route's own reorg test): a shallow fork a heavier branch can win.
    cw.send_post(cw.post(|_, _| {}), None).await;
    let posted = cw.attempt().evidence.expect("posted");
    let p = cw.net.chain.sink();

    // ---- replay, and a heavier branch that never saw the evidence ----
    let z = cw.net.replay().await;
    cw.net.assert_same(&z, "Z on A");
    let zn = cw.net.on_chain(z);
    let b = t12_genesis_chain(&cw.net.config, &cw.net.bundle, &cw.net.premine, &cw.net.floats);
    let up_to_fork = chain_blocks(&cw.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for blk in up_to_fork {
        arrive(&b, blk, "a block up to the fork").await;
    }
    let mut b = b;
    b.ctx.simulated_time = fork_timestamp;
    let ttpb = cw.net.ttpb();
    let a_len = chain_blocks(&cw.net.chain, cw.net.chain.sink()).len() - chain_blocks(&cw.net.chain, fork).len();
    assert_eq!(a_len, 2, "the carrying block and the folding block");
    let mut b_blocks = Vec::new();
    for _ in 0..4 {
        b_blocks.push(b.heartbeat(ttpb, Vec::new()).await);
    }
    for blk in &b_blocks {
        arrive(&zn.chain, blk.clone(), "B's block").await;
    }
    assert_eq!(zn.chain.sink(), b.sink(), "Z reorgs onto B");
    let on_b = zn.api().expect("the route");
    let key = borsh::to_vec(&cw.v2_class).unwrap();
    for table in [39u8, 40] {
        assert_eq!(on_b.aux.get(&(table, key.clone())), at_fork.aux.get(&(table, key.clone())), "table {table} as at the fork");
    }
    assert!(on_b.conformance_attempt_v1(&cw.v2_class).unwrap().evidence.is_none(), "the reorged-out evidence left no row");
    let old_len = chain_blocks(&cw.net.chain, cw.net.chain.sink()).len();
    for _ in 0..4 {
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    for blk in chain_blocks(&cw.net.chain, cw.net.chain.sink()).into_iter().skip(old_len) {
        arrive(&zn.chain, blk, "A's later block").await;
    }
    cw.net.assert_same(&zn.chain, "Z back on A");
    assert_eq!(zn.api().unwrap().conformance_attempt_v1(&cw.v2_class).unwrap().evidence, Some(posted), "the evidence is back");

    // ---- a pruned importer at P (inside the window) ----
    let all = chain_blocks(&cw.net.chain, cw.net.chain.sink());
    let k = all.iter().position(|blk| blk.header.hash == p).expect("P is on the chain");
    let t = all[k + 1].clone();
    let importer = t12_genesis_chain(&cw.net.config, &cw.net.bundle, &cw.net.premine, &cw.net.floats);
    for blk in &all[..=k] {
        arrive(&importer, blk.clone(), "a block through the pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;
    let vp = cw.net.chain.vp();
    vp.capture_pruning_point_palw_state(p);
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");
    {
        let ivp = importer.vp();
        let mut store = ivp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(importer.config.params.genesis.hash).chain(all[..=k].iter().map(|blk| blk.header.hash)) {
            store.delete_delta_for_tests(blk).expect("no delta row at or below the pruning point");
        }
    }
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the carriage installs");
    assert_eq!(importer.tip_state().1.state_root(), root_at(&cw.net.chain, p), "the imported state is P's");
    assert!(
        importer.tip_state().1.kernel_route().unwrap().conformance_evidence_post_v1(&cw.v2_class).is_some(),
        "with the material (tail 0xEC)"
    );

    // ---- a real restart, then the window closes on the restarted node and on the importer alike ----
    let (sink, root) = (cw.net.chain.sink(), cw.net.chain.tip_state().1.state_root());
    let Cw { net, src, src_class, f, cand, v2_class, kernel_class, kernel_root, ops, jobs, turn } = cw;
    let net = net.restart(db.clone());
    let mut cw = Cw { net, src, src_class, f, cand, v2_class, kernel_class, kernel_root, ops, jobs, turn };
    assert_eq!((cw.net.chain.sink(), cw.net.chain.tip_state().1.state_root()), (sink, root), "the same tip off disk");
    assert_eq!(cw.attempt().evidence, Some(posted), "the attempt row off disk");
    cw.net.beat_to(posted.window_end_daa + 1).await;
    assert_eq!(cw.attempt().record.state, S::G14Eligible, "the restarted node passes the window");
    let all = chain_blocks(&cw.net.chain, cw.net.chain.sink());
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "A's block after P").await;
        assert_eq!(importer.tip_state().1.state_root(), root_at(&cw.net.chain, blk.header.hash), "the importer folds to A's root");
    }
    assert_eq!(importer.ctx.consensus.palw_kernel_route_v1(), cw.net.api(), "the same route, rows and aux");
    let z = cw.net.replay().await;
    cw.net.assert_same(&z, "a node replaying the whole chain");
}

// ---- G14-for-rewards (`docs/PRINCIPLES.md` §6; `palw_opv_bootstrap_v1::palw_reward_gate_v1`) ----------------------------------

impl Cw {
    /// The producer facts a REAL attempt's pre-check reads for `class`, for card `card`'s bond.
    fn facts(&self, class: Hash64, card: usize) -> kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2 {
        self.net.chain.ctx.consensus.palw_producer_facts_v2(class, Some(self.net.bond(card).0)).expect("producer facts")
    }

    /// The conformance path to its end: commit, two future OPV Finals of the source class lock the beacon, the evidence (chunked),
    /// the window closes unrefuted, the artifact binding reaches Final, and one more block runs the activation step.
    async fn pass_conformance(&mut self) {
        self.commit(0x22).await;
        self.source_claims(2).await;
        self.until_locked().await;
        let post = self.post(|_, _| {});
        self.send_post(post, Some(1024)).await;
        let posted = self.attempt().evidence.expect("the fold accepted the evidence");
        self.net.beat_to(posted.window_end_daa + 1).await;
        assert!(
            matches!(self.attempt().record.state, S::G14Eligible | S::ActiveRewardable),
            "CONFORMANCE_PASSED: {:?}",
            self.attempt().record.state
        );
        let binding = self.net.api().unwrap().artifact_binding_v1(&self.v2_class, &self.kernel_root).unwrap();
        self.net.beat_to(binding.final_daa.max(self.net.daa()) + 2).await;
    }

    /// **The onboarded (Cw) class's V2-root claims stay on the legacy channel** (the Lead's GAP-81 decision, 2026-10-10: the new
    /// rewards are per CLAIM verification route). Runs [`Self::pass_conformance`] on a drill network ([`Cw::over`]: floor 0, a
    /// sampled conformance allowed to gate — GAP-70's drill), then asserts that passing the onboarding/G14 path gives the class's
    /// V2-root claims NOTHING beyond the old rules:
    /// - the gate does not refuse it (it is onboarded);
    /// - no share beyond the one it registered with (no grant from the gate);
    /// - `producer`'s REAL-attempt pre-check is decided by the old rules: no Panel seat proves readiness here, so it is refused —
    ///   no G14 bypass of the registry lifecycle, the Panel room, the verify deadline, seating or the bond-share split.
    /// Returns the class and the old rules' refusal.
    pub(super) async fn onboarded_v2_claims_on_the_legacy_channel(&mut self, producer: usize) -> (Hash64, String) {
        let asked = match &self.net.chain.tip_state().1.class(&self.v2_class).unwrap().status {
            PalwClassStatusV2::Registered { pending_share_permille, .. } => *pending_share_permille,
            other => panic!("registered before its conformance: {other:?}"),
        };
        self.pass_conformance().await;
        let state = self.net.chain.tip_state().1;
        assert!(
            state.class_share_permille(&self.v2_class).unwrap_or(0) <= asked,
            "no share beyond the one it registered with ({asked}‰): {:?}",
            state.class_share_permille(&self.v2_class)
        );
        let facts = self.facts(self.v2_class, producer);
        let refusal = facts.class_admission_refusal.clone().unwrap_or_default();
        assert!(!refusal.contains("earns no reward"), "an onboarded class is not refused by the gate: {refusal}");
        let key = TestConsensus::palw_v2_registry_keypair(producer as u64).verification_key.as_ref().to_vec();
        assert!(
            facts.ready_to_produce(&key).is_err(),
            "a V2-root REAL attempt of the onboarded class is decided by the old rules, which no seat satisfies here"
        );
        (self.v2_class, refusal)
    }
}

/// **GAP-81: a V2 claim of a kernel-bound class never earns the new reward.** Before its conformance the class is refused by the
/// gate (`earns no reward`). After it (the drill terms) the class is onboarded, its kernel sibling is derived-eligible for OPV, and
/// yet its V2-root REAL attempt gets nothing the old rules do not give: no share grant and no bypass — the pre-check is refused by
/// the old rules. Its new reward is only its kernel-route claims' (the kernel class's OPV claims, gated per claim). A replay agrees.
#[tokio::test]
async fn g14_rewards_a_v2_claim_of_a_kernel_bound_class_never_earns_the_new_reward() {
    use kaspa_consensus_core::palw_opv_bootstrap_v1::{OpvClassFactsV1, OpvEligibilityViewV1, PalwRewardChannelV1};
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;
    let refusal = cw.facts(cw.v2_class, PRODUCERS[0]).class_admission_refusal.unwrap_or_default();
    assert!(refusal.contains("earns no reward"), "before its conformance the class earns nothing: {refusal}");
    let (_, old_rules) = cw.onboarded_v2_claims_on_the_legacy_channel(PRODUCERS[0]).await;
    assert!(!old_rules.contains("earns no reward"), "what refuses it is the old rules, never the gate: {old_rules}");
    // The class's new reward channel is its kernel class's OPV claims: derived-eligible on the drill terms (the binding's own path).
    let route = cw.net.api().expect("the route");
    let ledger = route.ledger().unwrap();
    let binding = route.kernel_binding_v1(&cw.v2_class).expect("kernel-bound");
    let facts = OpvClassFactsV1::of_registered(&ledger, &binding.kernel_class.as_bytes()).expect("the kernel class stands");
    let policy = route.header.opv.expect("the OPV policy");
    let view =
        OpvEligibilityViewV1 { policy: &policy, denied: &[], min_effective_bits: 0, sampled_gates_reward: true, test_eligible: &[] };
    assert!(
        route.v2_class_reward_eligibility_v1(&ledger, &cw.v2_class, cw.net.daa(), &view).is_ok(),
        "E1-E7 hold for its kernel class"
    );
    assert_eq!(
        facts.opv_id,
        binding.kernel_class.as_bytes(),
        "bound to its OPV kernel class: that class's claims are the new channel"
    );
    // The two channels name their budgets; a V2 root draws the legacy one whatever the class passed.
    assert_eq!(PalwRewardChannelV1::ALL, [PalwRewardChannelV1::LegacyPanelRoute, PalwRewardChannelV1::KernelRoute]);
    let z = cw.net.replay().await;
    cw.net.assert_same(&z, "replay");
}

/// **On the release terms a sampled conformance earns nothing** (the ruled floor of 128 effective bits, and GAP-70: only the complete
/// check gates rewards): the conformance passes (the record is G14_ELIGIBLE, the onboarding gate Ready) — a non-reward signal — but
/// E6 refuses (`POLICY_NOT_VERIFIED`, GAP-70) — the class stays Registered, holds no share, and every claim of it is refused.
#[tokio::test]
async fn g14_rewards_on_the_release_terms_a_sampled_conformance_earns_nothing() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::over_floor(|c| TestConsensus::new(c), 128).await;
    cw.pass_conformance().await;
    assert_eq!(cw.gate(), PalwOnboardingGateV1::Ready, "the onboarding gate alone would activate it");
    assert_eq!(cw.attempt().record.state, S::G14Eligible, "never ACTIVE_REWARDABLE");
    let state = cw.net.chain.tip_state().1;
    assert!(matches!(state.class(&cw.v2_class).unwrap().status, PalwClassStatusV2::Registered { .. }), "held Registered");
    assert_eq!(state.class_share_permille(&cw.v2_class), None, "no share");
    let refusal = cw.facts(cw.v2_class, PRODUCERS[0]).class_admission_refusal.unwrap_or_default();
    assert!(refusal.contains("POLICY_NOT_VERIFIED") && refusal.contains("GAP-70"), "E6, GAP-70: {refusal}");
}

/// **A class that never began onboarding never earns**: registered on a network with the gate armed, it passes its activation DAA
/// and stays Registered (no share), and its claims are refused `NOT_ONBOARDED` — whatever seats it gathers.
#[tokio::test]
async fn g14_rewards_a_class_that_never_began_onboarding_never_activates() {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::new().await;
    let f = onb_fixture(13);
    let o = cw.net.v2_registration(&f, OUTSIDER, cw.net.daa() + 3);
    let Obj::ClassRegisteredTirV1 { class_id, activation_daa, .. } = &o else { unreachable!() };
    let (class, activation) = (*class_id, *activation_daa);
    cw.net.send(vec![(OUTSIDER, o)]).await;
    cw.net.beat_to(activation + 3).await;
    let state = cw.net.chain.tip_state().1;
    assert!(matches!(state.class(&class).unwrap().status, PalwClassStatusV2::Registered { .. }), "past its activation, still held");
    assert_eq!(state.class_share_permille(&class), None);
    let refusal = cw.facts(class, OUTSIDER).class_admission_refusal.unwrap_or_default();
    assert!(refusal.contains("NOT_ONBOARDED"), "{refusal}");
}

/// **The two reward channels never mix** (the user's ruling of 2026-10-09, replacing a grandfathering flag). A class registered and
/// Active BEFORE the fence keeps earning through the OLD Panel route, whose verification stays in force in full: the reward gate does
/// not touch it (no G14 refusal — and no G14 bypass either). Its NEW, OPV rewards still require the gate: its program under OPV is
/// not eligible (`NOT_ONBOARDED`) and its OPV registration is refused. In the other direction, a class registered past the fence that
/// never onboarded earns nothing through the Panel route's rules: it stays Registered and its claims are refused `NOT_ONBOARDED`.
#[tokio::test]
async fn g14_rewards_a_legacy_panel_route_class_keeps_the_old_route_and_never_earns_opv_without_the_gate() {
    use kaspa_consensus_core::palw_opv_bootstrap_v1::{OpvClassFactsV1, OpvEligibilityViewV1, OpvIneligibleV1};
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = kernel_config_onboarding();
    let mut params = config.params.clone();
    params.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(60), Vec::new()));
    let mut net = Net::over_cfg((Config::new(params), bundle, premine, floats), TestConsensus::new);
    net.beat_to(1).await;
    let refusal = |net: &Net, class: Hash64, card: usize| {
        net.chain
            .ctx
            .consensus
            .palw_producer_facts_v2(class, Some(net.bond(card).0))
            .expect("facts")
            .class_admission_refusal
            .unwrap_or_default()
    };
    // ---- a legacy Panel-route class: registered and Active before the fence ----
    let legacy = onb_fixture(15);
    let o = net.v2_registration(&legacy, REGISTRANT, net.daa() + 3);
    let Obj::ClassRegisteredTirV1 { class_id, activation_daa, .. } = &o else { unreachable!() };
    let (legacy_class, activation) = (*class_id, *activation_daa);
    assert!(activation < 60);
    net.send(vec![(REGISTRANT, o)]).await;
    net.beat_to(activation + 1).await;
    assert!(matches!(net.chain.tip_state().1.class(&legacy_class).unwrap().status, PalwClassStatusV2::Active));
    net.beat_to(61).await;
    let r = refusal(&net, legacy_class, REGISTRANT);
    assert!(!r.contains("earns no reward"), "the legacy class keeps the OLD Panel route, in full (no G14 refusal): {r}");
    // ---- its NEW (OPV) rewards still need the gate ----
    let d = k2_tir_v2_descriptor();
    let register = K::RegisterClassV2 {
        mode: VerificationModeV1::OptimisticPublicVerification,
        descriptor: d.digest(),
        program_bytes: legacy.program.encode(),
        plan: legacy.plan.clone(),
        param_commitments: legacy.pc.clone(),
    };
    let o = net.route(REGISTRANT, &register);
    net.send(vec![(REGISTRANT, o)]).await;
    let facts = OpvClassFactsV1::of_registration(d.digest(), &legacy.program.encode(), &legacy.plan, &legacy.pc);
    assert!(!net.ledger().opv.classes.contains(&facts.opv_id), "its OPV registration is refused");
    let route = net.api().expect("the route");
    let ledger = route.ledger().unwrap();
    let policy = route.header.opv.expect("the OPV policy");
    let view =
        OpvEligibilityViewV1 { policy: &policy, denied: &[], min_effective_bits: 0, sampled_gates_reward: true, test_eligible: &[] };
    assert_eq!(route.opv_eligibility_v1(&ledger, &facts, net.daa(), &view), Err(OpvIneligibleV1::NotOnboarded), "OPV needs the gate");
    // ---- a class past the fence: the Panel route's rules alone open nothing ----
    let fresh = onb_fixture(16);
    let o = net.v2_registration(&fresh, OUTSIDER, net.daa() + 3);
    let Obj::ClassRegisteredTirV1 { class_id, activation_daa, .. } = &o else { unreachable!() };
    let (fresh_class, activation) = (*class_id, *activation_daa);
    net.send(vec![(OUTSIDER, o)]).await;
    net.beat_to(activation + 3).await;
    assert!(matches!(net.chain.tip_state().1.class(&fresh_class).unwrap().status, PalwClassStatusV2::Registered { .. }));
    let r = refusal(&net, fresh_class, OUTSIDER);
    assert!(r.contains("NOT_ONBOARDED"), "a post-fence class earns only through the G14 path: {r}");
}

// C4 round 4 (independent adversarial review): the onboarding court's share of the block's adjudication budget.
mod c4r4;
