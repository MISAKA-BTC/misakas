//! Typed inventory for an already-existing view. Construction still requires
//! its own preadmission; this supports explicit retained compatibility inputs.
use super::super::model_market::*;
use crate::palw_native_resource_v1::{NativeCoverageV1, NativeHeapV1};
macro_rules! heap {
    ($ty:ty; $($field:ident),+ $(,)?) => {
        impl NativeHeapV1 for $ty {
            fn native_heap_v1(&self, component: &str) -> NativeCoverageV1 {
                let Self { $($field),+ } = self;
                let result = NativeCoverageV1::Bounded { bytes: 0 };
                $(let result = result.add::<Self>($field.native_heap_v1(component), component);)+
                result
            }
            fn native_members_v1(&self) -> Option<u64> {
                let Self { $($field),+ } = self;
                let count = 0u64;
                $(let count = count.checked_add($field.native_members_v1()?)?;)+
                Some(count)
            }
        }
    };
}
heap!(PalwEvmClassRowV1; status, share_permille, budget_blocks, canonical_leaves, is_base, registrant_payload,
    registered_daa, certified_attempt, certified_fp, roots_in_force);
heap!(PalwEvmLineRowV1; class_id, owner_payload, developer_payload, maintainer_payload, name, founded_daa, current,
    versions_published, preview_count, contributor_permille_of_leg, status);
heap!(PalwEvmVersionRowV1; root, parent, adopted_from, runtime_hash, dataset_commitment, training_config_hash,
    notes_hash, published_daa, published_by_payload, status, until_daa, attempt_claims, fp_claims, work_leaves,
    first_used_daa, last_used_daa);
heap!(PalwEvmProposalRowV1; proposal_id, root, note_hash, by_payload, posted_daa, adopted_in);
heap!(PalwEvmEvaluationRowV1; evaluator_id, score_permille, report_hash, by_payload, posted_daa, is_lines_own);
heap!(PalwEvmViewV1; chain_daa, chain_id, classes, lines, versions, proposals, evaluations, markets, positions, market_refused_classes);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_market_existing_view_inventory_visits_nested_capacities() {
        let mut view = PalwEvmViewV1::default();
        let empty = view.native_heap_v1("view").bytes().unwrap();
        view.lines.push((
            crate::Hash64::from_u64_word(1),
            PalwEvmLineRowV1 {
                class_id: crate::Hash64::default(),
                owner_payload: None,
                developer_payload: None,
                maintainer_payload: None,
                name: Vec::with_capacity(117),
                founded_daa: 0,
                current: 0,
                versions_published: 0,
                preview_count: 0,
                contributor_permille_of_leg: 0,
                status: 0,
            },
        ));
        let with_line = view.native_heap_v1("view").bytes().unwrap();
        assert!(with_line >= empty + 117 + std::mem::size_of::<(crate::Hash64, PalwEvmLineRowV1)>() as u64);
        view.proposals.insert(crate::Hash64::default(), Vec::with_capacity(3));
        assert!(view.native_heap_v1("view").bytes().unwrap() >= with_line + 3 * std::mem::size_of::<PalwEvmProposalRowV1>() as u64);
    }
}
