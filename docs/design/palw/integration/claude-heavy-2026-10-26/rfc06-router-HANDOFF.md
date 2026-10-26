# RFC06 Q36 router — SEAT-S4 partial-resume→receipt HANDOFF (2026-10-26)

Branch `claude/heavy-rfc06-router`, base `2de0a526b41559994e9894e6e13a268bb4dc2247`.
Not pushed / merged / deployed. Activation unchanged (public None; normal selected-source / V6 bootstrap refused; unapproved economics dormant). No computation proof (ZK/PCS/GKR) used. Model tiers and per-claim lottery approval unchanged. K3 absent, native39 mapping unset, 15 campaign gates UNKNOWN — not filled.

## Requirement closed
SEAT-S4 segment worker holds the original native `Some`/explicit `None` decision (`SegmentRoutingGuardV2`) from bind through running/detached/done/matched/Licensed to the vertex/receipt publication door; prompt/job/guard/matched-node allocations are quoted (`reserve_native_requests_v1`) before cloning; native read / Resource refusals keep results and return Waiting; a known identity change (`Stale`) retires local claim state and requires replay; an original `None` never accepts a later `Some`. Q36 `replay_segment_from_checkpoint_v1` checks the exact bound native job first.

## Actual callers
- `kaspad/src/palw_panel.rs` SEAT-S4 resume (`BoundSegmentReplayV2::bind_with` → `start_routed_segment_v2`; `refresh_routing_with_v2` after each step).
- `kaspad/src/palw_panel/seat_progress.rs` refresh immediately before vertex publication / receipt signing.
- `misaka/palw/base0/src/qwen36_backend.rs` `require_native_routing_job_v2` at the segment entry.

## Ownership
Guard owns the retained job, authority Arc and metadata lease; `SegmentMatchBookV2` nodes were admitted by the guard before execution. Memory reservation lives in the blocking task.

## Colluding exception
The pre-existing `colluding` drill branch (false verified-resume flag, signs without arithmetic) is excluded from the publication refresh, unchanged in meaning; no new drill config/authority. It is never a verified resume.

## Changes vs staged patch
Staged patch (base SHA-256 all six matched) applied as-is; it compiled. Fix: Q36 base0 test fixture bound combine readers to the second router, which `bind_reader_router_v2` correctly refuses ("combine explicit routing input differs from the immutable selected router"); combine now binds the selected router, matching `qwen36_plan/routing_v2.rs` tests.

## Verification (executed)
Build: `buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline -p misaka-palw-base0 -p kaspad --lib --features kaspad/evm,kaspad/private-integration-config --no-run --config profile.test.package.kaspa-consensus-core.opt-level=0` (CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=1) exit 0.
- kaspad_lib sha256 `e5c5cd477ac2d8f47c90d99b9e77509e69d2705d337d180f2a552d832c72d5a2`
- misaka_palw_base0 sha256 `30c0ccc6365f1da7e7fdd6e1de1b42d06bdd5dd9fe8c4355a32cafda4d29d076`
Source: this commit (tests ran on the working tree identical to it).

| binary | filter | exit | parent tests |
|---|---|---|---|
| kaspad | `segment_routing_` | 0 | 6 passed |
| kaspad | `palw_panel::seat_s_tests` | 0 | 20 passed |
| kaspad | `palw_panel::fp_replay_routing::tests` | 0 | 1 passed |
| base0 | `segment_routing_q36_selected_reader_plan_replays_from_real_recurrence_checkpoint` | 0 | 1 passed |
| base0 | `separate_native_readers_and_layers_retarget_actual_q36_execution` | 0 | 1 passed |
| base0 | `a_hybrid_segment_resumes_only_from_a_leaf_that_carries_the_recurrence` | 0 | 1 passed |
| base0 | `p0a_graph_v8_captures_every_checkpoint_and_a_resumed_walk_is_the_fresh_one` | 0 | 1 passed |

All with `--nocapture --test-threads=1`, RUST_MIN_STACK=67108864. Logs: session scratchpad `logs/` (ephemeral). Isolated capacity child not counted separately.

## Not done / PENDING
- Live-native-owner factory / approval assertion in the Q36 test: none (synthetic native rows do not establish chain admission).
- Actual signed-receipt campaign, node drill, full suite, clippy: PENDING (not run by policy).
