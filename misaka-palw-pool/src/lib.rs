//! **A PALW mining pool: produce blocks for the BASE-0 LLM floor without running a node.**
//!
//! `kaspad/src/palw_producer.rs` opens by naming what it is not: *"Third-party mining over RPC
//! needs those facts on the wire, which is a protocol change and a separate piece of work. This is
//! the piece that makes the RC a network; that is the piece that makes it a network anyone can
//! mine."* This crate is that piece.
//!
//! # What the pool is, and what it deliberately is not
//!
//! It is a **bonded relay**. The pool runs the node and holds three things a miner cannot have
//! without one — the chain facts admission checks against, a block template, and a P2P mouth to
//! serve the execution material a claim is licensed on. The miner holds two things the pool must
//! never have — its own bond and the key that signs for it. Neither half can do the other's job,
//! and that is the design rather than a limitation:
//!
//! | | pool | miner |
//! |---|---|---|
//! | chain facts, template, submission | ✓ | — |
//! | material retention and gossip | ✓ | — |
//! | the inference, the nonce grind | — | ✓ |
//! | the bond, the key, the signature | — | ✓ |
//! | **who is paid** | — | ✓ (the coinbase pays the miner directly) |
//! | **who is slashed for a bad claim** | — | ✓ (its own bond) |
//!
//! It is **not** a variance-sharing pool, and saying so is better than shipping something that
//! looks like one. `base0_rc_job_anchor_v1` binds the BOND, so every miner derives its own anchor,
//! runs its own inference and searches its own space; there is no shared search to split into
//! shares. What this removes is the node requirement, not the variance. What a miner gains is that
//! it can mine the floor class on a laptop with no chain, no 30 GiB of state and no open ports —
//! the floor's weights are derived from a pinned seed, so there is nothing to download either.
//!
//! # Why a bond is required to join, and not merely to win
//!
//! Two independent reasons, and neither is policy:
//!
//! 1. **An unbonded attempt cannot exist.** Admission compares the attempt's `executor_bond`
//!    against a bond the chain holds and its `executor_pubkey` against the key that bond
//!    registered. A miner with no bond has nothing to put in those fields, so the work it did
//!    would be unmountable. The gate ([`session`]) asks the chain at the door so the miner learns
//!    this in a sentence instead of in wasted inferences.
//! 2. **One bond is one job.** Two miners sharing a bond at one template derive one anchor and
//!    grind one identical space — the second is a duplicate, not additional work. A bond per miner
//!    is what makes a second miner mean a second job.
//!
//! It also puts the slashing where the work is. A pool that signed on its miners' behalf would be
//! a pool that could be convicted for what a miner computed.
//!
//! # What a miner is trusting the pool with, stated plainly
//!
//! The miner has no chain, so it takes the pool's word for the facts — as any pooled miner does.
//! Three of those are checkable and the miner checks them: it recomputes the template's pre-pow
//! itself, recomputes the merkle root over the transactions it was given, and confirms the
//! coinbase pays the address it asked for ([`miner::verify_job_pays_me`]). Two are not: the class
//! target and the retention obligation. And one obligation is genuinely the pool's — **serving the
//! material**. A pool that takes a miner's material and never gossips it produces a claim no seat
//! can license, which voids and pays nothing. That is a liveness risk the miner carries, not a
//! slashing risk: an honest execution is never convictable, whoever fails to serve it.

pub mod miner;
pub mod protocol;
pub mod server;
pub mod session;
