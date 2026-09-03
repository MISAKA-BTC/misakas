//! **ADR-0069: end-to-end adjudicability is the price of weight.**
//!
//! [`crate::palw_catalog_coverage`] answers a question about a GRAPH — is every kernel this profile
//! reaches one the adjudicator re-executes, and is every node's shape one it can serve. Call that
//! **static adjudicability**. It is necessary and it is not sufficient, and the ADR-0068 launch
//! audit is what measured the gap: Relaunch 5's share table handed 97.8% of cadence to two families
//! whose backends answer the court's questions with the trait's defaults — `supports_court()` is
//! `false`, `bisect_prefix_state` returns `None`, `refutation_for_index` returns `Err`. Every one of
//! those classes passes coverage. None of them can be convicted, because no party can assemble the
//! evidence a conviction is made of.
//!
//! **End-to-end adjudicability** is the missing half: a real backend, on a real anchor, plays a real
//! dispute and the SHIPPED court convicts a planted fault while acquitting the honest run. That is
//! what this module certifies, and weight requires both properties.
//!
//! # Why a certificate rather than a flag
//!
//! `supports_court()` already exists and is exactly the wrong shape. It is node-local, it is a
//! `bool` a family writes about itself, and it appears in no consensus rule — the audit found it
//! `false` on both model families while cadence flowed to them regardless. A family that lies
//! upward is indistinguishable from one that does not.
//!
//! So certification here is the same construction the catalog gate uses (and for the same reason it
//! was chosen there): a [`PalwE2eCertificateV1`] has a private `_sealed` field, so the ONLY way to
//! hold one is to call [`certify_e2e_family_v1`], and that function does not take anybody's word
//! for anything. It takes the drill's recorded evidence and **re-runs the shipped adjudicator over
//! it** — `check_execution_step_refutation_v1`, the same call `adjudicate_close_proof_v2` makes —
//! requiring an acquittal on the honest material and a conviction at every planted leaf. A forged
//! certificate would therefore need evidence that convicts under the real court, which is precisely
//! the evidence a real court needs. There is nothing left to lie with.
//!
//! # What transfers from a drill to a registered class, and why it is the kernel set
//!
//! A drill runs one backend over one profile. The registered classes it should vouch for are not
//! that profile — a fixture is small and a production class is not — so the certificate has to say
//! what it generalises over.
//!
//! It generalises over the **reachable kernel set**, because that is the granularity at which this
//! tree already reasons about adjudication (`court_catalog_root` is set inclusion over exactly
//! these ids) and because it is the part a court's arithmetic depends on. The step-space walk
//! itself — coordinates, tiling, the bisection's prefix rule — is profile-driven and family-blind:
//! `canonical_step_coordinates` does not know which family it is enumerating. What a drill proves
//! about a BACKEND is that its material carries what a rung needs and its refutations convict; what
//! it proves about ARITHMETIC is bounded by the kernels the drilled graph actually walked through.
//!
//! Hence the admission rule in [`family_certified_for_kernels_v1`]: a class may hold weight iff some
//! certified family's drilled kernel set **contains** the class's reachable set. Not the union over
//! families — a class is served by one backend, and stitching two families' certificates together
//! would certify a graph nobody ever ran. This also makes ADR-0069's invariant 3 (`E2E ⊆ catalog`)
//! true by construction rather than by a second test: a drill cannot walk a kernel the adjudicator
//! cannot re-execute, because the re-execution is what grades the drill.

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;

use crate::palw_step::{PalwShapeProfileV3, PalwStepTableV1, canonical_step_coordinates};
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError, check_execution_step_refutation_v1};

// ---------------------------------------------------------------------------------------------
// Domains
// ---------------------------------------------------------------------------------------------

pub const PALW_E2E_VERSION_V1: u16 = 1;

/// `H(domain ‖ family_id ‖ drilled_class ‖ covering ‖ count ‖ sorted kernel ids)`.
pub const PALW_E2E_FAMILY_DIGEST_DOMAIN: &[u8] = b"misaka-palw/e2e-family-digest/v1";

/// The bundle commitment: `H(domain ‖ count ‖ sorted family digests)`.
pub const PALW_COURT_E2E_ROOT_DOMAIN: &[u8] = b"misaka-palw/court-e2e/root/v1";

/// A family's build-level name, hashed under its own domain.
pub const PALW_E2E_FAMILY_ID_DOMAIN: &[u8] = b"misaka-palw/e2e-family-id/v1";

/// Every domain this module introduces (uniqueness-tested against every other PALW family).
pub const PALW_E2E_ALL_DOMAINS: &[&[u8]] = &[PALW_E2E_FAMILY_DIGEST_DOMAIN, PALW_COURT_E2E_ROOT_DOMAIN, PALW_E2E_FAMILY_ID_DOMAIN];

// ---------------------------------------------------------------------------------------------
// What a drill covered
// ---------------------------------------------------------------------------------------------

/// **Which parts of the step space the planted faults actually reached** (ADR-0069 Decision 3).
///
/// A fault at a leaf the drill omits is a step the family could diverge on unconvicted, so a
/// smaller covering set is a smaller guarantee — and a certificate that did not record its own
/// covering set would let a one-leaf drill vouch for a whole graph. Recorded here, checked by
/// [`certify_e2e_family_v1`] against the profile the drill ran, and hashed into the family digest
/// so a narrower drill cannot inherit a wider certificate's identity.
///
/// The tables are the ones the profile DECLARES: a graph with no GDN layers is not asked for a GDN
/// fault, because there is no such step to plant one at.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Default, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwE2eCoveringV1 {
    pub pre: bool,
    pub gdn: bool,
    pub attn: bool,
    pub post: bool,
    /// A fault inside the prefill call, whose KV length grows with the position.
    pub prefill: bool,
    /// A fault inside a decode call, which reads the cache the prefill left.
    pub decode: bool,
    /// How many distinct leaves were planted and convicted. Recorded because it is the one number
    /// that says how much evidence stands behind the booleans above.
    pub convicted_leaves: u32,
    /// **The family answers malformed material with a verdict rather than a crash.**
    ///
    /// A seat runs `verify_material` on bytes anyone may gossip, with no bond behind them, so the
    /// verb is reachable by a stranger by construction. The launch audit found a family that
    /// fabricated an empty logits row for a row its material did not carry, tiled it into a
    /// zero-leaf tree, and hit an `.expect` — one message killed every seat that read it, and a
    /// claim with no seats never licenses and never reaches a court. A court that can be disarmed
    /// by a message is not a court, so certification asks for this the way it asks for a
    /// conviction.
    pub malformed_refused: bool,
}

impl PalwE2eCoveringV1 {
    /// Is this covering set complete FOR THIS PROFILE — every declared table, both call classes?
    ///
    /// `gdn` is only required of a graph that has GDN layers, and `attn` only of one that has
    /// attention layers, because a table with no layers behind it contributes no steps. Everything
    /// else is unconditional: every profile has a pre table, every profile has a post table (the
    /// shape check refuses one that does not), and every job has a prefill call and — at the
    /// canonical job this drill runs — a decode call.
    pub fn covers(&self, profile: &PalwShapeProfileV3) -> bool {
        let needs_gdn = profile.gdn_layer_exists();
        let needs_attn = profile.attention_layer_exists();
        self.pre
            && self.post
            && self.prefill
            && self.decode
            && (self.gdn || !needs_gdn)
            && (self.attn || !needs_attn)
            && self.convicted_leaves > 0
            && self.malformed_refused
    }
}

// ---------------------------------------------------------------------------------------------
// The family descriptor and its certificate
// ---------------------------------------------------------------------------------------------

/// **One family this build has taken all the way through a dispute.**
///
/// `family_id` is the build's own name for the lineage, hashed — it is evidence for a human
/// reading a log and plays no part in the admission rule, which is set inclusion over
/// `kernel_ids`. `drilled_class_id` anchors the evidence to the graph the faults were planted in,
/// so two certificates for the same kernels but different graphs are two certificates.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwE2eFamilyV1 {
    pub family_id: Hash64,
    /// The `shape_profile_id` of the profile the drill actually ran.
    pub drilled_class_id: Hash64,
    /// Every `kernel_semantics_id` the drilled graph reaches. A registered class may hold weight
    /// under this family iff its own reachable set is contained in this one.
    pub kernel_ids: BTreeSet<Hash64>,
    pub covering: PalwE2eCoveringV1,
}

impl PalwE2eFamilyV1 {
    /// `H(domain ‖ family ‖ class ‖ covering ‖ count ‖ sorted ids)` — count-prefixed so two
    /// families can never concatenate to the same byte stream.
    pub fn digest(&self) -> Hash64 {
        let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_E2E_FAMILY_DIGEST_DOMAIN).to_state();
        h.update(self.family_id.as_byte_slice());
        h.update(self.drilled_class_id.as_byte_slice());
        h.update(&borsh::to_vec(&self.covering).expect("a covering set is borsh-serializable"));
        h.update(&(self.kernel_ids.len() as u32).to_le_bytes());
        for id in &self.kernel_ids {
            h.update(id.as_byte_slice());
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(h.finalize().as_bytes());
        Hash64::from_bytes(out)
    }
}

/// Proof that one family played a real dispute to a conviction under the shipped court.
///
/// Only constructible through [`certify_e2e_family_v1`]. Deliberately **not** `BorshDeserialize`,
/// for the reason [`crate::palw_catalog_coverage::PalwCatalogCoverageCertificateV1`] records: the
/// derive would generate a constructor that fills every field including `_sealed`, so
/// `borsh::from_slice` would mint a certificate from arbitrary bytes in any crate with the drill
/// never having run. Serializing one is useful; reconstructing one must go through the verifier.
#[allow(clippy::manual_non_exhaustive)]
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize)]
pub struct PalwE2eCertificateV1 {
    pub family: PalwE2eFamilyV1,
    /// The digest of the family this certificate vouches for — the value that enters
    /// [`palw_court_e2e_root_v1`].
    pub family_digest: Hash64,
    _sealed: (),
}

// ---------------------------------------------------------------------------------------------
// The drill's evidence
// ---------------------------------------------------------------------------------------------

/// One planted fault and the two answers it produced.
///
/// Both sides come from the SAME prover (`refutation_for_index`), which is the property BASE-0's
/// own drill asserts and the one that makes the evidence meaningful: a prover only the challenger
/// could run would be a prover that decides the verdict.
/// **Serializable, deliberately** — and it does not weaken the seal.
///
/// The certificate must be unforgeable, so it is sealed and is not `BorshDeserialize`. The
/// EVIDENCE is the opposite: it may be written down, carried, and read back by a machine that has
/// never seen the model, because reading it back proves nothing on its own. What makes it mean
/// something is [`certify_e2e_family_v1`] re-running the shipped adjudicator over it, and evidence
/// that does not convict does not certify no matter where it came from.
///
/// That asymmetry is what makes the model tiers certifiable at all. `drill_family_v1` needs a live
/// backend, and a Qwen-scale family needs tens of gigabytes of weights — so a build-time drill
/// would make `court_e2e_root` depend on which artifacts a node happens to hold, which is exactly
/// the order- and environment-dependence the root is pinned to avoid. Instead: whoever holds the
/// weights drills once and exports these vectors; the build ships them; every node re-grades them
/// with no model present and computes the same root. ADR-0069 Decision 3 already says the passing
/// vectors ARE the certification evidence — this is the type that lets them be.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwE2eFaultVectorV1 {
    /// The leaf the fault was planted at, and the leaf both refutations open.
    pub leaf_index: u64,
    /// The honest run's refutation at that leaf. Must ACQUIT — `NoFaultFound`.
    pub honest: PalwExecutionStepRefutationV1,
    /// The corrupted run's refutation at that leaf. Must CONVICT.
    pub guilty: PalwExecutionStepRefutationV1,
    /// The weight rows the close carries, proven against `artifact_root`. The honest and the guilty
    /// close read the same coordinates, so one set serves both; carrying them separately would
    /// invite a drill that gave the two sides different oracles.
    pub operand_openings: Vec<crate::palw_artifact::PalwArtifactOpeningV1>,
    /// The bisection's answers around the fault: `(state_at_leaf, state_at_leaf_plus_one)` for the
    /// honest and the guilty run. A true prefix commitment agrees BEFORE the fault and differs
    /// once the fault is included.
    pub honest_prefix: (Hash64, Hash64),
    pub guilty_prefix: (Hash64, Hash64),
}

/// Everything a drill records, in the form [`certify_e2e_family_v1`] grades.
///
/// Serializable for the reason [`PalwE2eFaultVectorV1`] gives: a family whose weights do not fit
/// in a build drills once, elsewhere, and ships what it proved.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwE2eDrillEvidenceV1 {
    pub family_id: Hash64,
    /// The graph the drill ran. The certificate's kernel set is read off THIS, never supplied.
    pub profile: PalwShapeProfileV3,
    /// The class's registered artifact root — what the operand openings must prove against, and
    /// therefore what makes a close's weight rows the class's own rather than the drill's.
    pub artifact_root: Hash64,
    pub vectors: Vec<PalwE2eFaultVectorV1>,
    /// **How many malformed inputs `verify_material` answered instead of crashing on.**
    ///
    /// The drill has already survived them by the time it reports — a panic would have taken the
    /// process down rather than returned a number — so what this carries is the WIDTH of that
    /// evidence, and the certifier refuses a zero. A family whose seat verb was never pointed at a
    /// stranger's bytes has not been shown to survive one.
    pub malformed_inputs_refused: u32,
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwE2eError {
    #[error("the drilled profile is not well-formed: {0}")]
    Profile(String),
    #[error("the drill planted no faults — an empty drill certifies nothing")]
    NoVectors,
    /// The static half must hold first: a drill cannot vouch for a kernel the adjudicator cannot
    /// re-execute, because the re-execution is what grades the drill.
    #[error("the drilled profile is not statically adjudicable: {0}")]
    NotStaticallyAdjudicable(String),
    #[error("the covering set omits a table or a call class the profile declares: {covering:?}")]
    NotCovering { covering: PalwE2eCoveringV1 },
    #[error("the fault at leaf {leaf} opens a coordinate the profile does not have")]
    LeafIsNotACoordinate { leaf: u64 },
    /// **ADR-0069 Decision 4: the graph must not lie about the engine.**
    ///
    /// A commitment whose step space is a different size from the one the profile declares is a
    /// class whose graph and engine disagree — either the engine performed a narrowing nobody
    /// declared (so its row has no coordinate) or the graph declared a node the engine never
    /// computes (so a leaf was committed as a zero nobody can open). Both are caught at capture
    /// time by `Base0StepCaptureV1` — `UnknownSlot` one way, `CaptureIncomplete` the other — and
    /// this is the line that stops a certificate from being issued over material that reached the
    /// certifier some other way.
    #[error("the committed step space is {committed} leaves and the declared graph enumerates {declared}")]
    GraphMisdescribesTheEngine { committed: u64, declared: u64 },
    #[error("a refutation at leaf {leaf} opens leaf {opened} instead")]
    RefutationOpensAnotherLeaf { leaf: u64, opened: u64 },
    /// The vector's refutations are bound to a graph other than the one the evidence names.
    ///
    /// The certificate's kernel set, its covering and its `drilled_class_id` are all read off
    /// `evidence.profile`, while the verdicts below are read off each refutation's OWN binding —
    /// which carries the profile it was captured under. Leaving the two unbound let a drill of one
    /// graph be filed as evidence for another with the same leaf count (a kernel id moves the
    /// class id and nothing else), and the certificate would then vouch for kernels no court ever
    /// re-executed. The seal is only as good as this line.
    #[error("the vector at leaf {leaf} was drilled under class {vector}, not the evidence's class {drilled}")]
    VectorIsAboutAnotherGraph { leaf: u64, drilled: Hash64, vector: Hash64 },
    #[error("the operand openings do not prove against the class's artifact root: {0}")]
    OperandProofInvalid(String),
    /// The whole point. An honest capture that convicts itself would mean the court punishes
    /// correct arithmetic, which is worse than a court that cannot convict at all.
    #[error("the honest run was CONVICTED at leaf {leaf} — the court would punish correct work")]
    HonestRunConvicted { leaf: u64 },
    #[error("the guilty run was ACQUITTED at leaf {leaf} ({why}) — the fault is unpunishable")]
    GuiltyRunAcquitted { leaf: u64, why: String },
    /// Without this the ladder cannot converge: two parties would have no shared way to compute the
    /// same answer from the same execution.
    #[error("the bisection at leaf {leaf} is not a prefix commitment ({why})")]
    BisectionIsNotAPrefixCommitment { leaf: u64, why: &'static str },
    /// The caller handed a certified set that is not the one the network's ruleset commits to.
    /// Refused rather than read as "nothing is certified": a node whose court can play a different
    /// set of families than the network agreed to must stop, not quietly grant nobody weight.
    #[error("the supplied certified set hashes to {computed}, and the network committed to {committed}")]
    CertifiedSetIsNotTheCommittedOne { committed: Hash64, computed: Hash64 },
    #[error("the free-prompt drill carries {questions} questions for {vectors} vectors — every vector must name the job it answered")]
    FreePromptQuestionsMismatch { vectors: usize, questions: usize },
    #[error("the question at leaf {leaf} is not its own: the prompt does not hash or count to the job it is filed with")]
    FreePromptQuestionNotItsOwn { leaf: u64 },
    #[error("the vector at leaf {leaf} was not run over the question it is filed with (job id, prompt hash or prompt length differ)")]
    VectorIsNotAboutTheQuestion { leaf: u64 },
    #[error("a refutation at leaf {leaf} carries a prompt that is not the question's")]
    RefutationCarriesAnotherPrompt { leaf: u64 },
    #[error("no vector adjudicated a leaf with the user's prompt in hand — the free-prompt path was never exercised")]
    NoPromptCarried,
}

// ---------------------------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------------------------

/// **Certify one family, by re-running the shipped court over the drill's own evidence.**
///
/// Nothing here trusts the drill's report of what happened; the drill supplies refutations and
/// openings, and this grades them with [`check_execution_step_refutation_v1`] — the same call
/// `adjudicate_close_proof_v2` makes on a real close. So the certificate means exactly one thing,
/// and it means it mechanically: on this build, for this family, the court acquits correct work and
/// convicts a planted fault at every leaf the covering set claims.
pub fn certify_e2e_family_v1(evidence: &PalwE2eDrillEvidenceV1) -> Result<PalwE2eCertificateV1, PalwE2eError> {
    // Well-formedness first, before anything walks the shape — the same ordering rule
    // `verify_class_admission_v2` follows, and for the same reason: an unbounded shape decides how
    // much work it costs to reject it.
    evidence.profile.validate_shape().map_err(|e| PalwE2eError::Profile(e.to_string()))?;
    // **The static half, first and from this build's own tables.** ADR-0069 invariant 3 says the
    // E2E set is a subset of what the catalog covers; asserting it here rather than testing it
    // elsewhere makes it true by construction.
    crate::palw_catalog_coverage::verify_profile_coverage_v1(&evidence.profile)
        .map_err(|e| PalwE2eError::NotStaticallyAdjudicable(e.to_string()))?;
    if evidence.vectors.is_empty() {
        return Err(PalwE2eError::NoVectors);
    }

    let drilled_class_id = evidence.profile.shape_profile_id();
    let mut covering = PalwE2eCoveringV1 { malformed_refused: evidence.malformed_inputs_refused > 0, ..Default::default() };
    for vector in &evidence.vectors {
        // **Every vector is about THIS graph.** The verdicts below are read off each refutation's
        // own binding, and `verify_binding` ties that binding's profile to its job context — but
        // nothing ties either to `evidence.profile`, which is where the kernel set, the covering
        // and the drilled class id come from. Checked on both sides and by class id (the id IS the
        // borsh of the profile), so a drill of one graph cannot be filed as evidence for another.
        for refutation in [&vector.honest, &vector.guilty] {
            let bound = refutation.binding.shape_profile.shape_profile_id();
            if bound != drilled_class_id || refutation.binding.job_context.shape_profile_id != drilled_class_id {
                return Err(PalwE2eError::VectorIsAboutAnotherGraph {
                    leaf: vector.leaf_index,
                    drilled: drilled_class_id,
                    vector: bound,
                });
            }
        }
        // The context is the binding's own — the drill cannot hand one profile and adjudicate
        // under another, because the refutation carries the context it was built against.
        let ctx = &vector.honest.binding.job_context;
        // **Decision 4, checked on the evidence rather than trusted to the capture.**
        //
        // `Base0StepCaptureV1` already refuses both directions — a row at a slot the profile does
        // not declare is `UnknownSlot`, a declared leaf nobody filled is `CaptureIncomplete` — so
        // an honest producer cannot commit a mismatched space in the first place. That makes this
        // redundant for material that came through the capture and load-bearing for material that
        // did not: the certifier's job is to be the last reader, and a class whose committed space
        // is a different size from its declared graph's is exactly ADR-0049 Decision F's failure,
        // where a producer performs arithmetic the court recomputes differently and is convicted
        // for doing it correctly.
        // **The cap is the STRUCTURAL top, because the rule on this line is the EQUALITY**
        // (ADR-0082 Decision 1). The certifier holds no ruleset — a drill is graded anywhere, by
        // anyone, and `PalwE2eDrillEvidenceV1` is a borsh object that carries no bundle — so it
        // cannot ask "which ladder". What it CAN do is refuse to invent one: bounding the count at
        // `PALW_STEP_MAX_LEAVES` (the executor's `2^22`) made a class the ruleset admits at `2^26`
        // uncertifiable, and by `Profile("TooManyLeaves")` rather than by anything about the drill.
        // The ladder rule lives at admission (`verify_class_admission_v*`, against the bundle's own
        // `max_step_leaf_count`), which is the one place that holds the number; here the question
        // is only whether the graph enumerates what the capture committed. The enumeration is a
        // closed form, so the wider bound buys no walk.
        let declared = crate::palw_step::step_leaf_count_capped_v1(
            &evidence.profile,
            ctx,
            crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
        )
        .map_err(|e| PalwE2eError::Profile(e.to_string()))?;
        if vector.honest.binding.step_leaf_count != declared {
            return Err(PalwE2eError::GraphMisdescribesTheEngine { committed: vector.honest.binding.step_leaf_count, declared });
        }
        let coord = canonical_step_coordinates(&evidence.profile, ctx, vector.leaf_index)
            .ok_or(PalwE2eError::LeafIsNotACoordinate { leaf: vector.leaf_index })?;
        // Both sides must open the leaf the vector claims, or the evidence is about some other step
        // than the one the covering set is about to count.
        for refutation in [&vector.honest, &vector.guilty] {
            let opened = refutation.output_opening.leaf_index;
            if opened != vector.leaf_index {
                return Err(PalwE2eError::RefutationOpensAnotherLeaf { leaf: vector.leaf_index, opened });
            }
        }

        let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&vector.operand_openings, evidence.artifact_root)
            .map_err(|e| PalwE2eError::OperandProofInvalid(e.to_string()))?;

        // **The two verdicts, from the shipped adjudicator.** `Err(NoFaultFound)` is the acquittal
        // an honest party's own close produces; anything else on that side would mean the court
        // convicts correct arithmetic. `Ok(_)` is the conviction; every error on the guilty side —
        // including `Unadjudicable`, which is the fail-open this whole ADR exists to refuse — is an
        // acquittal and therefore a failure to certify.
        match check_execution_step_refutation_v1(&vector.honest, &operands) {
            Err(PalwStepRefuteError::NoFaultFound) => {}
            _ => return Err(PalwE2eError::HonestRunConvicted { leaf: vector.leaf_index }),
        }
        if let Err(why) = check_execution_step_refutation_v1(&vector.guilty, &operands) {
            return Err(PalwE2eError::GuiltyRunAcquitted { leaf: vector.leaf_index, why: format!("{why:?}") });
        }

        // **The rung, which is what makes a ladder converge on the fault rather than wander.** The
        // two executions must agree through the leaf and differ once it is included; a responder
        // whose prefix state did not have that property would win or lose a bisection for reasons
        // unrelated to whether it computed correctly.
        if vector.honest_prefix.0 != vector.guilty_prefix.0 {
            return Err(PalwE2eError::BisectionIsNotAPrefixCommitment {
                leaf: vector.leaf_index,
                why: "the two runs disagree BEFORE the planted fault, so the ladder cannot narrow into it",
            });
        }
        if vector.honest_prefix.1 == vector.guilty_prefix.1 {
            return Err(PalwE2eError::BisectionIsNotAPrefixCommitment {
                leaf: vector.leaf_index,
                why: "the two runs agree once the planted fault is included, so the rung is uninformative",
            });
        }

        match table_of_slot_v1(&evidence.profile, coord.node_slot) {
            Some(PalwStepTableV1::Pre) => covering.pre = true,
            Some(PalwStepTableV1::Gdn) => covering.gdn = true,
            Some(PalwStepTableV1::Attn) => covering.attn = true,
            Some(PalwStepTableV1::Post) => covering.post = true,
            None => return Err(PalwE2eError::LeafIsNotACoordinate { leaf: vector.leaf_index }),
        }
        if coord.call_index == 0 {
            covering.prefill = true;
        } else {
            covering.decode = true;
        }
        covering.convicted_leaves = covering.convicted_leaves.saturating_add(1);
    }

    if !covering.covers(&evidence.profile) {
        return Err(PalwE2eError::NotCovering { covering });
    }

    let family = PalwE2eFamilyV1 {
        family_id: evidence.family_id,
        drilled_class_id,
        // Read off the graph, never supplied: a drill that named its own kernel set would be
        // certifying arithmetic it had not walked.
        kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&evidence.profile),
        covering,
    };
    let family_digest = family.digest();
    Ok(PalwE2eCertificateV1 { family, family_digest, _sealed: () })
}

/// Which table a global node slot belongs to — the inverse of the `pre ‖ layers ‖ post` walk
/// `canonical_step_coordinates` enumerates in.
///
/// Public because the drill needs it to CHOOSE a covering leaf set and this function is what
/// grades one: a drill that classified slots by its own arithmetic could believe it had covered a
/// table the certifier then scores differently, and the two would disagree about what was proven.
pub fn table_of_slot_v1(profile: &PalwShapeProfileV3, slot: u32) -> Option<PalwStepTableV1> {
    let total = profile.global_node_count();
    if slot >= total {
        return None;
    }
    let pre = profile.pre_nodes.len() as u32;
    if slot < pre {
        return Some(PalwStepTableV1::Pre);
    }
    if slot >= total - profile.post_nodes.len() as u32 {
        return Some(PalwStepTableV1::Post);
    }
    let (_, layer) = profile.resolve_node_slot(slot)?;
    match profile.layer_kind(layer?) {
        crate::palw_step::PalwLayerKindV1::Attention => Some(PalwStepTableV1::Attn),
        crate::palw_step::PalwLayerKindV1::GatedDeltaNet => Some(PalwStepTableV1::Gdn),
    }
}

// ---------------------------------------------------------------------------------------------
// The build's certified set, and the admission rule
// ---------------------------------------------------------------------------------------------

/// **This build's end-to-end certified families, as one hash — the bundle's `court_e2e_root`.**
///
/// The exact counterpart of [`crate::palw_catalog_coverage::palw_court_catalog_root_v1`], and it
/// rides the bundle beside it for the same reason: the gate below is a consensus rule whose input
/// is a build fact, so two nodes that disagree about what their courts can play must produce
/// different ruleset ids and refuse to peer, rather than disagree about who gets paid.
///
/// Derived from [`certified_families_v1`] — the drill's own output, not a restated list.
pub fn palw_court_e2e_root_v1() -> Hash64 {
    palw_court_e2e_root_of_v1(&certified_families_v1())
}

/// **The root of an explicit family set** — the commitment [`palw_court_e2e_root_v1`] takes over
/// this build's own, and the check that makes a caller-supplied set unforgeable.
///
/// [`family_certified_for_weight_v1`] takes the set as an argument and requires it to hash to the
/// root the network committed, which is what lets the gate be a pure function without becoming a
/// caller's opinion: a caller that padded the set with families nobody drilled would produce a
/// different root and be refused. It is the shape `verify_catalog_coverage_v1` had to abandon (its
/// catalog side was an argument, and a caller could pass whatever the reachable set needed) —
/// available here only because a COMMITMENT exists to check the argument against, and that
/// commitment rides the bundle.
///
/// Order-free: the digests are sorted, so two builds that registered the same families in
/// different orders commit the same root.
pub fn palw_court_e2e_root_of_v1(families: &[PalwE2eFamilyV1]) -> Hash64 {
    let mut digests: Vec<Hash64> = families.iter().map(|f| f.digest()).collect();
    digests.sort();
    digests.dedup();
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_COURT_E2E_ROOT_DOMAIN).to_state();
    h.update(&(digests.len() as u64).to_le_bytes());
    for digest in &digests {
        h.update(digest.as_byte_slice());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **May a class reaching `reachable` hold weight on a network committed to `court_e2e_root`?**
///
/// `Ok(Some(family))` when one certified family's drilled kernel set contains the class's reachable
/// set; `Ok(None)` when the set is honest and simply covers nothing this class needs — the
/// weightless answer. `Err` when `certified` is not the set the network committed to, which is a
/// caller bug or a node running a build the network did not agree to, and is refused rather than
/// silently treated as "nothing is certified".
///
/// The root check is what makes the argument safe. Without it this would be
/// `verify_catalog_coverage_v1`'s abandoned shape — a gate whose input the caller chooses — and the
/// audit note on that function records exactly how that fails.
pub fn family_certified_for_weight_v1(
    court_e2e_root: Hash64,
    certified: &[PalwE2eFamilyV1],
    reachable: &BTreeSet<Hash64>,
) -> Result<Option<PalwE2eFamilyV1>, PalwE2eError> {
    let computed = palw_court_e2e_root_of_v1(certified);
    if computed != court_e2e_root {
        return Err(PalwE2eError::CertifiedSetIsNotTheCommittedOne { committed: court_e2e_root, computed });
    }
    Ok(certified.iter().find(|f| reachable.is_subset(&f.kernel_ids)).cloned())
}

/// **ADR-0075 Decision 4: the same question, asked of the genesis set AND the chain's own.**
///
/// The root check binds only `genesis` — that is the set the network committed to at birth. The
/// families in `chain` are chain history: each entered the state through a `FamilyCertified`
/// object whose evidence every node graded with this same court, so they need no second
/// commitment. Genesis is searched first so a class the network already covered keeps the family
/// it always had.
pub fn family_certified_for_weight_v2(
    court_e2e_root: Hash64,
    genesis: &[PalwE2eFamilyV1],
    chain: &[PalwE2eFamilyV1],
    reachable: &BTreeSet<Hash64>,
) -> Result<Option<PalwE2eFamilyV1>, PalwE2eError> {
    if let Some(found) = family_certified_for_weight_v1(court_e2e_root, genesis, reachable)? {
        return Ok(Some(found));
    }
    Ok(chain.iter().find(|f| reachable.is_subset(&f.kernel_ids)).cloned())
}

/// **The `court_e2e_root` the RC networks commit to.**
///
/// Pinned rather than computed at bundle-assembly time, because it is consensus identity and a
/// value read out of a process-global registry would depend on whether a drill had run yet when
/// the params were built — which is not a property an identity may have. The build is checked
/// against it (`the_pinned_rc_e2e_root_is_what_this_build_certifies`, and the node's boot gate), so
/// a binary whose court can play a different set of families refuses to start rather than joining
/// and disagreeing about who gets paid. Exactly the discipline `court_catalog_root` follows.
pub fn palw_rc_court_e2e_root_v1() -> Hash64 {
    Hash64::from_bytes(PALW_RC_COURT_E2E_ROOT_BYTES)
}

/// The pinned value's bytes. Replaced whenever the certified family set changes — which is an
/// activation, never a silent edit: the root is inside every RC network's `consensus_params_id`.
///
/// **Moved for ADR-0082's fused graph** (`PALW-QWEN25-A16-V5`, the fourth family). Measured
/// consequence, because the two halves of this doc are easy to read apart: adding the family alone
/// does NOT move `consensus_params_id` — it moves this constant's COMPUTED twin, and the params id
/// reads the pin. So the fingerprint is unchanged right up until this array is updated, and then
/// it moves. Anyone measuring "does a fourth family move the fingerprint" without also updating
/// the pin measures a build that is not self-consistent and gets "no".
const PALW_RC_COURT_E2E_ROOT_BYTES: [u8; 64] = [
    0x9d, 0xf1, 0x1e, 0xdd, 0x12, 0x57, 0x95, 0x20, 0x2c, 0x10, 0xdc, 0x63, 0x2a, 0xa2, 0x46, 0x56, 0x1d, 0x19, 0x4c, 0x65, 0xbf,
    0x2f, 0x05, 0xbe, 0xe9, 0xff, 0xf2, 0x11, 0xf1, 0x93, 0x03, 0xbb, 0x0b, 0x70, 0xd4, 0x4b, 0x6a, 0x78, 0x23, 0xff, 0xd6, 0xbe,
    0xa6, 0xe9, 0x12, 0x9c, 0x0e, 0x82, 0xf8, 0xa1, 0x4f, 0x87, 0xb9, 0x9d, 0x0b, 0x1d, 0xe4, 0x7a, 0x7d, 0x7f, 0x09, 0x54, 0xeb,
    0x8e,
];

/// **The certified family set this network COMMITS to — readable without a model runtime.**
///
/// The registry below is filled by whoever can run a drill, and a drill needs a backend: those live
/// in crates that depend on this one, and `kaspa-consensus` links them only as a DEV dependency
/// ("a node's consensus never links a model runtime", ADR-0042 Decision 4). So a consensus rule
/// that read the registry would be correct only when some other crate had filled it first — true
/// on a producing node whose boot path drills, false on a validating one, and the disagreement
/// would be about whether a block is valid. That is the failure `court_e2e_root` exists to prevent,
/// arriving through the door meant to enforce it.
///
/// So the SET is derived here, where every node can reach it, and only the parts a drill measures
/// are pinned:
///
/// * `kernel_ids` is computed from the class's own profile — the same `reachable_kernels_v1` the
///   admission gate runs on a registrant. Measured: a family's fixture geometry and its production
///   geometry reach the same set, which is what lets a drill on the small one vouch for the large.
/// * `drilled_class_id` and `convicted_leaves` are FACTS ABOUT A DRILL. Nothing here can recompute
///   them, so they are written down, and `misaka-palw-base0`'s drill asserts that this build
///   reproduces them exactly. A build whose drill produces anything else does not match the network.
///
/// [`palw_court_e2e_root_v1`] is the digest of this, so the pin and the set cannot drift apart.
pub fn palw_rc_certified_families_v1() -> Vec<PalwE2eFamilyV1> {
    let full = |gdn: bool, convicted_leaves: u32| PalwE2eCoveringV1 {
        pre: true,
        gdn,
        attn: true,
        post: true,
        prefill: true,
        decode: true,
        convicted_leaves,
        malformed_refused: true,
    };
    // Four families are pushed below, so the reservation is four. It said three while the body
    // pushed four — harmless, a Vec grows, and worth fixing because a capacity is the cheapest
    // statement of how many things the author believed were here, and it disagreed with them.
    let mut out = Vec::with_capacity(4);
    if let Ok(floor) = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-BASE-0"),
            drilled_class_id: floor.shape_profile_id(),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&floor),
            covering: full(false, 6),
        });
    }
    if let Ok(hybrid) = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
        crate::palw_qwen36_profile::QWEN36_35B_A3B,
    )) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-QWEN36"),
            drilled_class_id: Hash64::from_bytes(QWEN36_DRILLED_CLASS_ID),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&hybrid),
            covering: full(true, 8),
        });
    }
    if let Ok(dense) = crate::palw_qwen25_profile::qwen25_a16_profile_v2(crate::palw_qwen25_profile::QWEN25_1_5B_A16) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-QWEN25-A16"),
            drilled_class_id: Hash64::from_bytes(A16_DRILLED_CLASS_ID),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&dense),
            covering: full(false, 6),
        });
    }
    // **The fused graph is a FOURTH family, not a wider third one** (ADR-0082 Decision 1).
    //
    // `palw_fuse_attention_site_v5` REPLACES the scores/softmax/values kernels with one fused
    // kernel, so no single profile reaches both the fused kernel and the ones it replaces — a
    // union `kernel_ids` over the lineage's rows could only ever be a DECLARED superset, which is
    // a certificate asserting an adjudication nobody performed. Two graphs, two drilled
    // coverages, one family each; a class IS its graph, and these are two graphs.
    //
    // Both fields are read off the SAME projection the drill runs, never written beside it.
    if let Ok(fused) = crate::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(PALW_RC_A16_DRILL_GEOMETRY) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-QWEN25-A16-V5"),
            drilled_class_id: fused.shape_profile_id(),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&fused),
            covering: full(false, 6),
        });
    }
    out
}

/// **The fixture geometry the A16 lineage's drills run on — ONE home, read by both sides.**
///
/// `misaka-palw-base0`'s `a16_fixture_v1` builds its weights from this and the family entries
/// below take their `kernel_ids` from a profile projected from it, so the set a family DECLARES
/// and the graph its drill RUNS cannot come apart. They were separate numbers until the graph-v5
/// row needed a fused fixture and the two were found to have never been the same shape.
pub const PALW_RC_A16_DRILL_GEOMETRY: crate::palw_qwen25_profile::PalwQwen25GeometryV1 =
    crate::palw_qwen25_profile::PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 8,
        ffn_dim: 8,
        attn_heads: 2,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx: 32,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    };

/// A family's build-level name, hashed. Under its own domain so it can never be read as a class id
/// or an artifact root.
pub fn palw_e2e_family_id_v1(name: &str) -> Hash64 {
    let mut h = blake2b_simd::Params::new().hash_length(64).key(PALW_E2E_FAMILY_ID_DOMAIN).to_state();
    h.update(name.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The fixture graphs the two model tiers were drilled on. Facts about a drill, not values anything
/// here recomputes — see [`palw_rc_certified_families_v1`].
const QWEN36_DRILLED_CLASS_ID: [u8; 64] = [
    0xdd, 0x99, 0x86, 0x33, 0x45, 0x98, 0xca, 0xea, 0xe5, 0xae, 0x2e, 0x1c, 0xea, 0xc7, 0xe3, 0xdb, 0x0a, 0x3b, 0xd9, 0xfb, 0xdf,
    0x38, 0x2a, 0xdd, 0xb1, 0x81, 0x5a, 0x7e, 0x4b, 0x4b, 0xf2, 0xe2, 0x89, 0x91, 0xa3, 0x66, 0x19, 0xea, 0xbc, 0x76, 0xf8, 0xa1,
    0x47, 0xfa, 0x95, 0x65, 0x89, 0xde, 0xd4, 0x06, 0x4d, 0x69, 0x59, 0x54, 0x0b, 0x73, 0x13, 0x7a, 0x8f, 0x48, 0x3c, 0x6e, 0x43,
    0xf1,
];
const A16_DRILLED_CLASS_ID: [u8; 64] = [
    0xa1, 0x17, 0xaf, 0xb3, 0x93, 0xb2, 0xf7, 0x6c, 0xa9, 0x17, 0xc2, 0x9f, 0xf3, 0xde, 0x66, 0x7b, 0xae, 0x3e, 0x80, 0xcd, 0x90,
    0x2d, 0xde, 0xb0, 0x80, 0x12, 0x63, 0xff, 0xf9, 0x73, 0x0e, 0xe1, 0x92, 0xd3, 0x60, 0x96, 0x0e, 0x9c, 0x60, 0xe2, 0x21, 0x1c,
    0x13, 0xb7, 0x07, 0xc0, 0x12, 0x8b, 0x96, 0x24, 0xfe, 0xeb, 0x03, 0xce, 0x27, 0xd9, 0x94, 0x95, 0x29, 0x75, 0x72, 0xf7, 0x3a,
    0x04,
];

/// **The families this build has certified end to end.**
///
/// Empty in `kaspa-consensus-core` itself, and that is structural rather than an omission: a drill
/// needs a BACKEND, backends live in `misaka-palw-base0` and the SDK, and those crates depend on
/// this one. So the certified set is registered here by the crate that can actually run the drill,
/// through [`register_certified_family_v1`], at first use.
///
/// The registry is written once, by the build, before any consensus rule reads it — see that
/// function's contract.
pub fn certified_families_v1() -> Vec<PalwE2eFamilyV1> {
    CERTIFIED.lock().expect("the certified-family registry is not poisoned").clone()
}

static CERTIFIED: std::sync::Mutex<Vec<PalwE2eFamilyV1>> = std::sync::Mutex::new(Vec::new());

/// **Register a certified family into this build's set.**
///
/// Takes a `&PalwE2eCertificateV1` rather than a `PalwE2eFamilyV1` so that the only way to reach
/// the registry is through [`certify_e2e_family_v1`]: the seal is what makes "this family is in
/// `court_e2e_root`" and "the shipped court convicted its planted faults" the same fact.
///
/// Idempotent, and ordering-free — the root sorts the digests — so a family registered twice
/// (two callers, one build) does not move the root.
pub fn register_certified_family_v1(certificate: &PalwE2eCertificateV1) {
    let mut set = CERTIFIED.lock().expect("the certified-family registry is not poisoned");
    if !set.contains(&certificate.family) {
        set.push(certificate.family.clone());
    }
}

/// **The admission rule: may a class holding these kernels carry weight?**
///
/// `Some(family)` iff one certified family's drilled kernel set contains every kernel this class
/// reaches. Containment by a SINGLE family, never the union: a class is served by one backend, and
/// stitching two certificates together would vouch for a graph nobody ever ran.
///
/// A class this answers `None` for is still perfectly registrable, still produces blocks, is still
/// gossiped, stored and served — ADR-0039's "admissible for liveness, weightless". What it cannot
/// do is take a slice of the cadence away from families whose work can be checked.
pub fn family_certified_for_kernels_v1(reachable: &BTreeSet<Hash64>) -> Option<PalwE2eFamilyV1> {
    certified_families_v1().into_iter().find(|f| reachable.is_subset(&f.kernel_ids))
}

// ---------------------------------------------------------------------------------------------
// The free-prompt lane (ADR-0073 Decision 1f)
// ---------------------------------------------------------------------------------------------

/// The question a free-prompt drill vector answered: the user's job and the prompt that hashes
/// to it. Filed beside the attempt-shaped evidence, one per vector, in order.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwE2eFreePromptQuestionV1 {
    pub job: crate::palw_freeprompt_v3::PalwFreePromptJobV3,
    pub prompt_token_ids: Vec<u32>,
}

/// **Evidence that a family's FREE-PROMPT path adjudicates** (ADR-0073 Decision 1f): the same
/// drill evidence an attempt certificate is minted from, plus the question each vector answered.
///
/// The attempt lane's certificate says "this family's canonical job is adjudicable". It says
/// nothing about a job whose prompt the user chose — a prover that derives the prompt from the
/// anchor opens no prefill gather on such a job, and a court handed the wrong list files no
/// verdict — so a class's free-prompt lane bears weight (Decision 2) only against evidence that
/// was ABOUT a user's job: bindings that name it by `fp_job_id_v3`, refutations that carry its
/// prompt, and the shipped court convicting and acquitting on those.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwE2eFreePromptDrillEvidenceV1 {
    pub evidence: PalwE2eDrillEvidenceV1,
    pub questions: Vec<PalwE2eFreePromptQuestionV1>,
}

/// A free-prompt certificate. Sealed like [`PalwE2eCertificateV1`]: only
/// [`certify_e2e_free_prompt_lane_v1`] constructs one, so holding it IS having passed.
///
/// The private `_sealed` field is the seal, and `#[non_exhaustive]` is NOT a substitute for it:
/// that attribute stops construction outside the CRATE, while the whole point here is that nothing
/// outside this MODULE may mint a certificate — the grader is the only minter. Same allow, same
/// reason, as [`PalwE2eCertificateV1`] above.
#[allow(clippy::manual_non_exhaustive)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwE2eFreePromptCertificateV1 {
    pub family: PalwE2eFamilyV1,
    pub family_digest: Hash64,
    _sealed: (),
}

/// **An attempt certificate whose vectors were about a user's job** — never the other way round.
///
/// Runs [`certify_e2e_family_v1`] first (every check it makes, including the shipped court
/// convicting the guilty run and clearing the honest one), then asks of every vector: the
/// question it is filed with is its own (the prompt hashes and counts to the job); both bindings
/// name that job (`job_id == fp_job_id_v3(job)`, the prompt hash and the prefill length agree);
/// and both refutations carry that prompt and no other. At least one vector must have carried
/// it — a drill whose provers were all handed nothing certifies the attempt path under a new
/// name, which is the confusion this exists to refuse.
pub fn certify_e2e_free_prompt_lane_v1(
    drill: &PalwE2eFreePromptDrillEvidenceV1,
) -> Result<PalwE2eFreePromptCertificateV1, PalwE2eError> {
    let base = certify_e2e_family_v1(&drill.evidence)?;
    if drill.questions.len() != drill.evidence.vectors.len() {
        return Err(PalwE2eError::FreePromptQuestionsMismatch {
            vectors: drill.evidence.vectors.len(),
            questions: drill.questions.len(),
        });
    }
    let mut carried = false;
    for (vector, question) in drill.evidence.vectors.iter().zip(&drill.questions) {
        let leaf = vector.leaf_index;
        let ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&question.prompt_token_ids);
        if question.job.prompt_token_ids_hash != ids_hash || question.job.prompt_tokens as usize != question.prompt_token_ids.len() {
            return Err(PalwE2eError::FreePromptQuestionNotItsOwn { leaf });
        }
        let job_id = crate::palw_freeprompt_v3::fp_job_id_v3(&question.job);
        for refutation in [&vector.honest, &vector.guilty] {
            let ctx = &refutation.binding.job_context;
            if ctx.job_id != job_id
                || ctx.prompt_token_ids_hash != ids_hash
                || ctx.declared_prefill_tokens != question.job.prompt_tokens
            {
                return Err(PalwE2eError::VectorIsNotAboutTheQuestion { leaf });
            }
            if refutation.prompt_token_ids != question.prompt_token_ids {
                return Err(PalwE2eError::RefutationCarriesAnotherPrompt { leaf });
            }
            carried |= !refutation.prompt_token_ids.is_empty();
        }
    }
    if !carried {
        return Err(PalwE2eError::NoPromptCarried);
    }
    Ok(PalwE2eFreePromptCertificateV1 { family: base.family, family_digest: base.family_digest, _sealed: () })
}

/// **The RC free-prompt-certified set** — the families whose free-prompt lane bears weight
/// (ADR-0073 Decision 2, activated by ADR-0074 Decision 6). A class is in it by its drilled
/// class id, and the transition refuses a free-prompt commitment on any class that is not
/// (`PalwStateParamsV2::fp_certified_classes`).
///
/// **Four entries.** This said "one entry: the floor" — true when the floor was the only family
/// that had drilled, and still on the page after QWEN36, QWEN25-A16 and QWEN25-A16-V5 joined it.
/// The body below is the authority; this paragraph is a reader's map that had stopped matching
/// the terrain.
///
/// * `PALW-BASE-0` — the floor, from `misaka-palw-base0`'s
///   `the_floor_free_prompt_lane_certifies_and_a_swapped_question_is_refused`;
/// * `PALW-QWEN36` and `PALW-QWEN25-A16` — joined when their free-prompt paths drilled;
/// * `PALW-QWEN25-A16-V5` — the fused row's family, whose fixture drills `AttnFused` through
///   stream I's dissection route.
///
/// Each covering is pinned here exactly as the attempt set's is, and `e2e_drill` asserts the two
/// agree. **Not an idle doc**: the set feeds `palw_rc_fp_certified_class_ids_v1`, and a reader who
/// trusts "one entry" mis-reads what bears weight.
pub fn palw_rc_fp_certified_families_v1() -> Vec<PalwE2eFamilyV1> {
    let full = |gdn: bool, convicted_leaves: u32| PalwE2eCoveringV1 {
        pre: true,
        gdn,
        attn: true,
        post: true,
        prefill: true,
        decode: true,
        convicted_leaves,
        malformed_refused: true,
    };
    // Four families are pushed below, so the reservation is four. It said three while the body
    // pushed four — harmless, a Vec grows, and worth fixing because a capacity is the cheapest
    // statement of how many things the author believed were here, and it disagreed with them.
    let mut out = Vec::with_capacity(4);
    if let Ok(floor) = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-BASE-0"),
            drilled_class_id: floor.shape_profile_id(),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&floor),
            covering: full(false, 6),
        });
    }
    // ADR-0075 Decision 6: the two model tiers, drilled on the same fixture graphs their
    // attempt-lane certificates were drilled on (`rc_free_prompt_evidence_v1` in
    // `misaka-palw-base0`), with a caller's prompt instead of the anchor's. Pinned from those
    // drills field for field (`the_rc_free_prompt_set_is_the_one_this_build_drilled`).
    if let Ok(hybrid) = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
        crate::palw_qwen36_profile::QWEN36_35B_A3B,
    )) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-QWEN36"),
            drilled_class_id: Hash64::from_bytes(QWEN36_DRILLED_CLASS_ID),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&hybrid),
            covering: full(true, 8),
        });
    }
    if let Ok(dense) = crate::palw_qwen25_profile::qwen25_a16_profile_v2(crate::palw_qwen25_profile::QWEN25_1_5B_A16) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-QWEN25-A16"),
            drilled_class_id: Hash64::from_bytes(A16_DRILLED_CLASS_ID),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&dense),
            covering: full(false, 6),
        });
    }
    // The fused graph's free-prompt twin. Same fixture, same projection, same reason it is a
    // separate family rather than a wider one — see the attempt lane's comment.
    if let Ok(fused) = crate::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v5(PALW_RC_A16_DRILL_GEOMETRY) {
        out.push(PalwE2eFamilyV1 {
            family_id: palw_e2e_family_id_v1("PALW-QWEN25-A16-V5"),
            drilled_class_id: fused.shape_profile_id(),
            kernel_ids: crate::palw_class_admission_v2::reachable_kernels_v1(&fused),
            covering: full(false, 6),
        });
    }
    out
}

/// **The RC classes whose free-prompt lane is certified at genesis** (ADR-0074 Decision 6,
/// ADR-0075 Decision 6): every RC class some free-prompt-certified family covers — the rule
/// `ClassLaneCertified` applies on chain, applied to the shipped catalog at build time. The set
/// is class ids because the free-prompt arm of the transition holds no profile to read kernels
/// off; the coverage is decided here, once, and re-decided on chain for every later class.
pub fn palw_rc_fp_certified_class_ids_v1() -> BTreeSet<Hash64> {
    let families = palw_rc_fp_certified_families_v1();
    let mut classes: Vec<crate::palw_step::PalwShapeProfileV3> = Vec::with_capacity(3);
    if let Ok(floor) = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY) {
        classes.push(floor);
    }
    if let Ok(hybrid) = crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
        crate::palw_qwen36_profile::QWEN36_35B_A3B,
    )) {
        classes.push(hybrid);
    }
    if let Ok(dense) = crate::palw_qwen25_profile::qwen25_a16_profile_v2(crate::palw_qwen25_profile::QWEN25_1_5B_A16) {
        classes.push(dense);
    }
    classes
        .iter()
        .filter(|profile| {
            let reachable = crate::palw_class_admission_v2::reachable_kernels_v1(profile);
            families.iter().any(|f| reachable.is_subset(&f.kernel_ids))
        })
        .map(|profile| profile.shape_profile_id())
        .collect()
}

/// The root the free-prompt set commits to, derived from [`palw_rc_fp_certified_families_v1`].
pub fn palw_rc_court_fp_e2e_root_v1() -> Hash64 {
    palw_court_e2e_root_of_v1(&palw_rc_fp_certified_families_v1())
}

/// **A family covering everything this build catalogs — for tests that must SATISFY the weight
/// gate rather than exercise it.**
///
/// In-crate tests of the static admission properties (ids, coverage, ladder depth, court cost, pwu
/// derivation) all need a nonzero share to be grantable, and `kaspa-consensus-core` cannot run a
/// drill: backends live in crates that depend on this one. So they pass this set together with a
/// bundle rooted at [`palw_court_e2e_root_of_v1`] of it, which is honest — the gate still checks
/// the set against the commitment, and nothing is admitted that A4 would not admit, since coverage
/// already requires a class's kernels to be a subset of the catalog.
///
/// The gate's REFUSAL is proven where a genuinely uncertified family exists: `misaka-palw-base0`'s
/// drill, against the shipped genesis table.
#[cfg(test)]
pub(crate) fn catalog_covering_family_for_tests_v1() -> Vec<PalwE2eFamilyV1> {
    vec![PalwE2eFamilyV1 {
        family_id: Hash64::from_u64_word(0xFA11),
        drilled_class_id: Hash64::from_u64_word(0xFA12),
        kernel_ids: crate::palw_step_refute::catalogued_kernel_ids_v1(),
        covering: PalwE2eCoveringV1 {
            pre: true,
            gdn: true,
            attn: true,
            post: true,
            prefill: true,
            decode: true,
            convicted_leaves: 6,
            malformed_refused: true,
        },
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **An empty build certifies nothing, and its root says so.**
    ///
    /// The root is a hash of the empty set rather than a zero sentinel, because a bundle field that
    /// is `Hash64::default()` reads as "unset" everywhere else in this tree and the boot gate
    /// refuses it. A build with no drilled families is a legitimate build — it simply pays no
    /// family weight — so it must have a real root.
    #[test]
    fn a_build_that_has_certified_nothing_still_has_a_root() {
        assert_ne!(palw_court_e2e_root_v1(), Hash64::default(), "the empty set still commits to something");
    }

    /// The covering rule is about the profile's OWN tables: a graph with no GDN layers cannot be
    /// asked for a GDN fault, and requiring one would make every attention-only class uncertifiable.
    #[test]
    fn covering_is_measured_against_the_tables_the_profile_declares() {
        let profile = crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the shipped floor profile is well-formed");
        assert!(!profile.gdn_layer_exists(), "the floor is attention-only, which is what makes it the right fixture here");

        let full = PalwE2eCoveringV1 {
            pre: true,
            gdn: false,
            attn: true,
            post: true,
            prefill: true,
            decode: true,
            convicted_leaves: 4,
            malformed_refused: true,
        };
        assert!(full.covers(&profile), "a drill that skipped a table the graph does not have is still covering");

        // Each of the required parts, removed one at a time — asserted as a DIFFERENCE, per
        // ADR-0069 invariant 2, so a collapsed covering set cannot pass silently.
        for (what, narrowed) in [
            ("pre", PalwE2eCoveringV1 { pre: false, ..full.clone() }),
            ("attn", PalwE2eCoveringV1 { attn: false, ..full.clone() }),
            ("post", PalwE2eCoveringV1 { post: false, ..full.clone() }),
            ("prefill", PalwE2eCoveringV1 { prefill: false, ..full.clone() }),
            ("decode", PalwE2eCoveringV1 { decode: false, ..full.clone() }),
            ("any leaf at all", PalwE2eCoveringV1 { convicted_leaves: 0, ..full.clone() }),
            ("the malformed-material arm", PalwE2eCoveringV1 { malformed_refused: false, ..full.clone() }),
        ] {
            assert!(!narrowed.covers(&profile), "a drill missing {what} must not read as covering");
        }
    }

    /// **The digest moves with everything it vouches for.** Two families that differ in the kernels
    /// they walked, the graph they walked them in, or how much of the space they covered are two
    /// certificates — otherwise a narrow drill could inherit a wide one's identity.
    #[test]
    fn the_family_digest_separates_what_it_promises() {
        let base = PalwE2eFamilyV1 {
            family_id: Hash64::from_u64_word(1),
            drilled_class_id: Hash64::from_u64_word(2),
            kernel_ids: [Hash64::from_u64_word(3)].into_iter().collect(),
            covering: PalwE2eCoveringV1 { pre: true, attn: true, post: true, prefill: true, decode: true, ..Default::default() },
        };
        let mut wider = base.clone();
        wider.kernel_ids.insert(Hash64::from_u64_word(4));
        let mut other_graph = base.clone();
        other_graph.drilled_class_id = Hash64::from_u64_word(9);
        let mut narrower = base.clone();
        narrower.covering.decode = false;
        let mut renamed = base.clone();
        renamed.family_id = Hash64::from_u64_word(8);

        let digests: BTreeSet<Hash64> = [&base, &wider, &other_graph, &narrower, &renamed].iter().map(|f| f.digest()).collect();
        assert_eq!(digests.len(), 5, "each of the five differs from the others in exactly the field that should matter");
    }

    /// **ADR-0069 Decision 6: certification is a property of a build, not a signature.**
    ///
    /// The thing this ADR must not become is a maintainer's approval list — that is the central
    /// party ADR-0067 exists to remove, and it would make "permissionless registration" a slogan
    /// with a gatekeeper behind it. So the certificate carries no signer, no key and no authority
    /// field: it is a family descriptor and the digest of what a drill proved about it, and anyone
    /// who ships weights, an adjudicable backend and a passing drill mints the same value on their
    /// own machine.
    ///
    /// Asserted structurally, over the type's own serialization, because this is a property that
    /// would be lost by a well-meaning addition rather than by a bug.
    #[test]
    fn a_certificate_names_no_authority_to_ask() {
        let src = include_str!("palw_e2e_adjudicability.rs");
        let decl = src.split("pub struct PalwE2eFamilyV1").nth(1).expect("the descriptor is declared here");
        let body = decl.split('}').next().expect("its fields end at the brace");
        for banned in ["signature", "signer", "pubkey", "authority", "approved_by", "maintainer"] {
            assert!(!body.contains(banned), "a family descriptor carrying `{banned}` would make certification someone's permission");
        }
    }

    /// **Containment is by one family, not by the union.** A class whose kernels are split across
    /// two certificates is a class no single backend was ever drilled on.
    #[test]
    fn certification_does_not_stitch_two_families_together() {
        let a: BTreeSet<Hash64> = [Hash64::from_u64_word(1), Hash64::from_u64_word(2)].into_iter().collect();
        let b: BTreeSet<Hash64> = [Hash64::from_u64_word(3)].into_iter().collect();
        let families = [
            PalwE2eFamilyV1 {
                family_id: Hash64::from_u64_word(0xA),
                drilled_class_id: Hash64::from_u64_word(0xA),
                kernel_ids: a,
                covering: PalwE2eCoveringV1::default(),
            },
            PalwE2eFamilyV1 {
                family_id: Hash64::from_u64_word(0xB),
                drilled_class_id: Hash64::from_u64_word(0xB),
                kernel_ids: b,
                covering: PalwE2eCoveringV1::default(),
            },
        ];
        // The rule, applied to a fixed set rather than the process-wide registry so the test says
        // something about the rule and not about what else has registered.
        let holds = |want: BTreeSet<Hash64>| families.iter().any(|f| want.is_subset(&f.kernel_ids));
        assert!(holds([Hash64::from_u64_word(1)].into_iter().collect()), "a subset of one family certifies");
        assert!(
            !holds([Hash64::from_u64_word(2), Hash64::from_u64_word(3)].into_iter().collect()),
            "a set spanning two families must not certify — no backend was drilled on that graph"
        );
    }
}
