//! MISAKA EVM token-transfer indexer — pure core (§10).
//!
//! The off-chain Explorer indexer (`misaka-evm-indexer`) consumes the node ONLY
//! through public Ethereum JSON-RPC (§9 WebSocket `logs`/`newHeads` + §8
//! `eth_getLogs` backfill) and materializes token transfers + balances. This
//! crate is its dependency-light, side-effect-free CORE: it decodes raw logs
//! into normalized transfer rows and (later slices) computes reorg deltas. It
//! links no consensus / revm / secp / database — so it builds and unit-tests in
//! isolation, and never enters the node's secp-free default build.
//!
//! So far: the event decoder + transfer model ([`event`], slice 1) and the
//! materialized balances + reorg inverse-delta ([`balance`], slice 2). Storage
//! backends (PostgreSQL / RocksDB), the WS consumer service, the query API, and
//! the metadata worker land in later slices.

pub mod balance;
pub mod event;
pub mod store;
pub mod sync;

pub use balance::Balances;
pub use event::{DecodeError, DecodedEvent, TokenStandard, TokenTransfer};
pub use store::{IndexedBlock, LocatedTransfer, MemStore, TransferStore};
pub use sync::{BlockId, SyncPlan, plan_reconcile};
