# Family lane FPSEL (`FpSelection`) report, 2026-10-26

Branch `claude/g14-fam-fpsel2`, base `d8aded6de`. **Nothing is armed.** No record is pinned and no preset changed. No wire tag, ID or DB schema changed. Behavior below the fence is unchanged byte for byte. No new fence was added, so `for_each_fence` and fleet identity are not touched.

## Stage table (what the route has today)

Route: `route_armed(FpSelection) = framed && palw_tir_public_conviction_v1 in force at acceptance` (`palw_g14_gates_v1.rs`). Every FP job also needs `FpTokenizer`, so an FP claim earns again only when that family is armed too.

| Stage | State | Where |
|---|---|---|
| Admission: option (b) refuses an unframed FP selection job | implemented, fold test T-N10 | `palw_g14_route_fp_refusal_v1`, called in `palw_state_v2/objects.rs`. `framed` = `tir_public_originals` holds the claim |
| Admission: G-03, a TIR original without TFP1 is refused | implemented | `require_original_frame_v1` |
| Duty / supply: the frame's custodians, owner queue, slice debit | implemented, fold tests | `palw_tir_public_fold_v1`, `palw_tir_frame_cost_v1` |
| Honest Final and reward | implemented. Held to the liability horizon (`palw_reward_liability_horizon_v1`) | gates §4 |
| Lie: an outsider convicts through the selection door (greedy, Decision 11 sampling, V4) | implemented, fold T-N2 | `palw_tir_public_fold_v1` (selection door, `palw_court_shared_selection_native_tests.rs`) |
| Lie: conviction from a shared bottom, then slash | implemented. F03b positive leg exists | `apply_shared_bottom_conviction_v1` |
| Silence: DA default | implemented in the frame (withheld answer → default, never a computation fault) | frame doors |
| Reorg / restart / pruned IBD | fold-level only. **No node E2E for a selection pursuit** | — |
| Producer attaches TFP1 (node) | **canonical claims only**: `palw_panel.rs::build_canonical_claim` quotes and attaches TFP1 past the fence, and drill D-18 removes it | `kaspad/src/palw_panel.rs` |
| **Free-prompt rail (gateway → rail → fp-worker) attaches TFP1** | **missing.** `misaka-palw-fp-rail` builds with `build_fp_commitment_tx_with` and never quotes a frame. Past the fence, every user FP claim from the rail is refused: by G-03 on a TIR class, and by option (b) on a legacy class. This fails closed and gives no reward, but the user FP path earns nothing. | `misaka/palw/gateway/src/bin/rail.rs` |
| Frame cost value | estimate added (this lane). Record list stays empty | `palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1` |

## Changes
- `central:` `palw_g14_values_v1.rs` gets `PALW_G14_TIR_FRAME_COST_ESTIMATE_V1`. It is the 2026-10-16 measured candidate with one change: `reorg_retention_daa` is 360 (finality depth, same reason as V9) instead of 256. The register entry is "2026-10-26 lane FPSEL" in `docs/governance/parameter-register.md` and gives the basis and review conditions for each value. It is not in any preset or record, so arming still returns `NoRecord`.
- `central:` `palw_g14_gates_v1.rs` gets `PALW_G14_FP_SELECTION_FAMILY_ARMING_REQUIREMENTS_V1`, the family checklist the per-fence records must show. It does not change the record format.
- Tests: `fpsel_tir_frame_cost_estimate_is_well_formed_and_not_a_record` and `fpsel_family_requirements_name_only_gated_fences`. Results are in the commit message and the final section below.

## Not done (remaining work)
1. **The rail does not attach TFP1.** Fix: the rail asks the node for `palw_tir_public_original_quote_v1` through an RPC wrapper of `ConsensusApi`, then signs the source pledge with the sidecar key and calls `build_fp_commitment_tx_with_tir_public_original_v1`. It also needs a node E2E that runs gateway → rail → frame. This needs an RPC addition (a new method name; it changes no existing wire). It is the largest gap in this family.
2. F03b residual: no native test drives a conviction from evidence spread over several packets, and there is no wide-fixture bottom builder. Not done.
3. No node E2E covers full collusion on a selection lie, slash by an outside bond, or V7 restart/reorg/pruned IBD during a pursuit.
4. The frame cost clock needs re-measuring on the slowest node with genesis-size classes. `maximum_door_daa` must be re-checked against the horizon at the real fence.
5. `FpTokenizer` is another lane's family and must be armed too before any FP claim earns again.

## Test results
`buildslot.sh env CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 cargo test --offline -p kaspa-consensus-core --lib -- fpsel_ val_`: both new tests passed, and all 13 tests in `palw_g14_values_v1::tests` passed. The run selected 94 tests in total: 93 passed and 1 failed. The filter `val_` also matched `palw_step_refute::routing_v2::tests::routed_retrieval_authenticates_committed_bytes_without_a_merits_verdict`. That test fails (`ProfileNotCanonical("weight name and dtypes must be both present or both absent")`) and this change does not touch it. I did not check whether it also fails on the base commit, and I have not fixed it.

## Follow-up 2026-10-26: TFP1 on the free-prompt rail

**Done (source compiles; not run on a network):**
- One shared pledge assembly, `kaspa_pq_validator_core::palw_tir_public_original_pledges_v1`. The node producer (`build_canonical_claim`) now calls it in place of its inline code, and the rail calls the same function. This replaced that inline code and changed none of its steps.
- A read-only RPC `GetPalwTirOriginalQuote`, **op 248**, proto 1332/1333. It wraps the existing `palw_tir_public_original_quote_v1` and makes no consensus change. Why 248: the op type is u8, ops 250–255 are already claimed (the bond budget and RFC-0016), and 248 sits in X8R's 240–249 range. No branch uses it, but the coordinator must confirm the number.
- fp-worker: `retain_v1` writes `tir-public-binding.hex` (a TIR header only) next to `material.bin`.
- rail: the watcher passes that file as `--public-binding`. Before choosing the history-custody route, the one-shot rail quotes op 248, signs the pledges with the bond key (sidecar or seed), and builds with `build_fp_commitment_tx_with_tir_public_original_v1` (fee = relay + rent). If the TIR header has no `--rpc`, the rail prints a warning. Past the fence, consensus then refuses the claim (G-03).
- Node-less: `palw-remote-miner` only mines V2 Attempts and never builds FP commitments. The node-less FP path is the rail with `--rpc` pointed at a public wRPC node, so it gets the same TFP1 attachment.
- Tests: `fpsel_op248_messages_round_trip_and_refuse_unknown_versions` passed. `cargo check` passed for kaspad, gateway, base0, validator-core, grpc client/server (lib+bins) and kaspa-testing-integration `--tests`; the build includes the new rpc_tests arm.

**Not done:**
1. The node E2E (gateway → rail → fp-worker → framed claim → Final, and an unframed claim refused). Running it surfaced a further gap. The frame's original custodian (`palw_tir_public_custodian::retained_original_v1`) serves only an FPC1/TIR capture found in **the producer node's own** retention dir. A rail co-located with its node can stage into that dir (`--retention-dir`). A node-less rail has no node to serve the original, so its frame would end in a DA default. This needs a design decision: a provider delegation for the frame original, along the lines of RFC-0009's DA transfer.
2. Choice of op number (above).

**(3) Checked:** `routing_v2::...routed_retrieval...` also fails at base d8aded6de with the same `ProfileNotCanonical` error. The failure predates this lane.
