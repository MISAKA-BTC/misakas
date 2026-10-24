// Included inside palw_step::tests to reuse the actual existing profile corpus.
mod cached_rank_tests_v3 {
    use super::*;
    use crate::palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1};

    #[test]
    fn cached_native_rank_matches_legacy_on_shipped_profiles_tables_heads_and_context_edges() {
        let mut compared = 0;
        for (name, profile) in profiles_under_test() {
            for (prefill, decode) in [(0, 0), (1, 1), (2, 3), (7, 4), (64, 12)] {
                let mut ctx = tiny_context();
                ctx.declared_prefill_tokens = prefill;
                ctx.exact_decode_tokens = decode;
                let rank = PalwCanonicalStepRankV3::new_v3(&profile, &ctx).unwrap();
                let count = profile.global_node_count();
                let pre = profile.pre_nodes.len() as u32;
                let post = count.saturating_sub(profile.post_nodes.len() as u32);
                for slot in [0, pre.saturating_sub(1), pre, count / 2, post, count.saturating_sub(1), count, u32::MAX] {
                    for (call, position) in [(0, 0), (0, prefill.saturating_sub(1)), (0, prefill), (1, 0), (decode, 0), (u32::MAX, 0)]
                    {
                        for tile in [0, 1, u32::from(profile.gdn_heads).saturating_sub(1), u32::MAX] {
                            let coord = PalwStepCoordinateV1 { call_index: call, node_slot: slot, position, tile_index: tile };
                            assert_eq!(rank.index_v3(&coord), canonical_step_leaf_index(&profile, &ctx, &coord), "{name} {coord:?}");
                            compared += 1;
                        }
                    }
                }
            }
        }
        assert!(compared >= 8_640);
    }

    #[test]
    fn native_rank_context_capacity_refuses_before_construction_and_retains_arrays() {
        const CHILD: &str = "MISAKA_NATIVE_RANK_CONTEXT_CHILD";
        if std::env::var(CHILD).as_deref() != Ok("1") {
            let name = format!("{}::native_rank_context_capacity_refuses_before_construction_and_retains_arrays", module_path!());
            let name = name.split_once("::").unwrap().1;
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", name, "--nocapture", "--test-threads=1"])
                    .env(CHILD, "1")
                    .status()
                    .unwrap()
                    .success()
            );
            return;
        }
        let profile = tiny_profile();
        let ctx = tiny_context();
        let before = borsh::to_vec(&profile).unwrap();
        let ceiling = 1 << 20;
        arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
        let blocker = reserve_native_requests_v1(ceiling).unwrap();
        assert!(PalwCanonicalStepRankV3::new_v3(&profile, &ctx).is_err());
        assert_eq!(borsh::to_vec(&profile).unwrap(), before);
        drop(blocker);
        let rank = PalwCanonicalStepRankV3::new_v3(&profile, &ctx).unwrap();
        assert!(reserve_native_requests_v1(ceiling).is_err());
        let coordinate = PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 };
        assert_eq!(rank.index_v3(&coordinate), canonical_step_leaf_index(&profile, &ctx, &coordinate));
        drop(rank);
        assert!(reserve_native_requests_v1(ceiling).is_ok());
    }
}
