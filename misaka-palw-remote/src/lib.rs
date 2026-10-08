//! **RFC-0009 — the client half of a PALW miner that runs no `kaspad`.**
//!
//! A miner that has computed a claim and holds the bond key needs four things from the network and nothing else: a *view* of the
//! chain it can trust enough to start an expensive inference, a way to put its **already signed** bytes on the chain through any
//! node, a way to see what became of them across reorgs, and (stage B) a way to leave its evidence with someone else. This crate is
//! those four, as pure functions and traits. It owns no socket and no runtime: the binaries (`misaka-palw-fp-rail --relay`, a
//! future remote attempt miner) adapt their wRPC clients to [`view::ChainView`], [`relay::RelayNode`] and
//! [`template::BlockNode`].
//!
//! # What this does and does not give a miner
//!
//! * A *quorum* of independent nodes agreeing with a pinned checkpoint is an **operational** defence (RFC §6). It is not a light
//!   client: colluding or co-hosted nodes defeat it, and a header alone cannot prove a bond, a class or a fence (stage D).
//! * [`attempt`] mounts an attempt on a quorum-checked template with the miner's OWN executor and signer: the node supplies a template and
//!   relays the finished block, and never sees the inference, the key, or a signature made before the draw is won.
//! * An accepted relay is **not** inclusion: [`track::ClaimTracker`] follows tx id, claim id, block, licence, challenge window and
//!   `Final`/void separately and walks backwards on a reorg.
//! * Stage A0's [`register`] adds a model without a node: a quote that names every cost by its payer, a gate that stops
//!   signing when anything moved, and a tracker that keeps relay ACK, inclusion and the registry's accepted row apart.
//! * Stage B's [`evidence`] gives the Panel somewhere other than the miner's PC to read from; a storage receipt is a promise, not a
//!   proof of future availability, and nothing here moves a slash.
//!
//! Domain strings are `misaka-palw/remote/...`; none of them is a consensus rule.

pub mod attempt;
pub mod bundle;
pub mod checkpoint;
pub mod evidence;
pub mod proof;
pub mod register;
pub mod relay;
pub mod template;
pub mod track;
pub mod trust;
pub mod view;

use kaspa_hashes::Hash64;

pub(crate) fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

pub(crate) fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

pub(crate) fn put_len(state: &mut blake2b_simd::State, bytes: &[u8]) {
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(bytes);
}
