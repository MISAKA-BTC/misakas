//! **`misaka logs [node|gateway|rail] [--work <id>] [--events] [-f]`** — ADR-0122 Decision 8: the
//! lines every component printed about one work, found by the id they share.
//!
//! The node writes its own log (`<appdir>/misaka-<network>/logs/rusty-kaspa.log`). Under the
//! supervisor, kaspad's console and the gateway's and the rail's output are also in
//! `~/.misaka/<network>/run/<name>.out`. A work is found by any line naming its id: the `event`
//! lines carry `work=<16 hex>` (and `job=<16 hex>` for a prompt job), and the prose lines carry the
//! full claim id or the job's stem. A prefix of eight hex or more is enough to find them.

use crate::operator::finding::paint;
use crate::operator::profile::Profile;
use crate::{CliError, CliResult, exit};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Which component's lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Component {
    Node,
    Gateway,
    Rail,
}

impl Component {
    fn label(self) -> &'static str {
        match self {
            Component::Node => "node",
            Component::Gateway => "gateway",
            Component::Rail => "rail",
        }
    }
}

/// The files a component's lines are in, most authoritative first; a missing file is skipped.
fn sources(profile: &Profile, component: Component) -> Vec<PathBuf> {
    let run = crate::operator::supervisor::run_dir(&profile.network);
    match component {
        Component::Node => vec![profile.log_file.clone()],
        Component::Gateway => vec![run.join("gateway.out")],
        Component::Rail => vec![run.join("rail.out")],
    }
}

/// Does `line` name the work `needle` (lower-case hex)? A job id is matched in its `fp-job-` stem
/// and in `job=` as well as bare.
pub(crate) fn names_work(line: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let lower = line.to_ascii_lowercase();
    lower.contains(needle)
}

/// Is `line` an ADR-0122 `event` line?
pub(crate) fn is_event(line: &str) -> bool {
    line.contains("] event ") || line.contains("] event work=") || line.contains(" event work=") || line.contains(" event job=")
}

/// The lines of `path` that pass `keep`, reading at most `tail_bytes` from the end.
fn read_matching(path: &PathBuf, tail_bytes: u64, keep: &dyn Fn(&str) -> bool) -> Result<Vec<String>, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let start = len.saturating_sub(tail_bytes);
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let body = if start > 0 { text.split_once('\n').map(|(_, rest)| rest).unwrap_or("") } else { &text };
    Ok(body.lines().filter(|l| keep(l)).map(str::to_string).collect())
}

/// The lines about `needle` (8+ hex, lower case; empty = every line) in each component's files,
/// the last `max` of each — the dashboard's read.
pub(crate) fn collect(profile: &Profile, needle: &str, events_only: bool, max: usize) -> Vec<(&'static str, String)> {
    let keep = |line: &str| names_work(line, needle) && (!events_only || is_event(line));
    let mut out = Vec::new();
    for component in [Component::Node, Component::Gateway, Component::Rail] {
        for path in sources(profile, component) {
            if let Ok(found) = read_matching(&path, 64 << 20, &keep) {
                let skip = found.len().saturating_sub(max);
                out.extend(found.into_iter().skip(skip).map(|l| (component.label(), l)));
            }
        }
    }
    out
}

/// `misaka logs`.
pub(crate) async fn run(
    profile: Profile,
    components: &[Component],
    work: Option<&str>,
    events_only: bool,
    follow: bool,
    lines: usize,
) -> CliResult {
    let needle = match work {
        Some(id) => {
            let id = id.trim().trim_start_matches("job:").to_ascii_lowercase();
            if id.len() < 8 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(CliError::new(
                    exit::GENERIC,
                    format!("'{id}' is not a work id: give eight hex or more of the claim id or the job"),
                ));
            }
            id
        }
        None => String::new(),
    };
    let all = [Component::Node, Component::Gateway, Component::Rail];
    let wanted: Vec<Component> = if components.is_empty() { all.to_vec() } else { components.to_vec() };
    let keep = |line: &str| names_work(line, &needle) && (!events_only || is_event(line));
    let mut positions: Vec<(Component, PathBuf, u64)> = Vec::new();
    let mut printed = 0usize;
    for component in &wanted {
        for path in sources(&profile, *component) {
            if !path.exists() {
                continue;
            }
            // A work's lines can sit anywhere in a day of logs; a plain tail is the last screenful.
            let tail = if needle.is_empty() && !events_only { 1 << 20 } else { 64 << 20 };
            match read_matching(&path, tail, &keep) {
                Ok(found) => {
                    let skip = if needle.is_empty() { found.len().saturating_sub(lines) } else { 0 };
                    for line in &found[skip..] {
                        println!("{} {line}", paint::dim(&format!("{:<8}", component.label())));
                        printed += 1;
                    }
                }
                Err(e) => {
                    println!("{} {}", paint::dim(&format!("{:<8}", component.label())), paint::yellow(&format!("(unreadable: {e})")))
                }
            }
            positions.push((*component, path.clone(), std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)));
        }
    }
    if printed == 0 && !follow {
        let what = if needle.is_empty() { String::new() } else { format!(" naming {needle}") };
        let files = positions.iter().map(|(_, p, _)| crate::operator::host::tilde(p)).collect::<Vec<_>>().join(", ");
        println!(
            "{}",
            paint::dim(&format!("no lines{what} in {}", if files.is_empty() { "any log (none found)".to_string() } else { files }))
        );
    }
    if !follow {
        return Ok(());
    }
    // Follow: every file from where it ended, one pass a second, in arrival order.
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        for (component, path, pos) in positions.iter_mut() {
            let Ok(mut file) = std::fs::File::open(&*path) else { continue };
            let len = file.metadata().map(|m| m.len()).unwrap_or(0);
            if len < *pos {
                *pos = 0; // rolled or truncated
            }
            if len == *pos {
                continue;
            }
            if file.seek(SeekFrom::Start(*pos)).is_err() {
                continue;
            }
            let mut bytes = Vec::new();
            if file.read_to_end(&mut bytes).is_err() {
                continue;
            }
            // Only whole lines; a partial last line waits for its newline.
            let end = bytes.iter().rposition(|b| *b == b'\n').map(|i| i + 1).unwrap_or(0);
            *pos += end as u64;
            for line in String::from_utf8_lossy(&bytes[..end]).lines().filter(|l| keep(l)) {
                println!("{} {line}", paint::dim(&format!("{:<8}", component.label())));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The node's, the gateway's and the rail's event lines are all recognised, and a prefix finds
    /// a work in the prose lines too.
    #[test]
    fn a_work_is_found_by_the_id_every_component_prints() {
        let node =
            "2026-09-12 16:47:16.159+09:00 [INFO ] [palw-producer] event work=1578f5f0aa11bb22 lane=block stage=SUBMITTED block=0d";
        let gateway = "[misaka-palw-gateway] event work=1578f5f0aa11bb22 job=5b1e09d2c4a3f001 lane=prompt stage=COMMITTED quanta=12";
        let rail =
            "2026-09-12T07:47:16Z [misaka-palw-fp-rail] event work=1578f5f0aa11bb22 job=5b1e09d2c4a3f001 lane=prompt stage=SUBMITTED";
        let prose = "2026-09-12 16:47:16.159+09:00 [INFO ] [palw-panel] claim 1578F5F0AA11BB22ccdd: interval seat — drew [0]";
        for l in [node, gateway, rail] {
            assert!(is_event(l), "{l}");
            assert!(names_work(l, "1578f5f0"), "{l}");
        }
        assert!(!is_event(prose));
        assert!(names_work(prose, "1578f5f0aa11"), "case does not matter");
        assert!(names_work(gateway, "5b1e09d2"), "a job id finds its lines");
        assert!(!names_work(prose, "deadbeef"));
    }
}
