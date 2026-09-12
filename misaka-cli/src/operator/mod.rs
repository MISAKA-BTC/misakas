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
//! * [`logs`] — every component's lines about one work, by the id they share (Decision 8).
//! * [`snapshot`], [`nodelog`], [`procs`], [`host`] — what those read: the node's RPC, its log,
//!   the process table and the host.
//!
//! Everything but the supervisor only reads. The supervisor starts and stops this host's own
//! mining processes; nothing here signs, spends, or changes the host's configuration.

pub(crate) mod catalog;
pub(crate) mod doctor;
pub(crate) mod finding;
pub(crate) mod host;
pub(crate) mod logs;
pub(crate) mod nodelog;
pub(crate) mod procs;
pub(crate) mod profile;
pub(crate) mod snapshot;
pub(crate) mod status;
pub(crate) mod supervisor;
pub(crate) mod work;
pub(crate) mod work_cmd;
