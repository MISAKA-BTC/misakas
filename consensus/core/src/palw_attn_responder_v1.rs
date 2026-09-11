//! **ADR-0093 as built — both parties of a fused-attention dissection, over what one capture yields.**
//!
//! ADR-0093 Decision 1 made the backend's obligation one history tile's triple. Built, it is
//! smaller still, and that is the point of this module: a family reads the site's INPUTS and
//! OPENINGS out of the capture it committed ([`PalwAttnSiteEvidenceV1`], one backend verb), and
//! every number a move carries is computed HERE by the court's own kernels —
//! `a16_attn_root_claim_v1` for the root, `a16_attn_tile_triple_v1` folded by `palw_attn_fold_v1`
//! for a range — the same calls `check_attn_dissect_bottom_v1` makes at the bottom. Decision 4's
//! hazard, a family whose instrumented kernel answers "approximately" and convicts itself while
//! holding the claim's collateral, has nowhere to happen: no family computes a claim.
//!
//! # Who calls what
//!
//! * The RESPONDER (the claim's executor) holds evidence from its own capture and files, in turn,
//!   the root claim ([`PalwAttnSiteEvidenceV1::root_claim_v1`], with the out tile, the binding and
//!   the operand openings the evidence carries) and one round per `AwaitDisclosure`
//!   ([`PalwAttnSiteEvidenceV1::round_v1`]) — the children of the disputed range against the
//!   ROOT's `(m*, S*)`, which for an honest responder is its own.
//! * The CHALLENGER holds evidence from its OWN execution and names the first child whose
//!   disclosed claim its recompute does not reproduce ([`PalwAttnSiteEvidenceV1::divergent_child_v1`]).
//!   At the fused leaf the ladder narrowed to, every input — the query, every K and V row — is in
//!   the agreed prefix, so the challenger's inputs ARE the accused execution's committed ones and
//!   its recompute is exactly what the bottom will compute.
//! * At `Terminal` either party files the bottom ([`PalwAttnSiteEvidenceV1::bottom_v1`]) built from
//!   the ACCUSED capture's evidence — the rows must be opened against the claim's own commitments.
//!
//! Pure functions of the evidence, the derived site and the chain's phase: nothing here reads a
//! clock, a key or a fence.

use crate::Hash64;
use crate::palw_artifact::PalwArtifactOpeningV1;
use crate::palw_attn_court_v1::{
    PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnCheckpointAnchorV1, PalwAttnChunkOpeningV1, PalwAttnDissectBottomV1,
    PalwAttnDissectPhaseV1, PalwAttnRowOpeningV1, PalwAttnTileEvidenceV1,
};
use crate::palw_attn_dissect::{
    PALW_ATTN_DISSECT_MAX_CHILDREN, PALW_ATTN_DISSECT_OBJECT_VERSION_V1, PalwAttnDissectError, PalwAttnDissectRoundV1,
    PalwAttnRangeClaimV1, PalwAttnRootClaimV1, palw_attn_fold_v1,
};
use crate::palw_base0_a16::{PalwA16OpError, a16_attn_root_claim_v1, a16_attn_tile_triple_v1};
use crate::palw_court_v2::PalwAttnDisputeSiteV2;
use crate::palw_state_chunk_map::{PalwStateChunkKindV1, integer_kv_state_locate_v1};

/// Why a party could not compute a move. Every arm is a refusal to MOVE — the party stays silent
/// and the clock decides — never a statement about anybody's execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwAttnResponderError {
    /// The evidence's inputs do not have the site's shape: `what` names the series.
    InputsAreNotTheSites {
        what: &'static str,
        got: usize,
        want: usize,
    },
    /// The site could not be derived from the evidence's binding and openings.
    Site(String),
    Kernel(PalwA16OpError),
    Dissect(PalwAttnDissectError),
    /// A choice was asked for while the phase holds no disclosed children.
    NoPendingChildren,
    /// The bottom was asked for before the dissection narrowed to one tile.
    NotNarrowed,
    /// The bottom's route needs evidence this capture did not yield — named.
    EvidenceMissing(&'static str),
}

impl core::fmt::Display for PalwAttnResponderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InputsAreNotTheSites { what, got, want } => write!(f, "the {what} is {got} codes and the site reads {want}"),
            Self::Site(why) => write!(f, "the fused site does not derive from this evidence: {why}"),
            Self::Kernel(e) => write!(f, "the court's kernel refused the inputs: {e:?}"),
            Self::Dissect(e) => write!(f, "{e}"),
            Self::NoPendingChildren => write!(f, "the phase holds no disclosed children to choose among"),
            Self::NotNarrowed => write!(f, "the dissection has not narrowed to one tile"),
            Self::EvidenceMissing(what) => write!(f, "this capture yielded no {what}"),
        }
    }
}

impl std::error::Error for PalwAttnResponderError {}

impl From<PalwA16OpError> for PalwAttnResponderError {
    fn from(e: PalwA16OpError) -> Self {
        Self::Kernel(e)
    }
}

impl From<PalwAttnDissectError> for PalwAttnResponderError {
    fn from(e: PalwAttnDissectError) -> Self {
        Self::Dissect(e)
    }
}

/// **The fused site's inputs, as the capture committed them**: the disputed head's rotated query
/// slice at the site's position, and the site's layer's K and V cache rows over the whole history
/// `0..history_positions` — position-major, `kv_dim` codes a row, the cache's own layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnSiteInputsV1 {
    pub qh: Vec<i32>,
    pub k_series: Vec<i32>,
    pub v_series: Vec<i32>,
}

impl PalwAttnSiteInputsV1 {
    /// The inputs have the site's shape, or no claim is computed from them.
    pub fn check_v1(&self, site: &PalwAttnDisputeSiteV2) -> Result<(), PalwAttnResponderError> {
        let want_series = site.history_positions as usize * site.site.kv_dim;
        if self.qh.len() != site.site.d_head {
            return Err(PalwAttnResponderError::InputsAreNotTheSites {
                what: "query slice",
                got: self.qh.len(),
                want: site.site.d_head,
            });
        }
        if self.k_series.len() != want_series {
            return Err(PalwAttnResponderError::InputsAreNotTheSites {
                what: "K series",
                got: self.k_series.len(),
                want: want_series,
            });
        }
        if self.v_series.len() != want_series {
            return Err(PalwAttnResponderError::InputsAreNotTheSites {
                what: "V series",
                got: self.v_series.len(),
                want: want_series,
            });
        }
        Ok(())
    }

    fn lanes(site: &PalwAttnDisputeSiteV2) -> (usize, usize) {
        (site.head_lanes.1 as usize, site.head_lanes.2 as usize)
    }

    /// **The honest root claim over the whole history** — `a16_attn_root_claim_v1`, the court's
    /// own composition, at the court's tile.
    pub fn root_claim_v1(&self, site: &PalwAttnDisputeSiteV2) -> Result<PalwAttnRootClaimV1, PalwAttnResponderError> {
        self.check_v1(site)?;
        let claim = a16_attn_root_claim_v1(
            &self.qh,
            &self.k_series,
            &self.v_series,
            site.site.kv_dim,
            site.site.kv_off,
            Self::lanes(site),
            site.site.params,
            site.tile_positions as usize,
        )?;
        Ok(PalwAttnRootClaimV1 {
            version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1,
            head: site.head_lanes.0,
            lane_first: site.head_lanes.1,
            lane_count: site.head_lanes.2,
            history_positions: site.history_positions,
            claim,
        })
    }

    /// **One range's claim against a given `(m*, S*)`**: the tiles `[first_tile, first_tile +
    /// tile_count)` recomputed with `a16_attn_tile_triple_v1` — the last one ragged at the end of
    /// the history, as the phase's own `terminal_tile_positions` cuts it — and folded exactly as
    /// the rounds fold them. The fold is a max and two sums, so any grouping gives the same claim.
    pub fn range_claim_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        first_tile: u64,
        tile_count: u64,
        root_scale: (i32, i64),
    ) -> Result<PalwAttnRangeClaimV1, PalwAttnResponderError> {
        self.check_v1(site)?;
        let tile = site.tile_positions as u64;
        let history = site.history_positions as u64;
        let kv_dim = site.site.kv_dim;
        let mut level = Vec::with_capacity(tile_count as usize);
        for t in first_tile..first_tile.saturating_add(tile_count) {
            let lo = t.saturating_mul(tile);
            let hi = lo.saturating_add(tile).min(history);
            if lo >= hi {
                return Err(PalwAttnResponderError::InputsAreNotTheSites {
                    what: "tile range",
                    got: t as usize,
                    want: history.div_ceil(tile) as usize,
                });
            }
            let (lo, hi) = (lo as usize * kv_dim, hi as usize * kv_dim);
            level.push(a16_attn_tile_triple_v1(
                &self.qh,
                &self.k_series[lo..hi],
                &self.v_series[lo..hi],
                kv_dim,
                site.site.kv_off,
                Self::lanes(site),
                site.site.params,
                root_scale.0,
                root_scale.1,
            )?);
        }
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(PALW_ATTN_DISSECT_MAX_CHILDREN));
            for group in level.chunks(PALW_ATTN_DISSECT_MAX_CHILDREN) {
                next.push(palw_attn_fold_v1(group)?);
            }
            level = next;
        }
        level.pop().ok_or(PalwAttnResponderError::Dissect(PalwAttnDissectError::NoChildren))
    }

    /// **The children of the phase's disputed range**, each against the ROOT's `(m*, S*)`, in the
    /// pinned order — the round an honest responder discloses and the round a challenger
    /// recomputes.
    pub fn round_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<PalwAttnDissectRoundV1, PalwAttnResponderError> {
        let children = phase
            .child_ranges()
            .into_iter()
            .map(|(first, count)| self.range_claim_v1(site, first, count, phase.root_scale()))
            .collect::<Result<Vec<_>, _>>()?;
        if children.is_empty() {
            return Err(PalwAttnResponderError::NotNarrowed);
        }
        Ok(PalwAttnDissectRoundV1 { version: PALW_ATTN_DISSECT_OBJECT_VERSION_V1, children })
    }

    /// **The challenger's move: the first disclosed child this recompute does not reproduce.**
    /// `None` when every child reproduces — an honest disclosure, against which an honest
    /// challenger has no move that wins, and so makes none.
    pub fn divergent_child_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<Option<u8>, PalwAttnResponderError> {
        if phase.pending().is_empty() {
            return Err(PalwAttnResponderError::NoPendingChildren);
        }
        let honest = self.round_v1(site, phase)?;
        Ok(phase.pending().iter().zip(&honest.children).position(|(disclosed, recomputed)| disclosed != recomputed).map(|i| i as u8))
    }
}

/// **What the accused's root claim put on the chain about its own commitments** (ADR-0093
/// Decisions 7 and 8): the binding it named, the committed output tile at the narrowed leaf opened
/// against it, and — when it filed the anchored form — the checkpoint the disputed step anchors
/// at, opened against its checkpoint leg. With these and its OWN execution, a challenger opens every
/// row a bottom needs against the accused's roots without one byte the accused must serve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnAccusedFilingV1 {
    pub binding: crate::palw_step_leg::PalwStepBindingV2,
    pub out_tile: PalwAttnRowOpeningV1,
    pub anchor: Option<crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1>,
}

/// **The checkpoint a checkpoint-route bottom reads from**: the committed leaf and its opening, and
/// every chunk of the state it commits — in map order, with their leaf hashes — so the bottom for
/// ANY tile is a lookup and a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnAnchorEvidenceV1 {
    pub anchor: PalwAttnCheckpointAnchorV1,
    pub chunks: Vec<Vec<u8>>,
    pub chunk_hashes: Vec<Hash64>,
}

/// **Everything one capture yields about one fused site** — built once by a family's backend
/// (`PalwExecutionBackendV1::attn_site_evidence`) and read by every move of the session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnSiteEvidenceV1 {
    /// The leaf the ladder narrowed to — the fused node's output tile.
    pub narrowed: u64,
    /// The capture's own binding: the accused execution's, when the capture is the accused's.
    pub binding: crate::palw_step_leg::PalwStepBindingV2,
    /// The committed output tile, opened.
    pub out_tile: PalwAttnRowOpeningV1,
    /// The rotated-query row the disputed head reads, opened.
    pub query: PalwAttnRowOpeningV1,
    /// The site's registered narrowings, opened against the class's artifact root.
    pub operand_openings: Vec<PalwArtifactOpeningV1>,
    pub inputs: PalwAttnSiteInputsV1,
    /// The checkpoint the disputed step's evidence must anchor at, when the class commits one.
    pub anchor: Option<PalwAttnAnchorEvidenceV1>,
    /// The K and V cache-write rows of every history position, opened — the route a class that
    /// does not checkpoint every position serves its bottom from. `None` when the class's rows are
    /// not one leaf each (the bottom refuses that route by name) or its every position is checkpointed.
    pub cache_rows: Option<(Vec<PalwAttnRowOpeningV1>, Vec<PalwAttnRowOpeningV1>)>,
}

impl PalwAttnSiteEvidenceV1 {
    /// **The site, as the court derives it from this evidence** — the binding and the openings,
    /// proven against `artifact_root`; with the anchor for a checkpoint-route bottom, without it for
    /// the claims (which the anchor does not touch). Unpinned: a party computing its own moves.
    pub fn site_v1(&self, artifact_root: Hash64, with_anchor: bool) -> Result<PalwAttnDisputeSiteV2, PalwAttnResponderError> {
        let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&self.operand_openings, artifact_root)
            .map_err(|e| PalwAttnResponderError::Site(e.to_string()))?;
        let anchor = if with_anchor { self.anchor.as_ref().map(|a| &a.anchor) } else { None };
        crate::palw_court_v2::palw_attn_dispute_site_unpinned_v2(&self.binding, &operands, self.narrowed, anchor)
            .map_err(|e| PalwAttnResponderError::Site(e.to_string()))
    }

    pub fn root_claim_v1(&self, site: &PalwAttnDisputeSiteV2) -> Result<PalwAttnRootClaimV1, PalwAttnResponderError> {
        self.inputs.root_claim_v1(site)
    }

    pub fn round_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<PalwAttnDissectRoundV1, PalwAttnResponderError> {
        self.inputs.round_v1(site, phase)
    }

    pub fn divergent_child_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<Option<u8>, PalwAttnResponderError> {
        self.inputs.divergent_child_v1(site, phase)
    }

    /// **The bottom of a narrowed dissection, from THIS capture's commitments.** `site` must be the
    /// one derived WITH the anchor (`site_v1(root, true)`) whenever the class checkpoints, because
    /// the anchor's layout is what locates the tile's chunk. The route is the class's: one chunk per
    /// kind out of the anchor plus the rows past its edge where every position is checkpointed; the
    /// cache-write rows otherwise.
    pub fn bottom_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<PalwAttnDissectBottomV1, PalwAttnResponderError> {
        let tile = phase.terminal_tile().ok_or(PalwAttnResponderError::NotNarrowed)?;
        let (first, width) = phase.terminal_tile_positions().ok_or(PalwAttnResponderError::NotNarrowed)?;
        let rows = |series: &[PalwAttnRowOpeningV1], lo: u64, hi: u64| -> Result<Vec<PalwAttnRowOpeningV1>, PalwAttnResponderError> {
            series.get(lo as usize..hi as usize).map(<[_]>::to_vec).ok_or(PalwAttnResponderError::EvidenceMissing("cache-write row"))
        };
        let (k, v, anchor) = if site.site.every_position_is_checkpointed {
            let evidence = self.anchor.as_ref().ok_or(PalwAttnResponderError::EvidenceMissing("checkpoint anchor"))?;
            let geometry = site.site.anchor_geometry.as_ref().ok_or(PalwAttnResponderError::EvidenceMissing("anchor layout"))?;
            let covered = u64::from(site.site.anchor_positions).saturating_sub(first).min(width as u64);
            let chunk_for = |kind: PalwStateChunkKindV1| -> Result<PalwAttnChunkOpeningV1, PalwAttnResponderError> {
                let (chunk_index, _) = integer_kv_state_locate_v1(geometry, kind, site.site.attn_layer, first as u32)
                    .ok_or(PalwAttnResponderError::EvidenceMissing("the tile's chunk in the anchor's layout"))?;
                let chunk_bytes = evidence
                    .chunks
                    .get(chunk_index as usize)
                    .cloned()
                    .ok_or(PalwAttnResponderError::EvidenceMissing("anchor chunk"))?;
                let siblings = crate::palw_step_leg::state_chunk_path_v1(&evidence.chunk_hashes, chunk_index as usize)
                    .map_err(|_| PalwAttnResponderError::EvidenceMissing("anchor chunk path"))?;
                Ok(PalwAttnChunkOpeningV1 { chunk_index: chunk_index as u32, chunk_bytes, siblings })
            };
            let after = |series: Option<&Vec<PalwAttnRowOpeningV1>>| -> Result<Vec<PalwAttnRowOpeningV1>, PalwAttnResponderError> {
                if covered >= width as u64 {
                    return Ok(Vec::new());
                }
                rows(
                    series.ok_or(PalwAttnResponderError::EvidenceMissing("cache-write rows past the anchor"))?,
                    first + covered,
                    first + width as u64,
                )
            };
            let (k_rows, v_rows) = match &self.cache_rows {
                Some((k, v)) => (Some(k), Some(v)),
                None => (None, None),
            };
            (
                PalwAttnTileEvidenceV1::Checkpoint { chunk: chunk_for(PalwStateChunkKindV1::Key)?, rows_after: after(k_rows)? },
                PalwAttnTileEvidenceV1::Checkpoint { chunk: chunk_for(PalwStateChunkKindV1::Value)?, rows_after: after(v_rows)? },
                Some(evidence.anchor.clone()),
            )
        } else {
            let (k_rows, v_rows) = self.cache_rows.as_ref().ok_or(PalwAttnResponderError::EvidenceMissing("cache-write rows"))?;
            (
                PalwAttnTileEvidenceV1::CacheWrites { rows: rows(k_rows, first, first + width as u64)? },
                PalwAttnTileEvidenceV1::CacheWrites { rows: rows(v_rows, first, first + width as u64)? },
                None,
            )
        };
        Ok(PalwAttnDissectBottomV1 {
            version: PALW_ATTN_COURT_OBJECT_VERSION_V1,
            session_id: phase.session_id(),
            tile,
            query: self.query.clone(),
            anchor,
            k,
            v,
            out_tile: self.out_tile.clone(),
        })
    }
}
