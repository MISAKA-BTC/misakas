//! Remote monetary consumers accept a flat aux proof only after every bounded
//! page has authenticated. Intermediate pages never authorize signing or GC.
use kaspa_consensus_core::palw_material_release_recovery_v2::{
    PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2, PalwMaterialProviderAckCheckpointV2, PalwMaterialRecoveryRowV2,
    material_provider_stream_key_v2,
};
use kaspa_consensus_core::{
    Hash64,
    config::params::ForkActivation,
    palw_monetary_material_rpc_v1::*,
    palw_monetary_material_v1::{PALW_MONETARY_MATERIAL_TABLE_V1, PalwMonetaryMaterialRowV1},
};

/// Constructible only after a complete, pinned op242 aux commitment verifies.
/// This proof authenticates presence; finality still comes from the configured
/// native finality authority, never from a server's availability flag.
pub struct ProvenMonetarySnapshotV2 {
    pinned: Hash64,
    parent: Hash64,
    rows: Vec<PalwMonetaryMaterialRowV1>,
    recoveries: Vec<PalwMaterialRecoveryRowV2>,
    checkpoints: Vec<PalwMaterialProviderAckCheckpointV2>,
}
impl ProvenMonetarySnapshotV2 {
    /// The exact header hash against which every proof page was authenticated.
    pub fn pinned_block(&self) -> Hash64 { self.pinned }
    /// Parent header used by the proof. A finality consumer must independently
    /// require that this is its selected parent, not merely another DAG parent.
    pub fn parent_block(&self) -> Hash64 { self.parent }
    pub fn rows(&self) -> &[PalwMonetaryMaterialRowV1] {
        &self.rows
    }
    pub fn checkpoints(&self) -> &[PalwMaterialProviderAckCheckpointV2] {
        &self.checkpoints
    }
    pub fn recoveries(&self) -> &[PalwMaterialRecoveryRowV2] {
        &self.recoveries
    }
}

pub struct MonetaryMaterialProofPagerV1 {
    pinned: Hash64,
    parent: Hash64,
    aux_root: Hash64,
    state_preimage: Vec<u8>,
    kernel_route_offset: u64,
    total_rows: u64,
    row: u64,
    offset: u64,
    current: Option<(u8, Vec<u8>, u64)>,
    previous: Option<(u8, Vec<u8>)>,
    hash: blake2b_simd::State,
    monetary_bytes: Vec<u8>,
    retained: Vec<(u8, Vec<u8>, Vec<u8>)>,
    retained_bytes: usize,
    max_row_bytes: usize,
    max_total_bytes: usize,
    max_contracts: usize,
}
impl MonetaryMaterialProofPagerV1 {
    pub fn new(
        pinned: Hash64,
        fork_fence: Option<ForkActivation>,
        page: &PalwMonetaryAuxPageV1,
        max_row_bytes: usize,
        max_total_bytes: usize,
        max_contracts: usize,
    ) -> Result<Self, String> {
        use kaspa_consensus_core::palw_state_proof_v1::{PalwStateOpeningV1, state_root_of_pinned_header_v1};
        if max_row_bytes == 0
            || max_total_bytes < max_row_bytes
            || max_contracts == 0
            || max_contracts > 4096
            || page.start_row != 0
            || page.start_offset != 0
            || page.state_preimage.len() > (1 << 20)
            || page.total_rows > 1_000_000
        {
            return Err("monetary proof limits/initial cursor exceed the admitted client scope".into());
        }
        let committed = state_root_of_pinned_header_v1(&page.header, pinned).map_err(|e| e.to_string())?;
        let parent = kaspa_consensus_core::hashing::header::hash(&page.parent_header);
        if !page.header.parents_by_level.get(0).is_some_and(|parents| parents.contains(&parent)) {
            return Err("monetary proof parent header is not a parent of the pinned block".into());
        }
        let inner = PalwStateOpeningV1 { preimage: page.state_preimage.clone() }.root();
        match &page.fork_choice {
            Some(opening) => {
                if opening.inner_root != inner {
                    return Err("monetary state preimage substituted the fork-choice inner root".into());
                }
                opening
                    .verify(
                        &committed,
                        &kaspa_consensus_core::palw_fork_choice_commitment_v1::PalwForkChoicePointV1 {
                            block: parent,
                            daa_score: page.parent_header.daa_score,
                            blue_score: page.parent_header.blue_score,
                        },
                        fork_fence,
                    )
                    .map_err(|e| e.to_string())?;
            }
            None => {
                if inner != committed
                    || kaspa_consensus_core::palw_fork_choice_commitment_v1::palw_fork_choice_committed_at_v1(
                        fork_fence,
                        Some(page.parent_header.daa_score),
                    )
                {
                    return Err("monetary proof omits or substitutes its state-root envelope".into());
                }
            }
        }
        // The aux root is read from the ONE window the preimage's own grammar places the Some-only kernel-route block at
        // (`PalwChainStateV2::kernel_route_offset_of_preimage_v1`): a pure function of the bytes just authenticated against the
        // pinned header, so a serving node can neither choose the window nor plant another one (a label search could be steered:
        // with no kernel-route block, ANY later occurrence would have been read as the aux root). The node's carried offset is a
        // cross-check only — a version-1 peer carries 0 (none) and is still held to the derived offset.
        let label = b"kernel-route/v1";
        let at = kaspa_consensus_core::palw_state_v2::PalwChainStateV2::kernel_route_offset_of_preimage_v1(&page.state_preimage)?
            .ok_or("monetary proof's committed state has no kernel-route block")?;
        if page.kernel_route_offset != 0 && page.kernel_route_offset != at as u64 {
            return Err("monetary proof names a kernel-route offset its committed preimage does not place there".into());
        }
        if page.state_preimage[at + label.len() + 64..at + label.len() + 128] != *page.aux_root.as_byte_slice() {
            return Err("monetary auxiliary root differs from the committed kernel-route block".into());
        }
        let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kernel-route/aux-root/v1").to_state();
        hash.update(&page.total_rows.to_le_bytes());
        Ok(Self {
            pinned,
            parent,
            aux_root: page.aux_root,
            state_preimage: page.state_preimage.clone(),
            kernel_route_offset: page.kernel_route_offset,
            total_rows: page.total_rows,
            row: 0,
            offset: 0,
            current: None,
            previous: None,
            hash,
            monetary_bytes: Vec::new(),
            retained: Vec::new(),
            retained_bytes: 0,
            max_row_bytes,
            max_total_bytes,
            max_contracts,
        })
    }
    pub fn cursor(&self) -> (u64, u64) {
        (self.row, self.offset)
    }
    pub fn push(&mut self, page: &PalwMonetaryAuxPageV1) -> Result<(), String> {
        if kaspa_consensus_core::hashing::header::hash(&page.header) != self.pinned
            || kaspa_consensus_core::hashing::header::hash(&page.parent_header) != self.parent
            || page.aux_root != self.aux_root
            || page.state_preimage != self.state_preimage
            || page.kernel_route_offset != self.kernel_route_offset
            || page.total_rows != self.total_rows
            || (page.start_row, page.start_offset) != self.cursor()
            || page.fragments.len() > PALW_MONETARY_RPC_MAX_FRAGMENTS_V1
        {
            return Err("monetary proof page changed its pinned snapshot/cursor".into());
        }
        let mut page_bytes = 0usize;
        for fragment in &page.fragments {
            page_bytes =
                page_bytes.checked_add(64 + fragment.key.len() + fragment.bytes.len()).ok_or("monetary page byte count overflow")?;
            if page_bytes > PALW_MONETARY_RPC_MAX_PAGE_BYTES_V1
                || fragment.key.len() > PALW_MONETARY_RPC_MAX_KEY_BYTES_V1
                || fragment.row_bytes > u32::MAX as u64
                || fragment.row != self.row
                || fragment.offset != self.offset
                || self.row >= self.total_rows
            {
                return Err("monetary proof fragment is outside its canonical page/cursor".into());
            }
            if self.offset == 0 {
                let key = (fragment.table, fragment.key.clone());
                if self.previous.as_ref().is_some_and(|previous| previous >= &key) {
                    return Err("monetary proof repeats or reorders an auxiliary row".into());
                }
                self.hash.update(&[fragment.table]);
                self.hash.update(&(fragment.key.len() as u32).to_le_bytes());
                self.hash.update(&fragment.key);
                self.hash.update(&(fragment.row_bytes as u32).to_le_bytes());
                self.current = Some((fragment.table, fragment.key.clone(), fragment.row_bytes));
            }
            if self.current.as_ref() != Some(&(fragment.table, fragment.key.clone(), fragment.row_bytes))
                || self.offset.checked_add(fragment.bytes.len() as u64).is_none_or(|end| end > fragment.row_bytes)
                || (fragment.bytes.is_empty() && fragment.row_bytes != 0)
            {
                return Err("monetary proof changes a row mid-fragment or makes no progress".into());
            }
            self.hash.update(&fragment.bytes);
            // Challenge application checkpoints are committed by these pages,
            // but are not provider recovery/ACK authority. Stream their bytes
            // into the full aux hash without retaining a second population.
            let challenge_checkpoint = fragment.table == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2
                && fragment.key.len() == 69
                && fragment.key[0]
                    == kaspa_consensus_core::palw_material_challenge_checkpoint_v4::PALW_MATERIAL_CHALLENGE_CHECKPOINT_PREFIX_V4;
            if challenge_checkpoint
                && fragment.row_bytes
                    != kaspa_consensus_core::palw_material_challenge_checkpoint_v4::PALW_MATERIAL_CHALLENGE_CHECKPOINT_BYTES_V4 as u64
            {
                return Err("challenge application checkpoint has an invalid fixed extent".into());
            }
            let challenge_history = fragment.table == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2
                && fragment.key.len() == 69
                && fragment.key[0]
                    == kaspa_consensus_core::palw_material_challenge_history_v4::PALW_MATERIAL_CHALLENGE_HISTORY_PREFIX_V4;
            if challenge_history
                && fragment.row_bytes
                    != kaspa_consensus_core::palw_material_challenge_history_v4::PALW_MATERIAL_CHALLENGE_HISTORY_BYTES_V4 as u64
            {
                return Err("challenge application history has an invalid fixed extent".into());
            }
            let retain = !challenge_checkpoint
                && !challenge_history
                && matches!(fragment.table, PALW_MONETARY_MATERIAL_TABLE_V1 | PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2);
            if retain {
                if (fragment.key.len() != 64
                    && !(fragment.table == PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2
                        && fragment.key.len() == 69
                        && fragment.key[0] == 0xA1))
                    || fragment.row_bytes > self.max_row_bytes as u64
                    || self.retained.len() >= self.max_contracts.saturating_mul(2).saturating_add(32768)
                    || self.retained_bytes.checked_add(fragment.bytes.len()).is_none_or(|n| n > self.max_total_bytes)
                {
                    return Err("monetary contract proof exceeds the client's declared retained capacity".into());
                }
                self.retained_bytes += fragment.bytes.len();
                self.monetary_bytes.extend_from_slice(&fragment.bytes);
            }
            self.offset += fragment.bytes.len() as u64;
            if self.offset == fragment.row_bytes {
                if retain {
                    self.retained.push((fragment.table, fragment.key.clone(), std::mem::take(&mut self.monetary_bytes)));
                }
                self.previous = Some((fragment.table, fragment.key.clone()));
                self.current = None;
                self.row += 1;
                self.offset = 0;
            }
        }
        if (page.next_row, page.next_offset) != self.cursor() || (page.fragments.is_empty() && self.row < self.total_rows) {
            return Err("monetary proof falsely advances or stalls its cursor".into());
        }
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<PalwMonetaryMaterialRowV1>, String> {
        Ok(self.finish_with_recovery_v2()?.rows)
    }
    pub fn finish_with_recovery_v2(self) -> Result<ProvenMonetarySnapshotV2, String> {
        if self.row != self.total_rows || self.offset != 0 || self.current.is_some() {
            return Err("monetary auxiliary proof is incomplete; no contract/absence is authenticated".into());
        }
        if self.hash.finalize().as_bytes() != self.aux_root.as_byte_slice() {
            return Err("monetary auxiliary pages disagree with the pinned native state root".into());
        }
        use kaspa_consensus_core::palw_native_resource_v1::{NativeWireReaderV1, NativeWireScanV1};
        let mut requested = self
            .retained
            .capacity()
            .checked_mul(std::mem::size_of::<(u8, Vec<u8>, Vec<u8>)>())
            .ok_or("monetary proof heap arithmetic overflow")? as u64;
        requested = requested.checked_add(self.state_preimage.capacity() as u64).ok_or("monetary proof heap arithmetic overflow")?;
        for (table, key, bytes) in &self.retained {
            let mut scanner = NativeWireReaderV1::new(bytes.as_slice(), bytes.len() as u64);
            let shape = if *table == PALW_MONETARY_MATERIAL_TABLE_V1 {
                PalwMonetaryMaterialRowV1::scan_native_wire_v1(&mut scanner)
            } else if key.len() == 64 {
                PalwMaterialRecoveryRowV2::scan_native_wire_v1(&mut scanner)
            } else {
                PalwMaterialProviderAckCheckpointV2::scan_native_wire_v1(&mut scanner)
            }
            .map_err(|e| format!("monetary typed preflight refused: {e}"))?;
            if scanner.remaining() != 0 {
                return Err("monetary row has trailing bytes".into());
            }
            requested = requested
                .checked_add(shape.heap)
                .and_then(|n| {
                    n.checked_add(
                        (key.capacity()
                            + bytes.capacity()
                            + std::mem::size_of::<PalwMonetaryMaterialRowV1>().max(std::mem::size_of::<PalwMaterialRecoveryRowV2>()))
                            as u64,
                    )
                })
                .ok_or("monetary typed heap arithmetic overflow")?;
        }
        if requested > self.max_total_bytes as u64 {
            return Err("raw proof plus typed monetary heap exceeds the client's declared logical capacity".into());
        }
        let mut rows = Vec::new();
        let mut recoveries = Vec::new();
        let mut checkpoints = Vec::new();
        for (table, key, bytes) in self.retained {
            if table == PALW_MONETARY_MATERIAL_TABLE_V1 {
                let row: PalwMonetaryMaterialRowV1 =
                    borsh::from_slice(&bytes).map_err(|e| format!("authenticated monetary row is undecodable: {e}"))?;
                if row.terms.root.as_byte_slice() != key.as_slice() {
                    return Err("authenticated monetary root differs".into());
                }
                rows.push(row);
            } else if key.len() != 64 {
                let row: PalwMaterialProviderAckCheckpointV2 =
                    borsh::from_slice(&bytes).map_err(|e| format!("authenticated provider checkpoint is undecodable: {e}"))?;
                if key != material_provider_stream_key_v2(&row.provider) {
                    return Err("authenticated provider stream key differs".into());
                }
                checkpoints.push(row);
            } else {
                let row: PalwMaterialRecoveryRowV2 =
                    borsh::from_slice(&bytes).map_err(|e| format!("authenticated recovery row is undecodable: {e}"))?;
                if row.contract().terms.root.as_byte_slice() != key.as_slice() {
                    return Err("authenticated recovery root differs".into());
                }
                recoveries.push(row);
            }
        }
        Ok(ProvenMonetarySnapshotV2 { pinned: self.pinned, parent: self.parent, rows, recoveries, checkpoints })
    }
}

#[cfg(feature = "rpc")]
pub async fn fetch_monetary_material_contracts_v1(
    client: &kaspa_wrpc_client::KaspaRpcClient,
    pinned: Hash64,
    fork_fence: Option<ForkActivation>,
    max_row_bytes: usize,
    max_total_bytes: usize,
    max_contracts: usize,
) -> Result<Vec<PalwMonetaryMaterialRowV1>, String> {
    Ok(fetch_monetary_material_snapshot_v2(client, pinned, fork_fence, max_row_bytes, max_total_bytes, max_contracts).await?.rows)
}
#[cfg(feature = "rpc")]
pub async fn fetch_monetary_material_snapshot_v2(
    client: &kaspa_wrpc_client::KaspaRpcClient,
    pinned: Hash64,
    fork_fence: Option<ForkActivation>,
    max_row_bytes: usize,
    max_total_bytes: usize,
    max_contracts: usize,
) -> Result<ProvenMonetarySnapshotV2, String> {
    use kaspa_rpc_core::{GetPalwMonetaryMaterialRequest, api::rpc::RpcApi};
    let mut pager: Option<MonetaryMaterialProofPagerV1> = None;
    loop {
        let (row, offset) = pager.as_ref().map(|p| p.cursor()).unwrap_or((0, 0));
        let response = client
            .get_palw_monetary_material(GetPalwMonetaryMaterialRequest {
                block_hash: pinned.to_string(),
                row,
                offset,
                limit_bytes: PALW_MONETARY_RPC_MAX_PAGE_BYTES_V1 as u32,
            })
            .await
            .map_err(|e| e.to_string())?;
        if !response.available {
            return Err(response.reason);
        }
        let header = kaspa_consensus_core::header::Header::try_from(response.header.as_ref().ok_or("monetary RPC omitted header")?)
            .map_err(|e| e.to_string())?;
        let parent_header = kaspa_consensus_core::header::Header::try_from(
            response.parent_header.as_ref().ok_or("monetary RPC omitted parent header")?,
        )
        .map_err(|e| e.to_string())?;
        let page = PalwMonetaryAuxPageV1 {
            header,
            parent_header,
            state_preimage: response.state_preimage,
            fork_choice: if response.fork_choice_opening.is_empty() {
                None
            } else {
                Some(borsh::from_slice(&response.fork_choice_opening).map_err(|e| e.to_string())?)
            },
            kernel_route_offset: response.kernel_route_offset,
            aux_root: response.aux_root.parse::<Hash64>().map_err(|e| e.to_string())?,
            total_rows: response.total_rows,
            start_row: response.start_row,
            start_offset: response.start_offset,
            next_row: response.next_row,
            next_offset: response.next_offset,
            fragments: response
                .fragments
                .into_iter()
                .map(|f| PalwMonetaryAuxFragmentV1 {
                    row: f.row,
                    table: f.table,
                    key: f.key,
                    row_bytes: f.row_bytes,
                    offset: f.offset,
                    bytes: f.bytes,
                })
                .collect(),
        };
        if pager.is_none() {
            pager = Some(MonetaryMaterialProofPagerV1::new(pinned, fork_fence, &page, max_row_bytes, max_total_bytes, max_contracts)?);
        }
        let state = pager.as_mut().ok_or("monetary pager state missing")?;
        state.push(&page)?;
        if state.cursor() == (page.total_rows, 0) {
            return pager.ok_or("monetary pager disappeared")?.finish_with_recovery_v2();
        }
    }
}

#[cfg(test)]
#[path = "monetary_material_rpc_provenance_tests.rs"]
pub(crate) mod provenance_tests;
