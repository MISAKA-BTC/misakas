//! **RFC-0004 §6.3, PALW-MIP-15: composite artifacts** — a candidate that is its parent plus an
//! adapter section, the parent's inventory reused byte for byte and never re-committed.
//!
//! A candidate lowered with its adapter's params last (`misaka-palw-tir-lower`'s
//! `lower::adapter_params_last`) declares the parent program's params `0..P` first, unchanged, and
//! its adapter's params `P..` after. Its inventory (spec 04b §10, [`crate::palw_tir_artifact_v1`]) is
//! therefore two SECTIONS: the leaves of params `0..P`, which are the parent's inventory leaf for
//! leaf, and the leaves of params `P..`, the adapter section. A composite artifact commits the two
//! apart:
//!
//! ```text
//! artifact_root(Composite) = H64(key "misaka-palw/improve/composite-artifact/v1",
//!                                parent_class ‖ parent_root ‖ adapter_root ‖ le32(P))
//! tir_class_id_v1          = Phase F's formula over that root, unchanged
//! ```
//!
//! — `parent_root` the parent class's inventory root (the section `0..P`), `adapter_root` the root of
//! the section `P..` as a tree of its own ([`crate::palw_tir_artifact_v1::palw_tir_inventory_section_root_v1`]).
//!
//! **Court openings** ([`PalwTirCompositeOpeningV1`], the appended form of an IR close's parameter
//! carriage, [`crate::palw_tir_court_v1::PalwTirParamOpeningV1::Composite`]): the candidate's
//! inventory index is the court's, as for any class; a leaf `v` below the split opens at `v` under
//! `parent_root`, a leaf at or past it at `v − split` under `adapter_root`, and the reference rides
//! with the openings — it must hash to the class's artifact root, so the court believes nothing and
//! needs no registry. Past `palw_improvement_v1` only.
//!
//! **Admission of a composite** ([`palw_tir_composite_admits_v1`]): the family rule (the parent's
//! tokenizer, token bound, primitive set and logits scheme and row), the composite rule (params
//! `0..P` are the parent's, declaration and layout alike; no adapter param names a parent tensor;
//! every block within 512 nodes), the reference (the parent's root, the class's artifact root, the
//! class id over it) and every terminal close carriable in the composite form (`TirCloseDemandV1`,
//! [`crate::palw_tir_close_size_v1::PalwTirParamFormV1::Composite`]).

use std::collections::BTreeSet;

use crate::Hash64;
use crate::palw_artifact::PalwArtifactMultiproofV1;
use crate::palw_tir_artifact_v1::{
    PalwTirInventoryError, PalwTirTensorSourceV1, palw_tir_inventory_leaf_count_v1, palw_tir_inventory_section_root_v1,
    palw_tir_param_instances_v1,
};
use crate::palw_tir_class_v1::PalwTirClassV1;
use crate::palw_tir_close_size_v1::PalwTirCloseBoundV1;
use misaka_palw_tir::TirProgramV1;

/// The key of a composite artifact root (RFC-0004 §6.3) — one spelling, the reference type's module's.
pub use crate::palw_improve_artifact_v1::PALW_IMPROVE_COMPOSITE_ARTIFACT_DOMAIN_V1;

/// **A composite artifact root** over its four parts (RFC-0004 §6.3) — the reference type's
/// ([`crate::palw_improve_artifact_v1::palw_improve_composite_artifact_root_v1`]), one spelling.
pub fn palw_improve_composite_root_v1(parent_class: &Hash64, parent_root: &Hash64, adapter_root: &Hash64, p: u32) -> Hash64 {
    crate::palw_improve_artifact_v1::palw_improve_composite_artifact_root_v1(parent_class, parent_root, adapter_root, p)
}

/// **What a composite artifact is**: the parent class, the parent's inventory root, the adapter
/// section's root and `P`, the number of the parent's params the candidate declares first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirCompositeRefV1 {
    pub parent_class: Hash64,
    pub parent_root: Hash64,
    pub adapter_root: Hash64,
    pub p: u32,
}

impl PalwTirCompositeRefV1 {
    /// The artifact root the candidate's class id commits to.
    pub fn artifact_root(&self) -> Hash64 {
        palw_improve_composite_root_v1(&self.parent_class, &self.parent_root, &self.adapter_root, self.p)
    }
}

/// **The sub-root openings of a composite artifact** — an IR close's parameter carriage for a
/// composite class: the leaves of params `0..p` as ONE multiproof under `parent_root` (at their own
/// indices, which are the parent's), the leaves of params `p..` as ONE under `adapter_root` (rebased
/// to the section), either absent when no leaf of its section is read, never both.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirCompositeOpeningV1 {
    pub artifact: PalwTirCompositeRefV1,
    pub parent: Option<PalwArtifactMultiproofV1>,
    pub adapter: Option<PalwArtifactMultiproofV1>,
}

/// Why a candidate is not a composite of its parent, or not admissible as one.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwTirCompositeErrorV1 {
    #[error("a program does not decode in normal form: {0}")]
    Program(String),
    #[error("the candidate is not of its parent's family: {0}")]
    Family(&'static str),
    #[error("P is {p}: a composite's first P params are ALL of its parent's {parent}")]
    NotTheParentsParams { p: u32, parent: usize },
    #[error("the candidate declares no param past its parent's {0}: no adapter section")]
    NoAdapterSection(u32),
    #[error("param {index} (`{name}`) is not the parent's declaration")]
    ParentParamChanged { index: u16, name: String },
    #[error("param {index} (`{name}`) has other instances than the parent's: its leaves would not be the parent's")]
    ParentInstancesChanged { index: u16, name: String },
    #[error("adapter param {index} names the parent's tensor `{name}`")]
    AdapterNamesAParentTensor { index: u16, name: String },
    #[error("block {block} has {nodes} nodes, past the 512 a block may hold (NF-12)")]
    NodeBudget { block: u8, nodes: usize },
    #[error("the composite names parent root {named}; the parent class commits {committed}")]
    NotTheParentRoot { named: Hash64, committed: Hash64 },
    #[error("the composite names parent class {named}, not the parent given ({given})")]
    NotTheParentClass { named: Hash64, given: Hash64 },
    #[error("the composite artifact roots to {derived}; the class commits {committed}")]
    NotTheClassArtifact { derived: Hash64, committed: Hash64 },
    #[error("the class id over the composite is {derived}; the class is {registered}")]
    NotTheClassId { derived: Hash64, registered: Hash64 },
    #[error("the inventory: {0}")]
    Inventory(String),
    #[error("admission: {0}")]
    Admission(String),
}

impl From<PalwTirInventoryError> for PalwTirCompositeErrorV1 {
    fn from(e: PalwTirInventoryError) -> Self {
        Self::Inventory(e.to_string())
    }
}

/// **Where a candidate's inventory splits at param `p`**: the leaves of params `0..p` (the parent
/// section) and of params `p..` (the adapter section).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirCompositeSplitV1 {
    pub parent_leaves: u32,
    pub adapter_leaves: u32,
}

/// The split of `program`'s inventory at param `p` (`p` at most the param count).
pub fn palw_tir_composite_split_v1(program: &TirProgramV1, p: u32) -> Result<PalwTirCompositeSplitV1, PalwTirCompositeErrorV1> {
    let total = palw_tir_inventory_leaf_count_v1(program)?;
    let index = crate::palw_tir_court_v1::PalwTirInventoryIndexV1::new(program)
        .ok_or_else(|| PalwTirCompositeErrorV1::Inventory("the program's inventory has no index".into()))?;
    let at = u16::try_from(p)
        .ok()
        .and_then(|p| index.leaves_before(p))
        .ok_or(PalwTirCompositeErrorV1::NotTheParentsParams { p, parent: program.params.len() })?;
    Ok(PalwTirCompositeSplitV1 { parent_leaves: at, adapter_leaves: total - at })
}

/// **The family rule** (RFC-0004 §2.1 #1, §6.1): the candidate reads the parent's tokens and answers
/// in the parent's output interface — the same tokenizer, token bound and primitive set, the same
/// logits scheme, and a logits node of the same type.
pub fn palw_tir_family_rule_v1(
    parent: &PalwTirClassV1,
    parent_program: &TirProgramV1,
    candidate: &PalwTirClassV1,
    candidate_program: &TirProgramV1,
) -> Result<(), PalwTirCompositeErrorV1> {
    type E = PalwTirCompositeErrorV1;
    if candidate.tokenizer_id != parent.tokenizer_id {
        return Err(E::Family("another tokenizer"));
    }
    if candidate_program.token_bound != parent_program.token_bound {
        return Err(E::Family("another token bound"));
    }
    if candidate_program.prim_set_id != parent_program.prim_set_id {
        return Err(E::Family("another primitive set"));
    }
    if candidate_program.logits_scheme_id != parent_program.logits_scheme_id {
        return Err(E::Family("another logits scheme"));
    }
    let logits =
        |p: &TirProgramV1| p.blocks.get(p.schedule.post as usize).and_then(|b| b.nodes.get(p.logits as usize)).map(|n| n.out.clone());
    if logits(candidate_program).is_none() || logits(candidate_program) != logits(parent_program) {
        return Err(E::Family("another logits row"));
    }
    Ok(())
}

/// **The composite rule** (RFC-0004 §6.3, §6.5; PALW-MIP-15): the candidate's params `0..p` are the
/// parent program's params — all of them, in order, declaration for declaration — laid out at the
/// same instances, so its first leaves ARE the parent's inventory; it declares an adapter section
/// past them, none of whose params names a parent tensor; and every block holds at most 512 nodes.
pub fn palw_tir_composite_rule_v1(parent: &TirProgramV1, candidate: &TirProgramV1, p: u32) -> Result<(), PalwTirCompositeErrorV1> {
    type E = PalwTirCompositeErrorV1;
    if p as usize != parent.params.len() {
        return Err(E::NotTheParentsParams { p, parent: parent.params.len() });
    }
    let p = p as usize;
    if candidate.params.len() <= p {
        return Err(E::NoAdapterSection(p as u32));
    }
    for (j, (c, q)) in candidate.params[..p].iter().zip(&parent.params).enumerate() {
        if c != q {
            return Err(E::ParentParamChanged { index: j as u16, name: c.name.clone() });
        }
    }
    let (ic, ip) = (palw_tir_param_instances_v1(candidate), palw_tir_param_instances_v1(parent));
    for j in 0..p {
        if ic[j] != ip[j] {
            return Err(E::ParentInstancesChanged { index: j as u16, name: candidate.params[j].name.clone() });
        }
    }
    let names: BTreeSet<&str> = parent.params.iter().map(|d| d.name.as_str()).collect();
    for (j, d) in candidate.params.iter().enumerate().skip(p) {
        if names.contains(d.name.as_str()) {
            return Err(E::AdapterNamesAParentTensor { index: j as u16, name: d.name.clone() });
        }
    }
    for (b, block) in candidate.blocks.iter().enumerate() {
        if block.nodes.len() > misaka_palw_tir::program::MAX_NODES_PER_BLOCK {
            return Err(E::NodeBudget { block: b as u8, nodes: block.nodes.len() });
        }
    }
    Ok(())
}

/// **A candidate's two section roots**, from its tensors: `((parent section root, leaves),
/// (adapter section root, leaves))`. When the composite rule holds, the first is the parent's
/// inventory root.
#[allow(clippy::type_complexity)]
pub fn palw_tir_composite_section_roots_v1(
    candidate: &TirProgramV1,
    p: u32,
    src: &dyn PalwTirTensorSourceV1,
) -> Result<((Hash64, u32), (Hash64, u32)), PalwTirCompositeErrorV1> {
    let n = u16::try_from(candidate.params.len()).map_err(|_| PalwTirCompositeErrorV1::Inventory("too many params".into()))?;
    let p16 =
        u16::try_from(p).ok().filter(|p| *p <= n).ok_or(PalwTirCompositeErrorV1::NotTheParentsParams { p, parent: n as usize })?;
    Ok((palw_tir_inventory_section_root_v1(candidate, 0..p16, src)?, palw_tir_inventory_section_root_v1(candidate, p16..n, src)?))
}

/// **The composite reference of a candidate** whose tensors `src` holds, under `parent_class`.
pub fn palw_tir_composite_ref_v1(
    parent_class: Hash64,
    candidate: &TirProgramV1,
    p: u32,
    src: &dyn PalwTirTensorSourceV1,
) -> Result<PalwTirCompositeRefV1, PalwTirCompositeErrorV1> {
    let ((parent_root, _), (adapter_root, _)) = palw_tir_composite_section_roots_v1(candidate, p, src)?;
    Ok(PalwTirCompositeRefV1 { parent_class, parent_root, adapter_root, p })
}

/// What a composite's admission reads beside the two classes.
#[derive(Clone, Copy, Debug)]
pub struct PalwTirCompositeAdmissionV1 {
    /// Whether history cones are dissected (the k-ary court is armed).
    pub court: bool,
    /// The most bytes one close can be carried in (`palw_tir_carriable_close_bytes_v1`).
    pub carriable: u64,
    /// The sizing's work cap.
    pub work_cap: u64,
}

/// **Admission of a composite IR class** (RFC-0004 §6.3): `candidate` (its program filled) is the
/// class `class_id`, registered with `artifact_root`, of the composite `r` over `parent` (the class
/// `parent_class_id`, registered with `parent_root`). Checks the reference (its parent, its parent
/// root, the class's artifact root and the class id over it — Phase F's formula over the composite),
/// the family rule, the composite rule, and every terminal close of the candidate carriable in the
/// composite form. Returns the close bounds.
#[allow(clippy::too_many_arguments)]
pub fn palw_tir_composite_admits_v1(
    parent: &PalwTirClassV1,
    parent_class_id: &Hash64,
    parent_root: &Hash64,
    candidate: &PalwTirClassV1,
    class_id: &Hash64,
    artifact_root: &Hash64,
    r: &PalwTirCompositeRefV1,
    rules: &PalwTirCompositeAdmissionV1,
) -> Result<Vec<PalwTirCloseBoundV1>, PalwTirCompositeErrorV1> {
    type E = PalwTirCompositeErrorV1;
    if r.parent_class != *parent_class_id {
        return Err(E::NotTheParentClass { named: r.parent_class, given: *parent_class_id });
    }
    if r.parent_root != *parent_root {
        return Err(E::NotTheParentRoot { named: r.parent_root, committed: *parent_root });
    }
    let derived = r.artifact_root();
    if derived != *artifact_root {
        return Err(E::NotTheClassArtifact { derived, committed: *artifact_root });
    }
    let id = candidate.class_id(&derived);
    if id != *class_id {
        return Err(E::NotTheClassId { derived: id, registered: *class_id });
    }
    let parent_program = parent.decode_program().map_err(|e| E::Program(e.to_string()))?;
    let candidate_program = candidate.decode_program().map_err(|e| E::Program(e.to_string()))?;
    palw_tir_family_rule_v1(parent, &parent_program, candidate, &candidate_program)?;
    palw_tir_composite_rule_v1(&parent_program, &candidate_program, r.p)?;
    // Every terminal close carriable as a composite carries it, over the layout's longest job.
    let space = crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(candidate).map_err(|e| E::Admission(e.to_string()))?;
    let longest = crate::palw_tir_attempt_v1::palw_tir_canonical_context_v1(candidate, *class_id, (1, candidate.layout.max_context))
        .ok_or_else(|| E::Admission("the layout has no longest job".into()))?;
    crate::palw_tir_admission_v1::palw_tir_carried_closes_admit_form_v1(
        &space,
        &space.program,
        &longest,
        crate::palw_tir_close_size_v1::PalwTirParamFormV1::Composite { p: r.p },
        rules.court,
        rules.carriable,
        rules.work_cap,
    )
    .map_err(|e| E::Admission(e.to_string()))
}

/// **Whether `object` carries a composite parameter opening** anywhere an IR close rides — a court
/// close, a one-move accusation's proof, a root claim's finalize, an IR certification drill's
/// refutations. The composite form is APPENDED (tag 2 of
/// [`crate::palw_tir_court_v1::PalwTirParamOpeningV1`]): an older build cannot decode it and skips
/// the payload (A-2), so this build drops such an object by name below `palw_improvement_v1` — the
/// same outcome, byte for byte.
pub fn palw_object_carries_composite_opening_v1(object: &crate::palw_state_v2::PalwConsensusObjectV2) -> bool {
    use crate::palw_court_v2::PalwCourtVerdictProofV2 as P;
    use crate::palw_state_v2::{PalwCertificationEvidenceV1 as Ev, PalwConsensusObjectV2 as O};
    let proof = |p: &P| match p {
        P::TirCone { refutation } => refutation.params.is_composite(),
        P::TirDissection { bottom } => bottom.params.is_composite(),
        _ => false,
    };
    match object {
        O::CourtClosed { proof: p, .. } => proof(p),
        O::TirShardCourtAccused { accusation } => proof(&accusation.proof),
        O::CourtTirRootClaimed { root, .. } => root.finalize.params.is_composite(),
        O::FamilyCertified { evidence } => match evidence.as_ref() {
            Ev::TirAttempt(drill) => drill.vectors.iter().any(|v| v.honest.params.is_composite() || v.guilty.params.is_composite()),
            _ => false,
        },
        _ => false,
    }
}

/// **A candidate's artifact, as its submission names it** (RFC-0004 §6.1): full weights under one
/// root, or a composite over its parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirCandidateArtifactV1<'a> {
    /// Full weights: the class's own inventory root.
    Single(Hash64),
    /// Parent + adapter.
    Composite(&'a PalwTirCompositeRefV1),
}

/// **What the chain holds for checking a candidate's artifact** (RFC-0004 §6.1, §6.3): the
/// candidate's and its parent's `tir_classes` records and registered artifact roots, the layout the
/// submission carries (a record keeps only its digest), whether the line's policy admits full
/// weights, and admission's sizing rules. `parent_class_id` is the parent the caller resolved — the
/// line's head for full weights, the composite's named parent (a class of the line, the caller's
/// check) for a composite.
#[derive(Clone, Copy, Debug)]
pub struct PalwTirCandidateFactsV1<'a> {
    pub record: &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    pub artifact_root: Hash64,
    pub layout: &'a crate::palw_tir_class_v1::PalwTirLayoutV1,
    pub parent_class_id: Hash64,
    pub parent_record: &'a crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    pub parent_root: Hash64,
    pub full_weights_allowed: bool,
    pub rules: PalwTirCompositeAdmissionV1,
}

/// The class a record and a layout make (its program as the record keeps it).
fn class_of(
    record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1,
    layout: &crate::palw_tir_class_v1::PalwTirLayoutV1,
) -> PalwTirClassV1 {
    PalwTirClassV1 {
        version: crate::palw_tir_class_v1::PALW_TIR_CLASS_VERSION_V1,
        program: record.program.as_ref().clone(),
        layout: layout.clone(),
        tokenizer_id: record.tokenizer_id,
    }
}

/// **A candidate's artifact is admissible** (RFC-0004 §2.1 #1, §6.1, §6.3; PALW-MIP-8, PALW-MIP-15):
/// the class `class_id` is rebuilt from its record and the carried layout (whose digest the record
/// holds); full weights must be allowed by the policy, be the class's registered root, and be of the
/// parent's family; a composite must pass [`palw_tir_composite_admits_v1`] over the parent's record.
/// Returns the carried-close bounds a composite was sized to (none for full weights, whose closes
/// admission v10 sized at registration).
pub fn palw_tir_candidate_artifact_admits_v1(
    class_id: &Hash64,
    artifact: PalwTirCandidateArtifactV1<'_>,
    facts: &PalwTirCandidateFactsV1<'_>,
) -> Result<Vec<PalwTirCloseBoundV1>, PalwTirCompositeErrorV1> {
    type E = PalwTirCompositeErrorV1;
    for record in [facts.record, facts.parent_record] {
        record.check_program_v1().map_err(|why| E::Program(why.into()))?;
    }
    let class = class_of(facts.record, facts.layout);
    if class.layout_digest() != facts.record.layout_digest {
        return Err(E::Admission("the carried layout is not the class's (its digest differs from the record's)".into()));
    }
    // The parent's layout plays no part in either rule; its record's program and tokenizer do.
    let parent = class_of(facts.parent_record, facts.layout);
    match artifact {
        PalwTirCandidateArtifactV1::Single(root) => {
            if !facts.full_weights_allowed {
                return Err(E::Admission("the line's policy admits no full-weight candidate".into()));
            }
            if root != facts.artifact_root {
                return Err(E::NotTheClassArtifact { derived: root, committed: facts.artifact_root });
            }
            let id = class.class_id(&root);
            if id != *class_id {
                return Err(E::NotTheClassId { derived: id, registered: *class_id });
            }
            let (pp, cp) = (
                parent.decode_program().map_err(|e| E::Program(e.to_string()))?,
                class.decode_program().map_err(|e| E::Program(e.to_string()))?,
            );
            palw_tir_family_rule_v1(&parent, &pp, &class, &cp)?;
            Ok(Vec::new())
        }
        PalwTirCandidateArtifactV1::Composite(r) => palw_tir_composite_admits_v1(
            &parent,
            &facts.parent_class_id,
            &facts.parent_root,
            &class,
            class_id,
            &facts.artifact_root,
            r,
            &facts.rules,
        ),
    }
}
