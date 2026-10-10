//! **`KernelDescriptorV1`, the class binding, and the binary's descriptor schedule** (`docs/design/palw/versioned-kernels.md` §K.2, §K.5).
//!
//! A descriptor names, by id, every sub-specification a class bound to it is judged by. Its digest is
//! what a class binds; the schedule says whether that digest may be used at a DAA. Meanings are
//! append-only: a descriptor's semantics never change; a new meaning is a new descriptor (and, for a
//! class, a new class id). An unknown digest is never success.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::family::{CheckerIdV1, ConstraintFamilyV1, CourtIdV1};
use crate::hash::{Digest, object_id};

pub const KERNEL_DESCRIPTOR_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/descriptor/v1";
pub const CLASS_BINDING_DOMAIN_V1: &[u8] = b"misaka-palw/kernel/class-binding/v1";

/// One implemented family: its checker and its court. A descriptor holds at most one row per family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FamilySupportV1 {
    pub family: ConstraintFamilyV1,
    pub checker: CheckerIdV1,
    pub court: CourtIdV1,
}

/// The soundness policy: network-wide for every class bound to the descriptor, never plan-selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SoundnessPolicyV1 {
    /// The whole-claim conditional error target, as bits (`ε_check ≤ 2^-target_bits`). RFC-0011
    /// §15.4's proposed target is 128.
    pub target_bits: u16,
    /// Repetitions of every probabilistic check, per relation instance. A plan may not lower it.
    pub repetitions: u8,
    /// The binding term: commitments are BLAKE2b-512, whose collision resistance is taken as 2^-256.
    pub binding_bits: u16,
}

/// Resource ceilings the node enforces whatever the plan says (RFC-0011 §16.2: more producer CPU
/// never removes node DoS bounds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ResourceLimitsV1 {
    pub max_relations: u32,
    pub max_plan_bytes: u64,
    pub max_positions: u32,
    /// The verifier's field/integer operations for a whole claim at the plan's maximum positions.
    pub max_claim_verifier_work: u128,
    /// Bytes of evidence the verifier opens for a whole claim.
    pub max_claim_evidence_bytes: u128,
    /// The worst single fault proof: bytes it opens and the work its terminal court does.
    pub max_court_bytes: u64,
    pub max_court_work: u64,
}

/// The Kernel design §K.2's descriptor. Sub-ids are this crate's spellings (u32), not allocated wire ids.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelDescriptorV1 {
    pub kernel_id: u32,
    pub version: u16,
    /// Digest of the normative semantics text the descriptor implements.
    pub semantics_digest: Digest,
    pub plan_grammar_id: u32,
    /// The TIR primitive set the kernel's semantics are (`PRIM_SET_ID_V1` for PALW-TIR v1).
    pub primitive_set_id: Digest,
    pub constraint_set_id: u32,
    pub arithmetic_id: u32,
    pub memory_model_id: u32,
    pub commitment_suite_id: u32,
    pub checker_suite_id: u32,
    pub challenge_policy_id: u32,
    pub court_suite_id: u32,
    pub resource_schedule_id: u32,
    pub soundness_policy_id: u32,
    pub families: Vec<FamilySupportV1>,
    pub soundness: SoundnessPolicyV1,
    pub limits: ResourceLimitsV1,
}

impl KernelDescriptorV1 {
    pub fn digest(&self) -> Digest {
        object_id(KERNEL_DESCRIPTOR_DOMAIN_V1, self)
    }

    pub fn support(&self, family: ConstraintFamilyV1) -> Option<&FamilySupportV1> {
        self.families.iter().find(|f| f.family == family)
    }

    /// Structural sanity of a descriptor the binary carries: one row per family, a probabilistic
    /// checker only where repetitions and a target exist.
    pub fn well_formed(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for f in &self.families {
            if !seen.insert(f.family) {
                return Err(format!("family {} listed twice", f.family.name()));
            }
        }
        if self.soundness.repetitions == 0 || self.soundness.target_bits == 0 {
            return Err("a descriptor needs at least one repetition and a nonzero target".into());
        }
        if self.families.iter().any(|f| f.family == ConstraintFamilyV1::DenseMatrix && !f.checker.is_probabilistic()) {
            // Not unsound, but not this kernel line's dense relation: an exact MatMul recompute is the replay the route replaces.
            return Err("the dense-matrix family needs a probabilistic checker".into());
        }
        Ok(())
    }

    /// The bits one repetition buys against a false instance, for the weakest probabilistic checker the descriptor lists
    /// (`FIELD_BITS` when it lists none). The whole-claim error is derived from it, never from a plan.
    pub fn per_repetition_bits(&self) -> u32 {
        self.families
            .iter()
            .filter(|f| f.checker.is_probabilistic())
            .map(|f| f.checker.per_repetition_bits())
            .min()
            .unwrap_or(crate::field::FIELD_BITS)
    }
}

/// The digest of the normative text of K2-TIR-v1: the crate's own module docs are the spec of record
/// for this reference kernel, named by this string until an RFC section pins the bytes.
pub const K2_TIR_V1_SEMANTICS: &[u8] = b"misaka-palw-kernel K2-TIR-v1: PALW-TIR v1 semantics (RFC-0002 Part I, PRIM_SET_ID_V1); \
exact recompute for structure, exact-arithmetic, quant-range, nonlinear and selection; Freivalds over GF(2^127-1) with post-commit \
vectors for MatMul up to an i64 accumulator; state continuity by wiring for StateWrite/HistAppend; row/column Merkle node \
commitments (BLAKE2b-512); instance-recompute and matmul-scalar courts, the latter opening one row of X, one column of W and one \
row of Y";

/// **K2-TIR-v1**: the first reference kernel — every PALW-TIR v1 family except media pipelines.
pub fn k2_tir_v1_descriptor() -> KernelDescriptorV1 {
    use CheckerIdV1 as C;
    use ConstraintFamilyV1 as F;
    use CourtIdV1 as K;
    let row = |family, checker, court| FamilySupportV1 { family, checker, court };
    KernelDescriptorV1 {
        kernel_id: 2,
        version: 1,
        semantics_digest: crate::hash::id(KERNEL_DESCRIPTOR_DOMAIN_V1, K2_TIR_V1_SEMANTICS),
        plan_grammar_id: 1,
        primitive_set_id: misaka_palw_tir::prim::PRIM_SET_ID_V1,
        constraint_set_id: 1,
        arithmetic_id: 1,
        memory_model_id: 1,
        commitment_suite_id: 2,
        checker_suite_id: 1,
        challenge_policy_id: 1,
        court_suite_id: 1,
        resource_schedule_id: 1,
        soundness_policy_id: 1,
        families: vec![
            row(F::Structure, C::ExactRecompute, K::InstanceRecompute),
            row(F::ExactArithmetic, C::ExactRecompute, K::InstanceRecompute),
            row(F::DenseMatrix, C::FreivaldsM127, K::MatMulScalar),
            row(F::QuantRange, C::ExactRecompute, K::InstanceRecompute),
            row(F::Nonlinear, C::ExactRecompute, K::InstanceRecompute),
            row(F::Selection, C::ExactRecompute, K::InstanceRecompute),
            row(F::RecurrentState, C::StateContinuity, K::InstanceRecompute),
        ],
        soundness: SoundnessPolicyV1 { target_bits: 128, repetitions: 2, binding_bits: 256 },
        limits: ResourceLimitsV1 {
            max_relations: 1 << 16,
            max_plan_bytes: 1 << 24,
            max_positions: 1 << 21,
            max_claim_verifier_work: 1 << 64,
            max_claim_evidence_bytes: 1 << 50,
            max_court_bytes: 1 << 32,
            max_court_work: 1 << 36,
        },
    }
}

pub const K2_TIR_V2_SEMANTICS: &[u8] = b"misaka-palw-kernel K2-TIR-v2: K2-TIR-v1 semantics, except the dense-matrix family: \
Freivalds modulo the fewest of 2^127-1, 2^107-1, 2^89-1 whose product exceeds the relation's integer error span (CRT), each modulus \
with its own post-commit vectors, accepting MatMul up to an i128 accumulator; the per-repetition bound is that of 2^89-1";

/// **K2-TIR-v2**: K2-TIR-v1 with the multi-modulus dense-matrix relation, so an `i128` accumulator is expressible. A new
/// descriptor (new digest, new class ids): classes bound to K2-TIR-v1 keep its rules; nothing is reinterpreted.
pub fn k2_tir_v2_descriptor() -> KernelDescriptorV1 {
    let mut d = k2_tir_v1_descriptor();
    d.version = 2;
    d.semantics_digest = crate::hash::id(KERNEL_DESCRIPTOR_DOMAIN_V1, K2_TIR_V2_SEMANTICS);
    d.checker_suite_id = 2;
    for f in &mut d.families {
        if f.family == ConstraintFamilyV1::DenseMatrix {
            f.checker = CheckerIdV1::FreivaldsCrtV2;
        }
    }
    d
}

pub const K2_TIR_V3_SEMANTICS: &[u8] = b"misaka-palw-kernel K2-TIR-v3: K2-TIR-v2 semantics, plus the media-pipeline family: \
RFC-0003 pipelines of TIR v2 programs (spec 04b section 15), each stage checked over its version-1 view with its input tensors \
committed per position and its post writes wired to the next position; every stage input recomputed exactly from its binding \
(job scalars, token templates and counts, canonical u8 RGB images, earlier stages' committed rows and final values with zero pad, \
RFC-0003 R) inside its declared interval; edge court recomputes one input at one position";

/// **K2-TIR-v3**: K2-TIR-v2 with the media-pipeline family (Kernel design §K.3's "media and pipelines" row), so RFC-0003 pipeline
/// classes (text encoders, denoisers, decoders, vision encoders, decode stages, evaluation pipelines) are expressible.
pub fn k2_tir_v3_descriptor() -> KernelDescriptorV1 {
    let mut d = k2_tir_v2_descriptor();
    d.version = 3;
    d.semantics_digest = crate::hash::id(KERNEL_DESCRIPTOR_DOMAIN_V1, K2_TIR_V3_SEMANTICS);
    d.constraint_set_id = 3;
    d.court_suite_id = 3;
    d.families.push(FamilySupportV1 {
        family: ConstraintFamilyV1::MediaPipeline,
        checker: CheckerIdV1::EdgeRecompute,
        court: CourtIdV1::EdgeRecompute,
    });
    d
}

pub const K2_TIR_V4_SEMANTICS: &[u8] = b"misaka-palw-kernel K2-TIR-v4 (real scale): K2-TIR-v2 semantics for every TIR v1 family; \
tiled dual-root tensor commitments (v3: leaves of at most 4096 elements of one row or column); every committed value of a position \
(derived windows included) under a position root, position roots under segment roots of 1024 positions, segment roots under a claim \
root carried on chain; element courts (one output element, one leaf per input dependency line; a window judged against the previous \
position's window and the new row; greedy decode by a rival element); per-position demands served in parts; prompts posted in \
tiles of 4096 ids under a prompt root; OptimisticPublicVerification only";

/// **K2-TIR-v4** (`docs/design/palw/k2-real-scale.md`): K2-TIR-v2's families and checkers under the real-scale commitment and court
/// suites — segmented claims ([`crate::seg`]), element courts ([`crate::element`]), per-position DA ([`crate::seg_da`]). A new descriptor:
/// classes bound to v1–v3 keep their rules. The whole-claim evidence bound is a parse bound here (2^60): the gate bounds ONE prosecution
/// instead, and reports the whole-claim material as the producer's DA obligation.
pub fn k2_tir_v4_descriptor() -> KernelDescriptorV1 {
    let mut d = k2_tir_v2_descriptor();
    d.version = 4;
    d.semantics_digest = crate::hash::id(KERNEL_DESCRIPTOR_DOMAIN_V1, K2_TIR_V4_SEMANTICS);
    d.commitment_suite_id = 3;
    d.court_suite_id = 4;
    for f in &mut d.families {
        f.court = CourtIdV1::ElementRecompute;
    }
    d.limits.max_claim_evidence_bytes = 1 << 60;
    d.limits.max_court_bytes = 1 << 24;
    d
}

/// Whether a descriptor's claims are segmented (K2-TIR-v4's commitment suite; K2-TIR-v5 too).
pub fn is_segmented_v1(d: &KernelDescriptorV1) -> bool {
    d.commitment_suite_id == 3
}

pub const K2_TIR_V5_SEMANTICS: &[u8] = b"misaka-palw-kernel K2-TIR-v5 (encoders and heads): K2-TIR-v4 for a program of ONE position whose \
last two params, input.ids (idx [L], L <= 4096) and input.count (idx []), are the job's input and not the artifact: the job's prompt ids \
padded to L with id 0, and the prompt's length; no node reads the per-position token; a tiled job of no generated id and a claim of no \
delivered id; the result is the program's output node at position 0; OptimisticPublicVerification only";

/// The memory model of K2-TIR-v5: params past `first_input` are the job's input (`crate::seg_encoder`).
pub const MEMORY_MODEL_JOB_INPUTS_V1: u32 = 2;

/// **K2-TIR-v5** (`docs/design/palw/k2-real-scale.md` §12): K2-TIR-v4 (its families, commitments, element courts, DA and gate) for a
/// bidirectional encoder or a head over one: ONE position over a padded token axis whose ids and count are the job's input
/// ([`crate::seg_encoder`]). A new descriptor: v4 classes keep their rules.
pub fn k2_tir_v5_descriptor() -> KernelDescriptorV1 {
    let mut d = k2_tir_v4_descriptor();
    d.version = 5;
    d.semantics_digest = crate::hash::id(KERNEL_DESCRIPTOR_DOMAIN_V1, K2_TIR_V5_SEMANTICS);
    d.memory_model_id = MEMORY_MODEL_JOB_INPUTS_V1;
    d
}

/// Whether a descriptor's classes are encoders with job-bound inputs (K2-TIR-v5).
pub fn is_encoder_v1(d: &KernelDescriptorV1) -> bool {
    is_segmented_v1(d) && d.memory_model_id == MEMORY_MODEL_JOB_INPUTS_V1
}

/// Where a descriptor stands in the release sequence (ADR-0172 §3: proposal → reference + independent
/// implementation → vectors/review → shadow → LOCKED_IN → ACTIVE). These are release states, not a ballot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum KernelStatusV1 {
    Proposed,
    /// Implemented in this binary, not scheduled.
    Implemented,
    /// Scheduled: active from `activation_daa`.
    LockedIn {
        activation_daa: u64,
    },
    Active {
        since_daa: u64,
    },
    /// No new registration or claim from `stop_new_daa`; bound claims keep their recorded rules.
    Deprecated {
        since_daa: u64,
        stop_new_daa: u64,
    },
}

/// What a status means at one DAA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelStandingV1 {
    Active,
    NotActive(KernelStatusV1),
    Unknown,
}

/// The binary's schedule: which descriptor digests it implements and their status.
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelScheduleV1 {
    pub entries: Vec<(Digest, KernelStatusV1)>,
}

impl KernelScheduleV1 {
    pub fn with(mut self, digest: Digest, status: KernelStatusV1) -> Self {
        self.entries.retain(|(d, _)| *d != digest);
        self.entries.push((digest, status));
        self
    }

    pub fn standing_at(&self, digest: &Digest, daa: u64) -> KernelStandingV1 {
        let Some((_, status)) = self.entries.iter().find(|(d, _)| d == digest) else { return KernelStandingV1::Unknown };
        match *status {
            KernelStatusV1::Active { since_daa } if daa >= since_daa => KernelStandingV1::Active,
            KernelStatusV1::LockedIn { activation_daa } if daa >= activation_daa => KernelStandingV1::Active,
            KernelStatusV1::Deprecated { since_daa, stop_new_daa } if daa >= since_daa && daa < stop_new_daa => {
                KernelStandingV1::Active
            }
            other => KernelStandingV1::NotActive(other),
        }
    }
}

/// **The shipped schedule: K2-TIR-v1, v2, v3 and v4 are implemented and not active** — no network arms any.
pub fn builtin_schedule_v1() -> KernelScheduleV1 {
    KernelScheduleV1::default()
        .with(k2_tir_v1_descriptor().digest(), KernelStatusV1::Implemented)
        .with(k2_tir_v2_descriptor().digest(), KernelStatusV1::Implemented)
        .with(k2_tir_v3_descriptor().digest(), KernelStatusV1::Implemented)
        .with(k2_tir_v4_descriptor().digest(), KernelStatusV1::Implemented)
}

/// What a bound class commits beside its program/artifact (RFC-0011 §16.3(4)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ContextPolicyV1 {
    /// The positions a claim may span (the plan is checked at this worst case).
    pub max_positions: u32,
}

/// The Kernel design §K.2's `ModelKernelBindingV1`: the whole binding a new-format class id commits to.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ModelKernelBindingV1 {
    pub descriptor_digest: Digest,
    pub plan_root: Digest,
    pub program_root: Digest,
    pub artifact_root: Digest,
    pub tokenizer_or_input_schema_root: Digest,
    pub task_output_schema: Digest,
    pub context_and_state_policy: ContextPolicyV1,
}

impl ModelKernelBindingV1 {
    /// The new-format class id, under its own domain: no legacy class id is ever reinterpreted.
    pub fn class_binding_id(&self) -> Digest {
        object_id(CLASS_BINDING_DOMAIN_V1, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_kernel_is_implemented_not_active() {
        let d = k2_tir_v1_descriptor();
        d.well_formed().unwrap();
        let s = builtin_schedule_v1();
        assert_eq!(s.standing_at(&d.digest(), u64::MAX), KernelStandingV1::NotActive(KernelStatusV1::Implemented));
        assert_eq!(s.standing_at(&[0; 64], 0), KernelStandingV1::Unknown, "an unknown digest is never active");
        assert!(d.support(ConstraintFamilyV1::MediaPipeline).is_none());
        let v2 = k2_tir_v2_descriptor();
        v2.well_formed().unwrap();
        assert_ne!(v2.digest(), d.digest(), "a new dense relation is a new descriptor");
        assert!(matches!(s.standing_at(&v2.digest(), u64::MAX), KernelStandingV1::NotActive(KernelStatusV1::Implemented)));
        assert_eq!(d.per_repetition_bits(), 126);
        assert_eq!(v2.per_repetition_bits(), 88, "the weakest modulus prices every repetition");
    }

    #[test]
    fn locked_in_activates_at_its_daa_and_deprecation_stops_new_work() {
        let d = k2_tir_v1_descriptor().digest();
        let s = KernelScheduleV1::default().with(d, KernelStatusV1::LockedIn { activation_daa: 100 });
        assert!(matches!(s.standing_at(&d, 99), KernelStandingV1::NotActive(_)));
        assert_eq!(s.standing_at(&d, 100), KernelStandingV1::Active);
        let s = s.with(d, KernelStatusV1::Deprecated { since_daa: 0, stop_new_daa: 500 });
        assert_eq!(s.standing_at(&d, 499), KernelStandingV1::Active);
        assert!(matches!(s.standing_at(&d, 500), KernelStandingV1::NotActive(_)));
    }

    #[test]
    fn any_change_to_a_descriptor_or_binding_is_a_new_identity() {
        let a = k2_tir_v1_descriptor();
        let mut b = a.clone();
        b.soundness.repetitions = 1;
        assert_ne!(a.digest(), b.digest(), "a weaker suite is another descriptor, never the same one");
        let bind = ModelKernelBindingV1 {
            descriptor_digest: a.digest(),
            plan_root: [1; 64],
            program_root: [2; 64],
            artifact_root: [3; 64],
            tokenizer_or_input_schema_root: [4; 64],
            task_output_schema: [5; 64],
            context_and_state_policy: ContextPolicyV1 { max_positions: 8 },
        };
        let mut other = bind.clone();
        other.context_and_state_policy.max_positions = 16;
        assert_ne!(bind.class_binding_id(), other.class_binding_id(), "a different context is not an exact duplicate");
    }
}
