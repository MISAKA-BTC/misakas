//! **Binding a pack into a `ConformanceCommitmentV1`** (RFC-0013 §9, RFC-0007 Part VI).
//!
//! ```text
//! STATIC SEMANTIC ADMISSION → COMMIT FIRST → FUTURE PALW WORK BEACON → INDEPENDENT PROBABILISTIC CHECK → mismatch → EXACT PUBLIC COURT
//! ```
//!
//! This module is the second step. It takes a pack, the declared class file it describes and the parameters a commitment fixes
//! BEFORE any randomness — the challenge policy, the test scope, the chain it is for — and derives every root from the artifact
//! itself (never from a cached manifest line): the artifact root, program root, tokenizer, exact layout, the VerificationPlan root
//! of the kernel that would run it, the implementation set and the calibration identity. Commitment bytes are the contract's
//! [`ConformanceCommitmentV1::statement_root`]; JSON here is a tool record that is recomputed, never trusted.
//!
//! Static admission comes first and is never replaced by a beacon: a program no kernel can express is refused here with the
//! onboarding failure code (`FRONTEND_REQUIRED` / `KERNEL_EXTENSION_REQUIRED`), and no commitment exists to be challenged. The
//! kernel is judged **hypothetically armed** — the shipped schedule has no Active kernel — and the record says so.
//!
//! The test policy is [`misaka_palw_challenge::reference_policy_v1`] with the caller's numbers: **UNAPPROVED**, not an approved
//! (checker suite, challenge policy, soundness policy) tuple. Nothing here selects a value a release has not reviewed.

use super::manifest::{PACK_FILE, RuntimePackV1, blake2b256_hex};
use crate::tir_manifest::PalwTirManifestV1;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::conformance::ConformanceCommitmentV1;
use misaka_palw_challenge::hash::{self, Digest};
use misaka_palw_challenge::lifecycle::OnboardingFailureV1;
use misaka_palw_challenge::{PostCommitChallengePolicyV1, RootV1, SubjectKindV1};
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/impl_revisions.rs"));

/// The check protocol this tool runs on a committed pack: `pack-sampled-differential/v1`. A different protocol is a different scope root.
pub const CHECK_PROTOCOL_V1: &str = "pack-sampled-differential/v1";

pub const DOMAIN_SCOPE: &[u8] = b"misaka.palw.runtime-pack.conformance-scope.v1";
pub const DOMAIN_IMPL_SET: &[u8] = b"misaka.palw.runtime-pack.impl-set.v1";
pub const DOMAIN_CALIBRATION: &[u8] = b"misaka.palw.runtime-pack.calibration-id.v1";
pub const DOMAIN_BINDING: &[u8] = b"misaka.palw.runtime-pack.input-state-binding.v1";
pub const DOMAIN_RESOURCE: &[u8] = b"misaka.palw.runtime-pack.resource-profile.v1";
pub const DOMAIN_SOURCE: &[u8] = b"misaka.palw.runtime-pack.source-provenance.v1";

/// A refusal with a stable machine code (an [`OnboardingFailureV1`] code where one applies) and the reason in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: &'static str,
    pub detail: String,
}

impl Refusal {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for Refusal {}

pub fn unhex64(s: &str) -> Result<Digest, String> {
    if s.len() != 128 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("`{s}` is not 128 hex characters (64 bytes)"));
    }
    let mut out = [0u8; 64];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn hex(d: &[u8]) -> String {
    hash::hex(d)
}

/// A typed record's root under a tool domain (`H(domain; borsh(record))`, the contract's hash suite).
pub fn tool_root<T: BorshSerialize>(domain: &[u8], record: &T) -> Digest {
    hash::object_id(domain, record)
}

// ---------------------------------------------------------------------------------------------------------------------------
// The test scope
// ---------------------------------------------------------------------------------------------------------------------------

/// **What the committed check is** — fixed before any randomness (it is the commitment's `test_scope_root`).
///
/// A repetition (the policy's `repetition_count` of them) draws, from its own labelled streams of the one challenge seed,
/// `vectors_per_repetition` prompts (full forward passes on every required implementation) and `leaves_per_repetition` artifact
/// leaves (an authenticated opening against the artifact root, then the integer values every required implementation decodes from
/// those bytes). Nothing else is checked: this is a SAMPLED differential check, not full-scope fidelity, not semantic admission.
///
/// The fault model is part of the scope: `*_fault_ppm` is the density of faulty draws the bound speaks about (a fault that shows
/// on at least that fraction of the family's draws), under independent uniform draws. See [`ConformanceScopeV1::derived_epsilon_bits`].
/// **The fault model a soundness policy approves**: the densest fault (ppm of vector / leaf draws on which a faulty implementation
/// shows) a scope may assume under it. The candidate never chooses it (C4 GAP-C4-D): a denser assumed fault buys unearned bits.
/// Only the reference policy's unreviewed test soundness id has one here; every reviewed soundness policy is an external gate, so no
/// production policy can pass conformance until its fault model is approved and listed.
pub fn approved_fault_model_v1(policy: &PostCommitChallengePolicyV1) -> Option<(u32, u32)> {
    (policy.soundness_policy_id == hash::named_id("soundness/unreviewed-test-only/v1")).then_some((1_000_000, 1_000_000))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceScopeV1 {
    pub version: u16,
    pub protocol: String,
    pub vectors_per_repetition: u32,
    /// A prompt has `1 ..= max_prompt_len` tokens (the length is drawn too).
    pub max_prompt_len: u32,
    /// Greedily decoded tokens after the prompt.
    pub decode_tokens: u32,
    pub leaves_per_repetition: u32,
    pub vector_fault_ppm: u32,
    pub leaf_fault_ppm: u32,
    /// The independent second implementation must agree (a pack without it cannot pass).
    pub require_independent: bool,
    /// The typed backend must agree.
    pub require_backend: bool,
}

impl ConformanceScopeV1 {
    pub fn new(vectors: u32, max_prompt_len: u32, decode_tokens: u32, leaves: u32) -> Self {
        Self {
            version: 1,
            protocol: CHECK_PROTOCOL_V1.into(),
            vectors_per_repetition: vectors,
            max_prompt_len,
            decode_tokens,
            leaves_per_repetition: leaves,
            vector_fault_ppm: 500_000,
            leaf_fault_ppm: 62_500,
            require_independent: true,
            require_backend: true,
        }
    }

    pub fn root(&self) -> Digest {
        tool_root(DOMAIN_SCOPE, self)
    }

    pub fn validate(&self, policy: &PostCommitChallengePolicyV1) -> Result<(), String> {
        if self.version != 1 || self.protocol != CHECK_PROTOCOL_V1 {
            return Err(format!("unknown scope version/protocol ({} / {})", self.version, self.protocol));
        }
        if self.vectors_per_repetition == 0 && self.leaves_per_repetition == 0 {
            return Err("a scope that checks nothing".into());
        }
        if self.vectors_per_repetition > 0 && self.max_prompt_len == 0 {
            return Err("vectors need a prompt length of at least 1".into());
        }
        // The implementation set and the fault model are the protocol's, not the candidate's (C4 F-C4-11, GAP-C4-D): a candidate
        // may not drop the independent or backend implementation, nor assume a denser (more detectable) fault than scope v1 fixes —
        // either would let a reference-only or tiny scope derive a pass with any bits it likes.
        if !self.require_independent || !self.require_backend {
            return Err(
                "scope v1 requires the independent and the typed backend implementation (the candidate cannot waive them)".into()
            );
        }
        let Some((max_vector, max_leaf)) = approved_fault_model_v1(policy) else {
            return Err("the challenge policy's soundness policy approves no fault model (an external review gate)".into());
        };
        for (what, ppm, max) in
            [("vector_fault_ppm", self.vector_fault_ppm, max_vector), ("leaf_fault_ppm", self.leaf_fault_ppm, max_leaf)]
        {
            if ppm == 0 || ppm > max {
                return Err(format!(
                    "{what} must be in 1 ..= {max} (the soundness policy's fault model; a denser fault buys unearned bits)"
                ));
            }
        }
        Ok(())
    }

    /// Checks one repetition makes.
    pub fn checks_per_repetition(&self) -> u64 {
        self.vectors_per_repetition as u64 + self.leaves_per_repetition as u64
    }

    /// **`-log2 ε` the committed scope derives, as a lower bound** — an integer function of the scope and the policy's repetition
    /// count, so every machine gets the same number.
    ///
    /// Model: a faulty implementation shows on at least `f` of a family's draws; draws are independent and uniform; the check
    /// misses it only if every one of the `n` draws is clean: `ε ≤ (1 − f)^n ≤ e^(−f·n)`, so `−log2 ε ≥ f·n·log2 e`, and
    /// `log2 e > 1.4426`. The result is the smaller of the two families that are present. It is a CONDITIONAL bound under this
    /// fault model — not a theorem about the whole model, not a Kernel soundness claim, and it speaks of no fault outside the
    /// two families (a fault that lives only in unsampled leaves or unsampled prompts is exactly what a sample can miss).
    pub fn derived_epsilon_bits(&self, repetitions: u32) -> u16 {
        let bits = |n: u128, ppm: u32| -> u128 { n * ppm as u128 * 14_426 / 10_000_000_000 };
        let mut best: Option<u128> = None;
        if self.vectors_per_repetition > 0 {
            let b = bits(repetitions as u128 * self.vectors_per_repetition as u128, self.vector_fault_ppm);
            best = Some(best.map_or(b, |x| x.min(b)));
        }
        if self.leaves_per_repetition > 0 {
            let b = bits(repetitions as u128 * self.leaves_per_repetition as u128, self.leaf_fault_ppm);
            best = Some(best.map_or(b, |x| x.min(b)));
        }
        best.unwrap_or(0).min(u16::MAX as u128) as u16
    }

    /// The identity of the scope AND its fault model (what the evidence's `scope_and_fault_model_id` names).
    pub fn scope_and_fault_model_id(&self, repetitions: u32) -> Digest {
        tool_root(
            b"misaka.palw.runtime-pack.scope-fault-model.v1",
            &(self.clone(), repetitions, self.derived_epsilon_bits(repetitions)),
        )
    }

    /// The scope in words, for records: never claims more than it is.
    pub fn statement(&self, repetitions: u32) -> String {
        format!(
            "SAMPLED differential check ({}): {repetitions} repetition(s) x [{} prompt(s) of 1..={} tokens + {} decoded, run on reference{}{}; {} artifact leaf(s) opened against the artifact root and decoded by each]; \
             fault model: a fault visible on >= {} ppm of vector draws and >= {} ppm of leaf draws, independent uniform draws; \
             derived -log2(eps) >= {} (conditional bound, not whole-model fidelity, not semantic admission, not full-scope)",
            self.protocol,
            self.vectors_per_repetition,
            self.max_prompt_len,
            self.decode_tokens,
            if self.require_independent { " + independent" } else { "" },
            if self.require_backend { " + typed backend" } else { "" },
            self.leaves_per_repetition,
            self.vector_fault_ppm,
            self.leaf_fault_ppm,
            self.derived_epsilon_bits(repetitions),
        )
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// The implementation set, calibration identity and the other tool-level roots
// ---------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ImplEntryV1 {
    pub role: String,
    pub crate_name: String,
    pub crate_version: String,
    /// BLAKE2b-512 hex over the crate's source, taken when THIS binary was built (`build.rs`).
    pub source_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ImplementationSetV1 {
    pub entries: Vec<ImplEntryV1>,
    /// The pack's math policy and the lowering this build performs (the converter's contribution to the bytes).
    pub math_mode: String,
    pub lowering: String,
    pub prim_set_id: String,
    pub check_protocol: String,
}

impl ImplementationSetV1 {
    /// The implementations THIS binary is, matched to the pack's own record of the math and the primitive set.
    pub fn of_this_build(pack: &RuntimePackV1) -> Self {
        let entries = IMPL_REVISIONS
            .iter()
            .map(|(role, name, digest)| ImplEntryV1 {
                role: role.to_string(),
                crate_name: name.to_string(),
                crate_version: env!("CARGO_PKG_VERSION").to_string(),
                source_digest: digest.to_string(),
            })
            .collect();
        Self {
            entries,
            math_mode: pack.converter.math.mode.clone(),
            lowering: super::manifest::LOWERING_VERSION_V1.into(),
            prim_set_id: pack.executor.prim_set_id.clone(),
            check_protocol: CHECK_PROTOCOL_V1.into(),
        }
    }

    pub fn root(&self) -> Digest {
        tool_root(DOMAIN_IMPL_SET, self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct CalibrationIdentityV1 {
    stats_digest: String,
    sites: u64,
    source: String,
    headroom_bits: [u64; 3],
    max_window: Option<u32>,
    context: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SourceProvenanceV1 {
    format: String,
    config_digest: String,
    /// `(path, bytes, sha256)` in the pack's order.
    files: Vec<(String, u64, String)>,
    spec_digest: String,
    adapter: (String, Option<String>, Option<String>),
    builtin_pack_hash: String,
    quant_descriptors: Vec<(String, String)>,
}

/// How a prompt is drawn and what state a check starts from: the binding of the challenge-selected inputs.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct InputStateBindingV1 {
    input_rule: String,
    token_bound: u32,
    initial_state: String,
    decode: String,
    source_provenance_root: Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct ResourceProfileV1 {
    mode: String,
    leaf_piece_bytes: u64,
    read_ahead_bytes: u64,
    reference_params: String,
    independent_params: String,
    backend_params: String,
}

// ---------------------------------------------------------------------------------------------------------------------------
// The commitment parameters
// ---------------------------------------------------------------------------------------------------------------------------

/// **Everything a commitment is derived from besides the pack and the artifact.** Persisted (Borsh) beside the commitment so a later
/// process re-derives the commitment from the pack and compares: anything that moved is a stale commitment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CommitParamsV1 {
    pub version: u16,
    /// The declared class of the pack the commitment is for (`testnet-12`), optionally disambiguated by a class-id prefix.
    pub network: String,
    pub class_id_prefix: Option<String>,
    pub chain_genesis: Digest,
    pub ruleset_id: Digest,
    pub policy: PostCommitChallengePolicyV1,
    pub scope: ConformanceScopeV1,
    /// The candidate identity; the class id by default.
    pub candidate_id: Option<Digest>,
    /// The positions the VerificationPlan is checked at (the class's context, clamped to the program's history bound, by default).
    pub plan_positions: Option<u32>,
    /// Free text recorded with the commitment, e.g. `UNAPPROVED TEST POLICY`.
    pub policy_label: String,
}

impl CommitParamsV1 {
    pub fn new(
        network: impl Into<String>,
        chain_genesis: Digest,
        ruleset_id: Digest,
        policy: PostCommitChallengePolicyV1,
        scope: ConformanceScopeV1,
    ) -> Self {
        Self {
            version: 1,
            network: network.into(),
            class_id_prefix: None,
            chain_genesis,
            ruleset_id,
            policy,
            scope,
            candidate_id: None,
            plan_positions: None,
            policy_label: "UNAPPROVED TEST POLICY (reference_policy_v1 numbers; not an approved checker-suite/policy/soundness tuple)"
                .into(),
        }
    }
}

/// What static admission found, as the record states it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticAdmissionRec {
    pub kernel: String,
    pub descriptor: Digest,
    pub plan_root: Digest,
    pub error_bits: u16,
    pub positions: u32,
    /// The outcome under the schedule this binary ships (no kernel is Active there).
    pub shipped_outcome: String,
    pub hypothetically_armed: bool,
}

/// A bound commitment and everything the human record shows about it.
#[derive(Clone, Debug)]
pub struct BoundCommitment {
    pub commitment: ConformanceCommitmentV1,
    pub params: CommitParamsV1,
    pub pack_digest: String,
    pub admission: StaticAdmissionRec,
    pub class_id: Digest,
    pub artifact_bytes: u64,
    pub leaf_count: u32,
    pub implementation_set: ImplementationSetV1,
}

fn failure_code(o: &misaka_palw_kernel::outcome::RegistrationOutcomeV1) -> &'static str {
    use misaka_palw_kernel::outcome::RegistrationOutcomeV1 as O;
    match o {
        O::FrontendRequired { .. } | O::PlanForged { .. } => OnboardingFailureV1::FrontendRequired.code(),
        O::KernelExtensionRequired { .. } | O::KernelNotActive { .. } => OnboardingFailureV1::KernelExtensionRequired.code(),
        O::BoundsExceeded { .. } | O::IncompleteCoverage { .. } | O::CapacityPending { .. } => {
            OnboardingFailureV1::ResourceRefused.code()
        }
        O::ExternalBlocker { .. } => "STATIC_ADMISSION_REFUSED",
        O::EligibleAt { .. } => "STATIC_ADMISSION_REFUSED",
    }
}

/// Static semantic admission of the program under the reference kernels, hypothetically armed (the first that accepts it).
pub fn static_admission(
    program: &misaka_palw_tir::TirProgramV1,
    program_root: Digest,
    positions: u32,
) -> Result<StaticAdmissionRec, Refusal> {
    use misaka_palw_kernel::check::registration_outcome_v1;
    use misaka_palw_kernel::descriptor::{
        KernelScheduleV1, KernelStatusV1, builtin_schedule_v1, k2_tir_v1_descriptor, k2_tir_v2_descriptor,
    };
    use misaka_palw_kernel::outcome::RegistrationOutcomeV1 as O;
    let kernels = [("K2-TIR-v1", k2_tir_v1_descriptor()), ("K2-TIR-v2", k2_tir_v2_descriptor())];
    let judge = |schedule: &KernelScheduleV1, d: &misaka_palw_kernel::descriptor::KernelDescriptorV1| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registration_outcome_v1(schedule, d, program, program_root, positions, 0)
        }))
        .unwrap_or_else(|_| O::FrontendRequired { reason: "the kernel check panicked".into() })
    };
    let mut last: Option<O> = None;
    for (name, d) in &kernels {
        let armed = judge(&KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 }), d);
        match armed {
            O::EligibleAt { descriptor, plan_root, error_bits, .. } => {
                let shipped = judge(&builtin_schedule_v1(), d);
                return Ok(StaticAdmissionRec {
                    kernel: (*name).into(),
                    descriptor,
                    plan_root,
                    error_bits,
                    positions,
                    shipped_outcome: shipped.code().into(),
                    hypothetically_armed: true,
                });
            }
            other => last = Some(other),
        }
    }
    let o = last.expect("two kernels were judged");
    Err(Refusal::new(failure_code(&o), format!("static semantic admission refused the program under every reference kernel: {o}")))
}

fn sidecars_ok(dir: &Path, pack: &RuntimePackV1) -> Result<(), Refusal> {
    for f in &pack.files {
        let b = std::fs::read(dir.join(&f.path)).map_err(|e| Refusal::new("PACK_MISMATCH", format!("sidecar {}: {e}", f.path)))?;
        if b.len() as u64 != f.bytes || blake2b256_hex(&b) != f.blake2b256 {
            return Err(Refusal::new("PACK_MISMATCH", format!("sidecar {} differs from its pinned hash", f.path)));
        }
    }
    Ok(())
}

/// **Bind a commitment.** Reads the pack at `pack_dir` and the declared-class file `artifact`, re-derives every root from the
/// artifact, judges static admission, and returns the commitment (provenance fields empty: the chain observes those).
pub fn bind_commitment(
    pack_dir: &Path,
    artifact: &Path,
    params: &CommitParamsV1,
    log: &dyn Fn(String),
) -> Result<BoundCommitment, Refusal> {
    params.policy.validate().map_err(|e| Refusal::new("POLICY_INVALID", e.to_string()))?;
    params.scope.validate(&params.policy).map_err(|e| Refusal::new("SCOPE_INVALID", e))?;
    let text = std::fs::read_to_string(pack_dir.join(PACK_FILE))
        .map_err(|e| Refusal::new("PACK_MISMATCH", format!("{}: {e}", pack_dir.join(PACK_FILE).display())))?;
    let pack = RuntimePackV1::parse(&text).map_err(|e| Refusal::new("PACK_MISMATCH", e))?;
    sidecars_ok(pack_dir, &pack)?;

    log("deriving the artifact's roots (streamed: one pass for the inventory root, one for the file digest)".into());
    let m = PalwTirManifestV1::derive_streamed(artifact).map_err(|e| Refusal::new("PACK_MISMATCH", e))?;
    let artifact_root: Digest = *m.inventory_root.as_byte_slice();
    let program_root: Digest = *m.graph_ir_root.as_byte_slice();
    if hex(&artifact_root) != pack.result.inventory_root {
        return Err(Refusal::new(
            "PACK_MISMATCH",
            format!(
                "the artifact's inventory root is {}, the pack says {}",
                &hex(&artifact_root)[..16],
                &pack.result.inventory_root[..16]
            ),
        ));
    }
    if hex(&m.tokenizer_id) != pack.result.tokenizer_id {
        return Err(Refusal::new("PACK_MISMATCH", "the artifact's tokenizer id is not the pack's"));
    }

    // The exact layout: pinned in the pack AND carried by this class file, and the two agree.
    let mut cands: Vec<_> = pack.declared.iter().filter(|d| d.network == params.network).collect();
    if let Some(p) = &params.class_id_prefix {
        cands.retain(|d| d.class_id.starts_with(p.as_str()));
    }
    let d = match cands.as_slice() {
        [] => {
            return Err(Refusal::new(
                OnboardingFailureV1::LayoutRequired.code(),
                format!(
                    "the pack declares no class for `{}` (bind the declared artifact into a pack first: `pack bind-class`)",
                    params.network
                ),
            ));
        }
        [d] => *d,
        _ => {
            return Err(Refusal::new(
                "AMBIGUOUS_CLASS",
                format!("the pack declares {} classes for `{}`; pass --class-id <prefix>", cands.len(), params.network),
            ));
        }
    };
    if d.exact_layout.is_none() {
        return Err(Refusal::new(
            OnboardingFailureV1::LayoutRequired.code(),
            "the declared class is a legacy record without its exact layout; bind the declared artifact into a new pack",
        ));
    }
    if hex(&m.artifact_digest) != d.file_digest {
        return Err(Refusal::new("PACK_MISMATCH", "this file is not the declared class file the pack pins (file digest differs)"));
    }
    let (Some(class_id), Some(layout_digest)) = (m.class_id, m.layout_digest) else {
        return Err(Refusal::new(OnboardingFailureV1::LayoutRequired.code(), "the artifact carries no declared layout"));
    };
    if class_id.to_string() != d.class_id || layout_digest.to_string() != d.layout_digest {
        return Err(Refusal::new("PACK_MISMATCH", "the artifact's class id / layout digest is not the pack's declared one"));
    }
    let container =
        misaka_palw_tir_artifact::PalwTirContainerV1::open(artifact).map_err(|e| Refusal::new("PACK_MISMATCH", e.to_string()))?;
    let rebuilt = d
        .class_from_program(&container.program, kaspa_hashes::Hash64::from_bytes(m.tokenizer_id))
        .map_err(|e| Refusal::new("PACK_MISMATCH", e))?;
    if rebuilt.class_id(&m.inventory_root) != class_id {
        return Err(Refusal::new("PACK_MISMATCH", "the pack's exact layout does not reproduce the declared class id"));
    }
    let class_id_bytes: Digest = *class_id.as_byte_slice();
    let layout_root: Digest = *layout_digest.as_byte_slice();

    // Scope against the artifact: it must be drawable, and a prompt plus its decode must fit the class.
    let program = &container.program;
    if params.scope.leaves_per_repetition > m.leaf_count {
        return Err(Refusal::new(
            "SCOPE_INVALID",
            format!("the scope draws {} leaves of an artifact of {}", params.scope.leaves_per_repetition, m.leaf_count),
        ));
    }
    let positions = params.plan_positions.unwrap_or_else(|| d.max_context.min(program.history_bound).max(1));
    if params.scope.vectors_per_repetition > 0 {
        let longest = params.scope.max_prompt_len as u64 + params.scope.decode_tokens as u64;
        if longest > positions as u64 {
            return Err(Refusal::new("SCOPE_INVALID", format!("a vector may run {longest} positions, the class checks {positions}")));
        }
        if program.token_bound == 0 {
            return Err(Refusal::new("SCOPE_INVALID", "the program has an empty vocabulary"));
        }
    }
    let derived = params.scope.derived_epsilon_bits(params.policy.repetition_count);
    if derived < params.policy.security_bits {
        return Err(Refusal::new(
            "SCOPE_CANNOT_MEET_POLICY",
            format!(
                "the committed scope derives -log2(eps) >= {derived}, the policy asks {} security bits: a weaker check than the policy is refused BEFORE the beacon, not after",
                params.policy.security_bits
            ),
        ));
    }

    log("static semantic admission (reference kernels, hypothetically armed)".into());
    let admission = static_admission(program, program_root, positions)?;

    let implementation_set = ImplementationSetV1::of_this_build(&pack);
    let source_root = tool_root(
        DOMAIN_SOURCE,
        &SourceProvenanceV1 {
            format: pack.model.format.clone(),
            config_digest: pack.model.config_digest.clone(),
            files: pack.model.files.iter().map(|f| (f.path.clone(), f.bytes, f.sha256.clone())).collect(),
            spec_digest: pack.frontend.spec_digest.clone(),
            adapter: (pack.frontend.adapter.kind.clone(), pack.frontend.adapter.id.clone(), pack.frontend.adapter.hash.clone()),
            builtin_pack_hash: pack.frontend.builtin_pack_hash.clone(),
            quant_descriptors: pack.quant.descriptors.iter().map(|q| (q.name.clone(), q.digest.clone())).collect(),
        },
    );
    let calibration = tool_root(
        DOMAIN_CALIBRATION,
        &CalibrationIdentityV1 {
            stats_digest: pack.profile.calibration.stats_digest.clone(),
            sites: pack.profile.calibration.sites as u64,
            source: super::manifest::blake2b256_hex(pack.profile.calibration.source.to_string().as_bytes()),
            headroom_bits: [
                pack.profile.policy.headroom16.to_bits(),
                pack.profile.policy.headroom32.to_bits(),
                pack.profile.policy.headroom_resid.to_bits(),
            ],
            max_window: pack.profile.max_window,
            context: pack.profile.context.map(|c| c as u64),
        },
    );
    let binding = tool_root(
        DOMAIN_BINDING,
        &InputStateBindingV1 {
            input_rule: "challenge-selected-prompts: length 1..=max_prompt_len and tokens uniform below token_bound, drawn from the vector stream of the seed".into(),
            token_bound: program.token_bound,
            initial_state: "zero-initial-state".into(),
            decode: "greedy-argmax-lowest-index-on-tie".into(),
            source_provenance_root: source_root,
        },
    );
    let resource = tool_root(
        DOMAIN_RESOURCE,
        &ResourceProfileV1 {
            mode: "streamed".into(),
            leaf_piece_bytes: kaspa_consensus_core::palw_tir_artifact_v1::PALW_TIR_ROW_PIECE_BYTES_V1,
            read_ahead_bytes: 4 << 20,
            reference_params: "lazy: one tensor decoded per ask".into(),
            independent_params: "lazy: one tensor decoded per ask, own codec".into(),
            backend_params: "mapped file".into(),
        },
    );

    let commitment = ConformanceCommitmentV1 {
        version: 1,
        chain_genesis: params.chain_genesis,
        ruleset_id: params.ruleset_id,
        subject_kind: SubjectKindV1::ModelConformance,
        candidate_id: params.candidate_id.unwrap_or(class_id_bytes),
        kernel_descriptor_id: admission.descriptor,
        challenge_policy_id: params.policy.id(),
        artifact_root,
        program_root,
        source_root: RootV1::Present(source_root),
        tokenizer_or_input_schema_root: RootV1::Present(m.tokenizer_id),
        layout_root,
        verification_plan_root: admission.plan_root,
        constraint_root: RootV1::Absent,
        implementation_set_root: implementation_set.root(),
        test_scope_root: params.scope.root(),
        calibration_id: RootV1::Present(calibration),
        input_and_state_binding_root: RootV1::Present(binding),
        resource_profile_id: resource,
        commitment_object_id: None,
        canonical_commitment_position: None,
    };
    commitment.well_formed().map_err(|e| Refusal::new("COMMITMENT_INVALID", e))?;
    Ok(BoundCommitment {
        commitment,
        params: params.clone(),
        pack_digest: pack.digest(),
        admission,
        class_id: class_id_bytes,
        artifact_bytes: m.artifact_bytes,
        leaf_count: m.leaf_count,
        implementation_set,
    })
}

/// Which statement fields differ between two commitments (names only) — the reason a commitment is stale.
pub fn commitment_diff(a: &ConformanceCommitmentV1, b: &ConformanceCommitmentV1) -> Vec<&'static str> {
    let mut out = Vec::new();
    macro_rules! cmp {
        ($($f:ident),*) => { $(if a.$f != b.$f { out.push(stringify!($f)); })* };
    }
    cmp!(
        version,
        chain_genesis,
        ruleset_id,
        subject_kind,
        candidate_id,
        kernel_descriptor_id,
        challenge_policy_id,
        artifact_root,
        program_root,
        source_root,
        tokenizer_or_input_schema_root,
        layout_root,
        verification_plan_root,
        constraint_root,
        implementation_set_root,
        test_scope_root,
        calibration_id,
        input_and_state_binding_root,
        resource_profile_id
    );
    out
}
