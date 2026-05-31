use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum TxScriptError {
    #[error("invalid opcode length: {0:02x?}")]
    MalformedPushSize(Vec<u8>),
    #[error("opcode requires {0} bytes, but script only has {1} remaining")]
    MalformedPush(usize, usize),
    #[error("transaction input {0} is out of bounds, should be non-negative below {1}")]
    InvalidInputIndex(i32, usize),
    #[error("combined stack size {0} > max allowed {1}")]
    StackSizeExceeded(usize, usize),
    #[error("attempt to execute invalid opcode {0}")]
    InvalidOpcode(String),
    #[error("attempt to execute reserved opcode {0}")]
    OpcodeReserved(String),
    #[error("attempt to execute disabled opcode {0}")]
    OpcodeDisabled(String),
    #[error("attempt to read from empty stack")]
    EmptyStack,
    #[error("stack contains {0} unexpected items")]
    CleanStack(usize),
    // We return error if stack entry is false
    #[error("false stack entry at end of script execution")]
    EvalFalse,
    #[error("script returned early")]
    EarlyReturn,
    #[error("script ran, but verification failed")]
    VerifyError,
    #[error("encountered invalid state while running script: {0}")]
    InvalidState(String),
    // kaspa-pq PQ-only (ADR-0019 §14): the legacy secp256k1 signature error is
    // compiled only under `legacy-secp256k1`. ML-DSA-87 verification reports
    // failures via `SigLength`/`PubKeyFormat`/a `false` verify result instead.
    #[cfg(feature = "legacy-secp256k1")]
    #[error("signature invalid: {0}")]
    InvalidSignature(secp256k1::Error),
    #[error("invalid signature in sig cache")]
    SigcacheSignatureInvalid,
    #[error("exceeded max operation limit of {0}")]
    TooManyOperations(i32),
    #[error("Engine is not running on a transaction input")]
    NotATransactionInput,
    #[error("element size {0} exceeds max allowed size {1}")]
    ElementTooBig(usize, usize),
    #[error("push encoding is not minimal: {0}")]
    NotMinimalData(String),
    #[error("opcode not supported on current source: {0}")]
    InvalidSource(String),
    #[error("Unsatisfied lock time: {0}")]
    UnsatisfiedLockTime(String),
    #[error("Number too big: {0}")]
    NumberTooBig(String),
    #[error("not all signatures empty on failed checkmultisig")]
    NullFail,
    #[error("invalid signature count: {0}")]
    InvalidSignatureCount(String),
    #[error("invalid pubkey count: {0}")]
    InvalidPubKeyCount(String),
    #[error("invalid hash type {0:#04x}")]
    InvalidSigHashType(u8),
    #[error("unsupported public key type")]
    PubKeyFormat,
    #[error("invalid signature length {0}")]
    SigLength(usize),
    #[error("no scripts to run")]
    NoScripts,
    #[error("signature script is not push only")]
    SignatureScriptNotPushOnly,
    #[error("end of script reached in conditional execution")]
    ErrUnbalancedConditional,
    #[error("opcode requires at least {0} but stack has only {1}")]
    InvalidStackOperation(usize, usize),
    #[error("script of size {0} exceeded maximum allowed size of {1}")]
    ScriptSize(usize, usize),
    #[error("transaction output {0} is out of bounds, should be non-negative below {1}")]
    InvalidOutputIndex(i32, usize),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    #[error("sig op count exceeds passed limit of {0}")]
    ExceededSigOpLimit(u8),
    // kaspa-pq PQ-only (ADR-0019 / docs/kaspa-pq-design-mldsa87.md §6): legacy
    // secp256k1 signature opcodes are consensus-disabled; only ML-DSA-87
    // signature opcodes are permitted.
    #[error("legacy signature opcode {0:#04x} is disabled in PQ-only mode")]
    LegacySignatureOpcodeDisabled(u8),
    // kaspa-pq PQ-only (§6.5): pay-to-script-hash is out of launch scope.
    #[error("pay-to-script-hash is disabled in PQ-only mode")]
    ScriptHashDisabledInPqMode,
    // kaspa-pq PQ-only (ADR-0019 §13): the wallet/tx-generator refuses to build
    // an output paying to a legacy (secp256k1 / P2SH) address on a PQ network —
    // only the ML-DSA-87 P2PKH address class is permitted.
    #[error("legacy address (version {0:?}) is disabled on kaspa-pq; only ML-DSA P2PKH is permitted")]
    LegacyAddressDisabledInPqMode(String),
}

#[derive(Error, PartialEq, Eq, Debug, Clone, Copy)]
pub enum SerializationError {
    #[error("Number exceeds 8 bytes: {0}")]
    NumberTooLong(i64),
}
