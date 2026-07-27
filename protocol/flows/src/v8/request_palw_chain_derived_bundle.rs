//! ADR-0042: serve the chain-derived (permissionless) Header-v4 pruning-boundary authentication
//! bundle — the descendant header whose committed `overlay_commitment_root` the boundary must fold
//! into, the complete anti-spam support-row header closure below the pruning point, and the pruning
//! point's DNS/PoS-v2 overlay snapshot.
//!
//! Serving is unconditional and carries no trust: everything in the bundle is re-derived and
//! re-checked by the requester against sets it has authenticated itself (ADR-0042 review points
//! (a)/(b)), so a dishonest server can only make a requester refuse the bundle, never accept a false
//! boundary. That is why this flow needs no lever of its own — the lever fences *import*, not
//! *service*.
//!
//! `found = false` means this node cannot serve the COMPLETE closure. The dominant reason is
//! structural rather than transient: the pruning processor deletes below-pruning-point headers unless
//! the node is `--archival` (it deliberately retains the anti-spam *rows*, but not their headers), so
//! in practice only archival nodes can answer. A partial closure is never sent: a missing support
//! header is indistinguishable to the requester from a withheld one, so all-or-nothing is what keeps
//! "peer cannot serve" from degrading into "peer served a shorter closure".

use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_consensus_core::{
    BlockHash,
    api::ConsensusApi,
    constants::PALW_ANTISPAM_HEADER_VERSION,
    palw_pruned_frontier::{
        MAX_PALW_CHAIN_DERIVED_BUNDLE_BYTES, MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS, PalwChainDerivedHeaderBundleWireV1,
    },
};
use kaspa_core::debug;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, make_message,
    pb::{DonePalwChainDerivedBundleMessage, PalwChainDerivedBundleChunkMessage, kaspad_message::Payload},
};
use std::sync::Arc;

/// Wire chunk size. Chosen so the worst-case bundle (`MAX_PALW_CHAIN_DERIVED_BUNDLE_BYTES` = 128 MiB)
/// is at most 128 chunks, each two orders of magnitude below the 1 GiB P2P decode ceiling
/// (`P2P_MAX_MESSAGE_SIZE`), leaving the framing cost negligible while keeping any single decode
/// allocation small.
pub(crate) const PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BYTES: usize = 1 << 20;

/// Chunks streamed before the server waits for an explicit `RequestNext…`, mirroring the UTXO-set and
/// trusted-entry batching so a slow requester cannot be flooded.
pub(crate) const PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BATCH: usize = 16;

/// The most chunks an honest response can consist of. Derived from the two constants above so it can
/// never drift from them. The requester enforces it on the first chunk: the byte cap alone does not
/// terminate a stream of empty chunks, so the announced count needs its own bound.
pub(crate) const MAX_PALW_CHAIN_DERIVED_BUNDLE_CHUNKS: usize =
    MAX_PALW_CHAIN_DERIVED_BUNDLE_BYTES.div_ceil(PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BYTES);

pub struct RequestPalwChainDerivedBundleFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPalwChainDerivedBundleFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPalwChainDerivedBundleFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestPalwChainDerivedBundle)?;
            let pp = BlockHash::try_from(
                msg.pruning_point_hash.ok_or(ProtocolError::Other("chain-derived bundle request is missing the pruning point"))?,
            )
            .map_err(|_| ProtocolError::Other("chain-derived bundle request carries an invalid pruning point hash"))?;
            self.handle_request(pp).await?;
        }
    }

    async fn handle_request(&mut self, pruning_point: BlockHash) -> Result<(), ProtocolError> {
        let bytes = self.encoded_bundle(pruning_point).await;
        match bytes {
            None => {
                debug!("cannot serve a chain-derived PALW bundle for {}", pruning_point);
                self.router
                    .enqueue(make_message!(
                        Payload::PalwChainDerivedBundleChunk,
                        PalwChainDerivedBundleChunkMessage { found: false, chunk_index: 0, chunk_count: 0, chunk: vec![] }
                    ))
                    .await?;
            }
            Some(bytes) => {
                let chunk_count = bytes.len().div_ceil(PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BYTES).max(1);
                debug!("serving a {}-byte chain-derived PALW bundle for {} in {} chunks", bytes.len(), pruning_point, chunk_count);
                for (index, chunk) in bytes.chunks(PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BYTES).enumerate() {
                    self.router
                        .enqueue(make_message!(
                            Payload::PalwChainDerivedBundleChunk,
                            PalwChainDerivedBundleChunkMessage {
                                found: true,
                                chunk_index: index as u32,
                                chunk_count: chunk_count as u32,
                                chunk: chunk.to_vec(),
                            }
                        ))
                        .await?;
                    if (index + 1) % PALW_CHAIN_DERIVED_BUNDLE_CHUNK_BATCH == 0 && index + 1 < chunk_count {
                        dequeue!(self.incoming_route, Payload::RequestNextPalwChainDerivedBundleChunks)?;
                    }
                }
            }
        }
        self.router.enqueue(make_message!(Payload::DonePalwChainDerivedBundle, DonePalwChainDerivedBundleMessage {})).await?;
        Ok(())
    }

    async fn encoded_bundle(&self, pruning_point: BlockHash) -> Option<Vec<u8>> {
        // Header-v3 (anti-spam-inert) networks have no support-row closure to bind, so there is
        // nothing a chain-derived bundle could authenticate. Refuse before touching consensus.
        if self.ctx.config.params.palw_spam.is_inert() {
            return None;
        }
        let session = self.ctx.consensus().unguarded_session();
        let bundle = session.spawn_blocking(move |c| build_chain_derived_bundle(c, pruning_point)).await?;
        // Never emit an encoding the receiver is required to reject.
        bundle.validate_encoded_size().ok()?;
        borsh::to_vec(&bundle).ok()
    }
}

/// Assemble the bundle from local, fully-validated state. Every `None` return is a refusal to serve,
/// never a partial answer.
fn build_chain_derived_bundle(c: &dyn ConsensusApi, pruning_point: BlockHash) -> Option<PalwChainDerivedHeaderBundleWireV1> {
    // Only the node's *current* pruning point has all three artifacts captured together (PALW
    // snapshot, overlay snapshot, retained anti-spam closure). Serving a historical boundary would
    // mean serving a closure this node no longer retains rows for.
    if c.pruning_point() != pruning_point {
        return None;
    }
    let snapshot = c.pruning_point_palw_snapshot()?;
    if snapshot.payload.pruning_point != pruning_point {
        return None;
    }
    let spam = snapshot.payload.spam_accumulator.as_ref()?;
    if spam.support_rows.len() > MAX_PALW_PRUNING_SPAM_SUPPORT_ROWS {
        return None;
    }
    let overlay = c.pruning_point_overlay_snapshot()?;
    if overlay.pruning_point != pruning_point {
        return None;
    }

    // The support set the requester will demand is exactly {pruning-point row} ∪ {support rows} — the
    // same set `verify_support_rows_against_transported_headers` reconstructs. Serve every header or
    // none: a pruned (non-archival) node holds these rows but has deleted their headers, and that is
    // precisely the case this must report as "cannot serve".
    let mut support_headers = Vec::with_capacity(spam.support_rows.len().saturating_add(1));
    for hash in std::iter::once(pruning_point).chain(spam.support_rows.iter().map(|row| row.block_hash)) {
        let header = c.get_header(hash).ok()?;
        // Below Header-v4 `palw_spam_accumulator_commitment` is hash-invisible, so such a header could
        // not bind its row and the requester would reject the whole bundle.
        if header.version != PALW_ANTISPAM_HEADER_VERSION {
            return None;
        }
        support_headers.push((*header).clone());
    }

    let descendant_header = (*c.get_header(select_chain_child(c, pruning_point)?).ok()?).clone();
    Some(PalwChainDerivedHeaderBundleWireV1 { descendant_header, support_headers, dns_overlay_snapshot: overlay.snapshot })
}

/// The unique selected-chain child of the pruning point on this node's headers-selected chain.
///
/// This is a *hint*: the requester never trusts it. On the catch-up path it re-derives the descendant
/// from its own full-pipeline-validated store and requires the transported header to restate that
/// local header field-for-field (ADR-0042 review point (b)); on the headers-proof path it requires
/// membership in the accumulated-work-validated proof set. Serving a wrong child can therefore only
/// make the requester refuse.
fn select_chain_child(c: &dyn ConsensusApi, pruning_point: BlockHash) -> Option<BlockHash> {
    let sink = c.get_headers_selected_tip();
    c.get_block_children(pruning_point)?.into_iter().find(|&child| {
        c.get_header(child).is_ok_and(|header| header.version == PALW_ANTISPAM_HEADER_VERSION)
            && c.get_ghostdag_data(child).is_ok_and(|gd| gd.selected_parent == pruning_point)
            && c.is_chain_ancestor_of(child, sink).unwrap_or(false)
    })
}
