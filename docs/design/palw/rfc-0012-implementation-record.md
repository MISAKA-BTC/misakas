# RFC-0012 implementation record — requirement → code → test → status

Lane X12 · branch `rfc12/x12-dns-retirement` (base `af6e95453`) · 2026-10-08.
Policy numbers, latency, attacks and the go / no-go list: [policy proposal](rfc-0012-policy-proposal.md).
Spec: [RFC-0012](../../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) · dormant notes: [0012-dormant-implementation](../../rfc/0012-dormant-implementation.md).

Status vocabulary is the matrix's: **IMPLEMENTED_AND_TESTED** (real node path + tests) · **IMPLEMENTED_REFERENCE_ONLY** · **DORMANT_NOT_INTEGRATED**
· **CODE_GAP** · **DESIGN_GAP** · **EXTERNAL_GATE_PENDING** · **DEFERRED_BY_USER** · and **EXTERNAL** (client work outside this repository).
`palw_dns_retirement_v1` is `None` on every preset; no flag-day list contains it; nothing here changes a shipped preset. "In-process" means
`TestConsensus` nodes in one process, not a deployed network. A passing fixture is not production completeness.

## 0. What this lane changed in the code (and why)

| Change | Files | Why |
|---|---|---|
| Native evidence is read from each chain block's PALW **delta**, not from the claims the sink still holds | `consensus/core/src/palw_native_settlement_v1.rs` (`native_delta_evidence_v1`, `native_facts_of_block_v1`), `consensus/src/pipeline/virtual_processor/native_settlement.rs` | the old reader was blind to ordinary claims: retired at `Final + 3,000`, retention lapsing at `accepted + 5,400` (policy proposal §0) |
| Per-block row cache (memory only, 352 B/row) + O(N + F) prefix sweep + one-pass lifecycle closure | same + `certify_native_prefix_v1`, `native_open_from_v1` | the snapshot walked the whole chain with a DB read per block per virtual change and was O(chain × claims) |
| `latest` survives a failed certificate; genesis ends the executed chain | `native_settlement.rs` | `latest` is a fact about execution; a pruning-point-at-genesis chain read as `MissingHistory` forever |
| `dns_coinbase_settlement` ignores the frozen DNS anchor once the virtual is retired | `processor.rs` | node mempool vs wallet disagreement (mempool policy only) |
| `misaka palw settlement`: `FinalizedConflict` is an alarm line and an error exit | `misaka-cli/src/palw_settlement.rs` | a script waiting on `--min-depth` must not wait on a node that cannot settle |
| Explorer renders `finalizedConflict` as a safety alarm naming the resync | `contrib/misakascan-t12/app.js` | not an ordinary stop reason |
| Wallet: `dns_shortcut_after_poll` | `wallet/core/src/utxo/processor.rs` | the "clear an unavailable DNS shortcut" rule as a tested function |

Duplicate work still stops certification (`DuplicateWork`) and now takes precedence over overflow independent of fact order; the old
function returned whichever it met first. Unreachable today (the fold refuses duplicates).

## 1. Requirement table

| # | RFC requirement | Code | Test | Status |
|---|---|---|---|---|
| 1 | No DNS veto in post-fence selection (§1 row 1, §2.1, §8 row 2) | `processor.rs::dns_reorg_outcome` (`dns_retired_at(incumbent)`), `dns_bft.rs::dns_bft_gate_refusal`, `dns_stake_preferred_tip` | `tests.rs::rfc0012_retirement_uses_incumbent…`; **x2** (veto `HardCheckpointReject` below the fence, abstains at/after; two nodes, anchor planted vs none → same sink + snapshot) | IMPLEMENTED_AND_TESTED. The DNS state is not an input past the fence; an *all-equivocating-votes* case was not crafted |
| 2 | PALW comparator unchanged and sufficient for convergence (§2.2, §8 row 3) | unchanged `palw_candidate_order_v2` / `decide_deep_reorg_v2` | **x3** sibling race, tie, deep fork, 5 arrival orders, healed partitions, replay | IMPLEMENTED_AND_TESTED for identical-history convergence. **Not** a proof of common-prefix security (RFC §9) |
| 3 | `latest` / `safe` / `finalized` from PALW evidence only; explicit absence (§4.2, §3) | `native_settlement.rs`, `update_evm_canonical_heads`, `palw_native_settlement_v1.rs` | x1, x5, x8, x9, x10; core: sweep == reference (20,000 random instances), `open_from` == per-effect (5,000), `rfc0012_native_evidence_fold` | IMPLEMENTED_AND_TESTED in-process. **GAP:** no claim reaches `Final` through the processor with the EVM lane on (§3) |
| 4 | Evidence survives claim retirement; loss stops certification, never certifies wrongly (dormant doc "Activation work") | delta extraction; `MissingHistory` on a missing/undecodable delta; pruning point's delta never read | `rfc0012_evidence_window` (parameter arithmetic), `rfc0012_native_evidence_fold` (real fold: accepted 1001 / Final 1124 / retired 4125 / retention 6401), **x9** (lost delta → `MissingHistory`, restored → same certificate) | IMPLEMENTED_AND_TESTED |
| 5 | Conviction after `Final` retracts counted work | `voided` set over the chain's deltas | fold test (`PanelFalseValid` through the real fold voids the Final; extraction reads it; the fact is gone) | IMPLEMENTED_AND_TESTED |
| 6 | Bounded cost, incremental index (dormant doc) | row cache + sweep | `rfc0012_fork_id_and_cost` (83 ms @160k effects; reference 30× per 4×), `native_settlement::tests` (352 B/row), 542 ns decode+extract per real delta | IMPLEMENTED_AND_TESTED (debug-build numbers; release/long-chain processor run is a GAP) |
| 7 | Alarm / resync for below-finalized conflict | sticky `FinalizedConflict`, `error!` log, import clears | **x7**; CLI alarm + exit; explorer alarm | code IMPLEMENTED_AND_TESTED; the page/pager and the runbook are **OPS** (§5) |
| 8 | Native deposit/withdraw/market, one executor, conserved (§4.1, §8 row 1) | unchanged `evm_execute_acceptance`, `apply_evm_bridge_effects` | **x1** (lock → unasked claim → sells → withdrawal across the fence; combined ledger moves by exactly `(coinbase − L1 fees) × scale` on every block; withdrawal makes exactly one UTXO; lock consumed once; both orders reach `MarketSettle`) | IMPLEMENTED_AND_TESTED. Orders settle `Refused MARKET_MISSING` (no seeded market → **no fill**, GAP) |
| 9 | Reorg before/after deposit claim, withdrawal, market settlement: root/state equal after undo/replay (§8 row 7) | unchanged | **x14** double spend of a lock's funding across a reorg and the fence, both directions, 4 nodes + a merging block: one sink/snapshot/heads/ledger, exactly one spend of the float, credit exactly once iff the lock is the accepted spend | IMPLEMENTED_AND_TESTED for the lock → claim path; refund-boundary and withdrawal reorg are covered by the existing EVM tests, not re-run across the fence (GAP) |
| 10 | No DNS freshness gate in templates / claim RPC / wallet (§1 row 4, §7) | `bridge_finality_is_fresh`, `submit_evm_deposit_claim` guard | x1 (the node's own heartbeat carries the claim), **x13** (every DNS reader answers none; mempool ignores a frozen anchor) | IMPLEMENTED_AND_TESTED; the `Pause` policy path is code-read only (GAP-lite) |
| 11 | Fee / subsidy schedule, no extra issuance, retained liabilities (§6, §8 row 8) | `fee_split_at`, `palw_escrow_*_at`, `native_base_withheld_at`, `native_inclusion_ratio` | **x12**: on one script fenced vs unfenced the merging coinbase mints no subsidy; a legacy claim and a post-fence floor claim both record 320,084,650,080 of 444,562,014,000; withheld base 408,997,052,880; 88,912,402,800 never minted; `S = escrow + unminted + pool` exact; + `tests.rs` unit | IMPLEMENTED_AND_TESTED for the recovery floor. **GAP:** a REAL-class claim's 92 % escrow was asserted only at the carve reader (unit), not through a processor chain |
| 12 | Retired kinds refused in header, UTXO, template (§5) | `check_dns_retirement`, `classify_attestation_shard_for_template`, `utxo_validation.rs` | unit; **x11** (template refusal by name), **x15** (a hostile block carrying a bond past the fence → `TxInContextFailed(DnsParticipationRetired)`) | IMPLEMENTED_AND_TESTED |
| 13 | Historical exits and evidence survive (§5) | unchanged bond store; `legacy_dns_evidence_allowed_v1` | **x11** (a bond created below the fence exits by an owner-signed request after it: `Unbonding`, not slashed); unit for the evidence horizon | exits IMPLEMENTED_AND_TESTED; the evidence horizon is unit-only (GAP: no signed equivocation through the pipeline) |
| 14 | Restart: same heads / backing as full replay (§7, §8 row 9) | store rows + rebuildable cache | **x5** | IMPLEMENTED_AND_TESTED |
| 15 | Pruned IBD / legacy snapshot migration (§7, §8 row 9) | `import_pruning_point_evm_state` retired branch | **x7** calls the import directly (root-verified), then reconstruction | import path IMPLEMENTED_AND_TESTED; a real pruned join with EVM state is **not reachable in-process** → needs the drill (EXTERNAL_GATE_PENDING) |
| 16 | Old-version nodes diverge, do not participate (§8.5) | fork-id gate; consensus rules | **x6** (old rules accept the first two blocks at or past the fence and disqualify the third in that script: `StatusDisqualifiedFromChain`), `rfc0012_fork_id_and_cost` (refusal from the height; different height / policy value is a different network) | IMPLEMENTED_AND_TESTED |
| 17 | Versioned settlement status RPC; v1 bytes unchanged when absent (§7) | `message.rs` v1/v2, gRPC, wRPC | `rpc/core/src/model/tests.rs` (5 incl. pre-RFC JSON with an unknown key, either-field-selects-v2, stable stop names) | IMPLEMENTED_AND_TESTED |
| 18 | Wallet clears an unavailable DNS shortcut | `dns_shortcut_after_poll` | `settings.rs::rfc0012_an_unavailable_dns_confirmation_clears…` | IMPLEMENTED_AND_TESTED |
| 19 | CLI / explorer | `palw_settlement.rs`, `app.js` | CLI `rfc0012_a_finalized_conflict_is_an_alarm…`; `contrib/misakascan-t12/tests/native-settlement.cjs` | IMPLEMENTED_AND_TESTED |
| 20 | Third-party wallets / SDKs / EVM tooling / other explorers; the TypeScript SDK's own tests | — | — | **EXTERNAL** |
| 21 | Release gate 4: adversarial multi-node drill with zero DNS validators | — | in-process only, above | EXTERNAL_GATE_PENDING |
| 22 | Release gate 5: schedule, fingerprint, compatibility matrix, bond-exit procedure, release notes | — | — | DEFERRED_BY_USER (user decides arming after the evidence) |

## 2. Removal audit (RFC §8 last row): every live DNS reader, and what it does past the fence

| Reader | Past the fence | Evidence |
|---|---|---|
| `update_dns_state` (recompute) | skipped; the row is frozen | x1 (row identical on every retired block) |
| `dns_bft_gate_refusal`, BFT confirmed anchor | `None` / not computed | x2, unit |
| `dns_stake_preferred_tip` | `None` (incumbent retired) | code (guard first); no dedicated test |
| `dns_reorg_outcome` stake-score arm | PALW comparator only | x2 |
| `update_evm_canonical_heads` `safe` from `last_dns_confirmed_anchor` | replaced by the native snapshot | x1 (heads) |
| `bridge_finality_is_fresh` | `true` | x1 |
| `dns_coinbase_settlement` (mempool policy) | anchor `None` | x13 |
| `get_dns_confirmation`, `get_active_validator_set`, `get_attestation_quality_deficits`, `get_validator_attestation_targets` | none / empty | x13 |
| `getDnsConfirmation` / `getPrecommitDuty` / `getValidatorStatus` RPC | retired role reported | rpc tests (wire); handler code-read |
| validator service, standalone validator | stop signing / funding | code (`validator_service.rs`, `kaspa-pq-validator`); no process test |
| bond store, unbond / slashing paths, deferred quality liabilities | **kept** (historical exits, pre-fence liabilities) | x11 |

## 3. What the harness can and cannot reach (stated once)

The EVM lane executes a block against its header timestamp, so a template cannot be re-stamped on an EVM-active network: the in-process
clock is the host clock, testnet-12's DAA stops at one tick plus one stamped into its slot (DAA 1 here), and **no claim can reach `Final`**
(`Final` needs ≥ 123 DAA). So:

* Evidence *extraction* is proven where `Final`s exist — the pure fold, on testnet-12's own `Params`, with deltas from
  `apply_palw_transition_v7` (`rfc0012_native_evidence_fold`).
* The processor's *use* of evidence is proven with a `cfg(test)` seam (`native_fact_override`: facts placed at chain blocks) and a frontier
  planted **consistently** (tip and the sink's delta row, so every later header commits to it and a reorg reverts it) — x8, x9, x10. The
  seam is named in the file; it bypasses extraction, which the fold test covers.
* A REAL claim's journey through the processor with the EVM lane on is therefore **not exercised**: a **GAP** for the drill.
* Markets: the model line has no seeded market, so both sells settle `Refused MARKET_MISSING`; fills need the Position route.
* Real pruned IBD: not reachable; the import function is called directly (x7).

## 4. Reproduce

```
export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0
cargo test --offline -p kaspa-consensus-core --lib palw_native_settlement
cargo test --offline -p kaspa-consensus-core --test rfc0012_evidence_window --test rfc0012_native_evidence_fold --test rfc0012_fork_id_and_cost -- --nocapture
cargo test --offline -p kaspa-consensus --lib --features evm rfc12_x      # the matrix: x0 control, x1-x3, x5-x15
cargo test --offline -p kaspa-consensus --lib --features evm rfc0012      # the pre-existing dormant tests + the row-size test
cargo test --offline -p kaspa-rpc-core --lib rfc0012
cargo test --offline -p kaspa-wallet-core --lib rfc0012
RUST_MIN_STACK=33554432 cargo test --offline -p misaka-cli palw_settlement
node contrib/misakascan-t12/tests/native-settlement.cjs
```

Pre-existing and unrelated: `misaka-cli`'s `cli_surface_tests::palw_settlement_parses_and_says_the_depth_is_anchors` overflows a default 2 MiB
debug test-thread stack on this tree (clap's recursive help) and passes with `RUST_MIN_STACK=32M`; it does so with and without this branch.
The consensus test binary needs `--features evm` to run the EVM-lane tests (and `cfg(test)` code of this lane).

## 5. Operations (OPS — not code)

* **Alarm.** `FinalizedConflict` is: an `ERROR` log `[native-settlement] FINALIZED CONFLICT: <sink> abandons <head>; resync required`; `stop =
  finalizedConflict` in `getPalwSettlement.nativeSettlement` and `native_settlement` JSON; the CLI alarm line and non-zero exit; the explorer
  alarm; `latest` still reported, `safe` / `finalized` withheld. A pager rule on the log line and on the `finalizedConflict` string is **OPS**.
* **Resync.** Stop the node; wipe its consensus data dir; start with the pruned-IBD path so `import_pruning_point_evm_state` runs (it clears the
  sticky conflict and the native snapshot; the next virtual change rebuilds the snapshot from the chain). Do not edit the stores by hand.
* **A conflict is also an incident**: it means a branch past the node's own published finality was selected. Capture the sink and the abandoned
  head from the log and escalate; it is a consensus-security event, not a node-health one.
* **Before the height**: confirm every seat's build carries the new fence (fork-id), the DNS validator service is stopped or will self-retire,
  and bridge operators have the `null`-tag note (policy proposal §8).

## 6. D1 follow-up — the maturity offset and the `safe` explanation (lane X12b, branch `rfc12/x12-safe-maturity`, 2026-10-08)

The user's position on D1: do not shorten 5,400 now, do not treat it as final, RFC-0012 is outside DAA 9,000; the user wants evidence. Evidence and
recommendations-free analysis: [policy proposal §10](rfc-0012-policy-proposal.md). This section is requirement → code → test → status for what the
follow-up added. **Nothing here changes a shipped preset, a params / schedule / identity id, or the dormant fence (`palw_dns_retirement_v1` is still
`None` everywhere and in no flag-day list).** v1 is still the only maturity rule in the tree.

### 6.1 What changed in the code

| Change | Files | Why |
|---|---|---|
| `palw_native_readiness_v1`: a **pure** explanation of the certificate (which effect holds `safe` back, every unmet condition, running clocks, named history gaps) | `consensus/core/src/palw_native_readiness_v1.rs` (new), `lib.rs` | item 4: the snapshot says one stop; a reader waiting on a transaction needs the reasons |
| `native_facts_and_skips_v1`: the one fact conversion, now also counting what it skipped (`voided`, `baseClass`, `openDa`, `unpriced`, `bondNotHeld`); `native_facts_of_block_v1` is it with the counters dropped | `palw_native_settlement_v1.rs` | evidence seen and not counted must be visible, not silent |
| `native_evaluate` returns the snapshot AND the material it was weighed from (or why nothing was); `native_evm_settlement_snapshot` is `native_evaluate(..).snapshot`; walk faults carry a cause and a block | `consensus/src/pipeline/virtual_processor/native_settlement.rs` | the explanation can never be about other evidence than the certificate; the snapshot path pays nothing for it |
| `native_safe_readiness(sink)`, memoized per sink; `ConsensusApi::get_native_safe_readiness` (default `Ok(None)`), `Consensus` impl, `async_get_native_safe_readiness` | `native_settlement.rs`, `processor.rs` (the memo field), `consensus/src/consensus/mod.rs`, `consensus/core/src/api/mod.rs`, `components/consensusmanager/src/session.rs` | one evaluation per virtual change, however often the RPC is called |
| `getPalwSettlement.nativeReadiness` (**no new op**): wRPC response v3 only when present, gRPC field 12, JSON/wasm key + TypeScript types; served only when its `generation` equals the snapshot's | `rpc/core/src/model/message.rs`, `rpc/core/src/wasm/message.rs`, `rpc/grpc/core/proto/rpc.proto`, `rpc/grpc/core/src/convert/message.rs`, `rpc/service/src/service.rs` | item 4 on the existing RFC-0012 RPC |
| `misaka palw settlement` prints the reasons | `misaka-cli/src/palw_settlement.rs` | operators read it in a terminal |
| The real-fold fixture moves to a shared module; the existing fold test is unchanged in behaviour | `consensus/core/tests/rfc0012_fold_fixture/mod.rs` (new), `rfc0012_native_evidence_fold.rs` | two test files drive the same claim life |

### 6.2 Requirement table

| # | D1 requirement | Code | Test | Status |
|---|---|---|---|---|
| D1-1 | Derive 5,400 by formula, with the enforcement point of every term | policy proposal 10.1 (reading) | measured pins: `rfc0012_native_evidence_fold` (accepted 1,001 → `Final` 1,124 → retired 4,125 → retention lapses 6,401), `rfc0012_evidence_window` | IMPLEMENTED (analysis; the formula is not a parameter) |
| D1-2 | Compare 5,400 / 3,000 / 1,000 / 600: attacks, EVM finality time, what else must change | 10.2, 10.3.4 | `d1_a` (exact instants), `d1_f` (lags against the node's 600-blue bound) | IMPLEMENTED (analysis + tests); hours are DERIVED from 24 and 30 DAA/h |
| D1-3a | Producer + every Panel seat colluding | `rfc0012_safe_maturity_attacks.rs` (rules exist only there) | `d1_d`: v1 and `Final + 3,000` never retracted; `Final + 1,000` / `+ 600` retract for 2,000 / 2,400 blocks of convictions; `d1_b`: the last reversing block is `Final + 3,000` | IMPLEMENTED_AND_TESTED at the fold level; a fork **race** is a **GAP** |
| D1-3b | Evidence / material withholding | same | `d1_c` (DA accusation admitted through `Final + 3,000`; an open session holds the claim past retirement; default as late as `Final + 4,201`), `d1_e` (no rule counts a claim under accusation); `rfc12_r3` (the node's own lost history is a named retention gap) | IMPLEMENTED_AND_TESTED; **corrects an inference in 10.2** (stated there) |
| D1-3c | A private fork released late | none new (fork choice unchanged) | existing `hb_probe_e` (322 s) and `hb_probe_d` (629 s) re-run: both PASS — the comparator refuses a branch lacking a `Final` claim from `Final` on, and only the 600-blue finality depth stops a heartbeat-only reorg; `rfc12_r5` (labels withdrawn, no conflict alarm) | IMPLEMENTED_AND_TESTED for a branch that lacks the claim; a branch carrying its OWN `Final` claim is a **GAP** (capacity security) |
| D1-3d | Deep reorg across the would-be `safe` point; what `safe` / `finalized` do per value | `native_evaluate` (labels are recomputed; the conflict alarm is only for abandoning the finalized block) | `rfc12_r5` (two worlds: mature / not yet), `d1_f` | IMPLEMENTED_AND_TESTED; the harness has no real clock (`matured_daa` against DAA 1 to 2) |
| D1-4 | `safe` readiness as structured reasons on the existing RFC-0012 RPC; ask before a new op | `palw_native_readiness_v1.rs`, `native_settlement.rs`, rpc files above | core 11 (agreement with the certificate on 400 generated chains, exact and tight promise, non-monotone evidence), processor `rfc12_r1`–`r4`, wRPC v3 byte-compat, gRPC round trip, CLI | IMPLEMENTED_AND_TESTED; **no new op was needed** |
| D1-5 | The explorer renders the reasons | — | — | **CODE_GAP** (small; `contrib/misakascan-t12` reads the snapshot only) |
| D1-6 | Third-party readers of `getPalwSettlement` handle wire v3 | — | — | **EXTERNAL** (v3 appears only past the fence) |

### 6.3 Harness limits that apply to this section (in addition to §3)

* The fold tests are linear (one block per DAA, `blue == DAA`), floor-relabelled, `D = 1`; see policy proposal 10.3.2 for the five named seams. The three
  alternative maturity rules are a function in the test file, not code in the tree.
* The processor tests (`rfc12_r*`) place facts and a frontier through the §3 seams; "maturity" there is a fact's `matured_daa` against a sink DAA of 1.

### 6.4 Findings that changed an earlier statement or that a reader must not miss

1. **The DA channel outlives the retirement.** `da_admission_v1` admits an accusation through the block at `Final + claim_retirement`, and an open session holds the
   claim in state: a default can land at `Final + 4,201` (§10.2 had said `Final + 3,000`; corrected). The `Final + 3,000` rule is safe against it only because an
   open session removes the fact and keeps the lifecycle open from the accusation on (`d1_e`).
2. **`Final + 3,000` has zero slack**: the last reversing block is the rule's own instant block (`d1_b`/`d1_d`).
3. **A conviction after the retirement never retracts work** under any rule (`late` in `d1_d`).
4. **A published `finalized` retreats with `safe`** (`rfc12_r5`); nothing latches it. On a real chain it is a pruning point at least 74,920 blue score behind.
5. v1's closing term is the retention (`acceptance + 5,400`), not the retirement; it is above every reversal horizon measured here, including `Final + 4,201`.

### 6.5 What still stands between the fence and arming

The user's course changed (no DAA-9,000 flag day; the next public release enables `palw_dns_retirement_v1` with RFC-0008 / 0010 / 0011-K2 / 0014 / 0015). The sorted list —
code items with a concrete next step each (C1–C11), the policy values with a recommended value and its trade-off, and external / ops — is
[policy proposal §11](rfc-0012-policy-proposal.md). The ones this record's tables mark **GAP** / **CODE_GAP** are C1 (a claim reaching `Final` through the processor with EVM on), C2 (a real pruned IBD with
EVM state), C3 (market fills), C4 (re-measure D1 with 0008 / 0010 / 0011-K2 / 0014 / 0015 armed), C8 (explorer).

### 6.6 Reproduce

See the commands at the end of policy proposal §10.3 (10.3.6). Pre-existing and unrelated: `misaka-cli`'s `cli_surface_tests::palw_settlement_parses_and_says_the_depth_is_anchors`
needs `RUST_MIN_STACK=33554432` (§4).

## 7. Wave 2 - the code items C1-C11 (lane X12c, branch `rfc12/x12-c-items`, 2026-10-08)

Policy proposal section 12 is the narrative; this is requirement -> code -> test -> status. **All of it is written and none of it has been compiled or run: the Lead put every
cargo build on hold (disk and RAM pressure) after the code was written.** The statuses below say `WRITTEN - NOT RUN` until the one batched invocation is made; nothing is claimed
that a run has not shown. No params / schedule / identity id moves.

| # | Requirement | Code | Test | Status |
|---|---|---|---|---|
| C1 | A claim reaches `Final` through the processor with the EVM lane on | `VirtualStateProcessor::template_clock` / `template_now` (`cfg(test)`); `T12Chain::arm_clock`; `stamp_harness_time` checks instead of re-stamping on an EVM-active network; `native_relabel_class` (`cfg(test)`) | `rfc12_c1_a` (floor claim; the node reads its delta: `skipped.baseClass == 1`), `rfc12_c1_b` (read as REAL: one pending fact, the exact instant and countdown), `rfc12_c1_c` (`#[ignore]`, ~5,400 DAA: `safe` flips at the v1 instant) | WRITTEN - NOT RUN |
| C2 | A pruned join with EVM state | none new (the import functions are the node's own) | `rfc12_c2` | WRITTEN - NOT RUN; partial: UTXO set, overlay snapshot and P2P not exercised |
| C3 | Market fills | none | `rfc12_c3` (80 MSK deposit, 50 MSK buy, two one-unit sells; ADR-0162 armed in a TEST copy) | WRITTEN - NOT RUN |
| C4 | Re-measure D1 with the other lanes armed | fixture: `ExtrasEdit`, `live_life_with`, `try_step_with`, `try_step_for` | `d1_g` (tripwire) | WRITTEN - NOT RUN; OPV / permissionless panel / kernel route / RFC-0008 slices / K2 classes are not armable in the fixture: **redo at integration** |
| C5 | The maturity rule v2 | - | - | SKIPPED (no D1 other than v1 chosen) |
| C6 | A class's own court window extends retention | `palw_native_settlement_v1.rs`: `live_retention` in `native_facts_and_skips_v1` | `d1_h` | WRITTEN - NOT RUN |
| C7 | Preset and release wiring | none (checklist only) | - | READY-TO-APPLY CHECKLIST (policy proposal 12.8) |
| C8 | Explorer renders `nativeReadiness` | `contrib/misakascan-t12/app.js` | `tests/native-settlement.cjs` | **PASS** |
| C9 | Release-build cost at 100k+ blocks | none | `rfc0012_c9_release_cost.rs` (two `#[ignore]`d tests) | DESIGNED, NOT RUN (by order) |
| C10 | DNS all-equivocating case | none | `rfc12_x16` | WRITTEN - NOT RUN |
| C11 | May a published `finalized` retreat | `note_finalized_withdrawal`, `FinalizedReadinessV1::withdrawn_from`, CLI, explorer, TypeScript | `rfc12_r5` (extended), core/rpc/gRPC/CLI/explorer fixtures | DECIDED (policy proposal 12.7), WRITTEN - NOT RUN |

### 7.1 The one batched invocation (to run on the Lead's go)

```
cd /Users/wata/Downloads/MISAKA-wt-b/wc-x12c && export CARGO_BUILD_JOBS=3 CARGO_INCREMENTAL=0 RUST_MIN_STACK=33554432
cargo test --offline -p kaspa-consensus-core -p kaspa-consensus -p kaspa-rpc-core -p kaspa-grpc-core -p misaka-cli --features kaspa-consensus/evm \
  --lib --bins --test rfc0012_safe_maturity_attacks --test rfc0012_native_evidence_fold --test rfc0012_c9_release_cost \
  -- --test-threads=2 rfc0012 rfc12_ palw_settlement GetPalwSettlement            > target/x12c-batch.log 2>&1
node contrib/misakascan-t12/tests/native-settlement.cjs
# once, on the release candidate:  cargo test ... rfc12_c1_c --ignored   (~5,400 DAA)   and the C9 measurement (release profile, idle fleet-class machine)
```
