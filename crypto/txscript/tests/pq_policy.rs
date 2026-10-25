//! kaspa-pq PQ-only script policy tests (ADR-0019 / docs/kaspa-pq-design-mldsa87.md §6).
//!
//! These lock the consensus-critical decision of *exactly which* signature
//! opcodes are disabled under PQ-only enforcement: the six legacy secp256k1
//! opcodes are rejected, while the two ML-DSA-87 signature opcodes and the
//! non-signature opcodes that make up the ML-DSA P2PKH template survive.
//!
//! Engine-execution and P2SH-rejection behaviour is exercised end-to-end in
//! the consensus transaction-validator tests (where real tx/utxo fixtures and
//! the `ScriptPolicy::PQ_ONLY` threading exist); here we pin the tag set and
//! the policy constants, which are pure and fixture-free.

use kaspa_txscript::{ScriptPolicy, is_legacy_signature_opcode, opcodes::codes};

#[test]
fn pq_only_disables_exactly_the_legacy_secp256k1_signature_opcodes() {
    // Legacy secp256k1 signature opcodes — MUST be rejected in PQ-only mode.
    let legacy = [
        ("OpCheckSig", codes::OpCheckSig),                       // 0xac
        ("OpCheckSigVerify", codes::OpCheckSigVerify),           // 0xad
        ("OpCheckSigECDSA", codes::OpCheckSigECDSA),             // 0xab
        ("OpCheckMultiSig", codes::OpCheckMultiSig),             // 0xae
        ("OpCheckMultiSigVerify", codes::OpCheckMultiSigVerify), // 0xaf
        ("OpCheckMultiSigECDSA", codes::OpCheckMultiSigECDSA),   // 0xa9
    ];
    for (name, tag) in legacy {
        assert!(is_legacy_signature_opcode(tag), "{name} ({tag:#04x}) must be a disabled legacy signature opcode");
    }
}

#[test]
fn pq_only_keeps_the_mldsa_signature_opcodes() {
    // The ML-DSA-87 signature opcodes are the WHOLE point — they must survive.
    assert!(!is_legacy_signature_opcode(codes::OpCheckSigMlDsa87), "OpCheckSigMlDsa87 (0xa6) must NOT be treated as legacy");
    assert!(!is_legacy_signature_opcode(codes::OpCheckMultiSigMlDsa87), "OpCheckMultiSigMlDsa87 (0xa7) must NOT be treated as legacy");
}

#[test]
fn pq_only_keeps_the_p2pkh_template_opcodes() {
    // The non-signature opcodes that compose the ML-DSA P2PKH scriptPubKey
    // (OP_DUP OP_BLAKE2B OP_DATA32 <hash> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87)
    // must never be classified as legacy signature opcodes.
    for (name, tag) in [
        ("OpDup", codes::OpDup),
        ("OpBlake2b", codes::OpBlake2b),
        ("OpData32", codes::OpData32),
        ("OpEqualVerify", codes::OpEqualVerify),
    ] {
        assert!(!is_legacy_signature_opcode(tag), "{name} ({tag:#04x}) is not a signature opcode and must survive");
    }
}

#[test]
fn script_policy_constants_are_correct() {
    // PQ-only: legacy opcodes disabled, P2SH disabled.
    assert!(ScriptPolicy::PQ_ONLY.pq_only);
    assert!(!ScriptPolicy::PQ_ONLY.allow_p2sh);

    // Legacy (upstream-compatible): no restriction.
    assert!(!ScriptPolicy::LEGACY.pq_only);
    assert!(ScriptPolicy::LEGACY.allow_p2sh);

    // kaspa-pq PQ-only: the type's default policy is PQ_ONLY (secure) so any code
    // that asks for `ScriptPolicy::default()` gets PQ enforcement rather than the
    // permissive legacy engine. The engine *constructors* still pin LEGACY explicitly
    // for the upstream/back-compat opcode tests, and the production consensus path
    // sets PQ_ONLY via `with_script_policy`.
    assert_eq!(ScriptPolicy::default(), ScriptPolicy::PQ_ONLY);
}
