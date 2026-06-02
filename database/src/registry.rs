use enum_primitive_derive::Primitive;

/// We use `u8::MAX` which is never a valid block level. Also note that through
/// the [`DatabaseStorePrefixes`] enum we make sure it is not used as a prefix as well
pub const SEPARATOR: u8 = u8::MAX;

#[derive(Primitive, Debug, Clone, Copy)]
#[repr(u8)]
pub enum DatabaseStorePrefixes {
    // ---- Consensus ----
    AcceptanceData = 1,
    BlockTransactions = 2,
    NonDaaMergeset = 3,
    BlockDepth = 4,
    Ghostdag = 5,
    GhostdagCompact = 6,
    HeadersSelectedTip = 7,
    // Legacy headers store prefix. CompressedHeaders is used instead
    Headers = 8,
    HeadersCompact = 9,
    PastPruningPoints = 10,
    PruningUtxoset = 11,
    PruningUtxosetPosition = 12,
    PruningPoint = 13,
    RetentionCheckpoint = 14,
    Reachability = 15,
    ReachabilityReindexRoot = 16,
    ReachabilityRelations = 17,
    RelationsParents = 18,
    RelationsChildren = 19,
    ChainHashByIndex = 20,
    ChainIndexByHash = 21,
    ChainHighestIndex = 22,
    Statuses = 23,
    Tips = 24,
    UtxoDiffs = 25,
    UtxoMultisets = 26,
    VirtualUtxoset = 27,
    VirtualState = 28,
    PruningSamples = 29,

    // ---- Decomposed reachability stores ----
    ReachabilityTreeChildren = 30,
    ReachabilityFutureCoveringSet = 31,

    // Stores headers with run-length encoded parents
    CompressedHeaders = 32,

    // Stores a succinct pruning proof descriptor
    PruningProofDescriptor = 33,

    // ---- Ghostdag Proof
    TempGhostdag = 40,
    TempGhostdagCompact = 41,
    TempRelationsParents = 42,
    TempRelationsChildren = 43,

    // ---- Retention Period Root ----
    RetentionPeriodRoot = 50,

    // ---- Pruning metadata ----
    PruningUtxosetSyncFlag = 60,
    BodyMissingAnticone = 61,

    // ---- Metadata ----
    MultiConsensusMetadata = 124,
    ConsensusEntries = 125,

    // ---- Components ----
    Addresses = 128,
    BannedAddresses = 129,

    // ---- Indexes ----
    UtxoIndex = 192,
    UtxoIndexTips = 193,
    CirculatingSupply = 194,

    // ---- kaspa-pq DNS finality overlay (ADR-0009, Phase 10) ----
    /// Singleton: the per-anchor `DnsState` (work/stake depth, last
    /// DNS-confirmed anchor, rollout stage).
    DnsState = 195,
    /// Keyed by `TransactionOutpoint`: the active/unbonding/slashed
    /// `StakeBondRecord` set backing `StakeScore` and bond-existence checks.
    StakeBonds = 196,
    /// Keyed by `BlockHash`: the `(bond_outpoint, epoch)` pairs a chain block
    /// rewarded in its coinbase validator fan-out (ADR-0009 Addendum B §B.3(c)).
    /// Read by descendants' bounded-window uniqueness check so a `(bond,epoch)`
    /// is rewarded at most once across the selected chain; deleted on prune.
    RewardedEpochs = 197,

    // ---- kaspa-pq ADR-0018 "本格版" (PoS-v2 economics, Phase 1) ----
    /// Keyed by `u64` epoch: the per-epoch [`EpochTally`] accumulator
    /// (expected stake, included validators, accrued quality pool, finalized
    /// flag), recomputed from the selected-chain window at each virtual-state
    /// commit and read by the deferred §E quality-bonus payout. Inert
    /// (never written) until `pos_v2_activation_daa_score` (`u64::MAX` today).
    EpochAccumulator = 198,
    /// Keyed by `BlockHash`: the per-block validator **quality sub-pool**
    /// (`split_validator_pool(.).1`), the recompute input that the per-epoch
    /// accumulator sums (the per-block `validator_pool` is not cheaply
    /// re-derivable from a historical block). Written only past
    /// `pos_v2_activation_daa_score` (so inert on every net today); deleted on
    /// prune alongside `RewardedEpochs`.
    BlockValidatorQualityPool = 199,
    /// Keyed by `BlockHash`: the per-block **cumulative security-reserve balance**
    /// (`balance_after(block) = balance_after(selected_parent) + slashing-reserve
    /// accrual − drip`). The finalizing coinbase reads the selected parent's balance
    /// for the per-epoch reserve drip (so construction == validation without a
    /// lagging singleton). Written only past `pos_v2_activation_daa_score` (inert
    /// today); deleted on prune alongside `RewardedEpochs`.
    ReserveBalance = 200,

    // ---- Separator ----
    /// Reserved as a separator
    Separator = SEPARATOR,
}

impl From<DatabaseStorePrefixes> for Vec<u8> {
    fn from(value: DatabaseStorePrefixes) -> Self {
        [value as u8].to_vec()
    }
}

impl From<DatabaseStorePrefixes> for u8 {
    fn from(value: DatabaseStorePrefixes) -> Self {
        value as u8
    }
}

impl AsRef<[u8]> for DatabaseStorePrefixes {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: enum has repr(u8)
        std::slice::from_ref(unsafe { &*(self as *const Self as *const u8) })
    }
}

impl IntoIterator for DatabaseStorePrefixes {
    type Item = u8;
    type IntoIter = <[u8; 1] as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        [self as u8].into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_as_ref() {
        let prefix = DatabaseStorePrefixes::AcceptanceData;
        assert_eq!(&[prefix as u8], prefix.as_ref());
        assert_eq!(
            size_of::<u8>(),
            size_of::<DatabaseStorePrefixes>(),
            "DatabaseStorePrefixes is expected to have the same memory layout of u8"
        );
    }
}
