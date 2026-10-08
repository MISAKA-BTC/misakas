//! **The gates and their codes** (RFC-0002 §II.10.3): one stable `blocking` code per failed gate, `NOT_RUN_AFTER_<GATE>` for every gate
//! after a failed one, and the reason a gate was not run when nothing failed before it (the census reads headers: `pack` needs the
//! weights, `seat` and `final` need a chain).
//!
//! Codes are stable once published. Where §II.10.3 names a condition its name is used (`GATED_ACCESS`, `MISSING_WEIGHTS`,
//! `BASE_UNPINNED`, `RIGHTS_UNCONFIRMED`, `FEATURE_C`, `CUSTOM_CODE_UNMODELLED`, `MODALITY_PROFILE_MISSING`, `PARTIAL_TASK_ONLY`,
//! `QUANT_DESCRIPTOR_MISSING`, `COURT_BUDGET`, `CLOSE_TOO_LARGE`, `CONTEXT_BOUND`, `SEAT_MEMORY`); where the preflight (§II.2.4) already
//! has a stable code for the condition, that code is kept (`CONFIG_KEY_UNREAD`, `TENSOR_MISSING`, `FENCE_NOT_ARMED`, …), and
//! [`gate_code_of_preflight`] is the one table between the two vocabularies — a preflight code it does not know maps to
//! [`UNMAPPED_PREFLIGHT_CODE`], never to a pass.

use serde::{Deserialize, Serialize};

/// The six gates, in the order a repository meets them (§II.10.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    /// Pinned files, components, task and the access/rights decision; a complete artifact or a resolvable adapter base.
    Source,
    /// The full declared task lowers to a versioned `ModelSpec`/TIR program and every semantic key and weight is consumed.
    Lower,
    /// The real checkpoint converts by a bounded-memory stream and its runtime pack verifies (needs the weights).
    Pack,
    /// The real-size program passes `tir_admit_v1` and the registry path at a named ruleset and height.
    Admit,
    /// Distinct, independent operators hold the class and prove possession and conformance (needs a chain).
    Seat,
    /// A real job's claim reaches `Final` (needs a chain).
    Final,
}

impl Gate {
    pub const ALL: [Gate; 6] = [Gate::Source, Gate::Lower, Gate::Pack, Gate::Admit, Gate::Seat, Gate::Final];

    pub fn name(self) -> &'static str {
        match self {
            Gate::Source => "source",
            Gate::Lower => "lower",
            Gate::Pack => "pack",
            Gate::Admit => "admit",
            Gate::Seat => "seat",
            Gate::Final => "final",
        }
    }

    /// `NOT_RUN_AFTER_<GATE>`: the code of every gate after this one failed.
    pub fn not_run_after(self) -> String {
        format!("NOT_RUN_AFTER_{}", self.name().to_ascii_uppercase())
    }
}

/// A gate's state. `NotRun` is never a pass: its `blocking` says why it was not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateStatus {
    Pass,
    Fail,
    NotRun,
}

// ---- why a gate was not run although nothing failed before it ------------------------------------------------------------------

/// The gate needs the weights (a conversion, a pack's verification); the census reads headers only.
pub const NOT_RUN_NEEDS_WEIGHTS: &str = "NOT_RUN_NEEDS_WEIGHTS";
/// The gate needs a chain (registered class, seats, claims); the census runs offline.
pub const NOT_RUN_NEEDS_CHAIN: &str = "NOT_RUN_NEEDS_CHAIN";
/// The repository was not in the header sample: only its listing was read, and the gate needs its headers.
pub const NOT_RUN_NOT_SAMPLED: &str = "NOT_RUN_NOT_SAMPLED";
/// The class's layout search spent the census's per-judgment time budget: not run (counted as not passing), never a verdict.
pub const NOT_RUN_JUDGMENT_BUDGET: &str = "NOT_RUN_JUDGMENT_BUDGET";
/// A headers-depth pass: the shape depth's admission is deferred to a later pass (never inferred).
pub const NOT_RUN_DEPTH_HEADERS: &str = "NOT_RUN_DEPTH_HEADERS";
/// The class is a pipeline (RFC-0003) class: the census does not run the pipeline registration admission at the shape depth.
pub const NOT_RUN_PIPELINE_ADMISSION: &str = "NOT_RUN_PIPELINE_ADMISSION";
/// The frontend reads the data of a small tensor (a GGUF's `rope_freqs.weight`) to build the configuration; the census reads headers
/// only (the network policy of this census), so the gate is not run rather than failed.
pub const NOT_RUN_NEEDS_TENSOR_DATA: &str = "NOT_RUN_NEEDS_TENSOR_DATA";
/// The checkpoint is a PyTorch `pytorch_model.bin` (a zip of a pickle and its storages). The frontend reads it without running the
/// pickle (`weights::torchzip`), but the census fetches safetensors headers only: the zip's central directory and `data.pkl` of this
/// repository were never read, so the gate is not run rather than failed as an unsupported format.
pub const NOT_RUN_NEEDS_PICKLE_DIRECTORY: &str = "NOT_RUN_NEEDS_PICKLE_DIRECTORY";

// ---- source ------------------------------------------------------------------------------------------------------------------------

/// The repository could not be read at its pinned revision (removed after the snapshot, made private, an error after retries).
pub const REPO_UNREACHABLE: &str = "REPO_UNREACHABLE";
/// The Hub lists the repository as disabled.
pub const REPO_DISABLED: &str = "REPO_DISABLED";
/// The repository is gated: its files need terms accepted, which the census never does.
pub const GATED_ACCESS: &str = "GATED_ACCESS";
/// No weight file of any format.
pub const MISSING_WEIGHTS: &str = "MISSING_WEIGHTS";
/// An adapter (or other derived artifact) whose base cannot be pinned (absent, ambiguous, not public, gated).
pub const BASE_UNPINNED: &str = "BASE_UNPINNED";
/// The selected artifact is not complete (a shard the index names is absent; a file shorter than its header declares).
pub const WEIGHTS_INCOMPLETE: &str = "WEIGHTS_INCOMPLETE";
/// A metadata file or header the gate needs could not be fetched (counted as a failure, never dropped).
pub const FETCH_FAILED: &str = "FETCH_FAILED";
/// A header longer than the census's cap (64 MiB): refused by name, never read.
pub const HEADER_TOO_LARGE: &str = "HEADER_TOO_LARGE";
/// A header that is not a safetensors or GGUF header.
pub const HEADER_INVALID: &str = "HEADER_INVALID";
/// No rights decision under the named policy (a card's license is evidence, not a decision).
pub const RIGHTS_UNCONFIRMED: &str = "RIGHTS_UNCONFIRMED";

// ---- lower -------------------------------------------------------------------------------------------------------------------------

/// The repository declares no task and none follows from its configuration.
pub const TASK_UNKNOWN: &str = "TASK_UNKNOWN";
/// The `arg` of a `TASK_UNKNOWN` whose repository carries no configuration naming a model class (no `architectures`, `model_type` or
/// PEFT block): not a transformers / diffusers / PEFT repository at all, so nothing in the frontend can be asked to read it.
pub const TASK_UNKNOWN_NO_CONFIG: &str = "no-config";
/// The declared task has no canonical job profile (inputs, output, court path, artifact) in any RFC this build carries.
pub const MODALITY_PROFILE_MISSING: &str = "MODALITY_PROFILE_MISSING";
/// The declared task needs parts the class this build produces does not compute (a VLM's vision stage): the text class is a
/// separately scoped class and does not credit the repository's task.
pub const PARTIAL_TASK_ONLY: &str = "PARTIAL_TASK_ONLY";
/// The selected artifact's weight format has no reader (PyTorch pickle, ONNX, TensorFlow, Flax, …) or a layout the tool does not read.
pub const FORMAT_UNSUPPORTED: &str = "FORMAT_UNSUPPORTED";
/// An adapter: the census does not yet compose an adapter with its base to check the composite.
pub const ADAPTER_UNCHECKED: &str = "ADAPTER_UNCHECKED";
/// Weights with no configuration the frontend can read beside them.
pub const CONFIG_MISSING: &str = "CONFIG_MISSING";
/// `trust_remote_code` (or a custom diffusers pipeline) the adapters do not model; repository code is never run.
pub const CUSTOM_CODE_UNMODELLED: &str = "CUSTOM_CODE_UNMODELLED";
/// A feature (or primitive) the generic lowerer does not lower yet: Level C (§II.1 P3).
pub const FEATURE_C: &str = "FEATURE_C";
/// No descriptor reads the checkpoint's quantisation (§II.10.3 lists it under `pack`; the census meets it at `lower`, where the
/// lowering stops because the stored weights cannot be bound).
pub const QUANT_DESCRIPTOR_MISSING: &str = "QUANT_DESCRIPTOR_MISSING";
/// A code the preflight raised that this table does not map. Never a pass; a test keeps it from happening silently.
pub const UNMAPPED_PREFLIGHT_CODE: &str = "UNMAPPED_PREFLIGHT_CODE";

// ---- admit ---------------------------------------------------------------------------------------------------------------------------

/// A cost the court pays to prosecute the class is over its ceiling.
pub const COURT_BUDGET: &str = "COURT_BUDGET";
/// The worst carried close is over the carried cap.
pub const CLOSE_TOO_LARGE: &str = "CLOSE_TOO_LARGE";
/// The canonical job (the declared context) is out of the network's bounds.
pub const CONTEXT_BOUND: &str = "CONTEXT_BOUND";

// ---- seat ----------------------------------------------------------------------------------------------------------------------------

/// No seat tier of the network holds the class's replay (§II.2.3 item 8).
pub const SEAT_MEMORY: &str = "SEAT_MEMORY";

/// **The one table from a preflight blocker code (§II.2.4) to a gate and its census code.** `None` for a code this table does not know.
pub fn gate_code_of_preflight(code: &str) -> Option<(Gate, &'static str)> {
    Some(match code {
        // convert stage → lower (the source's completeness is the census's own check)
        "ARCH_NEEDS_FEATURE" | "ARCH_NEEDS_PRIMITIVE" => (Gate::Lower, FEATURE_C),
        "ARCH_REFUSED" => (Gate::Lower, "ARCH_REFUSED"),
        "CONFIG_KEY_UNREAD" => (Gate::Lower, "CONFIG_KEY_UNREAD"),
        "CONFIG_INVALID" => (Gate::Lower, "CONFIG_INVALID"),
        "REMOTE_CODE" => (Gate::Lower, CUSTOM_CODE_UNMODELLED),
        "QUANT_NO_DESCRIPTOR" | "QUANT_KNOWN_UNDESCRIBED" => (Gate::Lower, QUANT_DESCRIPTOR_MISSING),
        "QUANT_REFUSED" => (Gate::Lower, "QUANT_REFUSED"),
        "TENSOR_MISSING" => (Gate::Lower, "TENSOR_MISSING"),
        "TENSOR_SHAPE" => (Gate::Lower, "TENSOR_SHAPE"),
        "TOKENIZER_MISSING" => (Gate::Lower, "TOKENIZER_MISSING"),
        "ADAPTER_REFUSED" => (Gate::Lower, "ADAPTER_REFUSED"),
        // A weight file the frontend refuses by its form (a PyTorch pickle off the allowlist, a strided view, the legacy format).
        "FORMAT_UNSUPPORTED" => (Gate::Lower, FORMAT_UNSUPPORTED),
        "SOURCE_INCOMPLETE" => (Gate::Source, WEIGHTS_INCOMPLETE),
        // register stage → admit
        "ADMISSION_EXCEEDS" => (Gate::Admit, "ADMISSION_EXCEEDS"),
        "ADMISSION_REFUSED" => (Gate::Admit, "ADMISSION_REFUSED"),
        "CLOSE_SIZE_OVER_CAP" => (Gate::Admit, CLOSE_TOO_LARGE),
        "COURT_COST_OVER_CEILING" => (Gate::Admit, COURT_BUDGET),
        "DA_LADDER_EXCEEDED" => (Gate::Admit, "DA_LADDER_EXCEEDED"),
        "COURT_WINDOW_EXCEEDED" => (Gate::Admit, "COURT_WINDOW_EXCEEDED"),
        "CANONICAL_JOB_OUT_OF_BOUNDS" => (Gate::Admit, CONTEXT_BOUND),
        "FENCE_NOT_ARMED" => (Gate::Admit, "FENCE_NOT_ARMED"),
        "ARTIFACT_ROOT_KNOWN" => (Gate::Admit, "ARTIFACT_ROOT_KNOWN"),
        // mine stage → seat (or pack)
        "SEAT_MEMORY_SHORT" => (Gate::Seat, SEAT_MEMORY),
        "READY_SEATS_INSUFFICIENT" => (Gate::Seat, "READY_SEATS_SHORT"),
        "INDEPENDENT_OPERATORS" => (Gate::Seat, "INDEPENDENCE_SHORT"),
        "PACK_NOT_VERIFIED" => (Gate::Pack, "PACK_NOT_VERIFIED"),
        _ => return None,
    })
}

/// The order in which a gate's codes are ranked when more than one applies: the first is the gate's `blocking` code. Task-level codes
/// come first (a repository whose task has no profile is blocked by that whatever its architecture), then the artifact's form, then
/// the architecture, then the tensors.
pub fn priority(gate: Gate, code: &str) -> u32 {
    let order: &[&str] = match gate {
        Gate::Source => &[
            REPO_UNREACHABLE,
            REPO_DISABLED,
            GATED_ACCESS,
            MISSING_WEIGHTS,
            BASE_UNPINNED,
            FETCH_FAILED,
            HEADER_TOO_LARGE,
            HEADER_INVALID,
            WEIGHTS_INCOMPLETE,
            RIGHTS_UNCONFIRMED,
        ],
        Gate::Lower => &[
            TASK_UNKNOWN,
            MODALITY_PROFILE_MISSING,
            PARTIAL_TASK_ONLY,
            FORMAT_UNSUPPORTED,
            ADAPTER_UNCHECKED,
            "ADAPTER_REFUSED",
            CONFIG_MISSING,
            "CONFIG_INVALID",
            CUSTOM_CODE_UNMODELLED,
            FEATURE_C,
            "ARCH_REFUSED",
            "CONFIG_KEY_UNREAD",
            QUANT_DESCRIPTOR_MISSING,
            "QUANT_REFUSED",
            "TENSOR_MISSING",
            "TENSOR_SHAPE",
            "TOKENIZER_MISSING",
            UNMAPPED_PREFLIGHT_CODE,
        ],
        Gate::Admit => &[
            "FENCE_NOT_ARMED",
            CONTEXT_BOUND,
            "COURT_WINDOW_EXCEEDED",
            "DA_LADDER_EXCEEDED",
            COURT_BUDGET,
            CLOSE_TOO_LARGE,
            "ADMISSION_EXCEEDS",
            "ADMISSION_REFUSED",
            "ARTIFACT_ROOT_KNOWN",
        ],
        Gate::Pack | Gate::Seat | Gate::Final => &[],
    };
    order.iter().position(|c| *c == code).map(|p| p as u32).unwrap_or(order.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every code the preflight publishes (§II.2.4, `preflight/{model,chain}.rs`) maps to a gate: a new preflight code without a row
    /// here fails this test instead of reaching a census as an unmapped failure.
    #[test]
    fn every_published_preflight_code_has_a_gate() {
        let published = [
            "ARCH_NEEDS_FEATURE",
            "ARCH_NEEDS_PRIMITIVE",
            "ARCH_REFUSED",
            "CONFIG_KEY_UNREAD",
            "CONFIG_INVALID",
            "REMOTE_CODE",
            "QUANT_NO_DESCRIPTOR",
            "QUANT_KNOWN_UNDESCRIBED",
            "QUANT_REFUSED",
            "TENSOR_MISSING",
            "TENSOR_SHAPE",
            "TOKENIZER_MISSING",
            "ADAPTER_REFUSED",
            "SOURCE_INCOMPLETE",
            "FORMAT_UNSUPPORTED",
            "ADMISSION_EXCEEDS",
            "ADMISSION_REFUSED",
            "CLOSE_SIZE_OVER_CAP",
            "COURT_COST_OVER_CEILING",
            "DA_LADDER_EXCEEDED",
            "COURT_WINDOW_EXCEEDED",
            "CANONICAL_JOB_OUT_OF_BOUNDS",
            "FENCE_NOT_ARMED",
            "ARTIFACT_ROOT_KNOWN",
            "SEAT_MEMORY_SHORT",
            "READY_SEATS_INSUFFICIENT",
            "INDEPENDENT_OPERATORS",
            "PACK_NOT_VERIFIED",
        ];
        for c in published {
            assert!(gate_code_of_preflight(c).is_some(), "{c} has no gate");
        }
        assert_eq!(gate_code_of_preflight("SOMETHING_NEW"), None);
    }

    /// The source code grep: every `Blocker::new(Stage::…, "<CODE>"` in the preflight is one of the published codes above, so the
    /// list in the test above is the whole vocabulary.
    #[test]
    fn the_preflight_raises_no_code_outside_the_table() {
        let src = [include_str!("../preflight/model.rs"), include_str!("../preflight/chain.rs"), include_str!("../preflight/full.rs")];
        let mut seen = std::collections::BTreeSet::new();
        for s in src {
            for (i, _) in s.match_indices("Blocker::new(") {
                let rest = &s[i..s.len().min(i + 400)];
                if let Some(q) = rest.find('"') {
                    let tail = &rest[q + 1..];
                    if let Some(e) = tail.find('"') {
                        let code = &tail[..e];
                        if code.chars().all(|c| c.is_ascii_uppercase() || c == '_') && code.len() > 3 {
                            seen.insert(code.to_string());
                        }
                    }
                }
            }
        }
        // Codes chosen through a variable (`let (code, …) = match …`) are listed in the `match` arms the test above enumerates.
        for c in &seen {
            assert!(gate_code_of_preflight(c).is_some(), "the preflight raises {c}, which the census table does not map");
        }
        assert!(seen.len() >= 10, "the scan found the preflight's blockers ({seen:?})");
    }

    #[test]
    fn task_level_codes_outrank_the_architecture_and_the_tensors() {
        assert!(priority(Gate::Lower, MODALITY_PROFILE_MISSING) < priority(Gate::Lower, FEATURE_C));
        assert!(priority(Gate::Lower, PARTIAL_TASK_ONLY) < priority(Gate::Lower, "TENSOR_MISSING"));
        assert!(priority(Gate::Source, GATED_ACCESS) < priority(Gate::Source, RIGHTS_UNCONFIRMED));
        assert!(priority(Gate::Lower, "UNKNOWN_CODE") > priority(Gate::Lower, UNMAPPED_PREFLIGHT_CODE));
        assert_eq!(Gate::Admit.not_run_after(), "NOT_RUN_AFTER_ADMIT");
    }
}
