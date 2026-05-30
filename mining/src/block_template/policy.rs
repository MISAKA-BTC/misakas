/// Policy houses the policy (configuration parameters) which is used to control
/// the generation of block templates. See the documentation for
/// NewBlockTemplate for more details on how each of these parameters are used.
#[derive(Clone)]
pub struct Policy {
    /// max_block_mass is the maximum block mass to be used when generating a block template.
    pub(crate) max_block_mass: u64,

    /// kaspa-pq Phase 10 (ADR-0009 §"Why partial certificates"): the maximum
    /// number of `StakeAttestationShard` transactions to include in a single
    /// block template. `0` means unlimited (the overlay is off / unconfigured).
    /// Each shard is itself bounded to `MAX_ATTESTATIONS_PER_SHARD` attestations
    /// by PR-10.4 stateless validation, so capping shard txs bounds the
    /// per-block attestation budget without decoding payloads here.
    pub(crate) max_attestation_shard_txs: u64,
}

impl Policy {
    pub fn new(max_block_mass: u64) -> Self {
        Self { max_block_mass, max_attestation_shard_txs: 0 }
    }

    /// Sets the per-block `StakeAttestationShard` tx budget (`0` = unlimited).
    /// Sourced from `DnsParams::max_attestations_per_block` once the overlay is
    /// wired into the mining `Config` (follow-up); inert (unlimited) by default.
    pub fn with_max_attestation_shard_txs(mut self, max_attestation_shard_txs: u64) -> Self {
        self.max_attestation_shard_txs = max_attestation_shard_txs;
        self
    }
}
