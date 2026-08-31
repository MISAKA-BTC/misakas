//! Client for the `palw-agent` UDS protocol `misaka-palw-agent-borsh/v1`
//! (docs/misaka-palw-vps-canonical-worker-design-v0.1-ja.md §5, §10.3).
//!
//! One connection carries one framed request and one framed response; per the wire contract the
//! client **half-closes its write side after the request frame** so the agent can verify no
//! trailing bytes follow (skipping the half-close deadlocks into the agent's read timeout).
//!
//! # Trust stance
//!
//! The agent already re-binds the worker's result before answering — and this client re-verifies
//! the binding AGAIN from its own inputs (request hash over its own canonical encoding, job id,
//! token counts, CU re-derivation, and the full `job_context_hash` recomputed locally). kaspad
//! must never take a compute answer on the supervisor's word alone: a compromised or buggy agent
//! is exactly the component this second, independent check is for. What no client check can do
//! is prove the model actually ran — that remains the job of independent replay, committees and
//! bonds (v2 design §4).

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kaspa_consensus_core::palw_v2::{
    PALW_JOB_WIRE_VERSION_V2, PALW_V2_MAX_FRAME_BYTES, PalwAgentHealthV1, PalwAgentRequestV1, PalwAgentResponseV1, PalwJobContextV2,
    PalwJobEnvelopeV2, PalwJobResultV2, PalwStopReasonV2, canonical_compute_units_v2, decode_framed_borsh, job_request_hash_v2,
    read_framed, tokenizer_id_v2_for_gguf, write_framed,
};
use kaspa_consensus_core::vlt::qwen35_pins;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwAgentClientError {
    #[error("agent io: {0}")]
    Io(String),
    #[error("agent protocol violation: {0}")]
    Protocol(String),
    /// An admission decision — nothing executed. Safe to retry elsewhere or later.
    #[error("job rejected by the agent ({code}): {message}")]
    Rejected { code: String, message: String },
    /// A worker ran and did not produce an accepted result. Evidence about this host.
    #[error("job failed on the agent ({code}): {message}")]
    Failed { code: String, message: String },
    /// The agent answered with a result that is not THIS job's result. Treat as hostile.
    #[error("response binding violation: {0}")]
    Binding(String),
}

fn io_err(e: impl std::fmt::Display) -> PalwAgentClientError {
    PalwAgentClientError::Io(e.to_string())
}

pub struct PalwAgentClient {
    endpoint: PathBuf,
    /// Ceiling for connect/write and for the HEALTH read. Job reads get their own ceiling per
    /// call — a real job legitimately runs orders of magnitude longer than a health probe.
    io_timeout: Duration,
}

impl PalwAgentClient {
    pub fn new(endpoint: impl Into<PathBuf>, io_timeout: Duration) -> Self {
        Self { endpoint: endpoint.into(), io_timeout }
    }

    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    fn round_trip(&self, request: &PalwAgentRequestV1, read_timeout: Duration) -> Result<PalwAgentResponseV1, PalwAgentClientError> {
        let mut stream = UnixStream::connect(&self.endpoint).map_err(io_err)?;
        stream.set_write_timeout(Some(self.io_timeout)).map_err(io_err)?;
        stream.set_read_timeout(Some(read_timeout)).map_err(io_err)?;
        let payload = borsh::to_vec(request).map_err(|e| PalwAgentClientError::Protocol(e.to_string()))?;
        write_framed(&mut stream, &payload).map_err(io_err)?;
        // The wire contract: half-close after the one request frame.
        stream.shutdown(std::net::Shutdown::Write).map_err(io_err)?;
        let response_payload =
            read_framed(&mut stream, PALW_V2_MAX_FRAME_BYTES).map_err(|e| PalwAgentClientError::Protocol(e.to_string()))?;
        decode_framed_borsh(&response_payload).map_err(|e| PalwAgentClientError::Protocol(e.to_string()))
    }

    pub fn health(&self) -> Result<PalwAgentHealthV1, PalwAgentClientError> {
        match self.round_trip(&PalwAgentRequestV1::Health, self.io_timeout)? {
            PalwAgentResponseV1::Health(health) => Ok(health),
            other => Err(PalwAgentClientError::Protocol(format!("health probe answered with {other:?}"))),
        }
    }

    /// Executes (or replays — the envelope's mode decides) one canonical job through the agent
    /// and re-verifies the result binding locally before returning it.
    pub fn execute(&self, envelope: &PalwJobEnvelopeV2, job_read_timeout: Duration) -> Result<PalwJobResultV2, PalwAgentClientError> {
        match self.round_trip(&PalwAgentRequestV1::Job(envelope.clone()), job_read_timeout)? {
            PalwAgentResponseV1::JobOk(result) => {
                validate_result_binding(envelope, &result)?;
                Ok(result)
            }
            PalwAgentResponseV1::JobRejected { code, message } => Err(PalwAgentClientError::Rejected { code, message }),
            PalwAgentResponseV1::JobFailed { code, message } => Err(PalwAgentClientError::Failed { code, message }),
            PalwAgentResponseV1::Health(_) => Err(PalwAgentClientError::Protocol("job request answered with a health frame".into())),
        }
    }
}

/// The client-side re-verification. Everything here derives from the caller's OWN envelope —
/// nothing is taken from the response except the fields under test.
pub fn validate_result_binding(envelope: &PalwJobEnvelopeV2, result: &PalwJobResultV2) -> Result<(), PalwAgentClientError> {
    let fail = |what: &str| Err(PalwAgentClientError::Binding(what.to_string()));
    if result.version != PALW_JOB_WIRE_VERSION_V2 {
        return fail("result version is not v2");
    }
    let canonical_request = borsh::to_vec(envelope).map_err(|e| PalwAgentClientError::Protocol(e.to_string()))?;
    if result.request_hash != job_request_hash_v2(&canonical_request) {
        return fail("request hash does not bind this envelope's canonical encoding");
    }
    if result.job_id != envelope.job_id {
        return fail("result names a different job");
    }
    let projection = &result.projection;
    let expected_context =
        PalwJobContextV2::from_envelope(envelope, tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256)).context_hash();
    if projection.job_context_hash != expected_context {
        return fail("job context hash does not re-derive from this envelope");
    }
    if projection.prefill_tokens != envelope.declared_prefill_tokens()
        || projection.decode_tokens != envelope.exact_decode_tokens
        || projection.trace_event_count != envelope.exact_decode_tokens
    {
        return fail("token counts contradict the job's exact budgets");
    }
    if projection.canonical_compute_units
        != canonical_compute_units_v2(envelope.declared_prefill_tokens(), envelope.exact_decode_tokens)
    {
        return fail("CU does not re-derive from the canonical ruleset");
    }
    if projection.stop_reason != PalwStopReasonV2::ExactBudgetReached {
        return fail("only exact-budget completion bears a receipt in this profile");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_v2::{
        PalwJobModeV2, PalwJobTelemetryV2, PalwResultProjectionV2, output_commitment_v2, rendered_output_hash_v2,
    };
    use kaspa_hashes::Hash64;
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn h64(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    fn test_envelope() -> PalwJobEnvelopeV2 {
        PalwJobEnvelopeV2 {
            version: 2,
            network_id: b"misaka-devnet".to_vec(),
            job_id: h64(0x11),
            job_nullifier: h64(0x22),
            mode: PalwJobModeV2::Execute,
            model_profile_id: h64(0x33),
            runtime_manifest_hash: h64(0x44),
            runtime_class_id: h64(0x55),
            shape_profile_id: h64(0x66),
            trace_scheme_id: h64(0x77),
            cu_ruleset_id: h64(0x88),
            execution_seed: [0xAB; 32],
            prompt_token_ids: vec![5, 6, 7, 8, 9],
            exact_decode_tokens: 4,
            max_context_tokens: 4096,
            assignment_id: h64(0x99),
            assignment_epoch: 7,
            deadline_unix_ms: 0,
        }
    }

    /// A result that satisfies every client-side binding check for `envelope`, computed the same
    /// way an honest worker+agent chain would.
    fn well_bound_result(envelope: &PalwJobEnvelopeV2) -> PalwJobResultV2 {
        let context_hash =
            PalwJobContextV2::from_envelope(envelope, tokenizer_id_v2_for_gguf(qwen35_pins::GGUF_SHA256)).context_hash();
        PalwJobResultV2 {
            version: PALW_JOB_WIRE_VERSION_V2,
            request_hash: job_request_hash_v2(&borsh::to_vec(envelope).unwrap()),
            job_id: envelope.job_id,
            projection: PalwResultProjectionV2 {
                job_context_hash: context_hash,
                full_logits_trace_root: h64(0xE1),
                output_commitment: output_commitment_v2(&context_hash, &[1, 2, 3, 4], &rendered_output_hash_v2(b"x")),
                operation_schedule_commitment: h64(0xE2),
                canonical_compute_units: canonical_compute_units_v2(5, 4),
                prefill_tokens: 5,
                decode_tokens: 4,
                trace_event_count: 4,
                stop_reason: PalwStopReasonV2::ExactBudgetReached,
            },
            telemetry: PalwJobTelemetryV2::default(),
        }
    }

    /// One-shot mock agent: accepts one connection, hands the request payload to `respond`, and
    /// writes back whatever it returns. The socket path is kept short — a UDS path over ~104
    /// bytes fails to bind on macOS.
    fn mock_agent(respond: impl FnOnce(Vec<u8>) -> Vec<u8> + Send + 'static) -> (PathBuf, std::thread::JoinHandle<()>) {
        static SOCK_SEQ: AtomicU32 = AtomicU32::new(0);
        let path =
            std::env::temp_dir().join(format!("palw-ac-{}-{}.sock", std::process::id(), SOCK_SEQ.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("bind mock agent socket");
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let payload = read_framed(&mut stream, PALW_V2_MAX_FRAME_BYTES).expect("request frame");
            let response = respond(payload);
            stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            std::io::Write::write_all(&mut stream, &response).expect("write response");
            // **The mock obeys the wire contract, because the real agent has to.**
            //
            // `read_framed` requires EOF after the frame and documents how it is delivered: the
            // sender half-closes. This mock used to just return and let the thread's stack drop
            // the stream, which sends the FIN eventually — and "eventually" is a scheduler
            // decision. Under load the client's one-byte EOF probe ran first, blocked for the
            // whole read timeout and came back `EAGAIN`, so a test asserting `Binding(_)` saw
            // `Protocol("read after frame failed: …")` after exactly 5.00s. Alone it passed in
            // 0.00s.
            //
            // The fix is not a longer timeout — that makes the race rarer, not absent, and would
            // leave the mock modelling an agent that does something no real agent may do. The
            // production agent had the SAME omission and it is fixed in the same change
            // (`misaka-palw-agent/src/agent.rs`), so this mock is once again a faithful stand-in:
            // if the real half-close is ever removed, these tests go red rather than slow.
            let _ = stream.shutdown(std::net::Shutdown::Write);
        });
        (path, handle)
    }

    fn framed(payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(payload.len() + 4);
        write_framed(&mut out, payload).unwrap();
        out
    }

    fn client(path: &Path) -> PalwAgentClient {
        PalwAgentClient::new(path, Duration::from_secs(5))
    }

    #[test]
    fn health_round_trips() {
        let health = PalwAgentHealthV1 {
            state: kaspa_consensus_core::palw_v2::PalwAgentStateV1::Ready,
            selftest_passed: true,
            runtime_manifest_hash: h64(1),
            golden_vector_root: h64(2),
            max_context_tokens: 4096,
            jobs_total: 3,
            jobs_ok: 1,
            jobs_rejected: 2,
            jobs_failed: 0,
            timeouts_total: 0,
        };
        let expected = health.clone();
        let (path, handle) = mock_agent(move |request| {
            let decoded: PalwAgentRequestV1 = decode_framed_borsh(&request).unwrap();
            assert_eq!(decoded, PalwAgentRequestV1::Health);
            framed(&borsh::to_vec(&PalwAgentResponseV1::Health(health)).unwrap())
        });
        assert_eq!(client(&path).health().unwrap(), expected);
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn execute_accepts_a_well_bound_result() {
        let envelope = test_envelope();
        let (path, handle) = mock_agent(|request| {
            let decoded: PalwAgentRequestV1 = decode_framed_borsh(&request).unwrap();
            let PalwAgentRequestV1::Job(env) = decoded else { panic!("expected a job") };
            framed(&borsh::to_vec(&PalwAgentResponseV1::JobOk(well_bound_result(&env))).unwrap())
        });
        let result = client(&path).execute(&envelope, Duration::from_secs(5)).unwrap();
        assert_eq!(result.job_id, envelope.job_id);
        assert_eq!(result.projection.decode_tokens, 4);
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejected_and_failed_map_to_their_errors() {
        let envelope = test_envelope();
        let (path, handle) = mock_agent(|_| {
            framed(&borsh::to_vec(&PalwAgentResponseV1::JobRejected { code: "busy".into(), message: "slot occupied".into() }).unwrap())
        });
        let err = client(&path).execute(&envelope, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Rejected { ref code, .. } if code == "busy"), "{err:?}");
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);

        let (path, handle) = mock_agent(|_| {
            framed(&borsh::to_vec(&PalwAgentResponseV1::JobFailed { code: "timeout".into(), message: "killed".into() }).unwrap())
        });
        let err = client(&path).execute(&test_envelope(), Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Failed { ref code, .. } if code == "timeout"), "{err:?}");
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn misbound_results_are_hostile() {
        // Wrong job id.
        let envelope = test_envelope();
        let (path, handle) = mock_agent(|request| {
            let decoded: PalwAgentRequestV1 = decode_framed_borsh(&request).unwrap();
            let PalwAgentRequestV1::Job(env) = decoded else { panic!() };
            let mut result = well_bound_result(&env);
            result.job_id = h64(0xEE);
            framed(&borsh::to_vec(&PalwAgentResponseV1::JobOk(result)).unwrap())
        });
        let err = client(&path).execute(&envelope, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Binding(_)), "{err:?}");
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);

        // Wrong request hash.
        let (path, handle) = mock_agent(|request| {
            let decoded: PalwAgentRequestV1 = decode_framed_borsh(&request).unwrap();
            let PalwAgentRequestV1::Job(env) = decoded else { panic!() };
            let mut result = well_bound_result(&env);
            result.request_hash = h64(0xEF);
            framed(&borsh::to_vec(&PalwAgentResponseV1::JobOk(result)).unwrap())
        });
        let err = client(&path).execute(&test_envelope(), Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Binding(_)), "{err:?}");
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);

        // Wrong CU (a worker under-reporting its own ruleset).
        let (path, handle) = mock_agent(|request| {
            let decoded: PalwAgentRequestV1 = decode_framed_borsh(&request).unwrap();
            let PalwAgentRequestV1::Job(env) = decoded else { panic!() };
            let mut result = well_bound_result(&env);
            result.projection.canonical_compute_units += 1;
            framed(&borsh::to_vec(&PalwAgentResponseV1::JobOk(result)).unwrap())
        });
        let err = client(&path).execute(&test_envelope(), Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Binding(_)), "{err:?}");
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn protocol_violations_and_unreachable_endpoints_fail_closed() {
        // Trailing bytes after the response frame.
        let envelope = test_envelope();
        let (path, handle) = mock_agent(|request| {
            let decoded: PalwAgentRequestV1 = decode_framed_borsh(&request).unwrap();
            let PalwAgentRequestV1::Job(env) = decoded else { panic!() };
            let mut bytes = framed(&borsh::to_vec(&PalwAgentResponseV1::JobOk(well_bound_result(&env))).unwrap());
            bytes.push(0xFF);
            bytes
        });
        let err = client(&path).execute(&envelope, Duration::from_secs(5)).unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Protocol(_)), "{err:?}");
        handle.join().unwrap();
        let _ = std::fs::remove_file(&path);

        // Nobody listening.
        let err = client(Path::new("/tmp/palw-ac-nonexistent.sock")).health().unwrap_err();
        assert!(matches!(err, PalwAgentClientError::Io(_)), "{err:?}");
    }
}
