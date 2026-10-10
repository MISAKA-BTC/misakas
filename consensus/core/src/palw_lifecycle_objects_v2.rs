//! From accepted transactions to claim-lifecycle objects (P0-11).
//!
//! `palw_fp_objects_v3` does this for the one object a free-prompt commitment carries. This does
//! it for the objects that move a claim through the lattice — the panel binding, the licensing,
//! the producer default, and the four court moves — and its absence was a liveness hole rather
//! than a missing feature: with no extractor for `PanelBound`, no block could carry one, so every
//! claim on a V2 network sat `Provisional` until `window_bind` lapsed and voided as
//! `BindTimeout`. `safe_weight` never grew, the safe frontier never left the zero point, and PALW
//! weight — the network's entire fork choice — was permanently zero.
//!
//! # What may ride here, and what deliberately may not
//!
//! The payload is a `PalwConsensusObjectV2` and the extractor accepts a fixed subset of its
//! variants. The exclusions are the point:
//!
//! * **`BondRegistered` may, and the lock is what lets it.** The object DECLARES `collateral`,
//!   and for a long time nothing on this path locked a UTXO behind that number — a transaction
//!   saying "I have staked a million" would have staked a million, and every exposure ceiling,
//!   every slash and Decision 7's whole Sybil bound are denominated in it. So bonds came from the
//!   genesis registration list only.
//!
//!   [`palw_bond_registration_binds_its_carrier_v2`] is that lock, and the extractor calls it on
//!   every registration: the outpoint must be an output of the CARRYING transaction, holding at
//!   least the collateral it declares, paying to the P2PKH of the payload the registration names
//!   as its payee. Nothing is looked up, because nothing needs to be — the output is created by
//!   the transaction carrying the object, so its existence, amount and script are facts block
//!   validation established before this object was decoded. The carrier proves the money; the
//!   signature this list demands proves the owner.
//! * **`ClassRegistered` may, but only carrying what makes it checkable** (ADR-0049 Decision H —
//!   this used to be an outright refusal). A class entering a live chain moves the share table
//!   (ADR-0045 Decision 3 funds an entrant by donation from every incumbent) and brings its own
//!   `pwu_rule`, and nothing checked either — which was the real objection, and it is a statement
//!   about CHECKING rather than about forbidding. So the object must carry its shape profile and
//!   canonical job, and `verify_class_admission_v2` decides at acceptance: coverage over
//!   coordinates, the four cost bounds, the ladder, and the derived-pwu rule the genesis loader
//!   already enforces. A registration WITHOUT that material is still refused here, because there
//!   is nothing to check it with.
//! * **`FreePromptCommitted` may not.** It has its own subnetwork, its own codec and its own
//!   pricing rules; accepting it here would be a second path into one object with one of them
//!   unpriced.
//! * **`PanelBound` may not, and this one is a later decision than the module.** A panel is
//!   `derive_panel_v2` of the anchor block and the bond registry — a pure function of chain state
//!   with nothing for a publisher to choose. Carrying it made someone send an object nobody was
//!   paid to send for a claim that was not theirs, so in practice the producer decided whether
//!   its own claim proceeded (audit C5's tail). The chain derives the binding itself now
//!   (`palw_v2_derived_panel_bindings`), and a carried one would be a second answer to a question
//!   that has one.
//!
//! Everything else advances a claim without minting or locking value, and each kind already has
//! an acceptance check the pipeline runs before the transition folds it
//! (`validate_panel_bound_v2`, `check_court_open_acceptance_v2`, `adjudicate_court_close_v2`, and
//! the two rung signature checks).
//!
//! # Why a malformed carrier is SKIPPED, not fatal
//!
//! Same rule, same reason as the free-prompt walk beside it: transaction-level validity is the
//! transaction validator's job, while this walk must be a total function of whatever DID get
//! accepted. A walk that could panic or reject on a peer-supplied payload would be a remote
//! denial of service wearing a consensus rule's clothes. The skipped list is returned so a caller
//! can log it — a silently dropped carrier is the "reads as nothing" failure ADR-0042 Decision 5
//! warns about.

use crate::palw_state_v2::PalwConsensusObjectV2;
use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use crate::tx::{Transaction, TransactionId};

/// Wire version for a lifecycle carriage payload. A payload naming any other version is skipped,
/// never reinterpreted.
pub const PALW_LIFECYCLE_TX_VERSION_V2: u16 = 1;

/// What one transaction carries: exactly one lifecycle object.
///
/// One object per transaction rather than a batch, so that a malformed member cannot take valid
/// siblings down with it and so the carrier id names precisely one object for attribution.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwLifecycleTxPayloadV2 {
    pub version: u16,
    pub object: PalwConsensusObjectV2,
}

/// An extracted object plus the transaction that carried it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwLifecycleCarrierV2 {
    pub carrier: TransactionId,
    pub object: PalwConsensusObjectV2,
}

/// What one block's lifecycle extraction produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwLifecycleExtractionV2 {
    /// The objects, in acceptance order — the transition's order IS consensus.
    pub objects: Vec<PalwLifecycleCarrierV2>,
    /// Carriers routed here that produced no object, and why.
    pub skipped: Vec<(TransactionId, &'static str)>,
}

/// Whether this object kind may enter a chain through a transaction. See the module doc for why
/// each exclusion is an exclusion.
pub fn palw_lifecycle_object_may_ride_v2(object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    match object {
        PalwConsensusObjectV2::ReceiptLicensed { .. }
        | PalwConsensusObjectV2::ReceiptLicensedV2 { .. }
        // ADR-0160 F-B (tag 59): each window root carries its seat's signature, checked at
        // acceptance against the bond's registered key, like the single licence's receipts.
        | PalwConsensusObjectV2::ReceiptLicensedBatchV1 { .. }
        // ADR-0160 F-Q (tag 60): the auditor's signature is checked at acceptance against its genesis
        // key (lane A's operator rule), like a licence's.
        | PalwConsensusObjectV2::AuditReceiptBatchV1 { .. }
        // ADR-0164 F-M1 (tag 95): each rider's signature is checked at acceptance against its bond's registered key.
        | PalwConsensusObjectV2::AttemptRidersV1 { .. }
        | PalwConsensusObjectV2::OptimisticLicensed { .. }
        | PalwConsensusObjectV2::ProducerDefaulted { .. }
        | PalwConsensusObjectV2::CourtOpened { .. }
        | PalwConsensusObjectV2::CourtClosed { .. }
        | PalwConsensusObjectV2::CourtDisclosed { .. }
        | PalwConsensusObjectV2::CourtVerdictPosted { .. }
        // ADR-0075: certification rides an ordinary transaction. Neither object carries a
        // signature because neither needs one — the evidence is graded by the court in the
        // transition, and the class binding is checked against the class's own profile hash.
        | PalwConsensusObjectV2::FamilyCertified { .. }
        | PalwConsensusObjectV2::ClassLaneCertified { .. }
        // RFC-0002 Phase F (tag 63): the IR class's lane certification, checked the same way.
        | PalwConsensusObjectV2::ClassLaneCertifiedTirV1 { .. }
        | PalwConsensusObjectV2::ObjectChunk { .. }
        // **ADR-0080 design A: the split close.** The declaration carries the signature of one of
        // the two bonds the session id binds, checked at acceptance against that bond's registered
        // key — the same split every other court move uses. A chunk carries none and needs none:
        // the declaration already pinned its bytes at its index, so a chunk that is not the pinned
        // preimage is refused by the transition whoever sent it, and requiring a signature would
        // only stop a stranger from paying to deliver a mover's own evidence.
        | PalwConsensusObjectV2::CourtCloseChunk { .. } => Ok(()),
        // **ADR-0082 Decision 2: the dissection's three moves ride, each carrying its own
        // authorisation.** The same stateless/stateful split every other court move uses — this
        // layer checks that a signature is PRESENT, and whether it is the responder's or the
        // challenger's key is the acceptance layer's, where the registry is in hand. Unsigned,
        // either party could write the other's moves: a challenger writing disclosures binds an
        // honest executor to partial sums it never claimed, and a responder writing choices steers
        // the dissection away from its own divergence.
        //
        // Whether the k-ary court is armed at all is also the acceptance layer's, for the reason
        // this list is stateless: a fence is resolved at a DAA score, and this function does not
        // have one.
        PalwConsensusObjectV2::CourtAttnRootClaimed { signature, .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedAnchored { signature, .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedHeld { signature, .. }
        | PalwConsensusObjectV2::CourtAttnDissected { signature, .. }
        | PalwConsensusObjectV2::CourtAttnChildChosen { signature, .. }
            if !signature.is_empty() =>
        {
            Ok(())
        }
        PalwConsensusObjectV2::CourtAttnRootClaimed { .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedAnchored { .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedHeld { .. }
        | PalwConsensusObjectV2::CourtAttnDissected { .. }
        | PalwConsensusObjectV2::CourtAttnChildChosen { .. } => Err(
            "a fused-attention dissection move must carry the signature of the party it is attributed to — unsigned, either side could write the other's moves",
        ),
        // RFC-0002 Phase F (F7, tags 64–66): the IR history dissection's three moves ride as the
        // ADR-0082 moves do, each carrying its party's signature, checked at acceptance.
        // The root claim's finalize carriage references the registered program: it rides empty.
        PalwConsensusObjectV2::CourtTirRootClaimed { root, .. } if !root.finalize.binding.class.program.is_empty() => {
            Err("an IR root claim's binding carries no program: the chain holds the registered class's")
        }
        PalwConsensusObjectV2::CourtTirRootClaimed { signature, .. }
        | PalwConsensusObjectV2::CourtTirDissected { signature, .. }
        | PalwConsensusObjectV2::CourtTirChildChosen { signature, .. }
            if !signature.is_empty() =>
        {
            Ok(())
        }
        PalwConsensusObjectV2::CourtTirRootClaimed { .. }
        | PalwConsensusObjectV2::CourtTirDissected { .. }
        | PalwConsensusObjectV2::CourtTirChildChosen { .. } => {
            Err("an IR dissection move must carry the signature of the party it is attributed to — unsigned, either side could write the other's moves")
        }
        // RFC-0003 (tag 69): the generative root claim rides as F7's does, signed by its party.
        PalwConsensusObjectV2::CourtGenRootClaimed { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::CourtGenRootClaimed { .. } => {
            Err("a generative root claim must carry the signature of the responder — unsigned, the challenger could write it")
        }
        // The second IR fence: an IR step demand names a step unit and carries its accuser's signature.
        PalwConsensusObjectV2::DefaultAccusedTirStep { accusation } if !accusation.unit.is_tir_fence2_v1() => {
            Err("an IR step demand names a step leaf, a step node or a rows-tree node")
        }
        // RFC-0004 A6: the evaluation root claim rides as the generative one does, signed by its party.
        PalwConsensusObjectV2::CourtEvalRootClaimed { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::CourtEvalRootClaimed { .. } => {
            Err("an evaluation root claim must carry the signature of the responder — unsigned, the challenger could write it")
        }
        PalwConsensusObjectV2::DefaultAccusedTirStep { accusation } if !accusation.signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::DefaultAccusedTirStep { .. } => {
            Err("an IR step demand must carry its accuser's signature — unsigned, anyone could spend a bond's DA budget")
        }
        // Pipeline-claim data availability (tag 83): a pipeline step demand names a pipeline step unit and
        // carries its accuser's signature. Tags 84 and 85 are reserved and uninhabited.
        PalwConsensusObjectV2::DefaultAccusedPipelineStep { accusation } if !accusation.unit.is_pipeline_step_v1() => {
            Err("a pipeline step demand names a step leaf or a step node of a stage")
        }
        PalwConsensusObjectV2::DefaultAccusedPipelineStep { accusation } if !accusation.signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::DefaultAccusedPipelineStep { .. } => {
            Err("a pipeline step demand must carry its accuser's signature — unsigned, anyone could spend a bond's DA budget")
        }
        PalwConsensusObjectV2::ReservedPipelineDa84(never) | PalwConsensusObjectV2::ReservedPipelineDa85(never) => match *never {},
        PalwConsensusObjectV2::CourtCloseDeclared { signature, .. } if !signature.is_empty() => Ok(()),
        // ADR-0087 Decision 3: a buy is bound to its carrier's sink output below; a sell must carry
        // the holder's signature, checked at acceptance against the payload it names.
        PalwConsensusObjectV2::ModelBuy { .. } => Ok(()),
        // ADR-0090: a seed is bound to its carrier's sink output exactly as a buy is.
        PalwConsensusObjectV2::ModelSeed { .. } => Ok(()),
        // ADR-0152-adjacent (Activation Pool): a top-up is bound to its carrier's activation sink
        // below, and an activation sink nothing binds is refused at block validity.
        PalwConsensusObjectV2::ActivationPoolFunded { .. } => Ok(()),
        PalwConsensusObjectV2::ModelSell { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::ModelSell { .. } => Err("a model sell must carry the holder's signature — unsigned, anyone could drain a position"),
        // ADR-0088: every registry object is attributed to a bond and carries that bond's signature,
        // checked at acceptance against the bond's stored key; unsigned, anyone could publish a
        // version, hand a line over or speak in a line's name.
        PalwConsensusObjectV2::ModelLineFounded { signature, name, .. } => {
            if signature.is_empty() {
                Err("a line founding must carry the founder's signature")
            } else if name.is_empty() || name.len() > crate::palw_model_lines_v1::PALW_MODEL_LINE_NAME_MAX_BYTES {
                Err("a line's name must be 1..=64 bytes")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::ModelVersionPublished { signature, .. }
        | PalwConsensusObjectV2::ModelVersionPromoted { signature, .. }
        | PalwConsensusObjectV2::ModelVersionWithdrawn { signature, .. }
        | PalwConsensusObjectV2::ModelLineBenefitsDeclared { signature, .. }
        | PalwConsensusObjectV2::ModelLineRolesSet { signature, .. }
        | PalwConsensusObjectV2::ModelLineOwnerTransferred { signature, .. }
        | PalwConsensusObjectV2::ModelLineRetired { signature, .. }
        | PalwConsensusObjectV2::ModelProposalPosted { signature, .. }
        | PalwConsensusObjectV2::ModelProposalClosed { signature, .. }
        | PalwConsensusObjectV2::ModelEvaluationPosted { signature, .. } => {
            if signature.is_empty() {
                Err("a model registry object must carry the signature of the bond it is attributed to")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::CourtCloseDeclared { .. } => Err(
            "a close declaration must carry the signature of the side it declares for — without one either party could write the other's close and pin it to a verdict it never asserted",
        ),
        // **Audit M-01: a door nobody can authenticate is shut.**
        //
        // `BondRetireRequested { bond }` carried no signature and no owner binding, and a bond key
        // IS a premine outpoint — a public constant. One ordinary transaction from any stranger
        // flipped any bond to `Retiring`, with no inverse; on a network with one producer that is a
        // permanent halt for a transaction fee. `ClassFrozen`'s contradiction certificate has
        // signatures that `check_class_contradiction_shape_v2` explicitly defers to "the acceptance
        // layer", which had no arm for this object — so a forged certificate froze a class forever,
        // and there is deliberately no `ClassUnfrozen`.
        //
        // Both belong on chain eventually and neither can be re-admitted without carrying its own
        // authorization: an owner signature over the bond key, and a contradiction adjudicated by
        // `adjudicate_class_contradiction_v1` (which takes a verifier, and is wired only into the
        // other band today). Until then they are refused here AND at acceptance — one lock is a
        // lock somebody removes while refactoring.
        // **Re-admitted, now that it carries the authorisation the refusal stood in for.**
        //
        // The refusal was right and it was also a permanent capital lock: retirement is the ONLY
        // writer of `Retiring`, `palw_bond_collateral_is_locked_v2` is unconditionally true for an
        // `Active` bond, and the C-08 burn is collected only from a bond the lock has released —
        // so with this door shut, every genesis collateral outpoint is unspendable forever and
        // every slashed sompi freezes instead of being destroyed. "Stake" that can never be
        // withdrawn is not stake.
        //
        // This layer is stateless, so it checks SHAPE only: a retirement must carry a signature.
        // Whether that signature is the bond's own is the acceptance layer's, where the registry
        // is in hand — the same split `ClassRegistered` uses two arms below.
        PalwConsensusObjectV2::BondRetireRequested { signature, .. } if !signature.is_empty() => Ok(()),
        // **A capability declaration rides, and carries its own authorisation** (ADR-0071
        // Decision 3). Same split as retirement: this layer is stateless, so it checks that a
        // signature is PRESENT; whether it is the bond's own is the acceptance layer's, where the
        // registry is in hand. Without one, a relayer could volunteer any bond — a public outpoint
        // — for duty on any class, and the duty accounting convicts the seats the draw names.
        PalwConsensusObjectV2::BondCapabilityDeclared { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::BondCapabilityDeclared { .. } => Err(
            "a capability declaration must carry the owner signature that authorizes it — a bond key is a public outpoint, so without one anyone could volunteer anyone's collateral for duty",
        ),
        PalwConsensusObjectV2::BondRetireRequested { .. } => Err(
            "a bond retirement must carry the owner signature that authorizes it — a bond key is a public outpoint, so without one anyone could retire anyone's bond",
        ),
        PalwConsensusObjectV2::ClassFrozen { .. } => Err(
            "a class freeze carries a contradiction certificate no layer verifies — a forged one freezes a class permanently, and there is no unfreeze",
        ),
        // **ADR-0049 Decision H: a class MAY register post-genesis — gated, not forbidden.**
        //
        // The objection this refusal carried is correct and it is a statement about CHECKING: a
        // class entering a live chain moves the share table and brings its own `pwu_rule`, and
        // nothing checked either. Decisions C and D are that check, so the refusal is replaced by
        // the gate. What rides here is the SHAPE of a checkable registration; whether the graph
        // covers, fits the ladder, costs what the ruleset allows and counts the pwu it declares is
        // `verify_class_admission_v2`'s, at acceptance, where the bundle is in hand.
        PalwConsensusObjectV2::ClassRegistered { admission: Some(_), .. } => Ok(()),
        // RFC-0002 Phase F (tag 61): an IR registration carries its program, so it is checkable
        // whenever it is admitted. It RIDES at every height — a block carrying it must be valid on
        // this build and on an older one that skips it undecoded (A-2) — and the acceptance walk
        // drops it by name until `palw_tir_v1` is armed.
        PalwConsensusObjectV2::ClassRegisteredTirV1 { .. } => Ok(()),
        // RFC-0003 (tag 68): a generative registration carries its pipeline and every program, so it
        // is checkable whenever it is admitted. It RIDES at every height, as the IR registration does
        // (A-2), and the acceptance walk drops it by name until the pipeline admission lands.
        PalwConsensusObjectV2::ClassRegisteredGenV1 { .. } => Ok(()),
        // RFC-0004 (tags 70–82): every improvement object RIDES at every height, as the IR and
        // generative registrations do (A-2) — a block carrying one must be valid on this build and on
        // an older one that skips it undecoded — and the acceptance walk drops it by name until its
        // admission lands past `palw_improvement_v1`. Its signature, where it carries one, is the
        // acceptance layer's, where the registry is in hand.
        PalwConsensusObjectV2::ModelLineImprovementPolicySet { .. }
        | PalwConsensusObjectV2::HardCaseSubmitted { .. }
        | PalwConsensusObjectV2::DataUseOptIn { .. }
        | PalwConsensusObjectV2::SetterSetCommitted { .. }
        | PalwConsensusObjectV2::SetterSetRevealed { .. }
        | PalwConsensusObjectV2::SetterKeysRevealed { .. }
        | PalwConsensusObjectV2::HardCaseKeyRevealed { .. }
        | PalwConsensusObjectV2::DatasetRegistered { .. }
        | PalwConsensusObjectV2::TeachingArtifactCommitted { .. }
        | PalwConsensusObjectV2::TeachingArtifactRevealed { .. }
        | PalwConsensusObjectV2::TeacherLicenceRegistered { .. }
        | PalwConsensusObjectV2::CandidateSubmitted { .. }
        | PalwConsensusObjectV2::LineageHeadRolledBack { .. }
        | PalwConsensusObjectV2::ImprovementPoolFunded { .. } => Ok(()),
        // RFC-0001 §2.10 (tag 94): an adapter class listing rides at every height, as the IR and generative
        // registrations do (A-2); the acceptance walk drops it by name below `palw_adapter_class_v1`, and its
        // form (state-free) is checked here so a malformed listing is refused before any slot is charged.
        PalwConsensusObjectV2::AdapterClassListed { payload, .. } => crate::palw_adapter_class_v1::palw_adapter_listing_shape_v1(payload),
        // G14 lane D (tag 110): a kernel route object rides at every height (A-2) and carries its signer's signature; the bytes'
        // strict decode, the signer's bond and the kernel's own rules are the acceptance layer's and the ledger's. Only the shape is
        // stateless: a signature is present and the kernel's encoding fits the largest carriage.
        PalwConsensusObjectV2::KernelRouteV1 { bytes, signature, .. } => {
            if signature.is_empty() {
                Err("a kernel route object must carry its signer's signature — unsigned, anyone could act as any bond")
            } else if bytes.is_empty() || bytes.len() > crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 {
                Err("a kernel route object's encoding is empty or past the largest carriage")
            } else {
                Ok(())
            }
        }
        // (tag 113, C4 F-C4R3-03): a chunk of the route's own lane carries its opener's signature and one carrier's worth of a part.
        PalwConsensusObjectV2::KernelRouteChunkV1 { chunk, signature } => {
            if signature.is_empty() {
                Err("a kernel route chunk must carry its opener's signature — unsigned, anyone could open a group as any bond")
            } else if chunk.count == 0
                || chunk.count > crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT
                || chunk.index >= chunk.count
                || chunk.bytes.is_empty()
                || chunk.bytes.len() > crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES
            {
                Err("a kernel route chunk's index, count or part is out of range")
            } else {
                Ok(())
            }
        }
        // (tag 111): a seat's receipt carries its signature; everything else is the kernel's structural admission.
        PalwConsensusObjectV2::KernelConstraintReceiptV1 { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::KernelConstraintReceiptV1 { .. } => Err("a kernel constraint receipt must carry the seat's signature"),
        // G14 phase 3 (tags 104-108): onboarding objects ride at every height (A-2) and carry their signer's signature; the bonds, the
        // fences, the rows and the proofs are the acceptance layer's and the fold's. The envelope must wrap a registration.
        PalwConsensusObjectV2::ArtifactBoundV1 { signature, .. }
        | PalwConsensusObjectV2::ArtifactBindingChallengedV1 { signature, .. }
        | PalwConsensusObjectV2::KernelBoundV1 { signature, .. }
        | PalwConsensusObjectV2::ConformanceCommittedV1 { signature, .. }
        | PalwConsensusObjectV2::ConformanceEvidenceV1 { signature, .. }
            if signature.is_empty() =>
        {
            Err("an onboarding object must carry its signer's signature")
        }
        PalwConsensusObjectV2::ArtifactBoundV1 { .. }
        | PalwConsensusObjectV2::ArtifactBindingChallengedV1 { .. }
        | PalwConsensusObjectV2::KernelBoundV1 { .. }
        | PalwConsensusObjectV2::ConformanceCommittedV1 { .. }
        | PalwConsensusObjectV2::ConformanceEvidenceV1 { .. } => Ok(()),
        // DA16 (tags 150–153): provider-court objects ride at every height (A-2) and carry their signer's signature; the fence, the
        // bonds, the rows and the units are the acceptance layer's and the fold's.
        PalwConsensusObjectV2::ProviderLeaseV1 { signature, .. }
        | PalwConsensusObjectV2::ProviderChallengeV1 { signature, .. }
        | PalwConsensusObjectV2::ProviderAnswerV1 { signature, .. }
        | PalwConsensusObjectV2::DaTransferV1 { signature, .. }
            if signature.is_empty() =>
        {
            Err("a provider-court object must carry its signer's signature")
        }
        PalwConsensusObjectV2::ProviderLeaseV1 { .. }
        | PalwConsensusObjectV2::ProviderChallengeV1 { .. }
        | PalwConsensusObjectV2::ProviderAnswerV1 { .. }
        | PalwConsensusObjectV2::DaTransferV1 { .. } => Ok(()),
        PalwConsensusObjectV2::SignedRegistrationV1 { signature, registration, .. } => {
            if signature.is_empty() {
                Err("a signed registration envelope must carry its signer's signature")
            } else if matches!(
                registration.as_ref(),
                PalwConsensusObjectV2::ClassRegistered { .. } | PalwConsensusObjectV2::ClassRegisteredTirV1 { .. }
            ) {
                Ok(())
            } else {
                Err("a signed registration envelope wraps a class registration (ClassRegistered or ClassRegisteredTirV1) and nothing else")
            }
        }
        // RFC-0002 Phase F (tag 62): an IR one-move accusation rides signed and shaped (its proof
        // an IR close about the roots it names) at every height, as the registration does; the
        // ruleset's close ceiling and the verdict are the acceptance layer's.
        PalwConsensusObjectV2::TirShardCourtAccused { accusation } => crate::palw_tir_one_move_v1::palw_tir_one_move_shape_v1(accusation),
        // RFC-0003 §I.4.7: a pipeline claim's one-move accusation rides signed and shaped (its proof a
        // generative close about the execution it names) at every height, as the IR one's does; the court's
        // ceiling and the verdict are the acceptance layer's, and below `palw_gen_v1` the walk drops it by name.
        PalwConsensusObjectV2::GenShardCourtAccused { accusation } => crate::palw_gen_one_move_v1::palw_gen_one_move_shape_v1(accusation),
        // RFC-0003 decision 22: a held leaf challenge rides signed and shaped (one digest per declared chunk, the
        // court's structural bound) at every height, as the one-move accusations do; the ruleset's carriage count
        // and the signature are the acceptance layer's, and below `palw_held_close_chunks_v1` the walk drops it by name.
        PalwConsensusObjectV2::HeldLeafChallengeDeclared { challenge } => {
            crate::palw_held_close_v1::palw_held_leaf_challenge_shape_v1(challenge)
        }
        // RFC-0007 Part I: a vertex rides shaped (the strict leaf order, the caps, the root) at every height, and an equivocation
        // shaped (one seat, one round, two roots), as the held leaf challenge does; the signatures, the clock and the registry are
        // the acceptance layer's, and below `palw_verification_vertex_v1` the walk drops them by name.
        PalwConsensusObjectV2::VerificationVertexV1 { vertex } => {
            crate::palw_vertex_v1::palw_vertex_shape_v1(vertex).map_err(|_| "a verification vertex is malformed (RFC-0007 Part I)")
        }
        PalwConsensusObjectV2::VertexEquivocationV1 { evidence } => {
            crate::palw_vertex_v1::palw_vertex_equivocation_shape_v1(evidence).map_err(|_| "a vertex equivocation is malformed (RFC-0007 Part I)")
        }
        // RFC-0007 Part IV.1: the trap objects ride shaped at every height (the signature's length, the tile count the commitment can
        // bind); the setter's signature, the slot lottery and the windows are the acceptance layer's and the fold's, and below
        // `palw_audit_mesh_v1` the walk drops them by name.
        PalwConsensusObjectV2::TrapCommittedV1 { trap } => {
            crate::palw_mesh_v1::palw_trap_committed_shape_v1(trap).map_err(|_| "a trap commitment is malformed (RFC-0007 Part IV.1)")
        }
        PalwConsensusObjectV2::TrapRevealedV1 { reveal } => {
            crate::palw_mesh_v1::palw_trap_revealed_shape_v1(reveal).map_err(|_| "a trap reveal is malformed (RFC-0007 Part IV.1)")
        }
        // RFC-0010: a certified epoch output rides bounded (its proof is at most `MAX_BEACON_PROOF_BYTES_V1`); whether it verifies
        // against the branch is the fold's, which drops one that does not, the block standing. Below
        // `palw_permissionless_panel_v1` the walk drops it by name.
        PalwConsensusObjectV2::PanelBeaconProofV3 { proof } => {
            if proof.proof.len() > misaka_palw_panel::MAX_BEACON_PROOF_BYTES_V1 as usize {
                Err("a certified Panel epoch output exceeds the proof bound (RFC-0010)")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::ClassRegistered { admission: None, .. } => Err(
            "a class registered on a running chain must carry its shape profile and canonical job —              without them nothing can check its coverage, its ladder depth or its declared pwu",
        ),
        PalwConsensusObjectV2::PanelBound { .. } => {
            Err("the chain derives panel bindings; a carried one would be a second answer to a question with one")
        }
        // **Re-admitted, and the thing the refusal named is now proven by the carrier itself.**
        //
        // The objection was exact: a registration DECLARES collateral, and nothing on this path
        // locked it — so "stake" meant a number in an object. It could not be checked here either,
        // because this layer is stateless and has no UTXO set to look an outpoint up in.
        //
        // So the outpoint is not looked up: it is CREATED. A registration must name an output of
        // its own carrying transaction, which makes existence, value and script something block
        // validation has already established before this object is read —
        // `palw_bond_registration_binds_its_carrier_v2` is what checks that binding, and it needs
        // the transaction, so it runs in the extractor rather than here. What is left for this
        // layer is the shape: a registration must carry the signature that proves the registrant
        // holds the key it declares, since anyone can pay to somebody else's script.
        PalwConsensusObjectV2::BondRegistered { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::BondRegistered { .. } => Err(
            "a bond registration must carry the registrant's signature over the key it declares — the carrier proves the collateral, not the owner",
        ),
        PalwConsensusObjectV2::FreePromptCommitted { .. } => {
            Err("a free-prompt commitment rides its own subnetwork, where its price is checked")
        }
        // RFC-0003 §I.4: a tensor claim's commitment is a free-prompt commitment of job version 10.
        PalwConsensusObjectV2::GenTensorCommitted { .. } => {
            Err("a tensor claim's commitment rides the free-prompt subnetwork, where its price is checked")
        }
        // **ADR-0078: a derivation rides, and carries the executor's authorisation.** Same split
        // as a bond's declarations: this layer is stateless, so it checks SHAPE — the object's
        // version, a non-zero kind, an ML-DSA-87-sized executor key, a non-empty artifact — and
        // that a signature is present. Whether the signature is the declared key's is the
        // acceptance layer's; whether the declared key is the claim's bond key is the
        // transition's, where the registry and the claim table are in hand.
        //
        // **And the signature's LENGTH is pinned here, because this is the only layer that can
        // pin it** (audit 2026-09-02, X1). "Present" was the whole rule, which made the one
        // object that must never carry bytes the only lifecycle object with a free-length byte
        // field — and this list is a BLOCK rule (`tx_validation_in_isolation`) while the
        // acceptance layer merely drops an object and lets the block stand, so a refusal there
        // would leave the bytes in the chain. Pinned, a derivation's wire size is a constant.
        PalwConsensusObjectV2::DerivedArtifactV1 { signature, .. } if signature.is_empty() => Err(
            "a derived artifact must carry the claim executor's signature — without one anyone could put their name on a derivation of anyone's answer",
        ),
        PalwConsensusObjectV2::DerivedArtifactV1 { object, signature } => {
            crate::palw_derived_v1::check_derived_carriage_v1(object, signature)
        }
        // **ADR-0062 SA-1/SA-2: both DA-court objects ride, and both carry their own
        // authorisation.** Same stateless/stateful split as retirement: this layer checks that a
        // signature is PRESENT and that a disclosure fits the close ceiling; whether the signature
        // is the accuser's bond key or the claim's producer key is the acceptance layer's, where
        // the registry and the claim are in hand.
        //
        // Carriage is deliberately permissionless for the disclosure (SA-3): the object is signed
        // by the producer's bond, so who carried it is irrelevant, and that is precisely what makes
        // suppressing it cost an attacker every producer for a whole window instead of one.
        // **ADR-0099 Decision 5 / ADR-0100: the one-move court's accusation rides signed and
        // bounded.** The signature is the accuser's bond key, checked at acceptance against the
        // registry; the refutation and the openings answer to the same close ceiling a disclosure
        // does, for the same reason, and the ruleset's own ceiling is applied at acceptance, where
        // the bundle is. Whether the court is armed is the acceptance layer's, as for every fence.
        PalwConsensusObjectV2::ShardCourtAccused { accusation } if accusation.signature.is_empty() => Err(
            "a one-move accusation must carry the accuser's signature — a bond key is a public outpoint, so without one anyone could accuse under a stranger's identity",
        ),
        PalwConsensusObjectV2::ShardCourtAccused { accusation } => {
            if crate::palw_shard_court_v1::palw_shard_court_accusation_bytes_v1(accusation) > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES {
                return Err("a one-move accusation is above the close-byte ceiling this ruleset prices");
            }
            Ok(())
        }
        // **ADR-0100 Decision 4.** The plan and a bond's shard list ride signed — by the class's
        // registrant and by the bond, checked at acceptance against their registered keys — and
        // shaped: a count in range, a list inside it. A part rides unsigned like `ReceiptLicensed`
        // (its receipts are the authority) and bounded: at least one receipt, at most a shard's
        // worth under any ruleset, a shard inside its plan. Whether per-shard licensing is armed is
        // the acceptance layer's, for the reason the k-ary court's moves give.
        PalwConsensusObjectV2::ClassShardPlanDeclared { signature, .. } if signature.is_empty() => {
            Err("a shard plan must carry its registrant's signature")
        }
        PalwConsensusObjectV2::ClassShardPlanDeclared { shard_count, .. } => {
            if crate::palw_shard_licensing_v1::palw_shard_count_in_range_v1(*shard_count) {
                Ok(())
            } else {
                Err("a shard plan's count is outside the range the chain accepts")
            }
        }
        PalwConsensusObjectV2::BondShardsDeclared { signature, .. } if signature.is_empty() => {
            Err("a bond's shard list must carry the bond's signature")
        }
        PalwConsensusObjectV2::BondShardsDeclared { shard_count, shards, .. } => {
            crate::palw_shard_licensing_v1::palw_bond_shards_shape_v1(*shard_count, shards)
        }
        PalwConsensusObjectV2::ShardReceiptLicensed { part } => {
            if part.receipts.is_empty() || part.receipts.len() > crate::palw_shard_licensing_v1::PALW_SHARD_PART_MAX_RECEIPTS_V1 {
                return Err("a shard part carries between one receipt and a shard's worth");
            }
            if !crate::palw_shard_licensing_v1::palw_shard_count_in_range_v1(part.shard_count) || part.shard_index >= part.shard_count
            {
                return Err("a shard part names a shard outside a plan the chain accepts");
            }
            Ok(())
        }
        // **RFC-0006 (tags 91–93): the layer-sharded panels' objects ride at every height** (A-2: a block carrying one must be
        // valid on this build and on an older one that skips it undecoded), signed where they name a party and shaped: a plan
        // carries its registrant's signature and an `(S_L, S_P)` the chain's shape rule accepts (the class-relative bound, the
        // seat budget, is the fold's, where the program is in hand); a part carries between one receipt and a shard's worth,
        // each for the shard it names; a readiness proof carries its seat's signature and opens something. Whether the fence is
        // armed is the acceptance layer's, for the k-ary court's reason.
        PalwConsensusObjectV2::TirShardPlanDeclared { signature, .. } if signature.is_empty() => {
            Err("a layer-shard plan must carry its registrant's signature")
        }
        PalwConsensusObjectV2::TirShardPlanDeclared { s_l, s_p, .. } => {
            if !(2..=crate::palw_tir_shard_v1::PALW_TIR_SHARD_MAX_SHARDS_V1).contains(s_l) {
                Err("a layer-shard plan's shard count is outside the range the chain accepts")
            } else if *s_p != crate::palw_tir_shard_v1::PALW_TIR_SHARD_POSITION_SEGMENTS_LAYERS_ONLY_V1
                && *s_p != crate::palw_tir_shard_v1::PALW_TIR_SHARD_POSITION_SEGMENTS_S1_V1
            {
                Err("a layer-shard plan's position segments are one (layers only) or s_shard - 1")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::TirShardReceiptLicensed { part } => {
            if part.receipts.is_empty() || part.receipts.len() > crate::palw_tir_shard_v1::PALW_TIR_SHARD_PART_MAX_RECEIPTS_V1 {
                return Err("a layer-shard part carries between one receipt and a shard's worth");
            }
            if part.shard >= crate::palw_tir_shard_v1::PALW_TIR_SHARD_MAX_SHARDS_V1 || part.receipts.iter().any(|r| r.shard != part.shard) {
                return Err("a layer-shard part's receipts are each for the shard it names");
            }
            Ok(())
        }
        PalwConsensusObjectV2::TirSeatReadinessProved { signature, proof, .. } => {
            if signature.is_empty() {
                Err("a shard possession proof must carry the seat's signature — unsigned, a relayer could volunteer another bond's collateral")
            } else if proof.opened.is_empty() {
                Err("a possession proof that opens nothing shows nothing")
            } else if proof.opened.len() > crate::palw_model_registry_v1::PALW_READINESS_V2_CHUNKS_V1 as usize
                || proof.operand_bytes() > crate::palw_model_registry_v1::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1
            {
                Err("a shard possession proof opens the challenged leaves within the operand cap")
            } else {
                Ok(())
            }
        }
        // **ADR-0103: the held regime's three objects ride signed and bounded**, for the shard
        // court's reasons: the signature is checked at acceptance against the registered key (the
        // accuser's, or the claim's producer's for an answer), each object answers to the close
        // ceiling a court close does, and whether the regime is armed is the acceptance layer's.
        PalwConsensusObjectV2::CheckpointAccused { accusation } if accusation.signature.is_empty() => Err(
            "a checkpoint accusation must carry the accuser's signature — a bond key is a public outpoint, so without one anyone could accuse under a stranger's identity",
        ),
        PalwConsensusObjectV2::CheckpointAccused { accusation } => {
            if crate::palw_checkpoint_court_v1::palw_checkpoint_court_accusation_bytes_v1(accusation) > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES {
                return Err("a checkpoint accusation is above the close-byte ceiling this ruleset prices");
            }
            Ok(())
        }
        PalwConsensusObjectV2::DefaultAccusedHeld { accusation } if accusation.signature.is_empty() => {
            Err("a held data-availability accusation must carry the accuser's signature")
        }
        PalwConsensusObjectV2::DefaultAccusedHeld { accusation } => {
            if crate::palw_held_da_v1::palw_held_da_bytes_v1(accusation.as_ref()) > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES {
                return Err("a held data-availability accusation is above the close-byte ceiling this ruleset prices");
            }
            Ok(())
        }
        // ADR-0125 §7.3: the evidence's own shape; the grant, the key and the lane are acceptance's.
        PalwConsensusObjectV2::RoundPermitEquivocated { evidence } => {
            evidence.validate_shape().map_err(|_| "round equivocation evidence is malformed: two different signed blocks for one permit")
        }
        PalwConsensusObjectV2::ObjectiveOffence { evidence, .. } => {
            if evidence.is_empty() {
                Err("an objective offence must carry evidence (ADR-0144 §9)")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } if disclosure.signature.is_empty() => {
            Err("a held disclosure must carry the producer's signature — unsigned, a third party could bind the producer to material it never published")
        }
        PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } => {
            if crate::palw_held_da_v1::palw_held_da_bytes_v1(disclosure.as_ref()) > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES {
                return Err("a held disclosure is above the close-byte ceiling this ruleset prices");
            }
            Ok(())
        }
        PalwConsensusObjectV2::DefaultAccused { signature, .. } if !signature.is_empty() => Ok(()),
        PalwConsensusObjectV2::DefaultAccused { .. } => Err(
            "a data-availability accusation must carry the accuser's signature — a bond key is a public outpoint, so without one anyone could accuse under a stranger's identity",
        ),
        PalwConsensusObjectV2::MaterialDisclosed { disclosure, signature, .. } if !signature.is_empty() => {
            // The ceiling a close is priced at, applied to the whole disclosure for the reason the
            // ADR measures: a flat class's every row at a Qwen-class vocabulary is 607,744 bytes,
            // 7.4× the whole budget, so an unbounded disclosure would be a block-sized object
            // nobody priced. The ruleset's own ceiling is checked at acceptance, where the bundle is.
            let bytes = borsh::to_vec(disclosure).map(|b| b.len()).unwrap_or(usize::MAX);
            if bytes > crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES as usize {
                return Err("a data-availability disclosure is above the close-byte ceiling this ruleset prices");
            }
            Ok(())
        }
        PalwConsensusObjectV2::SeatReadinessProved { signature, opening, .. } => {
            if signature.is_empty() {
                Err("a readiness proof must carry the seat's signature — unsigned, a relayer could volunteer another bond's collateral")
            } else if opening.operand.bytes.len() > crate::palw_model_registry_v1::PALW_READINESS_OPENING_MAX_BYTES_V1 {
                Err("a readiness proof opens one leaf within the opening cap")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::SeatReadinessProvedV2 { signature, proof, .. } => {
            let legacy_full_challenge = crate::palw_model_registry_v1::palw_readiness_v2_is_legacy_full_challenge_v1(
                proof.opened.len(),
                proof.operand_bytes(),
            );
            if signature.is_empty() {
                Err("a possession proof must carry the seat's signature — unsigned, a relayer could volunteer another bond's collateral")
            } else if proof.operand_bytes() > crate::palw_model_registry_v1::PALW_READINESS_V2_OPERAND_MAX_BYTES_V1 && !legacy_full_challenge {
                Err("a V2 possession proof opens the challenged leaves within the operand cap")
            } else if proof.opened.is_empty() {
                Err("a possession proof that opens nothing shows nothing")
            } else if proof.opened.len() > crate::palw_model_registry_v1::PALW_READINESS_V2_CHUNKS_V1 as usize {
                // **The width is the challenge's, so anything wider is malformed whatever the state
                // says** (found by the 2026-09-18 diff audit). Without this the verifier walked an
                // attacker's list before the transition compared it to the challenge: the operand cap
                // bounds BYTES, and an entry whose operand carries none costs nothing, so a proof
                // could ask for millions of `blake2b` walks per object. The challenge never names
                // more than `PALW_READINESS_V2_CHUNKS_V1` leaves, and a malformed object is refused
                // where every node can see it is malformed.
                Err("a V2 possession proof opens more leaves than any challenge names")
            } else if proof.siblings.len() > 64 * crate::palw_model_registry_v1::PALW_READINESS_V2_CHUNKS_V1 as usize {
                // A tree of `u32` leaves is 32 levels, so one leaf needs at most 32 siblings and the
                // whole challenge at most 32 × k. Sixty-four × k is twice that: generous, and finite.
                Err("a V2 possession proof carries more siblings than a thirty-two-level tree can need")
            } else if !legacy_full_challenge && proof.opened.iter().any(|(_, operand)| {
                operand.bytes.len() > crate::palw_model_registry_v1::PALW_READINESS_V2_LEAF_MAX_BYTES_V1
            }) {
                // **A leaf no carrier can hold is malformed wherever it is read** (the 2026-09-20
                // measurement). The stateful rule decides WHICH leaves this span's challenge bought;
                // this one decides that none of them is a row the transport could never take.
                Err("a V2 possession proof opens a leaf above the largest a proof may carry")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::ClassManifestV2 { signature, artifact_bytes, .. } => {
            if signature.is_empty() {
                Err("a class manifest must carry the registrant's signature — unsigned, anyone could restate a class's bytes")
            } else if *artifact_bytes == 0 {
                Err("a class manifest names the artifact's bytes; zero is not a thing anyone keeps")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::MaterialDisclosed { .. } => Err(
            "a data-availability disclosure must carry the producer's signature — unsigned, a third party could bind a producer to material it never published",
        ),
        // **ADR-0152 v3.1 R-3 (S-7): a reporter's commitment must carry the reporter's signature**
        // — a bond key is a public outpoint, and an unsigned commitment filed under a stranger's
        // bond would take one of its 64 open slots. The shape rule the skeleton left to S-7, added
        // before testnet-12 launches — no live chain holds a v22 object (testnet-11's v20 layout
        // does not decode tag 53) — so it forks nothing; the acceptance layer then verifies the
        // signature against the bond's key. A reveal (tag 54) carries none by design: anyone may
        // carry it, the salt is the secret.
        PalwConsensusObjectV2::ReporterCommitted { signature, .. } if signature.is_empty() => Err(
            "a reporter commitment must carry the reporter's signature — a bond key is a public outpoint, so without one anyone could spend a stranger's commitment slots",
        ),
        // **ADR-0152 v3.1 §6 row 24, the v22 skeleton: tags 53–56 ride, and fold nothing** until
        // their owners land the rule. They decode now, so this stateless table decides whether a
        // carrier is block-valid; it admits them, and the acceptance layer
        // (`palw_v2_validate_objects`) drops each not landed by name while the fold refuses it — a
        // block carrying one stands, as a block carrying any object the chain refuses at acceptance
        // does. The remaining shape rules (a bounded receipt list for 56, the close ceiling for 55)
        // are their owners' to add with the rule (M3, S-5), before M5 freezes the layout; a shape
        // refused here is a BLOCK rule, and adding one to a live chain would be a fork, so none is
        // guessed at now.
        //
        // M3 (DA-4) adds tag 55's one stateless rule, the one `MaterialDisclosed` has: the answer
        // carries its discloser's signature (the acceptance layer verifies it; the close ceiling,
        // the unit and the discloser's standing are state, and stay there).
        PalwConsensusObjectV2::MaterialDisclosedV2 { signature, .. } => {
            if signature.is_empty() {
                Err("a data-availability answer must carry its discloser's signature — unsigned, anyone could bind a locked signer to an answer it never gave")
            } else {
                Ok(())
            }
        }
        PalwConsensusObjectV2::ReporterCommitted { .. }
        | PalwConsensusObjectV2::ReporterRevealed { .. }
        | PalwConsensusObjectV2::PanelUnavailableQuorum { .. } => Ok(()),
    }
}

/// One chain block's accepted lifecycle transactions, as consensus objects in acceptance order.
///
/// A pure function of the transactions: no chain state is read here. Everything that needs state
/// — that the claim exists and is in the right phase, that the panel is the one this chain
/// derives, that a court close's proof adjudicates — is the acceptance layer's and the
/// transition's, exactly as it is for the free-prompt walk.
/// **A bond registration must name an output of its own carrier, and that output must be the
/// collateral it declares.**
///
/// This is what replaces "bonds come from genesis". A registration used to declare a `bond`
/// outpoint and a `collateral` amount that no layer could check: the stateless ride list has no
/// UTXO set, and the acceptance validator is handed PALW state rather than the UTXO diff. Both
/// facts are still true — so the outpoint is not looked up anywhere. It is created by the
/// transaction carrying the registration, which means existence, amount and script are things
/// block validation established before this object was ever decoded.
///
/// The script must be the P2PKH-ML-DSA-87 of the payload the registration names as its payee, so
/// the collateral is reclaimable by exactly whoever the rewards are. Together with the signature
/// the ride list demands, that is the pair the audit asked for: the carrier proves the money, the
/// signature proves the owner.
pub fn palw_bond_registration_binds_its_carrier_v2(tx: &Transaction, object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    let PalwConsensusObjectV2::BondRegistered { bond, collateral, payout_payload, .. } = object else {
        return Ok(());
    };
    // **Named by index, with a zero id.** The output really must belong to the carrying
    // transaction — that is what makes the money a fact rather than a claim — but the registration
    // cannot NAME the carrier by id: the object travels in the payload, and `write_transaction`
    // folds the payload into the id, so an outpoint naming its own carrier is a hash fixed point.
    // A registrant would have to find a payload containing the id of the transaction that payload
    // produces. The zero id is "this carrier", and the chain substitutes the id it observes
    // (`palw_bond_registration_keyed_to_its_carrier_v2`). The index is checked against the outputs
    // below, so "belongs to this transaction" is enforced exactly as before.
    if bond.0.transaction_id != TransactionId::default() {
        return Err("a bond registration must name its collateral output by index, with a zero transaction id");
    }
    let Some(output) = tx.outputs.get(bond.0.index as usize) else {
        return Err("a bond registration names an output its carrier does not have");
    };
    if output.value < *collateral {
        return Err("a bond registration declares more collateral than the output it names holds");
    }
    // The same script the chain will pay this bond's rewards to. Two things follow from that
    // choice: the collateral is reclaimable by whoever the rewards are reclaimable by, and the
    // registration cannot lock money behind a script it did not also name as its own payee.
    let owner: [u8; 64] = *payout_payload.as_byte_slice();
    if output.script_public_key != crate::mldsa87_primitives::p2pkh_mldsa87_spk(&owner) {
        return Err("a bond's collateral output must pay to the payload the registration names as its payee");
    }
    Ok(())
}

/// ADR-0175: recognized historical objects that would mutate an existing model definition.
/// Keep their wire tags and shape checks. Acceptance and the pure fold ask this same policy
/// before rent, signatures or state writes once the independent immutable fence is active.
pub fn palw_model_definition_update_v1(object: &PalwConsensusObjectV2) -> Option<&'static str> {
    match object {
        PalwConsensusObjectV2::ModelVersionPublished { .. } => Some("ModelVersionPublished"),
        PalwConsensusObjectV2::ModelVersionPromoted { .. } => Some("ModelVersionPromoted"),
        PalwConsensusObjectV2::ModelVersionWithdrawn { .. } => Some("ModelVersionWithdrawn"),
        PalwConsensusObjectV2::LineageHeadRolledBack { .. } => Some("LineageHeadRolledBack"),
        PalwConsensusObjectV2::ModelLineBenefitsDeclared { tiers, .. }
            if tiers.iter().any(|t| t.grants & crate::palw_model_benefits_v1::grant::EARLY_VERSION != 0) =>
        {
            Some("EARLY_VERSION")
        }
        _ => None,
    }
}

/// **The bond key a registrant can actually SIGN.**
///
/// A carried registration names its collateral output by index with a zero transaction id, because
/// the carrier's id is a function of the payload the signature goes into — see
/// [`palw_bond_registration_binds_its_carrier_v2`]. So the signature is made over the zero form,
/// and every verifier has to rebuild that same form rather than the substituted one.
///
/// One function, two call sites — the extractor's substitution and the block validator's signature
/// check — because a registrant and a verifier that disagreed about which bytes were signed would
/// reject every honest registration with "not signed by the key it declares".
/// **ADR-0087 Decision 3: a buy's carrier pays `msk_in` to the class's sink**, at the output the
/// object names, or the object does not ride. The value is read off the carrier and never off
/// the object alone, so the reserve the fold credits is MSK that left a spendable output.
pub fn palw_model_buy_binds_its_carrier_v1(tx: &Transaction, object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    // ADR-0090: a seed is bound the same way — the whole seed sits in the line's sink.
    let (line_id, msk, sink_index, what) = match object {
        PalwConsensusObjectV2::ModelBuy { line_id, msk_in, sink_index, .. } => (line_id, msk_in, sink_index, "buy"),
        PalwConsensusObjectV2::ModelSeed { line_id, msk_seed, sink_index, .. } => (line_id, msk_seed, sink_index, "seed"),
        _ => return Ok(()),
    };
    let Some(output) = tx.outputs.get(*sink_index as usize) else {
        return Err(if what == "buy" {
            "a model buy names a sink output its carrier does not have"
        } else {
            "a model seed names a sink output its carrier does not have"
        });
    };
    if output.value != *msk {
        return Err(if what == "buy" {
            "a model buy declares an MSK leg its sink output does not hold"
        } else {
            "a model seed declares an MSK seed its sink output does not hold"
        });
    }
    if crate::palw_model_market_v1::palw_model_sink_class_v1(&output.script_public_key) != Some(*line_id) {
        return Err(if what == "buy" {
            "a model buy's sink output must be the class's own sink script"
        } else {
            "a model seed's sink output must be the line's own sink script"
        });
    }
    Ok(())
}

/// **ADR-0152-adjacent (Activation Pool): a top-up's carrier pays `amount` to the class's
/// activation sink**, at the output the object names, or the object does not ride. The block rule
/// ([`crate::palw_activation_pool_v1::palw_activation_sink_binding_refusal_v1`]) already refused
/// every activation sink no object binds; this is the other direction — an object naming an output
/// that is not that sink with that value credits nothing.
pub fn palw_activation_pool_binds_its_carrier_v1(tx: &Transaction, object: &PalwConsensusObjectV2) -> Result<(), &'static str> {
    let PalwConsensusObjectV2::ActivationPoolFunded { class_id, amount, sink_index } = object else {
        return Ok(());
    };
    let Some(output) = tx.outputs.get(*sink_index as usize) else {
        return Err("an activation top-up names a sink output its carrier does not have");
    };
    if output.value != *amount {
        return Err("an activation top-up declares an amount its sink output does not hold");
    }
    if crate::palw_activation_pool_v1::palw_activation_sink_class_v1(&output.script_public_key) != Some(*class_id) {
        return Err("an activation top-up's sink output must be the class's own activation sink");
    }
    Ok(())
}

/// **The 2026-09-23 Position route matrix, P-B1: who a refused carrier buy or seed is paid back
/// to** — the P2PKH-ML-DSA-87 payload of the carrier's first output that pays one, and `None` for
/// any other object, a zero amount, or a carrier with no such output.
///
/// The EVM lane refunds the account that signed the action; the carrier lane's equivalent is the
/// script the payer's own signature sent the change to. Every carrier the shipped tools build puts
/// that change at output 0, to the first input's script (`build_palw_lifecycle_tx_with_outputs`,
/// `build_palw_lifecycle_tx_multi`), and the sink is an `OP_RETURN` that can never read as a payee.
/// It is read off the OUTPUTS, which every input signs under `SIG_HASH_ALL`, rather than off the
/// object's `holder` or `seeder`: those name who the move was FOR — a buy may be a gift — and a
/// relay cannot change either the outputs or who signed them.
///
/// Called only where the object already rode: the carrier binding above pinned `amount` to the
/// sink output, so the refund is exactly the sompi the sink took.
pub fn palw_model_carrier_refund_v1(
    tx: &Transaction,
    object: &PalwConsensusObjectV2,
) -> Option<crate::palw_state_v2::PalwCarrierRefundV1> {
    let (line_id, amount) = match object {
        PalwConsensusObjectV2::ModelBuy { line_id, msk_in, .. } => (*line_id, *msk_in),
        PalwConsensusObjectV2::ModelSeed { line_id, msk_seed, .. } => (*line_id, *msk_seed),
        // ADR-0152-adjacent (Activation Pool): a refused top-up goes back the same way, named by the
        // class it was for (the refund row's `line_id` field is a log label; the row is keyed by
        // the carrier).
        PalwConsensusObjectV2::ActivationPoolFunded { class_id, amount, .. } => (*class_id, *amount),
        // RFC-0004 (spec 17 §17.11.1): a deposit the fold refuses goes back the same way.
        PalwConsensusObjectV2::ImprovementPoolFunded { payload } => (payload.line_id, payload.amount),
        _ => return None,
    };
    if amount == 0 {
        return None;
    }
    let payee = tx.outputs.iter().find_map(|output| crate::mldsa87_primitives::p2pkh_mldsa87_payload(&output.script_public_key))?;
    Some(crate::palw_state_v2::PalwCarrierRefundV1 { carrier: tx.id(), line_id, payee, amount })
}

pub fn palw_bond_registration_signed_key_v2(bond: &crate::palw_state_v2::PalwBondKeyV2) -> crate::palw_state_v2::PalwBondKeyV2 {
    crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(TransactionId::default(), bond.0.index))
}

/// Key a carried bond registration to the transaction that carried it.
///
/// Every other object passes through unchanged: this substitution exists only because a bond names
/// an output of its own carrier, and only a carrier knows its own id.
pub fn palw_bond_registration_keyed_to_its_carrier_v2(carrier: TransactionId, object: PalwConsensusObjectV2) -> PalwConsensusObjectV2 {
    match object {
        PalwConsensusObjectV2::BondRegistered {
            bond,
            pubkey,
            operator_pubkey,
            collateral,
            payout_payload,
            capable_classes,
            signature,
        } => PalwConsensusObjectV2::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(carrier, bond.0.index)),
            pubkey,
            operator_pubkey,
            collateral,
            payout_payload,
            capable_classes,
            signature,
        },
        other => other,
    }
}

pub fn palw_lifecycle_objects_from_accepted_txs_v2(txs: &[Transaction]) -> PalwLifecycleExtractionV2 {
    let mut out = PalwLifecycleExtractionV2::default();
    for tx in txs {
        if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
            continue;
        }
        let id = tx.id();
        let payload: PalwLifecycleTxPayloadV2 = match borsh::from_slice(&tx.payload) {
            Ok(payload) => payload,
            Err(_) => {
                out.skipped.push((id, "payload does not decode"));
                continue;
            }
        };
        if payload.version != PALW_LIFECYCLE_TX_VERSION_V2 {
            out.skipped.push((id, "payload names an unsupported wire version"));
            continue;
        }
        if let Err(reason) = palw_lifecycle_object_may_ride_v2(&payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        if let Err(reason) = palw_bond_registration_binds_its_carrier_v2(tx, &payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        if let Err(reason) = palw_model_buy_binds_its_carrier_v1(tx, &payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        if let Err(reason) = palw_activation_pool_binds_its_carrier_v1(tx, &payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        // RFC-0004 (spec 17 §17.11.1): a deposit is bound to its carrier's improvement sink.
        if let Err(reason) = crate::palw_improve_pool_v1::palw_improvement_pool_binds_its_carrier_v1(tx, &payload.object) {
            out.skipped.push((id, reason));
            continue;
        }
        // The chain supplies the half the registrant could not: the outpoint the bond is keyed
        // under from here on is a real one, so state, exposure and `--palw-producer-bond` all name
        // the same output the collateral sits in.
        let object = palw_bond_registration_keyed_to_its_carrier_v2(id, payload.object);
        out.objects.push(PalwLifecycleCarrierV2 { carrier: id, object });
    }
    out
}

/// Why a lifecycle carrier was refused at ADMISSION. Distinct from the walk's `skipped` strings
/// because a rejection is a fact about a transaction and has to name itself in a block-rule
/// error, while a skip is a note in a log.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwLifecycleTxError {
    #[error("the payload does not decode as a lifecycle carriage")]
    Undecodable,
    #[error("the payload names wire version {got}, not {expected}")]
    UnsupportedVersion { got: u16, expected: u16 },
    #[error("this object kind may not ride a transaction: {0}")]
    ObjectMayNotRide(&'static str),
}

/// **Transaction-level admission for [`SUBNETWORK_ID_PALW_LIFECYCLE`] (0x4b).**
///
/// The module doc above explains what the extractor is for; this is the gate that lets a carrier
/// reach it. Without it the id was defined, tested and unreachable: `check_transaction_subnetwork`
/// had no arm for 0x4b, so every lifecycle transaction was `SubnetworksDisabled` at admission and
/// the liveness hole the extractor was written to close stayed exactly as open as before — a
/// claim still could not be licensed, no court move could be filed, and PALW weight was still
/// permanently zero. An extractor with no door in front of it extracts nothing.
///
/// The rules are the walk's own, in the walk's order, so admission and extraction cannot
/// disagree: the may-ride table. Everything past that — that the claim exists, that it is in the
/// right phase, that a court close adjudicates — is stateful and stays where it is, in the
/// transition and its acceptance checks.
///
/// **A-2 (mainnet audit 2026-09-11): a payload this build cannot decode, or names a wire version
/// it does not know, is TOLERATED here, not rejected as block-invalid** — but only on a ruleset
/// that has DECLARED the audit fence (`tolerate_undecodable`, the caller's
/// `Params::palw_audit_2026_09_11_fence().is_some()`). The extraction walk
/// (`palw_lifecycle_objects_from_accepted_txs_v2`) SKIPS both cases — it folds nothing for them —
/// so a block carrying such a carrier is perfectly valid; only this isolation gate said otherwise.
/// That split every rolling upgrade that appends a lifecycle object kind (or bumps the version):
/// the newer build appends the variant behind a dormant fence and folds nothing for it, while the
/// older build failed the whole block at `borsh::from_slice` — a chain split for one transaction,
/// invisible to the fork-id gate (both builds advertise the same identity). Tolerating makes the
/// two builds agree (both skip), the only forward-compatible reading: a byte string this build
/// cannot parse is not a statement this build can call invalid. A payload that DOES decode at the
/// current version is held to the may-ride table regardless, exactly as before.
///
/// **Why ruleset-presence and not a height** (`validate_tx_in_isolation` is context-free by
/// contract, holds no DAA, and this gate is reached from block-body validation as well as the
/// mempool): a ruleset that scheduled the audit fence tolerates the undecodable SHAPE from the
/// moment it ships that preset, and a ruleset that did not — every build in the field today —
/// refuses it exactly as it always has. The `is_some()` reading is the 0x30/0x31 token-band and
/// `PanelDa` shape (`params.rs`, `palw_panel_da_admissible`): admitting the shape is part of the
/// coordinated release, while the *effect* the shape would have is inert here anyway (the fold
/// skips it at every height). **Residual:** on a fence-scheduled network the shape is admitted
/// before the fence's own height, so between ship time and the height an adversary-crafted
/// undecodable carrier is block-valid here where an un-upgraded peer still refuses it; the fold
/// does nothing with it either way, and the peer is gated out for real at the height. A stricter
/// height gate would need the DAA threaded into this context-free path (the same admission-layer
/// threading B-4 defers).
pub fn validate_palw_lifecycle_tx(payload: &[u8], tolerate_undecodable: bool) -> Result<(), PalwLifecycleTxError> {
    let payload: PalwLifecycleTxPayloadV2 = match borsh::from_slice(payload) {
        Ok(payload) => payload,
        // Unknown/appended object tag (or trailing bytes a newer build wrote): the extraction walk
        // skips it, so it must not fail the block here — on a ruleset that opted into the audit.
        Err(_) if tolerate_undecodable => return Ok(()),
        Err(_) => return Err(PalwLifecycleTxError::Undecodable),
    };
    if payload.version != PALW_LIFECYCLE_TX_VERSION_V2 {
        // A wire version this build does not know: extraction skips it, so tolerate it too — on an
        // audit-armed ruleset; below that, refuse it exactly as every build in the field does.
        if tolerate_undecodable {
            return Ok(());
        }
        return Err(PalwLifecycleTxError::UnsupportedVersion { got: payload.version, expected: PALW_LIFECYCLE_TX_VERSION_V2 });
    }
    // **A-2 uniformity (the A2U review): a kind the live testnet-12 build (int-12) cannot decode is judged here as that build judges
    // its bytes — undecodable — at every height**, because this gate holds no height and the kind's owning fence is a height. Tolerated
    // where the ruleset tolerates undecodable payloads (testnet-12 does; `Params::validate_palw_lifecycle_kind_fences_v1` makes that a
    // prerequisite of arming any owning fence), refused as `Undecodable` where it does not. The kind's own stateless rule (the may-ride
    // arms below) is asked past its fence, at the containing block's DAA, by [`validate_palw_lifecycle_tx_in_context_v1`].
    if let PalwLifecycleKindOwnerV1::Fence(_) = palw_lifecycle_kind_owner_v1(&payload.object) {
        return if tolerate_undecodable { Ok(()) } else { Err(PalwLifecycleTxError::Undecodable) };
    }
    palw_lifecycle_object_may_ride_v2(&payload.object).map_err(PalwLifecycleTxError::ObjectMayNotRide)
}

// =================================================================================================================================
// A-2 uniformity: every kind the live build cannot decode is owned by a fence, and rides unjudged below it (the A2U review, 2026-10-08)
// =================================================================================================================================
//
// testnet-12 declares the audit fence, so under A-2 a lifecycle payload the live build (int-12, `rcore/int-12` @ `0b1c11b87`) cannot
// decode is TOLERATED: the block stands and the walk skips the carrier. A newer build that DECODES the payload and then refuses some of
// its bytes (a may-ride arm, a shape check, a size bound) marks invalid a block the live build accepts — a consensus split the moment a
// mixed fleet exists, before any fence. And a newer build that decodes and FOLDS, charges or counts the object below its fence moves a
// root, a coinbase or a per-block cap the live build does not. So, for every kind int-12 does not know:
//
// * **below its owning fence** the bytes are read exactly as int-12 reads them — undecodable: the same block verdict (isolation,
//   [`validate_palw_lifecycle_tx`]), the same skip (the processor's objects-of-block walk drops it before the acceptance walk sees it,
//   and a chunk group assembling to it is undecodable to the walk and to the fold), no charge, no budget, no state write;
// * **past it** the kind's own rules apply (the may-ride arm in the header context, the acceptance walk, the gate and the fold).
//
// [`palw_lifecycle_kind_owner_v1`] is the one table, as an exhaustive `match` with no wildcard: a new `PalwConsensusObjectV2` variant
// does not compile until it names its owner, and `every_kind_in_the_enum_has_exactly_one_owner` scans the enum's source so the tag
// tables below cannot drift from it. **A new kind is never `Int12`** — that list is frozen at the live build's 100 kinds.

/// **The fences that own the lifecycle kinds added after the live testnet-12 build** (int-12, `0b1c11b87`). Each names the `Params`
/// field it is resolved from ([`PalwLifecycleKindFencesV1`]). The discriminant is the fence's index in [`Self::ALL`]
/// (`the_fence_list_is_the_enum`). A lane whose kind needs a fence of its own adds a variant here, its `params_field` arm, its
/// resolution in `Params::palw_lifecycle_kind_fences_v1` and its arm in the fold's `palw_fold_kind_in_force_v1` — the compiler asks
/// for each — and the row of [`PALW_A2_KIND_FENCE_TABLE_V1`] it fills must name the same field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PalwLifecycleKindFenceV1 {
    /// `Params::palw_probabilistic_constraints_v1` — G14 lane D's kernel route (tags 110, 111) and onboarding (104–107, 109).
    ProbabilisticConstraintsV1 = 0,
    /// `Params::palw_signed_registration_v1` — RFC-0009 G-EXPIRY / G-RULESET's signed-expiry registration envelope (tag 108).
    SignedRegistrationV1 = 1,
    /// `Params::palw_permissionless_panel_v1` — RFC-0010's certified Panel epoch output (tag 120).
    PermissionlessPanelV1 = 2,
    /// `Params::palw_provider_court_v1` — lane DA16's provider court (tags 150–153). In force only where the kernel route's fence is
    /// too (the court's rows are the route's), as the processor's `palw_provider_court_at` reads it.
    ProviderCourtV1 = 3,
}

impl PalwLifecycleKindFenceV1 {
    /// Every owning fence, in declaration order (index = discriminant).
    pub const ALL: [Self; 4] =
        [Self::ProbabilisticConstraintsV1, Self::SignedRegistrationV1, Self::PermissionlessPanelV1, Self::ProviderCourtV1];

    /// The `Params` field the fence is resolved from.
    pub const fn params_field(self) -> &'static str {
        match self {
            Self::ProbabilisticConstraintsV1 => "palw_probabilistic_constraints_v1",
            Self::SignedRegistrationV1 => "palw_signed_registration_v1",
            Self::PermissionlessPanelV1 => "palw_permissionless_panel_v1",
            Self::ProviderCourtV1 => "palw_provider_court_v1",
        }
    }
}

/// Who owns a lifecycle kind: the live build (which decodes it, and whose rules for it are this build's at every height), or the
/// fence below which this build reads its bytes as the live build does — undecodable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwLifecycleKindOwnerV1 {
    /// One of the 100 kinds `0b1c11b87` decodes ([`PALW_LIFECYCLE_INT12_KINDS_V1`]).
    Int12,
    /// A kind added after it, and the fence that owns it ([`PALW_LIFECYCLE_NEW_KINDS_V1`]).
    Fence(PalwLifecycleKindFenceV1),
}

/// **The kind → owner table.** Exhaustive and wildcard-free on purpose: adding a `PalwConsensusObjectV2` variant breaks this `match`
/// until the author names the fence that owns it (and the tag tables below, which a test reconciles with the enum's source).
pub fn palw_lifecycle_kind_owner_v1(object: &PalwConsensusObjectV2) -> PalwLifecycleKindOwnerV1 {
    use PalwConsensusObjectV2 as O;
    use PalwLifecycleKindFenceV1 as F;
    match object {
        // The 100 kinds the live build decodes — frozen; never extended (see `PALW_LIFECYCLE_INT12_KINDS_V1`).
        O::BondRegistered { .. }
        | O::BondCapabilityDeclared { .. }
        | O::BondRetireRequested { .. }
        | O::ClassRegistered { .. }
        | O::ClassFrozen { .. }
        | O::PanelBound { .. }
        | O::ReceiptLicensed { .. }
        | O::CourtOpened { .. }
        | O::CourtClosed { .. }
        | O::CourtDisclosed { .. }
        | O::CourtVerdictPosted { .. }
        | O::ProducerDefaulted { .. }
        | O::FreePromptCommitted { .. }
        | O::FamilyCertified { .. }
        | O::ClassLaneCertified { .. }
        | O::ObjectChunk { .. }
        | O::DerivedArtifactV1 { .. }
        | O::DefaultAccused { .. }
        | O::MaterialDisclosed { .. }
        | O::CourtCloseDeclared { .. }
        | O::CourtCloseChunk { .. }
        | O::CourtAttnRootClaimed { .. }
        | O::CourtAttnDissected { .. }
        | O::CourtAttnChildChosen { .. }
        | O::ModelBuy { .. }
        | O::ModelSell { .. }
        | O::ModelLineFounded { .. }
        | O::ModelVersionPublished { .. }
        | O::ModelVersionPromoted { .. }
        | O::ModelVersionWithdrawn { .. }
        | O::ModelLineRolesSet { .. }
        | O::ModelLineOwnerTransferred { .. }
        | O::ModelLineRetired { .. }
        | O::ModelProposalPosted { .. }
        | O::ModelProposalClosed { .. }
        | O::ModelEvaluationPosted { .. }
        | O::ModelSeed { .. }
        | O::ModelLineBenefitsDeclared { .. }
        | O::ShardCourtAccused { .. }
        | O::ClassShardPlanDeclared { .. }
        | O::BondShardsDeclared { .. }
        | O::ShardReceiptLicensed { .. }
        | O::CourtAttnRootClaimedAnchored { .. }
        | O::CheckpointAccused { .. }
        | O::DefaultAccusedHeld { .. }
        | O::MaterialDisclosedHeld { .. }
        | O::RoundPermitEquivocated { .. }
        | O::SeatReadinessProved { .. }
        | O::ClassManifestV2 { .. }
        | O::ReceiptLicensedV2 { .. }
        | O::SeatReadinessProvedV2 { .. }
        | O::ObjectiveOffence { .. }
        | O::OptimisticLicensed { .. }
        | O::ReporterCommitted { .. }
        | O::ReporterRevealed { .. }
        | O::MaterialDisclosedV2 { .. }
        | O::PanelUnavailableQuorum { .. }
        | O::CourtAttnRootClaimedHeld { .. }
        | O::ActivationPoolFunded { .. }
        | O::ReceiptLicensedBatchV1 { .. }
        | O::AuditReceiptBatchV1 { .. }
        | O::ClassRegisteredTirV1 { .. }
        | O::TirShardCourtAccused { .. }
        | O::ClassLaneCertifiedTirV1 { .. }
        | O::CourtTirRootClaimed { .. }
        | O::CourtTirDissected { .. }
        | O::CourtTirChildChosen { .. }
        | O::DefaultAccusedTirStep { .. }
        | O::ClassRegisteredGenV1 { .. }
        | O::CourtGenRootClaimed { .. }
        | O::ModelLineImprovementPolicySet { .. }
        | O::HardCaseSubmitted { .. }
        | O::DataUseOptIn { .. }
        | O::SetterSetCommitted { .. }
        | O::SetterSetRevealed { .. }
        | O::SetterKeysRevealed { .. }
        | O::DatasetRegistered { .. }
        | O::TeachingArtifactCommitted { .. }
        | O::TeachingArtifactRevealed { .. }
        | O::TeacherLicenceRegistered { .. }
        | O::CandidateSubmitted { .. }
        | O::LineageHeadRolledBack { .. }
        | O::ImprovementPoolFunded { .. }
        | O::DefaultAccusedPipelineStep { .. }
        | O::ReservedPipelineDa84 { .. }
        | O::ReservedPipelineDa85 { .. }
        | O::HardCaseKeyRevealed { .. }
        | O::GenTensorCommitted { .. }
        | O::GenShardCourtAccused { .. }
        | O::CourtEvalRootClaimed { .. }
        | O::HeldLeafChallengeDeclared { .. }
        | O::TirShardPlanDeclared { .. }
        | O::TirShardReceiptLicensed { .. }
        | O::TirSeatReadinessProved { .. }
        | O::AdapterClassListed { .. }
        | O::AttemptRidersV1 { .. }
        | O::VerificationVertexV1 { .. }
        | O::VertexEquivocationV1 { .. }
        | O::TrapCommittedV1 { .. }
        | O::TrapRevealedV1 { .. } => PalwLifecycleKindOwnerV1::Int12,
        // G14 lane D: the kernel route (110, 111) and model onboarding (104–107, 109).
        O::ArtifactBoundV1 { .. }
        | O::ArtifactBindingChallengedV1 { .. }
        | O::KernelBoundV1 { .. }
        | O::ConformanceCommittedV1 { .. }
        | O::ConformanceEvidenceV1 { .. }
        | O::KernelRouteV1 { .. }
        | O::KernelConstraintReceiptV1 { .. }
        | O::KernelRouteChunkV1 { .. } => PalwLifecycleKindOwnerV1::Fence(F::ProbabilisticConstraintsV1),
        // RFC-0009: the signed-expiry registration envelope (108).
        O::SignedRegistrationV1 { .. } => PalwLifecycleKindOwnerV1::Fence(F::SignedRegistrationV1),
        // RFC-0010: the certified Panel epoch output (120).
        O::PanelBeaconProofV3 { .. } => PalwLifecycleKindOwnerV1::Fence(F::PermissionlessPanelV1),
        // Lane DA16: the provider court — lease, challenge, answer, DA transfer (150–153).
        O::ProviderLeaseV1 { .. } | O::ProviderChallengeV1 { .. } | O::ProviderAnswerV1 { .. } | O::DaTransferV1 { .. } => {
            PalwLifecycleKindOwnerV1::Fence(F::ProviderCourtV1)
        }
    }
}

/// **The 100 kinds the live testnet-12 build (`0b1c11b87`) decodes, as `(tag, variant)`** — frozen. A kind added after that build
/// is NOT added here, whatever its fence: it goes in [`PALW_LIFECYCLE_NEW_KINDS_V1`]. Pinned by `the_int12_kind_list_is_frozen`.
pub const PALW_LIFECYCLE_INT12_KINDS_V1: [(u8, &str); 100] = [
    (0, "BondRegistered"),
    (1, "BondCapabilityDeclared"),
    (2, "BondRetireRequested"),
    (3, "ClassRegistered"),
    (4, "ClassFrozen"),
    (5, "PanelBound"),
    (6, "ReceiptLicensed"),
    (7, "CourtOpened"),
    (8, "CourtClosed"),
    (9, "CourtDisclosed"),
    (10, "CourtVerdictPosted"),
    (11, "ProducerDefaulted"),
    (12, "FreePromptCommitted"),
    (13, "FamilyCertified"),
    (14, "ClassLaneCertified"),
    (15, "ObjectChunk"),
    (16, "DerivedArtifactV1"),
    (17, "DefaultAccused"),
    (18, "MaterialDisclosed"),
    (19, "CourtCloseDeclared"),
    (20, "CourtCloseChunk"),
    (21, "CourtAttnRootClaimed"),
    (22, "CourtAttnDissected"),
    (23, "CourtAttnChildChosen"),
    (24, "ModelBuy"),
    (25, "ModelSell"),
    (26, "ModelLineFounded"),
    (27, "ModelVersionPublished"),
    (28, "ModelVersionPromoted"),
    (29, "ModelVersionWithdrawn"),
    (30, "ModelLineRolesSet"),
    (31, "ModelLineOwnerTransferred"),
    (32, "ModelLineRetired"),
    (33, "ModelProposalPosted"),
    (34, "ModelProposalClosed"),
    (35, "ModelEvaluationPosted"),
    (36, "ModelSeed"),
    (37, "ModelLineBenefitsDeclared"),
    (38, "ShardCourtAccused"),
    (39, "ClassShardPlanDeclared"),
    (40, "BondShardsDeclared"),
    (41, "ShardReceiptLicensed"),
    (42, "CourtAttnRootClaimedAnchored"),
    (43, "CheckpointAccused"),
    (44, "DefaultAccusedHeld"),
    (45, "MaterialDisclosedHeld"),
    (46, "RoundPermitEquivocated"),
    (47, "SeatReadinessProved"),
    (48, "ClassManifestV2"),
    (49, "ReceiptLicensedV2"),
    (50, "SeatReadinessProvedV2"),
    (51, "ObjectiveOffence"),
    (52, "OptimisticLicensed"),
    (53, "ReporterCommitted"),
    (54, "ReporterRevealed"),
    (55, "MaterialDisclosedV2"),
    (56, "PanelUnavailableQuorum"),
    (57, "CourtAttnRootClaimedHeld"),
    (58, "ActivationPoolFunded"),
    (59, "ReceiptLicensedBatchV1"),
    (60, "AuditReceiptBatchV1"),
    (61, "ClassRegisteredTirV1"),
    (62, "TirShardCourtAccused"),
    (63, "ClassLaneCertifiedTirV1"),
    (64, "CourtTirRootClaimed"),
    (65, "CourtTirDissected"),
    (66, "CourtTirChildChosen"),
    (67, "DefaultAccusedTirStep"),
    (68, "ClassRegisteredGenV1"),
    (69, "CourtGenRootClaimed"),
    (70, "ModelLineImprovementPolicySet"),
    (71, "HardCaseSubmitted"),
    (72, "DataUseOptIn"),
    (73, "SetterSetCommitted"),
    (74, "SetterSetRevealed"),
    (75, "SetterKeysRevealed"),
    (76, "DatasetRegistered"),
    (77, "TeachingArtifactCommitted"),
    (78, "TeachingArtifactRevealed"),
    (79, "TeacherLicenceRegistered"),
    (80, "CandidateSubmitted"),
    (81, "LineageHeadRolledBack"),
    (82, "ImprovementPoolFunded"),
    (83, "DefaultAccusedPipelineStep"),
    (84, "ReservedPipelineDa84"),
    (85, "ReservedPipelineDa85"),
    (86, "HardCaseKeyRevealed"),
    (87, "GenTensorCommitted"),
    (88, "GenShardCourtAccused"),
    (89, "CourtEvalRootClaimed"),
    (90, "HeldLeafChallengeDeclared"),
    (91, "TirShardPlanDeclared"),
    (92, "TirShardReceiptLicensed"),
    (93, "TirSeatReadinessProved"),
    (94, "AdapterClassListed"),
    (95, "AttemptRidersV1"),
    (100, "VerificationVertexV1"),
    (101, "VertexEquivocationV1"),
    (102, "TrapCommittedV1"),
    (103, "TrapRevealedV1"),
];

/// **Every kind added after the live build, as `(tag, variant, owning fence)`.** A kind added without an entry here fails
/// `every_kind_in_the_enum_has_exactly_one_owner`; the entry's fence must be what [`palw_lifecycle_kind_owner_v1`] answers
/// (`the_new_kind_table_is_the_owner_function`).
pub const PALW_LIFECYCLE_NEW_KINDS_V1: &[(u8, &str, PalwLifecycleKindFenceV1)] = &[
    (104, "ArtifactBoundV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (105, "ArtifactBindingChallengedV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (106, "KernelBoundV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (107, "ConformanceCommittedV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (108, "SignedRegistrationV1", PalwLifecycleKindFenceV1::SignedRegistrationV1),
    (109, "ConformanceEvidenceV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (110, "KernelRouteV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (111, "KernelConstraintReceiptV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (113, "KernelRouteChunkV1", PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1),
    (120, "PanelBeaconProofV3", PalwLifecycleKindFenceV1::PermissionlessPanelV1),
    (150, "ProviderLeaseV1", PalwLifecycleKindFenceV1::ProviderCourtV1),
    (151, "ProviderChallengeV1", PalwLifecycleKindFenceV1::ProviderCourtV1),
    (152, "ProviderAnswerV1", PalwLifecycleKindFenceV1::ProviderCourtV1),
    (153, "DaTransferV1", PalwLifecycleKindFenceV1::ProviderCourtV1),
];

/// **The owning fences' activations, resolved once from `Params`** (`Params::palw_lifecycle_kind_fences_v1`) and asked by every site
/// that judges a lifecycle payload with a height in hand: the transaction validator's header context, the processor's objects-of-block
/// walk and its chunk reader, and the UTXO walk's rent. `Default` is every fence unarmed — the live build's reading of every new kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwLifecycleKindFencesV1 {
    /// Indexed by the fence's discriminant ([`PalwLifecycleKindFenceV1::ALL`]'s order).
    activations: [Option<crate::config::params::ForkActivation>; PalwLifecycleKindFenceV1::ALL.len()],
}

impl PalwLifecycleKindFencesV1 {
    /// These activations with `fence` resolved to `activation` (how `Params` resolves each fence, and how a test arms one).
    pub fn with(mut self, fence: PalwLifecycleKindFenceV1, activation: Option<crate::config::params::ForkActivation>) -> Self {
        if let Some(slot) = self.activations.get_mut(fence as usize) {
            *slot = activation;
        }
        self
    }

    /// The activation a fence resolves to (`never()` read as absence; a fence missing from [`PalwLifecycleKindFenceV1::ALL`] is never
    /// in force, so its kinds ride unjudged — the safe reading).
    pub fn activation(&self, fence: PalwLifecycleKindFenceV1) -> Option<crate::config::params::ForkActivation> {
        self.activations
            .get(fence as usize)
            .copied()
            .flatten()
            .filter(|activation| *activation != crate::config::params::ForkActivation::never())
    }

    /// Is `fence` in force at `daa_score`?
    pub fn in_force_at(&self, fence: PalwLifecycleKindFenceV1, daa_score: u64) -> bool {
        self.activation(fence).is_some_and(|activation| activation.is_active(daa_score))
    }

    /// **Does this build read `object` as a statement at `daa_score`** — a kind the live build knows, or one whose owning fence is in
    /// force there? `false` means: read its bytes as the live build does, undecodable — skip it, charge nothing.
    pub fn kind_in_force_at(&self, object: &PalwConsensusObjectV2, daa_score: u64) -> bool {
        match palw_lifecycle_kind_owner_v1(object) {
            PalwLifecycleKindOwnerV1::Int12 => true,
            PalwLifecycleKindOwnerV1::Fence(fence) => self.in_force_at(fence, daa_score),
        }
    }
}

impl crate::config::params::Params {
    /// **The owning fences, resolved once** — the one reading every height-holding site asks ([`PalwLifecycleKindFencesV1`]).
    pub fn palw_lifecycle_kind_fences_v1(&self) -> PalwLifecycleKindFencesV1 {
        PalwLifecycleKindFenceV1::ALL.into_iter().fold(PalwLifecycleKindFencesV1::default(), |fences, fence| {
            fences.with(
                fence,
                match fence {
                    PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1 => self.palw_probabilistic_constraints_v1,
                    PalwLifecycleKindFenceV1::SignedRegistrationV1 => self.palw_signed_registration_v1,
                    PalwLifecycleKindFenceV1::PermissionlessPanelV1 => self.palw_permissionless_panel_v1.map(|rule| rule.activation),
                    // In force where BOTH the court's and the kernel route's fences are (the processor's `palw_provider_court_at`).
                    PalwLifecycleKindFenceV1::ProviderCourtV1 => {
                        let never = crate::config::params::ForkActivation::never();
                        match (self.palw_provider_court_v1, self.palw_probabilistic_constraints_v1) {
                            (Some(court), Some(route)) if court != never && route != never => {
                                Some(crate::config::params::ForkActivation::new(court.daa_score().max(route.daa_score())))
                            }
                            _ => None,
                        }
                    }
                },
            )
        })
    }

    /// **A-2 is the premise of every owning fence.** Below its fence a new kind's bytes are judged as the live build judges them, and
    /// isolation — which holds no height — judges them so at every height: tolerated where the ruleset declares
    /// `palw_audit_2026_09_11`, refused as undecodable where it does not. On a ruleset without the audit fence the kind could therefore
    /// never ride, past its fence included, so arming an owning fence there is refused rather than silently inert.
    pub fn validate_palw_lifecycle_kind_fences_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        let fences = self.palw_lifecycle_kind_fences_v1();
        let audit_declared = self.palw_audit_2026_09_11.is_some_and(|f| f != crate::config::params::ForkActivation::never());
        for fence in PalwLifecycleKindFenceV1::ALL {
            if fences.activation(fence).is_some() && !audit_declared {
                return Err(crate::palw_mode_v2::PalwModeV2Error::Invalid(
                    "a lifecycle kind's owning fence is armed without palw_audit_2026_09_11 declared: below it the kind is the live build's \
                     undecodable payload, which only an audit-declaring ruleset tolerates (A-2 uniformity)",
                ));
            }
        }
        Ok(())
    }
}

/// **The reason the live build gives when a payload's object tag is one it does not know** — borsh's own unknown-discriminant message
/// (`borsh-derive` 1.5, `use_discriminant`), so a site that reads a not-yet-in-force kind as undecodable reports, byte for byte, what
/// the live build reports (`a_not_in_force_kind_is_undecodable_in_the_live_builds_words` pins it against borsh).
pub fn palw_lifecycle_unknown_tag_reason_v1(object_bytes: &[u8]) -> String {
    match object_bytes.first() {
        Some(tag) => format!("Unexpected variant tag: {tag:?}"),
        None => "Unexpected length of input".to_string(),
    }
}

/// **The header-context half of the lifecycle door** (`TransactionValidator::check_palw_lifecycle_kind_in_context`): at the containing
/// block's DAA, a kind owned by a fence in force there meets its own stateless rule (the may-ride arm isolation skipped for it); below
/// its fence it is the live build's undecodable payload, whose verdict isolation already gave. Every kind the live build knows was
/// judged at isolation and passes here. A payload that does not decode, or names another wire version, was isolation's alone.
pub fn validate_palw_lifecycle_tx_in_context_v1(
    payload: &[u8],
    fences: &PalwLifecycleKindFencesV1,
    ctx_daa_score: u64,
) -> Result<(), PalwLifecycleTxError> {
    let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(payload) else { return Ok(()) };
    if payload.version != PALW_LIFECYCLE_TX_VERSION_V2 {
        return Ok(());
    }
    match palw_lifecycle_kind_owner_v1(&payload.object) {
        PalwLifecycleKindOwnerV1::Int12 => Ok(()),
        PalwLifecycleKindOwnerV1::Fence(fence) if !fences.in_force_at(fence, ctx_daa_score) => Ok(()),
        PalwLifecycleKindOwnerV1::Fence(_) => {
            palw_lifecycle_object_may_ride_v2(&payload.object).map_err(PalwLifecycleTxError::ObjectMayNotRide)
        }
    }
}

// ---- One level down: the kernel route's inner kinds (inside tag 110) ---------------------------------------------------------------
//
// Tag 110's bytes are a `misaka_palw_kernel::route::KernelRouteObjectV1`. Below `palw_probabilistic_constraints_v1` all of them are the
// live build's undecodable payload (the table above). Past it, a build that does not know an inner kind refuses it at the kernel's
// decode — the gate drops the object and the block stands — so an inner kind added later is read the same way below ITS fence, by the
// one gate rule [`palw_kernel_route_inner_fence_v1`] feeds (the processor's `palw_kernel_route_object_is_signed`).

/// **The fences that own kernel-route inner kinds beyond tag 110's own.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalwKernelInnerFenceV1 {
    /// `Params::palw_panel_free_v1` (RFC-0015): a registration under a verification mode (inner 13, 14).
    PanelFreeV1,
    /// `Params::palw_typed_roots_v1` (RFC-0004 Part II, R4X): a typed-root object (inner 19) and a typed-root proof
    /// (`ProsecutionV1::Spec` inside a filing, inner 7).
    TypedRootsV1,
    /// BOTH `Params::palw_panel_free_v1` and `Params::palw_typed_roots_v1`: an object an inner kind of the first carries in a form of
    /// the second — G14R's salted claim reveal (inner 20, `palw_panel_free_v1`) wrapping a typed-root claim (`SaltedCommitV1::Spec`,
    /// 19). Its row names `palw_typed_roots_v1`; in force only where `palw_panel_free_v1` is too (as `ProviderCourtV1` needs the
    /// kernel route's).
    PanelFreeAndTypedRootsV1,
}

impl PalwKernelInnerFenceV1 {
    /// Every inner-kind fence.
    pub const ALL: [Self; 3] = [Self::PanelFreeV1, Self::TypedRootsV1, Self::PanelFreeAndTypedRootsV1];

    /// The `Params` field the fence is resolved from.
    pub const fn params_field(self) -> &'static str {
        match self {
            Self::PanelFreeV1 => "palw_panel_free_v1",
            Self::TypedRootsV1 | Self::PanelFreeAndTypedRootsV1 => "palw_typed_roots_v1",
        }
    }
}

/// **The inner-kind → fence table** (`None`: tag 110's own fence is the whole rule). Exhaustive and wildcard-free: an inner kind
/// added to `KernelRouteObjectV1` does not compile until it names its fence, and `every_kernel_inner_kind_has_exactly_one_row`
/// reconciles [`PALW_KERNEL_ROUTE_INNER_KINDS_V1`] with the enum's source.
pub fn palw_kernel_route_inner_fence_v1(object: &misaka_palw_kernel::route::KernelRouteObjectV1) -> Option<PalwKernelInnerFenceV1> {
    use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
    match object {
        // Guarded arms first: a variant appended INSIDE an inner kind is owned by its fence, so a build without the variant (whose
        // kernel decode fails on it) and this one read the object the same way below that fence. R4X's typed-root proof rides a filing.
        K::FileProof { proof: misaka_palw_kernel::route::ProsecutionV1::Spec(_), .. } => Some(PalwKernelInnerFenceV1::TypedRootsV1),
        // G14R's salted reveal of a typed-root claim needs both fences (inner 20 carrying `SaltedCommitV1::Spec`).
        K::CommitClaimSalted { commit: misaka_palw_kernel::route::SaltedCommitV1::Spec { .. }, .. } => {
            Some(PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1)
        }
        K::RegisterClass { .. }
        | K::RegisterPipelineClass { .. }
        | K::PostJob { .. }
        | K::PostPipelineJob { .. }
        | K::CommitClaim { .. }
        | K::CommitPipelineClaim { .. }
        | K::FileProof { .. }
        | K::FileDemand { .. }
        | K::Respond { .. }
        | K::RequestExit { .. }
        | K::Withdraw { .. }
        | K::SealClaim { .. }
        | K::SealProof { .. } => None,
        // K2S (K2-TIR-v4/v5): tag 110's own fence; its classes register only under OPV (`palw_panel_free_v1`, inner 13), so none of
        // these can act where that fence is not armed either.
        K::CommitSegmentedClaim { .. } | K::PostTiledJob { .. } | K::PostPromptTile { .. } => None,
        K::CommitClaimSalted { .. } => Some(PalwKernelInnerFenceV1::PanelFreeV1),
        K::RegisterClassV2 { .. } | K::RegisterPipelineClassV2 { .. } => Some(PalwKernelInnerFenceV1::PanelFreeV1),
        K::Spec { .. } => Some(PalwKernelInnerFenceV1::TypedRootsV1),
    }
}

/// Every kernel-route inner kind as `(inner tag, variant, fence beyond tag 110's)`.
pub const PALW_KERNEL_ROUTE_INNER_KINDS_V1: &[(u8, &str, Option<PalwKernelInnerFenceV1>)] = &[
    (1, "RegisterClass", None),
    (2, "RegisterPipelineClass", None),
    (3, "PostJob", None),
    (4, "PostPipelineJob", None),
    (5, "CommitClaim", None),
    (6, "CommitPipelineClaim", None),
    (7, "FileProof", None),
    (8, "FileDemand", None),
    (9, "Respond", None),
    (10, "RequestExit", None),
    (11, "Withdraw", None),
    (12, "SealClaim", None),
    (15, "SealProof", None),
    (16, "CommitSegmentedClaim", None),
    (17, "PostTiledJob", None),
    (18, "PostPromptTile", None),
    (13, "RegisterClassV2", Some(PalwKernelInnerFenceV1::PanelFreeV1)),
    (14, "RegisterPipelineClassV2", Some(PalwKernelInnerFenceV1::PanelFreeV1)),
    (19, "Spec", Some(PalwKernelInnerFenceV1::TypedRootsV1)),
    (20, "CommitClaimSalted", Some(PalwKernelInnerFenceV1::PanelFreeV1)),
];

// The rows above are for inner kinds the live tree has. In-flight lanes join the table with the fence their row of
// [`PALW_A2_KIND_FENCE_TABLE_V1`] names: G14-R4 inner 15 and K2S 16–18 under tag 110's own fence (`None` here), R4X 19 `Spec` under
// `palw_typed_roots_v1` (a `PalwKernelInnerFenceV1` variant, and its gate in `palw_kernel_inner_fence_at` — not a hand-written arm in
// the gate), G14R 20 `CommitClaimSalted` under `palw_panel_free_v1` (`Some(PanelFreeV1)`, beside 13 and 14). A variant appended INSIDE an inner kind (R4X's `ClaimBodyV1::Spec`) is answered by a guarded arm of
// [`palw_kernel_route_inner_fence_v1`] ahead of the kind's own, exactly as the top-level table's guarded arms work.

// ---- Inside the live build's own kinds: its wire types are frozen ------------------------------------------------------------------
//
// A kind the live build DOES decode can still carry bytes it cannot read as a newer build does. Two shapes:
//
// * **appended** — a variant appended to a nested Borsh enum, or a field appended to a nested struct (HFX's
//   `PalwGenProfileOffersV1::Head` inside tag 68). The live build fails the whole payload's decode and tolerates it; a newer build
//   that decodes it must read it the same way below its fence: a guarded arm in [`palw_lifecycle_kind_owner_v1`] (before the `Int12`
//   arm) names the fence that owns every object carrying the form, and from that arm every site reads it as undecodable.
// * **re-read** — a new meaning for bytes the live build DECODES: a tag byte it reads by hand (HFX's `PalwGenProfileV1::Head = 6` in
//   the class's `profile: u8`). The live build judges those bytes with its own code — for tag 68, a refusal where its `from_tag`
//   fails — so below the fence the newer build must give THAT verdict at THAT stage with the same side effects; reading them as
//   undecodable would be a different verdict (skipped before the acceptance walk instead of dropped in it). No guarded arm: the kind's
//   own rule refuses the new meaning below its fence, and a probe in the pin test plus the replay through int-12 is the evidence.
//
// So every wire type the live build compiled is frozen (`palw_lifecycle_objects_v2/int12_borsh_manifest.tsv`, a digest per type, read
// off `0b1c11b87`'s source): each Borsh-derived `struct`/`enum`, each hand-written `impl BorshDeserialize`, and each `#[repr(uN)]` enum
// (the tag bytes read by hand). `every_int12_wire_type_is_unchanged_or_classified` refuses any change not classified here.

/// How a change to one of the live build's wire types is accounted for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwInt12WireChangeV1 {
    /// `PalwConsensusObjectV2` itself: its added variants are [`PALW_LIFECYCLE_NEW_KINDS_V1`]'s.
    ObjectEnum,
    /// State, delta or parameter encoding — never inside a block's carriage (why).
    NotCarried(&'static str),
    /// Appended inside a carried kind (the live build cannot decode the new form): every object carrying it is owned by the lifecycle
    /// fence whose `Params` field is `fence` ([`palw_lifecycle_kind_owner_v1`]'s guarded arm, which the test asks of a sample), and
    /// the type's current digest is pinned so a further change is classified again.
    CarriedAppended { fence: &'static str, digest: u64 },
    /// A new meaning for bytes the live build decodes and judges (a hand-read tag): below `fence` the kind's own rule gives the live
    /// build's verdict at the live build's stage. Digest pinned as above; its row in [`PALW_A2_KIND_FENCE_TABLE_V1`] names the probe.
    CarriedReread { fence: &'static str, digest: u64 },
}

/// **Every live-build wire type that has changed since `0b1c11b87`, keyed `"<path>::<item>"`** — reconciled exactly (no missing,
/// no stale row) by `every_int12_wire_type_is_unchanged_or_classified`.
pub const PALW_INT12_WIRE_CHANGES_V1: &[(&str, PalwInt12WireChangeV1)] = &[
    ("consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2", PalwInt12WireChangeV1::ObjectEnum),
    (
        "consensus/core/src/palw_state_v2.rs::PalwDeltaEntryV2",
        PalwInt12WireChangeV1::NotCarried("the fold's journal (stored deltas), never a block's carriage"),
    ),
    (
        "consensus/core/src/palw_state_v2.rs::PalwVoidReasonV2",
        PalwInt12WireChangeV1::NotCarried("a claim row's terminal reason, written by the fold; no carried object names one"),
    ),
    (
        "consensus/core/src/palw_state_v2.rs::PalwStateParamsV2",
        PalwInt12WireChangeV1::NotCarried("the ruleset's parameters (the V2 bundle), never carried"),
    ),
    (
        "consensus/core/src/palw_improve_state_v1.rs::PalwNoChangeReasonV1",
        PalwInt12WireChangeV1::NotCarried("an improvement epoch row's outcome, written by the fold"),
    ),
    // `pre` (ADR-0175): `CandidateSelected` (2) appended; the fold writes it only past `palw_model_immutable_v1`.
    (
        "consensus/core/src/palw_improve_state_v1.rs::PalwPromotionOutcomeV1",
        PalwInt12WireChangeV1::NotCarried(
            "an improvement epoch row's decision, written by the fold; CandidateSelected (2) only past palw_model_immutable_v1",
        ),
    ),
    // SMALL (RFC-0009 RDA4, RFC-0001 P1): the signer protocol and the worker frame — node-local wire forms, never a block's.
    (
        "consensus/core/src/dns_finality.rs::SigningPurpose",
        PalwInt12WireChangeV1::NotCarried(
            "the node ↔ signer protocol's purpose tag; a block carries signatures, never a purpose (RDA4 = 8 is offered only under \
             palw_receipt_spend_v4)",
        ),
    ),
    (
        "consensus/core/src/dns_finality.rs::SignerMessageDigest",
        PalwInt12WireChangeV1::NotCarried("the node ↔ signer protocol's typed digest; never a block's carriage"),
    ),
    (
        "consensus/core/src/palw_freeprompt_v3.rs::PalwFpWorkerFrameV1",
        PalwInt12WireChangeV1::NotCarried("the worker ↔ gateway v3-serve frame (Cancelled = 8); node-local, no consensus object"),
    ),
    (
        "consensus/core/src/palw_state_v2.rs::impl BorshDeserialize for PalwStateCarriageV2",
        PalwInt12WireChangeV1::NotCarried(
            "the PALW state's own carriage (pruning point, IBD sidecar): three tails appended (seat-root readiness, Panel V3, the kernel \
             route), each written only when its state is non-empty, so below every fence the bytes are int-12's — the int-12 replay \
             checks the root of every block",
        ),
    ),
];

// ---- The central kind → fence table (A2U, 2026-10-09) -------------------------------------------------------------------------------
//
// One row per thing the live testnet-12 build (int-12, `0b1c11b87`) cannot read the way a newer build reads it — a top-level object
// tag, a kernel-route inner kind or nested variant, a header carriage form, a coinbase trailer, a form appended inside a type int-12
// decodes, a free-prompt job form, a formula a header commits, a state encoding — and the fence below which a newer build must read
// it exactly as int-12 does. Rows are the Lead's allocations (`remaining-rfc-integration-matrix.md` §2); a row whose lane has not
// merged is a promise the merge is held to: the tests reconcile every row with what the code in this tree says
// (`every_landed_kind_sits_in_its_row_with_the_rows_fence` and its siblings), so a kind that lands under another fence, or outside
// every row, fails.

/// What a row of [`PALW_A2_KIND_FENCE_TABLE_V1`] covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwA2SlotV1 {
    /// `PalwConsensusObjectV2` tags `lo..=hi` on a 0x4b carrier. int-12: undecodable — tolerated (testnet-12 declares the audit fence),
    /// the carrier skipped. Landed kinds: [`PALW_LIFECYCLE_NEW_KINDS_V1`].
    ObjectTags { lo: u8, hi: u8 },
    /// `misaka_palw_kernel::route::KernelRouteObjectV1` inner kinds `lo..=hi`, inside tag 110's bytes. Below tag 110's own fence they
    /// are tag 110's undecodable payload; past it a build without the inner kind fails the kernel's decode and drops the object, the
    /// block standing. Landed kinds: [`PALW_KERNEL_ROUTE_INNER_KINDS_V1`].
    KernelInner { lo: u8, hi: u8 },
    /// A variant or field appended inside a kernel-route inner kind (`"<enum>::<variant> (<discriminant>)"`): read as that inner kind's
    /// undecodable bytes below the fence ([`palw_kernel_route_inner_fence_v1`] answers the fence for an object carrying it).
    KernelNested { what: &'static str },
    /// A header `palw_commitment` form, by algo id and magic. int-12: its shape gate has no arm for it — refused. Landed forms:
    /// `crate::pow_layer0::palw_header_form_owner_v1`.
    HeaderForm { algo_id: u8, magic: [u8; 4] },
    /// A trailer at the end of a chain block's coinbase extra data, by magic. int-12: opaque miner bytes — nothing validated or read.
    CoinbaseTrailer { magic: [u8; 4] },
    /// A form appended inside a type int-12 DECODES, carried by an int-12 kind (`"<path>::<item>"`, a key of the frozen manifest, or for
    /// a tag byte int-12 reads by hand the field that holds it). int-12's verdict is whatever its own code gives those bytes — for an
    /// appended Borsh variant, undecodable (tolerated on 0x4b); for an unknown tag byte, its own refusal at its own stage. Below the fence
    /// a newer build gives that verdict, at that stage, with the same side effects (`PalwInt12WireChangeV1::Carried`).
    Int12Inner { key: &'static str },
    /// A free-prompt job form on 0x4a (a job version or an appended variant inside a job). int-12: undecodable 0x4a bytes are REFUSED
    /// (the block is invalid) — 0x4a has no audit tolerance — so below the fence a newer build refuses them too: its door admits the
    /// form at isolation only where the ruleset carries the fence, and the header-context door below its height.
    FpJobForm { what: &'static str },
    /// A formula a header commits (e.g. `palw_state_root`): below the fence every header's bytes and hash are int-12's, which the
    /// replay through int-12 itself checks block by block (`docs/design/palw/a2-uniformity-new-kinds.md` §5).
    HeaderFormula { what: &'static str },
    /// State, delta, carriage-tail or engine encodings — never a block's carriage. Below the fence the fold must be byte-identical to
    /// int-12's (the same PALW state root at every block), which the int-12 replay checks.
    StateEncoding { what: &'static str },
    /// **Kinds int-12 decodes and judges, recognized and refused BY NAME past the fence** — the acceptance walk drops the object
    /// before any slot, rent, signature or state write, and the carrying block stands (ADR-0175 "有効化と履歴"). Below the fence
    /// int-12's own rule for them, unchanged. Reconciled exactly with [`palw_int12_kind_refused_past_fence_v1`].
    Int12RefusedByName { tags: &'static [u8], what: &'static str },
    /// **Kinds int-12 decodes and folds, whose fold a fence changes by STATE** (a refusal that depends on the rows, a new id formula,
    /// a different outcome) — never a block verdict. Below the fence the fold is byte-identical to int-12's, which the pin test's
    /// armed-far node and the int-12 replay check block by block.
    Int12FoldPastFence { tags: &'static [u8], what: &'static str },
}

/// **The fence past which an object of a kind the live build decodes is recognized and refused by name** (`None`: no such fence;
/// the kind's rule is int-12's at every height). The one question the table asks of the processor's acceptance walk and the pure
/// fold (each asks [`palw_model_definition_update_v1`] under `palw_model_immutable_v1`). `Some` is a `Params` field name.
pub fn palw_int12_kind_refused_past_fence_v1(object: &PalwConsensusObjectV2) -> Option<&'static str> {
    match palw_lifecycle_kind_owner_v1(object) {
        PalwLifecycleKindOwnerV1::Int12 => palw_model_definition_update_v1(object).map(|_| "palw_model_immutable_v1"),
        PalwLifecycleKindOwnerV1::Fence(_) => None,
    }
}

/// One row: the slot, the `Params` field of the fence that owns it, the lane or RFC that holds the allocation, and whether the code is
/// in this tree. `fence` is a field NAME so a row can be written before its lane merges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwA2RowV1 {
    pub slot: PalwA2SlotV1,
    pub fence: &'static str,
    pub owner: &'static str,
    pub landed: bool,
}

const fn a2_row(slot: PalwA2SlotV1, fence: &'static str, owner: &'static str, landed: bool) -> PalwA2RowV1 {
    PalwA2RowV1 { slot, fence, owner, landed }
}

/// **The central kind → fence table.** Every post-int-12 kind, variant, header form, trailer, appended form, formula and encoding, and
/// the fence below which it rides unjudged (read exactly as int-12 reads its bytes: the same verdict, nothing charged, counted or
/// written). The Lead allocates; A2U keeps the table; a lane fills its row's code at merge, and the reconciliation tests hold it to the
/// row.
pub const PALW_A2_KIND_FENCE_TABLE_V1: &[PalwA2RowV1] = &[
    // ---- top-level object tags (0x4b) ----
    a2_row(PalwA2SlotV1::ObjectTags { lo: 104, hi: 107 }, "palw_probabilistic_constraints_v1", "G14 lane D phase 3 onboarding", true),
    a2_row(PalwA2SlotV1::ObjectTags { lo: 108, hi: 108 }, "palw_signed_registration_v1", "RFC-0009 signed-expiry registration", true),
    a2_row(PalwA2SlotV1::ObjectTags { lo: 109, hi: 109 }, "palw_probabilistic_constraints_v1", "OB-P0 conformance evidence", true),
    // The kernel route's block 110–119: 110 route, 111 receipt (landed); 113 `KernelRouteChunkV1`, the route's own chunk lane (G14-R4,
    // `g14/r4-fixes`, merged into `adv/c4r4`) — the Lead, 2026-10-10: tag 113 → `palw_probabilistic_constraints_v1`. Its merge adds
    // `O::KernelRouteChunkV1 { .. }` to the `ProbabilisticConstraintsV1` arm of `palw_lifecycle_kind_owner_v1`, the
    // `PALW_LIFECYCLE_NEW_KINDS_V1` entry, its `PALW_A2_NEW_KIND_WIRE_V1` pin, and flips this row to landed; the tests name each.
    // 112 and 114–119 are unallocated: no row, so a kind landing there fails the table test until the Lead allocates it.
    a2_row(PalwA2SlotV1::ObjectTags { lo: 110, hi: 111 }, "palw_probabilistic_constraints_v1", "G14 lane D kernel route", true),
    a2_row(PalwA2SlotV1::ObjectTags { lo: 113, hi: 113 }, "palw_probabilistic_constraints_v1", "G14-R4 KernelRouteChunkV1", true),
    // 120 `PanelBeaconProofV3` (landed); 121–129 unallocated.
    a2_row(PalwA2SlotV1::ObjectTags { lo: 120, hi: 120 }, "palw_permissionless_panel_v1", "RFC-0010 V3 production fold", true),
    // 130 `ExecWorkRootOpenedV2` (X8R, `rfc8/x8r-review`).
    a2_row(PalwA2SlotV1::ObjectTags { lo: 130, hi: 139 }, "palw_exec_payload_v2", "X8R RFC-0008 v2", false),
    // 140–149: BUDGET (ADR-0176/0177, allocated 2026-10-10) — no row until its kinds exist: BUDGET adds one row per kind range with
    // the fence that owns it (`palw_bond_budget_v1` or `palw_model_bond_allocation_v1`, `PALW_A2_TAG_ALLOCATIONS_V1`) in the commit
    // that creates the kinds. A kind there without its row fails `every_kind_in_the_enum_has_exactly_one_owner` and
    // `every_landed_kind_sits_in_its_row_with_the_rows_fence`.
    // DA16: lease, challenge, answer, transfer. A change to their wire form (DA16's re-scope: the `Artifact` lease subject removed
    // under this fence) re-pins `PALW_A2_NEW_KIND_WIRE_V1` in the same commit and keeps this row's fence, or adds a row for another.
    a2_row(PalwA2SlotV1::ObjectTags { lo: 150, hi: 153 }, "palw_provider_court_v1", "DA16 provider court", true),
    // ---- kernel-route inner kinds (inside tag 110) ----
    a2_row(PalwA2SlotV1::KernelInner { lo: 1, hi: 12 }, "palw_probabilistic_constraints_v1", "G14 lane D (tag 110's own)", true),
    a2_row(PalwA2SlotV1::KernelInner { lo: 13, hi: 14 }, "palw_panel_free_v1", "RFC-0015 OPV registrations", true),
    a2_row(PalwA2SlotV1::KernelInner { lo: 15, hi: 15 }, "palw_probabilistic_constraints_v1", "G14-R4 SealProof", true),
    a2_row(
        PalwA2SlotV1::KernelInner { lo: 16, hi: 18 },
        "palw_probabilistic_constraints_v1",
        "K2S segmented / tiled / prompt tile",
        true,
    ),
    a2_row(PalwA2SlotV1::KernelInner { lo: 19, hi: 19 }, "palw_typed_roots_v1", "R4X Spec (RFC-0004 Part II)", true),
    // G14R's salted claim seal v2, beside the OPV registrations (the Lead, 2026-10-09): below `palw_panel_free_v1` the kernel refuses
    // it and the gate drops it through this table. Inner kinds 21 and 22 are not allocated.
    a2_row(PalwA2SlotV1::KernelInner { lo: 20, hi: 20 }, "palw_panel_free_v1", "G14R CommitClaimSalted (salted claim seal v2)", true),
    // K2S: a segmented fault rides a filing (inner 7) under tag 110's own fence, as the filing does, so no guarded arm is needed;
    // `ClaimBodyV1::Segmented` is the kernel ledger's state, never carried.
    a2_row(
        PalwA2SlotV1::KernelNested { what: "ProsecutionV1::Segmented (3) inside FileProof (inner 7); ClaimBodyV1::Segmented (2)" },
        "palw_probabilistic_constraints_v1",
        "K2S",
        true,
    ),
    // K2S × G14R: a salted reveal of a segmented claim (K2-TIR-v4/v5 classes are OPV-only, so past `palw_panel_free_v1` every
    // segmented claim reveals salted); inner 20's own fence, so no guarded arm.
    a2_row(
        PalwA2SlotV1::KernelNested { what: "SaltedCommitV1::Segmented (16) inside CommitClaimSalted (inner 20)" },
        "palw_panel_free_v1",
        "K2S × G14R",
        true,
    ),
    // `ClaimBodyV1::Spec` (3) is the kernel ledger's state, never carried; the carried form is the filing's proof.
    a2_row(
        PalwA2SlotV1::KernelNested { what: "ProsecutionV1::Spec (4) inside FileProof (inner 7)" },
        "palw_typed_roots_v1",
        "R4X",
        true,
    ),
    // G14R's salted reveal of a typed-root claim: `PalwKernelInnerFenceV1::PanelFreeAndTypedRootsV1` (both fences), by a guarded arm
    // of `palw_kernel_route_inner_fence_v1` ahead of inner 20's own — never the hand-written gate arm `g14/r4-fixes` carries.
    a2_row(
        PalwA2SlotV1::KernelNested { what: "SaltedCommitV1::Spec (19) inside CommitClaimSalted (inner 20)" },
        "palw_typed_roots_v1",
        "G14R × R4X (also needs palw_panel_free_v1)",
        true,
    ),
    // ---- header carriage forms and coinbase trailers ----
    a2_row(PalwA2SlotV1::HeaderForm { algo_id: 7, magic: *b"PFS4" }, "palw_receipt_spend_v4", "RFC-0009 V4 receipt carriage", true),
    a2_row(PalwA2SlotV1::HeaderForm { algo_id: 10, magic: *b"PXE2" }, "palw_exec_payload_v2", "X8R EXEC envelope", false),
    a2_row(PalwA2SlotV1::CoinbaseTrailer { magic: *b"PXA2" }, "palw_exec_payload_v2", "X8R anchor trailer", false),
    // ---- forms appended inside kinds int-12 decodes (tag 68 is ARMED on testnet-12: `palw_gen_v1`) ----
    a2_row(
        PalwA2SlotV1::Int12Inner { key: "consensus/core/src/palw_gen_v1.rs::PalwGenProfileV1" },
        "palw_task_heads_v1",
        "HFX task heads: PalwGenProfileV1::Head = 6, the class's hand-read profile byte inside tag 68 (re-read)",
        false,
    ),
    a2_row(
        PalwA2SlotV1::Int12Inner { key: "consensus/core/src/palw_gen_class_v1.rs::PalwGenProfileOffersV1" },
        "palw_task_heads_v1",
        "HFX task heads: PalwGenProfileOffersV1::Head (3), inside tag 68 (appended)",
        false,
    ),
    a2_row(
        PalwA2SlotV1::FpJobForm { what: "PalwGenBodyV1::Head (2) inside a generative job" },
        "palw_task_heads_v1",
        "HFX task heads",
        false,
    ),
    // ---- formulas and encodings ----
    a2_row(
        PalwA2SlotV1::HeaderFormula { what: "header.palw_state_root = H(fork-choice leaf || ADR-0043 root) past the fence" },
        "palw_fork_choice_commitment_v1",
        "L2FC",
        false,
    ),
    a2_row(
        PalwA2SlotV1::StateEncoding { what: "per-shard V3 draw (strata; delta 171, tail 0xED)" },
        "palw_permissionless_panel_v1",
        "SHARD",
        true,
    ),
    a2_row(
        PalwA2SlotV1::StateEncoding { what: "per-segment pricing and the shard engine's encodings" },
        "palw_tir_shard_segment_v2",
        "SHARD",
        true,
    ),
    a2_row(
        PalwA2SlotV1::StateEncoding { what: "bond budget engine Q/B/R/F (deltas 190–199, tail 0xEF, root block bond_budget/v1)" },
        "palw_bond_budget_v1",
        "BUDGET (ADR-0176)",
        false,
    ),
    a2_row(
        PalwA2SlotV1::StateEncoding { what: "model coinbase allocated by distinct locked miner capital f(S_m) inside the budget" },
        "palw_model_bond_allocation_v1",
        "BUDGET (ADR-0177)",
        false,
    ),
    // ADR-0032 (2026-10-10): R-1's reporter share 49% and DA-6's exposure at it. testnet-12 arms R-core+ at genesis, so below this
    // fence the share must stay int-12's 1,000 bps (and the params fingerprint int-12's); INTF moves `pre`'s unfenced constant here.
    a2_row(
        PalwA2SlotV1::StateEncoding { what: "R-1 reporter share and DA-6 exposure: 1,000 bps below, 4,900 bps past" },
        "palw_reporter_share_v2",
        "INTF (ADR-0032 amendment)",
        false,
    ),
    // ---- kinds the live build decodes, judged anew past a fence (ADR-0175, `pre`) ----
    a2_row(
        PalwA2SlotV1::Int12RefusedByName {
            tags: &[27, 28, 29, 37, 81],
            what: "ModelVersionPublished/Promoted/Withdrawn, ModelLineBenefitsDeclared granting EARLY_VERSION, LineageHeadRolledBack: \
                   recognized and refused at acceptance, not applied, the block stands",
        },
        "palw_model_immutable_v1",
        "ADR-0175 immutable registrations (pre)",
        true,
    ),
    a2_row(
        PalwA2SlotV1::Int12FoldPastFence {
            tags: &[3, 26, 39, 61, 68, 70, 91],
            what: "a Dormant class re-registered with a changed definition refused; a shard plan attached to an older class refused; a \
                   line's id H(class, root, founder, name); an epoch's winner CandidateSelected with no head move, a policy keeps the \
                   head, dissolution keeps the head history",
        },
        "palw_model_immutable_v1",
        "ADR-0175 immutable registrations (pre)",
        true,
    ),
];

/// **The Lead's object-tag allocations** (`remaining-rfc-integration-matrix.md` §2) as `(lo, hi, owner, fences a row there may
/// name)`. Every object-tag row of [`PALW_A2_KIND_FENCE_TABLE_V1`] lies inside one allocation and names one of its fences, and every
/// kind added after the live build lies inside one (`every_tag_row_is_inside_its_allocation`): a lane cannot land a kind outside its
/// block, nor under another lane's fence.
pub const PALW_A2_TAG_ALLOCATIONS_V1: &[(u8, u8, &str, &[&str])] = &[
    (104, 109, "G14 lane D onboarding, RFC-0009, OB-P0", &["palw_probabilistic_constraints_v1", "palw_signed_registration_v1"]),
    (110, 119, "kernel route (G14)", &["palw_probabilistic_constraints_v1"]),
    (120, 129, "RFC-0010 V3 production fold", &["palw_permissionless_panel_v1"]),
    (130, 139, "EXEC payload v2 (X8R)", &["palw_exec_payload_v2"]),
    (140, 149, "BUDGET (ADR-0176/0177)", &["palw_bond_budget_v1", "palw_model_bond_allocation_v1"]),
    (150, 153, "DA16 provider court", &["palw_provider_court_v1"]),
];

#[cfg(test)]
pub(crate) mod tests {
    /// **…except no registrant can build one, and the test above did not notice.**
    ///
    /// `a_bond_registration_that_locks_its_collateral_rides` constructs the transaction with an
    /// EMPTY payload, takes its id, and only then builds the object naming that id. That pair is
    /// consistent, so the lock accepts it — but it is not a transaction anyone can broadcast,
    /// because the object has to travel IN the payload and `write_transaction` folds the payload
    /// into the id (`hashing/tx.rs`). Put the object where it must go and the id moves out from
    /// under the outpoint that names it.
    ///
    /// So `bond.transaction_id == tx.id()` is a hash fixed point: to satisfy it a registrant would
    /// have to find a payload containing the id of the transaction that payload produces. That is
    /// preimage resistance, not an engineering problem.
    ///
    /// The consequence is the one an operator on testnet-11 reported and was told was fixed: no
    /// bond can enter after genesis, so only the holders of the genesis registry can ever produce.
    /// The rule is not wrong about what it wants — the carrier really should prove the money — it
    /// is wrong about how the carrier can name itself. See
    /// [`palw_bond_registration_binds_its_carrier_v2`] for the form that is constructible.
    #[test]
    fn naming_the_carrier_by_id_is_a_fixed_point_no_registrant_can_solve() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&owner);
        let outputs = vec![TransactionOutput::new(500_000, spk)];

        // The carrier, built the only way a registrant can build one: the object goes in the
        // payload, because that is the only place the chain reads it from.
        let carrier = |named: TransactionId| {
            let object = PalwConsensusObjectV2::BondRegistered {
                bond: PalwBondKeyV2(TransactionOutpoint::new(named, 0)),
                pubkey: vec![7; 4],
                operator_pubkey: vec![21; 8],
                collateral: 500_000,
                payout_payload: payee,
                capable_classes: Default::default(),
                signature: vec![1; 8],
            };
            let payload = borsh::to_vec(&crate::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object,
            })
            .expect("the lifecycle payload serializes");
            Transaction::new(0, vec![], outputs.clone(), 0, SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload)
        };

        // The registrant's best move: name the id the carrier would have had, then look at the id
        // it actually has once that name is inside it.
        let probe = carrier(TransactionId::default());
        let attempt = carrier(probe.id());
        assert_ne!(attempt.id(), probe.id(), "writing the id into the payload moves the id");

        // And the chain refuses it, by the rule's own words.
        let extracted = crate::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2(&[attempt]);
        assert!(extracted.objects.is_empty(), "no bond may enter through a transaction under this rule");
        assert_eq!(
            extracted.skipped.first().map(|(_, why)| *why),
            Some("a bond registration must name its collateral output by index, with a zero transaction id"),
            "and the refusal is the carrier-binding rule, pointing at the form that IS constructible"
        );
    }

    /// **A bond CAN enter through a transaction — this is what it has to prove.**
    ///
    /// The carrier proves the money and the signature proves the owner. Neither is a promise: the
    /// output is created by the very transaction carrying the registration, so its existence,
    /// amount and script are facts block validation established before this object was decoded.
    ///
    /// Driven through the REAL round trip — object into the payload, payload into the transaction,
    /// transaction through the extractor — because the earlier version of this test built the
    /// transaction with an empty payload and only then named its id. That pair was consistent, so
    /// the rule accepted it, and the test reported a capability nobody could use: putting the
    /// object where it must go moves the id out from under the outpoint naming it. A bond seam is
    /// only proven by the trip a registrant actually makes.
    #[test]
    fn a_bond_registration_that_locks_its_collateral_rides() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&owner);
        let object = PalwConsensusObjectV2::BondRegistered {
            // Named by index with a zero id: "the output at index 0 of whatever carries me".
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(TransactionId::default(), 0)),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 500_000,
            payout_payload: payee,
            capable_classes: Default::default(),
            signature: vec![1; 8],
        };
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("the lifecycle payload serializes");
        let tx = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(500_000, spk)],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE.clone(),
            0,
            payload,
        );

        let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
        assert!(extracted.skipped.is_empty(), "nothing should be skipped: {:?}", extracted.skipped);
        let [carried] = &extracted.objects[..] else { panic!("exactly one object rides") };
        let PalwConsensusObjectV2::BondRegistered { bond, collateral, .. } = &carried.object else {
            panic!("and it is the bond registration")
        };
        // The chain supplied the half the registrant could not.
        assert_eq!(bond.0.transaction_id, tx.id(), "the bond is keyed to the transaction that carried it");
        assert_eq!(bond.0.index, 0, "at the output it named");
        assert_eq!(*collateral, 500_000);
        // And the verifier can rebuild exactly what was signed, from the substituted key alone.
        assert_eq!(
            palw_bond_registration_signed_key_v2(bond).0,
            crate::tx::TransactionOutpoint::new(TransactionId::default(), 0),
            "a verifier recovers the signed form without needing the carrier"
        );
    }

    /// **Every way the lock can be lied to, refused by its own reason.**
    ///
    /// These are the four the audit named, and the first is the one that made the whole seam
    /// unsafe before the lock existed: a registration could DECLARE a million and stake a million,
    /// because nothing looked at any output.
    #[test]
    fn a_bond_registration_cannot_declare_collateral_it_did_not_lock() {
        let payee = h64(0xBEEF);
        let owner: [u8; 64] = *payee.as_byte_slice();
        let spk = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&owner);
        let tx = |value: u64, script: crate::tx::ScriptPublicKey| {
            Transaction::new(
                0,
                vec![],
                vec![TransactionOutput::new(value, script)],
                0,
                SUBNETWORK_ID_PALW_LIFECYCLE.clone(),
                0,
                vec![],
            )
        };
        let reg = |t: &Transaction, index: u32, collateral: u64, payee: crate::Hash64| PalwConsensusObjectV2::BondRegistered {
            bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
                // `t` selects which id the lie uses: the zero sentinel for the honest form, and a
                // real id for lie 3, which is what naming somebody else's transaction now looks like.
                if t.payload.is_empty() && t.outputs.first().map(|o| o.value) == Some(999) {
                    t.id()
                } else {
                    TransactionId::default()
                },
                index,
            )),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral,
            payout_payload: payee,
            capable_classes: Default::default(),
            signature: vec![1; 8],
        };

        // 1. Declaring more than the output holds — the free million.
        let t = tx(500_000, spk.clone());
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&t, 0, 1_000_000_000, payee)).unwrap_err();
        assert!(e.contains("more collateral than the output"), "{e}");

        // 2. Naming an output the carrier does not have.
        let t = tx(500_000, spk.clone());
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&t, 7, 500_000, payee)).unwrap_err();
        assert!(e.contains("does not have"), "{e}");

        // 3. Naming somebody ELSE's transaction — an output this registration did not create, and
        //    therefore one no layer on this path can check.
        let t = tx(500_000, spk.clone());
        let other = tx(999, spk.clone());
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&other, 0, 500_000, payee)).unwrap_err();
        assert!(e.contains("by index, with a zero transaction id"), "{e}");

        // 4. Locking the money behind a script that is not the payee's, so the collateral and the
        //    rewards would be reclaimable by different people.
        let t = tx(500_000, crate::mldsa87_primitives::p2pkh_mldsa87_spk(&[9u8; 64]));
        let e = palw_bond_registration_binds_its_carrier_v2(&t, &reg(&t, 0, 500_000, payee)).unwrap_err();
        assert!(e.contains("names as its payee"), "{e}");
    }

    /// A registration with no signature is still refused: the carrier proves the collateral, and
    /// only the signature proves who owns the key being registered.
    #[test]
    fn a_bond_registration_without_a_signature_does_not_ride() {
        let object = PalwConsensusObjectV2::BondRegistered {
            bond: bond(9),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            collateral: 500_000,
            payout_payload: h64(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        };
        let e = palw_lifecycle_object_may_ride_v2(&object).unwrap_err();
        assert!(e.contains("signature over the key it declares"), "{e}");
    }

    use super::*;
    use crate::palw_state_v2::{PalwBondKeyV2, PalwPanelSeatV2, PalwPwuRuleV2};
    use crate::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
    use crate::tx::{ScriptPublicKey, Transaction, TransactionOutpoint, TransactionOutput};
    use kaspa_hashes::Hash64;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }

    fn carrier(subnetwork: crate::subnets::SubnetworkId, payload: Vec<u8>) -> Transaction {
        Transaction::new(
            0,
            Vec::new(),
            vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            subnetwork,
            0,
            payload,
        )
    }

    fn lifecycle_tx(object: PalwConsensusObjectV2) -> Transaction {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object })
            .expect("a lifecycle payload is borsh-serializable");
        carrier(SUBNETWORK_ID_PALW_LIFECYCLE, payload)
    }

    #[test]
    fn an_historical_full_v2_possession_proof_still_rides_and_reaches_the_fold() {
        use crate::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOperandV1};

        // The first V2 producer opened its complete sixteen-leaf draw under the former 1 MiB
        // ceiling. Eighty KiB is a valid historical carrier but exceeds the current 67,232-byte
        // prefix ceiling, which is the exact replay/IBD compatibility boundary.
        let proof = PalwArtifactMultiproofV1 {
            leaf_count: 16,
            opened: (0..16)
                .map(|index| {
                    (
                        index,
                        PalwArtifactOperandV1 {
                            tensor_name: "legacy".into(),
                            layer: None,
                            row_start: index,
                            bytes: vec![index as u8; 5_000],
                        },
                    )
                })
                .collect(),
            siblings: Vec::new(),
        };
        assert!(crate::palw_model_registry_v1::palw_readiness_v2_is_legacy_full_challenge_v1(
            proof.opened.len(),
            proof.operand_bytes()
        ));
        let object = PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond: bond(1),
            class_id: h64(2),
            span: 3,
            proof: Box::new(proof),
            signature: vec![1],
        };
        let tx = lifecycle_tx(object);
        validate_palw_lifecycle_tx(&tx.payload, true).expect("the historical carrier remains block-valid");
        let extracted = palw_lifecycle_objects_from_accepted_txs_v2(&[tx]);
        assert_eq!(extracted.objects.len(), 1, "the historical proof reaches stateful validation rather than being skipped");
        assert!(extracted.skipped.is_empty());
    }

    fn panel_bound() -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::PanelBound {
            claim: h64(0xC1),
            anchor: h64(0x77),
            seats: vec![PalwPanelSeatV2 { bond: bond(2), operator_id: h64(0x22) }],
        }
    }

    /// **P0-11's fix, at this layer.** A `PanelBound` in a transaction becomes a `PanelBound` the
    /// transition can fold — which no block could do at all before this module existed.
    #[test]
    fn a_lifecycle_object_rides_a_transaction_and_arrives_in_acceptance_order() {
        let first = lifecycle_tx(PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC1), receipts: Vec::new() });
        let second = lifecycle_tx(PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC2), receipts: Vec::new() });
        let unrelated = carrier(crate::subnets::SUBNETWORK_ID_NATIVE, Vec::new());
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[first.clone(), unrelated, second.clone()]);

        assert_eq!(out.objects.len(), 2, "two carriers, two objects; the native transaction is not one");
        assert!(out.skipped.is_empty());
        assert_eq!(
            out.objects[0].object,
            PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC1), receipts: Vec::new() },
            "and it is the object the payload carried"
        );
        assert_eq!(out.objects[0].carrier, first.id(), "attributed to the transaction that carried it");
        // Acceptance ORDER is consensus: the transition folds them in this sequence.
        assert_eq!(out.objects[1].carrier, second.id());
    }

    /// **Audit M-01: the two doors nobody could authenticate are shut, and say so.**
    ///
    /// `BondRetireRequested` names a bond key — a PUBLIC premine outpoint — and carried no owner
    /// signature, so one ordinary transaction from any stranger retired any bond, permanently and
    /// with no inverse. `ClassFrozen`'s contradiction certificate has signatures the shape check
    /// explicitly defers to "the acceptance layer", which had no arm for the object at all.
    #[test]
    fn the_two_unauthenticated_objects_may_not_ride() {
        for (object, needle) in [
            (PalwConsensusObjectV2::BondRetireRequested { bond: bond(3), signature: Vec::new() }, "must carry the owner signature"),
            // Built through the transition's own `#[cfg(test)]` fixture, because ADR-0063 SA-5
            // left `ClassFrozen` with no constructor outside `palw_state_v2` — this test could
            // not spell the object by hand even to prove the door is shut, which is the point.
            (crate::palw_state_v2::tests::freeze(h64(1)), "no layer verifies"),
        ] {
            let err = palw_lifecycle_object_may_ride_v2(&object).unwrap_err();
            assert!(err.contains(needle), "the refusal must say why: got {err}");
            let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(object)]);
            assert!(out.objects.is_empty(), "and it must not reach the transition");
            assert_eq!(out.skipped.len(), 1, "skipped with its reason, not silently dropped");
        }
    }

    /// **ADR-0152 R-3 (S-7): an unsigned reporter commitment may not ride; a signed one and a
    /// reveal (unsigned by design) may.** This table refuses the shape only; the acceptance layer
    /// verifies the signature against the reporter bond's registered key.
    #[test]
    fn an_unsigned_reporter_commitment_may_not_ride() {
        let unsigned = PalwConsensusObjectV2::ReporterCommitted { commitment: h64(0x53), reporter: bond(3), signature: Vec::new() };
        let err = palw_lifecycle_object_may_ride_v2(&unsigned).unwrap_err();
        assert!(err.contains("must carry the reporter's signature"), "the refusal must say why: got {err}");
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(unsigned)]);
        assert!(out.objects.is_empty(), "it must not reach the transition");
        assert_eq!(out.skipped.len(), 1, "skipped with its reason, not silently dropped");
        let signed = PalwConsensusObjectV2::ReporterCommitted { commitment: h64(0x53), reporter: bond(3), signature: vec![1; 8] };
        palw_lifecycle_object_may_ride_v2(&signed).expect("a present signature is the shape; acceptance verifies it");
        let reveal = PalwConsensusObjectV2::ReporterRevealed { offence_key: h64(0x54), reporter: bond(3), salt: [5; 32] };
        palw_lifecycle_object_may_ride_v2(&reveal).expect("a reveal carries no signature by design");
    }

    /// The three kinds that may not ride, each for its own reason, each skipped with that reason
    /// rather than silently dropped.
    #[test]
    fn objects_that_must_not_ride_are_skipped_with_their_own_reason() {
        let bond_registration = PalwConsensusObjectV2::BondRegistered {
            bond: bond(9),
            pubkey: vec![7; 4],
            operator_pubkey: vec![21; 8],
            // The number that would be free: nothing on this path locks a UTXO behind it.
            collateral: 1_000_000_000_000,
            payout_payload: h64(0x9A11),
            capable_classes: Default::default(),
            signature: Vec::new(),
        };
        // A class registration with NO admission material: still refused, because there is
        // nothing to check it with (ADR-0049 Decision H replaced the blanket refusal with a gate,
        // and a gate needs an input).
        let class_registration = PalwConsensusObjectV2::ClassRegistered {
            class_id: h64(0xC1A55),
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 1,
            // The rule genesis refuses, arriving through the side door.
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(u64::MAX),
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let fp = PalwConsensusObjectV2::FreePromptCommitted {
            job_pin: kaspa_hashes::Hash64::default(),
            eval: None,
            claim: h64(0xF1),
            class_id: h64(1),
            bond: bond(1),
            // Any key: this test asks whether the object may ride a carriage, which is decided
            // before any state is consulted.
            executor_pubkey: vec![7; 4],
            work_leaves: 8,
            prompt_token_ids_hash: h64(0x7E),
            // Unread here: this test asks whether the object may ride a carriage, which is decided
            // before any state, any height and any fence is consulted.
            prompt_tokens: 0,
            prompt_token_ids: Vec::new(),
            decode_tokens_executed: 2,
            trace_root: h64(41),
            output_root: h64(42),
            execution_root: h64(43),
            trace_chunk_count: 4,
            trace_retention_daa: 99,
            consumed_prefix_state: crate::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(h64(1)),
        };
        // The panel binding: excluded for a different reason than the other three — not because
        // it moves value, but because the chain already derives it and one question gets one
        // answer.
        let panel = panel_bound();
        for object in [bond_registration, class_registration, fp, panel] {
            let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(object.clone())]);
            assert!(out.objects.is_empty(), "{object:?} must not enter a chain here");
            assert_eq!(out.skipped.len(), 1, "and the drop is reported, not silent");
        }
    }

    /// **ADR-0049 Decision H: one registration policy, and it is a gate.**
    ///
    /// Three policies coexisted — the carriage refused `ClassRegistered` outright,
    /// `verify_class_admission_v2` would have admitted it at the minimum grantable share, and the
    /// state machine implements a weightless activation clock. The carriage's objection was the
    /// right one and it was a statement about CHECKING, so the refusal is replaced by the gate:
    /// a registration that carries the graph and the canonical job RIDES, and the acceptance layer
    /// decides whether the graph covers, fits the ladder, costs what the ruleset allows and counts
    /// the pwu it declares.
    ///
    /// What this layer owns is the shape: carried material rides, missing material does not.
    #[test]
    fn a_class_registration_rides_only_when_it_carries_what_checks_it() {
        use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
        use crate::palw_state_v2::PalwClassAdmissionCarriageV2;

        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's own graph");
        let carriage = PalwClassAdmissionCarriageV2 {
            canonical: rc_job_context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1),
            profile: profile.clone(),
            // The ride list checks SHAPE only; who signed is the acceptance layer's, where the
            // registrant bond is resolved against chain state.
            registrant_bond: bond(1),
            signature: Vec::new(),
        };
        let registration = |admission: Option<Box<PalwClassAdmissionCarriageV2>>| PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            artifact_root: h64(0xA7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 4_096 },
            initial_target: u128::MAX / 2,
            share_permille: 1,
            activation_daa: 0,
            admission,
        };

        // Carried: it rides, and the acceptance layer gets something to check.
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(registration(Some(Box::new(carriage))))]);
        assert_eq!(out.objects.len(), 1, "a checkable registration reaches the chain");
        assert!(out.skipped.is_empty());

        // Missing: refused HERE, because the gate downstream has no input. The reason names the
        // material rather than the policy — the policy is now "check it", not "never".
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[lifecycle_tx(registration(None))]);
        assert!(out.objects.is_empty());
        assert!(out.skipped[0].1.contains("shape profile"), "got {:?}", out.skipped[0].1);
    }

    /// A payload that does not decode, or names another wire version, contributes no object and
    /// does not reject the block — the walk is total over whatever was accepted.
    #[test]
    fn a_malformed_carrier_is_skipped_with_a_reason() {
        let garbage = carrier(SUBNETWORK_ID_PALW_LIFECYCLE, vec![0xFF; 8]);
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[garbage]);
        assert!(out.objects.is_empty());
        assert_eq!(out.skipped[0].1, "payload does not decode");

        let wrong_version = {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: 99, object: panel_bound() }).unwrap();
            carrier(SUBNETWORK_ID_PALW_LIFECYCLE, payload)
        };
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[wrong_version]);
        assert!(out.objects.is_empty());
        assert_eq!(out.skipped[0].1, "payload names an unsupported wire version");
    }

    /// A lifecycle payload routed to the free-prompt subnetwork is not a lifecycle object: the
    /// band ids exist so one band's payload never reaches another band's validator.
    #[test]
    fn the_band_id_is_what_selects_this_walk() {
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: panel_bound() }).unwrap();
        let misrouted = carrier(SUBNETWORK_ID_PALW_FP_COMMITMENT, payload);
        let out = palw_lifecycle_objects_from_accepted_txs_v2(&[misrouted]);
        assert!(out.objects.is_empty(), "this walk reads its own band and nothing else");
        assert!(out.skipped.is_empty(), "and a foreign band is not even a skip — it is not addressed to us");
    }

    /// **Admission and extraction must give ONE answer.**
    ///
    /// The transaction validator decides what may be in a block; this walk decides what a block's
    /// contents mean. If the two disagreed in the permissive direction a carrier would be admitted
    /// and then silently dropped (the "reads as nothing" failure); in the strict direction a
    /// carrier the walk would have credited could never reach a block at all. Both are closed by
    /// running one table from one place — asserted here over every case the pair can see, rather
    /// than left to the fact that today they call the same function.
    #[test]
    fn admission_accepts_exactly_what_the_walk_extracts() {
        let cases: Vec<Vec<u8>> = vec![
            // Rides.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::ReceiptLicensed { claim: h64(0xC1), receipts: Vec::new() },
            })
            .unwrap(),
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::BondRetireRequested { bond: bond(3), signature: vec![0xEE; 8] },
            })
            .unwrap(),
            // Does not ride: the chain derives panel bindings.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: panel_bound() }).unwrap(),
            // Does not ride: a declared collateral nothing on this path locks.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::BondRegistered {
                    bond: bond(9),
                    pubkey: vec![3u8; 8],
                    operator_pubkey: vec![5u8; 8],
                    collateral: 1_000_000,
                    payout_payload: h64(0x1234),
                    capable_classes: Default::default(),
                    signature: Vec::new(),
                },
            })
            .unwrap(),
            // Wrong wire version.
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: 99, object: panel_bound() }).unwrap(),
            // Undecodable.
            vec![0xFF; 8],
        ];
        for payload in cases {
            let admitted = validate_palw_lifecycle_tx(&payload, true).is_ok();
            let extracted = !palw_lifecycle_objects_from_accepted_txs_v2(&[carrier(SUBNETWORK_ID_PALW_LIFECYCLE, payload.clone())])
                .objects
                .is_empty();
            // **A-2: admission never REJECTS a payload the walk folds** (`extracted ⇒ admitted`).
            // It no longer holds the reverse: a payload the walk SKIPS — one this build cannot
            // decode, or that names a wire version it does not know — is now TOLERATED at isolation
            // (Ok) rather than failing the block, because the walk folds nothing for it and an older
            // build must not split from a newer one over an appended object kind. A decodable,
            // current-version object that may-not-ride is still rejected, and the walk still skips
            // it, so those two agree as before.
            assert!(!extracted || admitted, "admission rejected a payload the walk extracts: {payload:?}");
        }
        // The two cases where admission is now deliberately more tolerant than extraction: an
        // unknown wire version, and bytes this build cannot decode. Both are Ok at isolation and
        // fold nothing.
        let unknown_version = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: 99, object: panel_bound() }).unwrap();
        assert!(
            validate_palw_lifecycle_tx(&unknown_version, true).is_ok(),
            "an unknown wire version is tolerated at isolation on an audit-armed ruleset (A-2)"
        );
        assert!(
            validate_palw_lifecycle_tx(&[0xFFu8; 8], true).is_ok(),
            "an undecodable payload is tolerated at isolation on an audit-armed ruleset (A-2)"
        );
        // The other side of the fence: a ruleset that has NOT declared the audit fence refuses both
        // exactly as every build in the field does today, so a fenced build and an unfenced one
        // agree that such a carrier is block-invalid until the audit ruleset ships.
        assert!(
            matches!(validate_palw_lifecycle_tx(&unknown_version, false), Err(PalwLifecycleTxError::UnsupportedVersion { .. })),
            "an unknown wire version is block-invalid without the audit ruleset"
        );
        assert!(
            matches!(validate_palw_lifecycle_tx(&[0xFFu8; 8], false), Err(PalwLifecycleTxError::Undecodable)),
            "an undecodable payload is block-invalid without the audit ruleset"
        );
    }

    /// ADR-0075: both certification objects ride the lifecycle subnetwork — admission and
    /// extraction give one answer — and neither needs a carrier-bound outpoint, so the object
    /// extracted is the object carried, byte for byte.
    #[test]
    fn certification_objects_ride_and_extract_unchanged() {
        use crate::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
        use crate::palw_e2e_adjudicability::{PalwE2eDrillEvidenceV1, palw_e2e_family_id_v1};
        use crate::palw_state_v2::{PalwCertificationEvidenceV1, PalwCertifiedLaneV1};

        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor's profile");
        let bind = PalwConsensusObjectV2::ClassLaneCertified {
            class_id: profile.shape_profile_id(),
            lane: PalwCertifiedLaneV1::FreePrompt,
            profile: Box::new(profile.clone()),
        };
        let family = PalwConsensusObjectV2::FamilyCertified {
            evidence: Box::new(PalwCertificationEvidenceV1::Attempt(PalwE2eDrillEvidenceV1 {
                family_id: palw_e2e_family_id_v1("RIDES"),
                profile,
                artifact_root: h64(9),
                vectors: Vec::new(),
                malformed_inputs_refused: 0,
            })),
        };
        for object in [bind, family] {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() })
                .expect("serializes");
            validate_palw_lifecycle_tx(&payload, true).expect("a certification object may ride");
            let tx = carrier(SUBNETWORK_ID_PALW_LIFECYCLE.clone(), payload);
            let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
            assert!(extracted.skipped.is_empty(), "{:?}", extracted.skipped);
            assert_eq!(extracted.objects.len(), 1);
            assert_eq!(extracted.objects[0].object, object, "extracted unchanged — nothing is keyed to the carrier");
            assert_eq!(extracted.objects[0].carrier, tx.id());
        }
    }

    /// ADR-0078: a derivation rides the lifecycle subnetwork with its signature and its shape,
    /// extracts unchanged, and is refused — admission and extraction agreeing — when the signature
    /// is missing or the shape is wrong.
    #[test]
    fn derived_artifacts_ride_signed_and_shaped_and_extract_unchanged() {
        use crate::palw_derived_v1::{
            PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN, PALW_DERIVED_V1_SIGNATURE_LEN, PALW_DERIVED_V1_VERSION, PalwDerivedArtifactV1, kind,
        };
        let object = PalwDerivedArtifactV1 {
            version: PALW_DERIVED_V1_VERSION,
            network_domain: h64(1),
            claim_id: h64(2),
            output_root: h64(3),
            grammar_id: h64(4),
            transformer_id: h64(5),
            kind: kind::MUSIC,
            dsl_hash: h64(6),
            artifact_hash: h64(7),
            artifact_bytes: 99,
            executor_pubkey: vec![9; PALW_DERIVED_V1_EXECUTOR_PUBKEY_LEN],
        };
        let signed = PalwConsensusObjectV2::DerivedArtifactV1 {
            object: Box::new(object.clone()),
            signature: vec![1; PALW_DERIVED_V1_SIGNATURE_LEN],
        };
        let payload =
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: signed.clone() }).unwrap();
        validate_palw_lifecycle_tx(&payload, true).expect("a signed, shaped derivation may ride");
        let tx = carrier(SUBNETWORK_ID_PALW_LIFECYCLE.clone(), payload);
        let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
        assert!(extracted.skipped.is_empty(), "{:?}", extracted.skipped);
        assert_eq!(extracted.objects[0].object, signed, "extracted unchanged");

        let unsigned = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature: Vec::new() };
        let mut zero_kind = object.clone();
        zero_kind.kind = 0;
        let unshaped = PalwConsensusObjectV2::DerivedArtifactV1 {
            object: Box::new(zero_kind),
            signature: vec![1; PALW_DERIVED_V1_SIGNATURE_LEN],
        };
        // **X1: a free-length signature is where a GLB would go.** A refusal at the ACCEPTANCE
        // layer drops the object and lets the block stand, so bytes refused there still ride an
        // accepted transaction forever; this list is a block rule, so it is where "under any
        // size" is enforced. A 4 MiB signature is refused by name, exactly like a 16-byte one.
        let overlong = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature: vec![0xAB; 4 << 20] };
        let short = PalwConsensusObjectV2::DerivedArtifactV1 { object: Box::new(object.clone()), signature: vec![1; 16] };
        for (refused, why) in
            [(unsigned, "signature"), (unshaped, "kind 0"), (overlong, "free-length field"), (short, "free-length field")]
        {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: refused }).unwrap();
            let err = validate_palw_lifecycle_tx(&payload, true).expect_err("refused at admission");
            assert!(format!("{err:?}").contains(why), "{err:?}");
            let tx = carrier(SUBNETWORK_ID_PALW_LIFECYCLE.clone(), payload);
            let extracted = palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(&tx));
            assert!(extracted.objects.is_empty(), "and skipped by the walk");
            assert_eq!(extracted.skipped.len(), 1);
        }
    }

    /// **The seed testnet-11 carried at DAA 1,945 still decodes as a seed.**
    ///
    /// Carrier `a1fb54e5…` in block `93eb20f0…`, the one model-market seed on the live chain, was
    /// written by a build whose `PalwConsensusObjectV2` ended at `ModelSeed` — discriminant 0x24.
    /// ADR-0095 first shipped `ModelLineBenefitsDeclared` in the middle of the enum, which made
    /// 0x24 `ModelEvaluationPosted`: this payload failed admission, the block failed with it, and
    /// every node that started from an empty datadir on that build stopped at DAA 1,945 for good
    /// (`block has missing parents: [93eb20f0…]` on each IBD retry). Nodes that had already
    /// accepted the block kept running, which is why nothing looked wrong from the fleet.
    ///
    /// The bytes are the chain's, copied out of the block. Re-encoding a seed with the current
    /// enum and decoding it back is exactly the round trip that cannot see this.
    #[test]
    fn the_seed_testnet_11_carried_at_daa_1945_still_decodes() {
        const PAYLOAD: &str = "0100241c87442e15e86143fb3ac44bc774cf70d3874329c1abd2e9cd66843f1458119f17479343a35f83ad1b027bf2e1e5890ffc920ee68d1560b42fbe16fe3644f2ce6cbcd66f6f69184783c737727f72d41629a55c199a54e0237d69df5a4cef33d8181c4e6ca50da268569d9971b6810aeab93c8f5ebe779d71da81bc7ef824702b00a0724e1809000001000000";
        let mut payload = vec![0u8; PAYLOAD.len() / 2];
        faster_hex::hex_decode(PAYLOAD.as_bytes(), &mut payload).unwrap();
        assert_eq!(payload.len(), 143);

        validate_palw_lifecycle_tx(&payload, true)
            .expect("the chain accepted this carrier at DAA 1,945; a build that refuses it cannot sync");
        let decoded: PalwLifecycleTxPayloadV2 = borsh::from_slice(&payload).unwrap();
        assert_eq!(decoded.version, PALW_LIFECYCLE_TX_VERSION_V2);
        let PalwConsensusObjectV2::ModelSeed { line_id, seeder: _, msk_seed, sink_index } = decoded.object else {
            panic!("0x24 is ModelSeed on the chain, decoded as {:?}", decoded.object)
        };
        assert_eq!(line_id.as_byte_slice(), &payload[3..67]);
        assert_eq!(msk_seed, 10_000_000_000_000, "the 100,000 MSK the carrier paid into the sink");
        assert_eq!(sink_index, 1);
        assert_eq!(borsh::to_vec(&decoded).unwrap(), payload, "and it re-encodes to the chain's bytes");
    }

    pub(crate) fn checkpoint_accusation() -> crate::palw_checkpoint_court_v1::PalwCheckpointAccusationV1 {
        crate::palw_checkpoint_court_v1::PalwCheckpointAccusationV1 {
            version: 1,
            claim: h64(15),
            execution_root: h64(16),
            trace_root: h64(17),
            executor_bond: bond(8),
            accuser_bond: bond(9),
            binding: shard_accusation().refutation.binding,
            anchor: crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1 {
                leaf: crate::palw_step_leg::PalwCheckpointLeafV2 {
                    version: 2,
                    checkpoint_index: 0,
                    covered_decode_call: 1,
                    prev_checkpoint_leaf_hash: h64(18),
                    state_chunk_count: 1,
                    state_chunks_root: h64(19),
                },
                opening: crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: h64(20), siblings: vec![] },
            },
            chunk: crate::palw_attn_court_v1::PalwAttnChunkOpeningV1 { chunk_index: 0, chunk_bytes: vec![0; 4], siblings: vec![] },
            kind: 0,
            attn_layer: 0,
            position: 0,
            rows: Vec::new(),
            signature: vec![7u8; 3],
        }
    }

    pub(crate) fn shard_accusation() -> crate::palw_shard_court_v1::PalwShardCourtAccusationV1 {
        let (binding, _, _, _) = crate::palw_step_refute::tests::base0_honest_decode_commitment();
        crate::palw_shard_court_v1::PalwShardCourtAccusationV1 {
            version: 1,
            claim: h64(8),
            execution_root: binding.committed_execution_root,
            trace_root: h64(9),
            executor_bond: bond(4),
            accuser_bond: bond(5),
            leaf_index: 0,
            refutation: crate::palw_step_refute::PalwExecutionStepRefutationV1 {
                binding,
                output_opening: crate::palw_step_leg::PalwStepOpeningV1 { leaf_index: 0, leaf_hash: h64(10), siblings: vec![] },
                output_preimage: crate::palw_step_leg::PalwStepTileLeafV1 {
                    version: 1,
                    coord: crate::palw_step::PalwStepCoordinateV1 { call_index: 0, position: 0, node_slot: 0, tile_index: 0 },
                    value_count: 0,
                    values_le: vec![],
                },
                inputs: vec![],
                prompt_token_ids: vec![],
                decode_tokens: None,
                kv_checkpoint: None,
            },
            artifact_openings: vec![],
            prompt_ids_opening: None,
            signature: vec![7u8; 3],
        }
    }

    /// **Every lifecycle kind keeps the discriminant a carrier already put on the chain.**
    ///
    /// Borsh numbers an enum's variants by POSITION, and a lifecycle carrier's payload carries that
    /// number, so a variant inserted anywhere but the end renumbers every kind below it and turns
    /// history into bytes this build reads as something else (see the test above). Pinning the
    /// tail catches an insertion anywhere above it; a new kind is appended and given the next
    /// number here.
    #[test]
    fn consensus_object_discriminants_are_the_ones_the_chain_carries() {
        let line = h64(0x11);
        let sig = || vec![7u8; 3];
        let pinned: Vec<(u8, PalwConsensusObjectV2)> = vec![
            (29, PalwConsensusObjectV2::ModelVersionWithdrawn { line_id: line, version: 1, signature: sig() }),
            (
                30,
                PalwConsensusObjectV2::ModelLineRolesSet {
                    line_id: line,
                    developer: None,
                    maintainer: None,
                    contributor_permille_of_leg: 0,
                    signature: sig(),
                },
            ),
            (31, PalwConsensusObjectV2::ModelLineOwnerTransferred { line_id: line, new_owner: bond(1), signature: sig() }),
            (32, PalwConsensusObjectV2::ModelLineRetired { line_id: line, signature: sig() }),
            (
                33,
                PalwConsensusObjectV2::ModelProposalPosted {
                    line_id: line,
                    root: h64(2),
                    note_hash: h64(3),
                    by: bond(2),
                    signature: sig(),
                },
            ),
            (34, PalwConsensusObjectV2::ModelProposalClosed { line_id: line, proposal_id: h64(4), signature: sig() }),
            (
                35,
                PalwConsensusObjectV2::ModelEvaluationPosted {
                    line_id: line,
                    version: 1,
                    evaluator_id: h64(5),
                    score_permille: 1,
                    report_hash: h64(6),
                    by: bond(3),
                    signature: sig(),
                },
            ),
            (36, PalwConsensusObjectV2::ModelSeed { line_id: line, seeder: h64(7), msk_seed: 1, sink_index: 1 }),
            (
                37,
                PalwConsensusObjectV2::ModelLineBenefitsDeclared {
                    line_id: line,
                    tiers: Vec::new(),
                    cadence_daa: 1,
                    expires_daa: 2,
                    signature: sig(),
                },
            ),
            // ADR-0099 Decision 5 / ADR-0100: the one-move court, appended after main's last.
            (38, PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(shard_accusation()) }),
            // ADR-0100 Decision 4, appended after it.
            (39, PalwConsensusObjectV2::ClassShardPlanDeclared { class_id: line, shard_count: 2, signature: sig() }),
            (
                40,
                PalwConsensusObjectV2::BondShardsDeclared {
                    bond: bond(6),
                    class_id: line,
                    shard_count: 2,
                    shards: vec![0],
                    signature: sig(),
                },
            ),
            (
                41,
                PalwConsensusObjectV2::ShardReceiptLicensed {
                    part: crate::palw_shard_licensing_v1::PalwShardReceiptPartV1 {
                        claim: h64(12),
                        shard_count: 2,
                        shard_index: 0,
                        receipts: Vec::new(),
                    },
                },
            ),
            // 42 is ADR-0093 Decision 8's `CourtAttnRootClaimedAnchored`, which the testnet-11 fleet
            // runs (pinned beside its own fixture in palw_state_v2's tests). ADR-0103's checkpoint
            // court and the held DA court's two moves were written as 42-44 on a branch that did not
            // have it, and are appended after it where the two branches meet.
            (43, PalwConsensusObjectV2::CheckpointAccused { accusation: Box::new(checkpoint_accusation()) }),
            (
                44,
                PalwConsensusObjectV2::DefaultAccusedHeld {
                    accusation: Box::new(crate::palw_held_da_v1::PalwHeldAccusationV1 {
                        version: 1,
                        claim: h64(13),
                        missing: crate::palw_held_da_v1::PalwHeldMissingV1::PromptIdsTile { tile: 0 },
                        accuser: bond(7),
                        binding: shard_accusation().refutation.binding,
                        signature: sig(),
                    }),
                },
            ),
            (
                45,
                PalwConsensusObjectV2::MaterialDisclosedHeld {
                    disclosure: Box::new(crate::palw_held_da_v1::PalwHeldDisclosureCarriageV1 {
                        version: 1,
                        claim: h64(13),
                        missing: crate::palw_held_da_v1::PalwHeldMissingV1::StepRange { first: 0, count: 1 },
                        binding: shard_accusation().refutation.binding,
                        disclosure: crate::palw_held_da_v1::PalwHeldDisclosureV1::StepRange {
                            opening: crate::palw_step_leg::PalwStepRangeOpeningV1 {
                                first_leaf_index: 0,
                                leaf_hashes: vec![h64(14)],
                                siblings: vec![],
                            },
                        },
                        signature: sig(),
                    }),
                },
            ),
        ];
        // ADR-0125 §7.3, appended after the held regime's answer.
        let signed = |pre: u64| crate::palw_execution_lane_v1::PalwExecSignedRoundV1 {
            pre_pow_hash: h64(pre),
            timestamp_ms: 0,
            nonce: 0,
            signature: vec![1; crate::palw_execution_lane_v1::PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        };
        let pinned = pinned.into_iter().chain([
            (
                46,
                PalwConsensusObjectV2::RoundPermitEquivocated {
                    evidence: Box::new(crate::palw_execution_lane_v1::PalwExecEquivocationV1 {
                        version: 1,
                        span: 0,
                        round: 0,
                        permit_index: 0,
                        bond: bond(7),
                        first: signed(15),
                        second: signed(16),
                    }),
                },
            ),
            (
                51,
                PalwConsensusObjectV2::ObjectiveOffence {
                    kind: crate::palw_offence_v1::PalwOffenceKindV1::ExecutorEquivocation,
                    accused: bond(7),
                    evidence_id: h64(17),
                    evidence: vec![1],
                },
            ),
            (52, PalwConsensusObjectV2::OptimisticLicensed { claim: h64(18), receipts: Vec::new() }),
            // ADR-0152 v3.1 §6 row 24, the v22 skeleton: appended at frozen tags.
            (53, PalwConsensusObjectV2::ReporterCommitted { commitment: h64(19), reporter: bond(7), signature: sig() }),
            (54, PalwConsensusObjectV2::ReporterRevealed { offence_key: h64(20), reporter: bond(7), salt: [5u8; 32] }),
            (
                55,
                PalwConsensusObjectV2::MaterialDisclosedV2 {
                    claim: h64(21),
                    unit: crate::palw_da_rcore_v1::PalwDaUnitV1::Held(crate::palw_held_da_v1::PalwHeldMissingV1::StepRange {
                        first: 0,
                        count: 1,
                    }),
                    answer: crate::palw_da_rcore_v1::PalwDaAnswerV1::Held(Box::new(
                        crate::palw_held_da_v1::PalwHeldDisclosureCarriageV1 {
                            version: 1,
                            claim: h64(21),
                            missing: crate::palw_held_da_v1::PalwHeldMissingV1::StepRange { first: 0, count: 1 },
                            binding: shard_accusation().refutation.binding,
                            disclosure: crate::palw_held_da_v1::PalwHeldDisclosureV1::StepRange {
                                opening: crate::palw_step_leg::PalwStepRangeOpeningV1 {
                                    first_leaf_index: 0,
                                    leaf_hashes: vec![h64(14)],
                                    siblings: vec![],
                                },
                            },
                            signature: Vec::new(),
                        },
                    )),
                    discloser: bond(7),
                    signature: sig(),
                },
            ),
            (56, PalwConsensusObjectV2::PanelUnavailableQuorum { claim: h64(22), receipts: Vec::new() }),
            // 57 is ADR-0152 §4-ter's `CourtAttnRootClaimedHeld`, pinned beside the held drill that
            // builds one in palw_state_v2's tests (`t_a2_the_held_root_claim_opens_the_phase_…`), as
            // 42 is beside its own.
        ]);
        for (discriminant, object) in pinned {
            assert_eq!(borsh::to_vec(&object).unwrap()[0], discriminant, "{object:?}");
        }
    }

    /// **A-2 uniformity (the A2U review): every kind the live testnet-12 build cannot decode is owned by a fence and rides unjudged
    /// below it.**
    mod a2u {
        use super::super::*;
        use crate::config::params::ForkActivation;
        use crate::palw_state_v2::PalwBondKeyV2;
        use kaspa_hashes::Hash64;

        fn bond() -> PalwBondKeyV2 {
            PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(7), 0))
        }

        /// **The smallest value of `T` a near-zero body decodes to**: a zero-filled body (every length 0, every option `None`, every
        /// nested enum its first variant), with each byte borsh refuses stepped up until it decodes — an enum whose first variant is
        /// `= 1` (`SubjectKindV1`) refuses tag 0, and the byte it refused is the last one it read. `None` when no stepping decodes.
        pub(crate) fn minimal_decode<T: borsh::BorshDeserialize>(prefix: &[u8]) -> Option<T> {
            let mut bytes = prefix.to_vec();
            bytes.extend_from_slice(&[0u8; 16_384]);
            for _ in 0..4_096 {
                let mut reader = &bytes[..];
                match T::deserialize(&mut reader) {
                    Ok(value) => return Some(value),
                    Err(_) => {
                        let refused = (bytes.len() - reader.len()).checked_sub(1)?;
                        if refused < prefix.len() || bytes[refused] == u8::MAX {
                            return None;
                        }
                        bytes[refused] += 1;
                    }
                }
            }
            None
        }

        fn zeros<T: borsh::BorshDeserialize>() -> T {
            minimal_decode(&[]).expect("a near-zero encoding decodes")
        }

        /// **The kind with object tag `tag`, every field zero** — its tag then a zero-filled body (every length 0, every option
        /// `None`, every nested enum its first variant). Generic over the enum, so a kind a lane adds is in every sweep the moment its
        /// table row exists; a kind whose zero-filled body does not decode needs a hand-built sample here instead (the panic says so).
        pub(crate) fn zero_filled_kind(tag: u8) -> PalwConsensusObjectV2 {
            let object: PalwConsensusObjectV2 = minimal_decode(&[tag]).unwrap_or_else(|| {
                panic!("kind {tag} decodes from no near-zero body; give it a hand-built sample in the A2U sweeps instead")
            });
            assert_eq!(borsh::to_vec(&object).unwrap()[0], tag);
            object
        }

        /// One well-formed, signed object of every kind added after the live build, by tag.
        pub(crate) fn new_kind_samples() -> Vec<(u8, PalwConsensusObjectV2)> {
            use PalwConsensusObjectV2 as O;
            let h = Hash64::from_bytes([3; 64]);
            let registration = O::ClassRegistered {
                class_id: h,
                artifact_root: h,
                slash_value_per_pwu: 1,
                pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(10),
                initial_target: 1,
                share_permille: 0,
                activation_daa: 0,
                admission: None,
            };
            vec![
                (104, O::ArtifactBoundV1 { v2_class: h, kernel_param_root: h, signer: bond(), signature: vec![1] }),
                (
                    105,
                    O::ArtifactBindingChallengedV1 {
                        v2_class: h,
                        kernel_param_root: h,
                        challenger: bond(),
                        proof: Box::new(zeros()),
                        signature: vec![1],
                    },
                ),
                (106, O::KernelBoundV1 { v2_class: h, kernel_class: h, challenge_policy_id: h, signer: bond(), signature: vec![1] }),
                (107, O::ConformanceCommittedV1 { commitment: Box::new(zeros()), signer: bond(), signature: vec![1] }),
                (
                    108,
                    O::SignedRegistrationV1 {
                        registration: Box::new(registration),
                        valid_from_daa: 0,
                        valid_until_daa: 10,
                        fork_digest: crate::Hash::from_bytes([4; 32]),
                        signer: bond(),
                        signature: vec![1],
                    },
                ),
                (109, O::ConformanceEvidenceV1 { v2_class: h, action: Box::new(zeros()), signer: bond(), signature: vec![1] }),
                (110, O::KernelRouteV1 { bytes: vec![5; 64], signer: bond(), signature: vec![1] }),
                (111, O::KernelConstraintReceiptV1 { receipt: Box::new(zeros()), signature: vec![1] }),
                (
                    120,
                    O::PanelBeaconProofV3 {
                        proof: Box::new(misaka_palw_panel::BeaconProofV1 { epoch: 1, output: h, proof: vec![6; 32] }),
                    },
                ),
            ]
        }

        /// Every new kind as it can be malformed: unsigned (each kind that carries a signature), oversized (the two with a size bound)
        /// and wrong inside (an empty kernel encoding, an envelope that wraps no registration) — each one a may-ride refusal on its own.
        pub(crate) fn new_kind_malformed() -> Vec<(u8, &'static str, PalwConsensusObjectV2)> {
            use PalwConsensusObjectV2 as O;
            let mut out = Vec::new();
            for (tag, object) in new_kind_samples() {
                let mut unsigned = object.clone();
                let signed = match &mut unsigned {
                    O::ArtifactBoundV1 { signature, .. }
                    | O::ArtifactBindingChallengedV1 { signature, .. }
                    | O::KernelBoundV1 { signature, .. }
                    | O::ConformanceCommittedV1 { signature, .. }
                    | O::SignedRegistrationV1 { signature, .. }
                    | O::ConformanceEvidenceV1 { signature, .. }
                    | O::KernelRouteV1 { signature, .. }
                    | O::KernelConstraintReceiptV1 { signature, .. } => {
                        signature.clear();
                        true
                    }
                    _ => false,
                };
                if signed {
                    out.push((tag, "unsigned", unsigned));
                }
            }
            let h = Hash64::from_bytes([3; 64]);
            out.push((
                110,
                "oversized",
                O::KernelRouteV1 {
                    bytes: vec![5; crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 + 1],
                    signer: bond(),
                    signature: vec![1],
                },
            ));
            out.push((110, "empty encoding", O::KernelRouteV1 { bytes: Vec::new(), signer: bond(), signature: vec![1] }));
            out.push((
                120,
                "oversized",
                O::PanelBeaconProofV3 {
                    proof: Box::new(misaka_palw_panel::BeaconProofV1 {
                        epoch: 1,
                        output: h,
                        proof: vec![6; misaka_palw_panel::MAX_BEACON_PROOF_BYTES_V1 as usize + 1],
                    }),
                },
            ));
            out.push((
                108,
                "wraps no registration",
                O::SignedRegistrationV1 {
                    registration: Box::new(O::KernelBoundV1 {
                        v2_class: h,
                        kernel_class: h,
                        challenge_policy_id: h,
                        signer: bond(),
                        signature: vec![1],
                    }),
                    valid_from_daa: 0,
                    valid_until_daa: 10,
                    fork_digest: crate::Hash::from_bytes([4; 32]),
                    signer: bond(),
                    signature: vec![1],
                },
            ));
            for (_, why, object) in &out {
                assert!(
                    palw_lifecycle_object_may_ride_v2(object).is_err(),
                    "the malformed fixture ({why}) is a may-ride refusal: {object:?}"
                );
            }
            out
        }

        fn payload_of(object: &PalwConsensusObjectV2) -> Vec<u8> {
            borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap()
        }

        /// `(variant, tag)` of every variant of `PalwConsensusObjectV2`, read from its SOURCE — declaration order, Rust's discriminant
        /// rule (explicit `= N`, else the previous plus one).
        fn enum_kinds_from_source() -> Vec<(String, u8)> {
            enum_variants_from_source(include_str!("palw_state_v2.rs"), "PalwConsensusObjectV2")
        }

        /// `(variant, discriminant)` of `pub enum <name>` in `src`, by Rust's rule (explicit `= N`, else the previous plus one).
        fn enum_variants_from_source(src: &str, name: &str) -> Vec<(String, u8)> {
            let head = format!("pub enum {name} {{");
            let start = src.find(&head).expect("the enum") + head.len();
            let bytes = src.as_bytes();
            let (mut i, mut depth) = (start, 0usize);
            let mut out: Vec<(String, u8)> = Vec::new();
            let mut next: u16 = 0;
            let mut pending: Option<String> = None;
            let finish = |pending: &mut Option<String>, explicit: Option<u16>, next: &mut u16, out: &mut Vec<(String, u8)>| {
                if let Some(name) = pending.take() {
                    let tag = explicit.unwrap_or(*next);
                    out.push((name, u8::try_from(tag).expect("a u8 discriminant")));
                    *next = tag + 1;
                }
            };
            while i < bytes.len() {
                let c = bytes[i];
                if bytes[i..].starts_with(b"//") {
                    i += bytes[i..].iter().position(|b| *b == b'\n').unwrap_or(bytes.len() - i);
                    continue;
                }
                if c == b'#' && depth == 0 {
                    // An attribute: skip its brackets.
                    let mut d = 0;
                    while i < bytes.len() {
                        if bytes[i] == b'[' {
                            d += 1;
                        } else if bytes[i] == b']' {
                            d -= 1;
                            if d == 0 {
                                i += 1;
                                break;
                            }
                        }
                        i += 1;
                    }
                    continue;
                }
                match c {
                    b'{' | b'(' | b'[' | b'<' if depth > 0 || c != b'<' => depth += 1,
                    b'}' | b')' | b']' | b'>' if depth > 0 && (c != b'>' || bytes[i - 1] != b'-') => depth -= 1,
                    b'}' if depth == 0 => {
                        finish(&mut pending, None, &mut next, &mut out);
                        return out;
                    }
                    b',' if depth == 0 => finish(&mut pending, None, &mut next, &mut out),
                    b'=' if depth == 0 => {
                        let rest = &src[i + 1..];
                        let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
                        let explicit: u16 = digits.parse().expect("an explicit discriminant");
                        let skip = rest.len() - rest.trim_start().len() + digits.len();
                        finish(&mut pending, Some(explicit), &mut next, &mut out);
                        i += 1 + skip;
                        continue;
                    }
                    _ if depth == 0 && (c as char).is_ascii_uppercase() && pending.is_none() => {
                        let name: String = src[i..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                        i += name.len();
                        pending = Some(name);
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            panic!("the enum's closing brace")
        }

        /// **The table test: every variant of the enum has exactly one owner**, and the tables name exactly the enum's variants. A kind
        /// added without a `PALW_LIFECYCLE_NEW_KINDS_V1` entry fails here (and `palw_lifecycle_kind_owner_v1` does not compile without
        /// an arm); a kind slipped into the frozen live-build list fails `the_int12_kind_list_is_frozen`.
        #[test]
        fn every_kind_in_the_enum_has_exactly_one_owner() {
            let kinds = enum_kinds_from_source();
            assert_eq!(kinds.len(), PALW_LIFECYCLE_INT12_KINDS_V1.len() + PALW_LIFECYCLE_NEW_KINDS_V1.len(), "{kinds:?}");
            for (name, tag) in &kinds {
                let int12 = PALW_LIFECYCLE_INT12_KINDS_V1.iter().filter(|(t, n)| t == tag && n == name).count();
                let new = PALW_LIFECYCLE_NEW_KINDS_V1.iter().filter(|(t, n, _)| t == tag && n == name).count();
                assert_eq!(
                    int12 + new,
                    1,
                    "PalwConsensusObjectV2::{name} (tag {tag}) needs exactly one owner: a kind added after the live testnet-12 build \
                     goes in PALW_LIFECYCLE_NEW_KINDS_V1 with the fence that owns it, and rides unjudged below that fence (A-2 \
                     uniformity, docs/design/palw/a2-uniformity-new-kinds.md)"
                );
            }
            let mut tags: Vec<u8> = kinds.iter().map(|(_, t)| *t).collect();
            tags.sort();
            tags.dedup();
            assert_eq!(tags.len(), kinds.len(), "a tag is one kind's");
            for (tag, name, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
                assert!(!PALW_LIFECYCLE_INT12_KINDS_V1.iter().any(|(t, _)| t == tag), "{name}: tag {tag} was the live build's");
            }
        }

        /// **The live build's list is frozen** — its 100 `(tag, name)` pairs, pinned by an FNV-1a over `"tag:name;"`.
        #[test]
        fn the_int12_kind_list_is_frozen() {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for (tag, name) in PALW_LIFECYCLE_INT12_KINDS_V1 {
                for b in format!("{tag}:{name};").bytes() {
                    h ^= u64::from(b);
                    h = h.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
            assert_eq!(h, 0xa926_dcc7_cfad_3770, "the live build's 100 kinds are what `0b1c11b87` decodes, and never move");
            assert_eq!(PALW_LIFECYCLE_INT12_KINDS_V1.last(), Some(&(103, "TrapRevealedV1")));
        }

        /// The new-kind table IS the owner function: every row's kind (decoded from a zero-filled body, generically, and each
        /// hand-built sample) is owned by the row's fence, never by the live build.
        #[test]
        fn the_new_kind_table_is_the_owner_function() {
            let mut samples = new_kind_samples();
            samples.extend(PALW_LIFECYCLE_NEW_KINDS_V1.iter().map(|(tag, _, _)| (*tag, zero_filled_kind(*tag))));
            for (tag, _, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
                assert!(samples.iter().any(|(t, _)| t == tag), "kind {tag} is sampled");
            }
            for (tag, object) in &samples {
                assert_eq!(borsh::to_vec(object).unwrap()[0], *tag, "{object:?}");
                let (_, _, fence) = PALW_LIFECYCLE_NEW_KINDS_V1.iter().find(|(t, _, _)| t == tag).expect("a table row");
                assert_eq!(palw_lifecycle_kind_owner_v1(object), PalwLifecycleKindOwnerV1::Fence(*fence), "tag {tag}");
                assert!(!PalwLifecycleKindFencesV1::default().kind_in_force_at(object, u64::MAX), "unarmed is never in force");
            }
            for fence in PalwLifecycleKindFenceV1::ALL {
                assert!(PALW_LIFECYCLE_NEW_KINDS_V1.iter().any(|(_, _, f)| *f == fence), "{} owns a kind", fence.params_field());
            }
        }

        /// `PalwLifecycleKindFenceV1::ALL` is the enum, in discriminant order — the activations array is indexed by it, so a variant
        /// missing from `ALL` would never be in force (its kinds would ride unjudged forever: safe, but never armable).
        #[test]
        fn the_fence_list_is_the_enum() {
            let variants = enum_variants_from_source(include_str!("palw_lifecycle_objects_v2.rs"), "PalwLifecycleKindFenceV1");
            assert_eq!(variants.len(), PalwLifecycleKindFenceV1::ALL.len(), "{variants:?}");
            for (index, fence) in PalwLifecycleKindFenceV1::ALL.into_iter().enumerate() {
                assert_eq!(fence as usize, index, "{fence:?}");
                assert_eq!(variants[index].1 as usize, index, "{:?}", variants[index]);
                let armed = PalwLifecycleKindFencesV1::default().with(fence, Some(ForkActivation::new(7)));
                assert_eq!(armed.activation(fence), Some(ForkActivation::new(7)));
                assert!(armed.in_force_at(fence, 7) && !armed.in_force_at(fence, 6));
                assert!(PalwLifecycleKindFenceV1::ALL.iter().filter(|f| **f != fence).all(|f| armed.activation(*f).is_none()));
            }
            let mut p =
                crate::config::params::Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
            p.palw_signed_registration_v1 = Some(ForkActivation::new(9));
            assert_eq!(
                p.palw_lifecycle_kind_fences_v1(),
                PalwLifecycleKindFencesV1::default()
                    .with(PalwLifecycleKindFenceV1::SignedRegistrationV1, Some(ForkActivation::new(9))),
                "Params resolves each fence into its own slot"
            );
        }

        /// The landed half of a table row: does `fence` name a `Params` fence this tree has?
        fn params_has_fence(name: &str) -> bool {
            crate::config::params::Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12))
                .palw_fences_v1()
                .iter()
                .any(|(n, _)| *n == name)
        }

        /// **The central table is consistent with itself and with the code** (`PALW_A2_KIND_FENCE_TABLE_V1`):
        ///
        /// * object-tag and inner-kind rows never overlap one another, nor the live build's tags;
        /// * every landed kind (top level, inner) sits in exactly one row, and that row names the fence the code answers;
        /// * every landed header form sits in its row, with the fence its gate asks;
        /// * every carried change to a live-build wire type sits in an `Int12Inner` row with the same fence;
        /// * a row is `landed` exactly when the code in this tree fills it, and a landed row's fence is a `Params` fence.
        ///
        /// A lane that merges a kind outside its allocation, under another fence, or without flipping its row fails here.
        #[test]
        fn every_landed_kind_sits_in_its_row_with_the_rows_fence() {
            use PalwA2SlotV1 as S;
            let rows = PALW_A2_KIND_FENCE_TABLE_V1;
            let tag_rows: Vec<(u8, u8, &PalwA2RowV1)> = rows
                .iter()
                .filter_map(|r| match r.slot {
                    S::ObjectTags { lo, hi } => Some((lo, hi, r)),
                    _ => None,
                })
                .collect();
            let inner_rows: Vec<(u8, u8, &PalwA2RowV1)> = rows
                .iter()
                .filter_map(|r| match r.slot {
                    S::KernelInner { lo, hi } => Some((lo, hi, r)),
                    _ => None,
                })
                .collect();
            for set in [&tag_rows, &inner_rows] {
                for (i, (lo, hi, row)) in set.iter().enumerate() {
                    assert!(lo <= hi, "{row:?}");
                    for (lo2, hi2, row2) in &set[i + 1..] {
                        assert!(hi < lo2 || hi2 < lo, "overlapping rows: {row:?} and {row2:?}");
                    }
                }
            }
            for (lo, hi, row) in &tag_rows {
                assert!(
                    !PALW_LIFECYCLE_INT12_KINDS_V1.iter().any(|(t, _)| (lo..=hi).contains(&t)),
                    "a row over the live build's tags: {row:?}"
                );
            }
            // Top-level kinds.
            for (tag, name, fence) in PALW_LIFECYCLE_NEW_KINDS_V1 {
                let covering: Vec<_> = tag_rows.iter().filter(|(lo, hi, _)| (lo..=hi).contains(&tag)).collect();
                assert_eq!(covering.len(), 1, "{name} (tag {tag}) sits in exactly one row of PALW_A2_KIND_FENCE_TABLE_V1");
                let (_, _, row) = covering[0];
                assert_eq!(row.fence, fence.params_field(), "{name} (tag {tag}) is owned by its row's fence ({row:?})");
            }
            for (lo, hi, row) in &tag_rows {
                let landed = PALW_LIFECYCLE_NEW_KINDS_V1.iter().any(|(t, _, _)| (lo..=hi).contains(&t));
                assert_eq!(row.landed, landed, "a row is landed exactly when a kind of it is in the enum: {row:?}");
            }
            // Kernel-route inner kinds.
            for (tag, name, fence) in PALW_KERNEL_ROUTE_INNER_KINDS_V1 {
                let covering: Vec<_> = inner_rows.iter().filter(|(lo, hi, _)| (lo..=hi).contains(&tag)).collect();
                assert_eq!(covering.len(), 1, "inner kind {name} ({tag}) sits in exactly one row");
                let (_, _, row) = covering[0];
                let wants = fence.map_or("palw_probabilistic_constraints_v1", |f| f.params_field());
                assert_eq!(row.fence, wants, "inner kind {name} ({tag}): `None` is tag 110's own fence ({row:?})");
            }
            for (lo, hi, row) in &inner_rows {
                let landed = PALW_KERNEL_ROUTE_INNER_KINDS_V1.iter().any(|(t, _, _)| (lo..=hi).contains(&t));
                assert_eq!(row.landed, landed, "{row:?}");
            }
            // Header forms: every owned form has its landed row, and a landed row's (algo, magic) is the form the gate owns.
            for form in crate::pow_layer0::PalwHeaderFormFenceV1::ALL {
                assert!(
                    rows.iter().any(|r| matches!(r.slot, S::HeaderForm { .. }) && r.landed && r.fence == form.params_field()),
                    "{form:?} has its landed HeaderForm row"
                );
            }
            for row in rows {
                if let S::HeaderForm { algo_id, magic } = row.slot {
                    let mut carriage = magic.to_vec();
                    carriage.extend_from_slice(&[0u8; 64]);
                    let owner = crate::pow_layer0::palw_header_form_owner_v1(algo_id, &carriage);
                    assert_eq!(owner.is_some(), row.landed, "{row:?}");
                    if let Some(form) = owner {
                        assert_eq!(form.params_field(), row.fence, "{row:?}");
                    }
                }
            }
            // Carried changes to the live build's wire types.
            for (key, change) in PALW_INT12_WIRE_CHANGES_V1 {
                let fence = match change {
                    PalwInt12WireChangeV1::CarriedAppended { fence, .. } | PalwInt12WireChangeV1::CarriedReread { fence, .. } => fence,
                    PalwInt12WireChangeV1::ObjectEnum | PalwInt12WireChangeV1::NotCarried(_) => continue,
                };
                assert!(
                    rows.iter().any(|r| r.slot == S::Int12Inner { key: *key } && r.fence == *fence && r.landed),
                    "{key}: a carried change has its landed Int12Inner row naming {fence}"
                );
            }
            for row in rows {
                if let S::Int12Inner { key } = row.slot {
                    let classified = PALW_INT12_WIRE_CHANGES_V1.iter().any(|(k, c)| {
                        *k == key
                            && matches!(c, PalwInt12WireChangeV1::CarriedAppended { .. } | PalwInt12WireChangeV1::CarriedReread { .. })
                    });
                    assert_eq!(row.landed, classified, "{row:?}");
                }
            }
            // A landed row's fence exists in this tree.
            for row in rows.iter().filter(|r| r.landed) {
                assert!(params_has_fence(row.fence), "a landed row names a Params fence: {row:?}");
            }
            let pending: Vec<String> =
                rows.iter().filter(|r| !r.landed).map(|r| format!("{:?} → {} ({})", r.slot, r.fence, r.owner)).collect();
            eprintln!("[a2u] rows awaiting their lane's merge:\n  {}", pending.join("\n  "));
        }

        /// **An appended form inside a live-build kind is owned at the top level**: for every `CarriedAppended` change, a sample object
        /// carrying the new form (listed here by the lane that appends it) is owned by its fence — the guarded arm of
        /// [`palw_lifecycle_kind_owner_v1`] — so every site reads it as the live build does, undecodable, below the fence.
        #[test]
        fn every_appended_form_has_a_guarded_owner_arm() {
            // `(manifest key, an object of an int-12 kind carrying the appended form)` — one per `CarriedAppended` change.
            let samples: Vec<(&str, PalwConsensusObjectV2)> = Vec::new();
            for (key, change) in PALW_INT12_WIRE_CHANGES_V1 {
                let PalwInt12WireChangeV1::CarriedAppended { fence, .. } = change else { continue };
                let carrying: Vec<_> = samples.iter().filter(|(k, _)| k == key).collect();
                assert!(!carrying.is_empty(), "{key}: a sample object carrying the appended form");
                for (_, object) in carrying {
                    match palw_lifecycle_kind_owner_v1(object) {
                        PalwLifecycleKindOwnerV1::Fence(f) => assert_eq!(f.params_field(), *fence, "{key}"),
                        PalwLifecycleKindOwnerV1::Int12 => panic!("{key}: an object carrying the appended form is owned by {fence}"),
                    }
                }
            }
        }

        /// **Below its fence every new kind — well-formed, unsigned, oversized or malformed — is judged at isolation exactly as the live
        /// build judges a payload it cannot decode** (a tag-254 payload is the reference: no build decodes it): tolerated where the
        /// audit fence is declared, `Undecodable` where it is not. Past the fence the header context asks the kind's own may-ride rule,
        /// below it nothing.
        #[test]
        fn below_its_fence_a_new_kind_is_judged_as_the_live_build_judges_undecodable_bytes() {
            let mut reference = borsh::to_vec(&PALW_LIFECYCLE_TX_VERSION_V2).unwrap();
            reference.extend_from_slice(&[254, 1, 2, 3]);
            assert!(borsh::from_slice::<PalwLifecycleTxPayloadV2>(&reference).is_err(), "no build decodes tag 254");
            let mut objects: Vec<(u8, &str, PalwConsensusObjectV2)> =
                new_kind_samples().into_iter().map(|(tag, object)| (tag, "well-formed", object)).collect();
            objects.extend(new_kind_malformed());
            objects.extend(PALW_LIFECYCLE_NEW_KINDS_V1.iter().map(|(tag, _, _)| (*tag, "zero-filled", zero_filled_kind(*tag))));
            for (tag, why, object) in &objects {
                let payload = payload_of(object);
                for tolerate in [true, false] {
                    assert_eq!(
                        validate_palw_lifecycle_tx(&payload, tolerate),
                        validate_palw_lifecycle_tx(&reference, tolerate),
                        "tag {tag} ({why}), tolerate {tolerate}: the live build's verdict"
                    );
                }
                let (_, _, fence) = PALW_LIFECYCLE_NEW_KINDS_V1.iter().find(|(t, _, _)| t == tag).unwrap();
                let unarmed = PalwLifecycleKindFencesV1::default();
                assert_eq!(validate_palw_lifecycle_tx_in_context_v1(&payload, &unarmed, u64::MAX), Ok(()), "unarmed: nothing asked");
                let armed = unarmed.with(*fence, Some(ForkActivation::new(1_000)));
                // Every OTHER fence armed from 0 changes nothing for this kind.
                let others = PalwLifecycleKindFenceV1::ALL
                    .into_iter()
                    .filter(|f| f != fence)
                    .fold(unarmed, |fences, f| fences.with(f, Some(ForkActivation::new(0))));
                assert_eq!(validate_palw_lifecycle_tx_in_context_v1(&payload, &others, u64::MAX), Ok(()), "another kind's fence");
                assert_eq!(
                    validate_palw_lifecycle_tx_in_context_v1(&payload, &armed, 999),
                    Ok(()),
                    "tag {tag} ({why}) below the fence"
                );
                assert_eq!(
                    validate_palw_lifecycle_tx_in_context_v1(&payload, &armed, 1_000),
                    palw_lifecycle_object_may_ride_v2(object).map_err(PalwLifecycleTxError::ObjectMayNotRide),
                    "tag {tag} ({why}) past the fence: its own rule"
                );
                // `never()` is absence.
                let never = armed.with(*fence, Some(ForkActivation::never()));
                assert_eq!(validate_palw_lifecycle_tx_in_context_v1(&payload, &never, u64::MAX), Ok(()));
                assert!(!never.kind_in_force_at(object, u64::MAX) && armed.kind_in_force_at(object, 1_000));
            }
            // A kind the live build knows is judged at isolation as before, at every height, and the header context asks nothing.
            let known = PalwConsensusObjectV2::ModelSell {
                line_id: Hash64::from_bytes([1; 64]),
                holder: Hash64::from_bytes([2; 64]),
                units_in: 1,
                min_msk_out: 0,
                held_units: 1,
                not_after_daa: 0,
                pubkey: Vec::new(),
                signature: Vec::new(),
            };
            assert!(matches!(validate_palw_lifecycle_tx(&payload_of(&known), true), Err(PalwLifecycleTxError::ObjectMayNotRide(_))));
            assert_eq!(
                validate_palw_lifecycle_tx_in_context_v1(&payload_of(&known), &PalwLifecycleKindFencesV1::default(), 0),
                Ok(())
            );
        }

        /// **The fold's and the walk's "undecodable" are the live build's words**: borsh's unknown-discriminant message, which is what
        /// that build's chunk completion reports for a new kind's bytes (`ChunkedObjectUndecodable`).
        #[test]
        fn a_not_in_force_kind_is_undecodable_in_the_live_builds_words() {
            for tag in [200u8, 254] {
                let err = borsh::from_slice::<PalwConsensusObjectV2>(&[tag, 0, 0, 0]).expect_err("no such kind");
                assert_eq!(err.to_string(), palw_lifecycle_unknown_tag_reason_v1(&[tag, 0, 0, 0]));
            }
        }

        /// **Arming an owning fence needs the audit fence declared** — below it isolation could only refuse the kind's bytes.
        #[test]
        fn an_owning_fence_needs_the_audit_fence_declared() {
            let mut p =
                crate::config::params::Params::from(crate::network::NetworkId::with_suffix(crate::network::NetworkType::Testnet, 12));
            assert_eq!(p.palw_lifecycle_kind_fences_v1(), PalwLifecycleKindFencesV1::default(), "testnet-12 arms no owning fence");
            p.validate_palw_lifecycle_kind_fences_v1().expect("nothing armed");
            p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(5_000));
            assert!(p.palw_audit_2026_09_11.is_some(), "testnet-12 declares the audit fence");
            p.validate_palw_lifecycle_kind_fences_v1().expect("declared");
            p.palw_audit_2026_09_11 = None;
            assert!(p.validate_palw_lifecycle_kind_fences_v1().is_err(), "an owning fence without A-2 is refused");
        }

        fn repo_root() -> std::path::PathBuf {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
        }

        /// **The table test one level down: every kernel-route inner kind has exactly one row**, read from `KernelRouteObjectV1`'s
        /// source; and each row's fence is what [`palw_kernel_route_inner_fence_v1`] answers for that kind (a zero-filled body).
        #[test]
        fn every_kernel_inner_kind_has_exactly_one_row() {
            let src = std::fs::read_to_string(repo_root().join("misaka-palw-kernel/src/route.rs")).expect("the kernel route's source");
            let kinds = enum_variants_from_source(&src, "KernelRouteObjectV1");
            assert_eq!(kinds.len(), PALW_KERNEL_ROUTE_INNER_KINDS_V1.len(), "{kinds:?}");
            for (name, tag) in &kinds {
                assert_eq!(
                    PALW_KERNEL_ROUTE_INNER_KINDS_V1.iter().filter(|(t, n, _)| t == tag && n == name).count(),
                    1,
                    "KernelRouteObjectV1::{name} (inner {tag}) needs exactly one row in PALW_KERNEL_ROUTE_INNER_KINDS_V1, naming the \
                     fence below which it is read as undecodable (A-2 uniformity)"
                );
            }
            for (tag, name, fence) in PALW_KERNEL_ROUTE_INNER_KINDS_V1 {
                let object: misaka_palw_kernel::route::KernelRouteObjectV1 =
                    minimal_decode(&[*tag]).unwrap_or_else(|| panic!("inner {tag} ({name}) decodes from a near-zero body"));
                assert_eq!(palw_kernel_route_inner_fence_v1(&object), *fence, "inner {tag} ({name})");
            }
            // A variant appended INSIDE an inner kind is owned by its fence through a guarded arm: R4X's typed-root proof in a filing.
            use misaka_palw_kernel::route::{KernelRouteObjectV1 as K, ProsecutionV1 as P};
            let filing = |proof| K::FileProof { accuser: [1; 64], claim: [2; 64], proof };
            assert_eq!(palw_kernel_route_inner_fence_v1(&filing(P::Spec(vec![9]))), Some(PalwKernelInnerFenceV1::TypedRootsV1));
            assert_eq!(palw_kernel_route_inner_fence_v1(&filing(P::Kernel(vec![9]))), None, "a filing of a kind tag 110 knows");
            assert_eq!(palw_kernel_route_inner_fence_v1(&filing(P::Segmented(vec![9]))), None, "K2S: a segmented fault is tag 110's");
        }

        /// The source directories whose types a payload can carry: consensus-core and every crate it decodes with (the PALW crates
        /// and their own PALW dependencies, hashes, math).
        const WIRE_DIRS: [&str; 11] = [
            "consensus/core/src",
            "crypto/hashes/src",
            "math/src",
            "misaka-palw-tir/src",
            "misaka-palw-tir-exec/src",
            "misaka-palw-tir-lower/src",
            "misaka-palw-tir-sketch/src",
            "misaka-palw-gen/src",
            "misaka-palw-panel/src",
            "misaka-palw-challenge/src",
            "misaka-palw-kernel/src",
        ];

        fn fnv64(bytes: &[u8]) -> u64 {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in bytes {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h
        }

        /// `src` without `//` and `/* */` comments (string and char literals kept whole).
        fn strip_comments(src: &str) -> String {
            let b = src.as_bytes();
            let mut out = Vec::with_capacity(b.len());
            let mut i = 0;
            while i < b.len() {
                if b[i..].starts_with(b"//") {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                } else if b[i..].starts_with(b"/*") {
                    i += 2;
                    while i < b.len() && !b[i..].starts_with(b"*/") {
                        i += 1;
                    }
                    i += 2;
                } else if b[i] == b'"' {
                    out.push(b[i]);
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        if b[i] == b'\\' {
                            out.push(b[i]);
                            i += 1;
                        }
                        if i < b.len() {
                            out.push(b[i]);
                            i += 1;
                        }
                    }
                    if i < b.len() {
                        out.push(b[i]);
                        i += 1;
                    }
                } else if b[i..].starts_with(b"'\"'") {
                    out.extend_from_slice(b"'\"'");
                    i += 3;
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            String::from_utf8_lossy(&out).into_owned()
        }

        /// The end (exclusive) of the bracketed group opening at `open`. String and char literals are skipped whole, so a `"{"` in a
        /// test's assertion cannot unbalance the walk.
        fn group_end(b: &[u8], open: usize) -> usize {
            let (o, c) = match b[open] {
                b'{' => (b'{', b'}'),
                b'(' => (b'(', b')'),
                b'[' => (b'[', b']'),
                _ => (b'<', b'>'),
            };
            let mut depth = 0usize;
            let mut i = open;
            while i < b.len() {
                if b[i] == b'"' && o != b'<' {
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                } else if b[i] == b'\'' && o != b'<' {
                    // A char literal (`'{'`, `'\\''`, `'\\u{..}'`) is skipped; a lifetime (`'a`) is not one.
                    let close = if b.get(i + 1) == Some(&b'\\') {
                        b.get(i + 3..).and_then(|rest| rest.iter().position(|x| *x == b'\'')).map(|k| i + 3 + k)
                    } else if b.get(i + 2) == Some(&b'\'') {
                        Some(i + 2)
                    } else {
                        None
                    };
                    if let Some(close) = close {
                        i = close;
                    }
                } else if b[i] == o {
                    depth += 1;
                } else if b[i] == c && !(c == b'>' && i > 0 && b[i - 1] == b'-') {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1;
                    }
                }
                i += 1;
            }
            b.len()
        }

        /// `text` (comments already stripped) without each item a `#[cfg(test)]` attribute guards — a `mod … { … }` or `mod …;`, a
        /// function, an `impl`, a `use` — wherever it stands: test-only types are not wire types, and a production type after a test
        /// helper still is.
        fn strip_cfg_test_items(text: &str) -> String {
            const ATTR: &str = "#[cfg(test)]";
            let b = text.as_bytes();
            let mut out = String::with_capacity(text.len());
            let mut i = 0;
            while let Some(at) = text[i..].find(ATTR).map(|k| k + i) {
                out.push_str(&text[i..at]);
                // The guarded item ends at its first `;` or at the close of its first `{` — whichever comes first outside brackets
                // and parentheses (`[u8; 4]` in a signature is not the end).
                let mut j = at + ATTR.len();
                let mut depth = 0usize;
                let end = loop {
                    match b.get(j) {
                        None => break b.len(),
                        Some(b'(') | Some(b'[') => depth += 1,
                        Some(b')') | Some(b']') => depth = depth.saturating_sub(1),
                        Some(b';') if depth == 0 => break j + 1,
                        Some(b'{') if depth == 0 => break group_end(b, j),
                        Some(b'"') => {
                            // a string literal inside an attribute or a default
                            j += 1;
                            while j < b.len() && b[j] != b'"' {
                                j += if b[j] == b'\\' { 2 } else { 1 };
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                };
                i = end;
            }
            out.push_str(&text[i..]);
            out
        }

        /// **Every Borsh wire type under `root`'s [`WIRE_DIRS`]** — each derived `struct`/`enum` and each hand-written
        /// `impl BorshDeserialize for …`, keyed `"<path>::<item>"`, valued by an FNV-1a of its declaration with comments and
        /// whitespace removed. Test modules (`#[cfg(test)]` onward, `tests/`) are not wire types.
        fn wire_types_of(root: &std::path::Path) -> std::collections::BTreeMap<String, (u64, String)> {
            fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
                let Ok(entries) = std::fs::read_dir(dir) else { return };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if path.file_name().is_some_and(|n| n != "tests" && n != "benches") {
                            walk(&path, out);
                        }
                    } else if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "tests.rs") {
                        out.push(path);
                    }
                }
            }
            let mut files = Vec::new();
            for dir in WIRE_DIRS {
                walk(&root.join(dir), &mut files);
            }
            files.sort();
            let mut out = std::collections::BTreeMap::new();
            for file in files {
                let rel = file.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                let raw = std::fs::read_to_string(&file).unwrap();
                let text = strip_cfg_test_items(&strip_comments(&raw));
                let b = text.as_bytes();
                let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                let mut insert = |name: String, decl: String| {
                    let mut key = format!("{rel}::{name}");
                    let mut n = 2;
                    while out.contains_key(&key) {
                        key = format!("{rel}::{name}#{n}");
                        n += 1;
                    }
                    out.insert(key, (fnv64(decl.as_bytes()), decl));
                };
                let mut i = 0;
                while let Some(at) = text[i..].find("#[derive(").map(|k| k + i) {
                    let attr_end = group_end(b, at + 1);
                    i = attr_end;
                    let borsh = text[at..attr_end].contains("Borsh");
                    // The attributes around the derive (the borsh and repr ones are part of the wire form): those before it, then
                    // those after it, then the visibility.
                    let mut attrs = String::new();
                    let mut start = at;
                    loop {
                        let before = text[..start].trim_end();
                        if !before.ends_with(']') {
                            break;
                        }
                        let close = before.len() - 1;
                        let mut depth = 0isize;
                        let mut k = close + 1;
                        let open = loop {
                            if k == 0 {
                                break None;
                            }
                            k -= 1;
                            match b[k] {
                                b']' => depth += 1,
                                b'[' => {
                                    depth -= 1;
                                    if depth <= 0 {
                                        break Some(k);
                                    }
                                }
                                _ => {}
                            }
                        };
                        match open {
                            Some(open) if open > 0 && b[open - 1] == b'#' => {
                                let a = &text[open - 1..=close];
                                if a.starts_with("#[borsh") || a.starts_with("#[repr") {
                                    attrs.insert_str(0, a);
                                }
                                start = open - 1;
                            }
                            _ => break,
                        }
                    }
                    let mut j = attr_end;
                    loop {
                        while j < b.len() && b[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        if j < b.len() && b[j] == b'#' {
                            let end = group_end(b, j + 1);
                            let a = &text[j..end];
                            if a.starts_with("#[borsh") || a.starts_with("#[repr") {
                                attrs.push_str(a);
                            }
                            j = end;
                        } else {
                            break;
                        }
                    }
                    if text[j..].starts_with("pub(") {
                        j = group_end(b, j + 3);
                    } else if text[j..].starts_with("pub ") {
                        j += 4;
                    }
                    while j < b.len() && b[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    let kind = if text[j..].starts_with("struct ") && borsh {
                        "struct"
                    } else if text[j..].starts_with("enum ") && (borsh || attrs.contains("#[repr(u") || attrs.contains("#[repr(i")) {
                        // A Borsh enum, or a tag enum read by hand from a byte (`#[repr(u8)]` + `from_tag`): both are wire forms.
                        "enum"
                    } else {
                        continue;
                    };
                    let name_at = j + kind.len() + 1;
                    let name: String = text[name_at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    let mut k = name_at + name.len();
                    while k < b.len() && !matches!(b[k], b'{' | b'(' | b';') {
                        if b[k] == b'<' {
                            k = group_end(b, k);
                        } else {
                            k += 1;
                        }
                    }
                    let end = match b.get(k) {
                        Some(b';') => k + 1,
                        Some(b'(') => {
                            let close = group_end(b, k);
                            close + text[close..].find(';').map_or(0, |s| s + 1)
                        }
                        Some(_) => group_end(b, k),
                        None => b.len(),
                    };
                    insert(name, squash(&format!("{attrs}{}", &text[j..end])));
                    i = end;
                }
                // Hand-written decoders.
                let mut i = 0;
                while let Some(at) = text[i..].find("BorshDeserialize for ").map(|k| k + i) {
                    let name_at = at + "BorshDeserialize for ".len();
                    let name: String = text[name_at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    let open = text[name_at..].find('{').map(|k| k + name_at).unwrap_or(b.len() - 1);
                    let end = group_end(b, open);
                    let start = text[..at].rfind("impl").unwrap_or(at);
                    insert(format!("impl BorshDeserialize for {name}"), squash(&text[start..end]));
                    i = end;
                }
            }
            out
        }

        fn int12_manifest() -> std::collections::BTreeMap<String, u64> {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/palw_lifecycle_objects_v2/int12_borsh_manifest.tsv");
            std::fs::read_to_string(&path)
                .expect("the frozen manifest of the live build's wire types")
                .lines()
                .filter(|l| !l.starts_with('#') && !l.is_empty())
                .map(|l| {
                    let (key, digest) = l.split_once('\t').expect("key<TAB>digest");
                    (key.to_string(), u64::from_str_radix(digest, 16).expect("a hex digest"))
                })
                .collect()
        }

        /// The item name a manifest key names (`"<path>::<item>"`, `"…::<item>#2"`, `"…::impl BorshDeserialize for <item>"`).
        fn item_of(key: &str) -> &str {
            let item = key.rsplit("::").next().unwrap_or(key);
            let item = item.strip_prefix("impl BorshDeserialize for ").unwrap_or(item);
            item.split('#').next().unwrap_or(item)
        }

        /// The item names reachable from `roots` through the declarations of `types` (a type named in a declaration is read; a name
        /// shared by two types counts for both — an over-approximation, which is the safe side for "never carried").
        fn reachable_from(
            types: &std::collections::BTreeMap<String, (u64, String)>,
            roots: &[&str],
        ) -> std::collections::BTreeSet<String> {
            let mut by_name: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
            for (key, (_, decl)) in types {
                by_name.entry(item_of(key)).or_default().push(decl);
            }
            let mut seen: std::collections::BTreeSet<String> = roots.iter().map(|r| r.to_string()).collect();
            let mut queue: Vec<String> = seen.iter().cloned().collect();
            while let Some(name) = queue.pop() {
                for decl in by_name.get(name.as_str()).into_iter().flatten() {
                    for ident in decl.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                        if by_name.contains_key(ident) && seen.insert(ident.to_string()) {
                            queue.push(ident.to_string());
                        }
                    }
                }
            }
            seen
        }

        /// **The live build's wire types are frozen: each is unchanged, or its change is classified** in
        /// [`PALW_INT12_WIRE_CHANGES_V1`] — the object enum (whose new variants the kind table owns), a type never carried (checked:
        /// not reachable from the lifecycle payload), or a carried type whose new form a fence owns (appended or re-read; its digest
        /// pinned). An appended variant or field inside a kind the live build decodes, or a new value of a tag it reads by hand,
        /// fails here until it is classified. No row may be stale.
        ///
        /// `A2U_WIRE_MANIFEST_OF=<tree> A2U_WIRE_MANIFEST_OUT=<file>` writes `<tree>`'s manifest to `<file>` instead (how the frozen
        /// one was made, from the source of `0b1c11b87`).
        #[test]
        fn every_int12_wire_type_is_unchanged_or_classified() {
            if let Ok(tree) = std::env::var("A2U_WIRE_MANIFEST_OF") {
                let out = std::env::var("A2U_WIRE_MANIFEST_OUT").expect("A2U_WIRE_MANIFEST_OUT=<file>");
                let manifest = wire_types_of(std::path::Path::new(&tree));
                let mut text = String::from(
                    "# The live testnet-12 build's wire types (int-12, `rcore/int-12` @ 0b1c11b87): `<path>::<item>`<TAB>FNV-1a of its\n\
                     # declaration without comments, `#[cfg(test)]` items and whitespace (`wire_types_of`: Borsh-derived structs and enums,\n\
                     # hand-written BorshDeserialize impls, `#[repr(uN)]` tag enums). Frozen; never regenerated against a newer tree.\n",
                );
                for (key, (digest, _)) in &manifest {
                    text.push_str(&format!("{key}\t{digest:016x}\n"));
                }
                std::fs::write(&out, text).expect("writes the manifest");
                eprintln!("[a2u] wrote {} wire types of {tree} to {out}", manifest.len());
                return;
            }
            let frozen = int12_manifest();
            assert!(frozen.len() > 500, "the frozen manifest is the live build's ({} types)", frozen.len());
            assert!(frozen.contains_key("consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2"));
            let now = wire_types_of(&repo_root());
            let mut unclassified = Vec::new();
            let mut changed = std::collections::BTreeSet::new();
            for (key, digest) in &frozen {
                if now.get(key).map(|(d, _)| d) == Some(digest) {
                    continue;
                }
                changed.insert(key.as_str());
                let now_digest = now.get(key).map(|(d, _)| *d);
                match PALW_INT12_WIRE_CHANGES_V1.iter().find(|(k, _)| k == key) {
                    None => unclassified
                        .push(format!("{key} ({})", now_digest.map_or("gone".to_string(), |d| format!("changed: now {d:016x}")))),
                    Some((
                        _,
                        PalwInt12WireChangeV1::CarriedAppended { digest: pinned, .. }
                        | PalwInt12WireChangeV1::CarriedReread { digest: pinned, .. },
                    )) if now_digest != Some(*pinned) => unclassified.push(format!("{key} (changed again: now {now_digest:016x?})")),
                    Some(_) => {}
                }
            }
            assert!(
                unclassified.is_empty(),
                "the live build's wire types changed without a classification in PALW_INT12_WIRE_CHANGES_V1 — a change inside a kind it \
                 decodes is bytes it cannot read as this build does, and below its fence must be read as it reads them (A-2 \
                 uniformity): {unclassified:#?}"
            );
            for (key, _) in PALW_INT12_WIRE_CHANGES_V1 {
                assert!(changed.contains(key), "a stale row: {key} is the live build's again (or never was one)");
            }
            // "Never carried" is checked, not taken on trust: no such type is reachable from the lifecycle payload.
            let carried = reachable_from(&now, &["PalwLifecycleTxPayloadV2", "PalwConsensusObjectV2"]);
            for (key, change) in PALW_INT12_WIRE_CHANGES_V1 {
                if let PalwInt12WireChangeV1::NotCarried(why) = change {
                    assert!(
                        !carried.contains(item_of(key)),
                        "{key} is classified NotCarried ({why}) but a lifecycle payload can carry it"
                    );
                }
            }
        }

        /// **Every object-tag row lies inside one of the Lead's allocations and names one of its fences, and so does every kind added
        /// after the live build** ([`PALW_A2_TAG_ALLOCATIONS_V1`]). BUDGET's 140–149 have no row until BUDGET creates its kinds; a kind
        /// there without one fails `every_landed_kind_sits_in_its_row_with_the_rows_fence`, and a row naming a fence outside
        /// `palw_bond_budget_v1` / `palw_model_bond_allocation_v1` fails here.
        #[test]
        fn every_tag_row_is_inside_its_allocation() {
            for (i, (lo, hi, owner, fences)) in PALW_A2_TAG_ALLOCATIONS_V1.iter().enumerate() {
                assert!(lo <= hi && !fences.is_empty(), "{owner}");
                assert!(!PALW_LIFECYCLE_INT12_KINDS_V1.iter().any(|(t, _)| (lo..=hi).contains(&t)), "{owner} overlaps int-12's tags");
                for (lo2, hi2, owner2, _) in &PALW_A2_TAG_ALLOCATIONS_V1[i + 1..] {
                    assert!(hi < lo2 || hi2 < lo, "overlapping allocations: {owner} and {owner2}");
                }
            }
            let allocation_of = |tag: u8| PALW_A2_TAG_ALLOCATIONS_V1.iter().find(|(lo, hi, _, _)| (*lo..=*hi).contains(&tag));
            for row in PALW_A2_KIND_FENCE_TABLE_V1 {
                let PalwA2SlotV1::ObjectTags { lo, hi } = row.slot else { continue };
                for tag in lo..=hi {
                    let (_, _, owner, fences) =
                        allocation_of(tag).unwrap_or_else(|| panic!("tag {tag} of {row:?} is not allocated: ask the Lead"));
                    assert!(fences.contains(&row.fence), "{row:?}: tag {tag} is {owner}'s, whose fences are {fences:?}");
                }
            }
            for (tag, name, fence) in PALW_LIFECYCLE_NEW_KINDS_V1 {
                let (_, _, owner, fences) =
                    allocation_of(*tag).unwrap_or_else(|| panic!("{name} (tag {tag}) is outside every allocation"));
                assert!(fences.contains(&fence.params_field()), "{name} (tag {tag}) is {owner}'s: {fences:?}");
            }
        }

        /// **ADR-0175 in the central table: the live build's kinds refused by name past `palw_model_immutable_v1` are exactly the
        /// `Int12RefusedByName` rows** — every int-12 kind is asked ([`palw_int12_kind_refused_past_fence_v1`], the policy the
        /// acceptance walk and the fold ask), from a near-zero body and, for tag 37, from a declaration granting `EARLY_VERSION`. A
        /// kind added to the policy without its row, or a row the policy does not refuse, fails. Every `Int12*` row names int-12 tags
        /// and a `Params` fence; no new kind is ever refused by this route (its own fence owns it).
        #[test]
        fn the_int12_kinds_refused_past_a_fence_are_the_rows() {
            use PalwA2SlotV1 as S;
            let samples_of = |tag: u8| -> Vec<PalwConsensusObjectV2> {
                let mut out: Vec<PalwConsensusObjectV2> = minimal_decode(&[tag]).into_iter().collect();
                if tag == 37 {
                    let mut declared = out.first().cloned().expect("tag 37 decodes from a near-zero body");
                    let PalwConsensusObjectV2::ModelLineBenefitsDeclared { tiers, .. } = &mut declared else { unreachable!("tag 37") };
                    let mut tier: crate::palw_model_benefits_v1::PalwModelBenefitTierV1 = zeros();
                    tier.grants = crate::palw_model_benefits_v1::grant::EARLY_VERSION;
                    tiers.push(tier);
                    out.push(declared);
                }
                out
            };
            let by_name: Vec<(&[u8], &PalwA2RowV1)> = PALW_A2_KIND_FENCE_TABLE_V1
                .iter()
                .filter_map(|r| match r.slot {
                    S::Int12RefusedByName { tags, .. } => Some((tags, r)),
                    _ => None,
                })
                .collect();
            assert!(!by_name.is_empty(), "ADR-0175's row");
            for (tag, name) in PALW_LIFECYCLE_INT12_KINDS_V1 {
                let refused: std::collections::BTreeSet<&str> =
                    samples_of(tag).iter().filter_map(palw_int12_kind_refused_past_fence_v1).collect();
                let rows: Vec<_> = by_name.iter().filter(|(tags, _)| tags.contains(&tag)).collect();
                match refused.len() {
                    0 => assert!(rows.is_empty(), "{name} (tag {tag}) has an Int12RefusedByName row the policy does not refuse"),
                    1 => {
                        assert_eq!(rows.len(), 1, "{name} (tag {tag}) is refused past {refused:?}: exactly one row");
                        assert!(
                            refused.contains(rows[0].1.fence),
                            "{name} (tag {tag}): the row's fence is the policy's ({refused:?})"
                        );
                    }
                    _ => panic!("{name} (tag {tag}) refused past two fences: {refused:?}"),
                }
            }
            for row in PALW_A2_KIND_FENCE_TABLE_V1 {
                let (S::Int12RefusedByName { tags, .. } | S::Int12FoldPastFence { tags, .. }) = row.slot else { continue };
                for tag in tags {
                    assert!(PALW_LIFECYCLE_INT12_KINDS_V1.iter().any(|(t, _)| t == tag), "{row:?}: tag {tag} is not the live build's");
                }
                assert!(row.landed && params_has_fence(row.fence), "{row:?}: a landed row naming a Params fence");
            }
            for (tag, name, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
                assert_eq!(palw_int12_kind_refused_past_fence_v1(&zero_filled_kind(*tag)), None, "{name}: its own fence owns it");
            }
        }

        /// **Each landed post-int-12 kind's wire form, pinned** — `(tag, digest)`: an FNV-1a over the kind's variant declaration in
        /// `PalwConsensusObjectV2` and every wire type reachable from it by name (A2U's manifest reader, comments and whitespace removed).
        /// A lane that creates a kind adds its pin; a lane that CHANGES a landed kind's form (DA16's re-scope of 150–153) re-pins it in the
        /// same commit and confirms the change rides under the row's fence (or adds a row for the fence it does ride under).
        /// `every_landed_kind_is_pinned_to_its_wire_form` fails on a missing, stale or moved pin and prints the current values.
        const PALW_A2_NEW_KIND_WIRE_V1: &[(u8, u64)] = &[
            (104, 0x98d7608e0a24ad99),
            (105, 0xdaed6c0cfaf81e2f),
            (106, 0x888f101d106c24bc),
            (107, 0x0081a402000ddbf4),
            (108, 0xac8ecc3ff97ec415),
            (109, 0xc7d6537424a66090),
            (110, 0x7da0346d0633de78),
            (111, 0x2d490db20d075bfe),
            (120, 0xcbb24fb8f993e5c6),
            (150, 0x5fb8c8b2822f8386),
            (151, 0x6a81242ebaeba7fd),
            (152, 0xc48f175fa3456d1e),
            (153, 0x74bac59d2cf50cd4),
        ];

        /// **The live build's 100 variants of `PalwConsensusObjectV2`, pinned as declared** — an FNV-1a over their declarations (inline
        /// fields included) in tag order. The frozen manifest pins the enum as a whole, which every new kind changes (`ObjectEnum`); this pin
        /// closes that gap: a field added to, or a type changed inside, a kind the live build decodes fails
        /// `every_landed_kind_is_pinned_to_its_wire_form` and is an `Int12Inner` change to classify, never a re-pin. Checked against
        /// `0b1c11b87`'s source on 2026-10-10 (every variant equal on this branch, `g14/r4-fixes` and `adv/c4r4`).
        const PALW_A2_INT12_VARIANTS_WIRE_V1: u64 = 0xd1982421da455cca;

        /// `PalwConsensusObjectV2`'s variants, squashed as the manifest reader reads them, by name.
        fn object_variant_decls(
            types: &std::collections::BTreeMap<String, (u64, String)>,
        ) -> std::collections::BTreeMap<String, String> {
            let (_, decl) = types.get("consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2").expect("the object enum");
            let b = decl.as_bytes();
            let open = decl.find("PalwConsensusObjectV2{").expect("the enum's body") + "PalwConsensusObjectV2".len();
            let close = group_end(b, open) - 1;
            let mut out = std::collections::BTreeMap::new();
            let (mut depth, mut start) = (0usize, open + 1);
            for i in open + 1..=close {
                match b[i] {
                    b'{' | b'(' | b'[' | b'<' if i < close => depth += 1,
                    b'}' | b')' | b']' | b'>' if depth > 0 => depth -= 1,
                    _ => {}
                }
                if (b[i] == b',' && depth == 0) || i == close {
                    let mut segment = &decl[start..i];
                    while segment.starts_with("#[") {
                        segment = &segment[group_end(segment.as_bytes(), 1)..];
                    }
                    let name: String = segment.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    if !name.is_empty() {
                        out.insert(name, segment.to_string());
                    }
                    start = i + 1;
                }
            }
            out
        }

        /// Each landed post-int-12 kind's wire digest (see [`PALW_A2_NEW_KIND_WIRE_V1`]), in the kind table's order.
        fn new_kind_wire_digests() -> Vec<(u8, u64)> {
            let mut types = wire_types_of(&repo_root());
            let variants = object_variant_decls(&types);
            // The enum itself is a leaf: an envelope (108) wraps "an object", not every kind's form.
            types.remove("consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2");
            PALW_LIFECYCLE_NEW_KINDS_V1
                .iter()
                .map(|(tag, name, _)| {
                    let segment = variants.get(*name).unwrap_or_else(|| panic!("{name}'s declaration in PalwConsensusObjectV2"));
                    let idents: Vec<&str> =
                        segment.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|s| !s.is_empty()).collect();
                    let mut text = segment.clone();
                    for item in reachable_from(&types, &idents) {
                        for (key, (digest, _)) in types.iter().filter(|(key, _)| item_of(key) == item) {
                            text.push_str(&format!("|{key}={digest:016x}"));
                        }
                    }
                    (*tag, fnv64(text.as_bytes()))
                })
                .collect()
        }

        /// **Every landed post-int-12 kind is pinned to its wire form** ([`PALW_A2_NEW_KIND_WIRE_V1`]). A lane that creates a kind,
        /// or changes one (a field, a nested variant, a type it reaches), re-pins it in the same commit — and with the pin confirms
        /// the change rides under its row's fence (DA16's re-scope of 150–153 under `palw_provider_court_v1`; BUDGET's 140–149). The
        /// failure prints the current pins.
        #[test]
        fn every_landed_kind_is_pinned_to_its_wire_form() {
            let types = wire_types_of(&repo_root());
            let variants = object_variant_decls(&types);
            let mut int12 = String::new();
            for (tag, name) in PALW_LIFECYCLE_INT12_KINDS_V1 {
                let declared = variants.get(name).unwrap_or_else(|| panic!("the live build's {name} (tag {tag}) is declared"));
                int12.push_str(&format!("{tag}:{declared};"));
            }
            let int12 = fnv64(int12.as_bytes());
            let now = new_kind_wire_digests();
            let listing: Vec<String> = now.iter().map(|(tag, digest)| format!("    ({tag}, 0x{digest:016x}),")).collect();
            assert!(
                PALW_A2_INT12_VARIANTS_WIRE_V1 == int12 && PALW_A2_NEW_KIND_WIRE_V1 == &now[..],
                "[a2u-pins] A-2 wire pins differ (A-2 uniformity).\n\
                 * PALW_A2_INT12_VARIANTS_WIRE_V1 (pinned 0x{PALW_A2_INT12_VARIANTS_WIRE_V1:016x}, now 0x{int12:016x}): a variant the live \
                 build decodes changed its declaration — classify it (an Int12Inner row and a guarded owner arm, or a re-read and its \
                 probe), never by re-pinning alone.\n\
                 * PALW_A2_NEW_KIND_WIRE_V1: a lane that creates or changes a post-int-12 kind re-pins it here and keeps the change under \
                 its row's fence. Current pins:\n[a2u-pins-begin]\n{}\n[a2u-pins-end]",
                listing.join("\n")
            );
        }
    }
}
