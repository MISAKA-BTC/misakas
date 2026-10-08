# HFV: independent verification of the LIVE-R1 `ibd_candidates` deadlock hotfix

- Verifier: agent HFV (opus). I did not write the fix.
- Date: 2026-10-08.
- Candidate: `57235a5b3` "fix(flows): one read of ibd_candidates in consider_post_ibd_switch". Its parent `0b1c11b87` is the live testnet-12 release (`rcore/int-12`).
- Branch: `verify/hotfix-ibd-lock` = `57235a5b3` + `d15b8dcd6` (tests) + the commit holding this record. The second commit is test-only: an HFV test module appended to `protocol/flows/src/flowcontext/ibd_candidates.rs`.
- Nothing was deployed. No live host and no ssh were touched. No running devnet process was touched.

## Verdict

**The fix is correct, minimal, and complete for this bug class in the node. Recommendation: deploy `57235a5b3` now as a node-only kit `upgrade`, one host at a time.**

- **Correct.** The root cause is confirmed three ways:
  - **Source:** a `match` scrutinee keeps read guard #1 alive across read #2 of a non-reentrant parking_lot `RwLock`.
  - **Library:** in parking_lot 0.12.3, a non-recursive read refuses whenever `WRITER_BIT` is set.
  - **Shipped binary:** the symbolized D6 stacks plus disassembly show worker 1 parked in read #2 at `consider_post_ibd_switch+736` while holding read #1, and the relay writer in `wait_for_readers`.

  The fix takes the lock once per statement. Its result equals the old one in every state.
- **Scoped.**
  - Only `kaspa-p2p-flows` changes, and nothing below it in the dependency graph.
  - There is no consensus, params, wire or storage change.
  - The t12 ids equal the live ones (§1.1).
  - It is safe in a mixed fleet: peers cannot observe the change.
- **Reproduced before and after.**
  - The shipped expression deadlocks in about 50 ms under the real relay writes; the fixed form survives millions of calls.
  - A source guard bound to `ibd/flow.rs` fails on `0b1c11b87` (line 886) and passes on `57235a5b3`.
  - The author's own tests demonstrate the mechanism but would not catch a regression in `flow.rs`; the source guard fills that gap.
- **Complete.** A whole-workspace scan of 2,103 lock acquisitions found no other re-entrant acquisition of a non-reentrant lock in the node. The one nested read (`pruning_lock`) is on the readers-first `RfRwLock`, which is built for it.
- **The risk, corrected.**
  - **Ready nodes:** a plain restart of a node that is `Ready` and has completed a review **does not** reach the deadlock. It restores `Ready`, and its catch-up `Sync` IBD returns to `Ready`, as observed on devnet B at 19:31.
  - **Exposed nodes:** every *review episode* is exposed. That means a node that has never completed a review (fresh or wiped appdir, or one that has only synced by relay since genesis) at its next restart-with-IBD, every new joiner, any outage longer than the pruning depth, any quarantined node, and fork/partition recovery.
  - **Measured rate:** 1 hang in 8 old-binary review episodes on disk (about 12 %, 95 % CI 0.3–53 %). A hung node stays hung until someone restarts it, and its producer and seat stop meanwhile.
  - **Exposure window:** until the single full-RFC release, which is weeks away with arming blockers open.
  - **The upgrade restart itself is safe from this bug,** because it starts the fixed binary.

## 1. Scope

`git diff --stat 0b1c11b87 57235a5b3`:

```
 protocol/flows/src/flowcontext/ibd_candidates.rs | 108 +++  (one accessor + its tests)
 protocol/flows/src/ibd/flow.rs                   |  16 +-   (the trigger statement)
```

- **Changed:** only `kaspa-p2p-flows`. There is no change to a consensus crate, a params file, a wire or protobuf message, a storage schema, `Cargo.toml`/`Cargo.lock`, or a `build.rs`.
- **Production change:** one statement in `IbdFlow::consider_post_ibd_switch`. The new accessor `IbdCandidateRegistry::proof_validated_claimed_tip_work` is a pure `&self` method.
- **Behaviour:** the old and the new forms return the same value in every validation state. `hfv_the_fixed_trigger_answers_what_the_shipped_one_did` checks this on both commits. The old two-read form could not observe two different states either, because its first guard excluded every writer.
- **Dependency direction:** `cargo tree -p kaspa-consensus-core` does not contain `kaspa-p2p-flows`. The flows crate is consumed only by `kaspa-rpc-service`, the gRPC/wRPC servers, `kaspad`, the stratum bridge and the integration tests. Nothing that computes a params, schedule or identity id can see this change.
- **Source digests:** no `build.rs` or identity computation hashes `protocol/flows` sources.
- **t12 ids:** see §1.1.

### 1.1 `scripts/t12-repin.sh --drift-only` on `57235a5b3`

**Pinned at `57235a5b3`:** byte-identical to `0b1c11b87`, because `git diff 0b1c11b87 57235a5b3 -- consensus kaspad contrib scripts rpc protocol/p2p` is empty. These are the live ids:

| id | value |
|---|---|
| params | `5ee7fd8ee019968cf52929b844cf9ddfb1aad500842a89cf04bced8ba4edefb6` |
| identity | `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5` |
| schedule | `1678e07359f6727e96224041450a3b1d2aadcf8acd6bb6db0c277ff4d401c9b9` |

Source: `T12_RELEASE` in `consensus/core/tests/palw_anchor_at_ceiling_is_t12_only.rs:41`. The same triple is in the other `*_is_t12_only` pins. The params and schedule ids match the brief's `5ee7fd8ee019968c…` and `1678e07359f6727e…`.

**Computed side.** `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0 scripts/t12-repin.sh --drift-only` was started on `57235a5b3` ("t12-repin: tree 57235a5b3611"). **I stopped it during the harvest build of `kaspa-consensus-core`,** when free disk fell to 11 GiB (other lanes were writing too), under the session's stop-below-12 GB rule. It had printed no rows yet.

The ids cannot differ from the pins, for a structural reason. The computed values come from `kaspa-consensus-core`'s own tests, and that crate's dependency closure does not include `kaspa-p2p-flows`, the only crate this diff touches. So the harvest build of `57235a5b3` compiles exactly the sources of `0b1c11b87`'s harvest, which produced these pins.

**Status: verified by LIVE-R1, cross-checked by the dependency graph.**
- **LIVE-R1's run:** `scripts/t12-repin.sh --drift-only` on `hotfix/int-12-ibd-candidates-lock` @`57235a5b3`. The Lead relayed the result: **no drift, 361 ok**, params `5ee7fd8ee019968c…`, schedule `1678e07359f6727e…`, manifest `9def81a1…`.
- **My cross-check:** the dependency argument above. `kaspa-consensus-core` cannot see `kaspa-p2p-flows`, and the diff outside `protocol/flows` is empty.
- **Not done by me:** the Lead decided against resuming my interrupted harvest build.

## 2. Root cause, re-derived

### 2.1 The code at `0b1c11b87`

`protocol/flows/src/ibd/flow.rs:886`:

```rust
let claimed_tip_work = match self.ctx.ibd_candidates().read().get(&id).map(|c| c.validation) {
    Some(CandidateValidation::ProofValidated { .. }) => {
        self.ctx.ibd_candidates().read().get(&id).and_then(|c| c.claimed_tip_blue_work())
    }
    _ => None,
};
```

A `match` scrutinee is not a temporary scope. Its temporaries, including the `RwLockReadGuard` from the first `read()`, live to the end of the whole `match`, in edition 2024 as well. So the arm's `read()` is a second shared acquisition by a thread that already holds one. `ibd_candidates` is `Arc<parking_lot::RwLock<IbdCandidateRegistry>>` (`flow_context.rs:590`).

**Writer path.** `FlowContext::expire_stale_verifications` (`flow_context.rs:1308`) does `self.ibd_candidates.write()`. It is called from:
- every relay flow's idle poll, `HandleRelayInvsFlow::poll_for_candidate_summary` (`v7/blockrelay/flow.rs:444`), every 5 s per peer while participation is withheld;
- the commit barrier (`ibd/flow.rs:1339`).

The same poll also takes `observe_peer` and `claim_summary_request` writes. The relay flow's inv path takes `observe_ibd_candidate_peer` writes on every inv while participation is withheld.

**Reader path.** `consider_post_ibd_switch` runs from `verify_challenger` once a proof validates, and from `reconsider_validated_candidates` on every peer's IBD-flow tick (`VALIDATED_CANDIDATE_RECHECK` = 3 s, `ibd/flow.rs:125`). Both callers return early unless participation is withheld and no IBD is running. A candidate stays `ProofValidated` after "not worth investigating", so while the node is withheld every peer's flow re-runs the trigger for every validated candidate every 3 s.

### 2.2 parking_lot semantics, in the locked version

`Cargo.lock` pins `parking_lot 0.12.3`, `lock_api 0.4.12` and `parking_lot_core 0.9.10`. In `~/.cargo/registry/src/*/parking_lot-0.12.3/src/raw_rwlock.rs`:

- **`read()`** is `lock_shared()`, which calls `try_lock_shared_fast(false)` and then `lock_shared_slow(false, None)`.
- **`try_lock_shared_fast` (l. 510):** `if state & WRITER_BIT != 0 { if !recursive || state & READERS_MASK == 0 { return false } }`. The comment there reads "We can't allow grabbing a shared lock if there is a writer, even if the writer is still waiting for the remaining readers to exit".
- **`lock_shared_slow` (l. 693):** the same condition, then the thread parks.
- **`lock_exclusive_slow` (l. 615):** step 1 grabs `WRITER_BIT` whenever no writer or upgradable reader holds it, even while readers are inside. Step 2 is `wait_for_readers`.

So, for a reader R holding a read guard, a writer W that sets `WRITER_BIT` and waits for R, and then a second non-recursive `read()` by R: the second read parks behind W, and W waits for R. That is a permanent deadlock. Only `read_recursive()` skips a pending writer, and the code does not use it.

### 2.3 The shipped binary deadlocked exactly there

- **Binary provenance:** the devnet r1 binary is `89ffb1fb7`, sha256 `6e9b1936…`. `git diff 0b1c11b87 89ffb1fb7` changes nothing in `consider_post_ibd_switch`, `expire_stale_verifications`, the relay poll or the registry (only a size cap in `ibd/flow.rs:2754`). The defect line was introduced by `53a723311` on 2026-08-09, so every t11/t12 release since then carries it.
- **Symbolization is exact:** the `__TEXT,__text` section of the pinned stripped binary `wh-h1-run/bin/89ffb1fb7/kaspad` and of the unstripped rebuild `wc-rj/target/release/kaspad` is byte-identical (same size `0x334b010`, same sha256 `d17ad45d…`). I symbolized every address of both `D6-sample*.txt` with `atos -l 0x1049ac000` myself.
- **The stacks** (both samples, 20:07:00 and 20:20:06, are identical): all 10 `tokio-runtime-worker` threads are parked:

| worker | frame | lock state |
|---|---|---|
| 1 | `IbdFlow::consider_post_ibd_switch::{closure}+736` → `RwLock::read` → `lock_shared_slow+876` | holds read #1, parked on read #2 |
| 1 | `HandleRelayInvsFlow::start_impl` → `FlowContext::expire_stale_verifications+140` → `lock_exclusive_slow+1280` → `wait_for_readers+676` | holds `WRITER_BIT`, waits for worker 1's read |
| 1 | `expire_stale_verifications+140` → `lock_exclusive_slow+792` | queued for `WRITER_BIT` |
| 7 | `IbdFlow::serve_pending_nomination+132` → `lock_shared_slow+876` | queued readers |

- **The parked read is the second one.** Disassembly of `consider_post_ibd_switch::{closure}` (`0x1012a24e4`):
  - **First read** (`0x…740`–`0x…774`): the inlined fast path. `casa` adds `ONE_READER`, `tbnz w8,#3` tests `WRITER_BIT`, and the slow path is called with `w1 = 0` (`recursive = false`).
  - **Between the reads:** `IbdCandidateRegistry::get` and the `ProofValidated` discriminant test.
  - **Second read:** `bl RwLock::read` at `0x1012a27c0`. Its return address `0x1012a27c4` is exactly `+736`, the frame in the sample.
  - **Releases:** read #1 is released at `0x…824`, after read #2's release at `0x…7fc`.

  So the parked thread holds read #1 while it waits in read #2. The disassembly of the shipped binary gives the same cycle as the source reading.
- **Other threads:** the consensus threads (`virtual-processor`, `pruning-processor`, header/body processors) are idle on their crossbeam input channels, not deadlocked. They starve because P2P is dead. Logging continued only from non-tokio threads (`[palw-host] memory at periodic` once a minute).

## 3. Reproduction, before and after

### 3.1 The author's regression tests, reviewed

- **`a_second_read_under_a_held_guard_deadlocks_behind_a_queued_writer`** demonstrates parking_lot's rule (`try_read_for` stands in for `read()`, through the same non-recursive slow path). It does not run the shipped trigger or `flow.rs`.
- **`the_trigger_is_answered_under_the_guard_a_queued_writer_waits_on`** shows that the accessor takes no lock (it is `&self`). That is true by construction.
- **`only_a_validated_proof_answers`** is a good semantic test of the accessor.
- **Gap:** none of the three fails if the double read is put back into `flow.rs`, or if `flow.rs` stops calling the accessor. They would all pass on `0b1c11b87` plus the accessor. They document the mechanism, not the fix.

### 3.2 HFV tests (`hfv_ibd_lock_verification`, on this branch; builds on both commits)

| test | what it runs | `0b1c11b87` + HFV module | `57235a5b3` + HFV module |
|---|---|---|---|
| `hfv_consider_post_ibd_switch_takes_the_registry_once_per_statement` | Reads `ibd/flow.rs`, `flow_context.rs` and `v7/blockrelay/flow.rs` (`include_str!`). Refuses an `ibd_candidates` guard in a match, if-let, while-let or for head whose body re-takes it, and a brace-free statement that takes it twice. | **FAILED**: "ibd/flow.rs line 886: `match` holds a guard on ibd_candidates in its head; its body takes it again at line(s) [888]" | **ok** |
| `hfv_the_source_guard_finds_the_shipped_shape` | The guard's teeth: finds the shipped shape, passes the fixed one and the block-scoped `let`, catches `(a.read().x(), a.read().y())`, ignores comments and strings. | ok | ok |
| `hfv_stress_the_shipped_trigger_deadlocks_behind_the_relay_writer` | The **verbatim** shipped trigger on 6 threads, the relay poll's real writes on 2 threads, on the real registry behind the real `parking_lot::RwLock`. Watchdog: no progress for 3 s, confirmed by another 7 s. | **DEADLOCK after 51 ms**, 3,129 calls, `WRITER_BIT` set | **DEADLOCK after 60 ms**, 8,485 calls, `WRITER_BIT` set |
| `hfv_stress_the_fixed_trigger_survives_the_same_load` | The same harness with the fixed trigger (accessor body inlined: one acquisition). | survived 20 s, 6,674,649 calls | survived 20 s, 3,961,296 calls |
| `hfv_the_fixed_trigger_answers_what_the_shipped_one_did` | Equal results in every validation state and for an unknown id, single-threaded. | ok | ok |
| `hfv_burst_model_shipped_vs_fixed` (ignored; a measurement) | Production timing shape: 8 IBD-flow threads (2 triggers each) and 8 relay-poll threads released by one `Barrier`, i.e. perfectly aligned. 10 trials × up to 2,000 bursts. | (not run: identical code on both sides) | shipped: **10 deadlocks in 10 trials**, after 0–4 surviving bursts (20 aligned bursts in total); fixed: **10,000 / 10,000 bursts survived** |
| author's `live_r1_reentrant_read_tests` ×3 | (§3.1) | (do not exist) | ok ×3 |

Commands:
- **`57235a5b3`:** `cargo test --offline --locked -p kaspa-p2p-flows --lib -- --include-ignored --test-threads=1 --nocapture hfv_ live_r1_reentrant`, 9 passed (`hfv-logs/m1-new-2.log`).
- **`0b1c11b87`:** the same HFV module appended to that commit's `ibd_candidates.rs`, then `… -- --test-threads=1 --nocapture hfv_`, 4 passed, 1 failed (the source guard), 1 ignored (`hfv-logs/m2-old.log`).

The logs are under `~/Downloads/MISAKA-wt-b/hfv-logs/`.

**How to read the results:**
- The stress pair runs the same expressions on both commits, because it carries its own copies. It shows that the shipped *expression* deadlocks in tens of milliseconds under the relay writer, and that the one-acquisition form does not, over millions of calls.
- The source guard is what ties those expressions to the production file. It fails on the live release and passes on the hotfix.
- The burst model is an upper bound. Perfectly aligned bursts on a loaded 10-core machine deadlock within a few bursts. In the field, alignment is to about ±200 ms, not µs, which gives about 1 % per aligned burst (§3.3).
- What remains unproven by any unit test is the full node under real I/O. The shipped binary's own stacks and disassembly (§2.3) cover that for the old side. For the new side, the guarantee is structural: one acquisition, whose guard is a `let`-initializer temporary dropped at the `;`, with no lock taken before that `;`.

### 3.3 Field record: every review episode on disk

I found every node log on disk that contains a `Chain participation held: reviewing the chain just adopted` episode with validated candidates. I searched `~/Downloads/MISAKA-wt-b` and `MISAKA-testnet`: 43 logs, 7 of them with review episodes. Every one runs a binary that carries the defect.

| node log | review episodes | trigger calls ("not worth investigating") | aligned bursts (3 s IBD tick and 5 s relay poll within ±200 ms) | outcome |
|---|---|---|---|---|
| fenceC-run/x9 (09-28) | 1 | 1,814 | 15 | completed |
| fenceC-run/g6 | 1 | 765 | 17 | completed |
| fenceD-run/x9 | 1 | 2,273 | 15 | completed |
| fenceD-run/g6 | 1 | 1,279 | 12 | completed |
| devnet-r1/B (10-08) | 2 | 1,318 | 30 | both completed |
| devnet-r1/D6 | 2 | 735 | 16 | **19:34 DEADLOCK** (at its 2nd aligned burst, 19:34:33.70); 20:35 completed |
| devnet-r1/reg | 1 (stopped by its operator at 60 s) | 280 | 5 | censored |

- **Totals:** 1 deadlock in 8 completed-or-deadlocked episodes, about 12 % per episode (95 % CI 0.3–53 %). That is 1 in 110 aligned bursts (about 0.9 % per burst, CI 0.02–5 %) and 1 in 8,464 trigger calls.
- **How the bursts line up:** after a restart every peer's flows start together. The 3 s IBD ticks and the 5 s relay polls stay phase-locked, so they coincide every 15 s, about 12 times in a 180 s floor. D6's last log lines, at 19:34:33.65–.71, interleave both bursts. Then the log goes silent.
- **What these numbers cover:** this one Mac, at load average 100–190 with heavy memory compression. Preemption inside the read #1 → read #2 window is more likely here than on an idle Linux host. Treat the rate as a field measurement of this hardware, not a fleet constant.


## 4. Completeness scan

### 4.1 Method

I wrote a scanner that works at text level with comments and string contents blanked. It takes every lock acquisition in the workspace: `.read() .write() .lock() .upgradable_read() .try_*() .blocking_*()` with no arguments, so parking_lot, std and tokio locks and the RfRwLock are all included. For each one it decides how long the guard lives:

- **head of a `match`, `if let`, `while let` or `for`:** the whole body;
- **`let g = …lock()` (optionally `.unwrap()`, `.await` or `?`):** the rest of the block, or up to `drop(g)`;
- **anything else:** the rest of the statement.

Inside that region it reports:
- **DIRECT:** the same lock key (the last field or accessor name) taken again;
- **INDIRECT:** a call to a function whose own body, or one of its callees' bodies (matched by name), takes the same key;
- **AWAIT:** an `.await` while a sync guard is alive.

**Positive controls.** The scanner finds the shipped defect in `git show 0b1c11b87:protocol/flows/src/ibd/flow.rs` (886 → 888). It also finds my verbatim copy of it inside the HFV test module.

**Coverage:** 2,393 `.rs` files, 250 with acquisitions, **2,103 acquisition sites**. `target/` was excluded.

The HFV source-guard test (§3.2) is the same rule, compiled into the flows test suite and limited to `ibd_candidates` in `ibd/flow.rs`, `flow_context.rs` and `v7/blockrelay/flow.rs`, the three files that touch the registry.

### 4.2 Every hit, with verdict

**DIRECT, production code (16 hits at `57235a5b3`, plus the defect from the scan of `0b1c11b87`):**

| site | verdict |
|---|---|
| `protocol/flows/src/ibd/flow.rs:886→888` (at `0b1c11b87`) | **The defect.** Absent at `57235a5b3`. |
| `consensus/src/consensus/mod.rs:828, 2148, 2152`; `consensus/src/pipeline/virtual_processor/processor.rs:1725, 18792` | Safe. `body_tips_store.read().get().unwrap().read()` takes two **different** locks: the store's outer `RwLock`, then the tips set's own lock. Upstream rusty-kaspa shape. |
| `database/src/item.rs:33→38` | Safe. The `if let … = self.cached_item.read().clone() { return … }` guard ends with its statement; the `write()` is in a later statement. |
| `mining/src/manager.rs:1290→1295` | Safe. Same shape: two separate statements. |
| `rpc/grpc/server/src/manager.rs:116→125` | Safe **in edition 2024**, which the workspace uses. `if let … = self.connections.read()… { … } else if self.connections.read()…`: edition 2024 drops if-let scrutinee temporaries before `else` (in 2021 this would be the same recursive read). The then-block calls `Connection::close()` (`connection.rs:427`), which only takes the connection's own `mutable_state` and sends a oneshot, so there is no relock of `connections`. |
| `kaspad/src/chain_participation_store.rs:94` | Not a lock. `self.item.lock().unwrap().read()`: the `read()` is `CachedDbItem::read`, a DB read under one std `Mutex`. |
| `bridge/src/share_handler.rs:1122, 1137, 1600, 1605` | Safe. `let x = *m.lock();` copies the value and drops the guard at the `;`. `if m.lock().is_none() { *m.lock() = … }` is a plain `if`, whose condition is a temporary scope. |
| `mining/src/testutils/consensus_mock.rs:73, 82, 96` (test utility) | Safe. Each match arm body is its own temporary scope. |

**DIRECT, test code:**
- `utils/src/sync/rwlock.rs:148–168`: the RfRwLock's own recursive-read test, by design.
- `ibd_candidates.rs` HFV module: my verbatim copy of the defect, intentional.

**INDIRECT, production code (81 hits by name).** I read every one:

- **Method on the guarded value or the guard variable (false positive):** most of the hits.
  - `mining/src/manager.rs` (`mempool.has_transaction` …)
  - `consensus/src/consensus/mod.rs` `pruning_point_read.pruning_point()` and `pruning_meta_write.set_…`
  - `pruning_processor`, `virtual_processor`
  - `mining/src/cache.rs`, `misaka-palw-pow-driver`, `misaka-palw-worker` (stdin)
  - `database/src/cache.rs`, `notify/src/address/tracker.rs`, `palw_heartbeat_relay.rs:116`, `misaka-palw-base0/src/qwen36.rs:462`
  - `recovery_trace.rs:152`, `verification_trace.rs:153` (`Vec::clear` on the guarded vec)
  - `core/src/core.rs:31`, `core/src/task/runtime.rs:44`
  - wallet `storage/local/interface.rs`, wallet `utxo/*`, `wallet/core/src/account/pskb.rs`
- **Call on another object, which the name matched but the code does not:**
  - `consensus/mod.rs` reachability and sync-manager calls under `pruning_lock`
  - `header_processor:339`, `virtual_processor:18785`
  - `consensus/core/src/palw_state_v2.rs:28988`: `builder.read()` is a state view, not a lock
  - `misaka-palw-tir-exec/src/node/residency.rs:660`: an atomic `load`
  - `components/consensusmanager/src/lib.rs:151, 199`: `ctl.clone().start()/stop()` spawn the consensus threads and never touch the manager's `inner`
- **Real nesting, safe by the lock's design:** `consensus/mod.rs:1997→2001`. `get_transactions_by_accepting_daa_score` holds `pruning_lock.blocking_read()` and calls `get_transactions_by_accepting_block`, which takes it again (l. 2058). `pruning_lock` is `SessionLock` = `kaspa_utils::sync::rwlock::RfRwLock`, documented as "Readers-first … this makes it safe to make recursive read calls". Its semaphore lets a reader in ahead of a queued writer, so this is not the parking_lot case.
- **Guard dropped or moved before the call:** `pruning_processor/processor.rs:208→278` (`RwLockUpgradableReadGuard::upgrade(pruning_point_read)`, then `drop(pruning_point_write)` before `prune()`). Also `misaka-palw-base0/src/inventory.rs:163/168→173` and `misaka-palw-tir-lower/src/lower/fill.rs:344→357`, where the `if let` guards end with their statements.
- **Plain `if` conditions** (a temporary scope), so safe: `bridge/src/client_handler.rs:76, 84` and `bridge/src/stratum_listener.rs:297`.
- **Different lock, fixed order:** `kaspad/src/palw_memory_ledger.rs:447, 528`. The pool ledger's std `Mutex` is held while calling the host ledger, which takes its own `local` `Mutex` plus `flock` and never calls back into a pool ledger. The order is always pool → host, so there is no inversion.
- **Async mutex in client libraries (not the node):** `rpc/wrpc/client/src/client.rs:441, 469, 481` (`connect_guard`/`disconnect_guard` held across `start()/stop()`, which do not take those guards) and `wallet/core/src/wallet/api.rs:301`.
- **`core/src/core.rs:43, 77` and `core/src/task/runtime.rs:80, 107, 122`:** `for service in self.services.lock().unwrap().iter() { service.start(core) }` holds a std `Mutex` during each service's start. That would self-deadlock only if a `start()` called `core.find()`. No production service does; only the integration tests call `find`, and from outside `start`.

**AWAIT, production code (8 hits):**
- `rpc/grpc/server/src/manager.rs:116/125`: the `sleep().await` comes after the guards' statements.
- The rest are CLI, wallet and benchmark code with tokio or async locks.

None of these is a parking_lot guard across an `.await` in the node. That would not compile in a `tokio::spawn`ed (Send) future anyway.

### 4.3 Specifically around `ibd_candidates`

I read all 29 acquisitions of `ibd_candidates` (13 in `flow_context.rs`, 16 in `ibd/flow.rs`). After the fix:
- Every one is a single-statement temporary or a block-scoped `let registry` that is dropped before any other acquisition.
- Inside the block-scoped guards (`flow.rs:500, 554, 702, 800, 1152, 1369`; `flow_context.rs:1314, 1357`) the only calls are to the pure registry, `record_stage` (a leaf std `Mutex`), and `chain_participation().state()` (atomics plus a meta-DB write under the store's own `Mutex`).
- No guard is held across an `.await`. parking_lot guards are `!Send` and the flows are spawned, so the compiler enforces this.
- No path takes `preferred_ibd_candidate`, `ibd_metadata` or `ibd_lease` while holding `ibd_candidates`, or the reverse (`flow_context.rs:1226–1448`), so there is no lock-order inversion.

**The author's claim holds.** The author scanned the other 12 scrutinee-held guards in flows, p2p, mining and rpc and called them safe. My whole-workspace scan agrees: no other instance of the class remains in the node.

**What a text scanner cannot see:** a guard reached through a type alias and relocked through a trait object, or callers that differ only in their method's receiver type. A type-aware second pass (`cargo clippy -- -W clippy::significant_drop_in_scrutinee`; lock_api 0.4.12 marks its guards `#[clippy::has_significant_drop]`) is worth running once the machine is idle. I did not run it here. It is a full check build, and the rule for this session was one build per milestone on a machine at load 100–190.

## 5. Risk re-evaluation

### 5.0 Quantified summary

**(a) Hang rate per review episode: 1 in 8, about 12 % (95 % CI 0.3–53 %).**
- **Source:** every on-disk node log with a review episode, all on binaries carrying the defect (§3.3). That is `fenceC-run/{x9,g6}` and `fenceD-run/{x9,g6}` (2026-09-28), plus `wh-h1-run/devnet-r1/{B,D6,reg}` (2026-10-08).
- **Count:** 9 episodes. 8 ran to completion or deadlock, and 1 (`reg`) was stopped by its operator after 60 s and is excluded. One hang: D6, 19:34:33.
- **Equivalent rates:** about 1 in 110 aligned 3 s/5 s bursts, and 1 in 8,464 trigger calls.
- **Hardware:** one Mac at load average 100–190. The rate on idle Linux fleet hosts is unmeasured and probably lower, but not negligible. A writer spinning in the same burst sets `WRITER_BIT` right after the reader gets in.

**(b) How often live t12 nodes enter review.** One episode lasts at least 180 s, longer while a decision is pending, and indefinitely in quarantine. I have no access to live state, so the table gives the rule per trigger and the evidence for each.

| trigger | episodes | evidence |
|---|---|---|
| Fresh joiner, or a re-synced, wiped or moved appdir | **1 per join, every time.** `ever_ready` starts false. | code; devnet B and D6 at 20:31–20:35 (datadirs moved aside) both went into review |
| Restart of a node that has **completed** a review (`ever_ready = true`) | **0.** It restores `Ready`; its catch-up IBD is a `Sync` and returns to `Ready`. | code; devnet B at 19:28 → `Sync` IBD at 19:31:38 → no review |
| Restart of a node that has **never** completed a review | **1 per restart that triggers an IBD.** That is any downtime longer than the orphan-resolution range (a few blocks), so in practice every upgrade or crash restart, until its first review completes. | code. On devnet r1, 7 of 9 nodes (A, C, D1–D5) started together and never ran an IBD in about 5 h and 2 starts each, so all 7 are still `ever_ready = false`. Fleet nodes that have synced only by relay since the t12 genesis are in the same position. `ever_ready` is not in any RPC; it is in the meta DB, or visible as a completed "reviewing the chain just adopted" countdown in a node's own log. |
| Outage longer than the pruning depth (74,920) | 1 per outage (headers-proof IBD → staging commit → review) | code; rare |
| Quarantine | continuous for as long as it lasts: about 4 aligned bursts a minute | code; incident-driven, rare |
| Fork, partition or reorg recovery | 1 per affected node, **correlated** across nodes | devnet r1 reorg run (D6) |

**Expected hangs on the old binary** ≈ ⅛ × (joins + IBD restarts of never-reviewed nodes + long outages), plus quarantined nodes, which hang with near certainty within hours. The window lasts until the full-RFC release, which is weeks away.

**(c) Impact of one hang on a fleet host.**
- **The node:** the kaspad process stays alive at 0 % CPU, and systemd does not restart it.
  - Every async worker that touches the registry parks: all 10 of 10 on D6, and the VPS profile runs only 2. P2P, block relay, RPC and the 10 s status line stop.
  - Consensus threads idle on empty channels, so the DAA freezes.
- **Miners and seats:** **the miners and seats served by that kaspad stop.** The producer stops mining, and the seat's attestations, precommits and panel duties stop, until an operator restarts it.
- **The rest of the fleet goes on.** Peers drop the node within about 2–4 min (ping every 120 s, 120 s timeout). The hung node holds nothing the others wait on. Evidence: at 20:20, B had no D6 in its peer list, and A and B kept accepting blocks.
- **Visible symptoms:** RPC timeouts or a frozen `virtualDaaScore`, a dwindling peer list, and only periodic non-tokio log lines.
- **Recovery:** one restart. The node comes back `Ready`, because the persisted review floor has expired and the pending decision is not persisted.

### 5.1 When does the vulnerable path run?

The trigger's reads are reachable only while `is_consensus_participation_allowed()` is false **and** `is_ibd_running()` is false. `ChainParticipationGate::state()` is `Ready` → allowed, `IbdRunning` → guarded out, `CandidateReview` or `Quarantined` → reachable. A writer is always present in exactly those states, because the relay poll runs only while participation is withheld.

The gate enters `CandidateReview` only through `FlowContext::finish_ibd_after_success`, and only when the IBD that just finished replaced the active consensus or the node has never been Ready (`replaced || !ever_ready`, `flow_context.rs:1877`). It is restored as `CandidateReview` from the meta DB when the process died in review, or mid-IBD before the staging commit (`chain_participation.rs:186`). `Quarantined` is restored as well.

| restart of a t12 node | reaches the trigger? | evidence |
|---|---|---|
| **Ready node** (`ever_ready = true`), any downtime below the pruning depth | **No.** It restores `Ready`. A catch-up IBD is a `Sync`: `enter_ibd` → `IbdRunning`, where every caller returns early, then `release_after_noop_ibd` → `Ready`. | devnet-r1/B 19:28 restart → `Sync` IBD 19:31:38 → **no review**, in a node that completed a review at 18:50 |
| **Node that has never completed a review** (`ever_ready = false`): fresh or wiped appdir, or a node that has synced only by relay since genesis | **Yes**, at its first IBD after the restart. An IBD follows any downtime longer than the orphan-resolution range, so most restarts trigger one. That IBD leads to a 180 s `CandidateReview`, held longer while a decision is pending. | D6 19:31 (never IBD'd since 15:54) → `Sync` IBD → review → deadlock 35 s in. B and D6 at 20:31 and 20:33 (datadirs moved aside, fresh) → review |
| **Long outage** (headers-proof IBD; the t12 pruning depth is 74,920) | **Yes.** The staging commit replaces consensus, which leads to review. | code (`flow.rs:1675`, `flow_context.rs:1879`) |
| **Restart while in review, quarantine, or mid-IBD before the commit** | **Yes**, with the restored state. A review whose floor has expired with no decision pending promotes to Ready at the first `state()`. | code |
| **Quarantined node** | **Yes, without end.** The 3 s and 5 s loops run for as long as the quarantine lasts, so a hang is close to certain over hours. | code |
| **Fork, partition or reorg recovery** (nodes adopt a chain by headers proof, or quarantine) | **Yes, on several nodes at once**, correlated with the event the review exists for. | devnet r1 reorg run |

**Peer candidates.** Do proof-validated peer candidates typically exist during a review? Yes, even when every peer is on the node's own chain. Each peer's summary becomes a candidate (keyed by pruning point and tip), its proof validates within seconds, and it then sits at `ProofValidated` with "not worth investigating … comparison=Equal". D6 had two validated candidates 4 s into its review, and fenceC/fenceD x9 validated 8–10.

**The "143 s floor"** in the D6 log is `POST_IBD_CANDIDATE_REVIEW` = 180 s counting down. The node hung at 19:34:33.7, about 35 s into the floor. The 19:34:38 status line was the last one the 10 s tick printed before the runtime starved.

### 5.2 Probability per restart

- **Ready node:** about 0. The path is not reached (code, and B at 19:31).
- **Node without a completed review, or any of the other entries above:** about 1 in 8 (12 %) per episode on the loaded Mac (§3.3). On an idle Linux host the window is narrower. The race needs a writer to set `WRITER_BIT` in the few hundred ns between read #1 and read #2. In an aligned burst, though, a writer is often already spinning for the lock. It takes `WRITER_BIT` right after the reader gets in, so the rate does not fall to the per-ns figure.
- **Fresh joiners** (public t12 operators, new seats, re-synced hosts): every first sync is an episode, so each joiner has about a 1-in-8 chance on this evidence.
- **Upgrade restart onto `57235a5b3`:** the post-restart review, if any, runs the fixed code. **Deploying the fix does not expose a node to this bug.**

I could not see the live nodes' `ever_ready` without access. It is not in any RPC; it is only in the meta DB (`ChainParticipationStore`). An operator can infer it from a node's own log: a completed "reviewing the chain just adopted" countdown since its appdir was created means `true`.

The live-race devnet measurement (2–3 nodes, repeated restarts) was **not run**:
- **Load:** the machine stood at load average 100–190 with 15 GiB compressed memory.
- **Builds:** it needs a release build of `57235a5b3`.
- **H1:** it needs H1's 9B conversion paused.
- **Coverage:** the on-disk record above already gives 8½ old-binary episodes with this exact code.

It remains a cheap follow-up on an idle machine. The scenario to drive is the one that reaches the path, a node whose datadir is wiped between restarts, not a plain restart of a Ready node, which never reaches it.

### 5.3 What a hung node looks like on live t12

**The node.** The process stays up at 0 % CPU and systemd does not restart it.
- **Runtime:** every tokio worker that touches the registry parks. The runtime has `--async-threads` workers, the core count by default and **2 under the VPS-8GB / sync-only profile**. Parked workers include every peer's IBD flow on its next tick, every relay poll, the nominations, and the deadlocked pair. With 10 workers (D6), all 10 parked within the burst, which stopped RPC, P2P, the producer, the panel's async work and the 10 s status tick. On a many-core host, RPC may keep answering while block relay is frozen.
- **Consensus threads** are idle on empty channels, not wedged: they starve because P2P is dead. **The DAA stops.**
- **The log** shows only non-tokio lines, such as `[palw-host] memory at periodic` once a minute.

**The rest of the fleet** continues. Peers drop the hung node by the ping flow (120 s interval, 120 s timeout, `v7/ping.rs:49`). At 20:20, B's peer list no longer contained D6 (`B-getConnectedPeerInfo.json`), and A and B kept accepting blocks. The hung node holds no consensus lock that others need, so nothing propagates.

**Miners and seats on that host stop.** Its producer and its seat's duties (attestations, precommits, panel duties) stop until an operator restarts it. During a panel round that is missed duty time for that seat.

**What the operator sees:** RPC timeouts or a stuck `virtualDaaScore`, `getConnectedPeerInfo` emptying, and no log lines other than periodic ones. The kit's `upgrade` gate (synced with ≥ 1 peer and a new block within `UPGRADE_SYNC_TIMEOUT` = 600 s) would catch it during a rollout and stop there.

**Recovery is a restart.** The persisted state is review with an expired floor, and `decision_pending` is not persisted, so the node comes back `Ready` and promotes immediately. That restart is itself a restart on the old binary, but it no longer takes the review path.

## 6. Recommendation

### 6.1 Deploy now (recommended)

**How:**
1. Build the Linux release from `57235a5b3` with the kit's reproducible builder (`build-release-local.sh`), and record the new `EXPECT_SHA`.
2. Run kit `upgrade` with `UPGRADE_FROM_FP` empty, since this is node-only and `EXPECT_FP` is unchanged. Go one host at a time.
3. Let the kit's gates run: fingerprint and genesis, DB sentinel, and synced with a new block within 600 s.

**What to expect:** nodes that have never completed a review will spend 180 s in `CandidateReview` after their post-upgrade IBD. They do not mine or attest during it. This is safe on the fixed code and is normal gate behaviour, not this bug.

**Residual risks:**
- **One routine rolling restart:** minutes of downtime per node and its seat. This is mitigated by host-by-host gating.
- **A new binary artifact:** the build must be the kit's reproducible build of exactly `57235a5b3`.
- **Forward-porting:** **the fix must also land in the integration tree that becomes the full-RFC release.** Otherwise that release reintroduces the deadlock. Merge `57235a5b3` (and preferably the HFV source-guard test from this branch) there.
- **Unknowns:** anything this change does not touch, and other latent bugs on the restart path, are the same risks any restart carries.

### 6.2 Wait for the full-RFC release

**Residual risks:**
- **Per-episode hang:** each old-binary review episode on a live node has about a 1-in-8 chance (measured on loaded hardware, wide CI) of a permanent hang, until an operator notices and restarts it. systemd will not restart it, because the process stays alive.
- **Duration:** weeks.
- **Joiners:** public joiners and re-synced or moved nodes hit it on their first sync.
- **Quarantine:** a quarantined node is exposed continuously.
- **Correlated failures:** a fork or partition recovery puts several nodes into review at once, so hangs would correlate with the event the review exists for.

**Mitigations if waiting anyway:**
- Avoid unnecessary restarts of nodes not known to have completed a review.
- Alert on "process alive, RPC timing out or `virtualDaaScore` stuck".
- Restart a hung node. It comes back `Ready`, because the persisted review floor has expired and `decision_pending` is not persisted.
- Publish the fixed binary for joiners regardless.

### 6.3 Mixed fleet

Safe. The change is local lock usage inside one function. Messages, validation, consensus, persistence and every id are unchanged, so old and new nodes peer exactly as before. An old node keeps its own exposure until it is upgraded.
