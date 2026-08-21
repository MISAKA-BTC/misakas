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
| Verdict | **NO-GO**, and the critic's own reasoning: the branch "does not fail the standard on a technicality — it fails it three independent ways, only one of which the external audit knew about." |

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
| **§1 (half)** | A ConsensusV2 node with **no PALW state ran anyway**, reading absent state as "no policy" — state root unchecked, tips by blue work, any pruning point, deep-reorg comparator skipped. It does not fork, so nothing reported it. | `0cf7ead2` | Refuses at startup. The guard covers the node that did **not** process genesis — the dangerous case — and tells staging apart by its database (`past_pruning_points[0]`) rather than a flag. Two tests: the degraded state is real, and the assertion fires in it. |
| **§4** | The free-prompt **commitment** signature was verified nowhere. `validate_stateless_v3` said "verified by the caller" and there was no caller; the only `validate_signature_v3` use in the tree was the SPEND envelope's. Any stranger's 0x4a tx created a claim bound to any bond outpoint it named. | `dc8ca79c` | The verifier is now an **argument** to the extraction walk, so "somebody else checks it" is unrepresentable. Tests pin the message (claim id), the key (carried), and the context. |
| **producer ×4** | `has_epoch_room()` capped the liveness floor that admission exempts, and the budget table is written for the tip's epoch and read for the candidate's — so the producer **refused the first block of every epoch**. Plus: a `trace_retention_daa` promise with the material dropped, `should_mine` bypassed, and a tokio worker pinned. | `2870f1d6` | `the_liveness_floor_is_never_capped_by_an_epoch_budget` asserts a floor with a ZERO budget is still producible. Material is persisted before the block publishes; a write failure aborts the publish. Both CPU phases moved to `spawn_blocking`. |
| **Ⅳ** | **The court convicted honest executions.** Three divergences from the engine, each invisible without >1 head *and* position >0: SoftMax (engine per head, court once over the concatenation); RoPE (court asked byte offset 0 — always position 0's row — and for the whole row's pairs, not one head's); P·V (V cache is `[position][kv_dim]`, court read `[out_dim][in_dim]` — the transpose, agreeing only at `kv_len == 1`). `map_refutation_outcome` → `ExecutorGuilty` → `void_and_slash` is a live money path, and `CourtClosed` may ride a transaction. | `5cf1a94c` | `the_court_convicts_no_leaf_of_an_honest_execution`: **914 leaves swept, 910 `NoFaultFound`, 0 convicted**, and 16 tampered tiles at the repaired nodes still convicted. Reverting each fix convicts **10 / 32 / 30** honest leaves. |

> **Why a sweep and not a test.** All three court defects survived every single-coordinate test in
> the tree. The RoPE width mismatch *masked* the position bug: at one head the widths coincided and
> it convicted every position but the first; at more than one head the oversized request failed
> instead, so a wrong-answer bug wore an `Unadjudicable` mask. Only "adjudicate every leaf of a real
> multi-head execution" finds that — and only "and still convict a tampered one" proves the fix
> was correctness rather than permissiveness.

---

## Open — blocks a public weight-bearing testnet-12

### 1. A node that did not walk from genesis silently runs with **no PALW rules at all**

`PalwChainStateV2` is written only by `process_genesis`. Absent state is read as "no policy", and
every PALW authority then fails **open**: `palw_state_root` unchecked, no transition applied, tips
ordered by blue work alone, any pruning point allowed, the deep-reorg comparator skipped, any
staged chain accepted at IBD. **It does not fork** — it is strictly more permissive — so nothing
warns anyone.

Four independent triggers:

* **Pruned IBD.** `import_pruning_point_utxo_set` writes no PALW row, and no P2P message carries
  one (`PalwStateCarriageV2` appears nowhere under `protocol/p2p`).
* **An existing datadir.** The only datadir guard is the genesis hash, and bundle-free testnet-12
  and bundled testnet-12 **share a genesis** — so a node that ran before the card was filled never
  installs the zero point.
* **`reindex_if_stale`.** Any schema-version bump deletes every delta row and the tip, with nothing
  that rebuilds. A routine binary upgrade disables PALW enforcement on a healthy node.
* **Staging consensus.** `factory.rs:389` builds it with `.skip_adding_genesis()`, so the ADR-0042
  "Unit D site 2" IBD commit gate is **structurally vacuous** — `decide_ibd_commit_v2` is never
  reached on a real IBD and the commit is authorized unconditionally.

**The refusal half is done** (`0cf7ead2`): a ConsensusV2 consensus resuming a real history with no
PALW tip now aborts at startup instead of running permissively. A silent consensus divergence is a
loud startup message.

**What remains is the half that lets such a node exist at all** — the critic's single
recommendation: give `PalwChainStateV2` a pruning-point import — a `RequestPruningPointPalwState` /
`PruningPointPalwState` message pair, a capture in the pruning processor beside the overlay
snapshot it already takes, and an `import_pruning_point_palw_state` that installs the root-verified
carriage as the store's tip. Until it exists, a stranger **cannot join** once the pruning point
leaves genesis — which is a smaller failure than joining wrongly, and is where the refusal buys
time.

### 2. The claim lattice has no configuration in which a claim reaches `Final`

With the shipped one bond, no panel seats (now refused at genesis by Ⅰb). **With enough bonds to
seat one, the chain binds a panel automatically and then slashes every seat at `ReceiptTimeout` —
because no code in the tree ever files a `ReceiptLicensed`.** Both configurations lose money. This
is the next thing that must exist for the lattice to turn over at all.

### 3. Post-genesis class registration is unauthenticated (H-01's side effect, H-07, OBS-01)

Closing H-01 made post-genesis `ClassRegistered` a live permissionless path. Nothing signs it and
nothing gates who may take a permille from every incumbent. Compounding it, **H-02**:
`verify_profile_coverage_v1` has no non-test caller — `verify_class_admission_v2` checks only the
reachable kernel-ID set — so a stranger can register a class naming catalogued kernel ids at shapes
the adjudicator cannot serve. Every dispute over those nodes ends `Unadjudicable`: **rejected but
unslashed**, which is unfalsifiable work on a chain where bonds are supposed to be at risk.

### 4. ~~The free-prompt commitment signature is verified nowhere~~ — **CLOSED** (`dc8ca79c`)

See the closed table above.

### 5. One solved PoW mints unbounded relay-valid blocks

The attempt envelope's ML-DSA-87 signature is verified on the chain-walk path only. Header/body
validation never checks it, so a block is broadcast to every peer with **arbitrary bytes of the
right length** in `signature`. Cost to the attacker: one byte flip and a re-hash. The block never
becomes chain, but it is DAG spam that costs nothing to produce.

### 6. The per-class DAA target divides by 4 at every epoch boundary

A `ConsensusV2` network demands exactly one `pow_algo_id`, so the receipt lane (algo 7) is
structurally impossible — every algo-7 header is rejected by the header gate. Yet the retarget still
measures the floor against a 150‰ attempt-lane expectation, so the floor reads as a 6.67× over-
producer at **every** epoch boundary. Its target reaches 1 after ~63 epochs (~87.5 days), after
which the class lottery refuses every attempt and there is no path back.

### 7. Fork-choice inversions

* On `ConsensusV2` a **resolvable tip beats any unresolvable one**, and only the current sink is
  resolvable — so a sink the network later orphans wedges a non-mining node permanently, and an
  attacker can force that state on a chosen node with one privately-delivered block.
* The deep-reorg gate fails **open** on any candidate this node cannot weigh: `palw_state_walk`
  deliberately refuses a missing delta, and `.ok()?` two layers up turns that refusal into `None`
  → `GateInactive` → allow.
* The tip site uses a **different comparator** from the other three, contradicting Unit D's claim of
  one authority, and drops the primary key.
* The V2 tip snapshot is written in its own `WriteBatch` **before** the virtual state commits — an
  unclean shutdown in that window leaves the PALW tip ahead of the sink and the next start panics in
  `apply_delta_v2`, in a loop, recoverable only by wiping the data directory.

### 8. Money conservation

* **Block subsidy is claimable with hash alone.** Attempt admission runs only for selected-chain
  candidates, while every merged blue block in the DAA window is paid its full worker share — with
  no escrow — from the accepting block's coinbase.
* **The escrow is destroyed rather than paid** whenever a claim never reaches `Final` (see §2): 620‰
  of every block's subsidy is withheld and burned by don't-mint, with no log line saying so.
* **PALW V2 state grows without bound** — claims are never removed and the pruning processor never
  deletes delta rows, while `load_tip` does a full root-verified rebuild per sink-search candidate.

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

1. **§1's remaining half — the pruning-point import.** The refusal now makes a stranger's node
   stop instead of diverge; the import is what lets it join. Still the critic's one recommendation.
2. **§2 — something has to file a `ReceiptLicensed`**, or the lattice never turns over and every
   panel seat is slashed at `ReceiptTimeout`.
3. **§3 — authorize post-genesis `ClassRegistered`** (or fence it off for t12) and wire the coverage
   gate; the two compose into unfalsifiable work.
4. **§6 — the per-class DAA divides by 4 every epoch** on a network whose receipt lane cannot
   produce. Latent while other blockers stop the chain first; it goes live the moment they lift.
