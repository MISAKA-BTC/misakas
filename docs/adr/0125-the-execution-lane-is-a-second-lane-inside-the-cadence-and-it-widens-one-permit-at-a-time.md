# ADR-0125 — The execution lane is a second lane inside the cadence, and it widens one permit at a time

* Status: **IMPLEMENTED 2026-09-17, dormant** on `feat/palw-exec-lane-and-validator-retirement`
  (from `main` at `6fdf6ba7`, after ADR-0124), including the stage table (§7.2), equivocation
  evidence and relay de-duplication (§7.3), the RPC read and the operator's status line (§7.4), and
  the devnet drill (§7.1). `Params::palw_execution_lane` is `None` on every shipped preset, so every
  fingerprint, every root and every block of testnet-11 is what it was; the lane runs where a
  network arms it. What remains is the operator's: testnet-11's height (§7).
* Operator's request: "testnet では BPS 1 から実装して最終目標を BPS 10 とする", with the design
  quoted in §1.
* Builds on: [0060](0060-the-liveness-doctrine.md) / [0066](0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)
  (a lane with its own algorithm id, a constant target that never touches `bits`, zero subsidy),
  [0072](0072-the-ticket-is-the-execution.md) (an attempt's execution commitment is a value nobody
  re-rolls for free), [0075](0075-certification-is-a-consensus-object.md) (a certified family is chain
  state), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md) (what a second lane does to a
  bonded block at `ghostdag_k = 1`), [0107](0107-a-share-grows-on-work-that-reached-final.md) (`Final`
  is what is counted), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md).
* Amends nothing: no DAG parameter, no PALW window and no cadence moves (§8 says why an earlier draft
  thought they had to). Supersedes nothing.

## 0. The sentence this ADR is

**Transactions get a fast lane without the PALW chain getting a fast clock. Every second is a round;
a round's permits go to bonds whose attempts reached `Final` in the previous scheduler span, in
quotas proportional to those credits and capped at 45 % per security domain, one permit an operator,
no domain in two consecutive rounds. A permit is a round block: a light, fee-only block that is never
a selected parent, never blue and never counted in the DAA score, so the chain's GHOSTDAG, windows,
retargets and depths are exactly those of the DAG without it. A merging chain block accepts a round
block's transactions when its parent state grants the permit, and pays the fees to the bond's
payout. 1 BPS is one permit a round; 10 BPS is ten.**

## 1. What the operator asked, in the operator's words

> 120 秒の PALW Anchor あたり 1200 個の Execution Slot にすれば … PALW のユーザー向け実行レイヤーを
> 10 BPS まで持っていけます。PALW 推論そのものを 10 BPS で実行するわけではありません。
>
> 1200 blocks すべてに「新しい PALW work」を付けない。… execution block 自体は 追加 PWU = 0、追加 PALW
> reward = 0。
>
> 10 BPS なら「10 本 × 1 BPS」の発想が良い。… slot interval を 1 秒 → 100 ms へ縮めるのではなく、1 秒
> round の並列幅を 1 → 10 へ増やす。
>
> 同一 security domain 連続禁止 … model_id ではなく security_domain_id で判定。同一 operator 最大 1/10。
>
> claim E の Credit = epoch E+1 から使用。
>
> testnet では BPS 1 から実装して最終目標を BPS 10 とする。
>
> 現在の方式は beacon はもうないのでは

The design text proposed the DNS epoch beacon `R_E` as the seed. That overlay depends on validators
and is being retired; this lane reads no beacon (Decision 3).

## 2. What exists, and what refuses the obvious design

* **The 120-second cadence is frozen.** `validate_palw_v2` refuses a `ConsensusV2` params set whose
  `target_time_per_block`, `ghostdag_k`, mergeset limit, merge depth, coinbase maturity or sample rates
  are not the two-minute set. Depths are plain `u64`s in `BlockrateParams`, not `ForkedParam`s.
* **`ghostdag_k = 1`.** A block a second on the chain's own DAG would colour every bonded block red
  (ADR-0105's incident at eight heartbeats), advance the DAA score 120 times faster than the PALW
  windows assume, and put a stale attempt outside merge depth before it could be merged.
* **Block production has no beacon.** The attempt ticket is `H(execution_commitment_v3(attempt,
  execution_anchor))`; the ADR-0074 walk serves the free-prompt receipt lane only.
* **A header hash is not seed material.** A header's nonce re-rolls it for free.

So the lane's blocks cannot be chain blocks. They are blocks beside the chain.

## 3. Decisions

**Decision 1 — a round block is a sidecar: algo 10, never a selected parent, always red, outside
the DAA set.** `POW_ALGO_ID_PALW_ROUND_V1 = 10` is admitted only past `palw_execution_lane`'s
activation, ORed into the header gate as the heartbeat is. Its target is the constant `2⁻¹⁶`
(`PALW_ROUND_WORK_LOG2`), its tag the hash lanes' tag, its level zero, its blue work zero, and it is not
a priced difficulty row. GHOSTDAG never selects it and colours it red before any k-cluster walk
(`GhostdagManager::is_round_block`); the window manager treats it as outside the DAA set whatever its
blue score, so it is neither counted in a DAA score nor sampled into a difficulty or median-time
window. Its coinbase declares zero subsidy and carries no outputs; its `palw_state_root` is zero (the
header gate requires it). `mergeset_size_limit` counts only the chain's own blocks; round blocks have
their own bound (Decision 6). It is never a sink candidate, never virtual's selected parent and never
the headers' selected tip.

**Decision 2 — a round block hangs from an anchor on the chain.** A block that is not a round block
names at least one parent that is not one, and its selected parent is the heaviest of those. A round
block names at most one: that one is its anchor (its selected parent) when it moves the lane onto a
newer chain block; with none, it extends the lane from its round parents and its selected parent is
the heaviest of their anchors. After GHOSTDAG every round parent's anchor must lie on the block's
selected chain (`check_round_lane_mergeset`).

The consequence is the property the lane rests on: **a round block's past brings no chain block into
any mergeset that its anchor's past had not already brought.** By induction a round block's chain
ancestors are its anchor's past, and the anchor is a chain ancestor of every block that names the
round block; so a chain block's mergeset restricted to chain blocks, its reachability among chain
blocks, its blue score, blue work, DAA score, windows, merge-depth root and finality point are those
of the same block in the DAG with every round block removed. A producer anchors at the sink's
selected parent, which the next chain block can name beside the round tips.

**Decision 3 — a round is a second, and its permits come from the previous span's finalized
attempts.** `round(t) = (t − genesis_timestamp) / 1000`; a span is `schedule_span_daa` of DAA.
When an attempt claim reaches `Final`, the fold records `(domain, bond, operator, claim id, execution
root)` in the span of the finalizing block (`record_round_final`). At the first block of a later span
those finals become the next span's schedule and leave (`rotate_round_lane`,
`palw_execution_schedule_v1`):

* the seed is `H(span ‖ count ‖ (claim id ‖ execution root)*)` over the finals in claim-id order —
  changing it costs an inference that reaches `Final`; no beacon and no header enters it;
* a domain's credits are its finals, its quota is its share capped at `450 ‰`, water-filled in exact
  integers (`palw_execution_quotas_v1`: one domain holds the lane, two split it evenly, three or more
  are capped and re-divided), listing at most 32 domains and 64 bonds a domain;
* the domains are split into two parity groups as evenly as a greedy split makes them
  (`palw_execution_parities_v1`); a domain holds permits only in rounds of its parity, so no domain
  holds permits in two consecutive rounds at any width.

A round's permits are drawn from the schedule alone (`palw_execution_permits_v1`): for each index,
a domain of the round's parity with room under `⌈width / 3⌉` is picked in proportion to its quota by
`H(seed ‖ round ‖ index)`, and within it the bond with the lowest `H(seed ‖ round ‖ bond)` among those
whose operator holds no permit yet. Credits earned in span `s` are spendable only in `s + 1`.

**Decision 4 — the permit travels signed in the header.** A round block's `palw_commitment` is a
`PXR1` envelope `{ version, network domain, round, permit index, bond, pubkey, signature }`. The
header stage checks its shape, that its round is the header timestamp's, and the ML-DSA-87 signature
over `H(network ‖ pre-PoW hash ‖ timestamp ‖ nonce ‖ round ‖ index ‖ bond)` under the carried key
(`palw_carriage_stateless_v1`). The nonce is signed: the envelope is inside the block identity and
outside the PoW pre-image, so a signature that did not cover it could be re-announced under any
re-solved nonce by anyone.

**Decision 5 — the merging block decides the permit, from its parent state.** For each round block in
its mergeset, a chain block grants the permit when (`palw_round_verdicts_v1`):

* the anchor's span is the merging block's span or the one before (the two the fold keeps);
* that span's schedule grants `(round, permit index)` to the envelope's bond at the lane's width;
* the bond is registered, `Active`, and its key is the envelope's;
* the round block's coinbase names the bond's registered payout;
* the permit is not already in the ledger.

A round block without its permit is merged and coloured like any red, and none of its transactions is
accepted. The permits a block grants are recorded by its fold (`record_round_permits`) under
`(span, round) → bit per index`; the ledger and the schedules older than the span before the block's
are dropped at the span boundary. The template, the virtual state and every validating node read
one parent state and give one answer.

**Decision 6 — the lane's mergeset rule and its fees.** A mergeset holds at most `max_per_mergeset`
round blocks, at most `permits_per_round` of one round, one per `(round, index)`, round blocks of at
most 64 bonds, and a round block merges only rounds older than its own
(`palw_execution_mergeset_rule_v1`). A merging block pays each permitted round block's accepted fees to
the script its coinbase names, aggregated per script in order of first appearance, after the red lump
— never to the merging miner, never through the ADR-0018 carve and never into a validator pool. The
coinbase output cap widens by the 64 payees where the lane is configured. The lane mints nothing.

**Decision 7 — widening is a stage table, and a span keeps the width it opens with.** The lane opens
at `permits_per_round` (1 on the first stage) and `widenings` lists up to nine later stages, each a
height and a wider round, up to 10 (`PALW_EXEC_MAX_PERMITS_PER_ROUND_V1`) — enough to go from 1 BPS to
10 one permit at a time. A span's width is the widest stage in force at the DAA score the span opens
with (`width_of_span`), so the draw, the permit index and the per-round mergeset bound read one number
for a whole span, and history validates at the width it was produced at. Stages only widen, so the
header's per-round bound — the width at the block's own span — bounds every span a mergeset reaches
back to; a template and virtual's parents use the width at their anchor, never wider than what the
block is judged by. Each widening's height is a fence (scheduled, normalised out of the identity with
its width, named to the fork-id gate as `palw_execution_lane_widening_1..9`); its width is reported in
the schedule id.

**Decision 8 — the chain merges the lane, and a node produces it.** Virtual offers round tips as
parents after the chain's own candidates, newest round first, keeping one parent slot for them, taking
a tip only while its anchor is on virtual's selected chain and the mergeset still passes both bounds
(`palw_add_round_parents`). `round_adapt_block_template` turns the mining manager's template into a
round block (parents per Decision 2, recomputed GHOSTDAG, DAA score, bits, median time and pruning
point, the round's timestamp, zero-subsidy coinbase to the payout, empty EVM payload).
`kaspad --palw-round-lane` (with `--palw-producer-key` and `--palw-producer-bond`) asks
`palw_round_view_v1` each second whether the bond holds an unused permit, and if it does builds,
solves, signs and submits the round block.

## 4. What is built

* `consensus/core/src/palw_execution_lane_v1.rs` — rounds, spans, domains, quotas, parities, the
  schedule, the draw, the envelope and its signing message, the mergeset rule, the producer's view;
  integer-only (a test holds it to that).
* `pow_layer0.rs` / `consensus/pow` — algo 10, its constant target, its tag arm, its predicates.
* `config/params.rs` — `PalwExecutionLaneV1 { activation, permits_per_round, max_per_mergeset,
  schedule_span_daa }` at every fence site; `validate_palw_v2` refuses an unrunnable shape.
* GHOSTDAG, windows and DAA, the header processor (gate, envelope, parent shape, anchor rule,
  mergeset rule, selected tip), the body processor (zero subsidy), the PALW state (finals, schedules,
  ledger — rooted and carried only once written, so no state version moves), the virtual processor
  (verdicts, skipped transactions, payouts, sink search, virtual parents, round view and template),
  the consensus API and session, `kaspad`'s round producer.
* §7.2: `PalwExecWideningV1` and the stage table in `config/params.rs` (validation, hashing, fence
  visitor, fence names, fork-id and ruleset-candidate probes); every width reader in the header, the
  verdict, virtual's parents, the round template and the round view.
* §7.3: `PalwExecEquivocationV1` (`palw_execution_lane_v1.rs`), the object and its acceptance and fold
  (`round_equivocations`, delta 48, carriage tail `0xA8`), the verdict's refusal of a burned permit,
  `protocol/flows/src/palw_round_relay.rs` (announce one block a permit, queue the evidence), the
  panel's filing, and the round producer's persist-before-sign record.
* §7.4: `getPalwRoundLane` (op 181; wRPC and gRPC; `PalwExecLaneStatusV1` through the consensus API),
  `misaka palw round-lane`, the lane line in `misaka mining status`, and one chain-walk log line per
  chain block that merges the lane.
* §7.1: `kaspad --palw-execution-lane-devnet=activation,width,span[,daa:width…]` and
  `scripts/misaka-palw-round-lane-devnet-drill.sh`.
* Tests: the rules module (including the evidence's own proofs); the state (`adr0125_*` in
  `palw_state_v2.rs`: a finalized attempt schedules the next span, a permit is accepted once, spans
  are dropped, the delta and the carriage round-trip, nothing is written below the fence, a permit
  signed twice burns once and slashes the floor, a reorg across a span boundary reverts the schedule
  and the ledger exactly); the stage table (`palw_execution_lane_stage_tests`); the pipeline
  (`adr0125_*` in the virtual processor's tests: round blocks hang beside the chain and never move it;
  a chain block naming only round blocks, a permit twice and a forged envelope are refused; a merging
  block grants exactly the permits its parent state schedules; a widening holds for whole spans; two
  real signed round blocks for one permit are evidence the chain accepts once); the coinbase (payouts
  aggregate after the lump and fund no pool); the relay memory; the producer's round record; the CLI's
  lane line; the devnet flag's parser.

## 5. Security amendments

* **SA-1 — the schedule costs work to move.** Its seed and its credits are finalized attempts; a
  fabricated one is a claim the panel and the court void and slash.
* **SA-2 — a permit holder can equivocate, only on its own permit, and it costs the floor.** ML-DSA
  signatures are randomised and the envelope is outside the PoW pre-image, so a holder can publish
  several blocks for one permit; a stranger cannot sign one. One mergeset accepts at most one of them
  and the ledger accepts the permit once. A node announces only the first block it holds for a
  `(round, index, bond)` (`PalwRoundRelayV1`); a second, different signed block is stored, not
  announced, and becomes evidence a funded panel files as `RoundPermitEquivocated` (object tag 46).
  The chain admits it when the named span's schedule grants that permit to that bond and both
  signatures verify under the bond's REGISTERED key; the fold then burns the permit (no merging block
  grants it again) and slashes the registry's collateral floor, once per permit, within the two spans
  the ledger keeps. A producer records each round before signing it, so a restart cannot sign one
  permit twice.
* **SA-3 — the anchor rule is what makes the lane invisible to the chain** (Decision 2). Without it a
  round block could carry a chain block into a mergeset through a block that is never a selected
  parent.
* **SA-4 — one domain halves the lane.** With one live domain every other round is empty: the parity
  rule doing its job.
* **SA-5 — a permit is earned per finalized attempt.** Only bonds with a `Final` attempt in the span
  are listed; `N` permits in one round need `N` operators.
* **SA-6 — a round block is never a chain block,** so its UTXO commitment, accepted-id root, overlay
  root and EVM commitment are never read; the template writes zeros and an empty EVM payload.
* **SA-7 — integers only.**
* **SA-8 — the attempt lane's domain is the class unless a free-prompt family certifies it.** The
  attempt lane keeps no per-class family row, so a class certified only by genesis is its own domain.
* **SA-9 — no validator is involved.** Nothing in the lane reads a beacon, a bond view of the DNS
  overlay or a validator pool, and its fees are excluded from the ADR-0018 carve.

## 6. What does not change

The attempt lane, its targets, budgets and subsidy; the receipt lane; the heartbeat; fork choice;
every DAG parameter, depth and PALW window; every network until its fence is armed.

## 7. What remains

1. **Built** — the devnet drill (`scripts/misaka-palw-round-lane-devnet-drill.sh`, four nodes: an
   attempt reaches `Final`, the next span is scheduled, round blocks are produced for held permits,
   chain blocks grant them, a granted round block carries fee-paying transactions). A reorg across a
   span boundary is pinned at the state layer (`adr0125_a_reorg_across_a_span_boundary_…`); the drill
   does not partition its network. The run's result is recorded in §9.
2. **Built** — the stage table (Decision 7).
3. **Built** — relay de-duplication and the equivocation slash (SA-2).
4. **Built** — `getPalwRoundLane`, `misaka palw round-lane` and `misaka mining status`'s lane line.
   An explorer reads op 181; misakascan lives in its own repository.
5. **The operator's** — testnet-11's height and its first stage table, at a NEW height with a roll
   window. Nothing is scheduled by this branch.

## 8. Corrections

* **The first draft seeded rounds from the ADR-0074 beacon.** Main's block production reads no
  beacon; the operator said so. Replaced by finalized attempts.
* **The second draft moved the DAG's parameters** — `ForkedParam` for `ghostdag_k`, the mergeset limit
  and the depths, and every PALW window scaled 120× — so round blocks could be chain blocks. Replaced:
  round blocks are outside the chain, so nothing the chain reads moves.
* **A round block first had to name exactly one chain parent.** Two round blocks of one anchor could
  then never be parent and child — naming the anchor beside a round tip that already hangs from it
  names an ancestor of a parent. A round block may now extend the lane from round parents alone.
* **The permit ledger first dropped rounds below the newest merged round.** A holder of a future
  round's permit could then publish early and make every honest round block of the rounds between
  stale. The ledger is keyed by `(span, round)` and kept for two spans instead.
