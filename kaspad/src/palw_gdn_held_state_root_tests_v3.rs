//! Actual serialized predecessor state; quota at the hashing workspace stage.
use super::*;
use kaspa_consensus_core::palw_native_resource_v1::{
    NativeDemandLeaseV1, NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1, reserve_native_requests_v1,
};
use kaspa_consensus_core::palw_step_refute::memory_v2::gdn_state_root_request_v3;
use misaka_palw_base0::fp_recompute::{Base0FpRecomputeError, Base0FpRecomputeKernelsV1};
use std::cell::{Cell, RefCell};

fn fixture() -> (World, PalwGdnHeldCaseV1) {
    let w = world_with_fault_at_checkpoint(0, false, 1);
    let mut case =
        palw_gdn_held_open_retained_v3(claim_of(&w.forged, &w), &w.forged.view(), || w.backend.gdn_recompute_kernels_v2().unwrap())
            .unwrap()
            .unwrap();
    assert_eq!(w.positions, 32);
    let chain: Vec<_> = case.demands.iter().map(|unit| producer_answer(&w, &case.claim, *unit)).collect();
    note_answers_v1(&mut case, &w.forged.view(), &chain).unwrap();
    case.demanded = Some((0, 110));
    (w, case)
}
fn child(key: &str, name: &str) -> bool {
    if std::env::var(key).as_deref() == Ok("1") {
        return true;
    }
    let name = name.split_once("::").unwrap().1;
    assert!(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(key, "1")
            .status()
            .unwrap()
            .success()
    );
    false
}
fn free(ceiling: u64) -> u64 {
    let (mut lo, mut hi) = (0, ceiling);
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if reserve_native_requests_v1(mid).is_ok() { lo = mid } else { hi = mid - 1 }
    }
    lo
}

#[test]
fn actual_borrowed_root_workspace_refuses_after_quote_and_matches_native_scalar_on_retry() {
    if !child(
        "MISAKA_GDN_STATE_ROOT_DIRECT_CHILD",
        &format!("{}::actual_borrowed_root_workspace_refuses_after_quote_and_matches_native_scalar_on_retry", module_path!()),
    ) {
        return;
    }
    let (w, _) = fixture();
    let leaf = w
        .honest
        .checkpoint_leaves
        .iter()
        .filter(|leaf| {
            let positions = palw_checkpoint_positions_at_v1(&w.profile, &w.honest.binding.job_context, leaf.covered_decode_call);
            positions < w.positions && palw_checkpoint_leaf_carries_recurrence_v1(&w.profile, positions)
        })
        .last()
        .unwrap();
    let mut kernels = w.backend.gdn_recompute_kernels_v2().unwrap();
    let state = misaka_palw_base0::fp_recompute::base0_fp_recompute_state_at_covered_v1(
        &w.profile,
        &w.honest.binding.job_context,
        &w.honest.prompt_token_ids,
        &w.honest.output_token_ids,
        leaf.covered_decode_call,
        &mut kernels,
        w.honest.prompt_ids_form,
    )
    .unwrap();
    assert_eq!(state.positions, 16);
    assert_eq!(state.state_chunks_root, leaf.state_chunks_root);
    let before = borsh::to_vec(&state.chunks).unwrap();
    let oversized = vec![vec![0u8; kaspa_consensus_core::palw_step_leg::PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES + 1]];
    let ceiling = 256 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: ceiling, database_bytes: 0 }));
    let request = gdn_state_root_request_v3(&w.profile, state.positions, &state.chunks).unwrap();
    assert!(request.metadata_bytes_v3() > 0 && request.workspace_bytes_v3() > 0);
    let blocker = reserve_native_requests_v1(free(ceiling)).unwrap();
    assert!(
        matches!(request.root_v3(), Err(PalwGdnCourtErrorV2::Resource(_))),
        "metadata/quote already succeeded: this is root-workspace admission itself"
    );
    assert!(
        matches!(gdn_state_root_request_v3(&w.profile, state.positions, &oversized), Err(PalwGdnCourtErrorV2::Evidence(_))),
        "existing byte-cap refusal precedes layout/quota even with wrong count"
    );
    assert_eq!(borsh::to_vec(&state.chunks).unwrap(), before);
    drop(blocker);
    assert!(misaka_palw_base0::fp_interval::base0_state_chunks_root_for_v1(&w.profile, state.positions, &oversized).is_err());
    assert!(matches!(
        gdn_state_root_request_v3(&w.profile, state.positions, &state.chunks[..state.chunks.len() - 1]),
        Err(PalwGdnCourtErrorV2::Evidence(_))
    ));
    let root = gdn_state_root_request_v3(&w.profile, state.positions, &state.chunks).unwrap().root_v3().unwrap();
    assert_eq!(root, state.state_chunks_root);
    assert_eq!(free(ceiling), ceiling, "all nested native scratch and metadata retire; only a scalar root remains");
}

struct AtRoot<'a, K> {
    inner: K,
    ceiling: u64,
    blocker: &'a RefCell<Option<NativeDemandLeaseV1>>,
    forwards: &'a Cell<usize>,
    serialized: &'a Cell<usize>,
    workspace: &'a Cell<u64>,
    metadata: &'a Cell<u64>,
}
impl<K: Base0FpRecomputeKernelsV1> Base0FpRecomputeKernelsV1 for AtRoot<'_, K> {
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
        let chunks = self.inner.state_chunks(profile, positions)?;
        assert_eq!(positions, 16, "real native predecessor, not a fabricated cache");
        let request = gdn_state_root_request_v3(profile, positions, &chunks).expect("real serialized source has a native layout");
        self.metadata.set(request.metadata_bytes_v3());
        self.workspace.set(request.workspace_bytes_v3());
        assert!(self.metadata.get() > 0 && self.workspace.get() > 0);
        drop(request); // quote only: no native root has been computed here
        let available = free(self.ceiling);
        assert!(available > self.metadata.get());
        let ticket = reserve_native_requests_v1(available - self.metadata.get()).unwrap();
        assert!(self.blocker.borrow().is_none());
        *self.blocker.borrow_mut() = ticket;
        assert_eq!(free(self.ceiling), self.metadata.get(), "only actual metadata can fit; root workspace cannot");
        self.serialized.set(self.serialized.get() + 1);
        Ok(chunks)
    }
}

#[test]
fn actual_serialized_predecessor_root_capacity_remains_typed_without_advance_or_sign_then_retries() {
    if !child(
        "MISAKA_GDN_STATE_ROOT_CALLER_CHILD",
        &format!("{}::actual_serialized_predecessor_root_capacity_remains_typed_without_advance_or_sign_then_retries", module_path!()),
    ) {
        return;
    }
    let (w, mut case) = fixture();
    let before = (case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded);
    let ceiling = 256 << 20;
    let blocker = RefCell::new(None);
    let forwards = Cell::new(0);
    let serialized = Cell::new(0);
    let workspace = Cell::new(0);
    let metadata = Cell::new(0);
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
    let result = palw_gdn_held_step_retained_v2(
        &mut case,
        &w.forged.view(),
        || AtRoot {
            inner: w.backend.gdn_recompute_kernels_v2().unwrap(),
            ceiling,
            blocker: &blocker,
            forwards: &forwards,
            serialized: &serialized,
            workspace: &workspace,
            metadata: &metadata,
        },
        &[],
        100,
        20,
        &accuser,
    );
    assert_eq!(forwards.get(), 16, "actual teacher-forced prefix completed before the refusal");
    assert_eq!(serialized.get(), 1, "actual serializer completed, then real quota occupied the root-workspace room");
    assert!(workspace.get() > 0 && metadata.get() > 0);
    assert!(matches!(result, Err(PalwGdnHeldErrorV2::Resource(_))), "root capacity must survive native callback and Base0 mapping");
    assert_eq!((case.at, case.demands.clone(), borsh::to_vec(&case.answers).unwrap(), case.demanded), before);
    assert_eq!(signs.get(), 0);
    drop(blocker.borrow_mut().take());
    assert_eq!(free(ceiling), ceiling);
    let next = palw_gdn_held_step_retained_v2(
        &mut case,
        &w.forged.view(),
        || w.backend.gdn_recompute_kernels_v2().unwrap(),
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
