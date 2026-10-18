//! Windowed held-attention evidence for the registered Qwen36 hybrid.
//! No admission rule is relaxed here; advertised answerability remains gated until
//! responder, challenger, cache-write and recurrence paths have been validated.
use super::*;

impl Qwen36Backend {
    /// **ADR-0152 §4-ter N1: the windowed RESPONDER** — the accused executor's evidence at a held
    /// fused site, out of its own retention without laying the job out.
    ///
    /// (a) the state at the anchor (`covered = p + 1`) recomputed by a plain forward, memoized per
    /// job ([`Self::fold_anchor_state_v1`]) and required to root to (b) the anchor, the fold's own
    /// retained checkpoint leaf opened in its leg; (c) the slice sub-roots from (a); (d) the out
    /// tile and the query row opened out of the fold (a block replayed from the interval's anchor,
    /// every completed block folded against the retained digest, the path from the retained
    /// nodes); (e) the inputs, layer ℓ's K and V out of (a). What it holds is the state (the walk's
    /// cache and the anchor's chunks with their leaf hashes — ≈ 1.5–2 GB at the peak at 8,192
    /// positions of the dense reference row, the review's F8; see the trait verb's note) and a few blocks of leaf
    /// hashes, never a leaf vector. The state walk is the seat memo's, which serialises forwards. A
    /// DENSE retention (a job small enough to have kept its tiles) answers through the dense builder,
    /// with the sub-roots of the anchor it reads.
    pub(super) fn attn_held_responder_v1(
        &self,
        material: &[u8],
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1, String> {
        use kaspa_consensus_core::palw_attn_court_v1::{PalwAttnCheckpointAnchorV1, PalwAttnRowOpeningV1};
        use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, step_merkle_path_capped_v1, step_merkle_root_capped_v1};
        let retention =
            crate::produce::base0_material_decode_any_v1(material).map_err(|_| "the capture does not decode".to_string())?;
        let fold = match &retention {
            crate::produce::Base0RetentionV1::Folded(fold) => fold,
            crate::produce::Base0RetentionV1::Dense(_) => {
                let evidence = self.attn_site_evidence(material, narrowed, carried_prompt, None)?;
                let anchor = evidence.anchor.as_ref().ok_or("a held site's evidence carries its anchor")?;
                let positions = kaspa_consensus_core::palw_context_ladder::palw_checkpoint_positions_at_v1(
                    &evidence.binding.shape_profile,
                    &evidence.binding.job_context,
                    anchor.anchor.leaf.covered_decode_call,
                );
                let slice_sub_roots = kaspa_consensus_core::palw_state_chunk_map::palw_state_slice_sub_roots_v4(
                    &kaspa_consensus_core::palw_state_chunk_map::palw_state_layout_v4(&evidence.binding.shape_profile, positions)
                        .map_err(|e| format!("the hybrid anchor layout: {e:?}"))?,
                    &anchor.chunk_hashes,
                )
                .map_err(|e| format!("the hybrid anchor sub-roots: {e:?}"))?;
                return Ok(kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1 { evidence, slice_sub_roots });
            }
        };
        let binding = &fold.binding;
        if binding.shape_profile.shape_profile_id() != self.class_profile_id {
            return Err("the capture is not this class's".to_string());
        }
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let cap = self.step_ladder_cap();
        let inventory =
            crate::inventory::qwen36_inventory_v1(&self.artifact, self.profile.as_ref().ok_or("no registered hybrid profile")?)
                .map_err(|e| format!("{e:?}"))?;
        let (site, _) =
            crate::attn_held::base0_attn_held_site_v1(binding, narrowed, inventory.operands(), inventory.root(), None, cap)?;
        let covered = site.site.anchor_covered_decode_call.ok_or("the held class's site derives no anchor")?;
        // (b) The anchor: the fold's own retained leaf, opened in the leg it committed.
        let leg = crate::legs::base0_checkpoint_leg_of_retention_v1(binding, &fold.checkpoint_chunks, &fold.checkpoint_leaves)
            .map_err(|e| format!("the fold's checkpoint leg does not rebuild: {e:?}"))?;
        if step_merkle_root_capped_v1(&leg.leaf_hashes, cap).ok() != Some(binding.checkpoint_merkle_root) {
            return Err("the fold's checkpoint leaves do not root to its own binding".to_string());
        }
        let i = leg
            .leaves
            .iter()
            .position(|leaf| leaf.covered_decode_call == covered)
            .ok_or_else(|| format!("the fold commits no checkpoint at counter {covered}"))?;
        let siblings = step_merkle_path_capped_v1(&leg.leaf_hashes, i, cap).map_err(|e| format!("{e:?}"))?;
        let anchor = PalwAttnCheckpointAnchorV1 {
            leaf: leg.leaves[i].clone(),
            opening: PalwStepOpeningV1 { leaf_index: i as u64, leaf_hash: leg.leaf_hashes[i], siblings },
        };
        let (site, operand_openings) =
            crate::attn_held::base0_attn_held_site_v1(binding, narrowed, inventory.operands(), inventory.root(), Some(&anchor), cap)?;
        // (a) The state at the anchor, as this executor's run committed it.
        let state = self
            .fold_anchor_state_v1(fold, &prompt_ids, covered)
            .ok_or_else(|| format!("the state at counter {covered} does not recompute"))?;
        // (d) The two committed leaves, out of the fold.
        let query_leaf = crate::attn_held::base0_attn_held_query_leaf_v1(binding, &site)?;
        let (_, kernels) = self.interval_kernels_v1()?;
        let mut opened = crate::fp_interval::base0_fp_leaf_openings_from_fold_v1(
            fold,
            &[narrowed, query_leaf],
            &prompt_ids,
            self.checkpoint_interval().ok_or("no registered hybrid checkpoint cadence")?,
            cap,
            &kernels,
            &|c| self.fold_anchor_state_v1(fold, &prompt_ids, c),
            self.prompt_ids_form,
        )
        .map_err(|e| format!("the fold's own leaves: {e}"))?
        .into_iter();
        let mut row = || opened.next().map(|(leaf, opening)| PalwAttnRowOpeningV1 { leaf, opening }).ok_or("an opening per leaf");
        let (out_tile, query) = (row()?, row()?);
        crate::attn_held::base0_attn_held_evidence_v1(
            binding,
            narrowed,
            &site,
            operand_openings,
            out_tile,
            query,
            anchor,
            state.chunks,
            None,
        )
    }

    /// **ADR-0152 §4-ter N2: the windowed CHALLENGER** — a held fused site's evidence from the
    /// accused's filing (its held root claim) and ONE honest replay of this node's own.
    ///
    /// The replay runs from genesis through the site's step, streaming every leaf before the
    /// narrowed one into [`kaspa_consensus_core::palw_step_leg::PalwStepPrefixStreamV1`] with the
    /// accused's filed path of the narrowed leaf — the query row's opening against the accused's step
    /// root, refused unless the stream roots to it (the first divergence is not this leaf) — keeping
    /// the query tile, and leaving its cache at exactly the anchor's positions: the state this node
    /// reads layer ℓ's K and V from and builds its block paths over. Nothing the size of the job is
    /// held. The anchor, the out tile and the sub-roots are the accused's, checked against the site
    /// before a leaf is replayed.
    pub(super) fn attn_held_challenger_v1(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
    ) -> Result<kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldEvidenceV1, String> {
        use kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1;
        use kaspa_consensus_core::palw_step_leg::PalwStepPrefixStreamV1;
        let binding = &filing.binding;
        if binding.shape_profile.shape_profile_id() != self.class_profile_id {
            return Err("the filing is not this class's".to_string());
        }
        let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let cap = self.step_ladder_cap();
        let step_leaf_count = crate::fp_interval::base0_fp_binding_step_space_v1(binding, cap).map_err(|e| e.to_string())?;
        let inventory =
            crate::inventory::qwen36_inventory_v1(&self.artifact, self.profile.as_ref().ok_or("no registered hybrid profile")?)
                .map_err(|e| format!("{e:?}"))?;
        let (site, operand_openings) = crate::attn_held::base0_attn_held_site_v1(
            binding,
            narrowed,
            inventory.operands(),
            inventory.root(),
            Some(&filing.anchor),
            cap,
        )?;
        crate::attn_held::base0_attn_held_filing_is_the_sites_v1(filing, &site, narrowed)?;
        let query_leaf = crate::attn_held::base0_attn_held_query_leaf_v1(binding, &site)?;
        let step = crate::attn_held::base0_attn_held_site_step_v1(binding, &site);
        if u64::from(site.site.anchor_positions) != step {
            return Err(format!("the anchor covers {} positions and the site's step is {step}", site.site.anchor_positions));
        }
        let mut stream = PalwStepPrefixStreamV1::new_capped_v1(step_leaf_count, &filing.out_tile.opening, &[query_leaf], cap)
            .map_err(|e| format!("the accused's path of the disputed leaf: {e}"))?;
        // ONE honest replay, genesis through the site's step; its cache is this node's state at the
        // anchor when it returns.
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let mut cache = Qwen36Cache::new(&self.artifact.shape);
        let engine = Qwen36Engine::new(&self.artifact);
        let plan = self.plan.as_ref().ok_or("no registered hybrid plan")?;
        let mut replay = crate::fp_interval::Base0FpStepsV1(|token, position| {
            if token >= self.artifact.shape.vocab || position >= self.artifact.shape.max_position {
                return Err("hybrid held replay exceeds the artifact token or position bounds".into());
            }
            let (logits, trace) = engine
                .forward_token_planned(plan, &mut cache, token, position)
                .map_err(|e| format!("hybrid held replay at {position}: {e}"))?;
            Ok((logits, qwen36_captured_rows_v1(profile, &trace)))
        });
        let hasher = crate::step_tile_hasher::StepTileHasherV1::new(ctx, profile.shape_profile_id());
        let mut query_tile = None;
        let mut pushed: Result<(), String> = Ok(());
        crate::fp_interval::base0_fp_replay_interval_with_v1(
            profile,
            ctx,
            &crate::fp_interval::Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt },
            crate::fp_interval::Base0FpWindowV1 { first_step: 1, last_step: step },
            step_leaf_count,
            &mut replay,
            &mut |index, tile| {
                if index < narrowed && pushed.is_ok() {
                    pushed = stream.push(hasher.hash(&tile)).map_err(|e| e.to_string());
                    if index == query_leaf {
                        query_tile = Some(tile);
                    }
                }
                Ok(())
            },
        )?;
        pushed?;
        let (root, mut openings) = stream.finish().map_err(|e| format!("the honest rows before the disputed leaf: {e}"))?;
        if root != binding.step_merkle_root {
            return Err(
                "the honest rows before the disputed leaf and the accused's opened tile do not root to the accused's binding — \
                 the first divergence is not this leaf"
                    .to_string(),
            );
        }
        let query = PalwAttnRowOpeningV1 {
            leaf: query_tile.ok_or("the replay produced no query row")?,
            opening: openings.pop().ok_or("the stream opened no query row")?,
        };
        // Serialize both attention history and recurrence through the hybrid map's
        // existing state serializer, from this same replay's cache.
        drop(replay);
        let kernels = crate::fp_recompute::Qwen36RecomputeKernelsV1::with_cache(&self.artifact, plan, cache);
        let chunks = crate::fp_recompute::Base0FpRecomputeKernelsV1::state_chunks(&kernels, profile, site.site.anchor_positions)
            .map_err(|e| format!("the hybrid anchor state: {e}"))?;
        crate::attn_held::base0_attn_held_evidence_v1(
            binding,
            narrowed,
            &site,
            operand_openings,
            filing.out_tile.clone(),
            query,
            filing.anchor.clone(),
            chunks,
            Some(filing.slice_sub_roots.clone()),
        )
    }

    /// **ADR-0152 §4-ter.3 step 6** — the trait verb's body: the cache-write node of `kind` at layer
    /// `attn_layer`, every tile of it at history position `history_position` (a prefill position `p`
    /// is `(call 0, p)`, a decode position the head of its call), each leaf before the narrowed one,
    /// streamed from ONE honest replay to the site's step with the accused's filed path of the
    /// narrowed leaf — refused unless the stream roots to the accused's step root (the rows are then
    /// its committed ones).
    pub(super) fn attn_held_cache_write_rows_of_v1(
        &self,
        filing: &kaspa_consensus_core::palw_attn_responder_v1::PalwAttnHeldFilingV1,
        narrowed: u64,
        carried_prompt: Option<&[u32]>,
        kind: kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1,
        attn_layer: u16,
        history_position: u32,
    ) -> Result<Vec<kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1>, String> {
        use kaspa_consensus_core::palw_attn_court_v1::PalwAttnRowOpeningV1;
        use kaspa_consensus_core::palw_step::{PalwStepCoordinateV1, PalwStepNodeRoleV1, PalwStepOutLenV1, PalwStepTableV1};
        use kaspa_consensus_core::palw_step_leg::PalwStepPrefixStreamV1;
        if self.plan.is_none() || self.profile.is_none() {
            return Err("the v1 class carries no capture to read a cache row out of".to_string());
        }
        let binding = &filing.binding;
        if binding.shape_profile.shape_profile_id() != self.class_profile_id {
            return Err("the filing is not this class's".to_string());
        }
        let (profile, ctx) = (&binding.shape_profile, &binding.job_context);
        let prompt_ids = self.attn_prompt_ids_v1(binding, carried_prompt)?;
        let cap = self.step_ladder_cap();
        let step_leaf_count = crate::fp_interval::base0_fp_binding_step_space_v1(binding, cap).map_err(|e| e.to_string())?;
        let inventory =
            crate::inventory::qwen36_inventory_v1(&self.artifact, self.profile.as_ref().ok_or("no registered hybrid profile")?)
                .map_err(|e| format!("{e:?}"))?;
        let (site, _) = crate::attn_held::base0_attn_held_site_v1(
            binding,
            narrowed,
            inventory.operands(),
            inventory.root(),
            Some(&filing.anchor),
            cap,
        )?;
        crate::attn_held::base0_attn_held_filing_is_the_sites_v1(filing, &site, narrowed)?;
        let role = match kind {
            kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1::Key => PalwStepNodeRoleV1::KCacheWrite,
            kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1::Value => PalwStepNodeRoleV1::VCacheWrite,
        };
        let index = profile.attn_nodes.iter().position(|n| n.role == role).ok_or("the class declares no such cache write")?;
        let node = &profile.attn_nodes[index];
        let PalwStepOutLenV1::Fixed { elements } = node.out_len else {
            return Err("a cache write's row is not a fixed width".to_string());
        };
        let tiles = elements.div_ceil(node.tile_len.max(1));
        let slot = profile.global_node_slot(PalwStepTableV1::Attn, attn_layer, index).ok_or("the layer has no such cache write")?;
        let prefill = ctx.declared_prefill_tokens;
        let (call_index, position) =
            if history_position < prefill { (0, history_position) } else { (history_position - prefill + 1, 0) };
        let wanted: Vec<u64> = (0..tiles)
            .map(|tile_index| {
                kaspa_consensus_core::palw_step::canonical_step_leaf_index(
                    profile,
                    ctx,
                    &PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index },
                )
                .ok_or_else(|| format!("history position {history_position} has no cache-write leaf in this job"))
            })
            .collect::<Result<_, _>>()?;
        if wanted.iter().any(|leaf| *leaf >= narrowed) {
            return Err(format!("the cache-write row at {history_position} is not before the narrowed leaf {narrowed}"));
        }
        let step = crate::attn_held::base0_attn_held_site_step_v1(binding, &site);
        let mut stream = PalwStepPrefixStreamV1::new_capped_v1(step_leaf_count, &filing.out_tile.opening, &wanted, cap)
            .map_err(|e| format!("the accused's path of the disputed leaf: {e}"))?;
        let prompt: Vec<usize> = prompt_ids.iter().map(|t| *t as usize).collect();
        let mut cache = Qwen36Cache::new(&self.artifact.shape);
        let engine = Qwen36Engine::new(&self.artifact);
        let plan = self.plan.as_ref().ok_or("no registered hybrid plan")?;
        let mut replay = crate::fp_interval::Base0FpStepsV1(|token, position| {
            if token >= self.artifact.shape.vocab || position >= self.artifact.shape.max_position {
                return Err("hybrid held replay exceeds the artifact token or position bounds".into());
            }
            let (logits, trace) = engine
                .forward_token_planned(plan, &mut cache, token, position)
                .map_err(|e| format!("hybrid held replay at {position}: {e}"))?;
            Ok((logits, qwen36_captured_rows_v1(profile, &trace)))
        });
        let hasher = crate::step_tile_hasher::StepTileHasherV1::new(ctx, profile.shape_profile_id());
        let mut kept: Vec<(u64, kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1)> = Vec::new();
        let mut pushed: Result<(), String> = Ok(());
        crate::fp_interval::base0_fp_replay_interval_with_v1(
            profile,
            ctx,
            &crate::fp_interval::Base0FpIntervalStartV1::Genesis { prompt_tokens: &prompt },
            crate::fp_interval::Base0FpWindowV1 { first_step: 1, last_step: step },
            step_leaf_count,
            &mut replay,
            &mut |index, tile| {
                if index < narrowed && pushed.is_ok() {
                    pushed = stream.push(hasher.hash(&tile)).map_err(|e| e.to_string());
                    if wanted.contains(&index) {
                        kept.push((index, tile));
                    }
                }
                Ok(())
            },
        )?;
        pushed?;
        let (root, openings) = stream.finish().map_err(|e| format!("the honest rows before the disputed leaf: {e}"))?;
        if root != binding.step_merkle_root {
            return Err("the honest rows before the disputed leaf do not root to the accused's binding".to_string());
        }
        kept.sort_by_key(|(index, _)| *index);
        if kept.len() != wanted.len() || openings.len() != wanted.len() {
            return Err("the replay did not produce every tile of the cache-write row".to_string());
        }
        // `wanted` is in tile order, which is leaf order; the stream answers in the order it was asked.
        Ok(kept.into_iter().zip(openings).map(|((_, leaf), opening)| PalwAttnRowOpeningV1 { leaf, opening }).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_attn_court_v1::{
        PALW_ATTN_COURT_OBJECT_VERSION_V1, PalwAttnCourtVerdictV1, PalwAttnDissectChoiceV1, PalwAttnDissectPhaseV1,
        check_attn_dissect_bottom_v1, palw_attn_opened_lanes_v1,
    };
    use kaspa_consensus_core::palw_attn_responder_v1::{PalwAttnHeldEvidenceV1, PalwAttnHeldFilingV1};
    use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
    use kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1;
    /// The filing a held root claim built from `evidence` carries.
    fn filing_of(evidence: &PalwAttnHeldEvidenceV1) -> PalwAttnHeldFilingV1 {
        PalwAttnHeldFilingV1 {
            binding: evidence.evidence.binding.clone(),
            out_tile: evidence.evidence.out_tile.clone(),
            anchor: evidence.evidence.anchor.as_ref().expect("a held site anchors").anchor.clone(),
            slice_sub_roots: evidence.slice_sub_roots.clone(),
        }
    }

    fn session() -> Hash64 {
        Hash64::from_u64_word(0xA5E1)
    }

    fn choose(phase: &mut PalwAttnDissectPhaseV1, child: u8, daa: u64) {
        let choice =
            PalwAttnDissectChoiceV1 { version: PALW_ATTN_COURT_OBJECT_VERSION_V1, session_id: session(), round: phase.round(), child };
        phase.apply_choice(&choice, daa, 10).expect("a legal choice");
    }

    /// **An honest responder's dissection, played to tile `target`**, the rounds from `responder`, the
    /// bottom from `bottom_from` (either party's evidence) — the court's verdict at the bottom.
    fn honest_bottom(
        responder: &PalwAttnHeldEvidenceV1,
        bottom_from: &PalwAttnHeldEvidenceV1,
        artifact_root: Hash64,
        target: u64,
    ) -> Result<PalwAttnCourtVerdictV1, kaspa_consensus_core::palw_attn_court_v1::PalwAttnCourtError> {
        let site = responder.site_v1(artifact_root, false, PALW_HELD_STEP_LADDER_V1).expect("the site derives");
        let anchored = bottom_from.site_v1(artifact_root, true, PALW_HELD_STEP_LADDER_V1).expect("and with its anchor");
        assert!(anchored.site.every_position_is_checkpointed, "the held map checkpoints every position");
        let root = responder.evidence.root_claim_v1(&site).expect("the root computes");
        let committed =
            palw_attn_opened_lanes_v1(&responder.evidence.out_tile, &anchored.binding, site.head_lanes.2 as usize).expect("opens");
        let mut phase = PalwAttnDissectPhaseV1::open_with_arity(
            session(),
            &root,
            site.head_lanes,
            site.history_positions,
            &committed,
            site.site.params.values,
            2,
            site.tile_positions,
            0,
            10,
            true,
        )
        .expect("the honest root finalizes to the committed tile");
        let mut daa = 1;
        while phase.turn() != PalwBisectTurnV1::Terminal {
            let round = responder.round_v1(&site, &phase).expect("a round computes");
            phase.apply_round(&round, daa, 10).expect("an honest round folds");
            assert_eq!(bottom_from.divergent_child_v1(&site, &phase).expect("recomputes"), None, "nothing to name");
            let child = phase.child_ranges().iter().position(|&(f, c)| target >= f && target < f + c).expect("covered") as u8;
            choose(&mut phase, child, daa + 1);
            daa += 2;
        }
        let bottom = bottom_from.bottom_v1(&anchored, &phase).expect("the bottom builds");
        check_attn_dissect_bottom_v1(&phase, &bottom, &anchored.binding, &anchored.site, true)
    }

    #[test]
    fn q36_held_attention_rebuilds_both_parties_from_fold_and_public_filing() {
        let artifact = std::sync::Arc::new(crate::qwen36::qwen36_dev_fixture(4, 8));
        let mut geometry = crate::qwen36_plan::fixture_geometry_of(&artifact.shape, 4);
        geometry.n_ctx = 32;
        geometry.tile_len = geometry.attn_head_dim;
        let profile = kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v8(geometry).unwrap();
        let backend = Qwen36Backend::from_registered_profile(artifact.clone(), b"held-q36-test".to_vec(), profile.clone(), (19, 3))
            .unwrap()
            .with_step_ladder_cap(1024)
            .with_held_answerability_v1(true);
        // Admission and advertising stay conservative until the complete path is validated.
        assert!(!backend.supports_dissection());
        let (ctx, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x3614)).unwrap();
        let ids: Vec<u32> = prompt.iter().map(|id| *id as u32).collect();
        let plan = backend.plan.as_ref().unwrap();
        let run =
            qwen36_execute_free_prompt_streaming_v1(&artifact, &profile, plan, &ctx, &prompt, backend.step_ladder_cap(), &mut |_| {})
                .unwrap();
        assert!(run.tiles.tiles.is_empty(), "the producer keeps no dense leaf vector");
        assert!(
            run.binding.step_leaf_count > backend.materialize_cap(),
            "the evidence test must exceed the dense materialization cap"
        );
        let material = crate::produce::base0_fp_material_encode_v2(&run, &ids).unwrap();
        let root = crate::inventory::qwen36_inventory_v1(&artifact, &profile).unwrap().root();
        let samples: Vec<u64> = (0..run.binding.step_leaf_count)
            .filter(|leaf| {
                let c = kaspa_consensus_core::palw_step::canonical_step_coordinates(&profile, &ctx, *leaf).unwrap();
                let fused = profile
                    .resolve_node_slot(c.node_slot)
                    .is_some_and(|(n, _)| n.op_kind == kaspa_consensus_core::palw_step::PalwStepOpKindV1::AttnFused);
                fused && c.tile_index == 0 && ((c.call_index == 0 && c.position == 15) || c.call_index == 2)
            })
            .collect();
        assert_eq!(samples.len(), 2, "prefill and decode attention sites are both exercised");
        for leaf in samples {
            assert!(
                backend.attn_site_evidence(&material, leaf, Some(&ids), None).is_err(),
                "the dense builder must refuse this job past its cap"
            );
            let responder = backend.attn_site_evidence_held_v1(&material, leaf, Some(&ids), None).unwrap();
            let filing = filing_of(&responder);
            let challenger = backend.attn_site_evidence_held_v1(&[], leaf, Some(&ids), Some(&filing)).unwrap();
            assert_eq!(responder, challenger, "the public filing alone rebuilds the same honest evidence");
            let site = responder.site_v1(root, false, PALW_HELD_STEP_LADDER_V1).unwrap();
            let tiles = u64::from(site.history_positions).div_ceil(u64::from(site.tile_positions));
            assert!(tiles > 0);
            for tile in 0..tiles {
                assert_eq!(honest_bottom(&responder, &challenger, root, tile), Ok(PalwAttnCourtVerdictV1::ChallengerDefeated));
            }
            for kind in [
                kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1::Key,
                kaspa_consensus_core::palw_state_chunk_map::PalwStateChunkKindV1::Value,
            ] {
                let rows = backend.attn_held_cache_write_rows_v1(&filing, leaf, Some(&ids), kind, 3, 0).unwrap();
                assert!(!rows.is_empty(), "the cache-write fallback opens every requested row");
            }
            let mut foreign_ids = ids.clone();
            foreign_ids[0] = (foreign_ids[0] + 1) % profile.vocab_size;
            assert!(backend.attn_site_evidence_held_v1(&[], leaf, Some(&foreign_ids), Some(&filing)).is_err());
            let mut foreign_filing = filing;
            foreign_filing.slice_sub_roots[0] = Hash64::from_u64_word(0xBAD);
            assert!(backend.attn_site_evidence_held_v1(&[], leaf, Some(&ids), Some(&foreign_filing)).is_err());
        }
    }
}
