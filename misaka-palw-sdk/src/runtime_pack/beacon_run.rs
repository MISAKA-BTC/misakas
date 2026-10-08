//! **Beacon conformance of a committed pack** (RFC-0013 §9, RFC-0007 Part VI): from a commitment and the canonical beacon facts to
//! a `BeaconConformanceEvidenceV1`, and from evidence back to a verdict a fresh process can reach alone.
//!
//! ```text
//! committed pack ─ facts (loader boundary) ─ collect_work_beacon_v1 ─ WaitingRandomness | BEACON_UNAVAILABLE | Locked
//!   Locked ─ challenge_seed_v1 ─ ChallengeStreamV1 per (family, relation, repetition)
//!        ├─ vectors: prompts drawn from the stream, run on reference / independent / typed backend (lazy, streamed)
//!        └─ leaves:  artifact leaves drawn from the stream, opened against the artifact root in ONE streamed pass, then decoded by
//!                    each implementation's own tensor decoder
//!   → checks_required / run / failed / missing, six result roots → BeaconConformanceEvidenceV1
//! ```
//!
//! **What a PASS is and is not.** A pass is: every check the committed scope derives was drawn from the seed this node recomputed
//! from canonical history, was run on every required implementation, and agreed. It is a SAMPLED check with a conditional,
//! scope-derived error bound ([`ConformanceScopeV1::derived_epsilon_bits`]). It is not full-scope fidelity, not semantic admission,
//! not G14, not a statement about the beacon being unbiased, and — when the facts are `Synthetic` — not a statement about any chain.
//!
//! **Fail closed.** `Skipped`, `Incomplete`, a stale seed, `BEACON_UNAVAILABLE`, a changed artifact/layout/plan/policy/implementation
//! after the commitment, and any evidence that a fresh re-derivation does not reproduce byte for byte are never a pass.
//!
//! **Resume.** Public facts and per-check completion records are persisted atomically under the state directory; a process that
//! exits while waiting for randomness, or mid-checks, resumes from them. A record is reused only when its binding (seed, statement,
//! implementation set, check selection, enabled executors) matches and its own digest verifies; a new beacon (reorg) is a new seed
//! and starts a new directory — the old evidence is retained, marked invalidated, never overwritten.

use super::commit::{BoundCommitment, CommitParamsV1, ConformanceScopeV1, Refusal, bind_commitment, commitment_diff, hex, tool_root};
use super::conformance::{ConformanceJob, ImplOutcomeV1, ImplSet, TripleResult, TripleRunner, ref2_dtype};
use super::facts::{BeaconFactSource, ChainBeaconFactsV1, FactsProvenanceV1, resolve_facts};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::palw_artifact::{
    PalwArtifactMultiproofStreamV1, PalwArtifactMultiproofV1, PalwArtifactOperandV1, artifact_leaf_parts_v1,
    verify_artifact_multiproof_v1,
};
use kaspa_consensus_core::palw_tir_artifact_v1::{
    PALW_TIR_ROW_PIECE_BYTES_V1, PalwTirInventoryRowV1, palw_tir_inventory_leaf_count_v1, palw_tir_visit_inventory_rows_v1,
};
use misaka_palw_challenge::beacon::{WorkBeaconStateV1, WorkBeaconV1, collect_work_beacon_v1, verify_work_beacon_v1};
use misaka_palw_challenge::conformance::{
    BeaconConformanceEvidenceV1, ConformanceCommitmentV1, ConformanceRefusalV1, ConformanceStatusV1, verify_conformance_evidence_v1,
};
use misaka_palw_challenge::hash::{Digest, named_id};
use misaka_palw_challenge::lifecycle::OnboardingFailureV1;
use misaka_palw_challenge::seed::{ChallengeStreamV1, StreamKindV1, StreamLabelV1, challenge_seed_v1};
use misaka_palw_challenge::{PostCommitChallengePolicyV1, RootV1};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const DOMAIN_SOURCES: &[u8] = b"misaka.palw.runtime-pack.beacon-sources.v1";
pub const DOMAIN_VECTORS: &[u8] = b"misaka.palw.runtime-pack.selected-vectors.v1";
pub const DOMAIN_RANGES: &[u8] = b"misaka.palw.runtime-pack.selected-leaves.v1";
pub const DOMAIN_RESULTS: &[u8] = b"misaka.palw.runtime-pack.result-root.v1";
pub const DOMAIN_OPENINGS: &[u8] = b"misaka.palw.runtime-pack.openings-root.v1";
pub const DOMAIN_LOCATORS: &[u8] = b"misaka.palw.runtime-pack.material-locators.v1";
pub const DOMAIN_CHECK_BINDING: &[u8] = b"misaka.palw.runtime-pack.check-binding.v1";
pub const DOMAIN_CHECK_OUTCOME: &[u8] = b"misaka.palw.runtime-pack.check-outcome.v1";
pub const DOMAIN_VALUES: &[u8] = b"misaka.palw.runtime-pack.decoded-values.v1";

pub const LEDGER_SCHEMA_V1: &str = "misaka.palw.beacon-conformance-ledger.v1";

// ---------------------------------------------------------------------------------------------------------------------------
// Atomic files, the state directory and the ledger
// ---------------------------------------------------------------------------------------------------------------------------

/// Write a file so that a reader sees all of it or none of it (a temporary sibling, synced, renamed).
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut f = std::fs::File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    f.write_all(bytes).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The directory name of a commitment: the first 32 hex characters of its statement root.
pub fn commitment_dirname(statement_root: &Digest) -> String {
    hex(statement_root)[..32].to_string()
}

pub fn ledger_path(state: &Path) -> PathBuf {
    state.join("ledger.json")
}

fn read_ledger(state: &Path) -> Vec<Value> {
    std::fs::read_to_string(ledger_path(state))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("entries").and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

/// Append one event to the ledger (an atomic rewrite; the history is never edited).
pub fn ledger_append(state: &Path, statement_root: &Digest, candidate_id: &Digest, event: &str, detail: Value) -> Result<(), String> {
    let mut entries = read_ledger(state);
    entries.push(json!({
        "seq": entries.len() + 1,
        "unix": unix_now(),
        "statement_root": hex(statement_root),
        "candidate_id": hex(candidate_id),
        "event": event,
        "detail": detail,
    }));
    let doc = json!({ "schema": LEDGER_SCHEMA_V1, "entries": entries });
    write_atomic(&ledger_path(state), serde_json::to_string_pretty(&doc).unwrap_or_default().as_bytes())
}

/// Beacon-unavailable windows already recorded for this candidate (each counts against the policy's retry limit).
pub fn beacon_retries(state: &Path, candidate_id: &Digest) -> u32 {
    let c = hex(candidate_id);
    read_ledger(state).iter().filter(|e| e["event"] == "BEACON_UNAVAILABLE" && e["candidate_id"].as_str() == Some(c.as_str())).count()
        as u32
}

/// The commitment as persisted, re-checked against itself: the directory name, the policy and the scope must be the ones its
/// statement names.
#[derive(Clone, Debug)]
pub struct LoadedCommitment {
    pub dir: PathBuf,
    pub commitment: ConformanceCommitmentV1,
    pub policy: PostCommitChallengePolicyV1,
    pub params: CommitParamsV1,
}

/// Persist a bound commitment (its canonical Borsh bytes, the policy, the parameters it is re-derived from, and a human summary).
pub fn save_commitment(state: &Path, b: &BoundCommitment) -> Result<PathBuf, Refusal> {
    let root = b.commitment.statement_root();
    let dir = state.join(commitment_dirname(&root));
    let io = |e: String| Refusal::new("STATE_IO", e);
    write_atomic(&dir.join("commitment.borsh"), &borsh::to_vec(&b.commitment).map_err(|e| io(e.to_string()))?).map_err(io)?;
    write_atomic(&dir.join("policy.borsh"), &borsh::to_vec(&b.params.policy).map_err(|e| io(e.to_string()))?).map_err(io)?;
    write_atomic(&dir.join("params.borsh"), &borsh::to_vec(&b.params).map_err(|e| io(e.to_string()))?).map_err(io)?;
    write_atomic(&dir.join("summary.json"), serde_json::to_string_pretty(&commitment_summary(b)).unwrap_or_default().as_bytes())
        .map_err(io)?;
    ledger_append(
        state,
        &root,
        &b.commitment.candidate_id,
        "COMMITTED",
        json!({ "pack": b.pack_digest, "policy_label": b.params.policy_label, "retries_before": beacon_retries(state, &b.commitment.candidate_id) }),
    )
    .map_err(io)?;
    Ok(dir)
}

pub fn commitment_summary(b: &BoundCommitment) -> Value {
    let c = &b.commitment;
    let root = |r: &RootV1| match r {
        RootV1::Absent => Value::String("ABSENT".into()),
        RootV1::Present(d) => Value::String(hex(d)),
    };
    json!({
        "schema": "misaka.palw.conformance-commitment-record.v1",
        "note": "A tool record. The commitment's identity is the contract's statement_root over its canonical Borsh bytes (commitment.borsh); every value here is recomputed, never trusted.",
        "statement_root": hex(&c.statement_root()),
        "subject_kind": c.subject_kind.code(),
        "policy": { "id": hex(&c.challenge_policy_id), "label": b.params.policy_label, "approved": false,
                     "k": b.params.policy.work_count_k, "anchor_delay_slots": b.params.policy.anchor_delay_slots,
                     "beacon_window_slots": b.params.policy.beacon_window_slots, "settlement_depth_d": b.params.policy.settlement_depth_d,
                     "repetition_count": b.params.policy.repetition_count, "security_bits": b.params.policy.security_bits,
                     "retry_limit": b.params.policy.retry_limit },
        "chain_genesis": hex(&c.chain_genesis),
        "ruleset_id": hex(&c.ruleset_id),
        "candidate_id": hex(&c.candidate_id),
        "pack_digest": b.pack_digest,
        "roots": {
            "artifact_root": hex(&c.artifact_root), "program_root": hex(&c.program_root),
            "tokenizer_or_input_schema_root": root(&c.tokenizer_or_input_schema_root), "layout_root": hex(&c.layout_root),
            "verification_plan_root": hex(&c.verification_plan_root), "kernel_descriptor_id": hex(&c.kernel_descriptor_id),
            "constraint_root": root(&c.constraint_root), "implementation_set_root": hex(&c.implementation_set_root),
            "test_scope_root": hex(&c.test_scope_root), "calibration_id": root(&c.calibration_id),
            "input_and_state_binding_root": root(&c.input_and_state_binding_root), "resource_profile_id": hex(&c.resource_profile_id),
        },
        "static_admission": { "kernel": b.admission.kernel, "hypothetically_armed": b.admission.hypothetically_armed,
                               "shipped_schedule_outcome": b.admission.shipped_outcome, "plan_positions": b.admission.positions,
                               "plan_error_bits": b.admission.error_bits },
        "implementation_set": b.implementation_set.entries.iter().map(|e| json!({ "role": e.role, "crate": e.crate_name,
            "version": e.crate_version, "source_digest": e.source_digest })).collect::<Vec<_>>(),
        "scope": b.params.scope.statement(b.params.policy.repetition_count),
        "artifact": { "bytes": b.artifact_bytes, "leaves": b.leaf_count },
        "provenance": { "commitment_object_id": null, "canonical_commitment_position": null,
                        "note": "observed submission provenance: filled by the chain, outside the statement digest" },
    })
}

/// **Commit**: bind the pack into a commitment and persist it. Refused up front when the candidate has already used up the policy's
/// counted retries (each beacon-unavailable window is one), and when the committed scope cannot meet the policy (see `bind_commitment`).
pub fn commit_conformance(
    pack_dir: &Path,
    artifact: &Path,
    state: &Path,
    params: &CommitParamsV1,
    log: &dyn Fn(String),
) -> Result<(BoundCommitment, PathBuf), Refusal> {
    let bound = bind_commitment(pack_dir, artifact, params, log)?;
    let unavailable = beacon_retries(state, &bound.commitment.candidate_id);
    if unavailable > params.policy.retry_limit {
        return Err(Refusal::new(
            "RETRY_LIMIT_EXHAUSTED",
            format!(
                "{unavailable} beacon windows closed short for this candidate; the policy allows {} retries. Each retry is a new commitment and a new window, counted",
                params.policy.retry_limit
            ),
        ));
    }
    let dir = save_commitment(state, &bound)?;
    Ok((bound, dir))
}

/// Find the commitment directory for a (possibly abbreviated) statement root.
pub fn resolve_commitment_dir(state: &Path, prefix: &str) -> Result<PathBuf, Refusal> {
    let prefix = prefix.to_ascii_lowercase();
    if prefix.len() < 8 || !prefix.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Refusal::new("COMMITMENT_UNKNOWN", format!("`{prefix}` is not a hex prefix of at least 8 characters")));
    }
    let mut hits = Vec::new();
    if let Ok(rd) = std::fs::read_dir(state) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if e.path().is_dir() && name.len() == 32 && (name.starts_with(&prefix) || prefix.starts_with(&name)) {
                hits.push(e.path());
            }
        }
    }
    match hits.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Refusal::new("COMMITMENT_UNKNOWN", format!("no commitment `{prefix}` in {}", state.display()))),
        _ => Err(Refusal::new("COMMITMENT_UNKNOWN", format!("`{prefix}` names more than one commitment"))),
    }
}

pub fn load_commitment(dir: &Path) -> Result<LoadedCommitment, Refusal> {
    let read =
        |name: &str| std::fs::read(dir.join(name)).map_err(|e| Refusal::new("STATE_IO", format!("{}: {e}", dir.join(name).display())));
    let commitment = ConformanceCommitmentV1::try_from_slice(&read("commitment.borsh")?)
        .map_err(|e| Refusal::new("COMMITMENT_INVALID", format!("commitment.borsh: {e}")))?;
    let policy = PostCommitChallengePolicyV1::try_from_slice(&read("policy.borsh")?)
        .map_err(|e| Refusal::new("POLICY_INVALID", format!("policy.borsh: {e}")))?;
    let params = CommitParamsV1::try_from_slice(&read("params.borsh")?)
        .map_err(|e| Refusal::new("COMMITMENT_INVALID", format!("params.borsh: {e}")))?;
    commitment.well_formed().map_err(|e| Refusal::new("COMMITMENT_INVALID", e))?;
    let root = commitment.statement_root();
    if dir.file_name().and_then(|n| n.to_str()) != Some(commitment_dirname(&root).as_str()) {
        return Err(Refusal::new(
            "COMMITMENT_INVALID",
            "the commitment's statement root is not its directory name (renamed or edited)",
        ));
    }
    policy.validate().map_err(|e| Refusal::new("POLICY_INVALID", e.to_string()))?;
    if policy.id() != commitment.challenge_policy_id || params.policy != policy {
        return Err(Refusal::new("POLICY_SUBSTITUTED", "the stored policy is not the one the commitment names"));
    }
    if params.scope.root() != commitment.test_scope_root {
        return Err(Refusal::new("COMMITMENT_STALE", "the stored scope is not the committed test_scope_root"));
    }
    Ok(LoadedCommitment { dir: dir.to_path_buf(), commitment, policy, params })
}

/// Re-derive the commitment from the pack and the artifact it was made from; anything that moved since is a different commitment.
pub fn rebind(loaded: &LoadedCommitment, pack_dir: &Path, artifact: &Path, log: &dyn Fn(String)) -> Result<BoundCommitment, Refusal> {
    let bound = bind_commitment(pack_dir, artifact, &loaded.params, log).map_err(|e| {
        Refusal::new("COMMITMENT_STALE", format!("the commitment can no longer be derived from this pack/artifact — {e}"))
    })?;
    if bound.commitment.statement_root() != loaded.commitment.statement_root() {
        let fields = commitment_diff(&bound.commitment, &loaded.commitment);
        return Err(Refusal::new(
            "COMMITMENT_STALE",
            format!(
                "changed since the commitment: {} — a changed artifact, layout, plan, kernel, policy, implementation or scope needs a NEW pre-beacon commitment",
                fields.join(", ")
            ),
        ));
    }
    Ok(bound)
}

// ---------------------------------------------------------------------------------------------------------------------------
// Selection: what the seed picks
// ---------------------------------------------------------------------------------------------------------------------------

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

fn sample_refusal(e: impl std::fmt::Display) -> Refusal {
    Refusal::new("SAMPLER_EXHAUSTED", e.to_string())
}

/// **The checks a seed selects**, from the contract's streams only: one labelled stream per (family, relation, repetition).
pub fn derive_selection(
    seed: &Digest,
    policy: &PostCommitChallengePolicyV1,
    scope: &ConformanceScopeV1,
    program: &misaka_palw_tir::TirProgramV1,
) -> Result<SelectionV1, Refusal> {
    let vector_scope = named_id("pack-conformance/vector/v1");
    let leaf_scope = named_id("pack-conformance/artifact-leaf/v1");
    let leaf_count = palw_tir_inventory_leaf_count_v1(program).map_err(|e| Refusal::new("SCOPE_INVALID", e.to_string()))?;
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
    .map_err(|e| Refusal::new("SCOPE_INVALID", e.to_string()))?;
    let mut leaves = Vec::new();
    for (r, k, idx) in raw_leaves {
        let row = coords.get(&idx).ok_or_else(|| Refusal::new("SCOPE_INVALID", format!("leaf {idx} has no coordinates")))?;
        leaves.push(SelectedLeafV1 {
            check_id: format!("leaf/r{r}/k{k}"),
            repetition: r,
            ordinal: k,
            leaf_index: idx,
            param: row.param,
            layer: row.layer,
            row_start: row.row_start,
            len: row.len,
            tensor_name: program.params[row.param as usize].name.clone(),
        });
    }
    Ok(SelectionV1 { vectors, leaves })
}

// ---------------------------------------------------------------------------------------------------------------------------
// Authenticated openings: one streamed pass
// ---------------------------------------------------------------------------------------------------------------------------

/// **The drawn leaves, opened against the artifact root in one streamed pass.** Every leaf of the inventory is read and hashed once
/// (32 KiB at a time, through the container's range reader), only the drawn leaves' bytes and `O(k log n)` hashes are kept, and
/// the multiproof is verified against the committed root before any byte of it is used. This is a full read of the artifact —
/// the honest price of a Merkle opening without a stored tree; it is recorded as hashed bytes, not as "checked sample bytes".
pub fn authenticated_openings(
    artifact: &Path,
    leaves: &[SelectedLeafV1],
    artifact_root: &Digest,
) -> Result<(PalwArtifactMultiproofV1, u64), Refusal> {
    use crate::tir_stream::{ContainerRanges, PalwTirRangeSourceV1};
    let fail = |e: String| Refusal::new("ARTIFACT_OPENING_FAILED", e);
    let c = misaka_palw_tir_artifact::PalwTirContainerV1::open(artifact).map_err(|e| fail(e.to_string()))?;
    let count = palw_tir_inventory_leaf_count_v1(&c.program).map_err(|e| fail(e.to_string()))?;
    let draw: Vec<u32> = leaves.iter().map(|l| l.leaf_index).collect::<BTreeSet<_>>().into_iter().collect();
    let mut stream =
        PalwArtifactMultiproofStreamV1::new(count, &draw).ok_or_else(|| fail("a drawn leaf is outside the inventory".into()))?;
    let ranges = ContainerRanges::open(&c).map_err(fail)?;
    let mut buf = vec![0u8; PALW_TIR_ROW_PIECE_BYTES_V1 as usize];
    let want: BTreeSet<u32> = draw.iter().copied().collect();
    let mut opened: Vec<(u32, PalwArtifactOperandV1)> = Vec::new();
    let mut at = 0u32;
    let mut hashed = 0u64;
    let mut failure: Option<String> = None;
    palw_tir_visit_inventory_rows_v1(&c.program, &mut |row: PalwTirInventoryRowV1| {
        if failure.is_some() {
            return;
        }
        let bytes = &mut buf[..row.len as usize];
        match ranges.read_range(row.param, row.layer, row.row_start as u64..row.row_start as u64 + row.len as u64, bytes) {
            Ok(()) => {
                let name = &c.program.params[row.param as usize].name;
                stream.push(artifact_leaf_parts_v1(name, row.layer, row.row_start, bytes));
                hashed += row.len as u64;
                if want.contains(&at) {
                    opened.push((
                        at,
                        PalwArtifactOperandV1 {
                            tensor_name: name.clone(),
                            layer: row.layer,
                            row_start: row.row_start,
                            bytes: bytes.to_vec(),
                        },
                    ));
                }
            }
            Err(e) => failure = Some(e),
        }
        at += 1;
    })
    .map_err(|e| fail(e.to_string()))?;
    if let Some(e) = failure {
        return Err(fail(e));
    }
    let proof = stream.finish(&opened).ok_or_else(|| fail("the streamed multiproof could not be assembled".into()))?;
    verify_artifact_multiproof_v1(&proof, kaspa_hashes::Hash64::from_bytes(*artifact_root))
        .map_err(|e| fail(format!("the openings do not reconstruct the committed artifact root: {e}")))?;
    Ok((proof, hashed))
}

// ---------------------------------------------------------------------------------------------------------------------------
// Check outcomes
// ---------------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum ResultV1 {
    NotRun,
    Error(String),
    Ran { a: [u8; 32], b: [u8; 32], tokens: Vec<u32>, positions: u32 },
}

impl From<&ImplOutcomeV1> for ResultV1 {
    fn from(o: &ImplOutcomeV1) -> Self {
        match o {
            ImplOutcomeV1::NotRun => ResultV1::NotRun,
            ImplOutcomeV1::Error(e) => ResultV1::Error(e.clone()),
            ImplOutcomeV1::Ran(r) => {
                ResultV1::Ran { a: r.logits_digest, b: r.commits_digest, tokens: r.tokens.clone(), positions: r.positions }
            }
        }
    }
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

    fn result(&self, role: Role) -> &ResultV1 {
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

/// Test-only: corrupt one implementation's recorded result for one check, to see the failure path end to end.
#[derive(Clone, Debug)]
pub struct InjectedFault {
    pub check_id: String,
    pub role: Role,
}

fn values_digest(values: &[i128]) -> [u8; 32] {
    let mut st = blake2b_simd::Params::new().hash_length(32).key(DOMAIN_VALUES).to_state();
    st.update(&(values.len() as u64).to_le_bytes());
    for v in values {
        st.update(&v.to_le_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(st.finalize().as_bytes());
    out
}

/// The leaf's bytes as every enabled implementation's own decoder reads them.
fn leaf_outcome(leaf: &SelectedLeafV1, bytes: &[u8], program: &misaka_palw_tir::TirProgramV1, impls: ImplSet) -> CheckOutcomeV1 {
    let dtype = program.params[leaf.param as usize].dtype;
    let w = dtype.width();
    let mut disagreement = None;
    let n = bytes.len() / w;
    let reference = if bytes.len() % w != 0 {
        disagreement = Some(format!("leaf {}: {} bytes are not whole {} elements", leaf.leaf_index, bytes.len(), dtype.name()));
        ResultV1::Error("the leaf is not whole elements".into())
    } else {
        match misaka_palw_tir::Tensor::from_le_bytes(dtype, &[n], bytes) {
            Ok(t) => ResultV1::Ran { a: values_digest(&t.data), b: [0; 32], tokens: vec![], positions: n as u32 },
            Err(e) => ResultV1::Error(e.to_string()),
        }
    };
    let independent = if !impls.ref2 {
        ResultV1::NotRun
    } else {
        match misaka_palw_tir_ref2::Tensor::from_le_bytes(ref2_dtype(dtype), vec![n as u64], bytes) {
            Ok(t) => ResultV1::Ran { a: values_digest(&t.data), b: [0; 32], tokens: vec![], positions: n as u32 },
            Err(e) => ResultV1::Error(format!("{e:?}")),
        }
    };
    let backend = if !impls.exec {
        ResultV1::NotRun
    } else {
        match misaka_palw_tir_exec::ParamData::from_le_bytes(dtype, bytes) {
            Ok(d) => ResultV1::Ran { a: values_digest(&d.slice().to_i128s()), b: [0; 32], tokens: vec![], positions: n as u32 },
            Err(e) => ResultV1::Error(e.to_string()),
        }
    };
    CheckOutcomeV1 { check_id: leaf.check_id.clone(), reference, independent, backend, disagreement }
}

fn vector_outcome(v: &SelectedVectorV1, t: &TripleResult) -> CheckOutcomeV1 {
    CheckOutcomeV1 {
        check_id: v.check_id.clone(),
        reference: (&t.reference).into(),
        independent: (&t.independent).into(),
        backend: (&t.backend).into(),
        disagreement: t.disagreement.clone(),
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Evidence assembly
// ---------------------------------------------------------------------------------------------------------------------------

/// **Assemble the evidence from the checks that ran** — a pure function of (commitment, policy, scope, beacon, seed, selection,
/// openings, outcomes). Used by the producer and, from scratch, by the verifier: evidence is accepted only if the verifier's own
/// assembly equals it exactly.
#[allow(clippy::too_many_arguments)]
pub fn assemble_evidence(
    commitment: &ConformanceCommitmentV1,
    policy: &PostCommitChallengePolicyV1,
    scope: &ConformanceScopeV1,
    beacon: &WorkBeaconV1,
    seed: &Digest,
    selection: &SelectionV1,
    proof: &Option<PalwArtifactMultiproofV1>,
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
        authenticated_openings_root: tool_root(DOMAIN_OPENINGS, proof),
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
        public_material_locator_root: tool_root(
            DOMAIN_LOCATORS,
            &(selection_digest, tool_root(DOMAIN_OPENINGS, proof), outcome_digests),
        ),
    }
}

/// The evidence fields that differ between two evidences (names only).
pub fn evidence_diff(a: &BeaconConformanceEvidenceV1, b: &BeaconConformanceEvidenceV1) -> Vec<&'static str> {
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

pub fn status_code(s: ConformanceStatusV1) -> &'static str {
    match s {
        ConformanceStatusV1::Passed => "PASSED",
        ConformanceStatusV1::Failed => "FAILED",
        ConformanceStatusV1::Skipped => "SKIPPED",
        ConformanceStatusV1::Incomplete => "INCOMPLETE",
        ConformanceStatusV1::BeaconUnavailable => "BEACON_UNAVAILABLE",
    }
}

pub fn evidence_json(ev: &BeaconConformanceEvidenceV1) -> Value {
    let root = |r: &RootV1| match r {
        RootV1::Absent => Value::String("ABSENT".into()),
        RootV1::Present(d) => Value::String(hex(d)),
    };
    json!({
        "schema": "misaka.palw.beacon-conformance-evidence-record.v1",
        "note": "A tool record. The evidence's identity is the contract's BeaconConformanceEvidenceV1::id() over evidence.borsh; a verifier re-derives every field, it trusts none of these.",
        "evidence_id": hex(&ev.id()),
        "status": status_code(ev.status),
        "commitment_root": hex(&ev.commitment_root),
        "challenge_policy_id": hex(&ev.challenge_policy_id),
        "challenge_anchor": hex(&ev.challenge_anchor),
        "beacon_output": hex(&ev.beacon_output),
        "challenge_seed": hex(&ev.challenge_seed),
        "lock_position": ev.lock_position,
        "qualifying_source_evidence_root": hex(&ev.qualifying_source_evidence_root),
        "selected_vectors_root": hex(&ev.selected_vectors_root),
        "selected_tensor_ranges_root": hex(&ev.selected_tensor_ranges_root),
        "reference_result_root": hex(&ev.reference_result_root),
        "independent_result_root": hex(&ev.independent_result_root),
        "backend_result_root": hex(&ev.backend_result_root),
        "authenticated_openings_root": hex(&ev.authenticated_openings_root),
        "transcript_root": root(&ev.transcript_root),
        "checks": { "required": ev.checks_required, "run": ev.checks_run, "failed": ev.checks_failed },
        "missing_checks": ev.missing_checks,
        "failures": ev.failures,
        "scope_and_fault_model_id": hex(&ev.scope_and_fault_model_id),
        "derived_epsilon_bits": ev.derived_epsilon_bits,
        "public_material_locator_root": hex(&ev.public_material_locator_root),
    })
}

// ---------------------------------------------------------------------------------------------------------------------------
// The engine: select, open, check, assemble
// ---------------------------------------------------------------------------------------------------------------------------

/// What a run measured. Never part of the evidence (it depends on the machine).
#[derive(Clone, Debug, Default)]
pub struct Measures {
    pub open_pass_ms: u64,
    pub open_pass_hashed_bytes: u64,
    pub leaf_checks_ms: u64,
    pub leaf_bytes_compared: u64,
    pub vector_checks_ms: u64,
    pub vector_positions: u64,
    pub checks_executed: u64,
    pub checks_reused: u64,
    pub wall_ms: u64,
    pub peak_rss_bytes: u64,
}

impl Measures {
    pub fn to_json(&self) -> Value {
        json!({
            "open_pass_ms": self.open_pass_ms, "open_pass_hashed_bytes": self.open_pass_hashed_bytes,
            "leaf_checks_ms": self.leaf_checks_ms, "leaf_bytes_compared": self.leaf_bytes_compared,
            "vector_checks_ms": self.vector_checks_ms, "vector_positions": self.vector_positions,
            "checks_executed": self.checks_executed, "checks_reused": self.checks_reused,
            "wall_ms": self.wall_ms, "peak_rss_bytes": self.peak_rss_bytes,
            "note": "peak RSS is the process's lifetime maximum (getrusage), mapped file pages included; hashed bytes are the authentication read of the whole artifact, compared bytes are the sampled leaves",
        })
    }
}

pub fn peak_rss_bytes() -> u64 {
    // SAFETY: `getrusage` writes into the zeroed struct we pass and nothing else.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
            return 0;
        }
        ru
    };
    let v = ru.ru_maxrss as u64;
    if cfg!(target_os = "linux") { v * 1024 } else { v }
}

pub struct Engine<'a> {
    pub commitment: &'a ConformanceCommitmentV1,
    pub policy: &'a PostCommitChallengePolicyV1,
    pub scope: &'a ConformanceScopeV1,
    pub beacon: &'a WorkBeaconV1,
    pub seed: Digest,
    pub artifact: &'a Path,
    pub impls: ImplSet,
    /// Stop after this many NEWLY executed checks (an interruption, or a time budget): the rest stay missing, honestly.
    pub max_checks: Option<usize>,
    pub fault: Option<&'a InjectedFault>,
    /// Where completion records live (`<seed dir>`); `None`: nothing is persisted or reused (a verifier).
    pub store: Option<&'a Path>,
}

pub struct Computed {
    pub selection: SelectionV1,
    pub proof: Option<PalwArtifactMultiproofV1>,
    pub outcomes: BTreeMap<String, CheckOutcomeV1>,
    pub evidence: BeaconConformanceEvidenceV1,
    pub measures: Measures,
}

fn check_binding(e: &Engine<'_>, id: &str, item: &Digest) -> Digest {
    tool_root(
        DOMAIN_CHECK_BINDING,
        &(
            e.commitment.statement_root(),
            e.seed,
            e.commitment.implementation_set_root,
            id.to_string(),
            *item,
            e.impls.exec,
            e.impls.ref2,
        ),
    )
}

fn read_record(path: &Path, binding: &Digest) -> Option<CheckOutcomeV1> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if v["binding"].as_str()? != hex(binding) {
        return None;
    }
    let raw = v["outcome_borsh"].as_str()?;
    let bytes: Vec<u8> = (0..raw.len() / 2).map(|i| u8::from_str_radix(&raw[2 * i..2 * i + 2], 16)).collect::<Result<_, _>>().ok()?;
    let o = CheckOutcomeV1::try_from_slice(&bytes).ok()?;
    (hex(&o.digest()) == v["outcome_digest"].as_str()?).then_some(o)
}

fn write_record(path: &Path, binding: &Digest, o: &CheckOutcomeV1, wall_ms: u64) -> Result<(), String> {
    let borsh_hex = hex(&borsh::to_vec(o).map_err(|e| e.to_string())?);
    let rec = json!({
        "schema": "misaka.palw.conformance-check-record.v1",
        "check_id": o.check_id, "binding": hex(binding), "outcome_digest": hex(&o.digest()), "outcome_borsh": borsh_hex,
        "summary": { "reference": format!("{:?}", short_result(&o.reference)), "independent": format!("{:?}", short_result(&o.independent)),
                     "backend": format!("{:?}", short_result(&o.backend)), "disagreement": o.disagreement },
        "wall_ms": wall_ms,
    });
    write_atomic(path, serde_json::to_string_pretty(&rec).unwrap_or_default().as_bytes())
}

fn short_result(r: &ResultV1) -> String {
    match r {
        ResultV1::NotRun => "not-run".into(),
        ResultV1::Error(e) => format!("error: {e}"),
        ResultV1::Ran { a, positions, .. } => format!("ran {positions} position(s)/element(s), digest {}", &hex(a)[..16]),
    }
}

fn corrupt(o: &mut CheckOutcomeV1, role: Role) {
    let r = match role {
        Role::Reference => &mut o.reference,
        Role::Independent => &mut o.independent,
        Role::Backend => &mut o.backend,
    };
    if let ResultV1::Ran { a, .. } = r {
        a[0] ^= 1;
    }
}

impl Engine<'_> {
    /// Select, open, check and assemble. Persists nothing unless a store is given; with one, every check is an atomic record.
    pub fn compute(&self, log: &dyn Fn(String)) -> Result<Computed, Refusal> {
        let t0 = Instant::now();
        let container = misaka_palw_tir_artifact::PalwTirContainerV1::open(self.artifact)
            .map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", format!("{}: {e}", self.artifact.display())))?;
        let program = &container.program;
        let selection = derive_selection(&self.seed, self.policy, self.scope, program)?;
        let mut m = Measures::default();
        if let Some(dir) = self.store {
            write_atomic(
                &dir.join("selection.borsh"),
                &borsh::to_vec(&selection).map_err(|e| Refusal::new("STATE_IO", e.to_string()))?,
            )
            .map_err(|e| Refusal::new("STATE_IO", e))?;
        }

        // Authenticated openings of the drawn leaves: reused from the store when they still verify against the committed root.
        let mut proof: Option<PalwArtifactMultiproofV1> = None;
        if !selection.leaves.is_empty() {
            let cached = self
                .store
                .and_then(|d| std::fs::read(d.join("multiproof.borsh")).ok())
                .and_then(|b| PalwArtifactMultiproofV1::try_from_slice(&b).ok());
            let want: Vec<u32> = selection.leaves.iter().map(|l| l.leaf_index).collect::<BTreeSet<_>>().into_iter().collect();
            let reusable = cached.filter(|p| {
                p.opened_indices() == want
                    && verify_artifact_multiproof_v1(p, kaspa_hashes::Hash64::from_bytes(self.commitment.artifact_root)).is_ok()
            });
            proof = Some(match reusable {
                Some(p) => p,
                None => {
                    log(format!("opening {} leaf(s) against the artifact root (one streamed pass over the artifact)", want.len()));
                    let t = Instant::now();
                    let (p, hashed) = authenticated_openings(self.artifact, &selection.leaves, &self.commitment.artifact_root)?;
                    m.open_pass_ms = t.elapsed().as_millis() as u64;
                    m.open_pass_hashed_bytes = hashed;
                    if let Some(dir) = self.store {
                        write_atomic(
                            &dir.join("multiproof.borsh"),
                            &borsh::to_vec(&p).map_err(|e| Refusal::new("STATE_IO", e.to_string()))?,
                        )
                        .map_err(|e| Refusal::new("STATE_IO", e))?;
                    }
                    p
                }
            });
        }

        let mut outcomes: BTreeMap<String, CheckOutcomeV1> = BTreeMap::new();
        let mut executed = 0usize;
        let budget = self.max_checks.unwrap_or(usize::MAX);
        let record_path = |id: &str| self.store.map(|d| d.join("checks").join(format!("{}.json", id.replace('/', "_"))));

        // Leaves first (cheap), then vectors.
        for leaf in &selection.leaves {
            let item = tool_root(b"misaka.palw.runtime-pack.check-item.v1", leaf);
            let binding = check_binding(self, &leaf.check_id, &item);
            if let Some(o) = record_path(&leaf.check_id).and_then(|p| read_record(&p, &binding)) {
                outcomes.insert(leaf.check_id.clone(), o);
                m.checks_reused += 1;
                continue;
            }
            if executed >= budget {
                continue;
            }
            let t = Instant::now();
            let p = proof.as_ref().expect("leaf checks have openings");
            let operand = p
                .opened
                .iter()
                .find(|(i, _)| *i == leaf.leaf_index)
                .map(|(_, o)| o)
                .ok_or_else(|| Refusal::new("ARTIFACT_OPENING_FAILED", format!("leaf {} was not opened", leaf.leaf_index)))?;
            let mut o = leaf_outcome(leaf, &operand.bytes, program, self.impls);
            if let Some(f) = self.fault.filter(|f| f.check_id == leaf.check_id) {
                corrupt(&mut o, f.role);
            }
            let ms = t.elapsed().as_millis() as u64;
            m.leaf_checks_ms += ms;
            m.leaf_bytes_compared += operand.bytes.len() as u64;
            if let Some(p) = record_path(&leaf.check_id) {
                write_record(&p, &binding, &o, ms).map_err(|e| Refusal::new("STATE_IO", e))?;
            }
            outcomes.insert(leaf.check_id.clone(), o);
            executed += 1;
            m.checks_executed += 1;
        }
        let mut runner: Option<TripleRunner> = None;
        for v in &selection.vectors {
            let item = tool_root(b"misaka.palw.runtime-pack.check-item.v1", v);
            let binding = check_binding(self, &v.check_id, &item);
            if let Some(o) = record_path(&v.check_id).and_then(|p| read_record(&p, &binding)) {
                outcomes.insert(v.check_id.clone(), o);
                m.checks_reused += 1;
                continue;
            }
            if executed >= budget {
                continue;
            }
            if runner.is_none() {
                runner = Some(TripleRunner::open(self.artifact, self.impls).map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e))?);
            }
            log(format!("{}: prompt {:?} + {} decoded token(s) on every required implementation", v.check_id, v.prompt, v.decode));
            let t = Instant::now();
            let job = ConformanceJob {
                label: v.check_id.clone(),
                prompt: v.prompt.iter().map(|x| *x as usize).collect(),
                decode: v.decode as usize,
            };
            let triple = runner.as_ref().expect("opened above").run_job(&job).map_err(|e| Refusal::new("CHECK_REFUSED", e))?;
            let mut o = vector_outcome(v, &triple);
            if let Some(f) = self.fault.filter(|f| f.check_id == v.check_id) {
                corrupt(&mut o, f.role);
            }
            let ms = t.elapsed().as_millis() as u64;
            m.vector_checks_ms += ms;
            m.vector_positions += (v.prompt.len() + v.decode as usize) as u64;
            if let Some(p) = record_path(&v.check_id) {
                write_record(&p, &binding, &o, ms).map_err(|e| Refusal::new("STATE_IO", e))?;
            }
            outcomes.insert(v.check_id.clone(), o);
            executed += 1;
            m.checks_executed += 1;
        }
        let evidence =
            assemble_evidence(self.commitment, self.policy, self.scope, self.beacon, &self.seed, &selection, &proof, &outcomes);
        m.wall_ms = t0.elapsed().as_millis() as u64;
        m.peak_rss_bytes = peak_rss_bytes();
        Ok(Computed { selection, proof, outcomes, evidence, measures: m })
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Run: commitment + facts -> beacon -> evidence
// ---------------------------------------------------------------------------------------------------------------------------

pub struct RunInput<'a> {
    pub pack_dir: &'a Path,
    pub artifact: &'a Path,
    pub state_dir: &'a Path,
    /// A (possibly abbreviated) statement root.
    pub commitment: &'a str,
    pub source: &'a dyn BeaconFactSource,
    pub impls: ImplSet,
    pub max_checks: Option<usize>,
    pub fault: Option<InjectedFault>,
}

#[derive(Debug)]
pub enum RunOutcome {
    /// `WaitingRandomness`: fewer than `k` sources so far, or `k` sources whose last is not yet at depth `D`. Nothing was run.
    Waiting { have: u32, need: u32, lock_position: Option<u64>, tip: u64 },
    /// `BEACON_UNAVAILABLE`: the window closed with fewer than `k`. Not fraud, not a pass, no fallback randomness.
    Unavailable { have: u32, need: u32, tip: u64, retries: u32, retry_limit: u32 },
    /// Some checks ran and were recorded; the rest are pending. Resume with the same inputs.
    Interrupted { done: usize, total: usize, seed: Digest },
    Evidence {
        evidence: BeaconConformanceEvidenceV1,
        dir: PathBuf,
        seed: Digest,
        /// The producer's own judgement of its evidence; a fresh `verify` is what counts.
        local: Result<(), ConformanceRefusalV1>,
        measures: Measures,
        provenance: FactsProvenanceV1,
    },
}

fn beacon_state_json(s: &WorkBeaconStateV1) -> Value {
    match s {
        WorkBeaconStateV1::Collecting { have, need } => json!({ "state": "COLLECTING", "have": have, "need": need }),
        WorkBeaconStateV1::Candidate { have, lock_position } => {
            json!({ "state": "CANDIDATE", "have": have, "lock_position": lock_position })
        }
        WorkBeaconStateV1::Locked(b) => json!({ "state": "LOCKED", "lock_position": b.lock_position }),
        WorkBeaconStateV1::Unavailable { have, need } => json!({ "state": "UNAVAILABLE", "have": have, "need": need }),
    }
}

/// Run (or resume) the conformance of a committed pack against the canonical facts the source supplies.
pub fn run_conformance(inp: &RunInput<'_>, log: &dyn Fn(String)) -> Result<RunOutcome, Refusal> {
    let loaded = load_commitment(&resolve_commitment_dir(inp.state_dir, inp.commitment)?)?;
    let io = |e: String| Refusal::new("STATE_IO", e);
    // The commitment must still be derivable from THIS pack and THIS artifact, bit for bit, before anything is run for it.
    rebind(&loaded, inp.pack_dir, inp.artifact, log)?;
    let root = loaded.commitment.statement_root();
    let cand = loaded.commitment.candidate_id;

    let facts = inp.source.facts(&loaded.commitment, &loaded.policy)?;
    let ctx = resolve_facts(&loaded.commitment, &loaded.policy, &facts)?;
    let state =
        collect_work_beacon_v1(&ctx, &facts.events, facts.tip_position).map_err(|e| Refusal::new("POLICY_INVALID", e.to_string()))?;
    let beacon = match state {
        WorkBeaconStateV1::Collecting { have, need } => {
            ledger_append(
                inp.state_dir,
                &root,
                &cand,
                "WAITING_RANDOMNESS",
                json!({ "beacon": beacon_state_json(&state), "tip": facts.tip_position }),
            )
            .map_err(io)?;
            return Ok(RunOutcome::Waiting { have, need, lock_position: None, tip: facts.tip_position });
        }
        WorkBeaconStateV1::Candidate { have, lock_position } => {
            ledger_append(
                inp.state_dir,
                &root,
                &cand,
                "WAITING_RANDOMNESS",
                json!({ "beacon": beacon_state_json(&state), "tip": facts.tip_position }),
            )
            .map_err(io)?;
            return Ok(RunOutcome::Waiting {
                have,
                need: loaded.policy.work_count_k,
                lock_position: Some(lock_position),
                tip: facts.tip_position,
            });
        }
        WorkBeaconStateV1::Unavailable { have, need } => {
            ledger_append(
                inp.state_dir,
                &root,
                &cand,
                "BEACON_UNAVAILABLE",
                json!({ "have": have, "need": need, "tip": facts.tip_position, "facts": hex(&facts.digest()), "code": OnboardingFailureV1::BeaconUnavailable.code() }),
            )
            .map_err(io)?;
            return Ok(RunOutcome::Unavailable {
                have,
                need,
                tip: facts.tip_position,
                retries: beacon_retries(inp.state_dir, &cand),
                retry_limit: loaded.policy.retry_limit,
            });
        }
        WorkBeaconStateV1::Locked(b) => b,
    };
    // A beacon this node derived must satisfy the contract's own presented-beacon check (a self-test of the derivation).
    verify_work_beacon_v1(&ctx, &beacon, &facts.events, facts.tip_position)
        .map_err(|e| Refusal::new("BEACON_NOT_CANONICAL", e.to_string()))?;
    let seed =
        challenge_seed_v1(&ctx, &loaded.commitment.subject(), &beacon).map_err(|e| Refusal::new("SEED_REFUSED", e.to_string()))?;
    let seed_name = format!("seed-{}", &hex(&seed)[..32]);
    let seed_dir = loaded.dir.join(&seed_name);

    // A different beacon for the same commitment is a different history: the old evidence is retained and marked invalidated.
    if let Ok(rd) = std::fs::read_dir(&loaded.dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("seed-") && n != seed_name && e.path().is_dir() && !e.path().join("INVALIDATED").exists() {
                let why = format!("the canonical beacon changed (reorg or another branch): new seed {seed_name}");
                write_atomic(&e.path().join("INVALIDATED"), why.as_bytes()).map_err(io)?;
                ledger_append(inp.state_dir, &root, &cand, "INVALIDATED", json!({ "seed_dir": n, "reason": why })).map_err(io)?;
            }
        }
    }
    let fresh = !seed_dir.exists();
    std::fs::create_dir_all(seed_dir.join("checks")).map_err(|e| io(e.to_string()))?;
    // The branch came back (a reorg undone): this seed is canonical again. Its evidence was never deleted; only the advisory mark goes.
    if seed_dir.join("INVALIDATED").exists() {
        std::fs::remove_file(seed_dir.join("INVALIDATED")).map_err(|e| io(e.to_string()))?;
        ledger_append(
            inp.state_dir,
            &root,
            &cand,
            "REVALIDATED",
            json!({ "seed_dir": seed_name, "reason": "the canonical beacon is this seed's again" }),
        )
        .map_err(io)?;
    }
    if fresh {
        ledger_append(
            inp.state_dir,
            &root,
            &cand,
            "CHALLENGE_RESOLVED",
            json!({ "seed": hex(&seed), "lock_position": beacon.lock_position, "sources": beacon.sources.len(), "provenance": facts.provenance.label() }),
        )
        .map_err(io)?;
    }
    write_atomic(&seed_dir.join("beacon.borsh"), &borsh::to_vec(beacon.beacon()).map_err(|e| io(e.to_string()))?).map_err(io)?;
    write_atomic(&seed_dir.join("facts.borsh"), &borsh::to_vec(&facts).map_err(|e| io(e.to_string()))?).map_err(io)?;
    write_atomic(
        &seed_dir.join("facts.json"),
        serde_json::to_string_pretty(&super::facts::facts_to_json(&facts, &loaded.policy.id())).unwrap_or_default().as_bytes(),
    )
    .map_err(io)?;

    let engine = Engine {
        commitment: &loaded.commitment,
        policy: &loaded.policy,
        scope: &loaded.params.scope,
        beacon: &beacon,
        seed,
        artifact: inp.artifact,
        impls: inp.impls,
        max_checks: inp.max_checks,
        fault: inp.fault.as_ref(),
        store: Some(&seed_dir),
    };
    let t0 = Instant::now();
    let c = engine.compute(log)?;
    let ev = &c.evidence;
    let total = c.selection.check_ids().len();
    let done = c.outcomes.len();
    if done < total && ev.status == ConformanceStatusV1::Incomplete {
        ledger_append(inp.state_dir, &root, &cand, "INTERRUPTED", json!({ "seed": hex(&seed), "done": done, "total": total }))
            .map_err(io)?;
        write_atomic(
            &seed_dir.join("run.json"),
            serde_json::to_string_pretty(
                &json!({ "state": "INTERRUPTED", "done": done, "total": total, "measures": c.measures.to_json() }),
            )
            .unwrap_or_default()
            .as_bytes(),
        )
        .map_err(io)?;
        return Ok(RunOutcome::Interrupted { done, total, seed });
    }
    write_atomic(&seed_dir.join("evidence.borsh"), &borsh::to_vec(ev).map_err(|e| io(e.to_string()))?).map_err(io)?;
    write_atomic(&seed_dir.join("evidence.json"), serde_json::to_string_pretty(&evidence_json(ev)).unwrap_or_default().as_bytes())
        .map_err(io)?;
    write_atomic(
        &seed_dir.join("run.json"),
        serde_json::to_string_pretty(&json!({
            "state": "CHECKED", "evidence_id": hex(&ev.id()), "status": status_code(ev.status), "provenance": facts.provenance.label(),
            "measures": c.measures.to_json(), "run_wall_ms": t0.elapsed().as_millis() as u64,
            "scope": loaded.params.scope.statement(loaded.policy.repetition_count),
            "policy": loaded.params.policy_label,
        }))
        .unwrap_or_default()
        .as_bytes(),
    )
    .map_err(io)?;
    ledger_append(
        inp.state_dir,
        &root,
        &cand,
        "EVIDENCE",
        json!({ "evidence_id": hex(&ev.id()), "status": status_code(ev.status), "seed": hex(&seed) }),
    )
    .map_err(io)?;
    let local = verify_conformance_evidence_v1(&loaded.commitment, &beacon, &seed, loaded.policy.security_bits, ev);
    Ok(RunOutcome::Evidence { evidence: ev.clone(), dir: seed_dir, seed, local, measures: c.measures, provenance: facts.provenance })
}

// ---------------------------------------------------------------------------------------------------------------------------
// Verify: a fresh process
// ---------------------------------------------------------------------------------------------------------------------------

pub struct VerifyInput<'a> {
    pub pack_dir: &'a Path,
    pub artifact: &'a Path,
    pub state_dir: &'a Path,
    pub commitment: &'a str,
    pub evidence: &'a Path,
    pub source: &'a dyn BeaconFactSource,
    /// Re-execute every selected check and require the producer's evidence to be reproduced exactly. `false`: only the challenge,
    /// the selection and the evidence's own consistency are verified, and the verdict can never be a pass.
    pub rerun: bool,
    pub impls: ImplSet,
}

#[derive(Debug)]
pub enum Verdict {
    /// Every derivable field recomputed, every check re-executed, the evidence reproduced exactly.
    Pass { evidence_id: Digest, measures: Measures, provenance: FactsProvenanceV1 },
    /// The challenge, selection and evidence are consistent, but the results were not re-executed: not a pass.
    NotReproduced { provenance: FactsProvenanceV1 },
    /// The beacon is not locked on this history (WaitingRandomness / BEACON_UNAVAILABLE): not a pass.
    Pending { why: String },
    /// Honest evidence of a non-passing outcome (`Skipped`, `Incomplete`, `Failed`): not a pass.
    NotPass { status: ConformanceStatusV1, detail: String },
    /// The evidence is not what this node derives: forged, stale or substituted.
    Fail { code: &'static str, detail: String },
}

impl Verdict {
    pub fn is_pass(&self) -> bool {
        matches!(self, Verdict::Pass { .. })
    }
}

fn fail(code: &'static str, detail: impl Into<String>) -> Result<Verdict, Refusal> {
    Ok(Verdict::Fail { code, detail: detail.into() })
}

/// Judge presented evidence from public inputs alone: the pack and artifact, the committed state, and the canonical facts.
pub fn verify_conformance(inp: &VerifyInput<'_>, log: &dyn Fn(String)) -> Result<Verdict, Refusal> {
    let loaded = load_commitment(&resolve_commitment_dir(inp.state_dir, inp.commitment)?)?;
    rebind(&loaded, inp.pack_dir, inp.artifact, log)?;
    let ev_bytes = std::fs::read(inp.evidence).map_err(|e| Refusal::new("STATE_IO", format!("{}: {e}", inp.evidence.display())))?;
    let presented = match BeaconConformanceEvidenceV1::try_from_slice(&ev_bytes) {
        Ok(e) => e,
        Err(e) => return fail("EVIDENCE_MALFORMED", format!("{}: {e}", inp.evidence.display())),
    };
    let facts: ChainBeaconFactsV1 = inp.source.facts(&loaded.commitment, &loaded.policy)?;
    let ctx = resolve_facts(&loaded.commitment, &loaded.policy, &facts)?;
    let state =
        collect_work_beacon_v1(&ctx, &facts.events, facts.tip_position).map_err(|e| Refusal::new("POLICY_INVALID", e.to_string()))?;
    let beacon = match state {
        WorkBeaconStateV1::Locked(b) => b,
        WorkBeaconStateV1::Unavailable { have, need } => {
            return Ok(Verdict::Pending {
                why: format!(
                    "{}: the window closed with {have} of {need} qualifying works",
                    OnboardingFailureV1::BeaconUnavailable.code()
                ),
            });
        }
        WorkBeaconStateV1::Collecting { have, need } => {
            return Ok(Verdict::Pending { why: format!("WaitingRandomness: {have} of {need} qualifying works so far") });
        }
        WorkBeaconStateV1::Candidate { have, lock_position } => {
            return Ok(Verdict::Pending {
                why: format!("WaitingRandomness: {have} works, the beacon locks at position {lock_position}"),
            });
        }
    };
    // A beacon presented next to the evidence is a claim to recompute: reordered, non-canonical, duplicated or forged sources fail here.
    if let Some(dir) = inp.evidence.parent()
        && let Ok(bytes) = std::fs::read(dir.join("beacon.borsh"))
    {
        match WorkBeaconV1::try_from_slice(&bytes) {
            Ok(p) => {
                if let Err(e) = verify_work_beacon_v1(&ctx, &p, &facts.events, facts.tip_position) {
                    return fail("BEACON_NOT_CANONICAL", e.to_string());
                }
            }
            Err(e) => return fail("BEACON_NOT_CANONICAL", format!("beacon.borsh: {e}")),
        }
    }
    let seed =
        challenge_seed_v1(&ctx, &loaded.commitment.subject(), &beacon).map_err(|e| Refusal::new("SEED_REFUSED", e.to_string()))?;

    // The contract's judgement first: same commitment, same policy, the beacon and seed THIS node derived, and only a complete pass.
    if let Err(e) = verify_conformance_evidence_v1(&loaded.commitment, &beacon, &seed, loaded.policy.security_bits, &presented) {
        return Ok(match e {
            ConformanceRefusalV1::NotPassed(s) => Verdict::NotPass { status: s, detail: e.to_string() },
            ConformanceRefusalV1::Incomplete { .. } => Verdict::NotPass { status: presented.status, detail: e.to_string() },
            ConformanceRefusalV1::Seed => Verdict::Fail { code: "STALE_OR_FORGED_SEED", detail: e.to_string() },
            ConformanceRefusalV1::Beacon => Verdict::Fail { code: "BEACON_MISMATCH", detail: e.to_string() },
            other => Verdict::Fail { code: "EVIDENCE_REFUSED", detail: other.to_string() },
        });
    }
    // The fields a fresh process can recompute from public inputs without running anything.
    let program = misaka_palw_tir_artifact::PalwTirContainerV1::open(inp.artifact)
        .map_err(|e| Refusal::new("ARTIFACT_UNREADABLE", e.to_string()))?
        .program;
    let selection = derive_selection(&seed, &loaded.policy, &loaded.params.scope, &program)?;
    let mut bad = Vec::new();
    if presented.qualifying_source_evidence_root != tool_root(DOMAIN_SOURCES, &beacon.sources) {
        bad.push("qualifying_source_evidence_root");
    }
    if presented.selected_vectors_root != selection.vectors_root() {
        bad.push("selected_vectors_root");
    }
    if presented.selected_tensor_ranges_root != selection.leaves_root() {
        bad.push("selected_tensor_ranges_root");
    }
    if presented.checks_required != selection.checks_required() {
        bad.push("checks_required");
    }
    if presented.scope_and_fault_model_id != loaded.params.scope.scope_and_fault_model_id(loaded.policy.repetition_count) {
        bad.push("scope_and_fault_model_id");
    }
    if presented.derived_epsilon_bits != loaded.params.scope.derived_epsilon_bits(loaded.policy.repetition_count) {
        bad.push("derived_epsilon_bits");
    }
    if presented.transcript_root != RootV1::Absent {
        bad.push("transcript_root");
    }
    if !bad.is_empty() {
        return fail("EVIDENCE_FORGED", format!("not what the canonical history and the committed scope derive: {}", bad.join(", ")));
    }
    if !inp.rerun {
        return Ok(Verdict::NotReproduced { provenance: facts.provenance });
    }
    let engine = Engine {
        commitment: &loaded.commitment,
        policy: &loaded.policy,
        scope: &loaded.params.scope,
        beacon: &beacon,
        seed,
        artifact: inp.artifact,
        impls: inp.impls,
        max_checks: None,
        fault: None,
        store: None,
    };
    let c = engine.compute(log)?;
    if c.evidence != presented {
        return fail(
            "EVIDENCE_NOT_REPRODUCED",
            format!(
                "re-executing every selected check does not reproduce the evidence; differing: {}",
                evidence_diff(&c.evidence, &presented).join(", ")
            ),
        );
    }
    Ok(Verdict::Pass { evidence_id: presented.id(), measures: c.measures, provenance: facts.provenance })
}

// ---------------------------------------------------------------------------------------------------------------------------
// Synthetic facts (for exercising the pipeline; NOT chain history)
// ---------------------------------------------------------------------------------------------------------------------------

/// **Synthetic canonical facts** for a commitment: `k` fresh, Final, DA-satisfied, independent works of two synthetic Active profiles
/// settled inside the window, and a tip at depth `D` past the last. Labelled `Synthetic` everywhere it travels — it exercises the
/// pipeline; it is not a canonical history of any chain, and a verdict reached on it says so.
pub fn synthetic_facts(
    commitment: &ConformanceCommitmentV1,
    policy: &PostCommitChallengePolicyV1,
    commitment_position: u64,
    label: &str,
) -> ChainBeaconFactsV1 {
    use misaka_palw_challenge::beacon::{FinalPathV1, WorkFinalEventV1, WorkSourceKindV1};
    let a = named_id(&format!("synthetic-profile/{label}/a"));
    let b = named_id(&format!("synthetic-profile/{label}/b"));
    let start = commitment_position.saturating_add(policy.anchor_delay_slots);
    let mut events = Vec::new();
    for i in 0..policy.work_count_k {
        events.push(WorkFinalEventV1 {
            kind: WorkSourceKindV1::RealUsefulWork,
            source_profile_id: if i % 2 == 0 { a } else { b },
            canonical_work_id: named_id(&format!("synthetic-work/{label}/{i}")),
            execution_commitment: named_id(&format!("synthetic-exec/{label}/{i}")),
            accepted_position: start + 1 + i as u64,
            settlement_position: start + 2 + i as u64,
            occurrence_index: 0,
            claim_final: true,
            da_satisfied: true,
            validity_independent: true,
            depends_on_profiles: vec![],
            // Panel-licensed is today's normal path; a MODEL_CONFORMANCE beacon accepts either.
            final_path: FinalPathV1::PanelLicensed {
                panel_seed_id: named_id(&format!("synthetic-panel/{label}/{i}")),
                panel_epoch: 1,
            },
        });
    }
    let last = events.last().map(|e| e.settlement_position).unwrap_or(start);
    ChainBeaconFactsV1 {
        provenance: FactsProvenanceV1::Synthetic(label.to_string()),
        commitment_position,
        challenge_epoch: 1,
        eligible_profiles: [a, b].into_iter().collect(),
        excluded_profiles: [commitment.candidate_id].into_iter().collect(),
        events,
        tip_position: last.saturating_add(policy.settlement_depth_d),
    }
}
