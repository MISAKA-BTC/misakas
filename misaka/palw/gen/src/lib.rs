//! **RFC-0003 Part I — the generative common layer.**
//!
//! Every generative class profile (image, embedding, audio, video; text keeps RFC-0001's FP Job V4)
//! shares one loop — *job → deterministic execution → committed output → dispute → court replay* —
//! and two pieces of it live here:
//!
//! * [`rand`] — **deterministic randomness** `R(seed, domain, step, position, lane)` (§I.1): a keyed
//!   BLAKE2b-512 counter, the domain table (domain 0 is RFC-0001's D11 sampler, byte for byte), and
//!   the value transforms — words as uniform integers, and `Normal` through the pinned `2^16`-entry
//!   inverse-CDF table `PALW_GAUSS_Q24_V1` (data, not arithmetic). A random tensor enters a PALW-TIR
//!   program as a derived input that the court recomputes; no primitive is added.
//! * [`output`] — **canonical outputs** (§I.3): the canonical bytes of each output kind and the
//!   tile-aligned `output_root`, with the per-tile proofs the court's consistency check opens.
//!
//! A LEAF crate: consensus, the court and node backends all call it, and it depends on nothing in
//! this repository. The golden vectors are `consensus-vectors/rand-v1/` and
//! `consensus-vectors/output-v1/` (`tests/vectors.rs`); `tests/d11_domain0.rs` proves domain 0 is
//! D11 against `consensus/core/src/palw_decode_select_v2.rs` and RFC-0001's own golden vectors.

pub mod output;
pub mod rand;

pub use output::{OutputErrorV1, OutputKindV1, OutputSpecV1, output_root_v1};
pub use rand::{RandDistV1, RandErrorV1, Seed, gauss_q24_v1, rand_values_v1, rand_word_v1, rand_words_v1};
