//! **ADR-0160 S-I1 / S-T2: no consensus rule calls the capacity formulas or the shadow.**
//!
//! Lane shadow ships as a node update before any capacity fence, so nothing it adds may reach a
//! verdict: `palw_capacity_formulas_v1` and `palw_capacity_shadow_v1` are called only by the shadow
//! module itself, the read API that serves `getPalwCapacityShadow` (a `ConsensusApi` default that
//! answers `None`, the consensus forwarding, and the processor's read impl over the committed
//! tip), and tests. This test scans every `.rs` file under `consensus/src` and `consensus/core/src`
//! — code only, comments and string literals blanked — for a `palw_capacity_` identifier, and
//! refuses any occurrence outside [`ALLOWED`] and outside `#[cfg(test)]` code.
//!
//! **Widening.** When a consensus lane (weight, escrow, liab, verify) starts calling a formula
//! behind its fence, it adds its call site to [`ALLOWED`] BY NAME — file and enclosing function —
//! in the same commit, so every consensus reader of the formulas is listed here and reviewed.

use std::path::{Path, PathBuf};

/// `(file relative to the repository's `consensus/`, enclosing fn name or "*" for the whole file)`.
const ALLOWED: &[(&str, &str)] = &[
    // The modules themselves.
    ("core/src/palw_capacity_formulas_v1.rs", "*"),
    ("core/src/palw_capacity_shadow_v1.rs", "*"),
    // Their declarations.
    ("core/src/lib.rs", "mod"),
    // The read API: the trait's `None` default, the forwarding, the processor's read impl.
    ("core/src/api/mod.rs", "palw_capacity_shadow_v1"),
    ("src/consensus/mod.rs", "palw_capacity_shadow_v1"),
    ("src/pipeline/virtual_processor/processor.rs", "palw_capacity_shadow_v1_impl"),
];

const NEEDLE: &str = "palw_capacity_";

/// Blank comments, string and char literals (keeping newlines and byte positions per line), so
/// the scan and the brace counts see code only.
fn code_only(src: &str) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum S {
        Code,
        Line,
        Block(u32),
        Str,
        Raw(usize),
    }
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut state = S::Code;
    let mut i = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match state {
            S::Code => {
                if c == '/' && next == Some('/') {
                    state = S::Line;
                    out.push(' ');
                } else if c == '/' && next == Some('*') {
                    state = S::Block(1);
                    out.push_str("  ");
                    i += 1;
                } else if c == '"' {
                    state = S::Str;
                    out.push(' ');
                } else if c == 'r'
                    && (next == Some('"') || next == Some('#'))
                    && !chars.get(i.wrapping_sub(1)).is_some_and(|p| p.is_alphanumeric() || *p == '_')
                {
                    // A raw string: r"…" or r#…#"…"#…#.
                    let mut j = i + 1;
                    let mut hashes = 0;
                    while chars.get(j) == Some(&'#') {
                        hashes += 1;
                        j += 1;
                    }
                    if chars.get(j) == Some(&'"') {
                        for _ in i..=j {
                            out.push(' ');
                        }
                        i = j;
                        state = S::Raw(hashes);
                    } else {
                        out.push(c);
                    }
                } else if c == '\'' {
                    // A char literal ('x', '\n', '\u{..}') — or a lifetime, left as code.
                    if next == Some('\\') {
                        let mut j = i + 2;
                        while j < chars.len() && chars[j] != '\'' {
                            j += 1;
                        }
                        for _ in i..=j.min(chars.len() - 1) {
                            out.push(' ');
                        }
                        i = j;
                    } else if chars.get(i + 2) == Some(&'\'') {
                        out.push_str("   ");
                        i += 2;
                    } else {
                        out.push(c);
                    }
                } else {
                    out.push(c);
                }
            }
            S::Line => {
                if c == '\n' {
                    state = S::Code;
                }
                out.push(blank(c));
            }
            S::Block(depth) => {
                if c == '*' && next == Some('/') {
                    state = if depth == 1 { S::Code } else { S::Block(depth - 1) };
                    out.push_str("  ");
                    i += 1;
                } else if c == '/' && next == Some('*') {
                    state = S::Block(depth + 1);
                    out.push_str("  ");
                    i += 1;
                } else {
                    out.push(blank(c));
                }
            }
            S::Str => {
                if c == '\\' {
                    out.push(' ');
                    if let Some(n) = next {
                        out.push(blank(n));
                        i += 1;
                    }
                } else if c == '"' {
                    state = S::Code;
                    out.push(' ');
                } else {
                    out.push(blank(c));
                }
            }
            S::Raw(hashes) => {
                if c == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#')) {
                    for _ in 0..=hashes {
                        out.push(' ');
                    }
                    i += hashes;
                    state = S::Code;
                } else {
                    out.push(blank(c));
                }
            }
        }
        i += 1;
    }
    out
}

/// The byte ranges (in `code`) of each brace-delimited item that starts at `start`: from `start`
/// to the `}` that closes the first `{` after it. `None` if a `;` ends the item first.
fn item_end(code: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i64;
    let mut opened = false;
    for (k, b) in code.iter().enumerate().skip(start) {
        match b {
            b'{' => {
                depth += 1;
                opened = true;
            }
            b'}' => {
                depth -= 1;
                if opened && depth == 0 {
                    return Some(k);
                }
            }
            b';' if !opened => return None,
            _ => {}
        }
    }
    None
}

/// Every `#[cfg(...test...)]`-gated item's byte range, and the names of `#[cfg(test)] mod x;`
/// declarations (their files are test code).
fn test_regions(code: &str) -> (Vec<(usize, usize)>, Vec<String>) {
    let bytes = code.as_bytes();
    let mut regions = Vec::new();
    let mut test_mods = Vec::new();
    let mut from = 0;
    while let Some(pos) = code[from..].find("#[cfg(") {
        let at = from + pos;
        let close = code[at..].find(")]").map(|c| at + c + 2).unwrap_or(code.len());
        let attr = &code[at..close];
        from = close;
        if !attr.contains("test") || attr.contains("not(test)") {
            continue;
        }
        match item_end(bytes, close) {
            Some(end) => regions.push((at, end)),
            None => {
                // `#[cfg(test)] mod name;` — the file `name.rs` is test code.
                let decl = &code[close..code[close..].find(';').map(|s| close + s).unwrap_or(code.len())];
                if let Some(name) = decl.split_whitespace().skip_while(|w| *w != "mod").nth(1) {
                    test_mods.push(name.trim().to_string());
                }
            }
        }
    }
    (regions, test_mods)
}

/// The byte range of `fn name`'s body in `code`.
fn fn_regions(code: &str, name: &str) -> Vec<(usize, usize)> {
    let bytes = code.as_bytes();
    let needle = format!("fn {name}");
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(pos) = code[from..].find(&needle) {
        let at = from + pos;
        let after = bytes.get(at + needle.len()).copied().unwrap_or(b' ');
        from = at + needle.len();
        if after.is_ascii_alphanumeric() || after == b'_' {
            continue;
        }
        if let Some(end) = item_end(bytes, at) {
            out.push((at, end));
        } else {
            // A declaration without a body (a trait method): its signature line.
            let end = code[at..].find(';').map(|s| at + s).unwrap_or(code.len());
            out.push((at, end));
        }
    }
    out
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every occurrence of [`NEEDLE`] outside the allowlist and outside test code, as `file:line`.
fn violations(consensus_dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    rust_files(&consensus_dir.join("src"), &mut files);
    rust_files(&consensus_dir.join("core/src"), &mut files);
    files.sort();
    // First pass: the test-module files every `#[cfg(test)] mod x;` names.
    let mut test_files: Vec<PathBuf> = Vec::new();
    let mut sources = Vec::new();
    for file in &files {
        let src = std::fs::read_to_string(file).expect("readable source");
        let code = code_only(&src);
        let (_, mods) = test_regions(&code);
        let dir = file.parent().unwrap().to_path_buf();
        let stem_dir = dir.join(file.file_stem().unwrap());
        for m in mods {
            for base in [&dir, &stem_dir] {
                test_files.push(base.join(format!("{m}.rs")));
                test_files.push(base.join(&m));
            }
        }
        sources.push((file.clone(), code));
    }
    let mut found = Vec::new();
    let mut allowed_hits: Vec<String> = Vec::new();
    for (file, code) in &sources {
        let rel = file.strip_prefix(consensus_dir).unwrap().to_string_lossy().replace('\\', "/");
        let is_test_file = rel.contains("/tests/") || test_files.iter().any(|t| file.starts_with(t));
        let (tests, _) = test_regions(code);
        let mut allowed: Vec<(usize, usize)> = tests;
        let mut whole = is_test_file;
        for (path, what) in ALLOWED {
            if rel != *path {
                continue;
            }
            match *what {
                "*" => whole = true,
                "mod" => {
                    let mut from = 0;
                    while let Some(pos) = code[from..].find("pub mod palw_capacity_") {
                        let at = from + pos;
                        let end = code[at..].find(';').map(|s| at + s).unwrap_or(code.len());
                        allowed.push((at, end));
                        from = end;
                    }
                }
                name => allowed.extend(fn_regions(code, name)),
            }
        }
        let mut from = 0;
        while let Some(pos) = code[from..].find(NEEDLE) {
            let at = from + pos;
            from = at + NEEDLE.len();
            // An identifier boundary before it (not `xpalw_capacity_…`).
            if at > 0 && (code.as_bytes()[at - 1].is_ascii_alphanumeric() || code.as_bytes()[at - 1] == b'_') {
                continue;
            }
            if whole || allowed.iter().any(|(s, e)| at >= *s && at <= *e) {
                allowed_hits.push(rel.clone());
                continue;
            }
            let line = code[..at].matches('\n').count() + 1;
            found.push(format!("{rel}:{line}"));
        }
    }
    (found, allowed_hits)
}

#[test]
fn s_t2_no_consensus_rule_calls_the_capacity_formulas_or_the_shadow() {
    let consensus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("consensus/").to_path_buf();
    let (found, allowed_hits) = violations(&consensus_dir);
    assert!(
        found.is_empty(),
        "ADR-0160 S-I1: the capacity formulas / shadow are called outside the allowlist — a consensus lane that \
         starts calling one adds its call site to ALLOWED by name, in the same commit: {found:#?}"
    );
    // The scan sees the allowed call sites (it is not vacuously green): the read impl calls the
    // shadow, the forwarding calls the impl, and the modules reference each other.
    for (path, _) in ALLOWED {
        assert!(allowed_hits.iter().any(|hit| hit == path), "no allowed reference found in {path} — is the scan reading the tree?");
    }
}

/// The scanner itself: a call in a non-test function is caught; one in a `#[cfg(test)]` module,
/// in a comment, in a string, or in an allowed function is not.
#[test]
fn s_t2_the_scanner_catches_a_call_and_ignores_tests_comments_and_strings() {
    let src = r##"
fn verdict() -> u64 {
    // palw_capacity_m_c_v1 in a comment
    let s = "palw_capacity_m_c_v1 in a string";
    let r = r#"palw_capacity_ in a raw string"#;
    let c = '{';
    palw_capacity_m_ramp_v1(1, 2)
}
fn allowed_read() { palw_capacity_shadow_v1(); }
#[cfg(test)]
mod tests {
    fn t() { let _ = "}"; palw_capacity_weight_cap_v1(0); }
}
"##;
    let code = code_only(src);
    assert_eq!(code.lines().count(), src.lines().count(), "blanking keeps every line");
    let (tests, _) = test_regions(&code);
    let allowed = fn_regions(&code, "allowed_read");
    let hits: Vec<usize> = code.match_indices(NEEDLE).map(|(at, _)| at).collect();
    let outside: Vec<usize> =
        hits.iter().copied().filter(|at| !tests.iter().chain(allowed.iter()).any(|(s, e)| at >= s && at <= e)).collect();
    assert_eq!(hits.len(), 3, "the comment and both strings are blanked: {code}");
    assert_eq!(outside.len(), 1, "only the verdict's call is outside");
    let line = code[..outside[0]].matches('\n').count() + 1;
    assert_eq!(line, 7);
}
