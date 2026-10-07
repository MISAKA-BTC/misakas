//! **ADR-0173 §4 — the node's fetch hook for a root it does not hold** (lane MU). When a class has a root in force (a line's current
//! version, a preview, a superseded root inside its grace) or is a `Candidate` and this node holds no bundle for it, the node asks an
//! operator-configured external command (`--palw-root-fetch-cmd`) to fetch it into the drop directory (`--palw-root-drop-dir`,
//! default `--palw-improve-artifact-dir`); when the command exits 0 the directory is scanned and a bundle whose `(class, root)` is
//! wanted joins the node's holdings, so the readiness duty proves it next tick.
//!
//! **Node policy, no new trust**: nothing here reaches consensus. A failed or missing fetch is a `ROOT_BUNDLE_MISSING` log line and
//! nothing else — never a ground for a slash, a void or an `Unavailable`. A bundle is accepted only if it declares the wanted class
//! and roots to the wanted root.
//!
//! TODO(ADR-0173 §4): integrate `misaka-model-transport` (`bundle_commitment`, btv2 infohash declarations of ADR-0171) as the default
//! fetcher; until it is in this tree the declared values are passed to the command only when a caller supplies them (none yet).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwModelRegistryClassReadV1, PalwModelRegistryReadV1};

/// How long a want waits before its command is run again.
pub(crate) const PALW_ROOT_FETCH_RETRY_V1: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PalwRootWantKindV1 {
    /// A root other than the class's registered one, in force on the chain.
    InForceRoot,
    /// The registered root of a class in `Candidate`.
    Candidate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PalwRootWantV1 {
    pub class_id: Hash64,
    pub root: Hash64,
    pub kind: PalwRootWantKindV1,
}

/// **The `(class, root)` pairs a seat proves**, each as the class's read with `artifact_root` set to the root, and whether the root is
/// a non-registered one: the registered root always, and — with `root_keyed` (`palw_audit_1004_v1`) — every other root in force.
pub(crate) fn palw_readiness_targets_v1(read: &PalwModelRegistryReadV1, root_keyed: bool) -> Vec<(PalwModelRegistryClassReadV1, bool)> {
    read.classes
        .iter()
        .filter(|c| !c.is_base_class)
        .flat_map(|class| {
            let mut one = vec![(class.clone(), false)];
            if root_keyed {
                for root in class.roots_in_force.iter().filter(|root| **root != class.artifact_root) {
                    let mut other = class.clone();
                    other.artifact_root = *root;
                    one.push((other, true));
                }
            }
            one
        })
        .collect()
}

/// **What this node would fetch**: every target (and nothing for the base class) whose root is an in-force non-registered root, or whose
/// class is a `Candidate`, and that `held` says this node has no bundle for.
pub(crate) fn palw_root_fetch_wants_v1(
    read: &PalwModelRegistryReadV1,
    root_keyed: bool,
    held: &dyn Fn(&Hash64, &Hash64) -> bool,
) -> Vec<PalwRootWantV1> {
    let mut out = Vec::new();
    for (class, extra) in palw_readiness_targets_v1(read, root_keyed) {
        let candidate = class.row.as_ref().is_some_and(|row| matches!(row.state, PalwModelLifecycleV1::Candidate));
        let kind = if extra {
            PalwRootWantKindV1::InForceRoot
        } else if candidate {
            PalwRootWantKindV1::Candidate
        } else {
            continue;
        };
        if !held(&class.class_id, &class.artifact_root) {
            out.push(PalwRootWantV1 { class_id: class.class_id, root: class.artifact_root, kind });
        }
    }
    out
}

/// **The command line**: `--palw-root-fetch-cmd` split on whitespace, then `class_id` and `root` (hex), then — only when declared —
/// `btv2_infohash=<hex>` and `bundle_commitment=<hex>`, then the drop directory as `drop_dir=<path>`.
pub(crate) fn palw_root_fetch_argv_v1(
    cmd: &str,
    want: &PalwRootWantV1,
    infohash: Option<&Hash64>,
    commitment: Option<&Hash64>,
    drop_dir: &Path,
) -> Option<Vec<String>> {
    let mut argv: Vec<String> = cmd.split_whitespace().map(str::to_string).collect();
    if argv.is_empty() {
        return None;
    }
    argv.push(want.class_id.to_string());
    argv.push(want.root.to_string());
    if let Some(h) = infohash {
        argv.push(format!("btv2_infohash={h}"));
    }
    if let Some(c) = commitment {
        argv.push(format!("bundle_commitment={c}"));
    }
    argv.push(format!("drop_dir={}", drop_dir.display()));
    Some(argv)
}

/// What the hook remembers between ticks.
#[derive(Default)]
pub(crate) struct PalwRootFetchStateV1 {
    last_run: BTreeMap<(Hash64, Hash64), Instant>,
    running: BTreeMap<(Hash64, Hash64), std::process::Child>,
    /// Files already scanned (path, mtime): a file that was not the wanted bundle is not opened again until it changes.
    scanned: BTreeSet<(PathBuf, Option<std::time::SystemTime>)>,
}

pub(crate) enum PalwRootFetchEventV1 {
    /// A command was started for the want.
    Started(PalwRootWantV1),
    /// A command exited 0: the drop directory is to be scanned.
    Finished(PalwRootWantV1),
    /// A command could not start or exited non-zero: `ROOT_BUNDLE_MISSING`, nothing more.
    Failed(PalwRootWantV1, String),
}

impl PalwRootFetchStateV1 {
    /// **One tick**: reap finished commands, then start a command for each want not run within the retry interval (at most one at a
    /// time per want, and one new command per tick). Without a command nothing starts — the caller logs `ROOT_BUNDLE_MISSING`.
    pub(crate) fn tick(
        &mut self,
        cmd: Option<&str>,
        drop_dir: Option<&Path>,
        wants: &[PalwRootWantV1],
        now: Instant,
    ) -> Vec<PalwRootFetchEventV1> {
        let mut events = Vec::new();
        let mut done = Vec::new();
        for (key, child) in self.running.iter_mut() {
            match child.try_wait() {
                Ok(None) => {}
                Ok(Some(status)) => done.push((*key, status.success(), format!("exit {status}"))),
                Err(e) => done.push((*key, false, e.to_string())),
            }
        }
        for ((class_id, root), ok, why) in done {
            self.running.remove(&(class_id, root));
            let want = PalwRootWantV1 { class_id, root, kind: PalwRootWantKindV1::InForceRoot };
            events.push(if ok { PalwRootFetchEventV1::Finished(want) } else { PalwRootFetchEventV1::Failed(want, why) });
        }
        let (Some(cmd), Some(drop_dir)) = (cmd, drop_dir) else { return events };
        for want in wants {
            let key = (want.class_id, want.root);
            if self.running.contains_key(&key) || self.last_run.get(&key).is_some_and(|at| now.duration_since(*at) < PALW_ROOT_FETCH_RETRY_V1) {
                continue;
            }
            let Some(argv) = palw_root_fetch_argv_v1(cmd, want, None, None, drop_dir) else { continue };
            self.last_run.insert(key, now);
            match std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    self.running.insert(key, child);
                    events.push(PalwRootFetchEventV1::Started(*want));
                }
                Err(e) => events.push(PalwRootFetchEventV1::Failed(*want, e.to_string())),
            }
            break;
        }
        events
    }

    /// **Scan the drop directory for a wanted bundle.** `open` loads one file and says which `(class, root)` it declares and holds
    /// (`None`: not a bundle this build reads). A file is opened once per mtime. Returns the loaded holdings whose `(class, root)` is
    /// wanted — a bundle that declares another class or roots to another root is not taken.
    pub(crate) fn ingest<T>(
        &mut self,
        dir: &Path,
        wants: &[PalwRootWantV1],
        open: &mut dyn FnMut(&Path) -> Vec<(Hash64, Hash64, T)>,
    ) -> Vec<T> {
        let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut files: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
        files.sort();
        let mut taken = Vec::new();
        for path in files {
            let key = (path.clone(), std::fs::metadata(&path).and_then(|m| m.modified()).ok());
            if self.scanned.contains(&key) {
                continue;
            }
            let mut matched = false;
            for (class_id, root, holding) in open(&path) {
                if wants.iter().any(|w| w.class_id == class_id && w.root == root) {
                    taken.push(holding);
                    matched = true;
                }
            }
            if !matched {
                self.scanned.insert(key);
            }
        }
        taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleRowV1;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    fn class(id: u64, root: u64, roots: &[u64], candidate: bool) -> PalwModelRegistryClassReadV1 {
        PalwModelRegistryClassReadV1 {
            economic_ccu_per_claim: 0,
            work_ratio_permille: 0,
            expected_forwards_q32: 0,
            work_ticket_target: 0,
            class_target: 0,
            panel_room: 0,
            final_work_share_10_permille: 0,
            final_work_share_100_permille: 0,
            class_id: h(id),
            artifact_root: h(root),
            roots_in_force: roots.iter().map(|r| h(*r)).collect(),
            is_base_class: false,
            row: candidate.then(|| PalwModelLifecycleRowV1 {
                state: PalwModelLifecycleV1::Candidate,
                work: Default::default(),
                profile: Default::default(),
                since_span: 0,
                probes_passed: 0,
                probes_failed: 0,
                probes_passed_this_span: 0,
                probes_failed_this_span: 0,
                ready_seats: 0,
                inflight_claims: 0,
                utilization_permille: 0,
                admission_milli: 0,
                cap_utilization_permille: 0,
                priced_share_permille: 0,
            }),
            ready_seats_now: 0,
            seating: None,
            inflight_now: 0,
            share_permille: None,
            no_capable_panel_voids: 0,
            reason: String::new(),
        }
    }

    fn read(classes: Vec<PalwModelRegistryClassReadV1>) -> PalwModelRegistryReadV1 {
        PalwModelRegistryReadV1 { classes, ..Default::default() }
    }

    #[test]
    fn the_hook_fires_for_an_unheld_in_force_root_and_a_candidate_and_for_nothing_else() {
        let r = read(vec![class(1, 10, &[10, 11], false), class(2, 20, &[], true), class(3, 30, &[30], false)]);
        let none_held = |_: &Hash64, _: &Hash64| false;
        let wants = palw_root_fetch_wants_v1(&r, true, &none_held);
        assert_eq!(
            wants,
            vec![
                PalwRootWantV1 { class_id: h(1), root: h(11), kind: PalwRootWantKindV1::InForceRoot },
                PalwRootWantV1 { class_id: h(2), root: h(20), kind: PalwRootWantKindV1::Candidate },
            ],
            "the registered root of a live class is the node's start-up business, not the hook's"
        );
        assert!(palw_root_fetch_wants_v1(&r, false, &none_held).iter().all(|w| w.kind == PalwRootWantKindV1::Candidate), "below the fence only candidates");
        let held = |_: &Hash64, root: &Hash64| *root == h(11);
        assert_eq!(palw_root_fetch_wants_v1(&r, true, &held).len(), 1, "a held root is not fetched");
    }

    #[test]
    fn the_command_line_carries_the_class_the_root_and_only_what_is_declared() {
        let want = PalwRootWantV1 { class_id: h(1), root: h(11), kind: PalwRootWantKindV1::InForceRoot };
        let argv = palw_root_fetch_argv_v1("fetch-bundle --fast", &want, None, None, Path::new("/d")).unwrap();
        assert_eq!(argv.len(), 5);
        assert_eq!((argv[0].as_str(), argv[1].as_str()), ("fetch-bundle", "--fast"));
        assert_eq!(argv[2], h(1).to_string());
        assert_eq!(argv[4], "drop_dir=/d");
        let with = palw_root_fetch_argv_v1("x", &want, Some(&h(5)), Some(&h(6)), Path::new("/d")).unwrap();
        assert!(with.iter().any(|a| a.starts_with("btv2_infohash=")) && with.iter().any(|a| a.starts_with("bundle_commitment=")));
        assert!(palw_root_fetch_argv_v1("  ", &want, None, None, Path::new("/d")).is_none());
    }

    #[test]
    fn a_failed_start_is_an_event_and_never_more_and_a_good_run_then_ingest_makes_the_root_dutiable() {
        let r = read(vec![class(1, 10, &[10, 11], false)]);
        let dir = std::env::temp_dir().join(format!("palw-root-fetch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut ingested: BTreeSet<(Hash64, Hash64)> = BTreeSet::new();
        let mut st = PalwRootFetchStateV1::default();
        let wants = palw_root_fetch_wants_v1(&r, true, &|c, root| ingested.contains(&(*c, *root)));
        assert_eq!(wants.len(), 1);
        // No command configured: nothing starts.
        assert!(st.tick(None, Some(&dir), &wants, Instant::now()).is_empty());
        // A command that cannot start: a Failed event, and the retry interval holds the next try.
        let t0 = Instant::now();
        let ev = st.tick(Some("/nonexistent/palw-fetch"), Some(&dir), &wants, t0);
        assert!(matches!(ev.as_slice(), [PalwRootFetchEventV1::Failed(..)]));
        assert!(st.tick(Some("/nonexistent/palw-fetch"), Some(&dir), &wants, t0 + Duration::from_secs(1)).is_empty());
        // A command that exits 0 → Finished; the directory then holds the bundle and the ingest takes it.
        let mut st = PalwRootFetchStateV1::default();
        let mut started = st.tick(Some("true"), Some(&dir), &wants, t0);
        assert!(matches!(started.pop(), Some(PalwRootFetchEventV1::Started(_))));
        let mut finished = Vec::new();
        for _ in 0..200 {
            finished = st.tick(Some("true"), Some(&dir), &wants, t0);
            if !finished.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(finished.as_slice(), [PalwRootFetchEventV1::Finished(_)]));
        std::fs::write(dir.join("other.bin"), b"not it").unwrap();
        std::fs::write(dir.join("r2.bin"), b"bundle").unwrap();
        let mut open = |p: &Path| -> Vec<(Hash64, Hash64, &'static str)> {
            if p.ends_with("r2.bin") { vec![(h(1), h(11), "held")] } else { vec![(h(1), h(99), "wrong root")] }
        };
        let taken = st.ingest(&dir, &wants, &mut open);
        assert_eq!(taken, vec!["held"], "a bundle that roots elsewhere is not taken");
        ingested.insert((h(1), h(11)));
        let wants = palw_root_fetch_wants_v1(&r, true, &|c, root| ingested.contains(&(*c, *root)));
        assert!(wants.is_empty(), "held: no more fetching");
        let targets = palw_readiness_targets_v1(&r, true);
        assert!(
            targets.iter().any(|(c, extra)| *extra && c.artifact_root == h(11)),
            "and the root is a proof duty target of the readiness loop"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
