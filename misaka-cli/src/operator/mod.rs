//! **The operator surface** — ADR-0122: mining is a purpose, and an operator runs one command and
//! reads one work id.
//!
//! * [`work`] — the state machine every work walks, both lanes, one id (Decision 2).
//! * [`status`] — the miner's own state and the one line that says why (Decision 3).
//! * [`finding`] and [`catalog`] — Reason / Current / Required / Fix / Docs (Decision 4).
//! * [`doctor`] — every check a miner needs, grouped and fixed (§6.3).
//! * [`supervisor`] — `mining start | stop | run`: one command to start, a stop that defends its
//!   claims (Decision 5).
//! * [`profile`] — `~/.misaka/mining.toml`, the flags, and the running node's own command line
//!   (Decision 6).
//! * [`dashboard`] — the same answers on one read-only page at 127.0.0.1:8791 (Decision 9).
//! * [`roles`] — `rewards` and `verifier status`; [`market`] — `model list` and `position …` (§8.2).
//! * [`logs`] — every component's lines about one work, by the id they share (Decision 8).
//! * [`wizard`] — `mining setup`, `verifier setup` and `init`: resumable, and it discovers
//!   instead of asking (Decision 7).
//! * [`snapshot`], [`nodelog`], [`procs`], [`host`] — what those read: the node's RPC, its log,
//!   the process table and the host.
//!
//! Most of it only reads. The supervisor starts and stops this host's own mining processes. Three
//! things sign and spend, each only after showing the move and asking: setup (a bond's collateral,
//! a capability declaration, a self-send), and `position buy` / `position sell`.

pub(crate) mod catalog;
pub(crate) mod dashboard;
pub(crate) mod doctor;
pub(crate) mod finding;
pub(crate) mod host;
pub(crate) mod logs;
pub(crate) mod market;
pub(crate) mod nodelog;
pub(crate) mod procs;
pub(crate) mod profile;
pub(crate) mod roles;
pub(crate) mod snapshot;
pub(crate) mod status;
pub(crate) mod supervisor;
pub(crate) mod wizard;
pub(crate) mod work;
pub(crate) mod work_cmd;
