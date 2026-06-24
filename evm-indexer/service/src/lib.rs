//! MISAKA EVM token-transfer indexer — the service IO driver (§10.5 / §10.6).
//!
//! [`misaka-evm-indexer-core`](misaka_evm_indexer_core) is pure: it decodes logs
//! ([`event`](misaka_evm_indexer_core::event)), folds balances with reorg
//! inverse-deltas ([`balance`](misaka_evm_indexer_core::balance)), persists
//! through the [`TransferStore`](misaka_evm_indexer_core::TransferStore) seam,
//! and plans reconciles ([`plan_reconcile`](misaka_evm_indexer_core::plan_reconcile)).
//! This crate is the side-effectful shell that drives that core against a LIVE
//! node:
//!
//! * [`node`] — the [`NodeRpc`](node::NodeRpc) async seam (what the indexer needs
//!   from the node: head height, a block by number, the logs in a height range)
//!   plus the eth-rpc JSON DTOs it parses.
//! * [`http`] — a hand-rolled HTTP/1.1 JSON-RPC client implementing [`NodeRpc`]
//!   over a tokio `TcpStream` (no reqwest/hyper — same tokio-1.42.1 pin that made
//!   the §9 adapter hand-roll its server).
//! * [`store`] — the [`IndexStore`](store::IndexStore) async seam over the core
//!   stores: [`MemIndexStore`](store::MemIndexStore) (always) and
//!   `PgIndexStore` (the `pg` feature).
//! * [`engine`] — the reconcile + backfill driver: the one piece with real
//!   correctness logic, generic over [`NodeRpc`] + [`IndexStore`] and exercised
//!   in [`engine`]'s tests against a fake in-memory node, so the gap → plan →
//!   revert → fetch → decode → apply loop is verified WITHOUT a live node.
//!
//! Latency note: the driver is **poll-based** (it asks the node for its head and
//! reconciles). A §9 WebSocket `newHeads`/`logs` push would lower latency but is
//! a pure optimization over this loop, not a correctness requirement — it is the
//! documented follow-on, kept out of this slice because a WS *client* is
//! unverifiable IO glue with no offline test.

pub mod engine;
pub mod http;
pub mod node;
pub mod store;

pub use engine::{sync_once, EngineError, SyncOutcome};
pub use http::HttpNodeRpc;
pub use node::{NodeBlock, NodeLog, NodeRpc, RpcError};
pub use store::{IndexStore, MemIndexStore};

#[cfg(feature = "pg")]
pub use store::PgIndexStore;
