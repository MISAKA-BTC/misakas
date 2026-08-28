//! ADR-0040 Decision A, made checkable: no IEEE-754 value and no `libm` symbol on the
//! `PALW-BASE-0` consensus path.
//!
//! # Why a source scan and not a lint
//!
//! Decision A is stated as a property of a *conforming implementation* — "a build that links
//! `libm` for this class is not a conforming implementation" — but nothing checked it. Rust has
//! no built-in lint that denies `f32`/`f64`, and clippy has none either, so the rule was held by
//! discipline alone. Discipline is exactly what fails at 3 a.m. six months from now, and the
//! failure is silent: a single `as f32` in a requantisation path still runs, still produces
//! plausible logits, and diverges from a second implementation only on the inputs nobody sampled
//! — which is the same as saying it diverges in court, after a bond is posted.
//!
//! So the check is a scan over the crate's own sources. It is cruder than a lint and it is also
//! the only thing available that runs in CI.
//!
//! # What is deliberately NOT scanned, and why that is not a loophole
//!
//! * `convert.rs` and `bin/qwen25-convert.rs` — the **offline** post-training quantisation
//!   pipeline. It reads float weights and computes the `(multiplier, shift)` pairs the class then
//!   uses, so float is its whole job. ADR-0040 Decision B pins those scales *at registration*:
//!   "there is no dynamic (per-inference) rescaling anywhere". The boundary is therefore real —
//!   what converts may use float; what executes may not — and this test is where the boundary is
//!   written down.
//! * `bin/base0-depth-sweep.rs` and `bin/base0-class-sizing.rs` — measurement tools. They report
//!   on the class, they are not the class.
//! * Everything after the file's `#[cfg(test)]` marker. Tests are allowed float precisely because
//!   several of them exist to demonstrate what float does: `rope.rs` checks the integer table
//!   against `f64` trigonometry, and `optimized.rs` shows an `f32` sum changing with block size.
//!   Refusing float there would delete the evidence for why the class is integer.
//!
//! # The guard must fail loudly when its target moves
//!
//! A scanner that silently passes because its paths no longer resolve is worse than no scanner:
//! it reports a property it did not check. So a missing file is a failure, and the expected file
//! set is enumerated rather than globbed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Library modules on the execution path. Enumerated, not globbed: a new module must be
/// classified by a human as "executes" or "converts", and adding one without touching this list
/// fails the completeness check below.
const CONSENSUS_PATH: &[&str] = &[
    "src/lib.rs",
    "src/artifact.rs",
    "src/backend.rs",
    "src/classes.rs",
    "src/engine.rs",
    "src/engine_a16.rs",
    "src/inventory.rs",
    "src/kernels.rs",
    "src/legs.rs",
    "src/mmap.rs",
    "src/optimized.rs",
    "src/produce.rs",
    "src/qwen36.rs",
    "src/qwen36_backend.rs",
    // The dense A16 tier's producer — same rule, same reason: an execution path may not
    // compute in floats, and this one derives its job, runs the engine and commits four roots.
    "src/qwen25_a16_backend.rs",
    // ADR-0049 Decision F/G, arrived with the canonical IR: the engine's op sequence is COMPILED
    // from `BASE0_LAYER_IR` and its operands are RESOLVED by name. Both are on the execution path
    // by construction — they decide what the engine performs and which bytes it reads — so
    // Decision A binds them exactly as it binds the engine they replaced.
    "src/plan.rs",
    "src/operands.rs",
    "src/rc.rs",
    "src/rope.rs",
    "src/tokenizer.rs",
];

/// Not executed by consensus, but they *state* the class's arithmetic: the KAT set publishes the
/// outputs a third party will implement against. A float here would not make a block invalid — it
/// would corrupt the artifact everyone else conforms to, which is worse, because nothing on the
/// execution path would ever notice.
const STATES_THE_ARITHMETIC: &[&str] = &["src/kat.rs", "src/bin/base0-kat.rs"];

/// Off the execution path, each with the reason it is off it. Present so that the union of this
/// list and [`CONSENSUS_PATH`] can be compared against what is actually on disk.
const EXEMPT: &[(&str, &str)] = &[
    ("src/convert.rs", "offline PTQ: float in, (multiplier, shift) out, frozen at registration"),
    ("src/gguf.rs", "offline checkpoint reader: the file it decodes is float, and it never executes"),
    ("src/lmstudio.rs", "offline checkpoint bridge: it re-encodes a GGUF's float weights as the HF checkpoint, and it never executes"),
    ("src/bin/qwen25-convert.rs", "offline PTQ driver"),
    ("src/bin/qwen36-convert.rs", "offline PTQ driver for the hybrid architecture"),
    ("src/bin/qwen36-run.rs", "the hybrid runtime's driver: it times itself, and a timer is float"),
    ("src/bin/qwen36-chat.rs", "the hybrid runtime's chat driver: tokenizer, template and a tokens-per-second figure"),
    ("src/qwen36_calibrate.rs", "offline PTQ: it turns measured ranges into (multiplier, shift) triples"),
    ("src/qwen36_reference.rs", "the hybrid graph in f32: it measures the checkpoint's ranges so the PTQ can pick scales"),
    ("src/reference.rs", "the float reference forward: it measures the checkpoint's ranges so the PTQ can pick scales"),
    ("src/bin/base0-depth-sweep.rs", "measurement tool"),
    ("src/bin/base0-class-sizing.rs", "measurement tool"),
    ("src/bin/palw-rc-genesis.rs", "genesis card generator"),
    ("src/bin/base0-chat.rs", "the runtime's front door: it times itself, and a timer is float"),
    ("examples/base0-throughput.rs", "measurement tool: it times the engine, it is not the engine"),
    ("examples/gguf-probe.rs", "offline checkpoint inspector"),
    (
        "examples/qwen25-gguf-fixture.rs",
        "dev fixture writer: it emits a GGUF for the LM Studio lane's smoke test, and executes nothing",
    ),
    ("examples/class-weight-report.rs", "measurement tool: it reports what a class would be worth, in floats, and executes nothing"),
];

/// The primitives themselves live in `consensus-core`, and they are the half of the class this
/// crate cannot vouch for on its own. Scanned through a relative path because they are the point:
/// a float appearing in `SRDHM` would be the most expensive possible place for one.
const CONSENSUS_CORE: &[&str] = &["palw_base0.rs", "palw_base0_ops.rs", "palw_base0_a16.rs", "palw_qwen36_ops.rs"];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The executable part of a line: string literal *contents* and `//` comments removed.
///
/// Both removals are load-bearing, and the second one only became obvious when the guard first
/// ran: `"Qwen/Qwen2.5-1.5B"` is a model name, and a scanner that reads `2.5` out of it reports a
/// float in a file that has none. A guard whose first output is a false positive gets switched
/// off, so the string contents go.
///
/// Raw strings (`r#"..."#`) are not tracked — none of the scanned files contain one, and the
/// `every_source_file_is_classified` test is what keeps that set from growing unnoticed.
fn code_only(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_string => i += 1,
            b'"' => {
                in_string = !in_string;
                out.push('"');
            }
            b'/' if !in_string && i + 1 < bytes.len() && bytes[i + 1] == b'/' => break,
            c if !in_string => out.push(c as char),
            _ => {}
        }
        i += 1;
    }
    out
}

/// A `f32`/`f64` occurrence as a whole word. `f32` inside `buf32` or `Hash64` is not a float.
fn has_float_type(code: &str) -> bool {
    for pattern in ["f32", "f64"] {
        let mut from = 0;
        while let Some(at) = code[from..].find(pattern) {
            let start = from + at;
            let end = start + pattern.len();
            let before_ok = start == 0 || !is_ident_char(code.as_bytes()[start - 1]);
            let after_ok = end == code.len() || !is_ident_char(code.as_bytes()[end]);
            if before_ok && after_ok {
                return true;
            }
            from = end;
        }
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The transcendentals ADR-0040 removed from the catalog. Written as method calls because that is
/// how they reach `libm` from Rust — `x.exp()` compiles to a call into the platform's math
/// library, and which platform's is exactly what the class refuses to depend on.
const LIBM_CALLS: &[&str] =
    &[".sqrt()", ".powf(", ".powi(", ".exp()", ".ln()", ".log2()", ".log10()", ".sin()", ".cos()", ".tan()", ".exp2()"];

/// A decimal or exponent float literal: a digit, then `.` or `e`/`E` with a sign, then a digit.
/// Tuple indices (`x.0`) do not match — the character before the dot must be a digit and `x` is
/// not. Ranges (`1..=4`) do not match either: the character after the first dot is another dot.
fn has_float_literal(code: &str) -> bool {
    let b = code.as_bytes();
    for i in 1..b.len().saturating_sub(1) {
        if !b[i - 1].is_ascii_digit() {
            continue;
        }
        if b[i] == b'.' && b[i + 1].is_ascii_digit() {
            return true;
        }
        if (b[i] == b'e' || b[i] == b'E') && i + 2 < b.len() && (b[i + 1] == b'-' || b[i + 1] == b'+') && b[i + 2].is_ascii_digit() {
            return true;
        }
    }
    false
}

struct Finding {
    file: String,
    line: usize,
    text: String,
    reason: &'static str,
}

/// Everything before the file's single trailing `#[cfg(test)]`, comments removed.
fn scan(path: &Path, label: &str) -> Vec<Finding> {
    let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "{label} could not be read ({e}). This guard enumerates its targets on purpose: a file \
             that moved must be re-classified here, not silently dropped from the scan."
        )
    });
    let mut findings = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        if raw.trim_start().starts_with("#[cfg(test)]") {
            break;
        }
        let code = code_only(raw);
        let reason = if has_float_type(&code) {
            Some("an IEEE-754 type (ADR-0040 Decision A)")
        } else if LIBM_CALLS.iter().any(|c| code.contains(c)) {
            Some("a libm call (ADR-0040 Decision A; ADR-0031 is superseded for this class by having none)")
        } else if has_float_literal(&code) {
            Some("a float literal (ADR-0040 Decision B: scales are (multiplier, shift), never a float)")
        } else {
            None
        };
        if let Some(reason) = reason {
            findings.push(Finding { file: label.to_string(), line: index + 1, text: raw.trim().to_string(), reason });
        }
    }
    findings
}

#[test]
fn the_execution_path_contains_no_float() {
    let root = crate_root();
    let mut findings = Vec::new();
    for relative in CONSENSUS_PATH.iter().chain(STATES_THE_ARITHMETIC.iter()) {
        findings.extend(scan(&root.join(relative), relative));
    }
    for name in CONSENSUS_CORE {
        let path = root.join("../consensus/core/src").join(name);
        findings.extend(scan(&path, &format!("consensus/core/src/{name}")));
    }
    assert!(
        findings.is_empty(),
        "ADR-0040 Decision A says integer-only means integer-only, and these lines are not:\n{}",
        findings.iter().map(|f| format!("  {}:{} — {}\n      {}", f.file, f.line, f.reason, f.text)).collect::<Vec<_>>().join("\n")
    );
}

/// The scan is only as good as its file list. A module added to `src/` that appears in neither
/// list is unclassified, and unclassified means unscanned — so it fails here rather than passing
/// quietly in the test above.
#[test]
fn every_source_file_is_classified() {
    let root = crate_root();
    let mut known: BTreeSet<String> = CONSENSUS_PATH.iter().chain(STATES_THE_ARITHMETIC.iter()).map(|s| s.to_string()).collect();
    known.extend(EXEMPT.iter().map(|(path, _)| path.to_string()));

    let mut on_disk = BTreeSet::new();
    for (directory, prefix) in [(root.join("src"), "src"), (root.join("src/bin"), "src/bin"), (root.join("examples"), "examples")] {
        for entry in std::fs::read_dir(&directory)
            .unwrap_or_else(|e| panic!("{} must exist for the classification to be complete: {e}", directory.display()))
        {
            let entry = entry.expect("a readable directory entry");
            if entry.path().extension().is_some_and(|e| e == "rs") {
                on_disk.insert(format!("{prefix}/{}", entry.file_name().to_string_lossy()));
            }
        }
    }

    let unclassified: Vec<_> = on_disk.difference(&known).cloned().collect();
    assert!(
        unclassified.is_empty(),
        "these sources are in none of CONSENSUS_PATH, STATES_THE_ARITHMETIC or EXEMPT, so nothing \
         decided whether ADR-0040 Decision A applies to them: {unclassified:?}"
    );
    let missing: Vec<_> = known.difference(&on_disk).cloned().collect();
    assert!(missing.is_empty(), "these are listed but no longer exist, so the guard is scanning nothing: {missing:?}");
}

/// The detectors, against the shapes that actually appear in this crate. Written because a guard
/// that cannot distinguish `f32` from `Hash64`, or a float literal from `1..=4`, would either be
/// ignored for noise or trusted while blind.
#[test]
fn the_detectors_separate_float_from_things_that_look_like_it() {
    assert!(has_float_type("let x = 1 as f32;"));
    assert!(has_float_type("fn f(v: f64) -> f64"));
    assert!(!has_float_type("let h: Hash64 = ...;"));
    assert!(!has_float_type("let b = buf32;"));
    assert!(!has_float_type("const MAX: u32 = 0;"));

    assert!(has_float_literal("let x = 1.0;"));
    assert!(has_float_literal("let e = 1e-8;"));
    assert!(!has_float_literal("for i in 1..=4 {"));
    assert!(!has_float_literal("let a = tuple.0;"));
    assert!(!has_float_literal("let n = 1_442_695_040;"));

    // The libm detector on its own: no `f32`/`f64` token appears on this line, so if the type
    // check were the only one firing, `x.exp()` would pass.
    let call = code_only("let y = x.exp();");
    assert!(LIBM_CALLS.iter().any(|c| call.contains(c)));
    assert!(!has_float_type(&call));

    assert_eq!(code_only("let x = 1; // 2.0 is fine in a comment"), "let x = 1; ");
    assert_eq!(code_only("//! a doc line with f64 in it"), "");
    // The literal that made this necessary: a model name is not an arithmetic constant.
    assert!(!has_float_literal(&code_only("let m = \"Qwen/Qwen2.5-1.5B\";")));
    assert!(!has_float_type(&code_only("let m = \"an f64 mentioned in a string\";")));
    // And a comment marker inside a string must not truncate the line before real code.
    assert!(has_float_literal(&code_only("let u = \"http://x\"; let x = 1.0;")));
}
