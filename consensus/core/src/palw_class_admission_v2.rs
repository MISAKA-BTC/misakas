//! **The gate a class must pass to join a chain that is already running** (ADR-0039's
//! "weightless until the catalog closes", used in the direction it was written for).
//!
//! # Why this exists
//!
//! `verify_palw_genesis_v2` checks the class set a network is born with, against a catalog whose
//! root is inside `PalwConsensusParamsV2` and therefore inside `palw_ruleset_id_v2`. That is the
//! right shape for genesis and the wrong shape for later: a class that does not exist yet has no
//! `artifact_root`, so it cannot be in the genesis catalog, so under the genesis gate alone **a
//! second class is a flag day** — a new ruleset id, a coordinated upgrade, a different network.
//!
//! The plan the RC is built on is the opposite: ship the BASE-0 liveness floor now, add a larger
//! class once its weights and its PTQ pipeline exist. This module is the missing half of that —
//! the checks the genesis loader runs, restated so they can run against ONE registration and the
//! shape profile it carries, with no pre-committed catalog to read from.
//!
//! # Derive, never declare
//!
//! The genesis catalog can afford to state `reachable_kernels`, `canonical_step_leaf_count` and
//! `max_step_leaf_count`, because its root is hashed into the ruleset and an operator who lies
//! contradicts a commitment the chain already made. A post-genesis registration has no such
//! anchor, so **nothing here is read from the registration that can be computed from the graph**:
//! the reachable set comes from the profile's own nodes, both leaf counts come from
//! `palw_step`, and the entry this returns is a derivation rather than a copy. The only fields a
//! registrant supplies are the ones no function can invent — `artifact_root` (the weights) and the
//! economic terms — and `pwu_per_inference`, which is checked against the count rather than
//! trusted.
//!
//! # What this does NOT do
//!
//! It does not admit anything. There is no carrier for `ClassRegistered` outside the genesis
//! object list in this tree, so this module is consensus-inert: nothing calls it, exactly like
//! every other V2 brick before its wiring lands. When the carriage does carry a registration, this
//! is the function it must call before the transition runs — and the transition is deliberately
//! left alone, because `apply_palw_transition_v2` is a pure state machine and adjudicability is an
//! arithmetic fact about a graph, not a fact about state.
//!
//! Nor does it decide share. `granted_share_table_v2` owns that, and it refuses a zero grant by
//! construction (`min_grantable_share_permille` is at least 1): a class with no share has a zero
//! epoch budget, which is a class that can never mine. So "register it weightless and activate it
//! later" is not available, and the honest version of the plan is **register it at the minimum
//! grantable share** — one permille, donated from the incumbents, which is the smallest weight the
//! ruleset admits rather than none.

use std::collections::BTreeSet;

use kaspa_hashes::Hash64;

use crate::palw_catalog_coverage::{PalwReachableKernelSetV1, verify_catalog_coverage_v1};
use crate::palw_mode_v2::{PalwClassCatalogEntryV2, PalwConsensusParamsV2};
use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};
use crate::palw_step::{PalwShapeProfileV3, step_leaf_count, worst_case_step_leaf_count_v1};
use crate::palw_step_refute::catalogued_kernel_ids_v1;
use crate::palw_v2::PalwJobContextV2;

/// **The `max_step_leaf_count` a network must freeze at genesis to keep a second class possible.**
///
/// `PalwCourtParamsV2::max_step_leaf_count` is a `PalwConsensusParamsV2` field, and the bundle is
/// what `palw_ruleset_id_v2` hashes. A class whose worst case is deeper than the ladder therefore
/// cannot join a running chain at all — it needs a new ruleset, which is a flag day. Unlike every
/// other obstacle to adding a class later, **this one cannot be repaired later**: by the time the
/// second class exists, the number is already inside the network's identity.
///
/// Provisioning it at the step space's own cap costs almost nothing, because the ladder is
/// `ceil(log2(leaves)) + terminal` ROUNDS. Measured on this tree
/// (`misaka-palw-base0/src/bin/base0-class-sizing.rs`), and pinned by
/// `provisioning_the_whole_step_space_costs_four_rounds`:
///
/// | provisioned for | leaves | bisection rounds |
/// |---|---|---|
/// | the RC floor alone | 184,456 | 18 |
/// | the whole step space | 4,194,304 | 22 |
///
/// The floor's figure is its WHOLE CONTEXT as prefill (`worst_case_step_leaf_count_v1`), not the
/// 47,020 of its declared 64/64 job — the ladder must reach the longest job a class admits, or it
/// admits a class an attacker picks the job length for. An earlier draft of this table used the
/// declared job and put the price at six rounds; the test below is what corrected it.
///
/// Four extra rounds of worst-case prosecution — paid only when a court actually runs to its worst
/// case — buys every class that could ever be adjudicable, because nothing deeper than
/// `PALW_STEP_MAX_LEAVES` is admissible in the first place (`worst_case_step_leaf_count_v1`
/// refuses it).
pub const PALW_RC_COURT_MAX_STEP_LEAF_COUNT: u64 = crate::palw_step::PALW_STEP_MAX_LEAVES;

/// **What one terminal adjudication of this class costs, derived from its graph** (ADR-0049
/// Decision C).
///
/// The ladder bounds how many rounds a dispute takes. Nothing bounded what a round COSTS, and the
/// answer used to be the model's size: the matmul arm opened `out_dim x in_len` — the whole matrix —
/// which is ~223 MiB for Qwen2.5-1.5B's unembed against a court-close budget of 152 KB. Decision B
/// made the opening tile-local; this is what turns "small" into "bounded", so a class cannot be
/// admitted whose disputes are unprosecutable in a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwCourtCostV1 {
    /// Bytes a refutation must open from the artifact for the most expensive single step.
    pub max_opening_bytes: u64,
    /// Multiply-accumulates a full node performs to recompute that step — its own CPU, on
    /// peer-supplied input.
    pub max_terminal_macs: u64,
    /// Rows a single step reads: its `input_refs` plus at most one weight operand. Bounds
    /// deserialization work before any arithmetic runs.
    pub max_operand_count: u32,
}

/// Widths a node can produce, worst case, from the profile's own geometry.
///
/// `KvScaled` is resolved at the CONTEXT maximum rather than at some typical position, because a
/// ceiling that held for a typical job and not for the longest one would admit a class an attacker
/// picks the job length for — the same rule `worst_case_step_leaf_count_v1` states.
fn node_out_width_v1(node: &crate::palw_step::PalwStepNodeV1, profile: &PalwShapeProfileV3) -> Option<u64> {
    match node.out_len {
        crate::palw_step::PalwStepOutLenV1::Fixed { elements } => Some(elements as u64),
        crate::palw_step::PalwStepOutLenV1::KvScaled { multiplier } => (multiplier as u64).checked_mul(profile.n_ctx as u64),
    }
}

/// The width of one input reference: a sentinel's own width, or an earlier node's output.
fn input_width_v1(r: u16, table: &[crate::palw_step::PalwStepNodeV1], profile: &PalwShapeProfileV3) -> Option<u64> {
    use crate::palw_step::{PALW_STEP_INPUT_CHECKPOINT_STATE, PALW_STEP_INPUT_KV_K, PALW_STEP_INPUT_KV_V, PALW_STEP_INPUT_LAYER_IN};
    let kv_dim = (profile.attn_kv_heads as u64).checked_mul(profile.attn_head_dim as u64)?;
    match r {
        PALW_STEP_INPUT_LAYER_IN => Some(profile.hidden_dim as u64),
        PALW_STEP_INPUT_KV_K | PALW_STEP_INPUT_KV_V => kv_dim.checked_mul(profile.n_ctx as u64),
        // The recurrent state chunk. Bounded by the widest state a GDN head can carry; a profile
        // with no GDN reaches this only if it names the sentinel, and then zero is the honest width.
        PALW_STEP_INPUT_CHECKPOINT_STATE => {
            (profile.gdn_heads as u64).checked_mul(profile.gdn_head_k_dim as u64)?.checked_mul(profile.gdn_head_v_dim as u64)
        }
        // An intra-table index: references are backwards, so the node exists by validation.
        i => table.get(i as usize).and_then(|n| node_out_width_v1(n, profile)),
    }
}

/// The cost of the most expensive step this profile can be challenged at.
///
/// Every quantity is read off the graph. Nothing is declared, so a registration cannot understate
/// what prosecuting it will cost — which is the same reason `pwu_per_inference` is checked against
/// a count rather than trusted.
pub fn derive_court_cost_v1(profile: &PalwShapeProfileV3) -> Result<PalwCourtCostV1, PalwClassAdmissionError> {
    use crate::palw_step::PalwStepOpKindV1 as Op;
    let over = || PalwClassAdmissionError::Profile("the class's court cost overflows a u64".to_string());
    let mut cost = PalwCourtCostV1 { max_opening_bytes: 0, max_terminal_macs: 0, max_operand_count: 0 };

    for table in [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes] {
        for node in table.iter() {
            let out_w = node_out_width_v1(node, profile).ok_or_else(over)?;
            let tile = (node.tile_len as u64).min(out_w.max(1));
            let in_w = match node.input_refs.first() {
                Some(r) => input_width_v1(*r, table, profile).ok_or_else(over)?,
                None => 0,
            };

            // The artifact bytes this node's parameters occupy, per catalogued kernel. A node with
            // no weight operand opens nothing from the artifact — its inputs ride the leg.
            let opening = if node.weight_name.is_empty() {
                0
            } else {
                match node.op_kind {
                    // Tile-local since Decision B: the tile's own weight rows, one byte per int8.
                    Op::MatMulQuant => tile.checked_mul(in_w).ok_or_else(over)?,
                    // (multiplier LE, shift, zero LE) per channel.
                    Op::MulElem => 9u64.checked_mul(out_w).ok_or_else(over)?,
                    // cos then sin, four bytes each, one pair per two lanes.
                    Op::RopeImrope => 8u64.checked_mul(out_w / 2).ok_or_else(over)?,
                    // One (multiplier, shift) for the whole node.
                    Op::Scale => 5,
                    // A gather opens the row it gathers; a norm's gain is one value per channel.
                    Op::EmbedLookup => out_w,
                    _ => 4u64.checked_mul(out_w).ok_or_else(over)?,
                }
            };

            // Recomputation. Only a reduction is quadratic in the row; everything else is a pass.
            let macs = match node.op_kind {
                Op::MatMulQuant => tile.checked_mul(in_w).ok_or_else(over)?,
                _ => out_w,
            };

            let operands = node.input_refs.len() as u32 + u32::from(!node.weight_name.is_empty());
            cost.max_opening_bytes = cost.max_opening_bytes.max(opening);
            cost.max_terminal_macs = cost.max_terminal_macs.max(macs);
            cost.max_operand_count = cost.max_operand_count.max(operands);
        }
    }
    Ok(cost)
}

/// Why a class may not join.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwClassAdmissionError {
    #[error("the object is not a class registration")]
    NotARegistration,
    /// ADR-0051 Decision 6. A class may register a thinner panel than the network's, down to a
    /// floor the bundle carries — but never below it, never above the network's own draw, and
    /// never half-specified.
    #[error("the class's panel terms are not admissible: {0}")]
    PanelTerms(String),
    /// ADR-0051 Decision 1. Family M's classes are capped at half the share table, so the half
    /// that can convict a liar stays in charge of the tie.
    #[error("this registration would put family {family:?} at {would_be}permille, past the {cap}permille cap")]
    FamilyShareCap { family: crate::palw_backend::PalwExecutionFamilyV1, would_be: u32, cap: u32 },
    /// A class IS its graph. Two registrations of the same profile are the same class and a
    /// different profile cannot borrow an id, so the id is checked against the profile rather than
    /// accepted as a label.
    #[error("the declared class id is not this profile's id")]
    ClassIdIsNotTheProfileId { declared: Hash64, derived: Hash64 },
    #[error("the profile is not well-formed: {0}")]
    Profile(String),
    /// ADR-0038 A4. Every kernel the graph can reach must be one this build can adjudicate,
    /// or every dispute over the uncatalogued one ends `Unadjudicable` — rejected but unslashed.
    #[error("the class reaches kernels this build cannot adjudicate")]
    CoverageGap,
    /// The class's LONGEST job — its whole context as prefill — must fit the ladder the ruleset
    /// already froze. Checking the typical job instead would admit a class an attacker picks the
    /// job length for.
    #[error("the class's worst-case trace is deeper than the court's ladder: {worst} > {ladder}")]
    DeeperThanTheLadder { worst: u64, ladder: u64 },
    /// ADR-0049 Decision C. A class whose cheapest prosecutable step still costs more than the
    /// ruleset allows is a class whose disputes nobody can raise — coverage-clean, ladder-deep
    /// enough, and unpolicable.
    #[error("the class's {what} of {got} exceeds the ruleset's ceiling of {ceiling}")]
    CourtCostExceedsCeiling { what: &'static str, got: u64, ceiling: u64 },
    /// A network that carries value registers only derived classes (the genesis loader's rule,
    /// restated: `MaxPerAttempt` bounds rather than checks, which makes PALW weight a collateral
    /// measure instead of a work measure).
    #[error("the class is not a derived-pwu class")]
    ClassIsNotDerived,
    #[error("the declared pwu_per_inference is not the canonical job's counted leaves: {declared} != {counted}")]
    PwuPerInferenceMismatch { declared: u64, counted: u64 },
    /// The canonical job is what the class is PAID per, so it may not be longer than the worst
    /// case the ladder was checked against.
    #[error("the canonical job is deeper than the class's own worst case: {canonical} > {worst}")]
    CanonicalDeeperThanWorstCase { canonical: u64, worst: u64 },
}

/// Every kernel a profile's graph can reach, read off the graph.
///
/// Public because the coverage claim and the catalog entry must be built from the same traversal —
/// two traversals that merely happen to agree is how A4 certifies a set nobody derived.
pub fn reachable_kernels_v1(profile: &PalwShapeProfileV3) -> BTreeSet<Hash64> {
    [&profile.pre_nodes, &profile.gdn_nodes, &profile.attn_nodes, &profile.post_nodes]
        .into_iter()
        .flatten()
        .map(|node| node.kernel_semantics_id)
        .collect()
}

/// The gate: `Ok(entry)` iff this class may join a chain running `bundle`.
///
/// `canonical` is the job the class is paid per. It is an argument rather than a derivation
/// because no function can choose it — it is the registrant's declaration of what one unit of this
/// class's work is — and a carrier must therefore commit it inside the signed registration, beside
/// `artifact_root`. Everything the catalog entry needs BESIDES that is computed here.
///
/// The returned entry is what a genesis catalog would have held for this class. A caller that
/// keeps it has the same object the genesis path produces, so the two lanes cannot drift into
/// describing a class differently.
pub fn verify_class_admission_v2(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    registration: &PalwConsensusObjectV2,
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    let PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, pwu_rule, terms, .. } = registration else {
        return Err(PalwClassAdmissionError::NotARegistration);
    };

    // **The panel first, because it is a property of the registration rather than of the graph**
    // (ADR-0051 Decision 6). A class may draw a thinner panel than the network's — a family whose
    // seats must hold particular hardware cannot demand six such operators on its first day — but
    // the terms are on chain, so a thin class is visibly thin, and the bundle's floor is what
    // stops "thinner" becoming "one".
    terms
        .panel_params(&bundle.panel, bundle.min_panel_seats(), bundle.min_panel_quorum())
        .map_err(PalwClassAdmissionError::PanelTerms)?;

    // **A family that cannot be convicted is admitted on different evidence** (ADR-0051 Decision
    // 2). There is no step space to count, no ladder to fit inside and no opening to price,
    // because a black-box runtime has none of them — so the checks below, every one of which is
    // about the SHAPE of an adjudicable graph, do not apply and must not be run. Running them
    // anyway would refuse every Family-M class for failing to be something it never claimed.
    //
    // What replaces them is the identity the family actually rests on, and the class carries it
    // in the same place a Family-D class carries its graph: the profile's ids ARE the runtime
    // manifest, model, tokenizer and shape pins that a producer's worker must reproduce before it
    // may produce (`MetalBackend::check_runtime_identity`).
    // **Pins exist exactly when the family's identity is not its graph.** A deterministic class
    // carrying them would be declaring an identity nothing checks; a black-box class without them
    // could be run by any runtime, since nothing downstream re-derives the arithmetic.
    match (terms.family.is_court_adjudicable(), terms.runtime_pins.is_some()) {
        (true, true) => {
            return Err(PalwClassAdmissionError::Profile(
                "a deterministic class's identity is its graph; runtime pins on it are an identity nothing checks".into(),
            ));
        }
        (false, false) => {
            return Err(PalwClassAdmissionError::Profile(
                "a black-box class must pin its runtime, model and tokenizer — without them any runtime could claim to be it".into(),
            ));
        }
        _ => {}
    }

    if !terms.family.is_court_adjudicable() {
        let pins = terms.runtime_pins.expect("checked just above");
        return palw_metal_class_admission_v1(bundle, profile, canonical, &pins, *class_id, *artifact_root, *pwu_rule);
    }

    // **Well-formedness first, before anything reads the shape.** Every check below — the id
    // derivation, the kernel walk, the leaf enumeration — is driven by `n_ctx` and `layer_count`,
    // so an unbounded shape decides how much work it costs to reject it. `validate_shape` is where
    // those bounds live; running it after the first consumer is running it too late.
    profile.validate_shape().map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;

    let derived_id = profile.shape_profile_id();
    if *class_id != derived_id {
        return Err(PalwClassAdmissionError::ClassIdIsNotTheProfileId { declared: *class_id, derived: derived_id });
    }

    // A4 first: a class whose disputes cannot be adjudicated must not reach any later check, so
    // that a coverage gap can never be reported as some more specific failure.
    let kernel_ids = reachable_kernels_v1(profile);
    verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: derived_id, kernel_ids: kernel_ids.clone() })
        .map_err(|_| PalwClassAdmissionError::CoverageGap)?;
    // The catalogued set is read from the adjudication table itself, which is what
    // `verify_catalog_coverage_v1` compares against — asserted here so a future refactor that
    // pointed the gate at a hand-kept list fails a test rather than certifying quietly.
    debug_assert!(kernel_ids.is_subset(&catalogued_kernel_ids_v1()), "coverage passed against a set that is not the table");
    // **And the STRONG gate, which had no non-test caller at all** (audit H-02).
    //
    // The id check above is set inclusion: it asks whether every kernel this profile names is in
    // the table. It cannot ask whether the adjudicator has an arm for the SHAPE each node wants —
    // an op needs operands of a particular arity and width, so a node can name a catalogued id
    // while asking for something nothing can produce. `verify_profile_coverage_v1` asks the
    // adjudicator itself, node by node and for both call classes.
    //
    // Near-inert while classes could only come from genesis; ADR-0049 Decision H made post-genesis
    // registration a live, permissionless path, and then a stranger could register a class whose
    // every dispute ends `Unadjudicable` — rejected but UNSLASHED, which is unfalsifiable work on a
    // chain where bonds are supposed to be at risk. The BASE-0 profile shipped 2026-08-20 did
    // exactly this at two nodes per layer and passed the id gate.
    crate::palw_catalog_coverage::verify_profile_coverage_v1(profile).map_err(|_| PalwClassAdmissionError::CoverageGap)?;

    let worst = worst_case_step_leaf_count_v1(profile).map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    let ladder = bundle.court.max_step_leaf_count();
    if worst > ladder {
        return Err(PalwClassAdmissionError::DeeperThanTheLadder { worst, ladder });
    }

    // **Decision C: what prosecuting this class costs, against what the ruleset allows.**
    //
    // Ordered after the ladder because the two answer different halves of one question — the ladder
    // bounds how many rounds a dispute takes, these bound what a round costs — and a class that
    // fails both should be told about the deeper problem first.
    let cost = derive_court_cost_v1(profile)?;
    for (what, got, ceiling) in [
        ("terminal opening", cost.max_opening_bytes, bundle.court.max_opening_bytes()),
        ("terminal multiply-accumulates", cost.max_terminal_macs, bundle.court.max_terminal_macs()),
        ("operand count", cost.max_operand_count as u64, bundle.court.max_operand_count() as u64),
    ] {
        if got > ceiling {
            return Err(PalwClassAdmissionError::CourtCostExceedsCeiling { what, got, ceiling });
        }
    }

    let counted = step_leaf_count(profile, canonical).map_err(|e| PalwClassAdmissionError::Profile(format!("{e:?}")))?;
    if counted > worst {
        return Err(PalwClassAdmissionError::CanonicalDeeperThanWorstCase { canonical: counted, worst });
    }
    match pwu_rule {
        PalwPwuRuleV2::MaxPerAttempt(_) => return Err(PalwClassAdmissionError::ClassIsNotDerived),
        PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => {
            if *pwu_per_inference != counted {
                return Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared: *pwu_per_inference, counted });
            }
        }
    }

    Ok(PalwClassCatalogEntryV2 {
        class_id: derived_id,
        artifact_root: *artifact_root,
        max_step_leaf_count: worst,
        canonical_step_leaf_count: counted,
        reachable_kernels: kernel_ids,
    })
}

/// **The Family-M admission gate** (ADR-0051 Decision 2).
///
/// What it checks instead of a step space:
///
/// * the class id is still the profile's id — a class IS its graph in every family, and for this
///   one the "graph" is the pinned identity set (runtime manifest, model, tokenizer, shape);
/// * every identity is actually pinned. A zero hash is a field nobody set, and a class that did
///   not pin its runtime is a class any runtime could claim to be — which is the entire security
///   of a family that never re-derives the arithmetic;
/// * the canonical job is inside the class's own context, and its decode budget is the
///   `pwu_per_inference` the registration declares. Counted, not believed, exactly as the
///   deterministic gate recounts step leaves: `pwu` is a direct multiplier on fork-choice weight
///   and a registrant that could declare it could mint weight from nothing.
///
/// It deliberately does NOT check a court window, a ladder depth or an opening cost. Those price
/// a dispute this family cannot have.
fn palw_metal_class_admission_v1(
    bundle: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    canonical: &PalwJobContextV2,
    pins: &crate::palw_state_v2::PalwRuntimePinsV2,
    class_id: Hash64,
    artifact_root: Hash64,
    pwu_rule: crate::palw_state_v2::PalwPwuRuleV2,
) -> Result<PalwClassCatalogEntryV2, PalwClassAdmissionError> {
    // GEOMETRY, not the graph: the node-table rules refuse an empty table, which is exactly what
    // a class with no court declares, and refusing it here would refuse the family for being
    // itself. Same reasoning as skipping the ladder and the opening cost.
    profile.validate_geometry().map_err(|e| PalwClassAdmissionError::Profile(e.to_string()))?;
    let derived_id = profile.shape_profile_id();
    if class_id != derived_id {
        return Err(PalwClassAdmissionError::ClassIdIsNotTheProfileId { declared: class_id, derived: derived_id });
    }

    // Every pin present. `Hash64::default()` is the "nobody set this" value everywhere in this
    // tree, and a Family-M class with an unset runtime pin is one whose producer's claim about
    // what it ran cannot be contradicted by anything.
    for (what, value) in [
        ("artifact_root", artifact_root),
        ("runtime_manifest_hash", pins.runtime_manifest_hash),
        ("runtime_class_id", pins.runtime_class_id),
        ("model_profile_id", pins.model_profile_id),
        ("tokenizer_id", pins.tokenizer_id),
        ("trace_scheme_id", pins.trace_scheme_id),
        ("cu_ruleset_id", pins.cu_ruleset_id),
    ] {
        if value == Hash64::default() {
            return Err(PalwClassAdmissionError::Profile(format!(
                "a Metal/GGUF class must pin its {what}; an unset one is a class any runtime could claim to be"
            )));
        }
    }

    // **The pins and the canonical job are one statement.** They are two encodings of the same
    // identity — the registration's, and the job it is priced over — and a registration whose job
    // was run under a different runtime than the class pins is a class priced for work it does not
    // describe. Correspondence checked here because this is the only place both are in hand.
    for (what, a, b) in [
        ("runtime_manifest_hash", pins.runtime_manifest_hash, canonical.runtime_manifest_hash),
        ("runtime_class_id", pins.runtime_class_id, canonical.runtime_class_id),
        ("model_profile_id", pins.model_profile_id, canonical.model_profile_id),
        ("tokenizer_id", pins.tokenizer_id, canonical.tokenizer_id),
        ("trace_scheme_id", pins.trace_scheme_id, canonical.trace_scheme_id),
        ("cu_ruleset_id", pins.cu_ruleset_id, canonical.cu_ruleset_id),
    ] {
        if a != b {
            return Err(PalwClassAdmissionError::Profile(format!(
                "the class pins {what} {a} and its canonical job was run at {b} — two identities under one registration"
            )));
        }
    }
    if (pins.prefill_tokens, pins.exact_decode_tokens, pins.max_context_tokens)
        != (canonical.declared_prefill_tokens, canonical.exact_decode_tokens, canonical.max_context_tokens)
    {
        return Err(PalwClassAdmissionError::Profile(
            "the class's declared job shape is not the canonical job's".into(),
        ));
    }
    if pins.vocab_size != profile.vocab_size {
        return Err(PalwClassAdmissionError::Profile("the class pins a vocabulary its profile does not declare".into()));
    }

    if canonical.exact_decode_tokens == 0 {
        return Err(PalwClassAdmissionError::Profile("a canonical job that decodes nothing is not one unit of work".into()));
    }
    let span = canonical.declared_prefill_tokens.saturating_add(canonical.exact_decode_tokens);
    if span > canonical.max_context_tokens || canonical.max_context_tokens > profile.n_ctx {
        return Err(PalwClassAdmissionError::Profile(format!(
            "the canonical job spans {span} tokens against a declared context of {} and a class context of {}",
            canonical.max_context_tokens, profile.n_ctx
        )));
    }

    // **`pwu_per_inference` is the decode budget, counted.** There are no step leaves to count
    // here, and the decode budget is the honest unit: it is what the runtime is exactly held to
    // (`exact_decode_tokens`, no early stop) and what a seat re-runs.
    let counted = canonical.exact_decode_tokens as u64;
    match pwu_rule {
        crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } if pwu_per_inference == counted => {}
        crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => {
            return Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared: pwu_per_inference, counted });
        }
        crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(_) => {
            return Err(PalwClassAdmissionError::Profile(
                "a Metal/GGUF class registers a derived pwu; MaxPerAttempt is pre-derivation scaffolding and mints weight from a choice".into(),
            ));
        }
    }

    let _ = bundle;
    Ok(PalwClassCatalogEntryV2 {
        class_id,
        artifact_root,
        canonical_step_leaf_count: counted,
        max_step_leaf_count: counted,
        reachable_kernels: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1};
    use crate::palw_mode_v2::{PalwCourtParamsV2, tests::conforming_bundle};
    use crate::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_3B, qwen25_profile_v1};
    use crate::palw_step::PALW_STEP_MAX_LEAVES;
    use crate::palw_v2::{PALW_TRACE_COMMITMENT_VERSION_V2, trace_scheme_id_v2};

    /// The measured Qwen2.5-1.5B graph, at the `tile_len` that actually admits its own declared
    /// context.
    ///
    /// `palw_qwen25_profile` ships `tile_len` 128 with `n_ctx` 4096, and at that pair the class's
    /// LONGEST job is 132,354,910 step leaves against a `PALW_STEP_MAX_LEAVES` of 4,194,304 — so
    /// the class as declared cannot be registered at all. Coverage says nothing about this: the
    /// graph reaches ten catalogued kernels and passes A4 either way. It is the leaf count that
    /// refuses, and `the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context` below is
    /// the tripwire for it.
    fn qwen_admissible() -> PalwShapeProfileV3 {
        // DERIVED against the full ladder, not a hand-picked tile. It was `tile_len: 16_384` at
        // the declared 4096 context, which was admissible against a hand-written 27-node layer
        // table; the table is projected from `BASE0_LAYER_IR` now (ADR-0049 Decision F) and the
        // engine performs 38 steps, so that pair is 4,194,650 leaves against a 4,194,304 cap.
        // A literal here would be a third description of the class, rotting on its own schedule.
        let court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 20, 2).expect("the full ladder is a legal court");
        let g = crate::palw_qwen25_profile::qwen25_admissible_geometry_v1(QWEN25_1_5B, &court).expect("some pair is admissible under the full ladder");
        qwen25_profile_v1(g).expect("the derived geometry is expressible")
    }

    fn context(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
        let mut ctx = PalwJobContextV2 {
            version: PALW_TRACE_COMMITMENT_VERSION_V2,
            network_id: b"misaka-palw-rc".to_vec(),
            job_id: Hash64::default(),
            job_nullifier: Hash64::default(),
            assignment_id: Hash64::default(),
            execution_seed: [0; 32],
            model_profile_id: Hash64::default(),
            runtime_manifest_hash: Hash64::default(),
            runtime_class_id: Hash64::default(),
            shape_profile_id: profile.shape_profile_id(),
            trace_scheme_id: Hash64::default(),
            cu_ruleset_id: Hash64::default(),
            tokenizer_id: Hash64::default(),
            prompt_token_ids_hash: Hash64::default(),
            declared_prefill_tokens: prefill,
            exact_decode_tokens: decode,
            max_context_tokens: profile.n_ctx,
        };
        ctx.trace_scheme_id = trace_scheme_id_v2();
        ctx
    }

    /// A bundle whose ladder is provisioned for the whole step space rather than for one class.
    ///
    /// This is the move the plan turns on. `max_step_leaf_count` is a bundle field and the bundle
    /// is `palw_ruleset_id_v2`, so a class deeper than the ladder cannot join a running chain —
    /// it needs a new ruleset. But the ladder is `ceil(log2(leaves)) + terminal` ROUNDS, so
    /// provisioning it at `PALW_STEP_MAX_LEAVES` covers every class that could ever be
    /// adjudicable, and costs six rounds over provisioning it for the floor alone (16 → 22).
    fn bundle_with_full_ladder() -> PalwConsensusParamsV2 {
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::new(PALW_STEP_MAX_LEAVES, 20, 2).expect("the full ladder is a legal court");
        bundle
    }

    /// A network that has DECIDED, at genesis, to carry a Qwen-scale class and to pay for its
    /// courts. The ceilings are the measured cost of that class rather than a round number, which
    /// is the only honest way to choose a value that is inside the ruleset id forever.
    fn bundle_that_pays_for_qwen() -> PalwConsensusParamsV2 {
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::with_cost_ceilings(PALW_STEP_MAX_LEAVES, 20, 2, 32 * 1024 * 1024, 128 * 1024 * 1024, 8)
            .expect("a court sized for a 1.5B class is legal, and expensive on purpose");
        bundle
    }

    fn registration(class_id: Hash64, pwu_per_inference: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            terms: crate::palw_state_v2::PalwClassTermsV2::deterministic_default(),
            artifact_root: Hash64::from_u64_word(0xA271FAC7),
            slash_value_per_pwu: 1,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target: 1,
            share_permille: 1,
            activation_daa: 0,
            admission: None,
        }
    }

    /// **The plan's load-bearing claim, as a test.** A Qwen-scale BASE-0 class passes every gate a
    /// class must pass to join a running chain: its graph reaches only adjudicable kernels, its
    /// longest job fits a ladder provisioned at the step-space cap, and its declared pwu is the
    /// counted one.
    #[test]
    fn a_qwen_scale_class_can_join_a_chain_provisioned_for_the_step_space() {
        let profile = qwen_admissible();
        // The floor's shape of canonical job. (64, 64) needs 128 positions and the DERIVED context is
        // smaller than that — the class is admissible at a context the projected graph allows, not
        // at the one the hand-written table pretended to.
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("the canonical job counts");
        let entry = verify_class_admission_v2(
            &bundle_that_pays_for_qwen(),
            &profile,
            &canonical,
            &registration(profile.shape_profile_id(), counted),
        )
        .expect("the measured Qwen2.5-1.5B class is admissible on a network that pays for its courts");

        assert_eq!(entry.class_id, profile.shape_profile_id(), "a class is its graph");
        assert_eq!(entry.canonical_step_leaf_count, counted);
        assert!(entry.max_step_leaf_count <= PALW_STEP_MAX_LEAVES, "the worst case is inside the step space");
        assert_eq!(entry.reachable_kernels.len(), 10, "the Qwen graph reaches ten of the catalog's kernels");
    }

    /// **Registering a model does not make a new network.** The property every "add an LLM" flow
    /// depends on, asserted where it can be broken.
    ///
    /// `class_catalog_root` commits to the classes a network is BORN with. A post-genesis
    /// registration is a consensus object that lands in chain state — it never re-enters the
    /// bundle — so the ruleset id, and therefore the `consensus_params_id` two nodes compare at
    /// handshake, cannot move because someone registered a class.
    ///
    /// If that ever stopped being true, adding a model would fork the network silently: every node
    /// that had not yet seen the registration would compute a different fingerprint and refuse to
    /// peer, and the failure would look like a connectivity problem rather than a rule change.
    #[test]
    fn registering_a_class_does_not_move_the_ruleset_id() {
        // The Qwen-scale class needs a network provisioned for its step space; that is a fact
        // about the CLASS, not about this property, and using the floor's bundle would fail on the
        // ladder before it ever reached the question being asked here.
        let bundle = bundle_that_pays_for_qwen();
        let before = crate::palw_mode_v2::palw_ruleset_id_v2(&bundle);

        // A second class, admitted by the chain's own gate — the same call a node makes.
        let profile = qwen_admissible();
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let object = registration(profile.shape_profile_id(), counted);
        verify_class_admission_v2(&bundle, &profile, &canonical, &object).expect("admissible");

        // The bundle is untouched by admitting it: admission RETURNS a catalog entry, it does not
        // put one into the params. The entry lives in chain state from here on.
        assert_eq!(
            crate::palw_mode_v2::palw_ruleset_id_v2(&bundle),
            before,
            "admitting a class must not move the ruleset id — a handshake would refuse across it"
        );
        assert_eq!(
            bundle.class_catalog_root,
            bundle_that_pays_for_qwen().class_catalog_root,
            "and the genesis catalog root is still the genesis one"
        );
    }
    /// The floor is not disturbed by the second class existing — the property that makes "add it
    /// later" different from "run a different network".
    #[test]
    fn admitting_a_second_class_does_not_move_the_floors_id() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor geometry is expressible");
        let before = floor.shape_profile_id();
        let big = qwen_admissible();
        let canonical = context(&big, 8, 4);
        let counted = step_leaf_count(&big, &canonical).expect("counts");
        verify_class_admission_v2(&bundle_that_pays_for_qwen(), &big, &canonical, &registration(big.shape_profile_id(), counted))
            .expect("admissible");
        assert_eq!(base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("re-derives").shape_profile_id(), before);
        assert_ne!(before, big.shape_profile_id(), "two geometries are two classes");
    }

    /// **A class whose kernel ids are all catalogued but whose SHAPE nothing can adjudicate is
    /// refused at registration** (audit H-02).
    ///
    /// The id gate is set inclusion: it asks whether every kernel a profile names is in the table.
    /// It cannot ask whether the adjudicator has an arm for the shape each node wants — an op needs
    /// operands of a particular arity, so a node can name a catalogued id while asking for
    /// something nothing can produce. `verify_profile_coverage_v1` asks the adjudicator itself, and
    /// until now had NO non-test caller.
    ///
    /// Near-inert while classes came only from genesis; ADR-0049 Decision H made post-genesis
    /// registration a live, permissionless path, and then this is a stranger registering a class
    /// whose every dispute ends `Unadjudicable` — rejected but UNSLASHED, which is unfalsifiable
    /// work on a chain where bonds are supposed to be at risk.
    #[test]
    fn a_class_the_adjudicator_cannot_serve_is_refused_even_when_every_id_is_catalogued() {
        let bundle = conforming_bundle();
        let good = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the floor is expressible");
        let canonical = context(&good, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);

        // Strip a node's operands: same catalogued kernel id, a shape the adjudicator refuses. This
        // is the exact defect the BASE-0 profile shipped on 2026-08-20 at its attention matmuls.
        let mut lame = good.clone();
        let victim = lame.attn_nodes.iter_mut().find(|n| n.input_refs.len() >= 2).expect("a binary node exists");
        victim.input_refs.truncate(1);

        // Its kernel ids are unchanged, so the WEAK gate still passes…
        let ids = reachable_kernels_v1(&lame);
        assert!(
            verify_catalog_coverage_v1(&PalwReachableKernelSetV1 { execution_class_id: lame.shape_profile_id(), kernel_ids: ids })
                .is_ok(),
            "every id it names is still catalogued — which is why the id gate alone certified this"
        );

        // …and admission refuses it anyway, now that the strong gate is wired.
        let reg = registration(lame.shape_profile_id(), 1);
        let lame_ctx = context(&lame, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        assert!(
            matches!(verify_class_admission_v2(&bundle, &lame, &lame_ctx, &reg), Err(PalwClassAdmissionError::CoverageGap)),
            "a class the adjudicator cannot serve must not be registrable"
        );

        // And the honest floor still admits, so the gate is a bound rather than a blanket refusal.
        let ok = registration(good.shape_profile_id(), 7_900);
        assert!(
            !matches!(verify_class_admission_v2(&bundle, &good, &canonical, &ok), Err(PalwClassAdmissionError::CoverageGap)),
            "the floor's own graph is servable"
        );
    }

    /// A ladder provisioned for the floor alone refuses the bigger class — which is exactly why
    /// the provisioning decision has to be made at genesis and cannot be made later.
    #[test]
    fn a_floor_sized_ladder_refuses_the_bigger_class() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let floor_worst = worst_case_step_leaf_count_v1(&floor).expect("the floor's worst case is inside the cap");
        let mut bundle = conforming_bundle();
        bundle.court = PalwCourtParamsV2::new(floor_worst, 20, 2).expect("a floor-sized court is legal");

        let big = qwen_admissible();
        let canonical = context(&big, 8, 4);
        let counted = step_leaf_count(&big, &canonical).expect("counts");
        let err = verify_class_admission_v2(&bundle, &big, &canonical, &registration(big.shape_profile_id(), counted))
            .expect_err("the ladder cannot reach it");
        assert!(matches!(err, PalwClassAdmissionError::DeeperThanTheLadder { .. }), "got {err:?}");
    }

    /// `pwu_per_inference` is a declaration and pwu is a direct multiplier on fork-choice weight,
    /// so the count is what decides it. Overstating by one is refused.
    #[test]
    fn an_overstated_pwu_is_refused_against_the_count() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let err = verify_class_admission_v2(
            &bundle_with_full_ladder(),
            &profile,
            &canonical,
            &registration(profile.shape_profile_id(), counted + 1),
        )
        .expect_err("an overstated pwu is a lie the count catches");
        assert!(matches!(err, PalwClassAdmissionError::PwuPerInferenceMismatch { .. }), "got {err:?}");
    }

    /// A class may not borrow another graph's id, and a network carrying value may not register a
    /// `MaxPerAttempt` class — the genesis loader's two rules, restated for the later lane so the
    /// two entry points cannot drift.
    #[test]
    fn the_id_must_be_the_graphs_and_the_rule_must_be_derived() {
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1);
        let counted = step_leaf_count(&profile, &canonical).expect("counts");
        let bundle = bundle_with_full_ladder();

        let borrowed = verify_class_admission_v2(&bundle, &profile, &canonical, &registration(Hash64::from_u64_word(7), counted))
            .expect_err("an id that is not the graph's is refused");
        assert!(matches!(borrowed, PalwClassAdmissionError::ClassIdIsNotTheProfileId { .. }), "got {borrowed:?}");

        let mut bounded = registration(profile.shape_profile_id(), counted);
        if let PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } = &mut bounded {
            *pwu_rule = PalwPwuRuleV2::MaxPerAttempt(1_000);
        }
        let err = verify_class_admission_v2(&bundle, &profile, &canonical, &bounded).expect_err("bounded is not derived");
        assert!(matches!(err, PalwClassAdmissionError::ClassIsNotDerived), "got {err:?}");
    }

    /// **The genesis decision, as arithmetic.** Provisioning the ladder for the whole step space
    /// rather than for the floor alone costs six rounds, and buys every admissible class — because
    /// `worst_case_step_leaf_count_v1` refuses anything deeper than the cap, so there is no class
    /// this ladder can fail to reach.
    #[test]
    fn provisioning_the_whole_step_space_costs_four_rounds() {
        let rounds = |leaves: u64| leaves.max(2).next_power_of_two().trailing_zeros();

        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        // The floor's LONGEST job — whole context as prefill — and not its declared 64/64 one,
        // which is 47,020. The ladder is checked against the longest job a class admits, so using
        // the declared one here understated the floor's own ladder by two rounds and the price of
        // provisioning by the same.
        let floor_worst = worst_case_step_leaf_count_v1(&floor).expect("the floor is inside the cap");
        // Re-measured four times, each because the declared graph grew to be the computation.
        // The layer table's narrowings took 184,456 to 366,728; the post table's narrowing added
        // eight; declaring attention PER QUERY HEAD — the engine runs score/amplify/softmax/narrow
        // once per head and the table declared them once per layer — took it to 465,040; and
        // ADR-0050's two residual gains take it to 481,424.
        assert_eq!(floor_worst, 481_424, "the floor's longest job, measured");
        assert_eq!(rounds(floor_worst), 19);

        assert_eq!(PALW_RC_COURT_MAX_STEP_LEAF_COUNT, PALW_STEP_MAX_LEAVES);
        assert_eq!(rounds(PALW_RC_COURT_MAX_STEP_LEAF_COUNT), 22);
        assert_eq!(rounds(PALW_RC_COURT_MAX_STEP_LEAF_COUNT) - rounds(floor_worst), 3, "the price of the whole step space");

        // And it really is every class: the cap is what `worst_case_step_leaf_count_v1` enforces,
        // so a class the ladder cannot reach is a class that was already inadmissible.
        let big = qwen_admissible();
        assert!(worst_case_step_leaf_count_v1(&big).expect("inside the cap") <= PALW_RC_COURT_MAX_STEP_LEAF_COUNT);
    }

    /// **ADR-0049 Decision C: at the floor's ceiling a Qwen class fits only at 125 tokens of context.**
    ///
    /// `tile_len` trades adjudicable context against court cost, and the window this leaves is the
    /// number a genesis has to look at. Measured with `derive_court_cost_v1` against the
    /// floor-derived 1 MiB default:
    ///
    /// | `tile_len` | opening | adjudicable `n_ctx` | fits the default |
    /// |---|---|---|---|
    /// | 64 | 560 KiB | **125** | yes |
    /// | 128 | 1.09 MiB | 244 | no |
    /// | 16,384 | 24 MiB | 4,838 | no, by 24x |
    ///
    /// So a network on the floor's own ceiling can carry Qwen2.5-1.5B with a 125-token context and
    /// nothing longer. Its declared 4,096 needs a ceiling twenty-four times larger, chosen at
    /// genesis and unchangeable after. ADR-0046's 152 KB court-close budget admits it at no tile
    /// length at all — its cheapest step is nearly four times that.
    ///
    /// A first draft of this test asserted "refused at EVERY tile length" and failed on tile 64,
    /// which is the assertion doing its job: the claim was one measurement short.
    #[test]
    fn a_qwen_class_fits_the_floors_ceiling_only_at_a_toy_context() {
        let floor = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let floor_cost = derive_court_cost_v1(&floor).expect("derivable");
        assert_eq!(floor_cost.max_opening_bytes, 32 * 1024, "the floor's widest step opens 32 KiB");
        assert!(floor_cost.max_opening_bytes * 32 <= crate::palw_mode_v2::DEFAULT_MAX_OPENING_BYTES, "the default is floor-derived");

        // Each tile is priced at the LONGEST context it can adjudicate, because that is the class a
        // network would actually register — a tile bought for its context and then not used for it
        // is a ceiling question nobody is asking.
        let widest_adjudicable_ctx = |tile: u32| -> u32 {
            let fits = |n_ctx: u32| {
                qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B })
                    .ok()
                    .and_then(|p| worst_case_step_leaf_count_v1(&p).ok())
                    .is_some()
            };
            let (mut lo, mut hi) = (2u32, 1u32 << 16);
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if fits(mid) { lo = mid } else { hi = mid }
            }
            lo
        };

        let default_bundle = bundle_with_full_ladder();
        let mut fits: Vec<u32> = Vec::new();
        for tile in [64u32, 128, 256, 512, 1024, 2048, 4096, 8192, 16_384] {
            let n_ctx = widest_adjudicable_ctx(tile);
            let p = qwen25_profile_v1(PalwQwen25GeometryV1 { n_ctx, tile_len: tile, ..QWEN25_1_5B }).expect("expressible");
            let cost = derive_court_cost_v1(&p).expect("derivable");
            let canonical = context(&p, 8, 4);
            let Ok(counted) = step_leaf_count(&p, &canonical) else { continue };
            match verify_class_admission_v2(&default_bundle, &p, &canonical, &registration(p.shape_profile_id(), counted)) {
                Ok(_) => {
                    assert!(cost.max_opening_bytes <= crate::palw_mode_v2::DEFAULT_MAX_OPENING_BYTES);
                    fits.push(tile);
                }
                Err(PalwClassAdmissionError::CourtCostExceedsCeiling { .. }) => {
                    assert!(cost.max_opening_bytes > crate::palw_mode_v2::DEFAULT_MAX_OPENING_BYTES);
                }
                Err(e) => panic!("tile {tile}: unexpected {e:?}"),
            }
        }
        assert_eq!(fits, vec![64], "only the smallest tile fits a floor-sized court, and it buys a toy context");
        assert!(widest_adjudicable_ctx(64) < 200, "the tile that fits a floor-sized court buys a toy context");

        // **The declared 4,096-token context is not adjudicable at ANY legal tile length**, and
        // that is a change: against the hand-written 27-node layer table `tile_len` 16,384 reached
        // it and cost 24 MiB an opening. The table is projected from `BASE0_LAYER_IR` now and the
        // engine performs 38 steps per layer, so the whole legal tile range — up to
        // `PALW_STEP_MAX_TILE_LEN` — is short. Swept rather than asserted at one tile, because
        // "16,384 works" was exactly the shape of claim that went stale silently.
        let mut admits_full_ctx = Vec::new();
        let mut tile = crate::palw_step::PALW_STEP_MIN_TILE_LEN;
        while tile <= crate::palw_step::PALW_STEP_MAX_TILE_LEN {
            if let Ok(p) = qwen25_profile_v1(PalwQwen25GeometryV1 { tile_len: tile, ..QWEN25_1_5B })
                && worst_case_step_leaf_count_v1(&p).is_ok()
            {
                admits_full_ctx.push(tile);
            }
            tile = tile.saturating_mul(2);
        }
        assert!(
            admits_full_ctx.is_empty(),
            "a tile length reached the declared 4096 context ({admits_full_ctx:?}) — the sizing table needs updating with it"
        );
    }

    /// **The shipped Qwen geometries do not admit their own declared context**, and coverage
    /// cannot see it.
    ///
    /// `worst_case_step_leaf_count_v1` is the whole context as prefill — the longest job a class
    /// admits — and both shipped constants are far past `PALW_STEP_MAX_LEAVES` at `tile_len` 128:
    /// 132.4 M leaves for 1.5B and 219.7 M for 3B. `tile_len` is the only knob that moves it, and
    /// measured, 1.5B needs 16,384 to reach `n_ctx` 4096 while 3B needs 65,536 — which is
    /// `PALW_STEP_MAX_TILE_LEN` exactly, so the 3B class at 4096 sits on the type's own ceiling
    /// with no headroom.
    ///
    /// This test fails the moment either constant changes, which is the point: it is a tripwire on
    /// a pair of numbers that pass every other gate.
    #[test]
    fn the_shipped_qwen_tile_len_does_not_admit_its_own_declared_context() {
        for shipped in [QWEN25_1_5B, QWEN25_3B] {
            let as_shipped = qwen25_profile_v1(shipped).expect("expressible");
            assert!(
                worst_case_step_leaf_count_v1(&as_shipped).is_err(),
                "a shipped Qwen geometry became admissible — update this tripwire and the sizing table with it"
            );
        }
        // The DERIVED geometry is admissible — that is what makes the class registrable at all.
        assert!(worst_case_step_leaf_count_v1(&qwen_admissible()).is_ok(), "the derived 1.5B geometry is inside the step space");
        // And the declared context is not reachable at the maximum legal tile either, for either
        // member. Against the hand-written table 1.5B reached 4096 at 16,384 and 3B at 65,536;
        // the projected graph is bigger and neither does.
        for shipped in [QWEN25_1_5B, QWEN25_3B] {
            let widest = qwen25_profile_v1(PalwQwen25GeometryV1 { tile_len: crate::palw_step::PALW_STEP_MAX_TILE_LEN, ..shipped })
                .expect("expressible at the maximum tile");
            assert!(
                worst_case_step_leaf_count_v1(&widest).is_err(),
                "the declared context became reachable at the maximum tile — update the sizing table"
            );
        }
    }

    // ---------------------------------------------------------------------------------------
    // ADR-0051: the Metal/GGUF family's own gate
    // ---------------------------------------------------------------------------------------

    use crate::palw_backend::PalwExecutionFamilyV1;
    use crate::palw_state_v2::PalwClassTermsV2;

    fn metal_pins() -> crate::palw_state_v2::PalwRuntimePinsV2 {
        crate::palw_state_v2::PalwRuntimePinsV2 {
            runtime_manifest_hash: Hash64::from_u64_word(0x1),
            runtime_class_id: Hash64::from_u64_word(0x2),
            model_profile_id: Hash64::from_u64_word(0x3),
            trace_scheme_id: Hash64::from_u64_word(0x5),
            cu_ruleset_id: Hash64::from_u64_word(0x6),
            tokenizer_id: Hash64::from_u64_word(0x4),
            prefill_tokens: 8,
            exact_decode_tokens: 4,
            max_context_tokens: 512,
            vocab_size: base0_profile_v1(PALW_RC_BASE0_GEOMETRY).unwrap().vocab_size,
        }
    }

    fn metal_terms(seats: Option<u16>, quorum: Option<u16>) -> PalwClassTermsV2 {
        PalwClassTermsV2 {
            family: PalwExecutionFamilyV1::MetalGguf,
            runtime_pins: Some(metal_pins()),
            panel_seats: seats,
            panel_quorum: quorum,
        }
    }

    /// A canonical job with every Family-M identity pinned. The profile is the floor's — a
    /// Family-M class's "graph" is its pinned identity set, and the shape only has to be
    /// well-formed and wide enough for the job.
    fn metal_job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
        let mut c = context(profile, prefill, decode);
        c.runtime_manifest_hash = Hash64::from_u64_word(0x1);
        c.runtime_class_id = Hash64::from_u64_word(0x2);
        c.model_profile_id = Hash64::from_u64_word(0x3);
        c.tokenizer_id = Hash64::from_u64_word(0x4);
        c.trace_scheme_id = Hash64::from_u64_word(0x5);
        c.cu_ruleset_id = Hash64::from_u64_word(0x6);
        // The pins and the job are one statement, so the fixture states it once: the job's shape
        // must be the pins' too, or the correspondence check refuses — as it should, and as it did
        // the first time this fixture set only half of them.
        c.max_context_tokens = 512;
        c
    }

    fn metal_registration(class_id: Hash64, terms: PalwClassTermsV2, pwu: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::ClassRegistered {
            class_id,
            terms,
            artifact_root: Hash64::from_u64_word(0xA7),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: pwu },
            initial_target: u128::MAX / 2,
            share_permille: 1,
            activation_daa: 0,
            admission: None,
        }
    }

    /// **A Metal class is admitted on its pins, not on a step space** — and the step-space checks
    /// that would refuse it are not run, because a black-box runtime has no step space to fail.
    #[test]
    fn a_metal_class_is_admitted_on_its_pinned_identity() {
        let mut bundle = bundle_with_full_ladder();
        bundle.min_class_panel = (2, 2);
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let job = metal_job(&profile, 8, 4);
        let reg = metal_registration(profile.shape_profile_id(), metal_terms(Some(2), Some(2)), 4);
        let entry = verify_class_admission_v2(&bundle, &profile, &job, &reg).expect("a pinned Metal class is admissible");
        assert_eq!(entry.class_id, profile.shape_profile_id());
        // pwu is the DECODE BUDGET for this family, counted from the canonical job.
        assert_eq!(entry.canonical_step_leaf_count, 4);
    }

    /// **An unpinned identity is refused by name.** A class that did not pin its runtime is one
    /// any runtime could claim to be, and this family re-derives no arithmetic to catch that.
    #[test]
    fn a_metal_class_must_pin_every_identity() {
        let mut bundle = bundle_with_full_ladder();
        bundle.min_class_panel = (2, 2);
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        for clear in ["runtime_manifest_hash", "runtime_class_id", "model_profile_id", "tokenizer_id"] {
            let mut job = metal_job(&profile, 8, 4);
            match clear {
                "runtime_manifest_hash" => job.runtime_manifest_hash = Hash64::default(),
                "runtime_class_id" => job.runtime_class_id = Hash64::default(),
                "model_profile_id" => job.model_profile_id = Hash64::default(),
                _ => job.tokenizer_id = Hash64::default(),
            }
            let reg = metal_registration(profile.shape_profile_id(), metal_terms(Some(2), Some(2)), 4);
            let err = verify_class_admission_v2(&bundle, &profile, &job, &reg).expect_err("an unset pin is refused");
            assert!(format!("{err}").contains(clear), "the refusal names the missing pin: {err}");
        }
    }

    /// `pwu_per_inference` is counted here too — the unit differs (decode tokens, not step
    /// leaves) but the rule does not: a registrant that could declare it could mint weight.
    #[test]
    fn a_metal_class_cannot_declare_its_own_pwu() {
        let mut bundle = bundle_with_full_ladder();
        bundle.min_class_panel = (2, 2);
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let job = metal_job(&profile, 8, 4);
        let reg = metal_registration(profile.shape_profile_id(), metal_terms(Some(2), Some(2)), 400);
        match verify_class_admission_v2(&bundle, &profile, &job, &reg) {
            Err(PalwClassAdmissionError::PwuPerInferenceMismatch { declared, counted }) => {
                assert_eq!((declared, counted), (400, 4));
            }
            other => panic!("an overstated pwu must be refused, got {other:?}"),
        }
    }

    /// **The panel floor is real in both directions**: below the bundle's floor is refused, above
    /// the network's own draw is refused, and half-specified is refused.
    #[test]
    fn the_per_class_panel_is_floored_and_capped() {
        let mut bundle = bundle_with_full_ladder();
        bundle.min_class_panel = (2, 2);
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let job = metal_job(&profile, 8, 4);
        let net_seats = bundle.panel.seat_count();

        for (seats, quorum, why) in [
            (Some(1u16), Some(1u16), "below the floor"),
            (Some(net_seats + 1), Some(2), "above the network's own draw"),
            (Some(2), None, "half-specified"),
            (None, Some(2), "half-specified the other way"),
        ] {
            let reg = metal_registration(profile.shape_profile_id(), metal_terms(seats, quorum), 4);
            assert!(
                matches!(
                    verify_class_admission_v2(&bundle, &profile, &job, &reg),
                    Err(PalwClassAdmissionError::PanelTerms(_))
                ),
                "a panel {why} must be refused"
            );
        }
        // And `None`/`None` — the network's own panel — is always admissible.
        let reg = metal_registration(profile.shape_profile_id(), metal_terms(None, None), 4);
        assert!(verify_class_admission_v2(&bundle, &profile, &job, &reg).is_ok());
    }

    /// **The deterministic family is unchanged by all of this.** A Family-D registration still
    /// goes through the step-space gate, which is the half of the network that can convict.
    #[test]
    fn the_deterministic_gate_is_untouched() {
        let bundle = bundle_with_full_ladder();
        let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("expressible");
        let canonical = context(&profile, 8, 4);
        let counted = step_leaf_count(&profile, &canonical).expect("the floor counts");
        let reg = PalwConsensusObjectV2::ClassRegistered {
            class_id: profile.shape_profile_id(),
            terms: PalwClassTermsV2::deterministic_default(),
            artifact_root: Hash64::from_u64_word(0xA1),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
            initial_target: u128::MAX / 2,
            share_permille: 1000,
            activation_daa: 0,
            admission: None,
        };
        let entry = verify_class_admission_v2(&bundle, &profile, &canonical, &reg).expect("the floor is admissible");
        assert_eq!(entry.canonical_step_leaf_count, counted, "still counted in STEP LEAVES, not decode tokens");
        assert!(!entry.reachable_kernels.is_empty(), "and still coverage-checked");
    }

}
