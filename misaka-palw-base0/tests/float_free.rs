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
    // The mmap container's profile interpreter (ADR-0067): it walks a REGISTERED declaration and
    // commits one row per node, so it is the execution path in its purest form — the declaration
    // is the program, and Decision A binds the walker exactly as it binds the compiled arms.
    "src/qwen36_plan.rs",
    // The dense A16 tier's producer — same rule, same reason: an execution path may not
    // compute in floats, and this one derives its job, runs the engine and commits four roots.
    "src/qwen25_a16_backend.rs",
    // The seat's recomputation of the state a checkpoint opens to. It is the execution path from
    // the other end: the producer commits these bytes and this reproduces them, so a float here
    // would put a seat and a producer on different arithmetic — the one disagreement the court
    // cannot adjudicate, because both sides would be "right" on their own hardware.
    "src/fp_recompute.rs",
    // The free-prompt worker (ADR-0067): it executes a caller's job and returns the roots a
    // commitment is assembled from. (`misaka-palw-serve`, which committed nothing, was retired by
    // ADR-0077 Decision 1: the server IS the worker now.)
    "src/bin/palw-a16-fp-worker.rs",
    // The runtime both family workers share (ADR-0077 Decision 1): it executes a caller's job,
    // retains the capture and returns the roots a commitment is assembled from — and the
    // hybrid's worker, the sparse capture, the interval openings and the certification tool
    // sit on the same path.
    "src/fp_worker.rs",
    "src/bin/palw-qwen36-fp-worker.rs",
    "src/fp_capture.rs",
    "src/fp_interval.rs",
    "src/bin/palw-certify.rs",
    // ADR-0049 Decision F/G, arrived with the canonical IR: the engine's op sequence is COMPILED
    // from `BASE0_LAYER_IR` and its operands are RESOLVED by name. Both are on the execution path
    // by construction — they decide what the engine performs and which bytes it reads — so
    // Decision A binds them exactly as it binds the engine they replaced.
    "src/plan.rs",
    "src/operands.rs",
    "src/rc.rs",
    "src/rope.rs",
    "src/tokenizer.rs",
    // The chat template (ADR-0077 Decision 6): it decides the SEGMENTS a prompt is made of, and
    // therefore the prompt ids, and `prompt_token_ids_hash_v2` over those ids is part of a job's
    // identity. Same argument as `tokenizer.rs` directly above, one step earlier in the same
    // pipeline — what it produces is committed.
    "src/chat_template.rs",
    // ADR-0067 Decision 5's fuzz gate. On the path by the same argument as `plan.rs`: it drives
    // the interpreter and ASSERTS that two runs of one plan are one bitstream, so a float here
    // would make the gate's own determinism claim unfalsifiable — the harness would inherit the
    // nondeterminism it exists to detect.
    "src/fuzz_a16.rs",
    // The mmap container's fuzz gate — the same argument, the same clause of the same ADR.
    "src/fuzz_qwen36.rs",
    // The end-to-end certification drill (ADR-0069). It runs no model arithmetic of its own, and
    // it is on this list anyway: its OUTPUT is committed. A certificate says which families this
    // build proved adjudicable, the set is hashed into the bundle's `court_e2e_root`, and that
    // value is part of `consensus_params_id` — so a float anywhere in deciding whether a planted
    // fault convicted would let two honest builds certify different sets, announce different
    // fingerprints and refuse each other at the handshake. That is the divergence Decision A
    // exists to make impossible, reached through a value rather than through a block.
    "src/e2e_drill.rs",
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
    ("src/bin/qwen25-convert.rs", "offline PTQ driver"),
    ("src/bin/qwen36-convert.rs", "offline PTQ driver for the hybrid architecture"),
    ("src/bin/qwen36-run.rs", "the hybrid runtime's driver: it times itself, and a timer is float"),
    ("src/qwen36_calibrate.rs", "offline PTQ: it turns measured ranges into (multiplier, shift) triples"),
    ("src/qwen36_reference.rs", "the hybrid graph in f32: it measures the checkpoint's ranges so the PTQ can pick scales"),
    ("src/reference.rs", "the float reference forward: it measures the checkpoint's ranges so the PTQ can pick scales"),
    ("src/bin/base0-depth-sweep.rs", "measurement tool"),
    ("src/bin/base0-class-sizing.rs", "measurement tool"),
    (
        "src/bin/palw-tile-measure.rs",
        "measurement tool: U-00's close sweep over derive_court_cost_shaped_v1; it prices, it computes no engine arithmetic",
    ),
    ("src/bin/palw-rc-genesis.rs", "genesis card generator"),
    // Off the path by CATEGORY, not because it holds a float — it holds none today. It drives
    // `A16Engine` over HTTP and its own module doc is explicit that a run served here is "a real
    // inference under a registered class and NOT yet" an adjudicable commitment: nothing it
    // produces is committed, so Decision A does not reach it. Listed rather than scanned so the
    // reason is on the record; if it ever starts building commitments it belongs in
    // CONSENSUS_PATH, and that move should be a deliberate edit here.
    // ADR-0067's saturation runner: it prints a tally and an elapsed time (a timer is float). The
    // arithmetic under test is `fuzz_a16.rs`, which is scanned.
    ("src/bin/palw-a16-profile-fuzz.rs", "fuzz driver: it times and tallies, the harness it drives is on the path"),
    ("src/bin/palw-qwen36-profile-fuzz.rs", "fuzz driver: it times and tallies, the harness it drives is on the path"),
    // ADR-0067 Decision 6 tier ②. It STATES arithmetic in the KAT sense — a publisher's rows
    // digest is what a third party conforms to — but it computes that digest by running the
    // scanned engine and hashing integers, adding none of its own. Listed with the reason rather
    // than scanned, because what it must not do is invent arithmetic, and it invents none.
    ("src/bin/palw-slice-kat.rs", "publishes a digest of the scanned engine's own integer rows; computes no arithmetic itself"),
    // The two model gates. They open a checkpoint, drive the SCANNED backends over a fixed case
    // list and write the answers to disk; they invent no arithmetic and commit nothing. Both were
    // unclassified until 2026-09-03 — `palw-model-gate.rs` since it landed — which is the state
    // this guard exists to refuse, so they are named here with the reason rather than left out.
    ("src/bin/palw-model-gate.rs", "model gate: drives the scanned dense backend and reports; commits nothing"),
    ("src/bin/palw-qwen36-model-gate.rs", "model gate: drives the scanned qwen36 backend and reports; commits nothing"),
    ("examples/base0-throughput.rs", "measurement tool: it times the engine, it is not the engine"),
    ("examples/gguf-probe.rs", "offline checkpoint inspector"),
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
/// **`in_string` is the CALLER's, because a Rust string does not end at the end of a line.** This
/// used to reset per line, and a `\`-continued message spanning several lines was therefore read
/// as bare code from its second line on — which is how this guard reported a float literal at
/// `palw-a16-fp-worker.rs:193`, where the "float" was the `2.5` inside `Qwen/Qwen2.5-1.5B` in the
/// middle of a `die(format!(…))` message.
///
/// The false positive is the visible half. The other half is worse and silent: on the line that
/// CLOSES such a string, the lone `"` flipped the state ON, so everything after it — real code,
/// on the execution path — was dropped from the scan. A guard that reports a violation it cannot
/// see is also skipping the ones it can.
///
/// Raw strings (`r#"..."#`) are still not tracked, and now they would desync the rest of the file
/// rather than one line, so [`scan`] refuses a file containing one instead of scanning it wrong.
fn code_only(line: &str, in_string: &mut bool) -> String {
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if *in_string => i += 1,
            b'"' => {
                *in_string = !*in_string;
                out.push('"');
            }
            b'/' if !*in_string && i + 1 < bytes.len() && bytes[i + 1] == b'/' => break,
            c if !*in_string => out.push(c as char),
            _ => {}
        }
        i += 1;
    }
    out
}

/// [`code_only`] for a line that starts outside a string — the single-line form the detector's own
/// tests use.
#[cfg(test)]
fn code_only_line(line: &str) -> String {
    code_only(line, &mut false)
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

/// Does this source open a raw string?
///
/// **`source.contains("r\"")` is not this question**, which is the whole reason it is a function:
/// every word ending in `r` before a closing quote — `"the executor"`, `"a carrier"` — contains
/// that pair, so the naive check refuses nearly every file in the crate for a raw string none of
/// them has. The `r` has to START a token.
fn has_raw_string(source: &str) -> bool {
    let b = source.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'r' || (i > 0 && is_ident_char(b[i - 1])) {
            continue;
        }
        let after_hashes = source[i + 1..].trim_start_matches('#');
        if after_hashes.starts_with('"') && after_hashes.len() < source.len() - i {
            return true;
        }
    }
    false
}

/// Everything before the file's single trailing `#[cfg(test)]`, comments removed.
fn scan(path: &Path, label: &str) -> (Vec<Finding>, usize, usize) {
    let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "{label} could not be read ({e}). This guard enumerates its targets on purpose: a file \
             that moved must be re-classified here, not silently dropped from the scan."
        )
    });
    // A raw string would flip `in_string` without a matching close and silently blind the scan
    // from there to the end of the file. Refusing is the only honest answer available here: the
    // alternative is a green result that means nothing.
    assert!(
        !has_raw_string(&source),
        "{label} contains a raw string, which `code_only` does not track — it would desync the \
         string state for the rest of the file and scan real code as prose (or skip it). Teach \
         `code_only` raw strings before adding a file that uses one."
    );
    let mut findings = Vec::new();
    let mut scanned = 0usize;
    let mut in_string = false;
    let mut skipping_tests = false;
    for (index, raw) in source.lines().enumerate() {
        // **A test module is SKIPPED, not treated as the end of the file — and it used to be
        // neither.**
        //
        // This read "everything before the file's single trailing `#[cfg(test)]`" and stopped at
        // the first line whose TRIMMED form began with it, so an INDENTED attribute on a
        // test-only helper inside a shipped `impl` ended the scan. `engine_a16.rs` has three
        // markers, the first at line 287 of 2,249: the guard read **12%** of a file it lists in
        // CONSENSUS_PATH — "the dense A16 engine, ADR-0040 Decision A's whole point" — and
        // reported the result as if it had read the file.
        //
        // Stopping at the first COLUMN-ZERO marker instead is still wrong, just less wrong: it
        // recovered 530 lines and left 1,432, because "the file's single trailing `#[cfg(test)]`"
        // is a premise this file does not satisfy — it has two top-level test modules, and code
        // that ships lives between them.
        //
        // So the rule is now what it should always have been: a `#[cfg(test)]` module is SKIPPED
        // — from its attribute to the closing brace at column zero — and the scan resumes after
        // it. Nothing about a test module makes the code below it untestable, and a guard that
        // treats "I found tests" as "the file is over" gets quieter every time someone adds one.
        if !in_string && raw.starts_with("#[cfg(test)]") {
            skipping_tests = true;
            continue;
        }
        if skipping_tests {
            // The module's closing brace, at column zero. Nothing else in this codebase's style
            // puts a bare `}` there.
            if raw == "}" {
                skipping_tests = false;
            }
            continue;
        }
        let code = code_only(raw, &mut in_string);
        let reason = if has_float_type(&code) {
            Some("an IEEE-754 type (ADR-0040 Decision A)")
        } else if LIBM_CALLS.iter().any(|c| code.contains(c)) {
            Some("a libm call (ADR-0040 Decision A; ADR-0031 is superseded for this class by having none)")
        } else if has_float_literal(&code) {
            Some("a float literal (ADR-0040 Decision B: scales are (multiplier, shift), never a float)")
        } else {
            None
        };
        scanned += 1;
        if let Some(reason) = reason {
            findings.push(Finding { file: label.to_string(), line: index + 1, text: raw.trim().to_string(), reason });
        }
    }
    (findings, scanned, source.lines().count())
}

#[test]
fn the_execution_path_contains_no_float() {
    let root = crate_root();
    let mut findings = Vec::new();
    let (mut scanned, mut total, mut files) = (0usize, 0usize, 0usize);
    for relative in CONSENSUS_PATH.iter().chain(STATES_THE_ARITHMETIC.iter()) {
        let (f, s, t) = scan(&root.join(relative), relative);
        findings.extend(f);
        scanned += s;
        total += t;
        files += 1;
    }
    for name in CONSENSUS_CORE {
        let path = root.join("../consensus/core/src").join(name);
        let (f, s, t) = scan(&path, &format!("consensus/core/src/{name}"));
        findings.extend(f);
        scanned += s;
        total += t;
        files += 1;
    }
    // **A checker that prints its verdict without printing its coverage is unfalsifiable** —
    // `ci-gates.sh`'s own second rule, applied to a guard that spent its whole life green while
    // reading 56% of what it claimed. The skipped remainder is `#[cfg(test)]` module bodies.
    println!(
        "float-free: {scanned} of {total} lines across {files} declared files ({}% — the rest is test modules)",
        100 * scanned / total.max(1)
    );
    assert!(
        scanned * 2 > total,
        "the guard read {scanned} of {total} lines, under half of what it declares — a stopping rule \
         has gone wrong again and a green result here would mean nothing"
    );
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
    let call = code_only_line("let y = x.exp();");
    assert!(LIBM_CALLS.iter().any(|c| call.contains(c)));
    assert!(!has_float_type(&call));

    assert_eq!(code_only_line("let x = 1; // 2.0 is fine in a comment"), "let x = 1; ");
    assert_eq!(code_only_line("//! a doc line with f64 in it"), "");
    // The literal that made this necessary: a model name is not an arithmetic constant.
    assert!(!has_float_literal(&code_only_line("let m = \"Qwen/Qwen2.5-1.5B\";")));
    assert!(!has_float_type(&code_only_line("let m = \"an f64 mentioned in a string\";")));

    // **A string that spans lines stays a string.** Both directions of the old per-line reset:
    // the continuation line is prose (not a float literal), and the code AFTER the closing quote
    // is still scanned (it is).
    let mut open = false;
    let first = code_only(r#"        die(format!("the row whose epsilon it \"#, &mut open);
    assert!(open, "a line that opens a string and does not close it leaves the scanner inside it");
    assert!(!has_float_literal(&first));
    let middle = code_only("             DOES execute is `Qwen/Qwen2.5-1.5B/graph-v3`, and moving \\", &mut open);
    assert!(open, "the continuation line does not close the string either");
    assert!(!has_float_literal(&middle), "a model name inside a continued string is not a float literal");
    let last = code_only("             this worker is a class decision\", 1.5);", &mut open);
    assert!(!open, "the closing quote ends the string");
    assert!(has_float_literal(&last), "real code after a multi-line string's closing quote must still be scanned");
    // And a comment marker inside a string must not truncate the line before real code.
    assert!(has_float_literal(&code_only_line("let u = \"http://x\"; let x = 1.0;")));
}
