//! The qi35-serve class → [`ReplicaMatchKey`] mapping — where the desktop protocol meets the
//! node's real k=2 predicate.
//!
//! The eight-field key and its equality rule are the consensus lane's leaf-minting predicate
//! (design §7.5); this module builds keys for the DESKTOP CHAT class from what the gateway
//! wires over, using the same domain-separated constructors consensus uses
//! ([`job_set_commitment`], [`gemm_trace_root`], [`operation_schedule_commitment`], and the
//! `MIL_PALW_*` domains):
//!
//! * `job_set_commitment`  ← canonical job descriptor: class label ‖ job_id ‖ max_new ‖ prompt ids (LE).
//! * `model_profile_id`    ← `Hash64_k(model-profile, MODEL_PROFILE_LABEL)`.
//! * `runtime_class_id`    ← `Hash64_k(runtime-class, RUNTIME_CLASS_LABEL)`.
//! * `output_commitment`   ← `Hash64_k(output, decoded output_root)`. The wire carries the
//!   gateway's blake2b-256 over output token ids, not the ids themselves, so this is an
//!   equality-preserving re-keying — NOT byte-identical to a consensus leaf's
//!   `output_commitment(salt, ids)`, which needs the beacon-derived salt this off-chain bridge
//!   does not have. That parity is a consensus-side seam, stated in the README.
//! * `canonical_gemm_trace_root` ← [`gemm_trace_root`] over the engine's ROUTE root bytes. The
//!   design defines this field as a keyed hash over "the already-serialized canonical trace";
//!   for the qi35-serve class the canonical execution trace commitment the engine exports IS
//!   its MoE routing-trace root.
//! * `operation_schedule_commitment` ← [`operation_schedule_commitment`] over KV-root ‖
//!   STATE-root bytes — the class's deterministic execution-state schedule commitment.
//! * `shape_id` / `quantum_count` ← fixed (1, 1): one serve-shape, one quantum per chat job in
//!   class v1.
//!
//! Two honest replicas of the same job therefore agree on all eight fields; a replica that
//! produced the same tokens through a DIFFERENT execution (other routing, other state) fails
//! the key even though the output matches — which is exactly what §7.5 is for.

use kaspa_hashes::{Hash64, blake2b_512_keyed};
use misaka_palw::domains::{MIL_PALW_OUTPUT_DOMAIN, MIL_PALW_PROFILE_DOMAIN, MIL_PALW_RUNTIME_CLASS_DOMAIN};
use misaka_palw::palw::{ReplicaMatchKey, gemm_trace_root, job_set_commitment, operation_schedule_commitment};
use misaka_palw::palw_replica::{ReplicaK2Outcome, run_replica_k2};

use crate::wire::RuntimeRootsV1;

/// The pinned runtime-class label for keys built by this bridge: the canonical integer QI35
/// engine, Metal v3 kernels, Phase-D (1 token = 1 command buffer) serve loop.
pub const RUNTIME_CLASS_LABEL: &[u8] = b"qi35-serve-metal-v3-phase-d.v1";
/// The pinned model-profile label: Qwen3.6-35B-A3B under the QI35 canonical integer profile.
pub const MODEL_PROFILE_LABEL: &[u8] = b"qwen3.6-35b-a3b-qi35-int.v1";

pub fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return Err(format!("bad hex length {}", hex.len()));
    }
    let mut out = vec![0u8; hex.len() / 2];
    faster_hex::hex_decode(hex.as_bytes(), &mut out).map_err(|e| format!("bad hex: {e}"))?;
    Ok(out)
}

/// The canonical job descriptor fed to [`job_set_commitment`] — binds the class label, the job
/// id, the generation bound, and the exact prompt ids.
fn job_descriptor(job_id: &str, max_new: u32, prompt_ids: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(RUNTIME_CLASS_LABEL.len() + 1 + job_id.len() + 1 + 4 + prompt_ids.len() * 4);
    bytes.extend_from_slice(RUNTIME_CLASS_LABEL);
    bytes.push(0);
    bytes.extend_from_slice(job_id.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(&max_new.to_le_bytes());
    for id in prompt_ids {
        bytes.extend_from_slice(&id.to_le_bytes());
    }
    bytes
}

/// Build one side's key. `output_root` and the three execution roots come from THAT side
/// (A's submission or B's result); everything else comes from the job.
pub fn build_match_key(
    job_id: &str,
    max_new: u32,
    prompt_ids: &[u32],
    output_root_hex: &str,
    roots: &RuntimeRootsV1,
) -> Result<ReplicaMatchKey, String> {
    let output_bytes = decode_hex(output_root_hex).map_err(|e| format!("output_root: {e}"))?;
    let route_bytes = decode_hex(&roots.route).map_err(|e| format!("runtime_roots.route: {e}"))?;
    let kv_bytes = decode_hex(&roots.kv).map_err(|e| format!("runtime_roots.kv: {e}"))?;
    let state_bytes = decode_hex(&roots.state).map_err(|e| format!("runtime_roots.state: {e}"))?;
    let mut kv_state = kv_bytes;
    kv_state.extend_from_slice(&state_bytes);
    Ok(ReplicaMatchKey {
        job_set_commitment: job_set_commitment(&job_descriptor(job_id, max_new, prompt_ids)),
        model_profile_id: blake2b_512_keyed(MIL_PALW_PROFILE_DOMAIN, MODEL_PROFILE_LABEL),
        runtime_class_id: blake2b_512_keyed(MIL_PALW_RUNTIME_CLASS_DOMAIN, RUNTIME_CLASS_LABEL),
        shape_id: 1,
        output_commitment: blake2b_512_keyed(MIL_PALW_OUTPUT_DOMAIN, &output_bytes),
        canonical_gemm_trace_root: gemm_trace_root(&route_bytes),
        operation_schedule_commitment: operation_schedule_commitment(&kv_state),
        quantum_count: 1,
    })
}

/// The bridge's match decision: build both keys and run the node's k=2 rule. Returns the shared
/// key on a match (the would-be leaf key), None on a mismatch.
pub fn k2_match(
    job_id: &str,
    max_new: u32,
    prompt_ids: &[u32],
    a_output_root: &str,
    a_roots: &RuntimeRootsV1,
    b_output_root: &str,
    b_roots: &RuntimeRootsV1,
) -> Result<Option<ReplicaMatchKey>, String> {
    let key_a = build_match_key(job_id, max_new, prompt_ids, a_output_root, a_roots)?;
    let key_b = build_match_key(job_id, max_new, prompt_ids, b_output_root, b_roots)?;
    Ok(match run_replica_k2(&key_a, &key_b) {
        ReplicaK2Outcome::Matched(shared) => Some(shared),
        ReplicaK2Outcome::Mismatch => None,
    })
}

pub fn hash64_hex(h: &Hash64) -> String {
    let bytes = h.as_byte_slice();
    let mut out = vec![0u8; bytes.len() * 2];
    faster_hex::hex_encode(bytes, &mut out).expect("exact-size buffer");
    String::from_utf8(out).expect("hex is ascii")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(route: &str, kv: &str, state: &str) -> RuntimeRootsV1 {
        RuntimeRootsV1 { route: route.into(), kv: kv.into(), state: state.into() }
    }

    #[test]
    fn honest_replicas_match_through_the_real_k2_rule() {
        let r = roots("aa11", "bb22", "cc33");
        let shared = k2_match("job-1", 256, &[1, 2, 3], "dd44", &r, "dd44", &r).unwrap();
        let key = shared.expect("identical execution must match");
        // The shared key is deterministic — rebuild equals.
        let rebuilt = build_match_key("job-1", 256, &[1, 2, 3], "dd44", &r).unwrap();
        assert_eq!(key, rebuilt);
    }

    #[test]
    fn any_divergent_field_is_a_mismatch() {
        let r = roots("aa11", "bb22", "cc33");
        // Different output.
        assert!(k2_match("job-1", 256, &[1], "dd44", &r, "ee55", &r).unwrap().is_none());
        // Same output, different ROUTING — the execution-structure catch output-only matching misses.
        let other_route = roots("ff66", "bb22", "cc33");
        assert!(k2_match("job-1", 256, &[1], "dd44", &r, "dd44", &other_route).unwrap().is_none());
        // Same output, different recurrent state.
        let other_state = roots("aa11", "bb22", "9999");
        assert!(k2_match("job-1", 256, &[1], "dd44", &r, "dd44", &other_state).unwrap().is_none());
    }

    #[test]
    fn job_binding_separates_jobs_and_prompts() {
        let r = roots("aa", "bb", "cc");
        let k1 = build_match_key("job-1", 256, &[1, 2], "dd", &r).unwrap();
        let k2 = build_match_key("job-2", 256, &[1, 2], "dd", &r).unwrap();
        let k3 = build_match_key("job-1", 256, &[1, 3], "dd", &r).unwrap();
        let k4 = build_match_key("job-1", 128, &[1, 2], "dd", &r).unwrap();
        assert_ne!(k1.job_set_commitment, k2.job_set_commitment);
        assert_ne!(k1.job_set_commitment, k3.job_set_commitment);
        assert_ne!(k1.job_set_commitment, k4.job_set_commitment);
    }

    #[test]
    fn hex_is_validated() {
        let r = roots("zz", "bb", "cc");
        assert!(build_match_key("j", 1, &[], "dd", &r).is_err(), "non-hex route refused");
        let r = roots("a", "bb", "cc");
        assert!(build_match_key("j", 1, &[], "dd", &r).is_err(), "odd-length refused");
        assert!(decode_hex("").is_err(), "empty refused");
    }
}
