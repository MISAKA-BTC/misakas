# Lane F03b report: claim-common localisation frontier (ADR-0192 A/A), 2026-10-26

Branch `claude/g14-f03b` (base `2d7f6023f`). **G14 is still incomplete. Nothing is armed.** No wire tag, ID or DB schema changed.

## Map: what claim-common frontier pieces exist and what was missing
| Piece | Where | Before F03b |
|---|---|---|
| Closed request universe, owner-independent request identity (requester/relay excluded) | `palw_court_tir_requests_v1`, `palw_court_shared_tir_frontier_v1` | present |
| Request fold: dedupe (`Coalesced`), no owner slot, no session reservation | `palw_court_shared_tir_fold_v1::request` | present |
| Shared authenticated body/IDs/streams, readable by anyone | `share_body`/`share_ids`/`stream_control`, `read_shared_data_v1` | present |
| Shared root/round/bottom and native-fault judgement | `fold::root/round/bottom`, tag 178 transport | present |
| Designated custodian answers (tag 245) | `palw_tir_designated_answers_v1` | present, dormant fence |
| Conviction funnel from a shared bottom | `apply_shared_bottom_conviction_v1` | present, **no test** |
| **Production install of the shared context at original acceptance** | none. Only test fixtures wrote `CourtSharedContext` | **missing** |

## Change
- `derive_shared_tir_context_v1` (`palw_court_shared_tir_state_v1.rs`) derives the context from the original frame, class/layout and the F02 service binding. Nothing in it comes from an owner, filer or relay.
- `install_shared_frontier_context_v1` (`palw_court_service_lifecycle_fold_v1.rs`) is called after the F02 binding in plain admission (`palw_tir_public_fold_v1.rs`) and after the funded all-branch book write (`palw_funded_original_coordinator_fold_v1.rs`). It is idempotent, an installed context cannot be changed, and it fails closed when the lifecycle is in force. It stays dormant below the shared lifecycle fences or when there is no binding.

## Tests
`buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline -p kaspa-consensus-core --lib -- f03b_t05 shared_history_root_binds rooted_shared_query_and_signed_streams`. Result: exit 0, **3 passed / 0 failed**.
- `f03b_t05_shared_frontier_survives_full_owner_capture_sybil_refill_and_latest_answer` covers:
  - 8 coalition owners take every owner slot, plus 4 refill Sybils with their own operator ids.
  - The coalition requests every coordinate first and then goes silent.
  - 3 rounds of refill and outsider requests all return `Coalesced`. The root and the journal do not change, nothing is refused for capacity, and the owner count stays 8.
  - The producer answers at `hard-1`, and the outsider can read that answer. An answer at `hard` is refused.
  - Negative: the frontier data alone does not void the claim, and the producer cannot file its own conviction.
- The `shared_history_root_binds_...` test now has a positive leg. For each native fault in the false-partition case (asserted to be at least 1), an outsider's shared bottom goes through `apply_shared_bottom_conviction_v1`. The claim ends `Voided{CourtHeldVerdict}`, the producer's `slashed` goes up, and a filing by the producer is refused.
- Test-only values: fixture key seeds 50..62 (they do not show that operators are independent), hard window `DISPUTE_WINDOW_DAA` = 10,000, the unit-cost service book from `bind_clock_service`. The fixture Panel has 3 seats, not the Panel=1 slice.

## Remaining work and decisions needed
1. The positive leg and the frontier leg run in separate fixtures. There is no single run that goes all-owner capture, then outsider bottom, then slash, and there is none on a real node.
2. The tag-178 dispatcher path (`Convicted`) has not been driven end to end through `apply_object`.
3. Decision needed: the per-request byte/work admission budget, and the scheduling of different legal choices when branches overlap (ADR-0192 lists these as open). They are not implemented. Requests are bounded only by the closed universe `queries`.
4. The service binding is still dormant in production (`PALW_COURT_SERVICE_PROFILE_V1` = None, funded schedule typed-missing), so the context is not installed on any network yet.
