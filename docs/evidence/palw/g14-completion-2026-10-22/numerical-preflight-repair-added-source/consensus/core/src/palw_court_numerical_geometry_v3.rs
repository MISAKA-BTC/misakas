//! Native numerical topology before any root disclosure. This is not a funded
//! original, semantic closure, default right, custody obligation or court result.
//! Prelude decoding/range analysis and retained allocations require separate
//! native resource admission; the explicit counters bound enumeration only.
use crate::palw_court_source_material_graph_v3::{NativeSourceMaterialGraphV3, SourceGraphBudgetV3};
use crate::palw_mode_v2::PalwCourtParamsV2;
use crate::palw_tir_admission_v1::PalwTirClassRecordV1;
use crate::palw_tir_class_v1::PalwTirLayoutV1;
use crate::palw_tir_dissect_v1 as dissect;
use crate::palw_tir_fence2_v1::PalwTirDemandRulesV1;
use crate::{Hash64, palw_tir_public_source_v1::PalwTirPublicSourceV1 as Source};
use misaka_palw_tir::interval::{Interval, analyze_ranges};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumericalGeometryBudgetV3 {
    pub max_original_leaves: u64,
    pub max_roots: u64,
    /// Includes one vertex per internal range and terminal range, per root.
    pub max_paths: u64,
    pub max_edges: u64,
    /// Sum of all retained path lengths, not merely maximum depth.
    pub max_path_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericalGeometryCoverageV3 {
    NumericalOnly,
    SourceAndNumericalGeometryOnly,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericalPathV3 {
    path: Vec<u8>,
    first_tile: u64,
    tile_count: u64,
}
impl NumericalPathV3 {
    pub fn path_v3(&self) -> &[u8] {
        &self.path
    }
    pub fn first_tile_v3(&self) -> u64 {
        self.first_tile
    }
    pub fn tile_count_v3(&self) -> u64 {
        self.tile_count
    }
    pub fn terminal_v3(&self) -> bool {
        self.tile_count == 1
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericalRootV3 {
    leaf: u64,
    paths: Vec<NumericalPathV3>,
}
impl NumericalRootV3 {
    pub fn leaf_v3(&self) -> u64 {
        self.leaf
    }
    pub fn paths_v3(&self) -> &[NumericalPathV3] {
        &self.paths
    }
}
/// Private prelude owned by one native-derived graph. No DTO can deserialize it.
/// Parameter-cover currently has an independent equivalent prelude; consolidating
/// them without changing either API remains separate work.
struct PreparedNumericalGeometryV3 {
    verified: Box<crate::palw_tir_step_v1::PalwTirVerifiedBindingV1>,
    context: crate::palw_v2::PalwJobContextV2,
    intervals: Vec<Vec<Interval>>,
}
#[derive(Debug)]
pub struct NativeNumericalGeometryV3 {
    source_id: Hash64,
    arity: u8,
    demand: PalwTirDemandRulesV1,
    original_leaves: u64,
    roots: Vec<NumericalRootV3>,
    paths: u64,
    edges: u64,
    path_bytes: u64,
}
fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or("numerical geometry counter overflow".into())
}
impl NativeNumericalGeometryV3 {
    /// Court/demand are explicit geometry inputs, not authenticated policy.
    /// Root identity is the pair (original leaf, path), scoped by this source;
    /// no scheduling IDs or role-specific packet IDs are granted here.
    pub fn derive_v3(
        source: &Source,
        registered: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        court: &PalwCourtParamsV2,
        demand: PalwTirDemandRulesV1,
        budget: NumericalGeometryBudgetV3,
    ) -> Result<Self, String> {
        let header = source.header_v1();
        if header.step_leaf_count > budget.max_original_leaves {
            return Err("numerical original scan budget exhausted".into());
        }
        // Source is an opaque native object with a retained authenticated header;
        // resolve rechecks its original class/layout/job, once for this scan.
        let binding =
            header.resolve_registered_layout_v1(registered, layout, header.committed_execution_root, budget.max_original_leaves)?;
        let verified =
            Box::new(crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, budget.max_original_leaves).map_err(|e| e.to_string())?);
        let intervals = analyze_ranges(&verified.space.program).map_err(|e| e.to_string())?;
        let prepared = PreparedNumericalGeometryV3 { verified, intervals, context: binding.job_context };
        let arity = court.dissection_arity();
        if !crate::palw_attn_dissect::palw_attn_arity_is_legal_v1(arity) {
            return Err("numerical geometry arity".into());
        }
        let mut out = Self {
            source_id: source.source_id_v1(),
            arity,
            demand,
            original_leaves: header.step_leaf_count,
            roots: Vec::new(),
            paths: 0,
            edges: 0,
            path_bytes: 0,
        };
        for index in 0..out.original_leaves {
            let leaf = prepared.verified.space.leaf_at(&prepared.context, index).ok_or("numerical original leaf missing")?;
            let Some(site) = dissect::palw_tir_dissect_site_v1(&prepared.verified.space, &prepared.intervals, &leaf) else { continue };
            if out.roots.len() as u64 >= budget.max_roots {
                return Err("numerical root budget exhausted".into());
            }
            let crate::palw_tir_step_v1::PalwTirLeafKindV1::Commit { block, node, .. } = leaf.kind else {
                return Err("numerical site is not commit".into());
            };
            dissect::palw_tir_dissect_obligations_v1(&prepared.verified.space.program.blocks[block as usize], node)
                .map_err(|e| e.to_string())?;
            let tile = prepared.verified.space.commit_tile_len(block, node).ok_or("numerical commit tile missing")?;
            let values = dissect::palw_tir_dissect_value_bound_v2(
                &prepared.verified.space.program,
                &prepared.verified.space.program.blocks[block as usize],
                node,
                tile,
                demand,
            );
            let tiles = site.history_positions.div_ceil(site.tile_positions);
            // Analytical preflight occurs before allocating this root's paths.
            let counts = dissect::tree_inventory_v1::numerical_tree_inventory_v1(tiles, arity, site.reductions.len(), values)
                .ok_or("numerical tree exceeds native caps")?;
            if counts.max_depth as usize > crate::palw_court_shared_tir_round_v1::SHARED_TIR_MAX_PATH_V1 {
                return Err("numerical tree exceeds native shared path depth".into());
            }
            let paths = add(counts.disclosures, counts.terminals)?;
            let bytes = add(counts.disclosure_path_bytes, counts.terminal_path_bytes)?;
            let next_paths = add(out.paths, paths)?;
            let next_edges = add(out.edges, counts.child_claims)?;
            let next_bytes = add(out.path_bytes, bytes)?;
            if next_paths > budget.max_paths || next_edges > budget.max_edges || next_bytes > budget.max_path_bytes {
                return Err("numerical path/edge/retained-path budget exhausted".into());
            }
            out.roots.try_reserve(1).map_err(|_| "numerical root allocation failed")?;
            let mut root = NumericalRootV3 { leaf: index, paths: Vec::new() };
            let mut actual_edges = 0u64;
            root.paths
                .try_reserve_exact(usize::try_from(paths).map_err(|_| "numerical path length overflow")?)
                .map_err(|_| "numerical path allocation failed")?;
            // DFS frontier has at most arity*depth siblings; output and pending
            // path bytes are bounded by the complete-tree budget above.
            let mut todo = vec![(Vec::<u8>::new(), 0, u64::from(tiles))];
            while let Some((path, first, count)) = todo.pop() {
                let children = crate::palw_attn_dissect::palw_attn_child_ranges_v1(first, count, arity).map_err(|e| e.to_string())?;
                actual_edges = add(actual_edges, children.len() as u64)?;
                for (child, (start, n)) in children.into_iter().enumerate().rev() {
                    let mut p = path.clone();
                    p.push(u8::try_from(child).map_err(|_| "numerical child index overflow")?);
                    todo.push((p, start, n));
                }
                root.paths.push(NumericalPathV3 { path, first_tile: first, tile_count: count });
            }
            let terminals = root.paths.iter().filter(|p| p.terminal_v3()).count() as u64;
            let actual_bytes = root.paths.iter().try_fold(0u64, |n, p| add(n, p.path.len() as u64))?;
            if root.paths.len() as u64 != paths
                || terminals != counts.terminals
                || actual_bytes != bytes
                || actual_edges != counts.child_claims
            {
                return Err("native numerical iterator/inventory mismatch".into());
            }
            out.roots.push(root);
            out.paths = next_paths;
            out.edges = next_edges;
            out.path_bytes = next_bytes;
        }
        Ok(out)
    }
    pub fn coverage_v3(&self) -> NumericalGeometryCoverageV3 {
        NumericalGeometryCoverageV3::NumericalOnly
    }
    pub fn source_id_v3(&self) -> Hash64 {
        self.source_id
    }
    pub fn arity_v3(&self) -> u8 {
        self.arity
    }
    pub fn demand_v3(&self) -> PalwTirDemandRulesV1 {
        self.demand
    }
    pub fn roots_v3(&self) -> &[NumericalRootV3] {
        &self.roots
    }
    pub fn path_count_v3(&self) -> u64 {
        self.paths
    }
    pub fn edge_count_v3(&self) -> u64 {
        self.edges
    }
    pub fn retained_path_bytes_v3(&self) -> u64 {
        self.path_bytes
    }
    pub fn original_leaves_v3(&self) -> u64 {
        self.original_leaves
    }
}
/// Both components are derived from the SAME original source inputs. Court and
/// demand are explicit caller-selected geometry parameters: this type does NOT
/// establish that they match the accepting network's pinned frame rules/profile.
/// No native-policy/admission capability follows from composition. There is no
/// constructor accepting caller-built partial graphs or a completeness boolean.
/// Numerical root values/signatures, path execution, semantic terminal cases,
/// parameter/prompt acquisition, F02, fees and actual escrow remain unclosed.
#[derive(Debug)]
pub struct SourceAndNumericalGeometryV3 {
    source: NativeSourceMaterialGraphV3,
    numerical: NativeNumericalGeometryV3,
}
impl SourceAndNumericalGeometryV3 {
    pub fn derive_v3(
        frame: &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
        source: &Source,
        profile: Hash64,
        source_budget: SourceGraphBudgetV3,
        registered: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        court: &PalwCourtParamsV2,
        demand: PalwTirDemandRulesV1,
        numerical_budget: NumericalGeometryBudgetV3,
    ) -> Result<Self, String> {
        let source_graph = NativeSourceMaterialGraphV3::derive_v3(frame, source, profile, source_budget)?;
        let numerical = NativeNumericalGeometryV3::derive_v3(source, registered, layout, court, demand, numerical_budget)?;
        if source_graph.source_v3() != numerical.source_id_v3() {
            return Err("composed numerical source substitution".into());
        }
        Ok(Self { source: source_graph, numerical })
    }
    pub fn coverage_v3(&self) -> NumericalGeometryCoverageV3 {
        NumericalGeometryCoverageV3::SourceAndNumericalGeometryOnly
    }
    pub fn source_v3(&self) -> &NativeSourceMaterialGraphV3 {
        &self.source
    }
    pub fn numerical_v3(&self) -> &NativeNumericalGeometryV3 {
        &self.numerical
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const B: NumericalGeometryBudgetV3 = NumericalGeometryBudgetV3 {
        max_original_leaves: 1_000_000,
        max_roots: 100_000,
        max_paths: 1_000_000,
        max_edges: 1_000_000,
        max_path_bytes: 10_000_000,
    };
    fn fixture() -> crate::palw_tir_public_claim_v1::PalwTirPublicFrameFixtureV1 {
        crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1()
    }
    #[test]
    fn all_native_paths_match_independent_aggregated_topology_for_every_arity() {
        let f = fixture();
        let source = f.frame.source_identity_v1(&f.record, &f.layout, f.rules.maximum_leaves).unwrap();
        // Actual golden GQA history sites include both an already-terminal
        // single history tile and a final partially populated history tile.
        // This checks fixture coverage independently of the iterator output.
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(&f.execution.binding, B.max_original_leaves).unwrap();
        let ranges = analyze_ranges(&verified.space.program).unwrap();
        let sites = (0..f.execution.binding.step_leaf_count)
            .filter_map(|i| {
                let leaf = verified.space.leaf_at(&f.execution.binding.job_context, i).unwrap();
                dissect::palw_tir_dissect_site_v1(&verified.space, &ranges, &leaf)
            })
            .collect::<Vec<_>>();
        assert!(sites.iter().any(|s| s.history_positions.div_ceil(s.tile_positions) == 1));
        assert!(sites.iter().any(|s| s.history_positions > s.tile_positions && !s.history_positions.is_multiple_of(s.tile_positions)));
        for arity in [2, 4, 8, 16, 32, 64] {
            let court = f.rules.court.with_dissection_arity(arity).unwrap();
            let g = NativeNumericalGeometryV3::derive_v3(&source, &f.record, &f.layout, &court, PalwTirDemandRulesV1::H7, B).unwrap();
            let upper = source.numerical_tree_upper_v1(&f.record, &f.layout, &court, PalwTirDemandRulesV1::H7).unwrap();
            assert!(!g.roots_v3().is_empty());
            assert_eq!(g.roots.len() as u64, upper.eligible_roots);
            assert_eq!(g.paths, upper.trees.disclosures + upper.trees.terminals);
            assert_eq!(g.edges, upper.trees.child_claims);
            assert_eq!(g.path_bytes, upper.trees.disclosure_path_bytes + upper.trees.terminal_path_bytes);
            let mut terminals = 0;
            for root in g.roots_v3() {
                assert!(root.paths[0].path.is_empty());
                let by_path = root.paths.iter().map(|p| (p.path.clone(), p)).collect::<std::collections::BTreeMap<_, _>>();
                assert_eq!(by_path.len(), root.paths.len());
                for p in &root.paths {
                    if p.terminal_v3() {
                        terminals += 1;
                    }
                    if !p.path.is_empty() {
                        let parent = by_path.get(&p.path[..p.path.len() - 1]).unwrap();
                        let children =
                            crate::palw_attn_dissect::palw_attn_child_ranges_v1(parent.first_tile, parent.tile_count, arity).unwrap();
                        assert_eq!(children[*p.path.last().unwrap() as usize], (p.first_tile, p.tile_count));
                    }
                }
            }
            assert_eq!(terminals, upper.trees.terminals);
            assert_eq!(g.coverage_v3(), NumericalGeometryCoverageV3::NumericalOnly);
        }
    }
    #[test]
    fn every_enumeration_budget_refuses_without_a_partial_graph() {
        let f = fixture();
        let source = f.frame.source_identity_v1(&f.record, &f.layout, f.rules.maximum_leaves).unwrap();
        let derive =
            |b| NativeNumericalGeometryV3::derive_v3(&source, &f.record, &f.layout, &f.rules.court, PalwTirDemandRulesV1::H7, b);
        let g = derive(B).unwrap();
        assert!(g.retained_path_bytes_v3() > 0);
        for b in [
            NumericalGeometryBudgetV3 { max_original_leaves: g.original_leaves - 1, ..B },
            NumericalGeometryBudgetV3 { max_roots: g.roots.len() as u64 - 1, ..B },
            NumericalGeometryBudgetV3 { max_paths: g.paths - 1, ..B },
            NumericalGeometryBudgetV3 { max_edges: g.edges - 1, ..B },
            NumericalGeometryBudgetV3 { max_path_bytes: g.path_bytes - 1, ..B },
        ] {
            assert!(derive(b).is_err());
        }
    }
    #[test]
    fn composition_derives_both_native_components_but_never_full_original() {
        let f = fixture();
        let source = f.frame.source_identity_v1(&f.record, &f.layout, f.rules.maximum_leaves).unwrap();
        let sb = SourceGraphBudgetV3 { max_queries: 100_000, max_roles: 8, max_vertices: 2_000_000, max_edges: 8_000_000 };
        let g = SourceAndNumericalGeometryV3::derive_v3(
            &f.frame,
            &source,
            Hash64::from_u64_word(71),
            sb,
            &f.record,
            &f.layout,
            &f.rules.court,
            PalwTirDemandRulesV1::H7,
            B,
        )
        .unwrap();
        assert_eq!(g.coverage_v3(), NumericalGeometryCoverageV3::SourceAndNumericalGeometryOnly);
        assert_eq!(g.source_v3().source_v3(), g.numerical_v3().source_id_v3());
        let other = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1();
        let wrong = other.frame.source_identity_v1(&other.record, &other.layout, other.rules.maximum_leaves).unwrap();
        assert!(
            SourceAndNumericalGeometryV3::derive_v3(
                &f.frame,
                &wrong,
                Hash64::from_u64_word(71),
                sb,
                &f.record,
                &f.layout,
                &f.rules.court,
                PalwTirDemandRulesV1::H7,
                B
            )
            .is_err()
        );
    }
}
