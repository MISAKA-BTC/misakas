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
| Verdict now (2026-08-22, `9d8c7645`) | **Code-complete.** All eight dimensions closed, and the missing subsystem — material gossip, the panel service, the quorum submitter — is built and correspondence-tested. What separates this from a public weight-bearing testnet-12 is operational: a multi-node drill, the genesis re-mint (with M-02 settled first), and fleet deployment. |

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

## What was still missing — built (`9d8c7645`, 2026-08-22)

**A panel seat could not obtain the material it judges, could not deliver its receipt, and nobody
assembled a quorum into the object a block accepts.** The consensus side existed end to end and was
unreachable from the network. All three pieces now exist:

1. **Transport** — broadcast, not request/response. The RC floor's material measures **2.27 MB**
   once per 120 s block; one flood serves all five seats and doubles as the producer discharging
   its retention obligation in the open. The bytes authenticate themselves against the claim's own
   committed roots. Flood control: relay-once by digest, an 8 MiB cap, 4 distinct payloads per
   claim, a bounded inbox. The band is gated on a ConsensusV2 ruleset in both directions (the
   old-binary lesson from the pruning-state pair, applied before it bit).
2. **The panel service** (`--palw-panel`): duties → material check (`base0_material_matches_claim_v1`,
   through the ONE codec the retention file and the broadcast now share) → signed
   `PalwSeatReceiptV2`, broadcast. An `Unavailable` is filed only after half the receipt window —
   an early accusation is a false one with a signature on it.
3. **The submitter, and the wallet decision decided**: no wallet. One outpoint
   (`--palw-fee-outpoint`) paying to the bond key's own P2PKH, spent per submission, change back to
   the same address, rolling outpoint persisted. The object comes from
   `palw_v2_receipt_quorum_assemble`, which grows the receipt set through the ACCEPTANCE validator
   itself — the submitter and the chain cannot disagree about what a quorum is. One funded node per
   network suffices; a duplicate submission dies at acceptance as a wrong-phase object.

The correspondence test (`palw_v2_a_gossiped_receipt_pool_assembles_the_object_a_block_accepts`)
pins the chain end to end: registry-signed receipts polluted with a forged signature and a
duplicate seat assemble into exactly the quorum, the payload passes the 0x4b admission gate, and
the object passes `palw_v2_validate_objects` at the same state. Building it caught a real
divergence — the assembler evaluated at the sink's DAA and refused every receipt signed at
virtual's — which is the "correspondence defects are found by round trips" pattern, again.

**What remains is operational, not code**: a multi-node drill (producer + 5 seats + one funded
submitter on real hosts) before any public weight-bearing announcement, and the genesis re-mint
the retirement change already forces (settle M-02 first — one flag day, not two).

---

## Launching it found fourteen more — the class of defect only a real network has

Recorded because the pattern is the finding: **every one of these is invisible to a test suite,
because a suite always starts from a chain that already exists and a binary somebody already built
correctly.** The launch of testnet-12 (2026-08-22) surfaced them in the order an operator would: six before the first block, two that only a running chain could show, four that needed a multi-host chain carrying real traffic, and two that needed the four before them fixed first.

| # | what | why no test could see it | fix |
|---|---|---|---|
| 1 | **A PALW network could not be born.** `should_mine` demands a sink "nearly synced" — a timestamp within a quarter of the difficulty window of now — and a genesis timestamp is by definition in the past. On a fresh chain the clause is false for EVERY node at once, so nobody may produce block 1, ever. The RPC mining path has always had the `--enable-unsynced-mining` escape; the in-node producer consulted `should_mine` alone and inherited none of it. | Every test runs on a chain with blocks in it. The genesis instant is the one moment this bites, and nothing re-enters it. | `65a848f6` — the flag waives the SYNC clause only; peers and participation are re-checked explicitly, so it never buys mining alone or on a quarantined chain |
| 2 | **The genesis tool could only build a card its own gate refuses.** It assembled ONE row; `verify_palw_genesis_v2` needs six distinct operators. Every invocation ended in `PanelCannotBeSeated`. | The gate and the tool were each tested against their own fixtures. Nobody ran the tool and fed its output to the gate. | `1e258704` — `--emit-row` / `--rows`, split along the secrecy line |
| 3 | **No key-length validation anywhere.** The genesis loader stores what it is handed, so a truncated paste mints a bond nobody can sign for — and `BondRegistered` may not ride a transaction, so the only repair is a flag-day relaunch. | Fixtures always carry well-formed keys. | `1e258704` — both sides demand exactly 2,592 bytes |
| 4 | **The payout could be an address nobody holds.** With no default, the obvious move is to paste some address; the obvious mistake is one whose key nobody on this network has — an unspendable payout, discovered a settlement window after launch. Caught mid-launch: the first row took its payout from a throwaway keygen probe. | There is no wrong answer to test against; the defect is a missing default. | `e76de592` — defaults to the bond key's own address |
| 5 | **The tool printed a params id no node would ever log.** `From<NetworkId>` wraps its match in `with_registered_models`; `palw_rc_params_from_artifacts` — documented as "the call a node makes at boot" — did not. Same registry, `c4a381f6…` from the tool against `9d0cc709…` from the node, on the one value the handshake turns on. | Both sides were self-consistent. The divergence exists only between two programs nobody had run against each other. | `b8a06cf1` — applied at the source, idempotent |
| 6 | **`--skip-cost` had never worked**, and `--emit-row` would not have either: `arg()` returns the value AFTER a flag, so a bare boolean at the end of the line reads as absent. | A flag that silently does nothing produces no failure to observe. | `1e258704` — `has_flag` |

### The two the chain itself found, after it started

Both needed a chain that had been running for a while — the first for 600 blocks, the second for
five re-syncs. Neither is reachable by any test, because a test would have to run the real thing
for real time.

| # | what | fix |
|---|---|---|
| 7 | **The exposure ceiling was one claim short of the bind window.** `palw_v2_collateral_for_bind_window_v1` sized collateral for exactly `window_bind` concurrent claims — which moved the genesis deadlock from block 2 to block 600 rather than removing it. Measured: 600 blocks, then `holding: the bond's exposure ceiling leaves no room for another claim`, forever. Admission runs against the PARENT state, so producing block `window_bind + 1` needs room for `window_bind + 1` live claims, and the first void is not swept until block `window_bind + 2` — which the chain can no longer reach. DAA advances only when blocks are produced, so no timeout helps. | `2b1097f3` |
| 8 | **Following one chain as it grows counted as abandoning it.** Host B adopted the producer's single chain five times as it advanced — five tips, one lineage, one pruning point — spent `MAX_CHAIN_SWITCHES` and quarantined itself while perfectly healthy. The earlier fix to this counter (count adoptions, not encounters) was right and insufficient. On PALW, where checking a header costs an inference, *every* node that falls behind hits this; `--clear-quarantine` deliberately keeps the count, so it is permanent. | `73c35c6d` |

### The three only a MULTI-HOST chain could show

| # | what | fix |
|---|---|---|
| 9 | **A sub-1-BPS network killed its own node with its own first transaction.** `FeerateEstimator::new` asserted `inclusion_interval < 1.0`, a bound whose justification `build_feerate_estimator` states out loud — "since … **bps >= 1**". PALW's frozen cadence is 120 s, so bps is 1/120 and the value is 120× larger: a 7,292-mass receipt transaction yields 1.75 s and the process exited. | `cdeb4080` |
| 10 | **…and the same assumption failed the other way.** `network_blocks_per_second: 1000 / target_milliseconds_per_block` is integer division, so at 120,000 ms it is **0**, and the estimator's `avg_mass / (mass_per_block × 0)` is `+inf`. Fixing only the bound moved the panic message, not the panic. | `44441f70` |
| 11 | **Two submitters racing one claim killed the honest block carrying both.** One funded submitter suffices, so several MAY be funded; both assemble the same quorum and both submit. Both objects are valid against the PARENT state, so a filter judging each one there passes both — and the transition, which applies them in order, refuses the second as `wrong phase`. Measured: 175 produced, 23 accepted, 74 disqualified, DAA frozen at 103 while three hosts submitted correctly. | `dc0fc144` |
| 12 | **And the fix for 11 ate every object after the first.** The rehearsal fold re-applied at the block's own chain point, which `apply_palw_transition_v3` accepts exactly once (it demands a strictly increasing blue score). 356 blocks, 72 submissions, **zero licensed** — and the only thing that said so was the weight line added the same day. | `0c2931f6` |

### The two that only appeared once the first twelve were fixed

Each of these was *behind* a defect above: nothing could reach them until the chain in front of
them worked. That is the shape of the whole list — a network reveals its next defect only after
you have removed the one standing in front of it.

| # | what | fix |
|---|---|---|
| 13 | **The collateral was sized for the wrong window, so no chain could ever finalize a claim.** `release_for_claim` runs on `Final` and on `Voided`, and nowhere else — a claim holds its executor's exposure for its whole life, not until a panel binds it. Collateral was derived from `WINDOW_BIND + 1`, admitting 601 concurrent claims, while the earliest `Final` any chain can reach is a licensed claim plus `WINDOW_CHALLENGE`: block 1200 at one claim per block. Every claim the chain was waiting to finalize was itself occupying the room the chain needed to keep producing. Measured twice: 601 blocks, `weight=0 final_claims=0 unresolved=600`, then held forever. Finding 7 was the same arithmetic one layer shallower — the `+1` was right, the window was not. | `c23a3ff2` |
| 14 | **The submitter double-spent its own fee UTXO inside a single tick.** It re-resolved the fee outpoint per claim, but a carrier submitted earlier in the same tick leaves its change in *our own mempool*, not in the virtual UTXO set — so every claim after the first was handed the same spent outpoint and refused. The unresolvable-funding warning fired once per claim per tick with it: **24,014 identical lines in fifteen minutes**, which is also how a long-running node fills a disk. Resolve once per tick, chain the change onto the next carrier, warn once. | `c23a3ff2` |

**A design fact the launch settled, which is not a defect and must not be read as one:** at the
frozen 120 s cadence, `WINDOW_CHALLENGE = 1200` puts the first `Final` on any chain **about 40
hours after its genesis**. `safe_weight` and the safe frontier are zero until then by
construction. The fork-choice order is not blind in the meantime — its third key, `live_total`,
carries the bounded immature contribution and moves from the first block — but nothing logged it,
so an operator watching the opening day could not tell a healthy young chain from a dead lattice.
`7eac681a` puts `live_total` on the same line as `safe_weight` for exactly that reading.

**The regression tests are at the layer that owns the invariant** (`e2b47753`): a chain must be
able to produce its way to the first `Final` without production stopping first. Against the old
sizing it prints what the fleet printed — *the ceiling admits 601 concurrent claims, but no claim
can finalize before block 1200*. All 797 PALW tests passed while that was true of the shipped
params, because no test asked whether the *deployed configuration* could reach the end of its own
lattice.

**The observability line is what made 12 findable.** Disqualifications were gone, submissions were
landing, every log an operator reads looked healthy — and `weight=0 unresolved=355` was the sole
statement that the lattice still was not turning over. A network can produce blocks, gossip
material, file receipts, assemble quorums and submit them, and still certify nothing.

Operational facts the launch also settled, which no amount of code reading would have produced:
host A cannot reach any fleet member on 26411 (its egress is selectively filtered; it reaches
1.1.1.1:443 fine), so the fleet is ibm-as-hub with B and C; C's ufw has no 26411 rule and that is
the operator's to open; and `pgrep -f <pattern>` matches the ssh command line carrying the pattern,
which reported two dead nodes as running until a listening-socket check replaced it.

---

## The integration crate had never run — and running it earned its keep immediately

> **Final state (2026-08-22, `4b5d8451`):** `MISAKA_PALW_POW_FIXTURE=1 cargo test --workspace
> --lib` is **fully green — 68 crates, 2428 passed, 0 failed**, the integration crate itself
> reporting `33 passed / 0 failed / 18 ignored` with the daemon tests included. The first complete
> workspace pass in this repo's recorded history. The serial-mode (`--test-threads=1`) spin in
> `daemon_cleaning_test` remains reproducible and remains item 3 below; the default mode a
> workspace run actually uses does not hit it.

Not one finding but FOUR, peeled in order, recorded so none is rediscovered.

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
3. **Then the wallet aborted on testnet-12** (`4b5d8451`) — consensus gained `Some(12)` when the
   RC network was named and the wallet's suffix table did not, the exact t11 incident replayed. The
   behaviour-comparison test written after t11 caught it — on its FIRST workspace run, because
   `cargo test` used to die at this crate before ever reaching the wallet's turn.
4. ~~**Still open, serial mode only**~~ — **DOES NOT REPRODUCE (measured 2026-08-22, two runs).**
   The claim was that a single-threaded run of the WHOLE crate spins at 99% CPU inside
   `daemon_cleaning_test` (the upstream shutdown-refcount assertion) while the daemon tests pass
   3/3 in isolation — something an earlier test keeping a reference alive.

   The documented reproduction was run twice, unmodified, at `4e6d08c6`: **33 passed / 0 failed /
   18 ignored in 859 s**, and again in **776 s** with the same counts. Re-run a third time at
   `b14b6a9e`, after the Decision F projection and the producer's class-resolution rewiring had
   both landed — the two changes most likely to have disturbed daemon startup — with the same
   result: **33 / 0 / 18 in 803 s**, `EXIT=0`. Three independent runs across three tree states. `daemon_cleaning_test`
   reports `ok` in both, and the suite exits on its own rather than being killed — a watcher
   sampling the harness every 15 s recorded CPU tracking the consensus tests' own work and the
   log line advancing throughout, never a stall.

   Recorded as *not reproducible* rather than *fixed*, because nothing here was aimed at it: the
   likely cause is that items 1–3 changed which tests run and in what order (the crate used to
   die before reaching this point either way, which is why the entry could never say whether the
   spin predated the session). If it returns, the thing to capture is a `sample` of the harness
   during the stall — the watcher script is the shape of it — because the refcount holder is an
   earlier test's leak and only a stack says which.

Reproduction:

```
MISAKA_PALW_POW_FIXTURE=1 cargo test -p kaspa-testing-integration --lib -- --test-threads=1
```

The fixture variable is the model-free devnet PALW path; both requirements predate this session.

---

## Known-open and deliberately scoped out

> **Update 2026-08-22: the first three entries below are CLOSED**, each with the mutation
> measurement that says so. Kept in place because each records what the defect *was*.

* ~~**The decode-call embedding gather is `Unadjudicable`**~~ — **CLOSED (the integer-leg
  dispatch).** The decode check now dispatches on the class's registered `PalwStepLaneV1`: an
  `Int32` class authenticates its generated ids by recomputing `base0_logits_trace_root_v1` from
  a carried pin (rows + ids), a `Float32` class through the v2 event-tree root as before, and a
  pin that does not speak the class's lane is refused by name. The sweep now demands **914/914
  adjudicated, 0 unadjudicable**; reverting the dispatch arm reddens it at leaf 542 by name.
  The retained-material codec grew the logits rows in the same change (one codec — retention
  file, broadcast, seat decode), so a third party can assemble the pin from broadcast material,
  and `base0_material_matches_claim_v1` now *recomputes* the integer trace root from the carried
  rows rather than comparing binding fields.
* ~~**ADR-0049 Decision E's selection rule does not exist**~~ — **CLOSED (integer-first).**
  `base0_decode_token_select_v1` (argmax, ties LOWEST) is one function the engine's decode loop
  and the court both call; `PalwCourtVerdictProofV2::DecodeToken` refutes a committed decode
  token on-chain with fault `DecodeTokenMismatch { position }` (evidence kind 6), cost-gated by
  the same opening-byte ceiling. Money path proven: the lying claim voids as `CourtFraud` and its
  bond is slashed; the honest one survives the same close
  (`palw_v2_a_lying_decode_token_convicts_through_the_court_close`). Re-tying the rule to the
  highest index reddens the tie test. A `Float32` class remains refused by name — its
  per-position openings arrive with the class that needs them (Gate 3, not a launch blocker).
* ~~**M-02**~~ — **CLOSED (`364fe079`), and it was sharper than documentation debt**: the drift
  was SIX items (the five named plus `retired_safe_weight`), and `retired_safe_weight` had
  entered the preimage **without the version bump** ADR-0043's own change rule demands. Settled
  as: ADR-0043 §2 amended to the full list with provenance, `PALW_STATE_V2_VERSION = 5`
  supplying the missed bump, and the correspondence made executable — a from-spec second
  implementation (`the_state_root_preimage_is_exactly_the_adr_0043_list`, with an exhaustive
  no-rest destructure so a new carriage field fails to compile beside the list it must join),
  a 20-field perturbation census (`every_primary_datum_moves_the_root`), and golden vectors.
  Settled before the ruleset id froze, as required. The ruleset id itself (H of the params
  bundle) does not move with the bump; what moves is every `palw_state_root`, which is a
  consensus change the forced t12 genesis re-mint absorbs.
* **Qwen (H-04 / H-05 / H-06 / M-03)** — not a launch blocker: testnet-12 registers only BASE-0.

---

## Reading order for the next session

1. **The data-availability transport** (above). Everything else on this page is closed; this is what
   stands between the code and a chain whose claims mature.
2. **The wallet decision** a validator node needs in order to submit a receipt at all.
3. **M-02** — settle `state_root()`'s five items against ADR-0043's preimage **before** the ruleset
   id is frozen. The id already moved once this session (retirement changed the state), so a t12
   genesis must be re-minted regardless; doing M-02 first makes that one flag day instead of two.
