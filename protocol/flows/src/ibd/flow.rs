use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    ibd::{HeadersChunkStream, TrustedEntryStream, negotiate::ChainNegotiationOutput},
    v8::request_palw_chain_derived_bundle::{MAX_PALW_CHAIN_DERIVED_BUNDLE_CHUNKS, PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BATCH},
};
use futures::future::{Either, join_all, select, try_join_all};
use itertools::Itertools;
use kaspa_consensus_core::{BlockHash, Hash64}; // PR-9.5e: block hashes are Hash64
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
    api::BlockValidationFuture,
    block::Block,
    constants::{PALW_ANTISPAM_HEADER_VERSION, PALW_HEADER_VERSION},
    header::Header,
    palw_pruned_frontier::{
        MAX_PALW_CHAIN_DERIVED_BUNDLE_BYTES, MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS, PalwChainDerivedAuthBundleV1,
        PalwChainDerivedHeaderBundleWireV1, PalwPruningPointSnapshotV1, PalwPruningSnapshotCheckpoint, PalwPruningSnapshotImportAuth,
        PalwPruningSnapshotImportProvenance, palw_bind_transported_header_identity, palw_chain_derived_descendant_shape_is_valid,
        palw_pruned_ibd_snapshot_import_allowed, verify_chain_derived_pruning_boundary_from_payload,
    },
    pruning::{PruningPointProof, PruningPointsList, PruningProofMetadata},
    trusted::TrustedBlock,
    tx::Transaction,
};
use kaspa_consensusmanager::{ConsensusProxy, StagingConsensus, spawn_blocking};
use kaspa_core::{debug, info, time::unix_now, warn};
use kaspa_muhash::MuHash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    convert::{
        header::{HeaderFormat, Versioned},
        model::trusted::TrustedDataPackage,
    },
    dequeue_with_timeout, make_message, make_request,
    pb::{
        RequestAntipastMessage, RequestBlockBodiesMessage, RequestHeadersMessage, RequestIbdBlocksMessage,
        RequestNextPalwChainDerivedBundleChunksMessage, RequestPalwChainDerivedBundleMessage,
        RequestPruningPointAndItsAnticoneMessage, RequestPruningPointEvmStateMessage, RequestPruningPointOverlaySnapshotMessage,
        RequestPruningPointPalwSnapshotMessage, RequestPruningPointProofMessage, RequestPruningPointUtxoSetMessage,
        kaspad_message::Payload,
    },
};
use kaspa_utils::channel::JobReceiver;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;

use super::{HeadersChunk, IBD_BATCH_SIZE, PruningPointUtxosetChunkStream, progress::ProgressReporter};
type BlockBody = Vec<Transaction>;

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    pub(super) ctx: FlowContext,
    pub(super) router: Arc<Router>,
    pub(super) incoming_route: IncomingRoute,
    pub(super) body_only_ibd_permitted: bool,
    header_format: HeaderFormat,

    // Receives relay blocks from relay flow which are out of orphan resolution range and hence trigger IBD
    relay_receiver: JobReceiver<Block>,
    expected_palw_snapshot_digest: Option<Hash64>,
    /// ADR-0042: the chain-derived bundle digest advertised in the trusted-data package, if any. No
    /// shipped server emits one, so this is `None` on every real connection today.
    expected_palw_chain_derived_bundle_digest: Option<Hash64>,
    /// ADR-0042 review point (a), headers-proof path: the exact set of headers that passed
    /// `validate_pruning_proof` — per-DAA algo id, block level, proof of work, strictly increasing
    /// blue work versus the proof parents, level-chain structure — and whose accumulated blue work
    /// beat the local defender. Captured while the flow still owns the proof, before it is moved into
    /// `spawn_blocking`, because that set is the only work-authenticated header set available at
    /// `sync_pruning_point_palw_snapshot` time.
    proof_validated_headers: Option<Arc<BlockHashMap<Arc<Header>>>>,
}

#[async_trait::async_trait]
impl Flow for IbdFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

pub enum IbdType {
    Sync { highest_known_syncer_chain_hash: BlockHash, is_utxo_stable: bool, is_pp_anticone_synced: bool },
    DownloadHeadersProof,
    PruningCatchUp { highest_known_syncer_chain_hash: BlockHash },
}

struct QueueChunkOutput {
    jobs: Vec<BlockValidationFuture>,
    daa_score: u64,
    timestamp: u64,
}

struct DownloadedPalwPruningSnapshot {
    daa_score: u64,
    header_version: u16,
    spam_commitment: Hash64,
    import_auth: PalwPruningSnapshotImportAuth,
    snapshot: PalwPruningPointSnapshotV1,
    /// ADR-0042: present exactly when `import_auth.provenance == ChainDerivedHeaderBundle`, i.e. only
    /// on a Header-v4 network with the node-local lever set and no operator pin for this boundary. The
    /// roots inside are already authenticated (review points (a)/(b) below) and the boundary fold has
    /// already been verified against them.
    chain_derived: Option<PalwChainDerivedAuthBundleV1>,
}

/// Which already-authenticated header set a transported chain-derived bundle is allowed to restate.
///
/// ADR-0042 review point (a) is "the transported headers are a subset of the proof-validated set".
/// The two IBD entry points stand in genuinely different positions with respect to that set, and the
/// difference is not cosmetic:
///
/// * On **catch-up** `sync_headers` has already run, so every post-pruning-point header is in the
///   local store having passed the unmodified header pipeline (PoW, difficulty, GHOSTDAG,
///   `check_palw_spam`) and is buried under the chain being adopted. That set is *strictly stronger*
///   than the pruning proof, and it is the set used here.
/// * On the **headers-proof** path the pruning proof is the only work-authenticated header set that
///   exists yet. It provably does not contain what a chain-derived bundle needs: the proof collects
///   `future(root) ∩ past(pruning point)` per level, so no post-pruning-point header exists in it at
///   all, and level 0 is bounded by `2 * pruning_proof_m` = 2000 headers while the anti-spam support
///   closure is `span + 1` = 32,769 rows at the only shipped non-inert preset. The subset test is
///   therefore expected to fail on this path, and failing is the correct, fail-closed outcome: the
///   requester falls back to the operator-pin path. It is enforced literally rather than skipped so
///   that the property is checked by code, not by an argument in a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChainDerivedHeaderAuthority {
    /// Catch-up: the local, full-pipeline-validated headers store, plus a locally selected descendant.
    LocalValidatedChain { syncer_sink: BlockHash },
    /// Headers-proof: the accumulated-work-validated pruning proof captured in `proof_validated_headers`.
    ProofValidatedHeaders,
}

/// The resolved answer to "what may this bundle restate, and which block may its descendant be".
struct AuthenticatedChainDerivedHeaders {
    /// Review point (a): the set every transported header must appear in, byte for byte.
    headers: BlockHashMap<Arc<Header>>,
    /// Review point (b): the single block the transported descendant is allowed to be. `None` means
    /// this IBD path has no work-authenticated descendant at all, which is an unconditional refusal —
    /// membership in `headers` alone must never be enough, because `overlay_commitment_root` is a body
    /// rule and a locally stored header-only block that merely lists the pruning point among its
    /// parents can therefore commit an arbitrary value.
    descendant: Option<BlockHash>,
}

/// Resolve authentication before requesting peer-controlled sidecar bytes. Header-v3 retains its
/// historical trusted-data rule. Header-v4 prefers an exact local operator pin; a peer digest is
/// additional consistency evidence when present, never a substitute for that pin.
///
/// ADR-0042: when — and only when — there is no local pin for this boundary AND the node-local
/// `--palw-permissionless-snapshot-auth` lever is set, the Header-v4 arm falls through to the
/// chain-derived provenance instead of refusing. With the lever off (the default, which no preset and
/// no CLI default sets) this function is byte-identical to its previous behaviour, including the exact
/// refusal text, so lever-off nodes cannot tell this change happened.
#[allow(clippy::too_many_arguments)]
fn preflight_palw_snapshot_import_auth(
    header_version: u16,
    pruning_point: BlockHash,
    trusted_digest: Option<Hash64>,
    require_trusted_digest: bool,
    operator_checkpoints: &[PalwPruningSnapshotCheckpoint],
    palw_spam_is_inert: bool,
    permissionless_enabled: bool,
    chain_derived_bundle_digest: Option<Hash64>,
) -> Result<Option<PalwPruningSnapshotImportAuth>, &'static str> {
    let configured_header_version = if palw_spam_is_inert { PALW_HEADER_VERSION } else { PALW_ANTISPAM_HEADER_VERSION };
    if header_version != configured_header_version {
        return Err("PALW pruning-point header version does not match this network's configured PALW schema");
    }
    match header_version {
        PALW_HEADER_VERSION => {
            if require_trusted_digest && trusted_digest.is_none() {
                return Err("active Header-v3 PALW headers-proof IBD is missing the pruning snapshot digest in trusted data");
            }
            Ok(trusted_digest.map(|digest| PalwPruningSnapshotImportAuth::legacy_header_v3(pruning_point, digest)))
        }
        PALW_ANTISPAM_HEADER_VERSION => {
            // The operator pin is tried first and is unconditionally preferred. A node that pinned the
            // boundary keeps the pinned semantics whether or not the lever is set, and a peer cannot
            // downgrade a pinned node to chain-derived authentication by withholding its digest.
            match operator_checkpoints.iter().find(|checkpoint| checkpoint.pruning_point == pruning_point).copied() {
                Some(checkpoint) => {
                    if trusted_digest.is_some_and(|digest| digest != checkpoint.payload_digest) {
                        return Err("Header-v4 PALW trusted-data digest conflicts with the local operator checkpoint");
                    }
                    Ok(Some(PalwPruningSnapshotImportAuth::operator_pinned(checkpoint)))
                }
                // Unchanged refusal when the lever is off. This is the entire lever-off behaviour of
                // the Header-v4 arm and it is deliberately expressed with the original message.
                None if !permissionless_enabled => {
                    Err("Header-v4 PALW pruned IBD requires a matching local --palw-pruning-snapshot-checkpoint")
                }
                None => {
                    // The bundle rides in a separate chunked transport, so on any path that carries a
                    // trusted-data package the package's digest is what binds those bytes to the
                    // accumulated-work-validated package. Without it the bundle would be an unbound
                    // message from an arbitrary peer, so refuse rather than accept an unbound object.
                    if require_trusted_digest && chain_derived_bundle_digest.is_none() {
                        return Err(
                            "Header-v4 chain-derived PALW import requires the bundle digest in trusted data; no local operator checkpoint either",
                        );
                    }
                    Ok(Some(PalwPruningSnapshotImportAuth::chain_derived(pruning_point)))
                }
            }
        }
        _ => Err("PALW pruned IBD does not support this pruning-point header version"),
    }
}

/// ADR-0042 review point (a), the whole of it, as one pure function.
///
/// Every header a peer transported in the bundle must RESTATE a header from `authenticated` — never
/// introduce one. Three things are checked per header, in this order, and all three are load-bearing:
///
/// 1. **Identity.** `hashing::header::hash(h) == h.hash`. `Header` derives `BorshDeserialize` with
///    `hash` as its first field and Borsh never re-derives it, so a wire header's block hash is a
///    value the *peer* chose. Without recomputing, a peer could point `hash` at a genuinely
///    authenticated block while attaching an arbitrary body, and step 2 would happily find it.
/// 2. **Membership.** `authenticated` must contain that hash.
/// 3. **Byte equality.** The canonical Borsh encoding of the transported header must equal that of the
///    authenticated one. For Header-v4 this is implied by step 1 (every non-cached field is in the
///    hash preimage), but "implied by" is exactly the kind of reasoning that rots the moment a field
///    is added, so it is asserted independently and cheaply.
///
/// This runs BEFORE `PalwChainDerivedHeaderBundleWireV1::extract_authenticated_bundle`, which by its
/// own documentation authenticates nothing beyond header self-consistency. If this function is ever
/// removed or moved after the projection, the entire scheme collapses to "trust the peer".
fn bind_chain_derived_headers_to_authenticated_set(
    bundle: &PalwChainDerivedHeaderBundleWireV1,
    authenticated: &BlockHashMap<Arc<Header>>,
) -> Result<(), &'static str> {
    for header in std::iter::once(&bundle.descendant_header).chain(bundle.support_headers.iter()) {
        palw_bind_transported_header_identity(header)
            .map_err(|_| "chain-derived bundle carries a header whose hash does not match its own preimage")?;
        let Some(known) = authenticated.get(&header.hash) else {
            return Err("chain-derived bundle carries a header that is not in the authenticated header set");
        };
        let (Ok(known_bytes), Ok(transported_bytes)) = (borsh::to_vec(known.as_ref()), borsh::to_vec(header)) else {
            return Err("chain-derived bundle header could not be canonically re-encoded for comparison");
        };
        if known_bytes != transported_bytes {
            return Err("chain-derived bundle header differs from the authenticated header with the same hash");
        }
    }
    Ok(())
}

/// ADR-0042 review points (a) and (b) together, as one pure, unit-testable gate. Everything a
/// transported bundle must satisfy before it is projected lives here.
///
/// (a) is [`bind_chain_derived_headers_to_authenticated_set`]. (b) is the descendant pin below, and it
/// is not redundant with (a): the authenticated set necessarily also contains the support headers and,
/// on the catch-up path, is populated by hashes the *peer* chose. Membership alone would therefore let
/// a peer nominate any locally stored block that lists the pruning point among its parents as "the
/// descendant". `overlay_commitment_root` is validated by a BODY rule
/// (`verify_expected_utxo_state`), so a header-only block's value is entirely unvalidated and such a
/// nomination would forge an arbitrary boundary for the cost of one block. Requiring the descendant to
/// be exactly the block this node itself selected and buried is what closes that.
fn bind_chain_derived_bundle_to_authenticated_headers(
    bundle: &PalwChainDerivedHeaderBundleWireV1,
    authenticated: &AuthenticatedChainDerivedHeaders,
) -> Result<(), &'static str> {
    bind_chain_derived_headers_to_authenticated_set(bundle, &authenticated.headers)?;
    let Some(required_descendant) = authenticated.descendant else {
        return Err("chain-derived PALW import has no work-authenticated descendant available on this IBD path");
    };
    if bundle.descendant_header.hash != required_descendant {
        return Err("chain-derived descendant is not the block this node selected and work-authenticated");
    }
    Ok(())
}

// TODO: define a peer banning strategy

impl IbdFlow {
    pub fn new(
        ctx: FlowContext,
        router: Arc<Router>,
        incoming_route: IncomingRoute,
        relay_receiver: JobReceiver<Block>,
        body_only_ibd_permitted: bool,
        header_format: HeaderFormat,
    ) -> Self {
        Self {
            ctx,
            router,
            incoming_route,
            relay_receiver,
            body_only_ibd_permitted,
            header_format,
            expected_palw_snapshot_digest: None,
            expected_palw_chain_derived_bundle_digest: None,
            proof_validated_headers: None,
        }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        while let Ok(relay_block) = self.relay_receiver.recv().await {
            // Skip triggering IBD from a peer whose recent IBD attempt failed, while
            // it is still in its backoff window. This does NOT take the IBD lock, so
            // a healthy peer's relay can win it instead — the fix for the
            // retry-same-peer wedge (a peer that cannot serve the pruning-point EVM
            // state would otherwise be re-selected immediately and stall sync). With
            // a single peer this only delays the retry until the backoff expires.
            if self.ctx.ibd_peer_in_backoff(self.router.key(), std::time::Instant::now()) {
                continue;
            }
            if let Some(_guard) = self.ctx.try_set_ibd_running(self.router.key(), relay_block.header.daa_score) {
                info!("IBD started with peer {}", self.router);

                match self.ibd(relay_block).await {
                    Ok(_) => info!("IBD with peer {} completed successfully", self.router),
                    Err(e) => {
                        info!("IBD with peer {} completed with error: {}", self.router, e);
                        // Back this peer off so the next round prefers another peer.
                        self.ctx.record_ibd_failure(self.router.key(), std::time::Instant::now());
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn ibd(&mut self, relay_block: Block) -> Result<(), ProtocolError> {
        self.expected_palw_snapshot_digest = None;
        // Per-attempt state: neither a digest nor an authenticated header set may survive into a
        // second IBD attempt, possibly against a different peer.
        self.expected_palw_chain_derived_bundle_digest = None;
        self.proof_validated_headers = None;
        let mut session = self.ctx.consensus().session().await;

        let negotiation_output = self.negotiate_missing_syncer_chain_segment(&session).await?;
        let ibd_type = self
            .determine_ibd_type(
                &session,
                &relay_block.header,
                negotiation_output.highest_known_syncer_chain_hash,
                negotiation_output.syncer_pruning_point,
            )
            .await?;
        match ibd_type {
            IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced } => {
                let pruning_point = session.async_pruning_point().await;

                info!("syncing ahead from current pruning point");
                // Following IBD catchup a new pruning point is designated and finalized in consensus. Blocks from its anticone (including itself)
                // have undergone normal header verification, but contain no body yet. Processing of new blocks in the pruning point's future cannot proceed
                // since these blocks' parents are missing block data.
                // Hence we explicitly process bodies of the currently body missing anticone blocks as trusted blocks
                // Notice that this is degenerate following sync_with_headers_proof
                // but not necessarily so after sync_headers -
                // as it might sync following a previous pruning_catch_up that crashed before this stage concluded
                if !is_pp_anticone_synced {
                    self.sync_missing_trusted_bodies(&session).await?;
                }
                if !is_utxo_stable
                // Utxo might not be available even if the pruning point block data is.
                // Utxo must be synced before all so the node could function
                {
                    info!(
                        "utxoset corresponding to the current pruning point is incomplete, attempting to download it from {}",
                        self.router
                    );

                    // Imports the pruning point's utxoset AND (ADR-0022) its EVM + overlay sidecars
                    // atomically before marking the utxoset stable — see sync_new_utxo_set.
                    self.sync_new_utxo_set(&session, pruning_point, true).await?;
                }
                // Once utxo is valid, simply sync missing headers
                self.sync_headers(
                    &session,
                    negotiation_output.syncer_virtual_selected_parent,
                    highest_known_syncer_chain_hash,
                    &relay_block,
                )
                .await?;
            }
            IbdType::DownloadHeadersProof => {
                drop(session); // Avoid holding the previous consensus throughout the staging IBD
                let staging = self.ctx.consensus_manager.new_staging_consensus();
                match self.ibd_with_headers_proof(&staging, negotiation_output.syncer_virtual_selected_parent, &relay_block).await {
                    Ok(()) => {
                        spawn_blocking(|| staging.commit()).await.unwrap();
                        info!(
                            "Header download stage of IBD with headers proof completed successfully from {}. Committed staging consensus.",
                            self.router
                        );

                        // This will reobtain the freshly committed staging consensus
                        session = self.ctx.consensus().session().await;
                        // Next, sync a utxoset corresponding to the new pruning point from the syncer.
                        // Note that the new pruning point's anticone need not be downloaded separately as in other IBD types
                        // as it was just downloaded as part of the headers proof.
                        // Imports the new pruning point's utxoset AND (ADR-0022) its EVM + overlay sidecars
                        // atomically before marking the utxoset stable — see sync_new_utxo_set. Without the
                        // sidecars the first post-pruning block re-executes EVM from an empty genesis state /
                        // recomputes overlay rewards from empty state and is disqualified (with all descendants).
                        self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point, true).await?;
                    }
                    Err(e) => {
                        warn!("IBD with headers proof from {} was unsuccessful ({})", self.router, e);
                        staging.cancel();
                        return Err(e);
                    }
                }
            }
            IbdType::PruningCatchUp { highest_known_syncer_chain_hash } => {
                info!("catching up to new pruning point {} ", negotiation_output.syncer_pruning_point);
                match self.pruning_point_catchup(&session, &negotiation_output, &relay_block, highest_known_syncer_chain_hash).await {
                    Ok(()) => {
                        info!("header stage of pruning catchup from peer {} completed", self.router);
                        self.sync_missing_trusted_bodies(&session).await?;
                        // Imports the new pruning point's utxoset AND (ADR-0022) its EVM + overlay sidecars
                        // atomically before marking the utxoset stable — see sync_new_utxo_set.
                        // `pruning_point_catchup` already installed this exact PALW/provider/DA
                        // boundary in the intrusive pointer batch. Do not ask the peer for a second
                        // independently committed copy before UTXO download. If the process crashes
                        // here, the next unstable-current-PP IBD takes the `true` path above and
                        // deliberately re-fetches the boundary before clearing the old UTXO set.
                        self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point, false).await?;
                        // Note that pruning of old data will only occur once virtual has caught up sufficiently far
                    }

                    Err(e) => {
                        warn!("IBD catchup from peer {} was unsuccessful ({})", self.router, e);
                        return Err(e);
                    }
                }
            }
        }

        // Sync missing bodies in the past of syncer sink (virtual selected parent)
        self.sync_missing_block_bodies(&session, negotiation_output.syncer_virtual_selected_parent).await?;

        // Relay block might be in the antipast of syncer sink, thus
        // check its past for missing bodies as well.
        self.sync_missing_block_bodies(&session, relay_block.hash()).await?;

        // Following IBD we revalidate orphans since many of them might have been processed during the IBD
        // or are now processable
        let (queued_hashes, virtual_processing_tasks) = self.ctx.revalidate_orphans(&session).await;
        let mut unorphaned_hashes = Vec::with_capacity(queued_hashes.len());
        let results = join_all(virtual_processing_tasks).await;
        for (hash, result) in queued_hashes.into_iter().zip(results) {
            match result {
                Ok(_) => unorphaned_hashes.push(hash),
                // We do not return the error and disconnect here since we don't know
                // that this peer was the origin of the orphan block
                Err(e) => warn!("Validation failed for orphan block {}: {}", hash, e),
            }
        }
        match unorphaned_hashes.len() {
            0 => {}
            n => info!("IBD post processing: unorphaned {} blocks ...{}", n, unorphaned_hashes.last().unwrap()),
        }

        Ok(())
    }

    async fn determine_ibd_type(
        &self,
        consensus: &ConsensusProxy,
        relay_header: &Header,
        highest_known_syncer_chain_hash: Option<BlockHash>,
        syncer_pruning_point: BlockHash,
    ) -> Result<IbdType, ProtocolError> {
        if let Some(highest_known_syncer_chain_hash) = highest_known_syncer_chain_hash {
            let pruning_point = consensus.async_pruning_point().await;
            let sink = consensus.async_get_sink().await;
            info!("current sink is:{}", sink);
            info!("current pruning point is:{}", pruning_point);
            if consensus.async_is_chain_ancestor_of(pruning_point, highest_known_syncer_chain_hash).await? {
                /// Categorizes the syncer's pruning point position relative to local
                enum SyncerSkew {
                    Lagging,
                    Aligned,
                    Leading,
                }

                let syncer_skew = if syncer_pruning_point == pruning_point {
                    SyncerSkew::Aligned
                } else if consensus.async_is_chain_ancestor_of(pruning_point, syncer_pruning_point).await.unwrap_or(false) {
                    SyncerSkew::Leading
                } else if consensus.async_get_n_last_pruning_points(4 /*syncer lag tolerance*/).await.contains(&syncer_pruning_point) {
                    SyncerSkew::Lagging
                } else {
                    return Err(ProtocolError::Other(
                        "The syncer purports to have data in the recent future but their pruning point could not be easily recognized",
                    ));
                };

                let is_utxo_stable = consensus.async_is_pruning_utxoset_stable().await;
                let is_pp_anticone_synced = consensus.async_is_pruning_point_anticone_fully_synced().await;

                return match (syncer_skew, is_utxo_stable && is_pp_anticone_synced) {
                    (SyncerSkew::Aligned, _) => {
                        Ok(IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced })
                    }
                    (SyncerSkew::Lagging, true) => {
                        Ok(IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced })
                    }
                    (SyncerSkew::Lagging, false) => Err(ProtocolError::Other(
                        "Local node is in a transitional state requiring external data to stabilize, but the syncer lags behind and is unable to provide said data",
                    )),
                    (SyncerSkew::Leading, true) => {
                        if consensus.async_get_block_status(syncer_pruning_point).await.is_some_and(|b| b.has_block_body()) {
                            // While a leading syncer skew often indicates the need for catchup, in this case
                            // the node is just missing a segment in the future of its current pruning point, that is available to the syncer
                            Ok(IbdType::Sync { highest_known_syncer_chain_hash, is_utxo_stable, is_pp_anticone_synced })
                        } else {
                            Ok(IbdType::PruningCatchUp { highest_known_syncer_chain_hash })
                        }
                    }
                    (SyncerSkew::Leading, false) => Ok(IbdType::PruningCatchUp { highest_known_syncer_chain_hash }),
                };
            }

            // If the pruning point is not in the chain of `highest_known_syncer_chain_hash`, it
            // means it's in its antichain (because if `highest_known_syncer_chain_hash` was in
            // the pruning point's past the pruning point itself would be
            // `highest_known_syncer_chain_hash`). So it means there's a finality conflict.
            //
            // TODO (relaxed): consider performing additional actions on finality conflicts in addition
            // to disconnecting from the peer (e.g., banning, rpc notification)
            return Err(ProtocolError::Other("peer is in a finality conflict with the local pruning point"));
        }

        let hst_header = consensus.async_get_header(consensus.async_get_headers_selected_tip().await).await.unwrap();
        let pruning_depth = self.ctx.config.pruning_depth();
        if relay_header.blue_score >= hst_header.blue_score + pruning_depth && relay_header.blue_work > hst_header.blue_work {
            let finality_duration_in_milliseconds = self.ctx.config.finality_duration_in_milliseconds();
            if unix_now() > consensus.async_creation_timestamp().await + finality_duration_in_milliseconds {
                let fp = consensus.async_finality_point().await;
                let fp_ts = consensus.async_get_header(fp).await?.timestamp;
                if unix_now() < fp_ts + finality_duration_in_milliseconds * 3 / 2 {
                    // We reject the headers proof if the node has a relatively up-to-date finality point and current
                    // consensus has matured for long enough (and not recently synced). This is mostly a spam-protector
                    // since subsequent checks identify these violations as well
                    // TODO (relaxed): consider performing additional actions on finality conflicts in addition to disconnecting from the peer (e.g., banning, rpc notification)
                    return Err(ProtocolError::Other(
                        "peer has no known block but local consensus appears to be up to date, this is most likely a spam attempt",
                    ));
                }
            }

            // The relayed block has sufficient blue score and blue work over the current header selected tip
            Ok(IbdType::DownloadHeadersProof)
        } else {
            Err(ProtocolError::Other("peer has no known block but conditions for requesting headers proof are not met"))
        }
    }

    /// This function is triggered when the syncer's pruning point is higher
    /// than ours and we already processed its header before.
    /// so we only need to sync more headers and set it to our new pruning point before proceeding with IBD
    async fn pruning_point_catchup(
        &mut self,
        consensus: &ConsensusProxy,
        negotiation_output: &ChainNegotiationOutput,
        relay_block: &Block,
        highest_known_syncer_chain_hash: BlockHash,
    ) -> Result<(), ProtocolError> {
        // Before attempting to update to the syncer's pruning point, sync to the latest headers of the syncer,
        // to ensure that we will locally have sufficient headers on top of the syncer's pruning point
        let syncer_pp = negotiation_output.syncer_pruning_point;
        let syncer_sink = negotiation_output.syncer_virtual_selected_parent;
        self.sync_headers(consensus, syncer_sink, highest_known_syncer_chain_hash, relay_block).await?;

        // Catch-up mutates the current consensus rather than a discardable staging DB. Download and
        // fully validate the sidecar first, then hand it to the atomic consensus API which commits the
        // PALW/provider/DA boundary together with the PP pointer, virtual/tips/selected-chain reset and
        // unstable-UTXO flag. A failed path/anticone/snapshot preflight leaves every cache untouched.
        let palw_snapshot = self
            .download_pruning_point_palw_snapshot(
                consensus,
                syncer_pp,
                self.expected_palw_snapshot_digest,
                false,
                // ADR-0042: `sync_headers` above has just validated every post-pruning-point header
                // through the unmodified pipeline into THIS consensus, so the local store — not the
                // pruning proof — is the authenticated set, and the descendant is selected from it.
                ChainDerivedHeaderAuthority::LocalValidatedChain { syncer_sink },
            )
            .await?;
        if let Some(downloaded) = palw_snapshot {
            Self::chain_derived_import_is_wired(&downloaded)?;
            consensus
                .async_intrusive_pruning_point_update_with_palw_snapshot(
                    syncer_pp,
                    syncer_sink,
                    downloaded.daa_score,
                    downloaded.header_version,
                    downloaded.spam_commitment,
                    downloaded.import_auth,
                    downloaded.snapshot,
                )
                .await?;
        } else {
            consensus.async_intrusive_pruning_point_update(syncer_pp, syncer_sink).await?;
        }

        // A sanity check to confirm that following the intrusive addition of new pruning points,
        // the latest pruning point still correctly agrees with the DAG data,
        // and is the head of a pruning points "chain" leading all the way down to genesis
        // TODO (relaxed): once the catchup functionality has sufficiently matured, consider only doing this test if sanity checks are enabled
        info!("validating pruning points consistency");
        consensus.async_validate_pruning_points(syncer_sink).await.unwrap();
        info!("pruning points consistency validated");
        Ok(())
    }

    async fn ibd_with_headers_proof(
        &mut self,
        staging: &StagingConsensus,
        syncer_virtual_selected_parent: BlockHash,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        info!("Starting IBD with headers proof with peer {}", self.router);

        let staging_session = staging.session().await;

        let pruning_point = self.sync_and_validate_pruning_proof(&staging_session, relay_block).await?;
        self.sync_headers(&staging_session, syncer_virtual_selected_parent, pruning_point, relay_block).await?;
        staging_session.async_validate_pruning_points(syncer_virtual_selected_parent).await?;
        self.validate_staging_timestamps(&self.ctx.consensus().session().await, &staging_session).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(
        &mut self,
        staging: &ConsensusProxy,
        relay_block: &Block,
    ) -> Result<BlockHash, ProtocolError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = Versioned(self.header_format, msg).try_into()?;
        info!(
            "Received headers proof with overall {} headers ({} unique)",
            proof.iter().map(|l| l.len()).sum::<usize>(),
            proof.iter().flatten().unique_by(|h| h.hash).count()
        );

        let proof_metadata = PruningProofMetadata::new(relay_block.header.blue_work);

        // Get a new session for current consensus (non staging)
        let consensus = self.ctx.consensus().session().await;

        // The proof is validated in the context of current consensus
        let proof =
            consensus.clone().spawn_blocking(move |c| c.validate_pruning_proof(&proof, &proof_metadata).map(|()| proof)).await?;

        // ADR-0042 review point (a), headers-proof path. Capture the authenticated header set here,
        // while the flow still owns `proof` and before it is moved into `apply_pruning_proof`'s
        // `spawn_blocking`. Everything in it has passed `validate_pruning_proof`: the per-DAA required
        // algo id, block level >= claimed level, proof of work, strictly increasing blue work versus
        // the proof parents, and the level-chain structure — and the proof as a whole has beaten the
        // local defender on accumulated blue work. It is the ONLY work-authenticated header set that
        // exists at `sync_pruning_point_palw_snapshot` time on this path.
        self.proof_validated_headers = Some(Arc::new(proof.iter().flatten().map(|header| (header.hash, header.clone())).collect()));

        let proof_pruning_point = proof[0].last().expect("was just ensured by validation").hash;

        if proof_pruning_point == self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the proof pruning point is the genesis block"));
        }

        if proof_pruning_point == consensus.async_pruning_point().await {
            return Err(ProtocolError::Other("the proof pruning point is the same as the current pruning point"));
        }
        drop(consensus);

        self.router
            .enqueue(make_message!(Payload::RequestPruningPointAndItsAnticone, RequestPruningPointAndItsAnticoneMessage {}))
            .await?;
        // First, all pruning points up to the last are sent
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPoints)?;
        let pruning_points: PruningPointsList = Versioned(self.header_format, msg).try_into()?;

        if pruning_points.is_empty() || pruning_points.last().unwrap().hash != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the last pruning point in the list"));
        }

        if pruning_points.first().unwrap().hash != self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the first pruning point in the list is expected to be genesis"));
        }

        // Check if past pruning points violate finality of current consensus
        if self.ctx.consensus().session().await.async_are_pruning_points_violating_finality(pruning_points.clone()).await {
            // TODO (relaxed): consider performing additional actions on finality conflicts in addition to disconnecting from the peer (e.g., banning, rpc notification)
            return Err(ProtocolError::Other("pruning points are violating finality"));
        }

        {
            // Sanity check for consistency between past pruning points and the headers proof
            let pruning_points_set: BlockHashSet = pruning_points.iter().map(|h| h.hash).collect();
            for level in proof.iter() {
                if let Some(root) = level.first()
                    && root.hash != self.ctx.config.genesis.hash
                    && !pruning_points_set.contains(&root.pruning_point)
                {
                    return Err(ProtocolError::Other("proof and past pruning points are inconsistent with each other"));
                }
            }
        }

        // Trusted data is sent in two stages:
        // The first, TrustedDataPackage, contains meta data about daa_window
        // blocks headers, and ghostdag data, which are required to verify the pruning
        // point and its anticone.
        // The latter, the trusted data entries, each represent a block (with daa) from the anticone of the pruning point
        // (including the PP itself), alongside indexing denoting the respective metadata headers or ghostdag data
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = Versioned(self.header_format, msg).try_into()?;
        let trusted_palw_snapshot_digest = pkg.palw_pruning_snapshot_digest;
        self.expected_palw_snapshot_digest = trusted_palw_snapshot_digest;
        // ADR-0042: the binding between the accumulated-work-validated package and the separately
        // chunked chain-derived bundle. `None` on every shipped peer.
        self.expected_palw_chain_derived_bundle_digest = pkg.palw_chain_derived_bundle_digest;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route, self.header_format);
        // The first entry of the trusted data is the pruning point itself.
        let Some(pruning_point_entry) = entry_stream.next().await? else {
            return Err(ProtocolError::Other("got `done` message before receiving the pruning point"));
        };

        if pruning_point_entry.block.hash() != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the expected trusted entry"));
        }

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }
        // Create a topologically ordered vector of  trusted blocks - the pruning point and its anticone,
        // and their daa windows headers
        let mut trusted_set = pkg.build_trusted_subdag(entries)?;

        if self.ctx.config.enable_sanity_checks {
            let con = self.ctx.consensus().unguarded_session_blocking();
            trusted_set = staging
                .clone()
                .spawn_blocking(move |c| {
                    let ref_proof = proof.clone();
                    c.apply_pruning_proof(proof, &trusted_set)?;
                    c.import_pruning_points(pruning_points)?;

                    info!("Building the proof which was just applied (sanity test)");
                    let built_proof = c.get_pruning_point_proof();
                    let mut mismatch_detected = false;
                    for (i, (ref_level, built_level)) in ref_proof.iter().zip(built_proof.iter()).enumerate() {
                        if ref_level.iter().map(|h| h.hash).collect::<BlockHashSet>()
                            != built_level.iter().map(|h| h.hash).collect::<BlockHashSet>()
                        {
                            mismatch_detected = true;
                            warn!("Locally built proof for level {} does not match the applied one", i);
                        }
                    }
                    if mismatch_detected {
                        info!("Validating the locally built proof (sanity test fallback #2)");
                        // Note: the proof is validated in the context of *current* consensus
                        if let Err(err) = con.validate_pruning_proof(&built_proof, &proof_metadata) {
                            panic!("Locally built proof failed validation: {}", err);
                        }
                        info!("Locally built proof was validated successfully");
                    } else {
                        info!("Proof was locally built successfully");
                    }
                    Result::<_, ProtocolError>::Ok(trusted_set)
                })
                .await?;
        } else {
            trusted_set = staging
                .clone()
                .spawn_blocking(move |c| {
                    c.apply_pruning_proof(proof, &trusted_set)?;
                    c.import_pruning_points(pruning_points)?;
                    Result::<_, ProtocolError>::Ok(trusted_set)
                })
                .await?;
        }

        // The proof/list imports above are staging-only. Before any trusted PP/anticone block can
        // advance fork-local PALW state, bind and atomically install the complete sidecar advertised
        // by the earlier trusted-data package. A missing digest is rejected on an active network.
        self.sync_pruning_point_palw_snapshot(staging, proof_pruning_point, trusted_palw_snapshot_digest, true).await?;

        // TODO (relaxed): add logs to staging commit process

        info!("Starting to process {} trusted blocks", trusted_set.len());
        let mut last_time = Instant::now();
        let mut last_index: usize = 0;
        for (i, tb) in trusted_set.into_iter().enumerate() {
            let now = Instant::now();
            let passed = now.duration_since(last_time);
            if passed > Duration::from_secs(1) {
                info!("Processed {} trusted blocks in the last {:.2}s (total {})", i - last_index, passed.as_secs_f64(), i);
                last_time = now;
                last_index = i;
            }
            // TODO (relaxed): queue and join in batches
            staging.validate_and_insert_trusted_block(tb).virtual_state_task.await?;
        }
        staging.async_clear_body_missing_anticone_set().await;
        info!("Done processing trusted blocks");
        Ok(proof_pruning_point)
    }

    async fn sync_headers(
        &mut self,
        consensus: &ConsensusProxy,
        syncer_virtual_selected_parent: BlockHash,
        highest_known_syncer_chain_hash: BlockHash,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        let highest_shared_header_score = consensus.async_get_header(highest_known_syncer_chain_hash).await?.daa_score;
        let mut progress_reporter = ProgressReporter::new(highest_shared_header_score, relay_block.header.daa_score, "block headers");

        self.router
            .enqueue(make_message!(
                Payload::RequestHeaders,
                RequestHeadersMessage {
                    low_hash: Some(highest_known_syncer_chain_hash.into()),
                    high_hash: Some(syncer_virtual_selected_parent.into())
                }
            ))
            .await?;
        let mut chunk_stream = HeadersChunkStream::new(&self.router, &mut self.incoming_route, self.header_format);

        if let Some(chunk) = chunk_stream.next().await? {
            let (mut prev_daa_score, mut prev_timestamp) = {
                let last_header = chunk.last().expect("chunk is never empty");
                (last_header.daa_score, last_header.timestamp)
            };
            let mut prev_jobs: Vec<BlockValidationFuture> =
                chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task).collect();

            while let Some(chunk) = chunk_stream.next().await? {
                let (current_daa_score, current_timestamp) = {
                    let last_header = chunk.last().expect("chunk is never empty");
                    (last_header.daa_score, last_header.timestamp)
                };
                let current_jobs = chunk
                    .into_iter()
                    .map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task)
                    .collect();
                let prev_chunk_len = prev_jobs.len();
                // Join the previous chunk so that we always concurrently process a chunk and receive another
                try_join_all(prev_jobs).await?;
                // Log the progress
                progress_reporter.report(prev_chunk_len, prev_daa_score, prev_timestamp);
                prev_daa_score = current_daa_score;
                prev_timestamp = current_timestamp;
                prev_jobs = current_jobs;
            }

            let prev_chunk_len = prev_jobs.len();
            try_join_all(prev_jobs).await?;
            progress_reporter.report_completion(prev_chunk_len);
        }

        if consensus.async_get_block_status(syncer_virtual_selected_parent).await.is_none() {
            // If the syncer's claimed sink header has still not been received, the peer is misbehaving
            return Err(ProtocolError::OtherOwned(format!(
                "did not receive syncer's virtual selected parent {} from peer {} during header download",
                syncer_virtual_selected_parent, self.router
            )));
        }

        self.sync_missing_relay_past_headers(consensus, syncer_virtual_selected_parent, relay_block.hash()).await?;

        Ok(())
    }

    /// Fetch, bound, decode and context-validate the complete PALW pruning boundary without mutating
    /// consensus. On the headers-proof path `trusted_digest` is mandatory and binds this later
    /// response to the earlier trusted-data package; catch-up still verifies the keyed payload digest
    /// and exact PP header.
    ///
    /// ADR-0042: when the preflight resolves to the chain-derived provenance (Header-v4, lever on, no
    /// operator pin), this additionally downloads the authentication bundle, proves review points (a)
    /// and (b) against `authority`, and verifies the boundary fold — all before returning, and
    /// therefore all before any consensus call.
    async fn download_pruning_point_palw_snapshot(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
        trusted_digest: Option<Hash64>,
        require_trusted_digest: bool,
        authority: ChainDerivedHeaderAuthority,
    ) -> Result<Option<DownloadedPalwPruningSnapshot>, ProtocolError> {
        if pruning_point == self.ctx.config.genesis.hash {
            return Ok(None);
        }
        let header = consensus.async_get_header(pruning_point).await?;
        if !self.ctx.config.params.is_palw_active(header.daa_score) {
            return Ok(None);
        }
        let preflight_auth = preflight_palw_snapshot_import_auth(
            header.version,
            pruning_point,
            trusted_digest,
            require_trusted_digest,
            &self.ctx.config.palw_pruning_snapshot_checkpoints,
            self.ctx.config.params.palw_spam.is_inert(),
            self.ctx.config.palw_permissionless_snapshot_auth,
            self.expected_palw_chain_derived_bundle_digest,
        )
        .map_err(ProtocolError::Other)?;

        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointPalwSnapshot,
                RequestPruningPointPalwSnapshotMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointPalwSnapshot, Duration::from_secs(600))?;
        if !msg.found {
            return Err(ProtocolError::Other(
                "peer cannot serve the complete PALW pruning snapshot required for pruned IBD on this network",
            ));
        }

        // The P2P envelope itself permits much larger messages. Enforce the snapshot-specific cap
        // before Borsh sees attacker-controlled collection lengths.
        if msg.snapshot.len() > kaspa_consensus_core::palw_pruned_frontier::MAX_PALW_PRUNING_SNAPSHOT_BYTES {
            return Err(ProtocolError::Other("PruningPointPalwSnapshot exceeds the accepted size cap"));
        }
        let snapshot: PalwPruningPointSnapshotV1 =
            borsh::from_slice(&msg.snapshot).map_err(|_| ProtocolError::Other("invalid PALW snapshot in PruningPointPalwSnapshot"))?;
        snapshot.validate_canonical().map_err(|_| ProtocolError::Other("non-canonical or corrupt PALW pruning snapshot"))?;
        if snapshot.payload.pruning_point != pruning_point || snapshot.payload.pruning_point_daa_score != header.daa_score {
            return Err(ProtocolError::Other("PALW pruning snapshot is bound to another pruning-point header"));
        }
        let import_auth =
            preflight_auth.unwrap_or_else(|| PalwPruningSnapshotImportAuth::legacy_header_v3(pruning_point, snapshot.payload_digest));
        debug_assert!(palw_pruned_ibd_snapshot_import_allowed(
            header.version,
            &import_auth,
            self.ctx.config.palw_permissionless_snapshot_auth
        ));
        let chain_derived = match import_auth.provenance {
            // ADR-0042. The chain-derived auth carries the all-zero sentinel digest by construction —
            // it is deliberately NOT an operator claim — so the digest-equality branch below must not
            // run for it. What replaces it is strictly more work, not less: the bundle is bound to the
            // package digest, every transported header is proven to restate an already-authenticated
            // header, the descendant is proven to be the buried chain child, and the payload is proven
            // to fold into that descendant's committed overlay root.
            PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle => Some(
                self.download_and_authenticate_chain_derived_bundle(consensus, pruning_point, &header, &snapshot, authority).await?,
            ),
            _ => {
                if snapshot.payload_digest != import_auth.checkpoint.payload_digest {
                    return Err(ProtocolError::Other(
                        "PALW pruning snapshot digest differs from its trusted-data/operator-checkpoint authentication",
                    ));
                }
                None
            }
        };

        Ok(Some(DownloadedPalwPruningSnapshot {
            daa_score: header.daa_score,
            header_version: header.version,
            spam_commitment: header.palw_spam_accumulator_commitment,
            import_auth,
            snapshot,
            chain_derived,
        }))
    }

    /// Download the ADR-0042 authentication bundle and authenticate it end to end.
    ///
    /// Order is the security property, so it is spelled out and must not be rearranged:
    ///
    /// 1. bind the transported bytes to the trusted-data package digest (when one was advertised);
    /// 2. **review point (a)** — prove every transported header restates a header from an already
    ///    authenticated set;
    /// 3. **review point (b)** — prove the descendant is exactly the buried selected-chain child this
    ///    node picked itself, plus the pure shape residue;
    ///    (2 and 3 are `bind_chain_derived_bundle_to_authenticated_headers` +
    ///    `palw_chain_derived_descendant_shape_is_valid`)
    /// 4. only then project with `extract_authenticated_bundle`, which authenticates nothing;
    /// 5. verify the boundary fold and the support-row binding against the downloaded payload.
    ///
    /// Step 4 before step 2 or 3 would be the whole scheme silently reduced to "trust the peer".
    async fn download_and_authenticate_chain_derived_bundle(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
        pruning_point_header: &Header,
        snapshot: &PalwPruningPointSnapshotV1,
        authority: ChainDerivedHeaderAuthority,
    ) -> Result<PalwChainDerivedAuthBundleV1, ProtocolError> {
        let wire = self.download_chain_derived_bundle(pruning_point).await?;

        // (1) The bundle rides in its own chunked transport, so the package digest is what ties those
        // bytes to the accumulated-work-validated package. `None` is only reachable on a path that
        // carries no package at all (catch-up), where the authenticated set is the local store.
        if let Some(expected) = self.expected_palw_chain_derived_bundle_digest
            && wire.digest() != expected
        {
            return Err(ProtocolError::Other("chain-derived PALW bundle does not match the digest advertised in trusted data"));
        }

        // (2)+(3) Review points (a) and (b): every transported header restates an already
        // authenticated header, and the descendant is exactly the block this node selected and buried.
        // Nothing below may run before this.
        let authenticated = self.authenticated_header_set(consensus, pruning_point, pruning_point_header, &wire, authority).await?;
        bind_chain_derived_bundle_to_authenticated_headers(&wire, &authenticated).map_err(ProtocolError::Other)?;

        // (3, pure residue) Header-v4, self-consistent identity, pruning point among the direct
        // parents, DAA strictly advancing. Necessary but not sufficient on its own — burial is what
        // authenticates, and that lives in `authenticated_header_set`.
        palw_chain_derived_descendant_shape_is_valid(&wire.descendant_header, pruning_point, pruning_point_header.daa_score)
            .map_err(|_| ProtocolError::Other("chain-derived descendant header is not a valid child of the pruning point"))?;

        // (4) Projection. By its own documentation this authenticates nothing beyond header
        // self-consistency; everything it is trusted for was established above.
        let bundle = wire
            .extract_authenticated_bundle()
            .map_err(|_| ProtocolError::Other("chain-derived PALW bundle failed its canonical projection"))?;

        // (5) The boundary itself: reconstruct the selected-parent PALW state from the transported
        // payload, fold it with the overlay root, require equality with the descendant's committed
        // `overlay_commitment_root`, and bind every anti-spam support row to a transported header
        // preimage. The importer runs this again at its own choke point; running it here means a bad
        // bundle is refused before consensus is touched at all.
        let walk_bound_daa =
            self.ctx.config.params.palw_batch_admission.paid_work_walk_bound_daa(self.ctx.config.params.palw_epoch_length_daa);
        verify_chain_derived_pruning_boundary_from_payload(&snapshot.payload, walk_bound_daa, &bundle)
            .map_err(|_| ProtocolError::Other("chain-derived PALW boundary does not fold into the authenticated descendant"))?;

        info!(
            "authenticated a chain-derived PALW pruning boundary for {} against descendant {} ({} support headers)",
            pruning_point,
            wire.descendant_header.hash,
            wire.support_headers.len()
        );
        Ok(bundle)
    }

    /// Produce the header set the transported bundle is allowed to restate, per `authority`, together
    /// with the one block the transported descendant is allowed to be.
    ///
    /// This is where ADR-0042 review points (a) and (b) get their inputs, because both burial and
    /// chain-child selection need reachability, the headers store and the syncer's tip — none of which
    /// a pure function can see.
    async fn authenticated_header_set(
        &self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
        pruning_point_header: &Header,
        wire: &PalwChainDerivedHeaderBundleWireV1,
        authority: ChainDerivedHeaderAuthority,
    ) -> Result<AuthenticatedChainDerivedHeaders, ProtocolError> {
        match authority {
            ChainDerivedHeaderAuthority::LocalValidatedChain { syncer_sink } => {
                let descendant =
                    self.select_local_chain_derived_descendant(consensus, pruning_point, pruning_point_header, syncer_sink).await?;
                let descendant_hash = descendant.hash;
                let mut headers = BlockHashMap::with_capacity(wire.support_headers.len().saturating_add(1));
                headers.insert(descendant_hash, descendant);
                // Every support header must already exist locally, having passed the unmodified header
                // pipeline. Looking each up BY THE TRANSPORTED HASH is safe only because
                // `bind_chain_derived_headers_to_authenticated_set` recomputes that hash from the
                // transported body first; a peer therefore cannot aim a forged body at a real hash.
                // A local miss is the pruned-below-the-boundary case and is a refusal, not a fallback.
                for header in &wire.support_headers {
                    let local = consensus.async_get_header(header.hash).await.map_err(|_| {
                        ProtocolError::Other(
                            "chain-derived support header is not present in the local validated header store; \
                             this node cannot authenticate the anti-spam closure",
                        )
                    })?;
                    headers.insert(local.hash, local);
                }
                Ok(AuthenticatedChainDerivedHeaders { headers, descendant: Some(descendant_hash) })
            }
            ChainDerivedHeaderAuthority::ProofValidatedHeaders => {
                // Structurally unsatisfiable, and deliberately expressed as code rather than prose: the
                // pruning proof contains only `future(root) ∩ past(pruning point)` per level, so it
                // holds no post-pruning-point descendant at all, and its level 0 is bounded by
                // `2 * pruning_proof_m` while the support closure is `span + 1` rows. The membership
                // test therefore fails first and reports the real reason, and `descendant: None` makes
                // the second failure unconditional so Phase B cannot land by accident.
                let headers = self.proof_validated_headers.as_ref().map(|set| set.as_ref().clone()).ok_or(ProtocolError::Other(
                    "chain-derived PALW import has no proof-validated header set to authenticate against",
                ))?;
                Ok(AuthenticatedChainDerivedHeaders { headers, descendant: None })
            }
        }
    }

    /// ADR-0042 review point (b), catch-up path: select the descendant from LOCAL state only.
    ///
    /// After `sync_headers` every post-pruning-point header has been validated by the unmodified
    /// pipeline and is buried under the chain being adopted, so the descendant is read from the store
    /// and the transported bundle is only ever allowed to restate it. No descendant bytes from the
    /// peer are trusted on this path.
    ///
    /// Uniqueness is deliberately not required and not asserted: `overlay_commitment_root` is a
    /// function of the pruning point alone, so every sibling whose selected parent is the pruning
    /// point commits the identical root. What is required is that the chosen block is *the* chain
    /// child and that it is buried:
    ///
    /// * its GHOSTDAG selected parent is exactly the pruning point;
    /// * it is a chain ancestor of the syncer's sink, i.e. it lies on the chain being adopted;
    /// * it has a block status, i.e. the local pipeline accepted it;
    /// * it is Header-v4 and its DAA score advances past the pruning point;
    /// * **burial floor**: the sink is at least `finality_depth()` blue score above it, and strictly
    ///   above it in blue work. This is the single quantitative knob in the scheme (ADR-0042 open
    ///   item 2). `finality_depth` is chosen because it is precisely the depth past which this node
    ///   already refuses to reorganize: accepting a boundary derived from a descendant shallower than
    ///   that would authenticate against history the node itself still considers revisable. Blue score
    ///   is a safe counter here — unlike on the headers-proof path — because every intervening header
    ///   has been difficulty-validated by the local pipeline.
    async fn select_local_chain_derived_descendant(
        &self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
        pruning_point_header: &Header,
        syncer_sink: BlockHash,
    ) -> Result<Arc<Header>, ProtocolError> {
        let sink_header = consensus.async_get_header(syncer_sink).await?;
        let children = consensus
            .async_get_block_children(pruning_point)
            .await
            .ok_or(ProtocolError::Other("the new pruning point has no known children; cannot derive a chain-derived boundary"))?;
        for child in children {
            if consensus.async_get_block_status(child).await.is_none() {
                continue;
            }
            let Ok(ghostdag) = consensus.async_get_ghostdag_data(child).await else { continue };
            if ghostdag.selected_parent != pruning_point {
                continue;
            }
            if !consensus.async_is_chain_ancestor_of(child, syncer_sink).await.unwrap_or(false) {
                continue;
            }
            let Ok(header) = consensus.async_get_header(child).await else { continue };
            if header.version != PALW_ANTISPAM_HEADER_VERSION || header.daa_score <= pruning_point_header.daa_score {
                continue;
            }
            if sink_header.blue_score < header.blue_score.saturating_add(self.ctx.config.finality_depth())
                || sink_header.blue_work <= header.blue_work
            {
                return Err(ProtocolError::Other(
                    "the chain child of the new pruning point is not buried deeply enough to authenticate a chain-derived boundary",
                ));
            }
            return Ok(header);
        }
        Err(ProtocolError::Other(
            "no locally validated Header-v4 chain child of the new pruning point is available on the syncer's chain",
        ))
    }

    /// Stream the chunked chain-derived bundle, bounding the total before Borsh sees any
    /// attacker-controlled collection length.
    async fn download_chain_derived_bundle(
        &mut self,
        pruning_point: BlockHash,
    ) -> Result<PalwChainDerivedHeaderBundleWireV1, ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestPalwChainDerivedBundle,
                RequestPalwChainDerivedBundleMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;

        let mut bytes: Vec<u8> = Vec::new();
        let mut expected_count: Option<u32> = None;
        let mut next_index: u32 = 0;
        loop {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::PalwChainDerivedBundleChunk, Duration::from_secs(600))?;
            if !msg.found {
                // Structural on a pruned peer: it retains the anti-spam rows but has deleted their
                // headers, so only an archival peer can answer at all.
                return Err(ProtocolError::Other(
                    "peer cannot serve the chain-derived PALW authentication bundle (archival retention of the anti-spam support headers is required)",
                ));
            }
            match expected_count {
                None => {
                    // Bound the announced chunk count against the byte cap on the FIRST chunk. Without
                    // this a peer could announce `u32::MAX` chunks and then trickle empty ones: the
                    // byte cap alone would never trip, so the stream would never terminate.
                    if msg.chunk_count == 0 || msg.chunk_count as usize > MAX_PALW_CHAIN_DERIVED_BUNDLE_CHUNKS {
                        return Err(ProtocolError::Other("chain-derived PALW bundle announced an out-of-range chunk count"));
                    }
                    expected_count = Some(msg.chunk_count);
                }
                Some(count) if count != msg.chunk_count => {
                    return Err(ProtocolError::Other("chain-derived PALW bundle changed its chunk count mid-stream"));
                }
                Some(_) => {}
            }
            if msg.chunk_index != next_index {
                return Err(ProtocolError::Other("chain-derived PALW bundle chunk arrived out of order"));
            }
            if bytes.len().saturating_add(msg.chunk.len()) > MAX_PALW_CHAIN_DERIVED_BUNDLE_BYTES {
                return Err(ProtocolError::Other("chain-derived PALW bundle exceeds the accepted size cap"));
            }
            bytes.extend_from_slice(&msg.chunk);
            next_index += 1;
            let count = expected_count.expect("set on the first chunk");
            if next_index >= count {
                break;
            }
            if next_index.is_multiple_of(PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BATCH as u32) {
                self.router
                    .enqueue(make_message!(
                        Payload::RequestNextPalwChainDerivedBundleChunks,
                        RequestNextPalwChainDerivedBundleChunksMessage {}
                    ))
                    .await?;
            }
        }
        // Drain the terminator so the route is left clean for the next request on this flow.
        dequeue_with_timeout!(self.incoming_route, Payload::DonePalwChainDerivedBundle, Duration::from_secs(600))?;

        let wire: PalwChainDerivedHeaderBundleWireV1 =
            borsh::from_slice(&bytes).map_err(|_| ProtocolError::Other("invalid chain-derived PALW authentication bundle"))?;
        // The cardinality fence has to bite HERE, not only where `extract_authenticated_bundle`
        // applies it. Between this point and that one, `authenticated_header_set` runs one
        // `async_get_header` — a `spawn_blocking` round trip — plus one `borsh::to_vec` per
        // transported support header, and a peer that names real local blocks makes every one of
        // those lookups SUCCEED, so nothing self-limits. Until this check the only bound on the count
        // is `MAX_PALW_CHAIN_DERIVED_BUNDLE_BYTES` (128 MiB), which admits roughly 115k headers
        // against a 65_536 ceiling: a ~1.8x overshoot of attacker-directed work per bundle. Ordered
        // before `validate_encoded_size` because it is O(1) on an already-parsed collection whereas
        // the envelope fence re-encodes. The identical check inside `extract_authenticated_bundle`
        // stays exactly where it is — that one is the projection's own invariant, this one is
        // transport-side defence in depth, and neither substitutes for the other.
        if wire.support_headers.len() > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS {
            return Err(ProtocolError::Other(
                "chain-derived PALW bundle carries more support headers than the anti-spam ceiling permits",
            ));
        }
        wire.validate_encoded_size().map_err(|_| ProtocolError::Other("chain-derived PALW bundle exceeds the accepted size cap"))?;
        Ok(wire)
    }

    /// The single place an authenticated chain-derived bundle would cross into consensus.
    ///
    /// The flow has, by this point, discharged every obligation ADR-0042 places on the *transport*:
    /// review point (a), review point (b), the support-row preimage binding and the boundary fold.
    ///
    /// READ THIS BEFORE DELETING THE GUARD. An earlier version of this comment justified the refusal
    /// by claiming the consensus import API "still takes no `Option<&PalwChainDerivedAuthBundleV1>`
    /// parameter" and that "both production callers inside `consensus/` hard-code `None`". That is
    /// **false** in this build, and anyone who deletes the guard on the strength of that sentence
    /// ships the live gap described in (2) below. The plumbing exists end to end:
    ///
    /// * `ConsensusApi::import_pruning_point_palw_snapshot_with_chain_derived_auth` and
    ///   `ConsensusApi::intrusive_pruning_point_update_with_palw_snapshot_and_chain_derived_auth`
    ///   both take the bundle (`consensus/core/src/api/mod.rs`);
    /// * `Consensus` implements both (`consensus/src/consensus/mod.rs`), and `ConsensusProxy` exposes
    ///   the intrusive one (`components/consensusmanager/src/session.rs`);
    /// * the importer honours it, lever-gated and provenance-gated, strictly before any durable write
    ///   (`consensus/src/pipeline/virtual_processor/processor.rs`).
    ///
    /// The guard is therefore a POLICY fence, not a plumbing stub. It stays until BOTH of the
    /// following are discharged — and neither can be discharged from inside this file:
    ///
    /// 1. **ADR-0042's external gates.** The ADR conditions this path on an independent review of the
    ///    chain-derived authentication scheme and on a multi-node Header-v4 soak. Neither has
    ///    happened. Both are external; no amount of local testing closes them.
    /// 2. **The paid-work attribution gap is still open.**
    ///    `reconstruct_selected_parent_state_from_pruning_payload`
    ///    (`consensus/core/src/palw_pruned_frontier.rs`) folds only the deduplicated UNION of
    ///    paid-work nullifiers. Each `PalwPrunedPaidWorkBlockV1` row also carries `block_hash` and
    ///    `block_daa_score`, and NEITHER enters any commitment, while
    ///    `prepare_pruning_point_palw_snapshot_import` performs no store cross-check on
    ///    `payload.paid_work`. So a peer can hold the nullifier union byte-identical and re-date rows
    ///    anywhere inside the admission window: the fold at the pruning point still matches EXACTLY,
    ///    yet `palw_paid_work_window` filters persisted rows against an ADVANCING `anchor_daa`, so
    ///    epochs later the victim's window diverges from the network's, it derives a different
    ///    `selected_parent_palw_state_root`, and it rejects HONEST blocks with
    ///    `BadOverlayCommitment` — permanent desync, at zero work to the attacker. The operator-pin
    ///    provenance is unaffected, because its digest covers those bytes verbatim; this is a
    ///    chain-derived-only gap and it must be closed (the row attribution has to be committed, or
    ///    cross-checked against the headers store) before the fence comes out.
    ///
    /// Refusing here is not a regression: the guard is reachable only with the lever on and no
    /// operator pin, a configuration that already fails IBD today with
    /// "requires a matching local --palw-pruning-snapshot-checkpoint". With the lever off
    /// `downloaded.chain_derived` is `None` at both call sites, so this is a no-op and the shipped
    /// operator-pin and v3 paths are byte-identical.
    ///
    /// When (1) and (2) are genuinely discharged — demonstrated, not asserted — the two call sites
    /// below pass `downloaded.chain_derived` to the `_with_chain_derived_auth` variants and this
    /// guard is deleted. Not before.
    fn chain_derived_import_is_wired(downloaded: &DownloadedPalwPruningSnapshot) -> Result<(), ProtocolError> {
        if downloaded.chain_derived.is_some() {
            return Err(ProtocolError::Other(
                "chain-derived PALW boundary is fully authenticated by the transport but this build refuses to import it \
                 (ADR-0042: independent review and multi-node soak are outstanding, and the paid-work row attribution \
                 -- block_hash/block_daa_score -- is not yet covered by any commitment)",
            ));
        }
        Ok(())
    }

    /// Existing staging/current-boundary importer. Catch-up uses the download helper directly and
    /// passes the result to the atomic intrusive-PP API instead.
    async fn sync_pruning_point_palw_snapshot(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
        trusted_digest: Option<Hash64>,
        require_trusted_digest: bool,
    ) -> Result<(), ProtocolError> {
        let Some(downloaded) = self
            .download_pruning_point_palw_snapshot(
                consensus,
                pruning_point,
                trusted_digest,
                require_trusted_digest,
                // Headers-proof / staging path: nothing above the pruning point exists yet, so the
                // validated pruning proof is the only work-authenticated header set available.
                ChainDerivedHeaderAuthority::ProofValidatedHeaders,
            )
            .await?
        else {
            return Ok(());
        };
        Self::chain_derived_import_is_wired(&downloaded)?;
        consensus
            .clone()
            .spawn_blocking(move |c| {
                c.import_pruning_point_palw_snapshot(
                    pruning_point,
                    downloaded.daa_score,
                    downloaded.header_version,
                    downloaded.spam_commitment,
                    downloaded.import_auth,
                    downloaded.snapshot,
                )
            })
            .await?;
        info!("imported the complete PALW pruning snapshot of {}", pruning_point);
        Ok(())
    }

    async fn sync_new_utxo_set(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
        install_palw_boundary: bool,
    ) -> Result<(), ProtocolError> {
        // Install the complete boundary before deleting the recoverable old UTXO set. The UTXO
        // importer later cross-checks every still-locked provider bond against the downloaded set.
        if install_palw_boundary && pruning_point != self.ctx.config.genesis.hash {
            self.sync_pruning_point_palw_snapshot(consensus, pruning_point, self.expected_palw_snapshot_digest, false).await?;
        }
        // A better solution could be to create a copy of the old utxo state for some sort of fallback rather than delete it.
        consensus.async_clear_pruning_utxo_set().await; // this deletes the old pruning utxoset and also sets the pruning utxo as invalidated
        self.sync_pruning_point_utxoset(consensus, pruning_point).await?;
        // kaspa-pq ADR-0022: import the pruning point's EVM execution state + DNS/PoS-v2 overlay snapshot
        // as part of the SAME "make the pruning point usable" step — BEFORE marking the utxoset stable.
        // Atomicity matters: async_set_pruning_utxoset_stable() below latches is_utxo_stable=true, and a
        // later IbdType::Sync SKIPS re-import while that flag is true. If a sidecar import failed (the
        // peer can answer not-found) or the node crashed AFTER the utxoset was marked stable but BEFORE
        // the sidecars landed, the node would be permanently missing the EVM/overlay state and would
        // disqualify every post-pruning block with no path to recover. Importing the sidecars first keeps
        // "utxoset + EVM + overlay" all-or-nothing w.r.t. the stable flag (a failure leaves the utxoset
        // unstable, so the next IBD re-runs the whole import). Skipped at genesis: there is no
        // below-genesis state and the peer has captured no snapshot (it would answer not-found and abort).
        if pruning_point != self.ctx.config.genesis.hash {
            self.sync_pruning_point_evm_state(consensus, pruning_point).await?;
            self.sync_pruning_point_overlay_snapshot(consensus, pruning_point).await?;
        }
        // Only if the function has reached here (utxoset + EVM + overlay all imported), is the utxo "final"
        consensus.async_set_pruning_utxoset_stable().await;
        // Once a new utxoset is stored, the utxoindex needs to be resynced as well. This happens through the reset handler mechanism.
        let consensus_manager = self.ctx.consensus_manager.clone();
        spawn_blocking(move || consensus_manager.invoke_consensus_reset_handlers()).await.unwrap();
        self.ctx.on_pruning_point_utxoset_override();
        Ok(())
    }

    async fn sync_missing_relay_past_headers(
        &mut self,
        consensus: &ConsensusProxy,
        syncer_virtual_selected_parent: BlockHash,
        relay_block_hash: BlockHash,
    ) -> Result<(), ProtocolError> {
        // Finished downloading syncer selected tip blocks,
        // check if we already have the triggering relay block
        if consensus.async_get_block_status(relay_block_hash).await.is_some() {
            return Ok(());
        }

        // Send a special header request for the sink antipast. This is expected to
        // be a relatively small set since virtual and relay blocks should be close topologically.
        // See server-side handling of `RequestAnticone` for further details.
        self.router
            .enqueue(make_message!(
                Payload::RequestAntipast,
                RequestAntipastMessage {
                    block_hash: Some(syncer_virtual_selected_parent.into()),
                    context_hash: Some(relay_block_hash.into())
                }
            ))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockHeaders)?;
        let chunk: HeadersChunk = Versioned(self.header_format, msg).try_into()?;
        let jobs: Vec<BlockValidationFuture> =
            chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task).collect();
        try_join_all(jobs).await?;
        dequeue_with_timeout!(self.incoming_route, Payload::DoneHeaders)?;

        if consensus.async_get_block_status(relay_block_hash).await.is_none() {
            // If the relay block has still not been received, the peer is misbehaving
            Err(ProtocolError::OtherOwned(format!(
                "did not receive relay block {} from peer {} during header download",
                relay_block_hash, self.router
            )))
        } else {
            Ok(())
        }
    }

    async fn validate_staging_timestamps(
        &self,
        consensus: &ConsensusProxy,
        staging_consensus: &ConsensusProxy,
    ) -> Result<(), ProtocolError> {
        // The purpose of this check is to prevent the potential abuse explained here:
        // https://github.com/kaspanet/research/issues/3#issuecomment-895243792
        let staging_hst = staging_consensus.async_get_header(staging_consensus.async_get_headers_selected_tip().await).await.unwrap();
        let current_hst = consensus.async_get_header(consensus.async_get_headers_selected_tip().await).await.unwrap();
        // If staging is behind current or within 10 minutes ahead of it, then something is wrong and we reject the IBD
        if staging_hst.timestamp < current_hst.timestamp || staging_hst.timestamp - current_hst.timestamp < 600_000 {
            Err(ProtocolError::OtherOwned(format!(
                "The difference between the timestamp of the current selected tip ({}) and the 
staging selected tip ({}) is too small or negative. Aborting IBD...",
                current_hst.timestamp, staging_hst.timestamp
            )))
        } else {
            Ok(())
        }
    }

    async fn sync_pruning_point_utxoset(&mut self, consensus: &ConsensusProxy, pruning_point: BlockHash) -> Result<(), ProtocolError> {
        info!("downloading the pruning point utxoset, this can take a little while.");
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointUtxoSet,
                RequestPruningPointUtxoSetMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let mut chunk_stream = PruningPointUtxosetChunkStream::new(&self.router, &mut self.incoming_route);
        let mut multiset = MuHash::new();
        while let Some(chunk) = chunk_stream.next().await? {
            multiset = consensus
                .clone()
                .spawn_blocking(move |c| {
                    c.append_imported_pruning_point_utxos(&chunk, &mut multiset);
                    multiset
                })
                .await;
        }
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_utxo_set(pruning_point, multiset)).await?;
        Ok(())
    }

    /// kaspa-pq ADR-0022: request + import the pruning point's EVM execution state. Required on an
    /// EVM-active network so the first post-pruning block re-executes against the real parent state.
    async fn sync_pruning_point_evm_state(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
    ) -> Result<(), ProtocolError> {
        let evm_active = {
            let pp = pruning_point;
            let pp_daa = consensus.clone().spawn_blocking(move |c| c.get_header(pp)).await.map(|h| h.daa_score).unwrap_or(0);
            self.ctx.config.is_evm_active(pp_daa)
        };
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointEvmState,
                RequestPruningPointEvmStateMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointEvmState, Duration::from_secs(600))?;
        if !msg.found {
            if evm_active {
                return Err(ProtocolError::Other(
                    "peer cannot serve the pruning point EVM state required for pruned IBD on this network",
                ));
            }
            return Ok(()); // EVM-inactive network — no EVM state to import.
        }
        // Audit H-03: bound the accepted bytes BEFORE deserializing so a malicious
        // IBD peer cannot send a near-1-GiB (P2P decode ceiling) or gzip-bomb-
        // decompressed payload that borsh expands into a huge nested allocation.
        // These are generous ceilings for the real pruning-point state (the current
        // nets are far smaller); a chunked/streamed manifest is the deeper follow-up.
        const MAX_EVM_HEADER_BYTES: usize = 1 << 20; // 1 MiB (the header is tiny)
        const MAX_EVM_STATE_SNAPSHOT_BYTES: usize = 256 << 20; // 256 MiB
        if msg.evm_header.len() > MAX_EVM_HEADER_BYTES {
            return Err(ProtocolError::Other("PruningPointEvmState header exceeds the accepted size cap"));
        }
        if msg.evm_state_snapshot.len() > MAX_EVM_STATE_SNAPSHOT_BYTES {
            return Err(ProtocolError::Other("PruningPointEvmState snapshot exceeds the accepted size cap"));
        }
        let header: kaspa_consensus_core::evm::EvmExecutionHeader = borsh::from_slice(&msg.evm_header)
            .map_err(|_| ProtocolError::Other("invalid EVM execution header in PruningPointEvmState"))?;
        let snapshot: kaspa_consensus_core::evm::EvmStateSnapshot = borsh::from_slice(&msg.evm_state_snapshot)
            .map_err(|_| ProtocolError::Other("invalid EVM state snapshot in PruningPointEvmState"))?;
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_evm_state(pruning_point, header, snapshot)).await?;
        info!("imported the EVM state of the pruning point {}", pruning_point);
        Ok(())
    }

    /// kaspa-pq ADR-0022: request + import the pruning point's DNS/PoS-v2 overlay snapshot. Required
    /// on an overlay-active network so the first post-pruning block's coinbase `c == v` reproduces.
    async fn sync_pruning_point_overlay_snapshot(
        &mut self,
        consensus: &ConsensusProxy,
        pruning_point: BlockHash,
    ) -> Result<(), ProtocolError> {
        let overlay_active = self.ctx.config.dns_params.is_some();
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointOverlaySnapshot,
                RequestPruningPointOverlaySnapshotMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointOverlaySnapshot, Duration::from_secs(600))?;
        if !msg.found {
            if overlay_active {
                return Err(ProtocolError::Other(
                    "peer cannot serve the pruning point overlay snapshot required for pruned IBD on this network",
                ));
            }
            return Ok(()); // overlay dormant — nothing to import.
        }
        // Audit H-03: bound the accepted overlay snapshot bytes before deserializing
        // (bonds + reward windows are far smaller than the EVM state).
        const MAX_OVERLAY_SNAPSHOT_BYTES: usize = 64 << 20; // 64 MiB
        if msg.overlay_snapshot.len() > MAX_OVERLAY_SNAPSHOT_BYTES {
            return Err(ProtocolError::Other("PruningPointOverlaySnapshot exceeds the accepted size cap"));
        }
        let snapshot: kaspa_consensus_core::dns_finality::OverlaySnapshot = borsh::from_slice(&msg.overlay_snapshot)
            .map_err(|_| ProtocolError::Other("invalid overlay snapshot in PruningPointOverlaySnapshot"))?;
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_overlay_snapshot(pruning_point, snapshot)).await?;
        info!("imported the overlay snapshot of the pruning point {}", pruning_point);
        Ok(())
    }

    async fn sync_missing_trusted_bodies(&mut self, consensus: &ConsensusProxy) -> Result<(), ProtocolError> {
        info!("downloading pruning point anticone missing block data");
        let diesembodied_hashes = consensus.async_get_body_missing_anticone().await;
        if self.body_only_ibd_permitted {
            self.sync_missing_trusted_bodies_no_headers(consensus, diesembodied_hashes).await?
        } else {
            self.sync_missing_trusted_bodies_full_blocks(consensus, diesembodied_hashes).await?;
        }
        consensus.async_clear_body_missing_anticone_set().await;
        Ok(())
    }
    async fn sync_missing_trusted_bodies_no_headers(
        &mut self,
        consensus: &ConsensusProxy,
        diesembodied_hashes: Vec<BlockHash>,
    ) -> Result<(), ProtocolError> {
        let iter = diesembodied_hashes.chunks(IBD_BATCH_SIZE);
        for chunk in iter {
            self.router
                .enqueue(make_message!(
                    Payload::RequestBlockBodies,
                    RequestBlockBodiesMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
                ))
                .await?;
            let mut jobs = Vec::with_capacity(chunk.len());

            for &hash in chunk.iter() {
                let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockBody)?;
                // kaspa-pq EVM Lane v0.4 (§3.1): the body response carries the
                // block's own EVM payload — without it a reassembled v2 block
                // would fail its `evm_payload_hash` body rule (a VALID block
                // rejected). The header (already stored + hash-validated)
                // commits to the payload hash, so a tampered payload is caught
                // by body validation.
                let (blk_body, evm_payload): (BlockBody, kaspa_consensus_core::evm::EvmExecutionPayload) = msg.try_into()?;
                // TODO (relaxed): make header queries in a batch.
                let blk_header = consensus.async_get_header(hash).await.map_err(|err| {
                    // Conceptually this indicates local inconsistency, since we received the expected hashes via a local
                    // get_missing_block_body_hashes call. However for now we fail gracefully and only disconnect from this peer.
                    ProtocolError::OtherOwned(format!("syncee inconsistency: missing block header for {}, err: {}", hash, err))
                })?;
                if blk_body.is_empty() {
                    return Err(ProtocolError::OtherOwned(format!("sent empty block body for block {}", hash)));
                }
                let block = Block { header: blk_header, transactions: blk_body.into(), evm_payload: Arc::new(evm_payload) };
                // TODO (relaxed): sending ghostdag data may be redundant, especially when the headers were already verified.
                // Consider sending empty ghostdag data, simplifying a great deal. The result should be the same -
                // a trusted task is sent, however the header is already verified, and hence only the block body will be verified.
                jobs.push(
                    consensus
                        .validate_and_insert_trusted_block(TrustedBlock::new(block, consensus.async_get_ghostdag_data(hash).await?))
                        .virtual_state_task,
                );
            }
            try_join_all(jobs).await?; // TODO (relaxed): be more efficient with batching as done with block bodies in general
        }
        Ok(())
    }
    async fn sync_missing_trusted_bodies_full_blocks(
        &mut self,
        consensus: &ConsensusProxy,
        diesembodied_hashes: Vec<BlockHash>,
    ) -> Result<(), ProtocolError> {
        let iter = diesembodied_hashes.chunks(IBD_BATCH_SIZE);
        for chunk in iter {
            self.router
                .enqueue(make_message!(
                    Payload::RequestIbdBlocks,
                    RequestIbdBlocksMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
                ))
                .await?;
            let mut jobs = Vec::with_capacity(chunk.len());

            for &hash in chunk.iter() {
                // TODO: change to BodyOnly requests when incorporated
                let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdBlock)?;
                let block: Block = Versioned(self.header_format, msg).try_into()?;
                if block.hash() != hash {
                    return Err(ProtocolError::OtherOwned(format!("expected block {} but got {}", hash, block.hash())));
                }
                if block.is_header_only() {
                    return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
                }
                // TODO (relaxed): sending ghostdag data may be redundant, especially when the headers were already verified.
                // Consider sending empty ghostdag data, simplifying a great deal. The result should be the same -
                // a trusted task is sent, however the header is already verified, and hence only the block body will be verified.
                jobs.push(
                    consensus
                        .validate_and_insert_trusted_block(TrustedBlock::new(block, consensus.async_get_ghostdag_data(hash).await?))
                        .virtual_state_task,
                );
            }
            try_join_all(jobs).await?; // TODO (relaxed): be more efficient with batching as done with block bodies in general
        }
        Ok(())
    }
    async fn sync_missing_block_bodies(&mut self, consensus: &ConsensusProxy, high: BlockHash) -> Result<(), ProtocolError> {
        // TODO (relaxed): query consensus in batches
        let sleep_task = sleep(Duration::from_secs(2));
        let hashes_task = consensus.async_get_missing_block_body_hashes(high);
        tokio::pin!(sleep_task);
        tokio::pin!(hashes_task);
        let hashes = match select(sleep_task, hashes_task).await {
            Either::Left((_, hashes_task)) => {
                // We select between the tasks in order to inform the user if this operation is taking too long. On full IBD
                // this operation requires traversing the full DAG which indeed might take several seconds or even minutes.
                info!(
                    "IBD: searching for missing block bodies to request from peer {}. This operation might take several seconds.",
                    self.router
                );
                // Now re-await the original task
                hashes_task.await
            }
            Either::Right((hashes_result, _)) => hashes_result,
        }?;
        if hashes.is_empty() {
            return Ok(());
        }

        // The total gives the operator the denominator for the "Processed M block bodies (P%)"
        // progress lines below — without it, a percentage that resets to a low value right after
        // the header phase reached 100% reads like a rollback when it is just a new phase. The
        // header phase and this body phase each have their OWN ProgressReporter (each starting at
        // 0% over its own DAA-score window), so a body percent below the header percent is normal.
        info!(
            "IBD: {} missing block bodies to sync from peer {} (header sync is already complete; this is the body phase)",
            hashes.len(),
            self.router
        );

        let low_header = consensus.async_get_header(*hashes.first().expect("hashes was non empty")).await?;
        let high_header = consensus.async_get_header(*hashes.last().expect("hashes was non empty")).await?;
        let mut progress_reporter = ProgressReporter::new(low_header.daa_score, high_header.daa_score, "block bodies");

        let mut iter = hashes.chunks(IBD_BATCH_SIZE);
        let QueueChunkOutput { jobs: mut prev_jobs, daa_score: mut prev_daa_score, timestamp: mut prev_timestamp } =
            self.queue_block_processing_chunk(consensus, iter.next().expect("hashes was non empty")).await?;

        for chunk in iter {
            let QueueChunkOutput { jobs: current_jobs, daa_score: current_daa_score, timestamp: current_timestamp } =
                self.queue_block_processing_chunk(consensus, chunk).await?;
            let prev_chunk_len = prev_jobs.len();
            // Join the previous chunk so that we always concurrently process a chunk and receive another
            try_join_all(prev_jobs).await?;
            // Log the progress
            progress_reporter.report(prev_chunk_len, prev_daa_score, prev_timestamp);
            prev_daa_score = current_daa_score;
            prev_timestamp = current_timestamp;
            prev_jobs = current_jobs;
        }

        let prev_chunk_len = prev_jobs.len();
        try_join_all(prev_jobs).await?;
        progress_reporter.report_completion(prev_chunk_len);

        Ok(())
    }

    async fn queue_block_processing_chunk(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[BlockHash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        if self.body_only_ibd_permitted {
            self.queue_block_processing_chunk_body_only(consensus, chunk).await
        } else {
            self.queue_block_processing_chunk_full_block(consensus, chunk).await
        }
    }

    async fn queue_block_processing_chunk_full_block(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[BlockHash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
        let mut current_timestamp = 0;
        self.router
            .enqueue(make_message!(
                Payload::RequestIbdBlocks,
                RequestIbdBlocksMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
            ))
            .await?;
        for &expected_hash in chunk {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdBlock)?;
            let block: Block = Versioned(self.header_format, msg).try_into()?;
            if block.hash() != expected_hash {
                return Err(ProtocolError::OtherOwned(format!("expected block {} but got {}", expected_hash, block.hash())));
            }
            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
            }
            current_daa_score = block.header.daa_score;
            current_timestamp = block.header.timestamp;
            jobs.push(consensus.validate_and_insert_block(block).virtual_state_task);
        }
        Ok(QueueChunkOutput { jobs, daa_score: current_daa_score, timestamp: current_timestamp })
    }

    async fn queue_block_processing_chunk_body_only(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[BlockHash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
        let mut current_timestamp = 0;
        self.router
            .enqueue(make_request!(
                Payload::RequestBlockBodies,
                RequestBlockBodiesMessage { hashes: chunk.iter().map(|h| h.into()).collect() },
                self.incoming_route.id()
            ))
            .await?;
        for &expected_hash in chunk {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockBody)?;
            // TODO (relaxed): make header queries in a batch.
            let blk_header = consensus.async_get_header(expected_hash).await.map_err(|err| {
                // Conceptually this indicates local inconsistency, since we received the expected hashes via a local
                // get_missing_block_body_hashes call. However for now we fail gracefully and only disconnect from this peer.
                ProtocolError::OtherOwned(format!("syncee inconsistency: missing block header for {}, err: {}", expected_hash, err))
            })?;
            // kaspa-pq EVM Lane v0.4 (§3.1): the body response carries the block's
            // own EVM payload (see sync_missing_trusted_bodies_no_headers).
            let (blk_body, evm_payload): (BlockBody, kaspa_consensus_core::evm::EvmExecutionPayload) = msg.try_into()?;
            if blk_body.is_empty() {
                return Err(ProtocolError::OtherOwned(format!("sent empty block body for block {}", expected_hash)));
            }
            let block = Block { header: blk_header, transactions: blk_body.into(), evm_payload: Arc::new(evm_payload) };
            current_daa_score = block.header.daa_score;
            current_timestamp = block.header.timestamp;
            jobs.push(consensus.validate_and_insert_block(block).virtual_state_task);
        }
        Ok(QueueChunkOutput { jobs, daa_score: current_daa_score, timestamp: current_timestamp })
    }
}

#[cfg(test)]
mod palw_snapshot_auth_tests {
    use super::*;
    use kaspa_consensus_core::palw_pruned_frontier::PalwPruningSnapshotImportProvenance;

    fn h(word: u64) -> Hash64 {
        Hash64::from_u64_word(word)
    }

    /// The pre-ADR-0042 call shape: lever OFF, no advertised chain-derived bundle. Every historical
    /// assertion below goes through this wrapper unchanged, which is what pins the safety property —
    /// with the lever off the preflight is byte-identical to what shipped, refusal texts included.
    fn preflight_palw_snapshot_import_auth(
        header_version: u16,
        pruning_point: BlockHash,
        trusted_digest: Option<Hash64>,
        require_trusted_digest: bool,
        operator_checkpoints: &[PalwPruningSnapshotCheckpoint],
        palw_spam_is_inert: bool,
    ) -> Result<Option<PalwPruningSnapshotImportAuth>, &'static str> {
        super::preflight_palw_snapshot_import_auth(
            header_version,
            pruning_point,
            trusted_digest,
            require_trusted_digest,
            operator_checkpoints,
            palw_spam_is_inert,
            false,
            None,
        )
    }

    #[test]
    fn header_v3_keeps_legacy_trusted_data_semantics() {
        let pruning_point = h(1);
        assert!(preflight_palw_snapshot_import_auth(PALW_HEADER_VERSION, pruning_point, None, true, &[], true).is_err());
        assert_eq!(preflight_palw_snapshot_import_auth(PALW_HEADER_VERSION, pruning_point, None, false, &[], true), Ok(None));

        let auth =
            preflight_palw_snapshot_import_auth(PALW_HEADER_VERSION, pruning_point, Some(h(2)), true, &[], true).unwrap().unwrap();
        assert_eq!(auth.provenance, PalwPruningSnapshotImportProvenance::LegacyHeaderV3);
        assert_eq!(auth.checkpoint, PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(2) });
    }

    #[test]
    fn header_v4_requires_local_pin_and_cross_checks_peer_digest_when_present() {
        let pruning_point = h(3);
        let checkpoint = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(4) };
        assert!(
            preflight_palw_snapshot_import_auth(PALW_ANTISPAM_HEADER_VERSION, pruning_point, Some(h(4)), true, &[], false).is_err()
        );

        let without_peer =
            preflight_palw_snapshot_import_auth(PALW_ANTISPAM_HEADER_VERSION, pruning_point, None, true, &[checkpoint], false)
                .unwrap()
                .unwrap();
        assert_eq!(without_peer.provenance, PalwPruningSnapshotImportProvenance::OperatorPinnedCheckpoint);
        assert_eq!(without_peer.checkpoint, checkpoint);
        assert!(
            preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                Some(checkpoint.payload_digest),
                true,
                &[checkpoint],
                false,
            )
            .is_ok()
        );
        assert!(
            preflight_palw_snapshot_import_auth(PALW_ANTISPAM_HEADER_VERSION, pruning_point, Some(h(5)), true, &[checkpoint], false,)
                .is_err()
        );
    }

    #[test]
    fn network_schema_mismatch_is_rejected_before_peer_sidecar_request() {
        let pruning_point = h(8);
        let checkpoint = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(9) };
        assert!(preflight_palw_snapshot_import_auth(PALW_HEADER_VERSION, pruning_point, None, false, &[checkpoint], false).is_err());
        assert!(
            preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                Some(checkpoint.payload_digest),
                true,
                &[checkpoint],
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn future_header_version_is_closed_even_with_operator_pin() {
        let checkpoint = PalwPruningSnapshotCheckpoint { pruning_point: h(6), payload_digest: h(7) };
        assert!(
            preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION + 1,
                checkpoint.pruning_point,
                Some(checkpoint.payload_digest),
                false,
                &[checkpoint],
                false,
            )
            .is_err()
        );
    }

    // ---------------------------------------------------------------------
    // ADR-0042: the chain-derived arm. Everything below is unreachable on a default node.
    // ---------------------------------------------------------------------

    /// The fence itself: an unpinned Header-v4 boundary is refused with the historical message while
    /// the lever is off, and only the lever changes that. Nothing a peer sends can flip it.
    #[test]
    fn header_v4_without_a_pin_is_closed_unless_the_node_local_lever_is_set() {
        let pruning_point = h(0x11);
        let lever_off = super::preflight_palw_snapshot_import_auth(
            PALW_ANTISPAM_HEADER_VERSION,
            pruning_point,
            None,
            false,
            &[],
            false,
            false,
            Some(h(0x99)), // even with a peer advertising a bundle
        );
        assert_eq!(lever_off, Err("Header-v4 PALW pruned IBD requires a matching local --palw-pruning-snapshot-checkpoint"));

        let lever_on = super::preflight_palw_snapshot_import_auth(
            PALW_ANTISPAM_HEADER_VERSION,
            pruning_point,
            None,
            false,
            &[],
            false,
            true,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(lever_on.provenance, PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle);
        // The sentinel, not a synthesized operator claim: the importer must branch on provenance.
        assert_eq!(lever_on.checkpoint, PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: Hash64::default() });
    }

    /// The lever must not widen the Header-v3 path in any direction.
    #[test]
    fn the_lever_does_not_touch_the_header_v3_path() {
        let pruning_point = h(0x12);
        for require in [false, true] {
            for digest in [None, Some(h(0x13))] {
                assert_eq!(
                    super::preflight_palw_snapshot_import_auth(
                        PALW_HEADER_VERSION,
                        pruning_point,
                        digest,
                        require,
                        &[],
                        true,
                        false,
                        Some(h(0x14)),
                    ),
                    super::preflight_palw_snapshot_import_auth(
                        PALW_HEADER_VERSION,
                        pruning_point,
                        digest,
                        require,
                        &[],
                        true,
                        true,
                        Some(h(0x14)),
                    ),
                );
            }
        }
    }

    /// A pinned boundary keeps pinned semantics with the lever on, including the conflict check: a
    /// peer cannot downgrade a pinned node to chain-derived authentication by withholding its digest.
    #[test]
    fn the_operator_pin_still_wins_when_the_lever_is_on() {
        let pruning_point = h(0x15);
        let checkpoint = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(0x16) };
        let pinned = super::preflight_palw_snapshot_import_auth(
            PALW_ANTISPAM_HEADER_VERSION,
            pruning_point,
            None,
            true,
            &[checkpoint],
            false,
            true,
            Some(h(0x17)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(pinned.provenance, PalwPruningSnapshotImportProvenance::OperatorPinnedCheckpoint);
        assert_eq!(pinned.checkpoint, checkpoint);
        assert!(
            super::preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                Some(h(0x18)),
                true,
                &[checkpoint],
                false,
                true,
                None,
            )
            .is_err()
        );
    }

    /// On any path that carries a trusted-data package, the bundle digest is what binds the separately
    /// chunked bytes to the accumulated-work-validated package. No digest, no chain-derived import.
    #[test]
    fn the_headers_proof_path_refuses_an_unbound_chain_derived_bundle() {
        let pruning_point = h(0x19);
        assert!(
            super::preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                None,
                true,
                &[],
                false,
                true,
                None,
            )
            .is_err()
        );
        assert!(
            super::preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                None,
                true,
                &[],
                false,
                true,
                Some(h(0x1a)),
            )
            .is_ok()
        );
    }

    /// The three refusal texts the preflight can produce. Pinned as constants so the tests below
    /// assert WHICH refusal fired, not merely that one did — precedence is the property under test.
    const SCHEMA_MISMATCH: &str = "PALW pruning-point header version does not match this network's configured PALW schema";
    const PIN_REQUIRED: &str = "Header-v4 PALW pruned IBD requires a matching local --palw-pruning-snapshot-checkpoint";
    const UNBOUND_BUNDLE: &str =
        "Header-v4 chain-derived PALW import requires the bundle digest in trusted data; no local operator checkpoint either";

    /// The lever admits chain-derived authentication for a Header-v4 boundary with NO operator pin —
    /// the entire point of ADR-0042 — on both IBD entry points, and the auth it produces is the one
    /// the consensus admission matrix accepts only while the lever is on.
    ///
    /// The two entry points differ in one argument and the difference is load-bearing: catch-up
    /// carries no trusted-data package (`require_trusted_digest = false`), so there is nothing to bind
    /// a bundle digest to and none is demanded; the headers-proof path carries one, so the separately
    /// chunked bundle bytes must be bound to it.
    #[test]
    fn the_lever_admits_chain_derived_without_an_operator_pin_on_both_ibd_entry_points() {
        let pruning_point = h(0x30);
        // A pin the operator holds for some OTHER boundary must not count as a pin for this one.
        let elsewhere = PalwPruningSnapshotCheckpoint { pruning_point: h(0x31), payload_digest: h(0x32) };
        let expected = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: Hash64::default() };

        // Catch-up: no package, so no bundle digest is required or expected.
        let catchup = super::preflight_palw_snapshot_import_auth(
            PALW_ANTISPAM_HEADER_VERSION,
            pruning_point,
            None,
            false,
            &[elsewhere],
            false,
            true,
            None,
        )
        .unwrap()
        .unwrap();
        assert_eq!(catchup.provenance, PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle);
        assert_eq!(catchup.checkpoint, expected, "chain-derived carries the all-zero sentinel, never a synthesized operator claim");

        // Headers-proof: the package is present, so the bundle must ride bound to its digest.
        let headers_proof = super::preflight_palw_snapshot_import_auth(
            PALW_ANTISPAM_HEADER_VERSION,
            pruning_point,
            None,
            true,
            &[elsewhere],
            false,
            true,
            Some(h(0x33)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(headers_proof.provenance, PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle);
        assert_eq!(headers_proof.checkpoint, expected);

        // The auth the preflight hands onward must be exactly what the consensus admission matrix
        // admits — and only with the lever on. This is the production `debug_assert!` at the
        // snapshot download site, asserted for real.
        for auth in [catchup, headers_proof] {
            assert!(palw_pruned_ibd_snapshot_import_allowed(PALW_ANTISPAM_HEADER_VERSION, &auth, true));
            assert!(!palw_pruned_ibd_snapshot_import_allowed(PALW_ANTISPAM_HEADER_VERSION, &auth, false));
            assert!(!palw_pruned_ibd_snapshot_import_allowed(PALW_HEADER_VERSION, &auth, true), "chain-derived is Header-v4 ONLY");
        }

        // Same inputs, lever off: the historical refusal, unchanged. The pin for another boundary is
        // still not a pin for this one.
        assert_eq!(
            super::preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION,
                pruning_point,
                None,
                false,
                &[elsewhere],
                false,
                false,
                Some(h(0x33)),
            ),
            Err(PIN_REQUIRED)
        );
    }

    /// The network-schema check runs FIRST, so the lever can never be reached on a boundary whose
    /// header version disagrees with this network's configured PALW schema. Both directions of the
    /// mismatch are pinned by exact refusal text: a Header-v4 boundary offered to a node whose
    /// anti-spam schema is inert (configured Header-v3), and a Header-v3 boundary offered to a
    /// Header-v4 node. Neither becomes a chain-derived import, with or without a pin or a bundle.
    #[test]
    fn a_network_schema_mismatch_is_still_rejected_before_the_lever_is_consulted() {
        let pruning_point = h(0x34);
        let pin = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(0x35) };
        for pins in [&[][..], &[pin][..]] {
            for require in [false, true] {
                // v4 boundary, node configured for v3 (inert anti-spam schema).
                assert_eq!(
                    super::preflight_palw_snapshot_import_auth(
                        PALW_ANTISPAM_HEADER_VERSION,
                        pruning_point,
                        None,
                        require,
                        pins,
                        true,
                        true,
                        Some(h(0x36)),
                    ),
                    Err(SCHEMA_MISMATCH),
                    "the lever must not reach a v4 boundary on a v3-configured network"
                );
                // v3 boundary, node configured for v4.
                assert_eq!(
                    super::preflight_palw_snapshot_import_auth(
                        PALW_HEADER_VERSION,
                        pruning_point,
                        Some(pin.payload_digest),
                        require,
                        pins,
                        false,
                        true,
                        Some(h(0x36)),
                    ),
                    Err(SCHEMA_MISMATCH),
                    "the lever must not reach a v3 boundary on a v4-configured network"
                );
            }
        }
    }

    /// The safety property that makes ADR-0042 landable, as an exhaustive claim rather than a
    /// sampled one: over the whole preflight input matrix, turning the lever on changes the answer
    /// for EXACTLY ONE cell — a Header-v4 boundary, on a Header-v4 network, with no local pin for
    /// that boundary. Every other cell is identical, refusal text included, so a lever-off node
    /// cannot observe that this change happened.
    #[test]
    fn the_lever_changes_exactly_one_cell_of_the_preflight_matrix() {
        let pruning_point = h(0x40);
        let pin = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: h(0x41) };
        let elsewhere = PalwPruningSnapshotCheckpoint { pruning_point: h(0x42), payload_digest: h(0x43) };
        let sentinel = PalwPruningSnapshotCheckpoint { pruning_point, payload_digest: Hash64::default() };
        let mut opened = 0usize;
        for header_version in [PALW_HEADER_VERSION, PALW_ANTISPAM_HEADER_VERSION, PALW_ANTISPAM_HEADER_VERSION + 1] {
            for palw_spam_is_inert in [false, true] {
                for require_trusted_digest in [false, true] {
                    for trusted_digest in [None, Some(pin.payload_digest), Some(h(0x44))] {
                        for pins in [&[][..], &[elsewhere][..], &[pin][..], &[elsewhere, pin][..]] {
                            for bundle_digest in [None, Some(h(0x45))] {
                                let call = |permissionless_enabled| {
                                    super::preflight_palw_snapshot_import_auth(
                                        header_version,
                                        pruning_point,
                                        trusted_digest,
                                        require_trusted_digest,
                                        pins,
                                        palw_spam_is_inert,
                                        permissionless_enabled,
                                        bundle_digest,
                                    )
                                };
                                let (off, on) = (call(false), call(true));
                                let is_the_open_cell = header_version == PALW_ANTISPAM_HEADER_VERSION
                                    && !palw_spam_is_inert
                                    && !pins.iter().any(|checkpoint| checkpoint.pruning_point == pruning_point);
                                if !is_the_open_cell {
                                    assert_eq!(
                                        off, on,
                                        "the lever must be inert at (version {header_version}, inert {palw_spam_is_inert}, \
                                         require {require_trusted_digest}, trusted {trusted_digest:?}, pins {pins:?}, \
                                         bundle {bundle_digest:?})"
                                    );
                                    continue;
                                }
                                opened += 1;
                                assert_eq!(off, Err(PIN_REQUIRED), "lever off keeps the historical refusal verbatim");
                                let expected_on = if require_trusted_digest && bundle_digest.is_none() {
                                    Err(UNBOUND_BUNDLE)
                                } else {
                                    Ok(Some(PalwPruningSnapshotImportAuth {
                                        checkpoint: sentinel,
                                        provenance: PalwPruningSnapshotImportProvenance::ChainDerivedHeaderBundle,
                                    }))
                                };
                                assert_eq!(on, expected_on);
                            }
                        }
                    }
                }
            }
        }
        // 1 version x 1 schema x 2 require x 3 trusted x 2 unpinned x 2 bundle.
        assert_eq!(opened, 24, "the open cell must be exactly the unpinned Header-v4-on-Header-v4 column");
    }

    /// A future header version stays closed even with the lever on: chain-derived is Header-v4 ONLY.
    #[test]
    fn the_lever_does_not_open_any_version_other_than_header_v4() {
        assert!(
            super::preflight_palw_snapshot_import_auth(
                PALW_ANTISPAM_HEADER_VERSION + 1,
                h(0x1b),
                None,
                false,
                &[],
                false,
                true,
                Some(h(0x1c)),
            )
            .is_err()
        );
    }

    // --- ADR-0042 review point (a), as executable assertions -------------------------------------

    fn v4_header(nonce: u64, parent: BlockHash) -> Header {
        Header::new_finalized(
            PALW_ANTISPAM_HEADER_VERSION,
            vec![vec![parent]].try_into().unwrap(),
            Default::default(),
            Default::default(),
            Default::default(),
            234,
            23,
            nonce,
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_KHEAVYHASH,
            7,
            0.into(),
            0,
            Default::default(),
        )
    }

    fn wire_bundle(pruning_point: BlockHash) -> PalwChainDerivedHeaderBundleWireV1 {
        let mut descendant = v4_header(1, pruning_point);
        descendant.overlay_commitment_root = h(0xcc);
        descendant.finalize();
        let mut support = v4_header(2, pruning_point);
        support.palw_spam_accumulator_commitment = h(0x11);
        support.finalize();
        PalwChainDerivedHeaderBundleWireV1 {
            descendant_header: descendant,
            support_headers: vec![support],
            dns_overlay_snapshot: kaspa_consensus_core::dns_finality::OverlaySnapshot {
                bonds: vec![],
                reserve_balance: 0,
                window: vec![],
            },
        }
    }

    fn authenticated_set(bundle: &PalwChainDerivedHeaderBundleWireV1) -> BlockHashMap<Arc<Header>> {
        std::iter::once(&bundle.descendant_header)
            .chain(bundle.support_headers.iter())
            .map(|header| (header.hash, Arc::new(header.clone())))
            .collect()
    }

    /// What a healthy catch-up path produces: the locally selected descendant pinned, plus the local
    /// headers for every transported support hash.
    fn authenticated(bundle: &PalwChainDerivedHeaderBundleWireV1) -> AuthenticatedChainDerivedHeaders {
        AuthenticatedChainDerivedHeaders { headers: authenticated_set(bundle), descendant: Some(bundle.descendant_header.hash) }
    }

    #[test]
    fn an_honest_bundle_restates_the_authenticated_set() {
        let bundle = wire_bundle(h(0x20));
        assert_eq!(bind_chain_derived_headers_to_authenticated_set(&bundle, &authenticated_set(&bundle)), Ok(()));
        assert_eq!(bind_chain_derived_bundle_to_authenticated_headers(&bundle, &authenticated(&bundle)), Ok(()));
    }

    /// The core of review point (a): a header the authenticated set does not contain is refused. This
    /// is exactly what happens on the headers-proof path, where the proof provably contains neither
    /// the post-pruning-point descendant nor the 32,769-row anti-spam closure.
    #[test]
    fn a_header_outside_the_authenticated_set_is_refused() {
        let bundle = wire_bundle(h(0x21));
        let mut set = authenticated_set(&bundle);
        set.remove(&bundle.support_headers[0].hash);
        assert!(bind_chain_derived_headers_to_authenticated_set(&bundle, &set).is_err());

        let mut set = authenticated_set(&bundle);
        set.remove(&bundle.descendant_header.hash);
        assert!(bind_chain_derived_headers_to_authenticated_set(&bundle, &set).is_err());

        // An empty authenticated set can never admit anything.
        assert!(bind_chain_derived_headers_to_authenticated_set(&bundle, &BlockHashMap::new()).is_err());
    }

    /// F6 regression, at the transport boundary. `Header` derives `BorshDeserialize` with `hash` as
    /// its first field, so a wire header's block hash is peer-chosen. Without recomputing it, a peer
    /// could aim a forged body at a genuinely authenticated hash and the set lookup would succeed.
    #[test]
    fn a_borsh_forged_block_hash_is_refused_even_when_it_names_an_authenticated_block() {
        let honest = wire_bundle(h(0x22));
        let set = authenticated_set(&honest);
        let authenticated_hash = honest.support_headers[0].hash;

        // A different body wearing an authenticated block's hash, surviving a real Borsh round trip.
        let mut forged_body = v4_header(9, h(0x22));
        forged_body.palw_spam_accumulator_commitment = h(0xdead);
        forged_body.finalize();
        forged_body.hash = authenticated_hash;
        let forged = PalwChainDerivedHeaderBundleWireV1 { support_headers: vec![forged_body], ..honest.clone() };
        let round_tripped: PalwChainDerivedHeaderBundleWireV1 = borsh::from_slice(&borsh::to_vec(&forged).unwrap()).unwrap();
        assert_eq!(round_tripped.support_headers[0].hash, authenticated_hash, "the forgery must survive the wire verbatim");
        assert!(bind_chain_derived_headers_to_authenticated_set(&round_tripped, &set).is_err());
    }

    /// A mutated body whose cached hash was never re-finalized is refused by the same identity check,
    /// before the set lookup can be reached.
    #[test]
    fn a_stale_cached_hash_is_refused() {
        let mut bundle = wire_bundle(h(0x23));
        let set = authenticated_set(&bundle);
        bundle.descendant_header.overlay_commitment_root = h(0xbeef); // no finalize()
        assert!(bind_chain_derived_headers_to_authenticated_set(&bundle, &set).is_err());
    }

    /// Review point (b): being *in* the authenticated set is not enough. The transported descendant
    /// must be exactly the block this node selected and buried, otherwise a peer could nominate any
    /// locally stored header-only block that lists the pruning point among its parents — whose
    /// `overlay_commitment_root` is unvalidated, since that is a body rule — and forge a boundary for
    /// the cost of one block.
    #[test]
    fn a_descendant_that_is_merely_in_the_authenticated_set_is_refused() {
        let pruning_point = h(0x26);
        let mut bundle = wire_bundle(pruning_point);
        // A second, genuinely-known post-pruning-point block: in the set, but not the pinned one.
        let mut impostor = v4_header(0xabc, pruning_point);
        impostor.overlay_commitment_root = h(0xf00d); // an arbitrary boundary of the peer's choosing
        impostor.finalize();
        let pinned = bundle.descendant_header.hash;

        let mut headers = authenticated_set(&bundle);
        headers.insert(impostor.hash, Arc::new(impostor.clone()));
        bundle.descendant_header = impostor;

        // (a) alone accepts it — which is exactly why (b) exists.
        assert_eq!(bind_chain_derived_headers_to_authenticated_set(&bundle, &headers), Ok(()));
        assert!(
            bind_chain_derived_bundle_to_authenticated_headers(
                &bundle,
                &AuthenticatedChainDerivedHeaders { headers, descendant: Some(pinned) }
            )
            .is_err()
        );
    }

    /// A path with no work-authenticated descendant — the headers-proof path — is an unconditional
    /// refusal, never "any member of the set will do".
    #[test]
    fn a_path_without_a_work_authenticated_descendant_is_refused_outright() {
        let bundle = wire_bundle(h(0x27));
        let authenticated = AuthenticatedChainDerivedHeaders { headers: authenticated_set(&bundle), descendant: None };
        assert!(bind_chain_derived_bundle_to_authenticated_headers(&bundle, &authenticated).is_err());
    }

    /// Review point (b)'s pure residue, wired here so the transport-side call is covered: the
    /// descendant must be Header-v4, self-consistent, list the pruning point among its direct parents
    /// and advance the DAA score.
    #[test]
    fn the_descendant_shape_residue_is_enforced_at_the_transport_boundary() {
        let pruning_point = h(0x24);
        let bundle = wire_bundle(pruning_point);
        assert!(palw_chain_derived_descendant_shape_is_valid(&bundle.descendant_header, pruning_point, 0).is_ok());
        // Not a child of this pruning point.
        assert!(palw_chain_derived_descendant_shape_is_valid(&bundle.descendant_header, h(0x25), 0).is_err());
        // Does not advance past the pruning point (the fixture header sits at DAA 7).
        assert!(palw_chain_derived_descendant_shape_is_valid(&bundle.descendant_header, pruning_point, 7).is_err());
    }
}
