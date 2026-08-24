//! `misaka-palw-reexecutor` core — the pure half of the ADR-0034 §6 agent.
//!
//! The §9.3 sequence the binary automates is: detect backend → resolve family/version → scan
//! local artifacts against bindings → verify memory fit → run goldens → run the replay
//! benchmark → emit the ready set → derive `max_model_band` → emit the signed capability →
//! heartbeat. Every DECISION in that sequence lives in this library as a pure function over
//! explicit inputs, so each one is unit-tested without a model, a worker binary or a node —
//! the binary in `main.rs` only performs IO (drive the worker, hash files, read state, sign)
//! and feeds the results here.
//!
//! Stage-0 discipline (ADR-0034 stage mapping, row 0): capability records are built, signed
//! and written locally — no carriage kind exists for them yet, no value moves, and nothing
//! here can ground an offense. The record layout is versioned so Stage-1 wiring replaces the
//! transport, not the semantics.
//!
//! Fail-closed rules, stated once and tested each:
//! * an unrecognized `runtime_class_id` (no registered tag derives it — checked against the
//!   consensus tag ledger AND the operator's own validated rows) refuses the whole host — a
//!   backend this build cannot NAME cannot be routed honestly;
//! * a binding row that does not `validate()`, does not join its model definition (shape +
//!   profile/artifact join — publisher-SIGNATURE verification is the registry's future act,
//!   not this agent's; Stage-0 record files are operator-trusted inputs), or whose
//!   identities differ from the probed worker in ANY of class/manifest/family/version/model
//!   is inadmissible (each check separate so the refusal names the actual mismatch);
//! * a golden-set failure or a drifting bench root disqualifies — abstaining beats attesting
//!   on a runtime that cannot reproduce its own class's vectors (the quarantine rule);
//! * a host whose measured p99 does not fit a binding's registered replay window at κ — or
//!   the operator's own advertised `max_accepted_replay_secs` — is not ready for THAT
//!   binding (a self-contradictory signed offer is refused at emission, not on the wire);
//! * memory totals must be declared or detected; zero is refusal, never "assume enough";
//! * the capability nonce only moves forward, and exhausting it is an error, not a wrap.

use kaspa_consensus_core::config::params::BlockrateParams;
use kaspa_consensus_core::palw_registry::PalwClassRegistrationV1;
use kaspa_consensus_core::palw_routing::{
    ModelDefinitionV1, PALW_REGISTERED_CLASS_TAGS, PALW_ROUTING_BAND_MEMORY_BASE_BYTES, PALW_ROUTING_OBJECT_VERSION_V1,
    PalwExecutionFamilyV1, PalwModelBandV1, PalwReadyBindingProofV1, PalwVerifierCapabilityV1, binding_matches_definition_v1,
    initial_family_max_active_band_v1, ready_binding_proof_v1, ready_binding_root_v1, routing_keys_for_class_tag_v1,
};
use kaspa_consensus_core::palw_schedule::replay_p99_fits_v1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use kaspa_hashes::Hash64;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------------------------
// Backend naming — the consensus tag ledger, with the operator's own rows as a second witness
// ---------------------------------------------------------------------------------------------

/// Reverse-resolves a derived class id to its registered tag. The worker's `v2-manifest`
/// reports only the derived `runtime_class_id`; naming it needs the preimage. Two sources,
/// both machine-checked, no third copy: (1) the consensus tag ledger
/// [`PALW_REGISTERED_CLASS_TAGS`]; (2) the operator's own binding rows, whose `class_tag` is
/// trustworthy exactly when the row validates — `PalwClassRegistrationV1::validate()`
/// recomputes the class id from the carried tag, so a validated row IS a (tag → id) witness,
/// and a host holding a newly-registered row routes its own class even before the ledger in
/// this binary catches up. `None` = a backend nothing names — the probe refuses the host
/// (fail-closed) rather than guessing a family.
pub fn resolve_class_tag_v1(runtime_class_id: &Hash64, validated_rows: &[PalwClassRegistrationV1]) -> Option<String> {
    if let Some(tag) =
        PALW_REGISTERED_CLASS_TAGS.iter().find(|tag| kaspa_consensus_core::vlt::derive_runtime_class_id(tag) == *runtime_class_id)
    {
        return Some((*tag).to_owned());
    }
    validated_rows.iter().find(|row| row.runtime_class_id == *runtime_class_id).map(|row| row.class_tag.clone())
}

// ---------------------------------------------------------------------------------------------
// Operator policy (the ADR §9.4 TOML surface)
// ---------------------------------------------------------------------------------------------

/// The operator's local policy — everything a human decides stays here; nothing in it can
/// make a dishonest claim on the wire, only narrow what this host offers.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReexecutorPolicyV1 {
    /// Namespace written into signed messages. Stage 0 runs under a drill namespace.
    pub network_id: String,
    /// Window/blockrate set the bindings are validated against: "two-minute" or "deci".
    pub network: String,
    /// Worker binary (the pinned build; its manifest is the probed identity).
    pub worker_bin: String,
    /// Golden set path (exported to the worker as `MISAKA_PALW_GOLDEN`).
    pub golden_set: String,
    /// Candidate model artifacts (exact files, not directories — what this host HOLDS).
    #[serde(default)]
    pub model_paths: Vec<String>,
    /// Binding-row files (`*.bin`, raw Borsh `PalwClassRegistrationV1`).
    #[serde(default)]
    pub binding_paths: Vec<String>,
    /// Model-definition files (`*.bin`, raw Borsh `ModelDefinitionV1`).
    #[serde(default)]
    pub definition_paths: Vec<String>,
    /// Allow globs over the model definition's `model_profile_id` hex (`*` wildcard only).
    /// Empty = allow nothing (fail-closed: offering models is an explicit act).
    #[serde(default)]
    pub allow_models: Vec<String>,
    /// Deny globs, checked BEFORE allow — deny always wins.
    #[serde(default)]
    pub deny_models: Vec<String>,
    /// Highest band this operator offers ("B0".."B4").
    pub max_band: String,
    pub max_concurrency: u16,
    /// Advisory offer terms (ADR-0034 §6: not eligibility conjuncts).
    #[serde(default)]
    pub minimum_reward: u64,
    pub max_accepted_replay_secs: u32,
    /// Total memory this host commits (bytes). 0 = detect; detection failure is an error.
    #[serde(default)]
    pub total_memory_bytes: u64,
    /// Headroom the memory-fit check demands, per mille (200 = a binding's peak must fit
    /// inside 1/1.2 of the total).
    #[serde(default = "default_headroom")]
    pub memory_headroom_permille: u32,
    /// Bench runs per qualification (p99 over these; ≥ 1).
    #[serde(default = "default_bench_runs")]
    pub bench_runs: u32,
    /// Golden job the bench replays (its decode budget is overridden toward the binding's
    /// credited ceiling, capped so prefill + decode fits the worker context — see
    /// [`bench_decode_tokens_v1`]).
    #[serde(default = "default_bench_golden")]
    pub bench_golden_name: String,
    /// The bench golden's declared prefill tokens (12 for `golden-probe-12tok-d16`, 1 for
    /// `golden-min-1tok-d1`). The worker refuses `prefill + decode > context`, so the agent
    /// must know the prefill to pick a decode override that actually runs.
    #[serde(default = "default_bench_golden_prefill")]
    pub bench_golden_prefill_tokens: u32,
    /// The worker's context size (`N_CTX`); the decode override is capped at
    /// `context − prefill`.
    #[serde(default = "default_bench_context")]
    pub bench_context_tokens: u32,
    /// Worker timeouts (seconds). Fixed constants proved wrong on loaded fleet hosts: a
    /// selftest that brushes a hard-coded ceiling records a quarantine for a healthy runtime.
    #[serde(default = "default_selftest_timeout")]
    pub selftest_timeout_secs: u64,
    #[serde(default = "default_bench_timeout")]
    pub bench_timeout_secs: u64,
    /// Capability TTL in DAA scores from issuance.
    pub ttl_daa: u64,
    /// Heartbeat: re-issue the capability every this many seconds while running.
    pub heartbeat_secs: u64,
    /// Declared bond (sompi). Stage 0 has no bond escrow; the declaration is telemetry.
    #[serde(default)]
    pub available_bond: u64,
    /// State directory (qualifications, nonce, emitted capabilities).
    pub state_dir: String,
}

fn default_headroom() -> u32 {
    200
}
fn default_bench_runs() -> u32 {
    3
}
fn default_bench_golden() -> String {
    "golden-probe-12tok-d16".into()
}
fn default_bench_golden_prefill() -> u32 {
    12
}
fn default_bench_context() -> u32 {
    4096
}
fn default_selftest_timeout() -> u64 {
    1800
}
fn default_bench_timeout() -> u64 {
    3 * 3600
}

/// Parses "B0".."B4". Anything else is an error — bands are frozen at five, and a config
/// typo must not silently become a different cap.
pub fn parse_band(s: &str) -> Result<PalwModelBandV1, String> {
    match s {
        "B0" => Ok(PalwModelBandV1::B0),
        "B1" => Ok(PalwModelBandV1::B1),
        "B2" => Ok(PalwModelBandV1::B2),
        "B3" => Ok(PalwModelBandV1::B3),
        "B4" => Ok(PalwModelBandV1::B4),
        other => Err(format!("unknown band {other:?} (expected B0..B4)")),
    }
}

impl ReexecutorPolicyV1 {
    /// The blockrate constants and block time the policy's `network` names.
    pub fn blockrate(&self) -> Result<(BlockrateParams, u64), String> {
        match self.network.as_str() {
            "two-minute" => Ok((BlockrateParams::new_two_minute_bps(), 120_000)),
            "deci" => Ok((BlockrateParams::new_deci_bps(), 10_000)),
            other => Err(format!("unknown network {other:?} (expected two-minute | deci)")),
        }
    }

    pub fn max_band_parsed(&self) -> Result<PalwModelBandV1, String> {
        parse_band(&self.max_band)
    }

    pub fn validate(&self) -> Result<(), String> {
        self.blockrate()?;
        self.max_band_parsed()?;
        if self.network_id.is_empty() || self.network_id.len() > 64 {
            return Err("network_id must be 1..=64 bytes".into());
        }
        if self.max_concurrency == 0 {
            return Err("max_concurrency 0 can hold no duty".into());
        }
        if self.max_accepted_replay_secs == 0 {
            return Err("max_accepted_replay_secs 0 is not an offer".into());
        }
        if self.bench_runs == 0 {
            return Err("bench_runs must be at least 1".into());
        }
        if self.bench_golden_prefill_tokens >= self.bench_context_tokens {
            return Err("bench golden prefill does not fit the worker context — no decode budget remains".into());
        }
        if self.selftest_timeout_secs == 0 || self.bench_timeout_secs == 0 {
            return Err("a zero worker timeout would kill every qualification at spawn".into());
        }
        if self.ttl_daa == 0 {
            return Err("ttl_daa 0 would issue capabilities already expired".into());
        }
        if self.heartbeat_secs == 0 {
            return Err("heartbeat_secs 0 would spin".into());
        }
        if self.memory_headroom_permille > 9_000 {
            return Err("memory_headroom_permille above 9000 leaves no usable memory".into());
        }
        Ok(())
    }
}

/// The decode override the bench may actually RUN: the binding's credited ceiling, capped by
/// the worker's own admission rule `prefill + decode ≤ context`. Without the cap, a
/// format-bound class (ceiling 4095) over a 12-prefill golden asks for 4107 > 4096 and the
/// worker refuses every run — the fleet-blocking shape the offline mock cannot see.
pub fn bench_decode_tokens_v1(credited_ceiling_tokens: u32, bench_golden_prefill_tokens: u32, bench_context_tokens: u32) -> u32 {
    credited_ceiling_tokens.min(bench_context_tokens.saturating_sub(bench_golden_prefill_tokens))
}

// ---------------------------------------------------------------------------------------------
// Globs — deny wins, allow is explicit
// ---------------------------------------------------------------------------------------------

/// Minimal glob: literal match with `*` matching any (possibly empty) run. No character
/// classes, no `?` — a pattern language nobody can misread. Case-sensitive. The classic
/// greedy two-pointer (O(pattern · text) worst case), NOT the naive recursion: a
/// many-star pattern in an operator's own deny list must cost microseconds, never the
/// exponential blowup that reads as a wedged agent.
pub fn simple_glob_match(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None; // (pattern index after '*', text index it matched to)
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi + 1, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((star_pi, star_ti)) = star {
            // Backtrack: let the last '*' swallow one more text byte.
            pi = star_pi;
            ti = star_ti + 1;
            star = Some((star_pi, star_ti + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// The policy gate over one model (by its `model_profile_id` hex): deny first, then an
/// EXPLICIT allow. An empty allow list offers nothing — offering compute is an act, not a
/// default.
pub fn model_allowed(policy: &ReexecutorPolicyV1, model_profile_hex: &str) -> bool {
    if policy.deny_models.iter().any(|p| simple_glob_match(p, model_profile_hex)) {
        return false;
    }
    policy.allow_models.iter().any(|p| simple_glob_match(p, model_profile_hex))
}

// ---------------------------------------------------------------------------------------------
// Backend detection
// ---------------------------------------------------------------------------------------------

/// What the probe established about this host: the worker's identities plus the named tag and
/// the routing keys read from it — the ADR §9.3 "detect backend → resolve family/version".
/// `model_profile_id` is here because the v2 worker pins exactly ONE model: a binding over
/// any other model would pass every coarse check and then be refused by the worker itself at
/// qualify time with an unrelated-looking "unpinned artifact" error, so admission checks the
/// pin up front. (No serde derive: consensus enums stay serde-free; the binary renders JSON.)
#[derive(Clone, Debug)]
pub struct HostProbeV1 {
    pub runtime_class_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub model_profile_id: Hash64,
    pub class_tag: String,
    pub execution_family: PalwExecutionFamilyV1,
    pub family_version: u16,
    pub total_memory_bytes: u64,
}

/// Builds the probe from the worker's `v2-manifest` document, the resolved memory total and
/// the operator's validated rows (the second tag witness — [`resolve_class_tag_v1`]).
/// Refuses unknown class ids and memoryless hosts — both are "cannot participate honestly",
/// not defaults. The manifest MUST have been produced with the golden set registered
/// (`MISAKA_PALW_GOLDEN` exported to the worker): the worker's certified identity hash
/// CHANGES when goldens are registered, and fleet bindings pin the golden-populated hash —
/// probing without it would refuse every real binding with a misleading manifest mismatch.
pub fn build_host_probe_v1(
    manifest: &serde_json::Value,
    total_memory_bytes: u64,
    validated_rows: &[PalwClassRegistrationV1],
) -> Result<HostProbeV1, String> {
    let class_hex = manifest.get("runtime_class_id").and_then(|v| v.as_str()).ok_or("manifest carries no runtime_class_id")?;
    let manifest_hex =
        manifest.get("runtime_manifest_hash_v2").and_then(|v| v.as_str()).ok_or("manifest carries no runtime_manifest_hash_v2")?;
    let model_hex = manifest.get("model_profile_id").and_then(|v| v.as_str()).ok_or("manifest carries no model_profile_id")?;
    let runtime_class_id = parse_hash64(class_hex)?;
    let runtime_manifest_hash = parse_hash64(manifest_hex)?;
    let model_profile_id = parse_hash64(model_hex)?;
    let Some(class_tag) = resolve_class_tag_v1(&runtime_class_id, validated_rows) else {
        return Err(format!(
            "the worker's runtime_class_id {} matches no registered class tag (neither the consensus ledger nor a validated \
             row names it) — this build cannot name (or route) that backend; transcribe the registered tag into \
             PALW_REGISTERED_CLASS_TAGS first",
            &class_hex[..16.min(class_hex.len())]
        ));
    };
    let Some((execution_family, family_version)) = routing_keys_for_class_tag_v1(&class_tag) else {
        return Err(format!("known tag {class_tag:?} does not parse into routing keys — the tag table and the parser disagree"));
    };
    if total_memory_bytes == 0 {
        return Err(
            "total memory is zero — declare total_memory_bytes in the policy or fix detection; \"assume enough\" is not a mode".into(),
        );
    }
    Ok(HostProbeV1 {
        runtime_class_id,
        runtime_manifest_hash,
        model_profile_id,
        class_tag,
        execution_family,
        family_version,
        total_memory_bytes,
    })
}

/// Thin delegations to `Hash64`'s own tested `FromStr`/`Display` — a third hand-rolled hex
/// codec is a third place a format decision could silently diverge.
pub fn parse_hash64(s: &str) -> Result<Hash64, String> {
    s.parse::<Hash64>().map_err(|e| format!("bad 64-byte hash hex: {e}"))
}

pub fn hex64(h: &Hash64) -> String {
    h.to_string()
}

// ---------------------------------------------------------------------------------------------
// Admission — which bindings this host may even attempt
// ---------------------------------------------------------------------------------------------

/// The full admission conjunction for one binding on this host, each conjunct separate so the
/// refusal names the actual mismatch. Admission is necessary, not sufficient: qualification
/// (goldens + bench) still gates readiness.
pub fn binding_admissible_v1(
    binding: &PalwClassRegistrationV1,
    definition: &ModelDefinitionV1,
    probe: &HostProbeV1,
    policy: &ReexecutorPolicyV1,
    blockrate: &BlockrateParams,
    target_time_per_block_ms: u64,
) -> Result<(), String> {
    binding.validate(blockrate, target_time_per_block_ms).map_err(|e| format!("binding row does not validate: {e}"))?;
    definition.validate().map_err(|e| format!("model definition does not validate: {e}"))?;
    if !binding_matches_definition_v1(binding, definition) {
        return Err("binding does not join the signed model definition (profile id or artifact size)".into());
    }
    if binding.runtime_class_id != probe.runtime_class_id {
        return Err(format!(
            "binding class {} is not this host's class {} — replays would be cross-class, which never decides anything",
            &hex64(&binding.runtime_class_id)[..16],
            &hex64(&probe.runtime_class_id)[..16]
        ));
    }
    if binding.runtime_manifest_hash != probe.runtime_manifest_hash {
        return Err("binding pins a different runtime manifest than the probed worker".into());
    }
    if binding.model_profile_id != probe.model_profile_id {
        return Err("binding is over a different model than the worker's pin — the v2 worker executes exactly one pinned model, so \
             qualifying this binding would only fail later with an unrelated-looking artifact error"
            .into());
    }
    if binding.execution_family != probe.execution_family || binding.family_version != probe.family_version {
        return Err("binding family/version differ from the probed backend (registry incoherence — refuse loudly)".into());
    }
    let max_band = policy.max_band_parsed()?;
    if binding.model_band > max_band {
        return Err(format!("binding band {:?} exceeds the operator cap {:?}", binding.model_band, max_band));
    }
    let profile_hex = hex64(&binding.model_profile_id);
    if !model_allowed(policy, &profile_hex) {
        return Err("model is not allowed by operator policy (deny wins; allow is explicit)".into());
    }
    memory_fits_v1(binding.peak_memory_bytes, probe.total_memory_bytes, policy.memory_headroom_permille)?;
    Ok(())
}

/// Memory fit with headroom: `peak · (1000 + headroom) ≤ total · 1000`, in u128 so no
/// adversarial peak value can overflow the check into a pass.
pub fn memory_fits_v1(peak_memory_bytes: u64, total_memory_bytes: u64, headroom_permille: u32) -> Result<(), String> {
    let need = (peak_memory_bytes as u128) * (1000u128 + headroom_permille as u128);
    let have = (total_memory_bytes as u128) * 1000u128;
    if need > have {
        return Err(format!(
            "peak memory {peak_memory_bytes} B with {headroom_permille}‰ headroom exceeds the committed total {total_memory_bytes} B"
        ));
    }
    Ok(())
}

/// Artifact identity: the held file must BE the signed definition's artifact — size first
/// (cheap), digest second, both exact.
pub fn artifact_matches_definition_v1(file_size: u64, file_sha256: &[u8; 32], definition: &ModelDefinitionV1) -> bool {
    file_size == definition.gguf_size && *file_sha256 == definition.gguf_sha256
}

// ---------------------------------------------------------------------------------------------
// Qualification — goldens and the bench, judged
// ---------------------------------------------------------------------------------------------

/// The bench numbers this agent keeps per binding (total = load + execute, the cold-replay
/// convention of `v2-replay-bench`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchSummaryV1 {
    pub runs: u32,
    pub p50_total_ms: u64,
    pub p95_total_ms: u64,
    pub p99_total_ms: u64,
    pub max_total_ms: u64,
    pub roots_identical: bool,
}

/// Parses the worker's `misaka.palw.v2-replay-bench.v1` document. Every field is required —
/// a bench that did not print a number did not measure it.
pub fn parse_bench_summary_v1(doc: &serde_json::Value) -> Result<BenchSummaryV1, String> {
    if doc.get("schema").and_then(|v| v.as_str()) != Some("misaka.palw.v2-replay-bench.v1") {
        return Err("not a v2-replay-bench document".into());
    }
    let total = doc.get("total_ms").ok_or("bench document carries no total_ms")?;
    let num = |v: &serde_json::Value, k: &str| -> Result<u64, String> {
        v.get(k).and_then(|x| x.as_u64()).ok_or_else(|| format!("bench total_ms.{k} missing or not a number"))
    };
    Ok(BenchSummaryV1 {
        runs: doc.get("runs").and_then(|v| v.as_u64()).ok_or("bench runs missing")? as u32,
        p50_total_ms: num(total, "p50")?,
        p95_total_ms: num(total, "p95")?,
        p99_total_ms: num(total, "p99")?,
        max_total_ms: num(total, "max")?,
        roots_identical: doc
            .get("roots_identical_across_runs")
            .and_then(|v| v.as_bool())
            .ok_or("bench roots_identical_across_runs missing")?,
    })
}

/// One binding's qualification record, appended to the state log. `selftest_passed` is only
/// ever written `true` by the binary when the worker exited zero on `v2-selftest` — the
/// worker's own contract is that any vector mismatch dies non-zero. `bench: None` means the
/// bench produced NO measurement (spawn failure, timeout, schema drift) — recorded as the
/// absence it is, never as four fabricated zero-milliseconds; `failure_reason` preserves the
/// actual cause so the operator is not sent chasing golden vectors for a bench timeout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QualificationV1 {
    pub binding_id_hex: String,
    pub selftest_passed: bool,
    pub bench: Option<BenchSummaryV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    pub qualified_unix: u64,
}

/// The readiness judgment for one qualified binding (ADR-0034 §6: artifact held + runtime
/// boots + goldens passed + bench measured — and the measurement must actually FIT both the
/// binding's registered window at κ on THIS host and the operator's own advertised
/// `max_accepted_replay_secs`, or the signed capability would be a self-contradictory offer
/// discovered on the wire instead of at emission).
pub fn binding_ready_v1(
    qualification: &QualificationV1,
    binding: &PalwClassRegistrationV1,
    policy: &ReexecutorPolicyV1,
    target_time_per_block_ms: u64,
) -> Result<(), String> {
    if !qualification.selftest_passed {
        return Err(match &qualification.failure_reason {
            Some(reason) => format!("golden selftest did not pass — quarantined, never ready ({reason})"),
            None => "golden selftest did not pass — quarantined, never ready".into(),
        });
    }
    let Some(bench) = &qualification.bench else {
        return Err(match &qualification.failure_reason {
            Some(reason) => format!("no bench measurement recorded: {reason}"),
            None => "no bench measurement recorded".into(),
        });
    };
    if !bench.roots_identical {
        return Err("bench roots drifted across runs — this host does not reproduce itself; never ready".into());
    }
    if bench.runs < policy.bench_runs {
        return Err(format!("bench ran {} times, policy demands {}", bench.runs, policy.bench_runs));
    }
    if !replay_p99_fits_v1(bench.p99_total_ms, &binding.windows, target_time_per_block_ms) {
        return Err(format!("measured p99 {} ms does not fit the binding's replay window at κ on this host", bench.p99_total_ms));
    }
    let advertised_ms = (policy.max_accepted_replay_secs as u64).saturating_mul(1000);
    if bench.p99_total_ms > advertised_ms {
        return Err(format!(
            "measured p99 {} ms exceeds the operator's own max_accepted_replay_secs ({} s) — refusing to sign a \
             self-contradictory offer; raise the advisory or drop the binding",
            bench.p99_total_ms, policy.max_accepted_replay_secs
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Capability assembly
// ---------------------------------------------------------------------------------------------

/// The hardware band cap this host's memory supports: the largest band whose MEMORY base fits
/// the committed total; below the B0 base the cap is still B0 (the per-binding memory-fit
/// check remains the last gate, so a generous cap cannot admit an unfitting binding).
pub fn hardware_band_cap_v1(total_memory_bytes: u64) -> PalwModelBandV1 {
    let mut cap = PalwModelBandV1::B0;
    for band in [PalwModelBandV1::B1, PalwModelBandV1::B2, PalwModelBandV1::B3, PalwModelBandV1::B4] {
        if total_memory_bytes >= PALW_ROUTING_BAND_MEMORY_BASE_BYTES << (band as u32) {
            cap = band;
        }
    }
    cap
}

/// The declared band ceiling: never above the operator's cap, the hardware's cap, or the
/// family's initial cap (a Stage-0 agent has no manifest set to read yet, so the bootstrap
/// default stands in for the registered one — conservative in every direction).
pub fn derive_max_model_band_v1(policy_cap: PalwModelBandV1, probe: &HostProbeV1) -> PalwModelBandV1 {
    policy_cap.min(hardware_band_cap_v1(probe.total_memory_bytes)).min(initial_family_max_active_band_v1(probe.execution_family))
}

/// The capability nonce discipline: forward-only, reserved-before-use, and exhaustion is an
/// error rather than a wrap (a wrapped nonce would let a stale capability supersede a fresh
/// one).
pub fn next_capability_nonce(previous: Option<u64>) -> Result<u64, String> {
    match previous {
        None => Ok(1),
        Some(u64::MAX) => Err("capability nonce space exhausted — this identity cannot issue further capabilities".into()),
        Some(n) => Ok(n + 1),
    }
}

/// Everything the capability build needs, gathered by the binary. Assembly itself is pure so
/// the tests can hold the whole record to the light.
pub struct CapabilityInputsV1<'a> {
    pub verifier_id: Hash64,
    pub probe: &'a HostProbeV1,
    pub policy: &'a ReexecutorPolicyV1,
    /// Ready binding ids, ANY order; assembly sorts and the root builder enforces canonical.
    pub ready_binding_ids: Vec<Hash64>,
    pub now_daa: u64,
    pub nonce: u64,
}

/// The assembled, still-unsigned capability plus the per-binding membership proofs an
/// assignment observer will demand. The signature is the only field the binary adds.
pub struct CapabilityAssemblyV1 {
    pub capability: PalwVerifierCapabilityV1,
    pub ready_binding_ids_sorted: Vec<Hash64>,
    pub proofs: Vec<(Hash64, PalwReadyBindingProofV1)>,
}

/// Builds the capability record (ADR-0034 §6). Refuses an empty ready set — a capability
/// that can replay nothing is not a capability, and emitting one would only add noise for
/// the matcher to filter.
pub fn assemble_capability_v1(inputs: CapabilityInputsV1<'_>) -> Result<CapabilityAssemblyV1, String> {
    if inputs.ready_binding_ids.is_empty() {
        return Err("the ready set is empty — nothing to offer; not emitting a capability".into());
    }
    let mut ids = inputs.ready_binding_ids;
    ids.sort();
    ids.dedup();
    let root = ready_binding_root_v1(&ids).ok_or("ready set is not canonical after sort+dedup (bug)")?;
    let mut proofs = Vec::with_capacity(ids.len());
    for (index, id) in ids.iter().enumerate() {
        let proof = ready_binding_proof_v1(&ids, index).ok_or("proof construction failed on a canonical set (bug)")?;
        proofs.push((*id, proof));
    }
    let expiry = inputs.now_daa.checked_add(inputs.policy.ttl_daa).ok_or("TTL overflows the DAA space")?;
    let capability = PalwVerifierCapabilityV1 {
        version: PALW_ROUTING_OBJECT_VERSION_V1,
        verifier_id: inputs.verifier_id,
        execution_family: inputs.probe.execution_family,
        family_version: inputs.probe.family_version,
        max_model_band: derive_max_model_band_v1(inputs.policy.max_band_parsed()?, inputs.probe),
        ready_binding_root: root,
        max_concurrency: inputs.policy.max_concurrency,
        available_slots: inputs.policy.max_concurrency,
        max_accepted_replay_secs: inputs.policy.max_accepted_replay_secs,
        minimum_reward: inputs.policy.minimum_reward,
        // Stage 0: no bond escrow exists; the zero outpoint is the drill convention, exactly
        // as the shadow drill's carriages use it.
        replay_bond_outpoint: TransactionOutpoint::new(TransactionId::from_bytes([0u8; 64]), 0),
        available_bond: inputs.policy.available_bond,
        availability_expiry_daa: expiry,
        capability_nonce: inputs.nonce,
        signature: Vec::new(),
    };
    Ok(CapabilityAssemblyV1 { capability, ready_binding_ids_sorted: ids, proofs })
}

// ---------------------------------------------------------------------------------------------
// The emitted capability record — ONE typed shape, shared by the emitter, the tests and any
// Stage-0 matching-shadow consumer, so a field rename cannot compile in one crate and silently
// drop every capability in another. The canonical payload for Stage-1 is `capability_borsh_hex`
// (the Borsh bytes of the consensus struct); everything else is the human/matcher view.
// ---------------------------------------------------------------------------------------------

pub const CAPABILITY_RECORD_SCHEMA_V1: &str = "misaka.palw-reexecutor.capability-record.v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CapabilityRecordV1 {
    pub schema: String,
    pub network_id: String,
    pub capability_id: String,
    pub verifier_id: String,
    /// The raw ML-DSA-87 verification key, hex — WITHOUT this no third party could ever
    /// check the signature (`verifier_id` is only a hash of it), and Stage-1 observers must
    /// be able to verify Stage-0 records unchanged.
    pub verifier_public_key: String,
    pub class_tag: String,
    pub execution_family: String,
    pub family_version: u16,
    pub max_model_band: String,
    pub ready_binding_root: String,
    pub max_concurrency: u16,
    pub available_slots: u16,
    pub max_accepted_replay_secs: u32,
    pub minimum_reward: u64,
    pub available_bond: u64,
    pub availability_expiry_daa: u64,
    pub issued_now_daa: u64,
    pub capability_nonce: u64,
    pub signing_message: String,
    pub capability_borsh_hex: String,
    pub ready_bindings: Vec<ReadyBindingRecordV1>,
    pub not_ready: Vec<RefusalRecordV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadyBindingRecordV1 {
    pub binding_id: String,
    pub proof: ReadyProofRecordV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReadyProofRecordV1 {
    pub leaf_count: u32,
    pub leaf_index: u32,
    pub siblings: Vec<String>,
}

impl ReadyProofRecordV1 {
    pub fn from_proof(proof: &PalwReadyBindingProofV1) -> Self {
        Self { leaf_count: proof.leaf_count, leaf_index: proof.leaf_index, siblings: proof.siblings.iter().map(hex64).collect() }
    }

    pub fn to_proof(&self) -> Result<PalwReadyBindingProofV1, String> {
        Ok(PalwReadyBindingProofV1 {
            leaf_count: self.leaf_count,
            leaf_index: self.leaf_index,
            siblings: self.siblings.iter().map(|s| parse_hash64(s)).collect::<Result<Vec<_>, _>>()?,
        })
    }
}

/// A refused/not-ready binding with its reason — named fields so a future call site cannot
/// transpose `(reason, id)` and type-check.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefusalRecordV1 {
    pub binding_id: String,
    pub reason: String,
}

// =============================================================================================
// Test fixtures — shared between the unit tests and the offline end-to-end integration test,
// so the two can never drift apart. Hidden from docs; nothing operational may depend on them.
// =============================================================================================

#[doc(hidden)]
pub mod fixtures {
    use super::*;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    /// A validating model definition joined to `profile`/`size`, digest `sha256` (the
    /// integration test hashes a real mock artifact and passes its digest here).
    pub fn definition_with(profile: Hash64, size: u64, sha256: [u8; 32]) -> ModelDefinitionV1 {
        ModelDefinitionV1 {
            version: PALW_ROUTING_OBJECT_VERSION_V1,
            model_profile_id: profile,
            gguf_sha256: sha256,
            gguf_size: size,
            tokenizer_id: h64(0x04),
            architecture_id: h64(0x05),
            total_parameter_count: 2_000_000_000,
            active_parameter_count: 2_000_000_000,
            publisher_signature: vec![0x55; 64],
        }
    }

    /// A fully validating binding row shaped like the fleet registration (the registry's own
    /// fixture is crate-private, so the shape is reconstructed here — same tag, same measured
    /// envelope, same windows), with the artifact size a parameter so the integration test
    /// can bind a small mock file.
    pub fn test_binding_with_artifact(model_artifact_bytes: u64) -> PalwClassRegistrationV1 {
        use kaspa_consensus_core::palw_registry::{PALW_REGISTRY_OBJECT_VERSION_V1, PalwAdjudicationDepthV1, PalwCommitmentFormV1};
        use kaspa_consensus_core::palw_schedule::{
            PalwLeverageRemedyV1, PalwReplayCostMeasurementV1, PalwScheduleParamsV1, credited_ceiling_tokens_v1,
        };
        use kaspa_consensus_core::palw_step::{
            PALW_STEP_INPUT_LAYER_IN, PALW_STEP_OBJECT_VERSION_V1, PalwShapeProfileV3, PalwStepNodeRoleV1, PalwStepNodeV1,
            PalwStepOpKindV1, PalwStepOutLenV1, PalwTranscendentalBindingV1, PalwTranscendentalSiteV1,
        };
        let node = |kind| PalwStepNodeV1 {
            op_kind: kind,
            role: PalwStepNodeRoleV1::Plain,
            weight_name: String::new(),
            weight_dtypes: Vec::new(),
            out_len: PalwStepOutLenV1::Fixed { elements: 16 },
            tile_len: 16,
            kernel_semantics_id: h64(0x11),
            input_refs: vec![PALW_STEP_INPUT_LAYER_IN],
        };
        let shape_profile = PalwShapeProfileV3 {
            version: PALW_STEP_OBJECT_VERSION_V1,
            lane: kaspa_consensus_core::palw_step::PalwStepLaneV1::Float32,
            layer_count: 4,
            full_attention_interval: 4,
            hidden_dim: 16,
            ffn_dim: 16,
            attn_heads: 1,
            attn_kv_heads: 1,
            attn_head_dim: 16,
            rope_dims: 2,
            rope_sections: [1, 1, 0, 0],
            rope_freq_base_bits: 0x4CBE_BC20,
            rms_eps_bits: 0x3583_37BD,
            l2_eps_bits: 0x3583_37BD,
            // The registered BASE-0 epsilon (Q8), matching the registry's fleet fixture. A
            // profile field, not an adjudicator constant: see `palw_step`'s own note.
            base0_rms_eps_q: 1 << 8,
            gdn_heads: 1,
            gdn_head_k_dim: 16,
            gdn_head_v_dim: 16,
            gdn_conv_kernel: 4,
            vocab_size: 16,
            repack_on: 1,
            llamafile_on: 1,
            flash_attn_disabled: 1,
            fused_gdn_on: 1,
            use_ref_off: 1,
            kv_cache_f16: 1,
            // The re-executor mirrors the class it replays; a GPU class is a different class.
            gpu_offload_layers: 0,
            n_ctx: 64,
            n_batch: 64,
            n_ubatch: 64,
            n_seq: 1,
            n_threads: 4,
            pre_nodes: vec![node(PalwStepOpKindV1::EmbedLookup)],
            gdn_nodes: vec![node(PalwStepOpKindV1::RmsNorm)],
            attn_nodes: vec![node(PalwStepOpKindV1::RmsNorm)],
            post_nodes: vec![node(PalwStepOpKindV1::MatMulQuant)],
            reference_ruleset_id: h64(0x22),
            transcendental_bindings: vec![PalwTranscendentalBindingV1 {
                site: PalwTranscendentalSiteV1::VectorExpPolynomial,
                algorithm_id: h64(0x34),
            }],
            contraction_facts: vec![],
            kv_chunk_calls: 0,
            state_chunk_map_id: h64(0x44),
        };
        let windows = PalwScheduleParamsV1::stage1_defaults_two_minute_bps();
        let replay_cost =
            PalwReplayCostMeasurementV1 { fixed_overhead_ms: 4_300, ms_per_decode_token: 165, format_ceiling_tokens: 4_095 };
        let ceiling = credited_ceiling_tokens_v1(&replay_cost, &windows, 120_000);
        let class_tag = "misaka-palw-lite-cpu/x86_64/v1";
        PalwClassRegistrationV1 {
            version: PALW_REGISTRY_OBJECT_VERSION_V1,
            // ADR-0038 D: the normative per-inference op count. Same indicative value as the
            // consensus-core fixture so the two agree on what this class is.
            pwu_per_inference: 512_000_000,
            label: class_tag.into(),
            class_tag: class_tag.into(),
            runtime_class_id: kaspa_consensus_core::vlt::derive_runtime_class_id(class_tag),
            runtime_manifest_hash: h64(0x02),
            model_profile_id: h64(0x03),
            tokenizer_id: h64(0x04),
            shape_profile,
            tap_semantics_id: h64(0x05),
            state_layout_id: h64(0x06),
            state_chunk_map_id: h64(0x44),
            tap_layer_indices: vec![0, 1, 2, 3],
            checkpoint_interval: 8,
            execution_family: PalwExecutionFamilyV1::Cpu,
            family_version: 1,
            model_band: PalwModelBandV1::B0,
            quantization_id: h64(0x07),
            model_artifact_bytes,
            peak_memory_bytes: 5_000_000_000,
            max_proof_material_bytes: 8 << 20,
            commitment_form: PalwCommitmentFormV1::CompositeV2,
            // The FLOAT CPU class, so structural-only — mirroring the consensus-core fixture.
            // Its profile nodes carry an uncatalogued `h64(0x11)`, and under the ADR-0039 1a
            // coverage gate a row that claimed arithmetic depth would stop VALIDATING, which
            // would silently drop it from this tool's binding-row filter and stop capability
            // issuance. OPERATIONAL NOTE: any binding-row `.bin` already generated on the fleet
            // with `ArithmeticCatalogued` over a float profile is now invalid and must be
            // regenerated as structural-only before deploying this build.
            adjudication_depth: PalwAdjudicationDepthV1::StructuralOnly,
            libm_transcribed: true,
            replay_cost,
            credited_ceiling_tokens: ceiling,
            rho_v_permille: 1_000,
            p99_cold_replay_ms: 90_716,
            // Must track the registry's fleet fixture: (10, 0.2 %) stopped being Stage-2
            // eligible when §4e began measuring the full per-job payout (base + q · ρ_v · base
            // = 3 × base here) instead of base(C) alone. A binding this helper builds is meant
            // to be a VALIDATING one, so a stale remedy here would quietly make it ineligible.
            leverage_remedy: PalwLeverageRemedyV1 { min_credit_interval_daa: 14, base_subsidy_permille: 1 },
            windows,
            transcendental_algorithms: vec![(
                kaspa_consensus_core::palw_step::PalwTranscendentalSiteV1::VectorExpPolynomial,
                h64(0x34),
            )],
        }
    }
}

// =============================================================================================
// Tests — every decision above, held to the light without hardware
// =============================================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_routing::verify_ready_binding_v1;

    fn h64(fill: u8) -> Hash64 {
        Hash64::from_bytes([fill; 64])
    }

    fn policy() -> ReexecutorPolicyV1 {
        toml::from_str(
            r#"
            network_id = "misaka-palw-drill/v1"
            network = "two-minute"
            worker_bin = "/opt/palw/palw-worker"
            golden_set = "/opt/palw/golden.json"
            allow_models = ["*"]
            max_band = "B1"
            max_concurrency = 2
            max_accepted_replay_secs = 3600
            total_memory_bytes = 17179869184
            ttl_daa = 15
            heartbeat_secs = 300
            state_dir = "/tmp/reexecutor"
            "#,
        )
        .expect("policy toml")
    }

    #[test]
    fn the_policy_surface_parses_validates_and_refuses_degenerate_values() {
        let p = policy();
        p.validate().unwrap();
        assert_eq!(p.max_band_parsed().unwrap(), PalwModelBandV1::B1);
        assert_eq!(p.blockrate().unwrap().1, 120_000);
        let base = "network_id = \"d\"\nnetwork = \"deci\"\nworker_bin = \"w\"\ngolden_set = \"g\"\nmax_band = \"B0\"\n\
                    max_concurrency = 1\nmax_accepted_replay_secs = 1\nttl_daa = 1\nheartbeat_secs = 1\nstate_dir = \"s\"\n";
        let p: ReexecutorPolicyV1 = toml::from_str(base).unwrap();
        p.validate().unwrap();
        for (field, value) in [
            ("max_concurrency", "0"),
            ("max_accepted_replay_secs", "0"),
            ("bench_runs", "0"),
            ("ttl_daa", "0"),
            ("heartbeat_secs", "0"),
            ("memory_headroom_permille", "9001"),
        ] {
            let text: String = base
                .lines()
                .filter(|line| !line.starts_with(&format!("{field} =")))
                .map(|line| format!("{line}\n"))
                .chain(std::iter::once(format!("{field} = {value}\n")))
                .collect();
            let p: ReexecutorPolicyV1 = toml::from_str(&text).unwrap();
            assert!(p.validate().is_err(), "{field}={value} must refuse");
        }
        let unknown_band: Result<PalwModelBandV1, _> = parse_band("B5");
        assert!(unknown_band.is_err(), "bands are frozen at five");
        assert!(toml::from_str::<ReexecutorPolicyV1>("network_id = \"x\"\nunknown_key = 1").is_err(), "unknown keys refuse");
    }

    #[test]
    fn globs_are_literal_with_star_only_and_deny_wins() {
        assert!(simple_glob_match("*", "anything"));
        assert!(simple_glob_match("abc", "abc"));
        assert!(!simple_glob_match("abc", "abd"));
        assert!(simple_glob_match("ab*ef", "abcdef"));
        assert!(simple_glob_match("ab*", "ab"));
        assert!(!simple_glob_match("", "x"));
        assert!(simple_glob_match("", ""));
        assert!(!simple_glob_match("a?c", "abc"), "no ? language — literal only");
        // The greedy matcher's backtracking cases, and the adversarial many-star shape that
        // must stay instantaneous (the naive recursion here was an effective hang).
        assert!(simple_glob_match("*a*b*", "xxaxxbxx"));
        assert!(!simple_glob_match("*a*b*c*", "xxaxxbxx"));
        let long_text = "a".repeat(120) + "b";
        assert!(!simple_glob_match("*a*a*a*a*a*a*a*a*a*a*a*a*a*a*a*c", &long_text), "many-star non-match stays linear-ish");
        assert!(simple_glob_match("*a*a*a*a*a*a*a*a*b", &long_text));

        let mut p = policy();
        p.allow_models = vec!["aa*".into()];
        p.deny_models = vec!["aab*".into()];
        assert!(model_allowed(&p, "aacc"));
        assert!(!model_allowed(&p, "aabb"), "deny wins over allow");
        p.allow_models.clear();
        assert!(!model_allowed(&p, "aacc"), "an empty allow list offers nothing");
    }

    #[test]
    fn the_tag_reverse_index_round_trips_and_a_validated_row_is_a_second_witness() {
        for tag in PALW_REGISTERED_CLASS_TAGS {
            let id = kaspa_consensus_core::vlt::derive_runtime_class_id(tag);
            assert_eq!(resolve_class_tag_v1(&id, &[]).as_deref(), Some(*tag), "ledger tag {tag} does not round-trip");
        }
        assert_eq!(resolve_class_tag_v1(&h64(0x77), &[]), None, "an unknown class id resolves to nothing");
        // A validated row names its own class even when the binary's ledger lags: the row's
        // (tag → id) join was checked by validate(), so it is a witness, not a claim.
        let row = test_binding();
        assert_eq!(resolve_class_tag_v1(&row.runtime_class_id, std::slice::from_ref(&row)).as_deref(), Some(row.class_tag.as_str()));
        assert_eq!(resolve_class_tag_v1(&h64(0x77), std::slice::from_ref(&row)), None);
    }

    fn manifest_doc(row: &PalwClassRegistrationV1) -> serde_json::Value {
        serde_json::json!({
            "runtime_class_id": hex64(&row.runtime_class_id),
            "runtime_manifest_hash_v2": hex64(&row.runtime_manifest_hash),
            "model_profile_id": hex64(&row.model_profile_id),
        })
    }

    #[test]
    fn the_probe_fails_closed_on_unknown_class_missing_model_and_zero_memory() {
        let row = test_binding();
        let doc = manifest_doc(&row);
        let probe = build_host_probe_v1(&doc, 16 << 30, &[]).unwrap();
        assert_eq!(probe.class_tag, row.class_tag);
        assert_eq!(probe.execution_family, PalwExecutionFamilyV1::Cpu);
        assert_eq!(probe.family_version, 1);
        assert_eq!(probe.model_profile_id, row.model_profile_id, "the worker's single model pin is part of the probe");

        let mut unknown = doc.clone();
        unknown["runtime_class_id"] = serde_json::Value::String(hex64(&h64(0x99)));
        assert!(build_host_probe_v1(&unknown, 16 << 30, &[]).is_err(), "an unnameable backend refuses the host");
        let mut no_model = doc.clone();
        no_model.as_object_mut().unwrap().remove("model_profile_id");
        assert!(build_host_probe_v1(&no_model, 16 << 30, &[]).is_err(), "a manifest without the model pin refuses");
        assert!(build_host_probe_v1(&doc, 0, &[]).is_err(), "zero memory is refusal, not 'assume enough'");
        assert!(build_host_probe_v1(&serde_json::json!({}), 16 << 30, &[]).is_err());
    }

    #[test]
    fn the_bench_decode_override_fits_the_workers_context_rule() {
        // The fleet-blocking shape: ceiling 4095 over a 12-prefill golden must cap at 4084
        // (12 + 4084 = 4096 = N_CTX), never ask for 4095 and die.
        assert_eq!(bench_decode_tokens_v1(4_095, 12, 4_096), 4_084);
        assert_eq!(bench_decode_tokens_v1(4_095, 1, 4_096), 4_095, "a 1-prefill golden can bench the full ceiling");
        assert_eq!(bench_decode_tokens_v1(100, 12, 4_096), 100, "a small ceiling is unaffected");
        assert_eq!(bench_decode_tokens_v1(4_095, 4_096, 4_096), 0, "no budget left degrades to zero, not underflow");
    }

    #[test]
    fn memory_fit_holds_headroom_and_cannot_overflow_into_a_pass() {
        assert!(memory_fits_v1(10 << 30, 16 << 30, 200).is_ok(), "12 GiB effective need inside 16 GiB");
        assert!(memory_fits_v1(14 << 30, 16 << 30, 200).is_err(), "16.8 GiB effective need past 16 GiB");
        assert!(memory_fits_v1(u64::MAX, u64::MAX, 200).is_err(), "the headroom product must not wrap into a pass");
        assert!(memory_fits_v1(0, 1, 0).is_ok());
    }

    #[test]
    fn artifact_identity_is_size_and_digest_exact() {
        let def = definition(h64(0x03), 100);
        assert!(artifact_matches_definition_v1(100, &[0xAA; 32], &def));
        assert!(!artifact_matches_definition_v1(101, &[0xAA; 32], &def));
        assert!(!artifact_matches_definition_v1(100, &[0xAB; 32], &def));
    }

    #[test]
    fn bench_parsing_requires_every_number() {
        let good = serde_json::json!({
            "schema": "misaka.palw.v2-replay-bench.v1",
            "runs": 3,
            "total_ms": { "p50": 10, "p95": 12, "p99": 13, "max": 14 },
            "roots_identical_across_runs": true,
        });
        let b = parse_bench_summary_v1(&good).unwrap();
        assert_eq!((b.runs, b.p99_total_ms, b.roots_identical), (3, 13, true));
        for strip in ["schema", "runs", "total_ms", "roots_identical_across_runs"] {
            let mut broken = good.clone();
            broken.as_object_mut().unwrap().remove(strip);
            assert!(parse_bench_summary_v1(&broken).is_err(), "{strip} missing must refuse");
        }
    }

    #[test]
    fn readiness_demands_goldens_stable_roots_enough_runs_a_fitting_p99_and_a_coherent_offer() {
        let binding = test_binding();
        let p = policy();
        let good = QualificationV1 {
            binding_id_hex: hex64(&binding.registration_id()),
            selftest_passed: true,
            bench: Some(BenchSummaryV1 {
                runs: 3,
                p50_total_ms: 600_000,
                p95_total_ms: 650_000,
                p99_total_ms: 680_000,
                max_total_ms: 700_000,
                roots_identical: true,
            }),
            failure_reason: None,
            qualified_unix: 1,
        };
        binding_ready_v1(&good, &binding, &p, 120_000).unwrap();

        let mut failed_goldens = good.clone();
        failed_goldens.selftest_passed = false;
        failed_goldens.bench = None;
        failed_goldens.failure_reason = Some("selftest refused".into());
        let refusal = binding_ready_v1(&failed_goldens, &binding, &p, 120_000).unwrap_err();
        assert!(refusal.contains("selftest"), "the refusal names the actual stage: {refusal}");

        // A bench-stage failure after a PASSING selftest is not a golden quarantine — the
        // record keeps selftest_passed true, and the refusal carries the real cause.
        let mut bench_failed = good.clone();
        bench_failed.bench = None;
        bench_failed.failure_reason = Some("bench timeout".into());
        let refusal = binding_ready_v1(&bench_failed, &binding, &p, 120_000).unwrap_err();
        assert!(refusal.contains("bench timeout") && !refusal.contains("selftest"), "refusal: {refusal}");

        let mut drifting = good.clone();
        drifting.bench.as_mut().unwrap().roots_identical = false;
        assert!(binding_ready_v1(&drifting, &binding, &p, 120_000).is_err());
        let mut short = good.clone();
        short.bench.as_mut().unwrap().runs = 2;
        assert!(binding_ready_v1(&short, &binding, &p, 120_000).is_err(), "fewer runs than policy demands");
        let mut slow = good.clone();
        // w_replay 30 × 120 000 ms / κ 3 = 1 200 000 ms budget; past it must refuse.
        slow.bench.as_mut().unwrap().p99_total_ms = 1_200_001;
        assert!(binding_ready_v1(&slow, &binding, &p, 120_000).is_err(), "a p99 past the window at κ is not ready");

        // The operator's own advisory binds: a measured p99 past max_accepted_replay_secs is
        // a self-contradictory offer, refused at emission rather than discovered on the wire.
        let mut contradictory_policy = policy();
        contradictory_policy.max_accepted_replay_secs = 600; // 600 s < the 680 s measured p99
        let refusal = binding_ready_v1(&good, &binding, &contradictory_policy, 120_000).unwrap_err();
        assert!(refusal.contains("max_accepted_replay_secs"), "refusal: {refusal}");
    }

    #[test]
    fn the_hardware_band_cap_follows_the_memory_bases_and_never_exceeds_the_family() {
        // Receiving band b demands the memory to HOLD a b-sized peak: total ≥ 8 GiB << b.
        // Below the B1 base the cap floors at B0 — the per-binding memory-fit check stays the
        // last gate, so the floor cannot admit an unfitting binding.
        assert_eq!(hardware_band_cap_v1(0), PalwModelBandV1::B0);
        assert_eq!(hardware_band_cap_v1(8 << 30), PalwModelBandV1::B0, "8 GiB holds B0 peaks, not B1's 16 GiB");
        assert_eq!(hardware_band_cap_v1((16 << 30) - 1), PalwModelBandV1::B0);
        assert_eq!(hardware_band_cap_v1(16 << 30), PalwModelBandV1::B1);
        assert_eq!(hardware_band_cap_v1(32 << 30), PalwModelBandV1::B2);
        assert_eq!(hardware_band_cap_v1(u64::MAX), PalwModelBandV1::B4);

        let tag = PALW_REGISTERED_CLASS_TAGS[0];
        let probe = HostProbeV1 {
            runtime_class_id: kaspa_consensus_core::vlt::derive_runtime_class_id(tag),
            runtime_manifest_hash: h64(0x02),
            model_profile_id: h64(0x03),
            class_tag: tag.into(),
            execution_family: PalwExecutionFamilyV1::Cpu,
            family_version: 1,
            total_memory_bytes: 1 << 40, // a terabyte host…
        };
        // …still declares at most the CPU family's bootstrap cap, whatever the operator asks.
        assert_eq!(derive_max_model_band_v1(PalwModelBandV1::B4, &probe), PalwModelBandV1::B1);
        assert_eq!(derive_max_model_band_v1(PalwModelBandV1::B0, &probe), PalwModelBandV1::B0, "the operator cap binds downward");
    }

    #[test]
    fn the_nonce_only_moves_forward_and_exhaustion_is_an_error() {
        assert_eq!(next_capability_nonce(None).unwrap(), 1);
        assert_eq!(next_capability_nonce(Some(1)).unwrap(), 2);
        assert!(next_capability_nonce(Some(u64::MAX)).is_err(), "a wrap would let a stale capability supersede a fresh one");
    }

    #[test]
    fn capability_assembly_is_canonical_verifiable_and_refuses_an_empty_ready_set() {
        let tag = PALW_REGISTERED_CLASS_TAGS[0];
        let probe = HostProbeV1 {
            runtime_class_id: kaspa_consensus_core::vlt::derive_runtime_class_id(tag),
            runtime_manifest_hash: h64(0x02),
            model_profile_id: h64(0x03),
            class_tag: tag.into(),
            execution_family: PalwExecutionFamilyV1::Cpu,
            family_version: 1,
            total_memory_bytes: 16 << 30,
        };
        let p = policy();
        let inputs = CapabilityInputsV1 {
            verifier_id: h64(0x01),
            probe: &probe,
            policy: &p,
            ready_binding_ids: vec![h64(0x30), h64(0x10), h64(0x30), h64(0x20)], // unsorted + duplicate
            now_daa: 1_000,
            nonce: 7,
        };
        let assembled = assemble_capability_v1(inputs).unwrap();
        assert_eq!(assembled.ready_binding_ids_sorted, vec![h64(0x10), h64(0x20), h64(0x30)], "sorted, deduped");
        let cap = &assembled.capability;
        assert_eq!(cap.availability_expiry_daa, 1_015);
        assert_eq!(cap.capability_nonce, 7);
        assert_eq!(cap.max_model_band, PalwModelBandV1::B1, "min(policy B1, hw B1 at 16 GiB, family B1)");
        assert_eq!(cap.available_slots, cap.max_concurrency);
        // Every proof verifies against the committed root, and a foreign binding does not.
        for (id, proof) in &assembled.proofs {
            assert!(verify_ready_binding_v1(&cap.ready_binding_root, id, proof));
        }
        assert!(!verify_ready_binding_v1(&cap.ready_binding_root, &h64(0x77), &assembled.proofs[0].1));
        // The record validates once signed (shape-wise: give it a placeholder signature).
        let mut signed = cap.clone();
        signed.signature = vec![0x55; 64];
        signed.validate().unwrap();

        let empty = CapabilityInputsV1 {
            verifier_id: h64(0x01),
            probe: &probe,
            policy: &p,
            ready_binding_ids: vec![],
            now_daa: 1_000,
            nonce: 8,
        };
        assert!(assemble_capability_v1(empty).is_err(), "a capability that can replay nothing is not a capability");
    }

    // -----------------------------------------------------------------------------------------
    // Admission end-to-end against a real validated binding row (shared fixtures)
    // -----------------------------------------------------------------------------------------

    fn definition(profile: Hash64, size: u64) -> ModelDefinitionV1 {
        crate::fixtures::definition_with(profile, size, [0xAA; 32])
    }

    fn test_binding() -> PalwClassRegistrationV1 {
        crate::fixtures::test_binding_with_artifact(1_280_835_840)
    }

    fn matching_probe(binding: &PalwClassRegistrationV1) -> HostProbeV1 {
        HostProbeV1 {
            runtime_class_id: binding.runtime_class_id,
            runtime_manifest_hash: binding.runtime_manifest_hash,
            model_profile_id: binding.model_profile_id,
            class_tag: binding.class_tag.clone(),
            execution_family: binding.execution_family,
            family_version: binding.family_version,
            total_memory_bytes: 16 << 30,
        }
    }

    #[test]
    fn every_admission_conjunct_refuses_on_its_own() {
        let binding = test_binding();
        let def = definition(binding.model_profile_id, binding.model_artifact_bytes);
        let probe = matching_probe(&binding);
        let p = policy();
        let (blockrate, block_ms) = p.blockrate().unwrap();
        binding_admissible_v1(&binding, &def, &probe, &p, &blockrate, block_ms).unwrap();

        // An invalid row refuses before anything else looks at it.
        let mut invalid = binding.clone();
        invalid.model_band = PalwModelBandV1::B1; // no longer derived
        assert!(binding_admissible_v1(&invalid, &def, &probe, &p, &blockrate, block_ms).is_err());

        // The definition join.
        let wrong_size = definition(binding.model_profile_id, binding.model_artifact_bytes + 1);
        assert!(binding_admissible_v1(&binding, &wrong_size, &probe, &p, &blockrate, block_ms).is_err());
        let wrong_profile = definition(h64(0x99), binding.model_artifact_bytes);
        assert!(binding_admissible_v1(&binding, &wrong_profile, &probe, &p, &blockrate, block_ms).is_err());

        // Host identity: class, manifest.
        let mut foreign_class = matching_probe(&binding);
        foreign_class.runtime_class_id = h64(0x99);
        assert!(binding_admissible_v1(&binding, &def, &foreign_class, &p, &blockrate, block_ms).is_err(), "cross-class");
        let mut foreign_manifest = matching_probe(&binding);
        foreign_manifest.runtime_manifest_hash = h64(0x99);
        assert!(binding_admissible_v1(&binding, &def, &foreign_manifest, &p, &blockrate, block_ms).is_err());
        let mut foreign_model = matching_probe(&binding);
        foreign_model.model_profile_id = h64(0x99);
        assert!(
            binding_admissible_v1(&binding, &def, &foreign_model, &p, &blockrate, block_ms).is_err(),
            "a binding over a model the worker does not pin is refused at scan, not at qualify"
        );

        // Operator policy: band cap, deny, empty allow, memory.
        let mut capped = policy();
        capped.max_band = "B0".into();
        binding_admissible_v1(&binding, &def, &probe, &capped, &blockrate, block_ms).unwrap(); // B0 row fits a B0 cap
        let mut denied = policy();
        denied.deny_models = vec![hex64(&binding.model_profile_id)];
        assert!(binding_admissible_v1(&binding, &def, &probe, &denied, &blockrate, block_ms).is_err(), "deny wins");
        let mut nothing_allowed = policy();
        nothing_allowed.allow_models.clear();
        assert!(binding_admissible_v1(&binding, &def, &probe, &nothing_allowed, &blockrate, block_ms).is_err());
        let mut tiny_host = matching_probe(&binding);
        tiny_host.total_memory_bytes = 4 << 30;
        assert!(binding_admissible_v1(&binding, &def, &tiny_host, &p, &blockrate, block_ms).is_err(), "5 GB peak in 4 GiB");
    }
}
