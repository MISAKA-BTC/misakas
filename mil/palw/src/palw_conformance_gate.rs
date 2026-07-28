//! ADR-0039 Canonical Compute v1 §14 — the provider-side **self-conformance gate**
//! (`docs/design/misaka-canonical-compute-v1.md`).
//!
//! A provider MUST reproduce its determinism class's committed golden vector set `V_i` (§11/§13) to be
//! eligible to register / stay registered. The gate runs the [`check_conformance`] predicate:
//!
//! - **at startup** (never-checked ⇒ run),
//! - **on a fixed period**, and
//! - **whenever the stack fingerprint changes** (driver / OS / toolchain update) — an in-flight update
//!   otherwise slips past a startup-only gate and silently breaks the fail-closed guarantee.
//!
//! Any non-conformance is **fail-closed** (registration refused); per §13 that same stack is then a
//! candidate for a *new* set (rolling migration), so a codegen cliff becomes a migration rather than a
//! silent class break. The [`StackFingerprint`] rides along as **off-consensus telemetry** (never a
//! consensus input) purely to diagnose set-split events and drive the §13 promotion pipeline.

use crate::palw_determinism::{ConformanceReport, ConformanceVector, check_conformance};
use crate::palw_replica::VerifiableInferenceBackend;

/// Off-consensus telemetry (§14): a coarse fingerprint of the parts of a stack that can silently change
/// codegen — GPU arch, GPU driver, OS build, and the runtime/compiler toolchain. Attached to the
/// registration record for diagnostics ONLY; it is never a consensus input and never a pairing key (the
/// pairing key is the reproduced `set_id`, §13).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct StackFingerprint {
    pub gpu_arch: String,
    pub driver_version: String,
    pub os_build: String,
    pub runtime_toolchain: String,
}

/// The §14 decision for a provider registering under / staying registered to a class set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistrationDecision {
    /// Reproduced `V_i` byte-exact ⇒ eligible; the fingerprint rides along as telemetry.
    Admit { fingerprint: StackFingerprint },
    /// Rejected: did not reproduce `V_i` (drift, wrong stack or no committed set). Registration
    /// refused; per §13 this stack is a candidate for a new set.
    Refuse { report: ConformanceReport },
}

impl RegistrationDecision {
    pub fn admitted(&self) -> bool {
        matches!(self, RegistrationDecision::Admit { .. })
    }
}

/// §14 re-conformance policy — MUST re-run when never checked (startup), when the fixed period has
/// elapsed, or when the stack fingerprint changed since the last check. Pure.
pub fn should_reconform(
    last: Option<&StackFingerprint>,
    current: &StackFingerprint,
    ticks_since_last: u64,
    period_ticks: u64,
) -> bool {
    match last {
        None => true, // never checked ⇒ startup gate
        Some(prev) => prev != current || (period_ticks != 0 && ticks_since_last >= period_ticks),
    }
}

/// Run the §14 gate ONCE: self-run the committed vectors and decide, fail-closed on any non-conformance
/// (including an empty / non-live set — a tier with no committed set is not registerable, §13/§15). Pure
/// over the backend's deterministic output.
pub fn run_self_conformance(
    backend: &dyn VerifiableInferenceBackend,
    vectors: &[ConformanceVector],
    fingerprint: StackFingerprint,
) -> RegistrationDecision {
    match check_conformance(backend, vectors) {
        ConformanceReport::Conforms { .. } => RegistrationDecision::Admit { fingerprint },
        report => RegistrationDecision::Refuse { report },
    }
}

/// The provider-side stateful §14 gate: remembers the last checked fingerprint and re-runs on startup, on
/// the fixed period, or on a fingerprint change. Off-consensus provider infra; the returned decision drives
/// whether the provider offers to register / keeps its registration. A `period_ticks` of 0 disables the
/// periodic trigger (fingerprint-change + startup only).
#[derive(Clone, Debug, Default)]
pub struct SelfConformanceGate {
    last_fingerprint: Option<StackFingerprint>,
    ticks_since_check: u64,
    period_ticks: u64,
}

impl SelfConformanceGate {
    pub fn new(period_ticks: u64) -> Self {
        Self { last_fingerprint: None, ticks_since_check: 0, period_ticks }
    }

    /// Advance the clock by `ticks` and, if a re-conformance is due (startup / period / fingerprint
    /// change), run it and return the decision; otherwise `None` (no check this poll). On a check the
    /// fingerprint and clock are recorded regardless of Admit/Refuse, so a persistently-drifted stack keeps
    /// being refused rather than re-triggering every poll.
    pub fn poll(
        &mut self,
        backend: &dyn VerifiableInferenceBackend,
        vectors: &[ConformanceVector],
        current: &StackFingerprint,
        ticks: u64,
    ) -> Option<RegistrationDecision> {
        self.ticks_since_check = self.ticks_since_check.saturating_add(ticks);
        if should_reconform(self.last_fingerprint.as_ref(), current, self.ticks_since_check, self.period_ticks) {
            let decision = run_self_conformance(backend, vectors, current.clone());
            self.last_fingerprint = Some(current.clone());
            self.ticks_since_check = 0;
            Some(decision)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw::{PalwRuntimeProfileV1, PalwSamplingParams, PalwTier};
    use crate::palw_replica::{MockDeterministicRuntime, VerifiableInferenceBackend};
    use kaspa_hashes::Hash64;

    fn h(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }
    fn profile() -> PalwRuntimeProfileV1 {
        PalwRuntimeProfileV1 {
            version: 1,
            tier: PalwTier::Quality,
            model_id: PalwTier::Quality.model_id(),
            tokenizer_hash: h(1),
            quantization_manifest_hash: h(2),
            runtime_image_hash: h(3),
            kernel_graph_hash: h(4),
            operation_table_hash: h(5),
            shape_table_hash: h(6),
            gpu_arch_class: 100,
            tensor_parallel_degree: 1,
            pipeline_parallel_degree: 1,
            deterministic_reduction: true,
            batch_invariant: true,
            speculative_decode: false,
            sampling: PalwSamplingParams::greedy(),
        }
    }
    const JS: &[u8] = b"job-set-descriptor";

    fn fp(driver: &str) -> StackFingerprint {
        StackFingerprint {
            gpu_arch: "sm_89".into(),
            driver_version: driver.into(),
            os_build: "linux-6.8".into(),
            runtime_toolchain: "cuda-12.4".into(),
        }
    }

    fn v_i(backend: &MockDeterministicRuntime) -> Vec<ConformanceVector> {
        vec![ConformanceVector {
            job_set_descriptor: JS.to_vec(),
            prompt: b"q".to_vec(),
            output_salt: [7u8; 32],
            expected: backend.infer_with_trace(JS, b"q", &[7u8; 32]),
        }]
    }

    /// Startup: the first poll always runs a check; a conforming stack is admitted with its fingerprint.
    #[test]
    fn startup_admits_a_conforming_stack() {
        let backend = MockDeterministicRuntime::new(profile(), 3, 2);
        let vectors = v_i(&backend);
        let mut gate = SelfConformanceGate::new(1_000);
        let d = gate.poll(&backend, &vectors, &fp("550.1"), 0).expect("startup runs a check");
        assert_eq!(d, RegistrationDecision::Admit { fingerprint: fp("550.1") });
        assert!(d.admitted());
    }

    /// Steady state: within the period and with an unchanged fingerprint, no re-check runs.
    #[test]
    fn no_recheck_within_period_and_same_fingerprint() {
        let backend = MockDeterministicRuntime::new(profile(), 3, 2);
        let vectors = v_i(&backend);
        let mut gate = SelfConformanceGate::new(1_000);
        assert!(gate.poll(&backend, &vectors, &fp("550.1"), 0).is_some()); // startup
        assert!(gate.poll(&backend, &vectors, &fp("550.1"), 999).is_none(), "under period, same fp ⇒ no check");
    }

    /// The period trigger fires a re-check once the fixed period elapses.
    #[test]
    fn period_elapse_triggers_recheck() {
        let backend = MockDeterministicRuntime::new(profile(), 3, 2);
        let vectors = v_i(&backend);
        let mut gate = SelfConformanceGate::new(1_000);
        assert!(gate.poll(&backend, &vectors, &fp("550.1"), 0).is_some()); // startup
        assert!(gate.poll(&backend, &vectors, &fp("550.1"), 1_000).is_some(), "period elapsed ⇒ re-check");
    }

    /// The in-flight-driver-update hole: a fingerprint change forces a re-check even inside the period, and
    /// if the updated stack no longer reproduces V_i it is refused fail-closed (a §13 new-set candidate).
    #[test]
    fn fingerprint_change_forces_recheck_and_fails_closed_on_drift() {
        let backend = MockDeterministicRuntime::new(profile(), 3, 2);
        let good = v_i(&backend);
        let mut gate = SelfConformanceGate::new(1_000);
        assert_eq!(gate.poll(&backend, &good, &fp("550.1"), 0), Some(RegistrationDecision::Admit { fingerprint: fp("550.1") }));

        // Model a codegen drift after a driver update: the committed set no longer matches this stack.
        let mut drifted = good.clone();
        drifted[0].expected.canonical_gemm_trace_root = h(0x99);
        let d = gate.poll(&backend, &drifted, &fp("560.0"), 1).expect("fingerprint change forces a re-check inside the period");
        assert!(matches!(d, RegistrationDecision::Refuse { .. }), "drift must fail closed, got {d:?}");
    }

    /// A tier with no committed set is not registerable (fail-closed on the empty set).
    #[test]
    fn empty_set_is_refused() {
        let backend = MockDeterministicRuntime::new(profile(), 3, 2);
        let d = run_self_conformance(&backend, &[], fp("550.1"));
        assert!(matches!(d, RegistrationDecision::Refuse { report: ConformanceReport::EmptySet }), "{d:?}");
    }
}
