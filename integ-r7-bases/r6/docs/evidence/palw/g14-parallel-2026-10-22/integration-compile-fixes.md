# G14 integration — compile fixes (branch g14/integration-lanes, 2026-10-22)

Base: 564b7d3cc. All fixes are semantics-preserving; no wire tag, Borsh layout, fence, cap, deadline or assertion changed.

## Fixes

1. `consensus/core/src/config/params.rs` — close missing brace (pre-existing uncommitted fix), committed with the t12-16000 decisions doc.
2. Native inventory/wire generators (`scripts/drills/gen_rfc06_native_inventory.py`, `gen_rfc06_native_wire.py`) — the escrow lane put
   `PalwFundedOriginalPairingV1` (owns a whole carrier `Transaction`) and the `#[cfg(test)]` `PalwFundedOriginalScheduleV1` into
   `PalwTransitionExtrasV1`, so the generators traversed `crate::tx` private layouts (`AtomicU64`, `SmallVec`, `script_public_key`)
   and the schedule's `parameter_cover_v1` path. Both types added to the `manual` sets and skipped in the inventory traversal
   (`FUNDED_MANUAL`), so the cover path is no longer resolved at all. Regenerated: the unmounted private visitor files `tx.rs`,
   `subnets.rs`, `palw_funded_original_{coordinator,admission}_v1.rs` disappeared; 0 unresolved types.
3. `palw_native_resource_v1/typed.rs` — hand `NativeHeapV1`: pairing = input/output vector capacities + signature scripts + output
   script bytes (counted as heap conservatively) + payload + TFO1 envelope byte vectors; schedule = zero (all `Copy`).
4. `palw_native_resource_v1/wire.rs` — `NativeWireScanV1` for both refuses (`wire_bad`): extras has no Borsh codec, these are never
   persisted; an empty map never reaches the element scanner.
5. `scripts/drills/rfc06_native_manual_wire_review_v1.py` — removed the `ScriptPublicKey` / `TransactionMass` pins (no longer reached;
   regeneration confirms). The TFO1 envelope pin is kept (harmless, unused now).
6. `palw_court_service_lifecycle_fold_v1.rs:92`, `palw_court_service_binding_v1.rs:83` — `Hash64::as_byte_slice()` returns `&[u8;64]`;
   `try_into` from a reference does not exist; dereference instead (identical bytes).
7. `kaspad/src/palw_panel.rs`, `kaspad/src/palw_producer_backpressure.rs` (tests) — set new `PalwClaimRowV1::claim_local_settlement`
   to `String::new()` (dormant, as the core test helper does).
8. `testing/integration/src/rpc_tests.rs` — add the missing `GetPalwAcceptanceReads` arm (empty request must be answered).

## FIXMEs / design blockers
None.

## Status
| crate | `cargo check --release --tests` |
|---|---|
| kaspa-consensus-core | pass |
| kaspa-consensus | pass |
| kaspa-mining | pass |
| kaspad | pass |
| misaka-palw-remote | pass |
| misaka-palw-tir-exec | pass |
| workspace (`--exclude misaka-palw-worker`) | pass |

`misaka-palw-worker` fails only in its build script: it needs `MISAKA_LLAMA_SRC` (pinned llama.cpp build), environmental, not code.
