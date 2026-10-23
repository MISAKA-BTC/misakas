//! GDN parameter openings from the canonical row stream. Merkle paths establish
//! data identity only; the existing courts still recompute the claimed steps.
//! Keep selected operands and their V1 paths, never every weight or leaf hash.
use kaspa_consensus_core::{
    palw_artifact::{
        PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwInventoryLayoutCheckerV1, artifact_leaf_parts_v1, artifact_node_v1,
    },
    palw_step::{PalwShapeProfileV3, kernel_semantics_id_v1},
    palw_step_refute::KDESC_Q36_GDN_STEP,
};
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
struct Branch {
    hash: Hash64,
    first: u64,
    end: u64,
}

struct OpeningStream {
    // V1's leaf_count is u32. At most 32 siblings per selected leaf; odd
    // subtrees promote without introducing a duplicate leaf or path element.
    frontier: [Option<Branch>; 33],
    count: u32,
    checker: PalwInventoryLayoutCheckerV1,
    opened: Vec<PalwArtifactOpeningV1>,
}
impl OpeningStream {
    fn new() -> Self {
        Self { frontier: [None; 33], count: 0, checker: PalwInventoryLayoutCheckerV1::new(), opened: Vec::new() }
    }
    fn sibling(&mut self, first: u64, end: u64, sibling: Hash64) -> Result<(), String> {
        let a = self.opened.partition_point(|o| u64::from(o.leaf_index) < first);
        let b = self.opened.partition_point(|o| u64::from(o.leaf_index) < end);
        for opening in &mut self.opened[a..b] {
            if opening.path.len() >= 32 {
                return Err("GDN inventory path exceeds V1's u32 geometry".into());
            }
            opening.path.push(sibling);
        }
        Ok(())
    }
    fn join(&mut self, left: Branch, right: Branch) -> Result<Branch, String> {
        if left.end != right.first {
            return Err("GDN inventory subtrees are not adjacent".into());
        }
        self.sibling(left.first, left.end, right.hash)?;
        self.sibling(right.first, right.end, left.hash)?;
        Ok(Branch { hash: artifact_node_v1(&left.hash, &right.hash), first: left.first, end: right.end })
    }
    fn push(&mut self, name: &str, layer: Option<u16>, start: u32, bytes: &[u8], keep: bool) -> Result<(), String> {
        let next = self.count.checked_add(1).ok_or("GDN inventory exceeds V1's leaf count")?;
        let len = u32::try_from(bytes.len()).map_err(|_| "GDN inventory row exceeds V1's byte length")?;
        self.checker.push(name, layer, start, len).map_err(|e| e.to_string())?;
        if keep {
            self.opened.push(PalwArtifactOpeningV1 {
                operand: PalwArtifactOperandV1 { tensor_name: name.into(), layer, row_start: start, bytes: bytes.to_vec() },
                leaf_index: self.count,
                leaf_count: 0,
                path: Vec::with_capacity(32),
            });
        }
        let mut node =
            Branch { hash: artifact_leaf_parts_v1(name, layer, start, bytes), first: u64::from(self.count), end: u64::from(next) };
        let mut level = 0;
        while let Some(left) = self.frontier[level].take() {
            node = self.join(left, node)?;
            level += 1;
        }
        self.frontier[level] = Some(node);
        self.count = next;
        Ok(())
    }
    fn finish(mut self) -> Result<(Vec<PalwArtifactOpeningV1>, Hash64), String> {
        self.checker.finish().map_err(|e| e.to_string())?;
        let mut root = None;
        for left in self.frontier.into_iter().flatten() {
            root = Some(match root {
                None => left,
                Some(right) => self.join(left, right)?,
            });
        }
        for opening in &mut self.opened {
            opening.leaf_count = self.count;
        }
        Ok((self.opened, root.ok_or("empty GDN inventory")?.hash))
    }
}

pub(crate) fn qwen36_gdn_inventory_openings_streamed_v2(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &PalwShapeProfileV3,
) -> Result<(Vec<PalwArtifactOpeningV1>, Hash64), String> {
    // Select the parameter tensors named by actual registered GDN descriptors,
    // not a second spelling of the court's per-head byte offset/length rules.
    let gdn = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let mut wanted: BTreeMap<&str, BTreeSet<Option<u16>>> = BTreeMap::new();
    for slot in 0..profile.global_node_count() {
        let (node, layer) = profile.resolve_node_slot(slot).ok_or("GDN profile slot does not resolve")?;
        if node.kernel_semantics_id == gdn {
            wanted.entry(&node.weight_name).or_default().insert(layer);
        }
    }
    let mut stream = OpeningStream::new();
    let mut failure = None;
    crate::inventory::qwen36_visit_inventory_rows_v1(artifact, profile, &mut |name, layer, start, bytes| {
        let keep = wanted.get(name).is_some_and(|layers| layers.contains(&layer));
        stream.push(name, layer, start, bytes, keep).map_err(|error| {
            failure = Some(error);
            // Stop the existing emitter; return the original stream refusal below.
            crate::inventory::InventoryBuildError::NotCanonical(kaspa_consensus_core::palw_artifact::PalwInventoryError::Empty)
        })
    })
    .map_err(|error| failure.unwrap_or_else(|| format!("{error:?}")))?;
    stream.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_artifact::{PalwArtifactInventoryV1, open_artifact_leaf_v1, verify_artifact_opening_v1};

    #[test]
    fn streamed_paths_match_v1_for_sparse_dense_and_odd_trees() {
        for count in (1..=65).chain([1023, 1024, 1025]) {
            let operands: Vec<_> = (0..count)
                .map(|i| PalwArtifactOperandV1 {
                    tensor_name: "parameter".into(),
                    layer: Some(0),
                    row_start: i * 3,
                    bytes: vec![i as u8; 3],
                })
                .collect();
            let inventory = PalwArtifactInventoryV1::new(operands.clone()).unwrap();
            for mode in 0..4 {
                if count > 65 && mode != 2 {
                    continue;
                }
                let mut stream = OpeningStream::new();
                for (index, operand) in operands.iter().enumerate() {
                    let keep = match mode {
                        0 => false,
                        1 => true,
                        2 => index == 0 || index + 1 == count as usize,
                        _ => index % 3 == 1,
                    };
                    stream.push(&operand.tensor_name, operand.layer, operand.row_start, &operand.bytes, keep).unwrap();
                }
                let (openings, root) = stream.finish().unwrap();
                assert_eq!(root, inventory.root());
                for opening in openings {
                    assert_eq!(Some(&opening), open_artifact_leaf_v1(inventory.operands(), opening.leaf_index).as_ref());
                    verify_artifact_opening_v1(&opening, root).unwrap();
                }
            }
        }
    }

    #[test]
    fn unselected_weights_do_not_accumulate_bytes_or_leaf_metadata() {
        let mut stream = OpeningStream::new();
        let mut reference = kaspa_consensus_core::palw_artifact::PalwArtifactInventoryStreamV1::new();
        let bytes = [0x5a; 8192];
        for i in 0..10_000 {
            stream.push("weight", Some(0), i * bytes.len() as u32, &bytes, false).unwrap();
            reference.push("weight", Some(0), i * bytes.len() as u32, &bytes).unwrap();
        }
        let (openings, root) = stream.finish().unwrap();
        assert!(openings.is_empty());
        assert_eq!(root, reference.finish().unwrap().root);
    }

    #[test]
    fn q36_stream_keeps_gdn_parameters_under_the_complete_unchanged_root() {
        use kaspa_consensus_core::{
            palw_artifact::PalwProvenOperandsV1,
            palw_qwen36_profile::{PalwQwen36GeometryV1, qwen36_profile_v8},
            palw_step_refute::PalwWeightOracleV1,
        };
        let artifact = crate::qwen36::qwen36_dev_fixture(4, 8);
        let profile = qwen36_profile_v8(PalwQwen36GeometryV1 {
            layer_count: 4,
            full_attention_interval: 4,
            hidden_dim: 32,
            attn_heads: 4,
            attn_kv_heads: 2,
            attn_head_dim: 16,
            rope_dims: 4,
            rope_freq_base_bits: 0x4B18_9680,
            gdn_k_heads: 2,
            gdn_v_heads: 4,
            gdn_head_dim: 8,
            gdn_conv_kernel: 4,
            n_experts: 8,
            experts_per_token: 4,
            moe_dim: 16,
            shared_dim: 16,
            attn_output_gate: 1,
            vocab_size: 64,
            n_ctx: 160,
            n_threads: 1,
            rms_eps_q: 1,
            tile_len: 512,
        })
        .unwrap();
        let legacy = crate::inventory::qwen36_inventory_v1(&artifact, &profile).unwrap();
        let (openings, root) = qwen36_gdn_inventory_openings_streamed_v2(&artifact, &profile).unwrap();
        assert_eq!(root, legacy.root());
        assert!(!openings.is_empty());
        assert!(openings.len() < legacy.operands().len());
        let oracle = PalwProvenOperandsV1::from_openings_v1(&openings, root).unwrap();
        let gdn = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
        let mut kept = 0;
        for (index, operand) in legacy.operands().iter().enumerate() {
            let wanted = (0..profile.global_node_count()).any(|slot| {
                let (node, layer) = profile.resolve_node_slot(slot).unwrap();
                node.kernel_semantics_id == gdn && node.weight_name == operand.tensor_name && layer == operand.layer
            });
            if wanted {
                assert_eq!(Some(&openings[kept]), open_artifact_leaf_v1(legacy.operands(), index as u32).as_ref());
                assert_eq!(
                    oracle
                        .operand_bytes(&operand.tensor_name, operand.layer, operand.row_start, operand.bytes.len() as u32)
                        .as_deref(),
                    Some(operand.bytes.as_slice())
                );
                kept += 1;
            } else {
                assert!(
                    oracle.operand_bytes(&operand.tensor_name, operand.layer, operand.row_start, operand.bytes.len() as u32).is_none()
                );
            }
        }
        assert_eq!(kept, openings.len());
        let kept_bytes: usize = openings.iter().map(|o| o.operand.bytes.len()).sum();
        let all_bytes: usize = legacy.operands().iter().map(|o| o.bytes.len()).sum();
        println!("Q36 GDN parameter opening bytes: {kept_bytes}/{all_bytes}; rows {}/{}", openings.len(), legacy.operands().len());
        use kaspa_consensus_core::{
            palw_attempt_rules_v1::PalwAttemptRulesV1,
            palw_attempt_v2::palw_attempt_job_v1,
            palw_backend::PalwExecutionBackendV1,
            palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
            palw_qwen36_profile::qwen36_held_canonical_v1,
            palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
            palw_step_refute::{
                PalwStepRefuteError,
                memory_v2::{PalwGdnCourtErrorV2, check_gdn_step_refutation_v2},
            },
        };
        let backend = crate::qwen36_backend::Qwen36Backend::from_registered_profile(
            std::sync::Arc::new(artifact),
            b"misaka-palw-rc".to_vec(),
            profile.clone(),
            qwen36_held_canonical_v1(160),
        )
        .unwrap()
        .with_step_ladder_cap(PALW_STEP_LEG_MAX_LEAVES)
        .with_prompt_ids_form(PalwPromptIdsFormV1::MerkleV1)
        .with_attempt_rules(PalwAttemptRulesV1::CoreV1);
        let (canonical, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0x535452)).unwrap();
        let out = backend.execute(&palw_attempt_job_v1(canonical, true), &prompt).unwrap();
        let carried: Vec<u32> = prompt.iter().map(|t| *t as u32).collect();
        let mut owned = backend.gdn_accuser_view_v2(&out.material, Some(&carried)).unwrap();
        owned.prompt_ids_form = [PalwPromptIdsFormV1::MerkleV1, PalwPromptIdsFormV1::Flat]
            .into_iter()
            .find(|form| prompt_token_ids_match_v1(*form, &owned.prompt_token_ids, &owned.binding.job_context.prompt_token_ids_hash))
            .unwrap();
        let coord = owned
            .tiles
            .tiles
            .iter()
            .map(|(_, tile)| tile.coord)
            .find(|coord| profile.resolve_node_slot(coord.node_slot).is_some_and(|(node, _)| node.kernel_semantics_id == gdn))
            .unwrap();
        let evidence =
            crate::gdn_evidence_v2::base0_gdn_step_evidence_v2(&owned.view(), coord, || backend.gdn_recompute_kernels_v2().unwrap())
                .unwrap();
        let crate::gdn_evidence_v2::Base0GdnEvidenceV2::Step(refutation) = evidence else {
            panic!("honest predecessor");
        };
        assert!(
            matches!(
                check_gdn_step_refutation_v2(
                    &refutation,
                    owned.binding.committed_execution_root,
                    profile.shape_profile_id(),
                    &oracle,
                    PALW_STEP_LEG_MAX_LEAVES
                ),
                Err(PalwGdnCourtErrorV2::Evidence(PalwStepRefuteError::NoFaultFound))
            ),
            "the actual step court can read its parameters from the streamed subset and acquits the honest step"
        );
    }

    #[test]
    fn empty_noncanonical_or_overflowed_streams_are_refused() {
        assert!(OpeningStream::new().finish().is_err());
        let mut stream = OpeningStream::new();
        stream.push("p", None, 0, &[1], false).unwrap();
        assert!(stream.push("p", None, 0, &[1], false).is_err());
        let mut overflow = OpeningStream::new();
        overflow.count = u32::MAX;
        assert!(overflow.push("p", None, 0, &[1], true).is_err());
        assert!(overflow.opened.is_empty());
    }
}
