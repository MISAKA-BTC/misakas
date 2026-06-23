//! F003 `MLDSA87_VERIFY` — the post-quantum signature-verify precompile
//! (PREA design v1.1 §9 / FSL §4.3, P0-1).
//!
//! A PURE verify: it changes no state and moves no value, so — unlike F002 — it
//! is reachable from any frame including `STATICCALL` (a contract can verify in a
//! `view` function). Implemented as the same **call-frame interception** seam as
//! F002 (`handler.execution.call` wrap) so executor and `eth_call`/`eth_estimateGas`
//! simulation share ONE registration path (parity).
//!
//! Calldata is **version-discriminated** by `input[0]`:
//! - `0x01` (FSL generic): `version(1) ‖ pubkey(2592) ‖ message_hash64(64) ‖
//!   signature(4627)` (7284 B). Verifies `signature` over `message_hash64` under
//!   [`F003_FSL_VERIFY_MLDSA87_CONTEXT`].
//! - `0x02` (PREA root): `version(1) ‖ expected_key_payload64(64) ‖
//!   message_hash64(64) ‖ pubkey(2592) ‖ signature(4627)` (7348 B). FIRST binds
//!   the pubkey to its UTXO address payload
//!   (`blake2b_512(MLDSA87_ADDRESS_CONTEXT, pubkey) == expected_key_payload64`),
//!   THEN verifies under [`F003_PREA_ROOT_MLDSA87_CONTEXT`].
//!
//! Output is a 32-byte ABI `bool` (`0x…01` valid / `0x…00` otherwise). Any
//! malformed length, unknown version, key-payload mismatch, or invalid signature
//! returns ABI `false` — it NEVER panics and NEVER reverts (a value-bearing call
//! is the one exception: F003 is non-payable, so a non-zero `msg.value` reverts so
//! the value is not silently stranded). [`F003_VERIFY_GAS`] is charged up-front
//! (before dispatch) so a malformed flood pays the same as a real verify; the gas
//! is the deterministic per-block/per-tx bound on lattice-verify CPU.
//!
//! Determinism: verification reuses [`kaspa_txscript::verify_mldsa87_with_context`],
//! which calls the libcrux PORTABLE verify (audit H-2 — NOT the per-CPU AVX2/NEON
//! multiplexer), so accept/reject is bit-identical on every node/CPU.

use kaspa_consensus_core::evm::{
    F003_FSL_VERIFY_MLDSA87_CONTEXT, F003_INPUT_LEN_FSL, F003_INPUT_LEN_PREA, F003_PREA_ROOT_MLDSA87_CONTEXT, F003_VERIFY_GAS,
    F003_VERSION_FSL_GENERIC, F003_VERSION_PREA_ROOT, MISAKA_MLDSA_VERIFY_PRECOMPILE,
};
use kaspa_hashes::blake2b_512_address_payload;
use kaspa_txscript::verify_mldsa87_with_context;
use revm::handler::register::EvmHandler;
use revm::interpreter::{CallOutcome, Gas, InstructionResult, InterpreterResult};
use revm::primitives::{Address, Bytes};
use revm::{Database, FrameOrResult, FrameResult};

/// The F003 address as a revm `Address`.
pub fn f003_address() -> Address {
    Address::from(MISAKA_MLDSA_VERIFY_PRECOMPILE.as_bytes())
}

const MLDSA87_PK_LEN: usize = 2592;
const MSG_HASH64_LEN: usize = 64;
const KEY_PAYLOAD64_LEN: usize = 64;
// The signature is the remaining tail (length is fixed by the exact-match input
// length check + verify_mldsa87_with_context's own MLDSA87_SIG_LEN guard).

/// The pure F003 logic: parse the versioned input and return whether it verifies.
/// `false` (never panic) for ANY malformed input, unknown version, key-payload
/// mismatch, or invalid signature. This is the consensus-critical decision; the
/// handler only wraps it with gas + ABI encoding.
pub fn run_f003_verify(input: &[u8]) -> bool {
    match input.first().copied() {
        Some(F003_VERSION_FSL_GENERIC) => {
            if input.len() != F003_INPUT_LEN_FSL {
                return false;
            }
            let pubkey = &input[1..1 + MLDSA87_PK_LEN];
            let msg = &input[1 + MLDSA87_PK_LEN..1 + MLDSA87_PK_LEN + MSG_HASH64_LEN];
            let sig = &input[1 + MLDSA87_PK_LEN + MSG_HASH64_LEN..];
            verify_mldsa87_with_context(pubkey, msg, sig, F003_FSL_VERIFY_MLDSA87_CONTEXT).unwrap_or(false)
        }
        Some(F003_VERSION_PREA_ROOT) => {
            if input.len() != F003_INPUT_LEN_PREA {
                return false;
            }
            let expected = &input[1..1 + KEY_PAYLOAD64_LEN];
            let msg = &input[1 + KEY_PAYLOAD64_LEN..1 + KEY_PAYLOAD64_LEN + MSG_HASH64_LEN];
            let pubkey = &input[1 + KEY_PAYLOAD64_LEN + MSG_HASH64_LEN..1 + KEY_PAYLOAD64_LEN + MSG_HASH64_LEN + MLDSA87_PK_LEN];
            let sig = &input[1 + KEY_PAYLOAD64_LEN + MSG_HASH64_LEN + MLDSA87_PK_LEN..];
            // Bind the pubkey to its UTXO address payload BEFORE verifying — this is
            // what makes the F003-0x02 result attest "this key owns that PQ identity".
            if blake2b_512_address_payload(pubkey).as_bytes() != expected {
                return false;
            }
            verify_mldsa87_with_context(pubkey, msg, sig, F003_PREA_ROOT_MLDSA87_CONTEXT).unwrap_or(false)
        }
        _ => false,
    }
}

/// The 32-byte ABI-bool output (`0x…01` for `true`, all-zero for `false`).
fn abi_bool(b: bool) -> Bytes {
    let mut out = [0u8; 32];
    if b {
        out[31] = 1;
    }
    Bytes::from(out.to_vec())
}

/// Wrap `handler.execution.call` so calls targeting F003 run the verify instead
/// of loading (empty) code. Everything else delegates to the previous handle.
/// Registered ONLY when the F003 fence is active (see
/// [`crate::precompiles::register_all_misaka_precompiles`]); below the fence the
/// handler is absent and a call to `0x…F003` behaves as a call to an empty
/// account (byte-identical execution).
pub fn register_f003_mldsa_verify<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {
    let prev = handler.execution.call.clone();
    handler.execution.call = std::sync::Arc::new(move |ctx, inputs| {
        let f003 = f003_address();
        if inputs.target_address != f003 || inputs.bytecode_address != f003 {
            return prev(ctx, inputs);
        }
        // Charge the fixed cost first; an under-gassed call fails outright (this is
        // the per-block/per-tx bound on verify CPU — paid by malformed calls too).
        let mut gas = Gas::new(inputs.gas_limit);
        if !gas.record_cost(F003_VERIFY_GAS) {
            return Ok(FrameOrResult::Result(FrameResult::Call(CallOutcome::new(
                InterpreterResult { result: InstructionResult::PrecompileOOG, output: Bytes::new(), gas },
                inputs.return_memory_offset.clone(),
            ))));
        }
        // F003 is NON-PAYABLE (a pure verify): a value-bearing call reverts so the
        // value is never silently stranded in the precompile. STATICCALL (value 0)
        // and zero-value CALL are fine. delegate/callcode never match (target is
        // the caller's own address, handled by the pass-through above).
        if let Some(v) = inputs.value.transfer() {
            if !v.is_zero() {
                return Ok(FrameOrResult::Result(FrameResult::Call(CallOutcome::new(
                    InterpreterResult { result: InstructionResult::Revert, output: Bytes::new(), gas },
                    inputs.return_memory_offset.clone(),
                ))));
            }
        }
        let ok = run_f003_verify(&inputs.input);
        Ok(FrameOrResult::Result(FrameResult::Call(CallOutcome::new(
            InterpreterResult { result: InstructionResult::Return, output: abi_bool(ok), gas },
            inputs.return_memory_offset.clone(),
        ))))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{MAX_MLDSA_AUTH_BYTES_PER_EVM_BLOCK, MAX_MLDSA_VERIFY_PER_EVM_BLOCK, MAX_MLDSA_VERIFY_PER_TX};
    use libcrux_ml_dsa::ml_dsa_87 as mldsa;

    const PREA_CTX: &[u8] = F003_PREA_ROOT_MLDSA87_CONTEXT;
    const FSL_CTX: &[u8] = F003_FSL_VERIFY_MLDSA87_CONTEXT;

    /// (pubkey 2592, signature 4627) for `msg` under `ctx`, from a fixed seed.
    fn keyed(seed: u8, msg: &[u8], ctx: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let kp = mldsa::generate_key_pair([seed; 32]);
        let sig = mldsa::sign(&kp.signing_key, msg, ctx, [seed ^ 0xA5; 32]).expect("sign");
        (kp.verification_key.as_ref().to_vec(), sig.as_ref().to_vec())
    }

    fn prea_input(expected_payload: &[u8], msg: &[u8], pubkey: &[u8], sig: &[u8]) -> Vec<u8> {
        let mut v = vec![F003_VERSION_PREA_ROOT];
        v.extend_from_slice(expected_payload);
        v.extend_from_slice(msg);
        v.extend_from_slice(pubkey);
        v.extend_from_slice(sig);
        v
    }

    fn fsl_input(pubkey: &[u8], msg: &[u8], sig: &[u8]) -> Vec<u8> {
        let mut v = vec![F003_VERSION_FSL_GENERIC];
        v.extend_from_slice(pubkey);
        v.extend_from_slice(msg);
        v.extend_from_slice(sig);
        v
    }

    #[test]
    fn frozen_layout_lengths_and_caps() {
        assert_eq!(F003_INPUT_LEN_FSL, 1 + 2592 + 64 + 4627);
        assert_eq!(F003_INPUT_LEN_PREA, 1 + 64 + 64 + 2592 + 4627);
        // Gas-implied per-block ceiling must not exceed the documented cap.
        assert!(kaspa_consensus_core::evm::EVM_GAS_LIMIT / F003_VERIFY_GAS <= MAX_MLDSA_VERIFY_PER_EVM_BLOCK as u64);
        assert!(MAX_MLDSA_VERIFY_PER_TX <= MAX_MLDSA_VERIFY_PER_EVM_BLOCK);
        assert!((MAX_MLDSA_VERIFY_PER_EVM_BLOCK * F003_INPUT_LEN_PREA) < MAX_MLDSA_AUTH_BYTES_PER_EVM_BLOCK);
    }

    #[test]
    fn version_0x02_prea_roundtrip_and_tamper() {
        let msg = [0x11u8; 64];
        let (pubkey, sig) = keyed(0x33, &msg, PREA_CTX);
        let payload = blake2b_512_address_payload(&pubkey).as_bytes().to_vec();

        // valid → true
        assert!(run_f003_verify(&prea_input(&payload, &msg, &pubkey, &sig)));

        // flipped signature → false
        let mut bad_sig = sig.clone();
        bad_sig[0] ^= 0x01;
        assert!(!run_f003_verify(&prea_input(&payload, &msg, &pubkey, &bad_sig)));

        // flipped message → false
        let mut bad_msg = msg;
        bad_msg[0] ^= 0x01;
        assert!(!run_f003_verify(&prea_input(&payload, &bad_msg, &pubkey, &sig)));

        // wrong expected key payload (does not match pubkey) → false (binding rejects before verify)
        let mut bad_payload = payload.clone();
        bad_payload[0] ^= 0x01;
        assert!(!run_f003_verify(&prea_input(&bad_payload, &msg, &pubkey, &sig)));

        // a signature made under the FSL context must NOT verify as a PREA root op
        // (context domain separation — the core anti-cross-protocol-replay property).
        let (pk2, fsl_sig) = keyed(0x33, &msg, FSL_CTX);
        let payload2 = blake2b_512_address_payload(&pk2).as_bytes().to_vec();
        assert!(!run_f003_verify(&prea_input(&payload2, &msg, &pk2, &fsl_sig)));
    }

    #[test]
    fn version_0x01_fsl_roundtrip_and_context_separation() {
        let msg = [0x77u8; 64];
        let (pubkey, sig) = keyed(0x44, &msg, FSL_CTX);
        assert!(run_f003_verify(&fsl_input(&pubkey, &msg, &sig)));

        // a PREA-context signature must not verify as an FSL generic op.
        let (pk2, prea_sig) = keyed(0x44, &msg, PREA_CTX);
        assert!(!run_f003_verify(&fsl_input(&pk2, &msg, &prea_sig)));
    }

    #[test]
    fn malformed_and_unknown_version_return_false_never_panic() {
        assert!(!run_f003_verify(&[])); // empty
        assert!(!run_f003_verify(&[0x02])); // version only
        assert!(!run_f003_verify(&[0x00; 100])); // unknown version (0x00) + wrong length
        assert!(!run_f003_verify(&[0xFF; F003_INPUT_LEN_PREA])); // unknown version 0xFF, right PREA length
        // right version byte but one byte short / long
        let mut short = vec![F003_VERSION_PREA_ROOT];
        short.extend_from_slice(&[0u8; F003_INPUT_LEN_PREA - 2]);
        assert!(!run_f003_verify(&short));
        let mut long = vec![F003_VERSION_FSL_GENERIC];
        long.extend_from_slice(&[0u8; F003_INPUT_LEN_FSL]); // one too many
        assert!(!run_f003_verify(&long));
        // all-zero bodies of the right length (garbage key/sig) → false, no panic
        let mut zero02 = vec![F003_VERSION_PREA_ROOT];
        zero02.extend_from_slice(&[0u8; F003_INPUT_LEN_PREA - 1]);
        assert!(!run_f003_verify(&zero02));
    }
}
