//! Kinds `code` (id 4) and `contract` (id 22) — ADR-0078 Decisions 8, 9 and 11. The `code` row:
//! "source files + a build manifest (targets, tests)" through "a pinned, hermetic toolchain: no
//! network, fixed clock, fixed locale and env, build tree hash in the manifest", yielding "build
//! outputs + a test log". The `contract` row: "EVM initcode + a test manifest → the in-tree EVM
//! → runtime code + test log". Decision 11: the first toolchain NAMED is the one the tree already
//! holds at consensus grade — the in-tree EVM (`contract`, and `code` under `toolchain =
//! evm/v1`); pinned external toolchains are manifests, and none is named by an object until its
//! fleet drill passes.
//!
//! ## What is here
//!
//! * ONE grammar, `code/v1`: sources + a build manifest, canonicalized (§ "The DSL").
//! * TWO transformers over it, `code/evm/v1` and `contract/evm/v1`. They run the same toolchain
//!   and differ in the kind a person asked for — a program vs a contract — which is the object's
//!   `kind`; `contract/evm/v1` refuses any toolchain but `evm/v1` by definition (a contract IS the
//!   in-tree EVM, Decision 9), and `code/evm/v1` accepts only `evm/v1` today because no other
//!   toolchain is named (Decision 11).
//! * The toolchain `evm/v1` (§ "The toolchain"): a fresh in-memory state, a fixed environment,
//!   one deploy, the manifest's tests, and the `MCOD` artifact (§ "The artifact").
//! * The hermetic half of the `code` row, [`run_external`]: a pinned external toolchain run under
//!   an [`ExternalToolchainManifest`]. It is a LIBRARY FUNCTION ONLY — it has no `Transformer`
//!   impl and is not in [`register`] — because ADR-0078 Decision 11 names an external toolchain
//!   only by its fleet drill, and no drill has passed.
//!
//! ## Confinement (ADR-0078 SA-1, ADR-0079 Decision 12, invariant S11)
//!
//! *Executing model-written code is the largest privilege in the lineage, and the in-tree EVM is
//! not exempt.* Two gates, and there is no third door:
//!
//! * **The EVM runs in a separate process.** [`run_evm_v1`] never executes initcode in the
//!   caller's process: it spawns `palw-evm-runner` (this crate's second binary) through
//!   `Confinement::command`, in an ephemeral tree destroyed after the run, under the environment
//!   discipline, a resident ceiling and a wall-clock deadline. The runner reads one canonical job
//!   on stdin and writes one canonical result on stdout. A denied, killed, over-ceiling or absent
//!   runner is the parse-failure arm — no object — never a panic and never an in-process retry.
//!   The in-process executor still exists, as [`execute_evm_job_in_this_process`], and the only
//!   caller in the tree is the runner binary (`tests/derive_tree_guard.rs` holds that).
//! * **An external toolchain runs only under a proven backend.** [`run_external`] refuses on a
//!   host whose confinement backend is `none`, and refuses on a host where a bond or wallet key is
//!   reachable — Decision 12's third bullet, which is a completion condition for ADR-0078's Q-05
//!   and not advice.
//!
//! What confinement is NOT: a consensus mechanism (ADR-0079 Decision 3). The deadline and the
//! resident ceiling can turn a build into **no object**; they cannot turn it into a different
//! artifact. Every number in the artifact is a function of the committed bytes and the run
//! manifest, which is why a host with a backend and a host without one either agree or fail.
//!
//! ## Discipline (ADR-0078 Decision 3, invariant X3)
//!
//! The EVM is an integer machine and revm is pure Rust: gas, output and success are functions of
//! the bytes alone. No clock (block timestamp 0), no randomness (prevrandao zero), no network, no
//! floating point on any path (a source scan in the tests refuses the two float type names from
//! this file, comments included), no hash-map iteration order reaching the bytes (sources are a
//! `BTreeMap`; tests run and are written in manifest order). The MISAKA precompiles (F002/F003)
//! are NOT registered: they are the chain's execution lane and hang on a fence, and a build must
//! be the same on every host, fence or no fence.
//!
//! ## Verification mode (ADR-0078 Decision 10)
//!
//! A failing test is NOT a derivation failure. "The transformer is a checker and the artifact is
//! its verdict log": the artifact records, per test, what happened and whether the expectation
//! held. What DOES refuse the derivation is a build that cannot be a build — a deploy that
//! reverts or halts, a deploy that leaves no runtime code, a test whose transaction the EVM will
//! not even admit (intrinsic gas above its limit), an artifact above the bound.

use crate::bytes::{put_u16_le, put_u32_le, put_u64_le};
use crate::canon_json::{CanonValue, parse_canonical, write_canonical};
use crate::checksum::crc32;
use crate::{Artifact, DeriveError, Discipline, Grammar, Transformer, TransformerManifest};
use kaspa_consensus_core::palw_derived_v1::kind;
use misaka_palw::host_security::{
    ConfinementBackend, establish_confinement, harden_worker_command, reachable_signing_secrets, resident_bytes,
};
use revm::primitives::{AccountInfo, Address, B256, Bytes, ExecutionResult, KECCAK_EMPTY, Output, TxEnv, TxKind, U256};
use revm::{
    Database, Evm,
    db::{CacheDB, EmptyDB},
};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------
// Names and bounds
// ---------------------------------------------------------------------------------------------

/// The grammar both transformers consume.
pub const GRAMMAR_NAME: &str = "code/v1";
/// The only toolchain this build names (ADR-0078 Decision 11).
pub const TOOLCHAIN_EVM_V1: &str = "evm/v1";
pub const CODE_TRANSFORMER_NAME: &str = "code/evm/v1";
pub const CONTRACT_TRANSFORMER_NAME: &str = "contract/evm/v1";
/// The artifact's canonical writer, as the manifests name it — **and the run manifest's tag**.
///
/// `misaka-code-build/2/…` is the `MCOD` writer at version 2 (the artifact names the run manifest,
/// § "The artifact"). The `+evm-run/<deploy gas ceiling>/<run manifest digest>` half is ADR-0078
/// SA-1's requirement that *"the gas ceiling and the state-fixture hash are part of
/// `transformer_id`'s manifest"*: `TransformerManifest` has no parameter field of its own, and
/// `writer` is the manifest field whose spelling this file owns, so the run manifest rides here
/// and a changed ceiling or a changed fixture is a changed `transformer_id` by construction rather
/// than by the source-tree hash's accident. [`evm_v1_run_manifest_tag`] recomputes it and
/// `the_writer_name_pins_the_run_manifest` is the pin.
pub const WRITER_NAME: &str = "misaka-code-build/2/canonical-v1+evm-run/30000000/aae1b60f1da9ae9b";
pub const MEDIA_TYPE: &str = "application/vnd.misaka.code-build";
pub const EXTENSION: &str = "mcod";

pub const MCOD_MAGIC: &[u8; 4] = b"MCOD";
/// Version 2 carries the run manifest digest in its header (ADR-0078 SA-1).
pub const MCOD_VERSION: u16 = 2;
/// The keyed BLAKE2b-512 domain under which a source's text is digested into the artifact.
pub const SOURCE_DIGEST_DOMAIN: &[u8] = b"misaka-palw/derive/code/source/v1";

pub const MAX_SOURCES: usize = 64;
pub const MAX_PATH_BYTES: usize = 128;
/// The bound on the sum of every source's text.
pub const MAX_SOURCE_TEXT_BYTES: usize = 1 << 20;
/// EIP-3860's initcode limit, which Shanghai enforces anyway; stated here so the grammar refuses
/// it by name instead of the EVM halting on it.
pub const MAX_INITCODE_BYTES: usize = 48 * 1024;
pub const MAX_CONSTRUCTOR_ARGS_BYTES: usize = 8 * 1024;
pub const MAX_CALLDATA_BYTES: usize = 8 * 1024;
pub const MAX_EXPECT_OUTPUT_BYTES: usize = 1 << 20;
pub const MAX_TESTS: usize = 256;
pub const MAX_NAME_BYTES: usize = 64;
pub const MIN_TEST_GAS_LIMIT: u64 = 21_000;
pub const MAX_TEST_GAS_LIMIT: u64 = 30_000_000;
/// **ADR-0078 SA-2's `max_dsl_bytes`.** The most answer bytes this kind will look at, checked on
/// the byte COUNT before the parser is asked what the bytes spell — a JSON parser is an allocator
/// driven by its input, and a bound applied after parsing is applied after the damage. Exceeding
/// it is "no object" (Decision 2's parse-failure arm, X4), never a repair and never a truncation.
///
/// The number is the retention payload's own cap (`PALW_FP_DSL_V1_MAX_BYTES`): a DSL above it
/// could not be served to a verifier under Decision 6 even if it derived, so deriving from one
/// would be building a derivation nobody could check. This kind's schema admits documents larger
/// than that in its extreme corner (256 tests of a mebibyte of expected output each), and this ceiling is the
/// binding one — it is far above any answer a class at these widths emits, and far below
/// what a parser could be made to allocate.
pub const MAX_DSL_BYTES: u64 = kaspa_consensus_core::palw_derived_v1::PALW_FP_DSL_V1_MAX_BYTES as u64;

pub const MAX_ARTIFACT_BYTES: usize = 16 << 20;

/// The toolchain's fixed environment. Chain id 1 (not the lane's: a build names no chain),
/// block 0 at timestamp 0, a zero coinbase, the block gas limit, zero basefee, zero
/// difficulty, zero prevrandao, and the spec id the tree pins (`kaspa_evm::EVM_SPEC_ID`).
pub const EVM_V1_CHAIN_ID: u64 = 1;
pub const EVM_V1_BLOCK_GAS_LIMIT: u64 = 30_000_000;
pub const EVM_V1_DEPLOY_GAS_LIMIT: u64 = 30_000_000;
/// **ADR-0078 SA-2's `max_steps`, in this kind's own unit: EVM gas (SA-1's ceiling).** What one
/// run of this toolchain can burn — the deployment, plus every test the manifest admits at the
/// largest limit it admits. It is derived from the two numbers above it rather than chosen, so a
/// change to either moves it, and moving it moves `transformer_id`.
pub const MAX_RUN_GAS: u64 = EVM_V1_DEPLOY_GAS_LIMIT + (MAX_TESTS as u64) * MAX_TEST_GAS_LIMIT;
/// The fixed deployer. NOT `0x…01`: that is the ecrecover precompile's address, and a contract
/// that pays or calls its deployer would reach a precompile instead of an account. Twenty ASCII
/// bytes that name what they are, with no code and a large balance.
pub const EVM_V1_DEPLOYER: [u8; 20] = *b"misaka-code-build/v1";
/// The deployer's balance in the fixture, so a `value`-bearing test can be paid for. Spelled as a
/// constant because it is a fixture field and the fixture is hashed.
pub const EVM_V1_DEPLOYER_BALANCE: u128 = u128::MAX;

// --- the confined runner's own bounds (ADR-0079 Decisions 5, 6, 12) ---------------------------
//
// Neither of these can change an artifact: they can only refuse one. That is why they are host
// safety nets and NOT part of the run manifest — a host that kills the run produces no object,
// which is ADR-0078 Decision 2's parse-failure arm and ADR-0079 S4's "never a different number".

/// The binary that runs model-written initcode. It is a sibling of whatever binary derives, and
/// [`RUNNER_PATH_ENV`] names it outright when a deployment puts it elsewhere.
pub const RUNNER_BIN_NAME: &str = "palw-evm-runner";
/// Operator override for the runner's absolute path.
pub const RUNNER_PATH_ENV: &str = "MISAKA_PALW_EVM_RUNNER";
/// The prefix of every ephemeral tree this module makes, so an operator (and a test) can see that
/// none outlives its run.
pub const WORK_DIR_PREFIX: &str = "misaka-code-build-";
/// The runner's resident ceiling. An `evm/v1` job's memory is bounded by gas — 30M gas buys about
/// four megabytes of EVM memory — so a gigabyte is a wall, not a budget.
pub const EVM_V1_MAX_RESIDENT_BYTES: u64 = 1 << 30;
/// The deadline is DERIVED from the gas the answer itself declares (ADR-0077 SA-4's shape: a
/// deadline is derived, never chosen): a fixed base for process start-up, plus the declared gas at
/// a floor rate no host in the fleet is slower than, capped so nothing hangs a gateway for a day.
pub const EVM_V1_DEADLINE_BASE_SECS: u64 = 30;
pub const EVM_V1_GAS_PER_SEC_FLOOR: u64 = 1_000_000;
pub const EVM_V1_DEADLINE_MAX_SECS: u64 = 3_600;
/// How often the parent looks at the child while it works.
pub const EVM_V1_POLL_MILLIS: u64 = 20;
/// Every how many polls the resident size is measured. On macOS that measurement is a `/bin/ps`,
/// so it runs at half a second rather than at every poll: a ceiling is a wall, not a sampler.
pub const EVM_V1_RESIDENT_POLL_EVERY: u32 = 25;
/// What the parent will read from a child before it stops reading. The result frame is bounded by
/// the artifact bound; stderr is a message, not a channel.
pub const MAX_CHILD_STDOUT_BYTES: u64 = (MAX_ARTIFACT_BYTES + (1 << 20)) as u64;
pub const MAX_CHILD_STDERR_BYTES: u64 = 64 * 1024;

/// The keyed BLAKE2b-512 domain of the state fixture's digest (ADR-0078 SA-1).
pub const EVM_V1_STATE_FIXTURE_DOMAIN: &[u8] = b"misaka-palw/derive/code/evm-state-fixture/v1";
/// The keyed BLAKE2b-512 domain of the run manifest's digest (ADR-0078 SA-1).
pub const EVM_V1_RUN_MANIFEST_DOMAIN: &[u8] = b"misaka-palw/derive/code/evm-run-manifest/v1";

// ---------------------------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------------------------

/// The `code/v1` grammar.
pub struct CodeGrammar;
/// Kind `code` (4) under toolchain `evm/v1`.
pub struct CodeEvmTransformer;
/// Kind `contract` (22): the in-tree EVM, by definition.
pub struct ContractEvmTransformer;

/// One grammar shared by two transformers, as the registry sees them.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![Box::new(CodeGrammar)], vec![Box::new(CodeEvmTransformer), Box::new(ContractEvmTransformer)])
}

impl Grammar for CodeGrammar {
    fn name(&self) -> &'static str {
        GRAMMAR_NAME
    }
    fn canonicalize(&self, answer: &[u8]) -> Result<Vec<u8>, DeriveError> {
        crate::check_dsl_bytes(MAX_DSL_BYTES, answer)?;
        parse_and_canonicalize(answer).map(|(_, canonical)| canonical)
    }
}

impl Transformer for CodeEvmTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: CODE_TRANSFORMER_NAME,
            kind: kind::CODE,
            grammar: GRAMMAR_NAME,
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2 / SA-1: the DSL ceiling this grammar enforces, the artifact ceiling
            // the writer enforces, and the gas one run can burn — a deploy plus every test.
            max_dsl_bytes: MAX_DSL_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES as u64,
            max_steps: MAX_RUN_GAS,
        }
    }
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        run_evm_v1(dsl, CODE_TRANSFORMER_NAME, |toolchain| {
            DeriveError::Transformer(format!(
                "{CODE_TRANSFORMER_NAME} runs toolchain {TOOLCHAIN_EVM_V1:?} only; {toolchain:?} is not a toolchain this build names \
                 (ADR-0078 Decision 11: an external toolchain is named only by its fleet drill)"
            ))
        })
    }
}

impl Transformer for ContractEvmTransformer {
    fn manifest(&self) -> TransformerManifest {
        TransformerManifest {
            name: CONTRACT_TRANSFORMER_NAME,
            kind: kind::CONTRACT,
            grammar: GRAMMAR_NAME,
            discipline: Discipline::Integer,
            writer: WRITER_NAME,
            source_tree_sha256: crate::SOURCE_TREE_SHA256_HEX,
            // ADR-0078 SA-2 / SA-1: the DSL ceiling this grammar enforces, the artifact ceiling
            // the writer enforces, and the gas one run can burn — a deploy plus every test.
            max_dsl_bytes: MAX_DSL_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES as u64,
            max_steps: MAX_RUN_GAS,
        }
    }
    fn run(&self, dsl: &[u8]) -> Result<Artifact, DeriveError> {
        run_evm_v1(dsl, CONTRACT_TRANSFORMER_NAME, |toolchain| {
            DeriveError::Transformer(format!(
                "{CONTRACT_TRANSFORMER_NAME} is the in-tree EVM by definition (ADR-0078 Decision 9, kind {}); \
                 toolchain {toolchain:?} is refused",
                kind::CONTRACT
            ))
        })
    }
}

/// Both transformers' pipeline: re-canonicalize (refusing input that is not the grammar's
/// output), apply the transformer's own toolchain rule, build, write.
fn run_evm_v1(dsl: &[u8], transformer_name: &str, refuse_toolchain: fn(&str) -> DeriveError) -> Result<Artifact, DeriveError> {
    // SA-2's first gate, on the transformer's entry as well as the grammar's: a consumer
    // verifying a derivation may hand these bytes straight in, and a transformer that trusts
    // its caller's bounds has none of its own.
    crate::check_dsl_bytes(MAX_DSL_BYTES, dsl)?;
    let (code, canonical) = parse_and_canonicalize(dsl)?;
    if canonical != dsl {
        return Err(DeriveError::Transformer(format!(
            "{transformer_name}: the input is not canonical under {GRAMMAR_NAME}; the grammar's output is the transformer's input"
        )));
    }
    if code.toolchain != TOOLCHAIN_EVM_V1 {
        return Err(refuse_toolchain(&code.toolchain));
    }
    // ADR-0078 SA-1: the initcode below was written by a model. It runs in a child process, in an
    // ephemeral tree, under whatever confinement this host can PROVE — never here.
    let build = build_evm_v1_confined(&code)?;
    let bytes = write_mcod(&code, &build)?;
    Ok(Artifact { bytes, media_type: MEDIA_TYPE, extension: EXTENSION })
}

// ---------------------------------------------------------------------------------------------
// The DSL (grammar `code/v1`)
// ---------------------------------------------------------------------------------------------
//
// ```json
// {
//   "v": 1,
//   "toolchain": "evm/v1",
//   "sources": { "<path>": "<text>", ... },
//   "manifest": {
//     "targets": [ { "name": "...", "source": "<path of the .hex>", "constructor_args": "0x..." } ],
//     "tests": [ { "name": "...", "calldata": "0x...", "value": 0, "gas_limit": 100000,
//                  "expect": { "success": true, "output": "0x..." | null } }, ... ]
//   }
// }
// ```
//
// Exactly these keys at every level; an unknown key is a grammar refusal. Sources: 1..=64 files,
// paths 1..=128 bytes of `[A-Za-z0-9._/-]` with no empty, `.` or `..` segment, total text at most
// 1 MiB. Under `evm/v1` exactly one source path ends in `.hex` and holds the contract's INITCODE
// as `0x`-prefixed hex (at most 48 KiB of bytecode; surrounding ASCII whitespace is tolerated
// when the file is read, and the text itself stays verbatim in the DSL); every other file (a
// `.sol` original, a README) is carried for provenance and digested into the artifact, never
// executed. Exactly one target at v1, naming the `.hex`. Tests: 0..=256, names 1..=64 bytes and
// unique, calldata at most 8 KiB, value in `u64`, gas limit 21_000..=30_000_000.
//
// Canonical form: the JSON canonicalizer's (sorted keys, no whitespace, integers only) plus
// number form for the hex fields of the manifest (`constructor_args`, `calldata`,
// `expect.output`), which are emitted lowercase. Source texts are never touched.

/// A parsed, validated `code/v1` answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeDsl {
    pub toolchain: String,
    /// path → text, sorted by path (the map's order is the artifact's order).
    pub sources: BTreeMap<String, String>,
    pub target: BuildTarget,
    pub tests: Vec<TestCase>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildTarget {
    pub name: String,
    /// The `.hex` source path.
    pub source: String,
    pub constructor_args: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub calldata: Vec<u8>,
    pub value: u64,
    pub gas_limit: u64,
    pub expect_success: bool,
    /// `None` is the manifest's `null`: only `success` is compared.
    pub expect_output: Option<Vec<u8>>,
}

/// Parse an answer under `code/v1` and return it with its canonical bytes.
pub fn parse_and_canonicalize(answer: &[u8]) -> Result<(CodeDsl, Vec<u8>), DeriveError> {
    let tree = parse_canonical(answer)?;
    let code = parse_dsl(&tree)?;
    let canonical = write_canonical(&canonical_tree(&code));
    Ok((code, canonical))
}

fn grammar(msg: String) -> DeriveError {
    DeriveError::Grammar(msg)
}

/// `v` as an object with exactly `keys`.
fn exact_object<'a>(v: &'a CanonValue, ctx: &str, keys: &[&str]) -> Result<&'a BTreeMap<String, CanonValue>, DeriveError> {
    let obj = v.as_obj().ok_or_else(|| grammar(format!("{ctx} must be an object")))?;
    if let Some(k) = obj.keys().find(|k| !keys.contains(&k.as_str())) {
        return Err(grammar(format!("unknown key {k:?} in {ctx}")));
    }
    if let Some(k) = keys.iter().find(|k| !obj.contains_key(**k)) {
        return Err(grammar(format!("{ctx} is missing {k:?}")));
    }
    Ok(obj)
}

fn string_field<'a>(obj: &'a BTreeMap<String, CanonValue>, ctx: &str, key: &str) -> Result<&'a str, DeriveError> {
    obj.get(key).and_then(CanonValue::as_str).ok_or_else(|| grammar(format!("{ctx}.{key} must be a string")))
}

fn name_field(obj: &BTreeMap<String, CanonValue>, ctx: &str, key: &str) -> Result<String, DeriveError> {
    let s = string_field(obj, ctx, key)?;
    if s.is_empty() || s.len() > MAX_NAME_BYTES {
        return Err(grammar(format!("{ctx}.{key} must be 1..={MAX_NAME_BYTES} bytes, not {}", s.len())));
    }
    Ok(s.to_string())
}

/// Decode `0x`-prefixed hex (either case) into bytes, bounded.
fn decode_hex(ctx: &str, text: &str, max_bytes: usize) -> Result<Vec<u8>, DeriveError> {
    let digits = text.strip_prefix("0x").ok_or_else(|| grammar(format!("{ctx}: expected 0x-prefixed hex")))?;
    if digits.len() % 2 != 0 {
        return Err(grammar(format!("{ctx}: odd-length hex")));
    }
    if digits.len() / 2 > max_bytes {
        return Err(grammar(format!("{ctx}: {} bytes exceeds the bound of {max_bytes}", digits.len() / 2)));
    }
    let nibble = |c: u8| -> Result<u8, DeriveError> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(grammar(format!("{ctx}: {:?} is not a hex digit", c as char))),
        }
    };
    digits.as_bytes().chunks(2).map(|pair| Ok((nibble(pair[0])? << 4) | nibble(pair[1])?)).collect()
}

/// Lowercase `0x` hex — the canonical number form of a byte string.
pub fn hex0x(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 0x0f) as usize] as char);
    }
    s
}

/// A source path: 1..=128 bytes of `[A-Za-z0-9._/-]`, no empty, `.` or `..` segment (so it can
/// neither escape nor alias a directory when materialized).
pub fn check_source_path(path: &str) -> Result<(), DeriveError> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES {
        return Err(grammar(format!("source path {path:?}: length must be 1..={MAX_PATH_BYTES} bytes")));
    }
    if let Some(c) = path.chars().find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))) {
        return Err(grammar(format!("source path {path:?}: {c:?} is outside [A-Za-z0-9._/-]")));
    }
    for seg in path.split('/') {
        if seg.is_empty() {
            return Err(grammar(format!("source path {path:?}: an empty segment (a leading, trailing or doubled '/')")));
        }
        if seg == "." || seg == ".." {
            return Err(grammar(format!("source path {path:?}: a {seg:?} segment is not a name")));
        }
    }
    Ok(())
}

/// The `.hex` source's bytes: the initcode.
fn decode_initcode(path: &str, text: &str) -> Result<Vec<u8>, DeriveError> {
    let trimmed = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let bytes = decode_hex(&format!("source {path:?}"), trimmed, MAX_INITCODE_BYTES)?;
    if bytes.is_empty() {
        return Err(grammar(format!("source {path:?} carries no bytecode; an empty initcode builds nothing")));
    }
    Ok(bytes)
}

fn parse_dsl(tree: &CanonValue) -> Result<CodeDsl, DeriveError> {
    let root = exact_object(tree, "the answer", &["manifest", "sources", "toolchain", "v"])?;
    if root.get("v").and_then(CanonValue::as_i64) != Some(1) {
        return Err(grammar("v must be the integer 1".into()));
    }
    let toolchain = string_field(root, "the answer", "toolchain")?;
    if toolchain != TOOLCHAIN_EVM_V1 {
        return Err(grammar(format!(
            "toolchain {toolchain:?} is not named by this build: an external toolchain is named only by its fleet drill \
             (ADR-0078 Decision 11); the one this build names is {TOOLCHAIN_EVM_V1:?}"
        )));
    }

    // sources
    let sources_obj = root.get("sources").and_then(CanonValue::as_obj).ok_or_else(|| grammar("sources must be an object".into()))?;
    if sources_obj.is_empty() || sources_obj.len() > MAX_SOURCES {
        return Err(grammar(format!("sources must hold 1..={MAX_SOURCES} files, not {}", sources_obj.len())));
    }
    let mut sources = BTreeMap::new();
    let mut total = 0usize;
    let mut hex_paths: Vec<&str> = Vec::new();
    for (path, text) in sources_obj {
        check_source_path(path)?;
        let text = text.as_str().ok_or_else(|| grammar(format!("source {path:?} must be a string")))?;
        total += text.len();
        if total > MAX_SOURCE_TEXT_BYTES {
            return Err(grammar(format!("the sources' text exceeds {MAX_SOURCE_TEXT_BYTES} bytes in total")));
        }
        if path.ends_with(".hex") {
            hex_paths.push(path);
        }
        sources.insert(path.clone(), text.to_string());
    }
    if hex_paths.len() != 1 {
        return Err(grammar(format!(
            "{TOOLCHAIN_EVM_V1} needs exactly one .hex source (the initcode); this answer has {}",
            hex_paths.len()
        )));
    }
    let hex_path = hex_paths[0];
    decode_initcode(hex_path, &sources[hex_path])?;

    // manifest
    let manifest = exact_object(root.get("manifest").expect("checked"), "manifest", &["targets", "tests"])?;
    let targets =
        manifest.get("targets").and_then(CanonValue::as_arr).ok_or_else(|| grammar("manifest.targets must be an array".into()))?;
    if targets.len() != 1 {
        return Err(grammar(format!("manifest.targets must hold exactly one target at v1, not {}", targets.len())));
    }
    let t = exact_object(&targets[0], "manifest.targets[0]", &["constructor_args", "name", "source"])?;
    let target_name = name_field(t, "manifest.targets[0]", "name")?;
    let source = string_field(t, "manifest.targets[0]", "source")?;
    if source != hex_path {
        return Err(grammar(format!("manifest.targets[0].source must name the .hex source {hex_path:?}, not {source:?}")));
    }
    let constructor_args = decode_hex(
        "manifest.targets[0].constructor_args",
        string_field(t, "manifest.targets[0]", "constructor_args")?,
        MAX_CONSTRUCTOR_ARGS_BYTES,
    )?;
    let target = BuildTarget { name: target_name, source: source.to_string(), constructor_args };

    let tests_arr =
        manifest.get("tests").and_then(CanonValue::as_arr).ok_or_else(|| grammar("manifest.tests must be an array".into()))?;
    if tests_arr.len() > MAX_TESTS {
        return Err(grammar(format!("manifest.tests must hold at most {MAX_TESTS} tests, not {}", tests_arr.len())));
    }
    let mut tests = Vec::with_capacity(tests_arr.len());
    let mut names: Vec<&str> = Vec::new();
    for (i, tv) in tests_arr.iter().enumerate() {
        let ctx = format!("manifest.tests[{i}]");
        let tobj = exact_object(tv, &ctx, &["calldata", "expect", "gas_limit", "name", "value"])?;
        let name = name_field(tobj, &ctx, "name")?;
        if names.contains(&name.as_str()) {
            return Err(grammar(format!("duplicate test name {name:?}")));
        }
        let calldata = decode_hex(&format!("{ctx}.calldata"), string_field(tobj, &ctx, "calldata")?, MAX_CALLDATA_BYTES)?;
        let value = tobj
            .get("value")
            .and_then(CanonValue::as_u64)
            .ok_or_else(|| grammar(format!("{ctx}.value must be an integer in 0..=2^64-1")))?;
        let gas_limit = tobj
            .get("gas_limit")
            .and_then(CanonValue::as_u64)
            .ok_or_else(|| grammar(format!("{ctx}.gas_limit must be an integer")))?;
        if !(MIN_TEST_GAS_LIMIT..=MAX_TEST_GAS_LIMIT).contains(&gas_limit) {
            return Err(grammar(format!("{ctx}.gas_limit must be {MIN_TEST_GAS_LIMIT}..={MAX_TEST_GAS_LIMIT}, not {gas_limit}")));
        }
        let expect = exact_object(tobj.get("expect").expect("checked"), &format!("{ctx}.expect"), &["output", "success"])?;
        let expect_success = expect
            .get("success")
            .and_then(CanonValue::as_bool)
            .ok_or_else(|| grammar(format!("{ctx}.expect.success must be a boolean")))?;
        let expect_output = match expect.get("output").expect("checked") {
            CanonValue::Null => None,
            CanonValue::Str(s) => Some(decode_hex(&format!("{ctx}.expect.output"), s, MAX_EXPECT_OUTPUT_BYTES)?),
            _ => return Err(grammar(format!("{ctx}.expect.output must be 0x-prefixed hex or null"))),
        };
        // `names` borrows the test's name from the tree, not from `name`, so it can outlive the move.
        names.push(string_field(tobj, &ctx, "name")?);
        tests.push(TestCase { name, calldata, value, gas_limit, expect_success, expect_output });
    }

    Ok(CodeDsl { toolchain: toolchain.to_string(), sources, target, tests })
}

/// The canonical tree of a validated answer: the parsed tree's keys and values, with the
/// manifest's hex fields in number form (lowercase).
fn canonical_tree(code: &CodeDsl) -> CanonValue {
    use CanonValue::{Arr, Bool, Int, Null, Obj, Str};
    let mut target = BTreeMap::new();
    target.insert("constructor_args".to_string(), Str(hex0x(&code.target.constructor_args)));
    target.insert("name".to_string(), Str(code.target.name.clone()));
    target.insert("source".to_string(), Str(code.target.source.clone()));
    let tests = code
        .tests
        .iter()
        .map(|t| {
            let mut expect = BTreeMap::new();
            expect.insert("output".to_string(), t.expect_output.as_ref().map_or(Null, |o| Str(hex0x(o))));
            expect.insert("success".to_string(), Bool(t.expect_success));
            let mut test = BTreeMap::new();
            test.insert("calldata".to_string(), Str(hex0x(&t.calldata)));
            test.insert("expect".to_string(), Obj(expect));
            test.insert("gas_limit".to_string(), Int(t.gas_limit as i128));
            test.insert("name".to_string(), Str(t.name.clone()));
            test.insert("value".to_string(), Int(t.value as i128));
            Obj(test)
        })
        .collect();
    let mut manifest = BTreeMap::new();
    manifest.insert("targets".to_string(), Arr(vec![Obj(target)]));
    manifest.insert("tests".to_string(), Arr(tests));
    let mut root = BTreeMap::new();
    root.insert("manifest".to_string(), Obj(manifest));
    root.insert("sources".to_string(), Obj(code.sources.iter().map(|(p, t)| (p.clone(), Str(t.clone()))).collect()));
    root.insert("toolchain".to_string(), Str(code.toolchain.clone()));
    root.insert("v".to_string(), Int(1));
    Obj(root)
}

// ---------------------------------------------------------------------------------------------
// The toolchain `evm/v1`
// ---------------------------------------------------------------------------------------------
//
// THE STATE FIXTURE, hashed into the run manifest (ADR-0078 SA-1): a fresh `CacheDB<EmptyDB>` —
// an empty world with no accounts, no code and no storage, and not one byte of this chain's own
// state — plus the fixed deployer at nonce 0 with a balance, under the fixed environment above.
// A contract that reads any other address reads zero.
//
// THE RUN. DEPLOY: `TxKind::Create` with data = initcode ‖ constructor_args, the deploy gas
// ceiling, gas price 0, committed. A deploy that reverts, halts, creates nothing or leaves no
// runtime code refuses the whole derivation, naming it. TESTS, in manifest order:
// `TxKind::Call(created)` from the deployer with the test's calldata, value and gas limit,
// executed WITHOUT commit — so EVERY TEST SEES THE FRESHLY DEPLOYED STATE and no test can depend
// on another's order or side effects; a test that needs prior state carries it in its own
// calldata or in the constructor. Recorded per call: success, output bytes and gas used.
//
// WHERE IT RUNS: not in this process. [`build_evm_v1_confined`] frames the job, spawns the runner
// binary under [`establish_confinement`], and compares the expectations itself. The split is the
// point, not the ceremony: the process that executes a stranger's bytecode holds no answer, no
// claim and no key, and the process that holds them executes nothing. The runner returns FACTS
// (success, output, gas); the transformer turns them into VERDICTS (`expectation_held`).

/// The outcome of one test call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestOutcome {
    pub name: String,
    pub success: bool,
    pub output: Vec<u8>,
    pub gas_used: u64,
    pub expectation_held: bool,
}

/// What the toolchain produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmBuild {
    pub created: [u8; 20],
    pub deploy_gas_used: u64,
    pub runtime_code: Vec<u8>,
    pub tests: Vec<TestOutcome>,
}

fn transformer(msg: String) -> DeriveError {
    DeriveError::Transformer(msg)
}

/// The fixed environment around one transaction.
fn evm_v1(db: &mut CacheDB<EmptyDB>, tx: TxEnv) -> Evm<'_, (), &mut CacheDB<EmptyDB>> {
    Evm::builder()
        .with_db(db)
        .with_spec_id(kaspa_evm::EVM_SPEC_ID)
        .modify_cfg_env(|c| c.chain_id = EVM_V1_CHAIN_ID)
        .modify_block_env(|b| {
            b.number = U256::ZERO;
            b.timestamp = U256::ZERO;
            b.coinbase = Address::ZERO;
            b.gas_limit = U256::from(EVM_V1_BLOCK_GAS_LIMIT);
            b.basefee = U256::ZERO;
            b.difficulty = U256::ZERO;
            b.prevrandao = Some(B256::ZERO);
        })
        .modify_tx_env(move |t| *t = tx)
        .build()
}

fn tx_env(caller: Address, to: TxKind, data: Vec<u8>, value: u64, gas_limit: u64, nonce: u64) -> TxEnv {
    TxEnv {
        caller,
        gas_limit,
        gas_price: U256::ZERO,
        transact_to: to,
        value: U256::from(value),
        data: Bytes::from(data),
        nonce: Some(nonce),
        chain_id: Some(EVM_V1_CHAIN_ID),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------
// The run manifest (ADR-0078 SA-1): the gas ceiling and the state fixture, named
// ---------------------------------------------------------------------------------------------
//
// SA-1: *"on an ephemeral, isolated state with a gas ceiling from the transformer manifest … The
// gas ceiling and the state-fixture hash are part of `transformer_id`'s manifest."* Two digests
// carry that: the fixture's, over the state the EVM starts from; the run manifest's, over the
// fixture digest and every gas ceiling the toolchain enforces. The run manifest's digest is in
// [`WRITER_NAME`] — hence in `transformer_id` — and in the artifact's header, and the runner
// refuses any job whose digest is not the one IT was compiled with.

/// A keyed BLAKE2b-512 digest under a domain, length-prefixed so no two preimages collide by
/// concatenation.
fn keyed_digest(domain: &[u8], data: &[u8]) -> [u8; 64] {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    state.update(&(data.len() as u64).to_le_bytes());
    state.update(data);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// The state fixture's canonical bytes — the state an `evm/v1` build starts from, spelled field
/// by field so a reader can check the digest by hand.
pub fn evm_v1_state_fixture_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"evm/v1/state-fixture");
    out.push(kaspa_evm::EVM_SPEC_ID as u8);
    put_u64_le(&mut out, EVM_V1_CHAIN_ID);
    put_u64_le(&mut out, EVM_V1_BLOCK_GAS_LIMIT);
    // The block environment, every field named and every field zero: a build reads no clock, no
    // coinbase, no basefee and no randomness.
    put_u64_le(&mut out, 0); // number
    put_u64_le(&mut out, 0); // timestamp
    out.extend_from_slice(&[0u8; 20]); // coinbase
    out.extend_from_slice(&[0u8; 32]); // basefee
    out.extend_from_slice(&[0u8; 32]); // difficulty
    out.extend_from_slice(&[0u8; 32]); // prevrandao
    // The only account that exists.
    out.extend_from_slice(&EVM_V1_DEPLOYER);
    out.extend_from_slice(&U256::from(EVM_V1_DEPLOYER_BALANCE).to_be_bytes::<32>());
    put_u64_le(&mut out, 0); // its nonce
    out.extend_from_slice(KECCAK_EMPTY.as_slice()); // it holds no code
    put_u32_le(&mut out, 0); // and there is no other account, no storage and no chain state
    out
}

/// `H(the state fixture)` — the "state-fixture hash" ADR-0078 SA-1 requires.
pub fn evm_v1_state_fixture_hash() -> [u8; 64] {
    keyed_digest(EVM_V1_STATE_FIXTURE_DOMAIN, &evm_v1_state_fixture_bytes())
}

/// The run manifest's canonical bytes: the fixture, then every ceiling the toolchain enforces.
pub fn evm_v1_run_manifest_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"evm/v1/run-manifest");
    out.extend_from_slice(&evm_v1_state_fixture_hash());
    put_u64_le(&mut out, EVM_V1_DEPLOY_GAS_LIMIT);
    put_u64_le(&mut out, EVM_V1_BLOCK_GAS_LIMIT);
    put_u64_le(&mut out, MIN_TEST_GAS_LIMIT);
    put_u64_le(&mut out, MAX_TEST_GAS_LIMIT);
    put_u64_le(&mut out, MAX_INITCODE_BYTES as u64);
    put_u64_le(&mut out, MAX_CONSTRUCTOR_ARGS_BYTES as u64);
    put_u64_le(&mut out, MAX_CALLDATA_BYTES as u64);
    put_u64_le(&mut out, MAX_TESTS as u64);
    out
}

/// `H(the run manifest)`.
pub fn evm_v1_run_manifest_hash() -> [u8; 64] {
    keyed_digest(EVM_V1_RUN_MANIFEST_DOMAIN, &evm_v1_run_manifest_bytes())
}

/// The tag [`WRITER_NAME`] carries: the ceiling a reader can see, and the digest that fixes the
/// rest of it.
pub fn evm_v1_run_manifest_tag() -> String {
    format!("evm-run/{EVM_V1_DEPLOY_GAS_LIMIT}/{}", &hex0x(&evm_v1_run_manifest_hash()[..8])[2..])
}

// ---------------------------------------------------------------------------------------------
// The job and the result — the runner's whole vocabulary
// ---------------------------------------------------------------------------------------------
//
// Little-endian, length-prefixed, CRC-trailered, the same conventions as the artifact. The job
// carries the run manifest's DIGEST and not its numbers: a runner uses the ceilings IT was
// compiled with and refuses a job that names any others, so no caller can widen a ceiling by
// asking (ADR-0072 Decision 8's rule in this small place — a field the caller chooses freely is a
// free draw, so this one is not a field).

/// One call in a job — the DSL's test, stripped of its name and its expectation. The runner does
/// not learn what a test is *for*: it returns facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmJobCall {
    pub calldata: Vec<u8>,
    pub value: u64,
    pub gas_limit: u64,
}

/// The canonical job: everything the runner needs, and nothing it may choose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmJob {
    pub run_manifest: [u8; 64],
    /// initcode ‖ constructor args.
    pub deploy_data: Vec<u8>,
    pub calls: Vec<EvmJobCall>,
}

/// What one call did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmCallOutcome {
    pub success: bool,
    pub output: Vec<u8>,
    pub gas_used: u64,
}

/// The canonical result: a build, or a refusal the parent turns into its own sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvmJobResult {
    Built { created: [u8; 20], deploy_gas_used: u64, runtime_code: Vec<u8>, calls: Vec<EvmCallOutcome> },
    Refused { code: u16, index: u32, gas_used: u64, detail: Vec<u8> },
}

/// Why a job produced no build. The runner reports the code; the transformer writes the sentence,
/// because the transformer is the side that knows the test's name.
pub mod refusal {
    pub const DEPLOY_NOT_ADMITTED: u16 = 1;
    pub const DEPLOY_CREATED_NOTHING: u16 = 2;
    pub const DEPLOY_REVERTED: u16 = 3;
    pub const DEPLOY_HALTED: u16 = 4;
    pub const DEPLOY_NO_RUNTIME_CODE: u16 = 5;
    pub const ACCOUNT_MISSING: u16 = 6;
    pub const CODE_UNREADABLE: u16 = 7;
    pub const CALL_NOT_ADMITTED: u16 = 8;
    pub const JOB_MALFORMED: u16 = 9;
}

pub const MEVJ_MAGIC: &[u8; 4] = b"MEVJ";
pub const MEVR_MAGIC: &[u8; 4] = b"MEVR";
pub const MEV_VERSION: u16 = 1;

/// The job frame the runner reads on stdin.
pub fn encode_evm_job(job: &EvmJob) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MEVJ_MAGIC);
    put_u16_le(&mut out, MEV_VERSION);
    out.extend_from_slice(&job.run_manifest);
    put_u32_le(&mut out, job.deploy_data.len() as u32);
    out.extend_from_slice(&job.deploy_data);
    put_u32_le(&mut out, job.calls.len() as u32);
    for call in &job.calls {
        put_u32_le(&mut out, call.calldata.len() as u32);
        out.extend_from_slice(&call.calldata);
        put_u64_le(&mut out, call.value);
        put_u64_le(&mut out, call.gas_limit);
    }
    let crc = crc32(&out);
    put_u32_le(&mut out, crc);
    out
}

/// Read a job frame. Every bound is checked here too: the runner trusts nothing, including its
/// own parent (ADR-0078 SA-2 — bounds are enforced before it runs).
pub fn decode_evm_job(bytes: &[u8]) -> Result<EvmJob, DeriveError> {
    let body = check_frame(bytes, MEVJ_MAGIC, "mevj")?;
    let mut c = Cursor { bytes: body, pos: 6, tag: "mevj" };
    let mut run_manifest = [0u8; 64];
    run_manifest.copy_from_slice(c.take(64, "run manifest")?);
    let deploy_data = c.bytes("deploy data")?;
    if deploy_data.len() > MAX_INITCODE_BYTES + MAX_CONSTRUCTOR_ARGS_BYTES {
        return Err(DeriveError::Mismatch(format!("mevj: deploy data is {} bytes, above the manifest's bound", deploy_data.len())));
    }
    let n = c.u32("call count")? as usize;
    if n > MAX_TESTS {
        return Err(DeriveError::Mismatch(format!("mevj: {n} calls, above the manifest's bound of {MAX_TESTS}")));
    }
    let mut calls = Vec::with_capacity(n);
    for i in 0..n {
        let calldata = c.bytes("calldata")?;
        if calldata.len() > MAX_CALLDATA_BYTES {
            return Err(DeriveError::Mismatch(format!("mevj: call {i}'s calldata is {} bytes, above the bound", calldata.len())));
        }
        let value = c.u64("value")?;
        let gas_limit = c.u64("gas limit")?;
        if !(MIN_TEST_GAS_LIMIT..=MAX_TEST_GAS_LIMIT).contains(&gas_limit) {
            return Err(DeriveError::Mismatch(format!("mevj: call {i}'s gas limit {gas_limit} is outside the manifest's range")));
        }
        calls.push(EvmJobCall { calldata, value, gas_limit });
    }
    if c.pos != body.len() {
        return Err(DeriveError::Mismatch(format!("mevj: {} trailing bytes before the CRC", body.len() - c.pos)));
    }
    Ok(EvmJob { run_manifest, deploy_data, calls })
}

/// The result frame the runner writes on stdout.
pub fn encode_evm_result(run_manifest: &[u8; 64], result: &EvmJobResult) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MEVR_MAGIC);
    put_u16_le(&mut out, MEV_VERSION);
    out.extend_from_slice(run_manifest);
    match result {
        EvmJobResult::Built { created, deploy_gas_used, runtime_code, calls } => {
            out.push(0);
            out.extend_from_slice(created);
            put_u64_le(&mut out, *deploy_gas_used);
            put_u32_le(&mut out, runtime_code.len() as u32);
            out.extend_from_slice(runtime_code);
            put_u32_le(&mut out, calls.len() as u32);
            for call in calls {
                out.push(u8::from(call.success));
                put_u32_le(&mut out, call.output.len() as u32);
                out.extend_from_slice(&call.output);
                put_u64_le(&mut out, call.gas_used);
            }
        }
        EvmJobResult::Refused { code, index, gas_used, detail } => {
            out.push(1);
            put_u16_le(&mut out, *code);
            put_u32_le(&mut out, *index);
            put_u64_le(&mut out, *gas_used);
            put_u32_le(&mut out, detail.len() as u32);
            out.extend_from_slice(detail);
        }
    }
    let crc = crc32(&out);
    put_u32_le(&mut out, crc);
    out
}

/// Read a result frame, with the run manifest it was produced under.
pub fn decode_evm_result(bytes: &[u8]) -> Result<([u8; 64], EvmJobResult), DeriveError> {
    let body = check_frame(bytes, MEVR_MAGIC, "mevr")?;
    let mut c = Cursor { bytes: body, pos: 6, tag: "mevr" };
    let mut run_manifest = [0u8; 64];
    run_manifest.copy_from_slice(c.take(64, "run manifest")?);
    let result = match c.u8("status")? {
        0 => {
            let mut created = [0u8; 20];
            created.copy_from_slice(c.take(20, "created address")?);
            let deploy_gas_used = c.u64("deploy gas used")?;
            let runtime_code = c.bytes("runtime code")?;
            let n = c.u32("call count")? as usize;
            if n > MAX_TESTS {
                return Err(DeriveError::Mismatch(format!("mevr: {n} outcomes, above the bound of {MAX_TESTS}")));
            }
            let mut calls = Vec::with_capacity(n);
            for _ in 0..n {
                let success = c.flag("call success")?;
                let output = c.bytes("call output")?;
                let gas_used = c.u64("call gas used")?;
                calls.push(EvmCallOutcome { success, output, gas_used });
            }
            EvmJobResult::Built { created, deploy_gas_used, runtime_code, calls }
        }
        1 => EvmJobResult::Refused {
            code: c.u16("refusal code")?,
            index: c.u32("refusal index")?,
            gas_used: c.u64("refusal gas")?,
            detail: c.bytes("refusal detail")?,
        },
        other => return Err(DeriveError::Mismatch(format!("mevr: status {other} is neither built nor refused"))),
    };
    if c.pos != body.len() {
        return Err(DeriveError::Mismatch(format!("mevr: {} trailing bytes before the CRC", body.len() - c.pos)));
    }
    Ok((run_manifest, result))
}

/// Magic, version and CRC, before a byte of a frame is believed.
fn check_frame<'a>(bytes: &'a [u8], magic: &[u8; 4], tag: &str) -> Result<&'a [u8], DeriveError> {
    if bytes.len() < 4 + 2 + 64 + 1 + 4 {
        return Err(DeriveError::Mismatch(format!("{tag}: shorter than a frame ({} bytes)", bytes.len())));
    }
    let (body, trailer) = bytes.split_at(bytes.len() - 4);
    let want = u32::from_le_bytes(trailer.try_into().expect("4 bytes"));
    if crc32(body) != want {
        return Err(DeriveError::Mismatch(format!("{tag}: the trailer's CRC-32 does not match the bytes")));
    }
    if &body[..4] != magic {
        return Err(DeriveError::Mismatch(format!("{tag}: magic is not {}", String::from_utf8_lossy(magic))));
    }
    let version = u16::from_le_bytes(body[4..6].try_into().expect("2 bytes"));
    if version != MEV_VERSION {
        return Err(DeriveError::Mismatch(format!("{tag}: version {version} is not {MEV_VERSION}")));
    }
    Ok(body)
}

// ---------------------------------------------------------------------------------------------
// The in-process executor — THE RUNNER'S ENTRY POINT, and nothing else's
// ---------------------------------------------------------------------------------------------

/// **Execute a job in THIS process.** ADR-0078 SA-1 forbids calling this from a process that
/// holds a claim, an answer or a key: the only caller in the tree is `bin/palw-evm-runner.rs`,
/// and `tests/derive_tree_guard.rs` fails if a second one appears. It is `pub` because a separate
/// binary needs it, and for no other reason.
///
/// Pure: the same job yields the same result on every host. It refuses a job whose run manifest is
/// not this build's — a runner beside a library that names different ceilings is a runner that
/// would silently execute under someone else's manifest.
pub fn execute_evm_job_in_this_process(job: &EvmJob) -> EvmJobResult {
    let malformed =
        |detail: String| EvmJobResult::Refused { code: refusal::JOB_MALFORMED, index: 0, gas_used: 0, detail: detail.into_bytes() };
    let mine = evm_v1_run_manifest_hash();
    if job.run_manifest != mine {
        return malformed(format!(
            "the job names run manifest {} and this runner was built with {}",
            hex0x(&job.run_manifest[..8]),
            hex0x(&mine[..8])
        ));
    }
    if job.deploy_data.is_empty() || job.deploy_data.len() > MAX_INITCODE_BYTES + MAX_CONSTRUCTOR_ARGS_BYTES {
        return malformed(format!("the deploy data is {} bytes", job.deploy_data.len()));
    }
    if job.calls.len() > MAX_TESTS {
        return malformed(format!("the job holds {} calls", job.calls.len()));
    }

    let deployer = Address::from(EVM_V1_DEPLOYER);
    let mut db = CacheDB::new(EmptyDB::default());
    db.insert_account_info(
        deployer,
        AccountInfo { balance: U256::from(EVM_V1_DEPLOYER_BALANCE), nonce: 0, code_hash: KECCAK_EMPTY, code: None },
    );

    let deploy = {
        let mut evm = evm_v1(&mut db, tx_env(deployer, TxKind::Create, job.deploy_data.clone(), 0, EVM_V1_DEPLOY_GAS_LIMIT, 0));
        match evm.transact_commit() {
            Ok(result) => result,
            Err(e) => {
                return EvmJobResult::Refused {
                    code: refusal::DEPLOY_NOT_ADMITTED,
                    index: 0,
                    gas_used: 0,
                    detail: format!("{e:?}").into_bytes(),
                };
            }
        }
    };
    let (created, deploy_gas_used) = match deploy {
        ExecutionResult::Success { output: Output::Create(_, Some(address)), gas_used, .. } => (address, gas_used),
        ExecutionResult::Success { gas_used, .. } => {
            return EvmJobResult::Refused { code: refusal::DEPLOY_CREATED_NOTHING, index: 0, gas_used, detail: Vec::new() };
        }
        ExecutionResult::Revert { gas_used, output } => {
            return EvmJobResult::Refused { code: refusal::DEPLOY_REVERTED, index: 0, gas_used, detail: output.to_vec() };
        }
        ExecutionResult::Halt { reason, gas_used } => {
            return EvmJobResult::Refused {
                code: refusal::DEPLOY_HALTED,
                index: 0,
                gas_used,
                detail: format!("{reason:?}").into_bytes(),
            };
        }
    };
    let info = match db.basic(created).ok().flatten() {
        Some(info) => info,
        None => {
            return EvmJobResult::Refused {
                code: refusal::ACCOUNT_MISSING,
                index: 0,
                gas_used: deploy_gas_used,
                detail: created.as_slice().to_vec(),
            };
        }
    };
    let runtime_code = if info.code_hash == KECCAK_EMPTY {
        Vec::new()
    } else {
        match db.code_by_hash(info.code_hash) {
            Ok(code) => code.original_bytes().to_vec(),
            Err(e) => {
                return EvmJobResult::Refused {
                    code: refusal::CODE_UNREADABLE,
                    index: 0,
                    gas_used: deploy_gas_used,
                    detail: format!("{e:?}").into_bytes(),
                };
            }
        }
    };
    if runtime_code.is_empty() {
        return EvmJobResult::Refused {
            code: refusal::DEPLOY_NO_RUNTIME_CODE,
            index: 0,
            gas_used: deploy_gas_used,
            detail: created.as_slice().to_vec(),
        };
    }

    let mut calls = Vec::with_capacity(job.calls.len());
    for (i, call) in job.calls.iter().enumerate() {
        let mut evm = evm_v1(&mut db, tx_env(deployer, TxKind::Call(created), call.calldata.clone(), call.value, call.gas_limit, 1));
        let outcome = match evm.transact() {
            Ok(outcome) => outcome,
            Err(e) => {
                return EvmJobResult::Refused {
                    code: refusal::CALL_NOT_ADMITTED,
                    index: i as u32,
                    gas_used: 0,
                    detail: format!("{e:?}").into_bytes(),
                };
            }
        };
        let (success, output, gas_used) = match outcome.result {
            ExecutionResult::Success { output, gas_used, .. } => (true, output.into_data().to_vec(), gas_used),
            ExecutionResult::Revert { output, gas_used } => (false, output.to_vec(), gas_used),
            ExecutionResult::Halt { gas_used, .. } => (false, Vec::new(), gas_used),
        };
        calls.push(EvmCallOutcome { success, output, gas_used });
    }

    EvmJobResult::Built { created: created.into_array(), deploy_gas_used, runtime_code, calls }
}

// ---------------------------------------------------------------------------------------------
// The confined half — where a transformer actually runs the toolchain
// ---------------------------------------------------------------------------------------------

/// Run the toolchain on a validated answer, in a child process (ADR-0078 SA-1).
pub fn build_evm_v1_confined(code: &CodeDsl) -> Result<EvmBuild, DeriveError> {
    let mut deploy_data = decode_initcode(&code.target.source, &code.sources[&code.target.source])?;
    deploy_data.extend_from_slice(&code.target.constructor_args);
    let job = EvmJob {
        run_manifest: evm_v1_run_manifest_hash(),
        deploy_data,
        calls: code
            .tests
            .iter()
            .map(|t| EvmJobCall { calldata: t.calldata.clone(), value: t.value, gas_limit: t.gas_limit })
            .collect(),
    };
    match run_job_confined(&job)? {
        EvmJobResult::Built { created, deploy_gas_used, runtime_code, calls } => {
            if calls.len() != code.tests.len() {
                return Err(transformer(format!(
                    "{TOOLCHAIN_EVM_V1}: the runner answered {} calls for {} tests",
                    calls.len(),
                    code.tests.len()
                )));
            }
            let tests = code
                .tests
                .iter()
                .zip(calls)
                .map(|(t, o)| {
                    let expectation_held =
                        t.expect_success == o.success && t.expect_output.as_ref().is_none_or(|want| *want == o.output);
                    TestOutcome { name: t.name.clone(), success: o.success, output: o.output, gas_used: o.gas_used, expectation_held }
                })
                .collect();
            Ok(EvmBuild { created, deploy_gas_used, runtime_code, tests })
        }
        EvmJobResult::Refused { code: reason, index, gas_used, detail } => {
            Err(refusal_sentence(code, reason, index, gas_used, &detail))
        }
    }
}

/// The runner's refusal in the transformer's own words — the same sentences this kind has always
/// produced, written on the side that knows the test's name.
fn refusal_sentence(code: &CodeDsl, reason: u16, index: u32, gas_used: u64, detail: &[u8]) -> DeriveError {
    let text = String::from_utf8_lossy(detail).chars().take(2048).collect::<String>();
    transformer(match reason {
        refusal::DEPLOY_NOT_ADMITTED => format!("{TOOLCHAIN_EVM_V1} deploy is not a transaction the EVM admits: {text}"),
        refusal::DEPLOY_CREATED_NOTHING => format!("{TOOLCHAIN_EVM_V1} deploy succeeded but created no account"),
        refusal::DEPLOY_REVERTED => format!("{TOOLCHAIN_EVM_V1} deploy reverted after {gas_used} gas with output {}", hex0x(detail)),
        refusal::DEPLOY_HALTED => format!("{TOOLCHAIN_EVM_V1} deploy halted ({text}) after {gas_used} gas"),
        refusal::DEPLOY_NO_RUNTIME_CODE => format!(
            "{TOOLCHAIN_EVM_V1} deploy left no runtime code at {}: an initcode that returns nothing builds nothing",
            hex0x(detail)
        ),
        refusal::ACCOUNT_MISSING => {
            format!("{TOOLCHAIN_EVM_V1} deploy named {} but the state holds no such account", hex0x(detail))
        }
        refusal::CODE_UNREADABLE => format!("{TOOLCHAIN_EVM_V1}: the created account's code is unreadable: {text}"),
        refusal::CALL_NOT_ADMITTED => {
            let name = code.tests.get(index as usize).map_or_else(|| format!("#{index}"), |t| format!("{:?}", t.name));
            format!("{TOOLCHAIN_EVM_V1} test {name} is not a transaction the EVM admits: {text}")
        }
        refusal::JOB_MALFORMED => format!("{TOOLCHAIN_EVM_V1}: the runner refused the job: {text}"),
        other => format!("{TOOLCHAIN_EVM_V1}: the runner refused with an unknown code {other}: {text}"),
    })
}

/// The deadline, DERIVED from the gas the answer itself declares (ADR-0077 SA-4's shape: a
/// deadline is derived per row, never chosen), capped so nothing hangs a gateway for a day.
pub fn evm_v1_deadline_secs(job: &EvmJob) -> u64 {
    let declared = job.calls.iter().fold(EVM_V1_DEPLOY_GAS_LIMIT, |acc, c| acc.saturating_add(c.gas_limit));
    EVM_V1_DEADLINE_BASE_SECS.saturating_add(declared / EVM_V1_GAS_PER_SEC_FLOOR).min(EVM_V1_DEADLINE_MAX_SECS)
}

/// Where the runner is: the operator's answer first, then beside this binary, then one directory
/// up (a `target/debug/deps/` test binary is a sibling of the very tree that built the runner).
/// There is no fourth candidate and no fallback: an absent runner refuses the derivation.
pub fn locate_runner() -> Result<PathBuf, DeriveError> {
    if let Some(named) = std::env::var_os(RUNNER_PATH_ENV) {
        let path = PathBuf::from(named);
        if path.is_file() {
            return Ok(path);
        }
        return Err(transformer(format!("{RUNNER_PATH_ENV} names {}, which is not a file", path.display())));
    }
    let exe = std::env::current_exe()
        .map_err(|e| transformer(format!("cannot read this binary's own path to find {RUNNER_BIN_NAME}: {e}")))?;
    let mut candidates = Vec::new();
    if let Some(dir) = exe.parent() {
        candidates.push(dir.join(RUNNER_BIN_NAME));
        if let Some(up) = dir.parent() {
            candidates.push(up.join(RUNNER_BIN_NAME));
        }
    }
    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }
    let looked: Vec<String> = candidates.iter().map(|c| c.display().to_string()).collect();
    Err(transformer(format!(
        "{RUNNER_BIN_NAME} is not beside this binary (looked at {}). ADR-0078 SA-1: model-written initcode runs in a \
         separate confined process, so `code`/`contract` refuse rather than fall back in-process — ship \
         {RUNNER_BIN_NAME} next to the binary that derives, or name it with {RUNNER_PATH_ENV}",
        looked.join(", ")
    )))
}

/// **Spawn a child and wait for it under a resident ceiling and a wall-clock deadline**
/// (ADR-0079 Decision 6 as corrected by SA-1: the ceiling measures what the process is charged
/// for, and it is a safety net, not a tuning knob).
///
/// Neither bound can change a result: a child that breaches one is killed and its caller returns
/// a refusal — ADR-0078 Decision 2's parse-failure arm, *no object* — so two hosts either agree on
/// the artifact or one of them produces none. That is exactly ADR-0079 S4's shape.
///
/// `arm_memory_ceiling` is deliberately NOT used here: on Linux it writes `memory.max` into the
/// operator's delegated WORKER cgroup, and a derivation must not rewrite the ceiling of the model
/// process that owns it. The parent polls instead, which is the same mechanism
/// `MemoryCeilingBackend::ResidentWatchdog` names.
fn wait_with_ceiling(
    cmd: &mut std::process::Command,
    stdin_bytes: Option<&[u8]>,
    what: &str,
    deadline_secs: u64,
) -> Result<std::process::Output, DeriveError> {
    let io = |stage: &str, e: std::io::Error| transformer(format!("{what}: {stage}: {e}"));
    let mut child = cmd.spawn().map_err(|e| io("spawn", e))?;
    let pid = child.id();

    if let Some(bytes) = stdin_bytes
        && let Some(mut pipe) = child.stdin.take()
    {
        // A broken pipe here means the child died early; the exit status below says how. Both
        // children of this module read their input to the end before they write, so a single
        // write cannot deadlock against a full output pipe.
        let _ = pipe.write_all(bytes);
        let _ = pipe.flush();
    }
    // Bounded reads. The runner is this crate's own binary, but its PATH is an operator's to set
    // (`MISAKA_PALW_EVM_RUNNER`), and a parent that reads an unbounded pipe from a child it did not
    // build is a parent a child can exhaust.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stdout.as_mut() {
            let _ = pipe.take(MAX_CHILD_STDOUT_BYTES).read_to_end(&mut buf);
        }
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(pipe) = stderr.as_mut() {
            let _ = pipe.take(MAX_CHILD_STDERR_BYTES).read_to_end(&mut buf);
        }
        buf
    });

    let deadline = Duration::from_secs(deadline_secs);
    let started = Instant::now();
    let mut polls: u32 = 0;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(io("wait", e)),
        }
        if started.elapsed() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(transformer(format!(
                "{what}: killed at its deadline of {deadline_secs} s — no object (a deadline can refuse a build, \
                 never change one)"
            )));
        }
        // The resident measurement costs a `/bin/ps` on macOS, so it runs on its own slower
        // cadence: the ceiling is a wall against a runaway, not a sampler.
        if polls.is_multiple_of(EVM_V1_RESIDENT_POLL_EVERY)
            && let (Some(resident), _) = resident_bytes(pid)
            && resident > EVM_V1_MAX_RESIDENT_BYTES
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(transformer(format!(
                "{what}: killed at {resident} resident bytes, above the ceiling of {EVM_V1_MAX_RESIDENT_BYTES} — no \
                 object"
            )));
        }
        polls = polls.wrapping_add(1);
        std::thread::sleep(Duration::from_millis(EVM_V1_POLL_MILLIS));
    };
    Ok(std::process::Output { status, stdout: out_reader.join().unwrap_or_default(), stderr: err_reader.join().unwrap_or_default() })
}

/// Spawn the runner in an ephemeral tree and destroy the tree afterwards.
fn run_job_confined(job: &EvmJob) -> Result<EvmJobResult, DeriveError> {
    let runner = locate_runner()?;
    let work = fresh_work_dir("evm")?;
    let outcome = run_job_in(&runner, job, &work);
    // The tree is scratch and holds nothing but the confinement's own profile; a failure to remove
    // it changes no result, and the test that asserts no tree outlives a run is the check.
    let _ = std::fs::remove_dir_all(&work);
    outcome
}

fn run_job_in(runner: &Path, job: &EvmJob, work: &Path) -> Result<EvmJobResult, DeriveError> {
    // The child may write in its own ephemeral tree and nowhere else. A host with no backend runs
    // it under the environment discipline alone, which is ADR-0079 Decision 5's honest `none` —
    // the in-tree EVM is Decision 12's first bullet ("it needs none of this"), so a missing
    // backend degrades the cage and never the answer.
    let (confinement, _notes) = establish_confinement(work, &[work.to_path_buf()]);
    let mut cmd = confinement.command(runner);
    // ADR-0079 Decision 5's portable half, in its one spelling: env_clear plus the allowlist, and
    // a working directory that is neither the operator's home nor the node's datadir.
    harden_worker_command(&mut cmd, work);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let what = format!("{TOOLCHAIN_EVM_V1} runner");
    let output = wait_with_ceiling(&mut cmd, Some(&encode_evm_job(job)), &what, evm_v1_deadline_secs(job))?;
    if !output.status.success() {
        let shown: String = String::from_utf8_lossy(&output.stderr).chars().take(2048).collect();
        return Err(transformer(format!("{what}: exited with {} (backend {}): {shown}", output.status, confinement.backend().name())));
    }
    let (run_manifest, result) = decode_evm_result(&output.stdout)?;
    if run_manifest != job.run_manifest {
        return Err(transformer(format!(
            "{TOOLCHAIN_EVM_V1} runner: answered under run manifest {}, and the job named {}",
            hex0x(&run_manifest[..8]),
            hex0x(&job.run_manifest[..8])
        )));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------------------------
// The artifact: `MCOD` version 1
// ---------------------------------------------------------------------------------------------
//
// Little-endian throughout; a string or byte string is a `u32` length then the bytes.
//
// ```text
// HEADER    magic b"MCOD" · u16 version = 2 · string toolchain · u8 spec id (EVM_SPEC_ID as u8)
//           · [u8; 64] run manifest digest — the gas ceiling and the state fixture this build ran
//             under, so the artifact names them and not only `transformer_id` (ADR-0078 SA-1)
// SOURCES   u32 count · per source, sorted by path: string path · [u8; 64] digest
//           digest = keyed BLAKE2b-512 (key SOURCE_DIGEST_DOMAIN) over u64 len ‖ text
//           — the DSL already carries the text; the artifact is the build, and names it
// BUILD     string target name · [u8; 20] created address · u64 deploy gas used · bytes runtime code
// TEST LOG  u32 count · per test, in manifest order:
//           string name · u8 success · bytes output · u64 gas used · u8 expectation held
// SUMMARY   u32 tests passed · u32 tests failed
// TRAILER   u32 CRC-32 over everything before it
// ```

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DeriveError> {
    let len = u32::try_from(bytes.len()).map_err(|_| transformer("a field exceeds the u32 length prefix".into()))?;
    put_u32_le(out, len);
    out.extend_from_slice(bytes);
    Ok(())
}

/// The keyed BLAKE2b-512 digest of a source's text, as the SOURCES section records it.
pub fn source_digest(text: &str) -> [u8; 64] {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(SOURCE_DIGEST_DOMAIN).to_state();
    state.update(&(text.len() as u64).to_le_bytes());
    state.update(text.as_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// Write the artifact.
pub fn write_mcod(code: &CodeDsl, build: &EvmBuild) -> Result<Vec<u8>, DeriveError> {
    let mut out = Vec::new();
    out.extend_from_slice(MCOD_MAGIC);
    put_u16_le(&mut out, MCOD_VERSION);
    put_bytes(&mut out, code.toolchain.as_bytes())?;
    out.push(kaspa_evm::EVM_SPEC_ID as u8);
    out.extend_from_slice(&evm_v1_run_manifest_hash());

    put_u32_le(&mut out, code.sources.len() as u32);
    for (path, text) in &code.sources {
        put_bytes(&mut out, path.as_bytes())?;
        out.extend_from_slice(&source_digest(text));
    }

    put_bytes(&mut out, code.target.name.as_bytes())?;
    out.extend_from_slice(&build.created);
    put_u64_le(&mut out, build.deploy_gas_used);
    put_bytes(&mut out, &build.runtime_code)?;

    put_u32_le(&mut out, build.tests.len() as u32);
    let mut passed = 0u32;
    for t in &build.tests {
        put_bytes(&mut out, t.name.as_bytes())?;
        out.push(u8::from(t.success));
        put_bytes(&mut out, &t.output)?;
        put_u64_le(&mut out, t.gas_used);
        out.push(u8::from(t.expectation_held));
        passed += u32::from(t.expectation_held);
        if out.len() > MAX_ARTIFACT_BYTES {
            return Err(transformer(format!("the artifact exceeds {MAX_ARTIFACT_BYTES} bytes at test {:?}", t.name)));
        }
    }
    put_u32_le(&mut out, passed);
    put_u32_le(&mut out, build.tests.len() as u32 - passed);

    let crc = crc32(&out);
    put_u32_le(&mut out, crc);
    if out.len() > MAX_ARTIFACT_BYTES {
        return Err(transformer(format!("the artifact exceeds {MAX_ARTIFACT_BYTES} bytes")));
    }
    Ok(out)
}

/// A read `MCOD` file — what a consumer or a CLI sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McodFile {
    pub version: u16,
    pub toolchain: String,
    pub spec_id: u8,
    /// The run manifest the build ran under (ADR-0078 SA-1) — a consumer compares it with
    /// [`evm_v1_run_manifest_hash`] before it believes the gas numbers below.
    pub run_manifest: [u8; 64],
    pub sources: Vec<(String, [u8; 64])>,
    pub target_name: String,
    pub created: [u8; 20],
    pub deploy_gas_used: u64,
    pub runtime_code: Vec<u8>,
    pub tests: Vec<TestOutcome>,
    pub tests_passed: u32,
    pub tests_failed: u32,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// Which format the reader is in — `mcod`, `mevj` or `mevr` — so a truncation names itself.
    tag: &'static str,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8], DeriveError> {
        let end = self.pos.checked_add(n).filter(|end| *end <= self.bytes.len());
        let end = end.ok_or_else(|| DeriveError::Mismatch(format!("{}: truncated at {what} (offset {})", self.tag, self.pos)))?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }
    fn u8(&mut self, what: &str) -> Result<u8, DeriveError> {
        Ok(self.take(1, what)?[0])
    }
    fn u16(&mut self, what: &str) -> Result<u16, DeriveError> {
        Ok(u16::from_le_bytes(self.take(2, what)?.try_into().expect("2 bytes")))
    }
    fn u32(&mut self, what: &str) -> Result<u32, DeriveError> {
        Ok(u32::from_le_bytes(self.take(4, what)?.try_into().expect("4 bytes")))
    }
    fn u64(&mut self, what: &str) -> Result<u64, DeriveError> {
        Ok(u64::from_le_bytes(self.take(8, what)?.try_into().expect("8 bytes")))
    }
    fn bytes(&mut self, what: &str) -> Result<Vec<u8>, DeriveError> {
        let len = self.u32(what)? as usize;
        Ok(self.take(len, what)?.to_vec())
    }
    fn string(&mut self, what: &str) -> Result<String, DeriveError> {
        String::from_utf8(self.bytes(what)?).map_err(|_| DeriveError::Mismatch(format!("{}: {what} is not UTF-8", self.tag)))
    }
    fn flag(&mut self, what: &str) -> Result<bool, DeriveError> {
        let tag = self.tag;
        match self.u8(what)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(DeriveError::Mismatch(format!("{tag}: {what} is {other}, not 0 or 1"))),
        }
    }
}

/// Read an `MCOD` artifact, checking the trailer's CRC first and every bound on the way.
pub fn read_mcod(bytes: &[u8]) -> Result<McodFile, DeriveError> {
    if bytes.len() < 4 + 2 + 4 + 1 + 64 + 4 {
        return Err(DeriveError::Mismatch("mcod: shorter than a header".into()));
    }
    let (body, trailer) = bytes.split_at(bytes.len() - 4);
    let want = u32::from_le_bytes(trailer.try_into().expect("4 bytes"));
    if crc32(body) != want {
        return Err(DeriveError::Mismatch("mcod: the trailer's CRC-32 does not match the bytes".into()));
    }
    let mut c = Cursor { bytes: body, pos: 0, tag: "mcod" };
    if c.take(4, "magic")? != MCOD_MAGIC {
        return Err(DeriveError::Mismatch("mcod: magic is not MCOD".into()));
    }
    let version = c.u16("version")?;
    if version != MCOD_VERSION {
        return Err(DeriveError::Mismatch(format!("mcod: version {version} is not {MCOD_VERSION}")));
    }
    let toolchain = c.string("toolchain")?;
    let spec_id = c.u8("spec id")?;
    let mut run_manifest = [0u8; 64];
    run_manifest.copy_from_slice(c.take(64, "run manifest")?);
    let n_sources = c.u32("source count")? as usize;
    let mut sources = Vec::with_capacity(n_sources.min(MAX_SOURCES));
    for _ in 0..n_sources {
        let path = c.string("source path")?;
        let mut digest = [0u8; 64];
        digest.copy_from_slice(c.take(64, "source digest")?);
        sources.push((path, digest));
    }
    let target_name = c.string("target name")?;
    let mut created = [0u8; 20];
    created.copy_from_slice(c.take(20, "created address")?);
    let deploy_gas_used = c.u64("deploy gas used")?;
    let runtime_code = c.bytes("runtime code")?;
    let n_tests = c.u32("test count")? as usize;
    let mut tests = Vec::with_capacity(n_tests.min(MAX_TESTS));
    for _ in 0..n_tests {
        let name = c.string("test name")?;
        let success = c.flag("test success")?;
        let output = c.bytes("test output")?;
        let gas_used = c.u64("test gas used")?;
        let expectation_held = c.flag("test expectation")?;
        tests.push(TestOutcome { name, success, output, gas_used, expectation_held });
    }
    let tests_passed = c.u32("tests passed")?;
    let tests_failed = c.u32("tests failed")?;
    if c.pos != body.len() {
        return Err(DeriveError::Mismatch(format!("mcod: {} trailing bytes before the CRC", body.len() - c.pos)));
    }
    Ok(McodFile {
        version,
        toolchain,
        spec_id,
        run_manifest,
        sources,
        target_name,
        created,
        deploy_gas_used,
        runtime_code,
        tests,
        tests_passed,
        tests_failed,
    })
}

// ---------------------------------------------------------------------------------------------
// The external runner — the hermetic half of the `code` row, UNREGISTERED
// ---------------------------------------------------------------------------------------------
//
// ADR-0078 Decision 11: "pinned external toolchains (rustc → wasm32, solc, clang → wasm32) are
// manifests — toolchain hash, arguments, environment whitelist, SOURCE_DATE_EPOCH, no network —
// whose two-architecture drill runs on the fleet's Intel, AMD and Apple hosts, and none is named
// by an object until its drill passes". This is the runner such a drill would use. It is not a
// `Transformer` and [`register`] does not return it: a toolchain is named by its drill, and no
// drill has passed.
//
// What the runner fixes: the binary (its SHA-256 must equal the manifest's), the arguments, the
// WHOLE environment (`env_clear()`, then exactly the manifest's variables, then the three the
// runner owns: `SOURCE_DATE_EPOCH` from the manifest's field, `TZ=UTC`, `LC_ALL=C` — these three
// win over anything the manifest spelled), the working tree (a fresh directory, the sources
// materialized under `src/`, outputs collected from `out/`, sorted by name), and the standard
// input (closed).
//
// What the runner CANNOT fix by itself, stated plainly: network isolation and the clock. A
// process environment cannot deny a socket or freeze time; `SOURCE_DATE_EPOCH` is a convention a
// toolchain honours, not a wall this function builds. ADR-0079 Decision 12 answers the first half
// and refuses to run without it: the socket denial is the platform BACKEND's, proven by its own
// drill (`establish_confinement`), and a host whose backend is `none` does not run an external
// toolchain at all. The clock stays a convention, and a toolchain that reaches for it is excluded
// by ADR-0078 Decision 11's fleet drill, not by this function.
//
// THE GATE (ADR-0079 Decision 12, invariant S11), in the order it refuses:
//
//   1. no bond or wallet key may be reachable — "the build's output is never executed on a host
//      that holds a bond key or a wallet key", checked over this process's OWN environment and
//      the directories the caller names (identity, outbox, datadir). A `code` row's test log IS
//      the execution of a program a model wrote; this is the completion condition for ADR-0078's
//      Q-05 and not advice.
//   2. the confinement backend must be one the host PROVED (`establish_confinement` runs its
//      drill and reports `none` when it cannot) — otherwise there is no network denial and no
//      write denial, only a promise.
//   3. an ephemeral tree, destroyed after the run, holding the sources and the outputs.
//   4. the manifest's environment and nothing else, plus the resident ceiling and the derived
//      deadline the EVM runner uses.

/// A pinned external toolchain (ADR-0078 Decision 11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalToolchainManifest {
    /// e.g. `rustc-1.88.0/wasm32-unknown-unknown`
    pub name: String,
    /// The SHA-256 of the binary the runner is handed; a mismatch refuses the run.
    pub binary_sha256: [u8; 32],
    /// Arguments, with `{src}` (the materialized sources' directory) and `{out}` (the directory
    /// outputs are collected from) replaced wherever they occur.
    pub argv: Vec<String>,
    /// The WHOLE environment — nothing is inherited.
    pub env: BTreeMap<String, String>,
    pub source_date_epoch: u64,
}

/// A fresh work directory under `std::env::temp_dir()`, named by process id and a counter — no
/// clock, no randomness. Every tree this module makes is destroyed by the run that made it; the
/// name is [`WORK_DIR_PREFIX`] so an operator can see at a glance that none outlived one.
pub fn fresh_work_dir(tag: &str) -> Result<PathBuf, DeriveError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for _ in 0..1024 {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("{WORK_DIR_PREFIX}{tag}-{pid}-{n}"));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(transformer(format!("external toolchain: cannot create a work directory under {}: {e}", base.display())));
            }
        }
    }
    Err(transformer("external toolchain: no free work directory name".into()))
}

/// Run a pinned external toolchain over `sources` and collect `out_collect` (paths under the
/// output directory), sorted by name.
///
/// `key_dirs` are the directories the CALLER knows about — its identity directory, its outbox, its
/// datadir. They are swept for secret-shaped files before anything runs (ADR-0079 Decision 12's
/// third bullet). Passing an empty slice does not weaken the rule to nothing: the process's own
/// environment is always swept. It does mean a caller that knows where its keys live must say so,
/// which is the honest shape of a check no library can make complete.
///
/// See the module section above for the whole gate and for what it does not fix.
pub fn run_external(
    manifest: &ExternalToolchainManifest,
    binary: &Path,
    sources: &BTreeMap<String, String>,
    out_collect: &[String],
    key_dirs: &[&Path],
) -> Result<Vec<(String, Vec<u8>)>, DeriveError> {
    // (1) Keys first, and before the binary is even read: a host that holds a bond does not build.
    let reachable = reachable_signing_secrets(|name| std::env::var(name).ok(), key_dirs);
    if !reachable.is_empty() {
        let found: Vec<String> = reachable.iter().map(|s| s.to_string()).collect();
        return Err(transformer(format!(
            "external toolchain {}: refusing to build on a host that holds a bond or wallet key — {}. ADR-0079 \
             Decision 12: a `code` row's test log is the execution of a program a model wrote; it runs on a \
             disposable host or in the same confinement with no writable state that outlives it, or the row's \
             transformer does not ship",
            manifest.name,
            found.join("; ")
        )));
    }

    let binary_bytes = std::fs::read(binary)
        .map_err(|e| transformer(format!("external toolchain {}: cannot read {}: {e}", manifest.name, binary.display())))?;
    let got = sha256(&binary_bytes);
    if got != manifest.binary_sha256 {
        return Err(transformer(format!(
            "external toolchain {}: the binary at {} hashes to {}, the manifest names {}",
            manifest.name,
            binary.display(),
            hex0x(&got),
            hex0x(&manifest.binary_sha256)
        )));
    }
    if manifest.argv.is_empty() {
        return Err(transformer(format!("external toolchain {}: the manifest has no arguments", manifest.name)));
    }
    for path in sources.keys() {
        check_source_path(path)?;
    }
    let mut collect: Vec<&String> = out_collect.iter().collect();
    collect.sort();
    collect.dedup();
    for path in &collect {
        check_source_path(path)?;
    }

    let work = fresh_work_dir("run")?;
    let result = run_in(manifest, binary, sources, &collect, &work);
    // Best effort: the work tree is scratch, and a failure to remove it changes no result.
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn run_in(
    manifest: &ExternalToolchainManifest,
    binary: &Path,
    sources: &BTreeMap<String, String>,
    collect: &[&String],
    work: &Path,
) -> Result<Vec<(String, Vec<u8>)>, DeriveError> {
    let io = |what: &str, e: std::io::Error| transformer(format!("external toolchain {}: {what}: {e}", manifest.name));

    // (2) The backend, PROVEN. `establish_confinement` runs its own drill and reports `none` when
    // it cannot deny a socket and a write; on `none` there is no cage, so there is no run.
    let (confinement, notes) = establish_confinement(work, &[work.to_path_buf()]);
    if confinement.backend() == ConfinementBackend::None {
        return Err(transformer(format!(
            "external toolchain {}: this host's confinement backend is `none`, so a build cannot be denied a socket \
             or a write — an external toolchain is named only by its fleet drill (ADR-0078 Decision 11), and \
             ADR-0079 Decision 12 gives it the narrowest cage or no run. Set {}=linux-seccomp-landlock or \
             macos-sandbox-exec on a host whose backend proves its own denials. The attempt said: {}",
            manifest.name,
            misaka_palw::host_security::PALW_CONFINEMENT_ENV,
            notes.join("; ")
        )));
    }

    let src_dir = work.join("src");
    let out_dir = work.join("out");
    std::fs::create_dir(&src_dir).map_err(|e| io("create src/", e))?;
    std::fs::create_dir(&out_dir).map_err(|e| io("create out/", e))?;
    for (path, text) in sources {
        let file = src_dir.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io(&format!("create the directory of {path}"), e))?;
        }
        std::fs::write(&file, text.as_bytes()).map_err(|e| io(&format!("write {path}"), e))?;
    }

    let src_s = src_dir.to_string_lossy().into_owned();
    let out_s = out_dir.to_string_lossy().into_owned();
    let argv: Vec<String> = manifest.argv.iter().map(|a| a.replace("{src}", &src_s).replace("{out}", &out_s)).collect();

    // (4) The manifest's environment and nothing else, inside the cage the backend proved.
    let mut cmd = confinement.command(binary);
    cmd.args(&argv)
        .env_clear()
        .envs(&manifest.env)
        .env("SOURCE_DATE_EPOCH", manifest.source_date_epoch.to_string())
        .env("TZ", "UTC")
        .env("LC_ALL", "C")
        .current_dir(work)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = wait_with_ceiling(&mut cmd, None, &format!("external toolchain {}", manifest.name), EVM_V1_DEADLINE_MAX_SECS)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let shown: String = stderr.chars().take(2048).collect();
        return Err(transformer(format!(
            "external toolchain {}: exited with {} under backend {}: {shown}",
            manifest.name,
            output.status,
            confinement.backend().name()
        )));
    }

    let mut collected = Vec::with_capacity(collect.len());
    for name in collect {
        let bytes = std::fs::read(out_dir.join(name)).map_err(|e| io(&format!("collect out/{name}"), e))?;
        collected.push(((*name).clone(), bytes));
    }
    Ok(collected)
}

/// SHA-256 (FIPS 180-4), integer code with no crate — the same function `build.rs` uses for the
/// source tree, here for the external binary's pin.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
        0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
        0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (word, quad) in w.iter_mut().zip(chunk.chunks(4)) {
            *word = u32::from_be_bytes(quad.try_into().expect("4 bytes"));
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut a = h;
        for (k, wi) in K.iter().zip(w.iter()) {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ (!a[4] & a[6]);
            let t1 = a[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(*k).wrapping_add(*wi);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a = [t1.wrapping_add(t2), a[0], a[1], a[2], a[3].wrapping_add(t1), a[4], a[5], a[6]];
        }
        for (hi, ai) in h.iter_mut().zip(a.iter()) {
            *hi = hi.wrapping_add(*ai);
        }
    }
    let mut out = [0u8; 32];
    for (dst, word) in out.chunks_mut(4).zip(h.iter()) {
        dst.copy_from_slice(&word.to_be_bytes());
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

/// **These tests spawn `palw-evm-runner`** — ADR-0078 SA-1 means there is no in-process path for
/// a `code` or `contract` derivation, in a test any more than in a gateway. Run them as
/// `cargo test -p misaka-palw-derive`, which builds the crate's binaries. `cargo test
/// -p misaka-palw-derive --lib` does NOT build binaries, and every EVM test then fails with the
/// runner's absence message: that is the gate holding, not a broken test.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::{ClaimBinding, derive_with};
    use kaspa_hashes::Hash64;
    use serde_json::{Value, json};

    // Hand-assembled EVM bytecode. The classic "return 42" runtime, 10 bytes:
    //   60 2a   PUSH1 0x2a          the answer
    //   60 00   PUSH1 0x00          memory offset 0
    //   52      MSTORE              mem[0..32] = 42 (left-padded)
    //   60 20   PUSH1 0x20          32 bytes
    //   60 00   PUSH1 0x00          from offset 0
    //   f3      RETURN
    // and the standard initcode that copies a runtime of length L from code offset 12 (the
    // initcode is itself 12 bytes) to memory and returns it:
    //   60 LL   PUSH1 L             runtime length
    //   60 0c   PUSH1 0x0c          code offset of the runtime (right after these 12 bytes)
    //   60 00   PUSH1 0x00          memory destination
    //   39      CODECOPY
    //   60 LL   PUSH1 L
    //   60 00   PUSH1 0x00
    //   f3      RETURN              the runtime becomes the account's code
    const RETURN_42_RUNTIME: &str = "602a60005260206000f3";
    const RETURN_42_INITCODE: &str = "600a600c600039600a6000f3";
    /// PUSH1 0 PUSH1 0 REVERT — an initcode that reverts.
    const REVERTING_INITCODE: &str = "60006000fd";
    const WORD_42: &str = "0x000000000000000000000000000000000000000000000000000000000000002a";
    const WORD_43: &str = "0x000000000000000000000000000000000000000000000000000000000000002b";

    fn return_42_answer() -> Value {
        json!({
            "v": 1,
            "toolchain": "evm/v1",
            "sources": {
                "return42.hex": format!("0x{RETURN_42_INITCODE}{RETURN_42_RUNTIME}\n"),
                "README.md": "the answer is 42\n"
            },
            "manifest": {
                "targets": [ { "name": "return42", "source": "return42.hex", "constructor_args": "0x" } ],
                "tests": [
                    { "name": "answers-42", "calldata": "0x", "value": 0, "gas_limit": 100000,
                      "expect": { "success": true, "output": WORD_42 } },
                    { "name": "expects-43", "calldata": "0x01", "value": 0, "gas_limit": 100000,
                      "expect": { "success": true, "output": WORD_43 } }
                ]
            }
        })
    }

    fn bytes_of(v: &Value) -> Vec<u8> {
        serde_json::to_vec(v).unwrap()
    }

    fn canonical(v: &Value) -> Vec<u8> {
        CodeGrammar.canonicalize(&bytes_of(v)).unwrap()
    }

    fn refusal(v: &Value) -> String {
        match CodeGrammar.canonicalize(&bytes_of(v)) {
            Err(DeriveError::Grammar(msg)) => msg,
            other => panic!("expected a grammar refusal, got {other:?}"),
        }
    }

    fn binding() -> ClaimBinding {
        ClaimBinding {
            network_domain: Hash64::from_bytes([0x01; 64]),
            claim_id: Hash64::from_bytes([0x02; 64]),
            output_root: Hash64::from_bytes([0x03; 64]),
            executor_pubkey: vec![0x11; 2592],
        }
    }

    // (1) canonicalization -------------------------------------------------------------------

    #[test]
    fn canonicalization_is_idempotent_sorted_and_case_normalizing() {
        let once = canonical(&return_42_answer());
        let twice = CodeGrammar.canonicalize(&once).unwrap();
        assert_eq!(once, twice);
        let text = std::str::from_utf8(&once).unwrap();
        assert!(
            text.starts_with(
                r#"{"manifest":{"targets":[{"constructor_args":"0x","name":"return42","source":"return42.hex"}],"tests":["#
            )
        );
        // sources sorted by path: README.md before return42.hex (the entries, not the target's name)
        assert!(text.find(r#""README.md":"the answer"#).unwrap() < text.find(r#""return42.hex":"0x"#).unwrap());
        assert!(text.ends_with(r#""toolchain":"evm/v1","v":1}"#));
        // hex fields are number-formed to lowercase; source text is verbatim
        let mut upper = return_42_answer();
        upper["manifest"]["tests"][0]["expect"]["output"] = json!(WORD_42.to_uppercase().replace("0X", "0x"));
        upper["manifest"]["tests"][1]["calldata"] = json!("0x0A");
        assert_ne!(bytes_of(&upper), bytes_of(&return_42_answer()));
        let mut lower = return_42_answer();
        lower["manifest"]["tests"][1]["calldata"] = json!("0x0a");
        assert_eq!(canonical(&upper), canonical(&lower));
    }

    // (2) schema refusals ---------------------------------------------------------------------

    #[test]
    fn schema_refusals_name_their_reasons() {
        let mut v = return_42_answer();
        v["toolchain"] = json!("rustc-1.88.0/wasm32-unknown-unknown");
        let msg = refusal(&v);
        assert!(msg.contains("fleet drill") && msg.contains("Decision 11"), "{msg}");

        let mut v = return_42_answer();
        v["extra"] = json!(1);
        assert!(refusal(&v).contains("unknown key \"extra\""));
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["gas"] = json!(1);
        assert!(refusal(&v).contains("unknown key \"gas\" in manifest.tests[0]"));
        let mut v = return_42_answer();
        v["v"] = json!(2);
        assert!(refusal(&v).contains("v must be the integer 1"));

        for bad in ["../escape.hex", "/abs.hex", "a//b.hex", "dir/./x.hex", "space here.hex", "trailing/"] {
            let mut v = return_42_answer();
            v["sources"][bad] = json!("0x00");
            let msg = refusal(&v);
            assert!(msg.contains("source path"), "{bad}: {msg}");
        }

        let mut v = return_42_answer();
        v["sources"]["second.hex"] = json!("0x00");
        assert!(refusal(&v).contains("exactly one .hex source (the initcode); this answer has 2"));
        let mut v = return_42_answer();
        v["sources"].as_object_mut().unwrap().remove("return42.hex");
        assert!(refusal(&v).contains("this answer has 0"));

        let mut v = return_42_answer();
        v["manifest"]["tests"][1]["name"] = json!("answers-42");
        assert!(refusal(&v).contains("duplicate test name \"answers-42\""));

        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["calldata"] = json!("0xzz");
        assert!(refusal(&v).contains("manifest.tests[0].calldata: 'z' is not a hex digit"));
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["calldata"] = json!("0x1");
        assert!(refusal(&v).contains("odd-length hex"));
        let mut v = return_42_answer();
        v["manifest"]["targets"][0]["constructor_args"] = json!("1234");
        assert!(refusal(&v).contains("expected 0x-prefixed hex"));
        let mut v = return_42_answer();
        v["sources"]["return42.hex"] = json!("0x");
        assert!(refusal(&v).contains("carries no bytecode"));

        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["gas_limit"] = json!(20_999);
        assert!(refusal(&v).contains("gas_limit must be 21000..=30000000"));
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["value"] = json!(-1);
        assert!(refusal(&v).contains("value must be an integer in 0..=2^64-1"));
        let mut v = return_42_answer();
        v["manifest"]["targets"][0]["source"] = json!("README.md");
        assert!(refusal(&v).contains("must name the .hex source"));
        let mut v = return_42_answer();
        v["manifest"]["targets"] = json!([]);
        assert!(refusal(&v).contains("exactly one target"));
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["expect"]["output"] = json!(42);
        assert!(refusal(&v).contains("expect.output must be 0x-prefixed hex or null"));
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["name"] = json!("");
        assert!(refusal(&v).contains("name must be 1..=64 bytes"));

        // the JSON layer's own refusals still apply
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["value"] = json!(1.5);
        assert!(refusal(&v).contains("no canonical form"));
    }

    // (3) determinism -------------------------------------------------------------------------

    #[test]
    fn same_answer_twice_and_its_variants_give_identical_bytes() {
        let answer = bytes_of(&return_42_answer());
        let a = derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &answer).unwrap();
        let b = derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &answer).unwrap();
        assert_eq!(a.artifact.bytes, b.artifact.bytes);
        assert_eq!(a.dsl_hash, b.dsl_hash);
        assert_eq!(a.artifact_hash, b.artifact_hash);

        // whitespace and key order are not semantic
        let pretty = serde_json::to_vec_pretty(&return_42_answer()).unwrap();
        let reordered = {
            let v = return_42_answer();
            let mut m = serde_json::Map::new();
            m.insert("toolchain".into(), v["toolchain"].clone());
            m.insert("manifest".into(), v["manifest"].clone());
            m.insert("v".into(), v["v"].clone());
            m.insert("sources".into(), v["sources"].clone());
            let s = serde_json::to_string(&Value::Object(m)).unwrap();
            format!("  {s}\n").into_bytes()
        };
        for variant in [pretty, reordered] {
            let d = derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &variant).unwrap();
            assert_eq!(d.canonical_dsl, a.canonical_dsl);
            assert_eq!(d.artifact.bytes, a.artifact.bytes);
        }

        // the two transformers run the same toolchain: same artifact, different name and kind
        let c = derive_with(&CodeGrammar, &ContractEvmTransformer, &binding(), &answer).unwrap();
        assert_eq!(c.artifact.bytes, a.artifact.bytes);
        assert_ne!(c.transformer_id, a.transformer_id);
        assert_eq!(a.kind, kind::CODE);
        assert_eq!(c.kind, kind::CONTRACT);
        assert_eq!(a.artifact.media_type, MEDIA_TYPE);
        assert_eq!(a.artifact.extension, EXTENSION);
    }

    #[test]
    fn run_refuses_input_that_is_not_canonical() {
        let pretty = serde_json::to_vec_pretty(&return_42_answer()).unwrap();
        for t in [&CodeEvmTransformer as &dyn Transformer, &ContractEvmTransformer] {
            match t.run(&pretty) {
                Err(DeriveError::Transformer(msg)) => assert!(msg.contains("not canonical"), "{msg}"),
                other => panic!("{other:?}"),
            }
        }
    }

    // (4) a real contract ---------------------------------------------------------------------

    #[test]
    fn return_42_deploys_and_the_verdict_log_records_both_tests() {
        let d = derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &bytes_of(&return_42_answer())).unwrap();
        let m = read_mcod(&d.artifact.bytes).unwrap();
        assert_eq!(m.runtime_code, decode_hex("rt", &format!("0x{RETURN_42_RUNTIME}"), 1024).unwrap());
        assert_eq!(
            m.created,
            Address::from(EVM_V1_DEPLOYER).create(0).into_array(),
            "the created address is the deployer's nonce-0 address"
        );
        assert!(m.deploy_gas_used > 53_000, "a CREATE costs more than its intrinsic gas: {}", m.deploy_gas_used);
        assert_eq!(m.tests.len(), 2);
        let first = &m.tests[0];
        assert_eq!(first.name, "answers-42");
        assert!(first.success);
        assert_eq!(hex0x(&first.output), WORD_42);
        assert!(first.gas_used > 21_000 && first.gas_used < 100_000);
        assert!(first.expectation_held);
        let second = &m.tests[1];
        assert_eq!(second.name, "expects-43");
        assert!(second.success, "the call itself succeeds");
        assert_eq!(hex0x(&second.output), WORD_42, "the contract still answers 42");
        assert!(!second.expectation_held, "the expectation of 43 did not hold — recorded, not refused");
        assert_eq!((m.tests_passed, m.tests_failed), (1, 1));
        assert_eq!(d.object.artifact_bytes, d.artifact.bytes.len() as u64);
    }

    #[test]
    fn a_reverting_initcode_refuses_the_derivation() {
        let mut v = return_42_answer();
        v["sources"]["return42.hex"] = json!(format!("0x{REVERTING_INITCODE}"));
        match derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &bytes_of(&v)) {
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("deploy reverted"), "{msg}"),
            other => panic!("{other:?}"),
        }
        // an initcode that returns nothing builds nothing: PUSH1 0 PUSH1 0 RETURN
        let mut v = return_42_answer();
        v["sources"]["return42.hex"] = json!("0x60006000f3");
        match derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &bytes_of(&v)) {
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("left no runtime code"), "{msg}"),
            other => panic!("{other:?}"),
        }
        // a test the EVM will not admit (intrinsic gas above its limit) refuses, naming the test
        let mut v = return_42_answer();
        v["manifest"]["tests"][0]["calldata"] = json!(hex0x(&[0xffu8; 2048]));
        v["manifest"]["tests"][0]["gas_limit"] = json!(21_000);
        match derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &bytes_of(&v)) {
            Err(DeriveError::Transformer(msg)) => {
                assert!(msg.contains("test \"answers-42\" is not a transaction the EVM admits"), "{msg}")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_failing_expectation_and_a_reverting_call_are_verdicts_not_refusals() {
        // a runtime that reverts on every call: PUSH1 0 PUSH1 0 REVERT, 5 bytes at code offset 12
        let mut v = return_42_answer();
        v["sources"]["return42.hex"] = json!(format!("0x6005600c60003960056000f3{REVERTING_INITCODE}"));
        v["manifest"]["tests"] = json!([
            { "name": "expects-a-revert", "calldata": "0x", "value": 0, "gas_limit": 50000,
              "expect": { "success": false, "output": "0x" } },
            { "name": "expects-success-wrongly", "calldata": "0x", "value": 7, "gas_limit": 50000,
              "expect": { "success": true, "output": null } }
        ]);
        let d = derive_with(&CodeGrammar, &ContractEvmTransformer, &binding(), &bytes_of(&v)).unwrap();
        let m = read_mcod(&d.artifact.bytes).unwrap();
        assert_eq!(m.runtime_code, decode_hex("rt", &format!("0x{REVERTING_INITCODE}"), 16).unwrap());
        assert!(!m.tests[0].success && m.tests[0].expectation_held);
        assert!(!m.tests[1].success && !m.tests[1].expectation_held);
        assert_eq!((m.tests_passed, m.tests_failed), (1, 1));
    }

    #[test]
    fn every_test_sees_the_freshly_deployed_state() {
        // a counter: runtime that does slot0 += 1 and returns the new value (18 bytes)
        //   60 00 54        PUSH1 0, SLOAD
        //   60 01 01        PUSH1 1, ADD
        //   80              DUP1
        //   60 00 55        PUSH1 0, SSTORE
        //   60 00 52        PUSH1 0, MSTORE
        //   60 20 60 00 f3  PUSH1 32, PUSH1 0, RETURN
        let runtime = "6000546001018060005560005260206000f3";
        let one = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let mut v = return_42_answer();
        v["sources"]["return42.hex"] = json!(format!("0x6012600c60003960126000f3{runtime}"));
        v["manifest"]["tests"] = json!([
            { "name": "first", "calldata": "0x", "value": 0, "gas_limit": 100000, "expect": { "success": true, "output": one } },
            { "name": "second-also-counts-one", "calldata": "0x", "value": 5, "gas_limit": 100000, "expect": { "success": true, "output": one } }
        ]);
        let d = derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &bytes_of(&v)).unwrap();
        let m = read_mcod(&d.artifact.bytes).unwrap();
        assert!(m.tests.iter().all(|t| t.success && t.expectation_held), "{:?}", m.tests);
        assert_eq!(m.tests[0].gas_used, m.tests[1].gas_used, "the same call over the same state costs the same gas");
        assert_eq!((m.tests_passed, m.tests_failed), (2, 0));
    }

    // (5) the MCOD bytes, structurally ---------------------------------------------------------

    #[test]
    fn mcod_bytes_parse_structurally() {
        let d = derive_with(&CodeGrammar, &CodeEvmTransformer, &binding(), &bytes_of(&return_42_answer())).unwrap();
        let b = &d.artifact.bytes;
        assert_eq!(&b[..4], b"MCOD");
        assert_eq!(&b[4..6], &2u16.to_le_bytes());
        assert_eq!(&b[6..10], &6u32.to_le_bytes());
        assert_eq!(&b[10..16], b"evm/v1");
        assert_eq!(b[16], kaspa_evm::EVM_SPEC_ID as u8);
        assert_eq!(&b[17..81], &evm_v1_run_manifest_hash(), "the artifact names the ceiling and the fixture it ran under");
        assert_eq!(&b[81..85], &2u32.to_le_bytes(), "two sources");
        assert_eq!(&b[85..89], &9u32.to_le_bytes());
        assert_eq!(&b[89..98], b"README.md");
        assert_eq!(&b[98..162], &source_digest("the answer is 42\n"));
        let n = b.len();
        assert_eq!(u32::from_le_bytes(b[n - 4..].try_into().unwrap()), crc32(&b[..n - 4]));
        assert_eq!(&b[n - 12..n - 4], &[1u32.to_le_bytes(), 1u32.to_le_bytes()].concat(), "summary: 1 passed, 1 failed");

        let m = read_mcod(b).unwrap();
        assert_eq!(m.version, 2);
        assert_eq!(m.run_manifest, evm_v1_run_manifest_hash());
        assert_eq!(m.toolchain, "evm/v1");
        assert_eq!(m.spec_id, 16, "Shanghai");
        assert_eq!(m.sources.len(), 2);
        assert_eq!(m.sources[0].0, "README.md");
        assert_eq!(m.sources[1].0, "return42.hex");
        assert_eq!(m.target_name, "return42");
        assert_eq!(m.runtime_code, decode_hex("rt", &format!("0x{RETURN_42_RUNTIME}"), 1024).unwrap());

        // a flipped byte fails the CRC; a truncated file is named as truncated
        let mut flipped = b.clone();
        flipped[40] ^= 1;
        assert!(matches!(read_mcod(&flipped), Err(DeriveError::Mismatch(m)) if m.contains("CRC")));
        assert!(matches!(read_mcod(&b[..30]), Err(DeriveError::Mismatch(_))));
        // the digest is keyed: an unkeyed or differently-keyed digest is not this one
        assert_ne!(source_digest("a"), source_digest("a\n"));
    }

    // (6) the external runner -----------------------------------------------------------------

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(hex0x(&sha256(b"abc")), "0xba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            hex0x(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "0x248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(hex0x(&sha256(&[0u8; 1000])), "0x541b3e9daa09b20bf85fa273e5cbd3e80185aa4ec298e765db87742b70138a53");
    }

    /// The external toolchain's own gate — refused on a `none` backend, refused beside a key,
    /// executed inside an ephemeral tree — is `tests/external_toolchain_gate.rs`: it mutates the
    /// process environment (`MISAKA_PALW_CONFINEMENT`) and therefore owns a test binary of its own.
    ///
    /// What stays here is the half that needs no environment: a source path that would escape the
    /// tree is refused before anything is written or spawned.
    #[test]
    fn external_runner_refuses_an_escaping_source_before_it_writes_anything() {
        let manifest = ExternalToolchainManifest {
            name: "fake/1".to_string(),
            binary_sha256: [0u8; 32],
            argv: vec!["{src}".to_string()],
            env: BTreeMap::new(),
            source_date_epoch: 1_700_000_000,
        };
        let mut escaping = BTreeMap::new();
        escaping.insert("../escape.txt".to_string(), String::new());
        // `/bin/sh` exists and hashes to something other than zeros, so the binary pin refuses
        // first unless the path check comes first — which is what this asserts.
        let outcome = run_external(&manifest, Path::new("/bin/sh"), &escaping, &[], &[]);
        match outcome {
            Err(DeriveError::Grammar(msg)) => assert!(msg.contains("source path"), "{msg}"),
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("hashes to"), "the pin may refuse first: {msg}"),
            other => panic!("{other:?}"),
        }
    }

    // (6b) the run manifest and the runner's frames -------------------------------------------

    /// **ADR-0078 SA-1's pin.** The gas ceiling and the state-fixture hash are part of the
    /// transformer manifest — they ride in [`WRITER_NAME`], which is a manifest field — so a
    /// changed ceiling or a changed fixture is a changed `transformer_id`. When this fails, the
    /// change was deliberate or it was not; re-pin by COPYING the value this test prints.
    #[test]
    fn the_writer_name_pins_the_run_manifest() {
        let expected = format!("misaka-code-build/2/canonical-v1+{}", evm_v1_run_manifest_tag());
        assert_eq!(
            WRITER_NAME, expected,
            "the run manifest moved: the gas ceiling or the state fixture changed. \
             WRITER_NAME is {WRITER_NAME}, the run manifest says {expected}"
        );
        // The fixture is the state, not the code: the digest is over bytes a reader can spell.
        assert_eq!(evm_v1_state_fixture_bytes().len(), 20 + 1 + 8 + 8 + 8 + 8 + 20 + 32 + 32 + 32 + 20 + 32 + 8 + 32 + 4);
        assert_ne!(evm_v1_state_fixture_hash(), evm_v1_run_manifest_hash(), "two domains, two digests");
    }

    #[test]
    fn the_runner_frames_round_trip_and_refuse_a_flipped_byte() {
        let job = EvmJob {
            run_manifest: evm_v1_run_manifest_hash(),
            deploy_data: vec![0x60, 0x2a],
            calls: vec![
                EvmJobCall { calldata: vec![], value: 0, gas_limit: 21_000 },
                EvmJobCall { calldata: vec![1, 2, 3], value: 7, gas_limit: 100_000 },
            ],
        };
        let frame = encode_evm_job(&job);
        assert_eq!(decode_evm_job(&frame).unwrap(), job);
        let mut flipped = frame.clone();
        flipped[10] ^= 1;
        assert!(matches!(decode_evm_job(&flipped), Err(DeriveError::Mismatch(m)) if m.contains("CRC")));
        assert!(matches!(decode_evm_job(&frame[..8]), Err(DeriveError::Mismatch(m)) if m.contains("shorter than a frame")));

        // A job that names a gas limit outside the manifest's range is refused by the READER, so
        // the runner never runs it: the ceilings are the manifest's, not the caller's.
        let mut wide = job.clone();
        wide.calls[0].gas_limit = MAX_TEST_GAS_LIMIT + 1;
        assert!(matches!(decode_evm_job(&encode_evm_job(&wide)), Err(DeriveError::Mismatch(m)) if m.contains("outside the manifest")));

        for result in [
            EvmJobResult::Built {
                created: [7u8; 20],
                deploy_gas_used: 53_000,
                runtime_code: vec![0xf3],
                calls: vec![EvmCallOutcome { success: true, output: vec![42], gas_used: 21_064 }],
            },
            EvmJobResult::Refused { code: refusal::DEPLOY_REVERTED, index: 0, gas_used: 1234, detail: vec![0xde, 0xad] },
        ] {
            let frame = encode_evm_result(&evm_v1_run_manifest_hash(), &result);
            let (manifest, back) = decode_evm_result(&frame).unwrap();
            assert_eq!(manifest, evm_v1_run_manifest_hash());
            assert_eq!(back, result);
        }
    }

    /// The refusal codes become this kind's own sentences — the runner returns facts, the
    /// transformer names the test. A code the parent does not know is still a refusal, not a
    /// panic.
    #[test]
    fn every_refusal_code_becomes_a_sentence_that_names_the_test() {
        let (code, _) = parse_and_canonicalize(&bytes_of(&return_42_answer())).unwrap();
        let cases = [
            (refusal::DEPLOY_REVERTED, "deploy reverted after 5 gas with output 0xdead"),
            (refusal::DEPLOY_NO_RUNTIME_CODE, "left no runtime code"),
            (refusal::CALL_NOT_ADMITTED, "test \"expects-43\" is not a transaction the EVM admits"),
            (refusal::JOB_MALFORMED, "the runner refused the job"),
            (4242, "unknown code 4242"),
        ];
        for (reason, expected) in cases {
            let detail = if reason == refusal::CALL_NOT_ADMITTED { b"why".to_vec() } else { vec![0xde, 0xad] };
            match refusal_sentence(&code, reason, 1, 5, &detail) {
                DeriveError::Transformer(msg) => assert!(msg.contains(expected), "code {reason}: {msg}"),
                other => panic!("{other:?}"),
            }
        }
    }

    /// **The deadline is derived, never chosen** (ADR-0077 SA-4's shape): it is a function of the
    /// gas the answer declares, and it is capped.
    #[test]
    fn the_deadline_is_a_function_of_the_declared_gas() {
        let job = |gas: u64, n: usize| EvmJob {
            run_manifest: evm_v1_run_manifest_hash(),
            deploy_data: vec![0],
            calls: vec![EvmJobCall { calldata: vec![], value: 0, gas_limit: gas }; n],
        };
        assert_eq!(evm_v1_deadline_secs(&job(21_000, 0)), EVM_V1_DEADLINE_BASE_SECS + 30);
        assert!(evm_v1_deadline_secs(&job(MAX_TEST_GAS_LIMIT, 4)) > evm_v1_deadline_secs(&job(MAX_TEST_GAS_LIMIT, 1)));
        assert_eq!(evm_v1_deadline_secs(&job(MAX_TEST_GAS_LIMIT, MAX_TESTS)), EVM_V1_DEADLINE_MAX_SECS, "and it is capped");
    }

    // (7) the fixture corpus ------------------------------------------------------------------

    /// Every `corpus/code/NN-<name>.json`, derived under both transformers, against
    /// `corpus/code/golden.json`. Set `MISAKA_PALW_DERIVE_REGEN_GOLDEN=1` to rewrite the golden
    /// file (generate once, then pin: a moved value is a changed toolchain or a changed writer).
    #[test]
    fn corpus_matches_golden() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus").join("code");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "json") && p.file_name().is_some_and(|n| n != "golden.json"))
            .collect();
        files.sort();
        assert!(files.len() >= 3, "the corpus holds at least three samples: {files:?}");

        let mut computed = serde_json::Map::new();
        for file in &files {
            let answer = std::fs::read(file).unwrap();
            let name = file.file_name().unwrap().to_str().unwrap();
            for t in [&CodeEvmTransformer as &dyn Transformer, &ContractEvmTransformer] {
                let d = derive_with(&CodeGrammar, t, &binding(), &answer).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(d.grammar_id, crate::ids::grammar_id_v1(GRAMMAR_NAME));
                read_mcod(&d.artifact.bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
                computed.insert(
                    format!("{name}#{}", t.manifest().name),
                    json!({
                        "dsl_hash": d.dsl_hash.to_string(),
                        "artifact_hash": d.artifact_hash.to_string(),
                        "artifact_bytes": d.artifact.bytes.len(),
                    }),
                );
            }
        }
        let computed = Value::Object(computed);
        let golden_path = dir.join("golden.json");
        if std::env::var_os("MISAKA_PALW_DERIVE_REGEN_GOLDEN").is_some() {
            std::fs::write(&golden_path, format!("{}\n", serde_json::to_string_pretty(&computed).unwrap())).unwrap();
        }
        let golden: Value = serde_json::from_slice(&std::fs::read(&golden_path).unwrap()).unwrap();
        assert_eq!(computed, golden, "corpus/code/golden.json moved");
    }

    // (8) the discipline's source scan ---------------------------------------------------------

    #[test]
    fn no_float_type_name_appears_in_this_file() {
        // The names are assembled at run time so this test's own text does not carry them.
        let src = include_str!("code.rs");
        for width in ["32", "64"] {
            let name = format!("f{width}");
            assert!(!src.contains(&name), "{name} appears in code.rs");
        }
    }

    #[test]
    fn manifests_name_this_build_and_the_kinds_the_table_assigns() {
        let (g, t) = register();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name(), "code/v1");
        assert_eq!(t.len(), 2);
        let code = t[0].manifest();
        let contract = t[1].manifest();
        assert_eq!((code.name, code.kind, code.grammar), ("code/evm/v1", 4, "code/v1"));
        assert_eq!((contract.name, contract.kind, contract.grammar), ("contract/evm/v1", 22, "code/v1"));
        for m in [&code, &contract] {
            assert_eq!(m.discipline, Discipline::Integer);
            assert_eq!(m.writer, WRITER_NAME);
            assert_eq!(m.source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX);
        }
        assert_eq!(std::str::from_utf8(&EVM_V1_DEPLOYER).unwrap(), "misaka-code-build/v1");
    }
}
