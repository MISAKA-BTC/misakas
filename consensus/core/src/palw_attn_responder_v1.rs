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
                // The path under the class's map (ADR-0103 Decision 3: the held map's two-level
                // proof), folded from the leaves the evidence already holds.
                let siblings = crate::palw_state_chunk_map::palw_state_chunk_path_from_leaves_for_map_v1(
                    &self.binding.shape_profile,
                    site.site.anchor_positions,
                    &evidence.chunk_hashes,
                    chunk_index as u32,
                )
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

// =================================================================================================
// ADR-0152 §4-ter (A-held) — the held site's evidence and the anchor's slice sub-roots
// =================================================================================================

/// **What the accused's held root claim put on the chain** (`CourtAttnRootClaimedHeld`, tag 57):
/// the binding, the committed output tile at the narrowed leaf, the anchor checkpoint the site's
/// bottom reads, and every slice sub-root of the anchor's state — the fold refused the object
/// unless those root to the anchor, so each is the accused's own. What a challenger opens a bottom
/// against when the accused's execution followed its lie (4-ter F9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnHeldFilingV1 {
    pub binding: crate::palw_step_leg::PalwStepBindingV2,
    pub out_tile: PalwAttnRowOpeningV1,
    pub anchor: PalwAttnCheckpointAnchorV1,
    pub slice_sub_roots: Vec<Hash64>,
}

impl PalwAttnHeldFilingV1 {
    /// The filing a held root claim carries, and the session it was filed in; `None` for any other
    /// object.
    pub fn from_object_v1(object: &crate::palw_state_v2::PalwConsensusObjectV2) -> Option<(Hash64, Self)> {
        match object {
            crate::palw_state_v2::PalwConsensusObjectV2::CourtAttnRootClaimedHeld {
                session_id,
                binding,
                out_tile,
                anchor,
                slice_sub_roots,
                ..
            } => Some((
                *session_id,
                Self {
                    binding: (**binding).clone(),
                    out_tile: out_tile.clone(),
                    anchor: (**anchor).clone(),
                    slice_sub_roots: slice_sub_roots.clone(),
                },
            )),
            _ => None,
        }
    }

    /// **The held filing `object` carries for `session_id`, checked the way the fold checked it —
    /// for a PARTY reading root claims back off the chain** (the feat/t12-aheld-node review, HIGH).
    ///
    /// A walk of the accepted lifecycle objects returns every object an accepted carrier held,
    /// including the ones the fold REFUSED: a liar can put a held root claim with one sub-root
    /// swapped on the chain before its real one — the fold drops it (`HeldSubRootsDoNotRoot`), its
    /// carrier stays accepted — and a challenger that took the first object naming the session
    /// would build its bottom against a top path no anchor has. So the reader applies the fold's own
    /// checks of the held arm, in its order, with nothing but chain facts the party already holds —
    /// the claim's execution root and class, the class's artifact root, the narrowed leaf and the
    /// court's opening cap — and keeps only a filing that passes all of them:
    ///
    /// 1. the object is a held root claim for `session_id`;
    /// 2. its binding is the claim's (the registered class's profile, the claim's execution root),
    ///    registers the held map, and counts its leaves canonically (C-01);
    /// 3. its output tile is the narrowed leaf;
    /// 4. its operand openings prove against `artifact_root`, and the site derives from them — which
    ///    re-derives the execution root from the binding's parts (`verify_binding_v1`);
    /// 5. the anchor is the site's own checkpoint, opened against the claim's checkpoint leg
    ///    (`palw_attn_anchor_is_the_sites_v1`);
    /// 6. the sub-roots are the anchor's: their count is the held layout's slice count, and they fold
    ///    through the top tree to the anchor's committed state root (H2/H3,
    ///    `palw_state_top_root_from_sub_roots_v4`);
    /// 7. the output tile opens against the binding's step root.
    ///
    /// `Err` names the first check that refused. Any filing that passes is the accused's own
    /// commitments (each field is pinned to a root the claim committed), so which passing object a
    /// party reads does not matter; the fold remains the only judge of which one opened the phase.
    #[allow(clippy::too_many_arguments)]
    pub fn from_object_checked_v1(
        object: &crate::palw_state_v2::PalwConsensusObjectV2,
        session_id: &Hash64,
        claim_execution_root: Hash64,
        class_id: Hash64,
        artifact_root: Hash64,
        narrowed: u64,
        opening_cap: u64,
    ) -> Result<Self, String> {
        let crate::palw_state_v2::PalwConsensusObjectV2::CourtAttnRootClaimedHeld {
            session_id: filed,
            binding,
            out_tile,
            anchor,
            slice_sub_roots,
            operand_openings,
            ..
        } = object
        else {
            return Err("not a held root claim".to_string());
        };
        if filed != session_id {
            return Err("a held root claim for another session".to_string());
        }
        if binding.shape_profile.shape_profile_id() != class_id {
            return Err("the binding's profile is not the claim's class".to_string());
        }
        if binding.committed_execution_root != claim_execution_root {
            return Err("the binding is not the claim's execution".to_string());
        }
        if !crate::palw_state_chunk_map::palw_map_is_held_v4(&binding.shape_profile.state_chunk_map_id) {
            return Err("the binding does not register the held map".to_string());
        }
        match crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, binding.step_leaf_count) {
            Ok(count) if count == binding.step_leaf_count => {}
            _ => return Err("the binding's step_leaf_count is not canonical (C-01)".to_string()),
        }
        if out_tile.opening.leaf_index != narrowed {
            return Err("the output tile is not the narrowed leaf".to_string());
        }
        let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(operand_openings, artifact_root)
            .map_err(|e| format!("the operand openings: {e}"))?;
        let site = crate::palw_court_v2::palw_attn_dispute_site_unpinned_v3(binding, &operands, narrowed, Some(anchor), opening_cap)
            .map_err(|e| format!("the site: {e}"))?;
        crate::palw_attn_court_v1::palw_attn_anchor_is_the_sites_v1(anchor, &site.binding, &site.site)
            .map_err(|e| format!("the anchor is not the site's: {e}"))?;
        let layout = crate::palw_state_chunk_map::palw_state_layout_v4(&binding.shape_profile, site.site.anchor_positions)
            .map_err(|e| format!("the anchor's held layout: {e:?}"))?;
        if slice_sub_roots.len() != layout.slice_count() as usize {
            return Err(format!("{} sub-roots for {} slices (H2)", slice_sub_roots.len(), layout.slice_count()));
        }
        let rooted = crate::palw_state_chunk_map::palw_state_top_root_from_sub_roots_v4(&layout, slice_sub_roots)
            .map_err(|e| format!("the sub-roots' top tree: {e:?}"))?;
        if rooted != anchor.leaf.state_chunks_root {
            return Err("the filed sub-roots do not root to the anchor's committed state (H3, HeldSubRootsDoNotRoot)".to_string());
        }
        crate::palw_attn_court_v1::palw_attn_opened_lanes_v1(out_tile, &site.binding, site.head_lanes.2 as usize)
            .map_err(|e| format!("the output tile does not open: {e}"))?;
        Ok(Self {
            binding: (**binding).clone(),
            out_tile: out_tile.clone(),
            anchor: (**anchor).clone(),
            slice_sub_roots: slice_sub_roots.clone(),
        })
    }
}

/// **A held site's evidence, built by a windowed builder** (ADR-0152 §4-ter N1/N2): the court
/// kernels' evidence ([`PalwAttnSiteEvidenceV1`] — the site's inputs, the opened out tile and query
/// row, the operand openings, and the anchor with every chunk of the state THIS party holds at it),
/// beside the anchor's slice sub-roots the bottom's top path is built from.
///
/// For the RESPONDER both are its own: its committed state, and the sub-roots its held root claim
/// files. For a CHALLENGER the chunks are its own honest state — slice `(K|V, ℓ)` is before the lie
/// in any execution, so its bytes are the accused's — while the anchor, the out tile and the
/// sub-roots are the accused's filing: the block path of a chunk is the challenger's, the top path
/// the accused's, and only together do they reach the anchor a downstream-consistent forger
/// committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnHeldEvidenceV1 {
    pub evidence: PalwAttnSiteEvidenceV1,
    pub slice_sub_roots: Vec<Hash64>,
}

/// **Where a held bottom cannot be built from the filing** (4-ter.3 step 6): the challenger's own
/// sub-root of slice `slice` — layer `layer`'s K or V — differs from the one the accused filed, so
/// the accused's checkpoint disagrees with rows that precede its lie. The route there is the
/// held-DA `StateChunk` demand and then `CheckpointAccused` (or a bottom on the disclosed bytes),
/// never this bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAttnHeldFallbackV1 {
    pub slice: u32,
    pub kind: PalwStateChunkKindV1,
    pub layer: u16,
    pub own: Hash64,
    pub filed: Hash64,
}

impl PalwAttnHeldEvidenceV1 {
    /// **The site, as the court derives it from this evidence, its rows opened under
    /// `opening_cap`** — the claim's own ladder under the held regime
    /// ([`crate::palw_court_v2::palw_attn_opening_cap_v1`]); a held job is past the structural
    /// `2^22` [`PalwAttnSiteEvidenceV1::site_v1`] opens under. With the anchor for the bottom, without
    /// it for the claims.
    pub fn site_v1(
        &self,
        artifact_root: Hash64,
        with_anchor: bool,
        opening_cap: u64,
    ) -> Result<PalwAttnDisputeSiteV2, PalwAttnResponderError> {
        let operands = crate::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&self.evidence.operand_openings, artifact_root)
            .map_err(|e| PalwAttnResponderError::Site(e.to_string()))?;
        let anchor = if with_anchor { self.evidence.anchor.as_ref().map(|a| &a.anchor) } else { None };
        crate::palw_court_v2::palw_attn_dispute_site_unpinned_v3(
            &self.evidence.binding,
            &operands,
            self.evidence.narrowed,
            anchor,
            opening_cap,
        )
        .map_err(|e| PalwAttnResponderError::Site(e.to_string()))
    }

    fn anchor_evidence(&self) -> Result<&PalwAttnAnchorEvidenceV1, PalwAttnResponderError> {
        self.evidence.anchor.as_ref().ok_or(PalwAttnResponderError::EvidenceMissing("checkpoint anchor"))
    }

    fn layout(&self, site: &PalwAttnDisputeSiteV2) -> Result<crate::palw_state_chunk_map::PalwStateLayoutV4, PalwAttnResponderError> {
        crate::palw_state_chunk_map::palw_state_layout_v4(&self.evidence.binding.shape_profile, site.site.anchor_positions)
            .map_err(|_| PalwAttnResponderError::EvidenceMissing("the anchor's held layout"))
    }

    /// **The held root claim this evidence files** (`CourtAttnRootClaimedHeld`, unsigned — the
    /// responder signs `palw_attn_root_claim_message_v1` over its root as for the other two forms).
    /// `site` is [`Self::site_v1`]'s, with or without the anchor (the root does not read it).
    pub fn root_claim_held_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        session_id: Hash64,
        arity: u8,
    ) -> Result<crate::palw_state_v2::PalwConsensusObjectV2, PalwAttnResponderError> {
        let root = self.evidence.root_claim_v1(site)?;
        Ok(crate::palw_state_v2::PalwConsensusObjectV2::CourtAttnRootClaimedHeld {
            session_id,
            root,
            arity,
            binding: Box::new(self.evidence.binding.clone()),
            out_tile: self.evidence.out_tile.clone(),
            anchor: Box::new(self.anchor_evidence()?.anchor.clone()),
            slice_sub_roots: self.slice_sub_roots.clone(),
            operand_openings: self.evidence.operand_openings.clone(),
            signature: Vec::new(),
        })
    }

    pub fn round_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<PalwAttnDissectRoundV1, PalwAttnResponderError> {
        self.evidence.round_v1(site, phase)
    }

    pub fn divergent_child_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<Option<u8>, PalwAttnResponderError> {
        self.evidence.divergent_child_v1(site, phase)
    }

    /// **The 4-ter.3 step 6 case, named**: a slice of layer `site`'s own layer whose sub-root this
    /// party computes differently from the one it opens against. `None` when the bottom can be built.
    /// `site` must be derived with the anchor (`site_v1(root, true)`).
    pub fn fallback_v1(&self, site: &PalwAttnDisputeSiteV2) -> Result<Option<PalwAttnHeldFallbackV1>, PalwAttnResponderError> {
        let layout = self.layout(site)?;
        let own = crate::palw_state_chunk_map::palw_state_slice_sub_roots_v4(&layout, &self.anchor_evidence()?.chunk_hashes)
            .map_err(|_| PalwAttnResponderError::EvidenceMissing("the anchor's own sub-roots"))?;
        if own.len() != self.slice_sub_roots.len() {
            return Err(PalwAttnResponderError::EvidenceMissing("a sub-root per slice"));
        }
        for kind in [PalwStateChunkKindV1::Key, PalwStateChunkKindV1::Value] {
            let (flat, _) = integer_kv_state_locate_v1(&layout.attn, kind, site.site.attn_layer, 0)
                .ok_or(PalwAttnResponderError::EvidenceMissing("the layer's slice in the anchor's layout"))?;
            let slice = layout.address(flat).ok_or(PalwAttnResponderError::EvidenceMissing("an address"))?.slice;
            if own[slice as usize] != self.slice_sub_roots[slice as usize] {
                return Ok(Some(PalwAttnHeldFallbackV1 {
                    slice,
                    kind,
                    layer: site.site.attn_layer,
                    own: own[slice as usize],
                    filed: self.slice_sub_roots[slice as usize],
                }));
            }
        }
        Ok(None)
    }

    /// **The bottom of a narrowed held dissection**: [`PalwAttnSiteEvidenceV1::bottom_v1`] with each
    /// chunk's path cut at its slice — the block path from this party's own leaves, the top path
    /// from `slice_sub_roots`. For the responder the two halves are one tree; for a challenger the
    /// top half is the accused's, which is what reaches the anchor a consistent forger committed.
    pub fn bottom_v1(
        &self,
        site: &PalwAttnDisputeSiteV2,
        phase: &PalwAttnDissectPhaseV1,
    ) -> Result<PalwAttnDissectBottomV1, PalwAttnResponderError> {
        let mut bottom = self.evidence.bottom_v1(site, phase)?;
        let layout = self.layout(site)?;
        let leaves = &self.anchor_evidence()?.chunk_hashes;
        for tile in [&mut bottom.k, &mut bottom.v] {
            if let PalwAttnTileEvidenceV1::Checkpoint { chunk, .. } = tile {
                let address = layout
                    .address(u64::from(chunk.chunk_index))
                    .ok_or(PalwAttnResponderError::EvidenceMissing("the chunk's address"))?;
                let block_hashes: Vec<Hash64> = (0..address.block_count)
                    .map(|b| layout.flat_index(address.slice, b).and_then(|flat| leaves.get(flat as usize).copied()))
                    .collect::<Option<_>>()
                    .ok_or(PalwAttnResponderError::EvidenceMissing("the slice's own leaves"))?;
                let mut siblings = crate::palw_step_leg::state_slice_path_v4(&block_hashes, address.block as usize)
                    .map_err(|_| PalwAttnResponderError::EvidenceMissing("the chunk's block path"))?;
                siblings.extend(
                    crate::palw_state_chunk_map::palw_state_top_path_from_sub_roots_v4(&layout, &self.slice_sub_roots, address.slice)
                        .map_err(|_| PalwAttnResponderError::EvidenceMissing("the filed top path"))?,
                );
                chunk.siblings = siblings;
            }
        }
        Ok(bottom)
    }
}
