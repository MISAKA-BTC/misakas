//! **One shape for every refusal, hold and failed check** — ADR-0122 Decision 4.
//!
//! `Reason` is what is wrong in the operator's words, `Current` the measured values, `Required`
//! what would pass, `Fix` one command where one exists, `Docs` an anchor in this repository. The
//! same five fields go to JSON, so the dashboard and MISAKA Studio read the fields, never the prose.
//!
//! A finding never invents a number: `current` carries only values read from the node, the host or
//! a file, and a value the command could not read is said to be unknown rather than shown as zero.

use serde::Serialize;
use std::io::IsTerminal;

/// How bad a finding is. `Ok` rows exist so `doctor` can print the checks that passed with the
/// same renderer; they never carry a code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    Ok,
    Skip,
    Info,
    Warning,
    Error,
}

impl Severity {
    /// The one-column mark every operator screen uses: `✓` done, `·` skipped, `i` a fact, `!` a
    /// warning, `✗` a failure.
    pub(crate) fn mark(self) -> &'static str {
        match self {
            Severity::Ok => "✓",
            Severity::Skip => "·",
            Severity::Info => "i",
            Severity::Warning => "!",
            Severity::Error => "✗",
        }
    }

    pub(crate) fn paint(self, s: &str) -> String {
        match self {
            Severity::Ok => paint::green(s),
            Severity::Skip | Severity::Info => paint::dim(s),
            Severity::Warning => paint::yellow(s),
            Severity::Error => paint::red(s),
        }
    }
}

/// ADR-0122 Decision 4's five fields, plus the code, the severity and the exit status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Finding {
    /// `E-<AREA>-<NAME>` for an error, `W-…` for a warning. Never reused for another condition.
    pub(crate) code: &'static str,
    pub(crate) severity: Severity,
    /// One line: what the operator sees first.
    pub(crate) title: String,
    pub(crate) reason: String,
    pub(crate) current: Vec<String>,
    pub(crate) required: String,
    pub(crate) fix: Vec<String>,
    pub(crate) docs: Option<&'static str>,
    /// The process exit status this finding maps to (ADR-0122 §5's range, or an existing code).
    pub(crate) exit: i32,
}

impl Finding {
    pub(crate) fn error(code: &'static str, exit: i32, title: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, exit, title)
    }

    pub(crate) fn warning(code: &'static str, title: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, crate::exit::SUCCESS, title)
    }

    fn new(code: &'static str, severity: Severity, exit: i32, title: impl Into<String>) -> Self {
        Self {
            code,
            severity,
            title: title.into(),
            reason: String::new(),
            current: Vec::new(),
            required: String::new(),
            fix: Vec::new(),
            docs: None,
            exit,
        }
    }

    pub(crate) fn reason(mut self, s: impl Into<String>) -> Self {
        self.reason = s.into();
        self
    }

    pub(crate) fn current(mut self, s: impl Into<String>) -> Self {
        self.current.push(s.into());
        self
    }

    pub(crate) fn required(mut self, s: impl Into<String>) -> Self {
        self.required = s.into();
        self
    }

    pub(crate) fn fix(mut self, s: impl Into<String>) -> Self {
        self.fix.push(s.into());
        self
    }

    pub(crate) fn docs(mut self, anchor: &'static str) -> Self {
        self.docs = Some(anchor);
        self
    }

    /// The block an operator reads. The fields are aligned under one another and never wrapped by
    /// this function: a `Fix` is often a command, and a command broken across lines is a command
    /// that no longer pastes.
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        let head = format!("{} {}", self.severity.mark(), self.title);
        out.push_str(&self.severity.paint(&head));
        out.push_str(&paint::dim(&format!("   [{}]", self.code)));
        out.push('\n');
        let mut field = |name: &str, lines: &[String]| {
            let mut first = true;
            for line in lines.iter().filter(|l| !l.is_empty()) {
                let label = if first { name } else { "" };
                out.push_str(&format!("  {}  {line}\n", paint::bold(&format!("{label:<8}"))));
                first = false;
            }
        };
        field("Reason", std::slice::from_ref(&self.reason));
        field("Current", &self.current);
        field("Required", std::slice::from_ref(&self.required));
        field("Fix", &self.fix);
        if let Some(docs) = self.docs {
            field("Docs", &[docs.to_string()]);
        }
        out.trim_end().to_string()
    }
}

/// The severity a set of findings adds up to, and the exit status it maps to: the most severe
/// finding decides, and among findings of one severity the first one listed does (a command lists
/// its findings in the order it checked them, so that is the cause furthest upstream).
pub(crate) fn exit_of(findings: &[Finding], strict: bool) -> i32 {
    let worst = findings.iter().map(|f| f.severity).max().unwrap_or(Severity::Ok);
    match worst {
        Severity::Error => findings.iter().find(|f| f.severity == Severity::Error).map(|f| f.exit).unwrap_or(crate::exit::GENERIC),
        Severity::Warning if strict => crate::exit::GENERIC,
        _ => crate::exit::SUCCESS,
    }
}

/// ANSI colour, only where a person is looking: stdout is a terminal and `NO_COLOR` is unset.
/// Everything else — a pipe, a file, `--output json` — gets the same text without escapes.
pub(crate) mod paint {
    use super::IsTerminal;
    use std::sync::OnceLock;

    fn on() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        // A test asserts on the text, and the harness's stdout can be a terminal.
        *ON.get_or_init(|| !cfg!(test) && std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
    }

    fn wrap(code: &str, s: &str) -> String {
        if on() { format!("\x1b[{code}m{s}\x1b[0m") } else { s.to_string() }
    }

    pub(crate) fn green(s: &str) -> String {
        wrap("32", s)
    }
    pub(crate) fn yellow(s: &str) -> String {
        wrap("33", s)
    }
    pub(crate) fn red(s: &str) -> String {
        wrap("31", s)
    }
    pub(crate) fn cyan(s: &str) -> String {
        wrap("36", s)
    }
    pub(crate) fn dim(s: &str) -> String {
        wrap("2", s)
    }
    pub(crate) fn bold(s: &str) -> String {
        wrap("1", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Finding {
        Finding::error("E-BOND-EXPOSURE-FULL", crate::exit::NOT_READY, "Not mining: the bond's exposure is full")
            .reason("every claim reserves collateral until it is final")
            .current("3 claims reserve 2,150 of 2,200 MSK")
            .current("one more needs 740 MSK")
            .required("740 MSK of room")
            .fix("wait: room returns as claims turn final")
            .docs("docs/testnet11-join-mining.md#exposure")
    }

    /// The five fields render in their fixed order, each labelled once, continuation lines under
    /// the first — the shape every screen in ADR-0122 §11 shows.
    #[test]
    fn a_finding_renders_the_five_fields_in_order() {
        let text = sample().render();
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("✗ Not mining: the bond's exposure is full"), "{text}");
        assert!(lines[0].ends_with("[E-BOND-EXPOSURE-FULL]"), "{text}");
        assert_eq!(lines[1], "  Reason    every claim reserves collateral until it is final");
        assert_eq!(lines[2], "  Current   3 claims reserve 2,150 of 2,200 MSK");
        assert_eq!(lines[3], "            one more needs 740 MSK");
        assert_eq!(lines[4], "  Required  740 MSK of room");
        assert_eq!(lines[5], "  Fix       wait: room returns as claims turn final");
        assert_eq!(lines[6], "  Docs      docs/testnet11-join-mining.md#exposure");
        assert_eq!(lines.len(), 7);
    }

    /// An empty field is left out rather than printed as a bare label.
    #[test]
    fn an_empty_field_is_not_printed() {
        let text = Finding::warning("W-HOST-CLOCK", "the clock is not NTP-synchronised").render();
        assert_eq!(text.lines().count(), 1, "{text}");
    }

    /// The exit status is the most severe finding's; warnings exit 0 unless `--strict`.
    #[test]
    fn the_worst_finding_decides_the_exit() {
        let warn = Finding::warning("W-HOST-CLOCK", "clock");
        let host = Finding::error("E-HOST-DISK-FLOOR", crate::exit::HOST, "disk");
        let funds = Finding::error("E-FUNDS-FEE-OUTPOINT-SPENT", crate::exit::FUNDS, "fee");
        assert_eq!(exit_of(&[], false), crate::exit::SUCCESS);
        assert_eq!(exit_of(std::slice::from_ref(&warn), false), crate::exit::SUCCESS);
        assert_eq!(exit_of(std::slice::from_ref(&warn), true), crate::exit::GENERIC);
        assert_eq!(exit_of(&[warn.clone(), funds.clone(), host.clone()], false), crate::exit::FUNDS);
        assert_eq!(exit_of(&[host, funds, warn], false), crate::exit::HOST);
    }

    /// The JSON carries the same fields by name — what the dashboard and the Studio read.
    #[test]
    fn the_json_is_the_same_five_fields() {
        let v = serde_json::to_value(sample()).unwrap();
        for key in ["code", "severity", "title", "reason", "current", "required", "fix", "docs", "exit"] {
            assert!(v.get(key).is_some(), "missing {key}: {v}");
        }
        assert_eq!(v["severity"], "error");
        assert_eq!(v["exit"], crate::exit::NOT_READY);
    }
}
