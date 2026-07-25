//! Bounded, read-only PALW operator snapshot derived at the current sink.

use kaspa_consensus_core::{
    palw::{
        da::PalwDaChallengeStatusV1, effective_provider_bond_status, palw_lagged_activation_open, palw_seed_carry_run,
        provider_bond_release_daa_score,
    },
    palw_probe::{
        PalwActivationProbe, PalwBatchProbe, PalwDaChallengeProbe, PalwDaObligationProbe, PalwProviderBondProbe,
        PalwSearchChallengeProbe, PalwStateProbe,
        PalwStateProbeError,
    },
    tx::TransactionOutpoint,
};
use kaspa_database::prelude::StoreErrorPredicates;
use kaspa_hashes::Hash64;

use super::Consensus;
use crate::{
    model::stores::{
        headers::HeaderStoreReader, palw::PalwStoreReader, palw_da::PalwDaStoreReader,
        palw_provider_bonds::PalwProviderBondsStoreReader, palw_search_availability::PalwSearchAvailabilityStoreReader,
        virtual_state::VirtualStateStoreReader,
    },
    processes::palw::{resolve_palw_buried_epoch_seeds, resolve_palw_lagged_anchor},
};

impl Consensus {
    pub(crate) fn palw_state_probe_impl(
        &self,
        batch_id: Option<Hash64>,
        provider_bond: Option<TransactionOutpoint>,
    ) -> Result<PalwStateProbe, PalwStateProbeError> {
        // Keep the sink, fork-local carried view, and selected-chain provider registry under the same
        // snapshot guard. Virtual commit takes the matching write lock while changing those surfaces.
        // PALW blob writes are direct and do not take this lock; their presence fields remain explicitly
        // diagnostic rather than an atomic/fork-scoped acceptance claim.
        let virtual_read = self.virtual_stores.read();
        let virtual_state =
            virtual_read.state.get().map_err(|error| PalwStateProbeError::Store(format!("virtual state: {error:?}")))?;
        let sink = virtual_state.ghostdag_data.selected_parent;
        let sink_daa_score = self
            .storage
            .headers_store
            .get_daa_score(sink)
            .map_err(|error| PalwStateProbeError::Store(format!("sink header: {error:?}")))?;
        let enabled = self.config.params.palw_activation_daa_score != u64::MAX;

        let view = if enabled {
            self.storage
                .palw_overlay_view_store
                .view(sink)
                .map_err(|error| PalwStateProbeError::Store(format!("sink overlay view: {error:?}")))?
        } else {
            None
        };
        let overlay_view_available = view.is_some();

        let batch = match (batch_id, view.as_ref()) {
            (Some(batch_id), Some(view)) => match view.entry(&batch_id).cloned() {
                Some(lifecycle) => {
                    let manifest = match self.storage.palw_store.batch_manifest(batch_id) {
                        Ok(manifest) => Some((*manifest).clone()),
                        Err(error) if error.is_key_not_found() => None,
                        Err(error) => return Err(PalwStateProbeError::Store(format!("batch manifest: {error:?}"))),
                    };
                    // Defense in depth for corrupted/legacy state: the operator RPC remains bounded
                    // even if a lifecycle row claims a leaf count beyond the activated network cap.
                    let scan_limit = lifecycle.leaf_count.min(self.config.params.palw_batch_admission.max_batch_leaves);
                    let leaf_scan_complete = scan_limit == lifecycle.leaf_count;
                    let mut leaf_blobs_present = 0;
                    for leaf_index in 0..scan_limit {
                        if self
                            .storage
                            .palw_store
                            .has_leaf(batch_id, leaf_index)
                            .map_err(|error| PalwStateProbeError::Store(format!("leaf presence: {error:?}")))?
                        {
                            leaf_blobs_present += 1;
                        }
                    }
                    let certificate_blob_present = match lifecycle.cert_hash {
                        Some(cert_hash) => match self.storage.palw_store.certificate(cert_hash) {
                            Ok(_) => true,
                            Err(error) if error.is_key_not_found() => false,
                            Err(error) => {
                                return Err(PalwStateProbeError::Store(format!("certificate presence: {error:?}")));
                            }
                        },
                        None => false,
                    };
                    Some(PalwBatchProbe {
                        batch_id,
                        lifecycle,
                        manifest,
                        leaf_blobs_present,
                        leaf_scan_complete,
                        certificate_blob_present,
                    })
                }
                None => None,
            },
            _ => None,
        };

        // Open DA challenges on the requested bond, read under the same virtual snapshot guard. Bounded
        // by construction: reported only for the one requested outpoint and only for Open challenges.
        let da_challenges = match (provider_bond, enabled) {
            (Some(wanted), true) => {
                let state = self
                    .storage
                    .palw_da_store
                    .read()
                    .state(sink)
                    .map_err(|error| PalwStateProbeError::Store(format!("DA state: {error:?}")))?;
                state
                    .challenges
                    .iter()
                    .filter(|(_, challenge)| {
                        challenge.provider_bond == wanted && matches!(challenge.status, PalwDaChallengeStatusV1::Open)
                    })
                    .map(|(challenge_id, challenge)| PalwDaChallengeProbe {
                        challenge_id: *challenge_id,
                        provider_bond: challenge.provider_bond,
                        object_root: challenge.object_root,
                        chunk_index: challenge.chunk_index,
                        opened_daa_score: challenge.challenge.opened_daa_score,
                        response_deadline_daa_score: challenge.challenge.response_deadline_daa_score,
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        // DA obligations for the requested batch (all statuses), read under the same virtual snapshot
        // guard. Bounded by the batch's leaf/provider sample fan-out. Reported only when a batch id was
        // requested. These expose the obligation ids + sampled chunk indices a challenger needs to open
        // a 0x3a — the node previously surfaced only OPEN challenges, so a mock lifecycle had no way to
        // learn which obligation to challenge in order to satisfy `certificate_allowed`.
        let da_obligations = match (batch_id, enabled) {
            (Some(wanted), true) => {
                let state = self
                    .storage
                    .palw_da_store
                    .read()
                    .state(sink)
                    .map_err(|error| PalwStateProbeError::Store(format!("DA state: {error:?}")))?;
                state
                    .obligations
                    .values()
                    .filter(|obligation| obligation.batch_id == wanted)
                    .map(|obligation| {
                        use kaspa_consensus_core::palw::da::PalwDaObligationStatusV1;
                        let status = match obligation.status {
                            PalwDaObligationStatusV1::Pending => 0,
                            PalwDaObligationStatusV1::Challenged(_) => 1,
                            PalwDaObligationStatusV1::Satisfied(_) => 2,
                            PalwDaObligationStatusV1::TimedOut(_) => 3,
                        };
                        PalwDaObligationProbe {
                            obligation_id: obligation.obligation_id,
                            provider_bond: obligation.provider_bond,
                            object_root: obligation.object_root,
                            chunk_index: obligation.chunk_index,
                            status,
                            beacon_epoch: obligation.beacon_epoch,
                            retention_until_daa_score: obligation.retention_until_daa_score,
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        // Open search-availability challenges anchored by the requested bond (as scheduler), read
        // under the same virtual snapshot guard. Same bounded contract as `da_challenges`.
        let search_challenges = match (provider_bond, enabled) {
            (Some(wanted), true) => {
                let state = self
                    .storage
                    .palw_search_availability_store
                    .read()
                    .state(sink)
                    .map_err(|error| PalwStateProbeError::Store(format!("search availability state: {error:?}")))?;
                state
                    .obligations
                    .iter()
                    .filter_map(|(object_root, obligation)| {
                        use kaspa_consensus_core::palw::search_snapshot::PalwSearchObligationStatusV1;
                        if obligation.scheduler_bond != wanted {
                            return None;
                        }
                        let PalwSearchObligationStatusV1::Challenged { response_deadline_daa_score, chunk_index } =
                            obligation.status
                        else {
                            return None;
                        };
                        Some(PalwSearchChallengeProbe {
                            object_root: *object_root,
                            scheduler_bond: obligation.scheduler_bond,
                            chunk_index,
                            response_deadline_daa_score,
                            availability_deadline_daa_score: obligation.anchor.availability_deadline_daa_score,
                        })
                    })
                    .collect()
            }
            _ => Vec::new(),
        };

        let provider_bond = match provider_bond {
            Some(outpoint) if enabled => match self.storage.palw_provider_bonds_store.read().get(&outpoint) {
                Ok(record) => Some(PalwProviderBondProbe {
                    effective_status: effective_provider_bond_status(&record, sink_daa_score),
                    release_daa_score: provider_bond_release_daa_score(&record, self.config.params.palw_epoch_length_daa),
                    record: (*record).clone(),
                }),
                Err(error) if error.is_key_not_found() => None,
                Err(error) => return Err(PalwStateProbeError::Store(format!("provider bond: {error:?}"))),
            },
            _ => None,
        };

        // The LAGGED activation signal (review §6.4): the exact anchor walk + buried per-epoch seed
        // sampler the virtual processor's Certified→Active gate consumes, re-run read-only at the
        // sink. Byte-identical arguments to `commit_palw_overlay_effects` (vp:3003-3021) and the mint
        // preflight (`palw_mint.rs:72-101`). Anchor-`None` ⇒ activation_open=false with zero samples —
        // mirroring the gate's own `.unwrap_or(false)` fail-closed polarity, never an error.
        let activation = match (&self.config.params.dns_params, enabled) {
            (Some(dns_params), true) => {
                let params = &self.config.params;
                let anchor =
                    resolve_palw_lagged_anchor(&self.storage.headers_store, &self.services.reachability_service, dns_params, sink);
                let (samples, anchor_hash) = match &anchor {
                    Some(anchor) => (
                        resolve_palw_buried_epoch_seeds(
                            &self.storage.headers_store,
                            &self.services.reachability_service,
                            anchor.anchor_hash,
                            params.palw_activation_daa_score,
                            params.palw_epoch_length_daa,
                            params.palw_beacon_grace_epochs.saturating_add(2),
                        ),
                        Some(anchor.anchor_hash),
                    ),
                    None => (Vec::new(), None),
                };
                // The sink's own persisted per-block beacon state (exact derived mode) — distinct
                // from the lagged buried signal; absence is reported as None, never invented.
                let beacon_state = self
                    .storage
                    .palw_beacon_store
                    .beacon_state(sink)
                    .map_err(|error| PalwStateProbeError::Store(format!("sink beacon state: {error:?}")))?;
                Some(PalwActivationProbe {
                    activation_open: palw_lagged_activation_open(&samples),
                    newest_sample: samples.last().copied(),
                    previous_sample: samples.len().checked_sub(2).and_then(|i| samples.get(i)).copied(),
                    buried_sample_count: samples.len() as u64,
                    buried_carry_run: palw_seed_carry_run(&samples),
                    anchor_hash,
                    current_epoch: sink_daa_score / params.palw_epoch_length_daa.max(1),
                    grace_epochs: params.palw_beacon_grace_epochs,
                    derived_mode: beacon_state.as_ref().map(|s| s.mode),
                    derived_degraded_epochs: beacon_state.as_ref().map(|s| s.degraded_epochs),
                })
            }
            _ => None,
        };

        let probe =
            PalwStateProbe {
                enabled,
                sink,
                sink_daa_score,
                overlay_view_available,
                batch,
                provider_bond,
                da_challenges,
                da_obligations,
                search_challenges,
                activation,
            };
        drop(virtual_read);
        Ok(probe)
    }
}
