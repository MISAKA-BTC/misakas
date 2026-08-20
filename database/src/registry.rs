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
    /// commit and read by the deferred §E quality-bonus payout. Gated by
    /// `pos_v2_activation_daa_score`: inert (never written) on devnet/simnet
    /// (`GENESIS_ACTIVE_DNS_PARAMS`, fence `u64::MAX`); written from block 1 on
    /// mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence `0` — v2 active).
    EpochAccumulator = 198,
    /// Keyed by `BlockHash`: the per-block validator **quality sub-pool**
    /// (`split_validator_pool(.).1`), the recompute input that the per-epoch
    /// accumulator sums (the per-block `validator_pool` is not cheaply
    /// re-derivable from a historical block). Written only past
    /// `pos_v2_activation_daa_score` (inert on devnet/simnet with fence `u64::MAX`;
    /// written from block 1 on mainnet/testnet with fence `0`); deleted on
    /// prune alongside `RewardedEpochs`.
    BlockValidatorQualityPool = 199,
    /// Keyed by `BlockHash`: the per-block **cumulative security-reserve balance**
    /// (`balance_after(block) = balance_after(selected_parent) + slashing-reserve
    /// accrual − drip`). The finalizing coinbase reads the selected parent's balance
    /// for the per-epoch reserve drip (so construction == validation without a
    /// lagging singleton). Written only past `pos_v2_activation_daa_score` (inert on
    /// devnet/simnet with fence `u64::MAX`; written from block 1 on mainnet/testnet
    /// with fence `0`); deleted on prune alongside `RewardedEpochs`.
    ReserveBalance = 200,

    // ---- kaspa-pq Selected-Parent EVM Lane (ADR-0020) ----
    // Defined in P1 (consensus types); the stores themselves are wired in the
    // EVM stores phase (P3). All keyed by the L1 `BlockHash` unless noted, so an
    // EVM result is append-only per block (no re-execution on virtual reorg).
    /// Keyed by `BlockHash`: the per-block `EvmExecutionHeader`.
    EvmHeader = 201,
    /// Keyed by `BlockHash`: the post-execution EVM state-trie root (fast path
    /// for fetching a selected parent's root).
    EvmStateRoots = 202,
    /// Keyed by `BlockHash`: the per-block EVM transaction receipts.
    EvmReceipts = 203,
    /// Keyed by EVM tx hash: `(BlockHash, index)` locations (side branches
    /// allowed; canonical query resolved via the head tag).
    EvmTxLookup = 204,
    /// Logs index for `eth_getLogs` acceleration.
    EvmLogs = 205,
    /// Keyed by `BlockHash`: the per-block EVM state change set (flat-state /
    /// pruning / debug).
    EvmStateDiff = 206,
    /// Keyed by `BlockHash`: EVM → UTXO withdrawal records materialized by the
    /// block (audit + RPC + UTXO outpoint correspondence).
    EvmWithdrawals = 207,
    /// Keyed by `BlockHash`: UTXO → EVM deposit records reflected by the block
    /// (`system_ops_root` verification + audit + RPC).
    EvmDeposits = 208,
    /// Singleton: the canonical EVM heads (`latest_unsafe` / `safe` /
    /// `finalized`) used to resolve Ethereum block tags.
    EvmCanonicalHeads = 209,
    /// Keyed by EVM block hash: the L1 `BlockHash` (for `eth_getBlockByHash`).
    EvmBlockHashMap = 210,
    /// Keyed by `BlockHash`: the block's own `EvmExecutionPayload` (v0.4 §3.1),
    /// persisted at body validation. The virtual processor reads MERGESET
    /// blocks' payloads from here to assemble `AcceptedEvmTxs(B)` — a chain
    /// block's acceptance executes OTHER blocks' payloads, which the chain
    /// block's own body cannot supply.
    EvmPayload = 211,
    /// kaspa-pq ADR-0022: singleton holding the DNS/PoS-v2 `OverlaySnapshot`
    /// as-of the current pruning point (`PruningPointOverlaySnapshot`), captured
    /// at pruning-advance before the below-pp overlay rows are deleted. Served to
    /// peers during their headers-proof IBD and consulted by `compute_overlay_snapshot`
    /// when its selected-chain walk reaches the pruning point (the below-pp window
    /// is otherwise unreachable post-prune / post-import).
    PruningPointOverlaySnapshot = 212,
    /// kaspa-pq EVM Lane (§16, eth-rpc): keyed by `evm_number` (u64 BE) → the L1
    /// `BlockHash` of the chain block with that EVM number (for `eth_getBlockByNumber`
    /// + `eth_getLogs` ranges). Upserted per chain block at commit; on a reorg the new
    /// canonical block at a number overwrites the old, and the reader validates
    /// `is_chain_block(hash) && header(hash).evm_number == n` so a stale row reads as
    /// absent (same canonical-resolution pattern as `get_evm_tx_receipt`). RPC index
    /// only — never part of any commitment.
    EvmNumberIndex = 213,

    /// kaspa-pq EVM Lane (§16, audit R-2): keyed by EVM `tx_hash` → the raw
    /// EIP-2718 bytes (+ originating payload block), so
    /// `eth_getTransactionByHash`/receipt resolve a tx by hash without the
    /// bounded `EvmTxLookup.included_in` scan. RPC index only — never part of any
    /// commitment. (214–216 are reserved for the RPC canonical-v2 block-meta /
    /// journal stores, not yet built.)
    EvmRawTransaction = 217,

    /// kaspa-pq EVM Lane (§16, design §8/§14): singleton — the lowest `evm_number`
    /// from which the `EvmLogs` posting index is complete (the writer's floor). The
    /// `eth_getLogs` index fast path is used only for `from >= floor`; below it the
    /// query falls back to the canonical scan, so a chain indexed mid-life never
    /// silently drops logs. RPC index only — never part of any commitment.
    EvmLogIndexMeta = 218,

    /// kaspa-pq EVM Lane (§16, design §11) — keyed by the accepting L1 `BlockHash`:
    /// the per-block [`EvmTraceReplayBodyV1`] (env inputs + system ops + the full
    /// ordered acceptance-candidate list), the deterministic replay plan that lets
    /// `debug_traceTransaction` re-execute a tx with a revm inspector against the
    /// selected parent's committed post-state. Written in the same commit batch as
    /// the EVM result (atomic, inert pre-activation); deleted on prune alongside the
    /// per-block state/header/receipts. RPC/replay data only — never part of any
    /// commitment.
    EvmTraceReplay = 219,

    // ---- MISAKA Verified LLM Token-Weighted BFT (`vlt`) ----
    /// Keyed by `u64` epoch: the per-epoch verified-compute credit `X_i(epoch)`
    /// (`VltEpochCredits`), written ONLY once an epoch is buried past both the challenge window
    /// and the reorg horizon — i.e. once no challenge and no branch can still change it, which
    /// is what makes one epoch-keyed row valid for every branch. Turns the `C_i(E)` sum from a
    /// full `credit_window_epochs`-deep re-walk (re-verifying every certificate's ML-DSA-87
    /// signatures) into a walk of the unfinalized tail. Inert (never written) while
    /// `DnsParams::vlt.vlt_activation_daa_score` is `u64::MAX`, which it is on every shipped
    /// preset.
    VltCredits = 235,

    /// Singleton `u32`: the derivation rules the [`Self::VltCredits`] rows were produced under.
    ///
    /// Those rows are write-once and derived, which is a dangerous pair — a bug in the derivation
    /// is not corrected by fixing the bug, because the wrong answer has already been recorded as
    /// final. This is the escape: bump the version and every row from the old rules is dropped and
    /// re-derived, while the chain, bonds, commitments, certificates, verdicts and challenges they
    /// came from are untouched.
    VltCreditsSchemaVersion = 236,

    /// Keyed by the declaring transaction: an accepted [`ComputeCapabilityRecord`].
    ///
    /// A capability is valid for `max_capability_validity_blocks` (a day of blocks), while the
    /// credit walk spans `vlt_credit_window_blue_score` (minutes). Deriving the verifier committee
    /// from declarations the walk happens to have collected therefore loses the pool the moment the
    /// walk floor rises past a declaration — and a certificate whose committee cannot be redrawn
    /// reads as unverified, however many honest verdicts it has. Stored, like the bonds it is
    /// filtered against, and queried at the beacon's own DAA.
    ComputeCapabilities = 237,

    /// Singleton flag: history has been swept into [`Self::ComputeCapabilities`]. A database that
    /// predates that store has declarations on chain and no rows for them, and an empty candidate
    /// pool reads exactly like "nobody declared this profile".
    ComputeCapabilitiesBackfilled = 238,

    /// Singleton `u32`: the record layout the [`Self::ComputeCapabilities`] rows were written
    /// under. A borsh layout change makes existing rows undecodable, and an undecodable row is
    /// dropped silently — so without this the store reads as empty, which is a wrong answer that
    /// looks like a valid one.
    ComputeCapabilitiesSchema = 239,

    /// kaspa-pq EVM Lane (§12 archive) — keyed by canonical `BlockHash`: the forward
    /// state DIFF ([`EvmStateDiffV2`]) of the block over its selected parent. The
    /// long-term retention form (the per-block full snapshot at prefix 206 is the
    /// hot/reorg-window form); reconstructed historical state replays these from the
    /// nearest checkpoint. RPC/archive data only — never part of any commitment.
    EvmStateDiffV2 = 220,
    /// kaspa-pq EVM Lane (§12 archive) — keyed by `BlockHash`: a periodic full-state
    /// [`EvmStateCheckpointV1`] (≈ every 2048 canonical blocks + at pruning advance),
    /// the seed a historical reconstruction starts from. RPC/archive data only.
    EvmStateCheckpoint = 221,
    /// kaspa-pq EVM Lane (§12 archive) — content-addressed `code_hash → code` store so
    /// diffs/checkpoints carry only the code hash. RPC/archive data only.
    EvmCode = 222,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 1) — singleton `EvmLatestStatePtr`:
    /// the block whose `state_root` the flat state currently materializes. State data.
    EvmLatestStatePtr = 231,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 1) — `BlockHash → state_root[32]`:
    /// O(1) lookup of any committed block's EVM state root. State/RPC data.
    EvmBlockStateRoot = 232,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 1) — `EvmAddress → FlatAccount`: the
    /// flat LATEST-canonical state (one row per account, NOT per block), replacing the
    /// per-block O(state × blocks) snapshot. Code is content-addressed (222). State data.
    EvmFlatAccount = 234,

    /// Singleton `PersistedChainParticipation`: whether this node is entitled to mine, attest, or
    /// call itself synced, and why not.
    ///
    /// Lives in the node-level **meta** DB rather than a consensus DB on purpose. The state it
    /// records is precisely "an IBD may have replaced my consensus and I have not settled whether
    /// that was right", so storing it inside a consensus that a `staging.commit()` can swap out
    /// would lose it exactly when it matters. Held across restarts because a quarantine that a
    /// process restart clears is not a quarantine.
    ///
    /// 236 on the branch this arrived on, which [`Self::VltCreditsSchemaVersion`] had already taken
    /// on the branch it merged with. The two never share a keyspace — this one is in the meta DB
    /// and that one in a consensus DB — so nothing on disk was ever ambiguous, but the enum cannot
    /// carry the value twice. This side moved because its only on-disk instances are regression
    /// fixtures that are rebuilt per round, whereas the VLT rows may already exist under 236 in a
    /// deployed consensus DB. A node upgrading across this change finds no row at 240 and starts
    /// from the unset default, which is the same state a first boot presents.
    ChainParticipation = 240,

    /// Singleton `VltActivationRecord`: where this consensus is on the §6 VLT activation state
    /// machine — awaiting an eligible snapshot, holding a reservation for the next epoch, or
    /// activated (terminal). Written by the virtual processor's per-epoch DNS recompute in the
    /// same batch as [`Self::DnsState`]; read back at the next recompute and after a restart, so
    /// a committed reservation survives the process and an activated network can never be
    /// re-derived back onto bootstrap weight. Lives in the consensus DB (not meta): the record is
    /// a fact about this consensus's chain, and a staging commit that swaps the consensus out is
    /// exactly the moment it must be re-derived from the new chain.
    VltActivation = 241,

    /// Per-epoch frozen `VltVotingSnapshot` rows (§5), keyed by `u64` wall epoch: the complete
    /// voting denominator — validator rows, weights, and the two roots a vote signs — frozen at
    /// the first recompute of each epoch and write-once thereafter, which is what "the validator
    /// set and its weights are fixed within an epoch" means on disk. Derived state: every row is
    /// re-derivable from the chain at its pinned anchor, so a fresh IBD converges to identical
    /// rows.
    VltVotingSnapshots = 242,
    /// Singleton schema-version marker for [`Self::VltVotingSnapshots`], mirroring
    /// [`Self::VltCreditsSchemaVersion`]: frozen rows are derived AND write-once, so a derivation
    /// change must discard rows recorded under the old rules rather than read them as final.
    VltVotingSnapshotsSchemaVersion = 243,

    /// Per-epoch `DnsFinalityCertificate` rows (§7.2), keyed by `u64` target epoch: the
    /// persistent proof that the epoch's anchor reached its precommit quorum — denominator
    /// roots, weight arithmetic, and every ML-DSA-87 signature. Written once when the quorum
    /// first counts on the selected chain; never reindexed away, because unlike the frozen
    /// snapshots these stop being re-derivable the moment the vote window slides past the
    /// epoch — the certificate IS the surviving evidence, and the §12 checkpoint package's
    /// payload.
    DnsFinalityCertificates = 244,

    // ---- MISAKA Compute Token Program (design v0.1, Phase A) ----
    /// Keyed by `(asset_id, owner)` (8 LE bytes + 64-byte overlay id): one
    /// [`TokenAccount`] `{balance, nonce}` ledger row per holder (design §4.2). Inert (never
    /// written) while every preset's token fence is `u64::MAX`; the acceptance-time application
    /// and its reorg-rollback strategy land with the processor wiring (design §9.5).
    TokenLedger = 245,
    /// Keyed by `u64` asset id: the asset's [`TokenSupply`] `{minted, burned}` counters, the
    /// anchors of the §4.2 conservation invariant `Σ balance == minted − burned`.
    TokenSupply = 246,
    /// Keyed by `u64` epoch: the epoch's settled [`TokenEmissionSettlement`] (budget, X(E), paid
    /// rewards). Write-once per epoch and derived from the **finalized** credit rows
    /// ([`Self::VltCredits`]), so a row is branch-invariant for the same reason those are —
    /// settlement never reads an epoch a challenge or reorg could still change (design §5.3).
    TokenEmissionSettlements = 247,
    /// Singleton `u32`: the rules the three token stores' rows were written under, mirroring
    /// [`Self::VltCreditsSchemaVersion`] — a derivation/layout change discards and rebuilds
    /// rather than reading old rows as final.
    TokenLedgerSchemaVersion = 248,
    /// Singleton `u64`: the next selected-chain **index** the token ledger fold will process
    /// (design §9.2). The fold applies accepted 0x30/0x31 ops only from chain blocks buried past
    /// the reorg horizon, in chain order, so the ledger is an append-only fold with no undo — the
    /// cursor is where the fold resumes.
    TokenLedgerFoldCursor = 249,
    /// Singleton `u64`: the next epoch emission settlement will consider (design §5.3). Advances
    /// in epoch order; a settled (or deliberately skipped) epoch is never revisited.
    TokenSettlementCursor = 250,

    // ---- MISAKA PALW chain carriage (ADR-0029, Stage 1) ----
    /// Keyed by the carrying transaction: an accepted `PalwCarriageRecord` (kind byte, acceptance
    /// DAA, Borsh body bytes verbatim) for every transaction on the PALW carriage band
    /// (0x40-0x45). Written/deleted by the virtual processor's accept/revert walk exactly like
    /// [`Self::ComputeCapabilities`]; an **index** — no consensus rule reads it yet (Stage 2, the
    /// credit gate and duty/offense grounding, is the reader).
    PalwCarriages = 251,
    /// Singleton flag: history has been swept into [`Self::PalwCarriages`], mirroring
    /// [`Self::ComputeCapabilitiesBackfilled`] — a database whose chain predates the store has
    /// carriers accepted and no rows for them, and an empty index reads exactly like "nothing was
    /// carried".
    PalwCarriagesBackfilled = 252,
    /// Singleton `u32`: the record layout the [`Self::PalwCarriages`] rows were written under,
    /// mirroring [`Self::ComputeCapabilitiesSchema`] — an undecodable row is dropped silently by
    /// the iterator, so without this a layout change reads as an empty store, which is a wrong
    /// answer that looks like a valid one.
    PalwCarriagesSchema = 253,
    /// ADR-0038 Decision D: per-`ExecutionClass` difficulty state — the DAA target the class's
    /// lottery runs against, which is one of the two factors `palw_pwu` needs and the only one
    /// that is not frozen at registration.
    PalwClassState = 215,

    /// Singleton `u32`: the layout [`Self::PalwClassState`] rows were written under. Same reason
    /// as [`Self::PalwCarriagesSchema`] — a class whose target reads as absent is a class whose
    /// blocks weigh nothing, which is a wrong answer that looks like a valid one.
    PalwClassStateSchema = 214,

    // ---- MISAKA PALW V2 candidate-scoped state (ADR-0042 Decision 5 / ADR-0044 Unit C) ----
    /// Per-chain-block `PalwStateDeltaV2` (Borsh bytes, verbatim) + the resulting `state_root` —
    /// the reorg primitive on disk. Written in the same batch as the block's chain-commit data,
    /// reverted (and deleted) newest-first when the block leaves the selected chain. A
    /// candidate's V2 standing is a fold of these deltas along ITS chain, never a read of the
    /// node's sink.
    PalwStateV2Deltas = 216,
    /// Singleton: the materialized V2 state at the selected sink — `PalwStateCarriageV2` Borsh
    /// bytes plus the chain block they stand at. Loading verifies the committed `state_root`, so
    /// a corrupted or hand-edited snapshot refuses to become a sink instead of becoming one
    /// quietly.
    PalwStateV2Tip = 223,
    /// Singleton `u32`: the layout the two stores above were written under. Same reason as
    /// [`Self::PalwCarriagesSchema`] — undecodable rows read as absent, and an absent V2 state
    /// looks exactly like "no PALW work matured", which fork choice would act on.
    PalwStateV2Schema = 224,

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
