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
