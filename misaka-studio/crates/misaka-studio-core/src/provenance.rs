//! **What ran, exactly** — the identities that let an answer be re-derived later by somebody who
//! was not here.
//!
//! MISAKA's long path is `Inference → Deterministic Execution → Inference Hash → Verification →
//! Compute Credit → PALW → MISAKA Network`. This version implements the first and third links and
//! nothing beyond them: no chain client, no bond, no credit. That is the whole design decision.
//! Verification is additive — it can be built on top of a record that already exists — but a
//! record cannot be reconstructed after the fact. An app that does not write down which artifact
//! answered a prompt has destroyed the evidence, and no later release can recover it.
//!
//! # Why these exact derivations
//!
//! [`derive_model_weights_hash`], [`derive_runtime_hash`] and [`derive_runtime_class_id`] are
//! **the consensus derivations**, not Studio-shaped imitations: same keyed BLAKE2b-512, same
//! domain keys, same field order as `kaspa_consensus_core::vlt`. A model the Studio downloads
//! therefore already carries the `h_M` the chain would compute for it, and a runtime the Studio
//! drives carries the `h_R` and class id a validator would register.
//!
//! They are duplicated rather than imported because the alternative is worse: depending on
//! `kaspa-consensus-core` would drag the node's entire dependency graph — RocksDB, the P2P stack,
//! ML-DSA — into a desktop app's build, on three platforms, for five hashes. What keeps the copy
//! honest is that the vectors in this module's tests were produced by a third implementation
//! (Python's `hashlib.blake2b`), so agreement is a real cross-check rather than a copy agreeing
//! with itself.
//!
//! # What is NOT in the inference hash
//!
//! Wall-clock time, tokens per second, the machine's name. All of it is recorded and none of it
//! is committed, because a verifier re-running the same job on different hardware must produce
//! the same hash. A digest that included timing would fail every honest replay.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Consensus domain key for [`derive_model_weights_hash`].
pub const MODEL_IDENTITY_KEY: &[u8] = b"misaka-vlt-model-identity-v1";
/// Consensus domain key for [`derive_runtime_hash`].
pub const RUNTIME_IDENTITY_KEY: &[u8] = b"misaka-vlt-runtime-identity-v1";
/// Consensus domain key for [`derive_runtime_class_id`].
pub const RUNTIME_CLASS_KEY: &[u8] = b"misaka-vlt-runtime-class-v1";

/// Studio-local domains. Deliberately under a different prefix from the `misaka-vlt-*` keys
/// above: these commitments are the app's own, and a value it invents must never be mistakable
/// for one consensus defined.
pub const STUDIO_PROMPT_KEY: &[u8] = b"misaka-studio/prompt/v1";
pub const STUDIO_OUTPUT_KEY: &[u8] = b"misaka-studio/output/v1";
pub const STUDIO_PARAMS_KEY: &[u8] = b"misaka-studio/params/v1";
pub const STUDIO_INFERENCE_KEY: &[u8] = b"misaka-studio/inference/v1";

/// A 64-byte BLAKE2b digest, hex in JSON.
///
/// 64 bytes because that is the identity space the overlay works in (`Hash64`), not 32: a
/// Studio-side digest that had to be widened before the chain could read it would be a
/// conversion step, and conversion steps are where two implementations stop agreeing.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Digest64(pub [u8; 64]);

impl Digest64 {
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let bytes = hex::decode(s).ok()?;
        let arr: [u8; 64] = bytes.try_into().ok()?;
        Some(Digest64(arr))
    }

    /// First 8 bytes, for logs and UI. Never for equality.
    pub fn short(self) -> String {
        hex::encode(&self.0[..8])
    }
}

impl fmt::Debug for Digest64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}…", self.short())
    }
}

impl fmt::Display for Digest64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Digest64 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Digest64 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Digest64::from_hex(&s).ok_or_else(|| serde::de::Error::custom("expected 128 hex characters"))
    }
}

fn keyed(key: &[u8], parts: &[&[u8]]) -> Digest64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for p in parts {
        state.update(p);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Digest64(out)
}

/// `h_M` for a GGUF-distributed model: keyed digest over the content digest, size, filename and
/// the base-metadata revision the GGUF was converted from.
///
/// The repository and revision are in the identity because two conversions of the same weights
/// with different tokenizer revisions are different functions from prompt text to tokens — the
/// same reason consensus binds them.
pub fn derive_model_weights_hash(
    gguf_sha256_hex: &str,
    gguf_size: u64,
    filename: &str,
    base_repo: &str,
    base_revision: &str,
) -> Digest64 {
    keyed(
        MODEL_IDENTITY_KEY,
        &[gguf_sha256_hex.as_bytes(), &gguf_size.to_le_bytes(), filename.as_bytes(), base_repo.as_bytes(), base_revision.as_bytes()],
    )
}

/// `h_R` — the identity of the code that ran, down to the build flags.
///
/// The build profile is in the digest because it changes results: fused multiply-add, BLAS
/// dispatch and LTO all reorder floating-point reductions, so two binaries from one commit with
/// different flags are two different functions.
pub fn derive_runtime_hash(commit: &str, patch_sha256_hex: &str, build_number: u64, build_profile: &str) -> Digest64 {
    keyed(
        RUNTIME_IDENTITY_KEY,
        &[commit.as_bytes(), patch_sha256_hex.as_bytes(), &build_number.to_le_bytes(), build_profile.as_bytes()],
    )
}

/// The determinism **class** — the set of runtimes expected to agree bit-for-bit with each other.
pub fn derive_runtime_class_id(class_tag: &str) -> Digest64 {
    keyed(RUNTIME_CLASS_KEY, &[class_tag.as_bytes()])
}

/// A model, as an identity plus the facts it was derived from.
///
/// The inputs travel with the digest so anyone can re-derive it. A bare hash is unfalsifiable;
/// a hash with its preimage is checkable in one line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub h_m: Digest64,
    pub gguf_sha256: String,
    pub gguf_size: u64,
    pub filename: String,
    pub base_repo: String,
    pub base_revision: String,
}

impl ModelIdentity {
    pub fn derive(gguf_sha256: &str, gguf_size: u64, filename: &str, base_repo: &str, base_revision: &str) -> Self {
        ModelIdentity {
            h_m: derive_model_weights_hash(gguf_sha256, gguf_size, filename, base_repo, base_revision),
            gguf_sha256: gguf_sha256.to_string(),
            gguf_size,
            filename: filename.to_string(),
            base_repo: base_repo.to_string(),
            base_revision: base_revision.to_string(),
        }
    }

    /// Re-derive and compare. What a verifier does with a published record.
    pub fn verify(&self) -> bool {
        self.h_m == derive_model_weights_hash(&self.gguf_sha256, self.gguf_size, &self.filename, &self.base_repo, &self.base_revision)
    }
}

/// Everything about the executing code that changes its output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    /// Which backend drove the model: `llamacpp`, `mlx`, `mock`.
    pub backend: String,
    /// Upstream commit of the engine, when it identifies itself. `unknown` is recorded as such —
    /// an engine that will not say what it is produces a record that says so.
    pub engine_commit: String,
    /// SHA-256 of any patch applied on top, or the literal `unpatched`.
    ///
    /// The literal is load-bearing: an empty string would let a patched and an unpatched build
    /// collide in the identity, so "no patch" has to be a value, not an absence.
    pub engine_patch_sha256: String,
    pub engine_build_number: u64,
    /// Canonical build-profile tag: target, accelerator, and the flags that move arithmetic.
    pub build_profile: String,
    /// Determinism class — the set this runtime is expected to agree with bit-for-bit.
    pub class_tag: String,
}

/// A runtime, as its two consensus identities plus the descriptor they came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeIdentity {
    pub h_r: Digest64,
    pub class_id: Digest64,
    pub descriptor: RuntimeDescriptor,
}

impl RuntimeIdentity {
    pub fn derive(descriptor: RuntimeDescriptor) -> Self {
        RuntimeIdentity {
            h_r: derive_runtime_hash(
                &descriptor.engine_commit,
                &descriptor.engine_patch_sha256,
                descriptor.engine_build_number,
                &descriptor.build_profile,
            ),
            class_id: derive_runtime_class_id(&descriptor.class_tag),
            descriptor,
        }
    }

    pub fn verify(&self) -> bool {
        self.h_r
            == derive_runtime_hash(
                &self.descriptor.engine_commit,
                &self.descriptor.engine_patch_sha256,
                self.descriptor.engine_build_number,
                &self.descriptor.build_profile,
            )
            && self.class_id == derive_runtime_class_id(&self.descriptor.class_tag)
    }
}

/// The sampling settings, exactly as the runtime received them.
///
/// Floats are committed by their IEEE-754 bits rather than by a formatted string: `0.7` prints
/// differently in four languages and rounds differently in three, and a commitment that depends
/// on a formatter is a commitment to the formatter.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SamplingCommitment {
    pub temperature: f64,
    pub top_p: f64,
    pub top_k: i64,
    pub min_p: f64,
    pub repeat_penalty: f64,
    pub max_tokens: u64,
    /// The RNG seed. `None` means the runtime chose one, which is exactly the case that makes a
    /// run unrepeatable — see [`Replayability`].
    pub seed: Option<u64>,
}

impl Default for SamplingCommitment {
    fn default() -> Self {
        SamplingCommitment { temperature: 0.7, top_p: 0.95, top_k: 40, min_p: 0.05, repeat_penalty: 1.1, max_tokens: 2048, seed: None }
    }
}

impl SamplingCommitment {
    pub fn commitment(&self) -> Digest64 {
        keyed(
            STUDIO_PARAMS_KEY,
            &[
                &self.temperature.to_bits().to_le_bytes(),
                &self.top_p.to_bits().to_le_bytes(),
                &self.top_k.to_le_bytes(),
                &self.min_p.to_bits().to_le_bytes(),
                &self.repeat_penalty.to_bits().to_le_bytes(),
                &self.max_tokens.to_le_bytes(),
                // Absent seed and seed 0 must not collide: the tag distinguishes them.
                &[self.seed.is_some() as u8],
                &self.seed.unwrap_or(0).to_le_bytes(),
            ],
        )
    }

    /// Whether a replay could be expected to reproduce this run.
    pub fn replayability(&self) -> Replayability {
        if self.temperature == 0.0 {
            // Greedy decoding needs no seed: the argmax is the argmax.
            Replayability::Deterministic
        } else if self.seed.is_some() {
            Replayability::SeededSampling
        } else {
            Replayability::Unrepeatable
        }
    }
}

/// How reproducible a recorded run is — recorded honestly, including when the answer is "not".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Replayability {
    /// Greedy decoding: the same runtime on the same class reproduces it exactly.
    Deterministic,
    /// Sampling with a recorded seed: reproducible on the same runtime, since the RNG stream is
    /// pinned. Still class-scoped — a different arithmetic class can diverge on a near-tie.
    SeededSampling,
    /// Sampling with a runtime-chosen seed. Nothing reproduces this, and the record says so
    /// rather than implying a verifiability it does not have.
    Unrepeatable,
}

/// Commit to a byte string under a Studio domain.
pub fn commit_prompt(bytes: &[u8]) -> Digest64 {
    keyed(STUDIO_PROMPT_KEY, &[&(bytes.len() as u64).to_le_bytes(), bytes])
}

/// The canonical bytes of a conversation, for [`commit_prompt`].
///
/// **Length-prefixed, and that is the whole point.** The obvious encoding — `role: content`, one
/// per line — lets two different conversations produce identical bytes: a single user message
/// reading `a\nassistant:b` and the two-message exchange `[user "a", assistant "b"]` flatten to
/// the same string, so one could be committed and the other claimed. Prefixing every field with
/// its length makes the encoding injective, which is the minimum a commitment has to be.
///
/// # What this commits to, precisely
///
/// The **conversation as the runtime received it** — not the token sequence the engine ran. The
/// chat template that turns one into the other lives inside the GGUF, and `h_M` binds the GGUF;
/// the tokenizer does too, for the same reason. So `(h_M, prompt_commitment)` determines the
/// tokens without this layer having to re-implement a template it would only get subtly wrong.
pub fn canonical_prompt_bytes(messages: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(messages.len() as u64).to_le_bytes());
    for (role, content) in messages {
        out.extend_from_slice(&(role.len() as u64).to_le_bytes());
        out.extend_from_slice(role.as_bytes());
        out.extend_from_slice(&(content.len() as u64).to_le_bytes());
        out.extend_from_slice(content.as_bytes());
    }
    out
}

/// The canonical bytes of a raw completion prompt.
///
/// Tagged apart from a chat conversation so that a `/v1/completions` request and a
/// `/v1/chat/completions` request carrying the same text are different commitments — they are
/// different jobs, and the engine renders them differently.
pub fn canonical_raw_prompt_bytes(prompt: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&u64::MAX.to_le_bytes()); // a message count no conversation can have
    out.extend_from_slice(&(prompt.len() as u64).to_le_bytes());
    out.extend_from_slice(prompt.as_bytes());
    out
}

pub fn commit_output(bytes: &[u8]) -> Digest64 {
    keyed(STUDIO_OUTPUT_KEY, &[&(bytes.len() as u64).to_le_bytes(), bytes])
}

/// One completed inference: the identities, the commitments, and the measurements.
///
/// Everything above `inference_hash` is committed; everything below it is not. The split is the
/// contract — a verifier re-running this job on other hardware must reach the same
/// `inference_hash`, so hardware-dependent facts stay outside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InferenceRecord {
    pub id: String,
    /// `None` when the model's file has not been hashed yet — the run happened, but it cannot be
    /// attributed to a specific artifact, and that is stated rather than papered over.
    pub model: Option<ModelIdentity>,
    pub runtime: RuntimeIdentity,
    pub params: SamplingCommitment,
    pub prompt_commitment: Digest64,
    pub output_commitment: Digest64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// The commitment this whole record reduces to. The value a future verification layer
    /// publishes, disputes and pays against.
    pub inference_hash: Digest64,
    pub replayability: Replayability,

    // --- measured, never committed ---
    pub started_at_unix_ms: u64,
    pub duration_ms: u64,
    /// Time to the first token — the latency a person actually feels.
    pub time_to_first_token_ms: Option<u64>,
    pub tokens_per_second: f64,
}

/// The inputs to a record, before the hash exists.
pub struct InferenceInputs<'a> {
    pub model: Option<&'a ModelIdentity>,
    pub runtime: &'a RuntimeIdentity,
    pub params: SamplingCommitment,
    pub prompt: &'a [u8],
    pub output: &'a [u8],
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub started_at_unix_ms: u64,
    pub duration_ms: u64,
    pub time_to_first_token_ms: Option<u64>,
}

impl InferenceRecord {
    pub fn new(id: impl Into<String>, inputs: InferenceInputs<'_>) -> Self {
        let prompt_commitment = commit_prompt(inputs.prompt);
        let output_commitment = commit_output(inputs.output);
        let params_commitment = inputs.params.commitment();
        // A run with no model identity commits a zero digest in its place — a distinct,
        // deliberate value rather than an omission that would shorten the preimage and let two
        // different records collide.
        let h_m = inputs.model.map(|m| m.h_m).unwrap_or(Digest64([0u8; 64]));

        let inference_hash = keyed(
            STUDIO_INFERENCE_KEY,
            &[
                &h_m.0,
                &inputs.runtime.h_r.0,
                &inputs.runtime.class_id.0,
                &params_commitment.0,
                &prompt_commitment.0,
                &output_commitment.0,
                &inputs.prompt_tokens.to_le_bytes(),
                &inputs.completion_tokens.to_le_bytes(),
            ],
        );

        let tokens_per_second =
            if inputs.duration_ms > 0 { inputs.completion_tokens as f64 * 1000.0 / inputs.duration_ms as f64 } else { 0.0 };

        InferenceRecord {
            id: id.into(),
            model: inputs.model.cloned(),
            runtime: inputs.runtime.clone(),
            params: inputs.params,
            prompt_commitment,
            output_commitment,
            prompt_tokens: inputs.prompt_tokens,
            completion_tokens: inputs.completion_tokens,
            inference_hash,
            replayability: inputs.params.replayability(),
            started_at_unix_ms: inputs.started_at_unix_ms,
            duration_ms: inputs.duration_ms,
            time_to_first_token_ms: inputs.time_to_first_token_ms,
            tokens_per_second,
        }
    }

    /// Re-derive the hash from the record's own fields.
    ///
    /// This is the interface the later stages plug into: a verifier holding a record and the
    /// prompt/output bytes checks the commitments, then this. Nothing else about the Studio has
    /// to change for that to work.
    pub fn verify(&self, prompt: &[u8], output: &[u8]) -> bool {
        if commit_prompt(prompt) != self.prompt_commitment || commit_output(output) != self.output_commitment {
            return false;
        }
        let h_m = self.model.as_ref().map(|m| m.h_m).unwrap_or(Digest64([0u8; 64]));
        let expected = keyed(
            STUDIO_INFERENCE_KEY,
            &[
                &h_m.0,
                &self.runtime.h_r.0,
                &self.runtime.class_id.0,
                &self.params.commitment().0,
                &self.prompt_commitment.0,
                &self.output_commitment.0,
                &self.prompt_tokens.to_le_bytes(),
                &self.completion_tokens.to_le_bytes(),
            ],
        );
        expected == self.inference_hash && self.model.as_ref().map(ModelIdentity::verify).unwrap_or(true) && self.runtime.verify()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vectors produced by an INDEPENDENT implementation — Python's `hashlib.blake2b(key=…,
    // digest_size=64)` — over the pins in `consensus/core/src/vlt.rs`. That is what makes this a
    // cross-check: if this module and the node's `vlt.rs` had been compared to each other, a
    // shared misreading of the spec would agree with itself.
    const QWEN36_H_M: &str = "36bbca9a5a77896f6b88cf5f51f31afb97eb371ddb7b1602a0613cf730e7a58f\
                              7830c8948904e7d3ed5d20af8ef9c8ad00f2b75aa857abc291136802db7d43df";
    const QWEN35_H_M: &str = "6cfeba30273cdc5b6d17daa6ace5f08d9ad112fddd1b4b757db111257f4e0b2c\
                              f79c99b7a7434a4ca80d28ce97a764d0eef5c4e360443ec372adb6b85fd9ccbf";
    const QWEN36_H_R: &str = "21f464064df389cd81515767970f554080fdf361d5c02e77f9a4c3e2b4f48891\
                              dca39caebfae8c972f923d53295dcbc400bc76fd9690698e25736e086787c30e";
    const METAL_CLASS: &str = "a02b03a2651e4d809e70de0fa803ed45bd321e44d1bf6317e42f9556e5c4a3e8\
                               37b283181a5daed440ca7e6f20dcf284457ecd414ce186d2fa065eade83d3000";

    #[test]
    fn model_identity_matches_the_consensus_pins() {
        let h_m = derive_model_weights_hash(
            "1dc494614bee8a3bc00e79fe5a49da0fc1c36b3b118c4156e223e98e5a0a671b",
            23_938_321_728,
            "Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf",
            "huihui-ai/Huihui-Qwen3.6-35B-A3B-Claude-4.7-Opus-abliterated",
            "ac18882735d037f6074a7630eb68d85db8234c25",
        );
        assert_eq!(h_m.to_hex(), QWEN36_H_M);

        let lite = derive_model_weights_hash(
            "aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223",
            1_280_835_840,
            "Qwen3.5-2B-Q4_K_M.gguf",
            "Qwen/Qwen3.5-2B",
            "15852e8c16360a2fea060d615a32b45270f8a8fc",
        );
        assert_eq!(lite.to_hex(), QWEN35_H_M);
    }

    #[test]
    fn runtime_identity_and_class_match_the_consensus_pins() {
        let h_r = derive_runtime_hash(
            "12127defda4f41b7679cb2477a4b0d65ee6a0c8f",
            "d155a88b7c11ee74f48011760cb1a37773a694c8cab28258ee108c85e2f9e02c",
            10_015,
            "release/arm64/metal-embed/no-native/no-lto/no-kleidiai/accelerate-blas-apple/cuda-off/shared",
        );
        assert_eq!(h_r.to_hex(), QWEN36_H_R);
        assert_eq!(derive_runtime_class_id("palw-fp-per-vendor/apple-metal-arm64/v1").to_hex(), METAL_CLASS);
    }

    /// Every field must move the digest. A field that does not is a field an attacker can change
    /// for free.
    #[test]
    fn every_model_field_is_load_bearing() {
        let base = derive_model_weights_hash("aa", 1, "f.gguf", "r", "rev");
        assert_ne!(base, derive_model_weights_hash("ab", 1, "f.gguf", "r", "rev"));
        assert_ne!(base, derive_model_weights_hash("aa", 2, "f.gguf", "r", "rev"));
        assert_ne!(base, derive_model_weights_hash("aa", 1, "g.gguf", "r", "rev"));
        assert_ne!(base, derive_model_weights_hash("aa", 1, "f.gguf", "s", "rev"));
        assert_ne!(base, derive_model_weights_hash("aa", 1, "f.gguf", "r", "rew"));
    }

    fn runtime() -> RuntimeIdentity {
        RuntimeIdentity::derive(RuntimeDescriptor {
            backend: "llamacpp".into(),
            engine_commit: "deadbeef".into(),
            engine_patch_sha256: "unpatched".into(),
            engine_build_number: 42,
            build_profile: "release/x86_64/cuda/v1".into(),
            class_tag: "misaka-studio/x86_64-cuda/v1".into(),
        })
    }

    fn record(params: SamplingCommitment, output: &str) -> InferenceRecord {
        let model = ModelIdentity::derive("abc", 100, "m.gguf", "repo", "rev");
        InferenceRecord::new(
            "run-1",
            InferenceInputs {
                model: Some(&model),
                runtime: &runtime(),
                params,
                prompt: b"why is the sky blue?",
                output: output.as_bytes(),
                prompt_tokens: 6,
                completion_tokens: 12,
                started_at_unix_ms: 1_700_000_000_000,
                duration_ms: 600,
                time_to_first_token_ms: Some(120),
            },
        )
    }

    #[test]
    fn a_record_verifies_against_its_own_bytes() {
        let r = record(SamplingCommitment::default(), "rayleigh scattering");
        assert!(r.verify(b"why is the sky blue?", b"rayleigh scattering"));
        assert!(!r.verify(b"why is the sky blue?", b"mie scattering"), "a different answer must not verify");
        assert!(!r.verify(b"a different question", b"rayleigh scattering"));
    }

    /// The measurements are outside the commitment, so the same job on a faster machine has the
    /// same inference hash. If this ever fails, verification has become hardware-dependent.
    #[test]
    fn timing_does_not_change_the_inference_hash() {
        let mut fast = record(SamplingCommitment::default(), "rayleigh scattering");
        let slow = fast.clone();
        fast.duration_ms = 60_000;
        fast.tokens_per_second = 0.2;
        assert_eq!(fast.inference_hash, slow.inference_hash);
    }

    #[test]
    fn sampling_settings_are_committed() {
        let a = record(SamplingCommitment { temperature: 0.7, ..Default::default() }, "x");
        let b = record(SamplingCommitment { temperature: 0.8, ..Default::default() }, "x");
        assert_ne!(a.inference_hash, b.inference_hash);
    }

    /// Seed 0 and "no seed" are different runs; a commitment that conflated them would let an
    /// unrepeatable run pass as a seeded one.
    #[test]
    fn an_absent_seed_differs_from_seed_zero() {
        let none = SamplingCommitment { seed: None, ..Default::default() };
        let zero = SamplingCommitment { seed: Some(0), ..Default::default() };
        assert_ne!(none.commitment(), zero.commitment());
        assert_eq!(none.replayability(), Replayability::Unrepeatable);
        assert_eq!(zero.replayability(), Replayability::SeededSampling);
        assert_eq!(
            SamplingCommitment { temperature: 0.0, seed: None, ..Default::default() }.replayability(),
            Replayability::Deterministic,
            "greedy decoding needs no seed"
        );
    }

    /// The collision the length prefixes exist for. Without them, `[user "a\nassistant:b"]` and
    /// `[user "a", assistant "b"]` flatten to the same string, and one conversation can be
    /// committed while another is claimed.
    #[test]
    fn two_conversations_cannot_share_a_commitment() {
        let flattened = canonical_prompt_bytes(&[("user", "a\nassistant:b")]);
        let exchange = canonical_prompt_bytes(&[("user", "a"), ("assistant", "b")]);
        assert_ne!(flattened, exchange);
        assert_ne!(commit_prompt(&flattened), commit_prompt(&exchange));

        // Moving a character across the role/content boundary must also change the bytes.
        assert_ne!(canonical_prompt_bytes(&[("us", "era")]), canonical_prompt_bytes(&[("user", "a")]));
    }

    #[test]
    fn a_raw_prompt_is_a_different_job_from_the_same_text_as_a_message() {
        let raw = canonical_raw_prompt_bytes("hello");
        let chat = canonical_prompt_bytes(&[("user", "hello")]);
        assert_ne!(commit_prompt(&raw), commit_prompt(&chat));
    }

    #[test]
    fn the_canonical_encoding_is_stable() {
        // A change here changes every historical commitment, so it is pinned by a vector.
        let bytes = canonical_prompt_bytes(&[("user", "hi")]);
        assert_eq!(
            bytes,
            [1u64.to_le_bytes().as_slice(), 4u64.to_le_bytes().as_slice(), b"user", 2u64.to_le_bytes().as_slice(), b"hi"].concat()
        );
    }

    #[test]
    fn digests_round_trip_through_json() {
        let r = record(SamplingCommitment::default(), "hello");
        let json = serde_json::to_string(&r).expect("serializes");
        let back: InferenceRecord = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(back.inference_hash, r.inference_hash);
        assert!(back.verify(b"why is the sky blue?", b"hello"));
    }
}
