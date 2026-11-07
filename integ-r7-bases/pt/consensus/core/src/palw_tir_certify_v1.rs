//! **RFC-0002 Phase F, step F6: the IR family certifier** — `certify_e2e_family_v1`'s twin for an IR
//! class (design §5 Q6: a TIR family certified for the primitive set, drilled end to end).
//!
//! A drill plants faults in an IR class's execution and records, per planted leaf, the honest run's
//! refutation (which the shipped IR court must ACQUIT) and the corrupted run's (which it must
//! CONVICT), with the two runs' prefix commitments around the leaf. [`certify_tir_e2e_family_v1`]
//! re-runs the shipped court (`check_tir_cone_refutation_v1`) over every vector and certifies a family
//! whose `kernel_ids` are the program's primitives (`palw-tir/v1/prim=<Name>`, read off the program,
//! never supplied) only if the convicted leaves' cones cover every one of them, in a prefill and a
//! decode position, and the family answered malformed material without crashing.
//!
//! **The program rides once.** Every refutation's binding carries its class whole, and a class's
//! program is up to ~88 KB; a vector therefore carries its bindings with the program EMPTY
//! ([`palw_tir_strip_program_v1`]) and the grader puts the evidence's class back before it reads
//! anything — so the class every vector is graded against is the one the certificate names.
//!
//! The certifier holds no ruleset: it grades at the structural step-leaf cap and at fixed work
//! limits ([`PALW_TIR_CERTIFY_LIMITS_V1`], the order of testnet-12's court) — an evaluation past them
//! is not graded, so a certification object cannot buy unbounded work.

use crate::Hash64;
use crate::palw_e2e_adjudicability::{PalwE2eCertificateV1, PalwE2eCoveringV1, PalwE2eError, PalwE2eFamilyV1};
use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use crate::palw_step_refute::PalwStepRefuteError;
use crate::palw_tir_class_v1::PalwTirClassV1;
use crate::palw_tir_court_v1::{PalwTirConeRefutationV1, PalwTirCourtRulesV1, check_tir_cone_refutation_v1};
use crate::palw_tir_step_v1::{PalwTirLeafKindV1, PalwTirStepSpaceV1};
use misaka_palw_tir::demand::DemandLimits;
use misaka_palw_tir::{Prim, TirProgramV1};

/// The work one graded refutation may cost: `2^26` computed elements and reduction terms each —
/// testnet-12's IR court limits (four times its 16 Mi terminal MACs).
pub const PALW_TIR_CERTIFY_LIMITS_V1: DemandLimits = DemandLimits { max_elements: 1 << 26, max_terms: 1 << 26 };

/// One planted fault and the two answers the IR court gives it.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirE2eFaultVectorV1 {
    /// The leaf the fault was planted at, which both refutations open.
    pub leaf_index: u64,
    /// The honest run's refutation at the leaf (its binding's program empty). Must ACQUIT.
    pub honest: PalwTirConeRefutationV1,
    /// The corrupted run's refutation at the leaf (its binding's program empty). Must CONVICT.
    pub guilty: PalwTirConeRefutationV1,
    /// `(through the leaf's predecessor, through the leaf)` for each run: agreeing before the fault
    /// and differing once it is included — the rung a ladder converges on.
    pub honest_prefix: (Hash64, Hash64),
    pub guilty_prefix: (Hash64, Hash64),
}

/// Everything an IR drill records, in the form [`certify_tir_e2e_family_v1`] grades.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirE2eDrillEvidenceV1 {
    pub family_id: Hash64,
    /// The class the drill ran — its program the certificate's primitives are read off.
    pub class: PalwTirClassV1,
    /// The class's inventory root: every refutation's parameter openings prove against it.
    pub artifact_root: Hash64,
    pub vectors: Vec<PalwTirE2eFaultVectorV1>,
    /// How many malformed inputs the family's seat verb answered instead of crashing on; a zero is
    /// refused, as for the legacy families.
    pub malformed_inputs_refused: u32,
}

/// **Empties a refutation's carried program** — what a drill does to each refutation before it
/// files the evidence (the evidence carries the class once).
pub fn palw_tir_strip_program_v1(refutation: &mut PalwTirConeRefutationV1) {
    refutation.binding.class.program = Vec::new();
}

/// The refutation with the evidence's class put back, refused unless the vector carried the same
/// class with its program stripped.
fn rebind(refutation: &PalwTirConeRefutationV1, class: &PalwTirClassV1, leaf: u64) -> Result<PalwTirConeRefutationV1, PalwE2eError> {
    let carried = &refutation.binding.class;
    if !carried.program.is_empty()
        || carried.version != class.version
        || carried.layout != class.layout
        || carried.tokenizer_id != class.tokenizer_id
    {
        return Err(PalwE2eError::VectorIsAboutAnotherGraph { leaf, drilled: class.graph_ir_root(), vector: carried.layout_digest() });
    }
    let mut r = refutation.clone();
    r.binding.class = class.clone();
    Ok(r)
}

/// The primitives of the cone a leaf's value is computed by: a commit point's cone, a `Fixed`
/// state's writer's cone, or a history's append's cone.
fn cone_prims(program: &TirProgramV1, kind: &PalwTirLeafKindV1) -> Vec<&'static str> {
    let cone_of = |block: usize, node: usize| {
        misaka_palw_tir::admit::cone_nodes(program, block, node)
            .into_iter()
            .map(|i| program.blocks[block].nodes[i as usize].prim.name())
            .collect::<Vec<_>>()
    };
    let writers = |state: u16, append: bool| {
        let mut out = Vec::new();
        for (bi, b) in program.blocks.iter().enumerate() {
            for (ni, n) in b.nodes.iter().enumerate() {
                let hit = match n.prim {
                    Prim::StateWrite { state: s } => !append && s == state,
                    Prim::HistAppend { state: s } => append && s == state,
                    _ => false,
                };
                if hit {
                    out.extend(cone_of(bi, ni));
                }
            }
        }
        out
    };
    match *kind {
        PalwTirLeafKindV1::Commit { block, node, .. } => cone_of(block as usize, node as usize),
        PalwTirLeafKindV1::State { state, .. } => writers(state, false),
        PalwTirLeafKindV1::HistTile { state, .. } => writers(state, true),
    }
}

/// **Grade an IR drill.** The certificate's family is the class's primitive set, certified only when
/// every vector's honest refutation acquits and its guilty one convicts under the shipped IR court,
/// the rungs are prefix commitments, and the convicted leaves cover every primitive the program
/// reaches, in a prefill and a decode position.
pub fn certify_tir_e2e_family_v1(evidence: &PalwTirE2eDrillEvidenceV1) -> Result<PalwE2eCertificateV1, PalwE2eError> {
    let program = evidence.class.decode_program().map_err(|e| PalwE2eError::Profile(e.to_string()))?;
    let space = PalwTirStepSpaceV1::new(&evidence.class).map_err(|e| PalwE2eError::Profile(e.to_string()))?;
    if evidence.vectors.is_empty() {
        return Err(PalwE2eError::NoVectors);
    }
    let class_id = evidence.class.class_id(&evidence.artifact_root);
    let reachable = crate::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&program);
    let mut covering = PalwE2eCoveringV1 { malformed_refused: evidence.malformed_inputs_refused > 0, ..Default::default() };
    for vector in &evidence.vectors {
        let leaf = vector.leaf_index;
        let honest = rebind(&vector.honest, &evidence.class, leaf)?;
        let guilty = rebind(&vector.guilty, &evidence.class, leaf)?;
        for r in [&honest, &guilty] {
            if r.binding.artifact_root != evidence.artifact_root || r.binding.job_context.shape_profile_id != class_id {
                return Err(PalwE2eError::VectorIsAboutAnotherGraph {
                    leaf,
                    drilled: class_id,
                    vector: r.binding.job_context.shape_profile_id,
                });
            }
            if r.output_opening.leaf_index != leaf {
                return Err(PalwE2eError::RefutationOpensAnotherLeaf { leaf, opened: r.output_opening.leaf_index });
            }
        }
        let rules = |r: &PalwTirConeRefutationV1| PalwTirCourtRulesV1 {
            max_step_leaf_count: crate::palw_context_ladder::PALW_CONTEXT_LADDER_MAX_STEP_LEAVES,
            // The form the drill's network commits in, read off what the refutation carries.
            prompt_form: if r.prompt_ids_openings.is_empty() { PalwPromptIdsFormV1::Flat } else { PalwPromptIdsFormV1::MerkleV1 },
            limits: PALW_TIR_CERTIFY_LIMITS_V1,
        };
        match check_tir_cone_refutation_v1(&honest, &rules(&honest)) {
            Err(PalwStepRefuteError::NoFaultFound) => {}
            _ => return Err(PalwE2eError::HonestRunConvicted { leaf }),
        }
        if let Err(why) = check_tir_cone_refutation_v1(&guilty, &rules(&guilty)) {
            return Err(PalwE2eError::GuiltyRunAcquitted { leaf, why: format!("{why:?}") });
        }
        if vector.honest_prefix.0 != vector.guilty_prefix.0 {
            return Err(PalwE2eError::BisectionIsNotAPrefixCommitment {
                leaf,
                why: "the two runs disagree BEFORE the planted fault, so the ladder cannot narrow into it",
            });
        }
        if vector.honest_prefix.1 == vector.guilty_prefix.1 {
            return Err(PalwE2eError::BisectionIsNotAPrefixCommitment {
                leaf,
                why: "the two runs agree once the planted fault is included, so the rung is uninformative",
            });
        }
        let at = space.leaf_at(&honest.binding.job_context, leaf).ok_or(PalwE2eError::LeafIsNotACoordinate { leaf })?;
        if let PalwTirLeafKindV1::Commit { block, .. } = at.kind {
            if block == program.schedule.pre {
                covering.pre = true;
            } else if block == program.schedule.post {
                covering.post = true;
            } else {
                covering.attn = true;
            }
        }
        if at.coord.call_index == 0 {
            covering.prefill = true;
        } else {
            covering.decode = true;
        }
        for name in cone_prims(&program, &at.kind) {
            covering.drilled_kernel_ids.insert(crate::palw_tir_admission_v1::palw_tir_prim_kernel_id_v1(name));
        }
        covering.convicted_leaves = covering.convicted_leaves.saturating_add(1);
    }
    let covers = covering.prefill
        && covering.decode
        && covering.convicted_leaves > 0
        && covering.malformed_refused
        && reachable.is_subset(&covering.drilled_kernel_ids);
    if !covers {
        return Err(PalwE2eError::NotCovering { covering });
    }
    Ok(crate::palw_e2e_adjudicability::palw_e2e_certificate_sealed_v1(PalwE2eFamilyV1 {
        family_id: evidence.family_id,
        drilled_class_id: class_id,
        kernel_ids: reachable,
        covering,
    }))
}

/// A drill over the court's tiny IR class, for the tests here and the fold's.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::palw_tir_court_v1::build_tir_cone_refutation_v1;
    use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};

    fn prefix(hashes: &[Hash64]) -> Hash64 {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(b"test/prefix").to_state();
        for h in hashes {
            s.update(h.as_byte_slice());
        }
        Hash64::from_bytes(s.finalize().as_bytes().try_into().unwrap())
    }

    const RULES: PalwTirCourtRulesV1 =
        PalwTirCourtRulesV1 { max_step_leaf_count: 1 << 26, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };

    /// A vector at `leaf`: the honest execution's refutation and one with lane 0 of the leaf moved.
    pub(crate) fn vector(honest: &TinyExecution, leaf: u64) -> PalwTirE2eFaultVectorV1 {
        let forged = tiny_execution(Some((leaf as usize, 0)));
        let mut h = build_tir_cone_refutation_v1(&honest.binding, leaf, honest, &RULES).expect("buildable");
        let mut g = build_tir_cone_refutation_v1(&forged.binding, leaf, &forged, &RULES).expect("buildable");
        palw_tir_strip_program_v1(&mut h);
        palw_tir_strip_program_v1(&mut g);
        let l = leaf as usize;
        PalwTirE2eFaultVectorV1 {
            leaf_index: leaf,
            honest: h,
            guilty: g,
            honest_prefix: (prefix(&honest.hashes[..l]), prefix(&honest.hashes[..=l])),
            guilty_prefix: (prefix(&forged.hashes[..l]), prefix(&forged.hashes[..=l])),
        }
    }

    /// The drill at `leaves` (every leaf when `None`).
    pub(crate) fn evidence(leaves: Option<&[u64]>) -> PalwTirE2eDrillEvidenceV1 {
        let honest = tiny_execution(None);
        let all: Vec<u64> = (0..honest.preimages.len() as u64).collect();
        PalwTirE2eDrillEvidenceV1 {
            family_id: Hash64::from_bytes([0xF1; 64]),
            class: honest.binding.class.clone(),
            artifact_root: honest.binding.artifact_root,
            vectors: leaves.unwrap_or(&all).iter().map(|leaf| vector(&honest, *leaf)).collect(),
            malformed_inputs_refused: 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::evidence;
    use super::*;
    use crate::palw_tir_court_v1::test_support::tiny_execution;

    #[test]
    fn a_drill_over_every_leaf_certifies_the_program_s_primitives() {
        let honest = tiny_execution(None);
        let all: Vec<u64> = (0..honest.preimages.len() as u64).collect();
        let e = evidence(None);
        let certificate = certify_tir_e2e_family_v1(&e).expect("certifies");
        let program = e.class.decode_program().unwrap();
        assert_eq!(certificate.family.kernel_ids, crate::palw_tir_admission_v1::palw_tir_reachable_prims_v1(&program));
        assert_eq!(certificate.family.drilled_class_id, e.class.class_id(&e.artifact_root));
        assert_eq!(certificate.family.covering.convicted_leaves as usize, all.len());
        assert!(certificate.family.covering.prefill && certificate.family.covering.decode);
        assert_eq!(certificate.family_digest, certificate.family.digest());
    }

    #[test]
    fn a_drill_that_proves_less_certifies_nothing() {
        let honest = tiny_execution(None);
        let all: Vec<u64> = (0..honest.preimages.len() as u64).collect();
        let one = &all[..1];
        assert!(matches!(certify_tir_e2e_family_v1(&evidence(Some(&[]))), Err(PalwE2eError::NoVectors)));
        // Swapped sides: the court convicts the "honest" one.
        let mut e = evidence(Some(one));
        let v = &mut e.vectors[0];
        std::mem::swap(&mut v.honest, &mut v.guilty);
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::HonestRunConvicted { .. })));
        // A "guilty" run that is the honest one is acquitted.
        let mut e = evidence(Some(one));
        e.vectors[0].guilty = e.vectors[0].honest.clone();
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::GuiltyRunAcquitted { .. })));
        // A vector that carries the program (it rides once), or another layout.
        let mut e = evidence(Some(one));
        e.vectors[0].honest.binding.class.program = vec![1];
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::VectorIsAboutAnotherGraph { .. })));
        let mut e = evidence(Some(one));
        e.vectors[0].guilty.binding.class.layout.h_tile += 1;
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::VectorIsAboutAnotherGraph { .. })));
        // Another inventory root.
        let mut e = evidence(Some(one));
        e.artifact_root = Hash64::from_bytes([1; 64]);
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::VectorIsAboutAnotherGraph { .. })));
        // A rung that is no prefix commitment.
        let mut e = evidence(Some(one));
        e.vectors[0].guilty_prefix.0 = Hash64::from_bytes([2; 64]);
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::BisectionIsNotAPrefixCommitment { .. })));
        // A vector opening another leaf than it names.
        let mut e = evidence(Some(one));
        e.vectors[0].leaf_index += 1;
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::RefutationOpensAnotherLeaf { .. })));
        // Too few leaves to cover every primitive and both call classes, or no malformed input.
        assert!(matches!(certify_tir_e2e_family_v1(&evidence(Some(one))), Err(PalwE2eError::NotCovering { .. })));
        let mut e = evidence(None);
        e.malformed_inputs_refused = 0;
        assert!(matches!(certify_tir_e2e_family_v1(&e), Err(PalwE2eError::NotCovering { .. })));
    }
}
