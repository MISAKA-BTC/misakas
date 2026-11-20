//! Nonce search for the isolated historical clock. Candidate blocks still use the normal PoW verifier.
use blake2b_simd::{Params, State};
use kaspa_consensus_core::{
    hashing,
    header::Header,
    pow_layer0::{
        PALW_HEARTBEAT_WORK_LOG2, POW_ALGO_ID_HEARTBEAT_V1, POW_FINALIZER_BYTES, POW_FINALIZER_DOMAIN, POW_L1_BLAKE2B_SHA3_OUT_BYTES,
        POW_L1_BLAKE2B_SHA3_V1_DOMAIN,
    },
};
use kaspa_math::Uint512;
use sha3::{Digest, Sha3_512};

pub(super) struct PreparedHeartbeat {
    blake_prefix: State,
    sha_prefix: Sha3_512,
    finalizer_prefix: State,
    target: Uint512,
}

impl PreparedHeartbeat {
    pub(super) fn new(header: &Header, network_id: &[u8]) -> Self {
        assert_eq!(header.pow_algo_id, POW_ALGO_ID_HEARTBEAT_V1);
        let network_len = (network_id.len() as u16).to_le_bytes();
        let prehash = hashing::header::pre_pow_hash_64(header);
        let prehash = prehash.as_byte_slice();
        let mut blake_prefix = Params::new().key(POW_L1_BLAKE2B_SHA3_V1_DOMAIN).to_state();
        blake_prefix.update(&network_len).update(network_id).update(prehash);
        let mut sha_prefix = Sha3_512::new();
        sha_prefix.update(POW_L1_BLAKE2B_SHA3_V1_DOMAIN);
        sha_prefix.update(network_len);
        sha_prefix.update(network_id);
        sha_prefix.update(prehash);

        let mut finalizer_prefix = Params::new().key(POW_FINALIZER_DOMAIN).to_state();
        finalizer_prefix
            .update(&network_len)
            .update(network_id)
            .update(&[header.pow_algo_id])
            .update(prehash)
            .update(&header.timestamp.to_le_bytes())
            .update(&header.bits.to_le_bytes());
        Self { blake_prefix, sha_prefix, finalizer_prefix, target: Uint512::MAX >> PALW_HEARTBEAT_WORK_LOG2 }
    }

    #[inline]
    pub(super) fn solves(&self, nonce: u64) -> bool {
        Uint512::from_le_bytes(self.digest(nonce)) <= self.target
    }

    fn digest(&self, nonce: u64) -> [u8; POW_FINALIZER_BYTES] {
        let nonce = nonce.to_le_bytes();
        let blake = self.blake_prefix.clone().update(&nonce).finalize();
        let sha = self.sha_prefix.clone().chain_update(nonce).finalize();
        let digest = self
            .finalizer_prefix
            .clone()
            .update(&nonce)
            .update(&(POW_L1_BLAKE2B_SHA3_OUT_BYTES as u16).to_le_bytes())
            .update(blake.as_bytes())
            .update(&sha)
            .finalize();
        let mut bytes = [0; POW_FINALIZER_BYTES];
        bytes.copy_from_slice(digest.as_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_pow::StateLayer0;

    #[test]
    fn prepared_heartbeat_matches_verifier() {
        let header = Header::new_finalized(
            1,
            vec![vec![1.into()]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            42,
            0x207fffff,
            0,
            POW_ALGO_ID_HEARTBEAT_V1,
            0,
            0.into(),
            0,
            Default::default(),
        );
        for network in [b"testnet-12".as_slice(), b"simnet".as_slice()] {
            let pow = StateLayer0::new(&header, network);
            let prepared = PreparedHeartbeat::new(&header, network);
            for nonce in (0..256).chain([u32::MAX.into(), u64::MAX]) {
                assert_eq!(prepared.digest(nonce), pow.calculate_pow_layer0(nonce).unwrap());
                assert_eq!(prepared.solves(nonce), pow.check_pow_layer0(nonce).unwrap().0);
            }
        }
    }
}
