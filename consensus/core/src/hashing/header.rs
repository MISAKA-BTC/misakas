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
/// roots and pruning point are all 64-byte; kaspa-pq (ADR-0004 /
/// design §12) widened `utxo_commitment` to 64-byte too, so every
/// field fed into the preimage is now a 64-byte PQ consensus identity.
///
/// kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4 §4.3): for
/// `version >= EVM_HEADER_VERSION` (= 2) only, the two 64-byte EVM
/// commitments — `evm_payload_hash` (the block's own payload data) then
/// `evm_commitment_root` (the mergeset-acceptance execution result) — are
/// appended after `pruning_point`. The gate keeps every v0/v1 preimage
/// byte-identical to the pre-EVM protocol.
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

    // kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4 §4.3): the EVM
    // commitments enter the preimage ONLY for v2+ (`version >=
    // EVM_HEADER_VERSION`) headers. For every existing v0 (genesis) / v1 (live)
    // header this branch is skipped, so the preimage — and therefore all three
    // digests below (legacy-32, identity-64, pre-PoW-64) — is byte-identical to
    // the pre-EVM protocol and no genesis hash or block identity changes. Frozen
    // v2+ byte order (hard-fork to change): evm_payload_hash (the block's own
    // payload data), then evm_commitment_root (the mergeset-acceptance result).
    if header.version >= crate::constants::EVM_HEADER_VERSION {
        hasher.update(header.evm_payload_hash);
        hasher.update(header.evm_commitment_root);
    }

    // kaspa-pq ADR-0022: the DNS/PoS-v2 overlay-state commitment. The overlay is
    // genesis-active on every network (`dns_params.is_some()`), so — unlike the
    // two EVM commitments above, which are gated by the EVM activation fence via
    // the header version — `overlay_commitment_root` enters the preimage
    // UNCONDITIONALLY (all versions), appended last. There is no pre-overlay era
    // to gate against. Adding it is a hard fork: every genesis hash and block
    // identity is recomputed (ADR-0022 §8). Frozen byte position (last).
    hasher.update(header.overlay_commitment_root);

    // ADR-0039 PALW Replica-GEMM lane (design §13.2): the ten PALW ticket/work fields enter the
    // preimage ONLY for v3+ (`version >= PALW_HEADER_VERSION`) headers, appended AFTER
    // `overlay_commitment_root`. For every existing v0/v1/v2 header this branch is skipped, so the
    // preimage — and all three digests — is byte-identical to the pre-PALW protocol and no genesis
    // hash or block identity changes. Frozen v3+ byte order (hard-fork to change): component work
    // (hash then compute), then batch_id, leaf_index, ticket_nullifier, epoch_certificate_hash,
    // chain_commit, target_daa_interval, authorization_hash, proof_type, beacon_seed.
    if header.version >= crate::constants::PALW_HEADER_VERSION {
        hasher
            .write_blue_work(header.blue_hash_work)
            .write_blue_work(header.blue_compute_work)
            .update(header.palw_batch_id)
            .update(header.palw_leaf_index.to_le_bytes())
            .update(header.palw_ticket_nullifier)
            .update(header.palw_epoch_certificate_hash)
            .update(header.palw_chain_commit)
            .update(header.palw_target_daa_interval.to_le_bytes())
            .update(header.palw_authorization_hash)
            .update([header.palw_proof_type])
            .update(header.palw_beacon_seed);
    }

    // PALW Header-v4 public/value-network anti-spam extension. This is a re-genesis-only layout:
    // no existing preset activates v4 and therefore no existing block sees new preimage bytes.
    // Frozen order is accumulator commitment followed by the sole grindable spam nonce.
    if header.version >= crate::constants::PALW_ANTISPAM_HEADER_VERSION {
        hasher.update(header.palw_spam_accumulator_commitment).update(header.palw_spam_nonce.to_le_bytes());
    }

    // ADR-MA Header-v5 Compute Set registry extension (§13). Activation-gated layout like v4:
    // no existing preset activates v5, so no existing block sees new preimage bytes. Frozen
    // order: set id, then the exact policy record id, then the exact allocation plan id — the
    // three references §14/§21.4 resolve historical work from, immutably, forever.
    if header.version >= crate::constants::PALW_COMPUTE_SET_HEADER_VERSION {
        hasher.update(header.palw_compute_set_id).update(header.palw_compute_policy_id).update(header.palw_allocation_plan_id);
    }
}

/// kaspa-pq **ADR-0040 (AUTH-02) — the PALW ticket-authorization header commitment.**
///
/// Returns the digest a ticket authority ML-DSA-signs to authorize ONE algo-4 block. It is the
/// block's own canonical header preimage — the exact bytes [`write_header_preimage`] produces, with
/// no parallel serializer that could drift — under a DISJOINT hasher domain
/// (`PalwAuthPreimageHash64`) and prefixed with the PALW `network_id`, subject to two circular
/// substitutions on v3 and one additional, deliberate anti-spam substitution on v4:
///
///   1. `palw_authorization_hash` := `Hash64::default()`. Necessarily excluded: it is the hash of
///      the authorization that carries this very commitment, so including it is circular.
///   2. `hash_merkle_root` := `authed_root`, the merkle root over every transaction EXCEPT the
///      subnetwork-0x38 authorization transaction. Necessarily substituted for the same reason: the
///      real root covers the authorization tx, whose payload contains this commitment.
///   3. Header-v4 only: `palw_spam_nonce` := 0. This lets the producer finish the authorization tx
///      and final merkle root before grinding the independent objective stamp. Reusing the same
///      signature for another nonce does not make that nonce free: every variant must independently
///      satisfy [`palw_spam_hash`], which binds the complete final header.
///
/// These are the ONLY substitutions. The two circular values are pinned elsewhere:
/// `palw_authorization_hash` must equal `auth.hash()` (clause 7), and the authorization transaction
/// must be in canonical shape with a payload that is the byte-exact borsh re-encoding of the parsed
/// authorization — so, given `authed_root`, the real `hash_merkle_root` has exactly one legal value.
/// The v4 nonce remains objectively constrained by [`palw_spam_hash`] as described above.
///
/// Everything else in the block hash is bound BY CONSTRUCTION, including the five fields that are
/// checked only at the virtual/UTXO stage and therefore never validated at all on a block that does
/// not become a chain candidate (`utxo_commitment`, `accepted_id_merkle_root`, `pruning_point`,
/// `overlay_commitment_root`, `palw_beacon_seed`), plus `palw_epoch_certificate_hash`, `bits`, and
/// the parent hashes at EVERY level in their consensus order. This replaces the previous 9-value
/// allowlist, under which every unlisted header field was free — and any newly added header field
/// would have been silently free too. Because the preimage writer is shared, a new header field is
/// automatically bound the moment it enters the block hash.
///
/// This function does NOT alter the block-hash preimage or its byte order: it only reads it. No
/// genesis hash and no block identity moves.
pub fn palw_authorization_commitment(network_id: u32, header: &Header, authed_root: &Hash64) -> Hash64 {
    let mut substituted = header.clone();
    substituted.hash_merkle_root = *authed_root;
    substituted.palw_authorization_hash = Hash64::default();
    substituted.palw_spam_nonce = 0;
    let mut hasher = kaspa_hashes::PalwAuthPreimageHash64::new();
    hasher.update(network_id.to_le_bytes());
    write_header_preimage(&mut hasher, &substituted, substituted.nonce, substituted.timestamp);
    hasher.finalize()
}

/// Independent Header-v4 objective anti-spam stamp over the complete, FINAL canonical header.
///
/// Unlike AUTH-02 this applies no substitutions: it binds the completed authorization hash, final
/// transaction merkle root, accumulator commitment, and every other header field. The caller may
/// vary only `palw_spam_nonce` while grinding.
pub fn palw_spam_hash(header: &Header) -> Hash64 {
    let mut hasher = kaspa_hashes::PalwSpamHash64::new();
    write_header_preimage(&mut hasher, header, header.nonce, header.timestamp);
    hasher.finalize()
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
// into this hasher are 64-byte; kaspa-pq (ADR-0004 / design §12) widened
// `utxo_commitment` to 64-byte too. The preimage is identical to the
// 32-byte and identity-64 digests (see `write_header_preimage`); only
// the hasher domain differs.

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

    /// kaspa-pq **ADR-0040 (AUTH-02) — the PALW authorization binding is TOTAL.**
    ///
    /// The previous binding committed to nine hand-picked scalars, which meant every OTHER header
    /// field was a free variation axis for an observer holding a valid authorization — and algo-4
    /// blocks are PoW-exempt, so each axis is a free, fully-valid twin block. This test walks the
    /// exact field list the audit enumerated, mutates ONE field at a time, and requires the
    /// commitment to move. It is deliberately exhaustive rather than sampled: the failure mode being
    /// guarded against is "a field nobody thought to list".
    #[test]
    fn palw_authorization_commitment_binds_every_header_field() {
        let mut base = palw_test_header();
        base.version = crate::constants::PALW_ANTISPAM_HEADER_VERSION;
        base.palw_spam_accumulator_commitment = Hash64::from_bytes([0x30; 64]);
        base.palw_spam_nonce = 7;
        base.finalize();
        let authed_root = Hash64::from_bytes([0x11; 64]);
        let net_id = 111u32;
        let base_commitment = palw_authorization_commitment(net_id, &base, &authed_root);

        // Every mutation below is a single field, applied to a fresh clone of the same base header.
        let cases: Vec<(&str, Box<dyn Fn(&mut Header)>)> = vec![
            // --- the five checked ONLY at the virtual/UTXO stage, hence never checked at all on a
            // block that does not become a chain candidate. These were the worst of the free axes.
            ("accepted_id_merkle_root", Box::new(|h: &mut Header| h.accepted_id_merkle_root = Hash64::from_bytes([0xA1; 64]))),
            ("utxo_commitment", Box::new(|h: &mut Header| h.utxo_commitment = Hash64::from_bytes([0xA2; 64]))),
            ("pruning_point", Box::new(|h: &mut Header| h.pruning_point = Hash64::from_bytes([0xA3; 64]))),
            ("overlay_commitment_root", Box::new(|h: &mut Header| h.overlay_commitment_root = Hash64::from_bytes([0xA4; 64]))),
            ("palw_beacon_seed", Box::new(|h: &mut Header| h.palw_beacon_seed = Hash64::from_bytes([0xA5; 64]))),
            // --- freely chosen over the set of store-resident active certificates.
            ("palw_epoch_certificate_hash", Box::new(|h: &mut Header| h.palw_epoch_certificate_hash = Hash64::from_bytes([0xA6; 64]))),
            // --- the remaining block-hash fields, listed so a future header field cannot quietly
            // drop off the binding without this test noticing the shape has changed.
            ("palw_proof_type", Box::new(|h: &mut Header| h.palw_proof_type ^= 1)),
            ("blue_score", Box::new(|h: &mut Header| h.blue_score ^= 1)),
            ("blue_work", Box::new(|h: &mut Header| h.blue_work = h.blue_work + 1u64)),
            ("blue_hash_work", Box::new(|h: &mut Header| h.blue_hash_work = h.blue_hash_work + 1u64)),
            ("blue_compute_work", Box::new(|h: &mut Header| h.blue_compute_work = h.blue_compute_work + 1u64)),
            ("bits", Box::new(|h: &mut Header| h.bits ^= 1)),
            ("nonce", Box::new(|h: &mut Header| h.nonce ^= 1)),
            ("timestamp", Box::new(|h: &mut Header| h.timestamp ^= 1)),
            ("daa_score", Box::new(|h: &mut Header| h.daa_score ^= 1)),
            ("version", Box::new(|h: &mut Header| h.version += 1)),
            ("pow_algo_id", Box::new(|h: &mut Header| h.pow_algo_id ^= 1)),
            ("palw_batch_id", Box::new(|h: &mut Header| h.palw_batch_id = Hash64::from_bytes([0xA7; 64]))),
            ("palw_leaf_index", Box::new(|h: &mut Header| h.palw_leaf_index ^= 1)),
            ("palw_ticket_nullifier", Box::new(|h: &mut Header| h.palw_ticket_nullifier = Hash64::from_bytes([0xA8; 64]))),
            ("palw_chain_commit", Box::new(|h: &mut Header| h.palw_chain_commit = Hash64::from_bytes([0xA9; 64]))),
            ("palw_target_daa_interval", Box::new(|h: &mut Header| h.palw_target_daa_interval ^= 1)),
            (
                "palw_spam_accumulator_commitment",
                Box::new(|h: &mut Header| h.palw_spam_accumulator_commitment = Hash64::from_bytes([0xAA; 64])),
            ),
            // --- level-0 parents (order-sensitive) and, crucially, the parents at levels >= 1, whose
            // ORDER `check_indirect_parents` compares only as a set and so does not pin.
            (
                "parents level 0 (order)",
                Box::new(|h: &mut Header| {
                    let mut levels: Vec<Vec<Hash64>> = h.parents_by_level.expanded_iter().map(|l| l.to_vec()).collect();
                    levels[0].swap(0, 1);
                    h.parents_by_level = levels.try_into().unwrap();
                }),
            ),
            (
                "parents level 1 (order)",
                Box::new(|h: &mut Header| {
                    let mut levels: Vec<Vec<Hash64>> = h.parents_by_level.expanded_iter().map(|l| l.to_vec()).collect();
                    levels[1].swap(0, 1);
                    h.parents_by_level = levels.try_into().unwrap();
                }),
            ),
        ];

        for (name, mutate) in cases {
            let mut mutated = base.clone();
            mutate(&mut mutated);
            assert_ne!(
                hash(&mutated),
                hash(&base),
                "test bug: mutating {name} must produce a DIFFERENT block, otherwise the case proves nothing"
            );
            assert_ne!(
                palw_authorization_commitment(net_id, &mutated, &authed_root),
                base_commitment,
                "ADR-0040 AUTH-02: mutating {name} must break the ticket authorization binding"
            );
        }

        // The authed merkle root and the network id are bound too.
        assert_ne!(palw_authorization_commitment(net_id, &base, &Hash64::from_bytes([0x12; 64])), base_commitment);
        assert_ne!(palw_authorization_commitment(net_id + 1, &base, &authed_root), base_commitment);
    }

    /// ADR-0040 (AUTH-02) — the two circular exclusions plus the v4 stamp nonce.
    ///
    /// `palw_authorization_hash` cannot be bound (it hashes the authorization that carries this
    /// commitment) and `hash_merkle_root` cannot be bound (the real root covers that same
    /// authorization transaction). Both are substituted, so varying them must NOT move the
    /// commitment — clause 7 pins them by other means (`auth.hash()` equality, and the canonical
    /// authorization-transaction shape that makes the real root a function of `authed_root`).
    #[test]
    fn palw_authorization_commitment_excludes_circular_fields_and_only_v4_spam_nonce() {
        let base = palw_test_header();
        let authed_root = Hash64::from_bytes([0x11; 64]);
        let base_commitment = palw_authorization_commitment(111, &base, &authed_root);

        let mut a = base.clone();
        a.palw_authorization_hash = Hash64::from_bytes([0xEE; 64]);
        assert_eq!(palw_authorization_commitment(111, &a, &authed_root), base_commitment);

        let mut b = base.clone();
        b.hash_merkle_root = Hash64::from_bytes([0xEF; 64]);
        assert_eq!(palw_authorization_commitment(111, &b, &authed_root), base_commitment);

        let mut v4 = base;
        v4.version = crate::constants::PALW_ANTISPAM_HEADER_VERSION;
        v4.palw_spam_accumulator_commitment = Hash64::from_bytes([0x31; 64]);
        v4.palw_spam_nonce = 7;
        v4.finalize();
        let v4_commitment = palw_authorization_commitment(111, &v4, &authed_root);
        let mut another_nonce = v4.clone();
        another_nonce.palw_spam_nonce = 8;
        another_nonce.finalize();
        assert_eq!(palw_authorization_commitment(111, &another_nonce, &authed_root), v4_commitment);

        let mut changed_state = v4;
        changed_state.palw_spam_accumulator_commitment = Hash64::from_bytes([0x32; 64]);
        changed_state.finalize();
        assert_ne!(palw_authorization_commitment(111, &changed_state, &authed_root), v4_commitment);
    }

    /// ADR-0040 (AUTH-02) — the authorization domain is DISJOINT from the block-hash domain.
    ///
    /// The commitment is computed over the very same preimage bytes as the block hash (modulo the two
    /// substitutions), so a shared hasher key would let one digest stand in for the other.
    #[test]
    fn palw_authorization_commitment_is_domain_separated_from_the_block_hash() {
        let mut h = palw_test_header();
        // Make the substitutions vacuous so the two functions see byte-identical preimages.
        h.palw_authorization_hash = Hash64::default();
        h.finalize();
        let authed_root = h.hash_merkle_root;
        assert_ne!(palw_authorization_commitment(0, &h, &authed_root).as_bytes().as_slice(), h.hash.as_bytes().as_slice());
    }

    /// A v3 (PALW) header with EVERY hashed field set to a distinct non-zero value and two parent
    /// levels of two parents each, so a single-field mutation is always observable.
    fn palw_test_header() -> Header {
        let mut h = Header::new_finalized(
            crate::constants::PALW_HEADER_VERSION,
            vec![
                vec![Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64])],
                vec![Hash64::from_bytes([3; 64]), Hash64::from_bytes([4; 64])],
            ]
            .try_into()
            .unwrap(),
            Hash64::from_bytes([5; 64]),
            Hash64::from_bytes([6; 64]),
            Hash64::from_bytes([7; 64]),
            0x5000,
            0x1f00_ffff,
            0x0102_0304_0506_0708,
            crate::pow_layer0::POW_ALGO_ID_PALW_REPLICA,
            4242,
            123456.into(),
            777,
            Hash64::from_bytes([8; 64]),
        );
        h.evm_payload_hash = Hash64::from_bytes([9; 64]);
        h.evm_commitment_root = Hash64::from_bytes([10; 64]);
        h.overlay_commitment_root = Hash64::from_bytes([11; 64]);
        h.blue_hash_work = 4444.into();
        h.blue_compute_work = 5555.into();
        h.palw_batch_id = Hash64::from_bytes([12; 64]);
        h.palw_leaf_index = 13;
        h.palw_ticket_nullifier = Hash64::from_bytes([14; 64]);
        h.palw_epoch_certificate_hash = Hash64::from_bytes([15; 64]);
        h.palw_chain_commit = Hash64::from_bytes([16; 64]);
        h.palw_target_daa_interval = 17;
        h.palw_authorization_hash = Hash64::from_bytes([18; 64]);
        h.palw_proof_type = 2;
        h.palw_beacon_seed = Hash64::from_bytes([19; 64]);
        h.finalize();
        h
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

    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): proves the version gate in
    /// `write_header_preimage` — the EVM commitment root enters the header hash
    /// for v2+ headers only. This is the load-bearing property that keeps every
    /// existing v0/v1 genesis hash and block identity unchanged.
    #[test]
    fn evm_commitments_gated_by_header_version() {
        let mk = |version: u16| {
            Header::new_finalized(
                version,
                vec![vec![1.into()]].try_into().unwrap(),
                Default::default(),
                Default::default(),
                Default::default(),
                234,
                23,
                567,
                crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
                0,
                0.into(),
                0,
                Default::default(),
            )
        };
        let evm = |h: Header| h.with_evm_commitment(Hash64::from_bytes([4u8; 64]));
        let pay = |h: Header| h.with_evm_payload_hash(Hash64::from_bytes([7u8; 64]));

        // v1 (current BLOCK_VERSION): EVM commitments are hash-invisible.
        let v1 = mk(crate::constants::BLOCK_VERSION);
        assert!(crate::constants::BLOCK_VERSION < crate::constants::EVM_HEADER_VERSION);
        assert_eq!(evm(v1.clone()).hash, v1.hash, "v1 header hash must NOT change with EVM commitments");
        assert_eq!(pay(v1.clone()).hash, v1.hash, "v1 header hash must NOT change with an EVM payload hash");

        // v2 (EVM_HEADER_VERSION): both EVM commitments are part of the preimage.
        let v2 = mk(crate::constants::EVM_HEADER_VERSION);
        assert_ne!(evm(v2.clone()).hash, v2.hash, "v2 header hash MUST change with EVM commitments");
        assert_ne!(pay(v2.clone()).hash, v2.hash, "v2 header hash MUST change with an EVM payload hash");
        // The two fields occupy distinct preimage positions (payload_hash first,
        // then commitment_root — design v0.4 §4.3): swapping the same 64 bytes
        // between them must produce different hashes.
        let x = Hash64::from_bytes([4u8; 64]);
        let in_payload = v2.clone().with_evm_payload_hash(x);
        let in_commitment = v2.clone().with_evm_commitment(x);
        assert_ne!(in_payload.hash, in_commitment.hash, "payload_hash and commitment_root are position-distinct in the preimage");
        // Version itself participates in the preimage, so v1 != v2 even at zero EVM commitments.
        assert_ne!(v1.hash, v2.hash);
    }

    /// ADR-0039 PALW: proves the v3 gate in `write_header_preimage` — the PALW fields enter the
    /// header hash for v3+ headers only, so every existing v0/v1/v2 genesis hash and block identity
    /// is unchanged (the load-bearing inert property).
    #[test]
    fn palw_fields_gated_by_header_version() {
        use crate::header::PalwHeaderFields;
        let mk = |version: u16| {
            Header::new_finalized(
                version,
                vec![vec![1.into()]].try_into().unwrap(),
                Default::default(),
                Default::default(),
                Default::default(),
                234,
                23,
                567,
                crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
                0,
                0.into(),
                0,
                Default::default(),
            )
        };
        let some_fields = PalwHeaderFields {
            blue_hash_work: 7u64.into(),
            blue_compute_work: 5u64.into(),
            palw_batch_id: Hash64::from_bytes([1u8; 64]),
            palw_leaf_index: 9,
            palw_ticket_nullifier: Hash64::from_bytes([2u8; 64]),
            palw_epoch_certificate_hash: Hash64::from_bytes([3u8; 64]),
            palw_chain_commit: Hash64::from_bytes([4u8; 64]),
            palw_target_daa_interval: 42,
            palw_authorization_hash: Hash64::from_bytes([5u8; 64]),
            palw_proof_type: 1,
            palw_beacon_seed: Hash64::from_bytes([6u8; 64]),
            palw_spam_accumulator_commitment: Hash64::default(),
            palw_spam_nonce: 0,
            palw_compute_set_id: Hash64::default(),
            palw_compute_policy_id: Hash64::default(),
            palw_allocation_plan_id: Hash64::default(),
        };

        // v2 (EVM_HEADER_VERSION, < PALW): PALW fields are hash-invisible.
        let v2 = mk(crate::constants::EVM_HEADER_VERSION);
        assert!(crate::constants::EVM_HEADER_VERSION < crate::constants::PALW_HEADER_VERSION);
        assert_eq!(v2.clone().with_palw_fields(some_fields).hash, v2.hash, "v2 hash must NOT change with PALW fields");

        // v3 (PALW_HEADER_VERSION): PALW fields are part of the preimage.
        let v3 = mk(crate::constants::PALW_HEADER_VERSION);
        assert_ne!(v3.clone().with_palw_fields(some_fields).hash, v3.hash, "v3 hash MUST change with PALW fields");

        // each field is position-distinct: perturbing any single field changes the hash.
        let base = v3.clone().with_palw_fields(some_fields);
        let mutate = |edit: fn(&mut PalwHeaderFields)| {
            let mut f = some_fields;
            edit(&mut f);
            assert_ne!(v3.clone().with_palw_fields(f).hash, base.hash);
        };
        mutate(|f| f.blue_hash_work = 8u64.into());
        mutate(|f| f.blue_compute_work = 6u64.into());
        mutate(|f| f.palw_batch_id = Hash64::from_bytes([0x11; 64]));
        mutate(|f| f.palw_leaf_index = 10);
        mutate(|f| f.palw_ticket_nullifier = Hash64::from_bytes([0x22; 64]));
        mutate(|f| f.palw_epoch_certificate_hash = Hash64::from_bytes([0x33; 64]));
        mutate(|f| f.palw_chain_commit = Hash64::from_bytes([0x44; 64]));
        mutate(|f| f.palw_target_daa_interval = 43);
        mutate(|f| f.palw_authorization_hash = Hash64::from_bytes([0x55; 64]));
        mutate(|f| f.palw_proof_type = 2);
        mutate(|f| f.palw_beacon_seed = Hash64::from_bytes([0x66; 64]));

        // The anti-spam extension is v4-only and position-distinct. Existing v3 hashes stay inert.
        let mut spam = some_fields;
        spam.palw_spam_accumulator_commitment = Hash64::from_bytes([0x77; 64]);
        spam.palw_spam_nonce = 99;
        assert_eq!(v3.clone().with_palw_fields(spam).hash, base.hash);
        let v4 = mk(crate::constants::PALW_ANTISPAM_HEADER_VERSION);
        let v4_base = v4.clone().with_palw_fields(some_fields);
        let v4_accumulator = v4
            .clone()
            .with_palw_fields(PalwHeaderFields { palw_spam_accumulator_commitment: Hash64::from_bytes([0x77; 64]), ..some_fields });
        let v4_nonce = v4.clone().with_palw_fields(PalwHeaderFields { palw_spam_nonce: 99, ..some_fields });
        assert_ne!(v4_accumulator.hash, v4_base.hash);
        assert_ne!(v4_nonce.hash, v4_base.hash);
        assert_ne!(v4_nonce.hash, v4_accumulator.hash);

        // version participates: v2 != v3 even at zero PALW fields.
        assert_ne!(v2.hash, v3.hash);

        // ADR-MA Header-v5: the three Compute Set registry references are v5-only and
        // position-distinct. Existing v4 hashes stay inert under them.
        let mut refs = some_fields;
        refs.palw_compute_set_id = Hash64::from_bytes([0x88; 64]);
        refs.palw_compute_policy_id = Hash64::from_bytes([0x99; 64]);
        refs.palw_allocation_plan_id = Hash64::from_bytes([0xaa; 64]);
        assert_eq!(v4.clone().with_palw_fields(refs).hash, v4_base.hash, "v4 hash must NOT change with v5 fields");
        let v5 = mk(crate::constants::PALW_COMPUTE_SET_HEADER_VERSION);
        let v5_base = v5.clone().with_palw_fields(some_fields);
        let v5_set =
            v5.clone().with_palw_fields(PalwHeaderFields { palw_compute_set_id: Hash64::from_bytes([0x88; 64]), ..some_fields });
        let v5_policy =
            v5.clone().with_palw_fields(PalwHeaderFields { palw_compute_policy_id: Hash64::from_bytes([0x99; 64]), ..some_fields });
        let v5_plan =
            v5.clone().with_palw_fields(PalwHeaderFields { palw_allocation_plan_id: Hash64::from_bytes([0xaa; 64]), ..some_fields });
        assert_ne!(v5_set.hash, v5_base.hash);
        assert_ne!(v5_policy.hash, v5_base.hash);
        assert_ne!(v5_plan.hash, v5_base.hash);
        assert_ne!(v5_set.hash, v5_policy.hash);
        assert_ne!(v5_policy.hash, v5_plan.hash);
        // version participates: v4 != v5 even at identical fields.
        assert_ne!(v4_base.hash, v5_base.hash);
    }
}
