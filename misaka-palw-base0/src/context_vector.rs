//! **ADR-0110 — a context vector, and the stages that verify it.**
//!
//! A vector is a name, a seed and a geometry (Decision 1). Everything a verifier needs is derived
//! from those three with no free parameter: the class is the family's held row projected at the
//! geometry, the weights are the family's deterministic derivation from the seed, the job's anchor
//! is a keyed hash of the seed, and the prompt is the family's own canonical prompt for that anchor
//! (`base0_rc_job_v1`'s stream), committed under the Merkle form and carried by a free-prompt job.
//!
//! [`palw_verify_context_vector_v1`] runs the network's own pipeline over it (Decision 2), through
//! the seam a node holds — `PalwExecutionBackendV1` on the family's registered-row backend — and
//! returns what each stage found. The findings render as one canonical document (Decision 3):
//! `consensus` and `verdicts` must agree byte for byte on every honest machine and are what
//! [`PalwContextFindingsV1::document_id`] covers; `host` is reported and never compared.
//!
//! Nothing here is read by a consensus, node or SDK crate, and nothing here signs: the receipt is
//! the CLI's (ADR-0110 Decision 4).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;
use kaspa_hashes::Hash64;

/// The vector document's own name, inside every vector.
pub const PALW_CONTEXT_VECTOR_V1: &str = "misaka-palw/context-vector/v1";
/// The verification document's own name, and its id's key.
pub const PALW_CONTEXT_VERIFICATION_V1: &str = "misaka-palw/context-verification/v1";
const DOMAIN_SEED: &[u8] = b"misaka-palw/context-vector/seed/v1";
const DOMAIN_ANCHOR: &[u8] = b"misaka-palw/context-vector/anchor/v1";
const DOMAIN_VECTOR_ID: &[u8] = b"misaka-palw/context-vector/v1";
const DOMAIN_JOB_FIELD: &[u8] = b"misaka-palw/context-vector/job-field/v1";

/// The one family a vector names today: the dense A16 row under the held map (graph-v7).
pub const PALW_CONTEXT_FAMILY_A16_V7: &str = "a16-graph-v7";

/// The decode calls every shipped vector runs: enough for a decode leaf in the court's sample and
/// a seed row in the last interval, and nothing a context vector is about.
pub const PALW_CONTEXT_VECTOR_DECODE_V1: u32 = 4;

/// **The thinnest geometry the held row admits** (Decision 1). A vector proves the protocol at a
/// width, not a model's cost at it — that is `palw-model-fit --preset held`'s table (Decision 7).
pub fn palw_context_vector_geometry_v1(n_ctx: u32) -> PalwQwen25GeometryV1 {
    PalwQwen25GeometryV1 {
        layer_count: 2,
        hidden_dim: 8,
        ffn_dim: 8,
        attn_heads: 2,
        attn_kv_heads: 2,
        attn_head_dim: 4,
        vocab_size: 64,
        n_ctx,
        n_threads: 1,
        rms_eps_q: 1,
        tile_len: 4,
    }
}

/// **A vector** — a name, a seed and a geometry, and the job's two counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwContextVectorV1 {
    pub name: String,
    pub family: String,
    pub geometry: PalwQwen25GeometryV1,
    pub prefill: u32,
    pub decode: u32,
    pub prompt_ids_form: PalwPromptIdsFormV1,
    pub seed: Hash64,
    /// Whether this is one of [`palw_context_vectors_v1`], by name and value.
    pub shipped: bool,
}

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(&(part.len() as u64).to_le_bytes());
        state.update(part);
    }
    Hash64::from_slice(state.finalize().as_bytes())
}

/// A shipped vector's seed: a keyed hash of its NAME, so nobody chooses it (Decision 1).
pub fn palw_context_vector_seed_v1(name: &str) -> Hash64 {
    keyed64(DOMAIN_SEED, &[name.as_bytes()])
}

/// **The shipped vectors, by width** (Decision 5): the three CI pins, then the external runs.
///
/// `0110-dense-v7-256k` was added on 2026-09-11 as the widest vector the A16 tier could run: its
/// last position attends to 262,143 rows, and the attention ops refused a history past 2^18
/// (ADR-0103 §10.7). ADR-0116 moved that wall, for a class under the held regime, to the regime's
/// own width, so `0110-dense-v7-2m` — the width the regime is for, whose last position attends to
/// 2,097,151 rows — is inside it. [`palw_context_vector_blocked_v1`] still refuses, by name and
/// before a position runs, a vector past its class's bound.
pub fn palw_context_vectors_v1() -> Vec<PalwContextVectorV1> {
    [
        ("0110-dense-v7-512", 512u32),
        ("0110-dense-v7-4k", 4096),
        ("0110-dense-v7-32k", 32_768),
        ("0110-dense-v7-128k", 131_072),
        ("0110-dense-v7-256k", 262_144),
        ("0110-dense-v7-2m", 2_097_152),
    ]
    .into_iter()
    .map(|(name, n_ctx)| PalwContextVectorV1 {
        name: name.to_string(),
        family: PALW_CONTEXT_FAMILY_A16_V7.to_string(),
        geometry: palw_context_vector_geometry_v1(n_ctx),
        prefill: n_ctx - PALW_CONTEXT_VECTOR_DECODE_V1,
        decode: PALW_CONTEXT_VECTOR_DECODE_V1,
        prompt_ids_form: PalwPromptIdsFormV1::MerkleV1,
        seed: palw_context_vector_seed_v1(name),
        shipped: true,
    })
    .collect()
}

/// The shipped vector named `name`.
pub fn palw_context_vector_v1(name: &str) -> Option<PalwContextVectorV1> {
    palw_context_vectors_v1().into_iter().find(|v| v.name == name)
}

/// **Why a vector cannot be produced on this tree, if it cannot** — the attention history wall,
/// stated before a single position runs. The job's last forward attends to every row the job
/// holds (`prefill + decode − 1`). The A16 tier's attention ops refuse a history past the class's
/// bound (`palw_attn_history_bound_v1`: the held regime's 2^21 for the held row every vector runs,
/// ADR-0116; 2^18 before it, ADR-0103 §10.7), and so do the engine and the court, which compose
/// them. A vector past it would otherwise run for hours and then fail at its first row past the
/// bound with an op's error.
pub fn palw_context_vector_blocked_v1(vector: &PalwContextVectorV1) -> Option<String> {
    let rows = u64::from(vector.prefill) + u64::from(vector.decode.saturating_sub(1));
    let wall = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(vector.geometry)
        .map(|profile| kaspa_consensus_core::palw_state_chunk_map::palw_attn_history_bound_v1(&profile))
        .unwrap_or(kaspa_consensus_core::palw_base0_a16::A16_MAX_ATTN_HISTORY_V1) as u64;
    (rows > wall).then(|| {
        format!(
            "this job's last position attends to {rows} rows and this class's attention ops refuse a history past {wall} \
             (the A16 attention history bound, ADR-0116): no A16 class produces, replays or is adjudicated at this width"
        )
    })
}

fn form_name(form: PalwPromptIdsFormV1) -> &'static str {
    match form {
        PalwPromptIdsFormV1::Flat => "flat",
        PalwPromptIdsFormV1::MerkleV1 => "merkle-v1",
    }
}

impl PalwContextVectorV1 {
    /// The vector as its canonical document (Decision 1): the inputs, and nothing derived.
    pub fn to_json_v1(&self) -> serde_json::Value {
        let g = &self.geometry;
        serde_json::json!({
            "vector": PALW_CONTEXT_VECTOR_V1,
            "name": self.name,
            "family": self.family,
            "geometry": {
                "layers": g.layer_count, "hidden": g.hidden_dim, "ffn": g.ffn_dim, "heads": g.attn_heads,
                "kv_heads": g.attn_kv_heads, "head_dim": g.attn_head_dim, "vocab": g.vocab_size, "n_ctx": g.n_ctx,
                "tile_len": g.tile_len, "rms_eps_q": g.rms_eps_q,
            },
            "job": { "prefill": self.prefill, "decode": self.decode, "prompt_ids_form": form_name(self.prompt_ids_form) },
            "seed": self.seed.to_string(),
        })
    }

    /// **A vector file, read back** — the fields [`Self::to_json_v1`] writes, every one required.
    pub fn from_json_v1(value: &serde_json::Value) -> Result<Self, String> {
        let field = |v: &serde_json::Value, k: &str| -> Result<serde_json::Value, String> {
            v.get(k).cloned().ok_or_else(|| format!("the vector has no `{k}`"))
        };
        let uint = |v: &serde_json::Value, k: &str| -> Result<u64, String> {
            field(v, k)?.as_u64().ok_or_else(|| format!("`{k}` is not an unsigned integer"))
        };
        if field(value, "vector")?.as_str() != Some(PALW_CONTEXT_VECTOR_V1) {
            return Err(format!("not a {PALW_CONTEXT_VECTOR_V1} document"));
        }
        let text = |k: &str| -> Result<String, String> {
            field(value, k)?.as_str().map(str::to_string).ok_or_else(|| format!("`{k}` is not a string"))
        };
        let family = text("family")?;
        if family != PALW_CONTEXT_FAMILY_A16_V7 {
            return Err(format!("family `{family}` is not one this build verifies (only {PALW_CONTEXT_FAMILY_A16_V7})"));
        }
        let g = field(value, "geometry")?;
        let small = |k: &str| -> Result<u32, String> { u32::try_from(uint(&g, k)?).map_err(|_| format!("`{k}` is out of range")) };
        let geometry = PalwQwen25GeometryV1 {
            layer_count: u16::try_from(uint(&g, "layers")?).map_err(|_| "`layers` is out of range".to_string())?,
            hidden_dim: small("hidden")?,
            ffn_dim: small("ffn")?,
            attn_heads: u16::try_from(uint(&g, "heads")?).map_err(|_| "`heads` is out of range".to_string())?,
            attn_kv_heads: u16::try_from(uint(&g, "kv_heads")?).map_err(|_| "`kv_heads` is out of range".to_string())?,
            attn_head_dim: small("head_dim")?,
            vocab_size: small("vocab")?,
            n_ctx: small("n_ctx")?,
            n_threads: 1,
            rms_eps_q: field(&g, "rms_eps_q")?.as_i64().ok_or("`rms_eps_q` is not an integer")?,
            tile_len: small("tile_len")?,
        };
        let job = field(value, "job")?;
        let prompt_ids_form = match field(&job, "prompt_ids_form")?.as_str() {
            Some("merkle-v1") => PalwPromptIdsFormV1::MerkleV1,
            Some("flat") => PalwPromptIdsFormV1::Flat,
            other => return Err(format!("prompt_ids_form {other:?} is neither `merkle-v1` nor `flat`")),
        };
        let seed_hex = text("seed")?;
        let seed = seed_hex.parse::<Hash64>().map_err(|_| "`seed` is not 128 hex characters".to_string())?;
        let name = text("name")?;
        let mut vector = Self {
            name,
            family,
            geometry,
            prefill: u32::try_from(uint(&job, "prefill")?).map_err(|_| "`prefill` is out of range".to_string())?,
            decode: u32::try_from(uint(&job, "decode")?).map_err(|_| "`decode` is out of range".to_string())?,
            prompt_ids_form,
            seed,
            shipped: false,
        };
        vector.shipped = palw_context_vector_v1(&vector.name).is_some_and(|s| s == Self { shipped: true, ..vector.clone() });
        Ok(vector)
    }

    /// `vector_id = BLAKE2b-512(key = "misaka-palw/context-vector/v1", canonical bytes)`.
    pub fn vector_id(&self) -> Hash64 {
        let bytes = palw_canonical_json_v1(&self.to_json_v1());
        let mut state = blake2b_simd::Params::new().hash_length(64).key(DOMAIN_VECTOR_ID).to_state();
        state.update(&bytes);
        Hash64::from_slice(state.finalize().as_bytes())
    }

    /// The job's anchor: a keyed hash of the seed (Decision 1).
    pub fn anchor(&self) -> Hash64 {
        keyed64(DOMAIN_ANCHOR, &[self.seed.as_byte_slice()])
    }

    /// The weights' seed, as the family's derivation takes it.
    fn artifact_seed(&self) -> u64 {
        u64::from_le_bytes(self.seed.as_byte_slice()[..8].try_into().expect("a 64-byte hash has 8 bytes"))
    }

    /// A field of the job the vector fixes, derived from the seed so two vectors never share one.
    fn job_field(&self, what: &[u8]) -> Hash64 {
        keyed64(DOMAIN_JOB_FIELD, &[what, self.seed.as_byte_slice()])
    }
}

// =================================================================================================
// The canonical form (Decision 3)
// =================================================================================================

/// **RFC 8785 for the documents this module writes** — objects with their keys sorted, no
/// whitespace, integers only, strings with JSON's short escapes and `\u00xx` for the other control
/// characters. The documents hold no float by construction; one is refused rather than formatted.
/// The CLI checks this against the tree's general canonicaliser (`misaka_palw_derive::canon_json`).
pub fn palw_canonical_json_v1(value: &serde_json::Value) -> Vec<u8> {
    fn string(out: &mut Vec<u8>, s: &str) {
        out.push(b'"');
        for c in s.chars() {
            match c {
                '"' => out.extend_from_slice(b"\\\""),
                '\\' => out.extend_from_slice(b"\\\\"),
                '\u{08}' => out.extend_from_slice(b"\\b"),
                '\u{0c}' => out.extend_from_slice(b"\\f"),
                '\n' => out.extend_from_slice(b"\\n"),
                // Two pushes, not a literal: the float guard's raw-string detector reads `\` `r`
                // `"` as the start of a raw string, and a slice of the two chars is the one
                // spelling clippy (`byte_char_slices`) refuses.
                '\r' => {
                    out.push(b'\\');
                    out.push(b'r');
                }
                '\t' => out.extend_from_slice(b"\\t"),
                c if (c as u32) < 0x20 => out.extend_from_slice(format!("\\u{:04x}", c as u32).as_bytes()),
                c => {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
        }
        out.push(b'"');
    }
    fn write(out: &mut Vec<u8>, value: &serde_json::Value) {
        match value {
            serde_json::Value::Null => out.extend_from_slice(b"null"),
            serde_json::Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
            serde_json::Value::Number(n) => {
                assert!(n.is_u64() || n.is_i64(), "a context document holds integers only, and {n} is not one");
                out.extend_from_slice(n.to_string().as_bytes());
            }
            serde_json::Value::String(s) => string(out, s),
            serde_json::Value::Array(items) => {
                out.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    write(out, item);
                }
                out.push(b']');
            }
            serde_json::Value::Object(map) => {
                // JCS sorts by UTF-16 code units; every key this module writes is ASCII, where
                // that order is the bytes' order.
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
                out.push(b'{');
                for (i, key) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(b',');
                    }
                    string(out, key);
                    out.push(b':');
                    write(out, &map[key]);
                }
                out.push(b'}');
            }
        }
    }
    let mut out = Vec::new();
    write(&mut out, value);
    out
}

// =================================================================================================
// The stages (Decision 2)
// =================================================================================================

/// The six stages, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwContextStageV1 {
    Produce,
    Commit,
    Seat,
    Court,
    Availability,
    Fit,
}

impl PalwContextStageV1 {
    pub const ALL: [Self; 6] = [Self::Produce, Self::Commit, Self::Seat, Self::Court, Self::Availability, Self::Fit];

    pub fn name(self) -> &'static str {
        match self {
            Self::Produce => "produce",
            Self::Commit => "commit",
            Self::Seat => "seat",
            Self::Court => "court",
            Self::Availability => "availability",
            Self::Fit => "fit",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// A stage's verdict. There is no bare overall pass: the document says which stages ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwContextVerdictV1 {
    Pass,
    Fail(String),
    Skipped(String),
}

impl PalwContextVerdictV1 {
    fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Pass => serde_json::json!("pass"),
            Self::Fail(why) => serde_json::json!({ "fail": why }),
            Self::Skipped(why) => serde_json::json!({ "skipped": why }),
        }
    }
}

/// **What a verifier is judged under** — the network's ruleset, which the fit reads and the
/// backend's ladder and prompt form follow.
pub struct PalwContextRulesetV1 {
    /// A name a reader recognises (`devnet-held`).
    pub name: String,
    pub params: kaspa_consensus_core::config::params::Params,
}

impl PalwContextRulesetV1 {
    /// **The devnet minted with the held regime** — exactly what `kaspad
    /// --palw-held-context-devnet` runs: `palw_held_context_mint_v1` at the devnet's own ladder.
    pub fn devnet_held_v1() -> Result<Self, String> {
        let base = kaspa_consensus_core::config::params::devnet_shipped_params();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &base.palw_consensus_mode else {
            return Err("the shipped devnet is not a ConsensusV2 network".to_string());
        };
        let ladder = bundle.court.max_step_leaf_count();
        let params = kaspa_consensus_core::config::params::palw_held_context_mint_v1(base, ladder)
            .map_err(|e| format!("the held devnet does not assemble: {e:?}"))?;
        Ok(Self { name: "devnet-held".to_string(), params })
    }

    fn bundle(&self) -> Result<&kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2, String> {
        match &self.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Ok(bundle),
            _ => Err(format!("{} is not a ConsensusV2 ruleset", self.name)),
        }
    }

    fn network_name(&self) -> String {
        self.params.net.to_string()
    }
}

/// What the `commit` stage records: the roots the chain would hold for the claim.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwContextCommitV1 {
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub job_id: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub output_root: Hash64,
    pub step_merkle_root: Hash64,
    pub checkpoint_merkle_root: Hash64,
    pub step_leaf_count: u64,
    pub checkpoint_count: u32,
    pub decode_executed: u32,
    pub capture_bytes: u64,
}

/// One interval, as the seat judged it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwContextIntervalV1 {
    pub index: u32,
    pub opening_bytes: u64,
    /// Bytes of served state the seat resumed from, on the Resume route.
    pub resume_bytes: Option<u64>,
    pub verdict: String,
}

/// What the `seat` stage records.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwContextSeatV1 {
    pub route: String,
    pub interval_count: u32,
    pub intervals: Vec<PalwContextIntervalV1>,
}

/// One sampled leaf, as the court judged the executor's evidence at it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwContextCourtLeafV1 {
    pub leaf: u64,
    pub why: &'static str,
    pub evidence_bytes: u64,
    pub verdict: String,
}

/// The tampered capture, as the seat and the court judged it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwContextTamperV1 {
    pub leaf: u64,
    pub seat: String,
    pub named: Option<u64>,
    pub evidence_bytes: u64,
    pub verdict: String,
}

/// One held unit, answered and checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwContextAnswerV1 {
    pub unit: String,
    pub answer_bytes: u64,
    pub accepted: Result<(), String>,
}

/// One wall of the fit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwContextFitRowV1 {
    pub wall: String,
    pub order: String,
    pub need: u64,
    pub have: u64,
    pub unit: &'static str,
    pub verdict: String,
}

/// **Everything a run found**, stage by stage, and what the host measured.
#[derive(Clone, Debug)]
pub struct PalwContextFindingsV1 {
    pub vector: PalwContextVectorV1,
    pub ruleset_name: String,
    pub network: String,
    pub consensus_params_id: String,
    pub ladder: u64,
    pub commit: Option<PalwContextCommitV1>,
    pub seat: Option<PalwContextSeatV1>,
    pub court: Vec<PalwContextCourtLeafV1>,
    pub tamper: Option<PalwContextTamperV1>,
    pub availability: Vec<PalwContextAnswerV1>,
    pub fit: Vec<PalwContextFitRowV1>,
    pub verdicts: BTreeMap<PalwContextStageV1, PalwContextVerdictV1>,
    /// Wall milliseconds per stage — host facts.
    pub stage_ms: BTreeMap<PalwContextStageV1, u64>,
}

fn hex(h: &Hash64) -> String {
    h.to_string()
}

impl PalwContextFindingsV1 {
    /// **The document without its host section** — what [`Self::document_id`] covers.
    pub fn agreed_json_v1(&self) -> serde_json::Value {
        let mut consensus = serde_json::Map::new();
        if let Some(c) = &self.commit {
            consensus.insert(
                "commit".into(),
                serde_json::json!({
                    "class_id": hex(&c.class_id), "artifact_root": hex(&c.artifact_root), "job_id": hex(&c.job_id),
                    "execution_root": hex(&c.execution_root), "trace_root": hex(&c.trace_root),
                    "output_root": hex(&c.output_root), "step_merkle_root": hex(&c.step_merkle_root),
                    "checkpoint_merkle_root": hex(&c.checkpoint_merkle_root), "step_leaf_count": c.step_leaf_count,
                    "checkpoint_count": c.checkpoint_count, "decode_executed": c.decode_executed,
                    "capture_bytes": c.capture_bytes,
                }),
            );
        }
        if let Some(s) = &self.seat {
            let intervals: Vec<serde_json::Value> = s
                .intervals
                .iter()
                .map(|i| {
                    serde_json::json!({ "index": i.index, "opening_bytes": i.opening_bytes, "resume_bytes": i.resume_bytes,
                    "verdict": i.verdict })
                })
                .collect();
            consensus.insert(
                "seat".into(),
                serde_json::json!({ "route": s.route, "interval_count": s.interval_count, "intervals": intervals }),
            );
        }
        if !self.court.is_empty() || self.tamper.is_some() {
            let leaves: Vec<serde_json::Value> = self
                .court
                .iter()
                .map(|l| serde_json::json!({ "leaf": l.leaf, "why": l.why, "evidence_bytes": l.evidence_bytes, "verdict": l.verdict }))
                .collect();
            let tamper = self.tamper.as_ref().map(|t| {
                serde_json::json!({ "leaf": t.leaf, "seat": t.seat, "named": t.named, "evidence_bytes": t.evidence_bytes,
                "verdict": t.verdict })
            });
            consensus.insert("court".into(), serde_json::json!({ "honest": leaves, "tampered": tamper }));
        }
        if !self.availability.is_empty() {
            let answers: Vec<serde_json::Value> = self
                .availability
                .iter()
                .map(|a| {
                    let accepted = match &a.accepted {
                        Ok(()) => serde_json::json!(true),
                        Err(why) => serde_json::json!({ "refused": why }),
                    };
                    serde_json::json!({ "unit": a.unit, "answer_bytes": a.answer_bytes, "accepted": accepted })
                })
                .collect();
            consensus.insert("availability".into(), serde_json::Value::Array(answers));
        }
        if !self.fit.is_empty() {
            let rows: Vec<serde_json::Value> = self
                .fit
                .iter()
                .map(|r| {
                    serde_json::json!({ "wall": r.wall, "order": r.order, "need": r.need, "have": r.have, "unit": r.unit,
                    "verdict": r.verdict })
                })
                .collect();
            consensus.insert("fit".into(), serde_json::Value::Array(rows));
        }
        let verdicts: serde_json::Map<String, serde_json::Value> =
            self.verdicts.iter().map(|(stage, v)| (stage.name().to_string(), v.to_json())).collect();
        serde_json::json!({
            "document": PALW_CONTEXT_VERIFICATION_V1,
            "vector": self.vector.to_json_v1(),
            "vector_id": hex(&self.vector.vector_id()),
            "shipped": self.vector.shipped,
            "ruleset": { "name": self.ruleset_name, "network": self.network, "consensus_params_id": self.consensus_params_id,
                         "ladder": self.ladder },
            "consensus": consensus,
            "verdicts": verdicts,
        })
    }

    /// `document_id = BLAKE2b-512(key = "misaka-palw/context-verification/v1", canonical bytes of
    /// the document without host)` (Decision 3).
    pub fn document_id(&self) -> Hash64 {
        let bytes = palw_canonical_json_v1(&self.agreed_json_v1());
        let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_CONTEXT_VERIFICATION_V1.as_bytes()).to_state();
        state.update(&bytes);
        Hash64::from_slice(state.finalize().as_bytes())
    }

    /// **The whole document**: the agreed sections, the id, and the host's own facts beside them.
    pub fn document_json_v1(&self, host: serde_json::Value) -> serde_json::Value {
        let mut doc = self.agreed_json_v1();
        let map = doc.as_object_mut().expect("the document is an object");
        map.insert("document_id".into(), serde_json::json!(hex(&self.document_id())));
        let stages: serde_json::Map<String, serde_json::Value> =
            self.stage_ms.iter().map(|(s, ms)| (s.name().to_string(), serde_json::json!(ms))).collect();
        let mut host = host;
        if let Some(h) = host.as_object_mut() {
            h.insert("stages_ms".into(), serde_json::Value::Object(stages));
        }
        map.insert("host".into(), host);
        doc
    }

    /// Whether every stage that ran passed.
    pub fn all_passed(&self) -> bool {
        self.verdicts.values().all(|v| !matches!(v, PalwContextVerdictV1::Fail(_)))
    }
}

/// The class a vector names: its artifact, its profile and the backend a node would build for it.
struct VectorClassV1 {
    artifact: Arc<crate::artifact::Base0ArtifactV1>,
    profile: PalwShapeProfileV3,
    backend: crate::qwen25_a16_backend::Qwen25A16Backend,
}

fn vector_class_v1(vector: &PalwContextVectorV1, ruleset: &PalwContextRulesetV1) -> Result<VectorClassV1, String> {
    let g = vector.geometry;
    let shape = crate::artifact::Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: crate::artifact::LN_THETA_10000_GEN_Q,
        eps_q: g.rms_eps_q,
    };
    let artifact = crate::artifact::Base0ArtifactV1::derive_deterministic(shape, vector.artifact_seed())
        .map_err(|e| format!("the geometry derives no artifact: {e:?}"))?
        .with_a16_params(crate::engine_a16::derived_a16_store(&shape))
        .map_err(|e| format!("the derived store does not attach: {e:?}"))?;
    let profile = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_profile_v7(g)
        .map_err(|e| format!("the held row does not project at this geometry: {e:?}"))?;
    if !kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile) {
        return Err("the projected row is not under the held map".to_string());
    }
    let artifact = Arc::new(artifact);
    let ladder = ruleset.bundle()?.court.max_step_leaf_count();
    let backend = crate::qwen25_a16_backend::Qwen25A16Backend::from_registered_profile(
        artifact.clone(),
        ruleset.network_name().into_bytes(),
        profile.clone(),
        (vector.prefill, vector.decode),
    )?
    .with_prompt_ids_form(vector.prompt_ids_form)
    .with_step_ladder_cap(ladder);
    Ok(VectorClassV1 { artifact, profile, backend })
}

/// The free-prompt job a vector's executor runs: its prompt committed under the vector's form,
/// every other field fixed or derived from the seed.
fn vector_job_v1(
    vector: &PalwContextVectorV1,
    ruleset: &PalwContextRulesetV1,
    profile: &PalwShapeProfileV3,
) -> Result<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3, Vec<usize>), String> {
    use kaspa_consensus_core::palw_freeprompt_v3 as fp;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    let (_, prompt) = crate::produce::base0_rc_job_v1(
        profile,
        vector.anchor(),
        vector.geometry.vocab_size as usize,
        vector.prefill,
        vector.decode,
        vector.prompt_ids_form,
    );
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(vector.prompt_ids_form, &ids)
        .map_err(|e| format!("{} prompt ids do not commit: {e}", ids.len()))?;
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        ruleset.network_name().as_bytes(),
        Some(ruleset.params.genesis.hash),
    );
    let bond_txid = vector.job_field(b"executor-bond");
    let job = fp::PalwFreePromptJobV3 {
        version: fp::PALW_FP_V3_VERSION,
        network_domain,
        class_id: profile.shape_profile_id(),
        executor_bond: TransactionOutpoint::new(TransactionId::from_slice(bond_txid.as_byte_slice()), 0),
        executor_pubkey: vector.job_field(b"executor-pubkey").as_byte_slice().to_vec(),
        operator_id: vector.job_field(b"operator"),
        anchor_block: vector.anchor(),
        anchor_daa: 0,
        job_nonce: vector.job_field(b"nonce").as_byte_slice()[..32].try_into().expect("a 64-byte hash has 32 bytes"),
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash,
        prompt_tokens: vector.prefill,
        decode_token_limit: vector.decode,
        max_context_tokens: vector.geometry.n_ctx,
        privacy_mode: fp::PALW_FP_PRIVACY_PANEL_DA,
        prompt_mode: fp::PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
    };
    Ok((job, prompt))
}

fn verdict_name<T: std::fmt::Debug>(v: &T) -> String {
    let mut s = format!("{v:?}");
    if let Some(i) = s.find([' ', '{', '(']) {
        s.truncate(i);
    }
    s
}

/// **The widest drill capture this module builds.** The tamper stage re-executes the job DENSE —
/// every tile kept — because the drill's fault moves one tile of a capture, and a fold keeps none.
/// Past this many step leaves the capture is gigabytes, and the stage says so rather than try.
pub const PALW_CONTEXT_TAMPER_MAX_LEAVES_V1: u64 = 1 << 24;

/// **Run the stages over a vector** (Decision 2). A stage that fails does not stop the stages
/// after it that can still run; one that cannot run says why.
pub fn palw_verify_context_vector_v1(
    vector: &PalwContextVectorV1,
    ruleset: &PalwContextRulesetV1,
    stages: &[PalwContextStageV1],
) -> PalwContextFindingsV1 {
    use PalwContextStageV1 as St;
    use PalwContextVerdictV1 as V;
    use kaspa_consensus_core::palw_held_da_v1::{PalwHeldDisclosureV1, PalwHeldMissingV1, palw_held_da_check_disclosure_v1};
    use kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1;
    use kaspa_consensus_core::palw_shard_court_v1::palw_leaf_evidence_bytes_v1;

    let wants = |s: St| stages.contains(&s);
    let ladder = ruleset.bundle().map(|b| b.court.max_step_leaf_count()).unwrap_or(0);
    let mut f = PalwContextFindingsV1 {
        vector: vector.clone(),
        ruleset_name: ruleset.name.clone(),
        network: ruleset.network_name(),
        consensus_params_id: ruleset.params.consensus_params_id().to_string(),
        ladder,
        commit: None,
        seat: None,
        court: Vec::new(),
        tamper: None,
        availability: Vec::new(),
        fit: Vec::new(),
        verdicts: BTreeMap::new(),
        stage_ms: BTreeMap::new(),
    };
    let skip_all = |f: &mut PalwContextFindingsV1, from: &[St], why: &str| {
        for s in from {
            if wants(*s) {
                f.verdicts.entry(*s).or_insert_with(|| V::Skipped(why.to_string()));
            }
        }
    };

    let class = match vector_class_v1(vector, ruleset) {
        Ok(class) => class,
        Err(why) => {
            if wants(St::Produce) {
                f.verdicts.insert(St::Produce, V::Fail(format!("the class: {why}")));
            }
            skip_all(&mut f, &St::ALL, "the vector's class does not build");
            return f;
        }
    };
    let backend: &dyn PalwExecutionBackendV1 = &class.backend;
    let form = vector.prompt_ids_form;

    // ---- fit: needs nothing but the class and the ruleset ----
    if wants(St::Fit) {
        let t = Instant::now();
        let verdict = (|| -> Result<(), String> {
            let bundle = ruleset.bundle()?;
            let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
                &ruleset.params,
                bundle,
                &class.profile,
                u64::MAX - 1,
            )?;
            // The regime the gate itself would read this class under at this point of judgement.
            let regime = kaspa_consensus_core::palw_model_fit_v1::palw_fit_regime_for_v1(shape.held, &class.profile);
            let report = kaspa_consensus_core::palw_model_fit_v1::palw_model_fit_v2(&class.profile, bundle, shape.court, form, regime);
            if !matches!(regime, kaspa_consensus_core::palw_model_fit_v1::PalwFitRegimeV1::Held { .. }) {
                return Err(format!("the ruleset reads this class under {regime:?}, not the held regime"));
            }
            let mut failures = Vec::new();
            for row in &report.rows {
                let verdict = verdict_name(&row.verdict);
                let order = verdict_name(&row.order);
                if verdict != "Admitted" {
                    failures.push(format!("{:?} {verdict}", row.wall));
                } else if order == "Linear" || order == "Unpriced" {
                    failures.push(format!("{:?} grows {order}", row.wall));
                }
                f.fit.push(PalwContextFitRowV1 {
                    wall: format!("{:?}", row.wall),
                    order,
                    need: row.need,
                    have: row.have,
                    unit: row.unit,
                    verdict,
                });
            }
            if failures.is_empty() { Ok(()) } else { Err(failures.join("; ")) }
        })();
        f.verdicts.insert(St::Fit, verdict.map_or_else(V::Fail, |()| V::Pass));
        f.stage_ms.insert(St::Fit, t.elapsed().as_millis() as u64);
    }

    // ---- produce ----
    let job = match vector_job_v1(vector, ruleset, &class.profile) {
        Ok(job) => job,
        Err(why) => {
            if wants(St::Produce) {
                f.verdicts.insert(St::Produce, V::Fail(format!("the job: {why}")));
            }
            skip_all(&mut f, &[St::Commit, St::Seat, St::Court, St::Availability], "no job");
            return f;
        }
    };
    let (job, prompt) = job;
    let ids: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
    let needs_run = [St::Produce, St::Commit, St::Seat, St::Court, St::Availability].into_iter().any(wants);
    if !needs_run {
        return f;
    }
    // ADR-0103 §10.7: refused before the first position rather than at the 262,145th.
    if let Some(why) = palw_context_vector_blocked_v1(vector) {
        if wants(St::Produce) {
            f.verdicts.insert(St::Produce, V::Fail(why));
        }
        skip_all(&mut f, &[St::Commit, St::Seat, St::Court, St::Availability], "nothing can be produced at this width");
        return f;
    }
    let t = Instant::now();
    let run = match backend.execute_free_prompt(&job, &prompt) {
        Ok(run) => run,
        Err(why) => {
            f.verdicts.insert(St::Produce, V::Fail(why));
            skip_all(&mut f, &[St::Commit, St::Seat, St::Court, St::Availability], "nothing was produced");
            return f;
        }
    };
    f.stage_ms.insert(St::Produce, t.elapsed().as_millis() as u64);
    if wants(St::Produce) {
        f.verdicts.insert(St::Produce, V::Pass);
    }
    let capture = run.outcome.material.clone();
    let output_ids = run.output_token_ids.clone();

    // ---- commit ----
    let t = Instant::now();
    let binding: PalwStepBindingV2 = match crate::produce::base0_material_decode_any_v1(&capture) {
        Ok(retention) => retention.binding().clone(),
        Err(e) => {
            f.verdicts.insert(St::Commit, V::Fail(format!("the capture does not decode: {e:?}")));
            skip_all(&mut f, &[St::Seat, St::Court, St::Availability], "no binding");
            return f;
        }
    };
    let artifact_root = match crate::inventory::a16_inventory_v1(&class.artifact, &class.profile) {
        Ok(inventory) => inventory.root(),
        Err(e) => {
            f.verdicts.insert(St::Commit, V::Fail(format!("the artifact has no inventory: {e:?}")));
            skip_all(&mut f, &[St::Seat, St::Court, St::Availability], "no artifact root");
            return f;
        }
    };
    let job_id = kaspa_consensus_core::palw_freeprompt_v3::fp_job_id_v3(&job);
    let roots = PalwClaimRootsV1 {
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        anchor: job_id,
        attempt_draw: None,
    };
    let work_leaves = binding.step_leaf_count;
    let ctx = binding.job_context.clone();
    f.commit = Some(PalwContextCommitV1 {
        class_id: class.profile.shape_profile_id(),
        artifact_root,
        job_id,
        execution_root: run.outcome.execution_root,
        trace_root: run.outcome.trace_root,
        output_root: run.outcome.output_root,
        step_merkle_root: binding.step_merkle_root,
        checkpoint_merkle_root: binding.checkpoint_merkle_root,
        step_leaf_count: binding.step_leaf_count,
        checkpoint_count: binding.checkpoint_count,
        decode_executed: output_ids.len() as u32,
        capture_bytes: capture.len() as u64,
    });
    if wants(St::Commit) {
        let committed = binding.committed_execution_root == run.outcome.execution_root && ctx.job_id == job_id;
        f.verdicts.insert(
            St::Commit,
            if committed { V::Pass } else { V::Fail("the capture's binding is not the claim's execution".to_string()) },
        );
    }
    f.stage_ms.insert(St::Commit, t.elapsed().as_millis() as u64);

    // ---- seat ----
    let window = ruleset.bundle().map(|b| b.state.window_receipt()).unwrap_or(0);
    let route = backend.fp_held_route_v1(window);
    if wants(St::Seat) {
        let t = Instant::now();
        let route_name = match route {
            Some(kaspa_consensus_core::palw_held_context_v1::PalwHeldSeatRouteV1::Recompute) => "recompute".to_string(),
            Some(kaspa_consensus_core::palw_held_context_v1::PalwHeldSeatRouteV1::Resume) => "resume".to_string(),
            None => "recompute (the class is not under the held map)".to_string(),
        };
        let mut seat = PalwContextSeatV1 { route: route_name, ..Default::default() };
        let verdict = (|| -> Result<(), String> {
            let count = backend.fp_interval_count(&capture).ok_or("the capture names no interval count")?;
            seat.interval_count = count;
            let resume = matches!(route, Some(kaspa_consensus_core::palw_held_context_v1::PalwHeldSeatRouteV1::Resume));
            let mut faults = Vec::new();
            for index in 0..count {
                let opening = backend.open_fp_interval(&capture, index, &ids)?;
                let covered = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening)
                    .ok()
                    .and_then(|v4| v4.anchor.map(|a| a.leaf.covered_decode_call));
                let mut resume_bytes = None;
                if let Some(covered) = covered {
                    if resume {
                        let state = backend.open_fp_resume_v1(&capture, index, &ids)?;
                        resume_bytes = Some(state.len() as u64);
                        backend.fp_accept_resume_v1(&state, &ctx, &ids, covered)?;
                    } else {
                        backend.checkpoint_root_for_context_v1(&ctx, &ids, &output_ids, covered)?;
                    }
                }
                let verdict = backend.verify_fp_interval_opening(&opening, roots, index, &ids, work_leaves);
                if verdict != PalwFpIntervalVerdictV1::Valid {
                    faults.push(format!("interval {index}: {verdict:?}"));
                }
                seat.intervals.push(PalwContextIntervalV1 {
                    index,
                    opening_bytes: opening.len() as u64,
                    resume_bytes,
                    verdict: verdict_name(&verdict),
                });
            }
            if faults.is_empty() { Ok(()) } else { Err(faults.join("; ")) }
        })();
        f.seat = Some(seat);
        f.verdicts.insert(St::Seat, verdict.map_or_else(V::Fail, |()| V::Pass));
        f.stage_ms.insert(St::Seat, t.elapsed().as_millis() as u64);
    }

    // The leaves the court samples: the first prefill leaf, the first leaf of the last interval,
    // and the last decode leaf that is not a fused site (whose terminal is the dissection).
    let class_id = class.profile.shape_profile_id();
    let sampled: Vec<(u64, &'static str)> = {
        let geometry = kaspa_consensus_core::palw_leaf_evidence_v1::PalwSeatIntervalGeometryV1::from_binding_v1(&binding);
        let interval_of = |leaf: u64| geometry.as_ref().and_then(|g| g.interval_of_leaf_v1(&binding, leaf));
        let fused = |leaf: u64| {
            kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &ctx, leaf).is_none_or(|c| {
                binding
                    .shape_profile
                    .resolve_node_slot(c.node_slot)
                    .is_some_and(|(n, _)| n.op_kind == kaspa_consensus_core::palw_step::PalwStepOpKindV1::AttnFused)
            })
        };
        let last_interval = geometry.map(|g| g.count.saturating_sub(1)).unwrap_or(0);
        let first_of_last = (0..work_leaves).find(|l| interval_of(*l) == Some(last_interval) && !fused(*l));
        let last_decode = (0..work_leaves).rev().find(|l| !fused(*l));
        let mut out = vec![(0u64, "the first prefill leaf")];
        if let Some(l) = first_of_last.filter(|l| *l != 0) {
            out.push((l, "the first leaf of the last interval"));
        }
        if let Some(l) = last_decode.filter(|l| out.iter().all(|(o, _)| o != l)) {
            out.push((l, "the last leaf that is not a fused site"));
        }
        out
    };

    // ---- court ----
    let mut leaf0_evidence = None;
    if wants(St::Court) || wants(St::Availability) {
        let t = Instant::now();
        let mut failures = Vec::new();
        for (leaf, why) in &sampled {
            match palw_leaf_evidence_from_capture_v1(backend, &capture, &ids, roots, work_leaves, *leaf, form) {
                Ok(evidence) => {
                    let verdict = evidence.verdict_v1(class_id, artifact_root, ladder);
                    let name = match &verdict {
                        Ok(v) => verdict_name(v),
                        Err(e) => format!("refused: {e}"),
                    };
                    if name != "FalseAccusation" {
                        failures.push(format!("honest leaf {leaf}: {name}"));
                    }
                    f.court.push(PalwContextCourtLeafV1 {
                        leaf: *leaf,
                        why,
                        evidence_bytes: palw_leaf_evidence_bytes_v1(&evidence),
                        verdict: name,
                    });
                    if *leaf == 0 {
                        leaf0_evidence = Some(evidence);
                    }
                }
                Err(e) => failures.push(format!("leaf {leaf}: the executor built no evidence: {e}")),
            }
        }
        if wants(St::Court) {
            if work_leaves > PALW_CONTEXT_TAMPER_MAX_LEAVES_V1 {
                failures.push(String::new());
                failures.pop();
                f.verdicts.insert(
                    St::Court,
                    V::Skipped(format!(
                        "the honest leaves ran ({}); the tampered half re-executes the job dense and {work_leaves} leaves is past \
                         {PALW_CONTEXT_TAMPER_MAX_LEAVES_V1}",
                        if failures.is_empty() { "every one false-accusation" } else { "with failures" }
                    )),
                );
            } else {
                match tamper_stage_v1(&class, backend, &ctx, &prompt, &ids, &output_ids, class_id, artifact_root, ladder, form) {
                    Ok(t) => {
                        if t.verdict != "ExecutorGuilty" || t.named != Some(t.leaf) || !t.seat.starts_with("Fault") {
                            failures
                                .push(format!("the tampered capture: seat {}, named {:?}, verdict {}", t.seat, t.named, t.verdict));
                        }
                        f.tamper = Some(t);
                    }
                    Err(e) => failures.push(format!("the tampered capture: {e}")),
                }
                f.verdicts.insert(St::Court, if failures.is_empty() { V::Pass } else { V::Fail(failures.join("; ")) });
            }
        }
        f.stage_ms.insert(St::Court, t.elapsed().as_millis() as u64);
    }

    // ---- availability ----
    if wants(St::Availability) {
        let t = Instant::now();
        let root = run.outcome.execution_root;
        let check = |missing: PalwHeldMissingV1, disclosure: Result<PalwHeldDisclosureV1, String>| -> PalwContextAnswerV1 {
            let unit = format!("{missing:?}");
            match disclosure {
                Ok(d) => PalwContextAnswerV1 {
                    unit,
                    answer_bytes: kaspa_consensus_core::palw_held_da_v1::palw_held_da_bytes_v1(&d),
                    accepted: palw_held_da_check_disclosure_v1(&root, &missing, &binding, &d, ladder, vector.prompt_ids_form)
                        .map_err(|e| e.to_string()),
                },
                Err(why) => PalwContextAnswerV1 { unit, answer_bytes: 0, accepted: Err(format!("unanswered: {why}")) },
            }
        };
        // The last tile of the prompt.
        let tile_len = kaspa_consensus_core::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN;
        let last_tile = (vector.prefill.saturating_sub(1)) / tile_len.max(1);
        f.availability.push(check(
            PalwHeldMissingV1::PromptIdsTile { tile: last_tile },
            kaspa_consensus_core::palw_prompt_ids_v1::prompt_ids_opening_v1(&ids, last_tile * tile_len)
                .map(|opening| PalwHeldDisclosureV1::PromptIdsTile { opening })
                .map_err(|e| e.to_string()),
        ));
        // The first chunk of the checkpoint the second interval resumes from, and its last.
        let checkpoint = binding.checkpoint_count.saturating_sub(1).min(1);
        for chunk in [0u32, u32::MAX] {
            let chunk = if chunk == u32::MAX {
                match backend.held_state_chunk_answer_v1(&capture, &ids, checkpoint, 0) {
                    Ok((anchor, _)) => anchor.leaf.state_chunk_count.saturating_sub(1),
                    Err(_) => continue,
                }
            } else {
                chunk
            };
            let answer = backend
                .held_state_chunk_answer_v1(&capture, &ids, checkpoint, chunk)
                .map(|(anchor, chunk)| PalwHeldDisclosureV1::StateChunk { anchor, chunk });
            f.availability.push(check(PalwHeldMissingV1::StateChunk { checkpoint, chunk }, answer));
        }
        // The widest range the court admits, ending at the last leaf.
        let count = (kaspa_consensus_core::palw_held_da_v1::PALW_HELD_DA_MAX_RANGE_LEAVES_V1 as u64).min(work_leaves);
        let first = work_leaves - count;
        let answer = backend
            .held_step_range_answer_v1(&capture, &ids, first, count as u32)
            .map(|opening| PalwHeldDisclosureV1::StepRange { opening });
        f.availability.push(check(PalwHeldMissingV1::StepRange { first, count: count as u32 }, answer));
        // A leaf's evidence (ADR-0111): the unit's answer, and the one verdict over it.
        let answer = leaf0_evidence
            .clone()
            .ok_or_else(|| "the executor built no evidence of leaf 0".to_string())
            .map(|evidence| PalwHeldDisclosureV1::StepLeaf { evidence: Box::new(evidence) });
        f.availability.push(check(PalwHeldMissingV1::StepLeaf { leaf: 0 }, answer));
        let refused: Vec<String> =
            f.availability.iter().filter_map(|a| a.accepted.as_ref().err().map(|e| format!("{}: {e}", a.unit))).collect();
        f.verdicts.insert(St::Availability, if refused.is_empty() { V::Pass } else { V::Fail(refused.join("; ")) });
        f.stage_ms.insert(St::Availability, t.elapsed().as_millis() as u64);
    }
    f
}

/// **The court's second half**: the same job re-executed with the drill's one-tile fault at leaf
/// 0, retained dense; its interval 0 verified by a seat (a fault), the block's leaves served and
/// the leaf named from them (ADR-0086 Decision 6), and the executor's evidence at that leaf judged
/// by the one verdict (ADR-0111 Decision 1).
#[allow(clippy::too_many_arguments)]
fn tamper_stage_v1(
    class: &VectorClassV1,
    backend: &dyn PalwExecutionBackendV1,
    ctx: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
    prompt: &[usize],
    ids: &[u32],
    output_ids: &[u32],
    class_id: Hash64,
    artifact_root: Hash64,
    ladder: u64,
    form: PalwPromptIdsFormV1,
) -> Result<PalwContextTamperV1, String> {
    let _ = class;
    let leaf = 0u64;
    let lying = backend.execute_with_injected_fault(ctx, prompt, leaf)?;
    let capture = &lying.material;
    let binding = crate::produce::base0_material_decode_any_v1(capture).map_err(|e| format!("{e:?}"))?.binding().clone();
    let roots = PalwClaimRootsV1 {
        execution_root: lying.execution_root,
        trace_root: lying.trace_root,
        anchor: ctx.job_id,
        attempt_draw: None,
    };
    let work_leaves = binding.step_leaf_count;
    let opening = backend.open_fp_interval(capture, 0, ids)?;
    let verdict = backend.verify_fp_interval_opening(&opening, roots, 0, ids, work_leaves);
    let named = match verdict {
        PalwFpIntervalVerdictV1::Fault { leaf_index } => Some(leaf_index),
        PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, .. } => {
            let v4 = crate::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).map_err(|e| format!("{e:?}"))?;
            let block = first_leaf_index >> v4.range.retain_level;
            let served = backend.open_fp_block_leaves(capture, 0, block, ids)?;
            backend.fp_name_the_leaf_v1(&opening, &served, roots, 0, ids, output_ids, work_leaves)?
        }
        _ => None,
    };
    let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
        backend,
        capture,
        ids,
        roots,
        work_leaves,
        leaf,
        form,
    )?;
    let judged = evidence.verdict_v1(class_id, artifact_root, ladder);
    Ok(PalwContextTamperV1 {
        leaf,
        seat: verdict_name(&verdict),
        named,
        evidence_bytes: kaspa_consensus_core::palw_shard_court_v1::palw_leaf_evidence_bytes_v1(&evidence),
        verdict: match judged {
            Ok(v) => verdict_name(&v),
            Err(e) => format!("refused: {e}"),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Invariant 1: the generator has no free parameter.** Two derivations of one vector agree
    /// byte for byte — its id, its job, its class — and a vector read back from its own document is
    /// the vector.
    #[test]
    fn a_vector_is_a_pure_function_of_its_name_seed_and_geometry() {
        let ruleset = PalwContextRulesetV1::devnet_held_v1().expect("the held devnet");
        let v = palw_context_vector_v1("0110-dense-v7-512").expect("shipped");
        assert_eq!(v.seed, palw_context_vector_seed_v1("0110-dense-v7-512"));
        assert_eq!(v.vector_id(), palw_context_vector_v1("0110-dense-v7-512").unwrap().vector_id());
        let class = vector_class_v1(&v, &ruleset).expect("the class builds");
        let (job_a, prompt_a) = vector_job_v1(&v, &ruleset, &class.profile).expect("a job");
        let (job_b, prompt_b) = vector_job_v1(&v, &ruleset, &class.profile).expect("a job");
        assert_eq!((job_a, prompt_a), (job_b, prompt_b));
        let back = PalwContextVectorV1::from_json_v1(&v.to_json_v1()).expect("reads back");
        assert_eq!(back, v);
        assert!(back.shipped);
        let mut edited = v.to_json_v1();
        edited["job"]["prefill"] = serde_json::json!(v.prefill - 1);
        let other = PalwContextVectorV1::from_json_v1(&edited).expect("reads");
        assert!(!other.shipped, "a vector that differs from the shipped one of its name is not shipped");
        assert_ne!(other.vector_id(), v.vector_id());
        let names: Vec<String> = palw_context_vectors_v1().into_iter().map(|v| v.name).collect();
        let ids: std::collections::BTreeSet<Hash64> = palw_context_vectors_v1().iter().map(|v| v.vector_id()).collect();
        assert_eq!(ids.len(), names.len(), "every shipped vector is its own");
    }

    /// The canonical form: sorted keys, no whitespace, JSON's escapes.
    #[test]
    fn the_canonical_form_sorts_and_escapes() {
        let v = serde_json::json!({ "b": 1, "a": [true, null, "x\"\n\r\t\u{1}"], "A": { "z": -2, "y": "é" } });
        assert_eq!(
            String::from_utf8(palw_canonical_json_v1(&v)).unwrap(),
            "{\"A\":{\"y\":\"é\",\"z\":-2},\"a\":[true,null,\"x\\\"\\n\\r\\t\\u0001\"],\"b\":1}"
        );
    }

    /// Run every stage over a shipped vector under the held devnet and check the document it
    /// yields against its pin: every stage passes, and the id that covers the consensus facts and
    /// the verdicts is the one this tree has always produced for it (Decision 5, invariant 2).
    fn check_pinned(name: &str, pinned_document_id: &str) {
        let ruleset = PalwContextRulesetV1::devnet_held_v1().expect("the held devnet");
        let vector = palw_context_vector_v1(name).expect("shipped");
        let f = palw_verify_context_vector_v1(&vector, &ruleset, &PalwContextStageV1::ALL);
        for (stage, verdict) in &f.verdicts {
            assert_eq!(verdict, &PalwContextVerdictV1::Pass, "{name}: stage {} did not pass", stage.name());
        }
        assert_eq!(f.verdicts.len(), PalwContextStageV1::ALL.len(), "{name}: every stage ran");
        let tamper = f.tamper.as_ref().expect("the court's tampered half ran");
        assert_eq!((tamper.named, tamper.verdict.as_str()), (Some(0), "ExecutorGuilty"), "{name}: invariant 4");
        assert!(f.court.iter().all(|l| l.verdict == "FalseAccusation"), "{name}: honest leaves clear");
        assert_eq!(
            f.document_id().to_string(),
            pinned_document_id,
            "{name}: the document moved — a root, a count, a size or a verdict changed; if on purpose, re-pin with the reason \
             (the document: {})",
            String::from_utf8(palw_canonical_json_v1(&f.agreed_json_v1())).unwrap()
        );
    }

    /// **ADR-0116: the 2M vector is inside the held bound, and one row past the bound is refused
    /// by name before a position runs.** Every vector runs the held row, so its bound is the
    /// regime's 2^21: `0110-dense-v7-2m`'s last position attends to 2,097,151 rows, inside it, and
    /// so do 262,145 rows — one past the old eighth wall (ADR-0103 §10.7). A job one row past the
    /// held bound is refused up front, its document saying why produce failed, skipping the four
    /// stages that need a run and still reporting the fit, in the time the fit takes.
    #[test]
    fn the_2m_vector_is_inside_the_held_bound_and_one_row_past_it_is_refused_by_name() {
        let wide = palw_context_vector_v1("0110-dense-v7-256k").expect("shipped");
        let mut past_the_old_wall = wide.clone();
        past_the_old_wall.prefill += 2;
        assert_eq!(past_the_old_wall.prefill + past_the_old_wall.decode - 1, 262_145);
        assert_eq!(palw_context_vector_blocked_v1(&past_the_old_wall), None, "one past 2^18 is inside the held bound");

        let two_m = palw_context_vector_v1("0110-dense-v7-2m").expect("shipped");
        assert_eq!(two_m.prefill + two_m.decode - 1, 2_097_151);
        assert_eq!(palw_context_vector_blocked_v1(&two_m), None, "the regime's own width runs");
        let mut at_the_bound = two_m.clone();
        at_the_bound.prefill += 1;
        assert_eq!(palw_context_vector_blocked_v1(&at_the_bound), None, "2^21 rows is the bound itself");

        // A context two positions wider, so the job itself is well formed and only the bound
        // refuses it.
        let mut one_past = two_m.clone();
        one_past.geometry.n_ctx += 2;
        one_past.prefill += 2;
        let why = palw_context_vector_blocked_v1(&one_past).expect("2^21 + 1 rows is past it");
        assert!(why.contains("ADR-0116") && why.contains("2097153") && why.contains("2097152"), "{why}");
        let ruleset = PalwContextRulesetV1::devnet_held_v1().expect("the held devnet");
        let t = Instant::now();
        let f = palw_verify_context_vector_v1(&one_past, &ruleset, &PalwContextStageV1::ALL);
        assert!(t.elapsed().as_secs() < 60, "refused up front, not after a produce: {:?}", t.elapsed());
        match f.verdicts.get(&PalwContextStageV1::Produce) {
            Some(PalwContextVerdictV1::Fail(why)) => assert!(why.contains("ADR-0116"), "{why}"),
            other => panic!("produce must fail by name, got {other:?}"),
        }
        for stage in
            [PalwContextStageV1::Commit, PalwContextStageV1::Seat, PalwContextStageV1::Court, PalwContextStageV1::Availability]
        {
            assert!(matches!(f.verdicts.get(&stage), Some(PalwContextVerdictV1::Skipped(_))), "{}: skipped by name", stage.name());
        }
        assert!(f.verdicts.contains_key(&PalwContextStageV1::Fit), "the fit reads no run and still reports");
    }

    /// **The 512-position vector, in the default suite.**
    #[test]
    fn the_512_vector_passes_every_stage_and_is_pinned() {
        check_pinned(
            "0110-dense-v7-512",
            "d236461504f657edcdd1fb94b79b829f3bf011364154b2bd829b7f67edbf02dca0ac63c1407a08d5721ca96f7a120bdba9c5e7c9e97cc99fc0a4ec8076d92995",
        );
    }

    /// **The 4,096-position vector** — the release-mode vector job
    /// (`cargo test --release -p misaka-palw-base0 --lib -- --ignored context_vector`).
    ///
    /// Re-pinned 2026-09-11 for ADR-0103 §10.6: with the history priced, this thin row's whole
    /// prefix no longer fits the held devnet's seat budget at the family's rate, so its seat takes
    /// the Resume route. The document's `seat.route` and each interval's `resume_bytes` moved.
    /// Nothing else did: the roots are produced before the seat runs and never read the route,
    /// and every count, size and verdict still reads as ADR-0110 §9.2's table.
    #[test]
    #[ignore = "the release-mode vector job: about 20 s in release, minutes in debug"]
    fn the_4k_vector_passes_every_stage_and_is_pinned() {
        check_pinned(
            "0110-dense-v7-4k",
            "9c5a772cddbcba9eef861cdca1074dc8447cbecf1e295bbd2e9242d341105fc250c742a7806f2bd118a41733b4bc5ae5d3bd5714a939fc39775798120aa1c49e",
        );
    }

    /// **The 32,768-position vector** — the release-mode vector job, about seven minutes on an
    /// M-series host (ADR-0110 §9 has the measured stages). Re-pinned with the 4,096 one, for the
    /// same reason.
    #[test]
    #[ignore = "the release-mode vector job: about seven minutes in release"]
    fn the_32k_vector_passes_every_stage_and_is_pinned() {
        check_pinned(
            "0110-dense-v7-32k",
            "848d9352a21e0a57ac575f4365056fccdd65095494119cc535e1f2d061a6f3df4241c23adb31bd4931a0a86d54280ba4f67d06bc09f2556dd2ba7c4b46cb3998",
        );
    }
}
