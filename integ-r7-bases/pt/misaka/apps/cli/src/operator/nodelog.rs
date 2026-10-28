//! **What the node's own log says**, read the way an operator reads it — ADR-0122 Decision 3.
//!
//! Until the node serves its runtime over RPC (ADR-0122 §6.5, `getPalwNodeStatus`), a few facts
//! live only in its log:
//! * why the producer is holding;
//! * how many draws it has made and at what odds;
//! * which blocks it produced;
//! * the fingerprint and fence schedule it booted with;
//! * what the retention janitor last did.
//!
//! This module parses exactly those lines, by the same strings the node prints (`palw_producer.rs`,
//! `palw_panel.rs`, `palw_retention.rs`, `daemon.rs`). Nothing else in a log line is interpreted.
//!
//! A line the parser does not recognise is skipped, never guessed at. A fact the log did not
//! contain is `None`, and the screens say "not in the log" rather than print a default.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// `2026-09-12 13:48:29.123+09:00 [INFO ] message` — `LOG_LINE_PATTERN` in `core/src/log/consts.rs`.
const TS_LEN: usize = "2026-09-12 13:48:29.123+09:00".len();
const TS_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f%:z";

/// One parsed line: when, how loud, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line<'a> {
    /// Unix seconds, from the line's own timestamp (which carries its UTC offset).
    pub(crate) ts: i64,
    pub(crate) level: &'a str,
    pub(crate) msg: &'a str,
}

pub(crate) fn parse_line(line: &str) -> Option<Line<'_>> {
    let stamp = line.get(..TS_LEN)?;
    let ts = chrono::DateTime::parse_from_str(stamp, TS_FORMAT).ok()?.timestamp();
    let rest = line.get(TS_LEN..)?.strip_prefix(" [")?;
    let close = rest.find("] ")?;
    Some(Line { ts, level: rest[..close].trim(), msg: &rest[close + 2..] })
}

/// The producer's draw report, printed every five minutes while it draws (`palw_producer.rs`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Draws {
    pub(crate) draws: u64,
    pub(crate) produced: u64,
    pub(crate) network_lost: u64,
    /// The class ticket's chance per draw, as the node printed it.
    pub(crate) class_p: Option<f64>,
}

/// The node's own account of one run — the facts ADR-0122's screens take from the log.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeLog {
    pub(crate) path: PathBuf,
    /// The last line's timestamp: a log that stopped moving is a node that stopped writing.
    pub(crate) last_ts: Option<i64>,
    /// When the node last booted (its fingerprint line), and what it printed then.
    pub(crate) boot_ts: Option<i64>,
    pub(crate) fingerprint: Option<String>,
    pub(crate) fingerprint_network: Option<String>,
    pub(crate) schedule: Option<Vec<u64>>,
    pub(crate) schedule_id: Option<String>,
    pub(crate) app_dir: Option<String>,
    /// `[palw-producer] starting (bond=…, key=…)` in this boot.
    pub(crate) producer_bond: Option<String>,
    pub(crate) producer_key: Option<String>,
    pub(crate) producer_started: Option<i64>,
    /// A startup refusal (`… — production disabled`, `not producing (…)`), in this boot.
    pub(crate) producer_disabled: Option<(i64, String)>,
    /// The last `holding:` detail in this boot, and whether a block was produced after it.
    pub(crate) last_hold: Option<(i64, String)>,
    pub(crate) last_draws: Option<(i64, Draws)>,
    pub(crate) last_loading: Option<(i64, String)>,
    pub(crate) producer_stopped: Option<i64>,
    /// Every `produced block` line in the lines read, oldest first: `(ts, count, hash)`.
    pub(crate) produced: Vec<(i64, u64, String)>,
    /// Every `produced RECEIPT block` line: a prompt claim's quantum, spent.
    pub(crate) receipts: Vec<(i64, u64, String)>,
    /// The producer's other warnings (`[palw-producer] <err>`), the last few in this boot.
    pub(crate) producer_errors: Vec<(i64, String)>,
    pub(crate) panel_started: Option<(i64, String)>,
    pub(crate) bond_registered: Option<(i64, String)>,
    /// The registration worker's lines (`--palw-register-bond`) in this boot, the last few: what
    /// `misaka mining setup` shows while it waits for a bond.
    pub(crate) registration: Vec<(i64, String)>,
    pub(crate) janitor_started: Option<i64>,
    pub(crate) janitor_pruned: Option<(i64, String)>,
    pub(crate) janitor_short: Option<(i64, String)>,
    /// Bytes read from the end of the file, and whether that was the whole file.
    pub(crate) bytes_read: u64,
    pub(crate) whole_file: bool,
}

const PRODUCER: &str = "[palw-producer] ";
const PANEL: &str = "[palw-panel] ";
const RETENTION: &str = "[palw-retention] ";

impl NodeLog {
    /// Fold one line into the account. Order matters: a later line of a kind replaces an earlier
    /// one, and a boot (the fingerprint line) clears everything that described the previous run.
    pub(crate) fn absorb(&mut self, line: &Line<'_>) {
        let (ts, msg) = (line.ts, line.msg);
        self.last_ts = Some(ts);
        if let Some(rest) = msg.strip_prefix("Consensus params fingerprint: ") {
            self.begin_boot(ts);
            let (id, network) = match rest.split_once(" (network ") {
                Some((id, net)) => (id.trim(), Some(net.trim_end_matches(')').trim().to_string())),
                None => (rest.trim(), None),
            };
            self.fingerprint = Some(id.to_string());
            self.fingerprint_network = network;
            return;
        }
        if let Some(rest) = msg.strip_prefix("Consensus fence schedule: ") {
            let (list, id) = match rest.rsplit_once(" (schedule id ") {
                Some((list, id)) => (list, Some(id.trim_end_matches(')').trim().to_string())),
                None => (rest, None),
            };
            self.schedule = Some(list.split(',').filter_map(|h| h.trim().parse::<u64>().ok()).collect());
            self.schedule_id = id;
            return;
        }
        if let Some(rest) = msg.strip_prefix("Application directory: ") {
            self.app_dir = Some(rest.trim().to_string());
            return;
        }
        if let Some(rest) = msg.strip_prefix(PRODUCER) {
            self.producer(ts, rest);
            return;
        }
        if let Some(rest) = msg.strip_prefix(PANEL) {
            if rest.starts_with("starting (") {
                self.panel_started = Some((ts, rest.to_string()));
            } else if let Some(reg) = rest.strip_prefix("registered bond ") {
                let outpoint = reg.split_whitespace().next().unwrap_or_default().to_string();
                self.bond_registered = Some((ts, outpoint));
                self.registration.push((ts, rest.to_string()));
            } else if registration_note(rest).is_some() {
                self.registration.push((ts, rest.to_string()));
                let keep = self.registration.len().saturating_sub(12);
                self.registration.drain(..keep);
            }
            return;
        }
        if let Some(rest) = msg.strip_prefix(RETENTION) {
            if rest.starts_with("pruning ") {
                self.janitor_started = Some(ts);
            } else if rest.starts_with("pruned ") {
                self.janitor_pruned = Some((ts, rest.to_string()));
            } else if rest.starts_with("the retention volume is still ") {
                self.janitor_short = Some((ts, rest.to_string()));
            }
        }
    }

    fn begin_boot(&mut self, ts: i64) {
        self.boot_ts = Some(ts);
        self.schedule = None;
        self.schedule_id = None;
        self.producer_bond = None;
        self.producer_key = None;
        self.producer_started = None;
        self.producer_disabled = None;
        self.last_hold = None;
        self.last_draws = None;
        self.last_loading = None;
        self.producer_stopped = None;
        self.producer_errors.clear();
        self.panel_started = None;
        self.registration.clear();
        self.janitor_started = None;
    }

    fn producer(&mut self, ts: i64, rest: &str) {
        if let Some(args) = rest.strip_prefix("starting (") {
            // `bond={bond}` is the outpoint's Debug form, `(<txid>, <index>)`, and the key a path:
            // `starting (bond=(6d69…, 0), key=/root/miner.seed)`.
            if let Some((_, after)) = args.split_once("bond=(")
                && let Some((pair, _)) = after.split_once(')')
                && let Some((txid, index)) = pair.split_once(", ")
            {
                self.producer_bond = Some(format!("{}:{}", txid.trim(), index.trim()));
            } else if let Some((_, after)) = args.split_once("bond=") {
                self.producer_bond = after.split([',', ')']).next().map(|b| b.trim().to_string());
            }
            if let Some((_, key)) = args.rsplit_once("key=") {
                self.producer_key = Some(key.trim_end_matches(')').to_string());
            }
            self.producer_started = Some(ts);
            self.producer_stopped = None;
        } else if let Some(detail) = rest.strip_prefix("holding: ") {
            self.last_hold = Some((ts, detail.to_string()));
        } else if let Some(after) = rest.strip_prefix("produced RECEIPT block #") {
            if let Some((n, hash)) = count_and_hash(after) {
                self.receipts.push((ts, n, hash));
            }
        } else if let Some(after) = rest.strip_prefix("produced block #") {
            if let Some((n, hash)) = count_and_hash(after) {
                self.produced.push((ts, n, hash));
            }
            // A block after a hold means the hold cleared; the producer does not print "resumed".
            if self.last_hold.as_ref().is_some_and(|(at, _)| *at <= ts) {
                self.last_hold = None;
            }
        } else if let Some(draws) = parse_draws(rest) {
            // A draw report means the producer is drawing again, whatever it held on before.
            if self.last_hold.as_ref().is_some_and(|(at, _)| *at <= ts) {
                self.last_hold = None;
            }
            self.last_draws = Some((ts, draws));
        } else if rest.starts_with("loading class artifact ") {
            self.last_loading = Some((ts, rest.to_string()));
        } else if rest.starts_with("stopping (") {
            self.producer_stopped = Some(ts);
        } else if rest.contains("— production disabled") || rest.starts_with("not producing") {
            self.producer_disabled = Some((ts, rest.to_string()));
        } else if !rest.starts_with("palw weight=")
            && !rest.starts_with("this draw read ")
            && !rest.starts_with("residency for ")
            && !rest.starts_with("the sink is older")
            && !rest.starts_with("class ")
        {
            self.producer_errors.push((ts, rest.to_string()));
            let keep = self.producer_errors.len().saturating_sub(8);
            self.producer_errors.drain(..keep);
        }
    }
}

/// **What one registration line says** — `palw_panel.rs`'s bond registration worker, by the strings
/// it prints. The wizard reads its node's log with these instead of asking the operator to copy an
/// outpoint off it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationNote {
    /// Still working, or waiting on something that may clear by itself (funds confirming, sync).
    Waiting(String),
    /// The carrier is in this node's mempool: `carrier <txid> admitted …`.
    Admitted(String),
    /// `registered bond <txid>:<i> …` — the bond exists on the chain.
    Registered(String),
    /// The key already holds this bond: nothing was registered, and nothing needs to be.
    AlreadyHolds(String),
    /// The key's bond is retiring: this key can never register again.
    Retiring(String),
    /// The node gave up on this carrier or on the registration; the line says why.
    Failed(String),
}

pub(crate) fn registration_note(rest: &str) -> Option<RegistrationNote> {
    let outpoint = |s: &str| s.split_whitespace().next().unwrap_or_default().trim_end_matches([',', '.']).to_string();
    if let Some(r) = rest.strip_prefix("registered bond ") {
        return Some(RegistrationNote::Registered(outpoint(r)));
    }
    if let Some(r) = rest.strip_prefix("this key already holds bond ") {
        return Some(RegistrationNote::AlreadyHolds(outpoint(r)));
    }
    if let Some(r) = rest.strip_prefix("this key's bond ") {
        return r.contains(" is RETIRING").then(|| RegistrationNote::Retiring(outpoint(r)));
    }
    if let Some(r) = rest.strip_prefix("carrier ") {
        if r.contains(" admitted to this node's mempool") {
            return Some(RegistrationNote::Admitted(outpoint(r)));
        }
        if r.contains("produced no bond")
            || r.contains("NOT in any block")
            || r.contains("reached NO block")
            || r.contains("reached no block")
        {
            return Some(RegistrationNote::Failed(rest.to_string()));
        }
        return None;
    }
    if let Some(why) = rest.strip_prefix("cannot register a bond yet: ") {
        return Some(RegistrationNote::Waiting(why.to_string()));
    }
    if let Some(r) = rest.strip_prefix("still cannot register a bond") {
        let why = r.rsplit_once(": ").map(|(_, why)| why).unwrap_or(r);
        return Some(RegistrationNote::Waiting(why.to_string()));
    }
    if rest.starts_with("--palw-register-bond") || rest.starts_with("stopping: no bond was registered") {
        return Some(RegistrationNote::Failed(rest.to_string()));
    }
    if rest.starts_with("no bond yet; registering one")
        || rest.starts_with("registering this node's bond")
        || rest.starts_with("sizing collateral at ")
        || rest.starts_with("raising collateral from ")
    {
        return Some(RegistrationNote::Waiting(rest.to_string()));
    }
    None
}

/// `#12 00a1…ff (…)` → `(12, "00a1…ff")`.
fn count_and_hash(after: &str) -> Option<(u64, String)> {
    let mut words = after.split_whitespace();
    let n = words.next()?.parse::<u64>().ok()?;
    let hash = words.next()?.to_string();
    (hash.len() >= 16 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some((n, hash))
}

/// `41 draws this run, 0 produced, 0 won the class ticket and lost the network draw against bits;
/// class ticket p = 2.934e-4 per draw (1 in 3.408e3)`.
fn parse_draws(rest: &str) -> Option<Draws> {
    let (draws, rest) = rest.split_once(" draws this run, ")?;
    let (produced, rest) = rest.split_once(" produced, ")?;
    let (lost, rest) = rest.split_once(" won the class ticket")?;
    let class_p = rest.split_once("class ticket p = ").and_then(|(_, p)| p.split_whitespace().next()).and_then(|p| p.parse().ok());
    Some(Draws {
        draws: draws.trim().parse().ok()?,
        produced: produced.trim().parse().ok()?,
        network_lost: lost.trim().parse().ok()?,
        class_p,
    })
}

/// `<appdir>/misaka-<network>/logs/rusty-kaspa.log` — `get_log_dir` in `kaspad/src/daemon.rs`,
/// unless the node was started with `--logdir`.
pub(crate) fn default_log_file(appdir: &Path, network: &str) -> PathBuf {
    appdir.join(format!("misaka-{network}")).join("logs").join("rusty-kaspa.log")
}

/// **Read the node's log**: the last `tail_bytes` of the file, then — if the most recent boot's
/// fingerprint line was not among them — the rest of the file and the newest rolled archive, for
/// those two lines only. A node up for days has written its startup lines far above any tail.
pub(crate) fn read(path: &Path, tail_bytes: u64) -> Result<NodeLog, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.len();
    let start = len.saturating_sub(tail_bytes);
    file.seek(SeekFrom::Start(start)).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    // A tail that starts mid-line drops its first, partial line.
    let body = if start > 0 { text.split_once('\n').map(|(_, rest)| rest).unwrap_or("") } else { &text };
    let mut log = NodeLog { path: path.to_path_buf(), bytes_read: len - start, whole_file: start == 0, ..Default::default() };
    for raw in body.lines() {
        if let Some(line) = parse_line(raw) {
            log.absorb(&line);
        }
    }
    if log.fingerprint.is_none() && start > 0 {
        // The boot is above the tail. Find its two lines without re-reading what was absorbed.
        let head = read_prefix(path, start).unwrap_or_default();
        if let Some(boot) = last_boot_lines(&head) {
            log.boot_ts = log.boot_ts.or(Some(boot.0));
            log.fingerprint = Some(boot.1);
            log.fingerprint_network = boot.2;
            log.schedule = boot.3;
            log.schedule_id = boot.4;
        }
    }
    if log.fingerprint.is_none() {
        let archive = PathBuf::from(format!("{}.1.gz", path.display()));
        if let Ok(f) = std::fs::File::open(&archive) {
            let mut text = String::new();
            if flate2::read::GzDecoder::new(f).read_to_string(&mut text).is_ok()
                && let Some(boot) = last_boot_lines(&text)
            {
                log.boot_ts = log.boot_ts.or(Some(boot.0));
                log.fingerprint = Some(boot.1);
                log.fingerprint_network = boot.2;
                log.schedule = boot.3;
                log.schedule_id = boot.4;
            }
        }
    }
    Ok(log)
}

fn read_prefix(path: &Path, upto: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut bytes = vec![0u8; upto as usize];
    file.read_exact(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

type BootLines = (i64, String, Option<String>, Option<Vec<u64>>, Option<String>);

/// The last boot's fingerprint and schedule lines in `text`, if it has them.
fn last_boot_lines(text: &str) -> Option<BootLines> {
    let mut found: Option<BootLines> = None;
    for raw in text.lines() {
        let Some(line) = parse_line(raw) else { continue };
        if line.msg.starts_with("Consensus params fingerprint: ") {
            let mut one = NodeLog::default();
            one.absorb(&line);
            found = Some((line.ts, one.fingerprint.unwrap_or_default(), one.fingerprint_network, None, None));
        } else if line.msg.starts_with("Consensus fence schedule: ")
            && let Some(boot) = found.as_mut()
        {
            let mut one = NodeLog::default();
            one.absorb(&line);
            boot.3 = one.schedule;
            boot.4 = one.schedule_id;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> NodeLog {
        let mut log = NodeLog::default();
        for raw in lines {
            let line = parse_line(raw).unwrap_or_else(|| panic!("unparsed: {raw}"));
            log.absorb(&line);
        }
        log
    }

    const HASH: &str = "00a1b2c3d4e5f60718293a4b5c6d7e8f00a1b2c3d4e5f60718293a4b5c6d7e8f";

    /// The pattern is the node's `LOG_LINE_PATTERN`, offset included.
    #[test]
    fn a_node_log_line_is_its_timestamp_level_and_message() {
        let l = parse_line("2026-09-12 13:48:29.123+09:00 [WARN ] [palw-producer] holding: x").unwrap();
        assert_eq!(l.ts, 1_789_188_509, "13:48:29 at +09:00 is 04:48:29 UTC");
        assert_eq!(l.level, "WARN");
        assert_eq!(l.msg, "[palw-producer] holding: x");
        assert!(parse_line("not a log line").is_none());
        assert!(parse_line("^SIGTERM - shutting down...").is_none());
    }

    /// The boot lines the daemon prints, read back as the values the doctor compares.
    #[test]
    fn the_boot_prints_its_fingerprint_and_its_heights() {
        let log = feed(&[
            "2026-09-12 10:00:00.000+00:00 [INFO ] Consensus params fingerprint: ae1d6162ffee (network testnet-11)",
            "2026-09-12 10:00:00.001+00:00 [INFO ] Consensus fence schedule: 1150, 1900, 2150, 7000, 2125000 (schedule id c9b15873aa)",
            "2026-09-12 10:00:00.002+00:00 [INFO ] Application directory: /root/.t11",
        ]);
        assert_eq!(log.fingerprint.as_deref(), Some("ae1d6162ffee"));
        assert_eq!(log.fingerprint_network.as_deref(), Some("testnet-11"));
        assert_eq!(log.schedule, Some(vec![1150, 1900, 2150, 7000, 2125000]));
        assert_eq!(log.schedule_id.as_deref(), Some("c9b15873aa"));
        assert_eq!(log.app_dir.as_deref(), Some("/root/.t11"));
    }

    /// The registration worker's lines, read as the node prints them (`palw_panel.rs`): the setup
    /// wizard waits on these instead of asking the operator to copy an outpoint off the log.
    #[test]
    fn the_registration_worker_is_read_by_its_own_sentences() {
        use RegistrationNote as N;
        let txid = "4f2a".repeat(32);
        let registered = format!(
            "registered bond {txid}:0 with 1110106160 sompi of collateral, in tx {txid}. Restart with --palw-producer-bond={txid}:0 (and --palw-produce) to mine with it; the collateral is reclaimable at this node's pay address once the bond is retired."
        );
        assert_eq!(registration_note(&registered), Some(N::Registered(format!("{txid}:0"))));
        let held = format!(
            "this key already holds bond {txid}:0 on this chain — not registering another. Drop --palw-register-bond and run with --palw-producer-bond={txid}:0"
        );
        assert_eq!(registration_note(&held), Some(N::AlreadyHolds(format!("{txid}:0"))));
        let retiring = format!("this key's bond {txid}:0 is RETIRING (since DAA 812), so it can take no new work");
        assert_eq!(registration_note(&retiring), Some(N::Retiring(format!("{txid}:0"))));
        let admitted = format!("carrier {txid} admitted to this node's mempool and queued for relay, spending aa:1 for 5 sompi");
        assert_eq!(registration_note(&admitted), Some(N::Admitted(txid.clone())));
        let queued = format!("carrier {txid} is NOT in any block after 10 minutes — it is still sitting in this node's mempool");
        assert!(matches!(registration_note(&queued), Some(N::Failed(_))));
        let why = "no confirmed UTXO to spend — send at least 1110106160 sompi plus a fee to this node's pay address";
        assert_eq!(registration_note(&format!("cannot register a bond yet: {why}")), Some(N::Waiting(why.into())));
        let repeat = format!(
            "still cannot register a bond — the same refusal, unchanged for 3m across 36 attempts. Nothing about the retry differs, so it will not clear until the funding, the flags or the chain do: {why}"
        );
        assert_eq!(registration_note(&repeat), Some(N::Waiting(why.into())));
        assert!(matches!(registration_note("--palw-register-bond needs --palw-producer-key — not registering"), Some(N::Failed(_))));
        assert_eq!(registration_note("starting (bond=…)"), None, "not a registration line");

        let log = feed(&[
            "2026-09-12 10:00:00.000+00:00 [INFO ] Consensus params fingerprint: ae1d6162ffee (network testnet-11)",
            &format!("2026-09-12 10:00:05.000+00:00 [WARN ] [palw-panel] cannot register a bond yet: {why}"),
            &format!("2026-09-12 10:01:00.000+00:00 [INFO ] [palw-panel] {admitted}"),
            &format!("2026-09-12 10:01:30.000+00:00 [INFO ] [palw-panel] {registered}"),
        ]);
        assert_eq!(log.registration.len(), 3);
        assert_eq!(log.bond_registered.as_ref().map(|(_, b)| b.clone()), Some(format!("{txid}:0")));
    }

    /// A hold is current until the producer draws or produces again; a new boot forgets the old run.
    #[test]
    fn a_hold_clears_when_the_producer_draws_again_and_a_boot_forgets_the_last_run() {
        let hold = "2026-09-12 10:01:00.000+00:00 [WARN ] [palw-producer] holding: the bond's exposure ceiling leaves no room \
                    for another claim [class=ab epoch=3 produced=1 budget=12 exposure=10/10 per_claim=5]";
        // The line as the producer prints it: the outpoint in its Debug form, a tuple.
        let log =
            feed(&["2026-09-12 10:00:00.000+00:00 [INFO ] [palw-producer] starting (bond=(6d69aa, 0), key=/k/miner.seed)", hold]);
        assert!(log.last_hold.as_ref().unwrap().1.starts_with("the bond's exposure ceiling"));
        assert_eq!(log.producer_bond.as_deref(), Some("6d69aa:0"));
        assert_eq!(log.producer_key.as_deref(), Some("/k/miner.seed"));

        let drawing = "2026-09-12 10:06:00.000+00:00 [INFO ] [palw-producer] 41 draws this run, 1 produced, 2 won the class ticket \
                       and lost the network draw against bits; class ticket p = 2.934e-4 per draw (1 in 3.408e3)";
        let log = feed(&[hold, drawing]);
        assert!(log.last_hold.is_none(), "drawing again clears the hold");
        let (_, d) = log.last_draws.unwrap();
        assert_eq!((d.draws, d.produced, d.network_lost), (41, 1, 2));
        assert!((d.class_p.unwrap() - 2.934e-4).abs() < 1e-9);

        let produced = format!(
            "2026-09-12 10:07:00.000+00:00 [INFO ] [palw-producer] produced block #2 {HASH} (class ticket + Layer-0 both under target)"
        );
        let log = feed(&[hold, &produced]);
        assert!(log.last_hold.is_none());
        assert_eq!(log.produced, vec![(1_789_207_620, 2, HASH.to_string())]);

        let reboot = "2026-09-12 11:00:00.000+00:00 [INFO ] Consensus params fingerprint: ae1d (network testnet-11)";
        let log = feed(&[hold, &produced, reboot]);
        assert!(log.last_hold.is_none() && log.producer_bond.is_none(), "a boot forgets the previous run's state");
        assert_eq!(log.produced.len(), 1, "but the blocks it produced are still its blocks");
    }

    /// A receipt block is a spent quantum, not an attempt: it is kept apart from produced blocks.
    #[test]
    fn a_receipt_block_is_not_an_attempt() {
        let log = feed(&[&format!(
            "2026-09-12 10:07:00.000+00:00 [INFO ] [palw-producer] produced RECEIPT block #3 {HASH} (a certified free-prompt \
             claim, mined)"
        )]);
        assert!(log.produced.is_empty());
        assert_eq!(log.receipts.len(), 1);
    }

    /// The startup refusals and the janitor's lines are recognised by the node's own words.
    #[test]
    fn a_refusal_and_the_janitor_are_read_by_the_nodes_own_words() {
        let log = feed(&[
            "2026-09-12 10:00:00.000+00:00 [WARN ] [palw-producer] pay address is not ML-DSA-87 P2PKH — production disabled",
            "2026-09-12 10:00:01.000+00:00 [INFO ] [palw-retention] pruning /r every 60 s: attempt captures after 60 min",
            "2026-09-12 10:01:01.000+00:00 [INFO ] [palw-retention] pruned 3 retained file group(s), 2400 MB",
            "2026-09-12 10:02:01.000+00:00 [WARN ] [palw-retention] the retention volume is still 900 MB under its free-space floor",
            "2026-09-12 10:02:02.000+00:00 [INFO ] [palw-panel] registered bond abcd:0 with 1110106160 sompi of collateral, in tx x.",
        ]);
        assert!(log.producer_disabled.unwrap().1.contains("production disabled"));
        assert!(log.janitor_started.is_some());
        assert!(log.janitor_pruned.unwrap().1.starts_with("pruned 3"));
        assert!(log.janitor_short.is_some());
        assert_eq!(log.bond_registered.unwrap().1, "abcd:0");
    }

    /// Reading a file: the tail is absorbed, and a boot above the tail is still found.
    #[test]
    fn a_boot_above_the_tail_is_still_found() {
        let dir = std::env::temp_dir().join(format!("misaka-nodelog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rusty-kaspa.log");
        let mut text = String::from(
            "2026-09-12 10:00:00.000+00:00 [INFO ] Consensus params fingerprint: ae1d6162 (network testnet-11)\n\
             2026-09-12 10:00:00.001+00:00 [INFO ] Consensus fence schedule: 4000, 7000 (schedule id c9b1)\n",
        );
        for i in 0..2000 {
            text.push_str(&format!("2026-09-12 10:00:01.000+00:00 [INFO ] Accepted block {i}\n"));
        }
        text.push_str("2026-09-12 10:30:00.000+00:00 [WARN ] [palw-producer] holding: nothing\n");
        std::fs::write(&path, &text).unwrap();
        let log = read(&path, 4096).unwrap();
        assert!(!log.whole_file);
        assert_eq!(log.fingerprint.as_deref(), Some("ae1d6162"));
        assert_eq!(log.schedule, Some(vec![4000, 7000]));
        assert_eq!(log.last_hold.as_ref().map(|h| h.1.as_str()), Some("nothing"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
