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

fn publish_private_file(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
    let written = file.write_all(bytes);
    drop(file);
    let result = written.and_then(|_| std::fs::rename(&temporary, path));
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

/// Report the ordinary slot and attempt-priority rule's absolute timestamp
/// once, for the controller's current parent and DAA score.
pub(super) fn report_slot_request(path: &std::path::Path, permit: &super::HistoricalHeartbeatPermit, opens_ms: u64) {
    let request = path.with_file_name(format!("heartbeat-slot-{}-{}.request", permit.sink, permit.daa));
    if !request.is_file() {
        return;
    }
    let reply = serde_json::json!({"sink": permit.sink, "daa": permit.daa, "opensMs": opens_ms});
    if publish_private_file(&path.with_file_name("heartbeat-slot.json"), reply.to_string().as_bytes()).is_ok() {
        let _ = std::fs::remove_file(request);
    }
}

/// Optional local nonce source for the isolated clock's heartbeat miner. Only the
/// public prehash and PoW fields leave the process. A missing or stale reply
/// leaves the ordinary CPU search running. The caller verifies every proposed
/// nonce against the current header with the native PoW verifier.
pub(super) struct PrivateNonceSource {
    answer: std::path::PathBuf,
}

impl PrivateNonceSource {
    pub(super) fn for_header(header: &Header, network_id: &str) -> Option<Self> {
        let directory = std::path::PathBuf::from(std::env::var_os("MISAKA_PRIVATE_HEARTBEAT_NONCE_DIR")?);
        if !directory.is_absolute() {
            return None;
        }
        let prehash = hashing::header::pre_pow_hash_64(header);
        let work = serde_json::json!({
            "version": 1, "network": network_id, "prehash": faster_hex::hex_string(prehash.as_byte_slice()),
            "timestamp": header.timestamp, "bits": header.bits, "algo": header.pow_algo_id,
        });
        publish_private_file(&directory.join("work.json"), work.to_string().as_bytes()).ok()?;
        Some(Self { answer: directory.join("nonce.le64") })
    }

    fn candidate(&self) -> Option<u64> {
        use std::io::Read;
        let metadata = std::fs::symlink_metadata(&self.answer).ok()?;
        let mut bytes = [0u8; 8];
        if !metadata.is_file() || metadata.len() != bytes.len() as u64 {
            return None;
        }
        std::fs::File::open(&self.answer).ok()?.read_exact(&mut bytes).ok()?;
        Some(u64::from_le_bytes(bytes))
    }

    pub(super) fn verified(&self, solves: impl FnOnce(u64) -> bool) -> Option<u64> {
        self.candidate().filter(|&nonce| solves(nonce))
    }
}

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
    fn slot_report_requires_a_request_for_this_selected_parent_and_score() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("heartbeat-permit.json");
        let reply = dir.path().join("heartbeat-slot.json");
        let permit = super::super::HistoricalHeartbeatPermit { sink: Default::default(), daa: 17, transaction_id: None };
        std::fs::write(dir.path().join(format!("heartbeat-slot-{}-16.request", permit.sink)), []).unwrap();
        report_slot_request(&path, &permit, 250_000);
        assert!(!reply.exists());
        let current = dir.path().join(format!("heartbeat-slot-{}-17.request", permit.sink));
        std::fs::write(&current, []).unwrap();
        report_slot_request(&path, &permit, 250_000);
        let actual: serde_json::Value = serde_json::from_slice(&std::fs::read(reply).unwrap()).unwrap();
        assert_eq!(actual, serde_json::json!({"sink": permit.sink, "daa": 17, "opensMs": 250_000}));
        assert!(!current.exists());
    }

    #[test]
    fn optional_nonce_requires_a_complete_reply_and_current_header_verification() {
        let directory = tempfile::tempdir().unwrap();
        let source = PrivateNonceSource { answer: directory.path().join("nonce.le64") };
        assert!(source.verified(|_| true).is_none());
        for malformed in [&[0u8; 7][..], &[0u8; 9][..]] {
            std::fs::write(&source.answer, malformed).unwrap();
            assert!(source.verified(|_| true).is_none());
        }
        std::fs::write(&source.answer, 42u64.to_le_bytes()).unwrap();
        assert!(source.verified(|n| n == 41).is_none());
        assert_eq!(source.verified(|n| n == 42), Some(42));
    }

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
