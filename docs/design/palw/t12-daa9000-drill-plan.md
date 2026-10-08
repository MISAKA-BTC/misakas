# testnet-12 DAA 9,000 (int-13) — drill plan, rollout and rollback

Status: **plan, 2026-10-08** (`release/t12-daa9000`, agent R9). **Nothing in this file has been run.** It describes a drill on a Mac, not a
change to any shared host. The release candidate is the tree whose commit carries the re-pin (`re-pin:`); every id below is read from that
build, never typed. Long builds and the drill itself are scheduled by the lead after the code freeze (about 10-10).

**Candidate ids at the re-pin commit `f754719df`** (to be re-read from the release build, and re-pinned if the list changes before the freeze):
params `2e56764257fe24888a7aab4f109b6ef83fa6ac5000dc47d1369398e5959cbc1f`, schedule
`5f5df8177e77fdf4e690222a690a0b5875f66d4e722d81159273435d754a8c5f`; identity (`5de80e64…`), genesis (`a27f8f44…`), premine txid (`5e0d5f1b…`) and the rule
manifest digest (`9def81a1…`) do not move.

Related: [`int-11-drill-flags.md`](int-11-drill-flags.md) (the frozen drill flag set, rows 1–19), `audit-combined/dc.sh` (the combined drill
harness of the DAA-5,300 candidate, which this plan extends), `contrib/t12-deploy-kit/DAA750-ROLLOUT.md` (the rollout runbook this plan
follows), `consensus/src/pipeline/virtual_processor/tests/t12_int13_flag_day_crossing.rs` (the same crossing at the processor).

## 1. What is armed at DAA 9,000, and what that means for the drill

`PALW_T12_INT13_FENCES_V1` at `PALW_T12_INT13_DAA` = 9,000 — one height, four fences, all code changes whose prerequisites are in force on
testnet-12 already (the range twin stands on the int-11 list's `palw_gen_v1` at 5,300):

| # | Fence | What changes from the fence | Evidence before this drill |
| --- | --- | --- | --- |
| 1 | `palw_audit_1004_v1` | the 2026-10-04 audit's consensus fixes (P-F1, P-F4, B-F3, B-F4, C-F1/B-F6, S-1/B-F5, RF-1, RF-3/P-F5, RF-4, G-3, G-2, C-F4 — `consensus/core/src/palw_audit_1004_v1.rs`), plus the node-side duty that rides it: a seat proves **every root** a class has in force (ADR-0173; `kaspad/src/palw_panel.rs` `root_keyed`) | unit and processor tests per rule; **no full-chain drill** |
| 2 | `palw_gen_range_twin_v1` | a generative class's closes are sized with the range twin instead of the element twin (same read sets, same bounds, fewer steps of the same 2^26 cap) | unit tests (`palw_gen_close_price.rs`: the range twin sizes every close exactly as the element twin); **no full-chain drill** |
| 3 | `palw_model_court_window` | a class registered from the fence stores its own court window (delta entry 100, `ClassCourtWindow`); on the held clock it equals the network window for every admissible class | `palw_t12_court_window_changes_no_admission.rs` (no verdict moves); the delta position is the same on every node of the int-12 line; **never armed anywhere before** |
| 4 | `palw_receipt_spend_v4` | RFC-0009 stage C: a winning quantum of a `Final` free-prompt claim may be spent by any bonded builder on the executor's signed `RDA4` authorization; the `PFS4` header carriage cap (14.9 KB) opens for `PFS4` payloads only; the coinbase splits the worker reward miner leg / builder fee | processor tests (`rfc9_redemption_v4.rs`) and the coinbase expression; RFC-0009 spec §8: "dormant; **no full-chain drill**" |

Fences 1, 3 and 4 have never run on a multi-node chain, and 2 has run only in unit tests: **this drill is the first multi-node evidence for all
four.** Fence 4 is the least exercised (it adds a header form, a producer mode, a coinbase split and a new carriage); if it cannot be driven
end to end in the time available (§3, fence 4), the lead should decide on dropping it from the list before the freeze (one line in
`PALW_T12_INT13_FENCES_V1` plus the re-pin) rather than carry an undrilled consensus change into a public flag day.

## 2. The drill

### 2.1 What runs

* **The shipped binary** — a `--release` build of the candidate (`contrib/t12-deploy-kit/build-release-local.sh <commit>`), `kaspad`, `misaka`,
  `palw-class` and `misaka-palw-fp-rail`, with the build's `BUILD-INFO` beside them. Not a debug build: a drill that crosses a fence "with a
  different binary from the one that ships" proves nothing (memory rule: *the drill must run the binary you are shipping*). The drill harness
  checks the binary's public ids against `BUILD-INFO` (`dc.sh dry` does) and **the manifest's `public_consensus_params_id` is the re-pin's**.
* **One salted testnet-12 chain** on the Mac (`--palw-drill-genesis-salt=<64 hex>`; never on a shared host, never on public testnet-12's
  genesis). Same shape as `dc.sh`: nodes `new0` (floor + heartbeat clock), `new1..new3` (seats), `new4` (head, REAL producer), `new5`
  (evaluator), `new6` (evaluator + REAL producer with the 8k emulation `--palw-drill-real-submit-delay-s`), `new9` (outsider seat, a
  post-genesis bond), the old relay (see below), a late joiner. **`--ram-scale` on the seats** — a drill's own nodes starve the panel they
  test (memory rule); watch `top -o mem`, and stop at < 15 GB free.
* **Two extra roles for fence 4:** an executor with a free-prompt claim (`misaka-palw-fp-rail --redeem-auth-out`) and a **builder node**
  (`--palw-redemption-auth-dir=<dir>`) on a bond of its own — a post-genesis bond, like the outsider's.
* **The old relay:** the fleet's current release (**int-12**: `rcore/int-12`, params `5ee7fd8e…`, schedule `1678e073…`; `OLD_KASPAD_BIN`)
  joined to the drill chain below the fence. It must be the *same* int-12 binary the fleet runs, not the baseline rebuilt from this tree.

### 2.2 Flags and heights

The drill ruleset arms the whole release at the release heights (a drill drills what ships), so every drill flag **moves** an armed entry.
The layout extends `int-11-drill-flags.md` §3 / `dc.sh` (`INT11_AT=26`, ρ = 100 / 250 / 1000 at 121 / 216 / 311):

```
kaspad --testnet --netsuffix=12 --nodnsseed --palw-drill-genesis-salt=<64 hex> \
  --palw-drill-fence-at=6 --palw-drill-fence2-at=10 --palw-drill-fence3-at=14 \
  --palw-drill-tir-at=16 --palw-drill-tir2-at=24 --palw-drill-int11-at=26 \
  --palw-drill-int13-at=110
```

* `--palw-drill-int13-at=H'` moves the four entries together, on a copy, through each entry's own `set` (mirrors included). It is
  refused by name without the salt, at 0 / `never`, at a height another fence uses, beside `--palw-drill-model-court-at` (a second move of
  one of its entries), and **while `palw_gen_v1` is above it** — the range twin stands on the int-11 list, so `--palw-drill-int11-at` goes
  first and lower. It is applied after `--palw-drill-int11-at` in `PalwDrillExtraFencesV1::apply`.
* **H' = 110** is a proposal: after the first REAL attempts (about DAA 90, `dc.sh`) so that claims are in flight across the fence; before
  ρ = 100 at 121 so the capacity regime is the same on both sides; distinct from every other height of the layout (6, 10, 14, 16, 24, 26,
  121, 216, 311). The driver re-checks distinctness (the node refuses a collision by name).
* The datadir marker records `int13_at=` (a stored chain is never reopened under another height); the keyring manifest names `int13_at`
  and announces the fingerprint the node on the same command line announces — pass the same flags to `--palw-drill-write-keyring`.

`dc.sh` / `dm.sh` need one change to carry the flag (a pass-through `INT13_AT`, appended after `--palw-drill-int11-at`, and `--palw-drill-int13-at`
in the `extra_checks` flag list). The existing int-11 gates keep their meaning; the int-13 legs below are added to them.

### 2.3 How long

At the measured ~125 s per DAA (~29 DAA/h) on the Mac: H' = 110 at about 3.8 h after the chain starts; the crossing gates are final ~30 DAA
(about 1 h) after H'; the PANEL window that straddles H' needs ~32 DAA either side. **Budget 6 h for the crossing legs** (fences 1–3,
cross-cutting gates). Fence 4's leg needs a free-prompt claim made before H' to reach `Final` and then `receipt_maturity` before it can be
redeemed (`Final` is ~200 DAA after the claim in the previous drills): about **+7 h** after the claim, so the claim goes in at the start of the
drill and the full run is **~12 h**. The ρ steps after H' (121 / 216 / 311) are not part of this drill's verdicts; stop after the V4 leg.
Memory: 32 GB shared, load high — one drill at a time with no other build or drill running, `top -o mem` sampling
every minute, and the watchdog `memwatch.sh` (kills a process at 12 GB).

## 3. What to observe, per fence, and the pass criteria

Notation: **before** = blocks below H', **after** = H' and above. A gate PASSES only if both hold: the observed behaviour, and *no honest object
refused after H' that was accepted before* (grep every node's log for the fence's refusal strings; the list is §3.5).

### 3.1 Fence 1 — `palw_audit_1004_v1`

* **PANEL straddles the fence** (`dcwatch.py panel`, a window `[H'−16, H'+16)`): bind→licence p50 ≤ 6 and p95 ≤ 12 DAA both sides, the
  oldest seat wait ≤ 40, `PanelUnavailable` expiries 0 — **unchanged within the noise of the windows before it**.
* **Root-keyed possession proofs** (the node-side duty): after H' every seat holding a class proves its *registered* root as before **and**
  every other root the class has in force (a line's current version, a preview, a superseded root inside its grace). Observe per seat:
  proofs submitted per `(class, root)` (`palw_readiness_escalation` memo keys, `[palw-panel]` readiness lines), `no proof — <why>` notes
  (capacity), `readySeatsNow` per class. **PASS:** `readySeatsNow` for the 8k class ≥ its pre-fence value; no class goes `HELD` for
  `no_capable_panel`; the extra proofs' CPU/RSS per seat is recorded (the 5.104 host ran its 5 seats over-committed in the 10-01 incident —
  this duty is new work per seat at the fence, so the number matters to the host layout, not only to the verdict).
  *Needs:* a class with ≥ 2 roots in force at H' (register a second root of a line before H': `palw-class` / `misaka model add` of the new
  version), otherwise this part is not exercised and the verdict is "not drilled".
* **The refusals are for the dishonest only:** with the drill's honest producers, panel, registrar and improvement driver running across H',
  there is not one refusal line naming an audit-1004 rule (§3.5). A deliberately dishonest probe after H' (one per cheap rule that has an
  object a script can build: a single-tile trap commitment, a `CandidateSubmitted` from a non-registrant bond, an improvement fee below the
  floor) is **refused by name after H' and accepted before** — the pair is the evidence that the fence is live on a real chain. Rules with
  no cheap object (S-1, C-F1, B-F4, B-F6) stay processor-test-only; say so in the verdict.
* **State that straddles the fence:** open improvement epochs, open trap commitments and tensor claims whose lifetime spans H' keep
  folding (their state was written under the old rules): record the count of each open at H' and their resolution afterwards; **PASS:** none stuck.

### 3.2 Fence 2 — `palw_gen_range_twin_v1`

* A generative class (`toy-image`, `toy-embed`, `wide-embed`, `sd3-tiny` — `rfc3-gen-drill-inputs.md`) registered **before** H' and one
  **after** are admitted on both sides with the same verdict and the same priced closes (the range twin sizes the same read sets).
* If the census's vision-language class is available as a drill fixture (the 196-px Qwen2/2.5-VL slot that the element twin refuses at the
  2^26 cap — `misaka-palw-sdk/src/census/vlm.rs`): **refused at the cap before H', admitted after** — the one verdict the fence is meant to
  move. If it is not available, the verdict for this leg is "equal on the toy classes, VLM not drilled".
* **PASS:** no class's admission verdict moves except by the cap relief; no `GenAdmission` refusal after H' that was accepted before.

### 3.3 Fence 3 — `palw_model_court_window`

* A class registered **after** H' stores a `ClassCourtWindow` (delta 100); a class registered before keeps the network window. Read through
  the node's class RPC / `kaspad --palw-dump-classes` (the drill driver prints the windows).
* **PASS:** every after-class window equals the network window (the held clock: `window_court`, 3,000 DAA on public testnet-12 — the
  drill's own value is read from the node), no admission verdict differs from the same registration judged before H' (register the same
  corpus classes on both sides: `g14` corpus cases), and the delta entry at position 100 decodes identically on the old relay (below
  H' only — the old relay is refused at H', §3.4).
* Also compare a restarted node's state root and a node IBD'd across the fence (§3.4): the stored window must replay to the same root.

### 3.4 Fence 4 — `palw_receipt_spend_v4`

The lane has no full-chain evidence; this leg is the one that can fail the drill by being impossible to drive, so it is built first.

1. **Setup (at the start of the drill, DAA < 40):** an executor (`misaka-palw-fp-rail … --redeem-auth-out <dir> --bond-key-seed …`) makes a
   free-prompt claim on a certified free-prompt class (V5/V3 path as in the existing FP drills) and publishes its `RDA4` authorization
   (position-free: the claim, the executor bond, a quantum range, the beacon *rule*, a builder fee ≤ 10 %, an expiry). A builder node
   (`new7`, bond of its own, `--palw-redemption-auth-dir=<dir>`) is running.
2. **Before H':** the builder's V4 spend (the `PFS4` header) is **refused at the header stage by name** (`not valid below palw_receipt_spend_v4`);
   the V3 path (the executor itself spends) is unaffected.
3. **After H', once the claim is `Final` and `receipt_maturity` has run and the beacon draw selects a winning quantum:** the builder
   produces the receipt block; the chain accepts it; the coinbase pays `miner leg → the executor bond's registered payout` and
   `builder fee → the block's own miner script`, and their sum equals what V3 pays for the same block (no issuance, panel leg, reserve or
   maturity moves).
4. **PASS:** (a) refusal before, acceptance after, both by name; (b) the split sums to the V3 amount to the sompi; (c) single use: a second
   spend of the same `(claim, quantum)` — V3 or V4, same or sibling block — is refused (B-5's dedup); (d) every other node (seats, evaluator,
   the late joiner IBD'ing across H') folds the same root with the receipt in; (e) a `PFS4`-sized header (≈ 14.9 KB) is carried by no block
   before H' and by the receipt block after, and a **non-`PFS4` header above 8,192 bytes is refused after as before**.
   **Failure to reach step 3 inside the budget is a no-go for fence 4** (§1): record how far it got.

### 3.5 Cross-cutting gates (every fence, one chain)

* **DAA** — a block at every DAA through the crossing, no slot gap over 4 slots, in every state (`dcwatch` DAA/STATES).
* **SPLITS** — every node's sink sampled every 10 s; every split H2-class and self-converged within 3 slots; zero UNKNOWN or FENCE-class splits
  *except* the planned old-relay refusal.
* **FORK-a (the old release is refused at the fence)** — the old int-12 relay is kept below H' (peering with the armed nodes, params-id
  warning only) and is **dropped at H' by the new nodes with a named fork-id refusal** (`DisagreePastFence`, 110): *the crossing*, not a
  restart, shows it. **PASS:** the log line names H'; the old node never serves a block at ≥ H' to an armed node; it does not fork
  silently (it stalls or is disconnected).
* **FORK-b (IBD across the fence)** — a fresh node syncs from genesis across H' to the same sink and the same PALW root.
* **FORK-c (partition across the fence)** — a seat partitioned across H' rejoins and reorganises onto the chain; no stuck state.
* **RESTART** — every kind of node (a seat, the head, the builder, an evaluator) stopped and restarted *across* H' (stop at H'−3, start at
  H'+3, and stop at H'+3, start at H'+10) resumes at the tip with the same root.
* **IDS** — every node prints the candidate's params id and a `Consensus fence schedule:` line that contains 110 (the drill's schedule) at start; the
  keyring manifest's `consensus_params_id` equals the node's. Genesis and identity id are the drill's, unchanged by the flag.
* **Refusal strings (grep, all nodes, whole run):** `palw_audit_1004_v1`, `palw_receipt_spend_v4`, `palw_gen_range_twin_v1`,
  `palw_model_court_window` in any `refused`/`invalid`/`rejected` line. **PASS:** only the planned dishonest probes (§3.1) and the V4
  before-the-fence probe (§3.4) appear, each once and each by name.
* **Resources:** peak RSS per node and the host's swap, at the fence and one hour after (fence 1's per-root proofs); never the sole signal
  of failure, but a pre-condition for the fleet layout (§4).

**The drill passes** when every gate above PASSES or is explicitly "not drilled" with the lead's sign-off, and fence 4 reaches step 3.
A FAIL on any gate stops the rollout; the fix is a node-only change if it can be (no new consensus change after the freeze) or the
offending fence is removed from the list (one line + re-pin) and the drill is repeated for the remainder.

## 4. Rollout (after a passing drill) — `upgrade`, never `switch`

`switch` is a **regenesis**: it moves the appdir aside and syncs from genesis. A fence release is an **in-place** `upgrade`, one node at a
time, per `DAA750-ROLLOUT.md` §5.

Timing: public testnet-12 runs ≈ 24 DAA/h (about DAA 7,000 on 2026-10-08), so **DAA 9,000 is ≈ 83 h after DAA 7,000**. Re-read
`getBlockDagInfo.virtualDaaScore` before every step. **Every fleet node must be on the new binary at least 12 h (≈ 290 DAA, so by DAA
≈ 8,700) before 9,000**, and the external operators informed at least 48 h before (a node that crosses 9,000 on the int-12 binary is
refused at the handshake by every upgraded node, exactly as DAA 750 did).

1. **Build and verify (Mac):** `build-release-local.sh <the re-pin commit>` → check `IDENTITY` against the candidate's ids: `EXPECT_FP` = the
   re-pin's params id, **`EXPECT_GENESIS` and `PREMINE_TXID` unchanged**, the identity id unchanged (`5de80e64…`). The genesis must not
   move; if it does the build is wrong.
2. **`fleet.env`:** `REV`, `KASPAD_SHA256`, `MISAKA_SHA256`, `PALW_CLASS_SHA256`, `EXPECT_FP` = the new params id, **`UPGRADE_FROM_FP` =
   `5ee7fd8ee019968cf52929b844cf9ddfb1aad500842a89cf04bced8ba4edefb6`** (the int-12 params id the fleet prints now; full 64 hex);
   `INT13_FLAG_DAY_DAA=9000` is already in `fleet.env.example` and pinned to the core by `t12_deploy_kit_constants`.
3. **Stage everywhere** without touching a service: `distribute-from-mac.sh kit` → `distribute-from-mac.sh binaries` →
   `./install-<host>.sh stage` → `DRY_RUN=1 ./install-<host>.sh upgrade` on all three hosts; a launch-script `ARGS` difference is read, and
   accepted only with `UPGRADE_ARGS_CHANGE_OK=1` if intended (there is **no new argument** in this release: the drill flags are not in the
   unit files).
4. **Upgrade one host, one node at a time**, in the order of `DAA750-ROLLOUT.md` §5 (.113 → ibm → 5.104; `upgrade` checks
   `UPGRADE_REQUIRE_UP` that the other hosts' nodes are alive at every node stop): `./install-<host>.sh upgrade` → `check` →
   `CHECK_REGISTRY=1 ./check-fleet.sh` twice, two minutes apart: the new fingerprint, synced, peers, the DAA advancing, **the 8k class
   `readySeatsNow ≥ 7`**. A host whose seat layout the per-root proofs (fence 1) overload keeps fewer seats until the layout is re-planned.
5. **Between hosts:** all nodes on one sink; the startup log carries `Consensus fence schedule: 750, 1000, 1300, 1700, 2000, 3600, 5300, 5395,
   5490, 5585, 9000 (schedule id <new>)` and no checkpoint ERROR. A mixed fleet below 9,000 is fine: the fork id keeps old and new below the
   height, and the params-id difference is a warning.
6. **Public side:** the explorer banner (`deploy.sh files` → `verify`), the seeders' `40-verify.sh` and `60-join-check.sh <new kaspad>`, the
   panel monitor with `--fence-daa 9000`, the docs/README ids on `main` (fingerprint changes → push with the fleet, per the standing rule),
   the announcement to external operators with the DAA and the ETA.
7. **At 9,000:** watch all 8 nodes cross (the DAA clock keeps ticking, one sink, no `DisagreePastFence` between fleet nodes); then the four
   checks of §3.4 on the public chain are *observations*, not tests: a V4 spend needs an external builder, so the public verdict for fence 4
   is "accepted nothing wrong" until one appears.

## 5. Rollback

* **Before 9,000, any step:** from the failing host back (5.104 → ibm → .113 if several): `./install-<host>.sh upgrade-rollback` with
  `REV` left at the new release — it restores the int-12 binary and unit, appdir untouched. **`rollback` is the regenesis; never use it.**
  A host rolled back stays peer-compatible below 9,000. The `UPGRADE_FROM_FP` / `EXPECT_FP` pair in `fleet.env` is flipped for the
  rolled-back host's `check`.
* **After 9,000:** an int-12 binary no longer follows the chain (the new nodes refuse it; it would validate ≥ 9,000 blocks under the old
  rules — the same trap as DAA 750, `DAA750-ROLLOUT.md` §6/§8). **There is no rollback past the fence**; a defect found after it is fixed by
  a node-only `upgrade` or, if consensus, by the next flag day. This is why the drill is a gate and why fence 4 may be dropped *before* the
  freeze.
* **A node that crossed 9,000 on int-12** (an external operator): upgrade, then move `<appdir>/misaka-testnet-12/datadir` aside and
  resync (the `palw-panel/` round records survive); same instructions as `DAA750-ROLLOUT.md` §8.
* **If the fleet is split at 9,000** (some nodes upgraded, some not): the upgraded majority is the chain (fork id); upgrade the rest in
  place and resync only if one validated past the fence.

## 6. Open items for the lead

1. **Fence 4's evidence** (§1, §3.4): the plan can drive it end to end only if the V4 leg is scripted (rail + builder + a Final free-prompt
   claim). Decide by the freeze whether it ships on this evidence, with the leg as a gate, or is dropped.
   **Update 2026-10-08 evening (C1r2):** the chain-block E2E exists and passes — `rfc9_v4_chain_e2e` (real templates, funded
   carriers, the int-13 list armed at H=80): PFS4 refused by name below the fence; the executor offline after filing material + RDA4
   with two providers; seats fetch from the providers only; Final at DAA 147; another bond redeems at slot 547 in builder mode; payout
   to the executor's registered address, builder fee exactly 500 bps of the worker reward; a V3 counterfactual twin matches every other
   output, safe weight and claim row; sibling PFS4 and a later V3 re-spend paid nothing; reorg reverts and the quantum is re-spent once;
   replay from genesis reaches the same root. The leg is scripted: `audit-combined/rfc9-v4-leg.sh`. **Timing:** the 400-DAA receipt
   maturity puts the leg at ≈19.5 h of drill chain (≈125 s/DAA), longer than the ~12 h budget of §2 — start the leg's claim in the
   first DAA of the run. **Public t12:** V4 is reachable only after someone files `FamilyCertified` + `ClassLaneCertified` (FreePrompt)
   on chain (genesis certifies only in params); the observation in §4 step 7 waits for that.
2. **`palw_model_court_window` was "armed nowhere" by decision** (2026-10-01): this release arms it at 9,000 on the lead's instruction. The
   verdict-neutrality proof (`palw_t12_court_window_changes_no_admission.rs`) passes unchanged with the fence armed; the delta at position
   100 first appears on a public chain at 9,000.
3. **Fence 1's per-root duty** adds proofs per seat at the fence (§3.1): measure before the host layout is fixed.
4. **The undecided/other-lane fences** (`palw_tir_only_v1`, `palw_model_virtual_v1`, `palw_dns_retirement_v1`, `palw_exec_payload_v2`,
   `palw_permissionless_panel_v1`, `palw_probabilistic_constraints_v1`, `palw_panel_free_v1`) join `PALW_T12_INT13_FENCES_V1` by one line and the
   re-pin; each added fence adds its own leg to §3 and moves every id of §4 (so the drill and the fleet staging are repeated).
   **Only before the code freeze.** After the int-13 build is cut the list is frozen (C4 round 3, F-C4R3-04, pinned by
   `c4r3_fork_id_at_int13.rs`): a fence joining 9,000 after an int-13 binary is deployed is invisible to the fork id, so the two builds
   would stay peers past 9,000 while disagreeing. A later fence takes a fresh height.
