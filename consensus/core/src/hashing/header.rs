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
/// MISAKA ADR-0038: whether `header.palw_commitment` participates in this digest. The
/// commitment is a function of the winning nonce, so it is a **post-PoW** field: it enters
/// the block-identity digest only, and never any PoW-path digest (all three
/// `*_override_nonce_time*` functions and the pre-PoW hash pass `Exclude`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PalwCommitmentDigestRule {
    Include,
    Exclude,
}

#[inline]
fn write_header_preimage<H: HasherBase>(
    hasher: &mut H,
    header: &Header,
    nonce: u64,
    timestamp: u64,
    palw_rule: PalwCommitmentDigestRule,
) {
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

    // MISAKA ADR-0038 Decision A: the PALW block commitment. Double-gated:
    //   * by ALGO — only PALW headers (`pow_algo_id` 4/5) hash it, so every
    //     hash-algo network's preimage (and genesis hash) is byte-identical to
    //     the pre-ADR-0038 protocol. The gate is itself a function of
    //     `pow_algo_id`, which is already in the preimage above, so inclusion is
    //     deterministic from committed bytes. On non-PALW headers the field is
    //     hash-invisible AND `pow_layer0::check_palw_commitment_shape` requires
    //     it empty at validation — hash-invisible non-empty bytes would be
    //     block-hash malleability.
    //   * by DIGEST — `Include` only on the block-identity path. The commitment
    //     is a function of the winning nonce (ADR-0038: it cannot sit under the
    //     merkle root, and it cannot precede the grind), so every PoW-path
    //     digest passes `Exclude`.
    // Length-prefixed so empty-vs-absent and boundary shifts are distinct.
    // PALW soak networks adopt this via re-genesis (their genesis carries
    // `pow_algo_id = 4`, so their old genesis hashes do not survive — recorded
    // in ADR-0038; hash-algo networks are untouched).
    if palw_rule == PalwCommitmentDigestRule::Include && crate::pow_layer0::is_palw_algo_id(header.pow_algo_id) {
        hasher.write_len(header.palw_commitment.len());
        hasher.update(&header.palw_commitment);
    }
}

/// Returns the **legacy 32-byte** header hash using the provided
/// nonce+timestamp. Retained only for the 32-byte kHeavyHash PoW path
/// in `consensus/pow`; the canonical block *identity* is the 64-byte
/// [`hash`] below (ADR-0008).
#[inline]
pub fn hash_override_nonce_time(header: &Header, nonce: u64, timestamp: u64) -> Hash {
    let mut hasher = kaspa_hashes::BlockHash::new();
    // PoW path: the PALW commitment never enters (ADR-0038 post-PoW field).
    write_header_preimage(&mut hasher, header, nonce, timestamp, PalwCommitmentDigestRule::Exclude);
    hasher.finalize()
}

/// Returns the 64-byte block-identity hash (ADR-0008). Uses the keyed
/// BLAKE2b-512 `BlockHash64` domain over all header fields including
/// the real nonce/timestamp. This is what `Header::hash` caches and
/// what keys every block store / GHOSTDAG / reachability structure.
pub fn hash(header: &Header) -> Hash64 {
    let mut hasher = kaspa_hashes::BlockHash64::new();
    // Block identity: the ONLY digest the PALW commitment enters (ADR-0038).
    write_header_preimage(&mut hasher, header, header.nonce, header.timestamp, PalwCommitmentDigestRule::Include);
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
    // PoW path: the PALW commitment never enters (ADR-0038 post-PoW field).
    write_header_preimage(&mut hasher, header, nonce, timestamp, PalwCommitmentDigestRule::Exclude);
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

    /// MISAKA ADR-0038: the palw_commitment digest rules — the load-bearing properties of
    /// the layer inversion's header change, pinned:
    ///   1. on a PALW header the commitment moves the block identity;
    ///   2. on a PALW header it NEVER moves any PoW-path digest (post-PoW field);
    ///   3. on a non-PALW header it is hash-invisible (every existing hash-algo network's
    ///      block identity and genesis hash are unchanged — validation separately refuses
    ///      the non-empty case as malleability);
    ///   4. the length prefix separates empty-carried from distinct commitments.
    #[test]
    fn palw_commitment_digest_rules() {
        let mk = |algo: u8| {
            Header::new_finalized(
                crate::constants::BLOCK_VERSION,
                vec![vec![1.into()]].try_into().unwrap(),
                Default::default(),
                Default::default(),
                Default::default(),
                234,
                23,
                567,
                algo,
                0,
                0.into(),
                0,
                Default::default(),
            )
        };

        // (1) PALW header: identity moves with the commitment, and with its content.
        let palw = mk(crate::pow_layer0::POW_ALGO_ID_PALW_LLM);
        let with_a = palw.clone().with_palw_commitment(vec![0xAA; 100]);
        let with_b = palw.clone().with_palw_commitment(vec![0xBB; 100]);
        assert_ne!(with_a.hash, palw.hash, "PALW identity MUST move with the commitment");
        assert_ne!(with_a.hash, with_b.hash, "PALW identity MUST move with the commitment bytes");

        // (2) PoW-path digests ignore it entirely (the commitment is a function of the
        // winning nonce — it cannot precede the grind).
        assert_eq!(pre_pow_hash_64(&with_a), pre_pow_hash_64(&palw), "pre-PoW hash must NOT see the commitment");
        assert_eq!(
            hash_override_nonce_time(&with_a, 567, 234),
            hash_override_nonce_time(&palw, 567, 234),
            "legacy PoW digest must NOT see the commitment"
        );
        assert_eq!(
            hash_override_nonce_time_64(&with_a, 567, 234),
            hash_override_nonce_time_64(&palw, 567, 234),
            "64-byte PoW digest must NOT see the commitment"
        );

        // (3) Non-PALW header: hash-invisible (identity unchanged) — paired with
        // `check_palw_commitment_shape`, which refuses the non-empty case at validation.
        let khh = mk(crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH);
        assert_eq!(khh.clone().with_palw_commitment(vec![0xAA; 100]).hash, khh.hash, "non-PALW identity must NOT move");
        assert!(crate::pow_layer0::check_palw_commitment_shape(crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH, &[0xAA; 100]).is_err());
        assert!(crate::pow_layer0::check_palw_commitment_shape(crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH, &[]).is_ok());

        // (4) Ollama-PALW (algo 5) hashes it too, and empty-carried is length-prefixed so a
        // PALW header WITH an empty commitment still differs from... itself only via bytes —
        // i.e. empty is a committed state, not an absent one.
        let ollama = mk(crate::pow_layer0::POW_ALGO_ID_PALW_OLLAMA);
        assert_ne!(ollama.clone().with_palw_commitment(vec![0x01]).hash, ollama.hash);

        // Shape rule: PALW side accepts empty and the cap, refuses above it.
        assert!(crate::pow_layer0::check_palw_commitment_shape(crate::pow_layer0::POW_ALGO_ID_PALW_LLM, &[]).is_ok());
        assert!(
            crate::pow_layer0::check_palw_commitment_shape(
                crate::pow_layer0::POW_ALGO_ID_PALW_LLM,
                &vec![0u8; crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES]
            )
            .is_ok()
        );
        assert!(
            crate::pow_layer0::check_palw_commitment_shape(
                crate::pow_layer0::POW_ALGO_ID_PALW_LLM,
                &vec![0u8; crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES + 1]
            )
            .is_err()
        );
    }

    /// MISAKA ADR-0038: a real PBC1 payload rides the header end-to-end — encode, carry,
    /// decode, and the header identity binds the exact bytes.
    #[test]
    fn palw_commitment_carries_pbc1_roundtrip() {
        use crate::palw_block_commitment::{PALW_BLOCK_COMMITMENT_VERSION_V1, PalwBlockCommitmentV1};
        let commitment = PalwBlockCommitmentV1 {
            version: PALW_BLOCK_COMMITMENT_VERSION_V1,
            execution_class_id: Hash64::from_u64_word(1),
            executor_bond_outpoint: crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(2), 3),
            trace_root: Hash64::from_u64_word(4),
            output_root: Hash64::from_u64_word(5),
            pwu_claim: 100,
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        };
        let bytes = commitment.encode();
        assert!(bytes.len() <= crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES, "PBC1 must fit the header cap");
        assert!(crate::pow_layer0::check_palw_commitment_shape(crate::pow_layer0::POW_ALGO_ID_PALW_LLM, &bytes).is_ok());
        let header = Header::new_finalized(
            crate::constants::BLOCK_VERSION,
            vec![vec![1.into()]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            234,
            23,
            567,
            crate::pow_layer0::POW_ALGO_ID_PALW_LLM,
            0,
            0.into(),
            0,
            Default::default(),
        )
        .with_palw_commitment(bytes);
        let decoded = PalwBlockCommitmentV1::decode(&header.palw_commitment).unwrap();
        assert_eq!(decoded, commitment);
    }
}
