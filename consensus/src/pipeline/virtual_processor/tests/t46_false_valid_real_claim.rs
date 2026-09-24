//! **ADR-0152 v2 stage F2's acceptance suite, T46 (spec §3.6): a false `Valid` on a REAL claim
//! is admitted by the object gate and convicts in the fold.**
//!
//! The user's bar for F2 is "proven only by a fold test on a real block-lane claim", so nothing a
//! conviction is measured against is hand-assembled:
//!
//! * **The claim is the producer's.** A template is built by this node
//!   (`build_block_template_keeping_time`), its anchor is `execution_anchor_v3` of the template's own
//!   pre-PoW hash, card 0 and the nonce, the job is `Base0Backend::job_for_anchor` of that anchor at
//!   the network's prompt form and `palw_attempt_job_v1` at the block's draw rule, and the roots are
//!   an execution of that job — the drill's own self-consistent lie
//!   (`execute_with_injected_fault`), a forged decode token over `base0_execute_for_attempt_v1`'s
//!   run re-committed exactly as `verify_binding` recomputes it, or a non-canonical leaf count
//!   re-committed the same way. The attempt carries `palw_producer_facts_v2`'s facts, is signed by
//!   card 0 and is folded as the block's OWN work (`palw_v2_fold_attempt_for_tests`: the execution
//!   key the pipeline derives from the carrying header). Every claim asserts, before anything else,
//!   that the chain keyed it under `attempt_id_v2(attempt)`, that its binding's job id is the anchor,
//!   and that the anchor is not the claim id — the V1 rule's `job_id == claim_id` fixed point.
//! * **The panel and the licence.** `PanelBound` with cards 1–5 is folded directly (its derivation
//!   from the anchor is the processor's and is not what is measured here); the licence is a
//!   `ReceiptLicensedV2` of V3 receipts with the masks `palw_segment_assignment_v2` assigns, signed by
//!   the cards' harness keys under the chain's domain, admitted by `palw_v2_validate_objects`, carried
//!   by the acceptance walk and folded.
//! * **Every offence takes the whole path** a block's object takes: the gate
//!   (`palw_v2_validate_objects`, with the signature half), the acceptance walk
//!   (`palw_v2_accepted_objects_for_tests`, whose per-object rehearsal is the fold) and the fold
//!   (`palw_v2_fold_accepted_with_delta_for_tests`, the pipeline's transition at this processor's
//!   fences and extras). A refusal is asserted by its exact `PalwOffenceVerifyError`, and the walk is
//!   asserted to drop it.
//!
//! The chain is folded in memory from the genesis state the processor stored at construction; no
//! block is mined (PoW, the class lottery and the carrier fees are the header and mempool rules, not
//! F2's). Everything else is testnet-12's shipped ruleset with harness keys on its eight cards
//! (`t12_round_lane_e2e::t12_with_harness_cards`), and T46a runs the same ruleset with
//! `palw_offence_attribution` unset.
//!
//! **What this suite found.** testnet-12 commits its prompts in the Merkle form, and the adjudicator
//! as first written judged a `StepArithmetic` with the V1 route's flat prompt comparison, which
//! refuses every refutation a prover builds there: no step fault on a real claim could convict.
//! The adjudicator now reads a step refutation in the network's prompt carriage
//! (`palw_false_valid_convicts_execution_v2`); `t46o` pins it on the embedding gather of prompt
//! position 0, the step that cannot be adjudicated without the prompt at all. And T46g answers the
//! spec's open question about the free-prompt lane: testnet-12's genesis certifies the floor's
//! free-prompt lane in its params but does not publish the class's graph that ADR-0145's
//! derived-work fence (armed at genesis) prices commitments with, so a floor commitment is skipped
//! until a `FamilyCertified` and a `ClassLaneCertified` publish it — which T46g carries.
//!
//! **The F2 review.** (F-1) A partial seat vouches for its segment's leaves as the function of what
//! it resumed from, never for their agreement with other segments' committed leaves, so it is liable
//! only for a step every leaf of which — the output and each input read — lies in its segment: T46b's
//! lie is placed at such a leaf (and pins the site), T46p runs the reviewer's probe (one lie in
//! segment 1, its downstream readers in segments 2 and 3 convicting no partial seat) and T46b's old
//! boundary leaf. (F-2) `ProducerWithholding` is refused: T46r. (F-3) A step refutation's prompt rides
//! as the evidence's `prompt_ids_opening` (T46o), and every offence this suite files is weighed in a
//! signed 0x4b carrier against one standard transaction (`H::fits_one_carrier`). (F-4) A partial mask
//! must be the assigned one while a full mask is a full attestation (T46q), and every session on the
//! claim — a non-held data-availability one included — defers a conviction (T46n).
use super::TestContext;
use crate::consensus::test_consensus::TestConsensus;
use crate::pipeline::virtual_processor::VirtualStateProcessor;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    attempt_trace_manifest_root_v1, challenge_v2, execution_anchor_v3, palw_attempt_job_v1,
};
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_FALSE_VALID_NETWORK_LADDER_V1, PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES, PALW_PANEL_FALSE_VALID_VERSION_V2,
    PalwFalseValidReceiptV1, PalwFaultSiteV1, PalwPanelFalseValidEvidenceV2, palw_check_panel_false_valid_v2,
    palw_false_valid_admission_v1, palw_false_valid_convicts_execution_v2, palw_false_valid_fault_site_v1,
    palw_false_valid_offence_id_v2,
};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwOffenceVerifyError as E, PalwPanelContradictionV1 as C,
    PalwPanelFalseValidEvidenceV1, palw_offence_evidence_digest_v1, palw_panel_contradiction_convicts_execution_v1,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3,
    palw_receipt_message_v2, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwConsensusObjectV2 as Obj,
    PalwPanelSeatV2, PalwStateCarriageV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwVoidReasonV2, revert_delta_v2,
};
use kaspa_consensus_core::palw_step_leg::{
    PalwStepBindingV2, PalwStepEvidenceV1, PalwStepRefutationV1, checkpoint_leg_root_v2, execution_commitment_root_v2,
    step_leg_root_v1, verify_binding_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_verification_v2::{
    PalwSegmentAssignmentV2, PalwSegmentMaskV2, palw_segment_assignment_v2, palw_segment_count_v2, palw_segment_index_of_leaf_v2,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;
use misaka_palw_base0::backend::Base0Backend;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Card 0 produces every claim here; cards 1–5 are its panel; card 6 is a bonded bystander.
const EXECUTOR: usize = 0;
const PANEL: [usize; 5] = [1, 2, 3, 4, 5];
const BYSTANDER: usize = 6;

fn card_pubkey(card: usize) -> Vec<u8> {
    TestConsensus::palw_v2_registry_keypair(card as u64).verification_key.as_ref().to_vec()
}

fn sign(card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&TestConsensus::palw_v2_registry_keypair(card as u64).signing_key, message, context, [0x46u8; 32])
        .expect("ML-DSA-87 signs")
        .as_ref()
        .to_vec()
}

fn offence(kind: PalwOffenceKindV1, accused: PalwBondKeyV2, evidence: Vec<u8>) -> Obj {
    Obj::ObjectiveOffence { kind, accused, evidence_id: palw_offence_evidence_digest_v1(&evidence), evidence }
}

/// **The probe's `rebind`** (`adr0152v2_charging_model_probe.rs:51-61`): re-commit a binding the way
/// `verify_binding` recomputes its root — the step leg and the checkpoint leg over the context, then
/// the execution root over the four legs. Asserted equal to the producer's own commitment on an
/// untouched run before it is used on a touched one.
fn rebind(b: &mut PalwStepBindingV2) {
    let ctx_hash = b.job_context.context_hash();
    let profile_hash = b.shape_profile.shape_profile_id();
    let decode_calls = b.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, b.step_leaf_count, &b.step_merkle_root);
    let ckpt_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &b.checkpoint_profile.profile_hash(),
        &b.state_chunk_map_id,
        decode_calls,
        b.checkpoint_count,
        &b.checkpoint_merkle_root,
    );
    b.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &b.full_logits_trace_root, &b.activation_leg_root, &ckpt_root, &step_root);
}

/// The chain folded so far, in memory: the state and the last point it was folded at.
#[derive(Clone)]
struct Walk {
    state: PalwChainStateV2,
    daa: u64,
    blue: u64,
}

impl Walk {
    /// The next block's point at `daa` — one blue score along, as the fold demands.
    fn at(&self, daa: u64) -> PalwBlockContextV2 {
        assert!(daa >= self.daa, "the DAA never goes back");
        PalwBlockContextV2 {
            block: Hash64::from_u64_word(0x7446_0000_0000 | (self.blue + 1)),
            daa_score: daa,
            blue_score: self.blue + 1,
            subsidy: 0,
        }
    }

    fn next(&self) -> PalwBlockContextV2 {
        self.at(self.daa + 1)
    }

    fn advance(&mut self, point: &PalwBlockContextV2, state: PalwChainStateV2) -> PalwChainStateV2 {
        self.daa = point.daa_score;
        self.blue = point.blue_score;
        std::mem::replace(&mut self.state, state)
    }
}

/// Which execution the producer commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    /// The job, run and committed honestly.
    Honest,
    /// (A) `execute_with_injected_fault`: one lane of one step tile moved by one, the capture
    /// re-committed from the corrupted tiles — a self-consistent lie only re-execution finds. At the
    /// first leaf the capture holds from the middle of the step space on WHOSE STEP READS ONLY ITS
    /// OWN SEGMENT: the lie a partial seat's replay decides by itself, so the partial holder is
    /// liable too (F2 review, F-1). The middle itself, `n / 2`, is the first leaf of segment 2 under
    /// the four-way cut and reads the last four leaves of segment 1 — [`Fault::StepAt`] files it.
    Step,
    /// (A) at a named leaf.
    StepAt(u64),
    /// (A) at the first K-cache write of a segment of the four-way cut that the drill's move keeps an
    /// int8 — the reviewer's probe: a cache row every later position's attention reads, in other
    /// segments. The drill moves the low byte of lane 0 by one (`corrupt_capture_v1`), so a lane at
    /// 127 or −1 leaves the int8 range and no reader of the row can even be adjudicated (`base0 int8
    /// lane out of range`); such a row is passed over for the next.
    KCacheWrite { segment: u16 },
    /// (A) at the first leaf of the step space: the embedding gather of prompt position 0, a step
    /// the court recomputes from the prompt id it read.
    StepGather,
    /// (B) `base0_execute_for_attempt_v1`'s run with `toks[0]` flipped, the logits trace root
    /// recomputed over the flipped token, and the binding re-committed.
    Forged,
    /// A non-canonical step-leaf count (`+ 1`), re-committed — a shape the structural pass answers
    /// from the binding alone.
    Shape,
    /// **F1 T18e (the relabel):** the producer runs ANOTHER anchor's prompt with this block's anchor
    /// written into the context as `job_id` and `execution_seed` — the attack J1 and J3 cannot see
    /// and J5 (the whole canonical context, its prompt root) does.
    Relabel,
    /// **F1 T18d (the output grind):** the honest run, committed under an `output_root` that is not
    /// its generated output's.
    OutputGrind,
    /// **F1 T18f (J4):** the honest binding, committed beside a trace root that is not its logits
    /// trace root.
    TraceSwap,
    /// **F1 T18c(iv), the R1 residual:** an honest step tree over garbage logits — one lane of the
    /// selecting row bent without moving its argmax, the trace root re-committed. Nothing in F1
    /// proves it (F1c's `LogitsNotStepOutput` does).
    GarbageLogits,
}

/// The roots an attempt carries.
#[derive(Clone, Copy)]
struct AttemptRoots {
    trace: Hash64,
    output: Hash64,
    execution: Hash64,
    manifest: Hash64,
    chunks: u32,
}

impl AttemptRoots {
    fn as_attempt_roots(&self) -> AttemptRoots {
        *self
    }
}

/// A claim the chain opened from the producer's own work.
struct RealClaim {
    claim_id: Hash64,
    anchor: Hash64,
    job: PalwJobContextV2,
    /// The canonical prompt the anchor implies.
    prompt: Vec<usize>,
    envelope: PalwAttemptEnvelopeV2,
    /// The binding the claim's `execution_root` commits to.
    binding: PalwStepBindingV2,
    /// The capture whose roots the claim carries (`Honest`, `Step`).
    material: Vec<u8>,
    /// The objective proof of the fault (`None` for `Honest`).
    contradiction: Option<C>,
    /// The step leaf the fault is at (`Step`).
    fault_leaf: Option<u64>,
    /// The generated ids and logits rows the claim's binding commits (the honest producer's own,
    /// or the faulted run's) — the pin an `OutputMismatch` carries.
    pin: Option<kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1>,
    /// The header that carried the attempt (its pre-PoW hash names the anchor).
    header: Header,
}

impl RealClaim {
    fn contradiction(&self) -> C {
        self.contradiction.clone().expect("a faulted claim carries its proof")
    }
}

/// What the licence carried: the assignment the anchor drew and each seat's V3 receipt.
struct Licence {
    assignment: PalwSegmentAssignmentV2,
    /// `(card, receipt)` in panel order.
    receipts: Vec<(usize, PalwSeatReceiptV3)>,
    licensed_daa: u64,
}

impl Licence {
    fn full_card(&self) -> usize {
        self.receipts[self.assignment.full_seat as usize].0
    }

    fn partials(&self) -> Vec<usize> {
        self.receipts
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.assignment.full_seat as usize)
            .map(|(_, (card, _))| *card)
            .collect()
    }

    /// The one partial seat whose mask holds `segment`.
    fn holder_of(&self, segment: u16) -> usize {
        let holders: Vec<usize> = self
            .receipts
            .iter()
            .enumerate()
            .filter(|(i, (_, r))| *i != self.assignment.full_seat as usize && r.segments.covers(segment))
            .map(|(_, (card, _))| *card)
            .collect();
        assert_eq!(holders.len(), 1, "the partial seats partition the job: segment {segment} has one holder");
        holders[0]
    }

    fn receipt(&self, card: usize) -> &PalwSeatReceiptV3 {
        &self.receipts.iter().find(|(c, _)| *c == card).expect("a seat of the panel").1
    }

    fn segmented(&self, card: usize) -> PalwFalseValidReceiptV1 {
        PalwFalseValidReceiptV1::Segmented(self.receipt(card).clone())
    }
}

struct H {
    ctx: TestContext,
    config: Config,
    bundle: PalwConsensusParamsV2,
    /// `palw_network_domain_v2_for(network id, genesis)` — what the processor derives.
    domain: Hash64,
    /// The genesis cards' bond keys, in registry order.
    cards: Vec<PalwBondKeyV2>,
    genesis: PalwChainStateV2,
    /// The floor, resolved as a node resolves it: from its registered root, at the ruleset's
    /// ladder and the network's prompt form.
    backend: Base0Backend,
    artifact_root: Hash64,
    /// Each card's fee float in the genesis UTXO set — what a filer's 0x4b carrier spends.
    floats: Vec<(TransactionOutpoint, UtxoEntry)>,
}

/// testnet-12 with harness cards at genesis; `armed = false` unsets `palw_offence_attribution`.
fn harness(armed: bool) -> H {
    harness_with(armed, armed)
}

/// **testnet-12 with `palw_offence_attribution` armed and `palw_rcore_plus` forced off** — the
/// fence-off twin ADR-0152 v3.1 R6 names for the tests that exercise the v1 data-availability court
/// (ADR-0062, ADR-0103 Decision 4: `DefaultDisputed`, `MaterialDisclosed(Held)`), which stands below
/// `palw_rcore_plus` and which M3's court replaces above it. Its bundle is the twin's own (mirrors
/// re-synced), so the fold the harness drives and the processor read one ruleset.
fn harness_rcore_off() -> H {
    harness_with(true, false)
}

fn harness_with(offence_attribution: bool, rcore_plus: bool) -> H {
    use misaka_palw_base0::classes::resolve_class_v1;
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, _premine, floats) = super::t12_round_lane_e2e::t12_with_harness_cards();
    assert!(config.params.palw_offence_attribution.is_some_and(|f| f.is_active(0)), "testnet-12 arms the fence from genesis");
    let (config, bundle): (Config, PalwConsensusParamsV2) = if offence_attribution && rcore_plus {
        (config, bundle)
    } else if !offence_attribution {
        let mut params = config.params.clone();
        params.palw_offence_attribution = None;
        // ADR-0152: R-core+ is armed above this fence on testnet-12 and refuses to stand without
        // it, so the fence-off twin takes R-core+ off too (every R-core+ writer is dormant, so the
        // twin folds exactly as it did before the v22 skeleton).
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        (ConfigBuilder::new(params).skip_proof_of_work().build(), bundle)
    } else {
        let mut params = config.params.clone();
        params.palw_rcore_plus = None;
        params.palw_rcore_conservative_classes = &[];
        params.sync_palw_rcore_plus();
        let config = ConfigBuilder::new(params).skip_proof_of_work().build();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(twin) = &config.params.palw_consensus_mode else {
            unreachable!("testnet-12 is ConsensusV2")
        };
        let twin = twin.clone();
        assert!(twin.state.rcore_plus_from_daa().is_none(), "the twin's bundle mirrors the fence off");
        (config, twin)
    };
    config.params.validate_palw_v2().expect("the fixture is a runnable ruleset");
    let ctx = TestContext::new(TestConsensus::new(&config));
    let (_, genesis) =
        ctx.consensus.virtual_processor().palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the genesis tip loads");
    let cards: Vec<PalwBondKeyV2> = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .collect();
    assert_eq!(cards.len(), 8, "testnet-12 registers eight cards");
    for (i, card) in cards.iter().enumerate() {
        assert_eq!(genesis.bond(card).expect("a genesis card").pubkey, card_pubkey(i), "card {i} carries its harness key");
    }
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let artifact_root = genesis.class(&bundle.base_class_id).expect("the floor is registered").artifact_root;
    assert_eq!(
        artifact_root,
        misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().expect("the floor's pinned root"),
        "testnet-12's floor is the RC floor's artifact"
    );
    let backend = Base0Backend::new(
        resolve_class_v1(&bundle.court, bundle.base_class_id, artifact_root, &[])
            .expect("the floor resolves from its registered root"),
    )
    .with_step_ladder_cap(bundle.court.max_step_leaf_count())
    .with_prompt_ids_form(config.params.palw_prompt_ids_form_v1());
    H { ctx, config, bundle, domain, cards, genesis, backend, artifact_root, floats }
}

impl H {
    fn vp(&self) -> &Arc<VirtualStateProcessor> {
        self.ctx.consensus.virtual_processor()
    }

    fn sp(&self) -> &PalwStateParamsV2 {
        &self.bundle.state
    }

    fn floor(&self) -> Hash64 {
        self.bundle.base_class_id
    }

    /// The network's prompt-id form — Merkle on testnet-12 from genesis.
    fn form(&self) -> kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1 {
        self.config.params.palw_prompt_ids_form_v1()
    }

    fn card_of(&self, bond: &PalwBondKeyV2) -> usize {
        self.cards.iter().position(|c| c == bond).expect("a genesis card")
    }

    fn genesis_walk(&self) -> Walk {
        let last = *self.genesis.last_point().expect("the genesis fold records its point");
        Walk { state: self.genesis.clone(), daa: last.daa_score, blue: last.blue_score }
    }

    /// **The identity rules this network's gate and fold read** (ADR-0152 v3.1 J-5): its prompt-id
    /// form and its base class.
    fn rules(&self) -> kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
        kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 {
            prompt_ids_form: self.form(),
            base_class_id: self.floor(),
            da_signer_liability: kaspa_consensus_core::palw_state_v2::palw_da_signer_liability_armed_v1(
                self.sp(),
                kaspa_consensus_core::palw_da_rcore_v1::PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1,
            ),
        }
    }

    /// The adjudicator's ladder for the floor — the fold's and the processor's.
    fn ladder(&self, state: &PalwChainStateV2) -> u64 {
        state.class_step_ladder_v1(&self.floor(), PALW_FALSE_VALID_NETWORK_LADDER_V1)
    }

    // ---- the processor's three doors -------------------------------------------------------

    fn validate(&self, state: &PalwChainStateV2, point: &PalwBlockContextV2, object: &Obj) -> Result<(), String> {
        self.vp().palw_v2_validate_objects(state, self.sp(), point, std::slice::from_ref(object))
    }

    fn accepted(&self, state: &PalwChainStateV2, point: &PalwBlockContextV2, objects: &[Obj]) -> Vec<Obj> {
        self.vp().palw_v2_accepted_objects_for_tests(state, self.sp(), point, objects.to_vec(), point.block)
    }

    fn fold(
        &self,
        state: &PalwChainStateV2,
        point: &PalwBlockContextV2,
        objects: &[Obj],
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        self.vp().palw_v2_fold_accepted_for_tests(state, self.sp(), point, objects)
    }

    /// **The whole path an object takes into a block**: every object clears the gate, the walk
    /// carries exactly them, and the fold applies what the walk carried. Returns the parent state
    /// and the delta the fold wrote.
    fn carry(&self, walk: &mut Walk, objects: Vec<Obj>) -> (PalwChainStateV2, PalwStateDeltaV2) {
        let point = walk.next();
        for object in &objects {
            self.fits_one_carrier(object);
            if let Err(why) = self.validate(&walk.state, &point, object) {
                panic!("the gate refuses an object this test needs admitted: {why}");
            }
        }
        assert_eq!(
            self.accepted(&walk.state, &point, &objects),
            objects,
            "the acceptance walk carries every object the gate admitted"
        );
        let (next, delta) = self
            .vp()
            .palw_v2_fold_accepted_with_delta_for_tests(&walk.state, self.sp(), &point, &objects)
            .unwrap_or_else(|e| panic!("the fold applies what the walk carried: {e}"));
        (walk.advance(&point, next), delta)
    }

    /// **A refusal at the gate, by its exact reason** — and the acceptance walk drops the object,
    /// so the block that carried it folds as if it had not.
    fn refused(&self, walk: &Walk, object: &Obj, want: &str) {
        self.fits_one_carrier(object);
        let point = walk.next();
        assert_eq!(self.validate(&walk.state, &point, object), Err(want.to_string()), "the gate's reason");
        assert!(self.accepted(&walk.state, &point, std::slice::from_ref(object)).is_empty(), "the walk drops what the gate refused");
    }

    /// The fold's own refusal of `object` at the next point, with its reason.
    fn fold_refusal(&self, walk: &Walk, object: &Obj) -> String {
        self.fits_one_carrier(object);
        match self.fold(&walk.state, &walk.next(), std::slice::from_ref(object)) {
            Err(PalwStateV2Error::ObjectiveOffenceRefused(_, why)) => why,
            other => panic!("the fold refuses the object: {other:?}"),
        }
    }

    /// Empty blocks, one DAA at a time, until `claim` is `Final`. Returns the state the `Final`
    /// block folded from.
    fn sweep_to_final(&self, walk: &mut Walk, claim: Hash64) -> PalwChainStateV2 {
        // One empty block most of the way through the challenge window (nothing happens to a
        // licensed claim inside it), then one DAA at a time to the block that finalises it.
        let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = walk.state.claim(&claim).expect("a live claim").phase else {
            panic!("sweep_to_final starts from a licensed claim");
        };
        let jump = licensed_daa + self.sp().window_challenge_at(licensed_daa).saturating_sub(2);
        if jump > walk.daa + 1 {
            let point = walk.at(jump);
            let next = self.fold(&walk.state, &point, &[]).expect("an empty block folds");
            let phase = next.claim(&claim).map(|c| c.phase.clone());
            assert!(matches!(phase, Some(PalwClaimPhaseV2::ReceiptLicensed { .. })), "still challengeable: {phase:?}");
            walk.advance(&point, next);
        }
        for _ in 0..20_000 {
            let point = walk.next();
            let next = self.fold(&walk.state, &point, &[]).expect("an empty block folds");
            let is_final = matches!(next.claim(&claim).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. }));
            let before = walk.advance(&point, next);
            if is_final {
                return before;
            }
        }
        panic!("claim {claim} reaches Final inside the challenge window");
    }

    // ---- receipts and offences --------------------------------------------------------------

    /// Card `card`'s `Valid` on `claim`, in the full (V2) form, signed under `domain`.
    fn full_receipt(&self, card: usize, claim: Hash64, domain: Hash64, signed_daa: u64) -> PalwSeatReceiptV2 {
        let message = palw_receipt_message_v2(domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
        PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: self.cards[card],
            signed_daa,
            signature: sign(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
        }
    }

    /// The same `Valid` in the segmented (V3) form over `mask`, signed under `domain` — what a
    /// kaspad seat signs past Verification V2.
    fn v3_receipt(
        &self,
        card: usize,
        claim: Hash64,
        domain: Hash64,
        signed_daa: u64,
        mask: kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2,
    ) -> PalwSeatReceiptV3 {
        let message = palw_receipt_message_v3(domain, claim, PalwReceiptVerdictV2::Valid, signed_daa, mask);
        let mut receipt = self.full_receipt(card, claim, domain, signed_daa);
        receipt.signature = sign(card, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT);
        PalwSeatReceiptV3 { receipt, segments: mask }
    }

    /// **A contradiction as a filer on this network sends it** (F2 review, F-3): a step
    /// refutation's prompt in the job's carriage — the id list the prover carries taken out, and the
    /// one tile the step reads opened against the Merkle root, as
    /// `palw_refutation_prompt_carriage_v1` builds the one-move court's pair — and every other
    /// contradiction as it is.
    fn carried(&self, contradiction: C) -> (C, Option<PalwPromptIdsOpeningV1>) {
        match contradiction {
            C::StepArithmetic { refutation, operand_openings } => {
                let (refutation, opening) =
                    kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(self.form(), refutation)
                        .expect("the prover's list is the job's");
                (C::StepArithmetic { refutation, operand_openings }, opening)
            }
            other => (other, None),
        }
    }

    /// The adjudicator's execution check on `contradiction` in this network's carriage.
    fn convicts(&self, contradiction: &C, execution_root: Hash64, ladder: u64) -> Result<(), E> {
        let (carried, opening) = self.carried(contradiction.clone());
        palw_false_valid_convicts_execution_v2(&carried, opening.as_ref(), execution_root, self.artifact_root, ladder)
    }

    /// The evidence exactly as given — the carriage is the caller's (T46o's controls).
    fn v2_payload_raw(
        &self,
        card: usize,
        claim: Hash64,
        receipt: PalwFalseValidReceiptV1,
        contradiction: C,
        prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
    ) -> PalwPanelFalseValidEvidenceV2 {
        PalwPanelFalseValidEvidenceV2 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V2,
            claim_id: claim,
            accused_seat: self.cards[card].0,
            receipt,
            contradiction,
            prompt_ids_opening,
            reporter_reveal: Vec::new(),
        }
    }

    /// The evidence a filer sends: the contradiction in the network's carriage.
    fn v2_payload(
        &self,
        card: usize,
        claim: Hash64,
        receipt: PalwFalseValidReceiptV1,
        contradiction: C,
    ) -> PalwPanelFalseValidEvidenceV2 {
        let (contradiction, opening) = self.carried(contradiction);
        self.v2_payload_raw(card, claim, receipt, contradiction, opening)
    }

    fn v2(&self, card: usize, claim: Hash64, receipt: PalwFalseValidReceiptV1, contradiction: C) -> Obj {
        let payload = self.v2_payload(card, claim, receipt, contradiction);
        offence(PalwOffenceKindV1::PanelFalseValidV2, self.cards[card], borsh::to_vec(&payload).unwrap())
    }

    /// **Every offence this suite files fits ONE carrier** (F2 review, F-3): the object in the 0x4b
    /// lifecycle payload the extractor reads, on a transaction spending the bystander's fee float and
    /// signed with its key, weighed by the consensus's own mass calculator (the masses block
    /// validation and the mempool read) against the mempool's standard mass and the block's. A
    /// kind-3 object cannot be chunked, so one that failed here could never reach a block. Returns
    /// the transient mass.
    fn fits_one_carrier(&self, object: &Obj) -> u64 {
        use kaspa_consensus_core::palw_lifecycle_objects_v2::{
            PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, validate_palw_lifecycle_tx,
        };
        use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput};
        let Obj::ObjectiveOffence { evidence, .. } = object else { return 0 };
        assert!(evidence.len() as u64 <= PALW_OFFENCE_V2_MAX_EVIDENCE_BYTES, "the evidence is within the one-carrier cap");
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
            .expect("the carriage serializes");
        validate_palw_lifecycle_tx(&payload, false).expect("the lifecycle admission takes the payload");
        let (outpoint, entry) = self.floats[BYSTANDER].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - 300_000, super::t12_round_lane_e2e::card_payout_spk(BYSTANDER))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        super::t12_round_lane_e2e::sign_spend(&mut tx, entry, BYSTANDER, self.config.params.storage_mass_parameter);
        let masses = self.ctx.consensus.calculate_transaction_non_contextual_masses(&tx);
        let standard = kaspa_consensus_core::palw_mode_v2::PALW_MIRRORED_STANDARD_TX_MASS;
        for (what, mass) in [("transient", masses.transient_mass), ("compute", masses.compute_mass)] {
            assert!(mass <= standard, "the carrier's {what} mass {mass} is within a standard transaction's {standard}");
            assert!(mass <= self.config.params.max_block_mass, "and within a block's");
        }
        masses.transient_mass
    }

    /// **Kind 4's evidence as a filer sends it** (ADR-0152 v3.1 J-4): the claim, the contradiction in
    /// the network's carriage, no receipt and an empty reporter slot.
    fn refuted_payload(
        &self,
        claim: Hash64,
        contradiction: C,
    ) -> kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1 {
        let (contradiction, prompt_ids_opening) = self.carried(contradiction);
        kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1 {
            version: kaspa_consensus_core::palw_offence_attribution_v1::PALW_EXECUTOR_REFUTED_VERSION_V1,
            claim_id: claim,
            contradiction,
            prompt_ids_opening,
            reporter_reveal: Vec::new(),
        }
    }

    /// `ExecutorRefuted` against card 0 — every claim here is card 0's — on `claim`.
    fn refuted(&self, claim: Hash64, contradiction: C) -> Obj {
        offence(
            PalwOffenceKindV1::ExecutorRefuted,
            self.cards[EXECUTOR],
            borsh::to_vec(&self.refuted_payload(claim, contradiction)).unwrap(),
        )
    }

    /// The kind-4 adjudicator's verdict on `contradiction` against `claim` in `state` — what the gate
    /// and the fold both run.
    fn judge_refuted(
        &self,
        state: &PalwChainStateV2,
        claim: Hash64,
        contradiction: C,
    ) -> Result<kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedFindingV1, E> {
        let bytes = borsh::to_vec(&self.refuted_payload(claim, contradiction)).unwrap();
        kaspa_consensus_core::palw_offence_attribution_v1::palw_check_executor_refuted_v1(
            state,
            &self.cards[EXECUTOR],
            &bytes,
            false,
            false,
            self.rules(),
        )
    }

    fn v1(&self, card: usize, claim: Hash64, valid_receipt: PalwSeatReceiptV2, contradiction: C) -> Obj {
        let payload = PalwPanelFalseValidEvidenceV1 {
            version: PALW_PANEL_FALSE_VALID_VERSION_V1,
            claim_id: claim,
            network_domain: self.domain,
            accused_seat: self.cards[card].0,
            valid_receipt,
            executor_pubkey: card_pubkey(EXECUTOR),
            contradiction,
        };
        offence(PalwOffenceKindV1::PanelFalseValid, self.cards[card], borsh::to_vec(&payload).unwrap())
    }

    // ---- the real claim -----------------------------------------------------------------------

    /// **A claim opened from the producer's own work**, folded as the template block's own attempt.
    fn open_claim(&self, walk: &mut Walk, fault: Fault) -> RealClaim {
        self.open_claim_at_nonce(walk, fault, 0)
    }

    /// [`Self::open_claim`] on a template carrying `nonce` — another anchor, so another job.
    fn open_claim_at_nonce(&self, walk: &mut Walk, fault: Fault, nonce: u64) -> RealClaim {
        use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
        use misaka_palw_base0::produce::{base0_execute_for_attempt_v1, base0_material_decode_v1};

        // The template this node builds, and the job its anchor implies.
        let template = self.ctx.build_block_template_keeping_time(nonce);
        let header: Header = template.block.header.clone();
        assert!(
            kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id),
            "a ConsensusV2 template declares the attempt lane"
        );
        let bond = self.cards[EXECUTOR];
        let facts = self.ctx.consensus.palw_producer_facts_v2(self.floor(), Some(bond.0)).expect("testnet-12 answers for its floor");
        facts.ready_to_produce(&card_pubkey(EXECUTOR)).expect("card 0 is ready to produce");
        assert_eq!(facts.class_id, self.floor());
        assert_eq!(facts.artifact_root, self.artifact_root);
        let pre_pow = pre_pow_hash_64(&header);
        let anchor = execution_anchor_v3(self.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let (canonical, prompt) = self.backend.job_for_anchor(anchor).expect("the floor implies a job");
        let prefill_draw = self.config.params.palw_prefill_draw_active_at(header.daa_score);
        assert!(prefill_draw, "testnet-12 draws one forward (ADR-0117)");
        let job = palw_attempt_job_v1(canonical, prefill_draw);
        let honest = self.backend.execute(&job, &prompt).expect("the floor runs its own job");
        // The backend's `execute` IS the producer's run: `base0_execute_for_attempt_v1` of the same job.
        let artifact = misaka_palw_base0::rc::palw_rc_base0_artifact_v1().expect("the floor's artifact derives");
        let direct =
            base0_execute_for_attempt_v1(&artifact, self.backend.profile(), &job, &prompt).expect("the producer runs the job");
        assert_eq!(direct.execution_root, honest.execution_root, "one job, one execution");
        // The probe's rebind is the chain's own derivation: it reproduces the producer's commitment.
        {
            let mut again = direct.binding.clone();
            again.committed_execution_root = Hash64::default();
            rebind(&mut again);
            assert_eq!(again.committed_execution_root, direct.execution_root, "rebind recomputes verify_binding's root");
        }

        type Roots = AttemptRoots;
        let of = |o: &kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1| Roots {
            trace: o.trace_root,
            output: o.output_root,
            execution: o.execution_root,
            manifest: o.trace_manifest_root,
            chunks: o.trace_chunk_count,
        };
        let ladder = self.ladder(&walk.state);
        let honest_pin = kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
            logits_rows: direct.logits_rows.clone(),
            generated_token_ids: direct.generated_token_ids.clone(),
        };
        let mut pin = Some(honest_pin.clone());
        let (roots, binding, material, contradiction, fault_leaf) = match fault {
            Fault::Honest => {
                let (binding, ..) = base0_material_decode_v1(&honest.material).expect("the capture decodes");
                (of(&honest), binding, honest.material.clone(), None, None)
            }
            Fault::Relabel => {
                // Another anchor's canonical job and prompt — what the producer actually ran …
                let other = Hash64::from_u64_word(0x4E1A_BE10_0000_0000 ^ nonce);
                let (their, their_prompt) = self.backend.job_for_anchor(other).expect("the floor implies a job");
                assert_ne!(their_prompt, prompt, "another anchor names another prompt");
                // … with THIS block's anchor written where J1 and J3 look.
                let mut relabelled = palw_attempt_job_v1(their, prefill_draw);
                relabelled.job_id = anchor;
                relabelled.execution_seed = anchor.as_byte_slice()[..32].try_into().unwrap();
                let run = base0_execute_for_attempt_v1(&artifact, self.backend.profile(), &relabelled, &their_prompt)
                    .expect("the relabelled job runs");
                let material = misaka_palw_base0::produce::base0_material_encode_v1(&run).expect("encodes");
                pin = Some(kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                    logits_rows: run.logits_rows.clone(),
                    generated_token_ids: run.generated_token_ids.clone(),
                });
                let roots = Roots {
                    trace: run.trace_root,
                    output: run.output_root,
                    execution: run.execution_root,
                    manifest: run.trace_manifest_root,
                    chunks: run.trace_chunk_count,
                };
                let contradiction = C::IdentityMismatch { binding: run.binding.clone() };
                (roots, run.binding, material, Some(contradiction), None)
            }
            Fault::OutputGrind => {
                let ground = kaspa_consensus_core::palw_v2::output_commitment_v2(
                    &direct.binding.job_context.context_hash(),
                    &direct.generated_token_ids,
                    &kaspa_consensus_core::palw_v2::rendered_output_hash_v2(b"a rendering the class never makes"),
                );
                assert_ne!(ground, honest.output_root, "the ground root is not the run's");
                let roots = Roots { output: ground, ..of(&honest) };
                let contradiction = C::OutputMismatch {
                    binding: direct.binding.clone(),
                    pin: kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(honest_pin.clone()),
                };
                (roots, direct.binding.clone(), honest.material.clone(), Some(contradiction), None)
            }
            Fault::TraceSwap => {
                let swapped = Hash64::from_u64_word(0x7ACE_5A9E ^ nonce);
                let roots = Roots {
                    trace: swapped,
                    manifest: attempt_trace_manifest_root_v1(swapped, honest.trace_chunk_count),
                    ..of(&honest)
                };
                let contradiction = C::IdentityMismatch { binding: direct.binding.clone() };
                (roots, direct.binding.clone(), honest.material.clone(), Some(contradiction), None)
            }
            Fault::GarbageLogits => {
                // Bend the selecting row's runner-up lane toward — never past — the argmax, so the
                // committed token stays the row's own selection and no decode check can object.
                let mut rows = direct.logits_rows.clone();
                let row = &mut rows[0];
                let top = (0..row.len()).max_by_key(|i| (row[*i], std::cmp::Reverse(*i))).unwrap();
                let lane = (0..row.len()).find(|i| *i != top && row[*i] < row[top] - 1).expect("a lane below the argmax by two");
                row[lane] += 1;
                let mut binding = direct.binding.clone();
                binding.full_logits_trace_root = kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(
                    &binding.job_context,
                    &rows,
                    &direct.generated_token_ids,
                );
                rebind(&mut binding);
                verify_binding_v1(&binding).expect("the bent commitment is well-formed");
                pin = Some(kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 {
                    logits_rows: rows,
                    generated_token_ids: direct.generated_token_ids.clone(),
                });
                let roots = Roots {
                    trace: binding.full_logits_trace_root,
                    output: honest.output_root,
                    execution: binding.committed_execution_root,
                    manifest: attempt_trace_manifest_root_v1(binding.full_logits_trace_root, 1),
                    chunks: 1,
                };
                (roots, binding, honest.material.clone(), None, None)
            }
            Fault::Step | Fault::StepGather | Fault::StepAt(_) | Fault::KCacheWrite { .. } => {
                pin = None;
                // The first leaf the capture holds and the prover opens, from the middle of the step
                // space on (or from its first leaf, or at a named one). Nothing is chosen for
                // convictability: that the lie at this leaf convicts is asserted once the claim is
                // open. `Step` asks one more thing — that the step reads only its own segment, as the
                // adjudicator reads the step (`palw_false_valid_fault_site_v1` over the honest
                // capture's refutation at that leaf: the same coordinates, the same inputs).
                let (hb, tiles, ..) = base0_material_decode_v1(&honest.material).expect("the capture decodes");
                let held: BTreeSet<u64> = tiles.iter().map(|(i, _)| *i).collect();
                let n = hb.step_leaf_count;
                let k = palw_segment_count_v2(PANEL.len() as u16);
                let coords =
                    |leaf: u64| kaspa_consensus_core::palw_step::canonical_step_coordinates(&hb.shape_profile, &hb.job_context, leaf);
                let openable = |leaf: &u64| held.contains(leaf) && coords(*leaf).is_some();
                let segment = |leaf: u64| palw_segment_index_of_leaf_v2(n, k, leaf).expect("in the cut");
                let reads_its_own_segment = |leaf: u64| {
                    let refutation = self.backend.refutation_for_index(&honest.material, leaf).expect("the honest capture opens");
                    let step = C::StepArithmetic { refutation, operand_openings: Vec::new() };
                    match palw_false_valid_fault_site_v1(&step, ladder) {
                        Ok((PalwFaultSiteV1::Leaf { first_read, last_read, .. }, _)) => {
                            segment(first_read) == segment(leaf) && segment(last_read) == segment(leaf)
                        }
                        _ => false,
                    }
                };
                let leaf = match fault {
                    Fault::StepGather => (0..n).find(openable),
                    Fault::StepAt(leaf) => Some(leaf).filter(openable),
                    Fault::KCacheWrite { segment: wanted } => (0..n).find(|leaf| {
                        let stays_int8 = tiles.iter().find(|(i, _)| i == leaf).is_some_and(|(_, tile)| {
                            let mut lane = [tile.values_le[0], tile.values_le[1], tile.values_le[2], tile.values_le[3]];
                            lane[0] = lane[0].wrapping_add(1);
                            (-128..=127).contains(&i32::from_le_bytes(lane))
                        });
                        openable(leaf)
                            && segment(*leaf) == wanted
                            && coords(*leaf).and_then(|c| hb.shape_profile.resolve_node_slot(c.node_slot)).map(|(node, _)| node.role)
                                == Some(kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::KCacheWrite)
                            && stays_int8
                    }),
                    _ => (n / 2..n).find(|leaf| openable(leaf) && reads_its_own_segment(*leaf)),
                }
                .expect("the capture holds an openable step leaf");
                let lying = self.backend.execute_with_injected_fault(&job, &prompt, leaf).expect("the drill's fault runs");
                let refutation = self.backend.refutation_for_index(&lying.material, leaf).expect("the lying capture opens");
                let operand_openings = self.backend.operand_openings_for(&refutation).expect("the class opens the rows");
                assert_eq!(refutation.output_opening.leaf_index, leaf, "the refutation opens the faulted leaf");
                let contradiction = C::StepArithmetic { refutation, operand_openings };
                assert_ne!(lying.execution_root, honest.execution_root, "a different execution");
                let (binding, ..) = base0_material_decode_v1(&lying.material).expect("the lying capture decodes");
                (of(&lying), binding, lying.material.clone(), Some(contradiction), Some(leaf))
            }
            Fault::Forged => {
                pin = None;
                let rows = direct.logits_rows.clone();
                let mut toks = direct.generated_token_ids.clone();
                toks[0] = (toks[0] + 1) % self.backend.profile().vocab_size;
                let mut binding = direct.binding.clone();
                binding.full_logits_trace_root =
                    kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(&binding.job_context, &rows, &toks);
                rebind(&mut binding);
                verify_binding_v1(&binding).expect("the forged commitment is well-formed");
                let ctx_hash = binding.job_context.context_hash();
                let output = kaspa_consensus_core::palw_v2::output_commitment_v2(
                    &ctx_hash,
                    &toks,
                    &kaspa_consensus_core::palw_v2::rendered_output_hash_v2(&[]),
                );
                let roots = Roots {
                    trace: binding.full_logits_trace_root,
                    output,
                    execution: binding.committed_execution_root,
                    manifest: attempt_trace_manifest_root_v1(binding.full_logits_trace_root, 1),
                    chunks: 1,
                };
                let pin =
                    kaspa_consensus_core::palw_step_refute::PalwBase0DecodeTokensV1 { logits_rows: rows, generated_token_ids: toks };
                let contradiction = C::ForgedOutput { binding: binding.clone(), pin, position: 0 };
                (roots, binding, honest.material.clone(), Some(contradiction), None)
            }
            Fault::Shape => {
                pin = None;
                let mut binding = direct.binding.clone();
                binding.step_leaf_count += 1;
                rebind(&mut binding);
                verify_binding_v1(&binding).expect("the re-committed binding is well-formed");
                let roots = Roots { execution: binding.committed_execution_root, ..of(&honest) };
                let contradiction =
                    C::StepStructural(PalwStepRefutationV1 { binding: binding.clone(), evidence: PalwStepEvidenceV1::Shape });
                (roots, binding, honest.material.clone(), Some(contradiction), None)
            }
        };
        assert_eq!(binding.committed_execution_root, roots.execution, "the claim's root is its binding's");
        let (claim_id, envelope, header) = self.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots.as_attempt_roots());
        let claim = RealClaim { claim_id, anchor, job, prompt, envelope, binding, material, contradiction, fault_leaf, pin, header };
        if claim.contradiction.is_none() || matches!(fault, Fault::Honest | Fault::GarbageLogits) {
            assert_eq!(claim.binding.job_context.job_id, anchor, "an honest binding's job id is the block's execution anchor");
        }
        if let Some(contradiction) = &claim.contradiction
            && matches!(contradiction, C::StepArithmetic { .. } | C::StepStructural(_) | C::ForgedOutput { .. })
        {
            let root = walk.state.claim(&claim.claim_id).unwrap().execution_root;
            self.convicts(contradiction, root, ladder).expect("the proof convicts the claim's own committed root");
        }
        claim
    }

    /// **A claim that carries ANOTHER claim's roots** (F1 T18b, the borrowed root): the template on
    /// `nonce` — another block, another anchor — carrying `lender`'s trace, output and execution
    /// roots and nothing of its own run. The chain admits it (nothing in admission rebuilds a root);
    /// its binding, which is the lender's, answers the lender's anchor.
    fn open_borrowed_claim(&self, walk: &mut Walk, nonce: u64, lender: &RealClaim) -> RealClaim {
        use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
        let template = self.ctx.build_block_template_keeping_time(nonce);
        let header: Header = template.block.header.clone();
        let bond = self.cards[EXECUTOR];
        let facts = self.ctx.consensus.palw_producer_facts_v2(self.floor(), Some(bond.0)).expect("testnet-12 answers for its floor");
        let pre_pow = pre_pow_hash_64(&header);
        let anchor = execution_anchor_v3(self.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        assert_ne!(anchor, lender.anchor, "another block, another anchor");
        let lent = &lender.envelope.attempt;
        let roots = AttemptRoots {
            trace: lent.trace_root,
            output: lent.output_root,
            execution: lent.execution_root,
            manifest: lent.trace_manifest_root,
            chunks: lent.trace_chunk_count,
        };
        let (claim_id, envelope, header) = self.fold_own_attempt(walk, header, anchor, pre_pow, &facts, roots);
        RealClaim {
            header,
            claim_id,
            anchor,
            job: lender.job.clone(),
            prompt: lender.prompt.clone(),
            envelope,
            binding: lender.binding.clone(),
            material: lender.material.clone(),
            contradiction: Some(C::IdentityMismatch { binding: lender.binding.clone() }),
            fault_leaf: None,
            pin: lender.pin.clone(),
        }
    }

    /// The attempt carrying `roots`, with `palw_producer_facts_v2`'s facts, signed by card 0 and
    /// folded as the template block's OWN work — as the pipeline folds it. Asserts the three facts
    /// F2 exists for and returns the claim the fold opened (its job, binding and proof to be filled).
    fn fold_own_attempt(
        &self,
        walk: &mut Walk,
        mut header: Header,
        anchor: Hash64,
        pre_pow: Hash64,
        facts: &kaspa_consensus_core::palw_producer_v2::PalwProducerFactsV2,
        roots: AttemptRoots,
    ) -> (Hash64, PalwAttemptEnvelopeV2, Header) {
        use kaspa_consensus_core::hashing::header::pre_pow_hash_64;
        let bond = self.cards[EXECUTOR];
        // The attempt, with `palw_producer_facts_v2`'s facts, signed by card 0.
        let bond_facts = facts.bond.as_ref().expect("a genesis card is a registered bond");
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.domain,
            challenge: challenge_v2(self.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(EXECUTOR),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: roots.trace,
            output_root: roots.output,
            execution_root: roots.execution,
            pwu: facts.pwu,
            trace_manifest_root: roots.manifest,
            trace_chunk_count: roots.chunks,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let signature = sign(EXECUTOR, attempt_id_v2(&attempt).as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT);
        let envelope = PalwAttemptEnvelopeV2 { attempt, signature };
        header.palw_commitment = envelope.encode_wire();
        header.finalize();
        assert_eq!(pre_pow_hash_64(&header), pre_pow, "the carriage is outside the pre-PoW hash, so the anchor is the block's");

        // Folded as this block's OWN work, as the pipeline folds it — at the template's own point, or,
        // for a second claim on a walk the first already advanced (the node's virtual has not moved:
        // no block is mined here), one blue score and one DAA past the walk. The anchor is the
        // header's pre-PoW hash either way; the fold reads the point only for its clocks.
        let daa_score = if header.daa_score >= walk.daa && header.blue_score > walk.blue { header.daa_score } else { walk.daa + 1 };
        let point = PalwBlockContextV2 {
            block: header.hash,
            daa_score,
            blue_score: header.blue_score.max(walk.blue + 1),
            subsidy: self.vp().coinbase_manager.calc_block_subsidy(daa_score),
        };
        assert!(point.blue_score > walk.blue && point.daa_score >= walk.daa, "the template block follows the walk");
        let (next, _delta, skips) = self
            .vp()
            .palw_v2_fold_attempt_for_tests(&walk.state, self.sp(), &point, &[], &envelope, &header)
            .expect("the block's own attempt folds");
        assert!(skips.is_empty(), "the own attempt is not skipped: {skips:?}");
        let before: BTreeSet<Hash64> = PalwStateCarriageV2::from_state(&walk.state).claims.keys().copied().collect();
        let opened: Vec<Hash64> =
            PalwStateCarriageV2::from_state(&next).claims.keys().copied().filter(|id| !before.contains(id)).collect();
        assert_eq!(opened.len(), 1, "the attempt opened one claim");
        let claim_id = opened[0];
        walk.advance(&point, next);

        // **Up front, the three facts F2 exists for, and F1's.**
        assert_eq!(claim_id, attempt_id_v2(&envelope.attempt), "the chain keyed the claim under attempt_id_v2(attempt)");
        assert_ne!(anchor, claim_id, "the anchor is not the claim id — the V1 rule's job_id == claim_id is a fixed point");
        let claim = walk.state.claim(&claim_id).expect("the claim is live");
        assert!(matches!(claim.phase, PalwClaimPhaseV2::Provisional) && matches!(claim.source, PalwClaimSourceV2::Attempt));
        assert_eq!((claim.execution_root, claim.trace_root, claim.bond), (roots.execution, roots.trace, bond));
        // F1 (J-1): past `palw_offence_attribution` the claim records the anchor its block carried.
        let recorded = if self.config.params.palw_offence_attribution_active_at(point.daa_score) { anchor } else { Hash64::default() };
        assert_eq!(claim.job_identity, recorded, "the claim records the anchor its carrying header names");
        (claim_id, envelope, header)
    }

    /// `PanelBound` with cards 1–5, folded directly.
    fn bind(&self, walk: &mut Walk, claim: Hash64) {
        let seats: Vec<PalwPanelSeatV2> = PANEL
            .iter()
            .map(|&i| PalwPanelSeatV2 {
                bond: self.cards[i],
                operator_id: walk.state.bond(&self.cards[i]).expect("a card").operator_id,
            })
            .collect();
        let point = walk.next();
        let object = Obj::PanelBound { claim, anchor: Hash64::from_u64_word(0x46A0_0000_0000_00B1), seats };
        let next = self.fold(&walk.state, &point, &[object]).expect("the panel binds");
        assert!(matches!(next.claim(&claim).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "every seat could lock");
        walk.advance(&point, next);
    }

    /// **The Verification V2 licence**: every seat's V3 `Valid` under its assigned mask.
    fn license_v2(&self, walk: &mut Walk, claim: Hash64) -> Licence {
        let panel = walk.state.panel(&claim).expect("a bound panel").clone();
        let assignment = palw_segment_assignment_v2(panel.anchor, claim, panel.seats.len() as u16);
        assert_eq!(assignment.segments, 4, "a five-seat panel cuts the job in four");
        let point = walk.next();
        let receipts: Vec<(usize, PalwSeatReceiptV3)> = panel
            .seats
            .iter()
            .enumerate()
            .map(|(i, seat)| {
                let card = self.card_of(&seat.bond);
                (card, self.v3_receipt(card, claim, self.domain, point.daa_score, assignment.mask_of(i as u16)))
            })
            .collect();
        let object = Obj::ReceiptLicensedV2 { claim, receipts: receipts.iter().map(|(_, r)| r.clone()).collect() };
        self.carry(walk, vec![object]);
        assert!(matches!(walk.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the licence folds");
        for (card, _) in &receipts {
            assert!(walk.state.slashable_lock(self.cards[*card], claim).is_some(), "card {card}'s Valid is locked");
        }
        Licence { assignment, receipts, licensed_daa: point.daa_score }
    }

    /// **The V1 licence** (`ReceiptLicensed`): every seat's full V2 `Valid` — the door still open on
    /// testnet-12.
    fn license_v1(&self, walk: &mut Walk, claim: Hash64) -> Vec<(usize, PalwSeatReceiptV2)> {
        let point = walk.next();
        let receipts: Vec<(usize, PalwSeatReceiptV2)> =
            PANEL.iter().map(|&card| (card, self.full_receipt(card, claim, self.domain, point.daa_score))).collect();
        let object = Obj::ReceiptLicensed { claim, receipts: receipts.iter().map(|(_, r)| r.clone()).collect() };
        self.carry(walk, vec![object]);
        assert!(matches!(walk.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the V1 licence folds");
        receipts
    }

    /// A real claim of `fault`, bound and licensed through Verification V2.
    fn licensed(&self, fault: Fault) -> (Walk, RealClaim, Licence) {
        self.licensed_at_nonce(fault, 0)
    }

    /// [`Self::licensed`] on a template carrying `nonce`.
    fn licensed_at_nonce(&self, fault: Fault, nonce: u64) -> (Walk, RealClaim, Licence) {
        let mut walk = self.genesis_walk();
        let claim = self.open_claim_at_nonce(&mut walk, fault, nonce);
        self.bind(&mut walk, claim.claim_id);
        let licence = self.license_v2(&mut walk, claim.claim_id);
        (walk, claim, licence)
    }

    /// The loader's own check (`dos_g2`'s `reloads`): the carriage round-trips through borsh and
    /// `into_state_v3` against the recorded root, with this network's flags.
    fn reloads(&self, s: &PalwChainStateV2) {
        let daa = s.last_point().map(|p| p.daa_score).unwrap_or(0);
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("serializes");
        let c: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
        let back = c
            .into_state_v3(
                self.sp(),
                Some(s.state_root()),
                self.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
                self.config.params.palw_canonical_work_daa(),
            )
            .expect("the state the fold wrote passes the loader's consistency check");
        assert_eq!(back.state_root(), s.state_root());
    }

    /// **A restart**: `s` written as this node's tip row and read back by the node's own loader
    /// (`load_tip`, which every virtual resolution runs), then the genesis row restored.
    fn restarts(&self, block: kaspa_consensus_core::BlockHash, s: &PalwChainStateV2) {
        let store = &self.vp().palw_state_v2_store;
        let (genesis_block, _) = store.read().load_tip(self.sp()).unwrap().expect("a tip");
        store.write().set_tip_for_tests(block, s).expect("the tip row writes");
        let (loaded_block, loaded) = store.read().load_tip(self.sp()).unwrap().expect("the tip row loads");
        assert_eq!((loaded_block, loaded.state_root()), (block, s.state_root()), "the node reloads the state the fold wrote");
        store.write().set_tip_for_tests(genesis_block, &self.genesis).expect("the genesis row is restored");
    }

    /// The chain with the claim's state rebuilt through the carriage by `edit` — the way `dos_g2`
    /// and `dos_repro_1` write the rows no block of this harness reaches.
    fn rebuilt(&self, s: &PalwChainStateV2, edit: impl FnOnce(&mut PalwStateCarriageV2)) -> PalwChainStateV2 {
        let daa = s.last_point().map(|p| p.daa_score).unwrap_or(0);
        let mut c = PalwStateCarriageV2::from_state(s);
        edit(&mut c);
        c.into_state_v3(
            self.sp(),
            None,
            self.config.params.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa)),
            self.config.params.palw_canonical_work_daa(),
        )
        .expect("the rebuilt carriage is consistent")
    }
}

/// What a conviction before `Final` must have written, for each convicted seat: its lock taken
/// FIRST and its bond reduced by exactly that lock, one kind-3 row under the (seat, claim) key with
/// the claim's root, the claim voided `CourtFraud` at the conviction, its executor charged its
/// reservation and escrow, and the liability row saying so.
fn assert_convicted_before_final(
    h: &H,
    licensed: &PalwChainStateV2,
    s: &PalwChainStateV2,
    claim_id: Hash64,
    convicted: &[usize],
    daa: u64,
) {
    let claim = licensed.claim(&claim_id).expect("the licensed claim").clone();
    for &card in convicted {
        let seat = h.cards[card];
        let lock = *licensed.slashable_lock(seat, claim_id).expect("the Valid seat locked at the licence");
        assert!(lock.amount > 0, "card {card} locked collateral for its Valid");
        assert!(s.slashable_lock(seat, claim_id).is_none(), "card {card}'s lock is taken");
        assert_eq!(
            s.bond(&seat).unwrap().collateral as u128,
            licensed.bond(&seat).unwrap().collateral as u128 - lock.amount,
            "card {card}'s bond is reduced by exactly its lock"
        );
        let row =
            s.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &claim_id)).expect("one row under the (seat, claim) key");
        assert_eq!(
            (row.kind, row.accused, row.amount as u128, row.accepted_daa, row.execution_root),
            (PalwOffenceKindV1::PanelFalseValidV2, seat.0, lock.amount, daa, claim.execution_root),
            "card {card}: kind 3, the lock, the claim's root"
        );
    }
    assert!(
        matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == daa),
        "the claim is voided CourtFraud at the conviction: {:?}",
        s.claim(&claim_id).unwrap().phase
    );
    let escrow = h.sp().claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
    assert!(escrow > 0, "the claim escrowed its block's worker carve");
    let executor = h.cards[EXECUTOR];
    assert_eq!(
        s.bond(&executor).unwrap().collateral as u128,
        licensed.bond(&executor).unwrap().collateral as u128 - (claim.reserved + escrow + claim.rights_reserved),
        "the executor pays its reservation and its escrow, as a court's CourtFraud charges it"
    );
    let row = s.panel_liability(&claim_id).expect("the void keeps the liability row");
    assert_eq!((row.voided_daa, row.void_reason), (Some(daa), Some(PalwVoidReasonV2::CourtFraud)), "the row says CourtFraud");
    assert!(
        !row.valid_signers.iter().any(|(seat, _)| *seat == h.cards[convicted[0]].0),
        "the seat convicted first gave up its lock before the void, so the row does not list it again"
    );
    assert!(s.palw_execution_root_is_forfeited_v1(&claim.execution_root), "the proven-false root is forfeited");
}

// ---------------------------------------------------------------------------------------------
// T46a — the red state
// ---------------------------------------------------------------------------------------------

/// **T46a: on the V1 rule a real claim cannot be prosecuted** (fence off). The claim's binding
/// names the anchor, its id is `attempt_id_v2`, and the V1 gate asks `job_id == claim_id`: with the
/// full (V2-signed) receipt the real contradiction is `PanelFalseValidWorkMismatch`, and with the V3
/// receipt the seat actually signed it is `PanelFalseValidReceiptUnverified`. The V2 kind is dormant.
#[tokio::test]
async fn t46a_real_claim_is_red_on_the_v1_rule() {
    let h = harness(false);
    let (walk, claim, licence) = h.licensed(Fault::Step);
    assert_eq!(claim.binding.job_context.job_id, claim.anchor, "the contradiction's job id is the anchor");
    assert_ne!(claim.binding.job_context.job_id, claim.claim_id, "so `job_id == claim_id` cannot hold");
    let contradiction = claim.contradiction();
    let full = licence.full_card();
    let v2_signed = h.full_receipt(full, claim.claim_id, h.domain, licence.licensed_daa);
    h.refused(&walk, &h.v1(full, claim.claim_id, v2_signed, contradiction.clone()), &E::PanelFalseValidWorkMismatch.to_string());
    let v3_signed = licence.receipt(full).receipt.clone();
    h.refused(&walk, &h.v1(full, claim.claim_id, v3_signed, contradiction.clone()), &E::PanelFalseValidReceiptUnverified.to_string());
    h.refused(&walk, &h.v2(full, claim.claim_id, licence.segmented(full), contradiction), &E::AttributionDormant.to_string());
}

// ---------------------------------------------------------------------------------------------
// T46b–d — before Final
// ---------------------------------------------------------------------------------------------

/// **T46b: the drill's injected step fault convicts before `Final`.** The full seat and the partial
/// holder of the faulted leaf's segment are convicted in one block — the step at that leaf reads only
/// leaves of the same segment (asserted on the adjudicator's own site), so the holder's replay alone
/// had to see the lie; the other three partial seats attested other segments and are refused
/// `SiteNotAttested`. (A leaf whose step reads another segment convicts no partial seat: T46p.) The
/// claim is voided `CourtFraud` and
/// its executor charged; a sweep past the height the unconvicted claim reaches `Final` at leaves it
/// voided and `safe_weight` where the licence left it — while the same sweep without the conviction
/// finalises the lie and adds its weight.
#[tokio::test]
async fn t46b_injected_step_fault_convicts_before_final() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let leaf = claim.fault_leaf.unwrap();
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let full = licence.full_card();
    let holder = licence.holder_of(segment);
    let others: Vec<usize> = licence.partials().into_iter().filter(|card| *card != holder).collect();
    assert_eq!(others.len(), 3);
    eprintln!(
        "[t46b] leaf {leaf} of {} is in segment {segment}; full seat card {full}, holder card {holder}, others {others:?}",
        claim.binding.step_leaf_count
    );
    let payload = borsh::to_vec(&h.v2_payload(full, id, licence.segmented(full), c.clone())).unwrap();
    let finding = palw_check_panel_false_valid_v2(&walk.state, &h.cards[full], &payload, false, false, h.rules(), None)
        .expect("the full seat is liable");
    let PalwFaultSiteV1::Leaf { leaf: at, first_read, last_read } = finding.site else { panic!("a step site: {:?}", finding.site) };
    eprintln!("[t46b] the step read leaves {first_read}..={last_read}");
    assert_eq!(at, leaf, "the site is the faulted leaf");
    for read in [first_read, last_read] {
        assert_eq!(
            palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, read),
            Some(segment),
            "every leaf the step read is in the holder's segment"
        );
    }
    for &other in &others {
        h.refused(&walk, &h.v2(other, id, licence.segmented(other), c.clone()), &E::SiteNotAttested.to_string());
    }
    let unconvicted = walk.clone();
    let (licensed, _) =
        h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c.clone()), h.v2(holder, id, licence.segmented(holder), c)]);
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full, holder], walk.daa);
    for &other in &others {
        assert_eq!(walk.state.bond(&h.cards[other]).unwrap().collateral, licensed.bond(&h.cards[other]).unwrap().collateral);
        assert!(walk.state.slashable_lock(h.cards[other], id).is_some(), "card {other} was not charged");
    }
    let weight = walk.state.safe_weight();
    assert_eq!(weight, licensed.safe_weight(), "a claim that never finalised weighed nothing");

    // Without the conviction the same claim finalises and weighs.
    let mut counterfactual = unconvicted;
    let before_final = h.sweep_to_final(&mut counterfactual, id);
    assert!(counterfactual.state.safe_weight() > before_final.safe_weight(), "unconvicted, the lie finalises and weighs");
    let window_end = licence.licensed_daa + h.sp().window_challenge_at(licence.licensed_daa);
    let past = counterfactual.daa.max(window_end) + 1;
    // With it, a sweep past that height leaves the claim voided and the weight unchanged.
    let point = walk.at(past);
    let swept = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
    assert!(
        matches!(swept.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. })),
        "the voided claim stays voided past L + window"
    );
    assert_eq!(swept.safe_weight(), weight, "and adds no weight");
}

/// **T46c: a forged output convicts the full seat only.** A decoded token is a whole-execution fault:
/// only a full attestation covers it, so every partial seat is refused `SiteNotAttested`.
#[tokio::test]
async fn t46c_forged_output_full_mask_only() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Forged);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let full = licence.full_card();
    for partial in licence.partials() {
        h.refused(&walk, &h.v2(partial, id, licence.segmented(partial), c.clone()), &E::SiteNotAttested.to_string());
    }
    let (licensed, _) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full], walk.daa);
}

/// **T46d: a structural shape fault — a non-canonical step-leaf count — convicts at `Whole`.** The
/// shape pass answers from the binding alone, so the full seat is convicted and every partial seat
/// is refused `SiteNotAttested`.
#[tokio::test]
async fn t46d_structural_shape() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Shape);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let full = licence.full_card();
    for partial in licence.partials() {
        h.refused(&walk, &h.v2(partial, id, licence.segmented(partial), c.clone()), &E::SiteNotAttested.to_string());
    }
    let (licensed, _) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full], walk.daa);
}

// ---------------------------------------------------------------------------------------------
// T46e–f — after Final, after retirement
// ---------------------------------------------------------------------------------------------

/// **T46e: after `Final` the conviction reverses it.** The claim is voided `CourtFraud`,
/// `safe_weight` falls by exactly what its `Final` added, the liability row (an honest-looking
/// `Final` until now) is marked, and a minted schedule's tickets for the convicted root leave it
/// while an honest work's keep their rounds. No executor charge after `Final` (SR-8, the peer's) is
/// pinned as the residual it is.
#[tokio::test]
async fn t46e_after_final() {
    use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecFinalV1, PalwExecScheduleV1, palw_execution_schedule_snapshot_v1};
    use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_bounded_v1};
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let before_final = h.sweep_to_final(&mut walk, id);
    let at_final = walk.state.clone();
    let contribution = at_final.safe_weight() - before_final.safe_weight();
    assert!(contribution > 0, "the Final added its weight");
    let row = at_final.panel_liability(&id).expect("a Final keeps its liability row");
    assert_eq!((row.voided_daa, row.void_reason), (None, None), "an honest-looking Final");
    let root = claim.envelope.attempt.execution_root;

    // A schedule minted with this Final's execution, beside an honest work's, written through the
    // carriage as `dos_g2` writes one (the first block of span n + 2 is what mints it).
    let (span, finals) = at_final.round_finals();
    let recorded = finals.values().find(|f| f.claim_id == id).copied();
    eprintln!("[t46e] round_finals span {span}: this claim's Final recorded = {recorded:?}");
    let convicted = PalwExecFinalV1 {
        credit: 4 * PALW_EXECUTION_QUANTUM_V1,
        ..recorded.unwrap_or(PalwExecFinalV1 {
            domain: Hash64::from_u64_word(0xD1),
            bond: h.cards[EXECUTOR],
            operator_id: at_final.bond(&h.cards[EXECUTOR]).unwrap().operator_id,
            claim_id: id,
            execution_root: root,
            credit: 0,
        })
    };
    assert_eq!((convicted.claim_id, convicted.execution_root), (id, root));
    let honest = PalwExecFinalV1 {
        domain: Hash64::from_u64_word(0xD2),
        bond: h.cards[BYSTANDER],
        operator_id: at_final.bond(&h.cards[BYSTANDER]).unwrap().operator_id,
        claim_id: Hash64::from_u64_word(0x4011_0E57),
        execution_root: Hash64::from_u64_word(0x4011_0E57_0000),
        credit: 3 * PALW_EXECUTION_QUANTUM_V1,
    };
    let minted_span = span + 7;
    let schedule = {
        let finals = [convicted, honest];
        let quanta = palw_execution_mint_quanta_bounded_v1(
            &finals,
            Hash64::from_u64_word(0x5EED_0046),
            u128::from(PALW_EXECUTION_QUANTUM_V1),
            10_000,
            0,
            &Default::default(),
            1 << 16,
        );
        let domains = palw_execution_schedule_snapshot_v1(minted_span, &finals).domains;
        PalwExecScheduleV1 {
            span_index: minted_span,
            seed: Hash64::from_u64_word(0x5EED_0046),
            domains,
            finals: finals.to_vec(),
            quanta,
        }
    };
    assert!(schedule.quanta.iter().any(|q| q.final_id == id), "the convicted work holds tickets");
    walk.state = h.rebuilt(&at_final, |c| {
        c.round_schedules.insert(minted_span, schedule.clone());
    });
    let with_schedule = walk.state.clone();
    let control = h.fold(&with_schedule, &walk.next(), &[]).expect("an empty block folds");
    assert_eq!(control.round_schedule(minted_span), Some(&schedule), "without a conviction the schedule stands");

    let full = licence.full_card();
    let executor_before = with_schedule.bond(&h.cards[EXECUTOR]).unwrap().collateral;
    let lock = *with_schedule.slashable_lock(h.cards[full], id).expect("the full seat's lock stands after Final");
    let (_, _) = h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c)]);
    let s = &walk.state;
    let daa = walk.daa;
    assert!(
        matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == daa),
        "reverse_convicted_final voided the Final"
    );
    assert_eq!(s.safe_weight(), with_schedule.safe_weight() - contribution, "safe_weight falls by exactly what the Final added");
    assert_eq!(s.safe_weight(), before_final.safe_weight());
    let row = s.panel_liability(&id).expect("the row stands");
    assert_eq!((row.voided_daa, row.void_reason), (Some(daa), Some(PalwVoidReasonV2::CourtFraud)), "the liability row is marked");
    let pruned = s.round_schedule(minted_span).expect("the span keeps its schedule");
    assert!(pruned.quanta.iter().all(|q| q.final_id != id), "the convicted work's tickets left the schedule");
    assert!(pruned.finals.iter().all(|f| f.execution_root != root), "and its Final left the list");
    let honest_before: Vec<_> = schedule.quanta.iter().filter(|q| q.final_id == honest.claim_id).copied().collect();
    let honest_after: Vec<_> = pruned.quanta.iter().filter(|q| q.final_id == honest.claim_id).copied().collect();
    assert!(!honest_before.is_empty());
    assert_eq!(honest_after, honest_before, "the honest tickets keep their rounds");
    assert!(s.palw_execution_root_is_forfeited_v1(&root));
    assert!(s.slashable_lock(h.cards[full], id).is_none());
    assert_eq!(
        s.bond(&h.cards[full]).unwrap().collateral as u128,
        with_schedule.bond(&h.cards[full]).unwrap().collateral as u128 - lock.amount
    );
    let consumed = s.consumed_offence(&palw_false_valid_offence_id_v2(&h.cards[full].0, &id)).expect("one kind-3 row");
    assert_eq!(
        (consumed.kind, consumed.amount as u128, consumed.execution_root),
        (PalwOffenceKindV1::PanelFalseValidV2, lock.amount, root)
    );
    // RESIDUAL (spec §5 item 8, SR-8 is the peer's): no executor charge after Final.
    assert_eq!(
        s.bond(&h.cards[EXECUTOR]).unwrap().collateral,
        executor_before,
        "SR-8 residual: the executor is not charged after Final"
    );
    h.reloads(s);
}

/// **T46f: after retirement only the liability row is left.** A `Full` receipt the seat signed
/// through the V1 licence path is convicted against the row. **F1 lifts F2's `SegmentsUnknown`:**
/// past `palw_offence_attribution` the row records the claim's segment cut (and its identity, lane
/// and trace root), so a V3 full-mask receipt is placed and convicts against the row too — while a
/// partial mask cannot be, the panel that assigned it having retired (`SegmentMaskNotAssigned`).
#[tokio::test]
async fn t46f_after_retirement() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    h.bind(&mut walk, id);
    let receipts = h.license_v1(&mut walk, id);
    h.sweep_to_final(&mut walk, id);
    let PalwClaimPhaseV2::Final { final_daa } = walk.state.claim(&id).unwrap().phase else { unreachable!() };
    let retirement = h.sp().claim_retirement_daa();
    assert!(retirement > 0, "testnet-12 retires terminal claims");
    let row = walk.state.panel_liability(&id).expect("the Final's liability row").clone();
    eprintln!("[t46f] Final at {final_daa}; retirement after {retirement}; liability row expires at {}", row.expiry_daa);
    let point = walk.at(final_daa + retirement + 1);
    let retired = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
    walk.advance(&point, retired);
    assert!(walk.state.claim(&id).is_none(), "the claim retired");
    assert!(walk.state.panel(&id).is_none(), "and its panel with it");
    let row = walk.state.panel_liability(&id).expect("the liability row outlives the claim").clone();
    assert_eq!((row.voided_daa, row.void_reason), (None, None));

    let (card, receipt) = receipts[0].clone();
    let seat = h.cards[card];
    assert!(row.valid_signers.iter().any(|(s, _)| *s == seat.0), "the row lists the seat");
    // F1: the row recorded what the claim record held.
    assert_eq!(
        (row.job_identity, row.free_prompt, row.trace_root, row.segment_count),
        (claim.anchor, false, claim.envelope.attempt.trace_root, 4),
        "the row copies the identity, the lane, the trace root and the five-seat panel's cut"
    );
    // The V3 form of another seat's Valid: a full mask is placed by the recorded cut and convicts;
    // a partial mask is not, its assignment having retired with the panel.
    let (other, other_receipt) = receipts[1].clone();
    let partial = h.v3_receipt(
        other,
        id,
        h.domain,
        other_receipt.signed_daa,
        kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::single(0),
    );
    h.refused(&walk, &h.v2(other, id, PalwFalseValidReceiptV1::Segmented(partial), c.clone()), &E::SegmentMaskNotAssigned.to_string());
    let v3 = h.v3_receipt(
        other,
        id,
        h.domain,
        other_receipt.signed_daa,
        kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2::full(4),
    );
    let other_before = walk.state.bond(&h.cards[other]).unwrap().collateral;
    let other_lock = walk.state.slashable_lock(h.cards[other], id).map(|l| l.amount).unwrap_or(0);
    h.carry(&mut walk, vec![h.v2(other, id, PalwFalseValidReceiptV1::Segmented(v3), c.clone())]);
    assert_eq!(
        walk.state.bond(&h.cards[other]).unwrap().collateral as u128,
        other_before as u128 - other_lock,
        "the full-mask V3 seat is convicted against the row"
    );
    // The full receipt the V1 licence carried: convicted against the row.
    let lock = walk.state.slashable_lock(seat, id).copied();
    let (before, _) = h.carry(&mut walk, vec![h.v2(card, id, PalwFalseValidReceiptV1::Full(receipt), c)]);
    let s = &walk.state;
    let charged = lock.map(|l| l.amount).unwrap_or(0);
    eprintln!("[t46f] the seat's lock at conviction: {lock:?}");
    assert_eq!(s.bond(&seat).unwrap().collateral as u128, before.bond(&seat).unwrap().collateral as u128 - charged);
    assert!(s.slashable_lock(seat, id).is_none());
    let consumed = s.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).expect("one kind-3 row");
    assert_eq!(
        (consumed.kind, consumed.amount as u128, consumed.execution_root),
        (PalwOffenceKindV1::PanelFalseValidV2, charged, row.execution_root)
    );
    let marked = s.panel_liability(&id).expect("the row stands");
    assert_eq!(marked.void_reason, Some(PalwVoidReasonV2::CourtFraud), "the row is marked (by the first conviction)");
    assert!(s.palw_execution_root_is_forfeited_v1(&row.execution_root));
}

// ---------------------------------------------------------------------------------------------
// T46g — the free-prompt lane
// ---------------------------------------------------------------------------------------------

/// **T46g: a free-prompt claim, from the drill's own FP lie.** The commitment is
/// `palw_fp_commitment_from_context_v3` of `execute_free_prompt_with_injected_fault`'s run, carried
/// on a 0x4a transaction and extracted by the processor's own walk; its binding's job id is
/// `fp_job_id_v3(job)` and not the claim id, and the full seat and the leaf's partial holder are
/// convicted.
///
/// A multi-token free prompt decodes, so the lie is placed as T46b places it (F2 review, F-1): at
/// the first leaf from the middle on whose step reads only its own segment — here a step at a decode
/// call that reads no generated token, which the prover's refutation nonetheless pins. The pinned
/// proof convicts the full seat but reads (as far as the adjudicator can tell) the generated tokens,
/// which a partial seat derives from its own replay and never compares: its holder is
/// `SiteNotAttested`. The filer files the same proof without the pin — the checker adjudicates the
/// step without it, which is the proof it was never read — and the holder is convicted.
#[tokio::test]
async fn t46g_fp_claim() {
    use kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION,
        PalwFpCommitmentTxPayloadV3, PalwFreePromptJobV3, fp_claim_id_v3, fp_job_id_v3,
    };
    use misaka_palw_base0::produce::base0_material_decode_v1;
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let floor = h.floor();
    let facts = h.ctx.consensus.palw_producer_facts_v2(floor, Some(h.cards[EXECUTOR].0)).expect("a V2 network answers");
    eprintln!(
        "[t46g] at genesis: fp_certified {}, work profile published {}, derived-work fence {:?}",
        facts.fp_certified,
        walk.state.fp_work_profile_of(&floor).is_some(),
        h.config.params.palw_fp_derived_work_fence()
    );
    // **The spec's open question — does the harness state need the lane certified? — answered by
    // the chain: yes.** testnet-12 names the floor free-prompt-certified in its params
    // (`fp_certified`), but arms ADR-0145's derived-work fence at genesis without the floor's graph
    // published, so the processor's walk skips every commitment on it ("the class has published no
    // shape profile"); and the one object that publishes it, `ClassLaneCertified`, needs a certified
    // free-prompt family in STATE covering the class's kernels, which genesis does not hold
    // (`NoCertifiedFamilyCovers`). So the family enters through its own drill evidence
    // (`FamilyCertified`, graded by the court) and the class through `ClassLaneCertified` — both
    // through the gate, the walk and the fold.
    assert!(facts.fp_certified, "the params certify the floor's free-prompt lane");
    assert!(walk.state.fp_work_profile_of(&floor).is_none(), "and genesis has not published its graph");
    let lane_certified = Obj::ClassLaneCertified {
        class_id: floor,
        lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt,
        profile: Box::new(h.backend.profile().clone()),
    };
    assert!(
        matches!(
            h.fold(&walk.state, &walk.next(), std::slice::from_ref(&lane_certified)),
            Err(PalwStateV2Error::NoCertifiedFamilyCovers { .. })
        ),
        "no free-prompt family is certified in genesis state"
    );
    let evidence = misaka_palw_base0::e2e_drill::rc_free_prompt_evidence_v1(misaka_palw_base0::e2e_drill::PalwRcFamilyV1::Base0)
        .expect("the floor drills its free-prompt lane");
    h.carry(
        &mut walk,
        vec![Obj::FamilyCertified {
            evidence: Box::new(kaspa_consensus_core::palw_state_v2::PalwCertificationEvidenceV1::FreePrompt(evidence)),
        }],
    );
    h.carry(&mut walk, vec![lane_certified]);
    assert!(walk.state.fp_work_profile_of(&floor).is_some(), "the class's graph is published");

    // The caller's prompt, as long a job as the class's context holds.
    let ids: Vec<u32> = vec![3, 5, 8, 13];
    let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    let n_ctx = h.backend.profile().n_ctx;
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: h.domain,
        class_id: floor,
        executor_bond: h.cards[EXECUTOR].0,
        executor_pubkey: card_pubkey(EXECUTOR),
        operator_id: walk.state.bond(&h.cards[EXECUTOR]).unwrap().operator_id,
        anchor_block: h.config.params.genesis.hash,
        anchor_daa: walk.daa,
        job_nonce: [0x46; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            h.backend.prompt_ids_form(),
            &ids,
        )
        .expect("a short prompt commits"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: n_ctx - ids.len() as u32,
        max_context_tokens: n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    };
    let honest = h.backend.execute_free_prompt(&job, &prompt).expect("the floor runs a caller's prompt");
    let (hb, tiles, ..) = base0_material_decode_v1(&honest.outcome.material).expect("decodes");
    let held: BTreeSet<u64> = tiles.iter().map(|(i, _)| *i).collect();
    let n = hb.step_leaf_count;
    let ladder = h.ladder(&walk.state);
    // The honest step at `leaf`, as a filer would send it without the generated-token pin: it must
    // adjudicate without it (it reads no token) and read only its own segment of the four-way cut.
    let resolved = misaka_palw_base0::classes::resolve_class_v1(&h.bundle.court, floor, h.artifact_root, &[]).expect("resolves");
    let inventory =
        misaka_palw_base0::inventory::base0_inventory_v1(&resolved.artifact, resolved.inventory_geometry).expect("inventory");
    let k = palw_segment_count_v2(PANEL.len() as u16);
    let decides_alone = |leaf: u64| {
        let Ok(mut refutation) = h.backend.refutation_for_free_prompt_index(&honest.outcome.material, leaf, &ids) else {
            return false;
        };
        refutation.decode_tokens = None;
        let recorder = kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1::new(inventory.operands());
        let adjudicable = matches!(
            kaspa_consensus_core::palw_step_refute::check_execution_step_refutation_carried_capped_v1(
                &refutation,
                &recorder,
                h.form(),
                h.backend.step_ladder_cap()
            ),
            Err(kaspa_consensus_core::palw_step_refute::PalwStepRefuteError::NoFaultFound)
        );
        let segment = |l: u64| palw_segment_index_of_leaf_v2(n, k, l);
        adjudicable
            && matches!(
                palw_false_valid_fault_site_v1(&C::StepArithmetic { refutation, operand_openings: Vec::new() }, ladder),
                Ok((PalwFaultSiteV1::Leaf { first_read, last_read, .. }, _))
                    if segment(first_read) == segment(leaf) && segment(last_read) == segment(leaf)
            )
    };
    let leaf = (n / 2..n)
        .find(|leaf| {
            held.contains(leaf)
                && kaspa_consensus_core::palw_step::canonical_step_coordinates(&hb.shape_profile, &hb.job_context, *leaf).is_some()
                && decides_alone(*leaf)
        })
        .expect("the capture holds an openable step leaf a segment decides alone");
    let lying = h.backend.execute_free_prompt_with_injected_fault(&job, &prompt, leaf).expect("the drill's FP fault runs");
    let refutation = h.backend.refutation_for_free_prompt_index(&lying.outcome.material, leaf, &ids).expect("opens");
    let operand_openings = h.backend.operand_openings_for(&refutation).expect("the class opens the rows");
    assert!(refutation.output_preimage.coord.call_index > 0 && refutation.decode_tokens.is_some(), "a decode step the prover pinned");
    let pinned = C::StepArithmetic { refutation: refutation.clone(), operand_openings: operand_openings.clone() };
    let mut unpinned = refutation;
    unpinned.decode_tokens = None;
    let c = C::StepArithmetic { refutation: unpinned, operand_openings };
    let context = h.backend.capture_shape(&lying.outcome.material).expect("the capture has a shape").job_context;
    let commitment = palw_fp_commitment_from_context_v3(&job, &context, &lying, walk.daa + facts.min_trace_retention_daa)
        .expect("the run becomes a commitment");
    let signature = sign(EXECUTOR, fp_claim_id_v3(&commitment).as_byte_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT);
    let payload = PalwFpCommitmentTxPayloadV3 {
        version: PALW_FP_V3_VERSION,
        commitment: commitment.clone(),
        prompt_token_ids: ids.clone(),
        signature,
    };
    let tx = kaspa_consensus_core::tx::Transaction::new(
        0,
        vec![],
        vec![],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT.clone(),
        0,
        borsh::to_vec(&payload).unwrap(),
    );
    let point = walk.next();
    let extraction = h.vp().palw_v2_fp_objects_of_txs_for_tests(std::slice::from_ref(&tx), &walk.state, point.daa_score);
    assert!(extraction.skipped.is_empty(), "the processor's walk takes the carrier: {:?}", extraction.skipped);
    let objects: Vec<Obj> = extraction.objects.into_iter().map(|carried| carried.object).collect();
    assert_eq!(objects.len(), 1, "one commitment, one object");
    h.carry(&mut walk, objects);
    let id = fp_claim_id_v3(&commitment);
    let opened = walk.state.claim(&id).expect("the commitment opened a claim").clone();
    assert!(matches!(opened.source, PalwClaimSourceV2::FreePrompt { .. }) && matches!(opened.phase, PalwClaimPhaseV2::Provisional));
    assert_eq!(opened.execution_root, commitment.execution_root);
    // The FP lane's own fixed point, and why F2 cannot keep the equality.
    assert_eq!(context.job_id, fp_job_id_v3(&job), "the binding's job id is the job's");
    assert_ne!(context.job_id, id, "and not the claim id");
    h.convicts(&c, opened.execution_root, ladder).expect("the proof pins the claim's root");
    // F1 (J-1, T18h): the claim records the commitment's job pin — the identity its binding's own
    // context reproduces, so the claim's own execution answers its own job.
    let pin = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&commitment);
    assert_eq!(opened.job_identity, pin, "a free-prompt claim records its commitment's pin");
    assert_eq!(kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&context), pin, "one spelling");
    let (own_binding, ..) = base0_material_decode_v1(&lying.outcome.material).expect("decodes");
    let target = palw_offence_target_v1(&walk.state, &id).expect("the claim");
    assert_eq!(target.lane, Some(kaspa_consensus_core::palw_offence_attribution_v1::PalwClaimSourceKindV1::FreePrompt));
    assert_eq!(palw_binding_identity_fault_v1(&target, &own_binding, h.rules(), true), Ok(None), "its own job: no identity fault");
    h.refused(&walk, &h.refuted(id, C::IdentityMismatch { binding: own_binding }), &E::IdentityHolds.to_string());

    h.bind(&mut walk, id);
    let licence = h.license_v2(&mut walk, id);
    let (binding, ..) = base0_material_decode_v1(&lying.outcome.material).expect("decodes");
    let segment = palw_segment_index_of_leaf_v2(binding.step_leaf_count, licence.assignment.segments, leaf).expect("in the cut");
    let full = licence.full_card();
    let holder = licence.holder_of(segment);
    eprintln!("[t46g] leaf {leaf} of {} is in segment {segment}", binding.step_leaf_count);
    h.convicts(&pinned, opened.execution_root, ladder).expect("the pinned proof convicts too");
    let site_of = |proof: &C| {
        let payload = borsh::to_vec(&h.v2_payload(full, id, licence.segmented(full), proof.clone())).unwrap();
        palw_check_panel_false_valid_v2(&walk.state, &h.cards[full], &payload, false, false, h.rules(), None)
            .expect("the full seat is liable")
            .site
    };
    assert_eq!(site_of(&pinned), PalwFaultSiteV1::Whole, "a decode step carrying the generated tokens is the whole's");
    h.refused(&walk, &h.v2(holder, id, licence.segmented(holder), pinned), &E::SiteNotAttested.to_string());
    let PalwFaultSiteV1::Leaf { first_read, last_read, .. } = site_of(&c) else { panic!("a step site without the pin") };
    eprintln!("[t46g] without the pin the step read leaves {first_read}..={last_read}");
    for other in licence.partials().into_iter().filter(|card| *card != holder) {
        h.refused(&walk, &h.v2(other, id, licence.segmented(other), c.clone()), &E::SiteNotAttested.to_string());
    }
    let (licensed, _) =
        h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c.clone()), h.v2(holder, id, licence.segmented(holder), c)]);
    let s = &walk.state;
    let daa = walk.daa;
    for card in [full, holder] {
        let seat = h.cards[card];
        let lock = *licensed.slashable_lock(seat, id).expect("locked at the licence");
        assert!(s.slashable_lock(seat, id).is_none());
        assert_eq!(s.bond(&seat).unwrap().collateral as u128, licensed.bond(&seat).unwrap().collateral as u128 - lock.amount);
        let row = s.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).expect("one kind-3 row");
        assert_eq!(
            (row.kind, row.amount as u128, row.execution_root),
            (PalwOffenceKindV1::PanelFalseValidV2, lock.amount, opened.execution_root)
        );
    }
    assert!(
        matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == daa)
    );
    let claim_row = licensed.claim(&id).unwrap();
    let executor = h.cards[EXECUTOR];
    assert_eq!(
        s.bond(&executor).unwrap().collateral as u128,
        licensed.bond(&executor).unwrap().collateral as u128
            - (claim_row.reserved
                + h.sp().claim_escrow_reservation_v1(claim_row.accepted_daa, claim_row.escrowed_reward)
                + claim_row.rights_reserved),
        "the FP executor pays its reservation and its receipt rights"
    );
    assert_eq!(s.panel_liability(&id).unwrap().void_reason, Some(PalwVoidReasonV2::CourtFraud));
}

// ---------------------------------------------------------------------------------------------
// T46h–k — what the gate refuses
// ---------------------------------------------------------------------------------------------

/// **T46h: past the fence the V1 kind is refused by name.** The forced object — a contradiction
/// whose job context names the CLAIM id, the one shape the V1 rule convicts on
/// (`review_economic_forged_false_valid.rs:78`) — is `SupersededOnThisNetwork` at the gate and in the
/// fold, whether it carries an equivocation certificate or a forged output over the forced binding
/// (which the V1 stateless verifier still passes). The same forced binding in V2 is pinned to the
/// claim's root and refused `PanelFalseValidWorkMismatch`.
#[tokio::test]
async fn t46h_v1_kind_refused_past_fence() {
    use kaspa_consensus_core::palw_offence_v1::palw_verify_objective_offence_v1;
    let h = harness(true);
    let (walk, claim, licence) = h.licensed(Fault::Forged);
    let id = claim.claim_id;
    let full = licence.full_card();
    let C::ForgedOutput { binding, pin, position } = claim.contradiction() else { unreachable!() };
    // The forced binding: the claim's own execution relabelled job_id := claim_id, its logits trace
    // re-rooted under that context, and re-committed — a well-formed binding the V1 rule accepts.
    let mut forced = binding.clone();
    forced.job_context.job_id = id;
    forced.full_logits_trace_root = kaspa_consensus_core::palw_step_refute::base0_logits_trace_root_v1(
        &forced.job_context,
        &pin.logits_rows,
        &pin.generated_token_ids,
    );
    rebind(&mut forced);
    verify_binding_v1(&forced).expect("the forced binding is well-formed");
    assert_ne!(forced.committed_execution_root, claim.envelope.attempt.execution_root, "a relabelled job is another root");
    let forced_forged = C::ForgedOutput { binding: forced.clone(), pin, position };
    let v2_receipt = h.full_receipt(full, id, h.domain, licence.licensed_daa);
    let v1_forged = h.v1(full, id, v2_receipt.clone(), forced_forged.clone());
    let Obj::ObjectiveOffence { evidence, evidence_id, .. } = &v1_forged else { unreachable!() };
    palw_verify_objective_offence_v1(
        PalwOffenceKindV1::PanelFalseValid,
        &h.cards[full].0,
        evidence_id,
        evidence,
        &card_pubkey(full),
        true,
        h.domain.as_byte_slice(),
        h.bundle.court.max_step_leaf_count(),
        |pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]| {
            kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, ctx).unwrap_or(false)
        },
    )
    .expect("the V1 stateless verifier passes the forced-id object");
    // The equivocation certificate shaped as the review's: its job context names the claim id.
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&card_pubkey(EXECUTOR)),
        job_context_hash: forced.job_context.context_hash(),
        full_logits_trace_root: Hash64::from_u64_word(root),
        committed_root: Hash64::from_u64_word(root),
        bond_outpoint: h.cards[EXECUTOR].0,
        signature: Vec::new(),
    };
    let equivocation = C::ExecutorEquivocation(kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: h.cards[EXECUTOR].0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context: forced.job_context.clone(),
            attestation_a: attestation(0xAA),
            attestation_b: attestation(0xBB),
        },
    });
    for v1 in [v1_forged, h.v1(full, id, v2_receipt, equivocation)] {
        h.refused(&walk, &v1, &E::SupersededOnThisNetwork.to_string());
        assert!(h.fold_refusal(&walk, &v1).contains("superseded"), "the fold refuses the V1 kind by name too");
    }
    // The same forced binding in V2: pinned to the claim's root.
    for forced_contradiction in
        [forced_forged, C::StepStructural(PalwStepRefutationV1 { binding: forced, evidence: PalwStepEvidenceV1::Shape })]
    {
        let object = h.v2(full, id, licence.segmented(full), forced_contradiction);
        h.refused(&walk, &object, &E::PanelFalseValidWorkMismatch.to_string());
        assert_eq!(h.fold_refusal(&walk, &object), E::PanelFalseValidWorkMismatch.to_string());
    }
}

/// **T46i: an honest run convicts nobody.** Its decoded token at position 0 is the pinned rule's,
/// and its step at every leaf recomputes: `PanelFalseValidNeedsContradiction` for each — at the gate
/// for position 0 and three leaves, and at the adjudicator both doors call for every leaf of the
/// step space. The refutations are the prover's (`base0_refutation_from_capture_capped_v1` over the
/// capture, and the rows `operand_openings_for` records), assembled from one decode of the capture
/// and one inventory instead of per leaf — and asserted equal to the backend's own verbs at the
/// leaves the gate is asked about.
#[tokio::test]
async fn t46i_honest_run_no_fault() {
    use kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1;
    use kaspa_consensus_core::palw_step_refute::{
        PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1, check_execution_step_refutation_carried_capped_v1,
    };
    use misaka_palw_base0::produce::base0_material_decode_v1;
    let h = harness(true);
    let (walk, claim, licence) = h.licensed(Fault::Honest);
    let id = claim.claim_id;
    let full = licence.full_card();
    let needs = E::PanelFalseValidNeedsContradiction.to_string();
    let (binding, tiles, rows, toks, _) = base0_material_decode_v1(&claim.material).expect("decodes");
    let pin = PalwBase0DecodeTokensV1 { logits_rows: rows, generated_token_ids: toks };
    h.refused(
        &walk,
        &h.v2(full, id, licence.segmented(full), C::ForgedOutput { binding: binding.clone(), pin: pin.clone(), position: 0 }),
        &needs,
    );

    // The prover's inputs, once: the capture's leaves by position, the prompt the anchor implies,
    // the decode pin, and the class's inventory.
    let resolved = misaka_palw_base0::classes::resolve_class_v1(&h.bundle.court, h.floor(), h.artifact_root, &[]).expect("resolves");
    let inventory =
        misaka_palw_base0::inventory::base0_inventory_v1(&resolved.artifact, resolved.inventory_geometry).expect("inventory");
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    for (index, tile) in &tiles {
        leaves[*index as usize] = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, tile);
    }
    let capture = misaka_palw_base0::legs::Base0StepTilesV1 { leaves, tiles: tiles.clone() };
    let prompt_ids: Vec<u32> = claim.prompt.iter().map(|t| *t as u32).collect();
    let ladder_cap = h.bundle.court.max_step_leaf_count();
    let prove = |leaf: u64| {
        let coord = kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf)?;
        let refutation = misaka_palw_base0::legs::base0_refutation_from_capture_capped_v1(
            &binding.shape_profile,
            &binding.job_context,
            &capture,
            binding.clone(),
            coord,
            prompt_ids.clone(),
            Some(PalwDecodeTokenPinV1::Base0V1(pin.clone())),
            None,
            ladder_cap,
        )
        .expect("the honest capture opens");
        let recorder = PalwRecordingOracleV1::new(inventory.operands());
        let _ = check_execution_step_refutation_carried_capped_v1(
            &refutation,
            &recorder,
            h.backend.prompt_ids_form(),
            h.backend.step_ladder_cap(),
        );
        Some((refutation, recorder.openings().expect("the inventory opens what it resolved")))
    };
    let n = binding.step_leaf_count;
    let main: Vec<u64> = (0..n)
        .filter(|leaf| {
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, *leaf).is_some()
        })
        .collect();
    assert_eq!(main.len() as u64, n, "every leaf of the floor's one-forward job is a step coordinate (no KV aux leaves)");
    let at_gate: BTreeSet<u64> = [0, n / 2, n - 1].into_iter().collect();
    for leaf in 0..n {
        let (refutation, operand_openings) = prove(leaf).expect("a step coordinate");
        if at_gate.contains(&leaf) {
            let by_backend = h.backend.refutation_for_index(&claim.material, leaf).expect("the backend opens it");
            assert_eq!(refutation, by_backend, "leaf {leaf}: the same refutation the backend's verb builds");
            assert_eq!(operand_openings, h.backend.operand_openings_for(&by_backend).expect("rows"), "leaf {leaf}: the same rows");
        }
        let payload = h.v2_payload(full, id, licence.segmented(full), C::StepArithmetic { refutation, operand_openings });
        let bytes = borsh::to_vec(&payload).unwrap();
        assert_eq!(
            palw_check_panel_false_valid_v2(&walk.state, &h.cards[full], &bytes, false, false, h.rules(), None),
            Err(E::PanelFalseValidNeedsContradiction),
            "leaf {leaf}"
        );
        if at_gate.contains(&leaf) {
            h.refused(&walk, &offence(PalwOffenceKindV1::PanelFalseValidV2, h.cards[full], bytes), &needs);
        }
    }
    eprintln!("[t46i] {n} leaves, every one NeedsContradiction");
}

/// **T46j: a receipt signed for another network verifies against nothing here.** Signed under
/// testnet-11's domain, the full and the segmented form are both `PanelFalseValidReceiptUnverified`.
#[tokio::test]
async fn t46j_wrong_domain() {
    use kaspa_consensus_core::network::{NetworkId, NetworkType};
    let h = harness(true);
    let (walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let t11 = kaspa_consensus_core::config::params::Params::from(NetworkId::with_suffix(NetworkType::Testnet, 11));
    let t11_domain =
        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(t11.net.to_string().as_bytes(), Some(t11.genesis.hash));
    assert_ne!(t11_domain, h.domain);
    let full = licence.full_card();
    let foreign_full = h.full_receipt(full, id, t11_domain, licence.licensed_daa);
    let foreign_v3 = h.v3_receipt(full, id, t11_domain, licence.licensed_daa, licence.receipt(full).segments);
    for receipt in [PalwFalseValidReceiptV1::Full(foreign_full), PalwFalseValidReceiptV1::Segmented(foreign_v3)] {
        h.refused(&walk, &h.v2(full, id, receipt, c.clone()), &E::PanelFalseValidReceiptUnverified.to_string());
    }
}

/// **T46k: the contradictions F2 refuses by name** — `Legs`, `ExecutorEquivocation`,
/// `CourtExecutorGuilty` and `ConflictingPermit` — are each `ContradictionNotAdmitted`, on the real
/// claim with the seat's real receipt.
#[tokio::test]
async fn t46k_refused_kinds() {
    let h = harness(true);
    let (walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let full = licence.full_card();
    let b = &claim.binding;
    let legs = C::Legs(kaspa_consensus_core::palw_legs::PalwLegsRefutationV1 {
        binding: kaspa_consensus_core::palw_legs::PalwLegsBindingV1 {
            version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
            job_context: b.job_context.clone(),
            tap_profile: kaspa_consensus_core::palw_legs::PalwActivationTapProfileV1 {
                version: kaspa_consensus_core::palw_legs::PALW_LEGS_OBJECT_VERSION_V1,
                tap_semantics_id: Hash64::from_u64_word(0x41),
                tap_layer_indices: vec![0],
                model_total_layers: b.shape_profile.layer_count,
                hidden_dim: b.shape_profile.hidden_dim,
                dtype: kaspa_consensus_core::palw_v2::PalwLogitsDtypeV2::F32Le,
            },
            checkpoint_profile: b.checkpoint_profile.clone(),
            full_logits_trace_root: b.full_logits_trace_root,
            activation_leaf_count: 0,
            activation_merkle_root: Hash64::default(),
            checkpoint_count: b.checkpoint_count,
            checkpoint_merkle_root: b.checkpoint_merkle_root,
            committed_execution_root: b.committed_execution_root,
        },
        evidence: kaspa_consensus_core::palw_legs::PalwLegsEvidenceV1::Shape,
    });
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&card_pubkey(EXECUTOR)),
        job_context_hash: claim.job.context_hash(),
        full_logits_trace_root: Hash64::from_u64_word(root),
        committed_root: Hash64::from_u64_word(root),
        bond_outpoint: h.cards[EXECUTOR].0,
        signature: Vec::new(),
    };
    let equivocation = C::ExecutorEquivocation(kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: h.cards[EXECUTOR].0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context: claim.job.clone(),
            attestation_a: attestation(0xAA),
            attestation_b: attestation(0xBB),
        },
    });
    let refused = [
        legs,
        equivocation,
        C::CourtExecutorGuilty { offence_id: Hash64::from_u64_word(0x0FF) },
        C::ConflictingPermit { span: 1, round: 2, permit_index: 0 },
    ];
    for contradiction in refused {
        let why = palw_false_valid_admission_v1(&contradiction).expect_err("refused by name");
        assert!(matches!(why, E::ContradictionNotAdmitted(_)));
        let object = h.v2(full, id, licence.segmented(full), contradiction);
        h.refused(&walk, &object, &why.to_string());
        assert_eq!(h.fold_refusal(&walk, &object), why.to_string(), "the fold refuses it by the same name");
    }
}

// ---------------------------------------------------------------------------------------------
// T46l–n — once, reversibly, and never under a session
// ---------------------------------------------------------------------------------------------

/// **T46l: one offence per (seat, claim).** Once the full seat is convicted, a second proof of the
/// same false `Valid` — the `CourtFraud` void the conviction wrote, or the same proof again — is
/// refused at the gate and folds as a no-op to the empty block's root. A filled reporter slot is
/// refused `ReporterSlotNotArmed` (F7 is not armed) at the gate and in the fold, the walk drops it
/// and the block's root is the empty block's; the slot is outside the ledger key, which the
/// adjudicator shows when the slot is armed. The partial holder of the leaf is a separate offence.
#[tokio::test]
async fn t46l_one_offence_per_seat_claim() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let full = licence.full_card();
    let first = h.v2(full, id, licence.segmented(full), c.clone());
    let (licensed, _) = h.carry(&mut walk, vec![first.clone()]);
    let voided_daa = walk.daa;
    let offence_id = palw_false_valid_offence_id_v2(&h.cards[full].0, &id);
    assert!(walk.state.consumed_offence(&offence_id).is_some());

    let point = walk.next();
    let empty = h.fold(&walk.state, &point, &[]).expect("an empty block folds").state_root();
    let court_fraud = h.v2(full, id, licence.segmented(full), C::CourtFraud { voided_daa });
    for again in [&court_fraud, &first] {
        let why = h.validate(&walk.state, &point, again).expect_err("the gate refuses a second proof of one offence");
        assert!(why.contains("already convicted"), "{why}");
        assert!(h.accepted(&walk.state, &point, std::slice::from_ref(again)).is_empty());
        let folded = h.fold(&walk.state, &point, std::slice::from_ref(again)).expect("the fold carries it as a no-op");
        assert_eq!(folded.state_root(), empty, "a second proof is a no-op with the empty block's root");
    }

    // The reporter slot: other bytes, the same (seat, claim).
    let mut revealed = h.v2_payload(full, id, licence.segmented(full), c.clone());
    revealed.reporter_reveal = vec![0xF7; 32];
    let revealed_bytes = borsh::to_vec(&revealed).unwrap();
    let revealed_object = offence(PalwOffenceKindV1::PanelFalseValidV2, h.cards[full], revealed_bytes.clone());
    h.refused(&walk, &revealed_object, &E::ReporterSlotNotArmed.to_string());
    assert_eq!(h.fold_refusal(&walk, &revealed_object), E::ReporterSlotNotArmed.to_string());
    let carried = h.accepted(&walk.state, &point, std::slice::from_ref(&revealed_object));
    assert_eq!(
        h.fold(&walk.state, &point, &carried).expect("folds").state_root(),
        empty,
        "the block that carried it is the empty block"
    );
    // Armed, the slot changes nothing the adjudicator finds, so nothing the ledger keys on.
    let plain_bytes = borsh::to_vec(&h.v2_payload(full, id, licence.segmented(full), c.clone())).unwrap();
    let plain =
        palw_check_panel_false_valid_v2(&licensed, &h.cards[full], &plain_bytes, false, true, h.rules(), None).expect("convicts");
    let armed =
        palw_check_panel_false_valid_v2(&licensed, &h.cards[full], &revealed_bytes, false, true, h.rules(), None).expect("convicts");
    assert_eq!(armed, plain, "the reveal is outside the finding");
    assert_eq!(palw_false_valid_offence_id_v2(&h.cards[full].0, &armed.target.claim_id), offence_id, "and outside the ledger key");

    // Another seat's false Valid on the same claim is its own offence.
    let leaf = claim.fault_leaf.unwrap();
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).unwrap();
    let holder = licence.holder_of(segment);
    h.carry(&mut walk, vec![h.v2(holder, id, licence.segmented(holder), c)]);
    let holder_id = palw_false_valid_offence_id_v2(&h.cards[holder].0, &id);
    assert_ne!(holder_id, offence_id);
    assert!(walk.state.consumed_offence(&holder_id).is_some(), "the partial holder is convicted on its own key");
}

/// **T46m: a conviction reverts exactly, re-applies identically, and survives a restart.** The
/// delta the fold wrote for the two convictions of T46b is reverted by `revert_delta_v2` to the
/// licensed state's root (locks, bonds, phase, ledger); the same block re-applied to the reverted
/// state writes the same root; and the convicted state round-trips through the loader's carriage
/// check (`dos_g2`'s `reloads`) and through this node's own tip row.
#[tokio::test]
async fn t46m_reorg_and_restart() {
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let leaf = claim.fault_leaf.unwrap();
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).unwrap();
    let (full, holder) = (licence.full_card(), licence.holder_of(segment));
    let objects = vec![h.v2(full, id, licence.segmented(full), c.clone()), h.v2(holder, id, licence.segmented(holder), c)];
    let point = walk.next();
    let (licensed, delta) = h.carry(&mut walk, objects.clone());
    let convicted = walk.state.clone();
    assert_convicted_before_final(&h, &licensed, &convicted, id, &[full, holder], point.daa_score);

    let reverted = revert_delta_v2(&convicted, &delta, h.sp()).expect("the conviction's delta reverts");
    assert_eq!(reverted.state_root(), licensed.state_root(), "revert restores the licensed root exactly");
    assert!(matches!(reverted.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    for card in [full, holder] {
        let seat = h.cards[card];
        assert_eq!(reverted.slashable_lock(seat, id), licensed.slashable_lock(seat, id), "card {card}'s lock is back");
        assert_eq!(reverted.bond(&seat).unwrap().collateral, licensed.bond(&seat).unwrap().collateral);
        assert!(reverted.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).is_none(), "and its ledger row is gone");
    }
    let again = h.fold(&reverted, &point, &objects).expect("the same block re-applies");
    assert_eq!(again.state_root(), convicted.state_root(), "re-applying gives the same root");

    h.reloads(&convicted);
    h.restarts(point.block, &convicted);
}

/// **T46n (restated by ADR-0152 v3.1 post-edit 2): only a COURT defers a conviction.** An open
/// court session on the claim makes the full seat's conviction — kind 3 — and the executor's — kind 4
/// — `ClaimUnderSession`: refused at the gate and in the fold, dropped by the walk, nothing written.
/// A data-availability session does NOT: a held one opened by a bonded bystander through the real gate
/// (T18y's first half) and a non-held one (a real `DefaultAccused`) each let the conviction land —
/// the claim voids `CourtFraud`, the session's held rows go with it, the accuser's reservation comes
/// back uncharged, and the state passes the loader's consistency check. (F2 review F-4 had deferred
/// here, reading a void mid-session as unhandled; the claim write drops the held rows and releases
/// the accuser at every exit from `DefaultDisputed`, which this pins, and deferring gave a filer a
/// session to open for delay.)
#[tokio::test]
async fn t46n_session_open() {
    use kaspa_consensus_core::palw_held_da_v1::{
        PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT, PALW_HELD_DA_VERSION_V1, PalwHeldAccusationV1, PalwHeldMissingV1,
        palw_held_da_accusation_message_v1,
    };
    // ADR-0152 R6: the v1 DA court's sessions (`DefaultDisputed`) stand below `palw_rcore_plus`; this
    // is their fence-off twin. The R-core twin is `t46n_r_an_rcore_da_session_does_not_defer_a_conviction`.
    let h = harness_rcore_off();
    let (walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let full = licence.full_card();
    let kind3 = h.v2(full, id, licence.segmented(full), c.clone());
    let kind4 = h.refuted(id, c.clone());
    let under_session = E::ClaimUnderSession.to_string();
    let untouched = |before: &PalwChainStateV2, after: &PalwChainStateV2| {
        let seat = h.cards[full];
        assert_eq!(after.slashable_lock(seat, id), before.slashable_lock(seat, id));
        assert_eq!(after.bond(&seat).unwrap().collateral, before.bond(&seat).unwrap().collateral);
        assert!(after.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).is_none());
        assert!(after.consumed_offence(&palw_executor_refuted_offence_id_v1(&h.cards[EXECUTOR].0, &id)).is_none());
    };
    // A conviction landing mid-session: the claim voids, the session and its rows go, the accuser is
    // released uncharged, and the state reloads.
    let lands = |mut session_walk: Walk, object: &Obj, accuser: PalwBondKeyV2| {
        let accuser_collateral = session_walk.state.bond(&accuser).unwrap().collateral;
        let held_by_accuser = session_walk.state.reserved_exposure(&accuser);
        h.carry(&mut session_walk, vec![object.clone()]);
        let s = &session_walk.state;
        assert!(
            matches!(s.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
            "the conviction lands mid-session: {:?}",
            s.claim(&id).unwrap().phase
        );
        assert!(s.held_da_missing_of(&id).is_none(), "the session's held unit goes with the claim");
        assert_eq!(s.bond(&accuser).unwrap().collateral, accuser_collateral, "the accuser pays nothing");
        assert!(s.reserved_exposure(&accuser) < held_by_accuser, "and its reservation comes back");
        h.reloads(s);
    };

    // 1. A held DA session: a bonded bystander demands one committed step leaf.
    let mut held_walk = walk.clone();
    let mut accusation = PalwHeldAccusationV1 {
        version: PALW_HELD_DA_VERSION_V1,
        claim: id,
        missing: PalwHeldMissingV1::StepRange { first: 0, count: 1 },
        accuser: h.cards[BYSTANDER],
        binding: claim.binding.clone(),
        signature: Vec::new(),
    };
    accusation.signature = sign(
        BYSTANDER,
        palw_held_da_accusation_message_v1(h.domain.as_byte_slice(), &accusation).as_byte_slice(),
        PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
    );
    h.carry(&mut held_walk, vec![Obj::DefaultAccusedHeld { accusation: Box::new(accusation) }]);
    assert!(matches!(held_walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "the session is open");
    assert!(held_walk.state.held_da_missing_of(&id).is_some());
    lands(held_walk.clone(), &kind3, h.cards[BYSTANDER]);
    lands(held_walk, &kind4, h.cards[BYSTANDER]);

    // 2. An open court session on the claim (testnet-12's held regime opens none through a
    //    `CourtOpened`, so the session row is written through the carriage with the challenger's
    //    stake where the loader demands it — nowhere past `palw_rcore_plus`, where A-6 keeps it off
    //    the claim ledger): both kinds wait.
    let mut court_walk = walk.clone();
    let at = court_walk.daa;
    let reserved = court_walk.state.claim(&id).unwrap().reserved;
    let challenger = h.cards[BYSTANDER];
    let executor = h.cards[EXECUTOR];
    let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
        &id,
        &claim.envelope.attempt.trace_root,
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&challenger),
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&executor),
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        h.bundle.court.max_step_leaf_count(),
        at,
        at + 50,
    )
    .expect("a ladder opens");
    let session_id = ladder.session_id();
    court_walk.state = h.rebuilt(&court_walk.state, |carriage| {
        carriage.court_sessions.insert(
            session_id,
            kaspa_consensus_core::palw_state_v2::PalwCourtSessionStateV2 {
                claim: id,
                challenger_bond: challenger,
                opened_daa: at,
                deadline_daa: at + h.sp().window_court(),
                ladder,
                dissection: None,
            },
        );
        // The challenger's stake sits in `reserved_exposure` below ADR-0152's `palw_rcore_plus`;
        // past it (A-6) the open session is the stake, read by the accuser ledger.
        if h.sp().rcore_plus_from_daa().is_none() {
            *carriage.reserved_exposure.entry(challenger).or_insert(0) += reserved;
        }
    });
    assert_eq!(court_walk.state.open_courts_of(&id), 1, "the court is open");
    for object in [&kind3, &kind4] {
        h.refused(&court_walk, object, &under_session);
        assert_eq!(h.fold_refusal(&court_walk, object), under_session);
        let point = court_walk.next();
        let carried = h.accepted(&court_walk.state, &point, std::slice::from_ref(object));
        let block = h.fold(&court_walk.state, &point, &carried).expect("folds");
        assert_eq!(block.state_root(), h.fold(&court_walk.state, &point, &[]).unwrap().state_root(), "nothing is written");
        untouched(&court_walk.state, &block);
    }

    // 3. A non-held data-availability session: a bonded bystander accuses one trace event.
    let mut da_walk = walk.clone();
    let accuser = h.cards[BYSTANDER];
    let message = kaspa_consensus_core::palw_state_v2::palw_da_accusation_message_v2(h.domain, &id, 0, &accuser);
    let accused = Obj::DefaultAccused {
        claim: id,
        missing_event_index: 0,
        accuser,
        signature: sign(
            BYSTANDER,
            message.as_byte_slice(),
            kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT,
        ),
    };
    h.carry(&mut da_walk, vec![accused]);
    assert!(matches!(da_walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "the session is open");
    assert!(
        da_walk.state.held_da_missing_of(&id).is_none() && da_walk.state.open_courts_of(&id) == 0,
        "and it is neither held nor a court"
    );
    lands(da_walk.clone(), &kind3, accuser);
    lands(da_walk, &kind4, accuser);
}

/// **The carriage, pinned on the step it matters most for: the embedding gather of prompt position
/// 0** (found by this suite; made ONE route by the F2 review, F-3). testnet-12 commits its prompts
/// in the Merkle form. The drill's lie at the first leaf is recomputed from the prompt id it read, so
/// the refutation must carry the prompt. The whole list the prover builds is refused against a
/// Merkle commitment — by the V1 route's flat comparison, and by the adjudicator with no opening,
/// which IS that route; a list stripped without its opening cannot adjudicate a gather at all; the
/// list and an opening together are two answers and refused; and the pair
/// `palw_refutation_prompt_carriage_v1` builds — no list, the one tile opened against the Merkle root,
/// carried in the evidence's `prompt_ids_opening` — convicts, in evidence that grows with a path and
/// never with the prompt. Through the gate, the walk and the fold the whole-list evidence is refused
/// and the carried one convicts the full seat and the holder of the leaf's segment (a gather reads
/// no other leaf).
#[tokio::test]
async fn t46o_a_prompt_gather_fault_convicts_in_the_networks_carriage() {
    use kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1;
    let h = harness(true);
    assert_eq!(
        h.form(),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        "testnet-12 commits Merkle prompt ids"
    );
    let (mut walk, claim, licence) = h.licensed(Fault::StepGather);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let leaf = claim.fault_leaf.unwrap();
    let coord =
        kaspa_consensus_core::palw_step::canonical_step_coordinates(&claim.binding.shape_profile, &claim.binding.job_context, leaf)
            .unwrap();
    assert_eq!(
        (coord.call_index, coord.position, coord.node_slot),
        (0, 0, 0),
        "the fault is the embedding gather of prompt position 0"
    );
    let root = claim.envelope.attempt.execution_root;
    let ladder = h.ladder(&walk.state);
    let needs = E::PanelFalseValidNeedsContradiction;
    assert_eq!(
        palw_panel_contradiction_convicts_execution_v1(&c, root, h.artifact_root, ladder),
        Err(needs.clone()),
        "the V1 route's flat comparison refuses the prover's refutation on a Merkle network"
    );
    assert_eq!(
        palw_false_valid_convicts_execution_v2(&c, None, root, h.artifact_root, ladder),
        Err(needs.clone()),
        "with no opening the adjudicator is that route: a whole list against a Merkle root"
    );
    let C::StepArithmetic { refutation, operand_openings } = c.clone() else { unreachable!() };
    let (carried, opening) = palw_refutation_prompt_carriage_v1(h.form(), refutation.clone()).expect("the prover's list is the job's");
    let opening = opening.expect("a prefill gather's tile is opened");
    assert!(carried.prompt_token_ids.is_empty(), "the carriage takes the list out");
    let stripped = C::StepArithmetic { refutation: carried, operand_openings };
    assert_eq!(
        palw_false_valid_convicts_execution_v2(&stripped, None, root, h.artifact_root, ladder),
        Err(needs.clone()),
        "a gather cannot be recomputed without the id it read"
    );
    assert_eq!(
        palw_false_valid_convicts_execution_v2(&c, Some(&opening), root, h.artifact_root, ladder),
        Err(needs.clone()),
        "the list and an opening at once are two answers"
    );
    palw_false_valid_convicts_execution_v2(&stripped, Some(&opening), root, h.artifact_root, ladder)
        .expect("read in the job's carriage, it convicts");
    let full = licence.full_card();
    let whole_list = borsh::to_vec(&h.v2_payload_raw(full, id, licence.segmented(full), c.clone(), None)).unwrap();
    let in_carriage = borsh::to_vec(&h.v2_payload(full, id, licence.segmented(full), c.clone())).unwrap();
    eprintln!("[t46o] evidence: {} bytes with the whole list, {} in the carriage", whole_list.len(), in_carriage.len());
    h.refused(&walk, &offence(PalwOffenceKindV1::PanelFalseValidV2, h.cards[full], whole_list), &needs.to_string());
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, leaf).unwrap();
    let holder = licence.holder_of(segment);
    let (licensed, _) =
        h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), c.clone()), h.v2(holder, id, licence.segmented(holder), c)]);
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full, holder], walk.daa);
}

// ---------------------------------------------------------------------------------------------
// T46p–r — the F2 review
// ---------------------------------------------------------------------------------------------

/// **T46p: a lie in one segment convicts no honest partial seat downstream** (F2 review, F-1 — the
/// reviewer's probe, on a real producer-built claim). The drill corrupts one lane of the first
/// K-cache write of segment 1 — leaf 1432, the key row of prompt position 2, unless the drill's move
/// would push that lane out of int8 (then the next key row: [`Fault::KCacheWrite`]) — and re-commits,
/// every later tile as the honest run computed it. A later position's attention scores read that row, so
/// where the query lane does not cancel the change the step recomputes, from the committed (wrong)
/// key, to something other than its committed (honest) value and "convicts" (the probe's job: leaves
/// 2820 and 3512 in segment 2, 4204 and 4896 in segment 3). Their holders replayed their segments and
/// matched; the adjudicator reads that each such step also read leaves of earlier segments and
/// refuses every partial seat `SiteNotAttested`, while the full seat, which attested the whole, is
/// convicted by any of them. The step at 1432 itself reads only leaves of segment 1: its holder is
/// convicted with the full seat, and the other holders are not. And T46b's first leaf, `n / 2` =
/// 2780 — the first leaf of segment 2, whose step reads the last four leaves of segment 1 — convicts
/// the full seat and neither holder.
#[tokio::test]
async fn t46p_a_lie_in_one_segment_convicts_no_honest_partial_downstream() {
    let h = harness(true);
    // The premise, found rather than assumed: one lane of a key row moves a later score only where
    // that position's query lane does not cancel it, which depends on the prompt — so the template
    // nonce (another anchor, another job) is stepped until the lie is read, convictingly, in both
    // later segments. What is asserted of those readers is asserted of every one found.
    let mut found = None;
    for nonce in 0..8u64 {
        let (walk, claim, licence) = h.licensed_at_nonce(Fault::KCacheWrite { segment: 1 }, nonce);
        let (n, k) = (claim.binding.step_leaf_count, licence.assignment.segments);
        let readers = convicting_readers(&h, &claim, claim.fault_leaf.unwrap(), h.ladder(&walk.state));
        let reached: BTreeSet<u16> = readers.iter().filter_map(|(leaf, _)| palw_segment_index_of_leaf_v2(n, k, *leaf)).collect();
        eprintln!("[t46p] nonce {nonce}: convicting readers {:?}", readers.iter().map(|(leaf, _)| *leaf).collect::<Vec<_>>());
        if reached.contains(&2) && reached.contains(&3) {
            found = Some((walk, claim, licence, readers));
            break;
        }
    }
    let (mut walk, claim, licence, readers) = found.expect("a job whose one-lane lie is read in segments 2 and 3");
    let id = claim.claim_id;
    let b = claim.binding.clone();
    let n = b.step_leaf_count;
    let k = licence.assignment.segments;
    let seg = |leaf: u64| palw_segment_index_of_leaf_v2(n, k, leaf).expect("in the cut");
    let coord = |leaf: u64| {
        kaspa_consensus_core::palw_step::canonical_step_coordinates(&b.shape_profile, &b.job_context, leaf).expect("a step")
    };
    let lie = claim.fault_leaf.unwrap();
    eprintln!("[t46p] the lie is at leaf {lie}: {:?}", coord(lie));
    assert_eq!(n, 5_560, "the floor's attempt job");
    assert_eq!(
        b.shape_profile.resolve_node_slot(coord(lie).node_slot).map(|(node, _)| node.role),
        Some(kaspa_consensus_core::palw_step::PalwStepNodeRoleV1::KCacheWrite),
        "a key row"
    );
    assert_eq!(
        (coord(lie).call_index, seg(lie)),
        (0, 1),
        "of the prefill, in segment 1 (leaf 1432, prompt position 2, in the probe's job)"
    );
    let full = licence.full_card();
    let site_for = |state: &PalwChainStateV2, card: usize, c: &C| {
        let payload = borsh::to_vec(&h.v2_payload(card, id, licence.segmented(card), c.clone())).unwrap();
        palw_check_panel_false_valid_v2(state, &h.cards[card], &payload, false, false, h.rules(), None).map(|finding| finding.site)
    };

    // Downstream: every reader "convicts" the root; only the full seat is liable for it.
    for (reader, c) in &readers {
        let reader = *reader;
        assert!(seg(reader) >= 2 && coord(reader).call_index == 0 && coord(reader).position > 2, "leaf {reader}: a later position");
        let site = site_for(&walk.state, full, c).expect("the full seat attested everything the step read");
        let PalwFaultSiteV1::Leaf { leaf, first_read, last_read } = site else { panic!("a step site: {site:?}") };
        eprintln!(
            "[t46p] reader {reader} (segment {}, position {}, node {}) read leaves {first_read}..={last_read}",
            seg(reader),
            coord(reader).position,
            coord(reader).node_slot
        );
        assert_eq!(leaf, reader);
        assert!(first_read <= lie && lie <= last_read, "leaf {reader}'s step read the lie");
        assert!(seg(first_read) < seg(reader), "and leaves of segments before its own");
        for partial in licence.partials() {
            h.refused(&walk, &h.v2(partial, id, licence.segmented(partial), c.clone()), &E::SiteNotAttested.to_string());
        }
    }
    let downstream: Vec<C> = readers.iter().map(|(_, c)| c.clone()).collect();

    // The step at the lie reads only segment 1.
    let at_lie = claim.contradiction();
    let holder = licence.holder_of(1);
    let site = site_for(&walk.state, holder, &at_lie).expect("the holder of segment 1 replayed the lie");
    let PalwFaultSiteV1::Leaf { leaf, first_read, last_read } = site else { panic!("a step site: {site:?}") };
    eprintln!("[t46p] the lie at {lie} read leaves {first_read}..={last_read}");
    assert_eq!((leaf, seg(first_read), seg(last_read)), (lie, 1, 1), "the lie's own step reads only segment 1");
    let others: Vec<usize> = [0u16, 2, 3].into_iter().map(|s| licence.holder_of(s)).collect();
    for &other in &others {
        h.refused(&walk, &h.v2(other, id, licence.segmented(other), at_lie.clone()), &E::SiteNotAttested.to_string());
    }
    // One block: the full seat by a downstream reader's proof, segment 1's holder by the lie's own.
    let (licensed, _) = h.carry(
        &mut walk,
        vec![h.v2(full, id, licence.segmented(full), downstream[0].clone()), h.v2(holder, id, licence.segmented(holder), at_lie)],
    );
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[full, holder], walk.daa);
    for &other in &others {
        assert!(walk.state.slashable_lock(h.cards[other], id).is_some(), "card {other}'s lock stands");
        assert_eq!(walk.state.bond(&h.cards[other]).unwrap().collateral, licensed.bond(&h.cards[other]).unwrap().collateral);
    }

    // T46b's first leaf: the first of segment 2, reading the end of segment 1.
    let (walk, boundary, licence) = h.licensed(Fault::StepAt(n / 2));
    let c = boundary.contradiction();
    let full = licence.full_card();
    let payload = borsh::to_vec(&h.v2_payload(full, boundary.claim_id, licence.segmented(full), c.clone())).unwrap();
    let site = palw_check_panel_false_valid_v2(&walk.state, &h.cards[full], &payload, false, false, h.rules(), None)
        .expect("the full seat attested it")
        .site;
    let PalwFaultSiteV1::Leaf { leaf, first_read, last_read } = site else { panic!("a step site: {site:?}") };
    eprintln!("[t46p] n/2 = {leaf} read leaves {first_read}..={last_read}");
    assert_eq!((leaf, seg(leaf), seg(first_read), last_read), (n / 2, 2, 1, n / 2), "the first leaf of segment 2 reads segment 1");
    for s in [1u16, 2] {
        let holder = licence.holder_of(s);
        h.refused(&walk, &h.v2(holder, boundary.claim_id, licence.segmented(holder), c.clone()), &E::SiteNotAttested.to_string());
    }
    h.validate(&walk.state, &walk.next(), &h.v2(full, boundary.claim_id, licence.segmented(full), c))
        .expect("the full seat is admitted");
}

/// **Every step of the later half of `claim`'s step space (segments 2 and 3 of the four-way cut)
/// whose refutation reads committed leaf `lie` and convicts the claim's root** — built as T46i builds
/// its refutations (the backend's prover over one decode of the capture, the rows a recording oracle
/// resolves), and judged by the adjudicator's execution check in the network's carriage.
fn convicting_readers(h: &H, claim: &RealClaim, lie: u64, ladder: u64) -> Vec<(u64, C)> {
    use kaspa_consensus_core::palw_artifact::PalwRecordingOracleV1;
    use kaspa_consensus_core::palw_step_refute::{
        PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1, check_execution_step_refutation_carried_capped_v1,
    };
    use misaka_palw_base0::produce::base0_material_decode_v1;
    let (binding, tiles, rows, toks, _) = base0_material_decode_v1(&claim.material).expect("decodes");
    let pin = PalwBase0DecodeTokensV1 { logits_rows: rows, generated_token_ids: toks };
    let resolved = misaka_palw_base0::classes::resolve_class_v1(&h.bundle.court, h.floor(), h.artifact_root, &[]).expect("resolves");
    let inventory =
        misaka_palw_base0::inventory::base0_inventory_v1(&resolved.artifact, resolved.inventory_geometry).expect("inventory");
    let ctx_hash = binding.job_context.context_hash();
    let profile_hash = binding.shape_profile.shape_profile_id();
    let mut leaves = vec![Hash64::default(); binding.step_leaf_count as usize];
    for (index, tile) in &tiles {
        leaves[*index as usize] = kaspa_consensus_core::palw_step_leg::step_tile_leaf_hash_v1(&ctx_hash, &profile_hash, tile);
    }
    let capture = misaka_palw_base0::legs::Base0StepTilesV1 { leaves, tiles };
    let prompt_ids: Vec<u32> = claim.prompt.iter().map(|t| *t as u32).collect();
    let coordinates =
        |leaf: u64| kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf);
    let lie_at = coordinates(lie).expect("the lie is a step coordinate");
    let root = claim.envelope.attempt.execution_root;
    let n = binding.step_leaf_count;
    let mut readers = Vec::new();
    for leaf in n / 2..n {
        let Some(coord) = coordinates(leaf) else { continue };
        let Ok(refutation) = misaka_palw_base0::legs::base0_refutation_from_capture_capped_v1(
            &binding.shape_profile,
            &binding.job_context,
            &capture,
            binding.clone(),
            coord,
            prompt_ids.clone(),
            Some(PalwDecodeTokenPinV1::Base0V1(pin.clone())),
            None,
            h.bundle.court.max_step_leaf_count(),
        ) else {
            continue;
        };
        if !refutation.inputs.iter().flat_map(|row| row.preimages.iter()).any(|p| p.coord == lie_at) {
            continue;
        }
        let recorder = PalwRecordingOracleV1::new(inventory.operands());
        let _ = check_execution_step_refutation_carried_capped_v1(&refutation, &recorder, h.form(), h.backend.step_ladder_cap());
        let Some(operand_openings) = recorder.openings() else { continue };
        let c = C::StepArithmetic { refutation, operand_openings };
        if h.convicts(&c, root, ladder).is_ok() {
            readers.push((leaf, c));
        }
    }
    readers
}

/// **T46q: a partial mask is the one the panel assigned, and a full mask is a full attestation**
/// (F2 review, F-4, with the peer's plan until SEAT-S4: a partial-assigned seat may replay the whole
/// job and sign a full `Valid`, licensed through the V1 door). On the injected fault, whose step
/// reads only its own segment:
///
/// 1. a partial seat's V3 `Valid` over a mask the panel did not assign it — the faulted segment,
///    signed by a seat assigned another — is refused `SegmentMaskNotAssigned` at the gate and in the
///    fold (under its assigned mask the same seat is `SiteNotAttested`);
/// 2. licensed through the V1 door with every seat's full V2 `Valid`, a partial-assigned seat whose
///    own segment does not hold the fault is convicted as a full-mask signer — neither a mask
///    mismatch nor `SiteNotAttested`; and so is another such seat's V3 `Valid` over the FULL mask,
///    which is representable (a seat's signature covers whichever mask it signs) and is a full
///    attestation whatever the assignment.
#[tokio::test]
async fn t46q_the_mask_is_the_assigned_one_and_a_full_mask_is_a_full_attestation() {
    let h = harness(true);

    // 1. A mask the panel did not assign.
    let (walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, licence.assignment.segments, claim.fault_leaf.unwrap())
        .expect("in the cut");
    let holder = licence.holder_of(segment);
    let other = licence.partials().into_iter().find(|card| *card != holder).expect("another partial seat");
    let faulted = licence.receipt(holder).segments;
    assert_ne!(faulted, licence.receipt(other).segments);
    let unassigned = h.v2(
        other,
        id,
        PalwFalseValidReceiptV1::Segmented(h.v3_receipt(other, id, h.domain, licence.licensed_daa, faulted)),
        c.clone(),
    );
    let why = E::SegmentMaskNotAssigned.to_string();
    h.refused(&walk, &unassigned, &why);
    assert_eq!(h.fold_refusal(&walk, &unassigned), why);
    h.refused(&walk, &h.v2(other, id, licence.segmented(other), c), &E::SiteNotAttested.to_string());

    // 2. The V1 door: every seat signed a full V2 `Valid`.
    let mut walk = h.genesis_walk();
    let claim = h.open_claim(&mut walk, Fault::Step);
    let id = claim.claim_id;
    let c = claim.contradiction();
    h.bind(&mut walk, id);
    let receipts = h.license_v1(&mut walk, id);
    let panel = walk.state.panel(&id).expect("a bound panel").clone();
    let assignment = palw_segment_assignment_v2(panel.anchor, id, panel.seats.len() as u16);
    let segment = palw_segment_index_of_leaf_v2(claim.binding.step_leaf_count, assignment.segments, claim.fault_leaf.unwrap())
        .expect("in the cut");
    let away: Vec<(usize, PalwSegmentMaskV2)> = panel
        .seats
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != assignment.full_seat as usize && !assignment.mask_of(*i as u16).covers(segment))
        .map(|(i, seat)| (h.card_of(&seat.bond), assignment.mask_of(i as u16)))
        .collect();
    assert!(away.len() >= 2, "partial-assigned seats whose own segment does not hold the fault");
    let ((a, a_mask), (b, _)) = (away[0], away[1]);
    let signed_daa = receipts[0].1.signed_daa;
    // Under their assigned partial masks they would not be liable.
    for (card, mask) in [away[0], away[1]] {
        let partial = PalwFalseValidReceiptV1::Segmented(h.v3_receipt(card, id, h.domain, signed_daa, mask));
        h.refused(&walk, &h.v2(card, id, partial, c.clone()), &E::SiteNotAttested.to_string());
    }
    assert!(!a_mask.is_full(assignment.segments));
    let a_full = receipts.iter().find(|(card, _)| *card == a).expect("the V1 licence carried a's receipt").1.clone();
    let b_full_mask = h.v3_receipt(b, id, h.domain, signed_daa, PalwSegmentMaskV2::full(assignment.segments));
    let (licensed, _) = h.carry(
        &mut walk,
        vec![
            h.v2(a, id, PalwFalseValidReceiptV1::Full(a_full), c.clone()),
            h.v2(b, id, PalwFalseValidReceiptV1::Segmented(b_full_mask), c),
        ],
    );
    assert_convicted_before_final(&h, &licensed, &walk.state, id, &[a, b], walk.daa);
}

/// **T46r: a `ProducerWithholding` void convicts no seat** (F2 review, F-2). An HONEST claim is
/// licensed; a bonded bystander files a held data-availability demand through the gate; the producer
/// — colluding — stays silent, and the sweep voids the claim `ProducerWithholding`. The honest full
/// seat owes no disclosure and has no move in that session, yet a kind-3 `ProducerWithholding` would
/// take its whole lock. Past the fence it is refused `ContradictionNotAdmitted` at the gate and in
/// the fold, the walk drops it, and the block that carried it folds to the empty block's root: the
/// lock, the bond and the ledger are as they were.
#[tokio::test]
async fn t46r_a_producer_withholding_void_convicts_no_seat() {
    use kaspa_consensus_core::palw_held_da_v1::{
        PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT, PALW_HELD_DA_VERSION_V1, PalwHeldAccusationV1, PalwHeldMissingV1,
        palw_held_da_accusation_message_v1,
    };
    // ADR-0152 R6: F-2's refusal is the rule below `palw_rcore_plus` (the v1 court's void); this is its
    // fence-off twin. Past the fence N9 admits `ProducerWithholding` only as a DA-7 default's
    // restatement — the R-core twin is `n9_producer_withholding_restates_a_da_default_only`.
    let h = harness_rcore_off();
    let (mut walk, claim, licence) = h.licensed(Fault::Honest);
    let id = claim.claim_id;
    let mut accusation = PalwHeldAccusationV1 {
        version: PALW_HELD_DA_VERSION_V1,
        claim: id,
        missing: PalwHeldMissingV1::StepRange { first: 0, count: 1 },
        accuser: h.cards[BYSTANDER],
        binding: claim.binding.clone(),
        signature: Vec::new(),
    };
    accusation.signature = sign(
        BYSTANDER,
        palw_held_da_accusation_message_v1(h.domain.as_byte_slice(), &accusation).as_byte_slice(),
        PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
    );
    h.carry(&mut walk, vec![Obj::DefaultAccusedHeld { accusation: Box::new(accusation) }]);
    assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "the session is open");
    // The producer's silence: empty blocks until the disclose window closes.
    let mut voided_daa = None;
    for _ in 0..20_000 {
        let point = walk.next();
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        if let PalwClaimPhaseV2::Voided { voided_daa: daa, reason } = walk.state.claim(&id).unwrap().phase {
            assert_eq!(reason, PalwVoidReasonV2::ProducerWithholding, "silence voids ProducerWithholding");
            voided_daa = Some(daa);
            break;
        }
    }
    let voided_daa = voided_daa.expect("the disclose window closes");
    assert!(walk.state.palw_void_binds_claim_v1(&id, PalwVoidReasonV2::ProducerWithholding, voided_daa), "the void the chain wrote");
    let full = licence.full_card();
    assert!(walk.state.slashable_lock(h.cards[full], id).is_some(), "the honest full seat still holds its lock");
    let contradiction = C::ProducerWithholding { voided_daa };
    let why = palw_false_valid_admission_v1(&contradiction).expect_err("refused by name").to_string();
    assert!(why.contains("ProducerWithholding"), "{why}");
    let object = h.v2(full, id, licence.segmented(full), contradiction);
    h.refused(&walk, &object, &why);
    assert_eq!(h.fold_refusal(&walk, &object), why, "the fold refuses it by the same name");
    let point = walk.next();
    let carried = h.accepted(&walk.state, &point, std::slice::from_ref(&object));
    let block = h.fold(&walk.state, &point, &carried).expect("folds");
    assert_eq!(block.state_root(), h.fold(&walk.state, &point, &[]).unwrap().state_root(), "nothing is written");
    let seat = h.cards[full];
    assert_eq!(block.slashable_lock(seat, id), walk.state.slashable_lock(seat, id));
    assert_eq!(block.bond(&seat).unwrap().collateral, walk.state.bond(&seat).unwrap().collateral);
    assert!(block.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).is_none());
}

// ---------------------------------------------------------------------------------------------
// T46s–t — the F2 residual: a court's DEFAULT is not its verdict
// ---------------------------------------------------------------------------------------------

/// A court the chain voided the claim by, driven to its end on a real claim.
struct CourtDefaultRun {
    walk: Walk,
    claim: RealClaim,
    licence: Licence,
    /// The chain with the court open, before the executor's silence ran its rung out.
    in_court: PalwChainStateV2,
    voided_daa: u64,
    reason: PalwVoidReasonV2,
}

/// **The attack the F2 residual names, on a real claim**: an HONEST floor claim licensed by its
/// five seats through Verification V2; a bonded bystander (card 6) opens a court on it; the
/// producer, colluding, never moves; its rung runs out and the sweep ends the court against it.
///
/// testnet-12's held regime opens no court through a `CourtOpened`; the session is the one a
/// one-move accusation at a fused leaf opens (`open_held_dissection_v1`), and on the floor — which
/// has no fused site — it is written through the carriage with the challenger's stake, as T46n
/// writes it, at the ladder's opening rung. The ending is the sweep's own `Responder` arm: the floor
/// bears weight, so the opening-rung mercy (a weightless class's) does not reach it, and a silent
/// responder DEFAULTS — the same arm, and the same charge, a fused terminal's withheld root claim
/// reaches past the deep fence (pinned on the drill in `palw_state_v2`'s
/// `f2_residual_a_withheld_root_claim_is_a_court_default_past_the_attribution_fence`).
fn court_default_on(h: &H) -> CourtDefaultRun {
    let (walk, claim, licence) = h.licensed(Fault::Honest);
    let id = claim.claim_id;
    let mut walk = walk;
    let at = walk.daa;
    let reserved = walk.state.claim(&id).unwrap().reserved;
    let (challenger, executor) = (h.cards[BYSTANDER], h.cards[EXECUTOR]);
    let first_rung = at + h.sp().turn_deadline_daa();
    let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
        &id,
        &claim.envelope.attempt.trace_root,
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&challenger),
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&executor),
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        h.bundle.court.max_step_leaf_count(),
        at,
        first_rung,
    )
    .expect("a ladder opens");
    let session_id = ladder.session_id();
    walk.state = h.rebuilt(&walk.state, |carriage| {
        carriage.court_sessions.insert(
            session_id,
            kaspa_consensus_core::palw_state_v2::PalwCourtSessionStateV2 {
                claim: id,
                challenger_bond: challenger,
                opened_daa: at,
                deadline_daa: at + h.sp().window_court(),
                ladder,
                dissection: None,
            },
        );
        // The challenger's stake sits in `reserved_exposure` below ADR-0152's `palw_rcore_plus`;
        // past it (A-6, S-3) the open session is the stake, read by the accuser ledger — as T46n writes it.
        if h.sp().rcore_plus_from_daa().is_none() {
            *carriage.reserved_exposure.entry(challenger).or_insert(0) += reserved;
        }
    });
    assert_eq!(walk.state.open_courts_of(&id), 1, "the court is open");
    assert!(first_rung < at + h.sp().window_court(), "the rung fires before the backstop, which would close challenger-side");
    let in_court = walk.state.clone();
    // The producer's silence: empty blocks until the court ends.
    for _ in 0..20_000 {
        let point = walk.next();
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        if walk.state.open_courts_of(&id) == 0 {
            break;
        }
    }
    assert_eq!(walk.state.open_courts_of(&id), 0, "the rung ran out and the court ended");
    let PalwClaimPhaseV2::Voided { voided_daa, reason } = walk.state.claim(&id).unwrap().phase else {
        panic!("the executor's silence ended the court against it: {:?}", walk.state.claim(&id).unwrap().phase)
    };
    assert_eq!(voided_daa, walk.daa, "voided in the block whose sweep ended the court");
    assert!(voided_daa > first_rung, "by the rung, after it passed");
    CourtDefaultRun { walk, claim, licence, in_court, voided_daa, reason }
}

/// The debit a bond took between two states.
fn debit(before: &PalwChainStateV2, after: &PalwChainStateV2, bond: PalwBondKeyV2) -> u64 {
    before.bond(&bond).unwrap().collateral - after.bond(&bond).unwrap().collateral
}

/// **T46s (F2 residual): a court DEFAULT is charged as a fraud and convicts no seat.**
///
/// On testnet-12 the silent executor's claim is voided `CourtDefault` — below the fence (the same
/// run with `palw_offence_attribution` unset) it is `CourtFraud`, as it always was — and the executor
/// pays exactly what the dormant chain charges it: its reservation, its escrow and the court's time.
/// The bystander pays nothing, and no seat's lock or bond moves.
///
/// Then kind 3: `CourtFraud { voided_daa }` naming the default against the full-mask seat, every
/// partial seat, and the full seat's V2-form receipt is refused by name at the gate and in the
/// fold, dropped by the walk, and the block that carried them is the empty block. The red twin
/// shows why the reason is the whole fix: the same chain with the void recorded as `CourtFraud` —
/// what the fold wrote before — convicts the honest full seat of its whole lock.
#[tokio::test]
async fn t46s_a_court_default_convicts_no_seat() {
    let h = harness(true);
    let run = court_default_on(&h);
    let dormant_h = harness(false);
    let dormant = court_default_on(&dormant_h);
    let id = run.claim.claim_id;
    assert_eq!(run.reason, PalwVoidReasonV2::CourtDefault, "past the fence the default is recorded as one");
    assert_eq!(dormant.reason, PalwVoidReasonV2::CourtFraud, "below it, byte for byte what the fold always wrote");

    // Charged as a fraud: the same debit the dormant chain takes, and at least the reservation and
    // the escrow a proven fraud forfeits.
    let executor = h.cards[EXECUTOR];
    let claim = run.in_court.claim(&id).unwrap().clone();
    let escrow = h.sp().claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
    let charged = debit(&run.in_court, &run.walk.state, executor);
    assert_eq!(
        charged,
        debit(&dormant.in_court, &dormant.walk.state, dormant_h.cards[EXECUTOR]),
        "the executor pays what the dormant chain's CourtFraud charges it"
    );
    assert!(u128::from(charged) >= claim.reserved + escrow + claim.rights_reserved, "reservation, escrow and rights at least");
    assert_eq!(debit(&run.in_court, &run.walk.state, h.cards[BYSTANDER]), 0, "the bystander pays nothing");
    for card in PANEL {
        let seat = h.cards[card];
        // The void starts the liability clock (`persist_panel_liability` extends the expiry); the
        // amount a conviction could take is what must not move.
        assert_eq!(
            run.walk.state.slashable_lock(seat, id).map(|lock| lock.amount),
            run.in_court.slashable_lock(seat, id).map(|lock| lock.amount),
            "card {card}'s lock stands"
        );
        assert!(run.walk.state.slashable_lock(seat, id).is_some(), "card {card} still holds it");
        assert_eq!(debit(&run.in_court, &run.walk.state, seat), 0, "card {card} paid nothing");
    }
    let row = run.walk.state.panel_liability(&id).expect("the void keeps the liability row");
    assert_eq!((row.voided_daa, row.void_reason), (Some(run.voided_daa), Some(PalwVoidReasonV2::CourtDefault)));
    assert!(run.walk.state.palw_void_binds_claim_v1(&id, PalwVoidReasonV2::CourtDefault, run.voided_daa));
    assert!(!run.walk.state.palw_void_binds_claim_v1(&id, PalwVoidReasonV2::CourtFraud, run.voided_daa));

    // Kind 3 naming the default: refused by name, for every receipt form and every seat.
    let contradiction = C::CourtFraud { voided_daa: run.voided_daa };
    let full = run.licence.full_card();
    let full_v2 = PalwFalseValidReceiptV1::Full(h.full_receipt(full, id, h.domain, run.licence.licensed_daa));
    let mut filed: Vec<(usize, PalwFalseValidReceiptV1)> = vec![(full, run.licence.segmented(full)), (full, full_v2)];
    filed.extend(run.licence.partials().into_iter().map(|card| (card, run.licence.segmented(card))));
    let point = run.walk.next();
    let empty = h.fold(&run.walk.state, &point, &[]).expect("an empty block folds").state_root();
    for (card, receipt) in filed {
        let payload = h.v2_payload(card, id, receipt.clone(), contradiction.clone());
        let why = palw_check_panel_false_valid_v2(
            &run.walk.state,
            &h.cards[card],
            &borsh::to_vec(&payload).unwrap(),
            false,
            false,
            h.rules(),
            None,
        )
        .expect_err("a default convicts no seat");
        assert!(matches!(why, E::ContradictionNotAdmitted(reason) if reason.contains("DEFAULT")), "card {card}: {why:?}");
        let why = why.to_string();
        let object = h.v2(card, id, receipt, contradiction.clone());
        h.refused(&run.walk, &object, &why);
        assert_eq!(h.fold_refusal(&run.walk, &object), why, "the fold refuses it by the same name");
        let carried = h.accepted(&run.walk.state, &point, std::slice::from_ref(&object));
        let block = h.fold(&run.walk.state, &point, &carried).expect("folds");
        assert_eq!(block.state_root(), empty, "nothing is written");
    }

    // The red twin: the same chain with the default recorded as the fraud the fold used to write.
    let voided_daa = run.voided_daa;
    let mut red = Walk { state: run.walk.state.clone(), daa: run.walk.daa, blue: run.walk.blue };
    red.state = h.rebuilt(&red.state, |carriage| {
        carriage.claims.get_mut(&id).unwrap().phase = PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud };
        carriage.panel_liabilities.get_mut(&id).unwrap().void_reason = Some(PalwVoidReasonV2::CourtFraud);
    });
    let seat = h.cards[full];
    let lock = *red.state.slashable_lock(seat, id).expect("the honest full seat's lock");
    let before = red.state.clone();
    h.carry(&mut red, vec![h.v2(full, id, run.licence.segmented(full), contradiction)]);
    assert!(red.state.slashable_lock(seat, id).is_none(), "recorded as CourtFraud, the silence takes the honest seat's lock");
    assert_eq!(u128::from(debit(&before, &red.state, seat)), lock.amount, "all of it");
}

/// **T46t (F2 residual): a PROVEN `CourtFraud` still convicts.** The one-move court's verdict is a
/// proof: a bonded bystander's `ShardCourtAccused` at the drill's faulted leaf passes the gate, the
/// fold finds the executor guilty and voids the claim `CourtFraud` — never `CourtDefault` — and kind
/// 3's `CourtFraud { voided_daa }` then convicts the full-mask seat of its lock through the gate, the
/// walk and the fold. It is the only kind-3 route for a fault larger than one carrier, so the
/// residual's fix must leave it standing.
#[tokio::test]
async fn t46t_a_proven_court_fraud_still_convicts() {
    use kaspa_consensus_core::palw_shard_court_v1::{
        PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, PALW_SHARD_COURT_VERSION_V1, PalwShardCourtAccusationV1,
        palw_shard_court_session_id_v1,
    };
    let h = harness(true);
    let (mut walk, claim, licence) = h.licensed(Fault::Step);
    let id = claim.claim_id;
    let (carried, prompt_ids_opening) = h.carried(claim.contradiction());
    let C::StepArithmetic { refutation, operand_openings } = carried else { panic!("the drill's fault is a step") };
    let mut accusation = PalwShardCourtAccusationV1 {
        version: PALW_SHARD_COURT_VERSION_V1,
        claim: id,
        execution_root: claim.envelope.attempt.execution_root,
        trace_root: claim.envelope.attempt.trace_root,
        executor_bond: h.cards[EXECUTOR],
        accuser_bond: h.cards[BYSTANDER],
        leaf_index: claim.fault_leaf.unwrap(),
        refutation,
        artifact_openings: operand_openings,
        prompt_ids_opening,
        signature: Vec::new(),
    };
    accusation.signature = sign(
        BYSTANDER,
        palw_shard_court_session_id_v1(h.domain.as_byte_slice(), &accusation).as_byte_slice(),
        PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT,
    );
    h.carry(&mut walk, vec![Obj::ShardCourtAccused { accusation: Box::new(accusation) }]);
    let voided_daa = walk.daa;
    assert!(
        matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa: d, reason: PalwVoidReasonV2::CourtFraud } if d == voided_daa),
        "the one-move verdict is a proof, recorded CourtFraud: {:?}",
        walk.state.claim(&id).unwrap().phase
    );
    let full = licence.full_card();
    let seat = h.cards[full];
    let lock = *walk.state.slashable_lock(seat, id).expect("the full seat's lock");
    let before = walk.state.clone();
    h.carry(&mut walk, vec![h.v2(full, id, licence.segmented(full), C::CourtFraud { voided_daa })]);
    assert!(walk.state.slashable_lock(seat, id).is_none(), "the lock is taken");
    assert_eq!(u128::from(debit(&before, &walk.state, seat)), lock.amount, "the full seat pays its lock");
    let row = walk.state.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).expect("one kind-3 row");
    assert_eq!(
        (row.kind, row.execution_root),
        (PalwOffenceKindV1::PanelFalseValidV2, Hash64::default()),
        "a named void forfeits no root"
    );
}

// ---------------------------------------------------------------------------------------------
// F1 (ADR-0152 v3.1 J-1…J-5, the audit's SPEC §4.7): the job identity — recorded, checked, convicted
// ---------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_offence_attribution_v1::{
    PalwForfeitScopeV1, PalwIdentityFaultV1, palw_binding_identity_fault_v1, palw_executor_refuted_offence_id_v1,
    palw_offence_target_v1,
};

/// The first nonce of bucket `n` — a template on it anchors another job (the anchor keys on the
/// nonce's bucket, `nonce >> PALW_TICKET_NONCE_BUCKET_LOG2`).
fn bucket(n: u64) -> u64 {
    n << kaspa_consensus_core::palw_attempt_v2::PALW_TICKET_NONCE_BUCKET_LOG2
}

/// What an `ExecutorRefuted` conviction before `Final` must have written: the claim voided
/// `CourtFraud` at the conviction, the executor charged its reservation, escrow and rights — and
/// nothing else — one kind-4 row under the per-claim key with the debit and `recorded_root`, the
/// liability row saying `CourtFraud`, and every seat's lock and bond untouched.
fn assert_refuted_before_final(
    h: &H,
    before: &PalwChainStateV2,
    s: &PalwChainStateV2,
    claim_id: Hash64,
    daa: u64,
    recorded_root: Hash64,
) {
    let claim = before.claim(&claim_id).expect("the claim before the conviction").clone();
    assert!(
        matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == daa),
        "the claim is voided CourtFraud at the conviction: {:?}",
        s.claim(&claim_id).unwrap().phase
    );
    let executor = h.cards[EXECUTOR];
    let escrow = h.sp().claim_escrow_reservation_v1(claim.accepted_daa, claim.escrowed_reward);
    let debit = before.bond(&executor).unwrap().collateral - s.bond(&executor).unwrap().collateral;
    assert_eq!(u128::from(debit), claim.reserved + escrow + claim.rights_reserved, "the executor pays reservation, escrow and rights");
    let row = s.consumed_offence(&palw_executor_refuted_offence_id_v1(&executor.0, &claim_id)).expect("one kind-4 row per claim");
    assert_eq!(
        (row.kind, row.accused, row.amount, row.accepted_daa, row.execution_root),
        (PalwOffenceKindV1::ExecutorRefuted, executor.0, debit, daa, recorded_root),
        "kind 4, the executor, the debit it paid, the root only for a proven-false execution"
    );
    if let Some(liability) = s.panel_liability(&claim_id) {
        assert_eq!((liability.voided_daa, liability.void_reason), (Some(daa), Some(PalwVoidReasonV2::CourtFraud)));
    }
    for card in PANEL {
        let seat = h.cards[card];
        assert_eq!(
            s.slashable_lock(seat, claim_id).map(|l| l.amount),
            before.slashable_lock(seat, claim_id).map(|l| l.amount),
            "card {card}'s lock: kind 4 charges the executor alone"
        );
        assert_eq!(s.bond(&seat).unwrap().collateral, before.bond(&seat).unwrap().collateral, "card {card}'s bond");
    }
}

/// **T-THREAD, the harness half: a claim records the anchor its carrying header names** (J-1). The
/// block's own attempt (`palw_v2_fold_attempt_for_tests`: the processor's own `palw_attempt_keys_v1`
/// of the header) records `execution_anchor_v3` of that header; the same attempt carried as MERGED
/// work (the pipeline's own `palw_merged_attempt_work_v1`) records the same anchor from the blue's
/// OWN header; a merged work whose header the store cannot serve records 0 — never the attempt id —
/// and so does every claim with the fence off.
#[tokio::test]
async fn t18_thread_a_claim_records_the_anchor_of_its_own_header() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let parent = walk.state.clone();
    let claim = h.open_claim(&mut walk, Fault::Honest);
    let recorded = walk.state.claim(&claim.claim_id).unwrap().job_identity;
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&claim.header);
    let anchor = execution_anchor_v3(h.domain, pre_pow, h.floor(), &h.cards[EXECUTOR].0, claim.header.nonce);
    assert_eq!((recorded, claim.anchor), (anchor, anchor), "own work: the header's execution anchor");
    assert_eq!(recorded, claim.binding.job_context.job_id, "the job the producer ran");

    // The same attempt as merged work under another chain block, from the blue's own header.
    let point = PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x7EAD_0001),
        daa_score: claim.header.daa_score,
        blue_score: claim.header.blue_score + 1,
        subsidy: 0,
    };
    let subsidy = h.vp().coinbase_manager.calc_block_subsidy(claim.header.daa_score);
    let (merged, _, skips) = h
        .vp()
        .palw_v2_fold_merged_attempt_for_tests(
            &parent,
            h.sp(),
            &point,
            claim.header.hash,
            Some(&claim.header),
            &claim.envelope,
            subsidy,
        )
        .expect("the merged attempt folds");
    assert!(skips.is_empty(), "the merged attempt is admitted: {skips:?}");
    assert_eq!(merged.claim(&claim.claim_id).expect("merged work opens the claim").job_identity, anchor, "merged: the blue's anchor");
    let (headless, _, _) = h
        .vp()
        .palw_v2_fold_merged_attempt_for_tests(&parent, h.sp(), &point, claim.header.hash, None, &claim.envelope, subsidy)
        .expect("folds");
    assert_eq!(
        headless.claim(&claim.claim_id).expect("opened").job_identity,
        Hash64::default(),
        "no header, no anchor: 0 (never the attempt-id fallback the execution key takes)"
    );

    // Fence off: nothing is recorded (the claim builder asserts it on every claim it opens).
    let dormant = harness(false);
    let mut dormant_walk = dormant.genesis_walk();
    let quiet = dormant.open_claim(&mut dormant_walk, Fault::Honest);
    assert_eq!(dormant_walk.state.claim(&quiet.claim_id).unwrap().job_identity, Hash64::default());
}

/// **`f1_job_identity_survives_reorg_across_admission`**: the block that admitted a claim is
/// reverted by its delta to the parent's root exactly, the same attempt is accepted again as merged
/// work under another chain block, and it records the same identity; the state round-trips through
/// the loader and the node's own tip row with the identity in it.
#[tokio::test]
async fn t18_job_identity_survives_reorg_across_admission() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let parent = walk.state.clone();
    let claim = h.open_claim(&mut walk, Fault::Honest);
    let point = PalwBlockContextV2 {
        block: claim.header.hash,
        daa_score: claim.header.daa_score,
        blue_score: claim.header.blue_score,
        subsidy: h.vp().coinbase_manager.calc_block_subsidy(claim.header.daa_score),
    };
    let (admitted, delta, _) =
        h.vp().palw_v2_fold_attempt_for_tests(&parent, h.sp(), &point, &[], &claim.envelope, &claim.header).expect("admits");
    assert_eq!(admitted.state_root(), walk.state.state_root(), "the same block folds to the same root");
    assert_eq!(admitted.claim(&claim.claim_id).unwrap().job_identity, claim.anchor);
    let reverted = revert_delta_v2(&admitted, &delta, h.sp()).expect("the admission reverts");
    assert_eq!(reverted.state_root(), parent.state_root(), "revert restores the parent exactly");
    assert!(reverted.claim(&claim.claim_id).is_none());
    let other = PalwBlockContextV2 { block: Hash64::from_u64_word(0x7EAD_0002), blue_score: point.blue_score + 1, ..point };
    let (again, _, _) = h
        .vp()
        .palw_v2_fold_merged_attempt_for_tests(
            &reverted,
            h.sp(),
            &other,
            claim.header.hash,
            Some(&claim.header),
            &claim.envelope,
            point.subsidy,
        )
        .expect("accepted again as merged work");
    assert_eq!(again.claim(&claim.claim_id).unwrap().job_identity, claim.anchor, "the same identity under another chain block");
    h.reloads(&admitted);
    h.reloads(&again);
    h.restarts(point.block, &admitted);
}

/// **T18b: a borrowed root answers another job.** C0 is an honest run, bound and licensed. C1 and C2
/// are later blocks of the same executor that carry C0's roots — and nothing of their own run — and
/// the chain admits both (no admission rebuilds a root).
///
/// * C1's data-availability answer is C0's event: it opens against the borrowed roots exactly as it
///   did before F1, and past the fence the identity rule refuses it as an answer (J1: the binding's
///   job is C0's anchor); the producer's silence then voids C1 for withholding, the bystander uncharged.
/// * On C2 an `ExecutorRefuted { IdentityMismatch }` through the gate, the walk and the fold voids it
///   `CourtFraud` and charges its executor; the kind-4 row records root 0 (forfeiture by claim); C0's
///   record, panel, locks and rights are byte-identical and its root is not forfeit.
/// * The colluding full seat that signed `Valid` on C2 follows through kind 3 (`CourtFraud` naming
///   the void); a partial seat is not liable at `Whole`.
#[tokio::test]
async fn t18b_a_borrowed_root_answers_another_job() {
    // ADR-0152 R6: the v1 DA court's half of T18b (`DefaultAccusedHeld` / `MaterialDisclosed` against a
    // borrowed binding, the `DefaultDisputed` default) stands below `palw_rcore_plus`; this is its
    // fence-off twin. The R-core twin is `t18b_r_a_borrowed_root_answers_no_rcore_session`.
    let h = harness_rcore_off();
    let mut walk = h.genesis_walk();
    let c0 = h.open_claim_at_nonce(&mut walk, Fault::Honest, 0);
    h.bind(&mut walk, c0.claim_id);
    let lic0 = h.license_v2(&mut walk, c0.claim_id);
    let root = c0.envelope.attempt.execution_root;

    // C1, and a bystander's event accusation on it.
    let c1 = h.open_borrowed_claim(&mut walk, bucket(1), &c0);
    let rec1 = walk.state.claim(&c1.claim_id).unwrap().clone();
    assert_eq!((rec1.execution_root, rec1.trace_root), (root, c0.envelope.attempt.trace_root), "the lender's roots");
    assert_eq!(rec1.job_identity, c1.anchor, "and its own block's anchor");
    h.bind(&mut walk, c1.claim_id);
    let accuser = h.cards[BYSTANDER];
    // A held-DA demand carrying the borrowed binding is not about THIS claim's job: the fold refuses it
    // by the identity rule (its filer proves the borrow with `ExecutorRefuted` instead).
    {
        use kaspa_consensus_core::palw_held_da_v1::{
            PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT, PALW_HELD_DA_VERSION_V1, PalwHeldAccusationV1, PalwHeldMissingV1,
            palw_held_da_accusation_message_v1,
        };
        let mut accusation = PalwHeldAccusationV1 {
            version: PALW_HELD_DA_VERSION_V1,
            claim: c1.claim_id,
            missing: PalwHeldMissingV1::StepRange { first: 0, count: 1 },
            accuser,
            binding: c0.binding.clone(),
            signature: Vec::new(),
        };
        accusation.signature = sign(
            BYSTANDER,
            palw_held_da_accusation_message_v1(h.domain.as_byte_slice(), &accusation).as_byte_slice(),
            PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
        );
        let demand = Obj::DefaultAccusedHeld { accusation: Box::new(accusation) };
        let point = walk.next();
        assert!(h.accepted(&walk.state, &point, std::slice::from_ref(&demand)).is_empty(), "the walk drops it");
        match h.fold(&walk.state, &point, std::slice::from_ref(&demand)) {
            Err(PalwStateV2Error::HeldDaRefused { why, .. }) => assert!(why.contains("answers another job or class (J1)"), "{why}"),
            other => panic!("the fold refuses the borrowed demand by the identity rule: {other:?}"),
        }
    }
    let message = kaspa_consensus_core::palw_state_v2::palw_da_accusation_message_v2(h.domain, &c1.claim_id, 0, &accuser);
    let accused = Obj::DefaultAccused {
        claim: c1.claim_id,
        missing_event_index: 0,
        accuser,
        signature: sign(
            BYSTANDER,
            message.as_byte_slice(),
            kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT,
        ),
    };
    let accuser_collateral = walk.state.bond(&accuser).unwrap().collateral;
    h.carry(&mut walk, vec![accused]);
    assert!(matches!(walk.state.claim(&c1.claim_id).unwrap().phase, PalwClaimPhaseV2::DefaultDisputed { .. }));
    // The lender's event — which opens against the borrowed roots, as it did before F1.
    let disclosure = kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat {
        binding: Box::new(c0.binding.clone()),
        pin: c0.pin.clone().expect("the lender's rows"),
    };
    kaspa_consensus_core::palw_step_refute::check_trace_event_disclosure_v1(
        rec1.trace_root,
        rec1.execution_root,
        0,
        0,
        &disclosure,
        kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    )
    .expect("the lender's event opens against the borrowed roots: without F1 it was an answer");
    let digest = kaspa_consensus_core::palw_state_v2::palw_da_disclosure_digest_v1(&disclosure);
    let answer_message = kaspa_consensus_core::palw_state_v2::palw_da_disclosure_message_v3(h.domain, &c1.claim_id, 0, &digest);
    let answer = Obj::MaterialDisclosed {
        claim: c1.claim_id,
        event_index: 0,
        disclosure,
        signature: sign(
            EXECUTOR,
            answer_message.as_byte_slice(),
            kaspa_consensus_core::palw_state_v2::PALW_DA_DISCLOSURE_V2_MLDSA87_CONTEXT,
        ),
    };
    let point = walk.next();
    assert!(h.accepted(&walk.state, &point, std::slice::from_ref(&answer)).is_empty(), "the walk drops the borrowed answer");
    match h.fold(&walk.state, &point, std::slice::from_ref(&answer)) {
        Err(PalwStateV2Error::DaOpeningRefused { why, .. }) => {
            assert!(why.contains("answers another job or class (J1)"), "{why}")
        }
        other => panic!("the fold refuses the borrowed answer by the identity rule: {other:?}"),
    }
    // The producer's silence: the disclose window closes on it.
    let mut voided = None;
    for _ in 0..20_000 {
        let point = walk.next();
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        if let PalwClaimPhaseV2::Voided { reason, .. } = walk.state.claim(&c1.claim_id).unwrap().phase {
            voided = Some(reason);
            break;
        }
    }
    assert_eq!(voided, Some(PalwVoidReasonV2::ProducerWithholding), "silence voids the borrower for withholding");
    assert_eq!(walk.state.bond(&accuser).unwrap().collateral, accuser_collateral, "and the accuser is not charged");

    // C2: bound, licensed by a colluding panel, and refuted.
    let c2 = h.open_borrowed_claim(&mut walk, bucket(2), &c0);
    h.bind(&mut walk, c2.claim_id);
    let lic2 = h.license_v2(&mut walk, c2.claim_id);
    let refutation = c2.contradiction();
    let finding = h.judge_refuted(&walk.state, c2.claim_id, refutation.clone()).expect("the borrowed binding answers C0's job");
    assert_eq!((finding.forfeit, finding.identity_fault), (PalwForfeitScopeV1::ByClaim, Some(PalwIdentityFaultV1::JobNotTheClaims)));
    let lender_before = (
        walk.state.claim(&c0.claim_id).cloned(),
        walk.state.panel(&c0.claim_id).cloned(),
        lic0.receipts.iter().map(|(card, _)| walk.state.slashable_lock(h.cards[*card], c0.claim_id).copied()).collect::<Vec<_>>(),
        walk.state.panel_liability(&c0.claim_id).cloned(),
    );
    let (before, _) = h.carry(&mut walk, vec![h.refuted(c2.claim_id, refutation.clone())]);
    let daa = walk.daa;
    assert_refuted_before_final(&h, &before, &walk.state, c2.claim_id, daa, Hash64::default());
    let lender_after = (
        walk.state.claim(&c0.claim_id).cloned(),
        walk.state.panel(&c0.claim_id).cloned(),
        lic0.receipts.iter().map(|(card, _)| walk.state.slashable_lock(h.cards[*card], c0.claim_id).copied()).collect::<Vec<_>>(),
        walk.state.panel_liability(&c0.claim_id).cloned(),
    );
    assert_eq!(lender_after, lender_before, "the honest lender's claim, panel, locks and rows are byte-identical");
    assert!(!walk.state.palw_execution_root_is_forfeited_v1(&root), "and its root is not forfeit");
    // A second proof of the same claim's refutation is one offence.
    let again = h.refuted(c2.claim_id, refutation);
    let why = h.validate(&walk.state, &walk.next(), &again).expect_err("one offence per claim");
    assert!(why.contains("already refuted"), "{why}");

    // The colluding full seat follows by kind 3; a partial seat is not liable at `Whole`.
    let full = lic2.full_card();
    let court_fraud = C::CourtFraud { voided_daa: daa };
    for partial in lic2.partials() {
        h.refused(&walk, &h.v2(partial, c2.claim_id, lic2.segmented(partial), court_fraud.clone()), &E::SiteNotAttested.to_string());
    }
    let seat = h.cards[full];
    let lock = *walk.state.slashable_lock(seat, c2.claim_id).expect("the full seat's lock");
    let (licensed, _) = h.carry(&mut walk, vec![h.v2(full, c2.claim_id, lic2.segmented(full), court_fraud)]);
    assert!(walk.state.slashable_lock(seat, c2.claim_id).is_none());
    assert_eq!(licensed.bond(&seat).unwrap().collateral - walk.state.bond(&seat).unwrap().collateral, lock.amount as u64);
    h.reloads(&walk.state);
}

/// **T18c: before any licence, the executor is refuted by its own execution** — (i) the drill's
/// injected arithmetic fault and (iii) a non-canonical leaf count, each through the gate, the walk
/// and the fold on a `Provisional` claim, root recorded and forfeit (execution-proving). (ii) A root
/// with no preimage is answered by nobody: a bystander's event accusation runs out and voids the claim
/// for withholding. (iv) **The R1 residual, pinned:** an honest step tree over garbage logits (the
/// argmax unmoved) is refused by every route F1 has — `ForgedOutput`, `IdentityMismatch`,
/// `OutputMismatch`, the shape pass — until F1c's `LogitsNotStepOutput`.
#[tokio::test]
async fn t18c_before_licence_the_executor_is_refuted() {
    let h = harness(true);
    for fault in [Fault::Step, Fault::Shape] {
        let mut walk = h.genesis_walk();
        let claim = h.open_claim(&mut walk, fault);
        let finding = h.judge_refuted(&walk.state, claim.claim_id, claim.contradiction()).expect("the proof refutes the execution");
        assert_eq!(finding.forfeit, PalwForfeitScopeV1::ByRoot, "{fault:?}");
        let (before, _) = h.carry(&mut walk, vec![h.refuted(claim.claim_id, claim.contradiction())]);
        let daa = walk.daa;
        let root = claim.envelope.attempt.execution_root;
        assert_refuted_before_final(&h, &before, &walk.state, claim.claim_id, daa, root);
        assert!(walk.state.palw_execution_root_is_forfeited_v1(&root), "{fault:?}: a proven-false root is forfeit");
    }

    // (ii) A root with no preimage: `t18c_ii_a_root_with_no_preimage_defaults` (the v1 court's twin
    // below `palw_rcore_plus`, and M3's session past it).

    // (iv) Garbage logits over an honest step tree — the R1 residual.
    // (iv) Garbage logits over an honest step tree — the R1 residual.
    let mut walk = h.genesis_walk();
    let bent = h.open_claim(&mut walk, Fault::GarbageLogits);
    let pin = bent.pin.clone().unwrap();
    let routes = [
        ("ForgedOutput", C::ForgedOutput { binding: bent.binding.clone(), pin: pin.clone(), position: 0 }),
        ("IdentityMismatch", C::IdentityMismatch { binding: bent.binding.clone() }),
        (
            "OutputMismatch",
            C::OutputMismatch {
                binding: bent.binding.clone(),
                pin: kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(pin.clone()),
            },
        ),
        ("Shape", C::StepStructural(PalwStepRefutationV1 { binding: bent.binding.clone(), evidence: PalwStepEvidenceV1::Shape })),
    ];
    for (route, contradiction) in routes {
        let why = h.judge_refuted(&walk.state, bent.claim_id, contradiction.clone()).expect_err("R1 has no F1 route");
        eprintln!("[t18c(iv)] {route}: {why}");
        h.refused(&walk, &h.refuted(bent.claim_id, contradiction), &why.to_string());
    }
}

/// **T18c (ii): a root with no preimage is answered by nobody** — an event accusation runs out and
/// the claim voids for withholding, the producer charged. ADR-0152 R6: below `palw_rcore_plus`
/// through the v1 court (`DefaultDisputed`, a bystander accusing); the R-core twin runs the same claim
/// through M3's session and DA-7's default, the accuser a seat of the panel — past the fence only a
/// seat's session pauses the claim (DA-5, V3S-08), so a bystander's would be outrun by the receipt
/// and bind timeouts (the claim voids `BindTimeout`, uncharged, and the session is released).
fn t18c_ii_body(h: &H, accuser_card: usize) {
    let mut walk = h.genesis_walk();
    let lender = h.open_claim_at_nonce(&mut walk, Fault::Honest, 0);
    let mut fake = lender;
    fake.envelope.attempt.execution_root = Hash64::from_u64_word(0x0000_F4CE);
    let orphan = h.open_borrowed_claim(&mut walk, bucket(3), &fake);
    assert_eq!(walk.state.claim(&orphan.claim_id).unwrap().execution_root, Hash64::from_u64_word(0x0000_F4CE));
    assert!(
        matches!(h.judge_refuted(&walk.state, orphan.claim_id, orphan.contradiction()), Err(E::PanelFalseValidWorkMismatch)),
        "no binding reproduces a root with no preimage"
    );
    h.bind(&mut walk, orphan.claim_id);
    let accuser = h.cards[accuser_card];
    let message = kaspa_consensus_core::palw_state_v2::palw_da_accusation_message_v2(h.domain, &orphan.claim_id, 0, &accuser);
    h.carry(
        &mut walk,
        vec![Obj::DefaultAccused {
            claim: orphan.claim_id,
            missing_event_index: 0,
            accuser,
            signature: sign(
                accuser_card,
                message.as_byte_slice(),
                kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT,
            ),
        }],
    );
    let executor_before = walk.state.bond(&h.cards[EXECUTOR]).unwrap().collateral;
    let mut voided = None;
    for _ in 0..20_000 {
        let point = walk.next();
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        if let PalwClaimPhaseV2::Voided { reason, .. } = walk.state.claim(&orphan.claim_id).unwrap().phase {
            voided = Some(reason);
            break;
        }
    }
    assert_eq!(voided, Some(PalwVoidReasonV2::ProducerWithholding));
    assert!(walk.state.bond(&h.cards[EXECUTOR]).unwrap().collateral < executor_before, "the withholding producer is charged");
}

#[tokio::test]
async fn t18c_ii_a_root_with_no_preimage_defaults() {
    t18c_ii_body(&harness_rcore_off(), BYSTANDER);
}

/// **T18d: an output root that is not the run's output is refuted** (`OutputMismatch`, the one
/// rendered rule): the ground root convicts through the gate, the walk and the fold, root 0 and
/// forfeiture by claim; the honest root with the honest pin is `OutputHolds`, refused at the gate and
/// in the fold.
#[tokio::test]
async fn t18d_the_output_grind_is_refuted() {
    let h = harness(true);
    let (mut walk, claim, _licence) = h.licensed(Fault::OutputGrind);
    let finding = h.judge_refuted(&walk.state, claim.claim_id, claim.contradiction()).expect("the ground root is not the run's");
    assert_eq!((finding.forfeit, finding.identity_fault), (PalwForfeitScopeV1::ByClaim, None));
    let (before, _) = h.carry(&mut walk, vec![h.refuted(claim.claim_id, claim.contradiction())]);
    assert_refuted_before_final(&h, &before, &walk.state, claim.claim_id, walk.daa, Hash64::default());

    let (honest_walk, honest, _) = h.licensed_at_nonce(Fault::Honest, bucket(5));
    let control = C::OutputMismatch {
        binding: honest.binding.clone(),
        pin: kaspa_consensus_core::palw_step_refute::PalwDecodeTokenPinV1::Base0V1(honest.pin.clone().unwrap()),
    };
    let object = h.refuted(honest.claim_id, control);
    h.refused(&honest_walk, &object, &E::OutputHolds.to_string());
    assert_eq!(h.fold_refusal(&honest_walk, &object), E::OutputHolds.to_string());
}

/// **T18e (the floor): the relabel is refuted by J5.** The producer ran another anchor's prompt with
/// this block's anchor written as the job id and the seed, so J1 and J3 hold and the seats' OWN rule
/// (the floor backend's whole-context check) is what the binding fails; on chain the J5 check
/// (the prompt root the anchor implies, J5b) convicts it — the DA answers, which leave J5b out, do
/// not. On a model class the same relabel is F1-M's residual (the core unit test pins it).
#[tokio::test]
async fn t18e_the_floor_relabel_is_refuted_by_j5() {
    let h = harness(true);
    let (mut walk, claim, _) = h.licensed(Fault::Relabel);
    let binding = &claim.binding;
    assert_eq!(binding.job_context.job_id, claim.anchor, "J1 holds: the anchor is written");
    assert_eq!(&binding.job_context.execution_seed[..], &claim.anchor.as_byte_slice()[..32], "J3 holds");
    let target = palw_offence_target_v1(&walk.state, &claim.claim_id).unwrap();
    assert_eq!(palw_binding_identity_fault_v1(&target, binding, h.rules(), true), Ok(Some(PalwIdentityFaultV1::PromptNotTheAnchors)));
    assert_eq!(palw_binding_identity_fault_v1(&target, binding, h.rules(), false), Ok(None), "J5b is the contradictions'");
    // The seats' own rule sees the same thing: the capture is not the anchor's canonical job.
    let verdict = h.backend.verify_material(
        &claim.material,
        kaspa_consensus_core::palw_backend::PalwClaimRootsV1 {
            execution_root: claim.envelope.attempt.execution_root,
            trace_root: claim.envelope.attempt.trace_root,
            anchor: claim.anchor,
            attempt_draw: Some(true),
            // SEAT-S2's field: the answer is compared by the caller, never by `verify_material`, and
            // this check is the job context's (the fixtures' `None`, as SEAT-S2 wrote them).
            output_root: None,
        },
    );
    assert_eq!(verdict, kaspa_consensus_core::palw_backend::PalwMaterialVerdictV1::Mismatch, "a seat refuses the relabel");
    let (before, _) = h.carry(&mut walk, vec![h.refuted(claim.claim_id, claim.contradiction())]);
    assert_refuted_before_final(&h, &before, &walk.state, claim.claim_id, walk.daa, Hash64::default());
}

/// **T18f (J4): the claim committed another trace root beside its binding** — refuted by
/// `IdentityMismatch` (`TraceNotTheClaims`), forfeiture by claim.
#[tokio::test]
async fn t18f_a_trace_root_that_is_not_the_bindings_is_refuted() {
    let h = harness(true);
    let (mut walk, claim, _) = h.licensed(Fault::TraceSwap);
    let finding = h.judge_refuted(&walk.state, claim.claim_id, claim.contradiction()).expect("J4");
    assert_eq!(finding.identity_fault, Some(PalwIdentityFaultV1::TraceNotTheClaims));
    let (before, _) = h.carry(&mut walk, vec![h.refuted(claim.claim_id, claim.contradiction())]);
    assert_refuted_before_final(&h, &before, &walk.state, claim.claim_id, walk.daa, Hash64::default());
}

/// **T18g: an unrecorded identity never convicts** — the borrowed claim of T18b with its identity
/// zeroed (what a claim admitted with no header, or below the fence, records) is refused
/// `IdentityNotRecorded` at the gate and in the fold, and the walk drops it.
#[tokio::test]
async fn t18g_an_unrecorded_identity_never_convicts() {
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let c0 = h.open_claim_at_nonce(&mut walk, Fault::Honest, 0);
    let c1 = h.open_borrowed_claim(&mut walk, bucket(1), &c0);
    let object = h.refuted(c1.claim_id, c1.contradiction());
    h.judge_refuted(&walk.state, c1.claim_id, c1.contradiction()).expect("recorded, the borrower is refuted");
    walk.state = h.rebuilt(&walk.state, |carriage| carriage.claims.get_mut(&c1.claim_id).unwrap().job_identity = Hash64::default());
    h.refused(&walk, &object, &E::IdentityNotRecorded.to_string());
    assert_eq!(h.fold_refusal(&walk, &object), E::IdentityNotRecorded.to_string());
}

/// **T18k: forfeiture by claim leaves the lender's rights intact.** C0 (honest) and C2 (its borrower)
/// both reach `Final` on ONE execution root; a schedule and a pending snapshot minted over both
/// Finals are written through the carriage (as T46e writes one). C2 is refuted after `Final`: its
/// `Final` is reversed, its own Final and tickets leave the schedule, the snapshot and the span's
/// gathered Finals — so no later snapshot takes it — while C0's rows, on the same root, are exactly
/// what they were and the root is not forfeit.
#[tokio::test]
async fn t18k_forfeiture_by_claim_leaves_the_lenders_rights() {
    use kaspa_consensus_core::palw_execution_lane_v1::{PalwExecFinalV1, PalwExecScheduleV1, palw_execution_schedule_snapshot_v1};
    use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_mint_quanta_bounded_v1};
    let h = harness(true);
    let mut walk = h.genesis_walk();
    let c0 = h.open_claim_at_nonce(&mut walk, Fault::Honest, 0);
    let c2 = h.open_borrowed_claim(&mut walk, bucket(2), &c0);
    for id in [c0.claim_id, c2.claim_id] {
        h.bind(&mut walk, id);
        h.license_v2(&mut walk, id);
    }
    h.sweep_to_final(&mut walk, c0.claim_id);
    if !matches!(walk.state.claim(&c2.claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }) {
        h.sweep_to_final(&mut walk, c2.claim_id);
    }
    let root = c0.envelope.attempt.execution_root;
    let final_of = |id: Hash64, domain: u64| PalwExecFinalV1 {
        domain: Hash64::from_u64_word(domain),
        bond: h.cards[EXECUTOR],
        operator_id: walk.state.bond(&h.cards[EXECUTOR]).unwrap().operator_id,
        claim_id: id,
        execution_root: root,
        credit: 4 * PALW_EXECUTION_QUANTUM_V1,
    };
    // One schedule per span, each minted over one of the two Finals — a mint over both would give the
    // shared root's tickets to one claim only (`palw_execution_mint_quanta_bounded_v1` dedups by root).
    let (span, _) = walk.state.round_finals();
    let seed = Hash64::from_u64_word(0x5EED_0018);
    let mint = |span_index: u64, f: PalwExecFinalV1| {
        let finals = [f];
        PalwExecScheduleV1 {
            span_index,
            seed,
            domains: palw_execution_schedule_snapshot_v1(span_index, &finals).domains,
            finals: finals.to_vec(),
            quanta: palw_execution_mint_quanta_bounded_v1(
                &finals,
                seed,
                u128::from(PALW_EXECUTION_QUANTUM_V1),
                10_000,
                0,
                &Default::default(),
                1 << 16,
            ),
        }
    };
    let (lender_span, borrower_span) = (span + 7, span + 8);
    let lender_schedule = mint(lender_span, final_of(c0.claim_id, 0xD0));
    let borrower_schedule = mint(borrower_span, final_of(c2.claim_id, 0xD2));
    assert!(!lender_schedule.quanta.is_empty() && !borrower_schedule.quanta.is_empty(), "both Finals hold tickets");
    let minted_span = borrower_span;
    let pending = palw_execution_schedule_snapshot_v1(minted_span + 1, &[final_of(c0.claim_id, 0xD0), final_of(c2.claim_id, 0xD2)]);
    walk.state = h.rebuilt(&walk.state, |c| {
        c.round_schedules.insert(lender_span, lender_schedule.clone());
        c.round_schedules.insert(borrower_span, borrower_schedule.clone());
        c.round_pending.insert(minted_span + 1, pending.clone());
    });
    let gathered_c0 = walk.state.round_finals().1.get(&c0.claim_id).copied();
    assert!(
        walk.state.round_finals().1.contains_key(&c2.claim_id),
        "the borrower's Final is gathered for its span's snapshot — the path by-claim forfeiture must close"
    );
    let (before, _) = h.carry(&mut walk, vec![h.refuted(c2.claim_id, c2.contradiction())]);
    let s = &walk.state;
    assert!(
        matches!(s.claim(&c2.claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }),
        "the borrower's Final is reversed"
    );
    assert!(matches!(s.claim(&c0.claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "the lender stays Final");
    assert!(before.safe_weight() > s.safe_weight(), "the reversed Final's weight is taken back");
    let pruned = s.round_schedule(borrower_span);
    assert!(
        pruned
            .is_none_or(|p| p.finals.iter().all(|f| f.claim_id != c2.claim_id) && p.quanta.iter().all(|q| q.final_id != c2.claim_id)),
        "the borrower's Final and tickets left its span's schedule"
    );
    assert_eq!(s.round_schedule(lender_span), Some(&lender_schedule), "the lender's schedule, on the same root, byte-identical");
    let snapshot = s.round_pending_snapshots().get(&(minted_span + 1)).expect("the snapshot stands on the lender's Final");
    assert!(snapshot.finals.iter().all(|f| f.claim_id != c2.claim_id) && snapshot.finals.iter().any(|f| f.claim_id == c0.claim_id));
    assert!(s.round_finals().1.get(&c2.claim_id).is_none(), "no later snapshot can take the voided claim");
    assert_eq!(s.round_finals().1.get(&c0.claim_id).copied(), gathered_c0, "the lender's gathered Final, as it was");
    assert!(!s.palw_execution_root_is_forfeited_v1(&root), "the root is the lender's too: not forfeit");
    let row = s.consumed_offence(&palw_executor_refuted_offence_id_v1(&h.cards[EXECUTOR].0, &c2.claim_id)).unwrap();
    assert_eq!((row.amount, row.execution_root), (0, Hash64::default()), "no post-Final executor debit here (R-core's S3), root 0");
    h.reloads(s);
}

// ---- ADR-0152 v3.1 §3.11 (M3): the redesigned data-availability court on real claims ------------
//
// The same harness (real producer runs, real bindings, the gate, the walk and the fold) with
// `palw_rcore_plus` armed (testnet-12 as shipped): sessions live in side maps, answers are
// `MaterialDisclosedV2` from any locked signer, defaults are DA-7's. The v1 court's tests above run on
// `harness_rcore_off()` (R6); these are their R-core twins and M3's own.
mod m3_da_court {
    use super::*;
    use kaspa_consensus_core::palw_da_rcore_v1::{
        PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT, PalwDaAnswerV1, PalwDaUnitV1, palw_da_answer_digest_v1, palw_da_disclosure_message_v4,
        palw_da_offence_id_v1,
    };
    use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
    use kaspa_consensus_core::palw_state_v2::{palw_accuser_exposure_v1, palw_da_accusation_message_v2, palw_da_event_index_v1};

    /// Card `card`'s signed event accusation of `(row, 0)` on `claim`.
    fn accuse(h: &H, claim: Hash64, card: usize, row: u32) -> Obj {
        let index = palw_da_event_index_v1(row, 0);
        let message = palw_da_accusation_message_v2(h.domain, &claim, index, &h.cards[card]);
        Obj::DefaultAccused {
            claim,
            missing_event_index: index,
            accuser: h.cards[card],
            signature: sign(card, message.as_byte_slice(), kaspa_consensus_core::palw_state_v2::PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT),
        }
    }

    /// Card `card`'s signed held accusation of `missing` on `claim`, carrying `binding`.
    fn accuse_held(h: &H, claim: Hash64, card: usize, missing: PalwHeldMissingV1, binding: &PalwStepBindingV2) -> Obj {
        use kaspa_consensus_core::palw_held_da_v1::{
            PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT, PALW_HELD_DA_VERSION_V1, PalwHeldAccusationV1, palw_held_da_accusation_message_v1,
        };
        let mut accusation = PalwHeldAccusationV1 {
            version: PALW_HELD_DA_VERSION_V1,
            claim,
            missing,
            accuser: h.cards[card],
            binding: binding.clone(),
            signature: Vec::new(),
        };
        accusation.signature = sign(
            card,
            palw_held_da_accusation_message_v1(h.domain.as_byte_slice(), &accusation).as_byte_slice(),
            PALW_HELD_DA_MLDSA87_ACCUSE_CONTEXT,
        );
        Obj::DefaultAccusedHeld { accusation: Box::new(accusation) }
    }

    /// Card `card`'s signed answer to `unit` on `claim`.
    fn answer(h: &H, claim: Hash64, card: usize, unit: PalwDaUnitV1, answer: PalwDaAnswerV1) -> Obj {
        let message = palw_da_disclosure_message_v4(&h.domain, &claim, &unit, &palw_da_answer_digest_v1(&answer), &h.cards[card]);
        Obj::MaterialDisclosedV2 {
            claim,
            unit,
            answer,
            discloser: h.cards[card],
            signature: sign(card, message.as_byte_slice(), PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT),
        }
    }

    /// The claim's `Flat` event disclosure (the floor's scheme): its binding and its decode pin.
    fn flat(claim: &RealClaim) -> PalwDaAnswerV1 {
        PalwDaAnswerV1::Event(kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::Flat {
            binding: Box::new(claim.binding.clone()),
            pin: claim.pin.clone().expect("the run's rows"),
        })
    }

    /// `OutOfRange`: the binding alone proves the event is past the run.
    fn out_of_range(claim: &RealClaim) -> PalwDaAnswerV1 {
        PalwDaAnswerV1::Event(kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1::OutOfRange {
            binding: Box::new(claim.binding.clone()),
        })
    }

    /// The fold's refusal of `object` at the next point (the gate admitted it; state refuses it), and
    /// the walk drops it with the block standing.
    fn fold_refuses(h: &H, walk: &Walk, object: &Obj) -> PalwStateV2Error {
        let point = walk.next();
        h.validate(&walk.state, &point, object).expect("the gate's checks pass: the refusal is the fold's");
        assert!(h.accepted(&walk.state, &point, std::slice::from_ref(object)).is_empty(), "the walk drops what the fold refuses");
        h.fold(&walk.state, &point, std::slice::from_ref(object)).expect_err("the fold refuses it")
    }

    /// Empty blocks from `claim`'s session of `card` to the first block past its deadline.
    fn run_out(h: &H, walk: &mut Walk, claim: Hash64, card: usize) {
        let deadline = walk.state.da_session(&claim, &h.cards[card]).expect("an open session").deadline_daa;
        let point = walk.at(deadline + 1);
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        assert!(walk.state.da_session(&claim, &h.cards[card]).is_none(), "gone the first block past its deadline");
        h.reloads(&walk.state);
    }

    /// **T32 + T64 (X7, DA-3, DA-4): a locked signer answers, a Flat answer covers the run's rows, an
    /// out-of-run unit is answered by `OutOfRange`, and the accuser holds its cost.** A partial seat
    /// (a seat: its session pauses the licensed claim) accuses row 7: the fold adds the run's one row
    /// `(0, 0)`. The FULL seat — not the producer, but it holds a live lock (X7) — answers `(0, 0)` with
    /// the floor's `Flat` disclosure (signed under the DA-disclosure-v4 context) and row 7 with
    /// `OutOfRange` from the binding alone; the session is refuted and its exposure held. A second
    /// answer of an answered unit, an answer to a unit no session demands, and an answer from a bond
    /// with no lock on the claim are each refused by the fold and dropped by the walk. The claim then
    /// finalizes at its credited anchor, and its retirement burns the held exposure (DA-6).
    #[tokio::test]
    async fn t32_t64_a_locked_signer_answers_every_unit_and_the_accuser_holds_the_cost() {
        let h = harness(true);
        let (mut walk, claim, licence) = h.licensed(Fault::Honest);
        let id = claim.claim_id;
        let full = licence.full_card();
        let accuser = licence.partials()[0];
        h.carry(&mut walk, vec![accuse(&h, id, accuser, 7)]);
        let paused = walk.daa;
        let session = walk.state.da_session(&id, &h.cards[accuser]).expect("the seat's session").clone();
        assert_eq!(
            session.units,
            vec![PalwDaUnitV1::Event { row: 7, tile: 0 }, PalwDaUnitV1::Event { row: 0, tile: 0 }],
            "the named row and the run's one row (T64)"
        );
        assert!(session.accuser_is_seat && walk.state.deadline_of(&id).is_none(), "a seat session pauses the claim");
        let exposure = session.exposure;
        h.reloads(&walk.state);
        // T40: a node restarting mid-session reloads the state the fold wrote.
        h.restarts(walk.next().block, &walk.state);
        // Refusals: a bond with no lock, a unit nobody demands.
        let bystander = answer(&h, id, BYSTANDER, PalwDaUnitV1::Event { row: 0, tile: 0 }, flat(&claim));
        assert!(matches!(fold_refuses(&h, &walk, &bystander), PalwStateV2Error::DaDiscloserNotLiable { .. }));
        let undemanded = answer(&h, id, full, PalwDaUnitV1::Event { row: 3, tile: 0 }, out_of_range(&claim));
        assert!(matches!(fold_refuses(&h, &walk, &undemanded), PalwStateV2Error::DaUnitNotDemanded(_)));
        // The full seat answers the in-run row with the Flat disclosure.
        h.carry(&mut walk, vec![answer(&h, id, full, PalwDaUnitV1::Event { row: 0, tile: 0 }, flat(&claim))]);
        let record = walk.state.da_claim(&id).unwrap().clone();
        assert!(record.flat_answered, "a Flat answer covers every in-run row (IMPL-16)");
        assert!(walk.state.da_session(&id, &h.cards[accuser]).is_some(), "row 7 is still open");
        let again = answer(&h, id, full, PalwDaUnitV1::Event { row: 0, tile: 0 }, flat(&claim));
        assert!(matches!(fold_refuses(&h, &walk, &again), PalwStateV2Error::DaUnitAlreadyAnswered(_)));
        // Row 7 is past the one-row run: OutOfRange, from the binding alone.
        let full_before = (walk.state.bond(&h.cards[full]).unwrap().collateral, *walk.state.slashable_lock(h.cards[full], id).unwrap());
        h.carry(&mut walk, vec![answer(&h, id, full, PalwDaUnitV1::Event { row: 7, tile: 0 }, out_of_range(&claim))]);
        let answered_at = walk.daa;
        assert!(walk.state.da_session(&id, &h.cards[accuser]).is_none(), "every unit answered: refuted");
        // The producer never answered; the covering signer that did is not charged (X7, DA-4).
        assert_eq!(
            (walk.state.bond(&h.cards[full]).unwrap().collateral, walk.state.slashable_lock(h.cards[full], id).unwrap().amount),
            (full_before.0, full_before.1.amount),
            "the answering covering signer keeps its collateral and its lock"
        );
        let record = walk.state.da_claim(&id).unwrap().clone();
        assert_eq!(record.refuted_held, vec![(h.cards[accuser], exposure)], "the refuted exposure is held (DA-6)");
        assert_eq!(palw_accuser_exposure_v1(&walk.state, &h.cards[accuser]), exposure, "on the accuser's ledger");
        h.reloads(&walk.state);
        // The pause is credited: the licence's anchor moves by the paused span.
        let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = walk.state.claim(&id).unwrap().phase else { panic!("licensed") };
        assert_eq!(licensed_daa, licence.licensed_daa + (answered_at - paused), "DA-5: licensed_daa += the pause");
        let before_final = h.sweep_to_final(&mut walk, id);
        let _ = before_final;
        let PalwClaimPhaseV2::Final { final_daa } = walk.state.claim(&id).unwrap().phase else { panic!("Final") };
        assert_eq!(final_daa, licensed_daa + h.sp().window_challenge_at(licensed_daa) + 1, "Final at the shifted anchor");
        // Retirement burns the held exposure.
        let collateral = walk.state.bond(&h.cards[accuser]).unwrap().collateral;
        let retire = walk.state.deadline_of(&id).expect("the retirement");
        let point = walk.at(retire + 1);
        let next = h.fold(&walk.state, &point, &[]).expect("folds");
        walk.advance(&point, next);
        assert!(walk.state.claim(&id).is_none() && walk.state.da_claim(&id).is_none(), "retired");
        assert_eq!(u128::from(collateral - walk.state.bond(&h.cards[accuser]).unwrap().collateral), exposure, "burned (DA-6)");
        h.reloads(&walk.state);
    }

    /// **T46n-R (N13): an R-core DA session defers no conviction, and the conviction closes it.** With
    /// a bystander's held session and a seat's event session open — and a refuted entry of a third bond
    /// held — the full seat's kind 3 and the executor's kind 4 each land: the claim voids `CourtFraud`,
    /// every session closes with its exposure returned, the refuted entry is refunded, nobody but the
    /// convicted pays, and the state reloads and restarts.
    #[tokio::test]
    async fn t46n_r_an_rcore_da_session_does_not_defer_a_conviction() {
        let h = harness(true);
        let (walk, claim, licence) = h.licensed(Fault::Step);
        let id = claim.claim_id;
        let c = claim.contradiction();
        let full = licence.full_card();
        let seat = licence.partials()[0];
        let mut open = walk.clone();
        h.carry(
            &mut open,
            vec![
                accuse_held(&h, id, BYSTANDER, PalwHeldMissingV1::StepRange { first: 0, count: 1 }, &claim.binding),
                accuse(&h, id, seat, 0),
            ],
        );
        assert_eq!(open.state.da_sessions_of(&id).count(), 2, "two sessions, no DefaultDisputed");
        assert!(matches!(open.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the phase is untouched (DA-1)");
        let held_card = licence.partials()[1];
        open.state = h.rebuilt(&open.state, |carriage| {
            carriage.da_claims.get_mut(&id).unwrap().refuted_held.push((h.cards[held_card], 777));
        });
        for object in [h.v2(full, id, licence.segmented(full), c.clone()), h.refuted(id, c.clone())] {
            let mut w = open.clone();
            let before = w.state.clone();
            h.carry(&mut w, vec![object]);
            assert!(matches!(w.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
            assert_eq!(w.state.da_sessions_of(&id).count(), 0, "the conviction closes every session (N13)");
            assert!(w.state.da_claim(&id).unwrap().refuted_held.is_empty(), "and refunds the refuted exposure (DA-6)");
            for card in [BYSTANDER, seat, held_card] {
                assert_eq!(palw_accuser_exposure_v1(&w.state, &h.cards[card]), 0, "card {card}'s exposure is returned");
                if card != full {
                    assert!(
                        w.state.bond(&h.cards[card]).unwrap().collateral >= before.bond(&h.cards[card]).unwrap().collateral,
                        "card {card} pays nothing"
                    );
                }
            }
            h.reloads(&w.state);
            h.restarts(w.next().block, &w.state);
        }
    }

    /// **T18b-R (J-5 on M3's court): a borrowed root answers no session.** The held accusation that
    /// carries the lender's binding is refused `DaBindingIsIdentityFault` (its filer sends
    /// `ExecutorRefuted` instead); the bystander's event session opens; the lender's `Flat` event,
    /// which opens against the borrowed roots, is refused by the identity rule; silence defaults the
    /// session (DA-7) and the borrower voids `ProducerWithholding`, the accuser uncharged.
    #[tokio::test]
    async fn t18b_r_a_borrowed_root_answers_no_rcore_session() {
        let h = harness(true);
        let mut walk = h.genesis_walk();
        let c0 = h.open_claim_at_nonce(&mut walk, Fault::Honest, 0);
        let c1 = h.open_borrowed_claim(&mut walk, bucket(1), &c0);
        h.bind(&mut walk, c1.claim_id);
        let borrowed = accuse_held(&h, c1.claim_id, BYSTANDER, PalwHeldMissingV1::StepRange { first: 0, count: 1 }, &c0.binding);
        assert!(matches!(fold_refuses(&h, &walk, &borrowed), PalwStateV2Error::DaBindingIsIdentityFault { .. }));
        h.carry(&mut walk, vec![accuse(&h, c1.claim_id, BYSTANDER, 0)]);
        let lender = answer(&h, c1.claim_id, EXECUTOR, PalwDaUnitV1::Event { row: 0, tile: 0 }, flat(&c0));
        match fold_refuses(&h, &walk, &lender) {
            PalwStateV2Error::DaOpeningRefused { why, .. } => assert!(why.contains("answers another job or class"), "{why}"),
            other => panic!("the identity rule refuses the lender's event: {other:?}"),
        }
        let accuser_before = walk.state.bond(&h.cards[BYSTANDER]).unwrap().collateral;
        run_out(&h, &mut walk, c1.claim_id, BYSTANDER);
        assert!(
            matches!(walk.state.claim(&c1.claim_id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
            "silence voids the borrower for withholding: {:?}",
            walk.state.claim(&c1.claim_id).unwrap().phase
        );
        assert_eq!(walk.state.bond(&h.cards[BYSTANDER]).unwrap().collateral, accuser_before, "the accuser is not charged");
        assert!(walk.state.consumed_offence(&palw_da_offence_id_v1(&h.cards[EXECUTOR].0, &c1.claim_id)).is_some(), "a DaDefault record");
    }

    /// **T18c (ii)-R: a root with no preimage defaults under M3's court.**
    #[tokio::test]
    async fn t18c_ii_r_a_root_with_no_preimage_defaults() {
        t18c_ii_body(&harness(true), PANEL[0]);
    }

    /// **N9 and P2-7's constant: `ProducerWithholding` convicts a `Valid` signer only as a DA-7
    /// default's restatement, and only once seats answer DA units.** A partial seat's session on a
    /// licensed claim runs out with nobody answering: the producer takes S1 and a `DaDefault` is
    /// recorded. As shipped (`PALW_RCORE_SEAT_DA_ANSWER_LANDED_V1 = false`) signer liability is
    /// dormant: no signer is charged, and a kind 3 `ProducerWithholding { voided_daa }` against the
    /// full seat is refused at the gate and in the fold, as F-2 refused it. The adjudicator with
    /// signer liability armed (the override the constant's flip gives) admits it against the full seat
    /// — it binds the `DaDefault` record — and refuses it against a partial seat (`SiteNotAttested`:
    /// its site is `Whole`) and at any other DAA.
    #[tokio::test]
    async fn n9_producer_withholding_restates_a_da_default_only() {
        use kaspa_consensus_core::palw_offence_attribution_v1::palw_check_panel_false_valid_v2;
        let h = harness(true);
        assert!(!h.rules().da_signer_liability, "as shipped, signer liability is dormant (P2-7)");
        let (mut walk, claim, licence) = h.licensed(Fault::Honest);
        let id = claim.claim_id;
        let full = licence.full_card();
        let seat = licence.partials()[0];
        let lock = *walk.state.slashable_lock(h.cards[full], id).expect("the full seat's lock");
        h.carry(&mut walk, vec![accuse(&h, id, seat, 0)]);
        let full_before = walk.state.bond(&h.cards[full]).unwrap().collateral;
        run_out(&h, &mut walk, id, seat);
        let PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::ProducerWithholding } = walk.state.claim(&id).unwrap().phase
        else {
            panic!("the default voids the claim")
        };
        assert!(walk.state.consumed_offence(&palw_da_offence_id_v1(&h.cards[EXECUTOR].0, &id)).is_some(), "the DaDefault record");
        let key = palw_false_valid_offence_id_v2(&h.cards[full].0, &id);
        assert!(walk.state.consumed_offence(&key).is_none(), "dormant: no S4 on the covering signer");
        assert_eq!(walk.state.bond(&h.cards[full]).unwrap().collateral, full_before, "and no debit");
        let _ = lock;
        // Kind 3 restating the default: refused while signer liability is dormant.
        let restated = h.v2(full, id, licence.segmented(full), C::ProducerWithholding { voided_daa });
        let why = h.validate(&walk.state, &walk.next(), &restated).expect_err("dormant: refused");
        assert!(why.contains("ProducerWithholding"), "{why}");
        assert!(h.fold_refusal(&walk, &restated).contains("ProducerWithholding"));
        // The adjudicator with signer liability armed: the restatement binds the default.
        let armed = kaspa_consensus_core::palw_offence_attribution_v1::PalwIdentityRulesV1 { da_signer_liability: true, ..h.rules() };
        let judge = |card: usize, daa: u64| {
            let payload = h.v2_payload(card, id, licence.segmented(card), C::ProducerWithholding { voided_daa: daa });
            palw_check_panel_false_valid_v2(&walk.state, &h.cards[card], &borsh::to_vec(&payload).unwrap(), false, false, armed, None)
        };
        let finding = judge(full, voided_daa).expect("armed: the full seat's Valid restates the DA default");
        assert!(!finding.acts_on_claim, "a restatement acts on nothing: the default already did");
        assert_eq!(judge(seat, voided_daa).expect_err("a partial seat"), E::SiteNotAttested, "its site is Whole");
        assert!(matches!(judge(full, voided_daa + 1), Err(E::ContradictionNotAdmitted(_))), "another DAA binds nothing");
    }

    /// **T67-R (DA-5): an answer at the last DAA of the window credits exactly the pause.** A seat's
    /// session pauses the licensed claim at P; the full seat answers at P + 1,199; the licence's anchor
    /// moves by 1,199 and the claim is `Final` only a whole challenge window after the credited
    /// anchor — never in the block after the answer.
    #[tokio::test]
    async fn t67_r_an_answer_at_the_deadline_credits_exactly_the_pause() {
        let h = harness(true);
        let (mut walk, claim, licence) = h.licensed(Fault::Honest);
        let id = claim.claim_id;
        let full = licence.full_card();
        let seat = licence.partials()[0];
        h.carry(&mut walk, vec![accuse(&h, id, seat, 0)]);
        let paused = walk.daa;
        let deadline = walk.state.da_session(&id, &h.cards[seat]).unwrap().deadline_daa;
        let point = walk.at(deadline - 1);
        let next = h.fold(&walk.state, &point, &[]).expect("folds");
        walk.advance(&point, next);
        h.carry(&mut walk, vec![answer(&h, id, full, PalwDaUnitV1::Event { row: 0, tile: 0 }, flat(&claim))]);
        let answered = walk.daa;
        let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = walk.state.claim(&id).unwrap().phase else { panic!("licensed") };
        assert_eq!(licensed_daa, licence.licensed_daa + (answered - paused));
        let next_block = walk.next();
        let after = h.fold(&walk.state, &next_block, &[]).expect("folds");
        assert!(matches!(after.claim(&id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not Final the next block");
        h.sweep_to_final(&mut walk, id);
        let PalwClaimPhaseV2::Final { final_daa } = walk.state.claim(&id).unwrap().phase else { panic!("Final") };
        assert_eq!(final_daa, licensed_daa + h.sp().window_challenge_at(licensed_daa) + 1);
    }

    /// **DA-6 / T69-R: a refuted cost is refunded when the claim is convicted.** A bystander's session
    /// on a faulted claim is refuted (the full seat answers with the Flat disclosure — the fault is in
    /// the output root, not in the logits) and its exposure held; the executor's kind 4 then convicts the claim
    /// and the held exposure returns to the bystander in the conviction's block — a correct filer of
    /// the garbage path nets zero.
    #[tokio::test]
    async fn t69_r_a_refuted_cost_is_refunded_when_the_claim_is_convicted() {
        let h = harness(true);
        // A ground output over an honest run: the logits trace is honest (the Flat answer opens), the
        // output root is not the run's (kind 4 `OutputMismatch` convicts).
        let (mut walk, claim, licence) = h.licensed(Fault::OutputGrind);
        let id = claim.claim_id;
        let full = licence.full_card();
        let collateral = walk.state.bond(&h.cards[BYSTANDER]).unwrap().collateral;
        h.carry(&mut walk, vec![accuse(&h, id, BYSTANDER, 0)]);
        h.carry(&mut walk, vec![answer(&h, id, full, PalwDaUnitV1::Event { row: 0, tile: 0 }, flat(&claim))]);
        assert_eq!(walk.state.da_claim(&id).unwrap().refuted_held.len(), 1, "refuted and held");
        assert!(palw_accuser_exposure_v1(&walk.state, &h.cards[BYSTANDER]) > 0);
        h.carry(&mut walk, vec![h.refuted(id, claim.contradiction())]);
        assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
        assert!(walk.state.da_claim(&id).unwrap().refuted_held.is_empty(), "refunded in the conviction's block");
        assert_eq!(palw_accuser_exposure_v1(&walk.state, &h.cards[BYSTANDER]), 0);
        assert_eq!(walk.state.bond(&h.cards[BYSTANDER]).unwrap().collateral, collateral, "nets zero");
        h.reloads(&walk.state);
    }

    /// **T66 (the floor X2, J-6): a seat names the divergent step leaf and the executor is convicted.**
    /// On a claim whose committed step tree holds an arithmetic lie, a partial seat (having replayed and
    /// found the leaf) opens a held session naming that leaf — a named `StepLeaf` is free past
    /// `palw_rcore_plus` (no seat draw, no once-per-seat rule); the fold draws width-1 ranges beside it.
    /// Its `ExecutorRefuted` with the `StepArithmetic` proof then convicts the claim, which closes the
    /// session with the seat's exposure returned.
    #[tokio::test]
    async fn t66_x2_a_seat_names_the_divergent_leaf_and_the_executor_is_convicted() {
        let h = harness(true);
        let (mut walk, claim, licence) = h.licensed(Fault::Step);
        let id = claim.claim_id;
        let leaf = claim.fault_leaf.expect("the fault's leaf");
        let seat = licence.holder_of(0);
        h.carry(&mut walk, vec![accuse_held(&h, id, seat, PalwHeldMissingV1::StepLeaf { leaf }, &claim.binding)]);
        let units = walk.state.da_session(&id, &h.cards[seat]).unwrap().units.clone();
        assert_eq!(units[0], PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf }), "the named leaf");
        assert!(
            units[1..].iter().all(|u| matches!(u, PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { count: 1, first }) if *first < claim.binding.step_leaf_count)),
            "drawn: width-1 ranges inside the committed step space: {units:?}"
        );
        h.carry(&mut walk, vec![h.refuted(id, claim.contradiction())]);
        assert!(matches!(walk.state.claim(&id).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
        assert!(walk.state.da_session(&id, &h.cards[seat]).is_none(), "the conviction closes the session");
        assert_eq!(palw_accuser_exposure_v1(&walk.state, &h.cards[seat]), 0, "the seat's exposure is returned");
        h.reloads(&walk.state);
    }
}
