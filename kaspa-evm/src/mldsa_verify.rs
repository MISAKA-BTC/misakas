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
//! - `0x02` (PREA root, design v1.1 §9.3 option B): `version(1) ‖
//!   expected_key_payload64(64) ‖ pubkey(2592) ‖ signature(4627) ‖
//!   op_preimage(1..=F003_MAX_PREA_PREIMAGE_BYTES)`. FIRST binds the pubkey to its
//!   UTXO address payload (`blake2b_512(MLDSA87_ADDRESS_CONTEXT, pubkey) ==
//!   expected_key_payload64`), THEN computes `message_hash64 =
//!   keyed_blake2b_512(F003_PREA_OP_MLDSA87_CONTEXT, op_preimage)` and verifies it
//!   under [`F003_PREA_ROOT_MLDSA87_CONTEXT`]. F003 hashing the preimage itself is
//!   what lets a Solidity `executeRoot` bind the signature to the exact operation
//!   bytes WITHOUT needing keyed-BLAKE2b-512 in the EVM (it just passes the op
//!   bytes it is about to execute).
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
    F003_FSL_VERIFY_MLDSA87_CONTEXT, F003_INPUT_LEN_FSL, F003_MAX_PREA_PREIMAGE_BYTES, F003_PREA_OP_MLDSA87_CONTEXT,
    F003_PREA_PREFIX_LEN, F003_PREA_ROOT_MLDSA87_CONTEXT, F003_VERIFY_GAS, F003_VERSION_FSL_GENERIC, F003_VERSION_PREA_ROOT,
    MISAKA_MLDSA_VERIFY_PRECOMPILE,
};
use kaspa_hashes::{blake2b_512_address_payload, blake2b_512_keyed};
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
            // Layout (design v1.1 §9.3 option B): fixed prefix then a variable
            // op_preimage of 1..=F003_MAX_PREA_PREIMAGE_BYTES. F003 hashes the
            // preimage itself so the on-chain caller needs no BLAKE2b in the EVM.
            if input.len() < F003_PREA_PREFIX_LEN + 1 || input.len() > F003_PREA_PREFIX_LEN + F003_MAX_PREA_PREIMAGE_BYTES {
                return false;
            }
            let expected = &input[1..1 + KEY_PAYLOAD64_LEN];
            let pubkey = &input[1 + KEY_PAYLOAD64_LEN..1 + KEY_PAYLOAD64_LEN + MLDSA87_PK_LEN];
            let sig = &input[1 + KEY_PAYLOAD64_LEN + MLDSA87_PK_LEN..F003_PREA_PREFIX_LEN];
            let op_preimage = &input[F003_PREA_PREFIX_LEN..];
            // Bind the pubkey to its UTXO address payload BEFORE verifying — this is
            // what makes the F003-0x02 result attest "this key owns that PQ identity".
            if blake2b_512_address_payload(pubkey).as_bytes() != expected {
                return false;
            }
            // The signed message is the full-PQ keyed-BLAKE2b-512 digest of the exact
            // operation bytes the caller is executing — binding the signature to the op.
            let digest = blake2b_512_keyed(F003_PREA_OP_MLDSA87_CONTEXT, op_preimage);
            verify_mldsa87_with_context(pubkey, digest.as_byte_slice(), sig, F003_PREA_ROOT_MLDSA87_CONTEXT).unwrap_or(false)
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

    /// Build a v0x02 input: prefix (`payload ‖ pubkey ‖ sig`) then the op preimage.
    fn prea_input(expected_payload: &[u8], pubkey: &[u8], sig: &[u8], preimage: &[u8]) -> Vec<u8> {
        let mut v = vec![F003_VERSION_PREA_ROOT];
        v.extend_from_slice(expected_payload);
        v.extend_from_slice(pubkey);
        v.extend_from_slice(sig);
        v.extend_from_slice(preimage);
        v
    }

    /// Sign a v0x02 op the way a caller must (design v1.1 §9.3 option B): the ML-DSA
    /// message is `keyed_blake2b_512(OP_CONTEXT, preimage)`, signed under ROOT_CONTEXT.
    fn prea_sign(seed: u8, preimage: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let kp = mldsa::generate_key_pair([seed; 32]);
        let digest = blake2b_512_keyed(F003_PREA_OP_MLDSA87_CONTEXT, preimage);
        let sig =
            mldsa::sign(&kp.signing_key, digest.as_byte_slice(), F003_PREA_ROOT_MLDSA87_CONTEXT, [seed ^ 0xA5; 32]).expect("sign");
        (kp.verification_key.as_ref().to_vec(), sig.as_ref().to_vec())
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
        assert_eq!(F003_PREA_PREFIX_LEN, 1 + 64 + 2592 + 4627);
        assert!(F003_MAX_PREA_PREIMAGE_BYTES > 0);
        // Gas-implied per-block ceiling must not exceed the documented cap.
        assert!(kaspa_consensus_core::evm::EVM_GAS_LIMIT / F003_VERIFY_GAS <= MAX_MLDSA_VERIFY_PER_EVM_BLOCK as u64);
        assert!(MAX_MLDSA_VERIFY_PER_TX <= MAX_MLDSA_VERIFY_PER_EVM_BLOCK);
        let _ = MAX_MLDSA_AUTH_BYTES_PER_EVM_BLOCK;
    }

    #[test]
    fn version_0x02_prea_roundtrip_and_tamper() {
        let preimage = b"op|chain=MSK|to=0xabcd|value=1|epoch=0|nonce=7".to_vec();
        let (pubkey, sig) = prea_sign(0x33, &preimage);
        let payload = blake2b_512_address_payload(&pubkey).as_bytes().to_vec();

        // valid → true
        assert!(run_f003_verify(&prea_input(&payload, &pubkey, &sig, &preimage)));

        // flipped signature → false
        let mut bad_sig = sig.clone();
        bad_sig[0] ^= 0x01;
        assert!(!run_f003_verify(&prea_input(&payload, &pubkey, &bad_sig, &preimage)));

        // TAMPERED op preimage (a different operation) → false: the signature is bound
        // to the exact bytes via the keyed-BLAKE2b digest.
        let mut bad_pre = preimage.clone();
        bad_pre[0] ^= 0x01;
        assert!(!run_f003_verify(&prea_input(&payload, &pubkey, &sig, &bad_pre)));
        // a longer preimage (same prefix) is a different digest → false.
        let mut longer = preimage.clone();
        longer.push(0x00);
        assert!(!run_f003_verify(&prea_input(&payload, &pubkey, &sig, &longer)));

        // wrong expected key payload → false (binding rejects before verify).
        let mut bad_payload = payload.clone();
        bad_payload[0] ^= 0x01;
        assert!(!run_f003_verify(&prea_input(&bad_payload, &pubkey, &sig, &preimage)));

        // empty preimage (below min) and over-max preimage → false, never panic.
        assert!(!run_f003_verify(&prea_input(&payload, &pubkey, &sig, &[])));
        assert!(!run_f003_verify(&prea_input(&payload, &pubkey, &sig, &vec![0u8; F003_MAX_PREA_PREIMAGE_BYTES + 1])));

        // a sig over the digest computed with the WRONG domain (ROOT instead of OP
        // context) must NOT verify — op-digest domain separation.
        let kp = mldsa::generate_key_pair([0x33; 32]);
        let wrong_digest = blake2b_512_keyed(F003_PREA_ROOT_MLDSA87_CONTEXT, &preimage);
        let wrong_sig = mldsa::sign(&kp.signing_key, wrong_digest.as_byte_slice(), F003_PREA_ROOT_MLDSA87_CONTEXT, [0x01; 32])
            .expect("sign")
            .as_ref()
            .to_vec();
        assert!(!run_f003_verify(&prea_input(&payload, &pubkey, &wrong_sig, &preimage)));
    }

    /// PREA P0-2 e2e: replicate `MisakaPqSmartAccount.executeRoot`'s EXACT on-chain
    /// construction — the `_opPreimage` packing (OP_DOMAIN ‖ chainId ‖ account ‖
    /// version ‖ nonce ‖ window ‖ target ‖ value ‖ callData, fixed widths) and the
    /// F003 v0x02 input — then verify it through the REAL F003 logic with a REAL
    /// ML-DSA-87 root signature. This proves the contract's F003 integration works
    /// end-to-end with a real lattice signature (the Foundry tests cover the contract
    /// LOGIC with F003 mocked; this covers the real F003 + real sig + the exact
    /// Solidity encoding). Regression guard: if `_opPreimage`'s layout changes, this
    /// must change in lock-step or on-chain auth breaks.
    #[test]
    fn contract_execute_root_f003_input_verifies_with_real_mldsa() {
        let kp = mldsa::generate_key_pair([0x77u8; 32]);
        let pubkey = kp.verification_key.as_ref().to_vec();
        let root_payload = blake2b_512_address_payload(&pubkey); // the account's stored 64-byte payload

        // op fields a real executeRoot(...) call would carry.
        let account = [0xACu8; 20];
        let version: u64 = 1;
        let nonce: u64 = 0;
        let valid_after: u64 = 0;
        let valid_until: u64 = u64::MAX;
        let max_relayer_fee = [0u8; 32]; // uint256 0 (no relayer reimbursement)
        let target = [0x7Au8; 20];
        let value = [0u8; 32]; // uint256 0
        let call_data = [0x12u8, 0x34];
        // uint256(block.chainid) = EVM_CHAIN_ID (0x4D534B), big-endian in 32 bytes.
        let mut chain_id32 = [0u8; 32];
        chain_id32[24..32].copy_from_slice(&(kaspa_consensus_core::evm::EVM_CHAIN_ID).to_be_bytes());

        // _opPreimage = abi.encodePacked(OP_DOMAIN, chainId, account, version, nonce,
        //                                validAfter, validUntil, maxRelayerFee, target, value, callData)
        let mut preimage = Vec::new();
        preimage.extend_from_slice(b"MISAKA_PQ_EXECUTE_ROOT_V1");
        preimage.extend_from_slice(&chain_id32);
        preimage.extend_from_slice(&account);
        preimage.extend_from_slice(&version.to_be_bytes());
        preimage.extend_from_slice(&nonce.to_be_bytes());
        preimage.extend_from_slice(&valid_after.to_be_bytes());
        preimage.extend_from_slice(&valid_until.to_be_bytes());
        preimage.extend_from_slice(&max_relayer_fee);
        preimage.extend_from_slice(&target);
        preimage.extend_from_slice(&value);
        preimage.extend_from_slice(&call_data);

        // The signer commits to keyed_blake2b_512(OP_CONTEXT, preimage) under ROOT_CONTEXT.
        let digest = blake2b_512_keyed(F003_PREA_OP_MLDSA87_CONTEXT, &preimage);
        let sig = mldsa::sign(&kp.signing_key, digest.as_byte_slice(), F003_PREA_ROOT_MLDSA87_CONTEXT, [0x01u8; 32]).expect("sign");

        // The exact F003 v0x02 input the contract builds: version ‖ payload ‖ pubkey ‖ sig ‖ preimage.
        let mut input = vec![F003_VERSION_PREA_ROOT];
        input.extend_from_slice(root_payload.as_byte_slice());
        input.extend_from_slice(&pubkey);
        input.extend_from_slice(sig.as_ref());
        input.extend_from_slice(&preimage);

        assert!(run_f003_verify(&input), "the contract's F003 v0x02 input with a real ML-DSA root signature verifies");

        // A DIFFERENT operation (one flipped byte in the op preimage, e.g. the target)
        // would produce a different on-chain preimage and must NOT verify.
        let mut tampered = input.clone();
        let off = F003_PREA_PREFIX_LEN + 25 /*OP_DOMAIN*/ + 32 /*chainId*/; // first byte of `account`
        tampered[off] ^= 0x01;
        assert!(!run_f003_verify(&tampered), "a different operation (tampered preimage) does not verify");
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
        assert!(!run_f003_verify(&[0x02])); // version only (below the prefix)
        assert!(!run_f003_verify(&[0x00; 100])); // unknown version (0x00)
        assert!(!run_f003_verify(&[0xFF; 7285])); // unknown version 0xFF, plausible length
        // v0x02 with the prefix present but ZERO preimage bytes (below the min).
        assert!(!run_f003_verify(&[F003_VERSION_PREA_ROOT; F003_PREA_PREFIX_LEN]));
        // v0x01 one byte too long.
        let mut long = vec![F003_VERSION_FSL_GENERIC];
        long.extend_from_slice(&[0u8; F003_INPUT_LEN_FSL]);
        assert!(!run_f003_verify(&long));
        // v0x02 right-length prefix + 1 preimage byte but garbage key/sig → false, no panic.
        let mut zero02 = vec![F003_VERSION_PREA_ROOT];
        zero02.extend_from_slice(&[0u8; F003_PREA_PREFIX_LEN]); // prefix(-version) + 1 preimage byte
        assert!(!run_f003_verify(&zero02));
    }
}
