//! Snapshot admission after the actual forward, separate from root admission.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1;
use misaka_palw_base0::fp_recompute::base0_fp_recompute_state_at_position_v1;

#[test]
fn native_q36_snapshot_serializer_matches_raw_bytes_and_roots_at_both_cadences() {
    let (w, _) = fixture();
    for positions in [15, 16] {
        assert_eq!(palw_checkpoint_leaf_carries_recurrence_v1(&w.profile, positions), positions == 16);
        let mut kernels = w.backend.gdn_recompute_kernels_v2().unwrap();
        let raw = base0_fp_recompute_state_at_position_v1(
            &w.profile,
            &w.honest.binding.job_context,
            &w.honest.prompt_token_ids,
            &w.honest.output_token_ids,
            positions,
            &mut kernels,
            w.honest.prompt_ids_form,
        )
        .unwrap();
        let chunks = kernels.state_chunks_snapshot_retained_v4(&w.profile, positions).unwrap();
        assert_eq!(borsh::to_vec(&chunks).unwrap(), borsh::to_vec(&raw.chunks).unwrap());
        assert_eq!(
            misaka_palw_base0::fp_interval::base0_state_chunks_root_for_v1(&w.profile, positions, &chunks).unwrap(),
            raw.state_chunks_root
        );
    }
}

struct AtSnapshot<'a, K> {
    inner: K,
    ceiling: u64,
    blocker: &'a RefCell<Option<NativeDemandLeaseV1>>,
    forwards: &'a Cell<usize>,
    entered: &'a Cell<usize>,
}
impl<K: Base0FpRecomputeKernelsV1> Base0FpRecomputeKernelsV1 for AtSnapshot<'_, K> {
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
        self.inner.forward_no_capture(token, position)?;
        self.forwards.set(self.forwards.get() + 1);
        Ok(())
    }
    fn state_chunks(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
        self.inner.state_chunks(profile, positions)
    }
    fn state_chunks_snapshot_retained_v4(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, NativeDecodeErrorV1<String>> {
        assert_eq!(positions, 16);
        assert_eq!(self.forwards.get(), 16, "real prefix completed before this target admission");
        assert!(self.blocker.borrow().is_none());
        *self.blocker.borrow_mut() = reserve_native_requests_v1(free(self.ceiling)).unwrap();
        self.entered.set(self.entered.get() + 1);
        self.inner.state_chunks_snapshot_retained_v4(profile, positions)
    }
}
/// Deliberately implements only the old raw interface. Its typed default must
/// withhold without interpreting missing local coverage as model misconduct.
struct RawOnly<K>(K);
impl<K: Base0FpRecomputeKernelsV1> Base0FpRecomputeKernelsV1 for RawOnly<K> {
    fn forward_no_capture(&mut self, token: usize, position: usize) -> Result<(), Base0FpRecomputeError> {
        self.0.forward_no_capture(token, position)
    }
    fn state_chunks(
        &self,
        profile: &kaspa_consensus_core::palw_step::PalwShapeProfileV3,
        positions: u32,
    ) -> Result<Vec<Vec<u8>>, Base0FpRecomputeError> {
        self.0.state_chunks(profile, positions)
    }
}

#[test]
fn post_forward_snapshot_capacity_and_missing_local_coverage_withhold_then_native_court_retries() {
    if !child(
        "MISAKA_GDN_SNAPSHOT_CALLER_CHILD",
        &format!("{}::post_forward_snapshot_capacity_and_missing_local_coverage_withhold_then_native_court_retries", module_path!()),
    ) {
        return;
    }
    let (w, mut case) = fixture();
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let ceiling = 256 << 20;
    let blocker = RefCell::new(None);
    let forwards = Cell::new(0);
    let entered = Cell::new(0);
    let signs = Cell::new(0);
    let signer = |m: &[u8], c: &[u8]| {
        signs.set(signs.get() + 1);
        sign(m, c)
    };
    let accuser = PalwGdnHeldAccuserV1 {
        bond: bond(2),
        network_domain: Hash64::from_u64_word(0xD0),
        openings: &w.openings,
        oracle: &w.oracle,
        sign: &signer,
    };
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let refused = palw_gdn_held_step_retained_v2(
        &mut case,
        &w.forged.view(),
        || AtSnapshot {
            inner: w.backend.gdn_recompute_kernels_v2().unwrap(),
            ceiling,
            blocker: &blocker,
            forwards: &forwards,
            entered: &entered,
        },
        &[],
        100,
        20,
        &accuser,
    );
    assert_eq!((forwards.get(), entered.get()), (16, 1));
    assert!(matches!(refused, Err(PalwGdnHeldErrorV2::Resource(_))));
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    drop(blocker.borrow_mut().take());
    assert_eq!(free(ceiling), ceiling);
    let unavailable = palw_gdn_held_step_retained_v2(
        &mut case,
        &w.forged.view(),
        || RawOnly(w.backend.gdn_recompute_kernels_v2().unwrap()),
        &[],
        100,
        20,
        &accuser,
    );
    assert!(
        matches!(unavailable, Err(PalwGdnHeldErrorV2::Resource(_))),
        "unsupported local ownership cannot become honest/settled/model unsupported"
    );
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    assert_eq!(free(ceiling), ceiling);
    // Exercise the production trait-object delegator, rather than only the
    // concrete Qwen method, on the successful retry through native court/sign.
    let next = palw_gdn_held_step_retained_v2(
        &mut case,
        &w.forged.view(),
        || PalwGdnBoxedKernelsV1(Box::new(w.backend.gdn_recompute_kernels_v2().unwrap())),
        &[],
        100,
        20,
        &accuser,
    )
    .unwrap();
    assert!(matches!(next, PalwGdnHeldNextRetainedV2::Accuse { .. }));
    assert_eq!(signs.get(), 1);
    drop(next);
    assert_eq!(free(ceiling), ceiling);
}
