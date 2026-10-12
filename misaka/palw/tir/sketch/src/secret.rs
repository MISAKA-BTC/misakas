//! **The seat's secret, and every vector drawn from it** (RFC-0007 Part II, §II.4).
//!
//! A Freivalds check is sound only against a producer who does not know the vector it is checked
//! with. The vector is therefore a function of a secret the seat never discloses:
//!
//! ```text
//! epoch key  = BLAKE2b-256(key = seat secret, "misaka-palw/tir/sketch/epoch/v1" ‖ class id ‖ le64(epoch))
//! site seed  = BLAKE2b-256(key = epoch key,   "misaka-palw/tir/sketch/site/v1"  ‖ le16(occurrence) ‖ le16(node) ‖ modulus tag)
//! fresh seed = BLAKE2b-256(key = epoch key,   "misaka-palw/tir/sketch/fresh/v1" ‖ job ‖ le32(pos) ‖ le16(occurrence) ‖ le16(node) ‖ modulus tag)
//! vector     = ChaCha20(seed), each u64 masked to the modulus's bit length and rejected until < p
//! ```
//!
//! * A **site** vector compresses one weight `MatMul` of one occurrence; the sketch of that weight
//!   under it is built once per epoch and reused for every job the seat checks in the epoch.
//! * A **fresh** vector is drawn per check of a `MatMul` whose operands are both activations; it
//!   never outlives the check.
//!
//! **What must stay secret, and why it is more than the seed.** The sketch `s = W·v` of a public
//! weight `W` is `K` linear equations in the `N` entries of `v`; when `N ≤ K` (a down projection)
//! they determine `v`. A sketch store is as secret as the secret it was built from: it is never
//! serialised, logged or sent ([`crate::sketch::TirSketchStoreV1`] has no encoding), and an epoch's
//! store is dropped with its epoch. Nothing in this module derives `Clone`, `Debug` prints no byte,
//! and the bytes are overwritten when a value is dropped.
//!
//! **What a producer learns.** A seat's only output about a claim is its verdict. A producer that
//! submits a wrong value learns that it was caught — which tells it only that its error was not
//! orthogonal to the vector, a set of measure `1 − 1/p` — and is prosecuted through the exact court
//! for the attempt (RFC-0007 Part II, §II.8). Probing the vector costs a conviction per probe.

use blake2b_simd::Params;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::field::TirSketchModulusV1;

/// Key of the epoch key's derivation.
pub const TIR_SKETCH_EPOCH_DOMAIN_V1: &[u8] = b"misaka-palw/tir/sketch/epoch/v1";
/// Key of a weight site's vector seed.
pub const TIR_SKETCH_SITE_DOMAIN_V1: &[u8] = b"misaka-palw/tir/sketch/site/v1";
/// Key of a fresh (activation × activation) vector seed.
pub const TIR_SKETCH_FRESH_DOMAIN_V1: &[u8] = b"misaka-palw/tir/sketch/fresh/v1";

fn wipe(bytes: &mut [u8]) {
    bytes.fill(0);
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    std::hint::black_box(&*bytes);
}

fn keyed(key: &[u8; 32], parts: &[&[u8]]) -> [u8; 32] {
    let mut state = Params::new().hash_length(32).key(key).to_state();
    for p in parts {
        state.update(p);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}

/// **A seat's sketch secret.** 32 bytes the seat draws once and keeps; never serialised.
pub struct TirSeatSketchSecretV1 {
    bytes: [u8; 32],
}

impl TirSeatSketchSecretV1 {
    /// A secret from bytes the seat already holds (its key store, a test).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// A fresh secret from the operating system's generator.
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// The keys of one class in one epoch: everything the sketch builder and the checker draw.
    pub fn keys(&self, class_id: &[u8; 64], epoch: u64) -> TirSketchKeysV1 {
        let key = keyed(&self.bytes, &[TIR_SKETCH_EPOCH_DOMAIN_V1, class_id, &epoch.to_le_bytes()]);
        TirSketchKeysV1 { key, class_id: *class_id, epoch }
    }
}

impl Drop for TirSeatSketchSecretV1 {
    fn drop(&mut self) {
        wipe(&mut self.bytes);
    }
}

impl std::fmt::Debug for TirSeatSketchSecretV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TirSeatSketchSecretV1(..)")
    }
}

/// **One class's keys in one epoch.** Derived from the seat secret; as secret as it.
pub struct TirSketchKeysV1 {
    key: [u8; 32],
    class_id: [u8; 64],
    epoch: u64,
}

impl TirSketchKeysV1 {
    pub fn class_id(&self) -> &[u8; 64] {
        &self.class_id
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The compression vector of the weight `MatMul` `node` in occurrence `occurrence`, over `m`.
    pub fn site_vector(&self, occurrence: u16, node: u16, m: TirSketchModulusV1, len: usize) -> Vec<u64> {
        let seed = keyed(&self.key, &[TIR_SKETCH_SITE_DOMAIN_V1, &occurrence.to_le_bytes(), &node.to_le_bytes(), &[m.tag()]]);
        stream(seed, m, len)
    }

    /// A fresh vector for one check of the activation × activation `MatMul` `node` of occurrence
    /// `occurrence` at position `pos` of job `job`, over `m`.
    pub fn fresh_vector(&self, job: &[u8; 32], pos: u32, occurrence: u16, node: u16, m: TirSketchModulusV1, len: usize) -> Vec<u64> {
        let seed = keyed(
            &self.key,
            &[TIR_SKETCH_FRESH_DOMAIN_V1, job, &pos.to_le_bytes(), &occurrence.to_le_bytes(), &node.to_le_bytes(), &[m.tag()]],
        );
        stream(seed, m, len)
    }
}

impl Drop for TirSketchKeysV1 {
    fn drop(&mut self) {
        wipe(&mut self.key);
    }
}

impl std::fmt::Debug for TirSketchKeysV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TirSketchKeysV1(epoch {}, ..)", self.epoch)
    }
}

/// `len` field elements, uniform in `[0, p)`: a ChaCha20 stream, each word masked to `p`'s bit
/// length and rejected until it is below `p` (exactly uniform; the rejection rate is below 2^-60 for
/// every rung of the ladder).
fn stream(mut seed: [u8; 32], m: TirSketchModulusV1, len: usize) -> Vec<u64> {
    let mut rng = ChaCha20Rng::from_seed(seed);
    wipe(&mut seed);
    let p = m.p();
    let mask = u64::MAX >> p.leading_zeros();
    (0..len)
        .map(|_| {
            loop {
                let x = rng.next_u64() & mask;
                if x < p {
                    break x;
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vectors_are_a_function_of_the_secret_the_class_the_epoch_and_the_site() {
        let class = [7u8; 64];
        let a = TirSeatSketchSecretV1::from_bytes([1; 32]).keys(&class, 3);
        let a2 = TirSeatSketchSecretV1::from_bytes([1; 32]).keys(&class, 3);
        let m = TirSketchModulusV1::P61;
        let v = a.site_vector(4, 9, m, 16);
        assert_eq!(v, a2.site_vector(4, 9, m, 16), "deterministic");
        assert!(v.iter().all(|x| *x < m.p()));
        assert_ne!(v, TirSeatSketchSecretV1::from_bytes([2; 32]).keys(&class, 3).site_vector(4, 9, m, 16), "another seat");
        assert_ne!(v, TirSeatSketchSecretV1::from_bytes([1; 32]).keys(&[8u8; 64], 3).site_vector(4, 9, m, 16), "another class");
        assert_ne!(v, TirSeatSketchSecretV1::from_bytes([1; 32]).keys(&class, 4).site_vector(4, 9, m, 16), "another epoch");
        assert_ne!(v, a.site_vector(4, 10, m, 16), "another node");
        assert_ne!(v, a.site_vector(5, 9, m, 16), "another occurrence");
        assert_ne!(v, a.site_vector(4, 9, TirSketchModulusV1::P64, 16), "another modulus");
        let job = [9u8; 32];
        let f = a.fresh_vector(&job, 11, 4, 9, m, 16);
        assert_ne!(f, v, "a fresh vector is not the site's");
        assert_ne!(f, a.fresh_vector(&job, 12, 4, 9, m, 16), "another position");
        assert_ne!(f, a.fresh_vector(&[0u8; 32], 11, 4, 9, m, 16), "another job");
        // A prefix is a prefix: a longer draw extends a shorter one (one stream per seed).
        assert_eq!(&a.site_vector(4, 9, m, 32)[..16], &v[..]);
    }

    #[test]
    fn nothing_prints_a_secret_byte() {
        let s = TirSeatSketchSecretV1::from_bytes([0xAB; 32]);
        let k = s.keys(&[0u8; 64], 1);
        let text = format!("{s:?} {k:?}");
        assert!(!text.contains("171") && !text.to_lowercase().contains("ab"), "{text}");
    }

    #[test]
    fn the_stream_is_uniform_enough_to_hit_every_residue_of_a_small_prime() {
        let k = TirSeatSketchSecretV1::from_bytes([5; 32]).keys(&[1u8; 64], 0);
        let m = TirSketchModulusV1::toy(13);
        let v = k.site_vector(0, 0, m, 13 * 200);
        let mut counts = [0u32; 13];
        for x in v {
            counts[x as usize] += 1;
        }
        // 200 expected each; six standard deviations is ±85.
        assert!(counts.iter().all(|c| (115..=285).contains(c)), "{counts:?}");
    }
}
