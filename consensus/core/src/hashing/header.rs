use super::HasherExtensions;
use crate::header::Header;
use kaspa_hashes::{Hash, Hash64, HasherBase};

/// Writes the canonical header preimage into `hasher`, overriding the
/// nonce/timestamp. Shared by the three header digests below so they
/// are guaranteed byte-identical except for the hasher domain:
///   * 32-byte legacy hash    — `kaspa_hashes::BlockHash`
///   * 64-byte block identity — `kaspa_hashes::BlockHash64`
///   * 64-byte pre-PoW hash    — `kaspa_hashes::BlockPrePowHash64`
///
/// Frozen byte order (changing it is a hard fork): version, parent
/// levels, hash_merkle_root, accepted_id_merkle_root, utxo_commitment,
/// timestamp, bits, nonce, pow_algo_id, daa_score, blue_score,
/// blue_work, pruning_point. As of PR-9.5e the parent hashes, merkle
/// roots and pruning point are all 64-byte; `utxo_commitment` stays
/// 32-byte (it is an accumulator commitment, not a block identity).
#[inline]
fn write_header_preimage<H: HasherBase>(hasher: &mut H, header: &Header, nonce: u64, timestamp: u64) {
    hasher.update(header.version.to_le_bytes()).write_len(header.parents_by_level.expanded_len()); // Write the number of parent levels

    // Write parents at each level
    header.parents_by_level.expanded_iter().for_each(|level| {
        hasher.write_var_array(level);
    });

    // Write all header fields
    hasher
        .update(header.hash_merkle_root)
        .update(header.accepted_id_merkle_root)
        .update(header.utxo_commitment)
        .update(timestamp.to_le_bytes())
        .update(header.bits.to_le_bytes())
        .update(nonce.to_le_bytes())
        // PR-9.5d: pow_algo_id participates in the header identity
        // after the (timestamp, bits, nonce) PoW triple and before
        // daa_score. Frozen byte order (hard-fork to change).
        .update([header.pow_algo_id])
        .update(header.daa_score.to_le_bytes())
        .update(header.blue_score.to_le_bytes())
        .write_blue_work(header.blue_work)
        .update(header.pruning_point);
}

/// Returns the **legacy 32-byte** header hash using the provided
/// nonce+timestamp. Retained only for the 32-byte kHeavyHash PoW path
/// in `consensus/pow`; the canonical block *identity* is the 64-byte
/// [`hash`] below (ADR-0008).
#[inline]
pub fn hash_override_nonce_time(header: &Header, nonce: u64, timestamp: u64) -> Hash {
    let mut hasher = kaspa_hashes::BlockHash::new();
    write_header_preimage(&mut hasher, header, nonce, timestamp);
    hasher.finalize()
}

/// Returns the 64-byte block-identity hash (ADR-0008). Uses the keyed
/// BLAKE2b-512 `BlockHash64` domain over all header fields including
/// the real nonce/timestamp. This is what `Header::hash` caches and
/// what keys every block store / GHOSTDAG / reachability structure.
pub fn hash(header: &Header) -> Hash64 {
    let mut hasher = kaspa_hashes::BlockHash64::new();
    write_header_preimage(&mut hasher, header, header.nonce, header.timestamp);
    hasher.finalize()
}

// kaspa-pq PR-8.6 / Phase 9 (ADR-0008): 64-byte header hashing path.
//
// `hash_override_nonce_time_64` mirrors the 32-byte function above,
// but uses the keyed BLAKE2b-512 `BlockPrePowHash64` hasher. The
// input layout — version, parent levels, merkle roots, UTXO
// commitment, timestamp, bits, nonce, daa/blue scores, blue_work,
// pruning point — is byte-identical to the 32-byte version, so the
// header hash widens cleanly under the Phase 9 consensus identity
// migration. Genesis hashes will need recomputing once the rest of
// the header struct migrates to Hash64; this function is the seed
// for that migration (and for the Layer 0 PoW verifier in
// consensus/pow).
//
// As of PR-9.5e the parent hashes, merkle roots and pruning point fed
// into this hasher are 64-byte; `utxo_commitment` stays 32-byte. The
// preimage is identical to the 32-byte and identity-64 digests (see
// `write_header_preimage`); only the hasher domain differs.

/// 64-byte pre-PoW hash for the kaspa-pq Layer 0 PoW path. Same
/// preimage layout as `hash_override_nonce_time` (via the shared
/// `write_header_preimage`) but produces a 64-byte `Hash64` under the
/// `BlockPrePowHash64` domain. See ADR-0008.
#[inline]
pub fn hash_override_nonce_time_64(header: &Header, nonce: u64, timestamp: u64) -> kaspa_hashes::Hash64 {
    let mut hasher = kaspa_hashes::BlockPrePowHash64::new();
    write_header_preimage(&mut hasher, header, nonce, timestamp);
    hasher.finalize()
}

/// 64-byte pre-PoW hash with nonce/time zeroed — the canonical
/// pre-PoW input fed to the Layer 0 PoW finalizer
/// (`kaspa_consensus_core::pow_layer0::pow_finalizer_blake2b_512`).
#[inline]
pub fn pre_pow_hash_64(header: &Header) -> kaspa_hashes::Hash64 {
    hash_override_nonce_time_64(header, 0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlueWorkType, blockhash};

    #[test]
    fn test_header_hashing() {
        let header = Header::new_finalized(
            1,
            vec![vec![1.into()]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            234,
            23,
            567,
            // PR-9.5d: pow_algo_id (Phase 1 kHeavyHash = 1).
            crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            0,
            0.into(),
            0,
            Default::default(),
        );
        assert_ne!(blockhash::NONE, header.hash);
    }

    #[test]
    fn test_hash_blue_work() {
        let tests: Vec<(BlueWorkType, Vec<u8>)> =
            vec![(0.into(), vec![0, 0, 0, 0, 0, 0, 0, 0]), (123456.into(), vec![3, 0, 0, 0, 0, 0, 0, 0, 1, 226, 64])];

        for test in tests {
            let mut hasher = kaspa_hashes::BlockHash::new();
            hasher.write_blue_work(test.0);

            let mut hasher2 = kaspa_hashes::BlockHash::new();
            hasher2.update(test.1);
            assert_eq!(hasher.finalize(), hasher2.finalize())
        }
    }
}
