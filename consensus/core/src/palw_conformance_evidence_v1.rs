//! **Conformance evidence on chain — the pure half** (G14 onboarding P0, tag 109; RFC-0013 §9, RFC-0011 §17.2, RFC-0007 Part VI).
//!
//! ONE implementation of what a conformance run's evidence is, shared by the producer (the SDK's runtime pack re-exports every item
//! of the first section), the chain's fold and a fresh verifier: the committed test scope, the checks a seed selects, one check's
//! recorded outcome and the evidence assembled from them. Encodings are byte-identical to the runtime pack's (they moved here), so
//! an evidence id the pack wrote is the id the chain computes. On top of it: the network's INTERIM post-commit challenge policy,
//! the payloads tag 109 carries, the fold's judgement of posted evidence and of a refutation, and the fresh verifier.
//!
//! # Design: optimistic, with a bounded in-fold admission (RFC-0015's pattern)
//!
//! ```text
//! 107 commitment ─ FUTURE OPV Finals (FinalPathV1::PanelIndependent; op 212's rows) ─ PalwWorkBeaconV1 locks ─ challenge seed
//!  109 Post (registrant): evidence + scope + every selected check's outcome
//!      in fold: bound to THIS attempt (commitment, policy, the chain's own beacon and seed, the committed scope)?  no ⇒ dropped
//!               rebuilt EXACTLY from its material (selection from the seed, counts, status, result roots)? Passed with the
//!               policy's bits? vector outcomes of the decoded length?                                    no ⇒ CONFORMANCE_FAILED
//!      yes ⇒ the challenge window opens
//!  109 Refute (any other operator, inside the window, from public material):
//!      LeafDecode     an opening of a selected leaf against the class's artifact root whose bytes the reference decoder reads
//!                     differently from the posted outcome                                                   ⇒ CONFORMANCE_FAILED
//!      VectorTokens   a Final claim of the bound kernel class on the selected prompt (greedy) whose tokens contradict the
//!                     posted reference tokens                                                               ⇒ CONFORMANCE_FAILED
//!  the window closes unrefuted and the beacon re-derives unchanged ⇒ CONFORMANCE_PASSED
//!  no evidence by lock + deadline ⇒ the attempt defaults (withheld, never a pass); no lock by the window's end ⇒ BEACON_UNAVAILABLE
//! ```
//!
//! **Why not in-fold verification alone (design a).** The contract's `verify_conformance_evidence_v1` is cheap (O(k) hashes), and
//! the fold also rebuilds the whole evidence from the carried outcomes — but a vector check is a forward pass of the model on three
//! implementations and a leaf check needs the artifact's bytes; neither is in the block. What the fold can recompute, it recomputes
//! at posting; what it cannot, an outsider holding the public artifact (or a Final of the bound kernel class) refutes inside the
//! window. **Residual** (stated, not hidden): a vector outcome's logits / commits digests (`a`, `b`) have no on-chain court — only
//! its greedy tokens do, through a Final kernel claim; a fresh verifier re-executes the rest off chain (the runtime pack's
//! `verify-conformance`).

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_challenge::beacon::{BeaconContextV1, VerifiedWorkBeaconV1, WorkBeaconStateV1, WorkBeaconV1, WorkFinalEventV1};
use misaka_palw_challenge::conformance::{
    BeaconConformanceEvidenceV1, ConformanceCommitmentV1, ConformanceRefusalV1, ConformanceStatusV1, verify_conformance_evidence_v1,
};
use misaka_palw_challenge::hash::{self, Digest, named_id};
use misaka_palw_challenge::seed::{ChallengeStreamV1, StreamKindV1, StreamLabelV1, challenge_seed_v1};
use misaka_palw_challenge::{PostCommitChallengePolicyV1, RootV1};
use misaka_palw_tir::TirProgramV1;

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1, verify_artifact_opening_v1};
use crate::palw_tir_artifact_v1::{PalwTirInventoryRowV1, palw_tir_inventory_leaf_count_v1, palw_tir_visit_inventory_rows_v1};

// =================================================================================================================================
// I. The evidence core (moved from `misaka-palw-sdk`'s runtime pack; encodings unchanged)
// =================================================================================================================================

/// The check protocol a committed pack runs: `pack-sampled-differential/v1`. A different protocol is a different scope root.
pub const CHECK_PROTOCOL_V1: &str = "pack-sampled-differential/v1";

pub const DOMAIN_SCOPE: &[u8] = b"misaka.palw.runtime-pack.conformance-scope.v1";
pub const DOMAIN_SOURCES: &[u8] = b"misaka.palw.runtime-pack.beacon-sources.v1";
pub const DOMAIN_VECTORS: &[u8] = b"misaka.palw.runtime-pack.selected-vectors.v1";
pub const DOMAIN_RANGES: &[u8] = b"misaka.palw.runtime-pack.selected-leaves.v1";
pub const DOMAIN_RESULTS: &[u8] = b"misaka.palw.runtime-pack.result-root.v1";
pub const DOMAIN_OPENINGS: &[u8] = b"misaka.palw.runtime-pack.openings-root.v1";
pub const DOMAIN_LOCATORS: &[u8] = b"misaka.palw.runtime-pack.material-locators.v1";
pub const DOMAIN_CHECK_OUTCOME: &[u8] = b"misaka.palw.runtime-pack.check-outcome.v1";
pub const DOMAIN_VALUES: &[u8] = b"misaka.palw.runtime-pack.decoded-values.v1";

/// A typed record's root under a tool domain (`H(domain; borsh(record))`, the contract's hash suite).
pub fn tool_root<T: BorshSerialize>(domain: &[u8], record: &T) -> Digest {
    hash::object_id(domain, record)
}

/// **The fault model a soundness policy approves**: the densest fault (ppm of vector / leaf draws on which a faulty implementation
/// shows) a scope may assume under it. The candidate never chooses it (C4 GAP-C4-D): a denser assumed fault buys unearned bits.
/// Only the reference policy's unreviewed test soundness id has one here; every reviewed soundness policy is an external gate, so no
/// production policy can pass conformance until its fault model is approved and listed.
pub fn approved_fault_model_v1(policy: &PostCommitChallengePolicyV1) -> Option<(u32, u32)> {
    (policy.soundness_policy_id == named_id("soundness/unreviewed-test-only/v1")).then_some((1_000_000, 1_000_000))
}

/// **What the committed check is** — fixed before any randomness (it is the commitment's `test_scope_root`).
///
/// A repetition (the policy's `repetition_count` of them) draws, from its own labelled streams of the one challenge seed,
/// `vectors_per_repetition` prompts (full forward passes on every required implementation) and `leaves_per_repetition` artifact
/// leaves (an authenticated opening against the artifact root, then the integer values every required implementation decodes from
/// those bytes). Nothing else is checked: this is a SAMPLED differential check, not full-scope fidelity, not semantic admission.
///
/// The fault model is part of the scope: `*_fault_ppm` is the density of faulty draws the bound speaks about (a fault that shows
/// on at least that fraction of the family's draws), under independent uniform draws. See [`ConformanceScopeV1::derived_epsilon_bits`].
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

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SelectedVectorV1 {
    pub check_id: String,
    pub repetition: u32,
    pub index: u32,
    pub prompt: Vec<u32>,
    pub decode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SelectedLeafV1 {
    pub check_id: String,
    pub repetition: u32,
    pub ordinal: u32,
    pub leaf_index: u32,
    pub param: u16,
    pub layer: Option<u16>,
    pub row_start: u32,
    pub len: u32,
    pub tensor_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SelectionV1 {
    pub vectors: Vec<SelectedVectorV1>,
    pub leaves: Vec<SelectedLeafV1>,
}

impl SelectionV1 {
    pub fn vectors_root(&self) -> Digest {
        tool_root(DOMAIN_VECTORS, &self.vectors)
    }
    pub fn leaves_root(&self) -> Digest {
        tool_root(DOMAIN_RANGES, &self.leaves)
    }
    pub fn checks_required(&self) -> u64 {
        (self.vectors.len() + self.leaves.len()) as u64
    }
    /// Check ids in canonical order: vectors, then leaves.
    pub fn check_ids(&self) -> Vec<String> {
        self.vectors.iter().map(|v| v.check_id.clone()).chain(self.leaves.iter().map(|l| l.check_id.clone())).collect()
    }
}

/// Why a seed selects nothing: the code is the runtime pack's (`SAMPLER_EXHAUSTED` / `SCOPE_INVALID`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionRefusalV1 {
    pub code: &'static str,
    pub detail: String,
}

fn sample_refusal(e: impl std::fmt::Display) -> SelectionRefusalV1 {
    SelectionRefusalV1 { code: "SAMPLER_EXHAUSTED", detail: e.to_string() }
}

fn scope_refusal(e: impl std::fmt::Display) -> SelectionRefusalV1 {
    SelectionRefusalV1 { code: "SCOPE_INVALID", detail: e.to_string() }
}

/// **The checks a seed selects**, from the contract's streams only: one labelled stream per (family, relation, repetition).
pub fn derive_selection_v1(
    seed: &Digest,
    policy: &PostCommitChallengePolicyV1,
    scope: &ConformanceScopeV1,
    program: &TirProgramV1,
) -> Result<SelectionV1, SelectionRefusalV1> {
    let vector_scope = named_id("pack-conformance/vector/v1");
    let leaf_scope = named_id("pack-conformance/artifact-leaf/v1");
    let leaf_count = palw_tir_inventory_leaf_count_v1(program).map_err(scope_refusal)?;
    let mut vectors = Vec::new();
    let mut raw_leaves: Vec<(u32, u32, u32)> = Vec::new();
    for r in 0..policy.repetition_count {
        for i in 0..scope.vectors_per_repetition {
            let mut s = ChallengeStreamV1::new(
                seed,
                &StreamLabelV1 { kind: StreamKindV1::Vector, scope_id: vector_scope, relation: i, repetition: r },
            );
            let len = 1 + s.index_below(scope.max_prompt_len as u64).map_err(sample_refusal)? as u32;
            let mut prompt = Vec::with_capacity(len as usize);
            for _ in 0..len {
                prompt.push(s.index_below(program.token_bound as u64).map_err(sample_refusal)? as u32);
            }
            vectors.push(SelectedVectorV1 {
                check_id: format!("vec/r{r}/i{i}"),
                repetition: r,
                index: i,
                prompt,
                decode: scope.decode_tokens,
            });
        }
        if scope.leaves_per_repetition > 0 {
            let mut s = ChallengeStreamV1::new(
                seed,
                &StreamLabelV1 { kind: StreamKindV1::TensorRange, scope_id: leaf_scope, relation: 0, repetition: r },
            );
            let picked = s.distinct_indices(leaf_count as u64, scope.leaves_per_repetition as u64).map_err(sample_refusal)?;
            for (k, idx) in picked.into_iter().enumerate() {
                raw_leaves.push((r, k as u32, idx as u32));
            }
        }
    }
    // Where each drawn leaf lives: one walk of the closed-form inventory layout.
    let want: BTreeSet<u32> = raw_leaves.iter().map(|x| x.2).collect();
    let mut coords: BTreeMap<u32, PalwTirInventoryRowV1> = BTreeMap::new();
    let mut at = 0u32;
    palw_tir_visit_inventory_rows_v1(program, &mut |row| {
        if want.contains(&at) {
            coords.insert(at, row);
        }
        at += 1;
    })
    .map_err(scope_refusal)?;
    let mut leaves = Vec::new();
    for (r, k, idx) in raw_leaves {
        let row = coords.get(&idx).ok_or_else(|| scope_refusal(format!("leaf {idx} has no coordinates")))?;
        let tensor_name =
            program.params.get(row.param as usize).map(|p| p.name.clone()).ok_or_else(|| scope_refusal("no such param"))?;
        leaves.push(SelectedLeafV1 {
            check_id: format!("leaf/r{r}/k{k}"),
            repetition: r,
            ordinal: k,
            leaf_index: idx,
            param: row.param,
            layer: row.layer,
            row_start: row.row_start,
            len: row.len,
            tensor_name,
        });
    }
    Ok(SelectionV1 { vectors, leaves })
}

/// One implementation's result for one check.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ResultV1 {
    NotRun,
    Error(String),
    Ran { a: [u8; 32], b: [u8; 32], tokens: Vec<u32>, positions: u32 },
}

/// The deterministic record of one check: nothing in it depends on the machine, the time or the thread count.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CheckOutcomeV1 {
    pub check_id: String,
    pub reference: ResultV1,
    pub independent: ResultV1,
    pub backend: ResultV1,
    pub disagreement: Option<String>,
}

impl CheckOutcomeV1 {
    pub fn digest(&self) -> Digest {
        tool_root(DOMAIN_CHECK_OUTCOME, self)
    }

    pub fn result(&self, role: Role) -> &ResultV1 {
        match role {
            Role::Reference => &self.reference,
            Role::Independent => &self.independent,
            Role::Backend => &self.backend,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Reference,
    Independent,
    Backend,
}

impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Role::Reference => "reference",
            Role::Independent => "independent",
            Role::Backend => "backend",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    Passed,
    Failed(String),
    /// A required implementation did not run. `chosen` says it was switched off on purpose (SKIPPED), not lost (INCOMPLETE).
    Missing {
        roles: Vec<&'static str>,
        chosen: bool,
    },
}

/// Judge one check against the scope's required implementations.
pub fn check_status(o: &CheckOutcomeV1, scope: &ConformanceScopeV1) -> CheckStatus {
    let mut required = vec![Role::Reference];
    if scope.require_independent {
        required.push(Role::Independent);
    }
    if scope.require_backend {
        required.push(Role::Backend);
    }
    for role in [Role::Reference, Role::Independent, Role::Backend] {
        if let ResultV1::Error(e) = o.result(role) {
            return CheckStatus::Failed(format!("{} implementation failed: {e}", role.name()));
        }
    }
    if let Some(d) = &o.disagreement {
        return CheckStatus::Failed(d.clone());
    }
    let ran: Vec<(Role, &ResultV1)> = [Role::Reference, Role::Independent, Role::Backend]
        .into_iter()
        .map(|r| (r, o.result(r)))
        .filter(|(_, x)| matches!(x, ResultV1::Ran { .. }))
        .collect();
    if let Some((_, first)) = ran.first()
        && let Some((r, _)) = ran.iter().find(|(_, x)| x != first)
    {
        return CheckStatus::Failed(format!("the {} implementation's result differs from the reference's", r.name()));
    }
    let missing: Vec<&'static str> = required.iter().filter(|r| matches!(o.result(**r), ResultV1::NotRun)).map(|r| r.name()).collect();
    if missing.is_empty() { CheckStatus::Passed } else { CheckStatus::Missing { roles: missing, chosen: true } }
}

/// The digest of a leaf's decoded integer values (what a leaf check's `a` records).
pub fn values_digest_v1(values: &[i128]) -> [u8; 32] {
    let mut st = blake2b_simd::Params::new().hash_length(32).key(DOMAIN_VALUES).to_state();
    st.update(&(values.len() as u64).to_le_bytes());
    for v in values {
        st.update(&v.to_le_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// **The reference implementation's reading of a leaf's bytes** (the runtime pack's leaf check, reference role): the bytes decoded as
/// whole elements of the param's dtype. A Result path on any bytes (never a panic): a ragged leaf or an undecodable one is an error.
pub fn reference_leaf_result_v1(program: &TirProgramV1, param: u16, bytes: &[u8]) -> Result<[u8; 32], String> {
    let decl = program.params.get(param as usize).ok_or("the program declares no such param")?;
    let dtype = decl.dtype;
    let w = dtype.width();
    if w == 0 || bytes.len() % w != 0 {
        return Err(format!("{} bytes are not whole {} elements", bytes.len(), dtype.name()));
    }
    let n = bytes.len() / w;
    let t = misaka_palw_tir::Tensor::from_le_bytes(dtype, &[n], bytes).map_err(|e| e.to_string())?;
    Ok(values_digest_v1(&t.data))
}

/// `authenticated_openings_root` of a multiproof (`None` when no leaf was drawn).
pub fn openings_root_v1(proof: &Option<PalwArtifactMultiproofV1>) -> Digest {
    tool_root(DOMAIN_OPENINGS, proof)
}

/// **Assemble the evidence from the checks that ran** — a pure function of (commitment, policy, scope, beacon, seed, selection,
/// openings root, outcomes). The producer assembles it; the chain's fold and a verifier re-assemble it from scratch, and evidence is
/// accepted only if their assembly equals it exactly. `openings_root` is [`openings_root_v1`] of the producer's multiproof (the
/// chain does not carry the multiproof: its leaf outcomes are refutable against the public artifact instead).
#[allow(clippy::too_many_arguments)]
pub fn assemble_evidence_v1(
    commitment: &ConformanceCommitmentV1,
    policy: &PostCommitChallengePolicyV1,
    scope: &ConformanceScopeV1,
    beacon: &WorkBeaconV1,
    seed: &Digest,
    selection: &SelectionV1,
    openings_root: &Digest,
    outcomes: &BTreeMap<String, CheckOutcomeV1>,
) -> BeaconConformanceEvidenceV1 {
    let ids = selection.check_ids();
    let (mut run, mut failed, mut chosen_missing, mut lost_missing) = (0u64, 0u64, 0u64, 0u64);
    let (mut missing_checks, mut failures) = (Vec::new(), Vec::new());
    for id in &ids {
        match outcomes.get(id) {
            None => {
                lost_missing += 1;
                missing_checks.push(format!("{id}: not run"));
            }
            Some(o) => match check_status(o, scope) {
                CheckStatus::Passed => run += 1,
                CheckStatus::Failed(why) => {
                    run += 1;
                    failed += 1;
                    failures.push(format!("{id}: {why}"));
                }
                CheckStatus::Missing { roles, chosen } => {
                    if chosen {
                        chosen_missing += 1;
                    } else {
                        lost_missing += 1;
                    }
                    missing_checks.push(format!("{id}: required implementation(s) not run: {}", roles.join(", ")));
                }
            },
        }
    }
    let role_root = |role: Role| -> Digest {
        let rows: Vec<(String, ResultV1)> =
            ids.iter().map(|id| (id.clone(), outcomes.get(id).map(|o| o.result(role).clone()).unwrap_or(ResultV1::NotRun))).collect();
        tool_root(DOMAIN_RESULTS, &(role.name().to_string(), rows))
    };
    let outcome_digests: Vec<Digest> =
        ids.iter().map(|id| outcomes.get(id).map(CheckOutcomeV1::digest).unwrap_or([0u8; 64])).collect();
    let status = if failed > 0 {
        ConformanceStatusV1::Failed
    } else if lost_missing > 0 {
        ConformanceStatusV1::Incomplete
    } else if chosen_missing > 0 {
        ConformanceStatusV1::Skipped
    } else {
        ConformanceStatusV1::Passed
    };
    let selection_digest = tool_root(b"misaka.palw.runtime-pack.selection.v1", selection);
    BeaconConformanceEvidenceV1 {
        version: 1,
        commitment_root: commitment.statement_root(),
        challenge_policy_id: policy.id(),
        challenge_anchor: beacon.challenge_anchor,
        qualifying_source_evidence_root: tool_root(DOMAIN_SOURCES, &beacon.sources),
        lock_evidence_root: misaka_palw_challenge::lock_evidence_root_v1(beacon),
        lock_position: beacon.lock_position,
        beacon_output: beacon.output,
        challenge_seed: *seed,
        selected_vectors_root: selection.vectors_root(),
        selected_tensor_ranges_root: selection.leaves_root(),
        reference_result_root: role_root(Role::Reference),
        independent_result_root: role_root(Role::Independent),
        backend_result_root: role_root(Role::Backend),
        authenticated_openings_root: *openings_root,
        transcript_root: RootV1::Absent,
        checks_required: selection.checks_required(),
        checks_run: run,
        checks_failed: failed,
        missing_checks,
        failures,
        scope_and_fault_model_id: scope.scope_and_fault_model_id(policy.repetition_count),
        soundness_assumptions_root: tool_root(
            b"misaka.palw.runtime-pack.soundness-assumptions.v1",
            &(policy.soundness_policy_id, policy.field_policy_id, scope.scope_and_fault_model_id(policy.repetition_count)),
        ),
        derived_epsilon_bits: scope.derived_epsilon_bits(policy.repetition_count),
        status,
        public_material_locator_root: tool_root(DOMAIN_LOCATORS, &(selection_digest, *openings_root, outcome_digests)),
    }
}

/// The evidence fields that differ between two evidences (names only).
pub fn evidence_diff_v1(a: &BeaconConformanceEvidenceV1, b: &BeaconConformanceEvidenceV1) -> Vec<&'static str> {
    let mut out = Vec::new();
    macro_rules! cmp {
        ($($f:ident),*) => { $(if a.$f != b.$f { out.push(stringify!($f)); })* };
    }
    cmp!(
        version,
        commitment_root,
        challenge_policy_id,
        challenge_anchor,
        qualifying_source_evidence_root,
        lock_evidence_root,
        lock_position,
        beacon_output,
        challenge_seed,
        selected_vectors_root,
        selected_tensor_ranges_root,
        reference_result_root,
        independent_result_root,
        backend_result_root,
        authenticated_openings_root,
        transcript_root,
        checks_required,
        checks_run,
        checks_failed,
        missing_checks,
        failures,
        scope_and_fault_model_id,
        soundness_assumptions_root,
        derived_epsilon_bits,
        status,
        public_material_locator_root
    );
    out
}

// =================================================================================================================================
// II. The network's challenge policy and the chain's bounds (INTERIM terms of a never-armed fence)
// =================================================================================================================================

/// The policy's numbers: `k` works, the anchor delay, the collection window, the settlement depth `D`, the repetitions, the bits.
pub const PALW_ONBOARDING_POLICY_K_V1: u32 = 2;
pub const PALW_ONBOARDING_POLICY_DELAY_DAA_V1: u64 = 2;
pub const PALW_ONBOARDING_POLICY_WINDOW_DAA_V1: u64 = 120;
pub const PALW_ONBOARDING_POLICY_DEPTH_DAA_V1: u64 = 2;
pub const PALW_ONBOARDING_POLICY_REPETITIONS_V1: u32 = 1;
pub const PALW_ONBOARDING_POLICY_SECURITY_BITS_V1: u16 = 2;

/// **The network's post-commit challenge policy for model onboarding** — INTERIM, a consensus constant of the never-armed kernel route
/// fence (like every other onboarding term): the contract's reference policy (unreviewed soundness id, `retry_limit` 2, so three
/// counted attempts) with a collection window long enough for an OPV Final (window 50 DAA) to settle inside it, and **2 bits**: a
/// number a drill can reach with a handful of checks, NOT a security value. An activation replaces it by a reviewed, approved tuple
/// (RFC-0007 §VI.8). Its id is what tag 106 binds a class to and tag 107's commitment names.
pub fn palw_onboarding_challenge_policy_v1() -> PostCommitChallengePolicyV1 {
    PostCommitChallengePolicyV1 {
        security_bits: PALW_ONBOARDING_POLICY_SECURITY_BITS_V1,
        ..misaka_palw_challenge::reference_policy_v1(
            PALW_ONBOARDING_POLICY_K_V1,
            PALW_ONBOARDING_POLICY_DELAY_DAA_V1,
            PALW_ONBOARDING_POLICY_WINDOW_DAA_V1,
            PALW_ONBOARDING_POLICY_DEPTH_DAA_V1,
            PALW_ONBOARDING_POLICY_REPETITIONS_V1,
        )
    }
}

/// After the beacon locks, the registrant has this long to post the attempt's evidence; then the attempt DEFAULTS (withheld evidence
/// is a counted failure, never a pass). INTERIM.
pub const PALW_CONFORMANCE_EVIDENCE_DEADLINE_DAA_V1: u64 = 60;
/// Posted evidence is refutable for this long; it can pass only once the window closes unrefuted. Long enough for an outsider to
/// post a job on the bound kernel class and see an OPV claim of it reach Final (window 50 DAA). INTERIM.
pub const PALW_CONFORMANCE_CHALLENGE_WINDOW_DAA_V1: u64 = 80;

/// **The chain's verification bound on a committed scope** — what the fold re-derives at posting: checks selected, prompt-token draws,
/// decoded tokens. A scope past it can never pass on this chain (the registrant chose it; its evidence fails, counted).
pub const PALW_CONFORMANCE_MAX_CHECKS_V1: u64 = 4_096;
pub const PALW_CONFORMANCE_MAX_PROMPT_DRAWS_V1: u64 = 1 << 20;
pub const PALW_CONFORMANCE_MAX_DECODE_V1: u32 = 1 << 12;

/// Whether `scope` is inside the chain's verification bound under `policy`.
pub fn scope_within_chain_bound_v1(scope: &ConformanceScopeV1, policy: &PostCommitChallengePolicyV1) -> Result<(), String> {
    let reps = policy.repetition_count as u64;
    let checks = reps.saturating_mul(scope.checks_per_repetition());
    if checks > PALW_CONFORMANCE_MAX_CHECKS_V1 {
        return Err(format!("{checks} checks, past the chain's bound of {PALW_CONFORMANCE_MAX_CHECKS_V1}"));
    }
    let draws = reps.saturating_mul(scope.vectors_per_repetition as u64).saturating_mul(scope.max_prompt_len as u64);
    if draws > PALW_CONFORMANCE_MAX_PROMPT_DRAWS_V1 {
        return Err(format!("{draws} prompt-token draws, past the chain's bound of {PALW_CONFORMANCE_MAX_PROMPT_DRAWS_V1}"));
    }
    if scope.decode_tokens > PALW_CONFORMANCE_MAX_DECODE_V1 {
        return Err(format!(
            "{} decoded tokens per vector, past the chain's bound of {PALW_CONFORMANCE_MAX_DECODE_V1}",
            scope.decode_tokens
        ));
    }
    Ok(())
}

// =================================================================================================================================
// III. What tag 109 carries
// =================================================================================================================================

/// **The evidence of one attempt and the material that rebuilds it**: the contract's evidence, the committed scope (its root is the
/// commitment's `test_scope_root`) and one outcome per selected check, in the selection's canonical order (vectors, then leaves).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ConformanceEvidencePostV1 {
    pub evidence: BeaconConformanceEvidenceV1,
    pub scope: ConformanceScopeV1,
    pub outcomes: Vec<CheckOutcomeV1>,
}

/// **An objective fault in posted evidence, from public material.** `check` indexes the selection's canonical check list (vectors,
/// then leaves).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ConformanceFaultV1 {
    /// A selected artifact leaf, opened against the class's registered artifact root at the leaf the selection names, whose bytes the
    /// reference decoder reads differently from what the posted outcome records.
    LeafDecode { check: u32, opening: PalwArtifactOpeningV1 },
    /// A selected vector whose posted reference tokens a **Final** claim of the class's bound kernel class contradicts: the claim's job
    /// is the selected prompt under the greedy rule, and one token both name differs.
    VectorTokens { check: u32, kernel_claim: Hash64 },
}

/// **Tag 109's action.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ConformanceEvidenceActionV1 {
    /// The class's registrant posts the evidence of its current attempt.
    Post(Box<ConformanceEvidencePostV1>),
    /// Any other operator refutes posted evidence inside its challenge window.
    Refute { evidence_id: Hash64, fault: Box<ConformanceFaultV1> },
}

// =================================================================================================================================
// IV. The fold's judgement
// =================================================================================================================================

/// Posted evidence that is not evidence about THIS attempt (another commitment, policy, beacon, seed or scope): it decides nothing, and
/// the object is dropped. The registrant may post the attempt's real evidence until the deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotThisAttemptV1(pub String);

/// The verdict of evidence bound to this attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PostVerdictV1 {
    /// Rebuilt exactly, a complete pass with the policy's bits: the challenge window opens.
    Pass,
    /// Bound to this attempt and not a pass (honestly reported, forged, or below the chain's bound): the attempt fails, counted.
    Fail { code: &'static str, detail: String },
}

fn fail(code: &'static str, detail: impl Into<String>) -> PostVerdictV1 {
    PostVerdictV1::Fail { code, detail: detail.into() }
}

/// **Judge posted evidence** against what the chain derived for the attempt: its commitment, the network's policy, the beacon
/// context frozen at the commitment, the beacon this chain locked and the class's program. Returns the seed and the verdict.
pub fn judge_posted_evidence_v1(
    commitment: &ConformanceCommitmentV1,
    policy: &PostCommitChallengePolicyV1,
    ctx: &BeaconContextV1,
    beacon: &VerifiedWorkBeaconV1,
    program: &TirProgramV1,
    post: &ConformanceEvidencePostV1,
) -> Result<(Digest, PostVerdictV1), NotThisAttemptV1> {
    let not = |why: &str| NotThisAttemptV1(why.to_string());
    let seed = challenge_seed_v1(ctx, &commitment.subject(), beacon).map_err(|e| NotThisAttemptV1(e.to_string()))?;
    let ev = &post.evidence;
    // ---- bound to this attempt? ----
    if ev.version != 1 {
        return Err(not("unknown evidence version"));
    }
    if ev.commitment_root != commitment.statement_root() {
        return Err(not("the evidence names another commitment (a changed artifact, layout, plan, scope or implementation set)"));
    }
    if ev.challenge_policy_id != commitment.challenge_policy_id || ev.challenge_policy_id != policy.id() {
        return Err(not("the evidence names another challenge policy"));
    }
    if ev.beacon_output != beacon.output
        || ev.challenge_anchor != beacon.challenge_anchor
        || ev.lock_position != beacon.lock_position
        || ev.lock_evidence_root != misaka_palw_challenge::lock_evidence_root_v1(beacon.beacon())
    {
        return Err(not("the evidence's beacon is not the one this chain locked for the attempt"));
    }
    if ev.challenge_seed != seed {
        return Err(not("the evidence's seed is not the one this chain derives (another context, a stale or forged seed)"));
    }
    if post.scope.root() != commitment.test_scope_root {
        return Err(not("the carried scope is not the committed one"));
    }
    // ---- bound: from here every outcome is the attempt's ----
    if let Err(why) = post.scope.validate(policy) {
        return Ok((seed, fail("SCOPE_BELOW_PROTOCOL", why)));
    }
    if let Err(why) = scope_within_chain_bound_v1(&post.scope, policy) {
        return Ok((seed, fail("SCOPE_BEYOND_CHAIN_BOUND", why)));
    }
    let selection = match derive_selection_v1(&seed, policy, &post.scope, program) {
        Ok(s) => s,
        Err(e) => return Ok((seed, fail(e.code, e.detail))),
    };
    let ids: BTreeSet<String> = selection.check_ids().into_iter().collect();
    let mut outcomes: BTreeMap<String, CheckOutcomeV1> = BTreeMap::new();
    for o in &post.outcomes {
        if !ids.contains(&o.check_id) {
            return Ok((seed, fail("EVIDENCE_FORGED", format!("an outcome for {}, which the seed did not select", o.check_id))));
        }
        if outcomes.insert(o.check_id.clone(), o.clone()).is_some() {
            return Ok((seed, fail("EVIDENCE_FORGED", format!("two outcomes for {}", o.check_id))));
        }
    }
    let rebuilt =
        assemble_evidence_v1(commitment, policy, &post.scope, beacon, &seed, &selection, &ev.authenticated_openings_root, &outcomes);
    if rebuilt != *ev {
        return Ok((
            seed,
            fail(
                "EVIDENCE_FORGED",
                format!("the material does not rebuild the evidence; differing: {}", evidence_diff_v1(&rebuilt, ev).join(", ")),
            ),
        ));
    }
    if let Err(e) = verify_conformance_evidence_v1(commitment, beacon, &seed, policy.security_bits, ev) {
        let code = match e {
            ConformanceRefusalV1::NotPassed(_) | ConformanceRefusalV1::Incomplete { .. } => "CONFORMANCE_NOT_PASSED",
            ConformanceRefusalV1::WeakEpsilon => "EPSILON_BELOW_POLICY",
            _ => "EVIDENCE_REFUSED",
        };
        return Ok((seed, fail(code, e.to_string())));
    }
    // A passing vector check names exactly the decoded tokens (so a Final kernel claim can contradict them).
    for v in &selection.vectors {
        if let Some(ResultV1::Ran { tokens, .. }) = outcomes.get(&v.check_id).map(|o| &o.reference)
            && tokens.len() != v.decode as usize
        {
            return Ok((
                seed,
                fail("EVIDENCE_FORGED", format!("{}: {} reference tokens, not {}", v.check_id, tokens.len(), v.decode)),
            ));
        }
    }
    Ok((seed, PostVerdictV1::Pass))
}

/// The selected check `index` names, with its posted outcome.
pub enum SelectedCheckV1<'a> {
    Vector(&'a SelectedVectorV1, &'a CheckOutcomeV1),
    Leaf(&'a SelectedLeafV1, &'a CheckOutcomeV1),
}

/// The selected check at `index` of the canonical list and the posted outcome for it.
pub fn selected_check_v1<'a>(
    selection: &'a SelectionV1,
    post: &'a ConformanceEvidencePostV1,
    index: u32,
) -> Option<SelectedCheckV1<'a>> {
    let i = index as usize;
    let outcome_of = |id: &str| post.outcomes.iter().find(|o| o.check_id == id);
    if i < selection.vectors.len() {
        let v = &selection.vectors[i];
        return Some(SelectedCheckV1::Vector(v, outcome_of(&v.check_id)?));
    }
    let l = selection.leaves.get(i - selection.vectors.len())?;
    Some(SelectedCheckV1::Leaf(l, outcome_of(&l.check_id)?))
}

/// **Judge a leaf refutation**: `Ok(())` means the posted evidence is FALSE about the leaf (proven); `Err` says why the opening proves
/// nothing. The opening must reach the class's registered artifact root at exactly the leaf and coordinates the selection names.
pub fn judge_leaf_fault_v1(
    program: &TirProgramV1,
    artifact_root: Hash64,
    leaf: &SelectedLeafV1,
    outcome: &CheckOutcomeV1,
    opening: &PalwArtifactOpeningV1,
) -> Result<(), &'static str> {
    let count = palw_tir_inventory_leaf_count_v1(program).map_err(|_| "the program's inventory is not countable")?;
    if opening.leaf_index != leaf.leaf_index || opening.leaf_count != count {
        return Err("the opening is not at the selected leaf of the class's inventory");
    }
    let op = &opening.operand;
    if op.tensor_name != leaf.tensor_name
        || op.layer != leaf.layer
        || op.row_start != leaf.row_start
        || op.bytes.len() != leaf.len as usize
    {
        return Err("the opening names other coordinates than the selected leaf");
    }
    verify_artifact_opening_v1(opening, artifact_root)
        .map_err(|_| "the opening does not reach the class's registered artifact root")?;
    let truth = reference_leaf_result_v1(program, leaf.param, &op.bytes);
    match (&outcome.reference, truth) {
        (ResultV1::Ran { a, .. }, Ok(digest)) if *a == digest => Err("the posted outcome is the reference reading of the leaf"),
        (ResultV1::Ran { .. }, _) => Ok(()),
        // A passing evidence only carries Ran reference results; anything else was already a failure at posting.
        _ => Err("the posted outcome records no reference reading to contradict"),
    }
}

/// **Judge a vector refutation** from a Final kernel claim's job and tokens: `Ok(())` means the evidence is FALSE about the vector.
pub fn judge_vector_fault_v1(
    vector: &SelectedVectorV1,
    outcome: &CheckOutcomeV1,
    job_prompt: &[u32],
    job_is_greedy: bool,
    claim_tokens: &[u32],
) -> Result<(), &'static str> {
    if !job_is_greedy || job_prompt != vector.prompt.as_slice() {
        return Err("the claim's job is not the selected prompt under the greedy rule");
    }
    let ResultV1::Ran { tokens, .. } = &outcome.reference else {
        return Err("the posted outcome records no reference tokens to contradict");
    };
    if tokens.iter().zip(claim_tokens).any(|(a, b)| a != b) {
        Ok(())
    } else {
        Err("the Final claim's tokens agree with the posted ones")
    }
}

// =================================================================================================================================
// V. The fresh verifier: the verdict from public reads alone
// =================================================================================================================================

/// What a fresh verifier holds: public reads (op 231's attempt and evidence rows, op 212's Final facts, the class's program and
/// registered artifact root from op 230 / the registry) and, optionally, the artifact from its public source.
pub struct FreshInputV1<'a> {
    pub commitment: &'a ConformanceCommitmentV1,
    pub policy: &'a PostCommitChallengePolicyV1,
    /// The beacon context the chain froze at the commitment (op 231).
    pub ctx: &'a BeaconContextV1,
    /// Every Final fact the route serves (op 212), any order.
    pub events: &'a [WorkFinalEventV1],
    /// The DAA the reads were taken at.
    pub tip_daa: u64,
    pub program: &'a TirProgramV1,
    pub artifact_root: Hash64,
    /// The posted evidence (op 231), if any.
    pub post: Option<&'a ConformanceEvidencePostV1>,
    /// The bytes of a selected leaf from the public artifact: `(param, layer, row_start, len) → bytes`, or `None` (not held).
    pub leaf_source: Option<&'a dyn Fn(&SelectedLeafV1) -> Option<Vec<u8>>>,
}

/// **The fresh verifier's verdict.** `beacon` and `posted` are what the verifier itself derived; `leaves_rechecked` counts the
/// selected leaves it re-read from the artifact and found to agree (a disagreement is a refutation it can file: `leaf_faults`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FreshVerdictV1 {
    /// `COLLECTING have/need`, `CANDIDATE lock@…`, `LOCKED`, `UNAVAILABLE have/need`.
    pub beacon: String,
    pub beacon_output: Option<Digest>,
    pub seed: Option<Digest>,
    /// `None`: nothing posted. `Some(Ok(()))`: bound and rebuilt exactly, a pass. `Some(Err(code: detail))`: not this attempt's
    /// evidence, or bound and failing.
    pub posted: Option<Result<(), String>>,
    pub leaves_selected: u32,
    pub leaves_rechecked: u32,
    /// Selected leaves whose public bytes contradict the posted outcome: `check` indices a `LeafDecode` refutation can name.
    pub leaf_faults: Vec<u32>,
    /// Vector checks: their tokens are refutable through a Final kernel claim; their logits digests are not re-executed here.
    pub vectors_selected: u32,
}

/// **Rebuild the verdict from public material alone** (no node-private state): the beacon from the Final facts, the seed, the
/// selection, the evidence re-assembled from its posted material, and — given the artifact — every selected leaf re-read.
pub fn fresh_verify_v1(input: &FreshInputV1<'_>) -> FreshVerdictV1 {
    let mut out = FreshVerdictV1 {
        beacon: String::new(),
        beacon_output: None,
        seed: None,
        posted: None,
        leaves_selected: 0,
        leaves_rechecked: 0,
        leaf_faults: Vec::new(),
        vectors_selected: 0,
    };
    let beacon = match misaka_palw_challenge::collect_work_beacon_v1(input.ctx, input.events, input.tip_daa) {
        Ok(WorkBeaconStateV1::Locked(b)) => {
            out.beacon = format!("LOCKED at {}", b.lock_position);
            out.beacon_output = Some(b.output);
            b
        }
        Ok(WorkBeaconStateV1::Collecting { have, need }) => {
            out.beacon = format!("COLLECTING {have}/{need}");
            return out;
        }
        Ok(WorkBeaconStateV1::Candidate { have, lock_position }) => {
            out.beacon = format!("CANDIDATE {have} works, locks at {lock_position}");
            return out;
        }
        Ok(WorkBeaconStateV1::Unavailable { have, need }) => {
            out.beacon = format!("UNAVAILABLE {have}/{need}");
            return out;
        }
        Err(e) => {
            out.beacon = format!("POLICY_INVALID {e}");
            return out;
        }
    };
    let Some(post) = input.post else {
        out.seed = challenge_seed_v1(input.ctx, &input.commitment.subject(), &beacon).ok();
        return out;
    };
    match judge_posted_evidence_v1(input.commitment, input.policy, input.ctx, &beacon, input.program, post) {
        Err(NotThisAttemptV1(why)) => {
            out.seed = challenge_seed_v1(input.ctx, &input.commitment.subject(), &beacon).ok();
            out.posted = Some(Err(format!("NOT_THIS_ATTEMPT: {why}")));
        }
        Ok((seed, verdict)) => {
            out.seed = Some(seed);
            out.posted = Some(match verdict {
                PostVerdictV1::Pass => Ok(()),
                PostVerdictV1::Fail { code, detail } => Err(format!("{code}: {detail}")),
            });
            if let Ok(selection) = derive_selection_v1(&seed, input.policy, &post.scope, input.program) {
                out.vectors_selected = selection.vectors.len() as u32;
                out.leaves_selected = selection.leaves.len() as u32;
                if let Some(source) = input.leaf_source {
                    for (j, leaf) in selection.leaves.iter().enumerate() {
                        let Some(outcome) = post.outcomes.iter().find(|o| o.check_id == leaf.check_id) else { continue };
                        let Some(bytes) = source(leaf) else { continue };
                        let agrees = matches!(
                            (&outcome.reference, reference_leaf_result_v1(input.program, leaf.param, &bytes)),
                            (ResultV1::Ran { a, .. }, Ok(d)) if *a == d
                        );
                        if agrees {
                            out.leaves_rechecked += 1;
                        } else {
                            out.leaf_faults.push((selection.vectors.len() + j) as u32);
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The interim policy is structurally valid, names the contract's reference algorithms, and its scope bound admits the drill's.
    #[test]
    fn the_interim_onboarding_policy_validates_and_its_numbers_are_the_declared_ones() {
        let p = palw_onboarding_challenge_policy_v1();
        p.validate().expect("structurally valid");
        assert_eq!(
            (p.work_count_k, p.anchor_delay_slots, p.beacon_window_slots, p.settlement_depth_d, p.repetition_count, p.security_bits),
            (2, 2, 120, 2, 1, 2)
        );
        assert_eq!(p.retry_limit, 2, "three counted attempts");
        assert!(approved_fault_model_v1(&p).is_some(), "the unreviewed test soundness id has its fault model");
        let mut s = ConformanceScopeV1::new(2, 3, 2, 2);
        (s.vector_fault_ppm, s.leaf_fault_ppm) = (1_000_000, 1_000_000);
        s.validate(&p).unwrap();
        scope_within_chain_bound_v1(&s, &p).unwrap();
        assert!(s.derived_epsilon_bits(p.repetition_count) >= p.security_bits);
        let huge = ConformanceScopeV1 { vectors_per_repetition: u32::MAX, ..s.clone() };
        assert!(scope_within_chain_bound_v1(&huge, &p).is_err());
        let long = ConformanceScopeV1 { decode_tokens: u32::MAX, ..s };
        assert!(scope_within_chain_bound_v1(&long, &p).is_err());
    }
}
