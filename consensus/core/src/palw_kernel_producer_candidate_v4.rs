//! Opaque own-node producer rehearsal. Its candidate is neither a signature nor
//! accepted state/credit. Only native replay can construct it; no wire ledger
//! or caller-projected collateral/key is admitted here.
use crate::{
    Hash64,
    palw_native_resource_v1::{NativeRetainedV1, ReservedKernelLedgerV1},
    palw_prospective_kernel_material_v4::{commit_identity_v4, object_hash, prepare_unsigned_kernel_candidate_v4},
    palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2},
};
use misaka_palw_kernel::{hash::Digest, ledger::KernelLedgerV1, route::SaltedCommitV1};
pub struct PalwKernelProducerCandidateOwnedV4 {
    candidate: ReservedKernelLedgerV1,
    pubkey: NativeRetainedV1<Vec<u8>>,
    work: Hash64,
    claim: Digest,
    checkpoint: Hash64,
    ledger_root: Hash64,
    aux_root: Hash64,
    network: Hash64,
    producer: PalwBondKeyV2,
    commit_daa: u64,
    expires_daa: u64,
}
impl PalwKernelProducerCandidateOwnedV4 {
    pub fn checkpoint(&self) -> Hash64 {
        self.checkpoint
    }
    pub fn ledger_root(&self) -> Hash64 {
        self.ledger_root
    }
    pub fn aux_root(&self) -> Hash64 {
        self.aux_root
    }
    pub fn network(&self) -> Hash64 {
        self.network
    }
    pub fn producer(&self) -> PalwBondKeyV2 {
        self.producer
    }
    pub fn commit_daa(&self) -> u64 {
        self.commit_daa
    }
    pub fn expires_daa(&self) -> u64 {
        self.expires_daa
    }
    pub fn ledger(&self) -> &KernelLedgerV1 {
        &self.candidate
    }
    pub fn producer_pubkey(&self) -> &[u8] {
        &self.pubkey
    }
    pub fn validate_work(&self, commit: &SaltedCommitV1, salt: &Digest) -> Result<(), String> {
        if object_hash(b"MISAKA/PALW/KERNEL-PRODUCER-WORK/V4", &(commit, salt)) != self.work {
            return Err("producer candidate belongs to different work/salt".into());
        }
        Ok(())
    }
    pub fn reserve_material_v4(&mut self) -> Result<(), String> {
        self.candidate.try_reserve_kernel_material_v4(&self.claim).map_err(|e| e.to_string())
    }
}
#[allow(clippy::too_many_arguments)]
pub fn kernel_producer_candidate_v4(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    checkpoint: Hash64,
    now: u64,
    producer: PalwBondKeyV2,
    commit: &SaltedCommitV1,
    salt: &Digest,
    commit_daa: u64,
    expires_daa: u64,
    raw_depth: Option<u64>,
) -> Result<PalwKernelProducerCandidateOwnedV4, String> {
    let policy = params.monetary_material_at_v1(now).ok_or("producer material policy dormant")?;
    if params.material_release_recovery_at_v2(now).is_none()
        || commit_daa < now
        || expires_daa < commit_daa
        || expires_daa - commit_daa > policy.max_enrollment_daa
    {
        return Err("producer material lifetime/recovery policy invalid".into());
    }
    let route = state.kernel_route().ok_or("producer native route absent")?;
    let key = &state.bond(&producer).ok_or("producer native bond absent")?.pubkey;
    let candidate = prepare_unsigned_kernel_candidate_v4(state, params, &producer, commit, salt, commit_daa, raw_depth)?;
    Ok(PalwKernelProducerCandidateOwnedV4 {
        candidate,
        pubkey: NativeRetainedV1::try_from_clone_v1(key).map_err(|e| e.to_string())?,
        work: object_hash(b"MISAKA/PALW/KERNEL-PRODUCER-WORK/V4", &(commit, salt)),
        claim: commit_identity_v4(commit).0,
        checkpoint,
        ledger_root: route.ledger_root(),
        aux_root: route.aux_root(),
        network: Hash64::from_bytes(route.header.policy.network_domain),
        producer,
        commit_daa,
        expires_daa,
    })
}
