# RFC-0006 Q36 SEAT-S4 routing consumer review — 2026-10-25

Status: **review only; no implementation or completion claim**. No Rust edits, Cargo execution, network access or campaign took place. Source identities, current HEAD and all dirty tracked/untracked Rust SHA-256 values are recorded in the companion JSON. Line numbers below were read from this exact source generation.

## Concrete remaining consumer

`kaspad/src/palw_panel.rs::seat_s4_resume_v1` (13995) resolves the claim backend at 14040 and takes an existing runtime-ledger reservation at 14044. It does not bind the accepted native claim owner before cloning context/prompt and calling `start_segment` at 14073. The actual Q36 `replay_segment_from_checkpoint_v1` in `misaka/palw/base0/src/qwen36_backend.rs` (1808) does not call `require_native_routing_job_v2`; it obtains `interval_kernels_v1` (1665), which uses the backend's current plan, then restores and replays the opening.

The runtime ledger reservation already exists. The missing work is native routing binding and its complete result lifetime, plus explicitly quoted retention for job/prompt/guard bookkeeping where needed. The runtime reservation must not silently stand in for a separate native retention owner.

## Ownership through receipt

`PalwSeatReplaysV1::start_segment` (1634) moves the backend and runtime reservation into the blocking worker. `PalwSeatTaskOutV1::Segment` (1183) returns only the segment result. `poll_task` (1420) clones that result into `PalwSeatReplayDoneV1` (1200). `PalwSeatResumesV1` (1904) records success in its matched set at 1949 or 2037, then produces `Licensed` at 2062. These structures retain no original Some/explicit None decision to refresh before the successful mask is reused.

The actual receipt consumers are `kaspad/src/palw_panel/seat_progress.rs`: FP Licensed at 459 and attempt Licensed at 1369 set `segments_attested` and the verified-resume flag. V3 signing occurs at 1837–1840. Capturing an Arc only inside the worker would therefore close too little: original authority and reservation must also survive completed-cache and matched-mask lifetimes, with a fresh own-native check before mask reuse/signing.

Reuse the existing `kaspad/src/palw_fp_replay_routing_v2.rs::FpReplayRoutingGuardV2` (27), `BoundFpReplayV2` (46), and `kaspad/src/palw_panel.rs::refresh_fp_routed_with_v2` (1593) pattern. Segment-specific code should preserve exact job identity and original Some/None, keep transient read/Resource failures retryable, and retire a known changed local identity without a claim conviction or penalty. A later Some cannot retrofit an originally unbound execution. Do not assume FP-specific count semantics apply unchanged to every segment.

## Already implemented

Offline SDK router-binding signing/CLI, Q36 reader/layer plan retarget, prospective block/FP/rider binding, accepted whole/FP/audit/operator binding, and rider pending/outbox sidecar handoff are present. In particular, the older prospective-routing record's pending-handoff limitation is superseded by `0006-rider-pending-native-owners-2026-10-25.md` and actual RiderBatchV2/CourtRoutingV2 code. Do not recreate those implementations.

## Required executable evidence

- Actual worker receives the same bound backend and original Arc; detached, done cache and matched mask retain its reservation and original decision.
- A quota refusal before job/prompt/guard retention leaves openings, candidate count and matched mask unchanged; release permits the same input to retry.
- Forward native progress preserving claim/job/import/declaration may reuse; changed import/router retires local result; native read failure retains it without a receipt.
- Original explicit None cannot acquire a later Some authority without reexecution; no active missing guard may license a mask.
- Use actual tiny Q36 independent-router/layer fixture to construct an authenticated checkpoint opening and run the real restore/segment arithmetic under its selected native plan.
- Retain existing real floor opening controls for junk, forged path, stranger job and local failure; add a signing counter proving stale/unavailable native results issue no V3 receipt.

Existing `kaspad/src/palw_panel/seat_s_tests.rs` controls use real floor openings (including the real backend call around 274); they do not prove native router binding or actual Q36 restore. The actual independent-router execution fixture is `misaka/palw/base0/src/qwen36_plan/routing_v2.rs::separate_native_readers_and_layers_retarget_actual_q36_execution` (204). Extend from these boundaries, preserving existing refusal controls and updating source-string pins only alongside meaningful runtime checks.

Activation remains `None`; normal bootstrap refusals, model tiers and claim-approval requirements are unchanged. K3 formal inputs and native39 settlement mapping remain unavailable. This review contributes no PASS to the 15 campaign gates.
