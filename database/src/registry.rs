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

    // ---- PALW model-agnostic Compute Set registry (ADR-MA §21.1) ----
    // Content-addressed, write-once record stores + the block-keyed fork-local view.
    PalwComputeSetDescriptor = 62,
    PalwComputeSetPolicy = 63,
    PalwAllocationPlan = 64,
    PalwComputeSetCertificate = 65,
    PalwComputeRegistryView = 66,

    // ---- PALW per-epoch beacon seed history (ADR-0045 D3-a) ----
    /// Keyed by epoch (`U64Key`): that epoch's derived `R_E`. The seed's VALUE, retained for a
    /// bounded window, as opposed to prefix 242 which holds the seed's MATERIAL and deliberately
    /// discards past epochs. PCPB re-derives `derive_b(R_e, …)` for the epoch a ticket was bound
    /// to, which can be older than the header walk (fails closed below the pruning point) or the
    /// per-block state rows (deleted by the pruning pass) can answer for.
    PalwBeaconSeedHistory = 67,

    // ---- PCPB per-epoch provider snapshot + A-commit anchors (ADR-0045 D3-b) ----
    /// Keyed by epoch (`U64Key`): that epoch's bond-weighted provider snapshot commitment
    /// (`PalwSnapshotCommitment` — snapshot root, assignment root, total bond, provider count).
    /// The clause-0 "independently resolved" side of `palw_dispatch_evidence_valid`, derived from
    /// the provider-bond registry as the epoch closed along the selected chain, retained for the
    /// beacon-history window + the snapshot lag `k`. Fail-closed outside the window.
    PalwProviderSnapshotHistory = 68,
    /// Keyed by `a_commit` (`Hash64`): the epoch the anchor was ACCEPTED on the selected chain.
    /// Clause 12's self arm requires row == the leaf's declared `a_commit_epoch`, which is what
    /// makes the post-commit draw beacon `R_{a_commit_epoch + Δ}` provably post-date the anchor.
    /// Selected-chain reconciled (written/unwound with the provider-bond registry), swept by the
    /// same window as the snapshot history.
    PalwACommitRegistry = 69,
    /// Keyed by epoch (`U64Key`): the canonical `Vec<PalwProviderSnapshotEntry>` the epoch's
    /// snapshot commitment was built from.
    ///
    /// **Producer aid, not a consensus input.** A verifier needs only the committed roots (prefix
    /// 68) because every witness carries its own membership proofs; a PRODUCER needs the entry set
    /// to BUILD those proofs, and it cannot reconstruct "the registry as of epoch e" from the
    /// current registry. So the node serves what it derived. Deliberately NOT carried in the
    /// pruning snapshot: a pruned joiner must still VERIFY pruned-epoch leaves (it has prefix 68
    /// for that) but is under no obligation to help PRODUCE for epochs it never saw.
    PalwProviderSnapshotEntries = 70,

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

    /// Search-availability fork-local `PalwSearchAvailabilityStateV1`, keyed by `BlockHash` —
    /// the 0x3d-0x3f sibling of `PalwDaStateByBlock` (250). A child clones its selected parent's
    /// state and applies accepted search challenge/response/timeout transitions; side branches
    /// never share a mutable tip row. Placed in the 189-191 gap because the 236-254 PALW block is
    /// full; the prefix value is DB layout only, never part of any commitment.
    PalwSearchAvailabilityStateByBlock = 189,
    /// Keyed by `BlockHash`: 64-byte anchor link for chain blocks whose search-availability state
    /// is bit-identical to an ancestor's stored full row (sibling of `PalwDaStateLinkByBlock` 254).
    PalwSearchAvailabilityStateLinkByBlock = 190,
    /// Singleton `PalwSearchPruningSnapshotV1` captured at the pruning point (sibling of
    /// `PalwDaPruningSnapshot` 252), so open search obligations/challenge deadlines survive
    /// pruned IBD.
    PalwSearchAvailabilityPruningSnapshot = 191,

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
    /// kaspa-pq EVM Lane (§12.3 v2) — keyed by `BlockHash`: a SPARSE
    /// [`EvmStateCheckpointV2`] anchor. Compressed, bytecode-free (only `code_hash`;
    /// the code is in 222) and written only at canonical anchors, replacing 221's
    /// uncompressed full-state copy every 2048 blocks. 221 stays readable for
    /// databases written before the change; the segment pruner reclaims it.
    /// RPC/archive data only.
    EvmStateCheckpointV2 = 223,
    /// kaspa-pq EVM Lane (§12.3 v2) — singleton `EvmCheckpointMeta`: when the last
    /// anchor was written and which anchors are retained, so the cadence is a
    /// decision with state rather than `evm_number % N`.
    EvmCheckpointMeta = 224,
    /// kaspa-pq EVM Lane (segment pruner) — `EvmPruneSegment → EvmPruneCursor`: how far
    /// each EVM data segment has been reclaimed and from where it is still available.
    /// Lets EVM retention run while L1 pruning is deferred (transitional IBD), which is
    /// the window in which the 144 GB incident accumulated.
    EvmPruneCursor = 225,
    /// kaspa-pq EVM Lane (segment pruner) — `BlockHash → EvmBlockIndexJournal`: the exact
    /// secondary-index rows a block created (log postings, tx hashes). A reverse journal
    /// is what makes deleting them exact instead of a range guess.
    EvmBlockIndexJournal = 226,
    /// kaspa-pq EVM Lane (segment pruner) — `tx_hash → EvmRawTxOwners`: which retained
    /// payload blocks still own a raw transaction. A raw tx is reclaimed only when the
    /// last owner is pruned; 204's bounded location vectors evict and so cannot serve as
    /// the ownership ledger.
    EvmRawTxOwners = 227,
    /// kaspa-pq EVM Lane (222 GC) — `code_hash → EvmCodeQuarantine`: bytecode that a
    /// mark-and-sweep pass found unreachable, held for a grace period before deletion so
    /// a transient mark miss cannot destroy shared code.
    EvmCodeQuarantine = 228,
    /// kaspa-pq EVM Lane (cold history) — singleton `EvmColdSegmentManifest`: the
    /// immutable, compressed history segments exported out of RocksDB and the ranges
    /// they cover.
    EvmColdSegmentManifest = 229,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 2) — `EvmAddress → FlatAccountCore`:
    /// nonce/balance/code_hash only. Splitting storage out (233) stops a one-slot write
    /// from rewriting an account's entire storage vector.
    EvmFlatAccountCore = 230,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 1) — singleton `EvmLatestStatePtr`:
    /// the block whose `state_root` the flat state currently materializes. State data.
    EvmLatestStatePtr = 231,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 1) — `BlockHash → state_root[32]`:
    /// O(1) lookup of any committed block's EVM state root. State/RPC data.
    EvmBlockStateRoot = 232,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 2) — `EvmAddress|slot → EvmU256`:
    /// one row per non-zero storage slot. The companion of 230; together they replace
    /// 234's single row per account holding the whole storage vector.
    EvmFlatStorageSlot = 233,
    /// kaspa-pq EVM Lane (C-01 state backend, Stage 1) — `EvmAddress → FlatAccount`: the
    /// flat LATEST-canonical state (one row per account, NOT per block), replacing the
    /// per-block O(state × blocks) snapshot. Code is content-addressed (222). State data.
    EvmFlatAccount = 234,
    /// kaspa-pq EVM Lane — content-addressed EvmPayload (`BlockHash → SlimEvmPayload`):
    /// the payload envelope + ordered tx-hash references, with the raw tx bytes held
    /// ONCE in 217. Supersedes the inline `transactions` of prefix 211 (which keeps
    /// LEGACY full rows that drain with pruning); the full payload is reconstructed on
    /// read from 217, byte-identically, so the `evm_payload_hash` commitment is
    /// unchanged. Storage-only dedup — a tx repeated across payloads costs one copy.
    EvmPayloadSlim = 235,

    // ---- kaspa-pq ADR-0039 PALW (audited-compute lane) ----
    /// Keyed by `BlockHash`: the `PalwActiveNullifierSet` — the retention-windowed set of ticket
    /// nullifiers active in that block's past (§15.2). Read by the GHOSTDAG duplicate-ticket dedup to
    /// seed a child from its selected parent's past; deleted on prune. Written only while PALW is
    /// active.
    PalwNullifiers = 236,
    /// ADR-0039 §18.1 PALW overlay stores — the on-chain audited-compute state a validator resolves an
    /// algo-4 ticket against (leaf descriptor / batch manifest / certificate / batch status /
    /// provider bond). Keyed as noted below and populated only while PALW is active.
    /// Keyed by `(batch_id, leaf_index)`: the `PalwPublicLeafV1` descriptor (§9.2).
    PalwLeaf = 237,
    /// Keyed by `batch_id`: the `PalwBatchManifestV1` (leaf/chunk counts, §9.3).
    PalwBatchManifest = 238,
    /// Keyed by `cert_hash`: the `PalwBatchCertificateV2` (§10.1).
    PalwCertificate = 239,
    /// Keyed by `batch_id`: the `PalwBatchStatus` state-machine value (§9.5).
    PalwBatchStatus = 240,
    /// Keyed by `TransactionOutpoint`: the `PalwProviderBondPayloadV1` (§9.6).
    PalwProviderBond = 241,
    /// Keyed by epoch (`U64Key`): the `PalwBeaconEpochAccumV1` — that epoch's committed + validly-revealed
    /// `(bond, commitment)` sets (§11.2). Range-free (whole-set value).
    PalwBeaconAccum = 242,
    /// Keyed by `BlockHash`: the block's carried `PalwBeaconStateV1` (the epoch's active `R_E` recurrence,
    /// §11.2/§18.2). Block-keyed and read relative to the selected parent.
    PalwBeaconState = 243,
    /// Keyed by `BlockHash`: the selected-parent-carried PALW beacon commit/reveal accumulator view.
    /// Unlike the legacy epoch-global accumulator at prefix 242, this value is fork-local: a child
    /// clones its selected parent's view, applies only its accepted commit/reveal deltas, and persists
    /// the result atomically with the block. This prevents side branches and processing order from
    /// contaminating `R_E` at an epoch boundary. Prefix 242 remains readable for the inert pre-activation
    /// scaffolding; activated PALW networks use this block-keyed store exclusively.
    PalwBeaconAccumByBlock = 244,
    /// Keyed by `BlockHash`: the block's carried `PalwLaneBitsV1` (hash_bits, replica_bits) — the §16.3
    /// per-lane difficulty HOLD sources. Block-keyed/past-relative: a lane cannot read the selected
    /// parent's `header.bits` (that is the OTHER lane's difficulty at a mixed-lane boundary), so both
    /// lanes' current bits are carried forward per block.
    PalwLaneBits = 245,
    /// Keyed by `BlockHash`: the block's carried `PalwBatchViewV1` — the fork-local batch-lifecycle
    /// overlay (§18.2, C5 option B). `view(B) = view(SP(B)) ⊕ Δ(mergeset(B))`, PalwNullifierStore-shaped
    /// (block-keyed, seeded from the selected parent, past-relative). Replaces the tip-global batch
    /// reads of `DbPalwStore` for the algo-4 ticket check.
    PalwOverlayView = 246,

    /// Singleton `(BlockHash, PalwPrunedFrontierV1)`: the PALW pruned-IBD frontier taken as-of the
    /// current pruning point (§18.2 / D3) — the pp's beacon state / overlay view / lane bits /
    /// active-nullifier window a pruned joiner seeds to validate the first post-pp v3 block. Its OWN
    /// singleton (not the `PruningPointOverlaySnapshot` wrapper) so appending it never disturbs that
    /// wrapper's bincode read on an in-place upgrade.
    PalwPrunedFrontier = 247,

    /// Keyed by `BlockHash`: the `job_nullifier`s THIS chain block's coinbase actually paid a
    /// `ReplicaPalw` provider pair for (ADR-0040 §5.15.13 / gate G16 — P1-9-RELAND).
    ///
    /// The per-chain-block delta of the reward-coordinate duplicate-work rule, shaped exactly like
    /// `RewardedEpochs`: written at `commit_utxo_state`, read back by a bounded selected-chain walk in
    /// descendants, deleted by the pruning processor. It is emphatically NOT the `job_nullifiers` map
    /// that ADR-0040 P1-5 deleted from `PalwBatchViewV1` — that one was cloned into every descendant
    /// block, grew by attacker-declared leaf-chunk content, and was never read. This one is not cloned,
    /// is bounded by the mergeset size limit, and every entry costs an accepted algo-4 block.
    ///
    /// Written on PALW networks when an accepted algo-4 source receives a `ReplicaPalw`
    /// classification.
    PalwPaidWork = 248,
    /// Keyed by `BlockHash`: Header-v4 fork-local objective anti-spam accumulator. The fixed-size row
    /// contains two cumulative blue-lane counters plus one deterministic selected-parent skip link.
    PalwSpamAccumulator = 249,
    /// DA-01 fork-local `PalwDaStateV1`, keyed by `BlockHash`. A child clones its selected parent's
    /// state and applies accepted 0x3a-0x3c transitions; side branches never share a mutable tip row.
    PalwDaStateByBlock = 250,
    /// DA-01 content-addressed canonical `PalwReceiptDaObjectV1` bytes, keyed by receipt DA root.
    /// Auxiliary availability cache: object presence is never accepted without root/semantic checks.
    PalwDaObject = 251,
    /// DA-01 singleton `PalwDaPruningSnapshotV1` captured at the pruning point before old block-state
    /// rows are reclaimed, so open obligations/challenges and dedup survive pruned IBD.
    PalwDaPruningSnapshot = 252,
    /// Node-anchored web-search snapshots (`PalwSearchSnapshotStoredV1`), keyed by their version-3
    /// DA object root. Separate from `PalwDaObject` so receipt-obligation GC and snapshot-retention
    /// GC can never delete each other's rows.
    PalwSearchSnapshotObject = 253,
    /// Keyed by `BlockHash`: 64-byte anchor link for chain blocks whose PALW DA state is
    /// bit-identical to an ancestor's stored full row. Written instead of duplicating the full
    /// `PalwDaStateV1` row per block; readers resolve `full → link → full(anchor)` in two reads.
    PalwDaStateLinkByBlock = 254,

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
