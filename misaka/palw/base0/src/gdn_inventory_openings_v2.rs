//! GDN parameter openings from the canonical row stream. Merkle paths establish
//! data identity only; the existing courts still recompute the claimed steps.
//! Keep selected operands and their V1 paths, never every weight or leaf hash.
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeDemandLeaseV1, btree_node_envelope_bytes_v1, native_logical_admission_enabled_v1, native_resource_admission_enabled_v1,
    reserve_native_requests_v1,
};
use kaspa_consensus_core::{
    palw_artifact::{
        PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwInventoryLayoutCheckerV1, artifact_leaf_parts_v1, artifact_node_v1,
    },
    palw_step::{PalwShapeProfileV3, kernel_semantics_id_v1},
    palw_step_refute::KDESC_Q36_GDN_STEP,
};
use kaspa_hashes::Hash64;
use std::{collections::BTreeSet, mem::size_of, ops::Deref};

struct Ticket {
    lease: Option<NativeDemandLeaseV1>,
    memory: u64,
    peak: u64,
}
impl Ticket {
    fn new(memory: u64) -> Result<Self, String> {
        Ok(Self { lease: reserve_native_requests_v1(memory).map_err(|e| e.to_string())?, memory, peak: memory })
    }
    fn grow(&mut self, additional: u64) -> Result<(), String> {
        let total = self.memory.checked_add(additional).ok_or("GDN owner request overflow")?;
        if let Some(lease) = &mut self.lease {
            lease.grow_memory_v1(additional).map_err(|e| e.to_string())?;
        }
        self.memory = total;
        self.peak = self.peak.max(total);
        Ok(())
    }
    fn release(&mut self, bytes: u64) -> Result<(), String> {
        let remaining = self.memory.checked_sub(bytes).ok_or("GDN owner release exceeds its request")?;
        if let Some(lease) = &mut self.lease {
            drop(lease.split_memory_v1(bytes).map_err(|e| e.to_string())?);
        }
        self.memory = remaining;
        Ok(())
    }
}
fn sum(values: impl IntoIterator<Item = u64>) -> Result<u64, String> {
    values.into_iter().try_fold(0u64, |total, n| total.checked_add(n).ok_or_else(|| "GDN typed request overflow".into()))
}
fn twice(count: usize) -> Result<u64, String> {
    u64::try_from(count).ok().and_then(|n| n.checked_mul(2)).ok_or_else(|| "GDN byte request overflow".into())
}
fn array<T>(count: usize) -> Result<u64, String> {
    count.checked_mul(size_of::<T>()).and_then(|n| u64::try_from(n).ok()).ok_or_else(|| "GDN typed array request overflow".into())
}
fn copy<T: Copy>(source: &[T]) -> Result<Vec<T>, String> {
    let mut values = Vec::new();
    values.try_reserve_exact(source.len()).map_err(|e| e.to_string())?;
    values.extend_from_slice(source);
    Ok(values)
}
fn name_copy(source: &str) -> Result<String, String> {
    let mut name = String::new();
    name.try_reserve_exact(source.len()).map_err(|e| e.to_string())?;
    name.push_str(source);
    Ok(name)
}

/// Typed opening storage with its construction reservation. Borrowing cannot
/// detach the ticket; deep copies require their own audited owner.
pub struct GdnInventoryOpeningsV2 {
    openings: Vec<PalwArtifactOpeningV1>,
    ticket: Ticket,
}
impl Deref for GdnInventoryOpeningsV2 {
    type Target = [PalwArtifactOpeningV1];
    fn deref(&self) -> &Self::Target {
        &self.openings
    }
}
impl GdnInventoryOpeningsV2 {
    /// A separate typed copy of existing openings, admitted before cloning.
    /// Used by retained-input adapters; it does not cover the source allocation.
    pub fn try_clone_from_v2(source: &[PalwArtifactOpeningV1]) -> Result<Self, String> {
        let mut request =
            sum([size_of::<Self>() as u64, array::<PalwArtifactOpeningV1>(source.len())?.checked_mul(2).ok_or("GDN slot overflow")?])?;
        for opening in source {
            request = sum([
                request,
                twice(opening.operand.tensor_name.len())?,
                twice(opening.operand.bytes.len())?,
                array::<Hash64>(opening.path.len())?.checked_mul(2).ok_or("GDN path overflow")?,
            ])?;
        }
        let ticket = Ticket::new(request)?;
        let mut openings = Vec::new();
        openings.try_reserve_exact(source.len()).map_err(|e| e.to_string())?;
        for opening in source {
            openings.push(PalwArtifactOpeningV1 {
                operand: PalwArtifactOperandV1 {
                    tensor_name: name_copy(&opening.operand.tensor_name)?,
                    layer: opening.operand.layer,
                    row_start: opening.operand.row_start,
                    bytes: copy(&opening.operand.bytes)?,
                },
                leaf_index: opening.leaf_index,
                leaf_count: opening.leaf_count,
                path: copy(&opening.path)?,
            });
        }
        Ok(Self { openings, ticket })
    }
    fn oracle_request(&self) -> Result<u64, String> {
        let mut request = sum([
            size_of::<GdnInventoryOracleV2>() as u64,
            array::<PalwArtifactOperandV1>(self.len())?.checked_mul(2).ok_or("GDN oracle slot overflow")?,
        ])?;
        for opening in self.iter() {
            request = sum([request, twice(opening.operand.tensor_name.len())?, twice(opening.operand.bytes.len())?])?;
        }
        Ok(request)
    }
    /// Authenticate and reserve the oracle's independent operand copies while
    /// retaining the original opening owner through the caller's entire use.
    pub fn into_oracle_v2(self, root: Hash64) -> Result<GdnInventoryOracleV2, String> {
        let ticket = Ticket::new(self.oracle_request()?)?;
        let oracle =
            kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&self, root).map_err(|e| e.to_string())?;
        Ok(GdnInventoryOracleV2 { oracle, _ticket: ticket, openings: self })
    }
}

/// Both payloads drop before their respective reservations.
pub struct GdnInventoryOracleV2 {
    oracle: kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1,
    _ticket: Ticket,
    openings: GdnInventoryOpeningsV2,
}
impl GdnInventoryOracleV2 {
    pub fn openings(&self) -> &[PalwArtifactOpeningV1] {
        &self.openings
    }
    pub fn oracle(&self) -> &kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1 {
        &self.oracle
    }
}

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
    owner: GdnInventoryOpeningsV2,
    slots: usize,
    slot_request: u64,
    name_request: u64,
}
impl OpeningStream {
    fn new() -> Result<Self, String> {
        // Retained conservative envelope also covers the frontier copy during
        // the final fold. This is a typed request, not allocator/RSS accounting.
        let ticket = Ticket::new(sum([array::<Self>(2)?, size_of::<GdnInventoryOpeningsV2>() as u64])?)?;
        Ok(Self {
            frontier: [None; 33],
            count: 0,
            checker: PalwInventoryLayoutCheckerV1::new(),
            owner: GdnInventoryOpeningsV2 { openings: Vec::new(), ticket },
            slots: 0,
            slot_request: 0,
            name_request: 0,
        })
    }
    fn sibling(&mut self, first: u64, end: u64, sibling: Hash64) -> Result<(), String> {
        let a = self.owner.openings.partition_point(|o| u64::from(o.leaf_index) < first);
        let b = self.owner.openings.partition_point(|o| u64::from(o.leaf_index) < end);
        for opening in &mut self.owner.openings[a..b] {
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
        let name_request = (name.len() as u64).checked_mul(2).ok_or("GDN checker name overflow")?;
        if name_request > self.name_request {
            self.owner.ticket.grow(name_request - self.name_request)?;
            self.name_request = name_request;
        }
        self.checker.push(name, layer, start, len).map_err(|e| e.to_string())?;
        if keep {
            self.owner.ticket.grow(sum([
                name_request,
                (bytes.len() as u64).checked_mul(2).ok_or("GDN operand overflow")?,
                array::<Hash64>(32)?,
            ])?)?;
            if self.owner.openings.len() == self.slots {
                let slots = self.slots.max(1).checked_mul(2).ok_or("GDN opening count overflow")?;
                let request = array::<PalwArtifactOpeningV1>(slots)?.checked_mul(2).ok_or("GDN opening slots overflow")?;
                // The new allocation is admitted while the old allocation is
                // still live. Release the old slot envelope only after growth.
                self.owner.ticket.grow(request)?;
                self.owner.openings.try_reserve_exact(slots - self.owner.openings.len()).map_err(|e| e.to_string())?;
                self.owner.ticket.release(self.slot_request)?;
                self.slots = slots;
                self.slot_request = request;
            }
            let mut path = Vec::new();
            path.try_reserve_exact(32).map_err(|e| e.to_string())?;
            self.owner.openings.push(PalwArtifactOpeningV1 {
                operand: PalwArtifactOperandV1 { tensor_name: name_copy(name)?, layer, row_start: start, bytes: copy(bytes)? },
                leaf_index: self.count,
                leaf_count: 0,
                path,
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
    fn finish(mut self) -> Result<(GdnInventoryOpeningsV2, Hash64), String> {
        self.checker.finish().map_err(|e| e.to_string())?;
        let mut root = None;
        for left in self.frontier.into_iter().flatten() {
            root = Some(match root {
                None => left,
                Some(right) => self.join(left, right)?,
            });
        }
        for opening in &mut self.owner.openings {
            opening.leaf_count = self.count;
        }
        Ok((self.owner, root.ok_or("empty GDN inventory")?.hash))
    }
}

pub(crate) fn qwen36_gdn_inventory_openings_streamed_v2(
    artifact: &crate::qwen36::Qwen36ArtifactV1,
    profile: &PalwShapeProfileV3,
) -> Result<(GdnInventoryOpeningsV2, Hash64), String> {
    // Select the parameter tensors named by actual registered GDN descriptors,
    // not a second spelling of the court's per-head byte offset/length rules.
    let gdn = kernel_semantics_id_v1(KDESC_Q36_GDN_STEP);
    let entries = (0..profile.global_node_count())
        .filter(|slot| profile.resolve_node_slot(*slot).is_some_and(|(node, _)| node.kernel_semantics_id == gdn))
        .count();
    if !cfg!(target_pointer_width = "64")
        && entries > 0
        && (native_logical_admission_enabled_v1() || native_resource_admission_enabled_v1())
    {
        return Err("GDN selector envelope requires a 64-bit target".into());
    }
    let selector_bytes = array::<u8>(
        entries.checked_mul(btree_node_envelope_bytes_v1::<(&str, Option<u16>), ()>()).ok_or("GDN selector request overflow")?,
    )?
    .checked_mul(2)
    .ok_or("GDN selector peak overflow")?;
    let _selector_ticket = Ticket::new(selector_bytes)?;
    let mut wanted: BTreeSet<(&str, Option<u16>)> = BTreeSet::new();
    for slot in 0..profile.global_node_count() {
        let (node, layer) = profile.resolve_node_slot(slot).ok_or("GDN profile slot does not resolve")?;
        if node.kernel_semantics_id == gdn {
            wanted.insert((&node.weight_name, layer));
        }
    }
    let mut stream = OpeningStream::new()?;
    let mut failure = None;
    crate::inventory::qwen36_visit_inventory_rows_v1(artifact, profile, &mut |name, layer, start, bytes| {
        let keep = wanted.contains(&(name, layer));
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
                let mut stream = OpeningStream::new().unwrap();
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
                for opening in openings.iter() {
                    assert_eq!(Some(opening), open_artifact_leaf_v1(inventory.operands(), opening.leaf_index).as_ref());
                    verify_artifact_opening_v1(&opening, root).unwrap();
                }
            }
        }
    }

    #[test]
    fn unselected_weights_do_not_accumulate_bytes_or_leaf_metadata() {
        let mut stream = OpeningStream::new().unwrap();
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

    fn tiny_stream(rows: usize) -> Result<(GdnInventoryOpeningsV2, Hash64), String> {
        let mut stream = OpeningStream::new()?;
        for row in 0..rows {
            stream.push("p", Some(0), row as u32 * 3, &[1, 2, 3], true)?;
        }
        stream.finish()
    }

    #[test]
    fn native_capacity_child() {
        use kaspa_consensus_core::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1};
        let Ok(case) = std::env::var("MISAKA_GDN_OPENING_CAPACITY_CASE") else {
            return;
        };
        let oracle_case = case.starts_with("oracle") || case == "bad-root";
        let rows = if oracle_case { 1 } else { 3 };
        let (planned, _) = tiny_stream(rows).unwrap();
        let peak = planned.ticket.peak;
        let oracle = planned.oracle_request().unwrap();
        // This pre-existing input is deliberately outside the new copy's quota.
        let source = if case.starts_with("copy") { planned.openings.clone() } else { Vec::new() };
        let copy_request = if case.starts_with("copy") {
            let copy = GdnInventoryOpeningsV2::try_clone_from_v2(&source).unwrap();
            copy.ticket.memory
        } else {
            0
        };
        let memory = match case.as_str() {
            "zero" => 0,
            "below" => peak - 1,
            "oracle-below" => planned.ticket.memory + oracle - 1,
            "oracle-exact" | "bad-root" => planned.ticket.memory + oracle,
            "copy-below" => copy_request - 1,
            "copy-exact" => copy_request,
            _ => peak,
        };
        drop(planned); // The unarmed sizing fixture does not escape into the admission test.
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: 0 }));
        if case.starts_with("copy") {
            let copied = GdnInventoryOpeningsV2::try_clone_from_v2(&source);
            if case == "copy-below" {
                assert!(copied.is_err());
            } else {
                let copied = copied.unwrap();
                assert_eq!(&*copied, source.as_slice());
                assert!(reserve_native_requests_v1(1).is_err());
                drop(copied);
            }
            assert!(reserve_native_requests_v1(memory).is_ok());
            return;
        }
        let result = tiny_stream(rows);
        if case == "zero" || case == "below" {
            assert!(result.is_err(), "construction must refuse before publishing a partial result");
        } else {
            let (owner, root) = result.unwrap();
            if case == "oracle-below" {
                assert!(owner.into_oracle_v2(root).is_err());
            } else if case == "bad-root" {
                assert!(owner.into_oracle_v2(Hash64::from_u64_word(99)).is_err());
            } else {
                let held = if oracle_case { owner.ticket.memory + oracle } else { owner.ticket.memory };
                let free = memory - held;
                if oracle_case {
                    let inputs = owner.into_oracle_v2(root).unwrap();
                    assert_eq!(inputs.openings().len(), rows);
                    use kaspa_consensus_core::palw_step_refute::PalwWeightOracleV1;
                    assert_eq!(inputs.oracle().operand_bytes("p", Some(0), 0, 3).as_deref(), Some(&[1, 2, 3][..]));
                    assert!(reserve_native_requests_v1(free + 1).is_err(), "oracle and opening tickets remain live");
                    drop(inputs);
                } else {
                    assert_eq!(owner.len(), rows);
                    assert!(reserve_native_requests_v1(free + 1).is_err(), "the returned opening owner keeps its ticket");
                    drop(owner);
                }
            }
        }
        assert!(reserve_native_requests_v1(memory).is_ok(), "refusal/drop releases every ticket");
    }

    #[test]
    fn native_capacity_boundaries_and_owner_lifetime() {
        for case in ["zero", "below", "exact", "oracle-below", "oracle-exact", "bad-root", "copy-below", "copy-exact"] {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "gdn_inventory_openings_v2::tests::native_capacity_child", "--nocapture"])
                .env("MISAKA_GDN_OPENING_CAPACITY_CASE", case)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{case}: {} {}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"), "child was not selected");
        }
    }

    #[test]
    fn empty_noncanonical_or_overflowed_streams_are_refused() {
        assert!(OpeningStream::new().unwrap().finish().is_err());
        let mut stream = OpeningStream::new().unwrap();
        stream.push("p", None, 0, &[1], false).unwrap();
        assert!(stream.push("p", None, 0, &[1], false).is_err());
        let mut overflow = OpeningStream::new().unwrap();
        overflow.count = u32::MAX;
        assert!(overflow.push("p", None, 0, &[1], true).is_err());
        assert!(overflow.owner.openings.is_empty());
    }
}
