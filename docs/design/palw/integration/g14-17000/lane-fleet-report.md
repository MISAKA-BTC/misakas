# Lane FLEET report: matching the public fleet's rules (2026-10-26)

Branch `claude/g14-fleet`, cut from `8f96ca0e9` (`claude/g14-panel1-17000`). Nothing was pushed, deployed, or activated.

## 1. What `origin/main` adds over the branch

- The branch and the fleet source split at **`6549f9be2`**. The fleet rev `247ade1f9` is **one commit** on top of `6549f9be2`, and `6549f9be2` is already in this branch. The other main commits do not need porting:
  - `1acfbc916` and `832d9279e` (full activation at 15,100 and then 15,400) are **not ancestors of `247ade1f9`**. They are main's own history and the fleet does not run them. `d2214e057` is a merge that publishes the `247ade1f9` tree.
  - `76d06bcb7` changes docs and joiner instructions only.
  - `366763a64` (palw-attempt-exec, http evidence providers, endpoints, node-free docs) and `321978444` (a test string) change node and tool code, not consensus.
- Within `247ade1f9`, the parts that matter for consensus below 17,000:
  - **`PALW_T12_INT13_DAA = Some(15_500)`**. Ported.
  - The deploy kit's `INT13_FLAG_DAY_DAA=15500`. Ported.
- Within `247ade1f9`, the parts that do not affect consensus below 17,000:
  - The `palw_source_economics_v1` field and `palw_paid_bootstrap_economics_v1.rs` are dormant. They are `None` on every preset and Some-only hashed, so the ids do not move.
  - The V6 receipt `observe_checker_economics_v1` call in `palw_claim_challenge_fold_v2.rs` runs only on the dormant V6 path.
  - The remaining changes are compile fixes against the `NativeRetainedV1` and `VirtualStateProcessor` APIs, plus RPC and session plumbing.
  - The Cargo change (`lto = false`, `codegen-units = 16`) affects build time only. `overflow-checks` stays on, and the identity test shows the params id does not depend on it.
  - **No patch is needed for the files Codex is editing.** That is why `fleet-patches/` was not created.

## 2. Commits

| SHA | What changed |
|---|---|
| `fa090ecae` | int-13 armed at 15,500. Also: the deploy kit copy, and the fleet's versions of 7 tests that this branch had not changed since `6549f9be2`. `t12_deploy_kit_constants` got the fleet's diff applied. |
| `9625c68dc` | `palw_t12_arm_after_int13_at_v1`. The preset's full-activation step and the v7 baseline no longer re-arm or clear the 4 int-13 entries. On the slice at 17,000, only the other 6 entries move. |
| (below) | `identity.rs`: handshake identity fix (§4), the identity test and script, and this report. |

## 3. Identity test against the fleet

`scripts/verify/t12-fleet-identity.sh [247ade1f9]` does the following:
1. Puts the fleet rev in a detached scratch worktree.
2. Copies in the dump part of `consensus/core/tests/t12_fleet_identity_247ade1f9.rs`.
3. Builds and runs it in both trees through buildslot.
4. Compares the `FLEETID` lines and then deletes the scratch worktree and both targets.

The lines compared are:
- the params id, the identity id and the schedule id;
- the fork id at DAA 0, 5,300, 15,000, 15,499, 15,500, 15,870 and 16,999;
- every armed fence below 17,000.

**Result: IDENTICAL.**
- params `ef379378f09bf33f29e9…e7f15e`: matches the release fingerprint named in `d2214e057`. `kaspad` logs it as `Consensus params fingerprint:`. It is a pure function of the params, so the debug and release builds compute the same value.
- identity `5de80e64…215bb5`: int-12's, unchanged.
- schedule `9ef2b1a7…c878f5`.
- schedule below 17,000: `[750 … 5585, 15500]`.

`fleet_ids_match_247ade1f9` pins these three values on this branch.

**Reproducing the release fingerprint:**
1. `cargo build --release -p kaspad` on `247ade1f9`, or on this branch.
2. Start it with `--testnet --netsuffix=12`.
3. The first log lines include `Consensus params fingerprint: ef379378f09bf33f…`.

You can also read it from `cargo test -p kaspa-consensus-core --test t12_fleet_identity_247ade1f9 -- --nocapture`.

## 4. A mismatch found and fixed: arming the slice moved the handshake identity

`slice_at_17000_keeps_the_handshake_identity` showed that arming `palw_kernel_route_panel_one_v1` at 17,000 changed the identity (`ed28c7df…` instead of `5de80e64…`). `for_each_fence` skipped this field with `: _`, so a future height was written into the handshake identity as it was. A 17,000 build would then have been **unable to peer with the fleet before 17,000**.

The same gap existed for four more fences on the full list: `palw_tir_bounded_door_v1`, `palw_tir_designated_answers_v1`, `palw_court_outsider_lane_v1` and `palw_court_clock_v2`.

The fix makes `for_each_fence` visit these five fields and collapses them in `normalize_values_a_scheduled_fence_drags_with_it` when they hold `Some(never)`. All five are `None` on the shipped presets, so the fleet ids stay as they were.

Not done: the full `consensus_params_id_tests` lib suite was not run. It should be run once at integration.

## 5. Checking that blocks below 17,000 are accepted and rejected as the fleet does

- **Rules:** the identity test above shows that every armed fence and its height below 17,000 match the fleet's, along with the params id that covers all values.
- **History:**
  - Sync from the public peers (read-only) with an empty datadir, and IBD to the live tip (about 15,870). The 15,500 crossing must contain no rule error, and the virtual must match the public `getBlockDagInfo` (`wss://wallet.misakascan.com/t12/wrpc/json`: tip hashes, `virtualDaaScore`, pruning point).
  - Alternatively, replay the public headers and history pages with the existing `docs/evidence/rfc/0012-public-history-*` procedure on this build.
  - Do not stop, swap or touch the DB of any public node.
- **P2P:** the protocol version is the same (108). Two behaviours exist only on this branch:
  1. txrelay `palw_relay_refusal_is_misbehaviour_v1` disconnects a peer that relays an H-1 carrier invalid at every tip.
  2. IBD and intake wait while the storage budget is paused.

  Neither is a consensus rule, but (1) could disconnect fleet peers. A short peering drill against the fleet should confirm it does not.

## 6. Remaining work and requests to the coordinator

- The slice release must arm the slice through `palw_t12_arm_after_int13_at_v1` (or keep the preset's `palw_t12_arm_full_activation_v1`). The drill functions `palw_t12_arm_full_activation_at_v1` and `palw_t12_arm_fence_list_at_v1` move the int-13 four as well.
- Rerun `scripts/verify/t12-fleet-identity.sh` after merging the other lanes. A new fence must be Some-only, visited by `for_each_fence` and collapse `never`, or it breaks this test.
- Run the history replay or IBD from §5 on the release build.
