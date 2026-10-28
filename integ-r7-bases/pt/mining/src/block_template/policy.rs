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
    ///
    /// kaspa-pq audit v24 (H-2/H-3): this is the *remaining* per-block shard-tx
    /// budget for whichever selector consumes the policy. When the
    /// [`AttestationPrioritySelector`](crate::mempool::model::frontier::selectors::AttestationPrioritySelector)
    /// pre-selects shards, the inner selector's policy carries the budget already
    /// consumed subtracted out, so the two lanes never double-count toward the cap.
    pub(crate) max_attestation_shard_txs: u64,

    /// kaspa-pq audit v24 (H-3/M-4): the per-block `StakeAttestationShard` mass
    /// budget. `0` means unlimited (the overlay is off / unconfigured). Enforced
    /// uniformly across ALL selector paths (TakeAll, Sequence, Rebalancing) via a
    /// shared post-selection cap, mirroring `max_attestation_shard_txs`. Like the
    /// tx count this is the *remaining* budget when the priority lane already
    /// consumed some shard mass.
    pub(crate) max_attestation_shard_mass: u64,
}

impl Policy {
    pub fn new(max_block_mass: u64) -> Self {
        Self { max_block_mass, max_attestation_shard_txs: 0, max_attestation_shard_mass: 0 }
    }

    /// Sets the per-block `StakeAttestationShard` tx budget (`0` = unlimited).
    /// Sourced from `DnsParams::max_attestations_per_block` once the overlay is
    /// wired into the mining `Config` (follow-up); inert (unlimited) by default.
    pub fn with_max_attestation_shard_txs(mut self, max_attestation_shard_txs: u64) -> Self {
        self.max_attestation_shard_txs = max_attestation_shard_txs;
        self
    }

    /// kaspa-pq audit v24 (H-3/M-4): sets the per-block `StakeAttestationShard`
    /// mass budget (`0` = unlimited). Enforced uniformly across every selector
    /// path so a `TakeAll`/`Sequence` selector cannot bypass the mass cap that the
    /// rebalancing selector and the priority lane already honor.
    pub fn with_max_attestation_shard_mass(mut self, max_attestation_shard_mass: u64) -> Self {
        self.max_attestation_shard_mass = max_attestation_shard_mass;
        self
    }
}
