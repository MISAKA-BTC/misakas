//! **The guided flows' questions, marks and stops** — ADR-0122 Decision 7.
//!
//! `mining setup`, `verifier setup`, `validator setup`, `model add` and `model market open` all ask
//! the same way and stop the same way. A spending question is shown with the move it makes and
//! answered by a person: `--yes` for a script, `y`/`n` at a terminal. Input that closes before a
//! line is no answer, never the default, and Ctrl-C at a question stops the flow. Every flow ends in
//! one of four ways — done, blocked (a finding says what only the operator can change), waiting
//! (the chain or the network will change it; running the command again resumes), or stopped (a
//! question answered no).

use crate::operator::finding::{Finding, Severity, paint};
use crate::{CliError, CliResult, OutputFormat, exit};
use std::io::{BufRead, IsTerminal, Write};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Yes,
    No,
    /// No terminal to ask and no `--yes`.
    Unanswered,
    /// Ctrl-C at the question.
    Interrupted,
}

/// What a question got back from the terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Input {
    Line(String),
    /// The input closed (or failed) before a line: nobody answered. Never the default — a
    /// question whose default spends must not be answered by a pipe running dry.
    Closed,
    Interrupted,
}

/// One line from the terminal, read so that a Ctrl-C at a question stops the flow instead of
/// waiting for Enter.
///
/// **On a thread of its own, not the runtime's blocking pool.** A read abandoned on Ctrl-C stays
/// blocked until a line arrives, and a runtime waits for its blocking pool on the way out: setup
/// printed "interrupted" and then never exited. A detached thread ends with the process.
async fn read_line() -> Input {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let input = match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => Input::Closed,
            Ok(_) => Input::Line(line),
        };
        let _ = tx.send(input);
    });
    tokio::select! {
        input = rx => input.unwrap_or(Input::Closed),
        _ = tokio::signal::ctrl_c() => Input::Interrupted,
    }
}

pub(crate) struct Ui {
    pub(crate) interactive: bool,
    pub(crate) yes: bool,
    pub(crate) json: bool,
}

impl Ui {
    /// Questions are asked only at a terminal, and only when the output is for a person.
    pub(crate) fn new(output: OutputFormat, yes: bool) -> Ui {
        let json = output == OutputFormat::Json;
        Ui { interactive: !json && std::io::stdin().is_terminal() && std::io::stdout().is_terminal(), yes, json }
    }

    /// Human lines go to stdout, or to stderr when stdout carries the JSON document.
    pub(crate) fn say(&self, line: &str) {
        if self.json {
            eprintln!("{line}");
        } else {
            println!("{line}");
        }
    }
    pub(crate) fn mark(&self, sev: Severity, name: &str, value: &str) {
        let mark = sev.paint(match sev {
            Severity::Info => "◐",
            other => other.mark(),
        });
        self.say(&format!("  {mark} {name:<13} {value}"));
    }
    /// A line under the value column of the last mark.
    pub(crate) fn sub(&self, text: &str) {
        self.say(&format!("                  {text}"));
    }
    pub(crate) async fn confirm(&self, question: &str, default_yes: bool) -> Answer {
        if self.yes {
            self.say(&format!("  ? {question} yes (--yes)"));
            return Answer::Yes;
        }
        if !self.interactive {
            return Answer::Unanswered;
        }
        print!("  {} {question} {} ", paint::cyan("?"), if default_yes { "[Y/n]" } else { "[y/N]" });
        let _ = std::io::stdout().flush();
        let line = match read_line().await {
            Input::Line(line) => line,
            Input::Closed => {
                println!();
                return Answer::Unanswered;
            }
            Input::Interrupted => {
                println!();
                return Answer::Interrupted;
            }
        };
        match line.trim().to_ascii_lowercase().as_str() {
            "" => {
                if default_yes {
                    Answer::Yes
                } else {
                    Answer::No
                }
            }
            "y" | "yes" => Answer::Yes,
            _ => Answer::No,
        }
    }
    /// A number from 1 to `count`; Enter takes `default` (0-based). Without a terminal, the default.
    /// `None` on Ctrl-C, or when the input closed.
    pub(crate) async fn choose(&self, question: &str, count: usize, default: usize) -> Option<usize> {
        if !self.interactive || self.yes {
            return Some(default);
        }
        for _ in 0..3 {
            print!("  {} {question} [1-{count}, Enter = {}] ", paint::cyan("?"), default + 1);
            let _ = std::io::stdout().flush();
            let line = match read_line().await {
                Input::Line(line) => line,
                // Nobody answered: no choice is made for them.
                Input::Closed | Input::Interrupted => return None,
            };
            let t = line.trim();
            if t.is_empty() {
                return Some(default);
            }
            if let Ok(n) = t.parse::<usize>()
                && (1..=count).contains(&n)
            {
                return Some(n - 1);
            }
        }
        Some(default)
    }
}

/// Why a flow stopped before the end.
pub(crate) enum Halt {
    /// Something only the operator can change; the finding says what.
    Blocked(Finding),
    /// Something the chain or the network will change; re-running resumes. The exit code says which.
    Waiting(String, i32),
    /// A question answered no, or not answered.
    Declined(String),
    Interrupted,
}

pub(crate) type Step = Result<(), Halt>;

/// One row of a flow's report, for the JSON document.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Row {
    pub(crate) step: &'static str,
    pub(crate) state: &'static str,
    pub(crate) value: String,
}

/// A guided flow's screen: its rows as it goes, its questions, and how it ends.
pub(crate) struct Flow {
    pub(crate) ui: Ui,
    pub(crate) rows: Vec<Row>,
}

impl Flow {
    pub(crate) fn new(output: OutputFormat, yes: bool) -> Flow {
        Flow { ui: Ui::new(output, yes), rows: Vec::new() }
    }

    pub(crate) fn row(&mut self, sev: Severity, step: &'static str, value: impl Into<String>) {
        let value = value.into();
        self.ui.mark(sev, step, &value);
        let state = match sev {
            Severity::Ok => "ok",
            Severity::Skip => "skipped",
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "blocked",
        };
        self.rows.push(Row { step, state, value });
    }

    /// Ask, and turn the answer into the flow's next move: yes continues; anything else stops,
    /// saying `declined` (or, without a terminal, that `--yes` answers it).
    pub(crate) async fn ask(&self, question: &str, default_yes: bool, declined: &str) -> Step {
        match self.ui.confirm(question, default_yes).await {
            Answer::Yes => Ok(()),
            Answer::No => Err(Halt::Declined(declined.to_string())),
            Answer::Unanswered => Err(Halt::Declined(format!("{declined} — re-run with --yes, or at a terminal, to answer yes"))),
            Answer::Interrupted => Err(Halt::Interrupted),
        }
    }

    /// Wait `secs`, or stop on Ctrl-C.
    pub(crate) async fn pause(&self, secs: u64) -> Step {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => Err(Halt::Interrupted),
            _ = tokio::time::sleep(Duration::from_secs(secs)) => Ok(()),
        }
    }

    /// Render how the flow ended, and the JSON document when that is the output. `done` is the
    /// last line of a flow that finished; `resume` is the command that picks a stopped one up.
    pub(crate) fn finish(
        &self,
        result: Step,
        schema: &str,
        done: &str,
        resume: &str,
        mut doc: serde_json::Map<String, serde_json::Value>,
    ) -> CliResult {
        let (code, state, finding) = match result {
            Ok(()) => {
                self.ui.say("");
                self.ui.say(&paint::green(&format!("● {done}")));
                (0, "done", None)
            }
            Err(Halt::Blocked(f)) => {
                self.ui.say("");
                self.ui.say(&f.render());
                (f.exit, "blocked", Some(f))
            }
            Err(Halt::Waiting(what, code)) => {
                self.ui.say("");
                self.ui.say(&paint::yellow(&format!("◐ waiting for {what} — {resume} picks up here")));
                (code, "waiting", None)
            }
            Err(Halt::Declined(why)) => {
                self.ui.say("");
                self.ui.say(&paint::yellow(&format!("◐ stopped: {why}")));
                (exit::NOT_READY, "stopped", None)
            }
            Err(Halt::Interrupted) => {
                self.ui.say("");
                self.ui.say(&paint::yellow(&format!("◐ interrupted — {resume} picks up here")));
                (exit::NOT_READY, "interrupted", None)
            }
        };
        if self.ui.json {
            doc.insert("schema".into(), schema.into());
            doc.insert("state".into(), state.into());
            doc.insert("steps".into(), serde_json::to_value(&self.rows).unwrap_or_default());
            doc.insert("finding".into(), serde_json::to_value(&finding).unwrap_or_default());
            println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(doc)).expect("serializable"));
        }
        if code == 0 { Ok(()) } else { Err(CliError::new(code, String::new())) }
    }
}
