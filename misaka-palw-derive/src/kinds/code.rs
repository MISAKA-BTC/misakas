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
use revm::primitives::{AccountInfo, Address, B256, Bytes, ExecutionResult, KECCAK_EMPTY, Output, TxEnv, TxKind, U256};
use revm::{
    Database, Evm,
    db::{CacheDB, EmptyDB},
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------------------------
// Names and bounds
// ---------------------------------------------------------------------------------------------

/// The grammar both transformers consume.
pub const GRAMMAR_NAME: &str = "code/v1";
/// The only toolchain this build names (ADR-0078 Decision 11).
pub const TOOLCHAIN_EVM_V1: &str = "evm/v1";
pub const CODE_TRANSFORMER_NAME: &str = "code/evm/v1";
pub const CONTRACT_TRANSFORMER_NAME: &str = "contract/evm/v1";
/// The artifact's canonical writer, as the manifests name it.
pub const WRITER_NAME: &str = "misaka-code-build/1/canonical-v1";
pub const MEDIA_TYPE: &str = "application/vnd.misaka.code-build";
pub const EXTENSION: &str = "mcod";

pub const MCOD_MAGIC: &[u8; 4] = b"MCOD";
pub const MCOD_VERSION: u16 = 1;
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
    let build = build_evm_v1(&code)?;
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
// A fresh `CacheDB<EmptyDB>`, the fixed environment above, the fixed deployer at nonce 0.
// DEPLOY: `TxKind::Create` with data = initcode ‖ constructor_args, the deploy gas limit, gas
// price 0, committed. A deploy that reverts, halts, creates nothing or leaves no runtime code
// refuses the whole derivation, naming it. TESTS, in manifest order: `TxKind::Call(created)`
// from the deployer with the test's calldata, value and gas limit, executed WITHOUT commit —
// so EVERY TEST SEES THE FRESHLY DEPLOYED STATE and no test can depend on another's order or
// side effects; a test that needs prior state carries it in its own calldata or in the
// constructor. Recorded per test: success, output bytes, gas used, and whether the expectation
// held (`expect.success == success`, and byte equality with `expect.output` when it is not null).

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

/// Run the toolchain on a validated answer.
pub fn build_evm_v1(code: &CodeDsl) -> Result<EvmBuild, DeriveError> {
    let mut deploy_data = decode_initcode(&code.target.source, &code.sources[&code.target.source])?;
    deploy_data.extend_from_slice(&code.target.constructor_args);

    let deployer = Address::from(EVM_V1_DEPLOYER);
    let mut db = CacheDB::new(EmptyDB::default());
    db.insert_account_info(deployer, AccountInfo { balance: U256::from(u128::MAX), nonce: 0, code_hash: KECCAK_EMPTY, code: None });

    let deploy = {
        let mut evm = evm_v1(&mut db, tx_env(deployer, TxKind::Create, deploy_data, 0, EVM_V1_DEPLOY_GAS_LIMIT, 0));
        evm.transact_commit()
            .map_err(|e| transformer(format!("{TOOLCHAIN_EVM_V1} deploy is not a transaction the EVM admits: {e:?}")))?
    };
    let (created, deploy_gas_used) = match deploy {
        ExecutionResult::Success { output: Output::Create(_, Some(address)), gas_used, .. } => (address, gas_used),
        ExecutionResult::Success { .. } => {
            return Err(transformer(format!("{TOOLCHAIN_EVM_V1} deploy succeeded but created no account")));
        }
        ExecutionResult::Revert { gas_used, output } => {
            return Err(transformer(format!(
                "{TOOLCHAIN_EVM_V1} deploy reverted after {gas_used} gas with output {}",
                hex0x(&output)
            )));
        }
        ExecutionResult::Halt { reason, gas_used } => {
            return Err(transformer(format!("{TOOLCHAIN_EVM_V1} deploy halted ({reason:?}) after {gas_used} gas")));
        }
    };
    let info = db.basic(created).ok().flatten().ok_or_else(|| {
        transformer(format!("{TOOLCHAIN_EVM_V1} deploy named {} but the state holds no such account", hex0x(created.as_slice())))
    })?;
    let runtime_code = if info.code_hash == KECCAK_EMPTY {
        Vec::new()
    } else {
        db.code_by_hash(info.code_hash)
            .map_err(|e| transformer(format!("{TOOLCHAIN_EVM_V1}: the created account's code is unreadable: {e:?}")))?
            .original_bytes()
            .to_vec()
    };
    if runtime_code.is_empty() {
        return Err(transformer(format!(
            "{TOOLCHAIN_EVM_V1} deploy left no runtime code at {}: an initcode that returns nothing builds nothing",
            hex0x(created.as_slice())
        )));
    }

    let mut tests = Vec::with_capacity(code.tests.len());
    for t in &code.tests {
        let mut evm = evm_v1(&mut db, tx_env(deployer, TxKind::Call(created), t.calldata.clone(), t.value, t.gas_limit, 1));
        let outcome = evm
            .transact()
            .map_err(|e| transformer(format!("{TOOLCHAIN_EVM_V1} test {:?} is not a transaction the EVM admits: {e:?}", t.name)))?;
        let (success, output, gas_used) = match outcome.result {
            ExecutionResult::Success { output, gas_used, .. } => (true, output.into_data().to_vec(), gas_used),
            ExecutionResult::Revert { output, gas_used } => (false, output.to_vec(), gas_used),
            ExecutionResult::Halt { gas_used, .. } => (false, Vec::new(), gas_used),
        };
        let expectation_held = t.expect_success == success && t.expect_output.as_ref().is_none_or(|want| *want == output);
        tests.push(TestOutcome { name: t.name.clone(), success, output, gas_used, expectation_held });
    }

    Ok(EvmBuild { created: created.into_array(), deploy_gas_used, runtime_code, tests })
}

// ---------------------------------------------------------------------------------------------
// The artifact: `MCOD` version 1
// ---------------------------------------------------------------------------------------------
//
// Little-endian throughout; a string or byte string is a `u32` length then the bytes.
//
// ```text
// HEADER    magic b"MCOD" · u16 version = 1 · string toolchain · u8 spec id (EVM_SPEC_ID as u8)
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
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, what: &str) -> Result<&'a [u8], DeriveError> {
        let end = self.pos.checked_add(n).filter(|end| *end <= self.bytes.len());
        let end = end.ok_or_else(|| DeriveError::Mismatch(format!("mcod: truncated at {what} (offset {})", self.pos)))?;
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
        String::from_utf8(self.bytes(what)?).map_err(|_| DeriveError::Mismatch(format!("mcod: {what} is not UTF-8")))
    }
    fn flag(&mut self, what: &str) -> Result<bool, DeriveError> {
        match self.u8(what)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(DeriveError::Mismatch(format!("mcod: {what} is {other}, not 0 or 1"))),
        }
    }
}

/// Read an `MCOD` artifact, checking the trailer's CRC first and every bound on the way.
pub fn read_mcod(bytes: &[u8]) -> Result<McodFile, DeriveError> {
    if bytes.len() < 4 + 2 + 4 + 1 + 4 {
        return Err(DeriveError::Mismatch("mcod: shorter than a header".into()));
    }
    let (body, trailer) = bytes.split_at(bytes.len() - 4);
    let want = u32::from_le_bytes(trailer.try_into().expect("4 bytes"));
    if crc32(body) != want {
        return Err(DeriveError::Mismatch("mcod: the trailer's CRC-32 does not match the bytes".into()));
    }
    let mut c = Cursor { bytes: body, pos: 0 };
    if c.take(4, "magic")? != MCOD_MAGIC {
        return Err(DeriveError::Mismatch("mcod: magic is not MCOD".into()));
    }
    let version = c.u16("version")?;
    if version != MCOD_VERSION {
        return Err(DeriveError::Mismatch(format!("mcod: version {version} is not {MCOD_VERSION}")));
    }
    let toolchain = c.string("toolchain")?;
    let spec_id = c.u8("spec id")?;
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
// What the runner CANNOT fix, stated plainly: network isolation and the clock. A process
// environment cannot deny a socket or freeze time; `SOURCE_DATE_EPOCH` is a convention a
// toolchain honours, not a wall the runner builds. Those are the fleet drill host's (sandbox)
// responsibility — a network namespace with no routes, a pinned clock — and the manifest's `env`
// cannot provide them. A toolchain that reaches for either is excluded by the drill, not by
// this function.

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
/// clock, no randomness.
fn fresh_work_dir(tag: &str) -> Result<PathBuf, DeriveError> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for _ in 0..1024 {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("misaka-code-build-{tag}-{pid}-{n}"));
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
/// output directory), sorted by name. See the module section above for what is and is not fixed.
pub fn run_external(
    manifest: &ExternalToolchainManifest,
    binary: &Path,
    sources: &BTreeMap<String, String>,
    out_collect: &[String],
) -> Result<Vec<(String, Vec<u8>)>, DeriveError> {
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

    let output = std::process::Command::new(binary)
        .args(&argv)
        .env_clear()
        .envs(&manifest.env)
        .env("SOURCE_DATE_EPOCH", manifest.source_date_epoch.to_string())
        .env("TZ", "UTC")
        .env("LC_ALL", "C")
        .current_dir(work)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| io("spawn", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let shown: String = stderr.chars().take(2048).collect();
        return Err(transformer(format!("external toolchain {}: exited with {}: {shown}", manifest.name, output.status)));
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
        assert_eq!(&b[4..6], &1u16.to_le_bytes());
        assert_eq!(&b[6..10], &6u32.to_le_bytes());
        assert_eq!(&b[10..16], b"evm/v1");
        assert_eq!(b[16], kaspa_evm::EVM_SPEC_ID as u8);
        assert_eq!(&b[17..21], &2u32.to_le_bytes(), "two sources");
        assert_eq!(&b[21..25], &9u32.to_le_bytes());
        assert_eq!(&b[25..34], b"README.md");
        assert_eq!(&b[34..98], &source_digest("the answer is 42\n"));
        let n = b.len();
        assert_eq!(u32::from_le_bytes(b[n - 4..].try_into().unwrap()), crc32(&b[..n - 4]));
        assert_eq!(&b[n - 12..n - 4], &[1u32.to_le_bytes(), 1u32.to_le_bytes()].concat(), "summary: 1 passed, 1 failed");

        let m = read_mcod(b).unwrap();
        assert_eq!(m.version, 1);
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

    #[cfg(unix)]
    #[test]
    fn external_runner_runs_a_pinned_fake_toolchain_and_refuses_a_wrong_hash() {
        use std::os::unix::fs::PermissionsExt;
        let dir = fresh_work_dir("test").unwrap();
        let script = dir.join("copy.sh");
        std::fs::write(&script, b"#!/bin/sh\ncat \"$1\" > \"$2\"\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let binary_sha256 = sha256(&std::fs::read(&script).unwrap());
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        let manifest = ExternalToolchainManifest {
            name: "fake-copy/1".to_string(),
            binary_sha256,
            argv: vec!["{src}/hello.txt".to_string(), "{out}/hello.out".to_string()],
            env,
            source_date_epoch: 1_700_000_000,
        };
        let mut sources = BTreeMap::new();
        sources.insert("hello.txt".to_string(), "hello, hermetic world\n".to_string());
        let outs = run_external(&manifest, &script, &sources, &["hello.out".to_string()]).unwrap();
        assert_eq!(outs, vec![("hello.out".to_string(), b"hello, hermetic world\n".to_vec())]);

        let wrong = ExternalToolchainManifest { binary_sha256: [0xAB; 32], ..manifest.clone() };
        match run_external(&wrong, &script, &sources, &["hello.out".to_string()]) {
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("hashes to") && msg.contains("the manifest names"), "{msg}"),
            other => panic!("{other:?}"),
        }
        // a missing output is named
        match run_external(&manifest, &script, &sources, &["missing.out".to_string()]) {
            Err(DeriveError::Transformer(msg)) => assert!(msg.contains("collect out/missing.out"), "{msg}"),
            other => panic!("{other:?}"),
        }
        // a source path that would escape is refused before anything is written
        let mut escaping = BTreeMap::new();
        escaping.insert("../escape.txt".to_string(), String::new());
        assert!(matches!(run_external(&manifest, &script, &escaping, &[]), Err(DeriveError::Grammar(_))));
        let _ = std::fs::remove_dir_all(&dir);
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
