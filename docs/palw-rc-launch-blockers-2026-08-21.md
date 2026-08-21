# PALW-RC (testnet-12) — launch blockers, 2026-08-21

**What this is.** The standing answer to "what still stops testnet-12 from being a public
weight-bearing network, and how do we know?" Every entry carries file:line evidence, its current
state, and — where it has been closed — the commit and the measurement that says so.

It exists because the evidence behind it does not survive: the verification ran as a 36-agent
workflow whose full output (≈700 KB) lives in a session-scoped temp file and a
`journal.jsonl` under `~/.claude/projects/…/subagents/workflows/`. Both vanish. The findings do not
belong there, for the same reason [the external audit](palw-external-audit-2026-08-21.md) was
recorded verbatim: *"the audit said X" is a claim somebody should be able to check.*

## Provenance

| | |
|---|---|
| Base of the sweep | `8db458f2` (branch `palw-base0-depth`) |
| Method | 36 agents: the 19 external-audit findings re-verified in 9 buckets → adversarial refutation of every CLOSED verdict → 8 launch dimensions swept → each dimension adversarially re-examined → a completeness critic |
| Inputs | [docs/palw-external-audit-2026-08-21.md](palw-external-audit-2026-08-21.md) (the commissioned static audit and its Appendix A) |
| Verdict at the sweep | **NO-GO**, and the critic's own reasoning: the branch "does not fail the standard on a technicality — it fails it three independent ways, only one of which the external audit knew about." |
| Verdict now (2026-08-21, `bb62f1fc`) | **All eight dimensions closed at the code level.** Still NO-GO for a weight-bearing launch, for exactly one reason, and it is a missing subsystem rather than a defect — see *What is still missing*. |

**Appendix A of the external audit does not apply to this branch.** It was written against
`palw-mainnet-rc-integration` at baseline `e5d7a69c`. Its status column is stale here.

**The adversarial pass refuted 8 of 10 CLOSED verdicts.** One pattern accounts for almost all of
them: *closed for a node that walked the DAG from genesis, open for a node that joined by pruned
sync* — which is how every stranger joins a public network.

---

## Closed since the sweep ran

Each was verified by mutation: reverting the fix makes the test fail with the stated measurement.

| # | What it was | Commit | Evidence it is closed |
|---|---|---|---|
| **Ⅰa** | The chain wedged at genesis+2. Collateral 400,000 × 500‰ = ceiling 200,000; one claim reserves 15,800 pwu × 5 = 79,000 → **2 concurrent claims** against a 600-DAA `window_bind`, and DAA only advances when blocks are produced. No timeout, no message, no restart. | `45ba25fa` | `BondCannotSustainBindWindow` fired on the shipped bundle with exactly `{ supported: 2, window_bind: 600, needed_collateral: 94_800_000 }`. Collateral is now derived (`palw_v2_collateral_for_bind_window_v1`). |
| **Ⅰb** | No panel could **ever** seat. `PalwPanelParamsV2::new(5, 3, …)` needs 5 seats; `derive_panel_v2` excludes the executor by bond, operator AND key and seats one bond per operator → 6 distinct operators required. Genesis registered **one** bond and `BondRegistered` may not ride a transaction. Every claim voids at `BindTimeout`, `safe_weight` stays 0, and each block's 620‰ worker carve is burned. | `45ba25fa` | `PanelCannotBeSeated`. The genesis card is now a **registry** (`PALW_RC_GENESIS_BONDS`). Tests refuse a registry one operator short *and* a registry of clones. |
| **Ⅱ.1** | `ProducerDefaulted { claim, receipts: [] }` — no signature of any kind — fell through `_ => {}` at `processor.rs:3887` and was folded: `slash_silent_seats` charged **every** panel seat `claim.reserved`, then `void_and_slash` debited the producer's bond. `ReceiptLicensed { receipts: [] }` was the mirror. `validate_receipt_quorum_v2` was written, tested, and had **zero callers in the tree**. | `40002ddd` | Objects now carry signed `PalwSeatReceiptV2`; acceptance runs the quorum check and verifies its **direction**. `palw_v2_an_unsigned_receipt_set_cannot_slash_anyone` drives the acceptance layer directly. |
| **Ⅱ.2** | `BondRetireRequested { bond }` had no owner binding, and a bond key **is a published premine outpoint**. One transaction retired any bond, permanently, with no inverse — on a one-producer network, a free permanent halt. | `40002ddd` | Refused at the ride list **and** at acceptance. |
| **Ⅱ.3** | `ClassFrozen`'s contradiction certificate has signatures `check_class_contradiction_shape_v2` defers to "the acceptance layer", which had no arm. A forged one froze a class forever — there is deliberately no `ClassUnfrozen`. | `40002ddd` | Refused at both layers until `adjudicate_class_contradiction_v1` is wired. |
| **Ⅱ.4** | `CourtOpened` named a challenger bond with **no authorization from it**. Everything `validate_court_opened_v2` checked was a fact *about* the bond, none about who spoke for it — and the transition disarms the claim's final deadline while a session is open. | `4724863a` | Challenger signature over the session id, in its own ML-DSA-87 context. The test asserts the message **and** the context. |
| **Ⅱ.5** | A stateful lie in a 0x4b transaction **killed the honest block that accepted it**. 0x4b admission is purely stateless, so the transaction relayed and mined freely; the first honest block to accept it was disqualified, and the transaction stayed in the acceptance set for the next candidate. ~100 bytes, one fee, chain stops. | `4724863a` | A failing object is now **dropped** and the block stands. The verdict is a pure function of `(state, params, point, object)`, so every node drops the same ones. |
| **producer ×4** | `has_epoch_room()` capped the liveness floor that admission exempts, and the budget table is written for the tip's epoch and read for the candidate's — so the producer **refused the first block of every epoch**. Plus: a `trace_retention_daa` promise with the material dropped, `should_mine` bypassed, and a tokio worker pinned. | `2870f1d6` | `the_liveness_floor_is_never_capped_by_an_epoch_budget` asserts a floor with a ZERO budget is still producible. Material is persisted before the block publishes; a write failure aborts the publish. Both CPU phases moved to `spawn_blocking`. |
| **Ⅳ** | **The court convicted honest executions.** Three divergences from the engine, each invisible without >1 head *and* position >0: SoftMax (engine per head, court once over the concatenation); RoPE (court asked byte offset 0 — always position 0's row — and for the whole row's pairs, not one head's); P·V (V cache is `[position][kv_dim]`, court read `[out_dim][in_dim]` — the transpose, agreeing only at `kv_len == 1`). `map_refutation_outcome` → `ExecutorGuilty` → `void_and_slash` is a live money path, and `CourtClosed` may ride a transaction. | `5cf1a94c` | `the_court_convicts_no_leaf_of_an_honest_execution`: **914 leaves swept, 910 `NoFaultFound`, 0 convicted**, and 16 tampered tiles at the repaired nodes still convicted. Reverting each fix convicts **10 / 32 / 30** honest leaves. |

> **Why a sweep and not a test.** All three court defects survived every single-coordinate test in
> the tree. The RoPE width mismatch *masked* the position bug: at one head the widths coincided and
> it convicted every position but the first; at more than one head the oversized request failed
> instead, so a wrong-answer bug wore an `Unadjudicable` mask. Only "adjudicate every leaf of a real
> multi-head execution" finds that — and only "and still convict a tampered one" proves the fix
> was correctness rather than permissiveness.

---

## The eight launch dimensions — **all closed at the code level**

Each was verified by mutation or by a test that is red on regression. The text of what each one
*was* is kept, because "we fixed it" is worth nothing without "here is what it did".

| # | What it was | Commit |
|---|---|---|
| **1** | A node that did not walk from genesis ran with **no PALW rules at all**. Absent state read as "no policy", so every authority failed open — `palw_state_root` unchecked, tips by blue work, any pruning point, the deep-reorg comparator skipped. It does not fork (it is strictly more permissive), so nothing warned anyone. Four triggers: pruned IBD, an existing datadir, `reindex_if_stale` after any schema bump, and a staging consensus built with `.skip_adding_genesis()`. | `0cf7ead2` (refuse) + `e52a1234` (import) |
| **2** | **The lattice had no configuration in which a claim reached `Final`.** Every `PalwSeatReceiptV2` and `ReceiptLicensed` in the tree was a test fixture. With one bond no panel seated; with enough bonds the chain bound one and then slashed every seat at `ReceiptTimeout`. `safe_weight` stayed 0 forever and every block's 620‰ carve was burned. | `d85ab588` — **consensus side only**, see *What is still missing* below |
| **3** | Post-genesis `ClassRegistered` was **unauthenticated** — nothing signed it and nothing gated who could take a permille from every incumbent. Compounded by `verify_profile_coverage_v1` having no non-test caller, so a stranger could register a class at shapes the adjudicator cannot serve: every dispute over those nodes ends `Unadjudicable` — rejected but unslashed, which is unfalsifiable work on a chain where bonds are supposed to be at risk. | `cb131570` |
| **4** | The free-prompt **commitment** signature was verified nowhere: `validate_stateless_v3` said "verified by the caller" and there was no caller. Any stranger's 0x4a transaction created a claim bound to any bond outpoint it named. The verifier is now an **argument** to the extraction walk, so "somebody else checks it" is unrepresentable. | `dc8ca79c` |
| **5** | **One solved PoW minted unbounded relay-valid blocks.** The attempt envelope's signature was checked on the chain-walk path only, so a block relayed to every peer with arbitrary bytes of the right length in `signature`. Cost to the attacker: one byte flip and a re-hash. | `be690d20` |
| **6** | The per-class DAA target **divided by 4 at every epoch boundary**. A ConsensusV2 network demands one `pow_algo_id`, so the receipt lane is structurally impossible — yet the retarget measured the floor against a 150‰ attempt-lane expectation, reading it as a 6.67× over-producer forever. Target 1 after ~63 epochs (~87.5 days), after which the lottery refuses every attempt with no path back. | `4210c7be` |
| **7** | **Four fork-choice inversions.** (a) A resolvable tip beat any unresolvable one, and a candidate is only weighable once its deltas exist — which is only after it has been committed. So the incumbent outranked every challenger by construction: a node whose branch stalls never reorgs again, and one privately-delivered block forces that on a chosen victim. (b) The deep-reorg gate then failed **open** on a candidate it could not weigh. (c) The tip site used a different comparator from the other three. (d) The tip row is written in its own batch before the virtual state commits, and the walk took it as the state at its own starting point — so a crash in that window panicked `revert_delta_v2` on every subsequent start, forever. | `268293b9` |
| **8** | **Money conservation, three ways.** (a) The subsidy was claimable with a hash alone: the stateful half of admission runs on the selected chain only, while the coinbase paid every merged blue its full worker share — an unbonded miner collected on PoW. (b) A voided claim's escrow is burned by don't-mint deliberately, but nothing said so, so a network whose panels never bind burns the whole carve of every block and looks identical to one paying it. (c) The state grew by one claim (and one panel) per block forever, and `state_root` re-hashes every collection on every block. | `bb62f1fc` |

### The §8 growth measurement

Release build, frozen 120 s cadence, one claim of 530 bytes per attempt-lane block:

| claims | `state_root` | tip row | reached after |
|---|---|---|---|
| 10,000 | 8.2 ms/block | 5.4 MB/block | ~14 days |
| 100,000 | 49 ms/block | 54 MB/block | ~139 days |
| 1,000,000 | 467 ms/block | 538 MB/block | ~3.8 years |

It does not plateau. Terminal claims now retire on a span the ruleset declares (the court window on
the shipped bundle), which settles the map at ~7,200 entries; `validate` refuses a ruleset that
declares no span at all. The measurement is kept runnable as `measure_claim_growth_cost`
(`#[ignore]`d).

---

## What is still missing — and it is not a defect, it is a subsystem

**A panel seat cannot obtain the material it is supposed to check.** §2 built the consensus side —
`palw_seat_duties_v2` tells a node which claims it is seated on, `base0_material_matches_claim_v1`
decides what a seat should answer, and a signed quorum licenses a claim — but the producer writes
its retained execution to its **own disk** (`kaspad/src/palw_producer.rs`) and *nothing serves it
and nothing fetches it*. There is no `trace_chunk` message under `protocol/p2p/proto`, no serving
flow, and no fetch. A seat on another host has no way to get the tiles, so no honest panel can file
a `ReceiptLicensed` on a real network however the service is written.

Three pieces, and only the first is purely mechanical:

1. **A trace data-availability transport**, and it needs no new format. BASE-0's
   `trace_chunk_count` is **1** — `base0_execute_for_attempt_v1` says so in as many words: "the
   whole trace is one object at this class's size" — and the producer already writes exactly that
   object, `borsh((binding, tiles, generated_token_ids))`, to
   `<retention_dir>/<attempt_id>.material`. So the request is keyed by the claim id alone and the
   response is that blob. Note what the fetcher must NOT do: `trace_manifest_root` hashes
   `(ctx, trace_root, step_merkle_root, count)` and **not the chunk bytes**, so it cannot
   authenticate a served blob — the check is `base0_material_matches_claim_v1`, which rebuilds the
   roots the on-chain claim already carries. Shaped like §1's pruning-point carriage pair otherwise.

   Deliberately not built this session: a request/response pair needs a requester that owns the
   route, and the only requester is (2). Landing the serving half alone would add protocol surface
   nothing can reach — which is the exact shape of the defects this sweep spent itself finding.
2. **A panel service in kaspad.** Poll `palw_seat_duties_v2`, fetch, run the material check, sign a
   `PalwSeatReceiptV2`.
3. **A way to submit it.** `ReceiptLicensed` rides a 0x4b transaction, which needs a funded UTXO and
   a signing key — i.e. a validator node has to hold a wallet. **That is a product decision, not an
   implementation detail**, and it is the reason this is written down rather than guessed at.

Until (1)–(3) exist, a live network still voids every claim at `BindTimeout` or `ReceiptTimeout`.
The difference from before this session is that it now says so in the log (§8b) and cannot slash
anyone for it (Ⅱ.1), rather than doing it silently.

---

## Bugs introduced in this session's own producer work — **all four CLOSED** (`2870f1d6`)

Recorded rather than quietly fixed. Kept here because the first one is the kind of mistake worth
remembering: a client-side re-creation of a chain-stopping deadlock that had already been removed
from consensus.

| Where | What | State |
|---|---|---|
| `palw_producer_v2.rs:85` | `has_epoch_room()` applies the epoch budget to the base class, which **admission deliberately exempts** (`palw_admission_v2.rs:234`) — re-creating client-side the deadlock removed from consensus in `58291251`. Worse: the budget table is written for the **tip's** epoch and looked up for the **candidate's**, so a missing entry becomes `unwrap_or(0)` and the producer **refuses the first block of every epoch**. | CLOSED |
| `palw_producer.rs` | Signs a `trace_retention_daa` obligation it structurally cannot meet — the execution's tiles and binding are dropped when `produce_one` returns, and nothing persists or serves them. The comment asserting the opposite reads as a verified property. | CLOSED |
| `palw_producer.rs` | Bypasses `FlowContext::should_mine`, "the gate every participation path consults" — it will produce with zero peers, on a stale sink, and while chain participation is closed. | CLOSED |
| `palw_producer.rs` | The job build, inference and nonce grind run synchronously inside an async task, pinning one tokio worker. Trivial at genesis difficulty; bites once the retarget pulls the search out to the 120 s cadence. | CLOSED |

---

## Operator-facing errors

* **[runbook](palw-rc-testnet12-launch-runbook.md) §4 names the wrong P2P port** — testnet-12 listens
  on **26411**, the runbook says 16311. With `dns_seeders` empty, `--addpeer` is the only discovery
  path, so a fleet brought up by following the runbook **never forms**.
* `palw_rc_params` raises `finality_depth` to 600 without raising `pruning_depth` (1144), violating
  the prunality lower bound of 1384 — the region upstream's own comment labels "unsafe".
* `palw-rc-genesis` applies **no length validation** to `--bond-pubkey` / `--operator-pubkey`, and
  neither does the genesis gate, yet it prints `ACCEPTED — every gate the genesis loader runs has
  passed`. A truncated paste mints a genesis nobody can sign for; the cost is a flag-day relaunch.
* `--bond-index 40` silently locks the entire **9,000,000,000 MSK** main premine wallet as collateral
  — and it is the only premine output whose key a testnet operator plausibly holds.
* testnet-12 silently inherits testnet-10's DNS-finality PoS overlay and VLT shadow overlay,
  genesis-active, with the VLT model cost table inside the consensus fingerprint. (The specific
  wedge the audit feared is **not** reachable on a bundled node — the V2 comparator short-circuits
  the DNS gate — but the inheritance is unexamined, the same class of accident as the EVM lane.)
* `misaminer` and `pq-miner` **spin every core at 100% forever**, with no log line and no template
  refetch, when pointed at a ConsensusV2 network: both refuse algo 4/5 explicitly and have nothing
  for algo 6/7. This is the failure a public testnet actually produces.
* Half of §5's "verify the network is what you think it is" table names things an operator cannot
  observe: no log line on a healthy node mentions algo 6, and `epoch_produced_blocks` has no RPC,
  CLI or log surface at all.

---

## The integration crate had never run — and running it earned its keep immediately

Not one finding but three, peeled in order, recorded so none is rediscovered.

`cargo test --workspace --lib` reports **2044 passed / 0 failed across 41 crates**;
`kaspa-testing-integration` is the 42nd and had never completed a workspace run.

1. **It aborted at the first daemon test** (before `d0727415`): the daemon tests boot a real kaspad
   on devnet, devnet activates the EVM lane at DAA 0, and the startup refusal added with the RC's
   lane decision (`740ed99e`) exits the process — taking the harness with it. Fixed by the `evm`
   default (now on kaspad itself, `7b6412e7`).
2. **Then two IBD tests failed deterministically** — and this was a REAL §1 BUG, not the suite.
   `e52a1234` wired the request, the serving flow and the IBD-side handler, and missed the fourth
   wiring point: the IBD flow's **subscription list**. The `PruningPointPalwState` reply arrived at
   a router with no route for it → protocol error → connection closed → **every pruned IBD on every
   network failed**, because the request was sent unconditionally. The §1 import had been verified
   at the consensus layer only; the first real-TCP IBD ever run with this code in the binary found
   it within minutes. Fixed twice over: the subscription is registered (round trip proven on a live
   IBD, 142 s green), and the request is now sent **only under a ConsensusV2 ruleset** — the pair
   rode into protocol version 103 without a bump, so an old 103 leader has no flow for the request
   and would close the connection from its side; on a V2 network every peer is a new binary by
   construction (the ruleset is in the consensus fingerprint). With the fix (`bce0f4e4`) the crate
   minus the daemon tests is green in one serial run: **30 passed / 0 failed / 17 ignored**, 812 s.
3. **Still open**: a single-threaded run of the WHOLE crate spins at 99% CPU inside
   `daemon_cleaning_test` (the upstream shutdown-refcount assertion) even though the daemon tests
   pass 3/3 in isolation — something an earlier test starts keeps a reference alive. Whether it
   predates this session cannot be compared: before item 1 the crate died earlier either way.

Reproduction:

```
MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-testing-integration --lib -- --test-threads=1
```

The fixture variable is the model-free devnet PALW path; both requirements predate this session.

---

## Known-open and deliberately scoped out

* **The decode-call embedding gather is `Unadjudicable`** — 4 of 914 leaves. Its token is a
  generated id whose BASE-0 commitment rides `base0_logits_trace_root_v1` (the integer trace root)
  while the court's decode-token check recomputes the v2 event-tree root. Closing it is the
  integer-leg dispatch. It is *cannot check*, not *wrongly convict*, so it burns no honest bond —
  and `the_court_convicts_no_leaf_of_an_honest_execution` pins it so a **new** hole fails the test.
* **ADR-0049 Decision E's selection rule does not exist** — no argmax, no tie-break, no fault code
  (C-04's second half). Nothing on-chain refutes a committed decode token.
* **M-02** — `state_root()` covers five items ADR-0043's preimage does not (class shares, epoch
  budgets, receipt targets, pending payouts, receipt epoch counters). Documentation debt today; it
  must be settled **before the ruleset id is frozen**, or a golden vector is fixed against the wrong
  ADR and the correction becomes a flag day.
* **Qwen (H-04 / H-05 / H-06 / M-03)** — not a launch blocker: testnet-12 registers only BASE-0.

---

## Reading order for the next session

1. **The data-availability transport** (above). Everything else on this page is closed; this is what
   stands between the code and a chain whose claims mature.
2. **The wallet decision** a validator node needs in order to submit a receipt at all.
3. **M-02** — settle `state_root()`'s five items against ADR-0043's preimage **before** the ruleset
   id is frozen. The id already moved once this session (retirement changed the state), so a t12
   genesis must be re-minted regardless; doing M-02 first makes that one flag day instead of two.
