//! misaka-palw-bridge — the node-side server half of the desktop gateway's PALW coordinator
//! protocol (palw-gateway README "PALW coordinator protocol", desktop product doc §15).
//!
//! What this is: the process a `palw-gateway --palw-coordinator <url>` points at. It coordinates
//! A-commits and B replicas across REAL distinct providers and decides matches with the node's
//! own k=2 rule — [`misaka_palw::palw::ReplicaMatchKey::exact_match`] via
//! [`misaka_palw::palw_replica::run_replica_k2`] — over match keys built from the qi35-serve
//! class commitments (output ids root + the engine's route/kv/state execution roots).
//!
//! What this is NOT (honest boundary, same discipline as the gateway's loopback docs): it is not
//! consensus. No beacons, no provider bonds, no DA retention, no auditor lottery, no rewards, no
//! `Matured`. A `mismatch` verdict here is an UNRESOLVED DISPUTE between two replicas — the
//! consensus lane resolves disputes with sampled audits; this bridge only surfaces them. The
//! remaining seams to the chain are listed in the README.
//!
//! Three properties the gateway's in-process loopback deliberately lacks, which are the point of
//! this daemon:
//! * **Independence at the protocol layer**: a job is NEVER offered to its own submitter. There
//!   is no `allow_self_replica` here and no flag to add one.
//! * **Durability**: every state change is an append-only, hash-chained journal event
//!   (`state.rs`); boot replays and verifies the chain, so a restarted bridge resumes exactly
//!   where it stopped and a tampered journal refuses to load.
//! * **Class strictness**: this bridge coordinates the `qi35-serve` class, whose match key
//!   REQUIRES the engine execution roots. Submissions/results without them are rejected up
//!   front, not weakly matched.

pub mod arbitration;
pub mod chain;
pub mod challenge;
pub mod da;
pub mod http;
pub mod match_key;
pub mod pcpb;
pub mod provider;
pub mod state;
pub mod wire;
